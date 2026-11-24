//! Repairs are mended on the spot by maintainers, who draw the materials from the stores as
//! they work; a repair order only marks a building to be mended first. Clay reinforcement and
//! pebble facing are refits: paid work through the normal material-delivery and labour system.
include!(concat!(env!("OUT_DIR"), "/maintenance_constants.rs"));
use std::collections::BTreeMap;
use crate::{buildings::{Amount, Building, BuildingId, BuildingKind as K, Good}, work::{Priority, Workplace}, SimError, SimState};

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Refit { Repair, Reinforce, Pebbles }

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

/// What mending costs for each whole unit of condition put back: the materials, drawn from the
/// stores as the work goes, and the labour.
pub fn mend_rates(b: &Building) -> (Vec<Amount>, f32) {
    let (w,h) = b.kind.spec().footprint;
    let mut materials: Vec<Amount> = b.kind.spec().cost.iter().map(|&(g,a)|(g,a*REPAIR_MATERIAL_SHARE)).filter(|v|v.1>0.0).collect();
    if materials.is_empty() { materials.push((Good::Sticks,REPAIR_MIN_STICKS)); }
    if b.reinforced { materials.push((Good::Clay,(w*h) as f32*REINFORCED_REPAIR_CLAY_PER_CELL)); }
    (materials, (b.kind.spec().work*REPAIR_LABOUR_SHARE).max(REPAIR_MIN_LABOUR))
}

/// Buildings maintainers mend: finished buildings (not dams or canals, which have their own
/// upkeep) that are worn below `AUTO_REPAIR_BELOW`, marked for repair, or already being mended,
/// until they are whole again.
pub fn needs_mending(s: &SimState, b: &Building) -> bool {
    b.is_complete() && b.dam.is_none() && b.canal.is_none() && b.condition < 1.0
        && (b.condition < AUTO_REPAIR_BELOW
            || s.maintenance.jobs.get(&b.id).is_some_and(|j| j.kind == Refit::Repair)
            || s.agents.maint_targets.values().any(|&t| t == b.id))
}

/// Put back `labour` worth of condition, drawing the materials for it from the stores.
/// Returns false when the stores are short of something it needs.
pub fn mend(s: &mut SimState, id: BuildingId, labour: f32) -> bool {
    let Some(b) = s.buildings.building(id) else { return false };
    let (materials, per_unit) = mend_rates(b);
    let gain = (labour / per_unit).min(1.0 - b.condition).max(0.0);
    if materials.iter().any(|&(g,a)| s.buildings.stores.amount(g) < a*gain) { return false; }
    for (g,a) in materials {
        let _ = s.buildings.stores.withdraw_for(g, a*gain, crate::buildings::Use::Repairs);
    }
    let Some(b) = s.buildings.buildings.iter_mut().find(|b|b.id==id) else { return false };
    b.condition += gain;
    if b.condition >= 1.0 - 1e-4 {
        b.condition = 1.0;
        s.maintenance.repairs_completed += 1;
        if s.maintenance.jobs.get(&id).is_some_and(|j| j.kind == Refit::Repair) { s.maintenance.jobs.remove(&id); }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    fn lodge()->Building {Building{id:BuildingId(1),kind:K::Shelter,pos:(25,25),status:crate::buildings::Status::Complete,output:0.0,condition:0.7,reinforced:false,dam: None, canal: None}}
    #[test]
    fn a_repair_order_only_marks_the_building_and_refits_are_seatless_sites() {
        let mut s=SimState::new(1);let b=lodge();let id=b.id;s.work.add_complete(&b).unwrap();s.buildings.buildings.push(b);
        order(&mut s,id,Refit::Repair).unwrap();
        assert!(s.work.place(id).unwrap().site.is_none(),"the building keeps working");
        assert!(needs_mending(&s,s.buildings.building(id).unwrap()),"marked, it is mended though above the automatic mark");
        assert!(cancel(&mut s,id).unwrap().is_empty());assert!(s.maintenance.jobs.is_empty());
        order(&mut s,id,Refit::Reinforce).unwrap();
        let p=s.work.place(id).unwrap();
        assert!(p.site.is_some() && p.slots.is_empty(),"a refit is a site with no seats");
    }
    #[test]
    fn a_building_worn_to_nothing_falls_apart_and_leaves_its_goods() {
        let mut s=SimState::new(1);
        let mut store=lodge();store.id=BuildingId(7);store.kind=K::Warehouse;store.pos=(40,25);store.condition=1e-6;
        s.work.add_complete(&store).unwrap();s.buildings.buildings.push(store);
        s.buildings.stores.inventory.put(BuildingId(7),Good::Sticks,None,12.0);
        step(&mut s,1.0);
        assert!(s.buildings.building(BuildingId(7)).is_none(),"gone");
        assert!(s.work.place(BuildingId(7)).is_none(),"no workplace left");
        assert!(s.buildings.wrecked.contains(&BuildingId(7)));
        assert_eq!(s.buildings.stores.amount(Good::Sticks),0.0,"no longer in store");
        let lying:f32=s.work.piles.items.iter().filter(|p|p.good==Good::Sticks).map(|p|p.amount).sum();
        assert!((lying-12.0).abs()<1e-3,"left lying where it stood: {lying}");
    }
    #[test]
    fn a_lodge_worn_to_nothing_leaves_its_family_without_it() {
        let mut s=SimState::new(1);
        let mut b=lodge();b.condition=1e-6;let id=b.id;s.work.add_complete(&b).unwrap();s.buildings.buildings.push(b);
        let home=s.colony.add_home((26.0,26.0));s.work.homes.insert(id,home);
        step(&mut s,1.0);
        assert!(s.buildings.building(id).is_none());
        assert!(!s.work.homes.contains_key(&id));
        assert!(s.colony.lodges.iter().all(|l|l.id!=home),"its household is gone");
        assert!(s.colony.beavers.iter().all(|b|b.lodge!=Some(home)));
    }
    #[test]
    fn mending_needs_the_materials_in_store() {
        let mut s=SimState::new(1);let b=lodge();let id=b.id;s.work.add_complete(&b).unwrap();s.buildings.buildings.push(b);
        assert!(!mend(&mut s,id,1.0),"nothing in store");
        assert_eq!(s.buildings.building(id).unwrap().condition,0.7);
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
        order(&mut s,id,Refit::Reinforce).unwrap();s.work.deliver(id,Good::Sticks,1.0).unwrap();let before=s.buildings.stores.amount(Good::Sticks);
        cancel(&mut s,id).unwrap();assert_eq!(s.buildings.stores.amount(Good::Sticks),before+1.0);
        assert_eq!(s.buildings.building(id).unwrap().condition,0.7);assert!(s.work.place(id).unwrap().site.is_none());
    }
}

pub fn order(s: &mut SimState, id: BuildingId, kind: Refit) -> Result<(), SimError> {
    let bad = |m: &str| SimError::InvalidCommand(m.into());
    let b = s.buildings.building(id).ok_or_else(||bad("no such building"))?;
    if s.maintenance.jobs.contains_key(&id) { return Err(bad("this building already has maintenance queued")); }
    if !b.is_complete() { return Err(bad("finish construction first")); }
    if b.kind==K::Dam && kind != Refit::Pebbles {return Err(bad("dams are kept up by maintainers cell by cell"));}
    if kind == Refit::Pebbles {
        if !can_pebble(b.kind) {return Err(bad("this structure cannot be faced with pebbles"));}
        if s.buildings.pebbled.contains(&id) {return Err(bad("already faced with pebbles"));}
    } else if kind == Refit::Reinforce {
        if !can_reinforce(b.kind) {return Err(bad("this structure cannot be clay-reinforced"));}
        if b.reinforced {return Err(bad("already clay-reinforced"));}
    } else if b.condition >= REPAIR_CONDITION_THRESHOLD {return Err(bad("this building does not need repairs"));}
    if kind == Refit::Repair {
        // Mended first by the maintainers; the building keeps working meanwhile.
        let previous = Workplace::new(id, b.kind, Vec::new(), None, None);
        s.maintenance.jobs.insert(id,Job{kind,previous});
        return Ok(());
    }
    let (materials,labour) = cost(b,kind);
    let building_kind = b.kind;
    let i = s.work.places.iter().position(|p|p.building==id).ok_or_else(||bad("missing workplace"))?;
    let previous = s.work.places.remove(i);
    if let Err(e) = s.work.add_site_for(id,building_kind,&materials,labour,None) {
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
    let crew=s.work.crew(id);
    s.work.places.retain(|p|p.building!=id);
    previous.site=None;
    for slot in &mut previous.slots {slot.worker=None;}
    release(s,id);
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
            Refit::Repair => {}
            Refit::Pebbles => { s.buildings.pebbled.insert(id); }
        }
    }
    if job.kind==Refit::Pebbles { crate::dams::refresh_durability(s,id); }
    restore(s,id,job.previous);
    true
}

pub fn cancel(s: &mut SimState, id: BuildingId) -> Result<Vec<Amount>, SimError> {
    let job=s.maintenance.jobs.remove(&id).ok_or_else(||SimError::InvalidCommand("no maintenance queued".into()))?;
    if job.kind == Refit::Repair { return Ok(Vec::new()); }
    let delivered=s.work.place(id).and_then(|p|p.site.as_ref()).map(|p|p.delivered.clone()).unwrap_or_default();
    let mut refunds=Vec::new();
    for (g,a) in delivered {let kept=s.buildings.stores.refund(g,a).map_err(|e|SimError::InvalidCommand(e.to_string()))?; refunds.push((g,kept));}
    restore(s,id,job.previous);
    Ok(refunds)
}

/// A save from before repairs were mended in place may hold a repair as a site: put the
/// building's workplace back and return what was delivered, keeping the repair as a mark.
fn unsite_old_repairs(s: &mut SimState) {
    let old: Vec<BuildingId> = s.maintenance.jobs.iter().filter(|(id,j)| j.kind == Refit::Repair && s.work.place(**id).is_some_and(|p| p.site.is_some())).map(|(id,_)| *id).collect();
    for id in old {
        let delivered = s.work.place(id).and_then(|p|p.site.as_ref()).map(|p|p.delivered.clone()).unwrap_or_default();
        for (g,a) in delivered { let _ = s.buildings.stores.refund(g,a); }
        let Some(job) = s.maintenance.jobs.get_mut(&id) else { continue };
        let kind = job.previous.kind;
        let previous = std::mem::replace(&mut job.previous, Workplace::new(id, kind, Vec::new(), None, None));
        restore(s,id,previous);
    }
}

/// What wears a building down each day: the base rate for its kind, times the cold and a
/// flooding, divided by clay reinforcement and pebble facing. Every factor is 1 when it does
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
    if !b.is_complete() || b.kind == K::Dam {
        return None;
    }
    let cold=crate::weather::cold(s.water.sky.at(s.time_days).temp_c);
    Some(Wear {
        base: if b.kind==K::Shelter {HOME_WEAR_PER_DAY} else {BUILDING_WEAR_PER_DAY},
        cold: 1.0+COLD_WEAR_SCALE*cold,
        // Too deep under water, a building wears away fast.
        flooded: if crate::buildings::flooded(b,&s.water.depth) {FLOODED_WEAR_SCALE} else {1.0},
        reinforced: b.durability(),
        pebbled: if s.buildings.pebbled.contains(&b.id) {crate::buildings::PEBBLE_FACTOR} else {1.0},
    })
}

pub fn step(s: &mut SimState, dt: f32) {
    unsite_old_repairs(s);
    let rates: Vec<Option<f32>> = s.buildings.buildings.iter().map(|b| wear(s, b).map(|w| w.per_day())).collect();
    for (b, rate) in s.buildings.buildings.iter_mut().zip(rates) {
        if let Some(rate) = rate {
            b.condition=(b.condition-rate*dt).max(0.0);
        }
    }
    // Worn to nothing, a building falls apart. Dams and canals keep their own upkeep.
    let wrecked: Vec<BuildingId> = s.buildings.buildings.iter()
        .filter(|b| b.is_complete() && b.dam.is_none() && b.canal.is_none() && b.kind != K::Dam && b.condition <= 0.0)
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
