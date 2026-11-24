use sim::buildings::{Amount, BuildingKind, WET_DEPTH};

use sim::water::Cell;
use sim::{Command, SimState};


#[derive(Clone, Debug, PartialEq)]
pub enum PlaceError {
    OutOfBounds,
    Blocked,
    NeedsWater,
    TooClose,
    BlocksDoor,
    NoCells,
    Sim(String),
}

impl std::fmt::Display for PlaceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PlaceError::OutOfBounds => write!(f, "Outside the map"),
            PlaceError::Blocked => write!(f, "Something is already built here"),
            PlaceError::NeedsWater => write!(f, "Must touch water"),
            PlaceError::TooClose => write!(f, "Leave a cell between buildings"),
            PlaceError::BlocksDoor => write!(f, "Would block a home's door"),
            PlaceError::NoCells => write!(f, "Nothing drawn yet"),
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
        BuildError::OutOfBounds => PlaceError::OutOfBounds,
        BuildError::Blocked => PlaceError::Blocked,
        BuildError::BadTerrain("needs water") => PlaceError::NeedsWater,
        BuildError::BadTerrain("would block access to a home") => PlaceError::BlocksDoor,
        BuildError::BadTerrain(_) => PlaceError::TooClose,
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
    if cells.iter().any(|&(x, y)| !sim.water.depth.in_bounds(x, y)) {
        return Err(PlaceError::OutOfBounds);
    }
    if !sim.sandbox && !cells.iter().any(|&c| sim::dams::water_for_dam(sim, c)) {
        return Err(PlaceError::NeedsWater);
    }
    sim::dams::check_line(sim, cells).map_err(PlaceError::Sim)
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

/// A drawn dam line worked out: its targets, the pond it would make, its cost,
/// and the reason it cannot be built, if any.
#[derive(Clone, Debug)]
pub struct LinePlan {
    pub line: Vec<Cell>,
    pub targets: Vec<(Cell, f32)>,
    pub preview: Option<sim::dam_preview::DamPreview>,
    pub sticks: f32,
    pub new_wet: usize,
    pub refusal: Option<(crate::icons::Icon, String)>,
    /// The cells and heights the water engine is asked to settle (see `sim::dam_preview::settle`).
    pub settle_cells: Vec<Cell>,
    pub settle_heights: Vec<f32>,
}

fn sentence(m: &str) -> String {
    let mut c = m.chars();
    c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default()
}

/// Work out a drawn line: a new dam, or more cells for `extending`.
pub fn line_plan(sim: &SimState, line: &[Cell], extending: Option<sim::buildings::BuildingId>) -> LinePlan {
    use crate::icons::Icon;
    let mut out = LinePlan { line: line.to_vec(), targets: Vec::new(), preview: None, sticks: 0.0, new_wet: 0, refusal: None, settle_cells: Vec::new(), settle_heights: Vec::new() };
    match extending {
        Some(id) => match (sim::dams::dam(sim, id), sim::dams::plan_extend_to(sim, id, line)) {
            (Some(d), Ok(plan)) => {
                let row: Vec<&sim::dams::DamCell> = sim::dams::in_order(plan.cells.iter().filter(|c| c.row == 0)).collect();
                let cells: Vec<Cell> = row.iter().map(|c| c.cell).collect();
                let heights: Vec<f32> = row.iter().map(|c| c.target.max(sim::dams::built(sim, c.cell))).collect();
                out.preview = sim::dam_preview::preview_heights(sim, &cells, &heights);
                (out.settle_cells, out.settle_heights) = (cells, heights);
                out.sticks = sim::dams::plan_cost(sim, d, &plan);
                let added = plan.added(d);
                out.targets = plan.cells.iter().filter(|c| added.contains(&c.cell)).map(|c| (c.cell, c.target)).collect();
            }
            (_, Err(e)) => out.refusal = Some((Icon::Cross, sentence(&e))),
            (None, _) => out.refusal = Some((Icon::Cross, "Gone".into())),
        },
        None => {
            let h = sim::dams::first_targets(sim, line);
            out.targets = line.iter().copied().zip(h.iter().copied()).collect();
            out.preview = sim::dam_preview::preview_heights(sim, line, &h);
            (out.settle_cells, out.settle_heights) = (line.to_vec(), h);
            out.sticks = line_cost(sim, line);
            out.refusal = check_dam(sim, line).err().and_then(|e| match e {
                PlaceError::NoCells => None,
                PlaceError::NeedsWater => Some((Icon::Drop, "Must cross water".into())),
                PlaceError::Sim(m) => Some((Icon::Cross, sentence(&m))),
                e => Some((Icon::Cross, e.to_string())),
            });
        }
    }
    out.new_wet = out.preview.as_ref().map_or(0, |p| p.flow_flooded.iter().filter(|&&(x, y)| !is_wet(sim, x, y)).count());
    out
}

