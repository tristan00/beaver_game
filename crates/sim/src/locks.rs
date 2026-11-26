//! Locks. Once a Raft landing stands, a built gate can be upgraded into a lock: a pair of gates with
//! a chamber between. Lock state lives directly on each `Gate` (`LockState`).
include!(concat!(env!("OUT_DIR"), "/locks_constants.rs"));

use crate::buildings::BuildingId;
use crate::gates::LockState;
use crate::water::Cell;
use crate::{SimError, SimState};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Lock {
    pub cell: Cell,
    /// The dam or canal whose gate it is.
    pub building: BuildingId,
    /// Built once a builder has brought its materials; until then the gate is a plain gate.
    pub built: bool,
}

fn invalid(m: impl Into<String>) -> SimError {
    SimError::InvalidCommand(m.into())
}

/// Every lock (pending or built) across all dams and canals.
pub fn all(sim: &SimState) -> impl Iterator<Item = Lock> + '_ {
    sim.buildings.buildings.iter().flat_map(|b| {
        b.gates().iter().filter(|g| g.lock != LockState::None).map(move |g| Lock {
            cell: g.cell,
            building: b.id,
            built: g.lock == LockState::Built,
        })
    })
}

/// Every finished lock across all dams and canals.
pub fn built(sim: &SimState) -> impl Iterator<Item = Lock> + '_ {
    all(sim).filter(|l| l.built)
}

/// Whether a lock (pending or built) exists at `c`.
pub fn has(sim: &SimState, c: Cell) -> bool {
    all(sim).any(|l| l.cell == c)
}

/// Whether locks can be built: the building they need (`works.lock_needs`) stands finished.
pub fn unlocked(sim: &SimState) -> bool {
    crate::content::content().works.lock_needs.is_none_or(|k| sim.buildings.buildings.iter().any(|b| b.kind == k && sim.finished(b)))
}

fn gate_cells(sim: &SimState, l: &Lock) -> Vec<Cell> {
    crate::gates::gate_cells(sim, l.building, l.cell)
}

pub fn lock_cells(sim: &SimState, l: &Lock) -> Vec<Cell> {
    let Some(f) = frame(sim, l) else { return gate_cells(sim, l) };
    let w = lane(&f);
    let mut out = cells_where(sim, &f, f.half, |a, x| a.abs() <= f.half - 0.5 + 1e-3 && x.abs() <= w);
    if !out.contains(&l.cell) {
        out.push(l.cell);
    }
    out
}

pub fn wall_cells(sim: &SimState, l: &Lock) -> Vec<Cell> {
    let Some(f) = frame(sim, l) else { return Vec::new() };
    let w = lane(&f);
    let end = f.half * WALL_REACH;
    let lock = lock_cells(sim, l);
    cells_where(sim, &f, end, |a, x| a.abs() <= end && x.abs() > w && x.abs() <= HALF_WIDTH)
        .into_iter()
        .filter(|c| !lock.contains(c))
        .collect()
}

pub const HALF_WIDTH: f32 = 1.36;
pub const WALL_REACH: f32 = 1.6 / 1.05;

fn lane(f: &Frame) -> f32 {
    (f.axis.0.abs() + f.axis.1.abs()) / 2.0
}

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

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Frame {
    pub centre: (f32, f32),
    pub axis: (f32, f32),
    pub half: f32,
}

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
        None => crate::dams::dam(sim, l.building)?.cells.iter().filter(|x| x.row == 0).map(|x| x.cell).collect(),
    };
    let (dx, dy) = line_dir(&along, c);
    let n = (dx * dx + dy * dy).sqrt().max(1e-6);
    let axis = if canal.is_some() { (dx / n, dy / n) } else { (-dy / n, dx / n) };
    let cells = gate_cells(sim, l);
    let pts: Vec<(f32, f32)> = cells.iter().map(|c| (c.0 as f32 + 0.5, c.1 as f32 + 0.5)).collect();
    let k = pts.len().max(1) as f32;
    let centre = (pts.iter().map(|p| p.0).sum::<f32>() / k, pts.iter().map(|p| p.1).sum::<f32>() / k);
    let span = pts.iter().map(|p| ((p.0 - centre.0) * axis.0 + (p.1 - centre.1) * axis.1).abs()).fold(0.0, f32::max);
    let mut f = Frame { centre, axis, half: (span + 0.8).max(2.2) };
    let w = lane(&f);
    let ends = cells_where(sim, &f, f.half, |a, x| a.abs() <= f.half && x.abs() <= w)
        .iter()
        .map(|c| ((c.0 as f32 + 0.5 - centre.0) * axis.0 + (c.1 as f32 + 0.5 - centre.1) * axis.1).abs())
        .fold(0.0, f32::max);
    f.half = ends + 0.5;
    Some(f)
}

pub fn footprint(sim: &SimState, l: &Lock) -> Vec<Cell> {
    let Some(f) = frame(sim, l) else { return Vec::new() };
    let reach = f.half * 1.5 + 0.5;
    cells_where(sim, &f, reach, |a, x| a.abs() <= reach && x.abs() <= HALF_WIDTH)
}

pub fn reserved(sim: &SimState, c: Cell, except: Option<BuildingId>) -> bool {
    all(sim).any(|l| Some(l.building) != except && footprint(sim, &l).contains(&c))
}

pub fn check_room(sim: &SimState, building: BuildingId, cell: Cell) -> Result<(), String> {
    for c in footprint(sim, &Lock { cell, building, built: false }) {
        crate::gates::check_cell_open(sim, c, Some(building), false)?;
    }
    Ok(())
}

pub fn is_lock(sim: &SimState, c: Cell) -> bool {
    built(sim).any(|l| l.cell == c || lock_cells(sim, &l).contains(&c))
}

pub fn pending(sim: &SimState, c: Cell) -> bool {
    all(sim).any(|l| !l.built && l.cell == c)
}

/// Combined material cost to order a lock at `(building, cell)` (includes gate cost if no gate is built yet).
pub fn order_cost(sim: &SimState, building: BuildingId, cell: Cell) -> Vec<crate::buildings::Amount> {
    let mut cost: Vec<crate::buildings::Amount> = crate::gates::cost(crate::gates::Piece::Lock).0.to_vec();
    if crate::gates::gate_of(sim, building, cell).is_none_or(|g| !g.built) {
        for &(g, a) in crate::gates::cost(crate::gates::Piece::Build).0 {
            match cost.iter_mut().find(|c| c.0 == g) { Some(c) => c.1 += a, None => cost.push((g, a)) }
        }
    }
    cost
}

/// Single eligibility check for ordering a lock on a dam or canal gate.
pub fn check_order(sim: &SimState, building: BuildingId, cell: Cell) -> Result<(), String> {
    if !unlocked(sim) {
        let need = crate::content::content().works.lock_needs.map_or("raft landing", |k| k.name());
        return Err(format!("locks need a finished {}", need.to_lowercase()));
    }
    if has(sim, cell) { return Err("that gate is already a lock".into()); }
    match crate::gates::gate_of(sim, building, cell) {
        None if crate::dams::dam(sim, building).is_some() => { crate::dams::plan_gate(sim, building, cell)?; }
        None => return Err("no gate there".into()),
        Some(g) if !g.built => crate::gates::check_unlocked(sim)?,
        Some(_) => {}
    }
    check_room(sim, building, cell)
}

pub fn order(sim: &mut SimState, building: BuildingId, cell: Cell) -> Result<(), SimError> {
    check_order(sim, building, cell).map_err(invalid)?;
    if crate::gates::gate_of(sim, building, cell).is_none() {
        crate::dams::add_gate(sim, building, cell)?;
    }
    let b = sim.buildings.buildings.iter_mut().find(|b| b.id == building).ok_or_else(|| invalid("no such building"))?;
    let is_canal = b.canal.is_some();
    let g = b.gates_mut().iter_mut().find(|g| g.cell == cell).ok_or_else(|| invalid("no gate there"))?;
    if is_canal && !g.built && g.pending.is_none() {
        g.pending = Some(g.setting);
    }
    g.lock = LockState::Pending;
    Ok(())
}

pub fn build(sim: &mut SimState, cell: Cell) {
    for b in &mut sim.buildings.buildings {
        if let Some(g) = b.gates_mut().iter_mut().find(|g| g.cell == cell && g.lock == LockState::Pending) {
            g.lock = LockState::Built;
            return;
        }
    }
}
