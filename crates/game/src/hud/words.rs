use crate::build_logic::PlaceError;
use crate::icons::Icon;
use bevy::prelude::Color;
use sim::buildings::{BuildingKind, Good};
use sim::calendar::Season;
use sim::resources::ResourceKind;
use sim::jobs::Job;

/// A good's icon and colour, as its row gives them.
pub fn good_icon(g: Good) -> Icon {
    Icon::named(&g.row().icon).unwrap_or_else(|| panic!("good {}: no icon {}", g.key(), g.row().icon))
}

/// A cost as tooltip lines, one per good.
pub fn cost_lines(mut t: crate::hud::ui::Tip, cost: &[(Good, f32)]) -> crate::hud::ui::Tip {
    for &(g, a) in cost {
        t = t.line(g.name(), num(a));
    }
    t
}

/// A cost as a button's chip: its first good's icon and the whole amount.
pub fn cost_chip(cost: &[(Good, f32)]) -> Option<(Icon, Color, String)> {
    let g = cost.first()?.0;
    Some((good_icon(g), good_color(g), format!("{}", cost.iter().map(|c| c.1).sum::<f32>().round() as i64)))
}

/// Two costs added together, good by good.
pub fn cost_sum(a: &[(Good, f32)], b: &[(Good, f32)]) -> Vec<(Good, f32)> {
    let mut out = a.to_vec();
    for &(g, x) in b {
        match out.iter_mut().find(|c| c.0 == g) {
            Some(c) => c.1 += x,
            None => out.push((g, x)),
        }
    }
    out
}

/// "<prefix> <word in lower case>", made once for each pair.
pub fn phrase(prefix: &'static str, word: &'static str) -> &'static str {
    use std::{collections::HashMap, sync::Mutex};
    static MADE: Mutex<Option<HashMap<(&'static str, &'static str), &'static str>>> = Mutex::new(None);
    let mut made = MADE.lock().expect("phrases");
    made.get_or_insert_with(HashMap::new).entry((prefix, word)).or_insert_with(|| Box::leak(format!("{prefix} {}", word.to_lowercase()).into_boxed_str()))
}

pub fn good_phrase(prefix: &'static str, g: Good) -> &'static str {
    phrase(prefix, g.name())
}

pub fn good_color(g: Good) -> Color {
    let [r, gr, b] = g.row().colour;
    Color::srgb(r, gr, b)
}

pub fn building_icon(k: BuildingKind) -> Icon {
    Icon::named(&k.row().icon).unwrap_or_else(|| panic!("building {}: no icon {}", k.key(), k.row().icon))
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
            Food::Plant(_) => (crate::hud::words::resource_icon(sim::resources::ResourceKind::food_plant(false)), "Eating a plant"),
            Food::Sapling(_) => (crate::hud::words::good_icon(sim::buildings::Good::seedling()), "Eating a sapling"),
            Food::Pile(_) => (crate::hud::words::good_icon(sim::buildings::Good::food()), "Eating dropped food"),
        },
        Doing::Rest { .. } => (Icon::Idle, "Resting"),
        Doing::Flee => (Icon::Fast2, "Fleeing"),
        Doing::Leave => (Icon::Walk, "Leaving"),
        Doing::Caught => (Icon::Wolf, "Caught by the wolf"),
    }
}

pub fn resource_icon(k: ResourceKind) -> Icon {
    Icon::named(&k.row().icon).unwrap_or_else(|| panic!("resource {}: no icon {}", k.key(), k.row().icon))
}

pub fn resource_name(k: ResourceKind) -> &'static str {
    k.name()
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
        ("at least one good", "No goods"),
        ("no water joins", "No water"),
        ("deep enough", "Too shallow"),
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

/// A job's icon, as its row names it.
pub fn job_icon(j: Job) -> Icon {
    Icon::named(&j.row().icon).unwrap_or_else(|| panic!("job {}: no icon {}", j.key(), j.row().icon))
}

/// A beaver's name with its epithet, if it has earned one.
pub fn full_name(s: &sim::SimState, id: sim::colony::BeaverId, name: &str) -> String {
    sim::deeds::full_name(s, id, name)
}
