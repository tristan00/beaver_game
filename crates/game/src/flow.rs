//! Game flow: opening (arrival, scouting, settling), picking, selection, fog of war, valley
//! unlocking and colony collapse. The pure parts (state machine, picking math, fog, suitability)
//! are plain functions so they can be tested without a window.
use crate::camera::OrbitCamera;
use crate::sim_driver::SimClock;
use crate::state::Sim;
use crate::terrain_render::CELL_SIZE;
use bevy::prelude::*;
use sim::colony::{BeaverId, LodgeId};
use sim::grid::{Grid, HeightGrid};
use sim::worldgen::{ValleyId, World};
use std::collections::VecDeque;

pub type Cell = (usize, usize);

pub const SCOUT_COUNT: usize = 4;
pub const SCOUT_SPEED: f32 = 7.0;
pub const SCOUT_SIGHT: f32 = 7.0;
pub const FOCUS_SIGHT: f32 = 5.0;
pub const ARRIVAL_SECS: f32 = 1.5;
/// Fog value above which a cell counts as explored.
pub const EXPLORED: f32 = 0.5;

// ───────────────────────────── state machine ─────────────────────────────

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    /// Scouts walk in from the valley mouth.
    Arrival,
    /// The player explores and picks a site.
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
    ScoutsArrived,
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
            (Arrival, ScoutsArrived) => Some(Scouting),
            (Scouting, Settle) => Some(Settled),
            (Settled, Collapse) => Some(GameOver),
            (GameOver, Continue) => Some(Watching),
            (Scouting | Settled | GameOver | Watching, Restart) => Some(Arrival),
            _ => None,
        }
    }
    pub fn banner(self) -> &'static str {
        match self {
            Phase::Arrival => "Scouts arrive at the valley mouth...",
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

// ───────────────────────────── fog of war ─────────────────────────────

/// Explored amount per cell, 0 (fog) ..= 1 (seen).
#[derive(Clone, Debug)]
pub struct Fog {
    pub size: usize,
    pub seen: Vec<f32>,
    /// Set when `seen` changed since the terrain colours were last rebuilt.
    pub dirty: bool,
}

impl Fog {
    pub fn new(size: usize) -> Fog {
        Fog { size, seen: vec![0.0; size * size], dirty: true }
    }
    pub fn get(&self, c: Cell) -> f32 {
        if c.0 < self.size && c.1 < self.size { self.seen[c.1 * self.size + c.0] } else { 0.0 }
    }
    pub fn explored_cells(&self) -> usize {
        self.seen.iter().filter(|&&v| v >= EXPLORED).count()
    }
    /// Lifts fog in a soft-edged disc, only where `allowed`. Returns whether anything changed.
    pub fn reveal(&mut self, centre: (f32, f32), radius: f32, allowed: &dyn Fn(usize, usize) -> bool) -> bool {
        let r = radius + 2.0;
        let (x0, x1) = ((centre.0 - r).floor().max(0.0) as usize, ((centre.0 + r).ceil().max(0.0) as usize).min(self.size - 1));
        let (y0, y1) = ((centre.1 - r).floor().max(0.0) as usize, ((centre.1 + r).ceil().max(0.0) as usize).min(self.size - 1));
        let mut changed = false;
        for y in y0..=y1 {
            for x in x0..=x1 {
                let dist = ((x as f32 - centre.0).powi(2) + (y as f32 - centre.1).powi(2)).sqrt();
                let v = ((radius + 2.0 - dist) / 2.0).clamp(0.0, 1.0);
                let i = y * self.size + x;
                if v > self.seen[i] && allowed(x, y) {
                    self.seen[i] = v;
                    changed = true;
                }
            }
        }
        self.dirty |= changed;
        changed
    }
    /// Opens a whole valley.
    pub fn reveal_valley(&mut self, world: &World, id: ValleyId) {
        for y in 0..self.size {
            for x in 0..self.size {
                if world.valley_of((x, y)) == Some(id) {
                    self.seen[y * self.size + x] = 1.0;
                }
            }
        }
        self.dirty = true;
    }
}

pub fn open_at(world: &World) -> impl Fn(usize, usize) -> bool + '_ {
    move |x, y| world.valley_of((x, y)).is_some_and(|v| world.is_open(v))
}

// ───────────────────────────── site suitability ─────────────────────────────

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SiteHint {
    /// Each 0..=1, higher is better.
    pub water: f32,
    pub trees: f32,
    pub safety: f32,
}

impl SiteHint {
    pub fn overall(&self) -> f32 {
        0.4 * self.water + 0.3 * self.trees + 0.3 * self.safety
    }
    pub fn verdict(&self) -> &'static str {
        match self.overall() {
            o if o >= 0.7 => "Excellent",
            o if o >= 0.5 => "Good",
            o if o >= 0.3 => "Poor",
            _ => "Bad",
        }
    }
}

/// Water nearness, tree cover and safety (gentle, above flood level) around `cell`.
pub fn site_hint(world: &World, cell: Cell) -> SiteHint {
    let n = world.size as i32;
    let (cx, cy) = (cell.0 as i32, cell.1 as i32);
    // water: distance to the nearest river/lake cell; best at 2-5 cells
    let mut nearest = f32::MAX;
    let mut river_low = f32::MAX;
    for dy in -12..=12i32 {
        for dx in -12..=12i32 {
            let (x, y) = (cx + dx, cy + dy);
            if x < 0 || y < 0 || x >= n || y >= n {
                continue;
            }
            let c = (x as usize, y as usize);
            if world.river.get(c.0, c.1) == Some(&true) || world.lake.get(c.0, c.1) == Some(&true) {
                nearest = nearest.min(((dx * dx + dy * dy) as f32).sqrt());
                river_low = river_low.min(*world.surface.get(c.0, c.1).unwrap());
            }
        }
    }
    let water = if nearest == f32::MAX {
        0.0
    } else if nearest < 1.0 {
        0.0 // standing in the water
    } else if nearest <= 5.0 {
        1.0
    } else {
        (1.0 - (nearest - 5.0) / 8.0).clamp(0.0, 1.0)
    };
    // trees: forest share within radius 6
    let (mut tot, mut forest, mut slope) = (0, 0, 0.0f32);
    for dy in -6..=6i32 {
        for dx in -6..=6i32 {
            let (x, y) = (cx + dx, cy + dy);
            if x < 0 || y < 0 || x >= n || y >= n || dx * dx + dy * dy > 36 {
                continue;
            }
            tot += 1;
            forest += world.forest.get(x as usize, y as usize).is_some_and(|t| t.is_some()) as i32;
            slope += world.slope.get(x as usize, y as usize).copied().unwrap_or(0.0);
        }
    }
    let tot = tot.max(1) as f32;
    let trees = ((forest as f32 / tot) / 0.4).clamp(0.0, 1.0);
    // safety: gentle ground and well above the nearby river
    let gentle = (1.0 - (slope / tot) / 0.8).clamp(0.0, 1.0);
    let above = if river_low == f32::MAX { 1.0 } else { ((world.surface.get(cell.0, cell.1).unwrap() - river_low) / 1.5).clamp(0.0, 1.0) };
    SiteHint { water, trees, safety: 0.5 * gentle + 0.5 * above }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SettleError {
    OutOfBounds,
    Locked,
    Unexplored,
    Water,
}

impl SettleError {
    pub fn message(self) -> &'static str {
        match self {
            SettleError::OutOfBounds => "Outside the map",
            SettleError::Locked => "That valley is not open yet",
            SettleError::Unexplored => "Unexplored - scouts are sent there",
            SettleError::Water => "Beavers cannot settle in the water",
        }
    }
}

pub fn can_settle(world: &World, depth: &Grid<f32>, fog: &Fog, cell: Cell) -> Result<(), SettleError> {
    let valley = world.valley_of(cell).ok_or(SettleError::OutOfBounds)?;
    if !world.is_open(valley) {
        return Err(SettleError::Locked);
    }
    if fog.get(cell) < EXPLORED {
        return Err(SettleError::Unexplored);
    }
    if world.river.get(cell.0, cell.1) == Some(&true) || world.lake.get(cell.0, cell.1) == Some(&true) || depth.get(cell.0, cell.1).is_some_and(|d| *d > 0.05) {
        return Err(SettleError::Water);
    }
    Ok(())
}

/// Best explored, settleable cell by overall suitability (used by `--autoplay-settle`).
pub fn best_site(world: &World, depth: &Grid<f32>, fog: &Fog) -> Option<Cell> {
    let mut best: Option<(f32, Cell)> = None;
    for y in (0..world.size).step_by(2) {
        for x in (0..world.size).step_by(2) {
            if can_settle(world, depth, fog, (x, y)).is_ok() {
                let s = site_hint(world, (x, y)).overall();
                if best.is_none_or(|(b, _)| s > b) {
                    best = Some((s, (x, y)));
                }
            }
        }
    }
    best.map(|b| b.1)
}

// ───────────────────────────── scouts ─────────────────────────────

#[derive(Clone, Copy, Debug)]
pub struct Scout {
    pub pos: (f32, f32),
    pub target: (f32, f32),
}

impl Scout {
    /// Walks toward the target; returns true once there.
    pub fn walk(&mut self, dt: f32) -> bool {
        let (dx, dy) = (self.target.0 - self.pos.0, self.target.1 - self.pos.1);
        let dist = (dx * dx + dy * dy).sqrt();
        let step = SCOUT_SPEED * dt;
        if dist <= step {
            self.pos = self.target;
            true
        } else {
            self.pos = (self.pos.0 + dx / dist * step, self.pos.1 + dy / dist * step);
            false
        }
    }
}

/// Nearest unexplored dry cell of an open valley, steering clear of `avoid` targets.
pub fn explore_target(world: &World, fog: &Fog, from: (f32, f32), avoid: &[(f32, f32)]) -> Option<(f32, f32)> {
    let open = open_at(world);
    let mut best: Option<(f32, (f32, f32))> = None;
    for y in (1..world.size).step_by(3) {
        for x in (1..world.size).step_by(3) {
            if fog.get((x, y)) >= EXPLORED || !open(x, y) || world.lake.get(x, y) == Some(&true) {
                continue;
            }
            let p = (x as f32, y as f32);
            let mut score = ((p.0 - from.0).powi(2) + (p.1 - from.1).powi(2)).sqrt();
            for a in avoid {
                if ((p.0 - a.0).powi(2) + (p.1 - a.1).powi(2)).sqrt() < 12.0 {
                    score += 40.0;
                }
            }
            if best.is_none_or(|(b, _)| score < b) {
                best = Some((score, p));
            }
        }
    }
    best.map(|b| b.1)
}

// ───────────────────────────── flow resource ─────────────────────────────

#[derive(Resource)]
pub struct Flow {
    pub phase: Phase,
    pub scouts: Vec<Scout>,
    pub fog: Fog,
    /// Valley mouth where the scouts arrive.
    #[allow(dead_code)]
    pub mouth: (f32, f32),
    pub arrival_t: f32,
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
        let scouts = (0..SCOUT_COUNT)
            .map(|i| {
                let a = i as f32 * std::f32::consts::TAU / SCOUT_COUNT as f32;
                let p = (mouth.0 + 2.0 * a.cos(), mouth.1 + 2.0 * a.sin());
                Scout { pos: p, target: p }
            })
            .collect();
        let mut fog = Fog::new(world.size);
        fog.reveal(mouth, SCOUT_SIGHT + 2.0, &open_at(world));
        Flow {
            phase: Phase::Arrival,
            scouts,
            fog,
            mouth,
            arrival_t: 0.0,
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
    can_settle(&sim.world, &sim.water.depth, &flow.fog, cell).map_err(|e| e.message().to_string())?;
    sim.apply(sim::Command::Settle { cell }).map_err(|e| e.to_string())?;
    after_settle(sim, flow, cell);
    Ok(())
}

/// The flow's side of a founded colony: advance the state machine, lift fog, aim the camera, log it.
pub fn after_settle(sim: &sim::SimState, flow: &mut Flow, cell: Cell) {
    let pos = (cell.0 as f32 + 0.5, cell.1 as f32 + 0.5);
    flow.fire(FlowEvent::Settle);
    let allowed = open_at(&sim.world);
    flow.fog.reveal(pos, 10.0, &allowed);
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
    pub hint: Option<SiteHint>,
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
    let world = &sim.0.world;
    let allowed = open_at(world);
    // camera focus lifts fog too
    let (fx, fy) = world_cell(&sim.0.height, orbit.target);
    flow.fog.reveal((fx, fy), FOCUS_SIGHT, &allowed);
    match flow.phase {
        Phase::Arrival => {
            flow.arrival_t += dt;
            if flow.arrival_t >= ARRIVAL_SECS {
                flow.fire(FlowEvent::ScoutsArrived);
            }
        }
        Phase::Scouting => {
            let targets: Vec<_> = flow.scouts.iter().map(|s| s.target).collect();
            let mut new_targets = targets.clone();
            for i in 0..flow.scouts.len() {
                let arrived = {
                    let s = &mut flow.scouts[i];
                    s.walk(dt)
                };
                if arrived {
                    let others: Vec<_> = new_targets.iter().enumerate().filter(|(j, _)| *j != i).map(|(_, t)| *t).collect();
                    let from = flow.scouts[i].pos;
                    if let Some(t) = explore_target(world, &flow.fog, from, &others) {
                        flow.scouts[i].target = t;
                        new_targets[i] = t;
                    }
                }
                let p = flow.scouts[i].pos;
                flow.fog.reveal(p, SCOUT_SIGHT, &allowed);
            }
        }
        _ => {}
    }
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
        flow.fog.reveal_valley(&sim.0.world, id);
        flow.push_log(format!("The colony reached valley {} - its fog lifts", id.0));
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
        assert_eq!(Arrival.on(ScoutsArrived), Some(Scouting));
        assert_eq!(Scouting.on(Settle), Some(Settled));
        assert_eq!(Settled.on(Collapse), Some(GameOver));
        assert_eq!(GameOver.on(Continue), Some(Watching));
        assert_eq!(Watching.on(Restart), Some(Arrival));
        assert_eq!(GameOver.on(Restart), Some(Arrival));
        // invalid transitions are refused
        assert_eq!(Arrival.on(Settle), None);
        assert_eq!(Scouting.on(Collapse), None);
        assert_eq!(Settled.on(Settle), None);
        assert_eq!(Arrival.on(Restart), None);
    }

    #[test]
    fn flow_fog_lifts_only_in_open_valleys() {
        let mut fog = Fog::new(16);
        assert_eq!(fog.explored_cells(), 0);
        let open_left = |x: usize, _y: usize| x < 8;
        assert!(fog.reveal((8.0, 8.0), 4.0, &open_left));
        assert!(fog.get((6, 8)) >= EXPLORED);
        assert_eq!(fog.get((10, 8)), 0.0, "locked side stays fogged");
        assert!(!fog.reveal((8.0, 8.0), 4.0, &open_left), "second reveal changes nothing");
    }

    #[test]
    fn flow_settling_founds_a_lodge_and_lifts_fog() {
        let mut s = sim::SimState::new(1);
        let mut flow = Flow::new(&s.world);
        // not allowed before scouts have arrived
        assert!(settle(&mut s, &mut flow, (5, 5)).is_err());
        flow.fire(FlowEvent::ScoutsArrived);
        // fogged ground is refused
        let fogged = (0..s.world.size).flat_map(|y| (0..s.world.size).map(move |x| (x, y))).find(|c| flow.fog.get(*c) < EXPLORED && s.world.valley_of(*c) == Some(s.world.start)).unwrap();
        assert!(settle(&mut s, &mut flow, fogged).is_err());
        assert!(!s.colony.is_founded());
        let before = flow.fog.explored_cells();
        let site = best_site(&s.world, &s.water.depth, &flow.fog).expect("a settleable site near the mouth");
        settle(&mut s, &mut flow, site).unwrap();
        assert_eq!(flow.phase, Phase::Settled);
        assert!(s.colony.is_founded());
        assert_eq!(s.colony.lodges.len(), 1);
        assert_eq!(s.colony.population(), 4);
        assert!(flow.fog.explored_cells() >= before);
        // a second settle is refused
        assert!(settle(&mut s, &mut flow, site).is_err());
    }

    #[test]
    fn flow_site_hint_prefers_water_and_trees() {
        let s = sim::SimState::new(1);
        let w = &s.world;
        let mut scored: Vec<(f32, Cell)> = Vec::new();
        for y in (0..w.size).step_by(4) {
            for x in (0..w.size).step_by(4) {
                scored.push((site_hint(w, (x, y)).overall(), (x, y)));
            }
        }
        scored.sort_by(|a, b| b.0.total_cmp(&a.0));
        let best = site_hint(w, scored[0].1);
        let worst = site_hint(w, scored[scored.len() - 1].1);
        assert!(best.overall() > worst.overall());
        assert!((0.0..=1.0).contains(&best.water) && (0.0..=1.0).contains(&best.trees) && (0.0..=1.0).contains(&best.safety));
    }

    #[test]
    fn flow_a_collapsed_colony_is_reported() {
        let mut s = sim::SimState::new(1);
        let mut flow = Flow::new(&s.world);
        flow.fire(FlowEvent::ScoutsArrived);
        let site = best_site(&s.world, &s.water.depth, &flow.fog).unwrap();
        settle(&mut s, &mut flow, site).unwrap();
        assert!(!s.colony.is_collapsed());
        assert!(flow.fire(FlowEvent::Collapse));
        assert_eq!(flow.phase, Phase::GameOver);
        assert!(flow.fire(FlowEvent::Restart));
    }
}
