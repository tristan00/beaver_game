use crate::grid::Grid;
pub use crate::inventory::Kind;
use std::fmt;
pub use crate::tuning::buildings::*;

pub use crate::content::{Good, PerId};

impl Good {
    /// The good beavers eat (the one row marked `edible`).
    pub fn food() -> Good { crate::content::content().food }
    /// The good that keeps the tree species it grows into (the one row marked `carries_species`):
    /// what planters plant.
    pub fn seedling() -> Good { crate::content::content().seedling }
}

pub type Amount = (Good, f32);

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
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

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
pub enum Producer {
    Building(BuildingId),
    Order,
    Other,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
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

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Stores {
    pub(crate) inventory: crate::inventory::Inventory,
    capacity: PerId<Good, f32>,
    pub produced_total: PerId<Good, f32>,
    pub consumed_total: PerId<Good, f32>,
    pub produced_by: std::collections::BTreeMap<(Producer, Good), f32>,
    pub consumed_by: [PerId<Good, f32>; Use::COUNT],
    pub production_rate: PerId<Good, f32>,
    pub consumption_rate: PerId<Good, f32>,
    tick_produced: PerId<Good, f32>,
    tick_consumed: PerId<Good, f32>,
}

impl Default for Stores {
    fn default() -> Self {
        Self {
            inventory: Default::default(),
            capacity: PerId::filled(BASE_CAPACITY),
            produced_total: PerId::default(),
            consumed_total: PerId::default(),
            produced_by: Default::default(),
            consumed_by: std::array::from_fn(|_| PerId::default()),
            production_rate: PerId::default(),
            consumption_rate: PerId::default(),
            tick_produced: PerId::default(),
            tick_consumed: PerId::default(),
        }
    }
}

impl Stores {
    /// Book material actually delivered to a site, including freshly gathered loads.
    /// Carried materials are outside stored inventory, so no capacity is involved.
    pub(crate) fn book_site_delivery(&mut self, good: Good, amount: f32, fresh: Option<Producer>) {
        self.book_delivery(good, amount, fresh, Use::Sites);
    }
    /// Book goods left where work uses them (a site, a gate, a building being mended): gone from
    /// the stores, and newly made if they came straight from whoever gathered them.
    pub(crate) fn book_delivery(&mut self, good: Good, amount: f32, fresh: Option<Producer>, use_: Use) {
        if let Some(who) = fresh {
            self.tick_produced[good] += amount;
            self.produced_total[good] += amount;
            *self.produced_by.entry((who,good)).or_insert(0.0) += amount;
        }
        self.tick_consumed[good] += amount;
        self.consumed_total[good] += amount;
        self.consumed_by[use_.index()][good] += amount;
    }
    /// Book carried goods used up away from any store, such as a sapling planted.
    pub(crate) fn book_used(&mut self, good: Good, amount: f32, use_: Use) {
        self.tick_consumed[good] += amount;
        self.consumed_total[good] += amount;
        self.consumed_by[use_.index()][good] += amount;
    }
    pub fn amount(&self, g: Good) -> f32 {
        self.inventory.total(g)
    }
    pub fn capacity(&self, g: Good) -> f32 {
        self.capacity[g]
    }
    pub fn space(&self, g: Good) -> f32 {
        (self.capacity[g] - self.amount(g)).max(0.0)
    }
    pub fn deposit(&mut self, g: Good, amount: f32) -> Result<f32, StoresError> {
        self.deposit_by(g, amount, Producer::Other)
    }

    /// For the session journal: one store transaction and the stock it leaves.
    fn note(&self, op: &str, g: Good, asked: f32, moved: f32, detail: std::fmt::Arguments) {
        if (asked > 0.0 || moved > 0.0) && crate::trace::enabled() {
            crate::trace::event(format!("STORE {op} {} good={g:?} asked={asked:.3} moved={moved:.3} stock={:.2}/{:.0} {detail}", crate::trace::sim_stamp(), self.amount(g), self.capacity(g)));
        }
    }

    pub fn deposit_by(&mut self, g: Good, amount: f32, who: Producer) -> Result<f32, StoresError> {
        let taken = self.put_raw(g, amount)?;
        self.note("DEPOSIT", g, amount, taken, format_args!("by={who:?}"));
        self.tick_produced[g] += taken;
        self.produced_total[g] += taken;
        if taken > 0.0 {
            *self.produced_by.entry((who, g)).or_insert(0.0) += taken;
        }
        Ok(taken)
    }

    pub fn withdraw_for(&mut self, g: Good, amount: f32, use_: Use) -> Result<(), StoresError> {
        let r = self.take_raw(g, amount);
        self.note("WITHDRAW", g, amount, if r.is_ok() { amount } else { 0.0 }, format_args!("use={use_:?} result={r:?}"));
        r?;
        self.tick_consumed[g] += amount;
        self.consumed_total[g] += amount;
        self.consumed_by[use_.index()][g] += amount;
        Ok(())
    }

    pub fn refund(&mut self, g: Good, amount: f32) -> Result<f32, StoresError> {
        let r = self.put_raw(g, amount);
        self.note("REFUND", g, amount, *r.as_ref().unwrap_or(&0.0), format_args!(""));
        r
    }

    pub(crate) fn put_raw(&mut self, g: Good, amount: f32) -> Result<f32, StoresError> {
        check_amount(amount)?;
        let taken = amount.min(self.space(g));
        self.inventory.put(crate::agents::SUPPLIES, g, None, taken);
        Ok(taken)
    }
    pub(crate) fn take_raw(&mut self, g: Good, amount: f32) -> Result<(), StoresError> {
        check_amount(amount)?;
        let have = self.amount(g);
        if have < amount {
            return Err(StoresError::Insufficient { good: g, needed: amount, have });
        }
        self.inventory.consume(g, amount);
        Ok(())
    }

    /// Transfer from a physical store into a carried load. This is not consumption.
    pub fn take_from(&mut self, owner: BuildingId, good: Good, amount: f32) -> f32 {
        let got = self.inventory.take(owner, good, amount);
        self.note("TAKE", good, amount, got, format_args!("from={}", owner.0));
        got
    }
    /// Take a load of one kind, such as saplings of one species, and say which kind.
    pub fn take_kind_from(&mut self, owner: BuildingId, good: Good, amount: f32) -> (f32, Kind) {
        let got = self.inventory.take_kind(owner, good, amount);
        self.note("TAKE", good, amount, got.0, format_args!("from={} kind={:?}", owner.0, got.1));
        got
    }
    pub fn put_kind_at(&mut self, owner: BuildingId, good: Good, kind: Kind, amount: f32) -> Result<f32, StoresError> {
        check_amount(amount)?;
        let taken = amount.min(self.space(good));
        self.inventory.put(owner, good, kind, taken);
        self.note("PUT", good, amount, taken, format_args!("into={} kind={kind:?}", owner.0));
        Ok(taken)
    }
    pub fn produce_kind_at(&mut self, owner: BuildingId, good: Good, kind: Kind, amount: f32, who: Producer) -> Result<f32, StoresError> {
        let taken = self.put_kind_at(owner, good, kind, amount)?;
        self.note("PRODUCED", good, amount, taken, format_args!("into={} by={who:?}", owner.0));
        self.tick_produced[good] += taken;
        self.produced_total[good] += taken;
        *self.produced_by.entry((who, good)).or_default() += taken;
        Ok(taken)
    }

    fn end_tick(&mut self, dt: f32) {
        if dt > 0.0 {
            for g in Good::all() {
                self.production_rate[g] = self.tick_produced[g] / dt;
                self.consumption_rate[g] = self.tick_consumed[g] / dt;
            }
        }
        self.tick_produced = PerId::default();
        self.tick_consumed = PerId::default();
    }
}

pub use crate::content::BuildingKind;

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Site {
    /// Part of the footprint stands in water at least `WET_DEPTH` deep.
    TouchesWater,
}

impl BuildingKind {
    pub fn needs_water(self) -> bool {
        self.spec().site == Site::TouchesWater && !self.is_waterworks()
    }

    /// Dams and canals: drawn on the map and built a cell at a time, not dropped as a site.
    pub fn is_waterworks(self) -> bool {
        self.is_dam() || self.is_canal()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
pub struct BuildingId(pub u64);

#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum Status {
    Site,
    Complete,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Building {
    pub id: BuildingId,
    pub kind: BuildingKind,
    pub pos: (usize, usize),
    pub status: Status,
    pub output: f32,
    pub condition: f32,
    pub reinforced: bool,
    /// A dam owns its cells; every other building is its footprint.
    pub dam: Option<Box<crate::dams::Dam>>,
    /// A canal owns its cells, like a dam.
    pub canal: Option<Box<crate::canals::Canal>>,
}

impl Building {
    pub fn operational(&self) -> bool { self.is_complete() && self.condition > MIN_OPERATIONAL_CONDITION }
    pub fn durability(&self) -> f32 {
        if self.reinforced { crate::maintenance::lasts(self.kind, crate::maintenance::Refit::Reinforce) } else { 1.0 }
    }
    pub fn is_complete(&self) -> bool {
        self.status == Status::Complete
    }
    pub fn cells(&self) -> impl Iterator<Item = (usize, usize)> + use<> {
        let mut dam: Vec<(usize, usize)> = self.dam.as_ref().map(|d| d.cells.iter().map(|c| c.cell).collect()).unwrap_or_default();
        dam.extend(self.canal.as_ref().map(|k| k.cells.iter().map(|c| c.cell).collect::<Vec<_>>()).unwrap_or_default());
        let (w, h) = if self.dam.is_some() || self.canal.is_some() { (0, 0) } else { self.kind.spec().footprint };
        let (x, y) = self.pos;
        (0..h).flat_map(move |dy| (0..w).map(move |dx| (x + dx, y + dy))).chain(dam)
    }
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum BuildError {
    SupplyPile,
    Unexplored,
    OutOfBounds,
    Blocked,
    BadTerrain(&'static str),
    Stores(StoresError),
    UnknownBuilding(BuildingId),
    /// Not unlocked yet: needs a finished building of this kind first.
    Locked(BuildingKind),
}

impl fmt::Display for BuildError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SupplyPile => write!(f, "Blocked: the supply pile is there"),
            Self::Unexplored => write!(f, "Unexplored: the colony has not seen that ground"),
            Self::Locked(k) => write!(f, "Needs a finished {} first", k.spec().name.to_lowercase()),
            _ => write!(f, "{self:?}"),
        }
    }
}
impl std::error::Error for BuildError {}
impl From<StoresError> for BuildError {
    fn from(e: StoresError) -> Self {
        BuildError::Stores(e)
    }
}
#[derive(Clone, Copy)]
pub struct Terrain<'a> {
    pub depth: &'a Grid<f32>,
}

impl Terrain<'_> {
    fn wet(&self, x: usize, y: usize) -> bool {
        self.depth.get(x, y).is_some_and(|&d| d >= WET_DEPTH)
    }
}

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct Buildings {
    pub stores: Stores,
    pub buildings: Vec<Building>,
    pub water_bound: std::collections::BTreeSet<BuildingId>,
    /// Dams and buildings faced with pebbles.
    pub pebbled: std::collections::BTreeSet<BuildingId>,
    /// Buildings that wore to nothing and fell apart, for the journal. Not saved.
    #[serde(skip)]
    pub wrecked: std::collections::BTreeSet<BuildingId>,
    next_id: u64,
}

/// The mean water depth over a building's footprint (m); none for dams and canals.
pub fn flood_depth(b: &Building, depth: &Grid<f32>) -> f32 {
    footprint_depth(b.kind, b.pos, |x, y| depth.get(x, y).copied().unwrap_or(0.0))
}

/// The mean depth over a footprint, the depth of each cell read from `at`.
pub fn footprint_depth(kind: BuildingKind, pos: (usize, usize), at: impl Fn(usize, usize) -> f32) -> f32 {
    if kind.is_waterworks() {
        return 0.0;
    }
    let (w, h) = kind.spec().footprint;
    let n = (w * h).max(1) as f32;
    (0..h).flat_map(|dy| (0..w).map(move |dx| (pos.0 + dx, pos.1 + dy))).map(|(x, y)| at(x, y).max(0.0)).sum::<f32>() / n
}

/// Beaver buildings stand in shallow water, not deep: past `MAX_BUILDING_DEPTH` a finished
/// one cannot be used and wears away fast.
pub fn flooded(b: &Building, depth: &Grid<f32>) -> bool {
    b.is_complete() && !b.kind.is_waterworks() && flood_depth(b, depth) > MAX_BUILDING_DEPTH
}

impl Buildings {
    /// Out of use: high and dry, or too deep under water.
    pub fn unusable(&self, b: &Building, depth: &Grid<f32>) -> bool {
        self.stranded(b, depth) || flooded(b, depth)
    }

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
    pub(crate) fn fresh_id(&mut self) -> u64 {
        self.next_id += 1;
        self.next_id
    }

    pub fn footprint(kind: BuildingKind, pos: (usize, usize)) -> Vec<(usize, usize)> {
        Building { id: BuildingId(0), kind, pos, status: Status::Complete, output: 0.0, condition: 1.0, reinforced: false, dam: None, canal: None }.cells().collect()
    }

    fn check_ground(&self, kind: BuildingKind, pos: (usize, usize), terrain: Terrain) -> Result<(), BuildError> {
        let spec = kind.spec();
        let cells = Self::footprint(kind, pos);
        if cells.iter().any(|&(x, y)| !terrain.depth.in_bounds(x, y)) {
            return Err(BuildError::OutOfBounds);
        }
        if self.buildings.iter().any(|b| b.cells().any(|c| cells.contains(&c))) {
            return Err(BuildError::Blocked);
        }
        // A lodge's door is one cell outside its 3x3 footprint. Reserve it from
        // the moment the site is placed, in both placement orders. Dams may
        // share it; an interior of another lodge may not.
        let solid = |k: BuildingKind| !k.is_waterworks();
        // Keep a continuous access lane around occupied structures. Reserving
        // only a lodge door still allowed later buildings to enclose its whole
        // approach, trapping residents and deliveries in a one-cell pocket.
        if solid(kind) && self.buildings.iter().filter(|b|solid(b.kind)).any(|b|
            b.cells().any(|p|cells.iter().any(|c|p.0.abs_diff(c.0).max(p.1.abs_diff(c.1))<=1))) {
            return Err(BuildError::BadTerrain("leave one cell between buildings for access"));
        }
        if solid(kind) && self.buildings.iter().any(|b| b.kind.is_home() && cells.contains(&crate::nav::lodge_entrance(b.pos))) {
            return Err(BuildError::Blocked);
        }
        if kind.is_home() {
            let door = crate::nav::lodge_entrance(pos);
            if !terrain.depth.in_bounds(door.0, door.1) { return Err(BuildError::OutOfBounds); }
            if self.buildings.iter().any(|b| solid(b.kind) && b.cells().any(|c| c == door)) {
                return Err(BuildError::Blocked);
            }
        }
        if solid(kind) {
            let neighbours = |(x, y): (usize, usize)| {
                [x.checked_sub(1).map(|x| (x, y)), y.checked_sub(1).map(|y| (x, y)),
                    Some((x + 1, y)), Some((x, y + 1))].into_iter().flatten()
            };
            let open = |c: (usize, usize)| terrain.depth.in_bounds(c.0, c.1)
                && !cells.contains(&c)
                && !self.buildings.iter().any(|b| solid(b.kind) && b.cells().any(|p| p == c));
            // Reserve an approach to homes as well as the doorway itself.
            // Otherwise a final building can seal a burrow's only access cell
            // or surround a lodge entrance while meeting footprint checks.
            for (home, at) in self.buildings.iter().map(|b| (b.kind, b.pos)).chain(std::iter::once((kind, pos))) {
                let access = match home {
                    h if h.is_home() => {
                        let door = crate::nav::lodge_entrance(at);
                        if !cells.iter().any(|c| c.0.abs_diff(door.0) + c.1.abs_diff(door.1) <= 1) { continue; }
                        open(door) && neighbours(door).any(open)
                    }
                    _ => continue,
                };
                if !access { return Err(BuildError::BadTerrain("would block access to a home")); }
            }
        }
        match spec.site {
            Site::TouchesWater if !cells.iter().any(|&(x, y)| terrain.wet(x, y)) => {
                return Err(BuildError::BadTerrain("needs water"));
            }
            _ => {}
        }
        if solid(kind) && footprint_depth(kind, pos, |x, y| terrain.depth.get(x, y).copied().unwrap_or(0.0)) > MAX_BUILDING_DEPTH {
            return Err(BuildError::BadTerrain("too deep"));
        }
        Ok(())
    }

    pub fn can_place_site(&self, kind: BuildingKind, pos: (usize, usize), terrain: Terrain) -> Result<(), BuildError> {
        self.check_ground(kind, pos, terrain)
    }

    pub fn place_site(
        &mut self,
        kind: BuildingKind,
        pos: (usize, usize),
        terrain: Terrain,
    ) -> Result<BuildingId, BuildError> {
        self.check_ground(kind, pos, terrain)?;
        let id = BuildingId(self.fresh_id());
        self.buildings.push(Building { id, kind, pos, status: Status::Site, output: 0.0, condition: 1.0, reinforced: false, dam: None, canal: None });
        self.bind_water(id, kind);
        Ok(id)
    }

    pub fn step(&mut self, dt_days: f32, terrain: Terrain) {
        self.update_capacity(terrain);
        self.stores.end_tick(dt_days);
    }

    pub(crate) fn update_capacity(&mut self, terrain: Terrain) {
        let f = 1.0;
        for g in Good::all() {
            let stored: f32 = self
                .buildings
                .iter()
                .filter(|b| b.is_complete() && !self.unusable(b, terrain.depth) && crate::agents::accepts(b.kind, g))
                .map(|b| b.kind.spec().capacity)
                .sum();
            self.stores.capacity[g] = (BASE_CAPACITY + stored) * f;
        }
    }

}
