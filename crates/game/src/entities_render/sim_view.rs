use super::scene::{height_at, Pose};
use crate::terrain_render::CELL_SIZE as CELL;
use bevy::prelude::*;
use sim::agents::{Agent, Task, MOVE_COST_PER_DAY, LADEN_PACE};
use sim::buildings::Good;
use sim::nav::{Gait, Surface, CLIMB_COST, SWIM_COST, WALK_COST};
use sim::resources::ResourceKind;
use sim::SimState;

pub const WORLD_SCALE: f32 = 0.5;
pub const DAM_TOP: f32 = 0.8;

pub fn cell_xz(sim: &SimState, c: (f32, f32)) -> Vec2 {
    let (w, h) = (sim.height.width() as f32, sim.height.height() as f32);
    Vec2::new((c.0 - w / 2.0) * CELL, (c.1 - h / 2.0) * CELL)
}

fn lift_in(sim: &SimState, cell: (usize, usize)) -> f32 {
    match sim.nav.surface_at(cell) {
        Some(Surface::Dam) => sim.water.dam_height.get(cell.0, cell.1).copied().filter(|h| *h > 0.0).unwrap_or(DAM_TOP),
        Some(Surface::Water | Surface::LodgeEntrance) => sim.water.depth.get(cell.0, cell.1).copied().unwrap_or(0.0).max(0.0),
        _ => 0.0,
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

pub fn agent_world_pos(sim: &SimState, ag: &Agent, pending_days: f32) -> Vec3 {
    let here = (ag.cell.0 as f32 + 0.5, ag.cell.1 as f32 + 0.5);
    let (mut frac, mut next) = (0.0, ag.cell);
    if let Some(&(n, gait)) = ag.path.as_ref().and_then(|p| p.steps.first()) {
        let pace = MOVE_COST_PER_DAY * if ag.load.is_some() { LADEN_PACE } else { 1.0 };
        let cost = sim.nav.surface_at(n).and_then(sim::nav::NavGrid::step_cost).map_or(gait_cost(gait), |c| c.0);
        // step_frac is remaining movement cost, not a normalized cell fraction.
        frac = ((ag.step_frac + pending_days * pace) / cost).clamp(0.0, 1.0);
        next = n;
    }
    let to = (next.0 as f32 + 0.5, next.1 as f32 + 0.5);
    let xz = cell_xz(sim, (here.0 + (to.0 - here.0) * frac, here.1 + (to.1 - here.1) * frac));
    let (l0, l1) = (lift_in(sim, ag.cell), lift_in(sim, next));
    Vec3::new(xz.x, height_at(&sim.height, xz.x, xz.y) + l0 + (l1 - l0) * frac, xz.y)
}

pub fn moving_gait(ag: &Agent) -> Gait {
    [Gait::Climb, Gait::Swim].into_iter().find(|g| ag.trail.iter().any(|t| t.1 == *g)).unwrap_or(ag.gait)
}

pub fn pose_of(sim: &SimState, ag: &Agent) -> Pose {
    if is_moving(ag) {
        return match moving_gait(ag) {
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

pub fn is_wet(sim: &SimState, ag: &Agent) -> bool {
    if matches!(sim.nav.surface_at(ag.cell), Some(Surface::Water)) {
        return true;
    }
    match ag.path.as_ref().and_then(|p| p.steps.first()) {
        Some(&(n, _)) => ag.step_frac >= 0.5 && matches!(sim.nav.surface_at(n), Some(Surface::Water)),
        None => false,
    }
}

pub fn load_of(ag: &Agent) -> Option<(Good, f32)> {
    ag.load.map(|l| (l.good, l.amount))
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
        Task::Build { site } => sim.buildings.building(*site).and_then(|b| {
            let (w, h) = b.kind.spec().footprint;
            toward((b.pos.0 as f32 + w as f32 / 2.0, b.pos.1 as f32 + h as f32 / 2.0))
        }),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cell_fraction_uses_the_same_surface_cost_as_movement() {
        let mut sim = SimState::new(1);
        let id = sim::colony::BeaverId(1);
        sim.agents.spawn(id, (2,2));
        let mut agent = sim.agents.get(id).unwrap().clone();
        agent.path = Some(sim::nav::Path { steps: vec![((3,2), Gait::Walk)], cost: sim::nav::THICKET_COST });
        agent.step_frac = sim::nav::THICKET_COST * 0.5;
        sim.nav.set((3,2), Surface::Thicket);
        let actual = agent_world_pos(&sim, &agent, 0.0).xz();
        assert!(actual.distance(cell_xz(&sim, (3.0,2.5))) < 1e-5);
    }
}
