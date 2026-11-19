use crate::SimState;
use crate::grid::{Grid, HeightGrid, WaterGrid};
use crate::worldgen::{InputKind, World};
use std::cmp::Reverse;
use std::collections::BinaryHeap;
use std::fmt;
pub use crate::tuning::water::*;

pub type Cell = (usize, usize);

const DAY_S: f64 = 86_400.0;
const FLAT_STEP: f32 = 1.0e-5;
const BASIN_DEPTH: f32 = 0.01;
const MIN_SLOPE: f32 = 5.0e-4;
const DRY_Q: f32 = 1.0e-7;
const MIN_WETTED: f32 = 0.2;
const FLOOD_REACH: isize = 30;
const BATCH_DAYS: f32 = 1.0 / 144.0;
const SLOPE_REACH_M: f32 = 6.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Season { Spring, Summer, Autumn, Winter }

impl Season {
    pub fn at(time_days: f64) -> Season {
        match ((time_days / SEASON_DAYS).floor() as i64).rem_euclid(4) {
            0 => Season::Spring,
            1 => Season::Summer,
            2 => Season::Autumn,
            _ => Season::Winter,
        }
    }
    pub fn inflow_mult(self) -> f32 { INFLOW_MULT[self as usize] }
    pub fn rain_mult(self) -> f32 { RAIN_MULT[self as usize] }
    pub fn evap_mult(self) -> f32 { EVAP_MULT[self as usize] }
}

#[derive(Clone, Debug, PartialEq)]
pub struct WaterConfig {
    pub rain: f32,
    pub evaporation: f32,
    pub seepage: f32,
    pub dam_decay_per_day: f32,
    pub overtop_erosion: f32,
    pub overtop_depth: f32,
    pub ice_growth_per_day: f32,
    pub ice_melt_per_day: f32,
    pub open_edges: bool,
}

impl Default for WaterConfig {
    fn default() -> Self {
        Self {
            rain: RAIN,
            evaporation: EVAPORATION,
            seepage: SEEPAGE,
            dam_decay_per_day: DAM_DECAY_PER_DAY,
            overtop_erosion: OVERTOP_EROSION,
            overtop_depth: OVERTOP_DEPTH,
            ice_growth_per_day: ICE_GROWTH_PER_DAY,
            ice_melt_per_day: ICE_MELT_PER_DAY,
            open_edges: true,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Source {
    pub x: usize,
    pub y: usize,
    pub rate: f32,
    pub seasonality: f32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Budget {
    pub sources: f64,
    pub rain: f64,
    pub edges: f64,
    pub evaporation: f64,
    pub seepage: f64,
}

impl Budget {
    pub fn added(&self) -> f64 { self.sources + self.rain }
    pub fn lost(&self) -> f64 { self.edges + self.evaporation + self.seepage }
}

pub fn sources_for(world: &World) -> Vec<Source> {
    world.water_inputs.iter().map(|inp| Source { x: inp.cell.0, y: inp.cell.1, rate: inp.rate, seasonality: 1.0 }).collect()
}

pub fn bankfull(q_mean: f32) -> (f32, f32) {
    let q = BANKFULL_PER_MEAN * q_mean.max(0.0);
    (BANK_WIDTH.0 * q.powf(BANK_WIDTH.1), BANK_DEPTH.0 * q.powf(BANK_DEPTH.1))
}

pub fn manning_depth(q: f32, width: f32, slope: f32, n: f32) -> f32 {
    (q.max(0.0) * n / (width.max(1e-3) * slope.max(MIN_SLOPE).sqrt())).powf(0.6)
}

pub fn spill_head(q: f32, crest: f32) -> f32 {
    (q.max(0.0) / (WEIR * crest.max(1e-3))).powf(2.0 / 3.0)
}

#[derive(Clone, Debug, PartialEq)]
pub enum WaterError {
    OutOfBounds { x: usize, y: usize },
    NoCells,
    InvalidValue { what: &'static str, value: f32 },
    NoDam { x: usize, y: usize },
}

impl fmt::Display for WaterError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::OutOfBounds { x, y } => write!(f, "cell ({x}, {y}) is outside the map"),
            Self::NoCells => write!(f, "no cells given"),
            Self::InvalidValue { what, value } => write!(f, "invalid {what}: {value} (must be finite and > 0)"),
            Self::NoDam { x, y } => write!(f, "no dam at ({x}, {y})"),
        }
    }
}
impl std::error::Error for WaterError {}

#[derive(Clone, Debug, PartialEq)]
pub struct DamPreview {
    pub level: f32,
    pub pond_area: usize,
    pub volume: f32,
    pub flooded_cells: Vec<Cell>,
    pub flooded_trees: usize,
    pub flooded_buildings: usize,
    pub reaches_map_edge: bool,
    pub crest: f32,
    pub spill_at: Option<Cell>,
}

#[derive(Clone, Debug, Default)]
struct Basin {
    cells: Vec<usize>,
    beds: Vec<f32>,
    below: Vec<f64>,
    outlet: usize,
    spill: f32,
    crest: f32,
    cap: f64,
    vol: f64,
    fed: bool,
}

impl Basin {
    fn volume_at(&self, level: f32, a: f64) -> f64 {
        let k = self.beds.partition_point(|&b| b <= level);
        if k == 0 { 0.0 } else { self.below[k - 1] + k as f64 * a * (level - self.beds[k - 1]) as f64 }
    }
    fn level(&self, a: f64) -> f32 {
        if self.vol >= self.cap {
            return self.spill + ((self.vol - self.cap) / (self.cells.len() as f64 * a)) as f32;
        }
        let k = self.below.partition_point(|&v| v <= self.vol).max(1);
        self.beds[k - 1] + ((self.vol - self.below[k - 1]) / (k as f64 * a)) as f32
    }
    fn wet_area(&self, level: f32, a: f64) -> f64 {
        self.beds.partition_point(|&b| b < level) as f64 * a
    }
}

#[derive(Clone, Debug)]
pub struct Water {
    pub depth: WaterGrid,
    pub ice: Grid<f32>,
    pub dam_height: Grid<f32>,
    pub dam_integrity: Grid<f32>,
    pub canal: Grid<bool>,
    pub fed: Grid<bool>,
    pub structures: Grid<bool>,
    pub closed_edge: Grid<bool>,
    pub width: Grid<f32>,
    pub channel: Grid<bool>,
    pub discharge: Grid<f32>,
    pub lake_level: Grid<f32>,
    pub sources: Vec<Source>,
    pub config: WaterConfig,
    pub time_days: f64,
    pub added_volume: f64,
    pub lost_volume: f64,
    pub cell_m: f32,
    books: Budget,
    bank: Vec<(f32, f32)>,
    bed: Vec<f32>,
    base_down: Vec<Option<usize>>,
    routed_open: Option<bool>,
    filled: Vec<f32>,
    down: Vec<Option<usize>>,
    order: Vec<usize>,
    basin_of: Vec<Option<usize>>,
    basins: Vec<Basin>,
    releases: Vec<(usize, f64)>,
    shown: Vec<f32>,
    channel_volume: f64,
    lake_at: Vec<f32>,
    lake_volume: f64,
    pending: f32,
}

impl Water {
    pub fn new(size: usize) -> Self {
        let n = size * size;
        Self {
            depth: Grid::new(size, size, 0.0),
            ice: Grid::new(size, size, 0.0),
            dam_height: Grid::new(size, size, 0.0),
            dam_integrity: Grid::new(size, size, 0.0),
            canal: Grid::new(size, size, false),
            fed: Grid::new(size, size, false),
            structures: Grid::new(size, size, false),
            closed_edge: Grid::new(size, size, false),
            width: Grid::new(size, size, 0.0),
            channel: Grid::new(size, size, false),
            discharge: Grid::new(size, size, 0.0),
            lake_level: Grid::new(size, size, f32::NAN),
            sources: Vec::new(),
            config: WaterConfig::default(),
            time_days: 0.0,
            added_volume: 0.0,
            lost_volume: 0.0,
            cell_m: 4.0,
            books: Budget::default(),
            bank: vec![(0.0, 0.0); n],
            bed: Vec::new(),
            base_down: Vec::new(),
            routed_open: None,
            filled: Vec::new(),
            down: Vec::new(),
            order: Vec::new(),
            basin_of: vec![None; n],
            basins: Vec::new(),
            releases: Vec::new(),
            shown: vec![0.0; n],
            channel_volume: 0.0,
            lake_at: vec![f32::NAN; n],
            lake_volume: 0.0,
            pending: 0.0,
        }
    }

    pub fn from_world(world: &World) -> Self {
        let n = world.size;
        let mut w = Self::new(n);
        w.cell_m = world.cell_m;
        w.sources = sources_for(world);
        let inflows: Vec<(usize, usize, f32)> =
            world.water_inputs.iter().filter(|i| i.kind == InputKind::Inflow).map(|i| (i.cell.0, i.cell.1, 1.0)).collect();
        for (c, closed) in w.closed_edge.as_mut_slice().iter_mut().zip(crate::worldgen::inflow_walls(n, world.cell_m, &inflows)) {
            *c = closed;
        }
        for (f, &l) in w.fed.as_mut_slice().iter_mut().zip(world.lake.as_slice()) {
            *f = l;
        }
        w.lake_level = world.lake_level.clone();
        w.base_down = world.flow_to.as_slice().to_vec();
        w.route(&world.height);
        let mean = w.mean_flow();
        for i in 0..n * n {
            if world.river.as_slice()[i] && mean[i] > 0.0 {
                w.bank[i] = bankfull(mean[i]);
            }
        }
        w.route(&world.height);
        for b in &mut w.basins {
            b.vol = b.cap;
        }
        w.refresh(&world.height);
        w.reset_books();
        w
    }

    fn area(&self) -> f64 { (self.cell_m * self.cell_m) as f64 }



    pub fn reset_books(&mut self) {
        (self.added_volume, self.lost_volume) = (0.0, 0.0);
        self.books = Budget::default();
    }

    pub fn budget(&self) -> Budget {
        let edges = self.lost_volume - self.books.evaporation - self.books.seepage;
        Budget { edges, ..self.books }
    }

    pub fn season(&self) -> Season { Season::at(self.time_days) }

    pub fn total_volume(&self) -> f64 {
        let held: f64 = (0..self.shown.len()).map(|i| self.cell_volume(i)).sum();
        held + self.releases.iter().map(|r| r.1).sum::<f64>() / self.area()
    }

    fn cell_volume(&self, i: usize) -> f64 {
        let (d, w) = (self.depth.as_slice()[i] as f64, self.width.as_slice()[i]);
        if w > 0.0 { d * (w / self.cell_m).min(1.0) as f64 } else { d }
    }

    pub fn flow_at(&self, x: usize, y: usize) -> (f32, f32) {
        if !self.depth.in_bounds(x, y) || self.down.is_empty() {
            return (0.0, 0.0);
        }
        let n = self.depth.width();
        let i = y * n + x;
        let q = self.discharge.as_slice()[i] * DAY_S as f32 / self.area() as f32;
        match self.down[i] {
            Some(t) if self.basin_of[i].is_none() => {
                let (dx, dy) = ((t % n) as f32 - x as f32, (t / n) as f32 - y as f32);
                let l = (dx * dx + dy * dy).sqrt();
                (q * dx / l, q * dy / l)
            }
            _ => (0.0, 0.0),
        }
    }

    pub fn in_lake(&self, x: usize, y: usize) -> bool {
        self.depth.in_bounds(x, y) && self.lake_at.get(y * self.depth.width() + x).is_some_and(|l| l.is_finite())
    }

    pub fn bank_depth(&self, i: usize) -> f32 {
        self.bank.get(i).map_or(0.0, |b| b.1)
    }

    pub fn downstream(&self, x: usize, y: usize) -> Option<Cell> {
        let n = self.depth.width();
        if !self.depth.in_bounds(x, y) {
            return None;
        }
        self.down.get(y * n + x).copied().flatten().map(|t| (t % n, t / n))
    }

    pub fn has_dam(&self, x: usize, y: usize) -> bool {
        self.dam_height.get(x, y).is_some_and(|&h| h > 0.0)
    }

    fn check(&self, x: usize, y: usize) -> Result<usize, WaterError> {
        if self.depth.in_bounds(x, y) { Ok(y * self.depth.width() + x) } else { Err(WaterError::OutOfBounds { x, y }) }
    }

    fn check_cells(&self, cells: &[Cell]) -> Result<Vec<usize>, WaterError> {
        if cells.is_empty() {
            return Err(WaterError::NoCells);
        }
        cells.iter().map(|&(x, y)| self.check(x, y)).collect()
    }

    fn check_positive(what: &'static str, value: f32) -> Result<(), WaterError> {
        if value.is_finite() && value > 0.0 { Ok(()) } else { Err(WaterError::InvalidValue { what, value }) }
    }

    pub fn add_source(&mut self, x: usize, y: usize, rate: f32) -> Result<(), WaterError> {
        self.check(x, y)?;
        Self::check_positive("source rate", rate)?;
        self.sources.push(Source { x, y, rate, seasonality: 1.0 });
        Ok(())
    }

    pub fn raise_dam(&mut self, cells: &[(Cell, f32)]) -> Result<(), WaterError> {
        let only: Vec<Cell> = cells.iter().map(|c| c.0).collect();
        let idx = self.check_cells(&only)?;
        for &(_, h) in cells {
            Self::check_positive("dam height", h)?;
        }
        for (i, &(_, h)) in idx.into_iter().zip(cells) {
            let cur = &mut self.dam_height.as_mut_slice()[i];
            *cur = cur.max(h);
            self.dam_integrity.as_mut_slice()[i] = 1.0;
        }
        Ok(())
    }

    pub fn build_dam(&mut self, cells: &[Cell], height: f32) -> Result<(), WaterError> {
        let idx = self.check_cells(cells)?;
        Self::check_positive("dam height", height)?;
        for i in idx {
            self.dam_height.as_mut_slice()[i] = height;
            self.dam_integrity.as_mut_slice()[i] = 1.0;
        }
        Ok(())
    }

    pub fn breach(&mut self, x: usize, y: usize) -> Result<(), WaterError> {
        let i = self.check(x, y)?;
        if self.dam_height.as_slice()[i] <= 0.0 {
            return Err(WaterError::NoDam { x, y });
        }
        self.dam_height.as_mut_slice()[i] = 0.0;
        self.dam_integrity.as_mut_slice()[i] = 0.0;
        Ok(())
    }

    pub fn dig_canal(&mut self, height: &mut HeightGrid, cells: &[Cell], depth: f32) -> Result<(), WaterError> {
        assert_eq!(height.width(), self.depth.width(), "terrain size must match water size");
        let idx = self.check_cells(cells)?;
        Self::check_positive("canal depth", depth)?;
        for i in idx {
            height.as_mut_slice()[i] -= depth;
            self.canal.as_mut_slice()[i] = true;
        }
        Ok(())
    }

    pub fn refresh(&mut self, height: &HeightGrid) {
        self.step(height, 0.0);
    }

    pub fn step(&mut self, height: &HeightGrid, dt_days: f32) {
        assert_eq!((height.width(), height.height()), (self.depth.width(), self.depth.height()), "terrain size must match water size");
        let bed: Vec<f32> = height.as_slice().iter().zip(self.dam_height.as_slice()).map(|(t, d)| t + d).collect();
        let changed = bed != self.bed || self.routed_open != Some(self.config.open_edges) || self.depth.as_slice() != self.shown.as_slice();
        self.pending += dt_days.max(0.0);
        if !changed && self.pending < BATCH_DAYS && dt_days > 0.0 {
            return;
        }
        let dt_days = std::mem::take(&mut self.pending);
        if bed != self.bed || self.routed_open != Some(self.config.open_edges) {
            self.reroute(height);
        }
        self.take_loose_water();
        let dt = dt_days as f64 * DAY_S;
        let (edge, climate) = self.flow(dt);
        self.show(dt_days, edge, climate);
        self.update_dams(height, dt_days);
        self.time_days += dt_days as f64;
    }

    fn neighbours8(&self, i: usize) -> impl Iterator<Item = usize> + use<> {
        neighbours8(self.depth.width(), i)
    }

    fn route(&mut self, height: &HeightGrid) {
        let n = self.depth.width();
        let len = n * n;
        self.bed = height.as_slice().iter().zip(self.dam_height.as_slice()).map(|(t, d)| t + d).collect();
        self.routed_open = Some(self.config.open_edges);
        let border = |i: usize| i % n == 0 || i / n == 0 || i % n == n - 1 || i / n == n - 1;
        let mut filled = self.bed.clone();
        let mut down = vec![None; len];
        let mut done = vec![false; len];
        let mut heap = BinaryHeap::new();
        self.lake_at = self.lake_cells();
        for i in (0..len).filter(|&i| self.lake_at[i].is_finite()) {
            done[i] = true;
            heap.push(Reverse((filled[i].to_bits() ^ sign_flip(filled[i]), i)));
        }
        let closed = self.closed_edge.as_slice();
        let dams = self.dam_height.as_slice().to_vec();
        let open = self.config.open_edges;
        let falls_out = |i: usize| {
            let (x, y) = (i % n, i / n);
            let inner = [(x == 0).then(|| i + 1), (x == n - 1).then(|| i - 1), (y == 0).then(|| i + n), (y == n - 1).then(|| i - n)];
            inner.into_iter().flatten().all(|j| self.bed[j] >= self.bed[i])
        };
        for i in (0..len).filter(|&i| open && border(i) && !closed[i] && falls_out(i)) {
            done[i] = true;
            heap.push(Reverse((filled[i].to_bits() ^ sign_flip(filled[i]), i)));
        }
        let root = if heap.is_empty() {
            let r = (0..len).min_by(|&a, &b| self.bed[a].total_cmp(&self.bed[b]).then(a.cmp(&b))).unwrap_or(0);
            done[r] = true;
            heap.push(Reverse((filled[r].to_bits() ^ sign_flip(filled[r]), r)));
            Some(r)
        } else {
            None
        };
        while let Some(Reverse((_, c))) = heap.pop() {
            let nb: Vec<usize> = self.neighbours8(c).collect();
            for j in nb {
                if !done[j] {
                    done[j] = true;
                    filled[j] = self.bed[j].max(filled[c] + FLAT_STEP).max(corner(&self.bed, |k| dams[k] > 0.0, n, c, j));
                    down[j] = Some(c);
                    heap.push(Reverse((filled[j].to_bits() ^ sign_flip(filled[j]), j)));
                }
            }
        }
        for (i, base) in self.base_down.iter().enumerate() {
            if let Some(t) = *base {
                if down[i].is_some() && filled[t] < filled[i] && corner(&self.bed, |k| dams[k] > 0.0, n, i, t) <= filled[i] {
                    down[i] = Some(t);
                }
            }
        }
        let mut order: Vec<usize> = (0..len).collect();
        order.sort_by(|&a, &b| filled[b].total_cmp(&filled[a]).then(a.cmp(&b)));
        let mut basin_of = vec![None; len];
        let mut basins = Vec::new();
        let deep: Vec<bool> = (0..len).map(|i| filled[i] - self.bed[i] > BASIN_DEPTH).collect();
        for s in 0..len {
            if !deep[s] || basin_of[s].is_some() {
                continue;
            }
            let id = basins.len();
            let mut cells = vec![s];
            basin_of[s] = Some(id);
            let mut k = 0;
            while k < cells.len() {
                let c = cells[k];
                k += 1;
                let nb: Vec<usize> = self.neighbours8(c).collect();
                for j in nb {
                    if deep[j] && basin_of[j].is_none() {
                        basin_of[j] = Some(id);
                        cells.push(j);
                    }
                }
            }
            let outlet = *cells.iter().min_by(|&&a, &&b| filled[a].total_cmp(&filled[b]).then(a.cmp(&b))).unwrap();
            let spill = filled[outlet] - FLAT_STEP;
            basins.push(self.make_basin(cells, outlet, spill, &basin_of, id));
        }
        if let Some(r) = root {
            let all: Vec<usize> = (0..len).filter(|&i| basin_of[i].is_none()).collect();
            let id = basins.len();
            for &i in &all {
                basin_of[i] = Some(id);
            }
            let mut b = self.make_basin(all, r, f32::MAX, &basin_of, id);
            b.cap = f64::MAX;
            basins.push(b);
        }
        (self.filled, self.down, self.order, self.basin_of, self.basins) = (filled, down, order, basin_of, basins);
    }

    fn lake_cells(&self) -> Vec<f32> {
        let n = self.depth.width();
        let mut at = vec![f32::NAN; n * n];
        let seeds = self.lake_level.as_slice();
        let dams = self.dam_height.as_slice();
        let mut queue: std::collections::VecDeque<usize> = (0..n * n).filter(|&i| seeds[i].is_finite() && self.bed[i] < seeds[i]).collect();
        queue.iter().for_each(|&i| at[i] = seeds[i]);
        while let Some(i) = queue.pop_front() {
            let lvl = at[i];
            for j in neighbours8(n, i) {
                if at[j].is_nan() && self.bed[j] < lvl && corner(&self.bed, |k| dams[k] > 0.0, n, i, j) < lvl {
                    at[j] = lvl;
                    queue.push_back(j);
                }
            }
        }
        at
    }

    fn make_basin(&self, mut cells: Vec<usize>, outlet: usize, spill: f32, basin_of: &[Option<usize>], id: usize) -> Basin {
        cells.sort_by(|&a, &b| self.bed[a].total_cmp(&self.bed[b]).then(a.cmp(&b)));
        let beds: Vec<f32> = cells.iter().map(|&i| self.bed[i]).collect();
        let below = cumulative(&beds, self.area());
        let lip = cells.iter().flat_map(|&c| self.neighbours8(c)).filter(|&j| basin_of[j] != Some(id) && self.bed[j] <= spill + 0.01).collect::<std::collections::HashSet<_>>();
        let dammed = lip.iter().any(|&j| self.dam_height.as_slice()[j] > 0.0);
        let channel = self.bank[outlet].0;
        let crest = if !dammed && channel > 0.0 { channel } else { self.cell_m * lip.len().max(1) as f32 };
        let fed = cells.iter().any(|&i| self.fed.as_slice()[i]);
        let mut b = Basin { cells, beds, below, outlet, spill, crest, cap: 0.0, vol: 0.0, fed };
        b.cap = b.volume_at(spill, self.area());
        b
    }

    fn reroute(&mut self, height: &HeightGrid) {
        let a = self.area();
        let held: Vec<(usize, f64)> = (0..self.shown.len())
            .filter(|&i| self.basin_of.get(i).copied().flatten().is_some() && self.shown[i] > 0.0)
            .map(|i| (i, self.shown[i] as f64 * a))
            .collect();
        self.route(height);
        for (i, v) in held {
            match self.basin_of[i] {
                Some(b) => self.basins[b].vol += v,
                None => self.releases.push((i, v)),
            }
        }
    }

    fn take_loose_water(&mut self) {
        let a = self.area();
        for i in 0..self.shown.len() {
            let (d, s) = (self.depth.as_slice()[i], self.shown[i]);
            if (d - s).abs() <= 1e-6 {
                continue;
            }
            let v = (d - s) as f64 * a;
            match self.basin_of[i].filter(|&b| self.basins[b].spill < f32::MAX) {
                Some(b) => self.basins[b].vol = (self.basins[b].vol + v).max(0.0),
                None if v > 0.0 => self.releases.push((i, v)),
                None => {}
            }
        }
    }

    fn mean_flow(&self) -> Vec<f32> {
        let a = self.area() as f32;
        let mut q = vec![0f32; self.shown.len()];
        for s in &self.sources {
            q[s.y * self.depth.width() + s.x] += s.rate * a / DAY_S as f32;
        }
        for &i in &self.order {
            if let Some(t) = self.down[i] {
                q[t] += q[i];
            }
        }
        q
    }

    fn flow(&mut self, dt: f64) -> (f64, [f64; 3]) {
        let n = self.depth.width();
        let a = self.area();
        let season = self.season();
        let mut q = vec![0f64; n * n];
        for s in &self.sources {
            q[s.y * n + s.x] += (s.rate * (1.0 + s.seasonality * (season.inflow_mult() - 1.0))) as f64 * a / DAY_S;
        }
        let tau = RELEASE_DAYS as f64 * DAY_S;
        for r in &mut self.releases {
            let out = if dt > 0.0 { r.1 * (1.0 - (-dt / tau).exp()) } else { 0.0 };
            if dt > 0.0 {
                q[r.0] += out / dt;
            }
            r.1 -= out;
        }
        self.releases.retain(|r| r.1 > 1e-6);
        let mut inflow = vec![0f64; self.basins.len()];
        let mut edge = 0f64;
        let mut climate = [0f64; 3];
        for k in 0..self.order.len() {
            let i = self.order[k];
            if let Some(b) = self.basin_of[i].filter(|&b| self.basins[b].spill < f32::MAX || self.basins[b].outlet == i) {
                inflow[b] += q[i];
                if i != self.basins[b].outlet {
                    q[i] = 0.0;
                    continue;
                }
                q[i] = self.settle_basin(b, inflow[b], dt, &mut climate);
            }
            let captured = |c: usize| self.basin_of[c].filter(|&b| self.basins[b].spill < f32::MAX);
            match self.down[i] {
                Some(t) if captured(i).is_none() || captured(t) != captured(i) => q[t] += q[i],
                Some(_) => {}
                None => edge += q[i] * dt,
            }
        }
        for (d, v) in self.discharge.as_mut_slice().iter_mut().zip(&q) {
            *d = *v as f32;
        }
        (edge, climate)
    }

    fn settle_basin(&mut self, b: usize, qin: f64, dt: f64, climate: &mut [f64; 3]) -> f64 {
        let a = self.area();
        let season = self.season();
        let cfg = self.config.clone();
        let basin = &mut self.basins[b];
        if dt <= 0.0 {
            return if basin.vol >= basin.cap { qin } else { 0.0 };
        }
        let level = basin.level(a);
        let wet = basin.wet_area(level, a);
        let mut v = basin.vol + qin * dt;
        if !basin.fed {
            let day = dt / DAY_S;
            let rain = (cfg.rain * season.rain_mult()) as f64 * wet * day;
            v += rain;
            let evap = ((cfg.evaporation * season.evap_mult()) as f64 * wet * day).min(v.max(0.0));
            v -= evap;
            let seep = (cfg.seepage as f64 * wet * day).min(v.max(0.0));
            v -= seep;
            climate[0] += rain;
            climate[1] += evap;
            climate[2] += seep;
        }
        v = v.max(0.0);
        if v <= basin.cap {
            basin.vol = v;
            return 0.0;
        }
        let excess = v - basin.cap;
        let head = excess / (basin.cells.len() as f64 * a);
        let out = (WEIR as f64 * basin.crest as f64 * head.powf(1.5)).min(excess / dt);
        basin.vol = v - out * dt;
        out
    }

    fn show(&mut self, dt_days: f32, edge: f64, climate: [f64; 3]) {
        let n = self.depth.width();
        let a = self.area();
        let cm = self.cell_m;
        let mut depth = vec![0f32; n * n];
        let mut width = vec![0f32; n * n];
        let mut in_channel = vec![false; n * n];
        for i in (0..n * n).filter(|&i| self.lake_at[i].is_finite()) {
            depth[i] = (self.lake_at[i] - self.bed[i]).max(0.0);
            width[i] = cm;
        }
        for b in &self.basins {
            let level = b.level(a);
            for &i in &b.cells {
                if level > self.bed[i] {
                    depth[i] = level - self.bed[i];
                    width[i] = cm;
                }
            }
        }
        let q = self.discharge.as_slice().to_vec();
        let levels: Vec<f32> = self.basins.iter().map(|b| b.level(a)).collect();
        for i in 0..n * n {
            if self.basin_of[i].is_some() || q[i] <= DRY_Q || self.lake_at[i].is_finite() {
                continue;
            }
            let slope = self.slope(i, &levels);
            let (bw, bd) = self.bank[i];
            if bw <= 0.0 {
                let d = manning_depth(q[i], cm, slope, MANNING_OVERLAND);
                if depth[i] < d {
                    (depth[i], width[i], in_channel[i]) = (d, cm, true);
                }
                continue;
            }
            let qbf = (bw / BANK_WIDTH.0).powf(1.0 / BANK_WIDTH.1);
            let w = (bw * (q[i] / qbf).powf(WETTED_WIDTH_EXP)).clamp(MIN_WETTED.min(bw), bw);
            let d = manning_depth(q[i], w, slope, MANNING_CHANNEL);
            if d <= bd {
                if depth[i] < d {
                    (depth[i], width[i], in_channel[i]) = (d, w, true);
                }
                continue;
            }
            for (j, dj) in self.overbank(i, q[i], slope) {
                if depth[j] < dj {
                    (depth[j], width[j]) = (dj, cm);
                }
            }
        }
        let lake: f64 = (0..n * n).filter(|&i| self.lake_at[i].is_finite()).map(|i| depth[i] as f64).sum();
        let lake_change = lake - std::mem::replace(&mut self.lake_volume, lake);
        let channel: f64 = (0..n * n)
            .filter(|&i| self.basin_of[i].is_none() && self.lake_at[i].is_nan())
            .map(|i| depth[i] as f64 * if width[i] > 0.0 { (width[i] / cm).min(1.0) as f64 } else { 1.0 })
            .sum();
        let stored = channel - self.channel_volume;
        self.channel_volume = channel;
        let sourced: f64 = if dt_days > 0.0 {
            let season = self.season();
            self.sources.iter().map(|s| (s.rate * (1.0 + s.seasonality * (season.inflow_mult() - 1.0))) as f64 * dt_days as f64).sum()
        } else {
            0.0
        };
        let edges = edge / a - stored;
        self.books.sources += sourced + lake_change;
        self.books.rain += climate[0] / a;
        self.books.evaporation += climate[1] / a;
        self.books.seepage += climate[2] / a;
        self.added_volume += sourced + lake_change + climate[0] / a;
        self.lost_volume += edges + (climate[1] + climate[2]) / a;
        let winter = self.season() == Season::Winter;
        let (grow, melt) = (self.config.ice_growth_per_day * dt_days, self.config.ice_melt_per_day * dt_days);
        for (ice, &d) in self.ice.as_mut_slice().iter_mut().zip(&depth) {
            *ice = if winter && d > WET_EPS { (*ice + grow).min(MAX_ICE) } else { (*ice - melt).max(0.0) };
        }
        self.depth.as_mut_slice().copy_from_slice(&depth);
        self.width.as_mut_slice().copy_from_slice(&width);
        self.channel.as_mut_slice().copy_from_slice(&in_channel);
        self.shown = depth;
    }

    fn slope(&self, i: usize, levels: &[f32]) -> f32 {
        let n = self.depth.width();
        let dist = |a: usize, b: usize| if a % n != b % n && a / n != b / n { std::f32::consts::SQRT_2 } else { 1.0 };
        let (mut c, mut len, mut end) = (i, 0.0, self.bed[i]);
        while len * self.cell_m < SLOPE_REACH_M {
            let Some(t) = self.down[c] else { break };
            len += dist(c, t);
            c = t;
            if let Some(b) = self.basin_of[t] {
                end = levels[b].max(self.bed[t]).min(end);
                break;
            }
            end = self.bed[t];
        }
        if len <= 0.0 {
            let up = neighbours8(n, i).filter(|&j| self.down[j] == Some(i)).max_by(|&a, &b| self.bed[a].total_cmp(&self.bed[b]));
            return up.map_or(MIN_SLOPE, |j| ((self.bed[j] - self.bed[i]) / (dist(i, j) * self.cell_m)).max(MIN_SLOPE));
        }
        ((self.bed[i] - end) / (len * self.cell_m)).max(MIN_SLOPE)
    }

    fn overbank(&self, i: usize, q: f32, slope: f32) -> Vec<(usize, f32)> {
        let n = self.depth.width() as isize;
        let (bw, bd) = self.bank[i];
        let (x, y) = ((i as isize) % n, (i as isize) / n);
        let (dx, dy) = match self.down[i] {
            Some(t) => ((t as isize) % n - x, (t as isize) / n - y),
            None => (1, 0),
        };
        let sides: Vec<Vec<usize>> = [(-dy, dx), (dy, -dx)]
            .iter()
            .map(|&(px, py)| {
                (1..=FLOOD_REACH)
                    .map(|k| (x + px * k, y + py * k))
                    .take_while(|&(cx, cy)| (0..n).contains(&cx) && (0..n).contains(&cy))
                    .map(|(cx, cy)| (cy * n + cx) as usize)
                    .collect()
            })
            .collect();
        let bed = self.bed[i];
        let carry = |level: f32| {
            let mut k = bw * (level - bed).max(0.0).powf(5.0 / 3.0) / MANNING_CHANNEL;
            for side in &sides {
                for &j in side {
                    let h = level - self.bed[j];
                    if h <= 0.0 {
                        break;
                    }
                    k += self.cell_m * h.powf(5.0 / 3.0) / MANNING_OVERLAND;
                }
            }
            k * slope.sqrt()
        };
        let (mut lo, mut hi) = (bed + bd, bed + bd + 6.0);
        for _ in 0..30 {
            let mid = 0.5 * (lo + hi);
            if carry(mid) < q { lo = mid } else { hi = mid }
        }
        let mut out = vec![(i, hi - bed)];
        for side in &sides {
            for &j in side {
                if self.bed[j] >= hi {
                    break;
                }
                out.push((j, hi - self.bed[j]));
            }
        }
        out
    }

    fn update_dams(&mut self, height: &HeightGrid, dt: f32) {
        let w = self.depth.width();
        let (terrain, dam, depth) = (height.as_slice(), self.dam_height.as_slice(), self.depth.as_slice());
        let surface = |i: usize| if depth[i] > WET_EPS { terrain[i] + dam[i] + depth[i] } else { f32::NEG_INFINITY };
        let mut broken = Vec::new();
        for i in (0..w * w).filter(|&i| dam[i] > 0.0) {
            let (x, y) = (i % w, i / w);
            let crest = terrain[i] + dam[i];
            let mut top = surface(i);
            for (ok, j) in [(x > 0, i.wrapping_sub(1)), (x + 1 < w, i + 1), (y > 0, i.wrapping_sub(w)), (y + 1 < w, i + w)] {
                if ok {
                    top = top.max(surface(j));
                }
            }
            let over = (top - crest - self.config.overtop_depth).max(0.0);
            let integ = &mut self.dam_integrity.as_mut_slice()[i];
            *integ -= (self.config.dam_decay_per_day + self.config.overtop_erosion * over) * dt;
            if *integ <= 0.0 {
                broken.push(i);
            }
        }
        for i in broken {
            self.dam_height.as_mut_slice()[i] = 0.0;
            self.dam_integrity.as_mut_slice()[i] = 0.0;
        }
    }
}

fn sign_flip(v: f32) -> u32 {
    if v.is_sign_negative() { u32::MAX } else { 1 << 31 }
}

fn cumulative(beds: &[f32], a: f64) -> Vec<f64> {
    let mut below = Vec::with_capacity(beds.len());
    let mut acc = 0.0;
    for k in 0..beds.len() {
        if k > 0 {
            acc += k as f64 * a * (beds[k] - beds[k - 1]) as f64;
        }
        below.push(acc);
    }
    below
}

pub fn preview_dam(state: &SimState, cells: &[Cell], height: f32) -> Result<DamPreview, WaterError> {
    preview_dam_heights(state, cells, &vec![height; cells.len()])
}

pub fn neighbours8(n: usize, i: usize) -> impl Iterator<Item = usize> {
    let (x, y) = ((i % n) as isize, (i / n) as isize);
    let m = n as isize;
    (-1..=1).flat_map(move |dy| (-1..=1).map(move |dx| (dx, dy))).filter_map(move |(dx, dy)| {
        let (nx, ny) = (x + dx, y + dy);
        ((dx, dy) != (0, 0) && (0..m).contains(&nx) && (0..m).contains(&ny)).then_some((ny * m + nx) as usize)
    })
}

pub fn corner(bed: &[f32], dammed: impl Fn(usize) -> bool, n: usize, i: usize, j: usize) -> f32 {
    let (a, b) = ((i / n) * n + j % n, (j / n) * n + i % n);
    if i % n == j % n || i / n == j / n || !(dammed(a) || dammed(b)) { f32::NEG_INFINITY } else { bed[a].min(bed[b]) }
}

pub fn preview_dam_heights(state: &SimState, cells: &[Cell], heights: &[f32]) -> Result<DamPreview, WaterError> {
    let water = &state.water;
    let idx = water.check_cells(cells)?;
    if heights.len() != cells.len() {
        return Err(WaterError::NoCells);
    }
    for &hh in heights {
        Water::check_positive("dam height", hh)?;
    }
    let terrain = state.height.as_slice();
    let n = state.height.width();
    let mut bed: Vec<f32> = terrain.iter().zip(water.dam_height.as_slice()).map(|(t, d)| t + d).collect();
    let mut is_new = vec![false; n * n];
    for (&i, &hh) in idx.iter().zip(heights) {
        bed[i] = terrain[i] + hh.max(water.dam_height.as_slice()[i]);
        is_new[i] = true;
    }
    let crest = idx.iter().map(|&i| bed[i]).fold(f32::INFINITY, f32::min);
    let dams = water.dam_height.as_slice();
    let pass = |i: usize, j: usize| bed[i].max(bed[j]).max(corner(&bed, |k| is_new[k] || dams[k] > 0.0, n, i, j));
    let drains_to = |i: usize| neighbours8(n, i).filter(|&j| terrain[j] < terrain[i]).min_by(|&a, &b| terrain[a].total_cmp(&terrain[b]).then(a.cmp(&b)));
    let seeds: Vec<usize> = idx.iter().flat_map(|&d| neighbours8(n, d).filter(move |&m| drains_to(m) == Some(d))).filter(|&m| !is_new[m]).collect();
    let below: std::collections::HashSet<usize> = idx.iter().filter_map(|&d| drains_to(d)).filter(|&m| !is_new[m]).collect();
    let (mut spill_level, mut spill_at) = (f32::INFINITY, None);
    if !seeds.is_empty() && !below.is_empty() {
        let mut best = vec![f32::INFINITY; n * n];
        let mut saddle = vec![usize::MAX; n * n];
        let mut heap = BinaryHeap::new();
        let key = |l: f32, i: usize| Reverse(((l.max(-1.0e6) * 1000.0) as i64, i));
        for &s in &seeds {
            (best[s], saddle[s]) = (bed[s], s);
            heap.push(key(bed[s], s));
        }
        while let Some(Reverse((_, i))) = heap.pop() {
            let l = best[i];
            if l >= crest {
                break;
            }
            if below.contains(&i) {
                (spill_level, spill_at) = (l, Some(saddle[i]));
                break;
            }
            for m in neighbours8(n, i).filter(|&m| !is_new[m]) {
                let nl = l.max(pass(i, m));
                if nl < best[m] {
                    best[m] = nl;
                    saddle[m] = if nl > l { m } else { saddle[i] };
                    heap.push(key(nl, m));
                }
            }
        }
    }
    let level = crest.min(spill_level);
    let spill_at = spill_at.filter(|_| spill_level < crest).map(|i| (i % n, i / n));
    let mut seen = vec![false; n * n];
    let mut stack: Vec<usize> = seeds.iter().copied().filter(|&m| bed[m] < level).collect();
    stack.iter().for_each(|&m| seen[m] = true);
    let mut flooded = Vec::new();
    while let Some(i) = stack.pop() {
        flooded.push(i);
        for m in neighbours8(n, i) {
            if !seen[m] && !is_new[m] && pass(i, m) < level {
                seen[m] = true;
                stack.push(m);
            }
        }
    }
    flooded.sort_unstable();
    let veg = state.ecology.vegetation.as_slice();
    let occ = water.structures.as_slice();
    Ok(DamPreview {
        level,
        pond_area: flooded.len(),
        volume: flooded.iter().map(|&i| level - bed[i]).sum(),
        flooded_trees: flooded.iter().filter(|&&i| veg[i] > TREE_VEGETATION).count(),
        flooded_buildings: flooded.iter().filter(|&&i| occ[i]).count(),
        reaches_map_edge: flooded.iter().any(|&i| i % n == 0 || i % n == n - 1 || i / n == 0 || i / n == n - 1),
        flooded_cells: flooded.iter().map(|&i| (i % n, i / n)).collect(),
        crest,
        spill_at,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn quiet(s: &mut SimState) {
        s.water = Water::new(s.height.width());
        s.water.config = WaterConfig {
            rain: 0.0, evaporation: 0.0, seepage: 0.0, dam_decay_per_day: 0.0,
            overtop_erosion: 0.0, open_edges: false, ..Default::default()
        };
    }

    fn valley() -> SimState {
        let mut s = SimState::new(1);
        let n = s.height.width();
        for y in 0..n {
            for x in 0..n {
                let dy = (y as f32 - 64.0).abs();
                *s.height.get_mut(x, y).unwrap() = 0.2 * (n - x) as f32 + 0.15 * dy;
            }
        }
        quiet(&mut s);
        s
    }

    fn dam_row() -> Vec<Cell> { (0..128).map(|y| (40, y)).collect() }

    fn wet_cells(s: &SimState) -> usize {
        s.water.depth.as_slice().iter().filter(|&&d| d > 0.02).count()
    }

    fn pour(s: &mut SimState, volume: f32) {
        for x in 25..40 { *s.water.depth.get_mut(x, 64).unwrap() = volume / 15.0; }
    }

    #[test]
    fn water_mass_conserved_on_closed_map() {
        let mut s = SimState::new(3);
        quiet(&mut s);
        for (i, d) in s.water.depth.as_mut_slice().iter_mut().enumerate() {
            *d = if i % 7 == 0 { 1.5 } else { 0.0 };
        }
        let before = s.water.total_volume();
        for _ in 0..300 { s.step(0.01).unwrap(); }
        let after = s.water.total_volume();
        assert!(((after - before) / before).abs() < 0.01, "{before} -> {after}");
        assert!(s.water.depth.as_slice().iter().all(|d| d.is_finite() && *d >= 0.0));
    }

    #[test]
    fn water_accounting_matches_volume_with_inflow_and_climate() {
        let mut s = SimState::new(5);
        s.water.add_source(10, 10, 5.0).unwrap();
        let start = s.water.total_volume();
        for _ in 0..500 { s.step(0.05).unwrap(); }
        let w = &s.water;
        let expect = start + w.added_volume - w.lost_volume;
        assert!(((w.total_volume() - expect) / expect.abs().max(1.0)).abs() < 0.01);
    }

    #[test]
    fn water_dam_pond_matches_preview() {
        let mut s = valley();
        let cells = dam_row();
        let prev = preview_dam(&s, &cells, 2.0).unwrap();
        assert!(prev.pond_area > 100 && !prev.reaches_map_edge);
        pour(&mut s, prev.volume);
        s.water.build_dam(&cells, 2.0).unwrap();
        for _ in 0..3000 { s.step(0.01).unwrap(); }
        assert!(s.water.has_dam(40, 64), "dam must survive");
        let area = wet_cells(&s) as f32;
        let ratio = area / prev.pond_area as f32;
        assert!((0.9..=1.1).contains(&ratio), "sim {area} vs preview {}", prev.pond_area);
        assert!(((s.water.total_volume() as f32 - prev.volume) / prev.volume).abs() < 0.01);
    }

    #[test]
    fn water_preview_does_not_mutate_and_counts_assets() {
        let mut s = valley();
        s.ecology.vegetation.as_mut_slice().fill(0.9);
        *s.water.structures.get_mut(35, 64).unwrap() = true;
        let before = (s.water.dam_height.clone(), s.water.depth.clone(), s.height.clone());
        let p = preview_dam(&s, &dam_row(), 2.0).unwrap();
        assert_eq!(before, (s.water.dam_height.clone(), s.water.depth.clone(), s.height.clone()));
        assert_eq!(p.flooded_trees, p.pond_area);
        assert_eq!(p.flooded_buildings, 1);
        assert_eq!(p.flooded_cells.len(), p.pond_area);
        assert!(p.flooded_cells.contains(&(35, 64)));
    }

    #[test]
    fn water_overtopping_and_decay_breach_dams() {
        let mut s = valley();
        s.water.config.overtop_erosion = 5.0;
        s.water.build_dam(&dam_row(), 2.0).unwrap();
        s.water.add_source(5, 64, 400_000.0).unwrap();
        for _ in 0..4000 { s.step(0.01).unwrap(); if !s.water.has_dam(40, 64) { break; } }
        assert!(!s.water.has_dam(40, 64), "overtopped dam should breach");

        let mut s = valley();
        s.water.config.dam_decay_per_day = 0.1;
        s.water.build_dam(&[(40, 64)], 2.0).unwrap();
        for _ in 0..5 { s.step(1.0).unwrap(); }
        assert!(s.water.has_dam(40, 64));
        for _ in 0..6 { s.step(1.0).unwrap(); }
        assert!(!s.water.has_dam(40, 64));
    }

    #[test]
    fn water_canal_lowers_terrain_and_carries_flow() {
        let mut s = SimState::new(1);
        quiet(&mut s);
        s.height.as_mut_slice().fill(5.0);
        let cells: Vec<Cell> = (10..30).map(|x| (x, 10)).collect();
        s.water.dig_canal(&mut s.height, &cells, 1.0).unwrap();
        assert_eq!(s.height.get(15, 10), Some(&4.0));
        assert_eq!(s.water.canal.get(15, 10), Some(&true));
        *s.water.depth.get_mut(10, 10).unwrap() = 2.0;
        for _ in 0..400 { s.step(0.01).unwrap(); }
        assert!(*s.water.depth.get(29, 10).unwrap() > 0.01);
        assert!(*s.water.depth.get(29, 12).unwrap() < 1e-3);
    }

    #[test]
    fn water_seasons_modulate_inflow_and_freeze() {
        let d = SEASON_DAYS;
        assert_eq!(Season::at(0.0), Season::Spring);
        assert_eq!(Season::at(d + 5.0), Season::Summer);
        assert_eq!(Season::at(2.0 * d + 5.0), Season::Autumn);
        assert_eq!(Season::at(3.0 * d + 5.0), Season::Winter);
        assert_eq!(Season::at(4.0 * d + 5.0), Season::Spring);
        assert!(Season::Spring.inflow_mult() > 2.0 * Season::Summer.inflow_mult());

        let mut s = SimState::new(1);
        quiet(&mut s);
        s.height.as_mut_slice().fill(0.0);
        s.water.depth.as_mut_slice().fill(0.5);
        s.step(SEASON_DAYS as f32 * 3.0 + 1.0).unwrap();
        for _ in 0..20 { s.step(1.0).unwrap(); }
        assert_eq!(s.water.season(), Season::Winter);
        assert!(*s.water.ice.get(5, 5).unwrap() > 0.5);
        while s.water.time_days < 4.0 * SEASON_DAYS + 20.0 { s.step(1.0).unwrap(); }
        assert_eq!(s.water.season(), Season::Spring);
        assert_eq!(*s.water.ice.get(5, 5).unwrap(), 0.0);
    }

    #[test]
    fn water_invalid_input_is_rejected() {
        let mut s = valley();
        assert_eq!(s.water.build_dam(&[], 1.0), Err(WaterError::NoCells));
        assert_eq!(s.water.build_dam(&[(500, 0)], 1.0), Err(WaterError::OutOfBounds { x: 500, y: 0 }));
        assert!(s.water.build_dam(&[(1, 1)], f32::NAN).is_err());
        assert!(s.water.build_dam(&[(1, 1), (999, 1)], 1.0).is_err());
        assert!(!s.water.has_dam(1, 1), "failed build must not partially apply");
        assert!(s.water.add_source(1, 1, -1.0).is_err());
        assert!(s.water.dig_canal(&mut s.height, &[(1, 1)], 0.0).is_err());
        assert!(preview_dam(&s, &[(1, 1)], 0.0).is_err());
        assert!(preview_dam(&s, &[], 1.0).is_err());
    }

    fn inflow_valley(km2: f32) -> World {
        let n = 64;
        let raw: Vec<f32> = (0..n * n)
            .map(|i| {
                let (x, y) = ((i % n) as f32, (i / n) as f32);
                4.0 + 0.04 * (n as f32 - 1.0 - x) + 0.25 * (y - 32.0).abs()
            })
            .collect();
        let real = crate::worldgen::RealMap { cell_m: 4.0, inflows: &[(0, 32, km2)], cover: None };
        crate::worldgen::from_heights(9, n, raw, &real).unwrap()
    }

    #[test]
    fn water_budget_balances_at_steady_state_and_the_river_leaves_as_it_came() {
        let world = inflow_valley(2.0);
        let inflow = world.water_inputs.iter().find(|i| i.kind == InputKind::Inflow).expect("the river from beyond the map");
        assert_eq!(inflow.cell, (0, 32), "fed at its border cell");
        assert!(world.water_inputs.iter().filter(|i| i.kind != InputKind::Inflow).all(|i| world.river.get(i.cell.0, i.cell.1) == Some(&true)), "water from the land enters only along the river");
        let mut w = Water::from_world(&world);
        assert!(*w.closed_edge.get(0, 32).unwrap() && !*w.closed_edge.get(63, 32).unwrap(), "the inflow edge is closed, the outlet open");
        assert_eq!(w.sources[0].rate, crate::worldgen::inflow_rate(2.0, 4.0));
        for _ in 0..800 {
            w.step(&world.height, 0.05);
        }
        w.reset_books();
        let (v0, days) = (w.total_volume(), 20.0);
        for _ in 0..(days / 0.05) as usize {
            w.step(&world.height, 0.05);
        }
        let b = w.budget();
        let stored = w.total_volume() - v0;
        assert!(((b.added() - b.lost() - stored) / b.added()).abs() < 1e-3, "books balance: {b:?}, stored {stored}");
        assert!(b.rain == 0.0, "the ground soaks up the rain: {b:?}");
        let (inn, out) = (b.sources, b.edges + b.evaporation + b.seepage);
        assert!((inn - out).abs() < 0.05 * inn, "steady: in {:.2}/day, out {:.2}/day ({b:?})", inn / days as f64, out / days as f64);
        assert!(b.edges >= 0.8 * inn, "the river leaves with what came in: edges {:.2} of {:.2}/day", b.edges / days as f64, inn / days as f64);
        assert!(b.edges >= 0.99 * inn, "all of it leaves down the river: {:.3} of {:.3}/day", b.edges / days as f64, inn / days as f64);
    }
}
