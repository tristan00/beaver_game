use crate::state::Sim;
use bevy::prelude::*;

pub const FIXED_DT_DAYS: f32 = sim::STEP_DAYS;
pub const BASE_DAYS_PER_SECOND: f32 = 1.0 / 30.0;
pub const MAX_STEPS_PER_FRAME: u32 = 64;
const FRAME_SIM_BUDGET_S: f32 = 0.012;
// At maximum speed, spend more of a frame advancing large river maps. Keep
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
            Speed::X2 => 30.0,
            Speed::X3 => 180.0,
        }
    }
}

#[derive(Resource, Debug)]
pub struct SimClock {
    pub paused: bool,
    pub speed: Speed,
    acc_days: f64,
}

impl Default for SimClock {
    fn default() -> Self {
        SimClock { paused: false, speed: Speed::X1, acc_days: 0.0 }
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
    let budget = if clock.speed == Speed::X3 { FAST_FRAME_SIM_BUDGET_S } else { FRAME_SIM_BUDGET_S };
    // Speed changes the number of fixed steps, not the duration of a step.
    let dt = FIXED_DT_DAYS;
    let count = clock.queue(time.delta_secs_f64(), dt);
    for _ in 0..count {
        if let Err(e) = sim.0.step_game(dt) {
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
