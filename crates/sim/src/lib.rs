mod colony_bridge;
mod command;
mod construction;

pub use colony_bridge::WorldLand;
pub use command::{Command, DamGrowth};
pub use canals::CanalGrowth;
pub use gates::GateSetting;

pub mod agents;
pub mod autoplay;
pub mod buildings;
pub mod calendar;
pub mod catalogue;
pub mod colony;
pub mod canals;
pub mod dams;
pub mod gates;
pub mod dam_preview;
pub mod ecology;
pub mod fog;
pub mod events;
pub mod forestry;
pub mod grid;
pub mod hud;
pub mod jobs;
pub mod inventory;
pub mod ledger;
pub mod maps;
pub mod maintenance;
pub mod nav;
pub mod output;
pub mod resources;
pub mod runtime;
pub mod sandbox;
pub mod script;
pub mod site;
pub mod society;
pub mod starts;
pub mod stock;
pub mod trace;

pub mod tuning;
pub mod water;
pub mod wetland;
pub mod work;
pub mod worldgen;

use std::fmt;

pub use grid::Grid;

#[derive(Clone, Debug, PartialEq)]
pub enum SimError {
    InvalidDt(f32),
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

// Fifteen-minute colony decisions. The routed water solve is implicit and does
// not require wave-sized time steps; all game speeds share this same calendar.
pub const STEP_DAYS: f32 = 1.0 / 96.0;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct StepTimings {
    pub steps: u64,
    pub water_s: f64,
    pub ecology_s: f64,
    pub colony_s: f64,
    pub buildings_s: f64,
    pub agents_s: f64,
    pub shipping_s: f64,
    pub fog_s: f64,
    pub other_s: f64,
}

impl StepTimings {
    pub fn total_s(&self) -> f64 {
        self.water_s + self.ecology_s + self.colony_s + self.buildings_s + self.agents_s + self.shipping_s + self.fog_s + self.other_s
    }
}

pub struct SimState {
    pub seed: u64,
    pub instance: u64,
    pub client_commands: Option<Vec<Command>>,
    pub time_days: f64,
    pub height: grid::HeightGrid,
    pub world: worldgen::World,
    pub water: water::Water,
    pub ecology: ecology::Ecology,
    pub colony: colony::Colony,
    pub buildings: buildings::Buildings,
    pub events: events::Events,
    pub nav: nav::NavGrid,
    pub resources: resources::ResourceWorld,
    pub agents: agents::Agents,
    pub work: work::Work,
    pub maintenance: maintenance::Maintenance,
    pub society: society::Society,
    pub ledger: ledger::Ledger,
    pub output: output::Output,
    pub fog: fog::Fog,
    pub steps: u64,
    pub recorder: Option<Vec<(u64, Command)>>,
    pub forestry: forestry::Forestry,
    pub sandbox: bool,
    pub step_timings: StepTimings,
}

impl SimState {
    pub fn new(seed: u64) -> Self {
        let world = worldgen::fixture(seed);
        Self::from_world(seed, world)
    }

    pub fn from_world(seed: u64, world: worldgen::World) -> Self {
        let height = world.height.clone();
        let size = world.size;
        let water = water::Water::from_world(&world);
        let resources = resources::ResourceWorld::seeded(seed, &world);
        let mut sim = Self {
            seed,
            instance: { static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1); NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed) },
            client_commands: None,
            time_days: 0.0,
            height,
            world,
            water,
            ecology: ecology::Ecology::seeded(size, seed),
            colony: Default::default(),
            buildings: Default::default(),
            events: events::Events::new(seed),
            nav: nav::NavGrid::new(size, size),
            resources,
            agents: agents::Agents::default(),
            work: work::Work::default(),
            maintenance: Default::default(),
            society: society::Society::default(),
            ledger: ledger::Ledger::default(),
            output: output::Output::default(),
            fog: fog::Fog::clear(size),
            steps: 0,
            recorder: None,
            forestry: forestry::Forestry::new(seed),
            sandbox: false,
            step_timings: StepTimings::default(),
        };
        sim.work.inventory = sim.buildings.stores.inventory.view();
        forestry::init(&mut sim);
        sim
    }

    /// Gameplay, sandbox and scripted playtests share the same physical clock.
    pub fn step(&mut self, dt_days: f32) -> Result<(), SimError> {
        if !dt_days.is_finite() || dt_days < 0.0 {
            return Err(SimError::InvalidDt(dt_days));
        }
        if self.sandbox {
            let clock = std::time::Instant::now();
            self.time_days += dt_days as f64;
            self.steps += 1;
            self.water.step(&self.height, dt_days);
            self.step_timings.water_s += clock.elapsed().as_secs_f64();
            self.step_timings.steps += 1;
            return Ok(());
        }
        let mut clock = std::time::Instant::now();
        let mut lap = |slot: &mut f64| {
            let now = std::time::Instant::now();
            *slot += (now - clock).as_secs_f64();
            clock = now;
        };
        let mut t = self.step_timings;
        self.time_days += dt_days as f64;
        self.steps += 1;
        lap(&mut t.other_s);
        self.water.step(&self.height, dt_days);
        lap(&mut t.water_s);
        self.ecology.step(dt_days, &self.water.depth)?;
        lap(&mut t.ecology_s);
        self.derive_colony_jobs();
        let land = WorldLand { world: &self.world, depth: &self.water.depth };
        self.colony.step_with_stores(dt_days, &land, &mut self.buildings.stores).map_err(|_| SimError::InvalidDt(dt_days))?;
        lap(&mut t.colony_s);
        let terrain = buildings::Terrain { depth: &self.water.depth };
        self.buildings.step(dt_days, terrain);
        self.sync_homes();
        maintenance::step(self, dt_days);
        lap(&mut t.buildings_s);
        self.resources.step(dt_days, self.time_days).expect("dt validated above");
        let day = self.time_days.floor();
        if day > (self.time_days - dt_days as f64).floor() && self.colony.is_founded() {
            wetland::step(self, day as u64);
        }
        forestry::step(self, dt_days);
        self.refresh_store_view();
        lap(&mut t.other_s);
        agents::step(self, dt_days);
        lap(&mut t.agents_s);
        dams::step(self);
        canals::step(self, dt_days);
        lap(&mut t.shipping_s);
        self.explore();
        lap(&mut t.fog_s);
        self.society.step(dt_days, self.time_days, &self.colony);
        self.step_events(dt_days);

        self.output.step(dt_days);
        self.refresh_store_view();
        self.ledger.record(self.time_days, &self.buildings.stores);
        lap(&mut t.other_s);
        t.steps += 1;
        self.step_timings = t;
        Ok(())
    }

    fn explore(&mut self) {
        let world = &self.world;
        let allowed = fog::open_at(world);
        for a in self.agents.by_id.values().filter(|a| !a.inside) {
            self.fog.reveal((a.cell.0 as f32 + 0.5, a.cell.1 as f32 + 0.5), fog::BEAVER_SIGHT, &allowed);
        }
    }

}

fn invalid(e: water::WaterError) -> SimError { SimError::InvalidCommand(e.to_string()) }

impl Clone for SimState {
    fn clone(&self) -> Self {
        let mut copy = Self {
            seed: self.seed,
            instance: self.instance,
            client_commands: self.client_commands.clone(),
            time_days: self.time_days.clone(),
            height: self.height.clone(),
            world: self.world.clone(),
            water: self.water.clone(),
            ecology: self.ecology.clone(),
            colony: self.colony.clone(),
            buildings: self.buildings.clone(),
            events: self.events.clone(),
            nav: self.nav.clone(),
            resources: self.resources.clone(),
            agents: self.agents.clone(),
            work: self.work.clone(),
            maintenance: self.maintenance.clone(),
            society: self.society.clone(),
            ledger: self.ledger.clone(),
            output: self.output.clone(),
            fog: self.fog.clone(),
            steps: self.steps.clone(),
            recorder: self.recorder.clone(),
            forestry: self.forestry.clone(),
            sandbox: self.sandbox.clone(),
            step_timings: self.step_timings.clone(),
        };
        copy.work.inventory = copy.buildings.stores.inventory.view();
        copy
    }
}
