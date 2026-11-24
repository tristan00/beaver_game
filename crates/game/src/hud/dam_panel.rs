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
const WOOD: Color = Color::srgb(0.95, 0.85, 0.66);
const INNER: f32 = RIGHT_W - 20.0;
const SECTION_H: f32 = 64.0;

fn abs_box(left: f32, top: f32, w: f32, h: f32) -> Node {
    Node { position_type: PositionType::Absolute, left: Val::Px(left), top: Val::Px(top), width: Val::Px(w), height: Val::Px(h), ..default() }
}

/// A small painted view of the dam: deep water behind, a stick wall, the stream below.
fn portrait(u: &mut Ui, health: f32) {
    u.node(Node { width: Val::Px(52.0), height: Val::Px(52.0), padding: UiRect::all(Val::Px(5.0)), flex_shrink: 0.0, ..default() }, |frame| {
    frame.node(Node { width: Val::Px(42.0), height: Val::Px(42.0), overflow: Overflow::clip(), ..default() }, |p| {
        p.add(VNode::new(abs_box(0.0, 10.0, 19.0, 32.0))).bg(DEEP);
        p.add(VNode::new(abs_box(28.0, 32.0, 14.0, 10.0))).bg(SHALLOW.with_alpha(0.8));
        let mound = p.add(VNode::new(Node { border_radius: BorderRadius::top(Val::Px(5.0)), ..abs_box(16.0, 6.0, 14.0, 36.0) }));
        mound.bg = if health < 0.5 { Color::srgb(0.66, 0.4, 0.33) } else { MUD };
        for (y, x) in [(16.0, 23.0), (26.0, 22.0), (36.0, 23.0)] {
            p.add(VNode::new(abs_box(x - 6.0, y - 4.0, 14.0, 2.0))).bg(WOOD.with_alpha(0.6));
        }
    });
    })
    .skin(Skin::Portrait);
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
        r.col(7.0, |x| {
            x.bold("Dam", 16.0, INK);
            let mut tip = Tip::name("Condition").with_value(format!("{:.0}%", health * 100.0)).line("Worn cells", format!("{}", d.cells.iter().filter(|c| sim::dams::built(s, c.cell) > 0.0 && sim::dams::integrity(s, c.cell) < sim::dams::REPAIR_BELOW).count()));
            // Wear: the daily rate, then only what is acting on it now.
            if let Some(w) = sim::dams::wear(s, d) {
                tip = tip.line("Wear", format!("{:.2}%/day", (w.age + w.overtop) * 100.0)).line("Age", format!("{:.2}%/day", w.age * 100.0));
                if w.overtopped > 0 {
                    tip = tip.line("Overtopping", format!("{:.2}%/day", w.overtop * 100.0)).line("Overtopped cells", format!("{}", w.overtopped));
                }
                if w.faced > 1.005 {
                    tip = tip.line("Pebble faced", format!("÷{:.2}", w.faced));
                }
            }
            x.pips(health, health_colour(health)).tip(tip);
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

fn pond_rows(c: &Ctx, u: &mut Ui, d: &Dam) {
    let Some(p) = d.pond.as_ref() else {
        u.row(6.0, |r| {
            r.icon(Icon::Gauge, 15.0, WATER);
            r.notches(0.0, 12, 190.0, 12.0, WATER);
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
            b.notches(frac, 12, 186.0, 12.0, WATER);
            b.add(VNode::new(Node { width: Val::Px(3.0), height: Val::Px(20.0), margin: UiRect::left(Val::Px(3.0)), ..default() })).bg(WOOD);
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
                col.action(HudAction::DamCell(cell.cell)).tip(tip.line("Click", "pick for Raise or Gate").line("Shift", "add"));
            }
        });
    })
    .bg(Color::srgba(1.0, 1.0, 1.0, 0.04));
}

fn grow_button(u: &mut Ui, icon: Icon, name: &str, id: BuildingId, growth: DamGrowth, plan: Result<f32, String>, suggested: bool) {
    let ok = plan.is_ok();
    let v = u.node(Node { width: Val::Px(64.0), height: Val::Px(52.0), flex_direction: FlexDirection::Column, align_items: AlignItems::Center, justify_content: JustifyContent::Center, row_gap: Val::Px(3.0), ..default() }, |b| {
        b.icon(icon, 16.0, if ok { INK } else { FAINT.with_alpha(0.5) });
        b.row(3.0, |x| {
            x.icon(good_icon(Good::Sticks), 12.0, if ok { WOOD } else { MUTED });
            x.bold(plan.as_ref().map(|c| format!("{}", c.round() as i64)).unwrap_or_else(|_| "-".into()), 11.5, if ok { INK } else { MUTED });
        });
    });
    v.skin(if suggested { Skin::PebbleOn } else { Skin::Pebble });
    if !ok {
        v.faded();
    }
    let tip = match &plan {
        Ok(c) => Tip::name(name).line("Sticks", num(*c)),
        Err(e) => Tip::name(name).line(e.clone(), ""),
    };
    v.tip(tip);
    if ok {
        v.action(HudAction::GrowDam(id, growth));
    }
}

/// Turn the picked dam cell into a gate (suggested when the pond runs over the crest).
fn gate_button(u: &mut Ui, id: BuildingId, plan: Result<f32, String>, suggested: bool) {
    let ok = plan.is_ok();
    let v = u.node(Node { width: Val::Px(52.0), height: Val::Px(52.0), flex_direction: FlexDirection::Column, align_items: AlignItems::Center, justify_content: JustifyContent::Center, row_gap: Val::Px(3.0), ..default() }, |b| {
        b.icon(Icon::Lock, 16.0, if ok { INK } else { FAINT.with_alpha(0.5) });
        b.row(3.0, |x| {
            x.icon(good_icon(Good::Sticks), 12.0, if ok { WOOD } else { MUTED });
            x.bold(plan.as_ref().map(|c| format!("{}", c.round() as i64)).unwrap_or_else(|_| "-".into()), 11.5, if ok { INK } else { MUTED });
        });
    });
    v.skin(if suggested && ok { Skin::PebbleOn } else { Skin::Pebble });
    if !ok {
        v.faded();
    }
    v.tip(match &plan {
        Ok(c) => Tip::name("Gate").line("Let water through here", "").line("Sticks", num(*c)),
        Err(e) => Tip::name("Gate").line(e.clone(), ""),
    });
    if ok {
        v.action(HudAction::AddDamGate(id));
    }
}

/// Make the picked dam cell a lock straight away: its gate and the lock in one builder visit.
fn lock_button(u: &mut Ui, id: BuildingId, picked: Option<sim::water::Cell>) {
    use sim::locks;
    let ok = picked.is_some();
    let sticks = sim::canals::GATE_STICKS + locks::LOCK_STICKS;
    let v = u.node(Node { width: Val::Px(52.0), height: Val::Px(52.0), flex_direction: FlexDirection::Column, align_items: AlignItems::Center, justify_content: JustifyContent::Center, row_gap: Val::Px(3.0), ..default() }, |b| {
        b.icon(Icon::LockGates, 16.0, if ok { INK } else { FAINT.with_alpha(0.5) });
        b.row(3.0, |x| {
            x.icon(good_icon(Good::Sticks), 12.0, if ok { WOOD } else { MUTED });
            x.bold(format!("{}", sticks.round() as i64), 11.5, if ok { INK } else { MUTED });
        });
    });
    v.skin(Skin::Pebble);
    if !ok {
        v.faded();
    }
    let tip = Tip::name("Lock").line("Beavers and barges get past", "").line("Sticks", num(sticks)).line("Clay", num(locks::LOCK_CLAY));
    v.tip(if ok { tip } else { tip.line("Pick a built dam cell first", "") });
    if let Some(cell) = picked {
        v.action(HudAction::UpgradeLock(id, cell));
    }
}

/// The lock pebble at the end of a built gate's row: upgrade it into a lock (with a Harbor),
/// waiting for its builder, or a lock already.
pub(super) fn lock_pebble(r: &mut Ui, s: &sim::SimState, id: BuildingId, cell: sim::water::Cell) {
    use sim::locks;
    let cost = |t: Tip| t.line("Sticks", num(locks::LOCK_STICKS)).line("Clay", num(locks::LOCK_CLAY));
    // Only the upgrade itself is a button; the other states just show.
    let shown = |r: &mut Ui, look: Peb, tip: Tip| {
        r.pebble(Icon::LockGates, 13.0, HudAction::UpgradeLock(id, cell), look).tip(tip);
        if let Some(v) = r.nodes.last_mut() {
            v.action = None;
        }
    };
    if locks::is_lock(s, cell) {
        shown(r, Peb::On, Tip::name("Lock").line("Beavers and barges get past", ""));
    } else if locks::pending(s, cell) {
        shown(r, Peb::On, cost(Tip::name("Lock").line("A builder will build it", "")));
    } else if locks::unlocked(s) {
        r.pebble(Icon::LockGates, 13.0, HudAction::UpgradeLock(id, cell), Peb::Idle).tip(cost(Tip::name("Make a lock").line("Beavers and barges get past", "")));
    } else {
        shown(r, Peb::Faded, Tip::name("Lock").line("Needs", "Harbor"));
    }
}

/// One row per gate on the dam: Shut, Level (with the sill nudged in 5 cm steps) or Open.
fn gate_rows(u: &mut Ui, s: &sim::SimState, id: BuildingId, d: &Dam) {
    use sim::GateSetting;
    for g in &d.gates {
        let wanted = g.wanted();
        // Level starts at the pond's own level: hold it where it stands.
        let level_at = match wanted { GateSetting::Level(e) => e, _ => d.pond.as_ref().map_or(g.sill(), |p| p.level) };
        u.row(5.0, |r| {
            r.icon(Icon::Lock, 15.0, if g.built { WOOD } else { MUTED }).tip(if g.built {
                Tip::name("Gate").line("Sill", num(g.sill()))
            } else {
                Tip::name("Gate").line("A builder brings its sticks", "")
            });
            for (setting, icon, name) in [(GateSetting::Shut, Icon::Lock, "Shut"), (GateSetting::Level(level_at), Icon::Crest, "Level"), (GateSetting::Open, Icon::Flood, "Open")] {
                let on = std::mem::discriminant(&wanted) == std::mem::discriminant(&setting);
                let mut tip = Tip::name(name);
                tip = match setting {
                    GateSetting::Shut => tip.line("Stands at the crest", ""),
                    GateSetting::Level(_) => tip.line("Holds the pond at the sill", "").line("Sill", num(g.sill_of(setting))),
                    GateSetting::Open => tip.line("Lets the pond down to the ground", ""),
                };
                if g.pending.is_some() && on {
                    tip = tip.line("A builder will set it", "");
                }
                r.pebble(icon, 13.0, HudAction::SetGate(id, g.cell, setting), if on { Peb::On } else { Peb::Idle }).tip(tip);
            }
            r.spacer();
            if matches!(wanted, GateSetting::Level(_)) {
                r.pebble(Icon::Minus, 10.0, HudAction::SetGate(id, g.cell, GateSetting::Level(level_at - sim::canals::GATE_STEP)), Peb::Idle).tip(Tip::name("Lower the sill").line("Step", "5 cm"));
                let frac = ((g.sill_of(GateSetting::Level(level_at)) - g.floor) / (g.top - g.floor).max(0.05)).clamp(0.0, 1.0);
                r.notches(frac, 6, 70.0, 10.0, WOOD);
                r.pebble(Icon::Plus, 10.0, HudAction::SetGate(id, g.cell, GateSetting::Level(level_at + sim::canals::GATE_STEP)), Peb::Idle).tip(Tip::name("Raise the sill").line("Step", "5 cm"));
            }
            if g.built {
                lock_pebble(r, s, id, g.cell);
            }
        });
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
        let gate = match c.hud.dam_cells.as_slice() {
            [cell] => sim::dams::plan_gate(s, id, *cell),
            [] => Err("pick a dam cell first".into()),
            _ => Err("pick one cell".into()),
        };
        let picked = c.hud.dam_cells.first().copied().filter(|_| gate.is_ok());
        gate_button(r, id, gate, matches!(spill, Spill::Over(_)));
        if sim::locks::unlocked(s) {
            lock_button(r, id, picked);
        }
        r.spacer();
        pebble_button(s, r, id);
        let on = d.maintain;
        r.pebble(Icon::Hammer, 15.0, HudAction::DamMaintain(id, !on), if on { Peb::Good } else { Peb::Idle }).tip(Tip::name("Maintain").line(if on { "On" } else { "Off" }, "").line("Maintainers repair worn cells", ""));
        r.pebble(if paused { Icon::Play } else { Icon::Pause }, 14.0, HudAction::Paused(id, !paused), if paused { Peb::On } else { Peb::Idle }).tip(Tip::name(if paused { "Resume" } else { "Pause" }));
        super::words::cancel_build(r, s, id);
    });
}

/// Face the dam with pebbles: it then wears and erodes more slowly.
fn pebble_button(s: &sim::SimState, r: &mut Ui, id: BuildingId) {
    use super::words::{good_color, good_icon, good_name, num};
    let Some(b) = s.buildings.building(id) else { return };
    if s.buildings.pebbled.contains(&id) {
        r.icon(Icon::Pebbles, 15.0, GOOD).tip(Tip::name("Pebble faced").line("Erodes", format!("{}× slower", num(sim::buildings::PEBBLE_DAM_FACTOR))));
    } else if s.maintenance.jobs.contains_key(&id) {
        r.pebble(Icon::Undo, 12.0, HudAction::CancelMaintenance(id), Peb::Good).tip(Tip::name("Facing with pebbles").line("Stop", ""));
    } else if b.is_complete() {
        let mut tip = Tip::name("Face with pebbles").line("Erodes", format!("{}× slower", num(sim::buildings::PEBBLE_DAM_FACTOR)));
        for (g, a) in sim::maintenance::cost(b, sim::maintenance::Refit::Pebbles).0 {
            tip.push(TipLine::new(good_name(g), num(a)).icon(good_icon(g), good_color(g)));
        }
        r.pebble(Icon::Pebbles, 15.0, HudAction::PebbleBuilding(id), Peb::Idle).tip(tip);
    }
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
                gate_rows(u, c.s, id, d);
                actions(c, u, id, d);
            }
            DamTab::History => history(u, d),
        }
        footer(c, u, id, d);
    });
    let mut v = ui.nodes.pop().expect("panel node");
    panel(&mut v);
    v.node.padding = UiRect::all(Val::Px(12.0));
    v
}
