//! One definition per building: construction, staffing, storage and progression.
use crate::buildings::{Amount, BuildingKind, Good, Site};
use crate::resources::ResourceKind;
use crate::work::Role;

#[derive(Clone, Copy, Debug)]
pub struct Spec {
    pub name: &'static str,
    pub footprint: (usize, usize),
    pub site: Site,
    pub cost: &'static [Amount],
    pub work: f32,
    pub slots: &'static [Role],
    pub area: Option<(usize, usize)>,
    pub capacity: f32,
    /// A building that must be finished before this one can be placed.
    pub requires: Option<BuildingKind>,
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
