//! Rock and clay: the boulders of rocky outcrops (scenery, never gathered), the pebbles that
//! are the resource (heaps of small mixed stones on land, gravel bars in the streams), and clay
//! (wet heaps on the banks, slick grey seams where a river lays it down).
use super::meshgen::{hex, mix, Mb, Rng};
use bevy::prelude::*;
use std::f32::consts::TAU;

fn v(x: f32, y: f32, z: f32) -> Vec3 { Vec3::new(x, y, z) }

/// Shapes of boulder the outcrops pick from.
pub const BOULDER_VARIANTS: usize = 8;

/// A boulder about one unit across, bedded in the ground at its origin: no two alike. Granite,
/// banded gneiss or dark rock; rounded by ice or broken blocky; lichen on what faces the sky and
/// moss on some tops.
pub fn boulder_mesh(variant: usize) -> Mesh {
    let mut mb = Mb::new();
    let mut r = Rng(variant as u32 * 7919 + 101);
    let (base, light, banded) = match variant % 4 {
        0 => (0x8A7C78, 0xC4B4AC, false), // pink-grey granite
        1 => (0x7A7A7E, 0xB6B4B0, true),  // banded gneiss
        2 => (0x5E6064, 0x8E9090, false), // dark rock
        _ => (0x8E8A80, 0xC2BCB0, false), // pale weathered stone
    };
    let blocky = variant % 3 == 1;
    let squat = 0.55 + 0.25 * r.next();
    let (rx, rz) = (0.5 * r.range(0.85, 1.15), 0.5 * r.range(0.75, 1.05));
    let start = mb.len();
    mb.ellipsoid_g(v(0.0, 0.12, 0.0), v(rx, squat, rz), hex(base), hex(light), 14, 9);
    // Broken faces: planes the rock is cut back to, then a little roughness over all.
    let planes: Vec<(Vec3, f32)> = (0..if blocky { 7 } else { 4 })
        .map(|_| {
            let n = v(r.range(-1.0, 1.0), r.range(-0.2, 1.0), r.range(-1.0, 1.0)).normalize_or(Vec3::Y);
            (n, r.range(0.32, 0.45) * if blocky { 0.9 } else { 1.1 })
        })
        .collect();
    let ph = r.range(0.0, TAU);
    mb.displace(start, |p| {
        let mut q = p - v(0.0, 0.12, 0.0);
        for &(n, d) in &planes {
            let over = q.dot(n) - d;
            if over > 0.0 {
                q -= n * over * 0.85;
            }
        }
        let k = 1.0 + 0.06 * (q.x * 9.0 + ph).sin() * (q.z * 8.0 - ph).cos() + 0.03 * (q.y * 17.0 + ph).sin();
        let mut out = q * k + v(0.0, 0.12, 0.0);
        // Bedded: the underside sinks flat into the ground.
        if out.y < -0.05 {
            out.y = -0.05 - (out.y + 0.05) * 0.2;
        }
        out
    });
    mb.renormal(start);
    let lichen = [hex(0xB8C27E), hex(0xD9A04E), hex(0xC8CCB4)];
    let moss = variant % 4 == 3 || variant == 5;
    let (dark, pale) = (hex(base), hex(light));
    mb.paint(start, move |p, n, c| {
        let mut c = mix(c, if n.y > 0.4 { pale } else { dark }, 0.35);
        if banded {
            let band = ((p.y * 11.0 + p.x * 3.0).sin() * 0.5 + 0.5).powi(3);
            c = mix(c, hex(0x4A4A50), band * 0.5);
        }
        // Speckle, so it reads as stone and not cement.
        let s = ((p.x * 37.0).sin() * (p.z * 41.0).cos() * (p.y * 29.0).sin()).abs();
        c = mix(c, if s > 0.6 { hex(0x3A3A3E) } else { pale }, 0.12 * s);
        let blot = (p.x * 6.3 + 1.7).sin() * (p.z * 5.9 - 0.4).cos() + 0.4 * (p.y * 7.0).sin();
        if n.y > 0.35 && blot > 0.55 {
            c = mix(c, lichen[((p.x * 3.0).abs() as usize) % 3], 0.6);
        }
        if moss && n.y > 0.75 && blot > -0.1 {
            c = mix(c, hex(0x5E7E3A), 0.7);
        }
        // Darker where it meets the ground.
        mix(c, hex(0x3A342C), (1.0 - (p.y + 0.05) / 0.25).clamp(0.0, 1.0) * 0.45)
    });
    mb.build()
}

/// One small stone: rounded, its colour from the mix a river or outcrop gives.
fn stone(mb: &mut Mb, c: Vec3, s: f32, yaw: f32, col: [f32; 3], r: &mut Rng) {
    let rot = Quat::from_rotation_y(yaw) * Quat::from_rotation_z(r.range(-0.4, 0.4));
    mb.ellipsoid_m(Mat4::from_rotation_translation(rot, c) * Mat4::from_scale(v(s * r.range(1.0, 1.5), s * r.range(0.55, 0.8), s)), 6, 4, mix(col, hex(0x2E2A26), 0.3), col);
}

/// The colours pebbles come in: grey, warm tan, rust, white quartz, slate.
fn pebble_colour(r: &mut Rng, bleached: bool) -> [f32; 3] {
    let cols = [0x9A968E, 0xB8A890, 0x9A6A4A, 0xE6E0D6, 0x6E7680, 0x86827A, 0xC8BCA8];
    let c = hex(cols[(r.next() * cols.len() as f32) as usize % cols.len()]);
    if bleached { mix(c, hex(0xD8D4CC), 0.3) } else { c }
}

/// Pebbles heaped on the ground: a gravelly patch and on it a low heap of small mixed stones,
/// as many as are left (`q` quarters). Plainly smaller than any boulder.
pub fn pebble_heap_mesh(q: u8) -> Mesh {
    let mut mb = Mb::new();
    let mut r = Rng(4040 + q as u32);
    mb.disc(v(0.0, 0.012, 0.0), 0.5, 0.42, hex(0xA49C8E), 16);
    let n = 10 + 12 * q as usize;
    let top = 0.1 + 0.07 * q as f32;
    for i in 0..n {
        let a = i as f32 * 2.399 + r.range(-0.2, 0.2);
        let d = 0.42 * ((i as f32 + 0.5) / n as f32).sqrt();
        let h = top * (1.0 - (d / 0.45).powi(2)).max(0.0);
        let s = r.range(0.035, 0.075);
        stone(&mut mb, v(a.cos() * d, h + s * 0.4, a.sin() * d * 0.9), s, r.range(0.0, TAU), pebble_colour(&mut r, false), &mut r);
    }
    mb.build()
}

/// How high a gravel bar stands above its foot.
pub const GRAVEL_BAR_H: f32 = 0.35;

/// A gravel bar along a stream: a low ridge of washed, rounded stones heaped up from the bed,
/// its top `GRAVEL_BAR_H` above its foot, so it breaks the surface of a shallow stream.
pub fn gravel_bar_mesh(q: u8) -> Mesh {
    let mut mb = Mb::new();
    let mut r = Rng(5050 + q as u32);
    let start = mb.len();
    mb.ellipsoid_g(v(0.0, 0.0, 0.0), v(0.55, GRAVEL_BAR_H, 0.32), hex(0x7A7468), hex(0xB4AC9C), 12, 6);
    mb.displace(start, |p| v(p.x, p.y.max(-0.05), p.z));
    mb.renormal(start);
    let n = 8 + 10 * q as usize;
    for i in 0..n {
        let a = i as f32 * 2.399;
        let d = ((i as f32 + 0.5) / n as f32).sqrt();
        let (x, z) = (a.cos() * d * 0.5, a.sin() * d * 0.28);
        let h = (1.0 - (x / 0.55).powi(2) - (z / 0.32).powi(2)).max(0.0).sqrt();
        let s = r.range(0.04, 0.08);
        stone(&mut mb, v(x, h * GRAVEL_BAR_H * 0.96, z), s, r.range(0.0, TAU), pebble_colour(&mut r, true), &mut r);
    }
    mb.build()
}

/// A clay deposit on the bank: a wet, glistening heap of grey-brown clay over a dark puddle of
/// slick mud, a scoop dug out of one side showing the paler clay inside, and a few lumps rolled
/// ready to carry. Wet clay reads apart from the dark earth and from the dams' mud.
pub fn clay_heap_mesh() -> Mesh {
    let mut mb = Mb::new();
    let mut r = Rng(6060);
    let (wet, clay, pale) = (hex(0x3A2E26), hex(0x645244), hex(0x8E7C66));
    // The slick puddle round it.
    let start = mb.len();
    mb.ellipsoid_g(v(0.0, -0.02, 0.0), v(0.62, 0.05, 0.5), wet, mix(wet, clay, 0.3), 14, 4);
    mb.displace(start, |p| {
        let a = p.z.atan2(p.x);
        let k = 1.0 + 0.12 * (3.0 * a + 0.7).sin() + 0.06 * (7.0 * a).cos();
        v(p.x * k, p.y, p.z * k)
    });
    // The heap.
    let start = mb.len();
    mb.ellipsoid_g(v(0.05, 0.02, 0.0), v(0.42, 0.26, 0.34), clay, pale, 14, 8);
    mb.displace(start, |p| {
        let q = p - v(0.05, 0.02, 0.0);
        let a = q.z.atan2(q.x);
        let k = 1.0 + 0.08 * (4.0 * a + q.y * 6.0).sin() + 0.05 * (9.0 * a).cos();
        // The scoop: cut back on one side.
        let scoop = (-(q.x + 0.25).powi(2) * 18.0 - (q.z - 0.05).powi(2) * 12.0).exp() * 0.12;
        v(0.05 + q.x * k + scoop, q.y * k * (1.0 - scoop * 2.0), q.z * k)
    });
    mb.renormal(start);
    // Drying cracks across the top.
    for k in 0..5 {
        let a = k as f32 * 1.3 + 0.4;
        let p = v(0.05 + a.cos() * 0.15, 0.25 - 0.02 * k as f32, a.sin() * 0.12);
        mb.ellipsoid_r(p, v(0.09, 0.006, 0.008), Quat::from_rotation_y(a + 0.8), hex(0x5A4A3A));
    }
    // Lumps rolled and set aside.
    for k in 0..3 {
        let a = 3.6 + k as f32 * 0.5;
        let s = r.range(0.07, 0.09);
        mb.ellipsoid_g(v(a.cos() * 0.5, s * 0.8, a.sin() * 0.45), v(s, s * 0.85, s), clay, mix(clay, pale, 0.6), 8, 5);
    }
    mb.build()
}

/// River clay: a smooth, slick grey seam the current lays along the bank, its edge dark with
/// water, showing grey and ochre layers where it is cut.
pub fn river_clay_mesh() -> Mesh {
    let mut mb = Mb::new();
    let (grey, slick, ochre) = (hex(0x6E7274), hex(0xA2A6A4), hex(0xA0804E));
    let start = mb.len();
    mb.ellipsoid_g(v(0.0, -0.02, 0.0), v(0.6, 0.16, 0.42), mix(grey, hex(0x2E3032), 0.4), slick, 16, 6);
    mb.displace(start, |p| {
        let a = p.z.atan2(p.x);
        let k = 1.0 + 0.1 * (2.0 * a + 1.1).sin() + 0.05 * (5.0 * a).cos();
        v(p.x * k, p.y * (1.0 + 0.4 * (p.x * 2.0).sin().max(0.0)), p.z * k)
    });
    mb.renormal(start);
    mb.paint(start, move |p, _, c| {
        // Layers where it is cut: bands of ochre in the grey.
        let band = ((p.y * 70.0).sin() * 0.5 + 0.5).powi(4);
        mix(c, ochre, band * 0.35 * (1.0 - (p.y / 0.15).clamp(0.0, 1.0)))
    });
    for k in 0..4 {
        let a = k as f32 * 1.6 + 0.3;
        mb.ellipsoid_g(v(a.cos() * 0.3, 0.12, a.sin() * 0.2), v(0.12, 0.05, 0.09), grey, slick, 8, 4);
    }
    mb.build()
}
