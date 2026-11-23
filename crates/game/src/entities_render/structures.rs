use super::meshgen::{hex, mix, Mb, Rng};
use super::models::{branch, col, mud_lump, site_markers, stake, stick, stick_c, stick_pile};
use crate::palette;
use bevy::prelude::*;
use std::f32::consts::{FRAC_PI_2, TAU};

fn v(x: f32, y: f32, z: f32) -> Vec3 { Vec3::new(x, y, z) }

fn peeled() -> [f32; 3] { col(palette::PEELED) }

pub const DAM_STEP: f32 = 0.25;

/// How far a dam cell has come: a marker stick, a wall going up beside its
/// stick pile, or a finished wall.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DamStage {
    Staked,
    Rising,
    Done,
}

/// One cell of dam wall, running along x. The upstream face (+z) is steep and plastered with
/// mud; the downstream face (-z) is a long, low slope of sticks.
pub fn dam_cell(mb: &mut Mb, h: f32, len: f32, variant: u32, stage: DamStage) {
    if stage == DamStage::Staked {
        stake(mb, v(0.0, 0.0, 0.0), 0.9, true);
        mud_lump(mb, v(0.0, 0.0, 0.0), 0.35);
        return;
    }
    let site = stage == DamStage::Rising;
    let hl = len * 0.5;
    let h = h.max(0.1);
    let wt = 0.22f32;
    let (up_b, down_b) = (0.28 + 0.55 * h, 0.3 + 0.95 * h);
    let bury = 0.45f32;
    // Half-width at height t*h on each side: a slight belly, rounded into the crest.
    let face = |t: f32, base: f32| base + (wt - base) * t + 0.12 * h.min(1.5) * t * (1.0 - t);
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
    // Downstream: sticks laid up the slope, a few across.
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
    // Upstream: smooth wet mud, a few stick ends showing through.
    for _ in 0..((len * h * 3.0) as usize + 1) {
        let t = r.range(0.2, 0.9);
        let c = v(r.range(-hl, hl), t * h, up(t) + 0.01);
        let d = v(r.range(-1.0, 1.0), r.range(-0.3, 0.3), 0.15).normalize();
        stick(mb, c - d * 0.25, c + d * 0.25, 0.04, r.next());
    }
    // Along the crest.
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
        // Every cell being worked keeps its pile of sticks beside it.
        stick_pile(mb, v(0.0, 0.0, -down_b - 0.8), 0.0, 4, 3 + variant);
    }
}

/// One cell's stretch of dam wall, `len` long along x (a little more than the stretch, so
/// neighbouring pieces overlap into one wall).
pub fn dam_cell_mesh(steps: u8, len: f32, variant: u32, stage: DamStage) -> Mesh {
    let mut mb = Mb::new();
    dam_cell(&mut mb, steps.max(1) as f32 * DAM_STEP, len, variant, stage);
    mb.build()
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
        // A raft of logs on rough posts driven into the bed, packed round with mud.
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

/// A finished building left high and dry: it slumps, greys and grows over. Dark and solid, so it
/// is never taken for a see-through placement preview.
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
    // Sticks that have slid off and lie about the base.
    for _ in 0..7 {
        let a = r.range(0.0, TAU);
        let p = v(a.cos() * (hx + r.range(0.2, 0.9)), 0.08, a.sin() * (hz + r.range(0.2, 0.9)));
        let d = v(r.range(-1.0, 1.0), 0.0, r.range(-1.0, 1.0)).normalize_or_zero();
        stick_c(mb, p - d * 0.6, p + d * 0.6, 0.07, mix(col(palette::BARK), col(palette::DECAY), 0.5));
    }
}

/// The colony's stockpile: one raft holding all three stores side by side, so each reads at a
/// glance: a crib of cut sticks, a mound of wet mud and clay, and a heap of leafy food
/// branches. Every heap grows from nothing as the stockpile is built.
pub fn stockpile(mb: &mut Mb, f: f32) {
    let (hx, hz) = (2.4f32, 2.0f32);
    let done = f >= 1.0;
    let grow = 0.25 + 0.75 * f;
    let mut r = Rng(2024);
    // A low rim of piled sticks round the raft.
    for i in 0..30 {
        let a = i as f32 / 30.0 * TAU + r.range(-0.1, 0.1);
        let p = v(a.cos() * hx * 0.98, 0.1 + 0.12 * (i % 2) as f32, a.sin() * hz * 0.98);
        let d = v(-a.sin(), r.range(-0.1, 0.1), a.cos()).normalize();
        stick(mb, p - d * 0.55, p + d * 0.55, 0.07, r.next());
    }
    stick_crib(mb, v(-1.45, 0.0, 0.0), grow, &mut r);
    mud_mound(mb, v(0.05, 0.0, 0.1), grow, &mut r);
    pebble_cairn(mb, v(1.5, 0.0, -0.05), grow, &mut r);
    if !done {
        site_markers(mb, hx + 1.0, hz + 1.0, 0.0);
        stick_pile(mb, v(-hx - 0.8, 0.0, hz + 1.6), 0.2, 7, 41);
    }
}

/// A food store: a raft of piled food with a bundle of saplings heeled in at one end.
pub fn larder(mb: &mut Mb, f: f32) {
    let (hx, hz) = (2.4f32, 2.0f32);
    let done = f >= 1.0;
    let grow = 0.25 + 0.75 * f;
    let mut r = Rng(3031);
    for i in 0..30 {
        let a = i as f32 / 30.0 * TAU + r.range(-0.1, 0.1);
        let p = v(a.cos() * hx * 0.98, 0.1 + 0.12 * (i % 2) as f32, a.sin() * hz * 0.98);
        let d = v(-a.sin(), r.range(-0.1, 0.1), a.cos()).normalize();
        stick(mb, p - d * 0.55, p + d * 0.55, 0.07, r.next());
    }
    food_heap(mb, v(-0.9, 0.0, 0.1), grow, &mut r);
    food_heap(mb, v(0.6, 0.0, -0.3), grow * 0.8, &mut r);
    // Saplings standing in a heap of wet mud.
    let bed = v(1.55, 0.0, 0.75);
    mb.ellipsoid_g(bed, v(0.6, 0.25, 0.5), col(palette::MUD_WET), col(palette::DAM_MUD), 10, 5);
    let n = (2.0 + 6.0 * grow) as usize;
    for k in 0..n {
        let a = k as f32 * 2.4;
        let base = bed + v(a.cos() * 0.3, 0.15, a.sin() * 0.25);
        let tip = base + v(r.range(-0.15, 0.15), r.range(0.8, 1.15), r.range(-0.15, 0.15));
        stick_c(mb, base, tip, 0.035, hex(0x7a5a34));
        for j in 0..3 {
            let p = base.lerp(tip, 0.55 + 0.2 * j as f32) + v(r.range(-0.12, 0.12), 0.0, r.range(-0.12, 0.12));
            let s = r.range(0.12, 0.18);
            mb.ellipsoid_g(p, v(s, s * 0.7, s), col(palette::LEAF), col(palette::LEAF_LIGHT), 6, 4);
        }
    }
    if !done {
        site_markers(mb, hx + 1.0, hz + 1.0, 0.0);
        stick_pile(mb, v(-hx - 0.8, 0.0, hz + 1.6), 0.2, 7, 43);
    }
}

/// A tool shed: a lean-to of poles and bark over a rack of digging sticks, a heap of
/// pebbles beside it.
pub fn toolshed(mb: &mut Mb, f: f32) {
    let (hx, hz) = (1.6f32, 1.3f32);
    let done = f >= 1.0;
    let grow = 0.25 + 0.75 * f;
    let mut r = Rng(4242);
    let h = 0.4 + 1.4 * grow;
    // Four poles, the back pair taller, and a roof of laid sticks sloping to the front.
    for (x, z, top) in [(-hx * 0.8, -hz * 0.6, h), (hx * 0.8, -hz * 0.6, h), (-hx * 0.8, hz * 0.6, h * 0.65), (hx * 0.8, hz * 0.6, h * 0.65)] {
        stick_c(mb, v(x, 0.0, z), v(x, top, z), 0.08, col(palette::BARK));
    }
    if f > 0.4 {
        let n = 9;
        for k in 0..n {
            let x = -hx * 0.9 + 1.8 * hx * k as f32 / (n - 1) as f32;
            stick(mb, v(x, h + 0.05, -hz * 0.75), v(x + r.range(-0.05, 0.05), h * 0.65 + 0.05, hz * 0.8), 0.07, r.next());
        }
    }
    // Digging sticks leaning on the back.
    for k in 0..3 {
        let x = -0.6 + 0.6 * k as f32;
        stick_c(mb, v(x, 0.0, -hz * 0.3), v(x + 0.15, h * 0.85, -hz * 0.6), 0.05, col(palette::PEELED));
    }
    pebble_cairn(mb, v(hx + 0.6, 0.0, 0.5), grow * 0.7, &mut r);
    if !done {
        site_markers(mb, hx + 0.8, hz + 0.8, 0.0);
    }
}

/// A forester's nursery: a low wattle fence round beds of mud with saplings set out in rows,
/// a digging stick leaning on the fence.
pub fn forester(mb: &mut Mb, f: f32) {
    let (hx, hz) = (1.6f32, 1.3f32);
    let done = f >= 1.0;
    let grow = 0.25 + 0.75 * f;
    let mut r = Rng(5151);
    let post = 0.25 + 0.4 * grow;
    let corners = [v(-hx, 0.0, -hz), v(hx, 0.0, -hz), v(hx, 0.0, hz), v(-hx, 0.0, hz)];
    for (i, &a) in corners.iter().enumerate() {
        let b = corners[(i + 1) % 4];
        let n = 4;
        for k in 0..n {
            let p = a.lerp(b, k as f32 / n as f32);
            stick_c(mb, p, p + v(0.0, post, 0.0), 0.05, col(palette::BARK));
        }
        // Wattle: two rails woven between the posts, the gap left at the front for a gate.
        if i == 2 {
            continue;
        }
        for h in [0.4f32, 0.85] {
            let y = post * h;
            stick(mb, a + v(0.0, y, 0.0), b + v(0.0, y + r.range(-0.03, 0.03), 0.0), 0.04, r.next());
        }
    }
    // Two beds of dark mud with saplings set out along them.
    let rows = [-0.55f32, 0.45];
    let per = (1.0 + 3.0 * grow) as usize;
    for &z in &rows {
        let bed = v(-0.1, 0.0, z);
        mb.ellipsoid_g(bed, v(1.15, 0.12, 0.32), col(palette::MUD_DARK), col(palette::MUD_WET), 10, 4);
        for k in 0..per {
            let x = -0.9 + 1.6 * k as f32 / 3.0;
            let base = v(x, 0.08, z + r.range(-0.06, 0.06));
            let tip = base + v(r.range(-0.05, 0.05), r.range(0.45, 0.7) * grow, r.range(-0.05, 0.05));
            stick_c(mb, base, tip, 0.03, hex(0x7a5a34));
            let s = r.range(0.15, 0.22) * grow;
            mb.ellipsoid_g(tip, v(s, s * 0.8, s), col(palette::LEAF), col(palette::LEAF_LIGHT), 6, 4);
        }
    }
    stick_c(mb, v(hx + 0.15, 0.0, -0.2), v(hx - 0.05, post * 1.6, 0.3), 0.05, col(palette::PEELED));
    if !done {
        site_markers(mb, hx + 0.8, hz + 0.8, 0.0);
    }
}

/// A logistics yard: a lean-to over stacked loads, and a sled of poles for dragging them.
pub fn logistics(mb: &mut Mb, f: f32) {
    let (hx, hz) = (1.6f32, 1.3f32);
    let done = f >= 1.0;
    let grow = 0.25 + 0.75 * f;
    let mut r = Rng(6161);
    let h = 0.4 + 1.3 * grow;
    for (x, z, top) in [(-hx * 0.85, -hz * 0.7, h), (hx * 0.85, -hz * 0.7, h), (-hx * 0.85, 0.1, h * 0.7), (hx * 0.85, 0.1, h * 0.7)] {
        stick_c(mb, v(x, 0.0, z), v(x, top, z), 0.08, col(palette::BARK));
    }
    if f > 0.4 {
        for k in 0..9 {
            let x = -hx * 0.9 + 1.8 * hx * k as f32 / 8.0;
            stick(mb, v(x, h + 0.05, -hz * 0.8), v(x + r.range(-0.05, 0.05), h * 0.7 + 0.05, 0.2), 0.07, r.next());
        }
    }
    // Loads waiting under the roof: a crib of sticks and a heap of mud.
    stick_crib(mb, v(-0.6, 0.0, -0.4), grow * 0.6, &mut r);
    mud_mound(mb, v(0.7, 0.0, -0.4), grow * 0.6, &mut r);
    // A sled in front: two runners and cross bars.
    for x in [-0.35f32, 0.35] {
        stick_c(mb, v(x, 0.08, 0.5), v(x, 0.08, hz + 0.5), 0.06, col(palette::PEELED));
    }
    for k in 0..3 {
        let z = 0.7 + 0.45 * k as f32;
        stick(mb, v(-0.5, 0.15, z), v(0.5, 0.15, z), 0.05, r.next());
    }
    if !done {
        site_markers(mb, hx + 0.8, hz + 0.8, 0.0);
    }
}

/// Collected pebbles heaped in a low cairn.
fn pebble_cairn(mb: &mut Mb, c: Vec3, grow: f32, r: &mut Rng) {
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

/// Cut sticks stacked in crossing layers, cut ends out.
fn stick_crib(mb: &mut Mb, c: Vec3, grow: f32, r: &mut Rng) {
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

/// Wet mud heaped up with lumps of clay worked into balls on top.
fn mud_mound(mb: &mut Mb, c: Vec3, grow: f32, r: &mut Rng) {
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
        // Clay is a warmer ochre than the mud it sits on.
        mb.ellipsoid_g(p, v(s, s * 0.85, s), hex(0x9A5E36), hex(0xC48A58), 8, 5);
    }
}

/// Leafy branches piled into a rounded heap, leaves out.
fn food_heap(mb: &mut Mb, c: Vec3, grow: f32, r: &mut Rng) {
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
