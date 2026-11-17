//! Tiny procedural mesh toolkit: lathes, ellipsoids, cylinders and flat-shaded hexahedra, all with
//! vertex colours so one white material draws every model (which lets Bevy batch/instance them).
use bevy::asset::RenderAssetUsages;
use bevy::mesh::Indices;
use bevy::prelude::*;
use bevy::render::render_resource::PrimitiveTopology;

/// Linear-RGB colour from an sRGB hex value (so palettes can be picked by eye).
pub fn hex(rgb: u32) -> [f32; 3] {
    let ch = |s: u32| (((rgb >> s) & 0xff) as f32 / 255.0).powf(2.2);
    [ch(16), ch(8), ch(0)]
}

pub fn mix(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t]
}

/// Deterministic tiny RNG for decoration jitter.
pub struct Rng(pub u32);
impl Rng {
    pub fn next(&mut self) -> f32 {
        self.0 = self.0.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        ((self.0 >> 8) & 0xffff) as f32 / 65535.0
    }
    pub fn range(&mut self, a: f32, b: f32) -> f32 { a + (b - a) * self.next() }
}

#[derive(Default)]
pub struct Mb {
    pos: Vec<[f32; 3]>,
    nor: Vec<[f32; 3]>,
    col: Vec<[f32; 4]>,
    idx: Vec<u32>,
}

impl Mb {
    pub fn new() -> Self { Self::default() }

    fn push(&mut self, m: &Mat4, nm: &Mat3, p: Vec3, n: Vec3, c: [f32; 3]) -> u32 {
        let i = self.pos.len() as u32;
        self.pos.push(m.transform_point3(p).to_array());
        self.nor.push((*nm * n).normalize_or_zero().to_array());
        self.col.push([c[0], c[1], c[2], 1.0]);
        i
    }

    /// Surface of revolution around local Y. `prof` runs bottom to top as (radius, y); the outside is
    /// on the right of the direction of travel. `smooth` shares ring normals (round), otherwise each
    /// profile segment is shaded flat (crisp rims and caps). Colour blends `ca`..`cb` along the profile.
    pub fn lathe(&mut self, m: Mat4, prof: &[(f32, f32)], seg: usize, ca: [f32; 3], cb: [f32; 3], smooth: bool) {
        let nm = Mat3::from_mat4(m).inverse().transpose();
        let n = prof.len();
        if n < 2 {
            return;
        }
        let snorm = |i: usize| {
            let (r0, y0) = prof[i];
            let (r1, y1) = prof[i + 1];
            let (dr, dy) = (r1 - r0, y1 - y0);
            let l = (dr * dr + dy * dy).sqrt().max(1e-6);
            (dy / l, -dr / l) // (radial, y)
        };
        let ring = |this: &mut Mb, r: f32, y: f32, nrm: (f32, f32), c: [f32; 3]| -> Vec<u32> {
            (0..=seg)
                .map(|j| {
                    let a = j as f32 / seg as f32 * std::f32::consts::TAU;
                    let (s, co) = a.sin_cos();
                    let nr = if r < 1e-4 { 0.0 } else { nrm.0 };
                    this.push(&m, &nm, Vec3::new(r * co, y, r * s), Vec3::new(nr * co, nrm.1, nr * s), c)
                })
                .collect()
        };
        let t_of = |i: usize| i as f32 / (n - 1) as f32;
        let mut bands: Vec<(Vec<u32>, Vec<u32>)> = Vec::new();
        if smooth {
            let rs: Vec<Vec<u32>> = (0..n)
                .map(|i| {
                    let a = if i == 0 { snorm(0) } else { snorm(i - 1) };
                    let b = if i == n - 1 { snorm(n - 2) } else { snorm(i) };
                    let nn = ((a.0 + b.0) * 0.5, (a.1 + b.1) * 0.5);
                    ring(self, prof[i].0, prof[i].1, nn, mix(ca, cb, t_of(i)))
                })
                .collect();
            for i in 0..n - 1 {
                bands.push((rs[i].clone(), rs[i + 1].clone()));
            }
        } else {
            for i in 0..n - 1 {
                let s = snorm(i);
                let lo = ring(self, prof[i].0, prof[i].1, s, mix(ca, cb, t_of(i)));
                let hi = ring(self, prof[i + 1].0, prof[i + 1].1, s, mix(ca, cb, t_of(i + 1)));
                bands.push((lo, hi));
            }
        }
        for (lo, hi) in bands {
            for j in 0..seg {
                let (a, b, c, d) = (lo[j], hi[j], lo[j + 1], hi[j + 1]);
                self.idx.extend_from_slice(&[a, b, c, c, b, d]);
            }
        }
    }

    pub fn ellipsoid_m(&mut self, m: Mat4, seg: usize, rings: usize, ca: [f32; 3], cb: [f32; 3]) {
        let prof: Vec<(f32, f32)> = (0..=rings)
            .map(|i| {
                let phi = -std::f32::consts::FRAC_PI_2 + std::f32::consts::PI * i as f32 / rings as f32;
                (phi.cos().max(0.0), phi.sin())
            })
            .collect();
        self.lathe(m, &prof, seg, ca, cb, true);
    }

    pub fn ellipsoid(&mut self, c: Vec3, r: Vec3, col: [f32; 3]) { self.ellipsoid_g(c, r, col, col, 10, 7) }

    /// Ellipsoid with a vertical colour gradient `ca` (bottom) to `cb` (top).
    pub fn ellipsoid_g(&mut self, c: Vec3, r: Vec3, ca: [f32; 3], cb: [f32; 3], seg: usize, rings: usize) {
        self.ellipsoid_m(Mat4::from_translation(c) * Mat4::from_scale(r), seg, rings, ca, cb);
    }

    /// Ellipsoid scaled then rotated about its centre.
    pub fn ellipsoid_r(&mut self, c: Vec3, r: Vec3, rot: Quat, col: [f32; 3]) {
        self.ellipsoid_m(Mat4::from_rotation_translation(rot, c) * Mat4::from_scale(r), 10, 7, col, col);
    }

    /// Capped (optionally tapered) cylinder from `a` to `b`.
    pub fn cyl(&mut self, a: Vec3, b: Vec3, r0: f32, r1: f32, ca: [f32; 3], cb: [f32; 3], seg: usize) {
        let d = b - a;
        let len = d.length();
        if len < 1e-5 {
            return;
        }
        let m = Mat4::from_rotation_translation(Quat::from_rotation_arc(Vec3::Y, d / len), a);
        self.lathe(m, &[(0.0, 0.0), (r0, 0.0), (r1, len), (0.0, len)], seg, ca, cb, false);
    }

    /// Cylinder with a different colour on the end caps (log ends, sawn stumps).
    pub fn log(&mut self, a: Vec3, b: Vec3, r: f32, bark: [f32; 3], end: [f32; 3], seg: usize) {
        let d = b - a;
        let len = d.length();
        if len < 1e-5 {
            return;
        }
        let m = Mat4::from_rotation_translation(Quat::from_rotation_arc(Vec3::Y, d / len), a);
        self.lathe(m, &[(0.0, 0.0), (r * 0.8, 0.0)], seg, end, end, false);
        self.lathe(m, &[(r * 0.8, 0.0), (r, 0.03), (r, len - 0.03), (r * 0.8, len)], seg, bark, bark, false);
        self.lathe(m, &[(r * 0.8, len), (0.0, len)], seg, end, end, false);
    }

    /// Flat-shaded hexahedron from 8 corners: bottom 0..4 as (-,-,-) (+,-,-) (+,-,+) (-,-,+), top 4..8 the same.
    pub fn hex(&mut self, p: [Vec3; 8], ca: [f32; 3], cb: [f32; 3]) {
        let faces: [[usize; 4]; 6] = [[0, 1, 2, 3], [7, 6, 5, 4], [0, 4, 5, 1], [1, 5, 6, 2], [2, 6, 7, 3], [3, 7, 4, 0]];
        let id = Mat4::IDENTITY;
        let im = Mat3::IDENTITY;
        for f in faces {
            let n = (p[f[1]] - p[f[0]]).cross(p[f[3]] - p[f[0]]);
            let base = self.pos.len() as u32;
            for &k in &f {
                let c = if k >= 4 { cb } else { ca };
                self.push(&id, &im, p[k], n, c);
            }
            self.idx.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
        }
    }

    /// Box centred on `c`, rotated by `rot`.
    pub fn cuboid(&mut self, c: Vec3, half: Vec3, rot: Quat, ca: [f32; 3], cb: [f32; 3]) {
        let s = |x: f32, y: f32, z: f32| c + rot * Vec3::new(x * half.x, y * half.y, z * half.z);
        self.hex(
            [s(-1., -1., -1.), s(1., -1., -1.), s(1., -1., 1.), s(-1., -1., 1.), s(-1., 1., -1.), s(1., 1., -1.), s(1., 1., 1.), s(-1., 1., 1.)],
            ca,
            cb,
        );
    }

    /// Flat horizontal disc, visible from above (ground shade, water).
    pub fn disc(&mut self, c: Vec3, rx: f32, rz: f32, col: [f32; 3], seg: usize) {
        let m = Mat4::from_translation(c) * Mat4::from_scale(Vec3::new(rx, 1.0, rz));
        self.lathe(m, &[(1.0, 0.0), (0.0, 0.0)], seg, col, col, false);
    }

    /// Appends another builder's geometry translated by `off`.
    pub fn append(&mut self, other: Mb, off: Vec3) {
        let base = self.pos.len() as u32;
        self.pos.extend(other.pos.iter().map(|p| (Vec3::from(*p) + off).to_array()));
        self.nor.extend(other.nor);
        self.col.extend(other.col);
        self.idx.extend(other.idx.iter().map(|i| i + base));
    }

    pub fn build(self) -> Mesh {
        Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
            .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, self.pos)
            .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, self.nor)
            .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, self.col)
            .with_inserted_indices(Indices::U32(self.idx))
    }
}
