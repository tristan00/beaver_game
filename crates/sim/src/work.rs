//! Workplaces, construction sites, worker slots, work areas and hauling (owner: `build`).
//! A `Workplace` is any building with worker slots; a site is a workplace with `site: Some(..)`.

use std::collections::BTreeMap;

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
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Slot {
    pub role: Role,
    pub worker: Option<BeaverId>,
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
#[derive(Clone, Debug, Default)]
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
    /// When true, idle working-age beavers without a player order staff sites and gather the
    /// materials they lack (`agents::organise`). Off by default; the starts switch it on.
    pub auto_organise: bool,
    /// What `organise` last set for each beaver it manages (a player's change drops the beaver).
    pub managed: BTreeMap<BeaverId, ManagedDuty>,
}

/// The water dam a dam site turns into when it is finished.
#[derive(Clone, Debug, PartialEq)]
pub struct DamPlan {
    pub building: BuildingId,
    pub cells: Vec<Cell>,
    pub height: f32,
}

/// An automatic assignment made by `agents::organise`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ManagedDuty {
    Gather(crate::agents::Assignment),
    Build(BuildingId, usize),
}

impl Work {
    pub fn place(&self, b: BuildingId) -> Option<&Workplace> {
        self.places.iter().find(|p| p.building == b)
    }
    fn place_mut(&mut self, b: BuildingId) -> Result<&mut Workplace, WorkError> {
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
        for p in &mut self.places {
            p.slots.iter_mut().filter(|s| s.worker == Some(who)).for_each(|s| s.worker = None);
        }
        self.place_mut(b)?.slots[slot].worker = Some(who);
        Ok(())
    }
    pub fn unassign(&mut self, b: BuildingId, slot: usize) -> Result<(), WorkError> {
        self.place_mut(b)?.slots.get_mut(slot).ok_or(WorkError::SlotOutOfRange)?.worker = None;
        Ok(())
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
        self.places.push(Workplace {
            building: b,
            kind,
            slots: vec![Slot { role: Role::Build, worker: None }; SITE_SLOTS],
            area: None,
            site: Some(SiteProgress {
                needed: needed.to_vec(),
                delivered: needed.iter().map(|a| (a.0, 0.0)).collect(),
                labour_done: 0.0,
                labour_needed: labour,
            }),
        });
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
        let slots = crate::catalogue::def(b.kind).slots.iter().map(|&role| Slot { role, worker: None }).collect();
        self.places.push(Workplace { building: b.id, kind: b.kind, slots, area: crate::catalogue::work_area(b), site: None });
        Ok(())
    }
    /// Finished site `b` becomes an ordinary workplace with `slots` and `area`; builders leave.
    pub fn finish_site(&mut self, b: BuildingId, slots: &[Role], area: Option<Area>) -> Result<(), WorkError> {
        let p = self.place_mut(b)?;
        p.slots = slots.iter().map(|&role| Slot { role, worker: None }).collect();
        p.area = area;
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
