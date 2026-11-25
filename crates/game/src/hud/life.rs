//! A beaver's whole story, full screen, opened from its card: who it is and how it is
//! (health, mood, every need, the harm it has taken), what it does, its family (each a link to
//! their own page), its home and who shares it, its skill at every job, and its life in the
//! colony day by day, newest first (see `sim::lives`).
use super::panels::{stage_icon, Ctx};
use super::ui::*;
use super::words::*;
use super::world::mood_icon;
use super::{HudAction, Page};
use super::kin::{self, name_of, KinFilter};
use crate::icons::Icon;
use bevy::prelude::*;
use sim::colony::{Beaver, BeaverId, DeathCause, Job, Stage};
use sim::lives::{Life, What};

/// Chronology rows shown at once (the wheel scrolls through the rest).
pub(super) const ROWS: usize = 22;
/// The live portrait's shape, width over height.
const PORTRAIT_ASPECT: f32 = crate::entities_render::portrait::WIDTH as f32 / crate::entities_render::portrait::HEIGHT as f32;
/// The widest a column of the page grows.
const COL_MAX: f32 = 430.0;

/// A page's name, for the Back button that returns to it.
pub(super) fn page_name(s: &sim::SimState, p: Page) -> String {
    match p {
        Page::Beaver(id) => name_of(s, id),
        Page::Lodge(l) => s.colony.lodges.iter().find(|x| x.id == l).and_then(|x| s.colony.family(x.family)).map_or_else(|| sim::buildings::BuildingKind::home().name().to_string(), |f| format!("{} family", f.name)),
    }
}

/// The top of a full page: Back to the page it came from, if any, and Close.
pub(super) fn page_controls(c: &Ctx, r: &mut Ui, close: HudAction) {
    if let Some(&p) = c.hud.back.last() {
        r.back_button(&page_name(c.s, p));
    }
    r.close_button(close, c.hud.back.is_empty());
}

fn when(day: f64) -> String {
    date(day)
}

fn years(days: f64) -> String {
    span(days)
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

/// A labelled bar: icon, name, ticks and the number.
fn meter(u: &mut Ui, icon: Icon, col: Color, name: &str, v: f32, tip: Tip) {
    u.row(8.0, |r| {
        r.icon(icon, 15.0, col);
        r.text(name.to_string(), 12.0, MUTED).style(|n| n.width = Val::Px(110.0));
        r.ticks(v.clamp(0.0, 1.0), 10, 160.0, 9.0, level(v));
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
    let food = Tip::name(sim::buildings::Good::food().name()).line("Hunger", per_day(-r.hunger)).line("Eats below", pts(r.eat_below));
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
        (crate::hud::words::good_icon(sim::buildings::Good::food()), good_color(sim::buildings::Good::food()), sim::buildings::Good::food().name(), n.food, food),
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
}

/// What it does: now, its job, its pace, where, what it carries.
fn work(c: &Ctx, u: &mut Ui, b: &Beaver) {
    let s = c.s;
    heading(u, Icon::Hammer, "Work");
    let (ai, ac, doing) = super::inspect::activity(s, b);
    fact(u, ai, ac, "Now", doing.to_string());
    let job = sim::lives::job_of(s, b.id);
    fact(u, job.map_or(Icon::Idle, job_icon), INK, "Job", job.map_or("Free".to_string(), |r| r.name().to_string()));
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
                r.text(format!("{} {}", num(l.amount), l.good.name()), 12.0, INK);
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
    // The family tree lives on its lodge's family page: one clear way there.
    u.row(8.0, |r| {
        r.icon(Icon::Home, 16.0, ACCENT);
        r.bold("Family".to_string(), 14.0, INK);
        r.spacer();
        match b.and_then(|b| b.lodge) {
            Some(l) => r.small_button(Some(crate::hud::words::building_icon(sim::buildings::BuildingKind::home())), Some("Family tree"), Some(HudAction::LodgePage(Some(l))), Tip::name("Family page").line("Who lives in its lodge, and the family tree", ""), Small::Idle),
            None => r.small_button(Some(crate::hud::words::building_icon(sim::buildings::BuildingKind::home())), Some("Family tree"), None, Tip::name("Family page").line("No home: a family page belongs to a lodge", ""), Small::Idle),
        };
    });
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
    let my_lodge = b.and_then(|b| b.lodge);
    let at_home = |o: BeaverId| my_lodge.is_some() && s.colony.beaver(o).and_then(|x| x.lodge) == my_lodge;
    // Kin, and anyone else sharing its lodge.
    let mut ties: Vec<(u8, &'static str, BeaverId)> = all
        .iter()
        .filter_map(|&o| kin::kinship(s, id, o).map(|(order, word)| (order, word, o)).or_else(|| (o != id && at_home(o)).then_some((kin::LODGER, "Lodge-mate", o))))
        .collect();
    ties.sort_by(|a, b| a.0.cmp(&b.0).then(kin::born(s, a.2).unwrap_or(f64::MAX).total_cmp(&kin::born(s, b.2).unwrap_or(f64::MAX))).then(a.2.cmp(&b.2)));
    // Filters, each with how many it holds; one that holds nobody here falls back to All.
    let count = |f: KinFilter| ties.iter().filter(|t| f.takes(t.0, at_home(t.2))).count();
    let filter = if count(c.hud.kin) == 0 { KinFilter::All } else { c.hud.kin };
    u.row(4.0, |r| {
        for f in KinFilter::ALL {
            let n = count(f);
            if n == 0 && f != KinFilter::All {
                continue;
            }
            super::ledger::chip(r, &format!("{} {n}", f.label()), f == filter, HudAction::Kin(f));
        }
    })
    .style(|n| {
        n.flex_wrap = FlexWrap::Wrap;
        n.row_gap = Val::Px(4.0);
    });
    let shown: Vec<_> = ties.iter().filter(|t| filter.takes(t.0, at_home(t.2))).collect();
    if shown.is_empty() {
        u.text("No family in the colony".to_string(), 12.0, MUTED);
    } else {
        u.row(6.0, |r| {
            for (w, h) in [(92.0, "Tie"), (104.0, "Beaver"), (84.0, "Litter")] {
                r.text(h.to_string(), 11.0, MUTED).style(move |n| n.width = Val::Px(w));
            }
        });
    }
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
        let home = if at_home(o) { " · home" } else { "" };
        kin_row(u, s, o, word, litter, litter_tip(s, &all, theirs), format!("{}{since}{home}", about(s, o)));
    }
    u.rule();
    heading(u, crate::hud::words::building_icon(sim::buildings::BuildingKind::home()), "Home");
    match b.and_then(|b| b.lodge).and_then(|l| s.colony.lodges.iter().find(|x| x.id == l)) {
        Some(l) => {
            let fam = s.colony.family(l.family).map(|f| f.name.clone()).unwrap_or_default();
            fact(u, crate::hud::words::building_icon(sim::buildings::BuildingKind::home()), MUD, sim::buildings::BuildingKind::home().name(), fam);
            if let Some(d) = life.and_then(|l| l.home_since()) {
                fact(u, Icon::Hourglass, MUTED, "Since", when(d));
            }
        }
        None => {
            u.text("No home".to_string(), 12.0, MUTED);
        }
    }
}

fn skills(c: &Ctx, u: &mut Ui, b: &Beaver) {
    heading(u, Icon::Jobs, "Skills");
    let rec = c.s.deeds.get(b.id);
    for j in Job::all() {
        let v = b.skills[j];
        let mut tip = Tip::name(j.row().skill.as_str()).with_value(format!("{:.0}", v * 100.0));
        if j.row().skill_speeds {
            tip = tip.line("Work", format!("×{:.2}", sim::agents::work_factors(b, j).skill_x));
        }
        if let Some(d) = rec.map(|r| r.days[j] as f64).filter(|d| *d >= 1.0) {
            tip = tip.line("Time at it", years(d));
        }
        u.row(8.0, |r| {
            r.icon(job_icon(j), 15.0, INK);
            r.text(j.row().skill.as_str().to_string(), 12.0, MUTED).style(|n| n.width = Val::Px(110.0));
            r.ticks(v.clamp(0.0, 1.0), 10, 120.0, 9.0, GOOD);
            r.spacer();
            r.bold(format!("{:.0}", v * 100.0), 12.0, INK);
        })
        .tip(tip);
    }
}

/// Its life's work: a chip for each thing it has done, the detail in the tip.
fn records(c: &Ctx, u: &mut Ui, id: BeaverId) {
    use sim::buildings::Good;
    let s = c.s;
    let Some(r) = s.deeds.get(id) else { return };
    let km = |cells: f32| format!("{:.1} km", cells * s.water.cell_m / 1000.0);
    let n = |v: f32| format!("{}", v.round() as i64);
    let mut chips: Vec<(Icon, Color, String, Tip)> = Vec::new();
    if r.walked >= 1.0 {
        chips.push((Icon::Walk, INK, km(r.walked), Tip::name("Walked")));
    }
    if r.swum >= 1.0 {
        chips.push((Icon::Drop, WATER, km(r.swum), Tip::name("Swum")));
    }
    let felled: u32 = r.felled.values().sum();
    if felled > 0 {
        let mut t = Tip::name("Trees felled");
        for (sp, k) in &r.felled {
            t = t.line(sp.name(), format!("{k}"));
        }
        if r.great_trees > 0 {
            t = t.line("Great old trees", format!("{}", r.great_trees));
        }
        chips.push((crate::hud::words::resource_icon(sim::resources::ResourceKind::tree()), GOOD, format!("{felled}"), t));
    }
    let food = r.gathered[Good::food()];
    if food >= 1.0 {
        let mut t = Tip::name("Food gathered");
        for (p, f) in &r.harvested {
            t = t.line(p.name(), n(*f));
        }
        if r.bark >= 1.0 {
            t = t.line("Bark", n(r.bark));
        }
        if r.winter_food >= 1.0 {
            t = t.line("In winter", n(r.winter_food));
        }
        chips.push((good_icon(Good::food()), good_color(Good::food()), n(food), t));
    }
    for g in Good::all().filter(|g| !g.row().edible) {
        let v = r.gathered[g];
        if v >= 1.0 {
            chips.push((good_icon(g), good_color(g), n(v), Tip::name(format!("{} gathered", g.name()))));
        }
    }
    // Carried on foot, by what and where to.
    let carried: f32 = r.carried.values().sum();
    if carried >= 1.0 {
        let mut t = Tip::name("Carried");
        let mut rows: Vec<(&(Good, sim::buildings::BuildingKind), &f32)> = r.carried.iter().collect();
        rows.sort_by(|a, b| b.1.total_cmp(a.1));
        for ((g, k), v) in rows.into_iter().take(8) {
            t = t.line(format!("{} → {}", g.name(), k.spec().name.as_str()), n(*v));
        }
        chips.push((Icon::Haul, INK, n(carried), t));
    }
    let rafted = r.rafted_total();
    if rafted >= 1.0 {
        let mut t = Tip::name("Moved by raft");
        for g in Good::all().filter(|&g| r.rafted[g] >= 1.0) {
            t = t.line(g.name(), n(r.rafted[g]));
        }
        chips.push((Icon::Raft, WATER, n(rafted), t));
    }
    // A structure by its name: a dam by its pond's, a lodge by its family's.
    let structure = |b: sim::buildings::BuildingId, kind: Option<sim::buildings::BuildingKind>| -> String {
        if let Some(p) = sim::names::pond_name(s, b) {
            return p.to_string();
        }
        let family = s.work.homes.get(&b).and_then(|l| s.colony.lodges.iter().find(|x| x.id == *l)).and_then(|l| s.colony.family(l.family));
        match (family, kind) {
            (Some(f), _) => f.name.clone(),
            (None, Some(k)) => k.spec().name.to_string(),
            (None, None) => "Gone".to_string(),
        }
    };
    if r.dam_loads > 0 {
        let mut t = Tip::name("Dam loads packed");
        let mut dams: Vec<_> = r.works.iter().filter(|(_, w)| w.kind == Some(sim::buildings::BuildingKind::dam()) && w.loads > 0).collect();
        dams.sort_by(|a, b| b.1.loads.cmp(&a.1.loads));
        for (b, w) in dams.into_iter().take(6) {
            t = t.line(structure(*b, w.kind), format!("{}", w.loads));
        }
        chips.push((crate::hud::words::building_icon(sim::buildings::BuildingKind::dam()), MUD, format!("{}", r.dam_loads), t));
    }
    if r.canal_cells > 0 {
        chips.push((crate::hud::words::building_icon(sim::buildings::BuildingKind::canal()), WATER, format!("{}", r.canal_cells), Tip::name("Canal cells dug")));
    }
    if r.gates > 0 {
        chips.push((Icon::Lock, INK, format!("{}", r.gates), Tip::name("Gates set")));
    }
    let built: u32 = r.built.iter().filter(|(k, _)| !k.is_waterworks()).map(|(_, v)| v).sum();
    if built > 0 {
        let mut t = Tip::name("Buildings raised");
        for (k, v) in r.built.iter().filter(|(k, _)| !k.is_waterworks()) {
            t = t.line(k.spec().name.as_str(), format!("{v}"));
        }
        chips.push((Icon::Building, MUD, format!("{built}"), t));
    }
    if r.mended >= 1.0 {
        let mut t = Tip::name("Mending done");
        if r.leaks > 0 {
            t = t.line("Leaks stopped", format!("{}", r.leaks));
        }
        let mut fixed: Vec<_> = r.works.iter().filter(|(_, w)| w.repairs >= 0.5).collect();
        fixed.sort_by(|a, b| b.1.repairs.total_cmp(&a.1.repairs));
        for (b, w) in fixed.into_iter().take(6) {
            t = t.line(structure(*b, w.kind), n(w.repairs));
        }
        chips.push((Icon::Hammer, INK, n(r.mended), t));
    }
    if r.planted > 0 {
        let mut t = Tip::name("Saplings planted");
        if r.grown > 0 {
            t = t.line("Grown into trees", format!("{}", r.grown));
        }
        chips.push((crate::hud::words::good_icon(sim::buildings::Good::seedling()), GOOD, format!("{}", r.planted), t));
    }
    if r.seedlings > 0 {
        chips.push((crate::hud::words::resource_icon(sim::resources::ResourceKind::food_plant(false)), GOOD, format!("{}", r.seedlings), Tip::name("Seedlings collected")));
    }
    if r.wolves > 0 {
        let mut t = Tip::name("Wolves spotted");
        if r.warned > 0 {
            t = t.line("Beavers warned", format!("{}", r.warned));
        }
        chips.push((Icon::Wolf, BAD, format!("{}", r.wolves), t));
    }
    let (born, raised) = sim::deeds::kits(s, id);
    if born > 0 {
        chips.push((Icon::Kit, FUR, format!("{born}"), Tip::name("Kits").line("Grew up", format!("{raised}"))));
    }
    if chips.is_empty() {
        return;
    }
    u.rule();
    heading(u, Icon::Hourglass, "Life's work");
    u.node(Node { flex_direction: FlexDirection::Row, flex_wrap: FlexWrap::Wrap, column_gap: Val::Px(14.0), row_gap: Val::Px(8.0), ..default() }, |g| {
        for (icon, col, value, tip) in chips {
            g.row(4.0, |x| {
                x.icon(icon, 15.0, col);
                x.bold(value, 12.0, INK);
            })
            .style(|n| n.min_width = Val::Px(64.0))
            .tip(tip);
        }
    });
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
    let her = if life.sex == sim::colony::Sex::Male { "his" } else { "her" };
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
            What::Job(Some(r)) => (job_icon(r), INK, format!("Became a {}", r.name().to_lowercase())),
            What::Job(None) => (Icon::Idle, ACCENT, "Left free of work".into()),
            What::Partner(Some(p)) => (Icon::MoodGood, GOOD, format!("Paired with {}", name_of(s, p))),
            What::Partner(None) => (Icon::MoodBad, MUTED, format!("Lost {her} partner")),
            What::Moved(Some(l)) => {
                let fam = s.colony.lodges.iter().find(|x| x.id == l).and_then(|x| s.colony.family(x.family)).map(|f| f.name.clone()).unwrap_or_else(|| "a lodge".into());
                (crate::hud::words::building_icon(sim::buildings::BuildingKind::home()), MUD, format!("Moved into the {fam} lodge"))
            }
            What::Moved(None) => (crate::hud::words::building_icon(sim::buildings::BuildingKind::home()), MUTED, format!("Left {her} lodge")),
            What::Died(cause_of) => (Icon::Cross, BAD, format!("Died of {}", cause(cause_of))),
            What::Left => (Icon::Walk, MUTED, "Left the colony".into()),
        };
        u.row(8.0, |r| {
            r.text(when(e.day), 11.0, MUTED).style(|n| n.width = Val::Px(92.0));
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
        Node { flex_direction: FlexDirection::Column, row_gap: Val::Px(10.0), overflow: Overflow::clip(), ..default() },
        |u| {
            // Who: name, sex, age, stage, and how long it may live.
            u.row(14.0, |r| {
                r.col(4.0, |x| {
                    x.bold(full_name(s, id, &name_of(s, id)), 20.0, INK);
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
                    r.small_button(Some(Icon::Focus), None, Some(HudAction::SelectBeaver(id)), Tip::name("Show on the map"), Small::Idle);
                }
                page_controls(c, r, HudAction::BeaverDetails(None));
            })
            .style(|n| n.align_items = AlignItems::FlexStart);
            u.rule();
            u.row(28.0, |cols| {
                let col = |cols: &mut Ui, f: &dyn Fn(&mut Ui)| {
                    cols.node(Node { flex_grow: 1.0, flex_basis: Val::Px(0.0), min_width: Val::Px(0.0), max_width: Val::Px(COL_MAX), flex_direction: FlexDirection::Column, row_gap: Val::Px(7.0), overflow: Overflow::clip(), ..default() }, |u| f(u));
                };
                col(cols, &|u| {
                    // What it is doing right now, live.
                    if let Some(p) = c.portrait {
                        u.image(p.0.clone(), Node { width: Val::Percent(100.0), aspect_ratio: Some(PORTRAIT_ASPECT), flex_shrink: 0.0, ..default() }).skin(Skin::Portrait);
                    }
                    match b {
                        Some(b) => condition(c, u, b),
                        None => {
                            u.text("No longer in the colony".to_string(), 13.0, MUTED);
                        }
                    }
                    records(c, u, id);
                });
                col(cols, &|u| {
                    if let Some(b) = b {
                        work(c, u, b);
                        u.rule();
                    }
                    family(c, u, id, b, life);
                });
                col(cols, &|u| {
                    if let Some(b) = b {
                        skills(c, u, b);
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
    vec![page_area(v, true)]
}
