//! Region system (stub): owned state and per-tick update.

/// State of the region system.
#[derive(Clone, Debug, Default)]
pub struct Region;

impl Region {
    /// Advances the system by dt_days (finite, >= 0; validated by SimState::step).
    pub fn step(&mut self, _dt_days: f32) {}
}

