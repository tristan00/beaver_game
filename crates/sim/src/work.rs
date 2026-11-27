include!(concat!(env!("OUT_DIR"), "/work_constants.rs"));
use std::collections::{BTreeMap, BTreeSet};

use crate::buildings::{Amount, BuildingId, BuildingKind, Good};
use crate::colony::BeaverId;
use crate::jobs::Job;
use crate::water::Cell;

pub const EPS: f32 = 1e-3;

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Area {
    pub min: Cell,
    pub max: Cell,
}

impl Area {
    pub fn contains(&self, c: Cell) -> bool {
        c.0 >= self.min.0 && c.0 <= self.max.0 && c.1 >= self.min.1 && c.1 <= self.max.1
    }
    pub fn reaches(&self, c: Cell) -> bool {
        let (rx, ry) = ((self.max.0 - self.min.0) as f32 / 2.0 + 0.5, (self.max.1 - self.min.1) as f32 / 2.0 + 0.5);
        let (cx, cy) = ((self.min.0 + self.max.0) as f32 / 2.0, (self.min.1 + self.max.1) as f32 / 2.0);
        let (dx, dy) = ((c.0 as f32 - cx) / rx, (c.1 as f32 - cy) / ry);
        dx * dx + dy * dy <= 1.0
    }
    pub fn centre(&self) -> Cell {
        ((self.min.0 + self.max.0) / 2, (self.min.1 + self.max.1) / 2)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Slot {
    pub role: Job,
    pub beaver: Option<BeaverId>,
    pub off: bool,
}

impl Slot {
    pub fn empty(role: Job) -> Self {
        Slot { role, beaver: None, off: false }
    }
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Workplace {
    pub building: BuildingId,
    pub kind: BuildingKind,
    pub slots: Vec<Slot>,
    pub area: Option<Area>,
    pub site: Option<SiteProgress>,
    pub paused: bool,
}

impl Workplace {
    pub fn new(building: BuildingId, kind: BuildingKind, slots: Vec<Slot>, area: Option<Area>, site: Option<SiteProgress>) -> Self {
        Workplace { building, kind, slots, area, site, paused: false }
    }
    pub fn filled(&self) -> usize {
        self.slots.iter().filter(|s| s.beaver.is_some()).count()
    }
    pub fn is_open_site(&self) -> bool {
        self.site.as_ref().is_some_and(|s| !s.is_done())
    }
    pub fn open_seat(&self, want: impl Fn(Job) -> bool) -> Option<(usize, Job)> {
        let i = self.slots.iter().position(|s| s.beaver.is_none() && !s.off && want(s.role))?;
        Some((i, self.slots[i].role))
    }
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct SiteProgress {
    pub needed: Vec<Amount>,
    pub delivered: Vec<Amount>,
    pub labour_done: f32,
    pub labour_needed: f32,
}

impl SiteProgress {
    pub fn new(needed: &[Amount], labour: f32) -> Result<Self, WorkError> {
        let ok = |x: f32| x.is_finite() && x >= 0.0;
        if !ok(labour) || needed.iter().any(|a| !ok(a.1)) {
            return Err(WorkError::InvalidAmount);
        }
        let needed: Vec<Amount> = Good::all().filter_map(|g| {
            let total: f32 = needed.iter().filter(|a| a.0 == g).map(|a| a.1).sum();
            (total > 0.0).then_some((g, total))
        }).collect();
        Ok(SiteProgress {
            delivered: needed.iter().map(|a| (a.0, 0.0)).collect(),
            needed,
            labour_done: 0.0,
            labour_needed: labour,
        })
    }
    pub fn missing(&self, good: Good) -> f32 {
        let sum = |v: &[Amount]| v.iter().filter(|a| a.0 == good).map(|a| a.1).sum::<f32>();
        (sum(&self.needed) - sum(&self.delivered)).max(0.0)
    }
    pub fn is_done(&self) -> bool {
        self.needed.iter().all(|a| self.missing(a.0) <= EPS) && self.labour_done >= self.labour_needed - EPS
    }
    pub fn material_fraction(&self) -> f32 {
        let need: f32 = self.needed.iter().map(|a| a.1).sum();
        let got: f32 = self.delivered.iter().map(|a| a.1).sum();
        if need > 0.0 { (got / need).min(1.0) } else { 1.0 }
    }
    pub fn labour_open(&self) -> bool {
        self.labour_done < self.labour_needed * self.material_fraction() - EPS
    }
    pub fn fraction(&self) -> f32 {
        let need: f32 = self.needed.iter().map(|a| a.1).sum();
        let got: f32 = self.delivered.iter().map(|a| a.1).sum();
        let mat = if need > 0.0 { (got / need).min(1.0) } else { 1.0 };
        let lab = if self.labour_needed > 0.0 { (self.labour_done / self.labour_needed).min(1.0) } else { 1.0 };
        0.5 * (mat + lab)
    }
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum WorkError {
    NoSuchWorkplace,
    SlotTaken,
    SlotOutOfRange,
    NotASite,
    AlreadyRegistered,
    InvalidAmount,
}

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct Work {
    pub places: Vec<Workplace>,
    pub balanced: BTreeSet<BuildingId>,
    pub fills: BTreeSet<BuildingId>,
    pub supplies: Option<Cell>,
    pub piles: crate::piles::Piles,
    pub shore_limits: BTreeMap<crate::jobs::Job, u16>,
}

impl Work {
    pub fn place(&self, b: BuildingId) -> Option<&Workplace> {
        self.places.iter().find(|p| p.building == b)
    }
    pub(crate) fn place_mut(&mut self, b: BuildingId) -> Result<&mut Workplace, WorkError> {
        self.places.iter_mut().find(|p| p.building == b).ok_or(WorkError::NoSuchWorkplace)
    }
    pub fn slot_of(&self, who: BeaverId) -> Option<(BuildingId, usize, Job)> {
        self.places.iter().find_map(|p| {
            p.slots.iter().position(|s| s.beaver == Some(who)).map(|i| (p.building, i, p.slots[i].role))
        })
    }
    pub fn assign(&mut self, b: BuildingId, slot: usize, who: BeaverId) -> Result<(), WorkError> {
        let s = self.place_mut(b)?.slots.get(slot).ok_or(WorkError::SlotOutOfRange)?;
        if s.beaver.is_some() {
            return Err(WorkError::SlotTaken);
        }
        self.vacate(who);
        self.place_mut(b)?.slots[slot].beaver = Some(who);
        Ok(())
    }
    pub fn unassign(&mut self, b: BuildingId, slot: usize) -> Result<(), WorkError> {
        let s = self.place_mut(b)?.slots.get_mut(slot).ok_or(WorkError::SlotOutOfRange)?;
        s.beaver = None;
        Ok(())
    }
    pub fn vacate(&mut self, who: BeaverId) {
        for p in &mut self.places {
            for s in p.slots.iter_mut().filter(|s| s.beaver == Some(who)) {
                s.beaver = None;
            }
        }
    }
    pub fn add_site(&mut self, b: BuildingId, kind: BuildingKind, needed: &[Amount], labour: f32) -> Result<(), WorkError> {
        if self.place(b).is_some() {
            return Err(WorkError::AlreadyRegistered);
        }
        let site = SiteProgress::new(needed, labour)?;
        self.places.push(Workplace::new(b, kind, Vec::new(), None, Some(site)));
        Ok(())
    }
    pub fn start_site(&mut self, b: BuildingId, needed: &[Amount], labour: f32) -> Result<(), WorkError> {
        let site = SiteProgress::new(needed, labour)?;
        self.place_mut(b)?.site = Some(site);
        Ok(())
    }
    pub fn clear_site(&mut self, b: BuildingId) {
        if let Ok(p) = self.place_mut(b) { p.site = None; }
    }
    fn site_mut(&mut self, b: BuildingId) -> Result<&mut SiteProgress, WorkError> {
        self.place_mut(b)?.site.as_mut().ok_or(WorkError::NotASite)
    }
    pub fn deliver(&mut self, b: BuildingId, good: Good, amount: f32) -> Result<f32, WorkError> {
        if !amount.is_finite() || amount < 0.0 {
            return Err(WorkError::InvalidAmount);
        }
        let site = self.site_mut(b)?;
        let take = amount.min(site.missing(good));
        if let Some(d) = site.delivered.iter_mut().find(|d| d.0 == good) {
            d.1 += take;
            let need: f32 = site.needed.iter().filter(|n| n.0 == good).map(|n| n.1).sum();
            if need - d.1 < EPS {
                d.1 = need;
            }
        }
        Ok(take)
    }
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
    pub fn progress(&self, b: BuildingId) -> Option<f32> {
        self.place(b)?.site.as_ref().map(SiteProgress::fraction)
    }
    pub fn add_complete(&mut self, b: &crate::buildings::Building) -> Result<(), WorkError> {
        if self.place(b.id).is_some() {
            return Err(WorkError::AlreadyRegistered);
        }
        let slots = b.kind.spec().slots.iter().map(|&role| Slot::empty(role)).collect();
        self.places.push(Workplace::new(b.id, b.kind, slots, b.work_area(), None));
        self.balance_from_the_start(b.id, b.kind);
        Ok(())
    }
    pub fn finish_site(&mut self, b: BuildingId, slots: &[Job], area: Option<Area>) -> Result<(), WorkError> {
        let p = self.place_mut(b)?;
        p.slots = slots.iter().map(|&role| Slot::empty(role)).collect();
        p.site = None;
        p.area = area;
        let kind = p.kind;
        self.balance_from_the_start(b, kind);
        Ok(())
    }
    fn balance_from_the_start(&mut self, b: BuildingId, kind: BuildingKind) {
        if kind.balances() {
            self.balanced.insert(b);
        }
    }
}
