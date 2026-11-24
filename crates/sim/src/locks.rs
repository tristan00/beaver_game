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

/// The cells a built lock takes up: its gate's cell and, on a widened dam, the dam cell behind
/// it in every row, so the lock runs right through the dam to the water beyond.
pub fn lock_cells(sim: &SimState, l: &Lock) -> Vec<Cell> {
    let mut out = vec![l.cell];
    if let Some(d) = crate::dams::dam(sim, l.building) {
        for k in 1..d.rows as i64 {
            let (x, y) = (l.cell.0 as i64 + d.downstream.0 as i64 * k, l.cell.1 as i64 + d.downstream.1 as i64 * k);
            if x < 0 || y < 0 {
                break;
            }
            let c = (x as usize, y as usize);
            if !d.cells.iter().any(|dc| dc.cell == c) {
                break;
            }
            out.push(c);
        }
    }
    out
}

/// A cell of a built lock (see `lock_cells`).
pub fn is_lock(sim: &SimState, c: Cell) -> bool {
    sim.locks.at.get(&c).is_some_and(|l| l.built) || sim.locks.at.values().any(|l| l.built && lock_cells(sim, l).contains(&c))
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

/// Order a lock at a gate, or straight on a built dam cell: the cell is made a gate at the same
/// time, and the builder who comes builds the gate and then the lock in one visit.
pub fn order(sim: &mut SimState, building: BuildingId, cell: Cell) -> Result<(), SimError> {
    if !unlocked(sim) {
        return Err(invalid("locks need a finished Harbor"));
    }
    if sim.locks.at.contains_key(&cell) {
        return Err(invalid("that gate is already a lock"));
    }
    match gate_built(sim, building, cell) {
        Some(_) => {}
        None if crate::dams::dam(sim, building).is_some() => crate::dams::add_gate(sim, building, cell)?,
        None => return Err(invalid("no gate there")),
    }
    // A canal's gate not built yet is built on the same visit.
    if let Some(k) = crate::canals::canal_mut(sim, building).ok().filter(|k| !k.gate.built && k.gate.pending.is_none()) {
        k.gate.pending = Some(k.gate.setting);
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
