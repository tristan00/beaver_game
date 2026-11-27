use std::collections::BTreeSet;

use crate::buildings::{BuildingId, BuildingKind, Good, PerId};
use crate::water::Cell;
use crate::SimState;

#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct RunStats {
    pub raftd: PerId<Good, f32>,
    pub raft_trips: u32,
    pub built: PerId<BuildingKind, u32>,
    pub locks: u32,
    pub peak_water: f32,
    counted: BTreeSet<BuildingId>,
    counted_locks: BTreeSet<Cell>,
}

impl RunStats {
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
