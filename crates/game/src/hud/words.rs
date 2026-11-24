use crate::build_logic::PlaceError;
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
        Good::Pebbles => "Pebbles",
        Good::Saplings => "Saplings",
    }
}

pub fn good_icon(g: Good) -> Icon {
    match g {
        Good::Sticks => Icon::Sticks,
        Good::Clay => Icon::Mud,
        Good::Food => Icon::Food,
        Good::Pebbles => Icon::Pebbles,
        Good::Saplings => Icon::Sapling,
    }
}

pub fn good_color(g: Good) -> Color {
    match g {
        Good::Sticks => Color::srgb(0.88, 0.76, 0.52),
        Good::Clay => Color::srgb(0.66, 0.6, 0.42),
        Good::Food => Color::srgb(0.9, 0.42, 0.5),
        Good::Pebbles => Color::srgb(0.7, 0.71, 0.74),
        Good::Saplings => Color::srgb(0.52, 0.8, 0.42),
    }
}

pub fn building_icon(k: BuildingKind) -> Icon {
    use BuildingKind::*;
    match k {
        Dam => Icon::Dam,
        Shelter => Icon::Lodge,
        Warehouse => Icon::Warehouse,
        Canal => Icon::Canal,
        Larder => Icon::Larder,
        Toolshed => Icon::Toolshed,
        Forester => Icon::Forester,
        Logistics => Icon::Logistics,
        Lookout => Icon::Lookout,
        Harbor => Icon::Harbor,
    }
}

/// The button that cancels a building site, or takes a whole dam or canal away (which stay
/// works until removed, so they keep it once finished).
pub fn cancel_build(r: &mut super::ui::Ui, s: &sim::SimState, id: sim::buildings::BuildingId) {
    let Some(b) = s.buildings.building(id) else { return };
    let works = b.dam.is_some() || b.canal.is_some();
    if !works && b.is_complete() {
        return;
    }
    let what = if b.dam.is_some() { "dam" } else if b.canal.is_some() { "canal" } else { "site" };
    let tip = if !works {
        super::ui::Tip::name("Cancel").line("Delivered materials come back", "")
    } else if s.finished(b) {
        super::ui::Tip::name(format!("Remove the {what}")).line(if b.dam.is_some() { "The whole dam comes down" } else { "Dug ground stays dug" }, "")
    } else {
        super::ui::Tip::name(format!("Cancel the {what}")).line(if b.dam.is_some() { "Built cells come down" } else { "Dug ground stays dug" }, "")
    };
    r.pebble(Icon::Remove, 12.0, super::HudAction::CancelBuild(id), super::ui::Peb::Idle).tip(tip);
}

pub fn animal_icon(k: sim::wildlife::Kind) -> Icon {
    match k {
        sim::wildlife::Kind::Deer => Icon::Deer,
        sim::wildlife::Kind::Rabbit => Icon::Rabbit,
        sim::wildlife::Kind::Bird => Icon::Bird,
    }
}

pub fn animal_name(k: sim::wildlife::Kind) -> &'static str {
    match k {
        sim::wildlife::Kind::Deer => "Deer",
        sim::wildlife::Kind::Rabbit => "Rabbit",
        sim::wildlife::Kind::Bird => "Bird",
    }
}

/// What a wild animal is doing, as an icon and a few words.
pub fn animal_doing(a: &sim::wildlife::Animal) -> (Icon, &'static str) {
    use sim::wildlife::{Doing, Food};
    match a.doing {
        Doing::Wander => (Icon::Walk, "Wandering"),
        Doing::Feed { food, .. } | Doing::Digest { food, .. } => match food {
            Food::Plant(_) => (Icon::Plant, "Eating a plant"),
            Food::Sapling(_) => (Icon::Sapling, "Eating a sapling"),
            Food::Pile(_) => (Icon::Food, "Eating dropped food"),
        },
        Doing::Rest { .. } => (Icon::Idle, "Resting"),
        Doing::Flee => (Icon::Fast2, "Fleeing"),
        Doing::Leave => (Icon::Walk, "Leaving"),
        Doing::Caught => (Icon::Wolf, "Caught by the wolf"),
    }
}

pub fn resource_icon(k: ResourceKind) -> Icon {
    match k {
        ResourceKind::Tree | ResourceKind::Felled => Icon::Tree,
        ResourceKind::Plant => Icon::Plant,
        ResourceKind::WaterPlant => Icon::WaterPlant,
        ResourceKind::MudBank => Icon::MudBank,
        ResourceKind::RiverClay => Icon::MudBank,
        ResourceKind::Pebbles => Icon::Pebbles,
    }
}

pub fn resource_name(k: ResourceKind) -> &'static str {
    match k {
        ResourceKind::Tree => "Tree",
        ResourceKind::Felled => "Felled tree",
        ResourceKind::Plant => "Food plants",
        ResourceKind::WaterPlant => "Water plants",
        ResourceKind::MudBank => "Clay deposit",
        ResourceKind::RiverClay => "River clay",
        ResourceKind::Pebbles => "Pebbles",
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

/// A game day as a date, "Y2 Spring 14": the one way the HUD writes a date.
pub fn date(day: f64) -> String {
    let d = sim::calendar::Date::at(day);
    format!("Y{} {} {}", d.year, d.season.name(), d.day)
}

/// A length of time in years and days, "1y 40d" (or "40d" under a year).
pub fn span(days: f64) -> String {
    let y = sim::calendar::YEAR_DAYS;
    let (years, rest) = ((days.max(0.0) / y).floor() as i64, (days.max(0.0) % y).floor() as i64);
    if years > 0 { format!("{years}y {rest}d") } else { format!("{rest}d") }
}

pub fn days(d: f32) -> String {
    format!("{}d", d.max(0.0).floor() as i64)
}

pub fn place_error(e: &PlaceError) -> String {
    match e {
        PlaceError::OutOfBounds => "Off map".into(),
        PlaceError::Blocked => "Occupied".into(),
        PlaceError::NeedsWater => "No water".into(),
        PlaceError::TooClose => "Too close".into(),
        PlaceError::BlocksDoor => "Blocks door".into(),
        PlaceError::NoCells => String::new(),
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
        ("too deep", "Too deep"),
        ("flooded", "Flooded"),
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
        RoleKind::Forester => (Icon::RoleForester, "Forester"),
        RoleKind::Maintainer => (Icon::RoleMaintainer, "Maintainer"),
        RoleKind::Guard => (Icon::RoleGuard, "Guard"),
        RoleKind::PebbleCollector => (Icon::Pebbles, "Pebble collector"),
    }
}

pub fn role_kind_icon(role: RoleKind) -> Icon { role_presentation(role).0 }
pub fn role_kind_name(role: RoleKind) -> &'static str { role_presentation(role).1 }
pub fn role_icon(role: Role) -> Icon {
    match role {
        Role::Collect(ResourceKind::Pebbles) => Icon::Pebbles,
        r => role_kind_icon(jobs::role_kind(r)),
    }
}
pub fn role_name(role: Role) -> &'static str {
    match role {
        Role::Collect(ResourceKind::Pebbles) => "Pebble collector",
        r => role_kind_name(jobs::role_kind(r)),
    }
}
