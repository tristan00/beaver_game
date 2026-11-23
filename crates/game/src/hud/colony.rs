//! The colony book: a drawer beside the map that never covers the pond.
//! Beavers are tokens, amounts are notches, and words live in tooltips.
use super::panels::{good_tip, stage_icon, Ctx};
use super::ui::*;
use super::words::*;
use super::{ColonyTab, HudAction};
use crate::icons::Icon;
use bevy::prelude::*;
use sim::buildings::{BuildingKind, Good};

pub const DRAWER_W: f32 = 380.0;

const TABS: [(ColonyTab, Icon, &str); 4] = [
    (ColonyTab::Population, Icon::Beaver, "Beavers"),
    (ColonyTab::Jobs, Icon::Hammer, "Work"),
    (ColonyTab::Water, Icon::Drop, "Water"),
    (ColonyTab::Goods, Icon::Warehouse, "Stores"),
];

pub fn screen(c: &Ctx) -> Vec<VNode> {
    let Some(tab) = c.hud.colony else { return Vec::new() };
    let mut ui = Ui::default();
    ui.node(
        Node { position_type: PositionType::Absolute, left: Val::Px(76.0), width: Val::Px(DRAWER_W), top: Val::Px(84.0), max_height: Val::Percent(80.0), flex_direction: FlexDirection::Column, row_gap: Val::Px(10.0), overflow: Overflow::clip(), ..default() },
        |u| {
            u.row(6.0, |r| {
                for (t, icon, name) in TABS {
                    let on = t == tab;
                    r.pebble(icon, 16.0, HudAction::Colony(Some(t)), if on { Peb::On } else { Peb::Idle }).tip(Tip::name(name));
                }
                r.spacer();
                r.pebble(Icon::Cross, 10.0, HudAction::Colony(None), Peb::Bare).tip(Tip::name("Close").key("Tab"));
            });
            u.rule();
            match tab {
                ColonyTab::Population => beavers(c, u),
                ColonyTab::Jobs => work(c, u),
                ColonyTab::Water => water(c, u),
                ColonyTab::Goods => stores(c, u),
            }
        },
    );
    let mut v = ui.nodes.pop().unwrap();
    panel(&mut v);
    v.node.padding = UiRect::all(Val::Px(14.0));
    vec![v]
}

/// The pip under a beaver token: green working, amber free, blue resting or too young.
fn doing(s: &sim::SimState, b: &sim::colony::Beaver) -> Color {
    if !b.can_work() || b.resting {
        REST
    } else if s.work.slot_of(b.id).is_some() || s.agents.get(b.id).is_some_and(|a| a.assignment.is_some()) {
        GOOD
    } else {
        ACCENT
    }
}

fn token_px(st: sim::colony::Stage) -> f32 {
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
                    .line("Age", format!("{:.1}", b.age_years()))
                    .line("Health", format!("{:.0}", b.health * 100.0))
                    .line("Mood", format!("{:.0}", b.mood * 100.0));
                r.token(Icon::Beaver, token_px(b.stage), Some(doing(s, b))).action(HudAction::SelectBeaver(b.id)).tip(t);
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

/// Who does what: one row per kind of work, its beavers as tokens and open places as rings.
fn work(c: &Ctx, u: &mut Ui) {
    let s = c.s;
    for row in sim::jobs::jobs_board(s) {
        let role = row.role;
        u.row(8.0, |r| {
            r.icon(role_kind_icon(role), 20.0, MUTED).tip(Tip::name(role_kind_name(role)));
            r.node(Node { flex_direction: FlexDirection::Row, flex_wrap: FlexWrap::Wrap, column_gap: Val::Px(6.0), row_gap: Val::Px(4.0), width: Val::Px(220.0), align_items: AlignItems::Center, ..default() }, |t| {
                for i in 0..row.slots.min(12) {
                    if i < row.filled {
                        t.token(Icon::Beaver, 18.0, None);
                    } else {
                        t.empty_token(18.0);
                    }
                }
            });
            r.spacer();
            let add = match sim::jobs::role_add_preview(s, role) {
                Some(p) => Tip::name("Add one").line(s.colony.beaver(p.who).map(|b| b.name.clone()).unwrap_or_default(), "free"),
                None => Tip::name("No one to add"),
            };
            r.pebble(Icon::Minus, 10.0, HudAction::RoleAdjust(role, -1), Peb::Idle).tip(Tip::name("Free one"));
            r.pebble(Icon::Plus, 10.0, HudAction::RoleAdjust(role, 1), Peb::Idle).tip(add);
        });
    }
}

/// Every dam: its condition, how full its pond is, and work still to do.
fn water(c: &Ctx, u: &mut Ui) {
    let s = c.s;
    let mut any = false;
    for b in s.buildings.buildings.iter().filter(|b| b.kind == BuildingKind::Dam) {
        let Some(d) = b.dam.as_deref() else { continue };
        any = true;
        let health = sim::dams::health(s, d);
        let fill = d.pond.as_ref().map_or(0.0, |p| ((p.level - p.bottom) / (p.full_level - p.bottom).max(0.01)).clamp(0.0, 1.0));
        u.row(10.0, |r| {
            r.icon(Icon::Dam, 20.0, MUD);
            r.pips(health, if health < 0.5 { BAD } else if health < sim::dams::REPAIR_BELOW { WARN } else { GOOD }).tip(Tip::name("Condition").with_value(format!("{:.0}%", health * 100.0)));
            r.notches(fill, 8, 110.0, 12.0, WATER).tip(Tip::name("Pond").with_value(format!("{:.0}%", fill * 100.0)));
            r.spacer();
            let left = sim::dams::loads_left(s, b.id);
            if left > 0 {
                r.row(3.0, |x| {
                    x.icon(good_icon(Good::Sticks), 14.0, good_color(Good::Sticks));
                    x.bold(format!("{left}"), 13.0, INK);
                })
                .tip(Tip::name("Loads still to carry"));
            }
            r.pebble(Icon::Focus, 11.0, HudAction::SelectBuilding(b.id), Peb::Idle).tip(Tip::name("Open"));
        });
    }
    if !any {
        u.row(6.0, |r| {
            r.icon(Icon::Dam, 20.0, MUTED);
            r.bold("0", 14.0, MUTED);
        })
        .tip(Tip::name("No dams yet").line("Z, then draw across the water", ""));
    }
}

/// Each store as notches of its capacity, with the last sixty days beside it.
fn stores(c: &Ctx, u: &mut Ui) {
    let s = c.s;
    let st = &s.buildings.stores;
    for g in Good::ALL {
        let (have, cap) = (st.amount(g), st.capacity(g));
        u.row(10.0, |r| {
            r.icon(good_icon(g), 20.0, good_color(g));
            r.notches(if cap > 0.0 { have / cap } else { 0.0 }, 10, 150.0, 12.0, ACCENT);
            r.bold(format!("{}", have.round() as i64), 14.0, INK);
            r.spacer();
            let hist: Vec<f32> = s.ledger.days.iter().rev().take(60).rev().map(|d| d.stock[g.index()]).collect();
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
