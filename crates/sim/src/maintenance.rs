//! Paid repairs and stone reinforcement use the normal material-delivery and labour system.
use std::collections::BTreeMap;
use crate::{buildings::{Amount, Building, BuildingId, BuildingKind as K, Good}, work::{Priority, Workplace}, SimError, SimState};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refit { Repair, Reinforce }

#[derive(Clone, Debug)]
pub struct Job { pub kind: Refit, previous: Workplace }

#[derive(Clone, Debug, Default)]
pub struct Maintenance {
    pub jobs: BTreeMap<BuildingId, Job>,
    pub repairs_completed: u32,
    pub reinforcements_completed: u32,
}

pub fn can_reinforce(kind: K) -> bool {
    matches!(kind, K::Lodge | K::Dam | K::Lock | K::Dock | K::WaterWheel | K::Workshop | K::Warehouse | K::Storehouse | K::Mill
        | K::LoggingCamp | K::ForagerHut | K::ForesterLodge)
}

pub fn cost(b: &Building, kind: Refit) -> (Vec<Amount>, f32) {
    let (w,h) = b.kind.spec().footprint;
    let area = (w*h) as f32;
    match kind {
        Refit::Reinforce if b.kind == K::Lodge => (vec![(Good::Pebbles,18.0),(Good::Clay,8.0)],8.0),
        Refit::Reinforce if b.kind == K::Dam => {
            let n = b.dam_profile.len().max(1) as f32;
            (vec![(Good::Pebbles,2.0*n),(Good::Clay,(n*0.5).ceil())],n*0.25+2.0)
        }
        Refit::Reinforce => (vec![(Good::Pebbles,2.0*area),(Good::Clay,(area/4.0).ceil())],4.0+area*0.4),
        Refit::Repair => {
            let damage = (1.0-b.condition).clamp(0.0,1.0);
            let mut materials: Vec<_> = b.kind.spec().cost.iter().map(|&(g,a)|(g,(a*damage*0.35).ceil())).filter(|v|v.1>0.0).collect();
            if materials.is_empty() { materials.push((Good::Sticks,1.0)); }
            if b.reinforced { materials.push((Good::Pebbles,(area*damage*0.6).ceil().max(1.0))); }
            (materials, (b.kind.spec().work*damage*0.5).max(1.5))
        }
    }
}

pub fn quoted_cost(s: &SimState, b: &Building, kind: Refit) -> (Vec<Amount>,f32) {
    if b.kind!=K::Dam || kind!=Refit::Repair {return cost(b,kind);}
    let (cells,heights):(Vec<_>,Vec<_>)=b.dam_profile.iter().copied().unzip();
    let (mut materials,labour)=crate::dam_preview::cost_for(s,&cells,&heights);
    if b.reinforced {
        let damage=cells.iter().map(|&(x,y)|1.0-s.water.dam_integrity.get(x,y).copied().unwrap_or(0.0)).sum::<f32>();
        materials.push((Good::Pebbles,(damage*1.2).ceil().max(1.0)));
    }
    (materials,labour)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn lodge()->Building {Building{id:BuildingId(1),kind:K::Lodge,pos:(25,25),status:crate::buildings::Status::Complete,output:0.0,condition:0.7,reinforced:false,dam_profile:Vec::new()}}
    #[test]
    fn reinforcement_requires_quarry_materials_and_labour() {
        let mut s=SimState::new(1);let b=lodge();let id=b.id;s.work.add_complete(&b).unwrap();s.buildings.buildings.push(b);
        assert!(order(&mut s,id,Refit::Reinforce).is_err());s.buildings.buildings.push(Building{id:BuildingId(2),kind:K::QuarryCamp,..lodge()});order(&mut s,id,Refit::Reinforce).unwrap();
        assert!(!finish(&mut s,id));
        for (g,a) in s.work.place(id).unwrap().site.as_ref().unwrap().needed.clone(){s.work.deliver(id,g,a).unwrap();}
        assert!(!finish(&mut s,id));s.work.add_labour(id,100.0).unwrap();assert!(finish(&mut s,id));
        let b=s.buildings.building(id).unwrap();assert!(b.reinforced);assert_eq!(b.condition,0.7);assert!(s.work.place(id).unwrap().site.is_none());
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
    if matches!(b.kind,K::Canal|K::Barge) {return Err(bad("this structure does not use building repairs"));}
    if kind == Refit::Reinforce {
        if !can_reinforce(b.kind) {return Err(bad("this structure cannot be stone-reinforced"));}
        if b.reinforced {return Err(bad("already stone-reinforced"));}
        if !s.has_building(K::QuarryCamp) {return Err(bad("finish a quarry camp first"));}
    } else {
        if b.kind == K::Dam {
            let cell = b.dam_profile.first().map(|v|v.0).unwrap_or(b.pos);
            return s.repair_dam(cell).map(|_|());
        }
        if b.condition >= 0.98 {return Err(bad("this building does not need repairs"));}
    }
    let (materials,labour) = cost(b,kind);
    let building_kind = b.kind;
    let i = s.work.places.iter().position(|p|p.building==id).ok_or_else(||bad("missing workplace"))?;
    let previous = s.work.places.remove(i);
    if let Err(e) = s.work.add_site(id,building_kind,&materials,labour) {
        s.work.places.push(previous);
        return Err(bad(&format!("{e:?}")));
    }
    s.work.place_mut(id).unwrap().priority = Priority::High;
    release(s,id);
    s.maintenance.jobs.insert(id,Job{kind,previous});
    Ok(())
}

fn release(s: &mut SimState, id: BuildingId) {
    s.work.managed.retain(|_,d|!matches!(d,crate::work::ManagedDuty::Slot(b,_) if *b==id));
    for a in s.agents.by_id.values_mut() {
        if matches!(a.assignment,Some(crate::agents::Assignment::Slot(b,_)) if b==id) {
            a.assignment=None;
            s.work.player_orders.remove(&a.id);
        }
    }
}

fn restore(s: &mut SimState, id: BuildingId, mut previous: Workplace) {
    s.work.places.retain(|p|p.building!=id);
    previous.site=None;
    for slot in &mut previous.slots {slot.worker=None;slot.locked=false;}
    release(s,id);
    s.work.places.push(previous);
}

pub fn finish(s: &mut SimState, id: BuildingId) -> bool {
    if !s.work.place(id).and_then(|p|p.site.as_ref()).is_some_and(|p|p.is_done()) {return false;}
    let Some(job) = s.maintenance.jobs.remove(&id) else {return false};
    if let Some(b) = s.buildings.buildings.iter_mut().find(|b|b.id==id) {
        if job.kind==Refit::Reinforce {
            b.reinforced=true;
            for &(c,_) in &b.dam_profile {if let Some(v)=s.water.dam_durability.get_mut(c.0,c.1) {*v=b.durability();}}
            s.maintenance.reinforcements_completed+=1;
        } else {b.condition=1.0; s.maintenance.repairs_completed+=1;}
    }
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
    let temp=(std::f64::consts::TAU*(s.time_days-135.0)/crate::calendar::YEAR_DAYS).cos() as f32;
    let cold=((0.2-temp)/1.2).clamp(0.0,1.0);
    for b in &mut s.buildings.buildings {
        if !b.is_complete() {continue;}
        if b.kind==K::Dam {
            if !b.dam_profile.is_empty() {
                b.condition=b.dam_profile.iter().map(|&(c,_)|s.water.dam_integrity.get(c.0,c.1).copied().unwrap_or(0.0)).fold(1.0,f32::min);
            }
        } else if !matches!(b.kind,K::Canal|K::Barge) {
            let wet=b.cells().filter(|&(x,y)|s.water.depth.get(x,y).is_some_and(|&d|d>=0.02)).count();
            let (w,h)=b.kind.spec().footprint;
            let dry=b.kind.spec().site==crate::buildings::Site::Pond && wet<=w*h/2;
            let wear=if matches!(b.kind,K::Lodge|K::Burrow) {0.001} else {0.00065};
            let loss=wear*(1.0+0.8*cold)+if dry{0.008}else{0.0};
            b.condition=(b.condition-loss*dt/b.durability()).max(0.0);
        }
        if let Some(home)=s.work.homes.get(&b.id).copied() {
            if let Some(l)=s.colony.lodges.iter_mut().find(|l|l.id==home) {l.condition=b.condition;}
        }
    }
}
