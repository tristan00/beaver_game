use super::meshgen::{hex, mix, Mb, Rng};
use super::structures;
use super::scene::{Species, StructKind};
use bevy::prelude::*;
use std::f32::consts::TAU;

fn v(x: f32, y: f32, z: f32) -> Vec3 { Vec3::new(x, y, z) }

pub const HEAD_PIVOT: Vec3 = Vec3::new(0.0, 0.85, 0.62);
pub const TAIL_PIVOT: Vec3 = Vec3::new(0.0, 0.36, -0.6);
pub const LEG_PIVOTS: [Vec3; 4] = [Vec3::new(-0.3, 0.34, 0.4), Vec3::new(0.3, 0.34, 0.4), Vec3::new(-0.3, 0.36, -0.28), Vec3::new(0.3, 0.36, -0.28)];
/// Where a load hugged in the forepaws sits, in body space (moved by the pose).
pub const CHEST_HOLD: Vec3 = Vec3::new(0.0, 0.45, 0.85);
/// Where a load carried in the teeth sits, in head space.
pub const MOUTH: Vec3 = Vec3::new(0.0, -0.28, 0.72);

use crate::palette::{self, pack};
const FUR: u32 = pack(palette::FUR);
const FUR_DARK: u32 = pack(palette::FUR_DARK);
const FUR_LIGHT: u32 = pack(palette::FUR_LIGHT);
const INK: u32 = pack(palette::INK);

fn skin() -> [f32; 3] { mix(hex(FUR_DARK), hex(INK), 0.7) }
fn skin_light() -> [f32; 3] { mix(hex(FUR_DARK), hex(INK), 0.35) }

/// Fur shaded by which way it faces: a dark back (what the camera mostly sees, so a beaver
/// reads against grass and water), warm sides, a light belly. One rule for every fur piece,
/// so the overlapping shapes don't show as bands.
fn fur_paint(n: Vec3, old: [f32; 3]) -> [f32; 3] {
    let back = mix(hex(FUR_DARK), hex(INK), 0.6);
    let (side, belly) = (hex(FUR), hex(FUR_LIGHT));
    let up = n.y;
    let c = if up > 0.0 { mix(side, back, smooth01(0.1, 0.8, up)) } else { mix(side, belly, smooth01(0.05, 0.6, -up)) };
    mix(old, c, 0.85)
}

fn smooth01(a: f32, b: f32, x: f32) -> f32 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

pub fn beaver_body(mb: &mut Mb, o: Vec3) { beaver_body_d(mb, o, true) }

fn beaver_body_d(mb: &mut Mb, o: Vec3, detail: bool) {
    let (fd, f, fl) = (hex(FUR_DARK), hex(FUR), hex(FUR_LIGHT));
    let start = mb.len();
    mb.ellipsoid_g(o + v(0.0, 0.57, -0.14), v(0.54, 0.5, 0.62), fd, f, 14, 9);
    mb.ellipsoid_g(o + v(0.0, 0.6, 0.3), v(0.43, 0.43, 0.46), fd, f, 12, 8);
    mb.ellipsoid_g(o + v(0.0, 0.43, 0.16), v(0.37, 0.29, 0.55), mix(f, fl, 0.55), fl, 10, 6);
    for s in [-1.0, 1.0] {
        // Haunches stay inside the body's outline, not bulging out like saddlebags.
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
    // The crown darkens like the back; the face stays light.
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
        // Short incisors set under the lip, the warm orange of real beaver enamel.
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

/// Grizzled grey fur on an old beaver's muzzle and brows, laid over the head.
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

/// The paddle is broad and dark: from above it is what tells a beaver from any other lump.
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

/// A V of foam trailing back from the bow (at the origin) with a small bow wave in front.
/// Its length is scaled with speed; it is never a closed ring, so it can't be taken for
/// the selection marker.
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
    // Shared meshes sampled 24 times a year; individual trees stagger their changes.
    pub const COUNT: usize = 24;
    pub fn all() -> impl Iterator<Item = Self> { (0..Self::COUNT as u8).map(Self) }
    pub fn at(phase: f32) -> Self { Self(((phase.rem_euclid(1.0) * Self::COUNT as f32).round() as usize % Self::COUNT) as u8) }
    pub fn index(self) -> usize { self.0 as usize }
    fn phase(self) -> f32 { self.0 as f32 / Self::COUNT as f32 }
}

const FOLIAGE_SATURATION: f32 = 1.22;

fn foliage(sp: Species, s: Season) -> ([f32; 3], [f32; 3]) {
    let keys = match sp {
        Species::Aspen => [(0x77A45B,0xB1CC7B),(0x568347,0x9DB96B),(0xA98E49,0xD1B56C),(0x917A51,0xB7A377)],
        Species::Maple => [(0x679856,0xA2C276),(0x4F8055,0x87A965),(0x98634C,0xC59869),(0x775D48,0xAC8D67)],
        Species::Pine => [(0x5B8C61,0x95B678),(0x4C7D55,0x88AA6B),(0x567954,0x90A372),(0x4E715A,0x90AB8C)],
        Species::Willow => [(0x7CAA62,0xB5CF83),(0x628E52,0x9AB86E),(0x9A9552,0xC2BA74),(0x8E8257,0xB6A976)],
    };
    let dark = keys.map(|(a,_)| crate::palette::rgb(a));
    let light = keys.map(|(_,b)| crate::palette::rgb(b));
    // Foliage a little richer than the palette keys: under the strong ambient light, crowns at
    // the keys' saturation read as a pale olive haze.
    let richer = |c: [f32; 3]| { let g = (c[0] + c[1] + c[2]) / 3.0; c.map(|v| (g + (v - g) * FOLIAGE_SATURATION).clamp(0.0, 1.0)) };
    let colour = |k: &[_; 4]| crate::palette::display_linear(richer(crate::seasonal::blend(k,s.phase(),crate::palette::mix)));
    (colour(&dark), colour(&light))
}

fn trunk_color(sp: Species) -> [f32; 3] {
    hex(match sp {
        Species::Aspen => 0xe6dccb,
        Species::Maple => 0x7d5632,
        Species::Pine => 0x6b4630,
        Species::Willow => 0x6c5a3e,
    })
}

pub fn tree_cutaway_mesh(sp: Species) -> Mesh {
    let mut mb = Mb::new();
    // Dark, so a cut-away reads as a stump in shade rather than a pale shape on the grass.
    let trunk = mix(trunk_color(sp), hex(0x3e3024), 0.8);
    root_flare(&mut mb, 0.2, trunk);
    mb.cyl(v(0.0,-0.1,0.0),v(0.0,0.55,0.0),0.2,0.1,trunk,trunk,7);
    for i in 0..3 {
        let angle = i as f32 * TAU / 3.0;
        mb.cyl(v(0.0,0.35,0.0),v(angle.cos()*0.3,0.5,angle.sin()*0.3),0.07,0.025,trunk,trunk,5);
    }
    mb.build()
}

pub fn tree_mesh(sp: Species, season: Season, lod: u8) -> Mesh {
    let mut mb = Mb::new();
    let (dark, light) = foliage(sp, season);
    let trunk = trunk_color(sp);
    // Ground contact comes from the scene shadow map. An opaque horizontal disc
    // attached to the trunk floats over sloping banks and covers the river below.
    let leaves = crate::seasonal::leaves(season.phase());
    let leaf_scale = leaves.sqrt();
    let bare = leaves < 0.035 && sp != Species::Pine;
    let snow = crate::seasonal::snow(season.phase());
    let pine_top = mix(light, hex(0xCBDDE0), snow * 0.45);
    let seg = match lod { 0 => 9, 1 => 6, _ => 5 };
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
                canopy(&mut mb, canopy_of(sp), leaves, hex(0x9B8469), dark, light, true);
            }
            Species::Maple => {
                mb.lathe(Mat4::IDENTITY, &[(0.0, -0.1), (0.55, -0.1), (0.42, 0.08), (0.3, 0.45), (0.22, 1.6), (0.0, 1.6)], 9, mix(trunk, hex(INK), 0.2), trunk, false);
                let c = canopy_of(sp);
                let tips = canopy(&mut mb, c, leaves, hex(0x6b4a30), dark, light, true);
                let bloom = crate::seasonal::bloom(season.phase());
                if bloom > 0.1 {
                    // Blossom flecks around each tip's clump, on bare twigs before the leaves.
                    let mut r = Rng(77);
                    let reach = c.clump * leaves.powf(0.6).max(0.45);
                    for &(t, _) in &tips {
                        for _ in 0..3 {
                            let d = v(r.range(-1.0, 1.0), r.range(-0.2, 1.0), r.range(-1.0, 1.0)).normalize_or_zero();
                            mb.ellipsoid(t + d * reach, v(0.075, 0.05, 0.075) * bloom, mix(hex(0xF3C0BC), hex(0xFFF0EC), r.range(0.0, 0.5)));
                        }
                    }
                }
            }
            Species::Pine => {
                mb.cyl(v(0.0, -0.1, 0.0), v(0.0, 1.5, 0.0), 0.24, 0.16, trunk, trunk, 7);
                root_flare(&mut mb, 0.24, trunk);
                let top = pine_top;
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
                let c = canopy_of(sp);
                let tips = canopy(&mut mb, c, leaves, hex(0x7d6a44), dark, light, true);
                willow_strands(&mut mb, c, &tips, leaves, dark, light);
            }
        },
        1 => {
            if sp == Species::Pine {
                mb.cyl(v(0.0, -0.1, 0.0), v(0.0, top_h * 0.55, 0.0), 0.26, 0.16, trunk, trunk, 5);
                let top = pine_top;
                for (y0, rr, h) in [(1.0f32, 1.5f32, 1.9f32), (2.4, 1.05, 1.6), (3.6, 0.6, 1.2)] {
                    mb.lathe(Mat4::IDENTITY, &[(0.0, y0 + 0.2), (rr, y0), (rr * 0.6, y0 + h * 0.5), (0.0, y0 + h)], seg, dark, top, true);
                }
            } else {
                let c = canopy_of(sp);
                let tw = if sp == Species::Aspen { 0.12 } else { 0.2 };
                mb.cyl(v(0.0, -0.1, 0.0), c.fork + v(0.0, 0.2, 0.0), tw * 1.3, tw * 0.8, trunk, trunk, 5);
                canopy(&mut mb, c, leaves, trunk, dark, light, false);
            }
        }
        _ => {
            if bare {
                mb.cyl(v(0.0, -0.1, 0.0), v(0.0, top_h * 0.7, 0.0), 0.25, 0.1, trunk, hex(0x8a7a60), 4);
            } else if sp == Species::Pine {
                let top = pine_top;
                mb.lathe(Mat4::IDENTITY, &[(0.0, 0.8), (1.5, 1.8), (1.1, 3.4), (0.0, 4.9)], 6, dark, top, true);
            } else {
                mb.lathe(Mat4::IDENTITY, &[(0.0, 1.2), (1.5 * leaf_scale, 2.6), (1.1 * leaf_scale, 3.9), (0.0, 4.8)], 5, dark, light, false);
            }
        }
    }
    mb.build()
}

/// The shape of a broadleaf crown: where its limbs leave the trunk, the crown it fills, how many
/// branch tips carry leaves, how big a full clump is, and how thick a limb starts.
#[derive(Clone, Copy)]
struct Canopy {
    fork: Vec3,
    centre: Vec3,
    radius: Vec3,
    tips: usize,
    clump: f32,
    limb: f32,
    seed: u32,
}

fn canopy_of(sp: Species) -> Canopy {
    match sp {
        Species::Aspen => Canopy { fork: v(0.0, 2.3, 0.0), centre: v(0.0, 3.6, 0.0), radius: v(0.85, 1.45, 0.85), tips: 12, clump: 0.46, limb: 0.075, seed: 3 },
        Species::Maple => Canopy { fork: v(0.0, 1.45, 0.0), centre: v(0.0, 3.2, 0.0), radius: v(1.6, 1.3, 1.6), tips: 16, clump: 0.66, limb: 0.12, seed: 7 },
        Species::Willow => Canopy { fork: v(0.0, 2.2, 0.0), centre: v(0.0, 3.15, 0.0), radius: v(1.6, 0.95, 1.6), tips: 12, clump: 0.55, limb: 0.1, seed: 9 },
        Species::Pine => Canopy { fork: v(0.0, 1.5, 0.0), centre: v(0.0, 3.0, 0.0), radius: v(1.2, 1.5, 1.2), tips: 8, clump: 0.5, limb: 0.1, seed: 5 },
    }
}

/// A broadleaf crown grown on its branches. Limbs rise from the trunk, curve outward and fork;
/// every tip carries a clump of leaves that swells from a bud in spring to the full crown in
/// summer and thins again in autumn. Branches end in leaves (or bare tips in winter), so none
/// stick out past the crown. Returns the tips, for blossom and hanging strands.
fn canopy(mb: &mut Mb, c: Canopy, leaves: f32, bark: [f32; 3], dark: [f32; 3], light: [f32; 3], detail: bool) -> Vec<(Vec3, f32)> {
    let mut rng = Rng(c.seed * 7919 + 13);
    let n = if detail { c.tips } else { (c.tips / 2).max(4) };
    let tips: Vec<(Vec3, f32)> = (0..n).map(|i| {
        let y = 0.85 - (i as f32 + 0.5) / n as f32 * 1.35;
        let ring = (1.0 - y * y).max(0.0).sqrt();
        let a = i as f32 * 2.399_963 + rng.range(-0.3, 0.3);
        (c.centre + v(a.cos() * ring * c.radius.x, y * c.radius.y, a.sin() * ring * c.radius.z) * 0.78, y)
    }).collect();
    let seg = if detail { 5 } else { 4 };
    let tip_col = mix(bark, hex(0xb8a68c), 0.3);
    let lean = |from: Vec3, to: Vec3| v((to - from).x * 0.3, (to - from).y * 0.65, (to - from).z * 0.3);
    let mut forks = Vec::new();
    // Limbs in pairs of neighbouring tips: up from the trunk, out, then forking to each tip.
    for pair in tips.chunks(2) {
        let m = pair.iter().map(|t| t.0).sum::<Vec3>() / pair.len() as f32;
        let elbow = c.fork + lean(c.fork, m);
        let split = m.lerp(elbow, 0.35);
        mb.cyl(c.fork, elbow, c.limb, c.limb * 0.7, bark, bark, seg);
        mb.cyl(elbow, split, c.limb * 0.7, c.limb * 0.45, bark, bark, seg);
        for &(t, _) in pair {
            mb.cyl(split, t, c.limb * 0.42, (c.limb * 0.15).max(0.012), bark, tip_col, seg - 1);
            if detail && leaves < 0.6 {
                // Fine twigs at the tip: a bare crown still reads as a branching tree.
                let out = (t - c.centre).normalize_or_zero();
                for k in [-1.0f32, 1.0] {
                    let side = out.cross(Vec3::Y).normalize_or_zero() * 0.35 * k;
                    let end = t + (out * 0.6 + Vec3::Y * 0.55 + side).normalize() * 0.32;
                    mb.cyl(t, end, 0.018, 0.006, tip_col, tip_col, 3);
                }
            }
        }
        forks.push(split);
    }
    if leaves < 0.02 {
        return tips;
    }
    let grow = leaves.powf(0.6);
    let fill = smooth01(0.3, 1.0, leaves);
    let (cs, cr) = if detail { (8, 5) } else { (5, 3) };
    if fill > 0.02 {
        mb.ellipsoid_g(c.centre, c.radius * 0.72 * fill, mix(dark, hex(INK), 0.14), mix(dark, light, 0.4), cs + 1, cr + 1);
    }
    // Clumps darken toward their undersides and the crown's base, so a crown has form in
    // mostly ambient light.
    let clump_col = |y: f32, j: f32| {
        let up = (y * 0.5 + 0.5).clamp(0.0, 1.0);
        let lo = mix(mix(dark, hex(INK), 0.16 * (1.0 - up)), light, (0.02 + 0.3 * up + j).clamp(0.0, 1.0));
        let hi = mix(dark, light, (0.5 + 0.35 * up + j).clamp(0.0, 1.0));
        (lo, hi)
    };
    for &(t, y) in &tips {
        let s = c.clump * rng.range(0.85, 1.15) * (1.0 - 0.15 * y.max(0.0)) * grow;
        let (lo, hi) = clump_col(y, rng.range(-0.12, 0.12));
        mb.ellipsoid_g(t, v(s, s * 0.82, s), lo, hi, cs, cr);
    }
    if detail && fill > 0.02 {
        for &f in &forks {
            let y = ((f.y - c.centre.y) / c.radius.y).clamp(-1.0, 1.0);
            let s = c.clump * 0.8 * fill;
            let (lo, hi) = clump_col(y, rng.range(-0.1, 0.1));
            mb.ellipsoid_g(f, v(s, s * 0.82, s), lo, hi, cs, cr);
        }
    }
    tips
}

/// Willow: from each outer tip, thin yellowish withies droop to below the crown, leafed along
/// their length in season.
fn willow_strands(mb: &mut Mb, c: Canopy, tips: &[(Vec3, f32)], leaves: f32, dark: [f32; 3], light: [f32; 3]) {
    let withy = hex(0x9a8a4a);
    let grow = leaves.powf(0.6);
    let mut r = Rng(19);
    for &(t, y) in tips.iter().filter(|t| t.1 < 0.55) {
        let out = v(t.x - c.centre.x, 0.0, t.z - c.centre.z).normalize_or_zero();
        for k in 0..2 {
            let side = out.cross(Vec3::Y) * (k as f32 - 0.5) * 0.5;
            let len = r.range(1.3, 1.8) * (1.0 - 0.3 * y.max(0.0));
            let p1 = t + out * 0.3 + side + Vec3::Y * 0.08;
            let p2 = p1 + out * 0.12 - Vec3::Y * len * 0.5;
            let p3 = p2 + out * 0.04 - Vec3::Y * len * 0.5;
            for (a, b) in [(t, p1), (p1, p2), (p2, p3)] {
                mb.cyl(a, b, 0.02, 0.012, withy, withy, 3);
            }
            if grow > 0.02 {
                for i in 0..6 {
                    let u = i as f32 / 5.0;
                    let p = if u < 0.5 { p1.lerp(p2, u * 2.0) } else { p2.lerp(p3, (u - 0.5) * 2.0) };
                    let s = 0.1 * grow * (1.0 - 0.35 * u);
                    mb.ellipsoid_g(p, v(s, s * 1.7, s), mix(dark, light, 0.15), mix(dark, light, 0.5), 5, 3);
                }
            }
        }
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

/// Construction shows in this many steps before a building is complete.
pub const STAGES: u8 = 6;
/// A finished building left high and dry: decayed.
pub const DRY_STAGE: u8 = u8::MAX;

pub fn stage_of(progress: f32) -> u8 {
    if progress >= 1.0 { STAGES } else { ((progress.max(0.0) * STAGES as f32) as u8).min(STAGES - 1) }
}

pub(super) fn col(c: palette::Rgb) -> [f32; 3] { hex(pack(c)) }

/// A sprig of bright autumn leaves: how beavers' sites are marked, readable on grass and water.
pub(super) fn leaf_sprig(mb: &mut Mb, p: Vec3, seed: u32) {
    let mut r = Rng(seed * 31 + 7);
    for k in 0..4 {
        let a = k as f32 * 1.6 + r.range(-0.3, 0.3);
        let o = v(a.cos() * 0.09, r.range(-0.04, 0.06), a.sin() * 0.09);
        mb.ellipsoid_r(p + o, v(0.1, 0.025, 0.06), Quat::from_rotation_y(-a) * Quat::from_rotation_z(0.4), mix(col(palette::MARKER_LEAF), hex(0xF2A23C), r.next() * 0.6));
    }
}

/// A peeled marker stick pushed into the ground, leaning a little, with a sprig on top.
pub(super) fn stake(mb: &mut Mb, p: Vec3, h: f32, flag: bool) {
    let top = p + v(0.08, h, 0.04);
    stick_c(mb, p + v(0.0, -0.4, 0.0), top, 0.05, col(palette::PEELED));
    if flag {
        leaf_sprig(mb, top, (p.x * 13.0 + p.z * 7.0).abs() as u32);
    }
}

/// A building site: marker sticks at the corners and peeled twigs laid along its edges.
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

/// A stick: bark with cut ends a shade lighter (not bright, or sticks read as matches).
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

/// A leafy branch: a stick with clumps of leaves along its outer half.
pub(super) fn branch(mb: &mut Mb, a: Vec3, b: Vec3, r: f32, leaf: [f32; 3], seed: u32) {
    let mut rng = Rng(seed);
    stick(mb, a, b, r, rng.next());
    for k in 0..3 {
        let p = a.lerp(b, 0.45 + 0.22 * k as f32) + v(rng.range(-0.1, 0.1), 0.08, rng.range(-0.1, 0.1));
        let s = rng.range(0.18, 0.28);
        mb.ellipsoid_g(p, v(s, s * 0.6, s), mix(leaf, hex(pack(palette::INK)), 0.25), mix(leaf, col(palette::LEAF_LIGHT), 0.5), 7, 4);
    }
}

pub fn footing_of(kind: StructKind) -> Option<structures::Footing> {
    use structures::Footing::*;
    Some(match kind {
        StructKind::Lodge => Mound(3.3),
        StructKind::Warehouse | StructKind::Larder => Piles(2.9, 2.5),
        StructKind::Toolshed | StructKind::Forester | StructKind::Logistics => Piles(1.9, 1.6),
        StructKind::Lookout => Piles(1.5, 1.5),
        StructKind::Harbor => Piles(2.9, 2.5),
        _ => return None,
    })
}

pub fn struct_mesh(kind: StructKind, len: f32, stage: u8) -> Mesh {
    let mut mb = Mb::new();
    let dry = stage == DRY_STAGE;
    let f = if dry { 1.0 } else { stage.min(STAGES) as f32 / STAGES as f32 };
    match kind {
        StructKind::Lodge => lodge(&mut mb, f),
        StructKind::Dam => dam(&mut mb, len, f),
        StructKind::Canal => canal(&mut mb, len, if f >= 1.0 { 3 } else { 0 }),
        StructKind::Warehouse => structures::stockpile(&mut mb, f),
        StructKind::Larder => structures::larder(&mut mb, f),
        StructKind::Toolshed => structures::toolshed(&mut mb, f),
        StructKind::Forester => structures::forester(&mut mb, f),
        StructKind::Logistics => structures::logistics(&mut mb, f),
        StructKind::Lookout => structures::lookout(&mut mb, f),
        StructKind::Harbor => structures::harbor(&mut mb, f),
    }
    if let Some(fo) = footing_of(kind) {
        let (hx, hz) = match fo {
            structures::Footing::Mound(r) => (r, r),
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

pub fn reinforcement_mesh(kind: StructKind) -> Mesh {
    let mut mb = Mb::new();
    let (dark, light) = (col(palette::MUD_DARK), col(palette::DAM_MUD));
    let stone = (hex(0x77777c), hex(0xb7b5b2));
    match footing_of(kind).unwrap_or(structures::Footing::Piles(2.5, 1.4)) {
        structures::Footing::Mound(radius) => {
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
        // Mud and stones heaped round each corner post.
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

/// Grey pebbles set shoulder to shoulder round the footing.
pub fn pebble_facing_mesh(kind: StructKind) -> Mesh {
    let mut mb = Mb::new();
    let (dark, light) = (hex(0x6e6e74), hex(0xc2c0bc));
    let mut r = Rng(77);
    let (rx, rz, n) = match footing_of(kind).unwrap_or(structures::Footing::Piles(2.5, 1.4)) {
        structures::Footing::Mound(radius) => (radius + 0.45, radius + 0.45, 22),
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

/// A rocky outcrop: a few weathered boulders bedded in the ground.
pub fn pebble_mesh() -> Mesh {
    let mut mb = Mb::new();
    for (x, z, rx, ry, rz, yaw) in [(0.0, 0.0, 1.0, 0.75, 0.85, 0.3), (0.85, 0.55, 0.6, 0.45, 0.55, 1.2), (-0.75, 0.45, 0.55, 0.4, 0.5, 2.0), (0.2, -0.8, 0.55, 0.35, 0.45, 0.7)] {
        let start = mb.len();
        mb.ellipsoid_g(v(x, 0.0, z), v(rx, ry, rz), hex(0x5f5f64), hex(0xb9b7b2), 9, 6);
        let c = v(x, 0.0, z);
        mb.displace(start, |p| {
            let q = p - c;
            let k = 1.0 + 0.12 * (q.x * 6.0 + yaw).sin() * (q.z * 5.0 - yaw).cos();
            c + v(q.x * k, q.y * (1.0 + 0.08 * (q.z * 7.0).sin()), q.z * k)
        });
    }
    mb.ellipsoid(v(-0.2, 0.62, 0.15), v(0.28, 0.06, 0.22), hex(0x7f9a4a));
    mb.build()
}

fn sapling_sprig(mb: &mut Mb, a: Vec3, b: Vec3) {
    stick_c(mb, a, b, 0.03, hex(0x7a5a34));
    mb.ellipsoid_g(b, v(0.16, 0.12, 0.16), col(palette::LEAF), col(palette::LEAF_LIGHT), 7, 5);
    for (k, s) in [(0.55f32, 1.0f32), (0.75, -1.0)] {
        let p = a.lerp(b, k);
        mb.ellipsoid_g(p + v(0.1 * s, 0.02, 0.04), v(0.1, 0.07, 0.09), col(palette::LEAF), col(palette::LEAF_LIGHT), 6, 4);
    }
    mb.ellipsoid_g(a, v(0.11, 0.08, 0.11), col(palette::MUD_DARK), hex(0x77573a), 6, 4);
}

/// A lodge: a rough cone of crossed sticks bedded in mud, with its entrance under water and a
/// loose vent of sticks at the top. It rises from a low ring as it is built.
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
        // The open top of an unfinished lodge: the dark chamber inside.
        let y = hgt * top_t;
        mb.lathe(Mat4::from_translation(v(0.0, y - 0.04, 0.0)), &[(shape(top_t) * 0.92, 0.0), (0.0, -0.3)], 18, hex(0x24170e), hex(0x24170e), false);
    }
    // Lumpy, not turned on a lathe.
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
        // Crossed: half lean one way across the slope, half the other.
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
        // Mud plastered over the sticks in patches, thickest low down.
        for _ in 0..16 {
            let (t, a) = (m.range(0.05, 0.7), m.range(0.0, TAU));
            let p = surface(t, a);
            let up = (p - v(0.0, p.y * 0.3, 0.0)).normalize_or_zero();
            let s = m.range(0.4, 0.65);
            mb.ellipsoid_r(p, v(s, 0.06, s * 0.8), Quat::from_rotation_arc(Vec3::Y, up), mix(col(palette::MUD_DARK), col(palette::DAM_MUD), 0.4 + 0.4 * m.next()));
        }
        // The vent: bare sticks, no mud, so air gets in.
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

fn dam(mb: &mut Mb, len: f32, f: f32) {
    if f >= 1.0 {
        structures::dam_cell(mb, 1.4, len, 0, structures::DamStage::Done);
    } else {
        structures::dam_cell(mb, 0.25 + 0.75 * f, len, 1, structures::DamStage::Rising);
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
    } else {
        mb.cuboid(v(0.0, -0.05, 0.0), v(hl + 0.05, 0.1, 1.0), Quat::IDENTITY, hex(0x3d2a1a), hex(0x6a4a2e));
        mud_lump(mb, v(-hl * 0.4, 0.35, 2.4), 1.3);
        stake(mb, v(hl - 0.3, 0.0, -1.9), 1.2, true);
        stake(mb, v(hl - 0.3, 0.0, 1.9), 1.2, true);
    }
}

/// What a beaver carries in its teeth, centred on the mouth.
pub fn load_mesh(good: sim::buildings::Good) -> Mesh {
    use sim::buildings::Good;
    let mut mb = Mb::new();
    match good {
        Good::Sticks => {
            for (i, dy) in [-0.02f32, 0.05, 0.12].iter().enumerate() {
                let k = i as f32 - 1.0;
                stick(&mut mb, v(-0.55, 0.04 + dy, -0.06 + k * 0.05), v(0.55, 0.08 + dy, -0.06 - k * 0.07), 0.035, 0.3 + 0.3 * i as f32);
            }
            mb.ellipsoid(v(0.5, 0.16, -0.1), v(0.1, 0.03, 0.06), col(palette::LEAF_LIGHT));
            mb.ellipsoid(v(-0.5, 0.16, -0.02), v(0.1, 0.03, 0.06), col(palette::LEAF));
        }
        Good::Clay => {
            mb.ellipsoid_g(v(0.0, -0.02, 0.02), v(0.2, 0.15, 0.17), col(palette::MUD_DARK), hex(0x80603f), 8, 5);
        }
        Good::Pebbles => {
            mb.ellipsoid_g(v(0.0, -0.02, 0.02), v(0.18, 0.13, 0.15), hex(0x66666b), hex(0xb5b3ae), 7, 5);
        }
        // A rooted sapling held by the stem, leaves up.
        Good::Saplings => sapling_sprig(&mut mb, v(-0.05, -0.15, 0.0), v(0.05, 0.4, 0.02)),
        // A leafy sprig held crosswise.
        Good::Food => {
            stick_c(&mut mb, v(-0.4, 0.0, 0.0), v(0.4, 0.05, 0.02), 0.025, hex(0x6f8a3a));
            for k in 0..5 {
                let x = -0.35 + k as f32 * 0.175;
                mb.ellipsoid_r(v(x, 0.06, 0.04 * (k % 2) as f32), v(0.12, 0.03, 0.07), Quat::from_rotation_y(0.5 * (k as f32 - 2.0)), mix(col(palette::LEAF), col(palette::LEAF_LIGHT), 0.25 * (k % 3) as f32));
            }
        }
    }
    mb.build()
}

/// What a beaver hugs in its forepaws, centred on the hold point.
pub fn paw_load_mesh(good: sim::buildings::Good) -> Mesh {
    use sim::buildings::Good;
    let mut mb = Mb::new();
    match good {
        Good::Sticks => {
            for k in 0..3 {
                let y = k as f32 * 0.07 - 0.07;
                stick(&mut mb, v(-0.45, y, 0.0), v(0.45, y + 0.03, 0.02 * k as f32), 0.04, 0.3 * k as f32);
            }
        }
        Good::Clay => {
            mb.ellipsoid_g(v(0.0, 0.0, 0.0), v(0.26, 0.22, 0.22), col(palette::MUD_WET), col(palette::DAM_MUD), 9, 6);
            mb.ellipsoid(v(0.08, 0.13, 0.05), v(0.1, 0.06, 0.08), hex(0x6a4c33));
        }
        Good::Pebbles => {
            mb.ellipsoid_g(v(0.0, 0.0, 0.0), v(0.27, 0.2, 0.22), hex(0x5f5f64), hex(0xbab8b3), 8, 5);
        }
        // Held slanted across the chest so the leaves stand clear of the head.
        Good::Saplings => sapling_sprig(&mut mb, v(-0.12, -0.12, 0.02), v(0.42, 0.3, 0.2)),
        Good::Food => {
            for k in 0..3 {
                let a = (k as f32 - 1.0) * 0.35;
                let d = v(a.sin(), 0.25, a.cos() * 0.3).normalize() * 0.4;
                stick_c(&mut mb, -d * 0.3, d, 0.025, hex(0x6f8a3a));
                mb.ellipsoid_g(d * 0.85, v(0.16, 0.1, 0.14), col(palette::LEAF), col(palette::LEAF_LIGHT), 7, 4);
            }
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
    // A low, lobed spread of wet mud: no single dome (a dome with clods round it reads as a turtle).
    let mut mb = Mb::new();
    for (x, z, rx, rz, hgt) in [(0.0, 0.0, 1.35, 0.85, 0.11), (0.75, 0.45, 0.8, 0.6, 0.09), (-0.8, 0.3, 0.7, 0.75, 0.08), (-0.2, -0.65, 0.9, 0.5, 0.09), (0.9, -0.5, 0.45, 0.4, 0.07)] {
        mb.ellipsoid_g(v(x, 0.0, z), v(rx, hgt, rz), hex(0x3f2c1b), hex(0x5e4430), 10, 4);
    }
    mb.build()
}

/// Water lilies: pads floating flat on the surface round a flower or two, a clump of reeds
/// standing out of the water beside them. Origin at the water line.
pub fn water_plant_mesh() -> Mesh {
    let mut mb = Mb::new();
    let pads = [(0.0f32, 0.0f32, 0.34f32), (0.55, 0.2, 0.26), (-0.45, 0.35, 0.28), (0.15, -0.55, 0.24), (-0.4, -0.35, 0.2), (0.6, -0.4, 0.18)];
    for (k, &(x, z, r)) in pads.iter().enumerate() {
        let shade = if k % 2 == 0 { hex(0x3f8f35) } else { hex(0x56a443) };
        mb.ellipsoid_r(v(x, 0.02, z), v(r, 0.025, r * 0.92), Quat::from_rotation_y(k as f32 * 1.3), shade);
    }
    // Flowers: a ring of pale petals round a yellow heart.
    for &(x, z, s) in &[(0.05f32, 0.05f32, 1.0f32), (-0.42, 0.33, 0.75)] {
        for i in 0..6 {
            let a = i as f32 * std::f32::consts::TAU / 6.0;
            mb.ellipsoid_r(v(x + a.cos() * 0.09 * s, 0.09 * s, z + a.sin() * 0.09 * s), v(0.09 * s, 0.035 * s, 0.05 * s), Quat::from_rotation_y(-a) * Quat::from_rotation_z(0.5), hex(0xf4e9f0));
        }
        mb.ellipsoid(v(x, 0.12 * s, z), v(0.05 * s, 0.04 * s, 0.05 * s), hex(0xf2c94c));
    }
    // Reeds with brown heads.
    for i in 0..5 {
        let a = 2.2 + i as f32 * 0.35;
        let base = v(0.75 + 0.1 * a.cos(), -0.1, 0.55 + 0.1 * a.sin());
        let tip = base + v(0.08 * (i as f32 - 2.0), 0.95 + 0.12 * (i % 2) as f32, 0.03 * i as f32);
        mb.cyl(base, tip, 0.025, 0.012, hex(0x4f8a3a), hex(0x9cc96a), 4);
        if i % 2 == 0 {
            mb.cyl(tip - v(0.0, 0.28, 0.0), tip - v(0.0, 0.08, 0.0), 0.045, 0.045, hex(0x6b4426), hex(0x7d5130), 5);
        }
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

/// Goods lying on the ground, about one load's worth; scaled with the amount.
pub fn heap_mesh(good: sim::buildings::Good) -> Mesh {
    use sim::buildings::Good;
    let mut mb = Mb::new();
    match good {
        Good::Sticks => stick_pile(&mut mb, v(0.0, 0.0, 0.0), 0.4, 7, 11),
        Good::Clay => {
            for (x, z, r, h) in [(0.0f32, 0.0f32, 0.42f32, 0.2f32), (0.3, 0.22, 0.25, 0.14), (-0.28, 0.18, 0.22, 0.12)] {
                mb.ellipsoid_g(v(x, 0.0, z), v(r, h, r * 0.85), col(palette::MUD_DARK), hex(0x80603f), 9, 5);
            }
        }
        Good::Pebbles => {
            for (k, (x, z)) in [(0.0f32, 0.0f32), (0.22, 0.12), (-0.2, 0.15), (0.05, -0.22), (-0.15, -0.12)].into_iter().enumerate() {
                let r = 0.14 + 0.03 * (k % 2) as f32;
                mb.ellipsoid_g(v(x, r * 0.6, z), v(r, r * 0.75, r * 0.9), hex(0x66666b), hex(0xb5b3ae), 7, 5);
            }
        }
        Good::Saplings => {
            for k in 0..3 {
                let x = -0.2 + 0.2 * k as f32;
                sapling_sprig(&mut mb, v(x, 0.0, 0.05 * k as f32), v(x + 0.05, 0.45, 0.05 * k as f32));
            }
        }
        // A cache of cut food left on the ground, made to read apart from a growing plant: a patch
        // of trodden earth, cut branches laid crosswise (peeled white and in their bark), curls
        // of stripped bark, and bundled greens heaped in a mound on top.
        Good::Food => {
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
    }
    mb.build()
}

/// The family's starting supplies: a tied bundle of sticks, mud and a heap of food.
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
