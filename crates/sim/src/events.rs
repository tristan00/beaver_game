use rand_core::{Rng, SeedableRng};
use rand_pcg::Pcg32;
use std::fmt;
pub use crate::tuning::events::*;

#[derive(Clone, Debug, PartialEq)]
pub enum EventError {
    InvalidCalendar(u32),
    InvalidContext(&'static str),
    UnknownChoice(u64),
    UnknownOption { choice: u64, option: usize },
    Disabled(EventKind),
}

impl fmt::Display for EventError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EventError::InvalidCalendar(d) => write!(f, "days_per_season must be >= 1, got {d}"),
            EventError::InvalidContext(n) => write!(f, "event context field `{n}` is not finite"),
            EventError::UnknownChoice(id) => write!(f, "no pending choice {id}"),
            EventError::Disabled(k) => write!(f, "event {k:?} is disabled"),
            EventError::UnknownOption { choice, option } => {
                write!(f, "choice {choice} has no option {option}")
            }
        }
    }
}
impl std::error::Error for EventError {}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Season {
    Spring,
    Summer,
    Autumn,
    Winter,
}

impl Season {
    pub const ALL: [Season; 4] = [Season::Spring, Season::Summer, Season::Autumn, Season::Winter];
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Date {
    pub year: u32,
    pub season: Season,
    pub day: u32,
}

impl fmt::Display for Date {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Year {} {:?} day {}", self.year + 1, self.season, self.day)
    }
}

#[derive(Clone, Debug)]
pub struct Calendar {
    days_per_season: u32,
    elapsed: f64,
}

impl Calendar {
    pub fn new(days_per_season: u32) -> Result<Self, EventError> {
        if days_per_season == 0 {
            return Err(EventError::InvalidCalendar(days_per_season));
        }
        Ok(Self { days_per_season, elapsed: 0.0 })
    }
    pub fn days_per_season(&self) -> u32 {
        self.days_per_season
    }
    pub fn days_per_year(&self) -> u32 {
        self.days_per_season * 4
    }
    pub fn elapsed_days(&self) -> f64 {
        self.elapsed
    }
    pub fn advance(&mut self, dt: f64) {
        self.elapsed += dt;
    }
    pub fn date(&self) -> Date {
        let whole = self.elapsed.floor() as u64;
        let dps = self.days_per_season as u64;
        let season_idx = whole / dps;
        Date {
            year: (season_idx / 4) as u32,
            season: Season::ALL[(season_idx % 4) as usize],
            day: (whole % dps) as u32 + 1,
        }
    }
    pub fn season(&self) -> Season {
        self.date().season
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum EventKind {
    Flood,
    Drought,
    HardFreeze,
    Fire,
    Disease,
    PredatorRaid,
    WanderingBeaver,
    WetlandArrival,
}

impl EventKind {
    pub const ALL: [EventKind; 8] = [
        EventKind::Flood,
        EventKind::Drought,
        EventKind::HardFreeze,
        EventKind::Fire,
        EventKind::Disease,
        EventKind::PredatorRaid,
        EventKind::WanderingBeaver,
        EventKind::WetlandArrival,
    ];
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EventContext {
    pub population: u32,
    pub wetland: f32,
    pub forest: f32,
    pub danger: f32,
    pub mosquitoes: f32,
    pub water_level: f32,
}

impl Default for EventContext {
    fn default() -> Self {
        Self { population: 10, wetland: 0.3, forest: 0.5, danger: 0.3, mosquitoes: 0.2, water_level: 0.5 }
    }
}

impl EventContext {
    fn sanitized(&self) -> Result<Self, EventError> {
        let fields = [
            ("wetland", self.wetland),
            ("forest", self.forest),
            ("danger", self.danger),
            ("mosquitoes", self.mosquitoes),
            ("water_level", self.water_level),
        ];
        for (n, v) in fields {
            if !v.is_finite() {
                return Err(EventError::InvalidContext(n));
            }
        }
        let c = |v: f32| v.clamp(0.0, 1.0);
        Ok(Self {
            population: self.population,
            wetland: c(self.wetland),
            forest: c(self.forest),
            danger: c(self.danger),
            mosquitoes: c(self.mosquitoes),
            water_level: c(self.water_level),
        })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Effect {
    Food(f32),
    Wood(f32),
    Deaths(u32),
    Illness(f32),
    Arrivals(u32),
    Inflow { factor: f32, days: f32 },
}

pub trait EventHost {
    fn population(&self) -> u32;
    fn food(&self) -> f32;
    fn wood(&self) -> f32;
    fn add_food(&mut self, amount: f32);
    fn add_wood(&mut self, amount: f32);
    fn kill_beavers(&mut self, n: u32) -> u32;
    fn damage_health(&mut self, amount: f32);
    fn add_beavers(&mut self, n: u32) -> u32;
}

#[derive(Clone, Debug, PartialEq)]
pub struct Stockpile {
    pub population: u32,
    pub food: f32,
    pub wood: f32,
    pub health: f32,
}

impl EventHost for Stockpile {
    fn population(&self) -> u32 {
        self.population
    }
    fn food(&self) -> f32 {
        self.food
    }
    fn wood(&self) -> f32 {
        self.wood
    }
    fn add_food(&mut self, amount: f32) {
        self.food = (self.food + amount).max(0.0);
    }
    fn add_wood(&mut self, amount: f32) {
        self.wood = (self.wood + amount).max(0.0);
    }
    fn kill_beavers(&mut self, n: u32) -> u32 {
        let k = n.min(self.population);
        self.population -= k;
        k
    }
    fn damage_health(&mut self, amount: f32) {
        self.health = (self.health - amount).clamp(0.0, 1.0);
    }
    fn add_beavers(&mut self, n: u32) -> u32 {
        self.population += n;
        n
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ChoiceOption {
    pub label: String,
    pub effects: Vec<Effect>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PendingChoice {
    pub id: u64,
    pub kind: EventKind,
    pub date: Date,
    pub day: f64,
    pub prompt: String,
    pub options: Vec<ChoiceOption>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct EventRecord {
    pub id: u64,
    pub kind: EventKind,
    pub date: Date,
    pub day: f64,
    pub description: String,
    pub effects: Vec<Effect>,
    pub choice: Option<(String, bool)>,
}

#[derive(Clone, Debug)]
struct ActiveDrought {
    remaining: f32,
}

#[derive(Clone, Debug)]
pub struct Events {
    rng: Pcg32,
    pub calendar: Calendar,
    log: Vec<EventRecord>,
    pending: Vec<PendingChoice>,
    next_id: u64,
    drought: Option<ActiveDrought>,
    inflow_factor: f32,
    inflow_days: f32,
    muted: Vec<EventKind>,
}

impl Default for Events {
    fn default() -> Self {
        Self::new(0)
    }
}

impl Events {
    pub fn new(seed: u64) -> Self {
        Self::with_calendar(seed, DEFAULT_DAYS_PER_SEASON).expect("default calendar is valid")
    }

    pub fn with_calendar(seed: u64, days_per_season: u32) -> Result<Self, EventError> {
        Ok(Self {
            rng: Pcg32::seed_from_u64(seed ^ 0xE7E1_7500),
            calendar: Calendar::new(days_per_season)?,
            log: Vec::new(),
            pending: Vec::new(),
            next_id: 1,
            drought: None,
            inflow_factor: 1.0,
            inflow_days: 0.0,
            muted: Vec::new(),
        })
    }

    pub fn set_muted(&mut self, kinds: Vec<EventKind>) {
        self.muted = kinds;
    }
    pub fn muted(&self) -> &[EventKind] {
        &self.muted
    }

    pub fn log(&self) -> &[EventRecord] {
        &self.log
    }
    pub fn pending(&self) -> &[PendingChoice] {
        &self.pending
    }
    pub fn date(&self) -> Date {
        self.calendar.date()
    }
    pub fn is_drought(&self) -> bool {
        self.drought.is_some()
    }
    pub fn water_inflow_factor(&self) -> f32 {
        if self.inflow_days > 0.0 { self.inflow_factor } else { 1.0 }
    }

    pub fn step(&mut self, dt_days: f32) {
        self.advance_time(dt_days as f64);
    }

    fn advance_time(&mut self, dt: f64) {
        self.calendar.advance(dt);
        let d = dt as f32;
        if self.inflow_days > 0.0 {
            self.inflow_days = (self.inflow_days - d).max(0.0);
        }
        if let Some(dr) = &mut self.drought {
            dr.remaining -= d;
            if dr.remaining <= 0.0 {
                self.drought = None;
            }
        }
    }

    pub fn step_with(
        &mut self,
        dt_days: f32,
        ctx: &EventContext,
        host: &mut dyn EventHost,
    ) -> Result<Vec<u64>, EventError> {
        let mut ctx = ctx.sanitized()?;
        let mut produced = Vec::new();
        let mut left = dt_days as f64;
        while left > 0.0 {
            let sub = left.min(1.0);
            left -= sub;
            self.advance_time(sub);
            ctx.population = host.population();
            self.expire_choices(host, &mut produced);
            for kind in EventKind::ALL {
                if self.muted.contains(&kind) {
                    continue;
                }
                let p = self.probability(kind, &ctx) * sub as f32;
                if p > 0.0 && self.unit() < p {
                    produced.push(self.fire(kind, &ctx, host));
                }
            }
        }
        Ok(produced)
    }

    pub fn trigger(&mut self, kind: EventKind, ctx: &EventContext, host: &mut dyn EventHost) -> Result<u64, EventError> {
        if self.muted.contains(&kind) {
            return Err(EventError::Disabled(kind));
        }
        let mut ctx = ctx.sanitized()?;
        ctx.population = host.population();
        Ok(self.fire(kind, &ctx, host))
    }

    pub fn resolve(&mut self, choice: u64, option: usize, host: &mut dyn EventHost) -> Result<(), EventError> {
        let idx = self.pending.iter().position(|c| c.id == choice).ok_or(EventError::UnknownChoice(choice))?;
        if option >= self.pending[idx].options.len() {
            return Err(EventError::UnknownOption { choice, option });
        }
        let c = self.pending.remove(idx);
        self.settle(c, option, false, host);
        Ok(())
    }

    fn settle(&mut self, c: PendingChoice, option: usize, expired: bool, host: &mut dyn EventHost) {
        let opt = &c.options[option];
        for e in &opt.effects {
            self.apply(e, host);
        }
        if let Some(r) = self.log.iter_mut().find(|r| r.id == c.id) {
            r.effects = opt.effects.clone();
            r.choice = Some((opt.label.clone(), expired));
        }
    }

    fn expire_choices(&mut self, host: &mut dyn EventHost, produced: &mut Vec<u64>) {
        let now = self.calendar.elapsed_days();
        let (expired, keep): (Vec<_>, Vec<_>) =
            std::mem::take(&mut self.pending).into_iter().partition(|c| now - c.day >= CHOICE_EXPIRY_DAYS);
        self.pending = keep;
        for c in expired {
            produced.push(c.id);
            self.settle(c, 0, true, host);
        }
    }

    fn apply(&mut self, e: &Effect, host: &mut dyn EventHost) {
        match *e {
            Effect::Food(a) => host.add_food(a),
            Effect::Wood(a) => host.add_wood(a),
            Effect::Deaths(n) => {
                host.kill_beavers(n);
            }
            Effect::Illness(a) => host.damage_health(a),
            Effect::Arrivals(n) => {
                host.add_beavers(n);
            }
            Effect::Inflow { factor, days } => {
                self.inflow_factor = factor;
                self.inflow_days = days;
                if factor < 1.0 {
                    self.drought = Some(ActiveDrought { remaining: days });
                } else {
                    self.drought = None;
                }
            }
        }
    }

    fn probability(&self, kind: EventKind, c: &EventContext) -> f32 {
        let s = self.calendar.season();
        let drought = self.is_drought();
        let has_pop = c.population > 0;
        use EventKind::*;
        use Season::*;
        match kind {
            Flood if !drought && matches!(s, Spring | Summer) && c.water_level > FLOOD_MIN_WATER_LEVEL => {
                FLOOD_CHANCE
                    * (if s == Spring { FLOOD_SPRING_MULT } else { FLOOD_SUMMER_MULT })
                    * (FLOOD_WATER_BASE + c.water_level)
            }
            Drought if !drought && matches!(s, Summer | Autumn) => DROUGHT_CHANCE * (DROUGHT_WATER_BASE - c.water_level),
            HardFreeze if s == Winter && has_pop => HARD_FREEZE_CHANCE,
            Fire if fire_season(s) && c.forest > FIRE_MIN_FOREST => {
                FIRE_CHANCE * c.forest * (if drought { FIRE_DROUGHT_MULT } else { 1.0 })
            }
            Disease if c.population >= DISEASE_MIN_POPULATION => {
                DISEASE_CHANCE
                    * (DISEASE_MOSQUITO_BASE + c.mosquitoes * DISEASE_MOSQUITO_SCALE)
                    * (c.population as f32 / DISEASE_CROWD_POPULATION).min(DISEASE_CROWD_MAX)
            }
            PredatorRaid if has_pop => PREDATOR_RAID_CHANCE * c.danger,
            WanderingBeaver if s != Winter => WANDERING_BEAVER_CHANCE,
            WetlandArrival if c.wetland > WETLAND_ARRIVAL_MIN_WETLAND && matches!(s, Spring | Summer) => {
                WETLAND_ARRIVAL_CHANCE * c.wetland
            }
            _ => 0.0,
        }
    }

    fn unit(&mut self) -> f32 {
        (self.rng.next_u32() >> 8) as f32 / (1u32 << 24) as f32
    }
    fn between(&mut self, (lo, hi): (f32, f32)) -> f32 {
        lo + (hi - lo) * self.unit()
    }

    fn fire(&mut self, kind: EventKind, c: &EventContext, host: &mut dyn EventHost) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        let pop = c.population.max(1) as f32;
        let opt = |label: &str, effects: Vec<Effect>| ChoiceOption { label: label.to_string(), effects };
        let (desc, effects, options): (String, Vec<Effect>, Vec<ChoiceOption>) = match kind {
            EventKind::Flood => {
                let wood = -self.between(FLOOD_WOOD_LOSS) * pop.sqrt();
                let food = -self.between(FLOOD_FOOD_LOSS) * pop.sqrt();
                ("Spring melt floods the valley; stores wash away.".into(), vec![Effect::Wood(wood), Effect::Food(food)], vec![])
            }
            EventKind::Drought => {
                let days = self.between(DROUGHT_SEASONS) * self.calendar.days_per_season() as f32;
                ("A drought dries the streams; water inflow drops.".into(), vec![Effect::Inflow { factor: DROUGHT_INFLOW, days }], vec![])
            }
            EventKind::HardFreeze => {
                let food = -self.between(HARD_FREEZE_FOOD_LOSS) * pop.sqrt();
                ("A hard freeze ices the pond; foraging is difficult.".into(), vec![Effect::Food(food), Effect::Illness(HARD_FREEZE_ILLNESS)], vec![])
            }
            EventKind::Fire => {
                let loss = self.between(FIRE_WOOD_LOSS) * pop.sqrt();
                (
                    "Lightning sets the forest alight!".into(),
                    vec![],
                    vec![
                        opt("Fight the fire with mud and water", vec![Effect::Wood(-loss * FIRE_FIGHT_LOSS_SHARE), Effect::Illness(FIRE_FIGHT_ILLNESS)]),
                        opt("Let it burn", vec![Effect::Wood(-loss), Effect::Food(-loss * FIRE_BURN_FOOD_SHARE)]),
                    ],
                )
            }
            EventKind::Disease => {
                let dmg = self.between(DISEASE_ILLNESS);
                let deaths = (c.population as f32 * self.between(DISEASE_DEATH_SHARE)).floor() as u32;
                ("A fever spreads through the lodges.".into(), vec![Effect::Illness(dmg), Effect::Deaths(deaths)], vec![])
            }
            EventKind::PredatorRaid => {
                let deaths = ((c.population as f32 * self.between(RAID_DEATH_SHARE)).ceil() as u32).max(RAID_MIN_DEATHS);
                let food = -self.between(RAID_HIDE_FOOD_LOSS) * pop.sqrt();
                (
                    "Predators prowl near the lodges.".into(),
                    vec![],
                    vec![
                        opt("Stand and drive them off", vec![Effect::Deaths(deaths / RAID_FIGHT_DEATH_DIVISOR), Effect::Illness(RAID_FIGHT_ILLNESS)]),
                        opt("Hide in the lodges", vec![Effect::Food(food)]),
                    ],
                )
            }
            EventKind::WanderingBeaver => {
                let n = WANDERERS_MIN + (self.unit() * WANDERERS_SPREAD) as u32;
                (
                    "Wandering beavers ask to join the colony.".into(),
                    vec![],
                    vec![
                        opt("Welcome them", vec![Effect::Arrivals(n), Effect::Food(-WANDERER_FOOD_COST * n as f32)]),
                        opt("Turn them away", vec![]),
                    ],
                )
            }
            EventKind::WetlandArrival => {
                let food = self.between(WETLAND_ARRIVAL_FOOD) * (1.0 + c.wetland);
                ("Herons, frogs and waterfowl settle in the wetlands.".into(), vec![Effect::Food(food)], vec![])
            }
        };
        let day = self.calendar.elapsed_days();
        let date = self.calendar.date();
        for e in &effects {
            self.apply(e, host);
        }
        let mut record = EventRecord { id, kind, date, day, description: desc.clone(), effects, choice: None };
        if !options.is_empty() {
            record.description = desc.clone();
            self.pending.push(PendingChoice { id, kind, date, day, prompt: desc, options });
        }
        self.log.push(record);
        id
    }
}

fn fire_season(s: Season) -> bool {
    matches!(s, Season::Summer | Season::Autumn)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn host() -> Stockpile {
        Stockpile { population: 20, food: 100.0, wood: 100.0, health: 1.0 }
    }

    fn run(seed: u64, days: u32) -> (Events, Stockpile) {
        let mut ev = Events::new(seed);
        let mut h = host();
        for _ in 0..days {
            ev.step_with(1.0, &EventContext::default(), &mut h).unwrap();
            h.population = h.population.max(10);
        }
        (ev, h)
    }

    #[test]
    fn calendar_seasons_and_config() {
        assert_eq!(Calendar::new(0).unwrap_err(), EventError::InvalidCalendar(0));
        let mut c = Calendar::new(5).unwrap();
        assert_eq!(c.date(), Date { year: 0, season: Season::Spring, day: 1 });
        c.advance(5.0);
        assert_eq!(c.date().season, Season::Summer);
        c.advance(15.0);
        assert_eq!(c.date(), Date { year: 1, season: Season::Spring, day: 1 });
        c.advance(4.5);
        assert_eq!(c.date().day, 5);
    }

    #[test]
    fn events_deterministic_for_seed() {
        let (a, ha) = run(7, 400);
        let (b, hb) = run(7, 400);
        assert_eq!(a.log(), b.log());
        assert_eq!(ha, hb);
        assert!(!a.log().is_empty());
        let (c, _) = run(8, 400);
        assert_ne!(a.log(), c.log());
    }

    #[test]
    fn events_independent_of_step_chunking() {
        let ctx = EventContext::default();
        let (mut a, mut b) = (Events::new(3), Events::new(3));
        let (mut ha, mut hb) = (host(), host());
        for _ in 0..40 {
            a.step_with(1.0, &ctx, &mut ha).unwrap();
        }
        b.step_with(40.0, &ctx, &mut hb).unwrap();
        assert_eq!(a.log(), b.log());
    }

    #[test]
    fn events_log_records_dates_and_all_kinds_reachable() {
        let mut ev = Events::new(1);
        let mut h = host();
        let ctx = EventContext::default();
        for k in EventKind::ALL {
            ev.trigger(k, &ctx, &mut h).unwrap();
        }
        assert_eq!(ev.log().len(), 8);
        assert!(ev.log().iter().all(|r| r.date == Date { year: 0, season: Season::Spring, day: 1 }));
        assert_eq!(ev.pending().len(), 3);
    }

    #[test]
    fn events_drought_lowers_inflow_then_recovers() {
        let mut ev = Events::new(1);
        let mut h = host();
        assert_eq!(ev.water_inflow_factor(), 1.0);
        ev.trigger(EventKind::Drought, &EventContext::default(), &mut h).unwrap();
        assert!(ev.is_drought());
        assert!(ev.water_inflow_factor() < 1.0);
        ev.step(1000.0);
        assert!(!ev.is_drought());
        assert_eq!(ev.water_inflow_factor(), 1.0);
    }

    #[test]
    fn events_choice_resolution_and_expiry() {
        let ctx = EventContext::default();
        let mut ev = Events::new(1);
        let mut h = host();
        let id = ev.trigger(EventKind::WanderingBeaver, &ctx, &mut h).unwrap();
        assert_eq!(h.population, 20);
        assert_eq!(ev.resolve(id, 9, &mut h), Err(EventError::UnknownOption { choice: id, option: 9 }));
        ev.resolve(id, 0, &mut h).unwrap();
        assert!(h.population > 20);
        assert_eq!(ev.resolve(id, 0, &mut h), Err(EventError::UnknownChoice(id)));
        assert_eq!(ev.log()[0].choice.as_ref().unwrap().1, false);

        let id = ev.trigger(EventKind::PredatorRaid, &ctx, &mut h).unwrap();
        ev.step_with(CHOICE_EXPIRY_DAYS as f32 + 1.0, &EventContext { danger: 0.0, ..ctx }, &mut h).unwrap();
        assert!(ev.pending().iter().all(|c| c.id != id));
        let rec = ev.log().iter().find(|r| r.id == id).unwrap();
        assert_eq!(rec.choice.as_ref().unwrap().1, true);
    }

    #[test]
    fn events_reject_bad_context() {
        let mut ev = Events::new(1);
        let mut h = host();
        let bad = EventContext { danger: f32::NAN, ..Default::default() };
        assert_eq!(ev.step_with(1.0, &bad, &mut h), Err(EventError::InvalidContext("danger")));
    }

    #[test]
    fn events_stores_never_negative() {
        let mut h = Stockpile { population: 3, food: 1.0, wood: 1.0, health: 1.0 };
        let mut ev = Events::new(1);
        ev.trigger(EventKind::Flood, &EventContext::default(), &mut h).unwrap();
        assert_eq!((h.food, h.wood), (0.0, 0.0));
    }
}
