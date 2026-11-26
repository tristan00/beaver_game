//! The run's tallies, for the ledger's run page: what the colony has done since it began that
//! nothing else keeps count of. Births, deaths and goods gathered are kept elsewhere (the
//! colony's `Stats`, the stores' totals); this keeps goods moved by raft, buildings and locks
//! finished, and the most water ever held. Saved after the world (see `save`), so older saves
//! load with nothing counted and count what stands at the next day's close.
use std::collections::BTreeSet;

use crate::buildings::{BuildingId, BuildingKind, Good, PerId};
use crate::water::Cell;
use crate::SimState;

#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct RunStats {
    /// Goods unloaded from rafts, by good.
    pub raftd: PerId<Good, f32>,
    /// Raft loads delivered.
    pub raft_trips: u32,
    /// Buildings finished, by kind.
    pub built: PerId<BuildingKind, u32>,
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
            self.raftd[g] += amount;
            self.raft_trips += 1;
        }
    }

    pub fn built_of(&self, kind: BuildingKind) -> u32 {
        self.built[kind]
    }
}

/// At a day's close: count each building finished and each lock built for the first time, and
/// the water held.
pub fn close_day(sim: &mut SimState, water: f32) {
    let mut run = std::mem::take(&mut sim.run);
    for b in &sim.buildings.buildings {
        if !run.counted.contains(&b.id) && sim.finished(b) {
            run.counted.insert(b.id);
            run.built[b.kind] += 1;
        }
    }
    for l in crate::locks::built(sim) {
        if run.counted_locks.insert(l.cell) {
            run.locks += 1;
        }
    }
    run.peak_water = run.peak_water.max(water);
    sim.run = run;
}
