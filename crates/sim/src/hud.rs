use crate::agents::{Assignment, Task};
use crate::buildings::{BuildingId, BuildingKind, Good, Producer, Use, WET_DEPTH};
use crate::colony::{BeaverId, DeathCause, Stage};
use crate::jobs::RoleKind;
use crate::resources::ResourceKind;
use crate::society::RequestId;
use crate::water::{Cell, Season};
use crate::work::Role;
use crate::{Command, SimState};
pub use crate::forestry::{forest_report, ForestReport};

pub const MAX_PROBLEMS: usize = 4;
pub const FOOD_WARN_DAYS: f32 = 20.0;
pub const FULL_SHARE: f32 = 0.98;
pub const WEAK_DAM: f32 = 0.6;
pub const FIX_WORKERS: usize = 2;
pub const FOOD_GATHER_RADIUS: usize = 80;
pub const WINTER_RESERVE_DAYS: f32 = 90.0;

#[cfg(test)]
mod repair_alert_tests {
    use super::*;
    #[test]
    fn winter_planning_exposes_storage_and_gathering_before_an_immediate_shortage() {
        let mut s=SimState::new(1);
        s.colony.found((5.0,5.0),&crate::grid::Grid::new(8,8,0.0)).unwrap();
        let adult=s.colony.beavers[0].clone();
        for i in 0..8 {let mut b=adult.clone();b.id=BeaverId(100+i);s.colony.beavers.push(b);}
        for b in &s.colony.beavers {s.agents.spawn(b.id,(5,5));}
        s.resources.items.clear();s.resources.add(ResourceKind::Plant,(35,35),4.0);
        let hut=crate::buildings::Building{id:BuildingId(1),kind:BuildingKind::ForagerHut,pos:(5,5),status:crate::buildings::Status::Complete,output:0.0,condition:1.0,reinforced:false,dam_profile:Vec::new()};
        s.buildings.buildings.push(hut);
        assert!(winter_food_problem(&s).is_none());
        s.buildings.stores.deposit(Good::Food,100.0).unwrap();
        s.time_days=190.0;
        assert!(problems(&s).iter().any(|p|p.kind==ProblemKind::WinterFood));
        assert!(!problems(&s).iter().any(|p|p.kind==ProblemKind::Food));
        let p=winter_food_problem(&s).unwrap();
        assert_eq!(p.fix,Fix::Place(BuildingKind::FoodCache));
        assert!(matches!(p.alt_fix,Some(Fix::Gather{kind:ResourceKind::Plant,..})));
        s.buildings.buildings.push(crate::buildings::Building{id:BuildingId(2),kind:BuildingKind::FoodCache,status:crate::buildings::Status::Site,..s.buildings.buildings[0].clone()});
        assert!(matches!(winter_food_problem(&s).unwrap().fix,Fix::Gather{..}));
        s.time_days=280.0;assert!(winter_food_problem(&s).is_none());
        s.time_days=190.0;s.colony.beavers.truncate(4);
        assert!(winter_food_problem(&s).is_none());
    }
    #[test]
    fn food_forecast_accounts_for_decay_without_inventing_a_finite_runout_at_equilibrium() {
        assert_eq!(food_runout_days(100.0,2.0,1.0,0.0),100.0);
        assert!((food_runout_days(100.0,2.0,1.0,0.01)-69.31472).abs()<1e-4);
        assert_eq!(food_runout_days(0.0,2.0,1.0,0.01),0.0);
        assert!(food_runout_days(100.0,1.0,1.0,0.01).is_infinite());
        assert!(food_runout_days(100.0,1.0,2.0,0.01).is_infinite());
    }
    #[test]
    fn food_alert_does_not_staff_an_exhausted_area() {
        let mut s=SimState::new(1);
        s.colony.found((5.0,5.0),&crate::grid::Grid::new(8,8,0.0)).unwrap();
        for b in &s.colony.beavers {s.agents.spawn(b.id,(5,5));}
        s.resources.items.clear();
        let b=crate::buildings::Building{id:BuildingId(1),kind:BuildingKind::ForagerHut,pos:(5,5),status:crate::buildings::Status::Complete,output:0.0,condition:1.0,reinforced:false,dam_profile:Vec::new()};
        *s.water.depth.get_mut(5,5).unwrap()=0.2;
        s.work.add_complete(&b).unwrap();s.buildings.buildings.push(b);
        assert_eq!(food_fix(&s),Fix::Focus);
        s.resources.add(ResourceKind::Plant,(35,35),4.0);
        assert!(matches!(food_fix(&s),Fix::Gather{kind:ResourceKind::Plant,..}));
        s.resources.add(ResourceKind::Plant,(6,6),4.0);
        assert!(matches!(food_fix(&s),Fix::AddWorkers{building:BuildingId(1),..}));
    }
    #[test]
    fn emergency_food_response_scales_with_workers_and_respects_locked_orders() {
        let mut s=SimState::new(1);
        s.colony.found((5.0,5.0),&crate::grid::Grid::new(8,8,0.0)).unwrap();
        s.resources.items.clear();s.resources.add(ResourceKind::Plant,(35,35),4.0);
        let adult=s.colony.beavers[0].clone();
        for i in 0..28 {
            let mut b=adult.clone();b.id=BeaverId(100+i);
            if i>=12 {b.stage=Stage::Kit;}
            s.colony.beavers.push(b);
        }
        for b in &s.colony.beavers {s.agents.spawn(b.id,(5,5));}
        assert_eq!(food_fix(&s),Fix::Gather{kind:ResourceKind::Plant,n:4});
        for b in s.colony.beavers.iter().filter(|b|b.stage!=Stage::Kit).skip(1) {s.work.player_orders.insert(b.id);}
        assert_eq!(food_fix(&s),Fix::Gather{kind:ResourceKind::Plant,n:1});
        let id=s.colony.beavers[0].id;s.work.player_orders.insert(id);
        assert_eq!(food_fix(&s),Fix::Focus);
    }
    #[test]
    fn missing_materials_do_not_offer_an_impossible_builder_fix() {
        let mut s=SimState::new(1);s.colony.found((0.0,3.0),&crate::colony::RiverLand::default()).unwrap();let id=BuildingId(1);let c=(5,5);
        s.water.build_dam(&[c],1.0).unwrap();*s.water.dam_integrity.get_mut(c.0,c.1).unwrap()=0.2;
        s.buildings.buildings.push(crate::buildings::Building{id,kind:BuildingKind::Dam,pos:c,status:crate::buildings::Status::Site,output:0.0,condition:0.2,reinforced:false,dam_profile:vec![(c,1.0)]});
        s.work.add_site(id,BuildingKind::Dam,&[(Good::Clay,5.0)],1.0).unwrap();
        s.work.dam_plans.push(crate::work::DamPlan{building:id,repair:true,cells:vec![c],height:1.0,heights:vec![1.0],from:vec![1.0]});
        let fix=|s:&SimState|problems(s).into_iter().find(|p|p.kind==ProblemKind::WeakDam).unwrap().fix;
        assert_eq!(fix(&s),Fix::Focus);
        s.work.supplies=Some((1,1));s.work.stock_add(crate::agents::SUPPLIES,Good::Clay,5.0);s.buildings.stores.deposit(Good::Clay,5.0).unwrap();
        assert_eq!(fix(&s),Fix::StaffSites);
    }
}

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

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Source {
    Building(BuildingId),
    Beaver(BeaverId),
    Cell(Cell),
    None,
}

#[derive(Clone, Debug, PartialEq)]
pub struct BreakdownRow {
    pub label: String,
    pub value: f32,
    pub source: Source,
}

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
    let stock = sim.buildings.stores.amount(Good::Food);
    let need = food_need_per_day(sim);
    let gain = sim.ledger.in_per_day(Good::Food);
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
    food_runout_days(sim.buildings.stores.amount(Good::Food),food_need_per_day(sim),
        sim.ledger.in_per_day(Good::Food),crate::tuning::colony::FOOD_SPOILAGE)
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

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    Urgent,
    Warning,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Fix {
    AddWorkers { building: BuildingId, n: usize },
    Gather { kind: ResourceKind, n: usize },
    Place(BuildingKind),
    StaffSites,
    RepairDam { cell: Cell },
    RepairBuilding { building: BuildingId },
    PutToWork { role: RoleKind, n: usize },
    OpenJobs,
    Accept(RequestId),
    Decline(RequestId),
    Focus,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProblemKind {
    Food,
    WinterFood,
    Idle,
    NoBuilders,
    StoreFull,
    WeakDam,
    Maintenance,
    DamFailed,
    Stranger,
    LoggingAreaRunningOut,
    ForestFalling,
    HighAndDry,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Subject {
    Good(Good),
    Building(BuildingKind),
    Stage(Stage),
}

#[derive(Clone, Debug, PartialEq)]
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
        .filter(|b| matches!(b.stage, Stage::Adult | Stage::Yearling) && !sim.work.is_locked(b.id))
        .filter_map(|b| sim.agents.get(b.id).map(|a| (crate::jobs::is_busy(sim, b.id), a.cell.0.abs_diff(home.0) + a.cell.1.abs_diff(home.1), b.id)))
        .collect();
    v.sort();
    v.into_iter().take(n).map(|t| t.2).collect()
}

fn food_fix(sim: &SimState) -> Fix {
    let hut = sim
        .work
        .places
        .iter()
        .filter(|p| p.kind == BuildingKind::ForagerHut && !p.paused && sim.buildings.building(p.building).is_some_and(|b| b.is_complete()))
        .filter(|p| crate::agents::gathering_available(sim, Assignment::Slot(p.building,0)))
        .filter(|p| crate::jobs::add_worker_preview(sim, p.building).is_some())
        .max_by_key(|p| (p.priority, std::cmp::Reverse(p.building)));
    if let Some(p) = hut {
        let free = p.slots.iter().filter(|s| s.worker.is_none()).count();
        return Fix::AddWorkers { building: p.building, n: free.min(FIX_WORKERS) };
    }
    // Extra staff in an exhausted work area cannot resolve a food shortage.
    // Send available workers to food in the wider emergency gathering area.
    // A fixed pair is too small to respond to a mature colony's winter deficit.
    // The player explicitly moves a quarter of the workforce; locked orders stay put.
    let workers=sim.colony.beavers.iter().filter(|b|b.stage!=Stage::Kit).count();
    let n = gatherers(sim, workers.div_ceil(4).max(FIX_WORKERS)).len();
    if let Some(home) = colony_cell(sim).filter(|_|n>0) {
        let r=FOOD_GATHER_RADIUS;
        let area=crate::work::Area{min:(home.0.saturating_sub(r),home.1.saturating_sub(r)),max:(home.0+r,home.1+r)};
        if crate::agents::gathering_available(sim,Assignment::Area(ResourceKind::Plant,area)) {
            return Fix::Gather {kind:ResourceKind::Plant,n};
        }
    }
    let building_one = sim.buildings.buildings.iter().any(|b| b.kind == BuildingKind::ForagerHut && !b.is_complete());
    if sim.can_build(BuildingKind::ForagerHut) && !building_one {
        return Fix::Place(BuildingKind::ForagerHut);
    }
    Fix::Focus
}

fn winter_food_problem(sim:&SimState)->Option<Problem> {
    if crate::calendar::Season::at(sim.time_days)!=crate::calendar::Season::Autumn {return None;}
    let need=food_need_per_day(sim);
    let stock=sim.buildings.stores.amount(Good::Food);
    let target=need*WINTER_RESERVE_DAYS;
    if need<=0.0 || stock>=target {return None;}
    let capacity=sim.buildings.stores.capacity(Good::Food);
    let gathering=food_fix(sim);
    let building_cache=sim.buildings.buildings.iter().any(|b|b.kind==BuildingKind::FoodCache&&!b.is_complete());
    let add_cache=capacity<target && !building_cache && sim.can_build(BuildingKind::FoodCache);
    Some(Problem {
        kind:ProblemKind::WinterFood,severity:Severity::Warning,value:stock/need,
        cause:Breakdown::new(stock/need,vec![row("Stored food",stock,Source::None),row("Storage capacity",capacity,Source::None),row("Winter reserve goal",target,Source::None),row("Daily appetite",need,Source::None)]),
        subject:Some(Subject::Good(Good::Food)),
        fix:if add_cache {Fix::Place(BuildingKind::FoodCache)} else {gathering},
        alt_fix:if add_cache&&gathering!=Fix::Focus {Some(gathering)} else {None},focus:colony_cell(sim),
    })
}

pub fn problems(sim: &SimState) -> Vec<Problem> {
    let mut out = Vec::new();
    if !sim.colony.is_founded() {
        return out;
    }
    let idle = idle_beavers(sim);
    let pop = sim.colony.population();

    let need = food_need_per_day(sim);
    let gain = sim.ledger.in_per_day(Good::Food);
    if pop > 0 && need > gain {
        let days = food_days_left(sim);
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
    if !out.iter().any(|p|p.kind==ProblemKind::Food) {
        if let Some(winter)=winter_food_problem(sim) {out.push(winter);}
    }

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
            fix: if let Some(plan)=sim.work.dam_plans.iter().find(|p|p.cells.contains(&worst)) {
                if sim.work.place(plan.building).is_some_and(|p|crate::agents::site_stalled(sim,p)) {Fix::Focus} else {Fix::StaffSites}
            } else { Fix::RepairDam { cell: worst } },
            alt_fix: None,
            focus: Some(worst),
        });
    }

    if let Some(b)=sim.buildings.buildings.iter().filter(|b|b.is_complete() && b.kind!=BuildingKind::Dam
        && !matches!(b.kind,BuildingKind::Canal|BuildingKind::Barge)
        && (b.kind!=BuildingKind::Burrow || sim.work.homes.get(&b.id).is_some_and(|id|sim.colony.beavers.iter().any(|a|a.lodge==Some(*id))))
        && b.condition<0.55 && !sim.maintenance.jobs.contains_key(&b.id))
        .min_by(|a,b|a.condition.total_cmp(&b.condition)) {
        let value=b.condition*100.0;
        out.push(Problem{kind:ProblemKind::Maintenance,severity:if b.condition<0.25{Severity::Urgent}else{Severity::Warning},value,
            cause:Breakdown::new(value,vec![row("Condition",value,Source::Building(b.id))]),subject:Some(Subject::Building(b.kind)),
            fix:Fix::RepairBuilding{building:b.id},alt_fix:None,focus:Some(b.pos)});
    }

    let dry: Vec<&crate::buildings::Building> = sim.buildings.buildings.iter().filter(|b| sim.buildings.stranded(b, &sim.water.depth)).collect();
    if let Some(first) = dry.iter().find(|b| matches!(b.kind, BuildingKind::Lodge | BuildingKind::Burrow)).or(dry.first()) {
        let home = matches!(first.kind, BuildingKind::Lodge | BuildingKind::Burrow);
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

    let stalled: Vec<BuildingId> = sim
        .work
        .places
        .iter()
        .filter(|p| crate::agents::is_site(sim, p) && !p.paused && p.slots.iter().all(|s| s.worker.is_none()) && !crate::agents::site_stalled(sim, p))
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
                fix: if sim.can_build(BuildingKind::Warehouse) { Fix::Place(BuildingKind::Warehouse) } else { Fix::Focus },
                alt_fix: None,
                focus: colony_cell(sim),
            });
            break;
        }
    }

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
            fix: if let Some(p) = sim.work.places.iter().find(|p| p.kind == BuildingKind::ForesterLodge && p.filled() < p.slots.len()) {
                Fix::AddWorkers { building: p.building, n: 2 }
            } else if sim.can_build(BuildingKind::ForesterLodge) && sim.buildings.buildings.iter().filter(|b| b.kind == BuildingKind::ForesterLodge).count() < 2 {
                Fix::Place(BuildingKind::ForesterLodge)
            } else { Fix::OpenJobs },
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
                while sim.work.place(b).is_some_and(|p| p.filled() < p.slots.len()) && sim.apply(Command::AddWorker { building: b }).is_ok() {
                    placed += 1;
                }
            }
            if placed == 0 { Err("no beaver is free to build".into()) } else { Ok(()) }
        }
        Fix::PutToWork { role, n } => sim.apply(Command::AdjustRole { role, delta: n.max(1) as i32 }).map_err(err),
        Fix::RepairDam { cell } => sim.apply(Command::RepairDam { cell }).map_err(err),
        Fix::RepairBuilding { building } => sim.apply(Command::RepairBuilding { building }).map_err(err),
        Fix::Accept(id) => sim.apply(Command::AnswerJoin { id, answer: crate::society::Answer::Accept }).map_err(err),
        Fix::Decline(id) => sim.apply(Command::AnswerJoin { id, answer: crate::society::Answer::TurnAway }).map_err(err),
        Fix::Place(_) | Fix::OpenJobs | Fix::Focus => Ok(()),
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Factor {
    pub label: String,
    pub value: String,
    pub ok: Option<bool>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Report {
    pub measured: Vec<(Good, f32)>,
    pub steady: bool,
    pub ceiling: Option<(Good, f32)>,
    pub factors: Vec<Factor>,
}

fn factor(label: impl Into<String>, value: impl Into<String>, ok: Option<bool>) -> Factor {
    Factor { label: label.into(), value: value.into(), ok }
}

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
        let kinds = kind.gather_sources();
        let good = if kind == ResourceKind::Tree { Good::Sticks } else { kind.yields().first().copied()? };
        factors.push(factor("Workers", format!("{filled} of {total}"), Some(filled == total)));
        if let Some(area) = place.and_then(|p| p.area) {
            let left = sim.resources.items.iter().filter(|r| kinds.contains(&r.kind) && area.contains(r.cell) && r.has_yield()).count();
            let what = match kind {
                ResourceKind::Tree | ResourceKind::Felled => "Trees in its area",
                ResourceKind::Plant => "Food plants in its area",
                ResourceKind::MudBank => "Clay sources in its area",
                ResourceKind::RiverClay => "River clay spots in its area",
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
        factors.push(factor("Crafters", format!("{filled} of {total}"), Some(filled > 0)));
        factors.push(factor("Local work support", format!("+{:.0}%", b.output * 25.0), Some(b.output > 0.1)));
        for &(g, a) in crate::craftsmanship::COST_PER_DAY {
            factors.push(factor(format!("{g:?} per crafter day"), format!("{a:.2}"), Some(sim.buildings.stores.amount(g) >= a)));
        }
        return Some(Report {measured, steady, ceiling: None, factors});
    }
    None
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ProfilePoint {
    pub cell: Cell,
    pub bed: f32,
    pub crest: f32,
    pub surface: f32,
}

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

pub fn profile_with_preview(points: &[ProfilePoint], preview: &crate::dam_preview::DamPreview) -> Vec<f32> {
    points
        .iter()
        .map(|p| if preview.flooded.binary_search_by_key(&(p.cell.1, p.cell.0), |c| (c.1, c.0)).is_ok() { p.surface.max(preview.level) } else { p.surface })
        .collect()
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Ponds {
    pub dammed: bool,
    pub cells: usize,
    pub volume: f32,
    pub summer_loss_per_day: f32,
}

impl Ponds {
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
