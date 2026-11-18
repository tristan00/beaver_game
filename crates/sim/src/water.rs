//! Water simulation: a cell-based virtual-pipe model over the heightmap.
//!
//! Each cell holds a water `depth` above its *bed* (terrain + dam crest height). Every substep,
//! four outflow fluxes per cell (with momentum) move water toward lower neighbouring surfaces;
//! outflow is limited to the cell's contents, so the scheme is stable for any dt and conserves
//! mass (up to float rounding) on a closed map. Sources (river inflow), rain, evaporation,
//! seepage and open-edge outflow are the only mass exchanges and are accounted in
//! `added_volume` / `lost_volume`.
//!
//! Open edges: where the land keeps falling outward (a river running off the map) it is taken to go
//! on beyond the map with the slope it has at the edge, and water leaves through the same pipes as
//! any other flow. An edge the land rises from is a wall: water never leaves the world upstream.
//!
//! Dams are cell barriers (height + integrity). Water overtopping a dam erodes it and integrity
//! also decays with time; at zero integrity the dam is removed and the pond drains as a flood wave.
//! Canals lower terrain. Seasons modulate inflow/rain/evaporation; winter grows surface ice.
//! The first day of simulated time is the start of spring.
use crate::SimState;
use crate::grid::{Grid, HeightGrid, WaterGrid};
use crate::worldgen::{InputKind, World};
use std::fmt;
pub use crate::tuning::water::*;

/// Grid coordinate `(x, y)`.
pub type Cell = (usize, usize);


/// Flow substeps per simulated day. The number of substeps a step runs scales with its length, so
/// how far water travels in a day does not depend on the step size.
const FLOW_STEPS_PER_DAY: f32 = 1000.0;
/// Upper bound on substeps in one step (a guard against absurd steps, far above any real use).
const MAX_SUBSTEPS: usize = 4000;
const FRICTION: f32 = 0.9;
const PUSH: f32 = 0.2;
/// Max fraction of the surface difference one pipe moves (keeps neighbours from overshooting).
const PIPE_CAP: f32 = 0.24;
/// Water depth over a cell face at which it passes water freely. Shallower water conducts in
/// proportion to its depth (flow needs depth: a film creeps, a river runs, a deep canal pours), so
/// a river stands as deep as the flow it carries needs.
const FULL_FLOW_DEPTH: f32 = 2.35;
const WALL: f32 = 1.0e9;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Season { Spring, Summer, Autumn, Winter }

impl Season {
    /// Season at `time_days` (0 = start of spring).
    pub fn at(time_days: f64) -> Season {
        match ((time_days / SEASON_DAYS).floor() as i64).rem_euclid(4) {
            0 => Season::Spring,
            1 => Season::Summer,
            2 => Season::Autumn,
            _ => Season::Winter,
        }
    }
    /// River inflow multiplier: spring melt floods, summer drought.
    pub fn inflow_mult(self) -> f32 {
        INFLOW_MULT[self as usize]
    }
    pub fn rain_mult(self) -> f32 {
        RAIN_MULT[self as usize]
    }
    pub fn evap_mult(self) -> f32 {
        EVAP_MULT[self as usize]
    }
}

/// Tunable rates (depth units per day per cell unless noted).
#[derive(Clone, Debug, PartialEq)]
pub struct WaterConfig {
    pub rain: f32,
    pub evaporation: f32,
    pub seepage: f32,
    /// Rain per day that the ground soaks up instead of adding water.
    pub soil_absorb: f32,
    /// Integrity a dam loses per day from age alone.
    pub dam_decay_per_day: f32,
    /// Integrity lost per day per unit of water depth above `overtop_depth` on the crest.
    pub overtop_erosion: f32,
    pub overtop_depth: f32,
    pub ice_growth_per_day: f32,
    pub ice_melt_per_day: f32,
    /// Water at the map edge drains off-map (so floods and rivers leave the world).
    pub open_edges: bool,
}

impl Default for WaterConfig {
    fn default() -> Self {
        Self {
            rain: RAIN,
            evaporation: EVAPORATION,
            seepage: SEEPAGE,
            soil_absorb: SOIL_ABSORB,
            dam_decay_per_day: DAM_DECAY_PER_DAY,
            overtop_erosion: OVERTOP_EROSION,
            overtop_depth: OVERTOP_DEPTH,
            ice_growth_per_day: ICE_GROWTH_PER_DAY,
            ice_melt_per_day: ICE_MELT_PER_DAY,
            open_edges: true,
        }
    }
}

/// Inflow point; `rate` is depth added per day at base, scaled by the season: `1 + seasonality *
/// (Season::inflow_mult - 1)`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Source {
    pub x: usize,
    pub y: usize,
    pub rate: f32,
    /// How much of the seasonal inflow swing applies: 1 = snowmelt-fed river (full swing),
    /// 0 = steady groundwater spring.
    pub seasonality: f32,
}

/// Cumulative water volumes by where they entered and left the map (since `Water::reset_books`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Budget {
    /// Brought in by sources: rivers from beyond the map, springs and baseflow.
    pub sources: f64,
    /// Rain the ground did not soak up.
    pub rain: f64,
    /// Drained off the map at its border.
    pub edges: f64,
    pub evaporation: f64,
    pub seepage: f64,
}

impl Budget {
    pub fn added(&self) -> f64 { self.sources + self.rain }
    pub fn lost(&self) -> f64 { self.edges + self.evaporation + self.seepage }
}

/// The sources a world's water inputs give (`worldgen::water_inputs`): `WATER_PER_CATCHMENT_CELL`
/// per catchment cell per day, rivers from beyond the map following the seasons by
/// `INFLOW_SEASONALITY`, springs and baseflow by `SPRING_SEASONALITY`.
pub fn sources_for(world: &World) -> Vec<Source> {
    world
        .water_inputs
        .iter()
        .map(|inp| Source {
            x: inp.cell.0,
            y: inp.cell.1,
            rate: WATER_PER_CATCHMENT_CELL * inp.catchment as f32,
            seasonality: if inp.kind == InputKind::Inflow { INFLOW_SEASONALITY } else { SPRING_SEASONALITY },
        })
        .collect()
}

/// Invalid water operation; the state is left unchanged.
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

/// Result of `preview_dam`: the steady pond a dam would raise. Nothing is mutated.
#[derive(Clone, Debug, PartialEq)]
pub struct DamPreview {
    /// Water surface level the pond fills to (the lowest crest of the new dam cells).
    pub level: f32,
    /// Number of flooded cells.
    pub pond_area: usize,
    /// Water volume the full pond holds.
    pub volume: f32,
    pub flooded_cells: Vec<Cell>,
    pub flooded_trees: usize,
    pub flooded_buildings: usize,
    /// True if the pond would reach the map edge (the dam does not close the valley).
    pub reaches_map_edge: bool,
    /// Lowest crest of the new dam cells: the most the dam itself could hold back.
    pub crest: f32,
    /// Where the water would get round the dam before it reaches the crest: the lowest saddle on
    /// any way from the pond to the river below the dam. `level` is then this saddle's height,
    /// and the dam must be extended over it to hold more. `None` when the dam closes the valley.
    pub spill_at: Option<Cell>,
}

/// Water state. `step` needs the terrain: see `Water::step`.
#[derive(Clone, Debug)]
pub struct Water {
    /// Water depth above bed (0 = dry).
    pub depth: WaterGrid,
    /// Ice thickness on the surface, 0..=`MAX_ICE`.
    pub ice: Grid<f32>,
    /// Dam crest height above terrain (0 = no dam).
    pub dam_height: Grid<f32>,
    /// Dam integrity 0..=1.
    pub dam_integrity: Grid<f32>,
    /// Cells dug as canals.
    pub canal: Grid<bool>,
    /// Cells whose level the water table maintains (river channels, lakes, ponds): rain, evaporation
    /// and seepage cancel there, so only flow and dams change them.
    pub fed: Grid<bool>,
    /// Cells occupied by buildings (set by the buildings system; used by `preview_dam`).
    pub structures: Grid<bool>,
    pub sources: Vec<Source>,
    pub config: WaterConfig,
    pub time_days: f64,
    /// Cumulative volume added by sources and rain / removed by evaporation, seepage, edges.
    pub added_volume: f64,
    pub lost_volume: f64,
    /// Sources, rain, evaporation and seepage of those volumes (see `Water::budget`).
    books: Budget,
    dams: Vec<usize>,
    hs: Vec<f32>,
    /// Bed (terrain + dam) per padded cell, for the water depth over each face.
    bs: Vec<f32>,
    /// Padded indices of the cells flow is computed for this step (wet cells and their neighbours),
    /// with a mask; cells that got wet in the last substep (scratch).
    active: Vec<(u32, u32)>,
    /// The previous step's active set (scratch, reused).
    spare: Vec<(u32, u32)>,
    in_active: Vec<bool>,
    woke: Vec<usize>,
    fl: Vec<f32>,
    fr: Vec<f32>,
    fu: Vec<f32>,
    fd: Vec<f32>,
}

impl Water {
    /// Water for a generated world: every water input of the world (`sources_for`) feeds the
    /// network, lakes and ponds start brim-full and the rivers are settled into the flow the
    /// inputs sustain. Deterministic; the world is not modified.
    pub fn from_world(world: &World) -> Self {
        let mut w = Self::new(world.size);
        for ((d, &h), &s) in w.depth.as_mut_slice().iter_mut().zip(world.height.as_slice()).zip(world.surface.as_slice()) {
            *d = s - h; // zero except lake beds and river channels
        }
        for (d, &river) in w.depth.as_mut_slice().iter_mut().zip(world.river.as_slice()) {
            if river { *d *= RIVER_FILL; }
        }
        for ((f, &h), &sf) in w.fed.as_mut_slice().iter_mut().zip(world.height.as_slice()).zip(world.surface.as_slice()) {
            *f = h < sf;
        }
        w.sources = sources_for(world);
        // Let the springs' flow settle into the channels, then start the clock and the books afresh.
        for _ in 0..(SETTLE_DAYS / SETTLE_DT).round() as usize { w.step(&world.height, SETTLE_DT); }
        w.time_days = 0.0;
        w.reset_books();
        w
    }

    /// Starts the volume books afresh (`added_volume`, `lost_volume`, `budget`).
    pub fn reset_books(&mut self) {
        (self.added_volume, self.lost_volume) = (0.0, 0.0);
        self.books = Budget::default();
    }

    /// Volumes in and out since the books were last reset, by where they entered and left. What
    /// drained off the map edge is the loss that is not evaporation or seepage.
    pub fn budget(&self) -> Budget {
        let edges = (self.lost_volume - self.books.evaporation - self.books.seepage).max(0.0);
        Budget { edges, ..self.books }
    }
    /// Empty `size x size` water state. Panics if `size` is zero.
    pub fn new(size: usize) -> Self {
        let pad = (size + 2) * (size + 2);
        Self {
            depth: Grid::new(size, size, 0.0),
            ice: Grid::new(size, size, 0.0),
            dam_height: Grid::new(size, size, 0.0),
            dam_integrity: Grid::new(size, size, 0.0),
            canal: Grid::new(size, size, false),
            fed: Grid::new(size, size, false),
            structures: Grid::new(size, size, false),
            sources: Vec::new(),
            config: WaterConfig::default(),
            time_days: 0.0,
            added_volume: 0.0,
            lost_volume: 0.0,
            books: Budget::default(),
            dams: Vec::new(),
            hs: vec![WALL; pad],
            bs: vec![WALL; pad],
            active: Vec::new(),
            spare: Vec::new(),
            in_active: vec![false; pad],
            woke: Vec::new(),
            fl: vec![0.0; pad],
            fr: vec![0.0; pad],
            fu: vec![0.0; pad],
            fd: vec![0.0; pad],
        }
    }

    pub fn season(&self) -> Season { Season::at(self.time_days) }

    /// Total water volume on the map.
    pub fn total_volume(&self) -> f64 { self.depth.as_slice().iter().map(|&d| d as f64).sum() }

    /// Net water movement through a cell as (east, south), in depth units (over one cell) per day,
    /// from the last substep's pipe fluxes: what leaves it eastward minus westward, southward minus
    /// northward, averaged with what enters. Zero outside the map or where the water is still.
    pub fn flow_at(&self, x: usize, y: usize) -> (f32, f32) {
        if !self.depth.in_bounds(x, y) {
            return (0.0, 0.0);
        }
        let p = (y + 1) * (self.depth.width() + 2) + x + 1;
        let s = self.depth.width() + 2;
        let ex = 0.5 * ((self.fr[p] - self.fl[p]) + (self.fr[p - 1] - self.fl[p + 1]));
        let ey = 0.5 * ((self.fd[p] - self.fu[p]) + (self.fd[p - s] - self.fu[p + s]));
        (ex * FLOW_STEPS_PER_DAY, ey * FLOW_STEPS_PER_DAY)
    }

    pub fn has_dam(&self, x: usize, y: usize) -> bool {
        self.dam_height.get(x, y).is_some_and(|&h| h > 0.0)
    }

    fn check(&self, x: usize, y: usize) -> Result<usize, WaterError> {
        if self.depth.in_bounds(x, y) { Ok(y * self.depth.width() + x) } else { Err(WaterError::OutOfBounds { x, y }) }
    }

    fn check_cells(&self, cells: &[Cell]) -> Result<Vec<usize>, WaterError> {
        if cells.is_empty() { return Err(WaterError::NoCells); }
        cells.iter().map(|&(x, y)| self.check(x, y)).collect()
    }

    fn check_positive(what: &'static str, value: f32) -> Result<(), WaterError> {
        if value.is_finite() && value > 0.0 { Ok(()) } else { Err(WaterError::InvalidValue { what, value }) }
    }

    /// Adds a river inflow at (x, y).
    pub fn add_source(&mut self, x: usize, y: usize, rate: f32) -> Result<(), WaterError> {
        self.check(x, y)?;
        Self::check_positive("source rate", rate)?;
        self.sources.push(Source { x, y, rate, seasonality: 1.0 });
        Ok(())
    }

    /// Raises dams per cell to at least the given height above terrain, at full integrity (a
    /// worn dam is repaired, a lower one raised, a higher one kept). All-or-nothing.
    pub fn raise_dam(&mut self, cells: &[(Cell, f32)]) -> Result<(), WaterError> {
        let only: Vec<Cell> = cells.iter().map(|c| c.0).collect();
        let idx = self.check_cells(&only)?;
        for &(_, h) in cells {
            Self::check_positive("dam height", h)?;
        }
        for (i, &(_, h)) in idx.into_iter().zip(cells) {
            if self.dam_height.as_slice()[i] <= 0.0 { self.dams.push(i); }
            let cur = &mut self.dam_height.as_mut_slice()[i];
            *cur = cur.max(h);
            self.dam_integrity.as_mut_slice()[i] = 1.0;
        }
        Ok(())
    }

    /// Builds (or replaces) dams of `height` above terrain on `cells` at full integrity.
    /// All-or-nothing: on error nothing changes.
    pub fn build_dam(&mut self, cells: &[Cell], height: f32) -> Result<(), WaterError> {
        let idx = self.check_cells(cells)?;
        Self::check_positive("dam height", height)?;
        for i in idx {
            if self.dam_height.as_slice()[i] <= 0.0 { self.dams.push(i); }
            self.dam_height.as_mut_slice()[i] = height;
            self.dam_integrity.as_mut_slice()[i] = 1.0;
        }
        Ok(())
    }

    /// Breaches (removes) the dam at (x, y); the pond behind it drains downstream.
    pub fn breach(&mut self, x: usize, y: usize) -> Result<(), WaterError> {
        let i = self.check(x, y)?;
        if self.dam_height.as_slice()[i] <= 0.0 { return Err(WaterError::NoDam { x, y }); }
        self.remove_dam(i);
        Ok(())
    }

    fn remove_dam(&mut self, i: usize) {
        self.dam_height.as_mut_slice()[i] = 0.0;
        self.dam_integrity.as_mut_slice()[i] = 0.0;
        self.dams.retain(|&d| d != i);
    }

    /// Digs canals: lowers terrain by `depth` on `cells`. All-or-nothing.
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

    /// Advances water by `dt_days` over `height`. Panics if `height` does not match the water size
    /// (a programming error). `dt_days` must be finite and >= 0 (validated by `SimState::step`).
    pub fn step(&mut self, height: &HeightGrid, dt_days: f32) {
        assert_eq!(
            (height.width(), height.height()),
            (self.depth.width(), self.depth.height()),
            "terrain size must match water size"
        );
        if dt_days <= 0.0 { return; }
        let n = ((dt_days * FLOW_STEPS_PER_DAY).ceil() as usize).clamp(1, MAX_SUBSTEPS);
        self.prepare_flow(height);
        for _ in 0..n { self.flow_substep(); }
        self.apply_sources_and_climate(dt_days);
        self.update_dams(height, dt_days);
        self.time_days += dt_days as f64;
    }

    /// Before a step's substeps: beds and the ring outside the map (terrain and dams do not change
    /// within a step), surfaces, and the active set: every wet cell and its neighbours. Only active
    /// cells can send or receive water this substep; a cell that gets wet adds its neighbours.
    fn prepare_flow(&mut self, height: &HeightGrid) {
        let (w, h) = (self.depth.width(), self.depth.height());
        let stride = w + 2;
        let terrain = height.as_slice();
        let dam = self.dam_height.as_slice();
        let depth = self.depth.as_slice();
        for y in 0..h {
            let row = (y + 1) * stride + 1;
            for x in 0..w {
                let i = y * w + x;
                let b = terrain[i] + dam[i];
                self.bs[row + x] = b;
                self.hs[row + x] = b + depth[i];
            }
        }
        // The ring outside the map: a wall, or (open edges) dry ground continuing the edge's slope,
        // only where the land keeps falling (or lies flat) outward. An edge the ground rises from is
        // the foot of an upstream valley the map cuts off, and water does not leave the world up it.
        {
            let bed = |x: usize, y: usize| terrain[y * w + x] + dam[y * w + x];
            let open = self.config.open_edges;
            let out = |e: f32, inner: f32| {
                let beyond = 2.0 * e - inner;
                if open && beyond <= e { beyond } else { WALL }
            };
            for y in 0..h {
                let row = (y + 1) * stride;
                let inner_l = if w > 1 { bed(1, y) } else { bed(0, y) };
                let inner_r = if w > 1 { bed(w - 2, y) } else { bed(w - 1, y) };
                self.hs[row] = out(bed(0, y), inner_l);
                self.hs[row + w + 1] = out(bed(w - 1, y), inner_r);
                (self.bs[row], self.bs[row + w + 1]) = (self.hs[row], self.hs[row + w + 1]);
            }
            for x in 0..w {
                let inner_t = if h > 1 { bed(x, 1) } else { bed(x, 0) };
                let inner_b = if h > 1 { bed(x, h - 2) } else { bed(x, h - 1) };
                self.hs[x + 1] = out(bed(x, 0), inner_t);
                self.hs[(h + 1) * stride + x + 1] = out(bed(x, h - 1), inner_b);
                self.bs[x + 1] = self.hs[x + 1];
                self.bs[(h + 1) * stride + x + 1] = self.hs[(h + 1) * stride + x + 1];
            }
        }
        // Active set: wet cells and their in-map neighbours. The last set is kept aside so a cell
        // leaving it can have its stored flows cleared: a neighbour must never read a dry,
        // inactive cell's old outflow as water arriving.
        for &(p, _) in &self.active {
            self.in_active[p as usize] = false;
        }
        let mut was = std::mem::take(&mut self.active);
        for y in 0..h {
            for x in 0..w {
                if depth[y * w + x] > 0.0 {
                    let p = (y + 1) * stride + x + 1;
                    for q in [p, p - 1, p + 1, p - stride, p + stride] {
                        let (qx, qy) = (q % stride, q / stride);
                        if qx >= 1 && qx <= w && qy >= 1 && qy <= h && !self.in_active[q] {
                            self.in_active[q] = true;
                            self.active.push((q as u32, ((qy - 1) * w + qx - 1) as u32));
                        }
                    }
                }
            }
        }
        for &(p, _) in &was {
            let p = p as usize;
            if !self.in_active[p] {
                (self.fl[p], self.fr[p], self.fu[p], self.fd[p]) = (0.0, 0.0, 0.0, 0.0);
            }
        }
        was.clear();
        self.spare = was;
    }

    fn flow_substep(&mut self) {
        let (w, h) = (self.depth.width(), self.depth.height());
        let stride = w + 2;
        // Fluxes, limited to the water available in the source cell.
        {
            let depth = self.depth.as_slice();
            let (hs, bs) = (&self.hs, &self.bs);
            for &(p, i) in &self.active {
                let (p, have) = (p as usize, depth[i as usize]);
                // a dry cell sends nothing
                if have <= 0.0 {
                    (self.fl[p], self.fr[p], self.fu[p], self.fd[p]) = (0.0, 0.0, 0.0, 0.0);
                    continue;
                }
                let (s, b) = (hs[p], bs[p]);
                // driven by the surface difference, scaled by the depth of water over the face
                // (above the higher of the two beds): flow needs depth
                let pipe = |old: f32, q: usize| {
                    let dh = s - hs[q];
                    let face = (s.max(hs[q]) - b.max(bs[q])).max(0.0);
                    let g = (face / FULL_FLOW_DEPTH).min(1.0);
                    (old * FRICTION + PUSH * dh * g).min(PIPE_CAP * dh).max(0.0)
                };
                let (l, r, u, d) = (pipe(self.fl[p], p - 1), pipe(self.fr[p], p + 1), pipe(self.fu[p], p - stride), pipe(self.fd[p], p + stride));
                let total = l + r + u + d;
                let k = if total > have { have / total } else { 1.0 };
                self.fl[p] = l * k;
                self.fr[p] = r * k;
                self.fu[p] = u * k;
                self.fd[p] = d * k;
            }
        }
        // Apply fluxes; a cell that gets wet brings its neighbours into the active set.
        let mut woke = std::mem::take(&mut self.woke);
        {
            let depth = self.depth.as_mut_slice();
            for &(p, i) in &self.active {
                let p = p as usize;
                let out = self.fl[p] + self.fr[p] + self.fu[p] + self.fd[p];
                let inn = self.fr[p - 1] + self.fl[p + 1] + self.fd[p - stride] + self.fu[p + stride];
                let c = &mut depth[i as usize];
                let was = *c;
                *c = (*c - out + inn).max(0.0);
                self.hs[p] = self.bs[p] + *c;
                if was <= 0.0 && *c > 0.0 {
                    woke.push(p);
                }
            }
        }
        for &p in &woke {
            for q in [p - 1, p + 1, p - stride, p + stride] {
                let (qx, qy) = (q % stride, q / stride);
                if qx >= 1 && qx <= w && qy >= 1 && qy <= h && !self.in_active[q] {
                    self.in_active[q] = true;
                    self.active.push((q as u32, ((qy - 1) * w + qx - 1) as u32));
                }
            }
        }
        woke.clear();
        self.woke = woke;
        // What flowed through the border into the ring has left the map.
        if self.config.open_edges {
            let mut lost = 0.0f64;
            for y in 0..h {
                let row = (y + 1) * stride + 1;
                lost += self.fl[row] as f64 + self.fr[row + w - 1] as f64;
            }
            for x in 0..w {
                lost += self.fu[stride + 1 + x] as f64 + self.fd[h * stride + 1 + x] as f64;
            }
            self.lost_volume += lost;
        }
    }

    fn apply_sources_and_climate(&mut self, dt: f32) {
        let season = self.season();
        let w = self.depth.width();
        let cfg = &self.config;
        let mut added = 0.0f64;
        for s in &self.sources {
            let q = s.rate * (1.0 + s.seasonality * (season.inflow_mult() - 1.0)) * dt;
            self.depth.as_mut_slice()[s.y * w + s.x] += q;
            added += q as f64;
        }
        self.books.sources += added;
        let (mut rained, mut evaporated, mut seeped) = (0.0f64, 0.0f64, 0.0f64);
        let rain = cfg.rain * season.rain_mult() * dt;
        let evap = cfg.evaporation * season.evap_mult() * dt;
        let seep = cfg.seepage * dt;
        let winter = season == Season::Winter;
        let (grow, melt) = (cfg.ice_growth_per_day * dt, cfg.ice_melt_per_day * dt);
        let mut lost = 0.0f64;
        for ((d, ice), &fed) in self.depth.as_mut_slice().iter_mut().zip(self.ice.as_mut_slice()).zip(self.fed.as_slice()) {
            if fed {
                *ice = if winter && *d > WET_EPS { (*ice + grow).min(MAX_ICE) } else { (*ice - melt).max(0.0) };
                continue;
            }
            // The ground soaks rain up to its absorption rate; only the excess adds water.
            let rain = (rain - cfg.soil_absorb * dt).max(0.0);
            *d += rain;
            added += rain as f64;
            rained += rain as f64;
            let wet = *d > WET_EPS;
            // even a last film dries out (it is only too thin to freeze)
            let (e, q) = if *d > 0.0 {
                let e = evap * (1.0 - *ice);
                (e, (e + seep).min(*d))
            } else {
                (0.0, 0.0)
            };
            *d -= q;
            lost += q as f64;
            // Evaporation goes first; seepage takes what is left of the loss.
            let e = (e as f64).min(q as f64);
            evaporated += e;
            seeped += q as f64 - e;
            *ice = if winter && wet { (*ice + grow).min(MAX_ICE) } else { (*ice - melt).max(0.0) };
        }
        self.added_volume += added;
        self.lost_volume += lost;
        self.books.rain += rained;
        self.books.evaporation += evaporated;
        self.books.seepage += seeped;
    }

    /// Dam wear: age, plus water pouring over the crest. The head over a crest is how far the
    /// highest water beside it (or on it) stands above the crest top: water spilling off into a free
    /// fall leaves almost none on the crest itself, so the crest's own depth would miss it.
    fn update_dams(&mut self, height: &HeightGrid, dt: f32) {
        let cfg = &self.config;
        let (w, h) = (self.depth.width(), self.depth.height());
        let (terrain, dam, depth) = (height.as_slice(), self.dam_height.as_slice(), self.depth.as_slice());
        let surface = |i: usize| terrain[i] + dam[i] + depth[i];
        let mut broken = Vec::new();
        for &i in &self.dams {
            let (x, y) = (i % w, i / w);
            let crest = terrain[i] + dam[i];
            let mut top = surface(i);
            if x > 0 { top = top.max(surface(i - 1)); }
            if x + 1 < w { top = top.max(surface(i + 1)); }
            if y > 0 { top = top.max(surface(i - w)); }
            if y + 1 < h { top = top.max(surface(i + w)); }
            let over = (top - crest - cfg.overtop_depth).max(0.0);
            let integ = &mut self.dam_integrity.as_mut_slice()[i];
            *integ -= (cfg.dam_decay_per_day + cfg.overtop_erosion * over) * dt;
            if *integ <= 0.0 { broken.push(i); }
        }
        for i in broken { self.remove_dam(i); }
    }
}

/// Previews the steady pond a dam of `height` on `cells` would raise, without mutating `state`.
///
/// The pond fills to the lowest crest of the new dam cells. It is the region of cells with a bed
/// below that level connected (4-neighbourhood) to the cells that drain *into* the dam (upstream
/// side). Existing dams count as bed height. Trees are cells with vegetation above
/// `TREE_VEGETATION`; buildings are `state.water.structures`. If the dam does not close the
/// valley the region leaks around it and `reaches_map_edge` or a large area results.
pub fn preview_dam(state: &SimState, cells: &[Cell], height: f32) -> Result<DamPreview, WaterError> {
    preview_dam_heights(state, cells, &vec![height; cells.len()])
}

/// `preview_dam` with a height per cell (a dam built to one crest over uneven ground). A cell
/// that already holds a higher dam keeps it.
pub fn preview_dam_heights(state: &SimState, cells: &[Cell], heights: &[f32]) -> Result<DamPreview, WaterError> {
    let water = &state.water;
    let idx = water.check_cells(cells)?;
    if heights.len() != cells.len() { return Err(WaterError::NoCells); }
    for &hh in heights { Water::check_positive("dam height", hh)?; }
    let terrain = state.height.as_slice();
    let (w, h) = (state.height.width(), state.height.height());
    let mut bed: Vec<f32> = terrain.iter().zip(water.dam_height.as_slice()).map(|(t, d)| t + d).collect();
    let mut is_new = vec![false; w * h];
    for (&i, &hh) in idx.iter().zip(heights) { bed[i] = terrain[i] + hh.max(water.dam_height.as_slice()[i]); is_new[i] = true; }
    let crest = idx.iter().map(|&i| bed[i]).fold(f32::INFINITY, f32::min);

    let neighbours = |i: usize| {
        let (x, y) = (i % w, i / w);
        [
            (x > 0).then(|| i - 1), (x + 1 < w).then(|| i + 1),
            (y > 0).then(|| i - w), (y + 1 < h).then(|| i + w),
        ].into_iter().flatten()
    };
    // Steepest-descent target over raw terrain.
    let drains_to = |i: usize| neighbours(i).filter(|&j| terrain[j] < terrain[i])
        .min_by(|&a, &b| terrain[a].total_cmp(&terrain[b]));

    // Spill: the lowest way round the dam, from the cells draining into it to the cells it drains
    // to (minimax path over bed heights, not crossing the new dam).
    let seeds: Vec<usize> = idx.iter().flat_map(|&d| neighbours(d).filter(move |&n| drains_to(n) == Some(d))).filter(|&n| !is_new[n]).collect();
    let below: std::collections::HashSet<usize> = idx.iter().filter_map(|&d| drains_to(d)).filter(|&n| !is_new[n]).collect();
    let (mut spill_level, mut spill_at) = (f32::INFINITY, None);
    if !seeds.is_empty() && !below.is_empty() {
        let mut best = vec![f32::INFINITY; w * h];
        let mut saddle = vec![usize::MAX; w * h];
        let mut heap = std::collections::BinaryHeap::new();
        let key = |l: f32, i: usize| std::cmp::Reverse(((l.max(-1.0e6) * 1000.0) as i64, i));
        for &s in &seeds { best[s] = bed[s]; saddle[s] = s; heap.push(key(bed[s], s)); }
        while let Some(std::cmp::Reverse((_, i))) = heap.pop() {
            let l = best[i];
            if l >= crest { break; }
            if below.contains(&i) { spill_level = l; spill_at = Some(saddle[i]); break; }
            for n in neighbours(i) {
                if is_new[n] { continue; }
                let nl = l.max(bed[n]);
                if nl < best[n] {
                    best[n] = nl;
                    saddle[n] = if bed[n] > l { n } else { saddle[i] };
                    heap.push(key(nl, n));
                }
            }
        }
    }
    let level = crest.min(spill_level);
    let spill_at = spill_at.filter(|_| spill_level < crest).map(|i| (i % w, i / w));

    let mut seen = vec![false; w * h];
    let mut stack = Vec::new();
    for &d in &idx {
        for n in neighbours(d) {
            if !is_new[n] && !seen[n] && bed[n] < level && drains_to(n) == Some(d) {
                seen[n] = true;
                stack.push(n);
            }
        }
    }
    let mut flooded = Vec::new();
    while let Some(i) = stack.pop() {
        flooded.push(i);
        for n in neighbours(i) {
            if !seen[n] && !is_new[n] && bed[n] < level { seen[n] = true; stack.push(n); }
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
        reaches_map_edge: flooded.iter().any(|&i| i % w == 0 || i % w == w - 1 || i / w == 0 || i / w == h - 1),
        flooded_cells: flooded.iter().map(|&i| (i % w, i / w)).collect(),
        crest,
        spill_at,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn quiet(s: &mut SimState) {
        s.water = Water::new(s.height.width()); // no springs or seeded channels
        s.water.config = WaterConfig {
            rain: 0.0, evaporation: 0.0, seepage: 0.0, dam_decay_per_day: 0.0,
            overtop_erosion: 0.0, open_edges: false, ..Default::default()
        };
    }

    /// Valley sloping down toward +x with rising sides.
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
        let start = s.water.total_volume(); // the seeded rivers and lakes
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
        pour(&mut s, 900.0); // far more than the pond holds
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

    /// A 64-cell valley falling eastward, its floor along y = 32, with a river entering at its
    /// west edge from `extra` cells of catchment beyond the map.
    fn inflow_valley(extra: u32) -> World {
        let n = 64;
        let raw: Vec<f32> = (0..n * n)
            .map(|i| {
                let (x, y) = ((i % n) as f32, (i / n) as f32);
                4.0 + 0.12 * (n as f32 - 1.0 - x) + 0.25 * (y - 32.0).abs()
            })
            .collect();
        crate::worldgen::from_heights(9, n, raw, &[(0, 32, extra)]).unwrap()
    }

    /// The water budget of a map at steady state: what the sources bring in (the river from
    /// beyond the map, springs, baseflow) leaves again through the downstream edge, evaporation
    /// and seepage, within 5%; and the river leaves the map carrying about what entered it plus
    /// what its own catchment gave (at least 80% of all the water brought in drains off the edge).
    #[test]
    fn water_budget_balances_at_steady_state_and_the_river_leaves_as_it_came() {
        let world = inflow_valley(3000);
        let inflow = world.water_inputs.iter().find(|i| i.kind == InputKind::Inflow).expect("the river from beyond the map");
        assert_eq!((inflow.cell, inflow.catchment), ((1, 32), 3000), "fed at the border cell's inner neighbour");
        let local: u32 = world.water_inputs.iter().filter(|i| i.kind != InputKind::Inflow).map(|i| i.catchment).sum();
        let mut w = Water::from_world(&world);
        w.config.seepage = 0.01; // a clay valley floor, as the starts have
        let given: f32 = w.sources.iter().map(|s| s.rate).sum();
        assert!((given - WATER_PER_CATCHMENT_CELL * (3000 + local) as f32).abs() < 1e-3, "sources follow the catchment: {given}");
        // Settle further into steady flow, then keep the books over a stretch of days.
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
        assert!(b.edges >= WATER_PER_CATCHMENT_CELL as f64 * 3000.0 * days as f64, "more leaves than entered from beyond the map");
    }

}
