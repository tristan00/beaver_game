//! Locks. Once a Raft landing stands, a built gate can be upgraded into a lock: a pair of gates with
//! a chamber between. A swimming beaver or a raft goes in, the gate behind it shuts, the
//! chamber fills or drains to the level on the far side, and the gate ahead opens; so it gets
//! past, either way, while the gate keeps holding the water as set.
//!
//! The upgrade is builders' work at the gate, done as the lock's materials (`works.lock` in the content) are
//! brought there (see `gates::work`). Locks are kept apart from the gates they upgrade (and saved after the world,
//! see `save`), so older saves load.
include!(concat!(env!("OUT_DIR"), "/locks_constants.rs"));

use std::collections::BTreeMap;

use crate::buildings::{BuildingId};
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

/// Whether locks can be built: the building they need (`works.lock_needs`) stands finished.
pub fn unlocked(sim: &SimState) -> bool {
    crate::content::content().works.lock_needs.is_none_or(|k| sim.buildings.buildings.iter().any(|b| b.kind == k && sim.finished(b)))
}

/// The gate a lock is made from: its cell and, on a widened dam, the dam cell behind it in every
/// row.
fn gate_cells(sim: &SimState, l: &Lock) -> Vec<Cell> {
    match crate::dams::dam(sim, l.building) {
        Some(d) => crate::dams::gate_cells(d, l.cell),
        None => vec![l.cell],
    }
}

/// The cells a built lock's chamber takes up, swum through from gate to gate: a lane one cell
/// wide down its middle, the length of the chamber, right through the dam (every row of a
/// widened one). Stepped side to side (never corner to corner) on a slant, so it is swum
/// through between its walls.
pub fn lock_cells(sim: &SimState, l: &Lock) -> Vec<Cell> {
    let Some(f) = frame(sim, l) else { return gate_cells(sim, l) };
    let w = lane(&f);
    let mut out = cells_where(sim, &f, f.half, |a, x| a.abs() <= f.half - 0.5 + 1e-3 && x.abs() <= w);
    if !out.contains(&l.cell) {
        out.push(l.cell);
    }
    out
}

/// The cells under a built lock's walls, either side of its lane from end to end of the walls
/// (which run on past the gates): nothing goes through them, so the only way through or
/// into the chamber is along the lane.
pub fn wall_cells(sim: &SimState, l: &Lock) -> Vec<Cell> {
    let Some(f) = frame(sim, l) else { return Vec::new() };
    let w = lane(&f);
    let end = f.half * WALL_REACH;
    let lock = lock_cells(sim, l);
    cells_where(sim, &f, end, |a, x| a.abs() <= end && x.abs() > w && x.abs() <= HALF_WIDTH).into_iter().filter(|c| !lock.contains(c)).collect()
}

/// Half a lock's width across its flow, in cells: its walls and the stakes behind them.
pub const HALF_WIDTH: f32 = 1.36;

/// How far a lock's walls reach from its middle along the flow, in halves of its chamber: on
/// past each gate, guiding the way in.
pub const WALL_REACH: f32 = 1.6 / 1.05;

/// How far from a lock's middle line its lane takes in a cell: half a cell straight along the
/// grid, more on a slant, so the lane steps side to side.
fn lane(f: &Frame) -> f32 {
    (f.axis.0.abs() + f.axis.1.abs()) / 2.0
}

/// The cells within `reach` of a lock's middle, either way along, whose middles (along, across
/// its flow) let `keep` through.
fn cells_where(sim: &SimState, f: &Frame, reach: f32, keep: impl Fn(f32, f32) -> bool) -> Vec<Cell> {
    let r = reach.ceil() as i64 + 2;
    let (cx, cy) = (f.centre.0.floor() as i64, f.centre.1.floor() as i64);
    let (w, h) = (sim.height.width() as i64, sim.height.height() as i64);
    let mut out = Vec::new();
    for y in (cy - r).max(0)..=(cy + r).min(h - 1) {
        for x in (cx - r).max(0)..=(cx + r).min(w - 1) {
            let (px, py) = (x as f32 + 0.5 - f.centre.0, y as f32 + 0.5 - f.centre.1);
            if keep(px * f.axis.0 + py * f.axis.1, -px * f.axis.1 + py * f.axis.0) {
                out.push((x as usize, y as usize));
            }
        }
    }
    out
}

/// Where a lock's chamber lies, in cells: its middle, the way it runs (along the flow, either
/// sign) and half its length; long enough for a raft with its beaver however thin the dam.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Frame {
    pub centre: (f32, f32),
    pub axis: (f32, f32),
    pub half: f32,
}

/// The way a line of cells runs through `c`, from the line within a few cells of it: a
/// stepped line drawn at a slant runs at its slant, not along its last step.
fn line_dir(line: &[Cell], c: Cell) -> (f32, f32) {
    const REACH: f32 = 4.5;
    let pts: Vec<(f32, f32)> = line.iter().map(|o| (o.0 as f32 - c.0 as f32, o.1 as f32 - c.1 as f32)).filter(|p| p.0.hypot(p.1) <= REACH).collect();
    if pts.len() < 2 {
        return (1.0, 0.0);
    }
    let k = pts.len() as f32;
    let (mx, my) = (pts.iter().map(|p| p.0).sum::<f32>() / k, pts.iter().map(|p| p.1).sum::<f32>() / k);
    let (mut xx, mut yy, mut xy) = (0.0, 0.0, 0.0);
    for p in &pts {
        let (x, y) = (p.0 - mx, p.1 - my);
        xx += x * x;
        yy += y * y;
        xy += x * y;
    }
    let a = 0.5 * (2.0 * xy).atan2(xx - yy);
    (a.cos(), a.sin())
}

pub fn frame(sim: &SimState, l: &Lock) -> Option<Frame> {
    let c = l.cell;
    let canal = crate::canals::canal(sim, l.building);
    let along: Vec<Cell> = match canal {
        Some(k) => k.cells.iter().filter(|x| x.row == 0).map(|x| x.cell).collect(),
        // Along the drawn line only: the rows a widened dam adds lie behind it.
        None => crate::dams::dam(sim, l.building)?.cells.iter().filter(|x| x.row == 0).map(|x| x.cell).collect(),
    };
    let (dx, dy) = line_dir(&along, c);
    let n = (dx * dx + dy * dy).sqrt().max(1e-6);
    // A canal's line runs along the flow; a dam's across it.
    let axis = if canal.is_some() { (dx / n, dy / n) } else { (-dy / n, dx / n) };
    let cells = gate_cells(sim, l);
    let pts: Vec<(f32, f32)> = cells.iter().map(|c| (c.0 as f32 + 0.5, c.1 as f32 + 0.5)).collect();
    let k = pts.len().max(1) as f32;
    let centre = (pts.iter().map(|p| p.0).sum::<f32>() / k, pts.iter().map(|p| p.1).sum::<f32>() / k);
    let span = pts.iter().map(|p| ((p.0 - centre.0) * axis.0 + (p.1 - centre.1) * axis.1).abs()).fold(0.0, f32::max);
    // The gates stand half a cell past the lane's end cells, so whoever waits at one waits a
    // cell's middle away, clear of it.
    let mut f = Frame { centre, axis, half: (span + 0.8).max(2.2) };
    let w = lane(&f);
    let ends = cells_where(sim, &f, f.half, |a, x| a.abs() <= f.half && x.abs() <= w).iter().map(|c| ((c.0 as f32 + 0.5 - centre.0) * axis.0 + (c.1 as f32 + 0.5 - centre.1) * axis.1).abs()).fold(0.0, f32::max);
    f.half = ends + 0.5;
    Some(f)
}

/// The ground a lock keeps clear: its chamber, the walls that run on past its gates by about
/// half as far again, and the water off each end that leads in. Nothing else may stand there.
pub fn footprint(sim: &SimState, l: &Lock) -> Vec<Cell> {
    let Some(f) = frame(sim, l) else { return Vec::new() };
    let reach = f.half * 1.5 + 0.5;
    cells_where(sim, &f, reach, |a, x| a.abs() <= reach && x.abs() <= HALF_WIDTH)
}

/// Whether a lock (built or ordered) keeps this cell clear of everything but its own dam or
/// canal (`except`).
pub fn reserved(sim: &SimState, c: Cell, except: Option<BuildingId>) -> bool {
    sim.locks.at.values().any(|l| Some(l.building) != except && footprint(sim, l).contains(&c))
}

/// Why a lock can't go at this gate: something else stands where its chamber or its ways in
/// would be.
pub fn check_room(sim: &SimState, building: BuildingId, cell: Cell) -> Result<(), String> {
    let room = footprint(sim, &Lock { cell, building, built: false });
    let other = |c: &Cell| sim.buildings.buildings.iter().any(|b| b.id != building && b.cells().any(|p| p == *c));
    if room.iter().any(other) || sim.work.supplies.is_some_and(|p| room.contains(&p)) {
        return Err("something is built where the lock would stand".into());
    }
    Ok(())
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
/// time, and builders build the gate and then the lock.
pub fn order(sim: &mut SimState, building: BuildingId, cell: Cell) -> Result<(), SimError> {
    if !unlocked(sim) {
        return Err(invalid("locks need a finished Raft landing"));
    }
    if sim.locks.at.contains_key(&cell) {
        return Err(invalid("that gate is already a lock"));
    }
    let gate = gate_built(sim, building, cell);
    if gate.is_none() && crate::dams::dam(sim, building).is_none() {
        return Err(invalid("no gate there"));
    }
    check_room(sim, building, cell).map_err(invalid)?;
    if gate.is_none() {
        crate::dams::add_gate(sim, building, cell)?;
    }
    // A canal's gate not built yet is built first.
    if let Some(k) = crate::canals::canal_mut(sim, building).ok().filter(|k| !k.gate.built && k.gate.pending.is_none()) {
        k.gate.pending = Some(k.gate.setting);
    }
    sim.locks.at.insert(cell, Lock { cell, building, built: false });
    Ok(())
}

/// The lock ordered at a gate is finished (see `gates::work`).
pub fn build(sim: &mut SimState, cell: Cell) {
    if let Some(l) = sim.locks.at.get_mut(&cell) {
        l.built = true;
    }
}

/// Drop locks whose gate is gone (the dam or canal removed).
pub fn tidy(sim: &mut SimState) {
    let gone: Vec<Cell> = sim.locks.at.values().filter(|l| gate_built(sim, l.building, l.cell).is_none()).map(|l| l.cell).collect();
    for c in gone {
        sim.locks.at.remove(&c);
    }
}
