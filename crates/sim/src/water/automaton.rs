//! Game-tuned cellular automaton on the fine grid (after WCA2D, Guidolin et al. 2016).
//! Each tick the wet cells are swept from the highest water surface down (Gauss-Seidel), so
//! water runs the length of a channel in one tick. A sender solves its own end level
//! implicitly against Manning/critical conveyance on its faces, never drops below a receiver's
//! surface, and only fills a receiver that cannot pass water on (a hollow, or the cell that fed
//! it this tick) up to its own level. Connected water that is deep and nearly flat is one pool:
//! it holds one level, takes in what reaches it and lets out what crosses its rim, so ponds and
//! lakes stay flat and still. Cells that receive water after their turn get a second sweep;
//! water still left standing well above all its neighbours settles into its hollow.
use super::{Water, WEIR};
use std::collections::BinaryHeap;

/// Automaton tick in seconds; shorter advances are carried over to the next tick.
const TICK_S: f64 = 900.0;
/// Surface drop in metres below which a face carries nothing (keeps still water still).
const EPS_DROP: f64 = 1.0e-5;
/// Drop in metres below an edge lake's level before the lake tops up the water next to it.
const EPS_LAKE: f64 = 2.0e-3;
/// Depth in metres water must reach on a dry cell to spread onto it sideways (else steepest face only).
const H_SPREAD: f64 = 0.08;
/// Depth in metres a neighbour needs to count as wet for the stay-together rule.
const H_WET: f64 = 5.0e-3;
/// Depth in metres under which a cell's water is a trace and soaks away at the environment step.
const H_TRACE: f64 = 5.0e-4;
/// Depth in metres below which a film clings to the ground and does not flow.
const H_STILL: f64 = 2.0e-4;
/// Depth in metres a cell needs to join its neighbours in one level pool.
const H_POOL: f64 = 0.01;
/// Largest surface step in metres between two neighbours of one pool.
const POOL_STEP: f64 = 0.003;
/// Extra surface step allowed within a pool, as a fraction of the shallower depth (deep water levels fast).
const POOL_REL: f64 = 0.2;
/// Newton iterations allowed for one implicit level solve.
const MAX_IT: usize = 16;
/// Volume residual (metres of depth on one cell) at which the implicit solve stops.
const TOL: f64 = 5.0e-5;

const W: u8 = 0;
const E: u8 = 1;
const N: u8 = 2;
const S: u8 = 3;
const NONE: u32 = u32::MAX;
/// Surface step in metres above every neighbour at which leftover water is settled into its hollow.
const SETTLE_STEP: f64 = 0.2;
/// Largest hollow (cells) one settle may fill.
const SETTLE_MAX: usize = 64;
/// Settle-and-sweep rounds per tick.
const SETTLE_ROUNDS: usize = 2;
/// Times a cell or pool may be swept in one tick (more water may reach it after its turn).
const MAX_VISITS: u8 = 2;
/// Depth in metres a delivery must bring to sweep a cell (or pool) again in the same tick.
const REVISIT_MIN: f64 = 0.005;
/// Times a pool may be swept in one tick.
const MAX_POOL_VISITS: u8 = 2;

#[derive(Clone, Copy, Debug, Default, serde::Serialize, serde::Deserialize)]
struct Cand {
    /// Cell the water leaves from (a pool member for pools).
    from: usize,
    /// Receiving cell, or usize::MAX for the open edge.
    j: usize,
    dir: u8,
    /// Receiver surface, face crest and the level the sender must exceed.
    s: f64,
    zf: f64,
    thr: f64,
    /// Fill the receiver at most up to the sender's level.
    cap: bool,
    /// Plan area (cells) over which the receiver rises when capped.
    area: f64,
    /// Receiver absorbs (open edge or fixed lake): booked as an edge loss.
    sink: bool,
    t: f64,
}

#[derive(Clone, Copy, Debug, Default, serde::Serialize, serde::Deserialize)]
struct Cell {
    z: f64,
    h: f64,
}

#[derive(Clone, Copy, Debug, Default, serde::Serialize, serde::Deserialize)]
struct Pool {
    start: usize,
    len: usize,
    volume: f64,
    level: f64,
    dirty: bool,
    /// Times the pool has been swept this tick.
    visits: u8,
    /// Some member can pass water out of the pool (a lower non-member neighbour or open edge).
    outlet: bool,
    /// Members on the pool's rim, as a range of `rim`.
    rim_start: usize,
    rim_len: usize,
    /// Members holding water at the pool's level.
    area: f64,
}

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub(super) struct Automaton {
    version: Option<((u64, u64), (u64, u64), (u64, u64), (u64, u64), bool)>,
    n: usize,
    dx: f64,
    /// Bed and depth per cell, kept together for locality.
    c: Vec<Cell>,
    /// Boundary cells held at an external lake level.
    fix: Vec<bool>,
    /// Fixed external lake level on boundary cells, NaN elsewhere.
    lake: Vec<f64>,
    /// Bit per side (W, E, N, S) where water may leave the map.
    open: Vec<u8>,
    active: Vec<u32>,
    in_active: Vec<bool>,
    added: Vec<u32>,
    stamp: Vec<u32>,
    visits: Vec<u8>,
    mark: Vec<u32>,
    mark_id: u32,
    /// Tick in which water last crossed each face, per direction: [x faces +, x faces -, y +, y -].
    crossed: [Vec<u32>; 4],
    tick: u32,
    heap: BinaryHeap<(u64, u32)>,
    /// Depth each cell ended its last tick at: the warm start for its level solve.
    last: Vec<f32>,
    pool_of: Vec<u32>,
    pools: Vec<Pool>,
    members: Vec<u32>,
    rim: Vec<u32>,
    cands: Vec<Cand>,
    fx: Vec<f64>,
    fy: Vec<f64>,
    fx_list: Vec<u32>,
    fy_list: Vec<u32>,
    flux_s: f64,
    published: Vec<u32>,
    /// Seconds advanced but not yet ticked.
    pending: f64,
}


#[inline]
fn key(s: f64) -> u64 {
    let b = s.to_bits();
    if b >> 63 == 0 { b | (1 << 63) } else { !b }
}

/// Cube root to ~1e-7 relative: a bit-level guess refined by two Newton steps.
#[inline]
fn cbrt(x: f64) -> f64 {
    let mut y = f64::from_bits(x.to_bits() / 3 + 0x2A9F_7893_782D_A1CE);
    y -= (y * y * y - x) / (3.0 * y * y);
    y -= (y * y * y - x) / (3.0 * y * y);
    y
}

/// Level at which `volume` (metres of depth summed over cells) rests on beds sorted ascending.
fn fill_level(beds: &[f64], volume: f64) -> f64 {
    let mut sum = 0.0;
    for (k, &b) in beds.iter().enumerate() {
        sum += b;
        let l = (volume + sum) / (k + 1) as f64;
        if k + 1 == beds.len() || l <= beds[k + 1] {
            return l;
        }
    }
    beds[0] + volume
}

/// Face transfer over the tick (metres of depth on the sender) and its derivative in level.
#[inline]
fn eval(c: &mut [Cand], l: f64, coef: f64, inv_n: f64, inv_dx: f64) -> (f64, f64) {
    let (mut sum, mut dsum) = (0.0, 0.0);
    for c in c.iter_mut() {
        if l <= c.thr {
            c.t = 0.0;
            continue;
        }
        let hf = l - c.zf;
        let ds = l - c.s;
        let cb = cbrt(hf);
        let man = hf * cb * cb * (ds * inv_dx).sqrt() * inv_n;
        let weir = WEIR as f64 * hf * hf.sqrt();
        let (q, dq) = if man < weir { (man, man * (5.0 / 3.0 / hf + 0.5 / ds)) } else { (weir, 1.5 * weir / hf) };
        let (mut t, mut dt) = (coef * q, coef * dq);
        if c.cap && ds * c.area < t {
            t = ds * c.area;
            dt = c.area;
        }
        c.t = t;
        sum += t;
        dsum += dt;
    }
    (sum, dsum)
}

/// End level L of a sender of plan area `area` (cells) starting at `s`: area·(s − L) = Σ t(L).
/// Returns the total sent; `c[..].t` hold the transfers. Never lands below a receiver's surface.
fn solve(c: &mut [Cand], s: f64, floor: f64, area: f64, guess: f64, coef: f64, inv_n: f64, inv_dx: f64) -> f64 {
    let min_thr = c.iter().map(|c| c.thr).fold(f64::INFINITY, f64::min);
    let (mut lo, mut hi) = (floor.max(min_thr), s);
    if hi <= lo {
        return 0.0;
    }
    let g_at = |c: &mut [Cand], l: f64| area * (l - s) + eval(c, l, coef, inv_n, inv_dx).0;
    // A face's flux rises like sqrt just above the receiver's surface. Bracket the root between
    // consecutive receiver levels, highest first, so no such corner lies inside the bracket.
    let mut thr: [f64; 8] = [f64::NEG_INFINITY; 8];
    let mut k = 0;
    for cand in c.iter() {
        if cand.thr > lo && cand.thr < hi && k < thr.len() {
            thr[k] = cand.thr;
            k += 1;
        }
    }
    thr[..k].sort_unstable_by(|a, b| b.total_cmp(a));
    for &t in &thr[..k] {
        if t <= lo || t >= hi {
            continue;
        }
        if g_at(c, t) >= 0.0 { hi = t } else { lo = t; break }
    }
    // Newton in u = sqrt(L - lo), safeguarded by the bracket.
    let span = hi - lo;
    let (mut ulo, mut uhi) = (0.0f64, span.sqrt());
    let g0 = guess - lo;
    let mut u = if g0 > 0.0 && g0 < span { g0.sqrt() } else { 0.5 * uhi };
    let mut it = 0;
    loop {
        let l = lo + u * u;
        let (sum, ds) = eval(c, l, coef, inv_n, inv_dx);
        let g = area * (l - s) + sum;
        it += 1;
        if g.abs() < TOL * area || it >= MAX_IT || uhi - ulo < 1.0e-7 {
            let mut total = sum;
            if g > 0.0 && sum > 0.0 {
                total = area * (s - l);
                let scale = total / sum;
                c.iter_mut().for_each(|c| c.t *= scale);
            }
            return total;
        }
        if g > 0.0 { uhi = u } else { ulo = u }
        let dgdu = (area + ds) * 2.0 * u;
        let next = if dgdu > 0.0 { u - g / dgdu } else { f64::NAN };
        u = if next > ulo && next < uhi { next } else { 0.5 * (ulo + uhi) };
    }
}

impl Automaton {
    #[inline]
    fn fixed(&self, i: usize) -> bool { self.fix[i] }

    #[inline]
    fn surface(&self, i: usize) -> f64 {
        let c = self.c[i];
        c.z + c.h
    }

    fn ghost(&self, i: usize, dir: u8) -> f64 {
        let n = self.n;
        let inner = match dir { W => i + 1, E => i - 1, N => i + n, _ => i - n };
        self.c[i].z - ((self.c[inner].z - self.c[i].z) / self.dx).max(super::EDGE_SLOPE_MIN as f64) * self.dx
    }

    #[inline]
    fn neighbour(&self, i: usize, dir: u8) -> Option<usize> {
        let n = self.n;
        let (x, y) = (i % n, i / n);
        match dir {
            W => (x > 0).then(|| i - 1),
            E => (x + 1 < n).then(|| i + 1),
            N => (y > 0).then(|| i - n),
            _ => (y + 1 < n).then(|| i + n),
        }
    }

    fn rebuild(&mut self, w: &mut Water) {
        let n = w.n;
        let count = n * n;
        if self.n != n {
            self.n = n;
            self.c = vec![Cell::default(); count];
            self.last = vec![0.0; count];
            self.in_active = vec![false; count];
            self.stamp = vec![0; count];
            self.visits = vec![0; count];
            self.mark = vec![0; count];
            self.crossed = [vec![0; n * (n + 1)], vec![0; n * (n + 1)], vec![0; (n + 1) * n], vec![0; (n + 1) * n]];
            self.pool_of = vec![NONE; count];
            self.fx = vec![0.0; n * (n + 1)];
            self.fy = vec![0.0; (n + 1) * n];
            self.active.clear();
        }
        self.dx = w.cell_m as f64;
        for (c, &z) in self.c.iter_mut().zip(&w.z) {
            c.z = z as f64;
        }
        self.lake = w.lake_level.as_slice().iter().map(|&l| if l.is_finite() { l as f64 } else { f64::NAN }).collect();
        self.fix = self.lake.iter().map(|l| !l.is_nan()).collect();
        self.open = vec![0; count];
        if w.config.open_edges {
            for i in 0..count {
                let (x, y) = (i % n, i / n);
                if w.closed_edge.as_slice()[i] || self.fixed(i) {
                    continue;
                }
                let mut m = 0;
                if x == 0 { m |= 1 << W; }
                if x + 1 == n { m |= 1 << E; }
                if y == 0 { m |= 1 << N; }
                if y + 1 == n { m |= 1 << S; }
                self.open[i] = m;
            }
        }
    }

    /// Hold every fixed lake cell at its lake level, booking the exchange.
    fn pin_lakes(&mut self, w: &mut Water) {
        for i in 0..self.c.len() {
            if !self.fixed(i) {
                continue;
            }
            let target = (self.lake[i] - self.c[i].z).max(0.0);
            let change = target - self.c[i].h;
            if change >= 0.0 {
                w.books.sources += change;
                w.added_volume += change;
            } else {
                w.books.edges -= change;
                w.lost_volume -= change;
            }
            self.c[i].h = target;
        }
    }

    fn rebuild_active(&mut self, w: &Water) {
        self.active.clear();
        self.added.clear();
        self.in_active.iter_mut().for_each(|a| *a = false);
        for &i in &w.source_cells {
            self.in_active[i] = true;
        }
        for i in 0..self.c.len() {
            if self.c[i].h > 0.0 || self.fixed(i) {
                self.in_active[i] = true;
            }
            if self.in_active[i] {
                self.active.push(i as u32);
            }
        }
    }

    pub(super) fn prepare(&mut self, w: &mut Water, sync: bool) {
        let (terrain, dam) = w.bed_version.unwrap();
        let version = (terrain, dam, w.closed_edge.version(), w.lake_level.version(), w.config.open_edges);
        let rebuilt = self.version != Some(version);
        if rebuilt {
            self.clear_pools();
            self.rebuild(w);
            self.version = Some(version);
        }
        if sync {
            for (c, &v) in self.c.iter_mut().zip(&w.h) {
                c.h = v.max(0.0) as f64;
            }
            for (l, &v) in self.last.iter_mut().zip(&w.h) {
                *l = v.max(0.0);
            }
        }
        if rebuilt || sync {
            self.pin_lakes(w);
            self.rebuild_active(w);
            self.published = (0..self.c.len() as u32).collect();
        }
    }

    fn clear_pools(&mut self) {
        for &m in &self.members {
            self.pool_of[m as usize] = NONE;
        }
        self.members.clear();
        self.rim.clear();
        self.pools.clear();
    }

    /// Group deep water standing in hollows into pools of one level.
    fn build_pools(&mut self) {
        self.clear_pools();
        let joins = |s: &Self, i: usize| !s.fixed(i) && s.c[i].h > H_POOL;
        for k in 0..self.active.len() + self.added.len() {
            let seed = if k < self.active.len() { self.active[k] } else { self.added[k - self.active.len()] } as usize;
            if self.pool_of[seed] != NONE || !joins(self, seed) {
                continue;
            }
            let id = self.pools.len() as u32;
            let start = self.members.len();
            self.pool_of[seed] = id;
            self.members.push(seed as u32);
            let mut q = start;
            let mut volume = 0.0;
            while q < self.members.len() {
                let i = self.members[q] as usize;
                q += 1;
                volume += self.c[i].h;
                for dir in [W, E, N, S] {
                    if let Some(j) = self.neighbour(i, dir) {
                        if self.pool_of[j] == NONE && joins(self, j) && (self.c[i].h + self.c[i].z - self.c[j].h - self.c[j].z).abs() < POOL_STEP + POOL_REL * self.c[i].h.min(self.c[j].h) {
                            self.pool_of[j] = id;
                            self.members.push(j as u32);
                        }
                    }
                }
            }
            let len = self.members.len() - start;
            if len < 2 {
                self.pool_of[seed] = NONE;
                self.members.truncate(start);
                continue;
            }
            let rim_start = self.rim.len();
            for k in start..self.members.len() {
                let m = self.members[k] as usize;
                if self.open[m] != 0 || [W, E, N, S].iter().any(|&d| self.neighbour(m, d).is_none_or(|j| self.pool_of[j] != id)) {
                    self.rim.push(m as u32);
                }
            }
            let rim_len = self.rim.len() - rim_start;
            self.pools.push(Pool { start, len, volume, level: 0.0, dirty: true, visits: 0, outlet: false, rim_start, rim_len, area: 0.0 });
            self.level(id as usize);
            let level = self.pools[id as usize].level;
            let outlet = self.rim[rim_start..].iter().any(|&m| {
                let m = m as usize;
                self.open[m] != 0 || [W, E, N, S].iter().any(|&d| self.neighbour(m, d).is_some_and(|j| self.pool_of[j] != id && self.surface(j) < level - EPS_DROP))
            });
            self.pools[id as usize].outlet = outlet;
        }
    }

    /// Spread a pool's volume over its members at one level.
    fn level(&mut self, p: usize) {
        let pool = self.pools[p];
        let cells = &self.members[pool.start..pool.start + pool.len];
        let (mut sum_z, mut count) = (0.0, 0.0);
        for &m in cells {
            sum_z += self.c[m as usize].z;
            count += 1.0;
        }
        let mut l = (pool.volume + sum_z) / count;
        let mut settled = false;
        for _ in 0..8 {
            let (mut c, mut sz) = (0.0, 0.0);
            for &m in cells {
                let z = self.c[m as usize].z;
                if z < l {
                    c += 1.0;
                    sz += z;
                }
            }
            let next = (pool.volume + sz) / c;
            if (next - l).abs() < 1.0e-12 {
                l = next;
                settled = true;
                break;
            }
            l = next;
        }
        if !settled {
            // Shoreline still moving: fill the sorted beds exactly.
            let mut beds: Vec<f64> = cells.iter().map(|&m| self.c[m as usize].z).collect();
            beds.sort_unstable_by(f64::total_cmp);
            l = fill_level(&beds, pool.volume.max(0.0));
        }
        let mut spread = 0.0;
        let mut area = 0.0;
        let mut deepest = cells[0] as usize;
        for &m in cells {
            let m = m as usize;
            self.c[m].h = (l - self.c[m].z).max(0.0);
            spread += self.c[m].h;
            if self.c[m].h > 0.0 {
                area += 1.0;
            }
            if self.c[m].z < self.c[deepest].z {
                deepest = m;
            }
        }
        // Keep the books exact: the rounding residual goes to the deepest member.
        self.c[deepest].h = (self.c[deepest].h + pool.volume - spread).max(0.0);
        self.pools[p].level = l;
        self.pools[p].area = area;
        self.pools[p].dirty = false;
    }

    /// Whether cell j could pass water on this tick (an outlet at or below it).
    fn drains(&self, j: usize, from: usize) -> bool {
        if self.open[j] != 0 {
            return true;
        }
        let sj = self.c[j].z + self.c[j].h;
        for dir in [W, E, N, S] {
            if let Some(k) = self.neighbour(j, dir) {
                if k != from && !self.exhausted(k) && self.surface(k) <= sj {
                    return true;
                }
            }
        }
        false
    }

    fn could_send(&self, i: usize) -> bool {
        let si = self.surface(i);
        if !self.fixed(i) {
            if self.c[i].h <= H_STILL {
                return false;
            }
            if self.open[i] != 0 {
                return true;
            }
        }
        let eps = if self.fixed(i) { EPS_LAKE } else { EPS_DROP };
        for dir in [W, E, N, S] {
            if let Some(j) = self.neighbour(i, dir) {
                if self.surface(j) < si - eps && (self.pool_of[i] == NONE || self.pool_of[j] != self.pool_of[i]) {
                    return true;
                }
            }
        }
        false
    }

    /// Face of cell i on side dir: (horizontal, index, +1 if leaving i through it runs along +x/+y).
    #[inline]
    fn face(&self, i: usize, dir: u8) -> (bool, usize, f64) {
        let n = self.n;
        let (x, y) = (i % n, i / n);
        match dir {
            W => (true, y * (n + 1) + x, -1.0),
            E => (true, y * (n + 1) + x + 1, 1.0),
            N => (false, y * n + x, -1.0),
            _ => (false, (y + 1) * n + x, 1.0),
        }
    }

    /// Whether water crossed face (i, dir) into i this tick.
    #[inline]
    fn fed_by(&self, i: usize, dir: u8) -> bool {
        let (horizontal, face, sign) = self.face(i, dir);
        // Into i is the opposite direction to leaving it.
        let slot = (if horizontal { 0 } else { 2 }) + if sign > 0.0 { 1 } else { 0 };
        self.crossed[slot][face] == self.tick
    }

    /// Whether cell j (or its pool) has used up its sweeps this tick.
    #[inline]
    fn exhausted(&self, j: usize) -> bool {
        let p = self.pool_of[j];
        if p != NONE { self.pools[p as usize].visits >= MAX_POOL_VISITS } else { self.stamp[j] == self.tick && self.visits[j] >= MAX_VISITS }
    }

    fn flux(&mut self, i: usize, dir: u8, t: f64) {
        let (horizontal, face, sign) = self.face(i, dir);
        let slot = (if horizontal { 0 } else { 2 }) + if sign > 0.0 { 0 } else { 1 };
        self.crossed[slot][face] = self.tick;
        let (acc, list) = if horizontal { (&mut self.fx, &mut self.fx_list) } else { (&mut self.fy, &mut self.fy_list) };
        if acc[face] == 0.0 {
            list.push(face as u32);
        }
        acc[face] += sign * t;
        if acc[face] == 0.0 {
            acc[face] = f64::MIN_POSITIVE;
        }
    }

    /// Faces from cell i (level si) to lower neighbours outside its own pool.
    fn gather(&mut self, i: usize, si: f64, fixed: bool) {
        let zi = self.c[i].z;
        let pool = self.pool_of[i];
        for dir in [W, E, N, S] {
            if let Some(j) = self.neighbour(i, dir) {
                if (fixed && self.fixed(j)) || (pool != NONE && self.pool_of[j] == pool) {
                    continue;
                }
                let sj = self.surface(j);
                let zf = zi.max(self.c[j].z);
                let thr = sj.max(zf);
                if si > thr + if fixed { EPS_LAKE } else { EPS_DROP } {
                    let sink = self.fixed(j);
                    let pj = self.pool_of[j];
                    let cap = fixed
                        || (!sink
                            && (self.fed_by(i, dir)
                                || self.exhausted(j)
                                || if pj != NONE { !self.pools[pj as usize].outlet } else { !self.drains(j, i) }));
                    let area = if pj != NONE { self.pools[pj as usize].len as f64 } else { 1.0 };
                    self.cands.push(Cand { from: i, j, dir, s: sj, zf, thr, cap, area, sink, t: 0.0 });
                }
            } else if self.open[i] & (1 << dir) != 0 && !fixed {
                let g = self.ghost(i, dir);
                self.cands.push(Cand { from: i, j: usize::MAX, dir, s: g, zf: zi, thr: zi, cap: false, area: 1.0, sink: true, t: 0.0 });
            }
        }
    }

    /// Faces into one receiving pool share its rise allowance.
    fn share_pools(&mut self) {
        for a in 0..self.cands.len() {
            let j = self.cands[a].j;
            if j == usize::MAX || self.pool_of[j] == NONE {
                continue;
            }
            let p = self.pool_of[j];
            let count = self.cands.iter().filter(|c| c.j != usize::MAX && self.pool_of[c.j] == p).count();
            self.cands[a].area = self.pools[p as usize].len as f64 / count as f64;
        }
    }

    /// Stay together: water spreads onto dry ground only down its steepest face, or where it
    /// would stand at least H_SPREAD deep. Keeps creeks one channel and stops thin sheets.
    fn keep_together(&mut self, level: f64) {
        let dry = |s: &Self, c: &Cand| c.j != usize::MAX && !s.fixed(c.j) && s.c[c.j].h < H_WET;
        let mut best = usize::MAX;
        for (m, c) in self.cands.iter().enumerate() {
            if dry(self, c) && (best == usize::MAX || c.s < self.cands[best].s) {
                best = m;
            }
        }
        let mut kept = 0;
        for m in 0..self.cands.len() {
            let c = self.cands[m];
            if m == best || !dry(self, &c) || level - self.c[c.j].z >= H_SPREAD {
                self.cands[kept] = c;
                kept += 1;
            }
        }
        self.cands.truncate(kept);
    }

    /// Hand the transfers in `cands` to their receivers.
    fn deliver(&mut self, w: &mut Water) {
        let cands = std::mem::take(&mut self.cands);
        for c in &cands {
            if c.t <= 0.0 {
                continue;
            }
            self.flux(c.from, c.dir, c.t);
            if c.sink {
                w.books.edges += c.t;
                w.lost_volume += c.t;
                continue;
            }
            let j = c.j;
            self.c[j].h += c.t;
            if !self.in_active[j] {
                self.in_active[j] = true;
                self.added.push(j as u32);
            }
            let p = self.pool_of[j];
            if p != NONE {
                let pool = &mut self.pools[p as usize];
                pool.volume += c.t;
                pool.dirty = true;
                // Rough new level until it is levelled: keeps the sweep order by surface.
                pool.level += c.t / pool.len as f64;
                if pool.visits == 0 || (pool.visits < MAX_POOL_VISITS && c.t > REVISIT_MIN * pool.len as f64) {
                    self.heap.push((key(pool.level), j as u32));
                }
            } else if self.stamp[j] != self.tick || (self.visits[j] < MAX_VISITS && c.t > REVISIT_MIN) {
                self.heap.push((key(self.c[j].z + self.c[j].h), j as u32));
            }
        }
        self.cands = cands;
        self.cands.clear();
    }

    fn step_cell(&mut self, w: &mut Water, i: usize, dt: f64) {
        let fixed = self.fixed(i);
        let si = self.surface(i);
        let hi = self.c[i].h;
        if !fixed && hi <= H_STILL {
            return;
        }
        self.cands.clear();
        self.gather(i, si, fixed);
        if self.cands.is_empty() {
            return;
        }
        self.share_pools();
        self.keep_together(si);
        let coef = dt / self.dx;
        let inv_n = 1.0 / w.config.manning as f64;
        let inv_dx = 1.0 / self.dx;
        if fixed {
            let total = eval(&mut self.cands, si, coef, inv_n, inv_dx).0;
            w.books.sources += total;
            w.added_volume += total;
        } else {
            let zi = self.c[i].z;
            let mut total = solve(&mut self.cands, si, zi, 1.0, zi + self.last[i] as f64, coef, inv_n, inv_dx);
            if total > hi {
                let scale = hi / total;
                self.cands.iter_mut().for_each(|c| c.t *= scale);
                total = hi;
            }
            self.c[i].h = (hi - total).max(0.0);
            self.last[i] = self.c[i].h as f32;
        }
        self.deliver(w);
    }

    fn step_pool(&mut self, w: &mut Water, p: usize, dt: f64) {
        self.pools[p].visits += 1;
        if self.pools[p].dirty {
            self.level(p);
        }
        let pool = self.pools[p];
        self.cands.clear();
        let area = pool.area;
        for k in pool.rim_start..pool.rim_start + pool.rim_len {
            let m = self.rim[k] as usize;
            self.gather(m, pool.level, false);
        }
        if self.cands.is_empty() {
            return;
        }
        self.share_pools();
        self.keep_together(pool.level);
        let coef = dt / self.dx;
        let inv_n = 1.0 / w.config.manning as f64;
        let inv_dx = 1.0 / self.dx;
        let floor = pool.level - pool.volume / area.max(1.0);
        let mut total = solve(&mut self.cands, pool.level, floor, area.max(1.0), pool.level, coef, inv_n, inv_dx);
        if total > pool.volume {
            let scale = pool.volume / total;
            self.cands.iter_mut().for_each(|c| c.t *= scale);
            total = pool.volume;
        }
        if total <= 0.0 {
            return;
        }
        // Take the water from the sending members, then let the pool settle to one level.
        for c in &self.cands {
            self.c[c.from].h -= c.t;
        }
        self.pools[p].volume -= total;
        self.deliver(w);
        self.level(p);
    }

    /// Sweep the queued cells and pools from the highest surface down.
    fn sweep(&mut self, w: &mut Water, dt: f64) -> u64 {
        let mut updates = 0;
        while let Some((k, i)) = self.heap.pop() {
            let i = i as usize;
            let p = self.pool_of[i];
            if p != NONE {
                let pool = &self.pools[p as usize];
                // Skip used-up pools and entries superseded by a later, higher one.
                if pool.visits >= MAX_POOL_VISITS || (pool.visits > 0 && k != key(pool.level)) {
                    continue;
                }
                updates += pool.len as u64;
                self.step_pool(w, p as usize, dt);
                continue;
            }
            if self.stamp[i] != self.tick {
                self.stamp[i] = self.tick;
                self.visits[i] = 0;
            } else if self.visits[i] >= MAX_VISITS || k != key(self.surface(i)) {
                continue;
            }
            self.visits[i] += 1;
            updates += 1;
            self.step_cell(w, i, dt);
        }
        updates
    }

    /// Spread water standing well above every neighbour over the hollow it sits in, filling up to
    /// the rim; anything above the rim is handed to the outlet cell and queued. Returns whether
    /// anything was queued.
    fn settle(&mut self, w: &mut Water) -> bool {
        let mut towers: Vec<(u64, u32)> = Vec::new();
        for k in 0..self.active.len() + self.added.len() {
            let i = if k < self.active.len() { self.active[k] } else { self.added[k - self.active.len()] } as usize;
            if self.fixed(i) || self.c[i].h <= SETTLE_STEP || self.open[i] != 0 {
                continue;
            }
            if self.surface(i) - self.lowest_neighbour(i) > SETTLE_STEP {
                towers.push((key(self.surface(i)), i as u32));
            }
        }
        towers.sort_unstable_by(|a, b| b.cmp(a));
        let mut queued = false;
        for &(_, i) in &towers {
            let i = i as usize;
            if self.surface(i) - self.lowest_neighbour(i) > SETTLE_STEP {
                queued |= self.fill(w, i);
            }
        }
        queued
    }

    fn lowest_neighbour(&self, i: usize) -> f64 {
        [W, E, N, S].iter().filter_map(|&d| self.neighbour(i, d)).map(|j| self.surface(j)).fold(f64::INFINITY, f64::min)
    }

    fn mark_frontier(&mut self, frontier: &mut BinaryHeap<std::cmp::Reverse<(u64, u32)>>, c: usize, id: u32) {
        for d in [W, E, N, S] {
            if let Some(j) = self.neighbour(c, d) {
                if self.mark[j] != id {
                    self.mark[j] = id;
                    frontier.push(std::cmp::Reverse((key(self.surface(j)), j as u32)));
                }
            }
        }
    }

    /// Flood-fill from cell i: grow the region over its lowest neighbour until the level rests
    /// below every neighbour, or a neighbour that drains elsewhere takes the overflow.
    fn fill(&mut self, w: &mut Water, i: usize) -> bool {
        self.mark_id = self.mark_id.wrapping_add(1);
        if self.mark_id == 0 {
            self.mark.iter_mut().for_each(|m| *m = 0);
            self.mark_id = 1;
        }
        let id = self.mark_id;
        let mut region = vec![i];
        let mut beds = vec![self.c[i].z];
        let mut volume = self.c[i].h;
        let mut frontier = BinaryHeap::new();
        self.mark[i] = id;
        self.mark_frontier(&mut frontier, i, id);
        let mut level = fill_level(&beds, volume);
        let mut spill: Option<(usize, f64)> = None;
        while let Some(std::cmp::Reverse((_, j))) = frontier.pop() {
            let j = j as usize;
            let sj = self.surface(j);
            if sj >= level - EPS_DROP {
                break;
            }
            let outlet = self.fixed(j)
                || self.open[j] != 0
                || [W, E, N, S].iter().any(|&d| self.neighbour(j, d).is_some_and(|k| self.mark[k] != id && self.surface(k) < sj - EPS_DROP));
            if outlet || region.len() >= SETTLE_MAX {
                spill = Some((j, sj));
                break;
            }
            region.push(j);
            volume += self.c[j].h;
            let pos = beds.partition_point(|&b| b < self.c[j].z);
            beds.insert(pos, self.c[j].z);
            level = fill_level(&beds, volume);
            self.mark_frontier(&mut frontier, j, id);
        }
        let mut target = level;
        let mut excess = 0.0;
        if let Some((_, sj)) = spill {
            target = sj.max(beds[0]).min(level);
            let held: f64 = beds.iter().map(|&b| (target - b).max(0.0)).sum();
            excess = (volume - held).max(0.0);
        }
        // Spread the region at the target level; the rounding residual stays with the deepest cell.
        let mut spread = 0.0;
        let mut deepest = region[0];
        for &r in &region {
            self.c[r].h = (target - self.c[r].z).max(0.0);
            spread += self.c[r].h;
            if self.c[r].z < self.c[deepest].z {
                deepest = r;
            }
            if self.c[r].h > 0.0 && !self.in_active[r] {
                self.in_active[r] = true;
                self.added.push(r as u32);
            }
        }
        let rest = volume - spread - excess;
        self.c[deepest].h = (self.c[deepest].h + rest).max(0.0);
        if excess <= 0.0 {
            return false;
        }
        let (j, _) = spill.unwrap();
        if self.fixed(j) {
            w.books.edges += excess;
            w.lost_volume += excess;
            return false;
        }
        self.c[j].h += excess;
        if !self.in_active[j] {
            self.in_active[j] = true;
            self.added.push(j as u32);
        }
        if self.exhausted(j) {
            self.visits[j] = MAX_VISITS - 1;
        }
        self.heap.push((key(self.surface(j)), j as u32));
        true
    }

    fn tick(&mut self, w: &mut Water, dt: f64) {
        self.tick = self.tick.wrapping_add(1);
        if self.tick == 0 {
            self.stamp.iter_mut().for_each(|s| *s = 0);
            self.tick = 1;
        }
        for &i in &w.source_cells {
            let add = w.src[i] as f64 * dt;
            if add <= 0.0 {
                continue;
            }
            w.books.sources += add;
            w.added_volume += add;
            if self.fixed(i) {
                w.books.edges += add;
                w.lost_volume += add;
                continue;
            }
            self.c[i].h += add;
            if !self.in_active[i] {
                self.in_active[i] = true;
                self.added.push(i as u32);
            }
        }
        self.build_pools();
        self.heap.clear();
        for k in 0..self.active.len() + self.added.len() {
            let i = if k < self.active.len() { self.active[k] } else { self.added[k - self.active.len()] } as usize;
            if self.could_send(i) {
                let p = self.pool_of[i];
                let s = if p != NONE { self.pools[p as usize].level } else { self.surface(i) };
                self.heap.push((key(s), i as u32));
            }
        }
        let mut updates = self.sweep(w, dt);
        for p in 0..self.pools.len() {
            if self.pools[p].dirty {
                self.level(p);
            }
        }
        // Water that could not move on this tick (it reached hollows after their turn) settles
        // where it would come to rest, spilling the rest on towards the next outlet.
        self.clear_pools();
        for _ in 0..SETTLE_ROUNDS {
            if !self.settle(w) {
                break;
            }
            updates += self.sweep(w, dt);
        }
        w.timings.cell_updates += updates;
        self.compact(w);
    }

    /// Drop dried cells from the active list and merge in the newly wetted ones, in index order.
    fn compact(&mut self, w: &Water) {
        let keep = |s: &Self, i: usize| s.c[i].h > 0.0 || s.fixed(i) || w.src[i] > 0.0;
        let dried = self.active.iter().any(|&i| !keep(self, i as usize));
        if !dried && self.added.is_empty() {
            return;
        }
        let mut added = std::mem::take(&mut self.added);
        added.sort_unstable();
        let old = std::mem::take(&mut self.active);
        let mut out = Vec::with_capacity(old.len() + added.len());
        let (mut a, mut b) = (0, 0);
        while a < old.len() || b < added.len() {
            let i = if b >= added.len() || (a < old.len() && old[a] < added[b]) {
                a += 1;
                old[a - 1]
            } else {
                b += 1;
                added[b - 1]
            };
            if keep(self, i as usize) {
                out.push(i);
            } else {
                self.in_active[i as usize] = false;
            }
        }
        self.active = out;
        added.clear();
        self.added = added;
    }

    pub(super) fn advance(&mut self, w: &mut Water, seconds: f64) {
        if seconds <= 0.0 {
            return;
        }
        // Ticks have a fixed length; shorter calls are carried over to the next one.
        self.pending += seconds;
        let ticks = ((self.pending + 0.01) / TICK_S).floor() as usize;
        if ticks == 0 {
            return;
        }
        let seconds = std::mem::take(&mut self.pending);
        let dt = seconds / ticks as f64;
        for _ in 0..ticks {
            self.tick(w, dt);
            w.steps_taken += 1;
            w.timings.substeps += 1;
        }
        self.clear_pools();
        w.physics_seconds += seconds;
        w.timings.physics_s += seconds;
        self.flux_s += seconds;
    }

    pub(super) fn environment(&mut self, w: &mut Water, days: f64) {
        w.update_riverbed(days as f32);
        let forcing = w.forcing();
        let r = forcing.rain as f64 * days;
        let e = w.config.evaporation as f64 * forcing.evaporation as f64 * days;
        let s = w.config.seepage as f64 * days;
        let (mut rain, mut evaporation, mut seepage) = (0.0, 0.0, 0.0);
        for k in 0..self.active.len() {
            let i = self.active[k] as usize;
            let depth = self.c[i].h;
            if self.fixed(i) || depth <= 0.0 {
                continue;
            }
            let ev = e.min(depth + r);
            let infiltration = if w.riverbed.as_slice()[i] < 0.5 && !w.canal.as_slice()[i] { 4.0 * s } else { s };
            let mut sp = infiltration.min((depth + r - ev).max(0.0));
            let mut left = depth + r - ev - sp;
            if left < H_TRACE {
                sp += left.max(0.0);
                left = 0.0;
            }
            self.c[i].h = left;
            rain += r;
            evaporation += ev;
            seepage += sp;
        }
        w.books.rain += rain;
        w.books.evaporation += evaporation;
        w.books.seepage += seepage;
        w.added_volume += rain;
        w.lost_volume += evaporation + seepage;
        self.compact(w);
    }

    pub(super) fn publish(&mut self, w: &mut Water) {
        let n = self.n;
        for &i in &self.published {
            w.h[i as usize] = 0.0;
        }
        self.published.clear();
        for row in &mut w.rows {
            row.clear();
        }
        for &i in &self.active {
            let iu = i as usize;
            let h = self.c[iu].h as f32;
            if h <= 0.0 {
                continue;
            }
            w.h[iu] = h;
            self.published.push(i);
            let (x, y) = ((iu % n) as u32, iu / n);
            let row = &mut w.rows[y];
            match row.last_mut() {
                Some(last) if last.1 == x => last.1 = x + 1,
                _ => row.push((x, x + 1)),
            }
        }
        if self.flux_s > 0.0 {
            w.qx.fill(0.0);
            w.qy.fill(0.0);
            let scale = self.dx / self.flux_s;
            for &f in &self.fx_list {
                w.qx[f as usize] = (self.fx[f as usize] * scale) as f32;
                self.fx[f as usize] = 0.0;
            }
            for &f in &self.fy_list {
                w.qy[f as usize] = (self.fy[f as usize] * scale) as f32;
                self.fy[f as usize] = 0.0;
            }
            self.fx_list.clear();
            self.fy_list.clear();
            self.flux_s = 0.0;
        }
    }

    pub(super) fn total_volume(&self) -> Option<f64> {
        self.version.map(|_| self.active.iter().chain(&self.added).map(|&i| self.c[i as usize].h).sum::<f64>())
    }
}

impl super::WaterEngine for Automaton {
    fn prepare(&mut self, w: &mut Water, sync: bool) { Automaton::prepare(self, w, sync) }
    fn advance(&mut self, w: &mut Water, seconds: f64) { Automaton::advance(self, w, seconds) }
    fn environment(&mut self, w: &mut Water, days: f64) { Automaton::environment(self, w, days) }
    fn publish(&mut self, w: &mut Water) { Automaton::publish(self, w) }
    fn total_volume(&self) -> Option<f64> { Automaton::total_volume(self) }
}
