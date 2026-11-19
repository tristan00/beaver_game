//! The solver's per-substep passes over a band of rows. Bands run on different threads, so the
//! passes reach the water's arrays through raw pointers; each pass writes only its own rows.

use super::{edge_bed, kernels, Pool, Water, WET_SEED};

#[derive(Clone, Debug, Default)]
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
    act: *const f32,
    h_new: *mut f32,
    lake_delta: *mut f32,
    rows: *const Vec<(u32, u32)>,
    yrows: *const Vec<(u32, u32)>,
    lake_rows: *const bool,
    pools: *mut Pool,
    npools: usize,
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
            act: w.act.as_ptr(),
            h_new: w.h_new.as_mut_ptr(),
            lake_delta: w.lake_delta.as_mut_ptr(),
            rows: w.rows.as_ptr(),
            yrows: w.yrows.as_ptr(),
            lake_rows: w.lake_rows.as_ptr(),
            pools: w.pools.as_mut_ptr(),
            npools: w.pools.len(),
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
            let z = self.cells(self.z, 0);
            let (h, fac) = (self.cells(self.h, 0), self.cells(self.fac, 0));
            for y in y0..y1 {
                let base = y * w1;
                let r = y * n;
                // Interior faces 1..n-1 of the row, as lanes 0..n-1 (lane j is face j + 1).
                let out = self.cells_mut(self.qx_new, base + 1, n - 1);
                for &(a, b) in self.row(y) {
                    let (a, b) = (a as usize, b as usize);
                    let (lo, hi) = (a.max(1), b.min(n - 1));
                    if lo <= hi {
                        kernels::faces(
                            out,
                            self.xf(self.qx, base + 1),
                            self.xf(self.qx, base),
                            self.xf(self.qx, base + 2),
                            self.cells(self.z, r),
                            self.cells(self.z, r + 1),
                            self.cells(self.h, r),
                            self.cells(self.h, r + 1),
                            self.cells(self.fac, r),
                            self.cells(self.fac, r + 1),
                            self.xf(self.ox, base + 1),
                            self.cells(self.act, r),
                            self.cells(self.act, r + 1),
                            lo - 1,
                            hi,
                            n - 1,
                            p,
                        );
                    }
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
                }
                for &(a, b) in self.row(y) {
                    let (a, b) = (a as usize, b as usize);
                    std::ptr::copy_nonoverlapping(self.qx_new.add(base + a), self.qx.add(base + a), b - a + 1);
                }
            }
            let last = if y1 == n { n + 1 } else { y1 };
            for y in y0..last {
                let base = y * n;
                if y > 0 && y < n {
                    let out = self.cells_mut(self.qy_new, base, n);
                    for &(a, b) in self.yrow(y) {
                        kernels::faces(
                            out,
                            self.yf(self.qy, base),
                            self.yf(self.qy, base - n),
                            self.yf(self.qy, base + n),
                            self.cells(self.z, base - n),
                            self.cells(self.z, base),
                            self.cells(self.h, base - n),
                            self.cells(self.h, base),
                            self.cells(self.fac, base - n),
                            self.cells(self.fac, base),
                            self.yf(self.oy, base),
                            self.cells(self.act, base - n),
                            self.cells(self.act, base),
                            a as usize,
                            b as usize,
                            n,
                            p,
                        );
                    }
                } else {
                    for &(a, b) in self.yrow(y) {
                        for x in a as usize..b as usize {
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
                let out = self.cells_mut(self.fac, r, n);
                for &(a, b) in self.row(y) {
                    kernels::limiter(
                        out,
                        self.cells(self.h, r),
                        self.cells(self.src, r),
                        self.xf(self.qx, y * w1),
                        self.xf(self.qx, y * w1 + 1),
                        self.yf(self.qy_new, r),
                        self.yf(self.qy_new, r + n),
                        self.cells(self.member, r),
                        k,
                        dt,
                        a as usize,
                        b as usize,
                        n,
                    );
                }
            }
        }
    }

    /// One boundary cell's mass balance (its outer faces have no neighbour cell).
    unsafe fn continuity_cell(&self, i: usize, k: f32, dt: f32) -> (u32, u32) {
        unsafe {
            let n = self.n;
            let w1 = n + 1;
            let (x, y) = (i % n, i / n);
            let fc = self.cells(self.fac, i);
            let fw = if x == 0 { fc } else { self.cells(self.fac, i - 1) };
            let fe = if x + 1 == n { fc } else { self.cells(self.fac, i + 1) };
            let fnn = if y == 0 { fc } else { self.cells(self.fac, i - n) };
            let fs = if y + 1 == n { fc } else { self.cells(self.fac, i + n) };
            let r = kernels::continuity(
                self.cells_mut(self.h_new, i, 1),
                self.cells(self.h, i),
                self.cells(self.z, i),
                self.cells(self.src, i),
                self.cells(self.lake, i),
                self.cells(self.member, i),
                self.xf(self.qx, y * w1 + x),
                self.xf(self.qx, y * w1 + x + 1),
                self.yf(self.qy_new, i),
                self.yf(self.qy_new, i + n),
                fc,
                fw,
                fe,
                fnn,
                fs,
                self.cells_mut(self.lake_delta, i, 1),
                k,
                dt,
                WET_SEED,
                0,
                1,
                1,
            );
            *self.h.add(i) = *self.h_new.add(i);
            r
        }
    }

    /// Mass for the cells of rows y0..y1, then the new y faces of the band become current.
    pub unsafe fn continuity(&self, y0: usize, y1: usize, k: f32, dt: f32, out: &mut BandOut) {
        unsafe {
            let n = self.n;
            let w1 = n + 1;
            for y in y0..y1 {
                let r = y * n;
                let mut row_new = 0u32;
                let interior_row = y > 0 && y + 1 < n;
                for &(a, b) in self.row(y) {
                    let (a, b) = (a as usize, b as usize);
                    if interior_row {
                        let (lo, hi) = (a.max(1), b.min(n - 1));
                        if lo < hi {
                            let (hm, nw) = kernels::continuity(
                                self.cells_mut(self.h_new, r, n),
                                self.cells(self.h, r),
                                self.cells(self.z, r),
                                self.cells(self.src, r),
                                self.cells(self.lake, r),
                                self.cells(self.member, r),
                                self.xf(self.qx, y * w1),
                                self.xf(self.qx, y * w1 + 1),
                                self.yf(self.qy_new, r),
                                self.yf(self.qy_new, r + n),
                                self.cells(self.fac, r),
                                self.cells(self.fac, r - 1),
                                self.cells(self.fac, r + 1),
                                self.cells(self.fac, r - n),
                                self.cells(self.fac, r + n),
                                self.cells_mut(self.lake_delta, r, n),
                                k,
                                dt,
                                WET_SEED,
                                lo,
                                hi,
                                n,
                            );
                            std::ptr::copy_nonoverlapping(self.h_new.add(r + lo), self.h.add(r + lo), hi - lo);
                            out.hmax = out.hmax.max(hm);
                            row_new += nw;
                        }
                        for x in [0, n - 1] {
                            if (a..b).contains(&x) {
                                let (hm, nw) = self.continuity_cell(r + x, k, dt);
                                out.hmax = out.hmax.max(hm);
                                row_new += nw;
                            }
                        }
                    } else {
                        for x in a..b {
                            let (hm, nw) = self.continuity_cell(r + x, k, dt);
                            out.hmax = out.hmax.max(hm);
                            row_new += nw;
                        }
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
        }
    }

    /// Pools t, t + tasks, ...: what crossed the rim changes the volume, the level follows, and
    /// the rim cells show it. Run after `continuity` (faces and limiter are final).
    pub unsafe fn step_pools(&self, t: usize, tasks: usize, k: f32, dt: f32) {
        unsafe {
            let mut pi = t;
            while pi < self.npools {
                let pool = &mut *self.pools.add(pi);
                let mut gain = pool.source as f64 * dt as f64;
                for rf in &pool.faces {
                    let q = if rf.is_x { *self.qx.add(rf.face as usize) } else { *self.qy_new.add(rf.face as usize) };
                    let up = if q > 0.0 { rf.a } else { rf.b };
                    let eff = q * if up == u32::MAX { 1.0 } else { *self.fac.add(up as usize) };
                    // Map-edge rim faces are booked as edge losses by the continuity pass.
                    gain += (rf.sign * eff) as f64 * k as f64;
                }
                pool.volume = (pool.volume + gain).max(0.0);
                pool.level = pool.level_for(pool.volume);
                for &i in &pool.rim {
                    *self.h.add(i as usize) = (pool.level - *self.z.add(i as usize)).max(0.0);
                }
                pi += tasks;
            }
        }
    }
}
