//! Entity selection and panels (owner: `panels`). `panel` is a pure function from sim state and a
//! selection to a view-model, so it is testable without a window.

use sim::buildings::BuildingId;
use sim::colony::{BeaverId, FamilyId};
use sim::resources::ResourceId;
use sim::Command;

use super::explain::{Explained, Numbers};
use super::icons::Icon;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Selection {
    Beaver(BeaverId),
    Family(FamilyId),
    /// A building or construction site (including a dam).
    Building(BuildingId),
    Tree(ResourceId),
}

/// A button on a panel; `command` is what clicking sends to `SimState::apply`.
#[derive(Clone, Debug, PartialEq)]
pub struct PanelAction {
    pub icon: Icon,
    pub label: &'static str,
    pub command: Option<Command>,
}

/// A worker slot row on building and site panels.
#[derive(Clone, Debug, PartialEq)]
pub struct SlotRow {
    pub role: &'static str,
    pub worker: Option<BeaverId>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Panel {
    pub title: String,
    /// One or two sentences of character; never mechanics.
    pub prose: String,
    pub stats: Vec<Explained>,
    pub slots: Vec<SlotRow>,
    pub actions: Vec<PanelAction>,
    /// Where the camera flies on "show on map".
    pub at: (usize, usize),
}

impl Numbers for Panel {
    fn numbers(&self) -> Vec<&Explained> {
        self.stats.iter().collect()
    }
}

/// Builds the panel for a selection; `None` if the entity no longer exists. (`panels` task.)
pub fn panel(_sim: &sim::SimState, _sel: Selection) -> Option<Panel> {
    None
}
