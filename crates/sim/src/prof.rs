//! Named timers for the profile `perf` prints: where a step's time goes, by part
//! of the step and by what each beaver is doing. Off unless `enable` is called, and then a
//! timer costs two clock reads; while off it costs one load.
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

static ON: AtomicBool = AtomicBool::new(false);

#[derive(Clone, Copy)]
struct Row { name: &'static str, nanos: u64, calls: u64 }

/// One table for the whole program: the simulation runs on its own thread in the game.
static ROWS: Mutex<Vec<Row>> = Mutex::new(Vec::new());

pub fn enable() { ON.store(true, Ordering::Relaxed); }

pub fn enabled() -> bool { ON.load(Ordering::Relaxed) }

/// Times from here until it is dropped, under `name`. Names read as paths ("agents/haul/path"):
/// a name's time includes everything timed inside it.
pub struct Scope(Option<(&'static str, Instant)>);

pub fn scope(name: &'static str) -> Scope {
    Scope(enabled().then(|| (name, Instant::now())))
}

/// Count something that happened, with no time of its own.
pub fn count(name: &'static str, n: u64) {
    if enabled() { add(name, 0, n); }
}

fn add(name: &'static str, nanos: u64, calls: u64) {
    let mut rows = ROWS.lock().unwrap_or_else(|e| e.into_inner());
    match rows.iter_mut().find(|r| std::ptr::eq(r.name, name) || r.name == name) {
        Some(r) => { r.nanos += nanos; r.calls += calls; }
        None => rows.push(Row { name, nanos, calls }),
    }
}

impl Drop for Scope {
    fn drop(&mut self) {
        if let Some((name, start)) = self.0.take() {
            add(name, start.elapsed().as_nanos() as u64, 1);
        }
    }
}

pub fn reset() { ROWS.lock().unwrap_or_else(|e| e.into_inner()).clear(); }

/// Every timer, as `(name, seconds, calls)`, sorted by name.
pub fn rows() -> Vec<(&'static str, f64, u64)> {
    let mut rows: Vec<_> = ROWS.lock().unwrap_or_else(|e| e.into_inner()).iter().map(|r| (r.name, r.nanos as f64 / 1e9, r.calls)).collect();
    rows.sort_by(|a, b| a.0.cmp(b.0));
    rows
}

/// The timers as a table of milliseconds and calls for each simulated day. Names starting
/// with `+` are counted inside other rows (a path search happens within a beaver's step) and
/// are listed after them.
pub fn report(days: f64) -> String {
    let rows = rows();
    let total: f64 = rows.iter().filter(|r| !r.0.contains('/') && !r.0.starts_with('+')).map(|r| r.1).sum();
    let line = |label: String, secs: f64, calls: u64| {
        format!(
            "{label:<52} {:>10.3} {:>6.1}% {:>12.1} {:>10.2}
",
            secs * 1000.0 / days,
            100.0 * secs / total.max(f64::MIN_POSITIVE),
            calls as f64 / days,
            if calls > 0 { secs * 1e6 / calls as f64 } else { 0.0 },
        )
    };
    let mut out = format!("{:<52} {:>10} {:>7} {:>12} {:>10}
", "timer", "ms/day", "share", "calls/day", "us/call");
    let mut headed = "";
    for &(name, secs, calls) in rows.iter().filter(|r| !r.0.starts_with('+')) {
        let depth = name.matches('/').count();
        // A group with no timer of its own (the game's systems, each timed apart) is named.
        let top = name.split('/').next().unwrap_or(name);
        if depth > 0 && top != headed && !rows.iter().any(|r| r.0 == top) {
            headed = top;
            out.push_str(top);
            out.push('\n');
        }
        out.push_str(&line(format!("{}{}", "  ".repeat(depth), name.rsplit('/').next().unwrap_or(name)), secs, calls));
    }
    if rows.iter().any(|r| r.0.starts_with('+')) {
        out.push_str("inside the rows above:
");
        for &(name, secs, calls) in rows.iter().filter(|r| r.0.starts_with('+')) {
            out.push_str(&line(format!("  {}", name[1..].replace('/', ": ")), secs, calls));
        }
    }
    out
}
