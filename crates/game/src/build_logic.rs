use sim::buildings::{Amount, BuildingId, BuildingKind, Site, WET_DEPTH};
use sim::colony::OrderId;
use sim::tech::{Branch, Effect, TECHS};
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
    Branch(Branch),
}

impl Group {
    pub fn name(self) -> &'static str {
        match self {
            Group::Base => "Basics",
            Group::Branch(Branch::Water) => "Water",
            Group::Branch(Branch::Forest) => "Forest",
            Group::Branch(Branch::Building) => "Building",
            Group::Branch(Branch::Craft) => "Craft",
            Group::Branch(Branch::Transport) => "Transport",
            Group::Branch(Branch::Defence) => "Defence",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct MenuEntry {
    pub kind: BuildingKind,
    pub group: Group,
    pub unlocked: bool,
    pub need: Option<(&'static str, f32, f32)>,
}

pub fn menu(sim: &SimState) -> Vec<MenuEntry> {
    let mut out = Vec::new();
    for kind in BuildingKind::ALL.into_iter().filter(|k| sim.features.allows_building(*k)) {
        let tech = TECHS.iter().find(|t| t.effects.contains(&Effect::Unlock(kind)));
        let group = if kind == BuildingKind::Dam { Group::Branch(Branch::Water) } else { tech.map_or(Group::Base, |t| Group::Branch(t.branch)) };
        let unlocked = sim.tech.allows(kind) && sim.missing_prerequisites(kind).is_empty();
        let need = match (unlocked, tech) {
            (false, Some(t)) => Some((t.name, t.xp, t.branch.experience(&sim.colony))),
            _ => None,
        };
        out.push(MenuEntry { kind, group, unlocked, need });
    }
    let rank = |g: Group| match g {
        Group::Base => 0,
        Group::Branch(b) => 1 + Branch::ALL.iter().position(|x| *x == b).unwrap_or(0),
    };
    out.sort_by_key(|e| rank(e.group));
    out
}

fn fmt_amount(a: f32) -> String {
    if (a - a.round()).abs() < 0.05 { format!("{}", a.round() as i32) } else { format!("{a:.1}") }
}

#[derive(Clone, Debug, PartialEq)]
pub enum PlaceError {
    Locked(&'static str, f32, f32),
    OutOfBounds,
    Blocked,
    NeedsDry,
    NeedsWater,
    NeedsDock,
    NeedsDam,
    NoCells,
    TooLong(usize),
    Sim(String),
}

impl std::fmt::Display for PlaceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PlaceError::Locked(t, need, have) => write!(f, "Locked: {t} needs {} xp (have {})", fmt_amount(*need), fmt_amount(*have)),
            PlaceError::OutOfBounds => write!(f, "Outside the map"),
            PlaceError::Blocked => write!(f, "Something is already built here"),
            PlaceError::NeedsDry => write!(f, "Needs dry ground"),
            PlaceError::NeedsWater => write!(f, "Must touch water"),
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
    let (w, h) = kind.spec().footprint;
    (0..h).flat_map(|dy| (0..w).map(move |dx| (pos.0 + dx, pos.1 + dy))).collect()
}

fn locked(sim: &SimState, kind: BuildingKind) -> Result<(), PlaceError> {
    if sim.sandbox || sim.tech.allows(kind) {
        return Ok(());
    }
    match menu(sim).into_iter().find(|e| e.kind == kind).and_then(|e| e.need) {
        Some((t, need, have)) => Err(PlaceError::Locked(t, need, have)),
        None => Err(PlaceError::Locked("tech", 0.0, 0.0)),
    }
}

pub fn check_building(sim: &SimState, kind: BuildingKind, pos: Cell) -> Result<(), PlaceError> {
    locked(sim, kind)?;
    sim.check_site(kind, pos).map_err(|e| {
        let m = e.to_string();
        if m.contains("OutOfBounds") {
            PlaceError::OutOfBounds
        } else if m.contains("Blocked") {
            PlaceError::Blocked
        } else if m.contains("NoDock") {
            PlaceError::NeedsDock
        } else if m.contains("BadTerrain") {
            match kind.spec().site {
                Site::Dry => PlaceError::NeedsDry,
                _ => PlaceError::NeedsWater,
            }
        } else {
            PlaceError::Sim(m)
        }
    })?;
    if kind == BuildingKind::Lock && !footprint(kind, pos).iter().any(|&(x, y)| sim.water.has_dam(x, y)) {
        return Err(PlaceError::NeedsDam);
    }
    Ok(())
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
    if !sim.water.has_dam(seed.0, seed.1) {
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
            if sim.water.has_dam(c.0, c.1) && seen.insert(c) {
                out.push(c);
            }
        }
        i += 1;
    }
    out.sort();
    out
}

#[cfg(test)]
mod dam_line_tests {
    use super::*;

    #[test]
    fn a_scripted_long_dam_is_one_component_without_merging_a_second_dam() {
        let mut s = SimState::new(1);
        let long: Vec<_> = (5..105).map(|x| (x, 10)).collect();
        s.water.build_dam(&long, 1.0).unwrap();
        s.water.build_dam(&[(50, 20), (51, 20)], 1.0).unwrap();
        assert_eq!(dam_line_at(&s, (50, 10)), long);
        assert_eq!(dam_line_at(&s, (50, 20)).len(), 2);
    }
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

#[derive(Clone, Debug, PartialEq)]
pub struct PendingWork {
    pub order: OrderId,
    pub kind: BuildingKind,
    pub cells: Vec<Cell>,
    pub command: Command,
    pub total_work: f32,
    pub cost: Vec<Amount>,
}

#[derive(Clone, Debug, Default)]
pub struct Pending(pub Vec<PendingWork>);

pub fn emit(sim: &mut SimState, pending: &mut Pending, order: &BuildOrder) -> Result<String, PlaceError> {
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
            let _ = pending;
            sim.apply(Command::OrderCanal { cells: cells.clone() }).map_err(|e| PlaceError::Sim(e.to_string()))?;
            Ok(format!("Canal of {} cells ordered", cells.len()))
        }
    }
}

pub fn poll_pending(sim: &mut SimState, pending: &mut Pending) -> Vec<String> {
    let mut msgs = Vec::new();
    let mut i = 0;
    while i < pending.0.len() {
        if sim.colony.orders.iter().any(|o| o.id == pending.0[i].order) {
            i += 1;
            continue;
        }
        let w = pending.0.remove(i);
        match sim.apply(w.command.clone()) {
            Ok(()) => msgs.push(format!("{} finished ({} cells)", w.kind.spec().name, w.cells.len())),
            Err(e) => {
                refund(sim, &w.cost);
                msgs.push(format!("{} failed: {e}", w.kind.spec().name));
            }
        }
    }
    msgs
}

fn refund(sim: &mut SimState, cost: &[Amount]) {
    for &(g, a) in cost {
        let _ = sim.buildings.stores.deposit(g, a);
    }
}

