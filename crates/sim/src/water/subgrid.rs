use super::{Water, edge_bed};

const C: usize = 4;
const DT: f64 = 900.0;
const INERTIA: f64 = 0.0;
const BANK_S: f64 = 900.0;
const GRAV: f64 = 9.81;
const NEWTON_MAX: usize = 30;
const LEVEL_TOL: f64 = 1.0e-5;
const STEP_MAX: f64 = 1.0;
const NEWTON_TOL: f64 = 1.0e-4;
const LIN_MAX: usize = 200;
const LIN_REL: f64 = 1.0e-2;
const AGG_MIN: usize = 48;
const AGG: usize = 8;
const AGG_MAX: usize = 160;
const REDO_ROUNDS: usize = 6;
const POOL_MIN: usize = 16;
const POOL_SPREAD: f64 = 0.02;
const SPILL_MIN: f64 = 0.005;
const TOWER: f64 = 0.25;
const H_CAP: f64 = 5.0;
const LIMIT_ROUNDS: usize = 200;
const SPILL_MOVES: usize = 20000;
const DAM_HALO: usize = 1;
const RINGS: u8 = 1;
const MARGIN: f64 = 0.05;
const FROUDE_CAP: f64 = 1.0;
const H_MIN: f64 = 1.0e-6;
const TRACE: f64 = 1.0e-9;
const Q_STILL: f64 = 1.0e-6;
const PUB_EPS: f64 = 1.0e-4;
const LS_REG: f64 = 0.5;
const JAC_FLOOR: f64 = 1.0;

const NONE: u32 = u32::MAX;
const DEAD: u8 = 0;
const FLAT: u8 = 1;
const FREE_L: u8 = 2;
const FREE_R: u8 = 3;
const Y_FACE: u32 = 1 << 31;
const NEG: u32 = 1 << 30;
const IDX: u32 = NEG - 1;

type Version = ((u64, u64), (u64, u64), (u64, u64), (u64, u64), bool);

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub(super) struct Subgrid {
    version: Option<Version>,
    n: usize,
    owner: Vec<u32>,
    cell_start: Vec<u32>,
    cells: Vec<u32>,
    beds: Vec<f64>,
    prefix: Vec<f64>,
    centroid: Vec<(f64, f64)>,
    fixed: Vec<f64>,
    vol: Vec<f64>,
    eta: Vec<f64>,
    source: Vec<f64>,
    source_key: Option<u64>,
    source_regions: Vec<u32>,
    face_lr: Vec<(u32, u32)>,
    face_dx: Vec<f64>,
    face_sill: Vec<f64>,
    strip_start: Vec<u32>,
    strip_sill: Vec<f64>,
    strip_code: Vec<u32>,
    strip_u: Vec<f64>,
    strip_h: Vec<f64>,
    strip_unew: Vec<f64>,
    strip_gamma: Vec<f64>,
    strip_q: Vec<f64>,
    rf_start: Vec<u32>,
    rf: Vec<u32>,
    out_start: Vec<u32>,
    out_z: Vec<f64>,
    out_slope: Vec<f64>,
    out_code: Vec<u32>,
    out_q: Vec<f64>,
    edge_regions: Vec<u32>,
    wet: Vec<u32>,
    published: Vec<u32>,
    mark: Vec<u64>,
    fmark: Vec<u64>,
    stamp: u64,
    act: Vec<u32>,
    unk: Vec<u32>,
    loc: Vec<u32>,
    af: Vec<u32>,
    af_prev: Vec<u32>,
    af_loc: Vec<(u32, u32)>,
    af_g: Vec<f64>,
    af_q: Vec<f64>,
    af_a: Vec<f64>,
    af_b: Vec<f64>,
    af_dn: Vec<f64>,
    af_kind: Vec<u8>,
    af_done: Vec<u8>,
    cl_pool: Vec<bool>,
    uf: Vec<u32>,
    cl_of: Vec<u32>,
    cl_start: Vec<u32>,
    cl_mem: Vec<u32>,
    clf_start: Vec<u32>,
    clf: Vec<u32>,
    inflow_known: Vec<f64>,
    dt: f64,
    agg_of: Vec<u32>,
    agg_n: usize,
    coarse: Vec<f64>,
    coarse_y: Vec<f64>,
    ring: Vec<u8>,
    fric: f64,
    x: Vec<f64>,
    f: Vec<f64>,
    d: Vec<f64>,
    area: Vec<f64>,
    vn: Vec<f64>,
    qout: Vec<f64>,
    dx: Vec<f64>,
    work: [Vec<f64>; 4],
    theta: Vec<f64>,
    inflow: Vec<f64>,
    outflow: Vec<f64>,
    regq: Vec<(f64, f64)>,
    pub_stamp: u64,
    pub_mark: Vec<u64>,
    pub_eta: Vec<f64>,
    pub_faces: Vec<u32>,
    rows_dirty: bool,
    zcopy: Vec<f32>,
    face_k: Vec<u32>,
    stale: bool,
    pending: f64,
    tx: Vec<f32>,
    ty: Vec<f32>,
}

fn find(uf: &mut [u32], mut a: u32) -> u32 {
    while uf[a as usize] != a {
        let p = uf[uf[a as usize] as usize];
        uf[a as usize] = p;
        a = p;
    }
    a
}

fn decode(code: u32, n: usize) -> (usize, usize) {
    let idx = (code & IDX) as usize;
    if code & Y_FACE != 0 {
        let (yr, x) = (idx / n, idx % n);
        ((yr - 1) * n + x, yr * n + x)
    } else {
        let (y, xr) = (idx / (n + 1), idx % (n + 1));
        (y * n + xr - 1, y * n + xr)
    }
}

impl Subgrid {
    fn nreg(&self) -> usize {
        self.cell_start.len().saturating_sub(1)
    }

    fn span(&self, r: usize) -> (usize, usize) {
        (self.cell_start[r] as usize, self.cell_start[r + 1] as usize)
    }

    fn storage(&self, r: usize, eta: f64) -> (f64, f64) {
        let (s, e) = self.span(r);
        let beds = &self.beds[s..e];
        let k = beds.partition_point(|&z| z < eta);
        if k == 0 {
            return (0.0, 0.0);
        }
        (((eta * k as f64) - self.prefix[s + r + k]).max(0.0), k as f64)
    }

    fn level_for(&self, r: usize, volume: f64) -> f64 {
        let (s, e) = self.span(r);
        let beds = &self.beds[s..e];
        let pre = &self.prefix[s + r..e + r + 1];
        if volume <= 0.0 {
            return beds[0];
        }
        let (mut lo, mut hi) = (1, beds.len());
        while lo < hi {
            let k = (lo + hi) / 2;
            if (volume + pre[k]) / k as f64 > beds[k] {
                lo = k + 1;
            } else {
                hi = k;
            }
        }
        (volume + pre[lo]) / lo as f64
    }

    fn is_wet(&self, r: usize) -> bool {
        self.vol[r] > 0.0 || self.fixed[r].is_finite()
    }

    pub(super) fn total_volume(&self) -> Option<f64> {
        self.version.map(|_| self.vol.iter().sum())
    }

    fn rebuild(&mut self, w: &Water) {
        let n = w.n;
        self.n = n;
        let lake = w.lake_level.as_slice();
        let dam = w.dam_height.as_slice();
        let class = |i: usize| if lake[i].is_finite() { 2u8 } else if dam[i] > 0.0 { 1 } else { 0 };
        self.zcopy.clone_from(&w.z);
        self.owner = vec![NONE; n * n];
        self.cells.clear();
        self.cell_start = vec![0];
        let nb = (n + C - 1) / C;
        let mut dam_block = vec![false; nb * nb];
        for i in 0..n * n {
            if dam[i] > 0.0 {
                dam_block[(i / n / C) * nb + (i % n) / C] = true;
            }
        }
        let mut fine = vec![false; nb * nb];
        for b in 0..nb * nb {
            if dam_block[b] {
                let (bx, by) = ((b % nb) as i64, (b / nb) as i64);
                let h = DAM_HALO as i64;
                for y in (by - h).max(0)..=(by + h).min(nb as i64 - 1) {
                    for x in (bx - h).max(0)..=(bx + h).min(nb as i64 - 1) {
                        fine[y as usize * nb + x as usize] = true;
                    }
                }
            }
        }
        for by in (0..n).step_by(C) {
            for bx in (0..n).step_by(C) {
                let (x1, y1) = ((bx + C).min(n), (by + C).min(n));
                let single = fine[(by / C) * nb + bx / C];
                for y in by..y1 {
                    for x in bx..x1 {
                        let s = y * n + x;
                        if self.owner[s] != NONE {
                            continue;
                        }
                        let id = (self.cell_start.len() - 1) as u32;
                        let k = class(s);
                        self.owner[s] = id;
                        let mut q = self.cells.len();
                        self.cells.push(s as u32);
                        while q < self.cells.len() {
                            let i = self.cells[q] as usize;
                            q += 1;
                            let (cx, cy) = (i % n, i / n);
                            for (ok, j) in [(cx > bx, i.wrapping_sub(1)), (cx + 1 < x1, i + 1), (cy > by, i.wrapping_sub(n)), (cy + 1 < y1, i + n)] {
                                if ok && !single && self.owner[j] == NONE && class(j) == k {
                                    self.owner[j] = id;
                                    self.cells.push(j as u32);
                                }
                            }
                        }
                        self.cell_start.push(self.cells.len() as u32);
                    }
                }
            }
        }
        let nreg = self.nreg();
        self.beds = vec![0.0; self.cells.len()];
        self.prefix = vec![0.0; self.cells.len() + nreg];
        self.centroid = vec![(0.0, 0.0); nreg];
        self.fixed = vec![f64::NAN; nreg];
        for r in 0..nreg {
            let (s, e) = self.span(r);
            let (mut sx, mut sy) = (0.0, 0.0);
            let mut level = f64::NAN;
            for k in s..e {
                let i = self.cells[k] as usize;
                self.beds[k] = w.z[i] as f64;
                sx += (i % n) as f64;
                sy += (i / n) as f64;
                if lake[i].is_finite() {
                    level = if level.is_nan() { lake[i] as f64 } else { level.max(lake[i] as f64) };
                }
            }
            self.beds[s..e].sort_unstable_by(f64::total_cmp);
            let mut acc = 0.0;
            self.prefix[s + r] = 0.0;
            for k in s..e {
                acc += self.beds[k];
                self.prefix[k + r + 1] = acc;
            }
            let m = (e - s) as f64;
            self.centroid[r] = (sx / m, sy / m);
            self.fixed[r] = level;
        }
        let mut raw: Vec<(u32, u32, u32)> = Vec::new();
        for y in 0..n {
            for x in 0..n {
                let i = y * n + x;
                let a = self.owner[i];
                if x + 1 < n {
                    let b = self.owner[i + 1];
                    if a != b {
                        let code = (y * (n + 1) + x + 1) as u32;
                        raw.push(if a < b { (a, b, code) } else { (b, a, code | NEG) });
                    }
                }
                if y + 1 < n {
                    let b = self.owner[i + n];
                    if a != b {
                        let code = Y_FACE | ((y + 1) * n + x) as u32;
                        raw.push(if a < b { (a, b, code) } else { (b, a, code | NEG) });
                    }
                }
            }
        }
        raw.sort_unstable();
        self.face_lr.clear();
        self.face_dx.clear();
        self.face_sill.clear();
        self.strip_start = vec![0];
        self.strip_sill.clear();
        self.strip_code.clear();
        let cell_m = w.cell_m as f64;
        for (k, &(a, b, code)) in raw.iter().enumerate() {
            if k == 0 || (raw[k - 1].0, raw[k - 1].1) != (a, b) {
                if k > 0 {
                    self.strip_start.push(self.strip_code.len() as u32);
                }
                self.face_lr.push((a, b));
                let (ca, cb) = (self.centroid[a as usize], self.centroid[b as usize]);
                self.face_dx.push(((ca.0 - cb.0).hypot(ca.1 - cb.1)).max(1.0) * cell_m);
                self.face_sill.push(f64::INFINITY);
            }
            let (i, j) = decode(code, n);
            let sill = w.z[i].max(w.z[j]) as f64;
            let fi = self.face_lr.len() - 1;
            self.face_sill[fi] = self.face_sill[fi].min(sill);
            self.strip_sill.push(sill);
            self.strip_code.push(code);
        }
        self.strip_start.push(self.strip_code.len() as u32);
        let nf = self.face_lr.len();
        let ns = self.strip_code.len();
        let mut count = vec![0u32; nreg + 1];
        for &(a, b) in &self.face_lr {
            count[a as usize + 1] += 1;
            count[b as usize + 1] += 1;
        }
        for r in 0..nreg {
            count[r + 1] += count[r];
        }
        self.rf = vec![0; count[nreg] as usize];
        let mut fill = count.clone();
        for (f, &(a, b)) in self.face_lr.iter().enumerate() {
            for r in [a, b] {
                self.rf[fill[r as usize] as usize] = f as u32;
                fill[r as usize] += 1;
            }
        }
        self.rf_start = count;
        let mut outs: Vec<(u32, u32, f64, f64)> = Vec::new();
        if w.config.open_edges {
            let closed = w.closed_edge.as_slice();
            for y in 0..n {
                for x in 0..n {
                    let i = y * n + x;
                    if !(x == 0 || y == 0 || x + 1 == n || y + 1 == n) || closed[i] || lake[i].is_finite() {
                        continue;
                    }
                    for (yes, inner, code) in [
                        (x == 0, i + 1, (y * (n + 1)) as u32 | NEG),
                        (x + 1 == n, i.wrapping_sub(1), (y * (n + 1) + n) as u32),
                        (y == 0, i + n, Y_FACE | NEG | x as u32),
                        (y + 1 == n, i.wrapping_sub(n), Y_FACE | (n * n + x) as u32),
                    ] {
                        if yes {
                            let slope = ((w.z[i] - edge_bed(&w.z, i, inner, w.cell_m)) / w.cell_m) as f64;
                            outs.push((self.owner[i], code, w.z[i] as f64, slope));
                        }
                    }
                }
            }
        }
        outs.sort_by_key(|o| (o.0, o.1));
        self.out_start = vec![0; nreg + 1];
        for o in &outs {
            self.out_start[o.0 as usize + 1] += 1;
        }
        for r in 0..nreg {
            self.out_start[r + 1] += self.out_start[r];
        }
        self.out_code = outs.iter().map(|o| o.1).collect();
        self.out_z = outs.iter().map(|o| o.2).collect();
        self.out_slope = outs.iter().map(|o| o.3).collect();
        self.out_q = vec![0.0; outs.len()];
        self.edge_regions.clear();
        self.strip_u = vec![0.0; ns];
        self.strip_h = vec![0.0; ns];
        self.strip_unew = vec![0.0; ns];
        self.strip_gamma = vec![1.0; ns];
        self.strip_q = vec![0.0; ns];
        self.vol = vec![0.0; nreg];
        self.eta = vec![0.0; nreg];
        self.source = vec![0.0; nreg];
        self.mark = vec![0; nreg];
        self.ring = vec![0; nreg];
        self.loc = vec![NONE; nreg];
        self.fmark = vec![0; nf];
        self.face_k = vec![0; nf];
        self.stamp = 0;
        self.regq = vec![(0.0, 0.0); nreg];
        self.tx = vec![0.0; n * n];
        self.ty = vec![0.0; n * n];
        self.af.clear();
        self.af_prev.clear();
        self.source_key = None;
        self.source_regions.clear();
    }

    pub(super) fn prepare(&mut self, w: &mut Water, mut sync: bool) {
        let (terrain, dam) = w.bed_version.unwrap();
        let version = (terrain, dam, w.closed_edge.version(), w.lake_level.version(), w.config.open_edges);
        let rebuilt = self.version != Some(version);
        let carry: Option<Vec<f64>> = (rebuilt && self.version.is_some()).then(|| {
            let mut d = vec![0.0; self.n * self.n];
            for r in 0..self.nreg() {
                let level = if self.fixed[r].is_finite() { self.fixed[r] } else if self.vol[r] > 0.0 { self.eta[r] } else { continue };
                let (a, b) = self.span(r);
                for c in a..b {
                    let i = self.cells[c] as usize;
                    d[i] = (level - self.zcopy[i] as f64).max(0.0);
                }
            }
            d
        });
        let correction = if rebuilt {
            let held = match &carry { Some(d) => d.iter().sum::<f64>(), None => w.h.iter().map(|&h| h as f64).sum::<f64>() };
            self.total_volume().map(|v| v - held).unwrap_or(0.0)
        } else {
            0.0
        };
        if rebuilt {
            self.rebuild(w);
            self.version = Some(version);
            sync = true;
        }
        if sync {
            let nreg = self.nreg();
            let mut receiver = (0usize, f64::NEG_INFINITY);
            for r in 0..nreg {
                let (s, e) = self.span(r);
                let mut v = 0.0;
                for k in s..e {
                    let i = self.cells[k] as usize;
                    let h = match &carry { Some(d) => d[i], None => w.h[i] as f64 };
                    v += h;
                    if h > receiver.1 {
                        receiver = (r, h);
                    }
                }
                self.vol[r] = v;
            }
            if correction != 0.0 {
                let r = receiver.0;
                self.vol[r] = (self.vol[r] + correction).max(0.0);
            }
            for r in 0..nreg {
                if self.fixed[r].is_finite() {
                    let target = self.storage(r, self.fixed[r]).0;
                    let change = target - self.vol[r];
                    if change >= 0.0 {
                        w.books.sources += change;
                        w.added_volume += change;
                    } else {
                        w.books.edges -= change;
                        w.lost_volume -= change;
                    }
                    self.vol[r] = target;
                    self.eta[r] = self.fixed[r];
                } else {
                    self.eta[r] = self.level_for(r, self.vol[r]);
                }
            }
            self.wet = (0..nreg as u32).filter(|&r| self.is_wet(r as usize)).collect();
            if rebuilt {
                self.published.clone_from(&self.wet);
                self.pub_mark = vec![0; nreg];
                self.pub_eta = vec![f64::NAN; nreg];
            }
            self.pub_eta.iter_mut().for_each(|e| *e = f64::NAN);
            self.rows_dirty = true;
            self.stale = true;
            self.strip_u.iter_mut().for_each(|u| *u = 0.0);
            self.strip_q.iter_mut().for_each(|u| *u = 0.0);
            self.af.clear();
            self.af_prev.clear();
            self.edge_regions.clear();
        }
        if self.source_key != w.source_key {
            for &r in &self.source_regions {
                self.source[r as usize] = 0.0;
            }
            self.source_regions.clear();
            for &i in &w.source_cells {
                let r = self.owner[i];
                self.source[r as usize] += w.src[i] as f64;
                self.source_regions.push(r);
            }
            self.source_regions.sort_unstable();
            self.source_regions.dedup();
            self.source_key = w.source_key;
        }
    }

    pub(super) fn advance(&mut self, w: &mut Water, seconds: f64) {
        if seconds <= 0.0 {
            return;
        }
        self.pending += seconds;
        if self.pending + 1.0e-6 < BANK_S {
            return;
        }
        let seconds = std::mem::take(&mut self.pending);
        self.stale = true;
        let steps = (seconds / DT - 1.0e-9).ceil().max(1.0) as usize;
        let dt = seconds / steps as f64;
        for _ in 0..steps {
            self.step(w, dt);
            w.physics_seconds += dt;
            w.steps_taken += 1;
            w.timings.physics_s += dt;
            w.timings.substeps += 1;
        }
    }

    fn step(&mut self, w: &mut Water, dt: f64) {
        self.solve_step(w, dt);
        let nman = w.config.manning as f64;
        let wfac = 1.0 / w.cell_m as f64;
        let nfa = self.af.len();
        let m = self.unk.len();
        self.finish_step(w, dt, nman, wfac, nfa, m);
        self.spill(w);
    }

    fn spill(&mut self, w: &mut Water) {
        let stamp = self.stamp;
        let key = |z: f64| {
            let b = z.to_bits();
            if b >> 63 == 0 { b ^ (1 << 63) } else { !b }
        };
        let mut heap = std::collections::BinaryHeap::new();
        for k in 0..self.act.len() {
            let r = self.act[k] as usize;
            if self.fixed[r].is_nan() && self.vol[r] > 0.0 {
                heap.push((key(self.eta[r]), r as u32));
            }
        }
        let mut moves = 0;
        while let Some((kk, r)) = heap.pop() {
            let r = r as usize;
            if kk != key(self.eta[r]) || moves >= SPILL_MOVES {
                continue;
            }
            let lvl = self.eta[r];
            let mut best: Option<(f64, usize)> = None;
            for fi in self.rf_start[r]..self.rf_start[r + 1] {
                let f = self.rf[fi as usize] as usize;
                let sill = self.face_sill[f];
                if sill + SPILL_MIN >= lvl {
                    continue;
                }
                let (a, b) = self.face_lr[f];
                let o = if a as usize == r { b } else { a } as usize;
                if self.fmark[f] == stamp && lvl - sill < TOWER {
                    let kind = self.af_kind[self.face_k[f] as usize];
                    let into_r = if a as usize == r { kind == FREE_R } else { kind == FREE_L };
                    if !into_r {
                        continue;
                    }
                }
                let lo = if self.fixed[o].is_finite() { self.fixed[o] } else { self.eta[o] };
                if lo + SPILL_MIN >= lvl {
                    continue;
                }
                if best.map_or(true, |(s, _)| sill < s) {
                    best = Some((sill, o));
                }
            }
            let Some((sill, o)) = best else { continue };
            let lo = if self.fixed[o].is_finite() { self.fixed[o] } else { self.eta[o] };
            let floor = sill.max(lo);
            let dv = if self.fixed[o].is_finite() {
                self.vol[r] - self.storage(r, floor).0
            } else {
                let (vr, vo) = (self.vol[r], self.vol[o]);
                let gives = |l: f64| vr - self.storage(r, l.max(sill)).0;
                let takes = |l: f64| self.storage(o, l).0 - vo;
                if gives(floor) <= takes(floor) {
                    gives(floor)
                } else {
                    let (mut a, mut b) = (floor, lvl);
                    for _ in 0..40 {
                        let mid = 0.5 * (a + b);
                        if gives(mid) > takes(mid) { a = mid } else { b = mid }
                    }
                    gives(b).max(0.0)
                }
            };
            if dv <= TRACE {
                continue;
            }
            moves += 1;
            self.vol[r] -= dv;
            self.eta[r] = self.level_for(r, self.vol[r]);
            heap.push((key(self.eta[r]), r as u32));
            if self.fixed[o].is_finite() {
                w.books.edges += dv;
                w.lost_volume += dv;
            } else {
                if self.vol[o] <= 0.0 {
                    self.wet.push(o as u32);
                }
                self.vol[o] += dv;
                self.eta[o] = self.level_for(o, self.vol[o]);
                heap.push((key(self.eta[o]), o as u32));
            }
        }
    }

    fn solve_step(&mut self, w: &mut Water, dt: f64) {
        self.stamp += 1;
        let stamp = self.stamp;
        self.act.clear();
        self.af.clear();
        let (nw, ns) = (self.wet.len(), self.source_regions.len());
        for k in 0..nw + ns {
            let r = if k < nw { self.wet[k] } else { self.source_regions[k - nw] };
            if self.mark[r as usize] != stamp {
                self.mark[r as usize] = stamp;
                self.ring[r as usize] = 0;
                self.act.push(r);
            }
        }
        let mut k = 0;
        while k < self.act.len() {
            let r = self.act[k] as usize;
            k += 1;
            let ring = self.ring[r];
            if ring >= RINGS {
                continue;
            }
            for fi in self.rf_start[r]..self.rf_start[r + 1] {
                let f = self.rf[fi as usize] as usize;
                if self.fmark[f] == stamp {
                    continue;
                }
                let (a, b) = self.face_lr[f];
                if self.eta[a as usize].max(self.eta[b as usize]) + MARGIN <= self.face_sill[f] {
                    continue;
                }
                self.fmark[f] = stamp;
                self.face_k[f] = self.af.len() as u32;
                self.af.push(f as u32);
                let o = if a as usize == r { b } else { a } as usize;
                if self.mark[o] != stamp {
                    self.mark[o] = stamp;
                    self.ring[o] = ring + 1;
                    self.act.push(o as u32);
                }
            }
        }
        for k in 0..self.af_prev.len() {
            let f = self.af_prev[k] as usize;
            if self.fmark[f] != stamp {
                for s in self.strip_start[f]..self.strip_start[f + 1] {
                    self.strip_u[s as usize] = 0.0;
                    self.strip_q[s as usize] = 0.0;
                }
            }
        }
        self.unk.clear();
        for k in 0..self.act.len() {
            let r = self.act[k];
            if self.fixed[r as usize].is_nan() {
                self.loc[r as usize] = self.unk.len() as u32;
                self.unk.push(r);
            } else {
                self.loc[r as usize] = NONE;
            }
        }
        let m = self.unk.len();
        for v in [&mut self.x, &mut self.f, &mut self.d, &mut self.area, &mut self.vn, &mut self.qout, &mut self.dx, &mut self.inflow_known, &mut self.theta, &mut self.inflow, &mut self.outflow] {
            v.clear();
            v.resize(m, 0.0);
        }
        for li in 0..m {
            let r = self.unk[li] as usize;
            self.x[li] = self.eta[r];
            self.vn[li] = self.vol[r];
        }
        let nman = w.config.manning as f64;
        self.fric = GRAV * dt * nman * nman;
        self.dt = dt;
        let wfac = 1.0 / w.cell_m as f64;
        let nfa = self.af.len();
        self.af_loc.clear();
        self.af_g.clear();
        for v in [&mut self.af_q, &mut self.af_a, &mut self.af_b, &mut self.af_dn] {
            v.clear();
            v.resize(nfa, 0.0);
        }
        self.af_kind.clear();
        self.af_kind.resize(nfa, DEAD);
        self.af_done.clear();
        self.af_done.resize(nfa, 0);
        for k in 0..nfa {
            let f = self.af[k] as usize;
            let (a, b) = self.face_lr[f];
            self.af_loc.push((self.loc[a as usize], self.loc[b as usize]));
            let g = GRAV * dt / self.face_dx[f];
            self.af_g.push(g);
            let (el, er) = (self.eta[a as usize], self.eta[b as usize]);
            let sill = self.face_sill[f];
            if el.min(er) > sill {
                self.af_kind[k] = FLAT;
            } else if el != er {
                self.af_kind[k] = if el > er { FREE_L } else { FREE_R };
                self.af_dn[k] = el.min(er);
            }
        }
        self.uf.clear();
        self.uf.extend(0..m as u32);
        for k in 0..nfa {
            if self.af_kind[k] != FLAT {
                continue;
            }
            let (ll, lr) = self.af_loc[k];
            if ll != NONE && lr != NONE {
                let (ra, rb) = (find(&mut self.uf, ll), find(&mut self.uf, lr));
                if ra != rb {
                    let (lo, hi) = if ra < rb { (ra, rb) } else { (rb, ra) };
                    self.uf[hi as usize] = lo;
                }
            }
        }
        self.cl_of.clear();
        self.cl_of.resize(m, NONE);
        let mut ncl = 0u32;
        for li in 0..m {
            let root = find(&mut self.uf, li as u32) as usize;
            if self.cl_of[root] == NONE {
                self.cl_of[root] = ncl;
                ncl += 1;
            }
            self.cl_of[li] = self.cl_of[root];
        }
        let ncl = ncl as usize;
        for k in 0..nfa {
            let kind = self.af_kind[k];
            if kind == FREE_L || kind == FREE_R {
                let (ll, lr) = self.af_loc[k];
                if ll != NONE && lr != NONE && self.cl_of[ll as usize] == self.cl_of[lr as usize] {
                    self.af_kind[k] = FLAT;
                }
            }
        }
        let mut count = vec![0u32; ncl + 1];
        for li in 0..m {
            count[self.cl_of[li] as usize + 1] += 1;
        }
        for c in 0..ncl {
            count[c + 1] += count[c];
        }
        self.cl_start.clone_from(&count);
        self.cl_mem.clear();
        self.cl_mem.resize(m, 0);
        for li in 0..m {
            let c = self.cl_of[li] as usize;
            self.cl_mem[count[c] as usize] = li as u32;
            count[c] += 1;
        }
        let owner_cl = |kind: u8, (ll, lr): (u32, u32), cl_of: &[u32]| -> u32 {
            match kind {
                FLAT => if ll != NONE { cl_of[ll as usize] } else if lr != NONE { cl_of[lr as usize] } else { NONE },
                FREE_L => if ll != NONE { cl_of[ll as usize] } else { NONE },
                FREE_R => if lr != NONE { cl_of[lr as usize] } else { NONE },
                _ => NONE,
            }
        };
        let mut fcount = vec![0u32; ncl + 1];
        for k in 0..nfa {
            let c = owner_cl(self.af_kind[k], self.af_loc[k], &self.cl_of);
            if c != NONE {
                fcount[c as usize + 1] += 1;
            }
        }
        for c in 0..ncl {
            fcount[c + 1] += fcount[c];
        }
        self.clf_start.clone_from(&fcount);
        self.clf.clear();
        self.clf.resize(fcount[ncl] as usize, 0);
        for k in 0..nfa {
            let c = owner_cl(self.af_kind[k], self.af_loc[k], &self.cl_of);
            if c != NONE {
                self.clf[fcount[c as usize] as usize] = k as u32;
                fcount[c as usize] += 1;
            }
        }
        let mut lo_x = vec![f64::INFINITY; ncl];
        let mut hi_x = vec![f64::NEG_INFINITY; ncl];
        for li in 0..m {
            let c = self.cl_of[li] as usize;
            lo_x[c] = lo_x[c].min(self.x[li]);
            hi_x[c] = hi_x[c].max(self.x[li]);
        }
        self.cl_pool.clear();
        for c in 0..ncl {
            self.cl_pool.push(self.cl_start[c + 1] - self.cl_start[c] >= POOL_MIN as u32 && hi_x[c] - lo_x[c] <= POOL_SPREAD);
        }
        for k in 0..nfa {
            if self.af_kind[k] != FLAT {
                continue;
            }
            let f = self.af[k] as usize;
            let (ll, lr) = self.af_loc[k];
            if ll != NONE && lr != NONE && self.cl_pool[self.cl_of[ll as usize] as usize] {
                self.af_a[k] = 0.0;
                self.af_b[k] = 0.0;
                for s in self.strip_start[f] as usize..self.strip_start[f + 1] as usize {
                    self.strip_h[s] = 0.0;
                    self.strip_u[s] = 0.0;
                }
                continue;
            }
            let (a, b) = self.face_lr[f];
            let (el, er) = (self.eta[a as usize], self.eta[b as usize]);
            let g = self.af_g[k];
            let (mut fa, mut fb) = (0.0, 0.0);
            for s in self.strip_start[f] as usize..self.strip_start[f + 1] as usize {
                let bb = self.strip_u[s] - g * (er - el);
                let up = if bb > 0.0 { el } else if bb < 0.0 { er } else { el.max(er) };
                let h = up - self.strip_sill[s];
                if h <= H_MIN {
                    self.strip_h[s] = 0.0;
                    self.strip_u[s] = 0.0;
                    continue;
                }
                let kf = self.fric / (h * h.cbrt());
                let ustar = 2.0 * bb.abs() / (1.0 + (1.0 + 4.0 * kf * bb.abs()).sqrt());
                let mut gamma = 1.0 + kf * ustar;
                let cap = FROUDE_CAP * (GRAV * h).sqrt();
                if ustar > cap {
                    gamma = gamma.max(bb.abs() / cap);
                }
                self.strip_h[s] = h;
                self.strip_gamma[s] = gamma;
                fa += h * self.strip_u[s] / gamma;
                fb += h * g / gamma;
            }
            self.af_a[k] = fa * wfac;
            self.af_b[k] = fb * wfac;
        }
        for k in 0..nfa {
            let kind = self.af_kind[k];
            let (ll, lr) = self.af_loc[k];
            let (up, dn) = match kind {
                FREE_L => (ll, lr),
                FREE_R => (lr, ll),
                _ => continue,
            };
            if up == NONE && dn != NONE {
                let f = self.af[k] as usize;
                let (a, b) = self.face_lr[f];
                let xu = if kind == FREE_L { self.eta[a as usize] } else { self.eta[b as usize] };
                let (q, _) = self.free_flux(k, xu, wfac, false);
                self.inflow_known[dn as usize] += dt * q;
                self.af_done[k] = 1;
            }
        }
        let mut indeg = vec![0u32; ncl];
        let mut top = vec![f64::NEG_INFINITY; ncl];
        for li in 0..m {
            let c = self.cl_of[li] as usize;
            top[c] = top[c].max(self.x[li]);
        }
        for k in 0..nfa {
            let (up, dn) = match self.af_kind[k] {
                FREE_L => (self.af_loc[k].0, self.af_loc[k].1),
                FREE_R => (self.af_loc[k].1, self.af_loc[k].0),
                _ => continue,
            };
            if up != NONE && dn != NONE && self.cl_of[up as usize] != self.cl_of[dn as usize] {
                indeg[self.cl_of[dn as usize] as usize] += 1;
            }
        }
        let mut order: Vec<u32> = (0..ncl as u32).filter(|&c| indeg[c as usize] == 0).collect();
        let mut queued = vec![false; ncl];
        for &c in &order {
            queued[c as usize] = true;
        }
        let mut done = vec![false; ncl];
        let mut dirty = vec![false; ncl];
        let mut head = 0;
        let mut redo: Vec<u32> = Vec::new();
        while head < order.len() || order.len() < ncl {
            if head == order.len() {
                let mut best = NONE;
                for c in 0..ncl {
                    if !queued[c] && (best == NONE || top[c] > top[best as usize]) {
                        best = c as u32;
                    }
                }
                queued[best as usize] = true;
                order.push(best);
            }
            let c = order[head] as usize;
            head += 1;
            done[c] = true;
            self.solve_cluster(c, dt, w);
            self.solve_free(c, dt, wfac, &mut indeg, &mut queued, &mut order, &done, &mut dirty, &mut redo);
        }
        for _ in 0..REDO_ROUNDS {
            if redo.is_empty() {
                break;
            }
            let list = std::mem::take(&mut redo);
            for &c in &list {
                dirty[c as usize] = false;
            }
            for &c in &list {
                self.solve_cluster(c as usize, dt, w);
                self.solve_free(c as usize, dt, wfac, &mut indeg, &mut queued, &mut order, &done, &mut dirty, &mut redo);
            }
        }
    }

    fn finish_step(&mut self, w: &mut Water, dt: f64, nman: f64, wfac: f64, nfa: usize, m: usize) {
        for k in 0..nfa {
            let (ll, lr) = self.af_loc[k];
            let f = self.af[k] as usize;
            let (a, b) = self.face_lr[f];
            let el = if ll == NONE { self.eta[a as usize] } else { self.x[ll as usize] };
            let er = if lr == NONE { self.eta[b as usize] } else { self.x[lr as usize] };
            match self.af_kind[k] {
                FLAT => {
                    self.af_q[k] = self.af_a[k] - self.af_b[k] * (er - el);
                    let g = self.af_g[k];
                    for s in self.strip_start[f] as usize..self.strip_start[f + 1] as usize {
                        self.strip_unew[s] = if self.strip_h[s] > 0.0 { (self.strip_u[s] - g * (er - el)) / self.strip_gamma[s] } else { 0.0 };
                    }
                }
                FREE_L => self.af_q[k] = self.free_flux(k, el, wfac, true).0,
                FREE_R => self.af_q[k] = -self.free_flux(k, er, wfac, true).0,
                _ => {
                    for s in self.strip_start[f] as usize..self.strip_start[f + 1] as usize {
                        self.strip_h[s] = 0.0;
                        self.strip_unew[s] = 0.0;
                    }
                }
            }
        }
        for li in 0..m {
            let r = self.unk[li] as usize;
            self.qout[li] = self.edge_flux(r, self.x[li], nman, wfac, true).0;
        }
        self.finish(w, dt);
        w.timings.cell_updates += m as u64;
        std::mem::swap(&mut self.af, &mut self.af_prev);
        self.af.clone_from(&self.af_prev);
    }

    #[allow(clippy::too_many_arguments)]
    fn solve_free(&mut self, c: usize, dt: f64, wfac: f64, indeg: &mut [u32], queued: &mut [bool], order: &mut Vec<u32>, done: &[bool], dirty: &mut [bool], redo: &mut Vec<u32>) {
        for fi in self.clf_start[c]..self.clf_start[c + 1] {
            let k = self.clf[fi as usize] as usize;
            let kind = self.af_kind[k];
            if kind != FREE_L && kind != FREE_R {
                continue;
            }
            let (up, dn) = if kind == FREE_L { (self.af_loc[k].0, self.af_loc[k].1) } else { (self.af_loc[k].1, self.af_loc[k].0) };
            if dn == NONE || self.cl_of[dn as usize] as usize == c {
                continue;
            }
            let (q, _) = self.free_flux(k, self.x[up as usize], wfac, false);
            let first = self.af_done[k] == 0;
            let change = q - self.af_q[k];
            self.inflow_known[dn as usize] += dt * change;
            self.af_q[k] = q;
            self.af_done[k] = 1;
            let cd = self.cl_of[dn as usize] as usize;
            if first {
                indeg[cd] -= 1;
                if indeg[cd] == 0 && !queued[cd] {
                    queued[cd] = true;
                    order.push(cd as u32);
                }
            }
            if done[cd] && !dirty[cd] && dt * change.abs() > NEWTON_TOL {
                dirty[cd] = true;
                redo.push(cd as u32);
            }
        }
    }

    fn free_flux(&mut self, k: usize, xu: f64, wfac: f64, keep: bool) -> (f64, f64) {
        let f = self.af[k] as usize;
        let sign = if self.af_kind[k] == FREE_L { 1.0 } else { -1.0 };
        let g = self.af_g[k];
        let dn = self.af_dn[k];
        let (mut q, mut dq) = (0.0, 0.0);
        for s in self.strip_start[f] as usize..self.strip_start[f + 1] as usize {
            let h = (xu - self.strip_sill[s]).min(H_CAP);
            let bb = (sign * self.strip_u[s] + g * (xu - dn)).max(0.0);
            if h <= H_MIN || bb <= 0.0 {
                if keep {
                    self.strip_h[s] = 0.0;
                    self.strip_unew[s] = 0.0;
                }
                continue;
            }
            let kf = self.fric / (h * h.cbrt());
            let sq = (1.0 + 4.0 * kf * bb).sqrt();
            let mut u = 2.0 * bb / (1.0 + sq);
            let cap = FROUDE_CAP * (GRAV * h).sqrt();
            let d;
            if u > cap {
                u = cap;
                d = 1.5 * u;
            } else {
                d = u * (1.0 + 4.0 / 3.0 * kf * u / sq) + h * g / sq;
            }
            q += h * u;
            dq += d;
            if keep {
                self.strip_h[s] = h;
                self.strip_unew[s] = sign * u;
            }
        }
        (q * wfac, dq * wfac)
    }

    fn edge_flux(&mut self, r: usize, eta: f64, nman: f64, wfac: f64, keep: bool) -> (f64, f64) {
        let (mut q, mut dq) = (0.0, 0.0);
        for s in self.out_start[r] as usize..self.out_start[r + 1] as usize {
            let h = eta - self.out_z[s];
            if h <= H_MIN {
                if keep {
                    self.out_q[s] = 0.0;
                }
                continue;
            }
            let man = h.powf(5.0 / 3.0) * self.out_slope[s].sqrt() / nman;
            let crit = GRAV.sqrt() * h * h.sqrt();
            let (qs, ds) = if man < crit { (man, 5.0 / 3.0 * man / h) } else { (crit, 1.5 * crit / h) };
            if keep {
                self.out_q[s] = qs;
            }
            q += qs;
            dq += ds;
        }
        (q * wfac, dq * wfac)
    }

    fn solve_pool(&mut self, c: usize, dt: f64, nman: f64, wfac: f64) -> bool {
        let (m0, m1) = (self.cl_start[c] as usize, self.cl_start[c + 1] as usize);
        if !self.cl_pool[c] {
            return false;
        }
        let mut level = 0.0;
        let mut total = 0.0;
        for j in m0..m1 {
            let li = self.cl_mem[j] as usize;
            let a = self.storage(self.unk[li] as usize, self.x[li]).1.max(1.0);
            level += a * self.x[li];
            total += a;
        }
        level /= total;
        let (mut lo, mut hi) = (f64::NEG_INFINITY, f64::INFINITY);
        for _ in 0..NEWTON_MAX {
            let (mut f, mut d, mut area) = (0.0, 0.0, 0.0);
            for j in m0..m1 {
                let li = self.cl_mem[j] as usize;
                let r = self.unk[li] as usize;
                let (v, a) = self.storage(r, level);
                let (eq, edq) = self.edge_flux(r, level, nman, wfac, false);
                f += v - self.vn[li] - dt * self.source[r] - self.inflow_known[li] + dt * eq;
                d += self.right_area(r, level) + dt * edq;
                area += a.max(1.0);
            }
            for fi in self.clf_start[c]..self.clf_start[c + 1] {
                let k = self.clf[fi as usize] as usize;
                let (ll, lr) = self.af_loc[k];
                match self.af_kind[k] {
                    FLAT => {
                        if ll != NONE && lr != NONE {
                            continue;
                        }
                        let f_ = self.af[k] as usize;
                        let (a, b) = self.face_lr[f_];
                        let fixed = if ll == NONE { self.eta[a as usize] } else { self.eta[b as usize] };
                        let (el, er, sgn) = if ll == NONE { (fixed, level, -1.0) } else { (level, fixed, 1.0) };
                        let q = self.af_a[k] - self.af_b[k] * (er - el);
                        f += sgn * dt * q;
                        d += dt * self.af_b[k];
                    }
                    kind => {
                        let dn = if kind == FREE_L { lr } else { ll };
                        if dn != NONE && self.cl_of[dn as usize] as usize == c {
                            continue;
                        }
                        let (q, dq) = self.free_flux(k, level, wfac, false);
                        f += dt * q;
                        d += dt * dq;
                    }
                }
            }
            if (f / area).abs() < NEWTON_TOL {
                break;
            }
            if f < 0.0 {
                lo = lo.max(level);
            } else {
                hi = hi.min(level);
            }
            let mut next = level - f / d;
            if !(next > lo && next < hi) {
                next = if lo.is_finite() && hi.is_finite() { 0.5 * (lo + hi) } else if lo.is_finite() { lo.max(next) } else { hi.min(next) };
            }
            level = next;
        }
        for j in m0..m1 {
            let li = self.cl_mem[j] as usize;
            self.x[li] = level;
        }
        true
    }

    fn solve_cluster(&mut self, c: usize, dt: f64, w: &Water) {
        let nman = w.config.manning as f64;
        let wfac = 1.0 / w.cell_m as f64;
        if self.solve_pool(c, dt, nman, wfac) {
            return;
        }
        let (m0, m1) = (self.cl_start[c] as usize, self.cl_start[c + 1] as usize);
        let mut it = 0;
        let (mut lo, mut hi) = (f64::NEG_INFINITY, f64::INFINITY);
        loop {
            for j in m0..m1 {
                let li = self.cl_mem[j] as usize;
                let r = self.unk[li] as usize;
                let eta = self.x[li];
                let (v, a) = self.storage(r, eta);
                let (eq, edq) = self.edge_flux(r, eta, nman, wfac, false);
                self.f[li] = v - self.vn[li] - dt * self.source[r] - self.inflow_known[li] + dt * eq;
                self.d[li] = self.right_area(r, eta) + dt * edq;
                self.area[li] = a;
            }
            for fi in self.clf_start[c]..self.clf_start[c + 1] {
                let k = self.clf[fi as usize] as usize;
                let (ll, lr) = self.af_loc[k];
                let f = self.af[k] as usize;
                let (a, b) = self.face_lr[f];
                match self.af_kind[k] {
                    FLAT => {
                        let el = if ll == NONE { self.eta[a as usize] } else { self.x[ll as usize] };
                        let er = if lr == NONE { self.eta[b as usize] } else { self.x[lr as usize] };
                        let q = self.af_a[k] - self.af_b[k] * (er - el);
                        let cb = dt * self.af_b[k];
                        if ll != NONE {
                            self.f[ll as usize] += dt * q;
                            self.d[ll as usize] += cb;
                        }
                        if lr != NONE {
                            self.f[lr as usize] -= dt * q;
                            self.d[lr as usize] += cb;
                        }
                    }
                    kind => {
                        let (up, dn) = if kind == FREE_L { (ll, lr) } else { (lr, ll) };
                        let (q, dq) = self.free_flux(k, self.x[up as usize], wfac, false);
                        self.f[up as usize] += dt * q;
                        self.d[up as usize] += dt * dq;
                        if dn != NONE && self.cl_of[dn as usize] as usize == c {
                            self.f[dn as usize] -= dt * q;
                        }
                    }
                }
            }
            let (mut err, tol): (f64, f64) = (0.0, if m1 - m0 == 1 { NEWTON_TOL } else { LEVEL_TOL });
            let (mut fsum, mut asum) = (0.0, 0.0);
            for j in m0..m1 {
                let li = self.cl_mem[j] as usize;
                if m1 - m0 == 1 {
                    err = err.max(self.f[li].abs() / self.area[li].max(1.0));
                } else {
                    let a = self.f[li].abs();
                    err = err.max((a / self.d[li]).min(a / self.area[li].max(1.0) * (LEVEL_TOL / NEWTON_TOL)));
                    fsum += self.f[li];
                    asum += self.area[li].max(1.0);
                }
            }
            if m1 - m0 > 1 {
                err = err.max(fsum.abs() / asum * (LEVEL_TOL / NEWTON_TOL));
            }
            if err < tol || it >= NEWTON_MAX {
                break;
            }
            it += 1;
            if m1 - m0 == 1 {
                let li = self.cl_mem[m0] as usize;
                let r = self.unk[li] as usize;
                let (x, f) = (self.x[li], self.f[li]);
                if f < 0.0 {
                    lo = lo.max(x);
                    hi = hi.min(self.level_for(r, self.storage(r, x).0 - f).max(x));
                } else {
                    hi = hi.min(x);
                }
                let mut next = x - f / self.d[li];
                if !(next > lo && next < hi) {
                    next = if lo.is_finite() && hi.is_finite() { 0.5 * (lo + hi) } else if lo.is_finite() { lo.max(next) } else { hi.min(next) };
                }
                self.x[li] = next;
            } else {
                self.cg_cluster(c, (err * LIN_REL).max(LEVEL_TOL * 0.3));
                for j in m0..m1 {
                    let li = self.cl_mem[j] as usize;
                    let r = self.unk[li] as usize;
                    let mut next = self.x[li] + self.dx[li].clamp(-STEP_MAX, STEP_MAX);
                    if self.f[li] < 0.0 && next > self.x[li] {
                        next = next.min(self.level_for(r, self.storage(r, self.x[li]).0 - self.f[li]).max(self.x[li]));
                    }
                    self.x[li] = next;
                }
            }
        }
    }

    fn right_area(&self, r: usize, eta: f64) -> f64 {
        let (s, e) = self.span(r);
        (self.beds[s..e].partition_point(|&z| z <= eta) as f64).max(JAC_FLOOR)
    }

    fn cl_matvec(&self, c: usize, p: &[f64], y: &mut [f64], dt: f64) {
        for j in self.cl_start[c] as usize..self.cl_start[c + 1] as usize {
            let li = self.cl_mem[j] as usize;
            y[li] = self.d[li] * p[li];
        }
        for fi in self.clf_start[c]..self.clf_start[c + 1] {
            let k = self.clf[fi as usize] as usize;
            if self.af_kind[k] != FLAT {
                continue;
            }
            let (ll, lr) = self.af_loc[k];
            if ll != NONE && lr != NONE {
                let cb = dt * self.af_b[k];
                y[ll as usize] -= cb * p[lr as usize];
                y[lr as usize] -= cb * p[ll as usize];
            }
        }
    }

    fn build_coarse(&mut self, c: usize, dt: f64) -> bool {
        let (m0, m1) = (self.cl_start[c] as usize, self.cl_start[c + 1] as usize);
        if m1 - m0 < AGG_MIN {
            return false;
        }
        let mut size = AGG as f64 * C as f64;
        let mut keys: Vec<(u32, u32)> = Vec::with_capacity(m1 - m0);
        let na = loop {
            keys.clear();
            for j in m0..m1 {
                let li = self.cl_mem[j];
                let (cx, cy) = self.centroid[self.unk[li as usize] as usize];
                keys.push((((cy / size) as u32) << 16 | (cx / size) as u32, li));
            }
            keys.sort_unstable();
            let mut na = 0;
            for k in 0..keys.len() {
                if k == 0 || keys[k].0 != keys[k - 1].0 {
                    na += 1;
                }
            }
            if na <= AGG_MAX {
                break na;
            }
            size *= 2.0;
        };
        let mut a = 0usize;
        for k in 0..keys.len() {
            if k > 0 && keys[k].0 != keys[k - 1].0 {
                a += 1;
            }
            self.agg_of[keys[k].1 as usize] = a as u32;
        }
        self.agg_n = na;
        let e = &mut self.coarse;
        e.clear();
        e.resize(na * na, 0.0);
        for j in m0..m1 {
            let li = self.cl_mem[j] as usize;
            let ai = self.agg_of[li] as usize;
            e[ai * na + ai] += self.d[li];
        }
        for fi in self.clf_start[c]..self.clf_start[c + 1] {
            let k = self.clf[fi as usize] as usize;
            if self.af_kind[k] != FLAT {
                continue;
            }
            let (ll, lr) = self.af_loc[k];
            if ll != NONE && lr != NONE {
                let cb = dt * self.af_b[k];
                let (ai, bi) = (self.agg_of[ll as usize] as usize, self.agg_of[lr as usize] as usize);
                e[ai * na + bi] -= cb;
                e[bi * na + ai] -= cb;
            }
        }
        for j in 0..na {
            let mut s = e[j * na + j];
            for k in 0..j {
                s -= e[j * na + k] * e[j * na + k];
            }
            if s <= 0.0 {
                return false;
            }
            let djj = s.sqrt();
            e[j * na + j] = djj;
            for i in j + 1..na {
                let mut s = e[i * na + j];
                for k in 0..j {
                    s -= e[i * na + k] * e[j * na + k];
                }
                e[i * na + j] = s / djj;
            }
        }
        true
    }

    fn precond(&mut self, c: usize, coarse: bool, r: &[f64], z: &mut [f64]) {
        let (m0, m1) = (self.cl_start[c] as usize, self.cl_start[c + 1] as usize);
        for j in m0..m1 {
            let li = self.cl_mem[j] as usize;
            z[li] = r[li] / self.d[li];
        }
        if !coarse {
            return;
        }
        let na = self.agg_n;
        let y = &mut self.coarse_y;
        y.clear();
        y.resize(na, 0.0);
        for j in m0..m1 {
            let li = self.cl_mem[j] as usize;
            y[self.agg_of[li] as usize] += r[li];
        }
        let e = &self.coarse;
        for i in 0..na {
            let mut s = y[i];
            for k in 0..i {
                s -= e[i * na + k] * y[k];
            }
            y[i] = s / e[i * na + i];
        }
        for i in (0..na).rev() {
            let mut s = y[i];
            for k in i + 1..na {
                s -= e[k * na + i] * y[k];
            }
            y[i] = s / e[i * na + i];
        }
        for j in m0..m1 {
            let li = self.cl_mem[j] as usize;
            z[li] += y[self.agg_of[li] as usize];
        }
    }

    fn cg_cluster(&mut self, c: usize, tol: f64) {
        let dt = self.dt;
        let (m0, m1) = (self.cl_start[c] as usize, self.cl_start[c + 1] as usize);
        let m = self.unk.len();
        if self.agg_of.len() < m {
            self.agg_of.resize(m, 0);
        }
        let coarse = self.build_coarse(c, dt);
        let mut wk = std::mem::take(&mut self.work);
        for v in wk.iter_mut() {
            if v.len() != m {
                v.clear();
                v.resize(m, 0.0);
            }
        }
        let [r, z, p, ap, ..] = &mut wk;
        for j in m0..m1 {
            let li = self.cl_mem[j] as usize;
            self.dx[li] = 0.0;
            r[li] = -self.f[li];
        }
        self.precond(c, coarse, r, z);
        let mut rz = 0.0;
        for j in m0..m1 {
            let li = self.cl_mem[j] as usize;
            p[li] = z[li];
            rz += r[li] * z[li];
        }
        for _ in 0..LIN_MAX {
            let (mut err, mut rsum, mut asum): (f64, f64, f64) = (0.0, 0.0, 0.0);
            for j in m0..m1 {
                let li = self.cl_mem[j] as usize;
                let a = r[li].abs();
                err = err.max((a / self.d[li]).min(a / self.area[li].max(1.0) * (LEVEL_TOL / NEWTON_TOL)));
                rsum += r[li];
                asum += self.area[li].max(1.0);
            }
            err = err.max(rsum.abs() / asum * (LEVEL_TOL / NEWTON_TOL));
            if err < tol {
                break;
            }
            self.cl_matvec(c, p, ap, dt);
            let mut pap = 0.0;
            for j in m0..m1 {
                let li = self.cl_mem[j] as usize;
                pap += p[li] * ap[li];
            }
            if pap <= 0.0 {
                break;
            }
            let alpha = rz / pap;
            for j in m0..m1 {
                let li = self.cl_mem[j] as usize;
                self.dx[li] += alpha * p[li];
                r[li] -= alpha * ap[li];
            }
            self.precond(c, coarse, r, z);
            let mut rz1 = 0.0;
            for j in m0..m1 {
                let li = self.cl_mem[j] as usize;
                rz1 += r[li] * z[li];
            }
            let beta = rz1 / rz;
            rz = rz1;
            for j in m0..m1 {
                let li = self.cl_mem[j] as usize;
                p[li] = z[li] + beta * p[li];
            }
        }
        self.work = wk;
    }

    fn finish(&mut self, w: &mut Water, dt: f64) {
        let m = self.unk.len();
        for li in 0..m {
            self.theta[li] = 1.0;
        }
        let th = |t: &[f64], l: u32| if l == NONE { 1.0 } else { t[l as usize] };
        for round in 0..LIMIT_ROUNDS {
            for li in 0..m {
                self.inflow[li] = 0.0;
                self.outflow[li] = dt * self.qout[li] * self.theta[li];
            }
            for k in 0..self.af.len() {
                let (ll, lr) = self.af_loc[k];
                let q = self.af_q[k];
                let (from, to, amt) = if q > 0.0 { (ll, lr, dt * q * th(&self.theta, ll)) } else { (lr, ll, -dt * q * th(&self.theta, lr)) };
                if from != NONE {
                    self.outflow[from as usize] += amt;
                }
                if to != NONE {
                    self.inflow[to as usize] += amt;
                }
            }
            if round + 1 == LIMIT_ROUNDS {
                break;
            }
            let mut changed = false;
            for li in 0..m {
                let r = self.unk[li] as usize;
                let avail = self.vn[li] + dt * self.source[r] + self.inflow[li];
                if self.outflow[li] > avail * (1.0 + 1.0e-12) + 1.0e-12 {
                    self.theta[li] *= (avail / self.outflow[li]).max(0.0);
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }
        for k in 0..self.af.len() {
            let (ll, lr) = self.af_loc[k];
            let q = self.af_q[k];
            let f = self.af[k] as usize;
            let (from, to) = if q > 0.0 { (ll, lr) } else { (lr, ll) };
            let t = th(&self.theta, from);
            let amt = dt * q.abs() * t;
            if from == NONE && to != NONE {
                w.books.sources += amt;
                w.added_volume += amt;
            } else if to == NONE && from != NONE {
                w.books.edges += amt;
                w.lost_volume += amt;
            }
            for s in self.strip_start[f] as usize..self.strip_start[f + 1] as usize {
                let u = self.strip_unew[s] * t;
                self.strip_u[s] = u * INERTIA;
                self.strip_q[s] = u * self.strip_h[s];
            }
        }
        for li in 0..m {
            let r = self.unk[li] as usize;
            self.f[li] = self.vn[li] + dt * self.source[r] + self.inflow[li] - self.outflow[li];
        }
        self.settle_clusters(dt);
        self.edge_regions.clear();
        for li in 0..m {
            let r = self.unk[li] as usize;
            let t = self.theta[li];
            if self.qout[li] > 0.0 {
                let amt = dt * self.qout[li] * t;
                w.books.edges += amt;
                w.lost_volume += amt;
                for s in self.out_start[r] as usize..self.out_start[r + 1] as usize {
                    self.out_q[s] *= t;
                }
                self.edge_regions.push(r as u32);
            }
            let input = dt * self.source[r];
            w.books.sources += input;
            w.added_volume += input;
            let mut v = self.f[li];
            if v < 0.0 {
                w.books.sources -= v;
                w.added_volume -= v;
                v = 0.0;
            } else if v < TRACE {
                w.books.seepage += v;
                w.lost_volume += v;
                v = 0.0;
            }
            self.vol[r] = v;
            self.eta[r] = self.level_for(r, v);
        }
        for k in 0..self.source_regions.len() {
            let r = self.source_regions[k] as usize;
            if self.fixed[r].is_finite() {
                let input = dt * self.source[r];
                w.books.sources += input;
                w.added_volume += input;
                w.books.edges += input;
                w.lost_volume += input;
            }
        }
        self.wet.clear();
        for k in 0..self.act.len() {
            let r = self.act[k] as usize;
            if self.is_wet(r) {
                self.wet.push(r as u32);
            }
        }
    }

    fn settle_clusters(&mut self, dt: f64) {
        let m = self.unk.len();
        let nfa = self.af.len();
        let mut deg = vec![0u32; m + 1];
        for k in 0..nfa {
            let (ll, lr) = self.af_loc[k];
            if self.af_kind[k] == FLAT && ll != NONE && lr != NONE {
                deg[ll as usize + 1] += 1;
                deg[lr as usize + 1] += 1;
            }
        }
        for i in 0..m {
            deg[i + 1] += deg[i];
        }
        let mut adj = vec![0u32; deg[m] as usize];
        let mut fill = deg.clone();
        for k in 0..nfa {
            let (ll, lr) = self.af_loc[k];
            if self.af_kind[k] == FLAT && ll != NONE && lr != NONE {
                adj[fill[ll as usize] as usize] = k as u32;
                fill[ll as usize] += 1;
                adj[fill[lr as usize] as usize] = k as u32;
                fill[lr as usize] += 1;
            }
        }
        let mut seen = vec![false; m];
        let mut parent = vec![NONE; m];
        let mut order: Vec<u32> = Vec::new();
        for c in 0..self.cl_start.len().saturating_sub(1) {
            let (m0, m1) = (self.cl_start[c] as usize, self.cl_start[c + 1] as usize);
            if m1 - m0 < 2 {
                continue;
            }
            let root = self.cl_mem[m0];
            order.clear();
            order.push(root);
            seen[root as usize] = true;
            let mut h = 0;
            while h < order.len() {
                let li = order[h] as usize;
                h += 1;
                for e in deg[li]..deg[li + 1] {
                    let k = adj[e as usize] as usize;
                    let (ll, lr) = self.af_loc[k];
                    let o = if ll as usize == li { lr } else { ll };
                    if !seen[o as usize] {
                        seen[o as usize] = true;
                        parent[o as usize] = k as u32;
                        order.push(o);
                    }
                }
            }
            for idx in (1..order.len()).rev() {
                let li = order[idx] as usize;
                let r = self.unk[li] as usize;
                let excess = self.f[li] - self.storage(r, self.x[li]).0;
                let k = parent[li] as usize;
                let (ll, lr) = self.af_loc[k];
                let up = if ll as usize == li { lr } else { ll } as usize;
                self.f[li] -= excess;
                self.f[up] += excess;
                self.af_q[k] += if ll as usize == li { excess / dt } else { -excess / dt };
            }
            let rr = self.unk[root as usize] as usize;
            let left = self.f[root as usize] - self.storage(rr, self.x[root as usize]).0;
            let weight = |s: &Self, li: usize| if left >= 0.0 { s.area[li].max(1.0) } else { s.storage(s.unk[li] as usize, s.x[li]).0 };
            let total: f64 = order.iter().map(|&li| weight(self, li as usize)).sum();
            for &li in &order {
                let li = li as usize;
                self.dx[li] = if total > 0.0 { left * weight(self, li) / total } else { 0.0 };
            }
            if total <= 0.0 {
                self.dx[root as usize] = left;
            }
            for idx in (1..order.len()).rev() {
                let li = order[idx] as usize;
                let k = parent[li] as usize;
                let (ll, lr) = self.af_loc[k];
                let up = if ll as usize == li { lr } else { ll } as usize;
                let take = self.dx[li];
                self.f[li] += take;
                self.f[up] -= take;
                self.dx[up] += take;
                self.af_q[k] -= if ll as usize == li { take / dt } else { -take / dt };
            }
        }
    }

    pub(super) fn environment(&mut self, w: &mut Water, days: f64) {
        w.update_riverbed(days as f32);
        let forcing = w.forcing();
        let r = forcing.rain as f64 * days;
        let e = w.config.evaporation as f64 * forcing.evaporation as f64 * days;
        let s = w.config.seepage as f64 * days;
        let (mut rain, mut evaporation, mut seepage) = (0.0, 0.0, 0.0);
        for k in 0..self.wet.len() {
            let id = self.wet[k] as usize;
            if self.fixed[id].is_finite() || self.vol[id] <= 0.0 {
                continue;
            }
            let level = self.eta[id];
            let (a, b) = self.span(id);
            let mut change = 0.0;
            for c in a..b {
                let i = self.cells[c] as usize;
                let depth = (level - w.z[i] as f64).max(0.0);
                if depth == 0.0 {
                    continue;
                }
                let ev = e.min(depth + r);
                let infiltration = if w.riverbed.as_slice()[i] < 0.5 && !w.canal.as_slice()[i] { 4.0 * s } else { s };
                let sp = infiltration.min((depth + r - ev).max(0.0));
                change += r - ev - sp;
                rain += r;
                evaporation += ev;
                seepage += sp;
            }
            let v = self.vol[id] + change;
            if v < 0.0 {
                seepage += v;
            }
            self.vol[id] = v.max(0.0);
            self.eta[id] = self.level_for(id, self.vol[id]);
        }
        self.stale = true;
        w.books.rain += rain;
        w.books.evaporation += evaporation;
        w.books.seepage += seepage;
        w.added_volume += rain;
        w.lost_volume += evaporation + seepage;
        let (vol, fixed) = (&self.vol, &self.fixed);
        self.wet.retain(|&r| vol[r as usize] > 0.0 || fixed[r as usize].is_finite());
    }

    pub(super) fn publish(&mut self, w: &mut Water) {
        if !std::mem::take(&mut self.stale) {
            return;
        }
        let n = self.n;
        let cell_m = w.cell_m as f64;
        self.pub_stamp += 1;
        let stamp = self.pub_stamp;
        if self.pub_mark.len() != self.nreg() {
            self.pub_mark = vec![0; self.nreg()];
            self.pub_eta = vec![f64::NAN; self.nreg()];
        }
        for k in 0..self.wet.len() {
            self.pub_mark[self.wet[k] as usize] = stamp;
        }
        let mut changed = std::mem::take(&mut self.rows_dirty);
        for k in 0..self.published.len() {
            let r = self.published[k] as usize;
            if self.pub_mark[r] == stamp {
                continue;
            }
            self.pub_eta[r] = f64::NAN;
            let (a, b) = self.span(r);
            for c in a..b {
                let i = self.cells[c] as usize;
                changed |= w.h[i] > 0.0;
                w.h[i] = 0.0;
            }
        }
        for &code in &self.pub_faces {
            if code & Y_FACE != 0 {
                w.qy[(code & IDX) as usize] = 0.0;
            } else {
                w.qx[(code & IDX) as usize] = 0.0;
            }
        }
        self.pub_faces.clear();
        for k in 0..self.wet.len() {
            self.regq[self.wet[k] as usize] = (0.0, 0.0);
        }
        for k in 0..self.af.len() {
            let f = self.af[k] as usize;
            let (a, b) = self.face_lr[f];
            for s in self.strip_start[f] as usize..self.strip_start[f + 1] as usize {
                let q = self.strip_q[s];
                if q == 0.0 {
                    continue;
                }
                let code = self.strip_code[s];
                let sq = if code & NEG != 0 { -q } else { q };
                let half = 0.5 * sq * cell_m;
                self.pub_faces.push(code & (Y_FACE | IDX));
                if code & Y_FACE != 0 {
                    w.qy[(code & IDX) as usize] = sq as f32;
                    self.regq[a as usize].1 += half;
                    self.regq[b as usize].1 += half;
                } else {
                    w.qx[(code & IDX) as usize] = sq as f32;
                    self.regq[a as usize].0 += half;
                    self.regq[b as usize].0 += half;
                }
            }
        }
        for k in 0..self.edge_regions.len() {
            let r = self.edge_regions[k] as usize;
            for s in self.out_start[r] as usize..self.out_start[r + 1] as usize {
                let q = self.out_q[s];
                if q == 0.0 {
                    continue;
                }
                let code = self.out_code[s];
                let sq = if code & NEG != 0 { -q } else { q };
                self.pub_faces.push(code & (Y_FACE | IDX));
                if code & Y_FACE != 0 {
                    w.qy[(code & IDX) as usize] = sq as f32;
                    self.regq[r].1 += 0.5 * sq * cell_m;
                } else {
                    w.qx[(code & IDX) as usize] = sq as f32;
                    self.regq[r].0 += 0.5 * sq * cell_m;
                }
            }
        }
        for k in 0..self.wet.len() {
            let r = self.wet[k] as usize;
            let eta = if self.fixed[r].is_finite() { self.fixed[r] } else { self.eta[r] };
            if (eta - self.pub_eta[r]).abs() < PUB_EPS {
                continue;
            }
            self.pub_eta[r] = eta;
            let (a, b) = self.span(r);
            let mut sorted = [(0.0f64, 0u32); C * C];
            let len = b - a;
            let (gx, gy, lo, hi) = if self.fixed[r].is_finite() { (0.0, 0.0, eta, eta) } else { self.surface_slope(r) };
            let (cx, cy) = self.centroid[r];
            for (t, c) in (a..b).enumerate() {
                let i = self.cells[c] as usize;
                let dl = gx * ((i % n) as f64 - cx) + gy * ((i / n) as f64 - cy);
                sorted[t] = (w.z[i] as f64 - dl, i as u32);
            }
            let _ = (lo, hi);
            let sorted = &mut sorted[..len];
            let top = if gx == 0.0 && gy == 0.0 {
                eta
            } else {
                let (sum, top_bed) = sorted.iter().fold((0.0, f64::NEG_INFINITY), |(s, m), p| (s + p.0, m.max(p.0)));
                let flat = (self.vol[r] + sum) / len as f64;
                if flat >= top_bed {
                    flat
                } else {
                    sorted.sort_unstable_by(|p, q| p.0.total_cmp(&q.0).then(p.1.cmp(&q.1)));
                    let (mut acc, mut top) = (0.0, sorted[0].0);
                    for t in 0..len {
                        acc += sorted[t].0;
                        top = (self.vol[r] + acc) / (t + 1) as f64;
                        if t + 1 == len || top <= sorted[t + 1].0 {
                            break;
                        }
                    }
                    top
                }
            };
            for &(base, i) in sorted.iter() {
                let h = (top - base).max(0.0) as f32;
                let i = i as usize;
                changed |= (w.h[i] > 0.0) != (h > 0.0);
                w.h[i] = h;
            }
        }
        let w1 = n + 1;
        for k in 0..self.wet.len() {
            let r = self.wet[k] as usize;
            let (a, b) = self.span(r);
            let (qx, qy) = self.regq[r];
            if b - a < 2 || qx.abs() + qy.abs() < Q_STILL {
                continue;
            }
            let (mut col, mut row) = ([0.0f64; C], [0.0f64; C]);
            for c in a..b {
                let i = self.cells[c] as usize;
                let h = w.h[i];
                let cb = h.cbrt();
                let cv = h * cb * cb;
                self.ty[i] = cv;
                col[(i % n) % C] += cv as f64;
                row[(i / n) % C] += cv as f64;
            }
            for c in a..b {
                let i = self.cells[c] as usize;
                let cv = self.ty[i] as f64;
                let (cs, rs) = (col[(i % n) % C], row[(i / n) % C]);
                self.tx[i] = if cs > 0.0 { (qx / cell_m * cv / cs) as f32 } else { 0.0 };
                self.ty[i] = if rs > 0.0 { (qy / cell_m * cv / rs) as f32 } else { 0.0 };
            }
            for c in a..b {
                let i = self.cells[c] as usize;
                let (x, y) = (i % n, i / n);
                if x + 1 < n && self.owner[i + 1] as usize == r {
                    let q = 0.5 * (self.tx[i] + self.tx[i + 1]);
                    if q != 0.0 {
                        w.qx[y * w1 + x + 1] = q;
                        self.pub_faces.push((y * w1 + x + 1) as u32);
                    }
                }
                if y + 1 < n && self.owner[i + n] as usize == r {
                    let q = 0.5 * (self.ty[i] + self.ty[i + n]);
                    if q != 0.0 {
                        w.qy[(y + 1) * n + x] = q;
                        self.pub_faces.push(Y_FACE | ((y + 1) * n + x) as u32);
                    }
                }
            }
        }
        if changed {
            for row in &mut w.rows {
                row.clear();
            }
            for k in 0..self.wet.len() {
                let (a, b) = self.span(self.wet[k] as usize);
                for c in a..b {
                    let i = self.cells[c] as usize;
                    if w.h[i] > 0.0 {
                        w.rows[i / n].push(((i % n) as u32, (i % n) as u32 + 1));
                    }
                }
            }
            for row in &mut w.rows {
                row.sort_unstable();
                let mut count = 0;
                for read in 0..row.len() {
                    let (a, b) = row[read];
                    if count > 0 && a <= row[count - 1].1 {
                        row[count - 1].1 = row[count - 1].1.max(b);
                    } else {
                        row[count] = (a, b);
                        count += 1;
                    }
                }
                row.truncate(count);
            }
        }
        self.published.clone_from(&self.wet);
        debug_assert!(w.h.iter().chain(&w.qx).chain(&w.qy).all(|v| v.is_finite()), "subgrid published a non-finite depth or flux");
    }

    fn surface_slope(&self, r: usize) -> (f64, f64, f64, f64) {
        let n = self.n;
        let eta = self.eta[r];
        let (cx, cy) = self.centroid[r];
        let (mut m00, mut m01, mut m11, mut b0, mut b1) = (LS_REG, 0.0, LS_REG, 0.0, 0.0);
        let (mut hi, mut lo) = (eta, eta);
        for fi in self.rf_start[r]..self.rf_start[r + 1] {
            let f = self.rf[fi as usize] as usize;
            let (fa, fb) = self.face_lr[f];
            let o = if fa as usize == r { fb } else { fa } as usize;
            if !self.is_wet(o) || eta.max(self.eta[o]) <= self.face_sill[f] {
                continue;
            }
            let (dx, dy) = (self.centroid[o].0 - cx, self.centroid[o].1 - cy);
            let de = self.eta[o] - eta;
            m00 += dx * dx;
            m01 += dx * dy;
            m11 += dy * dy;
            b0 += dx * de;
            b1 += dy * de;
            hi = hi.max(self.eta[o]);
            lo = lo.min(self.eta[o]);
        }
        if hi - lo < 1.0e-9 {
            return (0.0, 0.0, lo, hi);
        }
        let det = m00 * m11 - m01 * m01;
        let (gx, gy) = ((m11 * b0 - m01 * b1) / det, (m00 * b1 - m01 * b0) / det);
        let mut phi: f64 = 1.0;
        let (a, b) = self.span(r);
        for c in a..b {
            let i = self.cells[c] as usize;
            let dl = gx * ((i % n) as f64 - cx) + gy * ((i / n) as f64 - cy);
            if dl > 1.0e-12 {
                phi = phi.min((hi - eta) / dl);
            } else if dl < -1.0e-12 {
                phi = phi.min((lo - eta) / dl);
            }
        }
        if (gx * phi).abs() + (gy * phi).abs() < 1.0e-9 {
            (0.0, 0.0, lo, hi)
        } else {
            (gx * phi, gy * phi, lo, hi)
        }
    }
}

impl super::WaterEngine for Subgrid {
    fn prepare(&mut self, w: &mut Water, sync: bool) { Subgrid::prepare(self, w, sync) }
    fn advance(&mut self, w: &mut Water, seconds: f64) { Subgrid::advance(self, w, seconds) }
    fn environment(&mut self, w: &mut Water, days: f64) { Subgrid::environment(self, w, days) }
    fn publish(&mut self, w: &mut Water) { Subgrid::publish(self, w) }
    fn total_volume(&self) -> Option<f64> { Subgrid::total_volume(self) }
}
