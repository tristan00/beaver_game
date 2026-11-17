//! Pure building logic behind the build UI: menu grouping, placement validation, order emission,
//! cancel / priority and job counts. No Bevy types, so everything here is unit-tested.
use sim::buildings::{Amount, BuildingId, BuildingKind, Good, Site, Status, WET_DEPTH};
use sim::colony::{BeaverId, Colony, FamilyId, Job, OrderId, Stage};
use sim::tech::{Branch, Effect, TECHS};
use sim::water::Cell;
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

/// Test helper: dry building sites in the start valley (nothing wet within 5 cells), at least 12
/// cells apart, nearest the valley centre first.
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

/// Whether a site for `kind` at `pos` would be accepted: exactly the sim's `PlaceSite` rule
/// (`SimState::check_site`), with the tech lock reported first so the player sees what to earn.
/// Sites cost nothing to place (builders haul the materials in), so there is no affordability check.
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
    })
}

// ───────────────────────────── hover preview ─────────────────────────────

/// What hovering a cell with a building tool shows: where the footprint lands, whether the sim
/// would accept a site there, the cost, and for a dam the sim's own dam preview for that spot.
#[derive(Clone, Debug)]
pub struct SitePreview {
    pub kind: BuildingKind,
    /// Top-left cell of the footprint (the hovered cell is its centre).
    pub pos: Cell,
    pub cells: Vec<Cell>,
    pub verdict: Result<(), PlaceError>,
    pub cost: Vec<Amount>,
    /// Dams only: `sim::dam_preview::preview` for the spot at the default height.
    pub dam: Option<sim::dam_preview::DamPreview>,
}

impl SitePreview {
    pub fn is_valid(&self) -> bool {
        self.verdict.is_ok()
    }
}

/// Preview of `kind` with the cursor over `hover`.
pub fn site_preview(sim: &SimState, kind: BuildingKind, hover: Cell) -> SitePreview {
    let pos = anchor(kind, hover);
    let (cells, cost, dam) = if kind == BuildingKind::Dam {
        let cells = sim::dam_preview::spot_cells(pos);
        let dam = sim::dam_preview::preview(sim, &cells, sim::dam_preview::DEFAULT_HEIGHT);
        let cost = sim::dam_preview::dam_cost(cells.len());
        (cells, cost, dam)
    } else {
        (footprint(kind, pos), kind.spec().cost.to_vec(), None)
    };
    SitePreview { kind, pos, cells, verdict: check_building(sim, kind, pos), cost, dam }
}

/// The one command a click on this preview issues: a place-site command if valid, nothing if not.
pub fn click_command(p: &SitePreview) -> Option<Command> {
    p.is_valid().then_some(Command::PlaceSite { kind: p.kind, pos: p.pos })
}

/// The dam hover readout: every figure comes straight from the sim's dam preview.
#[derive(Clone, Debug, PartialEq)]
pub struct DamReadout {
    /// Rise of the water surface at the dam, in world units.
    pub rise: f32,
    pub level: f32,
    pub flooded_cells: usize,
    pub flooded_trees: usize,
    pub flooded_buildings: usize,
    pub cost: Vec<Amount>,
    /// Inflow pushing on the dam, water units per day.
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

    /// Short lines for the map tooltip, most important first.
    pub fn lines(&self) -> Vec<String> {
        let fill = if self.fill_days.is_finite() { format!("fills in {:.0} d", self.fill_days) } else { "never fills".into() };
        vec![
            format!("Water rises {:.2}  (level {:.2})", self.rise, self.level),
            format!("Floods {} cells, {} trees, {} buildings", self.flooded_cells, self.flooded_trees, self.flooded_buildings),
            format!("Cost {}", cost_text(&self.cost)),
            format!("Current pressure {:.1}/day  ({fill})", self.pressure),
        ]
    }
}

/// Cells one catalogue cost buys: a standard 6-cell dam span, or a single canal cell.
pub fn unit_cells(kind: BuildingKind) -> f32 {
    if kind == BuildingKind::Dam { sim::dam_preview::DAM_SPAN_CELLS } else { (kind.spec().footprint.0 * kind.spec().footprint.1) as f32 }
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
    // No affordability check: a dam is a construction site and the family hauls the wood in.
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
    lack(sim, &path_cost(BuildingKind::Canal, cells.len()))
}

/// Pond the dam would create (None if the preview itself fails).
pub fn dam_preview(sim: &SimState, cells: &[Cell], height: f32) -> Option<sim::dam_preview::DamPreview> {
    sim::dam_preview::preview(sim, cells, height)
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
/// dams become construction sites the family builds (`Command::OrderDam`); canals queue a digger
/// work order and apply their `Command` once it completes.
pub fn emit(sim: &mut SimState, pending: &mut Pending, order: &BuildOrder) -> Result<String, PlaceError> {
    match order {
        BuildOrder::Building { kind, pos } => {
            check_building(sim, *kind, *pos)?;
            sim.apply(Command::PlaceSite { kind: *kind, pos: *pos }).map_err(|e| PlaceError::Sim(e.to_string()))?;
            Ok(format!("{} ordered at ({}, {})", kind.spec().name, pos.0, pos.1))
        }
        BuildOrder::Dam { cells, height } => {
            check_dam(sim, cells, *height)?;
            // A construction site: the family fells, hauls and builds; the sim raises the water dam
            // when the site is finished (`Command::OrderDam`).
            sim.apply(Command::OrderDam { cells: cells.clone(), height: *height }).map_err(|e| PlaceError::Sim(e.to_string()))?;
            Ok(format!("Dam of {} cells ordered", cells.len()))
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
        let site = dry_sites(&sim)[0];
        // locked
        assert!(matches!(check_building(&sim, BuildingKind::Workshop, site), Err(PlaceError::Locked(..))));
        // a site costs nothing to place: the family hauls the materials in
        assert_eq!(check_building(&sim, BuildingKind::Lodge, site), Ok(()));
        assert_eq!(check_building(&sim, BuildingKind::Lodge, site), Ok(()));
        assert_eq!(check_building(&sim, BuildingKind::Lodge, (sim.world.size - 1, 5)), Err(PlaceError::OutOfBounds));
        // dry-only building on water
        sim.water.depth.as_mut_slice()[site.1 * sim.world.size + site.0] = 1.0;
        assert_eq!(check_building(&sim, BuildingKind::Lodge, (site.0 - 1, site.1 - 1)), Err(PlaceError::NeedsDry));
    }

    #[test]
    fn building_click_places_a_site_and_blocks_overlap() {
        let mut sim = SimState::new(1);
        found(&mut sim);
        let mut p = Pending::default();
        let o = BuildOrder::Building { kind: BuildingKind::Lodge, pos: dry_sites(&sim)[0] };
        let n = sim.buildings.buildings.len();
        emit(&mut sim, &mut p, &o).unwrap();
        assert_eq!(sim.buildings.buildings.len(), n + 1);
        assert_eq!(sim.buildings.buildings.last().unwrap().status, Status::Site);
        assert_eq!(emit(&mut sim, &mut p, &o), Err(PlaceError::Blocked));
        assert_eq!(sim.buildings.buildings.len(), n + 1, "failed order changes nothing");
    }

    // ── placement preview and dam hover (task R19-22) ──

    /// A 32 by 32 window of cells around the start valley's river (wet and dry cells both appear).
    fn window(sim: &SimState) -> Vec<Cell> {
        let c = river_cell(sim);
        let n = sim.world.size;
        let (x0, y0) = (c.0.saturating_sub(16).min(n - 32), c.1.saturating_sub(16).min(n - 32));
        (y0..y0 + 32).flat_map(|y| (x0..x0 + 32).map(move |x| (x, y))).collect()
    }

    /// The catalogue site rule, written out from the spec alone (footprint, `Site`, water depth).
    fn catalogue_rule(sim: &SimState, kind: BuildingKind, hover: Cell) -> bool {
        let spec = kind.spec();
        let (w, h) = spec.footprint;
        let pos = (hover.0.saturating_sub(w / 2), hover.1.saturating_sub(h / 2));
        let cells: Vec<Cell> = (0..h).flat_map(|dy| (0..w).map(move |dx| (pos.0 + dx, pos.1 + dy))).collect();
        let n = sim.world.size;
        if cells.iter().any(|c| c.0 >= n || c.1 >= n) {
            return false;
        }
        if sim.buildings.buildings.iter().any(|b| b.cells().any(|c| cells.contains(&c))) {
            return false;
        }
        let wet = |c: &Cell| sim.water.depth.get(c.0, c.1).is_some_and(|&d| d >= WET_DEPTH);
        match spec.site {
            Site::Dry => !cells.iter().any(wet),
            Site::TouchesWater => cells.iter().any(wet),
            Site::WaterByDock => false, // no dock stands in this world
        }
    }

    #[test]
    fn preview_equals_the_catalogue_site_rule_and_a_click_issues_one_place_site_command() {
        let mut sim = SimState::new(1);
        found(&mut sim);
        sim.tech.unlock_all();
        // something already built, so overlap is exercised too
        let d = dry_sites(&sim)[0];
        sim.apply(Command::PlaceSite { kind: BuildingKind::Warehouse, pos: d }).unwrap();
        let cells = window(&sim);
        assert_eq!(cells.len(), 32 * 32);
        for kind in BuildingKind::ALL {
            let (mut valid, mut invalid) = (0, 0);
            let mut first_valid = None;
            for &hover in &cells {
                let p = site_preview(&sim, kind, hover);
                let want = catalogue_rule(&sim, kind, hover);
                assert_eq!(p.is_valid(), want, "{kind:?} hovering {hover:?}: preview {:?}", p.verdict);
                assert_eq!(p.cells.len(), kind.spec().footprint.0 * kind.spec().footprint.1);
                match click_command(&p) {
                    Some(c) => {
                        assert!(want, "{kind:?} {hover:?}: a click on an invalid cell must issue nothing");
                        assert_eq!(c, Command::PlaceSite { kind, pos: p.pos });
                        valid += 1;
                        first_valid.get_or_insert(c);
                    }
                    None => {
                        assert!(!want, "{kind:?} {hover:?}: a click on a valid cell must issue a command");
                        invalid += 1;
                    }
                }
            }
            if kind != BuildingKind::Barge {
                assert!(valid > 0 && invalid > 0, "{kind:?}: window shows {valid} valid and {invalid} invalid cells");
            }
            // the issued command is accepted by the sim, and exactly one site appears
            if let Some(cmd) = first_valid {
                let before = sim.buildings.buildings.len();
                sim.apply(cmd).unwrap();
                assert_eq!(sim.buildings.buildings.len(), before + 1);
            }
        }
    }

    #[test]
    fn hovering_a_dam_spot_shows_exactly_the_sims_dam_preview() {
        let mut sim = SimState::new(1);
        found(&mut sim);
        sim.tech.unlock_all();
        let mut seen_flood = false;
        for hover in window(&sim) {
            let p = site_preview(&sim, BuildingKind::Dam, hover);
            let want = sim::dam_preview::preview(&sim, &sim::dam_preview::spot_cells(p.pos), sim::dam_preview::DEFAULT_HEIGHT);
            assert_eq!(p.dam, want, "dam hover at {hover:?}");
            let (Some(d), Some(r)) = (&want, p.dam.as_ref().map(DamReadout::from_preview)) else { continue };
            assert_eq!((r.rise, r.level, r.flooded_cells, r.flooded_trees), (d.rise, d.level, d.flooded.len(), d.flooded_trees));
            assert_eq!((r.flooded_buildings, &r.cost, r.pressure), (d.flooded_buildings.len(), &d.cost, d.pressure));
            assert_eq!(p.cost, d.cost, "the panel cost is the sim's cost");
            let lines = r.lines().join("
");
            assert!(lines.contains(&format!("{:.2}", d.rise)) && lines.contains(&format!("{} cells", d.flooded.len())), "{lines}");
            assert!(lines.contains(&format!("{} trees", d.flooded_trees)) && lines.contains(&format!("{} buildings", d.flooded_buildings.len())));
            assert!(lines.contains(&format!("{:.1}/day", d.pressure)) && lines.contains(&cost_text(&d.cost)));
            seen_flood |= !d.flooded.is_empty();
        }
        assert!(seen_flood, "some spot in the window must flood cells");
    }

    #[test]
    fn a_placed_dam_site_is_a_real_dam_with_the_previewed_cost() {
        let mut sim = SimState::new(1);
        found(&mut sim);
        let hover = window(&sim).into_iter().find(|&h| site_preview(&sim, BuildingKind::Dam, h).is_valid()).unwrap();
        let p = site_preview(&sim, BuildingKind::Dam, hover);
        sim.apply(click_command(&p).unwrap()).unwrap();
        assert_eq!(sim.work.dam_plans.len(), 1);
        assert_eq!(sim.work.dam_plans[0].cells, p.cells);
    }

    #[test]
    fn dam_order_places_a_site_and_builds_the_water_dam_only_when_it_completes() {
        let mut sim = SimState::new(1);
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
        let c = river_cell(&sim);
        let logs = sim.buildings.stores.amount(Good::Logs);
        emit(&mut sim, &mut p, &BuildOrder::Canal { cells: vec![c] }).unwrap();
        let c2 = (0..sim.world.size * sim.world.size).map(|i| (i % sim.world.size, i / sim.world.size)).find(|&q| q != c && is_wet(&sim, q.0, q.1)).unwrap();
        emit(&mut sim, &mut p, &BuildOrder::Canal { cells: vec![c2] }).unwrap();
        let rows = order_rows(&sim, &p);
        assert_eq!(rows.len(), 2);
        let last = rows[1].order;
        assert!(prioritise(&mut sim.colony, last));
        assert_eq!(order_rows(&sim, &p)[0].order, last);
        let first = rows[0].target;
        cancel(&mut sim, &mut p, first).unwrap();
        let _ = logs;
        assert!(sim.buildings.stores.amount(Good::Logs) <= 100.0);
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
