//! A simulation owner on a dedicated thread. The client polls snapshots and
//! sends commands; it never waits for a simulation step or holds its lock.
use std::{sync::{Mutex, mpsc}, thread, time::{Duration, Instant}};
use crate::{Command, SimError, SimState, STEP_DAYS};

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct CommandResult { pub sequence: u64, pub result: Result<(), SimError> }

enum Input {
    Command(u64, Command),
    Clock { paused: bool, days_per_second: f64 },
    Stop,
}

/// Snapshots are published at most this often...
const PUBLISH_EVERY: Duration = Duration::from_millis(33);
/// ...and no more often than this many times the time a copy takes, so copying never takes
/// more than about a fifth of the simulation thread.
const COPY_SHARE: u32 = 4;

pub struct Snapshot {
    pub state: SimState,
    pub acknowledged: u64,
    pub results: Vec<CommandResult>,
}

pub struct SimThread {
    input: mpsc::Sender<Input>,
    spares: mpsc::Sender<SimState>,
    output: Mutex<mpsc::Receiver<Snapshot>>,
    thread: Option<thread::JoinHandle<()>>,
}

impl SimThread {
    pub fn spawn(mut state: SimState) -> Self {
        state.client_commands = None;
        state.colony.events.clear();
        // The water says what changed when a snapshot is about to be made, not at every step.
        state.water.note_on_request();
        let (input, receive) = mpsc::channel();
        let (publish, output) = mpsc::sync_channel(1);
        let (spares, recycled) = mpsc::channel::<SimState>();
        let thread = thread::Builder::new().name("colony-simulation".into()).spawn(move || {
            let mut paused = true;
            let mut rate = 0.0;
            let mut acc = 0.0;
            let mut acknowledged = 0;
            let mut results = Vec::new();
            let mut last = Instant::now();
            let mut published = last;
            let mut changed = false;
            // A world handed back by the interface (or a snapshot it never took), copied into
            // for the next snapshot; and how long a copy takes, to keep copying a small part
            // of the work at the fastest speeds.
            let mut spare: Option<SimState> = None;
            let mut gap = PUBLISH_EVERY;
            loop {
                if state.colony.is_collapsed() { paused = true; }
                let idle = paused || !state.is_running();
                let mut apply = |input| match input {
                    Input::Stop => false,
                    Input::Clock { paused: p, days_per_second } => {
                        if paused != p || rate != days_per_second { acc = 0.0; }
                        paused = p; rate = days_per_second.max(0.0); true
                    }
                    Input::Command(sequence, command) => {
                        let result = state.apply(command);
                        acknowledged = sequence;
                        results.push(CommandResult { sequence, result });
                        changed = true; true
                    }
                };
                if idle {
                    match receive.recv_timeout(Duration::from_millis(10)) {
                        Ok(input) => if !apply(input) { break; },
                        Err(mpsc::RecvTimeoutError::Disconnected) => break,
                        Err(mpsc::RecvTimeoutError::Timeout) => (),
                    }
                }
                let mut stop = false;
                while let Ok(input) = receive.try_recv() { if !apply(input) { stop = true; break; } }
                if stop { break; }
                let now = Instant::now();
                let elapsed = (now - last).as_secs_f64().min(0.1);
                last = now;
                if !paused && state.is_running() && !state.colony.is_collapsed() {
                    acc = (acc + elapsed * rate).min(STEP_DAYS as f64 * 64.0);
                    // Poll commands between steps. Water keeps its original fixed
                    // timestep, solver and step order.
                    if acc >= STEP_DAYS as f64 {
                        if let Err(error) = state.step(STEP_DAYS) {
                            results.push(CommandResult { sequence: acknowledged, result: Err(error) });
                            paused = true;
                        }
                        acc -= STEP_DAYS as f64;
                        changed = true;
                    } else { thread::sleep(Duration::from_millis(1)); }
                }
                if changed && published.elapsed() >= gap {
                    while let Ok(old) = recycled.try_recv() { spare = Some(old); }
                    state.water.note_changes();
                    let copying = Instant::now();
                    let copy = {
                        let _p = crate::prof::scope("snapshot copy");
                        match spare.take() {
                            Some(mut old) => { old.clone_from(&state); old }
                            None => { crate::prof::count("snapshot copy/into new memory", 1); state.clone() }
                        }
                    };
                    gap = PUBLISH_EVERY.max(copying.elapsed() * COPY_SHARE);
                    let snapshot = Snapshot { state: copy, acknowledged, results: results.clone() };
                    match publish.try_send(snapshot) {
                        Ok(()) => {
                            // Told once: the next snapshot says what was seen after this one.
                            state.fog.dirty = false;
                            state.colony.events.clear(); results.clear(); changed = false;
                            published = Instant::now();
                        }
                        Err(mpsc::TrySendError::Full(snapshot)) => { spare = Some(snapshot.state); published = Instant::now(); }
                        Err(mpsc::TrySendError::Disconnected(_)) => break,
                    }
                }
            }
        }).expect("start simulation sim_thread");
        Self { input, spares, output: Mutex::new(output), thread: Some(thread) }
    }
    pub fn command(&self, sequence: u64, command: Command) -> Result<(), String> {
        self.input.send(Input::Command(sequence, command)).map_err(|_| "simulation sim_thread stopped".into())
    }
    pub fn clock(&self, paused: bool, days_per_second: f64) -> Result<(), String> {
        self.input.send(Input::Clock { paused, days_per_second }).map_err(|_| "simulation sim_thread stopped".into())
    }
    pub fn poll(&self) -> Option<Snapshot> { self.output.lock().unwrap().try_recv().ok() }
    /// Hand back a world the interface no longer needs (the snapshot before the latest): the
    /// next snapshot is copied into it rather than into newly allocated memory.
    pub fn recycle(&self, old: SimState) { let _ = self.spares.send(old); }
}

impl Drop for SimThread {
    fn drop(&mut self) {
        let _ = self.input.send(Input::Stop);
        // Dropping a world must not wait for a costly in-flight step on the UI thread.
        self.thread.take();
    }
}
