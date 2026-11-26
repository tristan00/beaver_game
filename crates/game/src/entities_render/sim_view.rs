use super::scene::{height_at, Pose};
use crate::terrain_render::CELL_SIZE as CELL;
use bevy::prelude::*;
use sim::agents::{Agent, Task, MOVE_COST_PER_DAY};
use sim::buildings::Good;
use sim::nav::{Gait, Surface, CLIMB_COST, SWIM_COST, WALK_COST};

use sim::SimState;

pub const WORLD_SCALE: f32 = 0.5;
pub const DAM_TOP: f32 = 0.8;

pub fn cell_xz(sim: &SimState, c: (f32, f32)) -> Vec2 {
    let (w, h) = (sim.height.width() as f32, sim.height.height() as f32);
    Vec2::new((c.0 - w / 2.0) * CELL, (c.1 - h / 2.0) * CELL)
}

/// Whether a beaver on `cell` rides on the water's (or ice's) surface rather than the bare ground.
pub fn on_water(sim: &SimState, cell: (usize, usize)) -> bool {
    matches!(sim.nav.surface_at(cell), Some(Surface::Water | Surface::Canal | Surface::LodgeEntrance | Surface::Lock))
        || sim.water.ice.get(cell.0, cell.1).is_some_and(|&t| t >= sim::tuning::water::ICE_WALK_M)
}

pub(super) fn land_lift(sim: &SimState, cx: i32, cy: i32) -> f32 {
    if cx >= 0 && cy >= 0
        && sim.water.ice.get(cx as usize, cy as usize).is_some_and(|&t| t >= sim::tuning::water::ICE_WALK_M)
    {
        return sim.water.depth.get(cx as usize, cy as usize).copied().unwrap_or(0.0).max(0.0) + 0.02;
    }
    -crate::terrain_render::channel_cut(sim, cx, cy)
}

fn lift_in(sim: &SimState, cell: (usize, usize)) -> f32 {
    match sim.nav.surface_at(cell) {
        Some(Surface::Dam) => sim.water.dam_height.get(cell.0, cell.1).copied().filter(|h| *h > 0.0).unwrap_or(DAM_TOP),
        Some(Surface::Water | Surface::Canal | Surface::LodgeEntrance) => sim.water.depth.get(cell.0, cell.1).copied().unwrap_or(0.0).max(0.0),
        // In a lock's chamber: water depth, or half way up its gate where the gate holds back a drop.
        Some(Surface::Lock) => {
            let depth = sim.water.depth.get(cell.0, cell.1).copied().unwrap_or(0.0).max(0.0);
            let gate = sim.water.dam_height.get(cell.0, cell.1).copied().unwrap_or(0.0).max(0.0) * 0.5;
            depth.max(gate)
        }
        _ => land_lift(sim, cell.0 as i32, cell.1 as i32),
    }
}

pub fn stand_y(sim: &SimState, cell: (usize, usize)) -> f32 {
    let p = cell_xz(sim, (cell.0 as f32 + 0.5, cell.1 as f32 + 0.5));
    height_at(&sim.height, p.x, p.y) + lift_in(sim, cell)
}

pub fn gait_cost(g: Gait) -> f32 {
    match g {
        Gait::Walk => WALK_COST,
        Gait::Swim => SWIM_COST,
        Gait::Climb => CLIMB_COST,
    }
}

pub fn is_moving(ag: &Agent) -> bool {
    !ag.trail.is_empty() || ag.path.as_ref().is_some_and(|p| !p.steps.is_empty())
}

/// Where a beaver is drawn: on its way from cell to cell, or wherever it is in a lock's working
/// (`raft landing::lock_pose`).
pub fn agent_world_pos(sim: &SimState, ag: &Agent, pending_days: f32) -> Vec3 {
    let base = agent_cells_pos(sim, ag, pending_days);
    super::raft_landing::lock_pose(sim, ag, pending_days, base).map_or(base, |p| p.0)
}

fn agent_cells_pos(sim: &SimState, ag: &Agent, pending_days: f32) -> Vec3 {
    let here = (ag.cell.0 as f32 + 0.5, ag.cell.1 as f32 + 0.5);
    let (mut frac, mut next) = (0.0, ag.cell);
    if let Some(&(n, _)) = ag.path.as_ref().and_then(|p| p.steps.first()) {
        frac = step_progress(sim, ag, n, pending_days);
        next = n;
    }
    let to = (next.0 as f32 + 0.5, next.1 as f32 + 0.5);
    let xz = cell_xz(sim, (here.0 + (to.0 - here.0) * frac, here.1 + (to.1 - here.1) * frac));
    let y = if on_water(sim, ag.cell) || on_water(sim, next) {
        let (y0, y1) = (stand_y(sim, ag.cell), stand_y(sim, next));
        y0 + (y1 - y0) * frac
    } else {
        let (l0, l1) = (lift_in(sim, ag.cell), lift_in(sim, next));
        height_at(&sim.height, xz.x, xz.y) + l0 + (l1 - l0) * frac
    };
    Vec3::new(xz.x, y, xz.y)
}

pub fn moving_gait(ag: &Agent) -> Gait {
    [Gait::Climb, Gait::Swim].into_iter().find(|g| ag.trail.iter().any(|t| t.1 == *g)).unwrap_or(ag.gait)
}

pub fn pose_of(sim: &SimState, ag: &Agent, moving: bool) -> Pose {
    if moving {
        return match moving_gait(ag) {
            Gait::Swim => Pose::Swim,
            Gait::Climb => Pose::Climb,
            Gait::Walk if sim.agents.fleeing.contains(&ag.id) => Pose::Run,
            Gait::Walk if ag.load.is_some() => Pose::Carry,
            Gait::Walk => Pose::Walk,
        };
    }
    // Packing a dam cell is work, even standing in the stream.
    if matches!(ag.task, Task::Work { kind: sim::agents::WorkKind::Dam, .. }) {
        return Pose::Work;
    }
    if matches!(sim.nav.surface_at(ag.cell), Some(Surface::Water | Surface::Canal | Surface::Lock)) {
        return Pose::Swim;
    }
    use sim::agents::WorkKind;
    match &ag.task {
        Task::Collect { target, .. } => match sim.resources.get(*target).map(|r| r.kind) {
            Some(k) if k.row().pose == "Gnaw" => Pose::Gnaw,
            Some(k) if k.row().pose == "Dig" => Pose::Dig,
            Some(k) if k.row().pose == "Forage" => Pose::Forage,
            _ => Pose::Work,
        },
        Task::Work { kind: WorkKind::Build | WorkKind::Dam | WorkKind::Gate, .. } => Pose::Work,
        Task::Work { kind: WorkKind::Mend, .. } => Pose::Mend,
        Task::Work { kind: WorkKind::Dig, .. } | Task::Plant { .. } => Pose::Dig,
        Task::Seed { .. } => Pose::Forage,
        Task::Move { .. } => Pose::Handle,
        Task::Watch { .. } => Pose::Watch,
        Task::Raft { leg: 2 | 4, .. } => Pose::Handle,
        Task::Raft { .. } => Pose::Swim,
        Task::Rest => Pose::Rest,
        Task::Idle => Pose::Idle,
    }
}

/// How far through its step to the next cell a beaver is, `pending_days` after the simulation
/// last moved it.
pub(super) fn step_progress(sim: &SimState, ag: &Agent, next: (usize, usize), pending_days: f32) -> f32 {
    let pace = sim::agents::travel_pace(&sim.nav, ag);
    let gait = ag.path.as_ref().and_then(|p| p.steps.first()).filter(|s| s.0 == next).map_or(ag.gait, |s| s.1);
    let cost = sim.nav.move_cost(ag.cell, next, pace).unwrap_or_else(|| gait_cost(gait) * sim::nav::step_length(ag.cell, next)).max(1e-3);
    ((ag.step_frac + pending_days * MOVE_COST_PER_DAY * pace) / cost).clamp(0.0, 1.0)
}

pub fn is_wet(sim: &SimState, ag: &Agent) -> bool {
    let wet = |c| matches!(sim.nav.surface_at(c), Some(Surface::Water | Surface::Canal | Surface::Lock));
    if wet(ag.cell) {
        return true;
    }
    match ag.path.as_ref().and_then(|p| p.steps.first()) {
        Some(&(n, _)) => step_progress(sim, ag, n, 0.0) >= 0.5 && wet(n),
        None => false,
    }
}

pub(super) fn is_pushing_raft(ag: &Agent) -> bool {
    matches!(ag.task, Task::Raft { leg: 1 | 3 | 5, .. })
}

pub fn load_of(ag: &Agent) -> Option<(Good, f32)> {
    if matches!(ag.task, Task::Raft { .. }) { None } else { ag.load.map(|l| (l.good, l.amount)) }
}

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
        Task::Work { site, kind: sim::agents::WorkKind::Build | sim::agents::WorkKind::Mend, .. } => sim.buildings.building(*site).and_then(|b| {
            let (w, h) = b.kind.spec().footprint;
            toward((b.pos.0 as f32 + w as f32 / 2.0, b.pos.1 as f32 + h as f32 / 2.0))
        }),
        _ => None,
    }
}
