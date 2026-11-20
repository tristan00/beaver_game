use crate::terrain_render::CELL_SIZE as CELL;
use bevy::prelude::*;
use sim::grid::HeightGrid;
use sim::SimState;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum Species {
    Aspen,
    Maple,
    Pine,
    Willow,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Pose {
    Idle,
    Walk,
    Swim,
    Gnaw,
    Carry,
    Climb,
    Work,
    Rest,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum StructKind {
    Lodge,
    Dam,
    Canal,
    Burrow,
    Workshop,
    WaterWheel,
    Dock,
    Barge,
    BargeEmpty,
    FoodCache,
    ScentMound,
    GuardPost,
    ForagerHut,
    MudPit,
    TreeNursery,
    Warehouse,
    Storehouse,
    LoggingCamp,
    ForesterLodge,
    Mill,
    Lock,
}

pub fn height_at(h: &HeightGrid, x: f32, z: f32) -> f32 {
    let (w, d) = (h.width(), h.height());
    let gx = (x / CELL + w as f32 / 2.0).clamp(0.0, w as f32 - 1.001);
    let gz = (z / CELL + d as f32 / 2.0).clamp(0.0, d as f32 - 1.001);
    let (x0, z0) = (gx.floor() as usize, gz.floor() as usize);
    let (fx, fz) = (gx - x0 as f32, gz - z0 as f32);
    let at = |x: usize, z: usize| *h.get(x, z).unwrap_or(&0.0);
    let top = at(x0, z0) + (at(x0 + 1, z0) - at(x0, z0)) * fx;
    let bot = at(x0, z0 + 1) + (at(x0 + 1, z0 + 1) - at(x0, z0 + 1)) * fx;
    top + (bot - top) * fz
}

pub fn lodge_view(sim: &SimState) -> (Vec3, f32) {
    let cell = sim.buildings.buildings.iter()
        .find(|b| matches!(b.kind, sim::buildings::BuildingKind::Lodge | sim::buildings::BuildingKind::Burrow))
        .map(|b| b.pos)
        .unwrap_or((sim.world.size / 2, sim.world.size / 2));
    let xz = super::sim_view::cell_xz(sim, (cell.0 as f32 + 0.5, cell.1 as f32 + 0.5));
    (Vec3::new(xz.x, height_at(&sim.height, xz.x, xz.y), xz.y), 0.8)
}
