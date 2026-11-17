//! Colony system: beavers, families, lodges, needs, jobs, work orders and experience.
//!
//! The colony is self-contained: it never reads other systems directly. Terrain is supplied through
//! the [`Landscape`] trait (implemented for the water depth grid, so `colony.step_on(dt, &water.depth)`
//! works). `Colony::step(dt)` uses a built-in river landscape so `SimState` can drive it unchanged.
//!
//! Units: time is in days (365 per year); positions are world cells; every need is `0.0..=1.0` where
//! `1.0` means fully satisfied. All randomness comes from the colony's own seeded `Pcg32`.

use std::collections::{HashMap, HashSet, VecDeque};
use std::fmt;

use rand_core::{Rng, SeedableRng};
use rand_pcg::Pcg32;
pub use crate::tuning::colony::*;

pub const DAYS_PER_YEAR: f64 = 365.0;
const SUBSTEP_DAYS: f32 = 0.5;
const EVENT_LOG_CAP: usize = 2000;

// ───────────────────────────── identifiers and enums ─────────────────────────────

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct BeaverId(pub u64);
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct FamilyId(pub u64);
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct LodgeId(pub u64);
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct OrderId(pub u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Sex {
    Male,
    Female,
}

impl Sex {
    fn opposite(self) -> Sex {
        match self {
            Sex::Male => Sex::Female,
            Sex::Female => Sex::Male,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Stage {
    Kit,
    Yearling,
    Adult,
}

impl Stage {
    pub fn of_age_days(age_days: f32) -> Stage {
        if age_days < KIT_DAYS {
            Stage::Kit
        } else if age_days < ADULT_DAYS {
            Stage::Yearling
        } else {
            Stage::Adult
        }
    }
    /// Share of a full adult's work output.
    fn work_weight(self) -> f32 {
        match self {
            Stage::Kit => 0.0,
            Stage::Yearling => 0.5,
            Stage::Adult => 1.0,
        }
    }
    /// Share of a full adult's food needs.
    fn food_weight(self) -> f32 {
        match self {
            Stage::Kit => 0.4,
            Stage::Yearling => 0.7,
            Stage::Adult => 1.0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Job {
    Forager,
    Logger,
    Builder,
    Digger,
    Hauler,
    Guard,
    Crafter,
    Scout,
}

impl Job {
    pub const ALL: [Job; 8] =
        [Job::Forager, Job::Logger, Job::Builder, Job::Digger, Job::Hauler, Job::Guard, Job::Crafter, Job::Scout];
    pub fn index(self) -> usize {
        self as usize
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeathCause {
    Starvation,
    Cold,
    Injury,
    Exhaustion,
    OldAge,
}

/// Harm sources, indexed like `Beaver::harm`.
const HARM_CAUSES: [DeathCause; 5] =
    [DeathCause::Starvation, DeathCause::Cold, DeathCause::Injury, DeathCause::Exhaustion, DeathCause::OldAge];

// ───────────────────────────── errors ─────────────────────────────

#[derive(Clone, Debug, PartialEq)]
pub enum ColonyError {
    InvalidDt(f32),
    UnknownBeaver(BeaverId),
    UnknownFamily(FamilyId),
    UnknownOrder(OrderId),
    /// Argument was not acceptable (message says which and why).
    Invalid(String),
    AlreadyFounded,
}

impl fmt::Display for ColonyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ColonyError::InvalidDt(d) => write!(f, "invalid dt_days: {d} (must be finite and >= 0)"),
            ColonyError::UnknownBeaver(id) => write!(f, "unknown beaver {}", id.0),
            ColonyError::UnknownFamily(id) => write!(f, "unknown family {}", id.0),
            ColonyError::UnknownOrder(id) => write!(f, "unknown work order {}", id.0),
            ColonyError::Invalid(m) => write!(f, "invalid argument: {m}"),
            ColonyError::AlreadyFounded => write!(f, "colony already founded"),
        }
    }
}
impl std::error::Error for ColonyError {}

// ───────────────────────────── landscape ─────────────────────────────

/// Terrain queries the colony needs. Positions are continuous world coordinates.
pub trait Landscape {
    fn is_water(&self, x: f32, y: f32) -> bool;

    /// Distance to the nearest water: `0.0` in water, `MAX_WATER_SCAN` (10) if none within range.
    fn water_distance(&self, x: f32, y: f32) -> f32 {
        if self.is_water(x, y) {
            return 0.0;
        }
        let mut r = 1.0;
        while r <= MAX_WATER_SCAN {
            for k in 0..16 {
                let a = k as f32 * std::f32::consts::TAU / 16.0;
                if self.is_water(x + r * a.cos(), y + r * a.sin()) {
                    return r;
                }
            }
            r += 1.0;
        }
        MAX_WATER_SCAN
    }
}

/// Water-depth grid: a cell is water if deeper than 0.05; out of bounds is dry.
impl Landscape for crate::grid::Grid<f32> {
    fn is_water(&self, x: f32, y: f32) -> bool {
        if !(x >= 0.0 && y >= 0.0) {
            return false;
        }
        self.get(x as usize, y as usize).is_some_and(|d| *d > 0.05)
    }
}

/// A straight east-west river centred on `y`, used when no water grid is supplied.
#[derive(Clone, Copy, Debug)]
pub struct RiverLand {
    pub y: f32,
    pub half_width: f32,
}
impl Default for RiverLand {
    fn default() -> Self {
        Self { y: 0.0, half_width: 2.0 }
    }
}
impl Landscape for RiverLand {
    fn is_water(&self, _x: f32, y: f32) -> bool {
        (y - self.y).abs() <= self.half_width
    }
}

// ───────────────────────────── data ─────────────────────────────

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Needs {
    pub food: f32,
    pub rest: f32,
    /// Warmth, driven by shelter quality and the season.
    pub warmth: f32,
    pub safety: f32,
    pub social: f32,
}

impl Needs {
    fn satisfied() -> Self {
        Self { food: 1.0, rest: 1.0, warmth: 1.0, safety: 1.0, social: 0.7 }
    }
    fn mood(&self) -> f32 {
        0.4 * self.food + 0.2 * self.warmth + 0.15 * self.rest + 0.15 * self.safety + 0.1 * self.social
    }
}

#[derive(Clone, Debug)]
pub struct Beaver {
    pub id: BeaverId,
    pub name: String,
    pub age_days: f32,
    pub sex: Sex,
    pub family: FamilyId,
    pub mother: Option<BeaverId>,
    pub father: Option<BeaverId>,
    pub mate: Option<BeaverId>,
    /// Skill per job (indexed by `Job::index`), `0.0..=1.0`, grows with work.
    pub skills: [f32; 8],
    pub needs: Needs,
    pub mood: f32,
    pub health: f32,
    /// Player-assigned job; `None` follows the family's priorities.
    pub job: Option<Job>,
    /// Job actually worked on during the latest step.
    pub current_job: Option<Job>,
    pub lodge: Option<LodgeId>,
    pub pos: (f32, f32),
    pub lifespan_days: f32,
    pub stage: Stage,
    pub resting: bool,
    pub last_birth_year: i64,
    low_mood_days: f32,
    /// Accumulated health damage by source (see `HARM_CAUSES`), used to name the cause of death.
    harm: [f32; 5],
}

impl Beaver {
    pub fn age_years(&self) -> f32 {
        self.age_days / DAYS_PER_YEAR as f32
    }
    pub fn skill(&self, job: Job) -> f32 {
        self.skills[job.index()]
    }
}

#[derive(Clone, Debug)]
pub struct Family {
    pub id: FamilyId,
    pub name: String,
    /// Jobs handed out round-robin to family members without a personal job.
    pub priorities: Vec<Job>,
}

#[derive(Clone, Debug)]
pub struct Lodge {
    pub id: LodgeId,
    pub family: FamilyId,
    pub pos: (f32, f32),
    /// Structural quality `0.0..=1.0`; wears down, repaired by builders using wood.
    pub condition: f32,
}

#[derive(Clone, Debug)]
pub struct WorkOrder {
    pub id: OrderId,
    pub job: Job,
    pub pos: (f32, f32),
    /// Work units still needed (one full adult working day is about 0.6 units).
    pub remaining: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Event {
    Born { id: BeaverId, family: FamilyId },
    Matured { id: BeaverId, stage: Stage },
    Died { id: BeaverId, cause: DeathCause },
    Departed { id: BeaverId },
    Immigrated { id: BeaverId },
    NewFamily { family: FamilyId, lodge: LodgeId, a: BeaverId, b: BeaverId },
    NoLodgeSite { near: (f32, f32) },
    OrderDone { id: OrderId, job: Job },
    Collapsed(CollapseReport),
}

#[derive(Clone, Debug, PartialEq)]
pub struct LoggedEvent {
    pub day: f64,
    pub event: Event,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CollapseCause {
    /// Nobody is left.
    Extinct,
    /// Only kits and yearlings remain; no one can work or breed to sustain the colony.
    NoAdults,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CollapseReport {
    pub day: f64,
    pub cause: CollapseCause,
    pub peak_population: usize,
    pub deaths: u32,
    pub departures: u32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Stats {
    pub births: u32,
    pub deaths: u32,
    pub departures: u32,
    pub immigrants: u32,
    pub peak_population: usize,
}

#[derive(Clone, Copy, Debug)]
struct SiteEval {
    /// Travel cost: land counts 1 per cell, swimming 0.5.
    cost: f32,
    land_len: f32,
    water_dist: f32,
}

#[derive(Clone, Copy, Debug)]
struct Work {
    job: Job,
    units: f32,
    order: Option<OrderId>,
    eval: SiteEval,
    site: (f32, f32),
}

type Cell = (i32, i32);

#[derive(Clone, Debug)]
pub struct Colony {
    rng: Pcg32,
    pub time_days: f64,
    next_id: u64,
    pub beavers: Vec<Beaver>,
    pub families: Vec<Family>,
    pub lodges: Vec<Lodge>,
    pub food: f32,
    pub wood: f32,
    /// Tool quality `0..=1` from crafters; boosts production.
    pub tools: f32,
    /// Total canal work dug; lowers land risk.
    pub dug: f32,
    /// Total scouting done; improves foraging.
    pub scouted: f32,
    pub orders: VecDeque<WorkOrder>,
    /// Colony experience: total work units done per job (indexed by `Job::index`).
    pub experience: [f32; 8],
    pub stats: Stats,
    pub events: VecDeque<LoggedEvent>,
    pub events_dropped: u64,
    collapse: Option<CollapseReport>,
    founded: bool,
    site_cache: HashMap<(Cell, Cell), SiteEval>,
    cache_stamp: f64,
    guard_strength: f32,
}

impl Default for Colony {
    fn default() -> Self {
        Self::new(0)
    }
}

const FIRST_NAMES: [&str; 24] = [
    "Alder", "Birch", "Clover", "Dusk", "Elm", "Fern", "Gorse", "Hazel", "Iris", "Juniper", "Kelp", "Linden",
    "Moss", "Nettle", "Oak", "Pebble", "Quill", "Reed", "Sedge", "Thistle", "Umber", "Willow", "Yarrow", "Zinnia",
];
const FAMILY_NAMES: [&str; 12] = [
    "Stillwater", "Mudbank", "Fallowdam", "Rushmere", "Oakhollow", "Brookbend", "Sedgefield", "Ashpool",
    "Cattail", "Lowfen", "Marshgate", "Riverbrook",
];

fn cell(p: (f32, f32)) -> Cell {
    (p.0.round() as i32, p.1.round() as i32)
}

fn day_of_year(time_days: f64) -> f64 {
    time_days.rem_euclid(DAYS_PER_YEAR)
}

/// Seasonal temperature in -1 (midwinter) ..= 1 (midsummer).
fn temperature(time_days: f64) -> f32 {
    (std::f64::consts::TAU * (day_of_year(time_days) - 200.0) / DAYS_PER_YEAR).cos() as f32
}

fn check_point(name: &str, p: (f32, f32)) -> Result<(), ColonyError> {
    if p.0.is_finite() && p.1.is_finite() {
        Ok(())
    } else {
        Err(ColonyError::Invalid(format!("{name} position must be finite")))
    }
}

impl Colony {
    /// Creates an empty, unfounded colony with its own deterministic random stream.
    pub fn new(seed: u64) -> Self {
        Self {
            rng: Pcg32::seed_from_u64(seed ^ 0xC0_10_4E),
            time_days: 0.0,
            next_id: 1,
            beavers: Vec::new(),
            families: Vec::new(),
            lodges: Vec::new(),
            food: 0.0,
            wood: 0.0,
            tools: 0.0,
            dug: 0.0,
            scouted: 0.0,
            orders: VecDeque::new(),
            experience: [0.0; 8],
            stats: Stats::default(),
            events: VecDeque::new(),
            events_dropped: 0,
            collapse: None,
            founded: false,
            site_cache: HashMap::new(),
            cache_stamp: 0.0,
            guard_strength: 0.0,
        }
    }

    // ───── queries ─────

    pub fn population(&self) -> usize {
        self.beavers.len()
    }
    pub fn count_stage(&self, stage: Stage) -> usize {
        self.beavers.iter().filter(|b| b.stage == stage).count()
    }
    pub fn beaver(&self, id: BeaverId) -> Option<&Beaver> {
        self.beavers.iter().find(|b| b.id == id)
    }
    pub fn family(&self, id: FamilyId) -> Option<&Family> {
        self.families.iter().find(|f| f.id == id)
    }
    pub fn is_founded(&self) -> bool {
        self.founded
    }
    pub fn is_collapsed(&self) -> bool {
        self.collapse.is_some()
    }
    /// The collapse report, once the colony has collapsed.
    pub fn collapse(&self) -> Option<&CollapseReport> {
        self.collapse.as_ref()
    }
    /// Removes and returns all logged events (oldest first).
    pub fn take_events(&mut self) -> Vec<LoggedEvent> {
        self.events.drain(..).collect()
    }
    pub fn food_per_capita(&self) -> f32 {
        self.food / self.beavers.len().max(1) as f32
    }

    // ───── player commands ─────

    /// Founds the colony: a pair of adults and their two yearlings in one lodge at `pos` (dry land).
    pub fn found(&mut self, pos: (f32, f32), land: &dyn Landscape) -> Result<FamilyId, ColonyError> {
        if self.founded {
            return Err(ColonyError::AlreadyFounded);
        }
        check_point("founding", pos)?;
        if land.is_water(pos.0, pos.1) {
            return Err(ColonyError::Invalid("founding position is in water".into()));
        }
        let year = DAYS_PER_YEAR as f32;
        let fam = self.new_family();
        let lodge = self.new_lodge(fam, pos, 1.0);
        let father = self.spawn(Sex::Male, 3.0 * year, fam, Some(lodge), pos, None, None);
        let mother = self.spawn(Sex::Female, 3.0 * year, fam, Some(lodge), pos, None, None);
        self.set_mates(father, mother);
        self.spawn(Sex::Male, 1.5 * year, fam, Some(lodge), pos, Some(father), Some(mother));
        self.spawn(Sex::Female, 1.5 * year, fam, Some(lodge), pos, Some(father), Some(mother));
        self.food = 40.0;
        self.wood = 20.0;
        self.founded = true;
        self.stats.peak_population = self.beavers.len();
        Ok(fam)
    }

    /// Sets (or clears with `None`) a beaver's personal job.
    pub fn set_job(&mut self, id: BeaverId, job: Option<Job>) -> Result<(), ColonyError> {
        let b = self.beavers.iter_mut().find(|b| b.id == id).ok_or(ColonyError::UnknownBeaver(id))?;
        b.job = job;
        Ok(())
    }

    /// Replaces a family's job priorities. Must be non-empty and without duplicates.
    pub fn set_family_priorities(&mut self, id: FamilyId, jobs: Vec<Job>) -> Result<(), ColonyError> {
        if jobs.is_empty() {
            return Err(ColonyError::Invalid("priorities must not be empty".into()));
        }
        if jobs.iter().collect::<HashSet<_>>().len() != jobs.len() {
            return Err(ColonyError::Invalid("priorities must not contain duplicates".into()));
        }
        let f = self.families.iter_mut().find(|f| f.id == id).ok_or(ColonyError::UnknownFamily(id))?;
        f.priorities = jobs;
        Ok(())
    }

    /// Queues a work order at `pos` needing `work` units; beavers on `job` pick orders first-in-first-out.
    pub fn queue_order(&mut self, job: Job, pos: (f32, f32), work: f32) -> Result<OrderId, ColonyError> {
        check_point("order", pos)?;
        if !work.is_finite() || work <= 0.0 {
            return Err(ColonyError::Invalid("order work must be finite and > 0".into()));
        }
        let id = OrderId(self.next_id);
        self.next_id += 1;
        self.orders.push_back(WorkOrder { id, job, pos, remaining: work });
        Ok(id)
    }

    pub fn cancel_order(&mut self, id: OrderId) -> Result<(), ColonyError> {
        let n = self.orders.len();
        self.orders.retain(|o| o.id != id);
        if self.orders.len() == n {
            return Err(ColonyError::UnknownOrder(id));
        }
        Ok(())
    }

    // ───── stepping ─────

    /// Advances by `dt_days` on the built-in river landscape. Panics on invalid dt, which
    /// `SimState::step` has already rejected.
    pub fn step(&mut self, dt_days: f32) {
        if let Err(e) = self.step_on(dt_days, &RiverLand::default()) {
            panic!("Colony::step called with unvalidated dt: {e}");
        }
    }

    /// Advances by `dt_days` on `land`, in substeps of at most half a day.
    pub fn step_on(&mut self, dt_days: f32, land: &dyn Landscape) -> Result<(), ColonyError> {
        if !dt_days.is_finite() || dt_days < 0.0 {
            return Err(ColonyError::InvalidDt(dt_days));
        }
        let mut left = dt_days;
        while left > 0.0 {
            let dt = left.min(SUBSTEP_DAYS);
            self.tick(dt, land);
            left -= dt;
        }
        Ok(())
    }

    fn tick(&mut self, dt: f32, land: &dyn Landscape) {
        self.time_days += dt as f64;
        if !self.founded {
            return;
        }
        if self.time_days - self.cache_stamp >= CACHE_LIFETIME_DAYS {
            self.site_cache.clear();
            self.cache_stamp = self.time_days;
        }
        let temp = temperature(self.time_days);
        let cold = ((0.2 - temp) / 1.2).clamp(0.0, 1.0);
        let forage_season = (0.75 + 0.25 * temp).clamp(0.5, 1.0);

        self.adopt_orphans();
        let plan = self.plan_work(dt, land);
        self.do_work(dt, &plan, forage_season);
        self.roll_hazards(dt, &plan);
        self.update_needs(dt, cold);
        self.wear(dt, cold);
        self.age_and_stage(dt);
        self.remove_dead();
        self.pair_and_disperse(dt, land);
        self.births();
        self.depart_unhappy();
        self.families.retain({
            let live: HashSet<FamilyId> = self.beavers.iter().map(|b| b.family).collect();
            move |f| live.contains(&f.id)
        });
        self.check_collapse();
    }

    // ───── work planning ─────

    fn effective_job(&self, b: &Beaver, rank_in_family: usize) -> Option<Job> {
        if b.stage == Stage::Kit || b.resting {
            return None;
        }
        if let Some(j) = b.job {
            return Some(j);
        }
        if self.food_per_capita() < 4.0 {
            return Some(Job::Forager); // unassigned workers feed a hungry colony first
        }
        let failing = self.lodges.iter().any(|l| l.condition < 0.6);
        if failing && self.wood >= 2.0 && rank_in_family % 2 == 1 {
            return Some(Job::Builder); // keep roofs over heads before anything optional
        }
        let fam = self.family(b.family)?;
        Some(fam.priorities[rank_in_family % fam.priorities.len()])
    }

    fn lodge_pos(&self, b: &Beaver) -> (f32, f32) {
        b.lodge.and_then(|l| self.lodges.iter().find(|x| x.id == l)).map_or(b.pos, |l| l.pos)
    }

    fn eval_site(&mut self, from: (f32, f32), to: (f32, f32), land: &dyn Landscape) -> SiteEval {
        if let Some(e) = self.site_cache.get(&(cell(from), cell(to))) {
            return *e;
        }
        let dist = ((to.0 - from.0).powi(2) + (to.1 - from.1).powi(2)).sqrt();
        let steps = dist.ceil().max(1.0) as usize;
        let seg = dist / steps as f32;
        let (mut cost, mut land_len) = (0.0, 0.0);
        for i in 0..steps {
            let t = (i as f32 + 0.5) / steps as f32;
            let (x, y) = (from.0 + (to.0 - from.0) * t, from.1 + (to.1 - from.1) * t);
            if land.is_water(x, y) {
                cost += seg * 0.5; // swimming is twice as fast
            } else {
                cost += seg;
                land_len += seg;
            }
        }
        let e = SiteEval { cost, land_len, water_dist: land.water_distance(to.0, to.1) };
        self.site_cache.insert((cell(from), cell(to)), e);
        e
    }

    fn plan_work(&mut self, dt: f32, land: &dyn Landscape) -> Vec<Option<Work>> {
        let mut ranks: HashMap<FamilyId, usize> = HashMap::new();
        let mut plan = Vec::with_capacity(self.beavers.len());
        for i in 0..self.beavers.len() {
            let b = self.beavers[i].clone();
            let rank = *ranks.entry(b.family).and_modify(|r| *r += 1).or_insert(0);
            let Some(job) = self.effective_job(&b, rank) else {
                plan.push(None);
                continue;
            };
            let home = self.lodge_pos(&b);
            let order = self.orders.iter().find(|o| o.job == job && o.remaining > 0.0).map(|o| (o.id, o.pos));
            let site = order.map_or_else(
                || {
                    let k = job.index() as f32;
                    let a = k * std::f32::consts::FRAC_PI_4 + b.family.0 as f32;
                    (home.0 + (3.0 + k) * a.cos(), home.1 + (3.0 + k) * a.sin())
                },
                |(_, p)| p,
            );
            let eval = self.eval_site(home, site, land);
            let efficiency = 1.0 / (1.0 + 0.02 * eval.cost);
            let units = dt
                * WORK_FRACTION
                * b.stage.work_weight()
                * efficiency
                * (0.3 + 0.7 * b.needs.rest)
                * (0.5 + 0.5 * b.health)
                * (0.7 + 0.3 * b.mood);
            plan.push(Some(Work { job, units, order: order.map(|o| o.0), eval, site }));
        }
        plan
    }

    fn do_work(&mut self, dt: f32, plan: &[Option<Work>], forage_season: f32) {
        // Colony-wide helpers reflect this step's hauler and guard effort.
        let (mut carry, mut guard) = (0.0, 0.0);
        for (b, w) in self.beavers.iter().zip(plan) {
            if let Some(w) = w {
                let effort = w.units / dt * (0.5 + b.skill(w.job));
                match w.job {
                    Job::Hauler => carry += effort,
                    Job::Guard => guard += effort,
                    _ => {}
                }
            }
        }
        self.guard_strength = guard;
        // Foraging grounds thin out as the colony grows, which bounds the population.
        let forage_season = forage_season / (1.0 + self.beavers.len() as f32 / 100.0);
        let boost = 1.0 + (0.15 * carry).min(0.6) + 0.3 * self.tools.clamp(0.0, 1.0);
        let scout_bonus = 1.0 + 0.1 * (self.scouted / 20.0).min(2.0);
        let food_cap = 10.0 + 25.0 * self.beavers.len() as f32;
        let wood_cap = 60.0 + 10.0 * self.beavers.len() as f32;

        let mut order_work: HashMap<OrderId, f32> = HashMap::new();
        for i in 0..self.beavers.len() {
            let Some(w) = plan[i] else {
                self.beavers[i].current_job = None;
                continue;
            };
            let power = w.units * (0.5 + self.beavers[i].skill(w.job));
            match w.job {
                Job::Forager => self.food = (self.food + power * 4.0 * forage_season * boost * scout_bonus).min(food_cap),
                Job::Logger => self.wood = (self.wood + power * 1.2 * boost).min(wood_cap),
                Job::Builder => self.repair(power * 0.04 * boost),
                Job::Digger => self.dug += power,
                Job::Crafter => self.tools = (self.tools + power * 0.05).min(1.0),
                Job::Scout => self.scouted += power,
                Job::Hauler | Job::Guard => {}
            }
            let b = &mut self.beavers[i];
            b.current_job = Some(w.job);
            b.pos = w.site;
            let s = &mut b.skills[w.job.index()];
            *s = (*s + w.units * 0.01 * (1.0 - *s)).min(1.0);
            self.experience[w.job.index()] += w.units;
            if let Some(o) = w.order {
                *order_work.entry(o).or_insert(0.0) += w.units;
            }
        }
        let mut done = Vec::new();
        for o in self.orders.iter_mut() {
            if let Some(u) = order_work.get(&o.id) {
                o.remaining -= u;
                if o.remaining <= 0.0 {
                    done.push((o.id, o.job));
                }
            }
        }
        self.orders.retain(|o| o.remaining > 0.0);
        for (id, job) in done {
            self.log(Event::OrderDone { id, job });
        }
    }

    /// Spends wood (4 per condition point) to raise the worst lodge's condition by up to `amount`.
    fn repair(&mut self, amount: f32) {
        let Some(l) =
            self.lodges.iter_mut().filter(|l| l.condition < 1.0).min_by(|a, b| a.condition.total_cmp(&b.condition))
        else {
            return;
        };
        let gain = amount.min(1.0 - l.condition).min(self.wood / 4.0);
        l.condition += gain;
        self.wood -= gain * 4.0;
    }

    /// Working far from water, and long land trips, risk injury; guards and canals lower it.
    fn roll_hazards(&mut self, dt: f32, plan: &[Option<Work>]) {
        let mitigation = (1.0 + 0.4 * self.guard_strength) * (1.0 + 0.03 * self.dug.min(10.0));
        for i in 0..self.beavers.len() {
            let Some(w) = plan[i] else { continue };
            let per_day = 0.004 + 0.003 * w.eval.land_len + 0.01 * w.eval.water_dist.min(MAX_WATER_SCAN);
            let hit = self.chance(per_day * WORK_FRACTION * dt / mitigation);
            let b = &mut self.beavers[i];
            let target = (1.0 - 6.0 * per_day / mitigation).clamp(0.2, 1.0);
            b.needs.safety += (target - b.needs.safety) * (0.4 * dt).min(1.0);
            if hit {
                b.health -= 0.15;
                b.harm[2] += 0.15;
                b.needs.safety = (b.needs.safety - 0.3).max(0.0);
            }
        }
    }

    // ───── needs, health, mood ─────

    fn update_needs(&mut self, dt: f32, cold: f32) {
        let mut occupancy: HashMap<LodgeId, usize> = HashMap::new();
        for b in &self.beavers {
            if let Some(l) = b.lodge {
                *occupancy.entry(l).or_insert(0) += 1;
            }
        }
        let guarded = (self.guard_strength * 0.2).min(0.2);
        let mut food = self.food;
        for b in self.beavers.iter_mut() {
            let condition = b.lodge.and_then(|l| self.lodges.iter().find(|x| x.id == l)).map(|l| l.condition);
            let insulation = condition.map_or(0.05, |c| 0.05 + 0.85 * c);
            let n = &mut b.needs;

            // food: hunger, then eat from the stockpile when below 70%
            n.food = (n.food - 0.25 * dt).max(0.0);
            if n.food < 0.7 {
                let w = b.stage.food_weight();
                let take = ((1.0 - n.food) * w).min(food);
                food -= take;
                n.food += take / w;
            }
            // rest: work tires, shelter restores; exhausted beavers stop working until recovered
            let worked = if b.current_job.is_some() { WORK_FRACTION } else { 0.1 };
            n.rest = (n.rest - 0.20 * worked * dt + 0.40 * (1.0 - worked) * insulation * dt).clamp(0.0, 1.0);
            if b.resting {
                n.rest = (n.rest + 0.3 * insulation * dt).min(1.0);
                b.resting = n.rest < 0.6;
            } else if n.rest < 0.25 {
                b.resting = true;
            }
            // warmth: relax toward what the lodge can hold against the season
            n.warmth += (1.0 - cold * (1.0 - insulation) - n.warmth) * (0.5 * dt).min(1.0);
            // safety recovers when not working in danger
            if b.current_job.is_none() {
                n.safety += ((0.9 + guarded).min(1.0) - n.safety) * (0.4 * dt).min(1.0);
            }
            // social: company in the lodge
            let mates = b.lodge.map_or(0, |l| occupancy.get(&l).copied().unwrap_or(1) - 1).min(4);
            n.social += ((0.2 + 0.2 * mates as f32).min(1.0) - n.social) * (0.4 * dt).min(1.0);

            // health: neglect wears it down slowly, care restores it
            let frail = if b.stage == Stage::Kit { 1.5 } else { 1.0 };
            let mut loss = 0.0;
            for (k, rate, hurting) in [
                (0, 0.02 * frail, n.food < 0.15),
                (1, 0.015 * frail, n.warmth < 0.2),
                (3, 0.01, n.rest < 0.05),
                (4, 0.0004 * (b.age_days / DAYS_PER_YEAR as f32 - 8.0), b.age_days > 8.0 * DAYS_PER_YEAR as f32),
            ] {
                if hurting {
                    b.harm[k] += rate * dt;
                    loss += rate * dt;
                }
            }
            b.health -= loss;
            if loss == 0.0 && n.food > 0.5 && n.warmth > 0.5 {
                b.health += 0.01 * dt;
            }
            b.health = b.health.min(1.0);

            b.mood += (n.mood() * (0.5 + 0.5 * b.health) - b.mood) * (0.5 * dt).min(1.0);
            b.low_mood_days = if b.mood < 0.2 { b.low_mood_days + dt } else { 0.0 };
        }
        self.food = food * (1.0 - 0.003 * dt); // spoilage
        self.tools = (self.tools - 0.003 * dt).max(0.0);
    }

    fn wear(&mut self, dt: f32, cold: f32) {
        for l in self.lodges.iter_mut() {
            l.condition = (l.condition - 0.002 * (1.0 + 2.0 * cold) * dt).max(0.0);
        }
    }

    // ───── life cycle ─────

    fn age_and_stage(&mut self, dt: f32) {
        let mut matured = Vec::new();
        for b in self.beavers.iter_mut() {
            b.age_days += dt;
            let s = Stage::of_age_days(b.age_days);
            if s != b.stage {
                b.stage = s;
                matured.push((b.id, s));
            }
        }
        for (id, stage) in matured {
            self.log(Event::Matured { id, stage });
        }
    }

    fn remove_dead(&mut self) {
        let mut dead = Vec::new();
        for b in &self.beavers {
            if b.age_days >= b.lifespan_days {
                dead.push((b.id, DeathCause::OldAge));
            } else if b.health <= 0.0 {
                // The harm source that did the most damage is the cause.
                let (k, _) = b.harm.iter().enumerate().max_by(|a, c| a.1.total_cmp(c.1)).expect("non-empty");
                dead.push((b.id, HARM_CAUSES[k]));
            }
        }
        let ids: Vec<BeaverId> = dead.iter().map(|d| d.0).collect();
        self.stats.deaths += dead.len() as u32;
        self.remove_beavers(&ids);
        for (id, cause) in dead {
            self.log(Event::Died { id, cause });
        }
    }

    /// Adults and yearlings in a bad mood for a month leave the colony for good.
    fn depart_unhappy(&mut self) {
        let leaving: Vec<BeaverId> = self
            .beavers
            .iter()
            .filter(|b| b.stage != Stage::Kit && b.low_mood_days >= DEPARTURE_BAD_MOOD_DAYS)
            .map(|b| b.id)
            .collect();
        self.stats.departures += leaving.len() as u32;
        self.remove_beavers(&leaving);
        for id in leaving {
            self.log(Event::Departed { id });
        }
    }

    fn remove_beavers(&mut self, ids: &[BeaverId]) {
        if ids.is_empty() {
            return;
        }
        let set: HashSet<BeaverId> = ids.iter().copied().collect();
        self.beavers.retain(|b| !set.contains(&b.id));
        for b in self.beavers.iter_mut().filter(|b| b.mate.is_some_and(|m| set.contains(&m))) {
            b.mate = None;
        }
    }

    /// Lodgeless beavers move into the best lodge with room (their family's first).
    fn adopt_orphans(&mut self) {
        let mut occupancy: HashMap<LodgeId, usize> = HashMap::new();
        for b in &self.beavers {
            if let Some(l) = b.lodge {
                *occupancy.entry(l).or_insert(0) += 1;
            }
        }
        for b in self.beavers.iter_mut().filter(|b| b.lodge.is_none()) {
            let pick = self
                .lodges
                .iter()
                .filter(|l| occupancy.get(&l.id).copied().unwrap_or(0) < LODGE_CAPACITY)
                .max_by(|x, y| ((x.family == b.family), x.condition).partial_cmp(&((y.family == b.family), y.condition)).unwrap());
            if let Some(l) = pick {
                b.lodge = Some(l.id);
                b.pos = l.pos;
                *occupancy.entry(l.id).or_insert(0) += 1;
            }
        }
    }

    /// Spring litters: each healthy, housed, paired adult female bears 1–3 kits once a year.
    fn births(&mut self) {
        let doy = day_of_year(self.time_days);
        if !(BIRTH_FROM_DAY..SPRING_END).contains(&doy) || self.food_per_capita() < 4.0 {
            return;
        }
        let year = (self.time_days / DAYS_PER_YEAR) as i64;
        let mothers: Vec<Beaver> = self
            .beavers
            .iter()
            .filter(|b| {
                b.sex == Sex::Female
                    && b.stage == Stage::Adult
                    && b.age_years() < 10.0
                    && b.last_birth_year != year
                    && b.health >= 0.5
                    && b.needs.food >= 0.4
                    && b.lodge.is_some()
                    && b.mate.is_some_and(|m| self.beavers.iter().any(|x| x.id == m && x.family == b.family))
            })
            .cloned()
            .collect();
        for m in mothers {
            if let Some(b) = self.beavers.iter_mut().find(|b| b.id == m.id) {
                b.last_birth_year = year;
            }
            for _ in 0..1 + self.range(3) {
                let sex = if self.chance(0.5) { Sex::Male } else { Sex::Female };
                let id = self.spawn(sex, 0.0, m.family, m.lodge, m.pos, m.mate, Some(m.id));
                self.stats.births += 1;
                self.log(Event::Born { id, family: m.family });
            }
        }
        self.stats.peak_population = self.stats.peak_population.max(self.beavers.len());
    }

    /// In spring, grown unpaired beavers without dependants pair with a non-relative (another
    /// family's beaver, or a wanderer who arrives) and leave to found a new family and lodge.
    fn pair_and_disperse(&mut self, dt: f32, land: &dyn Landscape) {
        let doy = day_of_year(self.time_days);
        if !(SPRING_START..SPRING_END).contains(&doy) {
            return;
        }
        let parents: HashSet<BeaverId> =
            self.beavers.iter().filter(|b| b.stage != Stage::Adult).flat_map(|b| [b.mother, b.father]).flatten().collect();
        let eligible: Vec<(BeaverId, Sex, FamilyId)> = self
            .beavers
            .iter()
            .filter(|b| b.stage == Stage::Adult && b.mate.is_none() && b.health >= 0.5 && !parents.contains(&b.id))
            .map(|b| (b.id, b.sex, b.family))
            .collect();
        let mut used: HashSet<BeaverId> = HashSet::new();
        for &(a, sex, family) in &eligible {
            if used.contains(&a) || self.wood < LODGE_WOOD_COST {
                continue;
            }
            let partner = eligible.iter().find(|(b, s, f)| !used.contains(b) && *s != sex && *f != family).map(|p| p.0);
            if partner.is_none() && !(self.food_per_capita() >= 4.0 && self.chance(0.04 * dt)) {
                continue;
            }
            let near = self.beaver(a).map_or((0.0, 0.0), |b| self.lodge_pos(b));
            let Some(site) = self.find_lodge_site(near, land) else {
                if !self.events.iter().rev().take(20).any(|e| matches!(e.event, Event::NoLodgeSite { .. })) {
                    self.log(Event::NoLodgeSite { near });
                }
                continue;
            };
            // Only now that the pair can actually leave does a wanderer arrive.
            let b = partner.unwrap_or_else(|| self.immigrant(sex.opposite()));
            used.insert(a);
            used.insert(b);
            self.wood -= LODGE_WOOD_COST;
            let fam = self.new_family();
            let lodge = self.new_lodge(fam, site, 0.7);
            for x in self.beavers.iter_mut().filter(|x| x.id == a || x.id == b) {
                x.family = fam;
                x.lodge = Some(lodge);
                x.pos = site;
            }
            self.set_mates(a, b);
            self.log(Event::NewFamily { family: fam, lodge, a, b });
        }
    }

    fn immigrant(&mut self, sex: Sex) -> BeaverId {
        let age = (2.0 + 2.0 * self.unit()) * DAYS_PER_YEAR as f32;
        let fam = self.new_family(); // replaced as soon as they pair
        let id = self.spawn(sex, age, fam, None, (0.0, 0.0), None, None);
        self.stats.immigrants += 1;
        self.stats.peak_population = self.stats.peak_population.max(self.beavers.len());
        self.log(Event::Immigrated { id });
        id
    }

    fn find_lodge_site(&mut self, near: (f32, f32), land: &dyn Landscape) -> Option<(f32, f32)> {
        let start = self.unit() * std::f32::consts::TAU;
        for r in [6.0_f32, 8.0, 10.0, 12.0, 14.0, 18.0] {
            for k in 0..12 {
                let a = start + k as f32 * std::f32::consts::TAU / 12.0;
                let p = (near.0 + r * a.cos(), near.1 + r * a.sin());
                let clear = self.lodges.iter().all(|l| (l.pos.0 - p.0).hypot(l.pos.1 - p.1) >= MIN_LODGE_SPACING);
                if clear && !land.is_water(p.0, p.1) && land.water_distance(p.0, p.1) <= 3.0 {
                    return Some(p);
                }
            }
        }
        None
    }

    /// Detects and reports collapse once: nobody left, or nobody old enough to work or breed.
    fn check_collapse(&mut self) {
        if self.collapse.is_some() {
            return;
        }
        let cause = if self.beavers.is_empty() {
            CollapseCause::Extinct
        } else if !self.beavers.iter().any(|b| b.stage == Stage::Adult) {
            CollapseCause::NoAdults
        } else {
            return;
        };
        let report = CollapseReport {
            day: self.time_days,
            cause,
            peak_population: self.stats.peak_population,
            deaths: self.stats.deaths,
            departures: self.stats.departures,
        };
        self.collapse = Some(report.clone());
        self.log(Event::Collapsed(report));
    }

    // ───── helpers ─────

    fn new_family(&mut self) -> FamilyId {
        let id = FamilyId(self.next_id);
        self.next_id += 1;
        let name = FAMILY_NAMES[self.range(FAMILY_NAMES.len() as u32) as usize].to_string();
        let priorities =
            vec![Job::Forager, Job::Forager, Job::Logger, Job::Builder, Job::Hauler, Job::Guard, Job::Crafter, Job::Scout];
        self.families.push(Family { id, name, priorities });
        id
    }

    fn new_lodge(&mut self, family: FamilyId, pos: (f32, f32), condition: f32) -> LodgeId {
        let id = LodgeId(self.next_id);
        self.next_id += 1;
        self.lodges.push(Lodge { id, family, pos, condition });
        id
    }

    #[allow(clippy::too_many_arguments)]
    fn spawn(
        &mut self,
        sex: Sex,
        age_days: f32,
        family: FamilyId,
        lodge: Option<LodgeId>,
        pos: (f32, f32),
        father: Option<BeaverId>,
        mother: Option<BeaverId>,
    ) -> BeaverId {
        let id = BeaverId(self.next_id);
        self.next_id += 1;
        let name = FIRST_NAMES[self.range(FIRST_NAMES.len() as u32) as usize].to_string();
        let lifespan_days = (10.0 + 4.0 * self.unit()) * DAYS_PER_YEAR as f32;
        let mut skills = [0.0; 8];
        if age_days >= ADULT_DAYS {
            for s in skills.iter_mut() {
                *s = 0.1 * self.unit();
            }
        }
        self.beavers.push(Beaver {
            id,
            name,
            age_days,
            sex,
            family,
            mother,
            father,
            mate: None,
            skills,
            needs: Needs::satisfied(),
            mood: 0.9,
            health: 1.0,
            job: None,
            current_job: None,
            lodge,
            pos,
            lifespan_days,
            stage: Stage::of_age_days(age_days),
            resting: false,
            last_birth_year: -1,
            low_mood_days: 0.0,
            harm: [0.0; 5],
        });
        id
    }

    fn set_mates(&mut self, a: BeaverId, b: BeaverId) {
        for (x, y) in [(a, b), (b, a)] {
            if let Some(v) = self.beavers.iter_mut().find(|v| v.id == x) {
                v.mate = Some(y);
            }
        }
    }

    fn log(&mut self, event: Event) {
        if self.events.len() >= EVENT_LOG_CAP {
            self.events.pop_front();
            self.events_dropped += 1;
        }
        self.events.push_back(LoggedEvent { day: self.time_days, event });
    }

    fn unit(&mut self) -> f32 {
        (self.rng.next_u32() >> 8) as f32 / (1u32 << 24) as f32
    }
    fn chance(&mut self, p: f32) -> bool {
        self.unit() < p
    }
    fn range(&mut self, n: u32) -> u32 {
        self.rng.next_u32() % n
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn founded(seed: u64) -> (Colony, RiverLand) {
        let land = RiverLand::default();
        let mut c = Colony::new(seed);
        c.found((0.0, 5.0), &land).unwrap();
        (c, land)
    }

    fn run(c: &mut Colony, land: &RiverLand, days: usize) {
        for _ in 0..days {
            c.step_on(1.0, land).unwrap();
        }
    }

    fn born_ids(c: &Colony) -> HashSet<BeaverId> {
        c.events.iter().filter_map(|e| if let Event::Born { id, .. } = e.event { Some(id) } else { None }).collect()
    }

    #[test]
    fn colony_kits_mature() {
        let (mut c, land) = founded(7);
        run(&mut c, &land, 365 * 4);
        assert!(c.stats.births > 0, "no kits were born");
        let born = born_ids(&c);
        let grown = c.events.iter().any(|e| matches!(e.event, Event::Matured { id, stage: Stage::Adult } if born.contains(&id)));
        assert!(grown, "no colony-born kit reached adulthood in 4 years");
    }

    #[test]
    fn colony_new_lodges_form() {
        let (mut c, land) = founded(7);
        run(&mut c, &land, 365 * 6);
        assert!(c.lodges.len() > 1, "no new lodge formed");
        assert!(c.families.len() > 1);
        assert!(c.events.iter().any(|e| matches!(e.event, Event::NewFamily { .. })));
    }

    #[test]
    fn colony_starved_declines_and_collapse_is_reported() {
        let (mut c, land) = founded(3);
        c.food = 0.0;
        let ids: Vec<BeaverId> = c.beavers.iter().map(|b| b.id).collect();
        for id in ids {
            c.set_job(id, Some(Job::Hauler)).unwrap(); // nobody forages
        }
        let start = c.population();
        run(&mut c, &land, 60);
        assert!(c.beavers.iter().all(|b| b.health < 1.0), "starving beavers should lose health");
        run(&mut c, &land, 200);
        assert!(c.population() < start);
        let report = c.collapse().expect("collapse should be reported");
        assert_eq!(report.cause, CollapseCause::Extinct);
        assert!(report.deaths as usize >= start);
        assert!(c.events.iter().any(|e| matches!(e.event, Event::Died { cause: DeathCause::Starvation, .. })));
    }

    #[test]
    fn colony_skill_grows_with_work() {
        let (mut c, land) = founded(1);
        let id = c.beavers[0].id;
        c.set_job(id, Some(Job::Logger)).unwrap();
        let before = c.beaver(id).unwrap().skill(Job::Logger);
        run(&mut c, &land, 60);
        assert!(c.beaver(id).unwrap().skill(Job::Logger) > before + 0.05);
        assert!(c.experience[Job::Logger.index()] > 1.0);
    }

    #[test]
    fn colony_orders_complete_and_far_work_is_riskier() {
        let (mut c, land) = founded(1);
        let id = c.queue_order(Job::Digger, (0.0, 1.0), 1.0).unwrap();
        let ids: Vec<BeaverId> = c.beavers.iter().map(|b| b.id).collect();
        for b in ids {
            c.set_job(b, Some(Job::Digger)).unwrap();
        }
        run(&mut c, &land, 5);
        assert!(c.orders.iter().all(|o| o.id != id), "order should be done");
        assert!(c.events.iter().any(|e| matches!(e.event, Event::OrderDone { .. })));
        let near = c.eval_site((0.0, 5.0), (0.0, 2.0), &land);
        let far = c.eval_site((0.0, 5.0), (30.0, 40.0), &land);
        assert!(far.water_dist > near.water_dist && far.land_len > near.land_len);
        let swim = c.eval_site((-5.0, 0.0), (5.0, 0.0), &land);
        assert!(swim.cost < 10.0 * 0.6, "swimming should cost about half of walking");
    }

    #[test]
    fn colony_rejects_bad_input() {
        let (mut c, land) = founded(1);
        assert_eq!(c.found((0.0, 5.0), &land), Err(ColonyError::AlreadyFounded));
        assert!(matches!(c.step_on(f32::NAN, &land), Err(ColonyError::InvalidDt(_))));
        assert!(matches!(c.set_job(BeaverId(9999), None), Err(ColonyError::UnknownBeaver(_))));
        let fam = c.families[0].id;
        assert!(c.set_family_priorities(fam, vec![]).is_err());
        assert!(c.set_family_priorities(fam, vec![Job::Guard, Job::Guard]).is_err());
        assert!(c.set_family_priorities(FamilyId(9999), vec![Job::Guard]).is_err());
        assert!(c.queue_order(Job::Logger, (0.0, 0.0), 0.0).is_err());
        assert!(c.queue_order(Job::Logger, (f32::NAN, 0.0), 1.0).is_err());
        assert!(matches!(c.cancel_order(OrderId(9999)), Err(ColonyError::UnknownOrder(_))));
        assert!(Colony::new(1).found((0.0, 0.0), &land).is_err()); // in water
    }

    #[test]
    fn colony_is_deterministic() {
        let (mut a, land) = founded(5);
        let (mut b, _) = founded(5);
        run(&mut a, &land, 800);
        run(&mut b, &land, 800);
        assert_eq!(a.population(), b.population());
        assert_eq!(a.food, b.food);
    }
}
