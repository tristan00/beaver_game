//! Equilibrium water: a fill–spill–merge depression hierarchy with steady reach discharge
//! (after Barnes, Callaghan & Wickert 2021, "Computing water flow through complex
//! landscapes, Part 3: Fill–Spill–Merge").
//!
//! On every bed change the map is rebuilt: a priority flood from the outlets gives the
//! drainage tree of the filled surface (each cell drains to the neighbour that flooded it,
//! which is also how the map generator laid out its rivers), and steepest descent on the
//! raw bed labels every pit's watershed. Joining watersheds at their lowest saddles builds
//! the depression hierarchy: leaves are pits, a meta-depression appears where two of them
//! meet, and a root depression is one that spills to an outlet.
//!
//! Each step pushes the source discharge down the drainage graph in topological order.
//! A reach cell holds the channel storage of Manning normal flow for its discharge, so a
//! rising flow fills the channel first and its front moves at the water speed. Water that
//! reaches a depression fills it; a full depression passes its excess to its sibling or,
//! when both are full, to the merged parent. Roots spill over their crest by the weir law.
use super::{Water, edge_bed, neighbours8};
use std::collections::BinaryHeap;

/// Longest routing step in seconds.
const MAX_STEP_S: f64 = 900.0;
/// Gradient the priority flood lays across flats (the map generator's value).
const FLOW_EPS: f32 = 1.0e-3;
/// Channel width per square root of discharge, w = a·Q^0.5 (m per (m³/s)^0.5).
const WIDTH_A: f64 = 3.5;
/// Narrowest hydraulic channel (m); it is drawn at least one cell wide.
const WIDTH_MIN: f64 = 0.1;
/// Widest channel (m); a channel wider than a cell is spread sideways at one stage.
const WIDTH_MAX: f64 = 4.0;
/// Floor on the friction slope of reaches on the filled surface.
const SLOPE_MIN: f64 = 1.0e-3;
/// Cap on the friction slope (lidar steps are not waterfalls).
const SLOPE_MAX: f64 = 0.15;
/// Cells downstream over which the reach slope is averaged.
const SLOPE_CELLS: usize = 6;
/// Roughness multiplier on the configured Manning n for shallow creeks.
const ROUGHNESS: f64 = 1.75;
/// Rim cells within this of the spill level form the weir crest (m).
const RIM_TOL: f32 = 0.02;
/// Widest weir a spill concentrates into (m); overflow finds the low notch.
pub(crate) const WEIR_MAX_M: usize = 2;
/// Depth above which stranded water is held as a draining pool after a rebuild (m).
const POOL_MIN: f32 = 0.08;
/// Breach width of a stranded pool's outlet weir (m).
const POOL_WEIR_M: f64 = 0.1;
/// A pool whose head over its outlet falls below this dissolves into the reach (m).
const POOL_END: f64 = 0.01;
/// Reach storage below this is dry (m³).
const DRY_M3: f64 = 1.0e-7;

/// Overflows one depression may pass on in a step before it holds the rest (cycle guard).
const LOOP_LIMIT: u32 = 16;

const NONE: u32 = u32::MAX;
const OCEAN: u32 = u32::MAX - 1;

type Version = ((u64, u64), (u64, u64), (u64, u64), (u64, u64), bool);

#[derive(Clone, Debug, Default)]
pub(super) struct Equilibrium {
    version: Option<Version>,
    n: usize,
    zbed: Vec<f32>,
    // Static, per bed.
    recv: Vec<u32>,
    key: Vec<u64>,
    slope: Vec<f32>,
    own_of: Vec<u32>,
    pit_leaf: Vec<u32>,
    outlet: Vec<u8>,
    lake_cells: Vec<u32>,
    deps: Vec<Dep>,
    dcells: Vec<u32>,
    dbeds: Vec<f64>,
    dprefix: Vec<f64>,
    // Dynamic.
    own: Vec<f64>,
    slow: Vec<bool>,
    full: Vec<bool>,
    lvl: Vec<f64>,
    dirty: Vec<bool>,
    wet_deps: Vec<u32>,
    wet_flag: Vec<bool>,
    store: Vec<f64>,
    wet: Vec<u32>,
    pools: Vec<Pool>,
    pool_of: Vec<u32>,
    carry: Vec<(u32, f64)>,
    // Per step.
    heap: BinaryHeap<(u64, u32)>,
    pend: Vec<f64>,
    inq: Vec<bool>,
    vstamp: Vec<u64>,
    s0: Vec<f64>,
    iacc: Vec<f64>,
    oacc: Vec<f64>,
    touched: Vec<u32>,
    dstamp: Vec<u64>,
    dfwd: Vec<f64>,
    dcnt: Vec<u32>,
    crests: Vec<(u32, u32, f64)>,
    dinq: Vec<bool>,
    lake_roots: Vec<u32>,
    stamp: u64,
    dt: f64,
    last_dt: f64,
    fresh: bool,
    // Publishing.
    written: Vec<u32>,
    wmark: Vec<bool>,
    fx: Vec<f32>,
    fy: Vec<f32>,
    cx: Vec<u16>,
    cy: Vec<u16>,
    faces_x: Vec<u32>,
    faces_y: Vec<u32>,
    prev_x: Vec<u32>,
    prev_y: Vec<u32>,
}

#[derive(Clone, Debug)]
struct Dep {
    parent: u32,
    kids: [u32; 2],
    spill: f64,
    base: f64,
    desc: f64,
    start: u32,
    end: u32,
    own_cap: f64,
    crest: u32,
    target: u32,
    root: u32,
    zmin: f32,
    // Root only.
    weir: f64,
    rim: Vec<u32>,
    lake: f64,
}

#[derive(Clone, Debug)]
struct Pool {
    cells: Vec<u32>,
    beds: Vec<f64>,
    prefix: Vec<f64>,
    vol: f64,
    exit: u32,
    crest: f64,
    key: u64,
    fwd: f64,
    stamp: u64,
    alive: bool,
}

fn ord(v: f32) -> u32 {
    let b = v.to_bits();
    if b & 0x8000_0000 == 0 { b ^ 0x8000_0000 } else { !b }
}

fn hkey(v: f32, sec: u32) -> u64 {
    ((ord(v) as u64) << 32) | sec as u64
}

/// Level for `v` of water over `c` cells at `base` plus sorted `beds` (with prefix sums).
fn level_on(v: f64, c: f64, base: f64, beds: &[f64], prefix: &[f64]) -> f64 {
    let k_max = beds.len();
    let at = |k: usize| (v + c * base + prefix[k]) / (c + k as f64);
    if c > 0.0 {
        let l = base + v / c;
        if k_max == 0 || l <= beds[0] {
            return l;
        }
    } else if k_max == 0 {
        return f64::NEG_INFINITY;
    }
    let (mut lo, mut hi) = (1, k_max);
    while lo < hi {
        let k = (lo + hi) / 2;
        if at(k) > beds[k] { lo = k + 1 } else { hi = k }
    }
    at(lo)
}

fn volume_on(l: f64, c: f64, base: f64, beds: &[f64], prefix: &[f64]) -> f64 {
    let k = beds.partition_point(|&b| b < l);
    c * (l - base).max(0.0) + l * k as f64 - prefix[k]
}

impl Equilibrium {
    fn total(&self) -> f64 {
        self.own.iter().sum::<f64>()
            + self.store.iter().sum::<f64>()
            + self.pools.iter().filter(|p| p.alive).map(|p| p.vol).sum::<f64>()
            + self.carry.iter().map(|c| c.1).sum::<f64>()
    }

    fn beds(&self, d: usize) -> (&[f64], &[f64]) {
        let (s, e) = (self.deps[d].start as usize, self.deps[d].end as usize);
        (&self.dbeds[s..e], &self.dprefix[s + d..e + d + 1])
    }

    fn level_for(&self, d: usize, v: f64) -> f64 {
        let (beds, prefix) = self.beds(d);
        level_on(v, self.deps[d].desc, self.deps[d].base, beds, prefix)
    }

    fn level(&mut self, d: usize) -> f64 {
        if self.dirty[d] {
            self.lvl[d] = self.level_for(d, self.own[d]);
            self.dirty[d] = false;
        }
        self.lvl[d]
    }

    fn area(&self, d: usize) -> f64 {
        self.deps[d].desc + (self.deps[d].end - self.deps[d].start) as f64
    }

    /// Whether `p` holds one merged lake over its children.
    fn merged(&self, p: usize) -> bool {
        self.own[p] > 0.0 || (self.full[p] && self.deps[p].own_cap <= 0.0)
    }

    /// The depression whose level is the water surface over `d`.
    fn top(&self, mut d: usize) -> usize {
        loop {
            let p = self.deps[d].parent;
            if p == NONE || !self.merged(p as usize) {
                return d;
            }
            d = p as usize;
        }
    }

    /// Every lake with its own surface.
    fn tops(&self) -> Vec<usize> {
        let mut t: Vec<usize> = self.wet_deps.iter().filter(|&&d| self.own[d as usize] > 0.0).map(|&d| self.top(d as usize)).collect();
        t.sort_unstable();
        t.dedup();
        t
    }

    fn set_own(&mut self, d: usize, v: f64) {
        self.own[d] = v;
        self.dirty[d] = true;
        if v > 0.0 && !self.wet_flag[d] {
            self.wet_flag[d] = true;
            self.wet_deps.push(d as u32);
        }
        self.refresh_full(d);
    }

    fn refresh_full(&mut self, mut d: usize) {
        loop {
            let dep = &self.deps[d];
            let mut f = self.own[d] >= dep.own_cap - 1.0e-9 * (1.0 + dep.own_cap);
            if dep.kids[0] != NONE {
                f = f && self.full[dep.kids[0] as usize] && self.full[dep.kids[1] as usize];
            }
            if f == self.full[d] {
                return;
            }
            self.full[d] = f;
            if dep.parent == NONE {
                return;
            }
            d = dep.parent as usize;
        }
    }

    fn add(&mut self, c: u32, rate: f64) {
        debug_assert!(self.dt > 0.0);
        self.pend[c as usize] += rate;
        if !self.inq[c as usize] {
            self.inq[c as usize] = true;
            self.heap.push((self.key[c as usize], c));
        }
    }

    fn dtouch(&mut self, d: usize) {
        if self.dstamp[d] != self.stamp {
            self.dstamp[d] = self.stamp;
            self.dfwd[d] = 0.0;
            self.dcnt[d] = 0;
        }
    }

    fn schedule_dep(&mut self, d: usize) {
        if self.dt <= 0.0 {
            return;
        }
        let nn = (self.n * self.n) as u32;
        self.dtouch(d);
        if !self.dinq[d] {
            self.dinq[d] = true;
            let k = hkey(self.deps[d].zmin, 0);
            self.heap.push((k, nn + d as u32));
        }
    }

    /// Add water to depression `d`, spilling a full non-root depression's excess.
    fn deposit(&mut self, mut d: usize, mut vol: f64) {
        let kids = self.deps[d].kids;
        if kids[0] != NONE && !(self.full[kids[0] as usize] && self.full[kids[1] as usize]) {
            vol = self.fill(d, vol);
        }
        self.set_own(d, self.own[d] + vol);
        loop {
            let dep = &self.deps[d];
            if dep.parent == NONE {
                self.schedule_dep(d);
                return;
            }
            let cap = dep.own_cap;
            if self.own[d] <= cap {
                return;
            }
            let x = self.own[d] - cap;
            let (m, crest, target) = (dep.parent as usize, dep.crest, dep.target);
            let kids = self.deps[m].kids;
            let s = if kids[0] as usize == d { kids[1] } else { kids[0] } as usize;
            if !self.full[s] && self.dt > 0.0 {
                self.dtouch(d);
                self.dcnt[d] += 1;
                if self.dcnt[d] > LOOP_LIMIT {
                    return;
                }
            }
            self.set_own(d, cap);
            if !self.full[s] {
                if self.dt > 0.0 {
                    let rate = x / self.dt;
                    self.crests.push((crest, target, rate));
                    self.add(target, rate);
                } else {
                    self.carry.push((target, x));
                }
                return;
            }
            self.set_own(m, self.own[m] + x);
            d = m;
        }
    }

    /// Take up to `amount` from depression `d`, from its children when it runs dry.
    fn drain(&mut self, d: usize, amount: f64) -> f64 {
        let mut got = 0.0;
        let mut stack = vec![(d, amount)];
        while let Some((d, want)) = stack.pop() {
            let take = self.own[d].min(want);
            self.set_own(d, self.own[d] - take);
            got += take;
            let rem = want - take;
            let kids = self.deps[d].kids;
            if rem > 1.0e-15 && kids[0] != NONE {
                let (a0, a1) = (self.area(kids[0] as usize), self.area(kids[1] as usize));
                let f = a0 / (a0 + a1).max(1.0e-30);
                stack.push((kids[0] as usize, rem * f));
                stack.push((kids[1] as usize, rem * (1.0 - f)));
            }
        }
        got
    }

    /// Cells under the water of top lake `t` at level `l`.
    fn covered(&self, t: usize, l: f64, out: &mut Vec<u32>) {
        let mut stack = vec![t];
        let mut first = true;
        while let Some(d) = stack.pop() {
            let dep = &self.deps[d];
            let (s, e) = (dep.start as usize, dep.end as usize);
            if first {
                let k = self.dbeds[s..e].partition_point(|&b| b < l);
                out.extend_from_slice(&self.dcells[s..s + k]);
                first = false;
            } else {
                out.extend_from_slice(&self.dcells[s..e]);
            }
            if dep.kids[0] != NONE {
                stack.push(dep.kids[0] as usize);
                stack.push(dep.kids[1] as usize);
            }
        }
    }

    fn width(q: f64) -> f64 {
        (WIDTH_A * q.max(0.0).sqrt()).clamp(WIDTH_MIN, WIDTH_MAX)
    }

    /// Manning normal depth (m) of discharge `q` (m³/s) in a channel of width `wd`.
    fn normal_depth(q: f64, wd: f64, slope: f64, manning: f64) -> f64 {
        if q <= 0.0 {
            return 0.0;
        }
        (q * manning * ROUGHNESS / (wd * slope.sqrt())).powf(0.6)
    }

    pub(super) fn prepare(&mut self, w: &mut Water, sync: bool) {
        let (terrain, dam) = w.bed_version.unwrap();
        let version = (terrain, dam, w.closed_edge.version(), w.lake_level.version(), w.config.open_edges);
        if self.version == Some(version) && !sync {
            return;
        }
        // A new version stamp on an unchanged bed needs no rebuild.
        if let Some(old) = self.version {
            if !sync && old.2 == version.2 && old.3 == version.3 && old.4 == version.4 && self.zbed == w.z {
                self.version = Some(version);
                return;
            }
        }
        let first = self.version.is_none();
        let area = (w.cell_m * w.cell_m) as f64;
        self.fresh |= first;
        let (h, correction) = if first {
            (w.h.iter().map(|&v| v as f64 * area).collect(), 0.0)
        } else {
            let h = self.cell_volumes(w);
            let c = self.total() - h.iter().sum::<f64>();
            (h, c)
        };
        if self.version != Some(version) {
            self.rebuild(w);
            self.version = Some(version);
        }
        self.init(w, h, correction, !first);
    }

    /// The water of the current state, cell by cell (m³).
    fn cell_volumes(&mut self, w: &Water) -> Vec<f64> {
        let nn = self.n * self.n;
        let area = (w.cell_m * w.cell_m) as f64;
        let mut h = self.store.clone();
        h.resize(nn, 0.0);
        for &(c, v) in &self.carry {
            h[c as usize] += v;
        }
        let tops = self.tops();
        let mut cells = Vec::new();
        for t in tops {
            let l = self.level(t);
            cells.clear();
            self.covered(t, l, &mut cells);
            for &c in &cells {
                h[c as usize] += (l - self.zbed[c as usize] as f64).max(0.0) * area;
            }
        }
        for p in self.pools.iter().filter(|p| p.alive) {
            let l = Self::pool_level(p, p.vol);
            for &c in &p.cells {
                h[c as usize] += (l - self.zbed[c as usize] as f64).max(0.0) * area;
            }
        }
        h
    }

    fn rebuild(&mut self, w: &Water) {
        let n = w.n;
        let nn = n * n;
        self.n = n;
        let lake = w.lake_level.as_slice();
        let closed = w.closed_edge.as_slice();
        let dam = w.dam_height.as_slice();
        let mut outlet = vec![0u8; nn];
        let mut zz = w.z.clone();
        self.zbed = w.z.clone();
        self.lake_cells.clear();
        for i in 0..nn {
            let (x, y) = (i % n, i / n);
            let boundary = x == 0 || y == 0 || x + 1 == n || y + 1 == n;
            if lake[i].is_finite() {
                outlet[i] = 2;
                zz[i] = lake[i].max(w.z[i]);
                self.lake_cells.push(i as u32);
            } else if boundary && w.config.open_edges && !closed[i] {
                outlet[i] = 1;
            }
        }
        let corner = |i: usize, j: usize| -> f32 {
            if i % n == j % n || i / n == j / n {
                return f32::NEG_INFINITY;
            }
            let (a, b) = ((i / n) * n + j % n, (j / n) * n + i % n);
            if dam[a] > 0.0 || dam[b] > 0.0 { zz[a].min(zz[b]) } else { f32::NEG_INFINITY }
        };
        // Priority flood from the outlets: the drainage tree of the filled surface.
        let mut surf = zz.clone();
        let mut parent = vec![NONE; nn];
        let mut done = vec![false; nn];
        let mut heap = BinaryHeap::new();
        for i in 0..nn {
            if outlet[i] > 0 {
                done[i] = true;
                heap.push(std::cmp::Reverse((surf[i].to_bits(), i as u32)));
            }
        }
        if heap.is_empty() {
            let i = (0..nn).min_by(|&a, &b| zz[a].total_cmp(&zz[b]).then(a.cmp(&b))).unwrap();
            done[i] = true;
            heap.push(std::cmp::Reverse((surf[i].to_bits(), i as u32)));
        }
        while let Some(std::cmp::Reverse((_, c))) = heap.pop() {
            let c = c as usize;
            for j in neighbours8(n, c) {
                if !done[j] {
                    done[j] = true;
                    surf[j] = zz[j].max(surf[c] + FLOW_EPS).max(corner(c, j));
                    parent[j] = c as u32;
                    heap.push(std::cmp::Reverse((surf[j].to_bits(), j as u32)));
                }
            }
        }
        // Steepest descent on the raw bed, flats drained by distance, pits grouped.
        let mut d8 = vec![NONE; nn];
        let mut dist = vec![0u32; nn];
        for i in 0..nn {
            if outlet[i] > 0 {
                continue;
            }
            let mut best = 0.0f32;
            for j in neighbours8(n, i) {
                if zz[j] < zz[i] && corner(i, j) <= zz[i] {
                    let diag = i % n != j % n && i / n != j / n;
                    let s = (zz[i] - zz[j]) / if diag { std::f32::consts::SQRT_2 } else { 1.0 };
                    if s > best {
                        best = s;
                        d8[i] = j as u32;
                    }
                }
            }
        }
        let flat_free = |d8: &Vec<u32>, k: usize| d8[k] == NONE && outlet[k] == 0;
        let mut queue = std::collections::VecDeque::new();
        let mut assigned = vec![false; nn];
        for i in 0..nn {
            if !flat_free(&d8, i) {
                assigned[i] = true;
                if neighbours8(n, i).any(|j| flat_free(&d8, j) && zz[j] == zz[i]) {
                    queue.push_back(i);
                }
            }
        }
        while let Some(c) = queue.pop_front() {
            for j in neighbours8(n, c) {
                if !assigned[j] && zz[j] == zz[c] && corner(j, c) <= zz[j] {
                    assigned[j] = true;
                    d8[j] = c as u32;
                    dist[j] = dist[c] + 1;
                    queue.push_back(j);
                }
            }
        }
        let mut label = vec![NONE; nn];
        let mut leaves: Vec<u32> = Vec::new();
        for i in 0..nn {
            if assigned[i] || label[i] != NONE {
                continue;
            }
            let id = leaves.len() as u32;
            leaves.push(i as u32);
            label[i] = id;
            queue.push_back(i);
            while let Some(c) = queue.pop_front() {
                for j in neighbours8(n, c) {
                    if !assigned[j] && label[j] == NONE && zz[j] == zz[c] && corner(j, c) <= zz[j] {
                        label[j] = id;
                        d8[j] = c as u32;
                        dist[j] = dist[c] + 1;
                        queue.push_back(j);
                    }
                }
            }
        }
        let mut pit_leaf = vec![NONE; nn];
        for (id, &c) in leaves.iter().enumerate() {
            pit_leaf[c as usize] = id as u32;
        }
        for i in 0..nn {
            if outlet[i] > 0 {
                label[i] = OCEAN;
            }
        }
        let mut path = Vec::new();
        for i in 0..nn {
            let mut c = i;
            while label[c] == NONE {
                path.push(c);
                c = d8[c] as usize;
            }
            let l = label[c];
            for &p in &path {
                label[p] = l;
            }
            path.clear();
        }
        // Lowest saddles between watersheds, joined in order (Kruskal) into the hierarchy.
        let mut edges: Vec<(u32, u32, u32)> = Vec::new();
        for i in 0..nn {
            for j in neighbours8(n, i) {
                if j > i && label[i] != label[j] {
                    let e = zz[i].max(zz[j]).max(corner(i, j));
                    edges.push((ord(e), i as u32, j as u32));
                }
            }
        }
        edges.sort_unstable();
        let nl = leaves.len();
        let ocean_set = nl;
        let mut uf: Vec<u32> = (0..=nl as u32).collect();
        let mut linked = vec![false; nl + 1];
        linked[ocean_set] = true;
        let mut topd: Vec<u32> = (0..=nl as u32).collect();
        let mut deps: Vec<Dep> = (0..nl)
            .map(|k| Dep {
                parent: NONE,
                kids: [NONE, NONE],
                spill: f64::INFINITY,
                base: 0.0,
                desc: 0.0,
                start: 0,
                end: 0,
                own_cap: 0.0,
                crest: NONE,
                target: NONE,
                root: NONE,
                zmin: zz[leaves[k] as usize],
                weir: 1.0,
                rim: vec![],
                lake: f64::NAN,
            })
            .collect();
        fn find(uf: &mut [u32], mut a: usize) -> usize {
            let mut r = a;
            while uf[r] as usize != r {
                r = uf[r] as usize;
            }
            while uf[a] as usize != r {
                let next = uf[a] as usize;
                uf[a] = r as u32;
                a = next;
            }
            r
        }
        let set_of = |l: u32| if l == OCEAN { ocean_set } else { l as usize };
        let pour = |from: usize, to: usize| -> (u32, u32) {
            let crest = if zz[from] >= zz[to] { from } else { to };
            (crest as u32, to as u32)
        };
        for &(eb, i, j) in &edges {
            let (i, j) = (i as usize, j as usize);
            let (ra, rb) = (find(&mut uf, set_of(label[i])), find(&mut uf, set_of(label[j])));
            if ra == rb {
                continue;
            }
            let e = {
                let b = if eb & 0x8000_0000 != 0 { eb ^ 0x8000_0000 } else { !eb };
                f32::from_bits(b) as f64
            };
            let (la, lb) = (linked[ra], linked[rb]);
            if la && lb {
                uf[rb] = ra as u32;
                continue;
            }
            if la || lb {
                let (lk, un, from, to) = if la { (ra, rb, j, i) } else { (rb, ra, i, j) };
                let d = topd[un] as usize;
                deps[d].spill = e;
                (deps[d].crest, deps[d].target) = pour(from, to);
                uf[un] = lk as u32;
                continue;
            }
            let (da, db) = (topd[ra] as usize, topd[rb] as usize);
            let m = deps.len();
            deps[da].spill = e;
            deps[db].spill = e;
            (deps[da].crest, deps[da].target) = pour(i, j);
            (deps[db].crest, deps[db].target) = pour(j, i);
            deps[da].parent = m as u32;
            deps[db].parent = m as u32;
            let zmin = deps[da].zmin.min(deps[db].zmin);
            deps.push(Dep {
                parent: NONE,
                kids: [da as u32, db as u32],
                spill: f64::INFINITY,
                base: e,
                desc: 0.0,
                start: 0,
                end: 0,
                own_cap: 0.0,
                crest: NONE,
                target: NONE,
                root: NONE,
                zmin,
                weir: 1.0,
                rim: vec![],
                lake: f64::NAN,
            });
            uf[rb] = ra as u32;
            topd[ra] = m as u32;
        }
        // Each inside cell belongs to the lowest depression whose spill is above it.
        let mut own_of = vec![NONE; nn];
        let mut order: Vec<(u32, u32, u32)> = (0..nn)
            .filter(|&i| label[i] != OCEAN)
            .map(|i| (label[i], ord(zz[i]), i as u32))
            .collect();
        order.sort_unstable();
        let mut k = 0;
        while k < order.len() {
            let l = order[k].0;
            let mut d = l;
            while k < order.len() && order[k].0 == l {
                let i = order[k].2 as usize;
                while d != NONE && deps[d as usize].spill <= zz[i] as f64 {
                    d = deps[d as usize].parent;
                }
                own_of[i] = d;
                k += 1;
            }
        }
        let mut cells: Vec<(u32, u32, u32)> =
            (0..nn).filter(|&i| own_of[i] != NONE).map(|i| (own_of[i], ord(zz[i]), i as u32)).collect();
        cells.sort_unstable();
        self.dcells = cells.iter().map(|c| c.2).collect();
        self.dbeds = cells.iter().map(|c| zz[c.2 as usize] as f64).collect();
        let nd = deps.len();
        self.dprefix = Vec::with_capacity(self.dcells.len() + nd);
        let mut k = 0;
        for (d, dep) in deps.iter_mut().enumerate() {
            dep.start = k as u32;
            self.dprefix.push(0.0);
            let mut acc = 0.0;
            while k < cells.len() && cells[k].0 as usize == d {
                acc += self.dbeds[k];
                self.dprefix.push(acc);
                k += 1;
            }
            dep.end = k as u32;
        }
        for d in 0..nd {
            if deps[d].kids[0] != NONE {
                let [a, b] = deps[d].kids;
                let count = |x: &Dep| x.desc + (x.end - x.start) as f64;
                deps[d].desc = count(&deps[a as usize]) + count(&deps[b as usize]);
            }
            let (s, e) = (deps[d].start as usize, deps[d].end as usize);
            if s < e {
                deps[d].zmin = deps[d].zmin.min(self.dbeds[s] as f32);
            }
            deps[d].own_cap = if deps[d].spill.is_finite() {
                volume_on(deps[d].spill, deps[d].desc, deps[d].base, &self.dbeds[s..e], &self.dprefix[s + d..e + d + 1])
            } else {
                f64::INFINITY
            };
        }
        for d in (0..nd).rev() {
            deps[d].root = if deps[d].parent == NONE { d as u32 } else { deps[deps[d].parent as usize].root };
        }
        // A full depression overflows into its sibling: aim at the first inside cell downhill.
        for d in 0..nd {
            if deps[d].parent == NONE {
                continue;
            }
            let mut t = deps[d].target as usize;
            let mut guard = 0;
            while own_of[t] == NONE && d8[t] != NONE && guard < nn {
                t = d8[t] as usize;
                guard += 1;
            }
            deps[d].target = t as u32;
        }
        // A root pours where the flood first entered it, so its outflow never runs back in.
        let mut entry: Vec<(u32, u32)> = vec![(u32::MAX, u32::MAX); nd];
        for i in 0..nn {
            if own_of[i] != NONE {
                let r = deps[own_of[i] as usize].root as usize;
                entry[r] = entry[r].min((surf[i].to_bits(), i as u32));
            }
        }
        for d in 0..nd {
            if deps[d].parent == NONE && deps[d].spill.is_finite() && entry[d].1 != u32::MAX {
                let q = parent[entry[d].1 as usize];
                if q != NONE {
                    deps[d].crest = q;
                    deps[d].target = q;
                }
            }
        }
        // Roots: the crest cells their spill pours over, and fixed-lake links.
        let mut rims: Vec<Vec<(u32, u32)>> = vec![Vec::new(); nd];
        for i in 0..nn {
            if own_of[i] != NONE {
                continue;
            }
            let mut roots: Vec<u32> = neighbours8(n, i)
                .filter(|&j| own_of[j] != NONE)
                .map(|j| deps[own_of[j] as usize].root)
                .collect();
            roots.sort_unstable();
            roots.dedup();
            for r in roots {
                let sp = deps[r as usize].spill;
                if sp.is_finite() && (zz[i] as f64 - sp).abs() <= RIM_TOL as f64 {
                    rims[r as usize].push((ord(zz[i]), i as u32));
                }
            }
        }
        for d in 0..nd {
            if deps[d].parent != NONE || !deps[d].spill.is_finite() {
                continue;
            }
            let mut rim = std::mem::take(&mut rims[d]);
            let crest = deps[d].crest;
            if !rim.iter().any(|r| r.1 == crest) {
                rim.push((ord(zz[crest as usize]), crest));
            }
            rim.sort_unstable_by_key(|&(z, i)| (i != crest, z, i));
            rim.truncate(WEIR_MAX_M.max(1));
            deps[d].weir = rim.len().max(1) as f64;
            deps[d].rim = rim.into_iter().map(|r| r.1).collect();
            let t = deps[d].target as usize;
            if outlet[t] == 2 {
                deps[d].lake = lake[t] as f64;
            }
        }
        // Routing graph: outside depressions the filled-surface tree, inside steepest descent.
        self.recv = vec![NONE; nn];
        self.key = vec![0; nn];
        for i in 0..nn {
            if own_of[i] != NONE {
                self.recv[i] = d8[i];
                self.key[i] = hkey(zz[i], 1 + dist[i]);
            } else {
                self.recv[i] = if outlet[i] > 0 { NONE } else { parent[i] };
                self.key[i] = hkey(surf[i], 1);
            }
        }
        self.slope = vec![0.0; nn];
        for i in 0..nn {
            let (mut c, mut len, mut drop) = (i, 0.0f32, 0.0f32);
            for _ in 0..SLOPE_CELLS {
                let r = self.recv[c];
                if r == NONE {
                    if outlet[c] == 1 {
                        let (x, y) = (c % n, c / n);
                        let inner = if x == 0 {
                            c + 1
                        } else if x + 1 == n {
                            c - 1
                        } else if y == 0 {
                            c + n
                        } else {
                            c - n
                        };
                        drop += zz[c] - edge_bed(&zz, c, inner, w.cell_m);
                        len += w.cell_m;
                    }
                    break;
                }
                let r = r as usize;
                let diag = c % n != r % n && c / n != r / n;
                len += if diag { std::f32::consts::SQRT_2 } else { 1.0 } * w.cell_m;
                drop += zz[c] - zz[r];
                c = r;
            }
            self.slope[i] = if len > 0.0 { (drop / len) as f64 } else { 0.0 }.clamp(SLOPE_MIN, SLOPE_MAX) as f32;
        }
        self.own_of = own_of;
        self.pit_leaf = pit_leaf;
        self.outlet = outlet;
        self.deps = deps;
        self.pend = vec![0.0; nn];
        self.inq = vec![false; nn];
        self.vstamp = vec![0; nn];
        self.s0 = vec![0.0; nn];
        self.iacc = vec![0.0; nn];
        self.oacc = vec![0.0; nn];
        self.wmark = vec![false; nn];
        self.dstamp = vec![0; nd];
        self.dinq = vec![false; nd];
        self.lake_roots = (0..nd).filter(|&d| self.deps[d].lake.is_finite()).map(|d| d as u32).collect();
        self.dfwd = vec![0.0; nd];
        self.dcnt = vec![0; nd];
        if self.fx.len() != n * (n + 1) {
            self.fx = vec![0.0; n * (n + 1)];
            self.fy = vec![0.0; n * (n + 1)];
            self.cx = vec![0; n * (n + 1)];
            self.cy = vec![0; n * (n + 1)];
        }
    }

    /// State from the published depths `w.h`, plus a volume correction (m³).
    fn init(&mut self, w: &mut Water, mut h: Vec<f64>, correction: f64, pools: bool) {
        let n = w.n;
        let nn = n * n;
        let nd = self.deps.len();
        let area = (w.cell_m * w.cell_m) as f64;
        for &c in &self.lake_cells {
            h[c as usize] = 0.0;
        }
        if correction != 0.0 {
            let m = (0..nn).max_by(|&a, &b| h[a].total_cmp(&h[b]).then(b.cmp(&a))).unwrap();
            let v = h[m] + correction;
            if v < 0.0 {
                w.books.sources -= v;
                w.added_volume -= v;
            }
            h[m] = v.max(0.0);
        }
        self.own = vec![0.0; nd];
        self.slow = vec![false; nd];
        self.full = vec![false; nd];
        self.lvl = vec![0.0; nd];
        self.dirty = vec![true; nd];
        self.wet_flag = vec![false; nd];
        self.wet_deps.clear();
        self.store = vec![0.0; nn];
        self.wet.clear();
        self.pools.clear();
        self.pool_of = vec![NONE; nn];
        self.carry.clear();
        let mut tot = vec![0.0; nd];
        for i in 0..nn {
            if h[i] > 0.0 && self.own_of[i] != NONE {
                tot[self.own_of[i] as usize] += h[i];
            }
        }
        let mut wv = vec![0.0; nd];
        let mut own = vec![0.0; nd];
        let mut capv = vec![0.0; nd];
        for d in 0..nd {
            let kids = self.deps[d].kids;
            let mut kid_w = 0.0;
            capv[d] = self.deps[d].own_cap;
            if kids[0] != NONE {
                for k in kids {
                    tot[d] += tot[k as usize];
                    kid_w += wv[k as usize];
                    capv[d] += capv[k as usize];
                }
            }
            let cap = capv[d];
            wv[d] = if self.deps[d].parent == NONE { tot[d] } else { tot[d].min(cap) };
            own[d] = (wv[d] - kid_w).max(0.0);
        }
        for d in 0..nd {
            if own[d] > 0.0 {
                self.set_own(d, own[d]);
            }
        }
        for d in 0..nd {
            self.refresh_full(d);
        }
        // Water a merged level holds above children that are not full fills them first;
        // what a full depression cannot hold belongs to its parent.
        for d in 0..nd {
            let kids = self.deps[d].kids;
            if kids[0] != NONE && self.own[d] > 0.0 && !(self.full[kids[0] as usize] && self.full[kids[1] as usize]) {
                let x = self.own[d];
                self.set_own(d, 0.0);
                let left = self.fill(d, x);
                self.set_own(d, self.own[d] + left);
            }
            let p = self.deps[d].parent;
            let cap = self.deps[d].own_cap;
            if p != NONE && self.own[d] > cap {
                let x = self.own[d] - cap;
                self.set_own(d, cap);
                self.set_own(p as usize, self.own[p as usize] + x);
            }
        }
        // Water outside depressions is channel storage, or a stranded pool after a rebuild.
        let mut cand = vec![false; nn];
        for i in 0..nn {
            if self.own_of[i] == NONE && h[i] > 0.0 {
                if pools && h[i] > POOL_MIN as f64 * area && self.outlet[i] == 0 {
                    cand[i] = true;
                } else {
                    self.store[i] = h[i];
                    self.wet.push(i as u32);
                }
            }
        }
        if pools {
            self.make_pools(w, &cand, &h);
            // Water a rebuild leaves standing above a spill drains through a breach-sized weir.
            for d in 0..nd {
                let dep = &self.deps[d];
                if dep.parent == NONE && dep.spill.is_finite() && self.own[d] - dep.own_cap > POOL_MIN as f64 * self.area(d) {
                    self.slow[d] = true;
                }
            }
        }
    }

    /// Fill the children of `d` (deepest first) with up to `x`; return what is left.
    fn fill(&mut self, d: usize, mut x: f64) -> f64 {
        let mut stack: Vec<(usize, bool)> = self.deps[d].kids.iter().rev().map(|&k| (k as usize, false)).collect();
        while let Some((k, expanded)) = stack.pop() {
            if x <= 0.0 {
                break;
            }
            if self.full[k] {
                continue;
            }
            let kids = self.deps[k].kids;
            if !expanded && kids[0] != NONE {
                stack.push((k, true));
                stack.push((kids[1] as usize, false));
                stack.push((kids[0] as usize, false));
                continue;
            }
            let room = (self.deps[k].own_cap - self.own[k]).max(0.0);
            let take = room.min(x);
            if take > 0.0 {
                self.set_own(k, self.own[k] + take);
                x -= take;
            }
        }
        x
    }

    fn make_pools(&mut self, w: &Water, cand: &[bool], h: &[f64]) {
        let n = w.n;
        let mut seen = vec![false; n * n];
        for s in 0..n * n {
            if !cand[s] || seen[s] {
                continue;
            }
            let mut comp = vec![s];
            seen[s] = true;
            let mut k = 0;
            while k < comp.len() {
                let c = comp[k];
                k += 1;
                for j in neighbours8(n, c) {
                    if cand[j] && !seen[j] {
                        seen[j] = true;
                        comp.push(j);
                    }
                }
            }
            comp.sort_unstable();
            let id = self.pools.len() as u32;
            for &c in &comp {
                self.pool_of[c] = id;
            }
            let exit = comp
                .iter()
                .copied()
                .filter(|&c| self.recv[c] == NONE || self.pool_of[self.recv[c] as usize] != id)
                .min_by_key(|&c| (self.key[c], c))
                .unwrap_or(comp[0]);
            let mut beds: Vec<f64> = comp.iter().map(|&c| w.z[c] as f64).collect();
            beds.sort_unstable_by(f64::total_cmp);
            let mut prefix = vec![0.0];
            for &b in &beds {
                prefix.push(prefix.last().unwrap() + b);
            }
            let key = comp.iter().map(|&c| self.key[c]).min().unwrap() & !0xffff_ffff;
            self.pools.push(Pool {
                vol: comp.iter().map(|&c| h[c]).sum(),
                cells: comp.iter().map(|&c| c as u32).collect(),
                beds,
                prefix,
                exit: exit as u32,
                crest: w.z[exit] as f64,
                key,
                fwd: 0.0,
                stamp: 0,
                alive: true,
            });
        }
    }

    fn pool_level(p: &Pool, v: f64) -> f64 {
        level_on(v, 0.0, 0.0, &p.beds, &p.prefix)
    }

    fn schedule_pool(&mut self, p: usize) {
        let nn = (self.n * self.n) as u32;
        if self.pools[p].stamp != self.stamp {
            self.pools[p].stamp = self.stamp;
            self.pools[p].fwd = 0.0;
        }
        self.heap.push((self.pools[p].key, nn + self.deps.len() as u32 + p as u32));
    }

    pub(super) fn advance(&mut self, w: &mut Water, seconds: f64) {
        let mut left = seconds;
        while left > 1.0e-6 {
            let dt = if left <= MAX_STEP_S + 1.0e-3 { left } else { MAX_STEP_S };
            self.step(w, dt);
            left -= dt;
            w.physics_seconds += dt;
            w.steps_taken += 1;
            w.timings.physics_s += dt;
            w.timings.substeps += 1;
        }
    }

    fn step(&mut self, w: &mut Water, dt: f64) {
        self.stamp += 1;
        self.dt = dt;
        self.touched.clear();
        self.crests.clear();
        let area = (w.cell_m * w.cell_m) as f64;
        let manning = w.config.manning as f64;
        let nn = (self.n * self.n) as u32;
        let nd = self.deps.len() as u32;
        for k in 0..w.source_cells.len() {
            let i = w.source_cells[k];
            let q = w.src[i] as f64 * area;
            if q > 0.0 {
                w.books.sources += q * dt;
                w.added_volume += q * dt;
                self.add(i as u32, q);
            }
        }
        for (c, v) in std::mem::take(&mut self.carry) {
            self.add(c, v / dt);
        }
        for k in 0..self.wet.len() {
            let c = self.wet[k];
            self.add(c, 0.0);
        }
        for k in 0..self.wet_deps.len() {
            let d = self.wet_deps[k] as usize;
            if self.deps[d].parent == NONE && self.deps[d].spill.is_finite() {
                let over = self.own[d] > self.deps[d].own_cap;
                let low = self.deps[d].lake.is_finite() && self.own[d] < self.deps[d].own_cap;
                if over || low {
                    self.schedule_dep(d);
                }
            }
        }
        for k in 0..self.lake_roots.len() {
            let d = self.lake_roots[k] as usize;
            if !self.wet_flag[d] && self.own[d] < self.deps[d].own_cap {
                self.schedule_dep(d);
            }
        }
        for p in 0..self.pools.len() {
            if self.pools[p].alive {
                self.schedule_pool(p);
            }
        }
        let mut pops = 0u64;
        while let Some((_, id)) = self.heap.pop() {
            pops += 1;
            if id < nn {
                self.visit(w, id as usize, dt, manning);
            } else if id < nn + nd {
                self.dinq[(id - nn) as usize] = false;
                self.solve_root(w, (id - nn) as usize, dt);
            } else {
                self.solve_pool((id - nn - nd) as usize, dt);
            }
        }
        w.timings.cell_updates += pops;
        self.wet.clear();
        for k in 0..self.touched.len() {
            let c = self.touched[k] as usize;
            if self.store[c] > DRY_M3 {
                self.wet.push(c as u32);
            } else if self.store[c] > 0.0 {
                w.books.seepage += self.store[c];
                w.lost_volume += self.store[c];
                self.store[c] = 0.0;
            }
        }
        self.wet.sort_unstable();
        self.wet.dedup();
        self.wet_deps.retain(|&d| {
            let keep = self.own[d as usize] > 0.0;
            if !keep {
                self.wet_flag[d as usize] = false;
            }
            keep
        });
        self.last_dt = dt;
        self.dt = 0.0;
    }

    fn visit(&mut self, w: &mut Water, c: usize, dt: f64, manning: f64) {
        self.inq[c] = false;
        let rate = std::mem::take(&mut self.pend[c]);
        if self.vstamp[c] != self.stamp {
            self.vstamp[c] = self.stamp;
            self.s0[c] = self.store[c];
            self.iacc[c] = 0.0;
            self.oacc[c] = 0.0;
            self.touched.push(c as u32);
        }
        self.iacc[c] += rate;
        if self.outlet[c] == 2 {
            let v = rate * dt + std::mem::take(&mut self.store[c]);
            w.books.edges += v;
            w.lost_volume += v;
            return;
        }
        let z = w.z[c] as f64;
        let p = self.pool_of[c];
        if p != NONE && self.pools[p as usize].alive {
            let pool = &self.pools[p as usize];
            if Self::pool_level(pool, pool.vol) > z {
                let v = self.absorb(c, rate, dt);
                self.pools[p as usize].vol += v;
                self.schedule_pool(p as usize);
                return;
            }
        }
        let d = self.own_of[c];
        if d != NONE {
            let t = self.top(d as usize);
            if self.level(t) > z {
                let v = self.absorb(c, rate, dt);
                self.deposit(t, v);
                return;
            }
        }
        if self.recv[c] == NONE && self.outlet[c] == 0 {
            let leaf = self.pit_leaf[c];
            let v = self.absorb(c, rate, dt);
            if leaf != NONE {
                self.deposit(leaf as usize, v);
            } else {
                self.store[c] = v;
            }
            return;
        }
        // A reach: hold the channel storage of normal flow, pass on the rest.
        let i_rate = self.iacc[c];
        let wd = Self::width(i_rate);
        let hn = Self::normal_depth(i_rate, wd, self.slope[c] as f64, manning);
        let target = hn * wd * w.cell_m as f64;
        let s0 = self.s0[c];
        let inflow = i_rate * dt;
        let want = if target >= s0 { target.min(s0 + inflow) } else { target };
        let mut out = (s0 + inflow - want).max(self.oacc[c]);
        if out - self.oacc[c] <= 1.0e-12 {
            out = self.oacc[c];
        }
        self.store[c] = s0 + inflow - out;
        let delta = out - self.oacc[c];
        self.oacc[c] = out;
        if delta > 1.0e-12 {
            if self.outlet[c] == 1 {
                w.books.edges += delta;
                w.lost_volume += delta;
            } else {
                self.add(self.recv[c], delta / dt);
            }
        }
    }

    /// Everything cell `c` holds or receives this step, taken for a lake.
    fn absorb(&mut self, c: usize, rate: f64, dt: f64) -> f64 {
        let v = rate * dt + std::mem::take(&mut self.store[c]);
        self.s0[c] = 0.0;
        self.iacc[c] = 0.0;
        self.oacc[c] = 0.0;
        v
    }

    fn solve_root(&mut self, w: &mut Water, d: usize, dt: f64) {
        if !self.deps[d].spill.is_finite() {
            return;
        }
        self.dtouch(d);
        self.dcnt[d] += 1;
        if self.dcnt[d] > 4 * LOOP_LIMIT {
            return;
        }
        let dep = &self.deps[d];
        let b = if self.slow[d] { POOL_WEIR_M } else { dep.weir };
        let (cap, a, lake, target) = (dep.own_cap, self.area(d), dep.lake, dep.target);
        let fwd = self.dfwd[d];
        let x = self.own[d] + fwd - cap;
        if x < 0.0 {
            if lake.is_finite() && fwd == 0.0 {
                let l = self.level(d);
                let q = super::WEIR as f64 * b * (lake - l).max(0.0).powf(1.5);
                let v = (q * dt).min(-x);
                if v > 0.0 {
                    w.books.sources += v;
                    w.added_volume += v;
                    self.set_own(d, self.own[d] + v);
                }
            }
            return;
        }
        // A·H + c·H^1.5 = x: the head the weir leaves after this step.
        let c = super::WEIR as f64 * b * dt;
        let mut hd = (x / a).min((x / c).powf(2.0 / 3.0));
        for _ in 0..30 {
            let f = a * hd + c * hd.powf(1.5) - x;
            let df = a + 1.5 * c * hd.max(1.0e-12).sqrt();
            let next = (hd - f / df).max(0.0);
            if (next - hd).abs() < 1.0e-12 {
                hd = next;
                break;
            }
            hd = next;
        }
        if hd < POOL_END {
            self.slow[d] = false;
        }
        let out = (x - a * hd).max(0.0);
        let delta = out - fwd;
        if delta > 0.0 {
            self.dfwd[d] = out;
            self.set_own(d, self.own[d] - delta);
            self.add(target, delta / dt);
        }
    }

    fn solve_pool(&mut self, p: usize, dt: f64) {
        if !self.pools[p].alive {
            return;
        }
        if self.pools[p].stamp != self.stamp {
            self.pools[p].stamp = self.stamp;
            self.pools[p].fwd = 0.0;
        }
        let pool = &self.pools[p];
        let total = pool.vol + pool.fwd;
        let q = |v: f64| super::WEIR as f64 * POOL_WEIR_M * (Self::pool_level(pool, v) - pool.crest).max(0.0).powf(1.5);
        let (mut lo, mut hi) = (0.0, total);
        for _ in 0..60 {
            let mid = 0.5 * (lo + hi);
            if mid + q(mid) * dt > total { hi = mid } else { lo = mid }
        }
        let end = hi;
        let head = Self::pool_level(pool, end) - pool.crest;
        let out = total - end;
        let delta = out - pool.fwd;
        let exit = pool.exit;
        if delta > 0.0 {
            let pool = &mut self.pools[p];
            pool.fwd = out;
            pool.vol -= delta;
            let r = self.recv[exit as usize];
            let to = if r == NONE { exit } else { r };
            self.add(to, delta / dt);
        }
        if head < POOL_END {
            let pool = &mut self.pools[p];
            pool.alive = false;
            let v = std::mem::take(&mut pool.vol);
            let cells = std::mem::take(&mut pool.cells);
            for c in cells {
                self.pool_of[c as usize] = NONE;
            }
            self.add(exit, v / dt);
        }
    }

    pub(super) fn environment(&mut self, w: &mut Water, days: f64) {
        w.update_riverbed(days as f32);
        let forcing = w.forcing();
        let area = (w.cell_m * w.cell_m) as f64;
        let r = forcing.rain as f64 * days;
        let e = w.config.evaporation as f64 * forcing.evaporation as f64 * days;
        let s = w.config.seepage as f64 * days;
        let bed = w.riverbed.as_slice();
        let canal = w.canal.as_slice();
        let loss = |i: usize, depth: f64| -> (f64, f64) {
            let ev = e.min(depth + r);
            let inf = if bed[i] < 0.5 && !canal[i] { 4.0 * s } else { s };
            (ev, inf.min((depth + r - ev).max(0.0)))
        };
        let (mut rain, mut evap, mut seep) = (0.0, 0.0, 0.0);
        let tops = self.tops();
        let mut cells = Vec::new();
        for t in tops {
            let l = self.level(t);
            cells.clear();
            self.covered(t, l, &mut cells);
            let (mut rn, mut ev, mut sp) = (0.0, 0.0, 0.0);
            for &c in &cells {
                let depth = l - w.z[c as usize] as f64;
                let (a, b) = loss(c as usize, depth);
                rn += r;
                ev += a;
                sp += b;
            }
            let (rn, ev, sp) = (rn * area, ev * area, sp * area);
            let net = rn - ev - sp;
            rain += rn;
            if net >= 0.0 {
                evap += ev;
                seep += sp;
                self.deposit(t, net);
            } else {
                let got = self.drain(t, -net);
                let f = if ev + sp > 0.0 { (rn + got) / (ev + sp) } else { 0.0 };
                evap += ev * f;
                seep += sp * f;
            }
        }
        for k in 0..self.pools.len() {
            if !self.pools[k].alive {
                continue;
            }
            let l = Self::pool_level(&self.pools[k], self.pools[k].vol);
            let (mut rn, mut ls) = (0.0, (0.0, 0.0));
            for &c in &self.pools[k].cells {
                let depth = l - w.z[c as usize] as f64;
                if depth > 0.0 {
                    let (a, b) = loss(c as usize, depth);
                    rn += r * area;
                    ls.0 += a * area;
                    ls.1 += b * area;
                }
            }
            let pool = &mut self.pools[k];
            let net = rn - ls.0 - ls.1;
            let after = (pool.vol + net).max(0.0);
            let lost = pool.vol + rn - after;
            let f = if ls.0 + ls.1 > 0.0 { lost / (ls.0 + ls.1) } else { 0.0 };
            pool.vol = after;
            rain += rn;
            evap += ls.0 * f;
            seep += ls.1 * f;
        }
        for &c in &self.wet {
            let c = c as usize;
            let v = self.store[c];
            if v <= 0.0 {
                continue;
            }
            let depth = v / area;
            let (a, b) = loss(c, depth);
            let after = (v + (r - a - b) * area).max(0.0);
            let lost = v + r * area - after;
            let f = if a + b > 0.0 { lost / ((a + b) * area) } else { 0.0 };
            self.store[c] = after;
            rain += r * area;
            evap += a * area * f;
            seep += b * area * f;
        }
        w.books.rain += rain;
        w.books.evaporation += evap;
        w.books.seepage += seep;
        w.added_volume += rain;
        w.lost_volume += evap + seep;
    }

    fn write(&mut self, w: &mut Water, c: usize, h: f64) {
        if h <= 0.0 {
            return;
        }
        let h = h as f32;
        if !self.wmark[c] {
            self.wmark[c] = true;
            self.written.push(c as u32);
            w.h[c] = h;
        } else if h > w.h[c] {
            w.h[c] = h;
        }
    }

    fn flux(&mut self, c: usize, q: f64, dir: (f64, f64)) {
        let n = self.n;
        let (x, y) = (c % n, c / n);
        let len = (dir.0 * dir.0 + dir.1 * dir.1).sqrt();
        if q <= 0.0 || len <= 0.0 {
            return;
        }
        let (ux, uy) = ((q * dir.0 / len) as f32, (q * dir.1 / len) as f32);
        for f in [y * (n + 1) + x, y * (n + 1) + x + 1] {
            if self.cx[f] == 0 {
                self.faces_x.push(f as u32);
            }
            self.fx[f] += ux;
            self.cx[f] += 1;
        }
        for f in [c, c + n] {
            if self.cy[f] == 0 {
                self.faces_y.push(f as u32);
            }
            self.fy[f] += uy;
            self.cy[f] += 1;
        }
    }

    fn dir(&self, c: usize, to: usize) -> (f64, f64) {
        let n = self.n;
        ((to % n) as f64 - (c % n) as f64, (to / n) as f64 - (c / n) as f64)
    }

    pub(super) fn publish(&mut self, w: &mut Water) {
        let n = self.n;
        let area = (w.cell_m * w.cell_m) as f64;
        if std::mem::take(&mut self.fresh) {
            w.h.fill(0.0);
            w.qx.fill(0.0);
            w.qy.fill(0.0);
        }
        for &c in &self.written {
            w.h[c as usize] = 0.0;
            self.wmark[c as usize] = false;
        }
        self.written.clear();
        for &f in &self.prev_x {
            w.qx[f as usize] = 0.0;
        }
        for &f in &self.prev_y {
            w.qy[f as usize] = 0.0;
        }
        for row in &mut w.rows {
            row.clear();
        }
        for k in 0..self.lake_cells.len() {
            let c = self.lake_cells[k] as usize;
            let h = w.lake_level.as_slice()[c] as f64 - w.z[c] as f64;
            self.write(w, c, h);
        }
        let tops = self.tops();
        let mut cells = Vec::new();
        for t in tops {
            let l = self.level(t);
            cells.clear();
            self.covered(t, l, &mut cells);
            for k in 0..cells.len() {
                let c = cells[k] as usize;
                self.write(w, c, l - w.z[c] as f64);
            }
            let dep = &self.deps[t];
            if dep.parent == NONE && dep.spill.is_finite() && l > dep.spill {
                let q = if self.dstamp[t] == self.stamp { self.dfwd[t] / self.dt_last() } else { 0.0 };
                let rim = dep.rim.clone();
                let wet: Vec<u32> = rim.into_iter().filter(|&c| l > w.z[c as usize] as f64).collect();
                let qc = q / (wet.len() as f64 * w.cell_m as f64);
                for &c in &wet {
                    let c = c as usize;
                    self.write(w, c, l - w.z[c] as f64);
                    let mut dir = (0.0, 0.0);
                    let near: Vec<usize> = neighbours8(n, c)
                        .filter(|&j| self.own_of[j] != NONE && self.deps[self.own_of[j] as usize].root as usize == t)
                        .collect();
                    for &j in &near {
                        let d = self.dir(j, c);
                        dir.0 += d.0;
                        dir.1 += d.1;
                        // The lake draws towards its outlet.
                        self.flux(j, qc / near.len() as f64, d);
                    }
                    self.flux(c, qc, dir);
                }
            }
        }
        for k in 0..self.pools.len() {
            if !self.pools[k].alive {
                continue;
            }
            let l = Self::pool_level(&self.pools[k], self.pools[k].vol);
            for j in 0..self.pools[k].cells.len() {
                let c = self.pools[k].cells[j] as usize;
                self.write(w, c, l - w.z[c] as f64);
            }
            let pool = &self.pools[k];
            let q = if pool.stamp == self.stamp { pool.fwd / self.dt_last() } else { 0.0 };
            let exit = pool.exit as usize;
            let to = self.recv[exit];
            if q > 0.0 && to != NONE {
                let near: Vec<usize> = neighbours8(n, exit).filter(|&j| self.pool_of[j] == k as u32 && l > w.z[j] as f64).collect();
                for &j in &near {
                    self.flux(j, q / near.len() as f64, self.dir(j, exit));
                }
                self.flux(exit, q, self.dir(exit, to as usize));
            }
        }
        for k in 0..self.wet.len() {
            let c = self.wet[k] as usize;
            let q = if self.vstamp[c] == self.stamp { self.oacc[c] / self.dt_last() } else { 0.0 };
            let i_rate = if self.vstamp[c] == self.stamp { self.iacc[c] } else { 0.0 };
            let wd = Self::width(i_rate);
            let h = self.store[c] / (wd * w.cell_m as f64);
            self.write(w, c, h);
            let wd = wd.max(w.cell_m as f64);
            let to = self.recv[c];
            let dir = if to != NONE {
                self.dir(c, to as usize)
            } else {
                let (x, y) = (c % n, c / n);
                if x == 0 {
                    (-1.0, 0.0)
                } else if x + 1 == n {
                    (1.0, 0.0)
                } else if y == 0 {
                    (0.0, -1.0)
                } else {
                    (0.0, 1.0)
                }
            };
            self.flux(c, q / wd, dir);
            // A channel wider than a cell spreads sideways at the same stage.
            let side = ((wd / w.cell_m as f64 - 1.0) / 2.0).round() as i64;
            if side > 0 && h > 0.0 {
                let stage = w.z[c] as f64 + h;
                let (px, py) = (-dir.1 as i64, dir.0 as i64);
                for sign in [1i64, -1] {
                    for m in 1..=side {
                        let (x, y) = ((c % n) as i64 + sign * m * px, (c / n) as i64 + sign * m * py);
                        if x < 0 || y < 0 || x >= n as i64 || y >= n as i64 {
                            break;
                        }
                        let s = y as usize * n + x as usize;
                        let hs = (stage - w.z[s] as f64).min(h);
                        if hs <= 0.0 {
                            break;
                        }
                        self.write(w, s, hs);
                        self.flux(s, q / wd, dir);
                    }
                }
            }
        }
        let crests = std::mem::take(&mut self.crests);
        for &(crest, target, q) in &crests {
            let c = crest as usize;
            if self.wmark[c] && w.h[c] > 0.0 && self.own_of[c] != NONE {
                continue;
            }
            self.write(w, c, super::spill_head(q as f32, w.cell_m) as f64);
            if c != target as usize {
                self.flux(c, q, self.dir(c, target as usize));
            }
        }
        self.crests = crests;
        let _ = area;
        for k in 0..self.faces_x.len() {
            let f = self.faces_x[k] as usize;
            w.qx[f] = self.fx[f] / self.cx[f] as f32;
            self.fx[f] = 0.0;
            self.cx[f] = 0;
        }
        for k in 0..self.faces_y.len() {
            let f = self.faces_y[k] as usize;
            w.qy[f] = self.fy[f] / self.cy[f] as f32;
            self.fy[f] = 0.0;
            self.cy[f] = 0;
        }
        std::mem::swap(&mut self.prev_x, &mut self.faces_x);
        std::mem::swap(&mut self.prev_y, &mut self.faces_y);
        self.faces_x.clear();
        self.faces_y.clear();
        let mut span = vec![(u32::MAX, 0u32); n];
        for &c in &self.written {
            let (x, y) = (c as usize % n, c as usize / n);
            span[y].0 = span[y].0.min(x as u32);
            span[y].1 = span[y].1.max(x as u32 + 1);
        }
        for (y, &(a, b)) in span.iter().enumerate() {
            let row = &mut w.rows[y];
            let mut run: Option<u32> = None;
            for x in a..b.max(a) {
                let wet = w.h[y * n + x as usize] > 0.0;
                match (wet, run) {
                    (true, None) => run = Some(x),
                    (false, Some(s)) => {
                        row.push((s, x));
                        run = None;
                    }
                    _ => {}
                }
            }
            if let Some(s) = run {
                row.push((s, b));
            }
        }
    }

    fn dt_last(&self) -> f64 {
        self.last_dt.max(1.0e-9)
    }
}

impl super::WaterEngine for Equilibrium {
    fn prepare(&mut self, w: &mut Water, sync: bool) { Equilibrium::prepare(self, w, sync) }
    fn advance(&mut self, w: &mut Water, seconds: f64) { Equilibrium::advance(self, w, seconds) }
    fn environment(&mut self, w: &mut Water, days: f64) { Equilibrium::environment(self, w, days) }
    fn publish(&mut self, w: &mut Water) { Equilibrium::publish(self, w) }
    fn total_volume(&self) -> Option<f64> { self.version.map(|_| self.total()) }
}
