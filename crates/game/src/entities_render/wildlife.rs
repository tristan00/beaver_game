//! Wild animals (deer, rabbits, birds): procedural rigs, sim tracking, and pose animation.
use super::meshgen::{build_mesh, hex, mix, v, Mb};
use super::sim_view::cell_xz;
use super::wolf::{ax, coat as base_coat, ease_toward, env_floats, gait_mix, gallery_env, ground_pt, lerp_t, quadruped_feet, quadruped_legs, GaitTable, WolfTrack};
use super::Models;
use crate::seasonal::smooth as smooth01;
use crate::state::Sim;
use bevy::prelude::*;
use sim::wildlife::{Doing, Food, Kind};
use std::f32::consts::{PI, TAU};

const DEER_SIZE: f32 = 1.0;
const RABBIT_SIZE: f32 = 0.32;
const BIRD_SIZE: f32 = 0.36;

const D_NECK: Vec3 = Vec3::new(0.0, 0.95, 0.5);
const D_HEAD: Vec3 = Vec3::new(0.0, 0.52, 0.28);
const D_EAR: Vec3 = Vec3::new(0.06, 0.1, 0.0);
const D_TAIL: Vec3 = Vec3::new(0.0, 0.98, -0.63);
const D_LEGS: [Vec3; 4] = [Vec3::new(-0.11, 0.86, 0.42), Vec3::new(0.11, 0.86, 0.42), Vec3::new(-0.11, 0.9, -0.48), Vec3::new(0.11, 0.9, -0.48)];
const D_FRONT: (f32, f32) = (0.38, 0.52);
const D_HIND: (f32, f32) = (0.42, 0.54);
const D_CENTRE: Vec3 = Vec3::new(0.0, 0.92, 0.0);

const R_HEAD: Vec3 = Vec3::new(0.0, 0.5, 0.3);
const R_EAR: Vec3 = Vec3::new(0.05, 0.15, 0.02);
const R_HIND: [Vec3; 2] = [Vec3::new(-0.15, 0.26, -0.22), Vec3::new(0.15, 0.26, -0.22)];
const R_FRONT: [Vec3; 2] = [Vec3::new(-0.08, 0.24, 0.22), Vec3::new(0.08, 0.24, 0.22)];
const R_CENTRE: Vec3 = Vec3::new(0.0, 0.33, 0.0);

const B_HEAD: Vec3 = Vec3::new(0.0, 0.62, 0.24);
const B_TAIL: Vec3 = Vec3::new(0.0, 0.48, -0.28);
const B_WINGS: [Vec3; 2] = [Vec3::new(-0.17, 0.56, 0.06), Vec3::new(0.17, 0.56, 0.06)];
const B_LEGS: [Vec3; 2] = [Vec3::new(-0.07, 0.3, 0.02), Vec3::new(0.07, 0.3, 0.02)];
const FLY_HEIGHT: f32 = 3.5;

pub struct WildlifePlugin;

impl Plugin for WildlifePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, init_meshes).add_systems(Update, (drive, follow).chain());
    }
}

#[derive(Resource)]
pub(super) struct WildMeshes {
    deer_body: Handle<Mesh>,
    deer_neck: Handle<Mesh>,
    deer_head: [Handle<Mesh>; 2],
    deer_ear: Handle<Mesh>,
    deer_tail: Handle<Mesh>,
    deer_upper: [Handle<Mesh>; 2],
    deer_lower: [Handle<Mesh>; 2],
    rabbit_body: Handle<Mesh>,
    rabbit_head: Handle<Mesh>,
    rabbit_ear: Handle<Mesh>,
    rabbit_hind: Handle<Mesh>,
    rabbit_front: Handle<Mesh>,
    bird_body: [Handle<Mesh>; 2],
    bird_head: [Handle<Mesh>; 2],
    bird_tail: [Handle<Mesh>; 2],
    bird_wing: [[Handle<Mesh>; 2]; 2],
    bird_leg: Handle<Mesh>,
}

#[derive(Component)]
struct Beast {
    kind: Kind,
    size: f32,
    parts: [Entity; 14],
    phase: f32,
    moving: f32,
    gait: Vec3,
    speed: f32,
    last: Option<Vec2>,
    alt: f32,
    flap: f32,
    feed: f32,
    rest: f32,
    down: f32,
    seed: f32,
}

#[derive(Component)]
struct Wild(u32);

#[derive(Component)]
struct Carcass(u64);

#[derive(Component)]
pub(super) struct GalleryBeast { speed: f32, doing: Doing, alt: f32, dead: bool }

fn hash(n: u32) -> f32 {
    let mut x = n.wrapping_mul(0x9E37_79B9) ^ 0x85EB_CA6B;
    x ^= x >> 15;
    x = x.wrapping_mul(0x2C1B_3C6D);
    ((x ^ (x >> 12)) & 0xFFFF) as f32 / 65535.0
}

fn coat(p: Vec3, n: Vec3, top: [f32; 3], side: [f32; 3], under: [f32; 3]) -> [f32; 3] {
    base_coat(p, n, top, side, under, (53.0, 47.0, 43.0), 0.04)
}

// ---- Deer meshes ----

fn deer_coat(p: Vec3, n: Vec3) -> [f32; 3] { coat(p, n, hex(0x7a5233), hex(0xa2724a), hex(0xe9e1d3)) }

fn deer_body(mb: &mut Mb) {
    let start = mb.len();
    mb.loft(&[
        (v(0.0, 0.95, -0.67), 0.02, 0.02), (v(0.0, 0.95, -0.63), 0.1, 0.12), (v(0.0, 0.95, -0.53), 0.15, 0.19),
        (v(0.0, 0.93, -0.36), 0.165, 0.21), (v(0.0, 0.9, -0.15), 0.165, 0.215), (v(0.0, 0.9, 0.1), 0.175, 0.23),
        (v(0.0, 0.93, 0.32), 0.165, 0.225), (v(0.0, 0.97, 0.48), 0.135, 0.19), (v(0.0, 1.01, 0.58), 0.08, 0.11),
        (v(0.0, 1.03, 0.62), 0.02, 0.02),
    ], 16, [0.0; 3]);
    mb.paint(start, |p, n, _| if p.z < -0.56 && n.z < -0.3 && p.y < 1.0 { hex(0xe9e1d3) } else { deer_coat(p, n) });
}

fn deer_neck(mb: &mut Mb) {
    let (up, start) = (D_HEAD, mb.len());
    mb.loft(&[(v(0.0, -0.08, -0.1), 0.03, 0.03), (v(0.0, -0.04, -0.05), 0.11, 0.16), (up * 0.3, 0.09, 0.12), (up * 0.7, 0.075, 0.09), (up * 1.0, 0.07, 0.08), (up * 1.08, 0.03, 0.03)], 12, [0.0; 3]);
    mb.paint(start, |p, n, _| if n.z > 0.5 && p.y > 0.3 { hex(0xe9e1d3) } else { deer_coat(p, n) });
}

fn deer_head(mb: &mut Mb, antlers: bool) {
    let (brown, dark, pale, start) = (hex(0x8a5d3a), hex(0x4a3324), hex(0xe9e1d3), mb.len());
    mb.ellipsoid_g(v(0.0, 0.04, 0.04), v(0.085, 0.09, 0.11), brown, brown, 12, 8);
    mb.loft(&[(v(0.0, 0.05, 0.04), 0.03, 0.03), (v(0.0, 0.04, 0.09), 0.075, 0.075), (v(0.0, 0.0, 0.2), 0.055, 0.055), (v(0.0, -0.03, 0.28), 0.045, 0.042), (v(0.0, -0.035, 0.31), 0.02, 0.02)], 10, brown);
    mb.paint(start, |p, n, c| if n.y < -0.3 && p.z > 0.08 { pale } else { mix(c, deer_coat(p, n), 0.4) });
    mb.ellipsoid(v(0.0, -0.02, 0.305), v(0.035, 0.028, 0.02), hex(0x161210));
    mb.ellipsoid_r(v(0.0, -0.005, 0.25), v(0.05, 0.04, 0.03), Quat::IDENTITY, pale);
    for s in [-1.0f32, 1.0] {
        mb.ellipsoid(v(0.07 * s, 0.06, 0.08), v(0.022, 0.022, 0.025), hex(0x120e0c));
        mb.ellipsoid(v(0.072 * s, 0.067, 0.09), v(0.006, 0.006, 0.006), hex(0xffffff));
    }
    if antlers {
        let bone = hex(0xcbb894);
        for s in [-1.0f32, 1.0] {
            let (p0, p1, p2, p3) = (v(0.045 * s, 0.11, 0.02), v(0.13 * s, 0.22, -0.04), v(0.17 * s, 0.32, 0.04), v(0.12 * s, 0.38, 0.16));
            mb.cyl(p0, p1, 0.02, 0.017, dark, bone, 5);
            mb.cyl(p1, p2, 0.017, 0.014, bone, bone, 5);
            mb.cyl(p2, p3, 0.014, 0.006, bone, bone, 5);
            for (a, b) in [(p1.lerp(p2, 0.2), v(0.1 * s, 0.38, -0.02)), (p2.lerp(p3, 0.2), v(0.15 * s, 0.47, 0.06)), (p0.lerp(p1, 0.35), v(0.06 * s, 0.2, 0.1))] {
                mb.cyl(a, b, 0.011, 0.004, bone, bone, 4);
            }
        }
    }
}

fn deer_ear(mb: &mut Mb) {
    mb.ellipsoid_r(v(0.0, 0.09, 0.0), v(0.04, 0.1, 0.018), Quat::IDENTITY, hex(0x8a5d3a));
    mb.ellipsoid_r(v(0.0, 0.09, 0.012), v(0.026, 0.075, 0.008), Quat::IDENTITY, hex(0xd8c4b0));
}

fn deer_tail(mb: &mut Mb) {
    let start = mb.len();
    mb.loft(&[(v(0.0, 0.02, 0.0), 0.02, 0.02), (v(0.0, 0.0, -0.02), 0.055, 0.04), (v(0.0, -0.08, -0.05), 0.065, 0.045), (v(0.0, -0.16, -0.05), 0.04, 0.03), (v(0.0, -0.19, -0.045), 0.01, 0.01)], 10, [0.0; 3]);
    mb.paint(start, |_, n, _| if n.z > 0.1 { hex(0xf3eee6) } else { hex(0x7a5233) });
}

fn deer_upper(mb: &mut Mb, front: bool) {
    let (len, start) = (if front { D_FRONT.0 } else { D_HIND.0 }, mb.len());
    if front {
        mb.loft(&[(v(0.0, 0.07, 0.02), 0.02, 0.02), (v(0.0, 0.03, 0.01), 0.07, 0.1), (v(0.0, -0.14, 0.0), 0.06, 0.08), (v(0.0, -len, 0.0), 0.035, 0.04), (v(0.0, -len - 0.04, 0.0), 0.015, 0.015)], 10, [0.0; 3]);
    } else {
        mb.loft(&[(v(0.0, 0.09, -0.02), 0.02, 0.02), (v(0.0, 0.04, -0.03), 0.085, 0.14), (v(0.0, -0.14, 0.0), 0.075, 0.11), (v(0.0, -len, 0.02), 0.04, 0.045), (v(0.0, -len - 0.04, 0.02), 0.015, 0.015)], 10, [0.0; 3]);
    }
    mb.paint(start, |p, n, _| coat(p, n, hex(0x7a5233), hex(0xa2724a), hex(0xa98058)));
}

fn deer_lower(mb: &mut Mb, front: bool) {
    let (len, leg, start) = (if front { D_FRONT.1 } else { D_HIND.1 }, hex(0xb08a64), mb.len());
    if front {
        mb.loft(&[(v(0.0, 0.04, 0.0), 0.015, 0.015), (v(0.0, 0.0, 0.0), 0.035, 0.04), (v(0.0, -0.2, 0.0), 0.026, 0.03), (v(0.0, -len + 0.08, 0.0), 0.022, 0.026), (v(0.0, -len + 0.04, 0.01), 0.028, 0.03), (v(0.0, -len + 0.02, 0.01), 0.01, 0.01)], 8, leg);
    } else {
        mb.loft(&[(v(0.0, 0.04, 0.01), 0.015, 0.015), (v(0.0, 0.0, 0.0), 0.04, 0.05), (v(0.0, -0.22, -0.1), 0.026, 0.035), (v(0.0, -len + 0.08, -0.01), 0.022, 0.026), (v(0.0, -len + 0.04, 0.0), 0.028, 0.03), (v(0.0, -len + 0.02, 0.0), 0.01, 0.01)], 8, leg);
    }
    mb.paint(start, |p, n, c| mix(coat(p, n, hex(0x7a5233), hex(0xa2724a), hex(0xa98058)), c, smooth01(-0.02, -0.15, p.y)));
    mb.ellipsoid(v(0.0, -len + 0.022, 0.02), v(0.03, 0.024, 0.04), hex(0x1d1713));
}

// ---- Rabbit & bird meshes ----

fn rabbit_coat(p: Vec3, n: Vec3) -> [f32; 3] { coat(p, n, hex(0x6d5b48), hex(0x9a8670), hex(0xdcd3c4)) }

fn rabbit_body(mb: &mut Mb) {
    let start = mb.len();
    mb.loft(&[(v(0.0, 0.3, -0.43), 0.02, 0.02), (v(0.0, 0.32, -0.39), 0.15, 0.17), (v(0.0, 0.34, -0.26), 0.23, 0.26), (v(0.0, 0.34, -0.06), 0.21, 0.23), (v(0.0, 0.36, 0.14), 0.16, 0.19), (v(0.0, 0.42, 0.27), 0.11, 0.14), (v(0.0, 0.46, 0.32), 0.03, 0.03)], 14, [0.0; 3]);
    mb.paint(start, |p, n, _| rabbit_coat(p, n));
    mb.ellipsoid(v(0.0, 0.42, -0.43), v(0.08, 0.08, 0.06), hex(0xf4f1ea));
}

fn rabbit_head(mb: &mut Mb) {
    let start = mb.len();
    mb.ellipsoid_g(v(0.0, 0.05, 0.08), v(0.13, 0.13, 0.17), [0.0; 3], [0.0; 3], 12, 8);
    mb.ellipsoid(v(0.0, -0.01, 0.2), v(0.085, 0.075, 0.075), [0.0; 3]);
    mb.paint(start, |p, n, _| if p.y < -0.03 && p.z > 0.1 { hex(0xe6ddcf) } else { rabbit_coat(p, n) });
    mb.ellipsoid(v(0.0, 0.0, 0.272), v(0.025, 0.018, 0.012), hex(0xb07a78));
    for s in [-1.0f32, 1.0] {
        mb.ellipsoid(v(0.1 * s, 0.08, 0.11), v(0.035, 0.04, 0.04), hex(0x120d0b));
        mb.ellipsoid(v(0.112 * s, 0.09, 0.125), v(0.009, 0.009, 0.009), hex(0xffffff));
    }
}

fn rabbit_ear(mb: &mut Mb) {
    mb.ellipsoid_r(v(0.0, 0.17, 0.0), v(0.05, 0.18, 0.022), Quat::IDENTITY, hex(0x7a6752));
    mb.ellipsoid_r(v(0.0, 0.17, 0.014), v(0.032, 0.14, 0.01), Quat::IDENTITY, hex(0xc9a29a));
    mb.ellipsoid_r(v(0.0, 0.32, 0.0), v(0.03, 0.035, 0.018), Quat::IDENTITY, hex(0x3b2f25));
}

fn rabbit_hind(mb: &mut Mb) {
    let start = mb.len();
    mb.ellipsoid_g(v(0.0, -0.04, 0.0), v(0.075, 0.14, 0.14), [0.0; 3], [0.0; 3], 10, 7);
    mb.paint(start, |p, n, _| coat(p, n, hex(0x6d5b48), hex(0x8d7a64), hex(0x9a8670)));
    mb.ellipsoid_r(v(0.0, -0.235, 0.05), v(0.05, 0.03, 0.15), Quat::IDENTITY, hex(0xc8bba8));
}

fn rabbit_front(mb: &mut Mb) {
    mb.cyl(v(0.0, 0.02, 0.0), v(0.0, -0.21, 0.02), 0.038, 0.03, hex(0x9a8670), hex(0xc8bba8), 6);
    mb.ellipsoid(v(0.0, -0.225, 0.04), v(0.035, 0.02, 0.05), hex(0xc8bba8));
}

fn plumage(look: usize) -> ([f32; 3], [f32; 3], [f32; 3], [f32; 3]) {
    if look == 0 { (hex(0x3b6cb5), hex(0x6f96cc), hex(0xe7e9ea), hex(0x17191c)) }
    else { (hex(0x4f4842), hex(0x6d645b), hex(0xcc6a2d), hex(0x2b2725)) }
}

fn bird_body(mb: &mut Mb, look: usize) {
    let ((top, side, belly, _), start) = (plumage(look), mb.len());
    mb.ellipsoid_m(Mat4::from_translation(v(0.0, 0.45, 0.0)) * Mat4::from_quat(ax(-0.25)) * Mat4::from_scale(v(0.2, 0.2, 0.34)), 14, 9, [0.0; 3], [0.0; 3]);
    mb.paint(start, |p, n, _| {
        let c = coat(p, n, top, side, belly);
        if look == 1 && n.z > 0.1 && n.y < 0.45 { mix(c, belly, 0.9) } else if look == 1 { mix(c, hex(0xd8d0c4), smooth01(-0.3, -0.8, n.y)) } else { c }
    });
}

fn bird_head(mb: &mut Mb, look: usize) {
    let ((top, side, belly, dark), start) = (plumage(look), mb.len());
    mb.ellipsoid_g(v(0.0, 0.06, 0.06), v(0.14, 0.14, 0.15), [0.0; 3], [0.0; 3], 12, 8);
    mb.paint(start, |p, n, _| if look == 0 && n.y < -0.2 { belly } else if look == 1 { mix(dark, side, 0.25) } else { coat(p, n, top, side, side) });
    if look == 0 {
        mb.cyl(v(0.0, 0.16, 0.02), v(0.0, 0.27, -0.1), 0.07, 0.01, top, top, 6);
        mb.ellipsoid_r(v(0.0, -0.04, 0.06), v(0.15, 0.03, 0.12), Quat::IDENTITY, dark);
    }
    let beak = if look == 0 { hex(0x1c1c1c) } else { hex(0xd9a520) };
    mb.cyl(v(0.0, 0.04, 0.17), v(0.0, 0.02, 0.33), 0.045, 0.004, beak, beak, 6);
    for s in [-1.0f32, 1.0] {
        mb.ellipsoid(v(0.095 * s, 0.09, 0.12), v(0.025, 0.025, 0.025), hex(0x0d0d0d));
        if look == 1 { mb.ellipsoid(v(0.1 * s, 0.11, 0.12), v(0.02, 0.008, 0.02), hex(0xf0ece4)); }
    }
}

fn bird_tail(mb: &mut Mb, look: usize) {
    let ((top, side, _, dark), start) = (plumage(look), mb.len());
    mb.ellipsoid_r(v(0.0, -0.02, -0.2), v(0.12, 0.025, 0.22), ax(-0.1), top);
    mb.paint(start, |p, _, c| if look == 0 && p.z < -0.36 { hex(0xf2f2f2) } else if p.z < -0.3 { mix(c, dark, 0.4) } else { mix(c, side, 0.2) });
}

fn bird_wing(mb: &mut Mb, look: usize, s: f32) {
    let ((top, side, _, dark), start) = (plumage(look), mb.len());
    mb.ellipsoid_r(v(0.22 * s, 0.0, -0.04), v(0.24, 0.025, 0.14), Quat::from_rotation_y(0.15 * s), top);
    mb.ellipsoid_r(v(0.37 * s, 0.0, -0.1), v(0.13, 0.018, 0.08), Quat::from_rotation_y(0.5 * s), dark);
    mb.paint(start, |p, _, c| {
        if look == 0 && (p.x.abs() * 40.0).sin() > 0.75 && p.x.abs() > 0.12 { dark } else if p.x.abs() < 0.12 { mix(c, side, 0.5) } else { c }
    });
}

fn bird_leg(mb: &mut Mb) {
    let leg = hex(0x5b4a3e);
    mb.cyl(v(0.0, 0.02, 0.0), v(0.0, -0.3, 0.02), 0.016, 0.012, leg, leg, 5);
    for a in [-0.5f32, 0.0, 0.5] { mb.cyl(v(0.0, -0.3, 0.02), v(0.08 * a.sin(), -0.3, 0.02 + 0.08 * a.cos()), 0.01, 0.006, leg, leg, 4); }
    mb.cyl(v(0.0, -0.3, 0.02), v(0.0, -0.3, -0.05), 0.01, 0.006, leg, leg, 4);
}

fn init_meshes(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>) {
    commands.insert_resource(WildMeshes {
        deer_body: build_mesh(&mut meshes, deer_body),
        deer_neck: build_mesh(&mut meshes, deer_neck),
        deer_head: [build_mesh(&mut meshes, |m| deer_head(m, false)), build_mesh(&mut meshes, |m| deer_head(m, true))],
        deer_ear: build_mesh(&mut meshes, deer_ear),
        deer_tail: build_mesh(&mut meshes, deer_tail),
        deer_upper: [build_mesh(&mut meshes, |m| deer_upper(m, true)), build_mesh(&mut meshes, |m| deer_upper(m, false))],
        deer_lower: [build_mesh(&mut meshes, |m| deer_lower(m, true)), build_mesh(&mut meshes, |m| deer_lower(m, false))],
        rabbit_body: build_mesh(&mut meshes, rabbit_body),
        rabbit_head: build_mesh(&mut meshes, rabbit_head),
        rabbit_ear: build_mesh(&mut meshes, rabbit_ear),
        rabbit_hind: build_mesh(&mut meshes, rabbit_hind),
        rabbit_front: build_mesh(&mut meshes, rabbit_front),
        bird_body: [build_mesh(&mut meshes, |m| bird_body(m, 0)), build_mesh(&mut meshes, |m| bird_body(m, 1))],
        bird_head: [build_mesh(&mut meshes, |m| bird_head(m, 0)), build_mesh(&mut meshes, |m| bird_head(m, 1))],
        bird_tail: [build_mesh(&mut meshes, |m| bird_tail(m, 0)), build_mesh(&mut meshes, |m| bird_tail(m, 1))],
        bird_wing: [[build_mesh(&mut meshes, |m| bird_wing(m, 0, -1.0)), build_mesh(&mut meshes, |m| bird_wing(m, 0, 1.0))], [build_mesh(&mut meshes, |m| bird_wing(m, 1, -1.0)), build_mesh(&mut meshes, |m| bird_wing(m, 1, 1.0))]],
        bird_leg: build_mesh(&mut meshes, bird_leg),
    });
}

// ---- Spawning & driving ----

fn spawn(commands: &mut Commands, wm: &WildMeshes, m: &Models, kind: Kind, id: u32, at: Transform) -> Entity {
    let part = |commands: &mut Commands, h: &Handle<Mesh>, pos: Vec3, parent: Entity| {
        commands.spawn((Mesh3d(h.clone()), MeshMaterial3d(m.white.clone()), Transform::from_translation(pos), ChildOf(parent))).id()
    };
    let (base, blob) = match kind {
        Kind::Deer => (DEER_SIZE, Vec3::new(0.32, 1.0, 1.0)),
        Kind::Rabbit => (RABBIT_SIZE, Vec3::new(0.35, 1.0, 0.6)),
        Kind::Bird => (BIRD_SIZE, Vec3::new(0.25, 1.0, 0.35)),
    };
    let size = base * (0.9 + 0.2 * hash(id));
    let root = commands.spawn((at.with_scale(Vec3::splat(size)), Visibility::Inherited)).id();
    commands.spawn((Mesh3d(m.blob.clone()), MeshMaterial3d(m.blob_mat.clone()), Transform::from_xyz(0.0, 0.03, 0.0).with_scale(blob), ChildOf(root)));
    let mut parts = [Entity::PLACEHOLDER; 14];
    let body = commands.spawn((Transform::default(), Visibility::Inherited, ChildOf(root))).id();
    parts[0] = body;
    match kind {
        Kind::Deer => {
            part(commands, &wm.deer_body, Vec3::ZERO, body);
            parts[1] = part(commands, &wm.deer_neck, D_NECK, body);
            parts[2] = part(commands, &wm.deer_head[(id % 3 == 0) as usize], D_HEAD, parts[1]);
            for (k, s) in [-1.0f32, 1.0].into_iter().enumerate() { parts[3 + k] = part(commands, &wm.deer_ear, D_EAR * Vec3::new(s, 1.0, 1.0), parts[2]); }
            parts[5] = part(commands, &wm.deer_tail, D_TAIL, body);
            for (k, &pivot) in D_LEGS.iter().enumerate() {
                let (f, len) = if k < 2 { (0, D_FRONT.0) } else { (1, D_HIND.0) };
                parts[6 + k] = part(commands, &wm.deer_upper[f], pivot, body);
                parts[10 + k] = part(commands, &wm.deer_lower[f], Vec3::new(0.0, -len, 0.0), parts[6 + k]);
            }
        }
        Kind::Rabbit => {
            part(commands, &wm.rabbit_body, Vec3::ZERO, body);
            parts[1] = part(commands, &wm.rabbit_head, R_HEAD, body);
            for (k, s) in [-1.0f32, 1.0].into_iter().enumerate() {
                parts[2 + k] = part(commands, &wm.rabbit_ear, R_EAR * Vec3::new(s, 1.0, 1.0), parts[1]);
                parts[4 + k] = part(commands, &wm.rabbit_hind, R_HIND[k], body);
                parts[6 + k] = part(commands, &wm.rabbit_front, R_FRONT[k], body);
            }
        }
        Kind::Bird => {
            let look = (id % 2) as usize;
            part(commands, &wm.bird_body[look], Vec3::ZERO, body);
            parts[1] = part(commands, &wm.bird_head[look], B_HEAD, body);
            parts[2] = part(commands, &wm.bird_tail[look], B_TAIL, body);
            for k in 0..2 {
                parts[3 + k] = part(commands, &wm.bird_wing[look][k], B_WINGS[k], body);
                parts[5 + k] = part(commands, &wm.bird_leg, B_LEGS[k], body);
            }
        }
    }
    commands.entity(root).insert(Beast {
        kind, size, parts, phase: hash(id ^ 77), moving: 0.0, gait: Vec3::X, speed: 0.0, last: None,
        alt: 0.0, flap: 0.0, feed: 0.0, rest: 0.0, down: 0.0, seed: hash(id ^ 0xbeef) * 100.0,
    });
    root
}

pub(super) fn spawn_gallery(commands: &mut Commands, wm: &WildMeshes, m: &Models, centre: Vec3, ground: impl Fn(f32, f32) -> f32) {
    let (yaw, only) = gallery_env();
    let (feed, rest) = (Doing::Feed { food: Food::Pile(0), until: 0.0 }, Doing::Rest { until: 0.0 });
    let rows: [(Kind, f32, &[(f32, Doing, f32, bool, u32)]); 3] = [
        (Kind::Deer, -3.0, &[(0.0, Doing::Wander, 0.0, false, 1), (1.5, Doing::Wander, 0.0, false, 1), (3.4, Doing::Wander, 0.0, false, 1), (10.0, Doing::Flee, 0.0, false, 1), (0.0, feed, 0.0, false, 1), (0.0, rest, 0.0, false, 1), (0.0, Doing::Caught, 0.0, false, 1), (0.0, Doing::Caught, 0.0, true, 1), (1.5, Doing::Wander, 0.0, false, 3)]),
        (Kind::Rabbit, 0.0, &[(0.0, Doing::Wander, 0.0, false, 1), (2.0, Doing::Wander, 0.0, false, 1), (9.0, Doing::Flee, 0.0, false, 1), (0.0, feed, 0.0, false, 1), (0.0, rest, 0.0, false, 1), (0.0, rest, 0.0, false, 2), (0.0, Doing::Caught, 0.0, false, 1)]),
        (Kind::Bird, 1.8, &[(0.0, Doing::Wander, 0.0, false, 1), (1.0, Doing::Wander, 0.0, false, 2), (0.0, feed, 0.0, false, 1), (0.0, rest, 0.0, false, 2), (8.0, Doing::Flee, 1.2, false, 1), (8.0, Doing::Flee, 1.2, false, 2), (0.0, Doing::Wander, 0.6, false, 1)]),
    ];
    let mut n = 0;
    for (kind, dz, entries) in rows {
        let spacing = if kind == Kind::Deer { 2.0 } else { 0.8 };
        for (i, &(speed, doing, alt, dead, id)) in entries.iter().enumerate() {
            n += 1;
            if only.is_some_and(|o| o != n - 1) { continue; }
            let (x, z) = if only.is_some() { (centre.x, centre.z) } else { (centre.x + (i as f32 - entries.len() as f32 / 2.0) * spacing, centre.z + dz) };
            let e = spawn(commands, wm, m, kind, id, Transform::from_xyz(x, ground(x, z), z).with_rotation(Quat::from_rotation_y(yaw)));
            commands.entity(e).insert(GalleryBeast { speed, doing, alt, dead });
        }
    }
}

fn carcass_key(c: &sim::wildlife::Carcass) -> u64 {
    c.died.to_bits() ^ ((c.pos.0.to_bits() as u64) << 32) ^ c.pos.1.to_bits() as u64
}

#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn drive(
    mut commands: Commands, sim: Res<Sim>, time: Res<Time>, clock: Option<Res<crate::sim_driver::SimClock>>,
    meshes: Option<Res<WildMeshes>>, models: Option<Res<Models>>,
    mut beasts: Query<(Entity, &mut Beast, &mut Transform, &mut Visibility, Option<&mut WolfTrack>, Option<&Wild>, Option<&Carcass>, Option<&GalleryBeast>)>,
    mut parts: Query<&mut Transform, Without<Beast>>,
) {
    let _p = ::sim::prof::scope("game/wildlife");
    let (Some(wm), Some(m)) = (meshes, models) else { return };
    let (s, wl, dt, t) = (&sim.0, &sim.0.wildlife, time.delta_secs().clamp(1e-4, 0.1), time.elapsed_secs());
    let smoothing = clock.as_ref().map_or(1.0, |c| c.speed.smoothing());
    let now = clock.as_ref().map_or(s.time_days, |c| c.render_days(s.time_days));
    let (mut seen, mut seen_dead) = (Vec::with_capacity(wl.animals.len()), Vec::with_capacity(wl.carcasses.len()));
    for (e, mut b, mut tr, mut vis, track, wild, carcass, gallery) in &mut beasts {
        let (speed, doing, dead, alt) = if let Some(g) = gallery {
            (g.speed, g.doing, g.dead, Some(g.alt))
        } else if let Some(&Carcass(key)) = carcass {
            let Some(c) = wl.carcasses.iter().find(|c| carcass_key(c) == key) else { commands.entity(e).despawn(); continue; };
            seen_dead.push(key);
            let age = ((s.time_days - c.died) / sim::wildlife::CARCASS_DAYS) as f32;
            tr.translation = ground_pt(s, cell_xz(s, c.pos)) - Vec3::Y * 0.3 * b.size * smooth01(0.8, 1.0, age);
            (0.0, Doing::Caught, true, None)
        } else if let (Some(&Wild(id)), Some(mut track)) = (wild, track) {
            let Some(a) = wl.get(id) else { commands.entity(e).despawn(); continue; };
            seen.push(id);
            let (xz, going) = track.advance(s, &a.track, a.pos, 60.0, 6.0, now, dt, smoothing);
            tr.translation = ground_pt(s, xz);
            let raw = b.last.map_or(0.0, |l| l.distance(xz) / dt);
            b.last = Some(xz);
            b.speed += (raw - b.speed) * crate::sim_driver::ease_k(dt, 6.0, smoothing);
            let want = Quat::from_rotation_y(going.map_or(a.heading, |d| d.x.atan2(d.y)));
            tr.rotation = tr.rotation.slerp(want, crate::sim_driver::ease_k(dt, 8.0, smoothing));
            (b.speed, a.doing, false, None)
        } else { continue; };
        if gallery.is_none() {
            let cell = crate::flow::world_cell(&s.height, tr.translation);
            let hide = crate::fog_render::unexplored(s, (cell.0.max(0.0) as usize, cell.1.max(0.0) as usize));
            vis.set_if_neq(if hide { Visibility::Hidden } else { Visibility::Inherited });
            if hide { continue; }
        }
        animate(&mut b, speed, doing, dead, alt, t, dt, &mut parts);
    }
    for a in wl.animals.iter().filter(|a| !seen.contains(&a.id)) {
        let at = ground_pt(s, cell_xz(s, a.pos));
        let root = spawn(&mut commands, &wm, &m, a.kind, a.id, Transform::from_translation(at).with_rotation(Quat::from_rotation_y(a.heading)));
        let mut track = WolfTrack::default();
        track.record_with(s.instance, s.time_days, at.xz(), 60.0);
        commands.entity(root).insert((Wild(a.id), track));
    }
    for c in &wl.carcasses {
        let key = carcass_key(c);
        if seen_dead.contains(&key) { continue; }
        let at = ground_pt(s, cell_xz(s, c.pos));
        let root = spawn(&mut commands, &wm, &m, c.kind, (key as u32) ^ (key >> 32) as u32, Transform::from_translation(at).with_rotation(Quat::from_rotation_y(c.heading)));
        commands.entity(root).insert(Carcass(key));
    }
}

fn follow(mut orbit: ResMut<crate::camera::OrbitCamera>, beasts: Query<(&Transform, &Beast, &Wild)>, mut spec: Local<Option<Vec<f32>>>) {
    let p = spec.get_or_insert_with(|| env_floats("BC_WILD_CAM"));
    if p.len() < 2 { return; }
    let kind = Kind::ALL[(p[0] as usize).min(2)];
    let Some((tr, b, _)) = beasts.iter().filter(|(_, b, _)| b.kind == kind).min_by_key(|(_, _, w)| w.0) else { return };
    orbit.target = tr.translation + Vec3::Y * b.alt;
    orbit.distance = p[1];
    if let Some(&y) = p.get(2) { orbit.yaw = y; }
    if let Some(&pt) = p.get(3) { orbit.pitch = pt; }
}

// ---- Pose animation ----

#[allow(clippy::too_many_arguments)]
fn animate(b: &mut Beast, speed: f32, doing: Doing, dead: bool, alt: Option<f32>, t: f32, dt: f32, parts: &mut Query<&mut Transform, Without<Beast>>) {
    let first = b.down == 0.0 && dead;
    ease_toward(&mut b.feed, matches!(doing, Doing::Feed { .. }) && speed < 0.3, 3.0, dt);
    ease_toward(&mut b.rest, matches!(doing, Doing::Rest { .. } | Doing::Digest { .. }) && speed < 0.3, 1.6, dt);
    ease_toward(&mut b.down, doing == Doing::Caught, 5.0, dt);
    if first { b.down = 1.0; }
    let (local, tt) = (speed / b.size, t + b.seed);
    let mut set = |i: usize, tr: Transform| if let Ok(mut p) = parts.get_mut(b.parts[i]) { *p = tr; };
    match b.kind {
        Kind::Deer => {
            ease_toward(&mut b.moving, local > 0.15, 5.0, dt);
            let (walk, bound) = (1.0 - smooth01(2.0, 3.0, local), smooth01(4.5, 6.5, local));
            b.gait = b.gait.lerp(Vec3::new(walk, 1.0 - walk - bound, bound), 1.0 - (-dt * 5.0).exp());
            b.phase = (b.phase + dt * gait_mix(&DEER_GAITS, b.gait, |g| g.2) * b.moving).rem_euclid(1.0);
            let mut pose = deer_gait(b, local, tt);
            if b.feed > 0.001 { pose = pose.lerp(deer_graze(tt), b.feed); }
            if b.rest > 0.001 { pose = pose.lerp(deer_lie(tt), b.rest); }
            if b.down > 0.001 { pose = pose.lerp(deer_down(tt, dead), b.down); }
            set(0, pose.body);
            set(1, Transform::from_translation(D_NECK).with_rotation(pose.neck));
            set(2, Transform::from_translation(D_HEAD).with_rotation(pose.head));
            for k in 0..2 { let s = if k == 0 { -1.0 } else { 1.0 }; set(3 + k, Transform::from_translation(D_EAR * Vec3::new(s, 1.0, 1.0)).with_rotation(Quat::from_rotation_z(-0.95 * s) * pose.ears[k])); }
            set(5, Transform::from_translation(D_TAIL).with_rotation(pose.tail));
            for k in 0..4 {
                set(6 + k, Transform::from_translation(D_LEGS[k]).with_rotation(ax(pose.legs[k].0)));
                set(10 + k, Transform::from_xyz(0.0, -if k < 2 { D_FRONT.0 } else { D_HIND.0 }, 0.0).with_rotation(ax(pose.legs[k].1)));
            }
        }
        Kind::Rabbit => {
            ease_toward(&mut b.moving, local > 0.3, 8.0, dt);
            b.phase = (b.phase + dt * (2.6 + 0.12 * local.min(30.0)) * b.moving).rem_euclid(1.0);
            let mut pose = rabbit_hop(b.phase, b.moving, smooth01(8.0, 20.0, local), tt);
            if b.feed > 0.001 { pose = pose.lerp(rabbit_feed(tt), b.feed); }
            if b.rest > 0.001 { pose = pose.lerp(rabbit_rest(tt), b.rest); }
            if b.down > 0.001 { pose = pose.lerp(rabbit_down(tt, dead), b.down); }
            set(0, pose.body);
            set(1, Transform::from_translation(R_HEAD).with_rotation(pose.head));
            for k in 0..2 {
                let s = if k == 0 { -1.0 } else { 1.0 };
                set(2 + k, Transform::from_translation(R_EAR * Vec3::new(s, 1.0, 1.0)).with_rotation(Quat::from_rotation_z(-0.12 * s) * pose.ears[k]));
                set(4 + k, Transform::from_translation(R_HIND[k]).with_rotation(ax(pose.hind[k])));
                set(6 + k, Transform::from_translation(R_FRONT[k]).with_rotation(ax(pose.front[k])));
            }
        }
        Kind::Bird => {
            let flying = matches!(doing, Doing::Flee | Doing::Leave);
            let target = alt.unwrap_or(if flying { FLY_HEIGHT } else { 0.0 });
            b.alt = if alt.is_some() { target } else { b.alt + (target - b.alt) * (1.0 - (-dt * if target > b.alt { 2.2 } else { 1.3 }).exp()) };
            let air = smooth01(0.02, 0.25, b.alt);
            let climbing = target > b.alt + 0.2 || (alt.is_some() && flying);
            b.flap = (b.flap + dt * if climbing { 7.5 } else { 4.0 } * air).rem_euclid(1.0);
            ease_toward(&mut b.moving, local > 0.3 && air < 0.5, 8.0, dt);
            b.phase = (b.phase + dt * 3.2 * b.moving).rem_euclid(1.0);
            let mut pose = bird_ground(b.phase, b.moving, tt);
            if b.feed > 0.001 { pose = pose.lerp(bird_peck(tt), b.feed * (1.0 - air)); }
            if b.rest > 0.001 { pose = pose.lerp(bird_perch(tt), b.rest * (1.0 - air)); }
            if air > 0.001 { pose = pose.lerp(bird_fly(b.flap, !climbing && target < b.alt, tt), air); }
            if b.down > 0.001 { pose = pose.lerp(bird_down(), b.down); }
            pose.body.translation.y += b.alt / b.size;
            set(0, pose.body);
            set(1, Transform::from_translation(B_HEAD).with_rotation(pose.head));
            set(2, Transform::from_translation(B_TAIL).with_rotation(pose.tail));
            for k in 0..2 {
                set(3 + k, Transform::from_translation(B_WINGS[k]).with_rotation(pose.wings[k]));
                set(5 + k, Transform::from_translation(B_LEGS[k]).with_rotation(ax(pose.legs[k])));
            }
        }
    }
}

fn on_side(centre: Vec3, low: f32) -> Transform {
    let r = Quat::from_rotation_z(1.45);
    Transform::from_translation(Vec3::new(0.0, low, 0.0) - r * centre).with_rotation(r)
}

fn pitched_about(pivot: Vec3, pitch: f32, lift: f32) -> Transform {
    let r = ax(pitch);
    Transform::from_translation(pivot - r * pivot + Vec3::Y * lift).with_rotation(r)
}

fn ear_flick(tt: f32, k: usize) -> Quat {
    let f = smooth01(0.85, 0.95, ((tt * 0.7 + k as f32 * 2.1).sin() * 0.5 + 0.5).powf(3.0));
    Quat::from_rotation_y(f * 0.9 * if k == 0 { -1.0 } else { 1.0 })
}

#[derive(Clone, Copy)]
struct DeerPose { body: Transform, neck: Quat, head: Quat, tail: Quat, ears: [Quat; 2], legs: [(f32, f32); 4] }

impl DeerPose {
    fn lerp(self, o: DeerPose, k: f32) -> DeerPose {
        DeerPose {
            body: lerp_t(self.body, o.body, k), neck: self.neck.slerp(o.neck, k), head: self.head.slerp(o.head, k), tail: self.tail.slerp(o.tail, k),
            ears: [self.ears[0].slerp(o.ears[0], k), self.ears[1].slerp(o.ears[1], k)],
            legs: std::array::from_fn(|i| (self.legs[i].0 + (o.legs[i].0 - self.legs[i].0) * k, self.legs[i].1 + (o.legs[i].1 - self.legs[i].1) * k)),
        }
    }
}

const DEER_GAITS: GaitTable = [([0.25, 0.75, 0.0, 0.5], 0.64, 1.05, 0.1), ([0.0, 0.5, 0.5, 0.0], 0.45, 1.8, 0.16), ([0.08, 0.14, 0.58, 0.64], 0.26, 1.75, 0.34)];

fn deer_legs(body: Transform, feet: [Vec2; 4]) -> [(f32, f32); 4] { quadruped_legs(body, D_LEGS, D_FRONT, D_HIND, feet) }

fn deer_gait(b: &Beast, speed: f32, tt: f32) -> DeerPose {
    let (w, m, c, idle) = (b.gait, b.moving, b.phase * TAU, 1.0 - b.moving);
    let feet = quadruped_feet(&DEER_GAITS, w, m, speed, 1.0, b.phase, D_LEGS.map(|p| Vec2::new(0.0, p.z)), (0.0, 0.0), 1.0);
    let bob = (-0.015 * w.x - 0.03 * w.y) * (2.0 * c).cos() + 0.22 * w.z * (c - 2.2).sin().max(0.0) - 0.04 * w.z;
    let pitch = 0.14 * w.z * (c - 1.0).sin() + 0.015 * w.x * (2.0 * c).sin();
    let body = Transform::from_xyz(0.0, bob * m + idle * 0.004 * (tt * 2.2).sin(), 0.0).with_rotation(ax(pitch * m));
    let look = (tt * 0.31).sin() * 0.5 + (tt * 0.77).sin() * 0.2;
    let neck = Quat::from_rotation_y(idle * look * 0.6) * ax(idle * -0.05 + m * (0.12 * w.x + 0.22 * w.y + 0.3 * w.z + 0.05 * w.x * (2.0 * c).sin() + 0.08 * w.z * (c - 1.0).sin()));
    let head = ax(idle * 0.15 + m * (0.12 - 0.15 * w.z));
    let flag = smooth01(3.0, 5.5, speed) * m;
    let tail = ax(-2.3 * flag) * Quat::from_rotation_z((tt * 3.0).sin() * 0.25 * (1.0 - flag) * smooth01(0.6, 0.9, (tt * 0.4).sin()));
    let ears = [0, 1].map(|k| ax(-0.5 * flag) * ear_flick(tt, k) * ax(-0.15 * idle));
    DeerPose { body, neck, head, tail, ears, legs: deer_legs(body, feet) }
}

fn deer_graze(tt: f32) -> DeerPose {
    let up = smooth01(0.78, 0.9, (tt * 0.45).sin() * 0.5 + 0.5);
    let body = Transform::from_xyz(0.0, -0.06 * (1.0 - up), 0.0).with_rotation(ax(0.15 * (1.0 - up)));
    let mut feet = D_LEGS.map(|p| Vec2::new(0.0, p.z));
    feet[0].y += 0.1;
    feet[1].y += 0.04;
    DeerPose {
        body,
        neck: Quat::from_rotation_y(up * (tt * 0.5).sin() * 0.5) * ax(1.85 * (1.0 - up) - 0.05 * up),
        head: ax(-0.55 * (1.0 - up) + (tt * 5.0).sin() * 0.06 * (1.0 - up) + 0.12 * up) * Quat::from_rotation_z((tt * 4.0).sin() * 0.04 * up),
        tail: Quat::from_rotation_z((tt * 2.5).sin() * 0.2),
        ears: [ear_flick(tt, 0), ear_flick(tt, 1)],
        legs: deer_legs(body, feet),
    }
}

fn deer_lie(tt: f32) -> DeerPose {
    DeerPose {
        body: Transform::from_xyz(0.0, -0.64, 0.0).with_rotation(Quat::from_rotation_z(0.08)),
        neck: Quat::from_rotation_y((tt * 0.2).sin() * 0.4) * ax(-0.1),
        head: ax(0.2) * Quat::from_rotation_z((tt * 3.5).sin() * 0.04),
        tail: Quat::IDENTITY,
        ears: [ear_flick(tt, 0), ear_flick(tt, 1)],
        legs: [(-1.25, 2.75), (-1.1, 2.6), (-1.35, 2.75), (-1.2, 2.6)],
    }
}

fn deer_down(tt: f32, dead: bool) -> DeerPose {
    let kick = if dead { 0.0 } else { 1.0 };
    let legs = std::array::from_fn(|k| {
        let w = (tt * 9.0 + k as f32 * 1.7).sin() * 0.35 * kick;
        (0.3 + w + if k < 2 { -0.4 } else { 0.2 }, -0.3 - w * 0.5)
    });
    DeerPose { body: on_side(D_CENTRE, 0.17), neck: ax(-0.5 + 0.15 * kick * (tt * 6.0).sin()), head: ax(0.3), tail: Quat::IDENTITY, ears: [ax(-0.6); 2], legs }
}

#[derive(Clone, Copy)]
struct RabbitPose { body: Transform, head: Quat, ears: [Quat; 2], hind: [f32; 2], front: [f32; 2] }

impl RabbitPose {
    fn lerp(self, o: RabbitPose, k: f32) -> RabbitPose {
        let l = |a: f32, b: f32| a + (b - a) * k;
        RabbitPose {
            body: lerp_t(self.body, o.body, k), head: self.head.slerp(o.head, k),
            ears: [self.ears[0].slerp(o.ears[0], k), self.ears[1].slerp(o.ears[1], k)],
            hind: [l(self.hind[0], o.hind[0]), l(self.hind[1], o.hind[1])],
            front: [l(self.front[0], o.front[0]), l(self.front[1], o.front[1])],
        }
    }
}

fn rabbit_hop(p: f32, m: f32, bolt: f32, tt: f32) -> RabbitPose {
    let air_end = 0.55;
    let air = if p < air_end { (PI * p / air_end).sin() } else { 0.0 };
    let pitch = if p < air_end { -0.3 * (PI * p / air_end).cos() } else { 0.3 * (PI * (p - air_end) / (1.0 - air_end)).cos() };
    let hind = if p < 0.6 { 0.8 - 1.3 * smooth01(0.0, 0.6, p) } else { -0.5 + 1.3 * smooth01(0.6, 1.0, p) };
    let front = if p < 0.45 { 0.6 - 1.3 * smooth01(0.0, 0.45, p) } else { -0.7 + 1.3 * smooth01(0.45, 1.0, p) };
    let idle = 1.0 - m;
    RabbitPose {
        body: pitched_about(v(0.0, 0.2, -0.25), pitch * m * (1.0 - 0.3 * bolt), (0.16 + 0.12 * bolt) * air * m),
        head: ax(-pitch * 0.5 * m + idle * (0.05 + 0.03 * (tt * 17.0).sin().max(0.0))) * Quat::from_rotation_y(idle * (tt * 0.6).sin() * 0.4),
        ears: [0, 1].map(|k| ax(-0.25 * m - 0.9 * bolt) * Quat::from_rotation_y(idle * ear_flick(tt, k).to_axis_angle().1 * 0.5 * if k == 0 { -1.0 } else { 1.0 })),
        hind: [hind * m; 2],
        front: [front * m; 2],
    }
}

fn rabbit_feed(tt: f32) -> RabbitPose {
    RabbitPose {
        body: pitched_about(v(0.0, 0.2, -0.25), 0.18, -0.02),
        head: ax(0.55 + 0.04 * (tt * 14.0).sin()) * Quat::from_rotation_z((tt * 7.0).sin() * 0.03),
        ears: [0, 1].map(|k| ax(-0.3) * ear_flick(tt, k)),
        hind: [0.1; 2],
        front: [-0.2, 0.05 * (tt * 2.0).sin() - 0.2],
    }
}

fn rabbit_rest(tt: f32) -> RabbitPose {
    let loaf = RabbitPose { body: Transform::from_xyz(0.0, -0.07, 0.0), head: ax(0.1), ears: [ax(-1.1) * Quat::from_rotation_z(0.15), ax(-1.1) * Quat::from_rotation_z(-0.15)], hind: [-0.1; 2], front: [-1.2; 2] };
    let sit = RabbitPose { body: pitched_about(v(0.0, 0.15, -0.32), -1.05, 0.0), head: ax(0.9) * Quat::from_rotation_y((tt * 0.9).sin() * 0.6), ears: [ax(0.15); 2], hind: [-0.9; 2], front: [0.6; 2] };
    loaf.lerp(sit, smooth01(0.55, 0.75, (tt * 0.27).sin() * 0.5 + 0.5))
}

fn rabbit_down(tt: f32, dead: bool) -> RabbitPose {
    let kick = if dead { 0.0 } else { (tt * 11.0).sin() * 0.5 };
    RabbitPose { body: on_side(R_CENTRE, 0.2), head: ax(-0.3), ears: [ax(-1.3), ax(-1.2)], hind: [0.6 + kick, 0.4 - kick], front: [-0.4 - kick * 0.5, -0.2] }
}

#[derive(Clone, Copy)]
struct BirdPose { body: Transform, head: Quat, tail: Quat, wings: [Quat; 2], legs: [f32; 2] }

impl BirdPose {
    fn lerp(self, o: BirdPose, k: f32) -> BirdPose {
        BirdPose {
            body: lerp_t(self.body, o.body, k), head: self.head.slerp(o.head, k), tail: self.tail.slerp(o.tail, k),
            wings: [self.wings[0].slerp(o.wings[0], k), self.wings[1].slerp(o.wings[1], k)],
            legs: [self.legs[0] + (o.legs[0] - self.legs[0]) * k, self.legs[1] + (o.legs[1] - self.legs[1]) * k],
        }
    }
}

fn folded() -> [Quat; 2] { [-1.0f32, 1.0].map(|s| Quat::from_rotation_y(s * 1.45) * ax(-0.08) * Quat::from_rotation_z(s * 0.1)) }

fn glance(tt: f32, rate: f32, reach: f32) -> Quat {
    let (n, f) = ((tt * rate).floor(), smooth01(0.0, 0.12, (tt * rate).fract()));
    let (a, b) = ((hash(n as i32 as u32) - 0.5) * 2.0 * reach, (hash((n - 1.0) as i32 as u32) - 0.5) * 2.0 * reach);
    Quat::from_rotation_y(b + (a - b) * f)
}

fn bird_ground(p: f32, m: f32, tt: f32) -> BirdPose {
    let hop = (PI * p).sin() * 0.12 * m;
    BirdPose { body: Transform::from_xyz(0.0, hop, 0.0).with_rotation(ax(0.05 * m * (TAU * p).sin())), head: glance(tt, 1.3, 0.8), tail: ax(-0.15 * m * (PI * p).sin()), wings: folded(), legs: [-0.4 * hop / 0.12; 2] }
}

fn bird_peck(tt: f32) -> BirdPose {
    let c = (tt * 1.4).fract();
    let stab = smooth01(0.0, 0.08, c) * (1.0 - smooth01(0.16, 0.3, c)) + smooth01(0.35, 0.42, c) * (1.0 - smooth01(0.48, 0.6, c));
    BirdPose { body: pitched_about(v(0.0, 0.3, 0.0), 0.55 * stab + 0.1, 0.0), head: ax(0.7 * stab), tail: ax(-0.4 * stab), wings: folded(), legs: [-0.55 * stab - 0.1; 2] }
}

fn bird_perch(tt: f32) -> BirdPose {
    let preen = smooth01(0.7, 0.8, (tt * 0.35).sin() * 0.5 + 0.5);
    BirdPose {
        body: Transform::from_xyz(0.0, -0.08, 0.0).with_scale(Vec3::splat(1.0 + 0.08 * (1.0 - preen))),
        head: Quat::from_rotation_y(preen * 2.2) * ax(preen * (0.6 + 0.15 * (tt * 9.0).sin())) * glance(tt, 0.7, 0.6 * (1.0 - preen)),
        tail: Quat::IDENTITY, wings: folded(), legs: [0.15; 2],
    }
}

fn bird_fly(flap: f32, glide: bool, tt: f32) -> BirdPose {
    let a = if glide { 0.12 + 0.06 * (tt * 3.0).sin() } else { 0.85 * (TAU * flap).sin() + 0.1 };
    let wings = [-1.0f32, 1.0].map(|s| Quat::from_rotation_z(s * a) * ax(0.1 * (TAU * flap).cos()));
    BirdPose { body: Transform::from_xyz(0.0, if glide { 0.0 } else { -0.06 * (TAU * flap).sin() }, 0.0).with_rotation(ax(-0.05)), head: ax(-0.1), tail: ax(0.1), wings, legs: [1.3; 2] }
}

fn bird_down() -> BirdPose {
    let r = Quat::from_rotation_z(1.4);
    BirdPose { body: Transform::from_translation(v(0.0, 0.2, 0.0) - r * v(0.0, 0.45, 0.0)).with_rotation(r), head: ax(0.4), tail: Quat::IDENTITY, wings: [Quat::from_rotation_z(-0.3), Quat::from_rotation_y(1.2)], legs: [0.5, 0.3] }
}
