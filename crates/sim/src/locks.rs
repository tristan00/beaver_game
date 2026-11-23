//! Locks. Once a Harbor stands, a built gate can be upgraded into a lock: a pair of gates with
//! a chamber between. A swimming beaver or a barge goes in, the gate behind it shuts, the
//! chamber fills or drains to the level on the far side, and the gate ahead opens; so it gets
//! past, either way, while the gate keeps holding the water as set.
//!
//! The upgrade is a gate change a builder makes in one visit, bringing `LOCK_STICKS` and
//! `LOCK_CLAY`. Locks are kept apart from the gates they upgrade (and saved after the world,
//! see `save`), so older saves load.
include!(concat!(env!("OUT_DIR"), "/locks_constants.rs"));

use std::collections::BTreeMap;

use crate::buildings::{BuildingId, BuildingKind, Good};
use crate::water::Cell;
use crate::{SimError, SimState};

#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Lock {
    pub cell: Cell,
    /// The dam or canal whose gate it is.
    pub building: BuildingId,
    /// Built once a builder has brought its materials; until then the gate is a plain gate.
    pub built: bool,
}

#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Locks {
    pub at: BTreeMap<Cell, Lock>,
}

fn invalid(m: impl Into<String>) -> SimError {
    SimError::InvalidCommand(m.into())
}

/// Locks (and barges) need a finished Harbor.
pub fn unlocked(sim: &SimState) -> bool {
    sim.buildings.buildings.iter().any(|b| b.kind == BuildingKind::Harbor && sim.finished(b))
}

/// A built lock at this cell.
pub fn is_lock(sim: &SimState, c: Cell) -> bool {
    sim.locks.at.get(&c).is_some_and(|l| l.built)
}

/// A lock ordered at this cell and not yet built.
pub fn pending(sim: &SimState, c: Cell) -> bool {
    sim.locks.at.get(&c).is_some_and(|l| !l.built)
}

/// Whether this dam's or canal's gate at `cell` is built.
fn gate_built(sim: &SimState, building: BuildingId, cell: Cell) -> Option<bool> {
    if let Some(k) = crate::canals::canal(sim, building) {
        return (k.gate.cell == cell).then_some(k.gate.built);
    }
    crate::dams::dam(sim, building)?.gates.iter().find(|g| g.cell == cell).map(|g| g.built)
}

/// Order a gate upgraded into a lock; a builder visits to build it.
pub fn order(sim: &mut SimState, building: BuildingId, cell: Cell) -> Result<(), SimError> {
    if !unlocked(sim) {
        return Err(invalid("locks need a finished Harbor"));
    }
    match gate_built(sim, building, cell) {
        None => return Err(invalid("no gate there")),
        Some(false) => return Err(invalid("the gate is not built yet")),
        Some(true) => {}
    }
    if sim.locks.at.contains_key(&cell) {
        return Err(invalid("that gate is already a lock"));
    }
    sim.locks.at.insert(cell, Lock { cell, building, built: false });
    Ok(())
}

/// A builder's visit: build the ordered lock, using its sticks and clay from the stores.
pub fn build(sim: &mut SimState, cell: Cell) -> Result<(), SimError> {
    if !pending(sim, cell) {
        return Ok(());
    }
    if !crate::gates::has_goods(sim, Good::Sticks, LOCK_STICKS) || !crate::gates::has_goods(sim, Good::Clay, LOCK_CLAY) {
        return Err(invalid("the lock needs sticks and clay"));
    }
    crate::gates::take_goods(sim, cell, Good::Sticks, LOCK_STICKS);
    crate::gates::take_goods(sim, cell, Good::Clay, LOCK_CLAY);
    if let Some(l) = sim.locks.at.get_mut(&cell) {
        l.built = true;
    }
    Ok(())
}

/// Drop locks whose gate is gone (the dam or canal removed).
pub fn tidy(sim: &mut SimState) {
    let gone: Vec<Cell> = sim.locks.at.values().filter(|l| gate_built(sim, l.building, l.cell).is_none()).map(|l| l.cell).collect();
    for c in gone {
        sim.locks.at.remove(&c);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_gate_becomes_a_lock_only_with_a_harbor_and_once_built() {
        let mut s = SimState::new(1);
        let (k, cell) = (BuildingId(5), (3, 4));
        assert!(order(&mut s, k, cell).is_err(), "no harbor");
        s.buildings.buildings.push(crate::buildings::Building { id: BuildingId(9), kind: BuildingKind::Harbor, pos: (20, 20), status: crate::buildings::Status::Complete, output: 0.0, condition: 1.0, reinforced: false, dam: None, canal: None });
        assert!(unlocked(&s));
        assert!(order(&mut s, k, cell).is_err(), "no gate there");
    }
}
