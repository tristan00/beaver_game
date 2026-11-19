//! Branch-free inner loops of the local-inertial solver, written over equal-length slices so the
//! compiler turns them into SIMD.

pub const G: f32 = 9.81;
pub const FACE_DRY: f32 = 1.0e-4;
/// Froude number cap on any face (supercritical flow on 1 m cells is numerically fragile).
pub const FROUDE_MAX: f32 = 1.0;
/// Critical flow over a free overfall: q = 1.705 H^1.5 for a head H over the crest.
pub const WEIR_COEF: f32 = 1.705;
/// de Almeida et al. (2012) weighting: a little of the neighbouring faces' discharge damps the
/// checkerboard sloshing the plain local-inertial scheme leaves undamped.
pub const THETA: f32 = 0.8;

#[derive(Clone, Copy)]
pub struct Params {
    pub gdt: f32,
    pub fric: f32,
    pub inv_dx: f32,
}

#[inline(always)]
fn cbrt(x: f32) -> f32 {
    // Bit-level first guess (exponent / 3) done in float so it vectorises, then two Newton steps.
    let y = f32::from_bits(((((x.to_bits() as i32) as f32) * (1.0 / 3.0)) as i32 + 709_921_077) as u32);
    let y = y - (y * y * y - x) / (3.0 * y * y);
    y - (y * y * y - x) / (3.0 * y * y)
}

/// Local-inertial momentum update of one face (Bates et al. 2010) with semi-implicit Manning
/// friction, a Froude cap, a weir cap at free overfalls, and no outflow from a dry cell.
#[inline(always)]
#[allow(clippy::too_many_arguments)]
pub fn face(q_old: f32, q_before: f32, q_after: f32, za: f32, zb: f32, ha: f32, hb: f32, fa: f32, fb: f32, mask: f32, p: Params) -> f32 {
    // A neighbour face carrying nothing (dry, closed, inside a pool) is not evidence of slower
    // flow here; smooth only against faces that carry water.
    let qb = if q_before == 0.0 { q_old } else { q_before };
    let qa = if q_after == 0.0 { q_old } else { q_after };
    let q = THETA * q_old + 0.5 * (1.0 - THETA) * (qb + qa);
    let q = q * if q_old > 0.0 { fa } else { fb };
    let (ea, eb) = (za + ha, zb + hb);
    let hf = ea.max(eb) - za.max(zb);
    let hc = hf.max(FACE_DRY);
    let slope = (eb - ea) * p.inv_dx;
    let hf73 = hc * hc * cbrt(hc);
    let qn = (q - p.gdt * hc * slope) / (1.0 + p.fric * q.abs() / hf73);
    let overfall = ((qn > 0.0) & (eb < za)) | ((qn <= 0.0) & (ea < zb));
    let coef = if overfall { WEIR_COEF } else { FROUDE_MAX * 3.132_092 };
    let cap = coef * hc * hc.sqrt();
    let qn = qn.max(-cap).min(cap);
    let from_dry = ((qn > 0.0) & (ha <= 0.0)) | ((qn < 0.0) & (hb <= 0.0));
    if (hf <= FACE_DRY) | from_dry { 0.0 } else { qn * mask }
}

/// Lanes are computed in whole SIMD groups: a run s..e is widened to a multiple of `W` lanes
/// (shifted back to stay inside 0..len), so the vector loop never leaves lanes to slow scalar
/// code. Lanes outside s..e get computed too, so every kernel must be a pure function of its
/// inputs per lane (outputs go to scratch or are idempotent).
const W: usize = 8;

#[inline(always)]
fn widen(s: usize, e: usize, len: usize) -> (usize, usize) {
    if e <= s {
        return (s, 0);
    }
    let count = ((e - s).div_ceil(W) * W).min(len);
    (s.min(len - count), count)
}

/// Faces between cells `a[i]` and `b[i]` for lanes s..e of a row of `len` faces;
/// `q_before`/`q_after` are the old discharges of the faces on either side.
#[inline(always)]
#[allow(clippy::too_many_arguments)]
fn faces_impl(out: &mut [f32], q: &[f32], q_before: &[f32], q_after: &[f32], za: &[f32], zb: &[f32], ha: &[f32], hb: &[f32], fa: &[f32], fb: &[f32], mask: &[f32], act_a: &[f32], act_b: &[f32], s: usize, e: usize, len: usize, p: Params) {
    let (c, m) = widen(s, e, len);
    let out = &mut out[c..c + m];
    let (q, q_before, q_after, za, zb, ha, hb, fa, fb, mask, act_a, act_b) =
        (&q[c..c + m], &q_before[c..c + m], &q_after[c..c + m], &za[c..c + m], &zb[c..c + m], &ha[c..c + m], &hb[c..c + m], &fa[c..c + m], &fb[c..c + m], &mask[c..c + m], &act_a[c..c + m], &act_b[c..c + m]);
    for i in 0..m {
        out[i] = face(q[i], q_before[i], q_after[i], za[i], zb[i], ha[i], hb[i], fa[i], fb[i], mask[i] * act_a[i] * act_b[i], p);
    }
}

/// Outflow limiter: the share of a cell's outflow it can actually supply this step.
#[inline(always)]
#[allow(clippy::too_many_arguments)]
fn limit(h: f32, src: f32, qw: f32, qe: f32, qn: f32, qs: f32, member: f32, k: f32, dt: f32) -> f32 {
    let out = k * (qe.max(0.0) - qw.min(0.0) + qs.max(0.0) - qn.min(0.0));
    let have = h + src * dt;
    let f = if out > have { have / out.max(1.0e-30) } else { 1.0 };
    if member > 0.0 { 1.0 } else { f }
}

#[inline(always)]
#[allow(clippy::too_many_arguments)]
fn limiter_impl(fac: &mut [f32], h: &[f32], src: &[f32], qw: &[f32], qe: &[f32], qn: &[f32], qs: &[f32], member: &[f32], k: f32, dt: f32, s: usize, e: usize, len: usize) {
    let (c, m) = widen(s, e, len);
    let fac = &mut fac[c..c + m];
    let (h, src, qw, qe, qn, qs, member) = (&h[c..c + m], &src[c..c + m], &qw[c..c + m], &qe[c..c + m], &qn[c..c + m], &qs[c..c + m], &member[c..c + m]);
    for i in 0..m {
        fac[i] = limit(h[i], src[i], qw[i], qe[i], qn[i], qs[i], member[i], k, dt);
    }
}

/// One cell's mass balance: (new depth, lake booking, depth counted toward the step size as bits,
/// became wet).
#[inline(always)]
#[allow(clippy::too_many_arguments)]
fn mass(old: f32, z: f32, src: f32, lake: f32, member: f32, qw: f32, qe: f32, qn: f32, qs: f32, f_c: f32, f_w: f32, f_e: f32, f_n: f32, f_s: f32, k: f32, dt: f32, wet_seed: f32) -> (f32, f32, u32, u32) {
    let fw = qw * if qw > 0.0 { f_w } else { f_c };
    let fe = qe * if qe > 0.0 { f_c } else { f_e };
    let fnn = qn * if qn > 0.0 { f_n } else { f_c };
    let fs = qs * if qs > 0.0 { f_c } else { f_s };
    let flowed = (old + k * (fw - fe + fnn - fs)).max(0.0);
    let hh = (old + k * (fw - fe + fnn - fs) + src * dt).max(0.0);
    // Lake cells hold their level (NaN elsewhere, so the comparison fails off-lake).
    let is_lake = lake > -1.0e30;
    let want = (lake - z).max(0.0);
    let hh = if is_lake { want } else { hh };
    let booked = if is_lake { want - flowed } else { 0.0 };
    let pool = member > 0.0;
    let hh = if pool { old } else { hh };
    let counted = if is_lake | pool { 0 } else { hh.to_bits() };
    (hh, booked, counted, ((old <= wet_seed) & (hh > wet_seed)) as u32)
}

/// Mass balance of lanes s..e: new depths go to `h_out` (a scratch row: lanes may run twice).
/// Returns the largest new depth of a non-pool, non-lake cell (as bits, which order like the
/// values for non-negative floats) and how many cells became wet.
#[inline(always)]
#[allow(clippy::too_many_arguments)]
fn continuity_impl(
    h_out: &mut [f32],
    h: &[f32],
    z: &[f32],
    src: &[f32],
    lake: &[f32],
    member: &[f32],
    qw: &[f32],
    qe: &[f32],
    qn: &[f32],
    qs: &[f32],
    f_c: &[f32],
    f_w: &[f32],
    f_e: &[f32],
    f_n: &[f32],
    f_s: &[f32],
    lake_delta: &mut [f32],
    k: f32,
    dt: f32,
    wet_seed: f32,
    s: usize,
    e: usize,
    len: usize,
) -> (u32, u32) {
    let (c, m) = widen(s, e, len);
    let (h_out, lake_delta) = (&mut h_out[c..c + m], &mut lake_delta[c..c + m]);
    let (h, z, src, lake, member) = (&h[c..c + m], &z[c..c + m], &src[c..c + m], &lake[c..c + m], &member[c..c + m]);
    let (qw, qe, qn, qs) = (&qw[c..c + m], &qe[c..c + m], &qn[c..c + m], &qs[c..c + m]);
    let (f_c, f_w, f_e, f_n, f_s) = (&f_c[c..c + m], &f_w[c..c + m], &f_e[c..c + m], &f_n[c..c + m], &f_s[c..c + m]);
    let (mut hmax, mut newly) = (0u32, 0u32);
    for i in 0..m {
        let (hh, booked, counted, wet) = mass(h[i], z[i], src[i], lake[i], member[i], qw[i], qe[i], qn[i], qs[i], f_c[i], f_w[i], f_e[i], f_n[i], f_s[i], k, dt, wet_seed);
        h_out[i] = hh;
        lake_delta[i] = booked;
        hmax = hmax.max(counted);
        newly += wet;
    }
    (hmax, newly)
}

/// The kernels compiled twice: for any x86-64 (SSE2) and for CPUs with AVX2, picked at run time.
/// No FMA, so both give bit-identical results.
fn avx2() -> bool {
    #[cfg(target_arch = "x86_64")]
    {
        std::arch::is_x86_feature_detected!("avx2")
    }
    #[cfg(not(target_arch = "x86_64"))]
    {
        false
    }
}

macro_rules! dispatch {
    ($name:ident, $avx:ident, $imp:ident, ($($arg:ident: $ty:ty),*) -> $ret:ty) => {
        #[cfg(target_arch = "x86_64")]
        #[target_feature(enable = "avx2")]
        #[allow(clippy::too_many_arguments)]
        unsafe fn $avx($($arg: $ty),*) -> $ret {
            $imp($($arg),*)
        }

        #[allow(clippy::too_many_arguments)]
        pub fn $name($($arg: $ty),*) -> $ret {
            #[cfg(target_arch = "x86_64")]
            if avx2() {
                return unsafe { $avx($($arg),*) };
            }
            $imp($($arg),*)
        }
    };
}

dispatch!(faces, faces_avx2, faces_impl, (out: &mut [f32], q: &[f32], q_before: &[f32], q_after: &[f32], za: &[f32], zb: &[f32], ha: &[f32], hb: &[f32], fa: &[f32], fb: &[f32], mask: &[f32], act_a: &[f32], act_b: &[f32], s: usize, e: usize, len: usize, p: Params) -> ());
dispatch!(limiter, limiter_avx2, limiter_impl, (fac: &mut [f32], h: &[f32], src: &[f32], qw: &[f32], qe: &[f32], qn: &[f32], qs: &[f32], member: &[f32], k: f32, dt: f32, s: usize, e: usize, len: usize) -> ());
dispatch!(continuity, continuity_avx2, continuity_impl, (h_out: &mut [f32], h: &[f32], z: &[f32], src: &[f32], lake: &[f32], member: &[f32], qw: &[f32], qe: &[f32], qn: &[f32], qs: &[f32], f_c: &[f32], f_w: &[f32], f_e: &[f32], f_n: &[f32], f_s: &[f32], lake_delta: &mut [f32], k: f32, dt: f32, wet_seed: f32, s: usize, e: usize, len: usize) -> (u32, u32));

/// Prints the face kernel's cost per face for long runs and for runs of a few lanes.
pub fn bench_faces() {
    let m = 4096;
    let mut out = vec![0.0f32; m + 16];
    let q: Vec<f32> = (0..m + 16).map(|i| (i as f32 * 0.37).sin() * 0.05).collect();
    let z: Vec<f32> = (0..m + 16).map(|i| 10.0 - i as f32 * 0.01 + (i as f32 * 1.3).sin() * 0.05).collect();
    let h: Vec<f32> = (0..m + 16).map(|i| 0.1 + (i as f32 * 0.7).cos().abs() * 0.2).collect();
    let f = vec![1.0f32; m + 16];
    let p = Params { gdt: 9.81 * 0.3, fric: 9.81 * 0.3 * 0.0016, inv_dx: 1.0 };
    let reps = 20000;
    let t = std::time::Instant::now();
    for _ in 0..reps {
        faces(&mut out, &q, &q, &q[1..], &z, &z[1..], &h, &h[1..], &f, &f[1..], &f, &f, &f, 0, m, m, p);
        std::hint::black_box(&mut out);
    }
    println!("faces, long run: {:.2} ns/face", t.elapsed().as_secs_f64() * 1e9 / (reps * m) as f64);
    for seg in [4usize, 11, 32] {
        let t = std::time::Instant::now();
        let mut calls = 0usize;
        for _ in 0..reps / 4 {
            let mut a = 0;
            while a + seg + 1 < m {
                faces(&mut out, &q, &q, &q[1..], &z, &z[1..], &h, &h[1..], &f, &f[1..], &f, &f, &f, a, a + seg, m, p);
                a += seg + 3;
                calls += 1;
            }
            std::hint::black_box(&mut out);
        }
        let ns = t.elapsed().as_secs_f64() * 1e9;
        println!("faces, runs of {seg}: {:.1} ns/call, {:.2} ns/face", ns / calls as f64, ns / (calls * seg) as f64);
    }
}
