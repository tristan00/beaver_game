//! A beaver's full-screen life page: live portrait, condition meters, job, family, skills, life's work, and chronology.
use super::kin::{self, name_of, KinFilter};
use super::panels::{stage_icon, Ctx};
use super::ui::*;
use super::words::*;
use super::world::mood_icon;
use super::{HudAction, NoticeAt, Overlay, Page};
use crate::icons::Icon;
use bevy::prelude::*;
use sim::colony::{Beaver, BeaverId, DeathCause, Job, Stage};
use sim::lives::{Life, What};

const PORTRAIT_ASPECT: f32 = crate::entities_render::portrait::WIDTH as f32 / crate::entities_render::portrait::HEIGHT as f32;
const COL_MAX: f32 = 430.0;
const HARMS: [&str; 5] = ["Hunger", "Cold", "Injury", "Exhaustion", "Old age"];

pub(super) fn page_name(s: &sim::SimState, p: Page) -> String {
    match p {
        Page::Beaver(id) => name_of(s, id),
        Page::Works(b) => super::costs::structure_name(s, b),
        Page::Lodge(l) => s.colony.lodge_family(l).and_then(|f| s.colony.family(f)).map_or_else(|| sim::buildings::BuildingKind::home().name().to_string(), |f| format!("{} family", f.name)),
    }
}

pub(super) fn page_controls(c: &Ctx, r: &mut Ui, close: HudAction) {
    if let Some(&p) = c.hud.back.last() { r.back_button(&page_name(c.s, p)); }
    r.close_button(close, c.hud.back.is_empty());
}

fn cause(c: DeathCause) -> &'static str {
    match c { DeathCause::Starvation => "starvation", DeathCause::Cold => "cold", DeathCause::Injury => "injury", DeathCause::Exhaustion => "exhaustion", DeathCause::OldAge => "old age", DeathCause::Predator => "killed by a wolf" }
}

pub(super) fn stage_word(st: Stage) -> &'static str { match st { Stage::Kit => "Kit", Stage::Yearling => "Yearling", Stage::Adult => "Adult" } }

fn meter(u: &mut Ui, icon: Icon, col: Color, name: &str, v: f32, tip: Tip) {
    u.row(8.0, |r| {
        r.icon(icon, 15.0, col);
        r.text(name, 12.0, MUTED).style(|n| n.width = Val::Px(110.0));
        r.ticks(v.clamp(0.0, 1.0), 10, 160.0, 9.0, level_color(v));
        r.spacer();
        r.bold(format!("{:.0}", v * 100.0), 12.0, INK);
    }).style(|n| { n.width = Val::Percent(100.0); n.padding = UiRect::vertical(Val::Px(2.0)); }).tip(tip);
}

fn fact(u: &mut Ui, icon: Icon, col: Color, name: &str, value: String) {
    u.row(8.0, |r| { r.icon(icon, 15.0, col); r.text(name, 12.0, MUTED).style(|n| n.width = Val::Px(110.0)); r.text(value, 12.0, INK); });
}

pub(super) fn about(s: &sim::SimState, id: BeaverId) -> String {
    match s.colony.beaver(id) {
        Some(b) => format!("{} · {}", stage_word(b.stage), span(b.age_days as f64)),
        None => match s.lives.of.get(&id).and_then(|l| l.entries.last()) {
            Some(e) => match e.what {
                What::Died(c) => format!("died {} ({})", date(e.day), cause(c)),
                What::Left => format!("left {}", date(e.day)),
                _ => "gone".into(),
            },
            None => "gone".into(),
        },
    }
}

fn pts(v: f32) -> String { format!("{:.0}", v * 100.0) }
fn per_day(v: f32) -> String { format!("{:+.1}/d", v * 100.0) }
fn thresh_line(label: &str, val: String, bad: bool) -> TipLine { TipLine::new(label, val).color(if bad { BAD } else { MUTED }) }

pub(super) fn health_tip(b: &Beaver, r: &sim::colony::Rates) -> Tip {
    use sim::colony::{EXHAUST_BELOW, FREEZE_BELOW, HEAL_ABOVE, OLD_AGE_YEARS, STARVE_BELOW, WORK_HEALTH_BASE, WORK_HEALTH_SCALE};
    let mut tip = Tip::name("Health").with_value(pts(b.health));
    let mut any_harm = false;
    for (k, h) in r.harm.iter().enumerate().filter(|(_, h)| **h > 0.0) { any_harm = true; tip.push(TipLine::new(HARMS[k], per_day(-h)).color(BAD)); }
    if r.heal > 0.0 && b.health < 0.999 { tip.push(TipLine::new("Mending", per_day(r.heal)).color(GOOD)); }
    else if !any_harm && b.health >= 0.999 { tip.push(TipLine::new("Status", "Full health").color(GOOD)); }
    else if r.harm[..4].iter().all(|&h| h == 0.0) && b.health < 0.999 { tip.push(TipLine::new("Mends above", format!("Food & warmth {}", pts(HEAL_ABOVE))).color(WARN)); }
    tip.push(thresh_line("Starves below", format!("Food {}", pts(STARVE_BELOW)), b.needs.food < STARVE_BELOW));
    tip.push(thresh_line("Freezes below", format!("Warmth {}", pts(FREEZE_BELOW)), b.needs.warmth < FREEZE_BELOW));
    tip.push(thresh_line("Exhausted below", format!("Rest {}", pts(EXHAUST_BELOW)), b.needs.rest < EXHAUST_BELOW));
    tip.push(if r.old_age_death > 0.0 {
        TipLine::new("Old age", format!("{:.0}%/y death risk", (1.0 - (-r.old_age_death).exp()) * 100.0)).color(WARN)
    } else {
        TipLine::new("Old age", format!("After {OLD_AGE_YEARS:.0}y")).color(MUTED)
    });
    tip.line("Mood factor", format!("×{:.2}", r.mood_health)).line("Work pace", format!("×{:.2}", WORK_HEALTH_BASE + WORK_HEALTH_SCALE * b.health))
}

pub(super) fn mood_tip(b: &Beaver, r: &sim::colony::Rates) -> Tip {
    use sim::colony::{MOOD_FOOD_WEIGHT, MOOD_REST_WEIGHT, MOOD_SAFETY_WEIGHT, MOOD_SOCIAL_WEIGHT, MOOD_WARMTH_WEIGHT};
    let n = &b.needs;
    let share = |v: f32, w: f32| format!("{:.0} / {:.0}", v * w * 100.0, w * 100.0);
    let mut tip = Tip::name("Mood").with_value(pts(b.mood)).line("Target", pts(r.mood_to));
    for (name, v, w) in [(sim::buildings::Good::food().name(), n.food, MOOD_FOOD_WEIGHT), ("Warmth", n.warmth, MOOD_WARMTH_WEIGHT), ("Rest", n.rest, MOOD_REST_WEIGHT), ("Safety", n.safety, MOOD_SAFETY_WEIGHT), ("Company", n.social, MOOD_SOCIAL_WEIGHT)] {
        tip = tip.line(format!("{name} ({:.0}%)", w * 100.0), share(v, w));
    }
    tip = tip.line("Health factor", format!("×{:.2}", r.mood_health)).line("Work pace", format!("×{:.2}", sim::society::work_speed(b.mood)));
    if let Some(left) = r.leaves_in { tip.push(TipLine::new("Leaves colony in", days(left)).color(BAD)); }
    tip
}

pub(super) fn food_tip(b: &Beaver, r: &sim::colony::Rates) -> Tip {
    use sim::colony::{HEAL_ABOVE, MOOD_FOOD_WEIGHT, STARVE_BELOW};
    let mut tip = Tip::name(sim::buildings::Good::food().name()).with_value(pts(b.needs.food)).line("Hunger", per_day(-r.hunger)).line("Eats below", pts(r.eat_below));
    tip.push(thresh_line("Starves below", pts(STARVE_BELOW), b.needs.food < STARVE_BELOW));
    tip.line("Mends health above", pts(HEAL_ABOVE)).line("Mood weight", format!("{:.0}%", MOOD_FOOD_WEIGHT * 100.0))
}

pub(super) fn rest_tip(b: &Beaver, r: &sim::colony::Rates) -> Tip {
    use sim::colony::{EXHAUSTED_REST, EXHAUST_BELOW, MOOD_REST_WEIGHT, RESUME_WORK_REST};
    let state = if r.resting { "Resting off exhaustion" } else if b.current_job.is_some() { "Working" } else { "Idle" };
    let mut tip = Tip::name("Rest").with_value(pts(b.needs.rest)).line("Change", per_day(r.rest)).line("State", state)
        .line("Sleep quality", format!("×{:.2} ({})", r.sleep, if b.lodge.is_some() { "lodge" } else { "no lodge" }))
        .line("Rests below", format!("{} (to {})", pts(EXHAUSTED_REST), pts(RESUME_WORK_REST)));
    tip.push(thresh_line("Exhausted below", pts(EXHAUST_BELOW), b.needs.rest < EXHAUST_BELOW));
    tip.line("Mood weight", format!("{:.0}%", MOOD_REST_WEIGHT * 100.0))
}

pub(super) fn warmth_tip(s: &sim::SimState, b: &Beaver, r: &sim::colony::Rates) -> Tip {
    use sim::colony::{FREEZE_BELOW, MOOD_WARMTH_WEIGHT};
    let shelter = if b.in_lodge { format!("In lodge ({:.0}% insulated)", r.insulation * 100.0) }
        else if b.lodge.is_some() { format!("Outdoors ({:.0}%)", r.insulation * 100.0) }
        else { format!("No lodge ({:.0}%)", r.insulation * 100.0) };
    let mut tip = Tip::name("Warmth").with_value(pts(b.needs.warmth)).line("Target", pts(r.warmth_to))
        .line("Feels like", crate::menu::degrees(s.water.sky.at(s.time_days).feels_c)).line("Shelter", shelter);
    tip.push(thresh_line("Freezes below", pts(FREEZE_BELOW), b.needs.warmth < FREEZE_BELOW));
    tip.line("Mood weight", format!("{:.0}%", MOOD_WARMTH_WEIGHT * 100.0))
}

pub(super) fn safety_tip(b: &Beaver, r: &sim::colony::Rates) -> Tip {
    let mut tip = Tip::name("Safety").with_value(pts(b.needs.safety)).line("Target", pts(r.safety_to));
    tip = if b.in_lodge { tip.line("Where", "Inside lodge") }
        else if let Some((land, water)) = r.exposure {
            if land > 0.0 { tip.line("Where", "Working on land").line("To water", format!("{} m", num(water))) } else { tip.line("Where", "Working in water") }
        } else { tip.line("Where", "Not working") };
    tip.line("Mood weight", format!("{:.0}%", sim::colony::MOOD_SAFETY_WEIGHT * 100.0))
}

pub(super) fn company_tip(b: &Beaver, r: &sim::colony::Rates) -> Tip {
    use sim::colony::{MOOD_SOCIAL_WEIGHT, SOCIAL_BASE, SOCIAL_MAX_MATES, SOCIAL_PER_MATE};
    Tip::name("Company").with_value(pts(b.needs.social)).line("Target", pts(r.social_to))
        .line("Lodge-mates", if b.lodge.is_some() { format!("{} / {SOCIAL_MAX_MATES}", r.mates) } else { "No lodge".into() })
        .line("Per mate", format!("+{} (base {})", pts(SOCIAL_PER_MATE), pts(SOCIAL_BASE)))
        .line("Mood weight", format!("{:.0}%", MOOD_SOCIAL_WEIGHT * 100.0))
}

fn condition(c: &Ctx, u: &mut Ui, b: &Beaver) {
    let Some(r) = c.s.beaver_rates(b.id) else { return };
    u.heading(Icon::Health, "Condition");
    meter(u, Icon::Health, level_color(b.health), "Health", b.health, health_tip(b, &r));
    let (mi, mc) = mood_icon(b.mood);
    meter(u, mi, mc, "Mood", b.mood, mood_tip(b, &r));
    let n = &b.needs;
    for (icon, col, name, v, tip) in [
        (good_icon(sim::buildings::Good::food()), good_color(sim::buildings::Good::food()), sim::buildings::Good::food().name(), n.food, food_tip(b, &r)),
        (Icon::Idle, REST, "Rest", n.rest, rest_tip(b, &r)),
        (Icon::Winter, MUTED, "Warmth", n.warmth, warmth_tip(c.s, b, &r)),
        (Icon::Home, MUD, "Safety", n.safety, safety_tip(b, &r)),
        (Icon::Beaver, FUR, "Company", n.social, company_tip(b, &r)),
    ] { meter(u, icon, col, name, v, tip); }
    let harm = b.harm();
    if harm.iter().any(|h| *h > 0.005) {
        u.row(6.0, |r| {
            r.text("Harm taken", 12.0, MUTED).style(|n| n.width = Val::Px(110.0));
            for (k, &h) in harm.iter().enumerate().filter(|(_, h)| **h > 0.005) {
                r.text(format!("{} {:.0}", HARMS[k], h * 100.0), 11.0, if h > 0.3 { BAD } else { WARN });
            }
        });
    }
    if let Some(left) = r.leaves_in.filter(|_| b.low_mood_days() > 0.5) { fact(u, Icon::MoodBad, BAD, "Leaves in", days(left)); }
}

fn work(c: &Ctx, u: &mut Ui, b: &Beaver) {
    let s = c.s;
    u.heading(Icon::Hammer, "Work");
    let (ai, ac, doing) = super::inspect::activity(s, b);
    fact(u, ai, ac, "Now", doing.into());
    let (ji, _, jname, place) = super::inspect::job(s, b);
    fact(u, ji, INK, "Job", jname.into());
    if let Some((pace, tip)) = super::ledger::pace(s, b) {
        u.row(8.0, |r| { r.icon(Icon::Fast2, 15.0, INK); r.text("Pace", 12.0, MUTED).style(|n| n.width = Val::Px(110.0)); r.text(format!("×{pace:.2}"), 12.0, INK); }).tip(tip);
    }
    if let Some((bid, bb)) = place.and_then(|x| Some((x, s.buildings.building(x)?))) {
        u.row(8.0, |r| {
            r.icon(building_icon(bb.kind), 15.0, INK);
            r.text("Works at", 12.0, MUTED).style(|n| n.width = Val::Px(110.0));
            r.text(bb.kind.spec().name.as_str(), 12.0, INK);
            r.spacer();
            r.pebble(Icon::Focus, 11.0, HudAction::GoTo(NoticeAt::Building(bid)), Peb::Idle).tip(Tip::name("Show"));
        });
    }
    if let Some(a) = s.agents.get(b.id) {
        if let Some(l) = a.load.as_ref() {
            let walk = sim::agents::travel_pace(&s.nav, a);
            let mut tip = Tip::name("Carrying");
            if (walk - 1.0).abs() > 0.005 { tip = tip.line("Walk", format!("×{walk:.2}")); }
            u.row(8.0, |r| { r.icon(good_icon(l.good), 15.0, good_color(l.good)); r.text("Carrying", 12.0, MUTED).style(|n| n.width = Val::Px(110.0)); r.text(format!("{} {}", num(l.amount), l.good.name()), 12.0, INK); }).tip(tip);
        }
    }
    if let Some(p) = &b.pregnancy { fact(u, Icon::Kit, GOOD, "Expecting", format!("{} kits in {}", p.litter, days((p.due_day - s.time_days) as f32))); }
}

fn family(c: &Ctx, u: &mut Ui, id: BeaverId, b: Option<&Beaver>, life: Option<&Life>) {
    let s = c.s;
    let hi = building_icon(sim::buildings::BuildingKind::home());
    u.row(8.0, |r| {
        r.icon(Icon::Home, 16.0, ACCENT);
        r.bold("Family", 14.0, INK);
        r.spacer();
        match b.and_then(|b| b.lodge) {
            Some(l) => r.small_button(Some(hi), Some("Family tree"), Some(HudAction::Overlay(Some(Overlay::Page(Page::Lodge(l))))), Tip::name("Family page").line("Who lives in its lodge, and the family tree", ""), Small::Idle),
            None => r.small_button(Some(hi), Some("Family tree"), None, Tip::name("Family page").line("No home: a family page belongs to a lodge", ""), Small::Idle),
        };
    });
    let all = kin::everyone(s);
    let mine = kin::litter(s, &all, id);
    match s.beaver_conception(id) {
        Some(Ok(p)) => fact(u, Icon::Kit, GOOD, "Conceiving", format!("{:.1}%/d", p * 100.0)),
        Some(Err(why)) => fact(u, Icon::Kit, MUTED, "No kits yet", why.into()),
        None => {}
    }
    if let Some(l) = mine { fact(u, Icon::Kit, GOOD, "Litter", format!("{} of {}'s · {} · {} kits", l.number, name_of(s, l.mother), kin::year_word(l.year), kin::litter_mates(s, &all, l).len())); }
    let my_lodge = b.and_then(|b| b.lodge);
    let at_home = |o: BeaverId| my_lodge.is_some() && s.colony.beaver(o).and_then(|x| x.lodge) == my_lodge;
    let mut ties: Vec<_> = all.iter().filter_map(|&o| kin::kinship(s, id, o).map(|(ord, w)| (ord, w, o)).or_else(|| (o != id && at_home(o)).then_some((kin::LODGER, "Lodge-mate", o)))).collect();
    ties.sort_by(|a, b| a.0.cmp(&b.0).then(kin::born(s, a.2).unwrap_or(f64::MAX).total_cmp(&kin::born(s, b.2).unwrap_or(f64::MAX))).then(a.2.cmp(&b.2)));
    let count = |f: KinFilter| ties.iter().filter(|t| f.takes(t.0, at_home(t.2))).count();
    let filter = if count(c.hud.kin) == 0 { KinFilter::All } else { c.hud.kin };
    u.row(4.0, |r| {
        for f in KinFilter::ALL { let n = count(f); if n > 0 || f == KinFilter::All { super::ledger::chip(r, &format!("{} {n}", f.label()), f == filter, HudAction::Kin(f)); } }
    }).style(|n| { n.flex_wrap = FlexWrap::Wrap; n.row_gap = Val::Px(4.0); });
    let shown: Vec<_> = ties.iter().filter(|t| filter.takes(t.0, at_home(t.2))).collect();
    if shown.is_empty() {
        u.text("No family in the colony", 12.0, MUTED);
    } else {
        u.row(6.0, |r| for (w, h) in [(92.0, "Tie"), (104.0, "Beaver"), (84.0, "Litter")] { r.text(h, 11.0, MUTED).style(move |n| n.width = Val::Px(w)); });
    }
    let my_year = kin::born_year(s, id);
    for &&(order, word, o) in &shown {
        let theirs = kin::litter(s, &all, o);
        let litter = match (order, theirs) {
            (3, Some(t)) if mine.is_some_and(|m| m.mother == t.mother && m.year == t.year) => "Littermate".into(),
            (3, _) => match (kin::born_year(s, o), my_year) {
                (Some(y), Some(me)) if y < me => format!("Older · {}", kin::year_word(y)),
                (Some(y), Some(me)) if y > me => format!("Younger · {}", kin::year_word(y)),
                (Some(y), _) => format!("Same year · {}", kin::year_word(y)),
                _ => "—".into(),
            },
            (_, Some(t)) => format!("#{} · {}", t.number, kin::year_word(t.year)),
            (_, None) => "—".into(),
        };
        let ltip = match theirs {
            Some(l) => Tip::name(format!("Litter {} of {}", l.number, name_of(s, l.mother))).line("Born", kin::year_word(l.year)).line("Kits in it", format!("{}", kin::litter_mates(s, &all, l).len())),
            None => Tip::name("Litter not known").line("Came in as a newcomer, or before records began", ""),
        };
        let since = if order == 0 { life.and_then(|l| l.partnered_since()).map_or(String::new(), |d| format!(" · since {}", date(d))) } else { String::new() };
        let note = format!("{}{since}{}", about(s, o), if at_home(o) { " · home" } else { "" });
        let nm = name_of(s, o);
        u.row(6.0, |r| {
            r.text(word, 11.0, MUTED).style(|n| n.width = Val::Px(92.0));
            r.row(5.0, |x| { x.token(Icon::Beaver, 13.0, None); x.bold(nm.clone(), 12.0, if s.colony.beaver(o).is_some() { INK } else { MUTED }); })
                .style(|n| n.width = Val::Px(104.0)).action(HudAction::Overlay(Some(Overlay::Page(Page::Beaver(o))))).tip(Tip::name(nm).line("Open their page", ""));
            r.text(litter, 11.0, INK).style(|n| n.width = Val::Px(84.0)).tip(ltip);
            r.text(note, 11.0, MUTED);
        });
    }
    u.rule();
    u.heading(hi, "Home");
    match b.and_then(|b| b.lodge).and_then(|l| s.buildings.building(l)) {
        Some(l) => {
            fact(u, hi, MUD, sim::buildings::BuildingKind::home().name(), s.colony.lodge_family(l.id).and_then(|f| s.colony.family(f)).map(|f| f.name.clone()).unwrap_or_default());
            if let Some(d) = life.and_then(|l| l.home_since()) { fact(u, Icon::Hourglass, MUTED, "Since", date(d)); }
        }
        None => { u.text("No home", 12.0, MUTED); }
    }
}

fn skills(c: &Ctx, u: &mut Ui, b: &Beaver) {
    u.heading(Icon::Jobs, "Skills");
    let rec = c.s.deeds.get(b.id);
    let mut tip = Tip::name("Experience").with_value(format!("{:.0}", b.experience * 100.0)).line("Gained by", "Every day worked, in any job");
    for (stat, change) in sim::growth::experience_lines(b) { tip = tip.line(stat, change); }
    u.row(8.0, |r| {
        r.icon(Icon::Hourglass, 15.0, ACCENT);
        r.bold("Experience", 12.0, INK).style(|n| n.width = Val::Px(110.0));
        r.ticks(b.experience.clamp(0.0, 1.0), 10, 120.0, 9.0, ACCENT);
        r.spacer();
        r.bold(format!("{:.0}", b.experience * 100.0), 12.0, ACCENT);
    }).tip(tip);
    u.rule();
    for j in Job::all() {
        let v = b.skills[j];
        let mut tip = Tip::name(j.row().skill.as_str()).with_value(format!("{:.0}", v * 100.0)).line("Gives", j.row().skill_effect.as_str()).line("Now", sim::growth::effect_now(b, j));
        if let Some(d) = rec.map(|r| r.days[j] as f64).filter(|d| *d >= 1.0) { tip = tip.line("Time at it", span(d)); }
        u.row(8.0, |r| {
            r.icon(job_icon(j), 15.0, INK);
            r.text(j.row().skill.as_str(), 12.0, MUTED).style(|n| n.width = Val::Px(110.0));
            r.ticks(v.clamp(0.0, 1.0), 10, 120.0, 9.0, GOOD);
            r.spacer();
            r.bold(format!("{:.0}", v * 100.0), 12.0, INK);
        }).tip(tip);
    }
}

fn bonuses(u: &mut Ui, b: &Beaver) {
    u.heading(Icon::Fast2, "Bonuses");
    for x in sim::growth::bonuses(b) {
        let some = x.change.abs() >= 0.005;
        let value = if some { format!("{:+.0}%", x.change * 100.0) } else { "-".into() };
        let mut tip = Tip::name(x.stat).with_value(value.clone());
        if !x.from.is_empty() { tip = tip.line("From", x.from); }
        if x.less_is_better { tip = tip.line("Less is better", ""); }
        u.row(8.0, |r| { r.text(x.stat, 12.0, MUTED); r.spacer(); r.bold(value, 12.0, if some { GOOD } else { MUTED }); }).tip(tip);
    }
}

fn records(c: &Ctx, u: &mut Ui, id: BeaverId) {
    use sim::buildings::{BuildingKind, Good};
    let s = c.s;
    let Some(r) = s.deeds.get(id) else { return };
    let (km, n) = (|cells: f32| format!("{:.1} km", cells * s.water.cell_m / 1000.0), |v: f32| format!("{}", v.round() as i64));
    let mut chips = Vec::new();
    if r.walked >= 1.0 { chips.push((Icon::Walk, INK, km(r.walked), Tip::name("Walked"))); }
    if r.swum >= 1.0 { chips.push((Icon::Drop, WATER, km(r.swum), Tip::name("Swum"))); }
    let felled: u32 = r.felled.values().sum();
    if felled > 0 {
        let mut t = Tip::name("Trees felled");
        for (sp, k) in &r.felled { t = t.line(sp.name(), format!("{k}")); }
        if r.great_trees > 0 { t = t.line("Great old trees", format!("{}", r.great_trees)); }
        chips.push((resource_icon(sim::resources::ResourceKind::tree()), GOOD, format!("{felled}"), t));
    }
    if r.gathered[Good::food()] >= 1.0 {
        let mut t = Tip::name("Food gathered");
        for (p, f) in &r.harvested { t = t.line(p.name(), n(*f)); }
        if r.bark >= 1.0 { t = t.line("Bark", n(r.bark)); }
        if r.winter_food >= 1.0 { t = t.line("In winter", n(r.winter_food)); }
        chips.push((good_icon(Good::food()), good_color(Good::food()), n(r.gathered[Good::food()]), t));
    }
    for g in Good::all().filter(|g| !g.row().edible && r.gathered[*g] >= 1.0) {
        chips.push((good_icon(g), good_color(g), n(r.gathered[g]), Tip::name(format!("{} gathered", g.name()))));
    }
    let carried: f32 = r.carried.values().sum();
    if carried >= 1.0 {
        let mut rows: Vec<_> = r.carried.iter().collect();
        rows.sort_by(|a, b| b.1.total_cmp(a.1));
        let t = rows.into_iter().take(8).fold(Tip::name("Carried"), |t, ((g, k), v)| t.line(format!("{} → {}", g.name(), k.spec().name.as_str()), n(*v)));
        chips.push((Icon::Haul, INK, n(carried), t));
    }
    let rafted = r.rafted_total();
    if rafted >= 1.0 {
        let t = Good::all().filter(|&g| r.rafted[g] >= 1.0).fold(Tip::name("Moved by raft"), |t, g| t.line(g.name(), n(r.rafted[g])));
        chips.push((Icon::Raft, WATER, n(rafted), t));
    }
    let structure = |b: sim::buildings::BuildingId, kind: Option<BuildingKind>| -> String {
        if let Some(p) = sim::names::pond_name(s, b) { return p.into(); }
        s.colony.lodge_family(b).and_then(|f| s.colony.family(f))
            .map(|f| f.name.clone()).or_else(|| kind.map(|k| k.spec().name.to_string())).unwrap_or_else(|| "Gone".into())
    };
    if r.dam_loads > 0 {
        let mut dams: Vec<_> = r.works.iter().filter(|(_, w)| w.kind == Some(BuildingKind::dam()) && w.loads > 0).collect();
        dams.sort_by(|a, b| b.1.loads.cmp(&a.1.loads));
        let t = dams.into_iter().take(6).fold(Tip::name("Dam loads packed"), |t, (b, w)| t.line(structure(*b, w.kind), format!("{}", w.loads)));
        chips.push((building_icon(BuildingKind::dam()), MUD, format!("{}", r.dam_loads), t));
    }
    if r.canal_cells > 0 { chips.push((building_icon(BuildingKind::canal()), WATER, format!("{}", r.canal_cells), Tip::name("Canal cells dug"))); }
    if r.gates > 0 { chips.push((Icon::Lock, INK, format!("{}", r.gates), Tip::name("Gates set"))); }
    let built: u32 = r.built.iter().filter(|(k, _)| !k.is_waterworks()).map(|(_, v)| v).sum();
    if built > 0 {
        let t = r.built.iter().filter(|(k, _)| !k.is_waterworks()).fold(Tip::name("Buildings raised"), |t, (k, v)| t.line(k.spec().name.as_str(), format!("{v}")));
        chips.push((Icon::Building, MUD, format!("{built}"), t));
    }
    if r.mended >= 1.0 {
        let mut t = Tip::name("Mending done");
        if r.leaks > 0 { t = t.line("Leaks stopped", format!("{}", r.leaks)); }
        let mut fixed: Vec<_> = r.works.iter().filter(|(_, w)| w.repairs >= 0.5).collect();
        fixed.sort_by(|a, b| b.1.repairs.total_cmp(&a.1.repairs));
        for (b, w) in fixed.into_iter().take(6) { t = t.line(structure(*b, w.kind), n(w.repairs)); }
        chips.push((Icon::Hammer, INK, n(r.mended), t));
    }
    if r.planted > 0 {
        let mut t = Tip::name("Seedlings planted");
        if r.grown > 0 { t = t.line("Grown into trees", format!("{}", r.grown)); }
        chips.push((good_icon(Good::seedling()), GOOD, format!("{}", r.planted), t));
    }
    if r.seedlings > 0 { chips.push((good_icon(Good::seedling()), GOOD, format!("{}", r.seedlings), Tip::name("Seedlings taken"))); }
    if r.wolves > 0 {
        let mut t = Tip::name("Wolves spotted");
        if r.warned > 0 { t = t.line("Beavers warned", format!("{}", r.warned)); }
        chips.push((Icon::Wolf, BAD, format!("{}", r.wolves), t));
    }
    let (born, raised) = sim::deeds::kits(s, id);
    if born > 0 { chips.push((Icon::Kit, FUR, format!("{born}"), Tip::name("Kits").line("Grew up", format!("{raised}")))); }
    if chips.is_empty() { return; }
    u.rule();
    u.heading(Icon::Hourglass, "Life's work");
    u.node(Node { flex_direction: FlexDirection::Row, flex_wrap: FlexWrap::Wrap, column_gap: Val::Px(14.0), row_gap: Val::Px(8.0), ..default() }, |g| {
        for (icon, col, value, tip) in chips {
            g.row(4.0, |x| { x.icon(icon, 15.0, col); x.bold(value, 12.0, INK); }).style(|n| n.min_width = Val::Px(64.0)).tip(tip);
        }
    });
}

fn chronology(c: &Ctx, u: &mut Ui, life: Option<&Life>) {
    let s = c.s;
    u.heading(Icon::Journal, "Life in the colony");
    let Some(life) = life else { u.text("No record yet: it starts with the next look at the colony", 12.0, MUTED); return; };
    if let Some(d) = life.arrived() { fact(u, Icon::Hourglass, MUTED, "In the colony", format!("since {} ({})", date(d), span(s.time_days - d))); }
    let (her, hi) = (if life.sex == sim::colony::Sex::Male { "his" } else { "her" }, building_icon(sim::buildings::BuildingKind::home()));
    for e in life.entries.iter().rev() {
        let (icon, col, words) = match e.what {
            What::Known => (Icon::Beaver, MUTED, "In the colony when its records began".into()),
            What::Born { mother, father } => {
                let p: Vec<_> = [mother, father].into_iter().flatten().map(|p| name_of(s, p)).collect();
                (Icon::Kit, GOOD, if p.is_empty() { "Born".into() } else { format!("Born to {}", p.join(" and ")) })
            }
            What::Joined => (Icon::Beaver, GOOD, "Joined the colony as a newcomer".into()),
            What::Grew(st) => (stage_icon(st), INK, format!("Grew up: {}", stage_word(st).to_lowercase())),
            What::Job(Some(r)) => (job_icon(r), INK, format!("Became a {}", r.name().to_lowercase())),
            What::Job(None) => (Icon::Idle, ACCENT, "Left free of work".into()),
            What::Partner(Some(p)) => (Icon::MoodGood, GOOD, format!("Paired with {}", name_of(s, p))),
            What::Partner(None) => (Icon::MoodBad, MUTED, format!("Lost {her} partner")),
            What::Moved(Some(l)) => (hi, MUD, match s.colony.lodge_family(l).and_then(|f| s.colony.family(f)) {
                Some(f) => format!("Moved into the {} lodge", f.name),
                None => "Moved into a lodge".into(),
            }),
            What::Moved(None) => (hi, MUTED, format!("Left {her} lodge")),
            What::Died(cause_of) => (Icon::Cross, BAD, format!("Died of {}", cause(cause_of))),
            What::Left => (Icon::Walk, MUTED, "Left the colony".into()),
        };
        u.row(8.0, |r| { r.text(date(e.day), 11.0, MUTED).style(|n| n.width = Val::Px(92.0)); r.icon(icon, 14.0, col); r.text(words, 12.0, INK); });
    }
}

pub fn screen(c: &Ctx, id: BeaverId) -> Vec<VNode> {
    let (s, b, life) = (c.s, c.s.colony.beaver(id), c.s.lives.of.get(&id));
    Ui::full_page_shell(
        |r| {
            r.col(4.0, |x| {
                x.bold(full_name(s, id, &name_of(s, id)), 20.0, INK);
                match b {
                    Some(b) => {
                        let sex = if b.sex == sim::colony::Sex::Male { "Male" } else { "Female" };
                        x.row(8.0, |y| { y.icon(stage_icon(b.stage), 15.0, INK); y.text(format!("{sex} · {} · {} old", stage_word(b.stage), span(b.age_days as f64)), 13.0, MUTED); });
                    }
                    None => { x.text(about(s, id), 13.0, MUTED); }
                }
            });
            r.spacer();
            if b.is_some() { r.small_button(Some(Icon::Focus), None, Some(HudAction::GoTo(NoticeAt::Beaver(id))), Tip::name("Show on the map"), Small::Idle); }
            page_controls(c, r, HudAction::Overlay(None));
        },
        |u| {
            u.row(28.0, |cols| {
                let col = |cols: &mut Ui, f: &dyn Fn(&mut Ui)| {
                    cols.node(Node { flex_grow: 1.0, flex_basis: Val::Px(0.0), min_width: Val::Px(0.0), max_width: Val::Px(COL_MAX), height: Val::Percent(100.0), min_height: Val::Px(0.0), flex_direction: FlexDirection::Column, row_gap: Val::Px(7.0), ..default() }, |u| f(u)).scroll_y();
                };
                col(cols, &|u| {
                    if let Some(p) = c.portrait { u.image(p.0.clone(), Node { width: Val::Percent(100.0), aspect_ratio: Some(PORTRAIT_ASPECT), flex_shrink: 0.0, ..default() }).skin(Skin::Portrait); }
                    match b { Some(b) => condition(c, u, b), None => { u.text("No longer in the colony", 13.0, MUTED); } }
                    records(c, u, id);
                });
                col(cols, &|u| { if let Some(b) = b { work(c, u, b); u.rule(); } family(c, u, id, b, life); });
                col(cols, &|u| { if let Some(b) = b { skills(c, u, b); u.rule(); bonuses(u, b); u.rule(); } chronology(c, u, life); });
            }).style(|n| { n.align_items = AlignItems::Stretch; n.width = Val::Percent(100.0); n.flex_grow = 1.0; n.min_height = Val::Px(0.0); });
        },
    )
}
