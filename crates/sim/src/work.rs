//! Workplaces, construction sites, worker slots, work areas and hauling (owner: `build`).
//! A `Workplace` is any building with worker slots; a site is a workplace with `site: Some(..)`.
//!
//! **One job model (spec W0).** Work slots are the truth: a beaver's role is the role of the slot
//! it fills (or of its direct gather order, `Command::Assign`), otherwise it is free. In the game
//! (`SimState`, `colony.abstract_gathering == false`) the colony's daily job plan no longer hands
//! out jobs: `SimState::step` gives the colony each beaver's job derived from its slot
//! (`jobs::colony_job`), so `Beaver.job`/`current_job` follow the slots and skills grow from slot
//! work. Free beavers do no abstract work. The abstract jobs with no building slot have no
//! colony-wide effect there: Digger's `dug`, Scout's `scouted`, Guard's strength (Guard and Scout
//! are switched off by `features` anyway) and Crafter's `tools`. Lodge repair, which builders did,
//! is passive (like dam wear): the family mends a failing lodge from the stored logs, booked as
//! `Use::Repairs`. The old lake fixtures keep the abstract plan (`abstract_gathering == true`).
//!
//! **Auto-staffing (spec W1)** lives in `agents::organise`: free working-age beavers fill empty
//! slots of every workplace by building priority, then the role order, then distance; sites also
//! get material gatherers. Hand-placed beavers (`Slot::locked`) and beavers on a direct order
//! (`Work::player_orders`) are never moved; paused workplaces get no workers.

use std::collections::{BTreeMap, BTreeSet};

use crate::jobs::RoleKind;

use crate::buildings::{Amount, BuildingId, BuildingKind, Good};
use crate::colony::BeaverId;
use crate::water::Cell;

/// Builder slots on every construction site.
pub const SITE_SLOTS: usize = 2;
/// Builder work units done per day by one builder (an adult working day is about 0.6).
pub const LABOUR_PER_DAY: f32 = 0.6;
/// Amounts closer than this to a target count as reaching it.
pub const EPS: f32 = 1e-3;

/// Inclusive rectangle of cells: a workplace's gather area or a drag-assigned area.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Area {
    pub min: Cell,
    pub max: Cell,
}

impl Area {
    pub fn contains(&self, c: Cell) -> bool {
        c.0 >= self.min.0 && c.0 <= self.max.0 && c.1 >= self.min.1 && c.1 <= self.max.1
    }
    pub fn centre(&self) -> Cell {
        ((self.min.0 + self.max.0) / 2, (self.min.1 + self.max.1) / 2)
    }
}

/// What a slot's worker does.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Role {
    Collect(crate::resources::ResourceKind),
    Build,
    Haul,
    Craft,
    /// Elders study at an elders' den (`tech::den_lore`).
    Study,
    /// Plants and tends trees (forester's lodge) or grows saplings (tree nursery): `forestry`.
    Forester,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Slot {
    pub role: Role,
    pub worker: Option<BeaverId>,
    /// True when the player placed this worker by hand; auto-staffing never moves them.
    pub locked: bool,
}

impl Slot {
    pub fn empty(role: Role) -> Self {
        Slot { role, worker: None, locked: false }
    }
}

/// A workplace's staffing priority (building priority comes before role priority).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Priority {
    Low,
    #[default]
    Normal,
    High,
}

/// A building (complete or site) viewed as a place of work.
#[derive(Clone, Debug)]
pub struct Workplace {
    pub building: BuildingId,
    pub kind: BuildingKind,
    pub slots: Vec<Slot>,
    /// Where collectors of this workplace may gather; `None` for non-collecting workplaces.
    pub area: Option<Area>,
    /// Sites only.
    pub site: Option<SiteProgress>,
    pub priority: Priority,
    /// A paused workplace has no workers and auto-staffing leaves it empty.
    pub paused: bool,
    /// Most workers auto-staffing puts here (the inspector's "Max"); `RemoveWorker` lowers it and
    /// `AddWorker` raises it, so a beaver the player took out is not sent straight back.
    pub max_workers: usize,
}

impl Workplace {
    /// A workplace with empty `slots` at normal priority, unpaused, allowing every slot.
    pub fn new(building: BuildingId, kind: BuildingKind, slots: Vec<Slot>, area: Option<Area>, site: Option<SiteProgress>) -> Self {
        let max_workers = slots.len();
        Workplace { building, kind, slots, area, site, priority: Priority::Normal, paused: false, max_workers }
    }
    pub fn filled(&self) -> usize {
        self.slots.iter().filter(|s| s.worker.is_some()).count()
    }
    /// A site still being built (materials or labour outstanding).
    pub fn is_open_site(&self) -> bool {
        self.site.as_ref().is_some_and(|s| !s.is_done())
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct SiteProgress {
    pub needed: Vec<Amount>,
    pub delivered: Vec<Amount>,
    pub labour_done: f32,
    pub labour_needed: f32,
}

impl SiteProgress {
    /// Quantity of `good` still to be delivered (0 if the site does not use it).
    pub fn missing(&self, good: Good) -> f32 {
        let sum = |v: &[Amount]| v.iter().filter(|a| a.0 == good).map(|a| a.1).sum::<f32>();
        (sum(&self.needed) - sum(&self.delivered)).max(0.0)
    }
    /// All materials delivered and all labour done.
    pub fn is_done(&self) -> bool {
        self.needed.iter().all(|a| self.missing(a.0) <= 0.0) && self.labour_done >= self.labour_needed
    }
    /// Overall progress `0..=1`: materials and labour weighted equally.
    pub fn fraction(&self) -> f32 {
        let need: f32 = self.needed.iter().map(|a| a.1).sum();
        let got: f32 = self.delivered.iter().map(|a| a.1).sum();
        let mat = if need > 0.0 { (got / need).min(1.0) } else { 1.0 };
        let lab = if self.labour_needed > 0.0 { (self.labour_done / self.labour_needed).min(1.0) } else { 1.0 };
        0.5 * (mat + lab)
    }
}

/// Standing order moving goods between two stores.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HaulOrder {
    pub from: BuildingId,
    pub to: BuildingId,
    pub good: Good,
}

#[derive(Clone, Debug, PartialEq)]
pub enum WorkError {
    NoSuchWorkplace,
    SlotTaken,
    SlotOutOfRange,
    NotAdult,
    InvalidArea,
    NoSuchBeaver,
    /// The workplace is not a construction site.
    NotASite,
    AlreadyRegistered,
    /// A cost, labour or amount was negative, NaN or infinite.
    InvalidAmount,
}

/// All workplaces and haul orders.
#[derive(Clone, Debug)]
pub struct Work {
    pub places: Vec<Workplace>,
    pub hauls: Vec<HaulOrder>,
    /// Goods held in each store, which haulers move between stores. The colony ledger
    /// (`Buildings::stores`) stays the total; hauling only relocates goods between stores.
    pub stock: BTreeMap<(BuildingId, Good), f32>,
    /// Dam sites: the water dam to build when the site completes.
    pub dam_plans: Vec<DamPlan>,
    /// Dams that completed but could not be applied to the water, with the reason (never silent).
    pub dam_failures: Vec<(BuildingId, String)>,
    /// Auto-staffing: when true, free working-age beavers without a player order fill the empty
    /// slots of every workplace and gather the materials sites lack (`agents::organise`). Off for
    /// a bare `Work`; every new game start switches it on.
    pub auto_organise: bool,
    /// What `organise` last set for each beaver it manages (a player's change drops the beaver).
    pub managed: BTreeMap<BeaverId, ManagedDuty>,
    /// Role priority for auto-staffing and the Jobs board, first = highest.
    pub role_order: Vec<RoleKind>,
    /// Beavers on a direct player order (`Command::Assign`): auto-staffing leaves them alone
    /// until `Command::Unlock`.
    pub player_orders: BTreeSet<BeaverId>,
    /// Where the founding family dropped the supplies it carried in (a pile on the ground, not a
    /// building). Builders fetch from it until a store stands; it is gone once one does.
    pub supplies: Option<Cell>,
    /// Canal sites: the canal to cut when the site completes.
    pub canal_plans: Vec<CanalPlan>,
    /// Shelter buildings (burrows, lodges) and the colony home each one provides.
    pub homes: BTreeMap<BuildingId, crate::colony::LodgeId>,
}

impl Default for Work {
    fn default() -> Self {
        Work {
            places: Vec::new(),
            hauls: Vec::new(),
            stock: BTreeMap::new(),
            dam_plans: Vec::new(),
            dam_failures: Vec::new(),
            auto_organise: false,
            managed: BTreeMap::new(),
            role_order: RoleKind::DEFAULT_ORDER.to_vec(),
            player_orders: BTreeSet::new(),
            supplies: None,
            canal_plans: Vec::new(),
            homes: BTreeMap::new(),
        }
    }
}

/// The canal a canal site turns into when it is finished.
#[derive(Clone, Debug, PartialEq)]
pub struct CanalPlan {
    pub building: BuildingId,
    pub cells: Vec<Cell>,
    pub depth: f32,
}

/// The water dam a dam site turns into when it is finished.
#[derive(Clone, Debug, PartialEq)]
pub struct DamPlan {
    pub building: BuildingId,
    pub cells: Vec<Cell>,
    /// Tallest height in the plan (all cells for a level dam).
    pub height: f32,
    /// Height above terrain per cell, parallel to `cells`.
    pub heights: Vec<f32>,
}

/// An automatic assignment made by `agents::organise`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ManagedDuty {
    /// Gathering a material a site lacks (no slot: the site's material run).
    Gather(crate::agents::Assignment),
    /// Slot `.1` of workplace `.0` (any workplace, sites included).
    Slot(BuildingId, usize),
}

impl Work {
    pub fn place(&self, b: BuildingId) -> Option<&Workplace> {
        self.places.iter().find(|p| p.building == b)
    }
    pub(crate) fn place_mut(&mut self, b: BuildingId) -> Result<&mut Workplace, WorkError> {
        self.places.iter_mut().find(|p| p.building == b).ok_or(WorkError::NoSuchWorkplace)
    }
    /// The slot `who` fills, if any.
    pub fn slot_of(&self, who: BeaverId) -> Option<(BuildingId, usize, Role)> {
        self.places.iter().find_map(|p| {
            p.slots.iter().position(|s| s.worker == Some(who)).map(|i| (p.building, i, p.slots[i].role))
        })
    }
    /// Puts `who` into a free slot, leaving any slot they held. Nothing changes on error.
    /// (`SimState::apply` checks that `who` is an adult colony member.)
    pub fn assign(&mut self, b: BuildingId, slot: usize, who: BeaverId) -> Result<(), WorkError> {
        let s = self.place_mut(b)?.slots.get(slot).ok_or(WorkError::SlotOutOfRange)?;
        if s.worker.is_some() {
            return Err(WorkError::SlotTaken);
        }
        self.vacate(who);
        self.place_mut(b)?.slots[slot].worker = Some(who);
        Ok(())
    }
    /// Empties the slot (and clears its lock).
    pub fn unassign(&mut self, b: BuildingId, slot: usize) -> Result<(), WorkError> {
        let s = self.place_mut(b)?.slots.get_mut(slot).ok_or(WorkError::SlotOutOfRange)?;
        s.worker = None;
        s.locked = false;
        Ok(())
    }
    /// Takes `who` out of any slot they fill (clearing its lock).
    pub fn vacate(&mut self, who: BeaverId) {
        for p in &mut self.places {
            for s in p.slots.iter_mut().filter(|s| s.worker == Some(who)) {
                s.worker = None;
                s.locked = false;
            }
        }
    }
    /// Locks or unlocks the slot `who` fills; false if they fill none.
    pub fn set_locked(&mut self, who: BeaverId, locked: bool) -> bool {
        for p in &mut self.places {
            if let Some(s) = p.slots.iter_mut().find(|s| s.worker == Some(who)) {
                s.locked = locked;
                return true;
            }
        }
        false
    }
    /// Whether `who` is locked against auto-staffing (hand-placed or on a direct order).
    pub fn is_locked(&self, who: BeaverId) -> bool {
        self.player_orders.contains(&who) || self.places.iter().any(|p| p.slots.iter().any(|s| s.worker == Some(who) && s.locked))
    }
    /// Rank of `role` in `role_order` (0 = highest; unknown roles last).
    pub fn role_rank(&self, role: RoleKind) -> usize {
        self.role_order.iter().position(|r| *r == role).unwrap_or(self.role_order.len())
    }
    /// Registers construction site `b` needing `needed` goods and `labour` work units, with
    /// `SITE_SLOTS` builder slots.
    pub fn add_site(&mut self, b: BuildingId, kind: BuildingKind, needed: &[Amount], labour: f32) -> Result<(), WorkError> {
        if self.place(b).is_some() {
            return Err(WorkError::AlreadyRegistered);
        }
        let ok = |x: f32| x.is_finite() && x >= 0.0;
        if !ok(labour) || needed.iter().any(|a| !ok(a.1)) {
            return Err(WorkError::InvalidAmount);
        }
        let site = SiteProgress {
            needed: needed.to_vec(),
            delivered: needed.iter().map(|a| (a.0, 0.0)).collect(),
            labour_done: 0.0,
            labour_needed: labour,
        };
        self.places.push(Workplace::new(b, kind, vec![Slot::empty(Role::Build); SITE_SLOTS], None, Some(site)));
        Ok(())
    }
    fn site_mut(&mut self, b: BuildingId) -> Result<&mut SiteProgress, WorkError> {
        self.place_mut(b)?.site.as_mut().ok_or(WorkError::NotASite)
    }
    /// Delivers up to `amount` of `good` to site `b`; returns what it accepted (never more than
    /// is missing, so a site is built from exactly its cost).
    pub fn deliver(&mut self, b: BuildingId, good: Good, amount: f32) -> Result<f32, WorkError> {
        if !amount.is_finite() || amount < 0.0 {
            return Err(WorkError::InvalidAmount);
        }
        let site = self.site_mut(b)?;
        let take = amount.min(site.missing(good));
        if let Some(d) = site.delivered.iter_mut().find(|d| d.0 == good) {
            d.1 += take;
            if site.needed.iter().filter(|n| n.0 == good).map(|n| n.1).sum::<f32>() - d.1 < EPS {
                d.1 = site.needed.iter().filter(|n| n.0 == good).map(|n| n.1).sum();
            }
        }
        Ok(take)
    }
    /// Adds builder labour to site `b` (capped at what is needed).
    pub fn add_labour(&mut self, b: BuildingId, amount: f32) -> Result<(), WorkError> {
        if !amount.is_finite() || amount < 0.0 {
            return Err(WorkError::InvalidAmount);
        }
        let s = self.site_mut(b)?;
        s.labour_done += amount;
        if s.labour_done > s.labour_needed - EPS {
            s.labour_done = s.labour_needed;
        }
        Ok(())
    }
    /// Site progress `0..=1`; `None` if `b` is not a site.
    pub fn progress(&self, b: BuildingId) -> Option<f32> {
        self.place(b)?.site.as_ref().map(SiteProgress::fraction)
    }
    /// Registers the finished building `b` as a workplace with its catalogue slots and area (for
    /// buildings a start places complete). Errors if it is already registered.
    pub fn add_complete(&mut self, b: &crate::buildings::Building) -> Result<(), WorkError> {
        if self.place(b.id).is_some() {
            return Err(WorkError::AlreadyRegistered);
        }
        let slots = crate::catalogue::def(b.kind).slots.iter().map(|&role| Slot::empty(role)).collect();
        self.places.push(Workplace::new(b.id, b.kind, slots, crate::catalogue::work_area(b), None));
        Ok(())
    }
    /// Finished site `b` becomes an ordinary workplace with `slots` and `area`; builders leave
    /// (hand-placed ones too: their lock ends with the site). Priority and pause carry over.
    pub fn finish_site(&mut self, b: BuildingId, slots: &[Role], area: Option<Area>) -> Result<(), WorkError> {
        let p = self.place_mut(b)?;
        let builders: Vec<BeaverId> = p.slots.iter().filter_map(|s| s.worker).collect();
        p.slots = slots.iter().map(|&role| Slot::empty(role)).collect();
        p.area = area;
        p.max_workers = p.slots.len();
        self.managed.retain(|id, _| !builders.contains(id));
        Ok(())
    }
    /// Goods held in store `b`.
    pub fn stock_of(&self, b: BuildingId, good: Good) -> f32 {
        self.stock.get(&(b, good)).copied().unwrap_or(0.0)
    }
    pub fn stock_add(&mut self, b: BuildingId, good: Good, amount: f32) {
        *self.stock.entry((b, good)).or_insert(0.0) += amount;
    }
    /// Takes up to `amount` from store `b`'s stock; returns what was taken.
    pub fn stock_take(&mut self, b: BuildingId, good: Good, amount: f32) -> f32 {
        let have = self.stock_of(b, good);
        let take = amount.min(have).max(0.0);
        self.stock.insert((b, good), have - take);
        take
    }
    /// Moves or resizes a workplace's area (collectors only gather inside the current one).
    pub fn set_area(&mut self, b: BuildingId, area: Area) -> Result<(), WorkError> {
        if area.min.0 > area.max.0 || area.min.1 > area.max.1 {
            return Err(WorkError::InvalidArea);
        }
        self.place_mut(b)?.area = Some(area);
        Ok(())
    }
}
