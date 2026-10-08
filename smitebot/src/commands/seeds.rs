//! Generates a spec-derived seed corpus from a BOLT markdown file.
//!
//! Wraps the `smite-requirements` pipeline so a campaign's `seed_dir` can be
//! (re)generated in one step instead of invoking the `smite-requirements`
//! binary by hand. Seeds are postcard-encoded IR programs; point a campaign
//! config's `seed_dir` at the output directory.

use std::path::PathBuf;

use clap::Args;

/// Command handler for `smitebot seeds`.
pub struct SeedsCommand;

/// CLI arguments for `smitebot seeds`.
#[derive(Debug, Args)]
pub struct SeedsArgs {
    /// Path to a BOLT markdown file (e.g. `02-peer-protocol.md`).
    bolt_md: PathBuf,
    /// Output directory for the seed files; created if missing.
    out_dir: PathBuf,
}

impl SeedsCommand {
    /// Runs the seed pipeline and reports the counts.
    pub fn execute(args: &SeedsArgs) -> bool {
        match smite_requirements::emit_seed_dir(&args.bolt_md, &args.out_dir) {
            Ok(stats) => {
                println!(
                    "seeds: {} written, {} roundtrips verified (of {} sketches) -> {}",
                    stats.written,
                    stats.roundtrip_ok,
                    stats.sketches,
                    args.out_dir.display()
                );
                stats.written > 0
            }
            Err(e) => {
                log::error!("seed generation failed: {e}");
                false
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn execute_fails_on_missing_bolt_file() {
        let args = SeedsArgs {
            bolt_md: PathBuf::from("/nonexistent/bolt.md"),
            out_dir: std::env::temp_dir().join("smitebot-seeds-test"),
        };
        assert!(!SeedsCommand::execute(&args));
    }
}
