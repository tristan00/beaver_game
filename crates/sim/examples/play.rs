//! Runs game scripts headless (`sim::script`): starts, settling, commands, scripted players,
//! replays of recorded games, and checks. Prints the timeline and summary of each script; exits
//! non-zero if any check fails.
//!
//! Usage: cargo run --release -p sim --example play -- [--quiet] [--record OUT] FILE.script [...]
//! `--record OUT` writes the commands the script gave as a replayable log.
use std::process::ExitCode;

fn main() -> ExitCode {
    let mut quiet = false;
    let mut record: Option<String> = None;
    let mut files = Vec::new();
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--quiet" => quiet = true,
            "--record" => record = args.next(),
            _ => files.push(a),
        }
    }
    if files.is_empty() {
        eprintln!("usage: play [--quiet] [--record OUT] FILE.script [...]");
        return ExitCode::FAILURE;
    }
    let mut failed = false;
    for f in &files {
        let text = match std::fs::read_to_string(f) {
            Ok(t) => t,
            Err(e) => {
                eprintln!("{f}: {e}");
                failed = true;
                continue;
            }
        };
        let dir = std::path::Path::new(f).parent().map(|p| p.to_path_buf()).unwrap_or_default();
        let load = |name: &str| std::fs::read_to_string(dir.join(name)).or_else(|_| std::fs::read_to_string(name)).map_err(|e| format!("{name}: {e}"));
        println!("=== {f}");
        match sim::script::run_and_record(&text, &load) {
            Ok((out, log)) => {
                for l in &out.lines {
                    if !quiet || l.starts_with("SUMMARY") || l.contains("COLLAPSED") {
                        println!("{l}");
                    }
                }
                for (c, ok) in &out.checks {
                    println!("  check {c}: {}", if *ok { "ok" } else { "FAILED" });
                }
                if !out.passed() {
                    failed = true;
                }
                if let Some(path) = &record {
                    let _ = std::fs::write(path, log);
                }
            }
            Err(e) => {
                eprintln!("{f}: {e}");
                failed = true;
            }
        }
    }
    if failed { ExitCode::FAILURE } else { ExitCode::SUCCESS }
}
