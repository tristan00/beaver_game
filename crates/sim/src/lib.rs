//! Pure, deterministic simulation core (no rendering dependencies).
//! `SimState` composes one struct per system; the game crate only reads it,
//! advances it with `step`, and mutates it through `apply`.

pub mod agents;
pub mod buildings;
pub mod calendar;
pub mod catalogue;
pub mod colony;
pub mod dam_preview;
pub mod ecology;
pub mod events;
pub mod features;
pub mod grid;
pub mod hud;
pub mod ledger;
/// The old lake starts, kept only as test fixtures (feature `fixtures`); no player can start in them.
#[cfg(any(test, feature = "fixtures"))]
pub mod lakes;
pub mod nav;
pub mod output;
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
    /// Order a dam: a construction site the family fells, hauls and builds; the water dam (and
    /// the pond behind it) appears when the site is finished.
    OrderDam { cells: Vec<water::Cell>, height: f32 },
    /// Found the colony at `cell` (the player's settling choice); rejected for an unusable cell.
    Settle { cell: water::Cell },
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
    /// Whole-day goods history for the interface.
    pub ledger: ledger::Ledger,
    /// What each building produced (smoothed per day).
    pub output: output::Output,
}

impl SimState {
    /// Builds a world deterministically from `seed`.
    pub fn new(seed: u64) -> Self {
        let world = worldgen::WorldGen::new(seed).generate().expect("default world generation failed");
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
            tech: Default::default(),
            region: region::Region::new(seed),
            events: events::Events::new(seed),
            features: features::Features::default(),
            nav: nav::NavGrid::new(size, size),
            resources,
            agents: agents::Agents::default(),
            work: work::Work::default(),
            society: society::Society::default(),
            ledger: ledger::Ledger::default(),
            output: output::Output::default(),
        };
        sim.colony.abstract_gathering = false;
        sim.apply_features();
        sim
    }

    /// One economy: unless the colony runs its abstract larder (old fixtures), its `food` and
    /// `wood` mirror the stores' Food and Logs. Returns the mirrored values so
    /// [`Self::settle_colony_stores`] can book what the colony ate or used against the stores.
    fn mirror_colony_stores(&mut self) -> Option<(f32, f32)> {
        if self.colony.abstract_gathering {
            return None;
        }
        let s = &self.buildings.stores;
        let before = (s.amount(buildings::Good::Food), s.amount(buildings::Good::Logs));
        (self.colony.food, self.colony.wood) = before;
        Some(before)
    }

    /// Books the change in the colony's mirrored food and wood against the stores: eating,
    /// spoilage and lodge building are consumption, event windfalls are production.
    fn settle_colony_stores(&mut self, before: Option<(f32, f32)>) {
        let Some((food0, wood0)) = before else { return };
        let stores = &mut self.buildings.stores;
        for (good, was, now) in [(buildings::Good::Food, food0, self.colony.food), (buildings::Good::Logs, wood0, self.colony.wood)] {
            let d = now - was;
            if d < 0.0 {
                let take = (-d).min(stores.amount(good));
                stores.withdraw(good, take).expect("clamped to stock");
            } else if d > 0.0 {
                stores.deposit(good, d).expect("finite positive amount");
            }
        }
        self.colony.food = stores.amount(buildings::Good::Food);
        self.colony.wood = stores.amount(buildings::Good::Logs);
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
        let mirrored = self.mirror_colony_stores();
        let land = WorldLand { world: &self.world, depth: &self.water.depth };
        self.colony.step_on(dt_days, &land).map_err(|_| SimError::InvalidDt(dt_days))?;
        self.settle_colony_stores(mirrored);
        self.step_research(dt_days);
        let terrain = buildings::Terrain { height: &self.height, depth: &self.water.depth };
        self.buildings.step(dt_days, &self.tech, terrain, &self.colony, &self.work);
        if self.features.enabled(features::System::Rivals) {
            self.region.step(dt_days);
        }
        self.resources.step(dt_days).expect("dt validated above");
        agents::step(self, dt_days);
        self.society.step(dt_days, self.time_days, &self.colony);
        self.step_events(dt_days);
        for (b, g, a) in std::mem::take(&mut self.buildings.crafted) {
            self.output.record(b, g, a);
        }
        self.output.step(dt_days);
        self.ledger.record(self.time_days, &self.buildings.stores);
        Ok(())
    }

    /// Records achievements, adds the lore elders studied in dens and unlocks what is now ready.
    fn step_research(&mut self, dt_days: f32) {
        use tech::Achievement::*;
        let built = |k| self.buildings.buildings.iter().any(|b| b.kind == k && b.is_complete());
        if built(buildings::BuildingKind::Burrow) {
            self.tech.record(FirstBurrow);
        }
        if built(buildings::BuildingKind::Dam) {
            self.tech.record(FirstDam);
        }
        if self.time_days >= tuning::tech::YEAR_DAYS && self.survived_year() {
            self.tech.record(FirstWinter);
        }
        let lore = tech::den_lore(&self.buildings, &self.work, &self.colony, dt_days);
        self.tech.study(lore);
        self.tech.update(&self.colony);
    }

    /// Nobody has died of hunger or cold and the larder holds at least one food per beaver.
    fn survived_year(&self) -> bool {
        let lost = self.colony.events.iter().any(|e| {
            matches!(e.event, colony::Event::Died { cause: colony::DeathCause::Starvation | colony::DeathCause::Cold, .. })
        });
        !lost && self.colony.food >= self.colony.population() as f32
    }

    /// Rolls random events against the colony, applies their effects and delivers migrants.
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

    /// Whether `place_site` would accept `kind` at `pos`; changes nothing.
    pub fn check_site(&self, kind: buildings::BuildingKind, pos: (usize, usize)) -> Result<(), SimError> {
        let terrain = buildings::Terrain { height: &self.height, depth: &self.water.depth };
        self.buildings.can_place_site(kind, pos, &self.tech, terrain).map_err(|e| SimError::InvalidCommand(e.to_string()))
    }

    /// Places a construction site for `kind` at `pos` and registers it as a workplace with its
    /// catalogue cost and (tech-adjusted) build work. A dam site is a real dam: it spans its
    /// catalogue footprint at `dam_preview::DEFAULT_HEIGHT`, costs `dam_preview::dam_cost` for those
    /// cells, and raises the water dam when finished.
    pub fn place_site(&mut self, kind: buildings::BuildingKind, pos: (usize, usize)) -> Result<buildings::BuildingId, SimError> {
        let terrain = buildings::Terrain { height: &self.height, depth: &self.water.depth };
        let id = self
            .buildings
            .place_site(kind, pos, &self.tech, terrain)
            .map_err(|e| SimError::InvalidCommand(e.to_string()))?;
        let spec = kind.spec();
        let dam_cells = (kind == buildings::BuildingKind::Dam).then(|| dam_preview::spot_cells(pos));
        let (cost, labour) = match &dam_cells {
            Some(c) => (dam_preview::dam_cost(c.len()), spec.work * dam_preview::length_factor(c.len()) * self.tech.build_work_factor()),
            None => (spec.cost.to_vec(), spec.work * self.tech.build_work_factor()),
        };
        if let Err(e) = self.work.add_site(id, kind, &cost, labour) {
            self.buildings.buildings.retain(|b| b.id != id);
            return Err(SimError::InvalidCommand(format!("{e:?}")));
        }
        if let Some(cells) = dam_cells {
            self.work.dam_plans.push(work::DamPlan { building: id, cells, height: dam_preview::DEFAULT_HEIGHT });
        }
        Ok(id)
    }

    /// Where a dam on `cells` puts its construction site: the line cell nearest its middle (the
    /// middle first, ties toward the start) where the site can be placed (it must touch water).
    /// The error is the middle cell's if none can. Changes nothing.
    pub fn dam_site(&self, cells: &[water::Cell]) -> Result<water::Cell, buildings::BuildError> {
        let terrain = buildings::Terrain { height: &self.height, depth: &self.water.depth };
        let mid = cells.len() / 2;
        let mut order: Vec<usize> = (0..cells.len()).collect();
        order.sort_by_key(|&i| (i.abs_diff(mid), i));
        let mut first = None;
        for i in order {
            match self.buildings.can_place_site(buildings::BuildingKind::Dam, cells[i], &self.tech, terrain) {
                Ok(()) => return Ok(cells[i]),
                Err(e) => {
                    first.get_or_insert(e);
                }
            }
        }
        Err(first.unwrap_or(buildings::BuildError::OutOfBounds))
    }

    /// Places a dam construction site on `cells` (at `dam_site`). Its cost is the dam cost
    /// scaled by length and its labour scales the same way; `agents::step` applies the water dam
    /// when the site completes. Errors (state unchanged) if `cells` are not a valid dam line, the
    /// height is not finite and positive, or the site cannot be placed.
    pub fn order_dam(&mut self, cells: &[water::Cell], height: f32) -> Result<buildings::BuildingId, SimError> {
        if dam_preview::preview(self, cells, height).is_none() {
            return Err(SimError::InvalidCommand("not a valid dam line (cells empty, repeated or off the map, or height not positive)".into()));
        }
        let pos = self.dam_site(cells).map_err(|e| SimError::InvalidCommand(e.to_string()))?;
        let terrain = buildings::Terrain { height: &self.height, depth: &self.water.depth };
        let id = self
            .buildings
            .place_site(buildings::BuildingKind::Dam, pos, &self.tech, terrain)
            .map_err(|e| SimError::InvalidCommand(e.to_string()))?;
        let labour = buildings::BuildingKind::Dam.spec().work * dam_preview::length_factor(cells.len()) * self.tech.build_work_factor();
        if let Err(e) = self.work.add_site(id, buildings::BuildingKind::Dam, &dam_preview::dam_cost(cells.len()), labour) {
            self.buildings.buildings.retain(|b| b.id != id);
            return Err(SimError::InvalidCommand(format!("{e:?}")));
        }
        self.work.dam_plans.push(work::DamPlan { building: id, cells: cells.to_vec(), height });
        Ok(id)
    }

    /// Cancels an unfinished construction site (`Site` or `UnderConstruction`): the building, its
    /// workplace and any dam plan go, materials already delivered return to the stores and any
    /// beaver assigned to its slots stands down. Returns what was refunded. Errors (state
    /// unchanged) for an unknown or finished building.
    pub fn cancel_site(&mut self, id: buildings::BuildingId) -> Result<Vec<buildings::Amount>, SimError> {
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
        if let Some(i) = self.work.places.iter().position(|p| p.building == id) {
            let place = self.work.places.remove(i);
            for &(g, a) in place.site.as_ref().map(|s| s.delivered.as_slice()).unwrap_or(&[]) {
                if a > 0.0 {
                    let put = self.buildings.stores.refund(g, a).map_err(|e| SimError::InvalidCommand(e.to_string()))?;
                    refunded.push((g, put));
                }
            }
        }
        self.work.dam_plans.retain(|d| d.building != id);
        self.work.managed.retain(|_, duty| !matches!(duty, work::ManagedDuty::Build(b, _) if *b == id));
        for ag in self.agents.by_id.values_mut() {
            if matches!(ag.assignment, Some(agents::Assignment::Slot(b, _)) if b == id) {
                ag.assignment = None;
            }
        }
        self.buildings.buildings.retain(|b| b.id != id);
        self.output.forget(id);
        Ok(refunded)
    }

    /// Applies a command. Errors on invalid arguments, leaving state unchanged.
    pub fn apply(&mut self, cmd: Command) -> Result<(), SimError> {
        match cmd {
            Command::Regenerate { seed } => {
                *self = SimState::new(seed);
                Ok(())
            }
            Command::BuildDam { cells, height } => self.water.build_dam(&cells, height).map_err(invalid),
            Command::OrderDam { cells, height } => self.order_dam(&cells, height).map(|_| ()),
            Command::Settle { cell } => starts::settle(self, cell),
            Command::BreachDam { x, y } => self.water.breach(x, y).map_err(invalid),
            Command::DigCanal { cells, depth } => {
                self.water.dig_canal(&mut self.height, &cells, depth).map_err(invalid)
            }
            Command::Assign { who, to } => {
                if let Some(kit) = who.iter().find(|id| self.colony.beaver(**id).is_some_and(|b| b.stage == colony::Stage::Kit)) {
                    return Err(SimError::InvalidCommand(format!("beaver {} is a kit and cannot work", kit.0)));
                }
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
                self.society.answer(id, answer, &mut self.colony).map_err(|e| SimError::InvalidCommand(format!("{e:?}")))
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
    fn a_dam_site_stands_on_the_line_cell_nearest_the_middle_that_touches_water() {
        let mut s = SimState::new(1);
        let (x0, y0) = (60, 20);
        s.water.depth.as_mut_slice().iter_mut().for_each(|d| *d = 0.0);
        let line: Vec<water::Cell> = (x0 - 8..=x0).map(|x| (x, y0)).collect();
        assert!(s.dam_site(&line).is_err(), "a line on dry ground has no site");
        *s.water.depth.get_mut(x0, y0).unwrap() = 0.5;
        // The middle cell's 3x1 site is dry; the nearest that reaches the water starts two short of it.
        assert_eq!(s.dam_site(&line), Ok((x0 - 2, y0)));
        let id = s.order_dam(&line, 1.0).unwrap();
        assert_eq!(s.buildings.building(id).unwrap().pos, (x0 - 2, y0));
    }

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
    fn the_colony_eats_from_the_stores_and_jobs_conjure_nothing() {
        let mut s = founded(4);
        let food = buildings::Good::Food;
        s.step(0.1).unwrap();
        assert_eq!(s.buildings.stores.amount(food), 0.0, "no food appears without gatherers");
        assert_eq!(s.colony.food, 0.0);
        s.buildings.stores.deposit(food, 40.0).unwrap();
        for b in s.colony.beavers.iter_mut() {
            b.needs.food = 0.2;
        }
        let eaten_before = s.buildings.stores.consumed_total[food.index()];
        s.step(0.5).unwrap();
        let left = s.buildings.stores.amount(food);
        assert!(left < 40.0, "hungry beavers ate from the stores ({left} left)");
        assert!(s.buildings.stores.consumed_total[food.index()] > eaten_before, "eating is booked as consumption");
        assert_eq!(s.colony.food, left, "the colony's larder mirrors the stores");
    }

    #[test]
    fn cancelling_a_site_refunds_what_was_delivered_and_removes_it() {
        use buildings::{BuildingKind, Good};
        let mut s = founded(6);
        s.tech.update(&s.colony);
        let c = s.colony.beavers[0].pos;
        let (cx, cy) = (c.0 as usize, c.1 as usize);
        let id = (2..30usize)
            .flat_map(|r| (cx.saturating_sub(r)..=cx + r).flat_map(move |x| (cy.saturating_sub(r)..=cy + r).map(move |y| (x, y))))
            .find_map(|p| s.place_site(BuildingKind::Warehouse, p).ok())
            .expect("room for a warehouse");
        s.work.deliver(id, Good::Logs, 5.0).unwrap();
        let logs = s.buildings.stores.amount(Good::Logs);
        let refunded = s.cancel_site(id).unwrap();
        assert_eq!(refunded, vec![(Good::Logs, 5.0)]);
        assert_eq!(s.buildings.stores.amount(Good::Logs), logs + 5.0);
        assert!(s.buildings.building(id).is_none() && s.work.place(id).is_none());
        assert!(s.cancel_site(id).is_err(), "already gone");
        s.step(0.2).unwrap(); // nobody trips over the vanished site
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
