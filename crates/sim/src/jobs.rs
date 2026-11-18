//! The player's view of the worker model (spec W2–W5): roles, the Jobs board, headcount, why
//! beavers are idle, and who a "+" would move. Also the worker commands behind
//! `Command::AddWorker`, `RemoveWorker`, `AdjustRole` and friends. One job model: a beaver's
//! role is the role of the slot it fills (or of its gather order); see the `work` module docs.

use crate::agents::{can_fill, is_site, Assignment};
use crate::buildings::{BuildingId, Good};
use crate::colony::{BeaverId, Job, Stage};
use crate::resources::ResourceKind;
use crate::water::Cell;
use crate::work::{ManagedDuty, Priority, Role};
use crate::{SimError, SimState};

/// A role on the Jobs board.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum RoleKind {
    Forager,
    Logger,
    MudDigger,
    Quarrier,
    Builder,
    Hauler,
    Crafter,
    Elder,
}

impl RoleKind {
    pub const ALL: [RoleKind; 8] = [
        RoleKind::Forager,
        RoleKind::Logger,
        RoleKind::MudDigger,
        RoleKind::Quarrier,
        RoleKind::Builder,
        RoleKind::Hauler,
        RoleKind::Crafter,
        RoleKind::Elder,
    ];
    /// Default role priority, first = highest.
    pub const DEFAULT_ORDER: [RoleKind; 8] = [
        RoleKind::Builder,
        RoleKind::Forager,
        RoleKind::Logger,
        RoleKind::Hauler,
        RoleKind::MudDigger,
        RoleKind::Quarrier,
        RoleKind::Crafter,
        RoleKind::Elder,
    ];
}

/// The role of a slot.
pub fn role_kind(r: Role) -> RoleKind {
    match r {
        Role::Collect(k) => gather_role(k),
        Role::Build => RoleKind::Builder,
        Role::Haul => RoleKind::Hauler,
        Role::Craft => RoleKind::Crafter,
        Role::Study => RoleKind::Elder,
    }
}

/// The role of gathering `k`.
pub fn gather_role(k: ResourceKind) -> RoleKind {
    match k {
        ResourceKind::Plant => RoleKind::Forager,
        ResourceKind::Tree | ResourceKind::Felled => RoleKind::Logger,
        ResourceKind::MudBank => RoleKind::MudDigger,
        ResourceKind::StoneOutcrop => RoleKind::Quarrier,
    }
}

/// The colony job (skills, experience) a role trains. Elders' study has none.
pub fn colony_job(r: RoleKind) -> Option<Job> {
    match r {
        RoleKind::Forager => Some(Job::Forager),
        RoleKind::Logger => Some(Job::Logger),
        RoleKind::MudDigger | RoleKind::Quarrier => Some(Job::Digger),
        RoleKind::Builder => Some(Job::Builder),
        RoleKind::Hauler => Some(Job::Hauler),
        RoleKind::Crafter => Some(Job::Crafter),
        RoleKind::Elder => None,
    }
}

/// The role of a gather assignment.
fn assignment_role(sim: &SimState, a: Assignment) -> Option<RoleKind> {
    match a {
        Assignment::Resource(id) => sim.resources.get(id).map(|r| gather_role(r.kind)),
        Assignment::Area(k, _) => Some(gather_role(k)),
        Assignment::Slot(b, i) => sim.work.place(b)?.slots.get(i).map(|s| role_kind(s.role)),
        Assignment::Go(_) => None,
    }
}

/// The role `who` works: its slot's, else its gather order's; `None` when free.
pub fn role_of(sim: &SimState, who: BeaverId) -> Option<RoleKind> {
    if let Some((_, _, r)) = sim.work.slot_of(who) {
        return Some(role_kind(r));
    }
    sim.agents.get(who).and_then(|a| a.assignment).and_then(|a| assignment_role(sim, a))
}

/// Whether `who` holds a slot or a gather order.
pub fn is_busy(sim: &SimState, who: BeaverId) -> bool {
    sim.work.slot_of(who).is_some() || sim.agents.get(who).is_some_and(|a| a.assignment.is_some())
}

// ───────────────────────────── jobs board ─────────────────────────────

#[derive(Clone, Debug, PartialEq)]
pub struct RoleRow {
    pub role: RoleKind,
    /// Beavers working this role: slot workers plus gatherers on an order (a site's material run
    /// or a direct order), who also count as one slot each.
    pub filled: usize,
    pub slots: usize,
    /// Position in `Work::role_order` (0 = highest priority).
    pub rank: usize,
    /// Measured output of the workplaces with this role's slots, largest first.
    pub output_per_day: Vec<(Good, f32)>,
}

/// Workplaces whose slots are open to work: unpaused, and complete or a site still being built.
fn working_places(sim: &SimState) -> impl Iterator<Item = &crate::work::Workplace> {
    sim.work
        .places
        .iter()
        .filter(move |p| !p.paused && (sim.buildings.building(p.building).is_some_and(|b| b.is_complete()) || is_site(sim, p)))
}

/// One row per role in `Work::role_order`, only roles with slots or workers.
pub fn jobs_board(sim: &SimState) -> Vec<RoleRow> {
    sim.work
        .role_order
        .iter()
        .map(|&role| {
            let (mut filled, mut slots) = (0, 0);
            let mut out: Vec<(Good, f32)> = Vec::new();
            for p in working_places(sim).filter(|p| !p.paused) {
                let mine = p.slots.iter().filter(|s| role_kind(s.role) == role);
                let n = mine.clone().count();
                if n == 0 {
                    continue;
                }
                slots += n;
                filled += mine.filter(|s| s.worker.is_some()).count();
                for (g, r) in sim.output.of(p.building).map(|f| f.rates()).unwrap_or_default() {
                    match out.iter_mut().find(|e| e.0 == g) {
                        Some(e) => e.1 += r,
                        None => out.push((g, r)),
                    }
                }
            }
            let ordered = sim
                .colony
                .beavers
                .iter()
                .filter(|b| sim.work.slot_of(b.id).is_none())
                .filter(|b| sim.agents.get(b.id).and_then(|a| a.assignment).and_then(|a| assignment_role(sim, a)) == Some(role))
                .count();
            out.sort_by(|a, b| b.1.total_cmp(&a.1));
            RoleRow { role, filled: filled + ordered, slots: slots + ordered, rank: sim.work.role_rank(role), output_per_day: out }
        })
        .filter(|r| r.slots > 0 || r.filled > 0)
        .collect()
}

/// The Jobs board header. `working + free + kits + elders == total`: working and free count
/// adults and yearlings only.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Headcount {
    pub working: usize,
    pub free: usize,
    pub kits: usize,
    pub elders: usize,
    pub total: usize,
}

pub fn headcount(sim: &SimState) -> Headcount {
    let mut h = Headcount { working: 0, free: 0, kits: 0, elders: 0, total: sim.colony.population() };
    for b in &sim.colony.beavers {
        match b.stage {
            Stage::Kit => h.kits += 1,
            Stage::Elder => h.elders += 1,
            _ if is_busy(sim, b.id) => h.working += 1,
            _ => h.free += 1,
        }
    }
    h
}

// ───────────────────────────── idle reasons ─────────────────────────────

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum IdleReason {
    /// No empty slot it may take (or every workplace is at its "Max").
    NoOpenSlot,
    /// Empty slots exist but no route reaches any of them.
    Unreachable,
    /// Tired: back to work once rested.
    Resting,
    /// The only empty slots are in paused workplaces.
    Paused,
    /// Empty slots wait but auto-staffing is off.
    AutoOff,
}

/// Empty slots `stage` may take in unpaused workplaces below their "Max", with the building.
fn open_slots(sim: &SimState, stage: Stage) -> Vec<(BuildingId, usize)> {
    working_places(sim)
        .filter(|p| !p.paused && p.filled() < p.max_workers)
        .flat_map(|p| {
            p.slots.iter().enumerate().filter(|(_, s)| s.worker.is_none() && can_fill(stage, s.role)).map(|(i, _)| (p.building, i))
        })
        .collect()
}

/// Each idle beaver (`hud::idle_beavers`) with why.
pub fn idle_reasons(sim: &SimState) -> Vec<(BeaverId, IdleReason)> {
    crate::hud::idle_beavers(sim)
        .into_iter()
        .filter_map(|id| {
            let b = sim.colony.beaver(id)?;
            if b.resting {
                return Some((id, IdleReason::Resting));
            }
            let open = open_slots(sim, b.stage);
            if open.is_empty() {
                let paused = sim
                    .work
                    .places
                    .iter()
                    .any(|p| p.paused && p.slots.iter().any(|s| s.worker.is_none() && can_fill(b.stage, s.role)));
                return Some((id, if paused { IdleReason::Paused } else { IdleReason::NoOpenSlot }));
            }
            // A route to any of the first few workplaces with room (route searches are not free).
            let mut places: Vec<BuildingId> = open.iter().map(|o| o.0).collect();
            places.dedup();
            let reachable = sim.agents.get(id).is_none_or(|a| {
                places.iter().take(4).filter_map(|&bid| sim.buildings.building(bid)).any(|bd| {
                    bd.cells().any(|c| {
                        a.cell.0.abs_diff(c.0) + a.cell.1.abs_diff(c.1) == 1 || sim.nav.find_path(id, a.cell, crate::nav::Goal::Beside(c)).is_some()
                    })
                })
            });
            Some((id, if !reachable { IdleReason::Unreachable } else if !sim.work.auto_organise { IdleReason::AutoOff } else { IdleReason::NoOpenSlot }))
        })
        .collect()
}

// ───────────────────────────── who a "+" moves ─────────────────────────────

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AddPreview {
    pub who: BeaverId,
    /// Where they would be taken from (`None`: they were free).
    pub from: Option<(BuildingId, RoleKind)>,
}

fn d2(a: Cell, b: Cell) -> usize {
    a.0.abs_diff(b.0).pow(2) + a.1.abs_diff(b.1).pow(2)
}

fn agent_cell(sim: &SimState, who: BeaverId) -> Option<Cell> {
    sim.agents.get(who).map(|a| a.cell)
}

/// The beaver a "+" on a slot with `role` at `at` would place: the nearest free beaver who may
/// take it, else one taken from the lowest-priority unlocked job (lowest building priority, then
/// lowest role priority, then nearest). Never takes from `not_from` or from a job of `same_role`.
fn pick(sim: &SimState, role: Role, at: Cell, not_from: Option<BuildingId>, same_role: Option<RoleKind>) -> Option<(AddPreview, usize)> {
    let stage = |id: BeaverId| sim.colony.beaver(id).map(|b| b.stage);
    let free = sim
        .colony
        .beavers
        .iter()
        .filter(|b| can_fill(b.stage, role) && !sim.work.player_orders.contains(&b.id) && !is_busy(sim, b.id))
        .filter_map(|b| agent_cell(sim, b.id).map(|c| (b.id, d2(c, at))))
        .min_by_key(|&(id, d)| (d, id));
    if let Some((who, d)) = free {
        return Some((AddPreview { who, from: None }, d));
    }
    // Unlocked jobs: slot workers not placed by hand, and gatherers auto-staffing sent.
    let mut jobs: Vec<(Priority, usize, usize, BeaverId, BuildingId, RoleKind)> = Vec::new();
    for p in &sim.work.places {
        if Some(p.building) == not_from {
            continue;
        }
        for s in &p.slots {
            let Some(w) = s.worker.filter(|_| !s.locked) else { continue };
            if sim.work.player_orders.contains(&w) || !stage(w).is_some_and(|st| can_fill(st, role)) {
                continue;
            }
            let rk = role_kind(s.role);
            if Some(rk) == same_role {
                continue;
            }
            let Some(c) = agent_cell(sim, w) else { continue };
            jobs.push((p.priority, sim.work.role_rank(rk), d2(c, at), w, p.building, rk));
        }
    }
    for (&w, duty) in &sim.work.managed {
        let ManagedDuty::Gather(a) = *duty else { continue };
        let (Some(rk), Some(c)) = (assignment_role(sim, a), agent_cell(sim, w)) else { continue };
        if Some(rk) == same_role || sim.work.player_orders.contains(&w) || !stage(w).is_some_and(|st| can_fill(st, role)) {
            continue;
        }
        let site = match a {
            Assignment::Area(_, area) => sim
                .work
                .places
                .iter()
                .filter(|p| is_site(sim, p))
                .filter_map(|p| sim.buildings.building(p.building))
                .min_by_key(|b| (d2(b.pos, area.centre()), b.id))
                .map(|b| b.id),
            _ => None,
        };
        let Some(site) = site.filter(|s| Some(*s) != not_from) else { continue };
        let prio = sim.work.place(site).map_or(Priority::Normal, |p| p.priority);
        jobs.push((prio, sim.work.role_rank(rk), d2(c, at), w, site, rk));
    }
    // Lowest building priority, then lowest role priority (largest rank), then nearest.
    jobs.into_iter()
        .min_by_key(|j| (j.0, std::cmp::Reverse(j.1), j.2, j.3))
        .map(|j| (AddPreview { who: j.3, from: Some((j.4, j.5)) }, j.2))
}

/// The first empty slot of workplace `b` (with `role`, if given) and its role; `None` if the
/// workplace is missing, paused, finished as a site or full.
fn empty_slot(sim: &SimState, b: BuildingId, role: Option<RoleKind>) -> Option<(usize, Role, Cell)> {
    let p = sim.work.place(b)?;
    let bd = sim.buildings.building(b)?;
    if p.paused || !(bd.is_complete() || is_site(sim, p)) {
        return None;
    }
    let i = p.slots.iter().position(|s| s.worker.is_none() && role.is_none_or(|r| role_kind(s.role) == r))?;
    Some((i, p.slots[i].role, bd.pos))
}

/// Who a "+" on workplace `b` would place, and from where. `None`: no slot free or nobody available.
pub fn add_worker_preview(sim: &SimState, b: BuildingId) -> Option<AddPreview> {
    let (_, role, at) = empty_slot(sim, b, None)?;
    pick(sim, role, at, Some(b), None).map(|p| p.0)
}

/// The workplace a Jobs-board "+" on `role` fills: highest priority, then the nearest beaver.
fn role_target(sim: &SimState, role: RoleKind) -> Option<(BuildingId, usize, AddPreview)> {
    sim.work
        .places
        .iter()
        .filter_map(|p| {
            let (i, r, at) = empty_slot(sim, p.building, Some(role))?;
            let (pv, d) = pick(sim, r, at, Some(p.building), Some(role))?;
            Some((std::cmp::Reverse(p.priority), d, p.building, i, pv))
        })
        .min_by_key(|t| (t.0, t.1, t.2))
        .map(|t| (t.2, t.3, t.4))
}

/// Who a Jobs-board "+" on `role` would place, and from where.
pub fn role_add_preview(sim: &SimState, role: RoleKind) -> Option<AddPreview> {
    role_target(sim, role).map(|t| t.2)
}

// ───────────────────────────── commands ─────────────────────────────

fn invalid(m: impl Into<String>) -> SimError {
    SimError::InvalidCommand(m.into())
}

/// Puts `who` into slot `slot` of `b` by hand: they leave any slot or order, the slot is locked,
/// and the workplace's "Max" rises to hold them.
pub(crate) fn place_by_hand(sim: &mut SimState, b: BuildingId, slot: usize, who: BeaverId) -> Result<(), SimError> {
    sim.work.assign(b, slot, who).map_err(|e| invalid(format!("{e:?}")))?;
    sim.work.managed.remove(&who);
    sim.work.player_orders.remove(&who);
    sim.agents.assign(who, None);
    sim.work.set_locked(who, true);
    let p = sim.work.place_mut(b).map_err(|e| invalid(format!("{e:?}")))?;
    p.max_workers = p.max_workers.max(p.filled());
    Ok(())
}

fn workplace_error(sim: &SimState, b: BuildingId) -> SimError {
    match sim.work.place(b) {
        None => invalid(format!("building {} has no worker slots", b.0)),
        Some(p) if p.paused => invalid(format!("building {} is paused", b.0)),
        Some(p) if p.filled() == p.slots.len() => invalid(format!("building {} has no free slot", b.0)),
        Some(_) => invalid(format!("building {} takes no workers now", b.0)),
    }
}

pub(crate) fn add_worker(sim: &mut SimState, b: BuildingId) -> Result<(), SimError> {
    let (slot, role, at) = empty_slot(sim, b, None).ok_or_else(|| workplace_error(sim, b))?;
    let (pv, _) = pick(sim, role, at, Some(b), None).ok_or_else(|| invalid("no beaver is free to take the job"))?;
    place_by_hand(sim, b, slot, pv.who)
}

/// Frees the highest filled slot of `b`, and lowers its "Max" so the beaver is not sent back.
fn free_last(sim: &mut SimState, b: BuildingId, role: Option<RoleKind>) -> Result<BeaverId, SimError> {
    let p = sim.work.place(b).ok_or_else(|| invalid(format!("building {} has no worker slots", b.0)))?;
    let i = p
        .slots
        .iter()
        .rposition(|s| s.worker.is_some() && role.is_none_or(|r| role_kind(s.role) == r))
        .ok_or_else(|| invalid(format!("building {} has no worker to remove", b.0)))?;
    let who = p.slots[i].worker.expect("filled");
    sim.work.unassign(b, i).map_err(|e| invalid(format!("{e:?}")))?;
    sim.work.managed.remove(&who);
    let p = sim.work.place_mut(b).map_err(|e| invalid(format!("{e:?}")))?;
    p.max_workers = p.filled();
    Ok(who)
}

pub(crate) fn remove_worker(sim: &mut SimState, b: BuildingId) -> Result<(), SimError> {
    free_last(sim, b, None).map(|_| ())
}

pub(crate) fn set_priority(sim: &mut SimState, b: BuildingId, priority: Priority) -> Result<(), SimError> {
    sim.work.place_mut(b).map_err(|_| invalid(format!("building {} has no worker slots", b.0)))?.priority = priority;
    Ok(())
}

/// Pausing frees every worker (locked ones too); auto-staffing leaves the workplace empty.
pub(crate) fn set_paused(sim: &mut SimState, b: BuildingId, paused: bool) -> Result<(), SimError> {
    let p = sim.work.place_mut(b).map_err(|_| invalid(format!("building {} has no worker slots", b.0)))?;
    p.paused = paused;
    if paused {
        let gone: Vec<BeaverId> = p.slots.iter().filter_map(|s| s.worker).collect();
        for s in &mut p.slots {
            s.worker = None;
            s.locked = false;
        }
        sim.work.managed.retain(|id, _| !gone.contains(id));
    }
    Ok(())
}

pub(crate) fn adjust_role(sim: &mut SimState, role: RoleKind, delta: i32) -> Result<(), SimError> {
    for n in 0..delta.unsigned_abs() {
        let step = if delta > 0 {
            match role_target(sim, role) {
                Some((b, slot, pv)) => place_by_hand(sim, b, slot, pv.who),
                None => Err(invalid(format!("no open {role:?} slot with a beaver to take it"))),
            }
        } else {
            // The lowest-priority workplace with this role filled, newest first.
            let from = sim
                .work
                .places
                .iter()
                .filter(|p| p.slots.iter().any(|s| s.worker.is_some() && role_kind(s.role) == role))
                .min_by_key(|p| (p.priority, std::cmp::Reverse(p.building)))
                .map(|p| p.building);
            match from {
                Some(b) => free_last(sim, b, Some(role)).map(|_| ()),
                None => Err(invalid(format!("nobody works as {role:?}"))),
            }
        };
        // Later steps may run out; only the first must succeed.
        if let Err(e) = step {
            return if n == 0 { Err(e) } else { Ok(()) };
        }
    }
    Ok(())
}

/// Sets the role priority; roles left out keep their old relative order after the given ones.
pub(crate) fn set_role_order(sim: &mut SimState, order: Vec<RoleKind>) -> Result<(), SimError> {
    let mut seen = Vec::new();
    for r in &order {
        if seen.contains(r) {
            return Err(invalid(format!("{r:?} appears twice in the role order")));
        }
        seen.push(*r);
    }
    let rest: Vec<RoleKind> = sim.work.role_order.iter().copied().filter(|r| !seen.contains(r)).collect();
    seen.extend(rest);
    sim.work.role_order = seen;
    Ok(())
}

/// Returns `who` to auto-staffing: unlocks their slot and ends any direct order.
pub(crate) fn unlock(sim: &mut SimState, who: BeaverId) -> Result<(), SimError> {
    if sim.colony.beaver(who).is_none() {
        return Err(invalid(format!("no beaver {}", who.0)));
    }
    if sim.work.set_locked(who, false) {
        // Auto-staffing now owns the slot: it keeps the beaver there while the slot is wanted.
        if let Some((b, i, _)) = sim.work.slot_of(who) {
            sim.work.managed.insert(who, ManagedDuty::Slot(b, i));
        }
    }
    if sim.work.player_orders.remove(&who) {
        sim.agents.assign(who, None);
    }
    Ok(())
}

/// A direct gather order (`Command::Assign`): `Some` takes the beaver out of its slot and locks
/// it to the order; `None` stands it down and returns it to auto-staffing.
pub(crate) fn order(sim: &mut SimState, who: BeaverId, to: Option<Assignment>) {
    sim.work.managed.remove(&who);
    match to {
        Some(_) => {
            sim.work.vacate(who);
            sim.work.player_orders.insert(who);
        }
        None => {
            sim.work.player_orders.remove(&who);
        }
    }
}

/// Each beaver's job this step from its slot or order, and whether it is at work (for the colony).
pub(crate) fn slot_jobs(sim: &SimState) -> std::collections::HashMap<BeaverId, (Job, bool)> {
    sim.colony
        .beavers
        .iter()
        .filter_map(|b| {
            let job = colony_job(role_of(sim, b.id)?)?;
            let working = sim.agents.get(b.id).is_some_and(|a| !matches!(a.task, crate::agents::Task::Idle | crate::agents::Task::Rest));
            Some((b.id, (job, working)))
        })
        .collect()
}

/// A founded colony of four adults on the map, no resources, and hand-made workplaces.
#[cfg(test)]
pub(crate) mod fixtures {
    use super::*;
    use crate::buildings::{Building, BuildingKind, Status};
    use crate::work::{Slot, Workplace};

    pub fn colony() -> (SimState, Vec<BeaverId>) {
        let mut sim = SimState::new(3);
        sim.resources = Default::default();
        let land = crate::WorldLand { world: &sim.world, depth: &sim.water.depth };
        let spot = (0..sim.world.size)
            .map(|i| (i as f32, 100.0))
            .find(|&(x, y)| !crate::colony::Landscape::is_water(&land, x, y))
            .expect("a dry cell on row 100");
        sim.colony.found(spot, &land).unwrap();
        for b in sim.colony.beavers.iter_mut() {
            b.stage = Stage::Adult;
        }
        crate::agents::place_beavers(&mut sim);
        let ids = sim.colony.beavers.iter().map(|b| b.id).collect();
        (sim, ids)
    }

    /// The first beaver's cell offset by `(dx, dy)`.
    pub fn near(sim: &SimState, dx: usize, dy: usize) -> Cell {
        let c = sim.agents.by_id.values().next().unwrap().cell;
        (c.0 + dx, c.1 + dy)
    }

    /// A finished workplace of `kind` with its catalogue slots, `at` cells from the first beaver.
    pub fn workplace(sim: &mut SimState, id: u64, kind: BuildingKind, at: (usize, usize), priority: Priority) -> BuildingId {
        let pos = near(sim, at.0, at.1);
        let b = Building { id: BuildingId(id), kind, pos, status: Status::Complete, recipe: None, output: 0.0 };
        let slots = crate::catalogue::def(kind).slots.iter().map(|&r| Slot::empty(r)).collect();
        let mut p = Workplace::new(b.id, kind, slots, crate::catalogue::work_area(&b), None);
        p.priority = priority;
        sim.buildings.buildings.push(b);
        sim.work.places.push(p);
        BuildingId(id)
    }
}

#[cfg(test)]
mod tests {
    use super::fixtures::*;
    use super::*;
    use crate::buildings::BuildingKind;
    use crate::Command;

    fn filled(sim: &SimState, b: BuildingId) -> usize {
        sim.work.place(b).unwrap().filled()
    }

    #[test]
    fn roles_map_from_slots_and_the_default_order_lists_every_role_once() {
        assert_eq!(role_kind(Role::Collect(ResourceKind::Felled)), RoleKind::Logger);
        assert_eq!(role_kind(Role::Collect(ResourceKind::StoneOutcrop)), RoleKind::Quarrier);
        assert_eq!(role_kind(Role::Study), RoleKind::Elder);
        let mut order = RoleKind::DEFAULT_ORDER.to_vec();
        order.sort();
        assert_eq!(order, RoleKind::ALL.to_vec());
        assert_eq!(colony_job(RoleKind::MudDigger), Some(Job::Digger));
    }

    #[test]
    fn add_worker_takes_the_nearest_free_beaver_then_the_lowest_priority_unlocked_job() {
        let (mut s, ids) = colony();
        let hut = workplace(&mut s, 50, BuildingKind::ForagerHut, (3, 3), Priority::High);
        let low = workplace(&mut s, 51, BuildingKind::LoggingCamp, (20, 0), Priority::Low);
        let mid = workplace(&mut s, 52, BuildingKind::MudPit, (0, 20), Priority::Normal);
        s.work.assign(low, 0, ids[0]).unwrap();
        s.work.assign(low, 1, ids[1]).unwrap();
        s.work.assign(mid, 0, ids[2]).unwrap();
        assert_eq!(add_worker_preview(&s, hut), Some(AddPreview { who: ids[3], from: None }), "the free beaver first");
        s.apply(Command::AddWorker { building: hut }).unwrap();
        assert_eq!(s.work.slot_of(ids[3]).map(|t| (t.0, t.1)), Some((hut, 0)));
        assert!(s.work.is_locked(ids[3]), "a beaver placed by hand is locked");
        let pv = add_worker_preview(&s, hut).unwrap();
        assert_eq!(pv.from, Some((low, RoleKind::Logger)), "nobody free: taken from the lowest-priority job");
        s.apply(Command::AddWorker { building: hut }).unwrap();
        assert_eq!(s.work.slot_of(pv.who).map(|t| t.0), Some(hut));
        assert_eq!((filled(&s, hut), filled(&s, low), filled(&s, mid)), (2, 1, 1));
        assert_eq!(add_worker_preview(&s, hut), None, "the hut is full");
        assert!(s.apply(Command::AddWorker { building: hut }).is_err());
        // The mud pit takes the last logger, never a locked forager.
        assert_eq!(add_worker_preview(&s, mid).unwrap().from, Some((low, RoleKind::Logger)));
        s.apply(Command::RemoveWorker { building: hut }).unwrap();
        assert_eq!(s.work.slot_of(pv.who), None, "the last-filled slot is freed");
        assert!(!s.work.is_locked(pv.who));
        assert_eq!(s.work.place(hut).unwrap().max_workers, 1, "and the hut's max drops so it is not refilled");
        assert!(s.apply(Command::AddWorker { building: BuildingId(999) }).is_err());
        assert!(s.apply(Command::RemoveWorker { building: low }).is_ok());
        assert!(s.apply(Command::RemoveWorker { building: low }).is_err(), "nobody left to remove");
        // Unlocking a hand-placed beaver hands its slot to auto-staffing.
        s.apply(Command::Unlock { who: ids[3] }).unwrap();
        assert!(!s.work.is_locked(ids[3]));
        assert_eq!(s.work.managed.get(&ids[3]), Some(&ManagedDuty::Slot(hut, 0)));
    }

    #[test]
    fn auto_staffing_fills_by_priority_keeps_locked_beavers_and_skips_paused_workplaces() {
        let (mut s, ids) = colony();
        s.work.auto_organise = true;
        let camp = workplace(&mut s, 50, BuildingKind::LoggingCamp, (2, 2), Priority::Normal);
        let hut = workplace(&mut s, 51, BuildingKind::ForagerHut, (6, 6), Priority::High);
        s.apply(Command::FillSlot { building: camp, slot: 1, who: ids[0] }).unwrap();
        // A beaver on a direct order is not auto-staffed either.
        let area = crate::work::Area { min: (0, 0), max: (1, 1) };
        s.apply(Command::Assign { who: vec![ids[1]], to: Some(Assignment::Area(ResourceKind::Plant, area)) }).unwrap();
        crate::agents::step(&mut s, 0.0);
        assert_eq!(filled(&s, hut), 2, "the far high-priority hut is staffed before the near camp");
        assert_eq!(filled(&s, camp), 1);
        assert_eq!(s.work.slot_of(ids[0]).map(|t| (t.0, t.1)), Some((camp, 1)), "the hand-placed beaver stays");
        assert_eq!(s.work.slot_of(ids[1]), None, "the ordered beaver stays on its order");
        // Raising the camp above the hut moves an auto-staffed forager over.
        s.apply(Command::SetPriority { building: camp, priority: Priority::High }).unwrap();
        s.apply(Command::SetPriority { building: hut, priority: Priority::Low }).unwrap();
        crate::agents::step(&mut s, 0.0);
        assert_eq!((filled(&s, camp), filled(&s, hut)), (2, 1));
        // Pausing the hut frees its worker and leaves it empty.
        s.apply(Command::SetPaused { building: hut, paused: true }).unwrap();
        crate::agents::step(&mut s, 0.0);
        assert_eq!(filled(&s, hut), 0);
        let free = crate::hud::idle_beavers(&s);
        assert_eq!(free.len(), 1);
        assert_eq!(idle_reasons(&s), vec![(free[0], IdleReason::Paused)]);
        let h = headcount(&s);
        assert_eq!((h.working, h.free, h.working + h.free + h.kits + h.elders), (3, 1, h.total));
        // With auto-staffing off nothing moves.
        s.apply(Command::SetPaused { building: hut, paused: false }).unwrap();
        s.apply(Command::SetAutoStaff { on: false }).unwrap();
        crate::agents::step(&mut s, 0.0);
        assert_eq!(filled(&s, hut), 0);
        assert_eq!(idle_reasons(&s), vec![(free[0], IdleReason::AutoOff)]);
    }

    #[test]
    fn the_jobs_board_moves_beavers_between_a_role_and_the_free_pool() {
        let (mut s, _) = colony();
        let hut = workplace(&mut s, 50, BuildingKind::ForagerHut, (3, 3), Priority::Normal);
        s.apply(Command::AdjustRole { role: RoleKind::Forager, delta: 2 }).unwrap();
        assert_eq!(filled(&s, hut), 2);
        let board = jobs_board(&s);
        assert_eq!(board.len(), 1);
        assert_eq!((board[0].role, board[0].filled, board[0].slots), (RoleKind::Forager, 2, 2));
        assert_eq!(role_add_preview(&s, RoleKind::Forager), None, "no open forager slot");
        s.apply(Command::AdjustRole { role: RoleKind::Forager, delta: -1 }).unwrap();
        assert_eq!((filled(&s, hut), s.work.place(hut).unwrap().max_workers), (1, 1));
        assert!(s.apply(Command::AdjustRole { role: RoleKind::Quarrier, delta: 1 }).is_err(), "no quarry");
        assert!(s.apply(Command::SetRoleOrder { order: vec![RoleKind::Elder, RoleKind::Elder] }).is_err());
        s.apply(Command::SetRoleOrder { order: vec![RoleKind::Elder] }).unwrap();
        assert_eq!((s.work.role_order[0], s.work.role_order[1], s.work.role_order.len()), (RoleKind::Elder, RoleKind::Builder, 8));
        // A direct order takes the forager out of its slot and locks it; Unlock returns it.
        let w = s.work.place(hut).unwrap().slots.iter().find_map(|s| s.worker).unwrap();
        let area = crate::work::Area { min: (0, 0), max: (3, 3) };
        s.apply(Command::Assign { who: vec![w], to: Some(Assignment::Area(ResourceKind::Tree, area)) }).unwrap();
        assert_eq!((s.work.slot_of(w), s.work.is_locked(w), role_of(&s, w)), (None, true, Some(RoleKind::Logger)));
        s.apply(Command::Unlock { who: w }).unwrap();
        assert!(!s.work.is_locked(w) && !is_busy(&s, w));
        assert!(s.apply(Command::Unlock { who: BeaverId(999) }).is_err());
    }
}
