use super::meshgen::{hex, mix, Mb, Rng};
use super::models::Season;
use bevy::prelude::*;
use sim::plants::Plant;
use std::f32::consts::TAU;
use crate::seasonal::smooth;
use super::meshgen::v;

#[derive(Clone, Copy)]
struct Year {
    green: f32,
    flower: f32,
    fruit: f32,
    fall: f32,
    dead: f32,
    snow: f32,
}

fn year(phase: f32) -> Year {
    let day = phase.rem_euclid(1.0) * sim::calendar::YEAR_DAYS as f32;
    let green = crate::seasonal::leaves(phase);
    let fall = crate::seasonal::autumn(phase);
    Year {
        green,
        flower: smooth(110.0, 140.0, day) * (1.0 - smooth(190.0, 215.0, day)),
        fruit: smooth(165.0, 185.0, day) * (1.0 - smooth(225.0, 245.0, day)),
        fall,
        dead: (1.0 - green) * (1.0 - fall).max(0.0).max(smooth(240.0, 280.0, day)),
        snow: crate::seasonal::snow(phase),
    }
}

fn leaf(y: Year, spring: u32, summer: u32, autumn: u32, straw: u32) -> [f32; 3] {
    let live = mix(hex(spring), hex(summer), smooth(0.6, 1.0, y.green));
    mix(mix(live, hex(autumn), y.fall), hex(straw), y.dead)
}

fn blade(mb: &mut Mb, base: Vec3, out: Vec3, h: f32, w: f32, arch: f32, lo: [f32; 3], hi: [f32; 3]) {
    let out = v(out.x, 0.0, out.z).normalize_or(Vec3::X);
    let side = out.cross(Vec3::Y).normalize_or_zero() * w * 0.5;
    let pt = |t: f32| base + Vec3::Y * h * t * (1.0 - 0.55 * arch * t) + out * arch * h * t * t * 0.9;
    let segs = 3;
    for k in 0..segs {
        let (t0, t1) = (k as f32 / segs as f32, (k + 1) as f32 / segs as f32);
        let (w0, w1) = (1.0 - 0.8 * t0, 1.0 - 0.95 * t1);
        let (p0, p1) = (pt(t0), pt(t1));
        mb.quad2([p0 - side * w0, p0 + side * w0, p1 + side * w1, p1 - side * w1], mix(lo, hi, (t0 + t1) * 0.5));
    }
}

fn pad(mb: &mut Mb, c: Vec3, r: f32, yaw: f32, slit: f32, col: [f32; 3], tilt: f32) {
    let n = 12;
    let at = |a: f32| {
        let d = v(a.cos(), 0.0, a.sin());
        c + d * r + Vec3::Y * (tilt * r * d.dot(v(yaw.cos(), 0.0, yaw.sin())).max(0.0))
    };
    for i in 0..n {
        let a0 = yaw + slit * 0.5 + (TAU - slit) * i as f32 / n as f32;
        let a1 = yaw + slit * 0.5 + (TAU - slit) * (i + 1) as f32 / n as f32;
        let shade = mix(col, hex(0xffffff), 0.05 * (i % 2) as f32);
        mb.quad2([c, at(a0), at(a1), c], shade);
    }
}

fn snowed(c: [f32; 3], y: Year) -> [f32; 3] { mix(c, hex(0xEEF2F2), y.snow * 0.5) }

fn sedge(mb: &mut Mb, y: Year, r: &mut Rng) {
    let (lo, hi) = (leaf(y, 0x6E9E48, 0x4E7E3A, 0x9A8A48, 0xA89A70), leaf(y, 0xA6CC6E, 0x7EAE58, 0xC4B060, 0xD2C498));
    let (lo, hi) = (snowed(lo, y), snowed(hi, y));
    let h = 0.18 + 0.27 * y.green.max(0.6 * y.dead);
    mb.ellipsoid_g(v(0.0, 0.02, 0.0), v(0.16, 0.06, 0.16), mix(lo, hex(0x3A3020), 0.4), lo, 7, 3);
    for k in 0..20 {
        let a = k as f32 * 2.399 + r.range(-0.2, 0.2);
        let out = v(a.cos(), 0.0, a.sin());
        blade(mb, out * 0.06, out, h * r.range(0.8, 1.2), 0.035, 0.45 + 0.4 * r.next() + 0.3 * y.dead, lo, hi);
    }
    if y.flower + y.fruit > 0.1 {
        for k in 0..4 {
            let a = k as f32 * 1.7 + 0.3;
            let top = v(a.cos() * 0.08, h * 1.15, a.sin() * 0.08);
            mb.cyl(v(0.0, 0.0, 0.0), top, 0.008, 0.006, lo, hi, 3);
            mb.ellipsoid(top, v(0.018, 0.045, 0.018), hex(0x6A4A2E));
        }
    }
}

fn canary_grass(mb: &mut Mb, y: Year, r: &mut Rng) {
    let (lo, hi) = (leaf(y, 0x78A850, 0x5A8C48, 0xB0A050, 0xC4B482), leaf(y, 0xB0D47A, 0x8CBA6A, 0xD0BC6A, 0xE0D2A6));
    let h = 0.25 + 0.5 * y.green.max(0.7 * y.dead);
    for k in 0..14 {
        let a = k as f32 * 2.399 + r.range(-0.3, 0.3);
        let out = v(a.cos(), 0.0, a.sin());
        blade(mb, out * 0.05, out, h * r.range(0.75, 1.1), 0.04, 0.25 + 0.4 * r.next() + 0.3 * y.dead, snowed(lo, y), snowed(hi, y));
    }
    let plume = mix(mix(hex(0x9A6A8A), hex(0xC8B47A), y.fruit), hex(0xD8CCA8), y.dead);
    if y.flower + y.fruit + y.dead > 0.2 {
        for k in 0..5 {
            let a = k as f32 * 1.26 + 0.5;
            let top = v(a.cos() * 0.12, h * 1.3, a.sin() * 0.12);
            mb.cyl(v(0.0, 0.0, 0.0), top, 0.01, 0.007, lo, hi, 3);
            mb.ellipsoid_r(top + v(0.0, 0.06, 0.0), v(0.03, 0.1, 0.03), Quat::from_rotation_z(0.15 * (k as f32 - 2.0)), plume);
        }
    }
}

fn forb(mb: &mut Mb, y: Year, r: &mut Rng, tall: f32, stem: u32, leafc: (u32, u32), bloom: u32, dome: bool) {
    let lc = leaf(y, 0x6EA050, leafc.0, 0x9A7A3A, 0x8A6E4E);
    let lc2 = leaf(y, 0x9CC874, leafc.1, 0xB89A50, 0xA88A64);
    let h = tall * (0.3 + 0.7 * y.green.max(0.8 * y.dead));
    let stem_c = mix(hex(stem), hex(0x7A6048), y.dead);
    for k in 0..5 {
        let a = k as f32 * 1.256 + r.range(-0.3, 0.3);
        let foot = v(a.cos() * 0.1, 0.0, a.sin() * 0.1);
        let top = foot + v(a.cos() * 0.06, h * r.range(0.8, 1.1), a.sin() * 0.06);
        mb.cyl(foot, top, 0.018, 0.012, stem_c, stem_c, 4);
        if y.green > 0.05 {
            for j in 1..4 {
                let p = foot.lerp(top, j as f32 * 0.24);
                for s in [-1.0f32, 1.0] {
                    let d = Quat::from_rotation_y(a + j as f32 * 1.1) * v(s, 0.0, 0.0);
                    mb.ellipsoid_r(p + d * 0.08 + v(0.0, 0.02, 0.0), v(0.09, 0.015, 0.035) * y.green.max(0.4), Quat::from_rotation_y(-(a + j as f32 * 1.1)) * Quat::from_rotation_z(-0.3 * s), mix(lc, lc2, 0.3 * j as f32));
                }
            }
        }
        let head = mix(mix(hex(0x6A8A48), hex(bloom), y.flower), hex(0x7A5A40), (1.0 - y.flower) * (y.fruit + y.dead).min(1.0));
        if y.flower > 0.05 || y.fruit + y.dead > 0.3 {
            let n = if dome { 7 } else { 6 };
            for i in 0..n {
                let b = i as f32 * TAU / n as f32 + a;
                let o = if dome { v(b.cos() * 0.06, 0.02, b.sin() * 0.06) } else { v(b.cos() * 0.05, r.range(-0.02, 0.05), b.sin() * 0.05) };
                let s = if dome { 0.05 } else { 0.04 };
                mb.ellipsoid_g(top + o, v(s, s * 0.8, s), mix(head, hex(0x3A3020), 0.15), head, 5, 3);
            }
            mb.ellipsoid_g(top + v(0.0, 0.04, 0.0), v(0.05, 0.04, 0.05), mix(head, hex(0x3A3020), 0.1), head, 5, 3);
        }
    }
}

fn raspberry(mb: &mut Mb, y: Year, r: &mut Rng) {
    let cane = mix(hex(0x7A5A40), hex(0x8E4A3A), y.dead);
    let lc = leaf(y, 0x7AAA58, 0x4E8040, 0xB0542E, 0x7A5040);
    let lc2 = leaf(y, 0xA8CC80, 0x76A85E, 0xD08A3A, 0x8A6448);
    for k in 0..6 {
        let a = k as f32 * 1.05 + r.range(-0.2, 0.2);
        let out = v(a.cos(), 0.0, a.sin());
        let foot = out * 0.06;
        let mid = foot + out * 0.2 + Vec3::Y * r.range(0.45, 0.6);
        let tip = mid + out * 0.25 - Vec3::Y * 0.18;
        mb.cyl(foot, mid, 0.018, 0.014, cane, cane, 4);
        mb.cyl(mid, tip, 0.014, 0.008, cane, cane, 4);
        if y.green > 0.05 {
            for j in 0..3 {
                let p = if j == 0 { foot.lerp(mid, 0.6) } else { mid.lerp(tip, j as f32 * 0.45) };
                let s = 0.09 * y.green.max(0.3);
                for (o, rot) in [(v(0.05, 0.0, 0.0), 0.0f32), (v(-0.03, 0.0, 0.04), 2.1), (v(-0.03, 0.0, -0.04), 4.2)] {
                    let q = Quat::from_rotation_y(a);
                    mb.ellipsoid_r(p + q * o, v(s, s * 0.22, s * 0.65), q * Quat::from_rotation_y(rot), mix(lc, lc2, 0.4 * j as f32));
                }
            }
        }
        if y.fruit > 0.2 {
            for j in 0..2 {
                let p = mid.lerp(tip, 0.3 + 0.4 * j as f32) - Vec3::Y * 0.05;
                mb.ellipsoid(p, v(0.03, 0.035, 0.03) * y.fruit, hex(0xC8202E));
            }
        } else if y.flower > 0.3 {
            mb.ellipsoid(mid.lerp(tip, 0.5), v(0.025, 0.02, 0.025), hex(0xF6F0E6));
        }
    }
}

fn horsetail(mb: &mut Mb, y: Year, r: &mut Rng) {
    let c = leaf(y, 0x7CB858, 0x4E9A48, 0x9AA050, 0xA08A60);
    let band = hex(0x2A2A22);
    let h = 0.2 + 0.32 * y.green.max(0.5 * y.dead);
    for k in 0..18 {
        let a = k as f32 * 2.399;
        let d = r.range(0.02, 0.18);
        let foot = v(a.cos() * d, -0.02, a.sin() * d);
        let top = foot + v(r.range(-0.03, 0.03), h * r.range(0.75, 1.15), r.range(-0.03, 0.03));
        mb.cyl(foot, top, 0.012, 0.008, c, mix(c, hex(0xffffff), 0.1), 3);
        for j in 1..4 {
            let p = foot.lerp(top, j as f32 * 0.25);
            mb.cyl(p, p + v(0.0, 0.012, 0.0), 0.014, 0.014, band, band, 3);
        }
    }
}

fn cattail(mb: &mut Mb, y: Year, r: &mut Rng) {
    let (lo, hi) = (leaf(y, 0x6E9E4E, 0x4E8040, 0xB89A48, 0xC8B488), leaf(y, 0xA4C878, 0x82AE62, 0xD8BC64, 0xDCCCA4));
    let h = 0.35 + 0.6 * y.green.max(0.75 * y.dead);
    for k in 0..12 {
        let a = k as f32 * 2.399 + r.range(-0.2, 0.2);
        let out = v(a.cos(), 0.0, a.sin());
        blade(mb, out * 0.05, out, h * r.range(0.8, 1.1), 0.05, 0.12 + 0.2 * r.next() + 0.45 * y.dead * r.next(), lo, hi);
    }
    if y.fruit + y.dead + y.fall > 0.2 || y.flower > 0.5 {
        let head = mix(mix(hex(0x5A3A24), hex(0x6E4A2C), y.fruit), hex(0xB8A488), y.dead * 0.6);
        let stalk = mix(hex(0x6E8A48), hex(0xB8A47A), y.dead);
        for k in 0..3 {
            let a = k as f32 * 2.1 + 0.4;
            let foot = v(a.cos() * 0.05, 0.0, a.sin() * 0.05);
            let top = foot + v(a.cos() * 0.04, h * 1.15, a.sin() * 0.04);
            mb.cyl(foot, top, 0.012, 0.01, stalk, stalk, 3);
            let fat = 1.0 + 0.35 * y.dead;
            mb.ellipsoid_g(top - v(0.0, 0.1, 0.0), v(0.035 * fat, 0.09, 0.035 * fat), mix(head, hex(0x2A1A10), 0.3), head, 7, 4);
            mb.cyl(top, top + v(0.0, 0.08, 0.0), 0.006, 0.002, stalk, stalk, 3);
        }
    }
}

fn arrowhead(mb: &mut Mb, y: Year, r: &mut Rng) {
    if y.green < 0.1 {
        return;
    }
    let c = leaf(y, 0x7AB65A, 0x4E9448, 0xA8A048, 0x8A7A50);
    let c2 = leaf(y, 0xA8D47E, 0x7AB868, 0xC8B860, 0xA08C60);
    let stem = mix(hex(0x5A8A40), hex(0x8A7A50), y.fall);
    let g = y.green.sqrt();
    for k in 0..6 {
        let a = k as f32 * 1.05 + r.range(-0.2, 0.2);
        let foot = v(a.cos() * 0.05, 0.0, a.sin() * 0.05);
        let top = foot + v(a.cos() * 0.08, r.range(0.22, 0.34) * g, a.sin() * 0.08);
        mb.cyl(foot, top, 0.012, 0.01, stem, stem, 3);
        let face = Quat::from_rotation_y(-a) * Quat::from_rotation_z(1.2);
        let s = 0.1 * g;
        mb.ellipsoid_r(top + face * v(s * 0.6, 0.0, 0.0), v(s, s * 0.08, s * 0.45), face, c2);
        for side in [-1.0f32, 1.0] {
            let lobe = face * Quat::from_rotation_y(side * 0.6);
            mb.ellipsoid_r(top + lobe * v(-s * 0.7, 0.0, 0.0), v(s * 0.8, s * 0.08, s * 0.25), lobe, c);
        }
    }
    if y.flower > 0.2 {
        let top = v(0.0, 0.4 * g, 0.0);
        mb.cyl(Vec3::ZERO, top, 0.01, 0.008, stem, stem, 3);
        for i in 0..3 {
            let b = i as f32 * TAU / 3.0;
            mb.ellipsoid_r(top + v(b.cos() * 0.03, 0.0, b.sin() * 0.03), v(0.035, 0.01, 0.025) * y.flower, Quat::from_rotation_y(-b), hex(0xFBFAF4));
        }
        mb.ellipsoid(top + v(0.0, 0.01, 0.0), v(0.012, 0.012, 0.012), hex(0xE8C040));
    }
}

fn lily(mb: &mut Mb, y: Year, r: &mut Rng, white: bool) {
    if y.green < 0.08 {
        mb.ellipsoid(v(0.0, -0.12, 0.0), v(0.08, 0.02, 0.08), hex(0x3A4A30));
        return;
    }
    let c = leaf(y, 0x6EA048, 0x3E7E3A, 0x9A8A3A, 0x7A6A40);
    let c2 = leaf(y, 0x8CBC60, 0x5A9A48, 0xB09A48, 0x8A7A48);
    let g = y.green.sqrt();
    let n = if white { 8 } else { 6 };
    for k in 0..n {
        let a = k as f32 * 2.399 + r.range(-0.3, 0.3);
        let d = if k == 0 { 0.0 } else { r.range(0.18, 0.42) };
        let rr = if white { r.range(0.1, 0.14) } else { r.range(0.13, 0.18) } * g;
        let tilt = if !white && k % 3 == 1 { 0.9 } else { 0.0 };
        let h = if tilt > 0.0 { 0.08 } else { 0.02 };
        pad(mb, v(a.cos() * d, h, a.sin() * d), rr, r.range(0.0, TAU), if white { 0.25 } else { 0.5 }, mix(c, c2, r.next()), tilt);
    }
    if y.flower > 0.15 {
        for (k, &(x, z)) in [(0.1f32, 0.05f32), (-0.22, 0.2)].iter().enumerate() {
            let s = y.flower * if k == 0 { 1.0 } else { 0.8 };
            let p = v(x, 0.05, z);
            if white {
                for ring in 0..2 {
                    for i in 0..8 {
                        let b = i as f32 * TAU / 8.0 + ring as f32 * 0.4;
                        let rad = 0.05 - 0.018 * ring as f32;
                        let q = Quat::from_rotation_y(-b) * Quat::from_rotation_z(0.35 + 0.35 * ring as f32);
                        mb.ellipsoid_r(p + v(b.cos() * rad, 0.015 * ring as f32, b.sin() * rad) * s, v(0.05, 0.01, 0.02) * s, q, hex(0xFCFAF6));
                    }
                }
                mb.ellipsoid(p + v(0.0, 0.025, 0.0) * s, v(0.02, 0.015, 0.02) * s, hex(0xF0C840));
            } else {
                mb.cyl(p - v(0.0, 0.05, 0.0), p + v(0.0, 0.06, 0.0), 0.01, 0.01, hex(0x5A8A40), hex(0x5A8A40), 3);
                mb.ellipsoid_g(p + v(0.0, 0.08, 0.0), v(0.045, 0.04, 0.045) * s, hex(0xC89A20), hex(0xF2D040), 7, 5);
            }
        }
    }
}

fn pondweed(mb: &mut Mb, y: Year, r: &mut Rng) {
    if y.green < 0.08 {
        mb.ellipsoid(v(0.0, -0.12, 0.0), v(0.06, 0.02, 0.06), hex(0x3A4A30));
        return;
    }
    let c = leaf(y, 0x8AA850, 0x6A8A3A, 0x9A8040, 0x7A6A40);
    let c2 = leaf(y, 0xA8C06A, 0x8AA452, 0xB09450, 0x8A7A48);
    let g = y.green.sqrt();
    for k in 0..26 {
        let a = k as f32 * 2.399;
        let d = 0.45 * (k as f32 / 26.0).sqrt() * g + r.range(-0.03, 0.03);
        let p = v(a.cos() * d, 0.02, a.sin() * d);
        mb.ellipsoid_r(p, v(0.07, 0.008, 0.032) * g.max(0.5), Quat::from_rotation_y(r.range(0.0, TAU)), mix(c, c2, r.next()));
    }
    if y.flower > 0.2 {
        for k in 0..3 {
            let a = k as f32 * 2.1;
            let p = v(a.cos() * 0.15, 0.0, a.sin() * 0.15);
            mb.cyl(p, p + v(0.0, 0.1, 0.0), 0.01, 0.01, c, c, 3);
            mb.ellipsoid(p + v(0.0, 0.12, 0.0), v(0.018, 0.04, 0.018), hex(0x7A8A3A));
        }
    }
}

pub fn plant_mesh(p: Plant, season: Season) -> Mesh {
    let mut mb = Mb::new();
    let y = year(season.phase());
    let mut r = Rng(p.index() as u32 * 977 + 31);
    let l = &p.row().look;
    match l.model.as_str() {
        "sedge" => sedge(&mut mb, y, &mut r),
        "canary_grass" => canary_grass(&mut mb, y, &mut r),
        "forb" => forb(&mut mb, y, &mut r, l.height, l.stem, l.leaf, l.flower, l.variant),
        "raspberry" => raspberry(&mut mb, y, &mut r),
        "horsetail" => horsetail(&mut mb, y, &mut r),
        "cattail" => cattail(&mut mb, y, &mut r),
        "arrowhead" => arrowhead(&mut mb, y, &mut r),
        "lily" => lily(&mut mb, y, &mut r, l.variant),
        "pondweed" => pondweed(&mut mb, y, &mut r),
        m => panic!("plant {}: no plant model {m}", p.key()),
    }
    if mb.len() == 0 {
        mb.ellipsoid(v(0.0, -0.12, 0.0), v(0.05, 0.02, 0.05), hex(0x3A4A30));
    }
    mb.build()
}
