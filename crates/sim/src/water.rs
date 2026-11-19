use crate::SimState;
use crate::grid::{Grid, HeightGrid, WaterGrid};
use crate::worldgen::World;
use std::cmp::Reverse;
use std::collections::BinaryHeap;
use std::fmt;
pub use crate::tuning::water::*;

pub type Cell = (usize, usize);

const DAY_S: f64 = 86_400.0;
const G: f32 = 9.81;
const CFL: f32 = 0.45;
const DT_MAX_S: f32 = 2.0;
const FACE_DRY: f32 = 1.0e-4;
const EDGE_SLOPE_MIN: f32 = 1.0e-3;
const TILE: usize = 16;
const STEP_SPEED_LIMIT: f32 = 1.0;
const MOVING: f32 = 0.02;
const SETTLE_S: f32 = 30.0;
const CALM_H: f32 = 1.0e-7;
const CALM_Q: f32 = 1.0e-6;
const DRIFT_M: f64 = 1.0e-4;
const WAKE_H: f32 = 2.0e-3;
const WAKE_Q: f32 = 2.0e-3;

#[derive(Clone, Debug)]
struct Frozen {
    since: f64,
    applied: f64,
    rmax: f32,
    edge: f64,
    rain: f64,
    evap: f64,
    seep: f64,
    lake: f64,
    ring: Vec<(u32, f32, f32, f32)>,
    border: Vec<Border>,
}

#[derive(Clone, Copy, Debug)]
struct Border {
    face: u32,
    horiz: bool,
    cell: u32,
    other: u32,
    sign: f32,
    f0: f32,
    acc: f32,
}

#[derive(Clone, Copy, Default)]
struct TileStats {
    rain: f64,
    evap: f64,
    seep: f64,
    lake: f64,
    wave: f32,
    edge: f64,
    rh: f32,
    rq: f32,
    wet: bool,
}

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
    books: Budget,
    h: Vec<f32>,
    z: Vec<f32>,
    hu: Vec<f32>,
    hv: Vec<f32>,
    fx: Vec<f32>,
    fy: Vec<f32>,
    active: Vec<usize>,
    wave_max: f32,
    pending: f64,
    frozen: Vec<Option<Frozen>>,
    frozen_mask: Vec<bool>,
    calm: Vec<f32>,
    rate: Vec<f32>,
    src_buf: Vec<f32>,
    frozen_season: Option<Season>,
    pub freeze_enabled: bool,
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
            books: Budget::default(),
            h: vec![0.0; n],
            z: vec![0.0; n],
            hu: vec![0.0; n],
            hv: vec![0.0; n],
            fx: vec![0.0; 4 * (size + 1) * size],
            fy: vec![0.0; 4 * size * (size + 1)],
            active: (0..size.div_ceil(TILE) * size.div_ceil(TILE)).collect(),
            wave_max: 0.0,
            pending: 0.0,
            frozen: vec![None; size.div_ceil(TILE) * size.div_ceil(TILE)],
            frozen_mask: vec![false; size.div_ceil(TILE) * size.div_ceil(TILE)],
            calm: vec![0.0; size.div_ceil(TILE) * size.div_ceil(TILE)],
            rate: vec![0.0; n],
            src_buf: vec![0.0; n],
            frozen_season: None,
            freeze_enabled: std::env::var_os("WATER_NOFREEZE").is_none(),
        }
    }

    pub fn frozen_tiles(&self) -> (usize, usize) {
        (self.frozen.iter().filter(|f| f.is_some()).count(), self.active.len())
    }

    fn tile_of(&self, i: usize) -> usize {
        let n = self.depth.width();
        (i / n / TILE) * self.tiles() + (i % n) / TILE
    }

    fn sync_tile(&mut self, t: usize) {
        let now = self.physics_seconds;
        let (x0, x1, y0, y1) = self.tile_cells(t);
        let n = self.depth.width();
        let Some(f) = self.frozen[t].as_mut() else { return };
        let span = (now - f.applied) as f32;
        f.applied = now;
        if span <= 0.0 {
            return;
        }
        for y in y0..y1 {
            for x in x0..x1 {
                let i = y * n + x;
                self.h[i] = (self.h[i] + self.rate[i] * span).max(0.0);
            }
        }
    }

    fn sync_frozen(&mut self) {
        for t in 0..self.frozen.len() {
            self.sync_tile(t);
        }
    }

    fn wake(&mut self, t: usize) {
        if self.frozen[t].is_some() {
            self.sync_tile(t);
            self.frozen[t] = None;
            self.frozen_mask[t] = false;
        }
        self.calm[t] = 0.0;
    }

    fn wake_around(&mut self, i: usize) {
        let tn = self.tiles();
        let t = self.tile_of(i);
        let (tx, ty) = (t % tn, t / tn);
        for ay in ty.saturating_sub(1)..(ty + 2).min(tn) {
            for ax in tx.saturating_sub(1)..(tx + 2).min(tn) {
                self.wake(ay * tn + ax);
            }
        }
    }

    fn freeze(&mut self, t: usize, s: &TileStats, dt: f32) {
        let n = self.depth.width();
        let (x0, x1, y0, y1) = self.tile_cells(t);
        let mut ring = Vec::new();
        let mut push = |x: usize, y: usize, ring: &mut Vec<(u32, f32, f32, f32)>| {
            let i = y * n + x;
            ring.push((i as u32, self.h[i], self.hu[i], self.hv[i]));
        };
        for x in x0..x1 {
            if y0 > 0 {
                push(x, y0 - 1, &mut ring);
            }
            if y1 < n {
                push(x, y1, &mut ring);
            }
        }
        for y in y0..y1 {
            if x0 > 0 {
                push(x0 - 1, y, &mut ring);
            }
            if x1 < n {
                push(x1, y, &mut ring);
            }
        }
        let w1 = n + 1;
        let mut border = Vec::new();
        for y in y0..y1 {
            if x0 > 0 {
                let f = y * w1 + x0;
                border.push(Border { face: f as u32, horiz: true, cell: (y * n + x0) as u32, other: (y * n + x0 - 1) as u32, sign: 1.0, f0: self.fx[4 * f], acc: 0.0 });
            }
            if x1 < n {
                let f = y * w1 + x1;
                border.push(Border { face: f as u32, horiz: true, cell: (y * n + x1 - 1) as u32, other: (y * n + x1) as u32, sign: -1.0, f0: self.fx[4 * f], acc: 0.0 });
            }
        }
        for x in x0..x1 {
            if y0 > 0 {
                let f = y0 * n + x;
                border.push(Border { face: f as u32, horiz: false, cell: f as u32, other: ((y0 - 1) * n + x) as u32, sign: 1.0, f0: self.fy[4 * f], acc: 0.0 });
            }
            if y1 < n {
                let f = y1 * n + x;
                border.push(Border { face: f as u32, horiz: false, cell: ((y1 - 1) * n + x) as u32, other: f as u32, sign: -1.0, f0: self.fy[4 * f], acc: 0.0 });
            }
        }
        let d = dt as f64;
        let now = self.physics_seconds;
        self.frozen[t] = Some(Frozen { since: now, applied: now, rmax: s.rh, edge: s.edge, rain: s.rain / d, evap: s.evap / d, seep: s.seep / d, lake: s.lake / d, ring, border });
        self.frozen_mask[t] = true;
        self.fold_borders(t);
    }

    fn fold_borders(&mut self, t: usize) {
        let tn = self.tiles();
        let n = self.depth.width();
        let dx = self.cell_m;
        let (tx, ty) = (t % tn, t / tn);
        let mut around = vec![t];
        if tx > 0 { around.push(t - 1) }
        if tx + 1 < tn { around.push(t + 1) }
        if ty > 0 { around.push(t - tn) }
        if ty + 1 < tn { around.push(t + tn) }
        for a in around {
            let Some(mut f) = self.frozen[a].take() else { continue };
            self.sync_tile_with(a, &mut f);
            for b in f.border.iter_mut() {
                let o = b.other as usize;
                let ot = (o / n / TILE) * tn + (o % n) / TILE;
                if !self.frozen_mask[ot] {
                    continue;
                }
                let now = if b.horiz { self.fx[4 * b.face as usize] } else { self.fy[4 * b.face as usize] };
                let r = &mut self.rate[b.cell as usize];
                *r += b.sign * (now - b.f0) / dx;
                f.rmax = f.rmax.max(r.abs());
                b.f0 = now;
            }
            self.frozen[a] = Some(f);
        }
    }

    fn sync_tile_with(&mut self, t: usize, f: &mut Frozen) {
        let now = self.physics_seconds;
        let (x0, x1, y0, y1) = self.tile_cells(t);
        let n = self.depth.width();
        let span = (now - f.applied) as f32;
        f.applied = now;
        if span <= 0.0 {
            return;
        }
        for y in y0..y1 {
            for x in x0..x1 {
                let i = y * n + x;
                self.h[i] = (self.h[i] + self.rate[i] * span).max(0.0);
            }
        }
    }

    fn correct_borders(frozen: &mut [Option<Frozen>], act: &[bool], fx: &[f32], fy: &[f32], h: &mut [f32], n: usize, k: f32) -> f64 {
        let tn = n.div_ceil(TILE);
        let mut clipped = 0.0f64;
        for f in frozen.iter_mut().flatten() {
            for b in f.border.iter_mut() {
                let o = b.other as usize;
                if act[(o / n / TILE) * tn + (o % n) / TILE] {
                    continue;
                }
                let now = if b.horiz { fx[4 * b.face as usize] } else { fy[4 * b.face as usize] };
                let d = b.sign * k * (now - b.f0);
                if d == 0.0 {
                    continue;
                }
                let c = b.cell as usize;
                let hh = h[c] + d;
                if hh < 0.0 {
                    clipped -= hh as f64;
                }
                h[c] = hh.max(0.0);
                b.acc += d;
            }
        }
        clipped
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
        w.wave_max = (0..n * n).filter(|&i| w.lake_level.as_slice()[i].is_nan()).map(|i| (G * w.h[i]).sqrt()).fold(0.0, f32::max);
        w.settle(SPIN_UP_S);
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

    pub fn budget(&self) -> Budget {
        self.books
    }

    pub fn season(&self) -> Season { Season::at(self.time_days) }

    pub fn total_volume(&self) -> f64 {
        self.h.iter().map(|&v| v as f64).sum()
    }

    pub fn flow_at(&self, x: usize, y: usize) -> (f32, f32) {
        let Some(&(vx, vy)) = self.velocity.get(x, y) else { return (0.0, 0.0) };
        let h = self.depth.get(x, y).copied().unwrap_or(0.0);
        let scale = h * self.cell_m * DAY_S as f32 / self.area() as f32;
        (vx * scale, vy * scale)
    }

    pub fn downstream(&self, x: usize, y: usize) -> Option<Cell> {
        let n = self.depth.width();
        if x >= n || y >= n {
            return None;
        }
        let i = y * n + x;
        let (u, v) = (self.hu[i], self.hv[i]);
        if (u * u + v * v).sqrt() < 1.0e-6 {
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
        self.wake_around(y * self.depth.width() + x);
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

    fn load_bed(&mut self, height: &HeightGrid) {
        assert_eq!((height.width(), height.height()), (self.depth.width(), self.depth.height()), "terrain size must match water size");
        let dam = self.dam_height.as_slice();
        let mut changed = Vec::new();
        for (i, (t, d)) in height.as_slice().iter().zip(dam).enumerate() {
            let z = t + d;
            if self.z[i] != z {
                changed.push(i);
            }
            self.z[i] = z;
        }
        let tn = self.tiles();
        let mut seen = vec![false; tn * tn];
        for i in changed {
            let t = self.tile_of(i);
            if !seen[t] {
                seen[t] = true;
                self.wake_around(i);
            }
        }
    }

    pub fn step(&mut self, height: &HeightGrid, dt_days: f32) {
        self.load_bed(height);
        self.pending += dt_days.max(0.0) as f64 * DAY_S;
        if dt_days <= 0.0 {
            self.publish();
            return;
        }
        let run = std::mem::take(&mut self.pending);
        self.advance(run);
        self.update_dams(height, dt_days);
        self.update_ice(dt_days);
        self.time_days += dt_days as f64;
        self.publish();
    }

    fn settle(&mut self, seconds: f64) {
        let mut t = 0.0;
        while t < seconds {
            t += self.substep((seconds - t) as f32) as f64;
        }
        self.sync_frozen();
    }

    fn advance(&mut self, seconds: f64) {
        let mut t = 0.0;
        while t < seconds - 1e-6 {
            t += self.substep((seconds - t) as f32) as f64;
        }
        self.sync_frozen();
    }

    fn tiles(&self) -> usize {
        self.depth.width().div_ceil(TILE)
    }

    fn tile_cells(&self, t: usize) -> (usize, usize, usize, usize) {
        let n = self.depth.width();
        let tn = self.tiles();
        let (tx, ty) = (t % tn, t / tn);
        (tx * TILE, ((tx + 1) * TILE).min(n), ty * TILE, ((ty + 1) * TILE).min(n))
    }

    fn zero_tile_faces(&mut self, t: usize) {
        let n = self.depth.width();
        let (x0, x1, y0, y1) = self.tile_cells(t);
        let tn = self.tiles();
        let (west, north) = (x0 > 0 && self.frozen_mask[t - 1], y0 > 0 && self.frozen_mask[t - tn]);
        for y in y0..y1 {
            for x in x0 + usize::from(west)..x1 + usize::from(x1 == n) {
                self.fx[4 * (y * (n + 1) + x)..4 * (y * (n + 1) + x) + 4].fill(0.0);
            }
        }
        for y in y0 + usize::from(north)..y1 + usize::from(y1 == n) {
            for x in x0..x1 {
                self.fy[4 * (y * n + x)..4 * (y * n + x) + 4].fill(0.0);
            }
        }
        for y in y0..y1 {
            for x in x0..x1 {
                self.hu[y * n + x] = 0.0;
                self.hv[y * n + x] = 0.0;
            }
        }
    }

    fn refresh_activity(&mut self) {
        let n = self.depth.width();
        let tn = self.tiles();
        let season = self.season();
        if self.frozen_season != Some(season) || !self.freeze_enabled {
            self.frozen_season = Some(season);
            for t in 0..tn * tn {
                self.wake(t);
            }
        }
        let now = self.physics_seconds;
        for t in 0..tn * tn {
            let wake = match &self.frozen[t] {
                None => false,
                Some(f) => {
                    f.rmax as f64 * (now - f.since) > DRIFT_M
                        || f.ring.iter().any(|&(i, h0, u0, v0)| {
                            let i = i as usize;
                            (self.h[i] - h0).abs() > WAKE_H || (self.hu[i] - u0).abs() + (self.hv[i] - v0).abs() > WAKE_Q
                        })
                        || f.border.iter().any(|b| b.acc.abs() > WAKE_H)
                }
            };
            if wake {
                self.wake(t);
            }
        }
        let mut wet = vec![false; tn * tn];
        let lake = self.lake_level.as_slice();
        for &t in &self.active {
            let (x0, x1, y0, y1) = self.tile_cells(t);
            'tile: for y in y0..y1 {
                for x in x0..x1 {
                    let i = y * n + x;
                    if self.h[i] > 0.0 && lake[i].is_nan() {
                        wet[t] = true;
                        break 'tile;
                    }
                }
            }
        }
        for s in &self.sources {
            wet[(s.y / TILE) * tn + s.x / TILE] = true;
        }
        let frozen = &self.frozen_mask;
        let mut on = vec![false; tn * tn];
        for ty in 0..tn {
            for tx in 0..tn {
                if !wet[ty * tn + tx] || frozen[ty * tn + tx] {
                    continue;
                }
                for ay in ty.saturating_sub(1)..(ty + 2).min(tn) {
                    for ax in tx.saturating_sub(1)..(tx + 2).min(tn) {
                        on[ay * tn + ax] |= !frozen[ay * tn + ax];
                    }
                }
            }
        }
        let leaving: Vec<usize> = self.active.iter().copied().filter(|&t| !on[t] && !frozen[t]).collect();
        for t in leaving {
            self.zero_tile_faces(t);
        }
        self.active = (0..tn * tn).filter(|&t| on[t]).collect();
    }

    fn substep(&mut self, limit: f32) -> f32 {
        use rayon::prelude::*;
        self.refresh_activity();
        let n = self.depth.width();
        let w1 = n + 1;
        let tn = self.tiles();
        let dx = self.cell_m;
        let dt = if self.active.is_empty() { limit.max(1.0e-4) } else { (CFL * dx / self.wave_max.max(0.05)).min(DT_MAX_S).min(limit.max(1.0e-4)) };
        let nn2 = self.config.manning * self.config.manning;
        let open = self.config.open_edges;
        let bounds = move |t: usize| ((t % tn) * TILE, ((t % tn + 1) * TILE).min(n), (t / tn) * TILE, ((t / tn + 1) * TILE).min(n));
        let lake = self.lake_level.as_slice();
        let closed = self.closed_edge.as_slice();
        let (z, h, hu, hv) = (&self.z, &self.h, &self.hu, &self.hv);
        let vel = |i: usize| if h[i] > FACE_DRY { (hu[i] / h[i], hv[i] / h[i]) } else { (0.0, 0.0) };
        let flux = |hl: f32, ul: f32, vl: f32, hr: f32, ur: f32, vr: f32| -> [f32; 3] {
            if hl <= 0.0 && hr <= 0.0 {
                return [0.0; 3];
            }
            let (cl, cr) = ((G * hl).sqrt(), (G * hr).sqrt());
            let sl = (ul - cl).min(ur - cr);
            let sr = (ul + cl).max(ur + cr);
            let fl = [hl * ul, hl * ul * ul + 0.5 * G * hl * hl, hl * ul * vl];
            let fr = [hr * ur, hr * ur * ur + 0.5 * G * hr * hr, hr * ur * vr];
            if sl >= 0.0 {
                return fl;
            }
            if sr <= 0.0 {
                return fr;
            }
            let (ql, qr) = ([hl, hl * ul, hl * vl], [hr, hr * ur, hr * vr]);
            let d = sr - sl;
            [0, 1, 2].map(|k| (sr * fl[k] - sl * fr[k] + sl * sr * (qr[k] - ql[k])) / d)
        };
        let face = |l: (f32, f32, f32, f32), r: (f32, f32, f32, f32)| -> [f32; 4] {
            let (zl, hl, ul, vl) = l;
            let (zr, hr, ur, vr) = r;
            let zf = zl.max(zr);
            let (hls, hrs) = ((hl + zl - zf).max(0.0), (hr + zr - zf).max(0.0));
            let carry = |h: f32, hs: f32, u: f32| {
                let cap = STEP_SPEED_LIMIT * (G * hs).sqrt();
                if hs > FACE_DRY { (u * h / hs).clamp(-cap, cap) } else { 0.0 }
            };
            let f = flux(hls, carry(hl, hls, ul), vl, hrs, carry(hr, hrs, ur), vr);
            [f[0], f[1] + 0.5 * G * (hl * hl - hls * hls), f[1] + 0.5 * G * (hr * hr - hrs * hrs), f[2]]
        };
        let ghost = |c: usize, inner: usize, normal_out: f32, along_x: bool| -> (f32, f32, f32, f32) {
            let (u, v) = vel(c);
            let (un, ut) = if along_x { (u, v) } else { (v, u) };
            if !open || closed[c] {
                return (z[c], h[c], -un, ut);
            }
            let zg = z[c] - ((z[inner] - z[c]) / dx).max(EDGE_SLOPE_MIN) * dx;
            let un = if un * normal_out > 0.0 { un } else { 0.0 };
            (zg, h[c], un, ut)
        };
        let cell = |i: usize, along_x: bool| -> (f32, f32, f32, f32) {
            let (u, v) = vel(i);
            if along_x { (z[i], h[i], u, v) } else { (z[i], h[i], v, u) }
        };
        let (pfx, pfy) = (Shared(self.fx.as_mut_ptr()), Shared(self.fy.as_mut_ptr()));
        let fm = &self.frozen_mask;
        self.active.par_iter().for_each(|&t| {
            let (x0, x1, y0, y1) = bounds(t);
            let (east, south) = (x1 < n && fm[t + 1], y1 < n && fm[t + tn]);
            for y in y0..y1 {
                for x in x0..x1 + usize::from(x1 == n || east) {
                    let k = 4 * (y * w1 + x);
                    let f = if x == 0 {
                        let c = y * n;
                        face(ghost(c, c + 1, -1.0, true), cell(c, true))
                    } else if x == n {
                        let c = y * n + n - 1;
                        face(cell(c, true), ghost(c, c - 1, 1.0, true))
                    } else {
                        let (a, b) = (y * n + x - 1, y * n + x);
                        if (lake[a].is_finite() && lake[b].is_finite()) || (h[a] <= 0.0 && h[b] <= 0.0) { [0.0; 4] } else { face(cell(a, true), cell(b, true)) }
                    };
                    for (j, v) in f.into_iter().enumerate() {
                        unsafe { pfx.write(k + j, v) };
                    }
                }
            }
            for y in y0..y1 + usize::from(y1 == n || south) {
                for x in x0..x1 {
                    let k = 4 * (y * n + x);
                    let f = if y == 0 {
                        face(ghost(x, x + n, -1.0, false), cell(x, false))
                    } else if y == n {
                        let c = (n - 1) * n + x;
                        face(cell(c, false), ghost(c, c - n, 1.0, false))
                    } else {
                        let (a, b) = ((y - 1) * n + x, y * n + x);
                        if (lake[a].is_finite() && lake[b].is_finite()) || (h[a] <= 0.0 && h[b] <= 0.0) { [0.0; 4] } else { face(cell(a, false), cell(b, false)) }
                    };
                    for (j, v) in f.into_iter().enumerate() {
                        unsafe { pfy.write(k + j, v) };
                    }
                }
            }
        });
        let clipped = Self::correct_borders(&mut self.frozen, &self.frozen_mask, &self.fx, &self.fy, &mut self.h, n, dt / dx);
        let season = self.season();
        let mut sourced = clipped;
        for s in &self.sources {
            let rate = s.rate * (1.0 + s.seasonality * (season.inflow_mult() - 1.0));
            let a = rate * dt / DAY_S as f32;
            if !self.frozen_mask[(s.y / TILE) * tn + s.x / TILE] {
                self.src_buf[s.y * n + s.x] += a;
            }
            sourced += a as f64;
        }
        let day = dt as f64 / DAY_S;
        let cfg = &self.config;
        let (r_day, e_day, s_day) = ((cfg.rain * season.rain_mult()) as f64 * day, (cfg.evaporation * season.evap_mult()) as f64 * day, cfg.seepage as f64 * day);
        let k = dt / dx;
        let (fx, fy) = (&self.fx, &self.fy);
        let (ph, phu, phv) = (Shared(self.h.as_mut_ptr()), Shared(self.hu.as_mut_ptr()), Shared(self.hv.as_mut_ptr()));
        let prate = Shared(self.rate.as_mut_ptr());
        let src = &self.src_buf;
        let stats: Vec<TileStats> = self
            .active
            .par_iter()
            .map(|&t| {
                let (x0, x1, y0, y1) = bounds(t);
                let (mut rain, mut evap, mut seep, mut lake_change, mut wave, mut edge) = (0.0f64, 0.0f64, 0.0f64, 0.0f64, 0.0f32, 0.0f64);
                let (mut rh, mut rq, mut wet) = (0.0f32, 0.0f32, false);
                for y in y0..y1 {
                    for x in x0..x1 {
                        let i = y * n + x;
                        let (w, e) = (4 * (y * w1 + x), 4 * (y * w1 + x + 1));
                        let (nf, sf) = (4 * (y * n + x), 4 * ((y + 1) * n + x));
                        let (h0, u0, v0) = unsafe { (ph.read(i), phu.read(i), phv.read(i)) };
                        let mut hh = h0 + src[i] - k * (fx[e] - fx[w] + fy[sf] - fy[nf]);
                        let mut mx = u0 - k * (fx[e + 1] - fx[w + 2] + fy[sf + 3] - fy[nf + 3]);
                        let mut my = v0 - k * (fx[e + 3] - fx[w + 3] + fy[sf + 1] - fy[nf + 2]);
                        if x == 0 {
                            edge -= fx[w].min(0.0) as f64;
                        }
                        if x + 1 == n {
                            edge += fx[e].max(0.0) as f64;
                        }
                        if y == 0 {
                            edge -= fy[nf].min(0.0) as f64;
                        }
                        if y + 1 == n {
                            edge += fy[sf].max(0.0) as f64;
                        }
                        let lvl = lake[i];
                        if lvl.is_finite() {
                            let want = (lvl - z[i]).max(0.0);
                            lake_change += (want - hh.max(0.0)) as f64;
                            hh = want;
                            mx = 0.0;
                            my = 0.0;
                        } else if hh > WET_EPS {
                            let vv = hh as f64;
                            let ev = e_day.min(vv + r_day);
                            let sp = s_day.min(vv + r_day - ev);
                            hh = (vv + r_day - ev - sp) as f32;
                            rain += r_day;
                            evap += ev;
                            seep += sp;
                        }
                        if hh <= FACE_DRY {
                            hh = hh.max(0.0);
                            mx = 0.0;
                            my = 0.0;
                        } else {
                            let speed = (mx * mx + my * my).sqrt() / hh;
                            let damp = 1.0 + G * dt * nn2 * speed / hh.powf(4.0 / 3.0);
                            mx /= damp;
                            my /= damp;
                            if lvl.is_nan() {
                                wave = wave.max(speed + (G * hh).sqrt());
                            }
                        }
                        let r = (hh - h0) / dt;
                        rh = rh.max(r.abs());
                        rq = rq.max(((mx - u0).abs() + (my - v0).abs()) / dt);
                        wet |= hh > 0.0 && lvl.is_nan();
                        unsafe {
                            ph.write(i, hh);
                            phu.write(i, mx);
                            phv.write(i, my);
                            prate.write(i, r);
                        }
                    }
                }
                TileStats { rain, evap, seep, lake: lake_change, wave, edge, rh, rq, wet }
            })
            .collect();
        for s in &self.sources {
            self.src_buf[s.y * n + s.x] = 0.0;
        }
        let (mut rain, mut evap, mut seep, mut lake_change, mut wave, mut edge_out) = (0.0f64, 0.0f64, 0.0f64, 0.0f64, 0.0f32, 0.0f64);
        for s in &stats {
            rain += s.rain;
            evap += s.evap;
            seep += s.seep;
            lake_change += s.lake;
            wave = wave.max(s.wave);
            edge_out += s.edge;
        }
        let span = dt as f64;
        for f in self.frozen.iter().flatten() {
            rain += f.rain * span;
            evap += f.evap * span;
            seep += f.seep * span;
            lake_change += f.lake * span;
            edge_out += f.edge;
        }
        self.wave_max = wave;
        let edge = edge_out * dt as f64 * dx as f64 / self.area();
        self.physics_seconds += dt as f64;
        if self.freeze_enabled {
            let active = std::mem::take(&mut self.active);
            for (&t, s) in active.iter().zip(&stats) {
                let calm = s.rh < CALM_H && s.rq < CALM_Q;
                self.calm[t] = if calm { self.calm[t] + dt } else { 0.0 };
                if self.calm[t] >= SETTLE_S && s.wet {
                    self.freeze(t, s, dt);
                }
            }
            self.active = active;
        }
        self.physics_seconds -= dt as f64;
        self.books.sources += sourced + lake_change.max(0.0);
        self.books.edges += edge + (-lake_change).max(0.0);
        self.books.rain += rain;
        self.books.evaporation += evap;
        self.books.seepage += seep;
        self.added_volume += sourced + lake_change.max(0.0) + rain;
        self.lost_volume += edge + (-lake_change).max(0.0) + evap + seep;
        self.physics_seconds += dt as f64;
        self.steps_taken += 1;
        dt
    }

    fn publish(&mut self) {
        let n = self.depth.width();
        let dx = self.cell_m;
        for y in 0..n {
            for x in 0..n {
                let i = y * n + x;
                let h = self.h[i];
                let (qxc, qyc) = (self.hu[i], self.hv[i]);
                let q = (qxc * qxc + qyc * qyc).sqrt();
                let (vx, vy) = if h > FACE_DRY { (qxc / h, qyc / h) } else { (0.0, 0.0) };
                self.depth.as_mut_slice()[i] = h;
                self.width.as_mut_slice()[i] = if h > 0.0 { dx } else { 0.0 };
                self.discharge.as_mut_slice()[i] = q * dx;
                self.velocity.as_mut_slice()[i] = (vx, vy);
                self.channel.as_mut_slice()[i] = h > 0.0 && (vx * vx + vy * vy).sqrt() > MOVING && self.lake_level.as_slice()[i].is_nan();
            }
        }
    }

    fn update_ice(&mut self, dt_days: f32) {
        let winter = self.season() == Season::Winter;
        let (grow, melt) = (self.config.ice_growth_per_day * dt_days, self.config.ice_melt_per_day * dt_days);
        for (ice, &d) in self.ice.as_mut_slice().iter_mut().zip(&self.h) {
            *ice = if winter && d > WET_EPS { (*ice + grow).min(MAX_ICE) } else { (*ice - melt).max(0.0) };
        }
    }

    fn update_dams(&mut self, height: &HeightGrid, dt: f32) {
        let w = self.depth.width();
        let (terrain, dam, depth) = (height.as_slice(), self.dam_height.as_slice(), &self.h);
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

const SPIN_UP_S: f64 = 9000.0;

#[derive(Clone, Copy)]
struct Shared(*mut f32);
unsafe impl Send for Shared {}
unsafe impl Sync for Shared {}

impl Shared {
    unsafe fn write(self, i: usize, v: f32) {
        unsafe { *self.0.add(i) = v }
    }
    unsafe fn read(self, i: usize) -> f32 {
        unsafe { *self.0.add(i) }
    }
}

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
        w.wave_max = w.h.iter().map(|&v| (G * v).sqrt()).fold(0.0, f32::max);
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
        w.wave_max = (G * 1.0f32).sqrt();
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
