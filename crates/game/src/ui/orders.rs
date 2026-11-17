//! Input-to-command mapping, placement checks and located alerts (owners: `commands`,
//! `buildmenu`, `events_ui`). All pure so gestures can be tested without a window.

use sim::buildings::{BuildingId, BuildingKind};
use sim::colony::BeaverId;
use sim::resources::ResourceKind;
use sim::society::RequestId;
use sim::water::Cell;
use sim::work::Area;
use sim::{Command, SimState};

/// What the player did on the map with beavers selected.
#[derive(Clone, Debug, PartialEq)]
pub enum Gesture {
    /// Right-click a resource of `kind` at `cell`.
    RightClickResource { kind: ResourceKind, cell: Cell },
    /// Right-click a construction site.
    RightClickSite { building: BuildingId },
    /// Drag a rectangle to collect `kind` inside it.
    DragArea { kind: ResourceKind, area: Area },
    /// Drag a workplace's area to a new place or size.
    EditWorkArea { building: BuildingId, area: Area },
    /// Answer a join request.
    Answer { id: RequestId, accept: bool },
}

/// The command a gesture produces for the selected beavers; `None` if nothing applies. (`commands` task.)
pub fn gesture_command(_sim: &SimState, _selected: &[BeaverId], _g: &Gesture) -> Option<Command> {
    None
}

/// Result of hovering a building at a cell.
#[derive(Clone, Debug, PartialEq)]
pub enum Placement {
    Valid,
    /// Invalid, with the reason shown on the preview.
    Invalid(&'static str),
}

/// Whether `kind` may be placed with its anchor at `cell`, per its `Site` rule, footprint, solid
/// cells and cost. (`buildmenu` task.)
pub fn placement(_sim: &SimState, _kind: BuildingKind, _cell: Cell) -> Placement {
    Placement::Invalid("not implemented")
}

// --- events_ui ---

pub use super::events::{alerts, Alert, AlertKind};
