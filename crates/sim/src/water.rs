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
mod subgrid;
mod automaton;
pub(crate) mod equilibrium;

/// Which water engine moves the water. Chosen once per world with `BC_WATER_ENGINE`
/// (routed, subgrid, automaton, equilibrium); the player's Settings choice otherwise, equilibrium by default.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum EngineKind {
    Routed,
    Subgrid,
    Automaton,
    Equilibrium,
}

/// The engine new worlds start with unless the player picks another.
pub const DEFAULT_ENGINE: EngineKind = EngineKind::Equilibrium;
static CHOSEN_ENGINE: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(u8::MAX);

/// The player's setting: new worlds use this engine.
pub fn set_default_engine(kind: EngineKind) {
    CHOSEN_ENGINE.store(kind as u8, std::sync::atomic::Ordering::Relaxed);
}

impl EngineKind {
    pub const ALL: [EngineKind; 4] = [EngineKind::Routed, EngineKind::Subgrid, EngineKind::Automaton, EngineKind::Equilibrium];
    /// `BC_WATER_ENGINE` (tools and tests) wins, then the player's setting, then the default.
    pub fn from_env() -> Self {
        if let Some(k) = std::env::var("BC_WATER_ENGINE").ok().and_then(|v| Self::parse(&v)) {
            return k;
        }
        Self::ALL.get(CHOSEN_ENGINE.load(std::sync::atomic::Ordering::Relaxed) as usize).copied().unwrap_or(DEFAULT_ENGINE)
    }
    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|e| e.name() == s)
    }
    pub fn name(self) -> &'static str {
        match self {
            EngineKind::Routed => "routed",
            EngineKind::Subgrid => "subgrid",
            EngineKind::Automaton => "automaton",
            EngineKind::Equilibrium => "equilibrium",
        }
    }
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
enum Engine {
    Routed(routed::Flow),
    Subgrid(subgrid::Subgrid),
    Automaton(automaton::Automaton),
    Equilibrium(equilibrium::Equilibrium),
}

impl Clone for Engine {
    fn clone(&self) -> Self {
        match self {
            Engine::Routed(e) => Engine::Routed(e.clone()),
            Engine::Subgrid(e) => Engine::Subgrid(e.clone()),
            Engine::Automaton(e) => Engine::Automaton(e.clone()),
            Engine::Equilibrium(e) => Engine::Equilibrium(e.clone()),
        }
    }
    fn clone_from(&mut self, src: &Self) {
        match (self, src) {
            (Engine::Routed(a), Engine::Routed(b)) => a.clone_from(b),
            (Engine::Subgrid(a), Engine::Subgrid(b)) => a.clone_from(b),
            (Engine::Automaton(a), Engine::Automaton(b)) => a.clone_from(b),
            (Engine::Equilibrium(a), Engine::Equilibrium(b)) => a.clone_from(b),
            (a, b) => *a = b.clone(),
        }
    }
}

impl Engine {
    fn new(kind: EngineKind) -> Self {
        match kind {
            EngineKind::Routed => Engine::Routed(Default::default()),
            EngineKind::Subgrid => Engine::Subgrid(Default::default()),
            EngineKind::Automaton => Engine::Automaton(Default::default()),
            EngineKind::Equilibrium => Engine::Equilibrium(Default::default()),
        }
    }
    fn kind(&self) -> EngineKind {
        match self {
            Engine::Routed(_) => EngineKind::Routed,
            Engine::Subgrid(_) => EngineKind::Subgrid,
            Engine::Automaton(_) => EngineKind::Automaton,
            Engine::Equilibrium(_) => EngineKind::Equilibrium,
        }
    }
}

macro_rules! each_engine {
    ($engine:expr, $e:ident => $body:expr) => {
        match $engine {
            Engine::Routed($e) => $body,
            Engine::Subgrid($e) => $body,
            Engine::Automaton($e) => $body,
            Engine::Equilibrium($e) => $body,
        }
    };
}
pub use climate::{Forcing, Weather};
pub use crate::calendar::Season;
const FACE_DRY: f32 = 1.0e-4;
/// Depth above which a cell's bed turns to bare sediment (m).
const RIVERBED_WET: f32 = 0.005;
/// Water deeper than this before and after a step has crossed none of the depths anything
/// waits on (wading, the riverbed, the shore), and `publish` passes it by.
const SHALLOW: f32 = if crate::buildings::WET_DEPTH > 0.03 { crate::buildings::WET_DEPTH } else { 0.03 };
const _: () = assert!(RIVERBED_WET <= SHALLOW);

/// The ground is drawn by how deep the water's edge lies on it: in whole centimetres, up to
/// three (`Water::shore`). A pond rising a metre over ground already under water changes none
/// of it.
pub fn shore_step(depth: f32) -> u8 {
    (depth >= 0.005) as u8 + (depth >= 0.015) as u8 + (depth >= 0.025) as u8
}

/// The riverbed is drawn in this many steps from turf to bare silt (see `DEPTH_STEP`).
pub const RIVERBED_STEPS: f32 = 16.0;

/// How far the depth (m) on some cell a tile reads must move before the tile of `depth` is
/// stamped as changed, and likewise the speed (m/s) for `velocity`: the water is drawn no
/// finer. Measured from when the tile was last stamped, so a pond creeping up a hair every
/// step is reported once for each such step up, not at every one.
pub const DEPTH_STEP: f32 = 0.002;
pub const SPEED_STEP: f32 = 0.05;

/// What follows when shallow water on a cell changes depth. Kept apart from the loop over every
/// published cell, which rarely needs any of it.
struct Edge<'a> {
    nav_revision: &'a mut u64,
    nav_marked: &'a mut Vec<bool>,
    nav_cells: &'a mut Vec<usize>,
    bed_live: &'a mut Option<Vec<u32>>,
    bed_marked: &'a mut Vec<bool>,
    shore: &'a mut [u8],
    shore_marks: crate::grid::Marks<'a>,
}

impl Edge<'_> {
    /// Cell `i`, at (x, y), went from `was` deep to `h`.
    #[cold]
    #[inline(never)]
    fn moved(&mut self, i: usize, x: usize, y: usize, was: f32, h: f32) {
        if (was > crate::buildings::WET_DEPTH) != (h > crate::buildings::WET_DEPTH) {
            *self.nav_revision = self.nav_revision.wrapping_add(1);
            if !self.nav_marked[i] {
                self.nav_marked[i] = true;
                self.nav_cells.push(i);
            }
        }
        if (was > RIVERBED_WET) != (h > RIVERBED_WET) {
            // Water came or went: the riverbed here starts changing.
            if let Some(live) = self.bed_live.as_mut() {
                if !self.bed_marked[i] {
                    self.bed_marked[i] = true;
                    live.push(i as u32);
                }
            }
        }
        let step = shore_step(h);
        if self.shore[i] != step {
            self.shore[i] = step;
            self.shore_marks.at(x, y);
        }
    }
}

/// What was last told of the grids that are drawn only so finely (`Water::note_changes`).
#[derive(Debug, Default)]
struct Told {
    depth: crate::grid::Told<f32>,
    speed: crate::grid::Told<(f32, f32)>,
    riverbed: crate::grid::Told<f32>,
    /// The tiles with water published, and with riverbed changed, since they were looked at.
    wet: crate::grid::Due,
    bed: crate::grid::Due,
    /// Stamp when asked (`note_changes`), not at every publish.
    on_request: bool,
}

const EDGE_SLOPE_MIN: f32 = 1.0e-3;
const MOVING: f32 = 0.02;

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
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

#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Source {
    pub x: usize,
    pub y: usize,
    pub rate: f32,
    pub seasonality: f32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Budget {
    pub sources: f64,
    pub rain: f64,
    pub edges: f64,
    pub evaporation: f64,
    pub seepage: f64,
}

pub fn sources_for(world: &World) -> Vec<Source> {
    world.water_inputs.iter().map(|inp| Source { x: inp.cell.0, y: inp.cell.1, rate: inp.rate, seasonality: 1.0 }).collect()
}

pub fn spill_head(q: f32, crest: f32) -> f32 {
    (q.max(0.0) / (WEIR * crest.max(1e-3))).powf(2.0 / 3.0)
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
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

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
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
    /// The level with `head` of flowing water on top, and the cells it covers: the still pond
    /// plus the shallow fringe the extra depth reaches on its side of the dam.
    pub flow_level: f32,
    pub flow_cells: Vec<Cell>,
}

/// Where the water engine spends its time, accumulated since the last `reset_timings`.
#[derive(Clone, Copy, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
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

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct Water {
    engine: Engine,
    routed_needs_sync: bool,
    pub depth: WaterGrid,
    pub ice: Grid<f32>,
    pub dam_height: Grid<f32>,
    pub dam_integrity: Grid<f32>,
    pub dam_durability: Grid<f32>,
    pub canal: Grid<bool>,
    /// Gate cells: a dam cell whose height the player sets (a sill). Gates don't erode.
    pub gate: Grid<bool>,
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
    /// Cells whose riverbed is still turning to silt or back to turf; `None` until a full pass
    /// lists them. Every other cell is at rest: bare under water or in a canal, turf when dry.
    #[serde(skip)]
    bed_live: Option<Vec<u32>>,
    #[serde(skip)]
    bed_marked: Vec<bool>,
    /// Whether the published grids match `h` (not so between an engine's publish and ours).
    #[serde(skip)]
    h_published: bool,
    /// `shore_step` of each cell's depth. Its tiles are stamped when the water's edge moves,
    /// where `depth`'s are stamped by every ripple.
    #[serde(skip)]
    pub shore: Grid<u8>,
    #[serde(skip)]
    told: crate::grid::Own<Told>,
    /// Cells whose published `velocity` changed since the nav last took the currents
    /// (`current_changes`), each listed once.
    #[serde(skip)]
    current_cells: Vec<u32>,
    #[serde(skip)]
    current_marked: Vec<bool>,
    /// The place's weather (see `weather`): set from the world's seed, so never saved.
    #[serde(skip)]
    pub sky: crate::weather::Sky,
}

crate::grid::clone_fields!(Water { engine, routed_needs_sync, depth, ice, dam_height, dam_integrity, dam_durability, canal, gate, fed, structures, closed_edge, width, channel, riverbed, discharge, velocity, lake_level, sources, config, weather, time_days, added_volume, lost_volume, cell_m, physics_seconds, revision, nav_revision, topology_revision, nav_cells, nav_marked, steps_taken, timings, books, n, h, z, bed_version, qx, qy, src, source_key, source_cells, rows, topology_dirty, pub_rows, full_publish, dam_cells, ice_any, since_environment, bed_live, bed_marked, h_published, shore, told, current_cells, current_marked, sky });

const FORCING_EVERY_S: f64 = 300.0;

/// Water thinner than this is not drawn (a damp film reads as wet ground, not water).
pub const SHOW_MIN_M: f32 = 0.003;
/// A thin film on a cell with no wet neighbour is damp ground, not a pool.
pub const LONE_FILM_M: f32 = 0.02;
/// Water standing on a dam cell shallower than this (rain on the crest, the last of a spill)
/// is not drawn: a sheet at crest height would show through the wall. A gate's water is always
/// drawn, and so is a real overtopping.
pub const DAM_FILM_M: f32 = 0.12;

impl Water {
    fn depth_at(&self, x: i32, y: i32) -> f32 {
        let (w, h) = (self.depth.width() as i32, self.depth.height() as i32);
        self.depth.as_slice()[(y.clamp(0, h - 1) * w + x.clamp(0, w - 1)) as usize]
    }

    /// A thin film with no water on any of its four sides.
    pub fn lone_film(&self, x: i32, y: i32) -> bool {
        let h = self.depth_at(x, y);
        h < LONE_FILM_M && [(-1, 0), (1, 0), (0, -1), (0, 1)].iter().all(|&(dx, dy)| self.depth_at(x + dx, y + dy) <= SHOW_MIN_M)
    }

    /// A thin sheet on a dam's crest (not at a gate).
    pub fn dam_film(&self, x: i32, y: i32) -> bool {
        let (cx, cy) = (x.max(0) as usize, y.max(0) as usize);
        self.depth_at(x, y) < DAM_FILM_M
            && self.dam_height.get(cx, cy).is_some_and(|&d| d > 0.0)
            && !self.gate.get(cx, cy).copied().unwrap_or(false)
    }

    /// Whether this cell is shown as water: what the player sees is what play goes by.
    pub fn shown(&self, x: i32, y: i32) -> bool {
        self.depth_at(x, y) > SHOW_MIN_M && !self.lone_film(x, y) && !self.dam_film(x, y)
    }

    pub fn new(size: usize) -> Self {
        let n = size;
        Self {
            engine: Engine::new(EngineKind::from_env()),
            routed_needs_sync: true,
            depth: Grid::new(size, size, 0.0),
            ice: Grid::new(size, size, 0.0),
            dam_height: Grid::new(size, size, 0.0),
            dam_integrity: Grid::new(size, size, 0.0),
            dam_durability: Grid::new(size, size, 1.0),
            canal: Grid::new(size, size, false),
            gate: Grid::new(size, size, false),
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
            bed_live: None,
            bed_marked: Vec::new(),
            h_published: false,
            shore: Grid::default(),
            told: Default::default(),
            current_cells: Vec::new(),
            current_marked: Vec::new(),
            sky: Default::default(),
        }
    }

    pub fn from_world(world: &World) -> Self {
        let n = world.size;
        let mut w = Self::new(n);
        w.cell_m = world.cell_m;
        w.sky = crate::weather::Sky::for_seed(world.seed);
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
        w.run_engine(SPIN_UP_S, false);
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

    /// Put the engine's order-free lists in one order, so that two waters holding the same
    /// water encode equally. Only for comparing runs: the order never changes what happens.
    pub fn canonical(&mut self) {
        if let Engine::Equilibrium(e) = &mut self.engine {
            e.canonical();
        }
    }

    pub fn reset_timings(&mut self) {
        self.timings = WaterTimings::default();
    }

    pub fn season(&self) -> Season { Season::at(self.time_days) }

    pub fn forcing(&self) -> Forcing { climate::forcing(self.time_days, self.weather, &self.sky) }

    /// Stored water in metres of depth summed over cells.
    pub fn total_volume(&self) -> f64 {
        each_engine!(&self.engine, e => e.total_volume()).unwrap_or_else(|| self.h.iter().map(|&h| h as f64).sum())
    }

    #[cfg(test)]
    pub fn engine(&self) -> EngineKind {
        self.engine.kind()
    }

    /// Hand the water to another engine mid-game. The new engine starts from the current
    /// depths, so no water is gained or lost by switching.
    pub fn set_engine(&mut self, kind: EngineKind) {
        if kind == self.engine.kind() {
            return;
        }
        // The new engine starts from the water on screen. Engines may store a little more or
        // less than they draw (thin reaches, banked inflow), so book the difference here and
        // the water budget stays exact across the switch.
        let stored = self.total_volume();
        let shown: f64 = self.h.iter().map(|&h| h as f64).sum();
        let change = shown - stored;
        if change >= 0.0 {
            self.books.sources += change;
            self.added_volume += change;
        } else {
            self.books.edges -= change;
            self.lost_volume -= change;
        }
        self.engine = Engine::new(kind);
        self.routed_needs_sync = true;
        self.full_publish = true;
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
                self.dam_height[i] = h;
                changed = true;
            }
            if self.dam_integrity.as_slice()[i] != 1.0 {
                self.dam_integrity[i] = 1.0;
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
            self.dam_height[i] = height;
            self.dam_integrity[i] = 1.0;
        }
        Ok(())
    }

    pub fn breach(&mut self, x: usize, y: usize) -> Result<(), WaterError> {
        let i = self.check(x, y)?;
        if self.dam_height.as_slice()[i] <= 0.0 {
            return Err(WaterError::NoDam { x, y });
        }
        self.dam_height[i] = 0.0;
        self.dam_integrity[i] = 0.0;
        self.revision = self.revision.wrapping_add(1);
        self.nav_revision = self.nav_revision.wrapping_add(1);
        self.topology_revision = self.topology_revision.wrapping_add(1);
        Ok(())
    }

    /// Take away whatever dam or gate stands on a cell (a cancelled dam or canal gate), so the
    /// water runs over its ground again.
    pub fn clear_structure(&mut self, cell: Cell) -> Result<(), WaterError> {
        let i = self.check(cell.0, cell.1)?;
        if self.dam_height.as_slice()[i] <= 0.0 && !self.gate.as_slice()[i] {
            return Ok(());
        }
        self.dam_height[i] = 0.0;
        self.dam_integrity[i] = 0.0;
        self.gate[i] = false;
        self.revision = self.revision.wrapping_add(1);
        self.nav_revision = self.nav_revision.wrapping_add(1);
        self.topology_revision = self.topology_revision.wrapping_add(1);
        self.topology_dirty = true;
        Ok(())
    }

    /// Set a gate's sill: `height` above this cell's ground, 0 for wide open. The cell is
    /// marked as a gate so overtopping never wears it away.
    pub fn set_gate(&mut self, cell: Cell, height: f32) -> Result<(), WaterError> {
        let i = self.check(cell.0, cell.1)?;
        if !height.is_finite() || height < 0.0 {
            return Err(WaterError::InvalidValue { what: "gate height", value: height });
        }
        let was = (self.dam_height.as_slice()[i], self.gate.as_slice()[i]);
        if was == (height, true) {
            return Ok(());
        }
        self.dam_height[i] = height;
        self.dam_integrity[i] = if height > 0.0 { 1.0 } else { 0.0 };
        self.gate[i] = true;
        self.revision = self.revision.wrapping_add(1);
        self.nav_revision = self.nav_revision.wrapping_add(1);
        self.topology_revision = self.topology_revision.wrapping_add(1);
        self.topology_dirty = true;
        Ok(())
    }

    /// Silt settles back into a dug canal cell; it stops being a canal once filled to `ground`.
    pub fn silt_canal(&mut self, height: &mut HeightGrid, cell: Cell, amount: f32, ground: f32) -> Result<(), WaterError> {
        let i = self.check(cell.0, cell.1)?;
        Self::check_positive("silt", amount)?;
        let h = &mut height[i];
        *h = (*h + amount).min(ground);
        if *h >= ground - 0.05 {
            self.canal[i] = false;
            self.bed_live = None;
        }
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
            height[i] -= depth;
            self.canal[i] = true;
            self.bed_live = None;
            self.riverbed[i] = 1.0;
        }
        Ok(())
    }

    pub(crate) fn clear_navigation_changes(&mut self) {
        for &i in &self.nav_cells {self.nav_marked[i]=false;}
        self.nav_cells.clear();
        for &i in &self.current_cells {self.current_marked[i as usize]=false;}
        self.current_cells.clear();
    }

    /// Cells whose current changed since the navigation changes were last cleared.
    pub(crate) fn current_changes(&self) -> &[u32] {
        &self.current_cells
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
        let inflow = climate::forcing(forcing_time, self.weather, &self.sky).inflow;
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
        { let _p = crate::prof::scope("step/water/load bed"); self.load_bed(height); }
        self.timings.upkeep_s += t0.elapsed().as_secs_f64();
        if !dt_days.is_finite() || dt_days <= 0.0 {
            self.publish();
            return;
        }
        self.run_engine(dt_days as f64 * DAY_S, true);
        let t0 = std::time::Instant::now();
        { let _p = crate::prof::scope("step/water/dam wear"); self.update_dams(height, dt_days); }
        { let _p = crate::prof::scope("step/water/ice"); self.update_ice(dt_days); }
        self.timings.upkeep_s += t0.elapsed().as_secs_f64();
        let _p = crate::prof::scope("step/water/publish grids");
        self.publish();
    }

    /// Move water and apply weather on the same fifteen-minute clock, whichever engine runs.
    fn run_engine(&mut self, seconds:f64, losses:bool) {
        if self.topology_dirty {
            self.dam_cells = (0..self.n * self.n).filter(|&i| self.dam_height.as_slice()[i] > 0.0).map(|i| i as u32).collect();
            self.topology_dirty = false;
            self.full_publish = true;
        }
        let kind = self.engine.kind();
        let mut engine = std::mem::replace(&mut self.engine, Engine::new(kind));
        each_engine!(&mut engine, flow => self.run_with(flow, seconds, losses));
        self.engine = engine;
    }
}

/// What every water engine provides. Engines read the bed (`z`, terrain plus dams), the
/// source rates (`src`, metres per second on `source_cells`), the fixed edge-lake levels
/// (`lake_level`), the edges (`closed_edge`, `config.open_edges`) and the weather, and
/// publish depth (`h`), face fluxes (`qx`, `qy` in m²/s) and the active row spans (`rows`).
/// Every volume entering or leaving is booked in `books` and `added_volume`/`lost_volume`.
pub(crate) trait WaterEngine {
    fn prepare(&mut self, w: &mut Water, sync: bool);
    fn advance(&mut self, w: &mut Water, seconds: f64);
    fn environment(&mut self, w: &mut Water, days: f64);
    fn publish(&mut self, w: &mut Water);
    /// Stored water in cubic metres per square metre of cell, if the engine keeps its own.
    fn total_volume(&self) -> Option<f64>;
}

impl Water {
    fn run_with<E: WaterEngine>(&mut self, flow: &mut E, seconds: f64, losses: bool) {
        let mut left=seconds;
        while left>1.0e-6 {
            let limit=if losses {900.0-self.since_environment} else {seconds};
            // f32 calendar steps arrive a few microseconds over 900 seconds.
            // Do not run the entire graph again for that rounding remainder.
            let dt=if left<=limit+1.0e-3 {left} else {limit};
            self.refresh_sources();
            let started=std::time::Instant::now();
            let sync=std::mem::take(&mut self.routed_needs_sync);
            { let _p=crate::prof::scope("step/water/engine prepare"); flow.prepare(self,sync); }
            { let _p=crate::prof::scope("step/water/engine advance"); flow.advance(self,dt); }
            if losses {
                self.time_days+=dt/DAY_S;
                self.since_environment+=dt;
                if self.since_environment>=900.0-1.0e-3 {
                    let _p=crate::prof::scope("step/water/engine weather");
                    flow.environment(self,self.since_environment/DAY_S);
                    self.since_environment=0.0;
                }
            }
            { let _p=crate::prof::scope("step/water/engine publish"); flow.publish(self); }
            self.h_published=false;
            self.timings.faces_s+=started.elapsed().as_secs_f64();
            left-=dt;
        }
    }

    /// Remember bare sediment after floodwater recedes. Only cells still changing are visited:
    /// a cell at rest stays there until its water comes or goes (`publish` wakes it) or a
    /// canal is dug through it.
    fn update_riverbed(&mut self, dt: f32) {
        let next = |bed: f32, canal: bool, h: f32| {
            if canal {
                1.0
            } else if h > RIVERBED_WET {
                // Newly flooded turf becomes bare soil over half a day. Established beds
                // remain exposed through a dry season; regrowth is deliberately slow.
                (bed + dt * 2.0).min(1.0)
            } else {
                (bed - dt / 180.0).max(0.0)
            }
        };
        let resting = |bed: f32, canal: bool, h: f32| bed == if canal || h > RIVERBED_WET { 1.0 } else { 0.0 };
        if !self.h_published {
            // Depths moved without passing through `publish`: nothing was woken.
            self.bed_live = None;
        }
        let canal = self.canal.as_slice();
        let due = &mut self.told.0.bed;
        due.sized(self.n);
        let Some(mut live) = self.bed_live.take() else {
            let mut live = Vec::new();
            self.bed_marked.clear();
            self.bed_marked.resize(self.n * self.n, false);
            due.all();
            for (i, bed) in self.riverbed.cells_and_marks().0.iter_mut().enumerate() {
                *bed = next(*bed, canal[i], self.h[i]);
                if !resting(*bed, canal[i], self.h[i]) {
                    live.push(i as u32);
                    self.bed_marked[i] = true;
                }
            }
            self.bed_live = Some(live);
            return;
        };
        if !live.is_empty() {
            let n = self.n;
            let beds = self.riverbed.cells_and_marks().0;
            live.retain(|&i| {
                let i = i as usize;
                let bed = next(beds[i], canal[i], self.h[i]);
                if bed != beds[i] {
                    due.row(i / n, crate::grid::Due::across(i % n, i % n + 1));
                }
                beds[i] = bed;
                let keep = !resting(beds[i], canal[i], self.h[i]);
                self.bed_marked[i] = keep;
                keep
            });
        }
        self.bed_live = Some(live);
    }

    /// One explicit step of the local-inertial shallow-water equations (Bates et al. 2010):
    /// momentum per face from the water-surface slope with semi-implicit Manning friction, then
    /// mass per cell. Pools take what crosses their rim and hold one level. Returns the step in s.
    fn publish(&mut self) {
        self.revision = self.revision.wrapping_add(1);
        let t0 = std::time::Instant::now();
        let n = self.n;
        if self.shore.width() != n {
            self.shore = Grid::new(n, n, 0);
            for (shore, &d) in self.shore.as_mut_slice().iter_mut().zip(self.depth.as_slice()) {
                *shore = shore_step(d);
            }
        }
        self.told.0.wet.sized(n);
        let full = std::mem::take(&mut self.full_publish);
        if full {
            // Every cell may have changed: list the changing riverbed afresh.
            self.bed_live = None;
        }
        {
            let Self {
                cell_m, h, qx, qy, lake_level, depth, width, discharge, velocity, channel, rows, pub_rows,
                nav_revision, nav_marked, nav_cells, bed_live, bed_marked, shore, told, current_cells, current_marked, ..
            } = self;
            if current_marked.len() != n * n {
                *current_marked = vec![false; n * n];
                current_cells.clear();
            }
            // A cell whose current is no longer what the nav took is listed for it.
            let mut flowed = |i: usize, was: (f32, f32), now: (f32, f32)| {
                if (was.0.to_bits() != now.0.to_bits() || was.1.to_bits() != now.1.to_bits()) && !current_marked[i] {
                    current_marked[i] = true;
                    current_cells.push(i as u32);
                }
            };
            let w1 = n + 1;
            let dx = *cell_m;
            let lake = lake_level.as_slice();
            // Depth and speed are stamped in `note_changes`, which looks at the tiles written here.
            let wet = &mut told.0.wet;
            let (depth, velocity) = (depth.cells_and_marks().0, velocity.cells_and_marks().0);
            let (shore, shore_marks) = shore.cells_and_marks();
            let mut edge = Edge { nav_revision, nav_marked, nav_cells, bed_live, bed_marked, shore, shore_marks };
            let (width, discharge, channel) = (width.as_mut_slice(), discharge.as_mut_slice(), channel.as_mut_slice());
            // Each row's spans (and, where they changed, last time's too: water that has gone
            // must be cleared), copied to the grids the rest of the world reads.
            let whole = [(0u32, n as u32)];
            for y in 0..n {
                let (now, before): (&[(u32, u32)], &[(u32, u32)]) = if full {
                    (&whole, &[])
                } else if rows[y] == pub_rows[y] {
                    (&rows[y], &[])
                } else {
                    (&rows[y], &pub_rows[y])
                };
                let mut across = 0;
                for &(a, b) in now.iter().chain(before) {
                    let (a, b) = (a as usize, b as usize);
                    if a >= b {
                        continue;
                    }
                    across |= crate::grid::Due::across(a, b);
                    for x in a..b {
                        let i = y * n + x;
                        let h = h[i];
                        let was = depth[i];
                        // Deep water staying deep (most of a pond) crosses none of the depths anything
                        // waits on, nor does a cell left as it was (the dry ground between a row's waters).
                        if (was.to_bits() != h.to_bits()) & ((was <= SHALLOW) | (h <= SHALLOW)) {
                            edge.moved(i, x, y, was, h);
                        }
                        let qxc = 0.5 * (qx[y * w1 + x] + qx[y * w1 + x + 1]);
                        let qyc = 0.5 * (qy[y * n + x] + qy[(y + 1) * n + x]);
                        depth[i] = h;
                        width[i] = if h > 0.0 { dx } else { 0.0 };
                        if qxc.to_bits() == 0 && qyc.to_bits() == 0 {
                            // Still water (most of a pond): the same zeros, without the roots and divisions.
                            discharge[i] = 0.0;
                            flowed(i, velocity[i], (0.0, 0.0));
                            velocity[i] = (0.0, 0.0);
                            channel[i] = false;
                            continue;
                        }
                        let q = (qxc * qxc + qyc * qyc).sqrt();
                        let (vx, vy) = if h > FACE_DRY { (qxc / h, qyc / h) } else { (0.0, 0.0) };
                        discharge[i] = q * dx;
                        flowed(i, velocity[i], (vx, vy));
                        velocity[i] = (vx, vy);
                        channel[i] = h > 0.0 && (vx * vx + vy * vy).sqrt() > MOVING && lake[i].is_nan();
                    }
                }
                if across != 0 {
                    wet.row(y, across);
                }
            }
        }
        self.pub_rows.clone_from(&self.rows);
        self.h_published = true;
        self.timings.publish_s += t0.elapsed().as_secs_f64();
        if !self.told.0.on_request {
            self.note_changes();
        }
    }

    /// From now on, `depth`, `velocity` and `riverbed` are stamped only when `note_changes` is called:
    /// for an owner that shows the world less often than it steps it.
    pub fn note_on_request(&mut self) {
        self.told.0.on_request = true;
    }

    /// Stamp the tiles of `depth`, `velocity` and `riverbed` in which some cell has moved by
    /// more than is drawn of it (`DEPTH_STEP`, `SPEED_STEP`, a step of `RIVERBED_STEPS`) since
    /// the tile was last stamped. Only tiles written since the last call are looked at.
    pub fn note_changes(&mut self) {
        let _p = crate::prof::scope("note changes");
        let told = &mut self.told.0;
        told.wet.sized(self.n);
        told.bed.sized(self.n);
        told.depth.note(&mut self.depth, Some(&told.wet), |a, b| (a - b).abs() > DEPTH_STEP);
        told.speed.note(&mut self.velocity, Some(&told.wet), |a, b| (a.0 - b.0).abs() > SPEED_STEP || (a.1 - b.1).abs() > SPEED_STEP);
        told.riverbed.note(&mut self.riverbed, Some(&told.bed), |a, b| (a - b).abs() > 1.0 / RIVERBED_STEPS);
        told.wet.clear();
        told.bed.clear();
    }

    /// How much of a cell the ice covers, 0 to 1: a skin at first, whole once `ICE_COVER_M` thick.
    pub fn ice_cover(&self, x: usize, y: usize) -> f32 {
        self.ice.get(x, y).map_or(0.0, |t| (t / ICE_COVER_M).min(1.0))
    }

    /// Ice (metres thick) follows the last days' air: it thickens on still water while that is
    /// below freezing, quickly at first and slower as it thickens, and melts once it is above.
    /// Water running fast stays open, so a winter river keeps open leads and rapids.
    fn update_ice(&mut self, dt_days: f32) {
        let air = self.sky.at(self.time_days).ground_c;
        if air >= 0.0 && !self.ice_any {
            return;
        }
        let (mut any, mut changed) = (false, false);
        // Cells whose ice now bears a beaver, or no longer does: walked, not swum (see `nav`).
        let mut firm: Vec<usize> = Vec::new();
        let velocity = self.velocity.as_slice();
        let (cells, mut marks) = self.ice.cells_and_marks();
        for (i, ((ice, &d), v)) in cells.iter_mut().zip(&self.h).zip(velocity).enumerate() {
            let still = (1.0 - v.0.hypot(v.1) / ICE_OPEN_SPEED).clamp(0.0, 1.0);
            let now = if d <= WET_EPS {
                0.0
            } else if air < 0.0 && still > 0.0 {
                // Stefan's law: growth slows as the ice thickens.
                (*ice + ICE_STEFAN * -air / (*ice + ICE_SEED_M) * still * dt_days).min(MAX_ICE)
            } else {
                // Thawing air, or water running too fast to stay frozen.
                let thaw = ICE_MELT_PER_DEGREE_DAY * air.max(0.0) + ICE_FLOW_MELT * (1.0 - still);
                (*ice - thaw * dt_days).max(0.0)
            };
            let now = if now < 1e-4 { 0.0 } else { now };
            if (now >= ICE_WALK_M) != (*ice >= ICE_WALK_M) {
                firm.push(i);
            }
            changed |= now != *ice;
            any |= now > 0.0;
            *ice = now;
        }
        if changed {
            marks.all();
        }
        self.ice_any = any;
        if !firm.is_empty() {
            self.nav_revision = self.nav_revision.wrapping_add(1);
            for i in firm {
                if !self.nav_marked[i] {
                    self.nav_marked[i] = true;
                    self.nav_cells.push(i);
                }
            }
        }
    }

    /// Integrity a dam cell loses a day: from age, and from water running over its crest
    /// (deeper than the allowance), each divided by the cell's durability (pebble facing).
    pub fn dam_cell_wear(&self, height: &HeightGrid, i: usize) -> (f32, f32) {
        let w = self.n;
        let (terrain, dam, depth) = (height.as_slice(), self.dam_height.as_slice(), &self.h);
        let surface = |i: usize| if depth[i] > WET_EPS { terrain[i] + dam[i] + depth[i] } else { f32::NEG_INFINITY };
        let (x, y) = (i % w, i / w);
        let crest = terrain[i] + dam[i];
        let mut top = surface(i);
        for (ok, j) in [(x > 0, i.wrapping_sub(1)), (x + 1 < w, i + 1), (y > 0, i.wrapping_sub(w)), (y + 1 < w, i + w)] {
            if ok {
                top = top.max(surface(j));
            }
        }
        let over = (top - crest - self.config.overtop_depth).max(0.0);
        let hold = self.dam_durability.as_slice()[i].max(1.0);
        (self.config.dam_decay_per_day / hold, self.config.overtop_erosion * over / hold)
    }

    fn update_dams(&mut self, height: &HeightGrid, dt: f32) {
        let mut broken = Vec::new();
        let cells: Vec<usize> = self.dam_cells.iter().map(|&i| i as usize).filter(|&i| self.dam_height.as_slice()[i] > 0.0 && !self.gate.as_slice()[i]).collect();
        for i in cells {
            let (age, top) = self.dam_cell_wear(height, i);
            let integ = &mut self.dam_integrity[i];
            *integ -= (age + top) * dt;
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
            self.dam_height[i] = 0.0;
            self.dam_integrity[i] = 0.0;
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

/// For a dam line crossing water: a test of which side of it a neighbouring cell lies, by the
/// line's own run there (positive downstream, negative upstream, zero along the line).
/// Downstream is where the current over the wet cells runs, or failing that the side where the
/// water beside them stands lower. `None` when the line crosses no water to tell by.
fn dam_sides<'a>(state: &'a SimState, cells: &'a [Cell], idx: &'a [usize]) -> Option<impl Fn(usize, usize) -> i32 + 'a> {
    let water = &state.water;
    let depth = water.depth.as_slice();
    let n = state.height.width();
    let at = move |i: usize| ((i % n) as i64, (i / n) as i64);
    let tangent = move |k: usize| {
        let (a, b) = (cells[k.saturating_sub(1)], cells[(k + 1).min(cells.len() - 1)]);
        (b.0 as i64 - a.0 as i64, b.1 as i64 - a.1 as i64)
    };
    let raw = move |k: usize, m: usize| {
        let (t, c, p) = (tangent(k), at(idx[k]), at(m));
        (t.0 * (p.1 - c.1) - t.1 * (p.0 - c.0)).signum() as i32
    };
    let wet: Vec<usize> = (0..idx.len()).filter(|&k| depth[idx[k]] > WET_EPS).collect();
    if wet.is_empty() {
        return None;
    }
    let vel = water.velocity.as_slice();
    let mut by_flow = 0.0f32;
    for &k in &wet {
        let (t, v) = (tangent(k), vel[idx[k]]);
        by_flow += t.0 as f32 * v.1 - t.1 as f32 * v.0;
    }
    let terrain = state.height.as_slice();
    let surface = |i: usize| terrain[i] + water.dam_height.as_slice()[i] + depth[i].max(0.0);
    let mut by_level = 0.0f32;
    for &k in &wet {
        for m in neighbours8(n, idx[k]).filter(|&m| depth[m] > WET_EPS && !idx.contains(&m)) {
            by_level -= raw(k, m) as f32 * surface(m);
        }
    }
    let sign = if by_flow.abs() > 1e-4 { by_flow.signum() } else if by_level.abs() > 1e-4 { by_level.signum() } else { return None };
    Some(move |k: usize, m: usize| raw(k, m) * sign as i32)
}

/// The still pond these dam heights would hold, and (with `head` > 0) how far the extra depth
/// of water flowing over its spill would spread.
pub fn preview_dam_heights(state: &SimState, cells: &[Cell], heights: &[f32], head: f32) -> Result<DamPreview, WaterError> {
    Ok(StillPond::behind(state, cells, heights)?.preview(state, head))
}

/// The still pond behind planned dam heights: found once, then asked for the pond itself and
/// for how far flowing water spreads over it (a dam's estimate needs both).
pub(crate) struct StillPond {
    n: usize,
    /// Terrain plus dams, with the planned heights in place.
    bed: Vec<f32>,
    /// The planned dam's own cells.
    is_new: Vec<bool>,
    /// Cells the still pond covers, and where its flood started.
    seen: Vec<bool>,
    seeds: Vec<usize>,
    flooded: Vec<usize>,
    level: f32,
    crest: f32,
    spill_at: Option<Cell>,
}

impl StillPond {
    pub(crate) fn behind(state: &SimState, cells: &[Cell], heights: &[f32]) -> Result<StillPond, WaterError> {
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
        let _p = crate::prof::scope("+still pond/setup");
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
        // Which side of the line is downstream, from the water it crosses: where its current
        // runs, else the side its water stands lower. The ground's slope alone can't say: on
        // a long dam mostly over land, the banks on both sides slope down to the line.
        let side = (idx.len() > 1).then(|| dam_sides(state, cells, &idx)).flatten();
        let mut seeds: Vec<usize> = Vec::new();
        // Cells just below the dam: the flood has found its way round once it reaches one.
        let mut below = vec![false; n * n];
        let mut any_below = false;
        match &side {
            Some(side) => {
                for (k, &d) in idx.iter().enumerate() {
                    for m in neighbours8(n, d).filter(|&m| !is_new[m]) {
                        match side(k, m) {
                            s if s < 0 && (drains_to(m) == Some(d) || water.depth.as_slice()[m] > WET_EPS) => seeds.push(m),
                            s if s > 0 => {
                                below[m] = true;
                                any_below = true;
                            }
                            _ => {}
                        }
                    }
                }
                seeds.sort_unstable();
                seeds.dedup();
            }
            None => {
                seeds = idx.iter().flat_map(|&d| neighbours8(n, d).filter(move |&m| drains_to(m) == Some(d))).filter(|&m| !is_new[m]).collect();
                for m in idx.iter().filter_map(|&d| drains_to(d)).filter(|&m| !is_new[m]) {
                    below[m] = true;
                    any_below = true;
                }
            }
        }
        let (mut spill_level, mut spill_at) = (f32::INFINITY, None);
        drop(_p);
        let _p = crate::prof::scope("+still pond/find spill");
        if !seeds.is_empty() && any_below {
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
                if below[i] {
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
        drop(_p);
        let _p = crate::prof::scope("+still pond/flood");
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
        crate::prof::count("+still pond/flood/cells", flooded.len() as u64);
        // In map order from here on: every use wants it sorted.
        flooded.sort_unstable();
        drop(_p);
        Ok(StillPond { n, bed, is_new, seen, seeds, flooded, level, crest, spill_at })
    }

    pub(crate) fn preview(&self, state: &SimState, head: f32) -> DamPreview {
        let water = &state.water;
        let (n, bed, is_new, level) = (self.n, &self.bed, &self.is_new, self.level);
        let dams = water.dam_height.as_slice();
        let pass = |i: usize, j: usize| bed[i].max(bed[j]).max(corner(bed, |k| is_new[k] || dams[k] > 0.0, n, i, j));
        let flow_level = level + head.max(0.0);
        let flooded = &self.flooded;
        let mut flow = flooded.clone();
        if head > 0.0 {
            let mut seen = self.seen.clone();
            // The water reaching the dam stands `head` deep even where no still pond forms (it
            // runs straight round an end): start from the cells draining into the dam too.
            for &m in &self.seeds {
                if !seen[m] && bed[m] < flow_level {
                    seen[m] = true;
                    flow.push(m);
                }
            }
            // Only up into the shallow fringe: cells below the still level that the still pond
            // did not reach lie past a saddle, where the water runs away instead.
            let mut stack = flow.clone();
            while let Some(i) = stack.pop() {
                for m in neighbours8(n, i) {
                    if !seen[m] && !is_new[m] && bed[m] >= level && pass(i, m) < flow_level {
                        seen[m] = true;
                        flow.push(m);
                        stack.push(m);
                    }
                }
            }
        }
        flow.sort_unstable();
        let veg = state.ecology.vegetation.as_slice();
        let occ = water.structures.as_slice();
        DamPreview {
            level,
            pond_area: flooded.len(),
            volume: flooded.iter().map(|&i| level - bed[i]).sum(),
            flooded_trees: flooded.iter().filter(|&&i| veg[i] > TREE_VEGETATION).count(),
            flooded_buildings: flooded.iter().filter(|&&i| occ[i]).count(),
            reaches_map_edge: flooded.iter().any(|&i| i % n == 0 || i % n == n - 1 || i / n == 0 || i / n == n - 1),
            flooded_cells: flooded.iter().map(|&i| (i % n, i / n)).collect(),
            crest: self.crest,
            spill_at: self.spill_at,
            flow_level,
            flow_cells: flow.iter().map(|&i| (i % n, i / n)).collect(),
        }
    }
}

#[cfg(test)]
mod engine_tests {
    use super::*;

    /// The engine setting is process-wide; tests building worlds take turns.
    static SETTING: std::sync::Mutex<()> = std::sync::Mutex::new(());

    fn world_on(kind: EngineKind) -> SimState {
        let _turn = SETTING.lock().unwrap_or_else(|e| e.into_inner());
        set_default_engine(kind);
        let s = SimState::sandbox(1, crate::worldgen::fixture(1));
        set_default_engine(DEFAULT_ENGINE);
        s
    }

    fn balance(w: &Water, start: f64) -> f64 {
        (w.total_volume() - (start + w.added_volume - w.lost_volume)).abs() / w.added_volume.max(1.0)
    }

    #[test]
    fn every_engine_keeps_its_books_and_publishes_finite_water() {
        for kind in EngineKind::ALL {
            let mut s = world_on(kind);
            assert_eq!(s.water.engine(), kind);
            let start = s.water.total_volume() - (s.water.added_volume - s.water.lost_volume);
            crate::sandbox::run_days(&mut s, 2.0);
            assert!(balance(&s.water, start) < 1e-6, "{} loses track of water", kind.name());
            assert!(s.water.depth.as_slice().iter().all(|d| d.is_finite() && *d >= 0.0), "{} publishes bad depth", kind.name());
            assert!(s.water.velocity.as_slice().iter().all(|v| v.0.is_finite() && v.1.is_finite()), "{} publishes bad velocity", kind.name());
        }
    }

    #[test]
    fn switching_engine_mid_game_keeps_the_water() {
        for from in EngineKind::ALL {
            for to in EngineKind::ALL.into_iter().filter(|&k| k != from) {
                let mut s = world_on(from);
                crate::sandbox::run_days(&mut s, 1.0);
                let before = s.water.depth.as_slice().iter().map(|&d| d as f64).sum::<f64>();
                let start = s.water.total_volume() - (s.water.added_volume - s.water.lost_volume);
                s.water.set_engine(to);
                crate::sandbox::run_days(&mut s, 1.0 / 96.0);
                assert_eq!(s.water.engine(), to);
                let b = balance(&s.water, start);
                assert!(b < 1e-6, "{} -> {} loses track of water ({b:.2e})", from.name(), to.name());
                let after = s.water.depth.as_slice().iter().map(|&d| d as f64).sum::<f64>();
                assert!(after > 0.0 && before > 0.0, "{} -> {} dried the map", from.name(), to.name());
            }
        }
    }
}
