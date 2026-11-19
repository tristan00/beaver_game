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

#[cfg(test)]
pub(crate) fn dry_sites(sim: &SimState) -> Vec<Cell> {
    let n = sim.world.size;
    let c = sim.world.valleys[sim.world.start.0 as usize].center;
    let mut cells: Vec<Cell> = (8..n - 8).flat_map(|y| (8..n - 8).map(move |x| (x, y)))
        .filter(|&(x, y)| sim.world.valley_of((x, y)) == Some(sim.world.start))
        .filter(|&(x, y)| (y - 5..=y + 5).all(|j| (x - 5..=x + 5).all(|i| !is_wet(sim, i, j)))).collect();
    cells.sort_by(|a, b| {
        let d = |p: &Cell| (p.0 as f32 - c.0).powi(2) + (p.1 as f32 - c.1).powi(2);
        d(a).total_cmp(&d(b))
    });
    let mut out: Vec<Cell> = Vec::new();
    for p in cells {
        if out.iter().all(|q| q.0.abs_diff(p.0).max(q.1.abs_diff(p.1)) >= 12) { out.push(p); }
    }
    out
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
    if sim.tech.allows(kind) {
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
    if !cells.iter().any(|&(x, y)| is_wet(sim, x, y) || sim.water.has_dam(x, y)) {
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
    if !cells.iter().any(|&(x, y)| is_wet(sim, x, y)) {
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
    let mut i = 0;
    while i < out.len() && out.len() < MAX_DAM_CELLS {
        let (x, y) = out[i];
        for (dx, dy) in [(-1i64, -1i64), (0, -1), (1, -1), (-1, 0), (1, 0), (-1, 1), (0, 1), (1, 1)] {
            let (nx, ny) = (x as i64 + dx, y as i64 + dy);
            if nx < 0 || ny < 0 || out.len() >= MAX_DAM_CELLS {
                continue;
            }
            let c = (nx as usize, ny as usize);
            if sim.water.has_dam(c.0, c.1) && !out.contains(&c) {
                out.push(c);
            }
        }
        i += 1;
    }
    out.sort();
    out
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












#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use sim::buildings::Good;
    use sim::SimState;

    pub(crate) fn found(sim: &mut SimState) {
        let c = sim.world.valleys[sim.world.start.0 as usize].center;
        struct Dry;
        impl sim::colony::Landscape for Dry {
            fn is_water(&self, _: f32, _: f32) -> bool { false }
        }
        sim.colony.found((c.0, c.1), &Dry).expect("found");
    }

    fn river_cell(sim: &SimState) -> Cell {
        (0..sim.world.size * sim.world.size).map(|i| (i % sim.world.size, i / sim.world.size)).find(|&(x, y)| sim.world.river.get(x, y).copied().unwrap_or(false) && x > 3 && y > 3 && x + 4 < sim.world.size && y + 4 < sim.world.size).unwrap()
    }

    fn stock(sim: &mut SimState) {
        for g in Good::ALL {
            sim.buildings.stores.deposit(g, 100.0).unwrap();
        }
    }

    #[test]
    fn menu_groups_by_branch_and_marks_locked() {
        let sim = SimState::new(1);
        let m = menu(&sim);
        assert_eq!(m.len(), BuildingKind::ALL.len() - 2);
        assert!(m.iter().all(|e| e.kind != BuildingKind::GuardPost && e.kind != BuildingKind::ScentMound));
        assert_eq!(m[0].group, Group::Base);
        let lock = m.iter().find(|e| e.kind == BuildingKind::Lock).unwrap();
        assert_eq!(lock.group, Group::Branch(Branch::Water));
        assert!(!lock.unlocked);
        assert_eq!(lock.need.map(|n| (n.0, n.1)), Some(("Flow control", 60.0)));
        let dam = m.iter().find(|e| e.kind == BuildingKind::Dam).unwrap();
        assert!(dam.unlocked && dam.need.is_none());
        assert_eq!(dam.group, Group::Branch(Branch::Water));
        let groups: Vec<_> = m.iter().map(|e| e.group).collect();
        let mut seen = vec![];
        for g in groups {
            if seen.last() != Some(&g) {
                assert!(!seen.contains(&g), "group {g:?} split");
                seen.push(g);
            }
        }
    }

    #[test]
    fn building_validation_reports_each_reason() {
        let mut sim = SimState::new(1);
        found(&mut sim);
        let site = dry_sites(&sim)[0];
        assert!(matches!(check_building(&sim, BuildingKind::Workshop, site), Err(PlaceError::Locked(..))));
        sim.tech.unlock_all();
        assert_eq!(check_building(&sim, BuildingKind::QuarryCamp, site), Ok(()));
        assert_eq!(check_building(&sim, BuildingKind::Lodge, site), Err(PlaceError::NeedsWater));
        assert_eq!(check_building(&sim, BuildingKind::Lodge, (sim.world.size - 1, 5)), Err(PlaceError::OutOfBounds));
        sim.water.depth.as_mut_slice()[site.1 * sim.world.size + site.0] = 1.0;
        assert_eq!(check_building(&sim, BuildingKind::Lodge, (site.0 - 1, site.1 - 1)), Ok(()), "one cell in the water is enough");
        assert_eq!(check_building(&sim, BuildingKind::QuarryCamp, site), Err(PlaceError::NeedsDry));
    }







    #[test]
    fn dam_order_places_a_site_and_builds_the_water_dam_only_when_it_completes() {
        let mut sim = SimState::new(1);
        sim.tech.unlock_all();
        found(&mut sim);
        let c = river_cell(&sim);
        let cells = line_cells((c.0 - 2, c.1), (c.0 + 2, c.1));
        let mut p = Pending::default();
        let ledger = sim.buildings.stores.amount(Good::Logs);
        emit(&mut sim, &mut p, &BuildOrder::Dam { cells: cells.clone(), height: 1.5 }).unwrap();
        assert!(p.0.is_empty(), "a dam is a site, not a queued colony order");
        assert_eq!(sim.work.dam_plans.len(), 1);
        let site = sim.work.dam_plans[0].building;
        assert_eq!(sim.buildings.building(site).map(|b| b.is_complete()), Some(false));
        assert!(!sim.water.has_dam(c.0, c.1), "nothing is built before the work is done");
        assert_eq!(sim.buildings.stores.amount(Good::Logs), ledger, "nothing is paid up front");
        assert_eq!(emit(&mut sim, &mut p, &BuildOrder::Dam { cells: vec![], height: 1.5 }), Err(PlaceError::NoCells));
    }

    #[test]
    fn canal_orders_a_site_and_dam_needs_water() {
        let mut sim = SimState::new(1);
        sim.tech.unlock_all();
        found(&mut sim);
        stock(&mut sim);
        let c = river_cell(&sim);
        let mut p = Pending::default();
        emit(&mut sim, &mut p, &BuildOrder::Canal { cells: line_cells(c, (c.0 + 3, c.1)) }).unwrap();
        assert_eq!(sim.work.canal_plans.len(), 1, "a canal is a construction site with a plan");
        assert!(sim.buildings.buildings.iter().any(|b| b.kind == BuildingKind::Canal && !b.is_complete()));
        let dry = (0..sim.world.size).map(|x| (x, 1)).find(|&(x, y)| !is_wet(&sim, x, y)).unwrap();
        assert_eq!(check_dam(&sim, &[dry], 1.0), Err(PlaceError::NeedsWater));
        assert_eq!(check_dam(&sim, &[], 1.0), Err(PlaceError::NoCells));
    }

    #[test]
    fn a_cancelled_canal_site_leaves_no_plan() {
        let mut sim = SimState::new(1);
        sim.tech.unlock_all();
        found(&mut sim);
        stock(&mut sim);
        let mut p = Pending::default();
        let c = river_cell(&sim);
        emit(&mut sim, &mut p, &BuildOrder::Canal { cells: line_cells(c, (c.0 + 2, c.1)) }).unwrap();
        let id = sim.buildings.buildings.last().unwrap().id;
        sim.apply(Command::CancelSite { building: id }).unwrap();
        assert!(sim.work.canal_plans.is_empty() && sim.buildings.building(id).is_none());
    }



    #[test]
    fn a_line_over_a_dam_raises_it_and_raise_adds_one_step() {
        let mut sim = SimState::new(1);
        sim.tech.unlock_all();
        found(&mut sim);
        let c = river_cell(&sim);
        let line = line_cells((c.0, c.1 - 2), (c.0, c.1 + 2));
        sim.apply(Command::BuildDam { cells: line.clone(), height: 1.0 }).unwrap();
        assert_eq!(check_dam(&sim, &line, 1.5), Ok(()));
        assert!(line_cost(&sim, &line, 1.0, None).is_empty(), "already that high: free");
        let raise = line_cost(&sim, &line, 1.5, None);
        assert!(raise.iter().any(|a| a.0 == Good::Mud), "above one unit a dam is packed with mud: {raise:?}");
        match raise_order(&sim, line[2]) {
            Some(BuildOrder::Dam { cells, height }) => {
                assert_eq!(cells.len(), line.len());
                assert!((height - 1.5).abs() < 1e-4, "{height}");
            }
            o => panic!("{o:?}"),
        }
        assert_eq!(raise_order(&sim, (c.0 + 5, c.1 + 5)), None, "no dam there");
        let mut p = Pending::default();
        let order = raise_order(&sim, line[0]).unwrap();
        emit(&mut sim, &mut p, &order).expect("the raise is ordered");
    }

    #[test]
    fn a_line_built_to_a_crest_brings_each_cell_up_to_it() {
        let mut sim = SimState::new(1);
        let c = river_cell(&sim);
        let line = line_cells((c.0, c.1 - 2), (c.0, c.1 + 2));
        let ground: Vec<f32> = line.iter().map(|&(x, y)| *sim.height.get(x, y).unwrap()).collect();
        let crest = ground.iter().copied().fold(f32::MIN, f32::max) + 1.0;
        let (cells, heights) = line_heights(&sim, &line, 1.0, Some(crest));
        assert_eq!(cells, line);
        for (h, g) in heights.iter().zip(&ground) {
            assert!((h + g - crest).abs() < 1e-4 || (*h - 1.0).abs() < 1e-4 && g + 1.0 >= crest, "{h} on {g}");
        }
        let (base, added) = split_to_crest(&sim, &line, 1.0, crest);
        assert_eq!(base.len() + added.len(), line.len());
        assert!(!added.is_empty(), "the lowest cells need more than one unit to reach the crest");
        assert_eq!(line_heights(&sim, &line, 1.5, None).1, vec![1.5; line.len()]);
        sim.tech.unlock_all();
    }

    #[test]
    fn line_cells_includes_both_ends() {
        let l = line_cells((2, 2), (6, 4));
        assert_eq!((l[0], *l.last().unwrap()), ((2, 2), (6, 4)));
        assert_eq!(line_cells((3, 3), (3, 3)), vec![(3, 3)]);
    }
}
