//! The panel a dam opens: what it is doing in a word, its pond, each gate plainly built or
//! not, the work on it, and what can be done to it. Detail lives in tooltips.
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


fn tabs(c: &Ctx, u: &mut Ui) {
    u.row(6.0, |r| {
        for (tab, icon, name) in [(DamTab::Status, Icon::Drop, "Status"), (DamTab::History, Icon::Hourglass, "History")] {
            let on = c.hud.dam_tab == tab;
            r.pebble(icon, 13.0, HudAction::DamTab(tab), if on { Peb::On } else { Peb::Idle }).tip(Tip::name(name));
        }
    });
    u.rule();
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


/// A short state in its colour: an icon and a word or two.
fn chip<'a>(r: &'a mut Ui, icon: Icon, text: &str, col: Color) -> &'a mut VNode {
    r.row(4.0, |x| {
        x.icon(icon, 13.0, col);
        x.bold(text, 12.0, col);
    })
}

/// What the dam as a whole is doing, in a word or two, and its colour.
fn dam_state(s: &sim::SimState, id: BuildingId, d: &Dam) -> (Icon, &'static str, Color, &'static str) {
    let (builders, _) = sim::dams::crew(s, id);
    let building = sim::dams::has_work_for(s, id, false);
    if building && builders == 0 {
        return (Icon::RoleBuilder, "No builders", BAD, "Work is waiting and no builder is on it");
    }
    if building {
        return (Icon::Hammer, "Building", WARN, "Builders are at work on it");
    }
    match d.pond.as_ref().map(|p| (p.spill, p.days_to_full)) {
        None => (Icon::Hourglass, "No pond yet", MUTED, "The finished cells will start holding water"),
        Some((Spill::Over(_), _)) => (Icon::Crest, "Spilling over", WARN, "Water runs over the crest and wears it"),
        Some((Spill::Around(_), _)) => (Icon::Extend, "Water going round", WARN, "Water gets round an end: extend the dam"),
        Some((_, days)) if days > 0.0 => (Icon::Up, "Filling", WATER, "The pond is still rising"),
        Some((Spill::Gate(_), _)) => (Icon::Lock, "Holding", GOOD, "Full, letting water out by its gate"),
        Some(_) => (Icon::Check, "Holding", GOOD, "Full"),
    }
}

fn header(c: &Ctx, u: &mut Ui, id: BuildingId, d: &Dam) {
    let s = c.s;
    let health = sim::dams::health(s, d);
    let sel = Selection::Site(id);
    let pinned = c.hud.pinned.contains(&sel);
    let (icon, state, col, why) = dam_state(s, id, d);
    u.row(10.0, |r| {
        portrait(r, health);
        r.col(6.0, |x| {
            x.bold("Dam", 16.0, INK);
            chip(x, icon, state, col).tip(Tip::name(state).line(why, ""));
            let mut tip = Tip::name("Condition").with_value(format!("{:.0}%", health * 100.0)).line("Worn cells", format!("{}", d.cells.iter().filter(|c| sim::dams::built(s, c.cell) > 0.0 && sim::dams::integrity(s, c.cell) < sim::dams::REPAIR_BELOW).count()));
            if let Some(w) = sim::dams::wear(s, d) {
                tip = tip.line("Wear", format!("{:.2}%/day", (w.age + w.overtop) * 100.0)).line("Age", format!("{:.2}%/day", w.age * 100.0));
                if w.overtopped > 0 {
                    tip = tip.line("Overtopping", format!("{:.2}%/day", w.overtop * 100.0)).line("Overtopped cells", format!("{}", w.overtopped));
                }
                if w.faced > 1.005 {
                    tip = tip.line("Pebble faced", format!("÷{:.2}", w.faced));
                }
            }
            tip = tip.line("Rows", format!("{}", d.rows)).line("Height limit", num(d.cap()));
            x.row(5.0, |y| {
                y.pips(health, health_colour(health));
                y.text(format!("{:.0}%", health * 100.0), 11.0, MUTED);
            })
            .tip(tip);
        });
        r.spacer();
        r.row(4.0, |y| {
            y.pebble(Icon::Pin, 10.0, HudAction::Pin(sel), if pinned { Peb::On } else { Peb::Bare }).tip(Tip::name(if pinned { "Unpin" } else { "Pin" }));
            y.pebble(Icon::Cross, 10.0, if pinned { HudAction::Pin(sel) } else { HudAction::CloseInspector }, Peb::Bare).tip(Tip::name("Close").key("Esc"));
        })
        .style(|n| n.align_self = AlignSelf::FlexStart);
    });
}

/// The pond: how full, water in and out a day, and where it leaves.
fn water(u: &mut Ui, d: &Dam) {
    let Some(p) = d.pond.as_ref() else {
        u.row(6.0, |r| {
            r.icon(Icon::Gauge, 15.0, MUTED);
            r.text("No pond yet", 12.0, MUTED);
        });
        return;
    };
    let span = (p.full_level - p.bottom).max(0.01);
    let frac = ((p.level - p.bottom) / span).clamp(0.0, 1.0);
    u.row(6.0, |r| {
        r.icon(Icon::Gauge, 15.0, WATER);
        r.bar(frac, 170.0, 10.0, WATER);
        r.bold(format!("{} m", num(p.level - p.bottom)), 12.0, INK);
        r.spacer();
        if p.days_to_full > 0.0 {
            r.icon(Icon::Up, 11.0, WATER);
            r.text(days(p.days_to_full), 11.0, MUTED);
        }
    })
    .tip(Tip::name("Pond").line("Depth", format!("{} m", num(p.level - p.bottom))).line("Full at", format!("{} m", num(p.full_level - p.bottom))).line("Area", format!("{} cells", p.area)).line("Deepest", format!("{} m", num(p.max_depth))).line(if p.days_to_full > 0.0 { "Full in" } else { "Full" }, if p.days_to_full > 0.0 { days(p.days_to_full) } else { String::new() }));
    let (out_icon, out_col, out_name) = match p.spill {
        Spill::Over(_) => (Icon::Crest, WARN, "Over the crest"),
        Spill::Around(_) => (Icon::Extend, WARN, "Round an end"),
        Spill::Gate(_) => (Icon::Lock, GOOD, "Through a gate"),
        Spill::None => (Icon::Drop, MUTED, "Seeping and drying"),
    };
    u.row(12.0, |r| {
        r.row(4.0, |x| {
            x.icon(Icon::Upstream, 13.0, WATER);
            x.bold(format!("{}/d", num(p.inflow)), 12.0, INK);
        })
        .tip(Tip::name("Water in").line("per day", ""));
        r.row(4.0, |x| {
            x.icon(Icon::Push, 13.0, WATER);
            x.bold(format!("{}/d", num(p.outflow)), 12.0, INK);
            x.icon(out_icon, 13.0, out_col);
        })
        .tip(Tip::name("Water out").line(out_name, "").line("Spill, seepage and evaporation", "per day"));
    });
}

/// One row per gate: plainly whether it is built yet, what it is set to, and its settings.
fn gate_rows(u: &mut Ui, s: &sim::SimState, id: BuildingId, d: &Dam) {
    use sim::GateSetting;
    if d.gates.is_empty() {
        return;
    }
    u.rule();
    for g in &d.gates {
        let wanted = g.wanted();
        // Level starts at the pond's own level: hold it where it stands.
        let level_at = match wanted { GateSetting::Level(e) => e, _ => d.pond.as_ref().map_or(g.sill(), |p| p.level) };
        let lock = sim::locks::is_lock(s, g.cell);
        let setting_name = |st: GateSetting| match st {
            GateSetting::Shut => "Shut",
            GateSetting::Level(_) => "Level",
            GateSetting::Open => "Open",
        };
        u.row(5.0, |r| {
            r.icon(if lock { Icon::LockGates } else { Icon::Lock }, 15.0, if g.built { WOOD } else { WARN });
            // What it is now, in a word: not built yet (whatever it is set to), changing, or set.
            let (word, col, tip) = if !g.built {
                ("Not built", WARN, Tip::name("Not built yet").line("A builder brings its sticks", "").line("Then", setting_name(wanted)))
            } else if g.pending.is_some() {
                ("Changing", WARN, Tip::name(format!("To be {}", setting_name(wanted))).line("A builder will set it", ""))
            } else {
                (setting_name(g.setting), INK, Tip::name(setting_name(g.setting)).line("Sill", num(g.sill())))
            };
            r.node(Node { width: Val::Px(70.0), ..default() }, |w| {
                w.bold(word, 12.0, col);
            })
            .tip(tip);
            for (setting, icon, name) in [(GateSetting::Shut, Icon::Lock, "Shut"), (GateSetting::Level(level_at), Icon::Crest, "Level"), (GateSetting::Open, Icon::Flood, "Open")] {
                let on = std::mem::discriminant(&wanted) == std::mem::discriminant(&setting);
                let tip = match setting {
                    GateSetting::Shut => Tip::name(name).line("Stands at the crest", ""),
                    GateSetting::Level(_) => Tip::name(name).line("Holds the pond at the sill", "").line("Sill", num(g.sill_of(setting))),
                    GateSetting::Open => Tip::name(name).line("Lets the pond down to the ground", ""),
                };
                r.pebble(icon, 13.0, HudAction::SetGate(id, g.cell, setting), if on { Peb::On } else { Peb::Idle }).tip(tip);
            }
            if matches!(wanted, GateSetting::Level(_)) {
                r.pebble(Icon::Minus, 10.0, HudAction::SetGate(id, g.cell, GateSetting::Level(level_at - sim::canals::GATE_STEP)), Peb::Idle).tip(Tip::name("Lower the sill").line("Step", "5 cm"));
                r.pebble(Icon::Plus, 10.0, HudAction::SetGate(id, g.cell, GateSetting::Level(level_at + sim::canals::GATE_STEP)), Peb::Idle).tip(Tip::name("Raise the sill").line("Step", "5 cm"));
            }
            r.spacer();
            if g.built {
                lock_pebble(r, s, id, g.cell);
            } else if sim::locks::pending(s, g.cell) {
                r.icon(Icon::LockGates, 13.0, WARN).tip(Tip::name("Lock").line("Built with the gate", ""));
            }
        });
    }
}

/// The work on it: builders here, loads coming, loads still to carry; and plainly when work
/// waits with no builder on it.
fn work(c: &Ctx, u: &mut Ui, id: BuildingId) {
    let s = c.s;
    let (builders, loads) = sim::dams::crew(s, id);
    let left = sim::dams::loads_left(s, id);
    let waiting = sim::dams::has_work_for(s, id, false);
    if !waiting && builders == 0 {
        return;
    }
    u.rule();
    u.row(12.0, |r| {
        r.row(4.0, |x| {
            x.icon(Icon::RoleBuilder, 14.0, if builders == 0 && waiting { BAD } else { INK });
            x.bold(format!("{builders}"), 12.0, if builders == 0 && waiting { BAD } else { INK });
        })
        .tip(Tip::name("Builders here"));
        r.row(4.0, |x| {
            x.icon(good_icon(Good::Sticks), 14.0, WOOD);
            x.bold(format!("{loads}"), 12.0, INK);
        })
        .tip(Tip::name("Loads on the way"));
        if left > 0 {
            r.row(4.0, |x| {
                x.icon(Icon::Site, 13.0, MUTED);
                x.bold(format!("{left}"), 12.0, MUTED);
            })
            .tip(Tip::name("Loads still to carry").line("Sticks", num(left as f32 * sim::dams::sticks_per_step())));
        }
        r.spacer();
        if waiting && builders == 0 {
            chip(r, Icon::Jobs, "Assign builders", BAD).tip(Tip::name("No builder is on it").line("Open the jobs", "")).action(HudAction::Colony(Some(super::ColonyTab::Jobs)));
        }
    });
}

/// What can be done to it, and a word on picking a cell for a gate or a lock.
fn actions(c: &Ctx, u: &mut Ui, id: BuildingId, d: &Dam) {
    let s = c.s;
    let cost = |plan: Result<sim::dams::Plan, String>| plan.map(|p| sim::dams::plan_cost(s, d, &p));
    let raise = cost(sim::dams::plan_raise(s, id, &c.hud.dam_cells));
    let widen = cost(sim::dams::plan_widen(s, id));
    let spill = d.pond.as_ref().map_or(Spill::None, |p| p.spill);
    let extend = if matches!(spill, Spill::Around(_)) { cost(sim::dams::plan_extend(s, id)) } else { Err("water is not getting around the ends".into()) };
    let paused = s.work.place(id).is_some_and(|p| p.paused);
    let gate = match c.hud.dam_cells.as_slice() {
        [cell] => sim::dams::plan_gate(s, id, *cell),
        [] => Err("click a cell on the dam first".into()),
        _ => Err("pick one cell".into()),
    };
    let picked = c.hud.dam_cells.first().copied().filter(|_| gate.is_ok());
    u.rule();
    u.row(6.0, |r| {
        if c.hud.dam_cells.is_empty() {
            r.icon(Icon::Focus, 12.0, MUTED);
            r.text("Click the dam to pick a cell", 11.0, MUTED);
        } else {
            r.icon(Icon::Focus, 12.0, SELECT);
            r.text(if c.hud.dam_cells.len() == 1 { "1 cell picked".to_string() } else { format!("{} cells picked", c.hud.dam_cells.len()) }, 11.0, SELECT);
            r.spacer();
            r.pebble(Icon::Cross, 9.0, HudAction::DamCell(c.hud.dam_cells[0]), Peb::Bare).tip(Tip::name("Unpick"));
        }
    });
    u.row(4.0, |r| {
        grow_button(r, Icon::Rise, if c.hud.dam_cells.is_empty() { "Raise" } else { "Raise picked cells" }, id, DamGrowth::Raise, raise.clone(), matches!(spill, Spill::Over(_)) && raise.is_ok());
        grow_button(r, Icon::Widen, "Widen", id, DamGrowth::Widen, widen, matches!(spill, Spill::Over(_)) && raise.is_err());
        grow_button(r, Icon::Extend, "Extend", id, DamGrowth::Extend, extend.clone(), extend.is_ok());
        gate_button(r, id, gate, matches!(spill, Spill::Over(_)));
        if sim::locks::unlocked(s) {
            lock_button(r, id, picked);
        }
    });
    u.row(4.0, |r| {
        pebble_button(s, r, id);
        let on = d.maintain;
        r.pebble(Icon::Hammer, 15.0, HudAction::DamMaintain(id, !on), if on { Peb::Good } else { Peb::Idle }).tip(Tip::name("Maintain").line(if on { "On" } else { "Off" }, "").line("Maintainers repair worn cells", ""));
        r.pebble(if paused { Icon::Play } else { Icon::Pause }, 14.0, HudAction::Paused(id, !paused), if paused { Peb::On } else { Peb::Idle }).tip(Tip::name(if paused { "Resume" } else { "Pause" }));
        r.spacer();
        super::words::cancel_build(r, s, id);
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
                water(u, d);
                gate_rows(u, c.s, id, d);
                work(c, u, id);
                actions(c, u, id, d);
            }
            DamTab::History => history(u, d),
        }
    });
    let mut v = ui.nodes.pop().expect("panel node");
    panel(&mut v);
    v.node.padding = UiRect::all(Val::Px(12.0));
    v
}
