//! Pure, deterministic simulation core (no rendering dependencies).
//! `SimState` composes one struct per system; the game crate only reads it,
//! advances it with `step`, and mutates it through `apply`.

pub mod buildings;
pub mod colony;
pub mod ecology;
pub mod events;
pub mod grid;
pub mod region;
pub mod tech;
pub mod water;
pub mod worldgen;

use std::fmt;

pub use grid::Grid;

/// Errors returned by `SimState` operations. Nothing fails silently.
#[derive(Clone, Debug, PartialEq)]
pub enum SimError {
    /// `dt_days` was NaN, infinite or negative.
    InvalidDt(f32),
    /// A command carried an invalid argument.
    InvalidCommand(String),
}

impl fmt::Display for SimError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SimError::InvalidDt(dt) => write!(f, "invalid dt_days: {dt} (must be finite and >= 0)"),
            SimError::InvalidCommand(m) => write!(f, "invalid command: {m}"),
        }
    }
}
impl std::error::Error for SimError {}

/// Player/UI requests that mutate the simulation.
#[derive(Clone, Debug, PartialEq)]
pub enum Command {
    /// Rebuild the world from a new seed (resets time).
    Regenerate { seed: u64 },
}

/// Complete simulation state.
pub struct SimState {
    pub seed: u64,
    /// Elapsed simulated days.
    pub time_days: f64,
    pub height: grid::HeightGrid,
    pub water: water::Water,
    pub ecology: ecology::Ecology,
    pub colony: colony::Colony,
    pub buildings: buildings::Buildings,
    pub tech: tech::Tech,
    pub region: region::Region,
    pub events: events::Events,
}

impl SimState {
    /// Builds a world deterministically from `seed`.
    pub fn new(seed: u64) -> Self {
        let height = worldgen::WorldGen::new(seed).heightmap();
        let size = height.width();
        Self {
            seed,
            time_days: 0.0,
            height,
            water: water::Water::new(size),
            ecology: ecology::Ecology::new(size),
            colony: Default::default(),
            buildings: Default::default(),
            tech: Default::default(),
            region: Default::default(),
            events: Default::default(),
        }
    }

    /// Advances every system by `dt_days`. Errors if `dt_days` is not finite and >= 0.
    pub fn step(&mut self, dt_days: f32) -> Result<(), SimError> {
        if !dt_days.is_finite() || dt_days < 0.0 {
            return Err(SimError::InvalidDt(dt_days));
        }
        self.time_days += dt_days as f64;
        self.water.step(dt_days);
        self.ecology.step(dt_days);
        self.colony.step(dt_days);
        self.buildings.step(dt_days);
        self.tech.step(dt_days);
        self.region.step(dt_days);
        self.events.step(dt_days);
        Ok(())
    }

    /// Applies a command. Errors on invalid arguments, leaving state unchanged.
    pub fn apply(&mut self, cmd: Command) -> Result<(), SimError> {
        match cmd {
            Command::Regenerate { seed } => {
                *self = SimState::new(seed);
                Ok(())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_is_deterministic() {
        assert_eq!(SimState::new(42).height, SimState::new(42).height);
    }

    #[test]
    fn step_advances_time() {
        let mut s = SimState::new(1);
        s.step(0.5).unwrap();
        s.step(0.25).unwrap();
        assert_eq!(s.time_days, 0.75);
    }

    #[test]
    fn step_rejects_bad_dt() {
        let mut s = SimState::new(1);
        for dt in [-1.0, f32::NAN, f32::INFINITY] {
            assert!(matches!(s.step(dt), Err(SimError::InvalidDt(_))));
        }
        assert_eq!(s.time_days, 0.0);
    }

    #[test]
    fn regenerate_changes_seed_and_resets_time() {
        let mut s = SimState::new(1);
        s.step(2.0).unwrap();
        s.apply(Command::Regenerate { seed: 9 }).unwrap();
        assert_eq!(s.seed, 9);
        assert_eq!(s.time_days, 0.0);
        assert_eq!(s.height, SimState::new(9).height);
    }
}
