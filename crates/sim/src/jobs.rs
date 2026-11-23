include!(concat!(env!("OUT_DIR"), "/jobs_constants.rs"));
use crate::agents::{is_site, Assignment};
use crate::buildings::{BuildingId, Good};
use crate::colony::{BeaverId, Job};
use crate::resources::ResourceKind;
use crate::water::Cell;
use crate::work::{Priority, Role};
use crate::{SimError, SimState};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
pub enum RoleKind {
    Forager,
    Logger,
    MudDigger,
    Builder,
    Hauler,
    /// Tends and plants the woods round a food store.
    Forester,
    /// Repairs and refits buildings and keeps dams and canals in order.
    Maintainer,
}

impl RoleKind {
    pub const ALL: [RoleKind; 7] = [
        RoleKind::Forager,
        RoleKind::Logger,
        RoleKind::MudDigger,
        RoleKind::Builder,
        RoleKind::Hauler,
        RoleKind::Forester,
        RoleKind::Maintainer,
    ];
    pub const DEFAULT_ORDER: [RoleKind; 7] = DEFAULT_ROLE_ORDER;
}


/// Keep a winter reserve and the current building queue supplied before harvesting more.
pub fn stock_target(sim: &SimState, good: Good) -> f32 {
    let committed: f32 = sim.work.places.iter().filter(|p| is_site(sim,p))
        .filter_map(|p| p.site.as_ref()).map(|s|s.missing(good)).sum();
    let reserve = match good {
        Good::Food => crate::hud::food_need_per_day(sim) * FOOD_RESERVE_DAYS,
        Good::Sticks => STICKS_RESERVE_BASE + sim.buildings.buildings.len() as f32 * STICKS_RESERVE_PER_BUILDING,
        Good::Clay => CLAY_RESERVE_BASE + sim.buildings.buildings.len() as f32 * CLAY_RESERVE_PER_BUILDING,
        Good::Pebbles => PEBBLE_RESERVE,
        Good::Saplings => crate::agents::SAPLING_STOCK,
    };
    (committed + reserve).min(sim.buildings.stores.capacity(good))
}

/// Stock plus loads already taken out for work, so carrying a load never looks like a shortage.
pub fn on_hand(sim: &SimState, good: Good) -> f32 {
    sim.buildings.stores.amount(good) + crate::agents::in_hands(sim, good)
}

pub fn role_kind(r: Role) -> RoleKind {
    match r {
        Role::Collect(k) => gather_role(k),
        Role::Build => RoleKind::Builder,
        Role::Haul => RoleKind::Hauler,
        Role::Tend | Role::Plant => RoleKind::Forester,
        Role::Maintain => RoleKind::Maintainer,
    }
}

pub fn gather_role(k: ResourceKind) -> RoleKind {
    match k {
        ResourceKind::Plant | ResourceKind::WaterPlant => RoleKind::Forager,
        ResourceKind::Tree | ResourceKind::Felled => RoleKind::Logger,
        ResourceKind::MudBank => RoleKind::MudDigger,
        ResourceKind::RiverClay | ResourceKind::Pebbles => RoleKind::MudDigger,
    }
}

pub fn colony_job(r: RoleKind) -> Option<Job> {
    match r {
        RoleKind::Forager => Some(Job::Forager),
        RoleKind::Logger => Some(Job::Logger),
        RoleKind::MudDigger => Some(Job::Digger),
        RoleKind::Builder => Some(Job::Builder),
        RoleKind::Hauler => Some(Job::Hauler),
        RoleKind::Forester => Some(Job::Forager),
        RoleKind::Maintainer => Some(Job::Builder),
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

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct RoleRow {
    pub role: RoleKind,
    pub filled: usize,
    pub slots: usize,
    pub output_per_day: Vec<(Good, f32)>,
    /// The beavers filling it: slot workers first, then those on orders.
    pub workers: Vec<BeaverId>,
}

fn working_places(sim: &SimState) -> impl Iterator<Item = &crate::work::Workplace> {
    sim.work
        .places
        .iter()
        .filter(move |p| !p.paused && (sim.buildings.building(p.building).is_some_and(|b| b.operational() && !sim.buildings.unusable(b, &sim.water.depth)) || is_site(sim, p)))
}

pub fn jobs_board(sim: &SimState) -> Vec<RoleRow> {
    RoleKind::DEFAULT_ORDER
        .into_iter()
        .map(|role| {
            let (mut filled, mut slots) = (0, 0);
            let mut out: Vec<(Good, f32)> = Vec::new();
            let mut workers: Vec<BeaverId> = Vec::new();
            for p in working_places(sim).filter(|p| !p.paused) {
                let mine = p.slots.iter().filter(|s| role_kind(s.role) == role && !s.off);
                let n = mine.clone().count();
                if n == 0 {
                    continue;
                }
                slots += n;
                filled += mine.clone().filter(|s| s.worker.is_some()).count();
                workers.extend(mine.filter_map(|s| s.worker));
                for (g, r) in sim.output.of(p.building).map(|f| f.rates()).unwrap_or_default() {
                    match out.iter_mut().find(|e| e.0 == g) {
                        Some(e) => e.1 += r,
                        None => out.push((g, r)),
                    }
                }
            }
            let on_orders: Vec<BeaverId> = sim
                .colony
                .beavers
                .iter()
                .filter(|b| sim.work.slot_of(b.id).is_none())
                .filter(|b| sim.agents.get(b.id).and_then(|a| a.assignment).and_then(|a| assignment_role(sim, a)) == Some(role))
                .map(|b| b.id)
                .collect();
            let ordered = on_orders.len();
            workers.extend(on_orders);
            out.sort_by(|a, b| b.1.total_cmp(&a.1));
            RoleRow { role, filled: filled + ordered, slots: slots + ordered, output_per_day: out, workers }
        })
        // Gathering needs no building: its rows are always there, so the first beaver can be sent.
        .filter(|r| r.slots > 0 || r.filled > 0 || gather_kind(r.role).is_some())
        .collect()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Headcount {
    pub working: usize,
    pub free: usize,
    pub kits: usize,
    pub total: usize,
}

pub fn headcount(sim: &SimState) -> Headcount {
    let mut h = Headcount { working: 0, free: 0, kits: 0, total: sim.colony.population() };
    for b in &sim.colony.beavers {
        // Kits here are the ones still too young to work; older kits count as workers.
        if !b.can_work() {
            h.kits += 1;
        } else if is_busy(sim, b.id) {
            h.working += 1;
        } else {
            h.free += 1;
        }
    }
    h
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum IdleReason {
    NoOpenSlot,
    Unreachable,
    Resting,
    Paused,
    /// Open work it can reach: waiting to be given a job.
    Free,
}

fn open_slots(sim: &SimState, b: &crate::colony::Beaver) -> Vec<(BuildingId, usize)> {
    working_places(sim)
        .filter(|p| !p.paused && p.filled() < p.max_workers)
        .flat_map(|p| {
            p.slots.iter().enumerate().filter(|(_, s)| s.worker.is_none() && !s.off && b.can_work()).map(|(i, _)| (p.building, i))
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
            let open = open_slots(sim, b);
            if open.is_empty() {
                let paused = sim
                    .work
                    .places
                    .iter()
                    .any(|p| p.paused && b.can_work() && p.slots.iter().any(|s| s.worker.is_none()));
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
            Some((id, if reachable { IdleReason::Free } else { IdleReason::Unreachable }))
        })
        .collect()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct AddPreview {
    pub who: BeaverId,
}

fn d2(a: Cell, b: Cell) -> usize {
    a.0.abs_diff(b.0).pow(2) + a.1.abs_diff(b.1).pow(2)
}

fn agent_cell(sim: &SimState, who: BeaverId) -> Option<Cell> {
    sim.agents.get(who).map(|a| a.cell)
}

fn pick(sim: &SimState, at: Cell) -> Option<(AddPreview, usize)> {
    sim.colony
        .beavers
        .iter()
        .filter(|b| b.can_work() && !is_busy(sim, b.id))
        .filter_map(|b| agent_cell(sim, b.id).map(|c| (b.id, d2(c, at))))
        .min_by_key(|&(id, d)| (d, id))
        .map(|(who, d)| (AddPreview { who }, d))
}

fn empty_slot(sim: &SimState, b: BuildingId, role: Option<RoleKind>) -> Option<(usize, Role, Cell)> {
    let p = sim.work.place(b)?;
    let bd = sim.buildings.building(b)?;
    if p.paused || !(bd.is_complete() || is_site(sim, p)) || sim.buildings.unusable(bd, &sim.water.depth) {
        return None;
    }
    let i = p.slots.iter().position(|s| s.worker.is_none() && !s.off && role.is_none_or(|r| role_kind(s.role) == r))?;
    Some((i, p.slots[i].role, bd.pos))
}

pub fn add_worker_preview(sim: &SimState, b: BuildingId) -> Option<AddPreview> {
    let (_, _, at) = empty_slot(sim, b, None)?;
    pick(sim, at).map(|p| p.0)
}

fn role_target(sim: &SimState, role: RoleKind) -> Option<(BuildingId, usize, AddPreview)> {
    sim.work
        .places
        .iter()
        .filter_map(|p| {
            let (i, _, at) = empty_slot(sim, p.building, Some(role))?;
            let (pv, d) = pick(sim, at)?;
            Some((std::cmp::Reverse(p.priority), d, p.building, i, pv))
        })
        .min_by_key(|t| (t.0, t.1, t.2))
        .map(|t| (t.2, t.3, t.4))
}

pub fn role_add_preview(sim: &SimState, role: RoleKind) -> Option<AddPreview> {
    role_target(sim, role).map(|t| t.2).or_else(|| send_out_who(sim, role).map(|(who, _)| AddPreview { who }))
}

fn invalid(m: impl Into<String>) -> SimError {
    SimError::InvalidCommand(m.into())
}

pub(crate) fn place_by_hand(sim: &mut SimState, b: BuildingId, slot: usize, who: BeaverId) -> Result<(), SimError> {
    if sim.buildings.building(b).is_some_and(|bd| sim.buildings.unusable(bd, &sim.water.depth)) {
        return Err(workplace_error(sim, b));
    }
    sim.work.assign(b, slot, who).map_err(|e| invalid(format!("{e:?}")))?;
    sim.agents.assign(who, None);
    let p = sim.work.place_mut(b).map_err(|e| invalid(format!("{e:?}")))?;
    p.max_workers = p.max_workers.max(p.filled());
    Ok(())
}

fn workplace_error(sim: &SimState, b: BuildingId) -> SimError {
    match sim.work.place(b) {
        None => invalid(format!("building {} has no worker slots", b.0)),
        Some(p) if p.paused => invalid(format!("building {} is paused", b.0)),
        Some(_) if sim.buildings.building(b).is_some_and(|bd| crate::buildings::flooded(bd, &sim.water.depth)) => invalid(format!("building {} is flooded: too deep", b.0)),
        Some(_) if sim.buildings.building(b).is_some_and(|bd| sim.buildings.unusable(bd, &sim.water.depth)) => invalid(format!("building {} is high and dry: no water", b.0)),
        Some(p) if p.filled() == p.slots.len() => invalid(format!("building {} has no free slot", b.0)),
        Some(_) => invalid(format!("building {} takes no workers now", b.0)),
    }
}

pub(crate) fn add_worker(sim: &mut SimState, b: BuildingId) -> Result<(), SimError> {
    let (slot, _, at) = empty_slot(sim, b, None).ok_or_else(|| workplace_error(sim, b))?;
    let (pv, _) = pick(sim, at).ok_or_else(|| invalid("no beaver is free to take the job"))?;
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

/// Switch one worker slot on or off; switching off sends its worker away.
pub(crate) fn set_slot_off(sim: &mut SimState, b: BuildingId, slot: usize, off: bool) -> Result<(), SimError> {
    let p = sim.work.place_mut(b).map_err(|_| invalid(format!("building {} has no worker slots", b.0)))?;
    let s = p.slots.get_mut(slot).ok_or_else(|| invalid("SlotOutOfRange"))?;
    s.off = off;
    if off {
        s.worker = None;
    }
    Ok(())
}

pub(crate) fn set_paused(sim: &mut SimState, b: BuildingId, paused: bool) -> Result<(), SimError> {
    let p = sim.work.place_mut(b).map_err(|_| invalid(format!("building {} has no worker slots", b.0)))?;
    p.paused = paused;
    if paused {
        for s in &mut p.slots {
            s.worker = None;
        }
    }
    Ok(())
}

pub(crate) fn adjust_role(sim: &mut SimState, role: RoleKind, delta: i32) -> Result<(), SimError> {
    for n in 0..delta.unsigned_abs() {
        let step = if delta > 0 {
            match role_target(sim, role) {
                Some((b, slot, pv)) => place_by_hand(sim, b, slot, pv.who),
                None => send_out(sim, role),
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
                None => call_in(sim, role),
            }
        };
        if let Err(e) = step {
            return if n == 0 { Err(e) } else { Ok(()) };
        }
    }
    Ok(())
}

/// The kind of ground a gathering role is sent out over.
fn gather_kind(role: RoleKind) -> Option<ResourceKind> {
    match role {
        RoleKind::Forager => Some(ResourceKind::Plant),
        RoleKind::Logger => Some(ResourceKind::Tree),
        RoleKind::MudDigger => Some(ResourceKind::MudBank),
        _ => None,
    }
}

/// Beavers out gathering for `role` from home (no building to work at), most recent first.
pub(crate) fn out_gathering(sim: &SimState, role: RoleKind) -> Vec<BeaverId> {
    sim.colony.beavers.iter().rev()
        .filter(|b| sim.work.slot_of(b.id).is_none())
        .filter(|b| sim.agents.get(b.id).and_then(|a| a.assignment).and_then(|a| assignment_role(sim, a)) == Some(role))
        .map(|b| b.id)
        .collect()
}

/// One fewer on a gathering role: the latest beaver sent out for it comes back and is free.
fn call_in(sim: &mut SimState, role: RoleKind) -> Result<(), SimError> {
    let Some(&who) = out_gathering(sim, role).first() else {
        return Err(invalid(format!("nobody works as {role:?}")));
    };
    sim.agents.assign(who, None);
    Ok(())
}

/// Who would be sent out for one more on a gathering role, and where: the nearest free beaver.
fn send_out_who(sim: &SimState, role: RoleKind) -> Option<(BeaverId, Assignment)> {
    let kind = gather_kind(role)?;
    let (home, area) = crate::agents::home_area(sim)?;
    let near = |id: BeaverId| agent_cell(sim, id).map_or(usize::MAX, |c| d2(c, home));
    let able: Vec<BeaverId> = sim.colony.beavers.iter().filter(|b| b.can_work() && sim.work.slot_of(b.id).is_none()).map(|b| b.id).collect();
    let doing = |id: BeaverId| sim.agents.get(id).and_then(|a| a.assignment).and_then(|a| assignment_role(sim, a));
    let free = able.into_iter().filter(|&id| doing(id).is_none()).min_by_key(|&id| (near(id), id))?;
    Some((free, Assignment::Area(kind, area)))
}

/// One more on a gathering role: the beaver `send_out_who` picks is sent out from home for
/// it, by hand.
fn send_out(sim: &mut SimState, role: RoleKind) -> Result<(), SimError> {
    let (who, a) = send_out_who(sim, role).ok_or_else(|| invalid(format!("no beaver is free to send out as {role:?}")))?;
    order(sim, who, Some(a));
    sim.agents.assign(who, Some(a));
    Ok(())
}

/// A beaver sent somewhere by hand leaves any workplace it held.
pub(crate) fn order(sim: &mut SimState, who: BeaverId, to: Option<Assignment>) {
    if to.is_some() {
        sim.work.vacate(who);
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

#[cfg(test)]
mod home_run_tests {
    use super::*;

    /// No building to forage from: foragers are beavers sent out from home. The gathering
    /// rows are on the board from the start, nobody works until sent, and the buttons take
    /// one off (it stays free) and put one on.
    #[test]
    fn gatherers_sent_from_home_can_be_freed_and_added() {
        let (mut s, site) = crate::starts::new_game_on_map("voyageurs").unwrap();
        s.apply(crate::Command::Settle { cell: site.camp }).unwrap();
        crate::sandbox::run_days(&mut s, 1.0);
        let count = |s: &SimState| out_gathering(s, RoleKind::Forager).len();
        assert_eq!(count(&s), 0, "nobody is sent out without an order");
        let rows: Vec<RoleKind> = jobs_board(&s).iter().map(|r| r.role).collect();
        for role in [RoleKind::Forager, RoleKind::Logger, RoleKind::MudDigger] {
            assert!(rows.contains(&role), "{role:?} row is on the board before anyone gathers");
        }
        adjust_role(&mut s, RoleKind::Forager, 2).unwrap();
        assert_eq!(count(&s), 2);
        crate::sandbox::run_days(&mut s, 1.0);
        assert_eq!(count(&s), 2, "foragers sent by hand stay out");
        adjust_role(&mut s, RoleKind::Forager, -1).unwrap();
        assert_eq!(count(&s), 1);
        adjust_role(&mut s, RoleKind::Forager, 1).unwrap();
        assert_eq!(count(&s), 2);
    }
}
