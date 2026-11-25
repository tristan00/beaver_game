use crate::terrain_render::CELL_SIZE as CELL;
use bevy::prelude::*;
use sim::grid::HeightGrid;
use sim::SimState;

/// One model per tree species the simulation grows.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum Species {
    Aspen,
    Birch,
    Willow,
    Alder,
    Maple,
    Beech,
    Pine,
    Spruce,
}

impl Species {
    pub const ALL: [Species; 8] = [Species::Aspen, Species::Birch, Species::Willow, Species::Alder, Species::Maple, Species::Beech, Species::Pine, Species::Spruce];
    pub fn index(self) -> usize { self as usize }
    pub fn of(t: sim::trees::Tree) -> Self {
        use sim::trees::Tree;
        match t {
            Tree::Aspen => Species::Aspen,
            Tree::Birch => Species::Birch,
            Tree::Willow => Species::Willow,
            Tree::Alder => Species::Alder,
            Tree::Maple => Species::Maple,
            Tree::Beech => Species::Beech,
            Tree::Pine => Species::Pine,
            Tree::Spruce => Species::Spruce,
        }
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

// Models for retired buildings stay available for when the building list grows again.
#[allow(dead_code)]
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum StructKind {
    Lodge,
    Dam,
    Canal,
    StickPile,
    FoodCache,
    PebbleHeap,
    Forester,
    HaulMound,
    Lookout,
    RaftLanding,
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
        .find(|b| b.kind == sim::buildings::BuildingKind::Lodge)
        .map(|b| b.pos)
        .unwrap_or((sim.world.size / 2, sim.world.size / 2));
    let xz = super::sim_view::cell_xz(sim, (cell.0 as f32 + 0.5, cell.1 as f32 + 0.5));
    (Vec3::new(xz.x, height_at(&sim.height, xz.x, xz.y), xz.y), 0.8)
}
