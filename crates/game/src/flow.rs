//! Game flow: opening (choosing where to settle, settling), picking, selection, fog of war, valley
//! unlocking and colony collapse. The pure parts (state machine, picking math, fog, suitability)
//! are plain functions so they can be tested without a window.
use crate::camera::OrbitCamera;
use crate::sim_driver::SimClock;
use crate::state::Sim;
use crate::terrain_render::CELL_SIZE;
use bevy::prelude::*;
use sim::colony::{BeaverId, LodgeId};
use sim::grid::{Grid, HeightGrid};
use sim::worldgen::World;
use std::collections::VecDeque;

pub type Cell = (usize, usize);


// ───────────────────────────── state machine ─────────────────────────────

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    /// The player looks over the start valley and picks where the family settles.
    Scouting,
    /// The first lodge stands and the colony lives.
    Settled,
    /// The colony collapsed; the game-over panel is up.
    GameOver,
    /// Collapsed, and the player chose to keep watching.
    Watching,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FlowEvent {
    Settle,
    Collapse,
    Continue,
    Restart,
}

impl Phase {
    /// The phase after `ev`, or `None` if the event is not valid here.
    pub fn on(self, ev: FlowEvent) -> Option<Phase> {
        use FlowEvent::*;
        use Phase::*;
        match (self, ev) {
            (Scouting, Settle) => Some(Settled),
            (Settled, Collapse) => Some(GameOver),
            (GameOver, Continue) => Some(Watching),
            (Settled | GameOver | Watching, Restart) => Some(Scouting),
            _ => None,
        }
    }
    pub fn banner(self) -> &'static str {
        match self {
            Phase::Scouting => "",
            Phase::Settled => "",
            Phase::GameOver => "The colony has collapsed",
            Phase::Watching => "Colony collapsed - watching",
        }
    }
}

// ───────────────────────────── selection ─────────────────────────────

#[derive(Resource, Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Selection {
    #[default]
    None,
    Cell(Cell),
    Beaver(BeaverId),
    Lodge(LodgeId),
    /// A dam or other structure on the water grid.
    Building(Cell),
    /// A standing tree (opens its panel).
    Tree(sim::resources::ResourceId),
    /// A building or construction site (opens its panel).
    Site(sim::buildings::BuildingId),
}

// ───────────────────────────── picking math ─────────────────────────────

/// World-space position of continuous cell coordinates, on the terrain surface.
pub fn cell_world(h: &HeightGrid, pos: (f32, f32)) -> Vec3 {
    let (ox, oz) = (h.width() as f32 * CELL_SIZE / 2.0, h.height() as f32 * CELL_SIZE / 2.0);
    Vec3::new(pos.0 * CELL_SIZE - ox, height_at(h, pos.0, pos.1), pos.1 * CELL_SIZE - oz)
}

/// Inverse of [`cell_world`] for x/z: the (continuous) cell coordinates under a world point.
pub fn world_cell(h: &HeightGrid, p: Vec3) -> (f32, f32) {
    let (ox, oz) = (h.width() as f32 * CELL_SIZE / 2.0, h.height() as f32 * CELL_SIZE / 2.0);
    ((p.x + ox) / CELL_SIZE, (p.z + oz) / CELL_SIZE)
}

/// Bilinear terrain height at continuous cell coordinates (clamped to the grid).
pub fn height_at(h: &HeightGrid, x: f32, y: f32) -> f32 {
    let (w, d) = (h.width(), h.height());
    let x = x.clamp(0.0, (w - 1) as f32);
    let y = y.clamp(0.0, (d - 1) as f32);
    let (x0, y0) = (x.floor() as usize, y.floor() as usize);
    let (x1, y1) = ((x0 + 1).min(w - 1), (y0 + 1).min(d - 1));
    let (fx, fy) = (x - x0 as f32, y - y0 as f32);
    let g = |x, y| *h.get(x, y).expect("clamped in bounds");
    let top = g(x0, y0) * (1.0 - fx) + g(x1, y0) * fx;
    let bot = g(x0, y1) * (1.0 - fx) + g(x1, y1) * fx;
    top * (1.0 - fy) + bot * fy
}

/// First terrain hit of a ray: the grid cell, the hit point and the ray parameter.
pub fn ray_heightfield(origin: Vec3, dir: Vec3, h: &HeightGrid) -> Option<(Cell, Vec3, f32)> {
    let dir = dir.try_normalize()?;
    let (w, d) = (h.width() as f32, h.height() as f32);
    let max_t = 4000.0;
    let step = 0.5;
    let inside = |p: Vec3| {
        let (cx, cy) = world_cell(h, p);
        cx >= -0.5 && cy >= -0.5 && cx <= w - 0.5 && cy <= d - 0.5
    };
    let below = |p: Vec3| {
        let (cx, cy) = world_cell(h, p);
        p.y <= height_at(h, cx, cy)
    };
    let mut t = 0.0;
    let mut prev_t = 0.0;
    while t < max_t {
        let p = origin + dir * t;
        if inside(p) && below(p) {
            // The ray may start underground: only a crossing from above counts.
            if t == 0.0 {
                return None;
            }
            let (mut lo, mut hi) = (prev_t, t);
            for _ in 0..24 {
                let mid = 0.5 * (lo + hi);
                if below(origin + dir * mid) { hi = mid } else { lo = mid }
            }
            let hit = origin + dir * hi;
            let (cx, cy) = world_cell(h, hit);
            let cell = ((cx.round().max(0.0) as usize).min(h.width() - 1), (cy.round().max(0.0) as usize).min(h.height() - 1));
            return Some((cell, hit, hi));
        }
        prev_t = t;
        t += step;
    }
    None
}

/// Distance along the ray to a sphere, if hit.
pub fn ray_sphere(origin: Vec3, dir: Vec3, center: Vec3, radius: f32) -> Option<f32> {
    let oc = origin - center;
    let b = oc.dot(dir);
    let c = oc.dot(oc) - radius * radius;
    let disc = b * b - c;
    if disc < 0.0 {
        return None;
    }
    let t = -b - disc.sqrt();
    if t >= 0.0 {
        Some(t)
    } else if -b + disc.sqrt() >= 0.0 {
        Some(0.0)
    } else {
        None
    }
}

/// Nearest of the candidate (selection, centre, radius) spheres along the ray.
pub fn pick_entities(origin: Vec3, dir: Vec3, cands: &[(Selection, Vec3, f32)]) -> Option<(Selection, f32)> {
    cands
        .iter()
        .filter_map(|&(s, c, r)| ray_sphere(origin, dir, c, r).map(|t| (s, t)))
        .min_by(|a, b| a.1.total_cmp(&b.1))
}

// ───────────────────────────── fog, settling (in the sim) ─────────────────────────────

#[allow(unused_imports)]
pub use sim::fog::{open_at, EXPLORED};
pub use sim::site::{best_site, can_settle, score as site_hint, SettleError};

// ───────────────────────────── flow resource ─────────────────────────────

#[derive(Resource)]
pub struct Flow {
    pub phase: Phase,
    /// Valley mouth: where the family comes from.
    #[allow(dead_code)]
    pub mouth: (f32, f32),
    /// Camera focus target while the camera eases toward a spot (e.g. the new lodge).
    pub focus: Option<Vec3>,
    pub message: String,
    pub log: VecDeque<String>,
    /// (day, population, food) samples for the trend bars.
    pub history: Vec<(f64, usize, f32)>,
    pub real_secs: f32,
    /// Bumped on restart so views rebuild.
    pub generation: u32,
    pub settled_at: Option<f32>,
    pub lodge_cell: Option<Cell>,
    next_unlock: f32,
}

impl Flow {
    pub fn new(world: &World) -> Flow {
        let mouth = valley_mouth(world);
        Flow {
            phase: Phase::Scouting,
            mouth,
            focus: None,
            message: String::new(),
            log: VecDeque::new(),
            history: Vec::new(),
            real_secs: 0.0,
            generation: 0,
            settled_at: None,
            lodge_cell: None,
            next_unlock: 0.0,
        }
    }

    pub fn push_log(&mut self, s: impl Into<String>) {
        self.log.push_back(s.into());
        while self.log.len() > 40 {
            self.log.pop_front();
        }
    }

    /// Applies a state-machine event; returns whether it was valid.
    pub fn fire(&mut self, ev: FlowEvent) -> bool {
        match self.phase.on(ev) {
            Some(p) => {
                self.phase = p;
                true
            }
            None => false,
        }
    }
}

/// Where the start valley drains: its river cell with the largest catchment.
pub fn valley_mouth(world: &World) -> (f32, f32) {
    let mut best: Option<(u32, Cell)> = None;
    for y in 0..world.size {
        for x in 0..world.size {
            if world.valley_of((x, y)) == Some(world.start) && world.river.get(x, y) == Some(&true) {
                let acc = *world.flow_acc.get(x, y).unwrap();
                if best.is_none_or(|(b, _)| acc > b) {
                    best = Some((acc, (x, y)));
                }
            }
        }
    }
    match best {
        Some((_, c)) => (c.0 as f32, c.1 as f32),
        None => world.valleys[world.start.0 as usize].center,
    }
}

/// Founds the colony at `cell`. Validates, sends `Command::Settle` to the sim and does the flow
/// bookkeeping (state machine, fog, camera) in [`after_settle`].
pub fn settle(sim: &mut sim::SimState, flow: &mut Flow, cell: Cell) -> Result<(), String> {
    if flow.phase != Phase::Scouting {
        return Err("The colony can only be founded while choosing the settling place".into());
    }
    can_settle(sim, cell).map_err(|e| format!("{e:?}"))?;
    sim.apply(sim::Command::Settle { cell }).map_err(|e| e.to_string())?;
    after_settle(sim, flow, cell);
    Ok(())
}

/// The flow's side of a founded colony: advance the state machine, lift fog, aim the camera, log it.
pub fn after_settle(sim: &sim::SimState, flow: &mut Flow, cell: Cell) {
    let pos = (cell.0 as f32 + 0.5, cell.1 as f32 + 0.5);
    flow.fire(FlowEvent::Settle);
    flow.settled_at = Some(flow.real_secs);
    flow.lodge_cell = Some(cell);
    flow.focus = Some(cell_world(&sim.height, pos));
    let name = sim.colony.families.first().map(|f| f.name.clone()).unwrap_or_default();
    flow.push_log(format!("The {name} family settled at ({}, {})", cell.0, cell.1));
    flow.message.clear();
}

/// Resets everything to a fresh opening with the same seed.
pub fn restart(sim: &mut sim::SimState, flow: &mut Flow, clock: &mut SimClock, sel: &mut Selection) {
    if flow.phase.on(FlowEvent::Restart).is_none() {
        return;
    }
    let seed = sim.seed;
    sim.apply(sim::Command::Regenerate { seed }).expect("regenerate with a valid seed");
    let generation = flow.generation + 1;
    *flow = Flow::new(&sim.world);
    flow.generation = generation;
    *clock = SimClock::default();
    *sel = Selection::None;
}

// ───────────────────────────── systems ─────────────────────────────

/// Mouse-over state for the world (cell under the cursor and its site hint).
#[derive(Resource, Default)]
pub struct Hover {
    pub cell: Option<Cell>,
    pub hint: Option<sim::site::SiteScore>,
}

/// While false, `--screenshot` does not capture (set by `--autoplay-settle` until the run is done).
#[derive(Resource)]
pub struct CaptureGate(pub bool);

pub struct FlowPlugin;

impl Plugin for FlowPlugin {
    fn build(&self, app: &mut App) {
        let flow = Flow::new(&app.world().resource::<Sim>().0.world);
        app.insert_resource(flow)
            .init_resource::<Selection>()
            .init_resource::<Hover>()
            .insert_resource(CaptureGate(true))
            .add_systems(
                Update,
                (advance_flow, unlock_valleys, watch_collapse, sample_history, ease_camera)
                    .chain(),
            );
    }
}

fn advance_flow(
    time: Res<Time>,
    sim: Res<Sim>,
    orbit: Res<OrbitCamera>,
    mut flow: ResMut<Flow>,
) {
    let dt = time.delta_secs().min(0.1);
    flow.real_secs += dt;
    // The camera reveals nothing: exploring is the beavers' work (the sim lifts the fog).
    let _ = (&orbit, &sim);
}

/// Valleys unlock when a beaver reaches their border; unlocking opens their fog.
fn unlock_valleys(time: Res<Time>, mut sim: ResMut<Sim>, mut flow: ResMut<Flow>) {
    if !sim.0.colony.is_founded() || flow.real_secs < flow.next_unlock {
        return;
    }
    flow.next_unlock = flow.real_secs + 0.25;
    let cells: Vec<Cell> = sim
        .0
        .colony
        .beavers
        .iter()
        .map(|b| (b.pos.0.max(0.0) as usize, b.pos.1.max(0.0) as usize))
        .collect();
    let _ = &time;
    let mut opened_all = Vec::new();
    for c in cells {
        if let Ok(opened) = sim.0.world.unlock(c) {
            opened_all.extend(opened);
        }
    }
    for id in opened_all {
        // Reaching a valley opens it to the colony; its ground is still seen only as beavers go.
        flow.push_log(format!("The colony reached valley {}", id.0));
    }
}

fn watch_collapse(mut sim: ResMut<Sim>, mut flow: ResMut<Flow>, mut clock: ResMut<SimClock>) {
    let events = sim.0.colony.take_events();
    for e in &events {
        flow.push_log(format!("day {:.0}: {:?}", e.day, e.event));
    }
    if flow.phase == Phase::Settled && sim.0.colony.is_collapsed() {
        flow.fire(FlowEvent::Collapse);
        clock.paused = true;
        flow.push_log("The colony collapsed");
    }
}

fn sample_history(sim: Res<Sim>, mut flow: ResMut<Flow>) {
    if !sim.0.colony.is_founded() {
        return;
    }
    let day = sim.0.time_days;
    if flow.history.last().is_none_or(|h| day - h.0 >= 5.0) {
        flow.history.push((day, sim.0.colony.population(), sim.0.colony.food));
        if flow.history.len() > 400 {
            flow.history.remove(0);
        }
    }
}

fn ease_camera(time: Res<Time>, mut flow: ResMut<Flow>, mut orbit: ResMut<OrbitCamera>) {
    let Some(f) = flow.focus else { return };
    let (next, arrived) = ease_step(*orbit, f, (time.delta_secs() * 2.5).min(1.0), flow.settled_at.is_some());
    if arrived {
        flow.focus = None;
    }
    *orbit = next;
}

/// One easing step of the camera toward `focus` (`k` in 0..=1); true once it has arrived.
pub fn ease_step(orbit: OrbitCamera, focus: Vec3, k: f32, settled: bool) -> (OrbitCamera, bool) {
    let mut next = orbit;
    next.target = orbit.target.lerp(focus, k);
    if settled {
        next.distance += (70.0 - next.distance) * k;
    }
    let arrived = next.target.distance(focus) < 0.05;
    if arrived {
        next.target = focus;
    }
    (next, arrived)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flat(n: usize, h: f32) -> HeightGrid {
        Grid::new(n, n, h)
    }

    #[test]
    fn flow_picking_hits_flat_ground_at_the_right_cell() {
        let g = flat(32, 2.0);
        // Cell (10, 20) is at world x = 10 - 16, z = 20 - 16.
        let target = cell_world(&g, (10.0, 20.0));
        assert_eq!(target, Vec3::new(-6.0, 2.0, 4.0));
        let origin = target + Vec3::new(5.0, 30.0, -8.0);
        let (cell, hit, _) = ray_heightfield(origin, target - origin, &g).unwrap();
        assert_eq!(cell, (10, 20));
        assert!((hit - target).length() < 0.05);
        // pointing away misses
        assert!(ray_heightfield(origin, Vec3::Y, &g).is_none());
        // off the map misses
        assert!(ray_heightfield(Vec3::new(500.0, 30.0, 500.0), Vec3::NEG_Y, &g).is_none());
    }

    #[test]
    fn flow_picking_respects_hills() {
        let mut g = flat(32, 0.0);
        for y in 0..32 {
            *g.get_mut(16, y).unwrap() = 10.0;
        }
        // A low ray travelling in +x hits the ridge before the ground behind it.
        let origin = Vec3::new(-14.0, 6.0, 0.0);
        let (cell, _, _) = ray_heightfield(origin, Vec3::new(1.0, -0.05, 0.0), &g).unwrap();
        assert!(cell.0 >= 14 && cell.0 <= 16, "{cell:?}");
    }

    #[test]
    fn flow_entity_picking_takes_nearest_sphere() {
        let o = Vec3::ZERO;
        let c = [
            (Selection::Cell((1, 1)), Vec3::new(0.0, 0.0, -10.0), 1.0),
            (Selection::Lodge(LodgeId(7)), Vec3::new(0.0, 0.0, -5.0), 1.0),
            (Selection::Cell((9, 9)), Vec3::new(5.0, 0.0, -5.0), 1.0),
        ];
        let (s, t) = pick_entities(o, Vec3::NEG_Z, &c).unwrap();
        assert_eq!(s, Selection::Lodge(LodgeId(7)));
        assert!((t - 4.0).abs() < 1e-4);
        assert!(pick_entities(o, Vec3::Z, &c).is_none());
    }

    #[test]
    fn flow_settle_state_machine() {
        use FlowEvent::*;
        use Phase::*;
        assert_eq!(Scouting.on(Settle), Some(Settled));
        assert_eq!(Settled.on(Collapse), Some(GameOver));
        assert_eq!(GameOver.on(Continue), Some(Watching));
        assert_eq!(Watching.on(Restart), Some(Scouting));
        assert_eq!(GameOver.on(Restart), Some(Scouting));
        // invalid transitions are refused
        assert_eq!(Scouting.on(Collapse), None);
        assert_eq!(Settled.on(Settle), None);
        assert_eq!(Scouting.on(Restart), None);
    }

    #[test]
    fn flow_settling_founds_the_colony() {
        let mut s = sim::SimState::new(1);
        let mut flow = Flow::new(&s.world);
        let site = best_site(&s).expect("a settleable site");
        settle(&mut s, &mut flow, site).unwrap();
        assert_eq!(flow.phase, Phase::Settled);
        assert!(s.colony.is_founded());
        assert!(s.colony.lodges.is_empty(), "no home until they build one");
        assert_eq!(s.colony.population(), 4);
        assert!(settle(&mut s, &mut flow, site).is_err(), "a second settle is refused");
    }

    #[test]
    fn flow_a_collapsed_colony_is_reported() {
        let mut s = sim::SimState::new(1);
        let mut flow = Flow::new(&s.world);
        let site = best_site(&s).unwrap();
        settle(&mut s, &mut flow, site).unwrap();
        assert!(!s.colony.is_collapsed());
        assert!(flow.fire(FlowEvent::Collapse));
        assert_eq!(flow.phase, Phase::GameOver);
        assert!(flow.fire(FlowEvent::Restart));
    }
}
