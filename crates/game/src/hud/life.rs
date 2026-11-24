//! A beaver's whole story, full screen, opened from its card: who it is and how it is
//! (health, mood, every need, the harm it has taken), what it does, its family (each a link to
//! their own page), its home and who shares it, its skill at every job, and its life in the
//! colony day by day, newest first (see `sim::lives`).
use super::panels::{stage_icon, Ctx};
use super::ui::*;
use super::words::*;
use super::world::mood_icon;
use super::HudAction;
use super::kin::{self, name_of, KinFilter};
use crate::icons::Icon;
use bevy::prelude::*;
use sim::colony::{Beaver, BeaverId, DeathCause, Job, Stage};
use sim::lives::{Life, What};

/// Chronology rows shown at once (the wheel scrolls through the rest).
const ROWS: usize = 22;
/// The portrait's side on the details page.
const PORTRAIT_PX: f32 = 128.0;
const COL_W: f32 = 390.0;


/// "Year 2, day 140": the year of the colony and the day in it.
pub(super) fn when(day: f64) -> String {
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

pub(super) fn stage_word(st: Stage) -> &'static str {
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

/// A few words on a relative: stage and age, or that it is gone.
pub(super) fn about(s: &sim::SimState, id: BeaverId) -> String {
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

/// A level on the 0–100 scale the meters show.
fn pts(v: f32) -> String {
    format!("{:.0}", v * 100.0)
}

/// A change a day on that scale.
fn per_day(v: f32) -> String {
    format!("{:+.1}/d", v * 100.0)
}

/// A multiplier, only when it is not ×1.
fn factor(t: Tip, label: &str, f: f32) -> Tip {
    if (f - 1.0).abs() > 0.005 { t.line(label, format!("×{f:.2}")) } else { t }
}

const HARMS: [&str; 5] = ["Hunger", "Cold", "Injury", "Exhaustion", "Old age"];

fn condition(c: &Ctx, u: &mut Ui, b: &Beaver) {
    let s = c.s;
    let Some(r) = s.beaver_rates(b.id) else { return };
    heading(u, Icon::Health, "Condition");
    let mut health = Tip::name("Health");
    for (k, h) in r.harm.iter().enumerate().filter(|(_, h)| **h > 0.0) {
        health = health.line(HARMS[k], per_day(-h));
    }
    if r.heal > 0.0 {
        health = health.line("Mending", per_day(r.heal));
    }
    if r.old_age_death > 0.0 {
        health = health.line("Death from age", format!("{:.0}%/y", (1.0 - (-r.old_age_death).exp()) * 100.0));
    }
    meter(u, Icon::Health, level(b.health), "Health", b.health, health);
    let (mi, mc) = mood_icon(b.mood);
    let mood = factor(Tip::name("Mood").line("Toward", pts(r.mood_to)), "Health", r.mood_health);
    meter(u, mi, mc, "Mood", b.mood, factor(mood, "Work", sim::society::work_speed(b.mood)));
    let n = &b.needs;
    let food = Tip::name("Food").line("Hunger", per_day(-r.hunger)).line("Eats below", pts(r.eat_below));
    let mut rest = factor(Tip::name("Rest").line("Now", per_day(r.rest)), "Sleep", r.sleep);
    if r.resting {
        rest = rest.line("Resting", "");
    }
    let mut warmth = Tip::name("Warmth").line("Toward", pts(r.warmth_to));
    if r.cold > 0.005 {
        warmth = warmth.line("Cold", pts(r.cold)).line("Lodge keeps out", format!("{:.0}%", r.insulation * 100.0));
    }
    let mut safety = Tip::name("Safety").line("Toward", pts(r.safety_to));
    if let Some((land, water)) = r.exposure {
        if land > 0.0 {
            safety = safety.line("On land", "");
        }
        if water > 0.0 {
            safety = safety.line("To water", format!("{} m", num(water)));
        }
    }
    let company = Tip::name("Company").line("Toward", pts(r.social_to)).line("Lodge-mates", format!("{}", r.mates));
    for (icon, col, name, v, tip) in [
        (Icon::Food, good_color(sim::buildings::Good::Food), "Food", n.food, food),
        (Icon::Idle, REST, "Rest", n.rest, rest),
        (Icon::Winter, MUTED, "Warmth", n.warmth, warmth),
        (Icon::Home, MUD, "Safety", n.safety, safety),
        (Icon::Beaver, FUR, "Company", n.social, company),
    ] {
        meter(u, icon, col, name, v, tip);
    }
    let harm = b.harm();
    let names = HARMS;
    if harm.iter().any(|h| *h > 0.005) {
        u.row(6.0, |r| {
            r.text("Harm taken".to_string(), 12.0, MUTED).style(|n| n.width = Val::Px(110.0));
            for (k, h) in harm.iter().enumerate().filter(|(_, h)| **h > 0.005) {
                r.text(format!("{} {:.0}", names[k], h * 100.0), 11.0, if *h > 0.3 { BAD } else { WARN });
            }
        });
    }
    if let Some(left) = r.leaves_in.filter(|_| b.low_mood_days() > 0.5) {
        fact(u, Icon::MoodBad, BAD, "Leaves in", days(left));
    }
    u.rule();
    heading(u, Icon::Hammer, "Work");
    let (ai, ac, doing) = super::inspect::activity(s, b);
    fact(u, ai, ac, "Now", doing.to_string());
    let job = sim::lives::job_of(s, b.id);
    fact(u, job.map_or(Icon::Idle, role_kind_icon), INK, "Job", job.map_or("Free".to_string(), |r| role_kind_name(r).to_string()));
    if let Some((pace, tip)) = super::ledger::pace(s, b) {
        u.row(8.0, |r| {
            r.icon(Icon::Fast2, 15.0, INK);
            r.text("Pace".to_string(), 12.0, MUTED).style(|n| n.width = Val::Px(110.0));
            r.text(format!("×{pace:.2}"), 12.0, INK);
        })
        .tip(tip);
    }
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
    if let Some(a) = s.agents.get(b.id) {
        if let Some(l) = a.load.as_ref() {
            u.row(8.0, |r| {
                r.icon(good_icon(l.good), 15.0, good_color(l.good));
                r.text("Carrying".to_string(), 12.0, MUTED).style(|n| n.width = Val::Px(110.0));
                r.text(format!("{} {}", num(l.amount), good_name(l.good)), 12.0, INK);
            })
            .tip(factor(Tip::name("Carrying"), "Walk", sim::agents::travel_pace(&s.nav, a)));
        }
    }
    if let Some(p) = &b.pregnancy {
        fact(u, Icon::Kit, GOOD, "Expecting", format!("{} kits in {}", p.litter, days((p.due_day - s.time_days) as f32)));
    }
}

/// One row of the family table: tie, name (a link to their page), litter, and a few words.
fn kin_row(u: &mut Ui, s: &sim::SimState, other: BeaverId, tie: &str, litter: String, litter_tip: Tip, note: String) {
    let here = s.colony.beaver(other).is_some();
    u.row(6.0, |r| {
        r.text(tie.to_string(), 11.0, MUTED).style(|n| n.width = Val::Px(92.0));
        r.row(5.0, |x| {
            x.token(Icon::Beaver, 13.0, None);
            x.bold(name_of(s, other), 12.0, if here { INK } else { MUTED });
        })
        .style(|n| n.width = Val::Px(104.0))
        .action(HudAction::BeaverDetails(Some(other)))
        .tip(Tip::name(name_of(s, other)).line("Open their page", ""));
        r.text(litter, 11.0, INK).style(|n| n.width = Val::Px(84.0)).tip(litter_tip);
        r.text(note, 11.0, MUTED);
    });
}

/// Which of its mother's litters a beaver came in, and when, for a tip.
fn litter_tip(s: &sim::SimState, all: &[BeaverId], l: Option<kin::Litter>) -> Tip {
    match l {
        Some(l) => Tip::name(format!("Litter {} of {}", l.number, name_of(s, l.mother)))
            .line("Born", kin::year_word(l.year))
            .line("Kits in it", format!("{}", kin::litter_mates(s, all, l).len())),
        None => Tip::name("Litter not known").line("Came in as a newcomer, or before records began", ""),
    }
}

fn family(c: &Ctx, u: &mut Ui, id: BeaverId, b: Option<&Beaver>, life: Option<&Life>) {
    let s = c.s;
    heading(u, Icon::Home, "Family");
    let all = kin::everyone(s);
    let mine = kin::litter(s, &all, id);
    // A female who could have kits: the daily chance now, or what stands in the way.
    match s.colony.conception(id) {
        Some(Ok(p)) => fact(u, Icon::Kit, GOOD, "Conceiving", format!("{:.1}%/d", p * 100.0)),
        Some(Err(why)) => fact(u, Icon::Kit, MUTED, "No kits yet", why.to_string()),
        None => {}
    }
    if let Some(l) = mine {
        fact(u, Icon::Kit, GOOD, "Litter", format!("{} of {}'s · {} · {} kits", l.number, name_of(s, l.mother), kin::year_word(l.year), kin::litter_mates(s, &all, l).len()));
    }
    // Every beaver the colony has known, living or remembered, that is kin.
    let mut ties: Vec<(u8, &'static str, BeaverId)> = all.iter().filter_map(|&o| kin::kinship(s, id, o).map(|(order, word)| (order, word, o))).collect();
    ties.sort_by(|a, b| a.0.cmp(&b.0).then(kin::born(s, a.2).unwrap_or(f64::MAX).total_cmp(&kin::born(s, b.2).unwrap_or(f64::MAX))).then(a.2.cmp(&b.2)));
    // Filters, each with how many it holds.
    let filter = c.hud.kin;
    u.row(4.0, |r| {
        for f in KinFilter::ALL {
            let n = ties.iter().filter(|t| f.takes(t.0)).count();
            if n == 0 && f != KinFilter::All {
                continue;
            }
            super::ledger::chip(r, &format!("{} {n}", f.label()), f == filter, HudAction::Kin(f));
        }
    });
    let shown: Vec<_> = ties.iter().filter(|t| filter.takes(t.0)).collect();
    if shown.is_empty() {
        u.text("No family in the colony".to_string(), 12.0, MUTED);
    } else {
        u.row(6.0, |r| {
            for (w, h) in [(92.0, "Tie"), (104.0, "Beaver"), (84.0, "Litter")] {
                r.text(h.to_string(), 10.0, MUTED).style(move |n| n.width = Val::Px(w));
            }
        });
    }
    let my_lodge = b.and_then(|b| b.lodge);
    let my_year = kin::born_year(s, id);
    for &&(order, word, o) in &shown {
        let theirs = kin::litter(s, &all, o);
        let litter = match (order, theirs) {
            // Brothers and sisters: the same litter, or an older or younger one.
            (3, Some(t)) if mine.is_some_and(|m| m.mother == t.mother && m.year == t.year) => "Littermate".to_string(),
            (3, _) => match (kin::born_year(s, o), my_year) {
                (Some(y), Some(me)) if y < me => format!("Older · {}", kin::year_word(y)),
                (Some(y), Some(me)) if y > me => format!("Younger · {}", kin::year_word(y)),
                (Some(y), _) => format!("Same year · {}", kin::year_word(y)),
                _ => "—".to_string(),
            },
            (_, Some(t)) => format!("#{} · {}", t.number, kin::year_word(t.year)),
            (_, None) => "—".to_string(),
        };
        let since = if order == 0 { life.and_then(|l| l.partnered_since()).map_or(String::new(), |d| format!(" · since {}", when(d))) } else { String::new() };
        let home = if my_lodge.is_some() && s.colony.beaver(o).and_then(|x| x.lodge) == my_lodge { " · home" } else { "" };
        kin_row(u, s, o, word, litter, litter_tip(s, &all, theirs), format!("{}{since}{home}", about(s, o)));
    }
    u.rule();
    heading(u, Icon::Lodge, "Home");
    match b.and_then(|b| b.lodge).and_then(|l| s.colony.lodges.iter().find(|x| x.id == l)) {
        Some(l) => {
            let fam = s.colony.family(l.family).map(|f| f.name.clone()).unwrap_or_default();
            u.row(8.0, |r| {
                r.icon(Icon::Lodge, 15.0, MUD);
                r.text("Lodge".to_string(), 12.0, MUTED).style(|n| n.width = Val::Px(110.0));
                r.text(fam, 12.0, INK);
                r.spacer();
                r.pebble(Icon::Journal, 11.0, HudAction::LodgePage(Some(l.id)), Peb::Idle).tip(Tip::name("Family page").line("Who lives there, and the family tree", ""));
            });
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
    // Hauling is walking, which no skill speeds.
    for j in Job::ALL.into_iter().filter(|j| *j != Job::Hauler) {
        let v = b.skills[j.index()];
        u.row(8.0, |r| {
            r.icon(job_icon(j), 15.0, INK);
            r.text(job_name(j).to_string(), 12.0, MUTED).style(|n| n.width = Val::Px(170.0));
            r.notches(v.clamp(0.0, 1.0), 10, 120.0, 9.0, GOOD);
            r.spacer();
            r.bold(format!("{:.0}", v * 100.0), 12.0, INK);
        })
        .tip(Tip::name(job_name(j)).with_value(format!("{:.0}", v * 100.0)).line("Work", format!("×{:.2}", sim::agents::work_factors(b, j).skill_x)));
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
                match c.portrait {
                    Some(p) => {
                        r.image(p.0.clone(), Node { width: Val::Px(PORTRAIT_PX), height: Val::Px(PORTRAIT_PX), flex_shrink: 0.0, ..default() }).skin(Skin::Portrait);
                    }
                    None => {
                        r.portrait(Icon::Beaver, if b.is_some() { FUR } else { MUTED });
                    }
                }
                r.col(4.0, |x| {
                    x.bold(name_of(s, id), 20.0, INK);
                    match b {
                        Some(b) => {
                            let sex = if b.sex == sim::colony::Sex::Male { "Male" } else { "Female" };
                            x.row(8.0, |y| {
                                y.icon(stage_icon(b.stage), 15.0, INK);
                                y.text(format!("{sex} · {} · {} old", stage_word(b.stage), years(b.age_days as f64)), 13.0, MUTED);
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
