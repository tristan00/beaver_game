//! Gates on dams and canals, plus shared waterworks helpers.
use std::collections::VecDeque;

use crate::agents::{Task, WorkKind};
use crate::buildings::BuildingId;
use crate::canals::GATE_STEP;
use crate::colony::BeaverId;
use crate::water::Cell;
use crate::{SimError, SimState};

pub(crate) fn invalid(m: impl Into<String>) -> SimError { SimError::InvalidCommand(m.into()) }
pub(crate) fn ground(sim: &SimState, c: Cell) -> f32 { sim.height.get(c.0, c.1).copied().unwrap_or(0.0) }
pub(crate) fn depth(sim: &SimState, c: Cell) -> f32 { sim.water.depth.get(c.0, c.1).copied().unwrap_or(0.0) }
pub(crate) fn surface(sim: &SimState, c: Cell) -> f32 { ground(sim, c) + depth(sim, c) }
pub(crate) fn is_water(sim: &SimState, c: Cell) -> bool {
    sim.water.shown(c.0 as i32, c.1 as i32)
        || sim.world.river.get(c.0, c.1).copied().unwrap_or(false)
        || sim.world.lake.get(c.0, c.1).copied().unwrap_or(false)
}
pub(crate) fn cell_taken(sim: &SimState, c: Cell, except: Option<BuildingId>, reject_dam: bool) -> bool {
    sim.buildings.buildings.iter().any(|b| Some(b.id) != except && b.cells().any(|x| x == c))
        || (reject_dam && sim.water.dam_height.get(c.0, c.1).is_some_and(|h| *h > 0.0))
        || crate::locks::reserved(sim, c, except)
}
pub(crate) fn claimed_work(sim: &SimState, id: BuildingId, except: BeaverId, kinds: &[WorkKind]) -> Vec<Cell> {
    sim.agents.by_id.values().filter(|a| a.id != except).filter_map(|a| match a.task {
        Task::Work { site, cell: Some(cell), kind, .. } if site == id && kinds.contains(&kind) => Some(cell),
        _ => None,
    }).collect()
}
pub(crate) fn push_history<T>(h: &mut VecDeque<T>, cap: usize, item: T) {
    h.push_back(item);
    while h.len() > cap { h.pop_front(); }
}
pub(crate) fn finish_gate(g: &mut Gate) {
    g.built = true;
    if let Some(s) = g.pending.take() { g.setting = s; }
}
pub(crate) fn apply_across(sim: &mut SimState, g: Gate, cells: &[Cell]) -> Result<(), SimError> {
    for &cell in cells { apply(sim, &Gate { cell, ..g })?; }
    Ok(())
}

#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum GateSetting { Shut, Level(f32), Open }

impl GateSetting {
    pub fn name(self) -> &'static str {
        match self { Self::Shut => "shut", Self::Level(_) => "level", Self::Open => "open" }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum LockState { #[default] None, Pending, Built }

#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Gate {
    pub cell: Cell,
    pub floor: f32,
    pub top: f32,
    pub setting: GateSetting,
    pub pending: Option<GateSetting>,
    pub built: bool,
    #[serde(default)] pub lock: LockState,
}

impl Gate {
    pub fn new(cell: Cell, floor: f32, top: f32) -> Gate {
        Gate { cell, floor, top: top.max(floor), setting: GateSetting::Shut, pending: None, built: false, lock: LockState::None }
    }
    pub fn sill_of(&self, s: GateSetting) -> f32 {
        match s {
            GateSetting::Shut => self.top,
            GateSetting::Open => self.floor,
            GateSetting::Level(e) => (self.floor + ((e - self.floor) / GATE_STEP).round() * GATE_STEP).clamp(self.floor, self.top),
        }
    }
    pub fn sill(&self) -> f32 { self.sill_of(self.setting) }
    pub fn wanted(&self) -> GateSetting { self.pending.unwrap_or(self.setting) }
}

pub fn apply(sim: &mut SimState, g: &Gate) -> Result<(), SimError> {
    if !g.built { return Ok(()); }
    sim.water.set_gate(g.cell, (g.sill() - ground(sim, g.cell)).max(0.0)).map_err(|e| invalid(e.to_string()))
}

pub fn request(g: &mut Gate, setting: GateSetting) -> Result<(), SimError> {
    if matches!(setting, GateSetting::Level(e) if !e.is_finite()) { return Err(invalid("gate level must be finite")); }
    let same = g.built && std::mem::discriminant(&g.setting) == std::mem::discriminant(&setting) && (g.sill_of(setting) - g.sill()).abs() < 1e-4;
    g.pending = if same { None } else { Some(setting) };
    Ok(())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Piece { Build, Turn, Lock }

pub fn piece(sim: &SimState, building: BuildingId, g: &Gate) -> Option<Piece> {
    if !g.built { return (crate::canals::canal(sim, building).is_none() || g.pending.is_some()).then_some(Piece::Build); }
    if g.pending.is_some() { return Some(Piece::Turn); }
    crate::locks::pending(sim, g.cell).then_some(Piece::Lock)
}

pub fn cost(p: Piece) -> (&'static [crate::buildings::Amount], f32) {
    let w = &crate::content::content().works;
    match p { Piece::Build => (&w.gate, crate::canals::GATE_WORK), Piece::Turn => (&[], crate::canals::GATE_WORK), Piece::Lock => (&w.lock, crate::canals::GATE_WORK) }
}

fn labour_room(sim: &SimState, spot: crate::supply::Spot, p: Piece) -> f32 {
    let (mats, labour) = cost(p);
    let need: f32 = mats.iter().map(|a| a.1).sum();
    let got: f32 = mats.iter().map(|&(g, a)| crate::supply::delivered(sim, spot, g).min(a)).sum();
    labour * if need > 0.0 { (got / need).min(1.0) } else { 1.0 }
}

pub fn visit_ready(sim: &SimState, building: BuildingId, g: &Gate) -> bool {
    let spot = crate::supply::Spot { building, cell: g.cell };
    piece(sim, building, g).is_some_and(|p| crate::supply::labour(sim, spot) < labour_room(sim, spot, p) - 1e-3)
}

pub fn work(sim: &mut SimState, building: BuildingId, cell: Cell, amount: f32) -> Result<Option<Piece>, SimError> {
    let g = gate_of(sim, building, cell).ok_or_else(|| invalid("no gate there"))?;
    let p = piece(sim, building, &g).ok_or_else(|| invalid("nothing to do at the gate"))?;
    let spot = crate::supply::Spot { building, cell };
    let done = (crate::supply::labour(sim, spot) + amount).min(labour_room(sim, spot, p));
    crate::supply::set_labour(sim, spot, done);
    let (mats, labour) = cost(p);
    if done + 1e-3 < labour || !crate::supply::take(sim, spot, mats) { return Ok(None); }
    crate::supply::set_labour(sim, spot, 0.0);
    match p {
        Piece::Build | Piece::Turn if crate::canals::canal(sim, building).is_some() => crate::canals::turn_gate(sim, building)?,
        Piece::Build | Piece::Turn => crate::dams::work_gate(sim, building, cell)?,
        Piece::Lock => crate::locks::build(sim, cell),
    }
    Ok(Some(p))
}

pub fn gate_of(sim: &SimState, building: BuildingId, cell: Cell) -> Option<Gate> {
    sim.buildings.building(building)?.gates().iter().find(|g| g.cell == cell).copied()
}

pub fn set_gate(sim: &mut SimState, building: BuildingId, cell: Cell, setting: GateSetting) -> Result<(), SimError> {
    if let Some(k) = crate::canals::canal(sim, building) {
        if k.gate.cell != cell { return Err(invalid("no gate there")); }
        crate::canals::set_gate(sim, building, setting)
    } else {
        crate::dams::set_gate(sim, building, cell, setting)
    }
}

pub fn remove(sim: &mut SimState, building: BuildingId, cell: Cell) -> Result<(), SimError> {
    gate_of(sim, building, cell).ok_or_else(|| invalid("no gate there"))?;
    if let Some(k) = crate::canals::canal(sim, building) {
        let cells = crate::canals::gate_cells(k);
        let k = crate::canals::canal_mut(sim, building)?;
        (k.gate.built, k.gate.pending, k.gate.setting, k.gate.lock) = (false, None, GateSetting::Shut, LockState::None);
        for c in cells { sim.water.clear_structure(c).map_err(|e| invalid(format!("{e:?}")))?; }
    } else {
        let d = crate::dams::dam(sim, building).ok_or_else(|| invalid("no such dam"))?;
        let cells = crate::dams::gate_cells(d, cell);
        crate::dams::drop_gate(sim, building, cell)?;
        for c in cells { sim.water.unset_gate(c).map_err(|e| invalid(format!("{e:?}")))?; }
    }
    crate::supply::tidy(sim);
    Ok(())
}

pub fn ready_at(sim: &SimState, building: BuildingId, cell: Cell) -> bool {
    gate_of(sim, building, cell).is_some_and(|g| visit_ready(sim, building, &g))
}
