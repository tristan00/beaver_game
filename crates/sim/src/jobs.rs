use crate::agents::{is_site, Assignment};
use crate::buildings::{BuildingId, Good};
use crate::colony::BeaverId;
pub use crate::content::{Duty, Job};
use crate::resources::ResourceKind;
use crate::water::Cell;
use crate::work::Priority;
use crate::{SimError, SimState};

pub fn gather_role(k: ResourceKind) -> Job {
    k.row().role
}

pub(crate) fn assignment_role(sim: &SimState, a: Assignment) -> Option<Job> {
    match a {
        Assignment::Resource(id) => sim.resources.get(id).map(|r| gather_role(r.kind)),
        Assignment::Area(k, _) => Some(gather_role(k)),
        Assignment::Slot(b, i) => sim.work.place(b)?.slots.get(i).map(|s| s.role),
        Assignment::Go(_) => None,
        Assignment::Mend => Some(Job::with_duty(crate::content::Duty::Mend)),
        Assignment::Build => Some(Job::with_duty(crate::content::Duty::Build)),
    }
}

/// A gathering role that belongs to seats at a workplace (like Pebbler at a Pebble heap),
/// rather than being sent out freely without a seat cap.
fn is_seated_gather(role: Job) -> bool {
    role.duty() == Duty::Gather && crate::buildings::BuildingKind::all().any(|k| k.spec().slots.contains(&role))
}

pub fn role_of(sim: &SimState, who: BeaverId) -> Option<Job> {
    if let Some((_, _, r)) = sim.work.slot_of(who) {
        return Some(r);
    }
    sim.agents.get(who).and_then(|a| a.assignment).and_then(|a| assignment_role(sim, a))
}

pub fn is_busy(sim: &SimState, who: BeaverId) -> bool {
    sim.work.slot_of(who).is_some() || sim.agents.get(who).is_some_and(|a| a.assignment.is_some())
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct RoleRow {
    pub role: Job,
    pub filled: usize,
    pub slots: usize,
    pub output_per_day: Vec<(Good, f32)>,
    /// The beavers filling it: slot beavers first, then those on orders.
    pub beavers: Vec<BeaverId>,
}

fn working_places(sim: &SimState) -> impl Iterator<Item = &crate::work::Workplace> {
    sim.work
        .places
        .iter()
        .filter(move |p| !p.paused && (sim.buildings.building(p.building).is_some_and(|b| b.operational() && !sim.buildings.unusable(b, &sim.water.depth)) || is_site(sim, p)))
}

pub fn jobs_board(sim: &SimState) -> Vec<RoleRow> {
    Job::by_priority()
        .into_iter()
        .map(|role| {
            let (mut filled, mut slots) = (0, 0);
            let mut out: Vec<(Good, f32)> = Vec::new();
            let mut beavers: Vec<BeaverId> = Vec::new();
            for p in working_places(sim).filter(|p| !p.paused) {
                let mine = p.slots.iter().filter(|s| s.role == role && !s.off);
                let mut n = mine.clone().count();
                if n == 0 {
                    continue;
                }
                // Builder seats are open-ended: those filled, and one more.
                if role == Job::with_duty(crate::content::Duty::Build) && p.open_ended() {
                    n = p.builders() + usize::from(crate::agents::takes_builders(sim, p));
                }
                slots += n;
                filled += mine.clone().filter(|s| s.beaver.is_some()).count();
                beavers.extend(mine.filter_map(|s| s.beaver));
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
            beavers.extend(on_orders);
            out.sort_by(|a, b| b.1.total_cmp(&a.1));
            RoleRow { role, filled: filled + ordered, slots: slots + ordered, output_per_day: out, beavers }
        })
        // Unlimited roles (builders, menders, and unseated gatherers) are always shown so the
        // first beaver can be assigned; seated roles appear once a workplace stands for them.
        .filter(|r| {
            r.slots > 0
                || r.filled > 0
                || gather_kind(r.role).is_some_and(|k| !is_seated_gather(r.role) && crate::agents::tools_at_hand(sim, k))
                || matches!(r.role.duty(), Duty::Mend | Duty::Build)
        })
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
        // Kits here are the ones still too young to work; older kits count as beavers.
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
        .filter(|p| !p.paused && (p.filled() < p.max_beavers || p.open_ended()) && b.can_work())
        .filter_map(|p| {
            let builders_here = crate::agents::takes_builders(sim, p);
            p.open_seat(|r| builders_here || r.duty() != crate::content::Duty::Build).map(|(i, _)| (p.building, i))
        })
        .collect()
}

/// Why a beaver holding a job is not doing it now.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Wait {
    /// Its site (a building, or the dam) is short of this good and none is to be had.
    Good(Good),
    /// The beds and banks it digs are frozen in.
    Frozen,
    /// The stores have no room for what it gathers.
    StoreFull(Good),
    /// Its workplace is paused.
    Paused,
    /// Nothing it can reach to do just now.
    NoWork,
}

/// Why a beaver with a job is not at work: none while it works, carries, rests or eats, or
/// has no job. The one answer the inspector, the colony table and the alerts all give.
pub fn waiting(sim: &SimState, id: BeaverId) -> Option<Wait> {
    let b = sim.colony.beaver(id)?;
    if !b.can_work() || b.resting {
        return None;
    }
    let a = sim.agents.get(id)?;
    if !matches!(a.task, crate::agents::Task::Idle | crate::agents::Task::Rest) || a.load.is_some() {
        return None;
    }
    let role = role_of(sim, id)?;
    let place_id = sim
        .work
        .slot_of(id)
        .map(|(b, _, _)| b)
        .or_else(|| (a.assignment == Some(Assignment::Build)).then(|| sim.agents.build_targets.get(&id).copied()).flatten());
    if let Some(p) = place_id.and_then(|b| sim.work.place(b)) {
        if p.paused {
            return Some(Wait::Paused);
        }
        if let Some(g) = crate::agents::site_waits_on(sim, p) {
            return Some(Wait::Good(g));
        }
        // A dam with cells still to pack, and no sticks to pack them with.
        let dam = sim.buildings.building(p.building).is_some_and(|b| b.dam.is_some());
        if dam && crate::dams::has_work_for(sim, p.building, false) && sim.buildings.stores.amount(crate::dams::material()) < crate::dams::sticks_per_step().min(crate::agents::load_size(crate::dams::material())) {
            return Some(Wait::Good(crate::dams::material()));
        }
    }
    if crate::agents::frozen(sim) && frozen_out(role) {
        return Some(Wait::Frozen);
    }
    if let Some(g) = crate::hud::gathered_good(role).filter(|g| sim.buildings.stores.capacity(*g) > crate::work::EPS && sim.buildings.stores.space(*g) <= crate::work::EPS) {
        return Some(Wait::StoreFull(g));
    }
    // Finding no work, a beaver goes home to wait (`Task::Rest`); one still idle out here is
    // between two tasks, unless there is no home to go to.
    (a.task == crate::agents::Task::Rest || !crate::agents::has_lodge(sim)).then_some(Wait::NoWork)
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
                    .any(|p| p.paused && b.can_work() && p.slots.iter().any(|s| s.beaver.is_none()));
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

fn free_beaver_check(sim: &SimState) -> Result<(), String> {
    let mut any_free = false;
    for b in &sim.colony.beavers {
        if b.can_work() && !is_busy(sim, b.id) {
            any_free = true;
            if agent_cell(sim, b.id).is_some() {
                return Ok(());
            }
        }
    }
    if any_free {
        Err("Beavers not ready yet".into())
    } else {
        Err("No free beavers".into())
    }
}

fn empty_slot(sim: &SimState, b: BuildingId, role: Option<Job>) -> Option<(usize, Job, Cell)> {
    let p = sim.work.place(b)?;
    let bd = sim.buildings.building(b)?;
    if p.paused || !(bd.operational() || is_site(sim, p)) || sim.buildings.unusable(bd, &sim.water.depth) {
        return None;
    }
    let builders_here = crate::agents::takes_builders(sim, p);
    let (i, r) = p.open_seat(|r| role.is_none_or(|want| r == want) && (builders_here || r.duty() != crate::content::Duty::Build))?;
    Some((i, r, bd.pos))
}

/// Whether a beaver can be added to this building now, or the plain reason it cannot.
pub fn add_beaver_check(sim: &SimState, b: BuildingId) -> Result<AddPreview, String> {
    let (_, _, at) = empty_slot(sim, b, None).ok_or_else(|| workplace_error_reason(sim, b))?;
    pick(sim, at).map(|p| p.0).ok_or_else(|| free_beaver_check(sim).err().unwrap_or_else(|| "No free beavers".into()))
}

pub fn add_beaver_preview(sim: &SimState, b: BuildingId) -> Option<AddPreview> {
    add_beaver_check(sim, b).ok()
}

fn role_target(sim: &SimState, role: Job) -> Option<(BuildingId, usize, AddPreview)> {
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

/// Whether another beaver can take `role` now, or the plain reason it cannot.
pub fn role_add_check(sim: &SimState, role: Job) -> Result<AddPreview, String> {
    free_beaver_check(sim)?;
    if let Some((_, _, pv)) = role_target(sim, role) {
        return Ok(pv);
    }
    if matches!(role.duty(), Duty::Mend | Duty::Build) {
        return send_out_who(sim, role).map(|(who, _)| AddPreview { who }).ok_or_else(|| "No free beavers".into());
    }
    if role.duty() == Duty::Gather && !is_seated_gather(role) {
        if let Some(kind) = gather_kind(role) {
            if let Some(need) = kind.row().needs {
                if !crate::agents::tools_at_hand(sim, kind) {
                    return Err(format!("Needs a {}", need.name().to_lowercase()));
                }
            }
        }
        return send_out_who(sim, role).map(|(who, _)| AddPreview { who }).ok_or_else(|| "No colony home".into());
    }
    let kind = crate::buildings::BuildingKind::all().find(|k| k.spec().slots.contains(&role));
    let buildings: Vec<_> = sim.buildings.buildings.iter().filter(|b| b.kind.spec().slots.contains(&role)).collect();
    if buildings.is_empty() {
        return Err(kind.map_or_else(|| "No workplace".into(), |k| format!("Build a {}", k.name().to_lowercase())));
    }
    let complete: Vec<_> = buildings.iter().copied().filter(|b| b.is_complete()).collect();
    if complete.is_empty() {
        return Err(format!("{} still being built", buildings[0].kind.name()));
    }
    if let Some(b) = complete.iter().find(|b| sim.work.place(b.id).is_some_and(|p| p.paused)) {
        return Err(format!("{} paused", b.kind.name()));
    }
    if let Some(b) = complete.iter().find(|b| crate::buildings::flooded(b, &sim.water.depth)) {
        return Err(format!("{} flooded", b.kind.name()));
    }
    if let Some(b) = complete.iter().find(|b| sim.buildings.stranded(b, &sim.water.depth)) {
        return Err(format!("{} high and dry", b.kind.name()));
    }
    if let Some(b) = complete.iter().find(|b| !b.operational()) {
        return Err(format!("{} ruined", b.kind.name()));
    }
    if complete.iter().any(|b| sim.work.place(b.id).is_some_and(|p| p.slots.iter().any(|s| s.role == role && s.beaver.is_none() && s.off))) {
        return Err("Seat switched off".into());
    }
    Err(kind.map_or_else(|| "All seats full".into(), |k| format!("All seats full — build another {}", k.name().to_lowercase())))
}

pub fn role_add_preview(sim: &SimState, role: Job) -> Option<AddPreview> {
    role_add_check(sim, role).ok()
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
    p.max_beavers = p.max_beavers.max(p.filled());
    Ok(())
}

/// Plain reason why `b` cannot take another beaver now.
pub fn workplace_error_reason(sim: &SimState, b: BuildingId) -> String {
    let Some(bd) = sim.buildings.building(b) else {
        return "Building gone".into();
    };
    let Some(p) = sim.work.place(b) else {
        return "No worker slots".into();
    };
    if p.paused {
        return "Workplace paused".into();
    }
    if crate::buildings::flooded(bd, &sim.water.depth) {
        return "Building flooded".into();
    }
    if sim.buildings.stranded(bd, &sim.water.depth) {
        return "High and dry".into();
    }
    if bd.is_complete() && !bd.operational() {
        return "Building ruined".into();
    }
    if p.slots.is_empty() {
        return "No worker slots".into();
    }
    if p.slots.iter().any(|s| s.beaver.is_none() && s.off) {
        return "Seat switched off".into();
    }
    if !p.open_ended() && p.filled() >= p.slots.len() {
        return "All seats full".into();
    }
    if p.open_ended() && !crate::agents::takes_builders(sim, p) {
        return "Nothing to build".into();
    }
    "Takes no beavers now".into()
}

fn workplace_error(sim: &SimState, b: BuildingId) -> SimError {
    invalid(workplace_error_reason(sim, b))
}

pub(crate) fn add_beaver(sim: &mut SimState, b: BuildingId) -> Result<(), SimError> {
    let (slot, _, at) = empty_slot(sim, b, None).ok_or_else(|| workplace_error(sim, b))?;
    let (pv, _) = pick(sim, at).ok_or_else(|| invalid(free_beaver_check(sim).err().unwrap_or_else(|| "No free beavers".into())))?;
    place_by_hand(sim, b, slot, pv.who)
}

fn free_last(sim: &mut SimState, b: BuildingId, role: Option<Job>) -> Result<BeaverId, SimError> {
    let p = sim.work.place(b).ok_or_else(|| invalid("No worker slots"))?;
    let i = p
        .slots
        .iter()
        .rposition(|s| s.beaver.is_some() && role.is_none_or(|r| s.role == r))
        .ok_or_else(|| invalid("No beaver to free"))?;
    let who = p.slots[i].beaver.expect("filled");
    sim.work.unassign(b, i).map_err(|e| invalid(format!("{e:?}")))?;
    let p = sim.work.place_mut(b).map_err(|e| invalid(format!("{e:?}")))?;
    p.max_beavers = p.filled();
    Ok(who)
}

pub(crate) fn remove_beaver(sim: &mut SimState, b: BuildingId) -> Result<(), SimError> {
    free_last(sim, b, None).map(|_| ())
}

pub(crate) fn set_priority(sim: &mut SimState, b: BuildingId, priority: Priority) -> Result<(), SimError> {
    sim.work.place_mut(b).map_err(|_| invalid(format!("building {} has no beaver slots", b.0)))?.priority = priority;
    Ok(())
}

/// Switch one beaver slot on or off; switching off sends its beaver away.
pub(crate) fn set_slot_off(sim: &mut SimState, b: BuildingId, slot: usize, off: bool) -> Result<(), SimError> {
    let p = sim.work.place_mut(b).map_err(|_| invalid(format!("building {} has no beaver slots", b.0)))?;
    let s = p.slots.get_mut(slot).ok_or_else(|| invalid("SlotOutOfRange"))?;
    s.off = off;
    if off {
        s.beaver = None;
    }
    Ok(())
}

pub(crate) fn set_paused(sim: &mut SimState, b: BuildingId, paused: bool) -> Result<(), SimError> {
    let p = sim.work.place_mut(b).map_err(|_| invalid(format!("building {} has no beaver slots", b.0)))?;
    p.paused = paused;
    if paused {
        for s in &mut p.slots {
            s.beaver = None;
        }
    }
    Ok(())
}

pub(crate) fn adjust_role(sim: &mut SimState, role: Job, delta: i32) -> Result<(), SimError> {
    for n in 0..delta.unsigned_abs() {
        let step = if delta > 0 {
            match role_target(sim, role) {
                Some((b, slot, pv)) => place_by_hand(sim, b, slot, pv.who),
                None => match send_out_who(sim, role) {
                    Some((who, a)) => {
                        order(sim, who, Some(a));
                        sim.agents.assign(who, Some(a));
                        Ok(())
                    }
                    None => Err(invalid(role_add_check(sim, role).err().unwrap_or_else(|| "Cannot assign job".into()))),
                },
            }
        } else {
            let from = sim
                .work
                .places
                .iter()
                .filter(|p| p.slots.iter().any(|s| s.beaver.is_some() && s.role == role))
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
/// A gathering job whose every source winter closes: its beavers wait out frozen ground.
pub fn frozen_out(job: Job) -> bool {
    let mut kinds = ResourceKind::all().filter(|k| k.row().role == job).peekable();
    kinds.peek().is_some() && kinds.all(|k| k.row().freezes)
}

pub fn gather_kind(role: Job) -> Option<ResourceKind> {
    ResourceKind::all().find(|k| k.row().role == role)
}

/// Beavers out gathering for `role` from home (no building to work at), most recent first.
pub(crate) fn out_gathering(sim: &SimState, role: Job) -> Vec<BeaverId> {
    sim.colony.beavers.iter().rev()
        .filter(|b| sim.work.slot_of(b.id).is_none())
        .filter(|b| sim.agents.get(b.id).and_then(|a| a.assignment).and_then(|a| assignment_role(sim, a)) == Some(role))
        .map(|b| b.id)
        .collect()
}

/// One fewer on a gathering role: the latest beaver sent out for it comes back and is free.
fn call_in(sim: &mut SimState, role: Job) -> Result<(), SimError> {
    let Some(&who) = out_gathering(sim, role).first() else {
        return Err(invalid(format!("No {} to free", role.row().plural)));
    };
    sim.agents.assign(who, None);
    Ok(())
}

/// Who would be sent out for one more on a gathering role, and where: the nearest free beaver.
fn send_out_who(sim: &SimState, role: Job) -> Option<(BeaverId, Assignment)> {
    let (home, assignment) = if role == Job::with_duty(crate::content::Duty::Mend) {
        (crate::agents::home_area(sim).map_or((0, 0), |h| h.0), Assignment::Mend)
    } else if role == Job::with_duty(crate::content::Duty::Build) {
        (crate::agents::home_area(sim).map_or((0, 0), |h| h.0), Assignment::Build)
    } else {
        if is_seated_gather(role) {
            return None;
        }
        let kind = gather_kind(role)?;
        if !crate::agents::tools_at_hand(sim, kind) {
            return None;
        }
        let (home, area) = crate::agents::home_area(sim)?;
        (home, Assignment::Area(kind, area))
    };
    // Free: no seat, no job and no move order of the player's, and on the map.
    let free = sim
        .colony
        .beavers
        .iter()
        .filter(|b| b.can_work() && !is_busy(sim, b.id))
        .filter_map(|b| agent_cell(sim, b.id).map(|c| (b.id, d2(c, home))))
        .min_by_key(|&(id, d)| (d, id))
        .map(|(id, _)| id)?;
    Some((free, assignment))
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
            let job = role_of(sim, b.id)?;
            let working = sim.agents.get(b.id).is_some_and(|a| !matches!(a.task, crate::agents::Task::Idle | crate::agents::Task::Rest));
            Some((b.id, (job, working)))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    struct DryLand;
    impl crate::colony::Landscape for DryLand {
        fn is_water(&self, _: f32, _: f32) -> bool {
            false
        }
    }

    #[test]
    fn role_add_check_explains_refusals() {
        let mut sim = SimState::new(1);
        // No beavers yet -> "No free beavers"
        assert_eq!(role_add_check(&sim, Job::with_duty(Duty::Build)).unwrap_err(), "No free beavers");
        // Found a tiny colony on a dry cell with free working beavers.
        let cell = (sim.world.size / 2, sim.world.size / 2);
        sim.colony.found((cell.0 as f32, cell.1 as f32), &DryLand).expect("found");
        for b in &sim.colony.beavers {
            sim.agents.spawn(b.id, cell);
        }
        // Builder is unlimited even with no active build site.
        assert!(role_add_check(&sim, Job::with_duty(Duty::Build)).is_ok());
        // Seated roles explain which workplace is needed.
        assert_eq!(role_add_check(&sim, Job::with_duty(Duty::Haul)).unwrap_err(), "Build a haul mound");
        assert_eq!(role_add_check(&sim, Job::with_duty(Duty::Forest)).unwrap_err(), "Build a forester");
        assert_eq!(role_add_check(&sim, Job::with_duty(Duty::Watch)).unwrap_err(), "Build a lookout");
        assert_eq!(role_add_check(&sim, Job::of("Pebbler")).unwrap_err(), "Build a pebble heap");
        // Unlimited gathering & upkeep roles work as soon as free beavers exist.
        assert!(role_add_check(&sim, Job::of("Forager")).is_ok());
        assert!(role_add_check(&sim, Job::of("Feller")).is_ok());
        assert!(role_add_check(&sim, Job::with_duty(Duty::Mend)).is_ok());
        // Assigning multiple builders succeeds without any build site.
        adjust_role(&mut sim, Job::with_duty(Duty::Build), 2).expect("assign 2 builders");
        assert_eq!(jobs_board(&sim).iter().find(|r| r.role == Job::with_duty(Duty::Build)).unwrap().filled, 2);
        // Free those builders and test Pebbler strictly capped at 1 per Pebble heap.
        adjust_role(&mut sim, Job::with_duty(Duty::Build), -2).expect("free 2 builders");
        let heap = crate::buildings::BuildingKind::of("PebbleHeap");
        let b = crate::buildings::Building {
            id: BuildingId(999),
            kind: heap,
            pos: cell,
            status: crate::buildings::Status::Complete,
            output: 0.0,
            condition: 1.0,
            reinforced: false,
            dam: None,
            canal: None,
        };
        sim.work.add_complete(&b).expect("add pebble heap workplace");
        sim.buildings.buildings.push(b);
        assert!(role_add_check(&sim, Job::of("Pebbler")).is_ok());
        adjust_role(&mut sim, Job::of("Pebbler"), 1).expect("assign 1 pebbler");
        assert_eq!(
            role_add_check(&sim, Job::of("Pebbler")).unwrap_err(),
            "All seats full — build another pebble heap"
        );
        assert!(adjust_role(&mut sim, Job::of("Pebbler"), 1).is_err());
    }
}
