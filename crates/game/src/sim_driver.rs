use crate::state::Sim;
use bevy::prelude::*;

pub const FIXED_DT_DAYS: f32 = sim::STEP_DAYS;
pub const BASE_DAYS_PER_SECOND: f32 = 1.0 / 300.0;
pub const MAX_STEPS_PER_FRAME: u32 = 8;
const FRAME_SIM_BUDGET_S: f32 = 0.012;
const SANDBOX_DT_DAYS: f32 = 10.0 / 86_400.0;

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
    acc_days: f32,
}

impl Default for SimClock {
    fn default() -> Self {
        SimClock { paused: false, speed: Speed::X1, acc_days: 0.0 }
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
        self.acc_days
    }
    pub fn advance(&mut self, real_dt: f32) -> u32 {
        self.advance_with_step(real_dt, FIXED_DT_DAYS, MAX_STEPS_PER_FRAME)
    }

    fn advance_with_step(&mut self, real_dt: f32, step: f32, max_steps: u32) -> u32 {
        if self.paused || !real_dt.is_finite() || real_dt <= 0.0 {
            return 0;
        }
        self.acc_days += real_dt * BASE_DAYS_PER_SECOND * self.speed.multiplier();
        let n = (self.acc_days / step) as u32;
        self.acc_days -= n as f32 * step;
        if n > max_steps {
            self.acc_days = 0.0;
            return max_steps;
        }
        n
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

pub(crate) fn step_sim(time: Res<Time>, mut clock: ResMut<SimClock>, mut sim: ResMut<Sim>) {
    if !sim.0.is_running() {
        return;
    }
    let started = std::time::Instant::now();
    // Short slices keep a rainstorm or breach from monopolizing one render frame.
    let dt = if sim.0.sandbox {
        SANDBOX_DT_DAYS * match clock.speed { Speed::X1 => 1.0, Speed::X2 => 2.0, Speed::X3 => 3.0 }
    } else { FIXED_DT_DAYS };
    let count = if sim.0.sandbox { clock.advance_with_step(time.delta_secs(), dt, 32) } else { clock.advance(time.delta_secs()) };
    for _ in 0..count {
        if let Err(e) = sim.0.step(dt) {
            error!("sim step failed: {e}");
            clock.paused = true;
            return;
        }
        if started.elapsed().as_secs_f32() > FRAME_SIM_BUDGET_S {
            break;
        }
    }
}

