//! A beaver's whole story, full screen, opened from its card: who it is and how it is
//! (health, mood, every need, the harm it has taken), what it does, its family (each a link to
//! their own page), its home and who shares it, its skill at every job, and its life in the
//! colony day by day, newest first (see `sim::lives`).
use super::panels::{stage_icon, Ctx};
use super::ui::*;
use super::words::*;
use super::world::mood_icon;
use super::HudAction;
use crate::icons::Icon;
use bevy::prelude::*;
use sim::colony::{Beaver, BeaverId, DeathCause, Job, Stage};
use sim::lives::{Life, What};

/// Chronology rows shown at once (the wheel scrolls through the rest).
const ROWS: usize = 22;
const COL_W: f32 = 390.0;

fn name_of(s: &sim::SimState, id: BeaverId) -> String {
    s.colony.beaver(id).map(|b| b.name.clone()).or_else(|| s.lives.of.get(&id).map(|l| l.name.clone())).unwrap_or_else(|| "?".into())
}

/// "Year 2, day 140": the year of the colony and the day in it.
fn when(day: f64) -> String {
    let y = sim::calendar::YEAR_DAYS;
    format!("Y{} D{}", (day / y).floor() as i64 + 1, (day % y).floor() as i64 + 1)
}

fn years(days: f64) -> String {
    format!("{:.1} y", days / sim::calendar::YEAR_DAYS)
}

fn job_icon(j: Job) -> Icon {
    match j {
        Job::Forager => Icon::RoleForager,
        Job::Logger => Icon::RoleLogger,
        Job::Builder => Icon::RoleBuilder,
        Job::Digger => Icon::RoleMudDigger,
        Job::Hauler => Icon::RoleHauler,
    }
}

fn job_name(j: Job) -> &'static str {
    match j {
        Job::Forager => "Foraging (also forestry)",
        Job::Logger => "Logging",
        Job::Builder => "Building (also repairs)",
        Job::Digger => "Digging clay and pebbles",
        Job::Hauler => "Hauling",
    }
}

fn cause(c: DeathCause) -> &'static str {
    match c {
        DeathCause::Starvation => "starvation",
        DeathCause::Cold => "cold",
        DeathCause::Injury => "injury",
        DeathCause::Exhaustion => "exhaustion",
        DeathCause::OldAge => "old age",
        DeathCause::Predator => "killed by a wolf",
    }
}

fn stage_word(st: Stage) -> &'static str {
    match st {
        Stage::Kit => "Kit",
        Stage::Yearling => "Yearling",
        Stage::Adult => "Adult",
    }
}

fn level(v: f32) -> Color {
    if v < 0.35 {
        BAD
    } else if v < 0.6 {
        WARN
    } else {
        GOOD
    }
}

/// A section heading.
fn heading(u: &mut Ui, icon: Icon, words: &str) {
    u.row(8.0, |r| {
        r.icon(icon, 16.0, ACCENT);
        r.bold(words.to_string(), 14.0, INK);
    });
}

/// A labelled bar: icon, name, notches and the number.
fn meter(u: &mut Ui, icon: Icon, col: Color, name: &str, v: f32, tip: Tip) {
    u.row(8.0, |r| {
        r.icon(icon, 15.0, col);
        r.text(name.to_string(), 12.0, MUTED).style(|n| n.width = Val::Px(110.0));
        r.notches(v.clamp(0.0, 1.0), 10, 160.0, 9.0, level(v));
        r.spacer();
        r.bold(format!("{:.0}", v * 100.0), 12.0, INK);
    })
    .tip(tip);
}

/// A plain fact: icon, name and value.
fn fact(u: &mut Ui, icon: Icon, col: Color, name: &str, value: String) {
    u.row(8.0, |r| {
        r.icon(icon, 15.0, col);
        r.text(name.to_string(), 12.0, MUTED).style(|n| n.width = Val::Px(110.0));
        r.text(value, 12.0, INK);
    });
}

/// A beaver as a link to its own page: its name, and a few words on it.
fn link(u: &mut Ui, s: &sim::SimState, id: BeaverId, note: String) {
    let b = s.colony.beaver(id);
    u.row(8.0, |r| {
        r.token(Icon::Beaver, 16.0, None);
        r.bold(name_of(s, id), 12.0, if b.is_some() { INK } else { MUTED });
        r.text(note, 11.0, MUTED);
        r.spacer();
        r.icon(Icon::Journal, 12.0, MUTED);
    })
    .action(HudAction::BeaverDetails(Some(id)))
    .tip(Tip::name(name_of(s, id)).line("Open their page", ""));
}

/// A few words on a relative: stage and age, or that it is gone.
fn about(s: &sim::SimState, id: BeaverId) -> String {
    match s.colony.beaver(id) {
        Some(b) => format!("{} · {}", stage_word(b.stage), years(b.age_days as f64)),
        None => match s.lives.of.get(&id).and_then(|l| l.entries.last()) {
            Some(e) => match e.what {
                What::Died(c) => format!("died {} ({})", when(e.day), cause(c)),
                What::Left => format!("left {}", when(e.day)),
                _ => "gone".into(),
            },
            None => "gone".into(),
        },
    }
}

fn condition(c: &Ctx, u: &mut Ui, b: &Beaver) {
    let s = c.s;
    heading(u, Icon::Health, "Condition");
    meter(u, Icon::Health, level(b.health), "Health", b.health, Tip::name("Health").line("Falls with the harm below", ""));
    let (mi, mc) = mood_icon(b.mood);
    meter(u, mi, mc, "Mood", b.mood, Tip::name("Mood").line("Work pace", format!("×{}", num(sim::society::work_speed(b.mood)))));
    let n = &b.needs;
    for (icon, col, name, v, what) in [
        (Icon::Food, good_color(sim::buildings::Good::Food), "Food", n.food, "Eats from the stores or the land"),
        (Icon::Idle, REST, "Rest", n.rest, "Sleeps at home when tired"),
        (Icon::Winter, MUTED, "Warmth", n.warmth, "A lodge keeps it warm in winter"),
        (Icon::Home, MUD, "Safety", n.safety, "Falls when a wolf is about"),
        (Icon::Beaver, FUR, "Company", n.social, "Family and neighbours"),
    ] {
        meter(u, icon, col, name, v, Tip::name(name).line(what, ""));
    }
    let harm = b.harm();
    let names = ["Hunger", "Cold", "Injury", "Exhaustion", "Old age"];
    if harm.iter().any(|h| *h > 0.005) {
        u.row(6.0, |r| {
            r.text("Harm taken".to_string(), 12.0, MUTED).style(|n| n.width = Val::Px(110.0));
            for (k, h) in harm.iter().enumerate().filter(|(_, h)| **h > 0.005) {
                r.text(format!("{} {:.0}", names[k], h * 100.0), 11.0, if *h > 0.3 { BAD } else { WARN });
            }
        });
    }
    if b.low_mood_days() > 0.5 {
        fact(u, Icon::MoodBad, BAD, "Unhappy for", days(b.low_mood_days()));
    }
    u.rule();
    heading(u, Icon::Hammer, "Work");
    let (ai, ac, doing) = super::inspect::activity(s, b);
    fact(u, ai, ac, "Now", doing.to_string());
    let job = sim::lives::job_of(s, b.id);
    fact(u, job.map_or(Icon::Idle, role_kind_icon), INK, "Job", job.map_or("Free".to_string(), |r| role_kind_name(r).to_string()));
    if let Some(bid) = s.work.slot_of(b.id).map(|x| x.0) {
        if let Some(bb) = s.buildings.building(bid) {
            u.row(8.0, |r| {
                r.icon(building_icon(bb.kind), 15.0, INK);
                r.text("Works at".to_string(), 12.0, MUTED).style(|n| n.width = Val::Px(110.0));
                r.text(bb.kind.spec().name.to_string(), 12.0, INK);
                r.spacer();
                r.pebble(Icon::Focus, 11.0, HudAction::SelectBuilding(bid), Peb::Idle).tip(Tip::name("Show"));
            });
        }
    }
    if let Some(l) = s.agents.get(b.id).and_then(|a| a.load.as_ref()) {
        fact(u, good_icon(l.good), good_color(l.good), "Carrying", format!("{} {}", num(l.amount), good_name(l.good)));
    }
    if let Some(p) = &b.pregnancy {
        fact(u, Icon::Kit, GOOD, "Expecting", format!("{} kits in {}", p.litter, days((p.due_day - s.time_days) as f32)));
    }
}

/// A beaver's parents, whether it is still here or only remembered.
fn parents(s: &sim::SimState, id: BeaverId) -> [Option<BeaverId>; 2] {
    match s.colony.beaver(id) {
        Some(b) => [b.mother, b.father],
        None => s.lives.of.get(&id).map_or([None, None], |l| [l.mother, l.father]),
    }
}

fn sex_of(s: &sim::SimState, id: BeaverId) -> Option<sim::colony::Sex> {
    s.colony.beaver(id).map(|b| b.sex).or_else(|| s.lives.of.get(&id).map(|l| l.sex))
}

fn partner_of(s: &sim::SimState, id: BeaverId) -> Option<BeaverId> {
    s.colony.beaver(id).and_then(|b| b.mate)
}

/// How `other` is related to `me`, closest tie first, in words that follow its sex; with an
/// order to list relatives in (nearest first).
fn kinship(s: &sim::SimState, me: BeaverId, other: BeaverId) -> Option<(u8, &'static str)> {
    use sim::colony::Sex;
    if me == other {
        return None;
    }
    let male = sex_of(s, other) == Some(Sex::Male);
    let pick = |m: &'static str, f: &'static str| if male { m } else { f };
    let mine = parents(s, me);
    let theirs = parents(s, other);
    let parent_of = |p: BeaverId, c: BeaverId| parents(s, c).contains(&Some(p));
    let shared = mine.iter().flatten().filter(|p| theirs.contains(&Some(**p))).count();
    let siblings = |a: BeaverId, b: BeaverId| a != b && parents(s, a).iter().flatten().any(|p| parents(s, b).contains(&Some(*p)));
    if partner_of(s, me) == Some(other) {
        return Some((0, "Partner"));
    }
    if parent_of(other, me) {
        return Some((1, pick("Father", "Mother")));
    }
    if parent_of(me, other) {
        return Some((2, pick("Son", "Daughter")));
    }
    if shared == 2 {
        return Some((3, pick("Brother", "Sister")));
    }
    if shared == 1 {
        return Some((3, pick("Half-brother", "Half-sister")));
    }
    if mine.iter().flatten().any(|p| parent_of(other, *p)) {
        return Some((4, pick("Grandfather", "Grandmother")));
    }
    if theirs.iter().flatten().any(|p| parent_of(me, *p)) {
        return Some((5, pick("Grandson", "Granddaughter")));
    }
    if mine.iter().flatten().any(|p| siblings(*p, other)) {
        return Some((6, pick("Uncle", "Aunt")));
    }
    if theirs.iter().flatten().any(|p| siblings(*p, me)) {
        return Some((7, pick("Nephew", "Niece")));
    }
    if mine.iter().flatten().any(|p| theirs.iter().flatten().any(|q| siblings(*p, *q))) {
        return Some((8, "Cousin"));
    }
    if let Some(m) = partner_of(s, me) {
        if parent_of(other, m) {
            return Some((9, pick("Father-in-law", "Mother-in-law")));
        }
        if siblings(m, other) {
            return Some((9, pick("Brother-in-law", "Sister-in-law")));
        }
    }
    if partner_of(s, other).is_some_and(|p| parent_of(me, p)) {
        return Some((10, pick("Son-in-law", "Daughter-in-law")));
    }
    None
}

fn family(c: &Ctx, u: &mut Ui, id: BeaverId, b: Option<&Beaver>, life: Option<&Life>) {
    let s = c.s;
    heading(u, Icon::Home, "Family");
    // Every beaver the colony has known, living or remembered, that is kin.
    let mut kin: Vec<(u8, &'static str, BeaverId)> = s
        .colony
        .beavers
        .iter()
        .map(|o| o.id)
        .chain(s.lives.of.keys().copied())
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .filter_map(|o| kinship(s, id, o).map(|(order, word)| (order, word, o)))
        .collect();
    kin.sort_by_key(|k| (k.0, s.colony.beaver(k.2).is_none(), k.2));
    let my_lodge = b.and_then(|b| b.lodge);
    for &(_, word, o) in &kin {
        let since = if word == "Partner" { life.and_then(|l| l.partnered_since()).map_or(String::new(), |d| format!(" · since {}", when(d))) } else { String::new() };
        let home = if my_lodge.is_some() && s.colony.beaver(o).and_then(|x| x.lodge) == my_lodge { " · lives here too" } else { "" };
        link(u, s, o, format!("{word} · {}{since}{home}", about(s, o)));
    }
    if kin.is_empty() {
        u.text("No family in the colony".to_string(), 12.0, MUTED);
    }
    u.rule();
    heading(u, Icon::Lodge, "Home");
    match b.and_then(|b| b.lodge).and_then(|l| s.colony.lodges.iter().find(|x| x.id == l)) {
        Some(l) => {
            let fam = s.colony.family(l.family).map(|f| f.name.clone()).unwrap_or_default();
            fact(u, Icon::Lodge, MUD, "Lodge", fam);
            if let Some(d) = life.and_then(|l| l.home_since()) {
                fact(u, Icon::Hourglass, MUTED, "Since", when(d));
            }
        }
        None => {
            u.text("No home".to_string(), 12.0, MUTED);
        }
    }
}

fn skills(u: &mut Ui, b: &Beaver) {
    heading(u, Icon::Jobs, "Skills");
    for j in Job::ALL {
        let v = b.skills[j.index()];
        u.row(8.0, |r| {
            r.icon(job_icon(j), 15.0, INK);
            r.text(job_name(j).to_string(), 12.0, MUTED).style(|n| n.width = Val::Px(170.0));
            r.notches(v.clamp(0.0, 1.0), 10, 120.0, 9.0, GOOD);
            r.spacer();
            r.bold(format!("{:.0}", v * 100.0), 12.0, INK);
        })
        .tip(Tip::name(job_name(j)).with_value(format!("{:.0}", v * 100.0)).line("Grows with work at it; faster work", ""));
    }
}

fn chronology(c: &Ctx, u: &mut Ui, life: Option<&Life>) {
    let s = c.s;
    heading(u, Icon::Journal, "Life in the colony");
    let Some(life) = life else {
        u.text("No record yet: it starts with the next look at the colony".to_string(), 12.0, MUTED);
        return;
    };
    if let Some(d) = life.arrived() {
        fact(u, Icon::Hourglass, MUTED, "In the colony", format!("since {} ({})", when(d), years(s.time_days - d)));
    }
    let total = life.entries.len();
    let skip = c.hud.scroll.min(total.saturating_sub(1));
    for e in life.entries.iter().rev().skip(skip).take(ROWS) {
        let (icon, col, words) = match e.what {
            What::Known => (Icon::Beaver, MUTED, "In the colony when its records began".to_string()),
            What::Born { mother, father } => {
                let p: Vec<String> = [mother, father].into_iter().flatten().map(|p| name_of(s, p)).collect();
                (Icon::Kit, GOOD, if p.is_empty() { "Born".into() } else { format!("Born to {}", p.join(" and ")) })
            }
            What::Joined => (Icon::Beaver, GOOD, "Joined the colony as a newcomer".into()),
            What::Grew(st) => (stage_icon(st), INK, format!("Grew up: {}", stage_word(st).to_lowercase())),
            What::Job(Some(r)) => (role_kind_icon(r), INK, format!("Became a {}", role_kind_name(r).to_lowercase())),
            What::Job(None) => (Icon::Idle, ACCENT, "Left free of work".into()),
            What::Partner(Some(p)) => (Icon::MoodGood, GOOD, format!("Paired with {}", name_of(s, p))),
            What::Partner(None) => (Icon::MoodBad, MUTED, "Lost its partner".into()),
            What::Moved(Some(l)) => {
                let fam = s.colony.lodges.iter().find(|x| x.id == l).and_then(|x| s.colony.family(x.family)).map(|f| f.name.clone()).unwrap_or_else(|| "a lodge".into());
                (Icon::Lodge, MUD, format!("Moved into the {fam} lodge"))
            }
            What::Moved(None) => (Icon::Lodge, MUTED, "Left its lodge".into()),
            What::Died(cause_of) => (Icon::Cross, BAD, format!("Died of {}", cause(cause_of))),
            What::Left => (Icon::Walk, MUTED, "Left the colony".into()),
        };
        u.row(8.0, |r| {
            r.text(when(e.day), 11.0, MUTED).style(|n| n.width = Val::Px(72.0));
            r.icon(icon, 14.0, col);
            r.text(words, 12.0, INK);
        });
    }
    if total > ROWS {
        u.text(format!("{}–{} of {} · scroll for more", skip + 1, (skip + ROWS).min(total), total), 11.0, MUTED);
    }
}

pub fn screen(c: &Ctx, id: BeaverId) -> Vec<VNode> {
    let s = c.s;
    let b = s.colony.beaver(id);
    let life = s.lives.of.get(&id);
    let mut ui = Ui::default();
    ui.node(
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(76.0),
            right: Val::Px(20.0),
            top: Val::Px(70.0),
            bottom: Val::Px(96.0),
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(10.0),
            overflow: Overflow::clip(),
            ..default()
        },
        |u| {
            // Who: name, sex, age, stage, and how long it may live.
            u.row(14.0, |r| {
                r.portrait(Icon::Beaver, if b.is_some() { FUR } else { MUTED });
                r.col(4.0, |x| {
                    x.bold(name_of(s, id), 20.0, INK);
                    match b {
                        Some(b) => {
                            let sex = if b.sex == sim::colony::Sex::Male { "Male" } else { "Female" };
                            x.row(8.0, |y| {
                                y.icon(stage_icon(b.stage), 15.0, INK);
                                y.text(format!("{sex} · {} · {} old · lives to about {}", stage_word(b.stage), years(b.age_days as f64), years(b.lifespan_days as f64)), 13.0, MUTED);
                            });
                        }
                        None => {
                            x.text(about(s, id), 13.0, MUTED);
                        }
                    }
                });
                r.spacer();
                if b.is_some() {
                    r.pebble(Icon::Focus, 13.0, HudAction::SelectBeaver(id), Peb::Idle).tip(Tip::name("Show on the map"));
                }
                r.pebble(Icon::Cross, 11.0, HudAction::BeaverDetails(None), Peb::Bare).tip(Tip::name("Close").key("Esc"));
            });
            u.rule();
            u.row(28.0, |cols| {
                let col = |cols: &mut Ui, f: &dyn Fn(&mut Ui)| {
                    cols.node(Node { width: Val::Px(COL_W), flex_direction: FlexDirection::Column, row_gap: Val::Px(7.0), flex_shrink: 0.0, ..default() }, |u| f(u));
                };
                col(cols, &|u| match b {
                    Some(b) => condition(c, u, b),
                    None => {
                        u.text("No longer in the colony".to_string(), 13.0, MUTED);
                    }
                });
                col(cols, &|u| family(c, u, id, b, life));
                col(cols, &|u| {
                    if let Some(b) = b {
                        skills(u, b);
                        u.rule();
                    }
                    chronology(c, u, life);
                });
            })
            .style(|n| n.align_items = AlignItems::FlexStart);
        },
    )
    .tag(Tag::Scroll);
    let mut v = ui.nodes.pop().unwrap();
    panel(&mut v);
    v.node.padding = UiRect::all(Val::Px(18.0));
    vec![v]
}
