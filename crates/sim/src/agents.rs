//! Beaver agents: bodies on the navigation grid and the task state machine (owner: `collect`;
//! `build` adds Build/Fetch/haul behaviour inside the same `step`). Person data (age, needs,
//! skills, family) stays in `colony::Beaver`; an `Agent` is keyed by the same `BeaverId`.
//!
//! State machine (at most one transition per tick):
//! ```text
//! Idle ──assigned──────▶ Collect (go, work until load full) ──▶ Carry (go to store, deposit) ──▶ Collect
//! Idle ──site slot─────▶ Fetch (store, take) ──▶ Carry (to site) ──▶ Build (work) ──▶ Fetch …
//! Idle ──haul order────▶ Fetch (from store) ──▶ Carry (to `to` store, deposit) ──▶ Fetch …
//! any ──tired/unassigned/kit──▶ Rest (go to lodge entrance, enter)
//! ```
//! Kits never leave Idle/Rest; unassigned adults rest. Totals change only on deposit.

use std::collections::BTreeMap;

use crate::buildings::{BuildingId, Good};
use crate::colony::BeaverId;
use crate::nav::{Gait, Path};
use crate::resources::{ResourceId, ResourceKind};
use crate::water::Cell;
use crate::work::Area;

/// What the beaver carries; a load is sim state so the renderer can show it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Load {
    pub good: Good,
    pub amount: f32,
}

/// Where a collector works.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Assignment {
    /// A specific resource entity (right-click).
    Resource(ResourceId),
    /// Any resource of `kind` inside `area` (drag).
    Area(ResourceKind, Area),
    /// Slot `slot` of a workplace (uses that workplace's area and role).
    Slot(BuildingId, usize),
}

#[derive(Clone, Debug, PartialEq)]
pub enum Task {
    Idle,
    /// Walking to / working at a resource until a load is full.
    Collect { target: ResourceId, progress: f32 },
    /// Walking to `dest` with `Agent::load`; on arrival the load is deposited.
    Carry { dest: BuildingId },
    /// Walking to a store to pick up `good`, then carrying it to `then` (site or store).
    Fetch { from: BuildingId, good: Good, then: BuildingId },
    /// Working on a site.
    Build { site: BuildingId },
    /// Walking to / sleeping in the home lodge.
    Rest,
}

impl Task {
    /// Short label for panels and number provenance.
    pub fn label(&self) -> &'static str {
        match self {
            Task::Idle => "idle",
            Task::Collect { .. } => "collecting",
            Task::Carry { .. } => "carrying",
            Task::Fetch { .. } => "fetching",
            Task::Build { .. } => "building",
            Task::Rest => "resting",
        }
    }
}

#[derive(Clone, Debug)]
pub struct Agent {
    pub id: BeaverId,
    /// Cell the agent occupies (always reserved in `NavGrid`).
    pub cell: Cell,
    /// Progress `0..1` from `cell` towards `path.steps[0]`, for smooth drawing.
    pub step_frac: f32,
    pub gait: Gait,
    pub task: Task,
    pub assignment: Option<Assignment>,
    pub load: Option<Load>,
    pub path: Option<Path>,
    /// True while inside the lodge (hidden from the map).
    pub inside: bool,
    /// Ticks without progress; above `STUCK_TICKS` tests report a bug.
    pub stuck_ticks: u32,
}

pub const STUCK_TICKS: u32 = 600;

#[derive(Clone, Debug, Default)]
pub struct Agents {
    pub by_id: BTreeMap<BeaverId, Agent>,
}

impl Agents {
    pub fn get(&self, id: BeaverId) -> Option<&Agent> {
        self.by_id.get(&id)
    }
    /// Spawns an agent on `cell` (caller guarantees it is free).
    pub fn spawn(&mut self, id: BeaverId, cell: Cell) {
        self.by_id.insert(
            id,
            Agent {
                id,
                cell,
                step_frac: 0.0,
                gait: Gait::Walk,
                task: Task::Idle,
                assignment: None,
                load: None,
                path: None,
                inside: false,
                stuck_ticks: 0,
            },
        );
    }
    /// Sets or clears a collection assignment; false if no such agent. Kits and unassigned adults
    /// do no work (enforced in `step`).
    pub fn assign(&mut self, id: BeaverId, a: Option<Assignment>) -> bool {
        match self.by_id.get_mut(&id) {
            Some(ag) => {
                ag.assignment = a;
                true
            }
            None => false,
        }
    }
}

/// One tick of every agent: pick task, path, move, work, deliver. Called once from
/// `SimState::step` after water and ecology; does nothing until the colony is founded.
/// (`collect` fills in; `build` adds Build/Fetch/haul inside it.)
pub fn step(_sim: &mut crate::SimState, _dt_days: f32) {}
