//! Local diagnostics shared by the interface and command dispatcher. The game starts it for
//! every play session; headless runs and tests never write anything.
use std::{collections::VecDeque, fs::File, io::Write, path::Path, sync::{Mutex, OnceLock}, time::Instant};

/// Lines kept in memory for a capture, about the last few minutes of play.
const RECENT_LINES: usize = 20_000;

struct Trace { started: Instant, file: Mutex<Option<File>>, recent: Mutex<VecDeque<(f64, String)>> }

fn open(path: &Path) -> std::io::Result<File> {
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) { std::fs::create_dir_all(parent)?; }
    let mut file = std::fs::OpenOptions::new().create(true).append(true).open(path)?;
    writeln!(file, "SESSION pid={} unix_seconds={}", std::process::id(), std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs())?;
    Ok(file)
}

fn trace() -> &'static Trace {
    static TRACE: OnceLock<Trace> = OnceLock::new();
    TRACE.get_or_init(|| {
        let file = std::env::var_os("BC_INPUT_LOG").and_then(|path| match open(Path::new(&path)) {
            Ok(file) => Some(file),
            Err(e) => { eprintln!("Input trace could not open {}: {e}", Path::new(&path).display()); None }
        });
        Trace { started: Instant::now(), file: Mutex::new(file), recent: Mutex::new(VecDeque::new()) }
    })
}

/// Record to `path`, unless `BC_INPUT_LOG` already chose a file.
pub fn start(path: &Path) {
    let trace = trace();
    if let Ok(mut slot) = trace.file.lock() {
        if slot.is_none() {
            match open(path) {
                Ok(file) => *slot = Some(file),
                Err(e) => eprintln!("Input trace could not open {}: {e}", path.display()),
            }
        }
    }
}

pub fn enabled() -> bool { trace().file.lock().is_ok_and(|f| f.is_some()) }

/// Seconds since the session started, the clock every line is stamped with.
pub fn clock() -> f64 { trace().started.elapsed().as_secs_f64() }

pub fn event(message: impl std::fmt::Display) {
    let trace = trace();
    let Ok(mut slot) = trace.file.lock() else { return };
    let Some(file) = slot.as_mut() else { return };
    let t = trace.started.elapsed().as_secs_f64();
    let line = message.to_string();
    if let Err(e) = writeln!(file, "{t:010.3} {line}") {
        eprintln!("Input trace stopped: {e}");
        *slot = None;
    }
    if let Ok(mut recent) = trace.recent.lock() {
        if recent.len() >= RECENT_LINES { recent.pop_front(); }
        recent.push_back((t, line));
    }
}

/// The lines recorded in the last `seconds`, stamped as in the file.
pub fn recent(seconds: f64) -> Vec<String> {
    let trace = trace();
    let now = trace.started.elapsed().as_secs_f64();
    trace.recent.lock().map(|r| r.iter().filter(|(t, _)| now - t <= seconds).map(|(t, l)| format!("{t:010.3} {l}")).collect()).unwrap_or_default()
}
