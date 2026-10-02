//! `smite-requirements`: deterministic BOLT requirement extraction and
//! violation-seed scaffolding for spec-anchored fuzzing.
//!
//! Subcommands:
//! - `extract <bolt-md>` — parse a BOLT markdown file, print requirements.json
//! - `verify <bolt-md> <requirements.json>` — check stored texts still match (drift)
//! - `seeds <bolt-md> [--findings <findings.json>]` — derive seed candidates,
//!   gated by finding disclosure states

mod bridge;
mod converter;
mod model;
mod parser;


use std::path::PathBuf;
use std::process::ExitCode;

use model::FindingRef;

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(msg) => {
            eprintln!("smite-requirements: {msg}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), String> {
    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(String::as_str) {
        Some("extract") => {
            let path = args.get(2).ok_or("usage: extract <bolt-md>")?;
            let text = read(path)?;
            let reqs = parser::parse(&file_stem(path), &text).map_err(|e| e.to_string())?;
            print_json(&serde_json::to_string_pretty(&reqs).map_err(|e| e.to_string())?);
            Ok(())
        }
        Some("verify") => {
            let md = args.get(2).ok_or("usage: verify <bolt-md> <requirements.json>")?;
            let json = args.get(3).ok_or("usage: verify <bolt-md> <requirements.json>")?;
            let text = read(md)?;
            let reqs: Vec<model::Requirement> =
                serde_json::from_str(&read(json)?).map_err(|e| e.to_string())?;
            let lines: Vec<String> = text.lines().map(whitespace_join).collect();
            let mut drifted = 0usize;
            for r in &reqs {
                let Some(line) = lines.get(r.line.checked_sub(1).ok_or("line 0")?) else {
                    eprintln!("DRIFT {} source line {} missing", r.id, r.line);
                    drifted += 1;
                    continue;
                };
                let bullet = line.trim().trim_start_matches("- ").trim().to_owned();
                if bullet != r.text {
                    eprintln!("DRIFT {} text changed at {}", r.id, r.source);
                    drifted += 1;
                }
            }
            if drifted == 0 {
                println!("verify: {} requirements, 0 drift", reqs.len());
                Ok(())
            } else {
                Err(format!("{drifted} requirement(s) drifted"))
            }
        }
        Some("seeds") => {
            let md = args.get(2).ok_or("usage: seeds <bolt-md> [--findings <json>]")?;
            let text = read(md)?;
            let reqs = parser::parse(&file_stem(md), &text).map_err(|e| e.to_string())?;
            let mut seeds = parser::seeds(&reqs);

            let mut sealed = 0usize;
            if let Some(pos) = args.iter().position(|a| a == "--findings") {
                let fpath = args.get(pos + 1).ok_or("--findings needs a path")?;
                let findings: Vec<FindingRef> =
                    serde_json::from_str(&read(fpath)?).map_err(|e| e.to_string())?;
                for f in &findings {
                    if f.seedable() {
                        println!(
                            "# finding {} is seedable ({}); shape enrichment pending IR ops",
                            f.id, f.state
                        );
                    } else {
                        sealed += 1;
                    }
                }
            }
            seeds.sort_by(|a, b| a.id.cmp(&b.id));
            print_json(&serde_json::to_string_pretty(&seeds).map_err(|e| e.to_string())?);
            eprintln!(
                "seeds: {} candidates from {} requirements ({sealed} finding(s) sealed, correctly skipped)",
                seeds.len(),
                reqs.len()
            );
            Ok(())
        }
        Some("programs") => {
            let md = args.get(2).ok_or("usage: programs <bolt-md>")?;
            let text = read(md)?;
            let reqs = parser::parse(&file_stem(md), &text).map_err(|e| e.to_string())?;
            let seeds = parser::seeds(&reqs);
            let sketches = bridge::sketches(&seeds, &reqs);
            let (convertible, total) = converter::convertible_count(&sketches);
            let mut built = 0usize;
            for sketch in &sketches {
                if let Some(program) = converter::sketch_to_program(sketch) {
                    built += 1;
                    println!(
                        "# {} -> {} instructions",
                        sketch.id,
                        program.instructions.len()
                    );
                }
            }
            println!(
                "programs: {built} built, {convertible} convertible of {total} sketches"
            );
            if built < convertible {
                eprintln!("warning: {} convertible sketches failed to build", convertible - built);
            }
            Ok(())
        }
        Some("sketches") => {
            let md = args.get(2).ok_or("usage: sketches <bolt-md>")?;
            let text = read(md)?;
            let reqs = parser::parse(&file_stem(md), &text).map_err(|e| e.to_string())?;
            let seeds = parser::seeds(&reqs);
            let sketches = bridge::sketches(&seeds, &reqs);
            print_json(&serde_json::to_string_pretty(&sketches).map_err(|e| e.to_string())?);
            eprintln!(
                "sketches: {} from {} seeds / {} requirements",
                sketches.len(),
                seeds.len(),
                reqs.len()
            );
            Ok(())
        }
        _ => Err(format!(
            "usage: {} <extract|verify|seeds> ...\n  extract <bolt-md>\n  verify <bolt-md> <requirements.json>\n  seeds <bolt-md> [--findings <findings.json>]",
            args.first().map(String::as_str).unwrap_or("smite-requirements")
        )),
    }
}

fn read(path: &str) -> Result<String, String> {
    std::fs::read_to_string(PathBuf::from(path)).map_err(|e| format!("read {path}: {e}"))
}

fn file_stem(path: &str) -> String {
    PathBuf::from(path)
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default()
}

fn whitespace_join(line: &str) -> String {
    line.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn print_json(text: &str) {
    println!("{text}");
}
