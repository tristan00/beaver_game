include!(concat!(env!("OUT_DIR"), "/agents_constants.rs"));
use std::collections::BTreeMap;

use crate::buildings::{Building, BuildingId, BuildingKind, Good};
use crate::colony::BeaverId;
use crate::nav::{Gait, Goal, NavGrid, Path, Surface};
use crate::resources::{Resource, ResourceId, ResourceKind};
use crate::water::Cell;
use crate::work::{Area, Role, EPS as WORK_EPS, LABOUR_PER_DAY};
use crate::SimState;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Load {
    pub good: Good,
    pub amount: f32,
    /// Existing stock stays existing stock if a cancelled job returns it.
    pub booked: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Assignment {
    Resource(ResourceId),
    Area(ResourceKind, Area),
    Slot(BuildingId, usize),
    Go(Cell),
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MoveState {
    #[default]
    None,
    Moving,
    Arrived,
    Blocked,
}

#[derive(Clone, Debug, PartialEq)]
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
}

impl Task {
}

#[derive(Clone, Debug)]
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


#[derive(Clone, Debug, Default)]
pub struct Agents {
    pub by_id: BTreeMap<BeaverId, Agent>,
    pub unreachable: BTreeMap<ResourceId, f64>,
    work_available: BTreeMap<(BuildingId, usize), (f64, bool)>,
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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
        ResourceKind::Plant => r.amount >= PLANT_MIN_TAKE,
        ResourceKind::RiverClay => r.amount >= crate::resources::RIVER_CLAY_MIN_TAKE,
        _ => r.amount > 0.0,
    }
}

pub(crate) fn gathering_available(sim: &SimState, assignment: Assignment) -> bool {
    Job::resolve(sim, assignment).is_some_and(|job| sim.resources.items.iter().any(|r| job.permits(r)))
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
                .filter(|r| gatherable(r))
                .map(|_| Job { kinds: &[], area: None, only: Some(id), min_age: 0.0 }),
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
    fn pick(&self, sim: &SimState, from: Cell) -> Option<ResourceId> {
        let d = |c: Cell| c.0.abs_diff(from.0).pow(2) + c.1.abs_diff(from.1).pow(2);
        let rank = |r: &Resource| self.kinds.iter().position(|&k| k == r.kind).unwrap_or(0);
        let reachable = |r: &Resource| {
            sim.agents.unreachable.get(&r.id).is_none_or(|until| sim.time_days >= *until) && sim.nav.may_reach_beside(from, r.cell)
        };
        // Connectivity is much more expensive than the deterministic rank.
        // A resource that cannot improve the current choice needs no route test.
        let mut best=None;
        let mut consider=|r:&Resource| {
            if !self.permits(r) {return;}
            let key=(rank(r),d(r.cell),r.id);
            if best.as_ref().is_some_and(|(current,_)|key>=*current) {return;}
            if reachable(r) {best=Some((key,r.id));}
        };
        if let Some(id)=self.only {
            if let Some(r)=sim.resources.get(id) {consider(r);}
        } else {
            for r in &sim.resources.items {consider(r);}
        }
        best.map(|(_,id)|id)
    }
}

pub fn is_store(kind: BuildingKind) -> bool {
    Good::ALL.iter().any(|&g| accepts(kind, g))
}

pub(crate) fn accepts(kind: BuildingKind, good: Good) -> bool {
    match kind {
        BuildingKind::Warehouse => {
            let _ = good;
            true
        }
        _ => false,
    }
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
}

fn duty_of(sim: &SimState, ag: &Agent, can_work: bool) -> Duty {
    if !can_work {
        return Duty::None;
    }
    match sim.work.slot_of(ag.id) {
        Some((b, _, Role::Build)) if sim.work.place(b).is_some_and(|p| p.site.is_some() || (!p.paused && (crate::dams::has_work(sim, b) || crate::canals::has_work(sim, b)))) => Duty::Build(b),
        Some((_, _, Role::Haul)) => Duty::Haul,
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
            (Task::Fetch { good: g, then, .. }, None) if *then == site && *g == good => LOAD_SIZE,
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
fn canal_task(sim: &SimState, site: BuildingId, ag: &Agent) -> Option<Task> {
    let taken = crate::canals::claimed(sim, site, ag.id);
    let (cell, gate) = crate::canals::next_job(sim, site, &taken)?;
    Some(if gate { Task::Gate { site, cell, progress: 0.0 } } else { Task::Dig { site, cell, progress: 0.0 } })
}

/// A dam builder carries one load to the lowest unfinished cell. A gate waiting to be built
/// or turned comes first, and needs no load.
fn dam_task(sim: &SimState, dam: BuildingId, ag: &Agent) -> Option<Task> {
    if let Some(cell) = crate::dams::gate_job(sim, dam, &crate::dams::gates_claimed(sim, dam, ag.id)) {
        return Some(Task::Gate { site: dam, cell, progress: 0.0 });
    }
    let taken = crate::dams::claimed(sim, dam, ag.id);
    let cell = crate::dams::next_cell(sim, dam, ag.cell, &taken)?;
    if ag.load.is_some_and(|l| l.good == Good::Sticks) {
        return Some(Task::DamWork { dam, cell, progress: 0.0 });
    }
    let want = crate::dams::sticks_per_step().min(LOAD_SIZE);
    let from = crate::stock::nearest_holding(sim, cell, Good::Sticks, want).or_else(|| crate::stock::nearest_holding(sim, cell, Good::Sticks, WORK_EPS))?;
    Some(Task::Fetch { from, good: Good::Sticks, then: dam, site_delivery: true })
}

fn build_task(sim: &SimState, site: BuildingId, ag: &Agent) -> Option<Task> {
    if is_dam(sim, site) {
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
            let want = outstanding.min(LOAD_SIZE);
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
    match duty {
        Duty::None => None,
        Duty::Collect(j) => j.pick(sim, ag.cell).map(|t| Task::Collect { target: t, progress: 0.0 }),
        Duty::Build(site) => build_task(sim, *site, ag),
        Duty::Haul => site_haul_task(sim, ag).or_else(|| haul_task(sim)).or_else(|| {
            let own = sim.work.slot_of(ag.id).map(|s| s.0).filter(|b| sim.work.balanced.contains(b))?;
            let (from, then, good) = crate::stock::balance_move(sim, Some(own))?;
            Some(Task::Fetch { from, good, then, site_delivery: false })
        }),
    }
}

fn tick(sim: &mut SimState, ag: &mut Agent, dt: f32) {
    // A trail belongs to this step, including steps spent working or resting.
    ag.trail.clear();
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
            let dam_load = matches!(duty, Duty::Build(b) if is_dam(sim, b)) && ag.load.is_some_and(|l| l.good == Good::Sticks);
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
        Task::Carry { dest } if ag.load.is_some_and(|l| sim.work.place(dest).and_then(|p|p.site.as_ref()).is_some_and(|p|p.missing(l.good)>WORK_EPS)) => deliver(sim, ag, dt, dest),
        Task::Carry { dest } => carry(sim, ag, dt, dest),
        Task::Fetch { from, good, then, .. } if is_dam(sim, then) => dam_fetch(sim, ag, dt, &duty, from, good, then),
        Task::Fetch { from, good, then, site_delivery } => fetch(sim, ag, dt, &duty, from, good, then, site_delivery),
        Task::Build { site } => build(sim, ag, dt, &duty, site),
        Task::Haul { to } => haul(sim, ag, dt, to),
    }
}

fn dam_fetch(sim: &mut SimState, ag: &mut Agent, dt: f32, duty: &Duty, from: BuildingId, good: Good, dam: BuildingId) {
    let from_ok = if from == SUPPLIES { sim.work.supplies.is_some() } else { sim.buildings.building(from).is_some_and(|b| b.is_complete()) };
    if !matches!(duty, Duty::Build(b) if *b == dam) || !from_ok || !crate::dams::has_work(sim, dam) {
        return abandon(ag);
    }
    if !approach(sim, ag, dt, from) {
        return;
    }
    note(ag, true);
    let want = crate::dams::sticks_per_step().min(LOAD_SIZE).min(sim.work.stock_of(from, good));
    let got = if want > WORK_EPS { sim.buildings.stores.take_from(from, good, want) } else { 0.0 };
    if got <= WORK_EPS {
        return abandon(ag);
    }
    ag.load = Some(Load { good, amount: got, booked: true });
    let taken = crate::dams::claimed(sim, dam, ag.id);
    ag.task = match crate::dams::next_cell(sim, dam, ag.cell, &taken) {
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
    if !crate::dams::cell_needs_work(sim, dam, cell) {
        let taken = crate::dams::claimed(sim, dam, ag.id);
        ag.path = None;
        ag.task = match crate::dams::next_cell(sim, dam, ag.cell, &taken) {
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
        crate::canals::cell_needs_work(sim, site, cell)
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
    let done = |ag: &mut Agent, sim: &mut SimState| {
        ag.assignment = None;
        ag.path = None;
        ag.task = Task::Idle;
        sim.work.player_orders.remove(&ag.id);
    };
    if ag.cell == to {
        ag.move_state = MoveState::Arrived;
        return done(ag, sim);
    }
    if ag.inside || ag.path.is_none() {
        ag.path = sim.nav.find_path(ag.id, ag.cell, Goal::Cell(to));
        if ag.path.is_none() {
            ag.move_state = MoveState::Blocked;
            return done(ag, sim);
        }
    }
    ag.task = Task::Idle;
    let moved = follow(&mut sim.nav, ag, dt);
    note(ag, moved);
}

fn abandon(ag: &mut Agent) {
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
                    || (sim.work.balanced.contains(&from) && sim.work.balanced.contains(&then)))
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
        let want = LOAD_SIZE.min(outstanding).min(sim.work.stock_of(from, good)).min(sim.buildings.stores.amount(good));
        if want <= WORK_EPS {
            return abandon(ag);
        }
        let want = sim.buildings.stores.take_from(from, good, want);
        ag.load = Some(Load { good, amount: want, booked:true });
        ag.task = Task::Carry { dest: then };
    } else {
        let got = sim.buildings.stores.take_from(from, good, LOAD_SIZE);
        if got <= WORK_EPS {
            return abandon(ag);
        }
        ag.load = Some(Load { good, amount: got, booked:true });
        ag.task = Task::Haul { to: then };
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
        let put=sim.buildings.stores.put_at(to,load.good,load.amount).unwrap_or(0.0);
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


fn source_of(good: Good) -> ResourceKind {
    match good {
        Good::Sticks => ResourceKind::Tree,
        Good::Clay => ResourceKind::MudBank,
        Good::Food => ResourceKind::Plant,
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

pub(crate) fn site_material_run(sim: &SimState, site: BuildingId, pos: Cell) -> Option<Assignment> {
    let open: Vec<&crate::work::SiteProgress> =
        sim.work.places.iter().filter(|p| is_site(sim, p)).filter_map(|p| p.site.as_ref()).collect();
    let progress = sim.work.place(site)?.site.as_ref()?;
    let raw = |g: Good| matches!(g, Good::Sticks | Good::Clay);
    let good = progress.needed.iter().map(|n| n.0).find(|&g| {
        raw(g) && progress.missing(g) > WORK_EPS && open.iter().map(|p| p.missing(g)).sum::<f32>() - sim.buildings.stores.amount(g) > WORK_EPS
    })?;
    let r = GATHER_RADIUS;
    let area = Area { min: (pos.0.saturating_sub(r), pos.1.saturating_sub(r)), max: (pos.0 + r, pos.1 + r) };
    // A missing material is not itself a viable gathering job. Empty areas
    // otherwise reserve the entire workforce forever.
    if !sim.resources.items.iter().any(|item|area.contains(item.cell) && kinds_for(source_of(good)).contains(&item.kind) && gatherable(item)) {return None;}
    Some(Assignment::Area(source_of(good), area))
}

/// Goods beavers have taken out of stores and are still carrying.
pub fn in_hands(sim: &SimState, good: Good) -> f32 {
    sim.agents.by_id.values().filter_map(|a| a.load).filter(|l| l.booked && l.good == good).map(|l| l.amount).sum()
}

/// Gather sticks for a dam when the stores cannot cover the loads still to carry.
fn dam_material_run(sim: &SimState, dam: BuildingId, pos: Cell) -> Option<Assignment> {
    let need = crate::dams::loads_left(sim, dam) as f32 * crate::dams::sticks_per_step();
    if crate::jobs::on_hand(sim, Good::Sticks) >= need.min(LOAD_SIZE * 4.0) {
        return None;
    }
    let r = GATHER_RADIUS / 2;
    let area = Area { min: (pos.0.saturating_sub(r), pos.1.saturating_sub(r)), max: (pos.0 + r, pos.1 + r) };
    sim.resources.items.iter().any(|i| area.contains(i.cell) && kinds_for(ResourceKind::Tree).contains(&i.kind) && gatherable(i))
        .then_some(Assignment::Area(ResourceKind::Tree, area))
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

/// The colony sends beavers out from home for whatever it is short of.
fn colony_runs(sim: &SimState) -> Vec<(Role, Assignment, Cell)> {
    let Some(home) = colony_home(sim) else { return Vec::new() };
    let workers = sim.colony.beavers.iter().filter(|b| b.can_work()).count();
    let r = GATHER_RADIUS / 2;
    let area = Area { min: (home.0.saturating_sub(r), home.1.saturating_sub(r)), max: (home.0 + r, home.1 + r) };
    let mut out = Vec::new();
    for (good, kind, share) in [(Good::Food, ResourceKind::Plant, 0.5), (Good::Sticks, ResourceKind::Tree, 0.34), (Good::Clay, ResourceKind::MudBank, 0.2)] {
        if crate::jobs::on_hand(sim, good) >= crate::jobs::stock_target(sim, good) {
            continue;
        }
        let kinds = kinds_for(kind);
        if !sim.resources.items.iter().any(|i| area.contains(i.cell) && kinds.contains(&i.kind) && gatherable(i)) {
            continue;
        }
        let n = ((workers as f32 * share).ceil() as usize).max(1);
        out.extend(std::iter::repeat_n((Role::Collect(kind), Assignment::Area(kind, area), home), n));
    }
    out
}

struct Opening {
    duty: crate::work::ManagedDuty,
    key: (usize, std::cmp::Reverse<crate::work::Priority>, usize, BuildingId),
    at: Cell,
    role: Role,
}

fn tidy(sim: &mut SimState) {
    let alive: std::collections::BTreeSet<BeaverId> = sim.colony.beavers.iter().map(|b| b.id).collect();
    let stalled: Vec<BuildingId> = sim.work.places.iter().filter(|p| is_site(sim, p) && site_stalled(sim, p)).map(|p| p.building).collect();
    let dry: Vec<BuildingId> = sim.buildings.buildings.iter().filter(|b| sim.buildings.stranded(b, &sim.water.depth)).map(|b| b.id).collect();
    for p in &mut sim.work.places {
        let idle_site = stalled.contains(&p.building);
        let refitting=sim.maintenance.jobs.contains_key(&p.building);
        let ruined=sim.buildings.building(p.building).is_some_and(|b|b.is_complete() && !b.operational());
        let closed = p.paused || (!refitting && (dry.contains(&p.building) || ruined));
        for s in &mut p.slots {
            if s.worker.is_some_and(|w| closed || !alive.contains(&w) || (idle_site && s.role == Role::Build)) {
                s.worker = None;
                s.locked = false;
            }
            if s.worker.is_none() {
                s.locked = false;
            }
        }
    }
    sim.work.player_orders.retain(|id| alive.contains(id));
    let standing: std::collections::BTreeSet<BuildingId> = sim.buildings.buildings.iter().map(|b| b.id).collect();
    sim.work.balanced.retain(|b| standing.contains(b));
}

fn organise(sim: &mut SimState) {
    use crate::work::ManagedDuty;
    let managed: Vec<(BeaverId, ManagedDuty)> = sim.work.managed.iter().map(|(&k, &v)| (k, v)).collect();
    for (id, duty) in managed {
        let same = match duty {
            ManagedDuty::Gather(a) => sim.work.slot_of(id).is_none() && sim.agents.get(id).is_some_and(|ag| ag.assignment == Some(a)),
            ManagedDuty::Slot(b, s) => {
                sim.work.slot_of(id).is_some_and(|(pb, ps, _)| (pb, ps) == (b, s)) && !sim.work.is_locked(id)
            }
        };
        if !same || sim.colony.beaver(id).is_none() || sim.work.player_orders.contains(&id) {
            sim.work.managed.remove(&id);
        }
    }
    let mut openings: Vec<Opening> = Vec::new();
    for p in &sim.work.places {
        let Some(b) = sim.buildings.building(p.building) else { continue };
        let site = is_site(sim, p);
        if p.paused || !(b.operational() || site) || (!site && sim.buildings.stranded(b, &sim.water.depth)) {
            continue;
        }
        let ours = |i: usize, w: Option<BeaverId>| w.is_none() || w.is_some_and(|w| sim.work.managed.get(&w) == Some(&ManagedDuty::Slot(p.building, i)));
        let fixed = p.slots.iter().enumerate().filter(|(i, s)| !ours(*i, s.worker)).count();
        let mut budget = p.max_workers.saturating_sub(fixed);
        let canal_work = !site && crate::canals::has_work(sim, p.building);
        let dam_work = (!site && crate::dams::has_work(sim, p.building)) || canal_work;
        let run = if site { site_material_run(sim, p.building, b.pos) } else if dam_work && !canal_work { dam_material_run(sim, p.building, b.pos) } else { None };
        let prio = std::cmp::Reverse(p.priority);
        let mut runs = run.map_or(0, |_| crate::work::SITE_SLOTS.min(p.max_workers));
        for (i, s) in p.slots.iter().enumerate() {
            let available = if let Some(&(until, ready)) = sim.agents.work_available.get(&(b.id,i)).filter(|v| v.0 > sim.time_days) {
                let _ = until;
                ready
            } else {
                let who = s.worker.unwrap_or(BeaverId(0));
                let _ = who;
                let ready = match s.role {
                    Role::Collect(_) => gathering_available(sim, Assignment::Slot(b.id,i)),

                    _ => true,
                };
                sim.agents.work_available.insert((b.id,i),(sim.time_days+WORK_AVAILABLE_DAYS,ready));
                ready
            };
            if !available { continue; }
            let idle = match s.role {
                Role::Haul => !sim.work.hauls.iter().any(|o|sim.work.stock_of(o.from,o.good)>EPS)
                    && !sim.work.places.iter().filter(|p|is_site(sim,p)).filter_map(|p|p.site.as_ref()).any(|p|p.needed.iter().any(|&(g,_)|p.missing(g)>EPS&&sim.buildings.stores.amount(g)>EPS))
                    && !(sim.work.balanced.contains(&b.id) && crate::stock::balance_move(sim, Some(b.id)).is_some()),
                Role::Collect(kind) => {
                    let good = match kind {ResourceKind::MudBank | ResourceKind::RiverClay => Good::Clay, ResourceKind::Plant => Good::Food, _ => Good::Sticks};
                    sim.buildings.stores.amount(good) >= crate::jobs::stock_target(sim, good)
                },

                _ => false,
            };
            if idle {
                continue;
            }
            let rank = crate::jobs::rank(sim, crate::jobs::role_kind(s.role));
            if let Some(a) = run.filter(|_| runs > 0) {
                runs -= 1;
                let role = Role::Collect(match a { Assignment::Area(k, _) => k, _ => ResourceKind::Tree });
                openings.push(Opening { duty: ManagedDuty::Gather(a), key: (rank, prio, 2 * i, p.building), at: b.pos, role });
            }
            if site && s.role == Role::Build && site_stalled(sim, p) {
                continue;
            }
            if !site && s.role == Role::Build && !dam_work {
                continue;
            }
            if ours(i, s.worker) && budget > 0 {
                budget -= 1;
                openings.push(Opening { duty: ManagedDuty::Slot(p.building, i), key: (rank, prio, 2 * i + 1, p.building), at: b.pos, role: s.role });
            }
        }
    }
    for (i, (role, a, at)) in colony_runs(sim).into_iter().enumerate() {
        let rank = crate::jobs::rank(sim, crate::jobs::role_kind(role));
        openings.push(Opening { duty: ManagedDuty::Gather(a), key: (rank, std::cmp::Reverse(crate::work::Priority::Normal), 2 * i + 1, BuildingId(0)), at, role });
    }
    openings.sort_by_key(|o| o.key);
    let mut counts = [0usize; crate::jobs::RoleKind::ALL.len()];
    for o in &mut openings {
        let role = crate::jobs::role_kind(o.role);
        let i = role as usize;
        counts[i] += 1;
        let urgency = if crate::jobs::rank(sim, role) == 0 { FOOD_URGENCY_WEIGHT }
            else if crate::jobs::material_short(sim, role) { MATERIAL_URGENCY_WEIGHT } else { 1.0 };
        let weight = sim.work.role_weights[i] * urgency;
        o.key.0 = ((counts[i] as f32 * 100.0 / weight) as usize) + sim.work.role_rank(role) * 3;
    }
    openings.sort_by_key(|o| o.key);
    let pool: Vec<(BeaverId, Cell)> = sim
        .colony
        .beavers
        .iter()
        .filter(|b| b.can_work() && !sim.work.player_orders.contains(&b.id))
        .filter_map(|b| sim.agents.get(b.id).map(|a| (b, a)))
        .filter(|(b, a)| sim.work.managed.contains_key(&b.id) || (sim.work.slot_of(b.id).is_none() && a.assignment.is_none()))
        .map(|(b, a)| (b.id, a.cell))
        .collect();
    let mut holder: Vec<Option<usize>> = vec![None; openings.len()];
    let mut used = vec![false; pool.len()];
    for (o, opening) in openings.iter().enumerate() {
        if let Some(k) = (0..pool.len()).find(|&k| !used[k] && sim.work.managed.get(&pool[k].0) == Some(&opening.duty)) {
            used[k] = true;
            holder[o] = Some(k);
        }
    }
    let d2 = |a: Cell, b: Cell| a.0.abs_diff(b.0).pow(2) + a.1.abs_diff(b.1).pow(2);
    for (o, opening) in openings.iter().enumerate() {
        if holder[o].is_some() {
            continue;
        }
        let pick = (0..pool.len())
            .filter(|&k| !used[k])
            .min_by_key(|&k| (d2(pool[k].1, opening.at), pool[k].0));
        if let Some(k) = pick {
            used[k] = true;
            holder[o] = Some(k);
        }
    }
    for o in 0..openings.len() {
        if holder[o].is_some() {
            continue;
        }
        let rank = (openings[o].key.0, openings[o].key.1);
        let from = (o + 1..openings.len())
            .rev()
            .find(|&l| (openings[l].key.0, openings[l].key.1) > rank && holder[l].is_some());
        if let Some(l) = from {
            holder[o] = holder[l].take();
        }
    }
    let mut desired: Vec<(BeaverId, Option<ManagedDuty>)> = pool.iter().map(|(id, ..)| (*id, None)).collect();
    for (o, h) in holder.iter().enumerate() {
        if let Some(k) = *h {
            desired[k].1 = Some(openings[o].duty);
        }
    }
    for &(id, duty) in &desired {
        if sim.work.managed.get(&id).copied() == duty {
            continue;
        }
        if let Some((b, i, _)) = sim.work.slot_of(id).filter(|_| sim.work.managed.contains_key(&id)) {
            let _ = sim.work.unassign(b, i);
        }
        if sim.work.managed.remove(&id).is_some() {
            sim.agents.assign(id, None);
        }
    }
    for (id, duty) in desired {
        let Some(duty) = duty else { continue };
        if sim.work.managed.get(&id) == Some(&duty) {
            continue;
        }
        let ok = match duty {
            ManagedDuty::Gather(a) => sim.agents.assign(id, Some(a)),
            ManagedDuty::Slot(b, i) => sim.work.assign(b, i, id).is_ok(),
        };
        if ok {
            sim.work.managed.insert(id, duty);
        }
    }
}

fn collect(sim: &mut SimState, ag: &mut Agent, dt: f32, duty: Duty, target: ResourceId) {
    let job = match &duty {
        Duty::Collect(j) => Some(j),
        _ => None,
    };
    let res = sim.resources.get(target).filter(|r| job.is_some_and(|j| j.permits(r))).cloned();
    let Some(res) = res else {
        ag.path = None;
        ag.task = if ag.load.is_some() { Task::Carry { dest: NO_STORE } } else { Task::Idle };
        return;
    };
    let near = res.cell.0.abs_diff(ag.cell.0) + res.cell.1.abs_diff(ag.cell.1) <= 1;
    if !near {
        if ag.path.is_none() {
            ag.path = sim.nav.find_path(ag.id, ag.cell, Goal::Beside(res.cell));
            if ag.path.is_none() {
                sim.agents.unreachable.insert(target, sim.time_days + UNREACHABLE_RETRY_DAYS);
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
        ag.task = Task::Idle;
        return;
    }
    if ag.load.is_some_and(|l| l.good != good) {
        ag.task = Task::Carry { dest: NO_STORE };
        return;
    }
    let have = ag.load.map_or(0.0, |l| l.amount);
    let left = if bark { res.bark } else if res.kind == ResourceKind::Felled && res.amount <= 0.0 { res.branches } else { res.amount };
    let rate = if bark || res.kind == ResourceKind::Plant { FOOD_GATHER_PER_DAY } else { GATHER_PER_DAY };
    let job = if bark || res.kind == ResourceKind::Plant { crate::colony::Job::Forager }
        else if res.kind == ResourceKind::Felled { crate::colony::Job::Logger } else { crate::colony::Job::Digger };
    let mut want = (dt * worker_effort(sim, ag, job) * rate).min(LOAD_SIZE - have);
    if left - want < EPS {
        want = left;
    }
    let taken = if bark { sim.resources.strip(target, want).ok().filter(|t| t.1 > 0.0) } else { sim.resources.take(target, want) };
    if let Some((g, got)) = taken {
        if let Some((b, _, _)) = sim.work.slot_of(ag.id) {
            sim.output.record(b, g, got);
        }
        let amount = have + got;
        ag.load = Some(Load { good: g, amount, booked:false });
        ag.task = if amount >= LOAD_SIZE - EPS {
            Task::Carry { dest: NO_STORE }
        } else {
            Task::Collect { target, progress: amount / LOAD_SIZE }
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
        let booked = if load.booked {sim.buildings.stores.put_at(dest,load.good,load.amount)} else {sim.buildings.stores.produce_at(dest,load.good,load.amount,who)};
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
        a.load = Some(Load { good: Good::Clay, amount: 4.0, booked: false });
        a.task = Task::Carry { dest: NO_STORE };
        carry(&mut s, &mut a, 0.01, NO_STORE);
        assert_eq!(s.buildings.stores.amount(Good::Clay), 4.0);
        assert!(a.load.is_none());
        s.buildings.stores.deposit(Good::Clay, 1000.0).unwrap();
        a.load = Some(Load { good: Good::Clay, amount: 4.0, booked: false });
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
        for r in s.resources.items.iter().step_by(3) {s.agents.unreachable.insert(r.id,1.0);}
        for kind in [ResourceKind::Tree,ResourceKind::Plant,ResourceKind::MudBank,ResourceKind::RiverClay] {
            for from in [(5,5),(size/2,size/2),(size-5,size-5)] {
                for area in [None,Some(Area{min:(0,0),max:(size/2,size/2)})] {
                    let job=Job{kinds:kinds_for(kind),area,only:None,min_age:300.0};
                    let reference=s.resources.items.iter().filter(|r|job.permits(r)
                        && s.agents.unreachable.get(&r.id).is_none_or(|&until|s.time_days>=until)
                        && s.nav.may_reach_beside(from,r.cell))
                        .min_by_key(|r|(job.kinds.iter().position(|&k|k==r.kind).unwrap_or(0),r.cell.0.abs_diff(from.0).pow(2)+r.cell.1.abs_diff(from.1).pow(2),r.id)).map(|r|r.id);
                    assert_eq!(job.pick(&s,from),reference);
                }
            }
        }
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
        let taken = if load.booked {sim.buildings.stores.put_at(owner,load.good,load.amount)} else {sim.buildings.stores.produce_at(owner,load.good,load.amount,who)}.unwrap_or(0.0);
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

pub fn step(sim: &mut SimState, dt_days: f32) {
    if !sim.colony.is_founded() {
        return;
    }
    let mut nav = std::mem::replace(&mut sim.nav, NavGrid::new(1, 1));
    nav.refresh(sim);
    sim.nav = nav;
    sync_agents(sim);
    tidy(sim);
    if sim.work.auto_organise {
        organise(sim);
    }
    let ids: Vec<BeaverId> = sim.agents.by_id.keys().copied().collect();
    for id in ids {
        let Some(mut ag) = sim.agents.by_id.remove(&id) else { continue };
        tick(sim, &mut ag, dt_days);
        sim.agents.by_id.insert(id, ag);
    }
    finish_sites(sim);
}

fn at_resting_place(ag: &Agent, home: Option<&Building>) -> bool {
    let _ = home;
    ag.inside
}
