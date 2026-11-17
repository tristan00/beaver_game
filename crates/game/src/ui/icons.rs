//! Icon identities (owners: `icons` draws PNGs, `ui_shell` loads them). One enum variant per
//! icon in the style guide; `file()` is the asset name under `assets/icons/`. Text labels are used
//! only where an icon would be ambiguous.

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Icon {
    // resources
    Logs,
    Branches,
    Bark,
    Plants,
    Mud,
    Stone,
    Baskets,
    Rope,
    Tools,
    // needs
    Food,
    Shelter,
    Rest,
    Social,
    // jobs
    Collect,
    Carry,
    Build,
    Haul,
    Craft,
    // actions
    Assign,
    MoveArea,
    Accept,
    TurnAway,
    Cancel,
    Demolish,
    // map modes
    ModeWater,
    ModeForest,
    ModeFood,
    ModeWork,
    // navigation bar
    NavColony,
    NavBeavers,
    NavStores,
    NavBuild,
    NavTech,
    NavEvents,
}

impl Icon {
    /// File stem under `assets/icons/`, e.g. `logs.png`.
    pub fn file(self) -> String {
        format!("{}.png", format!("{self:?}").to_lowercase())
    }
}

impl From<sim::buildings::Good> for Icon {
    fn from(g: sim::buildings::Good) -> Icon {
        use sim::buildings::Good::*;
        match g {
            Logs => Icon::Logs,
            Sticks => Icon::Branches,
            Mud => Icon::Mud,
            Stone => Icon::Stone,
            Food => Icon::Food,
            Baskets => Icon::Baskets,
            Rope => Icon::Rope,
            Tools => Icon::Tools,
        }
    }
}
