use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::fmt;

use rand_core::{Rng, SeedableRng};
use rand_pcg::Pcg32;
pub use crate::content::{Duty, Job};
pub use crate::tuning::colony::*;

pub const DAYS_PER_YEAR: f64 = crate::calendar::YEAR_DAYS;
const SUBSTEP_DAYS: f32 = 0.5;
const EVENT_LOG_CAP: usize = 2000;
const HARM_CAUSES: [DeathCause; 5] = [DeathCause::Starvation, DeathCause::Cold, DeathCause::Injury, DeathCause::Exhaustion, DeathCause::OldAge];

fn old_age_hazard(years: f32) -> f32 {
    if years < OLD_AGE_YEARS { 0.0 } else { OLD_AGE_DEATH_RATE * (OLD_AGE_DEATH_GROWTH * (years - OLD_AGE_YEARS)).exp() }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
pub struct BeaverId(pub u64);
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
pub struct FamilyId(pub u64);
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
pub struct LodgeId(pub u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum Sex { Male, Female }

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
pub enum Stage { Kit, Yearling, Adult }

impl Stage {
    pub fn of_age_days(age: f32) -> Stage { if age < KIT_DAYS { Self::Kit } else if age < ADULT_DAYS { Self::Yearling } else { Self::Adult } }
    pub(crate) fn work_weight(self) -> f32 { match self { Self::Kit => KIT_WORK_WEIGHT, Self::Yearling => YEARLING_WORK_WEIGHT, Self::Adult => 1.0 } }
    pub fn food_weight(self) -> f32 { match self { Self::Kit => KIT_FOOD_WEIGHT, Self::Yearling => YEARLING_FOOD_WEIGHT, Self::Adult => 1.0 } }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum DeathCause { Starvation, Cold, Injury, Exhaustion, OldAge, Predator }

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum ColonyError { InvalidDt(f32), Invalid(String), AlreadyFounded }

impl fmt::Display for ColonyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidDt(d) => write!(f, "invalid dt_days: {d} (must be finite and >= 0)"),
            Self::Invalid(m) => write!(f, "invalid argument: {m}"),
            Self::AlreadyFounded => write!(f, "colony already founded"),
        }
    }
}
impl std::error::Error for ColonyError {}

pub trait Landscape {
    fn is_water(&self, x: f32, y: f32) -> bool;
    fn water_distance(&self, x: f32, y: f32) -> f32 {
        if self.is_water(x, y) { return 0.0; }
        let mut r = 1.0;
        while r <= MAX_WATER_SCAN {
            for k in 0..16 {
                let a = k as f32 * std::f32::consts::TAU / 16.0;
                if self.is_water(x + r * a.cos(), y + r * a.sin()) { return r; }
            }
            r += 1.0;
        }
        MAX_WATER_SCAN
    }
}

impl Landscape for crate::grid::Grid<f32> {
    fn is_water(&self, x: f32, y: f32) -> bool {
        x >= 0.0 && y >= 0.0 && self.get(x as usize, y as usize).is_some_and(|d| *d > WATER_DEPTH)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Needs { pub food: f32, pub rest: f32, pub warmth: f32, pub safety: f32, pub social: f32 }
impl Needs {
    fn satisfied() -> Self { Self { food: 1.0, rest: 1.0, warmth: 1.0, safety: 1.0, social: START_SOCIAL } }
    fn mood(&self) -> f32 {
        MOOD_FOOD_WEIGHT * self.food + MOOD_WARMTH_WEIGHT * self.warmth + MOOD_REST_WEIGHT * self.rest + MOOD_SAFETY_WEIGHT * self.safety + MOOD_SOCIAL_WEIGHT * self.social
    }
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Pregnancy { pub father: BeaverId, pub conceived_day: f64, pub due_day: f64, pub litter: u32 }

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Beaver {
    pub id: BeaverId, pub name: String, pub age_days: f32, age_remainder: f64,
    pub sex: Sex, pub family: FamilyId, pub mother: Option<BeaverId>, pub father: Option<BeaverId>, pub mate: Option<BeaverId>,
    pub skills: crate::buildings::PerId<Job, f32>, pub experience: f32, pub needs: Needs, pub mood: f32, pub health: f32,
    pub job: Option<Job>, pub current_job: Option<Job>, pub lodge: Option<LodgeId>, pub pos: (f32, f32),
    pub stage: Stage, pub resting: bool, #[serde(skip)] pub in_lodge: bool,
    pub last_birth_year: i64, pub pregnancy: Option<Pregnancy>, low_mood_days: f32, harm: [f32; 5],
}

impl Beaver {
    pub fn harm(&self) -> [f32; 5] { self.harm }
    pub fn low_mood_days(&self) -> f32 { self.low_mood_days }
    pub fn age_years(&self) -> f32 { self.age_days / DAYS_PER_YEAR as f32 }
    pub fn can_work(&self) -> bool { self.age_days >= WORK_AGE_DAYS }
    pub fn work_weight(&self) -> f32 { if self.can_work() { self.stage.work_weight() } else { 0.0 } }
    pub fn is_young(&self) -> bool { matches!(self.stage, Stage::Kit | Stage::Yearling) }
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Family { pub id: FamilyId, pub name: String }
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Lodge { pub id: LodgeId, pub family: FamilyId, pub pos: (f32, f32), pub condition: f32 }

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum Event {
    Born { id: BeaverId, family: FamilyId },
    Matured { id: BeaverId, stage: Stage },
    Died { id: BeaverId, cause: DeathCause, at: (f32, f32) },
    Departed { id: BeaverId },
    Immigrated { id: BeaverId },
    NewFamily { family: FamilyId, lodge: LodgeId, a: BeaverId, b: BeaverId },
    Collapsed(CollapseReport),
    LeftHome { id: BeaverId, lodge: LodgeId },
    Epithet { id: BeaverId },
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct LoggedEvent { pub day: f64, pub event: Event }
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum CollapseCause { Extinct, NoAdults }
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CollapseReport { pub day: f64, pub cause: CollapseCause, pub peak_population: usize, pub deaths: u32, pub departures: u32 }
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Stats { pub births: u32, pub deaths: u32, pub departures: u32, pub immigrants: u32, pub peak_population: usize }
#[derive(Clone, Copy, Debug)]
struct Work { job: Job, units: f32, land_len: f32, water_dist: f32 }

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rates {
    pub hunger: f32, pub eat_below: f32, pub rest: f32, pub sleep: f32, pub resting: bool,
    pub warmth_to: f32, pub cold: f32, pub insulation: f32, pub safety_to: f32, pub exposure: Option<(f32, f32)>,
    pub social_to: f32, pub mates: usize, pub mood_to: f32, pub mood_health: f32,
    pub harm: [f32; 5], pub heal: f32, pub old_age_death: f32, pub leaves_in: Option<f32>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Colony {
    rng: Pcg32,
    pub time_days: f64,
    next_id: u64,
    pub beavers: Vec<Beaver>,
    pub families: Vec<Family>,
    pub lodges: Vec<Lodge>,
    pub food: f32,
    pub stats: Stats,
    pub immigration_by_year: BTreeMap<i64, u32>,
    pub ancestry: HashMap<BeaverId, (Option<BeaverId>, Option<BeaverId>)>,
    pub events: VecDeque<LoggedEvent>,
    pub events_dropped: u64,
    collapse: Option<CollapseReport>,
    founded: bool,
    pub slot_jobs: HashMap<BeaverId, (Job, bool)>,
    pub eaten: [f32; 4],
    pub spoiled: f32,
    pub looks: BTreeMap<BeaverId, crate::looks::Looks>,
    #[serde(skip)] pub sky: crate::weather::Sky,
}

impl Default for Colony { fn default() -> Self { Self::new(0) } }

fn lodge_insulation(lodges: &[Lodge], b: &Beaver) -> f32 {
    b.lodge.and_then(|l| lodges.iter().find(|x| x.id == l)).map_or(OPEN_INSULATION, |l| OPEN_INSULATION + LODGE_INSULATION * l.condition)
}
fn insulation(lodges: &[Lodge], b: &Beaver) -> f32 { if b.in_lodge { lodge_insulation(lodges, b) } else { OPEN_INSULATION } }
fn rest_per_day(b: &Beaver, worked: f32, sleep: f32) -> f32 { -TIRE_RATE * crate::growth::tire_x(b) * worked + LODGE_REST_RATE * (1.0 - worked) * sleep }
fn warmth_target(cold: f32, ins: f32) -> f32 { 1.0 - cold * (1.0 - ins) }
fn social_target(mates: usize) -> f32 { (SOCIAL_BASE + SOCIAL_PER_MATE * mates as f32).min(1.0) }
fn work_safety(land_len: f32, water_dist: f32) -> f32 {
    (1.0 - SAFETY_EXPOSURE_SCALE * (WORK_EXPOSURE_BASE + WORK_EXPOSURE_PER_LAND_CELL * land_len + WORK_EXPOSURE_PER_WATER_DIST * water_dist.min(MAX_WATER_SCAN))).clamp(SAFETY_MIN, 1.0)
}
fn mood_target(b: &Beaver) -> f32 { b.needs.mood() * (MOOD_HEALTH_BASE + MOOD_HEALTH_SCALE * b.health) }
fn harm_per_day(b: &Beaver) -> [f32; 5] {
    let (n, frail, years) = (&b.needs, if b.stage == Stage::Kit { KIT_FRAILTY } else { 1.0 }, b.age_years());
    let on = |h: bool, r: f32| if h { r } else { 0.0 };
    [on(n.food < STARVE_BELOW, STARVE_RATE * frail), on(n.warmth < FREEZE_BELOW, FREEZE_RATE * frail * crate::growth::cold_x(b)), 0.0,
     on(n.rest < EXHAUST_BELOW, EXHAUST_RATE), on(years > OLD_AGE_YEARS, OLD_AGE_RATE * (years - OLD_AGE_YEARS))]
}
fn heals(b: &Beaver, harm: &[f32; 5]) -> bool { harm[..4].iter().all(|h| *h == 0.0) && b.needs.food > HEAL_ABOVE && b.needs.warmth > HEAL_ABOVE }
fn day_of_year(t: f64) -> f64 { t.rem_euclid(DAYS_PER_YEAR) }

impl Colony {
    pub fn new(seed: u64) -> Self {
        Self {
            rng: Pcg32::seed_from_u64(seed ^ 0xC0_10_4E), time_days: 0.0, next_id: 1,
            beavers: Vec::new(), families: Vec::new(), lodges: Vec::new(), food: 0.0, stats: Stats::default(),
            immigration_by_year: Default::default(), ancestry: HashMap::new(), events: VecDeque::new(),
            events_dropped: 0, collapse: None, founded: false, slot_jobs: HashMap::new(),
            eaten: [0.0; 4], spoiled: 0.0, looks: Default::default(), sky: Default::default(),
        }
    }

    pub fn population(&self) -> usize { self.beavers.len() }
    pub fn count_stage(&self, stage: Stage) -> usize { self.beavers.iter().filter(|b| b.stage == stage).count() }
    pub fn beaver(&self, id: BeaverId) -> Option<&Beaver> { self.beavers.iter().find(|b| b.id == id) }
    pub fn family(&self, id: FamilyId) -> Option<&Family> { self.families.iter().find(|f| f.id == id) }
    pub fn is_founded(&self) -> bool { self.founded }
    pub fn is_collapsed(&self) -> bool { self.collapse.is_some() }
    pub fn take_events(&mut self) -> Vec<LoggedEvent> { self.events.drain(..).collect() }
    pub fn food_per_capita(&self) -> f32 { self.food / self.beavers.len().max(1) as f32 }
    pub(crate) fn lives_with_parent(&self, b: &Beaver) -> bool {
        b.lodge.is_some() && [b.mother, b.father].into_iter().flatten().any(|p| self.beaver(p).is_some_and(|p| p.lodge == b.lodge))
    }

    fn lodge_occupancy(&self) -> HashMap<LodgeId, usize> {
        let mut occ = HashMap::new();
        for b in &self.beavers { if let Some(l) = b.lodge { *occ.entry(l).or_insert(0) += 1; } }
        occ
    }

    pub fn found(&mut self, pos: (f32, f32), land: &dyn Landscape) -> Result<FamilyId, ColonyError> {
        if self.founded { return Err(ColonyError::AlreadyFounded); }
        if !(pos.0.is_finite() && pos.1.is_finite()) { return Err(ColonyError::Invalid("founding position must be finite".into())); }
        if land.is_water(pos.0, pos.1) { return Err(ColonyError::Invalid("founding position is in water".into())); }
        let (year, fam) = (DAYS_PER_YEAR as f32, self.new_family());
        let lodge = self.new_lodge(fam, pos, FOUNDING_LODGE_CONDITION);
        let father = self.spawn(Sex::Male, FOUNDER_ADULT_AGE_YEARS * year, fam, Some(lodge), pos, None, None);
        let mother = self.spawn(Sex::Female, FOUNDER_ADULT_AGE_YEARS * year, fam, Some(lodge), pos, None, None);
        self.set_mates(father, mother);
        let litter = MIN_LITTER + self.range(LITTER_SPREAD);
        if let Some(m) = self.beavers.iter_mut().find(|b| b.id == mother) {
            m.pregnancy = Some(Pregnancy { father, conceived_day: -FOUNDER_PREGNANCY_DAYS, due_day: GESTATION_DAYS - FOUNDER_PREGNANCY_DAYS, litter });
        }
        for sex in [Sex::Male, Sex::Female] {
            self.spawn(sex, FOUNDER_YEARLING_AGE_YEARS * year, fam, Some(lodge), pos, Some(father), Some(mother));
        }
        (self.food, self.founded, self.stats.peak_population) = (0.0, true, self.beavers.len());
        Ok(fam)
    }

    pub fn arrive(&mut self, pos: (f32, f32), land: &dyn Landscape) -> Result<FamilyId, ColonyError> {
        let fam = self.found(pos, land)?;
        self.lodges.clear();
        for b in &mut self.beavers { b.lodge = None; }
        Ok(fam)
    }

    pub fn add_home(&mut self, pos: (f32, f32)) -> LodgeId {
        let fam = self.beavers.iter().find(|b| b.lodge.is_none()).map_or(FamilyId(0), |b| b.family);
        let id = self.new_lodge(fam, pos, 1.0);
        self.house_homeless();
        id
    }

    pub fn house_homeless(&mut self) {
        let mut occ = self.lodge_occupancy();
        for b in self.beavers.iter_mut().filter(|b| b.lodge.is_none()) {
            let pick = self.lodges.iter_mut()
                .filter(|l| { let n = occ.get(&l.id).copied().unwrap_or(0); (l.family == b.family || n == 0) && n < LODGE_CAPACITY })
                .min_by_key(|l| (l.family != b.family, l.id));
            if let Some(l) = pick {
                (l.family, b.lodge) = (b.family, Some(l.id));
                *occ.entry(l.id).or_insert(0) += 1;
            }
        }
    }

    pub fn step_with_stores(&mut self, dt_days: f32, land: &dyn Landscape, stores: &mut crate::buildings::Stores) -> Result<(), ColonyError> {
        if !dt_days.is_finite() || dt_days < 0.0 { return Err(ColonyError::InvalidDt(dt_days)); }
        self.food = stores.amount(crate::buildings::Good::food());
        let mut left = dt_days;
        while left > 0.0 { let dt = left.min(SUBSTEP_DAYS); self.tick(dt, land, stores); left -= dt; }
        Ok(())
    }

    fn tick(&mut self, dt: f32, land: &dyn Landscape, stores: &mut crate::buildings::Stores) {
        self.time_days += dt as f64;
        if !self.founded { return; }
        let cold = crate::weather::cold(self.sky.at(self.time_days).feels_c);
        self.adopt_orphans();
        let plan = self.plan_work(dt, land);
        for (b, work) in self.beavers.iter_mut().zip(&plan) {
            b.current_job = work.map(|w| w.job);
            if let Some(w) = work {
                let s = &mut b.skills[w.job];
                *s = (*s + w.units * SKILL_GAIN * (1.0 - *s)).min(1.0);
                crate::growth::learn(b, w.units);
                b.needs.safety += (work_safety(w.land_len, w.water_dist) - b.needs.safety) * (SAFETY_RATE * dt).min(1.0);
            }
        }
        self.teach_young(dt);
        self.update_needs(dt, cold, stores);
        self.age_and_stage(dt);
        self.remove_dead(dt);
        self.pair_and_disperse();
        self.leave_home();
        self.births(dt);
        self.depart_unhappy();
        let live: HashSet<FamilyId> = self.beavers.iter().map(|b| b.family).collect();
        self.families.retain(|f| live.contains(&f.id));
        self.check_collapse();
    }

    fn plan_work(&self, dt: f32, land: &dyn Landscape) -> Vec<Option<Work>> {
        self.beavers.iter().map(|b| {
            let job = self.slot_jobs.get(&b.id).filter(|(_, w)| *w && b.can_work() && !b.resting)?.0;
            let units = dt * WORK_FRACTION * b.work_weight() * (WORK_REST_BASE + WORK_REST_SCALE * b.needs.rest)
                * (WORK_HEALTH_BASE + WORK_HEALTH_SCALE * b.health) * crate::society::work_speed(b.mood);
            Some(Work { job, units, land_len: if land.is_water(b.pos.0, b.pos.1) { 0.0 } else { 1.0 }, water_dist: land.water_distance(b.pos.0, b.pos.1) })
        }).collect()
    }

    fn teach_young(&mut self, dt: f32) {
        let lessons: Vec<_> = self.beavers.iter().enumerate().filter_map(|(i, b)| {
            let lodge = b.lodge.filter(|_| b.is_young())?;
            let job = self.beavers.iter().filter(|m| m.id != b.id && m.lodge == Some(lodge) && !m.is_young()).find_map(|m| m.current_job)?;
            Some((i, job))
        }).collect();
        for (i, job) in lessons {
            let s = &mut self.beavers[i].skills[job];
            *s = (*s + dt * YOUNG_LEARN_RATE * (1.0 - *s)).min(1.0);
        }
    }

    pub fn rates(&self, id: BeaverId, land: &dyn Landscape) -> Option<Rates> {
        let b = self.beaver(id)?;
        let (ins, cold) = (insulation(&self.lodges, b), crate::weather::cold(self.sky.at(self.time_days).feels_c));
        let (worked, sleep) = (if b.current_job.is_some() { WORK_FRACTION } else { IDLE_FRACTION }, lodge_insulation(&self.lodges, b).max(OPEN_REST));
        let rest = rest_per_day(b, worked, sleep) + if b.resting { RESTING_RECOVERY * sleep } else { 0.0 };
        let exposure = b.current_job.map(|_| (if land.is_water(b.pos.0, b.pos.1) { 0.0 } else { 1.0 }, land.water_distance(b.pos.0, b.pos.1)));
        let mates = b.lodge.map_or(0, |l| self.beavers.iter().filter(|m| m.id != b.id && m.lodge == Some(l)).count()).min(SOCIAL_MAX_MATES);
        let harm = harm_per_day(b);
        Some(Rates {
            hunger: HUNGER_PER_DAY * crate::growth::hunger_x(b), eat_below: EAT_BELOW, rest, sleep, resting: b.resting,
            warmth_to: warmth_target(cold, ins), cold, insulation: ins,
            safety_to: exposure.map_or(IDLE_SAFETY.min(1.0), |(l, d)| work_safety(l, d)), exposure,
            social_to: social_target(mates), mates, mood_to: mood_target(b), mood_health: MOOD_HEALTH_BASE + MOOD_HEALTH_SCALE * b.health,
            harm, heal: if heals(b, &harm) { HEAL_RATE * crate::growth::heal_x(b) } else { 0.0 },
            old_age_death: old_age_hazard(b.age_years()),
            leaves_in: (b.stage != Stage::Kit && b.mood < LOW_MOOD_BELOW).then(|| (DEPARTURE_BAD_MOOD_DAYS - b.low_mood_days).max(0.0)),
        })
    }

    fn update_needs(&mut self, dt: f32, cold: f32, stores: &mut crate::buildings::Stores) {
        use crate::buildings::{Good, Use};
        let occ = self.lodge_occupancy();
        let eaten = &mut self.eaten;
        for b in &mut self.beavers {
            let (ins, sleep) = (insulation(&self.lodges, b), lodge_insulation(&self.lodges, b).max(OPEN_REST));
            let worked = if b.current_job.is_some() { WORK_FRACTION } else { IDLE_FRACTION };
            let (rest_chg, hunger) = (rest_per_day(b, worked, sleep), HUNGER_PER_DAY * crate::growth::hunger_x(b));
            b.needs.food = (b.needs.food - hunger * dt).max(0.0);
            if b.needs.food < EAT_BELOW {
                let w = b.stage.food_weight();
                let take = ((1.0 - b.needs.food) * w).min(stores.amount(Good::food()));
                stores.withdraw_for(Good::food(), take, Use::eaten_by(b.stage)).expect("limited to stock");
                eaten[b.stage as usize] += take;
                b.needs.food += take / w;
            }
            b.needs.rest = (b.needs.rest + rest_chg * dt).clamp(0.0, 1.0);
            if b.resting {
                b.needs.rest = (b.needs.rest + RESTING_RECOVERY * sleep * dt).min(1.0);
                b.resting = b.needs.rest < RESUME_WORK_REST;
            } else if b.needs.rest < EXHAUSTED_REST { b.resting = true; }
            b.needs.warmth += (warmth_target(cold, ins) - b.needs.warmth) * (WARMTH_RATE * dt).min(1.0);
            if b.current_job.is_none() { b.needs.safety += (IDLE_SAFETY.min(1.0) - b.needs.safety) * (SAFETY_RATE * dt).min(1.0); }
            let mates = b.lodge.map_or(0, |l| occ.get(&l).copied().unwrap_or(1) - 1).min(SOCIAL_MAX_MATES);
            b.needs.social += (social_target(mates) - b.needs.social) * (SOCIAL_RATE * dt).min(1.0);
            let harm = harm_per_day(b);
            for (k, r) in harm.iter().enumerate() { b.harm[k] += r * dt; }
            b.health -= harm.iter().sum::<f32>() * dt;
            if b.health > 0.0 && heals(b, &harm) { b.health += HEAL_RATE * crate::growth::heal_x(b) * dt; }
            b.health = b.health.min(1.0);
            b.mood += (mood_target(b) - b.mood) * (MOOD_RATE * dt).min(1.0);
            b.low_mood_days = if b.mood < LOW_MOOD_BELOW { b.low_mood_days + dt } else { 0.0 };
        }
        let spoiled = (stores.amount(Good::food()) * FOOD_SPOILAGE * dt).min(stores.amount(Good::food()));
        stores.withdraw_for(Good::food(), spoiled, Use::Spoiled).expect("limited to stock");
        self.spoiled += spoiled;
        self.food = stores.amount(Good::food());
    }

    fn age_and_stage(&mut self, dt: f32) {
        let mut matured = Vec::new();
        for b in &mut self.beavers {
            let exact = b.age_days as f64 + b.age_remainder + dt as f64;
            (b.age_days, b.age_remainder) = (exact as f32, exact - (exact as f32) as f64);
            let s = Stage::of_age_days(b.age_days);
            if s != b.stage { b.stage = s; matured.push((b.id, s)); }
        }
        for (id, stage) in matured { self.log(Event::Matured { id, stage }); }
    }

    fn remove_dead(&mut self, dt: f32) {
        let worn: Vec<bool> = self.beavers.iter().map(|b| b.age_years()).collect::<Vec<_>>().into_iter().map(|y| {
            let h = old_age_hazard(y);
            h > 0.0 && self.unit() < 1.0 - (-h * dt / DAYS_PER_YEAR as f32).exp()
        }).collect();
        let dead: Vec<_> = self.beavers.iter().zip(worn).filter_map(|(b, w)| {
            if w { Some((b.id, DeathCause::OldAge, b.pos)) }
            else if b.health <= 0.0 {
                let (k, _) = b.harm.iter().enumerate().max_by(|a, c| a.1.total_cmp(c.1)).expect("non-empty");
                Some((b.id, HARM_CAUSES[k], b.pos))
            } else { None }
        }).collect();
        self.trace_last_state("DYING", &dead.iter().map(|d| (d.0, format!("{:?}", d.1))).collect::<Vec<_>>());
        self.stats.deaths += dead.len() as u32;
        self.remove_beavers(&dead.iter().map(|d| d.0).collect::<Vec<_>>());
        for (id, cause, at) in dead { self.log(Event::Died { id, cause, at }); }
    }

    pub fn kill(&mut self, id: BeaverId, cause: DeathCause) -> bool {
        let Some(at) = self.beaver(id).map(|b| b.pos) else { return false };
        self.trace_last_state("DYING", &[(id, format!("{cause:?}"))]);
        self.stats.deaths += 1;
        self.remove_beavers(&[id]);
        self.log(Event::Died { id, cause, at });
        true
    }

    fn depart_unhappy(&mut self) {
        let leaving: Vec<BeaverId> = self.beavers.iter().filter(|b| b.stage != Stage::Kit && b.low_mood_days >= DEPARTURE_BAD_MOOD_DAYS).map(|b| b.id).collect();
        self.trace_last_state("DEPARTING", &leaving.iter().map(|&id| (id, "low mood".into())).collect::<Vec<_>>());
        self.stats.departures += leaving.len() as u32;
        self.remove_beavers(&leaving);
        for id in leaving { self.log(Event::Departed { id }); }
    }

    fn trace_last_state(&self, what: &str, who: &[(BeaverId, String)]) {
        if !crate::trace::enabled() { return; }
        let temp = self.sky.at(self.time_days).temp_c;
        for (id, why) in who {
            if let Some(b) = self.beaver(*id) {
                crate::trace::event(format!("{what} day={:.4} why={why} temperature={temp:.2} food_cache={:.1} | {}", self.time_days, self.food, crate::journal::describe(b)));
            }
        }
    }

    fn remove_beavers(&mut self, ids: &[BeaverId]) {
        if ids.is_empty() { return; }
        let set: HashSet<BeaverId> = ids.iter().copied().collect();
        self.beavers.retain(|b| !set.contains(&b.id));
        for b in self.beavers.iter_mut().filter(|b| b.mate.is_some_and(|m| set.contains(&m))) { b.mate = None; }
    }

    fn adopt_orphans(&mut self) {
        self.house_homeless();
        let live: HashSet<_> = self.beavers.iter().map(|b| b.id).collect();
        let hosts: HashSet<_> = self.beavers.iter().filter(|b| b.stage == Stage::Adult).filter_map(|b| b.lodge).collect();
        let orphans: Vec<_> = self.beavers.iter()
            .filter(|b| b.stage != Stage::Adult && (b.mother.is_some() || b.father.is_some()) && ![b.mother, b.father].into_iter().flatten().any(|p| live.contains(&p)))
            .map(|b| b.id).collect();
        for id in orphans {
            if self.beaver(id).and_then(|b| b.lodge).is_some_and(|l| hosts.contains(&l)) { continue; }
            let home = self.lodges.iter().filter(|l| hosts.contains(&l.id) && self.beavers.iter().filter(|b| b.lodge == Some(l.id)).count() < LODGE_CAPACITY)
                .max_by(|a, b| a.condition.total_cmp(&b.condition)).cloned();
            if let Some(h) = home && let Some(b) = self.beavers.iter_mut().find(|b| b.id == id) {
                (b.family, b.lodge, b.pos) = (h.family, Some(h.id), h.pos);
            }
        }
    }

    fn can_breed(&self, m: &Beaver) -> Option<Result<(), &'static str>> {
        if m.sex != Sex::Female || m.stage != Stage::Adult || m.age_years() >= MAX_BIRTH_AGE_YEARS || m.pregnancy.is_some() { return None; }
        let birth_year = ((self.time_days + GESTATION_DAYS) / DAYS_PER_YEAR).floor() as i64;
        let lodge = self.lodges.iter().find(|l| Some(l.id) == m.lodge && l.family == m.family);
        let mate = m.mate.and_then(|id| self.beaver(id)).filter(|f| f.family == m.family && f.lodge == m.lodge && f.stage == Stage::Adult);
        let room = LODGE_CAPACITY.saturating_sub(self.beavers.iter().filter(|b| b.lodge == m.lodge).count());
        let daily_food = HUNGER_PER_DAY * (self.beavers.iter().map(|b| b.stage.food_weight()).sum::<f32>() + MIN_LITTER as f32 * KIT_FOOD_WEIGHT);
        Some(if m.last_birth_year == birth_year { Err("Had a litter this year") }
            else if m.health < BREED_MIN_HEALTH { Err("Too weak") }
            else if m.needs.food < BREED_MIN_FOOD { Err("Hungry") }
            else if lodge.is_none() { Err("No family lodge") }
            else if lodge.is_some_and(|l| l.condition < BREED_MIN_HOME_CONDITION) { Err("Lodge needs repair") }
            else if mate.is_none() { Err("No mate at home") }
            else if room < MIN_LITTER as usize { Err("Lodge full") }
            else if self.food < daily_food * BIRTH_MIN_FOOD_DAYS { Err("Food caches too low") }
            else { Ok(()) })
    }

    pub fn conception(&self, id: BeaverId) -> Option<Result<f32, &'static str>> {
        let ready = self.can_breed(self.beaver(id)?)?;
        Some(ready.and_then(|()| if (MATING_FROM_DAY..MATING_TO_DAY).contains(&day_of_year(self.time_days)) { Ok(CONCEPTION_CHANCE_PER_DAY) } else { Err("Out of season") }))
    }

    fn births(&mut self, dt: f32) {
        let due: Vec<_> = self.beavers.iter().filter_map(|m| m.pregnancy.as_ref().filter(|p| self.time_days >= p.due_day).map(|p| (m.id, m.family, m.lodge, m.pos, p.father, p.litter))).collect();
        for (mid, fam, lodge, pos, father, litter) in due {
            let mother = self.beavers.iter_mut().find(|b| b.id == mid).unwrap();
            (mother.pregnancy, mother.last_birth_year) = (None, (self.time_days / DAYS_PER_YEAR).floor() as i64);
            for _ in 0..litter {
                let sex = if self.chance(MALE_SHARE) { Sex::Male } else { Sex::Female };
                let id = self.spawn(sex, 0.0, fam, lodge, pos, Some(father), Some(mid));
                self.stats.births += 1;
                self.log(Event::Born { id, family: fam });
            }
        }
        self.stats.peak_population = self.stats.peak_population.max(self.beavers.len());
        if !(MATING_FROM_DAY..MATING_TO_DAY).contains(&day_of_year(self.time_days)) { return; }
        let cands: Vec<_> = self.beavers.iter().filter(|m| self.can_breed(m) == Some(Ok(()))).map(|m| (m.id, m.mate.unwrap(), m.lodge)).collect();
        for (id, father, home) in cands {
            let room = LODGE_CAPACITY.saturating_sub(self.beavers.iter().filter(|b| b.lodge == home).count());
            if !self.chance(CONCEPTION_CHANCE_PER_DAY * dt) { continue; }
            let litter = (MIN_LITTER + self.range(LITTER_SPREAD)).min(room as u32);
            crate::trace::event(format!("CONCEIVED day={:.4} mother={} father={father:?} litter={litter} due_day={:.2} lodge_room={room} food={:.1}", self.time_days, id.0, self.time_days + GESTATION_DAYS, self.food));
            self.beavers.iter_mut().find(|b| b.id == id).unwrap().pregnancy = Some(Pregnancy { father, conceived_day: self.time_days, due_day: self.time_days + GESTATION_DAYS, litter });
        }
    }

    fn leave_home(&mut self) {
        let mut empty: Vec<LodgeId> = self.lodges.iter().filter(|l| !self.beavers.iter().any(|b| b.lodge == Some(l.id))).map(|l| l.id).collect();
        if empty.is_empty() { return; }
        let mut leaving: Vec<_> = self.beavers.iter()
            .filter(|b| b.stage == Stage::Adult && b.mate.is_none() && b.age_years() >= LEAVE_HOME_AGE_YEARS && self.lives_with_parent(b))
            .map(|b| (b.age_days, b.id)).collect();
        leaving.sort_by(|a, b| b.0.total_cmp(&a.0).then(a.1.cmp(&b.1)));
        empty.reverse();
        for (_, id) in leaving {
            let Some(home) = empty.pop() else { break };
            let fam = self.new_family();
            let l = self.lodges.iter_mut().find(|l| l.id == home).unwrap();
            (l.family, ) = (fam,);
            let site = l.pos;
            let b = self.beavers.iter_mut().find(|b| b.id == id).unwrap();
            (b.family, b.lodge, b.pos) = (fam, Some(home), site);
            self.log(Event::LeftHome { id, lodge: home });
        }
    }

    pub fn close_kin(&self, a: BeaverId, b: BeaverId) -> bool {
        let (Some(x), Some(y)) = (self.beaver(a), self.beaver(b)) else { return false };
        let (px, py) = ([x.mother, x.father], [y.mother, y.father]);
        px.contains(&Some(b)) || py.contains(&Some(a)) || px.into_iter().flatten().any(|p| py.contains(&Some(p)))
    }

    fn pair_and_disperse(&mut self) {
        let doy = day_of_year(self.time_days);
        if !(SPRING_START..SPRING_END).contains(&doy) && !(MATING_FROM_DAY..MATING_TO_DAY).contains(&doy) { return; }
        let mut eligible: Vec<_> = self.beavers.iter().filter(|b| b.stage == Stage::Adult && b.mate.is_none() && b.health >= PAIR_MIN_HEALTH).map(|b| (b.id, b.sex)).collect();
        for i in (1..eligible.len()).rev() { let j = self.range(i as u32 + 1) as usize; eligible.swap(i, j); }
        let mut used = HashSet::new();
        for &(a, sex) in &eligible {
            if used.contains(&a) { continue; }
            let opts: Vec<_> = eligible.iter().filter(|&&(b, s)| b != a && s != sex && !used.contains(&b) && !self.close_kin(a, b)).map(|p| p.0).collect();
            if opts.is_empty() { continue; }
            let b = opts[self.range(opts.len() as u32) as usize];
            let reuse = [a, b].into_iter().find_map(|id| {
                let own = self.beaver(id)?;
                let home = own.lodge?;
                self.lodges.iter().find(|l| l.id == home && l.family == own.family
                    && self.beavers.iter().filter(|x| x.lodge == Some(home) && x.stage == Stage::Adult).all(|x| x.id == a || x.id == b)).map(|l| l.id)
            });
            let Some(home) = reuse.or_else(|| self.lodges.iter().find(|l| !self.beavers.iter().any(|x| x.lodge == Some(l.id))).map(|l| l.id)) else { continue };
            let fam = match reuse { Some(id) => self.lodges.iter().find(|l| l.id == id).unwrap().family, None => self.new_family() };
            let l = self.lodges.iter_mut().find(|l| l.id == home).unwrap();
            l.family = fam;
            let site = l.pos;
            for x in self.beavers.iter_mut().filter(|x| x.id == a || x.id == b || (x.stage != Stage::Adult && [x.mother, x.father].into_iter().flatten().any(|p| p == a || p == b))) {
                (x.family, x.lodge, x.pos) = (fam, Some(home), site);
            }
            used.extend([a, b]);
            self.set_mates(a, b);
            self.log(Event::NewFamily { family: fam, lodge: home, a, b });
        }
    }

    pub fn migration_remaining(&self) -> u32 {
        if self.is_collapsed() { return 0; }
        let year = (self.time_days / DAYS_PER_YEAR).floor() as i64;
        MAX_IMMIGRANTS_PER_YEAR.saturating_sub(self.immigration_by_year.get(&year).copied().unwrap_or(0))
    }

    fn check_collapse(&mut self) {
        if self.collapse.is_some() { return; }
        let cause = if self.beavers.is_empty() { CollapseCause::Extinct } else if !self.beavers.iter().any(|b| b.stage == Stage::Adult) { CollapseCause::NoAdults } else { return };
        let r = CollapseReport { day: self.time_days, cause, peak_population: self.stats.peak_population, deaths: self.stats.deaths, departures: self.stats.departures };
        self.collapse = Some(r.clone());
        self.log(Event::Collapsed(r));
    }

    fn fresh(&mut self) -> u64 { let id = self.next_id; self.next_id += 1; id }
    fn new_family(&mut self) -> FamilyId { let id = FamilyId(self.fresh()); self.families.push(Family { id, name: String::new() }); id }
    fn new_lodge(&mut self, family: FamilyId, pos: (f32, f32), condition: f32) -> LodgeId { let id = LodgeId(self.fresh()); self.lodges.push(Lodge { id, family, pos, condition }); id }

    #[allow(clippy::too_many_arguments)]
    fn spawn(&mut self, sex: Sex, age_days: f32, family: FamilyId, lodge: Option<LodgeId>, pos: (f32, f32), father: Option<BeaverId>, mother: Option<BeaverId>) -> BeaverId {
        let id = BeaverId(self.fresh());
        let mut skills: crate::buildings::PerId<Job, f32> = Default::default();
        if age_days >= ADULT_DAYS { for s in skills.values_mut() { *s = START_SKILL_MAX * self.unit(); } }
        let experience = crate::growth::START_EXPERIENCE_MAX * skills.values().sum::<f32>() / (skills.values().count().max(1) as f32 * START_SKILL_MAX);
        self.beavers.push(Beaver {
            id, name: String::new(), age_days, age_remainder: 0.0, sex, family, mother, father, mate: None,
            skills, experience, needs: Needs::satisfied(), mood: START_MOOD, health: 1.0, job: None, current_job: None,
            lodge, pos, stage: Stage::of_age_days(age_days), resting: false, in_lodge: false, last_birth_year: -1,
            pregnancy: None, low_mood_days: 0.0, harm: [0.0; 5],
        });
        self.ancestry.insert(id, (father, mother));
        let looks = crate::looks::Looks::of_kit(crate::looks::key(id, self.time_days, pos), mother.map(|m| self.looks_of(m)), father.map(|f| self.looks_of(f)));
        self.looks.insert(id, looks);
        id
    }

    pub fn looks_of(&self, id: BeaverId) -> crate::looks::Looks {
        if let Some(l) = self.looks.get(&id) { return *l; }
        let (father, mother) = self.ancestry.get(&id).copied().unwrap_or((None, None));
        let parent = |p: Option<BeaverId>| p.filter(|p| p.0 < id.0).map(|p| self.looks_of(p));
        crate::looks::Looks::of_kit(crate::looks::key(id, 0.0, (0.0, 0.0)), parent(mother), parent(father))
    }

    fn set_mates(&mut self, a: BeaverId, b: BeaverId) {
        crate::trace::event(format!("PAIRED day={:.4} a={} b={}", self.time_days, a.0, b.0));
        for (x, y) in [(a, b), (b, a)] { if let Some(v) = self.beavers.iter_mut().find(|v| v.id == x) { v.mate = Some(y); } }
    }

    pub(crate) fn note(&mut self, event: Event) { self.log(event); }
    fn log(&mut self, event: Event) {
        if self.events.len() >= EVENT_LOG_CAP { self.events.pop_front(); self.events_dropped += 1; }
        self.events.push_back(LoggedEvent { day: self.time_days, event });
    }
    fn unit(&mut self) -> f32 { (self.rng.next_u32() >> 8) as f32 / (1u32 << 24) as f32 }
    fn chance(&mut self, p: f32) -> bool { self.unit() < p }
    fn range(&mut self, n: u32) -> u32 { self.rng.next_u32() % n }

    pub fn admit_wanderer(&mut self, adult: bool) -> Result<BeaverId, ColonyError> {
        if !self.founded { return Err(ColonyError::Invalid("colony is not founded".into())); }
        if self.migration_remaining() == 0 { return Err(ColonyError::Invalid(format!("annual migration limit reached ({MAX_IMMIGRANTS_PER_YEAR} a year)"))); }
        let waiting = adult.then(|| self.beavers.iter().find(|b| b.stage == Stage::Adult && b.mate.is_none()
            && b.lodge.is_some() && self.beavers.iter().filter(|x| x.lodge == b.lodge && x.stage == Stage::Adult).count() == 1).cloned()).flatten();
        let empty = self.lodges.iter().find(|l| !self.beavers.iter().any(|b| b.lodge == Some(l.id))).cloned();
        let sex = if self.chance(MALE_SHARE) { Sex::Male } else { Sex::Female };
        let (home, fam, pos) = if let Some(b) = waiting {
            (b.lodge.unwrap(), b.family, b.pos)
        } else if let Some(l) = empty {
            let fam = self.new_family();
            self.lodges.iter_mut().find(|h| h.id == l.id).unwrap().family = fam;
            (l.id, fam, l.pos)
        } else {
            return Err(ColonyError::Invalid("a newcomer needs an empty family lodge or an unpaired adult's home".into()));
        };
        let years = if adult { IMMIGRANT_MIN_AGE_YEARS + IMMIGRANT_AGE_SPREAD_YEARS * self.unit() } else { 1.0 + self.unit() };
        let id = self.spawn(sex, years * DAYS_PER_YEAR as f32, fam, Some(home), pos, None, None);
        self.stats.peak_population = self.stats.peak_population.max(self.beavers.len());
        let year = (self.time_days / DAYS_PER_YEAR).floor() as i64;
        *self.immigration_by_year.entry(year).or_default() += 1;
        self.stats.immigrants += 1;
        self.log(Event::Immigrated { id });
        Ok(id)
    }

    pub fn add_immigrants(&mut self, n: u32) -> u32 {
        (0..n).take_while(|_| self.admit_wanderer(true).is_ok()).count() as u32
    }
}
