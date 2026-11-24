//! The panel a canal opens. Laid out like the dam's (see `works`): header, what it is doing
//! with the one fix, then Water (every cell from the intake, with the pond level, the depth a
//! raft needs and the cells too shallow for one), Work, Dig, Gate and Upkeep.
use super::panels::{Ctx, RIGHT_W};
use super::ui::*;
use super::words::*;
use super::works::{self, cap, dashes, DEEP, EARTH, SHALLOW, VIEW_W, WOOD};
use super::{ColonyTab, DamTab, HudAction};
use crate::flow::{Cell, Selection};
use crate::icons::Icon;
use bevy::prelude::*;
use sim::buildings::{BuildingId, Good};
use sim::canals::{Canal, CanalGrowth};
use sim::GateSetting;

const PROFILE_H: f32 = 64.0;
/// Room above the profile for its two readings.
const HEAD: f32 = 18.0;
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
    OpenGate,
    Maintain,
}

/// What the canal is doing, its colour, why, and the one fix.
fn status(s: &sim::SimState, id: BuildingId, k: &Canal, deepen_ok: bool) -> (Icon, &'static str, Color, &'static str, Fix) {
    let crew = sim::canals::crew(s, id);
    let waiting = sim::canals::has_work_for(s, id, false);
    if waiting && crew == 0 {
        return (Icon::RoleBuilder, "No diggers", BAD, "Work is waiting and nobody is on it", Fix::Assign);
    }
    if waiting {
        return (Icon::Hammer, "Digging", WARN, "Diggers are at work on it", Fix::None);
    }
    match k.status.as_ref() {
        None => (Icon::Hourglass, "No water yet", MUTED, "It fills once the intake is dug", Fix::None),
        Some(st) if st.wet == 0 && k.gate.built && k.gate.setting == GateSetting::Shut => (Icon::Lock, "Gate shut", MUTED, "No water passes the gate", Fix::OpenGate),
        Some(st) if st.wet == 0 => (Icon::Drop, "Dry", WARN, "No water reaches it", Fix::None),
        Some(st) if st.floatable_share() < 0.5 => (Icon::Raft, "Too shallow for rafts", WARN, "Most cells are too shallow to float a load", if deepen_ok { Fix::Deepen } else { Fix::None }),
        Some(st) if st.silted > 0 && !k.maintain => (Icon::Down, "Silting", WARN, "Silt is filling it and nobody clears it", Fix::Maintain),
        Some(_) => (Icon::Check, "Flowing", GOOD, "Water runs through it", Fix::None),
    }
}

/// Every cell along the route from the intake: ground, bed, water, the depth a raft needs
/// (the short dashes) and, marked, the wet cells too shallow for one. The pond stands at the
/// left, its level dashed across.
fn profile(c: &Ctx, u: &mut Ui, id: BuildingId, k: &Canal) {
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
            w.node(works::abs_box(POND_W + 6.0, 1.0, 160.0, 16.0), |x| {
                x.bold(text, 12.0, INK).tip(tip);
            });
            if let Some(f) = sim::canals::end_drop(s, k) {
                works::label_left_of(w, VIEW_W - 6.0, 1.0, format!("{:.1} m drop", f), INK, Tip::name("Drop").line("From the pond down to the water at the far end", ""));
            }
        }
    })
    .bg(FAINT);
}

fn water(c: &Ctx, u: &mut Ui, id: BuildingId, k: &Canal) {
    u.section("Water", |_| {});
    profile(c, u, id, k);
    let Some(st) = k.status.as_ref() else {
        u.icon(Icon::Hourglass, 15.0, MUTED).tip(Tip::name("No water yet").line("It fills once the intake is dug", ""));
        return;
    };
    let share = st.floatable_share();
    u.row(14.0, |r| {
        r.stat(Icon::Raft, WATER, format!("{:.0}%", share * 100.0), Some("floats"), if share >= 0.5 { INK } else { WARN }, Tip::name("Floats a raft").line("Cells deep enough", format!("{} / {}", st.floatable, st.cells)).line("Wet", format!("{}", st.wet)).line("Dug", format!("{}", st.dug)));
        if st.inflow > 0.0 || st.outflow > 0.0 {
            r.stat(Icon::Upstream, WATER, format!("{}/d", num(st.inflow)), None, INK, Tip::name("Water in").line("At the gate, per day", ""));
            r.stat(Icon::Push, WATER, format!("{}/d", num(st.outflow)), None, INK, Tip::name("Water out").line("At the far end, per day", ""));
        }
        r.spacer();
        r.stat(Icon::Extend, MUTED, format!("{}", k.route().count()), None, INK, Tip::name("Length").line("Cells", ""));
    });
}

fn work(c: &Ctx, u: &mut Ui, id: BuildingId, k: &Canal, fix: Fix) {
    let s = c.s;
    let crew = sim::canals::crew(s, id);
    let left = sim::canals::visits_left(s, id);
    let waiting = sim::canals::has_work_for(s, id, false);
    let clay: f32 = s.resources.items().iter().filter(|r| r.kind == sim::resources::ResourceKind::MudBank && k.cells.iter().any(|x| x.cell.0.abs_diff(r.cell.0) <= 1 && x.cell.1.abs_diff(r.cell.1) <= 1)).map(|r| r.amount).sum();
    u.section("Work", |r| {
        r.small_button(Some(Icon::Jobs), Some("Assign"), Some(HudAction::Colony(Some(ColonyTab::Jobs))), Tip::name("Diggers").line("Open the jobs", ""), if fix == Fix::Assign { Small::Hint } else { Small::Idle });
    });
    let short = crew == 0 && waiting;
    u.row(14.0, |r| {
        r.stat(Icon::Beaver, if short { BAD } else { FUR }, format!("{crew}"), None, if short { BAD } else { INK }, Tip::name("Diggers here"));
        r.stat(Icon::Site, MUTED, format!("{left}"), Some("to dig"), INK, Tip::name("Visits still to make").line("Beaver-days", num(sim::canals::dig_days(left))));
        r.stat(good_icon(Good::Clay), good_color(Good::Clay), format!("{}", clay.round() as i64), Some("clay"), INK, Tip::name("Clay on the banks").line("Gatherers bring it in", ""));
    });
}

fn dig(c: &Ctx, u: &mut Ui, id: BuildingId, picks: &[Cell], fix: Fix) {
    let s = c.s;
    u.section("Dig", |r| works::picks(r, picks.len(), "canal"));
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
    });
}

fn gate(c: &Ctx, u: &mut Ui, id: BuildingId, k: &Canal) {
    let s = c.s;
    u.section("Gate", |_| {});
    let usable = if k.gate.built || sim::canals::gates_unlocked(s) { Ok(()) } else { Err("Needs Logistics") };
    works::gate_row(u, s, id, &k.gate, k.status.as_ref().map(|st| st.source_level), false, usable);
}

fn upkeep(c: &Ctx, u: &mut Ui, id: BuildingId, k: &Canal, fix: Fix) {
    u.section("Upkeep", |_| {});
    u.row(6.0, |r| {
        works::maintain_button(r, k.maintain, HudAction::CanalMaintain(id, !k.maintain), "Maintainers clear silt", fix == Fix::Maintain);
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
    let (icon, text, col, why, fix) = status(s, id, k, deepen_ok);
    let fix_button = match fix {
        Fix::None => None,
        Fix::Assign => Some(("Assign", HudAction::Colony(Some(ColonyTab::Jobs)), Tip::name("Diggers").line("Open the jobs", ""))),
        Fix::Deepen => Some(("Deepen", HudAction::GrowCanal(id, CanalGrowth::Deepen), Tip::name("Deepen").line("A step deeper", ""))),
        Fix::OpenGate => Some(("Open", HudAction::SetGate(id, k.gate.cell, GateSetting::Open), Tip::name("Open the gate"))),
        Fix::Maintain => Some(("Maintain", HudAction::CanalMaintain(id, true), Tip::name("Maintain").line("Maintainers clear silt", ""))),
    };
    let silted = k.status.as_ref().map_or(0, |st| st.silted);
    let tab = c.hud.dam_tab;
    let mut ui = Ui::default();
    ui.node(Node { width: Val::Px(RIGHT_W), flex_direction: FlexDirection::Column, row_gap: Val::Px(7.0), ..default() }, |u| {
        works::header(c, u, id, "Canal", c.sel == Selection::Site(id), health, Tip::name("Condition").with_value(format!("{:.0}%", health * 100.0)).line("Silted cells", format!("{silted}")), |p| portrait(p, dry));
        u.status_line(icon, text, col, Tip::name(text).line(why, ""), fix_button);
        u.tabs(&[(Icon::Drop, "Now", HudAction::DamTab(DamTab::Status), tab == DamTab::Status), (Icon::Hourglass, "History", HudAction::DamTab(DamTab::History), tab == DamTab::History)]);
        match tab {
            DamTab::Status => {
                water(c, u, id, k);
                work(c, u, id, k, fix);
                dig(c, u, id, &picks, fix);
                gate(c, u, id, k);
                upkeep(c, u, id, k, fix);
            }
            DamTab::History => history(u, k),
        }
    });
    let mut v = ui.nodes.pop().expect("panel node");
    panel(&mut v);
    v.node.padding = UiRect::all(Val::Px(12.0));
    v
}
