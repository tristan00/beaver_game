use crate::SimState;
use crate::grid::{Grid, HeightGrid, WaterGrid};
use crate::worldgen::World;
use std::cmp::Reverse;
use std::collections::BinaryHeap;
use std::fmt;
pub use crate::tuning::water::*;

pub type Cell = (usize, usize);

const DAY_S: f64 = 86_400.0;
mod gang;
pub mod kernels;
pub use kernels::bench_faces;
mod view;
use kernels::{FACE_DRY, G};
use view::{BandOut, SendPtr, View};
const CFL: f32 = 0.7;
const DT_MAX_S: f32 = 10.0;
const EDGE_SLOPE_MIN: f32 = 1.0e-3;
const MOVING: f32 = 0.02;

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
    pub manning: f32,
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
            manning: MANNING_CHANNEL,
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

/// Where the water engine spends its time, accumulated since the last `reset_timings`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct WaterTimings {
    pub faces_s: f64,
    pub faces_x_s: f64,
    pub limiter_s: f64,
    pub continuity_s: f64,
    pub pools_s: f64,
    pub activity_s: f64,
    pub publish_s: f64,
    pub upkeep_s: f64,
    pub substeps: u64,
    pub cell_updates: u64,
    pub physics_s: f64,
    pub pool_cells: u64,
    pub pool_samples: u64,
}

impl WaterTimings {
    pub fn total_s(&self) -> f64 {
        self.faces_s + self.limiter_s + self.continuity_s + self.pools_s + self.activity_s + self.publish_s + self.upkeep_s
    }
    pub fn report(&self, wall_s: f64) -> String {
        let t = self.total_s().max(1e-12);
        let pct = |v: f64| 100.0 * v / t;
        format!(
            "water {:.1} ms ({:.0}% of wall): faces {:.0}% (x {:.0}%) limiter {:.0}% continuity {:.0}% pools {:.0}% activity {:.0}% publish {:.0}% upkeep {:.0}% | {} substeps, mean dt {:.3} s, {:.0} cells/substep, {:.0} cells in pools, {:.2} ns/cell-update, physics x{:.0} real time",
            t * 1e3,
            100.0 * t / wall_s.max(1e-12),
            pct(self.faces_s),
            pct(self.faces_x_s),
            pct(self.limiter_s),
            pct(self.continuity_s),
            pct(self.pools_s),
            pct(self.activity_s),
            pct(self.publish_s),
            pct(self.upkeep_s),
            self.substeps,
            self.physics_s / self.substeps.max(1) as f64,
            self.cell_updates as f64 / self.substeps.max(1) as f64,
            self.pool_cells as f64 / self.pool_samples.max(1) as f64,
            1e9 * (self.faces_s + self.limiter_s + self.continuity_s) / self.cell_updates.max(1) as f64,
            self.physics_s / t,
        )
    }
}

#[derive(Clone, Copy, Debug)]
struct RimFace {
    face: u32,
    is_x: bool,
    /// +1 when the member sits on the +x/+y side (positive discharge flows into it), else -1.
    sign: f32,
    a: u32,
    b: u32,
}

/// Still, flat water (a pond, a wetland, a lake behind a dam) held as one level instead of being
/// stepped cell by cell: only the faces on its rim are computed, and it never limits the step.
#[derive(Clone, Debug)]
struct Pool {
    level: f32,
    /// Stored water, in metres of depth summed over cells (like `total_volume`).
    volume: f64,
    /// Member beds, ascending, and their running sums, for level from volume.
    beds: Vec<f32>,
    prefix: Vec<f64>,
    wet: usize,
    members: Vec<u32>,
    rim: Vec<u32>,
    /// Faces on the rim, with the cells on either side (u32::MAX off the map).
    faces: Vec<RimFace>,
    source: f32,
}

impl Pool {
    fn level_for(&mut self, volume: f64) -> f32 {
        let m = self.beds.len();
        let mut k = self.wet.clamp(1, m);
        let lvl = |k: usize| ((volume + self.prefix[k]) / k as f64) as f32;
        let mut l = lvl(k);
        while k < m && l > self.beds[k] {
            k += 1;
            l = lvl(k);
        }
        while k > 1 && l < self.beds[k - 1] {
            k -= 1;
            l = lvl(k);
        }
        self.wet = k;
        if volume <= 0.0 { self.beds[0] } else { l }
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
    pub velocity: Grid<(f32, f32)>,
    pub lake_level: Grid<f32>,
    pub sources: Vec<Source>,
    pub config: WaterConfig,
    pub time_days: f64,
    pub added_volume: f64,
    pub lost_volume: f64,
    pub cell_m: f32,
    pub physics_seconds: f64,
    pub steps_taken: u64,
    pub timings: WaterTimings,
    pub pools_enabled: bool,
    books: Budget,
    n: usize,
    h: Vec<f32>,
    z: Vec<f32>,
    qx: Vec<f32>,
    qy: Vec<f32>,
    qx_new: Vec<f32>,
    qy_new: Vec<f32>,
    ox_base: Vec<f32>,
    oy_base: Vec<f32>,
    ox: Vec<f32>,
    oy: Vec<f32>,
    fac: Vec<f32>,
    src: Vec<f32>,
    lake_delta: Vec<f32>,
    member: Vec<f32>,
    /// 1 for cells the solver steps this substep, else 0: water crosses a face only between two.
    act: Vec<f32>,
    /// Scratch for new depths (the mass kernel may compute a lane twice).
    h_new: Vec<f32>,
    rim_mask: Vec<bool>,
    pools: Vec<Pool>,
    src_season: Option<Season>,
    src_count: usize,
    rows: Vec<Vec<(u32, u32)>>,
    yrows: Vec<Vec<(u32, u32)>>,
    wet_rows: Vec<Vec<(u32, u32)>>,
    lake_rows: Vec<bool>,
    lake_inner: Vec<bool>,
    h_max: f32,
    masks_dirty: bool,
    /// Rows stepped at the last publish; cells outside them and the current rows did not change.
    pub_rows: Vec<Vec<(u32, u32)>>,
    full_publish: bool,
    dam_cells: Vec<u32>,
    ice_any: bool,
    since_shrink: u32,
    since_pools: f64,
}

/// Below this many active cells a substep runs on one thread (the fork-join costs more).
const PARALLEL_CELLS: u64 = 3000;
const ACTIVE_GAP: u32 = 16;
/// Depth below which a cell does not keep the cells around it computed (a damp film).
const WET_SEED: f32 = 1.0e-3;
/// Recompute the active area from scratch (letting it shrink) this often.
const SHRINK_EVERY: u32 = 256;
/// Pools are found again this often (seconds of physics).
const POOL_EVERY_S: f64 = 300.0;
const POOL_MIN_DEPTH: f32 = 0.03;
const POOL_MAX_SPEED: f32 = 0.08;
const POOL_FLAT_FACE: f32 = 1.5e-3;
const POOL_FLAT_RANGE: f32 = 1.5e-2;
const POOL_MIN_CELLS: usize = 6;
/// Pools at least this large pay for their rim; smaller ones only when deeper than POOL_DEEP_M.
const POOL_CHEAP_CELLS: usize = 64;
const POOL_DEEP_M: f32 = 0.2;

impl Water {
    pub fn new(size: usize) -> Self {
        let n = size;
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
            velocity: Grid::new(size, size, (0.0, 0.0)),
            lake_level: Grid::new(size, size, f32::NAN),
            sources: Vec::new(),
            config: WaterConfig::default(),
            time_days: 0.0,
            added_volume: 0.0,
            lost_volume: 0.0,
            cell_m: 1.0,
            physics_seconds: 0.0,
            steps_taken: 0,
            timings: WaterTimings::default(),
            pools_enabled: std::env::var_os("WATER_NOPOOLS").is_none(),
            books: Budget::default(),
            n,
            h: vec![0.0; n * n],
            z: vec![0.0; n * n],
            qx: vec![0.0; n * (n + 1)],
            qy: vec![0.0; (n + 1) * n],
            qx_new: vec![0.0; n * (n + 1)],
            qy_new: vec![0.0; (n + 1) * n],
            ox_base: vec![0.0; n * (n + 1)],
            oy_base: vec![0.0; (n + 1) * n],
            ox: vec![0.0; n * (n + 1)],
            oy: vec![0.0; (n + 1) * n],
            fac: vec![1.0; n * n],
            src: vec![0.0; n * n],
            lake_delta: vec![0.0; n * n],
            member: vec![0.0; n * n],
            act: vec![0.0; n * n],
            h_new: vec![0.0; n * n],
            rim_mask: vec![false; n * n],
            pools: Vec::new(),
            src_season: None,
            src_count: usize::MAX,
            rows: vec![Vec::new(); n],
            yrows: vec![Vec::new(); n + 1],
            wet_rows: vec![Vec::new(); n],
            lake_rows: vec![false; n],
            lake_inner: vec![false; n * n],
            h_max: 0.0,
            masks_dirty: true,
            pub_rows: vec![Vec::new(); n],
            full_publish: true,
            dam_cells: Vec::new(),
            ice_any: false,
            since_shrink: 0,
            since_pools: 0.0,
        }
    }

    pub fn from_world(world: &World) -> Self {
        let n = world.size;
        let mut w = Self::new(n);
        w.cell_m = world.cell_m;
        w.sources = sources_for(world);
        let inflows: Vec<(usize, usize, f32)> =
            world.water_inputs.iter().filter(|i| i.kind == crate::worldgen::InputKind::Inflow).map(|i| (i.cell.0, i.cell.1, 1.0)).collect();
        for (c, closed) in w.closed_edge.as_mut_slice().iter_mut().zip(crate::worldgen::inflow_walls(n, world.cell_m, &inflows)) {
            *c = closed;
        }
        for (f, &l) in w.fed.as_mut_slice().iter_mut().zip(world.lake.as_slice()) {
            *f = l;
        }
        w.lake_level = world.lake_level.clone();
        let (raw, surface) = (world.height.as_slice(), world.surface.as_slice());
        for i in 0..n * n {
            let lvl = w.lake_level.as_slice()[i];
            w.h[i] = if lvl.is_finite() { (lvl - raw[i]).max(0.0) } else if world.river.as_slice()[i] || world.lake.as_slice()[i] { (surface[i] - raw[i]).max(0.0) } else { 0.0 };
        }
        w.fill_fed_hollows(world);
        w.load_bed(&world.height);
        w.run_for(SPIN_UP_S, false);
        w.publish();
        w.reset_books();
        w
    }

    fn fill_fed_hollows(&mut self, world: &World) {
        let n = world.size;
        let (raw, surface) = (world.height.as_slice(), world.surface.as_slice());
        let hollow: Vec<bool> = (0..n * n).map(|i| surface[i] - raw[i] > 1.0e-3 && self.lake_level.as_slice()[i].is_nan()).collect();
        let mut seen = vec![false; n * n];
        for s in 0..n * n {
            if !hollow[s] || seen[s] {
                continue;
            }
            let mut comp = vec![s];
            seen[s] = true;
            let mut k = 0;
            while k < comp.len() {
                let c = comp[k];
                k += 1;
                for j in neighbours4(n, c) {
                    if hollow[j] && !seen[j] {
                        seen[j] = true;
                        comp.push(j);
                    }
                }
            }
            if comp.iter().any(|&i| world.river.as_slice()[i] || world.lake.as_slice()[i]) {
                for &i in &comp {
                    self.h[i] = surface[i] - raw[i];
                }
            }
        }
    }

    fn area(&self) -> f64 { (self.cell_m * self.cell_m) as f64 }

    pub fn reset_books(&mut self) {
        (self.added_volume, self.lost_volume) = (0.0, 0.0);
        self.books = Budget::default();
    }

    pub fn reset_timings(&mut self) {
        self.timings = WaterTimings::default();
    }

    pub fn budget(&self) -> Budget {
        self.books
    }

    pub fn season(&self) -> Season { Season::at(self.time_days) }

    /// Stored water in metres of depth summed over cells.
    pub fn total_volume(&self) -> f64 {
        let mut v: f64 = (0..self.h.len()).filter(|&i| self.member[i] == 0.0).map(|i| self.h[i] as f64).sum();
        v += self.pools.iter().map(|p| p.volume).sum::<f64>();
        v
    }

    pub fn active_segments(&self) -> usize {
        self.rows.iter().map(|r| r.len()).sum()
    }

    pub fn active_cells(&self) -> usize {
        self.rows.iter().flatten().map(|&(a, b)| (b - a) as usize).sum()
    }

    /// The deepest cells the solver steps (not lakes, not pools): what sets the time step.
    pub fn deepest_stepped(&self, k: usize) -> Vec<(usize, usize, f32)> {
        let n = self.n;
        let lake = self.lake_level.as_slice();
        let mut v: Vec<(usize, usize, f32)> = (0..n * n).filter(|&i| self.member[i] == 0.0 && !lake[i].is_finite() && self.h[i] > 0.0).map(|i| (i % n, i / n, self.h[i])).collect();
        v.sort_by(|a, b| b.2.total_cmp(&a.2));
        v.truncate(k);
        v
    }

    /// (pools, rim faces, member cells).
    pub fn pool_stats(&self) -> (usize, usize, usize) {
        (self.pools.len(), self.pools.iter().map(|p| p.faces.len()).sum(), self.pooled_cells())
    }

    pub fn pooled_cells(&self) -> usize {
        self.pools.iter().map(|p| p.members.len()).sum()
    }

    pub fn flow_at(&self, x: usize, y: usize) -> (f32, f32) {
        let Some(&(vx, vy)) = self.velocity.get(x, y) else { return (0.0, 0.0) };
        let h = self.depth.get(x, y).copied().unwrap_or(0.0);
        let scale = h * self.cell_m * DAY_S as f32 / self.area() as f32;
        (vx * scale, vy * scale)
    }

    pub fn downstream(&self, x: usize, y: usize) -> Option<Cell> {
        let n = self.n;
        if x >= n || y >= n {
            return None;
        }
        let (u, v) = self.velocity.as_slice()[y * n + x];
        if (u * u + v * v).sqrt() < 1.0e-4 {
            return None;
        }
        let (dx, dy) = ((u / (u.abs().max(v.abs()))).round() as i64, (v / (u.abs().max(v.abs()))).round() as i64);
        let (tx, ty) = (x as i64 + dx, y as i64 + dy);
        (tx >= 0 && ty >= 0 && (tx as usize) < n && (ty as usize) < n).then_some((tx as usize, ty as usize))
    }

    pub fn in_lake(&self, x: usize, y: usize) -> bool {
        self.lake_level.get(x, y).is_some_and(|l| l.is_finite())
    }

    pub fn has_dam(&self, x: usize, y: usize) -> bool {
        self.dam_height.get(x, y).is_some_and(|&h| h > 0.0)
    }

    pub(crate) fn check(&self, x: usize, y: usize) -> Result<usize, WaterError> {
        if self.depth.in_bounds(x, y) { Ok(y * self.depth.width() + x) } else { Err(WaterError::OutOfBounds { x, y }) }
    }

    pub(crate) fn check_cells(&self, cells: &[Cell]) -> Result<Vec<usize>, WaterError> {
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
        self.load_bed(height);
        self.publish();
    }

    /// Bed = terrain + dams.
    fn load_bed(&mut self, height: &HeightGrid) {
        assert_eq!((height.width(), height.height()), (self.n, self.n), "terrain size must match water size");
        let dam = self.dam_height.as_slice();
        let mut changed = false;
        for (i, (t, d)) in height.as_slice().iter().zip(dam).enumerate() {
            let z = t + d;
            if z != self.z[i] {
                changed = true;
                self.z[i] = z;
            }
        }
        if changed {
            self.masks_dirty = true;
        }
    }

    /// Face masks: 1 where water may cross, 0 at closed edges and between two fixed-level lake cells.
    fn rebuild_masks(&mut self) {
        self.dissolve_pools();
        let n = self.n;
        let lake = self.lake_level.as_slice();
        let closed = self.closed_edge.as_slice();
        let open = self.config.open_edges;
        let w1 = n + 1;
        for y in 0..n {
            for x in 0..=n {
                let m = if x == 0 || x == n {
                    let c = y * n + if x == 0 { 0 } else { n - 1 };
                    open && !closed[c]
                } else {
                    !(lake[y * n + x - 1].is_finite() && lake[y * n + x].is_finite())
                };
                self.ox_base[y * w1 + x] = if m { 1.0 } else { 0.0 };
            }
        }
        for y in 0..=n {
            for x in 0..n {
                let m = if y == 0 || y == n {
                    let c = if y == 0 { x } else { (n - 1) * n + x };
                    open && !closed[c]
                } else {
                    !(lake[(y - 1) * n + x].is_finite() && lake[y * n + x].is_finite())
                };
                self.oy_base[y * n + x] = if m { 1.0 } else { 0.0 };
            }
        }
        self.ox.copy_from_slice(&self.ox_base);
        self.oy.copy_from_slice(&self.oy_base);
        for i in 0..n * n {
            self.lake_inner[i] = lake[i].is_finite() && neighbours4(n, i).count() == 4 && neighbours4(n, i).all(|j| lake[j].is_finite());
        }
        for y in 0..n {
            self.lake_rows[y] = (0..n).any(|x| lake[y * n + x].is_finite());
        }
        self.dam_cells = (0..n * n).filter(|&i| self.dam_height.as_slice()[i] > 0.0).map(|i| i as u32).collect();
        self.masks_dirty = false;
        self.full_publish = true;
        self.rebuild_activity_full();
    }

    fn refresh_sources(&mut self) {
        let season = self.season();
        if self.src_season == Some(season) && self.src_count == self.sources.len() {
            return;
        }
        self.src.fill(0.0);
        let n = self.n;
        for s in &self.sources {
            let rate = s.rate * (1.0 + s.seasonality * (season.inflow_mult() - 1.0));
            // Source rates are metres of depth per day on their cell; src is metres per second.
            self.src[s.y * n + s.x] += rate / DAY_S as f32;
        }
        for p in &mut self.pools {
            p.source = p.members.iter().map(|&i| self.src[i as usize]).sum();
        }
        self.src_season = Some(season);
        self.src_count = self.sources.len();
        self.rebuild_activity_full();
    }

    fn is_seed(&self, i: usize) -> bool {
        (self.h[i] > WET_SEED && (self.member[i] == 0.0 || self.rim_mask[i]) && !self.lake_inner[i]) || self.src[i] > 0.0 || self.rim_mask[i]
    }

    fn row_wet_runs(&self, y: usize) -> Vec<(u32, u32)> {
        let n = self.n;
        let mut runs: Vec<(u32, u32)> = Vec::new();
        for x in 0..n {
            if self.is_seed(y * n + x) {
                push_run(&mut runs, x as u32, x as u32 + 1);
            }
        }
        runs
    }

    /// Rows of cells to compute: every wet cell outside a pool, every source, every pool's rim,
    /// and their 4-neighbours.
    fn rebuild_activity_full(&mut self) {
        for y in 0..self.n {
            self.wet_rows[y] = self.row_wet_runs(y);
        }
        self.since_shrink = 0;
        self.rebuild_rows(0, self.n);
    }

    /// Recompute active rows y0..y1 (and the face rows around them) from the wet runs.
    fn rebuild_rows(&mut self, y0: usize, y1: usize) {
        let n = self.n as u32;
        for y in y0..y1 {
            let mut runs: Vec<(u32, u32)> = Vec::new();
            for yy in y.saturating_sub(1)..(y + 2).min(self.n) {
                for &(a, b) in &self.wet_rows[yy] {
                    runs.push((a.saturating_sub(1), (b + 1).min(n)));
                }
            }
            runs.sort_unstable();
            let w = self.n;
            for &(a, b) in &self.rows[y] {
                self.act[y * w + a as usize..y * w + b as usize].fill(0.0);
            }
            self.rows[y] = merge_runs(runs, ACTIVE_GAP);
            for &(a, b) in &self.rows[y] {
                self.act[y * w + a as usize..y * w + b as usize].fill(1.0);
            }
        }
        for y in y0..=y1.min(self.n) {
            let mut runs: Vec<(u32, u32)> = Vec::new();
            if y > 0 {
                runs.extend_from_slice(&self.rows[y - 1]);
            }
            if y < self.n {
                runs.extend_from_slice(&self.rows[y]);
            }
            runs.sort_unstable();
            self.yrows[y] = merge_runs(runs, 0);
        }
    }

    pub fn step(&mut self, height: &HeightGrid, dt_days: f32) {
        let t0 = std::time::Instant::now();
        self.load_bed(height);
        self.timings.upkeep_s += t0.elapsed().as_secs_f64();
        if dt_days <= 0.0 {
            self.publish();
            return;
        }
        self.run_for(dt_days as f64 * DAY_S, true);
        let t0 = std::time::Instant::now();
        self.update_dams(height, dt_days);
        self.update_ice(dt_days);
        self.time_days += dt_days as f64;
        self.timings.upkeep_s += t0.elapsed().as_secs_f64();
        self.publish();
    }

    /// Runs the physics for `seconds`, finding pools again (and applying rain and losses) every
    /// `POOL_EVERY_S`.
    fn run_for(&mut self, seconds: f64, losses: bool) {
        let mut t = 0.0;
        while t < seconds - 1e-6 {
            let chunk = (POOL_EVERY_S - self.since_pools).min(seconds - t).max(1e-3);
            let mut done = 0.0;
            while done < chunk - 1e-6 {
                done += self.substep((chunk - done) as f32) as f64;
            }
            t += done;
            self.since_pools += done;
            if self.since_pools >= POOL_EVERY_S - 1e-6 || t >= seconds - 1e-6 {
                let t0 = std::time::Instant::now();
                let old_members: Vec<u32> = self.pools.iter().flat_map(|p| p.members.iter().copied()).collect();
                self.dissolve_pools();
                if losses {
                    self.apply_rain_and_losses((self.since_pools / DAY_S) as f32);
                }
                if self.pools_enabled {
                    self.find_pools(&old_members);
                }
                self.rebuild_activity_full();
                self.full_publish = true;
                self.since_pools = 0.0;
                self.timings.pools_s += t0.elapsed().as_secs_f64();
            }
        }
    }

    #[cfg(test)]
    fn advance(&mut self, seconds: f64) {
        self.run_for(seconds, false);
    }

    /// Rain on open water, evaporation and seepage from wet ground (millimetres a day).
    fn apply_rain_and_losses(&mut self, dt_days: f32) {
        let season = self.season();
        let cfg = &self.config;
        let (r, e, s) = ((cfg.rain * season.rain_mult()) * dt_days, (cfg.evaporation * season.evap_mult()) * dt_days, cfg.seepage * dt_days);
        if (r == 0.0 && e == 0.0 && s == 0.0) || dt_days <= 0.0 {
            return;
        }
        let lake = self.lake_level.as_slice();
        let (mut rain, mut evap, mut seep) = (0.0f64, 0.0f64, 0.0f64);
        for i in 0..self.h.len() {
            let hh = self.h[i];
            if hh <= 0.0 || lake[i].is_finite() {
                continue;
            }
            let rr = if hh > WET_EPS { r } else { 0.0 };
            let ev = e.min(hh + rr);
            let sp = s.min(hh + rr - ev);
            self.h[i] = hh + rr - ev - sp;
            rain += rr as f64;
            evap += ev as f64;
            seep += sp as f64;
        }
        self.books.rain += rain;
        self.books.evaporation += evap;
        self.books.seepage += seep;
        self.added_volume += rain;
        self.lost_volume += evap + seep;
    }

    /// Hands every pool's water back to its cells as ordinary still water.
    fn dissolve_pools(&mut self) {
        let n = self.n;
        let w1 = n + 1;
        for p in std::mem::take(&mut self.pools) {
            for &i in &p.members {
                let i = i as usize;
                self.h[i] = (p.level - self.z[i]).max(0.0);
                self.member[i] = 0.0;
                self.rim_mask[i] = false;
                let (x, y) = (i % n, i / n);
                for f in [y * w1 + x, y * w1 + x + 1] {
                    self.ox[f] = self.ox_base[f];
                }
                for f in [y * n + x, (y + 1) * n + x] {
                    self.oy[f] = self.oy_base[f];
                }
            }
            // Rounding: put the pool's exact volume back (into its deepest cell).
            let back: f64 = p.members.iter().map(|&i| self.h[i as usize] as f64).sum();
            if let Some(&deep) = p.members.iter().min_by(|&&a, &&b| self.z[a as usize].total_cmp(&self.z[b as usize])) {
                let d = &mut self.h[deep as usize];
                *d = (*d as f64 + p.volume - back).max(0.0) as f32;
            }
        }
    }

    /// Finds still, flat water and turns each large enough body into a pool.
    fn find_pools(&mut self, old_members: &[u32]) {
        let n = self.n;
        let w1 = n + 1;
        let lake = self.lake_level.as_slice();
        let (h, z, qx, qy) = (&self.h, &self.z, &self.qx, &self.qy);
        let eta = |i: usize| z[i] + h[i];
        // Only water the solver steps (or that was pooled) can be still and deep.
        let mut cand = vec![false; n * n];
        let mut check: Vec<usize> = old_members.iter().map(|&i| i as usize).collect();
        for y in 0..n {
            for &(a, b) in &self.rows[y] {
                check.extend((a as usize..b as usize).map(|x| y * n + x));
            }
        }
        let test = |i: usize| -> bool {
                let hh = h[i];
                if hh <= POOL_MIN_DEPTH || lake[i].is_finite() {
                    return false;
                }
                let (x, y) = (i % n, i / n);
                let q = qx[y * w1 + x].abs().max(qx[y * w1 + x + 1].abs()).max(qy[y * n + x].abs()).max(qy[(y + 1) * n + x].abs());
                if q > POOL_MAX_SPEED * hh {
                    return false;
                }
                // Only neighbours water can actually cross to matter (a damp film on a dam crest
                // or the bank does not).
                neighbours4(n, i).all(|j| eta(i).max(eta(j)) - z[i].max(z[j]) <= WET_SEED || (eta(j) - eta(i)).abs() < POOL_FLAT_FACE)
        };
        for &i in &check {
            cand[i] = test(i);
        }
        if std::env::var_os("WATER_POOL_DEBUG").is_some() {
            let deep: Vec<usize> = (0..n * n).filter(|&i| h[i] > POOL_MIN_DEPTH && !lake[i].is_finite()).collect();
            let slow = deep.iter().filter(|&&i| {
                let (x, y) = (i % n, i / n);
                let q = qx[y * w1 + x].abs().max(qx[y * w1 + x + 1].abs()).max(qy[y * n + x].abs()).max(qy[(y + 1) * n + x].abs());
                q <= POOL_MAX_SPEED * h[i]
            }).count();
            let c = cand.iter().filter(|&&c| c).count();
            let mut speeds: Vec<f32> = deep.iter().map(|&i| {
                let (x, y) = (i % n, i / n);
                qx[y * w1 + x].abs().max(qx[y * w1 + x + 1].abs()).max(qy[y * n + x].abs()).max(qy[(y + 1) * n + x].abs()) / h[i]
            }).collect();
            speeds.sort_by(|a, b| a.total_cmp(b));
            let pc = |p: usize| speeds.get(speeds.len().saturating_sub(1) * p / 100).copied().unwrap_or(0.0);
            let mut dl: Vec<f32> = deep.iter().map(|&i| neighbours4(n, i).filter(|&j| h[j] > 0.0 && eta(j) >= z[i]).map(|j| (eta(j) - eta(i)).abs()).fold(0.0, f32::max)).collect();
            dl.sort_by(|a, b| a.total_cmp(b));
            let pd = |p: usize| dl.get(dl.len().saturating_sub(1) * p / 100).copied().unwrap_or(0.0);
            eprintln!("  level step to neighbours p25 {:.1e} p50 {:.1e} p75 {:.1e} p90 {:.1e}", pd(25), pd(50), pd(75), pd(90));
            eprintln!("pools: deep {} slow {} candidates {} | speed p25 {:.4} p50 {:.4} p75 {:.4} p90 {:.4}", deep.len(), slow, c, pc(25), pc(50), pc(75), pc(90));
        }
        if let Ok(v) = std::env::var("WATER_POOL_CELL") {
            let xy: Vec<usize> = v.split(',').filter_map(|t| t.parse().ok()).collect();
            if xy.len() == 2 {
                let i = xy[1] * n + xy[0];
                let (x, y) = (xy[0], xy[1]);
                eprintln!(
                    "cell {:?}: h {:.3} eta {:.4} member {} cand {} q w {:.4} e {:.4} n {:.4} s {:.4}",
                    (x, y), h[i], eta(i), self.member[i], cand[i], qx[y * w1 + x], qx[y * w1 + x + 1], qy[y * n + x], qy[(y + 1) * n + x]
                );
                for j in neighbours4(n, i) {
                    eprintln!("   nb {:?}: h {:.3} eta {:.4} z {:.3} cand {}", (j % n, j / n), h[j], eta(j), z[j], cand[j]);
                }
            }
        }
        let mut seen = vec![false; n * n];
        let mut found: Vec<Vec<u32>> = Vec::new();
        for s in 0..n * n {
            if !cand[s] || seen[s] {
                continue;
            }
            let base = eta(s);
            let mut comp = vec![s as u32];
            seen[s] = true;
            let mut k = 0;
            while k < comp.len() {
                let c = comp[k] as usize;
                k += 1;
                for j in neighbours4(n, c) {
                    if cand[j] && !seen[j] && (eta(j) - base).abs() < POOL_FLAT_RANGE {
                        seen[j] = true;
                        comp.push(j as u32);
                    }
                }
            }
            // A small pool costs as much at its rim as it saves inside; keep one only when it is
            // deep enough to hold the time step down.
            let deepest = comp.iter().map(|&i| h[i as usize]).fold(0.0f32, f32::max);
            if comp.len() >= POOL_MIN_CELLS && (comp.len() >= POOL_CHEAP_CELLS || deepest >= POOL_DEEP_M) {
                found.push(comp);
            }
        }
        for members in found {
            let mut beds: Vec<f32> = members.iter().map(|&i| self.z[i as usize]).collect();
            beds.sort_by(|a, b| a.total_cmp(b));
            let mut prefix = vec![0.0f64; beds.len() + 1];
            for (k, &b) in beds.iter().enumerate() {
                prefix[k + 1] = prefix[k] + b as f64;
            }
            let volume: f64 = members.iter().map(|&i| self.h[i as usize] as f64).sum();
            let source = members.iter().map(|&i| self.src[i as usize]).sum();
            let mut p = Pool { level: 0.0, volume, beds, prefix, wet: members.len(), members, rim: Vec::new(), faces: Vec::new(), source };
            p.level = p.level_for(volume);
            let id = self.pools.len() as f32 + 1.0;
            for &i in &p.members {
                self.member[i as usize] = id;
            }
            for &i in &p.members {
                let i = i as usize;
                self.h[i] = (p.level - self.z[i]).max(0.0);
                let (x, y) = (i % n, i / n);
                let mut on_rim = false;
                let mut side = |f: usize, is_x: bool, other: Option<usize>, positive: bool, this: &mut Self| match other {
                    Some(j) if this.member[j] == id => {
                        if is_x { this.ox[f] = 0.0 } else { this.oy[f] = 0.0 }
                    }
                    _ => {
                        on_rim = true;
                        let o = other.map_or(u32::MAX, |j| j as u32);
                        let (a, b) = if positive { (o, i as u32) } else { (i as u32, o) };
                        p.faces.push(RimFace { face: f as u32, is_x, sign: if positive { 1.0 } else { -1.0 }, a, b });
                    }
                };
                side(y * w1 + x, true, (x > 0).then(|| i - 1), true, self);
                side(y * w1 + x + 1, true, (x + 1 < n).then(|| i + 1), false, self);
                side(y * n + x, false, (y > 0).then(|| i - n), true, self);
                side((y + 1) * n + x, false, (y + 1 < n).then(|| i + n), false, self);
                if on_rim {
                    p.rim.push(i as u32);
                    self.rim_mask[i] = true;
                }
            }
            self.pools.push(p);
        }
        // Faces inside pools carry nothing.
        for p in &self.pools {
            for &i in &p.members {
                let (x, y) = (i as usize % n, i as usize / n);
                for f in [y * w1 + x, y * w1 + x + 1] {
                    if self.ox[f] == 0.0 {
                        self.qx[f] = 0.0;
                    }
                }
                for f in [y * n + x, (y + 1) * n + x] {
                    if self.oy[f] == 0.0 {
                        self.qy[f] = 0.0;
                    }
                }
            }
        }
    }

    /// One explicit step of the local-inertial shallow-water equations (Bates et al. 2010):
    /// momentum per face from the water-surface slope with semi-implicit Manning friction, then
    /// mass per cell. Pools take what crosses their rim and hold one level. Returns the step in s.
    pub(crate) fn substep(&mut self, limit: f32) -> f32 {
        let _ftz = FlushDenormals::new();
        let t0 = std::time::Instant::now();
        if self.masks_dirty {
            self.rebuild_masks();
        }
        self.refresh_sources();
        let n = self.n;
        let dx = self.cell_m;
        let c = (G * self.h_max.max(0.01)).sqrt();
        let dt = (CFL * dx / c).min(DT_MAX_S).min(limit.max(1.0e-4));
        let p = kernels::Params { gdt: G * dt, fric: G * dt * self.config.manning * self.config.manning, inv_dx: 1.0 / dx };
        let k = dt / dx;

        // Split the rows into bands of about equal work, one per task.
        let cells: u64 = self.rows.iter().flatten().map(|&(a, b)| (b - a) as u64).sum();
        let gang = gang::gang();
        let tasks = if cells >= PARALLEL_CELLS { gang.width().min(n) } else { 1 };
        let mut bounds = Vec::with_capacity(tasks + 1);
        bounds.push(0usize);
        let mut acc = 0u64;
        for y in 0..n {
            acc += self.rows[y].iter().map(|&(a, b)| (b - a) as u64).sum::<u64>() + 1;
            let t = bounds.len();
            if t < tasks && acc * tasks as u64 >= (cells + n as u64) * t as u64 {
                bounds.push(y + 1);
            }
        }
        while bounds.len() <= tasks {
            bounds.push(n);
        }
        *bounds.last_mut().unwrap() = n;
        // Every band gets at least one row (pools are shared out by task).
        for t in 1..tasks {
            bounds[t] = bounds[t].max(bounds[t - 1] + 1).min(n - (tasks - t));
        }
        let mut outs = vec![BandOut::default(); tasks];
        let view = View::of(self);
        let run = |phase: &(dyn Fn(usize, usize, usize, &View) + Sync)| {
            if tasks == 1 {
                phase(0, 0, n, &view);
            } else {
                gang.run(&|t: usize| {
                    if t < tasks && bounds[t] < bounds[t + 1] {
                        phase(t, bounds[t], bounds[t + 1], &view);
                    }
                });
            }
        };
        self.timings.activity_s += t0.elapsed().as_secs_f64();

        let t0 = std::time::Instant::now();
        run(&|_, y0, y1, v| unsafe { v.faces(y0, y1, p, dx) });
        let t1 = std::time::Instant::now();
        self.timings.faces_s += (t1 - t0).as_secs_f64();
        run(&|_, y0, y1, v| unsafe { v.limiter(y0, y1, k, dt) });
        let t2 = std::time::Instant::now();
        self.timings.limiter_s += (t2 - t1).as_secs_f64();
        let outs_ptr = SendPtr(outs.as_mut_ptr());
        let cont = |t: usize, y0: usize, y1: usize, v: &View| unsafe {
            *outs_ptr.get().add(t) = v.continuity(y0, y1, k, dt);
            v.step_pools(t, tasks, k, dt);
        };
        run(&cont);
        let t3 = std::time::Instant::now();
        self.timings.continuity_s += (t3 - t2).as_secs_f64();
        let (mut hmax, mut lake_change, mut edge) = (0u32, 0.0f64, 0.0f64);
        let mut grew: Vec<usize> = Vec::new();
        for o in &outs {
            hmax = hmax.max(o.hmax);
            lake_change += o.lake_change;
            edge += o.edge;
            grew.extend_from_slice(&o.grew);
        }

        // (Pools were stepped inside the continuity phase, a share per task.)
        let pool_cells: u64 = self.pools.iter().map(|p| p.members.len() as u64).sum();
        let t4 = std::time::Instant::now();
        self.timings.pools_s += (t4 - t3).as_secs_f64();
        self.timings.pool_cells += pool_cells;
        self.timings.pool_samples += 1;

        // Activity: grow where water reached new cells; shrink now and then.
        self.since_shrink += 1;
        if self.since_shrink >= SHRINK_EVERY {
            self.rebuild_activity_full();
        } else if !grew.is_empty() {
            for &y in &grew {
                self.wet_rows[y] = self.row_wet_runs(y);
            }
            for &y in &grew {
                self.rebuild_rows(y.saturating_sub(1), (y + 2).min(n));
            }
        }
        self.h_max = f32::from_bits(hmax);
        self.timings.activity_s += t4.elapsed().as_secs_f64();

        let sourced = self.src_total() * dt as f64;
        let edge = edge * dt as f64 * dx as f64 / self.area();
        self.books.sources += sourced + lake_change.max(0.0);
        self.books.edges += edge + (-lake_change).max(0.0);
        self.added_volume += sourced + lake_change.max(0.0);
        self.lost_volume += edge + (-lake_change).max(0.0);
        self.physics_seconds += dt as f64;
        self.steps_taken += 1;
        self.timings.substeps += 1;
        self.timings.cell_updates += cells;
        self.timings.physics_s += dt as f64;
        dt
    }

    fn src_total(&self) -> f64 {
        let season = self.season();
        self.sources.iter().map(|s| (s.rate * (1.0 + s.seasonality * (season.inflow_mult() - 1.0))) as f64 / DAY_S).sum()
    }

    fn publish(&mut self) {
        let t0 = std::time::Instant::now();
        let n = self.n;
        for p in &self.pools {
            for &i in &p.members {
                self.h[i as usize] = (p.level - self.z[i as usize]).max(0.0);
            }
        }
        if std::mem::take(&mut self.full_publish) {
            for y in 0..n {
                self.publish_span(y, 0, n);
            }
        } else {
            for y in 0..n {
                for k in 0..self.rows[y].len() + self.pub_rows[y].len() {
                    let (a, b) = if k < self.rows[y].len() { self.rows[y][k] } else { self.pub_rows[y][k - self.rows[y].len()] };
                    self.publish_span(y, a as usize, b as usize);
                }
            }
            for pi in 0..self.pools.len() {
                for k in 0..self.pools[pi].members.len() {
                    let i = self.pools[pi].members[k] as usize;
                    self.publish_span(i / n, i % n, i % n + 1);
                }
            }
        }
        self.pub_rows.clone_from(&self.rows);
        self.timings.publish_s += t0.elapsed().as_secs_f64();
    }

    fn publish_span(&mut self, y: usize, a: usize, b: usize) {
        let n = self.n;
        let w1 = n + 1;
        let dx = self.cell_m;
        let lake = self.lake_level.as_slice();
        for x in a..b {
            let i = y * n + x;
            let h = self.h[i];
            let qxc = 0.5 * (self.qx[y * w1 + x] + self.qx[y * w1 + x + 1]);
            let qyc = 0.5 * (self.qy[y * n + x] + self.qy[(y + 1) * n + x]);
            let q = (qxc * qxc + qyc * qyc).sqrt();
            let (vx, vy) = if h > FACE_DRY { (qxc / h, qyc / h) } else { (0.0, 0.0) };
            self.depth.as_mut_slice()[i] = h;
            self.width.as_mut_slice()[i] = if h > 0.0 { dx } else { 0.0 };
            self.discharge.as_mut_slice()[i] = q * dx;
            self.velocity.as_mut_slice()[i] = (vx, vy);
            self.channel.as_mut_slice()[i] = h > 0.0 && (vx * vx + vy * vy).sqrt() > MOVING && lake[i].is_nan();
        }
    }

    fn update_ice(&mut self, dt_days: f32) {
        let winter = self.season() == Season::Winter;
        if !winter && !self.ice_any {
            return;
        }
        let (grow, melt) = (self.config.ice_growth_per_day * dt_days, self.config.ice_melt_per_day * dt_days);
        let mut any = false;
        for (ice, &d) in self.ice.as_mut_slice().iter_mut().zip(&self.h) {
            *ice = if winter && d > WET_EPS { (*ice + grow).min(MAX_ICE) } else { (*ice - melt).max(0.0) };
            any |= *ice > 0.0;
        }
        self.ice_any = any;
    }

    fn update_dams(&mut self, height: &HeightGrid, dt: f32) {
        let w = self.n;
        for p in &self.pools {
            for &i in &p.members {
                self.h[i as usize] = (p.level - self.z[i as usize]).max(0.0);
            }
        }
        let (terrain, dam, depth) = (height.as_slice(), self.dam_height.as_slice(), &self.h);
        let surface = |i: usize| if depth[i] > WET_EPS { terrain[i] + dam[i] + depth[i] } else { f32::NEG_INFINITY };
        let mut broken = Vec::new();
        for i in self.dam_cells.iter().map(|&i| i as usize).filter(|&i| dam[i] > 0.0) {
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
        if !broken.is_empty() {
            self.masks_dirty = true;
        }
        for i in broken {
            self.dam_height.as_mut_slice()[i] = 0.0;
            self.dam_integrity.as_mut_slice()[i] = 0.0;
            let z = &mut self.z[i];
            *z = height.as_slice()[i];
        }
    }
}

/// Tiny discharges decaying toward zero become denormal floats, which the CPU handles in slow
/// microcode; flush them to zero while the solver runs (restored afterwards).
struct FlushDenormals(#[allow(dead_code)] u32);

impl FlushDenormals {
    #[allow(deprecated)]
    fn new() -> Self {
        #[cfg(target_arch = "x86_64")]
        unsafe {
            use std::arch::x86_64::{_mm_getcsr, _mm_setcsr};
            let old = _mm_getcsr();
            _mm_setcsr(old | 0x8040);
            return FlushDenormals(old);
        }
        #[allow(unreachable_code)]
        FlushDenormals(0)
    }
}

impl Drop for FlushDenormals {
    #[allow(deprecated)]
    fn drop(&mut self) {
        #[cfg(target_arch = "x86_64")]
        unsafe {
            std::arch::x86_64::_mm_setcsr(self.0);
        }
    }
}

fn edge_bed(z: &[f32], c: usize, inner: usize, dx: f32) -> f32 {
    z[c] - ((z[inner] - z[c]) / dx).max(EDGE_SLOPE_MIN) * dx
}

fn push_run(runs: &mut Vec<(u32, u32)>, a: u32, b: u32) {
    match runs.last_mut() {
        Some(last) if a <= last.1 + ACTIVE_GAP => last.1 = last.1.max(b),
        _ => runs.push((a, b)),
    }
}

fn merge_runs(runs: Vec<(u32, u32)>, gap: u32) -> Vec<(u32, u32)> {
    let mut merged: Vec<(u32, u32)> = Vec::with_capacity(runs.len());
    for (a, b) in runs {
        match merged.last_mut() {
            Some(last) if a <= last.1 + gap => last.1 = last.1.max(b),
            _ => merged.push((a, b)),
        }
    }
    merged
}

const SPIN_UP_S: f64 = 9000.0;

#[allow(clippy::too_many_arguments)]
pub fn neighbours4(n: usize, i: usize) -> impl Iterator<Item = usize> {
    let (x, y) = (i % n, i / n);
    [(x > 0).then(|| i - 1), (x + 1 < n).then(|| i + 1), (y > 0).then(|| i - n), (y + 1 < n).then(|| i + n)].into_iter().flatten()
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

    fn tank(n: usize, bed: impl Fn(usize, usize) -> f32, manning: f32, open: bool) -> (Water, HeightGrid) {
        let mut w = Water::new(n);
        w.config = WaterConfig { rain: 0.0, evaporation: 0.0, seepage: 0.0, dam_decay_per_day: 0.0, open_edges: open, manning, ..WaterConfig::default() };
        let mut g = Grid::new(n, n, 0.0f32);
        for y in 0..n {
            for x in 0..n {
                *g.get_mut(x, y).unwrap() = bed(x, y);
            }
        }
        w.load_bed(&g);
        (w, g)
    }

    fn fill_to(w: &mut Water, level: f32) {
        for i in 0..w.h.len() {
            w.h[i] = (level - w.z[i]).max(0.0);
        }
        w.h_max = w.h.iter().copied().fold(0.0, f32::max);
    }

    fn run(w: &mut Water, seconds: f64) {
        w.advance(seconds);
        w.publish();
    }

    #[test]
    fn still_water_in_a_bowl_stays_level_and_still() {
        let (mut w, _) = tank(48, |x, y| 0.01 * ((x as f32 - 24.0).powi(2) + (y as f32 - 24.0).powi(2)), 0.03, false);
        fill_to(&mut w, 2.0);
        run(&mut w, 600.0);
        let speed = w.velocity.as_slice().iter().map(|v| (v.0 * v.0 + v.1 * v.1).sqrt()).fold(0.0, f32::max);
        let worst = (0..w.h.len()).filter(|&i| w.h[i] > 0.0).map(|i| (w.z[i] + w.h[i] - 2.0).abs()).fold(0.0, f32::max);
        assert!(speed < 1e-4 && worst < 1e-4, "speed {speed} m/s, level off by {worst} m");
    }

    #[test]
    fn a_closed_box_keeps_its_water_while_it_sloshes() {
        let (mut w, _) = tank(40, |x, _| 0.02 * x as f32, 0.03, false);
        for y in 0..40 {
            for x in 0..15 {
                w.h[y * 40 + x] = 1.0;
            }
        }
        w.h_max = 1.0;
        let before = w.total_volume();
        run(&mut w, 300.0);
        let after = w.total_volume();
        assert!(((after - before) / before).abs() < 1e-5, "{before} -> {after}");
    }

    #[test]
    fn a_small_wave_on_still_water_travels_at_the_shallow_water_speed() {
        let (n, h0, hump) = (200usize, 1.0f32, 20usize);
        let (mut w, _) = tank(n, |_, _| 0.0, 0.0, false);
        fill_to(&mut w, h0);
        for y in 0..n {
            for x in 0..hump {
                w.h[y * n + x] += 0.02;
            }
        }
        let gauge = 120usize;
        let (mut t, mut peak, mut at) = (0.0f64, 0.0f32, 0.0f64);
        while t < 80.0 {
            t += w.substep(0.05) as f64;
            let v = w.h[(n / 2) * n + gauge];
            if v > peak {
                (peak, at) = (v, t);
            }
        }
        let expect = gauge as f64 / (G * h0).sqrt() as f64;
        assert!((at - expect).abs() / expect < 0.1, "peak arrived at {at:.1} s, theory {expect:.1} s");
    }

    #[test]
    fn steady_flow_down_a_uniform_slope_settles_at_manning_normal_depth() {
        let (n, slope, manning, q_per_m) = (60usize, 0.01f32, 0.04f32, 0.02f32);
        let (mut w, _) = tank(n, |x, _| 5.0 - slope * x as f32, manning, false);
        for y in 0..n {
            w.closed_edge.as_mut_slice()[y * n + n - 1] = false;
            w.sources.push(Source { x: 0, y, rate: q_per_m * DAY_S as f32, seasonality: 0.0 });
        }
        w.config.open_edges = true;
        for x in 0..n - 1 {
            for &y in &[0usize, n - 1] {
                w.closed_edge.as_mut_slice()[y * n + x] = true;
            }
        }
        for y in 0..n {
            w.closed_edge.as_mut_slice()[y * n] = true;
        }
        run(&mut w, 3000.0);
        let normal = (q_per_m * manning / slope.sqrt()).powf(0.6);
        let got = w.h[(n / 2) * n + n / 2];
        assert!((got - normal).abs() / normal < 0.05, "depth {got:.4} m, Manning normal depth {normal:.4} m");
    }

    #[test]
    fn a_closed_pond_rises_by_exactly_its_inflow() {
        let (mut w, _) = tank(30, |x, y| 0.05 * ((x as f32 - 15.0).abs() + (y as f32 - 15.0).abs()), 0.04, false);
        let q = 0.01f32;
        w.sources.push(Source { x: 15, y: 15, rate: q * DAY_S as f32, seasonality: 0.0 });
        run(&mut w, 1000.0);
        let stored = w.total_volume() * (w.cell_m * w.cell_m) as f64;
        assert!((stored - 10.0).abs() < 1e-3, "stored {stored} m3, inflow 10 m3");
    }

    #[test]
    fn the_head_over_a_broad_crested_sill_matches_critical_flow() {
        let (n, q_per_m) = (80usize, 0.05f32);
        let crest = 1.0;
        let (mut w, _) = tank(n, |x, _| if x < 40 { 0.0 } else if x < 50 { crest } else { crest - 0.1 * (x - 49) as f32 }, 0.01, true);
        for y in 0..n {
            w.sources.push(Source { x: 0, y, rate: q_per_m * DAY_S as f32, seasonality: 0.0 });
            w.closed_edge.as_mut_slice()[y * n] = true;
        }
        for x in 0..n {
            w.closed_edge.as_mut_slice()[x] = true;
            w.closed_edge.as_mut_slice()[(n - 1) * n + x] = true;
        }
        run(&mut w, 4000.0);
        let head = w.z[(n / 2) * n + 30] + w.h[(n / 2) * n + 30] - crest;
        let theory = 1.5 * (q_per_m * q_per_m / G).cbrt();
        assert!((head - theory).abs() / theory < 0.15, "head {head:.4} m, critical-flow theory {theory:.4} m");
    }
}
