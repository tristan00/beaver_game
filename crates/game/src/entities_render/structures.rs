use super::meshgen::{hex, mix, Mb, Rng};
use super::models::{mud_lump, stake, stick, stick_c, stick_pile};
use crate::palette::{self, pack};
use bevy::prelude::*;
use std::f32::consts::{FRAC_PI_2, TAU};

fn v(x: f32, y: f32, z: f32) -> Vec3 { Vec3::new(x, y, z) }

const DAM_MUD: u32 = pack(palette::DAM_MUD);
const CUT_WOOD: u32 = pack(palette::CUT_WOOD);

fn peeled() -> [f32; 3] { mix(hex(CUT_WOOD), hex(0xd8c29a), 0.4) }

pub const DAM_STEP: f32 = 0.25;

pub fn dam_cell(mb: &mut Mb, h: f32, len: f32, variant: u32, site: bool) {
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
        stake(mb, v(0.0, 0.0, -wb - 0.3), 1.0 + h, false);
        if variant % 2 == 0 {
            stick_pile(mb, v(0.0, 0.0, -wb - 0.9), 0.0, 4, 3 + variant);
        }
    }
}

pub fn dam_cell_mesh(steps: u8, diagonal: bool, variant: u32, site: bool) -> Mesh {
    let mut mb = Mb::new();
    let len = if diagonal { std::f32::consts::SQRT_2 } else { 1.0 } + 0.35;
    dam_cell(&mut mb, steps.max(1) as f32 * DAM_STEP, len, variant, site);
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

