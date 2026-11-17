//! Pure mapping from sim state to what is drawn: where a beaver is, which pose its task asks for,
//! what it holds, and which models stand for resources and buildings. No Bevy ECS here, so it can
//! be tested against a stepped `SimState`.
//!
//! Pose table (see [`pose_of`]):
//!
//! | sim state                                   | pose                        |
//! |---------------------------------------------|-----------------------------|
//! | moving, gait `Swim`                         | `Swim` (load still held)    |
//! | moving, gait `Climb` (a dam)                | `Climb`                     |
//! | moving on land with a load                  | `Carry`                     |
//! | moving on land                              | `Walk`                      |
//! | still in water                              | `Swim` (treading water)     |
//! | `Collect` a standing tree (felling)         | `Gnaw`                      |
//! | `Collect` anything else, `Build`            | `Work`                      |
//! | `Rest` (outside the lodge)                  | `Rest`                      |
//! | everything else                             | `Idle`                      |
use super::scene::{height_at, Pose};
use crate::terrain_render::CELL_SIZE as CELL;
use bevy::prelude::*;
use sim::agents::{Agent, Task, MOVE_COST_PER_DAY};
use sim::buildings::Good;
use sim::nav::{Gait, Surface, CLIMB_COST, SWIM_COST, WALK_COST};
use sim::resources::ResourceKind;
use sim::SimState;

/// Size of everything drawn from the sim relative to the models' native size (a beaver is one cell
/// long, a lodge fills its 3x3 footprint).
pub const WORLD_SCALE: f32 = 0.5;
/// Height of a finished dam crest above the ground; climbing beavers walk along it.
pub const DAM_TOP: f32 = 0.8;

/// World x/z of continuous cell coordinates (cell `(x, y)` spans `x..x+1`, `y..y+1`).
pub fn cell_xz(sim: &SimState, c: (f32, f32)) -> Vec2 {
    let (w, h) = (sim.height.width() as f32, sim.height.height() as f32);
    Vec2::new((c.0 - w / 2.0) * CELL, (c.1 - h / 2.0) * CELL)
}

/// How far above the terrain a beaver stands in `cell`: the water depth when swimming, the dam crest
/// height on a dam.
fn lift_in(sim: &SimState, cell: (usize, usize)) -> f32 {
    match sim.nav.surface_at(cell) {
        Some(Surface::Dam) => DAM_TOP,
        Some(Surface::Water | Surface::LodgeEntrance) => sim.water.depth.get(cell.0, cell.1).copied().unwrap_or(0.0).max(0.0),
        _ => 0.0,
    }
}

/// Height a beaver stands at in the centre of `cell`.
pub fn stand_y(sim: &SimState, cell: (usize, usize)) -> f32 {
    let p = cell_xz(sim, (cell.0 as f32 + 0.5, cell.1 as f32 + 0.5));
    height_at(&sim.height, p.x, p.y) + lift_in(sim, cell)
}

fn gait_cost(g: Gait) -> f32 {
    match g {
        Gait::Walk => WALK_COST,
        Gait::Swim => SWIM_COST,
        Gait::Climb => CLIMB_COST,
    }
}

/// Whether the agent is part-way along a route.
pub fn is_moving(ag: &Agent) -> bool {
    ag.path.as_ref().is_some_and(|p| !p.steps.is_empty())
}

/// The agent's position in the world: its cell centre moved `step_frac` of the way to the next
/// cell on its path (the sim's own measure of progress), plus `pending_days` of sim time that
/// has elapsed since the last step (0 in a stepped scene). Returns (x, y, z).
pub fn agent_world_pos(sim: &SimState, ag: &Agent, pending_days: f32) -> Vec3 {
    let here = (ag.cell.0 as f32 + 0.5, ag.cell.1 as f32 + 0.5);
    let (mut frac, mut next) = (0.0, ag.cell);
    if let Some(&(n, gait)) = ag.path.as_ref().and_then(|p| p.steps.first()) {
        // Never extrapolate into the next cell: the sim may still refuse the move.
        frac = (ag.step_frac + pending_days * MOVE_COST_PER_DAY / gait_cost(gait)).clamp(0.0, 0.98);
        next = n;
    }
    let to = (next.0 as f32 + 0.5, next.1 as f32 + 0.5);
    let xz = cell_xz(sim, (here.0 + (to.0 - here.0) * frac, here.1 + (to.1 - here.1) * frac));
    // terrain under the beaver plus a lift that eases between the two cells (up the dam, into the water)
    let (l0, l1) = (lift_in(sim, ag.cell), lift_in(sim, next));
    Vec3::new(xz.x, height_at(&sim.height, xz.x, xz.y) + l0 + (l1 - l0) * frac, xz.y)
}

/// The pose the agent's sim state asks for.
pub fn pose_of(sim: &SimState, ag: &Agent) -> Pose {
    if is_moving(ag) {
        return match ag.gait {
            Gait::Swim => Pose::Swim,
            Gait::Climb => Pose::Climb,
            Gait::Walk if ag.load.is_some() => Pose::Carry,
            Gait::Walk => Pose::Walk,
        };
    }
    if matches!(sim.nav.surface_at(ag.cell), Some(Surface::Water)) {
        return Pose::Swim;
    }
    match &ag.task {
        Task::Collect { target, .. } => match sim.resources.get(*target).map(|r| r.kind) {
            Some(ResourceKind::Tree) => Pose::Gnaw,
            _ => Pose::Work,
        },
        Task::Build { .. } => Pose::Work,
        Task::Rest => Pose::Rest,
        _ => Pose::Idle,
    }
}

/// Whether the agent is over water right now: its cell is water, or it is past halfway into a water cell.
pub fn is_wet(sim: &SimState, ag: &Agent) -> bool {
    if matches!(sim.nav.surface_at(ag.cell), Some(Surface::Water)) {
        return true;
    }
    match ag.path.as_ref().and_then(|p| p.steps.first()) {
        Some(&(n, _)) => ag.step_frac >= 0.5 && matches!(sim.nav.surface_at(n), Some(Surface::Water)),
        None => false,
    }
}

/// The load held in the mouth, if any.
pub fn load_of(ag: &Agent) -> Option<(Good, f32)> {
    ag.load.map(|l| (l.good, l.amount))
}

/// Heading (radians about +Y, 0 = +Z) the agent faces, or `None` to keep its current one.
pub fn facing_of(sim: &SimState, ag: &Agent) -> Option<f32> {
    let here = (ag.cell.0 as f32 + 0.5, ag.cell.1 as f32 + 0.5);
    let toward = |c: (f32, f32)| {
        let d = (c.0 - here.0, c.1 - here.1);
        (d.0.abs() + d.1.abs() > 1e-3).then(|| d.0.atan2(d.1))
    };
    if let Some(&(n, _)) = ag.path.as_ref().and_then(|p| p.steps.first()) {
        return toward((n.0 as f32 + 0.5, n.1 as f32 + 0.5));
    }
    match &ag.task {
        Task::Collect { target, .. } => sim.resources.get(*target).and_then(|r| toward((r.cell.0 as f32 + 0.5, r.cell.1 as f32 + 0.5))),
        Task::Build { site } => sim.buildings.building(*site).and_then(|b| {
            let (w, h) = b.kind.spec().footprint;
            toward((b.pos.0 as f32 + w as f32 / 2.0, b.pos.1 as f32 + h as f32 / 2.0))
        }),
        _ => None,
    }
}
