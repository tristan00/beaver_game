//! Pure building logic behind the build UI: menu grouping, placement validation, order emission,
//! cancel / priority and job counts. No Bevy types, so everything here is unit-tested.
use sim::buildings::{Amount, BuildingId, BuildingKind, Good, Site, Status, Terrain, WET_DEPTH};
use sim::colony::{BeaverId, Colony, FamilyId, Job, OrderId, Stage};
use sim::tech::{Branch, Effect, TECHS};
use sim::water::{preview_dam, Cell, DamPreview};
use sim::{Command, SimState};

pub const DEFAULT_DAM_HEIGHT: f32 = 1.0;
pub const MIN_DAM_HEIGHT: f32 = 0.5;
pub const MAX_DAM_HEIGHT: f32 = 3.0;
pub const MAX_DAM_CELLS: usize = 48;
pub const MAX_CANAL_CELLS: usize = 80;
pub const CANAL_DEPTH: f32 = 0.5;

// ───────────────────────────── menu ─────────────────────────────

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
    /// For locked entries: (tech name, experience needed, experience the colony has in the branch).
    pub need: Option<(&'static str, f32, f32)>,
}

/// Every building, grouped by tech branch (basics first). Locked ones carry the experience needed.
pub fn menu(sim: &SimState) -> Vec<MenuEntry> {
    let mut out = Vec::new();
    for kind in BuildingKind::ALL.into_iter().filter(|k| sim.features.allows_building(*k)) {
        let tech = TECHS.iter().find(|t| t.effects.contains(&Effect::Unlock(kind)));
        let group = tech.map_or(Group::Base, |t| Group::Branch(t.branch));
        let unlocked = sim.tech.allows(kind);
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
    out.sort_by_key(|e| rank(e.group)); // stable: catalogue order inside a group
    out
}

pub fn cost_text(cost: &[Amount]) -> String {
    let name = |g: Good| match g {
        Good::Logs => "log",
        Good::Sticks => "stk",
        Good::Mud => "mud",
        Good::Stone => "stn",
        Good::Food => "food",
        Good::Baskets => "bsk",
        Good::Rope => "rope",
        Good::Tools => "tool",
    };
    cost.iter().map(|(g, a)| format!("{} {}", fmt_amount(*a), name(*g))).collect::<Vec<_>>().join("  ")
}

fn fmt_amount(a: f32) -> String {
    if (a - a.round()).abs() < 0.05 { format!("{}", a.round() as i32) } else { format!("{a:.1}") }
}

pub fn is_path_tool(kind: BuildingKind) -> bool {
    matches!(kind, BuildingKind::Dam | BuildingKind::Canal)
}

// ───────────────────────────── placement ─────────────────────────────

#[derive(Clone, Debug, PartialEq)]
pub enum PlaceError {
    Locked(&'static str, f32, f32),
    OutOfBounds,
    Blocked,
    NeedsDry,
    NeedsWater,
    NeedsDock,
    Afford(Good, f32, f32),
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
            PlaceError::Afford(g, need, have) => write!(f, "Not enough {g:?}: need {}, have {}", fmt_amount(*need), fmt_amount(*have)),
            PlaceError::NoCells => write!(f, "Nothing drawn yet"),
            PlaceError::TooLong(max) => write!(f, "Too long (max {max} cells)"),
            PlaceError::Sim(m) => write!(f, "{m}"),
        }
    }
}

/// Water for dams and canals: ponded water, rivers and lakes.
pub fn is_wet(sim: &SimState, x: usize, y: usize) -> bool {
    sim.water.depth.get(x, y).is_some_and(|&d| d >= WET_DEPTH)
        || sim.world.river.get(x, y).copied().unwrap_or(false)
        || sim.world.lake.get(x, y).copied().unwrap_or(false)
}

fn sim_wet(sim: &SimState, x: usize, y: usize) -> bool {
    sim.water.depth.get(x, y).is_some_and(|&d| d >= WET_DEPTH)
}

/// Top-left cell so the footprint is centred on `c`.
pub fn anchor(kind: BuildingKind, c: Cell) -> Cell {
    let (w, h) = kind.spec().footprint;
    (c.0.saturating_sub(w / 2), c.1.saturating_sub(h / 2))
}

pub fn footprint(kind: BuildingKind, pos: Cell) -> Vec<Cell> {
    let (w, h) = kind.spec().footprint;
    (0..h).flat_map(|dy| (0..w).map(move |dx| (pos.0 + dx, pos.1 + dy))).collect()
}

fn lack(sim: &SimState, cost: &[Amount]) -> Result<(), PlaceError> {
    for &(g, a) in cost {
        let have = sim.buildings.stores.amount(g);
        if have + 1e-4 < a {
            return Err(PlaceError::Afford(g, a, have));
        }
    }
    Ok(())
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

/// Mirrors `Buildings::place` without changing anything, so the ghost tint matches what the sim accepts.
pub fn check_building(sim: &SimState, kind: BuildingKind, pos: Cell) -> Result<(), PlaceError> {
    locked(sim, kind)?;
    let cells = footprint(kind, pos);
    if cells.iter().any(|&(x, y)| !sim.water.depth.in_bounds(x, y)) {
        return Err(PlaceError::OutOfBounds);
    }
    let taken = |c: &Cell| sim.buildings.buildings.iter().any(|b| footprint(b.kind, b.pos).contains(c));
    if cells.iter().any(taken) {
        return Err(PlaceError::Blocked);
    }
    let wet = |&(x, y): &Cell| sim_wet(sim, x, y);
    match kind.spec().site {
        Site::Dry if cells.iter().any(wet) => return Err(PlaceError::NeedsDry),
        Site::TouchesWater if !cells.iter().any(wet) => return Err(PlaceError::NeedsWater),
        Site::WaterByDock if !wet(&pos) => return Err(PlaceError::NeedsWater),
        Site::WaterByDock => {
            let near_dock = sim.buildings.buildings.iter().any(|b| {
                b.kind == BuildingKind::Dock
                    && b.is_complete()
                    && footprint(b.kind, b.pos).iter().any(|d| d.0.abs_diff(pos.0) + d.1.abs_diff(pos.1) == 1)
            });
            if !near_dock {
                return Err(PlaceError::NeedsDock);
            }
        }
        _ => {}
    }
    lack(sim, kind.spec().cost)
}

/// Cells one catalogue cost buys: a standard 6-cell dam span, or a single canal cell.
pub fn unit_cells(kind: BuildingKind) -> f32 {
    if kind == BuildingKind::Dam { 6.0 } else { (kind.spec().footprint.0 * kind.spec().footprint.1) as f32 }
}

/// Cost of a dam or canal of `n` cells.
pub fn path_cost(kind: BuildingKind, n: usize) -> Vec<Amount> {
    kind.spec().cost.iter().map(|&(g, a)| (g, a * n as f32 / unit_cells(kind))).collect()
}

fn path_work(sim: &SimState, kind: BuildingKind, n: usize) -> f32 {
    kind.spec().work * n as f32 / unit_cells(kind) * sim.tech.build_work_factor()
}

/// Cells of a straight line between two cells (Bresenham, both ends included).
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
    if !cells.iter().any(|&(x, y)| is_wet(sim, x, y)) {
        return Err(PlaceError::NeedsWater);
    }
    if !(MIN_DAM_HEIGHT..=MAX_DAM_HEIGHT).contains(&height) {
        return Err(PlaceError::Sim(format!("Dam height must be {MIN_DAM_HEIGHT}..{MAX_DAM_HEIGHT}")));
    }
    lack(sim, &path_cost(BuildingKind::Dam, cells.len()))
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
    lack(sim, &path_cost(BuildingKind::Canal, cells.len()))
}

/// Pond the dam would create (None if the preview itself fails).
pub fn dam_preview(sim: &SimState, cells: &[Cell], height: f32) -> Option<DamPreview> {
    preview_dam(sim, cells, height).ok()
}

// ───────────────────────────── emission ─────────────────────────────

/// What the player asked for.
#[derive(Clone, Debug, PartialEq)]
pub enum BuildOrder {
    Building { kind: BuildingKind, pos: Cell },
    Dam { cells: Vec<Cell>, height: f32 },
    Canal { cells: Vec<Cell> },
}

/// A dam or canal waiting for its builders. The sim `command` runs when the work order completes.
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

fn centre(cells: &[Cell]) -> (f32, f32) {
    let n = cells.len().max(1) as f32;
    (cells.iter().map(|c| c.0 as f32 + 0.5).sum::<f32>() / n, cells.iter().map(|c| c.1 as f32 + 0.5).sum::<f32>() / n)
}

/// Turns a player order into sim state: buildings are placed (cost paid, builder work order queued);
/// dams and canals queue a builder/digger work order and apply their `Command` once it completes.
pub fn emit(sim: &mut SimState, pending: &mut Pending, order: &BuildOrder) -> Result<String, PlaceError> {
    match order {
        BuildOrder::Building { kind, pos } => {
            check_building(sim, *kind, *pos)?;
            let terrain = Terrain { height: &sim.height, depth: &sim.water.depth };
            sim.buildings
                .place(*kind, *pos, &sim.tech, terrain, &mut sim.colony)
                .map_err(|e| PlaceError::Sim(format!("{e}")))?;
            Ok(format!("{} ordered at ({}, {})", kind.spec().name, pos.0, pos.1))
        }
        BuildOrder::Dam { cells, height } => {
            check_dam(sim, cells, *height)?;
            let kind = BuildingKind::Dam;
            queue_path(sim, pending, kind, cells, Command::BuildDam { cells: cells.clone(), height: *height })
        }
        BuildOrder::Canal { cells } => {
            check_canal(sim, cells)?;
            let kind = BuildingKind::Canal;
            queue_path(sim, pending, kind, cells, Command::DigCanal { cells: cells.clone(), depth: CANAL_DEPTH })
        }
    }
}

fn queue_path(sim: &mut SimState, pending: &mut Pending, kind: BuildingKind, cells: &[Cell], command: Command) -> Result<String, PlaceError> {
    let cost = path_cost(kind, cells.len());
    let work = path_work(sim, kind, cells.len());
    let job = if kind == BuildingKind::Canal { Job::Digger } else { Job::Builder };
    let order = sim.colony.queue_order(job, centre(cells), work).map_err(|e| PlaceError::Sim(e.to_string()))?;
    sim.buildings.stores.withdraw_all(&cost).expect("affordability checked");
    pending.0.push(PendingWork { order, kind, cells: cells.to_vec(), command, total_work: work, cost });
    Ok(format!("{} of {} cells ordered", kind.spec().name, cells.len()))
}

/// Runs the commands of dams/canals whose work order has completed. Returns messages.
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

// ───────────────────────────── orders: cancel & priority ─────────────────────────────

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OrderTarget {
    Building(BuildingId),
    Pending(OrderId),
}

#[derive(Clone, Debug, PartialEq)]
pub struct OrderRow {
    pub order: OrderId,
    pub target: OrderTarget,
    pub label: String,
    /// 0..=1
    pub progress: f32,
    /// Position in the builders' queue (0 = next).
    pub queue_pos: usize,
}

/// Every unfinished construction, with progress. Only sim data is read.
pub fn order_rows(sim: &SimState, pending: &Pending) -> Vec<OrderRow> {
    let colony = &sim.colony;
    let row = |order: OrderId, target, label: String, total: f32| {
        let (qi, o) = colony.orders.iter().enumerate().find(|(_, o)| o.id == order)?;
        Some(OrderRow { order, target, label, progress: (1.0 - o.remaining / total.max(1e-3)).clamp(0.0, 1.0), queue_pos: qi })
    };
    let mut rows = Vec::new();
    for b in &sim.buildings.buildings {
        if let Status::UnderConstruction(o) = b.status {
            let total = b.kind.spec().work * sim.tech.build_work_factor();
            rows.extend(row(o, OrderTarget::Building(b.id), format!("{} ({}, {})", b.kind.spec().name, b.pos.0, b.pos.1), total));
        }
    }
    for p in &pending.0 {
        let c = p.cells[0];
        rows.extend(row(p.order, OrderTarget::Pending(p.order), format!("{} x{} ({}, {})", p.kind.spec().name, p.cells.len(), c.0, c.1), p.total_work));
    }
    rows.sort_by_key(|r| r.queue_pos);
    rows
}

/// Cancels an unfinished order and refunds its cost.
pub fn cancel(sim: &mut SimState, pending: &mut Pending, target: OrderTarget) -> Result<(), String> {
    match target {
        OrderTarget::Building(id) => sim.buildings.cancel(id, &mut sim.colony).map_err(|e| e.to_string()),
        OrderTarget::Pending(o) => {
            let i = pending.0.iter().position(|p| p.order == o).ok_or("order already finished")?;
            sim.colony.cancel_order(o).map_err(|e| e.to_string())?;
            let w = pending.0.remove(i);
            refund(sim, &w.cost);
            Ok(())
        }
    }
}

/// Moves a work order to the front of the builders' queue.
pub fn prioritise(colony: &mut Colony, order: OrderId) -> bool {
    match colony.orders.iter().position(|o| o.id == order) {
        Some(i) => {
            let o = colony.orders.remove(i).expect("index valid");
            colony.orders.push_front(o);
            true
        }
        None => false,
    }
}

// ───────────────────────────── jobs ─────────────────────────────

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct JobRow {
    pub job: Job,
    /// Beavers whose personal job is set to this.
    pub assigned: usize,
    /// Beavers doing it right now.
    pub working: usize,
}

pub fn job_rows(colony: &Colony) -> Vec<JobRow> {
    colony
        .enabled_jobs()
        .into_iter()
        .map(|job| JobRow {
            job,
            assigned: colony.beavers.iter().filter(|b| b.job == Some(job)).count(),
            working: colony.beavers.iter().filter(|b| b.current_job == Some(job)).count(),
        })
        .collect()
}

pub fn adults(colony: &Colony) -> usize {
    colony.beavers.iter().filter(|b| b.stage == Stage::Adult).count()
}

/// Adults with no personal job (they follow their family's priorities).
pub fn unassigned(colony: &Colony) -> usize {
    colony.beavers.iter().filter(|b| b.stage == Stage::Adult && b.job.is_none()).count()
}

/// Sets how many adults are assigned to `job`. Growing takes unassigned adults first (then the
/// most skilled from other jobs); shrinking frees the least skilled. Returns the new count.
pub fn set_job_count(colony: &mut Colony, job: Job, target: usize) -> Result<usize, String> {
    let mine: Vec<BeaverId> = colony.beavers.iter().filter(|b| b.job == Some(job)).map(|b| b.id).collect();
    let cap = adults(colony).max(mine.len());
    let target = target.min(cap);
    if target < mine.len() {
        let mut v: Vec<_> = colony.beavers.iter().filter(|b| b.job == Some(job)).map(|b| (b.skill(job), b.id)).collect();
        v.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
        for (_, id) in v.into_iter().take(mine.len() - target) {
            colony.set_job(id, None).map_err(|e| e.to_string())?;
        }
    } else {
        let mut v: Vec<_> = colony
            .beavers
            .iter()
            .filter(|b| b.stage == Stage::Adult && b.job != Some(job))
            .map(|b| (b.job.is_some(), b.skill(job), b.id))
            .collect();
        v.sort_by(|a, b| a.0.cmp(&b.0).then(b.1.total_cmp(&a.1)).then(a.2.cmp(&b.2)));
        for (_, _, id) in v.into_iter().take(target - mine.len()) {
            colony.set_job(id, Some(job)).map_err(|e| e.to_string())?;
        }
    }
    Ok(colony.beavers.iter().filter(|b| b.job == Some(job)).count())
}

/// Moves the priority at `index` one place up in a family's list.
pub fn promote_priority(colony: &mut Colony, fam: FamilyId, index: usize) -> Result<(), String> {
    let mut p = colony.family(fam).ok_or("unknown family")?.priorities.clone();
    if index == 0 || index >= p.len() {
        return Ok(());
    }
    p.swap(index, index - 1);
    colony.set_family_priorities(fam, p).map_err(|e| e.to_string())
}

// ───────────────────────────── tests ─────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use sim::SimState;

    fn found(sim: &mut SimState) {
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
        // guard post and scent mound belong to switched-off systems
        assert_eq!(m.len(), BuildingKind::ALL.len() - 2);
        assert!(m.iter().all(|e| e.kind != BuildingKind::GuardPost && e.kind != BuildingKind::ScentMound));
        assert_eq!(m[0].group, Group::Base);
        let lock = m.iter().find(|e| e.kind == BuildingKind::Lock).unwrap();
        assert_eq!(lock.group, Group::Branch(Branch::Water));
        assert!(!lock.unlocked);
        assert_eq!(lock.need.map(|n| (n.0, n.1)), Some(("Flow Control", 3.0)));
        let dam = m.iter().find(|e| e.kind == BuildingKind::Dam).unwrap();
        assert!(dam.unlocked && dam.need.is_none());
        // groups are contiguous
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
        // locked
        assert!(matches!(check_building(&sim, BuildingKind::Workshop, (50, 50)), Err(PlaceError::Locked(..))));
        // cannot afford
        assert!(matches!(check_building(&sim, BuildingKind::Lodge, (50, 50)), Err(PlaceError::Afford(..))));
        stock(&mut sim);
        assert_eq!(check_building(&sim, BuildingKind::Lodge, (50, 50)), Ok(()));
        assert_eq!(check_building(&sim, BuildingKind::Lodge, (sim.world.size - 1, 5)), Err(PlaceError::OutOfBounds));
        // dry-only building on water
        sim.water.depth.as_mut_slice()[50 * sim.world.size + 50] = 1.0;
        assert_eq!(check_building(&sim, BuildingKind::Lodge, (49, 49)), Err(PlaceError::NeedsDry));
    }

    #[test]
    fn building_emits_work_order_and_blocks_overlap() {
        let mut sim = SimState::new(1);
        found(&mut sim);
        stock(&mut sim);
        let mut p = Pending::default();
        let before = sim.colony.orders.len();
        let o = BuildOrder::Building { kind: BuildingKind::Lodge, pos: (60, 60) };
        emit(&mut sim, &mut p, &o).unwrap();
        assert_eq!(sim.colony.orders.len(), before + 1);
        assert_eq!(sim.buildings.stores.amount(Good::Logs), 80.0);
        assert_eq!(emit(&mut sim, &mut p, &o), Err(PlaceError::Blocked));
        assert_eq!(sim.colony.orders.len(), before + 1, "failed order changes nothing");
    }

    #[test]
    fn dam_order_queues_work_then_emits_build_dam_command() {
        let mut sim = SimState::new(1);
        found(&mut sim);
        stock(&mut sim);
        let c = river_cell(&sim);
        let cells = line_cells((c.0 - 2, c.1), (c.0 + 2, c.1));
        let mut p = Pending::default();
        emit(&mut sim, &mut p, &BuildOrder::Dam { cells: cells.clone(), height: 1.5 }).unwrap();
        assert_eq!(p.0.len(), 1);
        assert_eq!(p.0[0].command, Command::BuildDam { cells: cells.clone(), height: 1.5 });
        assert!(!sim.water.has_dam(c.0, c.1), "nothing is built before the work is done");
        assert_eq!(sim.buildings.stores.amount(Good::Logs), 100.0 - 15.0 * 5.0 / 6.0);
        assert!(poll_pending(&mut sim, &mut p).is_empty());
        // builders finish the order
        let id = p.0[0].order;
        sim.colony.cancel_order(id).unwrap_or(()); // simulate completion: order leaves the queue
        let msgs = poll_pending(&mut sim, &mut p);
        assert_eq!(msgs.len(), 1);
        assert!(sim.water.has_dam(c.0, c.1));
    }

    #[test]
    fn canal_emits_dig_canal_and_dam_needs_water() {
        let mut sim = SimState::new(1);
        found(&mut sim);
        stock(&mut sim);
        let c = river_cell(&sim);
        let mut p = Pending::default();
        emit(&mut sim, &mut p, &BuildOrder::Canal { cells: line_cells(c, (c.0 + 3, c.1)) }).unwrap();
        assert!(matches!(p.0[0].command, Command::DigCanal { .. }));
        // a line far from any water is rejected
        let dry = (0..sim.world.size).map(|x| (x, 1)).find(|&(x, y)| !is_wet(&sim, x, y)).unwrap();
        assert_eq!(check_dam(&sim, &[dry], 1.0), Err(PlaceError::NeedsWater));
        assert_eq!(check_dam(&sim, &[], 1.0), Err(PlaceError::NoCells));
    }

    #[test]
    fn cancel_refunds_and_priority_moves_to_front() {
        let mut sim = SimState::new(1);
        found(&mut sim);
        stock(&mut sim);
        let mut p = Pending::default();
        emit(&mut sim, &mut p, &BuildOrder::Building { kind: BuildingKind::Lodge, pos: (60, 60) }).unwrap();
        emit(&mut sim, &mut p, &BuildOrder::Building { kind: BuildingKind::Burrow, pos: (70, 60) }).unwrap();
        let rows = order_rows(&sim, &p);
        assert_eq!(rows.len(), 2);
        let last = rows[1].order;
        assert!(prioritise(&mut sim.colony, last));
        assert_eq!(order_rows(&sim, &p)[0].order, last);
        let lodge = rows.iter().find(|r| r.label.starts_with("Lodge")).unwrap().target;
        cancel(&mut sim, &mut p, lodge).unwrap();
        assert_eq!(sim.buildings.stores.amount(Good::Logs), 100.0);
        assert_eq!(order_rows(&sim, &p).len(), 1);
    }

    #[test]
    fn job_counts_grow_shrink_and_cap() {
        let mut sim = SimState::new(1);
        found(&mut sim);
        let n = adults(&sim.colony);
        assert!(n >= 2, "need adults, got {n}");
        assert_eq!(set_job_count(&mut sim.colony, Job::Logger, 2).unwrap(), 2);
        assert_eq!(job_rows(&sim.colony).iter().find(|r| r.job == Job::Logger).unwrap().assigned, 2);
        assert_eq!(unassigned(&sim.colony), n - 2);
        assert_eq!(set_job_count(&mut sim.colony, Job::Logger, 1).unwrap(), 1);
        assert_eq!(set_job_count(&mut sim.colony, Job::Logger, 999).unwrap(), n);
        assert_eq!(set_job_count(&mut sim.colony, Job::Logger, 0).unwrap(), 0);
    }

    #[test]
    fn line_cells_includes_both_ends() {
        let l = line_cells((2, 2), (6, 4));
        assert_eq!((l[0], *l.last().unwrap()), ((2, 2), (6, 4)));
        assert_eq!(line_cells((3, 3), (3, 3)), vec![(3, 3)]);
    }
}
