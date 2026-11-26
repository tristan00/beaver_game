//! The panel a dam opens: the picture is the panel. A side cut through the dam shows the pond,
//! the mound, how far the pond is from spilling and the drop below, with the dam's state and a
//! chip for each gate and lock drawn on it. Under it the build buttons (Raise, Widen, Extend,
//! Gate, Lock), the strip of the gate picked, and the footer (builders, Mend, Face, Pause,
//! Remove). History is behind the hourglass in the header. Detail lives in tooltips.
use super::panels::Ctx;
use super::ui::*;
use super::words::*;
use super::works::{self, abs_box, cap, dashes, label_left_of, DEEP, EARTH, SHALLOW, VIEW_W, WOOD};
use super::{DamTab, HudAction, Opening};
use crate::flow::{Cell, Selection};
use crate::icons::Icon;
use bevy::prelude::*;
use sim::buildings::BuildingId;
use sim::dams::{Dam, Spill};
use sim::DamGrowth;

const VIEW_H: f32 = 124.0;
/// Room over the picture for the state and the chips.
const HEAD: f32 = 34.0;
const FLOOR: f32 = 8.0;
/// Where the mound stands across the picture.
const MOUND_X: f32 = 118.0;
const MOUND_W: f32 = 32.0;

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

/// What can be done to the dam now, worked out once for the state and the buttons.
struct Offers {
    picks: Vec<Cell>,
    raise: Result<f32, String>,
    widen: Result<f32, String>,
    extend: Result<f32, String>,
}

fn offers(c: &Ctx, id: BuildingId, d: &Dam) -> Offers {
    let s = c.s;
    let picks: Vec<Cell> = c.hud.dam_cells.iter().copied().filter(|p| d.cells.iter().any(|x| x.cell == *p)).collect();
    let cost = |plan: Result<sim::dams::Plan, String>| plan.map(|p| sim::dams::plan_cost(s, d, &p)).map_err(|e| cap(&e));
    let around = d.pond.as_ref().is_some_and(|p| matches!(p.spill, Spill::Around(_)));
    Offers {
        raise: cost(sim::dams::plan_raise(s, id, &picks)),
        widen: cost(sim::dams::plan_widen(s, id)),
        extend: if around { cost(sim::dams::plan_extend(s, id)) } else { Err("Water isn't getting round the ends".into()) },
        picks,
    }
}

/// The button the state points at, marked with the "!".
#[derive(Clone, Copy, PartialEq, Eq)]
enum Fix {
    None,
    Assign,
    Raise,
    Widen,
    Extend,
    Upkeep,
}

/// What the dam as a whole is doing, in a word, its colour, why, and the one fix.
fn status(s: &sim::SimState, id: BuildingId, d: &Dam, o: &Offers) -> (&'static str, Color, &'static str, Fix) {
    let builders = sim::agents::builders_on(s, id);
    let building = sim::dams::has_work_for(s, id, false);
    if building && builders == 0 {
        return ("No builders", WARN, "Work is waiting and no builder is on it", Fix::Assign);
    }
    if building {
        return ("Building", WARN, "Builders are at work on it", Fix::None);
    }
    let worn = sim::dams::health(s, d) < sim::dams::REPAIR_BELOW && !d.upkeep;
    match d.pond.as_ref().map(|p| (p.spill, p.days_to_full)) {
        None => ("No pond yet", MUTED, "The finished cells will start holding water", Fix::None),
        Some((Spill::Over(_), _)) => {
            let fix = if o.raise.is_ok() { Fix::Raise } else if o.widen.is_ok() { Fix::Widen } else { Fix::None };
            ("Spilling", WARN, "Water runs over the top and wears it", fix)
        }
        Some((Spill::Around(_), _)) => ("Going round", WARN, "Water gets round an end", if o.extend.is_ok() { Fix::Extend } else { Fix::None }),
        _ if worn => ("Wearing", WARN, "Cells are worn and nobody keeps it up", Fix::Upkeep),
        Some((_, days)) if days > 0.0 => ("Filling", WATER, "The pond is still rising", Fix::None),
        Some((Spill::Gate(_), _)) => ("Holding", GOOD, "Full, letting water out by its gate", Fix::None),
        Some(_) => ("Holding", GOOD, "Full", Fix::None),
    }
}

fn sticks(c: f32) -> Option<(Icon, Color, String)> {
    cost_chip(&[(sim::dams::material(), c)])
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

/// What the side cut goes through.
enum Through {
    /// A cell a gate or lock is being placed on: whether it fits there.
    Placing(Result<(), String>),
    /// The gate picked.
    Gate(sim::gates::Gate),
    /// The lowest finished cell of the crest: where the dam would spill first.
    Lowest,
}

/// The cell the side cut is drawn through: the one being placed on, else the gate picked, else
/// the lowest finished cell of the crest.
fn through(c: &Ctx, id: BuildingId, d: &Dam) -> Option<(Cell, Through)> {
    let s = c.s;
    let on_line = |cell: Cell| d.line().any(|x| x.cell == cell);
    if let (Some((dam, o)), Some(cell)) = (c.hud.opening, c.works_cell) {
        if dam == id && on_line(cell) {
            return Some((cell, Through::Placing(works::opening_fits(s, id, cell, o))));
        }
    }
    if let Some((_, cell)) = c.hud.gate_sel.filter(|g| g.0 == id) {
        if let Some(g) = d.gates.iter().find(|g| g.cell == cell) {
            return Some((cell, Through::Gate(*g)));
        }
    }
    let top = |cell: Cell| s.height.get(cell.0, cell.1).copied().unwrap_or(0.0) + sim::dams::built(s, cell);
    let lowest = d.line().filter(|x| !sim::dams::is_gate(d, x.cell) && sim::dams::built(s, x.cell) > 0.0).min_by(|a, b| top(a.cell).total_cmp(&top(b.cell))).map(|x| x.cell);
    let cell = lowest.or_else(|| d.line().nth(d.line().count() / 2).map(|x| x.cell))?;
    Some((cell, Through::Lowest))
}

/// The side cut: water behind the dam, the dam at one of its cells, the water below; how much
/// more the pond can rise there before it spills, and how far it stands above the water below.
/// The state is written over it, and a chip for each gate and lock.
fn side_cut(c: &Ctx, u: &mut Ui, id: BuildingId, d: &Dam, state: (&str, Color, &str), picks: usize) {
    let s = c.s;
    let Some((cell, what)) = through(c, id, d) else { return };
    let ground = s.height.get(cell.0, cell.1).copied().unwrap_or(0.0);
    let gate = match &what {
        Through::Gate(g) if g.built => Some(*g),
        _ => None,
    };
    // A built gate's cell stands at its sill; the mound either side stands at the gate's top.
    let crest = gate.map_or(ground + sim::dams::built(s, cell), |g| g.top);
    let pond = d.pond.as_ref();
    let level = pond.map(|p| p.level);
    let fall = sim::dams::drop(s, d);
    let below = match (level, fall) {
        (Some(l), Some(f)) => Some(l - f),
        _ => None,
    };
    let bottom = pond.map_or(ground, |p| p.bottom);
    let low = bottom.min(below.unwrap_or(bottom)).min(ground) - 0.1;
    let high = crest.max(level.unwrap_or(crest)).max(low + 0.3);
    let px = (VIEW_H - HEAD - FLOOR) / (high - low);
    let y = |v: f32| HEAD + (high - v) * px;
    let earth = y(low);
    let pond_tip = pond.map(|p| {
        let mut t = Tip::name(sim::names::pond_name(s, id).unwrap_or("Pond")).line("Depth", format!("{} m", num(p.level - p.bottom))).line("Full at", format!("{} m", num(p.full_level - p.bottom))).line("Area", format!("{} cells", p.area));
        if p.days_to_full > 0.0 {
            t = t.line("Full in", days(p.days_to_full));
        }
        let out = match p.spill {
            Spill::Over(_) => "over the top",
            Spill::Around(_) => "round an end",
            Spill::Gate(_) => "by its gate",
            Spill::None => "seeping",
        };
        t.line("Water in", format!("{}/d", num(p.inflow))).line("Water out", format!("{}/d {out}", num(p.outflow)))
    });
    u.node(Node { width: Val::Px(VIEW_W), height: Val::Px(VIEW_H), overflow: Overflow::clip(), border_radius: BorderRadius::all(Val::Px(5.0)), ..default() }, |v| {
        v.add(VNode::new(abs_box(0.0, earth, VIEW_W, VIEW_H - earth))).bg(EARTH);
        if let (Some(l), Some(tip)) = (level, pond_tip) {
            v.add(VNode::new(abs_box(0.0, y(l), MOUND_X, earth - y(l)))).bg(DEEP).tip(tip);
        }
        if let Some(b) = below {
            v.add(VNode::new(abs_box(MOUND_X + MOUND_W, y(b), VIEW_W - MOUND_X - MOUND_W, earth - y(b)))).bg(SHALLOW.with_alpha(0.8));
        }
        let worn = sim::dams::built(s, cell) > 0.0 && sim::dams::integrity(s, cell) < sim::dams::REPAIR_BELOW;
        if crest > ground + 0.01 {
            let wall = v.add(VNode::new(Node { border_radius: BorderRadius::top(Val::Px(4.0)), ..abs_box(MOUND_X, y(crest), MOUND_W, earth - y(crest)) }));
            wall.bg = if worn { BAD.with_alpha(0.7) } else { MUD };
        }
        // A gate: the gap down to its sill, its board, and the water going through.
        if let Some(g) = gate {
            let sill = g.sill();
            v.add(VNode::new(abs_box(MOUND_X + 8.0, y(crest), MOUND_W - 16.0, y(sill) - y(crest)))).bg(FAINT);
            v.add(VNode::new(abs_box(MOUND_X + 6.0, y(sill) - 2.0, MOUND_W - 12.0, 3.0))).bg(WOOD);
            if level.is_some_and(|l| l > sill + 0.02) {
                v.add(VNode::new(abs_box(MOUND_X + MOUND_W - 8.0, y(sill) - 1.0, 4.0, below.map_or(earth, y) - y(sill) + 1.0))).bg(WATER.with_alpha(0.8));
            }
        }
        // Placing: the gate it would be, outlined; or why it can't go here.
        if let Through::Placing(fit) = &what {
            let col = if fit.is_ok() { SELECT } else { BAD };
            let outline = v.add(VNode::new(Node { border: UiRect::all(Val::Px(1.5)), ..abs_box(MOUND_X + 6.0, y(crest), MOUND_W - 12.0, earth - y(crest)) }));
            outline.border = Some(BorderColor::all(col));
            if let Err(why) = fit {
                v.node(abs_box(6.0, earth - 18.0, MOUND_X + 60.0, 16.0), |l| {
                    l.bold(why.clone(), 12.0, BAD);
                });
            }
        }
        let spilling = level.is_some_and(|l| gate.is_none() && crest - l <= 0.02);
        if spilling {
            v.add(VNode::new(abs_box(MOUND_X - 2.0, y(crest) - 3.0, MOUND_W + 4.0, 3.0))).bg(WATER);
            v.add(VNode::new(abs_box(MOUND_X + MOUND_W, y(crest) - 1.0, 4.0, below.map_or(earth, y) - y(crest) + 1.0))).bg(WATER.with_alpha(0.8));
        }
        if let Some(l) = level {
            let top_y = (y(crest).min(y(l)) - 18.0).max(HEAD - 2.0);
            match gate {
                Some(g) => {
                    let over = l - g.sill();
                    let (text, tip) = if over > 0.0 { (format!("{:.1} m over sill", over), Tip::name("Over the sill").line("Water goes through", "")) } else { ("Below sill".to_string(), Tip::name("Below the sill").line("Nothing goes through", "")) };
                    label_left_of(v, MOUND_X - 6.0, top_y, text, INK, tip);
                }
                None if spilling => label_left_of(v, MOUND_X - 6.0, top_y, "0 m to spill".into(), WARN, Tip::name("Spilling").line("Water runs over the top here", "")),
                None => {
                    v.add(VNode::new(abs_box(MOUND_X - 8.0, y(crest), 1.5, y(l) - y(crest)))).bg(INK);
                    v.add(VNode::new(abs_box(MOUND_X - 12.0, y(crest), 9.0, 1.5))).bg(INK);
                    let tip = Tip::name("Room to rise").line(if matches!(what, Through::Lowest) { "At the lowest part of the top" } else { "At this cell" }, "");
                    label_left_of(v, MOUND_X - 14.0, top_y, format!("{:.1} m to spill", crest - l), INK, tip);
                }
            }
            if let (Some(f), Some(b)) = (fall, below) {
                dashes(v, MOUND_X + MOUND_W + 2.0, VIEW_W - 30.0, y(l), WATER.with_alpha(0.7));
                if f > 0.02 {
                    let x = VIEW_W - 26.0;
                    v.add(VNode::new(abs_box(x, y(l), 1.5, y(b) - y(l)))).bg(INK);
                    v.add(VNode::new(abs_box(x - 4.0, y(l), 9.0, 1.5))).bg(INK);
                    v.add(VNode::new(abs_box(x - 4.0, y(b) - 1.5, 9.0, 1.5))).bg(INK);
                    label_left_of(v, x - 6.0, ((y(l) + y(b)) / 2.0 - 8.0).max(HEAD), format!("{:.1} m drop", f), INK, Tip::name("Drop").line("From the pond down to the water below", ""));
                }
            }
        }
        // The state, and the picked cells if any.
        v.node(Node { position_type: PositionType::Absolute, left: Val::Px(8.0), top: Val::Px(6.0), flex_direction: FlexDirection::Row, column_gap: Val::Px(6.0), align_items: AlignItems::Center, ..default() }, |r| {
            r.bold(state.0, 13.0, state.1).tip(Tip::name(state.0).line(state.2, ""));
            if picks > 0 {
                let word = format!("{picks} picked");
                r.small_button(Some(Icon::Focus), Some(&word), Some(HudAction::ClearPicks), Tip::name("Unpick all"), Small::On).style(|n| n.height = Val::Px(22.0));
            }
        });
        if !d.gates.is_empty() {
            let room = VIEW_W - if picks > 0 { 200.0 } else { 110.0 };
            v.node(Node { position_type: PositionType::Absolute, right: Val::Px(6.0), top: Val::Px(5.0), ..default() }, |r| works::chips(c, r, id, &d.gates, room));
        }
    })
    .bg(FAINT);
}

fn build(c: &Ctx, u: &mut Ui, id: BuildingId, d: &Dam, o: &Offers, fix: Fix) {
    let picked = !o.picks.is_empty();
    u.row(6.0, |r| {
        let rows: [(Icon, &str, DamGrowth, &Result<f32, String>, &str, Fix); 3] = [
            (Icon::Rise, "Raise", DamGrowth::Raise, &o.raise, if picked { "The picked cells, one step" } else { "The whole top, one step" }, Fix::Raise),
            (Icon::Widen, "Widen", DamGrowth::Widen, &o.widen, "A row thicker, so it can stand higher", Fix::Widen),
            (Icon::Extend, "Extend", DamGrowth::Extend, &o.extend, "Longer, to close the gap water goes round", Fix::Extend),
        ];
        for (icon, word, growth, plan, what, f) in rows {
            let (cost, action, tip) = match plan {
                Ok(c) => (sticks(*c), Some(HudAction::GrowDam(id, growth)), Tip::name(word).line(what, "").line(sim::dams::material().name(), num(*c))),
                Err(e) => (Some((good_icon(sim::dams::material()), good_color(sim::dams::material()), "-".into())), None, Tip::name(word).line(what, "").line(e.clone(), "")),
            };
            r.big_button(BigButton { icon, word, cost, action, tip, hint: fix == f, on: false });
        }
        works::opening_button(c, r, id, d, Opening::Gate);
        works::opening_button(c, r, id, d, Opening::Lock);
    });
}

/// Face it with pebbles: then it wears more slowly.
fn face_button(s: &sim::SimState, r: &mut Ui, id: BuildingId) {
    let Some(b) = s.buildings.building(id) else { return };
    let slower = format!("{}× slower", num(sim::maintenance::lasts(sim::buildings::BuildingKind::dam(), sim::maintenance::Refit::Facing)));
    if b.pebbled {
        r.stat(Icon::Pebbles, GOOD, "Faced", None, GOOD, Tip::name("Pebble faced").line("Wears", slower));
    } else if s.maintenance.jobs.get(&id).is_some_and(|j| j.kind == sim::maintenance::Refit::Facing) {
        r.small_button(Some(Icon::Pebbles), Some("Stop"), Some(HudAction::Refit(id, None)), Tip::name("Facing with pebbles").line("Stop", "").line("Delivered pebbles come back", ""), Small::On);
    } else {
        let mut tip = Tip::name("Face with pebbles").line("Wears", slower);
        for (g, a) in sim::maintenance::cost(b, sim::maintenance::Refit::Facing).0 {
            tip.push(TipLine::new(g.name(), num(a)).icon(good_icon(g), good_color(g)));
        }
        if b.is_complete() {
            r.small_button(Some(Icon::Pebbles), Some("Face"), Some(HudAction::Refit(id, Some(sim::maintenance::Refit::Facing))), tip, Small::Idle);
        } else {
            r.small_button(Some(Icon::Pebbles), Some("Face"), None, tip.line("Finish the dam first", ""), Small::Idle);
        }
    }
}

fn footer(c: &Ctx, u: &mut Ui, id: BuildingId, d: &Dam, fix: Fix) {
    let builders = sim::agents::builders_on(c.s, id);
    u.row(6.0, |r| {
        works::crew_button(r, builders, crate::hud::words::job_icon(sim::jobs::Job::with_duty(sim::content::Duty::Build)), "Builders here", fix == Fix::Assign);
        works::upkeep_button(r, d.upkeep, HudAction::Upkeep(id, !d.upkeep), "Menders repair worn cells", fix == Fix::Upkeep);
        face_button(c.s, r, id);
        works::pause_button(r, c.s, id);
        r.spacer();
        super::costs::page_button(c, r, id);
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
    let (n, max) = (d.history.len(), sim::dams::HISTORY_DAYS);
    works::history_row(u, Icon::Gauge, WATER, "Pond level", n, max, d.history.iter().map(|h| (h.level - low) / (high - low)));
    works::history_row(u, Icon::Upstream, SHALLOW, "Water in", n, max, d.history.iter().map(|h| h.inflow / flow_top));
    works::history_row(u, Icon::Push, WARN, "Water out", n, max, d.history.iter().map(|h| h.outflow / flow_top));
}

pub fn dam_panel(c: &Ctx, id: BuildingId, d: &Dam) -> VNode {
    let s = c.s;
    let health = sim::dams::health(s, d);
    let o = offers(c, id, d);
    let (word, col, why, fix) = status(s, id, d, &o);
    let selected_here = c.sel == Selection::Site(id) || matches!(c.sel, Selection::Cell(cell) if d.cells.iter().any(|x| x.cell == cell));
    let title = sim::names::pond_name(s, id).unwrap_or(sim::buildings::BuildingKind::dam().name());
    works::works_panel(c, id, title, selected_here, health, condition_tip(s, d, health), |p| portrait(p, health), |u| {
        if c.hud.dam_tab == DamTab::History {
            history(u, d);
            return;
        }
        side_cut(c, u, id, d, (word, col, why), o.picks.len());
        build(c, u, id, d, &o, fix);
        if let Some(g) = c.hud.gate_sel.filter(|g| g.0 == id).and_then(|(_, cell)| d.gates.iter().find(|g| g.cell == cell)) {
            works::gate_row(u, s, id, g, d.pond.as_ref().map(|p| p.level), true, Ok(()));
        }
        super::costs::upkeep_row(c, u, id);
        footer(c, u, id, d, fix);
    })
}
