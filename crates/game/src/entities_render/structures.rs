use super::meshgen::{hex, mix, Mb, Rng};
use super::models::{hex_roof_c, mud_lump, site_markers, stake, stick, stick_c, stick_pile};
use crate::palette::{self, pack};
use bevy::prelude::*;
use std::f32::consts::{FRAC_PI_2, TAU};

fn v(x: f32, y: f32, z: f32) -> Vec3 { Vec3::new(x, y, z) }

const DAM_MUD: u32 = pack(palette::DAM_MUD);
const CUT_WOOD: u32 = pack(palette::CUT_WOOD);

fn peeled() -> [f32; 3] { mix(hex(CUT_WOOD), hex(0xd8c29a), 0.4) }

pub const DAM_STEP: f32 = 0.25;

/// How far a dam cell has come: a surveyor's stake, a wall going up beside its
/// stick pile, or a finished wall.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DamStage {
    Staked,
    Rising,
    Done,
}

pub fn dam_cell(mb: &mut Mb, h: f32, len: f32, variant: u32, stage: DamStage) {
    if stage == DamStage::Staked {
        stake(mb, v(0.0, 0.0, 0.0), 1.1, variant % 2 == 0);
        mud_lump(mb, v(0.0, 0.0, 0.0), 0.35);
        return;
    }
    let site = stage == DamStage::Rising;
    let hl = len * 0.5;
    let h = h.max(0.1);
    let (wt, wb, bury) = (0.2f32, 0.3 + 0.5 * h, 0.45f32);
    let face = |t: f32| wb + (wt - wb) * t + 0.22 * h.min(1.5) * t * (1.0 - t);
    let (mud_lo, mud_hi) = (hex(0x4d3520), hex(DAM_MUD));
    let mut prof: Vec<(f32, f32)> = vec![(-(wb + bury * (wb - wt) / h), -bury)];
    for t in [0.0f32, 0.3, 0.6, 0.85] {
        prof.push((-face(t), t * h));
    }
    prof.extend([(-wt * 0.55, h + 0.03), (wt * 0.55, h + 0.03)]);
    for t in [0.85f32, 0.6, 0.3, 0.0] {
        prof.push((face(t), t * h));
    }
    prof.push((wb + bury * (wb - wt) / h, -bury));
    mb.extrude_x(&prof, -hl, hl, mud_lo, mix(mud_lo, mud_hi, 0.85));
    let mut r = Rng(9001 + variant * 7717 + (h * 8.0) as u32 * 31);
    let dark_bark = |r: &mut Rng| mix(hex(0x5d3a20), hex(0x7a5230), r.next());
    for s in [-1.0f32, 1.0] {
        let t = r.range(0.25, 0.6);
        mb.ellipsoid_g(v(r.range(-hl * 0.5, hl * 0.5), t * h, s * (face(t) - 0.04)), v(r.range(0.3, 0.45), h * 0.22 + 0.05, 0.12), mud_lo, mix(mud_lo, mud_hi, 0.6), 8, 4);
        let n = (len * h * 12.0) as usize + 5;
        for _ in 0..n {
            let t = r.range(0.05, 0.95);
            let slope = (face((t + 0.05).min(1.0)) - face((t - 0.05).max(0.0))) / (0.1 * h);
            let up = v(0.0, 1.0, s * slope).normalize();
            let c = v(r.range(-hl, hl), t * h, s * (face(t) + 0.035));
            let across = r.next() < 0.25;
            let j = if across { FRAC_PI_2 + r.range(-0.3, 0.3) } else { r.range(-0.6, 0.6) };
            let d = (up * j.cos() + Vec3::X * j.sin()).normalize();
            let l = r.range(0.5, 1.1) * (0.55 + 0.45 * (h / 1.5).min(1.0));
            let rad = r.range(0.035, 0.065);
            let pick = r.next();
            if pick < 0.15 {
                stick_c(mb, c - d * l * 0.5, c + d * l * 0.5, rad, peeled());
            } else if pick < 0.45 {
                let col = dark_bark(&mut r);
                stick_c(mb, c - d * l * 0.5, c + d * l * 0.5, rad, col);
            } else {
                stick(mb, c - d * l * 0.5, c + d * l * 0.5, rad, r.range(0.0, 0.6));
            }
        }
    }
    for k in 0..3 {
        let z = (k as f32 - 1.0) * wt * 0.6 + r.range(-0.04, 0.04);
        let (x0, x1) = (-hl - r.range(0.0, 0.2), hl + r.range(0.0, 0.2));
        let (a, b) = (v(x0, h + 0.03, z), v(x1, h + r.range(-0.02, 0.05), z + r.range(-0.08, 0.08)));
        if k == 1 {
            stick_c(mb, a, b, 0.055, peeled());
        } else {
            stick(mb, a, b, 0.06, r.range(0.0, 0.5));
        }
    }
    for _ in 0..2 {
        mb.ellipsoid_g(v(r.range(-hl * 0.7, hl * 0.7), h + 0.04, r.range(-0.06, 0.06)), v(r.range(0.25, 0.4), 0.1, wt * 1.25), mud_lo, mud_hi, 8, 4);
    }
    if variant % 3 == 0 && !site {
        let c = v(r.range(-hl * 0.5, hl * 0.5), h + 0.05, r.range(-0.1, 0.1));
        stick(mb, c, c + v(0.3, 0.18, 0.15), 0.03, 0.5);
        for (o, k) in [(v(0.3, 0.2, 0.15), 1.0f32), (v(0.18, 0.15, 0.25), 0.8), (v(0.36, 0.16, 0.02), 0.75)] {
            mb.ellipsoid_r(c + o, v(0.13 * k, 0.035, 0.07 * k), Quat::from_rotation_y(o.x * 4.0), hex(0x6ab84a));
        }
    }
    for s in [-1.0f32, 1.0] {
        for _ in 0..2 {
            let p = v(r.range(-hl, hl), 0.06, s * (wb + r.range(0.0, 0.2)));
            let k = r.range(0.12, 0.22);
            mb.ellipsoid_g(p, v(k * 1.2, k * 0.75, k), hex(0x77777c), hex(0xb7b5b2), 7, 4);
        }
        mud_lump(mb, v(r.range(-hl * 0.6, hl * 0.6), 0.02, s * (face(0.0) - 0.05)), 0.55);
    }
    if site {
        stake(mb, v(0.0, 0.0, wb + 0.3), 1.0 + h, true);
        // Every cell being worked keeps its pile of sticks beside it.
        stick_pile(mb, v(0.0, 0.0, -wb - 0.8), 0.0, 4, 3 + variant);
    }
}

pub fn dam_cell_mesh(steps: u8, diagonal: bool, variant: u32, stage: DamStage) -> Mesh {
    let mut mb = Mb::new();
    let len = if diagonal { std::f32::consts::SQRT_2 } else { 1.0 } + 0.35;
    dam_cell(&mut mb, steps.max(1) as f32 * DAM_STEP, len, variant, stage);
    mb.build()
}

pub fn dam_dir(is_dam: impl Fn(i32, i32) -> bool) -> u8 {
    let mut acc = Vec2::ZERO;
    for dy in -1..=1 {
        for dx in -1..=1 {
            if (dx, dy) != (0, 0) && is_dam(dx, dy) {
                let a = (dy as f32).atan2(dx as f32) * 2.0;
                acc += Vec2::new(a.cos(), a.sin());
            }
        }
    }
    if acc.length() < 1e-3 {
        return 0;
    }
    let a = 0.5 * acc.y.atan2(acc.x);
    ((a / (TAU / 8.0)).round() as i32).rem_euclid(4) as u8
}

pub const FOOTING_DEPTH: f32 = 4.2;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Footing {
    Mound(f32),
    Piles(f32, f32),
}

const STAIN_Y: f32 = -0.42;
fn stain() -> [f32; 3] { hex(0xd9d0bf) }
fn dried_weed() -> [f32; 3] { hex(0x8a7a4a) }

pub fn footing(mb: &mut Mb, f: Footing, dry: bool) {
    let d = FOOTING_DEPTH;
    let mut r = Rng(4242);
    match f {
        Footing::Mound(rad) => {
            let (lo, hi) = (hex(0x3f2c1b), hex(DAM_MUD));
            mb.lathe(Mat4::IDENTITY, &[(0.0, -d), (rad * 1.3, -d), (rad * 1.16, -d * 0.5), (rad * 1.05, -0.5), (rad * 1.0, 0.08), (0.0, 0.08)], 16, lo, hi, true);
            for i in 0..16 {
                let a = i as f32 / 16.0 * TAU + r.range(-0.15, 0.15);
                let y = r.range(-d * 0.6, -0.25);
                let k = (y / -d).clamp(0.0, 1.0);
                let rr = rad * (1.0 + 0.16 * k) + 0.05;
                let p = v(a.cos() * rr, y, a.sin() * rr);
                let dir = v(-a.sin() * r.range(-0.6, 0.6), -1.0, a.cos() * r.range(-0.6, 0.6)).normalize();
                let l = r.range(0.9, 1.6);
                if r.next() < 0.2 {
                    stick_c(mb, p - dir * l * 0.5, p + dir * l * 0.5, 0.08, peeled());
                } else {
                    stick(mb, p - dir * l * 0.5, p + dir * l * 0.5, 0.085, r.next());
                }
            }
            if dry {
                mb.lathe(Mat4::IDENTITY, &[(rad * 1.06, STAIN_Y - 0.12), (rad * 1.075, STAIN_Y), (rad * 1.06, STAIN_Y + 0.08)], 16, stain(), stain(), true);
                for i in 0..10 {
                    let a = i as f32 / 10.0 * TAU + 0.3;
                    let rr = rad * 1.08;
                    let top = v(a.cos() * rr, STAIN_Y - 0.1, a.sin() * rr);
                    mb.cyl(top, top + v(0.0, -r.range(0.35, 0.8), 0.0), 0.07, 0.015, dried_weed(), mix(dried_weed(), stain(), 0.4), 4);
                }
            }
        }
        Footing::Piles(hx, hz) => {
            let wood = (hex(0x5a3a22), hex(0x8a5a32));
            let nx = ((hx * 2.0 / 1.3).ceil() as usize).max(1);
            let nz = ((hz * 2.0 / 1.3).ceil() as usize).max(1);
            let px = |i: usize| -hx + 2.0 * hx * i as f32 / nx as f32;
            let pz = |j: usize| -hz + 2.0 * hz * j as f32 / nz as f32;
            let planks = (hx * 2.0 / 0.42) as usize;
            for i in 0..planks {
                let x = -hx - 0.15 + (2.0 * hx + 0.3) * (i as f32 + 0.5) / planks as f32;
                let (a, b) = if i % 2 == 0 { (hex(0xa97a46), hex(0xc4965c)) } else { (hex(0x9a6e3e), hex(0xb98650)) };
                mb.cuboid(v(x, -0.04, 0.0), v((2.0 * hx + 0.3) / planks as f32 * 0.5 - 0.015, 0.09, hz + 0.2), Quat::IDENTITY, a, b);
            }
            for s in [-1.0f32, 1.0] {
                mb.cuboid(v(0.0, -0.22, s * (hz + 0.15)), v(hx + 0.2, 0.1, 0.08), Quat::IDENTITY, wood.0, wood.1);
            }
            let mut posts = Vec::new();
            for i in 0..=nx {
                for j in 0..=nz {
                    if i == 0 || i == nx || j == 0 || j == nz {
                        posts.push(v(px(i), 0.0, pz(j)));
                    }
                }
            }
            for p in &posts {
                mb.cyl(*p + v(0.0, -d, 0.0), *p + v(0.0, -0.1, 0.0), 0.15, 0.13, wood.0, wood.1, 7);
                if dry {
                    mb.cyl(*p + v(0.0, STAIN_Y - 0.1, 0.0), *p + v(0.0, STAIN_Y + 0.06, 0.0), 0.165, 0.165, stain(), stain(), 7);
                    mb.cyl(*p + v(0.0, STAIN_Y - 0.12, 0.0), *p + v(0.0, STAIN_Y - 1.1, 0.0), 0.17, 0.155, mix(dried_weed(), hex(0x4e6a3a), 0.35), dried_weed(), 7);
                }
            }
            for side in 0..4 {
                let (a, b) = match side {
                    0 => (v(-hx, 0.0, -hz), v(hx, 0.0, -hz)),
                    1 => (v(-hx, 0.0, hz), v(hx, 0.0, hz)),
                    2 => (v(-hx, 0.0, -hz), v(-hx, 0.0, hz)),
                    _ => (v(hx, 0.0, -hz), v(hx, 0.0, hz)),
                };
                let n = if side < 2 { nx } else { nz };
                for k in 0..n {
                    let (p, q) = (a.lerp(b, k as f32 / n as f32), a.lerp(b, (k + 1) as f32 / n as f32));
                    mb.cyl(p + v(0.0, -0.35, 0.0), q + v(0.0, -2.2, 0.0), 0.06, 0.06, wood.0, wood.1, 5);
                    mb.cyl(q + v(0.0, -0.35, 0.0), p + v(0.0, -2.2, 0.0), 0.06, 0.06, wood.0, wood.1, 5);
                }
            }
            if dry {
                for _ in 0..6 {
                    let p = posts[(r.next() * posts.len() as f32) as usize % posts.len()];
                    mb.cyl(p + v(r.range(-0.3, 0.3), -0.6, 0.18), p + v(r.range(-0.3, 0.3), -1.3, 0.25), 0.06, 0.015, dried_weed(), mix(dried_weed(), stain(), 0.4), 4);
                }
            }
        }
    }
}

pub fn abandoned_tint() -> [f32; 3] { hex(0xa39a8f) }

pub fn abandoned_dressing(mb: &mut Mb, hx: f32, hz: f32) {
    let mut r = Rng(777);
    let grass = (hex(0x9a8a52), hex(0xd2bf7c));
    for i in 0..10 {
        let a = i as f32 / 10.0 * TAU + r.range(-0.2, 0.2);
        let p = v(a.cos() * (hx + 0.1), 0.05, a.sin() * (hz + 0.1));
        for k in 0..3 {
            let b = k as f32 * 2.1 + a;
            mb.cyl(p, p + v(b.cos() * 0.15, r.range(0.35, 0.6), b.sin() * 0.15), 0.05, 0.01, grass.0, grass.1, 3);
        }
    }
    for _ in 0..3 {
        let a = r.range(0.0, TAU);
        let p = v(a.cos() * (hx + 0.5), 0.08, a.sin() * (hz + 0.5));
        let d = v(r.range(-1.0, 1.0), 0.0, r.range(-1.0, 1.0)).normalize_or_zero();
        stick(mb, p - d * 0.6, p + d * 0.6, 0.07, 0.8);
    }
}

fn sack(mb: &mut Mb, p: Vec3, s: f32, yaw: f32) {
    let (a, b) = (hex(0xc9b48a), hex(0xeadcb8));
    mb.ellipsoid_r(p + v(0.0, 0.32 * s, 0.0), v(0.36 * s, 0.34 * s, 0.3 * s), Quat::from_rotation_y(yaw), mix(a, b, 0.5));
    mb.ellipsoid_g(p + v(0.0, 0.62 * s, 0.0), v(0.14 * s, 0.1 * s, 0.14 * s), a, b, 6, 3);
    mb.cyl(p + v(-0.13 * s, 0.56 * s, 0.0), p + v(0.13 * s, 0.56 * s, 0.0), 0.03 * s, 0.03 * s, hex(0x8a6a40), hex(0x8a6a40), 4);
}

fn crate_box(mb: &mut Mb, p: Vec3, s: f32, yaw: f32) {
    let q = Quat::from_rotation_y(yaw);
    mb.cuboid(p + v(0.0, s, 0.0), v(s, s, s), q, hex(0xa97a46), hex(0xd1a266));
    for k in [-1.0f32, 1.0] {
        mb.cuboid(p + q * v(0.0, s, k * (s + 0.01)), v(s * 0.95, 0.06, 0.02), q, hex(0x7d5430), hex(0x8f6038));
    }
}

fn log_stack(mb: &mut Mb, p: Vec3, len: f32, r: f32, rows: usize) {
    let mut k = 0;
    for row in 0..rows {
        let n = rows - row;
        for i in 0..n {
            let z = (i as f32 - (n as f32 - 1.0) * 0.5) * r * 2.05;
            let y = r + row as f32 * r * 1.75;
            k += 1;
            mb.log(p + v(-len * 0.5, y, z), p + v(len * 0.5, y, z), r, mix(hex(0x7a5230), hex(0x9a6e3e), (k % 3) as f32 * 0.4), hex(0xecd2a0), 8);
        }
    }
}

fn log_walls(mb: &mut Mb, hx: f32, hz: f32, layers: usize, door: f32) {
    let r = 0.16;
    for i in 0..layers {
        let y = 0.25 + r + i as f32 * r * 1.85;
        let k = (i % 3) as f32 * 0.35;
        let bark = mix(hex(0x7a5230), hex(0x9a6e3e), k);
        let end = hex(0xecd2a0);
        mb.log(v(-hx - 0.25, y, -hz), v(hx + 0.25, y, -hz), r, bark, end, 7);
        for s in [-1.0f32, 1.0] {
            mb.log(v(s * hx, y, -hz - 0.25), v(s * hx, y, hz + 0.25), r, bark, end, 7);
        }
        if i < layers.saturating_sub(2) && door > 0.0 {
            mb.log(v(-hx - 0.25, y, hz), v(-door, y, hz), r, bark, end, 7);
            mb.log(v(door, y, hz), v(hx + 0.25, y, hz), r, bark, end, 7);
        } else {
            mb.log(v(-hx - 0.25, y, hz), v(hx + 0.25, y, hz), r, bark, end, 7);
        }
    }
}

pub fn warehouse(mb: &mut Mb, stage: u8) {
    let (hx, hz) = (2.4f32, 2.0f32);
    mb.cuboid(v(0.0, 0.0, 0.0), v(hx + 0.35, 0.25, hz + 0.35), Quat::IDENTITY, hex(0x6f6b66), hex(0x9a968f));
    let layers = [0, 3, 6, 7][stage as usize];
    log_walls(mb, hx, hz, layers, 0.85);
    if stage < 3 {
        site_markers(mb, hx + 1.0, hz + 1.0, 0.0);
        return log_stack(mb, v(-hx - 0.4, 0.0, hz + 1.6), 2.4, 0.18, 3);
    }
    hex_roof_c(mb, hx + 0.6, hz + 0.6, 2.75, hex(0xa63f28), hex(0xd75c38), hex(0x8f3320));
    for s in [-1.0f32, 1.0] {
        mb.cuboid(v(0.42 * s, 1.1, hz + 0.05), v(0.4, 0.85, 0.06), Quat::IDENTITY, hex(0x6a4020), hex(0x8a5a30));
        mb.cuboid(v(0.42 * s, 1.1, hz + 0.12), v(0.3, 0.05, 0.03), Quat::from_rotation_z(0.7 * s), hex(0x5a3a20), hex(0x6a4528));
    }
    crate_box(mb, v(1.7, 0.25, hz + 1.0), 0.32, 0.2);
    crate_box(mb, v(1.75, 0.89, hz + 1.0), 0.26, -0.3);
    crate_box(mb, v(2.4, 0.25, hz + 0.7), 0.28, 0.6);
    sack(mb, v(-1.5, 0.25, hz + 0.9), 1.0, 0.3);
    sack(mb, v(-2.1, 0.25, hz + 0.7), 0.9, -0.5);
}

