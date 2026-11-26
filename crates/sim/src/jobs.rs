use crate::agents::Assignment;
use crate::buildings::{BuildingId, BuildingKind, Good};
use crate::colony::BeaverId;
pub use crate::content::{Duty, Job};
use crate::gates::invalid;
use crate::resources::ResourceKind;
use crate::water::Cell;
use crate::{SimError, SimState};

pub fn gather_role(k: ResourceKind) -> Job { k.row().role }
pub fn gather_kind(role: Job) -> Option<ResourceKind> { ResourceKind::all().find(|k| k.row().role == role) }
pub fn frozen_out(job: Job) -> bool {
    let mut k = ResourceKind::all().filter(|k| k.row().role == job).peekable();
    k.peek().is_some() && k.all(|k| k.row().freezes)
}
fn is_seated_gather(role: Job) -> bool { role.duty() == Duty::Gather && BuildingKind::all().any(|k| k.spec().slots.contains(&role)) }

pub(crate) fn assignment_role(_sim: &SimState, a: Assignment) -> Option<Job> {
    match a {
        Assignment::Resource(_, k) | Assignment::Area(k, _) => Some(gather_role(k)),
        Assignment::Mend(_) => Some(Job::with_duty(Duty::Mend)),
        Assignment::Build(_) => Some(Job::with_duty(Duty::Build)),
        Assignment::Go(_) | Assignment::Work(_) => None,
    }
}

pub fn role_of(sim: &SimState, who: BeaverId) -> Option<Job> {
    sim.work.slot_of(who).map(|(_, _, r)| r)
        .or_else(|| sim.agents.get(who)?.assignment.and_then(|a| assignment_role(sim, a)))
}
pub fn is_busy(sim: &SimState, who: BeaverId) -> bool {
    sim.work.slot_of(who).is_some() || sim.agents.get(who).is_some_and(|a| a.assignment.is_some())
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct RoleRow { pub role: Job, pub filled: usize, pub slots: usize, pub output_per_day: Vec<(Good, f32)>, pub beavers: Vec<BeaverId> }

fn working_places(sim: &SimState) -> impl Iterator<Item = &crate::work::Workplace> {
    sim.work.places.iter().filter(move |p| crate::agents::place_open(sim, p))
}

fn roaming_in_role(sim: &SimState, role: Job) -> Vec<BeaverId> {
    sim.colony.beavers.iter()
        .filter(|b| sim.work.slot_of(b.id).is_none() && sim.agents.get(b.id).and_then(|a| a.assignment).and_then(|a| assignment_role(sim, a)) == Some(role))
        .map(|b| b.id).collect()
}

pub fn jobs_board(sim: &SimState) -> Vec<RoleRow> {
    Job::by_priority().into_iter().map(|role| {
        let (mut filled, mut slots, mut out, mut beavers) = (0, 0, Vec::<(Good, f32)>::new(), Vec::new());
        for p in working_places(sim) {
            if p.site.is_some() { continue; }
            let mine = p.slots.iter().filter(|s| s.role == role && !s.off);
            let n = mine.clone().count();
            if n == 0 { continue; }
            slots += n;
            filled += mine.clone().filter(|s| s.beaver.is_some()).count();
            beavers.extend(mine.filter_map(|s| s.beaver));
            for (g, r) in sim.ledger.building_rates(p.building) {
                match out.iter_mut().find(|e| e.0 == g) { Some(e) => e.1 += r, None => out.push((g, r)) }
            }
        }
        let on_orders = roaming_in_role(sim, role);
        let ordered = on_orders.len();
        beavers.extend(on_orders);
        out.sort_by(|a, b| b.1.total_cmp(&a.1));
        RoleRow { role, filled: filled + ordered, slots: slots + ordered, output_per_day: out, beavers }
    }).filter(|r| {
        r.slots > 0 || r.filled > 0 || matches!(r.role.duty(), Duty::Mend | Duty::Build)
            || gather_kind(r.role).is_some_and(|k| !is_seated_gather(r.role) && crate::agents::tools_at_hand(sim, k))
    }).collect()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Headcount { pub working: usize, pub free: usize, pub kits: usize, pub total: usize }

pub fn headcount(sim: &SimState) -> Headcount {
    let mut h = Headcount { working: 0, free: 0, kits: 0, total: sim.colony.population() };
    for b in &sim.colony.beavers {
        if !b.can_work() { h.kits += 1; } else if is_busy(sim, b.id) { h.working += 1; } else { h.free += 1; }
    }
    h
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum IdleReason { NoOpenSlot, Unreachable, Resting, Paused, Free }

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Wait { Good(Good), Frozen, StoreFull(Good), Paused, NoWork }

fn place_wait(sim: &SimState, p: &crate::work::Workplace, is_upkeep: bool) -> Option<Wait> {
    if p.paused { return Some(Wait::Paused); }
    if let Some(g) = crate::agents::site_waits_on(sim, p) { return Some(Wait::Good(g)); }
    let mat = crate::dams::material();
    if sim.buildings.building(p.building).is_some_and(|b| b.dam.is_some())
        && crate::dams::has_work_for(sim, p.building, is_upkeep)
        && !crate::agents::has_good_source(sim, mat)
    { return Some(Wait::Good(mat)); }
    if sim.buildings.building(p.building).is_some_and(|b| b.canal.is_some())
        && crate::agents::frozen(sim)
        && crate::canals::has_work_for(sim, p.building, is_upkeep)
    { return Some(Wait::Frozen); }
    None
}

pub fn waiting(sim: &SimState, id: BeaverId) -> Option<Wait> {
    let b = sim.colony.beaver(id)?;
    if !b.can_work() || b.resting { return None; }
    let a = sim.agents.get(id)?;
    if !matches!(a.task, crate::agents::Task::Idle | crate::agents::Task::Rest) || a.load.is_some() { return None; }
    let role = role_of(sim, id)?;
    let is_upkeep = a.is_mender();
    if let Some(p) = crate::agents::place_of(sim, a).and_then(|(bid, _, _)| sim.work.place(bid)) {
        if let Some(w) = place_wait(sim, p, is_upkeep) { return Some(w); }
    } else if matches!(a.assignment, Some(Assignment::Build(None) | Assignment::Mend(None))) {
        let mut paused = false;
        for p in &sim.work.places {
            let relevant = if is_upkeep { crate::agents::place_needs_menders(sim, p.building) } else { crate::agents::place_needs_builders(sim, p.building) };
            if relevant && let Some(w) = place_wait(sim, p, is_upkeep) { return Some(w); }
            paused |= p.paused && p.is_open_site();
        }
        if paused { return Some(Wait::Paused); }
    }
    if crate::agents::frozen(sim) && frozen_out(role) { return Some(Wait::Frozen); }
    if let Some(g) = crate::hud::gathered_good(role)
        .filter(|&g| sim.buildings.stores.capacity(g) > crate::work::EPS && !crate::agents::worth_fetching(sim, g))
    {
        return Some(Wait::StoreFull(g));
    }
    (a.task == crate::agents::Task::Rest || !crate::agents::has_lodge(sim)).then_some(Wait::NoWork)
}

pub fn idle_reasons(sim: &SimState) -> Vec<(BeaverId, IdleReason)> {
    crate::hud::idle_beavers(sim).into_iter().filter_map(|id| {
        let b = sim.colony.beaver(id)?;
        if b.resting { return Some((id, IdleReason::Resting)); }
        let open: Vec<BuildingId> = working_places(sim)
            .filter(|p| b.can_work() && (empty_slot(sim, p.building, None).is_some() || crate::agents::place_needs_builders(sim, p.building)))
            .map(|p| p.building).collect();
        if open.is_empty() {
            let paused = sim.work.places.iter().any(|p| p.paused && b.can_work() && (p.is_open_site() || p.slots.iter().any(|s| s.beaver.is_none() && !s.off)));
            return Some((id, if paused { IdleReason::Paused } else { IdleReason::NoOpenSlot }));
        }
        let reachable = sim.agents.get(id).is_none_or(|a| open.iter().take(4).filter_map(|&bid| sim.buildings.building(bid))
            .any(|bd| bd.cells().any(|c| a.cell.0.abs_diff(c.0) + a.cell.1.abs_diff(c.1) <= 1 || sim.nav.may_reach_beside(a.cell, c))));
        Some((id, if reachable { IdleReason::Free } else { IdleReason::Unreachable }))
    }).collect()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct AddPreview { pub who: BeaverId }

fn d2(a: Cell, b: Cell) -> usize { a.0.abs_diff(b.0).pow(2) + a.1.abs_diff(b.1).pow(2) }

fn pick(sim: &SimState, at: Cell) -> Option<(AddPreview, usize)> {
    sim.colony.beavers.iter().filter(|b| b.can_work() && !is_busy(sim, b.id))
        .filter_map(|b| sim.agents.get(b.id).map(|a| (b.id, d2(a.cell, at))))
        .min_by_key(|&(id, d)| (d, id)).map(|(who, d)| (AddPreview { who }, d))
}

fn free_beaver_check(sim: &SimState) -> Result<(), String> {
    let mut any = false;
    for b in &sim.colony.beavers {
        if b.can_work() && !is_busy(sim, b.id) {
            any = true;
            if sim.agents.get(b.id).is_some() { return Ok(()); }
        }
    }
    Err(if any { "Beavers not ready yet" } else { "No free beavers" }.into())
}

pub fn empty_slot(sim: &SimState, b: BuildingId, role: Option<Job>) -> Option<(usize, Job, Cell)> {
    let (p, bd) = (sim.work.place(b)?, sim.buildings.building(b)?);
    if !crate::agents::place_open(sim, p) || p.site.is_some() { return None; }
    let (i, r) = p.open_seat(|r| role.is_none_or(|want| r == want))?;
    Some((i, r, bd.pos))
}

pub fn add_beaver_check(sim: &SimState, b: BuildingId) -> Result<AddPreview, String> {
    let (_, _, at) = empty_slot(sim, b, None).ok_or_else(|| workplace_error_reason(sim, b))?;
    pick(sim, at).map(|p| p.0).ok_or_else(|| free_beaver_check(sim).err().unwrap_or_else(|| "No free beavers".into()))
}

fn role_target(sim: &SimState, role: Job) -> Option<(BuildingId, usize, AddPreview)> {
    sim.work.places.iter().filter_map(|p| {
        let (i, _, at) = empty_slot(sim, p.building, Some(role))?;
        let (pv, d) = pick(sim, at)?;
        Some((d, p.building, i, pv))
    }).min_by_key(|t| (t.0, t.1)).map(|t| (t.1, t.2, t.3))
}

pub fn role_add_check(sim: &SimState, role: Job) -> Result<AddPreview, String> {
    free_beaver_check(sim)?;
    if let Some((_, _, pv)) = role_target(sim, role) { return Ok(pv); }
    if matches!(role.duty(), Duty::Mend | Duty::Build) {
        return send_out_who(sim, role).map(|(who, _)| AddPreview { who }).ok_or_else(|| "No free beavers".into());
    }
    if role.duty() == Duty::Gather && !is_seated_gather(role) {
        if let Some(kind) = gather_kind(role) && let Some(need) = kind.row().needs && !crate::agents::tools_at_hand(sim, kind) {
            return Err(format!("Needs a {}", need.name().to_lowercase()));
        }
        return send_out_who(sim, role).map(|(who, _)| AddPreview { who }).ok_or_else(|| "No colony home".into());
    }
    let kind = BuildingKind::all().find(|k| k.spec().slots.contains(&role));
    let bs: Vec<_> = sim.buildings.buildings.iter().filter(|b| b.kind.spec().slots.contains(&role)).collect();
    if bs.is_empty() { return Err(kind.map_or_else(|| "No workplace".into(), |k| format!("Build a {}", k.name().to_lowercase()))); }
    let done: Vec<_> = bs.iter().copied().filter(|b| b.is_complete()).collect();
    if done.is_empty() { return Err(format!("{} still being built", bs[0].kind.name())); }
    if let Some(b) = done.iter().find(|b| sim.work.place(b.id).is_some_and(|p| p.paused)) { return Err(format!("{} paused", b.kind.name())); }
    if let Some(b) = done.iter().find(|b| sim.work.place(b.id).is_some_and(|p| p.site.is_some())) { return Err(format!("{} under refit", b.kind.name())); }
    if let Some(b) = done.iter().find(|b| crate::buildings::flooded(b, &sim.water.depth)) { return Err(format!("{} flooded", b.kind.name())); }
    if let Some(b) = done.iter().find(|b| sim.buildings.stranded(b, &sim.water.depth)) { return Err(format!("{} high and dry", b.kind.name())); }
    if let Some(b) = done.iter().find(|b| !b.operational()) { return Err(format!("{} ruined", b.kind.name())); }
    if done.iter().any(|b| sim.work.place(b.id).is_some_and(|p| p.slots.iter().any(|s| s.role == role && s.beaver.is_none() && s.off))) { return Err("Seat switched off".into()); }
    Err(kind.map_or_else(|| "All seats full".into(), |k| format!("All seats full — build another {}", k.name().to_lowercase())))
}

pub fn role_add_preview(sim: &SimState, role: Job) -> Option<AddPreview> { role_add_check(sim, role).ok() }

pub fn workplace_error_reason(sim: &SimState, b: BuildingId) -> String {
    let Some(bd) = sim.buildings.building(b) else { return "Building gone".into() };
    let Some(p) = sim.work.place(b) else { return "No worker slots".into() };
    if p.paused { "Workplace paused" }
    else if p.site.is_some() { if bd.is_complete() { "Under refit" } else { "Under construction" } }
    else if crate::buildings::flooded(bd, &sim.water.depth) { "Building flooded" }
    else if sim.buildings.stranded(bd, &sim.water.depth) { "High and dry" }
    else if bd.is_complete() && !bd.operational() { "Building ruined" }
    else if p.slots.is_empty() { "No worker slots" }
    else if p.slots.iter().any(|s| s.beaver.is_none() && s.off) { "Seat switched off" }
    else if p.filled() >= p.slots.len() { "All seats full" }
    else { "Takes no beavers now" }.into()
}

pub(crate) fn seat_in_workplace(sim: &mut SimState, b: BuildingId, slot: usize, who: BeaverId) -> Result<(), SimError> {
    let able = sim.colony.beaver(who).map(|x| x.can_work()).ok_or_else(|| invalid("NoSuchBeaver"))?;
    if !able { return Err(invalid("NotAdult")); }
    let p = sim.work.place(b).ok_or_else(|| invalid(workplace_error_reason(sim, b)))?;
    if !crate::agents::place_open(sim, p) || p.site.is_some() { return Err(invalid(workplace_error_reason(sim, b))); }
    match p.slots.get(slot) {
        None => return Err(invalid("SlotOutOfRange")),
        Some(s) if s.beaver.is_some() => return Err(invalid("SlotTaken")),
        Some(s) if s.off => return Err(invalid("Seat switched off")),
        Some(_) => {}
    }
    sim.work.assign(b, slot, who).map_err(|e| invalid(format!("{e:?}")))
}

pub(crate) fn place_by_hand(sim: &mut SimState, b: BuildingId, slot: usize, who: BeaverId) -> Result<(), SimError> {
    if !sim.agents.by_id.contains_key(&who) { return Err(invalid("no agent")); }
    seat_in_workplace(sim, b, slot, who)?;
    sim.agents.assign(who, None);
    Ok(())
}

pub(crate) fn staff_building(sim: &mut SimState, b: BuildingId, delta: i32) -> Result<(), SimError> {
    for n in 0..delta.unsigned_abs() {
        let step = if delta > 0 {
            let (slot, _, at) = empty_slot(sim, b, None).ok_or_else(|| invalid(workplace_error_reason(sim, b)))?;
            let (pv, _) = pick(sim, at).ok_or_else(|| invalid(free_beaver_check(sim).err().unwrap_or_else(|| "No free beavers".into())))?;
            place_by_hand(sim, b, slot, pv.who)
        } else {
            free_last(sim, b, None).map(|_| ())
        };
        if let Err(e) = step { return if n == 0 { Err(e) } else { Ok(()) }; }
    }
    Ok(())
}

fn free_last(sim: &mut SimState, b: BuildingId, role: Option<Job>) -> Result<BeaverId, SimError> {
    let p = sim.work.place(b).ok_or_else(|| invalid("No worker slots"))?;
    let i = p.slots.iter().rposition(|s| s.beaver.is_some() && role.is_none_or(|r| s.role == r)).ok_or_else(|| invalid("No beaver to free"))?;
    let who = p.slots[i].beaver.expect("filled");
    sim.work.unassign(b, i).map_err(|e| invalid(format!("{e:?}")))?;
    sim.agents.assign(who, None);
    Ok(who)
}

pub(crate) fn set_slot_off(sim: &mut SimState, b: BuildingId, slot: usize, off: bool) -> Result<(), SimError> {
    let s = sim.work.place_mut(b).map_err(|_| invalid(format!("building {} has no beaver slots", b.0)))?.slots.get_mut(slot).ok_or_else(|| invalid("SlotOutOfRange"))?;
    s.off = off;
    if let Some(who) = if off { s.beaver.take() } else { None } { sim.agents.assign(who, None); }
    Ok(())
}

pub(crate) fn set_paused(sim: &mut SimState, b: BuildingId, paused: bool) -> Result<(), SimError> {
    let p = sim.work.place_mut(b).map_err(|_| invalid(format!("building {} has no beaver slots", b.0)))?;
    p.paused = paused;
    if paused {
        let mut freed: Vec<BeaverId> = p.slots.iter_mut().filter_map(|s| s.beaver.take()).collect();
        for ag in sim.agents.by_id.values_mut() {
            if ag.clear_target(b) { freed.push(ag.id); }
        }
        for w in freed { crate::agents::wake(sim, w); }
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
                        sim.work.vacate(who);
                        if let Assignment::Mend(Some(b)) = a { crate::agents::mark_mending(sim, b); }
                        sim.agents.assign(who, Some(a));
                        if let Some(ag) = sim.agents.by_id.get_mut(&who) { ag.queue.clear(); }
                        Ok(())
                    }
                    None => Err(invalid(role_add_check(sim, role).err().unwrap_or_else(|| "Cannot assign job".into()))),
                },
            }
        } else {
            let from = sim.work.places.iter().filter(|p| p.slots.iter().any(|s| s.beaver.is_some() && s.role == role))
                .min_by_key(|p| std::cmp::Reverse(p.building)).map(|p| p.building);
            match from {
                Some(b) => free_last(sim, b, Some(role)).map(|_| ()),
                None => match roaming_in_role(sim, role).into_iter().last() {
                    Some(who) => { sim.agents.assign(who, None); Ok(()) }
                    None => Err(invalid(format!("No {} to free", role.row().plural))),
                },
            }
        };
        if let Err(e) = step { return if n == 0 { Err(e) } else { Ok(()) }; }
    }
    Ok(())
}

fn send_out_who(sim: &SimState, role: Job) -> Option<(BeaverId, Assignment)> {
    let (home, a) = match role.duty() {
        Duty::Mend => {
            let target = sim.buildings.buildings.iter()
                .filter(|b| crate::agents::place_needs_menders(sim, b.id))
                .min_by_key(|b| (crate::agents::menders_on(sim, b.id), b.id))
                .map(|b| (b.id, b.pos));
            let at = target.map_or_else(|| crate::agents::home_area(sim).map_or((0, 0), |h| h.0), |t| t.1);
            (at, Assignment::Mend(target.map(|t| t.0)))
        }
        Duty::Build => {
            let target = sim.work.places.iter()
                .filter(|p| crate::agents::place_needs_builders(sim, p.building) && !crate::agents::place_stalled(sim, p))
                .min_by_key(|p| (crate::agents::builders_on(sim, p.building), p.building))
                .and_then(|p| sim.buildings.building(p.building).map(|b| (b.id, b.pos)));
            let at = target.map_or_else(|| crate::agents::home_area(sim).map_or((0, 0), |h| h.0), |t| t.1);
            (at, Assignment::Build(target.map(|t| t.0)))
        }
        _ => {
            if is_seated_gather(role) { return None; }
            let kind = gather_kind(role)?;
            if !crate::agents::tools_at_hand(sim, kind) { return None; }
            let (home, area) = crate::agents::home_area(sim)?;
            (home, Assignment::Area(kind, area))
        }
    };
    Some((pick(sim, home)?.0.who, a))
}

pub(crate) fn slot_jobs(sim: &SimState) -> std::collections::HashMap<BeaverId, (Job, bool)> {
    sim.colony.beavers.iter().filter_map(|b| {
        let job = role_of(sim, b.id)?;
        let working = sim.agents.get(b.id).is_some_and(|a| !matches!(a.task, crate::agents::Task::Idle | crate::agents::Task::Rest));
        Some((b.id, (job, working)))
    }).collect()
}
