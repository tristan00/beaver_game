//! Buildings: catalogue, stores ledger, construction via builder work orders, workshops,
//! water wheels (need flow) and barges (travel only on connected water).

use crate::colony::{Colony, ColonyError, Job, OrderId};
use crate::grid::Grid;
use crate::tech::Tech;
use crate::work::{Role, Work};
use std::fmt;
pub use crate::tuning::buildings::*;


// ───────────────────────────── goods & stores ─────────────────────────────

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Good {
    Logs,
    Sticks,
    Mud,
    Stone,
    Food,
    Baskets,
    Rope,
    Tools,
}

impl Good {
    pub const ALL: [Good; 8] =
        [Good::Logs, Good::Sticks, Good::Mud, Good::Stone, Good::Food, Good::Baskets, Good::Rope, Good::Tools];
    pub fn index(self) -> usize {
        self as usize
    }
}

const N: usize = Good::ALL.len();

/// A (good, amount) pair used for costs, inputs and cargo.
pub type Amount = (Good, f32);

#[derive(Clone, Debug, PartialEq)]
pub enum StoresError {
    /// Amount was NaN, infinite or negative.
    InvalidAmount(f32),
    Insufficient { good: Good, needed: f32, have: f32 },
}

impl fmt::Display for StoresError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StoresError::InvalidAmount(a) => write!(f, "invalid amount {a} (must be finite and >= 0)"),
            StoresError::Insufficient { good, needed, have } => {
                write!(f, "not enough {good:?}: need {needed}, have {have}")
            }
        }
    }
}

fn check_amount(a: f32) -> Result<(), StoresError> {
    if a.is_finite() && a >= 0.0 { Ok(()) } else { Err(StoresError::InvalidAmount(a)) }
}

/// Stores ledger: current amounts, capacity, and production/consumption totals and rates.
#[derive(Clone, Debug, PartialEq)]
pub struct Stores {
    amounts: [f32; N],
    capacity: [f32; N],
    /// Lifetime totals.
    pub produced_total: [f32; N],
    pub consumed_total: [f32; N],
    /// Rates per day over the last step with dt > 0.
    pub production_rate: [f32; N],
    pub consumption_rate: [f32; N],
    tick_produced: [f32; N],
    tick_consumed: [f32; N],
}

impl Default for Stores {
    fn default() -> Self {
        Self {
            amounts: [0.0; N],
            capacity: [BASE_CAPACITY; N],
            produced_total: [0.0; N],
            consumed_total: [0.0; N],
            production_rate: [0.0; N],
            consumption_rate: [0.0; N],
            tick_produced: [0.0; N],
            tick_consumed: [0.0; N],
        }
    }
}

impl Stores {
    pub fn amount(&self, g: Good) -> f32 {
        self.amounts[g.index()]
    }
    pub fn capacity(&self, g: Good) -> f32 {
        self.capacity[g.index()]
    }
    pub fn space(&self, g: Good) -> f32 {
        (self.capacity[g.index()] - self.amounts[g.index()]).max(0.0)
    }
    pub fn net_rate(&self, g: Good) -> f32 {
        self.production_rate[g.index()] - self.consumption_rate[g.index()]
    }

    /// Adds produced goods (counted as production); returns the amount accepted given capacity.
    pub fn deposit(&mut self, g: Good, amount: f32) -> Result<f32, StoresError> {
        let taken = self.put_raw(g, amount)?;
        self.tick_produced[g.index()] += taken;
        self.produced_total[g.index()] += taken;
        Ok(taken)
    }

    /// Removes goods as consumption. Errors (changing nothing) if there is not enough.
    pub fn withdraw(&mut self, g: Good, amount: f32) -> Result<(), StoresError> {
        self.take_raw(g, amount)?;
        self.tick_consumed[g.index()] += amount;
        self.consumed_total[g.index()] += amount;
        Ok(())
    }

    pub fn can_afford(&self, costs: &[Amount]) -> Result<(), StoresError> {
        for &(g, a) in costs {
            let have = self.amount(g);
            if have < a {
                return Err(StoresError::Insufficient { good: g, needed: a, have });
            }
        }
        Ok(())
    }

    /// Withdraws all `costs` atomically.
    pub fn withdraw_all(&mut self, costs: &[Amount]) -> Result<(), StoresError> {
        self.can_afford(costs)?;
        for &(g, a) in costs {
            self.withdraw(g, a)?;
        }
        Ok(())
    }

    /// Moves goods without touching the ledger (refunds, barge cargo).
    fn put_raw(&mut self, g: Good, amount: f32) -> Result<f32, StoresError> {
        check_amount(amount)?;
        let taken = amount.min(self.space(g));
        self.amounts[g.index()] += taken;
        Ok(taken)
    }
    fn take_raw(&mut self, g: Good, amount: f32) -> Result<(), StoresError> {
        check_amount(amount)?;
        let have = self.amount(g);
        if have < amount {
            return Err(StoresError::Insufficient { good: g, needed: amount, have });
        }
        self.amounts[g.index()] -= amount;
        Ok(())
    }

    fn end_tick(&mut self, dt: f32) {
        if dt > 0.0 {
            for i in 0..N {
                self.production_rate[i] = self.tick_produced[i] / dt;
                self.consumption_rate[i] = self.tick_consumed[i] / dt;
            }
        }
        self.tick_produced = [0.0; N];
        self.tick_consumed = [0.0; N];
    }
}

// ───────────────────────────── catalogue ─────────────────────────────

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum BuildingKind {
    Dam,
    Lodge,
    Burrow,
    FoodCache,
    ScentMound,
    GuardPost,
    Canal,
    Lock,
    WaterWheel,
    Workshop,
    Warehouse,
    Dock,
    Barge,
    // Early-lake elementary gathering camps.
    LoggingCamp,
    ForagerHut,
    MudPit,
    QuarryCamp,
    // Industrial lake.
    Mill,
    Storehouse,
}

/// Terrain rule for a footprint.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Site {
    /// Every footprint cell dry.
    Dry,
    /// At least one footprint cell wet.
    TouchesWater,
    /// A single wet cell next to a completed dock.
    WaterByDock,
}

#[derive(Clone, Copy, Debug)]
pub struct Spec {
    pub name: &'static str,
    pub cost: &'static [Amount],
    /// Footprint width and height in cells.
    pub footprint: (usize, usize),
    /// Builder work units (one adult working day is about 0.6).
    pub work: f32,
    pub site: Site,
}

impl BuildingKind {
    pub const ALL: [BuildingKind; 19] = [
        BuildingKind::Dam, BuildingKind::Lodge, BuildingKind::Burrow, BuildingKind::FoodCache,
        BuildingKind::ScentMound, BuildingKind::GuardPost, BuildingKind::Canal, BuildingKind::Lock,
        BuildingKind::WaterWheel, BuildingKind::Workshop, BuildingKind::Warehouse, BuildingKind::Dock,
        BuildingKind::Barge, BuildingKind::LoggingCamp, BuildingKind::ForagerHut, BuildingKind::MudPit,
        BuildingKind::QuarryCamp, BuildingKind::Mill, BuildingKind::Storehouse,
    ];

    /// Name, footprint and site rule; cost and build work come from `tuning::buildings::cost_and_work`.
    pub fn spec(self) -> Spec {
        let (name, footprint, site) = match self {
            BuildingKind::Dam => ("Dam", (3, 1), Site::TouchesWater),
            BuildingKind::Lodge => ("Lodge", (3, 3), Site::Dry),
            BuildingKind::Burrow => ("Burrow hideout", (1, 1), Site::Dry),
            BuildingKind::FoodCache => ("Food cache", (2, 2), Site::Dry),
            BuildingKind::ScentMound => ("Scent mound", (1, 1), Site::Dry),
            BuildingKind::GuardPost => ("Guard post", (1, 1), Site::Dry),
            BuildingKind::Canal => ("Canal", (1, 1), Site::TouchesWater),
            BuildingKind::Lock => ("Lock", (2, 1), Site::TouchesWater),
            BuildingKind::WaterWheel => ("Water wheel", (2, 2), Site::TouchesWater),
            BuildingKind::Workshop => ("Workshop", (3, 2), Site::Dry),
            BuildingKind::Warehouse => ("Warehouse", (3, 3), Site::Dry),
            BuildingKind::Dock => ("Dock", (2, 1), Site::TouchesWater),
            BuildingKind::Barge => ("Barge", (1, 1), Site::WaterByDock),
            BuildingKind::LoggingCamp => ("Logging camp", (2, 2), Site::Dry),
            BuildingKind::ForagerHut => ("Forager hut", (2, 2), Site::Dry),
            BuildingKind::MudPit => ("Mud pit", (2, 2), Site::Dry),
            BuildingKind::QuarryCamp => ("Quarry camp", (2, 2), Site::Dry),
            BuildingKind::Mill => ("Mill", (3, 3), Site::Dry),
            BuildingKind::Storehouse => ("Storehouse", (4, 3), Site::Dry),
        };
        let (cost, work) = cost_and_work(self);
        Spec { name, cost, footprint, work, site }
    }
}

/// Workshop recipes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Recipe {
    Baskets,
    Rope,
    Tools,
}

impl Recipe {
    /// (inputs, output) for one batch.
    pub fn batch(self) -> (&'static [Amount], Amount) {
        recipe_batch(self)
    }
}

// ───────────────────────────── state ─────────────────────────────

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct BuildingId(pub u64);
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct BargeId(pub u64);

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Status {
    /// Waiting for builders; the colony work order tracks progress.
    UnderConstruction(OrderId),
    /// A construction site fed by hauled materials and builder labour; `work::Work` tracks it and
    /// marks the building `Complete` when its cost is delivered and its labour done.
    Site,
    Complete,
}

#[derive(Clone, Debug)]
pub struct Building {
    pub id: BuildingId,
    pub kind: BuildingKind,
    /// Top-left footprint cell.
    pub pos: (usize, usize),
    pub status: Status,
    /// Workshops only.
    pub recipe: Option<Recipe>,
    /// Water wheels: current output 0..=1 (0 without flow).
    pub output: f32,
}

impl Building {
    pub fn is_complete(&self) -> bool {
        self.status == Status::Complete
    }
    pub fn cells(&self) -> impl Iterator<Item = (usize, usize)> + use<> {
        let (w, h) = self.kind.spec().footprint;
        let (x, y) = self.pos;
        (0..h).flat_map(move |dy| (0..w).map(move |dx| (x + dx, y + dy)))
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum BargeState {
    Idle,
    Sailing,
    /// Water along the route dried up; cargo stays aboard.
    Stranded,
}

#[derive(Clone, Debug)]
pub struct Barge {
    pub id: BargeId,
    pub cell: (usize, usize),
    pub cargo: Option<Amount>,
    pub state: BargeState,
    /// Remaining route, reversed so the last element is the next cell.
    route: Vec<(usize, usize)>,
    dest: Option<BuildingId>,
    progress: f32,
    /// Dock where an idle barge sits.
    pub at_dock: Option<BuildingId>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum BuildError {
    Locked(BuildingKind),
    OutOfBounds,
    Blocked,
    BadTerrain(&'static str),
    NoDock,
    Stores(StoresError),
    Colony(ColonyError),
    UnknownBuilding(BuildingId),
    UnknownBarge(BargeId),
    NotComplete,
    WrongKind(&'static str),
    /// Barge is not on water, or the destination is not reachable over connected water.
    NoWaterRoute,
    BargeBusy,
    CargoTooLarge { max: f32 },
}

impl fmt::Display for BuildError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for BuildError {}
impl From<StoresError> for BuildError {
    fn from(e: StoresError) -> Self {
        BuildError::Stores(e)
    }
}
impl From<ColonyError> for BuildError {
    fn from(e: ColonyError) -> Self {
        BuildError::Colony(e)
    }
}

/// Read-only terrain used by buildings.
#[derive(Clone, Copy)]
pub struct Terrain<'a> {
    pub height: &'a Grid<f32>,
    pub depth: &'a Grid<f32>,
}

impl Terrain<'_> {
    fn wet(&self, x: usize, y: usize) -> bool {
        self.depth.get(x, y).is_some_and(|&d| d >= WET_DEPTH)
    }
    fn surface(&self, x: usize, y: usize) -> f32 {
        self.height.get(x, y).copied().unwrap_or(0.0) + self.depth.get(x, y).copied().unwrap_or(0.0)
    }
    /// Largest surface drop from this wet cell to a wet 4-neighbour; 0 if the cell is dry or still.
    pub fn flow(&self, x: usize, y: usize) -> f32 {
        if !self.wet(x, y) {
            return 0.0;
        }
        let s = self.surface(x, y);
        neighbours(x, y)
            .filter(|&(nx, ny)| self.wet(nx, ny))
            .map(|(nx, ny)| s - self.surface(nx, ny))
            .fold(0.0, f32::max)
    }
}

fn neighbours(x: usize, y: usize) -> impl Iterator<Item = (usize, usize)> {
    [(x.wrapping_sub(1), y), (x + 1, y), (x, y.wrapping_sub(1)), (x, y + 1)].into_iter()
}

#[derive(Clone, Debug, Default)]
pub struct Buildings {
    pub stores: Stores,
    pub buildings: Vec<Building>,
    pub barges: Vec<Barge>,
    /// Total running water-wheel output (sum of wheel outputs).
    pub power: f32,
    next_id: u64,
}

impl Buildings {
    pub fn building(&self, id: BuildingId) -> Option<&Building> {
        self.buildings.iter().find(|b| b.id == id)
    }
    pub fn barge(&self, id: BargeId) -> Option<&Barge> {
        self.barges.iter().find(|b| b.id == id)
    }
    pub fn count_complete(&self, kind: BuildingKind) -> usize {
        self.buildings.iter().filter(|b| b.kind == kind && b.is_complete()).count()
    }

    fn fresh_id(&mut self) -> u64 {
        self.next_id += 1;
        self.next_id
    }

    /// Placeholder-free probe used to compute footprints before a building exists.
    fn footprint(kind: BuildingKind, pos: (usize, usize)) -> Vec<(usize, usize)> {
        Building { id: BuildingId(0), kind, pos, status: Status::Complete, recipe: None, output: 0.0 }.cells().collect()
    }

    /// Checks tech, bounds, overlap and terrain for placing `kind` at `pos`; changes nothing.
    fn check_placement(
        &self,
        kind: BuildingKind,
        pos: (usize, usize),
        tech: &Tech,
        terrain: Terrain,
    ) -> Result<(), BuildError> {
        if !tech.allows(kind) {
            return Err(BuildError::Locked(kind));
        }
        self.check_ground(kind, pos, terrain)
    }

    /// Bounds, overlap and terrain rules only (no tech check); changes nothing.
    fn check_ground(&self, kind: BuildingKind, pos: (usize, usize), terrain: Terrain) -> Result<(), BuildError> {
        let spec = kind.spec();
        let cells = Self::footprint(kind, pos);
        if cells.iter().any(|&(x, y)| !terrain.depth.in_bounds(x, y)) {
            return Err(BuildError::OutOfBounds);
        }
        if self.buildings.iter().any(|b| b.cells().any(|c| cells.contains(&c))) {
            return Err(BuildError::Blocked);
        }
        match spec.site {
            Site::Dry if cells.iter().any(|&(x, y)| terrain.wet(x, y)) => {
                return Err(BuildError::BadTerrain("needs dry ground"));
            }
            Site::TouchesWater if !cells.iter().any(|&(x, y)| terrain.wet(x, y)) => {
                return Err(BuildError::BadTerrain("needs water"));
            }
            Site::WaterByDock => {
                if !terrain.wet(pos.0, pos.1) {
                    return Err(BuildError::BadTerrain("needs water"));
                }
                if !self.docks().any(|d| dock_access(d, terrain).contains(&pos)) {
                    return Err(BuildError::NoDock);
                }
            }
            _ => {}
        }
        Ok(())
    }

    /// Whether `place_site` would accept `kind` at `pos` (tech, bounds, overlap, terrain); changes nothing.
    pub fn can_place_site(&self, kind: BuildingKind, pos: (usize, usize), tech: &Tech, terrain: Terrain) -> Result<(), BuildError> {
        self.check_placement(kind, pos, tech, terrain)
    }

    /// Places an empty construction site (`Status::Site`) with the same checks as `place` but no
    /// cost taken: materials are hauled in later. The caller registers it with `work::Work`.
    pub fn place_site(
        &mut self,
        kind: BuildingKind,
        pos: (usize, usize),
        tech: &Tech,
        terrain: Terrain,
    ) -> Result<BuildingId, BuildError> {
        self.check_placement(kind, pos, tech, terrain)?;
        let id = BuildingId(self.fresh_id());
        self.buildings.push(Building { id, kind, pos, status: Status::Site, recipe: None, output: 0.0 });
        Ok(id)
    }

    /// Adds an already finished building (a start's founding stockpile): same ground checks as
    /// `place_site` but no tech requirement, no cost, no work. Returns its id.
    pub fn add_complete(&mut self, kind: BuildingKind, pos: (usize, usize), terrain: Terrain) -> Result<BuildingId, BuildError> {
        self.check_ground(kind, pos, terrain)?;
        let id = BuildingId(self.fresh_id());
        self.buildings.push(Building { id, kind, pos, status: Status::Complete, recipe: None, output: 0.0 });
        Ok(id)
    }

    /// Places `kind` with its top-left at `pos`: checks tech, bounds, terrain and overlap, consumes
    /// the cost from stores and queues a builder work order. Nothing changes on error.
    pub fn place(
        &mut self,
        kind: BuildingKind,
        pos: (usize, usize),
        tech: &Tech,
        terrain: Terrain,
        colony: &mut Colony,
    ) -> Result<BuildingId, BuildError> {
        self.check_placement(kind, pos, tech, terrain)?;
        let spec = kind.spec();
        let work = spec.work * tech.build_work_factor();
        let centre = (pos.0 as f32 + spec.footprint.0 as f32 / 2.0, pos.1 as f32 + spec.footprint.1 as f32 / 2.0);
        self.stores.can_afford(spec.cost)?;
        let order = colony.queue_order(Job::Builder, centre, work)?;
        self.stores.withdraw_all(spec.cost).expect("affordability checked above");
        let id = BuildingId(self.fresh_id());
        self.buildings.push(Building { id, kind, pos, status: Status::UnderConstruction(order), recipe: None, output: 0.0 });
        Ok(id)
    }

    /// Cancels an unfinished building: cancels its work order and refunds the full cost.
    /// Errors if the building is unknown or its construction has already finished.
    pub fn cancel(&mut self, id: BuildingId, colony: &mut Colony) -> Result<(), BuildError> {
        let i = self.buildings.iter().position(|b| b.id == id).ok_or(BuildError::UnknownBuilding(id))?;
        let Status::UnderConstruction(order) = self.buildings[i].status else {
            return Err(BuildError::WrongKind("only unfinished buildings can be cancelled"));
        };
        match colony.cancel_order(order) {
            Ok(()) => {}
            Err(ColonyError::UnknownOrder(_)) => return Err(BuildError::WrongKind("construction already finished")),
            Err(e) => return Err(e.into()),
        }
        let b = self.buildings.remove(i);
        for &(g, a) in b.kind.spec().cost {
            self.stores.put_raw(g, a)?;
        }
        Ok(())
    }

    pub fn set_recipe(&mut self, id: BuildingId, recipe: Option<Recipe>) -> Result<(), BuildError> {
        let b = self.buildings.iter_mut().find(|b| b.id == id).ok_or(BuildError::UnknownBuilding(id))?;
        if crate::catalogue::def(b.kind).batches_per_day <= 0.0 {
            return Err(BuildError::WrongKind("only workshops craft"));
        }
        b.recipe = recipe;
        Ok(())
    }

    fn docks(&self) -> impl Iterator<Item = &Building> {
        self.buildings.iter().filter(|b| b.kind == BuildingKind::Dock && b.is_complete())
    }

    /// Loads `cargo` from stores onto an idle barge and sends it to dock `dest` over connected
    /// water. Errors with `NoWaterRoute` (loading nothing) if no water path joins them.
    pub fn dispatch(
        &mut self,
        barge: BargeId,
        dest: BuildingId,
        cargo: Option<Amount>,
        terrain: Terrain,
    ) -> Result<(), BuildError> {
        let d = self.building(dest).ok_or(BuildError::UnknownBuilding(dest))?;
        if d.kind != BuildingKind::Dock {
            return Err(BuildError::WrongKind("destination must be a dock"));
        }
        if !d.is_complete() {
            return Err(BuildError::NotComplete);
        }
        let targets = dock_access(d, terrain);
        let bi = self.barges.iter().position(|b| b.id == barge).ok_or(BuildError::UnknownBarge(barge))?;
        let b = &self.barges[bi];
        if b.state != BargeState::Idle || b.cargo.is_some() {
            return Err(BuildError::BargeBusy);
        }
        if let Some((_, a)) = cargo {
            check_amount(a)?;
            if a > BARGE_CAPACITY {
                return Err(BuildError::CargoTooLarge { max: BARGE_CAPACITY });
            }
        }
        let mut route = water_route(terrain, b.cell, &targets).ok_or(BuildError::NoWaterRoute)?;
        if let Some((g, a)) = cargo {
            self.stores.take_raw(g, a)?;
        }
        route.reverse();
        let b = &mut self.barges[bi];
        b.cargo = cargo.filter(|c| c.1 > 0.0);
        b.route = route;
        b.dest = Some(dest);
        b.state = BargeState::Sailing;
        b.at_dock = None;
        Ok(())
    }

    /// Advances construction, workshops, water wheels and barges by `dt_days`.
    /// `dt_days` must be finite and >= 0 (validated by `SimState::step`). Workshops craft only as
    /// fast as they are staffed (see `crafters`): `work` holds slot workers of site-built workplaces.
    pub fn step(&mut self, dt_days: f32, tech: &Tech, terrain: Terrain, colony: &Colony, work: &Work) {
        self.complete_finished(colony, terrain);
        self.update_capacity(tech);
        self.update_wheels(terrain);
        let crafters = self.crafters(colony, work);
        self.run_workshops(dt_days, tech, &crafters);
        self.move_barges(dt_days, terrain);
        self.stores.end_tick(dt_days);
    }

    fn complete_finished(&mut self, colony: &Colony, terrain: Terrain) {
        let mut spawned = Vec::new();
        self.buildings.retain_mut(|b| {
            let Status::UnderConstruction(order) = b.status else { return true };
            if colony.orders.iter().any(|o| o.id == order) {
                return true;
            }
            if b.kind == BuildingKind::Barge {
                spawned.push(b.pos);
                return false;
            }
            b.status = Status::Complete;
            true
        });
        for cell in spawned {
            let at_dock = self.docks().find(|d| dock_access(d, terrain).contains(&cell)).map(|d| d.id);
            let id = BargeId(self.fresh_id());
            self.barges.push(Barge { id, cell, cargo: None, state: BargeState::Idle, route: vec![], dest: None, progress: 0.0, at_dock });
        }
    }

    /// Capacity per good: the base plus every complete store's catalogue capacity for goods it takes.
    pub(crate) fn update_capacity(&mut self, tech: &Tech) {
        let f = tech.storage_factor();
        for g in Good::ALL {
            let stored: f32 = self
                .buildings
                .iter()
                .filter(|b| b.is_complete() && crate::agents::accepts(b.kind, g))
                .map(|b| crate::catalogue::def(b.kind).capacity)
                .sum();
            self.stores.capacity[g.index()] = (BASE_CAPACITY + stored) * f;
        }
    }

    fn update_wheels(&mut self, terrain: Terrain) {
        self.power = 0.0;
        for b in self.buildings.iter_mut().filter(|b| b.kind == BuildingKind::WaterWheel) {
            let flow = if b.is_complete() { b.cells().map(|(x, y)| terrain.flow(x, y)).fold(0.0, f32::max) } else { 0.0 };
            b.output = if flow >= MIN_FLOW { (flow / FULL_FLOW).min(1.0) } else { 0.0 };
            self.power += b.output;
        }
    }

    /// Workers crafting at each complete building that has a recipe, in building order.
    /// A building registered in `work` (placed as a site) counts its filled `Craft` slots. A
    /// building placed by work order has no workplace; it is staffed from the colony's working
    /// `Crafter`s, shared out in building order, each building taking at most its slot count.
    fn crafters(&self, colony: &Colony, work: &Work) -> Vec<(BuildingId, usize)> {
        let mut pool = colony.beavers.iter().filter(|b| b.current_job == Some(Job::Crafter)).count();
        self.buildings
            .iter()
            .filter(|b| b.is_complete() && b.recipe.is_some())
            .map(|b| {
                let n = match work.place(b.id) {
                    Some(p) => p.slots.iter().filter(|s| s.role == Role::Craft && s.worker.is_some()).count(),
                    None => {
                        let slots = crate::catalogue::def(b.kind).slots.iter().filter(|r| **r == Role::Craft).count();
                        let n = slots.min(pool);
                        pool -= n;
                        n
                    }
                };
                (b.id, n)
            })
            .collect()
    }

    /// Crafts at every staffed workshop: `batches_per_worker` x workers, limited by inputs and space.
    fn run_workshops(&mut self, dt: f32, tech: &Tech, crafters: &[(BuildingId, usize)]) {
        if dt <= 0.0 {
            return;
        }
        let recipes: Vec<(Recipe, f32)> = crafters
            .iter()
            .filter(|(_, n)| *n > 0)
            .filter_map(|&(id, n)| {
                let b = self.building(id)?;
                Some((b.recipe?, crate::catalogue::def(b.kind).batches_per_worker() * n as f32))
            })
            .collect();
        if recipes.is_empty() {
            return;
        }
        // Wheel power is shared evenly; a fully powered workshop gains `WHEEL_MAX_BOOST`.
        let boost = 1.0 + (self.power / recipes.len() as f32).min(WHEEL_MAX_BOOST);
        for (r, rate) in recipes {
            let (inputs, (out, per)) = r.batch();
            let mut batches = rate * dt * boost * tech.craft_speed_factor();
            for &(g, a) in inputs {
                batches = batches.min(self.stores.amount(g) / a);
            }
            batches = batches.min(self.stores.space(out) / per);
            if batches <= 0.0 {
                continue;
            }
            for &(g, a) in inputs {
                self.stores.withdraw(g, a * batches).expect("limited by availability");
            }
            self.stores.deposit(out, per * batches).expect("amount is finite and >= 0");
        }
    }

    fn move_barges(&mut self, dt: f32, terrain: Terrain) {
        for b in self.barges.iter_mut() {
            if b.state == BargeState::Sailing {
                b.progress += BARGE_SPEED * dt;
                while b.progress >= 1.0 {
                    let Some(&next) = b.route.last() else { break };
                    if !terrain.wet(next.0, next.1) {
                        b.state = BargeState::Stranded;
                        break;
                    }
                    b.route.pop();
                    b.cell = next;
                    b.progress -= 1.0;
                }
                if b.state == BargeState::Sailing && b.route.is_empty() {
                    b.state = BargeState::Idle;
                    b.progress = 0.0;
                    b.at_dock = b.dest.take();
                }
            }
            // Idle barges at a dock hand over cargo as capacity allows.
            if b.state == BargeState::Idle && b.at_dock.is_some() {
                if let Some((g, a)) = b.cargo {
                    let put = self.stores.put_raw(g, a).expect("cargo amount is valid");
                    b.cargo = (a - put > 1e-6).then_some((g, a - put));
                }
            }
        }
    }
}

/// Wet cells inside or 4-adjacent to a dock's footprint, where barges can berth.
fn dock_access(dock: &Building, terrain: Terrain) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    for (x, y) in dock.cells() {
        for c in std::iter::once((x, y)).chain(neighbours(x, y)) {
            if terrain.wet(c.0, c.1) && !out.contains(&c) {
                out.push(c);
            }
        }
    }
    out
}

/// Shortest 4-connected path over wet cells from `from` to any of `targets` (excluding `from`).
/// `None` if `from` is dry or no target is connected.
fn water_route(terrain: Terrain, from: (usize, usize), targets: &[(usize, usize)]) -> Option<Vec<(usize, usize)>> {
    if !terrain.wet(from.0, from.1) {
        return None;
    }
    let (w, h) = (terrain.depth.width(), terrain.depth.height());
    let mut prev: Vec<Option<(usize, usize)>> = vec![None; w * h];
    let mut seen = vec![false; w * h];
    seen[from.1 * w + from.0] = true;
    let mut queue = std::collections::VecDeque::from([from]);
    while let Some(c) = queue.pop_front() {
        if targets.contains(&c) {
            let mut path = vec![];
            let mut cur = c;
            while cur != from {
                path.push(cur);
                cur = prev[cur.1 * w + cur.0].expect("visited cells have predecessors");
            }
            path.reverse();
            return Some(path);
        }
        for n in neighbours(c.0, c.1) {
            if terrain.wet(n.0, n.1) && !seen[n.1 * w + n.0] {
                seen[n.1 * w + n.0] = true;
                prev[n.1 * w + n.0] = Some(c);
                queue.push_back(n);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tech::TECHS;

    struct World {
        height: Grid<f32>,
        depth: Grid<f32>,
    }
    impl World {
        /// 16x8 land with a horizontal river on rows 3..=4.
        fn new() -> Self {
            let height = Grid::new(16, 8, 1.0);
            let mut depth = Grid::new(16, 8, 0.0);
            for x in 0..16 {
                for y in 3..=4 {
                    *depth.get_mut(x, y).unwrap() = 0.5;
                }
            }
            Self { height, depth }
        }
        fn t(&self) -> Terrain<'_> {
            Terrain { height: &self.height, depth: &self.depth }
        }
        fn set_column(&mut self, x: usize, d: f32) {
            for y in 3..=4 {
                *self.depth.get_mut(x, y).unwrap() = d;
            }
        }
    }

    fn all_tech() -> Tech {
        let mut t = Tech::default();
        let mut c = Colony::new(1);
        c.experience = [100.0; 8];
        t.update(&c);
        assert_eq!(t.unlocked_ids().count(), TECHS.len());
        t
    }

    fn rich(b: &mut Buildings) {
        for g in Good::ALL {
            b.stores.deposit(g, 90.0).unwrap();
        }
    }

    /// Completes all builder orders by clearing them (stands in for builders).
    fn finish_orders(c: &mut Colony) {
        c.orders.clear();
    }

    #[test]
    fn building_consumes_stores_and_queues_work() {
        let w = World::new();
        let (tech, mut c) = (all_tech(), Colony::new(1));
        let mut b = Buildings::default();
        let err = b.place(BuildingKind::Lodge, (1, 0), &tech, w.t(), &mut c).unwrap_err();
        assert!(matches!(err, BuildError::Stores(StoresError::Insufficient { .. })));
        assert!(c.orders.is_empty() && b.buildings.is_empty());
        rich(&mut b);
        let before = b.stores.amount(Good::Logs);
        let id = b.place(BuildingKind::Lodge, (1, 0), &tech, w.t(), &mut c).unwrap();
        assert_eq!(b.stores.amount(Good::Logs), before - 20.0);
        assert_eq!(b.stores.consumed_total[Good::Logs.index()], 20.0);
        assert_eq!(c.orders.len(), 1);
        b.step(1.0, &tech, w.t(), &c, &Work::default());
        assert!(!b.building(id).unwrap().is_complete());
        assert_eq!(b.stores.consumption_rate[Good::Logs.index()], 20.0);
        finish_orders(&mut c);
        b.step(1.0, &tech, w.t(), &c, &Work::default());
        assert!(b.building(id).unwrap().is_complete());
    }

    #[test]
    fn consumption_rate_reflects_the_step_it_happens_in() {
        let w = World::new();
        let (tech, mut c) = (all_tech(), Colony::new(1));
        let mut b = Buildings::default();
        rich(&mut b);
        b.place(BuildingKind::Burrow, (1, 0), &tech, w.t(), &mut c).unwrap();
        b.step(0.5, &tech, w.t(), &c, &Work::default());
        assert_eq!(b.stores.consumption_rate[Good::Mud.index()], 16.0);
        b.step(0.5, &tech, w.t(), &c, &Work::default());
        assert_eq!(b.stores.consumption_rate[Good::Mud.index()], 0.0);
    }

    #[test]
    fn placement_rules_and_cancel_refund() {
        let w = World::new();
        let (tech, mut c) = (all_tech(), Colony::new(1));
        let mut b = Buildings::default();
        rich(&mut b);
        assert_eq!(b.place(BuildingKind::Lodge, (15, 0), &tech, w.t(), &mut c), Err(BuildError::OutOfBounds));
        assert!(matches!(b.place(BuildingKind::Lodge, (1, 2), &tech, w.t(), &mut c), Err(BuildError::BadTerrain(_))));
        assert!(matches!(b.place(BuildingKind::Dock, (1, 0), &tech, w.t(), &mut c), Err(BuildError::BadTerrain(_))));
        let id = b.place(BuildingKind::Burrow, (1, 0), &tech, w.t(), &mut c).unwrap();
        assert_eq!(b.place(BuildingKind::Burrow, (1, 0), &tech, w.t(), &mut c), Err(BuildError::Blocked));
        let mud = b.stores.amount(Good::Mud);
        b.cancel(id, &mut c).unwrap();
        assert_eq!(b.stores.amount(Good::Mud), mud + 8.0);
        assert!(c.orders.is_empty() && b.buildings.is_empty());
        assert_eq!(
            b.place(BuildingKind::Workshop, (1, 0), &Tech::default(), w.t(), &mut c),
            Err(BuildError::Locked(BuildingKind::Workshop))
        );
    }

    #[test]
    fn workshop_crafts_from_stores_and_ledger_tracks_rates() {
        let w = World::new();
        let (tech, mut c) = (all_tech(), Colony::new(1));
        let mut b = Buildings::default();
        rich(&mut b);
        let id = b.place(BuildingKind::Workshop, (1, 0), &tech, w.t(), &mut c).unwrap();
        b.set_recipe(id, Some(Recipe::Baskets)).unwrap();
        finish_orders(&mut c);
        b.step(0.0, &tech, w.t(), &c, &Work::default());
        // Placed by work order, so staffed by the colony's crafters: none yet, nothing made.
        let (baskets, sticks) = (b.stores.amount(Good::Baskets), b.stores.amount(Good::Sticks));
        b.step(0.5, &tech, w.t(), &c, &Work::default());
        assert_eq!((b.stores.amount(Good::Baskets), b.stores.amount(Good::Sticks)), (baskets, sticks));
        c.add_immigrants(1);
        c.beavers[0].current_job = Some(Job::Crafter);
        b.step(0.5, &tech, w.t(), &c, &Work::default());
        assert!(b.stores.amount(Good::Baskets) > baskets);
        assert!(b.stores.amount(Good::Sticks) < sticks);
        assert!(b.stores.production_rate[Good::Baskets.index()] > 0.0);
        assert!(b.stores.consumption_rate[Good::Sticks.index()] > 0.0);
        assert!(b.stores.net_rate(Good::Sticks) < 0.0);
        assert!(b.set_recipe(BuildingId(999), None).is_err());
    }

    #[test]
    fn water_wheel_produces_only_with_flow() {
        let mut w = World::new();
        let (tech, mut c) = (all_tech(), Colony::new(1));
        let mut b = Buildings::default();
        rich(&mut b);
        let id = b.place(BuildingKind::WaterWheel, (5, 3), &tech, w.t(), &mut c).unwrap();
        finish_orders(&mut c);
        // still water: flat surface
        b.step(1.0, &tech, w.t(), &c, &Work::default());
        assert!(b.building(id).unwrap().is_complete());
        assert_eq!(b.building(id).unwrap().output, 0.0);
        assert_eq!(b.power, 0.0);
        // slope the riverbed so the surface drops downstream
        for x in 0..16 {
            for y in 3..=4 {
                *w.height.get_mut(x, y).unwrap() = 1.0 - 0.1 * x as f32;
            }
        }
        b.step(1.0, &tech, w.t(), &c, &Work::default());
        assert!(b.building(id).unwrap().output > 0.0 && b.power > 0.0);
        // dry the river: no output
        for x in 0..16 {
            w.set_column(x, 0.0);
        }
        b.step(1.0, &tech, w.t(), &c, &Work::default());
        assert_eq!(b.building(id).unwrap().output, 0.0);
    }

    fn setup_docks_and_barge(w: &World, tech: &Tech, c: &mut Colony, b: &mut Buildings) -> (BuildingId, BargeId) {
        let _d1 = b.place(BuildingKind::Dock, (0, 3), tech, w.t(), c).unwrap();
        let d2 = b.place(BuildingKind::Dock, (12, 3), tech, w.t(), c).unwrap();
        finish_orders(c);
        b.step(0.0, tech, w.t(), c, &Work::default());
        b.place(BuildingKind::Barge, (0, 4), tech, w.t(), c).unwrap();
        finish_orders(c);
        b.step(0.0, tech, w.t(), c, &Work::default());
        (d2, b.barges[0].id)
    }

    #[test]
    fn barge_travels_only_on_connected_water() {
        let mut w = World::new();
        let (tech, mut c) = (all_tech(), Colony::new(1));
        let mut b = Buildings::default();
        rich(&mut b);
        let (d2, barge) = setup_docks_and_barge(&w, &tech, &mut c, &mut b);
        assert!(b.barge(barge).unwrap().at_dock.is_some());
        // dry land across the river at column 7 cuts the route
        w.set_column(7, 0.0);
        let logs = b.stores.amount(Good::Logs);
        assert_eq!(b.dispatch(barge, d2, Some((Good::Logs, 5.0)), w.t()), Err(BuildError::NoWaterRoute));
        assert_eq!(b.stores.amount(Good::Logs), logs, "failed dispatch must not load cargo");
        // reconnect
        w.set_column(7, 0.5);
        b.dispatch(barge, d2, Some((Good::Logs, 5.0)), w.t()).unwrap();
        assert_eq!(b.stores.amount(Good::Logs), logs - 5.0);
        assert_eq!(b.dispatch(barge, d2, None, w.t()), Err(BuildError::BargeBusy));
        for _ in 0..20 {
            b.step(0.25, &tech, w.t(), &c, &Work::default());
        }
        let bg = b.barge(barge).unwrap();
        assert_eq!(bg.state, BargeState::Idle);
        assert_eq!(bg.at_dock, Some(d2));
        assert!(bg.cargo.is_none());
        assert_eq!(b.stores.amount(Good::Logs), logs);
    }

    #[test]
    fn barge_strands_when_water_dries_mid_route() {
        let mut w = World::new();
        let (tech, mut c) = (all_tech(), Colony::new(1));
        let mut b = Buildings::default();
        rich(&mut b);
        let (d2, barge) = setup_docks_and_barge(&w, &tech, &mut c, &mut b);
        b.dispatch(barge, d2, Some((Good::Stone, 3.0)), w.t()).unwrap();
        b.step(0.5, &tech, w.t(), &c, &Work::default());
        w.set_column(9, 0.0);
        for _ in 0..10 {
            b.step(0.5, &tech, w.t(), &c, &Work::default());
        }
        let bg = b.barge(barge).unwrap();
        assert_eq!(bg.state, BargeState::Stranded);
        assert!(bg.cell.0 < 9);
        assert_eq!(bg.cargo, Some((Good::Stone, 3.0)));
    }

    #[test]
    fn barge_needs_dock_and_water() {
        let w = World::new();
        let (tech, mut c) = (all_tech(), Colony::new(1));
        let mut b = Buildings::default();
        rich(&mut b);
        assert_eq!(b.place(BuildingKind::Barge, (0, 4), &tech, w.t(), &mut c), Err(BuildError::NoDock));
        assert!(matches!(b.place(BuildingKind::Barge, (0, 0), &tech, w.t(), &mut c), Err(BuildError::BadTerrain(_))));
    }

    #[test]
    fn stores_reject_invalid_amounts_and_respect_capacity() {
        let mut s = Stores::default();
        assert!(matches!(s.deposit(Good::Logs, f32::NAN), Err(StoresError::InvalidAmount(_))));
        assert!(matches!(s.withdraw(Good::Logs, -1.0), Err(StoresError::InvalidAmount(_))));
        assert_eq!(s.deposit(Good::Logs, 500.0), Ok(BASE_CAPACITY));
        assert!(s.withdraw(Good::Logs, 101.0).is_err());
        assert_eq!(s.amount(Good::Logs), BASE_CAPACITY);
    }
}
