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
        c.admit_wanderer(true).unwrap();c.admit_wanderer(true).unwrap();
        assert_eq!(c.migration_remaining(),0);assert!(c.admit_wanderer(true).is_err());assert_eq!(c.add_immigrants(10),0);
        let before=c.population();let h=Household{pair_years:3.0,pair_joined:true,born_days:Vec::new(),joined:0};
        assert!(c.seat_household((45.0,0.0),&RiverLand::default(),&h).is_err());assert_eq!(c.population(),before);
        c.time_days=360.0;assert_eq!(c.migration_remaining(),2);assert_eq!(c.immigration_by_year.get(&0),Some(&2));
        c.beavers.clear();c.check_collapse();assert!(c.is_collapsed());
        c.time_days=720.0;assert_eq!(c.migration_remaining(),0);assert_eq!(c.add_immigrants(2),0);
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
        c.do_work(0.5, &plan);
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
        c.update_needs(0.5, 0.0);
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

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct BeaverId(pub u64);
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct FamilyId(pub u64);
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct LodgeId(pub u64);

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

const HARM_CAUSES: [DeathCause; 5] =
    [DeathCause::Starvation, DeathCause::Cold, DeathCause::Injury, DeathCause::Exhaustion, DeathCause::OldAge];

#[derive(Clone, Debug, PartialEq)]
pub enum ColonyError {
    InvalidDt(f32),
    UnknownBeaver(BeaverId),
    UnknownFamily(FamilyId),
    Invalid(String),
    AlreadyFounded,
}

impl fmt::Display for ColonyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ColonyError::InvalidDt(d) => write!(f, "invalid dt_days: {d} (must be finite and >= 0)"),
            ColonyError::UnknownBeaver(id) => write!(f, "unknown beaver {}", id.0),
            ColonyError::UnknownFamily(id) => write!(f, "unknown family {}", id.0),
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

#[derive(Clone, Copy, Debug)]
pub struct RiverLand {
    pub y: f32,
    pub half_width: f32,
}
impl Default for RiverLand {
    fn default() -> Self {
        Self { y: 0.0, half_width: DEFAULT_RIVER_HALF_WIDTH }
    }
}
impl Landscape for RiverLand {
    fn is_water(&self, _x: f32, y: f32) -> bool {
        (y - self.y).abs() <= self.half_width
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
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

#[derive(Clone, Debug)]
pub struct Pregnancy {
    pub father: BeaverId,
    pub conceived_day: f64,
    pub due_day: f64,
    pub litter: u32,
}

#[derive(Clone, Debug)]
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
    pub skills: [f32; 8],
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
    pub fn age_years(&self) -> f32 {
        self.age_days / DAYS_PER_YEAR as f32
    }
    pub fn skill(&self, job: Job) -> f32 {
        self.skills[job.index()]
    }
    pub fn specialty(&self) -> Option<Job> {
        Job::ALL
            .iter()
            .copied()
            .filter(|&j| self.skill(j) >= SPECIALIST_SKILL)
            .max_by(|&a, &b| self.skill(a).total_cmp(&self.skill(b)))
    }
    pub fn title(&self) -> Option<String> {
        self.specialty().map(|j| format!("{j:?} specialist"))
    }
    pub fn is_young(&self) -> bool {
        matches!(self.stage, Stage::Kit | Stage::Yearling)
    }
    pub fn harm(&self) -> [(DeathCause, f32); 5] {
        std::array::from_fn(|i| (HARM_CAUSES[i], self.harm[i]))
    }
    pub fn output_factor(&self, job: Job) -> f32 {
        let s = self.skill(job);
        (SKILL_BASE + s) * if s >= SPECIALIST_SKILL { SPECIALIST_BONUS } else { 1.0 }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Household {
    pub pair_years: f32,
    pub pair_joined: bool,
    pub born_days: Vec<f32>,
    pub joined: u32,
}

#[derive(Clone, Debug)]
pub struct Family {
    pub id: FamilyId,
    pub name: String,
}

#[derive(Clone, Debug)]
pub struct Lodge {
    pub id: LodgeId,
    pub family: FamilyId,
    pub pos: (f32, f32),
    pub condition: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Event {
    Born { id: BeaverId, family: FamilyId },
    Matured { id: BeaverId, stage: Stage },
    Died { id: BeaverId, cause: DeathCause, at: (f32, f32) },
    Departed { id: BeaverId },
    Immigrated { id: BeaverId },
    NewFamily { family: FamilyId, lodge: LodgeId, a: BeaverId, b: BeaverId },
    Collapsed(CollapseReport),
}

#[derive(Clone, Debug, PartialEq)]
pub struct LoggedEvent {
    pub day: f64,
    pub event: Event,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CollapseCause {
    Extinct,
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
    land_len: f32,
    water_dist: f32,
}

#[derive(Clone, Copy, Debug)]
struct Work {
    job: Job,
    units: f32,
    eval: SiteEval,
}

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
    pub experience: [f32; 8],
    pub stats: Stats,
    pub immigration_by_year: std::collections::BTreeMap<i64, u32>,
    /// Parent records survive deaths and dispersal, so family changes cannot
    /// make close relatives eligible to mate.
    pub ancestry: HashMap<BeaverId, (Option<BeaverId>, Option<BeaverId>)>,
    pub events: VecDeque<LoggedEvent>,
    pub events_dropped: u64,
    collapse: Option<CollapseReport>,
    founded: bool,
    guard_strength: f32,
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

const FIRST_NAMES: [&str; 24] = [
    "Alder", "Birch", "Clover", "Dusk", "Elm", "Fern", "Gorse", "Hazel", "Iris", "Juniper", "Kelp", "Linden",
    "Moss", "Nettle", "Oak", "Pebble", "Quill", "Reed", "Sedge", "Thistle", "Umber", "Willow", "Yarrow", "Zinnia",
];
const FAMILY_NAMES: [&str; 12] = [
    "Stillwater", "Mudbank", "Fallowdam", "Rushmere", "Oakhollow", "Brookbend", "Sedgefield", "Ashpool",
    "Cattail", "Lowfen", "Marshgate", "Riverbrook",
];

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
            experience: [0.0; 8],
            stats: Stats::default(),
            immigration_by_year: Default::default(),
            ancestry: HashMap::new(),
            events: VecDeque::new(),
            events_dropped: 0,
            collapse: None,
            founded: false,
            guard_strength: 0.0,
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
    pub fn collapse(&self) -> Option<&CollapseReport> {
        self.collapse.as_ref()
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
            m.pregnancy = Some(Pregnancy { father, conceived_day: -60.0, due_day: GESTATION_DAYS - 60.0, litter });
        }
        self.spawn(Sex::Male, FOUNDER_YEARLING_AGE_YEARS * year, fam, Some(lodge), pos, Some(father), Some(mother));
        self.spawn(Sex::Female, FOUNDER_YEARLING_AGE_YEARS * year, fam, Some(lodge), pos, Some(father), Some(mother));
        self.food = START_FOOD;
        self.wood = START_WOOD;
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

    pub fn seat_household(&mut self, pos: (f32, f32), land: &dyn Landscape, h: &Household) -> Result<FamilyId, ColonyError> {
        check_point("household", pos)?;
        let arriving = h.joined + if h.pair_joined { 2 } else { 0 };
        if arriving > self.migration_remaining() {
            return Err(ColonyError::Invalid("annual migration limit reached (2 per year)".into()));
        }
        if land.is_water(pos.0, pos.1) {
            return Err(ColonyError::Invalid("household position is in water".into()));
        }
        if h.pair_years < ADULT_DAYS / DAYS_PER_YEAR as f32 {
            return Err(ColonyError::Invalid(format!("pair must be adults, got {} years", h.pair_years)));
        }
        let members = 2 + h.born_days.len() + h.joined as usize;
        if members > LODGE_CAPACITY {
            return Err(ColonyError::Invalid(format!("{members} beavers do not fit in one lodge (room for {LODGE_CAPACITY})")));
        }
        if let Some(a) = h.born_days.iter().find(|a| !a.is_finite() || **a < 0.0) {
            return Err(ColonyError::Invalid(format!("bad kit age {a}")));
        }
        let year = DAYS_PER_YEAR as f32;
        let fam = self.new_family();
        let lodge = self.new_lodge(fam, pos, FOUNDING_LODGE_CONDITION);
        let father = self.spawn(Sex::Male, h.pair_years * year, fam, Some(lodge), pos, None, None);
        let mother = self.spawn(Sex::Female, h.pair_years * year, fam, Some(lodge), pos, None, None);
        self.set_mates(father, mother);
        let mut arrived = Vec::new();
        if h.pair_joined {
            arrived.extend([father, mother]);
        }
        for (i, &age) in h.born_days.iter().enumerate() {
            let sex = if i % 2 == 0 { Sex::Male } else { Sex::Female };
            let kit = self.spawn(sex, age, fam, Some(lodge), pos, Some(father), Some(mother));
            self.stats.births += 1;
            self.log(Event::Born { id: kit, family: fam });
        }
        for i in 0..h.joined {
            let sex = if i % 2 == 0 { Sex::Female } else { Sex::Male };
            let age = (IMMIGRANT_MIN_AGE_YEARS + IMMIGRANT_AGE_SPREAD_YEARS * self.unit()) * year;
            arrived.push(self.spawn(sex, age, fam, Some(lodge), pos, None, None));
        }
        for id in arrived {
            self.record_immigrant(id);
        }
        if !self.founded {
            self.food = START_FOOD;
            self.wood = START_WOOD;
            self.founded = true;
        }
        self.stats.peak_population = self.stats.peak_population.max(self.beavers.len());
        Ok(fam)
    }

    pub fn step(&mut self, dt_days: f32) {
        if let Err(e) = self.step_on(dt_days, &RiverLand::default()) {
            panic!("Colony::step called with unvalidated dt: {e}");
        }
    }

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
        let temp = temperature(self.time_days);
        let cold = ((COLD_BELOW_TEMP - temp) / COLD_RANGE).clamp(0.0, 1.0);

        self.adopt_orphans();
        let plan = self.plan_work(dt, land);
        self.do_work(dt, &plan);
        self.teach_young(dt);
        self.update_work_safety(dt, &plan);
        self.update_needs(dt, cold);
        self.age_and_stage(dt);
        self.remove_dead();
        self.pair_and_disperse(dt);
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
                .filter(|(_, working)| *working && b.stage != Stage::Kit && !b.resting)?.0;
            // Travel and production belong to physical agents. The colony tracks
            // the resulting workload, skills and needs at each worker's position.
            let eval = SiteEval { land_len: if land.is_water(b.pos.0, b.pos.1) { 0.0 } else { 1.0 },
                water_dist: land.water_distance(b.pos.0, b.pos.1) };
            let units = dt * WORK_FRACTION * b.stage.work_weight()
                * (WORK_REST_BASE + WORK_REST_SCALE * b.needs.rest)
                * (WORK_HEALTH_BASE + WORK_HEALTH_SCALE * b.health)
                * crate::society::work_speed(b.mood);
            Some(Work { job, units, eval })
        }).collect()
    }

    fn do_work(&mut self, dt: f32, plan: &[Option<Work>]) {
        self.guard_strength = self.beavers.iter().zip(plan).filter_map(|(b,w)| {
            w.filter(|w| w.job == Job::Guard).map(|w| w.units / dt * b.output_factor(w.job))
        }).sum();
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
        let mitigation = 1.0 + GUARD_MITIGATION * self.guard_strength;
        for i in 0..self.beavers.len() {
            let Some(w) = plan[i] else { continue };
            let exposure = WORK_EXPOSURE_BASE + WORK_EXPOSURE_PER_LAND_CELL * w.eval.land_len
                + WORK_EXPOSURE_PER_WATER_DIST * w.eval.water_dist.min(MAX_WATER_SCAN);
            let b = &mut self.beavers[i];
            let target = (1.0 - SAFETY_EXPOSURE_SCALE * exposure / mitigation).clamp(SAFETY_MIN, 1.0);
            b.needs.safety += (target - b.needs.safety) * (SAFETY_RATE * dt).min(1.0);
        }
    }

    fn update_needs(&mut self, dt: f32, cold: f32) {
        let mut occupancy: HashMap<LodgeId, usize> = HashMap::new();
        for b in &self.beavers {
            if let Some(l) = b.lodge {
                *occupancy.entry(l).or_insert(0) += 1;
            }
        }
        let guarded = (self.guard_strength * GUARD_SAFETY_PER_STRENGTH).min(GUARD_SAFETY_MAX);
        let mut food = self.food;
        let eaten = &mut self.eaten;
        for b in self.beavers.iter_mut() {
            let condition = b.lodge.and_then(|l| self.lodges.iter().find(|x| x.id == l)).map(|l| l.condition);
            let insulation = condition.map_or(OPEN_INSULATION, |c| OPEN_INSULATION + LODGE_INSULATION * c);
            let n = &mut b.needs;

            n.food = (n.food - HUNGER_PER_DAY * dt).max(0.0);
            if n.food < EAT_BELOW {
                let w = b.stage.food_weight();
                let take = ((1.0 - n.food) * w).min(food);
                food -= take;
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
                n.safety += ((IDLE_SAFETY + guarded).min(1.0) - n.safety) * (SAFETY_RATE * dt).min(1.0);
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
        self.spoiled += food * FOOD_SPOILAGE * dt;
        self.food = food * (1.0 - FOOD_SPOILAGE * dt);
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
        self.stats.deaths += dead.len() as u32;
        self.remove_beavers(&ids);
        for (id, cause, at) in dead {
            self.log(Event::Died { id, cause, at });
        }
    }

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
                let sex = if self.chance(0.5) { Sex::Male } else { Sex::Female };
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
                        .any(|l| Some(l.id) == m.lodge && l.family == m.family && l.condition >= 0.5)
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
                || !self.chance(0.2 * dt)
            {
                continue;
            }
            let litter = (MIN_LITTER + self.range(LITTER_SPREAD)).min(room as u32);
            self.beavers.iter_mut().find(|b| b.id == id).unwrap().pregnancy = Some(Pregnancy {
                father,
                conceived_day: self.time_days,
                due_day: self.time_days + GESTATION_DAYS,
                litter,
            });
        }
    }
    fn pair_and_disperse(&mut self, dt: f32) {
        let doy = day_of_year(self.time_days);
        if !(SPRING_START..SPRING_END).contains(&doy) && !(MATING_FROM_DAY..MATING_TO_DAY).contains(&doy) {
            return;
        }
        let eligible: Vec<_> = self
            .beavers
            .iter()
            .filter(|b| b.stage == Stage::Adult && b.mate.is_none() && b.health >= PAIR_MIN_HEALTH)
            .map(|b| (b.id, b.sex))
            .collect();
        let mut used = HashSet::new();
        for &(a, sex) in &eligible {
            if used.contains(&a) {
                continue;
            }
            let partner = eligible
                .iter()
                .find(|&&(b, s)| b != a && s != sex && !used.contains(&b) && !self.close_relatives(a, b))
                .map(|p| p.0);
            // A widowed parent keeps its family home and dependent young. An
            // adult offspring leaves its parents only when a new home is ready.
            let reuse = [Some(a), partner].into_iter().flatten().find_map(|id| {
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
                                .all(|b| b.id == a || Some(b.id) == partner)
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
            if partner.is_none() && (self.food_per_capita() < HUNGRY_FOOD_PER_CAPITA || !self.chance(WANDERER_CHANCE_PER_DAY * dt)) {
                continue;
            }
            let b = match partner {
                Some(id) => id,
                None => { let Some(id) = self.immigrant(sex.opposite()) else { continue }; id }
            };
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

    fn immigrant(&mut self, sex: Sex) -> Option<BeaverId> {
        if self.migration_remaining() == 0 { return None; }
        let age = (IMMIGRANT_MIN_AGE_YEARS + IMMIGRANT_AGE_SPREAD_YEARS * self.unit()) * DAYS_PER_YEAR as f32;
        let fam = self.new_family();
        let id = self.spawn(sex, age, fam, None, (0.0, 0.0), None, None);
        self.stats.peak_population = self.stats.peak_population.max(self.beavers.len());
        self.record_immigrant(id);
        Some(id)
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
        let mut skills = [0.0; 8];
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
        if self.migration_remaining() == 0 {return Err(ColonyError::Invalid("annual migration limit reached (2 per year)".into()));}
        let waiting=adult.then(||self.beavers.iter().find(|b|b.stage==Stage::Adult && b.mate.is_none()
            && b.lodge.is_some() && self.beavers.iter().filter(|x|x.lodge==b.lodge && x.stage==Stage::Adult).count()==1).cloned()).flatten();
        let empty=self.lodges.iter().find(|l|!self.beavers.iter().any(|b|b.lodge==Some(l.id))).cloned();
        let (home,family,pos,sex)=if let Some(b)=waiting.as_ref() {
            (b.lodge.unwrap(),b.family,b.pos,b.sex.opposite())
        }else if let Some(l)=empty {
            let fam=self.new_family();self.lodges.iter_mut().find(|h|h.id==l.id).unwrap().family=fam;
            (l.id,fam,l.pos,if self.chance(0.5){Sex::Male}else{Sex::Female})
        }else{return Err(ColonyError::Invalid("a newcomer needs an empty family lodge or an unpaired adult's home".into()));};
        let years=if adult{IMMIGRANT_MIN_AGE_YEARS+IMMIGRANT_AGE_SPREAD_YEARS*self.unit()}else{1.0+self.unit()};
        let id=self.spawn(sex,years*DAYS_PER_YEAR as f32,family,Some(home),pos,None,None);
        if let Some(partner)=waiting {self.set_mates(partner.id,id);self.log(Event::NewFamily{family,lodge:home,a:partner.id,b:id});}
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
