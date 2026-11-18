//! Beaver agents: bodies on the navigation grid and the task state machine (owner: `collect`;
//! Build/Fetch/Haul behaviour for slot workers lives in the same `step`). Person data (age, needs,
//! skills, family) stays in `colony::Beaver`; an `Agent` is keyed by the same `BeaverId`.
//!
//! State machine (at most one transition per tick):
//! ```text
//! Idle ──assigned──────▶ Collect (go, work until load full) ──▶ Carry (go to store, deposit) ──▶ Collect
//! Idle ──site slot─────▶ Fetch (store, take) ──▶ Carry (to site) ──▶ Build (work) ──▶ Fetch …
//! Idle ──haul order────▶ Fetch (from store) ──▶ Carry (to `to` store, deposit) ──▶ Fetch …
//! any ──tired/unassigned/kit──▶ Rest (go to lodge entrance, enter)
//! ```
//! Kits never leave Idle/Rest; unassigned adults rest. Totals change only on deposit.

use std::collections::BTreeMap;

use crate::buildings::{Building, BuildingId, BuildingKind, Good};
use crate::colony::{BeaverId, Stage};
use crate::nav::{Gait, Goal, NavGrid, Path, Surface};
use crate::resources::{Resource, ResourceId, ResourceKind};
use crate::water::Cell;
use crate::work::{Area, Role, EPS as WORK_EPS, LABOUR_PER_DAY};
use crate::SimState;

/// What the beaver carries; a load is sim state so the renderer can show it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Load {
    pub good: Good,
    pub amount: f32,
}

/// Where a collector works.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Assignment {
    /// A specific resource entity (right-click).
    Resource(ResourceId),
    /// Any resource of `kind` inside `area` (drag).
    Area(ResourceKind, Area),
    /// Slot `slot` of a workplace (uses that workplace's area and role).
    Slot(BuildingId, usize),
}

#[derive(Clone, Debug, PartialEq)]
pub enum Task {
    Idle,
    /// Walking to / working at a resource until a load is full.
    Collect { target: ResourceId, progress: f32 },
    /// Walking to `dest` with `Agent::load`; on arrival the load is deposited.
    Carry { dest: BuildingId },
    /// Walking to a store to pick up `good`, then carrying it to `then` (site or store).
    Fetch { from: BuildingId, good: Good, then: BuildingId },
    /// Working on a site.
    Build { site: BuildingId },
    /// Walking to store `to` with a load taken from another store; on arrival it joins `to`'s stock.
    Haul { to: BuildingId },
    /// Walking to / sleeping in the home lodge.
    Rest,
}

impl Task {
    /// Short label for panels and number provenance.
    pub fn label(&self) -> &'static str {
        match self {
            Task::Idle => "idle",
            Task::Collect { .. } => "collecting",
            Task::Carry { .. } => "carrying",
            Task::Fetch { .. } => "fetching",
            Task::Build { .. } => "building",
            Task::Haul { .. } => "hauling",
            Task::Rest => "resting",
        }
    }
}

#[derive(Clone, Debug)]
pub struct Agent {
    pub id: BeaverId,
    /// Cell the agent occupies (always reserved in `NavGrid`).
    pub cell: Cell,
    /// Progress `0..1` from `cell` towards `path.steps[0]`, for smooth drawing.
    pub step_frac: f32,
    pub gait: Gait,
    pub task: Task,
    pub assignment: Option<Assignment>,
    pub load: Option<Load>,
    pub path: Option<Path>,
    /// True while inside the lodge (hidden from the map).
    pub inside: bool,
    /// Ticks without progress; above `STUCK_TICKS` tests report a bug.
    pub stuck_ticks: u32,
    /// Work-speed factor from mood, refreshed each tick.
    pub speed: f32,
}

pub const STUCK_TICKS: u32 = 600;

#[derive(Clone, Debug, Default)]
pub struct Agents {
    pub by_id: BTreeMap<BeaverId, Agent>,
    /// Resources no collector could find a path to, with the day they may be tried again (a tree
    /// on a cliff top must not hold its collector in place for ever).
    pub unreachable: BTreeMap<ResourceId, f64>,
}

/// Days a resource nobody could reach is skipped before collectors try it again.
pub const UNREACHABLE_RETRY_DAYS: f64 = 10.0;

impl Agents {
    pub fn get(&self, id: BeaverId) -> Option<&Agent> {
        self.by_id.get(&id)
    }
    /// Spawns an agent on `cell` (caller guarantees it is free).
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
            },
        );
    }
    /// Sets or clears a collection assignment; false if no such agent. Kits and unassigned adults
    /// do no work (enforced in `step`).
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

/// Cost points an agent travels per day (a land step costs 1, a swim step 0.6).
pub const MOVE_COST_PER_DAY: f32 = 30.0;
/// Felling work done per day by one collector (a tree needs `FELL_WORK`).
pub const FELL_PER_DAY: f32 = 1.0;
/// Units gathered per day while working a resource.
pub const GATHER_PER_DAY: f32 = 6.0;
/// Size of one load; a collector carries one load of one good at a time.
pub const LOAD_SIZE: f32 = 3.0;
/// Amounts below this are treated as zero.
const EPS: f32 = 1e-3;
/// Nearest free cell search radius when placing a beaver on the map.
const SPAWN_RADIUS: usize = 12;
/// Placeholder `Carry` destination meaning "choose the nearest store".
const NO_STORE: BuildingId = BuildingId(0);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpawnError {
    /// No such beaver in the colony.
    NoSuchBeaver,
    AlreadySpawned,
    /// The cell is out of bounds, solid, or held by another agent.
    CellUnavailable,
}

/// Puts the colony beaver `id` on the map at `cell`, reserving the cell.
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

/// Puts every colony beaver on the map (beside its position) without running a step; used by
/// starts so a freshly built state already shows its beavers. Rebuilds the navigation grid first.
pub fn place_beavers(sim: &mut SimState) {
    let mut nav = std::mem::replace(&mut sim.nav, NavGrid::new(1, 1));
    nav.rebuild(sim);
    sim.nav = nav;
    sync_agents(sim);
}

/// Spawns agents for colony beavers that have none (beside their position) and removes agents
/// of beavers that no longer exist, freeing their cells.
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
        // No free cell near the beaver: it stays off the map until space opens up.
        if let Some(cell) = free {
            spawn_on(sim, id, cell).expect("cell checked free and beaver checked missing");
        }
    }
}

// ───────────────────────────── collection targets ─────────────────────────────

/// A resolved collection assignment.
struct Job {
    kinds: &'static [ResourceKind],
    area: Option<Area>,
    only: Option<ResourceId>,
}

/// Whether anything a collector wants is left (bark is food, not part of wood loads).
fn gatherable(r: &Resource) -> bool {
    match r.kind {
        ResourceKind::Tree => true,
        ResourceKind::Felled => r.amount > 0.0 || r.branches > 0.0,
        _ => r.amount > 0.0,
    }
}

/// Kinds to look for, best first: a felled tree is finished before a new one is felled.
fn kinds_for(k: ResourceKind) -> &'static [ResourceKind] {
    match k {
        ResourceKind::Tree | ResourceKind::Felled => &[ResourceKind::Felled, ResourceKind::Tree],
        ResourceKind::Plant => &[ResourceKind::Plant],
        ResourceKind::MudBank => &[ResourceKind::MudBank],
        ResourceKind::StoneOutcrop => &[ResourceKind::StoneOutcrop],
    }
}

impl Job {
    /// `None` if the assignment cannot be worked (gone resource, non-collect slot, no area).
    fn resolve(sim: &SimState, a: Assignment) -> Option<Job> {
        match a {
            Assignment::Resource(id) => sim
                .resources
                .get(id)
                .filter(|r| gatherable(r))
                .map(|_| Job { kinds: &[], area: None, only: Some(id) }),
            Assignment::Area(kind, area) => Some(Job { kinds: kinds_for(kind), area: Some(area), only: None }),
            Assignment::Slot(b, slot) => {
                let place = sim.work.place(b)?;
                match (place.slots.get(slot)?.role, place.area) {
                    (Role::Collect(kind), Some(area)) => Some(Job { kinds: kinds_for(kind), area: Some(area), only: None }),
                    _ => None,
                }
            }
        }
    }
    fn permits(&self, r: &Resource) -> bool {
        gatherable(r)
            && self.only.is_none_or(|id| id == r.id)
            && self.area.is_none_or(|a| a.contains(r.cell))
            && (self.only.is_some() || self.kinds.contains(&r.kind))
    }
    /// Nearest permitted resource (preferred kind first, then squared distance, then id).
    fn pick(&self, sim: &SimState, from: Cell) -> Option<ResourceId> {
        let d = |c: Cell| c.0.abs_diff(from.0).pow(2) + c.1.abs_diff(from.1).pow(2);
        let rank = |r: &Resource| self.kinds.iter().position(|&k| k == r.kind).unwrap_or(0);
        let reachable = |r: &Resource| sim.agents.unreachable.get(&r.id).is_none_or(|until| sim.time_days >= *until);
        sim.resources
            .items
            .iter()
            .filter(|r| self.permits(r) && reachable(r))
            .min_by_key(|r| (rank(r), d(r.cell), r.id))
            .map(|r| r.id)
    }
}

// ───────────────────────────── storage and lodges ─────────────────────────────

pub(crate) fn accepts(kind: BuildingKind, good: Good) -> bool {
    match kind {
        BuildingKind::Warehouse | BuildingKind::Storehouse => true,
        BuildingKind::FoodCache => good == Good::Food,
        _ => false,
    }
}

fn adjacent(c: Cell, b: &Building) -> bool {
    b.cells().any(|f| f.0.abs_diff(c.0) + f.1.abs_diff(c.1) == 1)
}

/// Cheapest route from `from` to beside any footprint cell of a store accepting `good`.
fn nearest_store(sim: &SimState, who: BeaverId, from: Cell, good: Good) -> Option<(BuildingId, Path)> {
    sim.buildings
        .buildings
        .iter()
        .filter(|b| b.is_complete() && accepts(b.kind, good))
        .filter_map(|b| {
            if adjacent(from, b) {
                return Some((b.id, Path { steps: Vec::new(), cost: 0.0 }));
            }
            let path = b
                .cells()
                .filter_map(|c| sim.nav.find_path(who, from, Goal::Beside(c)))
                .min_by(|a, b| a.cost.total_cmp(&b.cost))?;
            Some((b.id, path))
        })
        .min_by(|a, b| a.1.cost.total_cmp(&b.1.cost).then(a.0.cmp(&b.0)))
}

fn has_lodge(sim: &SimState) -> bool {
    sim.buildings.buildings.iter().any(|b| b.kind == BuildingKind::Lodge && b.is_complete())
}

/// Cheapest route into a lodge interior cell.
fn route_home(sim: &SimState, who: BeaverId, from: Cell) -> Option<Path> {
    sim.buildings
        .buildings
        .iter()
        .filter(|b| b.kind == BuildingKind::Lodge && b.is_complete())
        .flat_map(|b| b.cells().collect::<Vec<_>>())
        .filter_map(|c| sim.nav.find_path(who, from, Goal::Inside(c)))
        .min_by(|a, b| a.cost.total_cmp(&b.cost))
}

// ───────────────────────────── per-agent tick ─────────────────────────────

/// Follows `ag.path` for `dt_days`, at most one cell per call. True if the agent moved.
fn follow(nav: &mut NavGrid, ag: &mut Agent, dt_days: f32) -> bool {
    let Some(path) = ag.path.as_mut() else { return false };
    let Some(&(next, gait)) = path.steps.first() else {
        ag.path = None;
        return false;
    };
    let cost = nav.surface_at(next).and_then(NavGrid::step_cost).map_or(1.0, |c| c.0);
    ag.gait = gait;
    ag.step_frac += dt_days * MOVE_COST_PER_DAY / cost;
    if ag.step_frac < 1.0 {
        return false;
    }
    if nav.advance(ag.id, ag.cell, next).is_err() {
        // Blocked (another agent, changed terrain): drop the route and replan next tick.
        ag.path = None;
        ag.step_frac = 0.0;
        return false;
    }
    ag.step_frac -= 1.0;
    ag.cell = next;
    path.steps.remove(0);
    ag.inside = nav.surface_at(next) == Some(Surface::LodgeInside);
    true
}

/// Records progress (or lack of it) for the stuck-agent check.
fn note(ag: &mut Agent, progressed: bool) {
    ag.stuck_ticks = if progressed { 0 } else { ag.stuck_ticks.saturating_add(1) };
}

/// What an agent does with its working time, from its slot or its assignment.
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
        Some((b, _, Role::Build)) if sim.work.place(b).is_some_and(|p| p.site.is_some()) => Duty::Build(b),
        Some((_, _, Role::Haul)) => Duty::Haul,
        Some((b, i, Role::Collect(_))) => Job::resolve(sim, Assignment::Slot(b, i)).map_or(Duty::None, Duty::Collect),
        Some(_) => Duty::None,
        None => ag.assignment.and_then(|a| Job::resolve(sim, a)).map_or(Duty::None, Duty::Collect),
    }
}

/// Goods of `good` on their way to `site`: loads being carried plus pickups under way.
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

/// Closest complete store accepting `good` by straight-line distance (cheap; routes are planned
/// on arrival).
fn closest_store(sim: &SimState, from: Cell, good: Good) -> Option<BuildingId> {
    let d = |b: &Building| b.cells().map(|c| c.0.abs_diff(from.0).pow(2) + c.1.abs_diff(from.1).pow(2)).min().unwrap_or(0);
    sim.buildings.buildings.iter().filter(|b| b.is_complete() && accepts(b.kind, good)).min_by_key(|b| (d(b), b.id)).map(|b| b.id)
}

/// Next builder step for `site`: fetch a missing material nobody is already bringing, else build.
fn build_task(sim: &SimState, site: BuildingId, ag: &Agent) -> Option<Task> {
    if sim.buildings.building(site).is_none_or(|b| b.is_complete()) {
        return None;
    }
    let progress = sim.work.place(site)?.site.as_ref()?;
    for &(good, _) in &progress.needed {
        let outstanding = progress.missing(good) - promised(sim, site, good);
        if outstanding > WORK_EPS && sim.buildings.stores.amount(good) > WORK_EPS {
            if let Some(from) = closest_store(sim, ag.cell, good) {
                return Some(Task::Fetch { from, good, then: site });
            }
        }
    }
    (progress.labour_done < progress.labour_needed).then_some(Task::Build { site })
}

/// Next hauler step: the first standing order whose source store holds goods.
fn haul_task(sim: &SimState) -> Option<Task> {
    let ok = |id| sim.buildings.building(id).is_some_and(|b| b.is_complete());
    sim.work
        .hauls
        .iter()
        .find(|o| ok(o.from) && ok(o.to) && sim.work.stock_of(o.from, o.good) > WORK_EPS)
        .map(|o| Task::Fetch { from: o.from, good: o.good, then: o.to })
}

/// The task an idle agent with `duty` would start, if any.
fn next_task(sim: &SimState, duty: &Duty, ag: &Agent) -> Option<Task> {
    match duty {
        Duty::None => None,
        Duty::Collect(j) => j.pick(sim, ag.cell).map(|t| Task::Collect { target: t, progress: 0.0 }),
        Duty::Build(site) => build_task(sim, *site, ag),
        Duty::Haul => haul_task(sim),
    }
}

fn tick(sim: &mut SimState, ag: &mut Agent, dt: f32) {
    // Kits never work; unassigned adults rest.
    // An exhausted beaver (colony `resting`) stops work and goes home until rested.
    let (can_work, tired) = sim.colony.beaver(ag.id).map_or((false, false), |b| (b.stage != Stage::Kit, b.resting));
    let duty = duty_of(sim, ag, can_work && !tired);
    ag.speed = sim.colony.beaver(ag.id).map_or(1.0, |b| crate::society::work_speed(b.mood));
    if tired && ag.load.is_none() && !ag.inside && !matches!(ag.task, Task::Rest | Task::Carry { .. }) {
        ag.path = None;
        ag.task = Task::Rest;
    }
    // A single-resource assignment ends when its resource is used up.
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
    }
}

/// Walks beside building `target`; true once there.
fn approach(sim: &mut SimState, ag: &mut Agent, dt: f32, target: BuildingId) -> bool {
    let Some(b) = sim.buildings.building(target) else {
        ag.path = None;
        note(ag, false);
        return false;
    };
    if adjacent(ag.cell, b) {
        ag.path = None;
        return true;
    }
    if ag.path.is_none() {
        let nav = &sim.nav;
        ag.path = b
            .cells()
            .filter_map(|c| nav.find_path(ag.id, ag.cell, Goal::Beside(c)))
            .min_by(|a, b| a.cost.total_cmp(&b.cost));
    }
    let moved = follow(&mut sim.nav, ag, dt);
    note(ag, moved);
    false
}

/// Gives up on the current errand; a held load goes to the nearest store.
fn abandon(ag: &mut Agent) {
    ag.path = None;
    ag.task = if ag.load.is_some() { Task::Carry { dest: NO_STORE } } else { Task::Idle };
}

/// Fetch: walk to store `from`, then take a load for `then` (site: from the colony ledger;
/// store: from `from`'s stock).
fn fetch(sim: &mut SimState, ag: &mut Agent, dt: f32, duty: &Duty, from: BuildingId, good: Good, then: BuildingId) {
    let site = sim.work.place(then).is_some_and(|p| p.site.is_some());
    let still_wanted = match duty {
        Duty::Build(s) => site && *s == then,
        Duty::Haul => !site && sim.work.hauls.iter().any(|o| (o.from, o.to, o.good) == (from, then, good)),
        _ => false,
    };
    if !still_wanted || !sim.buildings.building(from).is_some_and(|b| b.is_complete()) {
        return abandon(ag);
    }
    if !approach(sim, ag, dt, from) {
        return;
    }
    note(ag, true);
    if site {
        let outstanding = sim.work.place(then).and_then(|p| p.site.as_ref()).map_or(0.0, |s| s.missing(good))
            - promised(sim, then, good);
        let want = LOAD_SIZE.min(outstanding).min(sim.buildings.stores.amount(good));
        if want <= WORK_EPS || sim.buildings.stores.withdraw(good, want).is_err() {
            return abandon(ag);
        }
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

/// Carry a load to a site and hand it over; any excess goes to the nearest store.
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

/// Build: work beside the site, adding labour each tick.
fn build(sim: &mut SimState, ag: &mut Agent, dt: f32, duty: &Duty, site: BuildingId) {
    let open = sim.buildings.building(site).is_some_and(|b| !b.is_complete())
        && sim.work.place(site).and_then(|p| p.site.as_ref()).is_some_and(|s| s.labour_done < s.labour_needed);
    if !open || !matches!(duty, Duty::Build(s) if *s == site) {
        return abandon(ag);
    }
    if approach(sim, ag, dt, site) {
        sim.work.add_labour(site, dt * LABOUR_PER_DAY).expect("duty checked the site");
        note(ag, true);
    }
}

/// Haul: carry a load to store `to` and add it to that store's stock. If `to` has gone, the
/// load goes to the nearest store instead.
fn haul(sim: &mut SimState, ag: &mut Agent, dt: f32, to: BuildingId) {
    let Some(load) = ag.load else {
        ag.task = Task::Idle;
        return;
    };
    let to = if sim.buildings.building(to).is_some_and(|b| b.is_complete()) {
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

/// Completes sites whose cost is delivered and labour done: the building turns `Complete` and
/// its workplace takes the catalogue slots and default area.
fn finish_sites(sim: &mut SimState) {
    let done: Vec<BuildingId> =
        sim.work.places.iter().filter(|p| p.site.as_ref().is_some_and(|s| s.is_done())).map(|p| p.building).collect();
    for id in done {
        let Some(b) = sim.buildings.buildings.iter_mut().find(|b| b.id == id && b.status == crate::buildings::Status::Site)
        else {
            continue;
        };
        b.status = crate::buildings::Status::Complete;
        let area = crate::catalogue::work_area(b);
        let def = crate::catalogue::def(b.kind);
        sim.work.finish_site(id, &def.slots, area).expect("workplace exists");
        if let Some(i) = sim.work.dam_plans.iter().position(|p| p.building == id) {
            let plan = sim.work.dam_plans.remove(i);
            if let Err(e) = sim.water.build_dam(&plan.cells, plan.height) {
                sim.work.dam_failures.push((id, e.to_string()));
            }
        }
    }
}

// ───────────────────────────── automatic organisation ─────────────────────────────

/// Radius (cells) around a site within which automatic gatherers look for materials.
const GATHER_RADIUS: usize = 30;

/// The resource a missing good is gathered from.
fn source_of(good: Good) -> ResourceKind {
    match good {
        Good::Logs | Good::Sticks => ResourceKind::Tree,
        Good::Mud => ResourceKind::MudBank,
        Good::Stone => ResourceKind::StoneOutcrop,
        _ => ResourceKind::Plant,
    }
}

/// Staffs open sites and sends gatherers for the materials the colony ledger cannot cover.
/// Only beavers with no player order (no slot, no assignment) or ones this function placed itself
/// are touched; a beaver the player re-assigns is dropped from `Work::managed`. Half the free
/// workers (rounded up) gather while any material is short; the rest take builder slots.
fn organise(sim: &mut SimState) {
    use crate::work::ManagedDuty;
    // Drop beavers that left or were re-assigned by the player.
    let managed: Vec<(BeaverId, ManagedDuty)> = sim.work.managed.iter().map(|(&k, &v)| (k, v)).collect();
    for (id, duty) in managed {
        let same = match duty {
            ManagedDuty::Gather(a) => sim.work.slot_of(id).is_none() && sim.agents.get(id).is_some_and(|ag| ag.assignment == Some(a)),
            ManagedDuty::Build(b, s) => sim.work.slot_of(id).is_some_and(|(pb, ps, _)| (pb, ps) == (b, s)),
        };
        if !same || sim.colony.beaver(id).is_none() {
            sim.work.managed.remove(&id);
        }
    }
    let sites: Vec<(BuildingId, Cell)> = sim
        .work
        .places
        .iter()
        .filter(|p| p.site.as_ref().is_some_and(|s| !s.is_done()))
        .filter_map(|p| sim.buildings.building(p.building).map(|b| (p.building, b.pos)))
        .collect();
    let free: Vec<BeaverId> = sim
        .colony
        .beavers
        .iter()
        .filter(|b| b.stage != Stage::Kit && sim.agents.by_id.contains_key(&b.id))
        .filter(|b| {
            sim.work.managed.contains_key(&b.id)
                || (sim.work.slot_of(b.id).is_none() && sim.agents.get(b.id).is_some_and(|a| a.assignment.is_none()))
        })
        .map(|b| b.id)
        .collect();
    // First good the ledger cannot cover, across all open sites.
    let short = sites.iter().find_map(|&(site, pos)| {
        let progress = sim.work.place(site)?.site.as_ref()?;
        let good = progress.needed.iter().map(|n| n.0).find(|&g| {
            let missing: f32 = sites.iter().filter_map(|s| sim.work.place(s.0)?.site.as_ref()).map(|p| p.missing(g)).sum();
            missing - sim.buildings.stores.amount(g) > WORK_EPS
        })?;
        Some((good, pos))
    });
    let gatherers = if short.is_some() { free.len().div_ceil(2) } else { 0 };
    let mut builder_slots = sites.iter().flat_map(|&(s, _)| (0..crate::work::SITE_SLOTS).map(move |i| (s, i)));
    let desired: Vec<(BeaverId, Option<ManagedDuty>)> = free
        .iter()
        .enumerate()
        .map(|(n, &id)| {
            let duty = match short {
                Some((good, pos)) if n < gatherers => {
                    let r = GATHER_RADIUS;
                    let area = Area { min: (pos.0.saturating_sub(r), pos.1.saturating_sub(r)), max: (pos.0 + r, pos.1 + r) };
                    Some(ManagedDuty::Gather(Assignment::Area(source_of(good), area)))
                }
                _ => builder_slots.next().map(|(s, i)| ManagedDuty::Build(s, i)),
            };
            (id, duty)
        })
        .collect();
    // Release first so slots a worker leaves can be taken by another in the same pass.
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
            ManagedDuty::Build(b, i) => sim.work.assign(b, i, id).is_ok(),
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
        // Target gone, depleted, outside the area, or assignment changed: finish any load, re-pick.
        ag.path = None;
        ag.task = if ag.load.is_some() { Task::Carry { dest: NO_STORE } } else { Task::Idle };
        return;
    };
    let near = res.cell.0.abs_diff(ag.cell.0) + res.cell.1.abs_diff(ag.cell.1) <= 1;
    if !near {
        if ag.path.is_none() {
            ag.path = sim.nav.find_path(ag.id, ag.cell, Goal::Beside(res.cell));
            if ag.path.is_none() {
                // No way there: skip this resource for a while and pick another next tick.
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
        // The tree frees its cell when it falls; the nav grid follows at once.
        if sim.resources.fell(target, dt * ag.speed * FELL_PER_DAY).expect("kind checked above") {
            sim.nav.set(res.cell, Surface::Land);
        }
        return;
    }
    // Gather into the load; a load holds one good.
    let good = match res.kind {
        ResourceKind::Felled if res.amount > 0.0 => Good::Logs,
        ResourceKind::Felled => Good::Sticks,
        k => k.yields()[0],
    };
    if ag.load.is_some_and(|l| l.good != good) {
        ag.task = Task::Carry { dest: NO_STORE };
        return;
    }
    let have = ag.load.map_or(0.0, |l| l.amount);
    let left = if good == Good::Sticks { res.branches } else { res.amount };
    let mut want = (dt * ag.speed * GATHER_PER_DAY).min(LOAD_SIZE - have);
    if left - want < EPS {
        want = left; // never leave float dust behind that would become a tiny extra load
    }
    if let Some((g, got)) = sim.resources.take(target, want) {
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

/// An open site still short of `good` beyond what is already on its way, nearest first. Felled
/// wood goes straight to it: no stockpile is needed (the new game has none).
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
    let dest = if ag.path.is_some() || adjacent_to_store(sim, ag.cell, dest, load.good) {
        dest
    } else {
        // Choose the store with the cheapest route (swimming counts when it is shorter).
        let Some((id, path)) = nearest_store(sim, ag.id, ag.cell, load.good) else {
            if closest_store(sim, ag.cell, load.good).is_none() {
                // Nowhere to keep it and no site wants it (the new game has no stockpile): the
                // surplus of a felled tree is left behind rather than carried for ever.
                ag.load = None;
                ag.path = None;
                ag.task = Task::Idle;
                return;
            }
            note(ag, false); // no reachable store: keep the load and wait, visibly stuck
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
        match sim.buildings.stores.deposit(load.good, load.amount) {
            Ok(taken) if taken >= load.amount => {
                ag.load = None;
                ag.task = Task::Idle;
                note(ag, true);
            }
            Ok(taken) => {
                ag.load = Some(Load { good: load.good, amount: load.amount - taken });
                note(ag, taken > 0.0); // store full: keep the rest and wait
            }
            Err(_) => note(ag, false),
        }
        return;
    }
    let moved = follow(&mut sim.nav, ag, dt);
    note(ag, moved);
}

fn adjacent_to_store(sim: &SimState, c: Cell, dest: BuildingId, good: Good) -> bool {
    sim.buildings.building(dest).is_some_and(|b| b.is_complete() && accepts(b.kind, good) && adjacent(c, b))
}

/// One tick of every agent: pick task, path, move, work, deliver. Called once from
/// `SimState::step` after water and ecology; does nothing until the colony is founded.
/// Store totals change only when a load is deposited. (`build` adds Build/Fetch/haul inside it.)
pub fn step(sim: &mut SimState, dt_days: f32) {
    if !sim.colony.is_founded() {
        return;
    }
    let mut nav = std::mem::replace(&mut sim.nav, NavGrid::new(1, 1));
    nav.rebuild(sim);
    sim.nav = nav;
    sync_agents(sim);
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
    fn collectors_skip_a_resource_nobody_can_reach_until_the_retry_day() {
        let mut sim = SimState::new(1);
        let job = Job { kinds: &[ResourceKind::Tree], area: None, only: None };
        let first = job.pick(&sim, (64, 64)).expect("trees on the map");
        sim.agents.unreachable.insert(first, sim.time_days + UNREACHABLE_RETRY_DAYS);
        let second = job.pick(&sim, (64, 64)).expect("another tree");
        assert_ne!(first, second, "the unreachable tree is skipped");
        sim.time_days += UNREACHABLE_RETRY_DAYS;
        assert_eq!(job.pick(&sim, (64, 64)), Some(first), "and tried again after the retry days");
    }
    use crate::resources::{FELL_WORK, TREE_BRANCHES, TREE_LOGS};

    const DT: f32 = 0.02;

    /// Founded colony (adults at ids[0..2], yearlings after), empty resources, all-land nav.
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
    fn collector_fells_tree_and_delivers_to_nearest_store_only_on_delivery() {
        let (mut sim, ids) = setup();
        let near = warehouse(&mut sim, 1, (20, 9));
        warehouse(&mut sim, 2, (10, 40));
        let tree = sim.resources.add(ResourceKind::Tree, (14, 10), FELL_WORK);
        spawn_on(&mut sim, ids[0], (10, 10)).unwrap();
        sim.apply(crate::Command::Assign { who: vec![ids[0]], to: Some(Assignment::Resource(tree)) }).unwrap();
        let (mut felled, mut carried, mut deliveries) = (false, false, 0);
        for _ in 0..3000 {
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
    fn area_collector_stays_in_area_and_kits_and_unassigned_adults_idle() {
        let (mut sim, ids) = setup();
        warehouse(&mut sim, 1, (20, 9));
        let inside = [(14, 10), (15, 11)].map(|c| sim.resources.add(ResourceKind::Tree, c, FELL_WORK));
        // Closer to the collector than any tree in the area, but outside it.
        let outside = [(11, 10), (14, 14), (13, 7)].map(|c| sim.resources.add(ResourceKind::Tree, c, FELL_WORK));
        let area = Area { min: (13, 8), max: (18, 12) };
        sim.colony.beavers.iter_mut().find(|b| b.id == ids[2]).unwrap().stage = Stage::Kit;
        for (i, c) in [(0, (10, 10)), (1, (10, 12)), (2, (10, 14))] {
            spawn_on(&mut sim, ids[i], c).unwrap();
        }
        let start: Vec<Cell> = ids[1..3].iter().map(|&i| sim.agents.get(i).unwrap().cell).collect();
        sim.apply(crate::Command::Assign { who: vec![ids[0]], to: Some(Assignment::Area(ResourceKind::Tree, area)) }).unwrap();
        // A kit cannot be assigned at all; adult ids[1] is unassigned.
        assert!(sim.apply(crate::Command::Assign { who: vec![ids[2]], to: Some(Assignment::Resource(inside[0])) }).is_err());
        for _ in 0..4000 {
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
    fn carrier_swims_when_the_water_route_is_shorter() {
        let gaits = |river: bool| {
            let (mut sim, ids) = setup();
            // Hand-built terrain: no generated waterways, whatever the worldgen produces.
            sim.water.depth.as_mut_slice().fill(0.0);
            if river {
                // A river with its only land crossing far to the south.
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
            for _ in 0..3000 {
                step(&mut sim, DT);
                let ag = sim.agents.get(ids[0]).unwrap();
                seen.push(ag.gait);
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

    /// Digs a water-filled canal along row `y` for `xs`.
    fn dig(sim: &mut SimState, y: usize, xs: std::ops::RangeInclusive<usize>) {
        for x in xs {
            *sim.water.depth.get_mut(x, y).unwrap() = 1.0;
            *sim.water.canal.get_mut(x, y).unwrap() = true;
        }
    }

    #[test]
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
            for n in 1..=5000 {
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
                slots: vec![Slot { role: Role::Haul, worker: None }],
                area: None,
                site: None,
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

    // ───────────── construction, slots, hauling and work areas ─────────────

    use crate::work::{HaulOrder, Slot, Workplace};
    use crate::{Command, SimError};

    fn adult(sim: &mut SimState, id: BeaverId) {
        sim.colony.beavers.iter_mut().find(|b| b.id == id).unwrap().stage = Stage::Adult;
    }

    fn fill(sim: &mut SimState, building: BuildingId, slot: usize, who: BeaverId) -> Result<(), SimError> {
        sim.apply(Command::FillSlot { building, slot, who })
    }

    /// Lodge site at (14, 14), a warehouse and a ledger holding exactly the lodge cost, three adults.
    fn lodge_site() -> (SimState, Vec<BeaverId>, BuildingId, Vec<(Good, f32)>) {
        let (mut sim, ids) = setup();
        for &i in &ids[..3] {
            adult(&mut sim, i);
        }
        let cost = BuildingKind::Lodge.spec().cost.to_vec();
        for &(g, a) in &cost {
            assert_eq!(sim.buildings.stores.deposit(g, a).unwrap(), a, "ledger has room for the cost");
        }
        warehouse(&mut sim, 100, (20, 9)); // clear of the ids `place_site` hands out
        let site = sim.place_site(BuildingKind::Lodge, (14, 14)).unwrap();
        for (n, &i) in ids[..3].iter().enumerate() {
            spawn_on(&mut sim, i, (10, 10 + 2 * n)).unwrap();
        }
        (sim, ids, site, cost)
    }

    fn delivered(sim: &SimState, site: BuildingId) -> f32 {
        sim.work.place(site).and_then(|p| p.site.as_ref()).map_or(0.0, |s| s.delivered.iter().map(|a| a.1).sum())
    }

    /// Load amounts on their way to `site`.
    fn in_transit(sim: &SimState, site: BuildingId) -> f32 {
        sim.agents
            .by_id
            .values()
            .filter(|a| matches!(a.task, Task::Carry { dest } if dest == site))
            .filter_map(|a| a.load.map(|l| l.amount))
            .sum()
    }

    #[test]
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
                loads_delivered += 1;
                assert!(carried >= gain - 1e-3, "delivered {gain} but only {carried} was being carried");
                assert!(gain <= LOAD_SIZE + 1e-3);
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
    fn hauler_moves_goods_between_two_stores_and_every_unit_is_carried() {
        let (mut sim, ids) = setup();
        adult(&mut sim, ids[0]);
        let (a, b) = (warehouse(&mut sim, 1, (20, 9)), warehouse(&mut sim, 2, (10, 30)));
        sim.work.places.push(Workplace {
            building: a,
            kind: BuildingKind::Warehouse,
            slots: vec![Slot { role: Role::Haul, worker: None }],
            area: None,
            site: None,
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
            slots: vec![Slot { role: Role::Collect(ResourceKind::Tree), worker: None }],
            area: Some(area),
            site: None,
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
