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
        let water = water::Water::from_world(&world);
        Self {
            seed,
            time_days: 0.0,
            height,
            world,
            water,
            ecology: ecology::Ecology::seeded(size, seed),
            colony: Default::default(),
            buildings: Default::default(),
            tech: Default::default(),
            region: region::Region::new(seed),
            events: events::Events::new(seed),
            features: features::Features::default(),
            nav: nav::NavGrid::new(size, size),
            resources: resources::ResourceWorld::default(),
            agents: agents::Agents::default(),
            work: work::Work::default(),
            society: society::Society::default(),
        }
    }

    /// Advances every system by `dt_days`. Errors if `dt_days` is not finite and >= 0.
    pub fn step(&mut self, dt_days: f32) -> Result<(), SimError> {
        if !dt_days.is_finite() || dt_days < 0.0 {
            return Err(SimError::InvalidDt(dt_days));
        }
        self.time_days += dt_days as f64;
        self.water.step(&self.height, dt_days);
        self.ecology.step(dt_days, &self.water.depth)?;
        let land = WorldLand { world: &self.world, depth: &self.water.depth };
        self.colony.step_on(dt_days, &land).map_err(|_| SimError::InvalidDt(dt_days))?;
        let terrain = buildings::Terrain { height: &self.height, depth: &self.water.depth };
        self.tech.update(&self.colony);
        self.buildings.step(dt_days, &self.tech, terrain, &self.colony);
        self.region.step(dt_days);
        self.resources.step(dt_days);
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
        for m in self.region.take_arrivals() {
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
            Command::FillSlot { building, slot, who } => {
                self.work.assign(building, slot, who).map_err(|e| SimError::InvalidCommand(format!("{e:?}")))
            }
            Command::EmptySlot { building, slot } => {
                self.work.unassign(building, slot).map_err(|e| SimError::InvalidCommand(format!("{e:?}")))
            }
            Command::SetWorkArea { building, area } => {
                self.work.set_area(building, area).map_err(|e| SimError::InvalidCommand(format!("{e:?}")))
            }
            Command::SetHaul { order } => {
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
}
