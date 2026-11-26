//! One definition per building, from `content/buildings.toml`.
pub use crate::content::Spec;

pub fn work_area_at(kind: crate::buildings::BuildingKind, c: (usize, usize)) -> Option<crate::work::Area> {
    kind.spec().area.map(|(w, h)| crate::work::Area {
        min: (c.0.saturating_sub(w / 2), c.1.saturating_sub(h / 2)),
        max: (c.0 + w / 2, c.1 + h / 2),
    })
}

pub fn work_area(b: &crate::buildings::Building) -> Option<crate::work::Area> {
    work_area_at(b.kind, b.cells().next().unwrap_or(b.pos))
}
