//! Wild animals (see `sim::wildlife`): deer that walk, trot and bound on jointed legs, graze
//! with an eye up now and then and lie down to chew; rabbits that hop, bolt, nibble, loaf and
//! sit up to look round; birds that hop, peck, preen and fly off when startled. Each is
//! replayed smoothly from the sim's steps like the wolf, and one a wolf kills lies where it
//! fell a while. `BC_GALLERY_WHAT=wild` lines up every pose (see `gallery.rs`).
use super::meshgen::{hex, mix, Mb};
use super::scene::height_at;
use super::sim_view::cell_xz;
use super::wolf::{leg_ik, WolfTrack};
use super::Models;
use crate::state::Sim;
use bevy::prelude::*;
use sim::wildlife::{Doing, Food, Kind};
use std::f32::consts::{PI, TAU};

const DEER_SIZE: f32 = 1.0;
const RABBIT_SIZE: f32 = 0.32;
const BIRD_SIZE: f32 = 0.36;

// Deer, in its own units: the shoulders stand about a unit up.
const D_NECK: Vec3 = Vec3::new(0.0, 0.95, 0.5);
/// The head's pivot, from the neck's.
const D_HEAD: Vec3 = Vec3::new(0.0, 0.52, 0.28);
const D_EAR: Vec3 = Vec3::new(0.06, 0.1, 0.0);
const D_TAIL: Vec3 = Vec3::new(0.0, 0.98, -0.63);
const D_LEGS: [Vec3; 4] = [Vec3::new(-0.11, 0.86, 0.42), Vec3::new(0.11, 0.86, 0.42), Vec3::new(-0.11, 0.9, -0.48), Vec3::new(0.11, 0.9, -0.48)];
const D_FRONT: (f32, f32) = (0.38, 0.52);
const D_HIND: (f32, f32) = (0.42, 0.54);
const D_CENTRE: Vec3 = Vec3::new(0.0, 0.92, 0.0);

// Rabbit.
const R_HEAD: Vec3 = Vec3::new(0.0, 0.5, 0.3);
const R_EAR: Vec3 = Vec3::new(0.05, 0.15, 0.02);
const R_HIND: [Vec3; 2] = [Vec3::new(-0.15, 0.26, -0.22), Vec3::new(0.15, 0.26, -0.22)];
const R_FRONT: [Vec3; 2] = [Vec3::new(-0.08, 0.24, 0.22), Vec3::new(0.08, 0.24, 0.22)];
const R_CENTRE: Vec3 = Vec3::new(0.0, 0.33, 0.0);

// Bird.
const B_HEAD: Vec3 = Vec3::new(0.0, 0.62, 0.24);
const B_TAIL: Vec3 = Vec3::new(0.0, 0.48, -0.28);
const B_WINGS: [Vec3; 2] = [Vec3::new(-0.17, 0.56, 0.06), Vec3::new(0.17, 0.56, 0.06)];
const B_LEGS: [Vec3; 2] = [Vec3::new(-0.07, 0.3, 0.02), Vec3::new(0.07, 0.3, 0.02)];
/// How high a bird flies, in world units.
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
    /// A doe's head and a buck's.
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
    /// A jay and a robin.
    bird_body: [Handle<Mesh>; 2],
    bird_head: [Handle<Mesh>; 2],
    bird_tail: [Handle<Mesh>; 2],
    bird_wing: [[Handle<Mesh>; 2]; 2],
    bird_leg: Handle<Mesh>,
}

/// One drawn animal. Its parts, by kind: a deer's body, neck, head, two ears, tail, four
/// upper legs and four lower; a rabbit's body, head, two ears, two hind legs and two fore; a
/// bird's body, head, tail, two wings and two legs.
#[derive(Component)]
struct Beast {
    kind: Kind,
    size: f32,
    parts: [Entity; 14],
    /// Share of the stride or hop run, and how much it is moving (0 still, 1 on the move).
    phase: f32,
    moving: f32,
    /// Deer: walk / trot / bound weights, eased.
    gait: Vec3,
    /// Its speed over the ground, world units a second, eased.
    speed: f32,
    last: Option<Vec2>,
    /// Bird: height in the air, and wingbeat.
    alt: f32,
    flap: f32,
    /// Eased weights of feeding, resting and lying caught or dead.
    feed: f32,
    rest: f32,
    down: f32,
    seed: f32,
    shown: Option<Vec2>,
}

/// The sim animal a beast is.
#[derive(Component)]
struct Wild(u32);

/// A carcass: the sim's, by when it died and where.
#[derive(Component)]
struct Carcass(u64);

/// A gallery animal: its speed (world units a second), what it is doing, and how high it is.
#[derive(Component)]
pub(super) struct GalleryBeast {
    speed: f32,
    doing: Doing,
    alt: f32,
    dead: bool,
}

fn v(x: f32, y: f32, z: f32) -> Vec3 {
    Vec3::new(x, y, z)
}

fn smooth01(a: f32, b: f32, x: f32) -> f32 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn ax(a: f32) -> Quat {
    Quat::from_rotation_x(a)
}

fn hash(n: u32) -> f32 {
    let mut x = n.wrapping_mul(0x9E37_79B9) ^ 0x85EB_CA6B;
    x ^= x >> 15;
    x = x.wrapping_mul(0x2C1B_3C6D);
    x ^= x >> 12;
    (x & 0xFFFF) as f32 / 65535.0
}

/// A coat shaded by which way it faces: `top` along the back, `side` on the flanks, `under`
/// beneath, with a faint grain.
fn coat(p: Vec3, n: Vec3, top: [f32; 3], side: [f32; 3], under: [f32; 3]) -> [f32; 3] {
    let c = if n.y > 0.0 { mix(side, top, smooth01(0.3, 0.95, n.y)) } else { mix(side, under, smooth01(0.05, 0.5, -n.y)) };
    let g = 1.0 + (p.x * 53.0).sin() * (p.y * 47.0).sin() * (p.z * 43.0).sin() * 0.04;
    [c[0] * g, c[1] * g, c[2] * g]
}

// ---- Deer ----

fn deer_coat(p: Vec3, n: Vec3) -> [f32; 3] {
    coat(p, n, hex(0x7a5233), hex(0xa2724a), hex(0xe9e1d3))
}

fn deer_body(mb: &mut Mb) {
    let rings = [
        (v(0.0, 0.95, -0.67), 0.02, 0.02),
        (v(0.0, 0.95, -0.63), 0.1, 0.12),
        (v(0.0, 0.95, -0.53), 0.15, 0.19),
        (v(0.0, 0.93, -0.36), 0.165, 0.21),
        (v(0.0, 0.9, -0.15), 0.165, 0.215),
        (v(0.0, 0.9, 0.1), 0.175, 0.23),
        (v(0.0, 0.93, 0.32), 0.165, 0.225),
        (v(0.0, 0.97, 0.48), 0.135, 0.19),
        (v(0.0, 1.01, 0.58), 0.08, 0.11),
        (v(0.0, 1.03, 0.62), 0.02, 0.02),
    ];
    let start = mb.len();
    mb.loft(&rings, 16, [0.0; 3]);
    // A white rump patch under the tail.
    mb.paint(start, |p, n, _| if p.z < -0.56 && n.z < -0.3 && p.y < 1.0 { hex(0xe9e1d3) } else { deer_coat(p, n) });
}

fn deer_neck(mb: &mut Mb) {
    let up = D_HEAD;
    let start = mb.len();
    mb.loft(&[(v(0.0, -0.08, -0.1), 0.03, 0.03), (v(0.0, -0.04, -0.05), 0.11, 0.16), (up * 0.3, 0.09, 0.12), (up * 0.7, 0.075, 0.09), (up * 1.0, 0.07, 0.08), (up * 1.08, 0.03, 0.03)], 12, [0.0; 3]);
    // A pale throat patch.
    mb.paint(start, |p, n, _| if n.z > 0.5 && p.y > 0.3 { hex(0xe9e1d3) } else { deer_coat(p, n) });
}

fn deer_head(mb: &mut Mb, antlers: bool) {
    let (brown, dark, pale) = (hex(0x8a5d3a), hex(0x4a3324), hex(0xe9e1d3));
    let start = mb.len();
    mb.ellipsoid_g(v(0.0, 0.04, 0.04), v(0.085, 0.09, 0.11), brown, brown, 12, 8);
    mb.loft(&[(v(0.0, 0.05, 0.04), 0.03, 0.03), (v(0.0, 0.04, 0.09), 0.075, 0.075), (v(0.0, 0.0, 0.2), 0.055, 0.055), (v(0.0, -0.03, 0.28), 0.045, 0.042), (v(0.0, -0.035, 0.31), 0.02, 0.02)], 10, brown);
    mb.paint(start, |p, n, c| if n.y < -0.3 && p.z > 0.08 { pale } else { mix(c, deer_coat(p, n), 0.4) });
    // Black nose, a white band behind it, dark eyes.
    mb.ellipsoid(v(0.0, -0.02, 0.305), v(0.035, 0.028, 0.02), hex(0x161210));
    mb.ellipsoid_r(v(0.0, -0.005, 0.25), v(0.05, 0.04, 0.03), Quat::IDENTITY, pale);
    for s in [-1.0f32, 1.0] {
        mb.ellipsoid(v(0.07 * s, 0.06, 0.08), v(0.022, 0.022, 0.025), hex(0x120e0c));
        mb.ellipsoid(v(0.072 * s, 0.067, 0.09), v(0.006, 0.006, 0.006), hex(0xffffff));
    }
    if antlers {
        let bone = hex(0xcbb894);
        for s in [-1.0f32, 1.0] {
            // A main beam sweeping out, back and up, then forward, with tines off it.
            let p0 = v(0.045 * s, 0.11, 0.02);
            let p1 = v(0.13 * s, 0.22, -0.04);
            let p2 = v(0.17 * s, 0.32, 0.04);
            let p3 = v(0.12 * s, 0.38, 0.16);
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
    // A long, cupped ear pointing up from its pivot (splayed out by the rig).
    mb.ellipsoid_r(v(0.0, 0.09, 0.0), v(0.04, 0.1, 0.018), Quat::IDENTITY, hex(0x8a5d3a));
    mb.ellipsoid_r(v(0.0, 0.09, 0.012), v(0.026, 0.075, 0.008), Quat::IDENTITY, hex(0xd8c4b0));
}

fn deer_tail(mb: &mut Mb) {
    // Brown on top, white beneath: raised as a flag when it bolts.
    let start = mb.len();
    mb.loft(&[(v(0.0, 0.02, 0.0), 0.02, 0.02), (v(0.0, 0.0, -0.02), 0.055, 0.04), (v(0.0, -0.08, -0.05), 0.065, 0.045), (v(0.0, -0.16, -0.05), 0.04, 0.03), (v(0.0, -0.19, -0.045), 0.01, 0.01)], 10, [0.0; 3]);
    mb.paint(start, |_, n, _| if n.z > 0.1 { hex(0xf3eee6) } else { hex(0x7a5233) });
}

fn deer_upper(mb: &mut Mb, front: bool) {
    let len = if front { D_FRONT.0 } else { D_HIND.0 };
    let start = mb.len();
    if front {
        mb.loft(&[(v(0.0, 0.07, 0.02), 0.02, 0.02), (v(0.0, 0.03, 0.01), 0.07, 0.1), (v(0.0, -0.14, 0.0), 0.06, 0.08), (v(0.0, -len, 0.0), 0.035, 0.04), (v(0.0, -len - 0.04, 0.0), 0.015, 0.015)], 10, [0.0; 3]);
    } else {
        mb.loft(&[(v(0.0, 0.09, -0.02), 0.02, 0.02), (v(0.0, 0.04, -0.03), 0.085, 0.14), (v(0.0, -0.14, 0.0), 0.075, 0.11), (v(0.0, -len, 0.02), 0.04, 0.045), (v(0.0, -len - 0.04, 0.02), 0.015, 0.015)], 10, [0.0; 3]);
    }
    mb.paint(start, |p, n, _| coat(p, n, hex(0x7a5233), hex(0xa2724a), hex(0xa98058)));
}

fn deer_lower(mb: &mut Mb, front: bool) {
    let len = if front { D_FRONT.1 } else { D_HIND.1 };
    let leg = hex(0xb08a64);
    let start = mb.len();
    if front {
        mb.loft(&[(v(0.0, 0.04, 0.0), 0.015, 0.015), (v(0.0, 0.0, 0.0), 0.035, 0.04), (v(0.0, -0.2, 0.0), 0.026, 0.03), (v(0.0, -len + 0.08, 0.0), 0.022, 0.026), (v(0.0, -len + 0.04, 0.01), 0.028, 0.03), (v(0.0, -len + 0.02, 0.01), 0.01, 0.01)], 8, leg);
    } else {
        let hock = v(0.0, -0.22, -0.1);
        mb.loft(&[(v(0.0, 0.04, 0.01), 0.015, 0.015), (v(0.0, 0.0, 0.0), 0.04, 0.05), (hock, 0.026, 0.035), (v(0.0, -len + 0.08, -0.01), 0.022, 0.026), (v(0.0, -len + 0.04, 0.0), 0.028, 0.03), (v(0.0, -len + 0.02, 0.0), 0.01, 0.01)], 8, leg);
    }
    mb.paint(start, |p, n, c| mix(coat(p, n, hex(0x7a5233), hex(0xa2724a), hex(0xa98058)), c, smooth01(-0.02, -0.15, p.y)));
    // A small black hoof.
    mb.ellipsoid(v(0.0, -len + 0.022, 0.02), v(0.03, 0.024, 0.04), hex(0x1d1713));
}

// ---- Rabbit ----

fn rabbit_coat(p: Vec3, n: Vec3) -> [f32; 3] {
    coat(p, n, hex(0x6d5b48), hex(0x9a8670), hex(0xdcd3c4))
}

fn rabbit_body(mb: &mut Mb) {
    let rings = [
        (v(0.0, 0.3, -0.43), 0.02, 0.02),
        (v(0.0, 0.32, -0.39), 0.15, 0.17),
        (v(0.0, 0.34, -0.26), 0.23, 0.26),
        (v(0.0, 0.34, -0.06), 0.21, 0.23),
        (v(0.0, 0.36, 0.14), 0.16, 0.19),
        (v(0.0, 0.42, 0.27), 0.11, 0.14),
        (v(0.0, 0.46, 0.32), 0.03, 0.03),
    ];
    let start = mb.len();
    mb.loft(&rings, 14, [0.0; 3]);
    mb.paint(start, |p, n, _| rabbit_coat(p, n));
    // The white scut.
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
    // Dark tips.
    let start = mb.len();
    mb.ellipsoid_r(v(0.0, 0.32, 0.0), v(0.03, 0.035, 0.018), Quat::IDENTITY, hex(0x3b2f25));
    mb.paint(start, |_, _, c| c);
}

fn rabbit_hind(mb: &mut Mb) {
    let start = mb.len();
    mb.ellipsoid_g(v(0.0, -0.04, 0.0), v(0.075, 0.14, 0.14), [0.0; 3], [0.0; 3], 10, 7);
    mb.paint(start, |p, n, _| coat(p, n, hex(0x6d5b48), hex(0x8d7a64), hex(0x9a8670)));
    // The long hind foot, flat on the ground ahead of the hip.
    mb.ellipsoid_r(v(0.0, -0.235, 0.05), v(0.05, 0.03, 0.15), Quat::IDENTITY, hex(0xc8bba8));
}

fn rabbit_front(mb: &mut Mb) {
    mb.cyl(v(0.0, 0.02, 0.0), v(0.0, -0.21, 0.02), 0.038, 0.03, hex(0x9a8670), hex(0xc8bba8), 6);
    mb.ellipsoid(v(0.0, -0.225, 0.04), v(0.035, 0.02, 0.05), hex(0xc8bba8));
}

// ---- Birds ----

/// A jay (blue over white) and a robin (grey-brown over a rust breast).
fn plumage(look: usize) -> ([f32; 3], [f32; 3], [f32; 3], [f32; 3]) {
    match look {
        0 => (hex(0x3b6cb5), hex(0x6f96cc), hex(0xe7e9ea), hex(0x17191c)),
        _ => (hex(0x4f4842), hex(0x6d645b), hex(0xcc6a2d), hex(0x2b2725)),
    }
}

fn bird_body(mb: &mut Mb, look: usize) {
    let (top, side, belly, _) = plumage(look);
    let start = mb.len();
    mb.ellipsoid_m(Mat4::from_translation(v(0.0, 0.45, 0.0)) * Mat4::from_quat(ax(-0.25)) * Mat4::from_scale(v(0.2, 0.2, 0.34)), 14, 9, [0.0; 3], [0.0; 3]);
    mb.paint(start, |p, n, _| {
        let c = coat(p, n, top, side, belly);
        // A robin's rust breast; a jay's white throat.
        if look == 1 && n.z > 0.1 && n.y < 0.45 { mix(c, belly, 0.9) } else if look == 1 { mix(c, hex(0xd8d0c4), smooth01(-0.3, -0.8, n.y)) } else { c }
    });
}

fn bird_head(mb: &mut Mb, look: usize) {
    let (top, side, belly, dark) = plumage(look);
    let start = mb.len();
    mb.ellipsoid_g(v(0.0, 0.06, 0.06), v(0.14, 0.14, 0.15), [0.0; 3], [0.0; 3], 12, 8);
    mb.paint(start, |p, n, _| if look == 0 && n.y < -0.2 { belly } else if look == 1 { mix(dark, side, 0.25) } else { coat(p, n, top, side, side) });
    if look == 0 {
        // The jay's crest and black necklace.
        mb.cyl(v(0.0, 0.16, 0.02), v(0.0, 0.27, -0.1), 0.07, 0.01, top, top, 6);
        mb.ellipsoid_r(v(0.0, -0.04, 0.06), v(0.15, 0.03, 0.12), Quat::IDENTITY, dark);
    }
    let beak = if look == 0 { hex(0x1c1c1c) } else { hex(0xd9a520) };
    mb.cyl(v(0.0, 0.04, 0.17), v(0.0, 0.02, 0.33), 0.045, 0.004, beak, beak, 6);
    for s in [-1.0f32, 1.0] {
        mb.ellipsoid(v(0.095 * s, 0.09, 0.12), v(0.025, 0.025, 0.025), hex(0x0d0d0d));
        if look == 1 {
            // The robin's broken eye-ring.
            mb.ellipsoid(v(0.1 * s, 0.11, 0.12), v(0.02, 0.008, 0.02), hex(0xf0ece4));
        }
    }
}

fn bird_tail(mb: &mut Mb, look: usize) {
    let (top, side, _, dark) = plumage(look);
    let start = mb.len();
    mb.ellipsoid_r(v(0.0, -0.02, -0.2), v(0.12, 0.025, 0.22), ax(-0.1), top);
    mb.paint(start, |p, _, c| if look == 0 && p.z < -0.36 { hex(0xf2f2f2) } else if p.z < -0.3 { mix(c, dark, 0.4) } else { mix(c, side, 0.2) });
}

/// A wing spread out to the side `s` (-1 left, 1 right) from its pivot.
fn bird_wing(mb: &mut Mb, look: usize, s: f32) {
    let (top, side, _, dark) = plumage(look);
    let start = mb.len();
    mb.ellipsoid_r(v(0.22 * s, 0.0, -0.04), v(0.24, 0.025, 0.14), Quat::from_rotation_y(0.15 * s), top);
    // Long primaries at the tip.
    mb.ellipsoid_r(v(0.37 * s, 0.0, -0.1), v(0.13, 0.018, 0.08), Quat::from_rotation_y(0.5 * s), dark);
    mb.paint(start, |p, _, c| {
        let bars = look == 0 && ((p.x.abs() * 40.0).sin() > 0.75) && p.x.abs() > 0.12;
        if bars { dark } else if p.x.abs() < 0.12 { mix(c, side, 0.5) } else { c }
    });
}

fn bird_leg(mb: &mut Mb) {
    let leg = hex(0x5b4a3e);
    mb.cyl(v(0.0, 0.02, 0.0), v(0.0, -0.3, 0.02), 0.016, 0.012, leg, leg, 5);
    for a in [-0.5f32, 0.0, 0.5] {
        mb.cyl(v(0.0, -0.3, 0.02), v(0.08 * a.sin(), -0.3, 0.02 + 0.08 * a.cos()), 0.01, 0.006, leg, leg, 4);
    }
    mb.cyl(v(0.0, -0.3, 0.02), v(0.0, -0.3, -0.05), 0.01, 0.006, leg, leg, 4);
}

fn init_meshes(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>) {
    let mut build = |f: &dyn Fn(&mut Mb)| {
        let mut mb = Mb::new();
        f(&mut mb);
        meshes.add(mb.build())
    };
    commands.insert_resource(WildMeshes {
        deer_body: build(&deer_body),
        deer_neck: build(&deer_neck),
        deer_head: [build(&|m| deer_head(m, false)), build(&|m| deer_head(m, true))],
        deer_ear: build(&deer_ear),
        deer_tail: build(&deer_tail),
        deer_upper: [build(&|m| deer_upper(m, true)), build(&|m| deer_upper(m, false))],
        deer_lower: [build(&|m| deer_lower(m, true)), build(&|m| deer_lower(m, false))],
        rabbit_body: build(&rabbit_body),
        rabbit_head: build(&rabbit_head),
        rabbit_ear: build(&rabbit_ear),
        rabbit_hind: build(&rabbit_hind),
        rabbit_front: build(&rabbit_front),
        bird_body: [build(&|m| bird_body(m, 0)), build(&|m| bird_body(m, 1))],
        bird_head: [build(&|m| bird_head(m, 0)), build(&|m| bird_head(m, 1))],
        bird_tail: [build(&|m| bird_tail(m, 0)), build(&|m| bird_tail(m, 1))],
        bird_wing: [[build(&|m| bird_wing(m, 0, -1.0)), build(&|m| bird_wing(m, 0, 1.0))], [build(&|m| bird_wing(m, 1, -1.0)), build(&|m| bird_wing(m, 1, 1.0))]],
        bird_leg: build(&bird_leg),
    });
}

/// Spawns animal `id` of `kind` (its looks and size follow from the id) at `at`.
fn spawn(commands: &mut Commands, wm: &WildMeshes, m: &Models, kind: Kind, id: u32, at: Transform) -> Entity {
    let w = &m.white;
    let mesh = |h: &Handle<Mesh>| (Mesh3d(h.clone()), MeshMaterial3d(w.clone()));
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
            commands.spawn((mesh(&wm.deer_body), Transform::default(), ChildOf(body)));
            let neck = commands.spawn((mesh(&wm.deer_neck), Transform::from_translation(D_NECK), ChildOf(body))).id();
            let buck = id % 3 == 0;
            let head = commands.spawn((mesh(&wm.deer_head[buck as usize]), Transform::from_translation(D_HEAD), ChildOf(neck))).id();
            parts[1] = neck;
            parts[2] = head;
            for (k, s) in [-1.0f32, 1.0].into_iter().enumerate() {
                parts[3 + k] = commands.spawn((mesh(&wm.deer_ear), Transform::from_translation(D_EAR * Vec3::new(s, 1.0, 1.0)), ChildOf(head))).id();
            }
            parts[5] = commands.spawn((mesh(&wm.deer_tail), Transform::from_translation(D_TAIL), ChildOf(body))).id();
            for (k, pivot) in D_LEGS.iter().enumerate() {
                let (f, len) = if k < 2 { (0, D_FRONT.0) } else { (1, D_HIND.0) };
                parts[6 + k] = commands.spawn((mesh(&wm.deer_upper[f]), Transform::from_translation(*pivot), ChildOf(body))).id();
                parts[10 + k] = commands.spawn((mesh(&wm.deer_lower[f]), Transform::from_xyz(0.0, -len, 0.0), ChildOf(parts[6 + k]))).id();
            }
        }
        Kind::Rabbit => {
            commands.spawn((mesh(&wm.rabbit_body), Transform::default(), ChildOf(body)));
            let head = commands.spawn((mesh(&wm.rabbit_head), Transform::from_translation(R_HEAD), ChildOf(body))).id();
            parts[1] = head;
            for (k, s) in [-1.0f32, 1.0].into_iter().enumerate() {
                parts[2 + k] = commands.spawn((mesh(&wm.rabbit_ear), Transform::from_translation(R_EAR * Vec3::new(s, 1.0, 1.0)), ChildOf(head))).id();
                parts[4 + k] = commands.spawn((mesh(&wm.rabbit_hind), Transform::from_translation(R_HIND[k]), ChildOf(body))).id();
                parts[6 + k] = commands.spawn((mesh(&wm.rabbit_front), Transform::from_translation(R_FRONT[k]), ChildOf(body))).id();
            }
        }
        Kind::Bird => {
            let look = (id % 2) as usize;
            commands.spawn((mesh(&wm.bird_body[look]), Transform::default(), ChildOf(body)));
            parts[1] = commands.spawn((mesh(&wm.bird_head[look]), Transform::from_translation(B_HEAD), ChildOf(body))).id();
            parts[2] = commands.spawn((mesh(&wm.bird_tail[look]), Transform::from_translation(B_TAIL), ChildOf(body))).id();
            for k in 0..2 {
                parts[3 + k] = commands.spawn((mesh(&wm.bird_wing[look][k]), Transform::from_translation(B_WINGS[k]), ChildOf(body))).id();
                parts[5 + k] = commands.spawn((mesh(&wm.bird_leg), Transform::from_translation(B_LEGS[k]), ChildOf(body))).id();
            }
        }
    }
    commands.entity(root).insert(Beast {
        kind,
        size,
        parts,
        phase: hash(id ^ 77),
        moving: 0.0,
        gait: Vec3::X,
        speed: 0.0,
        last: None,
        alt: 0.0,
        flap: 0.0,
        feed: 0.0,
        rest: 0.0,
        down: 0.0,
        seed: hash(id ^ 0xbeef) * 100.0,
        shown: None,
    });
    root
}

/// `BC_GALLERY_WHAT=wild`: a row of deer (standing, walking, trotting, bounding, grazing,
/// lying, caught, dead, and a buck), one of rabbits and one of birds, side on to a camera at
/// yaw 0. `BC_GALLERY_POSE=n` shows entry n alone.
pub(super) fn spawn_gallery(commands: &mut Commands, wm: &WildMeshes, m: &Models, centre: Vec3, ground: impl Fn(f32, f32) -> f32) {
    let yaw: f32 = std::env::var("BC_GALLERY_YAW").ok().and_then(|v| v.parse().ok()).unwrap_or(std::f32::consts::FRAC_PI_2);
    let only: Option<usize> = std::env::var("BC_GALLERY_POSE").ok().and_then(|v| v.parse().ok());
    let feed = Doing::Feed { food: Food::Pile(0), until: 0.0 };
    let rest = Doing::Rest { until: 0.0 };
    let rows: [(Kind, f32, &[(f32, Doing, f32, bool, u32)]); 3] = [
        (Kind::Deer, -3.0, &[(0.0, Doing::Wander, 0.0, false, 1), (1.5, Doing::Wander, 0.0, false, 1), (3.4, Doing::Wander, 0.0, false, 1), (10.0, Doing::Flee, 0.0, false, 1), (0.0, feed, 0.0, false, 1), (0.0, rest, 0.0, false, 1), (0.0, Doing::Caught, 0.0, false, 1), (0.0, Doing::Caught, 0.0, true, 1), (1.5, Doing::Wander, 0.0, false, 3)]),
        (Kind::Rabbit, 0.0, &[(0.0, Doing::Wander, 0.0, false, 1), (2.0, Doing::Wander, 0.0, false, 1), (9.0, Doing::Flee, 0.0, false, 1), (0.0, feed, 0.0, false, 1), (0.0, rest, 0.0, false, 1), (0.0, rest, 0.0, false, 2), (0.0, Doing::Caught, 0.0, false, 1)]),
        (Kind::Bird, 1.8, &[(0.0, Doing::Wander, 0.0, false, 1), (1.0, Doing::Wander, 0.0, false, 2), (0.0, feed, 0.0, false, 1), (0.0, rest, 0.0, false, 2), (8.0, Doing::Flee, 1.2, false, 1), (8.0, Doing::Flee, 1.2, false, 2), (0.0, Doing::Wander, 0.6, false, 1)]),
    ];
    let mut n = 0;
    for (kind, dz, entries) in rows {
        let spacing = match kind {
            Kind::Deer => 2.0,
            _ => 0.8,
        };
        for (i, &(speed, doing, alt, dead, id)) in entries.iter().enumerate() {
            n += 1;
            if only.is_some_and(|o| o != n - 1) {
                continue;
            }
            let (x, z) = if only.is_some() { (centre.x, centre.z) } else { (centre.x + (i as f32 - entries.len() as f32 / 2.0) * spacing, centre.z + dz) };
            let at = Vec3::new(x, ground(x, z), z);
            let e = spawn(commands, wm, m, kind, id, Transform::from_translation(at).with_rotation(Quat::from_rotation_y(yaw)));
            commands.entity(e).insert(GalleryBeast { speed, doing, alt, dead });
        }
    }
}

fn carcass_key(c: &sim::wildlife::Carcass) -> u64 {
    c.died.to_bits() ^ ((c.pos.0.to_bits() as u64) << 32) ^ c.pos.1.to_bits() as u64
}

fn ground(s: &sim::SimState, xz: Vec2) -> Vec3 {
    Vec3::new(xz.x, height_at(&s.height, xz.x, xz.y), xz.y)
}

#[allow(clippy::type_complexity)]
fn drive(
    mut commands: Commands,
    sim: Res<Sim>,
    time: Res<Time>,
    clock: Option<Res<crate::sim_driver::SimClock>>,
    meshes: Option<Res<WildMeshes>>,
    models: Option<Res<Models>>,
    mut beasts: Query<(Entity, &mut Beast, &mut Transform, &mut Visibility, Option<&mut WolfTrack>, Option<&Wild>, Option<&Carcass>, Option<&GalleryBeast>)>,
    mut parts: Query<&mut Transform, Without<Beast>>,
) {
    let _p = ::sim::prof::scope("game/wildlife");
    let (Some(wm), Some(m)) = (meshes, models) else { return };
    let s = &sim.0;
    let wl = &s.wildlife;
    let dt = time.delta_secs().clamp(1e-4, 0.1);
    let t = time.elapsed_secs();
    let now = clock.as_ref().map_or(s.time_days, |c| c.render_days(s.time_days));
    let mut seen: Vec<u32> = Vec::with_capacity(wl.animals.len());
    let mut seen_dead: Vec<u64> = Vec::with_capacity(wl.carcasses.len());
    for (e, mut b, mut tr, mut vis, track, wild, carcass, gallery) in &mut beasts {
        let (speed, doing, dead, alt) = if let Some(g) = gallery {
            (g.speed, g.doing, g.dead, Some(g.alt))
        } else if let Some(&Carcass(key)) = carcass {
            let Some(c) = wl.carcasses.iter().find(|c| carcass_key(c) == key) else {
                commands.entity(e).despawn();
                continue;
            };
            seen_dead.push(key);
            // It sinks into the grass over its last few hours.
            let age = ((s.time_days - c.died) / sim::wildlife::CARCASS_DAYS) as f32;
            let g = ground(s, cell_xz(s, c.pos));
            tr.translation = g - Vec3::Y * 0.3 * b.size * smooth01(0.8, 1.0, age);
            (0.0, Doing::Caught, true, None)
        } else if let (Some(&Wild(id)), Some(mut track)) = (wild, track) {
            let Some(a) = wl.get(id) else {
                commands.entity(e).despawn();
                continue;
            };
            seen.push(id);
            let here = cell_xz(s, a.pos);
            track.follow(s, &a.track, 60.0);
            track.record_with(s.instance, s.time_days, here, 60.0);
            let (aim, going) = track.at(now).unwrap_or((here, None));
            let k = 1.0 - (-dt * 10.0).exp();
            let xz = match b.shown {
                Some(p) if p.distance(aim) < 6.0 => p.lerp(aim, k),
                _ => aim,
            };
            b.shown = Some(xz);
            tr.translation = ground(s, xz);
            let raw = b.last.map_or(0.0, |l| l.distance(xz) / dt);
            b.last = Some(xz);
            b.speed += (raw - b.speed) * (1.0 - (-dt * 6.0).exp());
            let want = Quat::from_rotation_y(going.map_or(a.heading, |d| d.x.atan2(d.y)));
            tr.rotation = tr.rotation.slerp(want, 1.0 - (-dt * 8.0).exp());
            (b.speed, a.doing, false, None)
        } else {
            continue;
        };
        // Nothing in the fog of war is drawn.
        if gallery.is_none() {
            let cell = crate::flow::world_cell(&s.height, tr.translation);
            let want = if crate::fog_render::unexplored(s, (cell.0.max(0.0) as usize, cell.1.max(0.0) as usize)) { Visibility::Hidden } else { Visibility::Inherited };
            if *vis != want {
                *vis = want;
            }
            if want == Visibility::Hidden {
                continue;
            }
        }
        animate(&mut b, speed, doing, dead, alt, t, dt, &mut parts);
    }
    for a in &wl.animals {
        if seen.contains(&a.id) {
            continue;
        }
        let at = ground(s, cell_xz(s, a.pos));
        let root = spawn(&mut commands, &wm, &m, a.kind, a.id, Transform::from_translation(at).with_rotation(Quat::from_rotation_y(a.heading)));
        let mut track = WolfTrack::default();
        track.record_with(s.instance, s.time_days, at.xz(), 60.0);
        commands.entity(root).insert((Wild(a.id), track));
    }
    for c in &wl.carcasses {
        let key = carcass_key(c);
        if seen_dead.contains(&key) {
            continue;
        }
        let at = ground(s, cell_xz(s, c.pos));
        let root = spawn(&mut commands, &wm, &m, c.kind, (key as u32) ^ (key >> 32) as u32, Transform::from_translation(at).with_rotation(Quat::from_rotation_y(c.heading)));
        commands.entity(root).insert(Carcass(key));
    }
}

/// `BC_WILD_CAM=KIND,DIST[,YAW[,PITCH]]` (KIND 0 rabbit, 1 deer, 2 bird): keep the camera on
/// the first such animal in the sim (for screenshots).
fn follow(mut orbit: ResMut<crate::camera::OrbitCamera>, beasts: Query<(&Transform, &Beast, &Wild)>, spec: Local<Option<Vec<f32>>>) {
    let mut spec = spec;
    let p = spec.get_or_insert_with(|| std::env::var("BC_WILD_CAM").map(|v| v.split(',').filter_map(|t| t.trim().parse().ok()).collect()).unwrap_or_default());
    if p.len() < 2 {
        return;
    }
    let kind = Kind::ALL[(p[0] as usize).min(2)];
    let Some((tr, b, _)) = beasts.iter().filter(|(_, b, _)| b.kind == kind).min_by_key(|(_, _, w)| w.0) else { return };
    orbit.target = tr.translation + Vec3::Y * b.alt;
    orbit.distance = p[1];
    orbit.yaw = p.get(2).copied().unwrap_or(orbit.yaw);
    orbit.pitch = p.get(3).copied().unwrap_or(orbit.pitch);
}

/// Eases `w` toward 1 when `on`, else toward 0, at `rate` a second.
fn ease(w: &mut f32, on: bool, rate: f32, dt: f32) {
    *w += (if on { 1.0 } else { 0.0 } - *w) * (1.0 - (-dt * rate).exp());
}

#[allow(clippy::too_many_arguments)]
fn animate(b: &mut Beast, speed: f32, doing: Doing, dead: bool, alt: Option<f32>, t: f32, dt: f32, parts: &mut Query<&mut Transform, Without<Beast>>) {
    let first = b.down == 0.0 && dead;
    ease(&mut b.feed, matches!(doing, Doing::Feed { .. }) && speed < 0.3, 3.0, dt);
    ease(&mut b.rest, matches!(doing, Doing::Rest { .. } | Doing::Digest { .. }) && speed < 0.3, 1.6, dt);
    ease(&mut b.down, doing == Doing::Caught, 5.0, dt);
    if first {
        b.down = 1.0;
    }
    let local = speed / b.size;
    let tt = t + b.seed;
    let mut set = |i: usize, tr: Transform| {
        if let Ok(mut p) = parts.get_mut(b.parts[i]) {
            *p = tr;
        }
    };
    match b.kind {
        Kind::Deer => {
            let going = local > 0.15;
            ease(&mut b.moving, going, 5.0, dt);
            let (walk, bound) = (1.0 - smooth01(2.0, 3.0, local), smooth01(4.5, 6.5, local));
            b.gait = b.gait.lerp(Vec3::new(walk, 1.0 - walk - bound, bound), 1.0 - (-dt * 5.0).exp());
            let freq = deer_mix(b.gait, |g| g.2);
            b.phase = (b.phase + dt * freq * b.moving).rem_euclid(1.0);
            let mut pose = deer_gait(b, local, tt);
            if b.feed > 0.001 {
                pose = pose.lerp(deer_graze(tt), b.feed);
            }
            if b.rest > 0.001 {
                pose = pose.lerp(deer_lie(tt), b.rest);
            }
            if b.down > 0.001 {
                pose = pose.lerp(deer_down(tt, dead), b.down);
            }
            set(0, pose.body);
            set(1, Transform::from_translation(D_NECK).with_rotation(pose.neck));
            set(2, Transform::from_translation(D_HEAD).with_rotation(pose.head));
            for k in 0..2 {
                let s = if k == 0 { -1.0 } else { 1.0 };
                set(3 + k, Transform::from_translation(D_EAR * Vec3::new(s, 1.0, 1.0)).with_rotation(Quat::from_rotation_z(-0.95 * s) * pose.ears[k]));
            }
            set(5, Transform::from_translation(D_TAIL).with_rotation(pose.tail));
            for k in 0..4 {
                let len = if k < 2 { D_FRONT.0 } else { D_HIND.0 };
                set(6 + k, Transform::from_translation(D_LEGS[k]).with_rotation(ax(pose.legs[k].0)));
                set(10 + k, Transform::from_xyz(0.0, -len, 0.0).with_rotation(ax(pose.legs[k].1)));
            }
        }
        Kind::Rabbit => {
            ease(&mut b.moving, local > 0.3, 8.0, dt);
            let freq = 2.6 + 0.12 * local.min(30.0);
            b.phase = (b.phase + dt * freq * b.moving).rem_euclid(1.0);
            let mut pose = rabbit_hop(b.phase, b.moving, smooth01(8.0, 20.0, local), tt);
            if b.feed > 0.001 {
                pose = pose.lerp(rabbit_feed(tt), b.feed);
            }
            if b.rest > 0.001 {
                pose = pose.lerp(rabbit_rest(tt), b.rest);
            }
            if b.down > 0.001 {
                pose = pose.lerp(rabbit_down(tt, dead), b.down);
            }
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
            // Up when it flees or flies off; down again once it has landed somewhere.
            let flying = matches!(doing, Doing::Flee | Doing::Leave);
            let target = alt.unwrap_or(if flying { FLY_HEIGHT } else { 0.0 });
            let rate = if target > b.alt { 2.2 } else { 1.3 };
            b.alt += (target - b.alt) * (1.0 - (-dt * rate).exp());
            if alt.is_some() {
                b.alt = target;
            }
            let air = smooth01(0.02, 0.25, b.alt);
            // Wingbeats: hard on the climb, slower level, a glide on the way down.
            let climbing = target > b.alt + 0.2 || (alt.is_some() && flying);
            let beat = if climbing { 7.5 } else { 4.0 };
            b.flap = (b.flap + dt * beat * air).rem_euclid(1.0);
            ease(&mut b.moving, local > 0.3 && air < 0.5, 8.0, dt);
            b.phase = (b.phase + dt * 3.2 * b.moving).rem_euclid(1.0);
            let mut pose = bird_ground(b.phase, b.moving, tt);
            if b.feed > 0.001 {
                pose = pose.lerp(bird_peck(tt), b.feed * (1.0 - air));
            }
            if b.rest > 0.001 {
                pose = pose.lerp(bird_perch(tt), b.rest * (1.0 - air));
            }
            if air > 0.001 {
                let glide = !climbing && target < b.alt;
                pose = pose.lerp(bird_fly(b.flap, glide, tt), air);
            }
            if b.down > 0.001 {
                pose = pose.lerp(bird_down(dead), b.down);
            }
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

/// Lying on its side with the body's centre at `centre` (in its own units), legs out: the
/// body's transform.
fn on_side(centre: Vec3, low: f32) -> Transform {
    let r = Quat::from_rotation_z(1.45);
    Transform::from_translation(Vec3::new(0.0, low, 0.0) - r * centre).with_rotation(r)
}

// ---- Deer poses ----

#[derive(Clone, Copy)]
struct DeerPose {
    body: Transform,
    neck: Quat,
    head: Quat,
    tail: Quat,
    ears: [Quat; 2],
    /// Each leg's upper and lower angle (see `leg_ik`).
    legs: [(f32, f32); 4],
}

fn lerp_t(a: Transform, b: Transform, k: f32) -> Transform {
    Transform::from_translation(a.translation.lerp(b.translation, k)).with_rotation(a.rotation.slerp(b.rotation, k))
}

impl DeerPose {
    fn lerp(self, o: DeerPose, k: f32) -> DeerPose {
        DeerPose {
            body: lerp_t(self.body, o.body, k),
            neck: self.neck.slerp(o.neck, k),
            head: self.head.slerp(o.head, k),
            tail: self.tail.slerp(o.tail, k),
            ears: [self.ears[0].slerp(o.ears[0], k), self.ears[1].slerp(o.ears[1], k)],
            legs: std::array::from_fn(|i| (self.legs[i].0 + (o.legs[i].0 - self.legs[i].0) * k, self.legs[i].1 + (o.legs[i].1 - self.legs[i].1) * k)),
        }
    }
}

/// Walk, trot and bound (fore pair then hind pair, with a high leap): each leg's offset in the
/// cycle, the share of it a foot is down, cycles a second and how high a foot lifts.
const DEER_GAITS: [([f32; 4], f32, f32, f32); 3] = [([0.25, 0.75, 0.0, 0.5], 0.64, 1.05, 0.1), ([0.0, 0.5, 0.5, 0.0], 0.45, 1.8, 0.16), ([0.08, 0.14, 0.58, 0.64], 0.26, 1.75, 0.34)];

fn deer_mix(w: Vec3, f: impl Fn(&([f32; 4], f32, f32, f32)) -> f32) -> f32 {
    f(&DEER_GAITS[0]) * w.x + f(&DEER_GAITS[1]) * w.y + f(&DEER_GAITS[2]) * w.z
}

/// Leg angles that put the feet (y, z, in the root's frame) where asked under `body`.
fn deer_legs(body: Transform, feet: [Vec2; 4]) -> [(f32, f32); 4] {
    let (pitch, _, _) = body.rotation.to_euler(EulerRot::XYZ);
    std::array::from_fn(|k| {
        let hip = body.transform_point(D_LEGS[k]);
        let front = k < 2;
        let (up, low) = leg_ik(Vec2::new(hip.y, hip.z), feet[k], if front { D_FRONT } else { D_HIND }, front);
        (up - pitch, low)
    })
}

fn ear_flick(tt: f32, k: usize) -> Quat {
    // Now and then one ear swivels back and forward again.
    let f = smooth01(0.85, 0.95, ((tt * 0.7 + k as f32 * 2.1).sin() * 0.5 + 0.5).powf(3.0));
    Quat::from_rotation_y(f * 0.9 * if k == 0 { -1.0 } else { 1.0 })
}

fn deer_gait(b: &Beast, speed: f32, tt: f32) -> DeerPose {
    let (w, m) = (b.gait, b.moving);
    let duty = deer_mix(w, |g| g.1);
    let lift = deer_mix(w, |g| g.3) * m;
    let stride = (speed * duty / deer_mix(w, |g| g.2)).min(1.0) * m;
    let ph = b.phase;
    let mut feet = D_LEGS.map(|p| Vec2::new(0.0, p.z));
    for (i, f) in feet.iter_mut().enumerate() {
        let p = (ph + deer_mix(w, |g| g.0[i])).rem_euclid(1.0);
        if p < duty {
            f.y += stride * (0.5 - p / duty);
        } else {
            let s = (p - duty) / (1.0 - duty);
            f.y += stride * (smooth01(0.0, 1.0, s) - 0.5);
            f.x = lift * (PI * s).sin().powf(0.8);
        }
    }
    let c = ph * TAU;
    // A bound sails: the body rises between the hind push and the fore landing.
    let leap = 0.22 * w.z * (c - 2.2).sin().max(0.0);
    let bob = (-0.015 * w.x - 0.03 * w.y) * (2.0 * c).cos() + leap - 0.04 * w.z;
    let pitch = 0.14 * w.z * (c - 1.0).sin() + 0.015 * w.x * (2.0 * c).sin();
    let idle = 1.0 - m;
    let body = Transform::from_xyz(0.0, bob * m + idle * 0.004 * (tt * 2.2).sin(), 0.0).with_rotation(ax(pitch * m));
    // Standing it looks about, head high; walking the head nods with each step; bounding the
    // neck stretches forward.
    let look = (tt * 0.31).sin() * 0.5 + (tt * 0.77).sin() * 0.2;
    let neck = Quat::from_rotation_y(idle * look * 0.6) * ax(idle * -0.05 + m * (0.12 * w.x + 0.22 * w.y + 0.3 * w.z + 0.05 * w.x * (2.0 * c).sin() + 0.08 * w.z * (c - 1.0).sin()));
    let head = ax(idle * 0.15 + m * (0.12 - 0.15 * w.z));
    // The tail flicks; at speed it goes up as a white flag.
    let flag = smooth01(3.0, 5.5, speed) * m;
    let tail = ax(-2.3 * flag) * Quat::from_rotation_z((tt * 3.0).sin() * 0.25 * (1.0 - flag) * smooth01(0.6, 0.9, (tt * 0.4).sin()));
    let ears = [0, 1].map(|k| ax(-0.5 * flag) * ear_flick(tt, k) * ax(-0.15 * idle));
    DeerPose { body, neck, head, tail, ears, legs: deer_legs(body, feet) }
}

/// Grazing: head down cropping the grass, up every so often to look round and chew.
fn deer_graze(tt: f32) -> DeerPose {
    let up = smooth01(0.78, 0.9, (tt * 0.45).sin() * 0.5 + 0.5);
    // The fore end dips (fore legs bending) so the muzzle reaches the grass ahead of the hooves.
    let body = Transform::from_xyz(0.0, -0.06 * (1.0 - up), 0.0).with_rotation(ax(0.15 * (1.0 - up)));
    let crop = (tt * 5.0).sin() * 0.06 * (1.0 - up);
    let neck = Quat::from_rotation_y(up * (tt * 0.5).sin() * 0.5) * ax(1.85 * (1.0 - up) - 0.05 * up);
    let head = ax(-0.55 * (1.0 - up) + crop + 0.12 * up) * Quat::from_rotation_z((tt * 4.0).sin() * 0.04 * up);
    let mut feet = D_LEGS.map(|p| Vec2::new(0.0, p.z));
    feet[0].y += 0.1;
    feet[1].y += 0.04;
    let tail = Quat::from_rotation_z((tt * 2.5).sin() * 0.2);
    DeerPose { body, neck, head, tail, ears: [ear_flick(tt, 0), ear_flick(tt, 1)], legs: deer_legs(body, feet) }
}

/// Lying down, legs folded under, head up, chewing the cud.
fn deer_lie(tt: f32) -> DeerPose {
    let body = Transform::from_xyz(0.0, -0.64, 0.0).with_rotation(Quat::from_rotation_z(0.08));
    // Fore legs folded back under the chest, hind legs drawn up with the shanks along the
    // ground.
    let legs = [(-1.25, 2.75), (-1.1, 2.6), (-1.35, 2.75), (-1.2, 2.6)];
    let neck = Quat::from_rotation_y((tt * 0.2).sin() * 0.4) * ax(-0.1);
    let head = ax(0.2) * Quat::from_rotation_z((tt * 3.5).sin() * 0.04);
    DeerPose { body, neck, head, tail: Quat::IDENTITY, ears: [ear_flick(tt, 0), ear_flick(tt, 1)], legs }
}

/// Down on its side, caught (legs kicking) or dead (still).
fn deer_down(tt: f32, dead: bool) -> DeerPose {
    let kick = if dead { 0.0 } else { 1.0 };
    let legs = std::array::from_fn(|k| {
        let w = (tt * 9.0 + k as f32 * 1.7).sin() * 0.35 * kick;
        (0.3 + w + if k < 2 { -0.4 } else { 0.2 }, -0.3 - w * 0.5)
    });
    DeerPose {
        body: on_side(D_CENTRE, 0.17),
        neck: ax(-0.5 + 0.15 * kick * (tt * 6.0).sin()),
        head: ax(0.3),
        tail: Quat::IDENTITY,
        ears: [ax(-0.6), ax(-0.6)],
        legs,
    }
}

// ---- Rabbit poses ----

#[derive(Clone, Copy)]
struct RabbitPose {
    body: Transform,
    head: Quat,
    ears: [Quat; 2],
    hind: [f32; 2],
    front: [f32; 2],
}

impl RabbitPose {
    fn lerp(self, o: RabbitPose, k: f32) -> RabbitPose {
        let l = |a: f32, b: f32| a + (b - a) * k;
        RabbitPose {
            body: lerp_t(self.body, o.body, k),
            head: self.head.slerp(o.head, k),
            ears: [self.ears[0].slerp(o.ears[0], k), self.ears[1].slerp(o.ears[1], k)],
            hind: [l(self.hind[0], o.hind[0]), l(self.hind[1], o.hind[1])],
            front: [l(self.front[0], o.front[0]), l(self.front[1], o.front[1])],
        }
    }
}

/// The body's transform for a pitch about a point (the haunches) rather than its origin.
fn pitched_about(pivot: Vec3, pitch: f32, lift: f32) -> Transform {
    let r = ax(pitch);
    Transform::from_translation(pivot - r * pivot + Vec3::Y * lift).with_rotation(r)
}

/// Hopping: the hind feet drive off, it sails, lands on its fore feet and swings the hind ones
/// up past them. `bolt` (0..1) makes the leaps long and low with the ears laid back.
fn rabbit_hop(p: f32, m: f32, bolt: f32, tt: f32) -> RabbitPose {
    let air_end = 0.55;
    let air = if p < air_end { (PI * p / air_end).sin() } else { 0.0 };
    let height = (0.16 + 0.12 * bolt) * air * m;
    let pitch = if p < air_end { -0.3 * (PI * p / air_end).cos() } else { 0.3 * (PI * (p - air_end) / (1.0 - air_end)).cos() };
    let body = pitched_about(Vec3::new(0.0, 0.2, -0.25), pitch * m * (1.0 - 0.3 * bolt), height);
    let hind = if p < 0.6 { 0.8 - 1.3 * smooth01(0.0, 0.6, p) } else { -0.5 + 1.3 * smooth01(0.6, 1.0, p) };
    let front = if p < 0.45 { 0.6 - 1.3 * smooth01(0.0, 0.45, p) } else { -0.7 + 1.3 * smooth01(0.45, 1.0, p) };
    let idle = 1.0 - m;
    // Standing, the nose twitches and the ears turn this way and that.
    let head = ax(-pitch * 0.5 * m + idle * (0.05 + 0.03 * (tt * 17.0).sin().max(0.0))) * Quat::from_rotation_y(idle * (tt * 0.6).sin() * 0.4);
    let back = -0.25 * m - 0.9 * bolt;
    let ears = [0, 1].map(|k| ax(back) * Quat::from_rotation_y(idle * ear_flick(tt, k).to_axis_angle().1 * 0.5 * if k == 0 { -1.0 } else { 1.0 }));
    RabbitPose { body, head, ears, hind: [hind * m; 2], front: [front * m; 2] }
}

/// Nibbling: hunched, head down, jaw and nose going.
fn rabbit_feed(tt: f32) -> RabbitPose {
    let body = pitched_about(Vec3::new(0.0, 0.2, -0.25), 0.18, -0.02);
    let head = ax(0.55 + 0.04 * (tt * 14.0).sin()) * Quat::from_rotation_z((tt * 7.0).sin() * 0.03);
    let ears = [0, 1].map(|k| ax(-0.3) * ear_flick(tt, k));
    RabbitPose { body, head, ears, hind: [0.1; 2], front: [-0.2, 0.05 * (tt * 2.0).sin() - 0.2] }
}

/// Resting: mostly loafing low with the ears laid along the back, now and then sitting bolt
/// upright on the haunches to look round.
fn rabbit_rest(tt: f32) -> RabbitPose {
    let up = smooth01(0.55, 0.75, (tt * 0.27).sin() * 0.5 + 0.5);
    let loaf = RabbitPose {
        body: Transform::from_xyz(0.0, -0.07, 0.0),
        head: ax(0.1),
        ears: [ax(-1.1) * Quat::from_rotation_z(0.15), ax(-1.1) * Quat::from_rotation_z(-0.15)],
        hind: [-0.1; 2],
        front: [-1.2; 2],
    };
    let sit = RabbitPose {
        body: pitched_about(Vec3::new(0.0, 0.15, -0.32), -1.05, 0.0),
        head: ax(0.9) * Quat::from_rotation_y((tt * 0.9).sin() * 0.6),
        ears: [ax(0.15), ax(0.15)],
        hind: [-0.9; 2],
        front: [0.6; 2],
    };
    loaf.lerp(sit, up)
}

fn rabbit_down(tt: f32, dead: bool) -> RabbitPose {
    let kick = if dead { 0.0 } else { (tt * 11.0).sin() * 0.5 };
    RabbitPose { body: on_side(R_CENTRE, 0.2), head: ax(-0.3), ears: [ax(-1.3), ax(-1.2)], hind: [0.6 + kick, 0.4 - kick], front: [-0.4 - kick * 0.5, -0.2] }
}

// ---- Bird poses ----

#[derive(Clone, Copy)]
struct BirdPose {
    body: Transform,
    head: Quat,
    tail: Quat,
    wings: [Quat; 2],
    legs: [f32; 2],
}

impl BirdPose {
    fn lerp(self, o: BirdPose, k: f32) -> BirdPose {
        BirdPose {
            body: lerp_t(self.body, o.body, k),
            head: self.head.slerp(o.head, k),
            tail: self.tail.slerp(o.tail, k),
            wings: [self.wings[0].slerp(o.wings[0], k), self.wings[1].slerp(o.wings[1], k)],
            legs: [self.legs[0] + (o.legs[0] - self.legs[0]) * k, self.legs[1] + (o.legs[1] - self.legs[1]) * k],
        }
    }
}

/// Wings folded along the back.
fn folded() -> [Quat; 2] {
    [-1.0f32, 1.0].map(|s| Quat::from_rotation_y(s * 1.45) * Quat::from_rotation_x(-0.08) * Quat::from_rotation_z(s * 0.1))
}

/// A head turned in quick jerks, as a bird's is, held between.
fn glance(tt: f32, rate: f32, reach: f32) -> Quat {
    let n = (tt * rate).floor();
    let f = smooth01(0.0, 0.12, (tt * rate).fract());
    let a = (hash(n as i32 as u32) - 0.5) * 2.0 * reach;
    let b = (hash((n - 1.0) as i32 as u32) - 0.5) * 2.0 * reach;
    Quat::from_rotation_y(b + (a - b) * f)
}

/// On the ground: still and glancing about, or hopping along with both feet together.
fn bird_ground(p: f32, m: f32, tt: f32) -> BirdPose {
    let hop = (PI * p).sin() * 0.12 * m;
    let body = Transform::from_xyz(0.0, hop, 0.0).with_rotation(ax(0.05 * m * (TAU * p).sin()));
    let legs = [-0.4 * hop / 0.12; 2];
    BirdPose { body, head: glance(tt, 1.3, 0.8), tail: ax(-0.15 * m * (PI * p).sin()), wings: folded(), legs }
}

/// Pecking at the ground: tipping forward in quick stabs, tail up.
fn bird_peck(tt: f32) -> BirdPose {
    let c = (tt * 1.4).fract();
    let stab = smooth01(0.0, 0.08, c) * (1.0 - smooth01(0.16, 0.3, c)) + smooth01(0.35, 0.42, c) * (1.0 - smooth01(0.48, 0.6, c));
    let body = pitched_about(Vec3::new(0.0, 0.3, 0.0), 0.55 * stab + 0.1, 0.0);
    BirdPose { body, head: ax(0.7 * stab), tail: ax(-0.4 * stab), wings: folded(), legs: [-0.55 * stab - 0.1; 2] }
}

/// Settled and fluffed up, preening now and then.
fn bird_perch(tt: f32) -> BirdPose {
    let preen = smooth01(0.7, 0.8, (tt * 0.35).sin() * 0.5 + 0.5);
    let body = Transform::from_xyz(0.0, -0.08, 0.0).with_scale(Vec3::splat(1.0 + 0.08 * (1.0 - preen)));
    let head = Quat::from_rotation_y(preen * 2.2) * ax(preen * (0.6 + 0.15 * (tt * 9.0).sin())) * glance(tt, 0.7, 0.6 * (1.0 - preen));
    BirdPose { body, head, tail: Quat::IDENTITY, wings: folded(), legs: [0.15; 2] }
}

/// In flight: wings beating (or held out in a glide), legs tucked up.
fn bird_fly(flap: f32, glide: bool, tt: f32) -> BirdPose {
    let a = if glide { 0.12 + 0.06 * (tt * 3.0).sin() } else { 0.85 * (TAU * flap).sin() + 0.1 };
    let wings = [-1.0f32, 1.0].map(|s| Quat::from_rotation_z(s * a) * Quat::from_rotation_x(0.1 * (TAU * flap).cos()));
    let bob = if glide { 0.0 } else { -0.06 * (TAU * flap).sin() };
    BirdPose { body: Transform::from_xyz(0.0, bob, 0.0).with_rotation(ax(-0.05)), head: ax(-0.1), tail: ax(0.1), wings, legs: [1.3; 2] }
}

fn bird_down(dead: bool) -> BirdPose {
    let r = Quat::from_rotation_z(1.4);
    let _ = dead;
    BirdPose {
        body: Transform::from_translation(Vec3::new(0.0, 0.2, 0.0) - r * Vec3::new(0.0, 0.45, 0.0)).with_rotation(r),
        head: ax(0.4),
        tail: Quat::IDENTITY,
        wings: [Quat::from_rotation_z(-0.3), Quat::from_rotation_y(1.2)],
        legs: [0.5, 0.3],
    }
}
