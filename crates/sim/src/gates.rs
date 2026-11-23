//! A gate is one cell of a dam, or a canal's intake, whose height the player sets.
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
    /// Hold the water at this elevation (metres): only what rises above it passes.
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

/// Use sticks from the colony's nearest stores; nothing is taken unless all are there.
pub fn take_sticks(sim: &mut SimState, near: Cell, amount: f32) -> bool {
    use crate::buildings::{Good, Use};
    let mut holders = crate::stock::holders(sim, Good::Sticks);
    let have: f32 = holders.iter().map(|&id| sim.work.stock_of(id, Good::Sticks)).sum();
    if have + 1e-4 < amount {
        return false;
    }
    let dist = |sim: &SimState, id| {
        if id == crate::agents::SUPPLIES {
            sim.work.supplies.map_or(usize::MAX, |p: Cell| near.0.abs_diff(p.0) + near.1.abs_diff(p.1))
        } else {
            sim.buildings.building(id).map_or(usize::MAX, |b| crate::stock::dist(near, b))
        }
    };
    holders.sort_by_key(|&id| (dist(sim, id), id));
    let mut left = amount;
    for id in holders {
        if left <= 1e-4 {
            break;
        }
        let here = sim.work.stock_of(id, Good::Sticks).min(left);
        if here > 0.0 {
            left -= sim.buildings.stores.consume_from(id, Good::Sticks, here, Use::Sites);
        }
    }
    left <= 1e-3
}
