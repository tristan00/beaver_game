//! The panel a dam opens. Laid out like the canal's (see `works`): header, what it is doing
//! with the one fix, then Water (a side view: how far the pond is from spilling, how far it
//! drops below the dam), Work, Build, Gates and Upkeep. Detail lives in tooltips.
use super::panels::{Ctx, RIGHT_W};
use super::ui::*;
use super::words::*;
use super::works::{self, abs_box, cap, dashes, label_left_of, DEEP, EARTH, SHALLOW, VIEW_W, WOOD};
use super::{ColonyTab, DamTab, HudAction};
use crate::flow::{Cell, Selection};
use crate::icons::Icon;
use bevy::prelude::*;
use sim::buildings::{BuildingId, Good};
use sim::dams::{Dam, Spill};
use sim::DamGrowth;

const VIEW_H: f32 = 92.0;

/// A small painted view of the dam: deep water behind, a stick wall, the stream below.
fn portrait(p: &mut Ui, health: f32) {
    p.node(Node { width: Val::Px(42.0), height: Val::Px(42.0), overflow: Overflow::clip(), ..default() }, |p| {
        p.add(VNode::new(abs_box(0.0, 10.0, 19.0, 32.0))).bg(DEEP);
        p.add(VNode::new(abs_box(28.0, 32.0, 14.0, 10.0))).bg(SHALLOW.with_alpha(0.8));
        let mound = p.add(VNode::new(Node { border_radius: BorderRadius::top(Val::Px(5.0)), ..abs_box(16.0, 6.0, 14.0, 36.0) }));
        mound.bg = if health < 0.5 { BAD.with_alpha(0.7) } else { MUD };
        for (y, x) in [(16.0, 23.0), (26.0, 22.0), (36.0, 23.0)] {
            p.add(VNode::new(abs_box(x - 6.0, y - 4.0, 14.0, 2.0))).bg(WOOD.with_alpha(0.6));
        }
    });
}

/// What can be done to the dam now, worked out once for the status line and the buttons.
struct Offers {
    picks: Vec<Cell>,
    raise: Result<f32, String>,
    widen: Result<f32, String>,
    extend: Result<f32, String>,
    gate: Result<f32, String>,
    /// The picked cell a lock would go on.
    lock: Result<Cell, String>,
}

fn offers(c: &Ctx, id: BuildingId, d: &Dam) -> Offers {
    let s = c.s;
    let picks: Vec<Cell> = c.hud.dam_cells.iter().copied().filter(|p| d.cells.iter().any(|x| x.cell == *p)).collect();
    let cost = |plan: Result<sim::dams::Plan, String>| plan.map(|p| sim::dams::plan_cost(s, d, &p)).map_err(|e| cap(&e));
    let around = d.pond.as_ref().is_some_and(|p| matches!(p.spill, Spill::Around(_)));
    let gate = match picks.as_slice() {
        [cell] => sim::dams::plan_gate(s, id, *cell).map_err(|e| cap(&e)),
        [] => Err("Pick a cell on the dam first".into()),
        _ => Err("Pick just one cell".into()),
    };
    let lock = if !sim::locks::unlocked(s) {
        Err("Needs a Harbor".into())
    } else {
        gate.clone().map(|_| picks[0])
    };
    Offers {
        raise: cost(sim::dams::plan_raise(s, id, &picks)),
        widen: cost(sim::dams::plan_widen(s, id)),
        extend: if around { cost(sim::dams::plan_extend(s, id)) } else { Err("Water isn't getting round the ends".into()) },
        gate,
        lock,
        picks,
    }
}

/// The button the status line points at.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Fix {
    None,
    Assign,
    Raise,
    Widen,
    Extend,
    Maintain,
}

/// What the dam as a whole is doing, its colour, why, and the one fix.
fn status(s: &sim::SimState, id: BuildingId, d: &Dam, o: &Offers) -> (Icon, &'static str, Color, &'static str, Fix) {
    let (builders, _) = sim::dams::crew(s, id);
    let building = sim::dams::has_work_for(s, id, false);
    if building && builders == 0 {
        return (Icon::RoleBuilder, "No builders", BAD, "Work is waiting and no builder is on it", Fix::Assign);
    }
    if building {
        return (Icon::Hammer, "Building", WARN, "Builders are at work on it", Fix::None);
    }
    let worn = sim::dams::health(s, d) < sim::dams::REPAIR_BELOW && !d.maintain;
    match d.pond.as_ref().map(|p| (p.spill, p.days_to_full)) {
        None => (Icon::Hourglass, "No pond yet", MUTED, "The finished cells will start holding water", Fix::None),
        Some((Spill::Over(_), _)) => {
            let fix = if o.raise.is_ok() { Fix::Raise } else if o.widen.is_ok() { Fix::Widen } else { Fix::None };
            (Icon::Crest, "Spilling over", WARN, "Water runs over the top and wears it", fix)
        }
        Some((Spill::Around(_), _)) => (Icon::Extend, "Water going round", WARN, "Water gets round an end", if o.extend.is_ok() { Fix::Extend } else { Fix::None }),
        _ if worn => (Icon::Hammer, "Wearing", WARN, "Cells are worn and nobody keeps it up", Fix::Maintain),
        Some((_, days)) if days > 0.0 => (Icon::Up, "Filling", WATER, "The pond is still rising", Fix::None),
        Some((Spill::Gate(_), _)) => (Icon::Lock, "Holding", GOOD, "Full, letting water out by its gate", Fix::None),
        Some(_) => (Icon::Check, "Holding", GOOD, "Full", Fix::None),
    }
}

fn sticks(c: f32) -> Option<(Icon, Color, String)> {
    Some((good_icon(Good::Sticks), WOOD, format!("{}", c.round() as i64)))
}

fn condition_tip(s: &sim::SimState, d: &Dam, health: f32) -> Tip {
    let worn = d.cells.iter().filter(|c| sim::dams::built(s, c.cell) > 0.0 && sim::dams::integrity(s, c.cell) < sim::dams::REPAIR_BELOW).count();
    let mut tip = Tip::name("Condition").with_value(format!("{:.0}%", health * 100.0)).line("Worn cells", format!("{worn}"));
    if let Some(w) = sim::dams::wear(s, d) {
        tip = tip.line("Wear", format!("{:.2}%/day", (w.age + w.overtop) * 100.0));
        if w.overtopped > 0 {
            tip = tip.line("From spilling over", format!("{:.2}%/day", w.overtop * 100.0));
        }
        if w.faced > 1.005 {
            tip = tip.line("Pebble faced", format!("÷{:.2}", w.faced));
        }
    }
    tip.line("Rows", format!("{}", d.rows)).line("Height limit", format!("{} m", num(d.cap())))
}

/// The pond from the side: water behind the dam, the dam, the water below; how much more the
/// pond can rise before it spills, and how far it stands above the water below.
fn water(c: &Ctx, u: &mut Ui, d: &Dam) {
    let s = c.s;
    u.section("Water", |_| {});
    let Some(p) = d.pond.as_ref() else {
        u.icon(Icon::Hourglass, 15.0, MUTED).tip(Tip::name("No pond yet").line("The finished cells will start holding water", ""));
        return;
    };
    let level = p.level;
    let room = sim::dams::freeboard(s, d);
    let fall = sim::dams::drop(s, d);
    let crest = room.map_or(level, |r| level + r.max(0.0));
    let below = fall.map(|f| level - f);
    let low = p.bottom.min(below.unwrap_or(p.bottom)) - 0.1;
    let high = crest.max(level);
    const HEAD: f32 = 22.0;
    const FLOOR: f32 = 8.0;
    let px = (VIEW_H - HEAD - FLOOR) / (high - low).max(0.3);
    let y = |v: f32| HEAD + (high - v) * px;
    let ground = y(low);
    let spilling = room.is_some_and(|r| r <= 0.02);
    let depth = level - p.bottom;
    let mut pond_tip = Tip::name("Pond").line("Depth", format!("{} m", num(depth))).line("Full at", format!("{} m", num(p.full_level - p.bottom))).line("Area", format!("{} cells", p.area));
    if p.days_to_full > 0.0 {
        pond_tip = pond_tip.line("Full in", days(p.days_to_full));
    }
    u.node(Node { width: Val::Px(VIEW_W), height: Val::Px(VIEW_H), overflow: Overflow::clip(), border_radius: BorderRadius::all(Val::Px(5.0)), ..default() }, |v| {
        v.add(VNode::new(abs_box(0.0, ground, VIEW_W, VIEW_H - ground))).bg(EARTH);
        v.add(VNode::new(abs_box(0.0, y(level), 118.0, ground - y(level)))).bg(DEEP).tip(pond_tip);
        if let Some(b) = below {
            v.add(VNode::new(abs_box(150.0, y(b), VIEW_W - 150.0, ground - y(b)))).bg(SHALLOW.with_alpha(0.8));
        }
        let wall = v.add(VNode::new(Node { border_radius: BorderRadius::top(Val::Px(4.0)), ..abs_box(118.0, y(crest), 32.0, ground - y(crest)) }));
        wall.bg = MUD;
        if spilling {
            v.add(VNode::new(abs_box(116.0, y(crest) - 3.0, 36.0, 3.0))).bg(WATER);
            v.add(VNode::new(abs_box(150.0, y(crest) - 1.0, 4.0, below.map_or(ground, y) - y(crest) + 1.0))).bg(WATER.with_alpha(0.8));
        }
        if depth > 0.3 && ground - y(level) > 20.0 {
            v.node(abs_box(6.0, ground - 18.0, 100.0, 16.0), |l| {
                l.bold(format!("{} m deep", num(depth)), 12.0, INK);
            });
        }
        if let Some(r) = room {
            let tip = Tip::name("Room to rise").line("Before water runs over the lowest part of the top", "");
            if spilling {
                label_left_of(v, 112.0, (y(crest) - 18.0).max(2.0), "0 m to spill".into(), WARN, tip);
            } else {
                v.add(VNode::new(abs_box(110.0, y(crest), 1.5, y(level) - y(crest)))).bg(INK);
                v.add(VNode::new(abs_box(106.0, y(crest), 9.0, 1.5))).bg(INK);
                label_left_of(v, 104.0, (y(crest) - 18.0).max(2.0), format!("{:.1} m to spill", r), INK, tip);
            }
        }
        if let (Some(f), Some(b)) = (fall, below) {
            dashes(v, 152.0, 262.0, y(level), WATER.with_alpha(0.7));
            if f > 0.02 {
                v.add(VNode::new(abs_box(266.0, y(level), 1.5, y(b) - y(level)))).bg(INK);
                v.add(VNode::new(abs_box(262.0, y(level), 9.0, 1.5))).bg(INK);
                v.add(VNode::new(abs_box(262.0, y(b) - 1.5, 9.0, 1.5))).bg(INK);
                label_left_of(v, 260.0, ((y(level) + y(b)) / 2.0 - 8.0).max(2.0), format!("{:.1} m drop", f), INK, Tip::name("Drop").line("From the pond down to the water below the dam", ""));
            }
        }
    })
    .bg(FAINT);
    let (out_word, out_col) = match p.spill {
        Spill::Over(_) => ("over the top", WARN),
        Spill::Around(_) => ("round an end", WARN),
        Spill::Gate(_) => ("by its gate", INK),
        Spill::None => ("seeping", INK),
    };
    u.row(14.0, |r| {
        r.stat(Icon::Upstream, WATER, format!("{}/d", num(p.inflow)), None, INK, Tip::name("Water in").line("Per day", ""));
        r.stat(Icon::Push, WATER, format!("{}/d", num(p.outflow)), Some(out_word), out_col, Tip::name("Water out").line(cap(out_word), "").line("Spill, seepage and evaporation", "per day"));
        r.spacer();
        if p.days_to_full > 0.0 {
            r.stat(Icon::Up, WATER, days(p.days_to_full), Some("to full"), INK, Tip::name("Still rising").line("Full in", days(p.days_to_full)));
        } else {
            r.stat(Icon::Check, GOOD, "Full", None, GOOD, Tip::name("As high as it will get"));
        }
    });
}

/// Builders here, loads coming, loads left; and the way to the jobs.
fn work(c: &Ctx, u: &mut Ui, id: BuildingId, fix: Fix) {
    let s = c.s;
    let (builders, loads) = sim::dams::crew(s, id);
    let left = sim::dams::loads_left(s, id);
    let waiting = sim::dams::has_work_for(s, id, false);
    u.section("Work", |r| {
        r.small_button(Some(Icon::Jobs), Some("Assign"), Some(HudAction::Colony(Some(ColonyTab::Jobs))), Tip::name("Builders").line("Open the jobs", ""), if fix == Fix::Assign { Small::Hint } else { Small::Idle });
    });
    let short = builders == 0 && waiting;
    u.row(14.0, |r| {
        r.stat(Icon::RoleBuilder, if short { BAD } else { INK }, format!("{builders}"), None, if short { BAD } else { INK }, Tip::name("Builders here"));
        r.stat(good_icon(Good::Sticks), WOOD, format!("{loads}"), Some("coming"), INK, Tip::name("Loads on the way"));
        r.stat(Icon::Site, MUTED, format!("{left}"), Some("to carry"), INK, Tip::name("Loads still to carry").line("Sticks", num(left as f32 * sim::dams::sticks_per_step())));
    });
}

fn build(u: &mut Ui, id: BuildingId, o: &Offers, fix: Fix) {
    u.section("Build", |_| {});
    let picked = !o.picks.is_empty();
    u.row(6.0, |r| {
        let rows: [(Icon, &str, DamGrowth, &Result<f32, String>, &str, Fix); 3] = [
            (Icon::Rise, "Raise", DamGrowth::Raise, &o.raise, if picked { "The picked cells, one step" } else { "The whole top, one step" }, Fix::Raise),
            (Icon::Widen, "Widen", DamGrowth::Widen, &o.widen, "A row thicker, so it can stand higher", Fix::Widen),
            (Icon::Extend, "Extend", DamGrowth::Extend, &o.extend, "Longer, to close the gap water goes round", Fix::Extend),
        ];
        for (icon, word, growth, plan, what, f) in rows {
            let (cost, action, tip) = match plan {
                Ok(c) => (sticks(*c), Some(HudAction::GrowDam(id, growth)), Tip::name(word).line(what, "").line("Sticks", num(*c))),
                Err(e) => (Some((good_icon(Good::Sticks), WOOD, "-".into())), None, Tip::name(word).line(what, "").line(e.clone(), "")),
            };
            r.big_button(BigButton { icon, word, cost, action, tip, hint: fix == f, on: false });
        }
    });
}

fn gates(c: &Ctx, u: &mut Ui, id: BuildingId, d: &Dam, o: &Offers) {
    let s = c.s;
    u.section("Gates", |r| works::picks(r, o.picks.len(), "dam"));
    u.row(6.0, |r| {
        let (cost, action, tip) = match &o.gate {
            Ok(c) => (sticks(*c), Some(HudAction::AddDamGate(id)), Tip::name("Gate").line("Let water through here", "").line("Sticks", num(*c))),
            Err(e) => (sticks(sim::canals::GATE_STICKS), None, Tip::name("Gate").line("Let water through here", "").line(e.clone(), "")),
        };
        r.big_button(BigButton { icon: Icon::Lock, word: "Gate", cost, action, tip, hint: false, on: false });
        let lock_sticks = sim::canals::GATE_STICKS + sim::locks::LOCK_STICKS;
        let what = Tip::name("Lock").line("A gate beavers and rafts get past", "").line("Sticks", num(lock_sticks)).line("Clay", num(sim::locks::LOCK_CLAY));
        let (action, tip) = match &o.lock {
            Ok(cell) => (Some(HudAction::UpgradeLock(id, *cell)), what),
            Err(e) => (None, what.line(e.clone(), "")),
        };
        r.big_button(BigButton { icon: Icon::LockGates, word: "Lock", cost: sticks(lock_sticks), action, tip, hint: false, on: false });
    });
    let pond = d.pond.as_ref().map(|p| p.level);
    for g in &d.gates {
        works::gate_row(u, s, id, g, pond, true, Ok(()));
    }
}

/// Face it with pebbles: then it wears more slowly.
fn face_button(s: &sim::SimState, r: &mut Ui, id: BuildingId) {
    let Some(b) = s.buildings.building(id) else { return };
    let slower = format!("{}× slower", num(sim::buildings::PEBBLE_DAM_FACTOR));
    if s.buildings.pebbled.contains(&id) {
        r.stat(Icon::Pebbles, GOOD, "Faced", None, GOOD, Tip::name("Pebble faced").line("Wears", slower));
    } else if s.maintenance.jobs.contains_key(&id) {
        r.small_button(Some(Icon::Pebbles), Some("Stop"), Some(HudAction::CancelMaintenance(id)), Tip::name("Facing with pebbles").line("Stop", "").line("Delivered pebbles come back", ""), Small::On);
    } else {
        let mut tip = Tip::name("Face with pebbles").line("Wears", slower);
        for (g, a) in sim::maintenance::cost(b, sim::maintenance::Refit::Pebbles).0 {
            tip.push(TipLine::new(good_name(g), num(a)).icon(good_icon(g), good_color(g)));
        }
        if b.is_complete() {
            r.small_button(Some(Icon::Pebbles), Some("Face"), Some(HudAction::PebbleBuilding(id)), tip, Small::Idle);
        } else {
            r.small_button(Some(Icon::Pebbles), Some("Face"), None, tip.line("Finish the dam first", ""), Small::Idle);
        }
    }
}

fn upkeep(c: &Ctx, u: &mut Ui, id: BuildingId, d: &Dam, fix: Fix) {
    u.section("Upkeep", |_| {});
    u.row(6.0, |r| {
        works::maintain_button(r, d.maintain, HudAction::DamMaintain(id, !d.maintain), "Maintainers repair worn cells", fix == Fix::Maintain);
        face_button(c.s, r, id);
        works::pause_button(r, c.s, id);
        r.spacer();
        works::remove_button(c, r, id);
    });
}

fn history(u: &mut Ui, d: &Dam) {
    if d.history.is_empty() {
        u.icon(Icon::Hourglass, 14.0, MUTED).tip(Tip::name("No days recorded yet"));
        return;
    }
    let low = d.history.iter().map(|h| h.level).fold(f32::INFINITY, f32::min);
    let high = d.history.iter().map(|h| h.level).fold(f32::NEG_INFINITY, f32::max).max(low + 0.05);
    let flow_top = d.history.iter().map(|h| h.inflow.max(h.outflow)).fold(0.0, f32::max).max(1e-3);
    let w = ((VIEW_W - 20.0) / sim::dams::HISTORY_DAYS as f32).floor().max(2.0);
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
    let s = c.s;
    let health = sim::dams::health(s, d);
    let o = offers(c, id, d);
    let (icon, text, col, why, fix) = status(s, id, d, &o);
    let selected_here = c.sel == Selection::Site(id) || matches!(c.sel, Selection::Cell(cell) if d.cells.iter().any(|x| x.cell == cell));
    let fix_button = match fix {
        Fix::None => None,
        Fix::Assign => Some(("Assign", HudAction::Colony(Some(ColonyTab::Jobs)), Tip::name("Builders").line("Open the jobs", ""))),
        Fix::Raise => Some(("Raise", HudAction::GrowDam(id, DamGrowth::Raise), Tip::name("Raise").line("Sticks", o.raise.as_ref().map(|c| num(*c)).unwrap_or_default()))),
        Fix::Widen => Some(("Widen", HudAction::GrowDam(id, DamGrowth::Widen), Tip::name("Widen").line("Sticks", o.widen.as_ref().map(|c| num(*c)).unwrap_or_default()))),
        Fix::Extend => Some(("Extend", HudAction::GrowDam(id, DamGrowth::Extend), Tip::name("Extend").line("Sticks", o.extend.as_ref().map(|c| num(*c)).unwrap_or_default()))),
        Fix::Maintain => Some(("Maintain", HudAction::DamMaintain(id, true), Tip::name("Maintain").line("Maintainers repair worn cells", ""))),
    };
    let tab = c.hud.dam_tab;
    let mut ui = Ui::default();
    ui.node(Node { width: Val::Px(RIGHT_W), flex_direction: FlexDirection::Column, row_gap: Val::Px(7.0), ..default() }, |u| {
        works::header(c, u, id, "Dam", selected_here, health, condition_tip(s, d, health), |p| portrait(p, health));
        u.status_line(icon, text, col, Tip::name(text).line(why, ""), fix_button);
        u.tabs(&[(Icon::Drop, "Now", HudAction::DamTab(DamTab::Status), tab == DamTab::Status), (Icon::Hourglass, "History", HudAction::DamTab(DamTab::History), tab == DamTab::History)]);
        match tab {
            DamTab::Status => {
                water(c, u, d);
                work(c, u, id, fix);
                build(u, id, &o, fix);
                gates(c, u, id, d, &o);
                upkeep(c, u, id, d, fix);
            }
            DamTab::History => history(u, d),
        }
    });
    let mut v = ui.nodes.pop().expect("panel node");
    panel(&mut v);
    v.node.padding = UiRect::all(Val::Px(12.0));
    v
}
