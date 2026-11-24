//! Building placement, paid construction and cancellation. Dams are placed
//! and grown through `dams`; everything else is a paid site.
use crate::{SimError, SimState, agents, buildings, maintenance, water};

#[cfg(test)]
mod placement_tests {
    use super::*;
    use buildings::{BuildError, BuildingKind};

    #[test]
    fn preview_and_command_reject_the_same_site_without_mutation() {
        let mut sim = SimState::new(1);
        sim.work.supplies = Some((5, 5));
        let before = sim.buildings.buildings.len();
        let error = sim.check_site(BuildingKind::Shelter, (5, 5)).unwrap_err();
        assert_eq!(error, BuildError::SupplyPile);
        let command = crate::Command::PlaceSite { kind: BuildingKind::Shelter, pos: (5, 5) };
        assert_eq!(sim.apply(command), Err(SimError::InvalidCommand(error.to_string())));
        assert_eq!(sim.buildings.buildings.len(), before);
        assert!(sim.work.places.is_empty());
    }

    /// A shelter, a dam and a canal can be built from day 1; the rest wait for what unlocks them.
    #[test]
    fn buildings_unlock_once_their_prerequisite_is_finished() {
        let mut sim = SimState::new(1);
        for k in [BuildingKind::Shelter, BuildingKind::Dam, BuildingKind::Canal] {
            assert_eq!(sim.unlocked(k), Ok(()), "{k:?} is open from the start");
        }
        for k in [BuildingKind::Warehouse, BuildingKind::Larder, BuildingKind::Toolshed, BuildingKind::Forester, BuildingKind::Logistics, BuildingKind::Lookout] {
            assert!(matches!(sim.unlocked(k), Err(BuildError::Locked(_))), "{k:?} is locked at the start");
        }
        let shelter = |status| buildings::Building { id: buildings::BuildingId(1), kind: BuildingKind::Shelter, pos: (5, 5), status, output: 0.0, condition: 1.0, reinforced: false, dam: None, canal: None };
        sim.buildings.buildings.push(shelter(buildings::Status::Site));
        assert_eq!(sim.unlocked(BuildingKind::Warehouse), Err(BuildError::Locked(BuildingKind::Shelter)), "a shelter site is not enough");
        sim.buildings.buildings[0] = shelter(buildings::Status::Complete);
        assert_eq!(sim.unlocked(BuildingKind::Warehouse), Ok(()));
        assert_eq!(sim.unlocked(BuildingKind::Toolshed), Err(BuildError::Locked(BuildingKind::Warehouse)), "one step at a time");
    }
}

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
        if kind == buildings::BuildingKind::Dam {
            return crate::dams::place(self, &[pos]);
        }
        if kind == buildings::BuildingKind::Canal {
            return Err(SimError::InvalidCommand("draw a canal from water onto land".into()));
        }
        self.check_site(kind, pos).map_err(|e| SimError::InvalidCommand(e.to_string()))?;
        let terrain = buildings::Terrain { depth: &self.water.depth };
        let id = self
            .buildings
            .place_site(kind, pos, terrain)
            .map_err(|e| SimError::InvalidCommand(e.to_string()))?;
        let spec = kind.spec();
        if let Err(e) = self.work.add_site(id, kind, spec.cost, spec.work) {
            self.buildings.buildings.retain(|b| b.id != id);
            return Err(SimError::InvalidCommand(format!("{e:?}")));
        }
        Ok(id)
    }

    pub fn has_building(&self, kind: buildings::BuildingKind) -> bool {
        self.buildings.buildings.iter().any(|b| b.kind == kind && b.is_complete())
    }

    pub fn cancel_site(&mut self, id: buildings::BuildingId) -> Result<Vec<buildings::Amount>, SimError> {
        if self.maintenance.jobs.contains_key(&id) { return maintenance::cancel(self, id); }
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
                    let put = self.buildings.stores.refund(g, a).map_err(|e| SimError::InvalidCommand(e.to_string()))?;
                    refunded.push((g, put));
                }
            }
        }
        self.release_workers(id);
        self.buildings.buildings.retain(|b| b.id != id);
        self.output.forget(id);
        Ok(refunded)
    }

    /// A cancelled dam or canal: the dam's built cells and any gate come down, so the water
    /// runs free again, and its locks go with them. A canal's dug cells stay dug: ground once
    /// dug is just low ground.
    fn tear_down_works(&mut self, id: buildings::BuildingId) -> Result<(), SimError> {
        let mut cells: Vec<crate::water::Cell> = crate::dams::dam(self, id).map(|d| d.cells.iter().map(|c| c.cell).collect()).unwrap_or_default();
        if let Some(k) = crate::canals::canal(self, id).filter(|k| k.gate.built) {
            cells.push(k.gate.cell);
        }
        for c in cells {
            self.water.clear_structure(c).map_err(|e| SimError::InvalidCommand(format!("{e:?}")))?;
        }
        self.locks.at.retain(|_, l| l.building != id);
        Ok(())
    }

    /// A finished building worn to nothing falls apart: what it held is left lying where it
    /// stood, its workers and residents go, and the ground is free again.
    pub(crate) fn wreck(&mut self, id: buildings::BuildingId) {
        let Some(b) = self.buildings.building(id) else { return };
        let (kind, pos) = (b.kind, b.pos);
        if self.maintenance.jobs.contains_key(&id) {
            let _ = maintenance::cancel(self, id);
        }
        let (w, h) = kind.spec().footprint;
        let spot = (pos.0 + w / 2, pos.1 + h / 2);
        for g in buildings::Good::ALL {
            let held = self.buildings.stores.inventory.at(id, g);
            let got = self.buildings.stores.take_from(id, g, held);
            if got > 0.0 {
                self.work.piles.drop_at(spot, g, got, self.time_days);
            }
        }
        self.work.places.retain(|p| p.building != id);
        self.work.hauls.retain(|o| o.from != id && o.to != id);
        self.work.balanced.remove(&id);
        self.work.fills.remove(&id);
        if let Some(home) = self.work.homes.remove(&id) {
            for b in self.colony.beavers.iter_mut().filter(|b| b.lodge == Some(home)) {
                b.lodge = None;
            }
            self.colony.lodges.retain(|l| l.id != home);
            self.colony.house_homeless();
        }
        self.agents.maint_targets.retain(|_, t| *t != id);
        self.forestry.cut_rules.remove(&id);
        self.buildings.water_bound.remove(&id);
        self.buildings.pebbled.remove(&id);
        self.release_workers(id);
        self.buildings.buildings.retain(|b| b.id != id);
        self.buildings.wrecked.insert(id);
        self.output.forget(id);
        crate::trace::event(format!("BUILDING_WRECKED {} id={} kind={kind:?}", crate::trace::sim_stamp(), id.0));
    }

    fn release_workers(&mut self, id: buildings::BuildingId) {
        for ag in self.agents.by_id.values_mut() {
            if matches!(ag.assignment, Some(agents::Assignment::Slot(b, _)) if b == id) {
                ag.assignment = None;
            }
        }
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
