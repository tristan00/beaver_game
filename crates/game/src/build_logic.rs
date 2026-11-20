use sim::buildings::{Amount, BuildingId, BuildingKind, Site, WET_DEPTH};

use sim::water::Cell;
use sim::{Command, SimState};

pub const DEFAULT_DAM_HEIGHT: f32 = 1.0;
pub const MIN_DAM_HEIGHT: f32 = 0.5;
pub const MAX_DAM_HEIGHT: f32 = sim::dam_preview::MAX_DAM_HEIGHT;
pub const DAM_STEP: f32 = 0.5;
pub const MAX_DAM_CELLS: usize = 48;
pub const MAX_CANAL_CELLS: usize = 80;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Group {
    Base,
    Water, Forest, Building, Craft, Transport, Defence,
}

impl Group {
    pub fn name(self) -> &'static str {
        match self {
            Group::Base => "Basics",
            Group::Water => "Water",
            Group::Forest => "Forest",
            Group::Building => "Building",
            Group::Craft => "Craft",
            Group::Transport => "Transport",
            Group::Defence => "Defence",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct MenuEntry {
    pub kind: BuildingKind,
    pub group: Group,
    pub unlocked: bool,
    pub need: Option<&'static str>,
}

pub fn menu(sim: &SimState) -> Vec<MenuEntry> {
    use BuildingKind::*;
    let mut out: Vec<_> = BuildingKind::ALL.into_iter().map(|kind| {
        let group = match kind {
            Dam | Canal | Lock | WaterWheel => Group::Water,
            LoggingCamp | ForagerHut | MudPit | ForesterLodge | TreeNursery => Group::Forest,
            Lodge => Group::Building,
            Workshop | Mill => Group::Craft,
            Dock | Barge | Storehouse | FoodCache => Group::Transport,
            GuardPost | ScentMound => Group::Defence,
            _ => Group::Base,
        };
        let need = sim.missing_prerequisites(kind).first().map(|k| k.spec().name);
        MenuEntry {kind, group, unlocked: sim.can_build(kind), need}
    }).collect();
    out.sort_by_key(|e| e.group as usize);
    out
}

#[derive(Clone, Debug, PartialEq)]
pub enum PlaceError {
    Locked(&'static str),
    OutOfBounds,
    Blocked,
    NeedsDry,
    NeedsWater,
    NeedsPond,
    NeedsDock,
    NeedsDam,
    NoCells,
    TooLong(usize),
    Sim(String),
}

impl std::fmt::Display for PlaceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PlaceError::Locked(t) => write!(f, "Needs a finished {t}"),
            PlaceError::OutOfBounds => write!(f, "Outside the map"),
            PlaceError::Blocked => write!(f, "Something is already built here"),
            PlaceError::NeedsDry => write!(f, "Needs dry ground"),
            PlaceError::NeedsWater => write!(f, "Must touch water"),
            PlaceError::NeedsPond => write!(f, "Most of the footprint needs a pond at least 0.10 m deep"),
            PlaceError::NeedsDock => write!(f, "Needs a free water cell next to a finished dock"),
            PlaceError::NeedsDam => write!(f, "A lock is built on a dam"),
            PlaceError::NoCells => write!(f, "Nothing drawn yet"),
            PlaceError::TooLong(max) => write!(f, "Too long (max {max} cells)"),
            PlaceError::Sim(m) => write!(f, "{m}"),
        }
    }
}

pub fn is_wet(sim: &SimState, x: usize, y: usize) -> bool {
    sim.water.depth.get(x, y).is_some_and(|&d| d >= WET_DEPTH)
        || sim.world.river.get(x, y).copied().unwrap_or(false)
        || sim.world.lake.get(x, y).copied().unwrap_or(false)
}

pub fn anchor(kind: BuildingKind, c: Cell) -> Cell {
    let (w, h) = kind.spec().footprint;
    (c.0.saturating_sub(w / 2), c.1.saturating_sub(h / 2))
}

pub fn footprint(kind: BuildingKind, pos: Cell) -> Vec<Cell> {
    sim::buildings::Buildings::footprint(kind, pos)
}

fn locked(sim: &SimState, kind: BuildingKind) -> Result<(), PlaceError> {
    match sim.missing_prerequisites(kind).first().filter(|_| !sim.sandbox) {
        Some(need) => Err(PlaceError::Locked(need.spec().name)),
        None => Ok(()),
    }
}

pub fn check_building(sim: &SimState, kind: BuildingKind, pos: Cell) -> Result<(), PlaceError> {
    use sim::buildings::BuildError;
    sim.check_site(kind, pos).map_err(|e| match e {
        BuildError::Locked(need) => PlaceError::Locked(need.spec().name),
        BuildError::OutOfBounds => PlaceError::OutOfBounds,
        BuildError::Blocked => PlaceError::Blocked,
        BuildError::NoDock => PlaceError::NeedsDock,
        BuildError::NeedsDam => PlaceError::NeedsDam,
        BuildError::BadTerrain(_) => match kind.spec().site {
            Site::Dry => PlaceError::NeedsDry,
            Site::Pond => PlaceError::NeedsPond,
            _ => PlaceError::NeedsWater,
        },
        _ => PlaceError::Sim(e.to_string()),
    })
}

#[derive(Clone, Debug)]
pub struct SitePreview {
    pub pos: Cell,
    pub cells: Vec<Cell>,
    pub verdict: Result<(), PlaceError>,
}

impl SitePreview {
    pub fn is_valid(&self) -> bool {
        self.verdict.is_ok()
    }
}

pub fn site_preview(sim: &SimState, kind: BuildingKind, hover: Cell) -> SitePreview {
    let pos = anchor(kind, hover);
    let cells = if kind == BuildingKind::Dam { sim::dam_preview::spot_cells(pos) } else { footprint(kind, pos) };
    SitePreview { pos, cells, verdict: check_building(sim, kind, pos) }
}

#[derive(Clone, Debug, PartialEq)]
pub struct DamReadout {
    pub rise: f32,
    pub level: f32,
    pub flooded_cells: usize,
    pub flooded_trees: usize,
    pub flooded_buildings: usize,
    pub cost: Vec<Amount>,
    pub pressure: f32,
    pub fill_days: f32,
}

impl DamReadout {
    pub fn from_preview(d: &sim::dam_preview::DamPreview) -> DamReadout {
        DamReadout {
            rise: d.rise,
            level: d.level,
            flooded_cells: d.flooded.len(),
            flooded_trees: d.flooded_trees,
            flooded_buildings: d.flooded_buildings.len(),
            cost: d.cost.clone(),
            pressure: d.pressure,
            fill_days: d.fill_days,
        }
    }

}

pub fn unit_cells(kind: BuildingKind) -> f32 {
    if kind == BuildingKind::Dam { sim::dam_preview::DAM_SPAN_CELLS } else { (kind.spec().footprint.0 * kind.spec().footprint.1) as f32 }
}

pub fn path_cost(kind: BuildingKind, n: usize) -> Vec<Amount> {
    kind.spec().cost.iter().map(|&(g, a)| (g, a * n as f32 / unit_cells(kind))).collect()
}

pub fn line_cells(a: Cell, b: Cell) -> Vec<Cell> {
    let (mut x, mut y) = (a.0 as i64, a.1 as i64);
    let (x1, y1) = (b.0 as i64, b.1 as i64);
    let (dx, dy) = ((x1 - x).abs(), -(y1 - y).abs());
    let (sx, sy) = (if x < x1 { 1 } else { -1 }, if y < y1 { 1 } else { -1 });
    let mut err = dx + dy;
    let mut out = Vec::new();
    loop {
        out.push((x as usize, y as usize));
        if x == x1 && y == y1 {
            break;
        }
        let e2 = 2 * err;
        if e2 >= dy {
            err += dy;
            x += sx;
        }
        if e2 <= dx {
            err += dx;
            y += sy;
        }
    }
    out
}

pub fn check_dam(sim: &SimState, cells: &[Cell], height: f32) -> Result<(), PlaceError> {
    locked(sim, BuildingKind::Dam)?;
    if cells.is_empty() {
        return Err(PlaceError::NoCells);
    }
    if cells.len() > MAX_DAM_CELLS {
        return Err(PlaceError::TooLong(MAX_DAM_CELLS));
    }
    if cells.iter().any(|&(x, y)| !sim.water.depth.in_bounds(x, y)) {
        return Err(PlaceError::OutOfBounds);
    }
    if !sim.sandbox && !cells.iter().any(|&(x, y)| is_wet(sim, x, y) || sim.water.has_dam(x, y)) {
        return Err(PlaceError::NeedsWater);
    }
    if !(MIN_DAM_HEIGHT..=MAX_DAM_HEIGHT + 1e-3).contains(&height) {
        return Err(PlaceError::Sim(format!("Dam height must be {MIN_DAM_HEIGHT}..{MAX_DAM_HEIGHT}")));
    }
    Ok(())
}

pub fn check_canal(sim: &SimState, cells: &[Cell]) -> Result<(), PlaceError> {
    locked(sim, BuildingKind::Canal)?;
    if cells.is_empty() {
        return Err(PlaceError::NoCells);
    }
    if cells.len() > MAX_CANAL_CELLS {
        return Err(PlaceError::TooLong(MAX_CANAL_CELLS));
    }
    if cells.iter().any(|&(x, y)| !sim.water.depth.in_bounds(x, y)) {
        return Err(PlaceError::OutOfBounds);
    }
    if !sim.sandbox && !cells.iter().any(|&(x, y)| is_wet(sim, x, y)) {
        return Err(PlaceError::NeedsWater);
    }
    Ok(())
}

pub fn line_heights(sim: &SimState, cells: &[Cell], height: f32, crest: Option<f32>) -> (Vec<Cell>, Vec<f32>) {
    let up = |c: &Cell| crest.map_or(0.0, |k| k - sim.height.get(c.0, c.1).copied().unwrap_or(k));
    (cells.to_vec(), cells.iter().map(|c| height.max(up(c))).collect())
}

pub fn split_to_crest(sim: &SimState, cells: &[Cell], height: f32, crest: f32) -> (Vec<Cell>, Vec<Cell>) {
    cells.iter().partition(|c| crest - sim.height.get(c.0, c.1).copied().unwrap_or(crest) <= height + 1e-3)
}

pub fn line_preview(sim: &SimState, cells: &[Cell], height: f32, crest: Option<f32>) -> Option<sim::dam_preview::DamPreview> {
    let (c, h) = line_heights(sim, cells, height, crest);
    sim::dam_preview::preview_heights(sim, &c, &h)
}

pub fn line_cost(sim: &SimState, cells: &[Cell], height: f32, crest: Option<f32>) -> Vec<Amount> {
    let (c, h) = line_heights(sim, cells, height, crest);
    sim::dam_preview::cost_for(sim, &c, &h).0
}

pub fn extend_line(sim: &SimState, cells: &[Cell], height: f32, crest: Option<f32>) -> Option<(Vec<Cell>, f32)> {
    let (c, h) = line_heights(sim, cells, height, crest);
    sim::dam_preview::preview_heights(sim, &c, &h)?.spill_at?;
    let (longer, _, p) = sim::dam_preview::extend_to_hold(sim, &c, &h, MAX_DAM_CELLS)?;
    let mut out = cells.to_vec();
    out.extend(longer.into_iter().filter(|x| !cells.contains(x)));
    Some((out, p.crest))
}

pub fn dam_line_at(sim: &SimState, seed: Cell) -> Vec<Cell> {
    sim.water.dam_line_at(seed)
}

pub fn dam_cell_of(sim: &SimState, id: BuildingId) -> Option<Cell> {
    let b = sim.buildings.building(id).filter(|b| b.kind == BuildingKind::Dam && b.is_complete())?;
    b.cells().find(|&(x, y)| sim.water.has_dam(x, y))
}

pub fn dam_line_height(sim: &SimState, cells: &[Cell]) -> f32 {
    cells.iter().filter_map(|&(x, y)| sim.water.dam_height.get(x, y).copied()).fold(0.0, f32::max)
}

pub fn raise_order(sim: &SimState, seed: Cell) -> Option<BuildOrder> {
    let cells = dam_line_at(sim, seed);
    if cells.is_empty() {
        return None;
    }
    let now = dam_line_height(sim, &cells);
    let height = ((now / DAM_STEP).round() * DAM_STEP + DAM_STEP).min(MAX_DAM_HEIGHT);
    (height > now + 1e-3).then_some(BuildOrder::Dam { cells, height })
}

#[derive(Clone, Debug, PartialEq)]
pub enum BuildOrder {
    Building { kind: BuildingKind, pos: Cell },
    Dam { cells: Vec<Cell>, height: f32 },
    DamTo { cells: Vec<Cell>, height: f32, crest: f32 },
    Canal { cells: Vec<Cell> },
}

pub fn emit(sim: &mut SimState, order: &BuildOrder) -> Result<String, PlaceError> {
    match order {
        BuildOrder::Building { kind, pos } => {
            check_building(sim, *kind, *pos)?;
            sim.apply(Command::PlaceSite { kind: *kind, pos: *pos }).map_err(|e| PlaceError::Sim(e.to_string()))?;
            Ok(format!("{} ordered at ({}, {})", kind.spec().name, pos.0, pos.1))
        }
        BuildOrder::Dam { cells, height } => {
            check_dam(sim, cells, *height)?;
            sim.apply(Command::OrderDam { cells: cells.clone(), height: *height }).map_err(|e| PlaceError::Sim(e.to_string()))?;
            Ok(format!("Dam of {} cells ordered", cells.len()))
        }
        BuildOrder::DamTo { cells, height, crest } => {
            check_dam(sim, cells, *height)?;
            let (base, added) = split_to_crest(sim, cells, *height, *crest);
            if !base.is_empty() {
                sim.apply(Command::OrderDam { cells: base, height: *height }).map_err(|e| PlaceError::Sim(e.to_string()))?;
            }
            if !added.is_empty() {
                sim.apply(Command::OrderDamTo { cells: added, crest: *crest }).map_err(|e| PlaceError::Sim(e.to_string()))?;
            }
            Ok(format!("Dam of {} cells ordered", cells.len()))
        }
        BuildOrder::Canal { cells } => {
            check_canal(sim, cells)?;
            sim.apply(Command::OrderCanal { cells: cells.clone() }).map_err(|e| PlaceError::Sim(e.to_string()))?;
            Ok(format!("Canal of {} cells ordered", cells.len()))
        }
    }
}
