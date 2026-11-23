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
    /// Clear round a food plant or young tree so it grows faster for a while.
    Tend { target: ResourceId, progress: f32 },
    /// Take a sapling from a food store and plant it on one cell.
    Plant { store: BuildingId, cell: Cell, progress: f32 },
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
}


#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct Agents {
    pub by_id: BTreeMap<BeaverId, Agent>,
    pub unreachable: BTreeMap<ResourceId, f64>,
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

fn sync_agents(sim: &mut SimState) {
    let gone: Vec<BeaverId> =
        sim.agents.by_id.keys().copied().filter(|&id| sim.colony.beaver(id).is_none()).collect();
    for id in gone {
        if let Some(ag) = sim.agents.by_id.remove(&id) {
            sim.nav.release(id, ag.cell);
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
}


fn gatherable(r: &Resource) -> bool {
    match r.kind {
        ResourceKind::Tree => crate::forestry::fellable(r),
        ResourceKind::Felled => r.amount > 0.0 || r.branches > 0.0,
        ResourceKind::Plant | ResourceKind::WaterPlant => r.amount >= PLANT_MIN_TAKE,
        ResourceKind::RiverClay => r.amount >= crate::resources::RIVER_CLAY_MIN_TAKE,
        _ => r.amount > 0.0,
    }
}

/// A food plant is only worth setting out for with enough on it; once there, a beaver strips
/// it down to `PLANT_MIN_TAKE`. Without the gap, foragers wait on a grazed plant and carry
/// home each sliver of regrowth.
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
                .map(|_| Job { kinds: &[], area: None, only: Some(id), min_age: 0.0 }),
            Assignment::Area(ResourceKind::Pebbles, _) if !has_toolshed(sim) => None,
            Assignment::Area(kind, area) => Some(Job { kinds: kinds_for(kind), area: Some(area), only: None, min_age: crate::forestry::cut_min_age(sim, None) }),
            Assignment::Slot(b, slot) => {
                let place = sim.work.place(b)?;
                match (place.slots.get(slot)?.role, place.area) {
                    (Role::Collect(kind), Some(area)) => {
                        Some(Job { kinds: kinds_for(kind), area: Some(area), only: None, min_age: crate::forestry::cut_min_age(sim, Some(b)) })
                    }
                    _ => None,
                }
            }
            Assignment::Go(_) => None,
        }
    }
    fn forages(&self) -> bool {
        self.kinds.first() == Some(&ResourceKind::Plant)
    }
    fn permits(&self, r: &Resource) -> bool {
        if self.only.is_some_and(|id|id!=r.id) || self.area.is_some_and(|a|!a.contains(r.cell)) {return false;}
        let wanted = if self.forages() && r.kind == ResourceKind::Felled { r.bark > 0.0 } else { gatherable(r) };
        wanted
            && (self.only.is_some() || self.kinds.contains(&r.kind))
            && (r.kind != ResourceKind::Tree || crate::forestry::cuttable(r, self.min_age))
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
    /// before standing ones); land and water plants count as one kind.
    fn pick(&self, sim: &SimState, from: Cell) -> Option<ResourceId> {
        let rank = |r: &Resource| {
            let kind = if r.kind == ResourceKind::WaterPlant { ResourceKind::Plant } else { r.kind };
            self.kinds.iter().position(|&k| k == kind).unwrap_or(0)
        };
        let reachable = |r: &Resource| {
            sim.agents.unreachable.get(&r.id).is_none_or(|until| sim.time_days >= *until) && sim.nav.may_reach_beside(from, r.cell)
        };
        let claims = claims(sim);
        let full = |r: &Resource| self.only.is_none() && claims.get(&r.id).copied().unwrap_or(0) >= gatherers_per(r.kind);
        let mut drops: [Option<Vec<Cell>>; Good::ALL.len()] = Default::default();
        // Connectivity is much more expensive than the walk; a source whose walk alone
        // cannot beat the current choice needs neither its haul nor a route test.
        let mut best: Option<((usize, u32, ResourceId), ResourceId)> = None;
        let mut consider = |r: &Resource| {
            if !self.permits(r) || !worth_starting(r) || full(r) {return;}
            let walk = cell_dist(from, r.cell);
            if best.as_ref().is_some_and(|((k, c, _), _)| (rank(r), trip_key(walk)) > (*k, *c)) {return;}
            let good = self.good_of(r);
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
        .filter(|b| b.is_complete() && accepts(b.kind, good) && !sim.buildings.stranded(b, &sim.water.depth))
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
        .filter(|b| b.is_complete() && accepts(b.kind, good) && !sim.buildings.stranded(b, &sim.water.depth))
        .map(|b|(crate::nav::minimum_cost(crate::stock::dist(from,b).saturating_sub(1)),b)).collect();
    stores.sort_by(|a,b|a.0.total_cmp(&b.0).then(a.1.id.cmp(&b.1.id)));
    let mut best:Option<(BuildingId,Path)>=None;
    for (bound,b) in stores {
        // Even an all-canal path to this store cannot beat the known route.
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
    let dry = !matches!(surface, Some(Surface::Water | Surface::LodgeEntrance | Surface::Canal));
    if load.good == Good::Sticks && dry { STICKS_LAND_PACE } else { LADEN_PACE }
}

pub(crate) fn follow(nav: &mut NavGrid, ag: &mut Agent, dt_days: f32) -> bool {
    let pace = pace(nav, ag);
    let mut budget = ag.step_frac + dt_days * MOVE_COST_PER_DAY * pace;
    let mut moved = false;
    ag.trail.clear();
    loop {
        let Some(path) = ag.path.as_mut() else { break };
        let Some(&(next, gait)) = path.steps.first() else {
            ag.path = None;
            break;
        };
        let cost = nav.surface_at(next).and_then(NavGrid::step_cost).map_or(1.0, |c| c.0) * crate::nav::step_length(ag.cell, next);
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
    Tend(Area),
    Plant(Area),
}

fn duty_of(sim: &SimState, ag: &Agent, can_work: bool) -> Duty {
    if !can_work {
        return Duty::None;
    }
    match sim.work.slot_of(ag.id) {
        Some((b, _, Role::Build)) if sim.work.place(b).is_some_and(|p| p.site.is_some() || (!p.paused && (crate::dams::has_work_for(sim, b, false) || crate::canals::has_work_for(sim, b, false)))) => Duty::Build(b),
        Some((b, _, Role::Maintain)) if sim.work.place(b).is_some_and(|p| p.site.is_some() || (!p.paused && (crate::dams::has_work_for(sim, b, true) || crate::canals::has_work_for(sim, b, true)))) => Duty::Build(b),
        Some((_, _, Role::Haul)) => Duty::Haul,
        Some((b, _, Role::Tend)) => sim.work.place(b).and_then(|p| p.area).map_or(Duty::None, Duty::Tend),
        Some((b, _, Role::Plant)) => sim.work.place(b).and_then(|p| p.area).map_or(Duty::None, Duty::Plant),
        Some((b, i, Role::Collect(_))) => Job::resolve(sim, Assignment::Slot(b, i)).map_or(Duty::None, Duty::Collect),
        Some(_) => Duty::None,
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
            _ => 0.0,
        })
        .sum()
}

fn closest_store(sim: &SimState, from: Cell, good: Good) -> Option<BuildingId> {
    let d = |b: &Building| b.cells().map(|c| c.0.abs_diff(from.0).pow(2) + c.1.abs_diff(from.1).pow(2)).min().unwrap_or(0);
    sim.buildings.buildings.iter().filter(|b| b.is_complete() && accepts(b.kind, good) && !sim.buildings.stranded(b, &sim.water.depth)).min_by_key(|b| (d(b), b.id)).map(|b| b.id)
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
    matches!(sim.work.slot_of(who), Some((_, _, Role::Maintain)))
}

fn canal_task(sim: &SimState, site: BuildingId, ag: &Agent) -> Option<Task> {
    let taken = crate::canals::claimed(sim, site, ag.id);
    let (cell, gate) = crate::canals::next_job(sim, site, &taken, upkeep(sim, ag.id))?;
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
    let from = crate::stock::nearest_holding(sim, cell, Good::Sticks, want).or_else(|| crate::stock::nearest_holding(sim, cell, Good::Sticks, WORK_EPS))?;
    Some(Task::Fetch { from, good: Good::Sticks, then: dam, site_delivery: true })
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
            let near = sim.buildings.building(site).map_or(ag.cell, |b| b.pos);
            if let Some(from) = crate::stock::nearest_holding(sim, near, good, want).or_else(|| crate::stock::nearest_holding(sim, near, good, WORK_EPS)) {
                return Some(Task::Fetch { from, good, then: site, site_delivery: true });
            }
        }
    }
    None
}

fn site_haul_task(sim: &SimState, ag: &Agent) -> Option<Task> {
    let mut sites: Vec<_> = sim.work.places.iter().filter(|p| is_site(sim, p) && !p.paused).collect();
    sites.sort_by_key(|p| (std::cmp::Reverse(p.priority), p.building));
    sites.into_iter().find_map(|p| site_fetch_task(sim, p.building, ag))
}

fn haul_task(sim: &SimState) -> Option<Task> {
    let ok = |id| sim.buildings.building(id).is_some_and(|b| b.is_complete());
    let open = |id| sim.buildings.building(id).is_some_and(|b| b.is_complete() && !sim.buildings.stranded(b, &sim.water.depth));
    sim.work
        .hauls
        .iter()
        .find(|o| ok(o.from) && open(o.to) && sim.work.stock_of(o.from, o.good) > WORK_EPS)
        .map(|o| Task::Fetch { from: o.from, good: o.good, then: o.to, site_delivery: false })
}

fn next_task(sim: &SimState, duty: &Duty, ag: &Agent) -> Option<Task> {
    let _p = crate::prof::scope(match duty {
        Duty::None => "+choose work/none",
        Duty::Collect(_) => "+choose work/collect",
        Duty::Build(_) => "+choose work/build",
        Duty::Tend(_) => "+choose work/tend",
        Duty::Plant(_) => "+choose work/plant",
        Duty::Haul => "+choose work/haul",
    });
    match duty {
        Duty::None => None,
        Duty::Collect(j) => j.pick(sim, ag.cell).map(|t| Task::Collect { target: t, progress: 0.0 }),
        Duty::Build(site) => build_task(sim, *site, ag),
        Duty::Tend(area) => crate::forestry::tend_target(sim, *area, ag.cell, ag.id).map(|target| Task::Tend { target, progress: 0.0 }),
        Duty::Plant(area) => plant_task(sim, *area, ag),
        Duty::Haul => site_haul_task(sim, ag).or_else(|| haul_task(sim)).or_else(|| {
            let (from, then, good) = crate::stock::fill_move(sim)?;
            Some(Task::Fetch { from, good, then, site_delivery: false })
        }).or_else(|| {
            let own = sim.work.slot_of(ag.id).map(|s| s.0).filter(|b| sim.work.balanced.contains(b))?;
            let (from, then, good) = crate::stock::balance_move(sim, Some(own))?;
            Some(Task::Fetch { from, good, then, site_delivery: false })
        }),
    }
}

fn tick(sim: &mut SimState, ag: &mut Agent, dt: f32) {
    // A trail belongs to this step, including steps spent working or resting.
    ag.trail.clear();
    if crate::predators::flee(sim, ag, dt) {
        return;
    }
    if let Some(Assignment::Go(to)) = ag.assignment {
        go_to(sim, ag, dt, to);
        return;
    }
    let (can_work, tired) = sim.colony.beaver(ag.id).map_or((false, false), |b| (b.can_work(), b.resting));
    let duty = duty_of(sim, ag, can_work && !tired);
    ag.speed = sim.colony.beaver(ag.id).map_or(1.0, |b| crate::society::work_speed(b.mood));
    if tired && ag.load.is_none() && !ag.inside && !matches!(ag.task, Task::Rest | Task::Carry { .. }) {
        ag.path = None;
        ag.task = Task::Rest;
    }
    if matches!(duty, Duty::None) && can_work && !tired && matches!(ag.assignment, Some(Assignment::Resource(_))) {
        ag.assignment = None;
    }
    match ag.task.clone() {
        Task::Idle => {
            ag.path = None;
            let dam_load = (matches!(duty, Duty::Build(b) if is_dam(sim, b) && !sim.maintenance.jobs.contains_key(&b)) && ag.load.is_some_and(|l| l.good == Good::Sticks))
                || (matches!(duty, Duty::Plant(..)) && ag.load.is_some_and(|l| l.good == Good::Saplings));
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
        Task::Tend { target, progress } => tend(sim, ag, dt, &duty, target, progress),
        Task::Plant { store, cell, progress } => plant(sim, ag, dt, &duty, store, cell, progress),
        Task::Carry { dest } if ag.load.is_some_and(|l| sim.work.place(dest).and_then(|p|p.site.as_ref()).is_some_and(|p|p.missing(l.good)>WORK_EPS)) => deliver(sim, ag, dt, dest),
        Task::Carry { dest } => carry(sim, ag, dt, dest),
        Task::Fetch { from, good, then, .. } if is_dam(sim, then) && !sim.maintenance.jobs.contains_key(&then) => dam_fetch(sim, ag, dt, &duty, from, good, then),
        Task::Fetch { from, good, then, site_delivery } => fetch(sim, ag, dt, &duty, from, good, then, site_delivery),
        Task::Build { site } => build(sim, ag, dt, &duty, site),
        Task::Haul { to } => haul(sim, ag, dt, to),
    }
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
    sim.colony.beaver(ag.id).map_or(0.0, |b| {
        b.work_weight() * crate::society::work_speed(b.mood)
            * (WORK_HEALTH_BASE + WORK_HEALTH_SCALE * b.health) * (WORK_REST_BASE + WORK_REST_SCALE * b.needs.rest)
            * (WORK_SKILL_BASE + WORK_SKILL_SCALE * b.skills[job.index()])
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

fn haul(sim: &mut SimState, ag: &mut Agent, dt: f32, to: BuildingId) {
    let Some(load) = ag.load else {
        ag.task = Task::Idle;
        return;
    };
    let to = if sim.buildings.building(to).is_some_and(|b| b.is_complete() && !sim.buildings.stranded(b, &sim.water.depth)) {
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

fn tidy(sim: &mut SimState) {
    let alive: std::collections::BTreeSet<BeaverId> = sim.colony.beavers.iter().map(|b| b.id).collect();
    let dry: Vec<BuildingId> = sim.buildings.buildings.iter().filter(|b| sim.buildings.stranded(b, &sim.water.depth)).map(|b| b.id).collect();
    for p in &mut sim.work.places {
        let refitting=sim.maintenance.jobs.contains_key(&p.building);
        let ruined=sim.buildings.building(p.building).is_some_and(|b|b.is_complete() && !b.operational());
        let closed = p.paused || (!refitting && (dry.contains(&p.building) || ruined));
        for s in &mut p.slots {
            if s.worker.is_some_and(|w| closed || !alive.contains(&w)) {
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
    if res.kind == ResourceKind::Tree {
        if sim.resources.fell(target, dt * worker_effort(sim, ag, crate::colony::Job::Logger) * FELL_PER_DAY).expect("kind checked above") {
            sim.nav.set(res.cell, Surface::Land);
            crate::forestry::felled(sim, ag.id, res.cell, res.species.unwrap_or(crate::trees::Tree::Aspen));
        }
        return;
    }
    let bark = res.kind == ResourceKind::Felled && job.is_some_and(|j| j.forages());
    let room = |g: Good| sim.buildings.stores.space(g) > EPS;
    let good = match res.kind {
        _ if bark => Good::Food,
        ResourceKind::Felled if res.amount > 0.0 => Good::Sticks,
        ResourceKind::Felled => Good::Sticks,
        k => k.yields()[0],
    };
    let has_store = closest_store(sim, ag.cell, good).is_some() || sim.work.supplies.is_some();
    if has_store && !room(good) && ag.load.is_none() && site_short_of(sim, ag.cell, good).is_none() {
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
    if sim.buildings.stores.space(load.good) <= EPS {
        ag.path = None;
        note(ag, false);
        return;
    }
    if closest_store(sim, ag.cell, load.good).is_none() {
        let home = if load.good == Good::Food { larder_cell(sim, ag.id) } else { sim.work.supplies };
        if let Some(home) = home {
            return carry_home(sim, ag, dt, load, home);
        }
        note(ag, false);
        return;
    }
    let dest = if ag.path.is_some() || adjacent_to_store(sim, ag.cell, dest, load.good) {
        dest
    } else {
        let Some((id, path)) = nearest_store(sim, ag.id, ag.cell, load.good) else {
            note(ag, false);
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
        let larder = store.kind == BuildingKind::Larder;
        let booked = stow(sim, dest, load, who);
        log_load(sim, ag, "DELIVER_STORE", dest, load.good, load.amount, *booked.as_ref().unwrap_or(&0.0), &format!("booked={} result={:?}", load.booked, booked.as_ref().map(|_| "ok").map_err(|e| format!("{e:?}"))));
        // Foragers bring back seedlings of the trees where they foraged with fresh food; they
        // are kept at the nearest forester, and lost if there is none.
        if let (true, false, Good::Food, Some(sp), Ok(taken)) = (larder, load.booked, load.good, load.species, &booked) {
            if let Some(nursery) = closest_store(sim, ag.cell, Good::Saplings) {
                let _ = sim.buildings.stores.produce_kind_at(nursery, Good::Saplings, Some(sp), taken * SAPLINGS_PER_FOOD, who);
            }
        }
        match booked {
            Ok(taken) if taken >= load.amount => {
                ag.load = None;
                ag.task = Task::Idle;
                note(ag, true);
            }
            Ok(taken) => {
                ag.load = Some(Load { amount: load.amount - taken, ..load });
                note(ag, taken > 0.0);
            }
            Err(_) => note(ag, false),
        }
        return;
    }
    let moved = follow(&mut sim.nav, ag, dt);
    note(ag, moved);
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

fn larder_cell(sim: &SimState, who: BeaverId) -> Option<Cell> {
    let b = sim.colony.beaver(who)?;
    match sim.colony.lodges.iter().find(|l| Some(l.id) == b.lodge).or(sim.colony.lodges.first()) {
        Some(l) => Some((l.pos.0.max(0.0) as usize, l.pos.1.max(0.0) as usize)),
        None => sim.work.supplies,
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
        .filter(|b| b.is_complete() && accepts(b.kind, Good::Saplings) && !sim.buildings.stranded(b, &sim.water.depth))
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

fn tend(sim: &mut SimState, ag: &mut Agent, dt: f32, duty: &Duty, target: ResourceId, progress: f32) {
    let Duty::Tend(area) = *duty else { return abandon(ag) };
    let Some(cell) = sim.resources.get(target).filter(|r| crate::forestry::wants_tending(r) && area.contains(r.cell)).map(|r| r.cell) else {
        ag.path = None;
        ag.task = Task::Idle;
        return;
    };
    match reach(sim, ag, dt, cell) {
        None => {
            sim.agents.unreachable.insert(target, sim.time_days + UNREACHABLE_RETRY_DAYS);
            if crate::trace::enabled() { crate::trace::event(format!("UNREACHABLE {} beaver={} resource={} cell=({},{}) retry_after_days={} task={:?}", crate::trace::sim_stamp(), ag.id.0, target.0, ag.cell.0, ag.cell.1, UNREACHABLE_RETRY_DAYS, ag.task)); }
            ag.task = Task::Idle;
            return;
        }
        Some(false) => return,
        Some(true) => {}
    }
    note(ag, true);
    let progress = progress + dt * LABOUR_PER_DAY * worker_effort(sim, ag, crate::colony::Job::Forager);
    if progress + WORK_EPS < TEND_WORK {
        ag.task = Task::Tend { target, progress };
        return;
    }
    if let Ok(r) = sim.resources.get_mut(target) {
        r.tended_days = crate::forestry::TEND_DAYS;
    }
    ag.task = Task::Idle;
}

#[allow(clippy::too_many_arguments)]
fn plant(sim: &mut SimState, ag: &mut Agent, dt: f32, duty: &Duty, store: BuildingId, cell: Cell, progress: f32) {
    let Duty::Plant(area) = *duty else { return abandon(ag) };
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
    if !area.contains(cell) || !crate::forestry::plantable(sim, cell) {
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
    sim.resources.add_tree(cell, 0.0, species);
    let used = load.amount.min(1.0);
    log_load(sim, ag, "PLANT", NO_STORE, Good::Saplings, load.amount, used, &format!("species={species:?} at=({},{})", cell.0, cell.1));
    sim.buildings.stores.book_used(Good::Saplings, used, crate::buildings::Use::Other);
    let rest = load.amount - used;
    ag.load = (rest > WORK_EPS).then_some(Load { amount: rest, ..load });
    ag.task = Task::Idle;
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
    fn foragers_bring_saplings_of_the_trees_where_they_foraged() {
        let (mut s, who, store, _) = setup();
        let mut a = take_agent(&mut s, who);
        deliver_food(&mut s, &mut a, Some(Tree::Birch));
        assert_eq!(s.work.stock_of(store, Good::Food), 4.0);
        let saplings = s.buildings.stores.kinds_at(BuildingId(8), Good::Saplings);
        assert_eq!(saplings.len(), 1);
        assert_eq!(saplings[0].0, Some(Tree::Birch));
        assert!((saplings[0].1 - 4.0 * SAPLINGS_PER_FOOD).abs() < 1e-5);
        // Food from where no tree grows brings no seedlings home.
        deliver_food(&mut s, &mut a, None);
        assert_eq!(s.work.stock_of(store, Good::Food), 8.0);
        assert!((s.work.stock_of(BuildingId(8), Good::Saplings) - 4.0 * SAPLINGS_PER_FOOD).abs() < 1e-5);
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
        let duty = Duty::Plant(area);
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
    /// regrowth in: they looked stuck at the store and the colony went hungry.
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
        collect(&mut s, &mut a, 0.01, Duty::Collect(job), grazed);
        assert!(matches!(a.task, Task::Collect { target, .. } if target == lush), "{:?}", a.task);
        assert_eq!(a.load.map(|l| l.amount), Some(1.5), "the part load is kept");
        let _ = area;
    }

    #[test]
    fn tending_marks_a_plant_to_grow_faster() {
        let (mut s, who, _, area) = setup();
        let at = s.agents.get(who).unwrap().cell;
        let id = s.resources.add(ResourceKind::Plant, (at.0 + 1, at.1), 1.0);
        s.resources.get_mut(id).unwrap().wet = true;
        s.resources.get_mut(id).unwrap().max_amount = 4.0;
        assert_eq!(crate::forestry::tend_target(&s, area, at, who), Some(id));
        let mut a = take_agent(&mut s, who);
        a.task = Task::Tend { target: id, progress: 0.0 };
        for _ in 0..400 {
            let Task::Tend { target, progress } = a.task.clone() else { break };
            tend(&mut s, &mut a, 0.01, &Duty::Tend(area), target, progress);
        }
        assert_eq!(a.task, Task::Idle);
        assert_eq!(s.resources.get(id).unwrap().tended_days, crate::forestry::TEND_DAYS);
        // A tended plant regrows faster than an untended one.
        let other = s.resources.add(ResourceKind::Plant, (at.0 + 3, at.1 + 3), 1.0);
        s.resources.get_mut(other).unwrap().wet = true;
        s.resources.get_mut(other).unwrap().max_amount = 4.0;
        s.resources.step(1.0, 150.0).unwrap();
        assert!(s.resources.get(id).unwrap().amount > s.resources.get(other).unwrap().amount);
    }

    #[test]
    fn planters_turn_a_stored_sapling_into_a_tree_in_range() {
        let (mut s, who, _, area) = setup();
        s.buildings.stores.produce_at(BuildingId(8), Good::Saplings, 2.0, Producer::Other).unwrap();
        let mut a = take_agent(&mut s, who);
        let duty = Duty::Plant(area);
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
        assert!(area.contains(trees[0].cell));
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
    fn gathered_materials_reach_camp_and_full_storage_preserves_the_load() {
        let mut s = SimState::new(1);
        s.nav = NavGrid::new(16, 16);
        s.work.supplies = Some((5, 5));
        let mut agents = Agents::default();
        agents.spawn(BeaverId(1), (5, 4));
        let mut a = agents.by_id.remove(&BeaverId(1)).unwrap();
        a.load = Some(Load::new(Good::Clay, 4.0, false));
        a.task = Task::Carry { dest: NO_STORE };
        carry(&mut s, &mut a, 0.01, NO_STORE);
        assert_eq!(s.buildings.stores.amount(Good::Clay), 4.0);
        assert!(a.load.is_none());
        s.buildings.stores.deposit(Good::Clay, 1000.0).unwrap();
        a.load = Some(Load::new(Good::Clay, 4.0, false));
        a.task = Task::Carry { dest: NO_STORE };
        carry(&mut s, &mut a, 0.01, NO_STORE);
        assert_eq!(a.load.unwrap().amount, 4.0);
        assert!(matches!(a.task, Task::Carry { .. }));
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
        crate::maintenance::order(&mut s,BuildingId(2),crate::maintenance::Refit::Repair).unwrap();
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
    #[test]
    fn resource_choice_matches_exhaustive_reachable_ranking() {
        let mut s=SimState::new(1);let size=s.world.size;let mut nav=NavGrid::new(size,size);nav.rebuild(&s);s.nav=nav;
        for r in s.resources.items().iter().step_by(3) {s.agents.unreachable.insert(r.id,1.0);}
        for kind in [ResourceKind::Tree,ResourceKind::Plant,ResourceKind::MudBank,ResourceKind::RiverClay] {
            for from in [(5,5),(size/2,size/2),(size-5,size-5)] {
                for area in [None,Some(Area{min:(0,0),max:(size/2,size/2)})] {
                    let job=Job{kinds:kinds_for(kind),area,only:None,min_age:300.0};
                    let reference=s.resources.items().iter().filter(|r|job.permits(r) && worth_starting(r)
                        && s.agents.unreachable.get(&r.id).is_none_or(|&until|s.time_days>=until)
                        && s.nav.may_reach_beside(from,r.cell))
                        .min_by_key(|r|{
                            let haul=drop_spots(&s,job.good_of(r),from).iter().map(|&d|cell_dist(r.cell,d)).fold(f32::INFINITY,f32::min);
                            let kind=if r.kind==ResourceKind::WaterPlant {ResourceKind::Plant} else {r.kind};
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
        let job=Job{kinds:kinds_for(ResourceKind::MudBank),area:None,only:None,min_age:0.0};
        assert_eq!(job.pick(&s,(20,41)),Some(west));
        assert_eq!(job.pick(&s,(60,41)),Some(east));
        // A deposit takes two diggers at once; a third goes to the next one.
        for (i,cell) in [(22,40),(22,42)].into_iter().enumerate() {
            let id=BeaverId(100+i as u64);s.agents.spawn(id,cell);
            s.agents.by_id.get_mut(&id).unwrap().task=Task::Collect{target:west,progress:0.0};
        }
        assert_eq!(job.pick(&s,(20,41)),Some(east),"west is fully worked");
        // A tree takes one logger.
        let tree=s.resources.add_tree((25,30),crate::forestry::MATURE_AGE,crate::trees::Tree::Aspen);
        let other=s.resources.add_tree((25,33),crate::forestry::MATURE_AGE,crate::trees::Tree::Aspen);
        let loggers=Job{kinds:kinds_for(ResourceKind::Tree),area:None,only:None,min_age:0.0};
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

fn carry_home(sim: &mut SimState, ag: &mut Agent, dt: f32, load: Load, home: Cell) {
    let at_home = ag.cell.0.abs_diff(home.0) + ag.cell.1.abs_diff(home.1) <= 1;
    if !at_home && ag.path.is_none() {
        ag.path = sim.nav.find_path(ag.id, ag.cell, Goal::Beside(home));
        if ag.path.is_none() {
            note(ag, false);
            return;
        }
    }
    if at_home {
        ag.path = None;
        let who = producer(sim, ag);
        // Only food is kept in a shelter; materials go back on the supply pile where builders look.
        let owner = sim.buildings.buildings.iter().find(|b| load.good == Good::Food && b.kind == BuildingKind::Shelter && adjacent(home,b)).map_or(SUPPLIES, |b| b.id);
        let taken = stow(sim, owner, load, who).unwrap_or(0.0);
        log_load(sim, ag, "DELIVER_HOME", owner, load.good, load.amount, taken, &format!("home=({},{})", home.0, home.1));
        if taken >= load.amount - EPS {
            ag.load = None;
            ag.task = Task::Idle;
        } else {
            ag.load = Some(Load { amount: load.amount - taken, ..load });
        }
        note(ag, taken > 0.0);
        return;
    }
    let moved = follow(&mut sim.nav, ag, dt);
    note(ag, moved);
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
        Task::Tend { .. } => "step/agents/beaver tend",
        Task::Plant { .. } => "step/agents/beaver plant",
        Task::Carry { .. } => "step/agents/beaver carry",
        Task::Fetch { .. } => "step/agents/beaver fetch",
        Task::Build { .. } => "step/agents/beaver build",
        Task::Haul { .. } => "step/agents/beaver haul",
    }
}

pub fn step(sim: &mut SimState, dt_days: f32) {
    if !sim.colony.is_founded() {
        return;
    }
    { let _p = crate::prof::scope("step/agents/retire fills"); crate::stock::retire_fills(sim); }
    {
        let _p = crate::prof::scope("step/agents/nav refresh");
        let mut nav = std::mem::replace(&mut sim.nav, NavGrid::new(1, 1));
        nav.refresh(sim);
        sim.nav = nav;
    }
    { let _p = crate::prof::scope("step/agents/sync"); sync_agents(sim); }
    { let _p = crate::prof::scope("step/agents/tidy"); tidy(sim); }
    let ids: Vec<BeaverId> = sim.agents.by_id.keys().copied().collect();
    for id in ids {
        let Some(mut ag) = sim.agents.by_id.remove(&id) else { continue };
        {
            let _p = crate::prof::scope(act_timer(&ag.task));
            tick(sim, &mut ag, dt_days);
        }
        sim.agents.by_id.insert(id, ag);
    }
    let _p = crate::prof::scope("step/agents/finish sites");
    finish_sites(sim);
}

fn at_resting_place(ag: &Agent, home: Option<&Building>) -> bool {
    let _ = home;
    ag.inside
}
