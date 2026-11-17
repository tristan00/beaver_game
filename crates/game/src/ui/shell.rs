//! Map-first shell and start-up menu models (owner: `ui_shell`). The map fills the screen; one
//! stable navigation bar opens and closes a single screen at a time. Pure view-model: tested
//! without a window; drawing lives in `panels.rs` (replaced by `ui_shell`).

use sim::features::{Features, System};
use sim::starts::Start;

use super::icons::Icon;

/// An entry on the navigation bar.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum NavItem {
    Colony,
    Beavers,
    Stores,
    Build,
    Tech,
    Events,
}

impl NavItem {
    pub const ALL: [NavItem; 6] =
        [NavItem::Colony, NavItem::Beavers, NavItem::Stores, NavItem::Build, NavItem::Tech, NavItem::Events];
    pub fn icon(self) -> Icon {
        match self {
            NavItem::Colony => Icon::NavColony,
            NavItem::Beavers => Icon::NavBeavers,
            NavItem::Stores => Icon::NavStores,
            NavItem::Build => Icon::NavBuild,
            NavItem::Tech => Icon::NavTech,
            NavItem::Events => Icon::NavEvents,
        }
    }
    /// Tooltip label.
    pub fn label(self) -> &'static str {
        match self {
            NavItem::Colony => "Colony",
            NavItem::Beavers => "Beavers",
            NavItem::Stores => "Stores",
            NavItem::Build => "Build",
            NavItem::Tech => "Tech",
            NavItem::Events => "Events",
        }
    }
    /// Entries naming a disabled system are hidden (none today; trade used to be one).
    pub fn visible(self, _features: &Features) -> bool {
        let _ = System::Trade;
        true
    }
}

/// What is on screen besides the map, nav bar and time controls.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Shell {
    /// The ledger or management screen currently open, if any.
    pub open: Option<NavItem>,
    /// The selected map entity (opens its panel).
    pub selection: Option<super::selection::Selection>,
    pub mode: super::mapmodes::MapMode,
}

impl Shell {
    /// Clicking a nav item opens it, or closes it if already open.
    pub fn toggle(&mut self, item: NavItem) {
        self.open = if self.open == Some(item) { None } else { Some(item) };
    }
    /// Items to draw on the bar for these features.
    pub fn bar(&self, features: &Features) -> Vec<NavItem> {
        NavItem::ALL.iter().copied().filter(|i| i.visible(features)).collect()
    }
}

/// One card on the start-up menu.
#[derive(Clone, Debug, PartialEq)]
pub struct StartCard {
    pub start: Start,
    pub title: &'static str,
    pub pitch: &'static str,
}

/// The cards the menu lists, in order (`--start N` skips the menu).
pub fn start_menu() -> Vec<StartCard> {
    Start::ALL
        .iter()
        .map(|s| {
            let (title, pitch) = s.title();
            StartCard { start: *s, title, pitch }
        })
        .collect()
}
