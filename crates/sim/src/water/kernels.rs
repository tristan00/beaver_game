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

/// Faces between cells `a[i]` and `b[i]`; `q_before`/`q_after` are the old discharges of the
/// faces on either side along the same direction.
#[allow(clippy::too_many_arguments)]
pub fn faces(out: &mut [f32], q: &[f32], q_before: &[f32], q_after: &[f32], za: &[f32], zb: &[f32], ha: &[f32], hb: &[f32], fa: &[f32], fb: &[f32], mask: &[f32], p: Params) {
    let m = out.len();
    let (q, q_before, q_after, za, zb, ha, hb, fa, fb, mask) =
        (&q[..m], &q_before[..m], &q_after[..m], &za[..m], &zb[..m], &ha[..m], &hb[..m], &fa[..m], &fb[..m], &mask[..m]);
    for i in 0..m {
        out[i] = face(q[i], q_before[i], q_after[i], za[i], zb[i], ha[i], hb[i], fa[i], fb[i], mask[i], p);
    }
}

/// Outflow limiter: the share of each cell's outflow it can actually supply this step.
#[allow(clippy::too_many_arguments)]
pub fn limiter(fac: &mut [f32], h: &[f32], src: &[f32], qw: &[f32], qe: &[f32], qn: &[f32], qs: &[f32], member: &[f32], k: f32, dt: f32) {
    let m = fac.len();
    let (h, src, qw, qe, qn, qs, member) = (&h[..m], &src[..m], &qw[..m], &qe[..m], &qn[..m], &qs[..m], &member[..m]);
    for i in 0..m {
        let out = k * (qe[i].max(0.0) - qw[i].min(0.0) + qs[i].max(0.0) - qn[i].min(0.0));
        let have = h[i] + src[i] * dt;
        let f = if out > have { have / out.max(1.0e-30) } else { 1.0 };
        fac[i] = if member[i] > 0.0 { 1.0 } else { f };
    }
}

/// Mass balance of interior cells. Returns the largest new depth of a non-pool, non-lake cell (as
/// bits, which order like the values for non-negative floats) and how many cells became wet.
#[allow(clippy::too_many_arguments)]
pub fn continuity(
    h: &mut [f32],
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
) -> (u32, u32) {
    let m = h.len();
    let (z, src, lake, member, qw, qe, qn, qs) = (&z[..m], &src[..m], &lake[..m], &member[..m], &qw[..m], &qe[..m], &qn[..m], &qs[..m]);
    let (f_c, f_w, f_e, f_n, f_s, lake_delta) = (&f_c[..m], &f_w[..m], &f_e[..m], &f_n[..m], &f_s[..m], &mut lake_delta[..m]);
    let (mut hmax, mut newly) = (0u32, 0u32);
    for i in 0..m {
        let fw = qw[i] * if qw[i] > 0.0 { f_w[i] } else { f_c[i] };
        let fe = qe[i] * if qe[i] > 0.0 { f_c[i] } else { f_e[i] };
        let fnn = qn[i] * if qn[i] > 0.0 { f_n[i] } else { f_c[i] };
        let fs = qs[i] * if qs[i] > 0.0 { f_c[i] } else { f_s[i] };
        let old = h[i];
        let hh = (old + k * (fw - fe + fnn - fs) + src[i] * dt).max(0.0);
        // Lake cells hold their level (NaN elsewhere, so the comparison fails off-lake).
        let is_lake = lake[i] > -1.0e30;
        let want = (lake[i] - z[i]).max(0.0);
        let hh = if is_lake { want } else { hh };
        lake_delta[i] = if is_lake { want - (old + k * (fw - fe + fnn - fs)).max(0.0) } else { 0.0 };
        let pool = member[i] > 0.0;
        let hh = if pool { old } else { hh };
        h[i] = hh;
        let counted = !(is_lake || pool);
        hmax = hmax.max(if counted { hh.to_bits() } else { 0 });
        newly += (old <= wet_seed && hh > wet_seed) as u32;
    }
    (hmax, newly)
}

#[cfg(test)]
mod bench {
    use super::*;
    #[test]
    #[ignore]
    fn bench_kernels() {
        let m = 4096;
        let mut out = vec![0.0f32; m];
        let q: Vec<f32> = (0..m + 1).map(|i| (i as f32 * 0.37).sin() * 0.05).collect();
        let z: Vec<f32> = (0..m + 1).map(|i| 10.0 - i as f32 * 0.01 + (i as f32 * 1.3).sin() * 0.05).collect();
        let h: Vec<f32> = (0..m + 1).map(|i| 0.1 + (i as f32 * 0.7).cos().abs() * 0.2).collect();
        let f = vec![1.0f32; m + 1];
        let mask = vec![1.0f32; m];
        let p = Params { gdt: 9.81 * 0.3, fric: 9.81 * 0.3 * 0.0016, inv_dx: 1.0 };
        let reps = 20000;
        let t = std::time::Instant::now();
        for _ in 0..reps {
            faces(&mut out, &q, &q, &q, &z, &z[1..], &h, &h[1..], &f, &f[1..], &mask, p);
            std::hint::black_box(&mut out);
        }
        let ns = t.elapsed().as_secs_f64() * 1e9 / (reps * m) as f64;
        println!("faces: {ns:.2} ns/face");
        let mut fac = vec![0.0f32; m];
        let t = std::time::Instant::now();
        for _ in 0..reps {
            limiter(&mut fac, &h, &h, &q, &q[1..], &q, &q[1..], &mask, 0.3, 0.3);
            std::hint::black_box(&mut fac);
        }
        println!("limiter: {:.2} ns/cell", t.elapsed().as_secs_f64() * 1e9 / (reps * m) as f64);
        let mut hh = h.clone();
        let mut ld = vec![0.0f32; m];
        let lake = vec![f32::NAN; m];
        let zero = vec![0.0f32; m];
        let t = std::time::Instant::now();
        for _ in 0..reps {
            let r = continuity(&mut hh[..m - 1], &z, &zero, &lake, &zero, &q, &q[1..], &q, &q[1..], &f, &f, &f, &f, &f, &mut ld, 0.0, 0.3, 1e-3);
            std::hint::black_box(r);
        }
        println!("continuity: {:.2} ns/cell", t.elapsed().as_secs_f64() * 1e9 / (reps * m) as f64);
        for seg in [4usize, 8, 11, 16, 32] {
            let t = std::time::Instant::now();
            let mut calls = 0usize;
            for _ in 0..reps / 4 {
                let mut a = 0;
                while a + seg + 1 < m {
                    faces(&mut out[a..a + seg], &q[a..], &q[a..], &q[a + 1..], &z[a..], &z[a + 1..], &h[a..], &h[a + 1..], &f[a..], &f[a + 1..], &mask[a..], p);
                    a += seg + 3;
                    calls += 1;
                }
                std::hint::black_box(&mut out);
            }
            let ns = t.elapsed().as_secs_f64() * 1e9;
            println!("faces in segments of {seg}: {:.1} ns/call, {:.2} ns/face", ns / calls as f64, ns / (calls * seg) as f64);
        }
    }
}
