//! Ecology system (stub): vegetation density field.
use crate::grid::VegetationGrid;

/// Vegetation density field (0..=1).
#[derive(Clone, Debug)]
pub struct Ecology {
    pub vegetation: VegetationGrid,
}

impl Ecology {
    pub fn new(size: usize) -> Self { Self { vegetation: VegetationGrid::new(size, size, 0.0) } }

    /// Advances ecology by dt_days (stub: no growth yet).
    pub fn step(&mut self, _dt_days: f32) {}
}

