use super::ledger::{chart, Bar};
use super::panels::Ctx;
use super::ui::*;
use super::words::*;
use super::{HudAction, NoticeAt, Overlay};
use crate::icons::Icon;
use bevy::prelude::*;
use sim::buildings::{BuildingId, Good};
use sim::works::{Spend, Tally, SEASON};

pub(super) fn structure_name(s: &sim::SimState, id: BuildingId) -> String {
    match s.buildings.building(id) {
        Some(b) if b.kind.is_dam() => sim::names::pond_name(s, id).unwrap_or(b.kind.name()).to_string(),
        Some(b) => b.kind.name().to_string(),
        None => "Gone".into(),
    }
}

fn spend_word(sp: Spend) -> &'static str { match sp { Spend::Build => "Building", Spend::Upkeep => "Upkeep" } }
fn work(d: f32) -> String { format!("{} beaver-days", num(d)) }

fn amounts(r: &mut Ui, t: &Tally, sp: Spend) {
    for (g, a) in t.goods_spent(sp) {
        r.row(3.0, |x| { x.icon(good_icon(g), 13.0, good_color(g)); x.bold(num(a), 12.0, INK); }).tip(Tip::name(g.name()).with_value(num(a)));
    }
    r.row(3.0, |x| { x.icon(Icon::Beaver, 13.0, FUR); x.bold(format!("{}d", num(t.work_days(sp))), 12.0, INK); }).tip(Tip::name("Work").with_value(work(t.work_days(sp))));
}

fn lines(mut tip: Tip, t: &Tally, sp: Spend) -> Tip {
    for (g, a) in t.goods_spent(sp) { tip = tip.line(g.name(), num(a)); }
    tip.line("Work", work(t.work_days(sp)))
}

pub(super) fn upkeep_row(c: &Ctx, u: &mut Ui, id: BuildingId) { upkeep_over(c, u, id, super::Span::Season); }

pub(super) fn upkeep_over(c: &Ctx, u: &mut Ui, id: BuildingId, span: super::Span) {
    let (book, today) = (c.s.works.book(id), sim::works::today(c.s));
    let (last, when, none) = match span {
        super::Span::Season => (book.map(|b| b.last_season(today)).unwrap_or_default(), format!("The last {SEASON} days"), "None last season"),
        super::Span::Year => {
            let mut t = Tally::default();
            if let Some(b) = book { for (_, x) in b.by_season(today).iter().rev().take(4) { t.add(x); } }
            (t, "The last four seasons".into(), "None last year")
        }
        super::Span::All => (book.map(|b| b.life().clone()).unwrap_or_default(), "Since it was begun".into(), "None yet"),
    };
    let mut tip = lines(Tip::name("Upkeep").line(when, ""), &last, Spend::Upkeep);
    if let Some(b) = book {
        tip = lines(lines(tip.line("Upkeep in all", ""), b.life(), Spend::Upkeep).line("Building it", ""), b.life(), Spend::Build);
    }
    u.row(6.0, |r| {
        r.icon(Icon::Hammer, 13.0, MUTED);
        r.text("Upkeep", 11.0, MUTED);
        r.spacer();
        if last.goods_spent(Spend::Upkeep).is_empty() && last.work_days(Spend::Upkeep) <= 0.0 { r.text(none, 11.0, MUTED); }
        else { amounts(r, &last, Spend::Upkeep); }
    }).tip(tip);
}

pub(super) fn page_button(c: &Ctx, r: &mut Ui, id: BuildingId) {
    r.pebble(Icon::Journal, 13.0, HudAction::Overlay(Some(Overlay::Page(super::Page::Works(id)))), if c.hud.page() == Some(super::Page::Works(id)) { Peb::On } else { Peb::Idle })
        .tip(Tip::name("Details").line("What it has cost, and who worked on it", ""));
}

fn by_season(u: &mut Ui, seasons: &[(u64, Tally)], icon: Icon, col: Color, name: &str, value: String, f: impl Fn(&Tally, Spend) -> f32, unit: &dyn Fn(f32) -> String) {
    let data: Vec<Bar> = seasons.iter().map(|(n, t)| Bar { a: f(t, Spend::Upkeep), b: f(t, Spend::Build), down: 0.0, day0: n * SEASON, day1: (n + 1) * SEASON - 1 }).collect();
    chart(u, icon, col, Color::srgb(0.55, 0.55, 0.6), name, value, INK, &data, false, |b| {
        let d = sim::calendar::Date::at(b.day0 as f64);
        Tip::titled(Some((icon, col)), format!("{}, year {}", d.season.name(), d.year)).line("Upkeep", unit(b.a)).line("Building", unit(b.b))
    });
}

fn season_charts(u: &mut Ui, seasons: &[(u64, Tally)], last: &Tally, goods: impl Iterator<Item = Good>) {
    by_season(u, seasons, Icon::Beaver, FUR, "Work each season (upkeep, with building above)", format!("{}d upkeep", num(last.work_days(Spend::Upkeep))), |t, sp| t.work_days(sp), &work);
    for g in goods {
        by_season(u, seasons, good_icon(g), good_color(g), &format!("{} each season (upkeep, with building above)", g.name()), format!("{} upkeep", num(last.goods(Spend::Upkeep, g))), |t, sp| t.goods(sp, g), &num);
    }
}

pub fn screen(c: &Ctx, id: BuildingId) -> Vec<VNode> {
    let s = c.s;
    let Some(b) = s.buildings.building(id) else { return Vec::new() };
    let (today, empty) = (sim::works::today(s), sim::works::Book::default());
    let book = s.works.book(id).unwrap_or(&empty);
    let (last, life, seasons, daily) = (book.last_season(today), book.life().clone(), book.by_season(today), book.daily(today));
    let mut crew: Vec<_> = s.deeds.of.iter().filter_map(|(who, r)| r.works.get(&id).map(|w| (*who, w.labour, w.repairs))).filter(|w| w.1 + w.2 > 0.0).collect();
    crew.sort_by(|a, b| (b.1 + b.2).total_cmp(&(a.1 + a.2)).then(a.0.cmp(&b.0)));
    let cond = match (b.dam.as_deref(), b.canal.as_ref()) { (Some(d), _) => sim::dams::health(s, d), (_, Some(k)) => sim::canals::health(s, k), _ => b.condition };
    Ui::full_page_shell(
        |r| {
            r.portrait(building_icon(b.kind), MUD);
            r.col(6.0, |x| {
                x.bold(structure_name(s, id), 20.0, INK);
                x.text(b.kind.name(), 12.0, MUTED);
                x.pips(cond, level_color(cond)).tip(Tip::name("Condition").with_value(format!("{:.0}%", cond * 100.0)));
            });
            r.spacer();
            r.pebble(Icon::Focus, 13.0, HudAction::GoTo(NoticeAt::Building(id)), Peb::Idle).tip(Tip::name("Show on the map"));
            super::life::page_controls(c, r, HudAction::Overlay(None));
        },
        |u| {
            u.row(28.0, |cols| {
                cols.node(Node { width: Val::Px(330.0), flex_direction: FlexDirection::Column, row_gap: Val::Px(8.0), flex_shrink: 0.0, ..default() }, |u| {
                    u.heading(Icon::Hammer, "What it has cost");
                    for sp in [Spend::Upkeep, Spend::Build] {
                        u.bold(spend_word(sp), 12.0, INK);
                        for (when, t) in [("Last season", &last), ("In all", &life)] {
                            u.row(6.0, |r| { r.text(when, 11.0, MUTED).style(|n| n.width = Val::Px(76.0)); amounts(r, t, sp); })
                                .tip(lines(Tip::name(format!("{}, {}", spend_word(sp), when.to_lowercase())), t, sp));
                        }
                    }
                    u.rule();
                    u.heading(Icon::Beaver, "Who worked on it");
                    if crew.is_empty() { u.text("Nobody yet", 12.0, MUTED); }
                    let per_day = sim::work::LABOUR_PER_DAY;
                    for &(who, built, mended) in crew.iter().take(12) {
                        let name = super::kin::name_of(s, who);
                        u.row(8.0, |r| {
                            r.bold(name.clone(), 12.0, if s.colony.beaver(who).is_none() { MUTED } else { INK });
                            r.spacer();
                            r.text(format!("{}d building · {}d upkeep", num(built / per_day), num(mended / per_day)), 11.0, MUTED);
                            r.icon(Icon::Next, 10.0, MUTED);
                        }).action(HudAction::Overlay(Some(Overlay::Page(super::Page::Beaver(who)))))
                            .tip(Tip::name(name).line("Building", work(built / per_day)).line("Upkeep", work(mended / per_day)).line("Open their page", ""));
                    }
                });
                cols.col(12.0, |u| {
                    u.heading(Icon::Hourglass, "Over time");
                    if seasons.is_empty() { u.text("Nothing spent on it yet", 12.0, MUTED); return; }
                    season_charts(u, &seasons, &last, Good::all().filter(|&g| life.goods(Spend::Build, g) + life.goods(Spend::Upkeep, g) > 0.0));
                    let first = today.saturating_sub(SEASON - 1);
                    let data: Vec<Bar> = daily.iter().enumerate().map(|(i, t)| Bar { a: t.work_days(Spend::Upkeep), b: t.work_days(Spend::Build), down: 0.0, day0: first + i as u64, day1: first + i as u64 }).collect();
                    chart(u, Icon::Beaver, FUR, Color::srgb(0.55, 0.55, 0.6), "Work each day, last season (upkeep, with building above)", format!("{}d", num(last.work_days(Spend::Upkeep) + last.work_days(Spend::Build))), INK, &data, false, |b| {
                        Tip::titled(Some((Icon::Beaver, FUR)), date(b.day0 as f64)).line("Upkeep", work(b.a)).line("Building", work(b.b))
                    });
                });
            }).style(|n| { n.align_items = AlignItems::FlexStart; n.flex_grow = 1.0; n.min_height = Val::Px(0.0); });
        },
    )
}

fn costliest(s: &sim::SimState) -> Vec<(BuildingId, Tally)> {
    let today = sim::works::today(s);
    let mut v: Vec<_> = s.buildings.buildings.iter()
        .filter_map(|b| Some((b.id, s.works.book(b.id)?.last_season(today))))
        .filter(|(_, t)| t.work_days(Spend::Upkeep) > 0.0 || !t.goods_spent(Spend::Upkeep).is_empty())
        .collect();
    let weight = |t: &Tally| t.work_days(Spend::Upkeep) + t.goods_spent(Spend::Upkeep).iter().map(|g| g.1).sum::<f32>();
    v.sort_by(|a, b| weight(&b.1).total_cmp(&weight(&a.1)).then(a.0.cmp(&b.0)));
    v
}

pub(super) fn upkeep_tab(c: &Ctx, u: &mut Ui) {
    let (s, today) = (c.s, sim::works::today(c.s));
    let (last, seasons, life) = (s.works.colony.last_season(today), s.works.colony.by_season(today), s.works.colony.life().clone());
    u.row(10.0, |r| { r.icon(Icon::Hammer, 16.0, ACCENT); r.bold("Upkeep, last season", 13.0, INK); r.spacer(); amounts(r, &last, Spend::Upkeep); })
        .tip(lines(Tip::name("Upkeep, last season").line(format!("The last {SEASON} days, every structure"), ""), &last, Spend::Upkeep));
    u.row(10.0, |r| { r.icon(Icon::Building, 16.0, MUTED); r.text("Building, last season", 12.0, MUTED); r.spacer(); amounts(r, &last, Spend::Build); })
        .tip(lines(Tip::name("Building, last season"), &last, Spend::Build));
    if seasons.is_empty() { return; }
    u.rule();
    u.node(Node { flex_direction: FlexDirection::Row, flex_wrap: FlexWrap::Wrap, column_gap: Val::Px(16.0), row_gap: Val::Px(12.0), ..default() }, |g| {
        season_charts(g, &seasons, &last, Good::all().filter(|&x| life.goods(Spend::Upkeep, x) > 0.0));
    });
    u.rule();
    let list = costliest(s);
    if list.is_empty() { u.text("Nothing needed upkeep last season", 12.0, MUTED); return; }
    u.col(4.0, |rows| {
        for (id, t) in &list {
            let Some(b) = s.buildings.building(*id) else { continue };
            let row = rows.row(8.0, |r| {
                r.icon(building_icon(b.kind), 14.0, MUD);
                r.bold(structure_name(s, *id), 12.0, INK).style(|n| n.width = Val::Px(160.0));
                r.spacer();
                amounts(r, t, Spend::Upkeep);
            });
            row.node.height = Val::Px(22.0);
            let open = if b.dam.is_some() || b.canal.is_some() { HudAction::Overlay(Some(Overlay::Page(super::Page::Works(*id)))) } else { HudAction::GoTo(NoticeAt::Building(*id)) };
            row.action(open).tip(lines(Tip::name(structure_name(s, *id)).line("Upkeep, last season", ""), t, Spend::Upkeep));
        }
    });
}
