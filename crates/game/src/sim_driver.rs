use crate::state::Sim;
use bevy::prelude::*;

pub const FIXED_DT_DAYS: f32 = sim::STEP_DAYS;
pub const BASE_DAYS_PER_SECOND: f32 = 1.0 / 300.0;
pub const MAX_STEPS_PER_FRAME: u32 = 8;

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
        if self.paused || !real_dt.is_finite() || real_dt <= 0.0 {
            return 0;
        }
        self.acc_days += real_dt * BASE_DAYS_PER_SECOND * self.speed.multiplier();
        let n = (self.acc_days / FIXED_DT_DAYS) as u32;
        self.acc_days -= n as f32 * FIXED_DT_DAYS;
        if n > MAX_STEPS_PER_FRAME {
            self.acc_days = 0.0;
            return MAX_STEPS_PER_FRAME;
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
    for _ in 0..clock.advance(time.delta_secs()) {
        if let Err(e) = sim.0.step(FIXED_DT_DAYS) {
            error!("sim step failed: {e}");
            clock.paused = true;
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn speeds_scale_steps() {
        let mut c = SimClock::default();
        let one: u32 = (0..600).map(|_| c.advance(1.0 / 60.0)).sum();
        let mut c3 = SimClock::default();
        c3.set_speed(Speed::X3);
        let three: u32 = (0..600).map(|_| c3.advance(1.0 / 60.0)).sum();
        let want = |speed: Speed| (10.0 * BASE_DAYS_PER_SECOND * speed.multiplier() / sim::STEP_DAYS).round() as i32;
        assert!((one as i32 - want(Speed::X1)).abs() <= 1, "{one}");
        assert!((three as i32 - want(Speed::X3)).abs() <= 1, "{three}");
    }


    #[test]
    fn pause_stops_steps_and_cap_holds() {
        let mut c = SimClock::default();
        c.toggle_pause();
        assert_eq!(c.advance(5.0), 0);
        c.set_speed(Speed::X3);
        assert!(!c.paused);
        assert_eq!(c.advance(100.0), MAX_STEPS_PER_FRAME);
    }
}
