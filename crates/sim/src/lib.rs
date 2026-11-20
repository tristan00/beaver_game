pub mod agents;
pub mod autoplay;
pub mod buildings;
pub mod calendar;
pub mod catalogue;
pub mod colony;
pub mod craftsmanship;
pub mod dam_preview;
pub mod ecology;
pub mod fog;
pub mod events;
pub mod features;
pub mod forestry;
pub mod grid;
pub mod hud;
pub mod jobs;
pub mod ledger;
pub mod maps;
pub mod maintenance;
pub mod nav;
pub mod output;
pub mod region;
pub mod resources;
pub mod sandbox;
pub mod script;
pub mod site;
pub mod society;
pub mod shipping;
pub mod starts;
pub mod stock;

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

#[derive(Clone, Debug, PartialEq)]
pub enum Command {
    Regenerate { seed: u64 },
    BuildDam { cells: Vec<water::Cell>, height: f32 },
    OrderDam { cells: Vec<water::Cell>, height: f32 },
    OrderDamTo { cells: Vec<water::Cell>, crest: f32 },
    RepairDam { cell: water::Cell },
    RepairBuilding { building: buildings::BuildingId },
    ReinforceBuilding { building: buildings::BuildingId },
    Settle { cell: water::Cell },
    BreachDam { x: usize, y: usize },
    DigCanal { cells: Vec<water::Cell>, depth: f32 },
    Assign { who: Vec<colony::BeaverId>, to: Option<agents::Assignment> },
    PlaceSite { kind: buildings::BuildingKind, pos: (usize, usize) },
    FillSlot { building: buildings::BuildingId, slot: usize, who: colony::BeaverId },
    EmptySlot { building: buildings::BuildingId, slot: usize },
    AddWorker { building: buildings::BuildingId },
    RemoveWorker { building: buildings::BuildingId },
    SetPriority { building: buildings::BuildingId, priority: work::Priority },
    SetPaused { building: buildings::BuildingId, paused: bool },
    AdjustRole { role: jobs::RoleKind, delta: i32 },
    SetRoleOrder { order: Vec<jobs::RoleKind> },
    SetRoleWeight { role: jobs::RoleKind, weight: f32 },
    SetAutoStaff { on: bool },
    Unlock { who: colony::BeaverId },
    SetWorkArea { building: buildings::BuildingId, area: work::Area },
    SetHaul { order: work::HaulOrder },
    SetBalance { building: buildings::BuildingId, on: bool },
    SetBargeRoute { barge: buildings::BargeId, from: buildings::BuildingId, to: buildings::BuildingId, goods: Vec<buildings::Good> },
    ClearBargeRoute { barge: buildings::BargeId },
    AnswerJoin { id: society::RequestId, answer: society::Answer },
    OrderCanal { cells: Vec<water::Cell> },
    CancelSite { building: buildings::BuildingId },
    SetCutRule { building: buildings::BuildingId, rule: forestry::CutRule },
    SetPlantRule { building: buildings::BuildingId, rule: forestry::PlantRule },
}

/// Seconds each part of `SimState::step` has taken since the counters were last cleared.
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
    pub time_days: f64,
    pub height: grid::HeightGrid,
    pub world: worldgen::World,
    pub water: water::Water,
    pub ecology: ecology::Ecology,
    pub colony: colony::Colony,
    pub buildings: buildings::Buildings,
    pub region: region::Region,
    pub events: events::Events,
    pub features: features::Features,
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
            time_days: 0.0,
            height,
            world,
            water,
            ecology: ecology::Ecology::seeded(size, seed),
            colony: Default::default(),
            buildings: Default::default(),
            region: region::Region::new(seed),
            events: events::Events::new(seed),
            features: features::Features::default(),
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
        forestry::init(&mut sim);
        sim.colony.abstract_gathering = false;
        sim.apply_features();
        sim
    }

    fn mirror_colony_stores(&mut self) -> Option<Mirror> {
        if self.colony.abstract_gathering {
            return None;
        }
        let s = &self.buildings.stores;
        let (food, wood) = (s.amount(buildings::Good::Food), s.amount(buildings::Good::Sticks));
        (self.colony.food, self.colony.wood) = (food, wood);
        let c = &self.colony;
        Some(Mirror { food, wood, eaten: c.eaten, spoiled: c.spoiled, repair_wood: c.repair_wood })
    }

    fn settle_colony_stores(&mut self, before: Option<Mirror>) {
        use buildings::{Good, Use};
        let Some(m) = before else { return };
        let c = &self.colony;
        let stages = [colony::Stage::Kit, colony::Stage::Yearling, colony::Stage::Adult];
        let mut food_uses: Vec<(Use, f32)> = stages.iter().map(|&st| (Use::eaten_by(st), c.eaten[st as usize] - m.eaten[st as usize])).collect();
        food_uses.push((Use::Spoiled, c.spoiled - m.spoiled));
        let wood_uses = vec![(Use::Repairs, c.repair_wood - m.repair_wood)];
        let stores = &mut self.buildings.stores;
        for (good, was, now, uses) in [(Good::Food, m.food, c.food, food_uses), (Good::Sticks, m.wood, c.wood, wood_uses)] {
            let d = now - was;
            if d < 0.0 {
                let take = (-d).min(stores.amount(good));
                let known: f32 = uses.iter().map(|u| u.1.max(0.0)).sum();
                let scale = if known > take { take / known } else { 1.0 };
                let mut left = take;
                for (u, a) in uses {
                    let a = (a.max(0.0) * scale).min(left);
                    if a > 0.0 {
                        stores.withdraw_for(good, a, u).expect("clamped to stock");
                        left -= a;
                    }
                }
                if left > 0.0 {
                    stores.withdraw_for(good, left.min(stores.amount(good)), Use::Other).expect("clamped to stock");
                }
            } else if d > 0.0 {
                stores.deposit(good, d).expect("finite positive amount");
            }
        }
        self.colony.food = stores.amount(Good::Food);
        self.colony.wood = stores.amount(Good::Sticks);
    }

    pub fn set_features(&mut self, features: features::Features) {
        self.features = features;
        self.apply_features();
    }

    fn apply_features(&mut self) {
        use features::System;
        let f = self.features;
        self.events.set_muted(events::EventKind::ALL.into_iter().filter(|k| !f.allows_event(*k)).collect());
        self.colony.set_disabled_jobs(colony::Job::ALL.into_iter().filter(|j| !f.allows_job(*j)).collect());
        if !f.enabled(System::Predators) && !self.ecology.predators.is_empty() {
            self.ecology.predators.clear();
        }
    }

    /// Gameplay, sandbox and scripted playtests share the same physical clock.
    pub fn step_game(&mut self, dt_days: f32) -> Result<(), SimError> {
        self.step(dt_days)
    }

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
        self.apply_features();
        self.time_days += dt_days as f64;
        self.steps += 1;
        lap(&mut t.other_s);
        self.water.step(&self.height, dt_days);
        lap(&mut t.water_s);
        self.ecology.step(dt_days, &self.water.depth)?;
        lap(&mut t.ecology_s);
        let mirrored = self.mirror_colony_stores();
        self.derive_colony_jobs();
        let land = WorldLand { world: &self.world, depth: &self.water.depth };
        self.colony.step_on(dt_days, &land).map_err(|_| SimError::InvalidDt(dt_days))?;
        self.settle_colony_stores(mirrored);
        lap(&mut t.colony_s);
        let terrain = buildings::Terrain { height: &self.height, depth: &self.water.depth };
        self.buildings.step(dt_days, terrain, &self.colony, &self.work);
        self.sync_homes();
        maintenance::step(self, dt_days);
        lap(&mut t.buildings_s);
        if self.features.enabled(features::System::Rivals) {
            self.region.step(dt_days);
        }
        self.resources.step(dt_days, self.time_days).expect("dt validated above");
        let day = self.time_days.floor();
        if day > (self.time_days - dt_days as f64).floor() && self.colony.is_founded() {
            wetland::step(self, day as u64);
        }
        forestry::step(self, dt_days);
        stock::reconcile(self);
        lap(&mut t.other_s);
        agents::step(self, dt_days);
        lap(&mut t.agents_s);
        shipping::step(self, dt_days);
        lap(&mut t.shipping_s);
        self.explore();
        lap(&mut t.fog_s);
        self.society.step(dt_days, self.time_days, &self.colony);
        self.step_events(dt_days);

        self.output.step(dt_days);
        stock::reconcile(self);
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

    fn sync_homes(&mut self) {
        use buildings::BuildingKind::{Burrow, Lodge};
        if self.colony.homes_from_buildings {
            let new: Vec<(buildings::BuildingId, (f32, f32))> = self
                .buildings
                .buildings
                .iter()
                .filter(|b| matches!(b.kind, Burrow | Lodge) && b.is_complete() && !self.work.homes.contains_key(&b.id))
                .map(|b| {
                    let (w, d) = b.kind.spec().footprint;
                    (b.id, (b.pos.0 as f32 + w as f32 / 2.0, b.pos.1 as f32 + d as f32 / 2.0))
                })
                .collect();
            for (id, pos) in new {
                let home = self.colony.add_home(pos);
                self.work.homes.insert(id, home);
                if self.buildings.building(id).is_some_and(|b| b.kind == Lodge)
                    && !self.colony.beavers.iter().any(|b| b.lodge == Some(home)) {
                    let previous = self.buildings.buildings.iter().filter(|b| b.kind == Burrow)
                        .filter_map(|b| self.work.homes.get(&b.id).copied())
                        .find(|old| self.colony.beavers.iter().any(|b| b.lodge == Some(*old)));
                    if let Some(old) = previous {
                        let family = self.colony.lodges.iter().find(|l| l.id == old).expect("burrow household").family;
                        self.colony.lodges.iter_mut().find(|l| l.id == home).unwrap().family = family;
                        for b in self.colony.beavers.iter_mut().filter(|b| b.lodge == Some(old)) { b.lodge = Some(home); b.pos = pos; }
                        // The old burrow remains a refuge, not a second family house.
                        self.colony.lodges.retain(|l| l.id != old);
                    }
                }
            }
            self.colony.house_homeless();
        }
        if self.work.supplies.is_some() && self.buildings.buildings.iter().any(|b| b.is_complete() && agents::accepts(b.kind, buildings::Good::Sticks)) {
            self.work.supplies = None;
        }
    }

    fn derive_colony_jobs(&mut self) {
        if self.colony.abstract_gathering {
            self.colony.slot_jobs = None;
            return;
        }
        let jobs = jobs::slot_jobs(self);
        for b in self.colony.beavers.iter_mut() {
            b.job = jobs.get(&b.id).map(|j| j.0);
            if let Some(a) = self.agents.get(b.id) {
                b.pos = (a.cell.0 as f32 + 0.5, a.cell.1 as f32 + 0.5);
            }
        }
        self.colony.slot_jobs = Some(jobs);
    }





    fn step_events(&mut self, dt_days: f32) {
        if !self.colony.is_founded() {
            self.events.step(dt_days);
            return;
        }
        let ctx = self.event_context();
        let mirrored = self.mirror_colony_stores();
        let mut host = ColonyHost { colony: &mut self.colony };
        if self.events.step_with(dt_days, &ctx, &mut host).is_err() {
            self.events.step(dt_days);
        }
        self.settle_colony_stores(mirrored);
        let arrivals = if self.features.enabled(features::System::Rivals) { self.region.take_arrivals() } else { Vec::new() };
        for m in arrivals {
            self.colony.add_immigrants(m.count);
        }
    }

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

    pub fn check_site(&self, kind: buildings::BuildingKind, pos: (usize, usize)) -> Result<(), SimError> {
        if !self.can_build(kind) {
            return Err(SimError::InvalidCommand(format!("requires finished buildings: {:?}", self.missing_prerequisites(kind))));
        }
        self.check_off_pile(kind, pos)?;
        self.check_explored(kind, pos)?;
        let terrain = buildings::Terrain { height: &self.height, depth: &self.water.depth };
        self.buildings.can_place_site(kind, pos, terrain).map_err(|e| SimError::InvalidCommand(e.to_string()))
    }

    fn check_explored(&self, kind: buildings::BuildingKind, pos: (usize, usize)) -> Result<(), SimError> {
        let (w, d) = kind.spec().footprint;
        for y in pos.1..pos.1 + d {
            for x in pos.0..pos.0 + w {
                if x < self.fog.size && y < self.fog.size && !self.fog.explored((x, y)) {
                    return Err(SimError::InvalidCommand("Unexplored: the colony has not seen that ground".into()));
                }
            }
        }
        Ok(())
    }

    fn check_off_pile(&self, kind: buildings::BuildingKind, pos: (usize, usize)) -> Result<(), SimError> {
        let (w, d) = kind.spec().footprint;
        if let Some(p) = self.work.supplies {
            if (pos.0..pos.0 + w).contains(&p.0) && (pos.1..pos.1 + d).contains(&p.1) {
                return Err(SimError::InvalidCommand("Blocked: the supply pile is there".into()));
            }
        }
        Ok(())
    }

    pub fn place_site(&mut self, kind: buildings::BuildingKind, pos: (usize, usize)) -> Result<buildings::BuildingId, SimError> {
        if let Some(need) = self.missing_prerequisites(kind).first().filter(|_| !self.sandbox) {
            return Err(SimError::InvalidCommand(format!("needs a finished {} first", need.spec().name.to_lowercase())));
        }
        if !self.can_build(kind) {
            return Err(SimError::InvalidCommand(format!("requires finished buildings: {:?}", self.missing_prerequisites(kind))));
        }
        self.check_off_pile(kind, pos)?;
        self.check_explored(kind, pos)?;
        if kind == buildings::BuildingKind::Lock && !buildings::Buildings::footprint(kind, pos).iter().any(|c| self.water.has_dam(c.0, c.1)) {
            return Err(SimError::InvalidCommand("a lock is built on a dam, to let barges through it".into()));
        }
        let terrain = buildings::Terrain { height: &self.height, depth: &self.water.depth };
        let id = self
            .buildings
            .place_site(kind, pos, terrain)
            .map_err(|e| SimError::InvalidCommand(e.to_string()))?;
        let spec = kind.spec();
        let dam_cells = (kind == buildings::BuildingKind::Dam).then(|| dam_preview::spot_cells(pos));
        let (cost, labour) = match &dam_cells {
            Some(c) => (dam_preview::dam_cost(c.len()), spec.work * dam_preview::length_factor(c.len())),
            None => (spec.cost.to_vec(), spec.work),
        };
        if let Err(e) = self.work.add_site(id, kind, &cost, labour) {
            self.buildings.buildings.retain(|b| b.id != id);
            return Err(SimError::InvalidCommand(format!("{e:?}")));
        }
        if let Some(cells) = dam_cells {
            let heights = vec![dam_preview::DEFAULT_HEIGHT; cells.len()];
            let from = cells.iter().map(|c| self.water.dam_height.get(c.0, c.1).copied().unwrap_or(0.0)).collect();
            self.work.dam_plans.push(work::DamPlan { building: id, repair: false, cells, height: dam_preview::DEFAULT_HEIGHT, heights, from });
        }
        Ok(id)
    }

    pub fn missing_prerequisites(&self, kind: buildings::BuildingKind) -> Vec<buildings::BuildingKind> {
        catalogue::prerequisites(kind)
            .iter()
            .copied()
            .filter(|&p| !self.buildings.buildings.iter().any(|b| b.kind == p && b.is_complete()))
            .collect()
    }

    pub fn has_building(&self, kind: buildings::BuildingKind) -> bool {
        self.buildings.buildings.iter().any(|b| b.kind == kind && b.is_complete())
    }

    pub fn can_build(&self, kind: buildings::BuildingKind) -> bool {
        self.sandbox || (self.features.allows_building(kind) && self.missing_prerequisites(kind).is_empty())
    }

    pub fn build_gaps(&self, kind: buildings::BuildingKind) -> Vec<buildings::BuildingKind> {
        self.missing_prerequisites(kind)
    }

    pub fn dam_site(&self, cells: &[water::Cell]) -> Result<water::Cell, buildings::BuildError> {
        let terrain = buildings::Terrain { height: &self.height, depth: &self.water.depth };
        let mid = cells.len() / 2;
        let mut order: Vec<usize> = (0..cells.len()).collect();
        order.sort_by_key(|&i| (i.abs_diff(mid), i));
        let mut first = None;
        for i in order {
            match self.buildings.can_place_site(buildings::BuildingKind::Dam, cells[i], terrain) {
                Ok(()) => return Ok(cells[i]),
                Err(e) => {
                    first.get_or_insert(e);
                }
            }
        }
        Err(first.unwrap_or(buildings::BuildError::OutOfBounds))
    }

    pub const MAX_CANAL_CELLS: usize = 150;
    pub const CANAL_DEPTH: f32 = 0.5;

    pub fn order_canal(&mut self, cells: &[water::Cell]) -> Result<buildings::BuildingId, SimError> {
        use buildings::BuildingKind::Canal;
        let invalid = |m: &str| SimError::InvalidCommand(m.to_string());
        if cells.is_empty() || cells.len() > Self::MAX_CANAL_CELLS {
            return Err(invalid("not a valid canal line (empty or too long)"));
        }
        if cells.iter().any(|&(x, y)| !self.water.depth.in_bounds(x, y)) {
            return Err(invalid("out of bounds"));
        }
        if cells.windows(2).any(|w| w[0].0.abs_diff(w[1].0).max(w[0].1.abs_diff(w[1].1)) != 1) {
            return Err(invalid("not a valid canal line (cells must touch)"));
        }
        if let Some(c) = cells.iter().find(|c| !self.fog.explored(**c)) {
            return Err(invalid(&format!("Unexplored at {c:?}")));
        }
        if !self.can_build(Canal) {
            return Err(invalid("Locked: Canal digging"));
        }
        let terrain = buildings::Terrain { height: &self.height, depth: &self.water.depth };
        let pos = cells
            .iter()
            .copied()
            .find(|&c| self.buildings.can_place_site(Canal, c, terrain).is_ok())
            .ok_or_else(|| invalid("needs water: a canal starts from water"))?;
        let id = self.buildings.place_site(Canal, pos, terrain).map_err(|e| SimError::InvalidCommand(e.to_string()))?;
        let n = cells.len() as f32;
        let cost: Vec<buildings::Amount> = Canal.spec().cost.iter().map(|&(g, a)| (g, a * n)).collect();
        let labour = Canal.spec().work * n;
        if let Err(e) = self.work.add_site(id, Canal, &cost, labour) {
            self.buildings.buildings.retain(|b| b.id != id);
            return Err(SimError::InvalidCommand(format!("{e:?}")));
        }
        self.work.canal_plans.push(work::CanalPlan { building: id, cells: cells.to_vec(), depth: Self::CANAL_DEPTH });
        Ok(id)
    }

    pub fn order_dam(&mut self, cells: &[water::Cell], height: f32) -> Result<buildings::BuildingId, SimError> {
        self.order_dam_heights(cells, &vec![height; cells.len()])
    }

    pub fn order_dam_to(&mut self, cells: &[water::Cell], crest: f32) -> Result<buildings::BuildingId, SimError> {
        let (cells, heights) = dam_preview::heights_to(self, cells, crest);
        if cells.is_empty() {
            return Err(SimError::InvalidCommand("the ground is already that high".into()));
        }
        self.order_dam_heights(&cells, &heights)
    }

    pub fn repair_dam(&mut self, cell: water::Cell) -> Result<buildings::BuildingId, SimError> {
        let recorded=self.buildings.buildings.iter().find(|b| b.kind==buildings::BuildingKind::Dam && b.dam_profile.iter().any(|v|v.0==cell)).cloned();
        let mut cells = recorded.as_ref().map(|b|b.dam_profile.iter().map(|v|v.0).collect::<Vec<_>>()).unwrap_or_else(||self.water.dam_line_at(cell));
        cells.sort();
        if cells.is_empty() {
            return Err(SimError::InvalidCommand("there is no dam here to repair".into()));
        }
        // Repeated clicks must not create competing work orders for the same dam.
        if let Some(plan) = self.work.dam_plans.iter().find(|p| p.cells.iter().any(|c| cells.binary_search(c).is_ok())) {
            return Ok(plan.building);
        }
        let heights: Vec<f32> = cells.iter().map(|&(x,y)| recorded.as_ref().and_then(|b|b.dam_profile.iter().find(|v|v.0==(x,y)).map(|v|v.1)).unwrap_or_else(||*self.water.dam_height.get(x,y).expect("dam cell"))).collect();
        let (mut cost, labour) = dam_preview::cost_for(self, &cells, &heights);
        if labour <= 1e-3 {
            return Err(SimError::InvalidCommand("the dam does not need repairs".into()));
        }
        let existing = recorded.as_ref().map(|b|b.id).or_else(||self.buildings.buildings.iter().find(|b| b.kind == buildings::BuildingKind::Dam
            && b.is_complete() && b.cells().any(|c| cells.binary_search(&c).is_ok())).map(|b| b.id));
        let Some(id) = existing else { return self.order_dam_heights(&cells, &heights) };
        if self.maintenance.jobs.contains_key(&id) {return Err(SimError::InvalidCommand("this dam already has maintenance queued".into()));}
        if self.buildings.building(id).is_some_and(|b|b.reinforced) {
            let damage=cells.iter().map(|&(x,y)|1.0-self.water.dam_integrity.get(x,y).copied().unwrap_or(0.0)).sum::<f32>();
            cost.push((buildings::Good::Clay,(damage*1.2).ceil().max(1.0)));
        }
        // A dam's crown can be dry. Reuse its original work site instead of
        // requiring the water/placement checks for a brand-new dam.
        let old = self.work.places.iter().position(|p| p.building == id).map(|i| self.work.places.remove(i));
        if let Err(e) = self.work.add_site(id, buildings::BuildingKind::Dam, &cost, labour) {
            if let Some(p) = old { self.work.places.push(p); }
            return Err(SimError::InvalidCommand(format!("{e:?}")));
        }
        self.buildings.buildings.iter_mut().find(|b| b.id == id).expect("existing dam").status = buildings::Status::Site;
        let height = heights.iter().copied().fold(0.0, f32::max);
        self.work.dam_plans.push(work::DamPlan { building: id, repair: true, cells, height, from: heights.clone(), heights });
        Ok(id)
    }

    pub fn order_dam_heights(&mut self, cells: &[water::Cell], heights: &[f32]) -> Result<buildings::BuildingId, SimError> {
        if dam_preview::preview_heights(self, cells, heights).is_none() {
            return Err(SimError::InvalidCommand("not a valid dam line (cells empty, repeated or off the map, or height not positive)".into()));
        }
        if let Some(h) = heights.iter().copied().find(|&h| h > dam_preview::MAX_DAM_HEIGHT + 1e-3) {
            return Err(SimError::InvalidCommand(format!("{h:.1} is too high: a dam stands at most {} above the ground", dam_preview::MAX_DAM_HEIGHT)));
        }
        let (cost, labour) = dam_preview::cost_for(self, cells, heights);
        if labour <= 1e-3 {
            return Err(SimError::InvalidCommand("the dam is already that high".into()));
        }
        let pos = self.dam_site(cells).map_err(|e| SimError::InvalidCommand(e.to_string()))?;
        let terrain = buildings::Terrain { height: &self.height, depth: &self.water.depth };
        let id = self
            .buildings
            .place_site(buildings::BuildingKind::Dam, pos, terrain)
            .map_err(|e| SimError::InvalidCommand(e.to_string()))?;
        let labour = labour;
        if let Err(e) = self.work.add_site(id, buildings::BuildingKind::Dam, &cost, labour) {
            self.buildings.buildings.retain(|b| b.id != id);
            return Err(SimError::InvalidCommand(format!("{e:?}")));
        }
        let height = heights.iter().copied().fold(0.0, f32::max);
        let from = cells.iter().map(|c| self.water.dam_height.get(c.0, c.1).copied().unwrap_or(0.0)).collect();
        self.work.dam_plans.push(work::DamPlan { building: id, repair: false, cells: cells.to_vec(), height, heights: heights.to_vec(), from });
        Ok(id)
    }

    pub fn cancel_site(&mut self, id: buildings::BuildingId) -> Result<Vec<buildings::Amount>, SimError> {
        if self.maintenance.jobs.contains_key(&id) { return maintenance::cancel(self, id); }
        let b = self.buildings.building(id).ok_or_else(|| SimError::InvalidCommand(format!("no building {}", id.0)))?;
        if b.is_complete() {
            return Err(SimError::InvalidCommand(format!("{} is finished; only unfinished sites can be cancelled", b.kind.spec().name)));
        }
        if matches!(b.status, buildings::Status::UnderConstruction(_)) {
            let before: Vec<_> = b.kind.spec().cost.to_vec();
            self.buildings.cancel(id, &mut self.colony).map_err(|e| SimError::InvalidCommand(e.to_string()))?;
            self.output.forget(id);
            return Ok(before);
        }
        let mut refunded = Vec::new();
        let is_dam = self.work.dam_plans.iter().any(|d| d.building == id);
        let is_repair = self.work.dam_plans.iter().any(|d| d.building == id && d.repair);
        if let Some(i) = self.work.places.iter().position(|p| p.building == id) {
            let place = self.work.places.remove(i);
            let site = place.site.as_ref();
            let built = if is_dam && !is_repair { site.map_or(0.0, |s| s.labour_fraction()) } else { 0.0 };
            for &(g, a) in site.map(|s| s.delivered.as_slice()).unwrap_or(&[]) {
                let used: f32 = site.map_or(0.0, |s| s.needed.iter().filter(|n| n.0 == g).map(|n| n.1).sum::<f32>()) * built;
                let a = (a - used).max(0.0);
                if a > 0.0 {
                    let put = self.buildings.stores.refund(g, a).map_err(|e| SimError::InvalidCommand(e.to_string()))?;
                    refunded.push((g, put));
                }
            }
        }
        self.work.dam_plans.retain(|d| d.building != id);
        self.work.canal_plans.retain(|d| d.building != id);
        self.work.managed.retain(|_, duty| !matches!(duty, work::ManagedDuty::Slot(b, _) if *b == id));
        for ag in self.agents.by_id.values_mut() {
            if matches!(ag.assignment, Some(agents::Assignment::Slot(b, _)) if b == id) {
                ag.assignment = None;
            }
        }
        if is_repair {
            let b = self.buildings.buildings.iter_mut().find(|b| b.id == id).expect("repair site");
            b.status = buildings::Status::Complete;
            self.work.add_complete(b).map_err(|e| SimError::InvalidCommand(format!("{e:?}")))?;
        } else {
            self.buildings.buildings.retain(|b| b.id != id);
        }
        self.output.forget(id);
        Ok(refunded)
    }

    pub fn apply(&mut self, cmd: Command) -> Result<(), SimError> {
        let logged = self.recorder.is_some().then(|| cmd.clone());
        let res = self.apply_inner(cmd);
        if let (Ok(()), Some(c)) = (&res, logged) {
            let step = self.steps;
            if let Some(r) = self.recorder.as_mut() {
                r.push((step, c));
            }
        }
        res
    }

    fn apply_inner(&mut self, cmd: Command) -> Result<(), SimError> {
        if self.sandbox {
            return sandbox::apply(self, cmd);
        }
        match cmd {
            Command::Regenerate { seed } => {
                *self = SimState::new(seed);
                Ok(())
            }
            Command::BuildDam { cells, height } => self.water.build_dam(&cells, height).map_err(invalid),
            Command::OrderDam { cells, height } => self.order_dam(&cells, height).map(|_| ()),
            Command::OrderDamTo { cells, crest } => self.order_dam_to(&cells, crest).map(|_| ()),
            Command::RepairDam { cell } => self.repair_dam(cell).map(|_| ()),
            Command::RepairBuilding { building } => maintenance::order(self, building, maintenance::Refit::Repair),
            Command::ReinforceBuilding { building } => maintenance::order(self, building, maintenance::Refit::Reinforce),
            Command::Settle { cell } => starts::settle(self, cell),
            Command::BreachDam { x, y } => self.water.breach(x, y).map_err(invalid),
            Command::DigCanal { cells, depth } => {
                self.water.dig_canal(&mut self.height, &cells, depth).map_err(invalid)
            }
            Command::Assign { who, to } => {
                if let Some(kit) = who.iter().find(|id| self.colony.beaver(**id).is_some_and(|b| b.stage == colony::Stage::Kit)) {
                    return Err(SimError::InvalidCommand(format!("beaver {} is a kit and cannot work", kit.0)));
                }
                if let Some(id) = who.iter().find(|id| !self.agents.by_id.contains_key(id)) {
                    return Err(SimError::InvalidCommand(format!("no agent {}", id.0)));
                }
                for id in who {
                    jobs::order(self, id, to);
                    self.agents.assign(id, to);
                }
                Ok(())
            }
            Command::PlaceSite { kind, pos } => self.place_site(kind, pos).map(|_| ()),
            Command::FillSlot { building, slot, who } => {
                let stage = self.colony.beaver(who).map(|b| b.stage).ok_or(work::WorkError::NoSuchBeaver);
                stage
                    .and_then(|st| if st == colony::Stage::Kit { Err(work::WorkError::NotAdult) } else { Ok(()) })
                    .and_then(|()| self.work.place(building).ok_or(work::WorkError::NoSuchWorkplace))
                    .and_then(|p| match p.slots.get(slot) {
                        None => Err(work::WorkError::SlotOutOfRange),
                        Some(s) if s.worker.is_some() => Err(work::WorkError::SlotTaken),
                        Some(_) => Ok(()),
                    })
                    .map_err(|e| SimError::InvalidCommand(format!("{e:?}")))?;
                jobs::place_by_hand(self, building, slot, who)
            }
            Command::EmptySlot { building, slot } => {
                let who = self.work.place(building).and_then(|p| p.slots.get(slot)).and_then(|s| s.worker);
                self.work.unassign(building, slot).map_err(|e| SimError::InvalidCommand(format!("{e:?}")))?;
                if let Some(w) = who {
                    self.work.managed.remove(&w);
                }
                if let Ok(p) = self.work.place_mut(building) {
                    p.max_workers = p.max_workers.min(p.filled());
                }
                Ok(())
            }
            Command::AddWorker { building } => jobs::add_worker(self, building),
            Command::RemoveWorker { building } => jobs::remove_worker(self, building),
            Command::SetPriority { building, priority } => jobs::set_priority(self, building, priority),
            Command::SetPaused { building, paused } => jobs::set_paused(self, building, paused),
            Command::AdjustRole { role, delta } => jobs::adjust_role(self, role, delta),
            Command::SetRoleWeight { role, weight } => {
                if !weight.is_finite() || !(0.1..=20.0).contains(&weight) {
                    return Err(SimError::InvalidCommand("role weight must be 0.1..20".into()));
                }
                self.work.role_weights[role as usize] = weight;
                Ok(())
            }
            Command::SetRoleOrder { order } => jobs::set_role_order(self, order),
            Command::SetAutoStaff { on } => {
                self.work.auto_organise = on;
                Ok(())
            }
            Command::Unlock { who } => jobs::unlock(self, who),
            Command::SetWorkArea { building, area } => {
                forestry::check_area(self, building, &area)?;
                self.work.set_area(building, area).map_err(|e| SimError::InvalidCommand(format!("{e:?}")))
            }
            Command::OrderCanal { cells } => self.order_canal(&cells).map(|_| ()),
            Command::CancelSite { building } => self.cancel_site(building).map(|_| ()),
            Command::SetCutRule { building, rule } => forestry::set_cut_rule(self, building, rule),
            Command::SetPlantRule { building, rule } => forestry::set_plant_rule(self, building, rule),
            Command::SetBargeRoute { barge, from, to, goods } => shipping::set_route(self, barge, from, to, goods).map_err(SimError::InvalidCommand),
            Command::ClearBargeRoute { barge } => shipping::clear_route(self, barge).map_err(SimError::InvalidCommand),
            Command::SetBalance { building, on } => {
                let store = self.buildings.building(building).is_some_and(|b| b.is_complete() && agents::is_store(b.kind));
                if !store {
                    return Err(SimError::InvalidCommand(format!("building {} is not a finished store", building.0)));
                }
                if on {
                    self.work.balanced.insert(building);
                } else {
                    self.work.balanced.remove(&building);
                }
                Ok(())
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
                self.society.answer(id, answer, &mut self.colony).map_err(|e| SimError::InvalidCommand(format!("{e:?}")))
            }
        }
    }
}

struct Mirror {
    food: f32,
    wood: f32,
    eaten: [f32; 4],
    spoiled: f32,
    repair_wood: f32,
}

pub struct WorldLand<'a> {
    pub world: &'a worldgen::World,
    pub depth: &'a grid::Grid<f32>,
}

fn water_wet() -> f32 {
    buildings::WET_DEPTH
}

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
mod store_mirror_tests {
    use super::*;
    use buildings::Good;
    #[test]
    fn meals_and_repairs_charge_separate_resources() {
        let mut sim=SimState::new(1);
        for (good,amount) in [(Good::Food,10.0),(Good::Sticks,20.0),(Good::Clay,30.0)] {
            sim.buildings.stores.deposit(good,amount).unwrap();
        }
        let before=sim.mirror_colony_stores();
        sim.colony.food-=2.0;
        sim.colony.eaten[colony::Stage::Adult as usize]+=2.0;
        sim.colony.wood-=3.0;
        sim.colony.repair_wood+=3.0;
        sim.settle_colony_stores(before);
        assert_eq!(sim.buildings.stores.amount(Good::Food),8.0);
        assert_eq!(sim.buildings.stores.amount(Good::Sticks),17.0);
        assert_eq!(sim.buildings.stores.amount(Good::Clay),30.0);
        assert_eq!((sim.colony.food,sim.colony.wood),(8.0,17.0));
    }
}
