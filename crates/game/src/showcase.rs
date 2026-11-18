//! `--showcase poses|world`: stages a still scene in the sim for screenshots and contact sheets.
//! Nothing here draws; it only sets up `SimState` (agents mid-step, resources, sites) and the camera, and
//! the normal sim-driven renderer shows it. The colony is never founded, so the sim does not advance and the
//! scene holds still (animation keeps running).
//!
//! * `poses`: six stations along a line, picked with `BC_STATION=0..5` (camera frames that station):
//!   0 walking, 1 swimming, 2 climbing over a dam, 3 working at a site, 4 resting by a lodge,
//!   5 carrying a log through water past a felled tree, 6 at a shore: one beaver on the grass, one
//!   about to step into the water (swim gait, under half way), one swimming (only it shows foam arcs).
//! * `world`: felled trees and logs, mud banks and stone outcrops, plants and three construction sites
//!   at 15 %, 50 % and 85 % progress.
use crate::camera::OrbitCamera;
use crate::cli::Cli;
use crate::state::Sim;
use bevy::prelude::*;
use sim::agents::{Load, Task};
use sim::buildings::{Building, BuildingId, BuildingKind, Good, Status};
use sim::colony::BeaverId;
use sim::nav::{Gait, Path, Surface};
use sim::resources::{ResourceKind, FELL_WORK};
use sim::SimState;

pub const STATIONS: usize = 7;
const SPACING: usize = 12;

pub struct ShowcasePlugin;

impl Plugin for ShowcasePlugin {
    fn build(&self, app: &mut App) {
        let Some(name) = app.world().resource::<Cli>().showcase.clone() else { return };
        let station = std::env::var("BC_STATION").ok().and_then(|s| s.parse::<usize>().ok()).unwrap_or(0);
        let mut sim = std::mem::replace(&mut app.world_mut().resource_mut::<Sim>().0, SimState::new(1));
        let origin = find_flat(&sim);
        let (focus, yaw, pitch, dist) = match name.as_str() {
            "poses" => {
                stage_poses(&mut sim, origin);
                let c = station_cell(origin, station.min(STATIONS - 1));
                (cell_centre(&sim, (c.0 as f32 + 1.0, c.1 as f32 - 0.5)), 0.45, 0.42, 7.5)
            }
            _ => {
                stage_world(&mut sim, origin);
                (cell_centre(&sim, (origin.0 as f32 + 18.0, origin.1 as f32 + 9.0)), 0.2, 0.8, 30.0)
            }
        };
        app.world_mut().resource_mut::<Sim>().0 = sim;
        app.add_systems(Startup, reveal_map);
        let mut orbit = app.world_mut().resource_mut::<OrbitCamera>();
        orbit.target = focus;
        orbit.yaw = yaw;
        orbit.pitch = pitch;
        orbit.distance = dist;
    }
}

fn cell_centre(sim: &SimState, c: (f32, f32)) -> Vec3 {
    let p = crate::entities_render::sim_view::cell_xz(sim, c);
    Vec3::new(p.x, crate::entities_render::height_at(&sim.height, p.x, p.y), p.y)
}

fn station_cell(o: (usize, usize), i: usize) -> (usize, usize) {
    (o.0 + 4 + i * SPACING, o.1 + 8)
}

/// A block of dry, gently sloping land near the start valley that fits the staged scenes.
fn find_flat(sim: &SimState) -> (usize, usize) {
    let c = sim.world.valleys.get(sim.world.start.0 as usize).map_or((sim.world.size as f32 / 2.0, sim.world.size as f32 / 2.0), |v| v.center);
    let (w, h) = (STATIONS * SPACING + 8, 24usize);
    let n = sim.world.size;
    let mut best = (f32::MAX, (c.0 as usize, c.1 as usize));
    for dy in (0..90).step_by(6) {
        for dx in (0..90).step_by(6) {
            let (x0, y0) = ((c.0 as usize).saturating_sub(45) + dx, (c.1 as usize).saturating_sub(45) + dy);
            if x0 + w >= n || y0 + h >= n {
                continue;
            }
            let (mut lo, mut hi, mut wet) = (f32::MAX, f32::MIN, 0);
            for y in (y0..y0 + h).step_by(2) {
                for x in (x0..x0 + w).step_by(2) {
                    let g = sim.height.get(x, y).copied().unwrap_or(0.0);
                    lo = lo.min(g);
                    hi = hi.max(g);
                    wet += sim.water.depth.get(x, y).is_some_and(|d| *d > 0.02) as usize;
                }
            }
            let score = (hi - lo) + wet as f32 * 3.0;
            if score < best.0 {
                best = (score, (x0, y0));
            }
        }
    }
    best.1
}

/// Clears the staged block of other resources and flattens the nav surface to land.
fn prepare(sim: &mut SimState, o: (usize, usize), w: usize, h: usize) {
    // level the block, easing back into the hillside over a margin, so every station stands on flat ground
    let level = sim.height.get(o.0 + w / 2, o.1 + h / 2).copied().unwrap_or(0.0);
    let m = 6usize;
    for y in o.1.saturating_sub(m)..o.1 + h + m {
        for x in o.0.saturating_sub(m)..o.0 + w + m {
            let dx = (o.0.saturating_sub(x)).max(x.saturating_sub(o.0 + w - 1));
            let dy = (o.1.saturating_sub(y)).max(y.saturating_sub(o.1 + h - 1));
            let t = (dx.max(dy) as f32 / m as f32).clamp(0.0, 1.0);
            if let Some(g) = sim.height.get_mut(x, y) {
                *g = level + (*g - level) * t * t * (3.0 - 2.0 * t);
            }
        }
    }
    sim.resources.items.retain(|r| !(r.cell.0 >= o.0 && r.cell.0 < o.0 + w && r.cell.1 >= o.1 && r.cell.1 < o.1 + h));
    for y in o.1..o.1 + h {
        for x in o.0..o.0 + w {
            sim.nav.set((x, y), Surface::Land);
            if let Some(d) = sim.water.depth.get_mut(x, y) {
                *d = 0.0;
            }
        }
    }
}

fn pool(sim: &mut SimState, x0: usize, y0: usize, w: usize, h: usize, depth: f32) {
    for y in y0..y0 + h {
        for x in x0..x0 + w {
            sim.nav.set((x, y), Surface::Water);
            if let Some(d) = sim.water.depth.get_mut(x, y) {
                *d = depth;
            }
        }
    }
}

fn agent(sim: &mut SimState, id: u64, cell: (usize, usize), steps: &[((usize, usize), Gait)], frac: f32) -> BeaverId {
    let id = BeaverId(id);
    sim.agents.spawn(id, cell);
    let ag = sim.agents.by_id.get_mut(&id).expect("just spawned");
    if let Some(&(_, g)) = steps.first() {
        ag.gait = g;
        ag.path = Some(Path { steps: steps.to_vec(), cost: steps.len() as f32 });
        ag.step_frac = frac;
    }
    id
}

fn add_site(sim: &mut SimState, id: u64, kind: BuildingKind, pos: (usize, usize), progress: f32) -> BuildingId {
    let id = BuildingId(id);
    sim.buildings.buildings.push(Building { id, kind, pos, status: Status::Site, recipe: None, output: 0.0 });
    let spec = kind.spec();
    sim.work.add_site(id, kind, spec.cost, spec.work).expect("fresh site");
    for &(g, a) in spec.cost {
        let _ = sim.work.deliver(id, g, a * progress);
    }
    let _ = sim.work.add_labour(id, spec.work * progress);
    for c in sim.buildings.building(id).map(|b| b.cells().collect::<Vec<_>>()).unwrap_or_default() {
        sim.nav.set(c, Surface::Solid);
    }
    id
}

fn add_done(sim: &mut SimState, id: u64, kind: BuildingKind, pos: (usize, usize)) {
    sim.buildings.buildings.push(Building { id: BuildingId(id), kind, pos, status: Status::Complete, recipe: None, output: 0.0 });
}

fn felled(sim: &mut SimState, cell: (usize, usize), logs_left: f32, branches: bool) {
    let t = sim.resources.add(ResourceKind::Tree, cell, FELL_WORK);
    sim.resources.fell(t, FELL_WORK).expect("a tree");
    let take = sim::resources::TREE_LOGS - logs_left;
    if take > 0.0 {
        sim.resources.take(t, take);
    }
    if !branches {
        if let Some(r) = sim.resources.items.iter_mut().find(|r| r.id == t) {
            r.branches = 0.0;
        }
    }
}

fn stage_poses(sim: &mut SimState, o: (usize, usize)) {
    prepare(sim, o, STATIONS * SPACING + 8, 16);
    let (land, swim, climb, work, rest, carry, shore) = (0, 1, 2, 3, 4, 5, 6);
    let c = |i| station_cell(o, i);

    // 0 walking: two land cells ahead, half way into the first
    let b = c(land);
    agent(sim, 1, b, &[((b.0 + 1, b.1), Gait::Walk), ((b.0 + 2, b.1), Gait::Walk)], 0.4);
    sim.resources.add(ResourceKind::Tree, (b.0 + 4, b.1 - 2), FELL_WORK);

    // 1 swimming in a pond
    let b = c(swim);
    pool(sim, b.0 - 3, b.1 - 3, 9, 6, 1.1);
    agent(sim, 2, b, &[((b.0 + 1, b.1), Gait::Swim), ((b.0 + 2, b.1), Gait::Swim)], 0.5);

    // 2 climbing onto a dam with a pond behind it
    let b = c(climb);
    pool(sim, b.0 - 2, b.1 - 4, 7, 3, 1.0);
    add_done(sim, 10, BuildingKind::Dam, (b.0, b.1 - 1));
    for x in b.0..b.0 + 3 {
        sim.nav.set((x, b.1 - 1), Surface::Dam);
    }
    agent(sim, 3, (b.0 + 1, b.1), &[((b.0 + 1, b.1 - 1), Gait::Climb), ((b.0 + 1, b.1 - 2), Gait::Swim)], 0.55);

    // 3 working on a lodge site (two builders' worth of progress)
    let b = c(work);
    let site = add_site(sim, 11, BuildingKind::Lodge, (b.0 + 1, b.1 - 3), 0.5);
    let id = agent(sim, 4, (b.0 + 1, b.1), &[], 0.0);
    sim.agents.by_id.get_mut(&id).expect("spawned").task = Task::Build { site };

    // 4 resting by a finished lodge
    let b = c(rest);
    add_done(sim, 12, BuildingKind::Lodge, (b.0 + 1, b.1 - 4));
    let id = agent(sim, 5, (b.0 + 1, b.1), &[], 0.0);
    sim.agents.by_id.get_mut(&id).expect("spawned").task = Task::Rest;

    // 5 carrying a log across the water, a felled tree on the bank
    let b = c(carry);
    pool(sim, b.0 - 1, b.1 - 3, 8, 6, 1.1);
    felled(sim, (b.0 - 3, b.1 - 1), 2.0, true);
    let id = agent(sim, 6, b, &[((b.0 + 1, b.1), Gait::Swim), ((b.0 + 2, b.1), Gait::Swim)], 0.5);
    sim.agents.by_id.get_mut(&id).expect("spawned").load = Some(Load { good: Good::Logs, amount: 3.0 });
    sim.agents.by_id.get_mut(&id).expect("spawned").task = Task::Carry { dest: BuildingId(1) };

    // 6 shore: water from x+2 on; a beaver standing on grass, one stepping into the water (under half
    // way, so still on land), and one swimming past
    let b = c(shore);
    // sunk into the ground so the water surface is level with the grass bank, as in a real pond
    for y in b.1 - 4..b.1 + 3 {
        for x in b.0 + 2..b.0 + 8 {
            if let Some(g) = sim.height.get_mut(x, y) {
                *g -= 1.1;
            }
        }
    }
    pool(sim, b.0 + 2, b.1 - 4, 6, 7, 1.1);
    agent(sim, 7, (b.0 - 1, b.1 - 2), &[], 0.0);
    agent(sim, 8, (b.0 + 1, b.1), &[((b.0 + 2, b.1), Gait::Swim), ((b.0 + 3, b.1), Gait::Swim)], 0.1);
    agent(sim, 9, (b.0 + 3, b.1 - 2), &[((b.0 + 4, b.1 - 2), Gait::Swim), ((b.0 + 5, b.1 - 2), Gait::Swim)], 0.5);
}

fn stage_world(sim: &mut SimState, o: (usize, usize)) {
    prepare(sim, o, 40, 24);
    let at = |dx: usize, dy: usize| (o.0 + dx, o.1 + dy);
    // standing trees, then felled ones with logs and branches left, fewer logs, bare logs
    let mut first_tree = None;
    for i in 0..3 {
        let t = sim.resources.add(ResourceKind::Tree, at(2 + i * 3, 2), FELL_WORK);
        first_tree.get_or_insert(t);
    }
    felled(sim, at(14, 2), 3.0, true);
    felled(sim, at(20, 3), 2.0, true);
    felled(sim, at(26, 2), 1.0, false);
    felled(sim, at(32, 4), 3.0, false);
    // deposits
    sim.resources.add(ResourceKind::MudBank, at(3, 9), sim::resources::MUD_AMOUNT);
    sim.resources.add(ResourceKind::MudBank, at(8, 9), sim::resources::MUD_AMOUNT * 0.4);
    sim.resources.add(ResourceKind::StoneOutcrop, at(15, 9), sim::resources::STONE_AMOUNT);
    sim.resources.add(ResourceKind::StoneOutcrop, at(20, 9), sim::resources::STONE_AMOUNT * 0.35);
    for i in 0..3 {
        sim.resources.add(ResourceKind::Plant, at(27 + i * 2, 9 + i % 2), 3.0);
    }
    // a lodge site at three stages of progress, and a finished lodge for reference
    for (i, p) in [0.15f32, 0.5, 0.85].into_iter().enumerate() {
        add_site(sim, 20 + i as u64, BuildingKind::Lodge, at(2 + i * 6, 15), p);
    }
    add_done(sim, 30, BuildingKind::Lodge, at(21, 15));
    // a few beavers at work so the scene is alive
    let id = agent(sim, 40, at(16, 12), &[(at(17, 12), Gait::Walk)], 0.3);
    sim.agents.by_id.get_mut(&id).expect("spawned").load = Some(Load { good: Good::Stone, amount: 3.0 });
    let id = agent(sim, 41, at(9, 12), &[(at(10, 12), Gait::Walk)], 0.6);
    sim.agents.by_id.get_mut(&id).expect("spawned").load = Some(Load { good: Good::Mud, amount: 3.0 });
    let id = agent(sim, 42, at(3, 3), &[], 0.0);
    sim.agents.by_id.get_mut(&id).expect("spawned").task = Task::Collect { target: first_tree.expect("a tree"), progress: 0.0 };
    let id = agent(sim, 43, at(26, 5), &[(at(27, 5), Gait::Walk)], 0.2);
    sim.agents.by_id.get_mut(&id).expect("spawned").load = Some(Load { good: Good::Sticks, amount: 3.0 });
}

/// Lifts the fog of war so the staged scene is lit like explored ground.
fn reveal_map(mut sim: ResMut<Sim>, mut flow: ResMut<crate::flow::Flow>) {
    let _ = &sim;
    let _ = &flow;
    sim.0.fog.seen.fill(1.0);
    sim.0.fog.dirty = true;
    flow.message.clear();
}
