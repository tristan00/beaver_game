use std::collections::BTreeMap;

use crate::buildings::{Building, BuildingId, BuildingKind, Good};
use crate::colony::{BeaverId, Stage};
use crate::nav::{Gait, Goal, NavGrid, Path, Surface};
use crate::resources::{Resource, ResourceId, ResourceKind};
use crate::water::Cell;
use crate::work::{Area, Role, EPS as WORK_EPS, LABOUR_PER_DAY};
use crate::SimState;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Load {
    pub good: Good,
    pub amount: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Assignment {
    Resource(ResourceId),
    Area(ResourceKind, Area),
    Slot(BuildingId, usize),
    Go(Cell),
}

#[derive(Clone, Debug, PartialEq)]
pub enum Task {
    Idle,
    Collect { target: ResourceId, progress: f32 },
    Carry { dest: BuildingId },
    Fetch { from: BuildingId, good: Good, then: BuildingId },
    Build { site: BuildingId },
    Haul { to: BuildingId },
    Rest,
    Forest(crate::forestry::ForestTask),
}

impl Task {
    pub fn label(&self) -> &'static str {
        match self {
            Task::Idle => "idle",
            Task::Collect { .. } => "collecting",
            Task::Carry { .. } => "carrying",
            Task::Fetch { .. } => "fetching",
            Task::Build { .. } => "building",
            Task::Haul { .. } => "hauling",
            Task::Rest => "resting",
            Task::Forest(_) => "foresting",
        }
    }
}

#[derive(Clone, Debug)]
pub struct Agent {
    pub id: BeaverId,
    pub cell: Cell,
    pub step_frac: f32,
    pub gait: Gait,
    pub task: Task,
    pub assignment: Option<Assignment>,
    pub load: Option<Load>,
    pub path: Option<Path>,
    pub inside: bool,
    pub stuck_ticks: u32,
    pub speed: f32,
    pub trail: Vec<(Cell, Gait)>,
}

pub const STUCK_TICKS: u32 = 600;

#[derive(Clone, Debug, Default)]
pub struct Agents {
    pub by_id: BTreeMap<BeaverId, Agent>,
    pub unreachable: BTreeMap<ResourceId, f64>,
}

pub const UNREACHABLE_RETRY_DAYS: f64 = 10.0;

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
                load: None,
                path: None,
                inside: false,
                stuck_ticks: 0,
                speed: 1.0,
                trail: Vec::new(),
            },
        );
    }
    pub fn assign(&mut self, id: BeaverId, a: Option<Assignment>) -> bool {
        match self.by_id.get_mut(&id) {
            Some(ag) => {
                ag.assignment = a;
                true
            }
            None => false,
        }
    }
}

pub const MOVE_COST_PER_DAY: f32 = 43_200.0;
pub const LADEN_PACE: f32 = 0.5;
pub const FELL_PER_DAY: f32 = 1.0;
pub const GATHER_PER_DAY: f32 = 6.0;
pub const LOAD_SIZE: f32 = 3.0;
const EPS: f32 = 1e-3;
const SPAWN_RADIUS: usize = 12;
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
                    && sim.nav.reserved.get(c.0, c.1) == Some(&None)
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

pub const PLANT_MIN_TAKE: f32 = 0.5;

fn gatherable(r: &Resource) -> bool {
    match r.kind {
        ResourceKind::Tree => crate::forestry::fellable(r),
        ResourceKind::Felled => r.amount > 0.0 || r.branches > 0.0,
        ResourceKind::Plant => r.amount >= PLANT_MIN_TAKE,
        _ => r.amount > 0.0,
    }
}

fn kinds_for(k: ResourceKind) -> &'static [ResourceKind] {
    match k {
        ResourceKind::Tree | ResourceKind::Felled => &[ResourceKind::Felled, ResourceKind::Tree],
        ResourceKind::Plant => &[ResourceKind::Plant, ResourceKind::Felled, ResourceKind::Tree],
        ResourceKind::MudBank => &[ResourceKind::MudBank],
        ResourceKind::StoneOutcrop => &[ResourceKind::StoneOutcrop],
    }
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
        let wanted = if self.forages() && r.kind == ResourceKind::Felled { r.bark > 0.0 } else { gatherable(r) };
        wanted
            && self.only.is_none_or(|id| id == r.id)
            && self.area.is_none_or(|a| a.contains(r.cell))
            && (self.only.is_some() || self.kinds.contains(&r.kind))
            && (r.kind != ResourceKind::Tree || crate::forestry::cuttable(r, self.min_age))
    }
    fn pick(&self, sim: &SimState, from: Cell) -> Option<ResourceId> {
        let d = |c: Cell| c.0.abs_diff(from.0).pow(2) + c.1.abs_diff(from.1).pow(2);
        let rank = |r: &Resource| self.kinds.iter().position(|&k| k == r.kind).unwrap_or(0);
        let reachable = |r: &Resource| {
            sim.agents.unreachable.get(&r.id).is_none_or(|until| sim.time_days >= *until) && sim.nav.may_reach_beside(from, r.cell)
        };
        sim.resources
            .items
            .iter()
            .filter(|r| self.permits(r) && reachable(r))
            .min_by_key(|r| (rank(r), d(r.cell), r.id))
            .map(|r| r.id)
    }
}

pub fn is_store(kind: BuildingKind) -> bool {
    Good::ALL.iter().any(|&g| accepts(kind, g))
}

pub(crate) fn accepts(kind: BuildingKind, good: Good) -> bool {
    match kind {
        BuildingKind::Warehouse | BuildingKind::Storehouse | BuildingKind::Dock => true,
        BuildingKind::FoodCache => good == Good::Food,
        _ => false,
    }
}

fn adjacent(c: Cell, b: &Building) -> bool {
    b.cells().any(|f| f.0.abs_diff(c.0) + f.1.abs_diff(c.1) == 1)
}

fn nearest_store(sim: &SimState, who: BeaverId, from: Cell, good: Good) -> Option<(BuildingId, Path)> {
    sim.buildings
        .buildings
        .iter()
        .filter(|b| b.is_complete() && accepts(b.kind, good) && !sim.buildings.stranded(b, &sim.water.depth))
        .filter_map(|b| {
            if adjacent(from, b) {
                return Some((b.id, Path { steps: Vec::new(), cost: 0.0 }));
            }
            let cells: Vec<Cell> = b.cells().collect();
            let path = sim.nav.find_path_beside_any(who, from, &cells)?;
            Some((b.id, path))
        })
        .min_by(|a, b| a.1.cost.total_cmp(&b.1.cost).then(a.0.cmp(&b.0)))
}

fn has_lodge(sim: &SimState) -> bool {
    sim.buildings.buildings.iter().any(|b| b.kind == BuildingKind::Lodge && b.is_complete())
}

fn home_of(sim: &SimState, who: BeaverId) -> Option<&Building> {
    let home = sim.colony.beaver(who)?.lodge?;
    let (&b, _) = sim.work.homes.iter().find(|(_, h)| **h == home)?;
    sim.buildings.building(b).filter(|b| b.is_complete())
}

fn route_home(sim: &SimState, who: BeaverId, from: Cell) -> Option<Path> {
    let cells = |b: &Building| b.cells().collect::<Vec<_>>();
    match home_of(sim, who) {
        Some(b) if b.kind == BuildingKind::Lodge => sim.nav.find_path_onto_any(who, from, &cells(b)),
        Some(b) => sim.nav.find_path_beside_any(who, from, &cells(b)),
        None => sim
            .buildings
            .buildings
            .iter()
            .filter(|b| b.kind == BuildingKind::Lodge && b.is_complete())
            .filter_map(|b| sim.nav.find_path_onto_any(who, from, &cells(b)))
            .min_by(|a, b| a.cost.total_cmp(&b.cost)),
    }
}

pub(crate) fn follow(nav: &mut NavGrid, ag: &mut Agent, dt_days: f32) -> bool {
    let pace = if ag.load.is_some() { LADEN_PACE } else { 1.0 };
    let mut budget = ag.step_frac + dt_days * MOVE_COST_PER_DAY * pace;
    let mut moved = false;
    ag.trail.clear();
    loop {
        let Some(path) = ag.path.as_mut() else { break };
        let Some(&(next, gait)) = path.steps.first() else {
            ag.path = None;
            break;
        };
        let cost = nav.surface_at(next).and_then(NavGrid::step_cost).map_or(1.0, |c| c.0);
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
    Forest(BuildingId, usize),
}

fn duty_of(sim: &SimState, ag: &Agent, can_work: bool) -> Duty {
    if !can_work {
        return Duty::None;
    }
    match sim.work.slot_of(ag.id) {
        Some((b, _, Role::Build)) if sim.work.place(b).is_some_and(|p| p.site.is_some()) => Duty::Build(b),
        Some((_, _, Role::Haul)) => Duty::Haul,
        Some((b, i, Role::Collect(_))) => Job::resolve(sim, Assignment::Slot(b, i)).map_or(Duty::None, Duty::Collect),
        Some((b, i, Role::Forester)) if sim.work.place(b).is_some_and(|p| p.kind == BuildingKind::ForesterLodge) => Duty::Forest(b, i),
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

fn build_task(sim: &SimState, site: BuildingId, ag: &Agent) -> Option<Task> {
    if sim.buildings.building(site).is_none_or(|b| b.is_complete()) {
        return None;
    }
    let progress = sim.work.place(site)?.site.as_ref()?;
    if progress.labour_open() {
        return Some(Task::Build { site });
    }
    for &(good, _) in &progress.needed {
        let outstanding = progress.missing(good) - promised(sim, site, good);
        let older_wants = sim.work.places.iter().filter(|p| p.building < site).filter_map(|p| p.site.as_ref().map(|s| (p.building, s))).any(|(b, s)| {
            sim.buildings.building(b).is_some_and(|bd| !bd.is_complete()) && s.missing(good) - promised(sim, b, good) > WORK_EPS
        });
        let in_stock: f32 = crate::stock::holders(sim, good).iter().map(|&h| sim.work.stock_of(h, good)).sum();
        if older_wants && in_stock < 2.0 * LOAD_SIZE {
            continue;
        }
        if outstanding > WORK_EPS {
            let want = outstanding.min(LOAD_SIZE);
            let near = sim.buildings.building(site).map_or(ag.cell, |b| b.pos);
            if let Some(from) = crate::stock::nearest_holding(sim, near, good, want).or_else(|| crate::stock::nearest_holding(sim, near, good, WORK_EPS)) {
                return Some(Task::Fetch { from, good, then: site });
            }
        }
    }
    None
}

fn haul_task(sim: &SimState) -> Option<Task> {
    let ok = |id| sim.buildings.building(id).is_some_and(|b| b.is_complete());
    let open = |id| sim.buildings.building(id).is_some_and(|b| b.is_complete() && !sim.buildings.stranded(b, &sim.water.depth));
    sim.work
        .hauls
        .iter()
        .find(|o| ok(o.from) && open(o.to) && sim.work.stock_of(o.from, o.good) > WORK_EPS)
        .map(|o| Task::Fetch { from: o.from, good: o.good, then: o.to })
}

fn next_task(sim: &SimState, duty: &Duty, ag: &Agent) -> Option<Task> {
    match duty {
        Duty::None => None,
        Duty::Collect(j) => j.pick(sim, ag.cell).map(|t| Task::Collect { target: t, progress: 0.0 }),
        Duty::Build(site) => build_task(sim, *site, ag),
        Duty::Haul => haul_task(sim).or_else(|| {
            let own = sim.work.slot_of(ag.id).map(|s| s.0).filter(|b| sim.work.balanced.contains(b))?;
            let (from, then, good) = crate::stock::balance_move(sim, Some(own))?;
            Some(Task::Fetch { from, good, then })
        }),
        Duty::Forest(b, i) => crate::forestry::next_task(sim, *b, *i, ag.id, ag.cell).map(Task::Forest),
    }
}

fn tick(sim: &mut SimState, ag: &mut Agent, dt: f32) {
    let (can_work, tired) = sim.colony.beaver(ag.id).map_or((false, false), |b| (b.stage != Stage::Kit, b.resting));
    let duty = duty_of(sim, ag, can_work && !tired);
    ag.speed = sim.colony.beaver(ag.id).map_or(1.0, |b| crate::society::work_speed(b.mood));
    if tired && ag.load.is_none() && !ag.inside && !matches!(ag.task, Task::Rest | Task::Carry { .. }) {
        ag.path = None;
        ag.task = Task::Rest;
    }
    if let Some(Assignment::Go(to)) = ag.assignment {
        if can_work && !tired && ag.load.is_none() {
            go_to(sim, ag, dt, to);
            return;
        }
    }
    if matches!(duty, Duty::None) && can_work && !tired && matches!(ag.assignment, Some(Assignment::Resource(_))) {
        ag.assignment = None;
    }
    match ag.task.clone() {
        Task::Idle => {
            ag.path = None;
            if ag.load.is_some() {
                ag.task = Task::Carry { dest: NO_STORE };
            } else if let Some(t) = next_task(sim, &duty, ag) {
                ag.task = t;
            } else if !ag.inside && has_lodge(sim) {
                ag.task = Task::Rest;
            }
        }
        Task::Rest => {
            if ag.load.is_some() || next_task(sim, &duty, ag).is_some() {
                ag.task = Task::Idle;
                return;
            }
            if ag.inside {
                ag.path = None;
                return;
            }
            if ag.path.is_none() {
                ag.path = route_home(sim, ag.id, ag.cell);
            }
            let moved = follow(&mut sim.nav, ag, dt);
            note(ag, moved);
        }
        Task::Collect { target, .. } => collect(sim, ag, dt, duty, target),
        Task::Carry { dest } if sim.work.place(dest).is_some_and(|p| p.site.is_some()) => deliver(sim, ag, dt, dest),
        Task::Carry { dest } => carry(sim, ag, dt, dest),
        Task::Fetch { from, good, then } => fetch(sim, ag, dt, &duty, from, good, then),
        Task::Build { site } => build(sim, ag, dt, &duty, site),
        Task::Haul { to } => haul(sim, ag, dt, to),
        Task::Forest(t) => {
            let lodge = if let Duty::Forest(b, _) = duty { Some(b) } else { None };
            crate::forestry::work(sim, ag, dt, lodge, t)
        }
    }
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

fn go_to(sim: &mut SimState, ag: &mut Agent, dt: f32, to: Cell) {
    let done = |ag: &mut Agent, sim: &mut SimState| {
        ag.assignment = None;
        ag.path = None;
        ag.task = Task::Idle;
        sim.work.player_orders.remove(&ag.id);
    };
    if ag.cell.0.abs_diff(to.0).max(ag.cell.1.abs_diff(to.1)) <= 1 {
        return done(ag, sim);
    }
    if ag.inside || ag.path.is_none() {
        ag.path = sim.nav.find_path(ag.id, ag.cell, Goal::Cell(to)).or_else(|| sim.nav.find_path(ag.id, ag.cell, Goal::Beside(to)));
        if ag.path.is_none() {
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

fn fetch(sim: &mut SimState, ag: &mut Agent, dt: f32, duty: &Duty, from: BuildingId, good: Good, then: BuildingId) {
    let site = sim.work.place(then).is_some_and(|p| p.site.is_some());
    let still_wanted = match duty {
        Duty::Build(s) => site && *s == then,
        Duty::Haul => {
            !site
                && (sim.work.hauls.iter().any(|o| (o.from, o.to, o.good) == (from, then, good))
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
        if want <= WORK_EPS || sim.buildings.stores.withdraw(good, want).is_err() {
            return abandon(ag);
        }
        sim.work.stock_take(from, good, want);
        ag.load = Some(Load { good, amount: want });
        ag.task = Task::Carry { dest: then };
    } else {
        let got = sim.work.stock_take(from, good, LOAD_SIZE);
        if got <= WORK_EPS {
            return abandon(ag);
        }
        ag.load = Some(Load { good, amount: got });
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
    note(ag, true);
    let rest = load.amount - taken;
    if rest > WORK_EPS {
        ag.load = Some(Load { good: load.good, amount: rest });
        ag.task = Task::Carry { dest: NO_STORE };
    } else {
        ag.load = None;
        ag.task = Task::Idle;
    }
}

fn build(sim: &mut SimState, ag: &mut Agent, dt: f32, duty: &Duty, site: BuildingId) {
    let open = sim.buildings.building(site).is_some_and(|b| !b.is_complete())
        && sim.work.place(site).and_then(|p| p.site.as_ref()).is_some_and(|s| s.labour_open());
    if !open || !matches!(duty, Duty::Build(s) if *s == site) {
        return abandon(ag);
    }
    if approach(sim, ag, dt, site) {
        sim.work.add_labour(site, dt * LABOUR_PER_DAY).expect("duty checked the site");
        grow_dam(sim, site);
        note(ag, true);
    }
}

pub(crate) fn grow_dam(sim: &mut SimState, site: BuildingId) {
    let Some(f) = sim.work.place(site).and_then(|p| p.site.as_ref()).map(|s| s.labour_fraction()) else { return };
    let Some(plan) = sim.work.dam_plans.iter().find(|p| p.building == site) else { return };
    let cells: Vec<(Cell, f32)> = plan
        .cells
        .iter()
        .zip(&plan.heights)
        .zip(&plan.from)
        .map(|((&c, &h), &from)| (c, from + f * (h - from)))
        .filter(|&(_, h)| h > 0.0)
        .collect();
    if !cells.is_empty() {
        if let Err(e) = sim.water.raise_dam(&cells) {
            sim.work.dam_failures.push((site, e.to_string()));
        }
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
        sim.work.stock_add(to, load.good, load.amount);
        ag.load = None;
        ag.task = Task::Idle;
        note(ag, true);
    }
}

fn finish_sites(sim: &mut SimState) {
    let done: Vec<BuildingId> =
        sim.work.places.iter().filter(|p| p.site.as_ref().is_some_and(|s| s.is_done())).map(|p| p.building).collect();
    for id in done {
        let Some(b) = sim.buildings.buildings.iter_mut().find(|b| b.id == id && b.status == crate::buildings::Status::Site)
        else {
            continue;
        };
        b.status = crate::buildings::Status::Complete;
        if b.kind == BuildingKind::Barge {
            let cell = b.pos;
            sim.buildings.buildings.retain(|x| x.id != id);
            sim.work.places.retain(|p| p.building != id);
            let locks = crate::shipping::lock_cells(sim);
            let at_dock = sim
                .buildings
                .buildings
                .iter()
                .filter(|d| d.kind == BuildingKind::Dock && d.is_complete())
                .find(|d| crate::shipping::berths(sim, &locks, d).contains(&cell))
                .map(|d| d.id);
            let bid = crate::buildings::BargeId(id.0);
            sim.buildings.barges.push(crate::buildings::Barge::new(bid, cell, at_dock));
            continue;
        }
        let area = crate::catalogue::work_area(b);
        let def = crate::catalogue::def(b.kind);
        sim.work.finish_site(id, &def.slots, area).expect("workplace exists");
        if let Some(i) = sim.work.dam_plans.iter().position(|p| p.building == id) {
            let plan = sim.work.dam_plans.remove(i);
            let cells: Vec<(Cell, f32)> = plan.cells.iter().copied().zip(plan.heights.iter().copied()).collect();
            if let Err(e) = sim.water.raise_dam(&cells) {
                sim.work.dam_failures.push((id, e.to_string()));
            }
        }
        if let Some(i) = sim.work.canal_plans.iter().position(|p| p.building == id) {
            let plan = sim.work.canal_plans.remove(i);
            if let Err(e) = sim.water.dig_canal(&mut sim.height, &plan.cells, plan.depth) {
                sim.work.dam_failures.push((id, e.to_string()));
            }
        }
    }
}

const GATHER_RADIUS: usize = 80;

fn source_of(good: Good) -> ResourceKind {
    match good {
        Good::Logs | Good::Sticks => ResourceKind::Tree,
        Good::Mud => ResourceKind::MudBank,
        Good::Stone => ResourceKind::StoneOutcrop,
        _ => ResourceKind::Plant,
    }
}

pub(crate) fn is_site(sim: &SimState, p: &crate::work::Workplace) -> bool {
    p.is_open_site() && sim.buildings.building(p.building).is_some_and(|b| !b.is_complete())
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
    let raw = |g: Good| matches!(g, Good::Logs | Good::Sticks | Good::Mud | Good::Stone);
    let good = progress.needed.iter().map(|n| n.0).find(|&g| {
        raw(g) && progress.missing(g) > WORK_EPS && open.iter().map(|p| p.missing(g)).sum::<f32>() - sim.buildings.stores.amount(g) > WORK_EPS
    })?;
    let r = GATHER_RADIUS;
    let area = Area { min: (pos.0.saturating_sub(r), pos.1.saturating_sub(r)), max: (pos.0 + r, pos.1 + r) };
    Some(Assignment::Area(source_of(good), area))
}

pub(crate) fn can_fill(stage: Stage, role: Role) -> bool {
    match role {
        _ => matches!(stage, Stage::Adult | Stage::Yearling),
    }
}

struct Opening {
    duty: crate::work::ManagedDuty,
    key: (std::cmp::Reverse<crate::work::Priority>, usize, usize, BuildingId),
    at: Cell,
    role: Role,
}

fn tidy(sim: &mut SimState) {
    let alive: std::collections::BTreeSet<BeaverId> = sim.colony.beavers.iter().map(|b| b.id).collect();
    let stalled: Vec<BuildingId> = sim.work.places.iter().filter(|p| is_site(sim, p) && site_stalled(sim, p)).map(|p| p.building).collect();
    let dry: Vec<BuildingId> = sim.buildings.buildings.iter().filter(|b| sim.buildings.stranded(b, &sim.water.depth)).map(|b| b.id).collect();
    for p in &mut sim.work.places {
        let idle_site = stalled.contains(&p.building);
        let closed = p.paused || dry.contains(&p.building);
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
        if p.paused || !(b.is_complete() || site) || sim.buildings.stranded(b, &sim.water.depth) {
            continue;
        }
        let ours = |i: usize, w: Option<BeaverId>| w.is_none() || w.is_some_and(|w| sim.work.managed.get(&w) == Some(&ManagedDuty::Slot(p.building, i)));
        let fixed = p.slots.iter().enumerate().filter(|(i, s)| !ours(*i, s.worker)).count();
        let mut budget = p.max_workers.saturating_sub(fixed);
        let run = if site { site_material_run(sim, p.building, b.pos) } else { None };
        let prio = std::cmp::Reverse(p.priority);
        let mut runs = run.map_or(0, |_| crate::work::SITE_SLOTS.min(p.max_workers));
        for (i, s) in p.slots.iter().enumerate() {
            let idle = match s.role {
                Role::Haul => sim.work.hauls.is_empty() && !(sim.work.balanced.contains(&b.id) && crate::stock::balance_move(sim, Some(b.id)).is_some()),
                Role::Craft => b.recipe.is_none(),
                _ => false,
            };
            if idle {
                continue;
            }
            let rank = crate::jobs::rank(sim, crate::jobs::role_kind(s.role));
            if let Some(a) = run.filter(|_| runs > 0) {
                runs -= 1;
                let role = Role::Collect(match a { Assignment::Area(k, _) => k, _ => ResourceKind::Tree });
                openings.push(Opening { duty: ManagedDuty::Gather(a), key: (prio, rank, 2 * i, p.building), at: b.pos, role });
            }
            if site && s.role == Role::Build && site_stalled(sim, p) {
                continue;
            }
            if ours(i, s.worker) && budget > 0 {
                budget -= 1;
                openings.push(Opening { duty: ManagedDuty::Slot(p.building, i), key: (prio, rank, 2 * i + 1, p.building), at: b.pos, role: s.role });
            }
        }
    }
    openings.sort_by_key(|o| o.key);
    let pool: Vec<(BeaverId, Stage, Cell)> = sim
        .colony
        .beavers
        .iter()
        .filter(|b| b.stage != Stage::Kit && !sim.work.player_orders.contains(&b.id))
        .filter_map(|b| sim.agents.get(b.id).map(|a| (b, a)))
        .filter(|(b, a)| sim.work.managed.contains_key(&b.id) || (sim.work.slot_of(b.id).is_none() && a.assignment.is_none()))
        .map(|(b, a)| (b.id, b.stage, a.cell))
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
            .filter(|&k| !used[k] && can_fill(pool[k].1, opening.role))
            .min_by_key(|&k| (d2(pool[k].2, opening.at), pool[k].0));
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
            .find(|&l| (openings[l].key.0, openings[l].key.1) > rank && holder[l].is_some_and(|k| can_fill(pool[k].1, openings[o].role)));
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
        if sim.resources.fell(target, dt * ag.speed * FELL_PER_DAY).expect("kind checked above") {
            sim.nav.set(res.cell, Surface::Land);
            crate::forestry::felled(sim, ag.id, res.cell);
        }
        return;
    }
    let bark = res.kind == ResourceKind::Felled && job.is_some_and(|j| j.forages());
    let room = |g: Good| sim.buildings.stores.space(g) > EPS;
    let good = match res.kind {
        _ if bark => Good::Food,
        ResourceKind::Felled if res.amount > 0.0 => Good::Logs,
        ResourceKind::Felled => Good::Sticks,
        k => k.yields()[0],
    };
    let has_store = closest_store(sim, ag.cell, good).is_some();
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
    let left = if bark { res.bark } else if good == Good::Sticks { res.branches } else { res.amount };
    let mut want = (dt * ag.speed * GATHER_PER_DAY).min(LOAD_SIZE - have);
    if left - want < EPS {
        want = left;
    }
    let taken = if bark { sim.resources.strip(target, want).ok().filter(|t| t.1 > 0.0) } else { sim.resources.take(target, want) };
    if let Some((g, got)) = taken {
        if let Some((b, _, _)) = sim.work.slot_of(ag.id) {
            sim.output.record(b, g, got);
        }
        let amount = have + got;
        ag.load = Some(Load { good: g, amount });
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
        .filter_map(|p| Some((sim.buildings.building(p.building).filter(|b| !b.is_complete())?, p.site.as_ref()?)))
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
    if closest_store(sim, ag.cell, load.good).is_some() && sim.buildings.stores.space(load.good) <= EPS {
        ag.load = None;
        ag.path = None;
        ag.task = Task::Idle;
        note(ag, false);
        return;
    }
    if load.good == Good::Food && closest_store(sim, ag.cell, Good::Food).is_none() {
        if let Some(home) = larder_cell(sim, ag.id) {
            return carry_home(sim, ag, dt, load, home);
        }
    }
    let dest = if ag.path.is_some() || adjacent_to_store(sim, ag.cell, dest, load.good) {
        dest
    } else {
        let Some((id, path)) = nearest_store(sim, ag.id, ag.cell, load.good) else {
            if closest_store(sim, ag.cell, load.good).is_none() {
                ag.load = None;
                ag.path = None;
                ag.task = Task::Idle;
                return;
            }
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
        let booked = sim.buildings.stores.deposit_by(load.good, load.amount, who);
        if let Ok(taken) = booked {
            sim.work.stock_add(dest, load.good, taken);
        }
        match booked {
            Ok(taken) if taken >= load.amount => {
                ag.load = None;
                ag.task = Task::Idle;
                note(ag, true);
            }
            Ok(taken) => {
                ag.load = Some(Load { good: load.good, amount: load.amount - taken });
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

pub const HOME_RANGE: usize = 20;

fn carry_home(sim: &mut SimState, ag: &mut Agent, dt: f32, load: Load, home: Cell) {
    let at_home = ag.cell.0.abs_diff(home.0) + ag.cell.1.abs_diff(home.1) <= 1;
    let near = ag.cell.0.abs_diff(home.0).max(ag.cell.1.abs_diff(home.1)) <= HOME_RANGE;
    if !at_home && ag.path.is_none() {
        ag.path = sim.nav.find_path(ag.id, ag.cell, Goal::Beside(home));
        if ag.path.is_none() && !near {
            ag.load = None;
            ag.task = Task::Idle;
            note(ag, false);
            return;
        }
    }
    if at_home || ag.path.is_none() {
        ag.path = None;
        let who = producer(sim, ag);
        let taken = sim.buildings.stores.deposit_by(load.good, load.amount, who).unwrap_or(0.0);
        if taken >= load.amount - EPS {
            ag.load = None;
            ag.task = Task::Idle;
        } else {
            ag.load = Some(Load { good: load.good, amount: load.amount - taken });
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
    nav.rebuild(sim);
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::buildings::Status;

    #[test]
    #[ignore = "beavers: off until the water engine is done"]
    fn a_half_built_dam_holds_back_part_of_its_pond() {
        let (site, height) = (BuildingId(900), 2.0);
        let pond_level = |labour: Option<f32>| {
            let mut sim = SimState::new(1);
            let n = sim.height.width();
            for y in 0..n {
                for x in 0..n {
                    *sim.height.get_mut(x, y).unwrap() = 0.2 * (n - x) as f32 + 0.15 * (y as f32 - 64.0).abs();
                }
            }
            sim.water = crate::water::Water::new(n);
            sim.water.config.overtop_erosion = 0.0;
            sim.water.config.dam_decay_per_day = 0.0;
            sim.water.add_source(10, 64, 4.0).unwrap();
            let cells: Vec<Cell> = (0..n).map(|y| (40, y)).collect();
            sim.work.add_site(site, BuildingKind::Dam, &[], 10.0).unwrap();
            sim.work.dam_plans.push(crate::work::DamPlan { building: site, heights: vec![height; n], from: vec![0.0; n], cells, height });
            if let Some(l) = labour {
                sim.work.add_labour(site, l).unwrap();
                grow_dam(&mut sim, site);
                let want = height * l / 10.0;
                assert!((sim.water.dam_height.get(40, 64).unwrap() - want).abs() < 1e-4, "the dam stands {want} after {l} of 10 labour");
            }
            for _ in 0..600 {
                sim.water.step(&sim.height, 0.05);
            }
            sim.height.get(39, 64).unwrap() + sim.water.depth.get(39, 64).unwrap()
        };
        let (unbuilt, half, finished) = (pond_level(None), pond_level(Some(5.0)), pond_level(Some(10.0)));
        assert!(unbuilt + 0.3 < half && half + 0.3 < finished, "pond levels: unbuilt {unbuilt}, half built {half}, finished {finished}");
    }

    #[test]
    #[ignore = "beavers: off until the water engine is done"]
    fn a_finished_barge_site_becomes_a_barge_at_its_dock() {
        let mut sim = SimState::new(1);
        let n = sim.water.depth.width();
        sim.water = crate::water::Water::new(n);
        for x in 10..30 {
            *sim.water.depth.get_mut(x, 20).unwrap() = 0.5;
        }
        let dock = BuildingId(500);
        sim.buildings.buildings.push(Building { id: dock, kind: BuildingKind::Dock, pos: (15, 19), status: Status::Complete, recipe: None, output: 0.0 });
        let site = BuildingId(501);
        sim.buildings.buildings.push(Building { id: site, kind: BuildingKind::Barge, pos: (16, 20), status: Status::Site, recipe: None, output: 0.0 });
        sim.work.add_site(site, BuildingKind::Barge, &[], 0.0).unwrap();
        finish_sites(&mut sim);
        assert!(sim.buildings.building(site).is_none() && sim.work.place(site).is_none(), "no barge building is left");
        let b = sim.buildings.barges.last().expect("a barge");
        assert_eq!((b.cell, b.at_dock), ((16, 20), Some(dock)));
    }

    #[test]
    #[ignore = "beavers: off until the water engine is done"]
    fn collectors_skip_a_resource_nobody_can_reach_until_the_retry_day() {
        let mut sim = SimState::new(1);
        let job = Job { kinds: &[ResourceKind::Tree], area: None, only: None, min_age: 0.0 };
        let first = job.pick(&sim, (64, 64)).expect("trees on the map");
        sim.agents.unreachable.insert(first, sim.time_days + UNREACHABLE_RETRY_DAYS);
        let second = job.pick(&sim, (64, 64)).expect("another tree");
        assert_ne!(first, second, "the unreachable tree is skipped");
        sim.time_days += UNREACHABLE_RETRY_DAYS;
        assert_eq!(job.pick(&sim, (64, 64)), Some(first), "and tried again after the retry days");
    }
    use crate::resources::{FELL_WORK, TREE_BRANCHES, TREE_LOGS};

    const DT: f32 = crate::STEP_DAYS;

    fn days_of(days: f32) -> usize {
        (days / DT).round() as usize
    }

    fn setup() -> (SimState, Vec<BeaverId>) {
        let mut sim = SimState::new(3);
        sim.resources = Default::default();
        sim.tech.unlock_all();
        let spot = (0..sim.world.size)
            .map(|i| (i as f32, 100.0))
            .find(|&(x, y)| !crate::colony::Landscape::is_water(&crate::WorldLand { world: &sim.world, depth: &sim.water.depth }, x, y))
            .expect("some dry cell on row 100");
        let land = crate::WorldLand { world: &sim.world, depth: &sim.water.depth };
        sim.colony.found(spot, &land).unwrap();
        let ids = sim.colony.beavers.iter().map(|b| b.id).collect();
        (sim, ids)
    }

    #[test]
    #[ignore = "beavers: off until the water engine is done"]
    fn a_walk_order_goes_there_and_then_ends() {
        let (mut sim, ids) = setup();
        place_beavers(&mut sim);
        let from = sim.agents.get(ids[0]).unwrap().cell;
        let to = (from.0 + 8, from.1);
        sim.apply(crate::Command::Assign { who: vec![ids[0]], to: Some(Assignment::Go(to)) }).unwrap();
        for _ in 0..days_of(80.0) {
            sim.step(DT).unwrap();
            if sim.agents.get(ids[0]).unwrap().assignment.is_none() {
                break;
            }
        }
        let a = sim.agents.get(ids[0]).unwrap();
        assert!(a.assignment.is_none(), "the order ended");
        assert!(a.cell.0.abs_diff(to.0).max(a.cell.1.abs_diff(to.1)) <= 1, "arrived at {:?}, wanted {:?}", a.cell, to);
        assert!(!sim.work.player_orders.contains(&ids[0]), "free again for auto-staffing");
    }

    fn warehouse(sim: &mut SimState, id: u64, pos: Cell) -> BuildingId {
        let id = BuildingId(id);
        sim.buildings.buildings.push(Building {
            id,
            kind: BuildingKind::Warehouse,
            pos,
            status: Status::Complete,
            recipe: None,
            output: 0.0,
        });
        id
    }

    fn total(sim: &SimState, g: Good) -> f32 {
        sim.buildings.stores.produced_total[g.index()]
    }

    #[test]
    #[ignore = "beavers: off until the water engine is done"]
    fn collector_fells_tree_and_delivers_to_nearest_store_only_on_delivery() {
        let (mut sim, ids) = setup();
        let near = warehouse(&mut sim, 1, (20, 9));
        warehouse(&mut sim, 2, (10, 40));
        let tree = sim.resources.add(ResourceKind::Tree, (14, 10), FELL_WORK);
        spawn_on(&mut sim, ids[0], (10, 10)).unwrap();
        sim.apply(crate::Command::Assign { who: vec![ids[0]], to: Some(Assignment::Resource(tree)) }).unwrap();
        let (mut felled, mut carried, mut deliveries) = (false, false, 0);
        for _ in 0..days_of(60.0) {
            let before = (total(&sim, Good::Logs), total(&sim, Good::Sticks));
            let load_before = sim.agents.get(ids[0]).unwrap().load;
            step(&mut sim, DT);
            let ag = sim.agents.get(ids[0]).unwrap();
            let after = (total(&sim, Good::Logs), total(&sim, Good::Sticks));
            felled |= sim.resources.get(tree).is_some_and(|r| r.kind == ResourceKind::Felled);
            carried |= matches!(ag.task, Task::Carry { dest } if dest == near);
            if after != before {
                deliveries += 1;
                let l = load_before.expect("totals rise only when a carried load is delivered");
                assert!((l.amount - ((after.0 - before.0) + (after.1 - before.1))).abs() < 1e-3);
                assert!(ag.load.is_none());
                assert!(adjacent(ag.cell, sim.buildings.building(near).unwrap()), "delivered to the nearest store");
            }
            assert!(felled || after == (0.0, 0.0), "nothing delivered before the tree falls");
            if ag.load.is_some() {
                assert_eq!(after, before, "a carried load is not yet in the totals");
            }
        }
        assert!(felled && carried);
        assert!((total(&sim, Good::Logs) - TREE_LOGS).abs() < 1e-3);
        assert!((total(&sim, Good::Sticks) - TREE_BRANCHES).abs() < 1e-3);
        assert_eq!(deliveries, 3, "3 logs, then branches in two loads");
        assert!((sim.buildings.stores.amount(Good::Logs) - TREE_LOGS).abs() < 1e-3);
        assert_eq!(total(&sim, Good::Food), 0.0, "bark is not part of loads");
    }

    #[test]
    #[ignore = "beavers: off until the water engine is done"]
    fn area_collector_stays_in_area_and_kits_and_unassigned_adults_idle() {
        let (mut sim, ids) = setup();
        warehouse(&mut sim, 1, (20, 9));
        let inside = [(14, 10), (15, 11)].map(|c| sim.resources.add(ResourceKind::Tree, c, FELL_WORK));
        let outside = [(11, 10), (14, 14), (13, 7)].map(|c| sim.resources.add(ResourceKind::Tree, c, FELL_WORK));
        let area = Area { min: (13, 8), max: (18, 12) };
        sim.colony.beavers.iter_mut().find(|b| b.id == ids[2]).unwrap().stage = Stage::Kit;
        for (i, c) in [(0, (10, 10)), (1, (10, 12)), (2, (10, 14))] {
            spawn_on(&mut sim, ids[i], c).unwrap();
        }
        let start: Vec<Cell> = ids[1..3].iter().map(|&i| sim.agents.get(i).unwrap().cell).collect();
        sim.apply(crate::Command::Assign { who: vec![ids[0]], to: Some(Assignment::Area(ResourceKind::Tree, area)) }).unwrap();
        assert!(sim.apply(crate::Command::Assign { who: vec![ids[2]], to: Some(Assignment::Resource(inside[0])) }).is_err());
        for _ in 0..days_of(80.0) {
            step(&mut sim, DT);
            for &o in &outside {
                let r = sim.resources.get(o).unwrap();
                assert_eq!((r.kind, r.amount), (ResourceKind::Tree, FELL_WORK), "outside tree touched");
            }
            for &i in &ids[1..3] {
                let ag = sim.agents.get(i).unwrap();
                assert!(ag.load.is_none() && matches!(ag.task, Task::Idle), "{:?} worked", ag.task);
            }
        }
        let now: Vec<Cell> = ids[1..3].iter().map(|&i| sim.agents.get(i).unwrap().cell).collect();
        assert_eq!(now, start, "idle beavers stay put");
        assert!(inside.iter().all(|&t| sim.resources.get(t).unwrap().kind == ResourceKind::Felled));
        assert!((total(&sim, Good::Logs) - 2.0 * TREE_LOGS).abs() < 1e-3, "both trees in the area were worked");
    }

    #[test]
    #[ignore = "beavers: off until the water engine is done"]
    fn carrier_swims_when_the_water_route_is_shorter() {
        let gaits = |river: bool| {
            let (mut sim, ids) = setup();
            sim.water.depth.as_mut_slice().fill(0.0);
            if river {
                for y in 0..40 {
                    for x in 15..17 {
                        *sim.water.depth.get_mut(x, y).unwrap() = 1.0;
                    }
                }
            }
            let store = warehouse(&mut sim, 1, (20, 9));
            spawn_on(&mut sim, ids[0], (10, 10)).unwrap();
            sim.agents.by_id.get_mut(&ids[0]).unwrap().load = Some(Load { good: Good::Sticks, amount: 2.0 });
            let mut seen = Vec::new();
            for _ in 0..days_of(60.0) {
                step(&mut sim, DT);
                let ag = sim.agents.get(ids[0]).unwrap();
                seen.extend(ag.trail.iter().map(|t| t.1));
                if ag.load.is_none() {
                    assert!(adjacent(ag.cell, sim.buildings.building(store).unwrap()));
                    assert!((total(&sim, Good::Sticks) - 2.0).abs() < 1e-3);
                    return seen;
                }
            }
            panic!("never delivered");
        };
        assert!(gaits(true).contains(&Gait::Swim), "crossed the river by swimming");
        assert!(!gaits(false).contains(&Gait::Swim), "walks when there is no water");
    }

    fn dig(sim: &mut SimState, y: usize, xs: std::ops::RangeInclusive<usize>) {
        for x in xs {
            *sim.water.depth.get_mut(x, y).unwrap() = 1.0;
            *sim.water.canal.get_mut(x, y).unwrap() = true;
        }
    }

    #[test]
    #[ignore = "beavers: off until the water engine is done"]
    fn beaver_crosses_faster_by_canal_than_overland() {
        let ticks = |canal: bool| {
            let (mut sim, ids) = setup();
            if canal {
                dig(&mut sim, 20, 13..=39);
            }
            let store = warehouse(&mut sim, 1, (40, 19));
            spawn_on(&mut sim, ids[0], (9, 20)).unwrap();
            sim.agents.by_id.get_mut(&ids[0]).unwrap().load = Some(Load { good: Good::Sticks, amount: 2.0 });
            let mut canal_steps = 0;
            for n in 1..=days_of(100.0) {
                step(&mut sim, DT);
                let ag = sim.agents.get(ids[0]).unwrap();
                canal_steps += (sim.nav.surface_at(ag.cell) == Some(Surface::Canal)) as usize;
                if ag.load.is_none() {
                    assert!(adjacent(ag.cell, sim.buildings.building(store).unwrap()));
                    assert_eq!(canal_steps > 0, canal, "canal used iff dug");
                    return n;
                }
            }
            panic!("never delivered");
        };
        let (overland, by_canal) = (ticks(false), ticks(true));
        assert!(by_canal < overland, "canal {by_canal} ticks vs overland {overland}");
    }

    #[test]
    #[ignore = "beavers: off until the water engine is done"]
    fn hauler_between_canal_linked_stores_delivers_sooner_than_over_land() {
        let ticks = |canal: bool| {
            let (mut sim, ids) = setup();
            adult(&mut sim, ids[0]);
            if canal {
                dig(&mut sim, 20, 13..=39);
            }
            let (a, b) = (warehouse(&mut sim, 1, (10, 19)), warehouse(&mut sim, 2, (40, 19)));
            sim.work.places.push(Workplace {
                building: a,
                kind: BuildingKind::Warehouse,
                slots: vec![Slot::empty(Role::Haul)],
                area: None,
                site: None, priority: Default::default(), paused: false, max_workers: 1,
            });
            sim.work.stock_add(a, Good::Logs, 9.0);
            spawn_on(&mut sim, ids[0], (9, 20)).unwrap();
            fill(&mut sim, a, 0, ids[0]).unwrap();
            sim.apply(Command::SetHaul { order: HaulOrder { from: a, to: b, good: Good::Logs } }).unwrap();
            for n in 1..=20_000 {
                step(&mut sim, DT);
                if sim.work.stock_of(b, Good::Logs) >= 9.0 - 1e-3 {
                    return n;
                }
            }
            panic!("never delivered");
        };
        let (overland, by_canal) = (ticks(false), ticks(true));
        assert!(by_canal < overland, "canal {by_canal} ticks vs overland {overland}");
    }

    #[test]
    #[ignore = "beavers: off until the water engine is done"]
    fn a_forager_with_only_trees_fells_one_for_its_bark_and_brings_it_home_without_a_store() {
        use crate::buildings::Producer;
        let (mut sim, ids) = setup();
        adult(&mut sim, ids[0]);
        place_beavers(&mut sim);
        let at = sim.agents.get(ids[0]).unwrap().cell;
        let tree = sim.resources.add(ResourceKind::Tree, (at.0 + 2, at.1), FELL_WORK);
        let area = Area { min: (at.0.saturating_sub(5), at.1.saturating_sub(5)), max: (at.0 + 5, at.1 + 5) };
        sim.apply(crate::Command::Assign { who: vec![ids[0]], to: Some(Assignment::Area(ResourceKind::Plant, area)) }).unwrap();
        for _ in 0..days_of(20.0) {
            step(&mut sim, DT);
            if total(&sim, Good::Food) > 0.0 && sim.agents.get(ids[0]).unwrap().load.is_none() {
                break;
            }
        }
        let r = sim.resources.get(tree).expect("logs and branches are left");
        assert_eq!((r.kind, r.bark, r.amount), (ResourceKind::Felled, 0.0, TREE_LOGS), "felled and stripped, wood left");
        assert!((total(&sim, Good::Food) - crate::resources::TREE_BARK).abs() < 1e-3, "the bark reached the lodge larder");
        assert!(sim.buildings.stores.produced_by.contains_key(&(Producer::Order, Good::Food)), "booked to the gather order");
        assert_eq!(total(&sim, Good::Logs), 0.0);
    }

    use crate::work::{HaulOrder, Slot, Workplace};
    use crate::{Command, SimError};

    fn adult(sim: &mut SimState, id: BeaverId) {
        sim.colony.beavers.iter_mut().find(|b| b.id == id).unwrap().stage = Stage::Adult;
    }

    fn fill(sim: &mut SimState, building: BuildingId, slot: usize, who: BeaverId) -> Result<(), SimError> {
        sim.apply(Command::FillSlot { building, slot, who })
    }

    fn lodge_site() -> (SimState, Vec<BeaverId>, BuildingId, Vec<(Good, f32)>) {
        let (mut sim, ids) = setup();
        for &i in &ids[..3] {
            adult(&mut sim, i);
        }
        let cost = BuildingKind::Lodge.spec().cost.to_vec();
        for &(g, a) in &cost {
            assert_eq!(sim.buildings.stores.deposit(g, a).unwrap(), a, "ledger has room for the cost");
        }
        warehouse(&mut sim, 100, (20, 9));
        crate::stock::reconcile(&mut sim);
        sim.buildings.buildings.push(Building { id: BuildingId(101), kind: BuildingKind::Burrow, pos: (26, 9), status: Status::Complete, recipe: None, output: 0.0 });
        *sim.water.depth.get_mut(14, 14).unwrap() = 0.3;
        let site = sim.place_site(BuildingKind::Lodge, (14, 14)).unwrap();
        for (n, &i) in ids[..3].iter().enumerate() {
            spawn_on(&mut sim, i, (10, 10 + 2 * n)).unwrap();
        }
        (sim, ids, site, cost)
    }

    fn delivered(sim: &SimState, site: BuildingId) -> f32 {
        sim.work.place(site).and_then(|p| p.site.as_ref()).map_or(0.0, |s| s.delivered.iter().map(|a| a.1).sum())
    }

    fn in_transit(sim: &SimState, site: BuildingId) -> f32 {
        sim.agents
            .by_id
            .values()
            .filter(|a| matches!(a.task, Task::Carry { dest } if dest == site))
            .filter_map(|a| a.load.map(|l| l.amount))
            .sum()
    }

    #[test]
    #[ignore = "beavers: off until the water engine is done"]
    fn site_slots_accept_two_beavers_reject_a_third_and_site_uses_exactly_its_cost() {
        let (mut sim, ids, site, cost) = lodge_site();
        assert_eq!(sim.work.place(site).unwrap().slots.len(), 2);
        fill(&mut sim, site, 0, ids[0]).unwrap();
        fill(&mut sim, site, 1, ids[1]).unwrap();
        assert!(fill(&mut sim, site, 0, ids[2]).is_err(), "slot 0 is taken");
        assert!(fill(&mut sim, site, 1, ids[2]).is_err(), "slot 1 is taken");
        assert!(fill(&mut sim, site, 2, ids[2]).is_err(), "no third slot");
        let workers: Vec<_> = sim.work.place(site).unwrap().slots.iter().map(|s| s.worker).collect();
        assert_eq!(workers, [Some(ids[0]), Some(ids[1])]);

        let total: f32 = cost.iter().map(|a| a.1).sum();
        let mut loads_delivered = 0;
        for _ in 0..40_000 {
            let (before, carried) = (delivered(&sim, site), in_transit(&sim, site));
            step(&mut sim, DT);
            let gain = delivered(&sim, site) - before;
            if gain > 0.0 {
                loads_delivered += (gain / LOAD_SIZE - 1e-3).ceil() as usize;
                assert!(carried >= gain - 1e-3, "delivered {gain} but only {carried} was being carried");
                assert!(gain <= 2.0 * LOAD_SIZE + 1e-3, "two builders deliver at most a load each");
            }
            if sim.buildings.building(site).unwrap().is_complete() {
                break;
            }
        }
        assert!(sim.buildings.building(site).unwrap().is_complete(), "lodge never finished");
        assert!((delivered(&sim, site) - total).abs() < 1e-3);
        assert!(loads_delivered >= (total / LOAD_SIZE).ceil() as usize, "every unit arrived in loads of at most {LOAD_SIZE}");
        for &(g, a) in &cost {
            assert!((sim.buildings.stores.consumed_total[g.index()] - a).abs() < 1e-3, "{g:?} used exactly its cost");
            assert!(sim.buildings.stores.amount(g).abs() < 1e-3, "{g:?} left over");
        }
        assert!(sim.agents.by_id.values().all(|a| a.load.is_none()), "nothing still carried");
    }

    #[test]
    #[ignore = "beavers: off until the water engine is done"]
    fn site_progress_rises_with_loads_and_labour_and_is_full_only_when_complete() {
        let (mut sim, ids, site, cost) = lodge_site();
        fill(&mut sim, site, 0, ids[0]).unwrap();
        fill(&mut sim, site, 1, ids[1]).unwrap();
        let total: f32 = cost.iter().map(|a| a.1).sum();
        assert_eq!(sim.work.progress(site), Some(0.0));
        let labour = |sim: &SimState| sim.work.place(site).unwrap().site.as_ref().unwrap().labour_done;
        let (mut saw_load_rise, mut saw_labour_rise) = (false, false);
        let mut last = 0.0;
        for _ in 0..40_000 {
            let (mats, lab) = (delivered(&sim, site), labour(&sim));
            step(&mut sim, DT);
            let now = sim.work.progress(site).unwrap();
            assert!(now >= last, "progress fell from {last} to {now}");
            if delivered(&sim, site) > mats {
                assert!(now > last, "a delivered load must raise progress");
                saw_load_rise = true;
            }
            if labour(&sim) > lab {
                assert!(now > last, "builder labour must raise progress");
                saw_labour_rise = true;
            }
            if delivered(&sim, site) < total - 1e-3 {
                assert!(now < 1.0, "progress {now} reached 100% before the full cost was delivered");
            }
            let complete = sim.buildings.building(site).unwrap().is_complete();
            assert!(complete == (now == 1.0), "100% exactly on completion (progress {now}, complete {complete})");
            last = now;
            if complete {
                break;
            }
        }
        assert!(saw_load_rise && saw_labour_rise && last == 1.0);
    }

    #[test]
    #[ignore = "beavers: off until the water engine is done"]
    fn a_hauler_at_a_balanced_store_evens_out_the_balanced_stores_without_orders() {
        let (mut sim, ids) = setup();
        adult(&mut sim, ids[0]);
        let (a, b) = (warehouse(&mut sim, 1, (20, 9)), warehouse(&mut sim, 2, (10, 30)));
        sim.work.places.push(Workplace {
            building: a,
            kind: BuildingKind::Warehouse,
            slots: vec![Slot::empty(Role::Haul)],
            area: None,
            site: None, priority: Default::default(), paused: false, max_workers: 1,
        });
        sim.work.stock_add(a, Good::Logs, 24.0);
        spawn_on(&mut sim, ids[0], (10, 10)).unwrap();
        fill(&mut sim, a, 0, ids[0]).unwrap();
        for _ in 0..4_000 {
            step(&mut sim, DT);
        }
        assert_eq!(sim.work.stock_of(b, Good::Logs), 0.0, "without the setting nothing moves");
        sim.apply(Command::SetBalance { building: a, on: true }).unwrap();
        sim.apply(Command::SetBalance { building: b, on: true }).unwrap();
        assert!(sim.apply(Command::SetBalance { building: BuildingId(77), on: true }).is_err(), "only a finished store");
        for _ in 0..20_000 {
            step(&mut sim, DT);
            let carried = sim.agents.get(ids[0]).unwrap().load.map_or(0.0, |l| l.amount);
            let (na, nb) = (sim.work.stock_of(a, Good::Logs), sim.work.stock_of(b, Good::Logs));
            assert!((na + nb + carried - 24.0).abs() < 1e-3, "units vanish or appear");
        }
        let (na, nb) = (sim.work.stock_of(a, Good::Logs), sim.work.stock_of(b, Good::Logs));
        assert!((na - nb).abs() <= 2.0 * crate::stock::BALANCE_SLACK, "two equal stores end even: {na} and {nb}");
        assert!(sim.agents.get(ids[0]).unwrap().load.is_none(), "and the hauler stops");
    }

    #[test]
    #[ignore = "beavers: off until the water engine is done"]
    fn hauler_moves_goods_between_two_stores_and_every_unit_is_carried() {
        let (mut sim, ids) = setup();
        adult(&mut sim, ids[0]);
        let (a, b) = (warehouse(&mut sim, 1, (20, 9)), warehouse(&mut sim, 2, (10, 30)));
        sim.work.places.push(Workplace {
            building: a,
            kind: BuildingKind::Warehouse,
            slots: vec![Slot::empty(Role::Haul)],
            area: None,
            site: None, priority: Default::default(), paused: false, max_workers: 1,
        });
        sim.work.stock_add(a, Good::Logs, 10.0);
        spawn_on(&mut sim, ids[0], (10, 10)).unwrap();
        fill(&mut sim, a, 0, ids[0]).unwrap();
        assert!(sim.apply(Command::SetHaul { order: HaulOrder { from: a, to: a, good: Good::Logs } }).is_err());
        sim.apply(Command::SetHaul { order: HaulOrder { from: a, to: b, good: Good::Logs } }).unwrap();
        let (mut deliveries, mut max_load) = (0, 0.0f32);
        for _ in 0..20_000 {
            let sb = sim.work.stock_of(b, Good::Logs);
            step(&mut sim, DT);
            let carried = sim.agents.get(ids[0]).unwrap().load.map_or(0.0, |l| l.amount);
            max_load = max_load.max(carried);
            let (na, nb) = (sim.work.stock_of(a, Good::Logs), sim.work.stock_of(b, Good::Logs));
            assert!((na + nb + carried - 10.0).abs() < 1e-3, "units vanish or appear: {na} + {nb} + {carried}");
            if nb > sb {
                deliveries += 1;
            }
            if nb >= 10.0 - 1e-3 {
                break;
            }
        }
        assert!((sim.work.stock_of(b, Good::Logs) - 10.0).abs() < 1e-3 && sim.work.stock_of(a, Good::Logs) < 1e-3);
        assert_eq!(deliveries, 4, "10 units in loads of {LOAD_SIZE}");
        assert!(max_load > 0.0 && max_load <= LOAD_SIZE + 1e-3);
        assert_eq!(total(&sim, Good::Logs), 0.0, "hauling is not production");
    }

    #[test]
    #[ignore = "beavers: off until the water engine is done"]
    fn workers_gather_only_inside_the_current_area_after_it_is_moved_and_resized() {
        let (mut sim, ids) = setup();
        adult(&mut sim, ids[0]);
        warehouse(&mut sim, 1, (24, 9));
        let camp = BuildingId(3);
        sim.buildings.buildings.push(Building {
            id: camp,
            kind: BuildingKind::GuardPost,
            pos: (5, 5),
            status: Status::Complete,
            recipe: None,
            output: 0.0,
        });
        let area = Area { min: (12, 8), max: (16, 12) };
        sim.work.places.push(Workplace {
            building: camp,
            kind: BuildingKind::GuardPost,
            slots: vec![Slot::empty(Role::Collect(ResourceKind::Tree))],
            area: Some(area),
            site: None, priority: Default::default(), paused: false, max_workers: 1,
        });
        let trees = [(13, 9), (15, 11), (14, 10), (12, 12), (16, 8), (13, 12), (18, 13), (19, 14), (20, 15), (21, 16), (19, 12), (22, 13), (17, 17)];
        for c in trees {
            sim.resources.add(ResourceKind::Tree, c, FELL_WORK);
        }
        spawn_on(&mut sim, ids[0], (10, 10)).unwrap();
        fill(&mut sim, camp, 0, ids[0]).unwrap();
        let snapshot = |sim: &SimState| {
            sim.resources.items.iter().map(|r| (r.id, r.cell, r.kind, r.amount, r.branches)).collect::<Vec<_>>()
        };
        let moved = Area { min: (18, 12), max: (22, 16) };
        let resized = Area { min: (18, 12), max: (20, 14) };
        let phases = [(area, 300usize), (moved, 400), (resized, 4000)];
        let mut gathers = [0usize; 3];
        for (n, &(current, ticks)) in phases.iter().enumerate() {
            if n > 0 {
                sim.apply(Command::SetWorkArea { building: camp, area: current }).unwrap();
            }
            for _ in 0..ticks {
                let before = snapshot(&sim);
                step(&mut sim, DT);
                for (old, new) in before.iter().zip(snapshot(&sim)) {
                    if *old != new {
                        gathers[n] += 1;
                        assert!(current.contains(new.1), "phase {n}: gathered at {:?} outside {current:?}", new.1);
                    }
                }
            }
        }
        assert!(gathers.iter().all(|&g| g > 0), "workers gathered in every phase: {gathers:?}");
        assert!(sim.work.set_area(camp, Area { min: (5, 5), max: (4, 9) }).is_err(), "inverted area rejected");
    }
}
