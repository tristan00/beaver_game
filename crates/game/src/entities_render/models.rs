use super::meshgen::{hex, mix, Mb, Rng};
use super::{islands, structures};
use sim::buildings::BuildingKind;
use bevy::prelude::*;
use std::f32::consts::TAU;

pub const HEAD_PIVOT: Vec3 = Vec3::new(0.0, 0.85, 0.62);
pub const TAIL_PIVOT: Vec3 = Vec3::new(0.0, 0.36, -0.6);
pub const LEG_PIVOTS: [Vec3; 4] = [Vec3::new(-0.3, 0.34, 0.4), Vec3::new(0.3, 0.34, 0.4), Vec3::new(-0.3, 0.36, -0.28), Vec3::new(0.3, 0.36, -0.28)];
pub const CHEST_HOLD: Vec3 = Vec3::new(0.0, 0.45, 0.85);
pub const MOUTH: Vec3 = Vec3::new(0.0, -0.28, 0.72);

use crate::palette::{self, pack};
use crate::seasonal::smooth as smooth01;
use super::meshgen::v;
const FUR: u32 = pack(palette::FUR);
const FUR_DARK: u32 = pack(palette::FUR_DARK);
const FUR_LIGHT: u32 = pack(palette::FUR_LIGHT);
const INK: u32 = pack(palette::INK);

fn skin() -> [f32; 3] { mix(hex(FUR_DARK), hex(INK), 0.7) }
fn skin_light() -> [f32; 3] { mix(hex(FUR_DARK), hex(INK), 0.35) }

fn fur_paint(n: Vec3, old: [f32; 3]) -> [f32; 3] {
    let back = mix(hex(FUR_DARK), hex(INK), 0.6);
    let (side, belly) = (hex(FUR), hex(FUR_LIGHT));
    let up = n.y;
    let c = if up > 0.0 { mix(side, back, smooth01(0.1, 0.8, up)) } else { mix(side, belly, smooth01(0.05, 0.6, -up)) };
    mix(old, c, 0.85)
}

pub fn beaver_body(mb: &mut Mb, o: Vec3) { beaver_body_d(mb, o, true) }

fn beaver_body_d(mb: &mut Mb, o: Vec3, detail: bool) {
    let (fd, f, fl) = (hex(FUR_DARK), hex(FUR), hex(FUR_LIGHT));
    let start = mb.len();
    mb.ellipsoid_g(o + v(0.0, 0.57, -0.14), v(0.54, 0.5, 0.62), fd, f, 14, 9);
    mb.ellipsoid_g(o + v(0.0, 0.6, 0.3), v(0.43, 0.43, 0.46), fd, f, 12, 8);
    mb.ellipsoid_g(o + v(0.0, 0.43, 0.16), v(0.37, 0.29, 0.55), mix(f, fl, 0.55), fl, 10, 6);
    for s in [-1.0, 1.0] {
        mb.ellipsoid_g(o + v(0.3 * s, 0.47, -0.32), v(0.24, 0.31, 0.36), fd, f, 10, 6);
    }
    if detail {
        mb.ellipsoid_g(o + v(0.0, 0.72, 0.56), v(0.36, 0.3, 0.16), f, fl, 10, 5);
        for (i, z) in [0.12f32, -0.16, -0.42].into_iter().enumerate() {
            let y = 1.03 - 0.06 * i as f32 - z.abs() * 0.12;
            mb.ellipsoid_r(o + v(0.0, y, z), v(0.11, 0.06, 0.16), Quat::from_rotation_x(0.35), mix(f, fd, 0.35));
        }
    }
    mb.paint(start, |_, n, c| fur_paint(n, c));
}

pub fn beaver_head(mb: &mut Mb, o: Vec3) { beaver_head_d(mb, o, true) }

fn beaver_head_d(mb: &mut Mb, o: Vec3, detail: bool) {
    let (f, fd, fl) = (hex(FUR), hex(FUR_DARK), hex(FUR_LIGHT));
    let start = mb.len();
    mb.ellipsoid_g(o + v(0.0, 0.05, 0.26), v(0.41, 0.37, 0.39), f, fl, 12, 8);
    mb.ellipsoid_g(o + v(0.0, 0.26, 0.2), v(0.3, 0.14, 0.26), f, mix(f, fd, 0.3), 10, 5);
    mb.paint(start, |p, n, c| if p.z - o.z < 0.45 { fur_paint(n, c) } else { c });
    for s in [-1.0, 1.0] {
        mb.ellipsoid(o + v(0.1 * s, -0.11, 0.58), v(0.15, 0.13, 0.15), fl);
    }
    mb.ellipsoid(o + v(0.0, -0.24, 0.5), v(0.13, 0.08, 0.12), mix(fl, hex(0xffffff), 0.25));
    mb.ellipsoid_g(o + v(0.0, 0.0, 0.73), v(0.1, 0.07, 0.07), hex(INK), mix(hex(INK), hex(0x8a6a58), 0.5), 10, 6);
    for s in [-1.0, 1.0] {
        mb.ellipsoid(o + v(0.21 * s, 0.14, 0.5), v(0.095, 0.105, 0.075), hex(INK));
        mb.ellipsoid(o + v(0.185 * s, 0.185, 0.565), v(0.032, 0.036, 0.03), hex(0xffffff));
        mb.ellipsoid(o + v(0.23 * s, 0.1, 0.565), v(0.014, 0.016, 0.014), hex(0xffffff));
        mb.ellipsoid(o + v(0.27 * s, 0.37, 0.1), v(0.12, 0.11, 0.07), mix(fd, hex(INK), 0.3));
        mb.ellipsoid(o + v(0.27 * s, 0.37, 0.12), v(0.07, 0.06, 0.05), hex(0xd89a8a));
        mb.ellipsoid(o + v(0.3 * s, -0.05, 0.42), v(0.1, 0.09, 0.09), fl);
        mb.ellipsoid(o + v(0.3 * s, 0.02, 0.47), v(0.06, 0.035, 0.03), mix(fl, hex(0xe89a8a), 0.55));
        mb.cuboid(o + v(0.045 * s, -0.27, 0.685), v(0.04, 0.062, 0.026), Quat::IDENTITY, hex(0xE9A55C), hex(0xFFE6BC));
        if detail {
            mb.ellipsoid(o + v(0.03 * s, 0.025, 0.79), v(0.022, 0.014, 0.012), hex(0xd8c4b8));
            for k in 0..3 {
                let a = (k as f32 - 1.0) * 0.22;
                let root = o + v(0.15 * s, -0.09 + 0.04 * (1.0 - k as f32), 0.66);
                let tip = root + v(0.36 * s, 0.03 + a * 0.4, -0.08 + a * 0.3);
                mb.cyl(root, tip, 0.011, 0.006, hex(0xfff6e4), hex(0xfff6e4), 3);
            }
        }
    }
}

pub fn beaver_lids(mb: &mut Mb) {
    let f = hex(FUR);
    for s in [-1.0, 1.0] {
        mb.ellipsoid_g(v(0.21 * s, 0.145, 0.512), v(0.106, 0.115, 0.08), f, hex(FUR_LIGHT), 10, 6);
        mb.ellipsoid_r(v(0.21 * s, 0.12, 0.588), v(0.075, 0.014, 0.018), Quat::from_rotation_z(0.12 * s), hex(INK));
    }
}

pub fn beaver_grey(mb: &mut Mb) {
    let g = hex(0xb8b3ab);
    mb.ellipsoid(v(0.0, -0.14, 0.66), v(0.2, 0.1, 0.12), g);
    for s in [-1.0, 1.0] {
        mb.ellipsoid(v(0.17 * s, 0.22, 0.5), v(0.09, 0.03, 0.06), g);
    }
}

pub fn gnaw_chips(mb: &mut Mb) {
    let cut = hex(0xf2d9a8);
    for (p, r) in [(v(0.0, 0.0, 0.0), 0.0f32), (v(0.12, 0.1, -0.05), 0.9), (v(-0.06, 0.16, 0.06), 1.8), (v(0.18, -0.04, 0.08), 2.6)] {
        mb.cuboid(p, v(0.05, 0.016, 0.035), Quat::from_rotation_y(r) * Quat::from_rotation_x(r * 0.7), cut, mix(cut, hex(0xc9a066), 0.5));
    }
}

pub fn beaver_tail(mb: &mut Mb, o: Vec3) { beaver_tail_d(mb, o, true) }

fn beaver_tail_d(mb: &mut Mb, o: Vec3, detail: bool) {
    let (rx, rz, cz) = (0.46f32, 0.66f32, -0.7f32);
    mb.ellipsoid(o + v(0.0, 0.0, -0.08), v(0.17, 0.12, 0.22), mix(hex(FUR_DARK), hex(INK), 0.3));
    let (a, b) = (skin(), skin_light());
    mb.ellipsoid_g(o + v(0.0, 0.0, cz), v(rx, 0.06, rz), a, b, 14, 6);
    if !detail {
        return;
    }
    let scale = mix(skin_light(), hex(INK), 0.45);
    for row in 0..8 {
        let z = -0.2 - row as f32 * 0.13;
        let off = if row % 2 == 0 { 0.0 } else { 0.065 };
        for col in -4..=4 {
            let x = col as f32 * 0.13 + off;
            let (u, w) = (x / rx, (z - cz) / rz);
            let inside = 1.0 - u * u - w * w;
            if inside < 0.18 {
                continue;
            }
            let y = 0.06 * inside.sqrt();
            mb.ellipsoid_g(o + v(x, y, z), v(0.05, 0.012, 0.045), scale, scale, 4, 2);
        }
    }
}

pub fn beaver_leg(mb: &mut Mb, front: bool) {
    let (fd, f) = (hex(FUR_DARK), hex(FUR));
    let start = mb.len();
    if front {
        mb.ellipsoid_g(v(0.0, -0.15, 0.02), v(0.13, 0.21, 0.14), fd, f, 8, 5);
        mb.paint(start, |_, n, c| fur_paint(n, c));
        mb.ellipsoid(v(0.0, -0.3, 0.07), v(0.11, 0.06, 0.13), skin_light());
        for k in 0..3 {
            let x = (k as f32 - 1.0) * 0.06;
            mb.ellipsoid(v(x, -0.31, 0.19), v(0.032, 0.03, 0.045), skin());
        }
    } else {
        mb.ellipsoid_g(v(0.0, -0.15, 0.04), v(0.16, 0.22, 0.2), fd, f, 8, 5);
        mb.paint(start, |_, n, c| fur_paint(n, c));
        mb.ellipsoid_g(v(0.0, -0.335, 0.16), v(0.15, 0.045, 0.26), skin(), skin_light(), 10, 4);
        mb.ellipsoid(v(0.0, -0.33, 0.32), v(0.2, 0.03, 0.12), skin_light());
        for k in 0..5 {
            let a = (k as f32 - 2.0) * 0.22;
            mb.ellipsoid(v(a.sin() * 0.24, -0.325, 0.18 + a.cos() * 0.24), v(0.035, 0.032, 0.05), skin());
        }
    }
}

pub fn beaver_static(mb: &mut Mb) {
    beaver_body_d(mb, Vec3::ZERO, false);
    beaver_head_d(mb, HEAD_PIVOT, false);
    let mut tail = Mb::new();
    beaver_tail_d(&mut tail, Vec3::ZERO, false);
    tail.displace(0, |p| Quat::from_rotation_x(-0.26) * p);
    mb.append(tail, TAIL_PIVOT);
    for (i, p) in LEG_PIVOTS.iter().enumerate() {
        let mut leg = Mb::new();
        beaver_leg(&mut leg, i < 2);
        mb.append(leg, *p);
    }
}

pub fn wake(mb: &mut Mb) {
    let foam = hex(0xF4FBF6);
    for s in [-1.0f32, 1.0] {
        let n = 7;
        let pt = |k: f32, side: f32| {
            let along = k / n as f32;
            let centre = v(s * (0.12 + 0.95 * along), 0.0, -3.4 * along);
            let w = 0.1 * (1.0 - along) + 0.025;
            let out = v(s * 0.94, 0.0, 0.28).normalize();
            centre + out * w * side
        };
        for k in 0..n {
            let (a, b) = (k as f32, k as f32 + 1.0);
            mb.quad2([pt(a, -1.0), pt(a, 1.0), pt(b, 1.0), pt(b, -1.0)], foam);
        }
    }
    let n = 6;
    for k in 0..n {
        let ang = |i: f32| -1.0 + 2.0 * i / n as f32;
        let p = |i: f32, r: f32| v(ang(i).sin() * r, 0.0, ang(i).cos() * r - 0.18);
        let (a, b) = (k as f32, k as f32 + 1.0);
        mb.quad2([p(a, 0.3), p(a, 0.38), p(b, 0.38), p(b, 0.3)], foam);
    }
}

pub fn blob_shadow(mb: &mut Mb) {
    mb.disc(Vec3::ZERO, 0.8, 1.15, [0.0, 0.0, 0.0], 14);
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub struct Season(u8);

impl Season {
    pub const COUNT: usize = 24;
    pub fn all() -> impl Iterator<Item = Self> { (0..Self::COUNT as u8).map(Self) }
    pub fn at(phase: f32) -> Self { Self(((phase.rem_euclid(1.0) * Self::COUNT as f32).round() as usize % Self::COUNT) as u8) }
    pub fn index(self) -> usize { self.0 as usize }
    pub(super) fn phase(self) -> f32 { self.0 as f32 / Self::COUNT as f32 }
}

pub const STAGES: u8 = 6;
pub const DRY_STAGE: u8 = u8::MAX;

pub fn stage_of(progress: f32) -> u8 {
    if progress >= 1.0 { STAGES } else { ((progress.max(0.0) * STAGES as f32) as u8).min(STAGES - 1) }
}

pub(super) fn col(c: palette::Rgb) -> [f32; 3] { hex(pack(c)) }

pub(super) fn leaf_sprig(mb: &mut Mb, p: Vec3, seed: u32) {
    let mut r = Rng(seed * 31 + 7);
    for k in 0..4 {
        let a = k as f32 * 1.6 + r.range(-0.3, 0.3);
        let o = v(a.cos() * 0.09, r.range(-0.04, 0.06), a.sin() * 0.09);
        mb.ellipsoid_r(p + o, v(0.1, 0.025, 0.06), Quat::from_rotation_y(-a) * Quat::from_rotation_z(0.4), mix(col(palette::MARKER_LEAF), hex(0xF2A23C), r.next() * 0.6));
    }
}

pub(super) fn stake(mb: &mut Mb, p: Vec3, h: f32, flag: bool) {
    let top = p + v(0.08, h, 0.04);
    stick_c(mb, p + v(0.0, -0.4, 0.0), top, 0.05, col(palette::PEELED));
    if flag {
        leaf_sprig(mb, top, (p.x * 13.0 + p.z * 7.0).abs() as u32);
    }
}

pub(super) fn site_markers(mb: &mut Mb, hx: f32, hz: f32, y: f32) {
    let corners = [v(-hx, y, -hz), v(hx, y, -hz), v(hx, y, hz), v(-hx, y, hz)];
    let mut r = Rng(97);
    for (i, c) in corners.iter().enumerate() {
        stake(mb, *c, 1.0, true);
        let n = corners[(i + 1) % 4];
        let segs = (((n - *c).length() / 0.7) as usize).max(2);
        for k in 0..segs {
            let (t0, t1) = ((k as f32 + 0.2) / segs as f32, (k as f32 + 0.8) / segs as f32);
            let j = v(r.range(-0.06, 0.06), 0.04, r.range(-0.06, 0.06));
            stick_c(mb, c.lerp(n, t0) + j, c.lerp(n, t1) - j, 0.035, col(palette::PEELED));
        }
    }
}

pub(super) fn stick(mb: &mut Mb, a: Vec3, b: Vec3, r: f32, k: f32) {
    let bark = mix(col(palette::BARK), col(palette::BARK_LIGHT), k);
    stick_c(mb, a, b, r, bark);
}

pub(super) fn stick_c(mb: &mut Mb, a: Vec3, b: Vec3, r: f32, c: [f32; 3]) {
    mb.log(a, b, r, c, mix(c, col(palette::CUT_WOOD), 0.45), 5);
}

pub(super) fn stick_pile(mb: &mut Mb, c: Vec3, yaw: f32, n: usize, seed: u32) {
    let mut r = Rng(seed);
    for i in 0..n {
        let layer = (i / 3) as f32;
        let a = yaw + r.range(-0.35, 0.35);
        let off = v(r.range(-0.3, 0.3), 0.1 + layer * 0.18, r.range(-0.25, 0.25));
        let (d, l) = (v(a.cos(), r.range(-0.05, 0.12), -a.sin()), r.range(0.9, 1.4));
        stick(mb, c + off - d * l * 0.5, c + off + d * l * 0.5, 0.075, r.next());
    }
}

pub(super) fn mud_lump(mb: &mut Mb, c: Vec3, s: f32) {
    mb.ellipsoid_g(c, v(0.55 * s, 0.26 * s, 0.45 * s), col(palette::MUD_DARK), hex(0x77573a), 8, 5);
}

pub(super) fn branch(mb: &mut Mb, a: Vec3, b: Vec3, r: f32, leaf: [f32; 3], seed: u32) {
    let mut rng = Rng(seed);
    stick(mb, a, b, r, rng.next());
    for k in 0..3 {
        let p = a.lerp(b, 0.45 + 0.22 * k as f32) + v(rng.range(-0.1, 0.1), 0.08, rng.range(-0.1, 0.1));
        let s = rng.range(0.18, 0.28);
        mb.ellipsoid_g(p, v(s, s * 0.6, s), mix(leaf, hex(pack(palette::INK)), 0.25), mix(leaf, col(palette::LEAF_LIGHT), 0.5), 7, 4);
    }
}

pub fn footing_of(kind: BuildingKind) -> Option<structures::Footing> {
    use structures::Footing::*;
    let row = kind.row();
    let (x, z) = row.footing_size;
    Some(match row.footing.as_deref()? {
        "mound" => Mound(x),
        "ring" => Ring(x),
        "piles" => Piles(x, z),
        f => panic!("building {}: no footing {f}", kind.key()),
    })
}

pub fn struct_mesh(kind: BuildingKind, stage: u8) -> Mesh {
    let mut mb = Mb::new();
    let dry = stage == DRY_STAGE;
    let f = if dry { 1.0 } else { stage.min(STAGES) as f32 / STAGES as f32 };
    match kind.row().model.as_str() {
        "lodge" => lodge(&mut mb, f),
        "cache_island" => islands::cache_island(&mut mb, f),
        "feed_pile" => islands::feed_pile(&mut mb, f),
        "stone_cache" => islands::stone_cache(&mut mb, f),
        "willow_shallows" => islands::willow_shallows(&mut mb, f),
        "haul_island" => islands::haul_island(&mut mb, f),
        "watch_mound" => islands::watch_mound(&mut mb, f),
        "raft_landing" => structures::raft_landing(&mut mb, f),
        m => panic!("building {}: no model {m}", kind.key()),
    }
    if let Some(fo) = footing_of(kind) {
        let (hx, hz) = match fo {
            structures::Footing::Mound(r) | structures::Footing::Ring(r) => (r, r),
            structures::Footing::Piles(x, z) => (x, z),
        };
        if dry {
            structures::decay(&mut mb, hx, hz);
        }
        structures::footing(&mut mb, fo, dry);
        if dry {
            mb.weather(col(palette::DECAY), 0.15);
        }
    }
    mb.build()
}

pub fn reinforcement_mesh(kind: BuildingKind) -> Mesh {
    let mut mb = Mb::new();
    let (dark, light) = (col(palette::MUD_DARK), col(palette::DAM_MUD));
    let stone = (hex(0x77777c), hex(0xb7b5b2));
    match footing_of(kind).unwrap_or(structures::Footing::Piles(2.5, 1.4)) {
        structures::Footing::Mound(radius) | structures::Footing::Ring(radius) => {
            for k in 0..16 {
                let angle = k as f32 * TAU / 16.0;
                let p = v(angle.cos() * (radius + 0.10), 0.08, angle.sin() * (radius + 0.10));
                if k % 3 == 0 {
                    mb.ellipsoid_g(p, v(0.4, 0.28, 0.34), stone.0, stone.1, 7, 4);
                } else {
                    mb.ellipsoid_r(p, v(0.55, 0.30, 0.40), Quat::from_rotation_y(-angle), if k % 2 == 0 { dark } else { light });
                }
            }
        }
        structures::Footing::Piles(x, z) => {
            let mut r = Rng(55);
            for a in [-1.0, 1.0] {
                for b in [-1.0, 1.0] {
                    let c = v(a * x, 0.0, b * z);
                    mb.ellipsoid_g(c + v(0.0, -0.1, 0.0), v(0.6, 0.45, 0.6), dark, light, 9, 5);
                    for _ in 0..3 {
                        let o = v(r.range(-0.45, 0.45), r.range(-0.05, 0.15), r.range(-0.45, 0.45));
                        let s = r.range(0.14, 0.24);
                        mb.ellipsoid_g(c + o, v(s * 1.2, s * 0.8, s), stone.0, stone.1, 7, 4);
                    }
                }
            }
        }
    }
    mb.build()
}

pub fn pebble_facing_mesh(kind: BuildingKind) -> Mesh {
    let mut mb = Mb::new();
    let (dark, light) = (hex(0x6e6e74), hex(0xc2c0bc));
    let mut r = Rng(77);
    let (rx, rz, n) = match footing_of(kind).unwrap_or(structures::Footing::Piles(2.5, 1.4)) {
        structures::Footing::Mound(radius) | structures::Footing::Ring(radius) => (radius + 0.45, radius + 0.45, 22),
        structures::Footing::Piles(x, z) => (x + 0.55, z + 0.55, 20),
    };
    for k in 0..n {
        let a = k as f32 * TAU / n as f32 + r.range(-0.05, 0.05);
        let p = v(a.cos() * rx, 0.02, a.sin() * rz);
        let s = r.range(0.28, 0.4);
        mb.ellipsoid_r(p, v(s * 1.3, s * 0.75, s), Quat::from_rotation_y(-a + r.range(-0.3, 0.3)), mix(dark, light, r.next()));
    }
    mb.build()
}

fn seedling_sprig(mb: &mut Mb, a: Vec3, b: Vec3) {
    stick_c(mb, a, b, 0.03, hex(0x7a5a34));
    mb.ellipsoid_g(b, v(0.16, 0.12, 0.16), col(palette::LEAF), col(palette::LEAF_LIGHT), 7, 5);
    for (k, s) in [(0.55f32, 1.0f32), (0.75, -1.0)] {
        let p = a.lerp(b, k);
        mb.ellipsoid_g(p + v(0.1 * s, 0.02, 0.04), v(0.1, 0.07, 0.09), col(palette::LEAF), col(palette::LEAF_LIGHT), 6, 4);
    }
    mb.ellipsoid_g(a, v(0.11, 0.08, 0.11), col(palette::MUD_DARK), hex(0x77573a), 6, 4);
}

fn lodge(mb: &mut Mb, f: f32) {
    let (rad, hgt) = (3.3f32, 3.0f32);
    let done = f >= 1.0;
    let built = if done { 1.0 } else { 0.22 + 0.7 * f };
    let shape = |t: f32| rad * (1.0 - t.powf(1.5)).max(0.0).powf(0.75);
    let n = 9;
    let top_t = built.min(0.97);
    let mut prof: Vec<(f32, f32)> = (0..=n).map(|k| {
        let t = top_t * k as f32 / n as f32;
        (shape(t), hgt * t)
    }).collect();
    if done {
        prof.push((0.0, hgt * 0.99));
    }
    let start = mb.len();
    mb.lathe(Mat4::IDENTITY, &prof, 20, col(palette::MUD_DARK), col(palette::DAM_MUD), true);
    if !done {
        let y = hgt * top_t;
        mb.lathe(Mat4::from_translation(v(0.0, y - 0.04, 0.0)), &[(shape(top_t) * 0.92, 0.0), (0.0, -0.3)], 18, hex(0x24170e), hex(0x24170e), false);
    }
    let wob = |p: Vec3| {
        let a = p.z.atan2(p.x);
        let k = 1.0 + 0.06 * (3.0 * a + 1.3 + p.y * 0.8).sin() + 0.04 * (7.0 * a + p.y * 2.1).sin();
        v(p.x * k, p.y * (1.0 + 0.03 * (5.0 * a).sin()), p.z * k)
    };
    mb.displace(start, wob);
    let surface = |t: f32, a: f32| wob(v(shape(t) * a.cos(), hgt * t, shape(t) * a.sin()));
    let mut r = Rng(1234);
    let count = (150.0 * built) as usize;
    for i in 0..count {
        let t = r.range(0.02, (top_t - 0.04).max(0.05));
        let a = r.range(0.0, TAU);
        let p = surface(t, a);
        let down = (surface((t - 0.05).max(0.0), a) - surface((t + 0.05).min(0.99), a)).normalize_or_zero();
        let around = v(-a.sin(), 0.0, a.cos());
        let out = around.cross(down).normalize_or_zero();
        let out = if out.dot(v(p.x, 0.0, p.z)) < 0.0 { -out } else { out };
        let phi = if i % 2 == 0 { r.range(0.35, 1.2) } else { -r.range(0.35, 1.2) };
        let dir = (down * phi.cos() + around * phi.sin()).normalize();
        let l = r.range(1.0, 1.9);
        let rr = r.range(0.06, 0.095);
        let c = p + out * rr * 0.7;
        let k = r.next();
        if k < 0.12 {
            stick_c(mb, c - dir * l * 0.5, c + dir * l * 0.5, rr, mix(col(palette::PEELED), col(palette::BARK_LIGHT), 0.3));
        } else {
            stick(mb, c - dir * l * 0.5, c + dir * l * 0.5, rr, k);
        }
    }
    if done {
        let mut m = Rng(4321);
        for _ in 0..16 {
            let (t, a) = (m.range(0.05, 0.7), m.range(0.0, TAU));
            let p = surface(t, a);
            let up = (p - v(0.0, p.y * 0.3, 0.0)).normalize_or_zero();
            let s = m.range(0.4, 0.65);
            mb.ellipsoid_r(p, v(s, 0.06, s * 0.8), Quat::from_rotation_arc(Vec3::Y, up), mix(col(palette::MUD_DARK), col(palette::DAM_MUD), 0.4 + 0.4 * m.next()));
        }
        for k in 0..7 {
            let a = k as f32 * TAU / 7.0 + m.range(-0.2, 0.2);
            let base = v(a.cos() * 0.35, hgt * 0.9, a.sin() * 0.35);
            stick_c(mb, base, base + v(a.cos() * 0.25, m.range(0.5, 0.8), a.sin() * 0.25), 0.06, col(palette::BARK_DARK));
        }
        for _ in 0..5 {
            let (t, a) = (m.range(0.15, 0.55), m.range(0.0, TAU));
            let s = m.range(0.3, 0.5);
            mb.ellipsoid_g(surface(t, a), v(s, s * 0.35, s), col(palette::LEAF), col(palette::LEAF_LIGHT), 8, 4);
        }
    } else {
        site_markers(mb, rad + 1.3, rad + 1.3, 0.0);
        stick_pile(mb, v(-3.2, 0.0, 3.9), 0.8, 9, 17);
        mud_lump(mb, v(3.9, 0.15, 2.9), 1.4);
        mud_lump(mb, v(-4.2, 0.1, -1.2), 1.1);
    }
}

pub fn load_mesh(good: sim::buildings::Good) -> Mesh {
    let mut mb = Mb::new();
    match good.row().model.as_str() {
        "sticks" => {
            for (i, dy) in [-0.02f32, 0.05, 0.12].iter().enumerate() {
                let k = i as f32 - 1.0;
                stick(&mut mb, v(-0.55, 0.04 + dy, -0.06 + k * 0.05), v(0.55, 0.08 + dy, -0.06 - k * 0.07), 0.035, 0.3 + 0.3 * i as f32);
            }
            mb.ellipsoid(v(0.5, 0.16, -0.1), v(0.1, 0.03, 0.06), col(palette::LEAF_LIGHT));
            mb.ellipsoid(v(-0.5, 0.16, -0.02), v(0.1, 0.03, 0.06), col(palette::LEAF));
        }
        "mud" => {
            mb.ellipsoid_g(v(0.0, -0.02, 0.02), v(0.2, 0.15, 0.17), col(palette::MUD_DARK), hex(0x80603f), 8, 5);
        }
        "pebbles" => {
            mb.ellipsoid_g(v(0.0, -0.02, 0.02), v(0.18, 0.13, 0.15), hex(0x66666b), hex(0xb5b3ae), 7, 5);
        }
        "seedlings" => seedling_sprig(&mut mb, v(-0.05, -0.15, 0.0), v(0.05, 0.4, 0.02)),
        "food" => {
            stick_c(&mut mb, v(-0.4, 0.0, 0.0), v(0.4, 0.05, 0.02), 0.025, hex(0x6f8a3a));
            for k in 0..5 {
                let x = -0.35 + k as f32 * 0.175;
                mb.ellipsoid_r(v(x, 0.06, 0.04 * (k % 2) as f32), v(0.12, 0.03, 0.07), Quat::from_rotation_y(0.5 * (k as f32 - 2.0)), mix(col(palette::LEAF), col(palette::LEAF_LIGHT), 0.25 * (k % 3) as f32));
            }
        }
        m => panic!("good {}: no model {m}", good.key()),
    }
    mb.build()
}

pub fn paw_load_mesh(good: sim::buildings::Good) -> Mesh {
    let mut mb = Mb::new();
    match good.row().model.as_str() {
        "sticks" => {
            for k in 0..3 {
                let y = k as f32 * 0.07 - 0.07;
                stick(&mut mb, v(-0.45, y, 0.0), v(0.45, y + 0.03, 0.02 * k as f32), 0.04, 0.3 * k as f32);
            }
        }
        "mud" => {
            mb.ellipsoid_g(v(0.0, 0.0, 0.0), v(0.26, 0.22, 0.22), col(palette::MUD_WET), col(palette::DAM_MUD), 9, 6);
            mb.ellipsoid(v(0.08, 0.13, 0.05), v(0.1, 0.06, 0.08), hex(0x6a4c33));
        }
        "pebbles" => {
            mb.ellipsoid_g(v(0.0, 0.0, 0.0), v(0.27, 0.2, 0.22), hex(0x5f5f64), hex(0xbab8b3), 8, 5);
        }
        "seedlings" => seedling_sprig(&mut mb, v(-0.12, -0.12, 0.02), v(0.42, 0.3, 0.2)),
        "food" => {
            for k in 0..3 {
                let a = (k as f32 - 1.0) * 0.35;
                let d = v(a.sin(), 0.25, a.cos() * 0.3).normalize() * 0.4;
                stick_c(&mut mb, -d * 0.3, d, 0.025, hex(0x6f8a3a));
                mb.ellipsoid_g(d * 0.85, v(0.16, 0.1, 0.14), col(palette::LEAF), col(palette::LEAF_LIGHT), 7, 4);
            }
        }
        m => panic!("good {}: no model {m}", good.key()),
    }
    mb.build()
}

pub fn felled_mesh(logs: u8, branches: bool) -> Mesh {
    let mut mb = Mb::new();
    let (bark, cut) = (hex(0x7a5230), hex(0xeed3a0));
    mb.append(stump_low(), Vec3::ZERO);
    for i in 0..logs as usize {
        let x = 0.9 + i as f32 * 1.7;
        let z = 0.12 * (i as f32 * 2.1).sin();
        mb.log(v(x, 0.34, z), v(x + 1.6, 0.34, z + 0.05), 0.32 - 0.03 * i as f32, bark, cut, 9);
        mb.ellipsoid(v(x + 0.8, 0.0, z), v(0.8, 0.09, 0.3), hex(0x2f6a2a));
    }
    if branches {
        let x0 = 0.9 + logs as f32 * 1.7 + 0.2;
        for k in 0..5 {
            let a = (k as f32 - 2.0) * 0.38;
            stick(&mut mb, v(x0, 0.3, 0.0), v(x0 + 1.5 * a.cos(), 0.55 + 0.12 * (k % 3) as f32, 1.5 * a.sin()), 0.07, 0.4);
        }
        mb.ellipsoid_g(v(x0 + 1.0, 0.7, 0.0), v(1.1, 0.55, 0.9), hex(0x3f7f2f), hex(0x7bb04a), 8, 5);
        mb.ellipsoid(v(x0 + 1.5, 0.85, 0.4), v(0.6, 0.35, 0.5), hex(0x5fae3c));
    }
    mb.build()
}

fn stump_low() -> Mb {
    let mut mb = Mb::new();
    let bark = hex(0x7a5230);
    mb.lathe(Mat4::IDENTITY, &[(0.0, -0.1), (0.55, -0.1), (0.4, 0.12), (0.34, 0.3)], 10, bark, bark, true);
    mb.lathe(Mat4::IDENTITY, &[(0.34, 0.3), (0.0, 0.32)], 10, hex(0xeed3a0), hex(0xf6e2b8), false);
    mb
}

pub fn heap_mesh(good: sim::buildings::Good) -> Mesh {
    let mut mb = Mb::new();
    match good.row().model.as_str() {
        "sticks" => stick_pile(&mut mb, v(0.0, 0.0, 0.0), 0.4, 7, 11),
        "mud" => {
            for (x, z, r, h) in [(0.0f32, 0.0f32, 0.42f32, 0.2f32), (0.3, 0.22, 0.25, 0.14), (-0.28, 0.18, 0.22, 0.12)] {
                mb.ellipsoid_g(v(x, 0.0, z), v(r, h, r * 0.85), col(palette::MUD_DARK), hex(0x80603f), 9, 5);
            }
        }
        "pebbles" => {
            for (k, (x, z)) in [(0.0f32, 0.0f32), (0.22, 0.12), (-0.2, 0.15), (0.05, -0.22), (-0.15, -0.12)].into_iter().enumerate() {
                let r = 0.14 + 0.03 * (k % 2) as f32;
                mb.ellipsoid_g(v(x, r * 0.6, z), v(r, r * 0.75, r * 0.9), hex(0x66666b), hex(0xb5b3ae), 7, 5);
            }
        }
        "seedlings" => {
            for k in 0..3 {
                let x = -0.2 + 0.2 * k as f32;
                seedling_sprig(&mut mb, v(x, 0.0, 0.05 * k as f32), v(x + 0.05, 0.45, 0.05 * k as f32));
            }
        }
        "food" => {
            mb.disc(v(0.0, 0.015, 0.0), 0.56, 0.5, mix(col(palette::MUD_DARK), col(palette::BARK), 0.3), 14);
            let mut r = Rng(77);
            for k in 0..6 {
                let a = k as f32 * 1.05 + r.range(-0.2, 0.2);
                let (dx, dz) = (0.44 * a.cos(), 0.44 * a.sin());
                let y = 0.05 + 0.045 * (k / 2) as f32;
                let c = if k % 2 == 0 { col(palette::CUT_WOOD) } else { col(palette::BARK_LIGHT) };
                stick_c(&mut mb, v(-dx, y, -dz), v(dx, y + 0.03, dz), 0.035, c);
            }
            for k in 0..4 {
                let a = k as f32 * 1.6 + 0.4;
                mb.ellipsoid_r(v(0.34 * a.cos(), 0.06, 0.34 * a.sin()), v(0.12, 0.025, 0.05), Quat::from_rotation_y(a + 0.6), col(palette::BARK));
            }
            let dark = mix(col(palette::LEAF), col(palette::BARK_DARK), 0.25);
            for (x, z, y, k) in [(0.0f32, 0.0f32, 0.2f32, 1.0f32), (0.15, 0.08, 0.15, 0.8), (-0.14, 0.1, 0.14, 0.8), (0.05, -0.15, 0.14, 0.75), (-0.08, -0.07, 0.27, 0.6)] {
                mb.ellipsoid_g(v(x, y, z), v(0.18 * k, 0.11 * k, 0.15 * k), dark, col(palette::LEAF_LIGHT), 7, 5);
            }
        }
        m => panic!("good {}: no model {m}", good.key()),
    }
    mb.build()
}

pub fn supplies_mesh() -> Mesh {
    let mut mb = Mb::new();
    for k in 0..9 {
        let (y, z) = (0.1 + 0.09 * (k / 3) as f32, -0.1 + 0.1 * (k % 3) as f32);
        stick(&mut mb, v(-0.7, y, z), v(0.7, y + 0.02, z + 0.02), 0.05, 0.2 + 0.08 * k as f32);
    }
    for x in [-0.4f32, 0.4] {
        mb.cyl(v(x, 0.0, -0.22), v(x, 0.42, 0.22), 0.03, 0.03, hex(0x9c7a4a), hex(0xb8935c), 5);
    }
    mb.ellipsoid_g(v(0.1, 0.0, 0.55), v(0.4, 0.24, 0.32), col(palette::MUD_DARK), hex(0x80603f), 9, 5);
    for k in 0..5 {
        let a = k as f32 * 1.25;
        mb.ellipsoid_r(v(-0.45 + 0.14 * a.cos(), 0.08 + 0.04 * (k % 2) as f32, 0.5 + 0.14 * a.sin()), v(0.17, 0.04, 0.09), Quat::from_rotation_y(a), mix(col(palette::LEAF), col(palette::LEAF_LIGHT), 0.3 * (k % 3) as f32));
    }
    mb.build()
}
