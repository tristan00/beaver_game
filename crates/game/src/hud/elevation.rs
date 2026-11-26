use super::MapLens;
use crate::state::Sim;
use crate::terrain_render::CELL_SIZE;
use bevy::prelude::*;

/// A light line every 10 cm of height, a bold one every metre.
pub const STEP_M: f32 = 0.1;
pub const MAJOR_EVERY: i32 = 10;
/// Where the ground rises more than this across one triangle (m), the 10 cm lines would bunch
/// into a smear: only the metre lines are drawn there.
const LIGHT_MAX_RELIEF: f32 = 0.5;
const LIFT: f32 = 0.05;
const CHECK_SECS: f32 = 0.5;
const DAM_QUANTUM: f32 = 1.0 / 32.0;

pub const MINOR_COLOR: Color = Color::srgba(0.16, 0.12, 0.08, 0.32);
pub const MAJOR_COLOR: Color = Color::srgba(0.12, 0.08, 0.05, 0.85);

/// The window drawn round the camera's target: this many cells per unit of camera distance
/// either side, within bounds; it moves in steps of `WINDOW_SNAP` cells.
const WINDOW_PER_DISTANCE: f32 = 0.9;
const WINDOW_MIN: f32 = 32.0;
const WINDOW_MAX: f32 = 160.0;
const WINDOW_SNAP: i32 = 16;

pub fn surface(s: &sim::SimState) -> Vec<f32> {
    s.height.as_slice().iter().zip(s.water.dam_height.as_slice()).map(|(t, d)| t + d.max(0.0)).collect()
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
    let steep = hi - lo > LIGHT_MAX_RELIEF;
    for k in (lo / step).ceil() as i32..=(hi / step).floor() as i32 {
        let is_major = k.rem_euclid(major_every) == 0;
        if steep && !is_major {
            continue;
        }
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
            if is_major { major.push(pts) } else { minor.push(pts) }
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
    key: Option<(u64, [i32; 4])>,
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

/// Contours of the ground round where the camera looks: light every 10 cm, bold every metre;
/// redrawn as the view moves or the ground and dams change.
pub fn update_contours(mut commands: Commands, time: Res<Time>, sim: Res<Sim>, orbit: Res<crate::camera::OrbitCamera>, active: Res<super::ActiveLens>, mut c: ResMut<Contours>, mut assets: ResMut<Assets<GizmoAsset>>) {
    let _p = ::sim::prof::scope("game/update contours");
    if active.0 != MapLens::Elevation {
        for e in c.shown.drain(..) {
            commands.entity(e).despawn();
        }
        c.key = None;
        return;
    }
    let now = time.elapsed_secs();
    let s = &sim.0;
    let (w, d) = (s.height.width(), s.height.height());
    // The window, snapped so small camera moves keep the same lines.
    let half = (orbit.distance * WINDOW_PER_DISTANCE).clamp(WINDOW_MIN, WINDOW_MAX) as i32;
    let (fx, fy) = crate::flow::world_cell(&s.height, orbit.target);
    let (cx, cy) = (fx as i32, fy as i32);
    let snap = |v: i32| v.div_euclid(WINDOW_SNAP) * WINDOW_SNAP;
    let (x0, y0) = (snap(cx - half).clamp(0, w as i32 - 2), snap(cy - half).clamp(0, d as i32 - 2));
    let (x1, y1) = ((snap(cx + half) + WINDOW_SNAP).clamp(x0 + 2, w as i32), (snap(cy + half) + WINDOW_SNAP).clamp(y0 + 2, d as i32));
    let window = [x0, y0, x1, y1];
    let moved = c.key.is_none_or(|(_, win)| win != window);
    if !moved && now < c.next_check {
        return;
    }
    c.next_check = now + CHECK_SECS;
    let wet = wet_mask(s);
    let key = (surface_key(s, &wet), window);
    if c.key == Some(key) {
        return;
    }
    let (ww, wd) = ((x1 - x0) as usize, (y1 - y0) as usize);
    let full = surface(s);
    let mut z = Vec::with_capacity(ww * wd);
    let mut wz = Vec::with_capacity(ww * wd);
    for y in y0 as usize..y1 as usize {
        for x in x0 as usize..x1 as usize {
            z.push(full[y * w + x]);
            wz.push(wet[y * w + x]);
        }
    }
    let (mut minor, mut major) = contours(&z, &wz, ww, wd, STEP_M, MAJOR_EVERY);
    for seg in minor.iter_mut().chain(major.iter_mut()) {
        for p in seg.iter_mut() {
            p.x += x0 as f32;
            p.z += y0 as f32;
        }
    }
    c.minor = assets.add(asset(&minor, w, d, MINOR_COLOR));
    c.major = assets.add(asset(&major, w, d, MAJOR_COLOR));
    c.key = Some(key);
    for e in c.shown.drain(..) {
        commands.entity(e).despawn();
    }
    for (handle, width) in [(c.minor.clone(), 1.2), (c.major.clone(), 2.4)] {
        let e = commands.spawn(Gizmo { handle, line_config: GizmoLineConfig { width, ..default() }, depth_bias: -0.002 }).id();
        c.shown.push(e);
    }
}
