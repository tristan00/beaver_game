//! Equipment is maintained locally by working crafters; it is not a stockpiled good.
use crate::{buildings::{Building, BuildingKind, Good, Use}, stock, SimState};

pub const COST_PER_DAY: &[(Good, f32)] = &[(Good::Sticks, 0.18), (Good::Clay, 0.10), (Good::Pebbles, 0.06)];
pub const REACH: usize = 40;

pub fn has_materials(sim: &SimState, building: &Building) -> bool {
    COST_PER_DAY.iter().all(|&(g, a)| stock::within_reach(sim, building, g) >= a * crate::STEP_DAYS)
}

pub fn support(sim: &SimState, cell: crate::water::Cell) -> f32 {
    sim.buildings.buildings.iter()
        .filter(|b| b.operational() && matches!(b.kind, BuildingKind::Workshop | BuildingKind::Mill)
            && stock::dist(cell, b) <= REACH && !sim.buildings.stranded(b, &sim.water.depth))
        .map(|b| b.output).fold(0.0, f32::max) * 0.25
}

pub fn work(sim: &mut SimState, id: crate::buildings::BuildingId, effort: f32) -> bool {
    let Some(b) = sim.buildings.building(id).cloned() else { return false };
    let mut work = effort.max(0.0);
    for &(g, a) in COST_PER_DAY {
        work = work.min(stock::within_reach(sim, &b, g).min(sim.buildings.stores.amount(g)) / a);
    }
    if work <= 0.0 { return false; }
    for &(g, a) in COST_PER_DAY {
        let used = stock::take_near(sim, &b, g, a * work);
        sim.buildings.stores.withdraw_for(g, used, Use::Crafting).expect("limited to available materials");
    }
    let power = 1.0 + sim.buildings.power.min(1.0);
    let output = if b.kind == BuildingKind::Mill { 0.9 } else { 0.6 };
    sim.buildings.buildings.iter_mut().find(|b| b.id == id).unwrap().output = (b.output + work * output * power).min(1.0);
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::buildings::{BuildingId, Status};
    #[test]
    fn equipment_consumes_only_local_materials_and_never_creates_a_fourth_good() {
        let mut s=SimState::new(1);
        let workshop=Building{id:BuildingId(1),kind:BuildingKind::Workshop,pos:(20,20),status:Status::Complete,output:0.0,condition:1.0,reinforced:false,dam_profile:vec![]};
        let store=Building{id:BuildingId(2),kind:BuildingKind::Warehouse,pos:(24,20),..workshop.clone()};
        s.buildings.buildings.extend([workshop,store]);
        assert!(!work(&mut s,BuildingId(1),1.0));
        for g in Good::ALL {s.buildings.stores.deposit(g,10.0).unwrap();s.work.stock_add(BuildingId(2),g,10.0);}
        assert!(work(&mut s,BuildingId(1),1.0));
        assert_eq!(Good::ALL.len(),3);
        for &(g,a) in COST_PER_DAY {
            assert!((s.buildings.stores.amount(g)-(10.0-a)).abs()<1e-5);
            assert!((s.work.stock_of(BuildingId(2),g)-(10.0-a)).abs()<1e-5);
            assert_eq!(s.buildings.stores.produced_total[g.index()],10.0);
        }
        assert!(s.buildings.building(BuildingId(1)).unwrap().output>0.0);
    }
}
