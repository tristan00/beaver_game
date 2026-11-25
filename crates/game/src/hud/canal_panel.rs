//! The panel a canal opens, laid out like the dam's (see `works`): the picture is the panel.
//! Its profile shows every cell from the intake, with the pond level, the depth a raft needs
//! and the cells too shallow for one, and the canal's state and its gate's chip drawn on it.
//! Under it Deepen, Widen, Extend, Gate and Lock, the gate's strip, and the footer (diggers,
//! Mend, Pause, Remove). History is behind the hourglass in the header.
use super::panels::{Ctx, RIGHT_W};
use super::ui::*;
use super::words::*;
use super::works::{self, cap, dashes, DEEP, EARTH, SHALLOW, VIEW_W, WOOD};
use super::{DamTab, HudAction};
use crate::flow::{Cell, Selection};
use crate::icons::Icon;
use bevy::prelude::*;
use sim::buildings::BuildingId;
use sim::canals::{Canal, CanalGrowth};
use sim::GateSetting;

const PROFILE_H: f32 = 64.0;
/// Room above the profile for the state, the gate's chip and two readings.
const HEAD: f32 = 46.0;
/// The pond's own column at the left of the profile.
const POND_W: f32 = 18.0;

/// A small painted view: a channel of water between two muddy banks.
fn portrait(p: &mut Ui, dry: bool) {
    p.node(Node { width: Val::Px(42.0), height: Val::Px(42.0), overflow: Overflow::clip(), ..default() }, |p| {
        p.add(VNode::new(works::abs_box(0.0, 0.0, 42.0, 42.0))).bg(EARTH);
        p.add(VNode::new(works::abs_box(0.0, 14.0, 42.0, 14.0))).bg(if dry { MUD } else { DEEP });
        p.add(VNode::new(works::abs_box(0.0, 12.0, 42.0, 2.0))).bg(MUD);
        p.add(VNode::new(works::abs_box(0.0, 28.0, 42.0, 2.0))).bg(MUD);
        if !dry {
            for x in [6.0, 20.0, 32.0] {
                p.add(VNode::new(works::abs_box(x, 20.0, 6.0, 1.5))).bg(SHALLOW.with_alpha(0.8));
            }
        }
    });
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Fix {
    None,
    Assign,
    Deepen,
    Upkeep,
}

/// What the canal is doing, in a word, its colour, why, and the one fix.
fn status(s: &sim::SimState, id: BuildingId, k: &Canal, deepen_ok: bool) -> (&'static str, Color, &'static str, Fix) {
    let waiting = sim::canals::has_work_for(s, id, false);
    let crew = sim::canals::crew(s, id);
    if waiting && crew == 0 {
        return ("No diggers", WARN, "Work is waiting and nobody is on it", Fix::Assign);
    }
    if waiting {
        return ("Digging", WARN, "Diggers are at work on it", Fix::None);
    }
    match k.status.as_ref() {
        None => ("No water yet", MUTED, "It fills once the intake is dug", Fix::None),
        Some(st) if st.wet == 0 && k.gate.built && k.gate.setting == GateSetting::Shut => ("Gate shut", MUTED, "No water gets past the gate", Fix::None),
        Some(st) if st.wet == 0 => ("Dry", WARN, "No water reaches it", Fix::None),
        Some(st) if st.floatable_share() < 0.5 => ("Too shallow", WARN, "Most cells are too shallow to float a load", if deepen_ok { Fix::Deepen } else { Fix::None }),
        Some(st) if st.silted > 0 && !k.upkeep => ("Silting", WARN, "Silt is filling it and nobody clears it", Fix::Upkeep),
        Some(_) => ("Flowing", GOOD, "Water runs through it", Fix::None),
    }
}

/// Every cell along the route from the intake: ground, bed, water, the depth a raft needs
/// (the short dashes) and, marked, the wet cells too shallow for one. The pond stands at the
/// left, its level dashed across.
fn profile(c: &Ctx, u: &mut Ui, id: BuildingId, k: &Canal, state: (&str, Color, Tip), picks: usize) {
    let s = c.s;
    let route: Vec<&sim::canals::CanalCell> = k.route().collect();
    if route.is_empty() {
        return;
    }
    let level = k.status.as_ref().map(|st| st.source_level);
    let n = route.len() as f32;
    let step = ((VIEW_W - POND_W - 4.0) / n).clamp(3.0, 16.0);
    let bar_w = (step - if step > 6.0 { 2.0 } else { 1.0 }).max(2.0);
    let ground = |cell: (usize, usize)| s.height.get(cell.0, cell.1).copied().unwrap_or(0.0);
    let depth = |cell: (usize, usize)| s.water.depth.get(cell.0, cell.1).copied().unwrap_or(0.0);
    let low = route.iter().map(|c| c.target.min(ground(c.cell))).fold(f32::INFINITY, f32::min);
    let high = route.iter().map(|c| c.ground.max(ground(c.cell) + depth(c.cell))).fold(f32::NEG_INFINITY, f32::max).max(level.unwrap_or(low)).max(low + 0.5);
    let px = (PROFILE_H - 6.0) / (high - low);
    let float = sim::canals::FLOAT_DEPTH * px;
    let working: Vec<_> = s.agents.by_id.values().filter_map(|a| match a.task {
        sim::agents::Task::Dig { site, cell, .. } | sim::agents::Task::Gate { site, cell, .. } if site == id => Some(cell),
        _ => None,
    }).collect();
    let total_h = PROFILE_H + 4.0 + HEAD;
    u.node(Node { width: Val::Px(VIEW_W), height: Val::Px(total_h), border_radius: BorderRadius::all(Val::Px(5.0)), overflow: Overflow::clip(), ..default() }, |w| {
        w.node(Node { position_type: PositionType::Absolute, left: Val::Px(0.0), bottom: Val::Px(0.0), flex_direction: FlexDirection::Row, align_items: AlignItems::FlexEnd, height: Val::Px(PROFILE_H + 4.0), ..default() }, |bars| {
            // The pond the canal draws from.
            bars.node(Node { width: Val::Px(POND_W), height: Val::Px(PROFILE_H + 4.0), flex_direction: FlexDirection::Column, justify_content: JustifyContent::FlexEnd, margin: UiRect::right(Val::Px(4.0)), ..default() }, |b| {
                if let Some(l) = level {
                    b.add(VNode::new(Node { width: Val::Px(POND_W), height: Val::Px(((l - low) * px).max(2.0)), ..default() })).bg(DEEP);
                }
            })
            .tip(Tip::name("The pond it draws from").line("Level", level.map_or("-".to_string(), |l| format!("{} m", num(l)))));
            for cell in &route {
                let g = ground(cell.cell);
                let d = depth(cell.cell);
                let silted = cell.done && g > cell.target + sim::canals::SILT_TOLERANCE;
                let picked = c.hud.dam_cells.contains(&cell.cell);
                let busy = working.contains(&cell.cell);
                let gate = cell.cell == k.gate.cell;
                let undug = (g - cell.target).max(0.0) * px;
                let water = (d * px).min(PROFILE_H);
                let base = ((cell.target.min(g) - low) * px).max(2.0);
                let shallow = d > 0.01 && d < sim::canals::FLOAT_DEPTH;
                let col = bars.node(Node { width: Val::Px(step), height: Val::Px(PROFILE_H + 4.0), flex_direction: FlexDirection::Column, justify_content: JustifyContent::FlexEnd, align_items: AlignItems::Center, ..default() }, |b| {
                    if busy && step >= 7.0 {
                        b.icon(Icon::Beaver, (step + 2.0).min(12.0), FUR);
                    }
                    if gate && k.gate.built {
                        let sill = (k.gate.sill() - g).max(0.0) * px;
                        if sill > 0.5 {
                            b.add(VNode::new(Node { width: Val::Px(bar_w), height: Val::Px(sill.min(PROFILE_H)), ..default() })).bg(WOOD);
                        }
                    }
                    if water > 0.5 {
                        b.add(VNode::new(Node { width: Val::Px(bar_w), height: Val::Px(water), ..default() })).bg(if shallow { SHALLOW.with_alpha(0.8) } else { WATER });
                    }
                    if undug > 0.5 {
                        let v = b.add(VNode::new(Node { width: Val::Px(bar_w), height: Val::Px(undug), border: UiRect::all(Val::Px(if silted || picked { 1.5 } else { 0.0 })), ..default() }));
                        v.bg = EARTH.with_alpha(0.95);
                        if silted || picked {
                            v.border = Some(BorderColor::all(if picked { SELECT } else { BAD }));
                        }
                    }
                    let bed = b.add(VNode::new(Node { width: Val::Px(bar_w), height: Val::Px(base), border: UiRect::top(Val::Px(1.5)), ..default() }));
                    bed.bg = MUD;
                    bed.border = Some(BorderColor::all(WOOD.with_alpha(0.7)));
                    // The depth a raft needs over this cell's floor, and a mark on water short of it.
                    if cell.done || d > 0.01 {
                        b.add(VNode::new(Node { position_type: PositionType::Absolute, bottom: Val::Px(base + undug + float), width: Val::Px(bar_w), height: Val::Px(1.0), ..default() })).bg(INK.with_alpha(0.6));
                    }
                    if shallow {
                        b.add(VNode::new(Node { position_type: PositionType::Absolute, bottom: Val::Px(base + undug + water), width: Val::Px(bar_w), height: Val::Px(3.0), border_radius: BorderRadius::all(Val::Px(1.0)), ..default() })).bg(ACCENT);
                    }
                });
                let mut tip = Tip::name(if gate { "Gate cell" } else { "Cell" }).line("Cut", format!("{} / {} m", num((cell.ground - g).max(0.0)), num(cell.ground - cell.target)));
                if d > 0.0 {
                    tip = tip.line("Water", format!("{} m", num(d)));
                }
                if shallow {
                    tip = tip.line("Too shallow for a raft", format!("{} m", num(sim::canals::FLOAT_DEPTH)));
                }
                if silted {
                    tip = tip.line("Silted", "");
                }
                if g < cell.ground - 0.001 {
                    let rate = sim::canals::silt_per_day(s, cell.cell);
                    tip = tip.line("Next silt in", days((sim::canals::SILT_STEP - cell.silt).max(0.0) / rate.max(1e-6)));
                }
                col.action(HudAction::DamCell(cell.cell)).tip(tip.line("Click", "pick").line("Shift", "add"));
            }
        });
        if let Some(l) = level {
            let top = total_h - (l - low) * px;
            dashes(w, POND_W, VIEW_W, top, WATER.with_alpha(0.7));
            let over = k.gate.built.then(|| l - k.gate.sill());
            let (text, tip) = match over {
                Some(o) if o > 0.0 => (format!("{:.1} m over sill", o), Tip::name("Over the gate").line("The pond above the gate's sill", "")),
                Some(_) => ("Below sill".to_string(), Tip::name("Below the gate").line("The pond is lower than the gate's sill: nothing gets in", "")),
                None => ("No gate".to_string(), Tip::name("No gate yet").line("Water runs in freely", "")),
            };
            w.node(works::abs_box(POND_W + 6.0, 26.0, 160.0, 16.0), |x| {
                x.bold(text, 12.0, INK).tip(tip);
            });
            if let Some(f) = sim::canals::end_drop(s, k) {
                works::label_left_of(w, VIEW_W - 6.0, 26.0, format!("{:.1} m drop", f), INK, Tip::name("Drop").line("From the pond down to the water at the far end", ""));
            }
        }
        w.node(Node { position_type: PositionType::Absolute, left: Val::Px(8.0), top: Val::Px(6.0), flex_direction: FlexDirection::Row, column_gap: Val::Px(6.0), align_items: AlignItems::Center, ..default() }, |r| {
            r.bold(state.0, 13.0, state.1).tip(state.2);
            if picks > 0 {
                let word = format!("{picks} picked");
                r.small_button(Some(Icon::Focus), Some(&word), Some(HudAction::ClearPicks), Tip::name("Unpick all"), Small::On).style(|n| n.height = Val::Px(22.0));
            }
        });
        if k.gate.built || k.gate.pending.is_some() {
            w.node(Node { position_type: PositionType::Absolute, right: Val::Px(6.0), top: Val::Px(5.0), ..default() }, |r| works::chips(c, r, id, std::slice::from_ref(&k.gate), 100.0));
        }
    })
    .bg(FAINT);
}

fn build(c: &Ctx, u: &mut Ui, id: BuildingId, k: &Canal, picks: &[Cell], fix: Fix) {
    let s = c.s;
    let picked = !picks.is_empty();
    u.row(6.0, |r| {
        for (icon, word, growth, what) in [
            (Icon::Down, "Deepen", CanalGrowth::Deepen, if picked { "The picked cells, a step deeper" } else { "A step deeper all along" }),
            (Icon::Widen, "Widen", CanalGrowth::Widen, if picked { "A row wider at the picked cells" } else { "A row wider all along" }),
        ] {
            let labour = sim::canals::growth_labour(s, id, growth, picks);
            let (cost, action, tip) = match &labour {
                Ok(d) => (Some((Icon::Beaver, FUR, format!("{}d", d.ceil().max(1.0) as i64))), Some(HudAction::GrowCanal(id, growth)), Tip::name(word).line(what, "").line("Beaver-days", num(*d))),
                Err(e) => (Some((Icon::Beaver, FUR, "-".into())), None, Tip::name(word).line(what, "").line(cap(e), "")),
            };
            r.big_button(BigButton { icon, word, cost, action, tip, hint: fix == Fix::Deepen && growth == CanalGrowth::Deepen, on: false });
        }
        let drawing = c.hud.extending == Some(id);
        r.big_button(BigButton { icon: Icon::Extend, word: "Extend", cost: None, action: Some(HudAction::GrowCanal(id, CanalGrowth::Extend)), tip: Tip::name("Extend").line("Drag on from the open end", "").line("Click", "add a corner"), hint: false, on: drawing });
        gate_buttons(s, r, id, k);
    });
}

/// The canal's one gate stands at its intake: Gate orders it, Lock makes it a lock. Once
/// ordered, the button stays lit and takes the camera there.
fn gate_buttons(s: &sim::SimState, r: &mut Ui, id: BuildingId, k: &Canal) {
    let g = &k.gate;
    let works = sim::content::content();
    let gate_tip = cost_lines(Tip::name("Gate").line("Sets how much water gets in", ""), &works.works.gate);
    let gate_there = g.built || g.pending.is_some();
    let (action, tip) = if gate_there {
        (Some(HudAction::GateChip(id, g.cell)), gate_tip.line("Ordered", ""))
    } else if !sim::canals::gates_unlocked(s) {
        (None, gate_tip.line("Needs a haul mound", ""))
    } else {
        (Some(HudAction::SetGate(id, g.cell, GateSetting::Shut)), gate_tip)
    };
    r.big_button(BigButton { icon: Icon::Lock, word: "Gate", cost: cost_chip(&works.works.gate), action, tip, hint: false, on: gate_there });
    let lock_there = sim::locks::is_lock(s, g.cell) || sim::locks::pending(s, g.cell);
    let lock_cost = if gate_there { works.works.lock.clone() } else { cost_sum(&works.works.gate, &works.works.lock) };
    let lock_tip = cost_lines(Tip::name("Lock").line("A gate beavers and rafts get past", ""), &lock_cost);
    let (action, tip) = if lock_there {
        (Some(HudAction::GateChip(id, g.cell)), lock_tip.line("Ordered", ""))
    } else if !sim::locks::unlocked(s) {
        (None, lock_tip.line("Needs a raft landing", ""))
    } else if !gate_there && !sim::canals::gates_unlocked(s) {
        (None, lock_tip.line("Needs a haul mound", ""))
    } else if sim::locks::check_room(s, id, g.cell).is_err() {
        (None, lock_tip.line("No room for a lock", ""))
    } else {
        (Some(HudAction::UpgradeLock(id, g.cell)), lock_tip)
    };
    r.big_button(BigButton { icon: Icon::LockGates, word: "Lock", cost: cost_chip(&lock_cost), action, tip, hint: false, on: lock_there });
}

fn footer(c: &Ctx, u: &mut Ui, id: BuildingId, k: &Canal, fix: Fix) {
    let crew = sim::canals::crew(c.s, id);
    u.row(6.0, |r| {
        works::crew_button(r, crew, Icon::Beaver, "Diggers here", fix == Fix::Assign);
        works::upkeep_button(r, k.upkeep, HudAction::CanalUpkeep(id, !k.upkeep), "Menders clear silt", fix == Fix::Upkeep);
        works::pause_button(r, c.s, id);
        r.spacer();
        works::remove_button(c, r, id);
    });
}

fn history(u: &mut Ui, k: &Canal) {
    if k.history.is_empty() {
        u.icon(Icon::Hourglass, 14.0, MUTED).tip(Tip::name("No days recorded yet"));
        return;
    }
    let flow_top = k.history.iter().map(|h| h.inflow.max(h.outflow)).fold(0.0, f32::max).max(1e-3);
    let silt_top = k.history.iter().map(|h| h.silted).fold(0.0, f32::max).max(1.0);
    let w = ((VIEW_W - 20.0) / sim::canals::HISTORY_DAYS as f32).floor().max(2.0);
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
    chart(u, Icon::Raft, WATER, "Floats a raft", &|h| h.floatable);
    chart(u, Icon::Upstream, SHALLOW, "Water in", &|h| h.inflow / flow_top);
    chart(u, Icon::Push, WARN, "Water out", &|h| h.outflow / flow_top);
    chart(u, Icon::Down, BAD, "Silted cells", &|h| h.silted / silt_top);
}

pub fn canal_panel(c: &Ctx, id: BuildingId, k: &Canal) -> VNode {
    let s = c.s;
    let health = sim::canals::health(s, k);
    let dry = k.status.as_ref().is_none_or(|st| st.wet == 0);
    let picks: Vec<Cell> = c.hud.dam_cells.iter().copied().filter(|p| k.cells.iter().any(|x| x.cell == *p)).collect();
    let deepen_ok = sim::canals::growth_labour(s, id, CanalGrowth::Deepen, &picks).is_ok();
    let (word, col, why, fix) = status(s, id, k, deepen_ok);
    let mut state_tip = Tip::name(word).line(why, "");
    if let Some(st) = k.status.as_ref() {
        state_tip = state_tip.line("Floats a raft", format!("{} / {} cells", st.floatable, st.cells)).line("Water in", format!("{}/d", num(st.inflow))).line("Water out", format!("{}/d", num(st.outflow)));
    }
    let silted = k.status.as_ref().map_or(0, |st| st.silted);
    let mut ui = Ui::default();
    ui.node(Node { width: Val::Px(RIGHT_W), flex_direction: FlexDirection::Column, row_gap: Val::Px(8.0), ..default() }, |u| {
        works::header(c, u, id, sim::buildings::BuildingKind::canal().name(), c.sel == Selection::Site(id), health, Tip::name("Condition").with_value(format!("{:.0}%", health * 100.0)).line("Silted cells", format!("{silted}")), |p| portrait(p, dry));
        if c.hud.dam_tab == DamTab::History {
            history(u, k);
            return;
        }
        profile(c, u, id, k, (word, col, state_tip), picks.len());
        build(c, u, id, k, &picks, fix);
        if k.gate.built || k.gate.pending.is_some() {
            let usable = if k.gate.built || sim::canals::gates_unlocked(s) { Ok(()) } else { Err("Needs a haul mound") };
            works::gate_row(u, s, id, &k.gate, k.status.as_ref().map(|st| st.source_level), false, usable);
        }
        footer(c, u, id, k, fix);
    });
    let mut v = ui.nodes.pop().expect("panel node");
    panel(&mut v);
    v.node.padding = UiRect::all(Val::Px(12.0));
    v
}
