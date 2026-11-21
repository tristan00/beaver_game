use sim::buildings::{Amount, BuildingKind, Good, Site, WET_DEPTH};

use sim::water::Cell;
use sim::{Command, SimState};

pub const MAX_DAM_CELLS: usize = sim::dams::MAX_CELLS;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Group {
    Water,
    Home,
}

impl Group {
    pub fn name(self) -> &'static str {
        match self {
            Group::Water => "Water",
            Group::Home => "Lodging",
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
    let mut out: Vec<_> = BuildingKind::ALL
        .into_iter()
        .map(|kind| {
            let group = if kind == BuildingKind::Dam { Group::Water } else { Group::Home };
            let need = sim.missing_prerequisites(kind).first().map(|k| k.spec().name);
            MenuEntry { kind, group, unlocked: sim.can_build(kind), need }
        })
        .collect();
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
            PlaceError::NeedsWater => write!(f, "Must cross water"),
            PlaceError::NeedsPond => write!(f, "Most of the footprint needs a pond at least 0.10 m deep"),
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

pub fn check_building(sim: &SimState, kind: BuildingKind, pos: Cell) -> Result<(), PlaceError> {
    use sim::buildings::BuildError;
    sim.check_site(kind, pos).map_err(|e| match e {
        BuildError::Locked(need) => PlaceError::Locked(need.spec().name),
        BuildError::OutOfBounds => PlaceError::OutOfBounds,
        BuildError::Blocked => PlaceError::Blocked,
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
    SitePreview { pos, cells: footprint(kind, pos), verdict: check_building(sim, kind, pos) }
}

/// What a drawn dam line would do, for the placement ghost.
#[derive(Clone, Debug, PartialEq)]
pub struct DamReadout {
    pub flooded_cells: usize,
    pub flooded_trees: usize,
    pub flooded_buildings: usize,
    pub sticks: f32,
}

impl DamReadout {
    pub fn from_preview(d: &sim::dam_preview::DamPreview) -> DamReadout {
        DamReadout {
            flooded_cells: d.flooded.len(),
            flooded_trees: d.flooded_trees,
            flooded_buildings: d.flooded_buildings.len(),
            sticks: d.cost.iter().filter(|c| c.0 == Good::Sticks).map(|c| c.1).sum(),
        }
    }
}

pub fn path_cost(kind: BuildingKind, n: usize) -> Vec<Amount> {
    kind.spec().cost.iter().map(|&(g, a)| (g, a * n as f32)).collect()
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

/// A polyline through its corners, each cell once.
pub fn polyline(corners: &[Cell]) -> Vec<Cell> {
    let mut out: Vec<Cell> = Vec::new();
    for (i, w) in corners.windows(2).enumerate() {
        for c in line_cells(w[0], w[1]).into_iter().skip(usize::from(i > 0)) {
            if !out.contains(&c) {
                out.push(c);
            }
        }
    }
    if corners.len() == 1 {
        out.push(corners[0]);
    }
    out
}

pub fn check_dam(sim: &SimState, cells: &[Cell]) -> Result<(), PlaceError> {
    if cells.is_empty() {
        return Err(PlaceError::NoCells);
    }
    if cells.len() > MAX_DAM_CELLS {
        return Err(PlaceError::TooLong(MAX_DAM_CELLS));
    }
    if cells.iter().any(|&(x, y)| !sim.water.depth.in_bounds(x, y)) {
        return Err(PlaceError::OutOfBounds);
    }
    if !sim.sandbox && !cells.iter().any(|&(x, y)| is_wet(sim, x, y)) {
        return Err(PlaceError::NeedsWater);
    }
    sim::dams::check_line(sim, cells).map_err(PlaceError::Sim)
}

pub fn line_preview(sim: &SimState, cells: &[Cell]) -> Option<sim::dam_preview::DamPreview> {
    let h = sim::dams::first_targets(sim, cells);
    sim::dam_preview::preview_heights(sim, cells, &h)
}

pub fn line_cost(sim: &SimState, cells: &[Cell]) -> f32 {
    let h = sim::dams::first_targets(sim, cells);
    sim::dam_preview::loads_for(sim, cells, &h) as f32 * sim::dams::sticks_per_step()
}

#[derive(Clone, Debug, PartialEq)]
pub enum BuildOrder {
    Building { kind: BuildingKind, pos: Cell },
    Dam { cells: Vec<Cell> },
}

pub fn emit(sim: &mut SimState, order: &BuildOrder) -> Result<String, PlaceError> {
    match order {
        BuildOrder::Building { kind, pos } => {
            check_building(sim, *kind, *pos)?;
            sim.apply(Command::PlaceSite { kind: *kind, pos: *pos }).map_err(|e| PlaceError::Sim(e.to_string()))?;
            Ok(format!("{} ordered at ({}, {})", kind.spec().name, pos.0, pos.1))
        }
        BuildOrder::Dam { cells } => {
            check_dam(sim, cells)?;
            sim.apply(Command::PlaceDam { cells: cells.clone() }).map_err(|e| PlaceError::Sim(e.to_string()))?;
            Ok(format!("Dam of {} cells drawn", cells.len()))
        }
    }
}
