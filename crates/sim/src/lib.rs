mod colony_bridge;
mod command;
mod construction;

pub use colony_bridge::WorldLand;
pub use command::{Command, DamGrowth};
pub use canals::CanalGrowth;
pub use gates::GateSetting;

pub mod agents;
pub mod buildings;
pub mod calendar;
pub mod catalogue;
pub mod colony;
pub mod canals;
pub mod dams;
pub mod deeds;
pub mod names;
pub mod gates;
pub mod raft_landing;
pub mod locks;
pub mod wildlife;
pub mod lives;
pub mod looks;
pub mod dam_preview;
pub mod ecology;
pub mod fog;
pub mod events;
pub mod forestry;
pub mod grid;
pub mod hud;
pub mod jobs;
pub mod save;
pub mod journal;
pub mod inventory;
pub mod ledger;
pub mod maps;
pub mod maintenance;
pub mod nav;
pub mod predators;
pub mod prof;
pub mod output;
pub mod piles;
pub mod plants;
pub mod resources;
pub mod run;
pub mod runtime;
pub mod sandbox;
pub mod script;
pub mod site;
pub mod society;
pub mod starts;
pub mod stock;
pub mod supply;
pub mod trace;
pub mod trees;

pub mod tuning;
pub mod water;
pub mod weather;
pub mod wetland;
pub mod work;
pub mod worldgen;

use std::fmt;


pub use grid::Grid;

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
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

#[derive(Clone, Copy, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
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

#[derive(serde::Serialize, serde::Deserialize)]
pub struct SimState {
    pub seed: u64,
    pub instance: u64,
    #[serde(skip)]
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
    #[serde(skip)]
    pub recorder: Option<Vec<(u64, Command)>>,
    pub forestry: forestry::Forestry,
    pub predators: predators::Predators,
    /// Gates upgraded into locks (saved after the world; see `save`).
    #[serde(skip)]
    pub locks: locks::Locks,
    /// Each raft landing's rafts (saved after the world; see `save`).
    #[serde(skip)]
    pub fleet: raft_landing::Fleet,
    /// Rabbits, deer and birds (saved after the world; see `save`).
    #[serde(skip)]
    pub wildlife: wildlife::Wildlife,
    /// Every beaver's life story (saved after the world; see `save`).
    #[serde(skip)]
    pub lives: lives::Lives,
    /// The run's tallies (saved after the world; see `save`).
    #[serde(skip)]
    pub run: run::RunStats,
    /// Ponds' names.
    pub names: names::Names,
    /// Each beaver's life's work and epithet.
    pub deeds: deeds::Deeds,
    /// Materials brought to gates, locks and buildings being mended.
    pub supplies: supply::Supplies,
    pub sandbox: bool,
    #[serde(skip)]
    pub step_timings: StepTimings,
    /// What the session journal has written so far (only used while a session trace is open).
    #[serde(skip)]
    pub journal: journal::Journal,
}

/// A number no other world in this run has: the interface restarts its beaver when it changes.
pub fn next_instance() -> u64 {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
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
            instance: next_instance(),
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
            predators: predators::Predators::new(seed),
            locks: locks::Locks::default(),
            fleet: raft_landing::Fleet::default(),
            wildlife: wildlife::Wildlife::new(seed),
            lives: lives::Lives::default(),
            run: run::RunStats::default(),
            names: names::Names::default(),
            deeds: deeds::Deeds::default(),
            supplies: supply::Supplies::default(),
            sandbox: false,
            step_timings: StepTimings::default(),
            journal: journal::Journal::default(),
        };
        sim.colony.sky = sim.water.sky;
        sim.work.inventory = sim.buildings.stores.inventory.view();
        forestry::clear_drowned(&mut sim);
        forestry::init(&mut sim);
        sim.resources.log_changes = true;
        sim
    }

    /// Gameplay, sandbox and scripted playtests share the same physical clock.
/// From here on this copy of the world is a guess: whoever shows the world applies commands
    /// to its copy before the simulation has answered, to show at once what they should do.
    /// What changes from now is marked as guessed, to be looked at again when the answer comes.
    pub fn guess_ahead(&mut self) {
        self.resources.guess_ahead();
    }

    pub fn step(&mut self, dt_days: f32) -> Result<(), SimError> {
        if !dt_days.is_finite() || dt_days < 0.0 {
            return Err(SimError::InvalidDt(dt_days));
        }
        if self.sandbox {
            let clock = std::time::Instant::now();
            self.time_days += dt_days as f64;
            self.steps += 1;
            // The sandbox is for watching the water answer each change at once.
            self.water.take_ground(&self.height);
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
        let whole = prof::scope("step");
        { let _p = prof::scope("step/journal"); journal::begin_step(self); }
        self.time_days += dt_days as f64;
        self.steps += 1;
        trace::set_clock(self.time_days, self.steps);
        lap(&mut t.other_s);
        { let _p = prof::scope("step/water"); self.water.step(&self.height, dt_days); }
        lap(&mut t.water_s);
        { let _p = prof::scope("step/ecology"); self.ecology.step(dt_days, &self.water.depth)?; }
        lap(&mut t.ecology_s);
        {
            let _p = prof::scope("step/colony");
            { let _p = prof::scope("step/colony/jobs"); self.derive_colony_jobs(); }
            let land = WorldLand { world: &self.world, depth: &self.water.depth };
            { let _p = prof::scope("step/colony/beavers"); self.colony.step_with_stores(dt_days, &land, &mut self.buildings.stores).map_err(|_| SimError::InvalidDt(dt_days))?; }
            { let _p = prof::scope("step/colony/names"); names::step(self); }
            let _p = prof::scope("step/journal");
            journal::scan(self, "colony");
        }
        lap(&mut t.colony_s);
        {
            let _p = prof::scope("step/buildings");
            let terrain = buildings::Terrain { depth: &self.water.depth };
            { let _p = prof::scope("step/buildings/step"); self.buildings.step(dt_days, terrain); }
            { let _p = prof::scope("step/buildings/homes"); self.sync_homes(); }
            let _p = prof::scope("step/buildings/maintenance");
            maintenance::step(self, dt_days);
        }
        lap(&mut t.buildings_s);
        { let _p = prof::scope("step/resources"); self.resources.step(dt_days, self.time_days).expect("dt validated above"); }
        let day = self.time_days.floor();
        if day > (self.time_days - dt_days as f64).floor() {
            let _p = prof::scope("step/wetland");
            wetland::seed_water_plants(self);
            if self.colony.is_founded() {
                wetland::step(self, day as u64);
            }
        }
        { let _p = prof::scope("step/forestry"); forestry::step(self, dt_days); }
        { let _p = prof::scope("step/stores"); self.refresh_store_view(); }
        lap(&mut t.other_s);
        { let _p = prof::scope("step/agents"); agents::step(self, dt_days); }
        { let _p = prof::scope("step/predators"); predators::step(self, dt_days); }
        { let _p = prof::scope("step/wildlife"); wildlife::step(self, dt_days); }
        wildlife::note_tracks(self);
        { let _p = prof::scope("step/lives"); lives::step(self); }
        lap(&mut t.agents_s);
        { let _p = prof::scope("step/dams"); dams::step(self); }
        { let _p = prof::scope("step/canals"); canals::step(self, dt_days); }
        { let _p = prof::scope("step/locks"); locks::tidy(self); }
        { let _p = prof::scope("step/raft_landing"); raft_landing::step(self); }
        lap(&mut t.shipping_s);
        { let _p = prof::scope("step/fog"); self.explore(); }
        lap(&mut t.fog_s);
        { let _p = prof::scope("step/society"); self.society.step(dt_days, self.time_days, &mut self.colony); }
        { let _p = prof::scope("step/events"); self.step_events(dt_days); }
        { let _p = prof::scope("step/deeds"); deeds::step(self, dt_days); names::step(self); }
        {
            let _p = prof::scope("step/journal");
            journal::scan(self, "agents");
            journal::poll(self);
        }

        {
            let _p = prof::scope("step/ledger");
            self.output.step(dt_days);
            self.refresh_store_view();
            // Counting the colony walks every tree: only when a day closes.
            let closes = self.ledger.closes_day(self.time_days);
            let census = if closes { self.census() } else { ledger::Census::default() };
            if closes {
                run::close_day(self, census.water);
            }
            self.ledger.record(self.time_days, &self.buildings.stores, move || census);
        }
        drop(whole);
        lap(&mut t.other_s);
        t.steps += 1;
        self.step_timings = t;
        Ok(())
    }

    /// The colony counted now, for the ledger's long record.
    pub fn census(&self) -> ledger::Census {
        let cell_area = self.water.cell_m * self.water.cell_m;
        ledger::Census {
            beavers: self.colony.beavers.len() as u16,
            young: self.colony.beavers.iter().filter(|b| b.is_young()).count() as u16,
            water: self.buildings.buildings.iter().filter_map(|b| b.dam.as_deref()?.pond.as_ref()).map(|p| p.volume * cell_area).sum(),
            trees: self.resources.items().iter().filter(|r| r.kind == resources::ResourceKind::Tree && !r.snag).count() as u32,
            planted: self.ledger.planted,
        }
    }

    fn explore(&mut self) {
        let world = &self.world;
        let allowed = fog::open_at(world);
        for a in self.agents.by_id.values().filter(|a| !a.inside) {
            self.fog.sight(a.id, a.cell, &allowed);
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
            predators: self.predators.clone(),
            locks: self.locks.clone(),
            fleet: self.fleet.clone(),
            wildlife: self.wildlife.clone(),
            lives: self.lives.clone(),
            run: self.run.clone(),
            names: self.names.clone(),
            deeds: self.deeds.clone(),
            supplies: self.supplies.clone(),
            sandbox: self.sandbox.clone(),
            step_timings: self.step_timings.clone(),
            journal: self.journal.clone(),
        };
        copy.work.inventory = copy.buildings.stores.inventory.view();
        copy
    }

    /// As `clone`, but copying into this state's buffers: the simulation's snapshots reuse the
    /// one the interface hands back, which is far cheaper than allocating a world each time.
    fn clone_from(&mut self, src: &Self) {
        let Self {
            seed, instance, client_commands, time_days, height, world, water, ecology, colony, buildings, events, nav,
            resources, agents, work, maintenance, society, ledger, output, fog, steps, recorder, forestry, predators,
            locks, fleet, wildlife, lives, run, names, deeds, supplies, sandbox, step_timings, journal,
        } = src;
        self.seed = *seed;
        self.instance = *instance;
        self.client_commands.clone_from(client_commands);
        self.time_days = *time_days;
        self.height.clone_from(height);
        self.world.clone_from(world);
        self.water.clone_from(water);
        self.ecology.clone_from(ecology);
        self.colony.clone_from(colony);
        self.buildings.clone_from(buildings);
        self.events.clone_from(events);
        self.nav.clone_from(nav);
        self.resources.clone_from(resources);
        self.agents.clone_from(agents);
        self.work.clone_from(work);
        self.maintenance.clone_from(maintenance);
        self.society.clone_from(society);
        self.ledger.clone_from(ledger);
        self.output.clone_from(output);
        self.fog.clone_from(fog);
        self.steps = *steps;
        self.recorder.clone_from(recorder);
        self.forestry.clone_from(forestry);
        self.predators.clone_from(predators);
        self.locks.clone_from(locks);
        self.fleet.clone_from(fleet);
        self.wildlife.clone_from(wildlife);
        self.lives.clone_from(lives);
        self.run.clone_from(run);
        self.names.clone_from(names);
        self.deeds.clone_from(deeds);
        self.supplies.clone_from(supplies);
        self.sandbox = *sandbox;
        self.step_timings = *step_timings;
        self.journal.clone_from(journal);
        self.work.inventory = self.buildings.stores.inventory.view();
    }
}
