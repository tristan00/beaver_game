//! Dam preview (owner: `dam`): what a candidate dam would do, as a pure function of the state.

use crate::buildings::{Amount, BuildingId};
use crate::water::Cell;

#[derive(Clone, Debug, PartialEq)]
pub struct DamPreview {
    /// Rise of the pond surface above the current level at the dam, in world units.
    pub rise: f32,
    /// Cells that would be under water afterwards.
    pub flooded: Vec<Cell>,
    pub flooded_trees: usize,
    pub flooded_buildings: Vec<BuildingId>,
    pub cost: Vec<Amount>,
    /// Inflow pushing on the dam, water units per day (main river above tributary).
    pub pressure: f32,
    /// Days for the pond to reach `rise` at current inflow.
    pub fill_days: f32,
}

/// Evaluates a dam of `height` on `cells`; `None` if the cells are not a valid dam line. (`dam` task.)
pub fn preview(_sim: &crate::SimState, _cells: &[Cell], _height: f32) -> Option<DamPreview> {
    None
}
