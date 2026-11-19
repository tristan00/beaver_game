use crate::colony::{Colony, ColonyError, Job, OrderId};
use crate::grid::Grid;
use crate::tech::Tech;
use crate::work::{Role, Work};
use std::fmt;
pub use crate::tuning::buildings::*;

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

pub type Amount = (Good, f32);

#[derive(Clone, Debug, PartialEq)]
pub enum StoresError {
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

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Producer {
    Building(BuildingId),
    Order,
    Other,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Use {
    EatenAdults,
    EatenYearlings,
    EatenKits,
    Spoiled,
    Sites,
    Crafting,
    Repairs,
    Other,
}

impl Use {
    pub const ALL: [Use; 8] = [
        Use::EatenAdults,
        Use::EatenYearlings,
        Use::EatenKits,
        Use::Spoiled,
        Use::Sites,
        Use::Crafting,
        Use::Repairs,
        Use::Other,
    ];
    pub const COUNT: usize = Self::ALL.len();
    pub fn index(self) -> usize {
        self as usize
    }
    pub fn eaten_by(stage: crate::colony::Stage) -> Use {
        match stage {
            crate::colony::Stage::Kit => Use::EatenKits,
            crate::colony::Stage::Yearling => Use::EatenYearlings,
            crate::colony::Stage::Adult => Use::EatenAdults,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Stores {
    amounts: [f32; N],
    capacity: [f32; N],
    pub produced_total: [f32; N],
    pub consumed_total: [f32; N],
    pub produced_by: std::collections::BTreeMap<(Producer, Good), f32>,
    pub consumed_by: [[f32; N]; Use::COUNT],
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
            produced_by: Default::default(),
            consumed_by: [[0.0; N]; Use::COUNT],
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

    pub fn deposit(&mut self, g: Good, amount: f32) -> Result<f32, StoresError> {
        self.deposit_by(g, amount, Producer::Other)
    }

    pub fn deposit_by(&mut self, g: Good, amount: f32, who: Producer) -> Result<f32, StoresError> {
        let taken = self.put_raw(g, amount)?;
        self.tick_produced[g.index()] += taken;
        self.produced_total[g.index()] += taken;
        if taken > 0.0 {
            *self.produced_by.entry((who, g)).or_insert(0.0) += taken;
        }
        Ok(taken)
    }

    pub fn withdraw(&mut self, g: Good, amount: f32) -> Result<(), StoresError> {
        self.withdraw_for(g, amount, Use::Other)
    }

    pub fn withdraw_for(&mut self, g: Good, amount: f32, use_: Use) -> Result<(), StoresError> {
        self.take_raw(g, amount)?;
        self.tick_consumed[g.index()] += amount;
        self.consumed_total[g.index()] += amount;
        self.consumed_by[use_.index()][g.index()] += amount;
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

    pub fn withdraw_all(&mut self, costs: &[Amount]) -> Result<(), StoresError> {
        self.can_afford(costs)?;
        for &(g, a) in costs {
            self.withdraw_for(g, a, Use::Sites)?;
        }
        Ok(())
    }

    pub fn refund(&mut self, g: Good, amount: f32) -> Result<f32, StoresError> {
        self.put_raw(g, amount)
    }

    pub(crate) fn put_raw(&mut self, g: Good, amount: f32) -> Result<f32, StoresError> {
        check_amount(amount)?;
        let taken = amount.min(self.space(g));
        self.amounts[g.index()] += taken;
        Ok(taken)
    }
    pub(crate) fn take_raw(&mut self, g: Good, amount: f32) -> Result<(), StoresError> {
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
    LoggingCamp,
    ForagerHut,
    MudPit,
    QuarryCamp,
    Mill,
    Storehouse,
    ForesterLodge,
    TreeNursery,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Site {
    Dry,
    TouchesWater,
    WaterByDock,
}

#[derive(Clone, Copy, Debug)]
pub struct Spec {
    pub name: &'static str,
    pub cost: &'static [Amount],
    pub footprint: (usize, usize),
    pub work: f32,
    pub site: Site,
}

impl BuildingKind {
    pub fn needs_water(self) -> bool {
        self.spec().site == Site::TouchesWater && !matches!(self, BuildingKind::Dam | BuildingKind::Canal | BuildingKind::Lock)
    }

    pub const ALL: [BuildingKind; 21] = [
        BuildingKind::Dam, BuildingKind::Lodge, BuildingKind::Burrow, BuildingKind::FoodCache,
        BuildingKind::ScentMound, BuildingKind::GuardPost, BuildingKind::Canal, BuildingKind::Lock,
        BuildingKind::WaterWheel, BuildingKind::Workshop, BuildingKind::Warehouse, BuildingKind::Dock,
        BuildingKind::Barge, BuildingKind::LoggingCamp, BuildingKind::ForagerHut, BuildingKind::MudPit,
        BuildingKind::QuarryCamp, BuildingKind::Mill, BuildingKind::Storehouse,
        BuildingKind::ForesterLodge, BuildingKind::TreeNursery,
    ];

    pub fn spec(self) -> Spec {
        let (name, footprint, site) = match self {
            BuildingKind::Dam => ("Dam", (3, 1), Site::TouchesWater),
            BuildingKind::Lodge => ("Lodge", (3, 3), Site::TouchesWater),
            BuildingKind::Burrow => ("Burrow hideout", (1, 1), Site::TouchesWater),
            BuildingKind::FoodCache => ("Food cache", (2, 2), Site::TouchesWater),
            BuildingKind::ScentMound => ("Scent mound", (1, 1), Site::TouchesWater),
            BuildingKind::GuardPost => ("Guard post", (1, 1), Site::Dry),
            BuildingKind::Canal => ("Canal", (1, 1), Site::TouchesWater),
            BuildingKind::Lock => ("Lock", (2, 1), Site::TouchesWater),
            BuildingKind::WaterWheel => ("Water wheel", (2, 2), Site::TouchesWater),
            BuildingKind::Workshop => ("Workshop", (3, 2), Site::TouchesWater),
            BuildingKind::Warehouse => ("Warehouse", (3, 3), Site::TouchesWater),
            BuildingKind::Dock => ("Dock", (2, 1), Site::TouchesWater),
            BuildingKind::Barge => ("Barge", (1, 1), Site::WaterByDock),
            BuildingKind::LoggingCamp => ("Logging camp", (2, 2), Site::TouchesWater),
            BuildingKind::ForagerHut => ("Forager hut", (2, 2), Site::TouchesWater),
            BuildingKind::MudPit => ("Mud pit", (2, 2), Site::TouchesWater),
            BuildingKind::QuarryCamp => ("Quarry camp", (2, 2), Site::Dry),
            BuildingKind::Mill => ("Mill", (3, 3), Site::TouchesWater),
            BuildingKind::Storehouse => ("Storehouse", (4, 3), Site::TouchesWater),
            BuildingKind::ForesterLodge => ("Forester's lodge", (2, 2), Site::Dry),
            BuildingKind::TreeNursery => ("Tree nursery", (2, 2), Site::Dry),
        };
        let (cost, work) = cost_and_work(self);
        Spec { name, cost, footprint, work, site }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Recipe {
    Baskets,
    Rope,
    Tools,
}

impl Recipe {
    pub fn batch(self) -> (&'static [Amount], Amount) {
        recipe_batch(self)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct BuildingId(pub u64);
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct BargeId(pub u64);

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Status {
    UnderConstruction(OrderId),
    Site,
    Complete,
}

#[derive(Clone, Debug)]
pub struct Building {
    pub id: BuildingId,
    pub kind: BuildingKind,
    pub pos: (usize, usize),
    pub status: Status,
    pub recipe: Option<Recipe>,
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

pub const KEEP_WET_DEPTH: f32 = 0.02;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum BargeState {
    Idle,
    Sailing,
    Stranded,
}

#[derive(Clone, Debug)]
pub struct Barge {
    pub id: BargeId,
    pub cell: (usize, usize),
    pub cargo: Option<Amount>,
    pub state: BargeState,
    pub(crate) route: Vec<(usize, usize)>,
    dest: Option<BuildingId>,
    pub(crate) progress: f32,
    pub at_dock: Option<BuildingId>,
    pub service: Option<crate::shipping::Service>,
    pub leg: crate::shipping::Leg,
    pub wait: f32,
    pub hold: Vec<Amount>,
    pub carried: f32,
}

impl Barge {
    pub fn new(id: BargeId, cell: (usize, usize), at_dock: Option<BuildingId>) -> Self {
        Barge {
            id,
            cell,
            cargo: None,
            state: BargeState::Idle,
            route: vec![],
            dest: None,
            progress: 0.0,
            at_dock,
            service: None,
            leg: Default::default(),
            wait: 0.0,
            hold: Vec::new(),
            carried: 0.0,
        }
    }
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
    pub power: f32,
    pub water_bound: std::collections::BTreeSet<BuildingId>,
    pub crafted: Vec<(BuildingId, Good, f32)>,
    next_id: u64,
}

impl Buildings {
    pub fn stranded(&self, b: &Building, depth: &Grid<f32>) -> bool {
        if !b.is_complete() || !self.water_bound.contains(&b.id) {
            return false;
        }
        let (w, h) = b.kind.spec().footprint;
        let (x0, y0) = (b.pos.0.saturating_sub(1), b.pos.1.saturating_sub(1));
        !(y0..=b.pos.1 + h).any(|y| (x0..=b.pos.0 + w).any(|x| depth.get(x, y).is_some_and(|&d| d >= KEEP_WET_DEPTH)))
    }

    fn bind_water(&mut self, id: BuildingId, kind: BuildingKind) {
        if kind.needs_water() {
            self.water_bound.insert(id);
        }
    }

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

    pub fn footprint(kind: BuildingKind, pos: (usize, usize)) -> Vec<(usize, usize)> {
        Building { id: BuildingId(0), kind, pos, status: Status::Complete, recipe: None, output: 0.0 }.cells().collect()
    }

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

    fn check_ground(&self, kind: BuildingKind, pos: (usize, usize), terrain: Terrain) -> Result<(), BuildError> {
        let spec = kind.spec();
        let cells = Self::footprint(kind, pos);
        if cells.iter().any(|&(x, y)| !terrain.depth.in_bounds(x, y)) {
            return Err(BuildError::OutOfBounds);
        }
        let on_dam = |b: &Building| matches!(kind, BuildingKind::Dam | BuildingKind::Lock) && b.kind == BuildingKind::Dam && b.is_complete();
        if self.buildings.iter().any(|b| !on_dam(b) && b.cells().any(|c| cells.contains(&c))) {
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

    pub fn can_place_site(&self, kind: BuildingKind, pos: (usize, usize), tech: &Tech, terrain: Terrain) -> Result<(), BuildError> {
        self.check_placement(kind, pos, tech, terrain)
    }

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
        self.bind_water(id, kind);
        Ok(id)
    }

    pub fn add_complete(&mut self, kind: BuildingKind, pos: (usize, usize), terrain: Terrain) -> Result<BuildingId, BuildError> {
        self.check_ground(kind, pos, terrain)?;
        let id = BuildingId(self.fresh_id());
        self.buildings.push(Building { id, kind, pos, status: Status::Complete, recipe: None, output: 0.0 });
        self.bind_water(id, kind);
        Ok(id)
    }

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
        self.bind_water(id, kind);
        Ok(id)
    }

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

    pub fn step(&mut self, dt_days: f32, tech: &Tech, terrain: Terrain, colony: &Colony, work: &Work) {
        self.complete_finished(colony, terrain);
        self.update_capacity(tech, terrain);
        self.update_wheels(terrain);
        let crafters = self.crafters(colony, work);
        self.run_workshops(dt_days, tech, &crafters, work);
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
            self.barges.push(Barge::new(id, cell, at_dock));
        }
    }

    pub(crate) fn update_capacity(&mut self, tech: &Tech, terrain: Terrain) {
        let f = tech.storage_factor();
        for g in Good::ALL {
            let stored: f32 = self
                .buildings
                .iter()
                .filter(|b| b.is_complete() && !self.stranded(b, terrain.depth) && crate::agents::accepts(b.kind, g))
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

    fn run_workshops(&mut self, dt: f32, tech: &Tech, crafters: &[(BuildingId, usize)], work: &Work) {
        if dt <= 0.0 {
            return;
        }
        let recipes: Vec<(BuildingId, Recipe, f32)> = crafters
            .iter()
            .filter(|(_, n)| *n > 0)
            .filter_map(|&(id, n)| {
                let b = self.building(id)?;
                Some((id, b.recipe?, crate::catalogue::def(b.kind).batches_per_worker() * n as f32))
            })
            .collect();
        if recipes.is_empty() {
            return;
        }
        let boost = 1.0 + (self.power / recipes.len() as f32).min(WHEEL_MAX_BOOST);
        for (id, r, rate) in recipes {
            let (inputs, (out, per)) = r.batch();
            let mut batches = rate * dt * boost * tech.craft_speed_factor();
            let me = self.building(id).expect("crafting building exists").clone();
            for &(g, a) in inputs {
                let near: f32 = self
                    .buildings
                    .iter()
                    .filter(|b| b.is_complete() && crate::agents::accepts(b.kind, g) && crate::stock::gap(&me, b) <= crate::stock::WORKSHOP_REACH)
                    .map(|b| work.stock_of(b.id, g))
                    .sum();
                let local = if work.stock.is_empty() { self.stores.amount(g) } else { near.min(self.stores.amount(g)) };
                batches = batches.min(local / a);
            }
            batches = batches.min(self.stores.space(out) / per);
            if batches <= 0.0 {
                continue;
            }
            for &(g, a) in inputs {
                self.stores.withdraw_for(g, a * batches, Use::Crafting).expect("limited by availability");
            }
            let made = self.stores.deposit_by(out, per * batches, Producer::Building(id)).expect("amount is finite and >= 0");
            self.crafted.push((id, out, made));
        }
    }

    fn move_barges(&mut self, dt: f32, terrain: Terrain) {
        for b in self.barges.iter_mut().filter(|b| b.service.is_none()) {
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
            if b.state == BargeState::Idle && b.at_dock.is_some() {
                if let Some((g, a)) = b.cargo {
                    let put = self.stores.put_raw(g, a).expect("cargo amount is valid");
                    b.cargo = (a - put > 1e-6).then_some((g, a - put));
                }
            }
        }
    }
}

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
        t.unlock_all();
        assert_eq!(t.unlocked_ids().count(), TECHS.len());
        t
    }

    fn rich(b: &mut Buildings) {
        for g in Good::ALL {
            b.stores.deposit(g, 90.0).unwrap();
        }
    }

    fn finish_orders(c: &mut Colony) {
        c.orders.clear();
    }

    #[test]
    fn building_consumes_stores_and_queues_work() {
        let w = World::new();
        let (tech, mut c) = (all_tech(), Colony::new(1));
        let mut b = Buildings::default();
        let err = b.place(BuildingKind::Lodge, (1, 1), &tech, w.t(), &mut c).unwrap_err();
        assert!(matches!(err, BuildError::Stores(StoresError::Insufficient { .. })));
        assert!(c.orders.is_empty() && b.buildings.is_empty());
        rich(&mut b);
        let before = b.stores.amount(Good::Logs);
        let id = b.place(BuildingKind::Lodge, (1, 1), &tech, w.t(), &mut c).unwrap();
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
        b.place(BuildingKind::ScentMound, (1, 3), &tech, w.t(), &mut c).unwrap();
        b.step(0.5, &tech, w.t(), &c, &Work::default());
        assert_eq!(b.stores.consumption_rate[Good::Mud.index()], 12.0);
        b.step(0.5, &tech, w.t(), &c, &Work::default());
        assert_eq!(b.stores.consumption_rate[Good::Mud.index()], 0.0);
    }

    #[test]
    fn a_water_building_is_stranded_only_once_its_water_is_gone_from_it_and_around_it() {
        let mut w = World::new();
        let (tech, mut c) = (all_tech(), Colony::new(1));
        let mut b = Buildings::default();
        rich(&mut b);
        let id = b.place(BuildingKind::Lodge, (1, 2), &tech, w.t(), &mut c).unwrap();
        finish_orders(&mut c);
        b.step(0.0, &tech, w.t(), &c, &Work::default());
        let lodge = b.building(id).unwrap().clone();
        assert!(lodge.is_complete() && !b.stranded(&lodge, &w.depth));
        for x in 0..16 {
            *w.depth.get_mut(x, 3).unwrap() = 0.0;
            *w.depth.get_mut(x, 4).unwrap() = 0.0;
        }
        *w.depth.get_mut(4, 3).unwrap() = KEEP_WET_DEPTH;
        assert!(!b.stranded(&lodge, &w.depth), "water right beside the footprint keeps it working");
        *w.depth.get_mut(4, 3).unwrap() = 0.0;
        assert!(b.stranded(&lodge, &w.depth), "no water in or around: high and dry");
        let post = b.place(BuildingKind::GuardPost, (8, 0), &tech, w.t(), &mut c).unwrap();
        finish_orders(&mut c);
        b.step(0.0, &tech, w.t(), &c, &Work::default());
        assert!(!b.stranded(b.building(post).unwrap(), &w.depth));
        let pushed = Building { id: BuildingId(999), kind: BuildingKind::Lodge, pos: (10, 0), status: Status::Complete, recipe: None, output: 0.0 };
        assert!(!b.stranded(&pushed, &w.depth));
    }

    #[test]
    fn a_stranded_store_adds_no_room() {
        let mut w = World::new();
        let (tech, mut c) = (all_tech(), Colony::new(1));
        let mut b = Buildings::default();
        rich(&mut b);
        b.place(BuildingKind::Warehouse, (1, 2), &tech, w.t(), &mut c).unwrap();
        finish_orders(&mut c);
        b.step(0.0, &tech, w.t(), &c, &Work::default());
        let wet = b.stores.capacity[Good::Logs.index()];
        w.depth = Grid::new(16, 8, 0.0);
        b.step(0.0, &tech, w.t(), &c, &Work::default());
        assert!(b.stores.capacity[Good::Logs.index()] < wet, "the dry warehouse's room is gone");
        w.set_column(1, 0.5);
        b.step(0.0, &tech, w.t(), &c, &Work::default());
        assert_eq!(b.stores.capacity[Good::Logs.index()], wet, "and comes back with the water");
    }

    #[test]
    fn placement_rules_and_cancel_refund() {
        let w = World::new();
        let (tech, mut c) = (all_tech(), Colony::new(1));
        let mut b = Buildings::default();
        rich(&mut b);
        assert_eq!(b.place(BuildingKind::Lodge, (15, 0), &tech, w.t(), &mut c), Err(BuildError::OutOfBounds));
        assert_eq!(b.place(BuildingKind::Lodge, (1, 0), &tech, w.t(), &mut c), Err(BuildError::BadTerrain("needs water")));
        assert_eq!(b.place(BuildingKind::GuardPost, (1, 3), &tech, w.t(), &mut c), Err(BuildError::BadTerrain("needs dry ground")));
        assert!(matches!(b.place(BuildingKind::Dock, (1, 0), &tech, w.t(), &mut c), Err(BuildError::BadTerrain(_))));
        let id = b.place(BuildingKind::ScentMound, (1, 3), &tech, w.t(), &mut c).unwrap();
        assert_eq!(b.place(BuildingKind::ScentMound, (1, 3), &tech, w.t(), &mut c), Err(BuildError::Blocked));
        let mud = b.stores.amount(Good::Mud);
        b.cancel(id, &mut c).unwrap();
        assert_eq!(b.stores.amount(Good::Mud), mud + 6.0);
        assert!(c.orders.is_empty() && b.buildings.is_empty());
        assert_eq!(
            b.place(BuildingKind::Workshop, (1, 2), &Tech::default(), w.t(), &mut c),
            Err(BuildError::Locked(BuildingKind::Workshop))
        );
    }

    #[test]
    #[ignore = "beavers: off until the water engine is done"]
    fn workshop_crafts_from_stores_and_ledger_tracks_rates() {
        let w = World::new();
        let (tech, mut c) = (all_tech(), Colony::new(1));
        let mut b = Buildings::default();
        rich(&mut b);
        let id = b.place(BuildingKind::Workshop, (1, 2), &tech, w.t(), &mut c).unwrap();
        b.set_recipe(id, Some(Recipe::Baskets)).unwrap();
        finish_orders(&mut c);
        b.step(0.0, &tech, w.t(), &c, &Work::default());
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
        b.step(1.0, &tech, w.t(), &c, &Work::default());
        assert!(b.building(id).unwrap().is_complete());
        assert_eq!(b.building(id).unwrap().output, 0.0);
        assert_eq!(b.power, 0.0);
        for x in 0..16 {
            for y in 3..=4 {
                *w.height.get_mut(x, y).unwrap() = 1.0 - 0.1 * x as f32;
            }
        }
        b.step(1.0, &tech, w.t(), &c, &Work::default());
        assert!(b.building(id).unwrap().output > 0.0 && b.power > 0.0);
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
        w.set_column(7, 0.0);
        let logs = b.stores.amount(Good::Logs);
        assert_eq!(b.dispatch(barge, d2, Some((Good::Logs, 5.0)), w.t()), Err(BuildError::NoWaterRoute));
        assert_eq!(b.stores.amount(Good::Logs), logs, "failed dispatch must not load cargo");
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
