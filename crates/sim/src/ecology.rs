use crate::SimError;
use crate::grid::{Grid, VegetationGrid, WaterGrid};
pub use crate::tuning::ecology::*;

const GRID_EVERY_DAYS: f32 = 0.25;
/// Plant cover is drawn in this many steps from bare to full.
pub const COVER_STEPS: f32 = 64.0;

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct Ecology {
    /// Tree cover density (written by `forestry`), with tile change stamps for terrain rendering.
    pub vegetation: VegetationGrid,
    pub flood_days: Grid<f32>,
    pending_days: f32,
    /// The cover each tile of `vegetation` reads, as it was when the tile was last stamped.
    #[serde(skip)]
    told: crate::grid::Own<crate::grid::Told<f32>>,
}

crate::grid::clone_fields!(Ecology { vegetation, flood_days, pending_days, told });

impl Ecology {
    pub fn new(size: usize) -> Self {
        Self {
            vegetation: Grid::new(size, size, 0.0),
            flood_days: Grid::new(size, size, 0.0),
            pending_days: 0.0,
            told: Default::default(),
        }
    }

    pub fn size(&self) -> usize { self.vegetation.width() }

    pub fn set_forest(&mut self, density: &Grid<f32>) {
        let n = self.size();
        if density.width() != n || density.height() != n { return; }
        for (cell, &d) in self.vegetation.cells_and_marks().0.iter_mut().zip(density.as_slice()) {
            *cell = d.clamp(0.0, 1.0);
        }
        self.told.0.note(&mut self.vegetation, None, |a, b| (a - b).abs() > 1.0 / COVER_STEPS);
    }

    pub fn step(&mut self, dt_days: f32, water: &WaterGrid) -> Result<(), SimError> {
        if !dt_days.is_finite() || dt_days < 0.0 {
            return Err(SimError::InvalidDt(dt_days));
        }
        let n = self.size();
        if water.width() != n || water.height() != n {
            return Err(SimError::InvalidCommand(format!(
                "water grid {}x{} does not match ecology size {n}", water.width(), water.height())));
        }
        if dt_days == 0.0 { return Ok(()); }
        self.pending_days += dt_days;
        if self.pending_days < GRID_EVERY_DAYS { return Ok(()); }
        let due = std::mem::take(&mut self.pending_days);
        for (fd, &depth) in self.flood_days.as_mut_slice().iter_mut().zip(water.as_slice()) {
            *fd = if depth > FLOOD_DEPTH { *fd + due } else { (*fd - FLOOD_DAYS_DRAIN_RATE * due).max(0.0) };
        }
        Ok(())
    }
}
