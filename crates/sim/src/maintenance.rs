//! Paid repairs and clay reinforcement use the normal material-delivery and labour system.
include!(concat!(env!("OUT_DIR"), "/maintenance_constants.rs"));
use std::collections::BTreeMap;
use crate::{buildings::{Amount, Building, BuildingId, BuildingKind as K, Good}, work::{Priority, Workplace}, SimError, SimState};

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Refit { Repair, Reinforce, Pebbles }

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Job { pub kind: Refit, previous: Workplace }

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct Maintenance {
    pub jobs: BTreeMap<BuildingId, Job>,
    pub repairs_completed: u32,
    pub reinforcements_completed: u32,
}

pub fn can_reinforce(kind: K) -> bool {
    matches!(kind, K::Shelter | K::Warehouse | K::Larder)
}

/// Dams and buildings can be faced with pebbles; canals cannot.
pub fn can_pebble(kind: K) -> bool {
    matches!(kind, K::Dam | K::Shelter | K::Warehouse | K::Larder)
}

pub fn cost(b: &Building, kind: Refit) -> (Vec<Amount>, f32) {
    let (w,h) = b.kind.spec().footprint;
    let area = (w*h) as f32;
    match kind {
        Refit::Pebbles if b.kind == K::Dam => {
            let n = b.dam.as_ref().map_or(1, |d| d.cells.len().max(1)) as f32;
            (vec![(Good::Pebbles,(n*PEBBLE_DAM_PER_CELL).ceil())],(n*PEBBLE_DAM_LABOUR_PER_CELL).max(1.0))
        }
        Refit::Pebbles => (vec![(Good::Pebbles,(area*PEBBLE_PER_CELL).ceil())],area*PEBBLE_LABOUR_PER_CELL),
        Refit::Reinforce if b.kind == K::Shelter => (vec![(Good::Sticks,LODGE_REINFORCE_STICKS),(Good::Clay,LODGE_REINFORCE_CLAY)],LODGE_REINFORCE_LABOUR),

        Refit::Reinforce => (vec![(Good::Sticks,REINFORCE_STICKS_PER_CELL*area),(Good::Clay,(area/REINFORCE_CLAY_AREA_DIVISOR).ceil())],REINFORCE_BASE_LABOUR+area*REINFORCE_LABOUR_PER_CELL),
        Refit::Repair => {
            let damage = (1.0-b.condition).clamp(0.0,1.0);
            let mut materials: Vec<_> = b.kind.spec().cost.iter().map(|&(g,a)|(g,(a*damage*REPAIR_MATERIAL_SHARE).ceil())).filter(|v|v.1>0.0).collect();
            if materials.is_empty() { materials.push((Good::Sticks,REPAIR_MIN_STICKS)); }
            if b.reinforced { materials.push((Good::Clay,(area*damage*REINFORCED_REPAIR_CLAY_PER_CELL).ceil().max(1.0))); }
            (materials, (b.kind.spec().work*damage*REPAIR_LABOUR_SHARE).max(REPAIR_MIN_LABOUR))
        }
    }
}

pub fn quoted_cost(_s: &SimState, b: &Building, kind: Refit) -> (Vec<Amount>,f32) {
    cost(b,kind)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn lodge()->Building {Building{id:BuildingId(1),kind:K::Shelter,pos:(25,25),status:crate::buildings::Status::Complete,output:0.0,condition:0.7,reinforced:false,dam: None, canal: None}}
    #[test]
    fn repairs_and_refits_are_maintainers_work() {
        let mut s=SimState::new(1);let b=lodge();let id=b.id;s.work.add_complete(&b).unwrap();s.buildings.buildings.push(b);
        order(&mut s,id,Refit::Repair).unwrap();
        let slots=&s.work.place(id).unwrap().slots;
        assert!(!slots.is_empty() && slots.iter().all(|sl|sl.role==crate::work::Role::Maintain));
        assert_eq!(crate::jobs::role_kind(crate::work::Role::Maintain),crate::jobs::RoleKind::Maintainer);
    }
    #[test]
    fn reinforcement_requires_materials_and_labour() {
        let mut s=SimState::new(1);let b=lodge();let id=b.id;s.work.add_complete(&b).unwrap();s.buildings.buildings.push(b);
        order(&mut s,id,Refit::Reinforce).unwrap();assert!(order(&mut s,id,Refit::Reinforce).is_err());
        assert!(!finish(&mut s,id));
        for (g,a) in s.work.place(id).unwrap().site.as_ref().unwrap().needed.clone(){s.work.deliver(id,g,a).unwrap();}
        assert!(!finish(&mut s,id));s.work.add_labour(id,100.0).unwrap();assert!(finish(&mut s,id));
        let b=s.buildings.building(id).unwrap();assert!(b.reinforced);assert_eq!(b.condition,0.7);assert!(s.work.place(id).unwrap().site.is_none());
    }
    #[test]
    fn pebble_facing_needs_pebbles_and_halves_wear() {
        let mut s=SimState::new(1);let b=lodge();let id=b.id;s.work.add_complete(&b).unwrap();s.buildings.buildings.push(b);
        let mut plain=lodge();plain.id=BuildingId(2);plain.pos=(40,25);s.work.add_complete(&plain).unwrap();s.buildings.buildings.push(plain);
        order(&mut s,id,Refit::Pebbles).unwrap();
        let needed=s.work.place(id).unwrap().site.as_ref().unwrap().needed.clone();
        assert!(needed.iter().any(|n|n.0==Good::Pebbles && n.1>0.0));
        for (g,a) in needed {s.work.deliver(id,g,a).unwrap();}
        s.work.add_labour(id,100.0).unwrap();assert!(finish(&mut s,id));
        assert!(s.buildings.pebbled.contains(&id));assert!(order(&mut s,id,Refit::Pebbles).is_err());
        step(&mut s,10.0);
        let wear=|s:&SimState,i:u64|0.7-s.buildings.building(BuildingId(i)).unwrap().condition;
        assert!((wear(&s,2)/wear(&s,1)-crate::buildings::PEBBLE_FACTOR).abs()<1e-3);
    }
    #[test]
    fn cancelling_refit_keeps_structure_and_refunds_deliveries() {
        let mut s=SimState::new(1);let b=lodge();let id=b.id;s.work.add_complete(&b).unwrap();s.buildings.buildings.push(b);
        order(&mut s,id,Refit::Repair).unwrap();s.work.deliver(id,Good::Sticks,1.0).unwrap();let before=s.buildings.stores.amount(Good::Sticks);
        cancel(&mut s,id).unwrap();assert_eq!(s.buildings.stores.amount(Good::Sticks),before+1.0);
        assert_eq!(s.buildings.building(id).unwrap().condition,0.7);assert!(s.work.place(id).unwrap().site.is_none());
    }
}

pub fn order(s: &mut SimState, id: BuildingId, kind: Refit) -> Result<(), SimError> {
    let bad = |m: &str| SimError::InvalidCommand(m.into());
    let b = s.buildings.building(id).ok_or_else(||bad("no such building"))?;
    if s.maintenance.jobs.contains_key(&id) { return Err(bad("this building already has maintenance queued")); }
    if !b.is_complete() { return Err(bad("finish construction first")); }
    if b.kind==K::Dam && kind != Refit::Pebbles {return Err(bad("dams are maintained by their own builders"));}
    if kind == Refit::Pebbles {
        if !can_pebble(b.kind) {return Err(bad("this structure cannot be faced with pebbles"));}
        if s.buildings.pebbled.contains(&id) {return Err(bad("already faced with pebbles"));}
    } else if kind == Refit::Reinforce {
        if !can_reinforce(b.kind) {return Err(bad("this structure cannot be clay-reinforced"));}
        if b.reinforced {return Err(bad("already clay-reinforced"));}
    } else if b.condition >= REPAIR_CONDITION_THRESHOLD {return Err(bad("this building does not need repairs"));}
    let (materials,labour) = cost(b,kind);
    let building_kind = b.kind;
    let i = s.work.places.iter().position(|p|p.building==id).ok_or_else(||bad("missing workplace"))?;
    let previous = s.work.places.remove(i);
    if let Err(e) = s.work.add_site_for(id,building_kind,&materials,labour,crate::work::Role::Maintain) {
        s.work.places.push(previous);
        return Err(bad(&format!("{e:?}")));
    }
    s.work.place_mut(id).unwrap().priority = Priority::High;
    release(s,id);
    s.maintenance.jobs.insert(id,Job{kind,previous});
    Ok(())
}

fn release(s: &mut SimState, id: BuildingId) {
    for a in s.agents.by_id.values_mut() {
        if matches!(a.assignment,Some(crate::agents::Assignment::Slot(b,_)) if b==id) {
            a.assignment=None;
        }
    }
}

fn restore(s: &mut SimState, id: BuildingId, mut previous: Workplace) {
    s.work.places.retain(|p|p.building!=id);
    previous.site=None;
    for slot in &mut previous.slots {slot.worker=None;}
    release(s,id);
    s.work.places.push(previous);
}

pub fn finish(s: &mut SimState, id: BuildingId) -> bool {
    if !s.work.place(id).and_then(|p|p.site.as_ref()).is_some_and(|p|p.is_done()) {return false;}
    let Some(job) = s.maintenance.jobs.remove(&id) else {return false};
    if let Some(b) = s.buildings.buildings.iter_mut().find(|b|b.id==id) {
        match job.kind {
            Refit::Reinforce => { b.reinforced=true; s.maintenance.reinforcements_completed+=1; }
            Refit::Repair => { b.condition=1.0; s.maintenance.repairs_completed+=1; }
            Refit::Pebbles => { s.buildings.pebbled.insert(id); }
        }
    }
    if job.kind==Refit::Pebbles { crate::dams::refresh_durability(s,id); }
    restore(s,id,job.previous);
    true
}

pub fn cancel(s: &mut SimState, id: BuildingId) -> Result<Vec<Amount>, SimError> {
    let job=s.maintenance.jobs.remove(&id).ok_or_else(||SimError::InvalidCommand("no maintenance queued".into()))?;
    let delivered=s.work.place(id).and_then(|p|p.site.as_ref()).map(|p|p.delivered.clone()).unwrap_or_default();
    let mut refunds=Vec::new();
    for (g,a) in delivered {let kept=s.buildings.stores.refund(g,a).map_err(|e|SimError::InvalidCommand(e.to_string()))?; refunds.push((g,kept));}
    restore(s,id,job.previous);
    Ok(refunds)
}

pub fn step(s: &mut SimState, dt: f32) {
    let temp=(std::f64::consts::TAU*(s.time_days-crate::tuning::colony::MIDSUMMER_DAY)/crate::calendar::YEAR_DAYS).cos() as f32;
    let cold=((crate::tuning::colony::COLD_BELOW_TEMP-temp)/crate::tuning::colony::COLD_RANGE).clamp(0.0,1.0);
    let pebbled=s.buildings.pebbled.clone();
    let depth=&s.water.depth;
    for b in &mut s.buildings.buildings {
        if !b.is_complete() {continue;}
        let drowned=crate::buildings::flooded(b,depth);
        if b.kind!=K::Dam {
            let wear=if b.kind==K::Shelter {HOME_WEAR_PER_DAY} else {BUILDING_WEAR_PER_DAY};
            // Too deep under water, a building wears away fast.
            let loss=wear*(1.0+COLD_WEAR_SCALE*cold)*if drowned {FLOODED_WEAR_SCALE} else {1.0};
            let faced=if pebbled.contains(&b.id) {crate::buildings::PEBBLE_FACTOR} else {1.0};
            b.condition=(b.condition-loss*dt/(b.durability()*faced)).max(0.0);
        }
        if let Some(home)=s.work.homes.get(&b.id).copied() {
            // A flooded lodge keeps no one warm.
            if let Some(l)=s.colony.lodges.iter_mut().find(|l|l.id==home) {l.condition=if drowned {0.0} else {b.condition};}
        }
    }
}
