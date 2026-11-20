//! Opt-in, local diagnostics shared by the interface and command dispatcher.
use std::{fs::File, io::Write, sync::{Mutex, OnceLock}, time::Instant};

struct Trace { started: Instant, file: Mutex<Option<File>> }

fn trace() -> &'static Trace {
    static TRACE: OnceLock<Trace> = OnceLock::new();
    TRACE.get_or_init(|| {
        let file = std::env::var_os("BC_INPUT_LOG").and_then(|path| {
            let path = std::path::PathBuf::from(path);
            let result = (|| -> std::io::Result<File> {
                if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) { std::fs::create_dir_all(parent)?; }
                let mut file = std::fs::OpenOptions::new().create(true).append(true).open(&path)?;
                writeln!(file, "SESSION pid={} unix_seconds={}", std::process::id(), std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs())?;
                Ok(file)
            })();
            match result { Ok(file) => Some(file), Err(e) => { eprintln!("Input trace could not open {}: {e}", path.display()); None } }
        });
        Trace { started: Instant::now(), file: Mutex::new(file) }
    })
}

pub fn enabled() -> bool { trace().file.lock().is_ok_and(|f| f.is_some()) }

pub fn event(message: impl std::fmt::Display) {
    let trace = trace();
    if let Ok(mut slot) = trace.file.lock() {
        if let Some(file) = slot.as_mut() {
            if let Err(e) = writeln!(file, "{:010.3} {message}", trace.started.elapsed().as_secs_f64()) {
                eprintln!("Input trace stopped: {e}");
                *slot = None;
            }
        }
    }
}
