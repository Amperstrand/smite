//! Library surface of the spec-derived seed pipeline.
//!
//! Exposes the deterministic BOLT requirement extraction and the
//! sketch-to-IR conversion so other crates (e.g. `smitebot`) can generate
//! seed corpora without shelling out to the `smite-requirements` binary.

pub mod bridge;
pub mod converter;
pub mod model;
pub mod parser;

use std::path::Path;

/// Outcome of [`emit_seed_dir`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EmitStats {
    /// Number of program sketches derived from the requirements.
    pub sketches: usize,
    /// Seed files written (sketches the converter could turn into programs).
    pub written: usize,
    /// Written seeds that survived a postcard encode/decode roundtrip.
    pub roundtrip_ok: usize,
}

/// Runs the full pipeline over a BOLT markdown file and writes postcard
/// seed files into `out_dir`.
///
/// Every written seed is roundtrip-verified; a mismatch fails the whole
/// run with an error naming the offending sketch.
///
/// # Errors
///
/// Returns a human-readable error string for I/O failures, parse
/// failures, or a seed roundtrip mismatch.
pub fn emit_seed_dir(bolt_md: &Path, out_dir: &Path) -> Result<EmitStats, String> {
    let text =
        std::fs::read_to_string(bolt_md).map_err(|e| format!("read {}: {e}", bolt_md.display()))?;
    let stem = bolt_md
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let reqs = parser::parse(&stem, &text).map_err(|e| e.to_string())?;
    let seeds = parser::seeds(&reqs);
    let sketches = bridge::sketches(&seeds, &reqs);

    std::fs::create_dir_all(out_dir).map_err(|e| format!("mkdir {}: {e}", out_dir.display()))?;
    let mut stats = EmitStats {
        sketches: sketches.len(),
        written: 0,
        roundtrip_ok: 0,
    };
    for sketch in &sketches {
        let Some(program) = converter::sketch_to_program(sketch) else {
            continue;
        };
        let bytes =
            postcard::to_allocvec(&program).map_err(|e| format!("serialize {}: {e}", sketch.id))?;
        let name = out_dir.join(format!("{}.seed", sketch.id.replace([':', '/'], "-")));
        std::fs::write(&name, &bytes).map_err(|e| format!("write {name:?}: {e}"))?;
        stats.written += 1;
        match postcard::from_bytes::<smite_ir::Program>(&bytes) {
            Ok(decoded) if decoded == program => stats.roundtrip_ok += 1,
            Ok(_) => return Err(format!("ROUNDTRIP MISMATCH: {}", sketch.id)),
            Err(e) => return Err(format!("ROUNDTRIP FAIL: {} — {e}", sketch.id)),
        }
    }
    Ok(stats)
}

#[cfg(test)]
mod tests {
    use super::*;

    // A minimal BOLT-02-shaped markdown covering one message; the same
    // shape the parser tests use, restricted to a converter-supported
    // message so the pipeline must emit at least one seed.
    const FIXTURE: &str = "\
### The `splice_init` Message

1. type: 80 (`splice_init`)

#### Requirements

The sending node:
  - MUST NOT send `splice_init` if the channel is not quiescent.
  - MUST set `funding_feerate_perkw` to the feerate for the splice transaction.
";

    #[test]
    fn emit_seed_dir_writes_and_roundtrips() {
        let dir = std::env::temp_dir().join(format!("smite-emit-test-{}", std::process::id()));
        let md = dir.join("02-peer-protocol.md");
        let out = dir.join("seeds");
        std::fs::create_dir_all(&dir).expect("temp dir");
        std::fs::write(&md, FIXTURE).expect("fixture");

        let stats = emit_seed_dir(&md, &out).expect("emit");
        assert!(stats.sketches > 0, "fixture must yield sketches: {stats:?}");
        assert_eq!(
            stats.written, stats.sketches,
            "splice_init sketches convert"
        );
        assert_eq!(stats.roundtrip_ok, stats.written);

        let entries: Vec<_> = std::fs::read_dir(&out)
            .expect("seed dir")
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().is_some_and(|ext| ext == "seed"))
            .collect();
        assert_eq!(entries.len(), stats.written);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn emit_seed_dir_rejects_missing_file() {
        let err =
            emit_seed_dir(Path::new("/nonexistent/bolt.md"), Path::new("/tmp/x")).unwrap_err();
        assert!(err.contains("read"), "unexpected error: {err}");
    }

    #[test]
    fn emit_seed_dir_is_deterministic() {
        // The whole chain (parser -> seeds -> sketches -> converter ->
        // postcard) must be byte-stable: AFL corpora are regenerated and
        // diffed against this output, so any nondeterminism would show up
        // as phantom corpus drift.
        let root = env!("CARGO_MANIFEST_DIR");
        let md = Path::new(root).join("data/bolt02-peer-protocol.md");
        let base = std::env::temp_dir().join(format!("smite-det-{}-a", std::process::id()));
        let other = std::env::temp_dir().join(format!("smite-det-{}-b", std::process::id()));

        let a = emit_seed_dir(&md, &base).expect("first emit");
        let b = emit_seed_dir(&md, &other).expect("second emit");
        assert_eq!(a, b, "emit stats differ between runs");

        let names = |dir: &Path| -> Vec<String> {
            let mut v: Vec<String> = std::fs::read_dir(dir)
                .expect("seed dir")
                .filter_map(|e| e.ok())
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .collect();
            v.sort();
            v
        };
        assert_eq!(names(&base), names(&other), "file names differ");
        for name in names(&base) {
            let fa = std::fs::read(base.join(&name)).expect("seed a");
            let fb = std::fs::read(other.join(&name)).expect("seed b");
            assert_eq!(fa, fb, "{name} differs between runs");
        }

        std::fs::remove_dir_all(&base).ok();
        std::fs::remove_dir_all(&other).ok();
    }
}
