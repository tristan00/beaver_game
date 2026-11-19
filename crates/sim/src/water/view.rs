//! The solver's per-substep passes over a band of rows. Bands run on different threads, so the
//! passes reach the water's arrays through raw pointers; each pass writes only its own rows.

use super::{edge_bed, kernels, Water, WET_SEED};

#[derive(Clone, Default)]
pub(super) struct BandOut {
    pub hmax: u32,
    pub lake_change: f64,
    pub edge: f64,
    pub grew: Vec<usize>,
}

pub(super) struct SendPtr<T>(pub *mut T);
unsafe impl<T> Sync for SendPtr<T> {}
unsafe impl<T> Send for SendPtr<T> {}
impl<T> SendPtr<T> {
    pub fn get(&self) -> *mut T {
        self.0
    }
}

pub(super) struct View {
    n: usize,
    h: *mut f32,
    z: *const f32,
    qx: *mut f32,
    qx_new: *mut f32,
    qy: *mut f32,
    qy_new: *mut f32,
    ox: *const f32,
    oy: *const f32,
    fac: *mut f32,
    src: *const f32,
    lake: *const f32,
    member: *const f32,
    lake_delta: *mut f32,
    rows: *const Vec<(u32, u32)>,
    yrows: *const Vec<(u32, u32)>,
    lake_rows: *const bool,
}

unsafe impl Sync for View {}
unsafe impl Send for View {}

impl View {
    pub fn of(w: &mut Water) -> View {
        View {
            n: w.n,
            h: w.h.as_mut_ptr(),
            z: w.z.as_ptr(),
            qx: w.qx.as_mut_ptr(),
            qx_new: w.qx_new.as_mut_ptr(),
            qy: w.qy.as_mut_ptr(),
            qy_new: w.qy_new.as_mut_ptr(),
            ox: w.ox.as_ptr(),
            oy: w.oy.as_ptr(),
            fac: w.fac.as_mut_ptr(),
            src: w.src.as_ptr(),
            lake: w.lake_level.as_slice().as_ptr(),
            member: w.member.as_ptr(),
            lake_delta: w.lake_delta.as_mut_ptr(),
            rows: w.rows.as_ptr(),
            yrows: w.yrows.as_ptr(),
            lake_rows: w.lake_rows.as_ptr(),
        }
    }

    // Slices from an offset to the end of each array.
    unsafe fn cells(&self, p: *const f32, i: usize) -> &[f32] {
        unsafe { std::slice::from_raw_parts(p.add(i), self.n * self.n - i) }
    }
    unsafe fn cells_mut(&self, p: *mut f32, i: usize, len: usize) -> &mut [f32] {
        unsafe { std::slice::from_raw_parts_mut(p.add(i), len) }
    }
    unsafe fn xf(&self, p: *const f32, f: usize) -> &[f32] {
        unsafe { std::slice::from_raw_parts(p.add(f), self.n * (self.n + 1) - f) }
    }
    unsafe fn yf(&self, p: *const f32, f: usize) -> &[f32] {
        unsafe { std::slice::from_raw_parts(p.add(f), (self.n + 1) * self.n - f) }
    }
    unsafe fn row(&self, y: usize) -> &[(u32, u32)] {
        unsafe { &*self.rows.add(y) }
    }
    unsafe fn yrow(&self, y: usize) -> &[(u32, u32)] {
        unsafe { &*self.yrows.add(y) }
    }

    /// Momentum on the x faces of rows y0..y1 and the y faces above them (and the map's bottom
    /// edge for the last band). y faces go to `qy_new`, since neighbouring rows read the old ones.
    pub unsafe fn faces(&self, y0: usize, y1: usize, p: kernels::Params, dx: f32) {
        unsafe {
            let n = self.n;
            let w1 = n + 1;
            for y in y0..y1 {
                let base = y * w1;
                let r = y * n;
                for &(a, b) in self.row(y) {
                    let (a, b) = (a as usize, b as usize);
                    let (lo, hi) = (a.max(1), b.min(n - 1));
                    if lo <= hi {
                        let m = hi - lo + 1;
                        kernels::faces(
                            self.cells_mut(self.qx_new, base + lo, m),
                            self.xf(self.qx, base + lo),
                            self.xf(self.qx, base + lo - 1),
                            self.xf(self.qx, base + lo + 1),
                            self.cells(self.z, r + lo - 1),
                            self.cells(self.z, r + lo),
                            self.cells(self.h, r + lo - 1),
                            self.cells(self.h, r + lo),
                            self.cells(self.fac, r + lo - 1),
                            self.cells(self.fac, r + lo),
                            self.xf(self.ox, base + lo),
                            p,
                        );
                    }
                    let z = self.cells(self.z, 0);
                    let (h, fac) = (self.cells(self.h, 0), self.cells(self.fac, 0));
                    if a == 0 {
                        let c = r;
                        let zg = edge_bed(z, c, c + 1, dx);
                        let q = *self.qx.add(base);
                        *self.qx_new.add(base) = kernels::face(q, q, q, zg, z[c], h[c], h[c], fac[c], fac[c], *self.ox.add(base), p).min(0.0);
                    }
                    if b == n {
                        let c = r + n - 1;
                        let zg = edge_bed(z, c, c - 1, dx);
                        let q = *self.qx.add(base + n);
                        *self.qx_new.add(base + n) = kernels::face(q, q, q, z[c], zg, h[c], h[c], fac[c], fac[c], *self.ox.add(base + n), p).max(0.0);
                    }
                    std::ptr::copy_nonoverlapping(self.qx_new.add(base + a), self.qx.add(base + a), b - a + 1);
                }
            }
            let last = if y1 == n { n + 1 } else { y1 };
            for y in y0..last {
                let base = y * n;
                for &(a, b) in self.yrow(y) {
                    let (a, b) = (a as usize, b as usize);
                    if y > 0 && y < n {
                        kernels::faces(
                            self.cells_mut(self.qy_new, base + a, b - a),
                            self.yf(self.qy, base + a),
                            self.yf(self.qy, base - n + a),
                            self.yf(self.qy, base + n + a),
                            self.cells(self.z, base - n + a),
                            self.cells(self.z, base + a),
                            self.cells(self.h, base - n + a),
                            self.cells(self.h, base + a),
                            self.cells(self.fac, base - n + a),
                            self.cells(self.fac, base + a),
                            self.yf(self.oy, base + a),
                            p,
                        );
                    } else {
                        let z = self.cells(self.z, 0);
                        let (h, fac) = (self.cells(self.h, 0), self.cells(self.fac, 0));
                        for x in a..b {
                            let f = base + x;
                            let q = *self.qy.add(f);
                            let o = *self.oy.add(f);
                            *self.qy_new.add(f) = if y == 0 {
                                let zg = edge_bed(z, x, x + n, dx);
                                kernels::face(q, q, q, zg, z[x], h[x], h[x], fac[x], fac[x], o, p).min(0.0)
                            } else {
                                let c = (n - 1) * n + x;
                                let zg = edge_bed(z, c, c - n, dx);
                                kernels::face(q, q, q, z[c], zg, h[c], h[c], fac[c], fac[c], o, p).max(0.0)
                            };
                        }
                    }
                }
            }
        }
    }

    /// Outflow limiter for the cells of rows y0..y1.
    pub unsafe fn limiter(&self, y0: usize, y1: usize, k: f32, dt: f32) {
        unsafe {
            let n = self.n;
            let w1 = n + 1;
            for y in y0..y1 {
                let r = y * n;
                for &(a, b) in self.row(y) {
                    let (a, b) = (a as usize, b as usize);
                    kernels::limiter(
                        self.cells_mut(self.fac, r + a, b - a),
                        self.cells(self.h, r + a),
                        self.cells(self.src, r + a),
                        self.xf(self.qx, y * w1 + a),
                        self.xf(self.qx, y * w1 + a + 1),
                        self.yf(self.qy_new, r + a),
                        self.yf(self.qy_new, r + n + a),
                        self.cells(self.member, r + a),
                        k,
                        dt,
                    );
                }
            }
        }
    }

    /// Mass for the cells of rows y0..y1, then the new y faces of the band become current.
    pub unsafe fn continuity(&self, y0: usize, y1: usize, k: f32, dt: f32) -> BandOut {
        unsafe {
            let n = self.n;
            let w1 = n + 1;
            let mut out = BandOut::default();
            for y in y0..y1 {
                let r = y * n;
                let mut row_new = 0u32;
                for &(a, b) in self.row(y) {
                    let (a, b) = (a as usize, b as usize);
                    // Split off the map-edge columns, whose outer face has no neighbour cell.
                    let mut parts: [(usize, usize); 3] = [(a, a), (a, b), (b, b)];
                    if a == 0 {
                        parts[0] = (0, 1);
                        parts[1].0 = 1;
                    }
                    if b == n && parts[1].1 > parts[1].0 {
                        parts[2] = (n - 1, n);
                        parts[1].1 = n - 1;
                    }
                    for (pa, pb) in parts {
                        if pb <= pa {
                            continue;
                        }
                        let i0 = r + pa;
                        let fc = self.cells(self.fac, i0);
                        let fw = if pa == 0 { fc } else { self.cells(self.fac, i0 - 1) };
                        let fe = if pb == n { fc } else { self.cells(self.fac, i0 + 1) };
                        let fnn = if y == 0 { fc } else { self.cells(self.fac, i0 - n) };
                        let fs = if y + 1 == n { fc } else { self.cells(self.fac, i0 + n) };
                        let (hm, nw) = kernels::continuity(
                            self.cells_mut(self.h, i0, pb - pa),
                            self.cells(self.z, i0),
                            self.cells(self.src, i0),
                            self.cells(self.lake, i0),
                            self.cells(self.member, i0),
                            self.xf(self.qx, y * w1 + pa),
                            self.xf(self.qx, y * w1 + pa + 1),
                            self.yf(self.qy_new, i0),
                            self.yf(self.qy_new, i0 + n),
                            fc,
                            fw,
                            fe,
                            fnn,
                            fs,
                            self.cells_mut(self.lake_delta, i0, pb - pa),
                            k,
                            dt,
                            WET_SEED,
                        );
                        out.hmax = out.hmax.max(hm);
                        row_new += nw;
                    }
                    if *self.lake_rows.add(y) {
                        out.lake_change += self.cells(self.lake_delta, r + a)[..b - a].iter().map(|&v| v as f64).sum::<f64>();
                    }
                    let fac = self.cells(self.fac, 0);
                    if a == 0 {
                        out.edge -= (*self.qx.add(y * w1) * fac[r]) as f64;
                    }
                    if b == n {
                        out.edge += (*self.qx.add(y * w1 + n) * fac[r + n - 1]) as f64;
                    }
                    if y == 0 {
                        out.edge -= self.yf(self.qy_new, a)[..b - a].iter().zip(&fac[a..b]).map(|(q, f)| (q * f) as f64).sum::<f64>();
                    }
                    if y + 1 == n {
                        out.edge += self.yf(self.qy_new, n * n + a)[..b - a].iter().zip(&fac[r + a..r + b]).map(|(q, f)| (q * f) as f64).sum::<f64>();
                    }
                }
                if row_new > 0 {
                    out.grew.push(y);
                }
            }
            let last = if y1 == n { n + 1 } else { y1 };
            for y in y0..last {
                let base = y * n;
                for &(a, b) in self.yrow(y) {
                    std::ptr::copy_nonoverlapping(self.qy_new.add(base + a as usize), self.qy.add(base + a as usize), (b - a) as usize);
                }
            }
            out
        }
    }
}
