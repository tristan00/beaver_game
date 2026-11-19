use crate::grid::{Grid, HeightGrid};
use crate::tuning::water::WATER_PER_CATCHMENT_CELL;
use rand_core::{Rng, SeedableRng};
use rand_pcg::Pcg32;
use std::cmp::Reverse;
use std::collections::{BinaryHeap, VecDeque};
use std::fmt;

const LAKE_MIN_DEPTH: f32 = 0.5;
const LAKE_AREA_M2: std::ops::RangeInclusive<f32> = 50.0..=20_000.0;
pub const MAX_STEP: f32 = 0.6;
const FLOW_EPS: f32 = 1e-3;
const MIN_SIZE: usize = 64;
const MOISTURE_REACH_M: f32 = 15.0;
const TREES_PER_M2: [(u8, f32); 5] = [(10, 0.06), (20, 0.025), (90, 0.012), (30, 0.002), (100, 0.002)];
pub const INFLOW_WALL_M: f32 = 12.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ValleyId(pub u16);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Tree { Aspen, Willow, Birch, Alder, Pine }

impl Tree {
    pub const ALL: [Tree; 5] = [Tree::Aspen, Tree::Willow, Tree::Birch, Tree::Alder, Tree::Pine];
}

#[derive(Clone, Debug, PartialEq)]
pub struct Valley {
    pub id: ValleyId,
    pub center: (f32, f32),
    pub open: bool,
    pub cells: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub enum WorldError {
    InvalidSize(usize),
    OutOfBounds((usize, usize)),
    ValleyLocked(ValleyId),
    GenerationFailed { seed: u64, reason: String },
}

impl fmt::Display for WorldError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            WorldError::InvalidSize(s) => write!(f, "invalid world size {s} (minimum {MIN_SIZE})"),
            WorldError::OutOfBounds(c) => write!(f, "cell {c:?} is outside the map"),
            WorldError::ValleyLocked(v) => write!(f, "valley {} is locked", v.0),
            WorldError::GenerationFailed { seed, reason } => write!(f, "world generation failed for seed {seed}: {reason}"),
        }
    }
}
impl std::error::Error for WorldError {}

#[derive(Clone, Debug)]
pub struct World {
    pub seed: u64,
    pub size: usize,
    pub cell_m: f32,
    pub height: HeightGrid,
    pub surface: HeightGrid,
    pub valley: Grid<ValleyId>,
    pub valleys: Vec<Valley>,
    pub start: ValleyId,
    pub flow_to: Grid<Option<usize>>,
    pub flow_acc: Grid<u32>,
    pub river: Grid<bool>,
    pub main_river: Grid<bool>,
    pub river_flow: Grid<f32>,
    pub lake: Grid<bool>,
    pub pond: Grid<bool>,
    pub water_inputs: Vec<WaterInput>,
    pub moisture: Grid<f32>,
    pub slope: Grid<f32>,
    pub forest: Grid<Option<Tree>>,
}

impl World {
    pub fn valley_of(&self, cell: (usize, usize)) -> Option<ValleyId> {
        self.valley.get(cell.0, cell.1).copied()
    }

    pub fn is_open(&self, id: ValleyId) -> bool {
        self.valleys.get(id.0 as usize).is_some_and(|v| v.open)
    }

    pub fn unlock(&mut self, cell: (usize, usize)) -> Result<Vec<ValleyId>, WorldError> {
        let own = self.valley_of(cell).ok_or(WorldError::OutOfBounds(cell))?;
        if !self.is_open(own) {
            return Err(WorldError::ValleyLocked(own));
        }
        Ok(Vec::new())
    }

    pub fn downstream(&self, x: usize, y: usize) -> Option<(usize, usize)> {
        self.flow_to.get(x, y).copied().flatten().map(|i| (i % self.size, i / self.size))
    }

    pub fn reachable_valleys(&self, from: ValleyId) -> Vec<ValleyId> {
        if self.is_open(from) || (from.0 as usize) < self.valleys.len() { vec![from] } else { Vec::new() }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum InputKind {
    Inflow,
    Baseflow,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WaterInput {
    pub cell: (usize, usize),
    pub catchment: u32,
    pub kind: InputKind,
    pub rate: f32,
}

#[derive(Clone, Copy, Debug)]
pub struct RealMap<'a> {
    pub cell_m: f32,
    pub inflows: &'a [(usize, usize, f32)],
    pub cover: Option<&'a [u8]>,
}

struct Lattice { cells: usize, vals: Vec<f32> }

impl Lattice {
    fn new(seed: u64, cells: usize) -> Self {
        let mut rng = Pcg32::seed_from_u64(seed);
        let side = cells + 1;
        Self { cells, vals: (0..side * side).map(|_| (rng.next_u32() >> 8) as f32 / (1u32 << 24) as f32).collect() }
    }

    fn sample(&self, u: f32, v: f32) -> f32 {
        let side = self.cells + 1;
        let smooth = |t: f32| t * t * (3.0 - 2.0 * t);
        let (fx, fy) = (u.max(0.0) * self.cells as f32, v.max(0.0) * self.cells as f32);
        let (x0, y0) = (fx.floor() as usize, fy.floor() as usize);
        let (tx, ty) = (smooth(fx - x0 as f32), smooth(fy - y0 as f32));
        let at = |x: usize, y: usize| self.vals[y.min(self.cells) * side + x.min(self.cells)];
        let top = at(x0, y0) + (at(x0 + 1, y0) - at(x0, y0)) * tx;
        let bot = at(x0, y0 + 1) + (at(x0 + 1, y0 + 1) - at(x0, y0 + 1)) * tx;
        top + (bot - top) * ty
    }
}

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

fn neighbours4(i: usize, n: usize) -> impl Iterator<Item = usize> {
    let (x, y) = (i % n, i / n);
    [(x > 0).then(|| i - 1), (x + 1 < n).then(|| i + 1), (y > 0).then(|| i - n), (y + 1 < n).then(|| i + n)].into_iter().flatten()
}

fn grid_from<T: Clone>(n: usize, data: Vec<T>) -> Grid<T> {
    let mut g = Grid::new(n, n, data[0].clone());
    g.as_mut_slice().clone_from_slice(&data);
    g
}

fn fill_depressions(h: &[f32], n: usize, closed: &[bool]) -> (Vec<f32>, Vec<Option<usize>>) {
    let mut surface = h.to_vec();
    let mut flow = vec![None; n * n];
    let mut done = vec![false; n * n];
    let mut heap = BinaryHeap::new();
    for i in 0..n * n {
        let (x, y) = (i % n, i / n);
        if (x == 0 || y == 0 || x == n - 1 || y == n - 1) && !closed[i] {
            done[i] = true;
            heap.push(Reverse((surface[i].to_bits(), i)));
        }
    }
    while let Some(Reverse((_, c))) = heap.pop() {
        for j in crate::water::neighbours8(n, c) {
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

fn components(mask: &[bool], n: usize) -> Vec<Vec<usize>> {
    let mut seen = vec![false; n * n];
    let mut out = Vec::new();
    for s in (0..n * n).filter(|&s| mask[s]) {
        if seen[s] {
            continue;
        }
        let (mut comp, mut q) = (vec![s], VecDeque::from([s]));
        seen[s] = true;
        while let Some(i) = q.pop_front() {
            for j in neighbours4(i, n) {
                if mask[j] && !seen[j] {
                    seen[j] = true;
                    comp.push(j);
                    q.push_back(j);
                }
            }
        }
        out.push(comp);
    }
    out
}

fn find_lakes(h: &[f32], surface: &[f32], n: usize, cell_m: f32) -> Vec<bool> {
    let deep: Vec<bool> = (0..n * n).map(|i| surface[i] - h[i] >= LAKE_MIN_DEPTH).collect();
    let mut lake = vec![false; n * n];
    for comp in components(&deep, n) {
        if LAKE_AREA_M2.contains(&(comp.len() as f32 * cell_m * cell_m)) {
            comp.iter().for_each(|&i| lake[i] = true);
        }
    }
    lake
}

fn accumulate(order: &[usize], flow: &[Option<usize>]) -> Vec<u32> {
    let mut acc = vec![1u32; flow.len()];
    for &i in order {
        if let Some(t) = flow[i] {
            acc[t] += acc[i];
        }
    }
    acc
}

fn main_stream(flow: &[Option<usize>], acc: &[u32], river: &[bool]) -> Vec<bool> {
    let len = flow.len();
    let mut main = vec![false; len];
    let Some(outlet) = (0..len).filter(|&i| river[i] && flow[i].is_none_or(|t| !river[t])).max_by_key(|&i| (acc[i], Reverse(i))) else { return main };
    let mut up: Vec<Vec<usize>> = vec![Vec::new(); len];
    for i in (0..len).filter(|&i| river[i]) {
        if let Some(t) = flow[i] {
            up[t].push(i);
        }
    }
    let mut cur = outlet;
    loop {
        main[cur] = true;
        match up[cur].iter().copied().max_by_key(|&j| (acc[j], Reverse(j))) {
            Some(j) => cur = j,
            None => return main,
        }
    }
}

fn classify_tree(moisture: f32, slope: f32) -> Tree {
    match (moisture, slope) {
        (m, s) if m > 0.80 && s < 0.3 => Tree::Willow,
        (m, s) if m > 0.62 && s < 0.5 => Tree::Alder,
        (m, s) if m > 0.46 && s < 0.7 => Tree::Aspen,
        (m, s) if m > 0.32 && s < 0.8 => Tree::Birch,
        _ => Tree::Pine,
    }
}

fn hash01(seed: u64, i: usize) -> f32 {
    let mut h = seed ^ (i as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15);
    h ^= h >> 33;
    h = h.wrapping_mul(0xFF51_AFD7_ED55_8CCD);
    h ^= h >> 33;
    (h >> 40) as f32 / (1u64 << 24) as f32
}

pub fn inflow_walls(n: usize, cell_m: f32, inflows: &[(usize, usize, f32)]) -> Vec<bool> {
    let wall = (INFLOW_WALL_M / cell_m).round().max(1.0) as isize;
    let mut closed = vec![false; n * n];
    let border = |x: isize, y: isize| (0..n as isize).contains(&x) && (0..n as isize).contains(&y) && (x == 0 || y == 0 || x == n as isize - 1 || y == n as isize - 1);
    for &(x, y, _) in inflows.iter().filter(|&&(x, y, _)| x < n && y < n) {
        for cy in y as isize - wall..=y as isize + wall {
            for cx in x as isize - wall..=x as isize + wall {
                if border(cx, cy) {
                    closed[cy as usize * n + cx as usize] = true;
                }
            }
        }
    }
    closed
}

pub fn inflow_rate(km2: f32, cell_m: f32) -> f32 {
    WATER_PER_CATCHMENT_CELL * km2.max(0.0) * 1.0e6 / (cell_m * cell_m)
}

fn carve_bankfull(height: &mut [f32], surface: &[f32], flow: &[Option<usize>], river: &[bool], q_mean: &[f32], order: &[usize]) {
    let mut from_above = vec![f32::INFINITY; height.len()];
    for &i in order.iter().filter(|&&i| river[i]) {
        let bed = (surface[i] - crate::water::bankfull(q_mean[i]).1).min(from_above[i] - 1.0e-3);
        height[i] = height[i].min(bed);
        if let Some(t) = flow[i].filter(|&t| river[t]) {
            from_above[t] = from_above[t].min(bed);
        }
    }
}

pub fn from_heights(seed: u64, size: usize, raw: Vec<f32>, map: &RealMap) -> Result<World, WorldError> {
    let n = size;
    if n < MIN_SIZE || raw.len() != n * n || raw.iter().any(|h| !h.is_finite()) || map.cover.is_some_and(|c| c.len() != n * n) {
        return Err(WorldError::InvalidSize(n));
    }
    let fail = |reason: String| WorldError::GenerationFailed { seed, reason };
    let cell_m = map.cell_m;
    let (surface, flow) = fill_depressions(&raw, n, &inflow_walls(n, cell_m, map.inflows));
    let lake = find_lakes(&raw, &surface, n, cell_m);
    let mut height: Vec<f32> = (0..n * n).map(|i| if lake[i] { raw[i] } else { surface[i] }).collect();
    let mut order: Vec<usize> = (0..n * n).collect();
    order.sort_by(|&a, &b| surface[b].total_cmp(&surface[a]).then(a.cmp(&b)));
    let mut acc = accumulate(&order, &flow);
    let cell_km2 = cell_m * cell_m / 1.0e6;
    let (mut river, mut on_path, mut inputs) = (vec![false; n * n], vec![false; n * n], Vec::new());
    for &(x, y, km2) in map.inflows.iter().filter(|&&(x, y, km2)| x < n && y < n && km2 > 0.0) {
        let extra = (km2 / cell_km2).round() as u32;
        inputs.push(WaterInput { cell: (x, y), catchment: extra, kind: InputKind::Inflow, rate: inflow_rate(km2, cell_m) });
        let (mut cur, mut steps) = (Some(y * n + x), 0);
        while let (Some(i), true) = (cur, steps <= n * n) {
            acc[i] = acc[i].saturating_add(extra);
            on_path[i] = true;
            river[i] |= !lake[i];
            cur = flow[i];
            steps += 1;
        }
    }
    let (mut land, mut lateral) = (vec![1u32; n * n], vec![0u32; n * n]);
    for &i in order.iter().filter(|&&i| !on_path[i]) {
        match flow[i] {
            Some(t) if on_path[t] => lateral[t] += land[i],
            Some(t) => land[t] += land[i],
            None => {}
        }
    }
    for i in (0..n * n).filter(|&i| lateral[i] > 0) {
        inputs.push(WaterInput { cell: (i % n, i / n), catchment: lateral[i], kind: InputKind::Baseflow, rate: WATER_PER_CATCHMENT_CELL * lateral[i] as f32 });
    }
    let mut q = vec![0f32; n * n];
    for inp in &inputs {
        q[inp.cell.1 * n + inp.cell.0] += inp.rate * cell_m * cell_m / 86_400.0;
    }
    for &i in order.iter().filter(|&&i| on_path[i]) {
        if let Some(t) = flow[i] {
            q[t] += q[i];
        }
    }
    let river_flow: Vec<f32> = (0..n * n).map(|i| if river[i] { q[i] } else { 0.0 }).collect();
    let mut pond = vec![false; n * n];
    for comp in components(&lake, n).into_iter().filter(|c| c.iter().all(|&i| !on_path[i])) {
        comp.iter().for_each(|&i| pond[i] = true);
    }
    carve_bankfull(&mut height, &surface, &flow, &river, &river_flow, &order);
    if river.iter().filter(|r| **r).count() < 20 {
        return Err(fail("fewer than 20 river cells".into()));
    }
    let main = main_stream(&flow, &acc, &river);
    let at = |x: isize, y: isize| surface[(y.clamp(0, n as isize - 1) as usize) * n + x.clamp(0, n as isize - 1) as usize];
    let slope: Vec<f32> = (0..n * n)
        .map(|i| {
            let (x, y) = ((i % n) as isize, (i / n) as isize);
            let (gx, gy) = ((at(x + 1, y) - at(x - 1, y)) / (2.0 * cell_m), (at(x, y + 1) - at(x, y - 1)) / (2.0 * cell_m));
            (gx * gx + gy * gy).sqrt()
        })
        .collect();
    let mut dist = vec![u32::MAX; n * n];
    let mut queue: VecDeque<usize> = (0..n * n).filter(|&i| river[i] || lake[i]).collect();
    queue.iter().for_each(|&i| dist[i] = 0);
    while let Some(i) = queue.pop_front() {
        for j in neighbours4(i, n) {
            if dist[j] == u32::MAX {
                dist[j] = dist[i] + 1;
                queue.push_back(j);
            }
        }
    }
    let wet = Fbm::new(seed ^ 0x3E7, 5, 4);
    let amplitude = surface.iter().copied().fold(1.0f32, f32::max);
    let (mut moisture, mut forest) = (vec![0f32; n * n], vec![None; n * n]);
    for i in 0..n * n {
        let (u, v) = ((i % n) as f32 / n as f32, (i / n) as f32 / n as f32);
        let near = if dist[i] == u32::MAX { 0.0 } else { (-(dist[i] as f32) * cell_m / MOISTURE_REACH_M).exp() };
        moisture[i] = if lake[i] { 1.0 } else { (0.30 + 0.55 * near + 0.5 * (wet.sample(u, v) - 0.5) - 0.25 * surface[i] / amplitude - 0.25 * slope[i].min(1.0)).clamp(0.0, 1.0) };
        if lake[i] || river[i] {
            continue;
        }
        let class = map.cover.map_or(10, |c| c[i]);
        let per_m2 = TREES_PER_M2.iter().find(|(c, _)| *c == class).map_or(0.0, |t| t.1);
        if hash01(seed, i) < per_m2 * cell_m * cell_m {
            forest[i] = Some(classify_tree(moisture[i], slope[i]));
        }
    }
    let half = n as f32 / 2.0;
    Ok(World {
        seed,
        size: n,
        cell_m,
        height: grid_from(n, height),
        surface: grid_from(n, surface),
        valley: Grid::new(n, n, ValleyId(0)),
        valleys: vec![Valley { id: ValleyId(0), center: (half, half), open: true, cells: n * n }],
        start: ValleyId(0),
        flow_to: grid_from(n, flow),
        flow_acc: grid_from(n, acc),
        river: grid_from(n, river),
        main_river: grid_from(n, main),
        river_flow: grid_from(n, river_flow),
        lake: grid_from(n, lake),
        pond: grid_from(n, pond),
        water_inputs: inputs,
        moisture: grid_from(n, moisture),
        slope: grid_from(n, slope),
        forest: grid_from(n, forest),
    })
}

pub fn fixture(seed: u64) -> World {
    let n = 128;
    let raw: Vec<f32> = (0..n * n)
        .map(|i| {
            let (x, y) = ((i % n) as f32, (i / n) as f32);
            let wobble = 6.0 * ((x + seed as f32) * 0.07).sin();
            8.0 - 0.01 * x + 0.08 * (y - 64.0 - wobble).abs()
        })
        .collect();
    let cover: Vec<u8> = (0..n * n).map(|i| if (i / n + i % n + seed as usize) % 7 == 0 { 30 } else { 10 }).collect();
    let real = RealMap { cell_m: 1.0, inflows: &[(0, 64, 0.5)], cover: Some(&cover) };
    from_heights(seed, n, raw, &real).expect("the fixture valley is a valid world")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_real_map_has_rivers_only_on_the_paths_of_its_inflows_each_running_to_another_edge() {
        let (info, w) = crate::maps::world(&crate::maps::all()[0].id).unwrap();
        let n = w.size;
        for (&(x, y, km2), inp) in info.inflows.iter().zip(&w.water_inputs) {
            assert_eq!((inp.kind, inp.cell, inp.rate), (InputKind::Inflow, (x, y), inflow_rate(km2, info.cell_m)));
        }
        for inp in &w.water_inputs[info.inflows.len()..] {
            let i = inp.cell.1 * n + inp.cell.0;
            assert!(inp.kind == InputKind::Baseflow && (w.river.as_slice()[i] || w.lake.as_slice()[i]));
        }
        let flow = w.flow_to.as_slice();
        let mut on_path = vec![false; n * n];
        for inp in &w.water_inputs[..info.inflows.len()] {
            let (mut i, mut len) = (inp.cell.1 * n + inp.cell.0, 0);
            while let Some(t) = flow[i] {
                on_path[i] = true;
                (i, len) = (t, len + 1);
            }
            on_path[i] = true;
            let (ex, ey) = (i % n, i / n);
            assert!(ex == 0 || ey == 0 || ex == n - 1 || ey == n - 1);
            assert!((ex.abs_diff(inp.cell.0) + ey.abs_diff(inp.cell.1)) as f32 * info.cell_m > INFLOW_WALL_M, "{:?} leaves where it came in", inp.cell);
            assert!(len as f32 * info.cell_m >= 100.0, "{:?} runs only {len} cells", inp.cell);
        }
        assert!((0..n * n).all(|i| !w.river.as_slice()[i] || on_path[i]));
    }

    #[test]
    fn a_channel_is_cut_to_its_bankfull_depth_and_keeps_falling_across_flat_ground() {
        let n = 16;
        let surface: Vec<f32> = (0..n * n).map(|i| 5.0 + 1e-4 * (n - 1 - i % n) as f32).collect();
        let flow: Vec<Option<usize>> = (0..n * n).map(|i| (i % n + 1 < n).then_some(i + 1)).collect();
        let river: Vec<bool> = (0..n * n).map(|i| i / n == 8).collect();
        let mut order: Vec<usize> = (0..n * n).collect();
        order.sort_by(|&a, &b| surface[b].total_cmp(&surface[a]).then(a.cmp(&b)));
        let cut = |q: f32| {
            let mut height = surface.clone();
            carve_bankfull(&mut height, &surface, &flow, &river, &vec![q; n * n], &order);
            height
        };
        for q in [0.01f32, 0.2] {
            let (h, bank) = (cut(q), crate::water::bankfull(q).1);
            for x in 0..n {
                let i = 8 * n + x;
                assert!(surface[i] - h[i] >= bank - 1e-5 && surface[i] - h[i] < bank + 0.02);
                assert!(x + 1 == n || h[i] > h[i + 1]);
                assert_eq!(h[7 * n + x], surface[7 * n + x]);
            }
        }
        assert!(surface[8 * n] - cut(0.2)[8 * n] > surface[8 * n] - cut(0.01)[8 * n]);
    }

    #[test]
    fn forest_is_as_dense_as_real_forest_and_follows_the_land_cover() {
        let w = fixture(1);
        let n = w.size;
        let trees = |class: u8| {
            let cells: Vec<usize> = (0..n * n).filter(|&i| ((i / n + i % n + 1) % 7 == 0) == (class == 30) && !w.river.as_slice()[i]).collect();
            cells.iter().filter(|&&i| w.forest.as_slice()[i].is_some()).count() as f32 / cells.len() as f32 * 10_000.0
        };
        let (forest, grass) = (trees(10), trees(30));
        assert!((300.0..1200.0).contains(&forest), "{forest} trees per hectare of forest");
        assert!(grass < 0.1 * forest, "{grass} trees per hectare of grass");
    }

    #[test]
    fn the_fixture_valley_carries_a_stream_from_its_west_edge_and_is_deterministic() {
        let (a, b) = (fixture(3), fixture(3));
        assert_eq!(a.height, b.height);
        assert_eq!(a.forest, b.forest);
        assert!(*a.river.get(0, 64).unwrap());
        assert!((0..a.size).any(|y| *a.river.get(a.size - 1, y).unwrap()));
    }
}
