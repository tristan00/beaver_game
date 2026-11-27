use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

static ON: AtomicBool = AtomicBool::new(false);

#[derive(Clone, Copy)]
struct Row { name: &'static str, nanos: u64, calls: u64 }

static ROWS: Mutex<Vec<Row>> = Mutex::new(Vec::new());

pub fn enable() { ON.store(true, Ordering::Relaxed); }

pub fn enabled() -> bool { ON.load(Ordering::Relaxed) }

pub struct Scope(Option<(&'static str, Instant)>);

pub fn scope(name: &'static str) -> Scope {
    Scope(enabled().then(|| (name, Instant::now())))
}

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

pub fn rows() -> Vec<(&'static str, f64, u64)> {
    let mut rows: Vec<_> = ROWS.lock().unwrap_or_else(|e| e.into_inner()).iter().map(|r| (r.name, r.nanos as f64 / 1e9, r.calls)).collect();
    rows.sort_by(|a, b| a.0.cmp(b.0));
    rows
}

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
