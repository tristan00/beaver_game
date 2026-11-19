use super::meshgen::{hex, mix, Mb, Rng};
use super::structures;
use super::scene::{Pond, Species, StructKind};
use bevy::prelude::*;
use std::f32::consts::{PI, TAU};

fn v(x: f32, y: f32, z: f32) -> Vec3 { Vec3::new(x, y, z) }

pub const HEAD_PIVOT: Vec3 = Vec3::new(0.0, 0.85, 0.62);
pub const TAIL_PIVOT: Vec3 = Vec3::new(0.0, 0.36, -0.6);
pub const LEG_PIVOTS: [Vec3; 4] = [Vec3::new(-0.3, 0.34, 0.4), Vec3::new(0.3, 0.34, 0.4), Vec3::new(-0.33, 0.36, -0.28), Vec3::new(0.33, 0.36, -0.28)];

use crate::palette::{self, pack};
const FUR: u32 = pack(palette::FUR);
const FUR_DARK: u32 = pack(palette::FUR_DARK);
const FUR_LIGHT: u32 = pack(palette::FUR_LIGHT);
const INK: u32 = pack(palette::INK);

fn skin() -> [f32; 3] { mix(hex(FUR_DARK), hex(INK), 0.55) }
fn skin_light() -> [f32; 3] { mix(hex(FUR_DARK), hex(INK), 0.25) }

pub fn beaver_body(mb: &mut Mb, o: Vec3) { beaver_body_d(mb, o, true) }

fn beaver_body_d(mb: &mut Mb, o: Vec3, detail: bool) {
    let (fd, f, fl) = (hex(FUR_DARK), hex(FUR), hex(FUR_LIGHT));
    mb.ellipsoid_g(o + v(0.0, 0.57, -0.14), v(0.54, 0.5, 0.62), fd, f, 14, 9);
    mb.ellipsoid_g(o + v(0.0, 0.6, 0.3), v(0.43, 0.43, 0.46), fd, f, 12, 8);
    mb.ellipsoid_g(o + v(0.0, 0.43, 0.16), v(0.37, 0.29, 0.55), mix(f, fl, 0.55), fl, 10, 6);
    for s in [-1.0, 1.0] {
        mb.ellipsoid_g(o + v(0.37 * s, 0.47, -0.32), v(0.26, 0.33, 0.37), fd, f, 10, 6);
    }
    if detail {
        mb.ellipsoid_g(o + v(0.0, 0.72, 0.56), v(0.36, 0.3, 0.16), f, fl, 10, 5);
        for (i, z) in [0.12f32, -0.16, -0.42].into_iter().enumerate() {
            let y = 1.03 - 0.06 * i as f32 - z.abs() * 0.12;
            mb.ellipsoid_r(o + v(0.0, y, z), v(0.11, 0.06, 0.16), Quat::from_rotation_x(0.35), mix(f, fd, 0.35));
        }
    }
}

pub fn beaver_head(mb: &mut Mb, o: Vec3) { beaver_head_d(mb, o, true) }

fn beaver_head_d(mb: &mut Mb, o: Vec3, detail: bool) {
    let (f, fd, fl) = (hex(FUR), hex(FUR_DARK), hex(FUR_LIGHT));
    mb.ellipsoid_g(o + v(0.0, 0.05, 0.26), v(0.41, 0.37, 0.39), f, fl, 12, 8);
    mb.ellipsoid_g(o + v(0.0, 0.26, 0.2), v(0.3, 0.14, 0.26), f, mix(f, fd, 0.3), 10, 5);
    for s in [-1.0, 1.0] {
        mb.ellipsoid(o + v(0.1 * s, -0.11, 0.58), v(0.15, 0.13, 0.15), fl);
    }
    mb.ellipsoid(o + v(0.0, -0.24, 0.5), v(0.13, 0.08, 0.12), mix(fl, hex(0xffffff), 0.25));
    mb.ellipsoid_g(o + v(0.0, 0.0, 0.73), v(0.1, 0.07, 0.07), hex(INK), mix(hex(INK), hex(0x8a6a58), 0.5), 10, 6);
    for s in [-1.0, 1.0] {
        mb.ellipsoid(o + v(0.21 * s, 0.14, 0.5), v(0.095, 0.105, 0.075), hex(INK));
        mb.ellipsoid(o + v(0.185 * s, 0.185, 0.565), v(0.032, 0.036, 0.03), hex(0xffffff));
        mb.ellipsoid(o + v(0.23 * s, 0.1, 0.565), v(0.014, 0.016, 0.014), hex(0xffffff));
        mb.ellipsoid(o + v(0.27 * s, 0.37, 0.1), v(0.12, 0.11, 0.07), fd);
        mb.ellipsoid(o + v(0.27 * s, 0.37, 0.12), v(0.07, 0.06, 0.05), hex(0xd89a8a));
        mb.ellipsoid(o + v(0.3 * s, -0.05, 0.42), v(0.1, 0.09, 0.09), fl);
        mb.ellipsoid(o + v(0.3 * s, 0.02, 0.47), v(0.06, 0.035, 0.03), mix(fl, hex(0xe89a8a), 0.55));
        mb.cuboid(o + v(0.047 * s, -0.31, 0.69), v(0.044, 0.095, 0.03), Quat::IDENTITY, hex(0xfff0c8), hex(0xffffff));
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

pub fn gnaw_chips(mb: &mut Mb) {
    let cut = hex(0xf2d9a8);
    for (p, r) in [(v(0.0, 0.0, 0.0), 0.0f32), (v(0.12, 0.1, -0.05), 0.9), (v(-0.06, 0.16, 0.06), 1.8), (v(0.18, -0.04, 0.08), 2.6)] {
        mb.cuboid(p, v(0.05, 0.016, 0.035), Quat::from_rotation_y(r) * Quat::from_rotation_x(r * 0.7), cut, mix(cut, hex(0xc9a066), 0.5));
    }
}

pub fn beaver_tail(mb: &mut Mb, o: Vec3) { beaver_tail_d(mb, o, true) }

fn beaver_tail_d(mb: &mut Mb, o: Vec3, detail: bool) {
    mb.ellipsoid(o + v(0.0, 0.0, -0.08), v(0.17, 0.12, 0.22), hex(FUR_DARK));
    let (a, b) = (skin(), skin_light());
    mb.ellipsoid_g(o + v(0.0, 0.0, -0.6), v(0.4, 0.06, 0.56), a, b, 14, 6);
    if !detail {
        return;
    }
    let scale = mix(skin_light(), hex(FUR_DARK), 0.35);
    for row in 0..7 {
        let z = -0.2 - row as f32 * 0.13;
        let off = if row % 2 == 0 { 0.0 } else { 0.065 };
        for col in -3..=3 {
            let x = col as f32 * 0.13 + off;
            let (u, w) = (x / 0.4, (z + 0.6) / 0.56);
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
    if front {
        mb.ellipsoid_g(v(0.0, -0.15, 0.02), v(0.13, 0.21, 0.14), fd, f, 8, 5);
        mb.ellipsoid(v(0.0, -0.3, 0.07), v(0.11, 0.06, 0.13), skin_light());
        for k in 0..3 {
            let x = (k as f32 - 1.0) * 0.06;
            mb.ellipsoid(v(x, -0.31, 0.19), v(0.032, 0.03, 0.045), skin());
        }
    } else {
        mb.ellipsoid_g(v(0.0, -0.15, 0.04), v(0.16, 0.22, 0.2), fd, f, 8, 5);
        mb.ellipsoid_g(v(0.0, -0.335, 0.16), v(0.15, 0.045, 0.26), skin(), skin_light(), 10, 4);
        mb.ellipsoid(v(0.0, -0.33, 0.32), v(0.2, 0.03, 0.12), skin_light());
        for k in 0..5 {
            let a = (k as f32 - 2.0) * 0.22;
            mb.ellipsoid(v(a.sin() * 0.24, -0.325, 0.18 + a.cos() * 0.24), v(0.035, 0.032, 0.05), skin());
        }
    }
}

pub fn beaver_log(mb: &mut Mb) {
    mb.log(v(-0.62, -0.22, 0.66), v(0.62, -0.22, 0.66), 0.1, hex(0x7a5230), hex(0xecd2a0), 8);
    mb.cyl(v(-0.3, -0.2, 0.62), v(-0.15, 0.0, 0.8), 0.025, 0.015, hex(0x7a5230), hex(0x7a5230), 4);
    mb.ellipsoid(v(-0.14, 0.02, 0.8), v(0.1, 0.03, 0.06), hex(0x76c24a));
}

pub fn beaver_static(mb: &mut Mb) {
    beaver_body_d(mb, Vec3::ZERO, false);
    beaver_head_d(mb, HEAD_PIVOT, false);
    beaver_tail_d(mb, TAIL_PIVOT, false);
    for (i, p) in LEG_PIVOTS.iter().enumerate() {
        let mut leg = Mb::new();
        beaver_leg(&mut leg, i < 2);
        mb.append(leg, *p);
    }
}

pub fn pond_surface(p: &Pond) -> Mesh {
    let rim: Vec<Vec3> = (0..96)
        .map(|i| {
            let a = i as f32 / 96.0 * TAU;
            let r = p.radius(a) + 0.9;
            Vec3::new(a.sin() * r, 0.0, a.cos() * r)
        })
        .collect();
    let mut mb = Mb::new();
    mb.fan(Vec3::ZERO, &rim, [1.0; 3]);
    mb.build()
}

pub fn ripple(mb: &mut Mb) {
    let foam = hex(0xF4FBF6);
    for k in 0..3 {
        let (a0, a1) = (k as f32 * TAU / 3.0 + 0.25, k as f32 * TAU / 3.0 + 0.25 + 1.5);
        let pts = |r: f32| (0..=8).map(move |i| { let a = a0 + (a1 - a0) * i as f32 / 8.0; Vec3::new(a.sin() * r, 0.0, a.cos() * r) }).collect::<Vec<_>>();
        let (inner, outer) = (pts(1.0), pts(1.2));
        for i in 0..8 {
            mb.fan(inner[i], &[inner[i], outer[i], outer[i + 1], inner[i + 1]], foam);
        }
    }
}

pub fn blob_shadow(mb: &mut Mb) {
    mb.disc(Vec3::ZERO, 0.8, 1.15, [0.0, 0.0, 0.0], 14);
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum Season {
    Spring,
    Summer,
    Autumn,
    Winter,
}

impl Season {
    pub const ALL: [Season; 4] = [Season::Spring, Season::Summer, Season::Autumn, Season::Winter];
    pub fn index(self) -> usize { self as usize }
}

fn foliage(sp: Species, s: Season) -> ([f32; 3], [f32; 3]) {
    let (a, b) = match (sp, s) {
        (Species::Aspen, Season::Spring) => (0x9ad86a, 0xd0f58f),
        (Species::Aspen, Season::Summer) => (0x5cb347, 0x9be066),
        (Species::Aspen, _) => (0xeaa21c, 0xffe055),
        (Species::Maple, Season::Spring) => (0x78c655, 0xb9ec7c),
        (Species::Maple, Season::Summer) => (0x5FA85A, 0x8CD070),
        (Species::Maple, _) => (0xe0602e, 0xffa63d),
        (Species::Pine, Season::Spring) => (0x5FA85A, 0x8CD070),
        (Species::Pine, Season::Summer) => (0x5FA85A, 0x8CD070),
        (Species::Pine, Season::Autumn) => (0x6AA85A, 0x98CC6C),
        (Species::Pine, Season::Winter) => (0x78B598, 0xA8D4BC),
        (Species::Willow, Season::Spring) => (0xa5de7c, 0xd6f59d),
        (Species::Willow, Season::Summer) => (0x7ac55e, 0xb0e47c),
        (Species::Willow, Season::Autumn) => (0xd6bd3c, 0xf3e776),
        (Species::Willow, Season::Winter) => (0xc2b060, 0xd8c880),
    };
    (hex(a), hex(b))
}

fn trunk_color(sp: Species) -> [f32; 3] {
    hex(match sp {
        Species::Aspen => 0xe6dccb,
        Species::Maple => 0x7d5632,
        Species::Pine => 0x6b4630,
        Species::Willow => 0x6c5a3e,
    })
}

fn bare(sp: Species, s: Season) -> bool {
    s == Season::Winter && sp != Species::Pine
}

pub fn tree_mesh(sp: Species, season: Season, lod: u8) -> Mesh {
    let mut mb = Mb::new();
    let (dark, light) = foliage(sp, season);
    let trunk = trunk_color(sp);
    let ground = hex(0x7B8B77);
    let shade_r = match sp {
        Species::Aspen => 1.3,
        Species::Maple => 1.9,
        Species::Pine => 1.6,
        Species::Willow => 2.1,
    };
    if lod == 0 {
        mb.disc(v(0.0, 0.04, 0.0), shade_r, shade_r, ground, 10);
    }
    let bare = bare(sp, season);
    let (seg, rings) = match lod { 0 => (9, 6), 1 => (6, 4), _ => (5, 3) };
    let top_h = match sp {
        Species::Aspen => 4.9,
        Species::Maple => 4.4,
        Species::Pine => 5.8,
        Species::Willow => 4.2,
    };
    match lod {
        0 => match sp {
            Species::Aspen => {
                root_flare(&mut mb, 0.19, trunk);
                mb.cyl(v(0.0, -0.1, 0.0), v(0.0, 2.9, 0.0), 0.19, 0.1, trunk, trunk, 7);
                bark_eyes(&mut mb, 2.9, 0.19, 0.1, 9, 11);
                if bare {
                    twigs(&mut mb, v(0.0, 2.2, 0.0), 6, 1.3, 1.7, hex(0xa89c88), 1);
                } else {
                    crown(&mut mb, v(0.0, 3.45, 0.0), v(0.95, 1.35, 0.95), 11, 0.52, dark, light, 3);
                    crown(&mut mb, v(0.1, 4.45, 0.0), v(0.45, 0.5, 0.45), 4, 0.36, dark, light, 4);
                }
            }
            Species::Maple => {
                mb.lathe(Mat4::IDENTITY, &[(0.0, -0.1), (0.55, -0.1), (0.42, 0.08), (0.3, 0.45), (0.22, 1.6), (0.0, 1.6)], 9, mix(trunk, hex(INK), 0.2), trunk, false);
                mb.cyl(v(0.0, 1.4, 0.0), v(0.7, 2.4, 0.2), 0.14, 0.08, trunk, trunk, 6);
                mb.cyl(v(0.0, 1.4, 0.0), v(-0.6, 2.3, -0.3), 0.14, 0.08, trunk, trunk, 6);
                mb.cyl(v(0.0, 1.5, 0.0), v(0.15, 2.5, -0.75), 0.12, 0.07, trunk, trunk, 6);
                if bare {
                    twigs(&mut mb, v(0.0, 1.8, 0.0), 8, 1.5, 1.9, hex(0x6b4a30), 2);
                } else {
                    crown(&mut mb, v(0.0, 3.05, 0.0), v(1.5, 1.2, 1.5), 13, 0.72, dark, light, 7);
                    crown(&mut mb, v(0.0, 3.95, 0.1), v(0.85, 0.7, 0.85), 5, 0.55, dark, light, 8);
                    if season == Season::Spring {
                        let mut r = Rng(77);
                        for _ in 0..11 {
                            let (a, e) = (r.range(0.0, TAU), r.range(0.1, 1.1));
                            let p = v(a.cos() * e.cos() * 1.7, 3.1 + e.sin() * 1.25, a.sin() * e.cos() * 1.7);
                            mb.ellipsoid(p, v(0.24, 0.2, 0.24), hex(0xffb3d1));
                        }
                    }
                }
            }
            Species::Pine => {
                mb.cyl(v(0.0, -0.1, 0.0), v(0.0, 1.5, 0.0), 0.24, 0.16, trunk, trunk, 7);
                root_flare(&mut mb, 0.24, trunk);
                let top = if season == Season::Winter { hex(0xf4f9ff) } else { light };
                let tiers = [(0.95f32, 1.55f32, 1.35f32), (1.85, 1.28, 1.2), (2.7, 1.0, 1.1), (3.45, 0.74, 1.0), (4.1, 0.46, 0.85)];
                for (k, (y0, rr, h)) in tiers.into_iter().enumerate() {
                    let prof = [(0.12, y0 + 0.3), (rr * 0.88, y0 - 0.04), (rr, y0 + 0.1), (rr * 0.74, y0 + h * 0.42), (rr * 0.32, y0 + h * 0.82), (0.0, y0 + h)];
                    let under = mix(dark, hex(INK), 0.12);
                    mb.lathe(Mat4::from_rotation_y(k as f32 * 0.7), &prof, seg + 1, under, mix(top, light, 0.15 * k as f32), true);
                }
            }
            Species::Willow => {
                root_flare(&mut mb, 0.28, trunk);
                mb.cyl(v(0.0, -0.1, 0.0), v(0.12, 1.3, 0.0), 0.28, 0.2, trunk, trunk, 7);
                mb.cyl(v(0.12, 1.2, 0.0), v(0.0, 2.3, 0.06), 0.2, 0.14, trunk, trunk, 7);
                if bare {
                    twigs(&mut mb, v(0.0, 2.3, 0.0), 9, 1.2, 1.3, hex(0x7d6a44), 2);
                    for i in 0..9 {
                        let a = i as f32 / 9.0 * TAU;
                        let p = v(a.cos() * 1.5, 3.4, a.sin() * 1.5);
                        mb.cyl(p, p + v(0.0, -1.9, 0.0), 0.04, 0.015, light, light, 4);
                    }
                } else {
                    crown(&mut mb, v(0.0, 3.1, 0.0), v(1.55, 0.95, 1.55), 10, 0.6, dark, light, 9);
                    mb.lathe(Mat4::IDENTITY, &[(1.7, 0.6), (2.05, 1.2), (1.95, 2.2), (1.45, 3.0)], 14, mix(dark, light, 0.2), light, true);
                    let mut r = Rng(19);
                    for i in 0..14 {
                        let a = i as f32 / 14.0 * TAU + r.range(-0.1, 0.1);
                        let (rr, y) = (r.range(1.75, 2.0), r.range(0.45, 0.8));
                        mb.ellipsoid_g(v(a.cos() * rr, y, a.sin() * rr), v(0.13, 0.32, 0.13), mix(dark, light, 0.15), mix(dark, light, 0.5), 5, 3);
                    }
                }
            }
        },
        1 => {
            let tw = if sp == Species::Aspen { 0.12 } else { 0.2 };
            mb.cyl(v(0.0, -0.1, 0.0), v(0.0, top_h * 0.55, 0.0), tw * 1.3, tw * 0.8, trunk, trunk, 5);
            if bare {
                mb.ellipsoid(v(0.0, top_h * 0.62, 0.0), v(0.7, 0.9, 0.7), mix(trunk, hex(0x8a7a60), 0.5));
            } else if sp == Species::Pine {
                let top = if season == Season::Winter { hex(0xeef6ff) } else { light };
                for (y0, rr, h) in [(1.0f32, 1.5f32, 1.9f32), (2.4, 1.05, 1.6), (3.6, 0.6, 1.2)] {
                    mb.lathe(Mat4::IDENTITY, &[(0.0, y0 + 0.2), (rr, y0), (rr * 0.6, y0 + h * 0.5), (0.0, y0 + h)], seg, dark, top, true);
                }
            } else {
                let (rx, ry, cy) = match sp {
                    Species::Aspen => (1.05, 1.6, 3.4),
                    Species::Maple => (1.9, 1.5, 3.1),
                    _ => (1.9, 1.3, 2.7),
                };
                mb.ellipsoid_g(v(0.0, cy, 0.0), v(rx, ry, rx), dark, light, seg, rings);
            }
        }
        _ => {
            if bare {
                mb.cyl(v(0.0, -0.1, 0.0), v(0.0, top_h * 0.7, 0.0), 0.25, 0.1, trunk, hex(0x8a7a60), 4);
            } else if sp == Species::Pine {
                let top = if season == Season::Winter { hex(0xeef6ff) } else { light };
                mb.lathe(Mat4::IDENTITY, &[(0.0, 0.8), (1.5, 1.8), (1.1, 3.4), (0.0, 4.9)], 6, dark, top, true);
            } else {
                mb.lathe(Mat4::IDENTITY, &[(0.0, 1.2), (1.5, 2.6), (1.1, 3.9), (0.0, 4.8)], 5, dark, light, false);
            }
        }
    }
    mb.build()
}

fn crown(mb: &mut Mb, c: Vec3, r: Vec3, n: usize, clump: f32, dark: [f32; 3], light: [f32; 3], seed: u32) {
    let mut rng = Rng(seed * 7919 + 13);
    mb.ellipsoid_g(c, r * 0.8, mix(dark, hex(INK), 0.08), mix(dark, light, 0.55), 9, 6);
    for i in 0..n {
        let y = 0.95 - (i as f32 + 0.5) / n as f32 * 1.45;
        let ring = (1.0 - y * y).max(0.0).sqrt();
        let a = i as f32 * 2.399_963 + rng.range(-0.35, 0.35);
        let p = c + v(a.cos() * ring * r.x, y * r.y, a.sin() * ring * r.z) * 0.8;
        let s = clump * rng.range(0.82, 1.15) * (1.0 - 0.15 * y.max(0.0));
        let up = (y * 0.5 + 0.5).clamp(0.0, 1.0);
        let j = rng.range(-0.12, 0.12);
        let lo = mix(dark, light, (0.05 + 0.3 * up + j).clamp(0.0, 1.0));
        let hi = mix(dark, light, (0.75 + 0.25 * up + j).clamp(0.0, 1.0));
        mb.ellipsoid_g(p, v(s, s * 0.82, s), lo, hi, 8, 5);
    }
}

fn root_flare(mb: &mut Mb, r: f32, bark: [f32; 3]) {
    mb.lathe(Mat4::IDENTITY, &[(0.0, -0.1), (r * 1.9, -0.1), (r * 1.25, 0.1), (r, 0.42)], 8, mix(bark, hex(INK), 0.25), bark, true);
}

fn bark_eyes(mb: &mut Mb, h: f32, r0: f32, r1: f32, n: usize, seed: u32) {
    let mut rng = Rng(seed);
    for _ in 0..n {
        let y = rng.range(0.35, h * 0.85);
        let a = rng.range(0.0, TAU);
        let rr = r0 + (r1 - r0) * (y / h) + 0.004;
        let rot = Quat::from_rotation_y(std::f32::consts::FRAC_PI_2 - a);
        mb.ellipsoid_r(v(a.cos() * rr, y, a.sin() * rr), v(0.075, 0.028, 0.018), rot, hex(0x4a4036));
    }
}

fn twigs(mb: &mut Mb, base: Vec3, n: usize, reach: f32, rise: f32, col: [f32; 3], twig_levels: u32) {
    let mut r = Rng(31 + n as u32);
    for i in 0..n {
        let a = i as f32 / n as f32 * TAU + r.range(-0.2, 0.2);
        let mid = base + v(a.cos() * reach * 0.45, rise * 0.55, a.sin() * reach * 0.45);
        let tip = base + v(a.cos() * reach, rise, a.sin() * reach);
        mb.cyl(base, mid, 0.09, 0.06, col, col, 5);
        mb.cyl(mid, tip, 0.06, 0.025, col, col, 5);
        if twig_levels > 1 {
            let a2 = a + 0.7;
            mb.cyl(mid, mid + v(a2.cos() * 0.6, 0.55, a2.sin() * 0.6), 0.04, 0.02, col, col, 4);
        }
    }
}

pub fn stump_mesh() -> Mesh {
    let mut mb = Mb::new();
    let bark = hex(0x7a5230);
    mb.disc(v(0.0, 0.04, 0.0), 1.0, 1.0, hex(0x2f6a2a), 9);
    mb.lathe(Mat4::IDENTITY, &[(0.0, -0.1), (0.62, -0.1), (0.44, 0.15), (0.38, 0.55)], 10, bark, bark, true);
    mb.lathe(Mat4::IDENTITY, &[(0.38, 0.55), (0.34, 0.57), (0.0, 0.57)], 10, hex(0xeed3a0), hex(0xf6e2b8), false);
    mb.lathe(Mat4::IDENTITY, &[(0.2, 0.575), (0.17, 0.575)], 10, hex(0xc9a066), hex(0xc9a066), false);
    for i in 0..4 {
        let a = i as f32 * 1.6 + 0.4;
        mb.ellipsoid_r(v(a.cos() * 0.6, 0.05, a.sin() * 0.6), v(0.3, 0.12, 0.14), Quat::from_rotation_y(-a), bark);
    }
    for (i, (x, z)) in [(0.8f32, 0.6f32), (-0.7, 0.8), (0.3, -0.9), (-0.9, -0.3)].iter().enumerate() {
        mb.ellipsoid_r(v(*x, 0.07, *z), v(0.14, 0.05, 0.08), Quat::from_rotation_y(i as f32), hex(0xe9cf9e));
    }
    mb.build()
}

pub fn snag_mesh() -> Mesh {
    let mut mb = Mb::new();
    let (g0, g1) = (hex(0x6e675d), hex(0x9a9283));
    mb.cyl(v(0.0, -0.4, 0.0), v(0.28, 3.4, 0.1), 0.24, 0.12, g0, g1, 7);
    mb.cyl(v(0.28, 3.3, 0.1), v(0.4, 3.9, 0.12), 0.12, 0.03, g1, hex(0xc8c0b0), 5);
    mb.cyl(v(0.15, 2.2, 0.05), v(1.0, 3.0, 0.3), 0.08, 0.03, g0, g1, 5);
    mb.cyl(v(0.2, 1.5, 0.0), v(-0.65, 2.2, 0.3), 0.07, 0.03, g0, g1, 5);
    mb.cyl(v(0.1, 2.9, 0.0), v(-0.4, 3.5, -0.2), 0.05, 0.02, g0, g1, 4);
    mb.ellipsoid_g(v(0.0, 0.1, 0.0), v(0.36, 0.25, 0.36), hex(0x4f6b2c), hex(0x7da83f), 8, 4);
    mb.build()
}

pub fn stage_of(progress: f32) -> u8 {
    if progress >= 1.0 { 3 } else { ((progress.max(0.0) * 3.0) as u8).min(2) }
}

pub(super) fn stake(mb: &mut Mb, p: Vec3, h: f32, flag: bool) {
    mb.cyl(p + v(0.0, -0.4, 0.0), p + v(0.0, h, 0.0), 0.06, 0.05, hex(0xf3e7c9), hex(0xf3e7c9), 5);
    mb.ellipsoid(p + v(0.0, h + 0.02, 0.0), v(0.08, 0.06, 0.08), hex(0xf3e7c9));
    if flag {
        mb.cuboid(p + v(0.22, h - 0.12, 0.0), v(0.2, 0.12, 0.015), Quat::from_rotation_z(-0.08), hex(0xff7a1a), hex(0xffa040));
    }
}

pub(super) fn site_markers(mb: &mut Mb, hx: f32, hz: f32, y: f32) {
    let corners = [v(-hx, y, -hz), v(hx, y, -hz), v(hx, y, hz), v(-hx, y, hz)];
    for (i, c) in corners.iter().enumerate() {
        stake(mb, *c, 1.25, true);
        let n = corners[(i + 1) % 4];
        let (a, b) = (*c + v(0.0, 0.95, 0.0), n + v(0.0, 0.95, 0.0));
        let segs = (((b - a).length() / 0.45) as usize).max(2);
        for k in 0..segs {
            let (t0, t1) = (k as f32 / segs as f32, (k + 1) as f32 / segs as f32);
            let col = if k % 2 == 0 { hex(0xff7a1a) } else { hex(0xfff3dc) };
            mb.cyl(a.lerp(b, t0), a.lerp(b, t1), 0.028, 0.028, col, col, 4);
        }
    }
}

pub(super) fn stick(mb: &mut Mb, a: Vec3, b: Vec3, r: f32, k: f32) {
    let bark = mix(hex(0x8b5f33), hex(0xb98a52), k);
    mb.log(a, b, r, bark, hex(0xe9cf9b), 5);
}

pub(super) fn stick_c(mb: &mut Mb, a: Vec3, b: Vec3, r: f32, col: [f32; 3]) {
    mb.log(a, b, r, col, hex(0xe9cf9b), 5);
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
    mb.ellipsoid_g(c, v(0.55 * s, 0.26 * s, 0.45 * s), hex(0x4d3520), hex(0x77573a), 8, 5);
}

pub const DRY_STAGE: u8 = 4;

pub fn footing_of(kind: StructKind) -> Option<structures::Footing> {
    use structures::Footing::*;
    Some(match kind {
        StructKind::Lodge => Mound(3.3),
        StructKind::Burrow => Mound(2.1),
        StructKind::FoodCache => Mound(1.65),
        StructKind::ScentMound => Mound(1.0),
        StructKind::MudPit => Mound(1.75),
        StructKind::ForagerHut => Piles(1.7, 1.7),
        StructKind::LoggingCamp => Piles(1.9, 1.9),
        StructKind::Workshop => Piles(2.1, 1.7),
        StructKind::Warehouse => Piles(2.9, 2.5),
        StructKind::Storehouse => Piles(3.8, 2.7),
        StructKind::Mill => Stone(1.9),
        _ => return None,
    })
}

pub fn struct_mesh(kind: StructKind, len: f32, stage: u8) -> Mesh {
    let mut mb = Mb::new();
    let dry = stage == DRY_STAGE;
    let stage = stage.min(3);
    match kind {
        StructKind::Lodge => lodge(&mut mb, stage),
        StructKind::Dam => dam(&mut mb, len, stage),
        StructKind::Canal => canal(&mut mb, len, stage),
        StructKind::Burrow => burrow(&mut mb, stage),
        StructKind::Workshop => workshop(&mut mb, stage),
        StructKind::WaterWheel => wheel_frame(&mut mb, stage),
        StructKind::Dock => dock(&mut mb, len, stage),
        StructKind::Barge => barge(&mut mb, len, stage, true),
        StructKind::BargeEmpty => barge(&mut mb, len, 3, false),
        StructKind::FoodCache => structures::food_cache(&mut mb, stage),
        StructKind::ScentMound => structures::scent_mound(&mut mb, stage),
        StructKind::GuardPost => structures::guard_post(&mut mb, stage),
        StructKind::ForagerHut => structures::forager_hut(&mut mb, stage),
        StructKind::MudPit => structures::mud_pit(&mut mb, stage),
        StructKind::TreeNursery => structures::tree_nursery(&mut mb, stage),
        StructKind::Warehouse => structures::warehouse(&mut mb, stage),
        StructKind::Storehouse => structures::storehouse(&mut mb, stage),
        StructKind::LoggingCamp => structures::logging_camp(&mut mb, stage),
        StructKind::QuarryCamp => structures::quarry_camp(&mut mb, stage),
        StructKind::ForesterLodge => structures::forester_lodge(&mut mb, stage),
        StructKind::Mill => structures::mill(&mut mb, stage),
        StructKind::Lock => structures::lock(&mut mb, stage),
    }
    if let Some(f) = footing_of(kind) {
        if dry {
            let (hx, hz) = match f {
                structures::Footing::Mound(r) | structures::Footing::Stone(r) => (r, r),
                structures::Footing::Piles(x, z) => (x, z),
            };
            structures::abandoned_dressing(&mut mb, hx, hz);
            mb.weather(structures::abandoned_tint(), 0.45);
        }
        structures::footing(&mut mb, f, dry);
        if dry {
            mb.weather(structures::abandoned_tint(), 0.12);
        }
    }
    mb.build()
}

fn lodge(mb: &mut Mb, stage: u8) {
    let (rad, hgt) = (3.3f32, 2.8f32);
    let f: f32 = [0.4, 0.7, 0.93, 1.0][stage as usize];
    let phi_top = if stage == 3 { PI / 2.0 } else { (f).asin() };
    let nb = if stage == 3 { 7 } else { (6.0 * f).ceil() as usize + 1 };
    let (c0, c1) = (hex(0x7c5530), hex(0x9a7040));
    for i in 0..nb {
        let (p0, p1) = (phi_top * i as f32 / nb as f32, phi_top * (i + 1) as f32 / nb as f32);
        let bulge = if i % 2 == 0 { 1.0 } else { 1.035 };
        let pm = (p0 + p1) * 0.5;
        let pt = |p: f32, k: f32| (rad * p.cos() * k, hgt * p.sin());
        let (a, b, c) = (pt(p0, 1.0), pt(pm, bulge), pt(p1, 1.0));
        let t = i as f32 / nb as f32;
        let (ca, cb) = if i % 2 == 0 { (mix(c0, c1, t), mix(c0, c1, t + 0.15)) } else { (mix(hex(0x6a4627), c0, t), mix(hex(0x8d6537), c1, t)) };
        mb.lathe(Mat4::IDENTITY, &[a, b, c], 18, ca, cb, true);
    }
    let mut r = Rng(1234 + stage as u32);
    let nst = (70.0 * f) as usize;
    for _ in 0..nst {
        let phi = r.range(0.04, (phi_top - 0.08).max(0.05));
        let th = r.range(0.0, TAU);
        let surf = v(rad * phi.cos() * th.cos(), hgt * phi.sin(), rad * phi.cos() * th.sin());
        if th.sin() > 0.3 && phi < 0.8 {
            continue;
        }
        let out = v(th.cos() * phi.cos(), phi.sin() * 1.1, th.sin() * phi.cos()).normalize();
        let tang = v(-th.sin(), r.range(-0.3, 0.5), th.cos());
        let dir = (out * r.range(0.25, 0.55) + tang * r.range(-1.0, 1.0)).normalize();
        let l = r.range(1.2, 2.2);
        let (rad, k) = (r.range(0.065, 0.1), r.next());
        if k < 0.15 {
            stick_c(mb, surf - dir * l * 0.55, surf + dir * l * 0.45, rad, mix(hex(0xf2d9a8), hex(0xd8c29a), 0.4));
        } else {
            stick(mb, surf - dir * l * 0.55, surf + dir * l * 0.45, rad, k);
        }
    }
    if stage == 3 {
        mb.ellipsoid_g(v(0.0, hgt * 0.93, 0.0), v(1.5, 0.55, 1.5), hex(0x5d4228), hex(0x80613d), 12, 6);
        for (x, y, z) in [(-1.7, 1.5, 1.0), (1.9, 1.2, -0.8), (-0.6, 2.3, -1.4), (1.1, 2.0, 1.3)] {
            mud_lump(mb, v(x, y, z), 1.0);
        }
        stick(mb, v(-0.3, 2.6, 0.0), v(0.5, 3.6, 0.2), 0.08, 0.3);
        stick(mb, v(0.4, 2.6, -0.1), v(-0.3, 3.55, 0.1), 0.08, 0.6);
        let mut m = Rng(4321);
        for _ in 0..7 {
            let (phi, th) = (m.range(0.25, 1.05), m.range(0.0, TAU));
            if th.sin() > 0.3 && phi < 0.8 {
                continue;
            }
            let p = v(rad * phi.cos() * th.cos(), hgt * phi.sin(), rad * phi.cos() * th.sin()) * 1.02;
            let s = m.range(0.35, 0.6);
            mb.ellipsoid_g(p, v(s, s * 0.35, s), hex(0x4e9a52), hex(0x85c468), 8, 4);
        }
    }
    if stage >= 2 {
        mb.ellipsoid_g(v(0.0, 0.85, 3.0), v(1.15, 1.0, 0.6), hex(0x4d3520), hex(0x6f5033), 12, 7);
        mb.ellipsoid(v(0.0, 0.7, 3.56), v(0.7, 0.72, 0.12), hex(0x140c09));
        mb.ellipsoid_g(v(0.0, 0.05, 3.85), v(1.25, 0.32, 1.05), hex(0x5d4228), hex(0x866640), 10, 5);
        stick_pile(mb, v(1.9, 0.0, 3.6), 0.3, 6, 5);
    }
    if stage < 3 {
        let nrib = 7;
        for i in 0..nrib {
            let th = i as f32 / nrib as f32 * TAU + 0.3;
            let mut prev = v(rad * phi_top.cos() * th.cos(), hgt * phi_top.sin(), rad * phi_top.cos() * th.sin());
            let steps = 4;
            for s in 1..=steps {
                let phi = phi_top + (PI / 2.0 - phi_top) * s as f32 / steps as f32;
                let p = v(rad * phi.cos() * th.cos() * 0.97, hgt * phi.sin(), rad * phi.cos() * th.sin() * 0.97);
                mb.cyl(prev, p, 0.065, 0.055, hex(0xd7b27a), hex(0xd7b27a), 5);
                prev = p;
            }
        }
        for s in 0..3 {
            let phi = phi_top + (PI / 2.0 - phi_top) * (s as f32 + 0.6) / 3.2;
            let (rr, yy) = (rad * phi.cos() * 0.97, hgt * phi.sin());
            let n = 14;
            for k in 0..n {
                let (a, b) = (k as f32 / n as f32 * TAU, (k + 1) as f32 / n as f32 * TAU);
                mb.cyl(v(rr * a.cos(), yy, rr * a.sin()), v(rr * b.cos(), yy, rr * b.sin()), 0.04, 0.04, hex(0xcaa56d), hex(0xcaa56d), 4);
            }
        }
        mb.lathe(Mat4::from_translation(v(0.0, hgt * phi_top.sin() - 0.02, 0.0)), &[(rad * phi_top.cos() * 0.95, 0.0), (0.0, 0.0)], 18, hex(0x2a1b11), hex(0x2a1b11), false);
        site_markers(mb, rad + 1.3, rad + 1.3, 0.0);
        stick_pile(mb, v(-3.2, 0.0, 3.9), 0.8, 9, 17);
        mud_lump(mb, v(3.9, 0.15, 2.9), 1.4);
        mud_lump(mb, v(-4.2, 0.1, -1.2), 1.1);
    }
}

fn dam(mb: &mut Mb, len: f32, stage: u8) {
    if stage == 3 {
        structures::dam_cell(mb, 1.4, len, 0, false);
    } else {
        structures::dam_cell(mb, [0.25, 0.6, 1.0][stage as usize], len, 1, true);
    }
}

fn canal(mb: &mut Mb, len: f32, stage: u8) {
    let hl = len * 0.5;
    let n = ((len / 1.0).ceil() as i32).max(2);
    for s in [-1.0f32, 1.0] {
        for i in 0..=n {
            let x = -hl + len * i as f32 / n as f32;
            mb.ellipsoid_g(v(x, 0.1, 1.35 * s), v(0.85, 0.5, 0.75), hex(0x5c4128), hex(0x7aa84a), 8, 5);
        }
    }
    if stage == 3 {
        mb.cuboid(v(0.0, 0.0, 0.0), v(hl + 0.05, 0.14, 1.0), Quat::IDENTITY, hex(0x2f7fb0), hex(0x58b8e6));
        for k in 0..3 {
            mb.ellipsoid(v(-hl * 0.6 + k as f32 * hl * 0.6, 0.15, 0.2 * (k as f32 - 1.0)), v(0.35, 0.02, 0.12), hex(0xd8f1ff));
        }
    } else {
        mb.cuboid(v(0.0, -0.05, 0.0), v(hl + 0.05, 0.1, 1.0), Quat::IDENTITY, hex(0x3d2a1a), hex(0x6a4a2e));
        mud_lump(mb, v(-hl * 0.4, 0.35, 2.4), 1.3);
        mb.cyl(v(0.7, 0.0, 2.0), v(1.0, 1.4, 2.1), 0.04, 0.04, hex(0xb98a52), hex(0xb98a52), 5);
        mb.cuboid(v(0.68, 0.1, 1.98), v(0.12, 0.17, 0.02), Quat::from_rotation_z(0.2), hex(0x9aa0a8), hex(0xb8bec6));
        stake(mb, v(hl - 0.3, 0.0, -1.9), 1.2, true);
        stake(mb, v(hl - 0.3, 0.0, 1.9), 1.2, true);
    }
}

fn burrow(mb: &mut Mb, stage: u8) {
    let _ = stage;
    mb.lathe(
        Mat4::IDENTITY,
        &[(2.1, -0.9), (2.05, 0.1), (1.7, 0.6), (1.1, 0.98), (0.0, 1.15)],
        14,
        hex(0x6a4a2e),
        hex(0x7cba58),
        true,
    );
    mb.ellipsoid_g(v(0.0, 0.5, 1.62), v(0.85, 0.75, 0.5), hex(0x4d3520), hex(0x6f5033), 10, 6);
    mb.ellipsoid(v(0.0, 0.42, 1.95), v(0.58, 0.6, 0.25), hex(0x140c09));
    mb.ellipsoid_g(v(0.0, 0.06, 2.35), v(0.9, 0.2, 0.7), hex(0x5d4228), hex(0x866640), 8, 4);
    stick(mb, v(1.0, 0.1, 1.9), v(2.0, 0.2, 2.5), 0.07, 0.4);
    stick(mb, v(1.1, 0.1, 2.1), v(2.1, 0.3, 1.5), 0.07, 0.7);
    for (x, z, s) in [(-1.6, 1.0, 0.3), (1.4, -1.2, 0.25), (-0.9, -1.6, 0.2)] {
        mb.ellipsoid_g(v(x, 0.1, z), v(s, s * 0.7, s), hex(0x6f6c68), hex(0xa7a39c), 7, 4);
    }
    for (x, z, c) in [(-0.7, 0.6, 0xffe066), (0.5, -0.5, 0xff9ec4), (0.9, 0.4, 0xffffff)] {
        mb.ellipsoid(v(x, 1.05, z), v(0.09, 0.07, 0.09), hex(c));
    }
}

fn workshop(mb: &mut Mb, stage: u8) {
    let (hx, hz) = (1.9f32, 1.55f32);
    mb.cuboid(v(0.0, 0.0, 0.0), v(hx + 0.12, 0.3, hz + 0.12), Quat::IDENTITY, hex(0x6f6b66), hex(0x9a968f));
    let wall_h = [0.0, 0.9, 1.75, 1.75][stage as usize];
    let post = |mb: &mut Mb, x: f32, z: f32, h: f32| mb.cyl(v(x, 0.2, z), v(x, h, z), 0.12, 0.1, hex(0x8a5a32), hex(0xb07840), 6);
    if stage >= 1 {
        mb.cuboid(v(0.0, 0.3 + wall_h * 0.5, 0.0), v(hx, wall_h * 0.5, hz), Quat::IDENTITY, hex(0xb9814a), hex(0xd49a5c));
        for i in 1..6 {
            let x = -hx + i as f32 * hx * 2.0 / 6.0;
            mb.cuboid(v(x, 0.3 + wall_h * 0.5, hz + 0.01), v(0.02, wall_h * 0.5, 0.01), Quat::IDENTITY, hex(0x7d5430), hex(0x8f6038));
        }
    }
    for (x, z) in [(-hx, -hz), (hx, -hz), (hx, hz), (-hx, hz)] {
        post(mb, x, z, if stage == 0 { 1.4 } else { wall_h + 0.4 });
    }
    if stage == 3 {
        let eaves = 2.05;
        hex_roof(mb, hx + 0.35, hz + 0.4, eaves);
        mb.cuboid(v(1.2, 2.9, -0.7), v(0.26, 0.55, 0.26), Quat::IDENTITY, hex(0x7b7670), hex(0xa09a92));
        mb.ellipsoid(v(1.25, 3.7, -0.7), v(0.32, 0.26, 0.32), hex(0xe9edf0));
        mb.ellipsoid(v(1.45, 4.1, -0.6), v(0.26, 0.22, 0.26), hex(0xf4f7f9));
        mb.cuboid(v(0.0, 0.3 + 0.65, hz + 0.02), v(0.46, 0.65, 0.07), Quat::IDENTITY, hex(0x6a4020), hex(0x8a5a30));
        mb.ellipsoid(v(0.3, 0.95, hz + 0.1), v(0.05, 0.05, 0.04), hex(0xf4d35e));
        for s in [-1.0, 1.0] {
            mb.cuboid(v(1.15 * s, 1.35, hz + 0.02), v(0.34, 0.3, 0.06), Quat::IDENTITY, hex(0x6a4020), hex(0x8a5a30));
            mb.cuboid(v(1.15 * s, 1.35, hz + 0.05), v(0.26, 0.22, 0.05), Quat::IDENTITY, hex(0xa8dcf0), hex(0xd6f2ff));
        }
        mb.cuboid(v(-2.6, 0.45, 0.3), v(0.5, 0.06, 0.9), Quat::IDENTITY, hex(0xb98a52), hex(0xd1a468));
        for z in [-0.4, 1.0] {
            mb.cuboid(v(-2.6, 0.22, z), v(0.38, 0.22, 0.06), Quat::IDENTITY, hex(0x7d5430), hex(0x8f6038));
        }
        stick(mb, v(-2.6, 0.62, -0.4), v(-2.6, 0.62, 1.0), 0.14, 0.3);
    } else if stage == 2 {
        for i in 0..5 {
            let x = -hx + i as f32 * hx * 0.5;
            mb.cyl(v(x, 2.0, -hz), v(x, 2.9, 0.0), 0.06, 0.05, hex(0xd7b27a), hex(0xd7b27a), 4);
            mb.cyl(v(x, 2.0, hz), v(x, 2.9, 0.0), 0.06, 0.05, hex(0xd7b27a), hex(0xd7b27a), 4);
        }
        mb.cyl(v(-hx, 2.9, 0.0), v(hx, 2.9, 0.0), 0.07, 0.07, hex(0xd7b27a), hex(0xd7b27a), 5);
    }
    let mut k = 0;
    for (layer, cnt) in [(0usize, 4usize), (1, 3), (2, 2)] {
        for i in 0..cnt {
            let x = 2.45 + layer as f32 * 0.15 + i as f32 * 0.0;
            let z = -1.0 + i as f32 * 0.33 + layer as f32 * 0.17;
            let y = 0.14 + layer as f32 * 0.24;
            k += 1;
            mb.log(v(x - 0.65, y, z), v(x + 0.65, y, z), 0.15, mix(hex(0x7a5230), hex(0x9a6e3e), (k % 3) as f32 * 0.4), hex(0xecd2a0), 7);
        }
    }
    if stage < 3 {
        site_markers(mb, hx + 1.8, hz + 1.4, 0.0);
        for i in 0..4 {
            mb.cuboid(v(-2.8, 0.12 + i as f32 * 0.09, 1.9), v(1.0, 0.045, 0.2), Quat::from_rotation_y(0.15 * i as f32), hex(0xcda56a), hex(0xe2bb7c));
        }
    }
}

fn hex_roof(mb: &mut Mb, hx: f32, hz: f32, eaves: f32) { hex_roof_c(mb, hx, hz, eaves, hex(0xa63f28), hex(0xd75c38), hex(0x8f3320)) }

pub(super) fn hex_roof_c(mb: &mut Mb, hx: f32, hz: f32, eaves: f32, c0: [f32; 3], c1: [f32; 3], cap: [f32; 3]) {
    let ridge = eaves + 0.95;
    mb.hex(
        [
            v(-hx, eaves, -hz), v(hx, eaves, -hz), v(hx, eaves, hz), v(-hx, eaves, hz),
            v(-hx, ridge, -0.04), v(hx, ridge, -0.04), v(hx, ridge, 0.04), v(-hx, ridge, 0.04),
        ],
        c0,
        c1,
    );
    mb.cyl(v(-hx, ridge + 0.02, 0.0), v(hx, ridge + 0.02, 0.0), 0.12, 0.12, cap, cap, 6);
    for k in 1..4 {
        let t = k as f32 / 4.0;
        for s in [-1.0f32, 1.0] {
            let (y, z) = (eaves + (ridge - eaves) * t + 0.015, s * hz * (1.0 - t));
            mb.cyl(v(-hx, y, z), v(hx, y, z), 0.035, 0.035, mix(c0, cap, 0.6), mix(c0, cap, 0.6), 4);
        }
    }
    mb.cuboid(v(0.0, eaves - 0.04, 0.0), v(hx - 0.1, 0.05, hz - 0.1), Quat::IDENTITY, hex(0x5a3a20), hex(0x6a4528));
}

fn wheel_frame(mb: &mut Mb, stage: u8) {
    let wood = (hex(0x8a5a32), hex(0xb07840));
    mb.cuboid(v(0.0, -0.1, 0.4), v(1.7, 0.3, 2.2), Quat::IDENTITY, hex(0x6f6b66), hex(0x9a968f));
    for s in [-1.0f32, 1.0] {
        mb.cyl(v(1.15 * s, -0.1, -1.1), v(1.15 * s, 2.05, 0.0), 0.12, 0.1, wood.0, wood.1, 6);
        mb.cyl(v(1.15 * s, -0.1, 1.1), v(1.15 * s, 2.05, 0.0), 0.12, 0.1, wood.0, wood.1, 6);
        mb.ellipsoid(v(1.15 * s, 2.05, 0.0), v(0.18, 0.18, 0.18), hex(0x555a60));
    }
    mb.cuboid(v(0.0, 2.75, -1.9), v(0.55, 0.14, 1.5), Quat::IDENTITY, hex(0x8a5a32), hex(0xb07840));
    if stage == 3 {
        mb.cuboid(v(0.0, 2.9, -1.9), v(0.42, 0.05, 1.5), Quat::IDENTITY, hex(0x3f8fc2), hex(0x6cc7f0));
    }
    for z in [-2.9, -1.0] {
        mb.cyl(v(0.0, -0.2, z), v(0.0, 2.6, z), 0.1, 0.1, wood.0, wood.1, 6);
    }
    if stage < 3 {
        site_markers(mb, 2.2, 2.3, 0.0);
        stick_pile(mb, v(2.4, 0.0, 0.6), 1.2, 6, 41);
    }
}

pub fn wheel_mesh() -> Mesh {
    let mut mb = Mb::new();
    let r = 1.75;
    let wood = (hex(0x93603a), hex(0xbd8450));
    mb.cyl(v(-1.15, 0.0, 0.0), v(1.15, 0.0, 0.0), 0.16, 0.16, hex(0x6a4528), hex(0x8a5a32), 8);
    mb.ellipsoid(v(0.0, 0.0, 0.0), v(0.4, 0.34, 0.34), hex(0x555a60));
    let n = 10;
    for i in 0..n {
        let t = i as f32 / n as f32 * TAU;
        let q = Quat::from_rotation_x(t);
        for s in [-0.8f32, 0.8] {
            mb.cuboid(q * v(s, r * 0.5, 0.0), v(0.07, r * 0.5, 0.07), q, wood.0, wood.1);
        }
        mb.cuboid(q * v(0.0, r + 0.02, 0.0), v(0.85, 0.33, 0.07), q, hex(0xa0703f), hex(0xc99a5e));
        let t2 = t + TAU / n as f32 * 0.5;
        let q2 = Quat::from_rotation_x(t2);
        for s in [-0.8f32, 0.8] {
            mb.cuboid(q2 * v(s, r - 0.1, 0.0), v(0.09, 0.09, TAU * r / n as f32 * 0.55), q2, hex(0x7a4f2c), hex(0x9b6a3c));
        }
    }
    mb.build()
}

fn dock(mb: &mut Mb, len: f32, stage: u8) {
    let (w, y) = (1.15f32, 0.5f32);
    let planks = (len / 0.5) as i32;
    let shown = if stage == 3 { planks } else { (planks * (stage as i32 + 1)) / 4 };
    for i in 0..shown {
        let z = 0.25 + i as f32 * 0.5;
        let (a, b) = if i % 2 == 0 { (hex(0xb98650), hex(0xd1a266)) } else { (hex(0xa97a46), hex(0xc4965c)) };
        mb.cuboid(v(0.0, y, z), v(w, 0.07, 0.23), Quat::IDENTITY, a, b);
    }
    for s in [-0.7f32, 0.7] {
        mb.cuboid(v(s, y - 0.13, len * 0.5), v(0.1, 0.07, len * 0.5), Quat::IDENTITY, hex(0x6a4528), hex(0x7d5430));
    }
    let mut z = 0.5;
    while z <= len + 0.01 {
        for s in [-1.0f32, 1.0] {
            mb.cyl(v((w + 0.1) * s, -1.3, z), v((w + 0.1) * s, y + 0.55, z), 0.13, 0.13, hex(0x5a3a22), hex(0x7d5430), 7);
            mb.ellipsoid(v((w + 0.1) * s, y + 0.56, z), v(0.14, 0.09, 0.14), hex(0x8f6038));
        }
        z += (len - 0.5).max(1.0);
    }
    if stage == 3 {
        mb.cyl(v(w + 0.1, y + 0.4, 0.5), v(w + 0.1, y + 0.4, len), 0.025, 0.025, hex(0xe9d9aa), hex(0xe9d9aa), 4);
        mb.cyl(v(-w - 0.1, y + 0.4, len), v(-w - 0.1, y + 0.9, len), 0.03, 0.03, hex(0x333333), hex(0x333333), 4);
        mb.ellipsoid(v(-w - 0.1, y + 1.05, len), v(0.13, 0.17, 0.13), hex(0xffe08a));
        mb.cuboid(v(-0.45, y + 0.35, len * 0.5), v(0.32, 0.28, 0.32), Quat::from_rotation_y(0.3), hex(0xa97a46), hex(0xd1a266));
        mb.lathe(Mat4::from_translation(v(0.55, y + 0.07, len * 0.38)), &[(0.0, 0.0), (0.28, 0.0), (0.34, 0.3), (0.3, 0.6), (0.0, 0.6)], 10, hex(0x7d5430), hex(0xa97a46), true);
    } else {
        stake(mb, v(w + 0.1, y, len), 1.1, true);
        stake(mb, v(-w - 0.1, y, len), 1.1, true);
        for i in 0..3 {
            mb.cuboid(v(-1.9, 0.12 + i as f32 * 0.09, 1.2), v(0.2, 0.045, 0.8), Quat::from_rotation_y(0.1 * i as f32), hex(0xcda56a), hex(0xe2bb7c));
        }
    }
}

fn barge(mb: &mut Mb, len: f32, stage: u8, cargo: bool) {
    let hl = len * 0.5;
    let top = if stage == 3 { 0.7 } else { 0.35 };
    mb.hex(
        [
            v(-0.95, -0.3, -hl), v(0.95, -0.3, -hl), v(0.5, -0.3, hl + 0.55), v(-0.5, -0.3, hl + 0.55),
            v(-1.3, top, -hl), v(1.3, top, -hl), v(0.78, top + 0.12, hl + 0.8), v(-0.78, top + 0.12, hl + 0.8),
        ],
        hex(0x6a4528),
        hex(0xb98650),
    );
    for s in [-1.0f32, 1.0] {
        mb.cyl(v(1.3 * s, top, -hl), v(0.78 * s, top + 0.12, hl + 0.8), 0.07, 0.07, hex(0x6a4528), hex(0x6a4528), 5);
    }
    mb.ellipsoid(v(0.0, 0.1, hl + 0.78), v(0.25, 0.22, 0.2), hex(0x2c2a28));
    if stage == 3 {
        for (row, cnt, y) in [(0, 3, 0.95), (1, 2, 1.32), (2, 1, 1.68)].into_iter().filter(|_| cargo) {
            for i in 0..cnt {
                let x = (i as f32 - (cnt as f32 - 1.0) * 0.5) * 0.62;
                let _ = row;
                mb.log(v(x, y, -hl + 0.7), v(x, y, hl - 0.4), 0.28, mix(hex(0x7a5230), hex(0x9a6e3e), 0.3 * i as f32), hex(0xecd2a0), 8);
            }
        }
        mb.cyl(v(0.0, 0.7, hl - 0.2), v(0.0, 2.6, hl - 0.2), 0.07, 0.05, hex(0x6a4528), hex(0x8f6038), 6);
        mb.cuboid(v(0.34, 2.35, hl - 0.2), v(0.34, 0.2, 0.02), Quat::from_rotation_z(-0.1), hex(0xd94a3a), hex(0xff7860));
    } else {
        for i in 0..5 {
            let z = -hl + 0.8 + i as f32 * (len - 1.5) / 4.0;
            mb.cyl(v(-1.1, top, z), v(-0.9, top + 1.2, z), 0.05, 0.04, hex(0xd7b27a), hex(0xd7b27a), 4);
            mb.cyl(v(1.1, top, z), v(0.9, top + 1.2, z), 0.05, 0.04, hex(0xd7b27a), hex(0xd7b27a), 4);
        }
    }
}

pub fn load_mesh(good: sim::buildings::Good) -> Mesh {
    use sim::buildings::Good;
    let mut mb = Mb::new();
    match good {
        Good::Logs => beaver_log(&mut mb),
        Good::Sticks => {
            for (i, dy) in [-0.02f32, 0.05, 0.12].iter().enumerate() {
                let k = i as f32 - 1.0;
                stick(&mut mb, v(-0.55, -0.24 + dy, 0.66 + k * 0.05), v(0.55, -0.2 + dy, 0.66 - k * 0.07), 0.035, 0.3 + 0.3 * i as f32);
            }
            mb.ellipsoid(v(0.5, -0.12, 0.62), v(0.1, 0.03, 0.06), hex(0x76c24a));
            mb.ellipsoid(v(-0.5, -0.12, 0.7), v(0.1, 0.03, 0.06), hex(0x5fae3c));
        }
        Good::Mud => {
            mb.ellipsoid_g(v(0.0, -0.3, 0.74), v(0.26, 0.2, 0.22), hex(0x4d3520), hex(0x80603f), 8, 5);
            mb.ellipsoid(v(0.09, -0.16, 0.74), v(0.1, 0.07, 0.09), hex(0x5d4228));
        }
        Good::Stone => {
            mb.ellipsoid_g(v(0.0, -0.3, 0.74), v(0.24, 0.19, 0.22), hex(0x77777c), hex(0xb2b2b6), 7, 4);
            mb.ellipsoid(v(0.1, -0.2, 0.72), v(0.1, 0.07, 0.09), hex(0xc7c7cb));
        }
        Good::Food => {
            mb.ellipsoid_g(v(0.0, -0.28, 0.72), v(0.3, 0.14, 0.2), hex(0x3f8f35), hex(0x7bc653), 8, 4);
            mb.ellipsoid(v(0.13, -0.18, 0.7), v(0.14, 0.05, 0.1), hex(0x9bd86a));
            mb.ellipsoid(v(-0.14, -0.2, 0.74), v(0.12, 0.05, 0.09), hex(0x5fae3c));
        }
        _ => {
            mb.ellipsoid_g(v(0.0, -0.28, 0.72), v(0.28, 0.17, 0.22), hex(0xa57c45), hex(0xd2a869), 8, 4);
        }
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

pub fn mud_mesh() -> Mesh {
    let mut mb = Mb::new();
    mb.ellipsoid_g(v(0.0, 0.0, 0.0), v(1.5, 0.55, 1.3), hex(0x3f2c1b), hex(0x7a5a3b), 12, 6);
    mb.ellipsoid_g(v(0.2, 0.28, 0.1), v(0.9, 0.3, 0.8), hex(0x5a4129), hex(0x93714b), 10, 5);
    for (x, z, s) in [(1.3, 0.5, 0.35), (-1.2, -0.6, 0.3), (0.4, -1.25, 0.28), (-0.5, 1.2, 0.25)] {
        mb.ellipsoid(v(x, 0.05, z), v(s, s * 0.45, s * 0.8), hex(0x5a4129));
    }
    mb.build()
}

pub fn stone_mesh() -> Mesh {
    let mut mb = Mb::new();
    let rocks = [(0.0, 0.0, 0.0, 1.1, 0.9, 0.9, 0.3), (1.2, 0.5, 0.0, 0.7, 0.55, 0.65, 1.1), (-0.9, 0.6, 0.0, 0.6, 0.5, 0.5, 2.0), (0.2, -1.0, 0.0, 0.75, 0.6, 0.6, 0.7)];
    for (x, z, y, rx, ry, rz, yaw) in rocks {
        mb.ellipsoid_g(v(x, y + ry * 0.55, z), v(rx, ry, rz), hex(0x6c6c72), hex(0xb7b5b2), 7, 4);
        mb.ellipsoid_r(v(x + 0.1, y + ry * 1.0, z), v(rx * 0.55, ry * 0.3, rz * 0.55), Quat::from_rotation_y(yaw), hex(0xc9c7c2));
    }
    mb.build()
}

pub fn plant_mesh() -> Mesh {
    let mut mb = Mb::new();
    for i in 0..6 {
        let a = i as f32 * 1.05;
        let tip = v(a.cos() * 0.45, 0.9 + 0.1 * (i % 3) as f32, a.sin() * 0.45);
        mb.cyl(v(a.cos() * 0.08, 0.0, a.sin() * 0.08), tip, 0.06, 0.02, hex(0x3f8f35), hex(0x8bd05a), 4);
    }
    mb.ellipsoid(v(0.0, 0.8, 0.0), v(0.3, 0.1, 0.3), hex(0x9bd86a));
    mb.build()
}
