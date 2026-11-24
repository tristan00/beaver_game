use std::collections::{HashMap, HashSet, VecDeque};
use std::fmt;

use rand_core::{Rng, SeedableRng};
use rand_pcg::Pcg32;
pub use crate::tuning::colony::*;

pub const DAYS_PER_YEAR: f64 = crate::calendar::YEAR_DAYS;

#[cfg(test)]
mod migration_tests {
    use super::*;
    #[test]
    fn all_arrivals_share_one_annual_allowance() {
        let mut c=Colony::default();c.found((0.0,3.0),&RiverLand::default()).unwrap();
        c.add_home((15.0,0.0));c.add_home((30.0,0.0));
        assert_eq!(MAX_IMMIGRANTS_PER_YEAR,1,"one newcomer a year");
        c.admit_wanderer(true).unwrap();
        assert_eq!(c.migration_remaining(),0);assert!(c.admit_wanderer(true).is_err());assert_eq!(c.add_immigrants(10),0);
        c.time_days=360.0;assert_eq!(c.migration_remaining(),1);assert_eq!(c.immigration_by_year.get(&0),Some(&1));
        c.beavers.clear();c.check_collapse();assert!(c.is_collapsed());
        c.time_days=720.0;assert_eq!(c.migration_remaining(),0);assert_eq!(c.add_immigrants(2),0);
    }
}

#[cfg(test)]
mod leave_home_tests {
    use super::*;

    #[test]
    fn unpaired_young_move_out_at_two_into_an_empty_lodge() {
        let mut c = Colony::default();
        c.found((0.0, 3.0), &RiverLand::default()).unwrap();
        let (mother, home) = (c.beavers[0].id, c.beavers[0].lodge);
        let year = DAYS_PER_YEAR as f32;
        let mut young = |i: usize, age_years: f32| {
            let b = &mut c.beavers[i];
            (b.mother, b.father, b.mate, b.lodge, b.stage, b.age_days) = (Some(mother), None, None, home, Stage::Adult, age_years * year);
            b.id
        };
        let (old, not_yet) = (young(2, 2.2), young(3, 1.9));
        // No empty lodge: it stays home.
        c.leave_home();
        assert_eq!(c.beaver(old).unwrap().lodge, home);
        let empty = c.add_home((15.0, 0.0));
        c.leave_home();
        let b = c.beaver(old).unwrap();
        assert_eq!(b.lodge, Some(empty), "a two-year-old with no mate moves out");
        assert_ne!(b.family, c.beaver(mother).unwrap().family, "it heads a family of its own");
        assert!(c.events.iter().any(|e| matches!(e.event, Event::LeftHome { id, lodge } if id == old && lodge == empty)));
        assert_eq!(c.beaver(not_yet).unwrap().lodge, home, "under two it stays");
        assert_eq!(c.beaver(mother).unwrap().lodge, home);
    }
}

#[cfg(test)]
mod health_tests {
    use super::*;

    #[test]
    fn assigned_work_trains_skills_without_creating_resources_or_moving_workers() {
        let mut c = Colony::default();
        let land = RiverLand::default();
        c.found((0.0, 3.0), &land).unwrap();
        let worker = c.beavers[0].id;
        c.slot_jobs.insert(worker, (Job::Forager, true));
        let before = (c.food, c.wood, c.beavers[0].pos);
        let skill = c.beavers[0].skill(Job::Forager);
        let plan = c.plan_work(0.5, &land);
        assert_eq!(plan.iter().flatten().count(), 1);
        c.do_work(&plan);
        assert_eq!((c.food, c.wood, c.beavers[0].pos), before);
        assert!(c.beavers[0].skill(Job::Forager) > skill);
        assert_eq!(c.beavers[0].current_job, Some(Job::Forager));
        c.beavers[0].resting = true;
        assert!(c.plan_work(0.5, &land)[0].is_none());
    }

    #[test]
    fn work_safety_never_damages_health() {
        let mut c = Colony::default();
        c.found((0.0, 3.0), &RiverLand::default()).unwrap();
        let work = Work { job: Job::Logger, units: 1.0,
            eval: SiteEval { land_len: 100.0, water_dist: MAX_WATER_SCAN } };
        let plan = vec![Some(work); c.beavers.len()];
        let initial = c.beavers.iter().map(|b| (b.health,b.harm)).collect::<Vec<_>>();
        for _ in 0..2000 { c.update_work_safety(0.5, &plan); }
        assert_eq!(initial,c.beavers.iter().map(|b| (b.health,b.harm)).collect::<Vec<_>>());
        assert!(c.beavers.iter().all(|b| b.needs.safety < 1.0));
    }

    #[test]
    fn care_heals_survivors_but_cannot_revive_a_fatality() {
        let mut c = Colony::default();
        c.found((0.0, 3.0), &RiverLand::default()).unwrap();
        let dead = c.beavers[0].id;
        c.beavers[0].health = 0.0;
        c.beavers[0].harm[0] = 1.0;
        c.beavers[1].health = 0.5;
        c.update_needs(0.5, 0.0, &mut crate::buildings::Stores::default());
        assert_eq!(c.beavers[0].health, 0.0);
        assert!(c.beavers[1].health > 0.5);
        c.remove_dead();
        assert!(c.beaver(dead).is_none());
        assert!(c.events.iter().any(|e| matches!(e.event,
            Event::Died { id, cause: DeathCause::Starvation, .. } if id == dead)));
    }
}
const SUBSTEP_DAYS: f32 = 0.5;
const EVENT_LOG_CAP: usize = 2000;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
pub struct BeaverId(pub u64);
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
pub struct FamilyId(pub u64);
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
pub struct LodgeId(pub u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum Sex {
    Male,
    Female,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
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
    pub(crate) fn work_weight(self) -> f32 {
        match self {
            Stage::Kit => KIT_WORK_WEIGHT,
            Stage::Yearling => YEARLING_WORK_WEIGHT,
            Stage::Adult => 1.0,
        }
    }
    pub fn food_weight(self) -> f32 {
        match self {
            Stage::Kit => KIT_FOOD_WEIGHT,
            Stage::Yearling => YEARLING_FOOD_WEIGHT,
            Stage::Adult => 1.0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
pub enum Job {
    Forager,
    Logger,
    Builder,
    Digger,
    Hauler,
}

impl Job {
    pub const ALL: [Job; 5] = [Job::Forager, Job::Logger, Job::Builder, Job::Digger, Job::Hauler];
    pub fn index(self) -> usize {
        self as usize
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum DeathCause {
    Starvation,
    Cold,
    Injury,
    Exhaustion,
    OldAge,
    Predator,
}

const HARM_CAUSES: [DeathCause; 5] =
    [DeathCause::Starvation, DeathCause::Cold, DeathCause::Injury, DeathCause::Exhaustion, DeathCause::OldAge];

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum ColonyError {
    InvalidDt(f32),
    Invalid(String),
    AlreadyFounded,
}

impl fmt::Display for ColonyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ColonyError::InvalidDt(d) => write!(f, "invalid dt_days: {d} (must be finite and >= 0)"),
            ColonyError::Invalid(m) => write!(f, "invalid argument: {m}"),
            ColonyError::AlreadyFounded => write!(f, "colony already founded"),
        }
    }
}
impl std::error::Error for ColonyError {}

pub trait Landscape {
    fn is_water(&self, x: f32, y: f32) -> bool;

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

impl Landscape for crate::grid::Grid<f32> {
    fn is_water(&self, x: f32, y: f32) -> bool {
        if !(x >= 0.0 && y >= 0.0) {
            return false;
        }
        self.get(x as usize, y as usize).is_some_and(|d| *d > WATER_DEPTH)
    }
}

/// A straight river along y = 0, for tests that need water without a map.
#[cfg(test)]
#[derive(Clone, Copy, Debug, serde::Serialize, serde::Deserialize)]
pub struct RiverLand {
    pub y: f32,
    pub half_width: f32,
}
#[cfg(test)]
impl Default for RiverLand {
    fn default() -> Self {
        Self { y: 0.0, half_width: 2.0 }
    }
}
#[cfg(test)]
impl Landscape for RiverLand {
    fn is_water(&self, _x: f32, y: f32) -> bool {
        (y - self.y).abs() <= self.half_width
    }
}

#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Needs {
    pub food: f32,
    pub rest: f32,
    pub warmth: f32,
    pub safety: f32,
    pub social: f32,
}

impl Needs {
    fn satisfied() -> Self {
        Self { food: 1.0, rest: 1.0, warmth: 1.0, safety: 1.0, social: START_SOCIAL }
    }
    fn mood(&self) -> f32 {
        MOOD_FOOD_WEIGHT * self.food
            + MOOD_WARMTH_WEIGHT * self.warmth
            + MOOD_REST_WEIGHT * self.rest
            + MOOD_SAFETY_WEIGHT * self.safety
            + MOOD_SOCIAL_WEIGHT * self.social
    }
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Pregnancy {
    pub father: BeaverId,
    pub conceived_day: f64,
    pub due_day: f64,
    pub litter: u32,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Beaver {
    pub id: BeaverId,
    pub name: String,
    pub age_days: f32,
    age_remainder: f64,
    pub sex: Sex,
    pub family: FamilyId,
    pub mother: Option<BeaverId>,
    pub father: Option<BeaverId>,
    pub mate: Option<BeaverId>,
    pub skills: [f32; Job::ALL.len()],
    pub needs: Needs,
    pub mood: f32,
    pub health: f32,
    pub job: Option<Job>,
    pub current_job: Option<Job>,
    pub lodge: Option<LodgeId>,
    pub pos: (f32, f32),
    pub lifespan_days: f32,
    pub stage: Stage,
    pub resting: bool,
    pub last_birth_year: i64,
    pub pregnancy: Option<Pregnancy>,
    low_mood_days: f32,
    harm: [f32; 5],
}

impl Beaver {
    /// Harm taken so far: starvation, cold, injury, exhaustion, old age.
    pub fn harm(&self) -> [f32; 5] {
        self.harm
    }
    pub fn low_mood_days(&self) -> f32 {
        self.low_mood_days
    }
    pub fn age_years(&self) -> f32 {
        self.age_days / DAYS_PER_YEAR as f32
    }
    #[cfg(test)]
    pub fn skill(&self, job: Job) -> f32 {
        self.skills[job.index()]
    }
    /// Kits join the work from six months old, at the yearlings' pace.
    pub fn can_work(&self) -> bool {
        self.age_days >= WORK_AGE_DAYS
    }
    pub fn work_weight(&self) -> f32 {
        if self.can_work() { self.stage.work_weight() } else { 0.0 }
    }
    pub fn is_young(&self) -> bool {
        matches!(self.stage, Stage::Kit | Stage::Yearling)
    }
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Family {
    pub id: FamilyId,
    pub name: String,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Lodge {
    pub id: LodgeId,
    pub family: FamilyId,
    pub pos: (f32, f32),
    pub condition: f32,
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum Event {
    Born { id: BeaverId, family: FamilyId },
    Matured { id: BeaverId, stage: Stage },
    Died { id: BeaverId, cause: DeathCause, at: (f32, f32) },
    Departed { id: BeaverId },
    Immigrated { id: BeaverId },
    NewFamily { family: FamilyId, lodge: LodgeId, a: BeaverId, b: BeaverId },
    Collapsed(CollapseReport),
    /// A grown young beaver that found no mate moved out of its parents' lodge into an empty one.
    LeftHome { id: BeaverId, lodge: LodgeId },
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct LoggedEvent {
    pub day: f64,
    pub event: Event,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum CollapseCause {
    Extinct,
    NoAdults,
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CollapseReport {
    pub day: f64,
    pub cause: CollapseCause,
    pub peak_population: usize,
    pub deaths: u32,
    pub departures: u32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Stats {
    pub births: u32,
    pub deaths: u32,
    pub departures: u32,
    pub immigrants: u32,
    pub peak_population: usize,
}

#[derive(Clone, Copy, Debug, serde::Serialize, serde::Deserialize)]
struct SiteEval {
    land_len: f32,
    water_dist: f32,
}

#[derive(Clone, Copy, Debug, serde::Serialize, serde::Deserialize)]
struct Work {
    job: Job,
    units: f32,
    eval: SiteEval,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Colony {
    rng: Pcg32,
    pub time_days: f64,
    next_id: u64,
    pub beavers: Vec<Beaver>,
    pub families: Vec<Family>,
    pub lodges: Vec<Lodge>,
    /// Read-only presentation totals, refreshed from physical stores during a step.
    pub food: f32,
    pub wood: f32,
    pub experience: [f32; Job::ALL.len()],
    pub stats: Stats,
    pub immigration_by_year: std::collections::BTreeMap<i64, u32>,
    /// Parent records survive deaths and dispersal, so family changes cannot
    /// make close relatives eligible to mate.
    pub ancestry: HashMap<BeaverId, (Option<BeaverId>, Option<BeaverId>)>,
    pub events: VecDeque<LoggedEvent>,
    pub events_dropped: u64,
    collapse: Option<CollapseReport>,
    founded: bool,
    pub slot_jobs: HashMap<BeaverId, (Job, bool)>,
    pub eaten: [f32; 4],
    pub spoiled: f32,
    pub repair_wood: f32,
}

impl Default for Colony {
    fn default() -> Self {
        Self::new(0)
    }
}


fn day_of_year(time_days: f64) -> f64 {
    time_days.rem_euclid(DAYS_PER_YEAR)
}

fn temperature(time_days: f64) -> f32 {
    (std::f64::consts::TAU * (day_of_year(time_days) - MIDSUMMER_DAY) / DAYS_PER_YEAR).cos() as f32
}

fn check_point(name: &str, p: (f32, f32)) -> Result<(), ColonyError> {
    if p.0.is_finite() && p.1.is_finite() {
        Ok(())
    } else {
        Err(ColonyError::Invalid(format!("{name} position must be finite")))
    }
}

impl Colony {
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
            experience: [0.0; Job::ALL.len()],
            stats: Stats::default(),
            immigration_by_year: Default::default(),
            ancestry: HashMap::new(),
            events: VecDeque::new(),
            events_dropped: 0,
            collapse: None,
            founded: false,
            slot_jobs: HashMap::new(),
            eaten: [0.0; 4],
            spoiled: 0.0,
            repair_wood: 0.0,
        }
    }

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
    pub fn take_events(&mut self) -> Vec<LoggedEvent> {
        self.events.drain(..).collect()
    }
    pub fn food_per_capita(&self) -> f32 {
        self.food / self.beavers.len().max(1) as f32
    }

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
        let lodge = self.new_lodge(fam, pos, FOUNDING_LODGE_CONDITION);
        let father = self.spawn(Sex::Male, FOUNDER_ADULT_AGE_YEARS * year, fam, Some(lodge), pos, None, None);
        let mother = self.spawn(Sex::Female, FOUNDER_ADULT_AGE_YEARS * year, fam, Some(lodge), pos, None, None);
        self.set_mates(father, mother);
        // The arriving breeding pair mated in the previous winter.
        let litter = MIN_LITTER + self.range(LITTER_SPREAD);
        if let Some(m) = self.beavers.iter_mut().find(|b| b.id == mother) {
            m.pregnancy = Some(Pregnancy { father, conceived_day: -FOUNDER_PREGNANCY_DAYS, due_day: GESTATION_DAYS - FOUNDER_PREGNANCY_DAYS, litter });
        }
        self.spawn(Sex::Male, FOUNDER_YEARLING_AGE_YEARS * year, fam, Some(lodge), pos, Some(father), Some(mother));
        self.spawn(Sex::Female, FOUNDER_YEARLING_AGE_YEARS * year, fam, Some(lodge), pos, Some(father), Some(mother));
        // The founders bring nothing with them.
        self.food = 0.0;
        self.wood = 0.0;
        self.founded = true;
        self.stats.peak_population = self.beavers.len();
        Ok(fam)
    }

    pub fn arrive(&mut self, pos: (f32, f32), land: &dyn Landscape) -> Result<FamilyId, ColonyError> {
        let fam = self.found(pos, land)?;
        self.lodges.clear();
        for b in self.beavers.iter_mut() {
            b.lodge = None;
        }
        Ok(fam)
    }

    pub fn add_home(&mut self, pos: (f32, f32)) -> LodgeId {
        let family = self.beavers.iter().find(|b| b.lodge.is_none()).map(|b| b.family).unwrap_or(FamilyId(0));
        let id = self.new_lodge(family, pos, 1.0);
        self.house_homeless();
        id
    }

    pub fn house_homeless(&mut self) {
        let mut occupancy: HashMap<LodgeId, usize> = HashMap::new();
        for b in &self.beavers {
            if let Some(l) = b.lodge {
                *occupancy.entry(l).or_insert(0) += 1;
            }
        }
        for b in self.beavers.iter_mut().filter(|b| b.lodge.is_none()) {
            let pick = self.lodges.iter_mut()
                .filter(|l| (l.family == b.family || occupancy.get(&l.id).copied().unwrap_or(0) == 0)
                    && occupancy.get(&l.id).copied().unwrap_or(0) < LODGE_CAPACITY)
                .min_by_key(|l| (l.family != b.family, l.id));
            if let Some(l) = pick {
                l.family = b.family;
                b.lodge = Some(l.id);
                *occupancy.entry(l.id).or_insert(0) += 1;
            }
        }
    }

    #[cfg(test)]
    pub fn step(&mut self, dt_days: f32) {
        let mut stores = crate::buildings::Stores::default();
        stores.deposit(crate::buildings::Good::Food, self.food).unwrap();
        stores.deposit(crate::buildings::Good::Sticks, self.wood).unwrap();
        self.step_with_stores(dt_days, &RiverLand::default(), &mut stores).unwrap();
    }

    pub fn step_with_stores(&mut self, dt_days: f32, land: &dyn Landscape, stores: &mut crate::buildings::Stores) -> Result<(), ColonyError> {
        if !dt_days.is_finite() || dt_days < 0.0 { return Err(ColonyError::InvalidDt(dt_days)); }
        self.food = stores.amount(crate::buildings::Good::Food);
        self.wood = stores.amount(crate::buildings::Good::Sticks);
        let mut left = dt_days;
        while left > 0.0 {
            let dt = left.min(SUBSTEP_DAYS);
            self.tick(dt, land, stores);
            left -= dt;
        }
        Ok(())
    }

    fn tick(&mut self, dt: f32, land: &dyn Landscape, stores: &mut crate::buildings::Stores) {
        self.time_days += dt as f64;
        if !self.founded {
            return;
        }
        let temp = temperature(self.time_days);
        let cold = ((COLD_BELOW_TEMP - temp) / COLD_RANGE).clamp(0.0, 1.0);

        self.adopt_orphans();
        let plan = self.plan_work(dt, land);
        self.do_work(&plan);
        self.teach_young(dt);
        self.update_work_safety(dt, &plan);
        self.update_needs(dt, cold, stores);
        self.age_and_stage(dt);
        self.remove_dead();
        self.pair_and_disperse();
        self.leave_home();
        self.births(dt);
        self.depart_unhappy();
        self.families.retain({
            let live: HashSet<FamilyId> = self.beavers.iter().map(|b| b.family).collect();
            move |f| live.contains(&f.id)
        });
        self.check_collapse();
    }

    fn plan_work(&self, dt: f32, land: &dyn Landscape) -> Vec<Option<Work>> {
        self.beavers.iter().map(|b| {
            let job = self.slot_jobs.get(&b.id)
                .filter(|(_, working)| *working && b.can_work() && !b.resting)?.0;
            // Travel and production belong to physical agents. The colony tracks
            // the resulting workload, skills and needs at each worker's position.
            let eval = SiteEval { land_len: if land.is_water(b.pos.0, b.pos.1) { 0.0 } else { 1.0 },
                water_dist: land.water_distance(b.pos.0, b.pos.1) };
            let units = dt * WORK_FRACTION * b.work_weight()
                * (WORK_REST_BASE + WORK_REST_SCALE * b.needs.rest)
                * (WORK_HEALTH_BASE + WORK_HEALTH_SCALE * b.health)
                * crate::society::work_speed(b.mood);
            Some(Work { job, units, eval })
        }).collect()
    }

    fn do_work(&mut self, plan: &[Option<Work>]) {
        for (b, work) in self.beavers.iter_mut().zip(plan) {
            b.current_job = work.map(|w| w.job);
            if let Some(w) = work {
                let skill = &mut b.skills[w.job.index()];
                *skill = (*skill + w.units * SKILL_GAIN * (1.0 - *skill)).min(1.0);
                self.experience[w.job.index()] += w.units;
            }
        }
    }

    fn teach_young(&mut self, dt: f32) {
        let mut lessons: Vec<(usize, Job, f32)> = Vec::new();
        for (i, b) in self.beavers.iter().enumerate() {
            let (Some(lodge), true) = (b.lodge, b.is_young()) else { continue };
            let mates = || self.beavers.iter().filter(move |m| m.id != b.id && m.lodge == Some(lodge) && !m.is_young());
            let lesson = mates().find_map(|m| m.current_job).map(|job| (job, 1.0));
            if let Some((job, factor)) = lesson {
                lessons.push((i, job, factor));
            }
        }
        for (i, job, factor) in lessons {
            let s = &mut self.beavers[i].skills[job.index()];
            *s = (*s + dt * YOUNG_LEARN_RATE * factor * (1.0 - *s)).min(1.0);
        }
    }

    fn update_work_safety(&mut self, dt: f32, plan: &[Option<Work>]) {
        for i in 0..self.beavers.len() {
            let Some(w) = plan[i] else { continue };
            let exposure = WORK_EXPOSURE_BASE + WORK_EXPOSURE_PER_LAND_CELL * w.eval.land_len
                + WORK_EXPOSURE_PER_WATER_DIST * w.eval.water_dist.min(MAX_WATER_SCAN);
            let b = &mut self.beavers[i];
            let target = (1.0 - SAFETY_EXPOSURE_SCALE * exposure).clamp(SAFETY_MIN, 1.0);
            b.needs.safety += (target - b.needs.safety) * (SAFETY_RATE * dt).min(1.0);
        }
    }

    fn update_needs(&mut self, dt: f32, cold: f32, stores: &mut crate::buildings::Stores) {
        let mut occupancy: HashMap<LodgeId, usize> = HashMap::new();
        for b in &self.beavers {
            if let Some(l) = b.lodge {
                *occupancy.entry(l).or_insert(0) += 1;
            }
        }
        use crate::buildings::{Good, Use};
        let eaten = &mut self.eaten;
        for b in self.beavers.iter_mut() {
            let condition = b.lodge.and_then(|l| self.lodges.iter().find(|x| x.id == l)).map(|l| l.condition);
            let insulation = condition.map_or(OPEN_INSULATION, |c| OPEN_INSULATION + LODGE_INSULATION * c);
            let n = &mut b.needs;

            n.food = (n.food - HUNGER_PER_DAY * dt).max(0.0);
            if n.food < EAT_BELOW {
                let w = b.stage.food_weight();
                let take = ((1.0 - n.food) * w).min(stores.amount(Good::Food));
                stores.withdraw_for(Good::Food, take, Use::eaten_by(b.stage)).expect("limited to stock");
                eaten[b.stage as usize] += take;
                n.food += take / w;
            }
            let worked = if b.current_job.is_some() { WORK_FRACTION } else { IDLE_FRACTION };
            let sleep = insulation.max(OPEN_REST);
            n.rest = (n.rest - TIRE_RATE * worked * dt + SHELTER_REST_RATE * (1.0 - worked) * sleep * dt).clamp(0.0, 1.0);
            if b.resting {
                n.rest = (n.rest + RESTING_RECOVERY * sleep * dt).min(1.0);
                b.resting = n.rest < RESUME_WORK_REST;
            } else if n.rest < EXHAUSTED_REST {
                b.resting = true;
            }
            n.warmth += (1.0 - cold * (1.0 - insulation) - n.warmth) * (WARMTH_RATE * dt).min(1.0);
            if b.current_job.is_none() {
                n.safety += (IDLE_SAFETY.min(1.0) - n.safety) * (SAFETY_RATE * dt).min(1.0);
            }
            let mates = b.lodge.map_or(0, |l| occupancy.get(&l).copied().unwrap_or(1) - 1).min(SOCIAL_MAX_MATES);
            n.social += ((SOCIAL_BASE + SOCIAL_PER_MATE * mates as f32).min(1.0) - n.social) * (SOCIAL_RATE * dt).min(1.0);

            let frail = if b.stage == Stage::Kit { KIT_FRAILTY } else { 1.0 };
            let (mut loss, mut ageing) = (0.0, 0.0);
            for (k, rate, hurting) in [
                (0, STARVE_RATE * frail, n.food < STARVE_BELOW),
                (1, FREEZE_RATE * frail, n.warmth < FREEZE_BELOW),
                (3, EXHAUST_RATE, n.rest < EXHAUST_BELOW),
                (
                    4,
                    OLD_AGE_RATE * (b.age_days / DAYS_PER_YEAR as f32 - OLD_AGE_YEARS),
                    b.age_days > OLD_AGE_YEARS * DAYS_PER_YEAR as f32,
                ),
            ] {
                if hurting {
                    b.harm[k] += rate * dt;
                    loss += rate * dt;
                    if k == 4 {
                        ageing += rate * dt;
                    }
                }
            }
            b.health -= loss;
            if b.health > 0.0 && loss == ageing && n.food > HEAL_ABOVE && n.warmth > HEAL_ABOVE {
                b.health += HEAL_RATE * dt;
            }
            b.health = b.health.min(1.0);

            b.mood += (n.mood() * (MOOD_HEALTH_BASE + MOOD_HEALTH_SCALE * b.health) - b.mood) * (MOOD_RATE * dt).min(1.0);
            b.low_mood_days = if b.mood < LOW_MOOD_BELOW { b.low_mood_days + dt } else { 0.0 };
        }
        let spoiled = (stores.amount(Good::Food) * FOOD_SPOILAGE * dt).min(stores.amount(Good::Food));
        stores.withdraw_for(Good::Food, spoiled, Use::Spoiled).expect("limited to stock");
        self.spoiled += spoiled;
        self.food = stores.amount(Good::Food);
    }

    fn age_and_stage(&mut self, dt: f32) {
        let mut matured = Vec::new();
        for b in self.beavers.iter_mut() {
            let exact = b.age_days as f64 + b.age_remainder + dt as f64;
            b.age_days = exact as f32;
            b.age_remainder = exact - b.age_days as f64;
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
                dead.push((b.id, DeathCause::OldAge, b.pos));
            } else if b.health <= 0.0 {
                let (k, _) = b.harm.iter().enumerate().max_by(|a, c| a.1.total_cmp(c.1)).expect("non-empty");
                dead.push((b.id, HARM_CAUSES[k], b.pos));
            }
        }
        let ids: Vec<BeaverId> = dead.iter().map(|d| d.0).collect();
        self.trace_last_state("DYING", &dead.iter().map(|d| (d.0, format!("{:?}", d.1))).collect::<Vec<_>>());
        self.stats.deaths += dead.len() as u32;
        self.remove_beavers(&ids);
        for (id, cause, at) in dead {
            self.log(Event::Died { id, cause, at });
        }
    }

    /// A beaver leaving the colony at once, as an unhappy one does (for tests).
    #[cfg(test)]
    pub(crate) fn depart(&mut self, id: BeaverId) {
        self.stats.departures += 1;
        self.remove_beavers(&[id]);
        self.log(Event::Departed { id });
    }

    /// A beaver killed outright, whatever its health (a predator's kill).
    pub fn kill(&mut self, id: BeaverId, cause: DeathCause) -> bool {
        let Some(at) = self.beaver(id).map(|b| b.pos) else { return false };
        self.trace_last_state("DYING", &[(id, format!("{cause:?}"))]);
        self.stats.deaths += 1;
        self.remove_beavers(&[id]);
        self.log(Event::Died { id, cause, at });
        true
    }

    fn depart_unhappy(&mut self) {
        let leaving: Vec<BeaverId> = self
            .beavers
            .iter()
            .filter(|b| b.stage != Stage::Kit && b.low_mood_days >= DEPARTURE_BAD_MOOD_DAYS)
            .map(|b| b.id)
            .collect();
        self.trace_last_state("DEPARTING", &leaving.iter().map(|&id| (id, "low mood".to_string())).collect::<Vec<_>>());
        self.stats.departures += leaving.len() as u32;
        self.remove_beavers(&leaving);
        for id in leaving {
            self.log(Event::Departed { id });
        }
    }

    /// For the session journal: a beaver's whole record just before it leaves the colony.
    fn trace_last_state(&self, what: &str, who: &[(BeaverId, String)]) {
        if !crate::trace::enabled() {
            return;
        }
        let temp = temperature(self.time_days);
        for (id, why) in who {
            if let Some(b) = self.beaver(*id) {
                crate::trace::event(format!("{what} day={:.4} why={why} temperature={temp:.2} food_store={:.1} | {}", self.time_days, self.food, crate::journal::describe(b)));
            }
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

    fn adopt_orphans(&mut self) {
        self.house_homeless();
        let live: HashSet<_> = self.beavers.iter().map(|b| b.id).collect();
        let hosts: HashSet<_> = self
            .beavers
            .iter()
            .filter(|b| b.stage == Stage::Adult)
            .filter_map(|b| b.lodge)
            .collect();
        let orphans: Vec<_> = self
            .beavers
            .iter()
            .filter(|b| {
                b.stage != Stage::Adult
                    && (b.mother.is_some() || b.father.is_some())
                    && ![b.mother, b.father].into_iter().flatten().any(|p| live.contains(&p))
            })
            .map(|b| b.id)
            .collect();
        for id in orphans {
            let current = self.beaver(id).and_then(|b| b.lodge);
            if current.is_some_and(|l| hosts.contains(&l)) {
                continue;
            }
            let home = self
                .lodges
                .iter()
                .filter(|l| hosts.contains(&l.id) && self.beavers.iter().filter(|b| b.lodge == Some(l.id)).count() < LODGE_CAPACITY)
                .max_by(|a, b| a.condition.total_cmp(&b.condition))
                .cloned();
            if let Some(home) = home {
                let b = self.beavers.iter_mut().find(|b| b.id == id).unwrap();
                b.family = home.family;
                b.lodge = Some(home.id);
                b.pos = home.pos;
            }
        }
    }

    pub fn close_relatives(&self, a: BeaverId, b: BeaverId) -> bool {
        let ancestors = |who| {
            let mut all = HashSet::from([who]);
            let mut generation = vec![who];
            for _ in 0..2 {
                let next: Vec<_> = generation
                    .iter()
                    .flat_map(|id| self.ancestry.get(id).into_iter().flat_map(|&(f, m)| [f, m].into_iter().flatten()))
                    .collect();
                all.extend(next.iter().copied());
                generation = next;
            }
            all
        };
        !ancestors(a).is_disjoint(&ancestors(b))
    }

    fn births(&mut self, dt: f32) {
        let due: Vec<_> = self
            .beavers
            .iter()
            .filter_map(|m| {
                m.pregnancy
                    .as_ref()
                    .filter(|p| self.time_days >= p.due_day)
                    .map(|p| (m.clone(), p.clone()))
            })
            .collect();
        for (m, p) in due {
            let mother = self.beavers.iter_mut().find(|b| b.id == m.id).unwrap();
            mother.pregnancy = None;
            mother.last_birth_year = (self.time_days / DAYS_PER_YEAR).floor() as i64;
            for _ in 0..p.litter {
                let sex = if self.chance(MALE_SHARE) { Sex::Male } else { Sex::Female };
                let id = self.spawn(sex, 0.0, m.family, m.lodge, m.pos, Some(p.father), Some(m.id));
                self.stats.births += 1;
                self.log(Event::Born { id, family: m.family });
            }
        }
        self.stats.peak_population = self.stats.peak_population.max(self.beavers.len());
        let doy = day_of_year(self.time_days);
        if !(MATING_FROM_DAY..MATING_TO_DAY).contains(&doy) {
            return;
        }
        let birth_year = ((self.time_days + GESTATION_DAYS) / DAYS_PER_YEAR).floor() as i64;
        let candidates: Vec<_> = self
            .beavers
            .iter()
            .filter(|m| {
                m.sex == Sex::Female
                    && m.stage == Stage::Adult
                    && m.age_years() < MAX_BIRTH_AGE_YEARS
                    && m.pregnancy.is_none()
                    && m.last_birth_year != birth_year
                    && m.health >= BREED_MIN_HEALTH
                    && m.needs.food >= BREED_MIN_FOOD
                    && self
                        .lodges
                        .iter()
                        .any(|l| Some(l.id) == m.lodge && l.family == m.family && l.condition >= BREED_MIN_HOME_CONDITION)
                    && m.mate.is_some_and(|id| {
                        self.beaver(id).is_some_and(|f| {
                            f.family == m.family && f.lodge == m.lodge && f.stage == Stage::Adult && !self.close_relatives(m.id, f.id)
                        })
                    })
            })
            .map(|m| (m.id, m.mate.unwrap(), m.lodge))
            .collect();
        for (id, father, home) in candidates {
            let room = LODGE_CAPACITY.saturating_sub(self.beavers.iter().filter(|b| b.lodge == home).count());
            let daily_food=HUNGER_PER_DAY*(self.beavers.iter().map(|b|b.stage.food_weight()).sum::<f32>()+MIN_LITTER as f32*KIT_FOOD_WEIGHT);
            if room < MIN_LITTER as usize
                || self.food < daily_food * BIRTH_MIN_FOOD_DAYS
                || !self.chance(CONCEPTION_CHANCE_PER_DAY * dt)
            {
                continue;
            }
            let litter = (MIN_LITTER + self.range(LITTER_SPREAD)).min(room as u32);
            crate::trace::event(format!("CONCEIVED day={:.4} mother={} father={father:?} litter={litter} due_day={:.2} lodge_room={room} food={:.1}", self.time_days, id.0, self.time_days + GESTATION_DAYS, self.food));
            self.beavers.iter_mut().find(|b| b.id == id).unwrap().pregnancy = Some(Pregnancy {
                father,
                conceived_day: self.time_days,
                due_day: self.time_days + GESTATION_DAYS,
                litter,
            });
        }
    }
    /// At `LEAVE_HOME_AGE_YEARS` an unpaired beaver still living with a parent moves into an
    /// empty lodge as a family of its own, oldest first, so grown young don't fill their
    /// parents' lodge and keep it from breeding. With no empty lodge it stays until one is free.
    fn leave_home(&mut self) {
        let mut empty: Vec<LodgeId> = self.lodges.iter().filter(|l| !self.beavers.iter().any(|b| b.lodge == Some(l.id))).map(|l| l.id).collect();
        if empty.is_empty() {
            return;
        }
        let with_parent = |b: &Beaver| {
            b.lodge.is_some() && [b.mother, b.father].into_iter().flatten().any(|p| self.beaver(p).is_some_and(|p| p.lodge == b.lodge))
        };
        let mut leaving: Vec<(f32, BeaverId)> = self
            .beavers
            .iter()
            .filter(|b| b.stage == Stage::Adult && b.mate.is_none() && b.age_years() >= LEAVE_HOME_AGE_YEARS && with_parent(b))
            .map(|b| (b.age_days, b.id))
            .collect();
        leaving.sort_by(|a, b| b.0.total_cmp(&a.0).then(a.1.cmp(&b.1)));
        empty.reverse();
        for (_, id) in leaving {
            let Some(home) = empty.pop() else { break };
            let fam = self.new_family();
            let l = self.lodges.iter_mut().find(|l| l.id == home).unwrap();
            l.family = fam;
            let site = l.pos;
            let b = self.beavers.iter_mut().find(|b| b.id == id).unwrap();
            b.family = fam;
            b.lodge = Some(home);
            b.pos = site;
            self.log(Event::LeftHome { id, lodge: home });
        }
    }

    fn pair_and_disperse(&mut self) {
        let doy = day_of_year(self.time_days);
        if !(SPRING_START..SPRING_END).contains(&doy) && !(MATING_FROM_DAY..MATING_TO_DAY).contains(&doy) {
            return;
        }
        let mut eligible: Vec<_> = self
            .beavers
            .iter()
            .filter(|b| b.stage == Stage::Adult && b.mate.is_none() && b.health >= PAIR_MIN_HEALTH)
            .map(|b| (b.id, b.sex))
            .collect();
        // Who looks for a mate first, and whom they find, is left to chance: any unpaired adult
        // of the other sex in the colony who is not a close relative, newcomer or not.
        for i in (1..eligible.len()).rev() {
            let j = self.range(i as u32 + 1) as usize;
            eligible.swap(i, j);
        }
        let mut used = HashSet::new();
        for &(a, sex) in &eligible {
            if used.contains(&a) {
                continue;
            }
            let options: Vec<BeaverId> = eligible
                .iter()
                .filter(|&&(b, s)| b != a && s != sex && !used.contains(&b) && !self.close_relatives(a, b))
                .map(|p| p.0)
                .collect();
            if options.is_empty() {
                continue;
            }
            let b = options[self.range(options.len() as u32) as usize];
            // A widowed parent keeps its family home and dependent young. An
            // adult offspring leaves its parents only when a new home is ready.
            let reuse = [a, b].into_iter().find_map(|id| {
                let own = self.beaver(id)?;
                let home = own.lodge?;
                self.lodges
                    .iter()
                    .find(|l| {
                        l.id == home
                            && l.family == own.family
                            && self
                                .beavers
                                .iter()
                                .filter(|b| b.lodge == Some(home) && b.stage == Stage::Adult)
                                .all(|x| x.id == a || x.id == b)
                    })
                    .map(|l| l.id)
            });
            let home = reuse.or_else(|| {
                self.lodges
                    .iter()
                    .find(|l| !self.beavers.iter().any(|b| b.lodge == Some(l.id)))
                    .map(|l| l.id)
            });
            let Some(home) = home else { continue };
            let fam = if let Some(id) = reuse {
                self.lodges.iter().find(|l| l.id == id).unwrap().family
            } else {
                self.new_family()
            };
            let l = self.lodges.iter_mut().find(|l| l.id == home).unwrap();
            l.family = fam;
            let site = l.pos;
            for x in self.beavers.iter_mut().filter(|x| {
                x.id == a
                    || x.id == b
                    || (x.stage != Stage::Adult && [x.mother, x.father].into_iter().flatten().any(|id| id == a || id == b))
            }) {
                x.family = fam;
                x.lodge = Some(home);
                x.pos = site;
            }
            used.insert(a);
            used.insert(b);
            self.set_mates(a, b);
            self.log(Event::NewFamily {
                family: fam,
                lodge: home,
                a,
                b,
            });
        }
    }

    pub fn migration_remaining(&self) -> u32 {
        if self.is_collapsed() {return 0;}
        let year = (self.time_days / DAYS_PER_YEAR).floor() as i64;
        MAX_IMMIGRANTS_PER_YEAR.saturating_sub(self.immigration_by_year.get(&year).copied().unwrap_or(0))
    }

    fn record_immigrant(&mut self, id: BeaverId) {
        let year = (self.time_days / DAYS_PER_YEAR).floor() as i64;
        *self.immigration_by_year.entry(year).or_default() += 1;
        self.stats.immigrants += 1;
        self.log(Event::Immigrated { id });
    }

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

    fn new_family(&mut self) -> FamilyId {
        let id = FamilyId(self.next_id);
        self.next_id += 1;
        let name = FAMILY_NAMES[self.range(FAMILY_NAMES.len() as u32) as usize].to_string();
        self.families.push(Family { id, name });
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
        let lifespan_days = (LIFESPAN_MIN_YEARS + LIFESPAN_SPREAD_YEARS * self.unit()) * DAYS_PER_YEAR as f32;
        let mut skills = [0.0; Job::ALL.len()];
        if age_days >= ADULT_DAYS {
            for s in skills.iter_mut() {
                *s = START_SKILL_MAX * self.unit();
            }
        }
        self.beavers.push(Beaver {
            id,
            name,
            age_days,
            age_remainder: 0.0,
            sex,
            family,
            mother,
            father,
            mate: None,
            skills,
            needs: Needs::satisfied(),
            mood: START_MOOD,
            health: 1.0,
            job: None,
            current_job: None,
            lodge,
            pos,
            lifespan_days,
            stage: Stage::of_age_days(age_days),
            resting: false,
            last_birth_year: -1,
            pregnancy: None,
            low_mood_days: 0.0,
            harm: [0.0; 5],
        });
        self.ancestry.insert(id, (father, mother));
        id
    }

    fn set_mates(&mut self, a: BeaverId, b: BeaverId) {
        crate::trace::event(format!("PAIRED day={:.4} a={} b={}", self.time_days, a.0, b.0));
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

impl Colony {
    pub fn admit_wanderer(&mut self, adult: bool) -> Result<BeaverId, ColonyError> {
        if !self.founded {return Err(ColonyError::Invalid("colony is not founded".into()));}
        if self.migration_remaining() == 0 {return Err(ColonyError::Invalid(format!("annual migration limit reached ({MAX_IMMIGRANTS_PER_YEAR} a year)")));}
        let waiting=adult.then(||self.beavers.iter().find(|b|b.stage==Stage::Adult && b.mate.is_none()
            && b.lodge.is_some() && self.beavers.iter().filter(|x|x.lodge==b.lodge && x.stage==Stage::Adult).count()==1).cloned()).flatten();
        let empty=self.lodges.iter().find(|l|!self.beavers.iter().any(|b|b.lodge==Some(l.id))).cloned();
        // A newcomer only finds a roof here. It pairs later, by chance, like any unpaired adult.
        let sex=if self.chance(MALE_SHARE){Sex::Male}else{Sex::Female};
        let (home,family,pos)=if let Some(b)=waiting.as_ref() {
            (b.lodge.unwrap(),b.family,b.pos)
        }else if let Some(l)=empty {
            let fam=self.new_family();self.lodges.iter_mut().find(|h|h.id==l.id).unwrap().family=fam;
            (l.id,fam,l.pos)
        }else{return Err(ColonyError::Invalid("a newcomer needs an empty family lodge or an unpaired adult's home".into()));};
        let years=if adult{IMMIGRANT_MIN_AGE_YEARS+IMMIGRANT_AGE_SPREAD_YEARS*self.unit()}else{1.0+self.unit()};
        let id=self.spawn(sex,years*DAYS_PER_YEAR as f32,family,Some(home),pos,None,None);
        self.stats.peak_population=self.stats.peak_population.max(self.beavers.len());
        self.record_immigrant(id);Ok(id)
    }
    pub fn add_immigrants(&mut self, n: u32) -> u32 {
        let mut admitted = 0;
        for _ in 0..n {
            if self.admit_wanderer(true).is_err() { break; }
            admitted += 1;
        }
        admitted
    }

}
