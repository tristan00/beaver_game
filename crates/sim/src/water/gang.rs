//! A tiny fork-join pool for the water solver: a few threads that spin briefly between the
//! solver's many short phases (a substep is tens of microseconds) and sleep when the game is
//! not stepping water.

use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, OnceLock};
use std::thread::{self, Thread};

struct Job<'a> {
    f: &'a (dyn Fn(usize) + Sync),
}

struct Shared {
    epoch: AtomicU64,
    remaining: AtomicUsize,
    job: AtomicUsize,
    quit: AtomicBool,
}

pub struct Gang {
    shared: Arc<Shared>,
    threads: Vec<Thread>,
    busy: std::sync::Mutex<()>,
}

/// How long a worker spins waiting for the next phase before it sleeps.
const SPIN: u32 = 1 << 16;

impl Gang {
    fn new(workers: usize) -> Self {
        let shared = Arc::new(Shared { epoch: AtomicU64::new(0), remaining: AtomicUsize::new(0), job: AtomicUsize::new(0), quit: AtomicBool::new(false) });
        let threads = (0..workers)
            .map(|w| {
                let s = shared.clone();
                thread::Builder::new()
                    .name(format!("water-{w}"))
                    .spawn(move || {
                        let mut seen = 0u64;
                        loop {
                            let mut spins = 0u32;
                            loop {
                                let e = s.epoch.load(Ordering::Acquire);
                                if e != seen {
                                    seen = e;
                                    break;
                                }
                                if s.quit.load(Ordering::Relaxed) {
                                    return;
                                }
                                spins += 1;
                                if spins < SPIN {
                                    std::hint::spin_loop();
                                } else {
                                    thread::park();
                                }
                            }
                            let job = unsafe { &*(s.job.load(Ordering::Acquire) as *const Job) };
                            (job.f)(w + 1);
                            s.remaining.fetch_sub(1, Ordering::Release);
                        }
                    })
                    .expect("spawn water worker")
                    .thread()
                    .clone()
            })
            .collect();
        Gang { shared, threads, busy: std::sync::Mutex::new(()) }
    }

    /// Number of parallel tasks a `run` executes (workers plus the calling thread).
    pub fn width(&self) -> usize {
        self.threads.len() + 1
    }

    /// Runs `f(0)` .. `f(width - 1)` in parallel and returns when all have finished.
    pub fn run(&self, f: &(dyn Fn(usize) + Sync)) {
        // Another simulation is using the workers (tests, tools): do the tasks here, in order.
        let Ok(_guard) = self.busy.try_lock() else {
            (0..self.width()).for_each(f);
            return;
        };
        if self.threads.is_empty() {
            f(0);
            return;
        }
        let job = Job { f };
        let s = &self.shared;
        s.job.store(&job as *const Job as usize, Ordering::Release);
        s.remaining.store(self.threads.len(), Ordering::Release);
        s.epoch.fetch_add(1, Ordering::AcqRel);
        for t in &self.threads {
            t.unpark();
        }
        f(0);
        while s.remaining.load(Ordering::Acquire) != 0 {
            std::hint::spin_loop();
        }
    }
}

impl Drop for Gang {
    fn drop(&mut self) {
        self.shared.quit.store(true, Ordering::Relaxed);
        for t in &self.threads {
            t.unpark();
        }
    }
}

/// The process-wide gang, sized to the machine (half the cores, at most 12; `WATER_THREADS` overrides).
pub fn gang() -> &'static Gang {
    static G: OnceLock<Gang> = OnceLock::new();
    G.get_or_init(|| {
        let cores = thread::available_parallelism().map_or(1, |n| n.get());
        let want = std::env::var("WATER_THREADS").ok().and_then(|v| v.parse().ok()).unwrap_or((cores / 2).clamp(1, 12));
        Gang::new(want.max(1) - 1)
    })
}
