//! Building placement, paid construction and cancellation. Dams are placed
//! and grown through `dams`; everything else is a paid site.
use crate::{SimError, SimState, agents, buildings, maintenance, water, work};

#[cfg(test)]
mod placement_tests {
    use super::*;
    use buildings::{BuildError, BuildingKind};

    #[test]
    fn preview_and_command_reject_the_same_site_without_mutation() {
        let mut sim = SimState::new(1);
        sim.work.supplies = Some((5, 5));
        let before = sim.buildings.buildings.len();
        let error = sim.check_site(BuildingKind::Warehouse, (5, 5)).unwrap_err();
        assert_eq!(error, BuildError::SupplyPile);
        let command = crate::Command::PlaceSite { kind: BuildingKind::Warehouse, pos: (5, 5) };
        assert_eq!(sim.apply(command), Err(SimError::InvalidCommand(error.to_string())));
        assert_eq!(sim.buildings.buildings.len(), before);
        assert!(sim.work.places.is_empty());
    }
}

impl SimState {
    /// Shared by command execution, placement previews and the scripted player.
    pub fn check_site(&self, kind: buildings::BuildingKind, pos: water::Cell) -> Result<(), buildings::BuildError> {
        use buildings::BuildError;
        if let Some(&need) = self.missing_prerequisites(kind).first().filter(|_| !self.sandbox) {
            return Err(BuildError::Locked(need));
        }
        let cells = buildings::Buildings::footprint(kind, pos);
        if self.work.supplies.is_some_and(|pile| cells.contains(&pile)) {
            return Err(BuildError::SupplyPile);
        }
        if cells.iter().any(|&(x, y)| x < self.fog.size && y < self.fog.size && !self.fog.explored((x, y))) {
            return Err(BuildError::Unexplored);
        }
        let terrain = buildings::Terrain { height: &self.height, depth: &self.water.depth };
        self.buildings.can_place_site(kind, pos, terrain)
    }

    pub fn place_site(&mut self, kind: buildings::BuildingKind, pos: (usize, usize)) -> Result<buildings::BuildingId, SimError> {
        if kind == buildings::BuildingKind::Dam {
            return crate::dams::place(self, &[pos]);
        }
        self.check_site(kind, pos).map_err(|e| SimError::InvalidCommand(e.to_string()))?;
        let terrain = buildings::Terrain { height: &self.height, depth: &self.water.depth };
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

    pub fn missing_prerequisites(&self, kind: buildings::BuildingKind) -> Vec<buildings::BuildingKind> {
        kind.spec().prerequisites
            .iter()
            .copied()
            .filter(|&p| !self.buildings.buildings.iter().any(|b| b.kind == p && b.is_complete()))
            .collect()
    }

    pub fn has_building(&self, kind: buildings::BuildingKind) -> bool {
        self.buildings.buildings.iter().any(|b| b.kind == kind && b.is_complete())
    }

    pub fn can_build(&self, kind: buildings::BuildingKind) -> bool {
        self.sandbox || self.missing_prerequisites(kind).is_empty()
    }

    pub fn cancel_site(&mut self, id: buildings::BuildingId) -> Result<Vec<buildings::Amount>, SimError> {
        if self.maintenance.jobs.contains_key(&id) { return maintenance::cancel(self, id); }
        let b = self.buildings.building(id).ok_or_else(|| SimError::InvalidCommand(format!("no building {}", id.0)))?;
        if b.dam.is_some() {
            return self.remove_dam(id);
        }
        if b.is_complete() {
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

    /// A dam with nothing built yet can be withdrawn; built cells stay in the river.
    fn remove_dam(&mut self, id: buildings::BuildingId) -> Result<Vec<buildings::Amount>, SimError> {
        let built = crate::dams::dam(self, id).is_some_and(|d| d.cells.iter().any(|c| crate::dams::built(self, c.cell) > 0.0));
        if built {
            return Err(SimError::InvalidCommand("part of this dam is already built".into()));
        }
        self.work.places.retain(|p| p.building != id);
        self.release_workers(id);
        self.buildings.buildings.retain(|b| b.id != id);
        self.output.forget(id);
        Ok(Vec::new())
    }

    fn release_workers(&mut self, id: buildings::BuildingId) {
        self.work.managed.retain(|_, duty| !matches!(duty, work::ManagedDuty::Slot(b, _) if *b == id));
        for ag in self.agents.by_id.values_mut() {
            if matches!(ag.assignment, Some(agents::Assignment::Slot(b, _)) if b == id) {
                ag.assignment = None;
            }
        }
    }
}
