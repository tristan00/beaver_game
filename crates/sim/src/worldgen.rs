//! Deterministic world generation from a seed.
//!
//! Pipeline: valley layout (one start valley ringed by neighbours) -> terrain with ridges and passes
//! -> priority-flood depression filling (strictly descending drainage) -> lakes in the hollows ->
//! flow accumulation / rivers / springs -> soil moisture -> forest species.
//!
//! Contracts (all verified by the tests below):
//! * same params => identical [`World`];
//! * every river cell drains to a strictly lower `surface` cell, ending at the map edge;
//! * every valley is walkable-reachable from the start valley (through passes);
//! * only the start valley is open; neighbours open via [`World::unlock`];
//! * every world has one main river, tributary streams that join it, springs at the stream heads
//!   and still ponds (hollows no river feeds); river cells sit in a carved channel.
use crate::grid::{Grid, HeightGrid};
use rand_core::{Rng, SeedableRng};
use rand_pcg::Pcg32;
use std::cmp::Reverse;
use std::collections::{BinaryHeap, VecDeque};
use std::fmt;

/// Cells with at least this many upstream cells (incl. itself) are river.
pub const RIVER_MIN_ACCUMULATION: u32 = 40;
/// Smallest lake depth (world units) that counts as a lake rather than a filled pit.
const LAKE_MIN_DEPTH: f32 = 0.5;
/// Smallest lake area in cells.
const LAKE_MIN_CELLS: usize = 6;
/// Hollows whose lake core exceeds this are filled into floodplain instead (keeps valleys habitable).
const LAKE_MAX_CELLS: usize = 120;
/// Largest surface height difference between 4-neighbours that a walker can cross.
pub const MAX_STEP: f32 = 1.5;
/// Cells within this distance of another valley count as "at its border".
pub const BORDER_REACH: usize = 3;
/// Minimum drop per cell along drainage, guarantees strict descent on flats.
const FLOW_EPS: f32 = 1e-3;
/// Channel depth carved below the drainage surface at a river's minimum accumulation, and per
/// doubling of the upstream area beyond it (capped at `CHANNEL_MAX`). Monotone downstream.
pub const CHANNEL_BASE: f32 = 0.30;
const CHANNEL_PER_DOUBLING: f32 = 0.08;
const CHANNEL_MAX: f32 = 0.60;
/// A world needs at least this many tributary cells (river cells that drain into the main river).
const TRIBUTARY_MIN_CELLS: usize = 12;
/// Ponds carved per world (at most; sites are verified, so fewer can result).
const POND_TARGET: usize = 3;
const POND_TRIES: usize = 60;
const POND_DEPTH: f32 = 1.6;
/// Ponds stay this many cells from the start valley's centre.
const POND_START_CLEARANCE: f32 = 20.0;
/// Height kept above zero so carved ponds and channels never go below it.
const CARVE_ROOM: f32 = 2.5;
/// Stream heads whose branch is shorter than this (cells to the junction) are not rivers.
const MIN_BRANCH: usize = 14;
const MIN_SIZE: usize = 64;
const ATTEMPTS: u64 = 16;

/// Identifier of a valley; `World::valleys[id.0]`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ValleyId(pub u16);

/// Tree species placed by moisture and slope.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Tree { Aspen, Willow, Birch, Alder, Pine }

impl Tree {
    pub const ALL: [Tree; 5] = [Tree::Aspen, Tree::Willow, Tree::Birch, Tree::Alder, Tree::Pine];
}

#[derive(Clone, Debug, PartialEq)]
pub struct Valley {
    pub id: ValleyId,
    /// Centre in cell coordinates.
    pub center: (f32, f32),
    pub open: bool,
    pub cells: usize,
}

/// Errors from world generation and valley unlocking.
#[derive(Clone, Debug, PartialEq)]
pub enum WorldError {
    /// `WorldGen::size` below the minimum.
    InvalidSize(usize),
    /// Cell outside the map.
    OutOfBounds((usize, usize)),
    /// The cell lies in a valley that is not open yet.
    ValleyLocked(ValleyId),
    /// No attempt produced a valid world (a generator bug, never expected).
    GenerationFailed { seed: u64, reason: String },
}

impl fmt::Display for WorldError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            WorldError::InvalidSize(s) => write!(f, "invalid world size {s} (minimum {MIN_SIZE})"),
            WorldError::OutOfBounds(c) => write!(f, "cell {c:?} is outside the map"),
            WorldError::ValleyLocked(v) => write!(f, "valley {} is locked", v.0),
            WorldError::GenerationFailed { seed, reason } => {
                write!(f, "world generation failed for seed {seed}: {reason}")
            }
        }
    }
}
impl std::error::Error for WorldError {}

/// A generated region. All grids share `size x size` dimensions.
#[derive(Clone, Debug)]
pub struct World {
    pub seed: u64,
    pub size: usize,
    /// Terrain height; equals `surface` except lake beds and river channels, which are lower.
    pub height: HeightGrid,
    /// Drainage surface (terrain with depressions filled); lakes sit at this level.
    pub surface: HeightGrid,
    pub valley: Grid<ValleyId>,
    pub valleys: Vec<Valley>,
    pub start: ValleyId,
    /// Valley pairs joined by a pass, with the pass cell.
    pub passes: Vec<(ValleyId, ValleyId, (usize, usize))>,
    /// Downstream cell index (`y * size + x`); `None` at the map edge outlets.
    pub flow_to: Grid<Option<usize>>,
    /// Number of cells draining through each cell (itself included).
    pub flow_acc: Grid<u32>,
    /// All river cells (main river and tributaries; other independent streams too).
    pub river: Grid<bool>,
    /// The main river: the largest river, from its map-edge outlet up to its source.
    pub main_river: Grid<bool>,
    /// Tributary stream cells: river cells (not main) whose drainage joins the main river.
    pub tributary: Grid<bool>,
    /// Main-river cells where a tributary joins.
    pub confluences: Vec<(usize, usize)>,
    pub lake: Grid<bool>,
    /// Still ponds: lake cells whose hollow is not fed by a river (a subset of `lake`).
    pub pond: Grid<bool>,
    /// Stream heads: where water enters the network.
    pub springs: Vec<(usize, usize)>,
    /// Soil moisture 0..=1.
    pub moisture: Grid<f32>,
    /// Surface gradient magnitude, world units per cell.
    pub slope: Grid<f32>,
    pub forest: Grid<Option<Tree>>,
}

impl World {
    /// Valley containing `cell` (x, y), or `None` outside the map.
    pub fn valley_of(&self, cell: (usize, usize)) -> Option<ValleyId> {
        self.valley.get(cell.0, cell.1).copied()
    }

    pub fn is_open(&self, id: ValleyId) -> bool {
        self.valleys.get(id.0 as usize).is_some_and(|v| v.open)
    }

    /// The colony reached `cell`. Opens every locked valley whose border is within
    /// [`BORDER_REACH`] cells and returns the newly opened ids (possibly empty).
    /// Errors if `cell` is outside the map or inside a locked valley.
    pub fn unlock(&mut self, cell: (usize, usize)) -> Result<Vec<ValleyId>, WorldError> {
        let own = self.valley_of(cell).ok_or(WorldError::OutOfBounds(cell))?;
        if !self.is_open(own) {
            return Err(WorldError::ValleyLocked(own));
        }
        let (x0, x1) = (cell.0.saturating_sub(BORDER_REACH), (cell.0 + BORDER_REACH).min(self.size - 1));
        let (y0, y1) = (cell.1.saturating_sub(BORDER_REACH), (cell.1 + BORDER_REACH).min(self.size - 1));
        let mut opened = Vec::new();
        for y in y0..=y1 {
            for x in x0..=x1 {
                let v = *self.valley.get(x, y).unwrap();
                if !self.valleys[v.0 as usize].open {
                    self.valleys[v.0 as usize].open = true;
                    opened.push(v);
                }
            }
        }
        Ok(opened)
    }

    /// Downstream cell of `(x, y)`, or `None` at an outlet / out of bounds.
    pub fn downstream(&self, x: usize, y: usize) -> Option<(usize, usize)> {
        self.flow_to.get(x, y).copied().flatten().map(|i| (i % self.size, i / self.size))
    }

    /// Valleys reachable on foot from `from` (4-neighbour moves on non-lake cells, steps <= [`MAX_STEP`]).
    pub fn reachable_valleys(&self, from: ValleyId) -> Vec<ValleyId> {
        let n = self.size;
        let (s, lake, val) = (self.surface.as_slice(), self.lake.as_slice(), self.valley.as_slice());
        let Some(seed) = (0..n * n).find(|&i| val[i] == from && !lake[i]) else { return vec![] };
        let mut seen = vec![false; n * n];
        let mut q = VecDeque::from([seed]);
        seen[seed] = true;
        let mut found = vec![false; self.valleys.len()];
        while let Some(i) = q.pop_front() {
            found[val[i].0 as usize] = true;
            for j in neighbours4(i, n) {
                if !seen[j] && !lake[j] && (s[j] - s[i]).abs() <= MAX_STEP {
                    seen[j] = true;
                    q.push_back(j);
                }
            }
        }
        found.iter().enumerate().filter(|(_, f)| **f).map(|(i, _)| ValleyId(i as u16)).collect()
    }
}

/// World generation parameters.
#[derive(Clone, Debug)]
pub struct WorldGen {
    pub seed: u64,
    /// Grid side length in cells (>= 64).
    pub size: usize,
    /// Peak-to-valley height range in world units.
    pub amplitude: f32,
}

impl WorldGen {
    pub const DEFAULT_SIZE: usize = 128;
    pub fn new(seed: u64) -> Self { Self { seed, size: Self::DEFAULT_SIZE, amplitude: 24.0 } }

    /// Height grid of the generated world. Panics only if `generate` fails (invalid `size`).
    pub fn heightmap(&self) -> HeightGrid {
        self.generate().expect("world generation failed").height
    }

    /// Generates the world; identical for identical params.
    pub fn generate(&self) -> Result<World, WorldError> {
        if self.size < MIN_SIZE {
            return Err(WorldError::InvalidSize(self.size));
        }
        let mut last = String::new();
        for attempt in 0..ATTEMPTS {
            let s = self.seed.wrapping_add(attempt.wrapping_mul(0x9E37_79B9_7F4A_7C15));
            match build(self, s) {
                Ok(w) => return Ok(w),
                Err(reason) => last = reason,
            }
        }
        Err(WorldError::GenerationFailed { seed: self.seed, reason: last })
    }
}

// ---------------------------------------------------------------- noise

/// Square lattice of random values in 0..1, sampled with smooth interpolation.
struct Lattice { cells: usize, vals: Vec<f32> }

impl Lattice {
    fn new(seed: u64, cells: usize) -> Self {
        let mut rng = Pcg32::seed_from_u64(seed);
        let side = cells + 1;
        let vals = (0..side * side).map(|_| unit(&mut rng)).collect();
        Self { cells, vals }
    }

    fn sample(&self, u: f32, v: f32) -> f32 {
        let side = self.cells + 1;
        let (fx, fy) = (u.max(0.0) * self.cells as f32, v.max(0.0) * self.cells as f32);
        let (x0, y0) = (fx.floor() as usize, fy.floor() as usize);
        let (tx, ty) = (smooth(fx - x0 as f32), smooth(fy - y0 as f32));
        let at = |x: usize, y: usize| self.vals[y.min(self.cells) * side + x.min(self.cells)];
        let top = at(x0, y0) + (at(x0 + 1, y0) - at(x0, y0)) * tx;
        let bot = at(x0, y0 + 1) + (at(x0 + 1, y0 + 1) - at(x0, y0 + 1)) * tx;
        top + (bot - top) * ty
    }
}

/// Fractal value noise in 0..1; each octave doubles frequency and halves amplitude.
struct Fbm(Vec<Lattice>);

impl Fbm {
    fn new(seed: u64, base_cells: usize, octaves: u32) -> Self {
        Fbm((0..octaves).map(|o| Lattice::new(seed.wrapping_add(o as u64 * 0x9E37_79B9), base_cells << o)).collect())
    }
    fn sample(&self, u: f32, v: f32) -> f32 {
        let (mut sum, mut amp, mut norm) = (0.0, 1.0, 0.0);
        for l in &self.0 {
            sum += amp * l.sample(u, v);
            norm += amp;
            amp *= 0.5;
        }
        sum / norm
    }
}

fn unit(rng: &mut Pcg32) -> f32 { (rng.next_u32() >> 8) as f32 / (1u32 << 24) as f32 }
fn smooth(t: f32) -> f32 { t * t * (3.0 - 2.0 * t) }

// ---------------------------------------------------------------- grid helpers

fn neighbours4(i: usize, n: usize) -> impl Iterator<Item = usize> {
    let (x, y) = (i % n, i / n);
    [(x > 0).then(|| i - 1), (x + 1 < n).then(|| i + 1), (y > 0).then(|| i - n), (y + 1 < n).then(|| i + n)]
        .into_iter()
        .flatten()
}

fn neighbours8(i: usize, n: usize) -> impl Iterator<Item = usize> {
    let (x, y) = ((i % n) as isize, (i / n) as isize);
    let m = n as isize;
    (-1isize..=1).flat_map(move |dy| (-1isize..=1).map(move |dx| (dx, dy))).filter_map(move |(dx, dy)| {
        let (nx, ny) = (x + dx, y + dy);
        ((dx, dy) != (0, 0) && nx >= 0 && ny >= 0 && nx < m && ny < m).then(|| (ny * m + nx) as usize)
    })
}

fn grid_from<T: Clone>(n: usize, data: Vec<T>) -> Grid<T> {
    let mut g = Grid::new(n, n, data[0].clone());
    g.as_mut_slice().clone_from_slice(&data);
    g
}

// ---------------------------------------------------------------- generation

/// Valley layout: labels plus the passes that connect valleys.
struct Layout {
    centers: Vec<(f32, f32)>,
    label: Vec<u16>,
    /// Distance gap between the two nearest valleys, in cells (0 on the boundary).
    gap: Vec<f32>,
    passes: Vec<(ValleyId, ValleyId, (usize, usize))>,
}

fn layout(seed: u64, n: usize) -> Layout {
    let mut rng = Pcg32::seed_from_u64(seed ^ 0xA11CE);
    let nf = n as f32;
    let mut centers = vec![(0.5 * nf + (unit(&mut rng) - 0.5) * 0.04 * nf, 0.5 * nf + (unit(&mut rng) - 0.5) * 0.04 * nf)];
    let a0 = unit(&mut rng) * std::f32::consts::TAU;
    for k in 0..5 {
        let a = a0 + k as f32 * std::f32::consts::TAU / 5.0 + (unit(&mut rng) - 0.5) * 0.4;
        let r = (0.40 + (unit(&mut rng) - 0.5) * 0.06) * nf;
        centers.push((0.5 * nf + r * a.cos(), 0.5 * nf + r * a.sin()));
    }
    let warp = (Fbm::new(seed ^ 0x57A2, 3, 3), Fbm::new(seed ^ 0x9B3C, 3, 3));
    let (mut label, mut gap) = (vec![0u16; n * n], vec![0f32; n * n]);
    for y in 0..n {
        for x in 0..n {
            let (u, v) = (x as f32 / nf, y as f32 / nf);
            let px = x as f32 + (warp.0.sample(u, v) - 0.5) * 0.18 * nf;
            let py = y as f32 + (warp.1.sample(u, v) - 0.5) * 0.18 * nf;
            let (mut d1, mut d2, mut best) = (f32::MAX, f32::MAX, 0);
            for (i, c) in centers.iter().enumerate() {
                let d = ((px - c.0).powi(2) + (py - c.1).powi(2)).sqrt();
                if d < d1 { d2 = d1; d1 = d; best = i; } else if d < d2 { d2 = d; }
            }
            label[y * n + x] = best as u16;
            gap[y * n + x] = d2 - d1;
        }
    }
    // Adjacent pairs and their boundary cell nearest the midpoint of the two centres.
    let k = centers.len();
    let mut best: Vec<Option<(f32, usize)>> = vec![None; k * k];
    for i in 0..n * n {
        for j in neighbours4(i, n) {
            let (a, b) = (label[i] as usize, label[j] as usize);
            if a < b {
                let mid = ((centers[a].0 + centers[b].0) / 2.0, (centers[a].1 + centers[b].1) / 2.0);
                let d = ((i % n) as f32 - mid.0).powi(2) + ((i / n) as f32 - mid.1).powi(2);
                if best[a * k + b].is_none_or(|(bd, _)| d < bd) { best[a * k + b] = Some((d, i)); }
            }
        }
    }
    // Random spanning tree from the start valley: every valley gets a route in.
    let mut in_tree = vec![false; k];
    in_tree[0] = true;
    let mut passes = Vec::new();
    while passes.len() + 1 < k {
        let mut cands = Vec::new();
        for a in 0..k { for b in a + 1..k {
            if let Some((_, cell)) = best[a * k + b] {
                if in_tree[a] != in_tree[b] { cands.push((a, b, cell)); }
            }
        } }
        if cands.is_empty() { break; } // disconnected layout; validation rejects it
        let (a, b, cell) = cands[(rng.next_u32() as usize) % cands.len()];
        in_tree[a] = true;
        in_tree[b] = true;
        passes.push((ValleyId(a as u16), ValleyId(b as u16), (cell % n, cell / n)));
    }
    Layout { centers, label, gap, passes }
}

/// Raw terrain in 0..amplitude*0.95: tilted toward the south, ridges on valley borders except at passes.
fn terrain(seed: u64, n: usize, amplitude: f32, lay: &Layout) -> Vec<f32> {
    let nf = n as f32;
    let detail = Fbm::new(seed ^ 0xD37A, 4, 5);
    let offsets: Vec<f32> = {
        let mut rng = Pcg32::seed_from_u64(seed ^ 0x0FF5);
        lay.centers.iter().map(|_| (unit(&mut rng) - 0.5) * 0.08).collect()
    };
    let (ridge_w, pass_sigma) = (0.11 * nf, 0.04 * nf);
    let mut h = vec![0f32; n * n];
    for y in 0..n {
        for x in 0..n {
            let i = y * n + x;
            let (u, v) = (x as f32 / nf, y as f32 / nf);
            let ridge = 1.0 - smooth((lay.gap[i] / ridge_w).clamp(0.0, 1.0));
            let pass = lay.passes.iter().map(|p| {
                let d2 = (x as f32 - p.2.0 as f32).powi(2) + (y as f32 - p.2.1 as f32).powi(2);
                (-d2 / (2.0 * pass_sigma * pass_sigma)).exp()
            }).fold(0.0, f32::max);
            h[i] = 0.30 * (1.0 - v) + offsets[lay.label[i] as usize]
                + 0.55 * ridge * (1.0 - 0.95 * pass)
                + 0.12 * detail.sample(u, v);
        }
    }
    let (lo, hi) = h.iter().fold((f32::MAX, f32::MIN), |(l, g), &x| (l.min(x), g.max(x)));
    // Floor offset leaves room for the ponds and channels carved later.
    h.iter_mut().for_each(|x| *x = CARVE_ROOM + (*x - lo) / (hi - lo) * (amplitude * 0.95 - CARVE_ROOM));
    h
}

/// Priority-flood with an epsilon (Barnes et al.): returns the filled surface and, for each
/// cell, the neighbour it drains to. Surface strictly decreases along every drainage path.
fn fill_depressions(h: &[f32], n: usize) -> (Vec<f32>, Vec<Option<usize>>) {
    let mut surface = h.to_vec();
    let mut flow = vec![None; n * n];
    let mut done = vec![false; n * n];
    let mut heap = BinaryHeap::new();
    for i in 0..n * n {
        let (x, y) = (i % n, i / n);
        if x == 0 || y == 0 || x == n - 1 || y == n - 1 {
            done[i] = true;
            heap.push(Reverse((surface[i].to_bits(), i))); // heights are >= 0, so bits order like values
        }
    }
    while let Some(Reverse((_, c))) = heap.pop() {
        for j in neighbours8(c, n) {
            if !done[j] {
                done[j] = true;
                surface[j] = h[j].max(surface[c] + FLOW_EPS);
                flow[j] = Some(c);
                heap.push(Reverse((surface[j].to_bits(), j)));
            }
        }
    }
    (surface, flow)
}

/// Lake mask: filled pits at least `LAKE_MIN_DEPTH` deep forming components of `LAKE_MIN_CELLS`+ cells.
fn find_lakes(h: &[f32], surface: &[f32], n: usize) -> Vec<bool> {
    let cand: Vec<bool> = (0..n * n).map(|i| surface[i] - h[i] >= LAKE_MIN_DEPTH).collect();
    let (mut lake, mut seen) = (vec![false; n * n], vec![false; n * n]);
    for s in 0..n * n {
        if !cand[s] || seen[s] { continue; }
        let (mut comp, mut q) = (vec![s], VecDeque::from([s]));
        seen[s] = true;
        while let Some(i) = q.pop_front() {
            for j in neighbours4(i, n) {
                if cand[j] && !seen[j] { seen[j] = true; comp.push(j); q.push_back(j); }
            }
        }
        // Only the deep core of a big hollow stays lake; its shallow rim becomes marshy plain.
        let max_depth = comp.iter().map(|&i| surface[i] - h[i]).fold(0.0, f32::max);
        comp.retain(|&i| surface[i] - h[i] >= 0.45 * max_depth);
        if (LAKE_MIN_CELLS..=LAKE_MAX_CELLS).contains(&comp.len()) { comp.iter().for_each(|&i| lake[i] = true); }
    }
    lake
}

/// Flow accumulation (upstream cell count, itself included) for a drainage forest. Upstream cells
/// are strictly higher on `surface`, so descending order visits them first.
fn accumulate(surface: &[f32], flow: &[Option<usize>]) -> Vec<u32> {
    let mut order: Vec<usize> = (0..surface.len()).collect();
    order.sort_by(|&a, &b| surface[b].total_cmp(&surface[a]).then(a.cmp(&b)));
    let mut acc = vec![1u32; surface.len()];
    for &i in &order {
        if let Some(t) = flow[i] { acc[t] += acc[i]; }
    }
    acc
}

/// Carves up to `POND_TARGET` bowls into `raw` at flat sites away from rivers. A bowl is kept only
/// if depression filling turns it into a small lake no river feeds; otherwise it is undone.
fn carve_ponds(raw: &mut [f32], n: usize, seed: u64, start_center: (f32, f32)) {
    let mut rng = Pcg32::seed_from_u64(seed ^ 0x90ED);
    let (surface, flow) = fill_depressions(raw, n);
    let acc = accumulate(&surface, &flow);
    let mut near_river = vec![u32::MAX; n * n];
    let mut q: VecDeque<usize> = (0..n * n).filter(|&i| acc[i] >= RIVER_MIN_ACCUMULATION / 2).collect();
    q.iter().for_each(|&i| near_river[i] = 0);
    while let Some(i) = q.pop_front() {
        for j in neighbours4(i, n) {
            if near_river[j] == u32::MAX { near_river[j] = near_river[i] + 1; q.push_back(j); }
        }
    }
    let radius = 3.0f32;
    let mut placed: Vec<(usize, usize)> = Vec::new();
    for t in 0..POND_TRIES {
        if placed.len() >= POND_TARGET { break; }
        // The first pond is tried near the start valley's centre, the rest anywhere.
        let (cx, cy) = if t < POND_TRIES / 3 && placed.is_empty() {
            let (jx, jy) = ((unit(&mut rng) - 0.5) * 0.30 * n as f32, (unit(&mut rng) - 0.5) * 0.30 * n as f32);
            ((start_center.0 + jx).clamp(10.0, n as f32 - 11.0) as usize, (start_center.1 + jy).clamp(10.0, n as f32 - 11.0) as usize)
        } else {
            (10 + (rng.next_u32() as usize) % (n - 20), 10 + (rng.next_u32() as usize) % (n - 20))
        };
        let c = cy * n + cx;
        // Keep the colony's own ground dry.
        let (dx, dy) = (cx as f32 - start_center.0, cy as f32 - start_center.1);
        if dx * dx + dy * dy < POND_START_CLEARANCE * POND_START_CLEARANCE { continue; }
        if near_river[c] < 7 || placed.iter().any(|&(px, py)| px.abs_diff(cx) + py.abs_diff(cy) < 16) { continue; }
        let saved = raw.to_vec();
        let r = radius.ceil() as isize;
        for dy in -r..=r { for dx in -r..=r {
            let d2 = (dx * dx + dy * dy) as f32;
            if d2 <= radius * radius {
                let i = ((cy as isize + dy) as usize) * n + (cx as isize + dx) as usize;
                raw[i] -= POND_DEPTH * (1.0 - d2 / (radius * radius + 1.0));
            }
        } }
        let (s2, f2) = fill_depressions(raw, n);
        let a2 = accumulate(&s2, &f2);
        let lake = find_lakes(raw, &s2, n);
        let hollow = lake_components(&lake, n).into_iter().find(|comp| comp.contains(&c));
        let ok = hollow.is_some_and(|comp| {
            (8..=60).contains(&comp.len()) && comp.iter().all(|&i| a2[i] < RIVER_MIN_ACCUMULATION / 2)
        });
        if ok { placed.push((cx, cy)); } else { raw.copy_from_slice(&saved); }
    }
}

/// 4-connected components of a mask.
fn lake_components(mask: &[bool], n: usize) -> Vec<Vec<usize>> {
    let mut seen = vec![false; n * n];
    let mut out = Vec::new();
    for s in 0..n * n {
        if !mask[s] || seen[s] { continue; }
        let (mut comp, mut q) = (vec![s], VecDeque::from([s]));
        seen[s] = true;
        while let Some(i) = q.pop_front() {
            for j in neighbours4(i, n) {
                if mask[j] && !seen[j] { seen[j] = true; comp.push(j); q.push_back(j); }
            }
        }
        out.push(comp);
    }
    out
}

/// Main river, tributaries and confluences of the river network.
struct Streams { main: Vec<bool>, trib: Vec<bool>, confluences: Vec<usize> }

fn classify_streams(surface: &[f32], flow: &[Option<usize>], acc: &[u32], river: &[bool]) -> Streams {
    let len = surface.len();
    let mut main = vec![false; len];
    // Main river: from the biggest outlet river cell, always up the larger branch.
    if let Some(outlet) = (0..len).filter(|&i| river[i] && flow[i].is_none()).max_by_key(|&i| (acc[i], Reverse(i))) {
        let mut up: Vec<Vec<usize>> = vec![Vec::new(); len];
        for i in (0..len).filter(|&i| river[i]) {
            if let Some(t) = flow[i] { up[t].push(i); }
        }
        let mut cur = outlet;
        loop {
            main[cur] = true;
            match up[cur].iter().copied().max_by_key(|&j| (acc[j], Reverse(j))) {
                Some(j) => cur = j,
                None => break,
            }
        }
    }
    // Downstream cells first: a river cell is tributary if what it drains into is main or tributary.
    let mut order: Vec<usize> = (0..len).collect();
    order.sort_by(|&a, &b| surface[a].total_cmp(&surface[b]).then(a.cmp(&b)));
    let mut trib = vec![false; len];
    let mut confluences = Vec::new();
    for &i in &order {
        if let (true, false, Some(t)) = (river[i], main[i], flow[i]) {
            if main[t] || trib[t] {
                trib[i] = true;
                if main[t] && !confluences.contains(&t) { confluences.push(t); }
            }
        }
    }
    confluences.sort_unstable();
    Streams { main, trib, confluences }
}

/// Carves river channels into `height`: `CHANNEL_BASE` below the drainage surface, deeper with
/// upstream area. Diagonal drainage steps also lower the lower corner cell so the channel stays
/// 4-connected for the water sim. Heights keep strictly descending along the river.
fn carve_channels(height: &mut [f32], surface: &[f32], flow: &[Option<usize>], acc: &[u32], river: &[bool], n: usize) {
    let depth = |i: usize| {
        (CHANNEL_BASE + CHANNEL_PER_DOUBLING * (acc[i] as f32 / RIVER_MIN_ACCUMULATION as f32).log2().max(0.0)).min(CHANNEL_MAX)
    };
    for i in (0..n * n).filter(|&i| river[i]) { height[i] = surface[i] - depth(i); }
    for i in (0..n * n).filter(|&i| river[i]) {
        let Some(t) = flow[i] else { continue };
        let ((x, y), (tx, ty)) = ((i % n, i / n), (t % n, t / n));
        if x != tx && y != ty {
            let corner = [y * n + tx, ty * n + x].into_iter().min_by(|&a, &b| surface[a].total_cmp(&surface[b]).then(a.cmp(&b))).unwrap();
            if !river[corner] {
                let target = 0.5 * (height[i] + if river[t] { height[t] } else { surface[t] });
                height[corner] = height[corner].min(target);
            }
        }
    }
}

/// Removes stream branches shorter than `MIN_BRANCH` cells that end at a junction, repeatedly, so
/// every remaining head is the source of a real stream. Only upstream cells go, so every river
/// cell still drains through river cells.
fn prune_short_branches(river: &mut [bool], flow: &[Option<usize>]) {
    loop {
        let mut up = vec![0u8; river.len()];
        for i in (0..river.len()).filter(|&i| river[i]) {
            if let Some(t) = flow[i] { up[t] = up[t].saturating_add(1); }
        }
        let mut changed = false;
        let heads: Vec<usize> = (0..river.len()).filter(|&i| river[i] && up[i] == 0).collect();
        for head in heads {
            let mut branch = vec![head];
            let mut cur = head;
            let joins = loop {
                match flow[cur].filter(|&t| river[t]) {
                    None => break false,
                    Some(t) if up[t] >= 2 => break true,
                    Some(t) => { branch.push(t); cur = t; }
                }
            };
            if joins && branch.len() < MIN_BRANCH {
                branch.iter().for_each(|&i| river[i] = false);
                changed = true;
            }
        }
        if !changed { return; }
    }
}

fn classify_tree(moisture: f32, slope: f32, height_frac: f32) -> Option<Tree> {
    if height_frac > 0.8 || slope > 1.2 { return None; } // bare rock above the tree line / cliffs
    match (moisture, slope) {
        (m, s) if m > 0.80 && s < 0.3 => Some(Tree::Willow),
        (m, s) if m > 0.62 && s < 0.5 => Some(Tree::Alder),
        (m, s) if m > 0.46 && s < 0.7 => Some(Tree::Aspen),
        (m, s) if m > 0.32 && s < 0.8 => Some(Tree::Birch),
        (m, _) if m > 0.12 => Some(Tree::Pine),
        _ => None,
    }
}

fn build(p: &WorldGen, seed: u64) -> Result<World, String> {
    let n = p.size;
    let nf = n as f32;
    let lay = layout(seed, n);
    let mut raw = terrain(seed, n, p.amplitude, &lay);
    carve_ponds(&mut raw, n, seed, lay.centers[0]);
    let (surface, flow) = fill_depressions(&raw, n);
    let lake = find_lakes(&raw, &surface, n);
    let mut height: Vec<f32> = (0..n * n).map(|i| if lake[i] { raw[i] } else { surface[i] }).collect();

    let acc = accumulate(&surface, &flow);
    let mut river: Vec<bool> = (0..n * n).map(|i| acc[i] >= RIVER_MIN_ACCUMULATION && !lake[i]).collect();
    prune_short_branches(&mut river, &flow);
    let streams = classify_streams(&surface, &flow, &acc, &river);
    let mut pond = vec![false; n * n];
    for comp in lake_components(&lake, n) {
        if comp.iter().all(|&i| acc[i] < RIVER_MIN_ACCUMULATION) { comp.iter().for_each(|&i| pond[i] = true); }
    }
    carve_channels(&mut height, &surface, &flow, &acc, &river, n);
    let mut has_upstream = vec![false; n * n];
    for i in 0..n * n {
        if let (true, Some(t)) = (river[i], flow[i]) { has_upstream[t] = true; }
    }
    let springs: Vec<(usize, usize)> =
        (0..n * n).filter(|&i| river[i] && !has_upstream[i]).map(|i| (i % n, i / n)).collect();

    // Slope of the drainage surface.
    let at = |x: isize, y: isize| surface[(y.clamp(0, n as isize - 1) as usize) * n + x.clamp(0, n as isize - 1) as usize];
    let slope: Vec<f32> = (0..n * n).map(|i| {
        let (x, y) = ((i % n) as isize, (i / n) as isize);
        let (gx, gy) = ((at(x + 1, y) - at(x - 1, y)) / 2.0, (at(x, y + 1) - at(x, y - 1)) / 2.0);
        (gx * gx + gy * gy).sqrt()
    }).collect();

    // Distance to the nearest water (4-neighbour BFS).
    let mut dist = vec![u32::MAX; n * n];
    let mut q: VecDeque<usize> = (0..n * n).filter(|&i| river[i] || lake[i]).collect();
    q.iter().for_each(|&i| dist[i] = 0);
    while let Some(i) = q.pop_front() {
        for j in neighbours4(i, n) {
            if dist[j] == u32::MAX { dist[j] = dist[i] + 1; q.push_back(j); }
        }
    }

    let (wet, tree_noise) = (Fbm::new(seed ^ 0x3E7, 5, 4), Fbm::new(seed ^ 0x7233, 16, 3));
    let (mut moisture, mut forest) = (vec![0f32; n * n], vec![None; n * n]);
    for i in 0..n * n {
        let (u, v) = ((i % n) as f32 / nf, (i / n) as f32 / nf);
        let prox = if dist[i] == u32::MAX { 0.0 } else { (-(dist[i] as f32) / 6.0).exp() };
        let hf = surface[i] / p.amplitude;
        let m = if lake[i] { 1.0 } else {
            (0.30 + 0.55 * prox + 0.5 * (wet.sample(u, v) - 0.5) - 0.25 * hf - 0.25 * slope[i].min(1.0)).clamp(0.0, 1.0)
        };
        moisture[i] = m;
        if !lake[i] && !river[i] && tree_noise.sample(u, v) + 0.3 * (m - 0.5) > 0.45 {
            forest[i] = classify_tree(m, slope[i], hf);
        }
    }

    let mut valleys: Vec<Valley> = lay.centers.iter().enumerate()
        .map(|(i, &center)| Valley { id: ValleyId(i as u16), center, open: i == 0, cells: 0 }).collect();
    lay.label.iter().for_each(|&l| valleys[l as usize].cells += 1);
    let valley = grid_from(n, lay.label.iter().map(|&l| ValleyId(l)).collect());

    let world = World {
        seed: p.seed, size: n,
        height: grid_from(n, height), surface: grid_from(n, surface),
        valley, valleys, start: ValleyId(0), passes: lay.passes,
        flow_to: grid_from(n, flow), flow_acc: grid_from(n, acc),
        river: grid_from(n, river), main_river: grid_from(n, streams.main), tributary: grid_from(n, streams.trib),
        confluences: streams.confluences.iter().map(|&i| (i % n, i / n)).collect(),
        lake: grid_from(n, lake), pond: grid_from(n, pond), springs,
        moisture: grid_from(n, moisture), slope: grid_from(n, slope), forest: grid_from(n, forest),
    };
    validate(&world)?;
    Ok(world)
}

/// Rejects worlds that break the contracts a colony needs.
fn validate(w: &World) -> Result<(), String> {
    if w.passes.len() + 1 != w.valleys.len() {
        return Err("valley layout is not connected".into());
    }
    if w.valleys.iter().any(|v| v.cells == 0) {
        return Err("a valley has no cells".into());
    }
    let reach = w.reachable_valleys(w.start);
    if reach.len() != w.valleys.len() {
        return Err(format!("only {} of {} valleys reachable", reach.len(), w.valleys.len()));
    }
    let trib = w.tributary.as_slice().iter().filter(|&&t| t).count();
    if trib < TRIBUTARY_MIN_CELLS || w.confluences.is_empty() {
        return Err(format!("only {trib} tributary cells"));
    }
    if !w.pond.as_slice().iter().any(|&p| p) {
        return Err("no still pond".into());
    }
    let river_in_start = (0..w.size * w.size)
        .filter(|&i| w.river.as_slice()[i] && w.valley.as_slice()[i] == w.start).count();
    if river_in_start < 20 {
        return Err(format!("start valley has only {river_in_start} river cells"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn world(seed: u64) -> World { WorldGen::new(seed).generate().unwrap() }
    const SEEDS: std::ops::Range<u64> = 0..24;

    #[test]
    fn same_seed_same_world() {
        let (a, b) = (world(7), world(7));
        assert_eq!(a.height, b.height);
        assert_eq!(a.surface, b.surface);
        assert_eq!(a.valley, b.valley);
        assert_eq!(a.river, b.river);
        assert_eq!(a.forest, b.forest);
        assert_eq!(a.moisture, b.moisture);
        assert_eq!(a.springs, b.springs);
    }

    #[test]
    fn same_seed_same_map() {
        assert_eq!(WorldGen::new(7).heightmap(), WorldGen::new(7).heightmap());
    }

    #[test]
    fn different_seed_differs() {
        assert_ne!(WorldGen::new(7).heightmap(), WorldGen::new(8).heightmap());
    }

    #[test]
    fn heights_within_amplitude() {
        let g = WorldGen::new(1);
        assert!(g.heightmap().as_slice().iter().all(|h| (0.0..=g.amplitude).contains(h)));
    }

    #[test]
    fn rivers_always_descend() {
        for seed in SEEDS {
            let w = world(seed);
            let n = w.size;
            assert!(w.river.as_slice().iter().any(|&r| r), "seed {seed}: no rivers");
            for start in (0..n * n).filter(|&i| w.river.as_slice()[i]) {
                let mut cur = (start % n, start / n);
                let mut steps = 0;
                while let Some(next) = w.downstream(cur.0, cur.1) {
                    let (a, b) = (*w.surface.get(cur.0, cur.1).unwrap(), *w.surface.get(next.0, next.1).unwrap());
                    assert!(b < a, "seed {seed}: surface rises {a} -> {b} at {cur:?}");
                    if !w.lake.get(cur.0, cur.1).unwrap() && !w.lake.get(next.0, next.1).unwrap() {
                        assert!(w.height.get(next.0, next.1) < w.height.get(cur.0, cur.1), "seed {seed}: height not descending");
                    }
                    cur = next;
                    steps += 1;
                    assert!(steps <= n * n, "seed {seed}: drainage loop");
                }
                assert!(cur.0 == 0 || cur.1 == 0 || cur.0 == n - 1 || cur.1 == n - 1, "seed {seed}: river ends inland at {cur:?}");
            }
        }
    }

    #[test]
    fn every_valley_reachable() {
        for seed in SEEDS {
            let w = world(seed);
            assert!(w.valleys.len() >= 4);
            assert_eq!(w.reachable_valleys(w.start).len(), w.valleys.len(), "seed {seed}");
        }
    }

    #[test]
    fn river_runs_through_start_valley_and_on() {
        for seed in SEEDS {
            let w = world(seed);
            let n = w.size;
            let leaves = (0..n * n).any(|i| {
                w.flow_acc.as_slice()[i] >= RIVER_MIN_ACCUMULATION && w.valley.as_slice()[i] == w.start
                    && w.downstream(i % n, i / n).is_some_and(|c| w.valley_of(c) != Some(w.start))
            });
            assert!(leaves, "seed {seed}: no river leaves the start valley");
            assert!(!w.springs.is_empty());
        }
    }

    #[test]
    fn lakes_sit_in_hollows_with_moisture() {
        let lakes = SEEDS.map(world).filter(|w| w.lake.as_slice().iter().any(|&l| l)).count();
        assert!(lakes > 0, "no seed produced a lake");
        for seed in SEEDS {
            let w = world(seed);
            for i in 0..w.size * w.size {
                assert!((0.0..=1.0).contains(&w.moisture.as_slice()[i]));
                if w.lake.as_slice()[i] {
                    assert!(w.height.as_slice()[i] < w.surface.as_slice()[i]);
                    assert_eq!(w.moisture.as_slice()[i], 1.0);
                    assert!(w.forest.as_slice()[i].is_none());
                }
            }
        }
    }

    #[test]
    fn species_follow_moisture() {
        let mut mean = std::collections::HashMap::<Tree, (f32, usize)>::new();
        for seed in 0..6 {
            let w = world(seed);
            for (i, t) in w.forest.as_slice().iter().enumerate() {
                if let Some(t) = t {
                    let e = mean.entry(*t).or_default();
                    e.0 += w.moisture.as_slice()[i];
                    e.1 += 1;
                }
            }
        }
        assert_eq!(mean.len(), 5, "species seen: {:?}", mean.keys());
        let m = |t: Tree| { let (s, c) = mean[&t]; s / c as f32 };
        assert!(m(Tree::Willow) > m(Tree::Alder) && m(Tree::Alder) > m(Tree::Aspen));
        assert!(m(Tree::Aspen) > m(Tree::Birch) && m(Tree::Birch) > m(Tree::Pine));
    }

    #[test]
    fn unlock_opens_neighbours_at_the_border_only() {
        let mut w = world(3);
        let start = w.start;
        assert!(w.valleys.iter().all(|v| v.open == (v.id == start)));
        assert_eq!(w.valley_of((w.size, 0)), None);
        // The valley centre is far from any border.
        let c = w.valleys[0].center;
        assert_eq!(w.unlock((c.0 as usize, c.1 as usize)).unwrap(), vec![]);
        // A pass cell lies on the border of two valleys; the pass from the start valley opens its neighbour.
        let (a, b, cell) = w.passes.iter().copied().find(|p| p.0 == start).unwrap();
        assert!(!w.is_open(b));
        assert!(w.valley_of(cell) == Some(a) || w.valley_of(cell) == Some(b));
        if w.valley_of(cell) == Some(b) {
            // Cell belongs to b (still locked): step back into a via the open side is not possible
            // generically, so open from the nearest start-valley cell.
            let n = w.size;
            let near = (0..n * n).filter(|&i| w.valley.as_slice()[i] == a)
                .min_by_key(|&i| (i % n).abs_diff(cell.0).pow(2) + (i / n).abs_diff(cell.1).pow(2)).unwrap();
            assert!(w.unlock((near % n, near / n)).unwrap().contains(&b));
        } else {
            assert!(w.unlock(cell).unwrap().contains(&b));
        }
        assert!(w.is_open(b));
        // Locked / invalid cells are errors.
        let locked = w.valleys.iter().find(|v| !v.open).map(|v| v.center).unwrap();
        assert!(matches!(w.unlock((locked.0 as usize, locked.1 as usize)), Err(WorldError::ValleyLocked(_))));
        assert_eq!(w.unlock((w.size, 0)), Err(WorldError::OutOfBounds((w.size, 0))));
    }

    #[test]
    fn every_seed_has_river_tributary_springs_and_varied_layout() {
        let (mut with_spring, mut with_pond) = (0, 0);
        let mut layouts = std::collections::HashSet::new();
        for seed in 1..=20u64 {
            let w = world(seed);
            let count = |g: &Grid<bool>| g.as_slice().iter().filter(|&&b| b).count();
            assert!(count(&w.main_river) >= 20, "seed {seed}: main river only {} cells", count(&w.main_river));
            assert!(count(&w.tributary) >= TRIBUTARY_MIN_CELLS, "seed {seed}: no tributary");
            assert!(!w.confluences.is_empty(), "seed {seed}: tributary never joins the main river");
            // Tributaries and the main river are disjoint river cells; confluences lie on the main river.
            for i in 0..w.size * w.size {
                let (m, t, r) = (w.main_river.as_slice()[i], w.tributary.as_slice()[i], w.river.as_slice()[i]);
                assert!(!(m && t) && (!m || r) && (!t || r), "seed {seed}: inconsistent stream masks at {i}");
            }
            assert!(w.confluences.iter().all(|&(x, y)| *w.main_river.get(x, y).unwrap()));
            // Every spring is the head of a river cell; ponds are lakes.
            with_spring += !w.springs.is_empty() as usize;
            assert!(w.springs.iter().all(|&(x, y)| *w.river.get(x, y).unwrap()));
            let ponds = count(&w.pond);
            with_pond += (ponds > 0) as usize;
            assert!((0..w.size * w.size).all(|i| !w.pond.as_slice()[i] || w.lake.as_slice()[i]));
            assert!(w.main_river.as_slice().iter().zip(w.pond.as_slice()).all(|(&m, &p)| !(m && p)));
            layouts.insert((w.main_river.as_slice().to_vec(), w.tributary.as_slice().to_vec(), w.pond.as_slice().to_vec()));
        }
        assert_eq!(with_spring, 20, "springs missing in some seeds");
        assert_eq!(with_pond, 20, "ponds missing in some seeds");
        assert_eq!(layouts.len(), 20, "two seeds share a water layout");
    }

    #[test]
    fn invalid_size_is_an_error() {
        let mut g = WorldGen::new(1);
        g.size = 8;
        assert_eq!(g.generate().unwrap_err(), WorldError::InvalidSize(8));
    }
}
