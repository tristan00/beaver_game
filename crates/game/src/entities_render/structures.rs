use super::meshgen::{hex, mix, Mb, Rng};
use super::models::{hex_roof_c, mud_lump, site_markers, stake, stick, stick_c, stick_pile};
use crate::palette::{self, pack};
use bevy::prelude::*;
use std::f32::consts::{FRAC_PI_2, TAU};

fn v(x: f32, y: f32, z: f32) -> Vec3 { Vec3::new(x, y, z) }

const DAM_MUD: u32 = pack(palette::DAM_MUD);
const CUT_WOOD: u32 = pack(palette::CUT_WOOD);
const INK: u32 = pack(palette::INK);

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

pub const FOOTING_DEPTH: f32 = 4.2;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Footing {
    Mound(f32),
    Stone(f32),
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
        Footing::Stone(rad) => {
            let (s0, s1) = (hex(0x6f6b66), hex(0xb9afa8));
            mb.lathe(Mat4::IDENTITY, &[(0.0, -d), (rad * 1.15, -d), (rad * 1.05, -0.3), (rad, 0.05), (0.0, 0.05)], 16, s0, s1, false);
            let mut y = -0.5;
            while y > -d {
                mb.lathe(Mat4::IDENTITY, &[(rad * 1.06, y - 0.04), (rad * 1.09, y), (rad * 1.06, y + 0.04)], 16, mix(s0, hex(INK), 0.2), s0, true);
                y -= 0.55;
            }
            if dry {
                mb.lathe(Mat4::IDENTITY, &[(rad * 1.07, STAIN_Y - 0.1), (rad * 1.09, STAIN_Y), (rad * 1.07, STAIN_Y + 0.08)], 16, stain(), stain(), true);
                mb.lathe(Mat4::IDENTITY, &[(rad * 1.08, STAIN_Y - 1.4), (rad * 1.1, STAIN_Y - 0.2)], 16, mix(dried_weed(), hex(0x4e6a3a), 0.4), dried_weed(), false);
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

fn site_frame(mb: &mut Mb, hx: f32, hz: f32, stage: u8, seed: u32) {
    site_markers(mb, hx + 0.5, hz + 0.5, 0.0);
    stick_pile(mb, v(-hx - 0.2, 0.0, hz + 1.0), 0.4, 6, seed);
    mud_lump(mb, v(hx + 0.6, 0.12, hz + 0.8), 1.0);
    let wood = (hex(0x8a5a32), hex(0xb07840));
    if stage >= 1 {
        mb.cuboid(v(0.0, 0.0, 0.0), v(hx, 0.18, hz), Quat::IDENTITY, hex(0x6f6b66), hex(0x9a968f));
        let ph = if stage == 1 { 0.8 } else { 1.9 };
        for (x, z) in [(-hx, -hz), (hx, -hz), (hx, hz), (-hx, hz)] {
            mb.cyl(v(x * 0.9, 0.1, z * 0.9), v(x * 0.9, ph, z * 0.9), 0.1, 0.09, wood.0, wood.1, 6);
        }
    }
    if stage >= 2 {
        mb.cyl(v(-hx * 0.9, 1.9, -hz * 0.9), v(hx * 0.9, 1.9, -hz * 0.9), 0.08, 0.08, wood.0, wood.1, 5);
        mb.cyl(v(-hx * 0.9, 1.9, hz * 0.9), v(hx * 0.9, 1.9, hz * 0.9), 0.08, 0.08, wood.0, wood.1, 5);
        mb.cyl(v(-hx * 0.9, 2.6, 0.0), v(hx * 0.9, 2.6, 0.0), 0.08, 0.08, hex(0xd7b27a), hex(0xd7b27a), 5);
        for x in [-hx * 0.9, hx * 0.9] {
            for z in [-hz * 0.9, hz * 0.9] {
                mb.cyl(v(x, 1.9, z), v(x, 2.6, 0.0), 0.06, 0.05, hex(0xd7b27a), hex(0xd7b27a), 4);
            }
        }
    }
}

fn basket(mb: &mut Mb, p: Vec3, s: f32, fill: [f32; 3]) {
    let (a, b) = (hex(0xb98a52), hex(0xd9b47a));
    mb.lathe(Mat4::from_translation(p) * Mat4::from_scale(Vec3::splat(s)), &[(0.0, 0.0), (0.32, 0.0), (0.42, 0.35), (0.45, 0.42), (0.0, 0.42)], 10, a, b, false);
    mb.lathe(Mat4::from_translation(p) * Mat4::from_scale(Vec3::splat(s)), &[(0.38, 0.12), (0.43, 0.22)], 10, hex(0x8a5a32), hex(0x8a5a32), false);
    mb.ellipsoid_g(p + v(0.0, 0.42 * s, 0.0), v(0.36 * s, 0.14 * s, 0.36 * s), mix(fill, hex(INK), 0.2), fill, 8, 4);
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

fn sapling(mb: &mut Mb, p: Vec3, s: f32, leaf: (u32, u32)) {
    mb.ellipsoid_g(p + v(0.0, 0.04, 0.0), v(0.3 * s, 0.1, 0.3 * s), hex(0x4d3520), hex(0x6f5033), 8, 3);
    mb.cyl(p, p + v(0.0, 0.7 * s, 0.0), 0.04 * s, 0.03 * s, hex(0x7a5230), hex(0x8f6038), 5);
    mb.ellipsoid_g(p + v(0.0, 0.85 * s, 0.0), v(0.3 * s, 0.32 * s, 0.3 * s), hex(leaf.0), hex(leaf.1), 8, 5);
}

fn lean_to(mb: &mut Mb, cx: f32, cz: f32, w: f32) {
    let wood = (hex(0x8a5a32), hex(0xb07840));
    for x in [-w, w] {
        mb.cyl(v(cx + x, 0.0, cz + 0.6), v(cx + x, 1.7, cz + 0.6), 0.09, 0.08, wood.0, wood.1, 6);
        mb.cyl(v(cx + x, 0.0, cz - 0.6), v(cx + x, 0.9, cz - 0.6), 0.09, 0.08, wood.0, wood.1, 6);
    }
    let n = 7;
    for i in 0..n {
        let x = cx - w - 0.15 + (2.0 * w + 0.3) * (i as f32 + 0.5) / n as f32;
        let (a, b) = if i % 2 == 0 { (hex(0xb98650), hex(0xd1a266)) } else { (hex(0xa97a46), hex(0xc4965c)) };
        mb.hex(
            [
                v(x - 0.16, 0.85, cz - 0.85), v(x + 0.16, 0.85, cz - 0.85), v(x + 0.16, 1.72, cz + 0.85), v(x - 0.16, 1.72, cz + 0.85),
                v(x - 0.16, 0.93, cz - 0.85), v(x + 0.16, 0.93, cz - 0.85), v(x + 0.16, 1.8, cz + 0.85), v(x - 0.16, 1.8, cz + 0.85),
            ],
            a,
            b,
        );
    }
}

pub fn food_cache(mb: &mut Mb, stage: u8) {
    if stage < 3 {
        return site_frame(mb, 1.5, 1.5, stage, 501);
    }
    let mut r = Rng(501);
    mb.ellipsoid_g(v(0.0, 0.0, 0.0), v(1.55, 0.7, 1.45), hex(0x4d3520), hex(0x6f5033), 10, 5);
    for layer in 0..4 {
        let (rad, y, n) = (1.45 - layer as f32 * 0.33, 0.25 + layer as f32 * 0.24, 11 - layer * 2);
        for i in 0..n {
            let a = i as f32 / n as f32 * TAU + r.range(-0.25, 0.25);
            let c = v(a.cos() * rad, y, a.sin() * rad);
            let d = v(-a.sin(), r.range(-0.1, 0.2), a.cos()).normalize() * 0.7 + v(a.cos(), 0.0, a.sin()) * r.range(-0.4, 0.4);
            let l = r.range(0.9, 1.4);
            if r.next() < 0.3 {
                stick_c(mb, c - d * l * 0.5, c + d * l * 0.5, 0.065, peeled());
            } else {
                stick(mb, c - d * l * 0.5, c + d * l * 0.5, 0.07, r.next());
            }
        }
    }
    for i in 0..8 {
        let a = i as f32 * 2.4;
        let rr = r.range(0.3, 1.2);
        let p = v(a.cos() * rr, 1.05 - rr * 0.45, a.sin() * rr);
        mb.ellipsoid_g(p, v(0.32, 0.2, 0.28), hex(0x3f8f35), hex(0x8bd05a), 8, 4);
    }
    for i in 0..6 {
        let a = i as f32 * 1.9 + 0.4;
        mb.ellipsoid(v(a.cos() * 0.8, 1.0 - 0.2 * (i % 3) as f32, a.sin() * 0.8), v(0.09, 0.09, 0.09), hex(0xd9434a));
    }
    basket(mb, v(1.6, 0.0, 1.3), 1.0, hex(0x7bc653));
    basket(mb, v(-1.7, 0.0, 1.1), 0.8, hex(0xd9434a));
}

pub fn scent_mound(mb: &mut Mb, stage: u8) {
    if stage < 3 {
        stake(mb, v(0.6, 0.0, 0.6), 1.0, true);
        return mud_lump(mb, v(0.0, 0.1, 0.0), 0.5 + 0.3 * stage as f32);
    }
    mb.lathe(Mat4::IDENTITY, &[(0.0, -0.1), (1.0, -0.1), (0.78, 0.3), (0.35, 0.68), (0.0, 0.75)], 12, hex(0x4d3520), hex(DAM_MUD), true);
    mb.ellipsoid_g(v(0.0, 0.62, 0.0), v(0.35, 0.14, 0.35), hex(0x3a281a), hex(0x5d4228), 8, 4);
    stick(mb, v(0.05, 0.4, 0.0), v(0.15, 1.5, 0.05), 0.05, 0.4);
    mb.ellipsoid_g(v(0.22, 1.5, 0.08), v(0.16, 0.09, 0.12), hex(0x4aa042), hex(0x8ad25a), 7, 4);
    for (x, z) in [(0.9f32, 0.4f32), (-0.7, 0.75), (-0.5, -0.9)] {
        mb.ellipsoid_g(v(x, 0.05, z), v(0.15, 0.1, 0.13), hex(0x77777c), hex(0xb7b5b2), 6, 3);
    }
}

pub fn guard_post(mb: &mut Mb, stage: u8) {
    let wood = (hex(0x8a5a32), hex(0xb07840));
    let top = [0.9, 1.6, 2.4, 2.4][stage as usize];
    mud_lump(mb, v(0.0, 0.0, 0.0), 1.4);
    for (x, z) in [(-0.6f32, -0.6f32), (0.6, -0.6), (0.6, 0.6), (-0.6, 0.6)] {
        mb.cyl(v(x * 1.15, -0.1, z * 1.15), v(x, top + 0.75, z), 0.1, 0.08, wood.0, wood.1, 6);
    }
    if stage < 3 {
        stake(mb, v(1.1, 0.0, 1.1), 1.1, true);
        return stick_pile(mb, v(-1.2, 0.0, 1.0), 0.6, 5, 77);
    }
    mb.cuboid(v(0.0, top, 0.0), v(0.85, 0.07, 0.85), Quat::IDENTITY, hex(0xa97a46), hex(0xd1a266));
    for (a, b) in [((-0.6f32, -0.6f32), (0.6f32, -0.6f32)), ((0.6, -0.6), (0.6, 0.6)), ((-0.6, 0.6), (-0.6, -0.6))] {
        mb.cyl(v(a.0, top + 0.55, a.1), v(b.0, top + 0.55, b.1), 0.05, 0.05, wood.0, wood.1, 5);
    }
    for s in [-0.25f32, 0.25] {
        mb.cyl(v(s, -0.05, 1.5), v(s, top + 0.1, 0.85), 0.05, 0.05, wood.0, wood.1, 5);
    }
    for i in 1..6 {
        let t = i as f32 / 6.0;
        let p = v(0.0, -0.05 + (top + 0.15) * t, 1.5 - 0.65 * t);
        mb.cyl(p - v(0.25, 0.0, 0.0), p + v(0.25, 0.0, 0.0), 0.035, 0.035, hex(0xd7b27a), hex(0xd7b27a), 4);
    }
    mb.cyl(v(0.6, top, -0.6), v(0.6, top + 1.6, -0.6), 0.04, 0.035, wood.0, wood.1, 5);
    mb.cuboid(v(0.85, top + 1.42, -0.6), v(0.25, 0.14, 0.015), Quat::from_rotation_z(-0.1), hex(0xff7a1a), hex(0xffa040));
}

pub fn forager_hut(mb: &mut Mb, stage: u8) {
    if stage < 3 {
        return site_frame(mb, 1.4, 1.4, stage, 601);
    }
    let (straw0, straw1) = (hex(0xb59a52), hex(pack(palette::STRAW)));
    mb.lathe(Mat4::IDENTITY, &[(1.35, -0.1), (1.32, 1.1)], 14, hex(0x7c5530), hex(0x9a7040), false);
    for k in 0..4 {
        let y = 0.15 + k as f32 * 0.27;
        mb.lathe(Mat4::IDENTITY, &[(1.37, y), (1.39, y + 0.1), (1.37, y + 0.18)], 14, hex(0x6a4627), hex(0x8d6537), true);
    }
    mb.lathe(Mat4::IDENTITY, &[(0.0, 0.95), (1.85, 0.95), (1.8, 1.08), (1.1, 1.75), (0.35, 2.35), (0.0, 2.5)], 14, straw0, straw1, true);
    for k in 0..3 {
        let y = 1.25 + k as f32 * 0.35;
        let rr = 1.55 - k as f32 * 0.4;
        mb.lathe(Mat4::IDENTITY, &[(rr + 0.06, y - 0.05), (rr - 0.05, y + 0.07)], 14, mix(straw0, hex(INK), 0.15), straw0, false);
    }
    mb.ellipsoid_g(v(0.0, 2.52, 0.0), v(0.18, 0.18, 0.18), straw0, straw1, 8, 4);
    mb.ellipsoid(v(0.0, 0.5, 1.33), v(0.42, 0.55, 0.12), hex(0x140c09));
    basket(mb, v(1.45, 0.0, 1.35), 0.9, hex(0x7bc653));
    basket(mb, v(-1.5, 0.0, 1.2), 0.75, hex(0xd9434a));
    basket(mb, v(-1.85, 0.0, 0.3), 0.7, hex(0xf2c94c));
}

pub fn mud_pit(mb: &mut Mb, stage: u8) {
    let mut r = Rng(701);
    let n = 10;
    for i in 0..n {
        let a = i as f32 / n as f32 * TAU;
        mud_lump(mb, v(a.cos() * 1.5, 0.05, a.sin() * 1.4), r.range(0.8, 1.1));
    }
    if stage < 3 {
        stake(mb, v(1.9, 0.0, 1.8), 1.1, true);
        stake(mb, v(-1.9, 0.0, -1.8), 1.1, true);
        return mb.disc(v(0.0, 0.06, 0.0), 1.2, 1.1, hex(0x5d4228), 14);
    }
    mb.disc(v(0.0, 0.1, 0.0), 1.3, 1.2, hex(0x3a281a), 16);
    mb.disc(v(-0.3, 0.11, -0.25), 0.45, 0.2, hex(0x6a5038), 10);
    for (p, s) in [(v(1.95, 0.0, 0.9), 1.0f32), (v(2.3, 0.0, 0.6), 1.0), (v(2.05, 0.0, 0.3), 1.0), (v(2.12, 0.3, 0.6), 0.95)] {
        mb.ellipsoid_g(p + v(0.0, 0.18 * s, 0.0), v(0.2 * s, 0.18 * s, 0.2 * s), hex(0x5d4228), hex(DAM_MUD), 8, 4);
    }
    mb.cyl(v(0.4, 0.0, 0.2), v(0.75, 1.3, 0.45), 0.04, 0.04, hex(0xb98a52), hex(0xd1a266), 5);
    mb.cuboid(v(0.38, 0.12, 0.18), v(0.14, 0.22, 0.03), Quat::from_rotation_y(0.6) * Quat::from_rotation_z(0.25), hex(0xa97a46), hex(0xc4965c));
}

pub fn tree_nursery(mb: &mut Mb, stage: u8) {
    let wood = (hex(0x8a5a32), hex(0xb07840));
    let (hx, hz) = (1.8f32, 1.8f32);
    let corners = [v(-hx, 0.0, -hz), v(hx, 0.0, -hz), v(hx, 0.0, hz), v(-hx, 0.0, hz)];
    for (i, c) in corners.iter().enumerate() {
        let nxt = corners[(i + 1) % 4];
        for k in 0..4 {
            let p = c.lerp(nxt, k as f32 / 4.0);
            mb.cyl(p - v(0.0, 0.1, 0.0), p + v(0.0, 0.7, 0.0), 0.05, 0.045, wood.0, wood.1, 5);
        }
        if stage >= 2 {
            for y in [0.3f32, 0.55] {
                mb.cyl(*c + v(0.0, y, 0.0), nxt + v(0.0, y, 0.0), 0.035, 0.035, hex(0x9a7040), hex(0x9a7040), 4);
            }
        }
    }
    let rows = if stage == 3 { 3 } else { stage as usize };
    for i in 0..rows {
        for j in 0..2 {
            let p = v(-1.0 + i as f32 * 1.0, 0.0, -0.7 + j as f32 * 1.4);
            sapling(mb, p, 0.75 + 0.15 * ((i + j) % 2) as f32, (0x5cb347, 0x9be066));
        }
    }
    if stage < 3 {
        return stick_pile(mb, v(-1.9, 0.0, 2.4), 0.2, 5, 88);
    }
    mb.lathe(Mat4::from_translation(v(2.2, 0.0, 1.5)), &[(0.0, 0.0), (0.22, 0.0), (0.26, 0.4), (0.0, 0.4)], 10, hex(0x7d5430), hex(0xa97a46), false);
    mb.disc(v(2.2, 0.36, 1.5), 0.22, 0.22, hex(0x6fd3e0), 10);
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

pub fn storehouse(mb: &mut Mb, stage: u8) {
    let (hx, hz) = (3.4f32, 2.3f32);
    let wood = (hex(0x8a5a32), hex(0xb07840));
    mb.cuboid(v(0.0, 0.0, 0.0), v(hx + 0.3, 0.25, hz + 0.3), Quat::IDENTITY, hex(0x6f6b66), hex(0x9a968f));
    let ph = [0.0, 1.0, 2.3, 2.3][stage as usize];
    for i in 0..4 {
        let x = -hx + i as f32 * hx * 2.0 / 3.0;
        for z in [-hz, hz] {
            if ph > 0.0 {
                mb.cyl(v(x, 0.2, z), v(x, 0.2 + ph, z), 0.13, 0.11, wood.0, wood.1, 6);
            }
        }
    }
    if stage < 3 {
        site_markers(mb, hx + 0.8, hz + 0.8, 0.0);
        return stick_pile(mb, v(-hx, 0.0, hz + 1.4), 0.2, 8, 141);
    }
    for i in 0..12 {
        let x = -hx + (i as f32 + 0.5) * hx * 2.0 / 12.0;
        let (a, b) = if i % 2 == 0 { (hex(0xb9814a), hex(0xd49a5c)) } else { (hex(0xa97445), hex(0xc48c52)) };
        mb.cuboid(v(x, 1.35, -hz), v(hx / 12.0 - 0.01, 1.1, 0.06), Quat::IDENTITY, a, b);
    }
    hex_roof_c(mb, hx + 0.5, hz + 0.6, 2.5, hex(0x8a5a32), hex(0xb98650), hex(0x6a4528));
    log_stack(mb, v(-2.3, 0.25, -0.6), 1.9, 0.2, 3);
    for (i, (x, y)) in [(0.0f32, 0.0f32), (0.75, 0.0), (0.38, 0.5)].into_iter().enumerate() {
        mb.cuboid(v(x - 0.3, 0.5 + y, -0.9), v(0.34, 0.24, 0.3), Quat::from_rotation_y(0.05 * i as f32), hex(0x8c8290), hex(0xb9afa8));
    }
    for (x, z) in [(1.8f32, -1.0f32), (2.4, -0.6), (2.0, -0.2)] {
        sack(mb, v(x, 0.25, z), 0.85, x);
    }
    for (x, z) in [(1.0f32, 0.9f32), (1.5, 1.2)] {
        mb.lathe(Mat4::from_translation(v(x, 0.25, z)), &[(0.0, 0.0), (0.2, 0.0), (0.32, 0.3), (0.18, 0.62), (0.22, 0.7), (0.0, 0.7)], 10, hex(0xb0603a), hex(0xd9844e), true);
    }
}

pub fn logging_camp(mb: &mut Mb, stage: u8) {
    if stage < 3 {
        site_frame(mb, 1.4, 1.2, stage, 801);
        return log_stack(mb, v(0.0, 0.0, 1.9), 1.8, 0.16, stage as usize + 1);
    }
    lean_to(mb, 0.0, -1.0, 1.4);
    log_stack(mb, v(-0.2, 0.0, 0.9), 2.4, 0.22, 4);
    mb.lathe(Mat4::from_translation(v(1.6, 0.0, 1.7)), &[(0.0, 0.0), (0.42, 0.0), (0.36, 0.1), (0.34, 0.5)], 10, hex(0x7a5230), hex(0x7a5230), true);
    mb.lathe(Mat4::from_translation(v(1.6, 0.0, 1.7)), &[(0.34, 0.5), (0.0, 0.52)], 10, hex(0xeed3a0), hex(0xf6e2b8), false);
    let mut r = Rng(811);
    for _ in 0..9 {
        let p = v(1.6 + r.range(-0.9, 0.9), 0.04, 1.7 + r.range(-0.8, 0.8));
        mb.cuboid(p, v(0.08, 0.02, 0.05), Quat::from_rotation_y(r.range(0.0, TAU)), hex(0xe9cf9e), hex(0xf6e2b8));
    }
    stick_pile(mb, v(-1.6, 0.0, -0.1), 0.9, 5, 812);
}

pub fn quarry_camp(mb: &mut Mb, stage: u8) {
    if stage < 3 {
        site_frame(mb, 1.4, 1.2, stage, 901);
        return mb.ellipsoid_g(v(0.0, 0.1, 2.0), v(0.5, 0.35, 0.45), hex(0x6c6c72), hex(0xb7b5b2), 7, 4);
    }
    lean_to(mb, 0.4, -1.1, 1.2);
    let rocks = [(-1.2f32, 0.8f32, 0.55f32), (-0.6, 1.3, 0.45), (-1.5, 1.5, 0.4), (-0.9, 0.9, 0.5)];
    for (i, (x, z, s)) in rocks.into_iter().enumerate() {
        mb.ellipsoid_g(v(x, s * 0.55, z), v(s, s * 0.8, s * 0.9), hex(0x6c6c72), hex(0xb7b5b2), 7, 4);
        mb.ellipsoid_r(v(x + 0.1, s * 1.05, z), v(s * 0.5, s * 0.25, s * 0.45), Quat::from_rotation_y(i as f32), hex(0xc9c7c2));
    }
    for (i, (x, y, z)) in [(0.7f32, 0.0f32, 0.9f32), (1.35, 0.0, 0.9), (1.0, 0.42, 0.9), (0.75, 0.0, 1.5)].into_iter().enumerate() {
        mb.cuboid(v(x, y + 0.21, z), v(0.3, 0.2, 0.26), Quat::from_rotation_y(0.08 * i as f32), hex(0x8c8290), hex(0xc4bcb4));
    }
    let (cx, cz) = (1.7f32, -0.2f32);
    mb.cuboid(v(cx, 0.65, cz), v(0.4, 0.22, 0.6), Quat::IDENTITY, hex(0xa97a46), hex(0xd1a266));
    for s in [-1.0f32, 1.0] {
        mb.lathe(Mat4::from_translation(v(cx + 0.46 * s, 0.38, cz)) * Mat4::from_rotation_z(FRAC_PI_2), &[(0.0, -0.05), (0.36, -0.05), (0.38, 0.0), (0.36, 0.05), (0.0, 0.05)], 10, hex(0x6a4528), hex(0x8a5a32), false);
    }
    mb.cyl(v(cx, 0.6, cz + 0.6), v(cx, 0.35, cz + 1.5), 0.05, 0.05, hex(0x8a5a32), hex(0x8a5a32), 5);
    mb.ellipsoid_g(v(cx, 0.95, cz), v(0.3, 0.18, 0.4), hex(0x77777c), hex(0xb2b2b6), 7, 4);
}

pub fn forester_lodge(mb: &mut Mb, stage: u8) {
    let (hx, hz) = (1.45f32, 1.2f32);
    if stage < 3 {
        return site_frame(mb, hx, hz, stage, 1001);
    }
    mb.cuboid(v(0.0, 0.0, 0.0), v(hx + 0.12, 0.22, hz + 0.12), Quat::IDENTITY, hex(0x6f6b66), hex(0x9a968f));
    mb.cuboid(v(0.0, 0.95, 0.0), v(hx, 0.75, hz), Quat::IDENTITY, hex(0xb9814a), hex(0xd49a5c));
    for i in 1..6 {
        let x = -hx + i as f32 * hx * 2.0 / 6.0;
        mb.cuboid(v(x, 0.95, hz + 0.01), v(0.02, 0.75, 0.01), Quat::IDENTITY, hex(0x7d5430), hex(0x8f6038));
    }
    hex_roof_c(mb, hx + 0.3, hz + 0.35, 1.7, hex(0x3f7f3a), hex(0x6ab35a), hex(0x2f6a2a));
    mb.cuboid(v(-0.4, 0.85, hz + 0.03), v(0.36, 0.6, 0.06), Quat::IDENTITY, hex(0x6a4020), hex(0x8a5a30));
    mb.cuboid(v(0.75, 1.15, hz + 0.03), v(0.28, 0.24, 0.06), Quat::IDENTITY, hex(0x6a4020), hex(0x8a5a30));
    mb.cuboid(v(0.75, 1.15, hz + 0.06), v(0.2, 0.17, 0.05), Quat::IDENTITY, hex(0xa8dcf0), hex(0xd6f2ff));
    for (x, z, s) in [(0.6f32, 1.85f32, 0.7f32), (1.2, 1.95, 0.6), (-1.35, 1.8, 0.8)] {
        mb.lathe(Mat4::from_translation(v(x, 0.0, z)), &[(0.0, 0.0), (0.16, 0.0), (0.22, 0.3), (0.0, 0.3)], 8, hex(0xb0603a), hex(0xd9844e), false);
        sapling(mb, v(x, 0.25, z), s, (0x4aa042, 0x8ad25a));
    }
    let c = v(-0.8, 0.0, 1.9);
    mb.lathe(Mat4::from_translation(c), &[(0.0, 0.0), (0.2, 0.0), (0.2, 0.32), (0.0, 0.32)], 10, hex(0x8a9aa8), hex(0xb8c4ce), false);
    mb.cyl(c + v(0.15, 0.1, 0.0), c + v(0.45, 0.4, 0.0), 0.03, 0.02, hex(0x8a9aa8), hex(0xb8c4ce), 5);
}

pub fn mill(mb: &mut Mb, stage: u8) {
    let (s0, s1) = (hex(0x8c8290), hex(0xc4bcb4));
    let wall = [0.0, 0.9, 1.9, 1.9][stage as usize];
    if wall > 0.0 {
        mb.lathe(Mat4::IDENTITY, &[(0.0, -0.1), (1.85, -0.1), (1.7, 0.2), (1.6, wall), (0.0, wall)], 14, s0, s1, false);
        let mut y = 0.45;
        while y < wall {
            mb.lathe(Mat4::IDENTITY, &[(1.63, y - 0.03), (1.66, y), (1.63, y + 0.03)], 14, mix(s0, hex(INK), 0.2), s0, true);
            y += 0.45;
        }
    }
    if stage < 3 {
        site_markers(mb, 2.4, 2.4, 0.0);
        return mb.ellipsoid_g(v(2.2, 0.15, 1.6), v(0.5, 0.35, 0.45), hex(0x6c6c72), hex(0xb7b5b2), 7, 4);
    }
    mb.lathe(Mat4::IDENTITY, &[(1.55, 1.9), (1.45, 3.0), (0.0, 3.0)], 14, hex(0xb9814a), hex(0xd49a5c), false);
    for k in 0..8 {
        let a = k as f32 / 8.0 * TAU;
        mb.cyl(v(a.cos() * 1.5, 1.9, a.sin() * 1.5), v(a.cos() * 1.46, 3.0, a.sin() * 1.46), 0.05, 0.05, hex(0x7d5430), hex(0x8f6038), 4);
    }
    mb.lathe(Mat4::IDENTITY, &[(0.0, 2.85), (1.95, 2.85), (1.98, 2.95), (1.85, 3.08), (1.0, 4.05), (0.25, 4.85), (0.0, 4.95)], 14, hex(0xa63f28), hex(0xd75c38), false);
    for k in 1..4 {
        let t = k as f32 / 4.0;
        let (rr, y) = (1.85 + (0.25 - 1.85) * t, 3.08 + (4.85 - 3.08) * t);
        mb.lathe(Mat4::IDENTITY, &[(rr + 0.05, y - 0.04), (rr - 0.02, y + 0.04)], 14, hex(0x8f3320), hex(0x8f3320), false);
    }
    mb.cyl(v(0.0, 4.8, 0.0), v(0.0, 5.4, 0.0), 0.04, 0.03, hex(0x555a60), hex(0x555a60), 5);
    mb.ellipsoid(v(0.0, 5.4, 0.0), v(0.1, 0.1, 0.1), hex(0xf2c94c));
    mb.cuboid(v(0.0, 0.85, 1.62), v(0.45, 0.7, 0.08), Quat::IDENTITY, hex(0x6a4020), hex(0x8a5a30));
    mb.cuboid(v(0.0, 2.4, 1.5), v(0.25, 0.22, 0.06), Quat::IDENTITY, hex(0xa8dcf0), hex(0xd6f2ff));
    let ms = Mat4::from_translation(v(1.25, 0.65, 2.0)) * Mat4::from_rotation_x(FRAC_PI_2 - 0.25);
    mb.lathe(ms, &[(0.0, -0.13), (0.62, -0.13), (0.66, 0.0), (0.62, 0.13), (0.12, 0.13), (0.12, 0.2), (0.0, 0.2)], 14, s0, s1, false);
    sack(mb, v(-1.0, 0.0, 2.0), 1.0, 0.4);
    sack(mb, v(-1.55, 0.0, 1.7), 0.9, -0.3);
    sack(mb, v(-1.25, 0.55, 1.85), 0.8, 1.0);
}

pub fn lock(mb: &mut Mb, stage: u8) {
    let wood = (hex(0x8a5a32), hex(0xb07840));
    let wall = [0.6, 1.2, 1.9, 1.9][stage as usize];
    for s in [-1.0f32, 1.0] {
        mb.cuboid(v(s * 1.75, wall * 0.5 - 0.6, 0.0), v(0.35, wall * 0.5 + 0.6, 1.1), Quat::IDENTITY, hex(0x6f6b66), hex(0x9a968f));
        let mut y = -0.3;
        while y < wall - 0.1 {
            mb.log(v(s * 1.42, y + 0.15, -1.15), v(s * 1.42, y + 0.15, 1.15), 0.15, hex(0x7a5230), hex(0xecd2a0), 7);
            y += 0.3;
        }
        for z in [-1.05f32, 1.05] {
            mb.cyl(v(s * 1.4, -0.8, z), v(s * 1.4, wall + 0.5, z), 0.13, 0.12, wood.0, wood.1, 6);
            mb.ellipsoid(v(s * 1.4, wall + 0.52, z), v(0.15, 0.08, 0.15), hex(0x8f6038));
        }
    }
    if stage < 3 {
        stake(mb, v(2.4, 0.0, 1.4), 1.2, true);
        return stake(mb, v(-2.4, 0.0, -1.4), 1.2, true);
    }
    for z in [-1.05f32, 1.05] {
        for i in 0..5 {
            let x = -1.2 + (i as f32 + 0.5) * 2.4 / 5.0;
            let (a, b) = if i % 2 == 0 { (hex(0x8a5a32), hex(0xb07840)) } else { (hex(0x7d5430), hex(0x9e6c3c)) };
            mb.cuboid(v(x, wall * 0.5 - 0.4, z), v(1.2 / 5.0 - 0.01, wall * 0.5 + 0.35, 0.08), Quat::IDENTITY, a, b);
        }
        mb.cyl(v(-1.25, wall - 0.05, z + 0.1), v(1.25, 0.0, z + 0.1), 0.05, 0.05, hex(0x6a4528), hex(0x6a4528), 4);
    }
    for i in 0..7 {
        let x = -1.95 + (i as f32 + 0.5) * 3.9 / 7.0;
        mb.cuboid(v(x, wall + 0.62, 1.05), v(3.9 / 14.0 - 0.02, 0.05, 0.28), Quat::IDENTITY, hex(0xb98650), hex(0xd1a266));
    }
    let w = v(1.75, wall + 0.1, 0.0);
    mb.cyl(w - v(0.0, 0.0, 0.35), w + v(0.0, 0.0, 0.35), 0.18, 0.18, hex(0x6a4528), hex(0x8a5a32), 8);
    mb.cyl(w + v(0.0, 0.0, 0.35), w + v(0.0, 0.45, 0.6), 0.04, 0.04, hex(0x8a5a32), hex(0x8a5a32), 5);
    mb.cyl(w - v(0.0, 0.0, 0.2), v(1.3, 0.2, -1.0), 0.02, 0.02, hex(0xe9d9aa), hex(0xe9d9aa), 4);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dir_of(cells: &[(i32, i32)]) -> u8 { dam_dir(|dx, dy| cells.contains(&(dx, dy))) }

    #[test]
    fn dam_direction_follows_the_line() {
        assert_eq!(dir_of(&[(-1, 0), (1, 0)]), 0);
        assert_eq!(dir_of(&[(0, -1), (0, 1)]), 2);
        assert_eq!(dir_of(&[(-1, -1), (1, 1)]), 1);
        assert_eq!(dir_of(&[(1, -1), (-1, 1)]), 3);
        assert_eq!(dir_of(&[(1, 0)]), 0, "a line end follows its one neighbour");
        assert_eq!(dir_of(&[(0, 1)]), 2);
        assert_eq!(dir_of(&[]), 0, "a lone cell runs along x");
    }
}
