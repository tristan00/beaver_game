//! The run's tallies, for the ledger's run page: what the colony has done since it began that
//! nothing else keeps count of. Births, deaths and goods gathered are kept elsewhere (the
//! colony's `Stats`, the stores' totals); this keeps goods moved by raft, buildings and locks
//! finished, and the most water ever held. Saved after the world (see `save`), so older saves
//! load with nothing counted and count what stands at the next day's close.
use std::collections::BTreeSet;

use crate::buildings::{BuildingId, BuildingKind, Good};
use crate::water::Cell;
use crate::SimState;

const GOODS: usize = Good::ALL.len();
const KINDS: usize = BuildingKind::ALL.len();

#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct RunStats {
    /// Goods unloaded from rafts, by good.
    pub raftd: [f32; GOODS],
    /// Raft loads delivered.
    pub raft_trips: u32,
    /// Buildings finished, by kind (in `BuildingKind::ALL` order).
    pub built: [u32; KINDS],
    /// Gates upgraded into locks.
    pub locks: u32,
    /// The most water held behind dams at a day's close (m³).
    pub peak_water: f32,
    counted: BTreeSet<BuildingId>,
    counted_locks: BTreeSet<Cell>,
}

impl RunStats {
    /// A raft's load unloaded into a store.
    pub fn raft_delivered(&mut self, g: Good, amount: f32) {
        if amount > 0.0 {
            self.raftd[g.index()] += amount;
            self.raft_trips += 1;
        }
    }

    pub fn built_of(&self, kind: BuildingKind) -> u32 {
        BuildingKind::ALL.iter().position(|&k| k == kind).map_or(0, |i| self.built[i])
    }
}

/// At a day's close: count each building finished and each lock built for the first time, and
/// the water held.
pub fn close_day(sim: &mut SimState, water: f32) {
    let mut run = std::mem::take(&mut sim.run);
    for b in &sim.buildings.buildings {
        if !run.counted.contains(&b.id) && sim.finished(b) {
            run.counted.insert(b.id);
            if let Some(i) = BuildingKind::ALL.iter().position(|&k| k == b.kind) {
                run.built[i] += 1;
            }
        }
    }
    for l in sim.locks.at.values().filter(|l| l.built) {
        if run.counted_locks.insert(l.cell) {
            run.locks += 1;
        }
    }
    run.peak_water = run.peak_water.max(water);
    sim.run = run;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::buildings::{Building, Status};

    #[test]
    fn a_building_is_counted_once_when_finished() {
        let mut s = SimState::new(1);
        let before = s.run.built_of(BuildingKind::Warehouse);
        s.buildings.buildings.push(Building { id: BuildingId(900), kind: BuildingKind::Warehouse, pos: (5, 5), status: Status::Complete, output: 0.0, condition: 1.0, reinforced: false, dam: None, canal: None });
        close_day(&mut s, 10.0);
        close_day(&mut s, 4.0);
        assert_eq!(s.run.built_of(BuildingKind::Warehouse), before + 1);
        assert_eq!(s.run.peak_water, 10.0);
    }

    #[test]
    fn the_tallies_are_saved() {
        let mut s = SimState::new(1);
        s.run.raft_delivered(Good::Food, 12.0);
        close_day(&mut s, 3.0);
        let (_, back) = crate::save::decode(&crate::save::encode(&s, "test").unwrap()).unwrap();
        assert_eq!(back.run, s.run);
    }
}
