use super::MapLens;
use crate::state::Sim;
use crate::terrain_render::CELL_SIZE;
use bevy::prelude::*;

pub const MAJOR_EVERY: i32 = 4;
const LIFT: f32 = 0.05;
const CHECK_SECS: f32 = 0.5;
const DAM_QUANTUM: f32 = 1.0 / 32.0;

pub const MINOR_COLOR: Color = Color::srgba(0.16, 0.12, 0.08, 0.55);
pub const MAJOR_COLOR: Color = Color::srgba(0.12, 0.08, 0.05, 0.85);

pub fn interval_for(range: f32) -> f32 {
    [0.25, 0.5, 1.0, 2.0, 5.0].into_iter().find(|st| range / st <= 32.0).unwrap_or(10.0)
}

pub fn surface(s: &sim::SimState) -> Vec<f32> {
    s.height.as_slice().iter().zip(s.water.dam_height.as_slice()).map(|(t, d)| t + d.max(0.0)).collect()
}

pub fn surface_range(s: &sim::SimState) -> (f32, f32) {
    let z = surface(s);
    let lo = z.iter().copied().fold(f32::MAX, f32::min);
    let hi = z.iter().copied().fold(f32::MIN, f32::max);
    if z.is_empty() { (0.0, 0.0) } else { (lo, hi) }
}

pub fn interval(s: &sim::SimState) -> f32 {
    let (lo, hi) = surface_range(s);
    interval_for(hi - lo)
}

pub fn contours(z: &[f32], wet: &[bool], w: usize, d: usize, step: f32, major_every: i32) -> (Vec<[Vec3; 2]>, Vec<[Vec3; 2]>) {
    let (mut minor, mut major) = (Vec::new(), Vec::new());
    if w < 2 || d < 2 || z.len() < w * d || step <= 0.0 {
        return (minor, major);
    }
    let p = |x: usize, y: usize| Vec3::new(x as f32, z[y * w + x], y as f32);
    let wet = |x: usize, y: usize| wet.get(y * w + x).copied().unwrap_or(false);
    for y in 0..d - 1 {
        for x in 0..w - 1 {
            let (a, b, c, e) = (p(x, y), p(x + 1, y), p(x, y + 1), p(x + 1, y + 1));
            let (wb, wc) = (wet(x + 1, y), wet(x, y + 1));
            if !(wet(x, y) && wb && wc) {
                triangle([a, b, c], step, major_every, &mut minor, &mut major);
            }
            if !(wb && wet(x + 1, y + 1) && wc) {
                triangle([b, e, c], step, major_every, &mut minor, &mut major);
            }
        }
    }
    (minor, major)
}

pub fn wet_mask(s: &sim::SimState) -> Vec<bool> {
    s.water.depth.as_slice().iter().map(|&d| d > crate::terrain_render::WET_EPS).collect()
}

fn triangle(t: [Vec3; 3], step: f32, major_every: i32, minor: &mut Vec<[Vec3; 2]>, major: &mut Vec<[Vec3; 2]>) {
    let lo = t[0].y.min(t[1].y).min(t[2].y);
    let hi = t[0].y.max(t[1].y).max(t[2].y);
    if hi - lo < 1e-6 {
        return;
    }
    for k in (lo / step).ceil() as i32..=(hi / step).floor() as i32 {
        let level = k as f32 * step;
        let mut pts = [Vec3::ZERO; 2];
        let mut n = 0;
        for (i, j) in [(0, 1), (1, 2), (2, 0)] {
            let (va, vb) = (t[i].y, t[j].y);
            if (va >= level) != (vb >= level) && n < 2 {
                pts[n] = t[i].lerp(t[j], (level - va) / (vb - va));
                n += 1;
            }
        }
        if n == 2 && pts[0].distance_squared(pts[1]) > 1e-8 {
            if k.rem_euclid(major_every) == 0 { major.push(pts) } else { minor.push(pts) }
        }
    }
}

fn surface_key(s: &sim::SimState, wet: &[bool]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325 ^ ((s.height.width() as u64) << 32 | s.height.height() as u64);
    for ((t, d), &w) in s.height.as_slice().iter().zip(s.water.dam_height.as_slice()).zip(wet) {
        let dq = (d / DAM_QUANTUM).round() as i32 as u32 as u64;
        h = (h ^ (t.to_bits() as u64 | dq << 32 | (w as u64) << 63)).wrapping_mul(0x100_0000_01b3);
        h ^= h >> 29;
    }
    h
}

#[derive(Resource, Default)]
pub struct Contours {
    key: Option<u64>,
    minor: Handle<GizmoAsset>,
    major: Handle<GizmoAsset>,
    shown: Vec<Entity>,
    next_check: f32,
}

fn asset(segs: &[[Vec3; 2]], w: usize, d: usize, color: Color) -> GizmoAsset {
    let (ox, oz) = (w as f32 * CELL_SIZE / 2.0, d as f32 * CELL_SIZE / 2.0);
    let world = |p: Vec3| Vec3::new(p.x * CELL_SIZE - ox, p.y + LIFT, p.z * CELL_SIZE - oz);
    let mut g = GizmoAsset::new();
    for [a, b] in segs {
        g.line(world(*a), world(*b), color);
    }
    g
}

pub fn update_contours(mut commands: Commands, time: Res<Time>, sim: Res<Sim>, mut c: ResMut<Contours>, mut assets: ResMut<Assets<GizmoAsset>>) {
    let on = crate::terrain_render::map_lens() == MapLens::Elevation.code();
    if !on {
        for e in c.shown.drain(..) {
            commands.entity(e).despawn();
        }
        return;
    }
    let now = time.elapsed_secs();
    if !c.shown.is_empty() && now < c.next_check {
        return;
    }
    c.next_check = now + CHECK_SECS;
    let s = &sim.0;
    let wet = wet_mask(s);
    let key = surface_key(s, &wet);
    if c.key != Some(key) {
        let (w, d) = (s.height.width(), s.height.height());
        let (minor, major) = contours(&surface(s), &wet, w, d, interval(s), MAJOR_EVERY);
        c.minor = assets.add(asset(&minor, w, d, MINOR_COLOR));
        c.major = assets.add(asset(&major, w, d, MAJOR_COLOR));
        c.key = Some(key);
        for e in c.shown.drain(..) {
            commands.entity(e).despawn();
        }
    }
    if c.shown.is_empty() {
        for (handle, width) in [(c.minor.clone(), 1.2), (c.major.clone(), 2.4)] {
            let e = commands.spawn(Gizmo { handle, line_config: GizmoLineConfig { width, ..default() }, depth_bias: -0.002 }).id();
            c.shown.push(e);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interval_keeps_the_map_to_a_readable_number_of_lines() {
        assert_eq!(interval_for(4.0), 0.25);
        assert_eq!(interval_for(12.0), 0.5);
        assert_eq!(interval_for(20.0), 1.0);
        assert_eq!(interval_for(60.0), 2.0);
        assert_eq!(interval_for(1000.0), 10.0);
    }

    #[test]
    fn a_slope_gives_one_straight_line_per_level_and_every_fourth_is_major() {
        let (w, d) = (5, 3);
        let z: Vec<f32> = (0..w * d).map(|i| (i % w) as f32).collect();
        let (minor, major) = contours(&z, &[], w, d, 0.5, MAJOR_EVERY);
        for seg in minor.iter().chain(&major) {
            assert!((seg[0].y - seg[1].y).abs() < 1e-6, "a segment stays on one level");
            assert!((seg[0].x - seg[0].y).abs() < 1e-5, "the line sits where the ground has that height");
        }
        let levels = |v: &[[Vec3; 2]]| {
            let mut l: Vec<i32> = v.iter().map(|s| (s[0].y * 2.0).round() as i32).collect();
            l.sort();
            l.dedup();
            l
        };
        assert_eq!(levels(&major), vec![4, 8]);
        assert!(levels(&minor).contains(&1) && levels(&minor).contains(&7));
        let span: f32 = minor.iter().filter(|s| (s[0].y - 0.5).abs() < 1e-6).map(|s| (s[0].z - s[1].z).abs()).sum();
        assert!((span - 2.0).abs() < 1e-4, "level 0.5 runs the whole grid, got {span}");
    }

    #[test]
    fn a_dam_crest_raises_the_surface_and_flat_ground_has_no_lines() {
        let (w, d) = (4, 4);
        let flat = vec![1.0; w * d];
        let (a, b) = contours(&flat, &[], w, d, 0.5, MAJOR_EVERY);
        assert!(a.is_empty() && b.is_empty());
        let mut dam = flat.clone();
        dam[w + 1] += 1.2;
        let (minor, major) = contours(&dam, &[], w, d, 0.5, MAJOR_EVERY);
        assert!(!minor.is_empty() || !major.is_empty(), "the crest shows as raised ground");
        assert!(minor.iter().chain(&major).all(|s| s[0].y > 1.0 && s[0].y <= 2.2));
    }

    #[test]
    fn ground_wholly_under_water_has_no_lines() {
        let (w, d) = (5, 3);
        let z: Vec<f32> = (0..w * d).map(|i| (i % w) as f32).collect();
        let wet: Vec<bool> = (0..w * d).map(|i| i % w < 2).collect();
        let (minor, major) = contours(&z, &wet, w, d, 0.5, MAJOR_EVERY);
        let (all_minor, _) = contours(&z, &[], w, d, 0.5, MAJOR_EVERY);
        assert!(minor.len() < all_minor.len());
        assert!(minor.iter().chain(&major).all(|s| s[0].x >= 1.0 && s[1].x >= 1.0), "nothing inside the pond");
    }
}
