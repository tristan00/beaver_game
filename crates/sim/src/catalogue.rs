//! One definition per building: construction, staffing, storage and progression.
use crate::buildings::{Amount, BuildingKind, Good::*, Site};
use crate::resources::ResourceKind;
use crate::work::Role;

#[derive(Clone, Copy, Debug)]
pub struct Spec {
    pub name: &'static str,
    pub footprint: (usize, usize),
    pub site: Site,
    pub cost: &'static [Amount],
    pub work: f32,
    pub prerequisites: &'static [BuildingKind],
    pub slots: &'static [Role],
    pub area: Option<(usize, usize)>,
    pub capacity: f32,
}

impl Spec {
    const fn new(name: &'static str, footprint: (usize, usize), site: Site, cost: &'static [Amount], work: f32, prerequisites: &'static [BuildingKind]) -> Self {
        Self { name, footprint, site, cost, work, prerequisites, slots: &[], area: None, capacity: 0.0 }
    }

    const fn staffed(mut self, slots: &'static [Role]) -> Self {
        self.slots = slots;
        self
    }

    const fn area(mut self) -> Self {
        self.area = Some((31, 31));
        self
    }

    const fn storage(mut self, capacity: f32) -> Self {
        self.capacity = capacity;
        self
    }
}

impl BuildingKind {
    pub fn spec(self) -> Spec {
        use BuildingKind::*;
        match self {
            Dam => Spec::new("Dam", (3, 1), Site::TouchesWater, &[(Sticks, 30.0), (Clay, 4.0)], 7.0, &[]),
            Lodge => Spec::new("Lodge", (3, 3), Site::Pond, &[(Sticks, 36.0), (Clay, 16.0)], 10.0, &[LoggingCamp, MudPit]),
            Burrow => Spec::new("Burrow hideout", (1, 1), Site::TouchesWater, &[], 5.0, &[]),
            FoodCache => Spec::new("Food cache", (2, 2), Site::Pond, &[(Sticks, 12.0), (Clay, 5.0)], 3.5, &[ForagerHut])
                .staffed(&[Role::Haul]).storage(100.0),
            ScentMound => Spec::new("Scent mound", (1, 1), Site::TouchesWater, &[(Clay, 5.0), (Sticks, 3.0)], 1.5, &[Burrow])
                .staffed(&[Role::Scout]),
            GuardPost => Spec::new("Guard post", (1, 1), Site::Dry, &[(Sticks, 16.0), (Clay, 3.0)], 3.0, &[ScentMound])
                .staffed(&[Role::Guard; 2]),
            Canal => Spec::new("Canal", (1, 1), Site::TouchesWater, &[(Clay, 1.5)], 0.8, &[MudPit]),
            Lock => Spec::new("Lock", (2, 1), Site::TouchesWater, &[(Sticks, 22.0), (Clay, 10.0)], 6.0, &[Dock, MudPit]),
            WaterWheel => Spec::new("Water wheel", (2, 2), Site::Pond, &[(Sticks, 26.0), (Clay, 4.0)], 7.0, &[Workshop]),
            Workshop => Spec::new("Workshop", (3, 2), Site::Pond, &[(Sticks, 18.0), (Clay, 6.0)], 5.0, &[LoggingCamp, MudPit])
                .staffed(&[Role::Craft]),
            Warehouse => Spec::new("Warehouse", (3, 3), Site::Pond, &[(Sticks, 32.0), (Clay, 10.0)], 8.0, &[])
                .staffed(&[Role::Haul]).storage(150.0),
            Dock => Spec::new("Dock", (2, 1), Site::TouchesWater, &[(Sticks, 16.0), (Clay, 4.0)], 4.0, &[Warehouse])
                .staffed(&[Role::Haul]).storage(60.0),
            Barge => Spec::new("Barge", (1, 1), Site::WaterByDock, &[(Sticks, 24.0), (Clay, 4.0)], 5.0, &[Dock, Workshop]),
            LoggingCamp => Spec::new("Logging camp", (2, 2), Site::Pond, &[(Sticks, 8.0)], 2.0, &[Burrow])
                .staffed(&[Role::Collect(ResourceKind::Tree); 2]).area(),
            ForagerHut => Spec::new("Forager hut", (2, 2), Site::Pond, &[(Sticks, 6.0), (Clay, 3.0)], 2.0, &[Burrow])
                .staffed(&[Role::Collect(ResourceKind::Plant); 2]).area(),
            MudPit => Spec::new("Clay pit", (2, 2), Site::TouchesWater, &[(Sticks, 6.0)], 2.0, &[Burrow])
                .staffed(&[Role::Collect(ResourceKind::MudBank); 2]).area(),
            Mill => Spec::new("Mill", (3, 3), Site::Pond, &[(Sticks, 50.0), (Clay, 14.0)], 12.0, &[Workshop, WaterWheel])
                .staffed(&[Role::Craft; 2]),
            Storehouse => Spec::new("Storehouse", (4, 3), Site::Pond, &[(Sticks, 58.0), (Clay, 16.0)], 14.0, &[Warehouse, Workshop])
                .staffed(&[Role::Haul; 2]).storage(400.0),
            ForesterLodge => Spec::new("Forester's lodge", (2, 2), Site::Pond, &[(Sticks, 10.0), (Clay, 3.0)], 3.0, &[LoggingCamp])
                .staffed(&[Role::Forester; 2]).area(),
            TreeNursery => Spec::new("Tree nursery", (2, 2), Site::Dry, &[(Sticks, 12.0), (Clay, 6.0)], 4.0, &[ForesterLodge])
                .staffed(&[Role::Forester]),
        }
    }
}

pub fn work_area(b: &crate::buildings::Building) -> Option<crate::work::Area> {
    let c = b.cells().next().unwrap_or(b.pos);
    b.kind.spec().area.map(|(w, h)| crate::work::Area {
        min: (c.0.saturating_sub(w / 2), c.1.saturating_sub(h / 2)),
        max: (c.0 + w / 2, c.1 + h / 2),
    })
}

#[cfg(test)]
mod progression_tests {
    use super::*;
    #[test]
    fn every_building_can_be_reached_from_the_starting_buildings() {
        let mut built = Vec::new();
        for _ in 0..BuildingKind::ALL.len() {
            for k in BuildingKind::ALL {
                if !built.contains(&k) && k.spec().prerequisites.iter().all(|p| built.contains(p)) { built.push(k); }
            }
        }
        assert_eq!(built.len(), BuildingKind::ALL.len(), "building prerequisites contain a cycle");
        assert!(BuildingKind::Workshop.spec().prerequisites.contains(&BuildingKind::MudPit));
        assert!(BuildingKind::Barge.spec().prerequisites.contains(&BuildingKind::Dock));
    }
}
