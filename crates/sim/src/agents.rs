//! Beaver agents: bodies on the navigation grid and the task state machine (owner: `collect`;
//! `build` adds Build/Fetch/haul behaviour inside the same `step`). Person data (age, needs,
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
use crate::work::{Area, Role};
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
}

pub const STUCK_TICKS: u32 = 600;

#[derive(Clone, Debug, Default)]
pub struct Agents {
    pub by_id: BTreeMap<BeaverId, Agent>,
}

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
                matches!(sim.nav.surface_at(c), Some(Surface::Land | Surface::Water))
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
        sim.resources.items.iter().filter(|r| self.permits(r)).min_by_key(|r| (rank(r), d(r.cell), r.id)).map(|r| r.id)
    }
}

// ───────────────────────────── storage and lodges ─────────────────────────────

fn accepts(kind: BuildingKind, good: Good) -> bool {
    match kind {
        BuildingKind::Warehouse => true,
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

fn tick(sim: &mut SimState, ag: &mut Agent, dt: f32) {
    // Kits never work; unassigned adults rest.
    let can_work = sim.colony.beaver(ag.id).is_some_and(|b| b.stage != Stage::Kit);
    let job = ag.assignment.filter(|_| can_work).and_then(|a| Job::resolve(sim, a));
    // A single-resource assignment ends when its resource is used up.
    if job.is_none() && can_work && matches!(ag.assignment, Some(Assignment::Resource(_))) {
        ag.assignment = None;
    }
    if !matches!(ag.task, Task::Collect { .. } | Task::Carry { .. } | Task::Rest) {
        ag.task = Task::Idle;
    }
    match ag.task.clone() {
        Task::Idle => {
            ag.path = None;
            if ag.load.is_some() {
                ag.task = Task::Carry { dest: NO_STORE };
            } else if let Some(t) = job.as_ref().and_then(|j| j.pick(sim, ag.cell)) {
                ag.task = Task::Collect { target: t, progress: 0.0 };
            } else if !ag.inside && has_lodge(sim) {
                ag.task = Task::Rest;
            }
        }
        Task::Rest => {
            if ag.load.is_some() || job.as_ref().is_some_and(|j| j.pick(sim, ag.cell).is_some()) {
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
        Task::Collect { target, .. } => collect(sim, ag, dt, job, target),
        Task::Carry { dest } => carry(sim, ag, dt, dest),
        Task::Fetch { .. } | Task::Build { .. } => unreachable!("normalised above"),
    }
}

fn collect(sim: &mut SimState, ag: &mut Agent, dt: f32, job: Option<Job>, target: ResourceId) {
    let res = sim.resources.get(target).filter(|r| job.as_ref().is_some_and(|j| j.permits(r))).cloned();
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
        }
        let moved = follow(&mut sim.nav, ag, dt);
        note(ag, moved);
        return;
    }
    ag.path = None;
    note(ag, true);
    if res.kind == ResourceKind::Tree {
        // The tree frees its cell when it falls; the nav grid follows at once.
        if sim.resources.fell(target, dt * FELL_PER_DAY).expect("kind checked above") {
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
    let mut want = (dt * GATHER_PER_DAY).min(LOAD_SIZE - have);
    if left - want < EPS {
        want = left; // never leave float dust behind that would become a tiny extra load
    }
    if let Some((g, got)) = sim.resources.take(target, want) {
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

fn carry(sim: &mut SimState, ag: &mut Agent, dt: f32, dest: BuildingId) {
    let Some(load) = ag.load else {
        ag.task = Task::Idle;
        return;
    };
    let dest = if ag.path.is_some() || adjacent_to_store(sim, ag.cell, dest, load.good) {
        dest
    } else {
        // Choose the store with the cheapest route (swimming counts when it is shorter).
        let Some((id, path)) = nearest_store(sim, ag.id, ag.cell, load.good) else {
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
    let ids: Vec<BeaverId> = sim.agents.by_id.keys().copied().collect();
    for id in ids {
        let Some(mut ag) = sim.agents.by_id.remove(&id) else { continue };
        tick(sim, &mut ag, dt_days);
        sim.agents.by_id.insert(id, ag);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::buildings::Status;
    use crate::resources::{FELL_WORK, TREE_BRANCHES, TREE_LOGS};

    const DT: f32 = 0.02;

    /// Founded colony (adults at ids[0..2], yearlings after), empty resources, all-land nav.
    fn setup() -> (SimState, Vec<BeaverId>) {
        let mut sim = SimState::new(3);
        sim.resources = Default::default();
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
        // The kit is assigned too, but kits do no work; adult ids[1] is unassigned.
        sim.apply(crate::Command::Assign { who: vec![ids[2]], to: Some(Assignment::Resource(inside[0])) }).unwrap();
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
}
