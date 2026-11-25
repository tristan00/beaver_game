include!(concat!(env!("OUT_DIR"), "/agents_constants.rs"));
use std::collections::BTreeMap;

use crate::buildings::{PerId, Building, BuildingId, BuildingKind, Good};
use crate::colony::BeaverId;
use crate::nav::{Gait, Goal, NavGrid, Path, Surface};
use crate::resources::{Behaviour, Resource, ResourceId, ResourceKind};
use crate::water::Cell;
use crate::content::Duty as JobDuty;
use crate::jobs::Job;
use crate::work::{Area, EPS as WORK_EPS, LABOUR_PER_DAY};
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

/// The most of a good one beaver carries at once.
pub fn load_size(good: Good) -> f32 {
    good.row().load
}

impl Load {
    pub fn new(good: Good, amount: f32, booked: bool) -> Self {
        Load { good, amount, booked, species: None }
    }
    /// Its kind in a store: saplings by species, every other good as one.
    pub fn kind(&self) -> crate::buildings::Kind {
        self.species.filter(|_| self.good.row().carries_species)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum Assignment {
    Resource(ResourceId),
    Area(ResourceKind, Area),
    Slot(BuildingId, usize),
    Go(Cell),
    /// Works as a mender: goes wherever repair is most needed, without a seat.
    Mend,
    /// Works as a builder: goes to any active build site, dam or canal, without a seat.
    Build,
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
    /// Take a sapling from a food cache and plant it on one cell.
    Plant { store: BuildingId, cell: Cell, progress: f32 },
    /// Walk to goods lying on the ground and pick up a load of them for work at `then`.
    Pick { pile: u64, then: BuildingId },
    /// Walk to food lying on the ground and eat there.
    Eat { pile: u64 },
    /// Mend a worn building in place, using the materials brought to it.
    Mend { building: BuildingId },
    /// Collect seedlings from a grown tree, to carry to a forester's store.
    Seed { tree: ResourceId, progress: f32 },
    /// Keep watch beside a lookout; a sentry leaves its post only to eat.
    Watch { lookout: BuildingId },
    /// A hauler's raft trip (see `raft landing`): to the raft (leg 0), push it to `from` (1), load
    /// (2), push it to `to` (3), unload (4), take it back to its raft landing (5), let it go (6).
    Raft { raft: u32, from: BuildingId, to: BuildingId, good: Good, leg: u8 },
    /// Bring a load of `good` to work done in place (a gate, a lock, a building being mended;
    /// see `supply`): from the store `from`, or from goods on the ground (`pile`), or the load
    /// in hand; then leave it there.
    Supply { spot: crate::supply::Spot, good: Good, from: BuildingId, pile: Option<u64> },
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
    /// The cells it has moved into over its last steps, each with the time the step that took
    /// it there ended: for drawing it along its real route when the interface sees the world
    /// only every few steps (at speed). Not saved.
    #[serde(skip)]
    pub history: std::collections::VecDeque<(f64, Cell, Gait)>,
    pub next_work_probe: f64,
    pub next_home_probe: f64,
    /// When a beaver whose seat or area gave it nothing next looks for another (`retarget`).
    #[serde(skip)]
    pub next_retarget: f64,
    /// When a hungry beaver next looks for food lying on the ground.
    #[serde(default)]
    pub next_meal_probe: f64,
    /// The source a gatherer is working: it goes back to it after each load until it is used
    /// up, and only picks another when something else (fleeing, eating, resting, an order)
    /// comes between.
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
    /// Where each mender is working now. Chosen again every few steps, so it is not saved.
    #[serde(skip)]
    pub maint_targets: BTreeMap<BeaverId, BuildingId>,
    /// Where each free-roaming builder is working now. Chosen again every few steps, so it is not saved.
    #[serde(skip)]
    pub build_targets: BTreeMap<BeaverId, BuildingId>,
    /// Cells from each cell to the nearest water, and the day it was measured (see `shore`).
    #[serde(skip)]
    pub(crate) shore: (f64, Vec<u16>),
    /// Bodies and leavers.
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
                history: Default::default(),
                next_work_probe: 0.0,
                next_home_probe: 0.0,
                next_retarget: 0.0,
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
                ag.next_retarget=0.0;
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

struct Gather {
    kinds: &'static [ResourceKind],
    area: Option<Area>,
    only: Option<ResourceId>,
    min_age: f32,
    /// The role the work counts for, whose distance from water limits it (None: a direct order).
    role: Option<crate::jobs::Job>,
    /// The ground frozen hard: no mud, mud or pebbles to dig, and the water plants under ice.
    frozen: bool,
}

/// Ground frozen too hard to dig, and still water iced over (see `weather::Now::ground_frozen`).
pub fn frozen(sim: &SimState) -> bool {
    !sim.sandbox && sim.water.sky.at(sim.time_days).ground_frozen()
}

/// Sources winter closes: dug from the ground or taken from under the water.
fn frozen_out(kind: ResourceKind) -> bool {
    kind.row().freezes
}

fn gatherable(r: &Resource) -> bool {
    match r.kind.behaviour() {
        Behaviour::Tree => crate::forestry::fellable(r),
        Behaviour::Felled => r.amount > 0.0 || r.branches > 0.0,
        // A living plant is picked to nothing (`worth_starting` decides whether to begin one).
        Behaviour::FoodPlant => r.amount >= crate::resources::PLANT_BARE,
        _ if r.kind.flowing() => r.amount >= crate::resources::RIVER_MUD_MIN_TAKE,
        _ => r.amount > 0.0,
    }
}

/// A fresh food plant is only worth setting out for with enough on it. A gatherer goes back
/// to its own source (`Agent::source`) whatever is left, and picks a plant bare, when it dies.
fn worth_starting(r: &Resource) -> bool {
    !r.kind.is_food_plant() || r.amount >= PLANT_PICK_MIN
}

pub(crate) fn gathering_available(sim: &SimState, assignment: Assignment) -> bool {
    Gather::resolve(sim, assignment).is_some_and(|job| sim.resources.items().iter().any(|r| job.permits(r) && worth_starting(r)))
}

fn kinds_for(k: ResourceKind) -> &'static [ResourceKind] {
    k.gather_sources()
}

impl Gather {
    fn resolve(sim: &SimState, a: Assignment) -> Option<Gather> {
        match a {
            Assignment::Resource(id) => sim
                .resources
                .get(id)
                .filter(|r| gatherable(r) && tools_at_hand(sim, r.kind))
                .map(|_| Gather { kinds: &[], area: None, only: Some(id), min_age: 0.0, role: None, frozen: frozen(sim) }),
            Assignment::Area(kind, _) if !tools_at_hand(sim, kind) => None,
            Assignment::Area(kind, area) => Some(Gather { kinds: kinds_for(kind), area: Some(area), only: None, min_age: crate::forestry::cut_min_age(sim, None), role: Some(crate::jobs::gather_role(kind)), frozen: frozen(sim) }),
            Assignment::Slot(b, slot) => {
                let place = sim.work.place(b)?;
                match (crate::jobs::gather_kind(place.slots.get(slot)?.role), place.area) {
                    (Some(kind), Some(area)) => {
                        Some(Gather { kinds: kinds_for(kind), area: Some(area), only: None, min_age: crate::forestry::cut_min_age(sim, Some(b)), role: Some(crate::jobs::gather_role(kind)), frozen: frozen(sim) })
                    }
                    _ => None,
                }
            }
            Assignment::Go(_) | Assignment::Mend | Assignment::Build => None,
        }
    }
    fn forages(&self) -> bool {
        self.kinds.iter().any(|k| k.is_food_plant())
    }
    fn permits(&self, r: &Resource) -> bool {
        if self.only.is_some_and(|id|id!=r.id) || self.area.is_some_and(|a|!a.contains(r.cell)) || (self.frozen && frozen_out(r.kind)) {return false;}
        let wanted = if self.forages() && r.kind.is_felled() { r.bark > 0.0 } else { gatherable(r) };
        wanted
            && (self.only.is_some() || self.kinds.contains(&r.kind))
            && (!r.kind.is_tree() || crate::forestry::cuttable(r, self.min_age))
    }
    /// The good this job brings in, by its first kind of source.
    fn yield_good(&self) -> Option<Good> {
        if self.forages() {
            return Some(Good::food());
        }
        self.kinds.first()?.good()
    }

    /// The good a resource yields to this job: bark is food to a forager.
    fn good_of(&self, r: &Resource) -> Good {
        if r.kind.is_felled() && self.forages() { Good::food() } else { r.kind.good().expect("a gathered kind yields a good") }
    }

    /// The source a beaver at `from` should go to: the shortest round of walking there and
    /// carrying the load to where it is dropped off, among sources not already worked by as
    /// many beavers as they take. Kinds earlier in the job's list come first (felled trees
    /// before standing ones); land and water plants count as one kind, and so do river mud
    /// and mud banks.
    fn pick(&self, sim: &SimState, from: Cell) -> Option<ResourceId> {
        let rank = |r: &Resource| {
            let kind = r.kind.row().alike.unwrap_or(r.kind);
            self.kinds.iter().position(|&k| k == kind).unwrap_or(0)
        };
        let reachable = |r: &Resource| {
            sim.agents.unreachable.get(&r.id).is_none_or(|until| sim.time_days >= *until) && sim.nav.may_reach_beside(from, r.cell)
        };
        let claims = claims(sim);
        let full = |r: &Resource| self.only.is_none() && claims.get(&r.id).copied().unwrap_or(0) >= gatherers_per(r.kind);
        let mut drops: PerId<Good, Option<Vec<Cell>>> = PerId::default();
        // Nothing is fetched for a full store; a tree felled on a direct order still comes down.
        let mut wanted: PerId<Good, Option<bool>> = PerId::default();
        // Connectivity is much more expensive than the walk; a source whose walk alone
        // cannot beat the current choice needs neither its haul nor a route test.
        let mut best: Option<((usize, u32, ResourceId), ResourceId)> = None;
        let mut consider = |r: &Resource| {
            if !self.permits(r) || !worth_starting(r) || full(r) {return;}
            if self.role.is_some_and(|role| !within_shore(sim, role, r.cell)) {return;}
            let walk = cell_dist(from, r.cell);
            if best.as_ref().is_some_and(|((k, c, _), _)| (rank(r), trip_key(walk)) > (*k, *c)) {return;}
            let good = self.good_of(r);
            let ordered_felling = self.only.is_some() && r.kind.is_tree();
            if !ordered_felling && !*wanted[good].get_or_insert_with(|| worth_fetching(sim, from, good)) {return;}
            let spots = drops[good].get_or_insert_with(|| drop_spots(sim, good, from));
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
    kind.row().gatherers
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
    !kind.spec().stores.is_empty()
}

pub fn accepts(kind: BuildingKind, good: Good) -> bool {
    kind.spec().stores.contains(&good)
}

/// Collecting pebbles needs the tools kept at a finished pebble heap.
/// Whether a role's beavers may be sent to `c`: within the player's limit of cells from water.
pub fn within_shore(sim: &SimState, role: crate::jobs::Job, c: Cell) -> bool {
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

/// The tools a kind of resource needs (`needs`) are kept at a finished building of their kind.
pub fn tools_at_hand(sim: &SimState, kind: ResourceKind) -> bool {
    kind.row().needs.is_none_or(|n| sim.buildings.buildings.iter().any(|b| b.kind == n && b.is_complete()))
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

pub(crate) fn has_lodge(sim: &SimState) -> bool {
    sim.buildings.buildings.iter().any(|b| b.kind.is_home() && b.is_complete())
}

fn home_of(sim: &SimState, who: BeaverId) -> Option<&Building> {
    let home = sim.colony.beaver(who)?.lodge?;
    let (&b, _) = sim.work.homes.iter().find(|(_, h)| **h == home)?;
    sim.buildings.building(b).filter(|b| b.is_complete())
}

fn route_home(sim: &SimState, who: BeaverId, from: Cell) -> Option<Path> {
    let cells = |b: &Building| b.cells().collect::<Vec<_>>();
    match home_of(sim, who) {
        Some(b) if b.kind.is_home() => sim.nav.find_path_onto_any(who, from, &cells(b)),
        Some(b) => sim.nav.find_path_beside_any(who, from, &cells(b)),
        None => sim
            .buildings
            .buildings
            .iter()
            .filter(|b| b.kind.is_home() && b.is_complete())
            .filter_map(|b| sim.nav.find_path_onto_any(who, from, &cells(b)))
            .min_by(|a, b| a.cost.total_cmp(&b.cost)),
    }
}

/// How fast a beaver moves for what it carries and where it is: full pace unladen, slower with a
/// load, slowest dragging wood over dry ground. A load floating in a canal deep enough to swim
/// costs nothing.
pub fn pace(nav: &NavGrid, ag: &Agent) -> f32 {
    let Some(load) = ag.load.as_ref() else { return 1.0 };
    let good = load.good.row();
    let surface = nav.surface_at(ag.cell);
    if good.floats && surface == Some(Surface::Canal) && nav.floats(ag.cell) {
        return 1.0;
    }
    let dry = !matches!(surface, Some(Surface::Water | Surface::LodgeEntrance | Surface::Canal | Surface::Lock));
    if dry { good.land_pace } else { good.pace }
}

/// How fast a beaver goes, as a share of a walk (or swim) unladen: `pace`, or a raft's when
/// it is pushing one.
pub fn travel_pace(nav: &NavGrid, ag: &Agent) -> f32 {
    match ag.task {
        Task::Raft { leg: 1 | 3 | 5, .. } => crate::raft_landing::RAFT_PACE,
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
    Collect(Gather),
    Build(BuildingId),
    Haul,
    /// A planter's work in its forester's area: collect seedlings and plant them.
    Forest(Area),
    Mend(BuildingId),
    /// A sentry's watch at its lookout.
    Sentry(BuildingId),
}

fn duty_of(sim: &SimState, ag: &Agent, can_work: bool) -> Duty {
    if !can_work {
        return Duty::None;
    }
    match sim.work.slot_of(ag.id) {
        Some((b, _, r)) if r.duty() == JobDuty::Build && sim.work.place(b).is_some_and(|p| p.site.is_some() || (!p.paused && (crate::dams::has_work_for(sim, b, false) || crate::canals::has_work_for(sim, b, false)))) => Duty::Build(b),
        Some((_, _, r)) if r.duty() == JobDuty::Haul => Duty::Haul,
        Some((b, _, r)) if r.duty() == JobDuty::Watch && sim.buildings.building(b).is_some_and(|x| x.is_complete()) => Duty::Sentry(b),
        Some((b, _, r)) if r.duty() == JobDuty::Forest => sim.work.place(b).and_then(|p| p.area).map_or(Duty::None, Duty::Forest),
        Some((b, i, r)) if r.duty() == JobDuty::Gather => Gather::resolve(sim, Assignment::Slot(b, i)).map_or(Duty::None, Duty::Collect),
        Some(_) => Duty::None,
        None if ag.assignment == Some(Assignment::Mend) => match sim.agents.maint_targets.get(&ag.id) {
            Some(&b) if sim.buildings.building(b).is_some_and(|x| crate::maintenance::needs_mending(sim, x)) => Duty::Mend(b),
            Some(&b) => Duty::Build(b),
            None => Duty::None,
        },
        None if ag.assignment == Some(Assignment::Build) => match sim.agents.build_targets.get(&ag.id) {
            Some(&b) if sim.work.place(b).is_some_and(|p| p.site.is_some() || (!p.paused && (crate::dams::has_work_for(sim, b, false) || crate::canals::has_work_for(sim, b, false)))) => Duty::Build(b),
            _ => Duty::None,
        },
        None => ag.assignment.and_then(|a| Gather::resolve(sim, a)).map_or(Duty::None, Duty::Collect),
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
/// A mender's dam or canal work is upkeep; a builder's is new work.
fn upkeep(sim: &SimState, who: BeaverId) -> bool {
    sim.agents.maint_targets.contains_key(&who)
}

/// The skill dam, canal and building work draws on: a mender's when it is upkeep, a builder's
/// when it is new work.
fn works_job(sim: &SimState, who: BeaverId) -> crate::colony::Job {
    if upkeep(sim, who) || is_mender(sim, who) { crate::jobs::Job::with_duty(crate::content::Duty::Mend) } else { crate::jobs::Job::with_duty(crate::content::Duty::Build) }
}

fn canal_task(sim: &SimState, site: BuildingId, ag: &Agent) -> Option<Task> {
    let taken = crate::canals::claimed(sim, site, ag.id);
    let job = crate::canals::next_job(sim, site, &taken, upkeep(sim, ag.id));
    if let Some((cell, true)) = job {
        return Some(Task::Gate { site, cell, progress: 0.0 });
    }
    if !upkeep(sim, ag.id) && let Some(t) = supply_task(sim, ag, &gate_spots(sim, site)) {
        return Some(t);
    }
    let (cell, _) = job?;
    if frozen(sim) {
        return None;
    }
    Some(Task::Dig { site, cell, progress: 0.0 })
}

/// The gates of a dam or canal, where their materials are brought.
fn gate_spots(sim: &SimState, site: BuildingId) -> Vec<crate::supply::Spot> {
    let cells: Vec<Cell> = match (crate::dams::dam(sim, site), crate::canals::canal(sim, site)) {
        (Some(d), _) => d.gates.iter().map(|g| g.cell).collect(),
        (_, Some(k)) => vec![k.gate.cell],
        _ => Vec::new(),
    };
    cells.into_iter().map(|cell| crate::supply::Spot { building: site, cell }).collect()
}

/// What others are bringing of a good to a spot: loads in hand, and a load each for those on
/// their way to fetch one.
fn supply_promised(sim: &SimState, spot: crate::supply::Spot, good: Good, except: BeaverId) -> f32 {
    sim.agents
        .by_id
        .values()
        .filter(|a| a.id != except)
        .map(|a| match a.task {
            Task::Supply { spot: s, good: g, .. } if s == spot && g == good => a.load.map_or(load_size(good), |l| l.amount),
            _ => 0.0,
        })
        .sum()
}

/// What a spot still wants brought of a good, beyond what others are bringing.
fn supply_outstanding(sim: &SimState, spot: crate::supply::Spot, good: Good, who: BeaverId) -> f32 {
    crate::supply::missing(sim, spot, good) - supply_promised(sim, spot, good, who)
}

fn spot_cells(sim: &SimState, spot: crate::supply::Spot) -> Vec<Cell> {
    if is_dam(sim, spot.building) || is_canal(sim, spot.building) {
        vec![spot.cell]
    } else {
        place_cells(sim, spot.building)
    }
}

/// A load to bring to the first of `spots` that still wants one: the load in hand if it is the
/// good wanted, else from the nearest store or goods on the ground. None when nothing wanted
/// can be had.
fn supply_task(sim: &SimState, ag: &Agent, spots: &[crate::supply::Spot]) -> Option<Task> {
    for &spot in spots {
        for (good, _) in crate::supply::needs(sim, spot) {
            let outstanding = supply_outstanding(sim, spot, good, ag.id);
            if outstanding <= WORK_EPS {
                continue;
            }
            if ag.load.is_some_and(|l| l.good == good) {
                return Some(Task::Supply { spot, good, from: NO_STORE, pile: None });
            }
            if ag.load.is_some() {
                continue;
            }
            match fetch_store(sim, ag, &spot_cells(sim, spot), good, outstanding.min(load_size(good))) {
                Some(Source::Store(from)) => return Some(Task::Supply { spot, good, from, pile: None }),
                Some(Source::Pile(pile)) => return Some(Task::Supply { spot, good, from: NO_STORE, pile: Some(pile) }),
                None => {}
            }
        }
    }
    None
}

/// A dam builder carries one load to the lowest unfinished cell. A gate ready to be built or
/// turned comes first, then materials its gates and locks still want.
fn dam_task(sim: &SimState, dam: BuildingId, ag: &Agent) -> Option<Task> {
    if let Some(cell) = crate::dams::gate_job(sim, dam, &crate::dams::gates_claimed(sim, dam, ag.id)) {
        return Some(Task::Gate { site: dam, cell, progress: 0.0 });
    }
    if !upkeep(sim, ag.id) && let Some(t) = supply_task(sim, ag, &gate_spots(sim, dam)) {
        return Some(t);
    }
    let taken = crate::dams::claimed(sim, dam, ag.id);
    let cell = crate::dams::next_cell(sim, dam, ag.cell, &taken, upkeep(sim, ag.id))?;
    let material = crate::dams::material();
    if ag.load.is_some_and(|l| l.good == material) {
        return Some(Task::DamWork { dam, cell, progress: 0.0 });
    }
    let want = crate::dams::sticks_per_step().min(load_size(material));
    Some(match fetch_store(sim, ag, &[cell], material, want)? {
        Source::Store(from) => Task::Fetch { from, good: material, then: dam, site_delivery: true },
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
        if older_wants && in_stock < SITE_RESERVE_LOADS * load_size(good) {
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

/// A haul between two stores goes by raft when there are at least two loads to move, raft
/// water joins the two, and a raft is free; otherwise as it is, a load at a time.
fn by_raft(sim: &SimState, ag: &Agent, t: Task) -> Task {
    let Task::Fetch { from, good, then, site_delivery: false } = t else { return t };
    if from == SUPPLIES || then == SUPPLIES || sim.fleet.rafts.is_empty() || sim.work.stock_of(from, good) < 2.0 * load_size(good) {
        return t;
    }
    let Some(raft) = crate::raft_landing::free_raft(sim, ag.cell) else { return t };
    let raft_landing = sim.fleet.get(raft).map(|b| b.raft_landing);
    if !raft_landing.is_some_and(|h| crate::raft_landing::joined(sim, h, from)) || !crate::raft_landing::joined(sim, from, then) {
        return t;
    }
    Task::Raft { raft, from, to: then, good, leg: 0 }
}

/// One turn of a raft trip (see `Task::Raft`).
#[allow(clippy::too_many_arguments)]
fn raft_trip(sim: &mut SimState, ag: &mut Agent, dt: f32, duty: &Duty, raft: u32, from: BuildingId, to: BuildingId, good: Good, leg: u8) {
    use crate::raft_landing;
    let let_go = |sim: &mut SimState, ag: &mut Agent| {
        let at = ag.cell;
        if let Some(b) = sim.fleet.get_mut(raft).filter(|b| b.hauler == Some(ag.id)) {
            b.hauler = None;
            if raft_landing::raft_water(sim, at) {
                if let Some(b) = sim.fleet.get_mut(raft) {
                    b.cell = at;
                }
            }
        }
        ag.path = None;
        ag.task = if ag.load.is_some() { Task::Carry { dest: NO_STORE } } else { Task::Idle };
    };
    let Some(b) = sim.fleet.get(raft).copied() else { return let_go(sim, ag) };
    if !matches!(duty, Duty::Haul) || b.hauler.is_some_and(|h| h != ag.id) {
        return let_go(sim, ag);
    }
    let docks_of = |sim: &SimState, id: BuildingId| sim.buildings.building(id).map(|x| raft_landing::docks(sim, x)).unwrap_or_default();
    let next = |ag: &mut Agent, leg: u8| {
        ag.path = None;
        ag.task = Task::Raft { raft, from, to, good, leg };
    };
    match leg {
        // To the raft, and on board.
        0 => {
            if let Some(x) = sim.fleet.get_mut(raft) {
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
        // Pushing it: to the first store, to the second, home to its raft landing.
        1 | 3 | 5 => {
            let target = docks_of(sim, match leg { 1 => from, 3 => to, _ => b.raft_landing });
            if target.contains(&ag.cell) {
                return next(ag, leg + 1);
            }
            // Water that has iced over (or run too shallow) since the way was planned: plan
            // again, or tie the raft up where it is.
            if ag.path.as_ref().and_then(|p| p.steps.first()).is_some_and(|st| !raft_landing::raft_water(sim, st.0)) {
                ag.path = None;
            }
            if ag.path.is_none() {
                ag.path = raft_landing::route(sim, ag.cell, &target);
                if ag.path.is_none() {
                    return let_go(sim, ag);
                }
            }
            let moved = follow(&mut sim.nav, ag, dt);
            let at = ag.cell;
            if let Some(x) = sim.fleet.get_mut(raft) {
                x.cell = at;
            }
            note(ag, moved);
        }
        // Loading as much as the raft takes.
        2 => {
            let want = (raft_landing::RAFT_LOADS * load_size(good)).min(sim.work.stock_of(from, good));
            let (got, kind) = if want > WORK_EPS { sim.buildings.stores.take_kind_from(from, good, want) } else { (0.0, None) };
            log_load(sim, ag, "TAKE_FOR_RAFT", from, good, want, got, &format!("to={} raft={raft}", to.0));
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
                log_load(sim, ag, "DELIVER_RAFT", to, load.good, load.amount, taken, &format!("raft={raft}"));
                sim.run.raft_delivered(load.good, taken);
                sim.deeds.of(ag.id).rafted[load.good] += taken;
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
    let ordered: Vec<BuildingId> = sites.iter().map(|p| p.building).collect();
    ordered.into_iter().find_map(|b| site_fetch_task(sim, b, ag)).or_else(|| {
        let spots: Vec<crate::supply::Spot> = sim.work.places.iter().filter(|p| !p.paused).flat_map(|p| gate_spots(sim, p.building)).collect();
        supply_task(sim, ag, &spots)
    })
}

/// Whether a move between stores has somewhere to go: the stores hold no more of the good than
/// they have room for (they can, once flooding takes a store out of use). A hauler doesn't set
/// out with a load nothing can take.
fn room_for_move(sim: &SimState, t: &Task) -> bool {
    match t {
        Task::Fetch { good, site_delivery: false, .. } => sim.buildings.stores.amount(*good) <= sim.buildings.stores.capacity(*good) - WORK_EPS,
        _ => true,
    }
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
/// How often (in steps) beavers check that their target still has work for them.
const RETARGET_STEPS: u64 = 4;
/// How long a beaver whose seat or area gave it nothing, and found nowhere better, waits
/// before looking again, in days.
const RETARGET_WAIT_DAYS: f64 = 1.0;
/// How long a beaver waiting at home for work lets go by between looks for some, in days. A
/// new job or order looks at once.
const WORK_PROBE_DAYS: f64 = 1.0;

/// The nearest gatherable resource of `kind` (felled trees before standing ones, as gatherers
/// rank them) within `reach` of `from`.
fn next_resource(sim: &SimState, kind: ResourceKind, from: Cell, reach: usize) -> Option<ResourceId> {
    let area = Area { min: (from.0.saturating_sub(reach), from.1.saturating_sub(reach)), max: (from.0 + reach, from.1 + reach) };
    let job = Gather::resolve(sim, Assignment::Area(kind, area))?;
    job.pick(sim, from)
}

/// Whether a workplace still has work for a seat of this role: a site with labour or materials
/// to come, a dam or canal with cells to build or keep, or a gathering area with something left.
fn seat_has_work(sim: &SimState, b: BuildingId, role: Job) -> bool {
    let Some(p) = sim.work.place(b) else { return false };
    if p.paused {
        // Paused by the player: its beavers wait there.
        return true;
    }
    match role.duty() {
        JobDuty::Build => {
            if p.site.is_some() && is_site(sim, p) {
                return !site_stalled(sim, p);
            }
            crate::dams::has_work_for(sim, b, false) || crate::canals::has_work_for(sim, b, false)
        }
        JobDuty::Gather => p
            .slots
            .iter()
            .position(|s| s.role == role)
            .and_then(|i| Gather::resolve(sim, Assignment::Slot(b, i)))
            .is_some_and(|j| sim.resources.items().iter().any(|r| j.permits(r) && worth_starting(r))),
        _ => true,
    }
}

/// Already at work on this place (a step under way, a load on its way there): not blocked.
fn busy_here(ag: Option<&Agent>, b: BuildingId) -> bool {
    ag.is_some_and(|a| match a.task {
        Task::Build { site } | Task::Dig { site, .. } | Task::Gate { site, .. } => site == b,
        Task::DamWork { dam, .. } => dam == b,
        Task::Fetch { then, .. } | Task::Pick { then, .. } => then == b,
        _ => false,
    })
}

fn reaches_place(sim: &SimState, from: Cell, b: BuildingId) -> bool {
    place_cells(sim, b).iter().any(|&c| sim.nav.may_reach_beside(from, c))
}

/// A beaver whose target is done or out of reach goes on to the next one: a seated builder,
/// mender or gatherer to the nearest workplace of the same job with work and an open seat;
/// a gatherer sent to an area that has run out, to the nearest spot with more of the same.
fn retarget(sim: &mut SimState) {
    if sim.steps % RETARGET_STEPS != 0 {
        return;
    }
    let beavers: Vec<(BeaverId, Cell)> = sim
        .colony
        .beavers
        .iter()
        .filter(|b| b.can_work())
        .filter_map(|b| sim.agents.get(b.id).map(|a| (b.id, a.cell)))
        .collect();
    // A beaver its target failed looks again only a day later: the looks plan tasks and search
    // routes, and would find the same thing every few steps.
    let wait = |sim: &mut SimState, who: BeaverId| {
        if let Some(a) = sim.agents.by_id.get_mut(&who) {
            a.next_retarget = sim.time_days + RETARGET_WAIT_DAYS;
        }
    };
    for (who, at) in beavers {
        if sim.agents.get(who).is_some_and(|a| sim.time_days < a.next_retarget) {
            continue;
        }
        match sim.work.slot_of(who) {
            Some((from, i, role)) if matches!(role.duty(), JobDuty::Build | JobDuty::Gather) => {
                // A builder stays while its place gives it something to do; one blocked there
                // (nothing it can take, or out of reach) goes on to the next place that does.
                let ag = sim.agents.get(who);
                // Only a beaver left with nothing to do can be blocked: one waiting at home, or
                // with no home, idle. Idle between loads it is only choosing its next one.
                let homeless = !has_lodge(sim);
                if busy_here(ag, from) || ag.is_some_and(|a| !(a.task == Task::Rest || (homeless && a.task == Task::Idle))) {
                    continue;
                }
                let gives = |b: BuildingId| role.duty() != JobDuty::Build || ag.is_some_and(|a| build_task(sim, b, a).is_some());
                if seat_has_work(sim, from, role) && reaches_place(sim, at, from) && (sim.work.place(from).is_some_and(|p| p.paused) || gives(from)) {
                    // At home, it looks for work once a day (`WORK_PROBE_DAYS`): so does this.
                    if ag.is_some_and(|a| a.task == Task::Rest) {
                        wait(sim, who);
                    }
                    continue;
                }
                let kind = role;
                let d = |b: BuildingId| sim.buildings.building(b).map_or(usize::MAX, |bd| bd.pos.0.abs_diff(at.0).pow(2) + bd.pos.1.abs_diff(at.1).pow(2));
                let next = sim
                    .work
                    .places
                    .iter()
                    .filter(|p| p.building != from && !p.paused)
                    .filter(|p| sim.buildings.building(p.building).is_some_and(|b| (b.is_complete() || is_site(sim, p)) && !sim.buildings.unusable(b, &sim.water.depth)))
                    .filter(|p| role.duty() != JobDuty::Build || takes_builders(sim, p))
                    .filter_map(|p| {
                        // A finished dam or canal keeps no builder seats; work ordered on it
                        // (a gate, a lock) opens one.
                        let reopen = role.duty() == JobDuty::Build && takes_builders(sim, p) && !p.slots.iter().any(|s| s.role.duty() == JobDuty::Build);
                        let (j, r) = p.open_seat(|r| r == kind).or(reopen.then_some((p.slots.len(), crate::jobs::Job::with_duty(JobDuty::Build))))?;
                        Some((p, j, r))
                    })
                    .filter(|(p, _, r)| seat_has_work(sim, p.building, *r) && reaches_place(sim, at, p.building))
                    .map(|(p, j, _)| ((std::cmp::Reverse(p.priority), d(p.building), p.building), j))
                    .collect::<Vec<_>>();
                // Best first, planning a task (route searches) only until one gives work.
                let mut next = next;
                next.sort_unstable();
                let next = next.into_iter().find(|(k, _)| gives(k.2)).map(|(k, j)| (k.2, j));
                let Some((to, j)) = next else {
                    wait(sim, who);
                    continue;
                };
                // A reopened builder seat on a finished dam or canal: put it back first.
                if let Ok(p) = sim.work.place_mut(to) {
                    if j == p.slots.len() && !p.open_ended() {
                        p.slots.push(crate::work::Slot::empty(crate::jobs::Job::with_duty(JobDuty::Build)));
                    }
                }
                // Moving takes the new seat before leaving the old one (`assign` vacates it).
                let _ = i;
                if sim.work.assign(to, j, who).is_ok() {
                    if let Ok(p) = sim.work.place_mut(to) {
                        p.max_beavers = p.max_beavers.max(p.filled());
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
                let Some(r) = next_resource(sim, kind, at, GATHER_RADIUS).and_then(|id| sim.resources.get(id)) else {
                    wait(sim, who);
                    continue;
                };
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
fn own_source(sim: &SimState, j: &Gather, ag: &Agent) -> Option<Task> {
    let id = ag.source?;
    let r = sim.resources.get(id).filter(|r| j.permits(r))?;
    if !(j.only.is_some() && r.kind.is_tree()) && !worth_fetching(sim, ag.cell, j.good_of(r)) {
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

fn pile_task(sim: &SimState, j: &Gather, ag: &Agent) -> Option<Task> {
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
        Duty::Sentry(_) => "+choose work/sentry",
    });
    match duty {
        Duty::None => None,
        Duty::Collect(j) => own_source(sim, j, ag)
            .or_else(|| pile_task(sim, j, ag))
            .or_else(|| j.pick(sim, ag.cell).map(|t| Task::Collect { target: t, progress: 0.0 })),
        Duty::Build(site) => build_task(sim, *site, ag),
        Duty::Mend(b) if crate::maintenance::mend_ready(sim, *b) => Some(Task::Mend { building: *b }),
        Duty::Mend(b) => supply_task(sim, ag, &crate::supply::mend_spot(sim, *b).into_iter().collect::<Vec<_>>()),
        Duty::Sentry(b) => Some(Task::Watch { lookout: *b }),
        // Plant what the stores hold, else collect more seedlings while there is room.
        Duty::Forest(area) => plant_task(sim, *area, ag).or_else(|| seed_task(sim, *area, ag)),
        Duty::Haul => site_haul_task(sim, ag).or_else(|| haul_task(sim).filter(|t| room_for_move(sim, t)).map(|t| by_raft(sim, ag, t))).or_else(|| {
            let (from, then, good) = crate::stock::fill_move(sim)?;
            Some(Task::Fetch { from, good, then, site_delivery: false }).filter(|t| room_for_move(sim, t)).map(|t| by_raft(sim, ag, t))
        }).or_else(|| {
            let own = sim.work.slot_of(ag.id).map(|s| s.0).filter(|b| sim.work.balanced.contains(b))?;
            let (from, then, good) = crate::stock::balance_move(sim, Some(own))?;
            Some(Task::Fetch { from, good, then, site_delivery: false }).filter(|t| room_for_move(sim, t)).map(|t| by_raft(sim, ag, t))
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
    // A sentry rests at its post.
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
            let dam_load = (matches!(duty, Duty::Build(b) if is_dam(sim, b) && !sim.maintenance.jobs.contains_key(&b)) && ag.load.is_some_and(|l| l.good == crate::dams::material()))
                || (matches!(duty, Duty::Forest(..)) && ag.load.is_some_and(|l| l.good == Good::seedling()));
            let dam_next = if dam_load { next_task(sim, &duty, ag) } else { None };
            if let Some(t) = dam_next {
                ag.task = t;
            } else if ag.load.is_some() {
                ag.task = Task::Carry { dest: NO_STORE };
            } else if let Some(t) = next_task(sim, &duty, ag) {
                ag.task = t;
            } else if has_lodge(sim) {
                // Nothing to do: it waits at home, or where it is if already in a lodge.
                ag.task = Task::Rest;
            }
        }
        Task::Rest => {
            // Fit to work and finding none, it looks again a day later; tired or holed up, it
            // looks as soon as it can work again.
            let fit = can_work && !tired && !holed;
            let work = if !fit {
                ag.next_work_probe = 0.0;
                false
            } else if sim.time_days >= ag.next_work_probe {
                let found = next_task(sim, &duty, ag).is_some();
                if !found {
                    ag.next_work_probe = sim.time_days + WORK_PROBE_DAYS;
                }
                found
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
        Task::Raft { raft, from, to, good, leg } => raft_trip(sim, ag, dt, &duty, raft, from, to, good, leg),
        Task::Supply { spot, good, from, pile } => supply(sim, ag, dt, &duty, spot, good, from, pile),
        Task::Watch { lookout } => {
            if !matches!(duty, Duty::Sentry(b) if b == lookout) {
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
        crate::dams::sticks_per_step().min(load_size(crate::dams::material()))
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
    let Some(cell) = sim.work.piles.get(pile).filter(|p| p.good == Good::food()).map(|p| p.cell) else {
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
    if !hungry || sim.buildings.stores.amount(Good::food()) > WORK_EPS {
        return None;
    }
    ag.next_meal_probe = sim.time_days + crate::piles::MEAL_PROBE_DAYS;
    let food = sim.work.piles.items.iter().filter(|p| p.good == Good::food()).map(|p| (p.id, vec![p.cell]));
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
    let want = crate::dams::sticks_per_step().min(load_size(crate::dams::material())).min(sim.work.stock_of(from, good));
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
    let Some(load) = ag.load.filter(|l| l.good == crate::dams::material()) else {
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
    let progress = progress + dt * LABOUR_PER_DAY * beaver_effort(sim, ag, works_job(sim, ag.id));
    if progress + WORK_EPS < crate::dams::pack_labour() {
        ag.task = Task::DamWork { dam, cell, progress };
        return;
    }
    // A worn cell this far gone was about to give way: a leak stopped.
    let failing = crate::dams::built(sim, cell) > 0.0 && crate::dams::integrity(sim, cell) < crate::deeds::LEAK_INTEGRITY;
    let used = crate::dams::pack(sim, dam, cell, load.amount);
    log_load(sim, ag, "PACK_DAM", dam, load.good, load.amount, used, &format!("dam_cell=({},{})", cell.0, cell.1));
    if used > 0.0 {
        *sim.deeds.of(ag.id).carried.entry((crate::dams::material(), BuildingKind::dam())).or_default() += used;
        if upkeep(sim, ag.id) {
            sim.deeds.repaired(ag.id, dam, Some(BuildingKind::dam()), crate::dams::pack_labour());
            if failing {
                sim.deeds.of(ag.id).leaks += 1;
            }
        } else {
            sim.deeds.packed(ag.id, dam, crate::dams::pack_labour());
        }
    }
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
        crate::canals::gate_owner(sim, cell) == Some(site) && crate::gates::ready_at(sim, site, cell)
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
    let labour = dt * LABOUR_PER_DAY * beaver_effort(sim, ag, works_job(sim, ag.id));
    if gate {
        // The gate's labour is kept at the gate, as a site's is, and keeps pace with what has
        // been brought; the visit ends once the work catches up (see `still` above).
        match crate::gates::work(sim, site, cell, labour) {
            Ok(Some(_)) => {
                let kind = sim.buildings.building(site).map_or(BuildingKind::canal(), |b| b.kind);
                let need = crate::canals::GATE_WORK;
                if upkeep(sim, ag.id) {
                    sim.deeds.repaired(ag.id, site, Some(kind), need);
                } else {
                    sim.deeds.worked(ag.id, site, kind, need);
                    let rec = sim.deeds.of(ag.id);
                    rec.gates += 1;
                    rec.work(site, Some(kind)).loads += 1;
                }
                ag.task = Task::Idle;
            }
            // Caught up with what has been brought: off to fetch more, or to other work.
            Ok(None) if !crate::gates::ready_at(sim, site, cell) => ag.task = Task::Idle,
            Ok(None) => ag.task = Task::Gate { site, cell, progress: 0.0 },
            Err(_) => {
                note(ag, false);
                ag.task = Task::Idle;
            }
        }
        return;
    }
    let progress = progress + labour;
    let need = crate::canals::DIG_WORK;
    if progress + WORK_EPS < need {
        ag.task = Task::Dig { site, cell, progress };
        return;
    }
    let done = crate::canals::dig(sim, site, cell);
    if done.is_ok() {
        let kind = sim.buildings.building(site).map_or(BuildingKind::canal(), |b| b.kind);
        if upkeep(sim, ag.id) {
            sim.deeds.repaired(ag.id, site, Some(kind), need);
        } else {
            sim.deeds.worked(ag.id, site, kind, need);
            let rec = sim.deeds.of(ag.id);
            rec.canal_cells += 1;
            rec.work(site, Some(kind)).loads += 1;
        }
    }
    note(ag, done.is_ok());
    ag.task = Task::Idle;
}

/// A newly paired beaver's turn: to its lodge, wait there for its mate, court, then back to
/// work. Returns false once there is no courtship (never was, or over).
fn court(sim: &mut SimState, ag: &mut Agent, dt: f32) -> bool {
    let Some(c) = sim.agents.courting.get(&ag.id).copied() else { return false };
    let now = sim.time_days;
    let partner_on = sim.agents.courting.get(&c.partner).is_some_and(|p| p.partner == ag.id);
    // A sentry keeps its post: it leaves only to eat, so it does not go courting.
    let sentry = |id: BeaverId| sim.work.slot_of(id).is_some_and(|(_, _, r)| r.duty() == JobDuty::Watch);
    let over = c.met.is_some_and(|m| now >= m + COURT_DAYS) || (c.met.is_none() && now >= c.started + COURT_WAIT_DAYS) || !partner_on || sentry(ag.id) || sentry(c.partner);
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

pub(crate) fn beaver_effort(sim: &SimState, ag: &Agent, job: crate::colony::Job) -> f32 {
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
    let skill = b.skills[job];
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
        (n + 1, rate + LABOUR_PER_DAY * beaver_effort(sim, a, crate::jobs::Job::with_duty(crate::content::Duty::Build)))
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
    // A move between stores is given up once nothing could take the load at the other end.
    let room = site || room_for_move(sim, &Task::Fetch { from, good, then, site_delivery: false });
    if !still_wanted || !from_ok || !room {
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

/// One turn of bringing a load to work done in place (see `Task::Supply`): fetch it, carry it
/// there, leave it.
#[allow(clippy::too_many_arguments)]
fn supply(sim: &mut SimState, ag: &mut Agent, dt: f32, duty: &Duty, spot: crate::supply::Spot, good: Good, from: BuildingId, pile: Option<u64>) {
    let gate = is_dam(sim, spot.building) || is_canal(sim, spot.building);
    let ours = match duty {
        Duty::Build(b) | Duty::Mend(b) => *b == spot.building,
        Duty::Haul => gate,
        _ => false,
    };
    let outstanding = supply_outstanding(sim, spot, good, ag.id);
    if !ours || outstanding <= WORK_EPS || ag.load.is_some_and(|l| l.good != good) {
        return abandon(ag);
    }
    let Some(load) = ag.load else {
        let want = outstanding.min(load_size(good));
        if let Some(id) = pile {
            let Some(cell) = sim.work.piles.get(id).filter(|p| p.good == good).map(|p| p.cell) else { return abandon(ag) };
            match reach(sim, ag, dt, cell) {
                None => return abandon(ag),
                Some(false) => return,
                Some(true) => {}
            }
            let got = sim.work.piles.take(id, want);
            log_load(sim, ag, "TAKE_GROUND_FOR_SPOT", spot.building, good, want, got, &format!("pile={id}"));
            if got <= WORK_EPS {
                return abandon(ag);
            }
            ag.load = Some(Load::new(good, got, false));
        } else {
            let from_ok = if from == SUPPLIES { sim.work.supplies.is_some() } else { sim.buildings.building(from).is_some_and(|b| b.is_complete()) };
            if from == NO_STORE || !from_ok {
                return abandon(ag);
            }
            if !approach(sim, ag, dt, from) {
                return;
            }
            let want = want.min(sim.work.stock_of(from, good));
            let (got, kind) = if want > WORK_EPS { sim.buildings.stores.take_kind_from(from, good, want) } else { (0.0, None) };
            log_load(sim, ag, "TAKE_FOR_SPOT", from, good, want, got, &format!("spot={} ({},{})", spot.building.0, spot.cell.0, spot.cell.1));
            if got <= WORK_EPS {
                return abandon(ag);
            }
            ag.load = Some(Load { species: kind, ..Load::new(good, got, true) });
        }
        note(ag, true);
        ag.path = None;
        return;
    };
    let there = if gate {
        match reach(sim, ag, dt, spot.cell) {
            None => return abandon(ag),
            Some(arrived) => arrived,
        }
    } else {
        approach(sim, ag, dt, spot.building)
    };
    if !there {
        return;
    }
    let put = crate::supply::deposit(sim, spot, good, load.amount);
    log_load(sim, ag, "DELIVER_SPOT", spot.building, good, load.amount, put, &format!("cell=({},{})", spot.cell.0, spot.cell.1));
    if put > 0.0 {
        let kind = sim.buildings.building(spot.building).map_or(BuildingKind::dam(), |b| b.kind);
        let rec = sim.deeds.of(ag.id);
        *rec.carried.entry((good, kind)).or_default() += put;
        if load.booked {
            rec.hauled += put;
        }
        let source = producer(sim, ag);
        let use_ = if gate { crate::buildings::Use::Sites } else { crate::buildings::Use::Repairs };
        sim.buildings.stores.book_delivery(good, put, (!load.booked).then_some(source), use_);
    }
    note(ag, true);
    let rest = load.amount - put;
    ag.path = None;
    if rest > WORK_EPS {
        ag.load = Some(Load { amount: rest, ..load });
        ag.task = Task::Carry { dest: NO_STORE };
    } else {
        ag.load = None;
        ag.task = Task::Idle;
    }
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
        let kind = sim.buildings.building(site).map_or(BuildingKind::dam(), |b| b.kind);
        let rec = sim.deeds.of(ag.id);
        *rec.carried.entry((load.good, kind)).or_default() += taken;
        if load.booked {
            rec.hauled += taken;
        }
    }
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
        let labour = dt * LABOUR_PER_DAY * beaver_effort(sim, ag, works_job(sim, ag.id));
        sim.work.add_labour(site, labour).expect("duty checked the site");
        if let Some(kind) = sim.buildings.building(site).map(|b| b.kind) {
            if upkeep(sim, ag.id) || is_mender(sim, ag.id) {
                sim.deeds.repaired(ag.id, site, Some(kind), labour);
            } else {
                sim.deeds.worked(ag.id, site, kind, labour);
            }
        }
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
        let labour = dt * LABOUR_PER_DAY * beaver_effort(sim, ag, crate::jobs::Job::with_duty(crate::content::Duty::Mend));
        let mended = crate::maintenance::mend(sim, building, labour);
        if mended {
            let kind = sim.buildings.building(building).map(|b| b.kind);
            sim.deeds.repaired(ag.id, building, kind, labour);
        }
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
    // The stores filled up on the way: the load is set down here, not carried to no room.
    if sim.buildings.stores.space(load.good) <= WORK_EPS {
        return drop_load(sim, ag, "the stores filled up");
    }
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
        let kind = sim.buildings.building(to).map(|b| b.kind);
        let rec = sim.deeds.of(ag.id);
        rec.hauled += put;
        if let Some(kind) = kind {
            *rec.carried.entry((load.good, kind)).or_default() += put;
        }
        if put >= load.amount-WORK_EPS { ag.load=None; ag.task=Task::Idle; }
        else if put <= WORK_EPS {
            // No room anywhere (stores lost to flooding hold less than there is): the load is
            // set down beside the store rather than tried again for ever.
            return drop_load(sim, ag, "no room in the stores");
        }
        else {ag.load=Some(Load{amount:load.amount-put,..load});}
        note(ag, put>0.0);
    }
}

fn clear_finished_footprint(sim: &mut SimState, id: BuildingId) -> bool {
    let Some(b) = sim.buildings.building(id) else { return false };
    if b.kind.is_dam() {
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

/// The good a stalled site waits on: the first it still needs that no store holds and no one
/// is bringing.
pub(crate) fn site_waits_on(sim: &SimState, p: &crate::work::Workplace) -> Option<Good> {
    let s = p.site.as_ref()?;
    if !site_stalled(sim, p) {
        return None;
    }
    s.needed.iter().map(|n| n.0).find(|&g| s.missing(g) > WORK_EPS)
}

/// Where the colony keeps its goods: its first store of the dam's material, its first home,
/// the supply pile, or (as a last resort) a lodge or beaver's cell.
pub fn colony_home(sim: &SimState) -> Option<Cell> {
    let built = |k: BuildingKind| sim.buildings.buildings.iter().find(|b| b.kind == k && b.is_complete()).map(|b| (b.pos.0 + 1, b.pos.1 + 1));
    BuildingKind::store_for(crate::dams::material())
        .and_then(built)
        .or_else(|| built(BuildingKind::home()))
        .or(sim.work.supplies)
        .or_else(|| sim.colony.lodges.first().map(|l| (l.pos.0.max(0.0) as usize, l.pos.1.max(0.0) as usize)))
        .or_else(|| sim.colony.beavers.first().map(|b| (b.pos.0.max(0.0) as usize, b.pos.1.max(0.0) as usize)))
}

/// The ground round home that beavers are sent out over to gather.
pub(crate) fn home_area(sim: &SimState) -> Option<(Cell, Area)> {
    let home = colony_home(sim)?;
    let r = GATHER_RADIUS / 2;
    Some((home, Area { min: (home.0.saturating_sub(r), home.1.saturating_sub(r)), max: (home.0 + r, home.1 + r) }))
}

/// How badly a waterwork or refit needs its menders, 0 (sound) to 1 (falling down): a dam
/// by its weakest cell, a canal silting up. Refits that only strengthen a building come last.
/// Worn buildings are mended in place and weighed in `send_menders`.
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

/// How much worse off another place must be before a busy mender leaves its own for it.
const REPAIR_SWITCH: f32 = 0.2;

fn is_mender(sim: &SimState, who: BeaverId) -> bool {
    sim.work.slot_of(who).is_none() && sim.agents.get(who).is_some_and(|a| a.assignment == Some(Assignment::Mend))
}

fn is_roaming_builder(sim: &SimState, who: BeaverId) -> bool {
    sim.work.slot_of(who).is_none() && sim.agents.get(who).is_some_and(|a| a.assignment == Some(Assignment::Build))
}

/// Builders assigned without a fixed seat automatically pick up any active build site, dam, or
/// canal needing new work, preferring sites with work ready and higher priority, and wait when
/// nothing needs building.
fn send_builders(sim: &mut SimState) {
    let builders: Vec<(BeaverId, Cell)> = sim
        .colony
        .beavers
        .iter()
        .filter(|b| b.can_work() && is_roaming_builder(sim, b.id))
        .filter_map(|b| sim.agents.get(b.id).map(|a| (b.id, a.cell)))
        .collect();
    sim.agents.build_targets.retain(|who, _| builders.iter().any(|b| b.0 == *who));
    if builders.is_empty() {
        return;
    }
    let any_unassigned = builders.iter().any(|(who, _)| !sim.agents.build_targets.contains_key(who));
    if !any_unassigned && sim.steps % RETARGET_STEPS != 0 {
        return;
    }
    let places: Vec<(BuildingId, crate::work::Priority, Cell)> = sim
        .work
        .places
        .iter()
        .filter(|p| {
            !p.paused
                && (is_site(sim, p)
                    || (sim.buildings.building(p.building).is_some_and(|b| b.operational() && !sim.buildings.unusable(b, &sim.water.depth))
                        && (crate::dams::has_work_for(sim, p.building, false) || crate::canals::has_work_for(sim, p.building, false))))
        })
        .filter_map(|p| Some((p.building, p.priority, sim.buildings.building(p.building)?.pos)))
        .collect();
    for (who, at) in builders {
        let ag = sim.agents.get(who);
        let ready = |b: BuildingId| ag.is_some_and(|a| build_task(sim, b, a).is_some());
        let prio_of = |b: BuildingId| places.iter().find(|p| p.0 == b).map(|p| p.1);
        let current = sim
            .agents
            .build_targets
            .get(&who)
            .copied()
            .filter(|&b| prio_of(b).is_some() && reaches_place(sim, at, b));
        let cur_rank = current.map(|b| (ready(b), prio_of(b).unwrap()));
        let crowd = |b: BuildingId| {
            let slotted = sim.work.place(b).map_or(0, |p| p.builders());
            let roaming = sim.agents.build_targets.iter().filter(|(w, t)| **w != who && **t == b).count();
            slotted + roaming
        };
        let best = places
            .iter()
            .filter(|p| reaches_place(sim, at, p.0))
            .map(|p| (p.0, ready(p.0), p.1, p.2))
            .filter(|p| cur_rank.is_none_or(|(cr, cp)| (p.1, p.2) > (cr, cp)))
            .min_by_key(|p| {
                (
                    std::cmp::Reverse(p.1),
                    std::cmp::Reverse(p.2),
                    crowd(p.0),
                    p.3.0.abs_diff(at.0).pow(2) + p.3.1.abs_diff(at.1).pow(2),
                    p.0,
                )
            })
            .map(|p| p.0);
        match best.or(current) {
            Some(b) => {
                sim.agents.build_targets.insert(who, b);
            }
            None => {
                sim.agents.build_targets.remove(&who);
            }
        }
    }
}

/// Menders need no seats and no orders, as gatherers need none. Each goes to the place it can
/// reach in the worst repair: a worn building to mend (one marked for repair first, if the
/// stores hold what mending it takes), a dam's worn cells, a canal's silt or a refit. Places in
/// equal need are shared out, and a mender stays put unless another is much worse off.
fn send_menders(sim: &mut SimState) {
    if sim.steps % RETARGET_STEPS != 0 {
        return;
    }
    let menders: Vec<(BeaverId, Cell)> = sim
        .colony
        .beavers
        .iter()
        .filter(|b| b.can_work() && is_mender(sim, b.id))
        .filter_map(|b| sim.agents.get(b.id).map(|a| (b.id, a.cell)))
        .collect();
    sim.agents.maint_targets.retain(|who, _| menders.iter().any(|m| m.0 == *who));
    if menders.is_empty() {
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
            .filter(|b| crate::maintenance::needs_mending(sim, b) && !crate::buildings::flooded(b, &sim.water.depth))
            .filter(|b| {
                let spot = crate::supply::Spot { building: b.id, cell: b.pos };
                crate::maintenance::mend_rates(b).0.iter().all(|&(g, _)| sim.buildings.stores.amount(g) > WORK_EPS || crate::supply::delivered(sim, spot, g) > WORK_EPS)
            })
            .map(|b| (b.id, 1.0 - b.condition + if marked(b.id) { 1.0 } else { 0.0 }, b.pos)),
    );
    for (who, at) in menders {
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
    // Seats kept for builders at a finished building go once their builder has moved on.
    let finished: Vec<BuildingId> = sim.work.places.iter().filter(|p| !takes_builders(sim, p)).map(|p| p.building).collect();
    for p in sim.work.places.iter_mut().filter(|p| finished.contains(&p.building)) {
        p.slots.retain(|s| s.beaver.is_some() || s.role.duty() != JobDuty::Build);
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
            let waiting = waiting_here && s.role.duty() == JobDuty::Build;
            if s.beaver.is_some_and(|w| (closed && !waiting) || !alive.contains(&w)) {
                s.beaver = None;
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
    if res.kind.is_tree() {
        if sim.resources.fell(target, dt * beaver_effort(sim, ag, crate::jobs::Job::gathering(crate::resources::ResourceKind::tree())) * FELL_PER_DAY).expect("kind checked above") {
            sim.nav.set(res.cell, Surface::Land);
            let rec = sim.deeds.of(ag.id);
            *rec.felled.entry(res.species.unwrap_or_else(crate::trees::Tree::unknown)).or_default() += 1;
            if res.age_days >= crate::deeds::GREAT_TREE_DAYS {
                rec.great_trees += 1;
            }
            crate::forestry::felled(sim, ag.id, res.cell, res.species.unwrap_or_else(crate::trees::Tree::unknown));
        }
        return;
    }
    let bark = res.kind.is_felled() && job.is_some_and(|j| j.forages());
    let good = match res.kind {
        _ if bark => Good::food(),
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
    let left = if bark { res.bark } else if res.kind.is_felled() && res.amount <= 0.0 { res.branches } else { res.amount };
    let rate = if bark { FOOD_GATHER_PER_DAY } else { res.kind.row().gather_rate };
    let source = if bark { ResourceKind::food_plant(false) } else { res.kind };
    let job = source.row().role;
    let full = load_size(good);
    let mut want = (dt * beaver_effort(sim, ag, job) * rate).min(full - have);
    if left - want < EPS {
        want = left;
    }
    let taken = if bark { sim.resources.strip(target, want).ok().filter(|t| t.1 > 0.0) } else { sim.resources.take(target, want) };
    if let Some((g, got)) = taken {
        if let Some((b, _, _)) = sim.work.slot_of(ag.id) {
            sim.output.record(b, g, got);
        }
        let winter = crate::calendar::Season::at(sim.time_days) == crate::calendar::Season::Winter;
        let rec = sim.deeds.of(ag.id);
        rec.gathered[g] += got;
        if g == Good::food() && winter {
            rec.winter_food += got;
        }
        if bark {
            rec.bark += got;
        } else if let (true, Some(p)) = (g == Good::food(), res.plant) {
            *rec.harvested.entry(p).or_default() += got;
        }
        let amount = have + got;
        // Foragers pick up seedlings of the trees growing by where they forage.
        let found = if g == Good::food() { ag.load.and_then(|l| l.species).or_else(|| crate::forestry::seedling_species(sim, res.cell)) } else { None };
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
        let kind = store.kind;
        let who = producer(sim, ag);
        let booked = stow(sim, dest, load, who);
        if let Some(taken) = booked.as_ref().ok().filter(|t| **t > 0.0) {
            *sim.deeds.of(ag.id).carried.entry((load.good, kind)).or_default() += *taken;
        }
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
        .filter(|b| b.is_complete() && accepts(b.kind, Good::seedling()) && !sim.buildings.unusable(b, &sim.water.depth))
        .filter(|b| sim.work.stock_of(b.id, Good::seedling()) >= 1.0 - WORK_EPS)
        .min_by_key(|b| (crate::stock::dist(from, b), b.id))
        .map(|b| b.id)
}

/// A planter plants in its forester's area; `store` is the food cache the sapling comes from.
fn plant_task(sim: &SimState, area: Area, ag: &Agent) -> Option<Task> {
    let carrying = ag.load.is_some_and(|l| l.good == Good::seedling());
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
    if ag.load.is_some() || sim.buildings.stores.space(Good::seedling()) < SEEDLINGS_PER_TREE {
        return None;
    }
    let now = sim.time_days;
    sim.resources
        .items()
        .iter()
        .filter(|r| area.reaches(r.cell) && seeds(r) && within_shore(sim, crate::jobs::Job::with_duty(crate::content::Duty::Forest), r.cell))
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
    let progress = progress + dt * LABOUR_PER_DAY * beaver_effort(sim, ag, crate::jobs::Job::with_duty(crate::content::Duty::Forest));
    if progress + WORK_EPS < SEED_WORK {
        ag.task = Task::Seed { tree, progress };
        return;
    }
    // Seedlings of this tree's species, for the nearest forester's store.
    sim.deeds.of(ag.id).seedlings += 1;
    ag.load = Some(Load { species, ..Load::new(Good::seedling(), SEEDLINGS_PER_TREE, false) });
    ag.path = None;
    ag.task = Task::Carry { dest: NO_STORE };
}

#[allow(clippy::too_many_arguments)]
fn plant(sim: &mut SimState, ag: &mut Agent, dt: f32, duty: &Duty, store: BuildingId, cell: Cell, progress: f32) {
    let Duty::Forest(area) = *duty else { return abandon(ag) };
    let Some(load) = ag.load.filter(|l| l.good == Good::seedling()) else {
        if ag.load.is_some() || sim.buildings.building(store).is_none_or(|b| !b.is_complete() || !accepts(b.kind, Good::seedling())) {
            return abandon(ag);
        }
        if !approach(sim, ag, dt, store) {
            return;
        }
        note(ag, true);
        let (got, species) = sim.buildings.stores.take_kind_from(store, Good::seedling(), 1.0);
        log_load(sim, ag, "TAKE_SAPLING", store, Good::seedling(), 1.0, got, &format!("species={species:?}"));
        if got > WORK_EPS {
            ag.load = Some(Load { species, ..Load::new(Good::seedling(), got, true) });
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
    let progress = progress + dt * LABOUR_PER_DAY * beaver_effort(sim, ag, crate::jobs::Job::with_duty(crate::content::Duty::Forest));
    if progress + WORK_EPS < PLANT_WORK {
        ag.task = Task::Plant { store, cell, progress };
        return;
    }
    // A sapling grows into its own species; one of no known species takes the species of a food tree nearby.
    let species = load.species.unwrap_or_else(|| crate::forestry::planting_species(sim, cell));
    let tree = sim.resources.add_tree(cell, 0.0, species);
    sim.forestry.planted.insert(tree, (ag.id, sim.time_days));
    sim.ledger.planted += 1;
    sim.deeds.of(ag.id).planted += 1;
    let used = load.amount.min(1.0);
    log_load(sim, ag, "PLANT", NO_STORE, Good::seedling(), load.amount, used, &format!("species={species:?} at=({},{})", cell.0, cell.1));
    sim.buildings.stores.book_used(Good::seedling(), used, crate::buildings::Use::Other);
    let rest = load.amount - used;
    ag.load = (rest > WORK_EPS).then_some(Load { amount: rest, ..load });
    ag.task = Task::Idle;
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
        Task::Raft { .. } => "step/agents/beaver raft",
        Task::Supply { .. } => "step/agents/beaver bring materials",
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
    { let _p = crate::prof::scope("step/agents/builders"); send_builders(sim); }
    { let _p = crate::prof::scope("step/agents/menders"); send_menders(sim); }
    { let _p = crate::prof::scope("step/agents/supplies"); crate::supply::tidy(sim); }
    let ids: Vec<BeaverId> = sim.agents.by_id.keys().copied().collect();
    for id in ids {
        let Some(mut ag) = sim.agents.by_id.remove(&id) else { continue };
        {
            let _p = crate::prof::scope(act_timer(&ag.task));
            tick(sim, &mut ag, dt_days);
        }
        if !ag.trail.is_empty() {
            let rec = sim.deeds.of(id);
            for &(_, gait) in &ag.trail {
                if gait == crate::nav::Gait::Swim { rec.swum += 1.0 } else { rec.walked += 1.0 }
            }
        }
        ag.keep_history(now);
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
            ag.keep_history(now);
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

/// Cells of history an agent keeps (`Agent::history`): more than it covers between two looks
/// at the world at the fastest speed.
const HISTORY_CELLS: usize = 256;

impl Agent {
    /// This step's trail into the history, at the time the step ends (`now`).
    /// When the agent stays put (working, delivering, eating, resting), record the dwell so
    /// fast-speed playback visits and pauses at destinations instead of cutting across them.
    fn keep_history(&mut self, now: f64) {
        if self.trail.is_empty() {
            let len = self.history.len();
            let same_last_two = len >= 2
                && self.history[len - 1].1 == self.cell
                && self.history[len - 2].1 == self.cell;
            if same_last_two {
                if let Some(back) = self.history.back_mut() {
                    back.0 = now;
                    back.2 = self.gait;
                }
            } else if self.history.back().is_none_or(|h| h.1 != self.cell || (now - h.0).abs() > 1e-9) {
                self.history.push_back((now, self.cell, self.gait));
            }
        } else {
            for &(c, g) in &self.trail {
                self.history.push_back((now, c, g));
            }
        }
        while self.history.len() > HISTORY_CELLS {
            self.history.pop_front();
        }
    }
}

fn at_resting_place(ag: &Agent, home: Option<&Building>) -> bool {
    let _ = home;
    ag.inside
}
