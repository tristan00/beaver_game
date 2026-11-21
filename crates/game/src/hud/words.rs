use crate::build_logic::{Group, PlaceError};
use crate::icons::Icon;
use bevy::prelude::Color;
use sim::buildings::{BuildingKind, Good};
use sim::calendar::Season;
use sim::resources::ResourceKind;
use sim::work::Role;
use sim::jobs::{self, RoleKind};

pub fn good_name(g: Good) -> &'static str {
    match g {
        Good::Sticks => "Sticks",
        Good::Clay => "Clay",
        Good::Food => "Food",
    }
}

pub fn good_icon(g: Good) -> Icon {
    match g {
        Good::Sticks => Icon::Sticks,
        Good::Clay => Icon::Mud,
        Good::Food => Icon::Food,
    }
}

pub fn good_color(g: Good) -> Color {
    match g {
        Good::Sticks => Color::srgb(0.88, 0.76, 0.52),
        Good::Clay => Color::srgb(0.66, 0.6, 0.42),
        Good::Food => Color::srgb(0.9, 0.42, 0.5),
    }
}

pub fn building_icon(k: BuildingKind) -> Icon {
    use BuildingKind::*;
    match k {
        Dam => Icon::Dam,
        Shelter => Icon::Lodge,
        Warehouse => Icon::Warehouse,
    }
}

pub fn group_icon(g: Group) -> Icon {
    match g {
        Group::Water => Icon::CatWater,
        Group::Home => Icon::CatBuilding,
    }
}

pub fn resource_icon(k: ResourceKind) -> Icon {
    match k {
        ResourceKind::Tree | ResourceKind::Felled => Icon::Tree,
        ResourceKind::Plant => Icon::Plant,
        ResourceKind::MudBank => Icon::MudBank,
        ResourceKind::RiverClay => Icon::MudBank,
    }
}

pub fn resource_name(k: ResourceKind) -> &'static str {
    match k {
        ResourceKind::Tree => "Tree",
        ResourceKind::Felled => "Felled tree",
        ResourceKind::Plant => "Edible shoots",
        ResourceKind::MudBank => "Clay deposit",
        ResourceKind::RiverClay => "River clay",
    }
}

pub fn season_icon(s: Season) -> Icon {
    match s {
        Season::Spring => Icon::Spring,
        Season::Summer => Icon::Summer,
        Season::Autumn => Icon::Autumn,
        Season::Winter => Icon::Winter,
    }
}

pub fn signed(r: f32) -> String {
    if r.abs() < 0.05 {
        "0".into()
    } else if r > 0.0 {
        format!("+{r:.1}")
    } else {
        format!("-{:.1}", -r)
    }
}

pub fn num(v: f32) -> String {
    let a = v.abs();
    if a >= 100.0 || (a - a.round()).abs() < 0.05 {
        format!("{:.0}", v)
    } else {
        format!("{v:.1}")
    }
}

pub fn days(d: f32) -> String {
    format!("{}d", d.max(0.0).floor() as i64)
}

pub fn place_error(e: &PlaceError) -> String {
    match e {
        PlaceError::Locked(t) => (*t).to_string(),
        PlaceError::OutOfBounds => "Off map".into(),
        PlaceError::Blocked => "Occupied".into(),
        PlaceError::NeedsDry => "Wet".into(),
        PlaceError::NeedsWater => "No water".into(),
        PlaceError::NeedsPond => "Needs a pond".into(),
        PlaceError::NoCells => String::new(),
        PlaceError::TooLong(max) => format!("Max {max}"),
        PlaceError::Sim(m) => sim_error(m),
    }
}

pub fn sim_error(m: &str) -> String {
    let l = m.to_lowercase();
    let table: &[(&str, &str)] = &[
        ("in the way", "Needs lock"),
        ("two different docks", "Same dock"),
        ("at least one good", "No goods"),
        ("no water joins", "No water"),
        ("cannot reach the first dock", "Unreachable"),
        ("deep enough", "Too shallow"),
        ("not a finished dock", "Needs dock"),
        ("lock is built on a dam", "Needs dam"),
        ("is too high", "Too high"),
        ("already that high", "Built"),
        ("dam height", "Crest 0.5-3"),
        ("not a valid dam line", "No pond"),
        ("kit", "Too young"),
        ("notadult", "Too young"),
        ("slottaken", "Taken"),
        ("only unfinished", "Finished"),
        ("haul needs", "No store"),
        ("nosuchworkplace", "Gone"),
        ("no building", "Gone"),
        ("no agent", "Unreachable"),
        ("invalidarea", "Bad area"),
        ("blocked", "Occupied"),
        ("out of bounds", "Off map"),
        ("disabled", "Locked"),
        ("(locked)", "Locked"),
    ];
    table.iter().find(|(k, _)| l.contains(k)).map(|(_, v)| (*v).to_string()).unwrap_or_else(|| "Refused".into())
}

fn role_presentation(role: RoleKind) -> (Icon, &'static str) {
    match role {
        RoleKind::Forager => (Icon::RoleForager, "Forager"),
        RoleKind::Logger => (Icon::RoleLogger, "Logger"),
        RoleKind::MudDigger => (Icon::RoleMudDigger, "Clay digger"),
        RoleKind::Builder => (Icon::RoleBuilder, "Builder"),
        RoleKind::Hauler => (Icon::RoleHauler, "Hauler"),
        RoleKind::Crafter => (Icon::RoleCrafter, "Crafter"),
        RoleKind::Forester => (Icon::RoleForester, "Forester"),
        RoleKind::Guard => (Icon::GuardPost, "Guard"),
        RoleKind::Scout => (Icon::Tree, "Scout"),
    }
}

pub fn role_kind_icon(role: RoleKind) -> Icon { role_presentation(role).0 }
pub fn role_kind_name(role: RoleKind) -> &'static str { role_presentation(role).1 }
pub fn role_icon(role: Role) -> Icon { role_kind_icon(jobs::role_kind(role)) }
pub fn role_name(role: Role) -> &'static str { role_kind_name(jobs::role_kind(role)) }
