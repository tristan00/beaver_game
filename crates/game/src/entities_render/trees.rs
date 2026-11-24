//! Trees: a model for each species, grown on its branches, for every step of the year and three
//! levels of detail. Each keeps its real outline, bark and autumn colour, so good food trees
//! (aspen, willow) can be told from poor ones (beech, the conifers) at a glance:
//! aspen a slim pale trunk under a small round crown that turns gold; birch two white stems
//! banded black; willow a leaning trunk under a weeping curtain; alder a shrubby clump of grey
//! stems that stays green late and hangs cones in winter; maple a broad dome that turns red;
//! beech a low wide spread on a silver trunk that keeps its dead leaves through winter; pine a
//! tall bare trunk under flat tufts; spruce a dark, narrow spire of drooping boughs.
use super::meshgen::{hex, mix, Mb, Rng};
use super::models::Season;
use super::scene::Species;
use crate::palette::{self, pack};
use bevy::prelude::*;
use std::f32::consts::TAU;

fn v(x: f32, y: f32, z: f32) -> Vec3 { Vec3::new(x, y, z) }

const INK: u32 = pack(palette::INK);

fn smooth01(a: f32, b: f32, x: f32) -> f32 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

const FOLIAGE_SATURATION: f32 = 1.22;

/// Leaf colour through the year (spring, summer, autumn, late autumn), each a shade and a light.
fn keys(sp: Species) -> [(u32, u32); 4] {
    match sp {
        Species::Aspen => [(0x86B35A, 0xC0DB86), (0x689A4C, 0xA6C676), (0xD6A42C, 0xF4D25A), (0xA88A44, 0xCDB070)],
        Species::Birch => [(0x95C064, 0xCBE494), (0x70A250, 0xACCD7A), (0xD9B43E, 0xF4D872), (0xB0924E, 0xD4B878)],
        Species::Willow => [(0xA2C866, 0xD6EA96), (0x82AC5A, 0xBAD688), (0xC4B446, 0xE4D676), (0xA49650, 0xC6BA78)],
        Species::Alder => [(0x6A9C50, 0x9EC278), (0x3E7240, 0x6C9A5A), (0x587A3E, 0x86985A), (0x6A6440, 0x8C8458)],
        Species::Maple => [(0x74A452, 0xA8C878), (0x4C8644, 0x84B062), (0xC8442A, 0xF08A3A), (0x92563A, 0xBC8458)],
        Species::Beech => [(0x8CBE5C, 0xC4E08A), (0x5C9048, 0x92BA6A), (0xB0602A, 0xD88E44), (0xA8845A, 0xCCAE80)],
        Species::Pine => [(0x4C7C5C, 0x86AE86), (0x467858, 0x80AA82), (0x4A7658, 0x84A882), (0x4C6E58, 0x86A28A)],
        Species::Spruce => [(0x2E5E46, 0x5E8E6C), (0x2A5A44, 0x5A8A68), (0x2C5A46, 0x5A8868), (0x2E5648, 0x5C8470)],
    }
}

fn foliage(sp: Species, s: Season) -> ([f32; 3], [f32; 3]) {
    let k = keys(sp);
    let dark = k.map(|(a, _)| palette::rgb(a));
    let light = k.map(|(_, b)| palette::rgb(b));
    // A little richer than the keys: under the strong ambient light, crowns at the keys'
    // saturation read as a pale olive haze.
    let richer = |c: [f32; 3]| {
        let g = (c[0] + c[1] + c[2]) / 3.0;
        c.map(|x| (g + (x - g) * FOLIAGE_SATURATION).clamp(0.0, 1.0))
    };
    let colour = |k: &[_; 4]| palette::display_linear(richer(crate::seasonal::blend(k, s.phase(), palette::mix)));
    (colour(&dark), colour(&light))
}

/// How much of a full crown is in leaf, and how much of that is last year's dead leaves (beech
/// keeps its leaves, brown, until spring).
fn leaf_cover(sp: Species, phase: f32) -> (f32, f32) {
    let l = crate::seasonal::leaves(phase);
    match sp {
        Species::Pine | Species::Spruce => (1.0, 0.0),
        Species::Beech => {
            const KEPT: f32 = 0.45;
            (l.max(KEPT), smooth01(KEPT, 0.08, l))
        }
        _ => (l, 0.0),
    }
}

/// A species' build: bark, trunk, and the crown its branches carry.
struct Shape {
    bark: u32,
    /// The trunk's foot: darker, rougher, or (birch) black.
    foot: u32,
    trunk_r: f32,
    fork: Vec3,
    centre: Vec3,
    radius: Vec3,
    tips: usize,
    clump: f32,
    /// Clumps squashed to flat sprays (beech) or round (1).
    flat: f32,
    /// How far outer tips hang down.
    droop: f32,
    limb: f32,
    seed: u32,
}

fn shape(sp: Species) -> Shape {
    let s = |bark, foot, trunk_r, fork: f32, centre: f32, radius: Vec3, tips, clump, flat, droop, limb, seed| Shape {
        bark, foot, trunk_r, fork: v(0.0, fork, 0.0), centre: v(0.0, centre, 0.0), radius, tips, clump, flat, droop, limb, seed,
    };
    match sp {
        Species::Aspen => s(0xDAD8C0, 0x5E5A4E, 0.16, 2.9, 3.95, v(0.85, 1.3, 0.85), 14, 0.46, 0.9, 0.0, 0.06, 3),
        Species::Birch => s(0xEFEBE2, 0x3A3631, 0.13, 2.6, 3.65, v(1.15, 1.2, 1.0), 16, 0.42, 0.85, 0.35, 0.05, 11),
        Species::Willow => s(0x6A5A42, 0x4A3E2E, 0.3, 2.3, 3.0, v(1.75, 0.95, 1.75), 14, 0.5, 0.8, 0.2, 0.1, 9),
        Species::Alder => s(0x77716A, 0x55504A, 0.12, 1.7, 2.8, v(1.3, 1.15, 1.3), 14, 0.5, 0.9, 0.0, 0.06, 21),
        Species::Maple => s(0x6E5C4C, 0x4E4034, 0.24, 1.8, 3.35, v(1.75, 1.4, 1.75), 18, 0.62, 0.85, 0.0, 0.12, 7),
        Species::Beech => s(0xA9A79E, 0x7E7C74, 0.28, 1.4, 2.75, v(2.05, 1.2, 2.05), 18, 0.62, 0.6, 0.1, 0.13, 17),
        Species::Pine => s(0xB06A40, 0x5A4434, 0.2, 2.4, 4.0, v(1.3, 1.6, 1.3), 0, 0.0, 1.0, 0.0, 0.06, 5),
        Species::Spruce => s(0x6A5040, 0x4E3C30, 0.2, 0.9, 3.4, v(1.3, 3.0, 1.3), 0, 0.0, 1.0, 0.0, 0.05, 23),
    }
}

fn root_flare(mb: &mut Mb, at: Vec3, r: f32, bark: [f32; 3]) {
    mb.lathe(Mat4::from_translation(at), &[(0.0, -0.1), (r * 1.8, -0.1), (r * 1.2, 0.1), (r, 0.4)], 8, mix(bark, hex(INK), 0.25), bark, true);
}

/// A stem from `a` to `b`, its lowest `foot_h` in the darker foot colour.
fn stem(mb: &mut Mb, a: Vec3, b: Vec3, r0: f32, r1: f32, foot: [f32; 3], bark: [f32; 3], foot_h: f32, seg: usize) {
    let d = b - a;
    let t = (foot_h / d.length().max(0.01)).clamp(0.0, 0.9);
    let m = a + d * t;
    let rm = r0 + (r1 - r0) * t;
    mb.cyl(a + v(0.0, -0.1, 0.0), m, r0, rm, foot, mix(foot, bark, 0.6), seg);
    mb.cyl(m, b, rm, r1, mix(foot, bark, 0.6), bark, seg);
}

/// A lumpy ellipsoid: a clump of leaves rather than a ball.
fn clump(mb: &mut Mb, c: Vec3, r: Vec3, lo: [f32; 3], hi: [f32; 3], (seg, rings): (usize, usize), seed: f32) {
    let start = mb.len();
    mb.ellipsoid_g(c, r, lo, hi, seg, rings);
    let s = r.max_element().max(0.01);
    mb.displace(start, move |p| {
        let q = (p - c) / s;
        let k = 1.0 + 0.14 * (q.x * 5.3 + seed).sin() * (q.z * 4.7 - seed * 1.7).cos() + 0.07 * (q.y * 6.1 + seed * 2.3).sin();
        c + (p - c) * k
    });
}

/// A broadleaf crown grown on its branches. Limbs rise from the nearest fork, curve outward and
/// split; every tip carries a clump of leaves that swells from a bud in spring to an overlapping
/// crown in summer and thins again in autumn, with more clumps filling the inside so few limbs
/// show through a crown in leaf. Fine twigs show only on a crown that is nearly bare. Returns
/// the tips, for blossom, catkins and hanging strands.
fn crown(mb: &mut Mb, sh: &Shape, forks: &[Vec3], leaves: f32, bark: [f32; 3], dark: [f32; 3], light: [f32; 3], detail: bool) -> Vec<(Vec3, f32)> {
    let mut rng = Rng(sh.seed * 7919 + 13);
    let n = if detail { sh.tips } else { (sh.tips / 2).max(5) };
    let tips: Vec<(Vec3, f32)> = (0..n)
        .map(|i| {
            let y = 0.85 - (i as f32 + 0.5) / n as f32 * 1.4;
            let ring = (1.0 - y * y).max(0.0).sqrt();
            let a = i as f32 * 2.399_963 + rng.range(-0.3, 0.3);
            let p = sh.centre + v(a.cos() * ring * sh.radius.x, y * sh.radius.y - sh.droop * ring * sh.radius.y * 0.6, a.sin() * ring * sh.radius.z) * 0.82;
            (p, y)
        })
        .collect();
    let seg = if detail { 5 } else { 4 };
    let tip_col = mix(bark, hex(0xb8a68c), 0.3);
    let lean = |from: Vec3, to: Vec3| v((to - from).x * 0.3, (to - from).y * 0.65, (to - from).z * 0.3);
    let mut splits = Vec::new();
    for pair in tips.chunks(2) {
        let m = pair.iter().map(|t| t.0).sum::<Vec3>() / pair.len() as f32;
        let fork = forks.iter().copied().min_by(|a, b| a.xz().distance_squared(m.xz()).total_cmp(&b.xz().distance_squared(m.xz()))).unwrap_or(sh.fork);
        let elbow = fork + lean(fork, m);
        let split = m.lerp(elbow, 0.35);
        mb.cyl(fork, elbow, sh.limb, sh.limb * 0.7, bark, bark, seg);
        mb.cyl(elbow, split, sh.limb * 0.7, sh.limb * 0.45, bark, bark, seg);
        for &(t, _) in pair {
            mb.cyl(split, t, sh.limb * 0.42, (sh.limb * 0.15).max(0.012), bark, tip_col, seg - 1);
            if detail && leaves < 0.3 {
                let out = (t - sh.centre).normalize_or_zero();
                for k in [-1.0f32, 1.0] {
                    let side = out.cross(Vec3::Y).normalize_or_zero() * 0.35 * k;
                    let end = t + (out * 0.6 + Vec3::Y * 0.55 + side).normalize() * 0.26;
                    mb.cyl(t, end, 0.016, 0.006, tip_col, tip_col, 3);
                }
            }
        }
        splits.push(split);
    }
    if leaves < 0.02 {
        return tips;
    }
    let grow = leaves.powf(0.6);
    let fill = smooth01(0.3, 1.0, leaves);
    let res = if detail { (9, 6) } else { (6, 4) };
    // Clumps darken toward their undersides and the crown's base, so a crown has form in mostly
    // ambient light.
    let col = |y: f32, j: f32| {
        let up = (y * 0.5 + 0.5).clamp(0.0, 1.0);
        let lo = mix(mix(dark, hex(INK), 0.18 * (1.0 - up)), light, (0.02 + 0.28 * up + j).clamp(0.0, 1.0));
        let hi = mix(dark, light, (0.5 + 0.35 * up + j).clamp(0.0, 1.0));
        (lo, hi)
    };
    if fill > 0.02 {
        let (lo, hi) = col(-0.3, 0.0);
        clump(mb, sh.centre, sh.radius * v(0.8, 0.8 * sh.flat.max(0.75), 0.8) * fill, lo, hi, res, sh.seed as f32);
    }
    for &(t, y) in &tips {
        let s = sh.clump * rng.range(0.85, 1.15) * (1.0 - 0.12 * y.max(0.0)) * grow;
        let (lo, hi) = col(y, rng.range(-0.1, 0.1));
        clump(mb, t, v(s, s * 0.82 * sh.flat, s), lo, hi, res, rng.range(0.0, 9.0));
    }
    if detail && fill > 0.02 {
        for &f in &splits {
            let y = ((f.y - sh.centre.y) / sh.radius.y).clamp(-1.0, 1.0);
            let s = sh.clump * 0.85 * fill;
            let (lo, hi) = col(y, rng.range(-0.1, 0.1));
            clump(mb, f, v(s, s * 0.82 * sh.flat, s), lo, hi, res, rng.range(0.0, 9.0));
        }
        // Clumps in the gaps between the tips, half way out.
        for i in 0..n / 2 {
            let a = i as f32 * 2.399_963 * 2.0 + 1.1;
            let y = rng.range(-0.3, 0.75);
            let ring = (1.0 - y * y).max(0.0).sqrt() * 0.6;
            let p = sh.centre + v(a.cos() * ring * sh.radius.x, y * sh.radius.y * 0.7, a.sin() * ring * sh.radius.z);
            let s = sh.clump * rng.range(0.8, 1.0) * fill;
            let (lo, hi) = col(y, rng.range(-0.1, 0.1));
            clump(mb, p, v(s, s * 0.82 * sh.flat, s), lo, hi, res, rng.range(0.0, 9.0));
        }
    }
    tips
}

/// Willow: from each outer tip, thin golden withies droop to below the crown, hung with long
/// narrow leaves in season, so the crown falls in a streaked curtain.
fn willow_strands(mb: &mut Mb, sh: &Shape, tips: &[(Vec3, f32)], leaves: f32, dark: [f32; 3], light: [f32; 3], detail: bool) {
    let withy = hex(0xB0984A);
    let grow = leaves.powf(0.6);
    let mut r = Rng(19);
    let per = if detail { 3 } else { 1 };
    for &(t, y) in tips.iter().filter(|t| t.1 < 0.6) {
        let out = v(t.x - sh.centre.x, 0.0, t.z - sh.centre.z).normalize_or_zero();
        for k in 0..per {
            let side = out.cross(Vec3::Y) * (k as f32 - (per as f32 - 1.0) * 0.5) * 0.45;
            let len = r.range(1.4, 2.0) * (1.0 - 0.3 * y.max(0.0));
            let p1 = t + out * 0.3 + side + Vec3::Y * 0.06;
            let p2 = p1 + out * 0.12 - Vec3::Y * len * 0.5;
            let p3 = p2 + out * 0.04 - Vec3::Y * len * 0.5;
            for (a, b) in [(t, p1), (p1, p2), (p2, p3)] {
                mb.cyl(a, b, 0.018, 0.01, withy, withy, 3);
            }
            if grow < 0.02 {
                continue;
            }
            let n = if detail { 7 } else { 3 };
            for i in 0..n {
                let u = (i as f32 + 0.5) / n as f32;
                let (p, d) = if u < 0.5 { (p1.lerp(p2, u * 2.0), p2 - p1) } else { (p2.lerp(p3, (u - 0.5) * 2.0), p3 - p2) };
                let s = 0.11 * grow * (1.0 - 0.3 * u);
                let rot = Quat::from_rotation_arc(Vec3::Y, -d.normalize_or_zero());
                let c = mix(mix(dark, light, 0.2 + 0.4 * r.next()), withy, 0.1);
                mb.ellipsoid_m(Mat4::from_rotation_translation(rot, p + out * 0.04) * Mat4::from_scale(v(s, s * 3.2, s * 0.7)), 5, 3, c, mix(c, light, 0.3));
            }
        }
    }
}

/// Aspen's black "eyes" where branches fell off the pale bark.
fn bark_eyes(mb: &mut Mb, h: f32, r0: f32, r1: f32, n: usize, seed: u32) {
    let mut rng = Rng(seed);
    for _ in 0..n {
        let y = rng.range(0.6, h * 0.9);
        let a = rng.range(0.0, TAU);
        let rr = r0 + (r1 - r0) * (y / h) + 0.004;
        let rot = Quat::from_rotation_y(std::f32::consts::FRAC_PI_2 - a);
        mb.ellipsoid_r(v(a.cos() * rr, y, a.sin() * rr), v(0.07, 0.03, 0.018), rot, hex(0x3E362E));
    }
}

/// Birch's black bands: short dashes across a white stem from `a` to `b`.
fn birch_dashes(mb: &mut Mb, a: Vec3, b: Vec3, r0: f32, r1: f32, n: usize, seed: u32) {
    let mut rng = Rng(seed);
    let d = (b - a).normalize_or_zero();
    let side = d.cross(Vec3::X).normalize_or_zero();
    for _ in 0..n {
        let t = rng.range(0.15, 0.9);
        let ang = rng.range(0.0, TAU);
        let out = Quat::from_axis_angle(d, ang) * side;
        let r = r0 + (r1 - r0) * t + 0.003;
        let rot = Quat::from_rotation_arc(Vec3::X, d.cross(out).normalize_or_zero());
        mb.ellipsoid_m(Mat4::from_rotation_translation(rot, a.lerp(b, t) + out * r) * Mat4::from_scale(v(rng.range(0.05, 0.1), 0.012, 0.014)), 5, 3, hex(0x2E2A26), hex(0x2E2A26));
    }
}

/// The stems a species grows from the ground: (foot, top, foot radius, top radius).
fn stems(sp: Species, sh: &Shape) -> Vec<(Vec3, Vec3, f32, f32)> {
    let r = sh.trunk_r;
    match sp {
        Species::Birch => vec![(v(-0.14, 0.0, 0.05), v(-0.5, 4.1, 0.2), r, r * 0.35), (v(0.14, 0.0, -0.05), v(0.42, 3.8, -0.25), r * 0.9, r * 0.35)],
        Species::Alder => (0..3)
            .map(|k| {
                let a = k as f32 * TAU / 3.0 + 0.4;
                (v(a.cos() * 0.1, 0.0, a.sin() * 0.1), v(a.cos() * 0.6, 3.1 - 0.25 * k as f32, a.sin() * 0.6), r, r * 0.4)
            })
            .collect(),
        Species::Willow => vec![(v(0.0, 0.0, 0.0), v(0.25, 1.35, 0.0), r, r * 0.75)],
        Species::Aspen => vec![(Vec3::ZERO, v(0.04, 4.5, 0.0), r, r * 0.35)],
        _ => vec![(Vec3::ZERO, sh.fork + v(0.0, 0.25, 0.0), r, r * 0.65)],
    }
}

fn broadleaf(mb: &mut Mb, sp: Species, sh: &Shape, phase: f32, dark: [f32; 3], light: [f32; 3], lod: u8) {
    let (bark, foot) = (hex(sh.bark), hex(sh.foot));
    let (leaves, dead) = leaf_cover(sp, phase);
    let (dark, light) = if dead > 0.0 { (mix(dark, hex(0x8E6E4A), dead), mix(light, hex(0xB89A70), dead)) } else { (dark, light) };
    let detail = lod == 0;
    let seg = if detail { 8 } else { 5 };
    let stems = stems(sp, sh);
    if detail && !matches!(sp, Species::Alder | Species::Birch) {
        root_flare(mb, Vec3::ZERO, sh.trunk_r, mix(foot, bark, 0.4));
    }
    let foot_h = match sp { Species::Birch => 0.55, Species::Aspen => 0.7, _ => 0.4 };
    let used: &[_] = if detail { &stems } else { &stems[..1.min(stems.len())] };
    for &(a, b, r0, r1) in used {
        stem(mb, a, b, r0, r1, foot, bark, foot_h, seg);
        if detail && sp == Species::Birch {
            birch_dashes(mb, a, b, r0, r1, 14, (b.x * 100.0).abs() as u32 + 3);
        }
    }
    if detail && sp == Species::Aspen {
        bark_eyes(mb, 4.4, sh.trunk_r, sh.trunk_r * 0.35, 10, 11);
    }
    let mut forks: Vec<Vec3> = match sp {
        // Limbs leave each stem part way up.
        Species::Birch | Species::Alder => used.iter().map(|&(a, b, ..)| a.lerp(b, 0.62)).collect(),
        Species::Willow => {
            let top = stems[0].1;
            let leaders = [(top, v(-0.5, 2.35, 0.35)), (top, v(0.75, 2.45, -0.3))];
            for (a, b) in leaders {
                mb.cyl(a, b, sh.trunk_r * 0.7, sh.trunk_r * 0.45, bark, bark, seg);
            }
            leaders.iter().map(|l| l.1).collect()
        }
        _ => vec![sh.fork],
    };
    if forks.is_empty() {
        forks.push(sh.fork);
    }
    let tips = crown(mb, sh, &forks, leaves, mix(bark, foot, 0.3), dark, light, detail);
    match sp {
        Species::Willow => willow_strands(mb, sh, &tips, leaves, dark, light, detail),
        Species::Maple if detail => {
            let bloom = crate::seasonal::bloom(phase) * (1.0 - smooth01(0.4, 0.8, leaves));
            if bloom > 0.1 {
                // Red flowers on bare twigs before the leaves.
                let mut r = Rng(77);
                let reach = sh.clump * leaves.powf(0.6).max(0.45);
                for &(t, _) in &tips {
                    for _ in 0..3 {
                        let d = v(r.range(-1.0, 1.0), r.range(-0.2, 1.0), r.range(-1.0, 1.0)).normalize_or_zero();
                        mb.ellipsoid(t + d * reach, v(0.07, 0.05, 0.07) * bloom, mix(hex(0xC8483A), hex(0xF0907A), r.range(0.0, 0.5)));
                    }
                }
            }
        }
        // Last year's cones and this year's catkins hang on the bare twigs.
        Species::Alder if detail && leaves < 0.5 => {
            let mut r = Rng(29);
            for &(t, _) in &tips {
                for _ in 0..3 {
                    let p = t + v(r.range(-0.2, 0.2), r.range(-0.15, 0.05), r.range(-0.2, 0.2));
                    mb.ellipsoid(p, v(0.045, 0.08, 0.045), hex(0x5A3E30));
                }
            }
        }
        _ => {}
    }
}

/// Pine: a tall trunk, grey-brown low and orange up high, bare most of its height, with whorls
/// of near-level limbs ending in flat tufts of needles and a plume at the top.
fn pine(mb: &mut Mb, sh: &Shape, dark: [f32; 3], light: [f32; 3], snow: f32, lod: u8) {
    let (bark, foot) = (hex(sh.bark), hex(sh.foot));
    let top = mix(light, hex(0xE6EEF0), snow * 0.45);
    let detail = lod == 0;
    if detail {
        root_flare(mb, Vec3::ZERO, sh.trunk_r, foot);
    }
    mb.cyl(v(0.0, -0.1, 0.0), v(0.0, 5.5, 0.0), sh.trunk_r, 0.05, foot, bark, if detail { 7 } else { 5 });
    let res = if detail { (9, 5) } else { (6, 3) };
    let mut r = Rng(sh.seed);
    let whorls: &[f32] = if lod == 0 { &[2.3, 2.95, 3.6, 4.2, 4.75] } else { &[2.5, 3.5, 4.4] };
    for (w, &y) in whorls.iter().enumerate() {
        let n = if detail { 3 } else { 2 };
        let reach = 1.4 * (1.0 - (y - 2.3) / 3.2 * 0.55);
        for k in 0..n {
            if detail && w > 0 && r.next() < 0.2 {
                continue;
            }
            let a = k as f32 * TAU / n as f32 + w as f32 * 1.1 + r.range(-0.4, 0.4);
            let l = reach * r.range(0.8, 1.15);
            let tip = v(a.cos() * l, y + r.range(0.1, 0.3), a.sin() * l);
            mb.cyl(v(0.0, y, 0.0), tip, sh.limb, sh.limb * 0.4, bark, bark, 4);
            let s = r.range(0.85, 1.1) * (1.0 - 0.25 * w as f32 / whorls.len() as f32);
            let rot = Quat::from_rotation_y(-a);
            for (f, k) in [(1.0f32, 1.0f32), (0.55, 0.7)] {
                let c = v(0.0, y, 0.0).lerp(tip, f) + v(0.0, 0.08, 0.0);
                let start = mb.len();
                mb.ellipsoid_m(Mat4::from_rotation_translation(rot, c) * Mat4::from_scale(v(0.7, 0.28, 0.55) * s * k), res.0, res.1, mix(dark, hex(INK), 0.15), top);
                let sd = a + y;
                mb.displace(start, move |p| {
                    let q = p - c;
                    c + q * (1.0 + 0.16 * (q.x * 6.0 + sd).sin() * (q.z * 5.0 - sd).cos())
                });
            }
        }
    }
    mb.ellipsoid_g(v(0.0, 5.35, 0.0), v(0.5, 0.45, 0.5), mix(dark, hex(INK), 0.1), top, res.0, res.1);
}

/// Spruce: a dense, narrow spire of drooping boughs from near the ground to a spike at the top,
/// their ragged edges dark, their tops catching the light (and the snow).
fn spruce(mb: &mut Mb, sh: &Shape, dark: [f32; 3], light: [f32; 3], snow: f32, lod: u8) {
    let (bark, foot) = (hex(sh.bark), hex(sh.foot));
    let top = mix(mix(dark, light, 0.6), hex(0xEAF0F2), snow * 0.5);
    let under = mix(dark, hex(INK), 0.2);
    mb.cyl(v(0.0, -0.1, 0.0), v(0.0, 1.2, 0.0), sh.trunk_r, sh.trunk_r * 0.85, foot, bark, 6);
    let tiers = match lod { 0 => 8, 1 => 5, _ => 3 };
    let seg = match lod { 0 => 14, 1 => 9, _ => 7 };
    let (base, height) = (0.55f32, 6.0f32);
    let step = (height - base - 0.6) / tiers as f32;
    for k in 0..tiers {
        let t = k as f32 / tiers as f32;
        let y0 = base + k as f32 * step;
        let rr = 1.35 * (1.0 - t).powf(1.1) + 0.15;
        let h = step * 1.9;
        let prof = [(0.1, y0 + 0.2), (rr * 0.92, y0 - 0.16), (rr, y0 - 0.05), (rr * 0.7, y0 + h * 0.35), (rr * 0.3, y0 + h * 0.75), (0.0, y0 + h)];
        let start = mb.len();
        mb.lathe(Mat4::from_rotation_y(k as f32 * 0.9), &prof, seg, under, mix(top, light, 0.1 * t), true);
        // Ragged bough ends rather than a turned cone.
        let ph = k as f32 * 1.7;
        mb.displace(start, move |p| {
            let a = p.z.atan2(p.x);
            let kk = 1.0 + 0.13 * (7.0 * a + ph).cos() + 0.05 * (13.0 * a - ph).sin();
            v(p.x * kk, p.y, p.z * kk)
        });
    }
    mb.cyl(v(0.0, height - 0.3, 0.0), v(0.0, height + 0.5, 0.0), 0.06, 0.01, under, top, 4);
}

pub fn tree_mesh(sp: Species, season: Season, lod: u8) -> Mesh {
    let mut mb = Mb::new();
    let sh = shape(sp);
    let (dark, light) = foliage(sp, season);
    let phase = season.phase();
    let snow = crate::seasonal::snow(phase);
    match (sp, lod) {
        (Species::Pine, l) if l < 2 => pine(&mut mb, &sh, dark, light, snow, l),
        (Species::Spruce, l) => spruce(&mut mb, &sh, dark, light, snow, l),
        (_, l) if l < 2 => broadleaf(&mut mb, sp, &sh, phase, dark, light, l),
        _ => far_tree(&mut mb, sp, &sh, phase, dark, light, snow),
    }
    mb.build()
}

/// The far view: a stem and one lumpy crown (or a bare stem with its main limbs).
fn far_tree(mb: &mut Mb, sp: Species, sh: &Shape, phase: f32, dark: [f32; 3], light: [f32; 3], snow: f32) {
    let (bark, foot) = (hex(sh.bark), hex(sh.foot));
    let (leaves, dead) = leaf_cover(sp, phase);
    if sp == Species::Pine {
        let top = mix(light, hex(0xE6EEF0), snow * 0.6);
        mb.cyl(v(0.0, -0.1, 0.0), v(0.0, 5.3, 0.0), sh.trunk_r, 0.06, foot, bark, 4);
        for (y, r) in [(3.1f32, 1.25f32), (4.3, 0.95), (5.2, 0.55)] {
            mb.ellipsoid_g(v(0.0, y, 0.0), v(r, r * 0.4, r), dark, top, 6, 3);
        }
        return;
    }
    let (dark, light) = (mix(dark, hex(0x8E6E4A), dead), mix(light, hex(0xB89A70), dead));
    mb.cyl(v(0.0, -0.1, 0.0), sh.fork + v(0.0, 0.2, 0.0), sh.trunk_r * 1.2, sh.trunk_r * 0.6, foot, bark, 4);
    if leaves < 0.035 {
        for k in 0..3 {
            let a = k as f32 * TAU / 3.0;
            mb.cyl(sh.fork, sh.centre + v(a.cos() * sh.radius.x * 0.7, sh.radius.y * 0.5, a.sin() * sh.radius.z * 0.7), sh.limb, 0.02, bark, bark, 3);
        }
        return;
    }
    let s = leaves.sqrt();
    clump(mb, sh.centre, sh.radius * v(s, sh.flat.max(0.75) * s, s), mix(dark, hex(INK), 0.1), mix(dark, light, 0.6), (7, 4), sh.seed as f32);
}

/// What shows of a tree cut away so a beaver or a site behind it can be seen: a dark stump.
pub fn tree_cutaway_mesh(sp: Species) -> Mesh {
    let mut mb = Mb::new();
    let sh = shape(sp);
    let trunk = mix(hex(sh.bark), hex(0x3e3024), 0.75);
    root_flare(&mut mb, Vec3::ZERO, sh.trunk_r, trunk);
    mb.cyl(v(0.0, -0.1, 0.0), v(0.0, 0.55, 0.0), sh.trunk_r, sh.trunk_r * 0.6, trunk, trunk, 7);
    for i in 0..3 {
        let angle = i as f32 * TAU / 3.0;
        mb.cyl(v(0.0, 0.35, 0.0), v(angle.cos() * 0.3, 0.5, angle.sin() * 0.3), 0.07, 0.025, trunk, trunk, 5);
    }
    mb.build()
}

/// A dead standing tree: grey, broken off, its last limbs bare, moss at its foot.
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
