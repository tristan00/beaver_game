include!(concat!(env!("OUT_DIR"), "/agents_constants.rs"));
use std::collections::{BTreeMap, BTreeSet, HashMap, VecDeque};

use crate::buildings::{Building, BuildingId, BuildingKind, Good, Kind, PerId, Producer, Use};
use crate::colony::{Beaver, BeaverId, DeathCause, Event, Stage};
use crate::content::{Duty as JobDuty, SkillTrait};
use crate::inventory::{SeedlingSort, Species};
use crate::jobs::Job;
use crate::nav::{Gait, Goal, NavGrid, Path, Surface};
use crate::resources::{Behaviour, Resource, ResourceId, ResourceKind};
use crate::supply::Spot;
use crate::water::Cell;
use crate::work::{Area, Workplace, EPS as WORK_EPS, LABOUR_PER_DAY};
use crate::works::Spend;
use crate::SimState;

const EPS: f32 = 1e-3;
const NO_STORE: BuildingId = BuildingId(0);
pub const SUPPLIES: BuildingId = BuildingId(u64::MAX);
const PILE_REACH: usize = 8;
const ORDER_REACH: usize = 24;
const WORK_PROBE_DAYS: f64 = 0.125;
const HISTORY_CELLS: usize = 256;

#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Load {
    pub good: Good,
    pub amount: f32,
    pub booked: bool,
    pub species: Option<Species>,
}

pub fn load_size(good: Good) -> f32 { good.row().load }
pub(crate) fn load_cap(sim: &SimState, who: BeaverId, good: Good) -> f32 {
    load_size(good) * sim.colony.beaver(who).map_or(1.0, crate::growth::load_x)
}

impl Load {
    pub fn new(good: Good, amount: f32, booked: bool) -> Self { Self { good, amount, booked, species: None } }
    pub fn kind(&self) -> Kind { self.species.filter(|_| self.good.row().carries_species) }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
pub enum Source { Store(BuildingId), Pile(u64) }

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Dest { Store(BuildingId), Site(BuildingId), Dam(BuildingId), Spot(Spot) }

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum WorkKind { Build, Mend, Dam, Dig, Gate }

#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum Assignment { Resource(ResourceId, ResourceKind), Area(ResourceKind, Area), Work(BuildingId), Go(Cell), Mend(Option<BuildingId>), Build(Option<BuildingId>) }

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum MoveState { #[default] None, Moving, Arrived, Blocked }

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum Task {
    Idle,
    Rest,
    Move { from: Option<Source>, to: Dest, good: Good, haul: bool },
    Work { site: BuildingId, cell: Option<Cell>, kind: WorkKind, progress: f32 },
    Collect { target: ResourceId, progress: f32 },
    Seed { target: ResourceId, progress: f32 },
    Plant { store: BuildingId, cell: Cell, progress: f32 },
    Watch { lookout: BuildingId },
    Raft { raft: u32, from: BuildingId, to: BuildingId, good: Good, leg: u8 },
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
    #[serde(default)] pub load_ease: f32,
    pub speed: f32,
    pub trail: Vec<(Cell, Gait)>,
    #[serde(skip)] pub history: VecDeque<(f64, Cell, Gait)>,
    pub next_work_probe: f64,
    pub next_home_probe: f64,
    pub source: Option<ResourceId>,
    #[serde(default)] pub queue: VecDeque<Assignment>,
}

#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Courtship { pub partner: BeaverId, pub started: f64, pub met: Option<f64> }

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Body { pub id: BeaverId, pub cell: Cell, pub cause: DeathCause, pub age_days: f32, pub died: f64 }

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct Remains { pub bodies: Vec<Body>, pub leaving: Vec<(Agent, f32)> }

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct Agents {
    pub by_id: BTreeMap<BeaverId, Agent>,
    pub unreachable: BTreeMap<ResourceId, f64>,
    #[serde(skip)] pub(crate) shore: (f64, Vec<u16>),
    pub remains: Remains,
    #[serde(skip)] ages: BTreeMap<BeaverId, f32>,
    #[serde(skip)] pub courting: BTreeMap<BeaverId, Courtship>,
    #[serde(skip)] paired_upto: f64,
    #[serde(skip)] pub fleeing: BTreeSet<BeaverId>,
    #[serde(skip)] pub holed_up: bool,
}

fn carry_task(load: Option<Load>) -> Task {
    load.map_or(Task::Idle, |l| Task::Move { from: None, to: Dest::Store(NO_STORE), good: l.good, haul: false })
}

impl Agents {
    pub fn get(&self, id: BeaverId) -> Option<&Agent> { self.by_id.get(&id) }
    pub fn spawn(&mut self, id: BeaverId, cell: Cell) {
        self.by_id.insert(id, Agent {
            id, cell, step_frac: 0.0, gait: Gait::Walk, task: Task::Idle, assignment: None,
            move_state: MoveState::None, load: None, path: None, inside: false, stuck_ticks: 0,
            load_ease: 0.0, speed: 1.0, trail: Vec::new(), history: VecDeque::new(),
            next_work_probe: 0.0, next_home_probe: 0.0,
            source: None, queue: VecDeque::new(),
        });
    }
    pub fn assign(&mut self, id: BeaverId, a: Option<Assignment>) -> bool {
        let Some(ag) = self.by_id.get_mut(&id) else { return false };
        ag.reset_for(a);
        if a.is_none() { ag.queue.clear(); }
        true
    }
}

impl Agent {
    pub fn is_mender(&self) -> bool { matches!(self.assignment, Some(Assignment::Mend(_))) }
    pub fn work_target(&self) -> Option<BuildingId> {
        match self.assignment { Some(Assignment::Build(b) | Assignment::Mend(b)) => b, _ => None }
    }
    pub(crate) fn clear_target(&mut self, b: BuildingId) -> bool {
        match self.assignment {
            Some(Assignment::Build(Some(t))) if t == b => { self.assignment = Some(Assignment::Build(None)); true }
            Some(Assignment::Mend(Some(t))) if t == b => { self.assignment = Some(Assignment::Mend(None)); true }
            _ => false,
        }
    }
    fn reset_for(&mut self, a: Option<Assignment>) {
        (self.next_work_probe, self.next_home_probe, self.step_frac, self.stuck_ticks) = (0.0, 0.0, 0.0, 0);
        self.assignment = a;
        self.path = None;
        self.trail.clear();
        self.move_state = if matches!(a, Some(Assignment::Go(_))) { MoveState::Moving } else { MoveState::None };
        self.task = carry_task(self.load);
    }

    fn keep_history(&mut self, now: f64) {
        if self.trail.is_empty() {
            let n = self.history.len();
            if n >= 2 && self.history[n - 1].1 == self.cell && self.history[n - 2].1 == self.cell {
                if let Some(back) = self.history.back_mut() { (back.0, back.2) = (now, self.gait); }
            } else if self.history.back().is_none_or(|h| h.1 != self.cell || (now - h.0).abs() > 1e-9) {
                self.history.push_back((now, self.cell, self.gait));
            }
        } else {
            self.history.extend(self.trail.iter().map(|&(c, g)| (now, c, g)));
        }
        while self.history.len() > HISTORY_CELLS { self.history.pop_front(); }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum SpawnError { NoSuchBeaver, AlreadySpawned, CellUnavailable }

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WorkFactors { pub skill: f32, pub skill_x: f32, pub age_x: f32, pub mood_x: f32, pub health_x: f32, pub rest_x: f32 }
impl WorkFactors { pub fn total(&self) -> f32 { self.age_x * self.mood_x * self.health_x * self.rest_x * self.skill_x } }

pub fn skill_pace(skill: f32) -> f32 { WORK_SKILL_BASE + WORK_SKILL_SCALE * skill }

pub fn work_factors(b: &Beaver, job: Job) -> WorkFactors {
    let skill = b.skills[job];
    WorkFactors {
        skill,
        skill_x: if job.skill_speeds() { skill_pace(skill) } else { 1.0 },
        age_x: b.work_weight(),
        mood_x: crate::society::work_speed(b.mood),
        health_x: WORK_HEALTH_BASE + WORK_HEALTH_SCALE * b.health,
        rest_x: crate::growth::rest_x(b, WORK_REST_BASE + WORK_REST_SCALE * b.needs.rest),
    }
}

pub(crate) fn beaver_effort(sim: &SimState, ag: &Agent, job: Job) -> f32 {
    sim.colony.beaver(ag.id).map_or(0.0, |b| work_factors(b, job).total())
}

pub fn site_crew(sim: &SimState, site: BuildingId) -> (usize, f32) {
    let build = Job::with_duty(JobDuty::Build);
    sim.agents.by_id.values()
        .filter(|a| matches!(a.task, Task::Work { site: s, kind: WorkKind::Build, .. } if s == site))
        .fold((0, 0.0), |(n, rate), a| (n + 1, rate + LABOUR_PER_DAY * beaver_effort(sim, a, build)))
}

pub fn is_store(kind: BuildingKind) -> bool { !kind.spec().stores.is_empty() }
pub fn accepts(kind: BuildingKind, good: Good) -> bool { kind.spec().stores.contains(&good) }
pub fn frozen(sim: &SimState) -> bool { !sim.sandbox && sim.water.sky.at(sim.time_days).ground_frozen() }
pub fn tools_at_hand(sim: &SimState, kind: ResourceKind) -> bool {
    kind.row().needs.is_none_or(|n| sim.buildings.buildings.iter().any(|b| b.kind == n && b.is_complete()))
}
pub(crate) fn has_lodge(sim: &SimState) -> bool { sim.buildings.buildings.iter().any(|b| b.kind.is_home() && b.is_complete()) }

pub fn within_shore(sim: &SimState, role: Job, c: Cell) -> bool {
    let Some(&max) = sim.work.shore_limits.get(&role) else { return true };
    sim.agents.shore.1.get(c.1 * sim.nav.surface.width() + c.0).is_none_or(|&d| d <= max)
}

pub fn colony_home(sim: &SimState) -> Option<Cell> {
    let built = |k: BuildingKind| sim.buildings.buildings.iter().find(|b| b.kind == k && b.is_complete()).map(|b| (b.pos.0 + 1, b.pos.1 + 1));
    BuildingKind::store_for(crate::dams::material()).and_then(built)
        .or_else(|| built(BuildingKind::home()))
        .or(sim.work.supplies)
        .or_else(|| sim.colony.beavers.first().map(|b| (b.pos.0.max(0.0) as usize, b.pos.1.max(0.0) as usize)))
}

pub(crate) fn home_area(sim: &SimState) -> Option<(Cell, Area)> {
    let h = colony_home(sim)?;
    let r = GATHER_RADIUS / 2;
    Some((h, Area { min: (h.0.saturating_sub(r), h.1.saturating_sub(r)), max: (h.0 + r, h.1 + r) }))
}

pub(crate) fn wake(sim: &mut SimState, who: BeaverId) {
    if let Some(ag) = sim.agents.by_id.get_mut(&who) { ag.next_work_probe = 0.0; }
}

pub(crate) fn wake_idle(sim: &mut SimState) {
    for ag in sim.agents.by_id.values_mut() {
        if matches!(ag.task, Task::Idle | Task::Rest) || matches!(ag.assignment, Some(Assignment::Build(_) | Assignment::Mend(_))) {
            ag.next_work_probe = 0.0;
        }
    }
}

pub(crate) fn has_good_source(sim: &SimState, good: Good) -> bool {
    crate::stock::holders(sim, good).iter().any(|&h| sim.buildings.stores.stock_of(h, good) > WORK_EPS)
        || sim.work.piles.items.iter().any(|p| p.good == good && p.amount > WORK_EPS && !crate::predators::in_view(sim, p.cell))
}

pub(crate) fn is_site(sim: &SimState, p: &Workplace) -> bool {
    p.is_open_site() && sim.buildings.building(p.building).is_some_and(|b| {
        !b.is_complete() || sim.maintenance.jobs.get(&b.id).is_some_and(|&r| r != crate::maintenance::Refit::Repair)
    })
}

pub(crate) fn place_open(sim: &SimState, p: &Workplace) -> bool {
    !p.paused && sim.buildings.building(p.building).is_some_and(|b| {
        (b.operational() || is_site(sim, p)) && !sim.buildings.unusable(b, &sim.water.depth)
    })
}

pub(crate) fn place_needs_builders(sim: &SimState, b: BuildingId) -> bool {
    sim.work.place(b).is_some_and(|p| place_open(sim, p) && (is_site(sim, p) || crate::dams::has_work_for(sim, b, false) || crate::canals::has_work_for(sim, b, false)))
}

pub(crate) fn place_needs_menders(sim: &SimState, b: BuildingId) -> bool {
    if sim.work.place(b).is_some_and(|p| p.paused) { return false; }
    let Some(bd) = sim.buildings.building(b) else { return false };
    !sim.buildings.unusable(bd, &sim.water.depth) && (crate::maintenance::needs_mending(sim, bd)
        || (sim.maintenance.jobs.get(&b).is_some_and(|&r| r != crate::maintenance::Refit::Repair) && sim.work.place(b).is_some_and(|p| p.is_open_site()))
        || crate::dams::has_work_for(sim, b, true)
        || crate::canals::has_work_for(sim, b, true))
}

pub(crate) fn mark_mending(sim: &mut SimState, b: BuildingId) {
    if sim.buildings.building(b).is_some_and(|x| crate::maintenance::needs_mending(sim, x)) {
        sim.maintenance.mending.insert(b);
    }
}

pub fn builders_on(sim: &SimState, b: BuildingId) -> usize {
    sim.agents.by_id.values().filter(|a| a.assignment == Some(Assignment::Build(Some(b)))).count()
}
pub fn menders_on(sim: &SimState, b: BuildingId) -> usize {
    sim.agents.by_id.values().filter(|a| a.assignment == Some(Assignment::Mend(Some(b)))).count()
}
pub fn site_workers(sim: &SimState, b: BuildingId) -> usize {
    builders_on(sim, b) + if sim.maintenance.jobs.contains_key(&b) { menders_on(sim, b) } else { 0 }
}

pub(crate) fn site_stalled(sim: &SimState, p: &Workplace) -> bool {
    p.site.as_ref().is_some_and(|s| !s.labour_open() && s.needed.iter().all(|n| {
        s.missing(n.0) <= WORK_EPS
            || (promised(sim, Dest::Site(p.building), n.0, None) <= WORK_EPS && !has_good_source(sim, n.0))
    }))
}
pub(crate) fn site_waits_on(sim: &SimState, p: &Workplace) -> Option<Good> {
    let s = p.site.as_ref()?;
    site_stalled(sim, p).then(|| s.needed.iter().map(|n| n.0).find(|&g| s.missing(g) > WORK_EPS)).flatten()
}
pub(crate) fn gathering_available(sim: &SimState, a: Assignment) -> bool {
    let from = match a {
        Assignment::Area(_, area) => area.centre(),
        Assignment::Resource(id, _) => sim.resources.get(id).map_or((0, 0), |r| r.cell),
        _ => (0, 0),
    };
    Gather::resolve(sim, a).is_some_and(|j| j.pick(sim, from).is_some())
}

pub(crate) fn go_cell(sim: &SimState, from: Cell, target: Cell, radius: usize, taken: &BTreeSet<Cell>) -> Option<Cell> {
    let ok = |c: Cell| !taken.contains(&c)
        && matches!(sim.nav.surface_at(c), Some(Surface::Land | Surface::Water | Surface::Canal | Surface::Dam | Surface::Thicket | Surface::LodgeEntrance | Surface::Lock))
        && (c == from || sim.nav.may_reach(from, [c]));
    if ok(target) { return Some(target); }
    for r in 1..=radius {
        let best = (target.1.saturating_sub(r)..=target.1 + r)
            .flat_map(|y| (target.0.saturating_sub(r)..=target.0 + r).map(move |x| (x, y)))
            .filter(|&(x, y)| x.abs_diff(target.0).max(y.abs_diff(target.1)) == r && ok((x, y)))
            .min_by_key(|&c| (dist2(c, target), c));
        if best.is_some() { return best; }
    }
    None
}

pub(crate) fn start_order(sim: &mut SimState, ag: &mut Agent, order: Assignment) -> bool {
    match order {
        Assignment::Go(target) => {
            let Some(cell) = go_cell(sim, ag.cell, target, 8, &BTreeSet::new()) else { return false };
            sim.work.vacate(ag.id);
            ag.reset_for(Some(Assignment::Go(cell)));
            true
        }
        Assignment::Work(b) => {
            if ag.is_mender() && place_needs_menders(sim, b) {
                mark_mending(sim, b);
                sim.work.vacate(ag.id);
                ag.reset_for(Some(Assignment::Mend(Some(b))));
                return true;
            }
            if place_needs_builders(sim, b) {
                sim.work.vacate(ag.id);
                ag.reset_for(Some(Assignment::Build(Some(b))));
                return true;
            }
            if let Some((slot, _, _)) = crate::jobs::empty_slot(sim, b, None)
                && crate::jobs::seat_in_workplace(sim, b, slot, ag.id).is_ok()
            {
                ag.reset_for(None);
                return true;
            }
            if place_needs_menders(sim, b) {
                mark_mending(sim, b);
                sim.work.vacate(ag.id);
                ag.reset_for(Some(Assignment::Mend(Some(b))));
                return true;
            }
            false
        }
        a => {
            if matches!(a, Assignment::Resource(id, _) if sim.resources.get(id).is_none_or(|r| !gatherable(r) || !tools_at_hand(sim, r.kind))) { return false; }
            if matches!(a, Assignment::Area(k, _) if !tools_at_hand(sim, k)) { return false; }
            if let Assignment::Mend(Some(b)) = a { mark_mending(sim, b); }
            sim.work.vacate(ag.id);
            ag.reset_for(Some(a));
            true
        }
    }
}

fn advance_queue(sim: &mut SimState, ag: &mut Agent) -> bool {
    while let Some(order) = ag.queue.pop_front() {
        if start_order(sim, ag, order) { return true; }
    }
    false
}

pub fn pace(nav: &NavGrid, ag: &Agent) -> f32 {
    let base = ag.speed.max(0.1);
    let Some(load) = ag.load.as_ref() else { return base };
    let good = load.good.row();
    let surface = nav.surface_at(ag.cell);
    if good.floats && surface == Some(Surface::Canal) && nav.floats(ag.cell) { return base; }
    let dry = !matches!(surface, Some(Surface::Water | Surface::LodgeEntrance | Surface::Canal | Surface::Lock));
    let raw = if dry { good.land_pace } else { good.pace };
    base * (raw + (1.0 - raw.min(1.0)) * ag.load_ease)
}

pub fn travel_pace(nav: &NavGrid, ag: &Agent) -> f32 {
    match ag.task {
        Task::Raft { leg: 1 | 3 | 5, .. } => crate::raft_landing::RAFT_PACE * ag.speed.max(0.1),
        _ => pace(nav, ag),
    }
}

pub(crate) fn follow(nav: &mut NavGrid, ag: &mut Agent, dt_days: f32) -> bool {
    let p = travel_pace(nav, ag);
    let mut budget = ag.step_frac + dt_days * MOVE_COST_PER_DAY * p;
    let mut moved = false;
    ag.trail.clear();
    loop {
        let Some(path) = ag.path.as_mut() else { break };
        let Some(&(next, gait)) = path.steps.first() else { ag.path = None; break };
        let cost = nav.move_cost(ag.cell, next, p).unwrap_or_else(|| crate::nav::step_length(ag.cell, next));
        ag.gait = gait;
        if budget < cost { ag.step_frac = budget; return moved; }
        if nav.advance(ag.id, ag.cell, next).is_err() { ag.path = None; break; }
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

pub(crate) fn walk(sim: &mut SimState, ag: &mut Agent, dt: f32, find: impl FnOnce(&SimState, &Agent) -> Option<Path>) -> Option<bool> {
    if ag.path.is_none() {
        ag.path = find(sim, ag);
        if ag.path.is_none() {
            if crate::trace::enabled() {
                crate::trace::event(format!("PATH_FAIL {} beaver={} cell=({},{}) task={:?} load={:?}",
                    crate::trace::sim_stamp(), ag.id.0, ag.cell.0, ag.cell.1, ag.task, ag.load.map(|l| l.good)));
            }
            note(ag, false);
            return None;
        }
    }
    let moved = follow(&mut sim.nav, ag, dt);
    note(ag, moved);
    Some(moved)
}

fn walk_beside(sim: &mut SimState, ag: &mut Agent, dt: f32, cells: &[Cell]) -> Option<bool> {
    if cells.is_empty() { ag.path = None; note(ag, false); return None; }
    if cells.iter().any(|c| c.0.abs_diff(ag.cell.0) + c.1.abs_diff(ag.cell.1) <= 1) {
        ag.path = None;
        return Some(true);
    }
    walk(sim, ag, dt, |sim, ag| sim.nav.find_path_beside_any(ag.id, ag.cell, cells)).map(|_| false)
}

fn abandon(ag: &mut Agent) {
    if crate::trace::enabled() {
        crate::trace::event(format!("ABANDON {} beaver={} cell=({},{}) task={:?} load={:?}",
            crate::trace::sim_stamp(), ag.id.0, ag.cell.0, ag.cell.1, ag.task, ag.load.map(|l| (l.good, l.amount))));
    }
    ag.path = None;
    ag.task = carry_task(ag.load);
}

fn dist2(a: Cell, b: Cell) -> usize { a.0.abs_diff(b.0).pow(2) + a.1.abs_diff(b.1).pow(2) }
fn cell_dist(a: Cell, b: Cell) -> f32 { (dist2(a, b) as f32).sqrt() }
fn place_dist(sim: &SimState, from: Cell, b: BuildingId) -> usize { place_cells(sim, b).into_iter().map(|c| dist2(c, from)).min().unwrap_or(usize::MAX) }
fn place_cells(sim: &SimState, id: BuildingId) -> Vec<Cell> {
    if id == SUPPLIES { sim.work.supplies.into_iter().collect() }
    else { sim.buildings.building(id).map(|b| b.cells().collect()).unwrap_or_default() }
}
fn is_dam(sim: &SimState, id: BuildingId) -> bool { sim.buildings.building(id).is_some_and(|b| b.dam.is_some()) }
fn is_waterworks(sim: &SimState, id: BuildingId) -> bool { sim.buildings.building(id).is_some_and(|b| b.dam.is_some() || b.canal.is_some()) }
fn spend_of(sim: &SimState, ag: &Agent, at: BuildingId) -> Spend {
    if ag.is_mender() || sim.maintenance.jobs.contains_key(&at) { Spend::Upkeep } else { Spend::Build }
}
fn open_stores(sim: &SimState, good: Good) -> impl Iterator<Item = &Building> {
    sim.buildings.buildings.iter().filter(move |b| b.is_complete() && accepts(b.kind, good) && !sim.buildings.unusable(b, &sim.water.depth))
}
fn missing_at(sim: &SimState, to: Dest, good: Good) -> f32 {
    match to {
        Dest::Site(site) => sim.work.place(site).and_then(|p| p.site.as_ref()).map_or(0.0, |s| s.missing(good)),
        Dest::Spot(spot) => crate::supply::missing(sim, spot, good),
        Dest::Dam(_) | Dest::Store(_) => 0.0,
    }
}
fn drop_cells(sim: &SimState, to: Dest) -> (BuildingId, Vec<Cell>) {
    match to {
        Dest::Spot(spot) if is_waterworks(sim, spot.building) => (spot.building, vec![spot.cell]),
        Dest::Spot(Spot { building: b, .. }) | Dest::Site(b) | Dest::Dam(b) | Dest::Store(b) => (b, place_cells(sim, b)),
    }
}

fn route_beside(sim: &SimState, who: BeaverId, from: Cell, cells: &[Cell]) -> Option<(f32, Cell, Path)> {
    if cells.iter().any(|c| c.0.abs_diff(from.0) + c.1.abs_diff(from.1) == 1) {
        return Some((0.0, from, Path { steps: Vec::new(), cost: 0.0 }));
    }
    let path = sim.nav.find_path_beside_any(who, from, cells)?;
    let end = path.steps.last().map_or(from, |s| s.0);
    Some((path.cost, end, path))
}

fn nearest_by_route<K: Copy + Ord>(sim: &SimState, ag: &Agent, to: &[Cell], places: impl Iterator<Item = (K, Vec<Cell>)>) -> Option<(K, Path)> {
    let steps = |a: &[Cell], b: &[Cell]| a.iter().flat_map(|p| b.iter().map(move |q| p.0.abs_diff(q.0).max(p.1.abs_diff(q.1)))).min().unwrap_or(0).saturating_sub(1);
    let mut ranked: Vec<(f32, K, Vec<Cell>)> = places.map(|(k, cells)| {
        let on = if to.is_empty() { 0.0 } else { sim.nav.minimum_cost(steps(&cells, to)) };
        (sim.nav.minimum_cost(steps(&[ag.cell], &cells)) + on, k, cells)
    }).collect();
    ranked.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
    let mut best: Option<(f32, K, Path)> = None;
    for (bound, k, cells) in ranked {
        if best.as_ref().is_some_and(|(cost, _, _)| bound > *cost) { break; }
        let Some((there, at, path)) = route_beside(sim, ag.id, ag.cell, &cells) else { continue };
        let on = if to.is_empty() { 0.0 } else { let Some((c, _, _)) = route_beside(sim, ag.id, at, to) else { continue }; c };
        let cost = there + on;
        if best.as_ref().is_none_or(|(c, b, _)| cost.total_cmp(c).then(k.cmp(b)).is_lt()) { best = Some((cost, k, path)); }
    }
    best.map(|(_, k, p)| (k, p))
}

fn find_source(sim: &SimState, ag: &Agent, to: &[Cell], good: Good, want: f32) -> Option<Source> {
    let mut sources: Vec<(Source, f32, Vec<Cell>)> = crate::stock::holders(sim, good).into_iter()
        .filter(|&id| id == SUPPLIES || !crate::predators::building_in_view(sim, id))
        .filter_map(|id| { let c = place_cells(sim, id); (!c.is_empty()).then(|| (Source::Store(id), sim.buildings.stores.stock_of(id, good), c)) })
        .collect();
    sources.extend(sim.work.piles.items.iter()
        .filter(|p| p.good == good && !crate::predators::in_view(sim, p.cell))
        .map(|p| (Source::Pile(p.id), p.amount, vec![p.cell])));
    let pick = |min: f32| nearest_by_route(sim, ag, to, sources.iter().filter(|s| s.1 >= min.max(WORK_EPS)).map(|s| (s.0, s.2.clone()))).map(|(s, _)| s);
    pick(want).or_else(|| pick(WORK_EPS))
}

fn home_of(sim: &SimState, who: BeaverId) -> Option<&Building> {
    let id = sim.colony.beaver(who)?.lodge?;
    sim.buildings.building(id).filter(|b| b.is_complete())
}

fn route_home(sim: &SimState, who: BeaverId, from: Cell) -> Option<Path> {
    let cells = |b: &Building| b.cells().collect::<Vec<_>>();
    match home_of(sim, who) {
        Some(b) => sim.nav.find_path_onto_any(who, from, &cells(b)),
        None => sim.buildings.buildings.iter().filter(|b| b.is_home_ready())
            .filter_map(|b| sim.nav.find_path_onto_any(who, from, &cells(b)))
            .min_by(|a, b| a.cost.total_cmp(&b.cost)),
    }
}

#[allow(clippy::too_many_arguments)]
fn log_load(sim: &SimState, ag: &Agent, what: &str, at: BuildingId, good: Good, offered: f32, moved: f32, detail: &str) {
    if !crate::trace::enabled() { return; }
    let kind = sim.buildings.building(at).map(|b| format!("{:?}", b.kind)).unwrap_or_else(|| if at == SUPPLIES { "Supplies".into() } else { "none".into() });
    crate::trace::event(format!("LOAD {what} day={:.4} step={} beaver={} good={good:?} offered={offered:.3} moved={moved:.3} building={} kind={kind} cell=({},{}) task={:?} stock_after={:.2} {detail}",
        sim.time_days, sim.steps, ag.id.0, at.0, ag.cell.0, ag.cell.1, ag.task, sim.buildings.stores.amount(good)));
}

fn producer(sim: &SimState, ag: &Agent) -> Producer {
    match (sim.work.slot_of(ag.id), ag.assignment) {
        (Some((b, _, _)), _) => Producer::Building(b),
        (None, Some(_)) => Producer::Order,
        (None, None) => Producer::Other,
    }
}

fn stow(sim: &mut SimState, owner: BuildingId, load: Load, who: Producer) -> Result<f32, crate::buildings::StoresError> {
    if load.booked { sim.buildings.stores.put_kind_at(owner, load.good, load.kind(), load.amount) }
    else { sim.buildings.stores.produce_kind_at(owner, load.good, load.kind(), load.amount, who) }
}

fn drop_load(sim: &mut SimState, ag: &mut Agent, why: &str) {
    if let Some(load) = ag.load.take() {
        sim.work.piles.drop_load(ag.cell, load.good, load.amount, sim.time_days, load.booked, load.species);
        log_load(sim, ag, "DROP_GROUND", NO_STORE, load.good, load.amount, load.amount, why);
    }
    ag.path = None;
    ag.task = Task::Idle;
    note(ag, true);
}

fn record_carry_deeds(sim: &mut SimState, ag: &Agent, at: BuildingId, good: Good, amount: f32, booked: bool) {
    if amount <= 0.0 { return; }
    let kind = sim.buildings.building(at).map_or(BuildingKind::dam(), |b| b.kind);
    let rec = sim.deeds.of(ag.id);
    *rec.carried.entry((good, kind)).or_default() += amount;
    if booked { rec.hauled += amount; }
}

fn promised(sim: &SimState, to: Dest, good: Good, except: Option<BeaverId>) -> f32 {
    sim.agents.by_id.values().filter(|a| except != Some(a.id)).map(|a| match (&a.task, a.load) {
        (Task::Move { to: t, .. }, Some(l)) if *t == to && l.good == good => l.amount,
        (Task::Move { from: Some(Source::Pile(pid)), to: t, good: g, .. }, None) if *t == to && *g == good => {
            sim.work.piles.get(*pid).filter(|p| p.good == good).map_or(0.0, |p| p.amount.min(load_cap(sim, a.id, good)))
        }
        (Task::Move { to: t, good: g, .. }, None) if *t == to && *g == good => load_cap(sim, a.id, good),
        _ => 0.0,
    }).sum()
}

#[allow(clippy::too_many_arguments)]
fn step_move(sim: &mut SimState, ag: &mut Agent, dt: f32, duty: &Duty, from: Option<Source>, to: Dest, good: Good, haul: bool) {
    if ag.load.is_none() {
        let Some(src) = from else { return abandon(ag) };
        let cap = load_cap(sim, ag.id, good);
        let (ok, want, target_id, tag) = match to {
            Dest::Dam(dam) => (
                matches!(duty, Duty::Build(b) | Duty::Mend(b) if *b == dam) && crate::dams::has_work_for(sim, dam, ag.is_mender()),
                crate::dams::sticks_per_step().min(cap), dam, "TAKE_FOR_DAM",
            ),
            Dest::Site(_) | Dest::Spot(_) => {
                let at = drop_cells(sim, to).0;
                let out = missing_at(sim, to, good) - promised(sim, to, good, Some(ag.id));
                let allowed = matches!(duty, Duty::Build(b) | Duty::Mend(b) if *b == at);
                (allowed && out > WORK_EPS, out.min(cap), at, "TAKE_FOR_WORKS")
            }
            Dest::Store(dest) => {
                let ok = if haul {
                    let room = sim.buildings.stores.amount(good) <= sim.buildings.stores.capacity(good) - WORK_EPS;
                    matches!(duty, Duty::Haul) && room && (sim.work.fills.contains(&dest) || crate::stock::balancing(sim))
                } else {
                    matches!(duty, Duty::Collect(_))
                };
                (ok, cap, dest, if haul { "TAKE_FOR_HAUL" } else { "TAKE_GROUND" })
            }
        };
        if !ok || want <= WORK_EPS { return abandon(ag); }
        let (cells, booked, pile_species) = match src {
            Source::Pile(pid) => match sim.work.piles.get(pid).filter(|p| p.good == good) {
                Some(p) => (vec![p.cell], p.booked, p.species),
                None => return abandon(ag),
            },
            Source::Store(id) => {
                let ready = if id == SUPPLIES { sim.work.supplies.is_some() } else { sim.buildings.building(id).is_some_and(|b| b.is_complete()) };
                if id == NO_STORE || !ready { return abandon(ag); }
                (place_cells(sim, id), true, None)
            }
        };
        match walk_beside(sim, ag, dt, &cells) {
            None => return abandon(ag),
            Some(false) => return,
            Some(true) => {}
        }
        let (got, species, at_id) = match src {
            Source::Pile(pid) => (sim.work.piles.take(pid, want), pile_species, target_id),
            Source::Store(id) => {
                let avail = want.min(sim.buildings.stores.stock_of(id, good));
                let (g, k) = if avail > WORK_EPS { sim.buildings.stores.take_kind_from(id, good, avail) } else { (0.0, None) };
                (g, k, id)
            }
        };
        log_load(sim, ag, tag, at_id, good, want, got, &format!("to={}", target_id.0));
        if got <= WORK_EPS { return abandon(ag); }
        ag.load = Some(Load { species, ..Load::new(good, got, booked) });
        note(ag, true);
        ag.path = None;
        ag.task = match to {
            Dest::Dam(dam) => {
                let taken = crate::dams::claimed(sim, dam, ag.id);
                match crate::dams::next_cell(sim, dam, ag.cell, &taken, ag.is_mender()) {
                    Some(cell) => Task::Work { site: dam, cell: Some(cell), kind: WorkKind::Dam, progress: 0.0 },
                    None => carry_task(ag.load),
                }
            }
            _ => Task::Move { from: None, to, good, haul },
        };
        return;
    }

    let load = ag.load.unwrap();
    match to {
        Dest::Dam(dam) => {
            let taken = crate::dams::claimed(sim, dam, ag.id);
            ag.task = match crate::dams::next_cell(sim, dam, ag.cell, &taken, ag.is_mender()) {
                Some(cell) if load.good == crate::dams::material() => Task::Work { site: dam, cell: Some(cell), kind: WorkKind::Dam, progress: 0.0 },
                _ => carry_task(ag.load),
            };
        }
        Dest::Site(_) | Dest::Spot(_) => {
            if missing_at(sim, to, load.good) <= WORK_EPS { ag.task = carry_task(ag.load); return; }
            let (at, cells) = drop_cells(sim, to);
            match walk_beside(sim, ag, dt, &cells) {
                None => return abandon(ag),
                Some(false) => return,
                Some(true) => {}
            }
            let put = match to {
                Dest::Site(site) => sim.work.deliver(site, load.good, load.amount).unwrap_or(0.0),
                Dest::Spot(spot) => crate::supply::deposit(sim, spot, load.good, load.amount),
                _ => 0.0,
            };
            log_load(sim, ag, "DELIVER_WORKS", at, load.good, load.amount, put, "");
            if put > 0.0 {
                let spend = spend_of(sim, ag, at);
                let use_kind = if spend == Spend::Upkeep { Use::Repairs } else { Use::Sites };
                sim.buildings.stores.book_delivery(load.good, put, (!load.booked).then(|| producer(sim, ag)), use_kind);
                crate::works::note(sim, at, spend, 0.0, Some((load.good, put)));
            }
            record_carry_deeds(sim, ag, at, load.good, put, load.booked);
            note(ag, true);
            ag.load = (load.amount - put > WORK_EPS).then_some(Load { amount: load.amount - put, ..load });
            ag.path = None;
            ag.task = carry_task(ag.load);
        }
        Dest::Store(dest) => {
            if sim.buildings.stores.space(load.good) <= EPS { return drop_load(sim, ag, "stores full"); }
            let target = if open_stores(sim, load.good).any(|b| b.id == dest && !crate::predators::building_in_view(sim, b.id)) {
                dest
            } else {
                let clear: Vec<_> = open_stores(sim, load.good).filter(|b| !crate::predators::building_in_view(sim, b.id)).map(|b| (b.id, b.cells().collect::<Vec<_>>())).collect();
                if clear.is_empty() && open_stores(sim, load.good).next().is_some() { ag.path = None; return note(ag, false); }
                let Some((id, path)) = nearest_by_route(sim, ag, &[], clear.into_iter()) else { return drop_load(sim, ag, "no way to a store") };
                (ag.task, ag.path) = (Task::Move { from: None, to: Dest::Store(id), good: load.good, haul }, Some(path));
                id
            };
            let cells = place_cells(sim, target);
            match walk_beside(sim, ag, dt, &cells) {
                None => return abandon(ag),
                Some(false) => return,
                Some(true) => {}
            }
            let who = producer(sim, ag);
            let res = if haul { sim.buildings.stores.put_kind_at(target, load.good, load.kind(), load.amount) } else { stow(sim, target, load, who) };
            let taken = *res.as_ref().unwrap_or(&0.0);
            record_carry_deeds(sim, ag, target, load.good, taken, haul);
            log_load(sim, ag, if haul { "HAUL_STORE" } else { "DELIVER_STORE" }, target, load.good, load.amount, taken, "");
            match res {
                Ok(got) if got >= load.amount - WORK_EPS => { ag.load = None; ag.task = Task::Idle; note(ag, true); }
                Ok(got) if got > WORK_EPS => { ag.load = Some(Load { amount: load.amount - got, ..load }); drop_load(sim, ag, "store filled"); }
                _ => drop_load(sim, ag, "no room in the stores"),
            }
        }
    }
}

fn gatherable(r: &Resource) -> bool {
    match r.kind.behaviour() {
        Behaviour::Tree => crate::forestry::fellable(r),
        Behaviour::Felled => r.amount > 0.0 || r.branches > 0.0,
        Behaviour::FoodPlant => !r.seedling && r.amount >= crate::resources::PLANT_BARE,
        _ => r.amount > 0.0,
    }
}

pub(crate) fn worth_fetching(sim: &SimState, good: Good) -> bool {
    sim.buildings.stores.space(good) > EPS || open_stores(sim, good).next().is_none()
}

fn drop_spots(sim: &SimState, good: Good, from: Cell) -> Vec<Cell> {
    let stores: Vec<Cell> = open_stores(sim, good).map(|b| { let (w, h) = b.kind.spec().footprint; (b.pos.0 + w / 2, b.pos.1 + h / 2) }).collect();
    if !stores.is_empty() { return stores; }
    if let Some(c) = sim.work.supplies { return vec![c]; }
    let lodges: Vec<Cell> = sim.buildings.buildings.iter().filter(|b| b.is_home_ready()).map(|b| b.pos).collect();
    if lodges.is_empty() { vec![from] } else { lodges }
}

struct Gather { kinds: &'static [ResourceKind], area: Option<Area>, only: Option<ResourceId>, role: Option<Job>, frozen: bool }

impl Gather {
    fn resolve(sim: &SimState, a: Assignment) -> Option<Self> {
        let f = frozen(sim);
        match a {
            Assignment::Resource(id, _) => sim.resources.get(id).filter(|r| gatherable(r) && tools_at_hand(sim, r.kind))
                .map(|_| Self { kinds: &[], area: None, only: Some(id), role: None, frozen: f }),
            Assignment::Area(kind, area) if tools_at_hand(sim, kind) => Some(Self {
                kinds: kind.gather_sources(), area: Some(area), only: None, role: Some(crate::jobs::gather_role(kind)), frozen: f,
            }),
            _ => None,
        }
    }
    fn for_slot(sim: &SimState, b: BuildingId, slot: usize) -> Option<Self> {
        let p = sim.work.place(b).filter(|p| !p.paused)?;
        let kind = crate::jobs::gather_kind(p.slots.get(slot)?.role).filter(|&k| tools_at_hand(sim, k))?;
        Some(Self { kinds: kind.gather_sources(), area: Some(p.area?), only: None, role: Some(crate::jobs::gather_role(kind)), frozen: frozen(sim) })
    }
    fn forages(&self) -> bool { self.kinds.iter().any(|k| k.is_food_plant()) }
    fn good_of(&self, r: &Resource) -> Good { if r.kind.is_felled() && self.forages() { Good::food() } else { r.kind.good().expect("gathered yields good") } }
    fn permits(&self, r: &Resource) -> bool {
        if self.only.is_some_and(|id| id != r.id) || self.area.is_some_and(|a| !a.contains(r.cell)) || (self.frozen && r.kind.row().freezes) {
            return false;
        }
        let wanted = if self.forages() && r.kind.is_felled() { r.bark > 0.0 } else { gatherable(r) };
        wanted && (self.only.is_some() || self.kinds.contains(&r.kind))
            && (self.only.is_some() || !r.kind.is_tree() || crate::forestry::cuttable(r, crate::forestry::MATURE_AGE))
    }
    fn open(&self, sim: &SimState, r: &Resource, claimed: usize) -> bool {
        self.permits(r)
            && (self.only.is_some() || (claimed < r.kind.row().gatherers && !crate::predators::in_view(sim, r.cell)))
            && self.role.is_none_or(|role| within_shore(sim, role, r.cell))
            && sim.agents.unreachable.get(&r.id).is_none_or(|&until| sim.time_days >= until)
    }
    fn pick(&self, sim: &SimState, from: Cell) -> Option<ResourceId> {
        let mut claims: HashMap<ResourceId, usize> = HashMap::new();
        for a in sim.agents.by_id.values() {
            if let Task::Collect { target, .. } = a.task { *claims.entry(target).or_insert(0) += 1; }
        }
        let rank = |r: &Resource| self.kinds.iter().position(|&k| k == r.kind.row().alike.unwrap_or(r.kind)).unwrap_or(0);
        let mut drops: PerId<Good, Option<Vec<Cell>>> = PerId::default();
        let mut wanted: PerId<Good, Option<bool>> = PerId::default();
        let mut best: Option<((usize, u32, ResourceId), ResourceId)> = None;
        let mut consider = |r: &Resource| {
            if !self.open(sim, r, claims.get(&r.id).copied().unwrap_or(0)) { return; }
            let walk = cell_dist(from, r.cell);
            if best.as_ref().is_some_and(|((k, c, _), _)| (rank(r), (walk * 16.0) as u32) > (*k, *c)) { return; }
            let good = self.good_of(r);
            if !(self.only.is_some() && r.kind.is_tree()) && !*wanted[good].get_or_insert_with(|| worth_fetching(sim, good)) { return; }
            let haul = drops[good].get_or_insert_with(|| drop_spots(sim, good, from)).iter().map(|&d| cell_dist(r.cell, d)).fold(f32::INFINITY, f32::min);
            let key = (rank(r), ((walk + if haul.is_finite() { haul } else { 0.0 }) * 16.0) as u32, r.id);
            if best.as_ref().is_none_or(|(cur, _)| key < *cur) && sim.nav.may_reach_beside(from, r.cell)
            {
                best = Some((key, r.id));
            }
        };
        if let Some(id) = self.only { if let Some(r) = sim.resources.get(id) { consider(r); } }
        else { for r in sim.resources.items() { consider(r); } }
        best.map(|(_, id)| id)
    }
}

fn next_resource(sim: &SimState, kind: ResourceKind, from: Cell, reach: usize) -> Option<ResourceId> {
    let area = Area { min: (from.0.saturating_sub(reach), from.1.saturating_sub(reach)), max: (from.0 + reach, from.1 + reach) };
    Gather::resolve(sim, Assignment::Area(kind, area))?.pick(sim, from)
}

enum Duty { None, Collect(Gather), Build(BuildingId), Haul, Forest(Area), Mend(BuildingId), Sentry(BuildingId) }

fn duty_at(sim: &SimState, b: BuildingId, slot: usize, role: JobDuty) -> Duty {
    if matches!(role, JobDuty::Gather | JobDuty::Haul | JobDuty::Watch | JobDuty::Forest)
        && sim.work.place(b).is_some_and(|p| p.site.is_some())
    { return Duty::None; }
    match role {
        JobDuty::Build if place_needs_builders(sim, b) => Duty::Build(b),
        JobDuty::Mend if place_needs_menders(sim, b) => Duty::Mend(b),
        JobDuty::Gather => Gather::for_slot(sim, b, slot).map_or(Duty::None, Duty::Collect),
        JobDuty::Haul => Duty::Haul,
        JobDuty::Watch if sim.buildings.building(b).is_some_and(|x| x.is_complete()) => Duty::Sentry(b),
        JobDuty::Forest => sim.work.place(b).and_then(|p| p.area).map_or(Duty::None, Duty::Forest),
        _ => Duty::None,
    }
}

pub(crate) fn place_of(sim: &SimState, ag: &Agent) -> Option<(BuildingId, usize, JobDuty)> {
    if let Some((b, slot, r)) = sim.work.slot_of(ag.id) { return Some((b, slot, r.duty())); }
    match ag.assignment {
        Some(Assignment::Build(Some(b))) => Some((b, 0, JobDuty::Build)),
        Some(Assignment::Mend(Some(b))) => Some((b, 0, JobDuty::Mend)),
        _ => None,
    }
}

fn duty_of(sim: &SimState, ag: &Agent, can_work: bool) -> Duty {
    if !can_work { return Duty::None; }
    match place_of(sim, ag) {
        Some((b, slot, role)) => duty_at(sim, b, slot, role),
        None => ag.assignment.and_then(|a| Gather::resolve(sim, a)).map_or(Duty::None, Duty::Collect),
    }
}

fn gate_spots(sim: &SimState, site: BuildingId) -> Vec<Spot> {
    sim.buildings.building(site).map(|b| b.gates().iter().map(|g| Spot { building: site, cell: g.cell }).collect()).unwrap_or_default()
}

fn supply_task(sim: &SimState, ag: &Agent, spots: &[Spot]) -> Option<Task> {
    for &spot in spots {
        if crate::predators::in_view(sim, spot.cell) { continue; }
        for (good, _) in crate::supply::needs(sim, spot) {
            let out = missing_at(sim, Dest::Spot(spot), good) - promised(sim, Dest::Spot(spot), good, Some(ag.id));
            if out <= WORK_EPS { continue; }
            if ag.load.is_some_and(|l| l.good == good) { return Some(Task::Move { from: None, to: Dest::Spot(spot), good, haul: false }); }
            if ag.load.is_some() { continue; }
            if let Some(src) = find_source(sim, ag, &drop_cells(sim, Dest::Spot(spot)).1, good, out.min(load_cap(sim, ag.id, good))) {
                return Some(Task::Move { from: Some(src), to: Dest::Spot(spot), good, haul: false });
            }
        }
    }
    None
}

fn site_fetch_task(sim: &SimState, site: BuildingId, ag: &Agent) -> Option<Task> {
    if crate::predators::building_in_view(sim, site) { return None; }
    let progress = sim.work.place(site)?.site.as_ref()?;
    for &(good, _) in &progress.needed {
        let out = missing_at(sim, Dest::Site(site), good) - promised(sim, Dest::Site(site), good, Some(ag.id));
        if out <= WORK_EPS { continue; }
        let older = sim.work.places.iter().filter(|p| p.building < site)
            .filter_map(|p| p.site.as_ref().map(|s| (p.building, s)))
            .any(|(b, s)| sim.buildings.building(b).is_some_and(|bd| !bd.is_complete()) && s.missing(good) - promised(sim, Dest::Site(b), good, None) > WORK_EPS);
        let in_stock: f32 = crate::stock::holders(sim, good).iter().map(|&h| sim.buildings.stores.stock_of(h, good)).sum();
        if older && in_stock < SITE_RESERVE_LOADS * load_size(good) { continue; }
        if let Some(src) = find_source(sim, ag, &place_cells(sim, site), good, out.min(load_cap(sim, ag.id, good))) {
            return Some(Task::Move { from: Some(src), to: Dest::Site(site), good, haul: false });
        }
    }
    None
}

fn build_task(sim: &SimState, site: BuildingId, ag: &Agent, is_upkeep: bool) -> Option<Task> {
    if is_dam(sim, site) && !sim.maintenance.jobs.contains_key(&site) {
        if !is_upkeep {
            if let Some(cell) = crate::dams::gate_job(sim, site, &crate::dams::gates_claimed(sim, site, ag.id)) {
                return Some(Task::Work { site, cell: Some(cell), kind: WorkKind::Gate, progress: 0.0 });
            }
            if let Some(t) = supply_task(sim, ag, &gate_spots(sim, site)) { return Some(t); }
        }
        let cell = crate::dams::next_cell(sim, site, ag.cell, &crate::dams::claimed(sim, site, ag.id), is_upkeep)?;
        let mat = crate::dams::material();
        if ag.load.is_some_and(|l| l.good == mat) {
            return Some(Task::Work { site, cell: Some(cell), kind: WorkKind::Dam, progress: 0.0 });
        }
        let src = find_source(sim, ag, &[cell], mat, crate::dams::sticks_per_step().min(load_cap(sim, ag.id, mat)))?;
        return Some(Task::Move { from: Some(src), to: Dest::Dam(site), good: mat, haul: false });
    }
    if sim.buildings.building(site).is_some_and(|b| b.canal.is_some()) {
        let job = crate::canals::next_job(sim, site, &crate::canals::claimed(sim, site, ag.id), is_upkeep);
        if let Some((cell, true)) = job { return Some(Task::Work { site, cell: Some(cell), kind: WorkKind::Gate, progress: 0.0 }); }
        if !is_upkeep && let Some(t) = supply_task(sim, ag, &gate_spots(sim, site)) { return Some(t); }
        let (cell, _) = job?;
        return (!frozen(sim)).then_some(Task::Work { site, cell: Some(cell), kind: WorkKind::Dig, progress: 0.0 });
    }
    if sim.buildings.building(site).is_none_or(|b| b.is_complete() && !sim.maintenance.jobs.contains_key(&site)) { return None; }
    let progress = sim.work.place(site)?.site.as_ref()?;
    if progress.labour_open() && !crate::predators::building_in_view(sim, site) {
        return Some(Task::Work { site, cell: None, kind: WorkKind::Build, progress: 0.0 });
    }
    site_fetch_task(sim, site, ag)
}

fn haul_choice(sim: &SimState, ag: &Agent) -> Option<Task> {
    let (from, to, good) = crate::stock::fill_move(sim)
        .or_else(|| crate::stock::balance_move(sim, sim.work.slot_of(ag.id).map(|s| s.0).filter(|b| sim.work.balanced.contains(b))))?;
    if sim.buildings.stores.amount(good) > sim.buildings.stores.capacity(good) - WORK_EPS { return None; }
    if from != SUPPLIES && to != SUPPLIES && !sim.fleet.rafts.is_empty() && sim.buildings.stores.stock_of(from, good) >= 2.0 * load_size(good) {
        if let Some(raft) = crate::raft_landing::free_raft(sim, ag.cell) {
            if sim.fleet.get(raft).is_some_and(|b| crate::raft_landing::joined(sim, b.raft_landing, from)) && crate::raft_landing::joined(sim, from, to) {
                return Some(Task::Raft { raft, from, to, good, leg: 0 });
            }
        }
    }
    Some(Task::Move { from: Some(Source::Store(from)), to: Dest::Store(to), good, haul: true })
}

fn wild_seedling(sim: &SimState, r: &Resource) -> Option<Species> {
    if !r.is_seedling() || sim.forestry.planted.contains_key(&r.id) || (frozen(sim) && r.kind.row().freezes) { return None; }
    r.species.map(Species::Tree).or(r.plant.map(Species::Plant))
}

fn forest_task(sim: &SimState, area: Area, ag: &Agent) -> Option<Task> {
    if let Some(load) = ag.load.filter(|l| l.good == Good::seedling()) {
        return crate::forestry::plant_cell(sim, area, ag.cell, ag.id, load.species).filter(|&c| !crate::predators::in_view(sim, c))
            .map(|cell| Task::Plant { store: NO_STORE, cell, progress: 0.0 });
    }
    if ag.load.is_some() { return None; }
    let mut stores: Vec<_> = open_stores(sim, Good::seedling())
        .filter(|b| sim.buildings.stores.stock_of(b.id, Good::seedling()) >= 1.0 - WORK_EPS && !crate::predators::building_in_view(sim, b.id)).collect();
    stores.sort_by_key(|b| (crate::stock::dist(ag.cell, b), b.id));
    for b in stores {
        let mut kinds = sim.buildings.stores.kinds_at(b.id, Good::seedling());
        kinds.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.cmp(&b.0)));
        for (sp, _) in kinds.into_iter().filter(|(_, a)| *a >= 1.0 - WORK_EPS) {
            if let Some(cell) = crate::forestry::plant_cell(sim, area, ag.cell, ag.id, sp).filter(|&c| !crate::predators::in_view(sim, c)) {
                return Some(Task::Plant { store: b.id, cell, progress: 0.0 });
            }
        }
    }
    if sim.buildings.stores.space(Good::seedling()) < SEEDLINGS_PER_PLANT { return None; }
    let mut counts = [sim.ledger.planted[0] as f32, sim.ledger.planted[1] as f32, sim.ledger.planted[2] as f32];
    for b in open_stores(sim, Good::seedling()) {
        for (k, amt) in sim.buildings.stores.kinds_at(b.id, Good::seedling()) { counts[k.map_or(SeedlingSort::Tree, Species::sort) as usize] += amt; }
    }
    let (now, job) = (sim.time_days, Job::with_duty(JobDuty::Forest));
    sim.resources.items().iter()
        .filter(|r| area.reaches(r.cell) && within_shore(sim, job, r.cell) && !crate::predators::in_view(sim, r.cell)
            && sim.agents.unreachable.get(&r.id).is_none_or(|&u| u <= now)
            && !sim.agents.by_id.values().any(|a| a.id != ag.id && matches!(a.task, Task::Seed { target, .. } if target == r.id)))
        .filter_map(|r| Some((r, wild_seedling(sim, r)?)))
        .min_by(|(a, sa), (b, sb)| (counts[sa.sort() as usize] as i32).cmp(&(counts[sb.sort() as usize] as i32))
            .then_with(|| cell_dist(ag.cell, a.cell).total_cmp(&cell_dist(ag.cell, b.cell))).then(a.id.cmp(&b.id)))
        .filter(|(r, _)| sim.nav.may_reach_beside(ag.cell, r.cell))
        .map(|(r, _)| Task::Seed { target: r.id, progress: 0.0 })
}

fn next_task(sim: &SimState, duty: &Duty, ag: &Agent) -> Option<Task> {
    match duty {
        Duty::None => None,
        Duty::Build(site) => build_task(sim, *site, ag, false),
        Duty::Mend(b) if crate::predators::building_in_view(sim, *b) => None,
        Duty::Mend(b) if sim.buildings.building(*b).is_some_and(|x| crate::maintenance::needs_mending(sim, x)) => {
            if crate::maintenance::mend_ready(sim, *b) {
                Some(Task::Work { site: *b, cell: None, kind: WorkKind::Mend, progress: 0.0 })
            } else {
                supply_task(sim, ag, &crate::supply::mend_spot(sim, *b).into_iter().collect::<Vec<_>>())
            }
        }
        Duty::Mend(b) => build_task(sim, *b, ag, true),
        Duty::Sentry(b) => Some(Task::Watch { lookout: *b }),
        Duty::Forest(area) => forest_task(sim, *area, ag),
        Duty::Haul => haul_choice(sim, ag),
        Duty::Collect(j) => {
            if let Some(id) = ag.source {
                let claimed = sim.agents.by_id.values().filter(|a| a.id != ag.id && matches!(a.task, Task::Collect { target, .. } if target == id)).count();
                if let Some(r) = sim.resources.get(id).filter(|r| j.open(sim, r, claimed)) {
                    if (j.only.is_some() && r.kind.is_tree()) || worth_fetching(sim, j.good_of(r)) { return Some(Task::Collect { target: id, progress: 0.0 }); }
                }
            }
            let good = if j.forages() { Some(Good::food()) } else { j.kinds.first().and_then(|k| k.good()) };
            if j.only.is_none() && let Some(g) = good.filter(|&g| sim.buildings.stores.space(g) > WORK_EPS) {
                let near = j.area.map(|a| Area { min: (a.min.0.saturating_sub(PILE_REACH), a.min.1.saturating_sub(PILE_REACH)), max: (a.max.0 + PILE_REACH, a.max.1 + PILE_REACH) });
                let claimed = |pid: u64| sim.agents.by_id.values().filter(|a| a.id != ag.id && matches!(a.task, Task::Move { from: Some(Source::Pile(p)), to: Dest::Store(_), .. } if p == pid)).count() as f32;
                if let Some(p) = sim.work.piles.items.iter()
                    .filter(|p| p.good == g && near.is_none_or(|a| a.contains(p.cell)) && !crate::predators::in_view(sim, p.cell))
                    .filter(|p| p.amount > claimed(p.id) * load_size(g) + WORK_EPS && open_stores(sim, g).next().is_some() && sim.nav.may_reach_beside(ag.cell, p.cell))
                    .min_by_key(|p| ((cell_dist(ag.cell, p.cell) * 16.0) as u32, p.id))
                {
                    return Some(Task::Move { from: Some(Source::Pile(p.id)), to: Dest::Store(NO_STORE), good: g, haul: false });
                }
            }
            j.pick(sim, ag.cell).map(|target| Task::Collect { target, progress: 0.0 })
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn step_work(sim: &mut SimState, ag: &mut Agent, dt: f32, duty: &Duty, site: BuildingId, cell: Option<Cell>, kind: WorkKind, progress: f32) {
    let is_upkeep = ag.is_mender();
    let on_site = matches!(duty, Duty::Build(b) | Duty::Mend(b) if *b == site);
    match kind {
        WorkKind::Mend => {
            if !matches!(duty, Duty::Mend(b) if *b == site)
                || sim.work.place(site).is_some_and(|p| p.paused)
                || !sim.buildings.building(site).is_some_and(|b| crate::maintenance::needs_mending(sim, b))
            {
                return abandon(ag);
            }
        }
        WorkKind::Build => {
            let open = on_site
                && sim.work.place(site).is_some_and(|p| !p.paused && is_site(sim, p) && p.site.as_ref().is_some_and(|s| s.labour_open()));
            if !open { return abandon(ag); }
        }
        WorkKind::Dam => {
            if ag.load.filter(|l| l.good == crate::dams::material()).is_none() { ag.path = None; ag.task = Task::Idle; return; }
            if !on_site { return abandon(ag); }
            let c = cell.expect("dam cell");
            if !crate::dams::cell_needs_work(sim, site, c, is_upkeep) {
                ag.path = None;
                ag.task = match crate::dams::next_cell(sim, site, ag.cell, &crate::dams::claimed(sim, site, ag.id), is_upkeep) {
                    Some(nc) => Task::Work { site, cell: Some(nc), kind: WorkKind::Dam, progress: 0.0 },
                    None => carry_task(ag.load),
                };
                return;
            }
        }
        WorkKind::Dig | WorkKind::Gate => {
            if !on_site { return abandon(ag); }
            let c = cell.expect("waterworks cell");
            let still = if kind == WorkKind::Gate {
                crate::canals::gate_owner(sim, c) == Some(site) && crate::gates::ready_at(sim, site, c)
            } else {
                crate::canals::cell_needs_work(sim, site, c, is_upkeep)
            };
            if !still { ag.path = None; ag.task = Task::Idle; return; }
        }
    }
    let cells = match cell { Some(c) => vec![c], None => place_cells(sim, site) };
    match walk_beside(sim, ag, dt, &cells) {
        None => return abandon(ag),
        Some(false) => return,
        Some(true) => {}
    }
    note(ag, true);
    let spend = spend_of(sim, ag, site);
    let job = Job::with_duty(if spend == Spend::Upkeep { JobDuty::Mend } else { JobDuty::Build });
    let labour = dt * LABOUR_PER_DAY * beaver_effort(sim, ag, job);
    let bkind = sim.buildings.building(site).map(|b| b.kind);
    let credit = |sim: &mut SimState, k: Option<BuildingKind>, amount: f32| {
        if spend == Spend::Upkeep { sim.deeds.repaired(ag.id, site, k, amount); } else if let Some(k) = k { sim.deeds.worked(ag.id, site, k, amount); }
    };
    crate::works::note(sim, site, spend, labour, None);
    match kind {
        WorkKind::Build => {
            sim.work.add_labour(site, labour).expect("duty checked site");
            credit(sim, bkind, labour);
        }
        WorkKind::Mend => {
            let mended = crate::maintenance::mend(sim, site, labour);
            if mended { credit(sim, bkind, labour); }
            note(ag, mended);
            let done = !sim.buildings.building(site).is_some_and(|b| crate::maintenance::needs_mending(sim, b));
            if done { ag.clear_target(site); }
            if !mended || done { ag.task = Task::Idle; }
        }
        WorkKind::Dam => {
            let (c, load) = (cell.unwrap(), ag.load.unwrap());
            let p = progress + labour;
            if p + WORK_EPS < crate::dams::pack_labour() { ag.task = Task::Work { site, cell, kind, progress: p }; return; }
            let failing = crate::dams::built(sim, c) > 0.0 && crate::dams::integrity(sim, c) < crate::deeds::LEAK_INTEGRITY;
            let src = (!load.booked).then(|| producer(sim, ag));
            let use_kind = if spend == Spend::Upkeep { Use::Repairs } else { Use::Sites };
            let used = crate::dams::pack(sim, site, c, load.amount, src, use_kind);
            log_load(sim, ag, "PACK_DAM", site, load.good, load.amount, used, &format!("dam_cell=({},{})", c.0, c.1));
            if used > 0.0 {
                crate::works::note(sim, site, spend, 0.0, Some((load.good, used)));
                record_carry_deeds(sim, ag, site, load.good, used, load.booked);
                if spend == Spend::Upkeep {
                    sim.deeds.repaired(ag.id, site, Some(BuildingKind::dam()), crate::dams::pack_labour());
                    if failing { sim.deeds.of(ag.id).leaks += 1; }
                } else {
                    sim.deeds.packed(ag.id, site, crate::dams::pack_labour());
                }
            }
            ag.load = (load.amount - used > WORK_EPS).then_some(Load { amount: load.amount - used, ..load });
            ag.task = Task::Idle;
        }
        WorkKind::Gate => {
            let (c, k) = (cell.unwrap(), bkind.unwrap_or_else(BuildingKind::canal));
            match crate::gates::work(sim, site, c, labour) {
                Ok(Some(_)) => {
                    credit(sim, Some(k), crate::canals::GATE_WORK);
                    if spend == Spend::Build { let r = sim.deeds.of(ag.id); r.gates += 1; r.work(site, Some(k)).loads += 1; }
                    ag.task = Task::Idle;
                }
                Ok(None) if !crate::gates::ready_at(sim, site, c) => ag.task = Task::Idle,
                Ok(None) => ag.task = Task::Work { site, cell, kind, progress: 0.0 },
                Err(_) => { note(ag, false); ag.task = Task::Idle; }
            }
        }
        WorkKind::Dig => {
            let (c, k, need) = (cell.unwrap(), bkind.unwrap_or_else(BuildingKind::canal), crate::canals::DIG_WORK);
            let p = progress + labour;
            if p + WORK_EPS < need { ag.task = Task::Work { site, cell, kind, progress: p }; return; }
            let ok = crate::canals::dig(sim, site, c).is_ok();
            if ok {
                credit(sim, Some(k), need);
                if spend == Spend::Build { let r = sim.deeds.of(ag.id); r.canal_cells += 1; r.work(site, Some(k)).loads += 1; }
            }
            note(ag, ok);
            ag.task = Task::Idle;
        }
    }
}

fn try_start_work(sim: &mut SimState, ag: &mut Agent, fit: bool, force: bool) -> bool {
    if !fit { ag.next_work_probe = 0.0; return false; }
    if !force && sim.time_days < ag.next_work_probe { return false; }
    let mut duty = duty_of(sim, ag, fit);
    let mut task = next_task(sim, &duty, ag);
    if task.is_none() && retarget_idle(sim, ag) {
        duty = duty_of(sim, ag, fit);
        task = next_task(sim, &duty, ag);
    }
    ag.next_work_probe = sim.time_days + WORK_PROBE_DAYS;
    match task {
        Some(t) => { ag.path = None; ag.task = t; true }
        None => false,
    }
}

fn tick(sim: &mut SimState, ag: &mut Agent, dt: f32) {
    ag.trail.clear();
    if let Some(b) = sim.colony.beaver(ag.id) {
        ag.speed = crate::growth::move_x(b);
        ag.load_ease = crate::growth::HEAVY_LOAD_EASE * crate::growth::level(b, SkillTrait::HeavyLoads);
    }
    if crate::predators::flee(sim, ag, dt) { ag.source = None; return; }
    if let Some(Assignment::Go(to)) = ag.assignment { ag.source = None; step_go(sim, ag, dt, to); return; }
    if step_court(sim, ag, dt) || step_follow_parent(sim, ag, dt) { ag.source = None; return; }
    let (can_work, tired) = sim.colony.beaver(ag.id).map_or((false, false), |b| (b.can_work(), b.resting));
    let holed = sim.agents.holed_up && has_lodge(sim);
    let fit = can_work && !tired && !holed;
    let was_active = !matches!(ag.task, Task::Idle | Task::Rest);
    let duty = duty_of(sim, ag, fit);
    if (tired || holed) && ag.load.is_none() && !ag.inside
        && !matches!(ag.task, Task::Rest | Task::Move { from: None, to: Dest::Store(_), .. })
        && (holed || !matches!(ag.task, Task::Watch { .. }))
    {
        ag.path = None;
        ag.task = Task::Rest;
    }
    if matches!(duty, Duty::None) && can_work && !tired
        && let Some(Assignment::Resource(_, kind)) = ag.assignment
        && !advance_queue(sim, ag)
    {
        ag.assignment = next_resource(sim, kind, ag.cell, ORDER_REACH).map(|id| Assignment::Resource(id, kind));
        ag.task = carry_task(ag.load);
        ag.path = None;
    }
    if !matches!(ag.task, Task::Idle | Task::Collect { .. } | Task::Move { from: None, to: Dest::Store(_), .. }) { ag.source = None; }

    match ag.task.clone() {
        Task::Idle => {
            ag.path = None;
            let keep = (matches!(duty, Duty::Build(b) | Duty::Mend(b) if is_dam(sim, b) && !sim.maintenance.jobs.contains_key(&b)) && ag.load.is_some_and(|l| l.good == crate::dams::material()))
                || (matches!(duty, Duty::Forest(..)) && ag.load.is_some_and(|l| l.good == Good::seedling()));
            if keep && try_start_work(sim, ag, fit, true) {}
            else if ag.load.is_some() { ag.task = carry_task(ag.load); }
            else if !try_start_work(sim, ag, fit, false) && has_lodge(sim) { ag.task = Task::Rest; }
        }
        Task::Rest => step_rest(sim, ag, dt, fit),
        Task::Move { from, to, good, haul } => step_move(sim, ag, dt, &duty, from, to, good, haul),
        Task::Work { site, cell, kind, progress } => step_work(sim, ag, dt, &duty, site, cell, kind, progress),
        Task::Collect { target, .. } => step_collect(sim, ag, dt, &duty, target),
        Task::Plant { store, cell, progress } => step_plant(sim, ag, dt, &duty, store, cell, progress),
        Task::Seed { target, progress } => step_seed(sim, ag, dt, &duty, target, progress),
        Task::Raft { raft, from, to, good, leg } => step_raft(sim, ag, dt, &duty, raft, from, to, good, leg),
        Task::Watch { lookout } => {
            if !matches!(duty, Duty::Sentry(b) if b == lookout) { ag.path = None; ag.task = Task::Idle; }
            else if match walk_beside(sim, ag, dt, &place_cells(sim, lookout)) { Some(true) => true, Some(false) => false, None => { abandon(ag); false } } {
                note(ag, true);
            }
        }
    }
    if was_active && matches!(ag.task, Task::Idle) { ag.next_work_probe = 0.0; }
}

fn step_go(sim: &mut SimState, ag: &mut Agent, dt: f32, to: Cell) {
    let finish = |sim: &mut SimState, ag: &mut Agent, state: MoveState| {
        (ag.move_state, ag.assignment, ag.path, ag.task) = (state, None, None, Task::Idle);
        advance_queue(sim, ag);
    };
    if ag.cell == to { return finish(sim, ag, MoveState::Arrived); }
    if ag.inside { ag.path = None; }
    ag.task = Task::Idle;
    if walk(sim, ag, dt, |sim, ag| sim.nav.find_path(ag.id, ag.cell, Goal::Cell(to))).is_none() { return finish(sim, ag, MoveState::Blocked); }
    if ag.cell == to { finish(sim, ag, MoveState::Arrived); }
}

fn step_rest(sim: &mut SimState, ag: &mut Agent, dt: f32, fit: bool) {
    if ag.load.is_some() { ag.path = None; ag.task = carry_task(ag.load); return; }
    if try_start_work(sim, ag, fit, false) { return; }
    if ag.inside { ag.path = None; note(ag, true); return; }
    if ag.path.is_none() && sim.time_days < ag.next_home_probe { return; }
    if walk(sim, ag, dt, |sim, ag| route_home(sim, ag.id, ag.cell)).is_none() { ag.next_home_probe = sim.time_days + crate::STEP_DAYS as f64; }
}

fn step_collect(sim: &mut SimState, ag: &mut Agent, dt: f32, duty: &Duty, target: ResourceId) {
    let job = match duty { Duty::Collect(j) => Some(j), _ => None };
    let Some(res) = sim.resources.get(target).filter(|r| job.is_some_and(|j| j.permits(r))).cloned() else {
        ag.path = None;
        let part = ag.load.filter(|l| l.amount < load_cap(sim, ag.id, l.good) - EPS);
        let next = part.and_then(|_| job?.pick(sim, ag.cell)).filter(|&t| t != target);
        ag.task = match next {
            Some(t) => Task::Collect { target: t, progress: part.map_or(0.0, |l| l.amount / load_cap(sim, ag.id, l.good)) },
            None => carry_task(ag.load),
        };
        return;
    };
    match walk_beside(sim, ag, dt, &[res.cell]) {
        None => { sim.agents.unreachable.insert(target, sim.time_days + UNREACHABLE_RETRY_DAYS); ag.task = carry_task(ag.load); return; }
        Some(false) => return,
        Some(true) => {}
    }
    note(ag, true);
    ag.source = Some(target);
    if res.kind.is_tree() {
        if sim.resources.fell(target, dt * beaver_effort(sim, ag, Job::gathering(ResourceKind::tree())) * FELL_PER_DAY).expect("tree checked") {
            sim.nav.set(res.cell, Surface::Land);
            let rec = sim.deeds.of(ag.id);
            *rec.felled.entry(res.species.expect("tree has species")).or_default() += 1;
            if res.age_days >= crate::deeds::GREAT_TREE_DAYS { rec.great_trees += 1; }
        }
        return;
    }
    let bark = res.kind.is_felled() && job.is_some_and(|j| j.forages());
    let good = if bark { Good::food() } else { res.kind.yields()[0] };
    if ag.load.is_none() && !worth_fetching(sim, good) {
        sim.agents.unreachable.insert(target, sim.time_days + UNREACHABLE_RETRY_DAYS);
        ag.task = Task::Idle;
        return;
    }
    if ag.load.is_some_and(|l| l.good != good) { ag.task = carry_task(ag.load); return; }
    let have = ag.load.map_or(0.0, |l| l.amount);
    let left = if bark { res.bark } else if res.kind.is_felled() && res.amount <= 0.0 { res.branches } else { res.amount };
    let (rate, role) = if bark { (FOOD_GATHER_PER_DAY, ResourceKind::food_plant(false).row().role) } else { (res.kind.row().gather_rate, res.kind.row().role) };
    let full = load_cap(sim, ag.id, good);
    let mut want = (dt * beaver_effort(sim, ag, role) * rate).min(full - have);
    if left - want < EPS { want = left; }
    let taken = if bark { sim.resources.strip(target, want).ok().filter(|t| t.1 > 0.0) } else { sim.resources.take(target, want) };
    if let Some((g, got)) = taken {
        let winter = crate::calendar::Season::at(sim.time_days) == crate::calendar::Season::Winter;
        let rec = sim.deeds.of(ag.id);
        rec.gathered[g] += got;
        if g == Good::food() && winter { rec.winter_food += got; }
        if bark { rec.bark += got; }
        else if let (true, Some(p)) = (g == Good::food(), res.plant) { *rec.harvested.entry(p).or_default() += got; }
        let amount = have + got;
        ag.load = Some(Load::new(g, amount, false));
        ag.task = if amount >= full - EPS {
            log_load(sim, ag, "GATHERED", NO_STORE, g, full, amount, &format!("from={:?}@({},{})", res.kind, res.cell.0, res.cell.1));
            carry_task(ag.load)
        } else {
            Task::Collect { target, progress: amount / full }
        };
    } else if ag.load.is_some() {
        ag.task = carry_task(ag.load);
    }
}

fn step_seed(sim: &mut SimState, ag: &mut Agent, dt: f32, duty: &Duty, target: ResourceId, progress: f32) {
    let Duty::Forest(area) = *duty else { return abandon(ag) };
    let Some((cell, species)) = sim.resources.get(target).filter(|r| area.reaches(r.cell)).and_then(|r| Some((r.cell, wild_seedling(sim, r)?))) else {
        ag.path = None; ag.task = Task::Idle; return;
    };
    match walk_beside(sim, ag, dt, &[cell]) {
        None => { sim.agents.unreachable.insert(target, sim.time_days + UNREACHABLE_RETRY_DAYS); ag.task = Task::Idle; return; }
        Some(false) => return,
        Some(true) => {}
    }
    note(ag, true);
    let p = progress + dt * LABOUR_PER_DAY * beaver_effort(sim, ag, Job::with_duty(JobDuty::Forest));
    if p + WORK_EPS < SEED_WORK { ag.task = Task::Seed { target, progress: p }; return; }
    if let Some(i) = sim.resources.items().iter().position(|r| r.id == target) { sim.resources.remove_at(i); }
    sim.deeds.of(ag.id).seedlings += 1;
    ag.load = Some(Load { species: Some(species), ..Load::new(Good::seedling(), SEEDLINGS_PER_PLANT, false) });
    ag.path = None;
    ag.task = carry_task(ag.load);
}

#[allow(clippy::too_many_arguments)]
fn step_plant(sim: &mut SimState, ag: &mut Agent, dt: f32, duty: &Duty, store: BuildingId, cell: Cell, progress: f32) {
    let Duty::Forest(area) = *duty else { return abandon(ag) };
    let Some(load) = ag.load.filter(|l| l.good == Good::seedling()) else {
        let store_ok = sim.buildings.building(store).is_some_and(|b| b.is_complete() && accepts(b.kind, Good::seedling()) && !sim.buildings.unusable(b, &sim.water.depth))
            && !crate::predators::building_in_view(sim, store);
        if ag.load.is_some() || !store_ok {
            return abandon(ag);
        }
        match walk_beside(sim, ag, dt, &place_cells(sim, store)) {
            None => return abandon(ag),
            Some(false) => return,
            Some(true) => {}
        }
        note(ag, true);
        let kind = sim.buildings.stores.kinds_at(store, Good::seedling()).into_iter()
            .filter(|(k, a)| *a >= 1.0 - WORK_EPS && crate::forestry::plantable(sim, cell, *k))
            .max_by(|a, b| a.1.total_cmp(&b.1).then(b.0.cmp(&a.0))).map(|(k, _)| k);
        let (got, species) = match kind {
            Some(k) => (sim.buildings.stores.take_this_kind_from(store, Good::seedling(), k, 1.0), k),
            None => sim.buildings.stores.take_kind_from(store, Good::seedling(), 1.0),
        };
        log_load(sim, ag, "TAKE_SAPLING", store, Good::seedling(), 1.0, got, &format!("species={species:?}"));
        if got > WORK_EPS { ag.load = Some(Load { species, ..Load::new(Good::seedling(), got, true) }); } else { ag.task = Task::Idle; }
        return;
    };
    if !area.reaches(cell) || !crate::forestry::plantable(sim, cell, load.species) {
        ag.path = None;
        ag.task = crate::forestry::plant_cell(sim, area, ag.cell, ag.id, load.species)
            .map_or_else(|| carry_task(ag.load), |c| Task::Plant { store, cell: c, progress: 0.0 });
        return;
    }
    match walk_beside(sim, ag, dt, &[cell]) {
        None => { crate::forestry::mark_unreachable(sim, cell); ag.task = Task::Idle; return; }
        Some(false) => return,
        Some(true) => {}
    }
    note(ag, true);
    let p = progress + dt * LABOUR_PER_DAY * beaver_effort(sim, ag, Job::with_duty(JobDuty::Forest));
    if p + WORK_EPS < PLANT_WORK { ag.task = Task::Plant { store, cell, progress: p }; return; }
    let species = load.species.unwrap_or_else(|| Species::Tree(crate::trees::Tree::all().next().expect("a tree")));
    let id = match species {
        Species::Tree(t) => sim.resources.add_tree(cell, 0.0, t),
        Species::Plant(p) => crate::wetland::plant_sprout(sim, cell, p),
    };
    sim.forestry.planted.insert(id, (ag.id, sim.time_days));
    sim.ledger.planted[species.sort() as usize] += 1;
    sim.deeds.of(ag.id).planted += 1;
    let used = load.amount.min(1.0);
    log_load(sim, ag, "PLANT", NO_STORE, Good::seedling(), load.amount, used, &format!("species={species:?} at=({},{})", cell.0, cell.1));
    sim.buildings.stores.book_used(Good::seedling(), used, Use::Other);
    ag.load = (load.amount - used > WORK_EPS).then_some(Load { amount: load.amount - used, ..load });
    ag.task = Task::Idle;
}

#[allow(clippy::too_many_arguments)]
fn step_raft(sim: &mut SimState, ag: &mut Agent, dt: f32, duty: &Duty, raft: u32, from: BuildingId, to: BuildingId, good: Good, leg: u8) {
    use crate::raft_landing;
    let let_go = |sim: &mut SimState, ag: &mut Agent| {
        let on_water = raft_landing::raft_water(sim, ag.cell);
        if let Some(b) = sim.fleet.get_mut(raft).filter(|b| b.hauler == Some(ag.id)) {
            b.hauler = None;
            if on_water { b.cell = ag.cell; }
        }
        ag.path = None;
        ag.task = carry_task(ag.load);
    };
    let Some(b) = sim.fleet.get(raft).copied() else { return let_go(sim, ag) };
    if !matches!(duty, Duty::Haul) || b.hauler.is_some_and(|h| h != ag.id) { return let_go(sim, ag); }
    let next = |ag: &mut Agent, leg: u8| { ag.path = None; ag.task = Task::Raft { raft, from, to, good, leg }; };
    match leg {
        0 => {
            if let Some(x) = sim.fleet.get_mut(raft) { x.hauler = Some(ag.id); }
            if ag.cell == b.cell { return next(ag, 1); }
            if walk(sim, ag, dt, |sim, ag| sim.nav.find_path(ag.id, ag.cell, Goal::Cell(b.cell))).is_none() { return let_go(sim, ag); }
        }
        1 | 3 | 5 => {
            let dest_id = match leg { 1 => from, 3 => to, _ => b.raft_landing };
            let target = sim.buildings.building(dest_id).map(|x| raft_landing::docks(sim, x)).unwrap_or_default();
            if target.contains(&ag.cell) { return next(ag, leg + 1); }
            if ag.path.as_ref().and_then(|p| p.steps.first()).is_some_and(|st| !raft_landing::raft_water(sim, st.0)) { ag.path = None; }
            if walk(sim, ag, dt, |sim, ag| raft_landing::route(sim, ag.cell, &target)).is_none() { return let_go(sim, ag); }
            if let Some(x) = sim.fleet.get_mut(raft) { x.cell = ag.cell; }
        }
        2 => {
            let want = (raft_landing::RAFT_LOADS * load_cap(sim, ag.id, good)).min(sim.buildings.stores.stock_of(from, good));
            let (got, kind) = if want > WORK_EPS { sim.buildings.stores.take_kind_from(from, good, want) } else { (0.0, None) };
            log_load(sim, ag, "TAKE_FOR_RAFT", from, good, want, got, &format!("to={} raft={raft}", to.0));
            if got > WORK_EPS { ag.load = Some(Load { species: kind, ..Load::new(good, got, true) }); next(ag, 3); } else { next(ag, 5); }
            note(ag, true);
        }
        4 => {
            if let Some(load) = ag.load {
                let who = producer(sim, ag);
                let taken = stow(sim, to, load, who).unwrap_or(0.0);
                log_load(sim, ag, "DELIVER_RAFT", to, load.good, load.amount, taken, &format!("raft={raft}"));
                sim.run.raft_delivered(load.good, taken);
                record_carry_deeds(sim, ag, to, load.good, taken, true);
                sim.deeds.of(ag.id).rafted[load.good] += taken;
                ag.load = (load.amount - taken > WORK_EPS).then(|| Load { amount: load.amount - taken, ..load });
            }
            note(ag, true);
            next(ag, 5);
        }
        _ => let_go(sim, ag),
    }
}

fn step_court(sim: &mut SimState, ag: &mut Agent, dt: f32) -> bool {
    let Some(c) = sim.agents.courting.get(&ag.id).copied() else { return false };
    let now = sim.time_days;
    let sentry = |id: BeaverId| sim.work.slot_of(id).is_some_and(|(_, _, r)| r.duty() == JobDuty::Watch);
    let home = home_of(sim, ag.id).map(|b| b.id);
    if c.met.is_some_and(|m| now >= m + COURT_DAYS) || (c.met.is_none() && now >= c.started + COURT_WAIT_DAYS)
        || !sim.agents.courting.get(&c.partner).is_some_and(|p| p.partner == ag.id) || sentry(ag.id) || sentry(c.partner) || home.is_none()
    {
        sim.agents.courting.remove(&ag.id); ag.path = None; ag.task = Task::Idle; return false;
    }
    if c.met.is_some() { ag.path = None; note(ag, true); return true; }
    let home = home.unwrap();
    if ag.inside {
        walk(sim, ag, dt, |sim, ag| {
            let cells = place_cells(sim, home);
            let ring: Vec<Cell> = cells.iter().flat_map(|&(x, y)| [(x + 1, y), (x.wrapping_sub(1), y), (x, y + 1), (x, y.wrapping_sub(1))])
                .filter(|c| !cells.contains(c) && matches!(sim.nav.surface_at(*c), Some(Surface::Land | Surface::Water | Surface::Canal | Surface::LodgeEntrance))).collect();
            sim.nav.find_path_onto_any(ag.id, ag.cell, &ring)
        });
    } else if match walk_beside(sim, ag, dt, &place_cells(sim, home)) { Some(true) => true, Some(false) => false, None => { abandon(ag); false } } {
        ag.task = Task::Idle;
        if sim.agents.by_id.get(&c.partner).is_some_and(|p| !p.inside && p.cell.0.abs_diff(ag.cell.0).max(p.cell.1.abs_diff(ag.cell.1)) <= 3) {
            for who in [ag.id, c.partner] { if let Some(x) = sim.agents.courting.get_mut(&who) { x.met = Some(now); } }
        }
    }
    true
}

fn step_follow_parent(sim: &mut SimState, ag: &mut Agent, dt: f32) -> bool {
    let Some(b) = sim.colony.beaver(ag.id).filter(|b| b.stage == Stage::Kit && !b.resting) else { return false };
    if sim.work.slot_of(ag.id).is_some() || ag.assignment.is_some() { return false; }
    let home = home_of(sim, ag.id).map(|h| h.pos);
    let Some(target) = [b.mother, b.father].into_iter().flatten().filter_map(|p| sim.agents.by_id.get(&p))
        .find(|p| !p.inside && home.is_some_and(|h| h.0.abs_diff(p.cell.0).max(h.1.abs_diff(p.cell.1)) <= KIT_FOLLOW_RANGE) && !sim.agents.fleeing.contains(&p.id))
        .map(|p| p.cell) else { return false };
    ag.task = Task::Idle;
    if target.0.abs_diff(ag.cell.0).max(target.1.abs_diff(ag.cell.1)) <= 1 { ag.path = None; note(ag, true); return true; }
    if ag.path.as_ref().and_then(|p| p.steps.last()).is_none_or(|s| s.0.0.abs_diff(target.0).max(s.0.1.abs_diff(target.1)) > 2) { ag.path = None; }
    walk(sim, ag, dt, |sim, ag| sim.nav.find_path_beside_any(ag.id, ag.cell, &[target]));
    true
}

pub fn spawn_on(sim: &mut SimState, id: BeaverId, cell: Cell) -> Result<(), SpawnError> {
    if sim.colony.beaver(id).is_none() { return Err(SpawnError::NoSuchBeaver); }
    if sim.agents.by_id.contains_key(&id) { return Err(SpawnError::AlreadySpawned); }
    if matches!(sim.nav.surface_at(cell), None | Some(Surface::Solid)) { return Err(SpawnError::CellUnavailable); }
    sim.agents.spawn(id, cell);
    Ok(())
}

pub fn place_beavers(sim: &mut SimState) {
    let mut nav = std::mem::replace(&mut sim.nav, NavGrid::new(1, 1));
    nav.rebuild(sim);
    sim.nav = nav;
    sync_agents(sim);
}

fn outside_near(sim: &SimState, from: Cell) -> Cell {
    for want in [&[Surface::Land][..], &[Surface::Water, Surface::Canal][..]] {
        for r in 0..=4usize {
            if let Some(c) = (from.1.saturating_sub(r)..=from.1 + r)
                .flat_map(|y| (from.0.saturating_sub(r)..=from.0 + r).map(move |x| (x, y)))
                .find(|&c| sim.nav.surface_at(c).is_some_and(|s| want.contains(&s)))
            { return c; }
        }
    }
    from
}

fn edge_cells(sim: &SimState, from: Cell) -> Vec<Cell> {
    let n = sim.nav.surface.width().min(sim.nav.surface.height());
    let mut out: Vec<Cell> = (0..n).flat_map(|i| [(i, 0), (i, n - 1), (0, i), (n - 1, i)])
        .filter(|&c| matches!(sim.nav.surface_at(c), Some(Surface::Land | Surface::Water | Surface::Canal))).collect();
    out.sort_by_key(|&c| (dist2(c, from), c));
    out.dedup();
    out
}

fn sync_agents(sim: &mut SimState) {
    for b in &sim.colony.beavers { sim.agents.ages.insert(b.id, b.age_days); }
    let since = sim.agents.paired_upto;
    let pairs: Vec<_> = sim.colony.events.iter().rev().take(256).filter_map(|e| match e.event {
        Event::NewFamily { a, b, .. } if e.day > since => Some((e.day, a, b)),
        _ => None,
    }).collect();
    for (day, a, b) in pairs {
        sim.agents.paired_upto = sim.agents.paired_upto.max(day);
        for (who, partner) in [(a, b), (b, a)] { sim.agents.courting.insert(who, Courtship { partner, started: sim.time_days, met: None }); }
    }
    let latest = |sim: &SimState, id: BeaverId| sim.colony.events.iter().rev().take(256).map(|e| &e.event)
        .find(|e| matches!(e, Event::Died { id: x, .. } | Event::Departed { id: x } | Event::Immigrated { id: x } if *x == id)).cloned();
    let gone: Vec<BeaverId> = sim.agents.by_id.keys().copied().filter(|&id| sim.colony.beaver(id).is_none()).collect();
    for id in gone {
        let Some(mut ag) = sim.agents.by_id.remove(&id) else { continue };
        let age = sim.agents.ages.remove(&id).unwrap_or(crate::colony::ADULT_DAYS);
        match latest(sim, id) {
            Some(Event::Died { cause, .. }) => {
                let cell = if ag.inside { outside_near(sim, ag.cell) } else { ag.cell };
                sim.agents.remains.bodies.push(Body { id, cell, cause, age_days: age, died: sim.time_days });
            }
            Some(Event::Departed { .. }) => {
                if let Some(e) = edge_cells(sim, ag.cell).into_iter().take(12).find(|&e| sim.nav.may_reach(ag.cell, [e])) {
                    (ag.inside, ag.load) = (false, None);
                    ag.reset_for(Some(Assignment::Go(e)));
                    sim.agents.remains.leaving.push((ag, age));
                }
            }
            _ => {}
        }
    }
    let missing: Vec<_> = sim.colony.beavers.iter().filter(|b| !sim.agents.by_id.contains_key(&b.id)).map(|b| (b.id, b.pos)).collect();
    for (id, pos) in missing {
        let (cx, cy) = (pos.0.max(0.0) as usize, pos.1.max(0.0) as usize);
        if matches!(latest(sim, id), Some(Event::Immigrated { .. })) {
            let home = outside_near(sim, (cx, cy));
            if let Some(e) = edge_cells(sim, home).into_iter().take(12).find(|&e| !sim.agents.by_id.values().any(|a| a.cell == e) && sim.nav.may_reach(e, [home])) {
                if spawn_on(sim, id, e).is_ok() {
                    if let Some(a) = sim.agents.by_id.get_mut(&id) { (a.assignment, a.move_state) = (Some(Assignment::Go(home)), MoveState::Moving); }
                    continue;
                }
            }
        }
        let free = (0..=SPAWN_RADIUS).flat_map(|r| (cy.saturating_sub(r)..=cy + r)
            .flat_map(move |y| (cx.saturating_sub(r)..=cx + r).map(move |x| (x, y)))
            .filter(move |&(x, y)| x.abs_diff(cx).max(y.abs_diff(cy)) == r))
            .find(|&c| matches!(sim.nav.surface_at(c), Some(Surface::Land | Surface::Water | Surface::Canal)) && !sim.agents.by_id.values().any(|a| a.cell == c));
        if let Some(cell) = free { let _ = spawn_on(sim, id, cell); }
    }
}

fn measure_shore(sim: &mut SimState) {
    let day = sim.time_days.floor();
    if sim.work.shore_limits.is_empty() || (sim.agents.shore.0 == day && !sim.agents.shore.1.is_empty()) { return; }
    let (w, h) = (sim.nav.surface.width(), sim.nav.surface.height());
    let (mut d, mut queue) = (vec![u16::MAX; w * h], VecDeque::new());
    for (i, s) in sim.nav.surface.as_slice().iter().enumerate() {
        if matches!(s, Surface::Water | Surface::Canal) { d[i] = 0; queue.push_back(i); }
    }
    while let Some(i) = queue.pop_front() {
        let (x, y) = ((i % w) as i64, (i / w) as i64);
        for (dx, dy) in [(-1i64, 0), (1, 0), (0, -1), (0, 1), (-1, -1), (1, -1), (-1, 1), (1, 1)] {
            let (nx, ny) = (x + dx, y + dy);
            if nx >= 0 && ny >= 0 && (nx as usize) < w && (ny as usize) < h {
                let j = ny as usize * w + nx as usize;
                if d[j] == u16::MAX { d[j] = d[i] + 1; queue.push_back(j); }
            }
        }
    }
    sim.agents.shore = (day, d);
}

fn tidy(sim: &mut SimState) {
    let alive: BTreeSet<BeaverId> = sim.colony.beavers.iter().map(|b| b.id).collect();
    let dry: BTreeSet<BuildingId> = sim.buildings.buildings.iter().filter(|b| sim.buildings.unusable(b, &sim.water.depth)).map(|b| b.id).collect();
    let standing: BTreeSet<BuildingId> = sim.buildings.buildings.iter().map(|b| b.id).collect();
    let mut freed = Vec::new();
    for p in &mut sim.work.places {
        let refit = sim.maintenance.jobs.get(&p.building).is_some_and(|&r| r != crate::maintenance::Refit::Repair);
        let ruined = sim.buildings.building(p.building).is_some_and(|b| b.is_complete() && !b.operational());
        let closed = p.paused || refit || dry.contains(&p.building) || ruined;
        for s in &mut p.slots {
            if let Some(w) = s.beaver {
                if !alive.contains(&w) || closed {
                    s.beaver = None;
                    if alive.contains(&w) { freed.push(w); }
                }
            }
        }
    }
    let keep_mending: BTreeSet<BuildingId> = sim.maintenance.mending.iter().copied()
        .filter(|&b| menders_on(sim, b) > 0 && sim.buildings.building(b).is_some_and(|x| x.condition < 1.0 - 1e-4))
        .collect();
    sim.maintenance.mending = keep_mending;
    let stale: Vec<(BeaverId, BuildingId)> = sim.agents.by_id.values().filter_map(|ag| match ag.assignment {
        Some(Assignment::Build(Some(b))) if !place_needs_builders(sim, b) => Some((ag.id, b)),
        Some(Assignment::Mend(Some(b))) if !place_needs_menders(sim, b) => Some((ag.id, b)),
        _ => None,
    }).collect();
    for (id, b) in stale {
        if sim.agents.by_id.get_mut(&id).is_some_and(|ag| ag.clear_target(b)) { freed.push(id); }
    }
    for w in freed { wake(sim, w); }
    sim.work.balanced.retain(|b| standing.contains(b));
}

fn reaches_place(sim: &SimState, from: Cell, b: BuildingId) -> bool {
    place_cells(sim, b).iter().any(|&c| sim.nav.may_reach_beside(from, c))
}

fn retarget_idle(sim: &mut SimState, ag: &mut Agent) -> bool {
    let (who, at) = (ag.id, ag.cell);
    let seat = sim.work.slot_of(who);
    let (current, role) = match (seat, ag.assignment) {
        (Some((b, _, r)), _) => (Some(b), r.duty()),
        (None, Some(Assignment::Build(cur))) => (cur, JobDuty::Build),
        (None, Some(Assignment::Mend(cur))) => (cur, JobDuty::Mend),
        (None, Some(Assignment::Area(kind, area))) => return recentre(sim, ag, kind, area),
        _ => return false,
    };
    if !matches!(role, JobDuty::Build | JobDuty::Gather | JobDuty::Mend) { return false; }
    let mut places: Vec<BuildingId> = sim.work.places.iter().map(|p| p.building).collect();
    if role == JobDuty::Mend {
        places.extend(sim.buildings.buildings.iter().filter(|b| crate::maintenance::needs_mending(sim, b) && sim.work.place(b.id).is_none()).map(|b| b.id));
    }
    let crowd = |b: BuildingId| sim.agents.by_id.values().filter(|a| a.id != who && place_of(sim, a).is_some_and(|p| p.0 == b)).count();
    let best = places.into_iter()
        .filter(|&b| Some(b) != current && !sim.buildings.building(b).is_some_and(|x| sim.buildings.unusable(x, &sim.water.depth)) && reaches_place(sim, at, b))
        .filter_map(|b| {
            let slot = match seat {
                Some((_, _, r)) => sim.work.place(b).filter(|p| place_open(sim, p) && p.site.is_none())?.open_seat(|x| x == r)?.0,
                None => 0,
            };
            let has_work = next_task(sim, &duty_at(sim, b, slot, role), ag).is_some();
            let refit = sim.maintenance.jobs.contains_key(&b);
            has_work.then(|| ((std::cmp::Reverse(refit), crowd(b), place_dist(sim, at, b), b), slot))
        })
        .min();
    let Some(((_, _, _, to), slot)) = best else { return false };
    match seat {
        Some(_) => {
            if crate::jobs::seat_in_workplace(sim, to, slot, who).is_err() { return false; }
            ag.assignment = None;
        }
        None if role == JobDuty::Build => ag.assignment = Some(Assignment::Build(Some(to))),
        None => { ag.assignment = Some(Assignment::Mend(Some(to))); mark_mending(sim, to); }
    }
    true
}

fn recentre(sim: &SimState, ag: &mut Agent, kind: ResourceKind, area: Area) -> bool {
    if Gather::resolve(sim, Assignment::Area(kind, area)).is_some_and(|j| j.pick(sim, ag.cell).is_some()) { return false; }
    let Some(r) = next_resource(sim, kind, ag.cell, GATHER_RADIUS).and_then(|id| sim.resources.get(id)) else { return false };
    let ((hw, hh), n, c) = (((area.max.0 - area.min.0) / 2, (area.max.1 - area.min.1) / 2), sim.world.size, r.cell);
    ag.assignment = Some(Assignment::Area(kind, Area { min: (c.0.saturating_sub(hw), c.1.saturating_sub(hh)), max: ((c.0 + hw).min(n - 1), (c.1 + hh).min(n - 1)) }));
    true
}

fn finish_sites(sim: &mut SimState) {
    let done: Vec<BuildingId> = sim.work.places.iter().filter(|p| p.site.as_ref().is_some_and(|s| s.is_done())).map(|p| p.building).collect();
    let mut completed = false;
    for id in done {
        if crate::maintenance::finish(sim, id) { completed = true; continue; }
        let Some(cells) = sim.buildings.building(id).filter(|b| b.status == crate::buildings::Status::Site).map(|b| (b.kind.is_dam(), b.cells().collect::<Vec<_>>())) else { continue };
        if !cells.0 {
            let occ: Vec<_> = sim.agents.by_id.values().filter(|a| cells.1.contains(&a.cell)).map(|a| a.id).collect();
            if !occ.is_empty() {
                let mut exits = Vec::new();
                for &(x, y) in &cells.1 {
                    for c in [x.checked_sub(1).map(|x| (x, y)), y.checked_sub(1).map(|y| (x, y)), Some((x + 1, y)), Some((x, y + 1))].into_iter().flatten() {
                        if !cells.1.contains(&c) && !exits.contains(&c) { exits.push(c); }
                    }
                }
                let mut nav = std::mem::replace(&mut sim.nav, NavGrid::new(1, 1));
                nav.refresh(sim);
                sim.nav = nav;
                let mut ok = true;
                for who in occ {
                    let ag = sim.agents.by_id.get_mut(&who).expect("site occupant");
                    let Some(path) = sim.nav.find_path_onto_any(who, ag.cell, &exits) else { ok = false; break; };
                    for (c, g) in path.steps {
                        if sim.nav.advance(who, ag.cell, c).is_err() { ok = false; break; }
                        (ag.cell, ag.gait) = (c, g);
                        ag.trail.push((c, g));
                    }
                    (ag.path, ag.step_frac, ag.inside, ag.stuck_ticks) = (None, 0.0, sim.nav.surface_at(ag.cell) == Some(Surface::LodgeInside), 0);
                }
                if !ok { continue; }
            }
        }
        let Some(b) = sim.buildings.buildings.iter_mut().find(|b| b.id == id && b.status == crate::buildings::Status::Site) else { continue };
        b.status = crate::buildings::Status::Complete;
        let (area, def) = (b.work_area(), b.kind.spec());
        sim.work.finish_site(id, &def.slots, area).expect("workplace exists");
        for ag in sim.agents.by_id.values_mut() { ag.clear_target(id); }
        completed = true;
    }
    if completed { wake_idle(sim); }
}

pub fn step(sim: &mut SimState, dt_days: f32) {
    if !sim.colony.is_founded() { return; }
    crate::stock::retire_fills(sim);
    let now = sim.time_days;
    sim.work.piles.spoil(now);
    { let mut nav = std::mem::replace(&mut sim.nav, NavGrid::new(1, 1)); nav.refresh(sim); sim.nav = nav; }
    measure_shore(sim);
    let sky = sim.water.sky.at(now);
    sim.agents.holed_up = if sky.bitter() { true } else if sky.feels_c > crate::weather::COME_OUT_FEELS_C { false } else { sim.agents.holed_up };
    sync_agents(sim);
    tidy(sim);
    crate::supply::tidy(sim);

    for id in sim.agents.by_id.keys().copied().collect::<Vec<_>>() {
        let Some(mut ag) = sim.agents.by_id.remove(&id) else { continue };
        tick(sim, &mut ag, dt_days);
        if !ag.trail.is_empty() {
            let rec = sim.deeds.of(id);
            for &(_, g) in &ag.trail { if g == Gait::Swim { rec.swum += 1.0 } else { rec.walked += 1.0 } }
        }
        ag.keep_history(now);
        sim.agents.by_id.insert(id, ag);
    }

    sim.agents.remains.bodies.retain(|b| now - b.died < REMAINS_DAYS);
    let mut leaving = std::mem::take(&mut sim.agents.remains.leaving);
    leaving.retain_mut(|(ag, _)| {
        ag.trail.clear();
        if let Some(Assignment::Go(to)) = ag.assignment { step_go(sim, ag, dt_days, to); }
        ag.keep_history(now);
        ag.assignment.is_some()
    });
    leaving.append(&mut sim.agents.remains.leaving);
    sim.agents.remains.leaving = leaving;
    finish_sites(sim);
}
