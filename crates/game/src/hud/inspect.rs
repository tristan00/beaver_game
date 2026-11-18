//! Inspectors (spec §4.2): one panel in the right column for whatever is selected.
use super::panels::{status_line, stage_icon, Ctx, RIGHT_W};
use super::ui::*;
use super::words::*;
use super::HudAction;
use crate::flow::Selection;
use crate::icons::Icon;
use bevy::prelude::*;
use sim::buildings::{BargeId, BargeState, BuildingId, BuildingKind, Good};
use sim::shipping::Leg;
use sim::colony::{BeaverId, Job};

fn shell(f: impl FnOnce(&mut Ui)) -> VNode {
    let mut ui = Ui::default();
    ui.node(Node { width: Val::Px(RIGHT_W), flex_direction: FlexDirection::Column, row_gap: Val::Px(6.0), ..default() }, f);
    let mut v = ui.nodes.pop().unwrap();
    panel(&mut v);
    v
}

fn header(c: &Ctx, sel: Selection, u: &mut Ui, icon: Icon, name: String, tip: Option<Tip>, extra: impl FnOnce(&mut Ui)) {
    let pinned = c.hud.pinned.contains(&sel);
    u.row(6.0, |r| {
        let t = r.row(6.0, |x| {
            x.icon(icon, 20.0, INK);
            x.bold(name, 14.0, INK);
        });
        if let Some(tip) = tip {
            t.tip(tip);
        }
        r.spacer();
        extra(r);
        r.icon_button(Icon::Pin, 11.0, HudAction::Pin(sel), FAINT, pinned).tip(Tip::name(if pinned { "Unpin" } else { "Pin" }));
        if pinned {
            r.icon_button(Icon::Cross, 11.0, HudAction::Pin(sel), FAINT, false).tip(Tip::name("Close"));
        } else {
            r.icon_button(Icon::Cross, 11.0, HudAction::CloseInspector, FAINT, false).tip(Tip::name("Close").key("Esc"));
        }
    });
}

pub fn inspector(c: &Ctx, sel: Selection) -> Option<VNode> {
    match sel {
        Selection::Site(id) => Some(building(c, id)),
        Selection::Beaver(id) => Some(beaver(c, id)),
        Selection::Tree(rid) => Some(resource(c, rid)),
        Selection::Cell(cell) => Some(cell_panel(c, cell)),
        Selection::Lodge(id) => Some(lodge(c, id)),
        Selection::Building(cell) => super::building_at(c.s, cell).map(|id| building(c, id)),
        Selection::Barge(id) => Some(barge(c, id)),
        Selection::None => None,
    }
}

// ───────────────────────────── barges and docks ─────────────────────────────

/// Finished docks in a fixed order (by id): a dock's number is its place here, plus one.
pub fn docks(s: &sim::SimState) -> Vec<BuildingId> {
    let mut v: Vec<BuildingId> = s.buildings.buildings.iter().filter(|b| b.kind == BuildingKind::Dock && b.is_complete()).map(|b| b.id).collect();
    v.sort_by_key(|b| b.0);
    v
}

pub fn dock_number(s: &sim::SimState, id: BuildingId) -> usize {
    docks(s).iter().position(|d| *d == id).map_or(0, |i| i + 1)
}

/// A barge's number: its place in the sim's list, plus one.
pub fn barge_number(s: &sim::SimState, id: BargeId) -> usize {
    s.buildings.barges.iter().position(|b| b.id == id).map_or(0, |i| i + 1)
}

/// Goods aboard: the route hold, or a single trip's cargo.
pub fn aboard(b: &sim::buildings::Barge) -> Vec<(Good, f32)> {
    let mut v: Vec<(Good, f32)> = b.hold.iter().copied().filter(|h| h.1 > 0.05).collect();
    if let Some(c) = b.cargo.filter(|c| c.1 > 0.05) {
        v.push(c);
    }
    v
}

/// What a barge is doing, as an icon, its colour and a tooltip name.
pub fn barge_face(b: &sim::buildings::Barge) -> (Icon, Color, &'static str) {
    if b.state == BargeState::Stranded {
        return (Icon::Blocked, BAD, "Stranded");
    }
    if b.service.is_none() {
        return if b.state == BargeState::Sailing { (Icon::Play, INK, "Sailing") } else { (Icon::Idle, MUTED, "No route") };
    }
    match (b.leg, b.state) {
        (Leg::Loading, _) => (Icon::Up, GOOD, "Loading"),
        (Leg::Unloading, _) => (Icon::Down, GOOD, "Unloading"),
        (Leg::ToLoad, BargeState::Sailing) => (Icon::Play, INK, "Sailing to load"),
        (Leg::ToUnload, BargeState::Sailing) => (Icon::Play, GOOD, "Sailing to unload"),
        _ => (Icon::Hourglass, WARN, "Waiting"),
    }
}

/// A dock as a small button: its icon and number; selecting it is the route picker's job.
fn dock_chip(u: &mut Ui, s: &sim::SimState, dock: BuildingId, action: HudAction, on: bool, tip: Tip) {
    u.icon_label_button(Icon::Dock, format!("{}", dock_number(s, dock)), 13.0, action, FAINT, on).tip(tip);
}

fn barge(c: &Ctx, id: BargeId) -> VNode {
    let s = c.s;
    let Some(b) = s.buildings.barge(id) else {
        return shell(|u| {
            u.text("-", 12.0, MUTED);
        });
    };
    shell(|u| {
        header(c, Selection::Barge(id), u, Icon::Barge, format!("Barge {}", barge_number(s, id)), None, |r| {
            r.icon_button(Icon::Focus, 11.0, HudAction::Focus(b.cell), FAINT, false).tip(Tip::name("Show"));
        });
        // What it is doing, what is aboard, and what it has delivered.
        let held = aboard(b);
        let total: f32 = held.iter().map(|h| h.1).sum();
        u.row(8.0, |r| {
            let (icon, col, name) = barge_face(b);
            r.icon(icon, 16.0, col).tip(Tip::name(name));
            for (g, a) in &held {
                r.row(2.0, |x| {
                    x.icon(good_icon(*g), 15.0, good_color(*g));
                    x.bold(num(*a), 12.5, INK);
                })
                .tip(Tip::name(good_name(*g)));
            }
            r.spacer();
            r.row(3.0, |x| {
                x.icon(Icon::Haul, 14.0, MUTED);
                x.bold(format!("{}/{}", total.round() as i64, sim::shipping::BARGE_CAPACITY.round() as i64), 12.0, INK);
            })
            .tip(Tip::name("Aboard"));
            r.row(3.0, |x| {
                x.icon(Icon::Check, 12.0, GOOD);
                x.bold(num(b.carried), 12.0, INK);
            })
            .tip(Tip::name("Delivered"));
        });
        // The route as it stands: loading dock ▶ unloading dock, and the goods it carries.
        u.row(5.0, |r| match &b.service {
            Some(svc) => {
                r.icon(Icon::Up, 11.0, GOOD);
                dock_chip(r, s, svc.from, HudAction::SelectBuilding(svc.from), b.leg == Leg::Loading || b.leg == Leg::ToLoad, Tip::name("Loads here"));
                r.icon(Icon::Play, 12.0, MUTED);
                r.icon(Icon::Down, 11.0, GOOD);
                dock_chip(r, s, svc.to, HudAction::SelectBuilding(svc.to), b.leg == Leg::Unloading || b.leg == Leg::ToUnload, Tip::name("Unloads here"));
                r.spacer();
                for g in &svc.goods {
                    r.icon(good_icon(*g), 15.0, good_color(*g)).tip(Tip::name(good_name(*g)));
                }
            }
            None => {
                r.icon(Icon::Idle, 14.0, MUTED);
                r.bold("-", 12.0, MUTED);
            }
        })
        .tip(Tip::name("Route"));
        // Picking a route: the loading dock, the unloading dock, the goods, then ✓.
        u.rule();
        let d = super::RouteDraft::of(s, c.hud, id);
        let all = docks(s);
        for (icon, pick, act, name) in [
            (Icon::Up, d.from, HudAction::RouteFrom as fn(BargeId, BuildingId) -> HudAction, "Load at"),
            (Icon::Down, d.to, HudAction::RouteTo as fn(BargeId, BuildingId) -> HudAction, "Unload at"),
        ] {
            u.row(4.0, |r| {
                r.icon(icon, 11.0, GOOD).tip(Tip::name(name));
                if all.is_empty() {
                    r.icon(Icon::Dock, 14.0, MUTED);
                    r.icon(Icon::Cross, 11.0, BAD);
                }
                for dock in &all {
                    dock_chip(r, s, *dock, act(id, *dock), pick == Some(*dock), Tip::name(name).with_value(format!("{}", dock_number(s, *dock))));
                }
            });
        }
        u.node(Node { flex_direction: FlexDirection::Row, flex_wrap: FlexWrap::Wrap, column_gap: Val::Px(3.0), row_gap: Val::Px(3.0), ..default() }, |r| {
            for g in Good::ALL {
                let on = d.goods.contains(&g);
                r.icon_button(good_icon(g), 15.0, HudAction::RouteGood(id, g), FAINT, on).tip(Tip::name(good_name(g)));
            }
        });
        u.row(6.0, |r| {
            // The last refusal: why, and for a dam in the way, where (a lock goes there).
            if let Some((_, m)) = c.hud.route_err.as_ref().filter(|e| e.0 == id) {
                r.icon(Icon::Cross, 12.0, BAD);
                match blocking_dam(m) {
                    Some(cell) => {
                        r.icon(Icon::Lock, 15.0, INK);
                        r.bold(format!("{},{}", cell.0, cell.1), 12.0, INK);
                        r.icon_button(Icon::Focus, 11.0, HudAction::Focus(cell), FAINT, false).tip(Tip::name("Dam in the way").line("Build a lock", ""));
                    }
                    None => {
                        r.bold(sim_error(m), 12.0, INK);
                    }
                }
            }
            r.spacer();
            if b.service.is_some() {
                r.icon_button(Icon::Clear, 13.0, HudAction::ClearRoute(id), FAINT, false).tip(Tip::name("Clear route"));
            }
            let changed = d.from != b.service.as_ref().map(|v| v.from) || d.to != b.service.as_ref().map(|v| v.to) || Some(&d.goods) != b.service.as_ref().map(|v| &v.goods);
            let fill = if d.ready() && changed { ACCENT } else if d.ready() { FAINT } else { Color::NONE };
            r.icon_button(Icon::Check, 15.0, HudAction::SetRoute(id), fill, false).tip(Tip::name("Set route"));
        });
    })
}

/// A dam's crest (height above the ground) and the button that raises it one step.
fn dam_row(s: &sim::SimState, u: &mut Ui, cell: crate::flow::Cell) {
    let line = crate::build_logic::dam_line_at(s, cell);
    if line.is_empty() {
        return;
    }
    let now = crate::build_logic::dam_line_height(s, &line);
    let ground = line.iter().filter_map(|&(x, y)| s.height.get(x, y).copied()).fold(f32::MAX, f32::min);
    let crest = line.iter().filter_map(|&(x, y)| Some(s.height.get(x, y)? + s.water.dam_height.get(x, y)?)).fold(f32::MAX, f32::min);
    let _ = ground;
    u.row(6.0, |r| {
        r.icon(Icon::Crest, 15.0, MUTED);
        r.bold(num(now), 13.0, INK).tip(Tip::name("Height").line("Crest", num(crest)).line("Cells", format!("{}", line.len())));
        r.spacer();
        match crate::build_logic::raise_order(s, cell) {
            Some(crate::build_logic::BuildOrder::Dam { cells, height }) => {
                let mut tip = Tip::name("Raise").with_value(format!("+{}", num(height - now)));
                for (g, a) in sim::dam_preview::cost_for(s, &cells, &vec![height; cells.len()]).0 {
                    tip.push(TipLine::new(good_name(g), num(a)).icon(good_icon(g), good_color(g)));
                }
                r.icon_label_button(Icon::Plus, num(height), 12.0, HudAction::RaiseDam(cell), ACCENT, false).tip(tip);
            }
            _ => {
                r.bold(format!("Max {}", num(sim::dam_preview::MAX_DAM_HEIGHT)), 11.5, MUTED);
            }
        }
    });
}

// ───────────────────────────── buildings and sites ─────────────────────────────

fn bar_row(u: &mut Ui, icon: Icon, col: Color, frac: f32, value: String, tip: Tip) {
    u.row(6.0, |r| {
        r.icon(icon, 15.0, col);
        r.bar(frac, 150.0, 6.0, col);
        r.spacer();
        r.bold(value, 12.0, INK);
    })
    .tip(tip);
}

fn building(c: &Ctx, id: BuildingId) -> VNode {
    let s = c.s;
    let Some(b) = s.buildings.building(id) else {
        return shell(|u| {
            u.text("-", 12.0, MUTED);
        });
    };
    let place = s.work.place(id);
    shell(|u| {
        header(c, Selection::Site(id), u, building_icon(b.kind), b.kind.spec().name.to_string(), None, |r| {
            if let Some(pl) = place.filter(|pl| !pl.slots.is_empty()) {
                use sim::work::Priority;
                let (icon, next, name) = match pl.priority {
                    Priority::Low => (Icon::PriorityDown, Priority::Normal, "Low priority"),
                    Priority::Normal => (Icon::Dot, Priority::High, "Normal priority"),
                    Priority::High => (Icon::PriorityUp, Priority::Low, "High priority"),
                };
                r.icon_button(icon, 11.0, HudAction::Priority(id, next), FAINT, pl.priority != Priority::Normal).tip(Tip::name(name).line("Click", "change"));
                r.icon_button(Icon::Pause, 11.0, HudAction::Paused(id, !pl.paused), FAINT, pl.paused).tip(Tip::name(if pl.paused { "Paused" } else { "Pause" }));
            }
            r.icon_button(Icon::Focus, 11.0, HudAction::Focus(b.pos), FAINT, false).tip(Tip::name("Show"));
        });
        // Status: icon + at most two words (A9).
        if let Some((icon, words, goods, col)) = status_line(s, id) {
            let line = u.row(5.0, |r| {
                r.icon(if col == GOOD { Icon::Dot } else { Icon::Warn }, 9.0, col);
                if let Some(i) = icon {
                    r.icon(i, 15.0, col);
                }
                r.bold(words, 12.5, INK);
                for g in goods {
                    r.icon(good_icon(g), 15.0, good_color(g));
                }
            });
            line.node.padding = UiRect::axes(Val::Px(6.0), Val::Px(4.0));
            line.node.border_radius = BorderRadius::all(Val::Px(5.0));
            line.bg = col.with_alpha(0.12);
        }
        // Construction: materials got/need, labour, progress.
        if let Some(site) = place.and_then(|pl| pl.site.as_ref()) {
            u.rule();
            u.row(10.0, |r| {
                for a in &site.needed {
                    let got = a.1 - site.missing(a.0);
                    r.row(3.0, |x| {
                        x.icon(good_icon(a.0), 15.0, good_color(a.0));
                        x.bold(format!("{}/{}", got.round() as i64, a.1.round() as i64), 12.0, if site.missing(a.0) > 0.0 { WARN } else { GOOD });
                    })
                    .tip(Tip::name(good_name(a.0)));
                }
            });
            bar_row(u, Icon::Hammer, INK, site.fraction(), format!("{:.0}%", site.fraction() * 100.0), Tip::name("Built").line("Labour", format!("{}/{}", num(site.labour_done), num(site.labour_needed))));
        }
        // Workers: dots, one per slot, with − and +.
        if let Some(pl) = place.filter(|pl| !pl.slots.is_empty()) {
            u.rule();
            let role = pl.slots[0].role;
            let filled = pl.slots.iter().filter(|sl| sl.worker.is_some()).count();
            u.row(5.0, |r| {
                r.icon(role_icon(role), 18.0, INK).tip(Tip::name(role_name(role)));
                for sl in &pl.slots {
                    match sl.worker.and_then(|w| s.colony.beaver(w)) {
                        Some(bv) => {
                            let tip = if sl.locked { Tip::name(bv.name.clone()).row(TipLine::new("Placed by hand", "").icon(Icon::HandLock, MUTED)) } else { Tip::name(bv.name.clone()) };
                            r.icon(if sl.locked { Icon::HandLock } else { Icon::Dot }, 16.0, GOOD).tip(tip).action(HudAction::SelectBeaver(bv.id));
                        }
                        None => {
                            r.icon(Icon::DotEmpty, 16.0, WARN).tip(Tip::name("Empty"));
                        }
                    }
                }
                r.spacer();
                r.icon_button(Icon::Minus, 11.0, HudAction::RemoveWorker(id), FAINT, false).tip(Tip::name("Free a worker"));
                let add_tip = super::add_worker_tip(s, id);
                let _ = filled;
                r.icon_button(Icon::Plus, 11.0, HudAction::AddWorker(id), if add_tip.lines.is_empty() { Color::NONE } else { FAINT }, false).tip(add_tip);
            });
        }
        // Production: measured output, the limiting factor, details behind an expander.
        if let Some(rep) = sim::hud::building_report(s, id) {
            u.rule();
            u.row(8.0, |r| {
                if rep.measured.is_empty() {
                    r.bold("-", 13.0, MUTED);
                }
                for (g, rate) in &rep.measured {
                    r.row(2.0, |x| {
                        x.icon(good_icon(*g), 16.0, good_color(*g));
                        x.bold(format!("{}{}", if rep.steady { "" } else { "~" }, num(*rate)), 13.0, GOOD);
                    });
                }
                r.spacer();
                if let Some((g, max)) = rep.ceiling {
                    r.row(3.0, |x| {
                        x.text("Max", 11.5, MUTED);
                        x.icon(good_icon(g), 13.0, good_color(g));
                        x.bold(num(max), 12.0, MUTED);
                    })
                    .tip(Tip::name("All slots, no walking").line("Per day", num(max)));
                }
            })
            .tip(Tip::name("Made per day").line(if rep.steady { "" } else { "Settling" }, ""));
            // Worker counts are already the dots above.
            let shown: Vec<&sim::hud::Factor> = rep.factors.iter().filter(|f| !matches!(f.label.as_str(), "Workers" | "Crafters")).collect();
            let limiting: Vec<&sim::hud::Factor> = shown.iter().copied().filter(|f| f.ok == Some(false)).collect();
            if let Some(f) = limiting.first() {
                factor_row(u, f);
            }
            let open = c.hud.details_open;
            u.row(4.0, |r| {
                r.icon(if open { Icon::Collapse } else { Icon::Expand }, 11.0, MUTED);
                r.icon(Icon::Log, 12.0, MUTED);
            })
            .action(HudAction::ToggleDetails)
            .tip(Tip::name("Details"));
            if open {
                for f in shown.iter().copied().filter(|f| f.ok != Some(false) || limiting.len() > 1) {
                    factor_row(u, f);
                }
            }
        }
        // Workshops and mills: what they make (nothing until chosen).
        if matches!(b.kind, BuildingKind::Workshop | BuildingKind::Mill) && b.is_complete() {
            use sim::buildings::Recipe;
            u.rule();
            u.row(6.0, |r| {
                r.icon(Icon::Tools, 14.0, MUTED);
                for (rec, g) in [(Recipe::Rope, Good::Rope), (Recipe::Baskets, Good::Baskets), (Recipe::Tools, Good::Tools)] {
                    let on = b.recipe == Some(rec);
                    let (inputs, out) = rec.batch();
                    let mut tip = Tip::titled(Some((good_icon(g), good_color(g))), good_name(g)).with_value(format!("+{}", num(out.1)));
                    for (ig, a) in inputs {
                        tip.push(TipLine::new(good_name(*ig), format!("-{}", num(*a))).icon(good_icon(*ig), good_color(*ig)).color(BAD));
                    }
                    r.icon_button(good_icon(g), 16.0, HudAction::Recipe(id, if on { None } else { Some(rec) }), FAINT, on).tip(tip);
                }
            });
        }
        // Logging camps: the cutting rule (once Forestry opens the others) and when the area runs out.
        if b.kind == BuildingKind::LoggingCamp && b.is_complete() {
            u.rule();
            forestry_camp(s, id, u);
        }
        // Forester's lodges: the planting rule, the trees in the area by stage, the next logs.
        if b.kind == BuildingKind::ForesterLodge && b.is_complete() {
            u.rule();
            forestry_lodge(s, id, u);
        }
        // A finished dam: its height and "raise".
        if let Some(cell) = crate::build_logic::dam_cell_of(s, id) {
            u.rule();
            dam_row(s, u, cell);
        }
        // A dock: its number (the route picker names docks by it) and the barges that serve it.
        if b.kind == BuildingKind::Dock && b.is_complete() {
            u.rule();
            u.row(6.0, |r| {
                r.icon(Icon::Dock, 14.0, MUTED);
                r.bold(format!("{}", dock_number(s, id)), 13.0, INK);
                r.spacer();
                for bg in s.buildings.barges.iter().filter(|bg| bg.at_dock == Some(id) || bg.service.as_ref().is_some_and(|v| v.from == id || v.to == id)) {
                    let (icon, col, name) = barge_face(bg);
                    r.icon_label_button(Icon::Barge, format!("{}", barge_number(s, bg.id)), 13.0, HudAction::SelectBarge(bg.id), FAINT, false).tip(Tip::name("Barge").with_value(format!("{}", barge_number(s, bg.id))).row(TipLine::new(name, "").icon(icon, col)));
                }
            })
            .tip(Tip::name("Barges"));
        }
        // Stores: stock and room per good.
        if matches!(b.kind, BuildingKind::Warehouse | BuildingKind::Storehouse | BuildingKind::FoodCache | BuildingKind::Dock) && b.is_complete() {
            u.rule();
            u.row(10.0, |r| {
                for g in Good::ALL {
                    let have = s.work.stock_of(id, g);
                    if have > 0.05 {
                        r.row(3.0, |x| {
                            x.icon(good_icon(g), 15.0, good_color(g));
                            x.bold(format!("{}", have.round() as i64), 12.0, INK);
                        })
                        .tip(Tip::name(good_name(g)));
                    }
                }
            });
        }
        // Gatherers: edit the work area.
        if place.is_some_and(|pl| pl.area.is_some()) && b.is_complete() {
            let editing = c.hud.area_edit == Some(id);
            u.row(6.0, |r| {
                r.icon_button(Icon::Area, 14.0, HudAction::EditArea(id), FAINT, editing).tip(Tip::name("Work area").line("Drag on the map", ""));
            });
        }
    })
}

/// The cutting rules a camp may choose now: Forestry opens all but coppicing, Coppicing that one.
fn open_cut_rules(s: &sim::SimState) -> Vec<sim::forestry::CutRule> {
    use sim::forestry::{knows, CutRule};
    CutRule::ALL
        .into_iter()
        .filter(|r| match r {
            CutRule::MatureOnly => true,
            CutRule::Coppice => knows(s, sim::tech::COPPICING_TECH),
            _ => knows(s, sim::tech::FORESTRY_TECH),
        })
        .collect()
}

/// A logging camp: rule picker (icons, one-word tips) and days until its area runs out.
fn forestry_camp(s: &sim::SimState, id: BuildingId, u: &mut Ui) {
    let rules = open_cut_rules(s);
    let now = s.forestry.cut_rule(id);
    let left = sim::forestry::cuttable_in_area(s, id);
    u.row(6.0, |r| {
        if rules.len() > 1 {
            for rule in rules {
                let (icon, name) = cut_rule_face(rule);
                r.icon_button(icon, 16.0, HudAction::CutRule(id, rule), FAINT, rule == now).tip(Tip::name(name));
            }
        }
        r.spacer();
        let (value, col) = match sim::forestry::days_until_out(s, id) {
            Some(d) => (days(d), if d < 30.0 { WARN } else { INK }),
            None => ("-".to_string(), MUTED),
        };
        r.row(3.0, |x| {
            x.icon(Icon::Tree, 14.0, GOOD);
            x.bold(format!("{left}"), 12.5, INK);
            x.icon(Icon::Hourglass, 13.0, MUTED);
            x.bold(value.clone(), 12.5, col);
        })
        .tip(Tip::name("Area runs out").line("Trees left", format!("{left}")).line("Felled per day", num(sim::forestry::felled_per_day(s, id))).with_value(value));
    });
}

/// A forester's lodge: rule picker, stage counts, planted and tended per day, logs ready in N d.
fn forestry_lodge(s: &sim::SimState, id: BuildingId, u: &mut Ui) {
    use sim::forestry::{PlantRule, TreeStage};
    let now = s.forestry.plant_rule(id);
    let Some(area) = s.work.place(id).and_then(|p| p.area) else { return };
    let rep = sim::hud::forest_report(s, &area);
    u.row(6.0, |r| {
        for rule in PlantRule::ALL {
            let (icon, name) = plant_rule_face(rule);
            r.icon_button(icon, 16.0, HudAction::PlantRule(id, rule), FAINT, rule == now).tip(Tip::name(name));
        }
        r.spacer();
        let value = rep.logs_ready_in_days.map_or("-".to_string(), days);
        r.row(3.0, |x| {
            x.icon(Icon::Logs, 14.0, good_color(Good::Logs));
            x.icon(Icon::Hourglass, 13.0, MUTED);
            x.bold(value.clone(), 12.5, INK);
        })
        .tip(Tip::name("Logs ready").with_value(value));
    });
    u.row(10.0, |r| {
        for (st, n) in [(TreeStage::Sapling, rep.saplings), (TreeStage::Young, rep.young), (TreeStage::Mature, rep.mature), (TreeStage::Old, rep.old)] {
            r.row(3.0, |x| {
                x.icon(tree_stage_icon(st), 15.0, super::world::stage_color(st));
                x.bold(format!("{n}"), 12.0, INK);
            })
            .tip(Tip::name(tree_stage_name(st)));
        }
    });
    u.row(10.0, |r| {
        r.row(3.0, |x| {
            x.icon(Icon::Sapling, 14.0, MUTED);
            x.bold(format!("+{}", num(rep.planted_per_day)), 12.0, INK);
        })
        .tip(Tip::name("Planted per day"));
        r.row(3.0, |x| {
            x.icon(Icon::Health, 14.0, MUTED);
            x.bold(num(rep.tended_per_day), 12.0, INK);
        })
        .tip(Tip::name("Tended per day"));
        if s.forestry.saplings >= 1.0 {
            r.row(3.0, |x| {
                x.icon(Icon::TreeNursery, 14.0, MUTED);
                x.bold(format!("{}", s.forestry.saplings.floor() as i64), 12.0, INK);
            })
            .tip(Tip::name("Nursery saplings"));
        }
    });
}

/// One of a building's limits as icon + number (A9).
fn factor_row(u: &mut Ui, f: &sim::hud::Factor) {
    let l = f.label.to_lowercase();
    let icon = if l.contains("tree") {
        Icon::Tree
    } else if l.contains("plant") {
        Icon::Plant
    } else if l.contains("mud") {
        Icon::MudBank
    } else if l.contains("stone") {
        Icon::Outcrop
    } else if l.contains("walk") {
        Icon::Walk
    } else if l.contains("store for") {
        Icon::NoStore
    } else if l.contains("worker") || l.contains("crafter") {
        Icon::Beaver
    } else if l.contains("wheel") {
        Icon::Wheel
    } else if l.contains("room") {
        Icon::Full
    } else if l.contains("recipe") {
        Icon::Tools
    } else {
        Icon::Dot
    };
    let col = match f.ok {
        Some(true) => GOOD,
        Some(false) => WARN,
        None => INK,
    };
    // Values are numbers where the sim gives one; words are cut to their first token.
    let v = f.value.split([' ', ',', ':']).next().unwrap_or("").to_string();
    let value = if v == "none" { "0".to_string() } else { v };
    u.row(6.0, |r| {
        r.icon(icon, 15.0, col);
        r.spacer();
        if f.ok == Some(false) {
            r.icon(Icon::Warn, 9.0, WARN);
        }
        r.bold(value, 12.5, col);
    })
    .tip(Tip::name(f.label.clone()).with_value(f.value.clone()));
}

// ───────────────────────────── beavers ─────────────────────────────

fn job_icon(j: Job) -> Icon {
    match j {
        Job::Forager => Icon::RoleForager,
        Job::Logger => Icon::RoleLogger,
        Job::Builder => Icon::RoleBuilder,
        Job::Digger => Icon::RoleMudDigger,
        Job::Hauler => Icon::RoleHauler,
        Job::Guard => Icon::RoleGuard,
        Job::Crafter => Icon::RoleCrafter,
        Job::Scout => Icon::Walk,
    }
}

pub fn mood_icon(m: f32) -> (Icon, Color) {
    if m >= 0.66 {
        (Icon::MoodGood, GOOD)
    } else if m >= 0.4 {
        (Icon::MoodOk, WARN)
    } else {
        (Icon::MoodBad, BAD)
    }
}

pub fn mood_tip(s: &sim::SimState, b: &sim::colony::Beaver) -> Tip {
    let (i, c) = mood_icon(b.mood);
    let mut t = Tip::titled(Some((i, c)), "Mood").with_value(format!("{:.0}", b.mood * 100.0));
    for r in sim::hud::mood_breakdown(s, b.id).rows {
        t.push(TipLine::new(r.label, format!("{:.0}", r.value * 100.0)));
    }
    t
}

fn health_tip(s: &sim::SimState, b: &sim::colony::Beaver) -> Tip {
    let mut t = Tip::titled(Some((Icon::Health, GOOD)), "Health").with_value(format!("{:.0}", b.health * 100.0));
    for r in sim::hud::health_breakdown(s, b.id).rows {
        t.push(TipLine::new(r.label, format!("{:+.0}", r.value * 100.0)).color(if r.value < 0.0 { BAD } else { INK }));
    }
    t
}

fn beaver(c: &Ctx, id: BeaverId) -> VNode {
    let s = c.s;
    let Some(b) = s.colony.beaver(id) else {
        return shell(|u| {
            u.text("-", 12.0, MUTED);
        });
    };
    shell(|u| {
        let tip = Tip::titled(Some((stage_icon(b.stage), INK)), b.name.clone()).line("Age", format!("{:.1}", b.age_years())).line("Life", format!("{:.0}", b.lifespan_days / 360.0));
        header(c, Selection::Beaver(id), u, stage_icon(b.stage), b.name.clone(), Some(tip), |r| {
            r.icon_button(Icon::Follow, 12.0, HudAction::Follow(id), FAINT, c.hud.follow == Some(id)).tip(Tip::name("Follow"));
        });
        // Job: role and workplace, or free.
        u.row(6.0, |r| {
            match s.work.slot_of(id) {
                Some((bid, _, role)) => {
                    r.icon(role_icon(role), 18.0, INK).tip(Tip::name(role_name(role)));
                    if let Some(bb) = s.buildings.building(bid) {
                        r.icon_label_button(building_icon(bb.kind), "", 14.0, HudAction::SelectBuilding(bid), FAINT, false).tip(Tip::name(bb.kind.spec().name));
                    }
                }
                None => {
                    let ordered = s.agents.get(id).and_then(|a| a.assignment).is_some();
                    r.icon(if ordered { Icon::Hammer } else { Icon::Idle }, 18.0, if ordered { INK } else { WARN }).tip(Tip::name(if ordered { "By order" } else { "Free" }));
                }
            }
            r.spacer();
            let (mi, mc) = mood_icon(b.mood);
            r.row(3.0, |x| {
                x.icon(mi, 16.0, mc);
                x.bold(format!("{:.0}", b.mood * 100.0), 12.0, INK);
            })
            .tip(mood_tip(s, b));
            r.row(3.0, |x| {
                x.icon(Icon::Health, 16.0, if b.health < 0.5 { BAD } else { GOOD });
                x.bold(format!("{:.0}", b.health * 100.0), 12.0, INK);
            })
            .tip(health_tip(s, b));
        });
        u.rule();
        // Needs: one-word labels, bars.
        let n = &b.needs;
        for (name, v) in [("Food", n.food), ("Rest", n.rest), ("Warmth", n.warmth), ("Safety", n.safety), ("Social", n.social)] {
            u.row(6.0, |r| {
                r.text(name, 11.5, MUTED).style(|nn| nn.width = Val::Px(56.0));
                r.bar(v, 180.0, 6.0, if v < 0.35 { BAD } else if v < 0.6 { WARN } else { GOOD });
                r.spacer();
                r.bold(format!("{:.0}", v * 100.0), 11.5, INK);
            });
        }
        u.rule();
        // Skills: one bar per job.
        u.node(Node { flex_direction: FlexDirection::Row, flex_wrap: FlexWrap::Wrap, column_gap: Val::Px(10.0), row_gap: Val::Px(4.0), ..default() }, |r| {
            for j in Job::ALL {
                let v = b.skills[j.index()];
                r.row(3.0, |x| {
                    x.icon(job_icon(j), 14.0, MUTED);
                    x.bar(v, 44.0, 5.0, INK);
                })
                .tip(Tip::name(format!("{j:?}")).with_value(format!("{:.0}", v * 100.0)));
            }
        });
        let locked = s.work.is_locked(id) || s.work.player_orders.contains(&id);
        if s.work.slot_of(id).is_some() || s.agents.get(id).and_then(|a| a.assignment).is_some() {
            u.row(6.0, |r| {
                r.spacer();
                if locked {
                    r.icon_button(Icon::HandLock, 11.0, HudAction::Unlock(id), FAINT, true).tip(Tip::name("Placed by hand").line("Click", "back to auto"));
                }
                r.icon_button(Icon::Minus, 11.0, HudAction::FreeBeaver(id), FAINT, false).tip(Tip::name("Free"));
            });
        }
    })
}

// ───────────────────────────── trees, cells, lodges ─────────────────────────────

fn resource(c: &Ctx, rid: sim::resources::ResourceId) -> VNode {
    let s = c.s;
    let Some(r) = s.resources.get(rid) else {
        return shell(|u| {
            u.text("-", 12.0, MUTED);
        });
    };
    shell(|u| {
        header(c, Selection::Tree(rid), u, resource_icon(r.kind), resource_name(r.kind).to_string(), None, |_| {});
        let frac = if r.max_amount > 0.0 { r.amount / r.max_amount } else { 0.0 };
        bar_row(u, resource_icon(r.kind), GOOD, frac, num(r.amount), Tip::name("Left").with_value(format!("{}/{}", num(r.amount), num(r.max_amount))));
        let areas: Vec<BuildingId> = s.work.places.iter().filter(|p| p.area.is_some_and(|a| a.contains(r.cell))).map(|p| p.building).collect();
        if !areas.is_empty() {
            u.row(4.0, |row| {
                row.icon(Icon::Area, 14.0, MUTED);
                for bid in areas {
                    if let Some(bb) = s.buildings.building(bid) {
                        row.icon_label_button(building_icon(bb.kind), "", 13.0, HudAction::SelectBuilding(bid), FAINT, false).tip(Tip::name(bb.kind.spec().name));
                    }
                }
            });
        }
    })
}

pub fn cell_rows(s: &sim::SimState, cell: crate::flow::Cell) -> Vec<(Icon, Color, &'static str, String)> {
    let (x, y) = cell;
    let h = s.height.get(x, y).copied().unwrap_or(0.0);
    let d = s.water.depth.get(x, y).copied().unwrap_or(0.0);
    let (fx, fy) = s.water.flow_at(x, y);
    let veg = s.ecology.vegetation.get(x, y).copied().unwrap_or(0.0);
    let flood = s.ecology.flood_days.get(x, y).copied().unwrap_or(0.0);
    let mut rows = vec![(Icon::Profile, MUTED, "Height", num(h))];
    if d > 0.01 {
        rows.push((Icon::Drop, WATER, "Depth", format!("{:.2}", d)));
        rows.push((Icon::Push, WATER, "Flow", format!("{:.2}", (fx * fx + fy * fy).sqrt())));
    }
    rows.push((Icon::Plant, GOOD, "Cover", format!("{:.0}%", veg * 100.0)));
    if flood > 0.5 {
        rows.push((Icon::Flood, WATER, "Flooded", days(flood)));
    }
    rows
}

fn cell_panel(c: &Ctx, cell: crate::flow::Cell) -> VNode {
    shell(|u| {
        header(c, Selection::Cell(cell), u, Icon::Profile, String::new(), None, |_| {});
        for (i, col, name, v) in cell_rows(c.s, cell) {
            u.row(6.0, |r| {
                r.icon(i, 15.0, col);
                r.spacer();
                r.bold(v, 12.5, INK);
            })
            .tip(Tip::name(name));
        }
        if c.s.water.has_dam(cell.0, cell.1) {
            u.rule();
            dam_row(c.s, u, cell);
        }
    })
}

fn lodge(c: &Ctx, id: sim::colony::LodgeId) -> VNode {
    let s = c.s;
    shell(|u| {
        header(c, Selection::Lodge(id), u, Icon::Lodge, String::new(), Some(Tip::name("Lodge")), |_| {});
        let residents: Vec<&sim::colony::Beaver> = s.colony.beavers.iter().filter(|b| b.lodge == Some(id)).collect();
        u.row(6.0, |r| {
            r.icon(Icon::Beaver, 16.0, INK);
            r.bold(format!("{}", residents.len()), 13.0, INK);
        })
        .tip({
            let mut t = Tip::name("Residents");
            for b in residents.iter().take(10) {
                t.push(TipLine::new(b.name.clone(), "").icon(stage_icon(b.stage), INK));
            }
            t
        });
        if let Some(l) = s.colony.lodges.iter().find(|l| l.id == id) {
            bar_row(u, Icon::Hammer, INK, l.condition, format!("{:.0}%", l.condition * 100.0), Tip::name("Condition"));
        }
    })
}
