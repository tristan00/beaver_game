use crate::state::Sim;
use bevy::prelude::*;

pub const FIXED_DT_DAYS: f32 = sim::STEP_DAYS;
pub const BASE_DAYS_PER_SECOND: f32 = 1.0 / 30.0;
pub const MAX_STEPS_PER_FRAME: u32 = 64;
const FRAME_SIM_BUDGET_S: f32 = 0.012;
const SANDBOX_DT_DAYS: f32 = 120.0 / 86_400.0;

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
            Speed::X2 => 6.0,
            Speed::X3 => 24.0,
        }
    }
}

#[derive(Resource, Debug)]
pub struct SimClock {
    pub paused: bool,
    pub speed: Speed,
    acc_days: f64,
    hydraulic_day_s: f64,
    water_cost_per_second: Option<f64>,
}

impl Default for SimClock {
    fn default() -> Self {
        SimClock { paused: false, speed: Speed::X1, acc_days: 0.0, hydraulic_day_s: sim::water::GAME_HYDRAULIC_DAY_S, water_cost_per_second: None }
    }
}

impl SimClock {
    pub fn toggle_pause(&mut self) {
        self.paused = !self.paused;
    }
    pub fn set_speed(&mut self, s: Speed) {
        self.speed = s;
        self.paused = false;
    }
    pub fn pending_days(&self) -> f32 {
        self.acc_days as f32
    }
    fn queue(&mut self, real_dt: f64, step: f32) -> u32 {
        if self.paused || !real_dt.is_finite() || real_dt <= 0.0 {
            return 0;
        }
        self.acc_days += real_dt * BASE_DAYS_PER_SECOND as f64 * self.speed.multiplier() as f64;
        ((self.acc_days / step as f64) as u32).min(MAX_STEPS_PER_FRAME)
    }

    fn complete(&mut self, step: f32) {
        self.acc_days = (self.acc_days - step as f64).max(0.0);
    }

    fn record_water_cost(&mut self, physics_s: f64, cpu_s: f64, real_dt: f64) {
        if physics_s <= 0.0 || cpu_s <= 0.0 || !cpu_s.is_finite() { return; }
        let sample = cpu_s / physics_s;
        let cost = self.water_cost_per_second.map_or(sample, |old| 0.7 * old + 0.3 * sample);
        self.water_cost_per_second = Some(cost);
        // Leave time for rendering and event handling, including when the GPU
        // renders fewer frames. Larger wet areas get fewer hydraulic seconds,
        // never larger/unstable fluid steps or discarded calendar days.
        let cpu_fraction = (FRAME_SIM_BUDGET_S as f64 / real_dt.max(1.0 / 240.0)).min(0.45) * 0.9;
        let days_per_second = BASE_DAYS_PER_SECOND as f64 * self.speed.multiplier() as f64;
        self.hydraulic_day_s = (cpu_fraction / (days_per_second * cost)).clamp(60.0, sim::water::MAX_GAME_HYDRAULIC_DAY_S);
    }
}

pub struct SimDriverPlugin;

impl Plugin for SimDriverPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SimClock>().add_systems(Update, (time_keys, step_sim).chain());
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

pub(crate) fn step_sim(time: Res<Time<Real>>, mut clock: ResMut<SimClock>, mut sim: ResMut<Sim>) {
    if !sim.0.is_running() {
        return;
    }
    let started = std::time::Instant::now();
    // Short slices keep a rainstorm or breach from monopolizing one render frame.
    let dt = if sim.0.sandbox {
        SANDBOX_DT_DAYS * match clock.speed { Speed::X1 => 1.0, Speed::X2 => 2.0, Speed::X3 => 4.0 }
    } else { FIXED_DT_DAYS };
    let count = clock.queue(time.delta_secs_f64(), dt);
    let physics0 = sim.0.water.physics_seconds;
    let cpu0 = sim.0.step_timings.water_s;
    for _ in 0..count {
        if let Err(e) = sim.0.step_game_at_hydraulic_rate(dt, clock.hydraulic_day_s) {
            error!("sim step failed: {e}");
            clock.paused = true;
            return;
        }
        clock.complete(dt);
        if started.elapsed().as_secs_f32() > FRAME_SIM_BUDGET_S {
            break;
        }
    }
    clock.record_water_cost(sim.0.water.physics_seconds - physics0, sim.0.step_timings.water_s - cpu0, time.delta_secs_f64());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_speeds_deliver_their_calendar_day_without_a_frame_cap() {
        for (speed, seconds, scale) in [(Speed::X1, 30.0, 1.0), (Speed::X2, 5.0, 2.0), (Speed::X3, 1.25, 4.0)] {
            let mut clock = SimClock { speed, ..Default::default() };
            let step = SANDBOX_DT_DAYS * scale;
            let mut completed = 0.0f64;
            for _ in 0..(seconds * 60.0) as usize {
                for _ in 0..clock.queue(1.0 / 60.0, step) {
                    clock.complete(step);
                    completed += step as f64;
                }
            }
            assert!((completed - 1.0).abs() < step as f64 + 1e-6, "{speed:?}: {completed} days");
            assert!((completed + clock.acc_days - 1.0).abs() < 1e-6);
        }
    }

    #[test]
    fn frame_budget_and_pause_keep_unfinished_time() {
        let mut clock = SimClock { speed: Speed::X3, ..Default::default() };
        let step = SANDBOX_DT_DAYS;
        assert_eq!(clock.queue(1.0, step), MAX_STEPS_PER_FRAME);
        for _ in 0..2 { clock.complete(step); }
        let backlog = clock.acc_days;
        assert!((backlog - (0.8 - 2.0 * step as f64)).abs() < 1e-6);
        clock.paused = true;
        assert_eq!(clock.queue(10.0, step), 0);
        assert_eq!(clock.acc_days, backlog);
        clock.paused = false;
        assert_eq!(clock.queue(0.001, step), MAX_STEPS_PER_FRAME);
        assert!(clock.acc_days > backlog);
    }

    #[test]
    fn hydraulic_work_adapts_without_changing_calendar_rate() {
        let mut clock = SimClock { speed: Speed::X3, ..Default::default() };
        clock.record_water_cost(10.0, 0.001, 1.0 / 60.0);
        assert!(clock.hydraulic_day_s >= sim::water::GAME_HYDRAULIC_DAY_S);
        for _ in 0..30 { clock.record_water_cost(10.0, 0.02, 1.0 / 60.0); }
        assert!(clock.hydraulic_day_s < 300.0);
        clock.queue(1.25, SANDBOX_DT_DAYS);
        assert!((clock.acc_days - 1.0).abs() < 1e-6);
        clock.speed = Speed::X1;
        for _ in 0..30 { clock.record_water_cost(10.0, 0.001, 1.0 / 60.0); }
        assert_eq!(clock.hydraulic_day_s, sim::water::MAX_GAME_HYDRAULIC_DAY_S);
    }
}

