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
    pub next_work_probe: f64,
    pub next_home_probe: f64,
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
pub const FOOD_GATHER_PER_DAY: f32 = 2.4;
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
            let work = if sim.time_days >= ag.next_work_probe {
                ag.next_work_probe=sim.time_days+1.0/96.0;
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
                if ag.path.is_none() { ag.next_home_probe=sim.time_days+1.0/96.0; }
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
    let rate = if good == Good::Food { FOOD_GATHER_PER_DAY } else { GATHER_PER_DAY };
    let mut want = (dt * ag.speed * rate).min(LOAD_SIZE - have);
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
    ag.inside || home.is_some_and(|b| b.kind == BuildingKind::Burrow
        && b.pos.0.abs_diff(ag.cell.0)+b.pos.1.abs_diff(ag.cell.1)<=1)
}

#[cfg(test)]
mod rest_tests {
    use super::*;
    #[test]
    fn burrow_side_is_a_valid_rest_destination_without_an_inside_flag() {
        let home=Building {id:BuildingId(1),kind:BuildingKind::Burrow,pos:(4,4),status:crate::buildings::Status::Complete,recipe:None,output:0.0};
        let mut agents=Agents::default();agents.spawn(BeaverId(1),(4,3));
        let a=agents.get(BeaverId(1)).unwrap();
        assert!(!a.inside);assert!(at_resting_place(a,Some(&home)));
        agents.by_id.get_mut(&BeaverId(1)).unwrap().cell=(4,2);
        assert!(!at_resting_place(agents.get(BeaverId(1)).unwrap(),Some(&home)));
    }
}

