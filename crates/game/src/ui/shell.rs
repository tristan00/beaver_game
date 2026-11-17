//! Map-first shell and start-up menu models (owner: `ui_shell`). The map fills the screen; one
//! stable navigation bar opens and closes a single screen at a time. Pure view-model: tested
//! without a window; drawing lives in `panels.rs`.
//!
//! Screens, as named in `docs/DESIGN.md` §11: **Colony** (beavers, families, needs, mood),
//! **Work** (jobs and open orders), **Stores** (goods and rates), **Buildings** (lodges, sites),
//! plus **Tech** and **Events**.

use sim::features::{Features, System};
use sim::starts::Start;

use super::icons::Icon;

/// An entry on the navigation bar.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum NavItem {
    Colony,
    Work,
    Stores,
    Buildings,
    Tech,
    Events,
}

impl NavItem {
    pub const ALL: [NavItem; 6] = [NavItem::Colony, NavItem::Work, NavItem::Stores, NavItem::Buildings, NavItem::Tech, NavItem::Events];
    pub fn icon(self) -> Icon {
        match self {
            NavItem::Colony => Icon::NavColony,
            NavItem::Work => Icon::NavBeavers,
            NavItem::Stores => Icon::NavStores,
            NavItem::Buildings => Icon::NavBuild,
            NavItem::Tech => Icon::NavTech,
            NavItem::Events => Icon::NavEvents,
        }
    }
    /// Caption and tooltip label.
    pub fn label(self) -> &'static str {
        match self {
            NavItem::Colony => "Colony",
            NavItem::Work => "Work",
            NavItem::Stores => "Stores",
            NavItem::Buildings => "Buildings",
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
    /// An area-assign order waiting for the player's drag: gather this kind inside the rectangle.
    pub armed: Option<sim::resources::ResourceKind>,
    /// The workplace whose work-area editor is open.
    pub editor: Option<sim::buildings::BuildingId>,
}

impl Shell {
    /// Clicking a nav item opens it, or closes it if already open.
    pub fn toggle(&mut self, item: NavItem) {
        self.open = if self.open == Some(item) { None } else { Some(item) };
    }
    pub fn close(&mut self) {
        self.open = None;
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

#[cfg(test)]
mod start_menu_tests {
    use super::*;

    #[test]
    fn menu_lists_starts_one_two_three_in_order() {
        let cards = start_menu();
        let nums: Vec<u8> = cards.iter().map(|c| c.start.number()).collect();
        assert_eq!(nums, [1, 2, 3]);
        assert!(cards.iter().all(|c| !c.title.is_empty() && !c.pitch.is_empty()));
    }
}
