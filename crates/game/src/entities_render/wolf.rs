//! The wolf and shared quadruped animation/tracking helpers used by `wildlife.rs`.
use super::meshgen::{build_mesh, hex, mix, v, Mb};
use super::scene::height_at;
use super::sim_view::cell_xz;
use super::Models;
use crate::seasonal::smooth as smooth01;
use crate::sim_driver::BASE_DAYS_PER_SECOND;
use crate::state::Sim;
use bevy::prelude::*;
use sim::predators::WolfState;
use std::collections::{HashMap, VecDeque};
use std::f32::consts::{PI, TAU};

const WOLF_SIZE: f32 = 0.55;
const NECK_PIVOT: Vec3 = Vec3::new(0.0, 1.24, 0.62);
const HEAD_ON_NECK: Vec3 = Vec3::new(0.0, 0.34, 0.3);
const TAIL_PIVOT: Vec3 = Vec3::new(0.0, 1.27, -0.84);
const LEG_PIVOTS: [Vec3; 4] = [Vec3::new(-0.12, 1.06, 0.46), Vec3::new(0.12, 1.06, 0.46), Vec3::new(-0.12, 1.12, -0.6), Vec3::new(0.12, 1.12, -0.6)];
const FRONT_BONES: (f32, f32) = (0.4, 0.72);
const HIND_BONES: (f32, f32) = (0.46, 0.74);

pub struct WolfPlugin;

impl Plugin for WolfPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<AlarmSlaps>()
            .init_resource::<WolfDrawn>()
            .add_systems(Startup, init_wolf_meshes)
            .add_systems(Update, (force_wolf, sync_wolf, animate_wolf, follow_wolf).chain())
            .add_systems(Update, (tail_slaps, force_lookout, select_wolf));
    }
}

// ---- Shared quadruped & animal rendering helpers (also used by wildlife.rs) ----

pub(super) type GaitTable = [([f32; 4], f32, f32, f32); 3];

pub(super) fn ax(a: f32) -> Quat { Quat::from_rotation_x(a) }

pub(super) fn lerp_t(a: Transform, b: Transform, k: f32) -> Transform {
    Transform::from_translation(a.translation.lerp(b.translation, k)).with_rotation(a.rotation.slerp(b.rotation, k))
}

pub(super) fn ease_toward(w: &mut f32, on: bool, rate: f32, dt: f32) {
    *w += (if on { 1.0 } else { 0.0 } - *w) * (1.0 - (-dt * rate).exp());
}

pub(super) fn ground_pt(s: &sim::SimState, xz: Vec2) -> Vec3 {
    Vec3::new(xz.x, height_at(&s.height, xz.x, xz.y), xz.y)
}

pub(super) fn coat(p: Vec3, n: Vec3, top: [f32; 3], side: [f32; 3], under: [f32; 3], f: (f32, f32, f32), grain: f32) -> [f32; 3] {
    let c = if n.y > 0.0 { mix(side, top, smooth01(0.28, 0.92, n.y)) } else { mix(side, under, smooth01(0.02, 0.52, -n.y)) };
    let g = 1.0 + (p.x * f.0).sin() * (p.y * f.1).sin() * (p.z * f.2).sin() * grain;
    [c[0] * g, c[1] * g, c[2] * g]
}

pub(super) fn env_floats(name: &str) -> Vec<f32> {
    std::env::var(name).map(|v| v.split(',').filter_map(|t| t.trim().parse().ok()).collect()).unwrap_or_default()
}

pub(super) fn gallery_env() -> (f32, Option<usize>) {
    (std::env::var("BC_GALLERY_YAW").ok().and_then(|v| v.parse().ok()).unwrap_or(std::f32::consts::FRAC_PI_2),
     std::env::var("BC_GALLERY_POSE").ok().and_then(|v| v.parse().ok()))
}

pub(super) fn gait_mix(table: &GaitTable, w: Vec3, f: impl Fn(&([f32; 4], f32, f32, f32)) -> f32) -> f32 {
    f(&table[0]) * w.x + f(&table[1]) * w.y + f(&table[2]) * w.z
}

#[allow(clippy::too_many_arguments)]
pub(super) fn quadruped_feet(table: &GaitTable, w: Vec3, m: f32, speed: f32, max_stride: f32, ph: f32, mut feet: [Vec2; 4], reach: (f32, f32), front_lift: f32) -> [Vec2; 4] {
    let (duty, lift) = (gait_mix(table, w, |g| g.1), gait_mix(table, w, |g| g.3) * m);
    let stride = (speed * duty / gait_mix(table, w, |g| g.2)).min(max_stride) * m;
    for (i, f) in feet.iter_mut().enumerate() {
        let p = (ph + gait_mix(table, w, |g| g.0[i])).rem_euclid(1.0);
        let r = if i < 2 { reach.0 } else { reach.1 } * w.z;
        if p < duty {
            f.y += r + stride * (0.5 - p / duty);
        } else {
            let s = (p - duty) / (1.0 - duty);
            f.y += r + stride * (smooth01(0.0, 1.0, s) - 0.5);
            f.x = lift * (PI * s).sin().powf(0.8) * if i < 2 { front_lift } else { 1.0 };
        }
    }
    feet
}

pub(super) fn leg_ik(hip: Vec2, foot: Vec2, (a, b): (f32, f32), knee_back: bool) -> (f32, f32) {
    let d = foot - hip;
    let dist = d.length().clamp((a - b).abs() + 1e-3, a + b - 1e-3);
    let reach = (-d.y).atan2(-d.x);
    let at_hip = ((a * a + dist * dist - b * b) / (2.0 * a * dist)).clamp(-1.0, 1.0).acos();
    let at_knee = ((a * a + b * b - dist * dist) / (2.0 * a * b)).clamp(-1.0, 1.0).acos();
    let s = if knee_back { 1.0 } else { -1.0 };
    (reach + s * at_hip, -s * (PI - at_knee))
}

pub(super) fn quadruped_legs(body: Transform, pivots: [Vec3; 4], front: (f32, f32), hind: (f32, f32), feet: [Vec2; 4]) -> [(f32, f32); 4] {
    let (pitch, _, _) = body.rotation.to_euler(EulerRot::XYZ);
    std::array::from_fn(|k| {
        let hip = body.transform_point(pivots[k]);
        let (up, low) = leg_ik(Vec2::new(hip.y, hip.z), feet[k], if k < 2 { front } else { hind }, k < 2);
        (up - pitch, low)
    })
}

// ---- Wolf meshes & components ----

#[derive(Resource)]
pub(super) struct WolfMeshes {
    body: Handle<Mesh>,
    neck: Handle<Mesh>,
    head: Handle<Mesh>,
    tail: Handle<Mesh>,
    upper: [Handle<Mesh>; 2],
    lower: [Handle<Mesh>; 2],
}

#[derive(Component)]
struct WolfRig {
    body: Entity,
    neck: Entity,
    head: Entity,
    tail: Entity,
    upper: [Entity; 4],
    lower: [Entity; 4],
    phase: f32,
    moving: f32,
    gait: Vec3,
    last: Vec3,
    kill_t: f32,
    kill_w: f32,
}

#[derive(Component)]
struct SimWolf(u64);

#[derive(Component)]
struct GalleryWolf { pace: f32, kill: bool }

fn fur(p: Vec3, n: Vec3) -> [f32; 3] {
    coat(p, n, hex(0x46433f), hex(0x8e8981), hex(0xd9d3c7), (41.0, 37.0, 29.0), 0.05)
}

fn furred(mb: &mut Mb, f: impl FnOnce(&mut Mb)) {
    let start = mb.len();
    f(mb);
    mb.paint(start, |p, n, _| fur(p, n));
}

fn body_mesh(mb: &mut Mb) {
    let rings = [
        (v(0.0, 1.17, -0.93), 0.02, 0.02), (v(0.0, 1.18, -0.88), 0.1, 0.12), (v(0.0, 1.19, -0.76), 0.16, 0.19),
        (v(0.0, 1.19, -0.58), 0.18, 0.22), (v(0.0, 1.19, -0.38), 0.16, 0.2), (v(0.0, 1.16, -0.15), 0.17, 0.25),
        (v(0.0, 1.12, 0.1), 0.19, 0.32), (v(0.0, 1.11, 0.32), 0.2, 0.35), (v(0.0, 1.15, 0.52), 0.19, 0.31),
        (v(0.0, 1.21, 0.66), 0.15, 0.23), (v(0.0, 1.26, 0.72), 0.06, 0.08),
    ];
    furred(mb, |mb| mb.loft(&rings, 16, [0.0; 3]));
}

fn neck_mesh(mb: &mut Mb) {
    let up = HEAD_ON_NECK;
    furred(mb, |mb| mb.loft(&[(v(0.0, -0.2, -0.2), 0.04, 0.04), (v(0.0, -0.12, -0.1), 0.15, 0.22), (up * 0.2, 0.16, 0.22), (up * 0.6, 0.14, 0.18), (up * 0.95, 0.12, 0.13), (up * 1.05, 0.04, 0.04)], 14, [0.0; 3]));
    let start = mb.len();
    mb.ellipsoid_r(v(0.0, 0.0, 0.12), v(0.11, 0.16, 0.1), ax(-0.7), [0.0; 3]);
    mb.paint(start, |p, n, _| mix(fur(p, n), hex(0xd9d3c7), 0.35));
}

fn head_mesh(mb: &mut Mb) {
    let (dark, pale, cream) = (hex(0x4b4843), hex(0xa29c92), hex(0xe2d9c8));
    furred(mb, |mb| {
        mb.ellipsoid_g(v(0.0, 0.06, 0.06), v(0.16, 0.15, 0.18), dark, dark, 14, 9);
        mb.loft(&[(v(0.0, 0.05, 0.08), 0.03, 0.03), (v(0.0, 0.04, 0.14), 0.13, 0.12), (v(0.0, 0.02, 0.28), 0.1, 0.095), (v(0.0, -0.01, 0.42), 0.075, 0.075), (v(0.0, -0.02, 0.5), 0.06, 0.062), (v(0.0, -0.02, 0.53), 0.025, 0.025)], 12, dark);
    });
    let start = mb.len();
    for s in [-1.0f32, 1.0] { mb.ellipsoid_r(v(0.1 * s, -0.04, 0.06), v(0.08, 0.1, 0.11), Quat::from_rotation_z(0.3 * s), pale); }
    mb.ellipsoid_r(v(0.0, -0.07, 0.3), v(0.065, 0.04, 0.2), ax(-0.06), cream);
    mb.paint(start, |p, n, c| if p.y < -0.06 { cream } else { mix(c, fur(p, n), 0.6) });
    mb.ellipsoid(v(0.0, -0.005, 0.53), v(0.045, 0.035, 0.03), hex(0x1a1816));
    for s in [-1.0f32, 1.0] {
        let tilt = Quat::from_rotation_z(-0.35 * s) * ax(0.15);
        let ear = Vec3::new(0.09 * s, 0.2, 0.0);
        mb.cyl(ear, ear + tilt * Vec3::new(0.0, 0.14, 0.0), 0.075, 0.008, dark, dark, 4);
        mb.cyl(ear + tilt * Vec3::new(0.0, 0.0, 0.015), ear + tilt * Vec3::new(0.0, 0.11, 0.015), 0.05, 0.005, hex(0xc9b7a2), hex(0xc9b7a2), 4);
        mb.ellipsoid(v(0.075 * s, 0.09, 0.2), v(0.035, 0.024, 0.02), hex(0xd49a32));
        mb.ellipsoid(v(0.078 * s, 0.09, 0.215), v(0.016, 0.016, 0.01), hex(0x1a1816));
        mb.ellipsoid_r(v(0.075 * s, 0.12, 0.2), v(0.045, 0.012, 0.025), Quat::from_rotation_z(0.25 * s), dark);
    }
}

fn tail_mesh(mb: &mut Mb) {
    let start = mb.len();
    mb.loft(&[(v(0.0, 0.03, 0.05), 0.03, 0.03), (v(0.0, 0.0, -0.02), 0.07, 0.07), (v(0.0, -0.12, -0.12), 0.1, 0.1), (v(0.0, -0.3, -0.2), 0.12, 0.12), (v(0.0, -0.5, -0.24), 0.11, 0.11), (v(0.0, -0.66, -0.25), 0.07, 0.07), (v(0.0, -0.74, -0.25), 0.02, 0.02)], 12, [0.0; 3]);
    mb.paint(start, |p, n, _| mix(fur(p, n), hex(0x1f1d1a), smooth01(-0.5, -0.62, p.y)));
}

fn upper_mesh(mb: &mut Mb, front: bool) {
    let len = if front { FRONT_BONES.0 } else { HIND_BONES.0 };
    furred(mb, |mb| if front {
        mb.loft(&[(v(0.0, 0.14, 0.02), 0.02, 0.02), (v(0.0, 0.08, 0.02), 0.1, 0.13), (v(0.0, -0.15, 0.0), 0.09, 0.11), (v(0.0, -len, -0.01), 0.065, 0.065), (v(0.0, -len - 0.05, -0.01), 0.02, 0.02)], 10, [0.0; 3]);
    } else {
        mb.loft(&[(v(0.0, 0.16, -0.02), 0.02, 0.02), (v(0.0, 0.08, -0.03), 0.12, 0.17), (v(0.0, -0.14, 0.0), 0.11, 0.15), (v(0.0, -len, 0.02), 0.06, 0.07), (v(0.0, -len - 0.05, 0.02), 0.02, 0.02)], 10, [0.0; 3]);
    });
}

fn lower_mesh(mb: &mut Mb, front: bool) {
    let (len, leg, start) = (if front { FRONT_BONES.1 } else { HIND_BONES.1 }, hex(0xb3ab9e), mb.len());
    if front {
        mb.loft(&[(v(0.0, 0.05, 0.0), 0.02, 0.02), (v(0.0, 0.0, 0.0), 0.075, 0.08), (v(0.0, -0.22, 0.0), 0.06, 0.065), (v(0.0, -len + 0.14, 0.0), 0.042, 0.048), (v(0.0, -len + 0.06, 0.02), 0.042, 0.046), (v(0.0, -len + 0.03, 0.03), 0.02, 0.02)], 8, leg);
    } else {
        let hock = v(0.0, -0.36, -0.15);
        mb.loft(&[(v(0.0, 0.05, 0.01), 0.02, 0.02), (v(0.0, 0.0, 0.0), 0.08, 0.1), (hock * 0.5, 0.06, 0.075), (hock, 0.045, 0.055), (v(0.0, -len + 0.08, -0.02), 0.04, 0.045), (v(0.0, -len + 0.03, 0.0), 0.02, 0.02)], 8, leg);
    }
    mb.paint(start, |p, n, c| mix(fur(p, n), c, smooth01(-0.05, -0.3, p.y)));
    mb.ellipsoid(v(0.0, -len + 0.035, 0.05), v(0.058, 0.038, 0.085), hex(0x86807a));
    mb.ellipsoid(v(0.0, -len + 0.012, 0.06), v(0.05, 0.012, 0.07), hex(0x2c2926));
}

fn init_wolf_meshes(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>) {
    commands.insert_resource(WolfMeshes {
        body: build_mesh(&mut meshes, body_mesh),
        neck: build_mesh(&mut meshes, neck_mesh),
        head: build_mesh(&mut meshes, head_mesh),
        tail: build_mesh(&mut meshes, tail_mesh),
        upper: [build_mesh(&mut meshes, |m| upper_mesh(m, true)), build_mesh(&mut meshes, |m| upper_mesh(m, false))],
        lower: [build_mesh(&mut meshes, |m| lower_mesh(m, true)), build_mesh(&mut meshes, |m| lower_mesh(m, false))],
    });
}

#[derive(Resource, Default)]
pub struct WolfDrawn(pub Vec<(u64, Vec3)>);

impl WolfDrawn {
    pub fn at(&self, key: u64) -> Option<Vec3> { self.0.iter().find(|w| w.0 == key).map(|w| w.1) }
}

#[derive(Resource, Default)]
pub struct AlarmSlaps { pub count: u32, pub amp: f32 }

#[derive(Component)]
struct Splash { age: f32, mat: Handle<StandardMaterial> }

const SLAP_EVERY: f32 = 2.5;
const SPLASH_SECS: f32 = 1.4;

#[allow(clippy::too_many_arguments)]
fn tail_slaps(
    mut commands: Commands, time: Res<Time>, sim: Res<Sim>, orbit: Res<crate::camera::OrbitCamera>,
    mut slaps: ResMut<AlarmSlaps>, render: Res<super::RenderClock>, mut slap_times: ResMut<super::TailSlapTimes>,
    mut meshes: ResMut<Assets<Mesh>>, mut materials: ResMut<Assets<StandardMaterial>>,
    mut ring: Local<Option<Handle<Mesh>>>, mut next: Local<HashMap<u64, f32>>,
    mut splashes: Query<(Entity, &mut Splash, &mut Transform)>,
) {
    let (dt, now) = (time.delta_secs(), time.elapsed_secs());
    for (e, mut sp, mut tr) in &mut splashes {
        sp.age += dt;
        let k = (sp.age / SPLASH_SECS).min(1.0);
        if k >= 1.0 { commands.entity(e).despawn(); materials.remove(&sp.mat); continue; }
        tr.scale = Vec3::splat(0.3 + 3.2 * (1.0 - (1.0 - k) * (1.0 - k)));
        if let Some(mut m) = materials.get_mut(&sp.mat) { m.base_color = Color::srgba(1.0, 1.0, 1.0, (1.0 - k) * (1.0 - k).sqrt()); }
    }
    let s = &sim.0;
    next.retain(|id, _| s.predators.alarms.iter().any(|b| b.0 == *id));
    for &id in &s.predators.alarms {
        let due = next.entry(id.0).or_insert(now);
        if now < *due { continue; }
        *due = now + SLAP_EVERY;
        let Some(at) = slap_spot(s, id) else { continue };
        if let Some(sentry) = s.work.place(id).and_then(|p| p.slots.iter().find_map(|sl| sl.beaver)) {
            slap_times.0.insert(sentry.0, render.0);
        }
        let mesh = ring.get_or_insert_with(|| build_mesh(&mut meshes, |mb| {
            mb.lathe(Mat4::IDENTITY, &[(1.0, 0.0), (0.72, 0.0)], 48, [1.0; 3], [1.0; 3], false);
            mb.lathe(Mat4::IDENTITY, &[(0.55, 0.0), (0.35, 0.0)], 32, [1.0; 3], [1.0; 3], false);
            mb.ellipsoid(v(0.0, 0.15, 0.0), v(0.2, 0.45, 0.2), [1.0; 3]);
        }));
        let mat = materials.add(StandardMaterial { base_color: Color::WHITE, emissive: LinearRgba::rgb(1.2, 1.25, 1.25), alpha_mode: AlphaMode::Blend, unlit: true, double_sided: true, cull_mode: None, ..default() });
        commands.spawn((Mesh3d(mesh.clone()), MeshMaterial3d(mat.clone()), Transform::from_translation(at + Vec3::Y * 0.25).with_scale(Vec3::splat(0.3)), Splash { age: 0.0, mat }));
        slaps.count = slaps.count.wrapping_add(1);
        slaps.amp = (1.3 - orbit.target.distance(at) / 90.0).clamp(0.25, 1.2);
    }
}

fn slap_spot(s: &sim::SimState, id: sim::buildings::BuildingId) -> Option<Vec3> {
    let b = s.buildings.building(id)?;
    let from = s.work.place(id)?.slots.iter().find_map(|sl| sl.beaver).and_then(|w| s.agents.get(w)).map_or(b.pos, |a| a.cell);
    let (c, _) = (-5i32..=5).flat_map(|dy| (-5i32..=5).map(move |dx| (dx, dy)))
        .filter_map(|(dx, dy)| {
            let c = ((from.0 as i32 + dx).max(0) as usize, (from.1 as i32 + dy).max(0) as usize);
            s.water.depth.get(c.0, c.1).is_some_and(|d| *d > 0.05).then_some((c, (dx * dx + dy * dy) as usize))
        }).min_by_key(|x| x.1)?;
    let xz = cell_xz(s, (c.0 as f32 + 0.5, c.1 as f32 + 0.5));
    Some(Vec3::new(xz.x, height_at(&s.height, xz.x, xz.y) + s.water.depth.get(c.0, c.1).copied().unwrap_or(0.0), xz.y))
}

fn ring_spot(s: &sim::SimState, centre: (f32, f32), ring: f32, steps: usize) -> Option<(f32, f32)> {
    (0..steps).map(|k| k as f32 / steps as f32 * TAU).find_map(|a| {
        let cell = ((centre.0 + ring * a.cos()).max(0.0) as usize, (centre.1 + ring * a.sin()).max(0.0) as usize);
        sim::predators::walkable(s, cell).then_some((cell.0 as f32 + 0.5, cell.1 as f32 + 0.5))
    })
}

fn force_wolf(mut sim: ResMut<Sim>, mut done: Local<bool>) {
    if *done || !sim.0.colony.is_founded() || sim.0.agents.by_id.is_empty() { return; }
    let Ok(how) = std::env::var("BC_WOLF") else { *done = true; return; };
    let s = &mut sim.0;
    let close = how.starts_with("close");
    let near = close || how.starts_with("near");
    if s.time_days < how.split_once(':').and_then(|(_, d)| d.parse().ok()).unwrap_or(0.0) { return; }
    let outdoor: Vec<_> = s.agents.by_id.values().filter(|a| !a.inside && matches!(s.nav.surface_at(a.cell), Some(sim::nav::Surface::Land))).map(|a| a.cell).collect();
    let dry_round = |c: (usize, usize)| (-8i32..=8).all(|dy| (-8i32..=8).all(|dx| {
        !matches!(s.nav.surface_at(((c.0 as i32 + dx).max(0) as usize, (c.1 as i32 + dy).max(0) as usize)), Some(sim::nav::Surface::Water | sim::nav::Surface::Canal))
    }));
    let prey = outdoor.iter().copied().find(|&c| dry_round(c)).or(outdoor.first().copied());
    if near && prey.is_none() { return; }
    *done = true;
    if !sim::predators::summon(s) { warn!("BC_WOLF: no edge of the map a wolf could walk in from"); return; }
    if let (true, Some(home)) = (near, prey) {
        let ring = if close { 20.0 } else { sim::predators::WOLF_SIGHT - 2.0 };
        if let (Some(p), Some(w)) = (ring_spot(s, (home.0 as f32, home.1 as f32), ring, 32), s.predators.wolves.last_mut()) { w.pos = p; }
    }
    info!("BC_WOLF: a wolf is on the map");
}

fn force_lookout(mut sim: ResMut<Sim>, mut orbit: ResMut<crate::camera::OrbitCamera>, mut stage: Local<u8>) {
    use sim::buildings::BuildingKind;
    if *stage >= 3 || std::env::var_os("BC_LOOKOUT").is_none() || !sim.0.colony.is_founded() { return; }
    let s = &mut sim.0;
    let Some(near) = s.agents.by_id.values().next().map(|a| a.cell) else { return };
    let build = |s: &mut sim::SimState, kind: BuildingKind| -> Option<sim::buildings::BuildingId> {
        let pos = (3..40usize).flat_map(|r| (0..=2 * r).flat_map(move |i| [(near.0 + i - r, near.1 - r), (near.0 + i - r, near.1 + r), (near.0 - r, near.1 + i - r), (near.0 + r, near.1 + i - r)])).find(|&p| s.check_site(kind, p).is_ok())?;
        let id = s.place_site(kind, pos).ok()?;
        for &(g, a) in &kind.spec().cost { let _ = s.work.deliver(id, g, a); }
        let _ = s.work.add_labour(id, kind.spec().work);
        Some(id)
    };
    match *stage {
        0 => { if !s.has_building(BuildingKind::home()) { build(s, BuildingKind::home()); } *stage = 1; }
        1 if s.has_building(BuildingKind::home()) => {
            *stage = if BuildingKind::all().find(|k| k.watches()).and_then(|k| build(s, k)).is_some() { 2 } else { warn!("BC_LOOKOUT: no site for a lookout"); 3 };
        }
        2 => {
            let Some(b) = s.buildings.buildings.iter().find(|b| b.kind.watches() && b.is_complete()).map(|b| (b.id, sim::predators::lookout_centre(b))) else { return };
            let _ = s.apply(sim::Command::AddBeaver { building: b.0 });
            orbit.target = ground_pt(s, cell_xz(s, b.1));
            let cam = env_floats("BC_LOOKOUT");
            orbit.distance = cam.first().copied().unwrap_or(90.0);
            if let Some(&p) = cam.get(1) { orbit.pitch = p; }
            if let Some(&y) = cam.get(2) { orbit.yaw = y; }
            info!("BC_LOOKOUT: a manned lookout at {:?}", b.1);
            *stage = 3;
            if let (Some(p), Some(w)) = (ring_spot(s, b.1, sim::predators::LOOKOUT_SIGHT - 8.0, 64), s.predators.wolves.first_mut()) { w.pos = p; }
        }
        _ => {}
    }
}

fn select_wolf(sim: Res<Sim>, sel: Option<ResMut<crate::flow::Selection>>) {
    let Some(first) = sim.0.predators.wolves.first() else { return };
    if std::env::var_os("BC_SELECT_WOLF").is_none() { return; }
    if let Some(mut sel) = sel {
        if !matches!(*sel, crate::flow::Selection::Wolf(k) if sim.0.predators.wolf(k).is_some()) {
            *sel = crate::flow::Selection::Wolf(first.key());
        }
    }
}

fn follow_wolf(sim: Res<Sim>, mut orbit: ResMut<crate::camera::OrbitCamera>, rigs: Query<&Transform, With<SimWolf>>, mut spec: Local<Option<Vec<f32>>>) {
    let p = spec.get_or_insert_with(|| env_floats("BC_WOLF_CAM"));
    if p.is_empty() || sim.0.predators.wolves.is_empty() { return; }
    if let Some(tr) = rigs.iter().next() {
        orbit.target = tr.translation;
        orbit.distance = p[0];
        if let Some(&y) = p.get(1) { orbit.yaw = y; }
        if let Some(&pt) = p.get(2) { orbit.pitch = pt; }
    }
}

#[derive(Component, Default)]
pub(super) struct WolfTrack {
    samples: VecDeque<(f64, Vec2)>,
    instance: u64,
    shown: Option<Vec2>,
}

impl WolfTrack {
    pub(super) fn record(&mut self, instance: u64, t: f64, p: Vec2) { self.record_with(instance, t, p, 8.0); }

    pub(super) fn record_with(&mut self, instance: u64, t: f64, p: Vec2, jump: f32) {
        if self.instance != instance { self.samples.clear(); self.instance = instance; }
        match self.samples.back() {
            Some(&(last, _)) if last >= t => return,
            Some(&(_, q)) if q.distance(p) > jump => self.samples.clear(),
            _ => {}
        }
        self.samples.push_back((t, p));
        while self.samples.len() > 64 { self.samples.pop_front(); }
    }

    pub(super) fn at(&self, t: f64) -> Option<(Vec2, Option<Vec2>)> {
        let &(t0, first) = self.samples.front()?;
        if t <= t0 { return Some((first, None)); }
        let Some(k) = self.samples.iter().position(|s| s.0 > t) else { return Some((self.samples.back()?.1, None)); };
        let ((ta, a), (tb, b)) = (self.samples[k - 1], self.samples[k]);
        let d = b - a;
        Some((a.lerp(b, ((t - ta) / (tb - ta)).clamp(0.0, 1.0) as f32), (d.length() > 1e-4).then(|| d.normalize())))
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn advance(&mut self, s: &sim::SimState, track: &sim::wildlife::Track, pos: (f32, f32), jump: f32, snap: f32, now: f64, dt: f32, smoothing: f32) -> (Vec2, Option<Vec2>) {
        let here = cell_xz(s, pos);
        for &(t, p) in &track.0 { self.record_with(s.instance, t, cell_xz(s, p), jump); }
        self.record_with(s.instance, s.time_days, here, jump);
        let (aim, going) = self.at(now).unwrap_or((here, None));
        let k = crate::sim_driver::ease_k(dt, 10.0, smoothing);
        let xz = match self.shown { Some(p) if p.distance(aim) < snap => p.lerp(aim, k), _ => aim };
        self.shown = Some(xz);
        (xz, going)
    }
}

#[allow(clippy::too_many_arguments)]
fn sync_wolf(
    mut commands: Commands, sim: Res<Sim>, time: Res<Time>, clock: Option<Res<crate::sim_driver::SimClock>>,
    meshes: Option<Res<WolfMeshes>>, models: Option<Res<Models>>, mut drawn: ResMut<WolfDrawn>,
    mut q: Query<(Entity, &SimWolf, &mut Transform, &mut WolfTrack, &mut Visibility), With<WolfRig>>,
) {
    let _p = ::sim::prof::scope("game/sync wolf");
    let (Some(wm), Some(m)) = (meshes, models) else { return };
    let (s, dt) = (&sim.0, time.delta_secs());
    let smoothing = clock.as_ref().map_or(1.0, |c| c.speed.smoothing());
    let now = clock.as_ref().map_or(s.time_days, |c| c.render_days(s.time_days));
    drawn.0.clear();
    let mut shown = Vec::new();
    for (e, id, mut tr, mut track, mut vis) in &mut q {
        let Some(wolf) = s.predators.wolf(id.0) else { commands.entity(e).despawn(); continue; };
        shown.push(id.0);
        vis.set_if_neq(Visibility::Inherited);
        let (xz, going) = track.advance(s, &wolf.track, wolf.pos, 8.0, 4.0, now, dt, smoothing);
        tr.translation = ground_pt(s, xz);
        if wolf.seen { drawn.0.push((id.0, tr.translation)); }
        let want = Quat::from_rotation_y(going.map_or(wolf.heading, |d| d.x.atan2(d.y)));
        tr.rotation = tr.rotation.slerp(want, crate::sim_driver::ease_k(dt, 8.0, smoothing));
    }
    for wolf in s.predators.wolves.iter().filter(|w| !shown.contains(&w.key())) {
        let xz = cell_xz(s, wolf.pos);
        let root = spawn_rig(&mut commands, &wm, &m, Transform::from_translation(ground_pt(s, xz)).with_rotation(Quat::from_rotation_y(wolf.heading)));
        let mut track = WolfTrack::default();
        track.record(s.instance, s.time_days, xz);
        commands.entity(root).insert((SimWolf(wolf.key()), track, Visibility::Inherited));
    }
}

fn spawn_rig(commands: &mut Commands, wm: &WolfMeshes, m: &Models, at: Transform) -> Entity {
    let mesh = |h: &Handle<Mesh>| (Mesh3d(h.clone()), MeshMaterial3d(m.white.clone()));
    let root = commands.spawn((at.with_scale(Vec3::splat(WOLF_SIZE)), Visibility::Inherited)).id();
    let body = commands.spawn((Transform::default(), Visibility::Inherited, ChildOf(root))).id();
    commands.spawn((Mesh3d(m.blob.clone()), MeshMaterial3d(m.blob_mat.clone()), Transform::from_xyz(0.0, 0.05, 0.0).with_scale(Vec3::new(0.5, 1.0, 1.25)), ChildOf(root)));
    commands.spawn((mesh(&wm.body), Transform::default(), ChildOf(body)));
    let neck = commands.spawn((mesh(&wm.neck), Transform::from_translation(NECK_PIVOT), ChildOf(body))).id();
    let head = commands.spawn((mesh(&wm.head), Transform::from_translation(HEAD_ON_NECK), ChildOf(neck))).id();
    let tail = commands.spawn((mesh(&wm.tail), Transform::from_translation(TAIL_PIVOT), ChildOf(body))).id();
    let (mut upper, mut lower) = ([Entity::PLACEHOLDER; 4], [Entity::PLACEHOLDER; 4]);
    for (k, &pivot) in LEG_PIVOTS.iter().enumerate() {
        let (f, len) = if k < 2 { (0, FRONT_BONES.0) } else { (1, HIND_BONES.0) };
        upper[k] = commands.spawn((mesh(&wm.upper[f]), Transform::from_translation(pivot), ChildOf(body))).id();
        lower[k] = commands.spawn((mesh(&wm.lower[f]), Transform::from_xyz(0.0, -len, 0.0), ChildOf(upper[k]))).id();
    }
    commands.entity(root).insert(WolfRig { body, neck, head, tail, upper, lower, phase: 0.0, moving: 0.0, gait: Vec3::Y, last: at.translation, kill_t: 0.0, kill_w: 0.0 });
    root
}

pub(super) fn spawn_gallery_wolves(commands: &mut Commands, wm: &WolfMeshes, m: &Models, centre: Vec3, ground: impl Fn(f32, f32) -> f32) {
    let (yaw, only) = gallery_env();
    for (i, (pace, kill)) in [(0.0, false), (25.0, false), (60.0, false), (200.0, false), (0.0, true)].into_iter().enumerate() {
        if only.is_some_and(|o| o != i) { continue; }
        let x = if only.is_some() { centre.x } else { centre.x + (i as f32 - 2.0) * 2.2 };
        let root = spawn_rig(commands, wm, m, Transform::from_xyz(x, ground(x, centre.z), centre.z).with_rotation(Quat::from_rotation_y(yaw)));
        commands.entity(root).insert(GalleryWolf { pace, kill });
    }
}

#[derive(Clone, Copy)]
struct Pose { body: Transform, neck: Quat, head: Quat, tail: Quat, feet: [Vec2; 4] }

impl Pose {
    fn lerp(self, o: Pose, k: f32) -> Pose {
        Pose {
            body: lerp_t(self.body, o.body, k),
            neck: self.neck.slerp(o.neck, k),
            head: self.head.slerp(o.head, k),
            tail: self.tail.slerp(o.tail, k),
            feet: std::array::from_fn(|i| self.feet[i].lerp(o.feet[i], k)),
        }
    }
}

fn rest_feet() -> [Vec2; 4] { LEG_PIVOTS.map(|p| Vec2::new(0.0, p.z + if p.z > 0.0 { 0.04 } else { -0.02 })) }

const GAITS: GaitTable = [([0.25, 0.75, 0.0, 0.5], 0.62, 1.2, 0.12), ([0.0, 0.5, 0.5, 0.0], 0.42, 1.9, 0.17), ([0.55, 0.65, 0.0, 0.1], 0.3, 2.6, 0.24)];

fn gait_pose(rig: &WolfRig, speed: f32, t: f32) -> Pose {
    let (w, m, c, idle) = (rig.gait, rig.moving, rig.phase * TAU, 1.0 - rig.moving);
    let feet = quadruped_feet(&GAITS, w, m, speed, 1.15, rig.phase, rest_feet(), (0.08, -0.04), 1.1);
    let bob = (-0.02 * w.x - 0.03 * w.y) * (2.0 * c).cos() + 0.06 * w.z * (c + 0.6).sin() - 0.05 * w.z;
    let pitch = 0.11 * w.z * (c - 0.9).sin() + 0.02 * w.x * (2.0 * c).sin();
    let body = Transform::from_xyz(0.0, bob * m + idle * 0.006 * (t * 2.4).sin(), 0.0).with_rotation(ax(pitch * m));
    let sniff = smooth01(0.55, 0.75, (t * 0.21).sin() * 0.5 + 0.5);
    let look = (t * 0.37).sin() * 0.55 + (t * 0.83).sin() * 0.15;
    let neck = Quat::from_rotation_y(idle * look * (1.0 - sniff)) * ax(idle * (-0.1 + 0.85 * sniff) + m * (0.25 * w.x + 0.35 * w.y + 0.6 * w.z + 0.05 * w.x * (2.0 * c).sin()));
    let head = ax(idle * (0.1 + 0.35 * sniff + 0.04 * (t * 7.0).sin() * sniff) + m * (0.05 * w.y - 0.25 * w.z));
    let tail = ax(0.15 + m * (0.15 * w.x + 0.3 * w.y + 0.95 * w.z + 0.05 * (2.0 * c).sin())) * Quat::from_rotation_z((t * 1.3).sin() * 0.1 * idle + c.sin() * 0.12 * m);
    Pose { body, neck, head, tail, feet }
}

fn kill_pose(k: f32) -> Pose {
    let crouch = smooth01(0.0, 0.2, k) * (1.0 - smooth01(0.2, 0.32, k));
    let (leap, air, landed) = (smooth01(0.2, 0.5, k), (PI * smooth01(0.22, 0.5, k)).sin(), smooth01(0.45, 0.6, k));
    let shake = if k > 0.6 { (k * 26.0).sin() * 0.4 * (1.0 - smooth01(1.1, 1.35, k)) } else { 0.0 };
    let body = Transform::from_xyz(0.0, -0.16 * crouch + 0.22 * air - 0.1 * landed, 0.85 * leap).with_rotation(ax(0.1 * crouch - 0.25 * air + 0.22 * landed));
    let mut feet = rest_feet();
    for (i, f) in feet.iter_mut().enumerate() {
        let (s, ahead, high) = if i < 2 { (smooth01(0.2, 0.5, k), 0.95, 0.3) } else { (smooth01(0.3, 0.62, k), 0.85, 0.2) };
        f.y += ahead * s;
        f.x = high * (PI * s).sin();
    }
    Pose {
        body,
        neck: Quat::from_rotation_y(shake * 0.5) * ax(0.15 * crouch - 0.2 * air + 0.75 * landed),
        head: Quat::from_rotation_y(shake) * Quat::from_rotation_z(shake * 0.4) * ax(0.2 * crouch + 0.45 * landed),
        tail: ax(0.2 + 0.9 * air + 0.35 * landed) * Quat::from_rotation_z(shake * 0.3),
        feet,
    }
}

fn animate_wolf(
    time: Res<Time>, sim: Res<Sim>, clock: Option<Res<crate::sim_driver::SimClock>>,
    mut rigs: Query<(&mut WolfRig, &Transform, Option<&GalleryWolf>, Option<&SimWolf>)>,
    mut parts: Query<&mut Transform, Without<WolfRig>>,
) {
    let (dt, t, paused) = (time.delta_secs().max(1e-4), time.elapsed_secs(), clock.is_some_and(|c| c.paused));
    for (mut rig, root, gallery, id) in &mut rigs {
        let (pace, killing) = match (gallery, id.and_then(|id| sim.0.predators.wolf(id.0))) {
            (Some(g), _) => (g.pace, g.kill),
            (None, Some(w)) => (w.pace, matches!(w.state, WolfState::Kill { .. } | WolfState::Feed { .. })),
            (None, None) => continue,
        };
        let speed = pace * BASE_DAYS_PER_SECOND / WOLF_SIZE;
        let moved = root.translation.xz().distance(rig.last.xz()) / dt;
        rig.last = root.translation;
        ease_toward(&mut rig.moving, speed > 0.1 && (gallery.is_some() || (!paused && moved > 0.05)), 5.0, dt);
        let (walk, gallop) = (1.0 - smooth01(1.2, 2.4, speed), smooth01(6.0, 8.5, speed));
        rig.gait = rig.gait.lerp(Vec3::new(walk, 1.0 - walk - gallop, gallop), 1.0 - (-dt * 5.0).exp());
        rig.phase = (rig.phase + dt * gait_mix(&GAITS, rig.gait, |g| g.2) * rig.moving).rem_euclid(1.0);
        if killing {
            rig.kill_t = if rig.kill_w < 0.01 { 0.0 } else { rig.kill_t + dt };
            if gallery.is_some() { rig.kill_t %= 2.2; }
        }
        ease_toward(&mut rig.kill_w, killing, if killing { 14.0 } else { 4.0 }, dt);
        let mut pose = gait_pose(&rig, speed, t);
        if rig.kill_w > 0.001 { pose = pose.lerp(kill_pose(rig.kill_t), rig.kill_w); }
        if let Ok(mut tr) = parts.get_mut(rig.body) { *tr = pose.body; }
        let mut set_rot = |e: Entity, rot: Quat| if let Ok(mut tr) = parts.get_mut(e) { tr.rotation = rot; };
        set_rot(rig.neck, pose.neck);
        set_rot(rig.head, pose.head);
        set_rot(rig.tail, pose.tail);
        for (k, (up, low)) in quadruped_legs(pose.body, LEG_PIVOTS, FRONT_BONES, HIND_BONES, pose.feet).into_iter().enumerate() {
            set_rot(rig.upper[k], ax(up));
            set_rot(rig.lower[k], ax(low));
        }
    }
}
