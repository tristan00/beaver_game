use crate::state::Sim;
use bevy::prelude::*;

pub const FIXED_DT_DAYS: f32 = sim::STEP_DAYS;
pub const BASE_DAYS_PER_SECOND: f32 = 1.0 / 30.0;
pub const MAX_STEPS_PER_FRAME: u32 = 64;
const FRAME_SIM_BUDGET_S: f32 = 0.012;
// At fast-forward speeds, spend more of a frame advancing large river maps. Keep
// ordinary play's shorter budget and the bounded catch-up queue unchanged.
const FAST_FRAME_SIM_BUDGET_S: f32 = 0.030;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Speed {
    X1,
    X2,
    X3,
}

impl Speed {
    pub fn multiplier(self) -> f32 {
        match self {
            Speed::X1 => 1.0,
            Speed::X2 => 120.0,
            Speed::X3 => 180.0,
        }
    }
}

#[derive(Resource, Debug)]
pub struct SimClock {
    pub paused: bool,
    pub speed: Speed,
    acc_days: f64,
    presentation_days: Option<f64>,
}

impl Default for SimClock {
    fn default() -> Self {
        SimClock { paused: false, speed: Speed::X1, acc_days: 0.0, presentation_days: None }
    }
}

impl SimClock {
    pub fn toggle_pause(&mut self) {
        self.paused = !self.paused;
        self.acc_days = 0.0;
    }
    pub fn set_speed(&mut self, s: Speed) {
        if self.speed != s || self.paused {
            self.acc_days = 0.0;
        }
        self.speed = s;
        self.paused = false;
    }
    pub fn pending_days(&self) -> f32 {
        self.acc_days.min(FIXED_DT_DAYS as f64) as f32
    }
    pub fn render_days(&self, latest: f64) -> f64 {
        self.presentation_days.unwrap_or(latest + self.pending_days() as f64 - FIXED_DT_DAYS as f64)
    }
    fn advance_presentation(&mut self, real_dt: f64, latest: f64) {
        let shown = self.presentation_days.get_or_insert(latest);
        if !self.paused && real_dt.is_finite() && real_dt > 0.0 {
            let rate = BASE_DAYS_PER_SECOND as f64 * self.speed.multiplier() as f64;
            // Play the received movement history at display frequency. Never
            // predict past the latest completed simulation step.
            // Anything more than a step behind (a long frame, a late snapshot) is made up
            // over a fraction of a second, so what is drawn never drifts behind the colony.
            let behind = (latest - *shown - FIXED_DT_DAYS as f64).max(0.0);
            *shown = (*shown + real_dt.min(0.25) * rate + behind * (real_dt * 4.0).min(1.0)).min(latest);
        }
        self.acc_days = (*shown - latest + FIXED_DT_DAYS as f64).clamp(0.0, FIXED_DT_DAYS as f64);
    }
    fn queue(&mut self, real_dt: f64, step: f32) -> u32 {
        if self.paused || !real_dt.is_finite() || real_dt <= 0.0 {
            return 0;
        }
        self.acc_days += real_dt.min(0.25) * BASE_DAYS_PER_SECOND as f64 * self.speed.multiplier() as f64;
        self.acc_days = self.acc_days.min(step as f64 * MAX_STEPS_PER_FRAME as f64);
        ((self.acc_days / step as f64) as u32).min(MAX_STEPS_PER_FRAME)
    }

    fn complete(&mut self, step: f32) {
        self.acc_days = (self.acc_days - step as f64).max(0.0);
    }

    fn discard_backlog(&mut self, step: f32) {
        // Overload slows the entire simulation. It must not schedule days of
        // catch-up work or let animation interpolate beyond simulated state.
        self.acc_days %= step as f64;
    }

}

pub struct SimDriverPlugin;

impl Plugin for SimDriverPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SimClock>().init_resource::<SimulationWorker>()
            .add_systems(First, receive_sim)
            .add_systems(Update, (time_keys, step_sim).chain())
            .add_systems(Last, send_commands);
    }
}

fn time_keys(
    keys: Res<ButtonInput<KeyCode>>,
    typing: Option<Res<crate::hud::TextCapture>>,
    menu: Option<Res<crate::hud::MenuOpen>>,
    mut clock: ResMut<SimClock>,
) {
    if typing.is_some_and(|t| t.0) || menu.is_some_and(|m| m.0) {
        return;
    }
    if keys.just_pressed(KeyCode::Space) {
        clock.toggle_pause();
    }
    for (k, s) in [(KeyCode::Digit1, Speed::X1), (KeyCode::Digit2, Speed::X2), (KeyCode::Digit3, Speed::X3)] {
        if keys.just_pressed(k) {
            clock.set_speed(s);
        }
    }
}

#[derive(Resource, Default)]
pub(crate) struct SimulationWorker {
    worker: Option<sim::runtime::Worker>,
    instance: u64,
    next_sequence: u64,
    pending: Vec<(u64, sim::Command)>,
    clock: Option<(bool, Speed)>,
}

fn receive_sim(mut worker: ResMut<SimulationWorker>, mut sim: ResMut<Sim>, mut clock: ResMut<SimClock>) {
    let _p = ::sim::prof::scope("game/receive sim");
    if worker.instance != sim.0.instance { return; }
    let Some(snapshot) = worker.worker.as_ref().and_then(|w| w.poll()) else { return; };
    worker.pending.retain(|(sequence, _)| *sequence > snapshot.acknowledged);
    for result in snapshot.results {
        if let Err(error) = result.result {
            error!("command {} failed on simulation owner: {}", result.sequence, error);
        }
    }
    let events = std::mem::take(&mut sim.0.colony.events);
    // The simulation reports fog as newly seen once; keep it until the fog is redrawn.
    let fog_seen = sim.0.fog.dirty;
    // The world it replaces goes back to the worker, which copies the next snapshot into it.
    let old = std::mem::replace(&mut sim.0, snapshot.state);
    if let Some(owner) = &worker.worker { owner.recycle(old); }
    for event in events.into_iter().rev() { sim.0.colony.events.push_front(event); }
    sim.0.fog.dirty |= fog_seen;
    // Reapply only commands newer than the received snapshot. They remain in
    // the worker's queue and are not transmitted twice. What they change here is a guess
    // until the simulation has applied them itself.
    if !worker.pending.is_empty() {
        sim.0.guess_ahead();
    }
    for (_, command) in &worker.pending {
        let events = sim.0.colony.events.clone();
        let _ = sim.0.apply(command.clone());
        sim.0.colony.events = events;
    }
    sim.0.client_commands = Some(Vec::new());
    if sim.0.colony.is_collapsed() { clock.paused = true; }
}

fn send_commands(mut worker: ResMut<SimulationWorker>, mut sim: ResMut<Sim>, mut clock: ResMut<SimClock>) {
    if worker.instance != sim.0.instance { return; }
    // Only touch the world when there is something to send: a write marks it changed, and
    // everything drawn from it would then redo its work every frame.
    if sim.0.client_commands.as_ref().is_none_or(|c| c.is_empty()) { return; }
    let commands = sim.0.client_commands.as_mut().map(std::mem::take).unwrap_or_default();
    for command in commands {
        worker.next_sequence += 1;
        let sequence = worker.next_sequence;
        if let Some(owner) = &worker.worker {
            if let Err(error) = owner.command(sequence, command.clone()) { error!("{error}"); clock.paused = true; }
        }
        worker.pending.push((sequence, command));
    }
}

pub(crate) fn step_sim(time: Res<Time<Real>>, cli: Res<crate::cli::Cli>, mut clock: ResMut<SimClock>, mut sim: ResMut<Sim>, mut worker: ResMut<SimulationWorker>) {
    // Keep the existing standalone water sandbox and deterministic screenshot
    // runner on their original scheduling path.
    if sim.0.sandbox || cli.screenshot.is_some() {
        synchronous_step(&time, &mut clock, &mut sim);
        return;
    }
    if worker.instance != sim.0.instance || worker.worker.is_none() {
        worker.worker = Some(sim::runtime::Worker::spawn(sim.0.clone()));
        worker.instance = sim.0.instance;
        worker.next_sequence = 0;
        worker.pending.clear();
        worker.clock = None;
        clock.presentation_days = Some(sim.0.time_days);
        sim.0.client_commands = Some(Vec::new());
    }
    let setting = (clock.paused, clock.speed);
    if worker.clock != Some(setting) {
        let rate = BASE_DAYS_PER_SECOND as f64 * clock.speed.multiplier() as f64;
        if let Err(error) = worker.worker.as_ref().unwrap().clock(clock.paused, rate) { error!("{error}"); clock.paused = true; }
        worker.clock = Some(setting);
    }
    clock.advance_presentation(time.delta_secs_f64(), sim.0.time_days);
}

fn synchronous_step(time: &Time<Real>, clock: &mut SimClock, sim: &mut Sim) {
    if sim.0.colony.is_collapsed() { clock.paused = true; return; }
    if !sim.0.is_running() {
        return;
    }
    let started = std::time::Instant::now();
    let budget = if clock.speed != Speed::X1 { FAST_FRAME_SIM_BUDGET_S } else { FRAME_SIM_BUDGET_S };
    // Speed changes the number of fixed steps, not the duration of a step.
    let dt = FIXED_DT_DAYS;
    let count = clock.queue(time.delta_secs_f64(), dt);
    for _ in 0..count {
        if let Err(e) = sim.0.step(dt) {
            error!("sim step failed: {e}");
            clock.paused = true;
            return;
        }
        clock.complete(dt);
        if started.elapsed().as_secs_f32() > budget {
            clock.discard_backlog(dt);
            break;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presentation_advances_each_frame_between_three_hertz_snapshots() {
        let mut clock = SimClock::default();
        clock.presentation_days = Some(0.0);
        let mut previous = 0.0;
        let mut moving_frames = 0;
        for frame in 0..180 {
            let elapsed = frame as f64 / 60.0;
            let latest = (elapsed * BASE_DAYS_PER_SECOND as f64 / FIXED_DT_DAYS as f64).floor() * FIXED_DT_DAYS as f64;
            clock.advance_presentation(1.0 / 60.0, latest);
            let shown = clock.render_days(latest);
            assert!(shown >= previous && shown <= latest);
            if frame >= 60 && shown > previous { moving_frames += 1; }
            previous = shown;
        }
        assert!(moving_frames >= 115, "only {moving_frames}/120 frames advanced");
        clock.toggle_pause();
        for _ in 0..60 { clock.advance_presentation(1.0 / 60.0, previous + 1.0); }
        assert_eq!(clock.render_days(previous + 1.0), previous);
        clock.set_speed(Speed::X2);
        clock.advance_presentation(1.0 / 60.0, previous + 1.0);
        assert!(clock.render_days(previous + 1.0) > previous);
    }
}
