//! Building placement, paid construction and cancellation. Dams are placed
//! and grown through `dams`; everything else is a paid site.
use crate::{SimError, SimState, agents, buildings, maintenance, water};

impl SimState {
    /// A building is unlocked once the one it needs has been finished; a placed site is not enough.
    pub fn unlocked(&self, kind: buildings::BuildingKind) -> Result<(), buildings::BuildError> {
        match kind.spec().requires {
            Some(need) if !self.buildings.buildings.iter().any(|b| b.kind == need && self.finished(b)) => Err(buildings::BuildError::Locked(need)),
            _ => Ok(()),
        }
    }

    /// Built in full. Dams and canals stand as soon as they are drawn and are raised or dug
    /// cell by cell, so they are finished only once no cell is left to build.
    pub fn finished(&self, b: &buildings::Building) -> bool {
        b.is_complete()
            && (b.dam.is_none() || !crate::dams::has_work_for(self, b.id, false))
            && b.canal.as_deref().is_none_or(|k| k.cells.iter().all(|c| c.done))
    }

    /// Shared by command execution, placement previews and the scripted player.
    pub fn check_site(&self, kind: buildings::BuildingKind, pos: water::Cell) -> Result<(), buildings::BuildError> {
        use buildings::BuildError;
        self.unlocked(kind)?;
        let cells = buildings::Buildings::footprint(kind, pos);
        if self.work.supplies.is_some_and(|pile| cells.contains(&pile)) {
            return Err(BuildError::SupplyPile);
        }
        if cells.iter().any(|&(x, y)| x < self.fog.size && y < self.fog.size && !self.fog.explored((x, y))) {
            return Err(BuildError::Unexplored);
        }
        if cells.iter().any(|&c| crate::locks::reserved(self, c, None) || self.resources.boulder_at(c)) {
            return Err(BuildError::Blocked);
        }
        let terrain = buildings::Terrain { depth: &self.water.depth };
        self.buildings.can_place_site(kind, pos, terrain)
    }

    pub fn place_site(&mut self, kind: buildings::BuildingKind, pos: (usize, usize)) -> Result<buildings::BuildingId, SimError> {
        if kind.is_dam() {
            return crate::dams::place(self, &[pos]);
        }
        if kind.is_canal() {
            return Err(SimError::InvalidCommand("draw a canal from water onto land".into()));
        }
        self.check_site(kind, pos).map_err(|e| SimError::InvalidCommand(e.to_string()))?;
        let terrain = buildings::Terrain { depth: &self.water.depth };
        let id = self
            .buildings
            .place_site(kind, pos, terrain)
            .map_err(|e| SimError::InvalidCommand(e.to_string()))?;
        let spec = kind.spec();
        if let Err(e) = self.work.add_site(id, kind, &spec.cost, spec.work) {
            self.buildings.buildings.retain(|b| b.id != id);
            return Err(SimError::InvalidCommand(format!("{e:?}")));
        }
        Ok(id)
    }

    pub fn has_building(&self, kind: buildings::BuildingKind) -> bool {
        self.buildings.buildings.iter().any(|b| b.kind == kind && b.is_complete())
    }

    pub fn cancel_site(&mut self, id: buildings::BuildingId) -> Result<Vec<buildings::Amount>, SimError> {
        if self.maintenance.jobs.contains_key(&id) {
            // A refit on a dam or canal is dropped and the works come down all the same;
            // on any other building the refit is all there is to cancel.
            let works = self.buildings.building(id).is_some_and(|b| b.dam.is_some() || b.canal.is_some());
            if !works {
                return maintenance::cancel(self, id);
            }
            maintenance::cancel(self, id)?;
        }
        let b = self.buildings.building(id).ok_or_else(|| SimError::InvalidCommand(format!("no building {}", id.0)))?;
        if b.dam.is_some() || b.canal.is_some() {
            self.tear_down_works(id)?;
        } else if b.is_complete() {
            return Err(SimError::InvalidCommand(format!("{} is finished; only unfinished sites can be cancelled", b.kind.spec().name)));
        }
        let mut refunded = Vec::new();
        if let Some(i) = self.work.places.iter().position(|p| p.building == id) {
            let place = self.work.places.remove(i);
            for &(g, a) in place.site.as_ref().map(|s| s.delivered.as_slice()).unwrap_or(&[]) {
                if a > 0.0 {
                    let put = self.refund_or_drop(self.buildings.building(id).map_or((0, 0), |b| b.pos), g, a);
                    refunded.push((g, put));
                }
            }
        }
        self.release_beavers(id);
        self.buildings.buildings.retain(|b| b.id != id);
        Ok(refunded)
    }

    /// A cancelled dam or canal: the dam's built cells and any gate come down, so the water
    /// runs free again, and its locks go with them. A canal's dug cells stay dug: ground once
    /// dug is just low ground.
    fn tear_down_works(&mut self, id: buildings::BuildingId) -> Result<(), SimError> {
        let mut cells: Vec<crate::water::Cell> = crate::dams::dam(self, id).map(|d| d.cells.iter().map(|c| c.cell).collect()).unwrap_or_default();
        if let Some(k) = crate::canals::canal(self, id).filter(|k| k.gate.built) {
            cells.extend(crate::gates::gate_cells(self, id, k.gate.cell));
        }
        for c in cells {
            self.water.clear_structure(c).map_err(|e| SimError::InvalidCommand(format!("{e:?}")))?;
        }
        Ok(())
    }

    /// Return `amount` of `good` to stores, dropping any overflow onto the ground at `cell` (marked already booked).
    pub(crate) fn refund_or_drop(&mut self, cell: crate::water::Cell, good: buildings::Good, amount: f32) -> f32 {
        let stored = self.buildings.stores.refund(good, amount).unwrap_or(0.0);
        if amount - stored > crate::work::EPS {
            self.work.piles.drop_load(cell, good, amount - stored, self.time_days, true, None);
        }
        stored
    }

    /// A finished building worn to nothing falls apart: what it held is left lying where it
    /// stood, its beavers and residents go, and the ground is free again.
    pub(crate) fn wreck(&mut self, id: buildings::BuildingId) {
        let Some(b) = self.buildings.building(id) else { return };
        let (kind, pos) = (b.kind, b.pos);
        if self.maintenance.jobs.contains_key(&id) {
            let _ = maintenance::cancel(self, id);
        }
        let (w, h) = kind.spec().footprint;
        let spot = (pos.0 + w / 2, pos.1 + h / 2);
        for g in buildings::Good::all() {
            let held = self.buildings.stores.inventory.at(id, g);
            let (got, sp) = self.buildings.stores.take_kind_from(id, g, held);
            if got > 0.0 {
                self.work.piles.drop_load(spot, g, got, self.time_days, true, sp);
            }
        }
        self.work.places.retain(|p| p.building != id);
        self.work.balanced.remove(&id);
        self.work.fills.remove(&id);
        self.maintenance.mending.remove(&id);
        self.release_beavers(id);
        self.buildings.buildings.retain(|b| b.id != id);
        if kind.is_home() {
            for b in self.colony.beavers.iter_mut().filter(|b| b.lodge == Some(id)) {
                b.lodge = None;
            }
            self.colony.house_homeless(&self.buildings.buildings);
        }
        self.buildings.wrecked.insert(id);
        crate::trace::event(format!("BUILDING_WRECKED {} id={} kind={kind:?}", crate::trace::sim_stamp(), id.0));
    }

    pub(crate) fn release_beavers(&mut self, id: buildings::BuildingId) {
        let mut wake_ids: Vec<crate::colony::BeaverId> = Vec::new();
        if let Some(p) = self.work.places.iter_mut().find(|p| p.building == id) {
            for s in &mut p.slots {
                if let Some(w) = s.beaver.take() { wake_ids.push(w); }
            }
        }
        for ag in self.agents.by_id.values_mut() {
            if ag.clear_target(id) { wake_ids.push(ag.id); }
        }
        for w in wake_ids { agents::wake(self, w); }
    }
}

use crate::buildings::{BuildingId, BuildingKind};
use crate::water::Cell;
use crate::Command;

/// Place a building site on the nearest good spot around `around`, keeping `keep_clear` free
/// and never walling the family off from its supplies.
pub fn place_best(s: &mut SimState, kind: BuildingKind, around: Cell, keep_clear: &[Cell]) -> Result<BuildingId, String> {
    place_ring(s, kind, around, 2..=29, keep_clear)
}

fn place_ring(s: &mut SimState, kind: BuildingKind, around: Cell, radii: std::ops::RangeInclusive<usize>, keep_clear: &[Cell]) -> Result<BuildingId, String> {
    let walker = s.colony.beavers.iter().find(|b| b.stage == crate::colony::Stage::Adult).map(|b| b.id);
    let from = walker.and_then(|w| s.agents.get(w)).map(|a| a.cell);
    let mut last = String::from("no cell tried");
    for r in radii {
        for y in around.1.saturating_sub(r)..=around.1 + r {
            for x in around.0.saturating_sub(r)..=around.0 + r {
                if x.abs_diff(around.0).max(y.abs_diff(around.1)) != r {
                    continue;
                }
                if let Err(e) = s.check_site(kind, (x, y)) {
                    last = e.to_string();
                    continue;
                }
                if crate::buildings::Buildings::footprint(kind, (x, y)).iter().any(|c| keep_clear.contains(c)) {
                    last = "on the planned dam".into();
                    continue;
                }
                if let (Some(w), Some(f)) = (walker, from) {
                    if s.nav.find_path(w, f, crate::nav::Goal::Beside((x, y))).is_none() {
                        last = "unreachable".into();
                        continue;
                    }
                }
                s.apply(Command::PlaceSite { kind, pos: (x, y) }).map_err(|e| e.to_string())?;
                let id = s.buildings.buildings.last().map(|b| b.id).ok_or("placed nothing")?;
                if let (Some(w), Some(f), Some(pile)) = (walker, from, s.work.supplies) {
                    if s.nav.find_path(w, f, crate::nav::Goal::Beside(pile)).is_none() {
                        let _ = s.apply(Command::CancelSite { building: id });
                        last = "would wall the family in".into();
                        continue;
                    }
                }
                return Ok(id);
            }
        }
    }
    Err(last)
}
