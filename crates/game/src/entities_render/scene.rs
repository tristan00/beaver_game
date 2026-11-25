use crate::terrain_render::CELL_SIZE as CELL;
use bevy::prelude::*;
use sim::grid::HeightGrid;
use sim::SimState;

/// The ways a tree is drawn; each species' row names one (`look.model`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TreeStyle {
    /// A slim pale trunk with dark bark eyes under a small round crown.
    Slim,
    /// Two white stems banded black.
    Twin,
    /// A leaning trunk under a weeping curtain.
    Weeping,
    /// A shrubby clump of stems that hangs cones in winter.
    Clump,
    /// A broad dome that flowers red before its leaves.
    Dome,
    /// A low wide spread.
    Spread,
    /// A tall bare trunk under flat tufts of needles.
    Tufted,
    /// A narrow spire of drooping boughs.
    Spire,
}

impl TreeStyle {
    pub fn named(model: &str) -> Option<TreeStyle> {
        use TreeStyle::*;
        [("slim", Slim), ("twin", Twin), ("weeping", Weeping), ("clump", Clump), ("dome", Dome), ("spread", Spread), ("tufted", Tufted), ("spire", Spire)]
            .into_iter().find(|(n, _)| *n == model).map(|s| s.1)
    }
    pub fn of(t: sim::trees::Tree) -> TreeStyle {
        Self::named(&t.row().look.model).unwrap_or_else(|| panic!("tree {}: no tree model {}", t.key(), t.row().look.model))
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Pose {
    Idle,
    Walk,
    Swim,
    Gnaw,
    Carry,
    Climb,
    /// Building: pressing mud into place.
    Work,
    /// Scooping mud or mud out of a bank.
    Dig,
    /// Sitting up, eating a plant.
    Forage,
    Rest,
    /// Running from a wolf: a flat-out bound, head low, tail up.
    Run,
    /// Taking up or putting down a load at a store, pile or site.
    Handle,
    /// Mending: patting fresh mud on, tamping it with the tail.
    Mend,
    /// A sentry on watch: sitting up tall, looking about.
    Watch,
    /// A new pair meeting at their lodge: nose to nose.
    Court,
    /// Grooming a family member beside it.
    Groom,
    /// Pinned under the wolf, struggling.
    Caught,
    /// Dead: fallen on its side, or curled up.
    Dead,
}

pub fn height_at(h: &HeightGrid, x: f32, z: f32) -> f32 {
    let (w, d) = (h.width(), h.height());
    // Grid values sit at cell centres (terrain_render::GRID_OFFSET).
    let off = crate::terrain_render::GRID_OFFSET;
    let gx = (x / CELL + w as f32 / 2.0 - off).clamp(0.0, w as f32 - 1.001);
    let gz = (z / CELL + d as f32 / 2.0 - off).clamp(0.0, d as f32 - 1.001);
    let (x0, z0) = (gx.floor() as usize, gz.floor() as usize);
    let (fx, fz) = (gx - x0 as f32, gz - z0 as f32);
    let at = |x: usize, z: usize| *h.get(x, z).unwrap_or(&0.0);
    let top = at(x0, z0) + (at(x0 + 1, z0) - at(x0, z0)) * fx;
    let bot = at(x0, z0 + 1) + (at(x0 + 1, z0 + 1) - at(x0, z0 + 1)) * fx;
    top + (bot - top) * fz
}

pub fn lodge_view(sim: &SimState) -> (Vec3, f32) {
    let cell = sim.buildings.buildings.iter()
        .find(|b| b.kind.is_home())
        .map(|b| b.pos)
        .unwrap_or((sim.world.size / 2, sim.world.size / 2));
    let xz = super::sim_view::cell_xz(sim, (cell.0 as f32 + 0.5, cell.1 as f32 + 0.5));
    (Vec3::new(xz.x, height_at(&sim.height, xz.x, xz.y), xz.y), 0.8)
}
