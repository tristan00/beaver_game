//! The panel a dam opens: picture, health, pond, a cross-section of every
//! cell, growth with stick costs, and upkeep. Words live in tooltips.
use super::panels::{Ctx, RIGHT_W};
use super::ui::*;
use super::words::*;
use super::{DamTab, HudAction};
use crate::flow::Selection;
use crate::icons::Icon;
use bevy::prelude::*;
use sim::buildings::{BuildingId, Good};
use sim::dams::{Dam, Spill};
use sim::DamGrowth;

const DEEP: Color = Color::srgb(0.12, 0.54, 0.72);
const SHALLOW: Color = Color::srgb(0.44, 0.83, 0.88);
const MUD: Color = Color::srgb(0.56, 0.43, 0.32);
const WOOD: Color = Color::srgb(0.95, 0.85, 0.66);
const BACKDROP: Color = Color::srgb(0.165, 0.141, 0.125);
const INNER: f32 = RIGHT_W - 20.0;
const SECTION_H: f32 = 64.0;

fn abs_box(left: f32, top: f32, w: f32, h: f32) -> Node {
    Node { position_type: PositionType::Absolute, left: Val::Px(left), top: Val::Px(top), width: Val::Px(w), height: Val::Px(h), ..default() }
}

/// A small painted view of the dam: deep water behind, a stick wall, the stream below.
fn portrait(u: &mut Ui, health: f32) {
    u.node(Node { width: Val::Px(58.0), height: Val::Px(48.0), border_radius: BorderRadius::all(Val::Px(4.0)), overflow: Overflow::clip(), flex_shrink: 0.0, ..default() }, |p| {
        p.add(VNode::new(abs_box(0.0, 14.0, 26.0, 34.0))).bg(DEEP);
        p.add(VNode::new(abs_box(36.0, 36.0, 22.0, 12.0))).bg(SHALLOW.with_alpha(0.8));
        let mound = p.add(VNode::new(Node { border_radius: BorderRadius::top(Val::Px(5.0)), ..abs_box(22.0, 10.0, 16.0, 38.0) }));
        mound.bg = if health < 0.5 { Color::srgb(0.66, 0.4, 0.33) } else { MUD };
        for (y, x) in [(16.0, 23.0), (26.0, 22.0), (36.0, 23.0)] {
            p.add(VNode::new(abs_box(x, y, 14.0, 2.0))).bg(WOOD.with_alpha(0.6));
        }
    })
    .bg(BACKDROP)
    .border(BorderColor::all(Color::srgba(1.0, 1.0, 1.0, 0.15)))
    .style(|n| n.border = UiRect::all(Val::Px(1.0)));
}

fn health_colour(h: f32) -> Color {
    if h < 0.5 { BAD } else if h < sim::dams::REPAIR_BELOW { WARN } else { GOOD }
}

fn days(d: f32) -> String {
    if !d.is_finite() { "∞".into() } else { format!("{}d", d.ceil().max(1.0) as i64) }
}

fn header(c: &Ctx, u: &mut Ui, id: BuildingId, d: &Dam) {
    let s = c.s;
    let health = sim::dams::health(s, d);
    let sel = Selection::Site(id);
    let pinned = c.hud.pinned.contains(&sel);
    u.row(10.0, |r| {
        portrait(r, health);
        r.col(6.0, |x| {
            x.bold("Dam", 15.0, INK);
            x.row(6.0, |y| {
                y.bar(health, 112.0, 6.0, health_colour(health));
                y.text(format!("{:.0}%", health * 100.0), 11.5, MUTED);
            })
            .tip(Tip::name("Condition").line("Worn cells", format!("{}", d.cells.iter().filter(|c| sim::dams::built(s, c.cell) > 0.0 && sim::dams::integrity(s, c.cell) < sim::dams::REPAIR_BELOW).count())));
        });
        r.spacer();
        r.col(4.0, |x| {
            x.row(4.0, |y| {
                y.icon_button(Icon::Pin, 10.0, HudAction::Pin(sel), FAINT, pinned).tip(Tip::name(if pinned { "Unpin" } else { "Pin" }));
                y.icon_button(Icon::Cross, 10.0, if pinned { HudAction::Pin(sel) } else { HudAction::CloseInspector }, FAINT, false).tip(Tip::name("Close").key("Esc"));
            });
        });
    });
}

fn tabs(c: &Ctx, u: &mut Ui) {
    u.row(14.0, |r| {
        for (tab, icon, name) in [(DamTab::Status, Icon::Drop, "Status"), (DamTab::History, Icon::Hourglass, "History")] {
            let on = c.hud.dam_tab == tab;
            r.col(3.0, |x| {
                x.icon(icon, 15.0, if on { INK } else { MUTED });
                x.add(VNode::new(Node { width: Val::Px(26.0), height: Val::Px(2.0), ..default() })).bg(if on { ACCENT } else { Color::NONE });
            })
            .style(|n| n.align_items = AlignItems::Center)
            .action(HudAction::DamTab(tab))
            .tip(Tip::name(name));
        }
    });
    u.rule();
}

fn pond_rows(c: &Ctx, u: &mut Ui, d: &Dam) {
    let Some(p) = d.pond.as_ref() else {
        u.row(6.0, |r| {
            r.icon(Icon::Gauge, 15.0, WATER);
            r.bar(0.0, 190.0, 12.0, WATER);
            r.spacer();
            r.icon(Icon::Hourglass, 13.0, MUTED);
        })
        .tip(Tip::name("No pond yet").line("The first finished cells start holding water", ""));
        return;
    };
    let span = (p.full_level - p.bottom).max(0.01);
    let frac = ((p.level - p.bottom) / span).clamp(0.0, 1.0);
    let trend = d.history.iter().rev().take(2).map(|h| h.level).collect::<Vec<_>>();
    let rising = trend.len() < 2 || trend[0] >= trend[1] - 1e-3;
    u.row(6.0, |r| {
        r.icon(Icon::Gauge, 15.0, WATER);
        r.node(Node { flex_direction: FlexDirection::Row, align_items: AlignItems::Center, ..default() }, |b| {
            b.bar(frac, 186.0, 12.0, WATER);
            b.add(VNode::new(Node { width: Val::Px(2.0), height: Val::Px(18.0), ..default() })).bg(WOOD);
        });
        r.spacer();
        if p.days_to_full <= 0.0 {
            r.icon(Icon::Check, 13.0, GOOD);
        } else {
            r.icon(if rising { Icon::Up } else { Icon::Down }, 12.0, if rising { GOOD } else { WARN });
            r.bold(days(p.days_to_full), 13.0, if rising { GOOD } else { WARN });
        }
    })
    .tip(
        Tip::name("Pond")
            .line("Level", num(p.level - p.bottom))
            .line("Full at", num(p.full_level - p.bottom))
            .line(if p.days_to_full <= 0.0 { "Full" } else { "Full in" }, if p.days_to_full <= 0.0 { String::new() } else { days(p.days_to_full) }),
    );
    let _ = c;
    u.row(14.0, |r| {
        let stat = |r: &mut Ui, icon: Icon, col: Color, v: String, name: &str, unit: &str| {
            r.row(4.0, |x| {
                x.icon(icon, 14.0, col);
                x.bold(v, 13.0, INK);
            })
            .tip(Tip::name(name).line(unit, ""));
        };
        stat(r, Icon::Upstream, WATER, format!("{}/d", num(p.inflow)), "Water in", "per day");
        stat(r, Icon::Push, WATER, format!("{}/d", num(p.outflow)), "Water out", "spill, seepage and evaporation per day");
        stat(r, Icon::Area, MUTED, format!("{}", p.area), "Pond area", "cells under water");
        stat(r, Icon::Drop, WATER, format!("{} m", num(p.max_depth)), "Deepest", "metres");
    });
}

/// Every cell along the dam, drawn as a stick wall against the water behind it.
fn cross_section(c: &Ctx, u: &mut Ui, id: BuildingId, d: &Dam) {
    let s = c.s;
    let line: Vec<&sim::dams::DamCell> = d.line().collect();
    if line.is_empty() {
        return;
    }
    let n = line.len() as f32;
    let step = (INNER / n).clamp(4.0, 18.0);
    let bar_w = (step - if step > 6.0 { 2.0 } else { 1.0 }).max(2.0);
    let top_h = d.cells.iter().map(|c| c.target.max(sim::dams::built(s, c.cell))).fold(d.cap(), f32::max).max(0.5);
    let px = (SECTION_H - 14.0) / top_h;
    let base = line.iter().map(|c| s.height.get(c.cell.0, c.cell.1).copied().unwrap_or(0.0)).fold(f32::INFINITY, f32::min);
    let water = d.pond.as_ref().map_or(0.0, |p| (p.level - base).max(0.0));
    let working: Vec<_> = s.agents.by_id.values().filter_map(|a| match a.task {
        sim::agents::Task::DamWork { dam, cell, .. } if dam == id => Some(cell),
        _ => None,
    }).collect();
    let width = step * n;
    u.node(Node { width: Val::Px(INNER), height: Val::Px(SECTION_H + 6.0), border_radius: BorderRadius::all(Val::Px(4.0)), overflow: Overflow::clip(), justify_content: JustifyContent::Center, ..default() }, |w| {
        w.add(VNode::new(abs_box(0.0, SECTION_H + 4.0 - (water * px).min(SECTION_H), INNER, (water * px).min(SECTION_H)))).bg(DEEP.with_alpha(0.55));
        w.node(Node { flex_direction: FlexDirection::Row, align_items: AlignItems::FlexEnd, width: Val::Px(width), height: Val::Px(SECTION_H + 4.0), ..default() }, |bars| {
            for cell in &line {
                let built = sim::dams::built(s, cell.cell);
                let worn = built > 0.0 && sim::dams::integrity(s, cell.cell) < sim::dams::REPAIR_BELOW;
                let picked = c.hud.dam_cells.contains(&cell.cell);
                let busy = working.contains(&cell.cell);
                let col = bars.node(Node { width: Val::Px(step), height: Val::Px(SECTION_H + 4.0), flex_direction: FlexDirection::Column, justify_content: JustifyContent::FlexEnd, align_items: AlignItems::Center, ..default() }, |k| {
                    if busy && step >= 8.0 {
                        k.icon(Icon::Beaver, (step + 2.0).min(12.0), Color::srgb(0.73, 0.47, 0.25));
                    }
                    let plan = (cell.target - built).max(0.0) * px;
                    if plan > 0.5 {
                        k.add(VNode::new(Node { width: Val::Px(bar_w), height: Val::Px(plan), border: UiRect::all(Val::Px(1.0)), border_radius: BorderRadius::top(Val::Px(2.0)), ..default() }))
                            .border(BorderColor::all(WOOD.with_alpha(0.7)));
                    }
                    let h = (built * px).max(if cell.target > 0.0 { 1.0 } else { 0.0 });
                    let wall = k.add(VNode::new(Node { width: Val::Px(bar_w), height: Val::Px(h), border: UiRect::all(Val::Px(if worn || picked { 1.5 } else { 0.0 })), border_radius: if plan > 0.5 { BorderRadius::ZERO } else { BorderRadius::top(Val::Px(2.0)) }, ..default() }));
                    wall.bg = MUD;
                    if worn || picked {
                        wall.border = Some(BorderColor::all(if picked { SELECT } else { BAD }));
                    }
                });
                let mut tip = Tip::name("Cell").line("Height", format!("{} / {}", num(built), num(cell.target)));
                if built > 0.0 {
                    tip = tip.line("Condition", format!("{:.0}%", sim::dams::integrity(s, cell.cell) * 100.0));
                }
                col.action(HudAction::DamCell(cell.cell)).tip(tip.line("Click", "pick for Raise").line("Shift", "add"));
            }
        });
    })
    .bg(Color::srgba(1.0, 1.0, 1.0, 0.04));
}

fn grow_button(u: &mut Ui, icon: Icon, name: &str, id: BuildingId, growth: DamGrowth, plan: Result<f32, String>, suggested: bool) {
    let ok = plan.is_ok();
    let v = u.node(Node { width: Val::Px(62.0), height: Val::Px(46.0), flex_direction: FlexDirection::Column, align_items: AlignItems::Center, justify_content: JustifyContent::Center, row_gap: Val::Px(3.0), border: UiRect::all(Val::Px(if suggested { 1.5 } else { 1.0 })), border_radius: BorderRadius::all(Val::Px(4.0)), ..default() }, |b| {
        b.icon(icon, 16.0, if ok { INK } else { FAINT.with_alpha(0.5) });
        b.row(3.0, |x| {
            x.icon(good_icon(Good::Sticks), 12.0, if ok { WOOD } else { MUTED });
            x.bold(plan.as_ref().map(|c| format!("{}", c.round() as i64)).unwrap_or_else(|_| "-".into()), 11.5, if ok { INK } else { MUTED });
        });
    });
    v.bg = Color::srgba(1.0, 1.0, 1.0, if ok { 0.07 } else { 0.03 });
    v.border = Some(BorderColor::all(if suggested { ACCENT } else { LINE }));
    let tip = match &plan {
        Ok(c) => Tip::name(name).line("Sticks", num(*c)),
        Err(e) => Tip::name(name).line(e.clone(), ""),
    };
    v.tip(tip);
    if ok {
        v.action(HudAction::GrowDam(id, growth));
    }
}

fn actions(c: &Ctx, u: &mut Ui, id: BuildingId, d: &Dam) {
    let s = c.s;
    let cost = |plan: Result<sim::dams::Plan, String>| plan.map(|p| sim::dams::plan_cost(s, d, &p));
    let raise = cost(sim::dams::plan_raise(s, id, &c.hud.dam_cells));
    let widen = cost(sim::dams::plan_widen(s, id));
    let spill = d.pond.as_ref().map_or(Spill::None, |p| p.spill);
    let extend = if matches!(spill, Spill::Around(_)) { cost(sim::dams::plan_extend(s, id)) } else { Err("water is not getting around the ends".into()) };
    let paused = s.work.place(id).is_some_and(|p| p.paused);
    u.row(4.0, |r| {
        grow_button(r, Icon::Rise, if c.hud.dam_cells.is_empty() { "Raise" } else { "Raise picked cells" }, id, DamGrowth::Raise, raise.clone(), matches!(spill, Spill::Over(_)) && raise.is_ok());
        grow_button(r, Icon::Widen, "Widen", id, DamGrowth::Widen, widen, matches!(spill, Spill::Over(_)) && raise.is_err());
        grow_button(r, Icon::Extend, "Extend", id, DamGrowth::Extend, extend.clone(), extend.is_ok());
        r.spacer();
        let on = d.maintain;
        let m = r.node(Node { width: Val::Px(30.0), height: Val::Px(46.0), align_items: AlignItems::Center, justify_content: JustifyContent::Center, border_radius: BorderRadius::all(Val::Px(4.0)), ..default() }, |b| {
            b.icon(Icon::Hammer, 16.0, if on { GOOD } else { MUTED });
        });
        m.bg = if on { GOOD.with_alpha(0.18) } else { Color::srgba(1.0, 1.0, 1.0, 0.07) };
        m.action(HudAction::DamMaintain(id, !on)).tip(Tip::name("Maintain").line(if on { "On" } else { "Off" }, "").line("Builders repair worn cells", ""));
        let p = r.node(Node { width: Val::Px(30.0), height: Val::Px(46.0), align_items: AlignItems::Center, justify_content: JustifyContent::Center, border_radius: BorderRadius::all(Val::Px(4.0)), ..default() }, |b| {
            b.icon(if paused { Icon::Play } else { Icon::Pause }, 15.0, if paused { WARN } else { MUTED });
        });
        p.bg = if paused { WARN.with_alpha(0.18) } else { Color::srgba(1.0, 1.0, 1.0, 0.07) };
        p.action(HudAction::Paused(id, !paused)).tip(Tip::name(if paused { "Resume" } else { "Pause" }));
    });
}

fn footer(c: &Ctx, u: &mut Ui, id: BuildingId, d: &Dam) {
    let s = c.s;
    let (builders, loads) = sim::dams::crew(s, id);
    let left = sim::dams::loads_left(s, id);
    u.rule();
    u.row(14.0, |r| {
        r.row(4.0, |x| {
            x.icon(Icon::Beaver, 14.0, Color::srgb(0.73, 0.47, 0.25));
            x.bold(format!("{builders}"), 13.0, INK);
        })
        .tip(Tip::name("Builders"));
        r.row(4.0, |x| {
            x.icon(good_icon(Good::Sticks), 14.0, WOOD);
            x.bold(format!("{loads}"), 13.0, INK);
            x.icon(Icon::Expand, 9.0, MUTED);
        })
        .tip(Tip::name("Loads on the way"));
        r.spacer();
        if left > 0 {
            r.row(4.0, |x| {
                x.icon(Icon::Site, 13.0, MUTED);
                x.bold(format!("{left}"), 12.0, MUTED);
            })
            .tip(Tip::name("Loads still to carry").line("Sticks", num(left as f32 * sim::dams::sticks_per_step())));
        }
        r.row(3.0, |x| {
            for _ in 0..d.rows {
                x.add(VNode::new(Node { width: Val::Px(4.0), height: Val::Px(12.0), border_radius: BorderRadius::all(Val::Px(1.0)), ..default() })).bg(MUD);
            }
        })
        .tip(Tip::name("Thickness").line("Rows", format!("{}", d.rows)).line("Height limit", num(d.cap())));
    });
}

fn history(u: &mut Ui, d: &Dam) {
    if d.history.is_empty() {
        u.row(6.0, |r| {
            r.icon(Icon::Hourglass, 14.0, MUTED);
        })
        .tip(Tip::name("No days recorded yet"));
        return;
    }
    let low = d.history.iter().map(|h| h.level).fold(f32::INFINITY, f32::min);
    let high = d.history.iter().map(|h| h.level).fold(f32::NEG_INFINITY, f32::max).max(low + 0.05);
    let flow_top = d.history.iter().map(|h| h.inflow.max(h.outflow)).fold(0.0, f32::max).max(1e-3);
    let w = (INNER / sim::dams::HISTORY_DAYS as f32).floor().max(2.0);
    let chart = |u: &mut Ui, icon: Icon, col: Color, name: &str, value: &dyn Fn(&sim::dams::DamDay) -> f32| {
        u.row(6.0, |r| {
            r.icon(icon, 13.0, col);
            r.node(Node { flex_direction: FlexDirection::Row, align_items: AlignItems::FlexEnd, height: Val::Px(36.0), column_gap: Val::Px(1.0), ..default() }, |b| {
                for day in &d.history {
                    let f = value(day).clamp(0.0, 1.0);
                    b.add(VNode::new(Node { width: Val::Px(w - 1.0), height: Val::Px((f * 34.0).max(1.0)), ..default() })).bg(col);
                }
            });
        })
        .tip(Tip::name(name).line("Days", format!("{}", d.history.len())));
    };
    chart(u, Icon::Gauge, WATER, "Pond level", &|h| (h.level - low) / (high - low));
    chart(u, Icon::Upstream, SHALLOW, "Water in", &|h| h.inflow / flow_top);
    chart(u, Icon::Push, WARN, "Water out", &|h| h.outflow / flow_top);
}

pub fn dam_panel(c: &Ctx, id: BuildingId, d: &Dam) -> VNode {
    let mut ui = Ui::default();
    ui.node(Node { width: Val::Px(RIGHT_W), flex_direction: FlexDirection::Column, row_gap: Val::Px(8.0), ..default() }, |u| {
        header(c, u, id, d);
        tabs(c, u);
        match c.hud.dam_tab {
            DamTab::Status => {
                pond_rows(c, u, d);
                cross_section(c, u, id, d);
                actions(c, u, id, d);
            }
            DamTab::History => history(u, d),
        }
        footer(c, u, id, d);
    });
    let mut v = ui.nodes.pop().expect("panel node");
    panel(&mut v);
    v
}
