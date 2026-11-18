//! Drives `SimState` on a fixed timestep with pause and 1x/2x/3x speeds.
//!
//! Real frame time is accumulated and converted to simulated days; the sim only ever advances in
//! whole [`FIXED_DT_DAYS`] steps, so results do not depend on the frame rate.
use crate::state::Sim;
use bevy::prelude::*;

/// Length of one sim step in days.
pub const FIXED_DT_DAYS: f32 = 0.05;
/// Simulated days per real second at 1x.
pub const BASE_DAYS_PER_SECOND: f32 = 0.4;
/// Steps per frame cap; excess time is dropped instead of spiralling.
pub const MAX_STEPS_PER_FRAME: u32 = 8;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Speed {
    X1,
    X2,
    X3,
}

impl Speed {
    pub const ALL: [Speed; 3] = [Speed::X1, Speed::X2, Speed::X3];
    pub fn multiplier(self) -> f32 {
        match self {
            Speed::X1 => 1.0,
            Speed::X2 => 2.0,
            Speed::X3 => 3.0,
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Speed::X1 => "1x",
            Speed::X2 => "2x",
            Speed::X3 => "3x",
        }
    }
}

/// Time controls and the fixed-step accumulator.
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
    /// Picks a speed and resumes.
    pub fn set_speed(&mut self, s: Speed) {
        self.speed = s;
        self.paused = false;
    }
    /// Sim days that have accumulated since the last fixed step (for drawing between steps).
    pub fn pending_days(&self) -> f32 {
        self.acc_days
    }
    /// Adds `real_dt` seconds and returns how many fixed steps to run now (0 while paused).
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

fn time_keys(keys: Res<ButtonInput<KeyCode>>, mut clock: ResMut<SimClock>) {
    if keys.just_pressed(KeyCode::Space) {
        clock.toggle_pause();
    }
    for (k, s) in [(KeyCode::Digit1, Speed::X1), (KeyCode::Digit2, Speed::X2), (KeyCode::Digit3, Speed::X3)] {
        if keys.just_pressed(k) {
            clock.set_speed(s);
        }
    }
}

/// The world only runs once the colony is founded; before that there is nothing to simulate.
fn step_sim(time: Res<Time>, mut clock: ResMut<SimClock>, mut sim: ResMut<Sim>) {
    if !sim.0.colony.is_founded() {
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
        // 10 s at 0.4 d/s = 4 days = 80 steps.
        assert!((one as i32 - 80).abs() <= 1, "{one}");
        assert!((three as i32 - 240).abs() <= 1, "{three}");
    }

    #[test]
    fn there_are_three_speeds_and_no_10x() {
        let m: Vec<f32> = Speed::ALL.iter().map(|s| s.multiplier()).collect();
        assert_eq!(m, vec![1.0, 2.0, 3.0]);
        assert_eq!(Speed::ALL.map(|s| s.label()), ["1x", "2x", "3x"]);
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
