//! Water system (stub): water depth field.
use crate::grid::WaterGrid;

/// Water depth field (0 = dry).
#[derive(Clone, Debug)]
pub struct Water {
    pub depth: WaterGrid,
}

impl Water {
    pub fn new(size: usize) -> Self { Self { depth: WaterGrid::new(size, size, 0.0) } }

    /// Advances water by dt_days (stub: no flow yet).
    pub fn step(&mut self, _dt_days: f32) {}
}

