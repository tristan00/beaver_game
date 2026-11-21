include!(concat!(env!("OUT_DIR"), "/work_constants.rs"));
use std::collections::{BTreeMap, BTreeSet};

use crate::jobs::RoleKind;

use crate::buildings::{Amount, BuildingId, BuildingKind, Good};
use crate::colony::BeaverId;
use crate::water::Cell;

pub const EPS: f32 = 1e-3;

#[cfg(test)]
mod lifecycle_tests {
    use super::*;
    #[test]
    fn combined_material_costs_require_the_full_delivery() {
        let mut w=Work::default();let id=BuildingId(1);
        w.add_site(id,BuildingKind::Dam,&[(Good::Sticks,2.0),(Good::Sticks,3.0),(Good::Clay,1.0)],1.0).unwrap();
        assert_eq!(w.place(id).unwrap().site.as_ref().unwrap().needed.len(),2);
        w.deliver(id,Good::Sticks,3.0).unwrap();
        assert_eq!(w.place(id).unwrap().site.as_ref().unwrap().missing(Good::Sticks),2.0);
    }
    #[test]
    fn finishing_construction_releases_delivery_site() {
        let mut w=Work::default();let id=BuildingId(1);
        w.add_site(id,BuildingKind::Warehouse,&[],1.0).unwrap();
        w.add_labour(id,1.0).unwrap();w.finish_site(id,&[Role::Haul],None).unwrap();
        assert!(w.place(id).unwrap().site.is_none());
        assert_eq!(w.place(id).unwrap().slots[0].role,Role::Haul);
    }
    #[test]
    fn completion_uses_the_same_tolerance_as_worker_tasks() {
        let s=SiteProgress{needed:vec![(Good::Sticks,2.3)],delivered:vec![(Good::Sticks,2.2999)],labour_done:1.4999,labour_needed:1.5};
        assert!(s.is_done());assert!(!s.labour_open());
    }
}

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

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Role {
    Collect(crate::resources::ResourceKind),
    Build,
    Haul,
    Craft,
    Forester,
    Guard,
    Scout,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Slot {
    pub role: Role,
    pub worker: Option<BeaverId>,
    pub locked: bool,
}

impl Slot {
    pub fn empty(role: Role) -> Self {
        Slot { role, worker: None, locked: false }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Priority {
    Low,
    #[default]
    Normal,
    High,
}

#[derive(Clone, Debug)]
pub struct Workplace {
    pub building: BuildingId,
    pub kind: BuildingKind,
    pub slots: Vec<Slot>,
    pub area: Option<Area>,
    pub site: Option<SiteProgress>,
    pub priority: Priority,
    pub paused: bool,
    pub max_workers: usize,
}

impl Workplace {
    pub fn new(building: BuildingId, kind: BuildingKind, slots: Vec<Slot>, area: Option<Area>, site: Option<SiteProgress>) -> Self {
        let max_workers = slots.len();
        Workplace { building, kind, slots, area, site, priority: Priority::Normal, paused: false, max_workers }
    }
    pub fn filled(&self) -> usize {
        self.slots.iter().filter(|s| s.worker.is_some()).count()
    }
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
    pub fn missing(&self, good: Good) -> f32 {
        let sum = |v: &[Amount]| v.iter().filter(|a| a.0 == good).map(|a| a.1).sum::<f32>();
        (sum(&self.needed) - sum(&self.delivered)).max(0.0)
    }
    pub fn is_done(&self) -> bool {
        self.needed.iter().all(|a| self.missing(a.0) <= EPS) && self.labour_done >= self.labour_needed-EPS
    }
    pub fn material_fraction(&self) -> f32 {
        let need: f32 = self.needed.iter().map(|a| a.1).sum();
        let got: f32 = self.delivered.iter().map(|a| a.1).sum();
        if need > 0.0 { (got / need).min(1.0) } else { 1.0 }
    }
    pub fn labour_fraction(&self) -> f32 {
        if self.labour_needed > 0.0 { (self.labour_done / self.labour_needed).min(1.0) } else { 1.0 }
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
    NotASite,
    AlreadyRegistered,
    InvalidAmount,
}

#[derive(Clone, Debug)]
pub struct Work {
    pub places: Vec<Workplace>,
    pub hauls: Vec<HaulOrder>,
    pub balanced: BTreeSet<BuildingId>,
    pub(crate) inventory: crate::inventory::Inventory,
    pub dam_plans: Vec<DamPlan>,
    pub dam_failures: Vec<(BuildingId, String)>,
    pub auto_organise: bool,
    pub managed: BTreeMap<BeaverId, ManagedDuty>,
    pub role_order: Vec<RoleKind>,
    pub role_weights: [f32; 9],
    pub player_orders: BTreeSet<BeaverId>,
    pub supplies: Option<Cell>,
    pub canal_plans: Vec<CanalPlan>,
    pub homes: BTreeMap<BuildingId, crate::colony::LodgeId>,
}

impl Default for Work {
    fn default() -> Self {
        Work {
            places: Vec::new(),
            hauls: Vec::new(),
            balanced: BTreeSet::new(),
            inventory: Default::default(),
            dam_plans: Vec::new(),
            dam_failures: Vec::new(),
            auto_organise: false,
            managed: BTreeMap::new(),
            role_order: RoleKind::DEFAULT_ORDER.to_vec(),
            role_weights: DEFAULT_ROLE_WEIGHTS,
            player_orders: BTreeSet::new(),
            supplies: None,
            canal_plans: Vec::new(),
            homes: BTreeMap::new(),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct CanalPlan {
    pub building: BuildingId,
    pub cells: Vec<Cell>,
    pub depth: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DamPlan {
    pub building: BuildingId,
    /// Reuses a completed dam building; cancelling keeps that building.
    pub repair: bool,
    pub cells: Vec<Cell>,
    pub height: f32,
    pub heights: Vec<f32>,
    pub from: Vec<f32>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ManagedDuty {
    Gather(crate::agents::Assignment),
    Slot(BuildingId, usize),
}

impl Work {
    pub fn place(&self, b: BuildingId) -> Option<&Workplace> {
        self.places.iter().find(|p| p.building == b)
    }
    pub(crate) fn place_mut(&mut self, b: BuildingId) -> Result<&mut Workplace, WorkError> {
        self.places.iter_mut().find(|p| p.building == b).ok_or(WorkError::NoSuchWorkplace)
    }
    pub fn slot_of(&self, who: BeaverId) -> Option<(BuildingId, usize, Role)> {
        self.places.iter().find_map(|p| {
            p.slots.iter().position(|s| s.worker == Some(who)).map(|i| (p.building, i, p.slots[i].role))
        })
    }
    pub fn assign(&mut self, b: BuildingId, slot: usize, who: BeaverId) -> Result<(), WorkError> {
        let s = self.place_mut(b)?.slots.get(slot).ok_or(WorkError::SlotOutOfRange)?;
        if s.worker.is_some() {
            return Err(WorkError::SlotTaken);
        }
        self.vacate(who);
        self.place_mut(b)?.slots[slot].worker = Some(who);
        Ok(())
    }
    pub fn unassign(&mut self, b: BuildingId, slot: usize) -> Result<(), WorkError> {
        let s = self.place_mut(b)?.slots.get_mut(slot).ok_or(WorkError::SlotOutOfRange)?;
        s.worker = None;
        s.locked = false;
        Ok(())
    }
    pub fn vacate(&mut self, who: BeaverId) {
        for p in &mut self.places {
            for s in p.slots.iter_mut().filter(|s| s.worker == Some(who)) {
                s.worker = None;
                s.locked = false;
            }
        }
    }
    pub fn set_locked(&mut self, who: BeaverId, locked: bool) -> bool {
        for p in &mut self.places {
            if let Some(s) = p.slots.iter_mut().find(|s| s.worker == Some(who)) {
                s.locked = locked;
                return true;
            }
        }
        false
    }
    pub fn is_locked(&self, who: BeaverId) -> bool {
        self.player_orders.contains(&who) || self.places.iter().any(|p| p.slots.iter().any(|s| s.worker == Some(who) && s.locked))
    }
    pub fn role_rank(&self, role: RoleKind) -> usize {
        self.role_order.iter().position(|r| *r == role).unwrap_or(self.role_order.len())
    }
    pub fn add_site(&mut self, b: BuildingId, kind: BuildingKind, needed: &[Amount], labour: f32) -> Result<(), WorkError> {
        if self.place(b).is_some() {
            return Err(WorkError::AlreadyRegistered);
        }
        let ok = |x: f32| x.is_finite() && x >= 0.0;
        if !ok(labour) || needed.iter().any(|a| !ok(a.1)) {
            return Err(WorkError::InvalidAmount);
        }
        let needed: Vec<Amount> = Good::ALL.into_iter().filter_map(|g| {
            let total: f32 = needed.iter().filter(|a| a.0 == g).map(|a| a.1).sum();
            (total > 0.0).then_some((g, total))
        }).collect();
        let site = SiteProgress {
            delivered: needed.iter().map(|a| (a.0, 0.0)).collect(),
            needed,
            labour_done: 0.0,
            labour_needed: labour,
        };
        self.places.push(Workplace::new(b, kind, vec![Slot::empty(Role::Build); SITE_SLOTS], None, Some(site)));
        Ok(())
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
            if site.needed.iter().filter(|n| n.0 == good).map(|n| n.1).sum::<f32>() - d.1 < EPS {
                d.1 = site.needed.iter().filter(|n| n.0 == good).map(|n| n.1).sum();
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
        self.places.push(Workplace::new(b.id, b.kind, slots, crate::catalogue::work_area(b), None));
        Ok(())
    }
    pub fn finish_site(&mut self, b: BuildingId, slots: &[Role], area: Option<Area>) -> Result<(), WorkError> {
        let p = self.place_mut(b)?;
        let builders: Vec<BeaverId> = p.slots.iter().filter_map(|s| s.worker).collect();
        p.slots = slots.iter().map(|&role| Slot::empty(role)).collect();
        p.site = None;
        p.area = area;
        p.max_workers = p.slots.len();
        self.managed.retain(|id, _| !builders.contains(id));
        Ok(())
    }
    pub fn stock_of(&self, b: BuildingId, good: Good) -> f32 {
        self.inventory.at(b, good)
    }
    #[cfg(test)]
    pub fn stock_add(&mut self, b: BuildingId, good: Good, amount: f32) {
        self.inventory.put(b, good, amount);
    }
    pub fn set_area(&mut self, b: BuildingId, area: Area) -> Result<(), WorkError> {
        if area.min.0 > area.max.0 || area.min.1 > area.max.1 {
            return Err(WorkError::InvalidArea);
        }
        self.place_mut(b)?.area = Some(area);
        Ok(())
    }
}
