//! Read-only views the interface needs: idle beavers, current problems with their cause and a fix,
//! number breakdowns (spec §4.3), the river's side profile, and how long the ponds would last
//! through a dry summer. Nothing here changes the state except [`apply_fix`], which performs a
//! problem's fix through `Command`s.
use crate::agents::{Assignment, Task};
use crate::buildings::{BuildingId, BuildingKind, Good, Producer, Use, WET_DEPTH};
use crate::colony::{BeaverId, DeathCause, Stage};
use crate::jobs::RoleKind;
use crate::resources::ResourceKind;
use crate::society::RequestId;
use crate::water::{Cell, Season};
use crate::work::Role;
use crate::{Command, SimState};
/// Stage counts, planting and tending rates and the next mature tree of an area (`forestry`).
pub use crate::forestry::{forest_report, ForestReport};

/// Most problems the interface shows at once.
pub const MAX_PROBLEMS: usize = 4;
/// Food stocks lasting fewer days than this raise a warning; under a third of it, urgent.
pub const FOOD_WARN_DAYS: f32 = 20.0;
/// A good counts as full at this share of its capacity.
pub const FULL_SHARE: f32 = 0.98;
/// A dam weaker than this is reported.
pub const WEAK_DAM: f32 = 0.35;
/// Beavers a food or idle fix puts to work at once (the "+2" on its button).
pub const FIX_WORKERS: usize = 2;
/// Half-size (cells) of the area around the lodge where a food fix sends gatherers.
pub const FOOD_GATHER_RADIUS: usize = 30;

/// Working-age beavers (yearlings and adults) with no collect assignment and no workplace slot.
pub fn idle_beavers(sim: &SimState) -> Vec<BeaverId> {
    sim.colony
        .beavers
        .iter()
        .filter(|b| matches!(b.stage, Stage::Yearling | Stage::Adult))
        .filter(|b| sim.work.slot_of(b.id).is_none())
        .filter(|b| sim.agents.get(b.id).is_none_or(|a| a.assignment.is_none() && matches!(a.task, Task::Idle | Task::Rest)))
        .map(|b| b.id)
        .collect()
}

// ───────────────────────────── breakdowns ─────────────────────────────

/// What a breakdown row links to.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Source {
    Building(BuildingId),
    Beaver(BeaverId),
    Cell(Cell),
    None,
}

/// One itemised term behind a number. `label` is at most three words.
#[derive(Clone, Debug, PartialEq)]
pub struct BreakdownRow {
    pub label: String,
    pub value: f32,
    pub source: Source,
}

/// A number and what makes it up; rows are sorted by `|value|`, largest first.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Breakdown {
    pub total: f32,
    pub rows: Vec<BreakdownRow>,
}

impl Breakdown {
    fn new(total: f32, mut rows: Vec<BreakdownRow>) -> Self {
        rows.sort_by(|a, b| b.value.abs().total_cmp(&a.value.abs()));
        Breakdown { total, rows }
    }
}

fn row(label: impl Into<String>, value: f32, source: Source) -> BreakdownRow {
    BreakdownRow { label: label.into(), value, source }
}

fn building_source(sim: &SimState, b: BuildingId) -> Source {
    if sim.buildings.building(b).is_some() { Source::Building(b) } else { Source::None }
}

fn building_name(sim: &SimState, b: BuildingId) -> String {
    sim.buildings.building(b).map_or("Gone", |b| b.kind.spec().name).to_string()
}

/// Each good's daily inflow (7-day mean of what reached the stores), per producing building,
/// "By order" for gatherers on a gather order, "Other" for the rest.
pub fn good_in_breakdown(sim: &SimState, g: Good) -> Breakdown {
    let rows = sim
        .ledger
        .in_by(g)
        .into_iter()
        .map(|(p, v)| match p {
            Producer::Building(b) => row(building_name(sim, b), v, building_source(sim, b)),
            Producer::Order => row("By order", v, Source::None),
            Producer::Other => row("Other", v, Source::None),
        })
        .collect();
    Breakdown::new(sim.ledger.in_per_day(g), rows)
}

fn use_label(u: Use) -> &'static str {
    match u {
        Use::EatenAdults => "Adults",
        Use::EatenYearlings => "Yearlings",
        Use::EatenKits => "Kits",
        Use::EatenElders => "Elders",
        Use::Spoiled => "Spoiled",
        Use::Sites => "Building sites",
        Use::Crafting => "Crafting",
        Use::Repairs => "Repairs",
        Use::Other => "Other",
    }
}

/// Each good's daily outflow (7-day mean) by use, as negative values: eating by life stage,
/// spoilage, building sites, crafting inputs, repairs.
pub fn good_out_breakdown(sim: &SimState, g: Good) -> Breakdown {
    let rows = Use::ALL
        .iter()
        .map(|&u| (u, sim.ledger.out_for(g, u)))
        .filter(|(_, v)| *v > 1e-4)
        .map(|(u, v)| row(use_label(u), -v, Source::None))
        .collect();
    Breakdown::new(-sim.ledger.out_per_day(g), rows)
}

/// Days the food lasts: total = days (`f32::INFINITY` when food is not falling); rows: the
/// stock, each stage's eating per day (negative), gathering per day.
pub fn food_days_breakdown(sim: &SimState) -> Breakdown {
    let stock = sim.buildings.stores.amount(Good::Food);
    let need = food_need_per_day(sim);
    let gain = sim.ledger.in_per_day(Good::Food);
    let mut rows = vec![row("Stock", stock, Source::None), row("Gathered", gain, Source::None)];
    for (st, label) in [(Stage::Adult, "Adults"), (Stage::Yearling, "Yearlings"), (Stage::Kit, "Kits"), (Stage::Elder, "Elders")] {
        let eat: f32 = sim.colony.beavers.iter().filter(|b| b.stage == st).map(|b| crate::tuning::colony::HUNGER_PER_DAY * b.stage.food_weight()).sum();
        if eat > 0.0 {
            rows.push(row(label, -eat, Source::None));
        }
    }
    let total = if need > gain { stock / (need - gain) } else { f32::INFINITY };
    Breakdown::new(total, rows)
}

/// The population: adults, yearlings, kits, elders, housed and unhoused, and this season's births,
/// arrivals and deaths. Total = population (the rows overlap; they do not sum to it).
pub fn headcount_breakdown(sim: &SimState) -> Breakdown {
    let c = &sim.colony;
    let mut rows = Vec::new();
    for (st, label) in [(Stage::Adult, "Adults"), (Stage::Yearling, "Yearlings"), (Stage::Kit, "Kits"), (Stage::Elder, "Elders")] {
        rows.push(row(label, c.count_stage(st) as f32, Source::None));
    }
    let housed = c.beavers.iter().filter(|b| b.lodge.is_some()).count();
    rows.push(row("Housed", housed as f32, Source::None));
    rows.push(row("Unhoused", (c.population() - housed) as f32, Source::None));
    let season_start = (sim.time_days / crate::calendar::SEASON_DAYS).floor() * crate::calendar::SEASON_DAYS;
    let (mut born, mut joined, mut died) = (0, 0, 0);
    for e in c.events.iter().filter(|e| e.day >= season_start) {
        match e.event {
            crate::colony::Event::Born { .. } => born += 1,
            crate::colony::Event::Immigrated { .. } => joined += 1,
            crate::colony::Event::Died { .. } | crate::colony::Event::Departed { .. } => died += 1,
            _ => {}
        }
    }
    rows.push(row("Born", born as f32, Source::None));
    rows.push(row("Joined", joined as f32, Source::None));
    rows.push(row("Lost", -(died as f32), Source::None));
    rows.retain(|r| r.value != 0.0);
    Breakdown::new(c.population() as f32, rows)
}

/// A beaver's mood: each need's share of the mood target (weight x need x the health factor),
/// plus how far the mood still lags behind that target. Rows sum to the mood. `None`-like empty
/// breakdown for an unknown beaver.
pub fn mood_breakdown(sim: &SimState, b: BeaverId) -> Breakdown {
    use crate::tuning::colony::*;
    let Some(bv) = sim.colony.beaver(b) else { return Breakdown::default() };
    let n = &bv.needs;
    let health = MOOD_HEALTH_BASE + MOOD_HEALTH_SCALE * bv.health;
    let terms = [
        ("Food", MOOD_FOOD_WEIGHT * n.food),
        ("Warmth", MOOD_WARMTH_WEIGHT * n.warmth),
        ("Rest", MOOD_REST_WEIGHT * n.rest),
        ("Safety", MOOD_SAFETY_WEIGHT * n.safety),
        ("Company", MOOD_SOCIAL_WEIGHT * n.social),
    ];
    let mut rows: Vec<BreakdownRow> = terms.iter().map(|(l, v)| row(*l, v * health, Source::Beaver(b))).collect();
    let target: f32 = rows.iter().map(|r| r.value).sum();
    let lag = bv.mood - target;
    if lag.abs() > 0.005 {
        rows.push(row(if lag > 0.0 { "Fading" } else { "Catching up" }, lag, Source::Beaver(b)));
    }
    Breakdown::new(bv.mood, rows)
}

/// A beaver's health: full health less the harm from each cause, plus what healed back. Rows sum
/// to the health.
pub fn health_breakdown(sim: &SimState, b: BeaverId) -> Breakdown {
    let Some(bv) = sim.colony.beaver(b) else { return Breakdown::default() };
    let mut rows = vec![row("Full health", 1.0, Source::Beaver(b))];
    let mut harmed = 0.0;
    for (cause, h) in bv.harm() {
        if h > 1e-4 {
            let label = match cause {
                DeathCause::Starvation => "Hunger",
                DeathCause::Cold => "Cold",
                DeathCause::Injury => "Injury",
                DeathCause::Exhaustion => "Exhaustion",
                DeathCause::OldAge => "Old age",
            };
            rows.push(row(label, -h, Source::Beaver(b)));
            harmed += h;
        }
    }
    let rest = bv.health - (1.0 - harmed);
    if rest.abs() > 1e-4 {
        rows.push(row(if rest > 0.0 { "Healed" } else { "Other harm" }, rest, Source::Beaver(b)));
    }
    Breakdown::new(bv.health, rows)
}

/// Days of summer cover per pond (its volume over its own summer evaporation and seepage).
/// Total = the colony's cover (`Ponds::summer_cover_days`, 0 without ponds). Rows link to a
/// cell of each pond.
pub fn pond_cover_breakdown(sim: &SimState) -> Breakdown {
    let w = &sim.water;
    let n = w.depth.width();
    let per_cell = w.config.evaporation * Season::Summer.evap_mult() + w.config.seepage;
    let pond = |i: usize| w.depth.as_slice()[i] >= WET_DEPTH && !w.fed.as_slice()[i];
    let mut seen = vec![false; w.depth.as_slice().len()];
    let mut rows = Vec::new();
    for start in 0..seen.len() {
        if seen[start] || !pond(start) {
            continue;
        }
        let (mut cells, mut volume, mut stack) = (0usize, 0.0f32, vec![start]);
        seen[start] = true;
        while let Some(i) = stack.pop() {
            cells += 1;
            volume += w.depth.as_slice()[i];
            let (x, y) = (i % n, i / n);
            for (nx, ny) in [(x.wrapping_sub(1), y), (x + 1, y), (x, y.wrapping_sub(1)), (x, y + 1)] {
                if nx < n && ny < n {
                    let j = ny * n + nx;
                    if !seen[j] && pond(j) {
                        seen[j] = true;
                        stack.push(j);
                    }
                }
            }
        }
        // Puddles of a cell or two are not ponds.
        if cells >= 4 && per_cell > 0.0 {
            rows.push((volume, row("", volume / (cells as f32 * per_cell), Source::Cell((start % n, start / n)))));
        }
    }
    rows.sort_by(|a, b| b.0.total_cmp(&a.0));
    let rows = rows.into_iter().enumerate().map(|(k, (_, mut r))| {
        r.label = format!("Pond {}", k + 1);
        r
    });
    Breakdown::new(ponds(sim).summer_cover_days().unwrap_or(0.0), rows.collect())
}

/// The dam cells of dam building `b`: its footprint's dam cells and every dam cell joined to them.
fn dam_cells(sim: &SimState, b: BuildingId) -> Vec<Cell> {
    let Some(bd) = sim.buildings.building(b).filter(|bd| bd.kind == BuildingKind::Dam) else { return Vec::new() };
    let w = &sim.water;
    let dam = |c: Cell| w.dam_height.get(c.0, c.1).is_some_and(|h| *h > 0.0);
    let mut out: Vec<Cell> = bd.cells().filter(|&c| dam(c)).collect();
    let mut k = 0;
    while k < out.len() {
        let (x, y) = out[k];
        for dy in -1i64..=1 {
            for dx in -1i64..=1 {
                let (nx, ny) = (x as i64 + dx, y as i64 + dy);
                if nx < 0 || ny < 0 {
                    continue;
                }
                let c = (nx as usize, ny as usize);
                if dam(c) && !out.contains(&c) {
                    out.push(c);
                }
            }
        }
        k += 1;
    }
    out
}

/// Strength of dam `b`: total = its weakest cell's integrity (0..=1); rows: what wears it per day
/// at that cell (age, water over the crest), negative. `None` if `b` is not a standing dam.
pub fn dam_strength_breakdown(sim: &SimState, b: BuildingId) -> Option<Breakdown> {
    let cells = dam_cells(sim, b);
    let w = &sim.water;
    let &weakest = cells.iter().min_by(|a, c| {
        let i = |c: &Cell| w.dam_integrity.get(c.0, c.1).copied().unwrap_or(0.0);
        i(a).total_cmp(&i(c))
    })?;
    Some(dam_wear(sim, weakest, Source::Building(b)))
}

fn dam_wear(sim: &SimState, c: Cell, source: Source) -> Breakdown {
    let w = &sim.water;
    let integrity = w.dam_integrity.get(c.0, c.1).copied().unwrap_or(0.0);
    let over = (w.depth.get(c.0, c.1).copied().unwrap_or(0.0) - w.config.overtop_depth).max(0.0);
    let mut rows = vec![row("Age", -w.config.dam_decay_per_day, source)];
    if over > 0.0 {
        rows.push(row("Overtopping", -w.config.overtop_erosion * over, source));
    }
    Breakdown::new(integrity, rows)
}

// ───────────────────────────── problems ─────────────────────────────

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    Urgent,
    Warning,
}

/// The one action a problem's fix button performs (spec W7). [`apply_fix`] performs the ones the
/// sim can; `Place`, `OpenJobs` and `Focus` are the interface's.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Fix {
    /// `n` beavers into this workplace (`Command::AddWorker` each).
    AddWorkers { building: BuildingId, n: usize },
    /// `n` beavers gather `kind` around the lodge (a direct `Assignment::Area` order).
    Gather { kind: ResourceKind, n: usize },
    /// Start placing this building.
    Place(BuildingKind),
    /// Fill every stalled site's slots (W2 rules).
    StaffSites,
    /// `n` beavers into `role` (`Command::AdjustRole`).
    PutToWork { role: RoleKind, n: usize },
    /// Open the Jobs board.
    OpenJobs,
    Accept(RequestId),
    /// Turn a stranger away.
    Decline(RequestId),
    /// Nothing to do but look: the jump button.
    Focus,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProblemKind {
    Food,
    Idle,
    NoBuilders,
    StoreFull,
    WeakDam,
    DamFailed,
    Stranger,
    /// A logging camp's area has fewer than `forestry::RUNNING_OUT` mature trees (value: how many).
    LoggingAreaRunningOut,
    /// Mature trees around the colony fell by more than `forestry::FALLING_SHARE` over the last
    /// `forestry::FALLING_DAYS` (value: percent lost).
    ForestFalling,
}

/// What a problem is about, for its icon.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Subject {
    Good(Good),
    Building(BuildingKind),
    /// A stranger's life stage.
    Stage(Stage),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Problem {
    pub kind: ProblemKind,
    pub severity: Severity,
    /// The alert's number: Food days left (floored), Idle beavers, NoBuilders sites, StoreFull
    /// percent full, WeakDam percent strength, DamFailed dams, Stranger 1.
    pub value: f32,
    /// What makes up the number (the tooltip).
    pub cause: Breakdown,
    pub subject: Option<Subject>,
    pub fix: Fix,
    /// A second button: `Decline` beside a stranger's `Accept`.
    pub alt_fix: Option<Fix>,
    /// Where the jump button takes the camera.
    pub focus: Option<Cell>,
}

fn building_cell(sim: &SimState, id: BuildingId) -> Option<Cell> {
    sim.buildings.building(id).map(|b| b.pos)
}

/// The colony's home: the first lodge, else the first beaver.
fn colony_cell(sim: &SimState) -> Option<Cell> {
    let p = sim.colony.lodges.first().map(|l| l.pos).or(sim.colony.beavers.first().map(|b| b.pos))?;
    Some((p.0.max(0.0) as usize, p.1.max(0.0) as usize))
}

/// Food the colony eats per day when everyone is fed (hunger rate times each stage's share).
pub fn food_need_per_day(sim: &SimState) -> f32 {
    sim.colony.beavers.iter().map(|b| crate::tuning::colony::HUNGER_PER_DAY * b.stage.food_weight()).sum()
}

/// Beavers a gather order may take: adults and yearlings not locked to a slot or order, free
/// ones first, then nearest the lodge.
fn gatherers(sim: &SimState, n: usize) -> Vec<BeaverId> {
    let home = colony_cell(sim).unwrap_or((0, 0));
    let mut v: Vec<(bool, usize, BeaverId)> = sim
        .colony
        .beavers
        .iter()
        .filter(|b| matches!(b.stage, Stage::Adult | Stage::Yearling) && !sim.work.is_locked(b.id))
        .filter_map(|b| sim.agents.get(b.id).map(|a| (crate::jobs::is_busy(sim, b.id), a.cell.0.abs_diff(home.0) + a.cell.1.abs_diff(home.1), b.id)))
        .collect();
    v.sort();
    v.into_iter().take(n).map(|t| t.2).collect()
}

/// The food fix (W7): staff a forager hut with free slots; else place one if unlocked (and none
/// is being built); else gather plants around the lodge; else just look.
fn food_fix(sim: &SimState) -> Fix {
    let hut = sim
        .work
        .places
        .iter()
        .filter(|p| p.kind == BuildingKind::ForagerHut && !p.paused && sim.buildings.building(p.building).is_some_and(|b| b.is_complete()))
        .filter(|p| crate::jobs::add_worker_preview(sim, p.building).is_some())
        .max_by_key(|p| (p.priority, std::cmp::Reverse(p.building)));
    if let Some(p) = hut {
        let free = p.slots.iter().filter(|s| s.worker.is_none()).count();
        return Fix::AddWorkers { building: p.building, n: free.min(FIX_WORKERS) };
    }
    let building_one = sim.buildings.buildings.iter().any(|b| b.kind == BuildingKind::ForagerHut && !b.is_complete());
    if sim.tech.allows(BuildingKind::ForagerHut) && !building_one {
        return Fix::Place(BuildingKind::ForagerHut);
    }
    match gatherers(sim, FIX_WORKERS).len() {
        0 => Fix::Focus,
        n => Fix::Gather { kind: ResourceKind::Plant, n },
    }
}

/// Current problems, urgent first, at most [`MAX_PROBLEMS`]. Each carries its number, what makes
/// it up and a fix.
pub fn problems(sim: &SimState) -> Vec<Problem> {
    let mut out = Vec::new();
    if !sim.colony.is_founded() {
        return out;
    }
    let idle = idle_beavers(sim);
    let pop = sim.colony.population();

    // Food: the stock the colony actually eats from, against what the colony needs per day.
    let food = sim.buildings.stores.amount(Good::Food);
    let need = food_need_per_day(sim);
    let gain = sim.ledger.in_per_day(Good::Food);
    if pop > 0 && need > gain {
        let days = food / (need - gain);
        if days < FOOD_WARN_DAYS {
            out.push(Problem {
                kind: ProblemKind::Food,
                severity: if days < FOOD_WARN_DAYS / 3.0 { Severity::Urgent } else { Severity::Warning },
                value: days.floor(),
                cause: food_days_breakdown(sim),
                subject: Some(Subject::Good(Good::Food)),
                fix: food_fix(sim),
                alt_fix: None,
                focus: colony_cell(sim),
            });
        }
    }

    // Dams that finished but could not be raised.
    if let Some((b, _)) = sim.work.dam_failures.last() {
        let rows = sim.work.dam_failures.iter().map(|(d, _)| row("Not raised", 1.0, building_source(sim, *d))).collect();
        out.push(Problem {
            kind: ProblemKind::DamFailed,
            severity: Severity::Urgent,
            value: sim.work.dam_failures.len() as f32,
            cause: Breakdown::new(sim.work.dam_failures.len() as f32, rows),
            subject: Some(Subject::Building(BuildingKind::Dam)),
            fix: Fix::Focus,
            alt_fix: None,
            focus: building_cell(sim, *b),
        });
    }

    // Weak dams: they breach at zero and release the pond.
    let n = sim.water.dam_height.width();
    let weak: Vec<Cell> = (0..n * n)
        .map(|i| (i % n, i / n))
        .filter(|&(x, y)| {
            sim.water.dam_height.get(x, y).is_some_and(|h| *h > 0.0) && sim.water.dam_integrity.get(x, y).is_some_and(|v| *v < WEAK_DAM)
        })
        .collect();
    if !weak.is_empty() {
        let integ = |c: &Cell| sim.water.dam_integrity.get(c.0, c.1).copied().unwrap_or(0.0);
        let worst = *weak.iter().min_by(|a, b| integ(a).total_cmp(&integ(b))).expect("non-empty");
        out.push(Problem {
            kind: ProblemKind::WeakDam,
            severity: if integ(&worst) < WEAK_DAM / 2.0 { Severity::Urgent } else { Severity::Warning },
            value: (integ(&worst) * 100.0).round(),
            cause: dam_wear(sim, worst, Source::Cell(worst)),
            subject: Some(Subject::Building(BuildingKind::Dam)),
            fix: Fix::Focus,
            alt_fix: None,
            focus: Some(worst),
        });
    }

    // Sites nobody is building.
    let stalled: Vec<BuildingId> = sim
        .work
        .places
        .iter()
        .filter(|p| crate::agents::is_site(sim, p) && !p.paused && p.slots.iter().all(|s| s.worker.is_none()))
        .map(|p| p.building)
        .collect();
    if let Some(&first) = stalled.first() {
        let can_staff = stalled.iter().any(|&b| crate::jobs::add_worker_preview(sim, b).is_some());
        let rows = stalled.iter().map(|&b| row(building_name(sim, b), sim.work.progress(b).unwrap_or(0.0), Source::Building(b))).collect();
        out.push(Problem {
            kind: ProblemKind::NoBuilders,
            severity: Severity::Warning,
            value: stalled.len() as f32,
            cause: Breakdown::new(stalled.len() as f32, rows),
            subject: Some(Subject::Building(sim.buildings.building(first).map_or(BuildingKind::Dam, |b| b.kind))),
            fix: if can_staff { Fix::StaffSites } else { Fix::Focus },
            alt_fix: None,
            focus: building_cell(sim, first),
        });
    }

    // Idle beavers (only when no card above already puts them to work).
    let busy_fix = |f: &Fix| matches!(f, Fix::AddWorkers { .. } | Fix::Gather { .. } | Fix::StaffSites);
    if !idle.is_empty() && !out.iter().any(|p| busy_fix(&p.fix)) {
        let reasons = crate::jobs::idle_reasons(sim);
        let rows = reasons
            .iter()
            .map(|&(id, r)| {
                let label = match r {
                    crate::jobs::IdleReason::NoOpenSlot => "No open slot",
                    crate::jobs::IdleReason::Unreachable => "Unreachable",
                    crate::jobs::IdleReason::Resting => "Resting",
                    crate::jobs::IdleReason::Paused => "Paused",
                    crate::jobs::IdleReason::AutoOff => "Auto off",
                };
                row(label, 1.0, Source::Beaver(id))
            })
            .collect();
        // The highest-priority role with an open slot someone can take.
        let open = sim.work.role_order.iter().find_map(|&role| {
            let slots: usize = sim
                .work
                .places
                .iter()
                .filter(|p| !p.paused && (crate::agents::is_site(sim, p) || sim.buildings.building(p.building).is_some_and(|b| b.is_complete())))
                .flat_map(|p| p.slots.iter())
                .filter(|s| s.worker.is_none() && crate::jobs::role_kind(s.role) == role)
                .count();
            (slots > 0 && crate::jobs::role_add_preview(sim, role).is_some()).then_some((role, slots))
        });
        out.push(Problem {
            kind: ProblemKind::Idle,
            severity: Severity::Warning,
            value: idle.len() as f32,
            cause: Breakdown::new(idle.len() as f32, rows),
            subject: None,
            fix: open.map_or(Fix::OpenJobs, |(role, slots)| Fix::PutToWork { role, n: slots.min(idle.len()) }),
            alt_fix: None,
            focus: sim.colony.beaver(idle[0]).map(|b| (b.pos.0.max(0.0) as usize, b.pos.1.max(0.0) as usize)),
        });
    }

    // Full stores: gatherers' loads are lost.
    for g in Good::ALL {
        let (have, cap) = (sim.buildings.stores.amount(g), sim.buildings.stores.capacity(g));
        let arriving = sim.ledger.in_per_day(g);
        if cap > 0.0 && have >= cap * FULL_SHARE && arriving > 0.0 {
            let rows = vec![row("Stored", have, Source::None), row("Room", cap, Source::None), row("Arriving", arriving, Source::None)];
            out.push(Problem {
                kind: ProblemKind::StoreFull,
                severity: Severity::Warning,
                value: (have / cap * 100.0).round(),
                cause: Breakdown::new((have / cap * 100.0).round(), rows),
                subject: Some(Subject::Good(g)),
                fix: if sim.tech.allows(BuildingKind::Warehouse) { Fix::Place(BuildingKind::Warehouse) } else { Fix::Focus },
                alt_fix: None,
                focus: colony_cell(sim),
            });
            break;
        }
    }

    // Wood running out: a logging camp's area nearly cut out, or the colony's forest shrinking.
    let low_camp = sim
        .work
        .places
        .iter()
        .filter(|p| p.kind == BuildingKind::LoggingCamp && !p.paused && sim.buildings.building(p.building).is_some_and(|b| b.is_complete()))
        .filter_map(|p| p.area.map(|a| (p.building, a, forest_report(sim, &a))))
        .find(|(_, _, r)| r.mature + r.old < crate::forestry::RUNNING_OUT);
    if let Some((camp, area, r)) = low_camp {
        let left = (r.mature + r.old) as f32;
        let rows = vec![
            row("Mature", r.mature as f32, Source::Building(camp)),
            row("Old", r.old as f32, Source::Building(camp)),
            row("Young", r.young as f32, Source::Building(camp)),
            row("Saplings", r.saplings as f32, Source::Building(camp)),
        ];
        out.push(Problem {
            kind: ProblemKind::LoggingAreaRunningOut,
            severity: Severity::Warning,
            value: left,
            cause: Breakdown::new(left, rows),
            subject: Some(Subject::Building(BuildingKind::LoggingCamp)),
            fix: if sim.tech.allows(BuildingKind::ForesterLodge) { Fix::Place(BuildingKind::ForesterLodge) } else { Fix::Focus },
            alt_fix: None,
            focus: Some(area.centre()),
        });
    }
    if let Some((then, now)) = crate::forestry::forest_trend(sim) {
        if then > 0 && (now as f32) < then as f32 * (1.0 - crate::forestry::FALLING_SHARE) {
            let lost = ((then - now) as f32 / then as f32 * 100.0).round();
            let rows = vec![row("Then", then as f32, Source::None), row("Now", now as f32, Source::None)];
            out.push(Problem {
                kind: ProblemKind::ForestFalling,
                severity: Severity::Warning,
                value: lost,
                cause: Breakdown::new(lost, rows),
                subject: Some(Subject::Building(BuildingKind::ForesterLodge)),
                fix: Fix::Focus,
                alt_fix: None,
                focus: colony_cell(sim),
            });
        }
    }

    // A stranger waiting for an answer.
    if let Some(r) = sim.society.pending().first() {
        let stage = if r.adult { Stage::Adult } else { Stage::Yearling };
        out.push(Problem {
            kind: ProblemKind::Stranger,
            severity: Severity::Warning,
            value: 1.0,
            cause: Breakdown::new(1.0, vec![row(if r.adult { "Adult" } else { "Young" }, 1.0, Source::Cell(r.at))]),
            subject: Some(Subject::Stage(stage)),
            fix: Fix::Accept(r.id),
            alt_fix: Some(Fix::Decline(r.id)),
            focus: Some(r.at),
        });
    }

    out.sort_by_key(|p| p.severity);
    out.truncate(MAX_PROBLEMS);
    out
}

/// Performs `fix` through `Command`s. `Place`, `OpenJobs` and `Focus` belong to the interface:
/// they return `Ok` and change nothing. Errors (with nothing or only part done) when the fix
/// cannot be carried out now, e.g. nobody is free.
pub fn apply_fix(sim: &mut SimState, fix: Fix) -> Result<(), String> {
    let err = |e: crate::SimError| e.to_string();
    match fix {
        Fix::AddWorkers { building, n } => {
            for k in 0..n.max(1) {
                if let Err(e) = sim.apply(Command::AddWorker { building }) {
                    return if k == 0 { Err(err(e)) } else { Ok(()) };
                }
            }
            Ok(())
        }
        Fix::Gather { kind, n } => {
            let who = gatherers(sim, n.max(1));
            let home = colony_cell(sim).ok_or("the colony has no home")?;
            if who.is_empty() {
                return Err("no beaver is free to gather".into());
            }
            let r = FOOD_GATHER_RADIUS;
            let area = crate::work::Area { min: (home.0.saturating_sub(r), home.1.saturating_sub(r)), max: (home.0 + r, home.1 + r) };
            sim.apply(Command::Assign { who, to: Some(Assignment::Area(kind, area)) }).map_err(err)
        }
        Fix::StaffSites => {
            // Every stalled site (no builder at all), filled up.
            let sites: Vec<BuildingId> = sim
                .work
                .places
                .iter()
                .filter(|p| crate::agents::is_site(sim, p) && !p.paused && p.filled() == 0)
                .map(|p| p.building)
                .collect();
            let mut placed = 0;
            for b in sites {
                while sim.work.place(b).is_some_and(|p| p.filled() < p.slots.len()) && sim.apply(Command::AddWorker { building: b }).is_ok() {
                    placed += 1;
                }
            }
            if placed == 0 { Err("no beaver is free to build".into()) } else { Ok(()) }
        }
        Fix::PutToWork { role, n } => sim.apply(Command::AdjustRole { role, delta: n.max(1) as i32 }).map_err(err),
        Fix::Accept(id) => sim.apply(Command::AnswerJoin { id, answer: crate::society::Answer::Accept }).map_err(err),
        Fix::Decline(id) => sim.apply(Command::AnswerJoin { id, answer: crate::society::Answer::TurnAway }).map_err(err),
        Fix::Place(_) | Fix::OpenJobs | Fix::Focus => Ok(()),
    }
}

// ───────────────────────────── building report ─────────────────────────────

/// One line of a building's "why this rate": a measured fact and whether it helps (`Some(true)`),
/// holds the building back (`Some(false)`), or is neutral (`None`).
#[derive(Clone, Debug, PartialEq)]
pub struct Factor {
    pub label: String,
    pub value: String,
    pub ok: Option<bool>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Report {
    /// Measured output per good, largest first (smoothed over `output::SMOOTH_DAYS`); empty when
    /// nothing has been made yet.
    pub measured: Vec<(Good, f32)>,
    /// Whether the building has produced long enough for the rates to be steady.
    pub steady: bool,
    /// The most the building could make per day with every slot filled and no walking.
    pub ceiling: Option<(Good, f32)>,
    pub factors: Vec<Factor>,
}

fn factor(label: impl Into<String>, value: impl Into<String>, ok: Option<bool>) -> Factor {
    Factor { label: label.into(), value: value.into(), ok }
}

/// What a finished workplace produces and every measurable thing that limits it. `None` for
/// unknown or unfinished buildings and for ones that produce nothing (stores, lodges, dams).
pub fn building_report(sim: &SimState, id: BuildingId) -> Option<Report> {
    let b = sim.buildings.building(id)?;
    if !b.is_complete() {
        return None;
    }
    let def = crate::catalogue::def(b.kind);
    let place = sim.work.place(id);
    let flow = sim.output.of(id);
    let measured = flow.map(|f| f.rates()).unwrap_or_default();
    let steady = flow.is_some_and(|f| f.age_days >= crate::output::SMOOTH_DAYS);
    let mut factors = Vec::new();
    let filled = place.map_or(0, |p| p.slots.iter().filter(|s| s.worker.is_some()).count());
    let total = place.map_or(def.slots.len(), |p| p.slots.len());
    let centre = {
        let (w, h) = b.kind.spec().footprint;
        (b.pos.0 + w / 2, b.pos.1 + h / 2)
    };
    let store_dist = |g: Good| {
        sim.buildings
            .buildings
            .iter()
            .filter(|s| s.is_complete() && crate::agents::accepts(s.kind, g))
            .map(|s| s.cells().map(|c| c.0.abs_diff(centre.0) + c.1.abs_diff(centre.1)).min().unwrap_or(usize::MAX))
            .min()
    };
    let collect = def.slots.iter().find_map(|r| if let Role::Collect(k) = r { Some(*k) } else { None });
    if let Some(kind) = collect {
        // A gathering camp: workers, what is left in its area, and the walk to a store.
        let kinds: Vec<ResourceKind> = if kind == ResourceKind::Tree { vec![ResourceKind::Tree, ResourceKind::Felled] } else { vec![kind] };
        let good = if kind == ResourceKind::Tree { Good::Logs } else { kind.yields().first().copied()? };
        factors.push(factor("Workers", format!("{filled} of {total}"), Some(filled == total)));
        if let Some(area) = place.and_then(|p| p.area) {
            let left = sim.resources.items.iter().filter(|r| kinds.contains(&r.kind) && area.contains(r.cell) && r.has_yield()).count();
            let what = match kind {
                ResourceKind::Tree | ResourceKind::Felled => "Trees in its area",
                ResourceKind::Plant => "Food plants in its area",
                ResourceKind::MudBank => "Mud banks in its area",
                ResourceKind::StoneOutcrop => "Stone outcrops in its area",
            };
            let value = if left == 0 { "none left".to_string() } else { left.to_string() };
            factors.push(factor(what, value, Some(left > 0)));
        }
        match store_dist(good) {
            Some(d) => factors.push(factor("Walk to the nearest store", format!("{d} cells"), Some(d <= 15))),
            None => factors.push(factor("Store for its goods", "none: loads go only to sites", Some(false))),
        }
        let ceiling = Some((good, total as f32 * crate::agents::GATHER_PER_DAY));
        return Some(Report { measured, steady, ceiling, factors });
    }
    if def.batches_per_day > 0.0 {
        // A workshop: crafters, recipe inputs on hand, room for the output, wheel power.
        factors.push(factor("Crafters", format!("{filled} of {total}"), Some(filled == total)));
        let Some(recipe) = b.recipe else {
            factors.push(factor("Recipe", "none chosen", Some(false)));
            return Some(Report { measured, steady, ceiling: None, factors });
        };
        let (inputs, (out, per)) = recipe.batch();
        for &(g, a) in inputs {
            let have = sim.buildings.stores.amount(g);
            factors.push(factor(format!("{g:?} in store (a batch needs {a})"), format!("{have:.0}"), Some(have >= a)));
        }
        let space = sim.buildings.stores.space(out);
        factors.push(factor(format!("Room for {out:?}"), format!("{space:.0}"), Some(space >= per)));
        let power = sim.buildings.power.min(1.0);
        let value = if power > 0.0 { format!("+{:.0}%, shared by workshops", power * 100.0) } else { "none".to_string() };
        factors.push(factor("Water wheel power", value, (power > 0.0).then_some(true)));
        let ceiling = Some((out, def.batches_per_day * per * (1.0 + power) * sim.tech.craft_speed_factor()));
        return Some(Report { measured, steady, ceiling, factors });
    }
    None
}

// ───────────────────────────── river profile ─────────────────────────────

/// One river cell seen from the side.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ProfilePoint {
    pub cell: Cell,
    /// Terrain height.
    pub bed: f32,
    /// Top of the dam on this cell; equals `bed` where there is none.
    pub crest: f32,
    /// Water surface (the crest where the cell is dry).
    pub surface: f32,
}

/// The main river from its source to its outlet, one point per cell.
pub fn river_profile(sim: &SimState) -> Vec<ProfilePoint> {
    let w = &sim.world;
    let n = w.size;
    let Some(mut cur) = (0..n * n)
        .map(|i| (i % n, i / n))
        .filter(|&(x, y)| w.main_river.get(x, y).copied().unwrap_or(false))
        .min_by_key(|&(x, y)| w.flow_acc.get(x, y).copied().unwrap_or(u32::MAX))
    else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for _ in 0..n * 4 {
        out.push(profile_point(sim, cur));
        match w.downstream(cur.0, cur.1) {
            Some(next) if !out.iter().any(|p: &ProfilePoint| p.cell == next) => cur = next,
            _ => break,
        }
    }
    out
}

/// The river through `near` (the river cell closest to it, within 12 cells): up to `up` cells
/// upstream (always following the larger branch) and `down` cells downstream, in flow order.
/// Returns the points and the index of the anchor cell, or `None` with no river nearby.
pub fn river_profile_through(sim: &SimState, near: (f32, f32), up: usize, down: usize) -> Option<(Vec<ProfilePoint>, usize)> {
    let w = &sim.world;
    let n = w.size as i64;
    let (cx, cy) = (near.0.round() as i64, near.1.round() as i64);
    let mut best: Option<(i64, Cell)> = None;
    for dy in -12..=12i64 {
        for dx in -12..=12i64 {
            let (x, y) = (cx + dx, cy + dy);
            if x < 0 || y < 0 || x >= n || y >= n || !w.river.get(x as usize, y as usize).copied().unwrap_or(false) {
                continue;
            }
            let d = dx * dx + dy * dy;
            if best.is_none_or(|(b, _)| d < b) {
                best = Some((d, (x as usize, y as usize)));
            }
        }
    }
    let (_, anchor) = best?;
    // Upstream: the river neighbour draining into the current cell with the largest catchment.
    let mut ups = Vec::new();
    let mut cur = anchor;
    for _ in 0..up {
        let mut next: Option<(u32, Cell)> = None;
        for (dx, dy) in [(-1i64, -1i64), (0, -1), (1, -1), (-1, 0), (1, 0), (-1, 1), (0, 1), (1, 1)] {
            let (x, y) = (cur.0 as i64 + dx, cur.1 as i64 + dy);
            if x < 0 || y < 0 || x >= n || y >= n {
                continue;
            }
            let c = (x as usize, y as usize);
            if w.river.get(c.0, c.1).copied().unwrap_or(false) && w.downstream(c.0, c.1) == Some(cur) {
                let acc = w.flow_acc.get(c.0, c.1).copied().unwrap_or(0);
                if next.is_none_or(|(b, _)| acc > b) {
                    next = Some((acc, c));
                }
            }
        }
        match next {
            Some((_, c)) => {
                ups.push(c);
                cur = c;
            }
            None => break,
        }
    }
    ups.reverse();
    let at = ups.len();
    let mut cells = ups;
    cells.push(anchor);
    let mut cur = anchor;
    for _ in 0..down {
        match w.downstream(cur.0, cur.1) {
            Some(c) if !cells.contains(&c) => {
                cells.push(c);
                cur = c;
            }
            _ => break,
        }
    }
    Some((cells.into_iter().map(|c| profile_point(sim, c)).collect(), at))
}

fn profile_point(sim: &SimState, (x, y): Cell) -> ProfilePoint {
    let bed = sim.height.get(x, y).copied().unwrap_or(0.0);
    let crest = bed + sim.water.dam_height.get(x, y).copied().unwrap_or(0.0);
    let surface = crest + sim.water.depth.get(x, y).copied().unwrap_or(0.0);
    ProfilePoint { cell: (x, y), bed, crest, surface }
}

/// The surface each profile point would have with the previewed dam: its pond level where the
/// pond reaches, unchanged elsewhere.
pub fn profile_with_preview(points: &[ProfilePoint], preview: &crate::dam_preview::DamPreview) -> Vec<f32> {
    points
        .iter()
        .map(|p| if preview.flooded.binary_search_by_key(&(p.cell.1, p.cell.0), |c| (c.1, c.0)).is_ok() { p.surface.max(preview.level) } else { p.surface })
        .collect()
}

// ───────────────────────────── ponds and the dry season ─────────────────────────────

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Ponds {
    /// Whether any dam stands (without one, standing water is just puddles and hollows).
    pub dammed: bool,
    /// Cells holding standing water that the river and lakes do not keep level (dammed ponds).
    pub cells: usize,
    pub volume: f32,
    /// Water the ponds lose per day in summer (evaporation and seepage).
    pub summer_loss_per_day: f32,
}

impl Ponds {
    /// Days the ponds would last through summer with no inflow; `None` without ponds.
    pub fn summer_cover_days(&self) -> Option<f32> {
        (self.dammed && self.cells > 0 && self.summer_loss_per_day > 0.0).then(|| self.volume / self.summer_loss_per_day)
    }
}

pub fn ponds(sim: &SimState) -> Ponds {
    let w = &sim.water;
    let (mut cells, mut volume) = (0, 0.0);
    for (i, &d) in w.depth.as_slice().iter().enumerate() {
        if d >= WET_DEPTH && !w.fed.as_slice()[i] {
            cells += 1;
            volume += d;
        }
    }
    let per_cell = w.config.evaporation * Season::Summer.evap_mult() + w.config.seepage;
    let dammed = w.dam_height.as_slice().iter().any(|h| *h > 0.0);
    Ponds { dammed, cells, volume, summer_loss_per_day: cells as f32 * per_cell }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dam_preview::DamPreview;
    use crate::jobs::fixtures::{colony, workplace};
    use crate::work::Priority;

    #[test]
    fn the_food_fix_staffs_a_hut_else_places_one_else_gathers() {
        let (mut s, _) = colony();
        let food = |s: &SimState| problems(s).into_iter().find(|p| p.kind == ProblemKind::Food).expect("no food in store");
        let p = food(&s);
        assert_eq!((p.value, p.subject, p.fix), (0.0, Some(Subject::Good(Good::Food)), Fix::Gather { kind: ResourceKind::Plant, n: 2 }));
        assert!(p.cause.rows.iter().any(|r| r.label == "Adults" && r.value < 0.0));
        s.tech.record(crate::tech::Achievement::FirstBurrow);
        s.tech.update(&s.colony);
        assert_eq!(food(&s).fix, Fix::Place(BuildingKind::ForagerHut), "the hut is unlocked");
        let hut = workplace(&mut s, 50, BuildingKind::ForagerHut, (3, 3), Priority::Normal);
        assert_eq!(food(&s).fix, Fix::AddWorkers { building: hut, n: 2 });
        let fix = food(&s).fix;
        apply_fix(&mut s, fix).unwrap();
        assert_eq!(s.work.place(hut).unwrap().filled(), 2);
        // A gather fix sends the beavers left on a locked order around the lodge.
        apply_fix(&mut s, Fix::Gather { kind: ResourceKind::Plant, n: 2 }).unwrap();
        let ordered: Vec<_> = s.colony.beavers.iter().filter(|b| s.work.player_orders.contains(&b.id)).collect();
        assert_eq!(ordered.len(), 2);
        assert!(ordered.iter().all(|b| matches!(s.agents.get(b.id).unwrap().assignment, Some(Assignment::Area(ResourceKind::Plant, _)))));
        assert_eq!(apply_fix(&mut s, Fix::Gather { kind: ResourceKind::Plant, n: 2 }), Err("no beaver is free to gather".into()));
        assert_eq!(apply_fix(&mut s, Fix::OpenJobs), Ok(()), "interface fixes change nothing");
    }

    #[test]
    fn stalled_sites_and_idle_beavers_get_work_fixes_and_strangers_can_be_turned_away() {
        let (mut s, _) = colony();
        s.buildings.stores.deposit(Good::Food, 90.0).unwrap();
        let hut = workplace(&mut s, 50, BuildingKind::ForagerHut, (3, 3), Priority::Normal);
        let first = problems(&s);
        assert_eq!((first[0].kind, first[0].value, first[0].fix), (ProblemKind::Idle, 4.0, Fix::PutToWork { role: RoleKind::Forager, n: 2 }));
        apply_fix(&mut s, first[0].fix).unwrap();
        assert_eq!(s.work.place(hut).unwrap().filled(), 2);
        // A site nobody builds.
        let site = BuildingId(60);
        let pos = crate::jobs::fixtures::near(&s, 0, 6);
        s.buildings.buildings.push(crate::buildings::Building { id: site, kind: BuildingKind::Burrow, pos, status: crate::buildings::Status::Site, recipe: None, output: 0.0 });
        s.work.add_site(site, BuildingKind::Burrow, &[], 6.0).unwrap();
        let p = problems(&s).into_iter().find(|p| p.kind == ProblemKind::NoBuilders).unwrap();
        assert_eq!((p.value, p.fix), (1.0, Fix::StaffSites));
        assert!(!problems(&s).iter().any(|p| p.kind == ProblemKind::Idle), "the site card already puts them to work");
        apply_fix(&mut s, Fix::StaffSites).unwrap();
        assert_eq!(s.work.place(site).unwrap().filled(), 2);
        // A stranger: accept or turn away.
        let id = s.society.raise(&s.colony, true, 0.0).unwrap();
        let p = problems(&s).into_iter().find(|p| p.kind == ProblemKind::Stranger).unwrap();
        assert_eq!((p.fix, p.alt_fix, p.subject), (Fix::Accept(id), Some(Fix::Decline(id)), Some(Subject::Stage(Stage::Adult))));
        let n = s.colony.population();
        apply_fix(&mut s, Fix::Decline(id)).unwrap();
        assert!(s.society.pending().is_empty() && s.colony.population() == n);
    }

    #[test]
    fn breakdown_rows_add_up_to_their_number() {
        let (mut s, ids) = colony();
        let sum = |b: &Breakdown| b.rows.iter().map(|r| r.value).sum::<f32>();
        s.colony.damage_health(0.25);
        for b in [mood_breakdown(&s, ids[0]), health_breakdown(&s, ids[0])] {
            assert!((sum(&b) - b.total).abs() < 1e-4, "{b:?}");
            assert!(b.rows.iter().all(|r| r.label.split(' ').count() <= 3));
            assert!(b.rows.windows(2).all(|w| w[0].value.abs() >= w[1].value.abs()), "sorted by size");
        }
        assert_eq!(health_breakdown(&s, ids[0]).total, 0.75);
        assert_eq!(mood_breakdown(&s, BeaverId(999)), Breakdown::default());
        let hut = workplace(&mut s, 50, BuildingKind::ForagerHut, (3, 3), Priority::Normal);
        s.ledger.record(0.0, &s.buildings.stores);
        s.buildings.stores.deposit_by(Good::Food, 6.0, Producer::Building(hut)).unwrap();
        s.buildings.stores.withdraw_for(Good::Food, 1.0, Use::EatenAdults).unwrap();
        s.ledger.record(1.0, &s.buildings.stores);
        let inflow = good_in_breakdown(&s, Good::Food);
        assert_eq!((inflow.total, inflow.rows[0].label.as_str(), inflow.rows[0].source), (6.0, "Forager hut", Source::Building(hut)));
        let out = good_out_breakdown(&s, Good::Food);
        assert_eq!((out.total, out.rows[0].label.as_str()), (-1.0, "Adults"));
        let h = headcount_breakdown(&s);
        assert_eq!((h.total, h.rows[0].label.as_str(), h.rows[0].value), (4.0, "Adults", 4.0));
        assert!(dam_strength_breakdown(&s, hut).is_none(), "not a dam");
        assert_eq!(pond_cover_breakdown(&s).total, 0.0, "no dam, no reservoir");
    }

    /// Places `kind` on the nearest valid, reachable cell around the lodge (the sim's own check).
    fn place_near_lodge(s: &mut SimState, kind: BuildingKind) -> bool {
        let c = colony_cell(s).unwrap();
        let walker = s.colony.beavers.iter().find(|b| b.stage == Stage::Adult).map(|b| b.id).unwrap();
        let from = s.agents.get(walker).unwrap().cell;
        for r in 1..25usize {
            for y in c.1.saturating_sub(r)..=c.1 + r {
                for x in c.0.saturating_sub(r)..=c.0 + r {
                    let ring = x.abs_diff(c.0).max(y.abs_diff(c.1)) == r;
                    if ring
                        && s.check_site(kind, (x, y)).is_ok()
                        && s.nav.find_path(walker, from, crate::nav::Goal::Beside((x, y))).is_some()
                        && s.place_site(kind, (x, y)).is_ok()
                    {
                        return true;
                    }
                }
            }
        }
        false
    }

    /// Spec W8 on a real map's standard start: auto-staffing on, and each day only the first
    /// problem's fix (a `Place` placed near the lodge). The family reaches day 30 without
    /// starving and food comes in beyond the larder. One map here (Telemark, the densest forest
    /// start; about 2 s); the other three maps pass the same run.
    #[test]
    fn w8_first_fixes_feed_the_family_through_the_first_month() {
        let (mut s, dam) = crate::starts::new_game_on_map("telemark").unwrap();
        s.apply(Command::Settle { cell: dam.camp }).unwrap();
        assert!(s.work.auto_organise, "auto-staffing is on in a new game");
        let larder = s.buildings.stores.amount(Good::Food);
        let mut next = 0.0;
        while s.time_days < 30.0 {
            if s.time_days >= next {
                next += 1.0;
                if let Some(p) = problems(&s).first() {
                    match p.fix {
                        Fix::Place(kind) => {
                            place_near_lodge(&mut s, kind);
                        }
                        fix => {
                            let _ = apply_fix(&mut s, fix);
                        }
                    }
                }
            }
            s.step(0.05).unwrap();
        }
        let starved = s.colony.events.iter().any(|e| matches!(e.event, crate::colony::Event::Died { cause: DeathCause::Starvation, .. }));
        assert!(!starved, "a beaver starved in the first month");
        let gathered = s.buildings.stores.produced_total[Good::Food.index()] - larder;
        assert!(gathered > 5.0, "the food fix brought in only {gathered}");
        assert!(s.buildings.stores.amount(Good::Food) > 5.0, "the larder is nearly empty on day 30");
    }

    fn pt(x: usize, surface: f32) -> ProfilePoint {
        ProfilePoint { cell: (x, 0), bed: 0.0, crest: 0.0, surface }
    }

    #[test]
    fn preview_raises_only_flooded_points() {
        let pts = [pt(0, 1.0), pt(1, 1.0), pt(2, 3.0)];
        let p = DamPreview {
            level: 2.0,
            rise: 1.0,
            flooded: vec![(0, 0), (2, 0)],
            flooded_trees: 0,
            flooded_buildings: vec![],
            cost: vec![],
            pressure: 0.0,
            fill_days: 0.0,
        };
        assert_eq!(profile_with_preview(&pts, &p), vec![2.0, 1.0, 3.0]);
    }

    #[test]
    fn unfounded_colony_has_no_problems_and_a_profile() {
        let s = SimState::new(5);
        assert!(problems(&s).is_empty());
        let prof = river_profile(&s);
        assert!(prof.len() > 10, "main river profile has {} points", prof.len());
        assert!(prof.first().unwrap().bed >= prof.last().unwrap().bed, "the river runs downhill overall");
    }

    #[test]
    fn profile_through_a_river_cell_runs_downstream_and_holds_the_anchor() {
        let s = SimState::new(5);
        let main = river_profile(&s);
        let c = main[main.len() / 2].cell;
        let (pts, at) = river_profile_through(&s, (c.0 as f32, c.1 as f32), 20, 10).expect("a river is right there");
        assert_eq!(pts[at].cell, c);
        assert!(pts.len() > 10 && at <= 20 && pts.len() - at - 1 <= 10);
        for pair in pts.windows(2) {
            assert_eq!(s.world.downstream(pair[0].cell.0, pair[0].cell.1), Some(pair[1].cell), "points follow the flow");
        }
    }

    #[test]
    fn a_staffed_camp_reports_measured_output_and_its_limits() {
        use crate::buildings::BuildingKind;
        let mut s = SimState::new(6);
        let c = s.world.valleys[s.world.start.0 as usize].center;
        let land = crate::WorldLand { world: &s.world, depth: &s.water.depth };
        s.colony.found((c.0, c.1), &land).unwrap();
        crate::agents::place_beavers(&mut s);
        s.tech.record(crate::tech::Achievement::FirstDam);
        s.tech.update(&s.colony);
        let (cx, cy) = (c.0 as usize, c.1 as usize);
        let id = (2..30usize)
            .flat_map(|r| (cx.saturating_sub(r)..=cx + r).flat_map(move |x| (cy.saturating_sub(r)..=cy + r).map(move |y| (x, y))))
            .find_map(|p| s.place_site(BuildingKind::LoggingCamp, p).ok())
            .expect("room for a logging camp");
        assert!(building_report(&s, id).is_none(), "a site produces nothing yet");
        // Finish it by hand and staff one slot.
        s.buildings.buildings.iter_mut().find(|b| b.id == id).unwrap().status = crate::buildings::Status::Complete;
        let area = crate::catalogue::work_area(s.buildings.building(id).unwrap());
        let slots: Vec<Role> = crate::catalogue::def(BuildingKind::LoggingCamp).slots;
        s.work.finish_site(id, &slots, area).unwrap();
        let adult = s.colony.beavers.iter().find(|b| b.stage == Stage::Adult).unwrap().id;
        s.work.assign(id, 0, adult).unwrap();
        s.output.record(id, Good::Logs, 3.0);
        s.output.step(1.0);
        let r = building_report(&s, id).expect("a camp has a report");
        assert_eq!(r.measured.first().map(|m| m.0), Some(Good::Logs));
        assert_eq!(r.factors[0], factor("Workers", "1 of 2", Some(false)));
        assert!(r.factors.iter().any(|f| f.label == "Trees in its area"));
        assert!(r.factors.iter().any(|f| f.label.starts_with("Store") && f.ok == Some(false)), "no store yet: {:?}", r.factors);
        assert_eq!(r.ceiling, Some((Good::Logs, 2.0 * crate::agents::GATHER_PER_DAY)));
    }

    #[test]
    fn pond_cover_needs_ponds() {
        let p = Ponds { dammed: true, cells: 0, volume: 0.0, summer_loss_per_day: 0.0 };
        assert_eq!(p.summer_cover_days(), None);
        let p = Ponds { dammed: false, cells: 10, volume: 50.0, summer_loss_per_day: 2.0 };
        assert_eq!(p.summer_cover_days(), None, "puddles without a dam are not a reservoir");
        let p = Ponds { dammed: true, cells: 10, volume: 50.0, summer_loss_per_day: 2.0 };
        assert_eq!(p.summer_cover_days(), Some(25.0));
    }
}
