use super::meshgen::{hex, mix, Mb, Rng};
use super::models::{branch, col, mud_lump, site_markers, stake, stick, stick_c, stick_pile};
use crate::palette;
use bevy::prelude::*;
use std::f32::consts::{FRAC_PI_2, TAU};
use super::meshgen::v;

fn peeled() -> [f32; 3] { col(palette::PEELED) }

pub const DAM_STEP: f32 = 0.25;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DamStage {
    Staked,
    Rising,
    Done,
}

const WALL_CREST: f32 = 0.22;
const WALL_BURY: f32 = 0.45;

fn wall_feet(h: f32) -> (f32, f32) {
    (0.28 + 0.55 * h, 0.3 + 0.95 * h)
}

fn wall_face(h: f32, t: f32, base: f32) -> f32 {
    base + (WALL_CREST - base) * t + 0.12 * h.min(1.5) * t * (1.0 - t)
}

pub fn dam_cell(mb: &mut Mb, h: f32, len: f32, variant: u32, stage: DamStage) {
    if stage == DamStage::Staked {
        stake(mb, v(0.0, 0.0, 0.0), 0.9, true);
        mud_lump(mb, v(0.0, 0.0, 0.0), 0.35);
        return;
    }
    let site = stage == DamStage::Rising;
    let hl = len * 0.5;
    let h = h.max(0.1);
    let wt = WALL_CREST;
    let (up_b, down_b) = wall_feet(h);
    let bury = WALL_BURY;
    let face = |t: f32, base: f32| wall_face(h, t, base);
    let up = |t: f32| face(t, up_b);
    let down = |t: f32| face(t, down_b);
    let (mud_lo, mud_hi) = (col(palette::MUD_WET), col(palette::DAM_MUD));
    let ts = [0.0f32, 0.3, 0.6, 0.85];
    let mut prof: Vec<(f32, f32)> = vec![(-(down_b + bury * 0.6), -bury)];
    for t in ts {
        prof.push((-down(t), t * h));
    }
    prof.extend([(-wt * 0.7, h + 0.02), (0.0, h + 0.05), (wt * 0.7, h + 0.02)]);
    for t in ts.iter().rev() {
        prof.push((up(*t), *t * h));
    }
    prof.push((up_b + bury * 0.3, -bury));
    mb.extrude_x(&prof, -hl, hl, mud_lo, mix(mud_lo, mud_hi, 0.85));
    let mut r = Rng(9001 + variant * 7717 + (h * 8.0) as u32 * 31);
    let dark_bark = |r: &mut Rng| mix(col(palette::BARK_DARK), col(palette::BARK), r.next());
    let n = (len * h * 16.0) as usize + 6;
    for _ in 0..n {
        let t = r.range(0.04, 0.95);
        let slope = (down((t + 0.05).min(1.0)) - down((t - 0.05).max(0.0))) / (0.1 * h);
        let upv = v(0.0, 1.0, -slope).normalize();
        let c = v(r.range(-hl, hl), t * h, -(down(t) + 0.035));
        let across = r.next() < 0.3;
        let j = if across { FRAC_PI_2 + r.range(-0.3, 0.3) } else { r.range(-0.5, 0.5) };
        let d = (upv * j.cos() + Vec3::X * j.sin()).normalize();
        let l = r.range(0.5, 1.1) * (0.55 + 0.45 * (h / 1.5).min(1.0));
        let rad = r.range(0.035, 0.065);
        let pick = r.next();
        if pick < 0.12 {
            stick_c(mb, c - d * l * 0.5, c + d * l * 0.5, rad, peeled());
        } else if pick < 0.45 {
            let colr = dark_bark(&mut r);
            stick_c(mb, c - d * l * 0.5, c + d * l * 0.5, rad, colr);
        } else {
            stick(mb, c - d * l * 0.5, c + d * l * 0.5, rad, r.range(0.0, 0.6));
        }
    }
    for _ in 0..((len * h * 3.0) as usize + 1) {
        let t = r.range(0.2, 0.9);
        let c = v(r.range(-hl, hl), t * h, up(t) + 0.01);
        let d = v(r.range(-1.0, 1.0), r.range(-0.3, 0.3), 0.15).normalize();
        stick(mb, c - d * 0.25, c + d * 0.25, 0.04, r.next());
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
    mb.ellipsoid_g(v(r.range(-hl * 0.5, hl * 0.5), h + 0.03, 0.0), v(r.range(0.3, 0.45), 0.08, wt * 1.2), mud_lo, mud_hi, 8, 4);
    if variant % 3 == 0 && !site {
        let c = v(r.range(-hl * 0.5, hl * 0.5), h + 0.05, r.range(-0.1, 0.1));
        stick(mb, c, c + v(0.3, 0.18, 0.15), 0.03, 0.5);
        for (o, k) in [(v(0.3, 0.2, 0.15), 1.0f32), (v(0.18, 0.15, 0.25), 0.8), (v(0.36, 0.16, 0.02), 0.75)] {
            mb.ellipsoid_r(c + o, v(0.13 * k, 0.035, 0.07 * k), Quat::from_rotation_y(o.x * 4.0), col(palette::LEAF));
        }
    }
    for (s, base) in [(-1.0f32, down_b), (1.0, up_b)] {
        for _ in 0..2 {
            let p = v(r.range(-hl, hl), 0.06, s * (base + r.range(0.0, 0.2)));
            let k = r.range(0.12, 0.22);
            mb.ellipsoid_g(p, v(k * 1.2, k * 0.75, k), hex(0x77777c), hex(0xb7b5b2), 7, 4);
        }
    }
    if site {
        stake(mb, v(0.0, 0.0, -down_b - 0.3), 0.9, true);
        stick_pile(mb, v(0.0, 0.0, -down_b - 0.8), 0.0, 4, 3 + variant);
    }
}

pub fn dam_cell_mesh(steps: u8, len: f32, variant: u32, stage: DamStage) -> Mesh {
    let mut mb = Mb::new();
    dam_cell(&mut mb, steps.max(1) as f32 * DAM_STEP, len, variant, stage);
    mb.build()
}

pub fn faced_dam_cell_mesh(steps: u8, len: f32, variant: u32, stage: DamStage) -> Mesh {
    let mut mb = Mb::new();
    let h = steps.max(1) as f32 * DAM_STEP;
    dam_cell(&mut mb, h, len, variant, stage);
    if stage != DamStage::Staked {
        pebble_face(&mut mb, h, len, variant);
    }
    mb.build()
}

pub fn pebble_face(mb: &mut Mb, h: f32, len: f32, variant: u32) {
    let h = h.max(0.1);
    let hl = len * 0.5;
    let (_, down_b) = wall_feet(h);
    let top = if h < 0.4 { 0.7 } else { 0.82 };
    let mut line = vec![Vec2::new(down_b + WALL_BURY * 0.6, -WALL_BURY)];
    for k in 0..=12 {
        let t = top * k as f32 / 12.0;
        line.push(Vec2::new(wall_face(h, t, down_b), t * h));
    }
    let mut cum = vec![0.0f32; line.len()];
    for i in 1..line.len() {
        cum[i] = cum[i - 1] + line[i].distance(line[i - 1]);
    }
    let end = cum[line.len() - 1];
    let at = |s: f32| {
        let i = cum.partition_point(|&c| c < s).clamp(1, line.len() - 1);
        let seg = (cum[i] - cum[i - 1]).max(1e-6);
        (line[i - 1].lerp(line[i], ((s - cum[i - 1]) / seg).clamp(0.0, 1.0)), (line[i] - line[i - 1]) / seg)
    };
    let mut r = Rng(6007 + variant * 3301 + (h * 8.0) as u32 * 53);
    let size = 0.085 + 0.02 * h.min(3.0);
    let mud = col(palette::MUD_WET);
    let mut s = cum[1] * 0.45;
    let mut course = 0;
    while s < end {
        let up = s / end;
        let k = size * (1.3 - 0.45 * up);
        let (_, d) = at(s);
        let n = v(0.0, -d.x, -d.y).normalize_or(Vec3::Y);
        let across = ((2.0 * hl) / (2.2 * k)).ceil().max(1.0) as usize;
        let step = 2.0 * hl / across as f32;
        let (count, shift) = if course % 2 == 0 { (across, 0.5) } else { (across + 1, 0.0) };
        for i in 0..count {
            if course >= 2 && r.next() < 0.07 {
                continue;
            }
            let x = -hl + (i as f32 + shift + r.range(-0.18, 0.18)) * step;
            let (sp, _) = at(s + r.range(-0.15, 0.15) * k);
            let thick = k * r.range(0.5, 0.7);
            let c = v(x, sp.y, -sp.x) + n * thick * 0.35;
            let rot = Quat::from_rotation_arc(Vec3::Y, n) * Quat::from_rotation_y(r.range(-0.4, 0.4)) * Quat::from_rotation_z(r.range(-0.15, 0.15));
            let scale = v(k * r.range(1.15, 1.45), thick, k * r.range(0.9, 1.1));
            let colour = mix(super::deposits::pebble_colour(&mut r, false), mud, 0.25 * (1.0 - up) + 0.05);
            mb.ellipsoid_m(Mat4::from_rotation_translation(rot, c) * Mat4::from_scale(scale), 6, 3, mix(colour, hex(0x2E2A26), 0.35), colour);
        }
        s += k * 1.75;
        course += 1;
    }
}

pub const FOOTING_DEPTH: f32 = 4.2;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Footing {
    Mound(f32),
    Piles(f32, f32),
    Ring(f32),
}

const STAIN_Y: f32 = -0.42;
fn stain() -> [f32; 3] { hex(0xd9d0bf) }
fn dried_weed() -> [f32; 3] { hex(0x8a7a4a) }

pub fn footing(mb: &mut Mb, f: Footing, dry: bool) {
    let d = FOOTING_DEPTH;
    let mut r = Rng(4242);
    match f {
        Footing::Mound(rad) => {
            let (lo, hi) = (col(palette::MUD_WET), col(palette::DAM_MUD));
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
        Footing::Ring(rad) => {
            let (lo, hi) = (col(palette::MUD_WET), col(palette::DAM_MUD));
            mb.lathe(Mat4::IDENTITY, &[(rad * 0.85, -d), (rad * 1.2, -d), (rad * 1.08, -0.3), (rad * 1.02, 0.0), (rad * 0.88, 0.0), (rad * 0.85, -d)], 20, lo, hi, false);
        }
        Footing::Piles(hx, hz) => {
            let logs = ((hz * 2.0 + 0.4) / 0.34) as usize;
            for i in 0..logs {
                let z = -hz - 0.2 + (2.0 * hz + 0.4) * (i as f32 + 0.5) / logs as f32;
                let j = r.range(-0.15, 0.15);
                stick(mb, v(-hx - 0.3 + j, -0.12, z), v(hx + 0.3 + j, -0.1, z + r.range(-0.05, 0.05)), 0.17, r.next());
            }
            for s in [-1.0f32, 1.0] {
                stick(mb, v(s * (hx - 0.2), -0.3, -hz - 0.35), v(s * (hx - 0.2), -0.3, hz + 0.35), 0.15, 0.2);
            }
            let nx = ((hx * 2.0 / 1.6).ceil() as usize).max(1);
            let nz = ((hz * 2.0 / 1.6).ceil() as usize).max(1);
            let mut posts = Vec::new();
            for i in 0..=nx {
                for j in 0..=nz {
                    if i == 0 || i == nx || j == 0 || j == nz {
                        posts.push(v(-hx + 2.0 * hx * i as f32 / nx as f32, 0.0, -hz + 2.0 * hz * j as f32 / nz as f32));
                    }
                }
            }
            for p in &posts {
                let lean = v(r.range(-0.15, 0.15), 0.0, r.range(-0.15, 0.15));
                stick(mb, *p + v(0.0, -d, 0.0) + lean, *p + v(0.0, 0.0, 0.0), 0.14, r.next());
                mud_lump(mb, *p + v(0.0, -0.45, 0.0), 0.55);
                if dry {
                    mb.cyl(*p + v(0.0, STAIN_Y - 0.1, 0.0), *p + v(0.0, STAIN_Y + 0.06, 0.0), 0.165, 0.165, stain(), stain(), 7);
                    mb.cyl(*p + v(0.0, STAIN_Y - 0.12, 0.0), *p + v(0.0, STAIN_Y - 1.1, 0.0), 0.17, 0.155, mix(dried_weed(), hex(0x4e6a3a), 0.35), dried_weed(), 7);
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

pub fn decay(mb: &mut Mb, hx: f32, hz: f32) {
    mb.displace(0, |p| if p.y > 0.2 { v(p.x * (1.0 + 0.04 * p.y), 0.2 + (p.y - 0.2) * 0.82, p.z * (1.0 + 0.04 * p.y)) } else { p });
    mb.weather(col(palette::DECAY), 0.4);
    let mut r = Rng(777);
    let grass = (hex(0x9a8a52), hex(0xd2bf7c));
    for i in 0..12 {
        let a = i as f32 / 12.0 * TAU + r.range(-0.2, 0.2);
        let p = v(a.cos() * (hx + 0.1), 0.05, a.sin() * (hz + 0.1));
        for k in 0..3 {
            let b = k as f32 * 2.1 + a;
            mb.cyl(p, p + v(b.cos() * 0.15, r.range(0.35, 0.6), b.sin() * 0.15), 0.05, 0.01, grass.0, grass.1, 3);
        }
    }
    for _ in 0..7 {
        let a = r.range(0.0, TAU);
        let p = v(a.cos() * (hx + r.range(0.2, 0.9)), 0.08, a.sin() * (hz + r.range(0.2, 0.9)));
        let d = v(r.range(-1.0, 1.0), 0.0, r.range(-1.0, 1.0)).normalize_or_zero();
        stick_c(mb, p - d * 0.6, p + d * 0.6, 0.07, mix(col(palette::BARK), col(palette::DECAY), 0.5));
    }
}

pub fn raft_landing(mb: &mut Mb, f: f32) {
    let (hx, hz) = (2.4f32, 2.2f32);
    let done = f >= 1.0;
    let mut r = Rng(8181);
    let deck = 0.55;
    let piles = (f / 0.35).min(1.0);
    for i in 0..4 {
        for j in 0..3 {
            let (x, z) = (-hx + 0.2 + i as f32 * (2.0 * hx - 0.4) / 3.0, -hz + 0.3 + j as f32 * (2.0 * hz - 0.6) / 2.0);
            stick_c(mb, v(x, -0.6, z), v(x, -0.6 + (deck + 0.65) * piles, z), 0.1, peeled());
        }
    }
    if f > 0.35 {
        let n = 16;
        let laid = ((f - 0.35) / 0.35 * n as f32).min(n as f32) as usize;
        for k in 0..laid {
            let z = -hz + (k as f32 + 0.5) * 2.0 * hz / n as f32;
            stick(mb, v(-hx, deck, z), v(hx, deck + r.range(-0.02, 0.02), z), 0.13, r.next());
        }
        for x in [-hx + 0.3, 0.0, hx - 0.3] {
            stick(mb, v(x, deck - 0.12, -hz), v(x, deck - 0.12, hz), 0.08, r.next());
        }
    }
    if f > 0.7 {
        for x in [-hx + 0.4, 0.0, hx - 0.4] {
            stick_c(mb, v(x, deck - 0.2, hz + 0.05), v(x, deck + 0.55, hz + 0.05), 0.1, peeled());
            mb.ellipsoid(v(x, deck + 0.58, hz + 0.05), v(0.13, 0.05, 0.13), col(palette::BARK));
        }
        stick(mb, v(-hx, deck + 0.5, -hz), v(hx, deck + 0.5, -hz), 0.05, r.next());
        for x in [-hx, -hx / 2.0, 0.0, hx / 2.0, hx] {
            stick_c(mb, v(x, deck, -hz), v(x, deck + 0.55, -hz), 0.05, peeled());
        }
    }
    if f > 0.85 || done {
        let foot = v(hx - 0.6, deck, hz - 0.8);
        let head = foot + v(0.0, 2.6, 0.0);
        stick(mb, foot, head, 0.11, r.next());
        stick(mb, foot + v(-0.5, 0.0, -0.5), foot + v(0.0, 1.2, 0.0), 0.06, r.next());
        let tip = head + v(-1.6, 0.25, 0.9);
        stick(mb, head + v(0.3, -0.1, -0.2), tip, 0.07, r.next());
        stick_c(mb, tip, tip + v(0.0, -1.3, 0.0), 0.015, hex(0xd9c79a));
        for k in 0..4 {
            let o = (k as f32 - 1.5) * 0.09;
            stick(mb, tip + v(-0.35, -1.45 + o * 0.3, o), tip + v(0.35, -1.45 - o * 0.3, o), 0.06, r.next());
        }
        stick_crib(mb, v(-1.2, deck, -0.9), 0.55, &mut r);
        mud_mound(mb, v(0.3, deck, -1.2), 0.5, &mut r);
        food_heap(mb, v(-1.6, deck, 0.5), 0.45, &mut r);
    }
    if !done {
        site_markers(mb, hx + 0.8, hz + 0.8, 0.0);
    }
}

pub(super) fn pebble_cairn(mb: &mut Mb, c: Vec3, grow: f32, r: &mut Rng) {
    let n = (3.0 + 9.0 * grow) as usize;
    for i in 0..n {
        let layer = (i / 5) as f32;
        let a = i as f32 * 2.2 + r.range(-0.3, 0.3);
        let e = 0.75 - 0.25 * layer;
        let p = c + v(a.cos() * 0.55 * e, 0.15 + layer * 0.32, a.sin() * 0.95 * e);
        let s = r.range(0.24, 0.34);
        mb.ellipsoid_r(p, v(s * 1.25, s * 0.8, s), Quat::from_rotation_y(a), mix(hex(0x66666b), hex(0xbdbbb6), r.next()));
    }
}

pub(super) fn stick_crib(mb: &mut Mb, c: Vec3, grow: f32, r: &mut Rng) {
    let layers = (1.0 + 5.0 * grow) as usize;
    let rad = 0.13;
    for layer in 0..layers {
        let y = 0.12 + layer as f32 * rad * 1.8;
        let shrink = 1.0 - layer as f32 * 0.06;
        let along_z = layer % 2 == 0;
        let n = if along_z { 4 } else { 6 };
        for k in 0..n {
            let o = (k as f32 - (n as f32 - 1.0) * 0.5) * if along_z { 0.28 } else { 0.42 } * shrink;
            let j = r.range(-0.05, 0.05);
            let (a, b) = if along_z {
                (c + v(o, y, -1.4 * shrink + j), c + v(o + r.range(-0.04, 0.04), y, 1.4 * shrink + j))
            } else {
                (c + v(-0.6 * shrink + j, y, o), c + v(0.6 * shrink + j, y, o + r.range(-0.04, 0.04)))
            };
            let pick = r.next();
            let colr = if pick < 0.18 { col(palette::PEELED) } else { mix(col(palette::BARK), col(palette::BARK_LIGHT), r.next()) };
            stick_c(mb, a, b, rad, colr);
        }
    }
}

pub(super) fn mud_mound(mb: &mut Mb, c: Vec3, grow: f32, r: &mut Rng) {
    let (lo, hi) = (col(palette::MUD_WET), col(palette::DAM_MUD));
    let (rx, ry, rz) = (0.8, 0.25 + 0.9 * grow, 1.35);
    let start = mb.len();
    mb.ellipsoid_g(c, v(rx, ry, rz), lo, hi, 14, 8);
    mb.displace(start, |p| {
        let q = p - c;
        let k = 1.0 + 0.07 * (q.x * 5.0 + q.z * 3.0).sin() + 0.05 * (q.z * 7.0).sin();
        c + v(q.x * k, q.y * (1.0 + 0.06 * (q.x * 4.0 - q.z * 6.0).sin()), q.z * k)
    });
    let balls = (2.0 + 5.0 * grow) as usize;
    for i in 0..balls {
        let a = i as f32 * 2.4 + r.range(-0.3, 0.3);
        let e = r.range(0.35, 1.0);
        let p = c + v(a.cos() * rx * 0.75 * (1.0 - e * 0.5), ry * e, a.sin() * rz * 0.75 * (1.0 - e * 0.5));
        let s = r.range(0.17, 0.24);
        mb.ellipsoid_g(p, v(s, s * 0.85, s), hex(0x9A5E36), hex(0xC48A58), 8, 5);
    }
}

pub(super) fn food_heap(mb: &mut Mb, c: Vec3, grow: f32, r: &mut Rng) {
    let (rx, top, rz) = (0.85f32, 0.3 + 1.15 * grow, 1.4f32);
    mb.ellipsoid_g(c, v(rx, top, rz), col(palette::BARK_DARK), mix(col(palette::BARK), hex(0x8a7a3a), 0.4), 12, 7);
    let leaf = |k: usize| match k % 3 { 0 => col(palette::LEAF), 1 => mix(col(palette::LEAF), hex(0xB59A3C), 0.55), _ => col(palette::LEAF_LIGHT) };
    let n = (6.0 + 16.0 * grow) as usize;
    for i in 0..n {
        let (a, e) = (r.range(0.0, TAU), r.range(0.1, 1.2));
        let p = c + v(a.cos() * e.cos() * rx, e.sin() * top, a.sin() * e.cos() * rz);
        let q = p - c;
        let normal = v(q.x / (rx * rx), q.y / (top * top), q.z / (rz * rz)).normalize();
        let t = normal.cross(Vec3::Y).normalize_or_zero();
        let t = if t == Vec3::ZERO { Vec3::X } else { t };
        let phi = r.range(0.0, TAU);
        let d = t * phi.cos() + normal.cross(t) * phi.sin();
        let l = r.range(0.6, 1.1);
        let m = p + normal * 0.06;
        branch(mb, m - d * l * 0.5, m + d * l * 0.5, 0.05, leaf(i), 31 + i as u32);
    }
}
