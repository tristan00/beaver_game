//! Repairs are mended on the spot by menders, who bring the materials a load at a time
//! (see `supply`) and use them as they work; a repair order only marks a building to be
//! mended first. Mud reinforcement and pebble facing are refits: paid work through the normal
//! material-delivery and labour system.
include!(concat!(env!("OUT_DIR"), "/maintenance_constants.rs"));
use std::collections::BTreeMap;
use crate::{buildings::{Amount, Building, BuildingId, BuildingKind as K}, work::{Priority, Workplace}, SimError, SimState};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
pub enum Refit { Repair, Reinforce, Facing, ReinforcedRepair }

/// Queued maintenance on one building. A refit turns the building's workplace into a site for
/// its duration and keeps the workplace here to put back; a repair replaces nothing and keeps an
/// empty one.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Job { pub kind: Refit, previous: Workplace }

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct Maintenance {
    pub jobs: BTreeMap<BuildingId, Job>,
    pub repairs_completed: u32,
    pub reinforcements_completed: u32,
}

pub fn can_reinforce(kind: K) -> bool {
    kind.spec().refits.contains_key(&Refit::Reinforce)
}

/// Dams and buildings can be faced with pebbles; canals cannot.
pub fn can_pebble(kind: K) -> bool {
    kind.spec().refits.contains_key(&Refit::Facing)
}

/// How many times longer a building lasts for a refit it has had.
pub fn lasts(kind: K, refit: Refit) -> f32 {
    kind.spec().refits.get(&refit).map_or(1.0, |r| r.lasts)
}

/// The cells a refit is costed by: a dam's cells, else the building's footprint.
fn cells(b: &Building) -> f32 {
    let (w, h) = b.kind.spec().footprint;
    b.dam.as_ref().map_or((w * h) as f32, |d| d.cells.len().max(1) as f32)
}

pub fn cost(b: &Building, kind: Refit) -> (Vec<Amount>, f32) {
    let spec = b.kind.spec();
    if kind != Refit::Repair {
        return spec.refits.get(&kind).map_or((Vec::new(), 0.0), |r| r.quote(cells(b)));
    }
    let damage = (1.0 - b.condition).clamp(0.0, 1.0);
    let (rates, per_unit) = mend_rates(b);
    let materials = rates.into_iter().map(|(g, a)| (g, (a * damage).ceil().max(1.0))).collect();
    (materials, (per_unit * damage).max(REPAIR_MIN_LABOUR))
}

/// What mending costs for each whole unit of condition put back: the materials, brought to the
/// building as the work goes, and the labour.
pub fn mend_rates(b: &Building) -> (Vec<Amount>, f32) {
    let spec = b.kind.spec();
    let (w, h) = spec.footprint;
    let mut materials: Vec<Amount> = spec.cost.iter().map(|&(g, a)| (g, a * REPAIR_MATERIAL_SHARE)).filter(|v| v.1 > 0.0).collect();
    if materials.is_empty() { materials.extend(crate::content::content().repair_fallback.iter().copied()); }
    if b.reinforced && let Some(r) = spec.refits.get(&Refit::ReinforcedRepair) {
        materials.extend(r.per_cell.iter().map(|&(g, a)| (g, (w * h) as f32 * a)));
    }
    (materials, (spec.work * REPAIR_LABOUR_SHARE).max(REPAIR_MIN_LABOUR))
}

/// Buildings menders mend: finished buildings (not dams or canals, which have their own
/// upkeep) that are worn below `AUTO_REPAIR_BELOW`, marked for repair, or already being mended,
/// until they are whole again.
pub fn needs_mending(s: &SimState, b: &Building) -> bool {
    b.is_complete() && b.dam.is_none() && b.canal.is_none() && b.condition < 1.0
        && (b.condition < AUTO_REPAIR_BELOW
            || s.maintenance.jobs.get(&b.id).is_some_and(|j| j.kind == Refit::Repair)
            || s.agents.maint_targets.values().any(|&t| t == b.id))
}

/// How much condition a mender's visit is worth bringing materials for: it starts work once
/// this much (or the whole repair, if less) can be done with what has been brought.
const MEND_BATCH: f32 = 0.05;

/// Whether enough has been brought to a building being mended to start on it.
pub fn mend_ready(s: &SimState, id: BuildingId) -> bool {
    let Some(b) = s.buildings.building(id).filter(|b| needs_mending(s, b)) else { return false };
    let spot = crate::supply::Spot { building: id, cell: b.pos };
    let (materials, _) = mend_rates(b);
    let step = (1.0 - b.condition).clamp(0.0, MEND_BATCH);
    materials.iter().all(|&(g, a)| crate::supply::delivered(s, spot, g) + 1e-4 >= a * step)
}

/// Put back `labour` worth of condition, using the materials brought to the building: as much
/// as they cover. Returns false when they cover none.
pub fn mend(s: &mut SimState, id: BuildingId, labour: f32) -> bool {
    let Some(b) = s.buildings.building(id) else { return false };
    let spot = crate::supply::Spot { building: id, cell: b.pos };
    let (materials, per_unit) = mend_rates(b);
    let covered = materials.iter().map(|&(g, a)| crate::supply::delivered(s, spot, g) / a.max(1e-6)).fold(f32::INFINITY, f32::min);
    let gain = (labour / per_unit).min(1.0 - b.condition).min(covered).max(0.0);
    if gain <= 1e-6 { return false; }
    let used: Vec<Amount> = materials.iter().map(|&(g, a)| (g, a * gain)).collect();
    if !crate::supply::take(s, spot, &used) { return false; }
    let Some(b) = s.buildings.buildings.iter_mut().find(|b|b.id==id) else { return false };
    b.condition += gain;
    if b.condition >= 1.0 - 1e-4 {
        b.condition = 1.0;
        s.maintenance.repairs_completed += 1;
        if s.maintenance.jobs.get(&id).is_some_and(|j| j.kind == Refit::Repair) { s.maintenance.jobs.remove(&id); }
    }
    true
}

pub fn order(s: &mut SimState, id: BuildingId, kind: Refit) -> Result<(), SimError> {
    let bad = |m: &str| SimError::InvalidCommand(m.into());
    let b = s.buildings.building(id).ok_or_else(||bad("no such building"))?;
    if s.maintenance.jobs.contains_key(&id) { return Err(bad("this building already has maintenance queued")); }
    if !b.is_complete() { return Err(bad("finish construction first")); }
    if b.kind.is_dam() && kind != Refit::Facing {return Err(bad("dams are kept up by menders cell by cell"));}
    if kind == Refit::Facing {
        if !can_pebble(b.kind) {return Err(bad("this structure cannot be faced with pebbles"));}
        if b.pebbled {return Err(bad("already faced with pebbles"));}
    } else if kind == Refit::Reinforce {
        if !can_reinforce(b.kind) {return Err(bad("this structure cannot be mud-reinforced"));}
        if b.reinforced {return Err(bad("already mud-reinforced"));}
    } else if b.condition >= REPAIR_CONDITION_THRESHOLD {return Err(bad("this building does not need repairs"));}
    if kind == Refit::Repair {
        // Mended first by the menders; the building keeps working meanwhile.
        let previous = Workplace::new(id, b.kind, Vec::new(), None, None);
        s.maintenance.jobs.insert(id,Job{kind,previous});
        return Ok(());
    }
    let (materials,labour) = cost(b,kind);
    let building_kind = b.kind;
    let i = s.work.places.iter().position(|p|p.building==id).ok_or_else(||bad("missing workplace"))?;
    // Builders already seated here (a dam's crew) stay on to do the refit.
    let crew = s.work.crew(id);
    let previous = s.work.places.remove(i);
    if let Err(e) = s.work.add_site_for(id,building_kind,&materials,labour,None) {
        s.work.places.push(previous);
        return Err(bad(&format!("{e:?}")));
    }
    s.work.place_mut(id).unwrap().priority = Priority::High;
    s.release_beavers(id);
    s.work.keep_crew(id,crew);
    s.maintenance.jobs.insert(id,Job{kind,previous});
    Ok(())
}

fn restore(s: &mut SimState, id: BuildingId, mut previous: Workplace) {
    let crew=s.work.crew(id);
    s.work.places.retain(|p|p.building!=id);
    previous.site=None;
    for slot in &mut previous.slots {slot.beaver=None;}
    s.release_beavers(id);
    s.work.places.push(previous);
    s.work.keep_crew(id,crew);
}

pub fn finish(s: &mut SimState, id: BuildingId) -> bool {
    if s.maintenance.jobs.get(&id).is_none_or(|j| j.kind == Refit::Repair) {return false;}
    if !s.work.place(id).and_then(|p|p.site.as_ref()).is_some_and(|p|p.is_done()) {return false;}
    let Some(job) = s.maintenance.jobs.remove(&id) else {return false};
    if let Some(b) = s.buildings.buildings.iter_mut().find(|b|b.id==id) {
        match job.kind {
            Refit::Reinforce => { b.reinforced=true; s.maintenance.reinforcements_completed+=1; }
            Refit::Repair | Refit::ReinforcedRepair => {}
            Refit::Facing => { b.pebbled = true; }
        }
    }
    if job.kind==Refit::Facing { crate::dams::refresh_durability(s,id); }
    restore(s,id,job.previous);
    true
}

pub fn cancel(s: &mut SimState, id: BuildingId) -> Result<Vec<Amount>, SimError> {
    let job = s.maintenance.jobs.remove(&id).ok_or_else(|| SimError::InvalidCommand("no maintenance queued".into()))?;
    if job.kind == Refit::Repair {
        s.agents.maint_targets.retain(|_, t| *t != id);
        crate::supply::tidy(s);
        return Ok(Vec::new());
    }
    let delivered = s.work.place(id).and_then(|p| p.site.as_ref()).map(|p| p.delivered.clone()).unwrap_or_default();
    let pos = s.buildings.building(id).map_or((0, 0), |b| b.pos);
    let refunds = delivered.into_iter().map(|(g, a)| (g, s.refund_or_drop(pos, g, a))).collect();
    restore(s, id, job.previous);
    Ok(refunds)
}

/// What wears a building down each day: the base rate for its kind, times the cold and a
/// flooding, divided by mud reinforcement and pebble facing. Every factor is 1 when it does
/// not apply. Dams wear cell by cell in the water model instead.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Wear { pub base: f32, pub cold: f32, pub flooded: f32, pub reinforced: f32, pub pebbled: f32 }

impl Wear {
    /// Condition lost a day, as a share of full.
    pub fn per_day(&self) -> f32 {
        self.base * self.cold * self.flooded / (self.reinforced * self.pebbled)
    }
}

pub fn wear(s: &SimState, b: &Building) -> Option<Wear> {
    if !b.is_complete() || b.kind.is_dam() {
        return None;
    }
    let cold=crate::weather::cold(s.water.sky.at(s.time_days).temp_c);
    Some(Wear {
        base: b.kind.spec().wear,
        cold: 1.0+COLD_WEAR_SCALE*cold,
        // Too deep under water, a building wears away fast.
        flooded: if crate::buildings::flooded(b,&s.water.depth) {FLOODED_WEAR_SCALE} else {1.0},
        reinforced: b.durability(),
        pebbled: if b.pebbled { lasts(b.kind, Refit::Facing) } else { 1.0 },
    })
}

pub fn step(s: &mut SimState, dt: f32) {
    let rates: Vec<Option<f32>> = s.buildings.buildings.iter().map(|b| wear(s, b).map(|w| w.per_day())).collect();
    for (b, rate) in s.buildings.buildings.iter_mut().zip(rates) {
        if let Some(rate) = rate {
            b.condition=(b.condition-rate*dt).max(0.0);
        }
    }
    // Worn to nothing, a building falls apart. Dams and canals keep their own upkeep.
    let wrecked: Vec<BuildingId> = s.buildings.buildings.iter()
        .filter(|b| b.is_complete() && b.dam.is_none() && b.canal.is_none() && !b.kind.is_dam() && b.condition <= 0.0)
        .map(|b| b.id).collect();
    for id in wrecked { s.wreck(id); }
    let depth=&s.water.depth;
    for b in &s.buildings.buildings {
        if !b.is_complete() {continue;}
        if let Some(home)=s.work.homes.get(&b.id).copied() {
            // A flooded lodge keeps no one warm.
            let drowned=crate::buildings::flooded(b,depth);
            if let Some(l)=s.colony.lodges.iter_mut().find(|l|l.id==home) {l.condition=if drowned {0.0} else {b.condition};}
        }
    }
}
