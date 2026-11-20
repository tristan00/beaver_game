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
mod climate;
mod routed;
pub use climate::{Forcing, Weather};
pub use crate::calendar::Season;
const FACE_DRY: f32 = 1.0e-4;
const EDGE_SLOPE_MIN: f32 = 1.0e-3;
const MOVING: f32 = 0.02;

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
    pub publish_s: f64,
    pub upkeep_s: f64,
    pub substeps: u64,
    pub cell_updates: u64,
    pub physics_s: f64,
}

impl WaterTimings {
    pub fn total_s(&self) -> f64 {
        self.faces_s + self.publish_s + self.upkeep_s
    }
    pub fn report(&self, wall_s: f64) -> String {
        let total = self.total_s().max(1e-12);
        format!(
            "water {:.1} ms ({:.0}% of wall): routing {:.0}%, publish {:.0}%, upkeep {:.0}% | {} steps, {:.0} cells/step, physics x{:.0} real time",
            total * 1e3, 100.0 * total / wall_s.max(1e-12),
            100.0 * self.faces_s / total, 100.0 * self.publish_s / total,
            100.0 * self.upkeep_s / total, self.substeps,
            self.cell_updates as f64 / self.substeps.max(1) as f64, self.physics_s / total,
        )
    }

}

#[derive(Clone, Debug)]
pub struct Water {
    routed: routed::Flow,
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
    books: Budget,
    n: usize,
    h: Vec<f32>,
    z: Vec<f32>,
    bed_version: Option<((u64,u64),(u64,u64))>,
    qx: Vec<f32>,
    qy: Vec<f32>,
    src: Vec<f32>,
    source_key: Option<u64>,
    source_cells: Vec<usize>,
    rows: Vec<Vec<(u32, u32)>>,
    topology_dirty: bool,
    /// Rows stepped at the last publish; cells outside them and the current rows did not change.
    pub_rows: Vec<Vec<(u32, u32)>>,
    full_publish: bool,
    dam_cells: Vec<u32>,
    ice_any: bool,
    since_environment: f64,
}

const FORCING_EVERY_S: f64 = 300.0;

impl Water {
    pub fn new(size: usize) -> Self {
        let n = size;
        Self {
            routed: routed::Flow::default(),
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
            books: Budget::default(),
            n,
            h: vec![0.0; n * n],
            z: vec![0.0; n * n],
            bed_version: None,
            qx: vec![0.0; n * (n + 1)],
            qy: vec![0.0; (n + 1) * n],
            src: vec![0.0; n * n],
            source_key: None,
            source_cells: Vec::new(),
            rows: vec![Vec::new(); n],
            topology_dirty: true,
            pub_rows: vec![Vec::new(); n],
            full_publish: true,
            dam_cells: Vec::new(),
            ice_any: false,
            since_environment: 0.0,
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
        w.run_routed(SPIN_UP_S, false);
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
        self.routed.total_volume().unwrap_or_else(|| self.h.iter().map(|&h| h as f64).sum())
    }

    pub fn active_segments(&self) -> usize {
        self.rows.iter().map(|r| r.len()).sum()
    }

    pub fn active_cells(&self) -> usize {
        self.rows.iter().flatten().map(|&(a, b)| (b - a) as usize).sum()
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
                changed = true;
                self.z[i] = z;
            }
        }
        if changed {
            self.topology_dirty = true;
            self.full_publish = true;
        }
        self.bed_version=Some(version);
    }

    fn refresh_sources(&mut self) {
        let forcing_time = (self.time_days * DAY_S / FORCING_EVERY_S).floor() * FORCING_EVERY_S / DAY_S;
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
        self.source_key = Some(key);
        source_cells.sort_unstable();
        source_cells.dedup();
        if source_cells != self.source_cells {
            self.source_cells = source_cells;
        }
    }

    pub fn step(&mut self, height: &HeightGrid, dt_days: f32) {
        self.step_physics(height, dt_days);
    }

    fn step_physics(&mut self, height: &HeightGrid, dt_days: f32) {
        let t0 = std::time::Instant::now();
        self.load_bed(height);
        self.timings.upkeep_s += t0.elapsed().as_secs_f64();
        if !dt_days.is_finite() || dt_days <= 0.0 {
            self.publish();
            return;
        }
        self.run_routed(dt_days as f64 * DAY_S, true);
        let t0 = std::time::Instant::now();
        self.update_dams(height, dt_days);
        self.update_ice(dt_days);
        self.timings.upkeep_s += t0.elapsed().as_secs_f64();
        self.publish();
    }

    /// Route inflow and apply weather on the same fifteen-minute clock.
    fn run_routed(&mut self, seconds:f64, losses:bool) {
        if self.topology_dirty {
            self.dam_cells = (0..self.n * self.n).filter(|&i| self.dam_height.as_slice()[i] > 0.0).map(|i| i as u32).collect();
            self.topology_dirty = false;
            self.full_publish = true;
        }
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

    /// Remember bare sediment after floodwater recedes.
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

    /// One explicit step of the local-inertial shallow-water equations (Bates et al. 2010):
    /// momentum per face from the water-surface slope with semi-implicit Manning friction, then
    /// mass per cell. Pools take what crosses their rim and hold one level. Returns the step in s.
    fn publish(&mut self) {
        self.revision = self.revision.wrapping_add(1);
        let t0 = std::time::Instant::now();
        let n = self.n;
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
            self.topology_dirty = true;
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

fn edge_bed(z: &[f32], c: usize, inner: usize, dx: f32) -> f32 {
    z[c] - ((z[inner] - z[c]) / dx).max(EDGE_SLOPE_MIN) * dx
}

const SPIN_UP_S: f64 = 9000.0;

#[allow(clippy::too_many_arguments)]
pub fn neighbours4(n: usize, i: usize) -> impl Iterator<Item = usize> {
    let (x, y) = (i % n, i / n);
    [(x > 0).then(|| i - 1), (x + 1 < n).then(|| i + 1), (y > 0).then(|| i - n), (y + 1 < n).then(|| i + n)].into_iter().flatten()
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
