//! The panel a canal opens: picture, condition, water, a long profile of every cell from the
//! intake, the gate, growth with labour costs, and upkeep. Words live in tooltips.
use super::panels::{Ctx, RIGHT_W};
use super::ui::*;
use super::words::*;
use super::{DamTab, HudAction};
use crate::flow::Selection;
use crate::icons::Icon;
use bevy::prelude::*;
use sim::buildings::{BuildingId, Good};
use sim::canals::{Canal, CanalGrowth};
use sim::GateSetting;

const DEEP: Color = Color::srgb(0.12, 0.54, 0.72);
const SHALLOW: Color = Color::srgb(0.44, 0.83, 0.88);
const WOOD: Color = Color::srgb(0.95, 0.85, 0.66);
const GROUND: Color = Color::srgb(0.35, 0.29, 0.22);
const DIGGER: Color = Color::srgb(0.73, 0.47, 0.25);
const INNER: f32 = RIGHT_W - 20.0;
const PROFILE_H: f32 = 64.0;

fn abs_box(left: f32, top: f32, w: f32, h: f32) -> Node {
    Node { position_type: PositionType::Absolute, left: Val::Px(left), top: Val::Px(top), width: Val::Px(w), height: Val::Px(h), ..default() }
}

/// A small painted view: a channel of water between two muddy banks.
fn portrait(u: &mut Ui, dry: bool) {
    u.node(Node { width: Val::Px(52.0), height: Val::Px(52.0), padding: UiRect::all(Val::Px(5.0)), flex_shrink: 0.0, ..default() }, |frame| {
        frame.node(Node { width: Val::Px(42.0), height: Val::Px(42.0), overflow: Overflow::clip(), ..default() }, |p| {
            p.add(VNode::new(abs_box(0.0, 0.0, 42.0, 42.0))).bg(GROUND);
            p.add(VNode::new(abs_box(0.0, 14.0, 42.0, 14.0))).bg(if dry { MUD } else { DEEP });
            p.add(VNode::new(abs_box(0.0, 12.0, 42.0, 2.0))).bg(MUD);
            p.add(VNode::new(abs_box(0.0, 28.0, 42.0, 2.0))).bg(MUD);
            if !dry {
                for x in [6.0, 20.0, 32.0] {
                    p.add(VNode::new(abs_box(x, 20.0, 6.0, 1.5))).bg(SHALLOW.with_alpha(0.8));
                }
            }
        });
    })
    .skin(Skin::Portrait);
}

fn health_colour(h: f32) -> Color {
    if h < 0.5 { BAD } else if h < 0.8 { WARN } else { GOOD }
}

fn header(c: &Ctx, u: &mut Ui, id: BuildingId, k: &Canal) {
    let s = c.s;
    let health = sim::canals::health(s, k);
    let sel = Selection::Site(id);
    let pinned = c.hud.pinned.contains(&sel);
    let dry = k.status.as_ref().is_none_or(|st| st.wet == 0);
    u.row(10.0, |r| {
        portrait(r, dry);
        r.col(7.0, |x| {
            x.bold("Canal", 16.0, INK);
            let silted = k.status.as_ref().map_or(0, |st| st.silted);
            x.pips(health, health_colour(health)).tip(Tip::name("Condition").line("Silted cells", format!("{silted}")).with_value(format!("{:.0}%", health * 100.0)));
        });
        r.spacer();
        r.row(4.0, |y| {
            y.pebble(Icon::Pin, 10.0, HudAction::Pin(sel), if pinned { Peb::On } else { Peb::Bare }).tip(Tip::name(if pinned { "Unpin" } else { "Pin" }));
            y.pebble(Icon::Cross, 10.0, if pinned { HudAction::Pin(sel) } else { HudAction::CloseInspector }, Peb::Bare).tip(Tip::name("Close").key("Esc"));
        })
        .style(|n| n.align_self = AlignSelf::FlexStart);
    });
}

fn tabs(c: &Ctx, u: &mut Ui) {
    u.row(6.0, |r| {
        for (tab, icon, name) in [(DamTab::Status, Icon::Drop, "Status"), (DamTab::History, Icon::Hourglass, "History")] {
            let on = c.hud.dam_tab == tab;
            r.pebble(icon, 13.0, HudAction::DamTab(tab), if on { Peb::On } else { Peb::Idle }).tip(Tip::name(name));
        }
    });
    u.rule();
}

/// Twelve notches: floatable (blue), only wet (pale), dry (empty).
fn notches3(u: &mut Ui, floatable: f32, wet: f32) {
    u.row(2.0, |r| {
        for i in 0..12 {
            let f = (i as f32 + 0.5) / 12.0;
            let col = if f <= floatable { WATER } else if f <= wet { SHALLOW.with_alpha(0.8) } else { Color::srgba(1.0, 1.0, 1.0, 0.07) };
            r.add(VNode::new(Node { width: Val::Px(13.0), height: Val::Px(12.0), border_radius: BorderRadius::all(Val::Px(2.0)), ..default() })).bg(col);
        }
    });
}

fn water_rows(u: &mut Ui, k: &Canal) {
    let Some(st) = k.status.as_ref() else {
        u.row(6.0, |r| {
            r.icon(Icon::Logs, 15.0, WATER);
            notches3(r, 0.0, 0.0);
            r.spacer();
            r.icon(Icon::Hourglass, 13.0, MUTED);
        })
        .tip(Tip::name("No water yet").line("Fills once the intake is dug", ""));
        return;
    };
    let n = st.cells.max(1) as f32;
    let share = st.floatable_share();
    u.row(6.0, |r| {
        r.icon(Icon::Logs, 15.0, WATER);
        notches3(r, share, st.wet as f32 / n);
        r.spacer();
        r.bold(format!("{:.0}%", share * 100.0), 13.0, if share > 0.5 { GOOD } else if share > 0.0 { WARN } else { MUTED });
    })
    .tip(Tip::name("Floats a load").line("Cells deep enough", format!("{} / {}", st.floatable, st.cells)).line("Wet", format!("{}", st.wet)).line("Dug", format!("{}", st.dug)));
    u.row(14.0, |r| {
        let stat = |r: &mut Ui, icon: Icon, col: Color, v: String, name: &str, unit: &str| {
            r.row(4.0, |x| {
                x.icon(icon, 14.0, col);
                x.bold(v, 13.0, INK);
            })
            .tip(Tip::name(name).line(unit, ""));
        };
        // Some engines route a creek cell to cell without face flows; then there is no reading.
        if st.inflow > 0.0 || st.outflow > 0.0 {
            stat(r, Icon::Upstream, WATER, format!("{}/d", num(st.inflow)), "Water in", "at the gate, per day");
            stat(r, Icon::Push, WATER, format!("{}/d", num(st.outflow)), "Water out", "at the far end, per day");
        }
        stat(r, Icon::Extend, MUTED, format!("{}", k.route().count()), "Length", "cells");
        stat(r, Icon::Drop, WATER, format!("{} m", num(st.mean_depth)), "Depth", "mean of wet cells");
    });
}

/// Every cell along the route from the intake: ground, target bed, dug bed and water.
fn profile(c: &Ctx, u: &mut Ui, id: BuildingId, k: &Canal) {
    let s = c.s;
    let route: Vec<&sim::canals::CanalCell> = k.route().collect();
    if route.is_empty() {
        return;
    }
    let n = route.len() as f32;
    let step = (INNER / n).clamp(3.0, 16.0);
    let bar_w = (step - if step > 6.0 { 2.0 } else { 1.0 }).max(2.0);
    let ground = |cell: (usize, usize)| s.height.get(cell.0, cell.1).copied().unwrap_or(0.0);
    let depth = |cell: (usize, usize)| s.water.depth.get(cell.0, cell.1).copied().unwrap_or(0.0);
    let low = route.iter().map(|c| c.target.min(ground(c.cell))).fold(f32::INFINITY, f32::min);
    let high = route.iter().map(|c| c.ground.max(ground(c.cell) + depth(c.cell))).fold(f32::NEG_INFINITY, f32::max).max(low + 0.5);
    let px = (PROFILE_H - 6.0) / (high - low);
    let working: Vec<_> = s.agents.by_id.values().filter_map(|a| match a.task {
        sim::agents::Task::Dig { site, cell, .. } | sim::agents::Task::Gate { site, cell, .. } if site == id => Some(cell),
        _ => None,
    }).collect();
    u.node(Node { width: Val::Px(INNER), height: Val::Px(PROFILE_H + 6.0), border_radius: BorderRadius::all(Val::Px(4.0)), overflow: Overflow::clip(), justify_content: JustifyContent::Center, ..default() }, |w| {
        w.node(Node { flex_direction: FlexDirection::Row, align_items: AlignItems::FlexEnd, width: Val::Px(step * n), height: Val::Px(PROFILE_H + 4.0), ..default() }, |bars| {
            for cell in &route {
                let g = ground(cell.cell);
                let d = depth(cell.cell);
                let silted = cell.done && g > cell.target + sim::canals::SILT_TOLERANCE;
                let picked = c.hud.dam_cells.contains(&cell.cell);
                let busy = working.contains(&cell.cell);
                let gate = cell.cell == k.gate.cell;
                let col = bars.node(Node { width: Val::Px(step), height: Val::Px(PROFILE_H + 4.0), flex_direction: FlexDirection::Column, justify_content: JustifyContent::FlexEnd, align_items: AlignItems::Center, ..default() }, |b| {
                    if busy && step >= 7.0 {
                        b.icon(Icon::Beaver, (step + 2.0).min(12.0), DIGGER);
                    }
                    // Undug ground above the bed, water over the bed, then the bed itself.
                    let undug = (g - cell.target).max(0.0) * px;
                    let water = (d * px).min(PROFILE_H);
                    if gate && k.gate.built {
                        let sill = (k.gate.sill() - g).max(0.0) * px;
                        if sill > 0.5 {
                            b.add(VNode::new(Node { width: Val::Px(bar_w), height: Val::Px(sill.min(PROFILE_H)), ..default() })).bg(WOOD);
                        }
                    }
                    if water > 0.5 {
                        b.add(VNode::new(Node { width: Val::Px(bar_w), height: Val::Px(water), ..default() })).bg(if d >= sim::canals::FLOAT_DEPTH { WATER } else { SHALLOW.with_alpha(0.8) });
                    }
                    if undug > 0.5 {
                        let v = b.add(VNode::new(Node { width: Val::Px(bar_w), height: Val::Px(undug), border: UiRect::all(Val::Px(if silted || picked { 1.5 } else { 0.0 })), ..default() }));
                        v.bg = GROUND.with_alpha(0.9);
                        if silted || picked {
                            v.border = Some(BorderColor::all(if picked { SELECT } else { BAD }));
                        }
                    }
                    let base = ((cell.target.min(g) - low) * px).max(2.0);
                    let bed = b.add(VNode::new(Node { width: Val::Px(bar_w), height: Val::Px(base), border: UiRect::top(Val::Px(1.5)), ..default() }));
                    bed.bg = MUD;
                    bed.border = Some(BorderColor::all(WOOD.with_alpha(0.7)));
                });
                let mut tip = Tip::name(if gate { "Gate cell" } else { "Cell" }).line("Cut", format!("{} / {}", num((cell.ground - g).max(0.0)), num(cell.ground - cell.target)));
                if d > 0.0 {
                    tip = tip.line("Water", format!("{} m", num(d)));
                }
                if silted {
                    tip = tip.line("Silted", "");
                }
                // Dug cells fill with silt, faster where water runs.
                if g < cell.ground - 0.001 {
                    let rate = sim::canals::silt_per_day(s, cell.cell);
                    tip = tip.line("Next silt in", days((sim::canals::SILT_STEP - cell.silt).max(0.0) / rate.max(1e-6)));
                    if sim::canals::running(s, cell.cell) {
                        tip = tip.line("Running water", format!("×{}", num(sim::canals::SILT_RUNNING_FACTOR)));
                    }
                }
                col.action(HudAction::DamCell(cell.cell)).tip(tip.line("Click", "pick for Deepen").line("Shift", "add"));
            }
        });
    })
    .bg(Color::srgba(1.0, 1.0, 1.0, 0.04));
}

fn gate_row(c: &Ctx, u: &mut Ui, id: BuildingId, k: &Canal) {
    let g = k.gate;
    let s = c.s;
    let wanted = g.wanted();
    let unlocked = g.built || sim::canals::gates_unlocked(s);
    u.row(5.0, |r| {
        r.icon(Icon::Lock, 15.0, if g.built { WOOD } else { MUTED }).tip(if g.built {
            Tip::name("Gate").line("Sill", num(g.sill())).line("Pond", k.status.as_ref().map_or(String::new(), |st| num(st.source_level)))
        } else if unlocked {
            Tip::name("No gate").line("Pick a setting to build one", "").line("Sticks", num(sim::canals::GATE_STICKS))
        } else {
            Tip::name("No gate").line("Needs", "Logistics")
        });
        if !unlocked {
            return;
        }
        // Level starts at the pond's own level: hold it where it stands.
        let level_at = match wanted { GateSetting::Level(e) => e, _ => k.status.as_ref().map_or(g.sill(), |st| st.source_level) };
        let drop = k.status.as_ref().map_or(0.0, |st| (st.source_level - g.floor).max(0.0));
        for (setting, icon, name) in [(GateSetting::Shut, Icon::Lock, "Shut"), (GateSetting::Level(level_at), Icon::Crest, "Level"), (GateSetting::Open, Icon::Flood, "Open")] {
            let on = (g.built || g.pending.is_some()) && std::mem::discriminant(&wanted) == std::mem::discriminant(&setting);
            let mut tip = Tip::name(name);
            tip = match setting {
                GateSetting::Shut => tip.line("No water passes", ""),
                GateSetting::Level(_) => tip.line("Holds the pond at the sill", "").line("Sill", num(g.sill_of(setting))),
                GateSetting::Open => tip.line("Free flow", "").line("Pond can fall to", num(g.floor)).line("Drop", format!("{} m", num(drop))),
            };
            if g.pending.is_some() && on {
                tip = tip.line("A builder will set it", "");
            }
            r.pebble(icon, 13.0, HudAction::SetGate(id, g.cell, setting), if on { Peb::On } else { Peb::Idle }).tip(tip);
        }
        r.spacer();
        if (g.built || g.pending.is_some()) && matches!(wanted, GateSetting::Level(_)) {
            let span = (g.top - g.floor).max(0.05);
            let frac = ((g.sill_of(GateSetting::Level(level_at)) - g.floor) / span).clamp(0.0, 1.0);
            r.notches(frac, 6, 70.0, 10.0, WOOD);
            super::dam_panel::sill_steps(r, id, &g, level_at);
        }
        if g.built {
            super::dam_panel::lock_pebble(r, s, id, g.cell);
        }
    });
}

fn grow_button(u: &mut Ui, icon: Icon, name: &str, id: BuildingId, growth: CanalGrowth, cost: Result<f32, String>, suggested: bool) {
    let ok = cost.is_ok();
    let v = u.node(Node { width: Val::Px(64.0), height: Val::Px(52.0), flex_direction: FlexDirection::Column, align_items: AlignItems::Center, justify_content: JustifyContent::Center, row_gap: Val::Px(3.0), ..default() }, |b| {
        b.icon(icon, 16.0, if ok { INK } else { FAINT.with_alpha(0.5) });
        b.row(3.0, |x| {
            x.icon(Icon::Beaver, 11.0, if ok { DIGGER } else { MUTED });
            x.bold(cost.as_ref().map(|d| format!("{}d", d.ceil().max(1.0) as i64)).unwrap_or_else(|_| "-".into()), 11.5, if ok { INK } else { MUTED });
        });
    });
    v.skin(if suggested { Skin::PebbleOn } else { Skin::Pebble });
    if !ok {
        v.faded();
    }
    v.tip(match &cost {
        Ok(d) => Tip::name(name).line("Beaver-days", num(*d)),
        Err(e) => Tip::name(name).line(e.clone(), ""),
    });
    if ok {
        v.action(HudAction::GrowCanal(id, growth));
    }
}

fn actions(c: &Ctx, u: &mut Ui, id: BuildingId, k: &Canal) {
    let s = c.s;
    let picked: Vec<(usize, usize)> = c.hud.dam_cells.iter().copied().filter(|p| k.cells.iter().any(|x| x.cell == *p)).collect();
    let deepen = sim::canals::growth_labour(s, id, CanalGrowth::Deepen, &picked);
    let widen = sim::canals::growth_labour(s, id, CanalGrowth::Widen, &picked);
    let paused = s.work.place(id).is_some_and(|p| p.paused);
    let dry = k.status.as_ref().is_some_and(|st| st.dug * 2 >= st.cells && st.floatable == 0);
    u.row(4.0, |r| {
        grow_button(r, Icon::Down, if picked.is_empty() { "Deepen" } else { "Deepen picked cells" }, id, CanalGrowth::Deepen, deepen.clone(), dry && deepen.is_ok());
        grow_button(r, Icon::Widen, if picked.is_empty() { "Widen" } else { "Widen at picked cells" }, id, CanalGrowth::Widen, widen, false);
        // Extending is drawn, not bought here: this starts the drawing from the open end.
        let drawing = c.hud.extending == Some(id);
        r.node(Node { width: Val::Px(64.0), height: Val::Px(52.0), justify_content: JustifyContent::Center, align_items: AlignItems::Center, ..default() }, |b| {
            b.icon(Icon::Extend, 18.0, INK);
        })
        .skin(if drawing { Skin::PebbleOn } else { Skin::Pebble })
        .action(HudAction::GrowCanal(id, CanalGrowth::Extend))
        .tip(Tip::name("Extend").line("Drag from the open end", "").line("Click", "add a corner"));
        r.spacer();
        let on = k.maintain;
        r.pebble(Icon::Hammer, 15.0, HudAction::CanalMaintain(id, !on), if on { Peb::Good } else { Peb::Idle }).tip(Tip::name("Maintain").line(if on { "On" } else { "Off" }, "").line("Maintainers clear silt", ""));
        r.pebble(if paused { Icon::Play } else { Icon::Pause }, 14.0, HudAction::Paused(id, !paused), if paused { Peb::On } else { Peb::Idle }).tip(Tip::name(if paused { "Resume" } else { "Pause" }));
        super::words::cancel_build(r, s, id);
    });
}

fn footer(c: &Ctx, u: &mut Ui, id: BuildingId, k: &Canal) {
    let s = c.s;
    let crew = sim::canals::crew(s, id);
    let left = sim::canals::visits_left(s, id);
    let clay: f32 = s.resources.items().iter().filter(|r| r.kind == sim::resources::ResourceKind::MudBank && k.cells.iter().any(|x| x.cell.0.abs_diff(r.cell.0) <= 1 && x.cell.1.abs_diff(r.cell.1) <= 1)).map(|r| r.amount).sum();
    u.rule();
    u.row(14.0, |r| {
        r.row(4.0, |x| {
            x.icon(Icon::Beaver, 14.0, DIGGER);
            x.bold(format!("{crew}"), 13.0, INK);
        })
        .tip(Tip::name("Diggers"));
        r.row(4.0, |x| {
            x.icon(good_icon(Good::Clay), 14.0, good_color(Good::Clay));
            x.bold(format!("{}", clay.round() as i64), 13.0, INK);
        })
        .tip(Tip::name("Clay on the banks").line("Gatherers bring it in", ""));
        r.spacer();
        if left > 0 {
            r.row(4.0, |x| {
                x.icon(Icon::Site, 13.0, MUTED);
                x.bold(format!("{left}"), 12.0, MUTED);
            })
            .tip(Tip::name("Visits still to make").line("Beaver-days", num(sim::canals::dig_days(left))));
        }
        r.row(3.0, |x| {
            for _ in 0..k.width() {
                x.add(VNode::new(Node { width: Val::Px(4.0), height: Val::Px(12.0), border_radius: BorderRadius::all(Val::Px(1.0)), ..default() })).bg(WATER);
            }
        })
        .tip(Tip::name("Width").line("Rows", format!("{}", k.width())));
    });
}

fn history(u: &mut Ui, k: &Canal) {
    if k.history.is_empty() {
        u.row(6.0, |r| {
            r.icon(Icon::Hourglass, 14.0, MUTED);
        })
        .tip(Tip::name("No days recorded yet"));
        return;
    }
    let flow_top = k.history.iter().map(|h| h.inflow.max(h.outflow)).fold(0.0, f32::max).max(1e-3);
    let silt_top = k.history.iter().map(|h| h.silted).fold(0.0, f32::max).max(1.0);
    let w = (INNER / sim::canals::HISTORY_DAYS as f32).floor().max(2.0);
    let chart = |u: &mut Ui, icon: Icon, col: Color, name: &str, value: &dyn Fn(&sim::canals::CanalDay) -> f32| {
        u.row(6.0, |r| {
            r.icon(icon, 13.0, col);
            r.node(Node { flex_direction: FlexDirection::Row, align_items: AlignItems::FlexEnd, height: Val::Px(36.0), column_gap: Val::Px(1.0), ..default() }, |b| {
                for day in &k.history {
                    let f = value(day).clamp(0.0, 1.0);
                    b.add(VNode::new(Node { width: Val::Px(w - 1.0), height: Val::Px((f * 34.0).max(1.0)), ..default() })).bg(col);
                }
            });
        })
        .tip(Tip::name(name).line("Days", format!("{}", k.history.len())));
    };
    chart(u, Icon::Logs, WATER, "Floats a load", &|h| h.floatable);
    chart(u, Icon::Upstream, SHALLOW, "Water in", &|h| h.inflow / flow_top);
    chart(u, Icon::Push, WARN, "Water out", &|h| h.outflow / flow_top);
    chart(u, Icon::Down, BAD, "Silted cells", &|h| h.silted / silt_top);
}

pub fn canal_panel(c: &Ctx, id: BuildingId, k: &Canal) -> VNode {
    let mut ui = Ui::default();
    ui.node(Node { width: Val::Px(RIGHT_W), flex_direction: FlexDirection::Column, row_gap: Val::Px(8.0), ..default() }, |u| {
        header(c, u, id, k);
        tabs(c, u);
        match c.hud.dam_tab {
            DamTab::Status => {
                water_rows(u, k);
                profile(c, u, id, k);
                gate_row(c, u, id, k);
                actions(c, u, id, k);
            }
            DamTab::History => history(u, k),
        }
        footer(c, u, id, k);
    });
    let mut v = ui.nodes.pop().expect("panel node");
    panel(&mut v);
    v.node.padding = UiRect::all(Val::Px(12.0));
    v
}
