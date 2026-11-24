include!(concat!(env!("OUT_DIR"), "/agents_constants.rs"));
use std::collections::BTreeMap;

use crate::buildings::{Building, BuildingId, BuildingKind, Good};
use crate::colony::BeaverId;
use crate::nav::{Gait, Goal, NavGrid, Path, Surface};
use crate::resources::{Resource, ResourceId, ResourceKind};
use crate::water::Cell;
use crate::work::{Area, Role, EPS as WORK_EPS, LABOUR_PER_DAY};
use crate::SimState;

#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Load {
    pub good: Good,
    pub amount: f32,
    /// Existing stock stays existing stock if a cancelled job returns it.
    pub booked: bool,
    /// Saplings: the tree they grow into. Fresh food: the tree whose seedlings the
    /// forager found where it foraged, brought home with the food.
    pub species: Option<crate::trees::Tree>,
}

/// The most of a good one beaver carries at once: pebbles are heavy, so a load is small.
pub fn load_size(good: Good) -> f32 {
    if good == Good::Pebbles { PEBBLE_LOAD } else { LOAD_SIZE }
}

impl Load {
    pub fn new(good: Good, amount: f32, booked: bool) -> Self {
        Load { good, amount, booked, species: None }
    }
    /// Its kind in a store: saplings by species, every other good as one.
    pub fn kind(&self) -> crate::buildings::Kind {
        self.species.filter(|_| self.good == Good::Saplings)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum Assignment {
    Resource(ResourceId),
    Area(ResourceKind, Area),
    Slot(BuildingId, usize),
    Go(Cell),
    /// Works as a maintainer: goes wherever repair is most needed, without a seat.
    Maintain,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum MoveState {
    #[default]
    None,
    Moving,
    Arrived,
    Blocked,
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum Task {
    Idle,
    Collect { target: ResourceId, progress: f32 },
    Carry { dest: BuildingId },
    Fetch { from: BuildingId, good: Good, then: BuildingId, site_delivery: bool },
    Build { site: BuildingId },
    Haul { to: BuildingId },
    Rest,
    /// Carry a load to one dam cell and pack it there.
    DamWork { dam: BuildingId, cell: Cell, progress: f32 },
    /// Dig one step out of a canal cell, or clear its silt.
    Dig { site: BuildingId, cell: Cell, progress: f32 },
    /// Walk to a gate (dam or canal) and set it as asked; on a new dam gate, build it.
    Gate { site: BuildingId, cell: Cell, progress: f32 },
    /// Take a sapling from a food store and plant it on one cell.
    Plant { store: BuildingId, cell: Cell, progress: f32 },
    /// Walk to goods lying on the ground and pick up a load of them for work at `then`.
    Pick { pile: u64, then: BuildingId },
    /// Walk to food lying on the ground and eat there.
    Eat { pile: u64 },
    /// Mend a worn building in place, drawing the materials from the stores.
    Mend { building: BuildingId },
    /// Collect seedlings from a grown tree, to carry to a forester's store.
    Seed { tree: ResourceId, progress: f32 },
    /// Keep watch beside a lookout; a guard leaves its post only to eat.
    Watch { lookout: BuildingId },
    /// A hauler's barge trip (see `harbor`): to the barge (leg 0), push it to `from` (1), load
    /// (2), push it to `to` (3), unload (4), take it back to its harbor (5), let it go (6).
    Barge { barge: u32, from: BuildingId, to: BuildingId, good: Good, leg: u8 },
}

impl Task {
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Agent {
    pub id: BeaverId,
    pub cell: Cell,
    pub step_frac: f32,
    pub gait: Gait,
    pub task: Task,
    pub assignment: Option<Assignment>,
    pub move_state: MoveState,
    pub load: Option<Load>,
    pub path: Option<Path>,
    pub inside: bool,
    pub stuck_ticks: u32,
    pub speed: f32,
    pub trail: Vec<(Cell, Gait)>,
    pub next_work_probe: f64,
    pub next_home_probe: f64,
    /// When a hungry beaver next looks for food lying on the ground.
    #[serde(default)]
    pub next_meal_probe: f64,
    /// The source a gatherer is working: it goes back to it after each load until it is used
    /// up, and only picks another when something else (fleeing, eating, resting, an order)
    /// comes between. Saved apart (see `save.rs`).
    #[serde(skip)]
    pub source: Option<ResourceId>,
}


/// A new pair's visit to their lodge: each goes to it, and once both are there they court for
/// `COURT_DAYS`, then go back to work.
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Courtship {
    pub partner: BeaverId,
    pub started: f64,
    /// When they met at the lodge.
    pub met: Option<f64>,
}

/// A beaver's body, lying where it died for `REMAINS_DAYS`.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Body {
    pub id: BeaverId,
    pub cell: Cell,
    pub cause: crate::colony::DeathCause,
    /// Its age when it died, for its size.
    pub age_days: f32,
    pub died: f64,
}

/// What outlasts a beaver that is gone from the colony: its body, or the beaver itself walking
/// off the map when it leaves.
#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct Remains {
    pub bodies: Vec<Body>,
    /// Beavers leaving the colony, each on its way to the map's edge, with its age.
    pub leaving: Vec<(Agent, f32)>,
}

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct Agents {
    pub by_id: BTreeMap<BeaverId, Agent>,
    pub unreachable: BTreeMap<ResourceId, f64>,
    /// The kind each one-resource order was for, so the beaver can go on to the next of that
    /// kind once it is used up. Worked out again every step while the resource stands, so it
    /// is not saved.
    #[serde(skip)]
    pub order_kinds: BTreeMap<BeaverId, ResourceKind>,
    /// Where each maintainer is working now. Chosen again every few steps, so it is not saved.
    #[serde(skip)]
    pub maint_targets: BTreeMap<BeaverId, BuildingId>,
    /// Cells from each cell to the nearest water, and the day it was measured (see `shore`).
    #[serde(skip)]
    pub(crate) shore: (f64, Vec<u16>),
    /// Bodies and leavers (saved after the world; see `save`).
    #[serde(skip)]
    pub remains: Remains,
    /// Every beaver's age as last seen, so a body is drawn the size the beaver was.
    #[serde(skip)]
    ages: BTreeMap<BeaverId, f32>,
    /// Newly paired beavers on their way to meet at their lodge, or courting there (see
    /// `court`). Not saved: after a reload they just go back to work.
    #[serde(skip)]
    pub courting: BTreeMap<BeaverId, Courtship>,
    /// The day of the last pairing taken up into `courting`.
    #[serde(skip)]
    paired_upto: f64,
    /// Beavers running from a wolf in sight, so they start their work afresh once it is gone.
    /// A load and a fresh start are all a reload loses, so it is not saved.
    #[serde(skip)]
    pub fleeing: std::collections::BTreeSet<BeaverId>,
    /// Weather too bitter to be out in: the colony keeps to its lodges until it eases (see
    /// `weather::Now::bitter`). Read again from the sky every step, so it is not saved.
    #[serde(skip)]
    pub holed_up: bool,
}


impl Agents {
    pub fn get(&self, id: BeaverId) -> Option<&Agent> {
        self.by_id.get(&id)
    }
    pub fn spawn(&mut self, id: BeaverId, cell: Cell) {
        self.by_id.insert(
            id,
            Agent {
                id,
                cell,
                step_frac: 0.0,
                gait: Gait::Walk,
                task: Task::Idle,
                assignment: None,
                move_state: MoveState::None,
                load: None,
                path: None,
                inside: false,
                stuck_ticks: 0,
                speed: 1.0,
                trail: Vec::new(),
                next_work_probe: 0.0,
                next_home_probe: 0.0,
                next_meal_probe: 0.0,
                source: None,
            },
        );
    }
    pub fn assign(&mut self, id: BeaverId, a: Option<Assignment>) -> bool {
        match self.by_id.get_mut(&id) {
            Some(ag) => {
                ag.next_work_probe=0.0;
                ag.next_home_probe=0.0;
                ag.assignment = a;
                ag.path = None;
                ag.step_frac = 0.0;
                ag.trail.clear();
                ag.stuck_ticks = 0;
                ag.move_state = if matches!(a, Some(Assignment::Go(_))) { MoveState::Moving } else { MoveState::None };
                ag.task = if ag.load.is_some() { Task::Carry { dest: NO_STORE } } else { Task::Idle };
                true
            }
            None => false,
        }
    }
}

const EPS: f32 = 1e-3;
const NO_STORE: BuildingId = BuildingId(0);
pub const SUPPLIES: BuildingId = BuildingId(u64::MAX);

fn place_cells(sim: &SimState, id: BuildingId) -> Vec<Cell> {
    if id == SUPPLIES {
        return sim.work.supplies.into_iter().collect();
    }
    sim.buildings.building(id).map(|b| b.cells().collect()).unwrap_or_default()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum SpawnError {
    NoSuchBeaver,
    AlreadySpawned,
    CellUnavailable,
}

pub fn spawn_on(sim: &mut SimState, id: BeaverId, cell: Cell) -> Result<(), SpawnError> {
    if sim.colony.beaver(id).is_none() {
        return Err(SpawnError::NoSuchBeaver);
    }
    if sim.agents.by_id.contains_key(&id) {
        return Err(SpawnError::AlreadySpawned);
    }
    if matches!(sim.nav.surface_at(cell), None | Some(Surface::Solid)) || !sim.nav.reserve(id, cell) {
        return Err(SpawnError::CellUnavailable);
    }
    sim.agents.spawn(id, cell);
    Ok(())
}

pub fn place_beavers(sim: &mut SimState) {
    let mut nav = std::mem::replace(&mut sim.nav, NavGrid::new(1, 1));
    nav.rebuild(sim);
    sim.nav = nav;
    sync_agents(sim);
}

/// Why a beaver is no longer in the colony, or how it came, from the colony's latest events.
fn latest_event(sim: &SimState, id: BeaverId) -> Option<&crate::colony::Event> {
    use crate::colony::Event;
    sim.colony.events.iter().rev().take(256).map(|e| &e.event).find(|e| match e {
        Event::Died { id: x, .. } | Event::Departed { id: x } | Event::Immigrated { id: x } => *x == id,
        _ => false,
    })
}

/// A cell near `from` a body can lie on or a beaver can stand on outside: land first, then water.
fn outside_near(sim: &SimState, from: Cell) -> Cell {
    let ring = |r: usize| {
        (from.1.saturating_sub(r)..=from.1 + r).flat_map(move |y| (from.0.saturating_sub(r)..=from.0 + r).map(move |x| (x, y)))
    };
    for want in [&[Surface::Land][..], &[Surface::Water, Surface::Canal][..]] {
        for r in 0..=4usize {
            if let Some(c) = ring(r).find(|&c| sim.nav.surface_at(c).is_some_and(|s| want.contains(&s))) {
                return c;
            }
        }
    }
    from
}

/// Map-edge cells a beaver can stand on, nearest `from` first.
fn edge_cells(sim: &SimState, from: Cell) -> Vec<Cell> {
    let n = sim.nav.surface.width().min(sim.nav.surface.height());
    let last = n - 1;
    let mut out: Vec<Cell> = (0..n)
        .flat_map(|i| [(i, 0), (i, last), (0, i), (last, i)])
        .filter(|&c| matches!(sim.nav.surface_at(c), Some(Surface::Land | Surface::Water | Surface::Canal)))
        .collect();
    out.sort_by_key(|c| (c.0.abs_diff(from.0).pow(2) + c.1.abs_diff(from.1).pow(2), *c));
    out.dedup();
    out
}

fn sync_agents(sim: &mut SimState) {
    use crate::colony::Event;
    for b in &sim.colony.beavers {
        sim.agents.ages.insert(b.id, b.age_days);
    }
    let since = sim.agents.paired_upto;
    let pairs: Vec<(f64, BeaverId, BeaverId)> = sim.colony.events.iter().rev().take(256).filter_map(|e| match e.event {
        Event::NewFamily { a, b, .. } if e.day > since => Some((e.day, a, b)),
        _ => None,
    }).collect();
    for (day, a, b) in pairs {
        sim.agents.paired_upto = sim.agents.paired_upto.max(day);
        for (who, partner) in [(a, b), (b, a)] {
            sim.agents.courting.insert(who, Courtship { partner, started: sim.time_days, met: None });
        }
    }
    let gone: Vec<BeaverId> =
        sim.agents.by_id.keys().copied().filter(|&id| sim.colony.beaver(id).is_none()).collect();
    for id in gone {
        let Some(mut ag) = sim.agents.by_id.remove(&id) else { continue };
        let age = sim.agents.ages.remove(&id).unwrap_or(crate::colony::ADULT_DAYS);
        match latest_event(sim, id).cloned() {
            // A body lies where the beaver fell; one that died in its lodge is laid outside it.
            Some(Event::Died { cause, .. }) => {
                sim.nav.release(id, ag.cell);
                let cell = if ag.inside { outside_near(sim, ag.cell) } else { ag.cell };
                sim.agents.remains.bodies.push(Body { id, cell, cause, age_days: age, died: sim.time_days });
            }
            // One that leaves walks off the map.
            Some(Event::Departed { .. }) => {
                let edge = edge_cells(sim, ag.cell).into_iter().take(12).find(|&e| sim.nav.may_reach(ag.cell, [e]));
                match edge {
                    Some(e) => {
                        ag.inside = false;
                        ag.load = None;
                        ag.path = None;
                        ag.task = Task::Idle;
                        ag.assignment = Some(Assignment::Go(e));
                        sim.agents.remains.leaving.push((ag, age));
                    }
                    None => sim.nav.release(id, ag.cell),
                }
            }
            _ => sim.nav.release(id, ag.cell),
        }
    }
    let missing: Vec<(BeaverId, (f32, f32))> = sim
        .colony
        .beavers
        .iter()
        .filter(|b| !sim.agents.by_id.contains_key(&b.id))
        .map(|b| (b.id, b.pos))
        .collect();
    for (id, pos) in missing {
        let (cx, cy) = (pos.0.max(0.0) as usize, pos.1.max(0.0) as usize);
        // A newcomer comes in from the map's edge and walks to its new home.
        if matches!(latest_event(sim, id), Some(Event::Immigrated { .. })) {
            let home = outside_near(sim, (cx, cy));
            let start = edge_cells(sim, home).into_iter().take(12).find(|&e| !sim.agents.by_id.values().any(|a| a.cell == e) && sim.nav.may_reach(e, [home]));
            if let Some(e) = start {
                if spawn_on(sim, id, e).is_ok() {
                    if let Some(a) = sim.agents.by_id.get_mut(&id) {
                        a.assignment = Some(Assignment::Go(home));
                        a.move_state = MoveState::Moving;
                    }
                    continue;
                }
            }
        }
        let free = (0..=SPAWN_RADIUS)
            .flat_map(|r| {
                (cy.saturating_sub(r)..=cy + r)
                    .flat_map(move |y| (cx.saturating_sub(r)..=cx + r).map(move |x| (x, y)))
                    .filter(move |&(x, y)| x.abs_diff(cx).max(y.abs_diff(cy)) == r)
            })
            .find(|&c| {
                matches!(sim.nav.surface_at(c), Some(Surface::Land | Surface::Water | Surface::Canal))
                    && !sim.agents.by_id.values().any(|a| a.cell == c)
            });
        if let Some(cell) = free {
            spawn_on(sim, id, cell).expect("cell checked free and beaver checked missing");
        }
    }
}

struct Job {
    kinds: &'static [ResourceKind],
    area: Option<Area>,
    only: Option<ResourceId>,
    min_age: f32,
    /// The role the work counts for, whose distance from water limits it (None: a direct order).
    role: Option<crate::jobs::RoleKind>,
    /// The ground frozen hard: no mud, clay or pebbles to dig, and the water plants under ice.
    frozen: bool,
}

/// Ground frozen too hard to dig, and still water iced over (see `weather::Now::ground_frozen`).
pub fn frozen(sim: &SimState) -> bool {
    !sim.sandbox && sim.water.sky.at(sim.time_days).ground_frozen()
}

/// Sources winter closes: dug from the ground or taken from under the water.
fn frozen_out(kind: ResourceKind) -> bool {
    matches!(kind, ResourceKind::MudBank | ResourceKind::RiverClay | ResourceKind::Pebbles | ResourceKind::WaterPlant)
}


fn gatherable(r: &Resource) -> bool {
    match r.kind {
        ResourceKind::Tree => crate::forestry::fellable(r),
        ResourceKind::Felled => r.amount > 0.0 || r.branches > 0.0,
        // A living plant is picked to nothing (`worth_starting` decides whether to begin one).
        ResourceKind::Plant | ResourceKind::WaterPlant => r.amount >= crate::resources::PLANT_BARE,
        ResourceKind::RiverClay => r.amount >= crate::resources::RIVER_CLAY_MIN_TAKE,
        _ => r.amount > 0.0,
    }
}

/// A fresh food plant is only worth setting out for with enough on it. A gatherer goes back
/// to its own source (`Agent::source`) whatever is left, and picks a plant bare, when it dies.
fn worth_starting(r: &Resource) -> bool {
    !r.kind.is_food_plant() || r.amount >= PLANT_PICK_MIN
}

pub(crate) fn gathering_available(sim: &SimState, assignment: Assignment) -> bool {
    Job::resolve(sim, assignment).is_some_and(|job| sim.resources.items().iter().any(|r| job.permits(r) && worth_starting(r)))
}

fn kinds_for(k: ResourceKind) -> &'static [ResourceKind] {
    k.gather_sources()
}

impl Job {
    fn resolve(sim: &SimState, a: Assignment) -> Option<Job> {
        match a {
            Assignment::Resource(id) => sim
                .resources
                .get(id)
                .filter(|r| gatherable(r) && (r.kind != ResourceKind::Pebbles || has_toolshed(sim)))
                .map(|_| Job { kinds: &[], area: None, only: Some(id), min_age: 0.0, role: None, frozen: frozen(sim) }),
            Assignment::Area(ResourceKind::Pebbles, _) if !has_toolshed(sim) => None,
            Assignment::Area(kind, area) => Some(Job { kinds: kinds_for(kind), area: Some(area), only: None, min_age: crate::forestry::cut_min_age(sim, None), role: Some(crate::jobs::gather_role(kind)), frozen: frozen(sim) }),
            Assignment::Slot(b, slot) => {
                let place = sim.work.place(b)?;
                match (place.slots.get(slot)?.role, place.area) {
                    (Role::Collect(kind), Some(area)) => {
                        Some(Job { kinds: kinds_for(kind), area: Some(area), only: None, min_age: crate::forestry::cut_min_age(sim, Some(b)), role: Some(crate::jobs::gather_role(kind)), frozen: frozen(sim) })
                    }
                    _ => None,
                }
            }
            Assignment::Go(_) | Assignment::Maintain => None,
        }
    }
    fn forages(&self) -> bool {
        self.kinds.contains(&ResourceKind::Plant)
    }
    fn permits(&self, r: &Resource) -> bool {
        if self.only.is_some_and(|id|id!=r.id) || self.area.is_some_and(|a|!a.contains(r.cell)) || (self.frozen && frozen_out(r.kind)) {return false;}
        let wanted = if self.forages() && r.kind == ResourceKind::Felled { r.bark > 0.0 } else { gatherable(r) };
        wanted
            && (self.only.is_some() || self.kinds.contains(&r.kind))
            && (r.kind != ResourceKind::Tree || crate::forestry::cuttable(r, self.min_age))
    }
    /// The good this job brings in, by its first kind of source.
    fn yield_good(&self) -> Option<Good> {
        if self.forages() {
            return Some(Good::Food);
        }
        match self.kinds.first()? {
            ResourceKind::Tree | ResourceKind::Felled => Some(Good::Sticks),
            k => k.yields().first().copied(),
        }
    }

    /// The good a resource yields to this job: bark is food to a forager.
    fn good_of(&self, r: &Resource) -> Good {
        match r.kind {
            ResourceKind::Felled if self.forages() => Good::Food,
            ResourceKind::Tree | ResourceKind::Felled => Good::Sticks,
            k => k.yields().first().copied().unwrap_or(Good::Sticks),
        }
    }

    /// The source a beaver at `from` should go to: the shortest round of walking there and
    /// carrying the load to where it is dropped off, among sources not already worked by as
    /// many beavers as they take. Kinds earlier in the job's list come first (felled trees
    /// before standing ones); land and water plants count as one kind, and so do river clay
    /// and mud banks.
    fn pick(&self, sim: &SimState, from: Cell) -> Option<ResourceId> {
        let rank = |r: &Resource| {
            let kind = match r.kind {
                ResourceKind::WaterPlant => ResourceKind::Plant,
                ResourceKind::RiverClay => ResourceKind::MudBank,
                k => k,
            };
            self.kinds.iter().position(|&k| k == kind).unwrap_or(0)
        };
        let reachable = |r: &Resource| {
            sim.agents.unreachable.get(&r.id).is_none_or(|until| sim.time_days >= *until) && sim.nav.may_reach_beside(from, r.cell)
        };
        let claims = claims(sim);
        let full = |r: &Resource| self.only.is_none() && claims.get(&r.id).copied().unwrap_or(0) >= gatherers_per(r.kind);
        let mut drops: [Option<Vec<Cell>>; Good::ALL.len()] = Default::default();
        // Nothing is fetched for a full store; a tree felled on a direct order still comes down.
        let mut wanted: [Option<bool>; Good::ALL.len()] = Default::default();
        // Connectivity is much more expensive than the walk; a source whose walk alone
        // cannot beat the current choice needs neither its haul nor a route test.
        let mut best: Option<((usize, u32, ResourceId), ResourceId)> = None;
        let mut consider = |r: &Resource| {
            if !self.permits(r) || !worth_starting(r) || full(r) {return;}
            if self.role.is_some_and(|role| !within_shore(sim, role, r.cell)) {return;}
            let walk = cell_dist(from, r.cell);
            if best.as_ref().is_some_and(|((k, c, _), _)| (rank(r), trip_key(walk)) > (*k, *c)) {return;}
            let good = self.good_of(r);
            let ordered_felling = self.only.is_some() && r.kind == ResourceKind::Tree;
            if !ordered_felling && !*wanted[good.index()].get_or_insert_with(|| worth_fetching(sim, from, good)) {return;}
            let spots = drops[good.index()].get_or_insert_with(|| drop_spots(sim, good, from));
            let haul = spots.iter().map(|&d| cell_dist(r.cell, d)).fold(f32::INFINITY, f32::min);
            let key = (rank(r), trip_key(walk + if haul.is_finite() { haul } else { 0.0 }), r.id);
            if best.as_ref().is_some_and(|(current, _)| key >= *current) {return;}
            if reachable(r) {best = Some((key, r.id));}
        };
        if let Some(id) = self.only {
            if let Some(r) = sim.resources.get(id) {consider(r);}
        } else {
            for r in sim.resources.items() {consider(r);}
        }
        best.map(|(_, id)| id)
    }
}

fn cell_dist(a: Cell, b: Cell) -> f32 {
    ((a.0 as f32 - b.0 as f32).powi(2) + (a.1 as f32 - b.1 as f32).powi(2)).sqrt()
}

/// A trip length as an ordered key, to a sixteenth of a cell.
fn trip_key(cells: f32) -> u32 {
    (cells * 16.0) as u32
}

/// How many beavers may work one source at once.
fn gatherers_per(kind: ResourceKind) -> usize {
    match kind {
        ResourceKind::Tree => GATHERERS_PER_TREE,
        ResourceKind::Felled => GATHERERS_PER_FELLED,
        ResourceKind::Plant | ResourceKind::WaterPlant => GATHERERS_PER_PLANT,
        ResourceKind::MudBank | ResourceKind::RiverClay | ResourceKind::Pebbles => GATHERERS_PER_DEPOSIT,
    }
}

/// Beavers on their way to, or working, each source.
fn claims(sim: &SimState) -> std::collections::HashMap<ResourceId, usize> {
    let mut out = std::collections::HashMap::new();
    for a in sim.agents.by_id.values() {
        if let Task::Collect { target, .. } = a.task {
            *out.entry(target).or_insert(0) += 1;
        }
    }
    out
}

/// Where a load of `good` can be dropped off: stores that take it, else the supply pile, else
/// the lodges; with none of those, where the beaver stands.
fn drop_spots(sim: &SimState, good: Good, from: Cell) -> Vec<Cell> {
    let stores: Vec<Cell> = sim.buildings.buildings.iter()
        .filter(|b| b.is_complete() && accepts(b.kind, good) && !sim.buildings.unusable(b, &sim.water.depth))
        .map(|b| { let (w, h) = b.kind.spec().footprint; (b.pos.0 + w / 2, b.pos.1 + h / 2) })
        .collect();
    if !stores.is_empty() {
        return stores;
    }
    if let Some(c) = sim.work.supplies {
        return vec![c];
    }
    let lodges: Vec<Cell> = sim.colony.lodges.iter().map(|l| (l.pos.0.max(0.0) as usize, l.pos.1.max(0.0) as usize)).collect();
    if lodges.is_empty() { vec![from] } else { lodges }
}

pub fn is_store(kind: BuildingKind) -> bool {
    Good::ALL.iter().any(|&g| accepts(kind, g))
}

pub fn accepts(kind: BuildingKind, good: Good) -> bool {
    match kind {
        BuildingKind::Warehouse => matches!(good, Good::Sticks | Good::Clay),
        BuildingKind::Larder => good == Good::Food,
        BuildingKind::Toolshed => good == Good::Pebbles,
        BuildingKind::Forester => good == Good::Saplings,
        _ => false,
    }
}

/// Collecting pebbles needs the tools kept at a finished tool shed.
/// Whether a role's workers may be sent to `c`: within the player's limit of cells from water.
pub fn within_shore(sim: &SimState, role: crate::jobs::RoleKind, c: Cell) -> bool {
    let Some(&max) = sim.work.shore_limits.get(&role) else { return true };
    let n = sim.nav.surface.width();
    sim.agents.shore.1.get(c.1 * n + c.0).is_none_or(|&d| d <= max)
}

/// Once a day while any role has a limit: how many steps (sideways or diagonal) each cell is
/// from the nearest water or canal.
fn measure_shore(sim: &mut SimState) {
    let day = sim.time_days.floor();
    if sim.work.shore_limits.is_empty() || (sim.agents.shore.0 == day && !sim.agents.shore.1.is_empty()) {
        return;
    }
    let (w, h) = (sim.nav.surface.width(), sim.nav.surface.height());
    let surf = sim.nav.surface.as_slice();
    let mut d = vec![u16::MAX; w * h];
    let mut queue = std::collections::VecDeque::new();
    for (i, s) in surf.iter().enumerate() {
        if matches!(s, Surface::Water | Surface::Canal) {
            d[i] = 0;
            queue.push_back(i);
        }
    }
    while let Some(i) = queue.pop_front() {
        let (x, y) = ((i % w) as i64, (i / w) as i64);
        for (dx, dy) in [(-1i64, 0i64), (1, 0), (0, -1), (0, 1), (-1, -1), (1, -1), (-1, 1), (1, 1)] {
            let (nx, ny) = (x + dx, y + dy);
            if nx < 0 || ny < 0 || nx >= w as i64 || ny >= h as i64 {
                continue;
            }
            let j = ny as usize * w + nx as usize;
            if d[j] == u16::MAX {
                d[j] = d[i] + 1;
                queue.push_back(j);
            }
        }
    }
    sim.agents.shore = (day, d);
}

pub fn has_toolshed(sim: &SimState) -> bool {
    sim.buildings.buildings.iter().any(|b| b.kind == BuildingKind::Toolshed && b.is_complete())
}

fn adjacent(c: Cell, b: &Building) -> bool {
    b.cells().any(|f| f.0.abs_diff(c.0) + f.1.abs_diff(c.1) == 1)
}

fn nearest_store(sim: &SimState, who: BeaverId, from: Cell, good: Good) -> Option<(BuildingId, Path)> {
    let mut stores:Vec<_>=sim.buildings
        .buildings
        .iter()
        .filter(|b| b.is_complete() && accepts(b.kind, good) && !sim.buildings.unusable(b, &sim.water.depth))
        .map(|b|(sim.nav.minimum_cost(crate::stock::dist(from,b).saturating_sub(1)),b)).collect();
    stores.sort_by(|a,b|a.0.total_cmp(&b.0).then(a.1.id.cmp(&b.1.id)));
    let mut best:Option<(BuildingId,Path)>=None;
    for (bound,b) in stores {
        // Even an all-canal path, with the fastest current behind it, cannot beat the known route.
        // Keep equal bounds: the lower building id still wins exact ties.
        if best.as_ref().is_some_and(|(_,path)|bound>path.cost) {break;}
        let path=if adjacent(from,b) {Path{steps:Vec::new(),cost:0.0}} else {
            let cells:Vec<_>=b.cells().collect();
            let Some(path)=sim.nav.find_path_beside_any(who,from,&cells) else {continue};path
        };
        if best.as_ref().is_none_or(|(id,p)|path.cost.total_cmp(&p.cost).then(b.id.cmp(id)).is_lt()) {best=Some((b.id,path));}
    }
    best
}

/// The cells of a store goods can be taken from: a building, or the starting supply pile.
fn store_cells(sim: &SimState, id: BuildingId) -> Option<Vec<Cell>> {
    if id == SUPPLIES {
        return sim.work.supplies.map(|c| vec![c]);
    }
    sim.buildings.building(id).map(|b| b.cells().collect())
}

/// A walk or swim from `from` to beside any of `cells`, and the cell it ends on.
fn route_beside(sim: &SimState, who: BeaverId, from: Cell, cells: &[Cell]) -> Option<(f32, Cell)> {
    if cells.iter().any(|c| c.0.abs_diff(from.0) + c.1.abs_diff(from.1) == 1) {
        return Some((0.0, from));
    }
    let path = sim.nav.find_path_beside_any(who, from, cells)?;
    Some((path.cost, path.steps.last().map_or(from, |s| s.0)))
}

/// Where a builder takes materials from: a store, or goods lying on the ground.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Source {
    Store(BuildingId),
    Pile(u64),
}

/// Where to fetch `good` for work at `to`: the store or pile on the ground with the shortest
/// walk or swim from the beaver to it and on to `to`. One that can fill a whole load comes first.
fn fetch_store(sim: &SimState, ag: &Agent, to: &[Cell], good: Good, want: f32) -> Option<Source> {
    let mut sources: Vec<(Source, f32, Vec<Cell>)> = crate::stock::holders(sim, good)
        .into_iter()
        .filter_map(|id| Some((Source::Store(id), sim.work.stock_of(id, good), store_cells(sim, id)?)))
        .collect();
    sources.extend(sim.work.piles.items.iter().filter(|p| p.good == good).map(|p| (Source::Pile(p.id), p.amount, vec![p.cell])));
    let pick = |min: f32| nearest_by_route(sim, ag, to, sources.iter().filter(|s| s.1 >= min.max(WORK_EPS)).map(|s| (s.0, s.2.clone())));
    pick(want).or_else(|| pick(WORK_EPS))
}

/// Of `places`, the one with the shortest walk or swim from the beaver to beside it and on to
/// beside `to` (no second leg when `to` is empty). Exact routes only for those that can win.
fn nearest_by_route<K: Copy + Ord>(sim: &SimState, ag: &Agent, to: &[Cell], places: impl Iterator<Item = (K, Vec<Cell>)>) -> Option<K> {
    // Fewest steps between two sets of cells; a step costs at least MIN_STEP, so this bounds a route.
    let steps = |a: &[Cell], b: &[Cell]| a.iter().flat_map(|p| b.iter().map(move |q| p.0.abs_diff(q.0).max(p.1.abs_diff(q.1)))).min().unwrap_or(0).saturating_sub(1);
    let mut places: Vec<(f32, K, Vec<Cell>)> = places
        .map(|(k, cells)| {
            let on = if to.is_empty() { 0.0 } else { sim.nav.minimum_cost(steps(&cells, to)) };
            (sim.nav.minimum_cost(steps(&[ag.cell], &cells)) + on, k, cells)
        })
        .collect();
    places.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
    let mut best: Option<(f32, K)> = None;
    for (bound, k, cells) in places {
        if best.is_some_and(|(cost, _)| bound > cost) {
            break;
        }
        let Some((there, at)) = route_beside(sim, ag.id, ag.cell, &cells) else { continue };
        let on = if to.is_empty() {
            0.0
        } else {
            let Some((on, _)) = route_beside(sim, ag.id, at, to) else { continue };
            on
        };
        let cost = there + on;
        if best.is_none_or(|(c, b)| cost.total_cmp(&c).then(k.cmp(&b)).is_lt()) {
            best = Some((cost, k));
        }
    }
    best.map(|b| b.1)
}

fn has_lodge(sim: &SimState) -> bool {
    sim.buildings.buildings.iter().any(|b| b.kind == BuildingKind::Shelter && b.is_complete())
}

fn home_of(sim: &SimState, who: BeaverId) -> Option<&Building> {
    let home = sim.colony.beaver(who)?.lodge?;
    let (&b, _) = sim.work.homes.iter().find(|(_, h)| **h == home)?;
    sim.buildings.building(b).filter(|b| b.is_complete())
}

fn route_home(sim: &SimState, who: BeaverId, from: Cell) -> Option<Path> {
    let cells = |b: &Building| b.cells().collect::<Vec<_>>();
    match home_of(sim, who) {
        Some(b) if b.kind == BuildingKind::Shelter => sim.nav.find_path_onto_any(who, from, &cells(b)),
        Some(b) => sim.nav.find_path_beside_any(who, from, &cells(b)),
        None => sim
            .buildings
            .buildings
            .iter()
            .filter(|b| b.kind == BuildingKind::Shelter && b.is_complete())
            .filter_map(|b| sim.nav.find_path_onto_any(who, from, &cells(b)))
            .min_by(|a, b| a.cost.total_cmp(&b.cost)),
    }
}

/// How fast a beaver moves for what it carries and where it is: full pace unladen, slower with a
/// load, slowest dragging wood over dry ground. A load floating in a canal deep enough to swim
/// costs nothing.
pub fn pace(nav: &NavGrid, ag: &Agent) -> f32 {
    let Some(load) = ag.load.as_ref() else { return 1.0 };
    if load.good == Good::Pebbles {
        return PEBBLE_PACE;
    }
    let surface = nav.surface_at(ag.cell);
    if surface == Some(Surface::Canal) && nav.floats(ag.cell) {
        return 1.0;
    }
    let dry = !matches!(surface, Some(Surface::Water | Surface::LodgeEntrance | Surface::Canal | Surface::Lock));
    if load.good == Good::Sticks && dry { STICKS_LAND_PACE } else { LADEN_PACE }
}

/// How fast a beaver goes, as a share of a walk (or swim) unladen: `pace`, or a barge's when
/// it is pushing one.
pub fn travel_pace(nav: &NavGrid, ag: &Agent) -> f32 {
    match ag.task {
        Task::Barge { leg: 1 | 3 | 5, .. } => crate::harbor::BARGE_PACE,
        _ => pace(nav, ag),
    }
}

pub(crate) fn follow(nav: &mut NavGrid, ag: &mut Agent, dt_days: f32) -> bool {
    let pace = travel_pace(nav, ag);
    let mut budget = ag.step_frac + dt_days * MOVE_COST_PER_DAY * pace;
    let mut moved = false;
    ag.trail.clear();
    loop {
        let Some(path) = ag.path.as_mut() else { break };
        let Some(&(next, gait)) = path.steps.first() else {
            ag.path = None;
            break;
        };
        // Swimming with the current is quicker than against it (`NavGrid::move_cost`).
        let cost = nav.move_cost(ag.cell, next, pace).unwrap_or_else(|| crate::nav::step_length(ag.cell, next));
        ag.gait = gait;
        if budget < cost {
            ag.step_frac = budget;
            return moved;
        }
        if nav.advance(ag.id, ag.cell, next).is_err() {
            ag.path = None;
            break;
        }
        budget -= cost;
        ag.cell = next;
        ag.trail.push((next, gait));
        path.steps.remove(0);
        ag.inside = nav.surface_at(next) == Some(Surface::LodgeInside);
        moved = true;
    }
    ag.step_frac = 0.0;
    moved
}

pub(crate) fn note(ag: &mut Agent, progressed: bool) {
    ag.stuck_ticks = if progressed { 0 } else { ag.stuck_ticks.saturating_add(1) };
}

enum Duty {
    None,
    Collect(Job),
    Build(BuildingId),
    Haul,
    /// A forester's work in its area: collect seedlings and plant them.
    Forest(Area),
    Mend(BuildingId),
    /// A guard's watch at its lookout.
    Guard(BuildingId),
}

fn duty_of(sim: &SimState, ag: &Agent, can_work: bool) -> Duty {
    if !can_work {
        return Duty::None;
    }
    match sim.work.slot_of(ag.id) {
        Some((b, _, Role::Build)) if sim.work.place(b).is_some_and(|p| p.site.is_some() || (!p.paused && (crate::dams::has_work_for(sim, b, false) || crate::canals::has_work_for(sim, b, false)))) => Duty::Build(b),
        Some((_, _, Role::Haul)) => Duty::Haul,
        Some((b, _, Role::Guard)) if sim.buildings.building(b).is_some_and(|x| x.is_complete()) => Duty::Guard(b),
        Some((b, _, Role::Forest)) => sim.work.place(b).and_then(|p| p.area).map_or(Duty::None, Duty::Forest),
        Some((b, i, Role::Collect(_))) => Job::resolve(sim, Assignment::Slot(b, i)).map_or(Duty::None, Duty::Collect),
        Some(_) => Duty::None,
        None if ag.assignment == Some(Assignment::Maintain) => match sim.agents.maint_targets.get(&ag.id) {
            Some(&b) if sim.buildings.building(b).is_some_and(|x| crate::maintenance::needs_mending(sim, x)) => Duty::Mend(b),
            Some(&b) => Duty::Build(b),
            None => Duty::None,
        },
        None => ag.assignment.and_then(|a| Job::resolve(sim, a)).map_or(Duty::None, Duty::Collect),
    }
}

fn promised(sim: &SimState, site: BuildingId, good: Good) -> f32 {
    sim.agents
        .by_id
        .values()
        .map(|a| match (&a.task, a.load) {
            (Task::Carry { dest }, Some(l)) if *dest == site && l.good == good => l.amount,
            (Task::Fetch { good: g, then, .. }, None) if *then == site && *g == good => load_size(good),
            (Task::Pick { pile, then }, None) if *then == site => sim.work.piles.get(*pile).filter(|p| p.good == good).map_or(0.0, |p| p.amount.min(load_size(good))),
            _ => 0.0,
        })
        .sum()
}

fn closest_store(sim: &SimState, from: Cell, good: Good) -> Option<BuildingId> {
    let d = |b: &Building| b.cells().map(|c| c.0.abs_diff(from.0).pow(2) + c.1.abs_diff(from.1).pow(2)).min().unwrap_or(0);
    sim.buildings.buildings.iter().filter(|b| b.is_complete() && accepts(b.kind, good) && !sim.buildings.unusable(b, &sim.water.depth)).min_by_key(|b| (d(b), b.id)).map(|b| b.id)
}

fn is_dam(sim: &SimState, id: BuildingId) -> bool {
    sim.buildings.building(id).is_some_and(|b| b.dam.is_some())
}

fn is_canal(sim: &SimState, id: BuildingId) -> bool {
    sim.buildings.building(id).is_some_and(|b| b.canal.is_some())
}

/// A canal digger walks to the next cell (or the gate) and works there; no load is needed.
/// A maintainer's dam or canal work is upkeep; a builder's is new work.
fn upkeep(sim: &SimState, who: BeaverId) -> bool {
    sim.agents.maint_targets.contains_key(&who)
}

fn canal_task(sim: &SimState, site: BuildingId, ag: &Agent) -> Option<Task> {
    let taken = crate::canals::claimed(sim, site, ag.id);
    let (cell, gate) = crate::canals::next_job(sim, site, &taken, upkeep(sim, ag.id))?;
    if !gate && frozen(sim) {
        return None;
    }
    Some(if gate { Task::Gate { site, cell, progress: 0.0 } } else { Task::Dig { site, cell, progress: 0.0 } })
}

/// A dam builder carries one load to the lowest unfinished cell. A gate waiting to be built
/// or turned comes first, and needs no load.
fn dam_task(sim: &SimState, dam: BuildingId, ag: &Agent) -> Option<Task> {
    if let Some(cell) = crate::dams::gate_job(sim, dam, &crate::dams::gates_claimed(sim, dam, ag.id)) {
        return Some(Task::Gate { site: dam, cell, progress: 0.0 });
    }
    let taken = crate::dams::claimed(sim, dam, ag.id);
    let cell = crate::dams::next_cell(sim, dam, ag.cell, &taken, upkeep(sim, ag.id))?;
    if ag.load.is_some_and(|l| l.good == Good::Sticks) {
        return Some(Task::DamWork { dam, cell, progress: 0.0 });
    }
    let want = crate::dams::sticks_per_step().min(LOAD_SIZE);
    Some(match fetch_store(sim, ag, &[cell], Good::Sticks, want)? {
        Source::Store(from) => Task::Fetch { from, good: Good::Sticks, then: dam, site_delivery: true },
        Source::Pile(pile) => Task::Pick { pile, then: dam },
    })
}

fn build_task(sim: &SimState, site: BuildingId, ag: &Agent) -> Option<Task> {
    if is_dam(sim, site) && !sim.maintenance.jobs.contains_key(&site) {
        return dam_task(sim, site, ag);
    }
    if is_canal(sim, site) {
        return canal_task(sim, site, ag);
    }
    if sim.buildings.building(site).is_none_or(|b| b.is_complete() && !sim.maintenance.jobs.contains_key(&site)) {
        return None;
    }
    let progress = sim.work.place(site)?.site.as_ref()?;
    if progress.labour_open() {
        return Some(Task::Build { site });
    }
    site_fetch_task(sim, site, ag)
}

fn site_fetch_task(sim: &SimState, site: BuildingId, ag: &Agent) -> Option<Task> {
    let progress = sim.work.place(site)?.site.as_ref()?;
    for &(good, _) in &progress.needed {
        let outstanding = progress.missing(good) - promised(sim, site, good);
        let older_wants = sim.work.places.iter().filter(|p| p.building < site).filter_map(|p| p.site.as_ref().map(|s| (p.building, s))).any(|(b, s)| {
            sim.buildings.building(b).is_some_and(|bd| !bd.is_complete()) && s.missing(good) - promised(sim, b, good) > WORK_EPS
        });
        let in_stock: f32 = crate::stock::holders(sim, good).iter().map(|&h| sim.work.stock_of(h, good)).sum();
        if older_wants && in_stock < SITE_RESERVE_LOADS * LOAD_SIZE {
            continue;
        }
        if outstanding > WORK_EPS {
            let want = outstanding.min(load_size(good));
            let to: Vec<Cell> = sim.buildings.building(site).map_or_else(|| vec![ag.cell], |b| b.cells().collect());
            match fetch_store(sim, ag, &to, good, want) {
                Some(Source::Store(from)) => return Some(Task::Fetch { from, good, then: site, site_delivery: true }),
                Some(Source::Pile(pile)) => return Some(Task::Pick { pile, then: site }),
                None => {}
            }
        }
    }
    None
}

/// A haul between two stores goes by barge when there are at least two loads to move, barge
/// water joins the two, and a barge is free; otherwise as it is, a load at a time.
fn by_barge(sim: &SimState, ag: &Agent, t: Task) -> Task {
    let Task::Fetch { from, good, then, site_delivery: false } = t else { return t };
    if from == SUPPLIES || then == SUPPLIES || sim.fleet.barges.is_empty() || sim.work.stock_of(from, good) < 2.0 * load_size(good) {
        return t;
    }
    let Some(barge) = crate::harbor::free_barge(sim, ag.cell) else { return t };
    let harbor = sim.fleet.get(barge).map(|b| b.harbor);
    if !harbor.is_some_and(|h| crate::harbor::joined(sim, h, from)) || !crate::harbor::joined(sim, from, then) {
        return t;
    }
    Task::Barge { barge, from, to: then, good, leg: 0 }
}

/// One turn of a barge trip (see `Task::Barge`).
#[allow(clippy::too_many_arguments)]
fn barge_trip(sim: &mut SimState, ag: &mut Agent, dt: f32, duty: &Duty, barge: u32, from: BuildingId, to: BuildingId, good: Good, leg: u8) {
    use crate::harbor;
    let let_go = |sim: &mut SimState, ag: &mut Agent| {
        let at = ag.cell;
        if let Some(b) = sim.fleet.get_mut(barge).filter(|b| b.hauler == Some(ag.id)) {
            b.hauler = None;
            if harbor::barge_water(sim, at) {
                if let Some(b) = sim.fleet.get_mut(barge) {
                    b.cell = at;
                }
            }
        }
        ag.path = None;
        ag.task = if ag.load.is_some() { Task::Carry { dest: NO_STORE } } else { Task::Idle };
    };
    let Some(b) = sim.fleet.get(barge).copied() else { return let_go(sim, ag) };
    if !matches!(duty, Duty::Haul) || b.hauler.is_some_and(|h| h != ag.id) {
        return let_go(sim, ag);
    }
    let docks_of = |sim: &SimState, id: BuildingId| sim.buildings.building(id).map(|x| harbor::docks(sim, x)).unwrap_or_default();
    let next = |ag: &mut Agent, leg: u8| {
        ag.path = None;
        ag.task = Task::Barge { barge, from, to, good, leg };
    };
    match leg {
        // To the barge, and on board.
        0 => {
            if let Some(x) = sim.fleet.get_mut(barge) {
                x.hauler = Some(ag.id);
            }
            if ag.cell == b.cell {
                return next(ag, 1);
            }
            if ag.path.is_none() {
                ag.path = sim.nav.find_path(ag.id, ag.cell, Goal::Cell(b.cell));
                if ag.path.is_none() {
                    return let_go(sim, ag);
                }
            }
            let moved = follow(&mut sim.nav, ag, dt);
            note(ag, moved);
        }
        // Pushing it: to the first store, to the second, home to its harbor.
        1 | 3 | 5 => {
            let target = docks_of(sim, match leg { 1 => from, 3 => to, _ => b.harbor });
            if target.contains(&ag.cell) {
                return next(ag, leg + 1);
            }
            // Water that has iced over (or run too shallow) since the way was planned: plan
            // again, or tie the barge up where it is.
            if ag.path.as_ref().and_then(|p| p.steps.first()).is_some_and(|st| !harbor::barge_water(sim, st.0)) {
                ag.path = None;
            }
            if ag.path.is_none() {
                ag.path = harbor::route(sim, ag.cell, &target);
                if ag.path.is_none() {
                    return let_go(sim, ag);
                }
            }
            let moved = follow(&mut sim.nav, ag, dt);
            let at = ag.cell;
            if let Some(x) = sim.fleet.get_mut(barge) {
                x.cell = at;
            }
            note(ag, moved);
        }
        // Loading as much as the barge takes.
        2 => {
            let want = (harbor::BARGE_LOADS * load_size(good)).min(sim.work.stock_of(from, good));
            let (got, kind) = if want > WORK_EPS { sim.buildings.stores.take_kind_from(from, good, want) } else { (0.0, None) };
            log_load(sim, ag, "TAKE_FOR_BARGE", from, good, want, got, &format!("to={} barge={barge}", to.0));
            if got > WORK_EPS {
                ag.load = Some(Load { species: kind, ..Load::new(good, got, true) });
                next(ag, 3);
            } else {
                next(ag, 5);
            }
            note(ag, true);
        }
        // Unloading into the second store; what it cannot take is carried on as usual.
        4 => {
            if let Some(load) = ag.load {
                let who = producer(sim, ag);
                let taken = stow(sim, to, load, who).unwrap_or(0.0);
                log_load(sim, ag, "DELIVER_BARGE", to, load.good, load.amount, taken, &format!("barge={barge}"));
                ag.load = (load.amount - taken > WORK_EPS).then(|| Load { amount: load.amount - taken, ..load });
            }
            note(ag, true);
            next(ag, 5);
        }
        _ => let_go(sim, ag),
    }
}

fn site_haul_task(sim: &SimState, ag: &Agent) -> Option<Task> {
    let mut sites: Vec<_> = sim.work.places.iter().filter(|p| is_site(sim, p) && !p.paused).collect();
    sites.sort_by_key(|p| (std::cmp::Reverse(p.priority), p.building));
    sites.into_iter().find_map(|p| site_fetch_task(sim, p.building, ag))
}

fn haul_task(sim: &SimState) -> Option<Task> {
    let ok = |id| sim.buildings.building(id).is_some_and(|b| b.is_complete());
    let open = |id| sim.buildings.building(id).is_some_and(|b| b.is_complete() && !sim.buildings.unusable(b, &sim.water.depth));
    sim.work
        .hauls
        .iter()
        .find(|o| ok(o.from) && open(o.to) && sim.work.stock_of(o.from, o.good) > WORK_EPS)
        .map(|o| Task::Fetch { from: o.from, good: o.good, then: o.to, site_delivery: false })
}

/// How far round a gatherer's area it looks for its goods lying on the ground (cells).
const PILE_REACH: usize = 8;

/// How far a beaver whose one resource is used up looks for the next one of its kind (cells).
const ORDER_REACH: usize = 24;
/// How often (in steps) workers check that their target still has work for them.
const RETARGET_STEPS: u64 = 4;

/// The nearest gatherable resource of `kind` (felled trees before standing ones, as gatherers
/// rank them) within `reach` of `from`.
fn next_resource(sim: &SimState, kind: ResourceKind, from: Cell, reach: usize) -> Option<ResourceId> {
    let area = Area { min: (from.0.saturating_sub(reach), from.1.saturating_sub(reach)), max: (from.0 + reach, from.1 + reach) };
    let job = Job::resolve(sim, Assignment::Area(kind, area))?;
    job.pick(sim, from)
}

/// Whether a workplace still has work for a seat of this role: a site with labour or materials
/// to come, a dam or canal with cells to build or keep, or a gathering area with something left.
fn seat_has_work(sim: &SimState, b: BuildingId, role: Role) -> bool {
    let Some(p) = sim.work.place(b) else { return false };
    if p.paused {
        // Paused by the player: its workers wait there.
        return true;
    }
    match role {
        Role::Build => {
            if p.site.is_some() && is_site(sim, p) {
                return !site_stalled(sim, p);
            }
            crate::dams::has_work_for(sim, b, false) || crate::canals::has_work_for(sim, b, false)
        }
        Role::Collect(_) => p
            .slots
            .iter()
            .position(|s| s.role == role)
            .and_then(|i| Job::resolve(sim, Assignment::Slot(b, i)))
            .is_some_and(|j| sim.resources.items().iter().any(|r| j.permits(r) && worth_starting(r))),
        _ => true,
    }
}

fn reaches_place(sim: &SimState, from: Cell, b: BuildingId) -> bool {
    place_cells(sim, b).iter().any(|&c| sim.nav.may_reach_beside(from, c))
}

/// A worker whose target is done or out of reach goes on to the next one: a seated builder,
/// maintainer or gatherer to the nearest workplace of the same job with work and an open seat;
/// a gatherer sent to an area that has run out, to the nearest spot with more of the same.
fn retarget(sim: &mut SimState) {
    if sim.steps % RETARGET_STEPS != 0 {
        return;
    }
    let workers: Vec<(BeaverId, Cell)> = sim
        .colony
        .beavers
        .iter()
        .filter(|b| b.can_work())
        .filter_map(|b| sim.agents.get(b.id).map(|a| (b.id, a.cell)))
        .collect();
    for (who, at) in workers {
        match sim.work.slot_of(who) {
            Some((from, i, role)) if matches!(role, Role::Build | Role::Collect(_)) => {
                if seat_has_work(sim, from, role) && reaches_place(sim, at, from) {
                    continue;
                }
                let kind = crate::jobs::role_kind(role);
                let d = |b: BuildingId| sim.buildings.building(b).map_or(usize::MAX, |bd| bd.pos.0.abs_diff(at.0).pow(2) + bd.pos.1.abs_diff(at.1).pow(2));
                let next = sim
                    .work
                    .places
                    .iter()
                    .filter(|p| p.building != from && !p.paused)
                    .filter(|p| sim.buildings.building(p.building).is_some_and(|b| (b.is_complete() || is_site(sim, p)) && !sim.buildings.unusable(b, &sim.water.depth)))
                    .filter(|p| role != Role::Build || takes_builders(sim, p))
                    .filter_map(|p| {
                        let (j, r) = p.open_seat(|r| crate::jobs::role_kind(r) == kind)?;
                        Some((p, j, r))
                    })
                    .filter(|(p, _, r)| seat_has_work(sim, p.building, *r) && reaches_place(sim, at, p.building))
                    .min_by_key(|(p, _, _)| (std::cmp::Reverse(p.priority), d(p.building), p.building))
                    .map(|(p, j, _)| (p.building, j));
                let Some((to, j)) = next else { continue };
                let _ = sim.work.unassign(from, i);
                if sim.work.assign(to, j, who).is_ok() {
                    if let Ok(p) = sim.work.place_mut(to) {
                        p.max_workers = p.max_workers.max(p.filled());
                    }
                    sim.agents.assign(who, None);
                    if crate::trace::enabled() {
                        crate::trace::event(format!("RESEAT {} beaver={} from={} to={} role={role:?}", crate::trace::sim_stamp(), who.0, from.0, to.0));
                    }
                }
            }
            Some(_) => {}
            None => {
                let Some(Assignment::Area(kind, area)) = sim.agents.get(who).and_then(|a| a.assignment) else { continue };
                if gathering_available(sim, Assignment::Area(kind, area)) {
                    continue;
                }
                let Some(r) = next_resource(sim, kind, at, GATHER_RADIUS).and_then(|id| sim.resources.get(id)) else { continue };
                // The same area, moved to centre on the nearest of the same kind.
                let (hw, hh) = ((area.max.0 - area.min.0) / 2, (area.max.1 - area.min.1) / 2);
                let n = sim.world.size;
                let c = r.cell;
                let moved = Area { min: (c.0.saturating_sub(hw), c.1.saturating_sub(hh)), max: ((c.0 + hw).min(n - 1), (c.1 + hh).min(n - 1)) };
                sim.agents.assign(who, Some(Assignment::Area(kind, moved)));
                if crate::trace::enabled() {
                    crate::trace::event(format!("REAREA {} beaver={} kind={kind:?} from={area:?} to={moved:?}", crate::trace::sim_stamp(), who.0));
                }
            }
        }
    }
}

/// What a gatherer's job left on the ground while no store would take it: once one will,
/// carrying that in comes before gathering more, or it rots where it lies. An order for one
/// resource goes straight to it.
/// Back to the gatherer's own source after a load, while it has something left for this job,
/// can be reached, and no one else has taken it over meanwhile.
fn own_source(sim: &SimState, j: &Job, ag: &Agent) -> Option<Task> {
    let id = ag.source?;
    let r = sim.resources.get(id).filter(|r| j.permits(r))?;
    if !(j.only.is_some() && r.kind == ResourceKind::Tree) && !worth_fetching(sim, ag.cell, j.good_of(r)) {
        return None;
    }
    if sim.agents.unreachable.get(&id).is_some_and(|until| sim.time_days < *until) {
        return None;
    }
    if sim.agents.by_id.values().filter(|a| matches!(a.task, Task::Collect { target, .. } if target == id)).count() >= gatherers_per(r.kind) {
        return None;
    }
    Some(Task::Collect { target: id, progress: 0.0 })
}

fn pile_task(sim: &SimState, j: &Job, ag: &Agent) -> Option<Task> {
    if j.only.is_some() || sim.work.piles.items.is_empty() {
        return None;
    }
    let good = j.yield_good()?;
    if sim.buildings.stores.space(good) <= WORK_EPS {
        return None;
    }
    let near = j.area.map(|a| Area { min: (a.min.0.saturating_sub(PILE_REACH), a.min.1.saturating_sub(PILE_REACH)), max: (a.max.0 + PILE_REACH, a.max.1 + PILE_REACH) });
    let claimed = |pile: u64| sim.agents.by_id.values().filter(|a| a.id != ag.id && matches!(a.task, Task::Pick { pile: p, then } if p == pile && then == NO_STORE)).count() as f32;
    sim.work
        .piles
        .items
        .iter()
        .filter(|p| p.good == good && near.is_none_or(|a| a.contains(p.cell)))
        .filter(|p| p.amount > claimed(p.id) * load_size(good) + WORK_EPS)
        .filter(|p| closest_store(sim, p.cell, good).is_some())
        .map(|p| (trip_key(cell_dist(ag.cell, p.cell)), p))
        .filter(|(_, p)| sim.nav.may_reach_beside(ag.cell, p.cell))
        .min_by_key(|(d, p)| (*d, p.id))
        .map(|(_, p)| Task::Pick { pile: p.id, then: NO_STORE })
}

fn next_task(sim: &SimState, duty: &Duty, ag: &Agent) -> Option<Task> {
    let _p = crate::prof::scope(match duty {
        Duty::None => "+choose work/none",
        Duty::Collect(_) => "+choose work/collect",
        Duty::Build(_) => "+choose work/build",
        Duty::Forest(_) => "+choose work/forest",
        Duty::Haul => "+choose work/haul",
        Duty::Mend(_) => "+choose work/mend",
        Duty::Guard(_) => "+choose work/guard",
    });
    match duty {
        Duty::None => None,
        Duty::Collect(j) => own_source(sim, j, ag)
            .or_else(|| pile_task(sim, j, ag))
            .or_else(|| j.pick(sim, ag.cell).map(|t| Task::Collect { target: t, progress: 0.0 })),
        Duty::Build(site) => build_task(sim, *site, ag),
        Duty::Mend(b) => Some(Task::Mend { building: *b }),
        Duty::Guard(b) => Some(Task::Watch { lookout: *b }),
        // Plant what the stores hold, else collect more seedlings while there is room.
        Duty::Forest(area) => plant_task(sim, *area, ag).or_else(|| seed_task(sim, *area, ag)),
        Duty::Haul => site_haul_task(sim, ag).or_else(|| haul_task(sim).map(|t| by_barge(sim, ag, t))).or_else(|| {
            let (from, then, good) = crate::stock::fill_move(sim)?;
            Some(by_barge(sim, ag, Task::Fetch { from, good, then, site_delivery: false }))
        }).or_else(|| {
            let own = sim.work.slot_of(ag.id).map(|s| s.0).filter(|b| sim.work.balanced.contains(b))?;
            let (from, then, good) = crate::stock::balance_move(sim, Some(own))?;
            Some(by_barge(sim, ag, Task::Fetch { from, good, then, site_delivery: false }))
        }),
    }
}

fn tick(sim: &mut SimState, ag: &mut Agent, dt: f32) {
    // A trail belongs to this step, including steps spent working or resting.
    ag.trail.clear();
    if crate::predators::flee(sim, ag, dt) {
        ag.source = None;
        return;
    }
    if let Some(Assignment::Go(to)) = ag.assignment {
        ag.source = None;
        go_to(sim, ag, dt, to);
        return;
    }
    if court(sim, ag, dt) || follow_parent(sim, ag, dt) {
        ag.source = None;
        return;
    }
    if let Some(pile) = hungry_for_ground_food(sim, ag) {
        ag.path = None;
        ag.task = Task::Eat { pile };
    }
    let (can_work, tired) = sim.colony.beaver(ag.id).map_or((false, false), |b| (b.can_work(), b.resting));
    // In bitter cold the colony keeps to its lodges, as tired beavers do, finishing a load first.
    let holed = sim.agents.holed_up && has_lodge(sim);
    let duty = duty_of(sim, ag, can_work && !tired && !holed);
    ag.speed = sim.colony.beaver(ag.id).map_or(1.0, |b| crate::society::work_speed(b.mood));
    // A guard rests at its post.
    if (tired || holed) && ag.load.is_none() && !ag.inside && !matches!(ag.task, Task::Rest | Task::Carry { .. } | Task::Eat { .. }) && (holed || !matches!(ag.task, Task::Watch { .. })) {
        ag.path = None;
        ag.task = Task::Rest;
    }
    if let Some(Assignment::Resource(id)) = ag.assignment {
        if let Some(r) = sim.resources.get(id) {
            sim.agents.order_kinds.insert(ag.id, r.kind);
        }
    }
    if matches!(duty, Duty::None) && can_work && !tired && matches!(ag.assignment, Some(Assignment::Resource(_))) {
        // That one is done with: on to the nearest of the same kind, or free if there is none.
        let kind = sim.agents.order_kinds.get(&ag.id).copied();
        ag.assignment = kind.and_then(|k| next_resource(sim, k, ag.cell, ORDER_REACH)).map(Assignment::Resource);
        ag.task = if ag.load.is_some() { Task::Carry { dest: NO_STORE } } else { Task::Idle };
        ag.path = None;
    }
    // Anything but gathering and taking the load in (eating, resting, other work) breaks a
    // gatherer's round: afterwards it picks its source afresh.
    if !matches!(ag.task, Task::Idle | Task::Collect { .. } | Task::Carry { .. }) {
        ag.source = None;
    }
    match ag.task.clone() {
        Task::Idle => {
            ag.path = None;
            let dam_load = (matches!(duty, Duty::Build(b) if is_dam(sim, b) && !sim.maintenance.jobs.contains_key(&b)) && ag.load.is_some_and(|l| l.good == Good::Sticks))
                || (matches!(duty, Duty::Forest(..)) && ag.load.is_some_and(|l| l.good == Good::Saplings));
            let dam_next = if dam_load { next_task(sim, &duty, ag) } else { None };
            if let Some(t) = dam_next {
                ag.task = t;
            } else if ag.load.is_some() {
                ag.task = Task::Carry { dest: NO_STORE };
            } else if let Some(t) = next_task(sim, &duty, ag) {
                ag.task = t;
            } else if !ag.inside && has_lodge(sim) {
                ag.task = Task::Rest;
            }
        }
        Task::Rest => {
            let work = if sim.time_days >= ag.next_work_probe {
                ag.next_work_probe=sim.time_days+crate::STEP_DAYS as f64;
                next_task(sim, &duty, ag).is_some()
            } else {false};
            if ag.load.is_some() || work {
                ag.task = Task::Idle;
                return;
            }
            if at_resting_place(ag, home_of(sim, ag.id)) {
                ag.path = None;
                note(ag,true);
                return;
            }
            if ag.path.is_none() {
                if sim.time_days < ag.next_home_probe { return; }
                ag.path = route_home(sim, ag.id, ag.cell);
                if ag.path.is_none() { ag.next_home_probe=sim.time_days+crate::STEP_DAYS as f64; }
            }
            let moved = follow(&mut sim.nav, ag, dt);
            note(ag, moved);
        }
        Task::DamWork { dam, cell, progress } => dam_work(sim, ag, dt, &duty, dam, cell, progress),
        Task::Dig { site, cell, progress } => waterworks_visit(sim, ag, dt, &duty, site, cell, progress, false),
        Task::Gate { site, cell, progress } => waterworks_visit(sim, ag, dt, &duty, site, cell, progress, true),
        Task::Collect { target, .. } => collect(sim, ag, dt, duty, target),
        Task::Plant { store, cell, progress } => plant(sim, ag, dt, &duty, store, cell, progress),
        Task::Carry { dest } if ag.load.is_some_and(|l| sim.work.place(dest).and_then(|p|p.site.as_ref()).is_some_and(|p|p.missing(l.good)>WORK_EPS)) => deliver(sim, ag, dt, dest),
        Task::Carry { dest } => carry(sim, ag, dt, dest),
        Task::Fetch { from, good, then, .. } if is_dam(sim, then) && !sim.maintenance.jobs.contains_key(&then) => dam_fetch(sim, ag, dt, &duty, from, good, then),
        Task::Fetch { from, good, then, site_delivery } => fetch(sim, ag, dt, &duty, from, good, then, site_delivery),
        Task::Build { site } => build(sim, ag, dt, &duty, site),
        Task::Haul { to } => haul(sim, ag, dt, to),
        Task::Pick { pile, then } => pick_up(sim, ag, dt, &duty, pile, then),
        Task::Eat { pile } => eat_on_ground(sim, ag, dt, pile),
        Task::Mend { building } => mend(sim, ag, dt, &duty, building),
        Task::Seed { tree, progress } => seed(sim, ag, dt, &duty, tree, progress),
        Task::Barge { barge, from, to, good, leg } => barge_trip(sim, ag, dt, &duty, barge, from, to, good, leg),
        Task::Watch { lookout } => {
            if !matches!(duty, Duty::Guard(b) if b == lookout) {
                ag.path = None;
                ag.task = Task::Idle;
            } else if approach(sim, ag, dt, lookout) {
                note(ag, true);
            }
        }
    }
}

/// Walk to a pile on the ground and pick up a load for the work at `then`.
fn pick_up(sim: &mut SimState, ag: &mut Agent, dt: f32, duty: &Duty, pile: u64, then: BuildingId) {
    // A gatherer fetches its own goods off the ground for a store (`then` is no building).
    let wanted = matches!(duty, Duty::Build(b) if *b == then) || matches!(duty, Duty::Haul) || (then == NO_STORE && matches!(duty, Duty::Collect(_)));
    let Some(p) = sim.work.piles.get(pile).cloned().filter(|_| wanted && ag.load.is_none()) else {
        return abandon(ag);
    };
    match reach(sim, ag, dt, p.cell) {
        None => return abandon(ag),
        Some(false) => return,
        Some(true) => {}
    }
    let want = if then == NO_STORE {
        load_size(p.good)
    } else if is_dam(sim, then) {
        crate::dams::sticks_per_step().min(LOAD_SIZE)
    } else {
        let missing = sim.work.place(then).and_then(|w| w.site.as_ref()).map_or(0.0, |s| s.missing(p.good));
        missing.min(load_size(p.good))
    };
    let got = sim.work.piles.take(pile, want);
    log_load(sim, ag, "TAKE_GROUND", then, p.good, want, got, &format!("pile={pile}"));
    if got <= WORK_EPS {
        return abandon(ag);
    }
    note(ag, true);
    ag.load = Some(Load::new(p.good, got, false));
    ag.path = None;
    // A dam builder packs it on its next turn; a site's builder carries it there.
    ag.task = if is_dam(sim, then) { Task::Idle } else { Task::Carry { dest: then } };
}

/// Walk to food on the ground and eat until full or it is gone.
fn eat_on_ground(sim: &mut SimState, ag: &mut Agent, dt: f32, pile: u64) {
    let Some(cell) = sim.work.piles.get(pile).filter(|p| p.good == Good::Food).map(|p| p.cell) else {
        ag.task = Task::Idle;
        return;
    };
    match reach(sim, ag, dt, cell) {
        None => {
            ag.task = Task::Idle;
            return;
        }
        Some(false) => return,
        Some(true) => {}
    }
    note(ag, true);
    if let Some((w, stage, food)) = sim.colony.beaver(ag.id).map(|b| (b.stage.food_weight(), b.stage as usize, b.needs.food)) {
        let got = sim.work.piles.take(pile, ((1.0 - food) * w).max(0.0));
        if let Some(b) = sim.colony.beavers.iter_mut().find(|b| b.id == ag.id) {
            b.needs.food = (b.needs.food + got / w).min(1.0);
        }
        sim.colony.eaten[stage] += got;
    }
    ag.task = Task::Idle;
}

/// A hungry beaver with nothing to eat in store goes to the nearest food on the ground.
fn hungry_for_ground_food(sim: &SimState, ag: &mut Agent) -> Option<u64> {
    if ag.load.is_some() || matches!(ag.task, Task::Eat { .. }) || sim.time_days < ag.next_meal_probe {
        return None;
    }
    let hungry = sim.colony.beaver(ag.id).is_some_and(|b| b.needs.food < crate::tuning::colony::EAT_BELOW);
    if !hungry || sim.buildings.stores.amount(Good::Food) > WORK_EPS {
        return None;
    }
    ag.next_meal_probe = sim.time_days + crate::piles::MEAL_PROBE_DAYS;
    let food = sim.work.piles.items.iter().filter(|p| p.good == Good::Food).map(|p| (p.id, vec![p.cell]));
    nearest_by_route(sim, ag, &[], food)
}

fn dam_fetch(sim: &mut SimState, ag: &mut Agent, dt: f32, duty: &Duty, from: BuildingId, good: Good, dam: BuildingId) {
    let from_ok = if from == SUPPLIES { sim.work.supplies.is_some() } else { sim.buildings.building(from).is_some_and(|b| b.is_complete()) };
    if !matches!(duty, Duty::Build(b) if *b == dam) || !from_ok || !crate::dams::has_work_for(sim, dam, upkeep(sim, ag.id)) {
        return abandon(ag);
    }
    if !approach(sim, ag, dt, from) {
        return;
    }
    note(ag, true);
    let want = crate::dams::sticks_per_step().min(LOAD_SIZE).min(sim.work.stock_of(from, good));
    let (got, kind) = if want > WORK_EPS { sim.buildings.stores.take_kind_from(from, good, want) } else { (0.0, None) };
    log_load(sim, ag, "TAKE_FOR_DAM", from, good, want, got, &format!("dam={}", dam.0));
    if got <= WORK_EPS {
        return abandon(ag);
    }
    ag.load = Some(Load { species: kind, ..Load::new(good, got, true) });
    let taken = crate::dams::claimed(sim, dam, ag.id);
    ag.task = match crate::dams::next_cell(sim, dam, ag.cell, &taken, upkeep(sim, ag.id)) {
        Some(cell) => Task::DamWork { dam, cell, progress: 0.0 },
        None => Task::Carry { dest: NO_STORE },
    };
}

fn dam_work(sim: &mut SimState, ag: &mut Agent, dt: f32, duty: &Duty, dam: BuildingId, cell: Cell, progress: f32) {
    let Some(load) = ag.load.filter(|l| l.good == Good::Sticks) else {
        ag.path = None;
        ag.task = Task::Idle;
        return;
    };
    if !matches!(duty, Duty::Build(b) if *b == dam) {
        return abandon(ag);
    }
    if !crate::dams::cell_needs_work(sim, dam, cell, upkeep(sim, ag.id)) {
        let taken = crate::dams::claimed(sim, dam, ag.id);
        ag.path = None;
        ag.task = match crate::dams::next_cell(sim, dam, ag.cell, &taken, upkeep(sim, ag.id)) {
            Some(cell) => Task::DamWork { dam, cell, progress: 0.0 },
            None => Task::Carry { dest: NO_STORE },
        };
        return;
    }
    let here = cell.0.abs_diff(ag.cell.0) + cell.1.abs_diff(ag.cell.1) <= 1;
    if !here {
        if ag.path.is_none() {
            ag.path = sim.nav.find_path(ag.id, ag.cell, Goal::Beside(cell));
            if ag.path.is_none() {
                if crate::trace::enabled() { crate::trace::event(format!("PATH_FAIL {} beaver={} cell=({},{}) task={:?} load={:?}", crate::trace::sim_stamp(), ag.id.0, ag.cell.0, ag.cell.1, ag.task, ag.load.map(|l| l.good))); }
                note(ag, false);
                ag.task = Task::Carry { dest: NO_STORE };
                return;
            }
        }
        let moved = follow(&mut sim.nav, ag, dt);
        note(ag, moved);
        return;
    }
    ag.path = None;
    note(ag, true);
    let progress = progress + dt * LABOUR_PER_DAY * worker_effort(sim, ag, crate::colony::Job::Builder);
    if progress + WORK_EPS < crate::dams::pack_labour() {
        ag.task = Task::DamWork { dam, cell, progress };
        return;
    }
    let used = crate::dams::pack(sim, dam, cell, load.amount);
    log_load(sim, ag, "PACK_DAM", dam, load.good, load.amount, used, &format!("dam_cell=({},{})", cell.0, cell.1));
    let rest = load.amount - used;
    ag.load = (rest > WORK_EPS).then_some(Load { amount: rest, ..load });
    ag.task = Task::Idle;
}

/// Walk beside a canal cell or a gate and work there: one digging step, or one gate visit.
#[allow(clippy::too_many_arguments)]
fn waterworks_visit(sim: &mut SimState, ag: &mut Agent, dt: f32, duty: &Duty, site: BuildingId, cell: Cell, progress: f32, gate: bool) {
    if !matches!(duty, Duty::Build(b) if *b == site) {
        return abandon(ag);
    }
    let still = if gate {
        crate::canals::gate_owner(sim, cell) == Some(site) && (crate::canals::gate_needs_visit(sim, site) || crate::dams::gate_job(sim, site, &[]) == Some(cell))
    } else {
        crate::canals::cell_needs_work(sim, site, cell, upkeep(sim, ag.id))
    };
    if !still {
        ag.path = None;
        ag.task = Task::Idle;
        return;
    }
    let here = cell.0.abs_diff(ag.cell.0) + cell.1.abs_diff(ag.cell.1) <= 1;
    if !here {
        if ag.path.is_none() {
            ag.path = sim.nav.find_path(ag.id, ag.cell, Goal::Beside(cell));
            if ag.path.is_none() {
                if crate::trace::enabled() { crate::trace::event(format!("PATH_FAIL {} beaver={} cell=({},{}) task={:?} load={:?}", crate::trace::sim_stamp(), ag.id.0, ag.cell.0, ag.cell.1, ag.task, ag.load.map(|l| l.good))); }
                note(ag, false);
                ag.task = Task::Idle;
                return;
            }
        }
        let moved = follow(&mut sim.nav, ag, dt);
        note(ag, moved);
        return;
    }
    ag.path = None;
    note(ag, true);
    let progress = progress + dt * LABOUR_PER_DAY * worker_effort(sim, ag, crate::colony::Job::Builder);
    let need = if gate { crate::canals::GATE_WORK } else { crate::canals::DIG_WORK };
    if progress + WORK_EPS < need {
        ag.task = if gate { Task::Gate { site, cell, progress } } else { Task::Dig { site, cell, progress } };
        return;
    }
    let done = if gate {
        if is_canal(sim, site) { crate::canals::turn_gate(sim, site) } else { crate::dams::work_gate(sim, site, cell) }
    } else {
        crate::canals::dig(sim, site, cell)
    };
    note(ag, done.is_ok());
    ag.task = Task::Idle;
}

/// A newly paired beaver's turn: to its lodge, wait there for its mate, court, then back to
/// work. Returns false once there is no courtship (never was, or over).
fn court(sim: &mut SimState, ag: &mut Agent, dt: f32) -> bool {
    let Some(c) = sim.agents.courting.get(&ag.id).copied() else { return false };
    let now = sim.time_days;
    let partner_on = sim.agents.courting.get(&c.partner).is_some_and(|p| p.partner == ag.id);
    // A guard keeps its post: it leaves only to eat, so it does not go courting.
    let guard = |id: BeaverId| sim.work.slot_of(id).is_some_and(|(_, _, r)| r == Role::Guard);
    let over = c.met.is_some_and(|m| now >= m + COURT_DAYS) || (c.met.is_none() && now >= c.started + COURT_WAIT_DAYS) || !partner_on || guard(ag.id) || guard(c.partner);
    let home = home_of(sim, ag.id).map(|b| b.id);
    if over || home.is_none() {
        sim.agents.courting.remove(&ag.id);
        ag.path = None;
        ag.task = Task::Idle;
        return false;
    }
    if c.met.is_some() {
        ag.path = None;
        note(ag, true);
        return true;
    }
    // Out of the lodge and beside it; once both are, they meet.
    let there = !ag.inside && approach(sim, ag, dt, home.unwrap());
    if there {
        ag.task = Task::Idle;
        let partner_there = sim.agents.by_id.get(&c.partner).is_some_and(|p| {
            !p.inside && p.cell.0.abs_diff(ag.cell.0).max(p.cell.1.abs_diff(ag.cell.1)) <= 3
        });
        if partner_there {
            for who in [ag.id, c.partner] {
                if let Some(x) = sim.agents.courting.get_mut(&who) {
                    x.met = Some(now);
                }
            }
        }
    } else if ag.inside {
        // Out of the lodge first, onto the ground or water just round it.
        if ag.path.is_none() {
            let cells = place_cells(sim, home.unwrap());
            let ring: Vec<Cell> = cells
                .iter()
                .flat_map(|&(x, y)| [(x + 1, y), (x.wrapping_sub(1), y), (x, y + 1), (x, y.wrapping_sub(1))])
                .filter(|c| !cells.contains(c) && matches!(sim.nav.surface_at(*c), Some(Surface::Land | Surface::Water | Surface::Canal | Surface::LodgeEntrance)))
                .collect();
            ag.path = sim.nav.find_path_onto_any(ag.id, ag.cell, &ring);
        }
        let moved = follow(&mut sim.nav, ag, dt);
        note(ag, moved);
    }
    true
}

/// A kit's turn while a parent works out near home: it tags along a step behind.
fn follow_parent(sim: &mut SimState, ag: &mut Agent, dt: f32) -> bool {
    let Some(b) = sim.colony.beaver(ag.id) else { return false };
    // Only a kit with nothing to do: one old enough to have a job keeps to it.
    if b.stage != crate::colony::Stage::Kit || b.resting || sim.work.slot_of(ag.id).is_some() || ag.assignment.is_some() {
        return false;
    }
    let home = home_of(sim, ag.id).map(|h| h.pos);
    let near_home = |c: Cell| home.is_some_and(|h| h.0.abs_diff(c.0).max(h.1.abs_diff(c.1)) <= KIT_FOLLOW_RANGE);
    let parent = [b.mother, b.father].into_iter().flatten().filter_map(|p| sim.agents.by_id.get(&p)).find(|p| {
        !p.inside && near_home(p.cell) && !sim.agents.fleeing.contains(&p.id)
    });
    let Some(target) = parent.map(|p| p.cell) else { return false };
    ag.task = Task::Idle;
    let d = target.0.abs_diff(ag.cell.0).max(target.1.abs_diff(ag.cell.1));
    if d <= 1 {
        ag.path = None;
        note(ag, true);
        return true;
    }
    // Head for beside the parent; set out again once it has moved on.
    let stale = ag.path.as_ref().and_then(|p| p.steps.last()).is_none_or(|s| s.0 .0.abs_diff(target.0).max(s.0 .1.abs_diff(target.1)) > 2);
    if stale {
        ag.path = sim.nav.find_path_beside_any(ag.id, ag.cell, &[target]);
    }
    let moved = follow(&mut sim.nav, ag, dt);
    note(ag, moved);
    true
}

fn approach(sim: &mut SimState, ag: &mut Agent, dt: f32, target: BuildingId) -> bool {
    let cells = place_cells(sim, target);
    if cells.is_empty() {
        ag.path = None;
        note(ag, false);
        return false;
    }
    if cells.iter().any(|f| f.0.abs_diff(ag.cell.0) + f.1.abs_diff(ag.cell.1) <= 1) {
        ag.path = None;
        return true;
    }
    if ag.path.is_none() {
        ag.path = sim.nav.find_path_beside_any(ag.id, ag.cell, &cells);
        if ag.path.is_none() {
            if crate::trace::enabled() { crate::trace::event(format!("PATH_FAIL {} beaver={} cell=({},{}) task={:?} load={:?}", crate::trace::sim_stamp(), ag.id.0, ag.cell.0, ag.cell.1, ag.task, ag.load.map(|l| l.good))); }
            note(ag, false);
            abandon(ag);
            return false;
        }
    }
    let moved = follow(&mut sim.nav, ag, dt);
    note(ag, moved);
    false
}

pub(crate) fn worker_effort(sim: &SimState, ag: &Agent, job: crate::colony::Job) -> f32 {
    sim.colony.beaver(ag.id).map_or(0.0, |b| work_factors(b, job).total())
}

/// What a beaver's pace at a job is made of: each factor multiplies the work done.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WorkFactors {
    /// The job's skill, 0 to 1, and what it multiplies by.
    pub skill: f32,
    pub skill_x: f32,
    /// Age (kits and yearlings work at a share), mood, health and rest.
    pub age_x: f32,
    pub mood_x: f32,
    pub health_x: f32,
    pub rest_x: f32,
}

impl WorkFactors {
    pub fn total(&self) -> f32 {
        self.age_x * self.mood_x * self.health_x * self.rest_x * self.skill_x
    }
}

pub fn work_factors(b: &crate::colony::Beaver, job: crate::colony::Job) -> WorkFactors {
    let skill = b.skills[job.index()];
    WorkFactors {
        skill,
        skill_x: WORK_SKILL_BASE + WORK_SKILL_SCALE * skill,
        age_x: b.work_weight(),
        mood_x: crate::society::work_speed(b.mood),
        health_x: WORK_HEALTH_BASE + WORK_HEALTH_SCALE * b.health,
        rest_x: WORK_REST_BASE + WORK_REST_SCALE * b.needs.rest,
    }
}

/// Beavers on their way to build a site or building it, and the labour they add a day
/// together once there.
pub fn site_crew(sim: &SimState, site: crate::buildings::BuildingId) -> (usize, f32) {
    sim.agents.by_id.values().filter(|a| matches!(a.task, Task::Build { site: s } if s == site)).fold((0, 0.0), |(n, rate), a| {
        (n + 1, rate + LABOUR_PER_DAY * worker_effort(sim, a, crate::colony::Job::Builder))
    })
}

fn go_to(sim: &mut SimState, ag: &mut Agent, dt: f32, to: Cell) {
    let done = |ag: &mut Agent, _: &mut SimState| {
        ag.assignment = None;
        ag.path = None;
        ag.task = Task::Idle;
    };
    if ag.cell == to {
        ag.move_state = MoveState::Arrived;
        return done(ag, sim);
    }
    if ag.inside || ag.path.is_none() {
        ag.path = sim.nav.find_path(ag.id, ag.cell, Goal::Cell(to));
        if ag.path.is_none() {
            if crate::trace::enabled() { crate::trace::event(format!("PATH_FAIL {} beaver={} cell=({},{}) task={:?} load={:?}", crate::trace::sim_stamp(), ag.id.0, ag.cell.0, ag.cell.1, ag.task, ag.load.map(|l| l.good))); }
            ag.move_state = MoveState::Blocked;
            return done(ag, sim);
        }
    }
    ag.task = Task::Idle;
    let moved = follow(&mut sim.nav, ag, dt);
    note(ag, moved);
}

fn abandon(ag: &mut Agent) {
    if crate::trace::enabled() {
        crate::trace::event(format!("ABANDON {} beaver={} cell=({},{}) task={:?} load={:?}", crate::trace::sim_stamp(), ag.id.0, ag.cell.0, ag.cell.1, ag.task, ag.load.map(|l| (l.good, l.amount))));
    }
    ag.path = None;
    ag.task = if ag.load.is_some() { Task::Carry { dest: NO_STORE } } else { Task::Idle };
}

fn fetch(sim: &mut SimState, ag: &mut Agent, dt: f32, duty: &Duty, from: BuildingId, good: Good, then: BuildingId, site_delivery: bool) {
    let site = site_delivery && sim.work.place(then).is_some_and(|p| is_site(sim, p))
        && (matches!(duty, Duty::Build(b) if *b==then) || matches!(duty, Duty::Haul));
    let still_wanted = match duty {
        Duty::Build(s) => site && *s == then,
        Duty::Haul => {
            site || (sim.work.hauls.iter().any(|o| (o.from, o.to, o.good) == (from, then, good))
                    || sim.work.fills.contains(&then)
                    || crate::stock::balancing(sim))
        }
        _ => false,
    };
    let from_ok = if from == SUPPLIES { sim.work.supplies.is_some() } else { sim.buildings.building(from).is_some_and(|b| b.is_complete()) };
    if !still_wanted || !from_ok {
        return abandon(ag);
    }
    if !approach(sim, ag, dt, from) {
        return;
    }
    note(ag, true);
    if site {
        let outstanding = sim.work.place(then).and_then(|p| p.site.as_ref()).map_or(0.0, |s| s.missing(good))
            - promised(sim, then, good);
        let want = load_size(good).min(outstanding).min(sim.work.stock_of(from, good)).min(sim.buildings.stores.amount(good));
        if want <= WORK_EPS {
            return abandon(ag);
        }
        let asked = want;
        let (want, kind) = sim.buildings.stores.take_kind_from(from, good, want);
        log_load(sim, ag, "TAKE_FOR_SITE", from, good, asked, want, &format!("site={}", then.0));
        ag.load = Some(Load { species: kind, ..Load::new(good, want, true) });
        ag.task = Task::Carry { dest: then };
    } else {
        let (got, kind) = sim.buildings.stores.take_kind_from(from, good, load_size(good));
        log_load(sim, ag, "TAKE_FOR_HAUL", from, good, load_size(good), got, &format!("to={}", then.0));
        if got <= WORK_EPS {
            return abandon(ag);
        }
        ag.load = Some(Load { species: kind, ..Load::new(good, got, true) });
        ag.task = Task::Haul { to: then };
    }
}

/// For the session journal: one load moving between a beaver and a building or the ground.
#[allow(clippy::too_many_arguments)]
fn log_load(sim: &SimState, ag: &Agent, what: &str, at: BuildingId, good: Good, offered: f32, moved: f32, note: &str) {
    if !crate::trace::enabled() {
        return;
    }
    let kind = sim.buildings.building(at).map(|b| format!("{:?}", b.kind)).unwrap_or_else(|| if at == SUPPLIES { "Supplies".into() } else { "none".into() });
    crate::trace::event(format!(
        "LOAD {what} day={:.4} step={} beaver={} good={good:?} offered={offered:.3} moved={moved:.3} building={} kind={kind} cell=({},{}) task={:?} stock_after={:.2} {note}",
        sim.time_days, sim.steps, ag.id.0, at.0, ag.cell.0, ag.cell.1, ag.task, sim.buildings.stores.amount(good)
    ));
}

fn deliver(sim: &mut SimState, ag: &mut Agent, dt: f32, site: BuildingId) {
    let Some(load) = ag.load else {
        ag.task = Task::Idle;
        return;
    };
    if !approach(sim, ag, dt, site) {
        return;
    }
    let taken = sim.work.deliver(site, load.good, load.amount).expect("dest checked to be a site");
    log_load(sim, ag, "DELIVER_SITE", site, load.good, load.amount, taken, "");
    if taken > 0.0 {
        let source = producer(sim, ag);
        sim.buildings.stores.book_site_delivery(load.good, taken, (!load.booked).then_some(source));
    }
    note(ag, true);
    let rest = load.amount - taken;
    if rest > WORK_EPS {
        ag.load = Some(Load { amount: rest, ..load });
        ag.task = Task::Carry { dest: NO_STORE };
    } else {
        ag.load = None;
        ag.task = Task::Idle;
    }
}

fn build(sim: &mut SimState, ag: &mut Agent, dt: f32, duty: &Duty, site: BuildingId) {
    let open = sim.buildings.building(site).is_some_and(|b| !b.is_complete() || sim.maintenance.jobs.contains_key(&site))
        && sim.work.place(site).and_then(|p| p.site.as_ref()).is_some_and(|s| s.labour_open());
    if !open || !matches!(duty, Duty::Build(s) if *s == site) {
        return abandon(ag);
    }
    if approach(sim, ag, dt, site) {
        sim.work.add_labour(site, dt * LABOUR_PER_DAY * worker_effort(sim, ag, crate::colony::Job::Builder)).expect("duty checked the site");
        note(ag, true);
    }
}

/// Walk to a worn building and mend it, drawing the materials from the stores as the work goes.
fn mend(sim: &mut SimState, ag: &mut Agent, dt: f32, duty: &Duty, building: BuildingId) {
    let wanted = matches!(duty, Duty::Mend(b) if *b == building) && sim.buildings.building(building).is_some_and(|b| crate::maintenance::needs_mending(sim, b));
    if !wanted {
        return abandon(ag);
    }
    if approach(sim, ag, dt, building) {
        let labour = dt * LABOUR_PER_DAY * worker_effort(sim, ag, crate::colony::Job::Builder);
        let mended = crate::maintenance::mend(sim, building, labour);
        note(ag, mended);
        if !mended || !sim.buildings.building(building).is_some_and(|b| crate::maintenance::needs_mending(sim, b)) {
            ag.task = Task::Idle;
        }
    }
}

fn haul(sim: &mut SimState, ag: &mut Agent, dt: f32, to: BuildingId) {
    let Some(load) = ag.load else {
        ag.task = Task::Idle;
        return;
    };
    let to = if sim.buildings.building(to).is_some_and(|b| b.is_complete() && !sim.buildings.unusable(b, &sim.water.depth)) {
        to
    } else {
        let Some(other) = closest_store(sim, ag.cell, load.good) else {
            note(ag, false);
            return;
        };
        ag.path = None;
        ag.task = Task::Haul { to: other };
        other
    };
    if approach(sim, ag, dt, to) {
        let put=sim.buildings.stores.put_kind_at(to,load.good,load.kind(),load.amount).unwrap_or(0.0);
        log_load(sim, ag, "HAUL_STORE", to, load.good, load.amount, put, "");
        if put >= load.amount-WORK_EPS { ag.load=None; ag.task=Task::Idle; }
        else {ag.load=Some(Load{amount:load.amount-put,..load});}
        note(ag, put>0.0);
    }
}

fn clear_finished_footprint(sim: &mut SimState, id: BuildingId) -> bool {
    let Some(b) = sim.buildings.building(id) else { return false };
    if b.kind == BuildingKind::Dam {
        return true;
    }
    let cells: Vec<Cell> = b.cells().collect();
    let mut exits = Vec::new();
    for &(x, y) in &cells {
        for c in [x.checked_sub(1).map(|x| (x, y)), y.checked_sub(1).map(|y| (x, y)), Some((x + 1, y)), Some((x, y + 1))].into_iter().flatten() {
            if !cells.contains(&c) && !exits.contains(&c) { exits.push(c); }
        }
    }
    let occupants: Vec<_> = sim.agents.by_id.values().filter(|a| cells.contains(&a.cell)).map(|a| a.id).collect();
    if occupants.is_empty() { return true; }
    // Another site may have finished earlier in this same update.
    let mut nav = std::mem::replace(&mut sim.nav, NavGrid::new(1, 1));
    nav.refresh(sim);
    sim.nav = nav;
    for who in occupants {
        let ag = sim.agents.by_id.get_mut(&who).expect("site occupant");
        // Walk out while the unfinished site is still traversable. If the
        // perimeter is occupied, leave completion pending until there is room.
        let Some(path) = sim.nav.find_path_onto_any(who, ag.cell, &exits) else { return false };
        for (cell, gait) in path.steps {
            if sim.nav.advance(who, ag.cell, cell).is_err() { return false; }
            ag.cell = cell;
            ag.gait = gait;
            ag.trail.push((cell, gait));
        }
        ag.path = None;
        ag.step_frac = 0.0;
        ag.inside = sim.nav.surface_at(ag.cell) == Some(Surface::LodgeInside);
        ag.stuck_ticks = 0;
    }
    true
}

fn finish_sites(sim: &mut SimState) {
    let done: Vec<BuildingId> =
        sim.work.places.iter().filter(|p| p.site.as_ref().is_some_and(|s| s.is_done())).map(|p| p.building).collect();
    for id in done {
        if crate::maintenance::finish(sim,id) {continue;}
        if sim.buildings.building(id).is_none_or(|b| b.status != crate::buildings::Status::Site) || !clear_finished_footprint(sim, id) {
            continue;
        }
        let Some(b) = sim.buildings.buildings.iter_mut().find(|b| b.id == id && b.status == crate::buildings::Status::Site)
        else {
            continue;
        };
        b.status = crate::buildings::Status::Complete;
        let area = crate::catalogue::work_area(b);
        let def = b.kind.spec();
        sim.work.finish_site(id, &def.slots, area).expect("workplace exists");
    }
}


pub(crate) fn is_site(sim: &SimState, p: &crate::work::Workplace) -> bool {
    p.is_open_site() && sim.buildings.building(p.building).is_some_and(|b| !b.is_complete() || sim.maintenance.jobs.contains_key(&b.id))
}

pub(crate) fn site_stalled(sim: &SimState, p: &crate::work::Workplace) -> bool {
    let Some(s) = p.site.as_ref() else { return false };
    !s.labour_open()
        && s.needed.iter().all(|n| {
            s.missing(n.0) <= WORK_EPS
                || (promised(sim, p.building, n.0) <= WORK_EPS && crate::stock::holders(sim, n.0).iter().all(|&h| sim.work.stock_of(h, n.0) <= WORK_EPS))
        })
}

/// Goods beavers have taken out of stores and are still carrying.
pub fn in_hands(sim: &SimState, good: Good) -> f32 {
    sim.agents.by_id.values().filter_map(|a| a.load).filter(|l| l.booked && l.good == good).map(|l| l.amount).sum()
}

/// Where the colony keeps its goods: its first shelter, store or the supply pile.
pub fn colony_home(sim: &SimState) -> Option<Cell> {
    let built = |k: BuildingKind| sim.buildings.buildings.iter().find(|b| b.kind == k && b.is_complete()).map(|b| (b.pos.0 + 1, b.pos.1 + 1));
    built(BuildingKind::Warehouse).or_else(|| built(BuildingKind::Shelter)).or(sim.work.supplies)
}

/// The ground round home that beavers are sent out over to gather.
pub(crate) fn home_area(sim: &SimState) -> Option<(Cell, Area)> {
    let home = colony_home(sim)?;
    let r = GATHER_RADIUS / 2;
    Some((home, Area { min: (home.0.saturating_sub(r), home.1.saturating_sub(r)), max: (home.0 + r, home.1 + r) }))
}

/// How badly a waterwork or refit needs its maintainers, 0 (sound) to 1 (falling down): a dam
/// by its weakest cell, a canal silting up. Refits that only strengthen a building come last.
/// Worn buildings are mended in place and weighed in `send_maintainers`.
pub fn repair_need(sim: &SimState, b: BuildingId) -> f32 {
    if let Some(job) = sim.maintenance.jobs.get(&b) {
        // A refit is a site waiting for its materials and labour.
        if job.kind != crate::maintenance::Refit::Repair {
            return 0.05;
        }
    }
    if let Some(d) = crate::dams::dam(sim, b) {
        if !crate::dams::has_work_for(sim, b, true) {
            return 0.0;
        }
        let weakest = d.cells.iter().filter(|c| crate::dams::built(sim, c.cell) > 0.0).map(|c| crate::dams::integrity(sim, c.cell)).fold(1.0f32, f32::min);
        return 1.0 - weakest;
    }
    if crate::canals::has_work_for(sim, b, true) {
        return 0.4;
    }
    0.0
}

/// How much worse off another place must be before a busy maintainer leaves its own for it.
const REPAIR_SWITCH: f32 = 0.2;

fn is_maintainer(sim: &SimState, who: BeaverId) -> bool {
    sim.work.slot_of(who).is_none() && sim.agents.get(who).is_some_and(|a| a.assignment == Some(Assignment::Maintain))
}

/// Maintainers need no seats and no orders, as gatherers need none. Each goes to the place it can
/// reach in the worst repair: a worn building to mend (one marked for repair first, if the
/// stores hold what mending it takes), a dam's worn cells, a canal's silt or a refit. Places in
/// equal need are shared out, and a maintainer stays put unless another is much worse off.
fn send_maintainers(sim: &mut SimState) {
    if sim.steps % RETARGET_STEPS != 0 {
        return;
    }
    let maintainers: Vec<(BeaverId, Cell)> = sim
        .colony
        .beavers
        .iter()
        .filter(|b| b.can_work() && is_maintainer(sim, b.id))
        .filter_map(|b| sim.agents.get(b.id).map(|a| (b.id, a.cell)))
        .collect();
    sim.agents.maint_targets.retain(|who, _| maintainers.iter().any(|m| m.0 == *who));
    if maintainers.is_empty() {
        return;
    }
    let marked = |b: BuildingId| sim.maintenance.jobs.get(&b).is_some_and(|j| j.kind == crate::maintenance::Refit::Repair);
    let mut places: Vec<(BuildingId, f32, Cell)> = sim
        .work
        .places
        .iter()
        .filter(|p| !p.paused)
        .filter_map(|p| {
            let need = repair_need(sim, p.building);
            let pos = sim.buildings.building(p.building)?.pos;
            (need > 0.0).then_some((p.building, need, pos))
        })
        .collect();
    places.extend(
        sim.buildings
            .buildings
            .iter()
            .filter(|b| crate::maintenance::needs_mending(sim, b))
            .filter(|b| crate::maintenance::mend_rates(b).0.iter().all(|&(g, _)| sim.buildings.stores.amount(g) > WORK_EPS))
            .map(|b| (b.id, 1.0 - b.condition + if marked(b.id) { 1.0 } else { 0.0 }, b.pos)),
    );
    for (who, at) in maintainers {
        let need_of = |b: BuildingId| places.iter().find(|p| p.0 == b).map_or(0.0, |p| p.1);
        let current = sim.agents.maint_targets.get(&who).copied().filter(|&b| need_of(b) > 0.0 && reaches_place(sim, at, b));
        let crowd = |b: BuildingId| sim.agents.maint_targets.iter().filter(|(w, t)| **w != who && **t == b).count();
        let best = places
            .iter()
            .filter(|p| reaches_place(sim, at, p.0))
            .filter(|p| current.is_none_or(|c| p.1 > need_of(c) + REPAIR_SWITCH))
            .min_by_key(|p| (std::cmp::Reverse((p.1 * 20.0) as i32), crowd(p.0), p.2.0.abs_diff(at.0).pow(2) + p.2.1.abs_diff(at.1).pow(2), p.0))
            .map(|p| p.0);
        match best.or(current) {
            Some(b) => {
                sim.agents.maint_targets.insert(who, b);
            }
            None => {
                sim.agents.maint_targets.remove(&who);
            }
        }
    }
}

/// Whether builders have a place here at all: a site, a refit, a dam or a canal.
pub(crate) fn takes_builders(sim: &SimState, p: &crate::work::Workplace) -> bool {
    is_site(sim, p) || sim.maintenance.jobs.contains_key(&p.building) || sim.buildings.building(p.building).is_some_and(|b| b.kind.is_waterworks())
}

fn tidy(sim: &mut SimState) {
    // Maintainers have no seats; anyone found in one (an older save) keeps the role without it.
    let seated: Vec<BeaverId> = sim.work.places.iter().flat_map(|p| p.slots.iter().filter(|s| s.role == Role::Maintain).filter_map(|s| s.worker)).collect();
    for p in &mut sim.work.places {
        p.slots.retain(|s| s.role != Role::Maintain);
    }
    for who in seated {
        sim.agents.assign(who, Some(Assignment::Maintain));
    }
    // A forester's planting seat (an older save) becomes its one forester seat, and it
    // takes the wider range foresters now have.
    let wider: Vec<(BuildingId, Option<Area>)> = sim
        .work
        .places
        .iter()
        .filter(|p| p.slots.iter().any(|s| s.role == Role::Plant))
        .map(|p| (p.building, sim.buildings.building(p.building).and_then(crate::catalogue::work_area)))
        .collect();
    for (b, area) in wider {
        let Ok(p) = sim.work.place_mut(b) else { continue };
        let worker = p.slots.iter().filter(|s| s.role == Role::Plant).find_map(|s| s.worker);
        p.slots.retain(|s| s.role != Role::Plant);
        p.slots.push(crate::work::Slot { role: Role::Forest, worker, off: false });
        if area.is_some() {
            p.area = area;
        }
    }
    // Seats kept for builders at a finished building go once their builder has moved on.
    let finished: Vec<BuildingId> = sim.work.places.iter().filter(|p| !takes_builders(sim, p)).map(|p| p.building).collect();
    for p in sim.work.places.iter_mut().filter(|p| finished.contains(&p.building)) {
        p.slots.retain(|s| s.worker.is_some() || s.role != Role::Build);
    }
    let alive: std::collections::BTreeSet<BeaverId> = sim.colony.beavers.iter().map(|b| b.id).collect();
    let dry: Vec<BuildingId> = sim.buildings.buildings.iter().filter(|b| sim.buildings.unusable(b, &sim.water.depth)).map(|b| b.id).collect();
    for p in &mut sim.work.places {
        let refitting=sim.maintenance.jobs.contains_key(&p.building);
        let ruined=sim.buildings.building(p.building).is_some_and(|b|b.is_complete() && !b.operational());
        let closed = p.paused || (!refitting && (dry.contains(&p.building) || ruined));
        let waiting_here = finished.contains(&p.building);
        for s in &mut p.slots {
            // Builders waiting at a finished building stay builders whatever becomes of it.
            let waiting = waiting_here && s.role == Role::Build;
            if s.worker.is_some_and(|w| (closed && !waiting) || !alive.contains(&w)) {
                s.worker = None;
            }
        }
    }
    let standing: std::collections::BTreeSet<BuildingId> = sim.buildings.buildings.iter().map(|b| b.id).collect();
    sim.work.balanced.retain(|b| standing.contains(b));
}

fn collect(sim: &mut SimState, ag: &mut Agent, dt: f32, duty: Duty, target: ResourceId) {
    let job = match &duty {
        Duty::Collect(j) => Some(j),
        _ => None,
    };
    let res = sim.resources.get(target).filter(|r| job.is_some_and(|j| j.permits(r))).cloned();
    let Some(res) = res else {
        ag.path = None;
        // Run out with a part load: go on to the next source rather than home with a sliver.
        let part = ag.load.filter(|l| l.amount < load_size(l.good) - EPS);
        let next = part.and_then(|_| job?.pick(sim, ag.cell)).filter(|&t| t != target);
        ag.task = match (next, ag.load) {
            (Some(t), _) => Task::Collect { target: t, progress: part.map_or(0.0, |l| l.amount / load_size(l.good)) },
            (None, Some(_)) => Task::Carry { dest: NO_STORE },
            (None, None) => Task::Idle,
        };
        return;
    };
    let near = res.cell.0.abs_diff(ag.cell.0) + res.cell.1.abs_diff(ag.cell.1) <= 1;
    if !near {
        if ag.path.is_none() {
            ag.path = sim.nav.find_path(ag.id, ag.cell, Goal::Beside(res.cell));
            if ag.path.is_none() {
                if crate::trace::enabled() { crate::trace::event(format!("PATH_FAIL {} beaver={} cell=({},{}) task={:?} load={:?}", crate::trace::sim_stamp(), ag.id.0, ag.cell.0, ag.cell.1, ag.task, ag.load.map(|l| l.good))); }
                sim.agents.unreachable.insert(target, sim.time_days + UNREACHABLE_RETRY_DAYS);
                if crate::trace::enabled() { crate::trace::event(format!("UNREACHABLE {} beaver={} resource={} cell=({},{}) retry_after_days={} task={:?}", crate::trace::sim_stamp(), ag.id.0, target.0, ag.cell.0, ag.cell.1, UNREACHABLE_RETRY_DAYS, ag.task)); }
                ag.task = if ag.load.is_some() { Task::Carry { dest: NO_STORE } } else { Task::Idle };
                note(ag, false);
                return;
            }
        }
        let moved = follow(&mut sim.nav, ag, dt);
        note(ag, moved);
        return;
    }
    ag.path = None;
    note(ag, true);
    ag.source = Some(target);
    if res.kind == ResourceKind::Tree {
        if sim.resources.fell(target, dt * worker_effort(sim, ag, crate::colony::Job::Logger) * FELL_PER_DAY).expect("kind checked above") {
            sim.nav.set(res.cell, Surface::Land);
            crate::forestry::felled(sim, ag.id, res.cell, res.species.unwrap_or(crate::trees::Tree::Aspen));
        }
        return;
    }
    let bark = res.kind == ResourceKind::Felled && job.is_some_and(|j| j.forages());
    let good = match res.kind {
        _ if bark => Good::Food,
        ResourceKind::Felled if res.amount > 0.0 => Good::Sticks,
        ResourceKind::Felled => Good::Sticks,
        k => k.yields()[0],
    };
    if ag.load.is_none() && !worth_fetching(sim, ag.cell, good) {
        sim.agents.unreachable.insert(target, sim.time_days + UNREACHABLE_RETRY_DAYS);
        if crate::trace::enabled() { crate::trace::event(format!("UNREACHABLE {} beaver={} resource={} cell=({},{}) retry_after_days={} task={:?}", crate::trace::sim_stamp(), ag.id.0, target.0, ag.cell.0, ag.cell.1, UNREACHABLE_RETRY_DAYS, ag.task)); }
        ag.task = Task::Idle;
        return;
    }
    if ag.load.is_some_and(|l| l.good != good) {
        ag.task = Task::Carry { dest: NO_STORE };
        return;
    }
    let have = ag.load.map_or(0.0, |l| l.amount);
    let left = if bark { res.bark } else if res.kind == ResourceKind::Felled && res.amount <= 0.0 { res.branches } else { res.amount };
    let rate = if bark || res.kind.is_food_plant() { FOOD_GATHER_PER_DAY } else if res.kind == ResourceKind::Pebbles { PEBBLE_GATHER_PER_DAY } else { GATHER_PER_DAY };
    let job = if bark || res.kind.is_food_plant() { crate::colony::Job::Forager }
        else if res.kind == ResourceKind::Felled { crate::colony::Job::Logger } else { crate::colony::Job::Digger };
    let full = if res.kind == ResourceKind::Pebbles { load_size(Good::Pebbles) } else { LOAD_SIZE };
    let mut want = (dt * worker_effort(sim, ag, job) * rate).min(full - have);
    if left - want < EPS {
        want = left;
    }
    let taken = if bark { sim.resources.strip(target, want).ok().filter(|t| t.1 > 0.0) } else { sim.resources.take(target, want) };
    if let Some((g, got)) = taken {
        if let Some((b, _, _)) = sim.work.slot_of(ag.id) {
            sim.output.record(b, g, got);
        }
        let amount = have + got;
        // Foragers pick up seedlings of the trees growing by where they forage.
        let found = if g == Good::Food { ag.load.and_then(|l| l.species).or_else(|| crate::forestry::seedling_species(sim, res.cell)) } else { None };
        ag.load = Some(Load { species: found, ..Load::new(g, amount, false) });
        if amount >= full - EPS {
            log_load(sim, ag, "GATHERED", NO_STORE, g, full, amount, &format!("from={:?}@({},{}) left={:.2}", res.kind, res.cell.0, res.cell.1, sim.resources.get(target).map_or(0.0, |r| r.amount)));
        }
        ag.task = if amount >= full - EPS {
            Task::Carry { dest: NO_STORE }
        } else {
            Task::Collect { target, progress: amount / full }
        };
    } else if ag.load.is_some() {
        ag.task = Task::Carry { dest: NO_STORE };
    }
}

/// Whether a load of `good` fetched near `from` has somewhere to go: room in the stores, a
/// building site short of it, or no store at all (then it is left on the ground for builders).
fn worth_fetching(sim: &SimState, from: Cell, good: Good) -> bool {
    sim.buildings.stores.space(good) > EPS || closest_store(sim, from, good).is_none() || site_short_of(sim, from, good).is_some()
}

fn site_short_of(sim: &SimState, from: Cell, good: Good) -> Option<BuildingId> {
    let d = |b: &Building| b.cells().map(|c| c.0.abs_diff(from.0).pow(2) + c.1.abs_diff(from.1).pow(2)).min().unwrap_or(0);
    sim.work
        .places
        .iter()
        .filter(|p| is_site(sim,p))
        .filter_map(|p| Some((sim.buildings.building(p.building)?, p.site.as_ref()?)))
        .filter(|(b, s)| s.missing(good) - promised(sim, b.id, good) > WORK_EPS)
        .min_by_key(|(b, _)| (d(b), b.id))
        .map(|(b, _)| b.id)
}

fn carry(sim: &mut SimState, ag: &mut Agent, dt: f32, dest: BuildingId) {
    let Some(load) = ag.load else {
        ag.task = Task::Idle;
        return;
    };
    if let Some(site) = site_short_of(sim, ag.cell, load.good) {
        ag.path = None;
        ag.task = Task::Carry { dest: site };
        return;
    }
    // With no store to take it, no room left, or no way to a store, the load is left on the
    // ground here, for builders and the hungry, and the beaver goes on with its work.
    if closest_store(sim, ag.cell, load.good).is_none() {
        drop_load(sim, ag, "no store");
        return;
    }
    if sim.buildings.stores.space(load.good) <= EPS {
        drop_load(sim, ag, "stores full");
        return;
    }
    let dest = if ag.path.is_some() || adjacent_to_store(sim, ag.cell, dest, load.good) {
        dest
    } else {
        let Some((id, path)) = nearest_store(sim, ag.id, ag.cell, load.good) else {
            drop_load(sim, ag, "no way to a store");
            return;
        };
        ag.task = Task::Carry { dest: id };
        ag.path = Some(path);
        id
    };
    let Some(store) = sim.buildings.building(dest).filter(|b| b.is_complete() && accepts(b.kind, load.good)) else {
        ag.path = None;
        ag.task = Task::Carry { dest: NO_STORE };
        note(ag, false);
        return;
    };
    if adjacent(ag.cell, store) {
        let who = producer(sim, ag);
        let booked = stow(sim, dest, load, who);
        log_load(sim, ag, "DELIVER_STORE", dest, load.good, load.amount, *booked.as_ref().unwrap_or(&0.0), &format!("booked={} result={:?}", load.booked, booked.as_ref().map(|_| "ok").map_err(|e| format!("{e:?}"))));
        match booked {
            Ok(taken) if taken >= load.amount => {
                ag.load = None;
                ag.task = Task::Idle;
                note(ag, true);
            }
            Ok(taken) => {
                // The store had room for only part of it: the rest goes on the ground.
                ag.load = Some(Load { amount: load.amount - taken, ..load });
                drop_load(sim, ag, "store filled");
            }
            Err(_) => drop_load(sim, ag, "store refused"),
        }
        return;
    }
    let moved = follow(&mut sim.nav, ag, dt);
    note(ag, moved);
}

/// Leave the carried load on the ground where the beaver stands and free it for other work.
fn drop_load(sim: &mut SimState, ag: &mut Agent, why: &str) {
    if let Some(load) = ag.load.take() {
        let now = sim.time_days;
        sim.work.piles.drop_at(ag.cell, load.good, load.amount, now);
        log_load(sim, ag, "DROP_GROUND", NO_STORE, load.good, load.amount, load.amount, why);
    }
    ag.path = None;
    ag.task = Task::Idle;
    note(ag, true);
}

/// Put a carried load in a store: fresh loads are new production, taken-out stock goes back.
fn stow(sim: &mut SimState, owner: BuildingId, load: Load, who: crate::buildings::Producer) -> Result<f32, crate::buildings::StoresError> {
    let stores = &mut sim.buildings.stores;
    if load.booked {
        stores.put_kind_at(owner, load.good, load.kind(), load.amount)
    } else {
        stores.produce_kind_at(owner, load.good, load.kind(), load.amount, who)
    }
}

fn producer(sim: &SimState, ag: &Agent) -> crate::buildings::Producer {
    use crate::buildings::Producer;
    match (sim.work.slot_of(ag.id), ag.assignment) {
        (Some((b, _, _)), _) => Producer::Building(b),
        (None, Some(_)) => Producer::Order,
        (None, None) => Producer::Other,
    }
}

/// Walk beside a cell. Some(true) once there, Some(false) on the way, None if no way there.
fn reach(sim: &mut SimState, ag: &mut Agent, dt: f32, cell: Cell) -> Option<bool> {
    if cell.0.abs_diff(ag.cell.0) + cell.1.abs_diff(ag.cell.1) <= 1 {
        ag.path = None;
        return Some(true);
    }
    if ag.path.is_none() {
        ag.path = sim.nav.find_path(ag.id, ag.cell, Goal::Beside(cell));
        if ag.path.is_none() {
            if crate::trace::enabled() { crate::trace::event(format!("PATH_FAIL {} beaver={} cell=({},{}) task={:?} load={:?}", crate::trace::sim_stamp(), ag.id.0, ag.cell.0, ag.cell.1, ag.task, ag.load.map(|l| l.good))); }
            note(ag, false);
            return None;
        }
    }
    let moved = follow(&mut sim.nav, ag, dt);
    note(ag, moved);
    Some(false)
}

/// The finished forester nearest a cell that holds at least one sapling.
fn sapling_store(sim: &SimState, from: Cell) -> Option<BuildingId> {
    sim.buildings.buildings.iter()
        .filter(|b| b.is_complete() && accepts(b.kind, Good::Saplings) && !sim.buildings.unusable(b, &sim.water.depth))
        .filter(|b| sim.work.stock_of(b.id, Good::Saplings) >= 1.0 - WORK_EPS)
        .min_by_key(|b| (crate::stock::dist(from, b), b.id))
        .map(|b| b.id)
}

/// A forester plants in its own area; `store` is the food store the sapling comes from.
fn plant_task(sim: &SimState, area: Area, ag: &Agent) -> Option<Task> {
    let carrying = ag.load.is_some_and(|l| l.good == Good::Saplings);
    let store = match sapling_store(sim, ag.cell) {
        Some(b) => b,
        None if carrying => NO_STORE,
        None => return None,
    };
    let cell = crate::forestry::plant_cell(sim, area, ag.cell, ag.id)?;
    Some(Task::Plant { store, cell, progress: 0.0 })
}

/// A grown tree in a forester's area to collect seedlings from, while a forester has room for them.
fn seed_task(sim: &SimState, area: Area, ag: &Agent) -> Option<Task> {
    if ag.load.is_some() || sim.buildings.stores.space(Good::Saplings) < SEEDLINGS_PER_TREE {
        return None;
    }
    let now = sim.time_days;
    sim.resources
        .items()
        .iter()
        .filter(|r| area.reaches(r.cell) && seeds(r) && within_shore(sim, crate::jobs::RoleKind::Forester, r.cell))
        .filter(|r| sim.agents.unreachable.get(&r.id).is_none_or(|&until| until <= now))
        .filter(|r| !sim.agents.by_id.values().any(|a| a.id != ag.id && matches!(a.task, Task::Seed { tree, .. } if tree == r.id)))
        .min_by(|a, b| cell_dist(ag.cell, a.cell).total_cmp(&cell_dist(ag.cell, b.cell)).then(a.id.cmp(&b.id)))
        .filter(|r| sim.nav.may_reach_beside(ag.cell, r.cell))
        .map(|r| Task::Seed { tree: r.id, progress: 0.0 })
}

/// Trees old enough to give seedlings.
fn seeds(r: &Resource) -> bool {
    matches!(r.stage(), Some(crate::forestry::TreeStage::Mature | crate::forestry::TreeStage::Old))
}

fn seed(sim: &mut SimState, ag: &mut Agent, dt: f32, duty: &Duty, tree: ResourceId, progress: f32) {
    let Duty::Forest(area) = *duty else { return abandon(ag) };
    let Some((cell, species)) = sim.resources.get(tree).filter(|r| seeds(r) && area.reaches(r.cell)).map(|r| (r.cell, r.species)) else {
        ag.path = None;
        ag.task = Task::Idle;
        return;
    };
    match reach(sim, ag, dt, cell) {
        None => {
            sim.agents.unreachable.insert(tree, sim.time_days + UNREACHABLE_RETRY_DAYS);
            ag.task = Task::Idle;
            return;
        }
        Some(false) => return,
        Some(true) => {}
    }
    note(ag, true);
    let progress = progress + dt * LABOUR_PER_DAY * worker_effort(sim, ag, crate::colony::Job::Forager);
    if progress + WORK_EPS < SEED_WORK {
        ag.task = Task::Seed { tree, progress };
        return;
    }
    // Seedlings of this tree's species, for the nearest forester's store.
    ag.load = Some(Load { species, ..Load::new(Good::Saplings, SEEDLINGS_PER_TREE, false) });
    ag.path = None;
    ag.task = Task::Carry { dest: NO_STORE };
}

#[allow(clippy::too_many_arguments)]
fn plant(sim: &mut SimState, ag: &mut Agent, dt: f32, duty: &Duty, store: BuildingId, cell: Cell, progress: f32) {
    let Duty::Forest(area) = *duty else { return abandon(ag) };
    let Some(load) = ag.load.filter(|l| l.good == Good::Saplings) else {
        if ag.load.is_some() || sim.buildings.building(store).is_none_or(|b| !b.is_complete() || !accepts(b.kind, Good::Saplings)) {
            return abandon(ag);
        }
        if !approach(sim, ag, dt, store) {
            return;
        }
        note(ag, true);
        let (got, species) = sim.buildings.stores.take_kind_from(store, Good::Saplings, 1.0);
        log_load(sim, ag, "TAKE_SAPLING", store, Good::Saplings, 1.0, got, &format!("species={species:?}"));
        if got > WORK_EPS {
            ag.load = Some(Load { species, ..Load::new(Good::Saplings, got, true) });
        } else {
            ag.task = Task::Idle;
        }
        return;
    };
    if !area.reaches(cell) || !crate::forestry::plantable(sim, cell) {
        ag.path = None;
        ag.task = match crate::forestry::plant_cell(sim, area, ag.cell, ag.id) {
            Some(cell) => Task::Plant { store, cell, progress: 0.0 },
            None => Task::Carry { dest: NO_STORE },
        };
        return;
    }
    match reach(sim, ag, dt, cell) {
        None => {
            crate::forestry::mark_unreachable(sim, cell);
            ag.task = Task::Idle;
            return;
        }
        Some(false) => return,
        Some(true) => {}
    }
    note(ag, true);
    let progress = progress + dt * LABOUR_PER_DAY * worker_effort(sim, ag, crate::colony::Job::Forager);
    if progress + WORK_EPS < PLANT_WORK {
        ag.task = Task::Plant { store, cell, progress };
        return;
    }
    // A sapling grows into its own species; one of no known species is a cutting from nearby.
    let species = load.species.unwrap_or_else(|| crate::forestry::planting_species(sim, cell));
    let tree = sim.resources.add_tree(cell, 0.0, species);
    sim.forestry.planted.insert(tree, (ag.id, sim.time_days));
    sim.ledger.planted += 1;
    let used = load.amount.min(1.0);
    log_load(sim, ag, "PLANT", NO_STORE, Good::Saplings, load.amount, used, &format!("species={species:?} at=({},{})", cell.0, cell.1));
    sim.buildings.stores.book_used(Good::Saplings, used, crate::buildings::Use::Other);
    let rest = load.amount - used;
    ag.load = (rest > WORK_EPS).then_some(Load { amount: rest, ..load });
    ag.task = Task::Idle;
}

#[cfg(test)]
mod ground_tests {
    use super::*;

    /// An open, founded valley with no stores, and one beaver standing on a dry cell.
    fn open_valley() -> (SimState, BeaverId, Cell) {
        let mut s = SimState::new(1);
        s.colony.found((0.0, 3.0), &crate::colony::RiverLand::default()).unwrap();
        let n = s.world.size;
        s.nav = NavGrid::new(n, n);
        s.resources.clear();
        s.work.supplies = None;
        let at = (n / 2, n / 2);
        let who = s.colony.beavers[0].id;
        s.agents.spawn(who, at);
        (s, who, at)
    }

    #[test]
    fn with_no_store_a_gathered_load_is_left_where_it_was_gathered() {
        let (mut s, who, at) = open_valley();
        let mut ag = s.agents.by_id.remove(&who).unwrap();
        ag.load = Some(Load::new(Good::Sticks, 2.0, false));
        ag.task = Task::Carry { dest: NO_STORE };
        carry(&mut s, &mut ag, 0.01, NO_STORE);
        assert!(ag.load.is_none() && ag.task == Task::Idle, "the gatherer puts it down and carries on");
        let pile = s.work.piles.items.first().expect("a pile on the ground");
        assert_eq!((pile.cell, pile.good, pile.amount), (at, Good::Sticks, 2.0));
        assert_eq!(s.buildings.stores.amount(Good::Sticks), 0.0, "nothing went into a store");
    }

    #[test]
    fn a_hungry_beaver_with_no_food_in_store_walks_to_food_on_the_ground_and_eats() {
        let (mut s, who, at) = open_valley();
        assert!(s.buildings.stores.amount(Good::Food) <= WORK_EPS, "no food in store");
        s.work.piles.drop_at((at.0 + 6, at.1), Good::Food, 3.0, 0.0);
        s.colony.beavers.iter_mut().find(|b| b.id == who).unwrap().needs.food = 0.2;
        for _ in 0..(1.0 / crate::STEP_DAYS) as u32 {
            step(&mut s, crate::STEP_DAYS);
        }
        let left = s.work.piles.items.first().map_or(0.0, |p| p.amount);
        assert!(left < 3.0, "it ate from the pile ({left} left)");
        assert!(s.colony.beaver(who).unwrap().needs.food > 0.5, "and is fed");
    }

    #[test]
    fn a_starving_beaver_eats_before_it_rests_even_what_it_just_put_down() {
        let (mut s, who, at) = open_valley();
        s.work.piles.drop_at((at.0 + 4, at.1), Good::Food, 3.0, 0.0);
        {
            let b = s.colony.beavers.iter_mut().find(|b| b.id == who).unwrap();
            b.needs.food = 0.0;
            b.needs.rest = 0.0;
            b.resting = true;
        }
        for _ in 0..(1.0 / crate::STEP_DAYS) as u32 {
            step(&mut s, crate::STEP_DAYS);
        }
        assert!(s.colony.beaver(who).unwrap().needs.food > 0.5, "tired as it is, it walked to the food and ate");

        // A forager with no store puts its load down, and only then eats from it.
        let (mut s, who, _) = open_valley();
        {
            let b = s.colony.beavers.iter_mut().find(|b| b.id == who).unwrap();
            b.needs.food = 0.0;
            b.needs.rest = 0.0;
            b.resting = true;
        }
        let ag = s.agents.by_id.get_mut(&who).unwrap();
        ag.load = Some(Load::new(Good::Food, 2.0, false));
        ag.task = Task::Carry { dest: NO_STORE };
        for _ in 0..(1.0 / crate::STEP_DAYS) as u32 {
            step(&mut s, crate::STEP_DAYS);
        }
        assert!(s.agents.get(who).unwrap().load.is_none(), "the load went down on the ground");
        assert!(s.colony.beaver(who).unwrap().needs.food > 0.5, "and it ate from it");
    }

    #[test]
    fn a_builder_whose_site_is_done_moves_on_to_the_next_site() {
        let (mut s, who, at) = open_valley();
        let (done, next) = (BuildingId(5), BuildingId(6));
        for (id, pos) in [(done, (at.0 + 4, at.1)), (next, (at.0 + 12, at.1))] {
            s.buildings.buildings.push(Building { id, kind: BuildingKind::Warehouse, pos, status: crate::buildings::Status::Site, output: 0.0, condition: 1.0, reinforced: false, dam: None, canal: None });
            s.work.add_site(id, BuildingKind::Warehouse, &[], 5.0).unwrap();
        }
        s.work.assign(done, 0, who).unwrap();
        s.steps = 0;
        retarget(&mut s);
        assert_eq!(s.work.slot_of(who).map(|x| x.0), Some(done), "its own site still has work");
        s.work.add_labour(done, 5.0).unwrap();
        retarget(&mut s);
        assert_eq!(s.work.slot_of(who).map(|x| x.0), Some(next), "done there: on to the next site");
    }

    #[test]
    fn builders_stay_builders_when_their_building_is_finished() {
        let (mut s, who, at) = open_valley();
        let (first, second) = (BuildingId(5), BuildingId(6));
        s.buildings.buildings.push(Building { id: first, kind: BuildingKind::Warehouse, pos: (at.0 + 4, at.1), status: crate::buildings::Status::Site, output: 0.0, condition: 1.0, reinforced: false, dam: None, canal: None });
        s.work.add_site(first, BuildingKind::Warehouse, &[], 1.0).unwrap();
        s.work.assign(first, 0, who).unwrap();
        s.work.add_labour(first, 1.0).unwrap();
        finish_sites(&mut s);
        assert!(s.buildings.building(first).unwrap().is_complete());
        assert_eq!(s.work.slot_of(who).map(|x| (x.0, x.2)), Some((first, Role::Build)), "still a builder, waiting where it finished");
        assert_eq!(crate::jobs::role_of(&s, who), Some(crate::jobs::RoleKind::Builder));
        s.buildings.buildings.push(Building { id: second, kind: BuildingKind::Warehouse, pos: (at.0 + 12, at.1), status: crate::buildings::Status::Site, output: 0.0, condition: 1.0, reinforced: false, dam: None, canal: None });
        s.work.add_site(second, BuildingKind::Warehouse, &[], 5.0).unwrap();
        s.steps = 0;
        retarget(&mut s);
        assert_eq!(s.work.slot_of(who).map(|x| x.0), Some(second), "on to the next builder job");
        tidy(&mut s);
        assert!(!s.work.place(first).unwrap().slots.iter().any(|x| x.role == Role::Build), "the seat it left behind is gone");
    }

    #[test]
    fn maintainers_mend_the_worst_building_in_place_without_orders() {
        let (mut s, who, at) = open_valley();
        let (worn, wrecked) = (BuildingId(5), BuildingId(6));
        for (id, pos, condition) in [(worn, (at.0 + 4, at.1), 0.7), (wrecked, (at.0 + 30, at.1), 0.2)] {
            let b = Building { id, kind: BuildingKind::Warehouse, pos, status: crate::buildings::Status::Complete, output: 0.0, condition, reinforced: false, dam: None, canal: None };
            s.work.add_complete(&b).unwrap();
            s.buildings.buildings.push(b);
        }
        for (g, _) in crate::maintenance::mend_rates(s.buildings.building(worn).unwrap()).0 {
            s.buildings.stores.produce_at(worn, g, 50.0, crate::buildings::Producer::Other).unwrap();
        }
        s.steps = 0;
        send_maintainers(&mut s);
        assert!(s.agents.maint_targets.is_empty(), "with no maintainers, nothing is mended");
        s.agents.assign(who, Some(Assignment::Maintain));
        send_maintainers(&mut s);
        assert!(s.maintenance.jobs.is_empty(), "no repair is ordered");
        assert_eq!(s.agents.maint_targets.get(&who), Some(&wrecked), "the worse-off building comes first, though further");
        assert!(matches!(duty_of(&s, s.agents.get(who).unwrap(), true), Duty::Mend(b) if b == wrecked));
        assert!(s.work.place(wrecked).unwrap().site.is_none(), "the building is not made a site");

        crate::maintenance::order(&mut s, worn, crate::maintenance::Refit::Repair).unwrap();
        send_maintainers(&mut s);
        assert_eq!(s.agents.maint_targets.get(&who), Some(&worn), "a building marked for repair comes first");

        let before: f32 = Good::ALL.iter().map(|&g| s.buildings.stores.amount(g)).sum();
        assert!(crate::maintenance::mend(&mut s, worn, 1000.0));
        assert_eq!(s.buildings.building(worn).unwrap().condition, 1.0);
        assert!(Good::ALL.iter().map(|&g| s.buildings.stores.amount(g)).sum::<f32>() < before, "mending draws on the stores");
        assert!(!s.maintenance.jobs.contains_key(&worn), "the mark goes once the building is whole");

        // A building begun is finished, though it rises past the automatic mark.
        s.buildings.buildings.iter_mut().find(|b| b.id == wrecked).unwrap().condition = crate::maintenance::AUTO_REPAIR_BELOW - 0.01;
        s.agents.maint_targets.insert(who, wrecked);
        assert!(crate::maintenance::mend(&mut s, wrecked, 0.5));
        assert!(s.buildings.building(wrecked).unwrap().condition > crate::maintenance::AUTO_REPAIR_BELOW);
        send_maintainers(&mut s);
        assert_eq!(s.agents.maint_targets.get(&who), Some(&wrecked), "it stays until the building is whole");
    }

    #[test]
    fn a_site_takes_as_many_builders_as_are_sent() {
        let (mut s, _, at) = open_valley();
        let site = BuildingId(5);
        s.buildings.buildings.push(Building { id: site, kind: BuildingKind::Warehouse, pos: (at.0 + 4, at.1), status: crate::buildings::Status::Site, output: 0.0, condition: 1.0, reinforced: false, dam: None, canal: None });
        s.work.add_site(site, BuildingKind::Warehouse, &[], 5.0).unwrap();
        let seats = s.work.place(site).unwrap().slots.len();
        let crew: Vec<BeaverId> = s.colony.beavers.iter().filter(|b| b.can_work()).map(|b| b.id).collect();
        for (n, &who) in crew.iter().enumerate() {
            s.agents.spawn(who, at);
            let (slot, _) = s.work.place(site).unwrap().open_seat(|_| true).expect("always a seat for a builder");
            s.apply(crate::Command::FillSlot { building: site, slot, who }).unwrap();
            assert_eq!(s.work.place(site).unwrap().builders(), n + 1);
        }
        assert!(crew.len() > seats, "more builders ({}) than the {seats} seats it began with", crew.len());
    }

    #[test]
    fn a_gatherer_whose_area_runs_out_moves_on_to_more() {
        let (mut s, who, at) = open_valley();
        let area = Area { min: (at.0 - 3, at.1 - 3), max: (at.0 + 3, at.1 + 3) };
        s.agents.assign(who, Some(Assignment::Area(ResourceKind::Plant, area)));
        let far = (at.0 + 30, at.1 + 5);
        s.resources.add(ResourceKind::Plant, far, 7.0);
        s.steps = 0;
        retarget(&mut s);
        match s.agents.get(who).unwrap().assignment {
            Some(Assignment::Area(ResourceKind::Plant, a)) => {
                assert!(a.contains(far), "the area moved onto the food: {a:?}");
                assert_eq!((a.max.0 - a.min.0, a.max.1 - a.min.1), (6, 6), "and kept its size");
            }
            other => panic!("still gathering food, got {other:?}"),
        }
    }

    #[test]
    fn a_beaver_sent_to_one_plant_goes_on_to_the_next_when_it_is_gone() {
        let (mut s, who, at) = open_valley();
        let first = s.resources.add(ResourceKind::Plant, (at.0 + 1, at.1), 7.0);
        s.agents.assign(who, Some(Assignment::Resource(first)));
        step(&mut s, crate::STEP_DAYS);
        s.resources.clear();
        let next = s.resources.add(ResourceKind::Plant, (at.0 + 6, at.1), 7.0);
        step(&mut s, crate::STEP_DAYS);
        assert_eq!(s.agents.get(who).unwrap().assignment, Some(Assignment::Resource(next)), "the first is gone: on to the next plant");
    }

    #[test]
    fn foragers_strip_fallen_trees_but_never_fell_one() {
        let (mut s, _, at) = open_valley();
        let area = Area { min: (at.0 - 10, at.1 - 10), max: (at.0 + 10, at.1 + 10) };
        for (i, sp) in [crate::trees::Tree::Aspen, crate::trees::Tree::Pine].into_iter().enumerate() {
            s.resources.add_tree((at.0 + 3, at.1 + 2 * i), 3000.0, sp);
        }
        let foragers = Job::resolve(&s, Assignment::Area(ResourceKind::Plant, area)).unwrap();
        let loggers = Job::resolve(&s, Assignment::Area(ResourceKind::Tree, area)).unwrap();
        let trees: Vec<_> = s.resources.items().iter().filter(|r| r.kind == ResourceKind::Tree).collect();
        assert!(trees.iter().all(|r| loggers.permits(r)), "loggers may fell both");
        assert!(trees.iter().all(|r| !foragers.permits(r)), "no standing tree is a forager's");
        let mut down = trees[0].clone();
        down.kind = ResourceKind::Felled;
        down.bark = 1.0;
        assert!(foragers.permits(&down), "bark on a fallen tree is still food");
        let plant = s.resources.add(ResourceKind::Plant, (at.0 + 1, at.1), 5.0);
        assert_eq!(foragers.pick(&s, at), Some(plant));
        let fallen = s.resources.add(ResourceKind::Felled, (at.0 - 6, at.1), 0.0);
        s.resources.get_mut(fallen).unwrap().bark = 1.0;
        assert_eq!(foragers.pick(&s, at), Some(fallen), "bark lying on the ground comes before a nearer plant");
    }

    #[test]
    fn a_role_kept_near_water_leaves_trees_farther_out() {
        let (mut s, _, at) = open_valley();
        for y in 0..s.nav.surface.height() {
            s.nav.set((at.0 + 12, y), Surface::Water);
        }
        let near = s.resources.add_tree((at.0 + 9, at.1), 3000.0, crate::trees::Tree::Aspen);
        let far = s.resources.add_tree((at.0 - 2, at.1), 3000.0, crate::trees::Tree::Aspen);
        let area = Area { min: (at.0 - 20, at.1 - 20), max: (at.0 + 20, at.1 + 20) };
        let loggers = Job::resolve(&s, Assignment::Area(ResourceKind::Tree, area)).unwrap();
        assert_eq!(loggers.pick(&s, at), Some(far), "with no limit the nearest tree");
        s.apply(crate::Command::SetShoreLimit { role: crate::jobs::RoleKind::Logger, cells: Some(5) }).unwrap();
        measure_shore(&mut s);
        assert_eq!(loggers.pick(&s, at), Some(near), "14 cells from water is past the limit; 3 is within");
        assert!(within_shore(&s, crate::jobs::RoleKind::Forager, (at.0 - 2, at.1)), "other roles are not limited");
    }

    #[test]
    fn a_logger_fetches_its_dropped_sticks_once_a_store_will_take_them() {
        let (mut s, who, at) = open_valley();
        s.work.piles.drop_at((at.0 + 3, at.1), Good::Sticks, 2.0, 0.0);
        let pile = s.work.piles.items[0].id;
        let area = Area { min: (at.0 - 5, at.1 - 5), max: (at.0 + 5, at.1 + 5) };
        let job = Job::resolve(&s, Assignment::Area(ResourceKind::Tree, area)).unwrap();
        let ag = s.agents.by_id.get(&who).unwrap().clone();
        assert_eq!(pile_task(&s, &job, &ag), None, "with no store the sticks stay where they lie");
        let b = Building { id: BuildingId(5), kind: BuildingKind::Warehouse, pos: (at.0 + 12, at.1), status: crate::buildings::Status::Complete, output: 0.0, condition: 1.0, reinforced: false, dam: None, canal: None };
        for c in b.cells() { s.nav.set(c, Surface::Solid); }
        s.buildings.buildings.push(b);
        assert_eq!(pile_task(&s, &job, &ag), Some(Task::Pick { pile, then: NO_STORE }), "with a store, the dropped sticks come first");
        let only = Job::resolve(&s, Assignment::Area(ResourceKind::Tree, Area { min: (at.0 + 30, at.1 + 30), max: (at.0 + 35, at.1 + 35) })).unwrap();
        assert_eq!(pile_task(&s, &only, &ag), None, "sticks far from the job's area are not its own");
        let mut ag = s.agents.by_id.remove(&who).unwrap();
        let duty = Duty::Collect(job);
        ag.task = Task::Pick { pile, then: NO_STORE };
        for _ in 0..400 {
            if ag.load.is_some() {
                break;
            }
            pick_up(&mut s, &mut ag, 0.01, &duty, pile, NO_STORE);
        }
        assert_eq!(ag.load.map(|l| (l.good, l.amount)), Some((Good::Sticks, 2.0)), "picked up");
        assert_eq!(ag.task, Task::Carry { dest: NO_STORE }, "and off to a store");
        assert!(s.work.piles.items.is_empty());
    }

    #[test]
    fn a_builder_takes_a_nearby_pile_over_a_far_store() {
        let (mut s, who, at) = open_valley();
        let b = Building { id: BuildingId(5), kind: BuildingKind::Warehouse, pos: (at.0 + 40, at.1), status: crate::buildings::Status::Complete, output: 0.0, condition: 1.0, reinforced: false, dam: None, canal: None };
        for c in b.cells() { s.nav.set(c, Surface::Solid); }
        s.buildings.buildings.push(b);
        s.buildings.stores.produce_at(BuildingId(5), Good::Sticks, 10.0, crate::buildings::Producer::Other).unwrap();
        s.work.piles.drop_at((at.0 + 2, at.1), Good::Sticks, 2.0 * LOAD_SIZE, 0.0);
        let pile = s.work.piles.items[0].id;
        let ag = s.agents.by_id.remove(&who).unwrap();
        assert_eq!(fetch_store(&s, &ag, &[(at.0 + 4, at.1)], Good::Sticks, LOAD_SIZE), Some(Source::Pile(pile)));
        let mut far = ag.clone();
        far.cell = (at.0 + 37, at.1);
        assert_eq!(fetch_store(&s, &far, &[(at.0 + 38, at.1 + 2)], Good::Sticks, LOAD_SIZE), Some(Source::Store(BuildingId(5))), "beside the store, the store wins");
    }
}

#[cfg(test)]
mod larder_tests {
    use super::*;
    use crate::buildings::{Producer, Status};
    use crate::trees::Tree;

    /// An open map with no resources, a family, one beaver placed, a finished food store and a
    /// forester beside it; returns the food store and the forester's area.
    fn setup() -> (SimState, BeaverId, BuildingId, Area) {
        let mut s = SimState::new(1);
        s.colony.found((0.0, 3.0), &crate::colony::RiverLand::default()).unwrap();
        let n = s.world.size;
        s.nav = NavGrid::new(n, n);
        s.resources.clear();
        let c = (0..n * n)
            .map(|i| (i % n, i / n))
            .find(|&(x, y)| x > 20 && y > 20 && x + 20 < n && y + 20 < n && (0..12).all(|k| crate::forestry::plantable(&s, (x + 2 * k, y))))
            .expect("a dry stretch");
        for (id, kind, x) in [(7, BuildingKind::Larder, c.0 + 8), (8, BuildingKind::Forester, c.0 + 12)] {
            let b = Building { id: BuildingId(id), kind, pos: (x, c.1 + 6), status: Status::Complete, output: 0.0, condition: 1.0, reinforced: false, dam: None, canal: None };
            s.work.add_complete(&b).unwrap();
            s.buildings.buildings.push(b);
        }
        assert!(s.work.place(BuildingId(7)).unwrap().slots.is_empty(), "a food store is only a store");
        let area = s.work.place(BuildingId(8)).unwrap().area.unwrap();
        let who = s.colony.beavers[0].id;
        s.agents.spawn(who, (c.0 + 8, c.1 + 5));
        s.nav.reserve(who, (c.0 + 8, c.1 + 5));
        (s, who, BuildingId(7), area)
    }

    fn take_agent(s: &mut SimState, who: BeaverId) -> Agent {
        s.agents.by_id.remove(&who).unwrap()
    }

    #[test]
    fn food_and_saplings_go_to_food_stores_and_materials_to_warehouses() {
        // Each store keeps its own goods: food, sticks and clay, pebbles, saplings.
        assert!(accepts(BuildingKind::Larder, Good::Food) && !accepts(BuildingKind::Larder, Good::Saplings));
        assert!(!accepts(BuildingKind::Larder, Good::Sticks));
        assert!(!accepts(BuildingKind::Warehouse, Good::Food) && !accepts(BuildingKind::Warehouse, Good::Saplings));
        assert!(accepts(BuildingKind::Warehouse, Good::Sticks) && accepts(BuildingKind::Warehouse, Good::Clay) && !accepts(BuildingKind::Warehouse, Good::Pebbles));
        assert!(accepts(BuildingKind::Toolshed, Good::Pebbles) && accepts(BuildingKind::Forester, Good::Saplings));
        assert!(!is_store(BuildingKind::Logistics), "logistics moves goods, it does not keep them");
    }

    fn deliver_food(s: &mut SimState, a: &mut Agent, species: Option<Tree>) {
        a.load = Some(Load { species, ..Load::new(Good::Food, 4.0, false) });
        a.task = Task::Carry { dest: NO_STORE };
        for _ in 0..5 {
            let dest = match a.task { Task::Carry { dest } => dest, _ => NO_STORE };
            carry(s, a, 0.01, dest);
        }
        assert!(a.load.is_none());
    }

    #[test]
    fn foresters_collect_seedlings_for_their_store_and_foragers_bring_none() {
        let (mut s, who, store, area) = setup();
        let forester = s.work.place(BuildingId(8)).unwrap();
        assert_eq!(forester.slots.iter().map(|x| x.role).collect::<Vec<_>>(), vec![Role::Forest], "one forester seat");
        assert_eq!(BuildingKind::Forester.spec().area, Some((91, 91)), "a wide range");
        let mut a = take_agent(&mut s, who);
        deliver_food(&mut s, &mut a, Some(Tree::Birch));
        assert_eq!(s.work.stock_of(store, Good::Food), 4.0);
        assert_eq!(s.work.stock_of(BuildingId(8), Good::Saplings), 0.0, "foragers bring no seedlings");

        let at = (a.cell.0 + 1, a.cell.1);
        let tree = s.resources.add_tree(at, 4000.0, Tree::Birch);
        let duty = Duty::Forest(area);
        assert!(matches!(seed_task(&s, area, &a), Some(Task::Seed { tree: t, .. }) if t == tree));
        a.task = Task::Seed { tree, progress: 0.0 };
        for _ in 0..200 {
            match a.task {
                Task::Seed { tree, progress } => seed(&mut s, &mut a, 0.01, &duty, tree, progress),
                Task::Carry { dest } => carry(&mut s, &mut a, 0.01, dest),
                _ => break,
            }
        }
        let saplings = s.buildings.stores.kinds_at(BuildingId(8), Good::Saplings);
        assert_eq!(saplings, vec![(Some(Tree::Birch), SEEDLINGS_PER_TREE)], "seedlings of the tree's species, in the forester's store");
    }

    #[test]
    fn seedlings_found_by_food_are_of_the_nearest_seeding_tree_food_trees_first() {
        let (mut s, _, _, area) = setup();
        let c = (area.min.0 + 4, area.min.1 + 4);
        let r = crate::forestry::SEEDLING_RADIUS;
        assert_eq!(crate::forestry::seedling_species(&s, c), None);
        s.resources.add_tree((c.0 + r + 1, c.1), crate::forestry::MATURE_AGE, Tree::Aspen);
        s.resources.add_tree((c.0, c.1 + 1), 0.0, Tree::Willow);
        assert_eq!(crate::forestry::seedling_species(&s, c), None, "too far, or too young to seed");
        s.resources.add_tree((c.0 + 1, c.1), crate::forestry::MATURE_AGE, Tree::Pine);
        assert_eq!(crate::forestry::seedling_species(&s, c), Some(Tree::Pine));
        s.resources.add_tree((c.0 + r, c.1 + r), crate::forestry::MATURE_AGE, Tree::Maple);
        assert_eq!(crate::forestry::seedling_species(&s, c), Some(Tree::Maple));
    }

    #[test]
    fn a_stored_sapling_is_planted_and_grows_as_its_own_species() {
        let (mut s, who, _, area) = setup();
        // A food tree nearby that a sapling of no known species would be a cutting of.
        let willow = s.resources.add_tree((area.min.0, area.min.1), crate::forestry::MATURE_AGE, Tree::Willow);
        s.buildings.stores.produce_kind_at(BuildingId(8), Good::Saplings, Some(Tree::Pine), 1.0, Producer::Other).unwrap();
        s.buildings.stores.produce_kind_at(BuildingId(8), Good::Saplings, Some(Tree::Birch), 3.0, Producer::Other).unwrap();
        let mut a = take_agent(&mut s, who);
        let duty = Duty::Forest(area);
        let mut planted = Vec::new();
        for _ in 0..20000 {
            if planted.len() == 4 {
                break;
            }
            match a.task.clone() {
                Task::Plant { store, cell, progress } => {
                    plant(&mut s, &mut a, 0.01, &duty, store, cell, progress);
                    if let Some(r) = s.resources.items().iter().find(|r| r.id != willow && r.kind == ResourceKind::Tree && !planted.contains(&r.id)) {
                        planted.push(r.id);
                    }
                }
                _ => a.task = plant_task(&s, area, &a).expect("a sapling and somewhere to plant it"),
            }
        }
        let species = |id| s.resources.get(id).unwrap().species.unwrap();
        let mut kinds: Vec<Tree> = planted.iter().map(|&id| species(id)).collect();
        kinds.sort();
        assert_eq!(kinds, vec![Tree::Birch, Tree::Birch, Tree::Birch, Tree::Pine]);
        assert_eq!(s.work.stock_of(BuildingId(8), Good::Saplings), 0.0);
        let pine = *planted.iter().find(|&&id| species(id) == Tree::Pine).unwrap();
        // Grow it on through spring until it is a young tree: still a pine.
        s.agents.by_id.insert(who, a);
        crate::forestry::step(&mut s, 0.0);
        s.resources.get_mut(pine).unwrap().age_days = crate::forestry::YOUNG_AGE - 0.5;
        for _ in 0..3 {
            s.time_days += 1.0;
            crate::forestry::step(&mut s, 1.0);
        }
        let tree = s.resources.get(pine).unwrap();
        assert_eq!(tree.stage(), Some(crate::forestry::TreeStage::Young));
        assert_eq!(tree.species, Some(Tree::Pine));
    }

    /// Foragers used to wait on a grazed plant by the food store, carrying each sliver of
    /// regrowth in: they looked stuck at the store and the colony went hungry. A plant a
    /// forager is on, though, it picks to nothing, and the plant dies.
    #[test]
    fn foragers_pass_over_a_grazed_plant_and_move_on_when_one_runs_out() {
        let (mut s, who, _, area) = setup();
        let at = s.agents.get(who).unwrap().cell;
        let grazed = s.resources.add(ResourceKind::Plant, (at.0 + 1, at.1), PLANT_MIN_TAKE + 0.1);
        let lush = s.resources.add(ResourceKind::Plant, (at.0 + 4, at.1 + 2), 7.0);
        let n = s.world.size;
        let job = Job::resolve(&s, Assignment::Area(ResourceKind::Plant, Area { min: (0, 0), max: (n, n) })).unwrap();
        assert_eq!(job.pick(&s, at), Some(lush), "a plant with too little on it is not worth a trip");
        // Half-way through a load, the plant being stripped runs out: on to the next one.
        s.resources.get_mut(grazed).unwrap().amount = 0.2;
        let mut a = take_agent(&mut s, who);
        a.load = Some(Load::new(Good::Food, 1.5, false));
        a.task = Task::Collect { target: grazed, progress: 0.4 };
        for _ in 0..200 {
            let job = Job::resolve(&s, Assignment::Area(ResourceKind::Plant, Area { min: (0, 0), max: (n, n) })).unwrap();
            collect(&mut s, &mut a, 0.01, Duty::Collect(job), grazed);
            if s.resources.get(grazed).is_none() {
                break;
            }
        }
        assert!(s.resources.get(grazed).is_none(), "the plant was picked bare and died");
        // All of it but the last sliver, which dies with the plant (`PLANT_BARE`).
        assert!(a.load.unwrap().amount > 1.7 - crate::resources::PLANT_BARE - 1e-3, "all of it was taken: {:?}", a.load);
        let kept = a.load.unwrap().amount;
        let job = Job::resolve(&s, Assignment::Area(ResourceKind::Plant, Area { min: (0, 0), max: (n, n) })).unwrap();
        collect(&mut s, &mut a, 0.01, Duty::Collect(job), grazed);
        assert!(matches!(a.task, Task::Collect { target, .. } if target == lush), "{:?}", a.task);
        assert_eq!(a.load.map(|l| l.amount), Some(kept), "the part load is kept");
        let _ = area;
    }

    #[test]
    fn planters_turn_a_stored_sapling_into_a_tree_in_range() {
        let (mut s, who, _, area) = setup();
        s.buildings.stores.produce_at(BuildingId(8), Good::Saplings, 2.0, Producer::Other).unwrap();
        let mut a = take_agent(&mut s, who);
        let duty = Duty::Forest(area);
        for _ in 0..3000 {
            if s.resources.items().iter().any(|r| r.kind == ResourceKind::Tree) {
                break;
            }
            match a.task.clone() {
                Task::Plant { store, cell, progress } => plant(&mut s, &mut a, 0.01, &duty, store, cell, progress),
                _ => a.task = plant_task(&s, area, &a).expect("a sapling and somewhere to plant it"),
            }
        }
        let trees: Vec<_> = s.resources.items().iter().filter(|r| r.kind == ResourceKind::Tree).collect();
        assert_eq!(trees.len(), 1);
        assert!(area.reaches(trees[0].cell));
        assert!(!area.reaches(area.min), "a forester's reach is round: its corners are out");
        assert_eq!(trees[0].stage(), Some(crate::forestry::TreeStage::Sapling));
        assert!(a.load.is_none());
        assert_eq!(s.work.stock_of(BuildingId(8), Good::Saplings), 1.0);
        assert_eq!(s.buildings.stores.consumed_total[Good::Saplings.index()], 1.0);
    }

    #[test]
    fn pebbles_are_heavy_small_loads_carried_slowly() {
        let (s, who, _, _) = setup();
        let mut a = s.agents.get(who).unwrap().clone();
        assert!(load_size(Good::Pebbles) < load_size(Good::Clay));
        a.load = Some(Load::new(Good::Clay, 4.0, false));
        let clay = pace(&s.nav, &a);
        a.load = Some(Load::new(Good::Pebbles, load_size(Good::Pebbles), false));
        assert!(pace(&s.nav, &a) < clay, "a few pebbles slow a beaver more than a full load of clay");
    }

    #[test]
    fn loggers_do_not_set_out_for_a_full_store_but_ordered_felling_goes_on() {
        let mut s = SimState::new(1);
        let n = s.world.size;
        let logging = Job::resolve(&s, Assignment::Area(ResourceKind::Tree, Area { min: (0, 0), max: (n, n) })).unwrap();
        let tree = s.resources.items().iter().find(|r| r.kind == ResourceKind::Tree && logging.permits(r)).map(|r| (r.id, r.cell)).unwrap();
        let from = tree.1;
        assert!(logging.pick(&s, from).is_some(), "with no store, loads are left on the ground");
        let b = Building { id: BuildingId(3), kind: BuildingKind::Warehouse, pos: from, status: Status::Complete, output: 0.0, condition: 1.0, reinforced: false, dam: None, canal: None };
        s.buildings.buildings.push(b);
        s.buildings.stores.deposit(Good::Sticks, 1e6).unwrap();
        assert!(logging.pick(&s, from).is_none(), "nothing is fetched for a full store");
        let order = Job::resolve(&s, Assignment::Resource(tree.0)).unwrap();
        assert_eq!(order.pick(&s, from), Some(tree.0), "a tree ordered down is still felled");
    }

    #[test]
    fn pebbles_lie_in_a_few_beds_and_need_a_tool_shed() {
        let mut s = SimState::new(1);
        let beds: Vec<Cell> = s.resources.items().iter().filter(|r| r.kind == ResourceKind::Pebbles).map(|r| r.cell).collect();
        assert!(!beds.is_empty(), "the map has pebbles");
        let mut clusters: Vec<Cell> = Vec::new();
        for &c in &beds {
            if clusters.iter().all(|&o| distance_sq(c, o) > 100) {
                clusters.push(c);
            }
        }
        assert!(clusters.len() <= crate::resources::PEBBLE_OUTCROPS, "pebbles are clustered, not scattered");
        let n = s.world.size;
        let all = Assignment::Area(ResourceKind::Pebbles, Area { min: (0, 0), max: (n, n) });
        assert!(!gathering_available(&s, all));
        let shed = Building { id: BuildingId(9), kind: BuildingKind::Toolshed, pos: (2, 2), status: Status::Complete, output: 0.0, condition: 1.0, reinforced: false, dam: None, canal: None };
        s.buildings.buildings.push(shed);
        assert!(gathering_available(&s, all));
    }

    fn distance_sq(a: Cell, b: Cell) -> usize {
        a.0.abs_diff(b.0).pow(2) + a.1.abs_diff(b.1).pow(2)
    }
}

#[cfg(test)]
mod freight_tests {
    use super::*;
    #[test]
    fn gathered_materials_with_no_store_or_no_room_are_left_on_the_ground() {
        let mut s = SimState::new(1);
        s.nav = NavGrid::new(16, 16);
        s.work.supplies = Some((5, 5));
        let mut agents = Agents::default();
        agents.spawn(BeaverId(1), (5, 4));
        let mut a = agents.by_id.remove(&BeaverId(1)).unwrap();
        a.load = Some(Load::new(Good::Clay, 4.0, false));
        a.task = Task::Carry { dest: NO_STORE };
        carry(&mut s, &mut a, 0.01, NO_STORE);
        assert_eq!(s.buildings.stores.amount(Good::Clay), 0.0, "the starting supplies are not a store to fill");
        assert_eq!(s.work.piles.items.iter().map(|p| (p.cell, p.amount)).collect::<Vec<_>>(), vec![((5, 4), 4.0)]);
        assert!(a.load.is_none());
        let b = Building { id: BuildingId(3), kind: BuildingKind::Warehouse, pos: (9, 9), status: crate::buildings::Status::Complete, output: 0.0, condition: 1.0, reinforced: false, dam: None, canal: None };
        s.buildings.buildings.push(b);
        s.buildings.stores.deposit(Good::Clay, 1000.0).unwrap();
        a.load = Some(Load::new(Good::Clay, 4.0, false));
        a.task = Task::Carry { dest: NO_STORE };
        carry(&mut s, &mut a, 0.01, NO_STORE);
        assert!(a.load.is_none(), "a full store does not keep the beaver waiting");
        assert!(matches!(a.task, Task::Idle));
        assert_eq!(s.work.piles.items.iter().map(|p| p.amount).sum::<f32>(), 8.0, "the load is left beside the first");
    }
    #[test]
    fn direct_move_walks_over_multiple_steps_and_reports_blockage() {
        let mut s = SimState::new(1);
        s.nav = NavGrid::new(32, 32);
        let id = BeaverId(1);
        let mut agents = Agents::default();
        agents.spawn(id, (2, 2));
        s.nav.reserve(id, (2, 2));
        agents.assign(id, Some(Assignment::Go((12, 2))));
        let mut a = agents.by_id.remove(&id).unwrap();
        go_to(&mut s, &mut a, crate::STEP_DAYS, (12, 2));
        assert!(a.cell.0 > 2 && a.cell.0 < 12);
        for _ in 0..20 { go_to(&mut s, &mut a, crate::STEP_DAYS, (12, 2)); }
        assert_eq!(a.cell, (12, 2));
        assert_eq!(a.move_state, MoveState::Arrived);
        a.assignment = Some(Assignment::Go((15, 2)));
        s.nav.set((15, 2), Surface::Solid);
        go_to(&mut s, &mut a, crate::STEP_DAYS, (15, 2));
        assert_eq!(a.move_state, MoveState::Blocked);
        assert_eq!(a.assignment, None);
    }
    #[test]
    fn builders_walk_to_the_site_over_multiple_steps() {
        let mut s = SimState::new(1);
        s.nav = NavGrid::new(32,32);
        let id = BeaverId(1);
        s.agents.spawn(id,(2,2));
        let mut agent = s.agents.get(id).unwrap().clone();
        let site = BuildingId(1);
        s.buildings.buildings.push(Building { id:site,kind:BuildingKind::Warehouse,pos:(12,2),status:crate::buildings::Status::Site,output:0.0,condition:1.0,reinforced:false,dam: None, canal: None });
        s.nav.reserve(id,agent.cell);
        assert!(!approach(&mut s,&mut agent,crate::STEP_DAYS,site));
        assert!(agent.cell.0 > 2 && agent.cell.0 < 12);
        assert!(agent.trail.len() <= 2,"builder crossed the entire route in one step");
        for _ in 0..20 { approach(&mut s,&mut agent,crate::STEP_DAYS,site); }
        assert!(approach(&mut s,&mut agent,crate::STEP_DAYS,site));
    }
    #[test]
    fn haulers_deliver_to_refits_without_creating_or_losing_stock() {
        let mut s=SimState::new(1);
        for (id,pos) in [(BuildingId(1),(10,10)),(BuildingId(2),(20,10))] {
            let b=Building{id,kind:BuildingKind::Warehouse,pos,status:crate::buildings::Status::Complete,output:0.0,condition:0.5,reinforced:false,dam: None, canal: None};
            s.work.add_complete(&b).unwrap();s.buildings.buildings.push(b);
        }
        crate::maintenance::order(&mut s,BuildingId(2),crate::maintenance::Refit::Reinforce).unwrap();
        s.buildings.stores.produce_at(BuildingId(1),Good::Clay,10.0,crate::buildings::Producer::Other).unwrap();
        let mut agents=Agents::default();agents.spawn(BeaverId(1),(9,10));let mut a=agents.by_id.remove(&BeaverId(1)).unwrap();
        fetch(&mut s,&mut a,0.01,&Duty::Haul,BuildingId(1),Good::Clay,BuildingId(2),true);
        let load=a.load.unwrap().amount;
        assert_eq!(s.buildings.stores.consumed_total[Good::Clay.index()],0.0);
        a.cell=(19,10);deliver(&mut s,&mut a,0.01,BuildingId(2));
        assert_eq!(s.buildings.stores.consumed_total[Good::Clay.index()],load);
        assert_eq!(s.buildings.stores.produced_total[Good::Clay.index()],10.0);
        assert!((s.buildings.stores.amount(Good::Clay)+load-10.0).abs()<1e-5);
    }
    #[test]
    fn nearest_store_matches_exhaustive_paths_with_detours_and_canals() {
        let mut s=SimState::new(1);let n=s.world.size;s.nav=NavGrid::new(n,n);
        for (i,pos) in [(3,3),(n/2,3),(3,n/2),(n-6,n-6)].into_iter().enumerate() {
            let b=Building{id:BuildingId(i as u64+1),kind:BuildingKind::Warehouse,pos,status:crate::buildings::Status::Complete,output:0.0,condition:1.0,reinforced:false,dam: None, canal: None};
            for c in b.cells(){s.nav.set(c,Surface::Solid);}s.buildings.buildings.push(b);
        }
        for y in 0..n-2 {if y!=n/2{s.nav.set((n/3,y),Surface::Solid);}}
        for x in 0..n {s.nav.set((x,n/2+4),Surface::Canal);}s.nav.label_regions();
        for from in [(1,1),(n/2,n/2),(n-2,n-2),(2,3)] {
            let who=BeaverId(1);let reference=s.buildings.buildings.iter().filter_map(|b| {
                if adjacent(from,b){return Some((b.id,Path{steps:Vec::new(),cost:0.0}));}
                s.nav.find_path_beside_any(who,from,&b.cells().collect::<Vec<_>>()).map(|p|(b.id,p))
            }).min_by(|a,b|a.1.cost.total_cmp(&b.1.cost).then(a.0.cmp(&b.0)));
            assert_eq!(nearest_store(&s,who,from,Good::Sticks),reference);
        }
    }
    /// Builders fetch from the store with the shortest walk or swim there and on to the work,
    /// not the one nearest as the crow flies.
    #[test]
    fn fetch_store_matches_exhaustive_routes_via_each_store() {
        let mut s=SimState::new(1);let n=s.world.size;s.nav=NavGrid::new(n,n);
        for (i,pos) in [(3,3),(n/2,3),(3,n/2),(n-6,n-6)].into_iter().enumerate() {
            let b=Building{id:BuildingId(i as u64+1),kind:BuildingKind::Warehouse,pos,status:crate::buildings::Status::Complete,output:0.0,condition:1.0,reinforced:false,dam: None, canal: None};
            for c in b.cells(){s.nav.set(c,Surface::Solid);}s.buildings.buildings.push(b);
            s.buildings.stores.produce_at(BuildingId(i as u64+1),Good::Sticks,10.0,crate::buildings::Producer::Other).unwrap();
        }
        for y in 0..n-2 {if y!=n/2{s.nav.set((n/3,y),Surface::Solid);}}
        for x in 0..n {s.nav.set((x,n/2+4),Surface::Canal);}s.nav.label_regions();
        let mut agents=Agents::default();
        for (from,to) in [((1,1),(n-2,2)),((n/2,n/2),(2,n-3)),((n-2,n-2),(n/3+3,3)),((2,3),(n/2,n/2+8))] {
            agents.spawn(BeaverId(1),from);let ag=agents.by_id.remove(&BeaverId(1)).unwrap();
            let reference=s.buildings.buildings.iter().filter_map(|b| {
                let cells:Vec<Cell>=b.cells().collect();
                let (there,at)=route_beside(&s,ag.id,from,&cells)?;
                let (on,_)=route_beside(&s,ag.id,at,&[to])?;
                Some((there+on,b.id))
            }).min_by(|a,b|a.0.total_cmp(&b.0).then(a.1.cmp(&b.1))).map(|r|Source::Store(r.1));
            assert!(reference.is_some());
            assert_eq!(fetch_store(&s,&ag,&[to],Good::Sticks,LOAD_SIZE),reference,"from {from:?} to {to:?}");
        }
    }
    #[test]
    fn resource_choice_matches_exhaustive_reachable_ranking() {
        let mut s=SimState::new(1);let size=s.world.size;let mut nav=NavGrid::new(size,size);nav.rebuild(&s);s.nav=nav;
        for r in s.resources.items().iter().step_by(3) {s.agents.unreachable.insert(r.id,1.0);}
        for kind in [ResourceKind::Tree,ResourceKind::Plant,ResourceKind::MudBank,ResourceKind::RiverClay] {
            for from in [(5,5),(size/2,size/2),(size-5,size-5)] {
                for area in [None,Some(Area{min:(0,0),max:(size/2,size/2)})] {
                    let job=Job{kinds:kinds_for(kind),area,only:None,min_age:300.0,role:None,frozen:false};
                    let reference=s.resources.items().iter().filter(|r|job.permits(r) && worth_starting(r)
                        && s.agents.unreachable.get(&r.id).is_none_or(|&until|s.time_days>=until)
                        && s.nav.may_reach_beside(from,r.cell))
                        .min_by_key(|r|{
                            let haul=drop_spots(&s,job.good_of(r),from).iter().map(|&d|cell_dist(r.cell,d)).fold(f32::INFINITY,f32::min);
                            let kind=match r.kind {ResourceKind::WaterPlant=>ResourceKind::Plant,ResourceKind::RiverClay=>ResourceKind::MudBank,k=>k};
                            (job.kinds.iter().position(|&k|k==kind).unwrap_or(0),trip_key(cell_dist(from,r.cell)+if haul.is_finite(){haul}else{0.0}),r.id)
                        }).map(|r|r.id);
                    assert_eq!(job.pick(&s,from),reference);
                }
            }
        }
    }
    #[test]
    fn gatherers_spread_out_and_choose_by_their_own_trip() {
        let mut s=SimState::new(1);let n=s.world.size;s.nav=NavGrid::new(n,n);s.resources.clear();
        let store=Building{id:BuildingId(1),kind:BuildingKind::Warehouse,pos:(40,40),status:crate::buildings::Status::Complete,output:0.0,condition:1.0,reinforced:false,dam:None,canal:None};
        for c in store.cells(){s.nav.set(c,Surface::Solid);}s.buildings.buildings.push(store);s.nav.label_regions();
        // Two deposits equally far from the store: the one nearer the beaver wins.
        let west=s.resources.add(ResourceKind::MudBank,(30,41),20.0);
        let east=s.resources.add(ResourceKind::MudBank,(53,41),20.0);
        let job=Job{kinds:kinds_for(ResourceKind::MudBank),area:None,only:None,min_age:0.0,role:None,frozen:false};
        assert_eq!(job.pick(&s,(20,41)),Some(west));
        assert_eq!(job.pick(&s,(60,41)),Some(east));
        // A deposit takes two diggers at once; a third goes to the next one.
        for (i,cell) in [(22,40),(22,42)].into_iter().enumerate() {
            let id=BeaverId(100+i as u64);s.agents.spawn(id,cell);
            s.agents.by_id.get_mut(&id).unwrap().task=Task::Collect{target:west,progress:0.0};
        }
        assert_eq!(job.pick(&s,(20,41)),Some(east),"west is fully worked");
        // River clay is no better than a mud bank: the nearer one wins.
        let river=s.resources.add(ResourceKind::RiverClay,(58,60),2.0);
        assert_eq!(job.pick(&s,(60,41)),Some(east),"walked past a mud bank for distant river clay");
        assert_eq!(job.pick(&s,(58,58)),Some(river));
        // A tree takes one logger.
        let tree=s.resources.add_tree((25,30),crate::forestry::MATURE_AGE,crate::trees::Tree::Aspen);
        let other=s.resources.add_tree((25,33),crate::forestry::MATURE_AGE,crate::trees::Tree::Aspen);
        let loggers=Job{kinds:kinds_for(ResourceKind::Tree),area:None,only:None,min_age:0.0,role:None,frozen:false};
        assert_eq!(loggers.pick(&s,(25,29)),Some(tree));
        s.agents.by_id.get_mut(&BeaverId(100)).unwrap().task=Task::Collect{target:tree,progress:0.0};
        assert_eq!(loggers.pick(&s,(25,29)),Some(other));
    }
    #[test]
    fn hauling_removes_transit_stock_and_preserves_production_totals() {
        let mut s=SimState::new(1);
        for (id,pos) in [(BuildingId(1),(10,10)),(BuildingId(2),(20,10))] {
            let b=Building{id,kind:BuildingKind::Warehouse,pos,status:crate::buildings::Status::Complete,output:0.0,condition:0.5,reinforced:false,dam: None, canal: None};
            s.work.add_complete(&b).unwrap();s.buildings.buildings.push(b);
        }
        s.buildings.stores.produce_at(BuildingId(1),Good::Sticks,20.0,crate::buildings::Producer::Other).unwrap();
        s.work.hauls.push(crate::work::HaulOrder{from:BuildingId(1),to:BuildingId(2),good:Good::Sticks});
        crate::maintenance::order(&mut s,BuildingId(2),crate::maintenance::Refit::Repair).unwrap();
        let mut agents=Agents::default();agents.spawn(BeaverId(1),(9,10));let mut a=agents.by_id.remove(&BeaverId(1)).unwrap();
        fetch(&mut s,&mut a,0.01,&Duty::Haul,BuildingId(1),Good::Sticks,BuildingId(2),false);
        assert_eq!(s.buildings.stores.amount(Good::Sticks),20.0-LOAD_SIZE);assert_eq!(a.load.unwrap().amount,LOAD_SIZE);
        assert_eq!(s.work.stock_of(BuildingId(1),Good::Sticks),20.0-LOAD_SIZE);
        a.cell=(19,10);haul(&mut s,&mut a,0.01,BuildingId(2));
        assert_eq!(s.work.stock_of(BuildingId(2),Good::Sticks),LOAD_SIZE);assert_eq!(s.buildings.stores.amount(Good::Sticks),20.0);
        assert_eq!(s.buildings.stores.produced_total[Good::Sticks.index()],20.0);assert!(a.load.is_none());
    }
}

fn adjacent_to_store(sim: &SimState, c: Cell, dest: BuildingId, good: Good) -> bool {
    sim.buildings.building(dest).is_some_and(|b| b.is_complete() && accepts(b.kind, good) && adjacent(c, b))
}

/// The profile's timer for a beaver's step, by what it was doing when the step began.
fn act_timer(task: &Task) -> &'static str {
    match task {
        Task::Idle => "step/agents/beaver idle (choosing work)",
        Task::Rest => "step/agents/beaver rest",
        Task::DamWork { .. } => "step/agents/beaver dam work",
        Task::Dig { .. } => "step/agents/beaver dig canal",
        Task::Gate { .. } => "step/agents/beaver gate",
        Task::Collect { .. } => "step/agents/beaver collect",
        Task::Plant { .. } => "step/agents/beaver plant",
        Task::Carry { .. } => "step/agents/beaver carry",
        Task::Fetch { .. } => "step/agents/beaver fetch",
        Task::Build { .. } => "step/agents/beaver build",
        Task::Haul { .. } => "step/agents/beaver haul",
        Task::Pick { .. } => "step/agents/beaver pick up from the ground",
        Task::Eat { .. } => "step/agents/beaver eat from the ground",
        Task::Mend { .. } => "step/agents/beaver mend",
        Task::Seed { .. } => "step/agents/beaver collect seedlings",
        Task::Watch { .. } => "step/agents/beaver keep watch",
        Task::Barge { .. } => "step/agents/beaver barge",
    }
}

pub fn step(sim: &mut SimState, dt_days: f32) {
    if !sim.colony.is_founded() {
        return;
    }
    { let _p = crate::prof::scope("step/agents/retire fills"); crate::stock::retire_fills(sim); }
    let now = sim.time_days;
    sim.work.piles.spoil(now);
    {
        let _p = crate::prof::scope("step/agents/nav refresh");
        let mut nav = std::mem::replace(&mut sim.nav, NavGrid::new(1, 1));
        nav.refresh(sim);
        sim.nav = nav;
    }
    measure_shore(sim);
    let sky = sim.water.sky.at(now);
    sim.agents.holed_up = if sky.bitter() { true } else if sky.feels_c > crate::weather::COME_OUT_FEELS_C { false } else { sim.agents.holed_up };
    { let _p = crate::prof::scope("step/agents/sync"); sync_agents(sim); }
    { let _p = crate::prof::scope("step/agents/tidy"); tidy(sim); }
    { let _p = crate::prof::scope("step/agents/retarget"); retarget(sim); }
    { let _p = crate::prof::scope("step/agents/maintainers"); send_maintainers(sim); }
    let ids: Vec<BeaverId> = sim.agents.by_id.keys().copied().collect();
    for id in ids {
        let Some(mut ag) = sim.agents.by_id.remove(&id) else { continue };
        {
            let _p = crate::prof::scope(act_timer(&ag.task));
            tick(sim, &mut ag, dt_days);
        }
        sim.agents.by_id.insert(id, ag);
    }
    {
        let _p = crate::prof::scope("step/agents/remains");
        // Bodies lie a few days; leavers walk on until they are off the map.
        sim.agents.remains.bodies.retain(|b| now - b.died < REMAINS_DAYS);
        let mut leaving = std::mem::take(&mut sim.agents.remains.leaving);
        leaving.retain_mut(|(ag, _)| {
            ag.trail.clear();
            if let Some(Assignment::Go(to)) = ag.assignment {
                go_to(sim, ag, dt_days, to);
            }
            let on = ag.assignment.is_some();
            if !on {
                sim.nav.release(ag.id, ag.cell);
            }
            on
        });
        leaving.append(&mut sim.agents.remains.leaving);
        sim.agents.remains.leaving = leaving;
    }
    let _p = crate::prof::scope("step/agents/finish sites");
    finish_sites(sim);
}

fn at_resting_place(ag: &Agent, home: Option<&Building>) -> bool {
    let _ = home;
    ag.inside
}

#[cfg(test)]
mod life_tests {
    use super::*;
    use crate::buildings::BuildingKind;

    /// A colony on a real map, settled, with a finished lodge.
    fn colony() -> SimState {
        let (mut s, site) = crate::starts::new_game_on_map("voyageurs").unwrap();
        s.apply(crate::Command::Settle { cell: site.camp }).unwrap();
        crate::sandbox::run_days(&mut s, 0.2);
        let near = site.camp;
        let pos = (4..40usize)
            .flat_map(|r| (0..=2 * r).flat_map(move |i| [(near.0 + i - r, near.1 - r), (near.0 + i - r, near.1 + r), (near.0 - r, near.1 + i - r), (near.0 + r, near.1 + i - r)]))
            .find(|&p| s.check_site(BuildingKind::Shelter, p).is_ok())
            .unwrap();
        let id = s.place_site(BuildingKind::Shelter, pos).unwrap();
        for &(g, a) in BuildingKind::Shelter.spec().cost {
            s.work.deliver(id, g, a).unwrap();
        }
        s.work.add_labour(id, BuildingKind::Shelter.spec().work).unwrap();
        crate::sandbox::run_days(&mut s, 0.2);
        assert!(s.buildings.building(id).unwrap().is_complete());
        s
    }

    /// In bitter cold the colony keeps to its lodge, and comes out again once it eases.
    #[test]
    fn in_bitter_cold_beavers_keep_to_their_lodges() {
        let mut s = colony();
        // The first spell of the place's winter bitter for half a day and more.
        let sky = s.water.sky;
        let bitter = (0..360 * 24).map(|h| s.time_days + h as f64 / 24.0).find(|&t| sky.at(t).bitter() && sky.at(t + 0.5).bitter()).expect("a bitter winter spell");
        // Skip ahead to the day before it.
        let start = bitter - 1.0;
        s.time_days = start;
        s.water.time_days = start;
        s.colony.time_days = start;
        // Everyone out cutting trees: not a source winter closes.
        let n = s.world.size;
        for a in s.agents.by_id.values_mut() {
            a.assignment = Some(Assignment::Area(ResourceKind::Tree, Area { min: (0, 0), max: (n, n) }));
        }
        let out = |s: &SimState| s.agents.by_id.values().filter(|a| !a.inside).count();
        crate::sandbox::run_days(&mut s, 0.4);
        assert!(!s.agents.holed_up && out(&s) > 0, "out at work before the cold comes");
        crate::sandbox::run_days(&mut s, 1.1);
        assert!(s.agents.holed_up, "holed up at {:?}", sky.at(s.time_days));
        // Still water has frozen over hard enough to walk on: it is ground now, not water.
        let frozen = (0..s.world.size * s.world.size).find(|&i| s.water.ice.as_slice()[i] >= crate::tuning::water::ICE_WALK_M && s.water.depth.as_slice()[i] > crate::buildings::WET_DEPTH && s.water.dam_height.as_slice()[i] <= 0.0).expect("ice");
        let n = s.world.size;
        assert_eq!(s.nav.surface_at((frozen % n, frozen / n)), Some(Surface::Land), "frozen water is walked");
        let (inside, all) = (s.agents.by_id.values().filter(|a| a.inside).count(), s.agents.by_id.len());
        assert!(inside * 10 >= all * 8, "{inside} of {all} in their lodge at {:?}", sky.at(s.time_days));
        let eased = (0..60 * 24).map(|h| s.time_days + h as f64 / 24.0).find(|&t| sky.at(t).feels_c > crate::weather::COME_OUT_FEELS_C + 3.0).unwrap();
        let wait = eased - s.time_days + 0.5;
        crate::sandbox::run_days(&mut s, wait);
        assert!(!s.agents.holed_up && out(&s) > 0, "out again once it eases");
    }

    fn on_edge(s: &SimState, c: Cell) -> bool {
        let n = s.nav.surface.width();
        c.0 == 0 || c.1 == 0 || c.0 == n - 1 || c.1 == n - 1
    }

    #[test]
    fn a_dead_beaver_leaves_a_body_for_a_while() {
        let mut s = colony();
        let a = s.agents.by_id.values().map(|a| (a.id, a.cell, a.inside)).next().unwrap();
        assert!(s.colony.kill(a.0, crate::colony::DeathCause::Predator));
        crate::sandbox::run_days(&mut s, crate::STEP_DAYS as f64);
        let body = s.agents.remains.bodies.iter().find(|b| b.id == a.0).expect("a body");
        if a.2 {
            // One that died in its lodge is laid outside, close by.
            assert_ne!(s.nav.surface_at(body.cell), Some(Surface::LodgeInside), "laid out of the lodge");
            assert!(body.cell.0.abs_diff(a.1.0).max(body.cell.1.abs_diff(a.1.1)) <= 4, "beside the lodge");
        } else {
            assert_eq!(body.cell, a.1, "it lies where the beaver fell");
        }
        assert_eq!(body.cause, crate::colony::DeathCause::Predator);
        crate::sandbox::run_days(&mut s, REMAINS_DAYS + 0.1);
        assert!(s.agents.remains.bodies.iter().all(|b| b.id != a.0), "gone after a few days");
    }

    #[test]
    fn a_beaver_that_leaves_walks_off_the_map() {
        let mut s = colony();
        let id = s.colony.beavers.iter().find(|b| b.stage == crate::colony::Stage::Adult).unwrap().id;
        let from = s.agents.get(id).unwrap().cell;
        s.colony.depart(id);
        crate::sandbox::run_days(&mut s, crate::STEP_DAYS as f64);
        let (ag, _) = s.agents.remains.leaving.iter().find(|(a, _)| a.id == id).expect("on its way");
        let Some(Assignment::Go(to)) = ag.assignment else { panic!("not heading off") };
        assert!(on_edge(&s, to), "heading for the edge: {to:?}");
        crate::sandbox::run_days(&mut s, 0.3);
        if let Some((a, _)) = s.agents.remains.leaving.iter().find(|(a, _)| a.id == id) {
            assert_ne!(a.cell, from, "it walks");
        }
        crate::sandbox::run_days(&mut s, 20.0);
        assert!(s.agents.remains.leaving.iter().all(|(a, _)| a.id != id), "off the map in the end");
    }

    #[test]
    fn a_newcomer_walks_in_from_the_edge() {
        let mut s = colony();
        // An empty lodge for it to come to.
        let home = s.agents.by_id.values().next().unwrap().cell;
        s.colony.add_home((home.0 as f32 + 0.5, home.1 as f32 + 0.5));
        let id = s.colony.admit_wanderer(true).unwrap();
        crate::sandbox::run_days(&mut s, crate::STEP_DAYS as f64);
        let a = s.agents.get(id).expect("it arrives");
        let n = s.nav.surface.width();
        let edge_gap = a.cell.0.min(a.cell.1).min(n - 1 - a.cell.0).min(n - 1 - a.cell.1);
        assert!(edge_gap <= 2, "it comes in at the edge: {:?}", a.cell);
        assert!(matches!(a.assignment, Some(Assignment::Go(_))));
    }

    #[test]
    fn a_new_pair_meets_at_its_lodge_then_goes_back_to_work() {
        let mut s = colony();
        let adults: Vec<BeaverId> = s.colony.beavers.iter().filter(|b| b.stage == crate::colony::Stage::Adult).map(|b| b.id).take(2).collect();
        let (a, b) = (adults[0], adults[1]);
        for (who, partner) in [(a, b), (b, a)] {
            s.agents.courting.insert(who, Courtship { partner, started: s.time_days, met: None });
        }
        let mut met = false;
        for _ in 0..400 {
            crate::sandbox::run_days(&mut s, crate::STEP_DAYS as f64);
            if s.agents.courting.get(&a).is_some_and(|c| c.met.is_some()) {
                met = true;
                break;
            }
        }
        assert!(met, "they never met");
        let (pa, pb) = (s.agents.get(a).unwrap().cell, s.agents.get(b).unwrap().cell);
        assert!(pa.0.abs_diff(pb.0).max(pa.1.abs_diff(pb.1)) <= 3, "together: {pa:?} {pb:?}");
        crate::sandbox::run_days(&mut s, COURT_DAYS + 0.05);
        assert!(!s.agents.courting.contains_key(&a) && !s.agents.courting.contains_key(&b), "back to work");
    }

    #[test]
    fn a_kit_tags_along_after_its_parent() {
        let mut s = colony();
        let mother = s.colony.beavers.iter().find(|b| b.stage == crate::colony::Stage::Adult && b.sex == crate::colony::Sex::Female).unwrap().id;
        let kit = s.colony.beavers.iter().find(|b| b.id != mother && b.stage != crate::colony::Stage::Adult).map(|b| b.id).unwrap();
        if let Some(b) = s.colony.beavers.iter_mut().find(|b| b.id == kit) {
            b.age_days = 30.0;
            b.stage = crate::colony::Stage::Kit;
            b.mother = Some(mother);
            b.resting = false;
            b.needs.rest = 1.0;
        }
        // The mother works out near home.
        let home = s.agents.get(mother).unwrap().cell;
        let spot = (3..10usize)
            .flat_map(|r| (0..=2 * r).flat_map(move |i| [(home.0 + i - r, home.1 - r), (home.0 + i - r, home.1 + r), (home.0 - r, home.1 + i - r), (home.0 + r, home.1 + i - r)]))
            .find(|&c| s.nav.surface_at(c) == Some(Surface::Land) && s.nav.may_reach(home, [c]))
            .unwrap();
        s.apply(crate::Command::Assign { who: vec![mother], to: Some(Assignment::Go(spot)) }).unwrap();
        // While the mother is out, the kit keeps beside her.
        let (mut out, mut beside) = (0, 0);
        for _ in 0..30 {
            crate::sandbox::run_days(&mut s, 0.01);
            let (m, k) = (s.agents.get(mother).unwrap(), s.agents.get(kit).unwrap());
            if !m.inside && m.cell.0.abs_diff(home.0).max(m.cell.1.abs_diff(home.1)) >= 3 {
                out += 1;
                beside += (!k.inside && m.cell.0.abs_diff(k.cell.0).max(m.cell.1.abs_diff(k.cell.1)) <= 2) as usize;
            }
        }
        assert!(out >= 3, "the mother never went out");
        assert!(beside * 3 >= out * 2, "the kit was beside its mother {beside} of {out} times");
    }
}

#[cfg(test)]
mod current_tests {
    use super::*;
    use crate::nav::{Goal, SWIM_COST, SWIM_SPEED_M_S};

    /// A channel one cell wide along row 1, walled in, with `drift` m/s of current along x.
    fn channel(len: usize, drift: f32) -> NavGrid {
        let mut nav = NavGrid::new(len, 3);
        for x in 0..len {
            nav.set((x, 0), Surface::Solid);
            nav.set((x, 2), Surface::Solid);
            nav.set((x, 1), Surface::Water);
            if drift != 0.0 {
                nav.set_current((x, 1), (drift, 0.0));
            }
        }
        nav
    }

    /// Days a beaver takes to swim the route from `from` to `to`, in steps of `dt`.
    fn swim_days(nav: &mut NavGrid, from: Cell, to: Cell) -> f32 {
        let mut agents = Agents::default();
        agents.spawn(BeaverId(1), from);
        let mut ag = agents.by_id.remove(&BeaverId(1)).unwrap();
        ag.path = nav.find_path(ag.id, from, Goal::Cell(to));
        assert!(ag.path.is_some());
        let dt = 1.0e-4;
        let mut days = 0.0;
        while ag.cell != to {
            follow(nav, &mut ag, dt);
            days += dt;
            assert!(days < 10.0, "never got there");
        }
        days
    }

    #[test]
    fn a_beaver_swims_faster_downstream_than_upstream_by_the_current() {
        // A current half a beaver's swimming speed, flowing east.
        let drift = 0.5 * SWIM_SPEED_M_S;
        let mut nav = channel(64, drift);
        let swim = MOVE_COST_PER_DAY / SWIM_COST;
        let cells = 50.0;
        let down = cells / swim_days(&mut nav, (5, 1), (55, 1));
        let up = cells / swim_days(&mut nav, (55, 1), (5, 1));
        // Its own speed plus the current's going down, less it coming up.
        assert!((down / swim - 1.5).abs() < 0.02, "downstream {down} cells a day, still water {swim}");
        assert!((up / swim - 0.5).abs() < 0.02, "upstream {up} cells a day, still water {swim}");
        // Against a current faster than it swims, it still crawls up at the floor.
        let mut flood = channel(8, 2.0 * SWIM_SPEED_M_S);
        let cost = flood.move_cost((4, 1), (3, 1), 1.0).unwrap();
        assert!((cost - SWIM_COST / crate::nav::MIN_SWIM_SHARE).abs() < 1e-4, "{cost}");
        assert!(swim_days(&mut flood, (6, 1), (1, 1)).is_finite());
    }

    #[test]
    fn still_water_is_swum_as_before() {
        let mut nav = channel(64, 0.0);
        let path = nav.find_path(BeaverId(1), (5, 1), Goal::Cell((55, 1))).unwrap();
        assert!((path.cost - 50.0 * SWIM_COST).abs() < 1e-3, "{}", path.cost);
        let speed = 50.0 / swim_days(&mut nav, (5, 1), (55, 1));
        assert!((speed / (MOVE_COST_PER_DAY / SWIM_COST) - 1.0).abs() < 0.02, "{speed}");
        for s in [Surface::Land, Surface::Water, Surface::Canal, Surface::Dam, Surface::Thicket] {
            nav.set((10, 1), s);
            let base = NavGrid::step_cost(s).unwrap().0;
            assert_eq!(nav.move_cost((9, 1), (10, 1), 1.0), Some(base));
            assert_eq!(nav.move_cost((11, 1), (10, 1), 0.5), Some(base));
        }
        // A current across a step neither helps nor hinders it.
        let mut across = NavGrid::new(4, 4);
        across.set((1, 1), Surface::Water);
        across.set_current((1, 1), (0.0, 0.6));
        assert_eq!(across.move_cost((0, 1), (1, 1), 1.0), Some(SWIM_COST));
    }

    #[test]
    fn a_route_down_the_current_beats_a_shorter_one_against_it() {
        // Two channels from (2, 2) to (20, 2): straight along row 2 against the current, or
        // round by row 0 with it, four cells longer.
        let mut nav = NavGrid::new(23, 3);
        for x in 0..23 {
            nav.set((x, 0), Surface::Water);
            nav.set((x, 1), if x == 2 || x == 20 { Surface::Water } else { Surface::Solid });
            nav.set((x, 2), Surface::Water);
        }
        let who = BeaverId(1);
        let still = nav.find_path(who, (2, 2), Goal::Cell((20, 2))).unwrap();
        assert!(still.steps.iter().all(|s| s.0 .1 == 2), "in still water the short way: {:?}", still.steps);
        let drift = 0.5 * SWIM_SPEED_M_S;
        for x in 0..23 {
            nav.set_current((x, 0), (drift, 0.0));
            nav.set_current((x, 2), (-drift, 0.0));
        }
        let path = nav.find_path(who, (2, 2), Goal::Cell((20, 2))).unwrap();
        assert!(path.steps.iter().any(|s| s.0 .1 == 0), "the long way, with the current: {:?}", path.steps);
        assert!(path.steps.len() > still.steps.len());
        // Four still cells across, eighteen at half again a swimmer's speed.
        let quick = 4.0 * SWIM_COST + 18.0 * SWIM_COST / 1.5;
        assert!((path.cost - quick).abs() < 1e-2, "{} vs {quick}", path.cost);
        assert!(path.cost < 18.0 * SWIM_COST / 0.5);
    }
}
