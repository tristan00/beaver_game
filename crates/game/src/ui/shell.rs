//! Map-first shell state (owner: `ui_shell`). The map fills the screen; the toolbar opens one
//! menu at a time (Build, Gather, Jobs, Screens) and **Screens** opens one centred ledger screen.
//! Pure view-model: tested without a window; drawing lives in `toolbar.rs`, `buildmenu.rs`,
//! `jobs.rs` and `panels.rs`.
//!
//! Screens, as named in `docs/UI_SPEC.md` §1: **Colony** (beavers, families, needs, mood),
//! **Stores** (goods and rates), **Buildings** (lodges, sites), plus **Tech** and **Events**.
//! Work is the Jobs panel (`HudTool::Jobs`), not a screen.

use sim::features::{Features, System};

use super::icons::Icon;

/// An entry on the navigation bar.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum NavItem {
    Colony,
    Stores,
    Buildings,
    Tech,
    Events,
}

impl NavItem {
    pub const ALL: [NavItem; 5] = [NavItem::Colony, NavItem::Stores, NavItem::Buildings, NavItem::Tech, NavItem::Events];
    pub fn icon(self) -> Icon {
        match self {
            NavItem::Colony => Icon::NavColony,
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

/// A button on the bottom toolbar (zone Z6). Build, Gather, Jobs and Screens open a menu or panel;
/// Haul and Demolish are listed but off until the sim can do them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum HudTool {
    Build,
    Gather,
    Haul,
    Jobs,
    Demolish,
    Screens,
}

impl HudTool {
    pub const ALL: [HudTool; 6] = [HudTool::Build, HudTool::Gather, HudTool::Haul, HudTool::Jobs, HudTool::Demolish, HudTool::Screens];
    pub fn label(self) -> &'static str {
        match self {
            HudTool::Build => "Build",
            HudTool::Gather => "Gather",
            HudTool::Haul => "Haul",
            HudTool::Jobs => "Jobs",
            HudTool::Demolish => "Demolish",
            HudTool::Screens => "Screens",
        }
    }
    pub fn icon(self) -> Icon {
        match self {
            HudTool::Build => Icon::Build,
            HudTool::Gather => Icon::Collect,
            HudTool::Haul => Icon::Haul,
            HudTool::Jobs => Icon::NavBeavers,
            HudTool::Demolish => Icon::Demolish,
            HudTool::Screens => Icon::NavColony,
        }
    }
    /// The one key that opens it (`docs/UI_SPEC.md` §7); Screens has none.
    pub fn key(self) -> Option<char> {
        match self {
            HudTool::Build => Some('B'),
            HudTool::Gather => Some('G'),
            HudTool::Haul => Some('H'),
            HudTool::Jobs => Some('J'),
            HudTool::Demolish => Some('X'),
            HudTool::Screens => None,
        }
    }
    /// Why the button does nothing, if it does nothing.
    pub fn off_reason(self) -> Option<&'static str> {
        match self {
            HudTool::Haul => Some("Standing hauls are not in the game yet: haulers carry goods on their own."),
            HudTool::Demolish => Some("Buildings cannot be taken down yet."),
            _ => None,
        }
    }
}

/// What is on screen besides the map, toolbar and time controls.
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
    /// The toolbar menu or panel that is open, if any (one at a time).
    pub tool: Option<HudTool>,
    /// The build menu's open tab; `None` is Basics.
    pub build_tab: Option<crate::build_logic::Group>,
}

impl Shell {
    /// Choosing a screen from the Screens list opens it, or closes it if already open; the list closes.
    pub fn toggle(&mut self, item: NavItem) {
        self.open = if self.open == Some(item) { None } else { Some(item) };
        self.tool = None;
    }
    pub fn close(&mut self) {
        self.open = None;
    }
    /// A toolbar click: opens that menu, or closes it if it is the open one. Tools that are off do
    /// nothing. Opening a menu closes the open screen; one menu at a time.
    pub fn toggle_tool(&mut self, t: HudTool) {
        if t.off_reason().is_some() {
            return;
        }
        self.tool = if self.tool == Some(t) { None } else { Some(t) };
        if self.tool.is_some() {
            self.open = None;
            self.armed = None;
            self.editor = None;
        }
    }
    /// Items for the Screens list for these features.
    pub fn bar(&self, features: &Features) -> Vec<NavItem> {
        NavItem::ALL.iter().copied().filter(|i| i.visible(features)).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn work_is_no_screen_and_the_toolbar_has_six_tools() {
        assert_eq!(NavItem::ALL.len(), 5);
        assert!(NavItem::ALL.iter().all(|n| n.label() != "Work"));
        assert_eq!(HudTool::ALL.map(|t| t.label()), ["Build", "Gather", "Haul", "Jobs", "Demolish", "Screens"]);
    }

    #[test]
    fn one_menu_at_a_time_and_a_click_on_the_open_one_closes_it() {
        let mut s = Shell::default();
        s.toggle_tool(HudTool::Build);
        assert_eq!(s.tool, Some(HudTool::Build));
        s.toggle_tool(HudTool::Jobs);
        assert_eq!(s.tool, Some(HudTool::Jobs));
        s.toggle_tool(HudTool::Jobs);
        assert_eq!(s.tool, None);
    }

    #[test]
    fn opening_a_menu_closes_the_screen_and_a_screen_closes_the_menu() {
        let mut s = Shell::default();
        s.toggle_tool(HudTool::Screens);
        s.toggle(NavItem::Stores);
        assert_eq!((s.open, s.tool), (Some(NavItem::Stores), None));
        s.toggle_tool(HudTool::Build);
        assert_eq!((s.open, s.tool), (None, Some(HudTool::Build)));
    }

    #[test]
    fn tools_the_sim_cannot_do_stay_off_with_a_reason() {
        let mut s = Shell::default();
        for t in [HudTool::Haul, HudTool::Demolish] {
            assert!(t.off_reason().is_some());
            s.toggle_tool(t);
            assert_eq!(s.tool, None);
        }
        assert!(HudTool::ALL.iter().filter(|t| t.off_reason().is_none()).count() == 4);
    }
}
