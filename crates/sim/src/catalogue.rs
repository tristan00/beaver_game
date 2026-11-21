//! One definition per building: construction, staffing, storage and progression.
use crate::buildings::{Amount, BuildingKind, Good, Site};
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

include!(concat!(env!("OUT_DIR"), "/catalogue.rs"));

impl BuildingKind {
    pub fn spec(self) -> Spec { SPECS[self as usize] }
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
    }
}
