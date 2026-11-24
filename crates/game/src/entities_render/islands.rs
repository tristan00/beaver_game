//! The colony's work buildings, built as beavers build: in the shallows where they are safe,
//! of sticks, mud and stones, each on its own island heaped up out of the water as a lodge's
//! base is (or, for the food store and the forester, in the water itself). `f` is how far
//! built, 0 to 1; each rises from a marked-out site as it is built. The origin is just above
//! the water; the footing (`structures::footing`) reaches down to the bed. Mouths and slides
//! face -z.
use super::meshgen::{hex, mix, Mb, Rng};
use super::models::{branch, col, mud_lump, site_markers, stick, stick_c};
use super::structures::{dam_cell, pebble_cairn, stick_crib, DamStage};
use crate::palette;
use bevy::prelude::*;
use std::f32::consts::{FRAC_PI_2, PI, TAU};

fn v(x: f32, y: f32, z: f32) -> Vec3 { Vec3::new(x, y, z) }

/// How far a building's later parts have got, once `from` of the work is done.
fn after(f: f32, from: f32) -> f32 {
    ((f - from) / (1.0 - from)).clamp(0.0, 1.0)
}

/// A stick-and-mud island heaped out of the water: mud `h` high, sticks crossed over its sides.
/// `rx`, `rz` are its half sizes at the water; `f` how far built (it rises as it is built).
fn island(mb: &mut Mb, rx: f32, rz: f32, h: f32, f: f32, seed: u32) {
    let mut r = Rng(seed);
    let h = h * (0.3 + 0.7 * (f / 0.5).min(1.0));
    let prof = [(0.0, -0.3), (1.15, -0.3), (1.08, h * 0.35), (0.9, h * 0.85), (0.6, h), (0.0, h + 0.02)];
    let start = mb.len();
    mb.lathe(Mat4::from_scale(v(rx, 1.0, rz)), &prof, 20, col(palette::MUD_WET), col(palette::DAM_MUD), true);
    let ph = seed as f32 * 0.37;
    let wob = move |p: Vec3| {
        let a = p.z.atan2(p.x);
        let k = 1.0 + 0.06 * (3.0 * a + ph).sin() + 0.04 * (7.0 * a - ph).cos();
        v(p.x * k, p.y, p.z * k)
    };
    mb.displace(start, wob);
    mb.renormal(start);
    let n = (14.0 * (rx + rz) * (0.3 + 0.7 * f)) as usize;
    for i in 0..n {
        let a = r.range(0.0, TAU);
        let t = r.range(0.0, 0.85);
        let k = 1.1 - 0.3 * t;
        let p = wob(v(a.cos() * rx * k, t * h, a.sin() * rz * k)) + v(a.cos(), 0.0, a.sin()) * 0.05;
        let down = v(a.cos() * 0.3, -1.0, a.sin() * 0.3).normalize();
        let around = v(-a.sin(), 0.0, a.cos());
        let phi = if i % 2 == 0 { r.range(0.4, 1.2) } else { -r.range(0.4, 1.2) };
        let d = (down * phi.cos() + around * phi.sin()).normalize();
        let l = r.range(0.7, 1.2);
        stick(mb, p - d * l * 0.5, p + d * l * 0.5, r.range(0.05, 0.07), r.next());
    }
}

/// Builds a part in its own frame and sets it at `at`, turned by `yaw`.
fn placed(mb: &mut Mb, at: Vec3, yaw: f32, f: impl FnOnce(&mut Mb)) {
    let start = mb.len();
    f(mb);
    mb.place(start, Mat4::from_rotation_translation(Quat::from_rotation_y(yaw), at));
}

/// A hood of sticks and mud, open on its -z side over a dark mouth; `g` how far raised.
fn hood(mb: &mut Mb, c: Vec3, w: f32, g: f32, r: &mut Rng) {
    if g <= 0.0 {
        return;
    }
    let start = mb.len();
    mb.ellipsoid_g(c, v(w, 0.75 * g, 0.8), col(palette::MUD_DARK), col(palette::DAM_MUD), 12, 7);
    mb.displace(start, |p| if p.y < c.y { v(p.x, c.y, p.z) } else { p });
    mb.renormal(start);
    if g >= 1.0 {
        mb.ellipsoid_g(c + v(0.0, 0.25, -0.62), v(w * 0.62, 0.42, 0.25), hex(0x1E1610), hex(0x2A2018), 10, 6);
    }
    for _ in 0..(14.0 * g) as usize {
        let a = r.range(0.2, PI - 0.2);
        let p = c + v(a.cos() * w * 0.95, a.sin() * 0.7 * g, r.range(-0.4, 0.6));
        let d = v(-a.sin(), a.cos(), r.range(-0.3, 0.3)).normalize();
        stick(mb, p - d * 0.4, p + d * 0.4, 0.05, r.next());
    }
}

/// The stockpile: an island with three hooded bays of sticks and mud, sticks, mud and pebbles
/// each before its own mouth.
pub fn cache_island(mb: &mut Mb, f: f32) {
    let mut r = Rng(11);
    let h = 0.45;
    island(mb, 2.6, 1.9, h, f, 11);
    let g = after(f, 0.45);
    for (i, x) in [-1.5f32, 0.0, 1.5].into_iter().enumerate() {
        hood(mb, v(x, h, 0.5), 0.7, (g * 3.0 - i as f32).clamp(0.0, 1.0), &mut r);
        let front = v(x, h, -0.6);
        match i {
            0 => stick_crib(mb, front, 0.2 * g, &mut r),
            1 => {
                mud_lump(mb, front, 0.4 + 0.4 * g);
                mud_lump(mb, front + v(0.3, 0.1, -0.2), 0.5 * g.max(0.3));
            }
            _ => pebble_cairn(mb, front, 0.45 * g, &mut r),
        }
    }
    if f < 1.0 {
        site_markers(mb, 3.3, 2.6, 0.0);
    }
}

/// The food store, as beavers really keep winter food: a feed pile of leafy branches pushed
/// into the shallows, butts stuck in the mud bed and tips standing out of the water.
pub fn feed_pile(mb: &mut Mb, f: f32) {
    let mut r = Rng(22);
    mb.ellipsoid_g(v(0.0, -0.1, 0.0), v(1.6, 0.25 + 0.1 * f, 1.3), col(palette::MUD_WET), col(palette::DAM_MUD), 12, 5);
    let leaf = |k: usize| match k % 3 { 0 => col(palette::LEAF), 1 => mix(col(palette::LEAF), hex(0xB59A3C), 0.4), _ => col(palette::LEAF_LIGHT) };
    let n = (8.0 + 32.0 * f) as usize;
    for i in 0..n {
        let a = i as f32 * 2.399 + r.range(-0.2, 0.2);
        let butt = v(a.cos() * 0.5, -0.3, a.sin() * 0.45);
        let dir = v(a.cos() * 0.8, r.range(0.7, 1.5), a.sin() * 0.8).normalize();
        branch(mb, butt, butt + dir * r.range(1.4, 2.4) * (0.6 + 0.4 * f), 0.06, leaf(i), 100 + i as u32);
    }
    for i in 0..(8.0 * f) as usize {
        let a = i as f32 * TAU / 8.0;
        let p = v(a.cos() * 1.5, -0.05, a.sin() * 1.2);
        stick(mb, p, p + v(-a.sin(), 0.05, a.cos()), 0.06, r.next());
    }
    if f < 1.0 {
        site_markers(mb, 2.4, 2.2, 0.0);
    }
}

/// The tool shed, as a stone cache: an island walled round with stacked stones, pebbles heaped
/// inside, peeled sticks and gnawed chips about its rim.
pub fn stone_cache(mb: &mut Mb, f: f32) {
    let mut r = Rng(33);
    let h = 0.4;
    island(mb, 1.6, 1.4, h, f, 33);
    for ring in 0..2 {
        if f < 0.4 + 0.25 * ring as f32 {
            break;
        }
        let n = 16 - ring * 3;
        for i in 0..n {
            let a = i as f32 * TAU / n as f32 + ring as f32 * 0.2;
            let s = r.range(0.14, 0.2);
            let c = mix(hex(0x6E6E72), hex(0xBAB6B0), r.next());
            mb.ellipsoid_g(v(a.cos() * 0.95, h + s * 0.5 + ring as f32 * 0.2, a.sin() * 0.8), v(s * 1.3, s * 0.7, s), mix(c, hex(0x2E2A26), 0.3), c, 7, 4);
        }
    }
    pebble_cairn(mb, v(0.0, h, 0.0), 0.7 * after(f, 0.5), &mut r);
    if f >= 1.0 {
        for i in 0..10 {
            let a = i as f32 * TAU / 10.0 + 0.3;
            let p = v(a.cos() * 1.35, h * 0.6, a.sin() * 1.15);
            stick_c(mb, p, p + v(-a.sin(), 0.08, a.cos()) * 0.8, 0.04, col(palette::PEELED));
        }
        for _ in 0..8 {
            let p = v(r.range(-1.1, 1.1), h + 0.02, r.range(-0.9, 0.9));
            mb.cuboid(p, v(0.05, 0.015, 0.03), Quat::from_rotation_y(r.range(0.0, TAU)), col(palette::CUT_WOOD), col(palette::PEELED));
        }
    } else {
        site_markers(mb, 2.2, 2.0, 0.0);
    }
}

/// The forester, as willow shallows: a low ring of piled sticks and mud closing off a calm
/// patch of shallow water, willow and aspen cuttings pushed into its bed to root, standing out
/// of the water in rows.
pub fn willow_shallows(mb: &mut Mb, f: f32) {
    let mut r = Rng(44);
    let wall = 12;
    let built = ((f / 0.6) * wall as f32).ceil() as usize;
    for k in 0..wall.min(built) {
        let a = k as f32 * TAU / wall as f32;
        let c = v(a.cos() * 1.75, -0.05, a.sin() * 1.45);
        placed(mb, c, -(a + FRAC_PI_2), |mb| dam_cell(mb, 0.35, 0.95, k as u32, DamStage::Done));
    }
    let withy = hex(0x8E9A48);
    let set = ((after(f, 0.6) * 15.0).ceil() as usize).min(15);
    for i in 0..set {
        let (row, k) = (i / 5, i % 5);
        let base = v(-1.0 + k as f32 * 0.5 + r.range(-0.08, 0.08), -0.25, -0.5 + row as f32 * 0.5);
        let tip = base + v(r.range(-0.08, 0.08), r.range(0.8, 1.1), r.range(-0.08, 0.08));
        stick_c(mb, base, tip, 0.025, withy);
        for j in 0..3 {
            let p = base.lerp(tip, 0.6 + 0.15 * j as f32) + v(r.range(-0.06, 0.06), 0.0, r.range(-0.06, 0.06));
            mb.ellipsoid_g(p, v(0.08, 0.05, 0.08), col(palette::LEAF), col(palette::LEAF_LIGHT), 6, 4);
        }
    }
    if f < 1.0 {
        site_markers(mb, 2.3, 2.0, 0.0);
    }
}

/// Logistics, as a haul island: loads stacked on an island, a slick slide worn down its side
/// into the water and sticks laid across its top as skids.
pub fn haul_island(mb: &mut Mb, f: f32) {
    let mut r = Rng(55);
    let h = 0.55;
    island(mb, 1.7, 1.5, h, f, 55);
    if f >= 0.6 {
        let start = mb.len();
        mb.ellipsoid_g(Vec3::ZERO, v(0.42, 0.06, 1.0), col(palette::MUD_WET), mix(col(palette::MUD_WET), hex(0x8A7A6A), 0.4), 12, 4);
        mb.place(start, Mat4::from_rotation_translation(Quat::from_rotation_x(-0.45), v(0.0, 0.22, -1.35)));
        for k in 0..5 {
            let z = -0.6 + k as f32 * 0.3;
            stick(mb, v(-0.5, h + 0.04, z), v(0.5, h + 0.04, z + r.range(-0.05, 0.05)), 0.05, r.next());
        }
    }
    if f >= 1.0 {
        stick_crib(mb, v(-0.8, h, 0.6), 0.2, &mut r);
        mud_lump(mb, v(0.8, h, 0.5), 0.8);
        mud_lump(mb, v(0.5, h + 0.05, 0.9), 0.5);
    } else {
        site_markers(mb, 2.3, 2.1, 0.0);
    }
}

/// The lookout, as a watch mound: a tall, narrow cone of sticks and mud rising out of the
/// shallows, a trodden flat top to sit up on, marked with scent. It rises as it is built.
pub fn watch_mound(mb: &mut Mb, f: f32) {
    let mut r = Rng(66);
    let tall = 0.25 + 0.75 * f;
    let full = [(0.0, -0.3), (1.55, -0.3), (1.3, 0.6), (0.9, 1.7), (0.6, 2.7), (0.52, 3.0), (0.0, 3.04)];
    let prof: Vec<(f32, f32)> = full.iter().map(|&(rr, y)| (rr, if y > 0.0 { y * tall } else { y })).collect();
    let start = mb.len();
    mb.lathe(Mat4::IDENTITY, &prof, 18, col(palette::MUD_DARK), col(palette::DAM_MUD), true);
    let wob = |p: Vec3| {
        let a = p.z.atan2(p.x);
        let k = 1.0 + 0.07 * (3.0 * a + p.y * 1.1).sin() + 0.04 * (7.0 * a + p.y * 2.3).sin();
        v(p.x * k, p.y, p.z * k)
    };
    mb.displace(start, wob);
    mb.renormal(start);
    let radius = |y: f32| {
        let mut out = 0.0;
        for w in prof.windows(2) {
            let ((r0, y0), (r1, y1)) = (w[0], w[1]);
            if y >= y0 && y <= y1 && y1 > y0 {
                out = r0 + (r1 - r0) * (y - y0) / (y1 - y0);
            }
        }
        out
    };
    let top = 3.0 * tall;
    for i in 0..(30.0 + 80.0 * f) as usize {
        let y = r.range(0.05, top - 0.1);
        let a = r.range(0.0, TAU);
        let rr = radius(y);
        let p = wob(v(a.cos() * rr, y, a.sin() * rr)) + v(a.cos(), 0.0, a.sin()) * 0.05;
        let slope = (radius(y - 0.1) - radius(y + 0.1)) / 0.2;
        let down = v(a.cos() * slope, -1.0, a.sin() * slope).normalize();
        let around = v(-a.sin(), 0.0, a.cos());
        let phi = if i % 2 == 0 { r.range(0.3, 1.1) } else { -r.range(0.3, 1.1) };
        let d = (down * phi.cos() + around * phi.sin()).normalize();
        let l = r.range(0.7, 1.3);
        stick(mb, p - d * l * 0.5, p + d * l * 0.5, r.range(0.05, 0.075), r.next());
    }
    if f >= 1.0 {
        mb.disc(v(0.0, top + 0.05, 0.0), 0.5, 0.5, hex(0x86705A), 14);
        mb.ellipsoid(v(0.15, top + 0.06, 0.1), v(0.2, 0.03, 0.16), hex(0x2E2018));
    } else {
        site_markers(mb, 2.0, 2.0, 0.0);
    }
}
