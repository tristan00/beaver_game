include!(concat!(env!("OUT_DIR"), "/work_constants.rs"));
use std::collections::{BTreeMap, BTreeSet};


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

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Area {
    pub min: Cell,
    pub max: Cell,
}

impl Area {
    pub fn contains(&self, c: Cell) -> bool {
        c.0 >= self.min.0 && c.0 <= self.max.0 && c.1 >= self.min.1 && c.1 <= self.max.1
    }
    /// Inside the circle that fits the area: a forester's reach.
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

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum Role {
    Collect(crate::resources::ResourceKind),
    Build,
    Haul,
    /// Only in saves from before the forester's two seats became one; merged into `Forest` on load.
    Plant,
    /// Only in saves from before maintainers worked without seats; such seats are removed on
    /// load and whoever sat in them keeps the Maintainer role.
    Maintain,
    /// The forester: collects seedlings from grown trees in the area for its store, plants the
    /// stored saplings there.
    Forest,
    /// The guard: keeps watch at its lookout and warns of wolves.
    Guard,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Slot {
    pub role: Role,
    pub worker: Option<BeaverId>,
    /// Switched off by the player: nobody is sent to it.
    pub off: bool,
}

impl Slot {
    pub fn empty(role: Role) -> Self {
        Slot { role, worker: None, off: false }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
pub enum Priority {
    Low,
    #[default]
    Normal,
    High,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
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
    /// Builder seats have no limit: a place that takes builders always has another seat.
    pub fn open_ended(&self) -> bool {
        self.slots.iter().any(|s| s.role == Role::Build)
    }
    /// The first empty seat `want` takes, or for a builder the next new one past the last.
    pub fn open_seat(&self, want: impl Fn(Role) -> bool) -> Option<(usize, Role)> {
        if let Some(i) = self.slots.iter().position(|s| s.worker.is_none() && !s.off && want(s.role)) {
            return Some((i, self.slots[i].role));
        }
        (self.open_ended() && want(Role::Build)).then_some((self.slots.len(), Role::Build))
    }
    /// Builders working here, and seats for others: one more than are filled, without limit.
    pub fn builders(&self) -> usize {
        self.slots.iter().filter(|s| s.role == Role::Build && s.worker.is_some()).count()
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

#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct HaulOrder {
    pub from: BuildingId,
    pub to: BuildingId,
    pub good: Good,
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
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

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Work {
    pub places: Vec<Workplace>,
    pub hauls: Vec<HaulOrder>,
    /// Logistics buildings whose haulers balance the stores: each from when it is finished,
    /// until the player switches it off.
    pub balanced: BTreeSet<BuildingId>,
    /// Stores the player asked haulers to fill to the brim.
    pub fills: BTreeSet<BuildingId>,
    pub(crate) inventory: crate::inventory::Inventory,
    pub dam_plans: Vec<DamPlan>,
    pub dam_failures: Vec<(BuildingId, String)>,
    pub supplies: Option<Cell>,
    /// Goods lying on the ground.
    pub piles: crate::piles::Piles,
    pub canal_plans: Vec<CanalPlan>,
    pub homes: BTreeMap<BuildingId, crate::colony::LodgeId>,
    /// How far from water, in cells, each role's workers are sent to work. Kept in the save's
    /// trailer (`save::Extras`) so saves from before it still load.
    #[serde(skip)]
    pub shore_limits: BTreeMap<crate::jobs::RoleKind, u16>,
}

impl Default for Work {
    fn default() -> Self {
        Work {
            places: Vec::new(),
            hauls: Vec::new(),
            balanced: BTreeSet::new(),
            fills: BTreeSet::new(),
            inventory: Default::default(),
            dam_plans: Vec::new(),
            dam_failures: Vec::new(),
            supplies: None,
            piles: Default::default(),
            canal_plans: Vec::new(),
            homes: BTreeMap::new(),
            shore_limits: BTreeMap::new(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CanalPlan {
    pub building: BuildingId,
    pub cells: Vec<Cell>,
    pub depth: f32,
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct DamPlan {
    pub building: BuildingId,
    /// Reuses a completed dam building; cancelling keeps that building.
    pub repair: bool,
    pub cells: Vec<Cell>,
    pub height: f32,
    pub heights: Vec<f32>,
    pub from: Vec<f32>,
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
        let p = self.place_mut(b)?;
        if slot == p.slots.len() && p.open_ended() {
            p.slots.push(Slot::empty(Role::Build));
        }
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
        Ok(())
    }
    pub fn vacate(&mut self, who: BeaverId) {
        for p in &mut self.places {
            for s in p.slots.iter_mut().filter(|s| s.worker == Some(who)) {
                s.worker = None;
            }
        }
    }
    pub fn add_site(&mut self, b: BuildingId, kind: BuildingKind, needed: &[Amount], labour: f32) -> Result<(), WorkError> {
        self.add_site_for(b, kind, needed, labour, Some(Role::Build))
    }
    /// A site whose slots are `role`: builders for new work, maintainers for repairs and refits.
    /// A site needing `needed` and `labour`; `role` is who takes its seats, or None for a site
    /// with no seats (a refit, worked by maintainers).
    pub fn add_site_for(&mut self, b: BuildingId, kind: BuildingKind, needed: &[Amount], labour: f32, role: Option<Role>) -> Result<(), WorkError> {
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
        let slots = role.map_or_else(Vec::new, |r| vec![Slot::empty(r); SITE_SLOTS]);
        self.places.push(Workplace::new(b, kind, slots, None, Some(site)));
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
        self.balance_from_the_start(b.id, b.kind);
        Ok(())
    }
    pub fn finish_site(&mut self, b: BuildingId, slots: &[Role], area: Option<Area>) -> Result<(), WorkError> {
        let crew = self.crew(b);
        let p = self.place_mut(b)?;
        p.slots = slots.iter().map(|&role| Slot::empty(role)).collect();
        p.site = None;
        p.area = area;
        p.max_workers = p.slots.len();
        let kind = p.kind;
        self.keep_crew(b, crew);
        self.balance_from_the_start(b, kind);
        Ok(())
    }
    /// A finished logistics building's haulers balance the stores until switched off.
    fn balance_from_the_start(&mut self, b: BuildingId, kind: BuildingKind) {
        if kind == BuildingKind::Logistics {
            self.balanced.insert(b);
        }
    }
    /// The builders and maintainers seated at a place.
    pub fn crew(&self, b: BuildingId) -> Vec<Slot> {
        self.place(b).map_or_else(Vec::new, |p| p.slots.iter().filter(|s| s.worker.is_some() && s.role == Role::Build).copied().collect())
    }
    /// A place whose work is done keeps its builders and maintainers seated: they are still
    /// builders, waiting there until the next job for them (see `agents::retarget`).
    pub fn keep_crew(&mut self, b: BuildingId, crew: Vec<Slot>) {
        let Ok(p) = self.place_mut(b) else { return };
        for s in crew {
            if !p.slots.iter().any(|x| x.worker == s.worker) {
                p.slots.push(s);
            }
        }
    }
    pub fn stock_of(&self, b: BuildingId, good: Good) -> f32 {
        self.inventory.at(b, good)
    }
    #[cfg(test)]
    pub fn stock_add(&mut self, b: BuildingId, good: Good, amount: f32) {
        self.inventory.put(b, good, None, amount);
    }
    pub fn set_area(&mut self, b: BuildingId, area: Area) -> Result<(), WorkError> {
        if area.min.0 > area.max.0 || area.min.1 > area.max.1 {
            return Err(WorkError::InvalidArea);
        }
        self.place_mut(b)?.area = Some(area);
        Ok(())
    }
}
