//! Navigation grid for beaver agents (owner: `nav`). Pure data plus path planning; no task logic.
//! Rules: trees and buildings are solid; dams are climbable (slow); lodges are entered only from
//! their underwater entrance cell; water is swimmable and usually faster; one agent per cell.

use crate::colony::BeaverId;
use crate::grid::Grid;
use crate::water::Cell;

/// What an agent can do in a cell.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Surface {
    Land,
    Water,
    /// Walkable only by climbing; cost multiplier `CLIMB_COST`.
    Dam,
    /// Impassable (tree, building body, stone).
    Solid,
    /// Underwater lodge door; the only way into the lodge interior.
    LodgeEntrance,
    /// Lodge interior cell; reachable only through its entrance.
    LodgeInside,
}

/// How an agent is moving; drives animation in `render_agents`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Gait {
    Walk,
    Swim,
    Climb,
}

pub const CLIMB_COST: f32 = 3.0;
pub const WALK_COST: f32 = 1.0;
pub const SWIM_COST: f32 = 0.6;

/// A planned route: cells from next step to goal, with the gait for each step.
#[derive(Clone, Debug, PartialEq)]
pub struct Path {
    pub steps: Vec<(Cell, Gait)>,
    pub cost: f32,
}

/// Where an agent wants to end up.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Goal {
    /// Stand on this cell.
    Cell(Cell),
    /// Stand on any cell adjacent to this (solid) cell, e.g. beside a tree or site.
    Beside(Cell),
    /// Enter lodge interior at this cell (route must pass its entrance).
    Inside(Cell),
}

/// Navigation state: surface per cell, plus the cell reservations that prevent overlaps.
#[derive(Clone, Debug)]
pub struct NavGrid {
    pub surface: Grid<Surface>,
    /// Which agent holds each cell (`None` = free).
    pub reserved: Grid<Option<BeaverId>>,
}

impl NavGrid {
    pub fn new(width: usize, height: usize) -> Self {
        Self { surface: Grid::new(width, height, Surface::Land), reserved: Grid::new(width, height, None) }
    }
    /// Rebuilds `surface` from terrain, water, trees and buildings. (`nav` task.)
    pub fn rebuild(&mut self, _sim: &crate::SimState) {}
    /// Cheapest path honouring the rules above, treating other agents' reservations as blocked.
    /// `None` if unreachable. (`nav` task.)
    pub fn find_path(&self, _who: BeaverId, _from: Cell, _goal: Goal) -> Option<Path> {
        None
    }
    /// Reserves `cell` for `who`; false if another agent holds it. Idempotent for the holder.
    pub fn reserve(&mut self, who: BeaverId, cell: Cell) -> bool {
        match self.reserved.get_mut(cell.0, cell.1) {
            Some(slot) if slot.is_none() || *slot == Some(who) => {
                *slot = Some(who);
                true
            }
            _ => false,
        }
    }
    /// Releases `cell` if `who` holds it.
    pub fn release(&mut self, who: BeaverId, cell: Cell) {
        if let Some(slot) = self.reserved.get_mut(cell.0, cell.1) {
            if *slot == Some(who) {
                *slot = None;
            }
        }
    }
    pub fn surface_at(&self, cell: Cell) -> Option<Surface> {
        self.surface.get(cell.0, cell.1).copied()
    }
}
