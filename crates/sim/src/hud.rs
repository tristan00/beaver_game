use crate::agents::{Assignment, Task};
use crate::buildings::{BuildingId, BuildingKind, Good, Producer, Use};
use crate::colony::{BeaverId, Stage};
use crate::jobs::Job;
use crate::resources::ResourceKind;
use crate::society::RequestId;
use crate::water::Cell;
use crate::{Command, SimState};

pub const MAX_PROBLEMS: usize = 4;
pub const FOOD_WARN_DAYS: f32 = 20.0;
pub const FULL_SHARE: f32 = 0.98;
pub const WEAK_DAM: f32 = 0.6;
pub const FIX_BEAVERS: usize = 2;
pub const FOOD_GATHER_RADIUS: usize = 80;
/// Food for one winter: a whole calendar season.
pub const WINTER_RESERVE_DAYS: f32 = crate::calendar::SEASON_DAYS as f32;

pub fn idle_beavers(sim: &SimState) -> Vec<BeaverId> {
    sim.colony
        .beavers
        .iter()
        .filter(|b| b.can_work())
        .filter(|b| sim.work.slot_of(b.id).is_none())
        .filter(|b| sim.agents.get(b.id).is_none_or(|a| a.assignment.is_none() && matches!(a.task, Task::Idle | Task::Rest)))
        .map(|b| b.id)
        .collect()
}

#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum Source {
    Building(BuildingId),
    Beaver(BeaverId),
    Cell(Cell),
    None,
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct BreakdownRow {
    pub label: String,
    pub value: f32,
    pub source: Source,
}

#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
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
    sim.buildings.building(b).map_or("Gone", |b| b.kind.spec().name.as_str()).to_string()
}

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
        Use::Spoiled => "Spoiled",
        Use::Sites => "Building sites",
        Use::Crafting => "Crafting",
        Use::Repairs => "Repairs",
        Use::Other => "Other",
    }
}

pub fn good_out_breakdown(sim: &SimState, g: Good) -> Breakdown {
    let rows = Use::ALL
        .iter()
        .map(|&u| (u, sim.ledger.out_for(g, u)))
        .filter(|(_, v)| *v > 1e-4)
        .map(|(u, v)| row(use_label(u), -v, Source::None))
        .collect();
    Breakdown::new(-sim.ledger.out_per_day(g), rows)
}

pub fn food_days_breakdown(sim: &SimState) -> Breakdown {
    let stock = sim.buildings.stores.amount(Good::food());
    let need = food_need_per_day(sim);
    let gain = sim.ledger.in_per_day(Good::food());
    let mut rows = vec![row("Stock", stock, Source::None), row("Gathered", gain, Source::None)];
    for (st, label) in [(Stage::Adult, "Adults"), (Stage::Yearling, "Yearlings"), (Stage::Kit, "Kits")] {
        let eat: f32 = sim.colony.beavers.iter().filter(|b| b.stage == st).map(|b| crate::tuning::colony::HUNGER_PER_DAY * b.stage.food_weight()).sum();
        if eat > 0.0 {
            rows.push(row(label, -eat, Source::None));
        }
    }
    let spoilage=stock*crate::tuning::colony::FOOD_SPOILAGE;
    if spoilage>0.0 { rows.push(row("Spoilage now",-spoilage,Source::None)); }
    let total = food_runout_days(stock,need,gain,crate::tuning::colony::FOOD_SPOILAGE);
    Breakdown::new(total, rows)
}

/// Constant gathering and appetite, with spoilage proportional to remaining stock.
pub fn food_runout_days(stock:f32,need:f32,gain:f32,spoilage:f32)->f32 {
    let deficit=need-gain;
    if deficit<=0.0 {return f32::INFINITY;}
    if stock<=0.0 {return 0.0;}
    if spoilage<=0.0 {return stock/deficit;}
    (stock*spoilage/deficit).ln_1p()/spoilage
}

pub fn food_days_left(sim:&SimState)->f32 {
    food_runout_days(sim.buildings.stores.amount(Good::food()),food_need_per_day(sim),
        sim.ledger.in_per_day(Good::food()),crate::tuning::colony::FOOD_SPOILAGE)
}

pub fn headcount_breakdown(sim: &SimState) -> Breakdown {
    let c = &sim.colony;
    let mut rows = Vec::new();
    for (st, label) in [(Stage::Adult, "Adults"), (Stage::Yearling, "Yearlings"), (Stage::Kit, "Kits")] {
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

fn dam_wear(sim: &SimState, c: Cell, source: Source) -> Breakdown {
    let w = &sim.water;
    let integrity = w.dam_integrity.get(c.0, c.1).copied().unwrap_or(0.0);
    let (age, over) = w.dam_cell_wear(&sim.height, c.1 * sim.height.width() + c.0);
    let mut rows = vec![row("Age", -age, source)];
    if over > 0.0 {
        rows.push(row("Overtopping", -over, source));
    }
    Breakdown::new(integrity, rows)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
pub enum Severity {
    Urgent,
    Warning,
}

#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum Fix {
    AddBeavers { building: BuildingId, n: usize },
    Gather { kind: ResourceKind, n: usize },
    Place(BuildingKind),
    StaffSites,
    /// Open this dam's (or canal's) panel.
    OpenDam { dam: BuildingId },
    RepairBuilding { building: BuildingId },
    PutToWork { role: Job, n: usize },
    OpenJobs,
    Accept(RequestId),
    Decline(RequestId),
    Focus,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ProblemKind {
    Food,
    WinterFood,
    Idle,
    NoBuilders,
    NoStorage,
    StoreFull,
    WeakDam,
    Maintenance,
    DamFailed,
    Stranger,
    FellingAreaRunningOut,
    ForestFalling,
    HighAndDry,
    /// A building stands in water deeper than beavers build for.
    Flooded,
    /// A dug canal has held no water deep enough to float a load for days.
    CanalDry,
    /// A canal's cells are silting up with nobody keeping it.
    CanalSilting,
    /// A pond is falling fast through an open gate.
    PondDraining,
    /// A wolf the colony has seen is on the map.
    Wolf,
    /// Every beaver in one job is waiting at home with nothing that job can do.
    NoWork,
    /// A digging job idle because the ground is frozen hard (see `weather`).
    GroundFrozen,
    /// Every builder waiting at a site that needs a good nobody has or is bringing (the subject).
    WaitingFor,
    /// No lodge stands empty, and a beaver needs one: one without a home, or one grown and
    /// ready to leave its parents' lodge.
    NoFreeLodge,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Subject {
    Good(Good),
    Building(BuildingKind),
    Stage(Stage),
    Role(Job),
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Problem {
    pub kind: ProblemKind,
    pub severity: Severity,
    pub value: f32,
    pub cause: Breakdown,
    pub subject: Option<Subject>,
    pub fix: Fix,
    pub alt_fix: Option<Fix>,
    pub focus: Option<Cell>,
}

fn building_cell(sim: &SimState, id: BuildingId) -> Option<Cell> {
    sim.buildings.building(id).map(|b| b.pos)
}

fn colony_cell(sim: &SimState) -> Option<Cell> {
    let p = sim.colony.lodges.first().map(|l| l.pos).or(sim.colony.beavers.first().map(|b| b.pos))?;
    Some((p.0.max(0.0) as usize, p.1.max(0.0) as usize))
}

pub fn food_need_per_day(sim: &SimState) -> f32 {
    sim.colony.beavers.iter().map(|b| crate::tuning::colony::HUNGER_PER_DAY * b.stage.food_weight()).sum()
}

fn gatherers(sim: &SimState, n: usize) -> Vec<BeaverId> {
    let home = colony_cell(sim).unwrap_or((0, 0));
    let mut v: Vec<(bool, usize, BeaverId)> = sim
        .colony
        .beavers
        .iter()
        .filter(|b| b.can_work())
        .filter_map(|b| sim.agents.get(b.id).map(|a| (crate::jobs::is_busy(sim, b.id), a.cell.0.abs_diff(home.0) + a.cell.1.abs_diff(home.1), b.id)))
        .collect();
    v.sort();
    v.into_iter().take(n).map(|t| t.2).collect()
}

fn food_fix(sim: &SimState) -> Fix {
    // Extra staff in an exhausted work area cannot resolve a food shortage.
    // Send available beavers to food in the wider emergency gathering area.
    // A fixed pair is too small to respond to a mature colony's winter deficit.
    // The player explicitly moves a quarter of the workforce.
    let beavers=sim.colony.beavers.iter().filter(|b|b.can_work()).count();
    let n = gatherers(sim, beavers.div_ceil(4).max(FIX_BEAVERS)).len();
    if let Some(home) = colony_cell(sim).filter(|_|n>0) {
        let r=FOOD_GATHER_RADIUS;
        let area=crate::work::Area{min:(home.0.saturating_sub(r),home.1.saturating_sub(r)),max:(home.0+r,home.1+r)};
        if crate::agents::gathering_available(sim,Assignment::Area(ResourceKind::food_plant(false),area)) {
            return Fix::Gather {kind:ResourceKind::food_plant(false),n};
        }
    }
    Fix::Focus
}

fn winter_food_problem(sim:&SimState)->Option<Problem> {
    if crate::calendar::Season::at(sim.time_days)!=crate::calendar::Season::Autumn {return None;}
    let need=food_need_per_day(sim);
    let stock=sim.buildings.stores.amount(Good::food());
    let target=need*WINTER_RESERVE_DAYS;
    if need<=0.0 || stock>=target {return None;}
    let capacity=sim.buildings.stores.capacity(Good::food());
    let gathering=food_fix(sim);
    let store=BuildingKind::store_for(Good::food());
    let building_cache=sim.buildings.buildings.iter().any(|b|Some(b.kind)==store&&!b.is_complete());
    let add_cache=capacity<target && !building_cache && store.is_some();
    Some(Problem {
        kind:ProblemKind::WinterFood,severity:Severity::Warning,value:stock/need,
        cause:Breakdown::new(stock/need,vec![row("Stored food",stock,Source::None),row("Storage capacity",capacity,Source::None),row("Winter reserve goal",target,Source::None),row("Daily appetite",need,Source::None)]),
        subject:Some(Subject::Good(Good::food())),
        fix:match store {Some(k) if add_cache => place_fix(sim,k), _ => gathering},
        alt_fix:if add_cache&&gathering!=Fix::Focus {Some(gathering)} else {None},focus:colony_cell(sim),
    })
}

/// A beaver in a job, fit to work and not tired, waiting at home because its job has nothing
/// for it (what its card shows as 'No work').
fn waiting_for_work(sim: &SimState, id: BeaverId) -> bool {
    crate::jobs::waiting(sim, id).is_some()
}

/// The good a gathering role brings in.
pub(crate) fn gathered_good(role: crate::jobs::Job) -> Option<Good> {
    crate::jobs::gather_kind(role)?.good()
}

fn store_full(sim: &SimState, g: Good) -> bool {
    let cap = sim.buildings.stores.capacity(g);
    cap > 0.0 && sim.buildings.stores.amount(g) >= cap * FULL_SHARE
}

/// Whether anyone in a job gathering `g` is waiting for want of room.
fn held_back_by_room(sim: &SimState, g: Good) -> bool {
    sim.colony.beavers.iter().any(|b| crate::jobs::role_of(sim, b.id).and_then(gathered_good) == Some(g) && waiting_for_work(sim, b.id))
}

fn good_word(g: Good) -> String {
    g.name().to_lowercase()
}

/// One alert for each job whose beavers are all waiting with nothing to do. Gatherers held
/// back by a full store are left to the full-store alert.
fn no_work_problems(sim: &SimState, out: &mut Vec<Problem>) {
    for role in crate::jobs::Job::by_priority() {
        if gathered_good(role).is_some_and(|g| store_full(sim, g)) {
            continue;
        }
        // Those in the job (a seat at a building or an order), fit to work and not tired.
        let fit: Vec<&crate::colony::Beaver> = sim
            .colony
            .beavers
            .iter()
            .filter(|b| b.can_work() && !b.resting && crate::jobs::role_of(sim, b.id) == Some(role))
            .collect();
        if fit.is_empty() || !fit.iter().all(|b| waiting_for_work(sim, b.id)) {
            continue;
        }
        // Builders at sites short of a good nobody has: that good is what they wait for.
        if role == Job::with_duty(crate::content::Duty::Build) {
            let waits: Vec<Option<Good>> = fit
                .iter()
                .map(|b| match crate::jobs::waiting(sim, b.id) { Some(crate::jobs::Wait::Good(g)) => Some(g), _ => None })
                .collect();
            let most = Good::all().map(|g| (waits.iter().filter(|w| **w == Some(g)).count(), g)).max_by_key(|x| x.0);
            if let (true, Some((_, g))) = (waits.iter().all(Option::is_some), most) {
                let rows = fit.iter().zip(&waits).map(|(b, w)| row(format!("Waiting for {}", good_word(w.unwrap_or(g))), 1.0, Source::Beaver(b.id))).collect();
                out.push(Problem {
                    kind: ProblemKind::WaitingFor,
                    severity: Severity::Warning,
                    value: fit.len() as f32,
                    cause: Breakdown::new(fit.len() as f32, rows),
                    subject: Some(Subject::Good(g)),
                    fix: Fix::OpenJobs,
                    alt_fix: None,
                    focus: Some((fit[0].pos.0.max(0.0) as usize, fit[0].pos.1.max(0.0) as usize)),
                });
                continue;
            }
        }
        // Diggers wait out frozen ground: their beds and banks are frozen in.
        let frozen = crate::agents::frozen(sim) && crate::jobs::frozen_out(role);
        let rows = fit.iter().map(|b| row(if frozen { "Ground frozen" } else { "No work" }, 1.0, Source::Beaver(b.id))).collect();
        out.push(Problem {
            kind: if frozen { ProblemKind::GroundFrozen } else { ProblemKind::NoWork },
            severity: Severity::Warning,
            value: fit.len() as f32,
            cause: Breakdown::new(fit.len() as f32, rows),
            subject: Some(Subject::Role(role)),
            fix: Fix::OpenJobs,
            alt_fix: None,
            focus: Some((fit[0].pos.0.max(0.0) as usize, fit[0].pos.1.max(0.0) as usize)),
        });
    }
}

pub fn problems(sim: &SimState) -> Vec<Problem> {
    let mut out = Vec::new();
    if !sim.colony.is_founded() {
        return out;
    }
    let idle = idle_beavers(sim);
    let pop = sim.colony.population();

    let need = food_need_per_day(sim);
    let gain = sim.ledger.in_per_day(Good::food());
    if pop > 0 && need > gain {
        let days = food_days_left(sim);
        if days < FOOD_WARN_DAYS {
            out.push(Problem {
                kind: ProblemKind::Food,
                severity: if days < FOOD_WARN_DAYS / 3.0 { Severity::Urgent } else { Severity::Warning },
                value: days.floor(),
                cause: food_days_breakdown(sim),
                subject: Some(Subject::Good(Good::food())),
                fix: food_fix(sim),
                alt_fix: None,
                focus: colony_cell(sim),
            });
        }
    }
    if !out.iter().any(|p|p.kind==ProblemKind::Food) {
        if let Some(winter)=winter_food_problem(sim) {out.push(winter);}
    }

    let n = sim.water.dam_height.width();
    // Row by row over both grids at once (they share the map's size): a scan of every cell.
    let weak: Vec<Cell> = sim.water.dam_height.as_slice()[..n * n].iter().zip(sim.water.dam_integrity.as_slice())
        .enumerate()
        .filter(|&(_, (h, v))| *h > 0.0 && *v < WEAK_DAM)
        .map(|(i, _)| (i % n, i / n))
        .collect();
    if !weak.is_empty() {
        let integ = |c: &Cell| sim.water.dam_integrity.get(c.0, c.1).copied().unwrap_or(0.0);
        let worst = *weak.iter().min_by(|a, b| integ(a).total_cmp(&integ(b))).expect("non-empty");
        out.push(Problem {
            kind: ProblemKind::WeakDam,
            severity: if integ(&worst) < WEAK_DAM / 2.0 { Severity::Urgent } else { Severity::Warning },
            value: (integ(&worst) * 100.0).round(),
            cause: dam_wear(sim, worst, Source::Cell(worst)),
            subject: Some(Subject::Building(BuildingKind::dam())),
            fix: sim.buildings.buildings.iter().find(|b| b.dam.is_some() && b.cells().any(|c| c == worst)).map_or(Fix::Focus, |b| Fix::OpenDam { dam: b.id }),
            alt_fix: None,
            focus: Some(worst),
        });
    }

    canal_problems(sim, &mut out);

    if let Some(b)=sim.buildings.buildings.iter().filter(|b|b.is_complete() && !b.kind.is_waterworks()
        && b.condition<0.55 && !sim.maintenance.jobs.contains_key(&b.id))
        .min_by(|a,b|a.condition.total_cmp(&b.condition)) {
        let value=b.condition*100.0;
        out.push(Problem{kind:ProblemKind::Maintenance,severity:if b.condition<0.25{Severity::Urgent}else{Severity::Warning},value,
            cause:Breakdown::new(value,vec![row("Condition",value,Source::Building(b.id))]),subject:Some(Subject::Building(b.kind)),
            fix:Fix::RepairBuilding{building:b.id},alt_fix:None,focus:Some(b.pos)});
    }

    let dry: Vec<&crate::buildings::Building> = sim.buildings.buildings.iter().filter(|b| sim.buildings.stranded(b, &sim.water.depth)).collect();
    if let Some(first) = dry.iter().find(|b| b.kind.is_home()).or(dry.first()) {
        let home = first.kind.is_home();
        let rows = dry.iter().map(|b| row("High and dry", 1.0, building_source(sim, b.id))).collect();
        out.push(Problem {
            kind: ProblemKind::HighAndDry,
            severity: if home { Severity::Urgent } else { Severity::Warning },
            value: dry.len() as f32,
            cause: Breakdown::new(dry.len() as f32, rows),
            subject: Some(Subject::Building(first.kind)),
            fix: Fix::Focus,
            alt_fix: None,
            focus: building_cell(sim, first.id),
        });
    }

    let drowned: Vec<&crate::buildings::Building> = sim.buildings.buildings.iter().filter(|b| crate::buildings::flooded(b, &sim.water.depth)).collect();
    if let Some(first) = drowned.iter().find(|b| b.kind.is_home()).or(drowned.first()) {
        let rows = drowned.iter().map(|b| row("Under water (m)", crate::buildings::flood_depth(b, &sim.water.depth), building_source(sim, b.id))).collect();
        out.push(Problem {
            kind: ProblemKind::Flooded,
            severity: Severity::Urgent,
            value: drowned.len() as f32,
            cause: Breakdown::new(drowned.len() as f32, rows),
            subject: Some(Subject::Building(first.kind)),
            fix: Fix::Focus,
            alt_fix: None,
            focus: building_cell(sim, first.id),
        });
    }

    let stalled: Vec<BuildingId> = sim
        .work
        .places
        .iter()
        .filter(|p| crate::agents::is_site(sim, p) && !p.paused && p.slots.iter().all(|s| s.beaver.is_none()) && !crate::agents::site_stalled(sim, p))
        .map(|p| p.building)
        .collect();
    if let Some(&first) = stalled.first() {
        let can_staff = stalled.iter().any(|&b| crate::jobs::add_beaver_preview(sim, b).is_some());
        let rows = stalled.iter().map(|&b| row(building_name(sim, b), sim.work.progress(b).unwrap_or(0.0), Source::Building(b))).collect();
        out.push(Problem {
            kind: ProblemKind::NoBuilders,
            severity: Severity::Warning,
            value: stalled.len() as f32,
            cause: Breakdown::new(stalled.len() as f32, rows),
            subject: Some(Subject::Building(sim.buildings.building(first).map_or(BuildingKind::dam(), |b| b.kind))),
            fix: if can_staff { Fix::StaffSites } else { Fix::Focus },
            alt_fix: None,
            focus: building_cell(sim, first),
        });
    }

    let busy_fix = |f: &Fix| matches!(f, Fix::AddBeavers { .. } | Fix::Gather { .. } | Fix::StaffSites);
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
                    crate::jobs::IdleReason::Free => "Free",
                };
                row(label, 1.0, Source::Beaver(id))
            })
            .collect();
        let open = crate::jobs::Job::by_priority().into_iter().find_map(|role| {
            let slots: usize = sim
                .work
                .places
                .iter()
                .filter(|p| !p.paused && (crate::agents::is_site(sim, p) || sim.buildings.building(p.building).is_some_and(|b| b.is_complete())))
                .flat_map(|p| p.slots.iter())
                .filter(|s| s.beaver.is_none() && s.role == role)
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

    no_work_problems(sim, &mut out);

    // Warn loudly when the colony has no finished store for staple goods (sticks/mud or food).
    let mut missing_stores: Vec<BuildingKind> = Vec::new();
    for g in [crate::dams::material(), Good::food()] {
        if sim.buildings.stores.capacity(g) <= crate::work::EPS {
            if let Some(k) = BuildingKind::store_for(g) {
                if !missing_stores.contains(&k) {
                    missing_stores.push(k);
                }
            }
        }
    }
    if !missing_stores.is_empty() {
        let rows = missing_stores
            .iter()
            .map(|k| row(k.name(), 0.0, Source::None))
            .collect();
        let fix = place_fix(sim, missing_stores[0]);
        let alt_fix = missing_stores.get(1).map(|&k| place_fix(sim, k));
        out.push(Problem {
            kind: ProblemKind::NoStorage,
            severity: Severity::Urgent,
            value: missing_stores.len() as f32,
            cause: Breakdown::new(0.0, rows),
            subject: Some(Subject::Building(missing_stores[0])),
            fix,
            alt_fix,
            focus: colony_cell(sim),
        });
    }

    for g in Good::all() {
        let (have, cap) = (sim.buildings.stores.amount(g), sim.buildings.stores.capacity(g));
        let arriving = sim.ledger.in_per_day(g);
        if store_full(sim, g) && (arriving > 0.0 || held_back_by_room(sim, g)) {
            let rows = vec![row("Stored", have, Source::None), row("Room", cap, Source::None), row("Arriving", arriving, Source::None)];
            out.push(Problem {
                kind: ProblemKind::StoreFull,
                severity: Severity::Warning,
                value: (have / cap * 100.0).round(),
                cause: Breakdown::new((have / cap * 100.0).round(), rows),
                subject: Some(Subject::Good(g)),
                fix: BuildingKind::store_for(g).map_or(Fix::Focus, |k| place_fix(sim, k)),
                alt_fix: None,
                focus: colony_cell(sim),
            });
            break;
        }
    }

    if let Some(p) = no_free_lodge(sim) {
        out.push(p);
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
                subject: None,
                fix: Fix::Focus,
                alt_fix: None,
                focus: colony_cell(sim),
            });
        }
    }

    if let Some(r) = sim.society.pending().first() {
        let stage = if r.adult { Stage::Adult } else { Stage::Yearling };
        out.push(Problem {
            kind: ProblemKind::Stranger,
            severity: Severity::Warning,
            value: 1.0,
            cause: Breakdown::new(1.0, vec![row(if r.adult { "Adult" } else { "Young" }, 1.0, Source::Cell(r.at)), row("Joins in (days)", crate::society::Society::days_to_answer(r, sim.time_days) as f32, Source::None)]),
            subject: Some(Subject::Stage(stage)),
            fix: Fix::Accept(r.id),
            alt_fix: Some(Fix::Decline(r.id)),
            focus: Some(r.at),
        });
    }

    // First, so the cap on alerts never hides it.
    if let Some(seen) = crate::predators::last_seen(sim) {
        out.insert(0, Problem {
            kind: ProblemKind::Wolf,
            severity: Severity::Urgent,
            value: 1.0,
            cause: Breakdown::new(1.0, Vec::new()),
            subject: None,
            fix: Fix::Focus,
            alt_fix: None,
            focus: Some((seen.0.max(0.0) as usize, seen.1.max(0.0) as usize)),
        });
    }

    out.sort_by_key(|p| p.severity);
    out.truncate(MAX_PROBLEMS);
    out
}

/// No lodge stands empty while a beaver needs one: a homeless beaver, or a grown one still in
/// its parents' lodge that cannot move out or start a family without one. Quiet while a new
/// lodge is going up.
fn no_free_lodge(sim: &SimState) -> Option<Problem> {
    let c = &sim.colony;
    let empty = c.lodges.iter().any(|l| !c.beavers.iter().any(|b| b.lodge == Some(l.id)));
    let going_up = sim.buildings.buildings.iter().any(|b| b.kind.is_home() && !b.is_complete());
    if empty || going_up {
        return None;
    }
    let with_parent = |b: &crate::colony::Beaver| {
        b.lodge.is_some() && [b.mother, b.father].into_iter().flatten().any(|p| c.beaver(p).is_some_and(|p| p.lodge == b.lodge))
    };
    let homeless: Vec<&crate::colony::Beaver> = c.beavers.iter().filter(|b| b.lodge.is_none()).collect();
    let leaving: Vec<&crate::colony::Beaver> = c.beavers.iter()
        .filter(|b| b.stage == Stage::Adult && b.mate.is_none() && b.age_years() >= crate::colony::LEAVE_HOME_AGE_YEARS && with_parent(b))
        .collect();
    let n = homeless.len() + leaving.len();
    if n == 0 {
        return None;
    }
    let rows = homeless.iter().map(|b| row("No home", 1.0, Source::Beaver(b.id)))
        .chain(leaving.iter().map(|b| row("Ready to leave home", 1.0, Source::Beaver(b.id))))
        .collect();
    Some(Problem {
        kind: ProblemKind::NoFreeLodge,
        severity: Severity::Warning,
        value: n as f32,
        cause: Breakdown::new(n as f32, rows),
        subject: Some(Subject::Building(BuildingKind::home())),
        fix: place_fix(sim, BuildingKind::home()),
        alt_fix: None,
        focus: colony_cell(sim),
    })
}

/// Build `kind`, or, while it is locked, the building that unlocks it.
fn place_fix(sim: &SimState, mut kind: BuildingKind) -> Fix {
    while let Err(crate::buildings::BuildError::Locked(need)) = sim.unlocked(kind) {
        if sim.buildings.buildings.iter().any(|b| b.kind == need) {
            break;
        }
        kind = need;
    }
    Fix::Place(kind)
}

pub fn apply_fix(sim: &mut SimState, fix: Fix) -> Result<(), String> {
    let err = |e: crate::SimError| e.to_string();
    match fix {
        Fix::AddBeavers { building, n } => {
            for k in 0..n.max(1) {
                if let Err(e) = sim.apply(Command::AddBeaver { building }) {
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
            let mut sites: Vec<(usize, BuildingId)> = sim
                .work
                .places
                .iter()
                .filter(|p| crate::agents::is_site(sim, p) && !p.paused && p.filled() < p.slots.len() && !crate::agents::site_stalled(sim, p))
                .map(|p| (p.filled().min(1), p.building))
                .collect();
            sites.sort();
            let sites: Vec<BuildingId> = sites.into_iter().map(|(_, b)| b).collect();
            let mut placed = 0;
            for b in sites {
                while sim.work.place(b).is_some_and(|p| p.filled() < p.slots.len()) && sim.apply(Command::AddBeaver { building: b }).is_ok() {
                    placed += 1;
                }
            }
            if placed == 0 { Err("no beaver is free to build".into()) } else { Ok(()) }
        }
        Fix::PutToWork { role, n } => sim.apply(Command::AdjustRole { role, delta: n.max(1) as i32 }).map_err(err),

        Fix::RepairBuilding { building } => sim.apply(Command::RepairBuilding { building }).map_err(err),
        Fix::Accept(id) => sim.apply(Command::AnswerJoin { id, answer: crate::society::Answer::Accept }).map_err(err),
        Fix::Decline(id) => sim.apply(Command::AnswerJoin { id, answer: crate::society::Answer::TurnAway }).map_err(err),
        Fix::Place(_) | Fix::OpenJobs | Fix::Focus | Fix::OpenDam { .. } => Ok(()),
    }
}

/// Canals that need the player: dry for three days, silting while unkept, or draining a pond.
fn canal_problems(sim: &SimState, out: &mut Vec<Problem>) {
    use crate::gates::GateSetting;
    for b in &sim.buildings.buildings {
        let Some(k) = b.canal.as_deref() else { continue };
        let Some(s) = k.status.as_ref() else { continue };
        let at = Some(k.gate.cell);
        let dug = s.dug * 2 >= s.cells.max(1);
        let dry_days = k.history.iter().rev().take(3).filter(|d| d.floatable <= 0.0).count();
        if dug && k.gate.setting != GateSetting::Shut && s.floatable == 0 && dry_days >= 3 {
            out.push(Problem { kind: ProblemKind::CanalDry, severity: Severity::Warning, value: 0.0,
                cause: Breakdown::new(0.0, vec![row("Floats a load", 0.0, Source::Building(b.id))]),
                subject: Some(Subject::Building(BuildingKind::canal())), fix: Fix::OpenDam { dam: b.id }, alt_fix: None, focus: at });
        }
        if !k.upkeep && s.silted * 4 >= s.cells.max(1) {
            let v = (s.silted as f32 / s.cells.max(1) as f32 * 100.0).round();
            out.push(Problem { kind: ProblemKind::CanalSilting, severity: Severity::Warning, value: v,
                cause: Breakdown::new(v, vec![row("Silted cells", s.silted as f32, Source::Building(b.id))]),
                subject: Some(Subject::Building(BuildingKind::canal())), fix: Fix::OpenDam { dam: b.id }, alt_fix: None, focus: at });
        }
    }
    // A pond that fell 0.2 m in a day while a gate on it was open.
    for b in &sim.buildings.buildings {
        let Some(d) = b.dam.as_deref() else { continue };
        let mut h = d.history.iter().rev();
        let (Some(today), Some(before)) = (h.next(), h.next()) else { continue };
        let open = matches!(d.pond.as_ref().map(|p| p.spill), Some(crate::dams::Spill::Gate(_)));
        if open && before.level - today.level >= 0.2 {
            let v = ((before.level - today.level) * 100.0).round();
            out.push(Problem { kind: ProblemKind::PondDraining, severity: Severity::Warning, value: v,
                cause: Breakdown::new(v, vec![row("Fell today (cm)", v, Source::Building(b.id))]),
                subject: Some(Subject::Building(BuildingKind::dam())), fix: Fix::OpenDam { dam: b.id }, alt_fix: None, focus: Some(b.pos) });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct DryLand;
    impl crate::colony::Landscape for DryLand {
        fn is_water(&self, _: f32, _: f32) -> bool {
            false
        }
    }

    #[test]
    fn no_storage_before_stores_built_has_zero_capacity_and_urgent_warning() {
        let mut sim = SimState::new(1);
        let cell = (sim.world.size / 2, sim.world.size / 2);
        sim.colony.found((cell.0 as f32, cell.1 as f32), &DryLand).expect("found");
        for g in Good::all() {
            assert_eq!(sim.buildings.stores.capacity(g), 0.0, "no phantom base capacity before stores exist");
        }
        let probs = problems(&sim);
        let no_store = probs.iter().find(|p| p.kind == ProblemKind::NoStorage).expect("NoStorage warning expected");
        assert_eq!(no_store.severity, Severity::Urgent);

        for x in cell.0..cell.0 + 4 {
            for y in cell.1..cell.1 + 4 {
                if let Some(v) = sim.water.depth.get_mut(x, y) {
                    *v = 0.25;
                }
            }
        }
        for (i, key) in ["StickPile", "FoodCache"].into_iter().enumerate() {
            let kind = BuildingKind::of(key);
            sim.buildings.buildings.push(crate::buildings::Building {
                id: BuildingId(100 + i as u64),
                kind,
                pos: (cell.0 + i * 2, cell.1),
                status: crate::buildings::Status::Complete,
                output: 0.0,
                condition: 1.0,
                reinforced: false,
                dam: None,
                canal: None,
            });
        }
        sim.buildings.update_capacity(crate::buildings::Terrain { depth: &sim.water.depth });
        assert!(sim.buildings.stores.capacity(Good::food()) > 0.0);
        assert!(sim.buildings.stores.capacity(crate::dams::material()) > 0.0);
        assert!(!problems(&sim).iter().any(|p| p.kind == ProblemKind::NoStorage));
    }
}
