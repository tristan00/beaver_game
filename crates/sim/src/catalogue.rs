//! One definition per building, from `content/buildings.toml`.
pub use crate::content::Spec;

pub fn work_area(b: &crate::buildings::Building) -> Option<crate::work::Area> {
    let c = b.cells().next().unwrap_or(b.pos);
    b.kind.spec().area.map(|(w, h)| crate::work::Area {
        min: (c.0.saturating_sub(w / 2), c.1.saturating_sub(h / 2)),
        max: (c.0 + w / 2, c.1 + h / 2),
    })
}
