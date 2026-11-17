//! Workplaces, construction sites, worker slots, work areas and hauling (owner: `build`).
//! A `Workplace` is any building with worker slots; a site is a workplace with `site: Some(..)`.

use crate::buildings::{Amount, BuildingId, BuildingKind, Good};
use crate::colony::BeaverId;
use crate::water::Cell;

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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WorkError {
    NoSuchWorkplace,
    SlotTaken,
    SlotOutOfRange,
    NotAdult,
    InvalidArea,
}

/// All workplaces and haul orders.
#[derive(Clone, Debug, Default)]
pub struct Work {
    pub places: Vec<Workplace>,
    pub hauls: Vec<HaulOrder>,
}

impl Work {
    pub fn place(&self, b: BuildingId) -> Option<&Workplace> {
        self.places.iter().find(|p| p.building == b)
    }
    fn place_mut(&mut self, b: BuildingId) -> Result<&mut Workplace, WorkError> {
        self.places.iter_mut().find(|p| p.building == b).ok_or(WorkError::NoSuchWorkplace)
    }
    /// Puts `who` into a free slot (`build` task adds the adult check and frees `who`'s old slot).
    pub fn assign(&mut self, b: BuildingId, slot: usize, who: BeaverId) -> Result<(), WorkError> {
        let s = self.place_mut(b)?.slots.get_mut(slot).ok_or(WorkError::SlotOutOfRange)?;
        if s.worker.is_some() {
            return Err(WorkError::SlotTaken);
        }
        s.worker = Some(who);
        Ok(())
    }
    pub fn unassign(&mut self, b: BuildingId, slot: usize) -> Result<(), WorkError> {
        self.place_mut(b)?.slots.get_mut(slot).ok_or(WorkError::SlotOutOfRange)?.worker = None;
        Ok(())
    }
    /// Moves or resizes a workplace's area.
    pub fn set_area(&mut self, b: BuildingId, area: Area) -> Result<(), WorkError> {
        if area.min.0 > area.max.0 || area.min.1 > area.max.1 {
            return Err(WorkError::InvalidArea);
        }
        self.place_mut(b)?.area = Some(area);
        Ok(())
    }
}
