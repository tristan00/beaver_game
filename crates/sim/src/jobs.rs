include!(concat!(env!("OUT_DIR"), "/jobs_constants.rs"));
use crate::agents::{can_fill, is_site, Assignment};
use crate::buildings::{BuildingId, Good};
use crate::colony::{BeaverId, Job, Stage};
use crate::resources::ResourceKind;
use crate::water::Cell;
use crate::work::{ManagedDuty, Priority, Role};
use crate::{SimError, SimState};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum RoleKind {
    Forager,
    Logger,
    MudDigger,
    Builder,
    Hauler,
}

impl RoleKind {
    pub const ALL: [RoleKind; 5] = [
        RoleKind::Forager,
        RoleKind::Logger,
        RoleKind::MudDigger,
        RoleKind::Builder,
        RoleKind::Hauler,
    ];
    pub const DEFAULT_ORDER: [RoleKind; 5] = DEFAULT_ROLE_ORDER;
}


pub fn rank(sim: &SimState, role: RoleKind) -> usize {
    let short = crate::hud::food_days_left(sim) < FOOD_EMERGENCY_DAYS;
    if short && role == RoleKind::Forager {
        return 0;
    }
    if material_short(sim, role) {
        return 1;
    }
    sim.work.role_rank(role) + 2
}


/// Keep a winter reserve and the current building queue supplied before harvesting more.
pub fn stock_target(sim: &SimState, good: Good) -> f32 {
    let committed: f32 = sim.work.places.iter().filter(|p| is_site(sim,p))
        .filter_map(|p| p.site.as_ref()).map(|s|s.missing(good)).sum();
    let reserve = match good {
        Good::Food => crate::hud::food_need_per_day(sim) * FOOD_RESERVE_DAYS,
        Good::Sticks => STICKS_RESERVE_BASE + sim.buildings.buildings.len() as f32 * STICKS_RESERVE_PER_BUILDING,
        Good::Clay => CLAY_RESERVE_BASE + sim.buildings.buildings.len() as f32 * CLAY_RESERVE_PER_BUILDING,
    };
    (committed + reserve).min(sim.buildings.stores.capacity(good))
}

/// Stock plus loads already taken out for work, so carrying a load never looks like a shortage.
pub fn on_hand(sim: &SimState, good: Good) -> f32 {
    sim.buildings.stores.amount(good) + crate::agents::in_hands(sim, good)
}

pub fn material_short(sim: &SimState, role: RoleKind) -> bool {
    use crate::buildings::Good;
    let goods: &[Good] = match role {
        RoleKind::Logger => &[Good::Sticks],
        RoleKind::MudDigger => &[Good::Clay],
        _ => return false,
    };
    if role == RoleKind::Logger && on_hand(sim, Good::Sticks) < WOOD_RESERVE {
        return true;
    }
    goods.iter().any(|&g| {
        let lacking: f32 = sim
            .work
            .places
            .iter()
            .filter(|p| is_site(sim,p))
            .filter_map(|p| p.site.as_ref())
            .map(|s| s.missing(g))
            .sum();
        lacking > on_hand(sim, g) + 1e-3
    })
}

pub fn role_kind(r: Role) -> RoleKind {
    match r {
        Role::Collect(k) => gather_role(k),
        Role::Build => RoleKind::Builder,
        Role::Haul => RoleKind::Hauler,
    }
}

pub fn gather_role(k: ResourceKind) -> RoleKind {
    match k {
        ResourceKind::Plant => RoleKind::Forager,
        ResourceKind::Tree | ResourceKind::Felled => RoleKind::Logger,
        ResourceKind::MudBank => RoleKind::MudDigger,
        ResourceKind::RiverClay => RoleKind::MudDigger,
    }
}

pub fn colony_job(r: RoleKind) -> Option<Job> {
    match r {
        RoleKind::Forager => Some(Job::Forager),
        RoleKind::Logger => Some(Job::Logger),
        RoleKind::MudDigger => Some(Job::Digger),
        RoleKind::Builder => Some(Job::Builder),
        RoleKind::Hauler => Some(Job::Hauler),
    }
}

fn assignment_role(sim: &SimState, a: Assignment) -> Option<RoleKind> {
    match a {
        Assignment::Resource(id) => sim.resources.get(id).map(|r| gather_role(r.kind)),
        Assignment::Area(k, _) => Some(gather_role(k)),
        Assignment::Slot(b, i) => sim.work.place(b)?.slots.get(i).map(|s| role_kind(s.role)),
        Assignment::Go(_) => None,
    }
}

pub fn role_of(sim: &SimState, who: BeaverId) -> Option<RoleKind> {
    if let Some((_, _, r)) = sim.work.slot_of(who) {
        return Some(role_kind(r));
    }
    sim.agents.get(who).and_then(|a| a.assignment).and_then(|a| assignment_role(sim, a))
}

pub fn is_busy(sim: &SimState, who: BeaverId) -> bool {
    sim.work.slot_of(who).is_some() || sim.agents.get(who).is_some_and(|a| a.assignment.is_some())
}

#[derive(Clone, Debug, PartialEq)]
pub struct RoleRow {
    pub role: RoleKind,
    pub filled: usize,
    pub slots: usize,
    pub rank: usize,
    pub output_per_day: Vec<(Good, f32)>,
}

fn working_places(sim: &SimState) -> impl Iterator<Item = &crate::work::Workplace> {
    sim.work
        .places
        .iter()
        .filter(move |p| !p.paused && (sim.buildings.building(p.building).is_some_and(|b| b.operational() && !sim.buildings.stranded(b, &sim.water.depth)) || is_site(sim, p)))
}

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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Headcount {
    pub working: usize,
    pub free: usize,
    pub kits: usize,
    pub total: usize,
}

pub fn headcount(sim: &SimState) -> Headcount {
    let mut h = Headcount { working: 0, free: 0, kits: 0, total: sim.colony.population() };
    for b in &sim.colony.beavers {
        match b.stage {
            Stage::Kit => h.kits += 1,
            _ if is_busy(sim, b.id) => h.working += 1,
            _ => h.free += 1,
        }
    }
    h
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum IdleReason {
    NoOpenSlot,
    Unreachable,
    Resting,
    Paused,
    AutoOff,
}

fn open_slots(sim: &SimState, stage: Stage) -> Vec<(BuildingId, usize)> {
    working_places(sim)
        .filter(|p| !p.paused && p.filled() < p.max_workers)
        .flat_map(|p| {
            p.slots.iter().enumerate().filter(|(_, s)| s.worker.is_none() && can_fill(stage, s.role)).map(|(i, _)| (p.building, i))
        })
        .collect()
}

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
            let mut places: Vec<BuildingId> = open.iter().map(|o| o.0).collect();
            places.dedup();
            let reachable = sim.agents.get(id).is_none_or(|a| {
                places.iter().take(4).filter_map(|&bid| sim.buildings.building(bid)).any(|bd| {
                    bd.cells().any(|c| {
                        a.cell.0.abs_diff(c.0) + a.cell.1.abs_diff(c.1) == 1 || sim.nav.may_reach_beside(a.cell, c)
                    })
                })
            });
            Some((id, if !reachable { IdleReason::Unreachable } else if !sim.work.auto_organise { IdleReason::AutoOff } else { IdleReason::NoOpenSlot }))
        })
        .collect()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AddPreview {
    pub who: BeaverId,
    pub from: Option<(BuildingId, RoleKind)>,
}

fn d2(a: Cell, b: Cell) -> usize {
    a.0.abs_diff(b.0).pow(2) + a.1.abs_diff(b.1).pow(2)
}

fn agent_cell(sim: &SimState, who: BeaverId) -> Option<Cell> {
    sim.agents.get(who).map(|a| a.cell)
}

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
            jobs.push((p.priority, rank(sim, rk), d2(c, at), w, p.building, rk));
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
        jobs.push((prio, rank(sim, rk), d2(c, at), w, site, rk));
    }
    jobs.into_iter()
        .min_by_key(|j| (j.0, std::cmp::Reverse(j.1), j.2, j.3))
        .map(|j| (AddPreview { who: j.3, from: Some((j.4, j.5)) }, j.2))
}

fn empty_slot(sim: &SimState, b: BuildingId, role: Option<RoleKind>) -> Option<(usize, Role, Cell)> {
    let p = sim.work.place(b)?;
    let bd = sim.buildings.building(b)?;
    if p.paused || !(bd.is_complete() || is_site(sim, p)) || sim.buildings.stranded(bd, &sim.water.depth) {
        return None;
    }
    let i = p.slots.iter().position(|s| s.worker.is_none() && role.is_none_or(|r| role_kind(s.role) == r))?;
    Some((i, p.slots[i].role, bd.pos))
}

pub fn add_worker_preview(sim: &SimState, b: BuildingId) -> Option<AddPreview> {
    let (_, role, at) = empty_slot(sim, b, None)?;
    pick(sim, role, at, Some(b), None).map(|p| p.0)
}

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

pub fn role_add_preview(sim: &SimState, role: RoleKind) -> Option<AddPreview> {
    role_target(sim, role).map(|t| t.2)
}

fn invalid(m: impl Into<String>) -> SimError {
    SimError::InvalidCommand(m.into())
}

pub(crate) fn place_by_hand(sim: &mut SimState, b: BuildingId, slot: usize, who: BeaverId) -> Result<(), SimError> {
    if sim.buildings.building(b).is_some_and(|bd| sim.buildings.stranded(bd, &sim.water.depth)) {
        return Err(workplace_error(sim, b));
    }
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
        Some(_) if sim.buildings.building(b).is_some_and(|bd| sim.buildings.stranded(bd, &sim.water.depth)) => invalid(format!("building {} is high and dry: no water", b.0)),
        Some(p) if p.filled() == p.slots.len() => invalid(format!("building {} has no free slot", b.0)),
        Some(_) => invalid(format!("building {} takes no workers now", b.0)),
    }
}

pub(crate) fn add_worker(sim: &mut SimState, b: BuildingId) -> Result<(), SimError> {
    let (slot, role, at) = empty_slot(sim, b, None).ok_or_else(|| workplace_error(sim, b))?;
    let (pv, _) = pick(sim, role, at, Some(b), None).ok_or_else(|| invalid("no beaver is free to take the job"))?;
    place_by_hand(sim, b, slot, pv.who)
}

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
        if let Err(e) = step {
            return if n == 0 { Err(e) } else { Ok(()) };
        }
    }
    Ok(())
}

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

pub(crate) fn unlock(sim: &mut SimState, who: BeaverId) -> Result<(), SimError> {
    if sim.colony.beaver(who).is_none() {
        return Err(invalid(format!("no beaver {}", who.0)));
    }
    if sim.work.set_locked(who, false) {
        if let Some((b, i, _)) = sim.work.slot_of(who) {
            sim.work.managed.insert(who, ManagedDuty::Slot(b, i));
        }
    }
    if sim.work.player_orders.remove(&who) {
        sim.agents.assign(who, None);
    }
    Ok(())
}

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
