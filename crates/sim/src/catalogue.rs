use crate::buildings::BuildingKind;
use crate::features::System;
use crate::resources::ResourceKind;
use crate::tuning::buildings::{batches_per_day, store_capacity};
use crate::work::Role;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Tier {
    Elementary,
    Industrial,
}

#[derive(Clone, Debug)]
pub struct Def {
    pub kind: BuildingKind,
    pub tier: Tier,
    pub slots: Vec<Role>,
    pub area: Option<(usize, usize)>,
    pub system: Option<System>,
    pub capacity: f32,
    pub batches_per_day: f32,
}

impl Def {
    pub fn batches_per_worker(&self) -> f32 {
        if self.slots.is_empty() { 0.0 } else { self.batches_per_day / self.slots.len() as f32 }
    }
}

const GATHER_AREA: (usize, usize) = (31, 31);

fn camp(kind: ResourceKind) -> (Tier, Vec<Role>, Option<(usize, usize)>) {
    (Tier::Elementary, vec![Role::Collect(kind); 2], Some(GATHER_AREA))
}

pub fn def(kind: BuildingKind) -> Def {
    use BuildingKind::*;
    let passive = (Tier::Elementary, vec![], None);
    let (tier, slots, area) = match kind {
        Dam | Lodge | Burrow | Canal | Lock | WaterWheel | Barge => passive,
        ScentMound => (Tier::Elementary, vec![Role::Scout], None),
        GuardPost => (Tier::Elementary, vec![Role::Guard; 2], None),
        FoodCache | Warehouse | Dock => (Tier::Elementary, vec![Role::Haul], None),
        Workshop => (Tier::Elementary, vec![Role::Craft], None),
        LoggingCamp => camp(ResourceKind::Tree),
        ForagerHut => camp(ResourceKind::Plant),
        MudPit => camp(ResourceKind::MudBank),
        ForesterLodge => (Tier::Elementary, vec![Role::Forester; 2], Some(GATHER_AREA)),
        TreeNursery => (Tier::Elementary, vec![Role::Forester], None),
        Mill => (Tier::Industrial, vec![Role::Craft; 2], None),
        Storehouse => (Tier::Industrial, vec![Role::Haul; 2], None),
    };
    Def {
        kind,
        tier,
        slots,
        area,
        system: System::of_building(kind),
        capacity: store_capacity(kind),
        batches_per_day: batches_per_day(kind),
    }
}

pub fn work_area(b: &crate::buildings::Building) -> Option<crate::work::Area> {
    let c = b.cells().next().unwrap_or(b.pos);
    def(b.kind).area.map(|(w, h)| crate::work::Area {
        min: (c.0.saturating_sub(w / 2), c.1.saturating_sub(h / 2)),
        max: (c.0 + w / 2, c.1 + h / 2),
    })
}

pub fn prerequisites(kind: BuildingKind) -> &'static [BuildingKind] {
    use BuildingKind::*;
    match kind {
        Dam | Burrow | Warehouse => &[],
        ForagerHut | LoggingCamp | MudPit | ScentMound => &[Burrow],
        Lodge => &[LoggingCamp, MudPit],
        FoodCache => &[ForagerHut],
        Dock => &[Warehouse],
        Workshop => &[LoggingCamp, MudPit],
        Canal => &[MudPit],
        Lock => &[Dock, MudPit],
        Barge => &[Dock, Workshop],
        WaterWheel => &[Workshop],
        Storehouse => &[Warehouse, Workshop],
        Mill => &[Workshop, WaterWheel],
        ForesterLodge => &[LoggingCamp],
        TreeNursery => &[ForesterLodge],
        GuardPost => &[ScentMound],
    }
}

pub fn available(sim: &crate::SimState) -> Vec<BuildingKind> {
    BuildingKind::ALL
        .iter()
        .copied()
        .filter(|k| def(*k).system.is_none_or(|s| sim.features.enabled(s)))
        .collect()
}

#[cfg(test)]
mod progression_tests {
    use super::*;
    #[test]
    fn every_building_can_be_reached_from_the_starting_buildings() {
        let mut built = Vec::new();
        for _ in 0..BuildingKind::ALL.len() {
            for k in BuildingKind::ALL {
                if !built.contains(&k) && prerequisites(k).iter().all(|p| built.contains(p)) { built.push(k); }
            }
        }
        assert_eq!(built.len(), BuildingKind::ALL.len(), "building prerequisites contain a cycle");
        assert!(prerequisites(BuildingKind::Workshop).contains(&BuildingKind::MudPit));
        assert!(prerequisites(BuildingKind::Barge).contains(&BuildingKind::Dock));
    }
}
