use crate::build_logic::{Group, PlaceError};
use crate::icons::Icon;
use bevy::prelude::Color;
use sim::buildings::{BuildingKind, Good};
use sim::calendar::Season;
use sim::resources::ResourceKind;
use sim::work::Role;

pub fn good_name(g: Good) -> &'static str {
    match g {
        Good::Logs => "Logs",
        Good::Sticks => "Sticks",
        Good::Mud => "Mud",
        Good::Stone => "Stone",
        Good::Food => "Food",
        Good::Baskets => "Baskets",
        Good::Rope => "Rope",
        Good::Tools => "Tools",
    }
}

pub fn good_icon(g: Good) -> Icon {
    match g {
        Good::Logs => Icon::Logs,
        Good::Sticks => Icon::Sticks,
        Good::Mud => Icon::Mud,
        Good::Stone => Icon::Stone,
        Good::Food => Icon::Food,
        Good::Baskets => Icon::Baskets,
        Good::Rope => Icon::Rope,
        Good::Tools => Icon::Tools,
    }
}

pub fn good_color(g: Good) -> Color {
    match g {
        Good::Logs => Color::srgb(0.79, 0.54, 0.29),
        Good::Sticks => Color::srgb(0.88, 0.76, 0.52),
        Good::Mud => Color::srgb(0.66, 0.6, 0.42),
        Good::Stone => Color::srgb(0.66, 0.68, 0.7),
        Good::Food => Color::srgb(0.9, 0.42, 0.5),
        Good::Baskets => Color::srgb(0.8, 0.65, 0.3),
        Good::Rope => Color::srgb(0.76, 0.67, 0.52),
        Good::Tools => Color::srgb(0.6, 0.68, 0.76),
    }
}

pub fn building_icon(k: BuildingKind) -> Icon {
    use BuildingKind::*;
    match k {
        Dam => Icon::Dam,
        Lodge => Icon::Lodge,
        Burrow => Icon::Burrow,
        FoodCache => Icon::FoodCache,
        ScentMound => Icon::ScentMound,
        GuardPost => Icon::GuardPost,
        Canal => Icon::Canal,
        Lock => Icon::Lock,
        WaterWheel => Icon::WaterWheel,
        Workshop => Icon::Workshop,
        Warehouse => Icon::Warehouse,
        Dock => Icon::Dock,
        Barge => Icon::Barge,
        LoggingCamp => Icon::LoggingCamp,
        ForagerHut => Icon::ForagerHut,
        MudPit => Icon::MudPit,
        QuarryCamp => Icon::QuarryCamp,
        Mill => Icon::Mill,
        Storehouse => Icon::Storehouse,
        ForesterLodge => Icon::ForesterLodge,
        TreeNursery => Icon::TreeNursery,
    }
}

pub fn group_icon(g: Group) -> Icon {
    use sim::tech::Branch;
    match g {
        Group::Base => Icon::CatBasics,
        Group::Branch(Branch::Water) => Icon::CatWater,
        Group::Branch(Branch::Forest) => Icon::CatForest,
        Group::Branch(Branch::Building) => Icon::CatBuilding,
        Group::Branch(Branch::Craft) => Icon::CatCraft,
        Group::Branch(Branch::Transport) => Icon::CatTransport,
        Group::Branch(Branch::Defence) => Icon::CatDefence,
    }
}

pub fn role_icon(r: Role) -> Icon {
    match r {
        Role::Collect(k) => match k {
            ResourceKind::Tree | ResourceKind::Felled => Icon::RoleLogger,
            ResourceKind::Plant => Icon::RoleForager,
            ResourceKind::MudBank => Icon::RoleMudDigger,
            ResourceKind::StoneOutcrop => Icon::RoleQuarrier,
        },
        Role::Build => Icon::RoleBuilder,
        Role::Haul => Icon::RoleHauler,
        Role::Craft => Icon::RoleCrafter,
        Role::Forester => Icon::RoleForester,
    }
}

pub fn role_name(r: Role) -> &'static str {
    match r {
        Role::Collect(k) => match k {
            ResourceKind::Tree | ResourceKind::Felled => "Logger",
            ResourceKind::Plant => "Forager",
            ResourceKind::MudBank => "Mud digger",
            ResourceKind::StoneOutcrop => "Quarrier",
        },
        Role::Build => "Builder",
        Role::Haul => "Hauler",
        Role::Craft => "Crafter",
        Role::Forester => "Forester",
    }
}

pub fn resource_icon(k: ResourceKind) -> Icon {
    match k {
        ResourceKind::Tree | ResourceKind::Felled => Icon::Tree,
        ResourceKind::Plant => Icon::Plant,
        ResourceKind::MudBank => Icon::MudBank,
        ResourceKind::StoneOutcrop => Icon::Outcrop,
    }
}

pub fn resource_name(k: ResourceKind) -> &'static str {
    match k {
        ResourceKind::Tree => "Tree",
        ResourceKind::Felled => "Felled tree",
        ResourceKind::Plant => "Food plant",
        ResourceKind::MudBank => "Mud bank",
        ResourceKind::StoneOutcrop => "Outcrop",
    }
}

pub fn tree_stage_icon(st: sim::forestry::TreeStage) -> Icon {
    use sim::forestry::TreeStage::*;
    match st {
        Sapling => Icon::Sapling,
        Young => Icon::TreeYoung,
        Mature => Icon::Tree,
        Old => Icon::TreeOld,
        Snag => Icon::Coppice,
    }
}

pub fn tree_stage_name(st: sim::forestry::TreeStage) -> &'static str {
    use sim::forestry::TreeStage::*;
    match st {
        Sapling => "Saplings",
        Young => "Young",
        Mature => "Mature",
        Old => "Old",
        Snag => "Snags",
    }
}

pub fn cut_rule_face(r: sim::forestry::CutRule) -> (Icon, &'static str) {
    use sim::forestry::CutRule::*;
    match r {
        MatureOnly => (Icon::Tree, "Mature only"),
        ClearCut => (Icon::Clear, "Clear-cut"),
        Thin => (Icon::TreeOld, "Thin"),
        Coppice => (Icon::Coppice, "Coppice"),
    }
}

pub fn plant_rule_face(r: sim::forestry::PlantRule) -> (Icon, &'static str) {
    use sim::forestry::PlantRule::*;
    match r {
        FillGaps => (Icon::Sapling, "Fill gaps"),
        PlantAll => (Icon::ViewForest, "Plant all"),
        TendOnly => (Icon::Health, "Tend only"),
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
        PlaceError::Locked(t, _, _) => (*t).to_string(),
        PlaceError::OutOfBounds => "Off map".into(),
        PlaceError::Blocked => "Occupied".into(),
        PlaceError::NeedsDry => "Wet".into(),
        PlaceError::NeedsWater => "No water".into(),
        PlaceError::NeedsPond => "Needs a pond".into(),
        PlaceError::NeedsDock => "Needs dock".into(),
        PlaceError::NeedsDam => "Needs dam".into(),
        PlaceError::NoCells => String::new(),
        PlaceError::TooLong(max) => format!("Max {max}"),
        PlaceError::Sim(m) => sim_error(m),
    }
}

pub fn blocking_dam(m: &str) -> Option<(usize, usize)> {
    let rest = m.split("a dam at ").nth(1)?;
    let xy = rest.split_whitespace().next()?;
    let (x, y) = xy.split_once(',')?;
    Some((x.parse().ok()?, y.trim_end_matches([',', ':']).parse().ok()?))
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

