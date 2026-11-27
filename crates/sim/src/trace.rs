use std::{collections::VecDeque, fs::File, io::{BufWriter, Write}, path::Path, sync::{Mutex, OnceLock, atomic::{AtomicBool, Ordering}}, time::Instant};

const RECENT_LINES: usize = 20_000;

struct Sink { out: BufWriter<File>, flushed: Instant }
const FLUSH_SECONDS: f64 = 0.5;

struct Trace { started: Instant, file: Mutex<Option<Sink>>, recent: Mutex<VecDeque<(f64, String)>> }

fn open(path: &Path) -> std::io::Result<Sink> {
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) { std::fs::create_dir_all(parent)?; }
    let file = std::fs::OpenOptions::new().create(true).append(true).open(path)?;
    let mut out = BufWriter::with_capacity(1 << 20, file);
    writeln!(out, "SESSION pid={} unix_seconds={}", std::process::id(), std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs())?;
    out.flush()?;
    Ok(Sink { out, flushed: Instant::now() })
}

pub fn flush() {
    if let Ok(mut slot) = trace().file.lock() {
        if let Some(sink) = slot.as_mut() {
            let _ = sink.out.flush();
            sink.flushed = Instant::now();
        }
    }
}

static ENABLED: AtomicBool = AtomicBool::new(false);

fn trace() -> &'static Trace {
    static TRACE: OnceLock<Trace> = OnceLock::new();
    TRACE.get_or_init(|| {
        let file = std::env::var_os("BC_INPUT_LOG").and_then(|path| match open(Path::new(&path)) {
            Ok(file) => Some(file),
            Err(e) => { eprintln!("Input trace could not open {}: {e}", Path::new(&path).display()); None }
        });
        ENABLED.store(file.is_some(), Ordering::Release);
        Trace { started: Instant::now(), file: Mutex::new(file), recent: Mutex::new(VecDeque::new()) }
    })
}

pub fn start(path: &Path) {
    let trace = trace();
    if let Ok(mut slot) = trace.file.lock() {
        if slot.is_none() {
            match open(path) {
                Ok(file) => { *slot = Some(file); ENABLED.store(true, Ordering::Release); }
                Err(e) => eprintln!("Input trace could not open {}: {e}", path.display()),
            }
        }
    }
}

pub fn enabled() -> bool {
    let _ = trace();
    ENABLED.load(Ordering::Acquire)
}

static SIM_DAY: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
static SIM_STEP: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

pub fn set_clock(day: f64, step: u64) {
    SIM_DAY.store(day.to_bits(), std::sync::atomic::Ordering::Relaxed);
    SIM_STEP.store(step, std::sync::atomic::Ordering::Relaxed);
}

pub fn sim_stamp() -> String {
    let day = f64::from_bits(SIM_DAY.load(std::sync::atomic::Ordering::Relaxed));
    format!("day={day:.4} step={}", SIM_STEP.load(std::sync::atomic::Ordering::Relaxed))
}

pub fn clock() -> f64 { trace().started.elapsed().as_secs_f64() }

pub fn event(message: impl std::fmt::Display) {
    let trace = trace();
    if !ENABLED.load(Ordering::Acquire) {
        return;
    }
    let Ok(mut slot) = trace.file.lock() else { return };
    let Some(sink) = slot.as_mut() else { return };
    let t = trace.started.elapsed().as_secs_f64();
    let line = message.to_string();
    let mut result = writeln!(sink.out, "{t:010.3} {line}");
    if result.is_ok() && sink.flushed.elapsed().as_secs_f64() >= FLUSH_SECONDS {
        result = sink.out.flush();
        sink.flushed = Instant::now();
    }
    if let Err(e) = result {
        eprintln!("Input trace stopped: {e}");
        *slot = None;
        ENABLED.store(false, Ordering::Release);
    }
    if let Ok(mut recent) = trace.recent.lock() {
        if recent.len() >= RECENT_LINES { recent.pop_front(); }
        recent.push_back((t, line));
    }
}

pub fn recent(seconds: f64) -> Vec<String> {
    let trace = trace();
    let now = trace.started.elapsed().as_secs_f64();
    trace.recent.lock().map(|r| r.iter().filter(|(t, _)| now - t <= seconds).map(|(t, l)| format!("{t:010.3} {l}")).collect()).unwrap_or_default()
}
