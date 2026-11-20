use crate::SimState;
use crate::grid::{Grid, HeightGrid, WaterGrid};
use crate::worldgen::World;
use std::cmp::Reverse;
use std::collections::BinaryHeap;
use std::fmt;
pub use crate::tuning::water::*;

pub type Cell = (usize, usize);

const DAY_S: f64 = 86_400.0;
/// A game day advances the same water and environmental day on every device.
pub const GAME_HYDRAULIC_DAY_S: f64 = DAY_S;
mod gang;
mod kernels;
mod view;
mod climate;
mod routed;
pub use climate::{Forcing, Weather};
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
    routed: routed::Flow,
    detailed: bool,
    routed_needs_sync: bool,
    pub depth: WaterGrid,
    pub ice: Grid<f32>,
    pub dam_height: Grid<f32>,
    pub dam_integrity: Grid<f32>,
    pub dam_durability: Grid<f32>,
    pub canal: Grid<bool>,
    pub fed: Grid<bool>,
    pub structures: Grid<bool>,
    pub closed_edge: Grid<bool>,
    pub width: Grid<f32>,
    pub channel: Grid<bool>,
    /// Exposed mineral bed, retained after water recedes (0 = turf, 1 = silt/gravel).
    pub riverbed: Grid<f32>,
    pub discharge: Grid<f32>,
    pub velocity: Grid<(f32, f32)>,
    pub lake_level: Grid<f32>,
    pub sources: Vec<Source>,
    pub config: WaterConfig,
    pub weather: Weather,
    pub time_days: f64,
    pub added_volume: f64,
    pub lost_volume: f64,
    pub cell_m: f32,
    pub physics_seconds: f64,
    /// Changes when published water or navigable waterworks change.
    pub revision: u64,
    pub(crate) nav_revision: u64,
    pub(crate) topology_revision: u64,
    pub(crate) nav_cells: Vec<usize>,
    nav_marked: Vec<bool>,
    pub steps_taken: u64,
    pub timings: WaterTimings,
    pub pools_enabled: bool,
    books: Budget,
    n: usize,
    h: Vec<f32>,
    z: Vec<f32>,
    bed_version: Option<((u64,u64),(u64,u64))>,
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
    activity_threshold: Vec<f32>,
    /// Scratch for new depths (the mass kernel may compute a lane twice).
    h_new: Vec<f32>,
    rim_mask: Vec<bool>,
    pools: Vec<Pool>,
    source_key: Option<u64>,
    source_total: f64,
    source_cells: Vec<usize>,
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
    since_environment: f64,
    work_dirty: bool,
    scoped_growth: bool,
    activity_scratch: Vec<(u32,u32)>,
    work_cells: u64,
    bands: Vec<usize>,
    band_out: Vec<BandOut>,
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
const POOL_MIN_DEPTH: f32 = 0.005;
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
            routed: routed::Flow::default(),
            detailed: std::env::var_os("BC_WATER_DETAILED").is_some(),
            routed_needs_sync: true,
            depth: Grid::new(size, size, 0.0),
            ice: Grid::new(size, size, 0.0),
            dam_height: Grid::new(size, size, 0.0),
            dam_integrity: Grid::new(size, size, 0.0),
            dam_durability: Grid::new(size, size, 1.0),
            canal: Grid::new(size, size, false),
            fed: Grid::new(size, size, false),
            structures: Grid::new(size, size, false),
            closed_edge: Grid::new(size, size, false),
            width: Grid::new(size, size, 0.0),
            channel: Grid::new(size, size, false),
            riverbed: Grid::new(size, size, 0.0),
            discharge: Grid::new(size, size, 0.0),
            velocity: Grid::new(size, size, (0.0, 0.0)),
            lake_level: Grid::new(size, size, f32::NAN),
            sources: Vec::new(),
            config: WaterConfig::default(),
            weather: Weather::default(),
            time_days: 0.0,
            added_volume: 0.0,
            lost_volume: 0.0,
            cell_m: 1.0,
            physics_seconds: 0.0,
            revision: 0,
            nav_revision: 0,
            topology_revision: 0,
            nav_cells: Vec::new(),
            nav_marked: vec![false;n*n],
            steps_taken: 0,
            timings: WaterTimings::default(),
            pools_enabled: std::env::var_os("WATER_NOPOOLS").is_none(),
            books: Budget::default(),
            n,
            h: vec![0.0; n * n],
            z: vec![0.0; n * n],
            bed_version: None,
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
            activity_threshold: vec![WET_SEED;n*n],
            h_new: vec![0.0; n * n],
            rim_mask: vec![false; n * n],
            pools: Vec::new(),
            source_key: None,
            source_total: 0.0,
            source_cells: Vec::new(),
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
            since_environment: 0.0,
            work_dirty: true,
            scoped_growth: std::env::var_os("WATER_FULL_GROWTH_SCAN").is_none(),
            activity_scratch: Vec::new(),
            work_cells: 0,
            bands: Vec::new(),
            band_out: Vec::new(),
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
        // Only the part of an external lake crossing the map boundary is a fixed
        // reservoir. Interior lake cells store finite water and respond to dams,
        // drainage canals, rain and drought just like other ponds.
        for y in 1..n - 1 {
            for x in 1..n - 1 {
                w.lake_level.as_mut_slice()[y * n + x] = f32::NAN;
            }
        }
        let (raw, surface) = (world.height.as_slice(), world.surface.as_slice());
        for i in 0..n * n {
            let lvl = world.lake_level.as_slice()[i];
            w.h[i] = if lvl.is_finite() { (lvl - raw[i]).max(0.0) } else if world.river.as_slice()[i] || world.lake.as_slice()[i] { (surface[i] - raw[i]).max(0.0) } else { 0.0 };
        }
        w.fill_fed_hollows(world);
        w.load_bed(&world.height);
        w.run_for(SPIN_UP_S, false);
        for i in 0..n * n {
            if w.h[i] > 0.005 || world.river.as_slice()[i] || world.lake.as_slice()[i] {
                w.riverbed.as_mut_slice()[i] = 1.0;
            }
        }
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

    pub fn forcing(&self) -> Forcing { climate::forcing(self.time_days, self.weather, self.config.rain) }

    /// Stored water in metres of depth summed over cells.
    pub fn total_volume(&self) -> f64 {
        if !self.detailed {
            if let Some(volume)=self.routed.total_volume() {return volume;}
        }
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

    /// All surviving cells of the connected dam, including diagonal sections.
    pub fn dam_line_at(&self, seed: Cell) -> Vec<Cell> {
        if !self.has_dam(seed.0, seed.1) {
            return Vec::new();
        }
        let mut out = vec![seed];
        let mut seen = std::collections::HashSet::from([seed]);
        let mut i = 0;
        while i < out.len() {
            let (x, y) = out[i];
            for (dx, dy) in [(-1i64, -1i64), (0, -1), (1, -1), (-1, 0), (1, 0), (-1, 1), (0, 1), (1, 1)] {
                let (nx, ny) = (x as i64 + dx, y as i64 + dy);
                if nx < 0 || ny < 0 {
                    continue;
                }
                let c = (nx as usize, ny as usize);
                if self.has_dam(c.0, c.1) && seen.insert(c) {
                    out.push(c);
                }
            }
            i += 1;
        }
        out.sort();
        out
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
        let mut changed = false;
        let mut repaired = false;
        for (i, &(_, h)) in idx.into_iter().zip(cells) {
            if h > self.dam_height.as_slice()[i] {
                self.dam_height.as_mut_slice()[i] = h;
                changed = true;
            }
            if self.dam_integrity.as_slice()[i] != 1.0 {
                self.dam_integrity.as_mut_slice()[i] = 1.0;
                repaired = true;
            }
        }
        if changed || repaired {
            self.revision = self.revision.wrapping_add(1);
        }
        if changed {
            self.nav_revision = self.nav_revision.wrapping_add(1);
            self.topology_revision = self.topology_revision.wrapping_add(1);
        }
        Ok(())
    }

    pub fn build_dam(&mut self, cells: &[Cell], height: f32) -> Result<(), WaterError> {
        let idx = self.check_cells(cells)?;
        Self::check_positive("dam height", height)?;
        self.revision = self.revision.wrapping_add(1);
        self.nav_revision = self.nav_revision.wrapping_add(1);
        self.topology_revision = self.topology_revision.wrapping_add(1);
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
        self.revision = self.revision.wrapping_add(1);
        self.nav_revision = self.nav_revision.wrapping_add(1);
        self.topology_revision = self.topology_revision.wrapping_add(1);
        Ok(())
    }

    pub fn dig_canal(&mut self, height: &mut HeightGrid, cells: &[Cell], depth: f32) -> Result<(), WaterError> {
        assert_eq!(height.width(), self.depth.width(), "terrain size must match water size");
        let idx = self.check_cells(cells)?;
        Self::check_positive("canal depth", depth)?;
        self.revision = self.revision.wrapping_add(1);
        self.nav_revision = self.nav_revision.wrapping_add(1);
        self.topology_revision = self.topology_revision.wrapping_add(1);
        for i in idx {
            height.as_mut_slice()[i] -= depth;
            self.canal.as_mut_slice()[i] = true;
            self.riverbed.as_mut_slice()[i] = 1.0;
        }
        Ok(())
    }

    pub(crate) fn clear_navigation_changes(&mut self) {
        for &i in &self.nav_cells {self.nav_marked[i]=false;}
        self.nav_cells.clear();
    }

    pub fn refresh(&mut self, height: &HeightGrid) {
        self.load_bed(height);
        self.publish();
    }

    /// Bed = terrain + dams.
    fn load_bed(&mut self, height: &HeightGrid) {
        assert_eq!((height.width(), height.height()), (self.n, self.n), "terrain size must match water size");
        let version=(height.version(),self.dam_height.version());
        if self.bed_version==Some(version) {return;}
        let mut changed = false;
        for (i, t) in height.as_slice().iter().enumerate() {
            let z = t + self.dam_height.as_slice()[i];
            if z != self.z[i] {
                if !changed {
                    // Materialize stored pool volumes against the OLD bed before any
                    // excavation or dam changes its geometry.
                    self.dissolve_pools();
                }
                changed = true;
                self.z[i] = z;
            }
        }
        if changed {
            self.masks_dirty = true;
            self.full_publish = true;
        }
        self.bed_version=Some(version);
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
        let forcing_time = (self.time_days * DAY_S / POOL_EVERY_S).floor() * POOL_EVERY_S / DAY_S;
        let inflow = climate::forcing(forcing_time, self.weather, self.config.rain).inflow;
        let mut key = inflow.to_bits() as u64;
        for s in &self.sources {
            for v in [s.x as u32, s.y as u32, s.rate.to_bits(), s.seasonality.to_bits()] {
                key = (key ^ v as u64).wrapping_mul(0x100_0000_01b3);
            }
        }
        if self.source_key == Some(key) {
            return;
        }
        for &i in &self.source_cells { self.src[i] = 0.0; }
        let mut source_cells = Vec::with_capacity(self.sources.len());
        let n = self.n;
        for s in &self.sources {
            let rate = s.rate * (1.0 + s.seasonality * (inflow - 1.0)).max(0.0);
            // Source rates are metres of depth per day on their cell; src is metres per second.
            self.src[s.y * n + s.x] += rate / DAY_S as f32;
            if rate > 0.0 { source_cells.push(s.y * n + s.x); }
        }
        for p in &mut self.pools {
            p.source = p.members.iter().map(|&i| self.src[i as usize]).sum();
        }
        self.source_key = Some(key);
        source_cells.sort_unstable();
        source_cells.dedup();
        self.source_total = source_cells.iter().map(|&i| self.src[i] as f64).sum();
        if source_cells != self.source_cells {
            self.source_cells = source_cells;
            self.rebuild_activity_full();
        }
    }

    fn refresh_wet_row(&mut self,y:usize,active_only:bool) {
        let mut runs=std::mem::take(&mut self.wet_rows[y]);runs.clear();
        let r=y*self.n;
        if active_only {
            for &(a,b) in &self.rows[y] {kernels::seed_runs(&mut runs,&self.h[r+a as usize..r+b as usize],&self.activity_threshold[r+a as usize..r+b as usize],a);}
        } else {kernels::seed_runs(&mut runs,&self.h[r..r+self.n],&self.activity_threshold[r..r+self.n],0);}
        self.wet_rows[y]=runs;
    }

    /// Rows of cells to compute: every wet cell outside a pool, every source, every pool's rim,
    /// and their 4-neighbours.
    fn rebuild_activity_full(&mut self) {
        for i in 0..self.h.len() {
            self.activity_threshold[i]=if self.src[i]>0.0 || self.rim_mask[i] {f32::NEG_INFINITY}
                else if self.member[i]==0.0 && !self.lake_inner[i] {WET_SEED} else {f32::INFINITY};
        }
        for y in 0..self.n {
            self.refresh_wet_row(y,false);
        }
        self.since_shrink = 0;
        self.rebuild_rows(0, self.n);
    }

    /// Recompute active rows y0..y1 (and the face rows around them) from the wet runs.
    fn rebuild_rows(&mut self, y0: usize, y1: usize) {
        let n=self.n as u32;
        let mut runs=std::mem::take(&mut self.activity_scratch);
        for y in y0..y1 {
            runs.clear();
            for yy in y.saturating_sub(1)..(y+2).min(self.n) {
                for &(a,b) in &self.wet_rows[yy] {runs.push((a.saturating_sub(1),(b+1).min(n)));}
            }
            runs.sort_unstable();merge_runs_in_place(&mut runs,ACTIVE_GAP);
            if runs!=self.rows[y] {
                self.work_dirty=true;
                let w=self.n;
                for &(a,b) in &self.rows[y] {self.act[y*w+a as usize..y*w+b as usize].fill(0.0);}
                for &(a,b) in &runs {self.act[y*w+a as usize..y*w+b as usize].fill(1.0);}
                std::mem::swap(&mut self.rows[y],&mut runs);
            }
        }
        for y in y0..=y1.min(self.n) {
            runs.clear();
            if y>0 {runs.extend_from_slice(&self.rows[y-1]);}
            if y<self.n {runs.extend_from_slice(&self.rows[y]);}
            runs.sort_unstable();merge_runs_in_place(&mut runs,0);
            if runs!=self.yrows[y] {std::mem::swap(&mut self.yrows[y],&mut runs);}
        }
        self.activity_scratch=runs;
    }

    pub fn step(&mut self, height: &HeightGrid, dt_days: f32) {
        self.step_physics(height, dt_days);
    }

    pub fn step_game(&mut self, height: &HeightGrid, dt_days: f32) {
        self.step(height, dt_days);
    }

    pub fn hydraulic_seconds_per_day(&self) -> f64 { DAY_S }

    fn step_physics(&mut self, height: &HeightGrid, dt_days: f32) {
        let t0 = std::time::Instant::now();
        self.load_bed(height);
        self.timings.upkeep_s += t0.elapsed().as_secs_f64();
        if !dt_days.is_finite() || dt_days <= 0.0 {
            self.publish();
            return;
        }
        self.run_for(dt_days as f64 * DAY_S, true);
        let t0 = std::time::Instant::now();
        self.update_dams(height, dt_days);
        self.update_ice(dt_days);
        self.timings.upkeep_s += t0.elapsed().as_secs_f64();
        self.publish();
    }

    /// Runs the physics for `seconds`, finding pools again (and applying rain and losses) every
    /// `POOL_EVERY_S`.
    fn run_for(&mut self, seconds: f64, losses: bool) {
        if !self.detailed {
            self.run_routed(seconds, losses);
            return;
        }
        let mut t = 0.0;
        while t < seconds - 1e-9 {
            let environment_limit = if losses { POOL_EVERY_S - self.since_environment } else { f64::INFINITY };
            let chunk = (POOL_EVERY_S - self.since_pools).min(environment_limit).min(seconds - t).max(1e-9);
            let upkeep = std::time::Instant::now();
            self.refresh_sources();
            self.timings.upkeep_s += upkeep.elapsed().as_secs_f64();
            let mut done = 0.0;
            while done < chunk - 1e-6 {
                done += self.substep((chunk - done) as f32) as f64;
            }
            // Substeps use f32. Do not dispatch the whole solver for a residual
            // below a microsecond, but keep the calendar on its exact boundary.
            // Hydraulic counters and water books retain the actual stepped time.
            t += chunk;
            self.since_pools += chunk;
            if losses {
                self.since_environment += chunk;
                self.time_days += chunk / DAY_S;
            }
            if losses && self.since_environment >= POOL_EVERY_S - 1e-3 {
                let t0 = std::time::Instant::now();
                let grew = self.apply_rain_and_losses((self.since_environment / DAY_S) as f32);
                self.since_environment = 0.0;
                if grew {
                    self.rebuild_activity_full();
                    self.full_publish = true;
                }
                self.timings.upkeep_s += t0.elapsed().as_secs_f64();
            }
            // Public durations are f32 days; summing ten-second frame slices can
            // land a few microseconds below a five-minute boundary.
            if self.since_pools >= POOL_EVERY_S - 1e-3 {
                let t0 = std::time::Instant::now();
                let old_members: Vec<u32> = self.pools.iter().flat_map(|p| p.members.iter().copied()).collect();
                self.dissolve_pools();
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

    fn run_routed(&mut self, seconds:f64, losses:bool) {
        self.dissolve_pools();
        if self.masks_dirty {self.rebuild_masks();}
        let mut flow=std::mem::take(&mut self.routed);
        let mut left=seconds;
        while left>1.0e-6 {
            let limit=if losses {900.0-self.since_environment} else {seconds};
            // f32 calendar steps arrive a few microseconds over 900 seconds.
            // Do not run the entire graph again for that rounding remainder.
            let dt=if left<=limit+1.0e-3 {left} else {limit};
            self.refresh_sources();
            let started=std::time::Instant::now();
            let sync=std::mem::take(&mut self.routed_needs_sync);
            flow.prepare(self,sync);
            flow.advance(self,dt);
            if losses {
                self.time_days+=dt/DAY_S;
                self.since_environment+=dt;
                if self.since_environment>=900.0-1.0e-3 {
                    flow.environment(self,self.since_environment/DAY_S);
                    self.since_environment=0.0;
                }
            }
            flow.publish(self);
            self.timings.faces_s+=started.elapsed().as_secs_f64();
            left-=dt;
        }
        self.routed=flow;
    }

    /// Rain on open water, evaporation and seepage from wet ground (millimetres a day).
    fn apply_rain_and_losses(&mut self, dt_days: f32) -> bool {
        // Environment updates need current depths, including pool interiors that
        // the hydraulic solver normally only publishes to their boundary cells.
        for p in &self.pools {
            for &i in &p.members { self.h[i as usize] = (p.level - self.z[i as usize]).max(0.0); }
        }
        self.update_riverbed(dt_days);
        let forcing = self.forcing();
        let cfg = &self.config;
        let (r, e, s) = (forcing.rain * dt_days, (cfg.evaporation * forcing.evaporation) * dt_days, cfg.seepage * dt_days);
        if (r == 0.0 && e == 0.0 && s == 0.0) || dt_days <= 0.0 {
            return false;
        }
        let lake = self.lake_level.as_slice();
        let (mut rain, mut evap, mut seep) = (0.0f64, 0.0f64, 0.0f64);
        let mut grew = false;
        for i in 0..self.h.len() {
            let hh = self.h[i];
            if lake[i].is_finite() || (hh == 0.0 && (r == 0.0 || !self.detailed)) {
                continue;
            }
            // Infiltration absorbs light rain on dry ground. Sustained precipitation
            // accumulates and starts overland flow through the same solver.
            let rr = r;
            let ev = e.min(hh + rr);
            let infiltration = if self.riverbed.as_slice()[i] < 0.5 && !self.canal.as_slice()[i] { s * 4.0 } else { s };
            let sp = infiltration.min(hh + rr - ev);
            self.h[i] = hh + rr - ev - sp;
            grew |= hh <= WET_SEED && self.h[i] > WET_SEED;
            if self.member[i] > 0.0 {
                self.pools[self.member[i] as usize - 1].volume += rr as f64 - ev as f64 - sp as f64;
            }
            rain += rr as f64;
            evap += ev as f64;
            seep += sp as f64;
        }
        for p in &mut self.pools {
            p.volume = p.volume.max(0.0);
            p.level = p.level_for(p.volume);
            for &i in &p.members { self.h[i as usize] = (p.level - self.z[i as usize]).max(0.0); }
        }
        self.books.rain += rain;
        self.books.evaporation += evap;
        self.books.seepage += seep;
        self.added_volume += rain;
        self.lost_volume += evap + seep;
        grew
    }

    fn update_riverbed(&mut self, dt: f32) {
        for (i, bed) in self.riverbed.as_mut_slice().iter_mut().enumerate() {
            if self.canal.as_slice()[i] {
                *bed = 1.0;
            } else if self.h[i] > 0.005 {
                // Newly flooded turf becomes bare soil over half a day. Established beds
                // remain exposed through a dry season; regrowth is deliberately slow.
                *bed = (*bed + dt * 2.0).min(1.0);
            } else {
                *bed = (*bed - dt / 180.0).max(0.0);
            }
        }
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
        let n = self.n;
        let dx = self.cell_m;
        let c = (G * self.h_max.max(0.01)).sqrt();
        debug_assert!(limit.is_finite() && limit > 0.0);
        let dt = (CFL * dx / c).min(DT_MAX_S).min(limit);
        let p = kernels::Params { gdt: G * dt, fric: G * dt * self.config.manning * self.config.manning, inv_dx: 1.0 / dx };
        let k = dt / dx;

        // The active footprint changes far less often than the CFL substep.
        // Reuse the partition and each worker's output allocation until it does.
        let gang = gang::gang();
        if self.work_dirty {
            self.work_cells = self.rows.iter().flatten().map(|&(a,b)|(b-a) as u64).sum();
            let tasks = if self.work_cells >= PARALLEL_CELLS {gang.width().min(n)} else {1};
            self.bands.clear();self.bands.push(0);
            let mut acc=0u64;
            for y in 0..n {
                acc+=self.rows[y].iter().map(|&(a,b)|(b-a) as u64).sum::<u64>()+1;
                let t=self.bands.len();
                if t<tasks && acc*tasks as u64 >= (self.work_cells+n as u64)*t as u64 {self.bands.push(y+1);}
            }
            while self.bands.len()<=tasks {self.bands.push(n);}
            *self.bands.last_mut().unwrap()=n;
            for t in 1..tasks {self.bands[t]=self.bands[t].max(self.bands[t-1]+1).min(n-(tasks-t));}
            self.band_out.resize_with(tasks,BandOut::default);
            self.work_dirty=false;
        }
        for out in &mut self.band_out {out.hmax=0;out.lake_change=0.;out.edge=0.;out.grew.clear();}
        let cells=self.work_cells;
        let tasks=self.band_out.len();
        let view = View::of(self);
        let bounds = &self.bands;
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
        let outs_ptr = SendPtr(self.band_out.as_mut_ptr());
        let cont = |t: usize, y0: usize, y1: usize, v: &View| unsafe {
            v.continuity(y0, y1, k, dt, &mut *outs_ptr.get().add(t));
        };
        run(&cont);
        // Pool rims can lie in any row band. Updating them inside a worker's
        // continuity task races another band's reads/writes of the same depth.
        // All cell balances must finish before any pool publishes its new level.
        unsafe { view.step_pools(0, 1, k, dt); }
        let t3 = std::time::Instant::now();
        self.timings.continuity_s += (t3 - t2).as_secs_f64();
        let (mut hmax, mut lake_change, mut edge) = (0u32, 0.0f64, 0.0f64);
        let mut grew: Vec<usize> = Vec::new();
        for o in &self.band_out {
            hmax = hmax.max(o.hmax);
            lake_change += o.lake_change;
            edge += o.edge;
            grew.extend_from_slice(&o.grew);
        }

        // Pools are published only after every continuity band has finished.
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
                self.refresh_wet_row(y,self.scoped_growth);
            }
            let mut start=grew[0].saturating_sub(1);
            let mut end=(grew[0]+2).min(n);
            for &y in &grew[1..] {
                let a=y.saturating_sub(1);let b=(y+2).min(n);
                if a<=end {end=end.max(b);} else {self.rebuild_rows(start,end);start=a;end=b;}
            }
            self.rebuild_rows(start,end);
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
        self.source_total
    }

    fn publish(&mut self) {
        self.revision = self.revision.wrapping_add(1);
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
                let old = if self.rows[y] == self.pub_rows[y] { 0 } else { self.pub_rows[y].len() };
                for k in 0..self.rows[y].len() + old {
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
            if (self.depth.as_slice()[i] > crate::buildings::WET_DEPTH) != (h > crate::buildings::WET_DEPTH) {
                self.nav_revision = self.nav_revision.wrapping_add(1);
                if !self.nav_marked[i] {self.nav_marked[i]=true;self.nav_cells.push(i);}
            }
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
            *integ -= (self.config.dam_decay_per_day + self.config.overtop_erosion * over) * dt / self.dam_durability.as_slice()[i].max(1.0);
            if *integ <= 0.0 {
                broken.push(i);
            }
        }
        if !broken.is_empty() {
            self.nav_revision = self.nav_revision.wrapping_add(1);
            self.topology_revision = self.topology_revision.wrapping_add(1);
            self.dissolve_pools();
            self.masks_dirty = true;
            self.full_publish = true;
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

fn merge_runs_in_place(runs:&mut Vec<(u32,u32)>,gap:u32) {
    let mut count=0;
    for read in 0..runs.len() {
        let (a,b)=runs[read];
        if count>0 && a<=runs[count-1].1+gap {runs[count-1].1=runs[count-1].1.max(b);}
        else {runs[count]=(a,b);count+=1;}
    }
    runs.truncate(count);
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
