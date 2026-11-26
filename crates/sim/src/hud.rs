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
pub const WINTER_RESERVE_DAYS: f32 = crate::calendar::SEASON_DAYS as f32;

pub fn idle_beavers(sim: &SimState) -> Vec<BeaverId> {
    sim.colony.beavers.iter()
        .filter(|b| b.can_work() && sim.work.slot_of(b.id).is_none()
            && sim.agents.get(b.id).is_none_or(|a| a.assignment.is_none() && matches!(a.task, Task::Idle | Task::Rest)))
        .map(|b| b.id).collect()
}

#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum Source { Building(BuildingId), Beaver(BeaverId), Cell(Cell), None }

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct BreakdownRow { pub label: String, pub value: f32, pub source: Source }

#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Breakdown { pub total: f32, pub rows: Vec<BreakdownRow> }

impl Breakdown {
    fn new(total: f32, mut rows: Vec<BreakdownRow>) -> Self {
        rows.sort_by(|a, b| b.value.abs().total_cmp(&a.value.abs()));
        Breakdown { total, rows }
    }
}

fn row(label: impl Into<String>, value: f32, source: Source) -> BreakdownRow { BreakdownRow { label: label.into(), value, source } }
fn building_source(sim: &SimState, b: BuildingId) -> Source { if sim.buildings.building(b).is_some() { Source::Building(b) } else { Source::None } }
fn building_name(sim: &SimState, b: BuildingId) -> String { sim.buildings.building(b).map_or("Gone", |b| b.kind.spec().name.as_str()).to_string() }
fn pos_cell(p: (f32, f32)) -> Cell { (p.0.max(0.0) as usize, p.1.max(0.0) as usize) }

pub fn good_in_breakdown(sim: &SimState, g: Good) -> Breakdown {
    let rows = sim.ledger.in_by(g).into_iter().map(|(p, v)| match p {
        Producer::Building(b) => row(building_name(sim, b), v, building_source(sim, b)),
        Producer::Order => row("By order", v, Source::None),
        Producer::Other => row("Other", v, Source::None),
    }).collect();
    Breakdown::new(sim.ledger.in_per_day(g), rows)
}

pub fn good_out_breakdown(sim: &SimState, g: Good) -> Breakdown {
    let label = |u| match u {
        Use::EatenAdults => "Adults", Use::EatenYearlings => "Yearlings", Use::EatenKits => "Kits",
        Use::Spoiled => "Spoiled", Use::Sites => "Building sites", Use::Crafting => "Crafting",
        Use::Repairs => "Repairs", Use::Other => "Other",
    };
    let rows = Use::ALL.iter().map(|&u| (u, sim.ledger.out_for(g, u))).filter(|(_, v)| *v > 1e-4).map(|(u, v)| row(label(u), -v, Source::None)).collect();
    Breakdown::new(-sim.ledger.out_per_day(g), rows)
}

pub fn food_days_breakdown(sim: &SimState) -> Breakdown {
    let (stock, need, gain) = (sim.buildings.stores.amount(Good::food()), food_need_per_day(sim), sim.ledger.in_per_day(Good::food()));
    let mut rows = vec![row("Stock", stock, Source::None), row("Gathered", gain, Source::None)];
    for (st, label) in [(Stage::Adult, "Adults"), (Stage::Yearling, "Yearlings"), (Stage::Kit, "Kits")] {
        let eat: f32 = sim.colony.beavers.iter().filter(|b| b.stage == st).map(|b| crate::tuning::colony::HUNGER_PER_DAY * crate::growth::hunger_x(b) * b.stage.food_weight()).sum();
        if eat > 0.0 { rows.push(row(label, -eat, Source::None)); }
    }
    let spoilage = stock * crate::tuning::colony::FOOD_SPOILAGE;
    if spoilage > 0.0 { rows.push(row("Spoilage now", -spoilage, Source::None)); }
    Breakdown::new(food_runout_days(stock, need, gain, crate::tuning::colony::FOOD_SPOILAGE), rows)
}

pub fn food_runout_days(stock: f32, need: f32, gain: f32, spoilage: f32) -> f32 {
    let deficit = need - gain;
    if deficit <= 0.0 { return f32::INFINITY; }
    if stock <= 0.0 { return 0.0; }
    if spoilage <= 0.0 { return stock / deficit; }
    (stock * spoilage / deficit).ln_1p() / spoilage
}

pub fn food_days_left(sim: &SimState) -> f32 {
    food_runout_days(sim.buildings.stores.amount(Good::food()), food_need_per_day(sim), sim.ledger.in_per_day(Good::food()), crate::tuning::colony::FOOD_SPOILAGE)
}

pub fn headcount_breakdown(sim: &SimState) -> Breakdown {
    let c = &sim.colony;
    let housed = c.beavers.iter().filter(|b| b.lodge.is_some()).count();
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
    let mut rows = vec![
        row("Adults", c.count_stage(Stage::Adult) as f32, Source::None),
        row("Yearlings", c.count_stage(Stage::Yearling) as f32, Source::None),
        row("Kits", c.count_stage(Stage::Kit) as f32, Source::None),
        row("Housed", housed as f32, Source::None),
        row("Unhoused", (c.population() - housed) as f32, Source::None),
        row("Born", born as f32, Source::None),
        row("Joined", joined as f32, Source::None),
        row("Lost", -(died as f32), Source::None),
    ];
    rows.retain(|r| r.value != 0.0);
    Breakdown::new(c.population() as f32, rows)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
pub enum Severity { Urgent, Warning }

#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum Fix {
    Gather { kind: ResourceKind, n: usize },
    Place(BuildingKind),
    StaffSites,
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
    Food, WinterFood, Idle, NoBuilders, NoStorage, StoreFull, WeakDam, Maintenance, Stranger,
    ForestFalling, HighAndDry, Flooded, CanalDry, CanalSilting, PondDraining, Wolf, NoWork,
    GroundFrozen, WaitingFor, NoFreeLodge,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Subject { Good(Good), Building(BuildingKind), Stage(Stage), Role(Job) }

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

fn prob(kind: ProblemKind, severity: Severity, value: f32, cause: Breakdown, subject: Option<Subject>, fix: Fix, focus: Option<Cell>) -> Problem {
    Problem { kind, severity, value, cause, subject, fix, alt_fix: None, focus }
}

fn building_cell(sim: &SimState, id: BuildingId) -> Option<Cell> { sim.buildings.building(id).map(|b| b.pos) }
fn colony_cell(sim: &SimState) -> Option<Cell> { sim.colony.lodges.first().map(|l| l.pos).or(sim.colony.beavers.first().map(|b| b.pos)).map(pos_cell) }
pub fn food_need_per_day(sim: &SimState) -> f32 {
    sim.colony.beavers.iter().map(|b| crate::tuning::colony::HUNGER_PER_DAY * crate::growth::hunger_x(b) * b.stage.food_weight()).sum()
}
pub(crate) fn gathered_good(role: Job) -> Option<Good> { crate::jobs::gather_kind(role)?.good() }
fn store_full(sim: &SimState, g: Good) -> bool {
    let cap = sim.buildings.stores.capacity(g);
    cap > 0.0 && sim.buildings.stores.amount(g) >= cap * FULL_SHARE
}

fn gatherers(sim: &SimState, n: usize) -> Vec<BeaverId> {
    let home = colony_cell(sim).unwrap_or((0, 0));
    let mut v: Vec<_> = sim.colony.beavers.iter().filter(|b| b.can_work())
        .filter_map(|b| sim.agents.get(b.id).map(|a| (crate::jobs::is_busy(sim, b.id), a.cell.0.abs_diff(home.0) + a.cell.1.abs_diff(home.1), b.id)))
        .collect();
    v.sort();
    v.into_iter().take(n).map(|t| t.2).collect()
}

fn food_fix(sim: &SimState) -> Fix {
    let workers = sim.colony.beavers.iter().filter(|b| b.can_work()).count();
    let n = gatherers(sim, workers.div_ceil(4).max(FIX_BEAVERS)).len();
    if let Some(home) = colony_cell(sim).filter(|_| n > 0) {
        let r = FOOD_GATHER_RADIUS;
        let area = crate::work::Area { min: (home.0.saturating_sub(r), home.1.saturating_sub(r)), max: (home.0 + r, home.1 + r) };
        if crate::agents::gathering_available(sim, Assignment::Area(ResourceKind::food_plant(false), area)) {
            return Fix::Gather { kind: ResourceKind::food_plant(false), n };
        }
    }
    Fix::Focus
}

fn no_work_problems(sim: &SimState, out: &mut Vec<Problem>) {
    for role in Job::by_priority() {
        if gathered_good(role).is_some_and(|g| store_full(sim, g)) { continue; }
        let fit: Vec<_> = sim.colony.beavers.iter().filter(|b| b.can_work() && !b.resting && crate::jobs::role_of(sim, b.id) == Some(role)).collect();
        if fit.is_empty() || !fit.iter().all(|b| crate::jobs::waiting(sim, b.id).is_some()) { continue; }
        let at = Some(pos_cell(fit[0].pos));
        if role == Job::with_duty(crate::content::Duty::Build) {
            let waits: Vec<_> = fit.iter().map(|b| match crate::jobs::waiting(sim, b.id) { Some(crate::jobs::Wait::Good(g)) => Some(g), _ => None }).collect();
            let most = Good::all().map(|g| (waits.iter().filter(|w| **w == Some(g)).count(), g)).max_by_key(|x| x.0);
            if let (true, Some((_, g))) = (waits.iter().all(Option::is_some), most) {
                let rows = fit.iter().zip(&waits).map(|(b, w)| row(format!("Waiting for {}", w.unwrap_or(g).name().to_lowercase()), 1.0, Source::Beaver(b.id))).collect();
                out.push(prob(ProblemKind::WaitingFor, Severity::Warning, fit.len() as f32, Breakdown::new(fit.len() as f32, rows), Some(Subject::Good(g)), Fix::OpenJobs, at));
                continue;
            }
        }
        let frozen = crate::agents::frozen(sim) && crate::jobs::frozen_out(role);
        let rows = fit.iter().map(|b| row(if frozen { "Ground frozen" } else { "No work" }, 1.0, Source::Beaver(b.id))).collect();
        out.push(prob(if frozen { ProblemKind::GroundFrozen } else { ProblemKind::NoWork }, Severity::Warning, fit.len() as f32, Breakdown::new(fit.len() as f32, rows), Some(Subject::Role(role)), Fix::OpenJobs, at));
    }
}

pub fn problems(sim: &SimState) -> Vec<Problem> {
    let mut out = Vec::new();
    if !sim.colony.is_founded() { return out; }
    let (idle, pop, need, gain) = (idle_beavers(sim), sim.colony.population(), food_need_per_day(sim), sim.ledger.in_per_day(Good::food()));

    if pop > 0 && need > gain {
        let days = food_days_left(sim);
        if days < FOOD_WARN_DAYS {
            out.push(prob(ProblemKind::Food, if days < FOOD_WARN_DAYS / 3.0 { Severity::Urgent } else { Severity::Warning },
                days.floor(), food_days_breakdown(sim), Some(Subject::Good(Good::food())), food_fix(sim), colony_cell(sim)));
        }
    }
    if !out.iter().any(|p| p.kind == ProblemKind::Food) && crate::calendar::Season::at(sim.time_days) == crate::calendar::Season::Autumn {
        let (stock, target, cap) = (sim.buildings.stores.amount(Good::food()), need * WINTER_RESERVE_DAYS, sim.buildings.stores.capacity(Good::food()));
        if need > 0.0 && stock < target {
            let (gathering, store) = (food_fix(sim), BuildingKind::store_for(Good::food()));
            let add = cap < target && store.is_some() && !sim.buildings.buildings.iter().any(|b| Some(b.kind) == store && !b.is_complete());
            let mut p = prob(ProblemKind::WinterFood, Severity::Warning, stock / need,
                Breakdown::new(stock / need, vec![row("Stored food", stock, Source::None), row("Storage capacity", cap, Source::None), row("Winter reserve goal", target, Source::None), row("Daily appetite", need, Source::None)]),
                Some(Subject::Good(Good::food())), match store { Some(k) if add => place_fix(sim, k), _ => gathering }, colony_cell(sim));
            if add && gathering != Fix::Focus { p.alt_fix = Some(gathering); }
            out.push(p);
        }
    }

    let worst = sim.buildings.buildings.iter().filter(|b| b.dam.is_some())
        .flat_map(|b| b.cells().map(move |c| (b.id, c)))
        .filter_map(|(id, c)| {
            let h = sim.water.dam_height.get(c.0, c.1).copied().unwrap_or(0.0);
            let v = sim.water.dam_integrity.get(c.0, c.1).copied().unwrap_or(1.0);
            (h > 0.0 && v < WEAK_DAM).then_some((id, c, v))
        })
        .min_by(|a, b| a.2.total_cmp(&b.2));
    if let Some((id, c, v)) = worst {
        let (age, over) = sim.water.dam_cell_wear(&sim.height, c.1 * sim.height.width() + c.0);
        let mut rows = vec![row("Age", -age, Source::Cell(c))];
        if over > 0.0 { rows.push(row("Overtopping", -over, Source::Cell(c))); }
        out.push(prob(ProblemKind::WeakDam, if v < WEAK_DAM / 2.0 { Severity::Urgent } else { Severity::Warning },
            (v * 100.0).round(), Breakdown::new(v, rows), Some(Subject::Building(BuildingKind::dam())), Fix::OpenDam { dam: id }, Some(c)));
    }

    canal_problems(sim, &mut out);

    if let Some(b) = sim.buildings.buildings.iter().filter(|b| b.is_complete() && !b.kind.is_waterworks() && b.condition < 0.55 && !sim.maintenance.jobs.contains_key(&b.id))
        .min_by(|a, b| a.condition.total_cmp(&b.condition))
    {
        let v = b.condition * 100.0;
        out.push(prob(ProblemKind::Maintenance, if b.condition < 0.25 { Severity::Urgent } else { Severity::Warning },
            v, Breakdown::new(v, vec![row("Condition", v, Source::Building(b.id))]), Some(Subject::Building(b.kind)), Fix::RepairBuilding { building: b.id }, Some(b.pos)));
    }

    let dry: Vec<_> = sim.buildings.buildings.iter().filter(|b| sim.buildings.stranded(b, &sim.water.depth)).collect();
    if let Some(first) = dry.iter().find(|b| b.kind.is_home()).or(dry.first()) {
        let rows = dry.iter().map(|b| row("High and dry", 1.0, building_source(sim, b.id))).collect();
        out.push(prob(ProblemKind::HighAndDry, if first.kind.is_home() { Severity::Urgent } else { Severity::Warning },
            dry.len() as f32, Breakdown::new(dry.len() as f32, rows), Some(Subject::Building(first.kind)), Fix::Focus, building_cell(sim, first.id)));
    }

    let drowned: Vec<_> = sim.buildings.buildings.iter().filter(|b| crate::buildings::flooded(b, &sim.water.depth)).collect();
    if let Some(first) = drowned.iter().find(|b| b.kind.is_home()).or(drowned.first()) {
        let rows = drowned.iter().map(|b| row("Under water (m)", crate::buildings::flood_depth(b, &sim.water.depth), building_source(sim, b.id))).collect();
        out.push(prob(ProblemKind::Flooded, Severity::Urgent, drowned.len() as f32,
            Breakdown::new(drowned.len() as f32, rows), Some(Subject::Building(first.kind)), Fix::Focus, building_cell(sim, first.id)));
    }

    let stalled: Vec<BuildingId> = sim.work.places.iter()
        .filter(|p| crate::agents::is_site(sim, p) && !p.paused && p.slots.iter().all(|s| s.beaver.is_none()) && !crate::agents::site_stalled(sim, p))
        .map(|p| p.building).collect();
    if let Some(&first) = stalled.first() {
        let can_staff = stalled.iter().any(|&b| crate::jobs::add_beaver_preview(sim, b).is_some());
        let rows = stalled.iter().map(|&b| row(building_name(sim, b), sim.work.progress(b).unwrap_or(0.0), Source::Building(b))).collect();
        out.push(prob(ProblemKind::NoBuilders, Severity::Warning, stalled.len() as f32, Breakdown::new(stalled.len() as f32, rows),
            Some(Subject::Building(sim.buildings.building(first).map_or(BuildingKind::dam(), |b| b.kind))),
            if can_staff { Fix::StaffSites } else { Fix::Focus }, building_cell(sim, first)));
    }

    if !idle.is_empty() && !out.iter().any(|p| matches!(p.fix, Fix::Gather { .. } | Fix::StaffSites)) {
        let rows = crate::jobs::idle_reasons(sim).into_iter().map(|(id, r)| row(match r {
            crate::jobs::IdleReason::NoOpenSlot => "No open slot",
            crate::jobs::IdleReason::Unreachable => "Unreachable",
            crate::jobs::IdleReason::Resting => "Resting",
            crate::jobs::IdleReason::Paused => "Paused",
            crate::jobs::IdleReason::Free => "Free",
        }, 1.0, Source::Beaver(id))).collect();
        let open = Job::by_priority().into_iter().find_map(|role| {
            let slots = sim.work.places.iter().filter(|p| !p.paused && (crate::agents::is_site(sim, p) || sim.buildings.building(p.building).is_some_and(|b| b.is_complete())))
                .flat_map(|p| &p.slots).filter(|s| s.beaver.is_none() && s.role == role).count();
            (slots > 0 && crate::jobs::role_add_preview(sim, role).is_some()).then_some((role, slots))
        });
        out.push(prob(ProblemKind::Idle, Severity::Warning, idle.len() as f32, Breakdown::new(idle.len() as f32, rows), None,
            open.map_or(Fix::OpenJobs, |(role, slots)| Fix::PutToWork { role, n: slots.min(idle.len()) }),
            sim.colony.beaver(idle[0]).map(|b| pos_cell(b.pos))));
    }

    no_work_problems(sim, &mut out);

    let mut missing_stores = Vec::new();
    for g in [crate::dams::material(), Good::food()] {
        if sim.buildings.stores.capacity(g) <= crate::work::EPS && let Some(k) = BuildingKind::store_for(g) && !missing_stores.contains(&k) {
            missing_stores.push(k);
        }
    }
    if !missing_stores.is_empty() {
        let rows = missing_stores.iter().map(|k| row(k.name(), 0.0, Source::None)).collect();
        let mut p = prob(ProblemKind::NoStorage, Severity::Urgent, missing_stores.len() as f32, Breakdown::new(0.0, rows),
            Some(Subject::Building(missing_stores[0])), place_fix(sim, missing_stores[0]), colony_cell(sim));
        p.alt_fix = missing_stores.get(1).map(|&k| place_fix(sim, k));
        out.push(p);
    }

    for g in Good::all() {
        let (have, cap, arr) = (sim.buildings.stores.amount(g), sim.buildings.stores.capacity(g), sim.ledger.in_per_day(g));
        let held = sim.colony.beavers.iter().any(|b| crate::jobs::role_of(sim, b.id).and_then(gathered_good) == Some(g) && crate::jobs::waiting(sim, b.id).is_some());
        if store_full(sim, g) && (arr > 0.0 || held) {
            let pct = (have / cap * 100.0).round();
            out.push(prob(ProblemKind::StoreFull, Severity::Warning, pct,
                Breakdown::new(pct, vec![row("Stored", have, Source::None), row("Room", cap, Source::None), row("Arriving", arr, Source::None)]),
                Some(Subject::Good(g)), BuildingKind::store_for(g).map_or(Fix::Focus, |k| place_fix(sim, k)), colony_cell(sim)));
            break;
        }
    }

    if let Some(p) = no_free_lodge(sim) { out.push(p); }

    if let Some((then, now)) = crate::forestry::forest_trend(sim)
        && then > 0 && (now as f32) < then as f32 * (1.0 - crate::forestry::FALLING_SHARE)
    {
        let lost = ((then - now) as f32 / then as f32 * 100.0).round();
        out.push(prob(ProblemKind::ForestFalling, Severity::Warning, lost,
            Breakdown::new(lost, vec![row("Then", then as f32, Source::None), row("Now", now as f32, Source::None)]), None, Fix::Focus, colony_cell(sim)));
    }

    if let Some(r) = sim.society.pending().first() {
        let mut p = prob(ProblemKind::Stranger, Severity::Warning, 1.0,
            Breakdown::new(1.0, vec![row(if r.adult { "Adult" } else { "Young" }, 1.0, Source::Cell(r.at)), row("Joins in (days)", crate::society::Society::days_to_answer(r, sim.time_days) as f32, Source::None)]),
            Some(Subject::Stage(if r.adult { Stage::Adult } else { Stage::Yearling })), Fix::Accept(r.id), Some(r.at));
        p.alt_fix = Some(Fix::Decline(r.id));
        out.push(p);
    }

    if let Some(seen) = crate::predators::last_seen(sim) {
        out.insert(0, prob(ProblemKind::Wolf, Severity::Urgent, 1.0, Breakdown::new(1.0, Vec::new()), None, Fix::Focus, Some(pos_cell(seen))));
    }

    out.sort_by_key(|p| p.severity);
    out.truncate(MAX_PROBLEMS);
    out
}

fn no_free_lodge(sim: &SimState) -> Option<Problem> {
    let c = &sim.colony;
    if c.lodges.iter().any(|l| !c.beavers.iter().any(|b| b.lodge == Some(l.id)))
        || sim.buildings.buildings.iter().any(|b| b.kind.is_home() && !b.is_complete())
    { return None; }
    let homeless: Vec<_> = c.beavers.iter().filter(|b| b.lodge.is_none()).collect();
    let leaving: Vec<_> = c.beavers.iter().filter(|b| b.stage == Stage::Adult && b.mate.is_none() && b.age_years() >= crate::colony::LEAVE_HOME_AGE_YEARS && c.lives_with_parent(b)).collect();
    let n = homeless.len() + leaving.len();
    if n == 0 { return None; }
    let rows = homeless.iter().map(|b| row("No home", 1.0, Source::Beaver(b.id)))
        .chain(leaving.iter().map(|b| row("Ready to leave home", 1.0, Source::Beaver(b.id)))).collect();
    Some(prob(ProblemKind::NoFreeLodge, Severity::Warning, n as f32, Breakdown::new(n as f32, rows),
        Some(Subject::Building(BuildingKind::home())), place_fix(sim, BuildingKind::home()), colony_cell(sim)))
}

fn place_fix(sim: &SimState, mut kind: BuildingKind) -> Fix {
    while let Err(crate::buildings::BuildError::Locked(need)) = sim.unlocked(kind) {
        if sim.buildings.buildings.iter().any(|b| b.kind == need) { break; }
        kind = need;
    }
    Fix::Place(kind)
}

pub fn apply_fix(sim: &mut SimState, fix: Fix) -> Result<(), String> {
    let err = |e: crate::SimError| e.to_string();
    match fix {
        Fix::Gather { kind, n } => {
            let (who, home) = (gatherers(sim, n.max(1)), colony_cell(sim).ok_or("the colony has no home")?);
            if who.is_empty() { return Err("no beaver is free to gather".into()); }
            let r = FOOD_GATHER_RADIUS;
            let area = crate::work::Area { min: (home.0.saturating_sub(r), home.1.saturating_sub(r)), max: (home.0 + r, home.1 + r) };
            sim.apply(Command::Assign { who, to: Some(Assignment::Area(kind, area)) }).map_err(err)
        }
        Fix::StaffSites => {
            let mut sites: Vec<_> = sim.work.places.iter()
                .filter(|p| crate::agents::is_site(sim, p) && !p.paused && p.filled() < p.slots.len() && !crate::agents::site_stalled(sim, p))
                .map(|p| (p.filled().min(1), p.building)).collect();
            sites.sort();
            let mut placed = 0;
            for (_, b) in sites {
                while sim.work.place(b).is_some_and(|p| p.filled() < p.slots.len()) && sim.apply(Command::AddBeaver { building: b }).is_ok() { placed += 1; }
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

fn canal_problems(sim: &SimState, out: &mut Vec<Problem>) {
    for b in &sim.buildings.buildings {
        if let Some(k) = b.canal.as_deref() && let Some(s) = k.status.as_ref() {
            let at = Some(k.gate.cell);
            let dry = k.history.iter().rev().take(3).filter(|d| d.floatable <= 0.0).count();
            if s.dug * 2 >= s.cells.max(1) && k.gate.setting != crate::gates::GateSetting::Shut && s.floatable == 0 && dry >= 3 {
                out.push(prob(ProblemKind::CanalDry, Severity::Warning, 0.0, Breakdown::new(0.0, vec![row("Floats a load", 0.0, Source::Building(b.id))]),
                    Some(Subject::Building(BuildingKind::canal())), Fix::OpenDam { dam: b.id }, at));
            }
            if !k.upkeep && s.silted * 4 >= s.cells.max(1) {
                let v = (s.silted as f32 / s.cells.max(1) as f32 * 100.0).round();
                out.push(prob(ProblemKind::CanalSilting, Severity::Warning, v, Breakdown::new(v, vec![row("Silted cells", s.silted as f32, Source::Building(b.id))]),
                    Some(Subject::Building(BuildingKind::canal())), Fix::OpenDam { dam: b.id }, at));
            }
        }
        if let Some(d) = b.dam.as_deref() {
            let mut h = d.history.iter().rev();
            if let (Some(today), Some(before)) = (h.next(), h.next())
                && matches!(d.pond.as_ref().map(|p| p.spill), Some(crate::dams::Spill::Gate(_)))
                && before.level - today.level >= 0.2
            {
                let v = ((before.level - today.level) * 100.0).round();
                out.push(prob(ProblemKind::PondDraining, Severity::Warning, v, Breakdown::new(v, vec![row("Fell today (cm)", v, Source::Building(b.id))]),
                    Some(Subject::Building(BuildingKind::dam())), Fix::OpenDam { dam: b.id }, Some(b.pos)));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    struct DryLand;
    impl crate::colony::Landscape for DryLand { fn is_water(&self, _: f32, _: f32) -> bool { false } }

    #[test]
    fn no_storage_before_stores_built_has_zero_capacity_and_urgent_warning() {
        let mut sim = SimState::new(1);
        let cell = (sim.world.size / 2, sim.world.size / 2);
        sim.colony.found((cell.0 as f32, cell.1 as f32), &DryLand).expect("found");
        for g in Good::all() { assert_eq!(sim.buildings.stores.capacity(g), 0.0, "no phantom base capacity before stores exist"); }
        assert_eq!(problems(&sim).iter().find(|p| p.kind == ProblemKind::NoStorage).expect("NoStorage").severity, Severity::Urgent);
        for x in cell.0..cell.0 + 4 { for y in cell.1..cell.1 + 4 { if let Some(v) = sim.water.depth.get_mut(x, y) { *v = 0.25; } } }
        for (i, key) in ["StickPile", "FoodCache"].into_iter().enumerate() {
            sim.buildings.buildings.push(crate::buildings::Building::new(BuildingId(100 + i as u64), BuildingKind::of(key), (cell.0 + i * 2, cell.1), crate::buildings::Status::Complete));
        }
        sim.buildings.update_capacity(crate::buildings::Terrain { depth: &sim.water.depth });
        assert!(sim.buildings.stores.capacity(Good::food()) > 0.0 && sim.buildings.stores.capacity(crate::dams::material()) > 0.0);
        assert!(!problems(&sim).iter().any(|p| p.kind == ProblemKind::NoStorage));
    }
}
