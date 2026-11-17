//! Pure, deterministic simulation core (no rendering dependencies).
//! `SimState` composes one struct per system; the game crate only reads it,
//! advances it with `step`, and mutates it through `apply`.

pub mod agents;
pub mod buildings;
pub mod catalogue;
pub mod colony;
pub mod dam_preview;
pub mod ecology;
pub mod events;
pub mod features;
pub mod grid;
pub mod nav;
pub mod region;
pub mod resources;
pub mod society;
pub mod starts;
pub mod tech;
pub mod tuning;
pub mod water;
pub mod work;
pub mod worldgen;

use std::fmt;

pub use grid::Grid;

/// Errors returned by `SimState` operations. Nothing fails silently.
#[derive(Clone, Debug, PartialEq)]
pub enum SimError {
    /// `dt_days` was NaN, infinite or negative.
    InvalidDt(f32),
    /// A command carried an invalid argument.
    InvalidCommand(String),
}

impl fmt::Display for SimError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SimError::InvalidDt(dt) => write!(f, "invalid dt_days: {dt} (must be finite and >= 0)"),
            SimError::InvalidCommand(m) => write!(f, "invalid command: {m}"),
        }
    }
}
impl std::error::Error for SimError {}

/// Player/UI requests that mutate the simulation.
#[derive(Clone, Debug, PartialEq)]
pub enum Command {
    /// Rebuild the world from a new seed (resets time).
    Regenerate { seed: u64 },
    /// Build dams of `height` above terrain on `cells`.
    BuildDam { cells: Vec<water::Cell>, height: f32 },
    /// Remove the dam at (x, y); the pond drains downstream.
    BreachDam { x: usize, y: usize },
    /// Dig canals `depth` deep on `cells`.
    DigCanal { cells: Vec<water::Cell>, depth: f32 },
    /// Send beavers to collect (`Some`) or stand down (`None`). Right-click and area drag.
    Assign { who: Vec<colony::BeaverId>, to: Option<agents::Assignment> },
    /// Place a construction site (no cost taken; builders haul the materials in).
    PlaceSite { kind: buildings::BuildingKind, pos: (usize, usize) },
    /// Put a beaver into a workplace slot / empty a slot.
    FillSlot { building: buildings::BuildingId, slot: usize, who: colony::BeaverId },
    EmptySlot { building: buildings::BuildingId, slot: usize },
    /// Move or resize a workplace's work area.
    SetWorkArea { building: buildings::BuildingId, area: work::Area },
    /// Standing haul between two stores.
    SetHaul { order: work::HaulOrder },
    /// Accept or turn away a wandering beaver.
    AnswerJoin { id: society::RequestId, answer: society::Answer },
}

/// Complete simulation state.
pub struct SimState {
    pub seed: u64,
    /// Elapsed simulated days.
    pub time_days: f64,
    pub height: grid::HeightGrid,
    /// Full generated region (valleys, rivers, lakes, moisture, forest).
    pub world: worldgen::World,
    pub water: water::Water,
    pub ecology: ecology::Ecology,
    pub colony: colony::Colony,
    pub buildings: buildings::Buildings,
    pub tech: tech::Tech,
    pub region: region::Region,
    pub events: events::Events,
    /// The one switch for out-of-scope systems.
    pub features: features::Features,
    /// Navigation surfaces and cell reservations.
    pub nav: nav::NavGrid,
    pub resources: resources::ResourceWorld,
    pub agents: agents::Agents,
    pub work: work::Work,
    pub society: society::Society,
}

impl SimState {
    /// Builds a world deterministically from `seed`.
    pub fn new(seed: u64) -> Self {
        let world = worldgen::WorldGen::new(seed).generate().expect("default world generation failed");
        let height = world.height.clone();
        let size = world.size;
        let resources = resources::ResourceWorld::seeded(seed, &world);
        let mut sim = Self {
            seed,
            time_days: 0.0,
            height,
            world,
            water: water::Water::new(size),
            ecology: ecology::Ecology::seeded(size, seed),
            colony: Default::default(),
            buildings: Default::default(),
            tech: Default::default(),
            region: region::Region::new(seed),
            events: events::Events::new(seed),
            features: features::Features::default(),
            nav: nav::NavGrid::new(size, size),
            resources,
            agents: agents::Agents::default(),
            work: work::Work::default(),
            society: society::Society::default(),
        };
        sim.apply_features();
        sim
    }

    /// Replaces the feature config and applies it at once.
    pub fn set_features(&mut self, features: features::Features) {
        self.features = features;
        self.apply_features();
    }

    /// Pushes `self.features` into every system that must never fire when off. Idempotent; called
    /// on construction and at the start of every step so direct edits of `features` also hold.
    fn apply_features(&mut self) {
        use features::System;
        let f = self.features;
        self.events.set_muted(events::EventKind::ALL.into_iter().filter(|k| !f.allows_event(*k)).collect());
        self.colony.set_disabled_jobs(colony::Job::ALL.into_iter().filter(|j| !f.allows_job(*j)).collect());
        if !f.enabled(System::Predators) && !self.ecology.predators.is_empty() {
            self.ecology.predators.clear();
        }
    }

    /// Advances every system by `dt_days`. Errors if `dt_days` is not finite and >= 0.
    pub fn step(&mut self, dt_days: f32) -> Result<(), SimError> {
        if !dt_days.is_finite() || dt_days < 0.0 {
            return Err(SimError::InvalidDt(dt_days));
        }
        self.apply_features();
        self.time_days += dt_days as f64;
        self.water.step(&self.height, dt_days);
        self.ecology.step(dt_days, &self.water.depth)?;
        let land = WorldLand { world: &self.world, depth: &self.water.depth };
        self.colony.step_on(dt_days, &land).map_err(|_| SimError::InvalidDt(dt_days))?;
        let terrain = buildings::Terrain { height: &self.height, depth: &self.water.depth };
        self.tech.update(&self.colony);
        self.buildings.step(dt_days, &self.tech, terrain, &self.colony);
        if self.features.enabled(features::System::Rivals) {
            self.region.step(dt_days);
        }
        self.resources.step(dt_days).expect("dt validated above");
        agents::step(self, dt_days);
        self.society.step(dt_days, self.time_days);
        self.step_events(dt_days);
        Ok(())
    }

    /// Rolls random events against the colony, applies their effects and delivers migrants.
    fn step_events(&mut self, dt_days: f32) {
        if !self.colony.is_founded() {
            self.events.step(dt_days);
            return;
        }
        let ctx = self.event_context();
        let mut host = ColonyHost { colony: &mut self.colony };
        if self.events.step_with(dt_days, &ctx, &mut host).is_err() {
            self.events.step(dt_days);
        }
        let arrivals = if self.features.enabled(features::System::Rivals) { self.region.take_arrivals() } else { Vec::new() };
        for m in arrivals {
            self.colony.add_immigrants(m.count);
        }
    }

    /// The world as the event rules see it (coarse samples; cheap enough to take every step).
    pub fn event_context(&self) -> events::EventContext {
        let mean = |g: &Grid<f32>| {
            let s = g.as_slice();
            if s.is_empty() { 0.0 } else { s.iter().step_by(7).sum::<f32>() / s.iter().step_by(7).count() as f32 }
        };
        let wet = {
            let s = self.water.depth.as_slice();
            s.iter().step_by(7).filter(|d| **d > water_wet()).count() as f32 / s.iter().step_by(7).count().max(1) as f32
        };
        events::EventContext {
            population: self.colony.population() as u32,
            wetland: mean(&self.ecology.wetland),
            forest: mean(&self.ecology.forest),
            danger: 0.3,
            mosquitoes: (mean(&self.ecology.wetland) * 0.7).min(1.0),
            water_level: (wet * 4.0).min(1.0),
        }
    }

    /// Places a construction site for `kind` at `pos` and registers it as a workplace with its
    /// catalogue cost and (tech-adjusted) build work.
    pub fn place_site(&mut self, kind: buildings::BuildingKind, pos: (usize, usize)) -> Result<buildings::BuildingId, SimError> {
        let terrain = buildings::Terrain { height: &self.height, depth: &self.water.depth };
        let id = self
            .buildings
            .place_site(kind, pos, &self.tech, terrain)
            .map_err(|e| SimError::InvalidCommand(e.to_string()))?;
        let spec = kind.spec();
        let labour = spec.work * self.tech.build_work_factor();
        if let Err(e) = self.work.add_site(id, kind, spec.cost, labour) {
            self.buildings.buildings.retain(|b| b.id != id);
            return Err(SimError::InvalidCommand(format!("{e:?}")));
        }
        Ok(id)
    }

    /// Applies a command. Errors on invalid arguments, leaving state unchanged.
    pub fn apply(&mut self, cmd: Command) -> Result<(), SimError> {
        match cmd {
            Command::Regenerate { seed } => {
                *self = SimState::new(seed);
                Ok(())
            }
            Command::BuildDam { cells, height } => self.water.build_dam(&cells, height).map_err(invalid),
            Command::BreachDam { x, y } => self.water.breach(x, y).map_err(invalid),
            Command::DigCanal { cells, depth } => {
                self.water.dig_canal(&mut self.height, &cells, depth).map_err(invalid)
            }
            Command::Assign { who, to } => {
                for id in who {
                    if !self.agents.assign(id, to) {
                        return Err(SimError::InvalidCommand(format!("no agent {}", id.0)));
                    }
                }
                Ok(())
            }
            Command::PlaceSite { kind, pos } => self.place_site(kind, pos).map(|_| ()),
            Command::FillSlot { building, slot, who } => {
                let stage = self.colony.beaver(who).map(|b| b.stage).ok_or(work::WorkError::NoSuchBeaver);
                stage
                    .and_then(|st| if st == colony::Stage::Kit { Err(work::WorkError::NotAdult) } else { Ok(()) })
                    .and_then(|()| self.work.assign(building, slot, who))
                    .map_err(|e| SimError::InvalidCommand(format!("{e:?}")))
            }
            Command::EmptySlot { building, slot } => {
                self.work.unassign(building, slot).map_err(|e| SimError::InvalidCommand(format!("{e:?}")))
            }
            Command::SetWorkArea { building, area } => {
                self.work.set_area(building, area).map_err(|e| SimError::InvalidCommand(format!("{e:?}")))
            }
            Command::SetHaul { order } => {
                let store = |id| {
                    self.buildings.building(id).is_some_and(|b| b.is_complete() && agents::accepts(b.kind, order.good))
                };
                if order.from == order.to || !store(order.from) || !store(order.to) {
                    return Err(SimError::InvalidCommand(format!("haul needs two distinct complete stores accepting the good: {order:?}")));
                }
                self.work.hauls.push(order);
                Ok(())
            }
            Command::AnswerJoin { id, answer } => {
                self.society.answer(id, answer).map_err(|e| SimError::InvalidCommand(format!("{e:?}")))
            }
        }
    }
}

/// Terrain seen by the colony: rivers, lakes and ponded water are water.
pub struct WorldLand<'a> {
    pub world: &'a worldgen::World,
    pub depth: &'a grid::Grid<f32>,
}

fn water_wet() -> f32 {
    buildings::WET_DEPTH
}

/// Lets events and trade read and change the colony's stores and beavers.
pub struct ColonyHost<'a> {
    pub colony: &'a mut colony::Colony,
}

impl events::EventHost for ColonyHost<'_> {
    fn population(&self) -> u32 {
        self.colony.population() as u32
    }
    fn food(&self) -> f32 {
        self.colony.food
    }
    fn wood(&self) -> f32 {
        self.colony.wood
    }
    fn add_food(&mut self, amount: f32) {
        self.colony.food = (self.colony.food + amount).max(0.0);
    }
    fn add_wood(&mut self, amount: f32) {
        self.colony.wood = (self.colony.wood + amount).max(0.0);
    }
    fn kill_beavers(&mut self, n: u32) -> u32 {
        self.colony.kill_beavers(n)
    }
    fn damage_health(&mut self, amount: f32) {
        self.colony.damage_health(amount);
    }
    fn add_beavers(&mut self, n: u32) -> u32 {
        self.colony.add_immigrants(n)
    }
}

impl colony::Landscape for WorldLand<'_> {
    fn is_water(&self, x: f32, y: f32) -> bool {
        if !(x >= 0.0 && y >= 0.0) {
            return false;
        }
        let (cx, cy) = (x as usize, y as usize);
        self.world.river.get(cx, cy).copied().unwrap_or(false)
            || self.world.lake.get(cx, cy).copied().unwrap_or(false)
            || self.depth.get(cx, cy).is_some_and(|d| *d > 0.05)
    }
}

fn invalid(e: water::WaterError) -> SimError { SimError::InvalidCommand(e.to_string()) }

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_is_deterministic() {
        assert_eq!(SimState::new(42).height, SimState::new(42).height);
    }

    #[test]
    fn step_advances_time() {
        let mut s = SimState::new(1);
        s.step(0.5).unwrap();
        s.step(0.25).unwrap();
        assert_eq!(s.time_days, 0.75);
    }

    #[test]
    fn step_rejects_bad_dt() {
        let mut s = SimState::new(1);
        for dt in [-1.0, f32::NAN, f32::INFINITY] {
            assert!(matches!(s.step(dt), Err(SimError::InvalidDt(_))));
        }
        assert_eq!(s.time_days, 0.0);
    }

    #[test]
    fn regenerate_changes_seed_and_resets_time() {
        let mut s = SimState::new(1);
        s.step(2.0).unwrap();
        s.apply(Command::Regenerate { seed: 9 }).unwrap();
        assert_eq!(s.seed, 9);
        assert_eq!(s.time_days, 0.0);
        assert_eq!(s.height, SimState::new(9).height);
    }

    fn founded(seed: u64) -> SimState {
        let mut s = SimState::new(seed);
        let c = s.world.valleys[s.world.start.0 as usize].center;
        let land = WorldLand { world: &s.world, depth: &s.water.depth };
        s.colony.found((c.0 as f32, c.1 as f32), &land).expect("found");
        s
    }

    #[test]
    fn scoped_colony_two_years_fires_nothing_from_disabled_systems() {
        let mut s = founded(2);
        assert_eq!(s.features, features::Features::scoped());
        let two_years = 2.0 * 4.0 * events::DEFAULT_DAYS_PER_SEASON as f32;
        let mut t = 0.0;
        while t < two_years {
            s.step(0.25).unwrap();
            assert!(!s.events.is_drought());
            assert_eq!(s.events.water_inflow_factor(), 1.0);
            t += 0.25;
        }
        let ev = s.events.log();
        assert!(ev.iter().all(|r| s.features.allows_event(r.kind)), "disabled event fired: {ev:?}");
        assert!(ev.iter().all(|r| !r.effects.iter().any(|e| matches!(e, events::Effect::Inflow { .. }))));
        assert!(s.ecology.predators.is_empty());
        assert_eq!(s.colony.scouted, 0.0);
        assert_eq!(s.colony.experience[colony::Job::Guard.index()], 0.0);
        assert_eq!(s.colony.experience[colony::Job::Scout.index()], 0.0);
        assert_eq!(s.region.time_days, 0.0, "rival region must not advance");
        assert!(s.region.offers().is_empty());
        assert!(s.colony.beavers.iter().all(|b| b.current_job.is_none_or(|j| s.features.allows_job(j))));
    }

    #[test]
    fn disabled_systems_reject_explicit_use() {
        let mut s = founded(3);
        let id = s.colony.beavers[0].id;
        assert!(s.colony.set_job(id, Some(colony::Job::Guard)).is_err());
        assert!(s.colony.set_job(id, Some(colony::Job::Scout)).is_err());
        let mut host = events::Stockpile { population: 5, food: 10.0, wood: 10.0, health: 1.0 };
        let r = s.events.trigger(events::EventKind::Flood, &events::EventContext::default(), &mut host);
        assert_eq!(r, Err(events::EventError::Disabled(events::EventKind::Flood)));
        assert!(!catalogue::available(&s).contains(&buildings::BuildingKind::GuardPost));
        s.set_features(features::Features::everything());
        assert!(s.colony.set_job(id, Some(colony::Job::Guard)).is_ok());
        assert!(catalogue::available(&s).contains(&buildings::BuildingKind::GuardPost));
    }
}
