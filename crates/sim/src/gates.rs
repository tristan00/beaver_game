//! A gate is one cell of a dam's line (and the cells behind it on a widened dam), or a canal's intake, whose height the player sets.
//!
//! The water grid sees a gate as an ordinary dam cell with a sill, so every water engine
//! handles it unchanged; the gate flag only stops overtopping from wearing it away.
//! Shut stands at the gate's top, Open at its floor, and Level at a chosen sill between.
use crate::canals::GATE_STEP;
use crate::water::Cell;
use crate::{SimError, SimState};

#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum GateSetting {
    Shut,
    /// Hold the water at this elevation (metres): only what rises above it flows on.
    Level(f32),
    Open,
}

impl GateSetting {
    pub fn name(self) -> &'static str {
        match self {
            GateSetting::Shut => "shut",
            GateSetting::Level(_) => "level",
            GateSetting::Open => "open",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Gate {
    pub cell: Cell,
    /// Elevation of the gate's floor: the sill when Open.
    pub floor: f32,
    /// Elevation of the gate's top: the sill when Shut.
    pub top: f32,
    pub setting: GateSetting,
    /// A setting the player asked for; a beaver walks over and makes the change.
    pub pending: Option<GateSetting>,
    /// Built once its sticks are in place; until then the cell is ground or dam.
    pub built: bool,
}

impl Gate {
    pub fn new(cell: Cell, floor: f32, top: f32) -> Gate {
        Gate { cell, floor, top: top.max(floor), setting: GateSetting::Shut, pending: None, built: false }
    }

    /// Sill elevation for a setting; Level snaps to the gate's 5 cm steps.
    pub fn sill_of(&self, s: GateSetting) -> f32 {
        match s {
            GateSetting::Shut => self.top,
            GateSetting::Open => self.floor,
            GateSetting::Level(e) => {
                let snapped = self.floor + ((e - self.floor) / GATE_STEP).round() * GATE_STEP;
                snapped.clamp(self.floor, self.top)
            }
        }
    }

    pub fn sill(&self) -> f32 {
        self.sill_of(self.setting)
    }

    /// The setting the gate is heading for.
    pub fn wanted(&self) -> GateSetting {
        self.pending.unwrap_or(self.setting)
    }
}

/// Put a built gate's sill into the water grid.
pub fn apply(sim: &mut SimState, g: &Gate) -> Result<(), SimError> {
    if !g.built {
        return Ok(());
    }
    let ground = sim.height.get(g.cell.0, g.cell.1).copied().unwrap_or(0.0);
    sim.water.set_gate(g.cell, (g.sill() - ground).max(0.0)).map_err(|e| SimError::InvalidCommand(e.to_string()))
}

/// Ask for a new setting. Nothing changes until a builder visits the gate.
pub fn request(g: &mut Gate, setting: GateSetting) -> Result<(), SimError> {
    if matches!(setting, GateSetting::Level(e) if !e.is_finite()) {
        return Err(SimError::InvalidCommand("gate level must be finite".into()));
    }
    let same = g.built && std::mem::discriminant(&g.setting) == std::mem::discriminant(&setting) && (g.sill_of(setting) - g.sill()).abs() < 1e-4;
    g.pending = if same { None } else { Some(setting) };
    Ok(())
}

/// The piece of work at a gate, done in this order: build the gate, turn a built one as asked,
/// build the lock ordered on it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Piece {
    Build,
    Turn,
    Lock,
}

/// The work at a gate now, if any.
pub fn piece(sim: &SimState, building: crate::buildings::BuildingId, g: &Gate) -> Option<Piece> {
    if !g.built {
        // A dam's gate is wanted once placed; a canal's once a setting is asked for.
        let wanted = crate::canals::canal(sim, building).is_none() || g.pending.is_some();
        return wanted.then_some(Piece::Build);
    }
    if g.pending.is_some() {
        return Some(Piece::Turn);
    }
    crate::locks::pending(sim, g.cell).then_some(Piece::Lock)
}

/// What a piece uses, brought to the gate, and the labour it takes.
pub fn cost(p: Piece) -> (&'static [crate::buildings::Amount], f32) {
    match p {
        Piece::Build => (&crate::content::content().works.gate, crate::canals::GATE_WORK),
        Piece::Turn => (&[], crate::canals::GATE_WORK),
        Piece::Lock => (&crate::content::content().works.lock, crate::canals::GATE_WORK),
    }
}

/// How much of a piece's labour can be done with what has been brought so far: work keeps
/// pace with deliveries, as on a building site.
fn labour_room(sim: &SimState, spot: crate::supply::Spot, p: Piece) -> f32 {
    let (materials, labour) = cost(p);
    let need: f32 = materials.iter().map(|a| a.1).sum();
    let got: f32 = materials.iter().map(|&(g, a)| crate::supply::delivered(sim, spot, g).min(a)).sum();
    let share = if need > 0.0 { (got / need).min(1.0) } else { 1.0 };
    labour * share
}

/// Whether a builder at this gate has work it can do now: some of the piece's materials are
/// there and not yet worked in, or it needs none. A visit is never handed out to stand idle.
pub fn visit_ready(sim: &SimState, building: crate::buildings::BuildingId, g: &Gate) -> bool {
    let spot = crate::supply::Spot { building, cell: g.cell };
    piece(sim, building, g).is_some_and(|p| crate::supply::labour(sim, spot) < labour_room(sim, spot, p) - 1e-3)
}

/// A builder's work at a gate: `amount` of labour, as far as what has been brought allows.
/// When the piece's labour is all done and its materials all there, it is finished: the
/// materials are used, and the gate is built or turned, or the lock built. Returns the piece
/// finished.
pub fn work(sim: &mut SimState, building: crate::buildings::BuildingId, cell: Cell, amount: f32) -> Result<Option<Piece>, SimError> {
    let g = gate_of(sim, building, cell).ok_or_else(|| SimError::InvalidCommand("no gate there".into()))?;
    let p = piece(sim, building, &g).ok_or_else(|| SimError::InvalidCommand("nothing to do at the gate".into()))?;
    let spot = crate::supply::Spot { building, cell };
    let done = (crate::supply::labour(sim, spot) + amount).min(labour_room(sim, spot, p));
    crate::supply::set_labour(sim, spot, done);
    let (materials, labour) = cost(p);
    if done + 1e-3 < labour || !crate::supply::take(sim, spot, materials) {
        return Ok(None);
    }
    crate::supply::set_labour(sim, spot, 0.0);
    match p {
        Piece::Build | Piece::Turn if crate::canals::canal(sim, building).is_some() => crate::canals::turn_gate(sim, building)?,
        Piece::Build | Piece::Turn => crate::dams::work_gate(sim, building, cell)?,
        Piece::Lock => crate::locks::build(sim, cell),
    }
    Ok(Some(p))
}

/// The gate of a dam or canal at a cell.
pub fn gate_of(sim: &SimState, building: crate::buildings::BuildingId, cell: Cell) -> Option<Gate> {
    match (crate::canals::canal(sim, building), crate::dams::dam(sim, building)) {
        (Some(k), _) => Some(k.gate).filter(|g| g.cell == cell),
        (_, Some(d)) => d.gates.iter().find(|g| g.cell == cell).copied(),
        _ => None,
    }
}

/// Take a gate off its dam or canal, and the lock on it with it. A dam's gate cells become
/// ordinary dam cells again (builders raise them back to the dam's height); a canal's intake
/// runs free. What was brought for it goes back to the stores (see `supply::tidy`).
pub fn remove(sim: &mut SimState, building: crate::buildings::BuildingId, cell: Cell) -> Result<(), SimError> {
    let bad = |m: &str| SimError::InvalidCommand(m.into());
    gate_of(sim, building, cell).ok_or_else(|| bad("no gate there"))?;
    if let Some(k) = crate::canals::canal(sim, building) {
        let cells = crate::canals::gate_cells(k);
        let k = crate::canals::canal_mut(sim, building)?;
        k.gate.built = false;
        k.gate.pending = None;
        k.gate.setting = GateSetting::Shut;
        for c in cells {
            sim.water.clear_structure(c).map_err(|e| bad(&format!("{e:?}")))?;
        }
    } else {
        let d = crate::dams::dam(sim, building).ok_or_else(|| bad("no such dam"))?;
        let cells = crate::dams::gate_cells(d, cell);
        crate::dams::drop_gate(sim, building, cell)?;
        for c in cells {
            sim.water.unset_gate(c).map_err(|e| bad(&format!("{e:?}")))?;
        }
    }
    sim.locks.at.remove(&cell);
    crate::supply::tidy(sim);
    Ok(())
}

/// `visit_ready` for the gate of a dam or canal at a cell.
pub fn ready_at(sim: &SimState, building: crate::buildings::BuildingId, cell: Cell) -> bool {
    gate_of(sim, building, cell).is_some_and(|g| visit_ready(sim, building, &g))
}
