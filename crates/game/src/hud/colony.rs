//! The colony (Tab): everything about the beavers — the families in their lodges, the work,
//! every beaver in a table, and this run. The first two are a drawer beside the map that
//! never covers the pond; the table and the run are wider pages (drawn in `ledger`).
//! Beavers are tokens, amounts are ticks, and words live in tooltips.
use super::panels::{good_tip, stage_icon, Ctx};
use super::ui::*;
use super::words::*;
use super::{ColonyTab, HudAction, NoticeAt, Overlay};
use crate::icons::Icon;
use bevy::prelude::*;
use sim::buildings::Good;

pub const DRAWER_W: f32 = 380.0;

const TABS: [(ColonyTab, Icon, &str); 4] = [
    (ColonyTab::Jobs, Icon::Hammer, "Work"),
    (ColonyTab::Population, Icon::Home, "Families"),
    (ColonyTab::Beavers, Icon::Beaver, "Beavers"),
    (ColonyTab::Run, Icon::Hourglass, "This run"),
];

pub fn screen(c: &Ctx, tab: ColonyTab) -> Vec<VNode> {
    let width = if matches!(tab, ColonyTab::Beavers | ColonyTab::Run) { super::ledger::LEDGER_W } else { DRAWER_W };
    Ui::book_shell(
        width,
        |r| {
            for (t, icon, name) in TABS {
                let on = t == tab;
                r.pebble(icon, 16.0, HudAction::Overlay(Some(Overlay::Colony(t))), if on { Peb::On } else { Peb::Idle }).tip(Tip::name(name));
            }
            r.spacer();
            if tab == ColonyTab::Run {
                super::ledger::span_chips(c, r);
            }
            r.close_button(HudAction::Overlay(None), true);
        },
        |body| match tab {
            ColonyTab::Population => beavers(c, body),
            ColonyTab::Jobs => work(c, body),
            ColonyTab::Beavers => super::ledger::beavers(c, body),
            ColonyTab::Run => super::ledger::run(c, body),
        },
    )
}

/// The pip under a beaver token: green working, yellow waiting, amber free, blue resting or too young.
pub(super) fn doing(s: &sim::SimState, b: &sim::colony::Beaver) -> Color {
    if !b.can_work() || b.resting {
        REST
    } else if sim::jobs::waiting(s, b.id).is_some() {
        WARN
    } else if s.work.slot_of(b.id).is_some() || s.agents.get(b.id).is_some_and(|a| a.assignment.is_some()) {
        GOOD
    } else {
        ACCENT
    }
}

pub(super) fn token_px(st: sim::colony::Stage) -> f32 {
    match st {
        sim::colony::Stage::Adult => 24.0,
        sim::colony::Stage::Yearling => 19.0,
        sim::colony::Stage::Kit => 15.0,
    }
}

/// One row per family: its name, then its beavers as tokens.
fn beavers(c: &Ctx, u: &mut Ui) {
    let s = c.s;
    for f in &s.colony.families {
        let members: Vec<&sim::colony::Beaver> = s.colony.beavers.iter().filter(|b| b.family == f.id).collect();
        if members.is_empty() {
            continue;
        }
        u.row(8.0, |r| {
            r.icon(Icon::Home, 16.0, MUD);
            r.bold(f.name.clone(), 14.0, INK);
        });
        u.node(Node { flex_direction: FlexDirection::Row, flex_wrap: FlexWrap::Wrap, column_gap: Val::Px(10.0), row_gap: Val::Px(8.0), align_items: AlignItems::FlexEnd, padding: UiRect::left(Val::Px(4.0)), ..default() }, |r| {
            for b in &members {
                let t = Tip::titled(Some((stage_icon(b.stage), INK)), b.name.clone())
                    .line("Age", span(b.age_days as f64))
                    .line("Health", format!("{:.0}", b.health * 100.0))
                    .line("Mood", format!("{:.0}", b.mood * 100.0));
                r.token(Icon::Beaver, token_px(b.stage), Some(doing(s, b))).action(HudAction::GoTo(NoticeAt::Beaver(b.id))).peek(b.id).tip(t);
            }
        });
        u.rule();
    }
    let due: Vec<(f64, u32)> = s.colony.beavers.iter().filter_map(|b| b.pregnancy.as_ref()).map(|p| (p.due_day, p.litter as u32)).collect();
    if let Some(next) = due.iter().map(|p| p.0).reduce(f64::min) {
        let kits: u32 = due.iter().map(|p| p.1).sum();
        u.row(6.0, |r| {
            for _ in 0..kits.min(6) {
                r.icon(Icon::Beaver, 15.0, FUR);
            }
            r.bold(format!("{:.0}d", (next - s.time_days).max(0.0)), 14.0, INK);
        })
        .tip(Tip::name(format!("{kits} kits on the way")).line("First due in", format!("{:.0} days", (next - s.time_days).max(0.0))));
    }
}

/// Shore limits a role steps through, widest first, in cells from water.
const SHORE_STEPS: [u16; 4] = [40, 20, 10, 5];

/// Who does what: one row per kind of work, its beavers as tokens and open places as rings.
fn work(c: &Ctx, u: &mut Ui) {
    let s = c.s;
    for row in sim::jobs::jobs_board(s) {
        let role = row.role;
        u.row(8.0, |r| {
            r.icon(job_icon(role), 20.0, MUTED).tip(Tip::name(role.name()));
            r.node(Node { flex_direction: FlexDirection::Row, flex_wrap: FlexWrap::Wrap, column_gap: Val::Px(6.0), row_gap: Val::Px(4.0), width: Val::Px(220.0), align_items: AlignItems::Center, ..default() }, |t| {
                for i in 0..row.slots.min(12) {
                    match row.beavers.get(i).and_then(|id| s.colony.beaver(*id)) {
                        Some(b) => {
                            t.token(Icon::Beaver, 18.0, None).action(HudAction::GoTo(NoticeAt::Beaver(b.id))).peek(b.id).tip(Tip::titled(Some((stage_icon(b.stage), INK)), b.name.clone()));
                        }
                        None if i < row.filled => {
                            t.token(Icon::Beaver, 18.0, None);
                        }
                        None => {
                            t.empty_token(18.0);
                        }
                    }
                }
                if row.filled > 12 {
                    t.bold(format!("+{}", row.filled - 12), 11.0, MUTED);
                }
            });
            r.spacer();
            if role.row().shore_limit {
                // Each click tightens the limit a step, then lifts it.
                let now = s.work.shore_limits.get(&role).copied();
                let next = match now {
                    None => Some(SHORE_STEPS[0]),
                    Some(n) => SHORE_STEPS.iter().copied().find(|&k| k < n),
                };
                r.pebble(Icon::Drop, 11.0, HudAction::Cmd(sim::Command::SetShoreLimit { role, cells: next }), if now.is_some() { Peb::On } else { Peb::Idle })
                    .tip(Tip::name("Distance from water").with_value(now.map_or("Any".into(), |n| format!("{n}"))));
                if let Some(n) = now {
                    r.bold(format!("{n}"), 12.0, INK);
                }
            }
            let (add_look, add_tip) = match sim::jobs::role_add_check(s, role) {
                Ok(p) => (Peb::Idle, Tip::name("Add one").line(s.colony.beaver(p.who).map(|b| b.name.clone()).unwrap_or_default(), "free")),
                Err(why) => (Peb::Faded, Tip::name("Can't add").line(why, "")),
            };
            r.pebble(Icon::Minus, 10.0, HudAction::Cmd(sim::Command::AdjustRole { role, delta: -1 }), if row.filled > 0 { Peb::Idle } else { Peb::Faded }).tip(Tip::name("Free one"));
            r.pebble(Icon::Plus, 10.0, HudAction::Cmd(sim::Command::AdjustRole { role, delta: 1 }), add_look).tip(add_tip);
        });
    }
}

/// Each store as ticks of its capacity, with the last sixty days beside it.
pub(super) fn stores(c: &Ctx, u: &mut Ui) {
    let s = c.s;
    let st = &s.buildings.stores;
    for g in Good::all() {
        let (have, cap) = (st.amount(g), st.capacity(g));
        u.row(10.0, |r| {
            r.icon(good_icon(g), 20.0, good_color(g));
            r.ticks(if cap > 0.0 { have / cap } else { 0.0 }, 10, 150.0, 12.0, ACCENT);
            r.bold(format!("{}", have.round() as i64), 14.0, INK);
            r.spacer();
            let hist: Vec<f32> = s.ledger.colony.iter().rev().take(60).rev().map(|d| d.stock[g]).collect();
            let top = hist.iter().copied().fold(1.0f32, f32::max);
            r.node(Node { height: Val::Px(20.0), align_items: AlignItems::FlexEnd, ..row_node(1.0) }, |sp| {
                for v in &hist {
                    sp.add(VNode::new(Node { width: Val::Px(1.5), height: Val::Px((v / top * 20.0).max(1.0)), ..default() })).bg(good_color(g).with_alpha(0.7));
                }
            });
        })
        .tip(good_tip(s, g));
    }
}
