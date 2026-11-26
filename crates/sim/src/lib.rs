mod command;
mod construction;

pub use command::{Command, DamGrowth};
pub use canals::CanalGrowth;
pub use gates::GateSetting;

pub mod agents;
pub mod buildings;
pub mod calendar;
pub mod content;
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
pub mod growth;
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
pub mod works;
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
    /// What each structure has cost to build and keep up.
    pub works: works::Works,
    pub society: society::Society,
    pub ledger: ledger::Ledger,
    pub fog: fog::Fog,
    pub steps: u64,
    #[serde(skip)]
    pub recorder: Option<Vec<(u64, Command)>>,
    pub forestry: forestry::Forestry,
    pub predators: predators::Predators,
    /// Each raft landing's rafts.
    pub fleet: raft_landing::Fleet,
    /// Rabbits, deer and birds.
    pub wildlife: wildlife::Wildlife,
    /// Every beaver's life story.
    pub lives: lives::Lives,
    /// The run's tallies.
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
            works: Default::default(),
            society: society::Society::default(),
            ledger: ledger::Ledger::default(),
            fog: fog::Fog::clear(size),
            steps: 0,
            recorder: None,
            forestry: forestry::Forestry::new(seed),
            predators: predators::Predators::new(seed),
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
            let slot_jobs = jobs::slot_jobs(self);
            for b in self.colony.beavers.iter_mut() {
                if let Some(a) = self.agents.get(b.id) {
                    b.pos = (a.cell.0 as f32 + 0.5, a.cell.1 as f32 + 0.5);
                    b.in_lodge = a.inside;
                }
            }
            let land = WorldLand { world: &self.world, depth: &self.water.depth };
            { let _p = prof::scope("step/colony/beavers"); self.colony.step_with_stores(dt_days, &land, &mut self.buildings, &self.water.depth, &mut self.work.piles, &slot_jobs).map_err(|_| SimError::InvalidDt(dt_days))?; }
            { let _p = prof::scope("step/colony/names"); names::step(self); }
            let _p = prof::scope("step/journal");
            journal::scan(self, "colony");
        }
        lap(&mut t.colony_s);
        {
            let _p = prof::scope("step/buildings");
            let terrain = buildings::Terrain { depth: &self.water.depth };
            { let _p = prof::scope("step/buildings/step"); self.buildings.step(terrain); }
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
        lap(&mut t.other_s);
        { let _p = prof::scope("step/agents"); agents::step(self, dt_days); }
        { let _p = prof::scope("step/predators"); predators::step(self, dt_days); }
        { let _p = prof::scope("step/wildlife"); wildlife::step(self, dt_days); }
        wildlife::note_tracks(self);
        { let _p = prof::scope("step/lives"); lives::step(self); }
        lap(&mut t.agents_s);
        { let _p = prof::scope("step/dams"); dams::step(self); }
        { let _p = prof::scope("step/canals"); canals::step(self, dt_days); }
        { let _p = prof::scope("step/raft_landing"); raft_landing::step(self); }
        lap(&mut t.shipping_s);
        { let _p = prof::scope("step/fog"); self.explore(); }
        lap(&mut t.fog_s);
        { let _p = prof::scope("step/society"); self.society.step(dt_days, self.time_days, &mut self.colony, &self.buildings); }
        {
            let _p = prof::scope("step/events");
            let wetland = self.ecology.wetland.as_slice();
            let mean = wetland.iter().step_by(7).sum::<f32>() / wetland.iter().step_by(7).count().max(1) as f32;
            self.events.step(dt_days, &mut self.colony, &mut self.buildings, mean);
        }
        { let _p = prof::scope("step/deeds"); deeds::step(self, dt_days); names::step(self); }
        {
            let _p = prof::scope("step/journal");
            journal::scan(self, "agents");
            journal::poll(self);
        }

        {
            let _p = prof::scope("step/ledger");
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

    /// What moves a beaver's needs, health and mood now, on this map (see `colony::Rates`).
    pub fn beaver_rates(&self, id: colony::BeaverId) -> Option<colony::Rates> {
        self.colony.rates(id, &WorldLand { world: &self.world, depth: &self.water.depth }, &self.buildings.buildings, &self.water.depth)
    }

    /// Whether a female beaver can conceive now (see `colony::Colony::conception`).
    pub fn beaver_conception(&self, id: colony::BeaverId) -> Option<Result<f32, &'static str>> {
        self.colony.conception(id, &self.buildings, &self.water.depth)
    }

    /// The colony counted now, for the ledger's long record.
    pub fn census(&self) -> ledger::Census {
        let cell_area = self.water.cell_m * self.water.cell_m;
        ledger::Census {
            beavers: self.colony.beavers.len() as u16,
            young: self.colony.beavers.iter().filter(|b| b.is_young()).count() as u16,
            water: self.buildings.buildings.iter().filter_map(|b| b.dam.as_deref()?.pond.as_ref()).map(|p| p.volume * cell_area).sum(),
            trees: self.resources.items().iter().filter(|r| r.kind.is_tree() && !r.snag).count() as u32,
            planted: self.ledger.planted.iter().sum(),
        }
    }

    fn explore(&mut self) {
        let sights: Vec<_> = self.agents.by_id.values().filter(|a| !a.inside)
            .map(|a| (a.id, a.cell, predators::beaver_sight(self, a.id))).collect();
        for (id, cell, r) in sights {
            self.fog.sight(id, cell, r);
        }
        let lookouts: Vec<_> = self.buildings.buildings.iter().filter(|b| predators::on_watch(self, b))
            .map(|b| (predators::lookout_centre(b), predators::lookout_sight(self, b))).collect();
        for (at, r) in lookouts {
            self.fog.reveal(at, r);
        }
    }

}

pub struct WorldLand<'a> {
    pub world: &'a worldgen::World,
    pub depth: &'a grid::Grid<f32>,
}

impl colony::Landscape for WorldLand<'_> {
    fn is_water(&self, x: f32, y: f32) -> bool {
        if !(x >= 0.0 && y >= 0.0) { return false; }
        let (cx, cy) = (x as usize, y as usize);
        self.world.river.get(cx, cy).copied().unwrap_or(false)
            || self.world.lake.get(cx, cy).copied().unwrap_or(false)
            || self.depth.get(cx, cy).is_some_and(|d| *d > 0.05)
    }
}

fn invalid(e: water::WaterError) -> SimError { SimError::InvalidCommand(e.to_string()) }

macro_rules! impl_sim_clone {
    (copy: [$($c:ident),*], clone: [$($f:ident),*]) => {
        impl Clone for SimState {
            fn clone(&self) -> Self {
                Self { $($c: self.$c,)* $($f: self.$f.clone(),)* }
            }
            /// Reuses snapshot buffers instead of reallocating the whole world each tick.
            fn clone_from(&mut self, src: &Self) {
                let Self { $($c,)* $($f,)* } = src;
                $(self.$c = *$c;)*
                $(self.$f.clone_from($f);)*
            }
        }
    };
}

impl_sim_clone!(
    copy: [seed, instance, time_days, steps, sandbox, step_timings],
    clone: [
        client_commands, height, world, water, ecology, colony, buildings, events, nav,
        resources, agents, work, maintenance, works, society, ledger, fog,
        recorder, forestry, predators, fleet, wildlife, lives, run, names,
        deeds, supplies, journal
    ]
);
