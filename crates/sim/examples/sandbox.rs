fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(path) = args.first() else {
        eprintln!("usage: sandbox SCRIPT");
        std::process::exit(2);
    };
    let text = std::fs::read_to_string(path).unwrap_or_else(|e| {
        eprintln!("cannot read {path}: {e}");
        std::process::exit(2);
    });
    match sim::sandbox::run_script(&text) {
        Ok((s, report)) => {
            for l in &report.lines {
                println!("{l}");
            }
            if std::env::var_os("WATER_TIMINGS").is_some() {
                println!("{}", s.water.timings.report(s.water.timings.total_s()));
            }
            let failed = report.checks.iter().filter(|c| !c.1).count();
            println!("{} checks, {} failed", report.checks.len(), failed);
            std::process::exit(if failed == 0 { 0 } else { 1 });
        }
        Err(e) => {
            eprintln!("error: {e}");
            std::process::exit(2);
        }
    }
}
