//! `--cam lake`: an opt-in camera that centres and zooms on the dam and the lake it holds, so canals and
//! buildings are legible. It frames the start's dam and lake from the real camera projection, and a
//! `--screenshot` run prints where the dam and lake land in the window (pixel boxes from the same projection).
use crate::camera::OrbitCamera;
use crate::flow::cell_world;
use bevy::camera::{CameraProjection, PerspectiveProjection, RenderTargetInfo};
use bevy::prelude::*;
use sim::grid::HeightGrid;
use sim::lakes::LakeInfo;

/// The dam and lake of the start being played (start 2 or 3); absent for start 1 and the menu.
#[derive(Resource, Default, Clone, Debug)]
pub struct LakeView(pub Option<LakeCells>);

#[derive(Clone, Debug)]
pub struct LakeCells {
    pub dam: Vec<(usize, usize)>,
    pub dam_height: f32,
    pub lake: Vec<(usize, usize)>,
    pub level: f32,
}

impl From<&LakeInfo> for LakeCells {
    fn from(i: &LakeInfo) -> Self {
        LakeCells { dam: i.dam.cells.clone(), dam_height: i.dam.height, lake: i.lake.clone(), level: i.level }
    }
}

/// A pixel box in window coordinates (origin top-left).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PxBox {
    pub min: Vec2,
    pub max: Vec2,
}

impl PxBox {
    pub fn size(&self) -> Vec2 {
        self.max - self.min
    }
    /// Lies fully within a window of `size`.
    pub fn inside(&self, size: Vec2) -> bool {
        self.min.x >= 0.0 && self.min.y >= 0.0 && self.max.x <= size.x && self.max.y <= size.y
    }
    /// Share of the window the box covers.
    pub fn coverage(&self, size: Vec2) -> f32 {
        let s = self.size();
        s.x * s.y / (size.x * size.y)
    }
}

/// A camera with the projection the game's `Camera3d` gets, for a window of `size`.
pub fn camera_for(size: Vec2) -> Camera {
    let mut c = Camera::default();
    c.computed.clip_from_view = PerspectiveProjection { aspect_ratio: size.x / size.y, ..default() }.get_clip_from_view();
    c.computed.target_info = Some(RenderTargetInfo { physical_size: size.as_uvec2(), scale_factor: 1.0 });
    c
}

/// Window pixel box of world `points` through `camera`; `None` if any point is not drawable.
pub fn project_box(camera: &Camera, at: &GlobalTransform, points: &[Vec3]) -> Option<PxBox> {
    let mut b: Option<PxBox> = None;
    for &p in points {
        let px = camera.world_to_viewport(at, p).ok()?;
        b = Some(match b {
            None => PxBox { min: px, max: px },
            Some(b) => PxBox { min: b.min.min(px), max: b.max.max(px) },
        });
    }
    b
}

/// The four corners of each cell, lifted to `y(corner terrain height)`.
pub fn cell_corners(h: &HeightGrid, cells: &[(usize, usize)], y: impl Fn(f32) -> f32) -> Vec<Vec3> {
    let mut out = Vec::with_capacity(cells.len() * 4);
    for &(cx, cy) in cells {
        for (dx, dy) in [(0.0, 0.0), (1.0, 0.0), (0.0, 1.0), (1.0, 1.0)] {
            let mut p = cell_world(h, (cx as f32 + dx, cy as f32 + dy));
            p.y = y(p.y);
            out.push(p);
        }
    }
    out
}

impl LakeCells {
    pub fn dam_points(&self, h: &HeightGrid) -> Vec<Vec3> {
        cell_corners(h, &self.dam, |t| t + self.dam_height)
    }
    pub fn lake_points(&self, h: &HeightGrid) -> Vec<Vec3> {
        cell_corners(h, &self.lake, |_| self.level)
    }
    /// Dam and lake boxes through `camera`.
    pub fn boxes(&self, h: &HeightGrid, camera: &Camera, at: &GlobalTransform) -> Option<(PxBox, PxBox)> {
        Some((project_box(camera, at, &self.dam_points(h))?, project_box(camera, at, &self.lake_points(h))?))
    }
}

/// Fraction of the window left empty on each side of the framed dam and lake.
const MARGIN: f32 = 0.04;

/// Centres and zooms `base` (angles kept) so the dam and lake fill the window of `size`, with a small margin.
pub fn frame(base: OrbitCamera, cells: &LakeCells, h: &HeightGrid, size: Vec2) -> OrbitCamera {
    let camera = camera_for(size);
    let mut pts = cells.dam_points(h);
    pts.extend(cells.lake_points(h));
    let (lo, hi) = pts.iter().fold((Vec3::MAX, Vec3::MIN), |(lo, hi), p| (lo.min(*p), hi.max(*p)));
    let mut orbit = base;
    orbit.target = Vec3::new((lo.x + hi.x) / 2.0, cells.level, (lo.z + hi.z) / 2.0);
    let avail = size * (1.0 - 2.0 * MARGIN);
    let fits = |o: &OrbitCamera| {
        let at = GlobalTransform::from(o.transform());
        project_box(&camera, &at, &pts).is_some_and(|b| b.size().x <= avail.x && b.size().y <= avail.y)
    };
    for _ in 0..4 {
        // the closest distance at which the dam and lake still fit (nearer points leave the window or the view)
        let (mut near, mut far) = (OrbitCamera::MIN_DISTANCE, OrbitCamera::MAX_DISTANCE);
        orbit.distance = far;
        if !fits(&orbit) {
            break;
        }
        for _ in 0..30 {
            orbit.distance = (near + far) / 2.0;
            if fits(&orbit) {
                far = orbit.distance;
            } else {
                near = orbit.distance;
            }
        }
        orbit.distance = far;
        // recentre: bring the middle of the box to the middle of the window
        let at = GlobalTransform::from(orbit.transform());
        let Some(b) = project_box(&camera, &at, &pts) else { break };
        let off = size / 2.0 - (b.min + b.max) / 2.0;
        if let Ok(ray) = camera.viewport_to_world(&at, size / 2.0 - off) {
            if let Some(t) = ray.intersect_plane(orbit.target, InfinitePlane3d::new(Vec3::Y)) {
                orbit.target = ray.get_point(t);
            }
        }
    }
    orbit.clamp();
    orbit
}

/// The lines a `--screenshot` run prints: dam and lake boxes from the real camera.
pub fn report(cells: &LakeCells, h: &HeightGrid, camera: &Camera, at: &GlobalTransform) -> Vec<String> {
    let Some(size) = camera.logical_viewport_size() else { return vec!["lake-cam: camera has no viewport yet".into()] };
    let Some((dam, lake)) = cells.boxes(h, camera, at) else { return vec!["lake-cam: dam or lake not drawable".into()] };
    let line = |n: &str, b: &PxBox| {
        format!("lake-cam: {n} box x {:.0}..{:.0} y {:.0}..{:.0} ({:.0}x{:.0} px, {:.1}% of window)", b.min.x, b.max.x, b.min.y, b.max.y, b.size().x, b.size().y, b.coverage(size) * 100.0)
    };
    vec![
        format!("lake-cam: window {:.0}x{:.0}", size.x, size.y),
        line("dam", &dam),
        line("lake", &lake),
        format!("lake-cam: dam inside window: {}; lake inside window: {}; lake covers {:.1}% of window", dam.inside(size), lake.inside(size), lake.coverage(size) * 100.0),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flat(w: usize, d: usize) -> HeightGrid {
        HeightGrid::new(w, d, 2.0)
    }

    /// The box of a known cell, projected by hand (view and projection matrices), matches the camera's.
    #[test]
    fn projected_box_of_a_known_cell_matches_where_it_is_drawn() {
        let h = flat(64, 64);
        let size = Vec2::new(1280.0, 720.0);
        let orbit = OrbitCamera { target: cell_world(&h, (30.0, 30.0)), yaw: 0.8, pitch: 0.7, distance: 40.0 };
        let (camera, at) = (camera_for(size), GlobalTransform::from(orbit.transform()));
        let pts = cell_corners(&h, &[(31usize, 29usize)], |t| t);
        let b = project_box(&camera, &at, &pts).expect("drawable");
        let view = orbit.transform().to_matrix().inverse();
        let proj = Mat4::perspective_rh(std::f32::consts::FRAC_PI_4, size.x / size.y, 0.1, 1000.0);
        let (mut lo, mut hi) = (Vec2::MAX, Vec2::MIN);
        for p in &pts {
            let c = proj * view * p.extend(1.0);
            let ndc = Vec2::new(c.x, c.y) / c.w;
            let px = Vec2::new((ndc.x + 1.0) / 2.0 * size.x, (1.0 - ndc.y) / 2.0 * size.y);
            lo = lo.min(px);
            hi = hi.max(px);
        }
        for (a, e) in [(b.min, lo), (b.max, hi)] {
            assert!(a.distance(e) <= 2.0, "projected {a:?} vs drawn {e:?}");
        }
        assert!(b.inside(size) && b.size().x > 5.0, "{b:?}");
    }

    #[test]
    fn frame_puts_dam_and_lake_inside_the_window() {
        let h = flat(128, 128);
        let size = Vec2::new(1280.0, 720.0);
        let lake: Vec<(usize, usize)> = (40..90).flat_map(|y| (30..80).map(move |x| (x, y))).collect();
        let cells = LakeCells { dam: (30..80).map(|x| (x, 90)).collect(), dam_height: 1.0, lake, level: 3.0 };
        let base = crate::camera::preset("lake").expect("preset");
        let o = frame(base, &cells, &h, size);
        let (camera, at) = (camera_for(size), GlobalTransform::from(o.transform()));
        let (dam, lake) = cells.boxes(&h, &camera, &at).expect("drawable");
        assert!(dam.inside(size) && lake.inside(size), "{dam:?} {lake:?}");
        assert!(lake.coverage(size) > 0.25, "{}", lake.coverage(size));
    }
}
