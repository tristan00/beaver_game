//! One inspector frame for everything you can select: a portrait, the name,
//! condition pips, then a short body of icons for its kind. Sentences live in
//! tooltips.
use super::panels::{stage_icon, Ctx, RIGHT_W};
use super::ui::*;
use super::words::*;
use super::HudAction;
use crate::flow::Selection;
use crate::icons::Icon;
use bevy::prelude::*;
use sim::buildings::{BuildingId, BuildingKind, Good};
use sim::colony::BeaverId;

/// The shared frame: portrait, name and condition pips, pin and close, then the body.
#[allow(clippy::too_many_arguments)]
fn frame(c: &Ctx, sel: Selection, icon: Icon, icon_col: Color, name: String, condition: Option<f32>, tip: Tip, body: impl FnOnce(&mut Ui)) -> VNode {
    let pinned = c.hud.pinned.contains(&sel);
    let mut ui = Ui::default();
    ui.node(Node { width: Val::Px(RIGHT_W), flex_direction: FlexDirection::Column, row_gap: Val::Px(8.0), ..default() }, |u| {
        u.row(12.0, |r| {
            r.portrait(icon, icon_col);
            r.col(7.0, |x| {
                x.bold(name, 16.0, INK);
                if let Some(cond) = condition {
                    x.pips(cond, condition_colour(cond));
                }
            })
            .tip(tip);
            r.spacer();
            r.row(4.0, |x| {
                x.pebble(Icon::Pin, 10.0, HudAction::Pin(sel), if pinned { Peb::On } else { Peb::Bare }).tip(Tip::name(if pinned { "Unpin" } else { "Pin" }));
                x.pebble(Icon::Cross, 10.0, if pinned { HudAction::Pin(sel) } else { HudAction::CloseInspector }, Peb::Bare).tip(Tip::name("Close").key("Esc"));
            })
            .style(|n| n.align_self = AlignSelf::FlexStart);
        });
        u.rule();
        body(u);
    });
    let mut v = ui.nodes.pop().expect("frame");
    panel(&mut v);
    v.node.padding = UiRect::all(Val::Px(12.0));
    v
}

fn condition_colour(v: f32) -> Color {
    if v < 0.35 {
        BAD
    } else if v < 0.6 {
        WARN
    } else {
        GOOD
    }
}

pub fn inspector(c: &Ctx, sel: Selection) -> Option<VNode> {
    match sel {
        Selection::Site(id) => {
            let b = c.s.buildings.building(id)?;
            Some(match (b.dam.as_deref(), b.canal.as_deref()) {
                (Some(d), _) => super::dam_panel::dam_panel(c, id, d),
                (_, Some(k)) => super::canal_panel::canal_panel(c, id, k),
                _ => building(c, id),
            })
        }
        Selection::Beaver(id) => Some(beaver(c, id)),
        Selection::Lodge(id) => Some(lodge(c, id)),
        Selection::Cell(cell) => {
            let b = c.s.buildings.buildings.iter().find(|b| b.dam.is_some() && b.cells().any(|x| x == cell))?;
            Some(super::dam_panel::dam_panel(c, b.id, b.dam.as_deref()?))
        }
        Selection::Tree(_) | Selection::None => None,
    }
}

/// What a beaver is doing, as a pip colour: working, free, or resting.
fn doing(s: &sim::SimState, b: &sim::colony::Beaver) -> Color {
    if b.stage == sim::colony::Stage::Kit || b.resting {
        return REST;
    }
    if s.work.slot_of(b.id).is_some() || s.agents.get(b.id).is_some_and(|a| a.assignment.is_some()) {
        GOOD
    } else {
        ACCENT
    }
}

fn resident_tokens(c: &Ctx, u: &mut Ui, residents: &[&sim::colony::Beaver], room: usize) {
    let s = c.s;
    u.node(Node { flex_direction: FlexDirection::Row, flex_wrap: FlexWrap::Wrap, column_gap: Val::Px(10.0), row_gap: Val::Px(8.0), align_items: AlignItems::FlexEnd, ..default() }, |r| {
        for b in residents {
            let px = match b.stage {
                sim::colony::Stage::Adult => 24.0,
                sim::colony::Stage::Yearling => 19.0,
                sim::colony::Stage::Kit => 15.0,
            };
            r.token(Icon::Beaver, px, Some(doing(s, b))).action(HudAction::SelectBeaver(b.id)).tip(Tip::titled(Some((stage_icon(b.stage), INK)), b.name.clone()));
        }
        for _ in residents.len()..room {
            r.empty_token(22.0).tip(Tip::name("Room for one more"));
        }
    });
}

fn repair_row(c: &Ctx, u: &mut Ui, id: BuildingId) {
    let s = c.s;
    let Some(b) = s.buildings.building(id) else { return };
    if !b.is_complete() {
        return;
    }
    u.row(8.0, |r| {
        if let Some(job) = s.maintenance.jobs.get(&id) {
            let what = if job.kind == sim::maintenance::Refit::Reinforce { "Reinforcing with clay" } else { "Repairing" };
            r.icon(Icon::Hammer, 18.0, GOOD).tip(Tip::name(what));
            r.pebble(Icon::Undo, 12.0, HudAction::CancelMaintenance(id), Peb::Idle).tip(Tip::name("Stop").line("Delivered materials come back", ""));
            return;
        }
        if b.condition < 0.98 {
            let mut tip = Tip::name("Repair");
            for (g, a) in sim::maintenance::quoted_cost(s, b, sim::maintenance::Refit::Repair).0 {
                tip.push(TipLine::new(good_name(g), num(a)).icon(good_icon(g), good_color(g)));
            }
            r.pebble(Icon::Hammer, 15.0, HudAction::RepairBuilding(id), if b.condition < 0.55 { Peb::Primary } else { Peb::Good }).tip(tip);
        }
        if !b.reinforced && sim::maintenance::can_reinforce(b.kind) {
            let mut tip = Tip::name("Reinforce with clay").line("Lasts", if b.kind == BuildingKind::Shelter { "1.75×" } else { "1.5×" });
            for (g, a) in sim::maintenance::cost(b, sim::maintenance::Refit::Reinforce).0 {
                tip.push(TipLine::new(good_name(g), num(a)).icon(good_icon(g), good_color(g)));
            }
            r.pebble(good_icon(Good::Clay), 15.0, HudAction::ReinforceBuilding(id), Peb::Idle).tip(tip);
        }
    });
}

/// A building still going up: the work done in notches, and what it waits for.
fn site_rows(c: &Ctx, u: &mut Ui, id: BuildingId) -> bool {
    let s = c.s;
    let Some(site) = s.work.place(id).and_then(|pl| pl.site.as_ref()) else { return false };
    if s.maintenance.jobs.contains_key(&id) {
        return false;
    }
    u.row(8.0, |r| {
        r.icon(Icon::Hammer, 16.0, MUTED);
        r.notches(site.fraction(), 10, 200.0, 12.0, ACCENT);
    })
    .tip(Tip::name("Built").line("Labour", format!("{} / {}", num(site.labour_done), num(site.labour_needed))));
    u.row(14.0, |r| {
        for a in &site.needed {
            let got = a.1 - site.missing(a.0);
            r.row(4.0, |x| {
                x.icon(good_icon(a.0), 16.0, good_color(a.0));
                x.bold(format!("{}/{}", got.round() as i64, a.1.round() as i64), 13.0, if site.missing(a.0) > 0.0 { WARN } else { INK });
            })
            .tip(Tip::name(good_name(a.0)).line("Delivered of needed", ""));
        }
    });
    true
}

fn building(c: &Ctx, id: BuildingId) -> VNode {
    let s = c.s;
    let b = s.buildings.building(id).expect("building");
    let sel = Selection::Site(id);
    let home = s.work.homes.get(&id).and_then(|h| s.colony.lodges.iter().find(|l| l.id == *h));
    let name = match (b.kind, home) {
        (BuildingKind::Shelter, Some(l)) => s.colony.family(l.family).map_or_else(|| b.kind.spec().name.to_string(), |f| f.name.clone()),
        _ => b.kind.spec().name.to_string(),
    };
    let tip = Tip::titled(Some((building_icon(b.kind), INK)), b.kind.spec().name).line(if b.reinforced { "Clay reinforced" } else { "Timber" }, format!("{:.0}%", b.condition * 100.0));
    let cond = b.is_complete().then_some(b.condition);
    frame(c, sel, building_icon(b.kind), MUD, name, cond, tip, |u| {
        let building = site_rows(c, u, id);
        match b.kind {
            BuildingKind::Shelter => {
                if let Some(l) = home {
                    let residents: Vec<&sim::colony::Beaver> = s.colony.beavers.iter().filter(|x| x.lodge == Some(l.id)).collect();
                    resident_tokens(c, u, &residents, sim::colony::LODGE_CAPACITY);
                } else if !building {
                    u.empty_token(22.0).tip(Tip::name("No family yet"));
                }
            }
            BuildingKind::Warehouse if b.is_complete() => {
                let cap = b.kind.spec().capacity.max(1.0);
                for g in Good::ALL {
                    let have = s.work.stock_of(id, g);
                    u.row(10.0, |r| {
                        r.icon(good_icon(g), 18.0, good_color(g));
                        r.notches(have / cap, 8, 170.0, 12.0, ACCENT);
                        r.spacer();
                        r.bold(format!("{}", have.round() as i64), 14.0, INK);
                    })
                    .tip(Tip::name(good_name(g)).with_value(format!("{} / {}", have.round() as i64, cap.round() as i64)));
                }
                if let Some(pl) = s.work.place(id).filter(|pl| !pl.slots.is_empty()) {
                    u.rule();
                    u.row(8.0, |r| {
                        for sl in &pl.slots {
                            match sl.worker.and_then(|w| s.colony.beaver(w)) {
                                Some(bv) => {
                                    let mut t = Tip::titled(Some((stage_icon(bv.stage), INK)), bv.name.clone()).line("Hauler", "");
                                    if sl.locked {
                                        t = t.line("Placed by hand", "");
                                    }
                                    r.token(Icon::Beaver, 22.0, Some(GOOD)).action(HudAction::SelectBeaver(bv.id)).tip(t);
                                }
                                None => {
                                    r.empty_token(22.0).tip(Tip::name("No hauler"));
                                }
                            }
                        }
                        r.spacer();
                        r.pebble(Icon::Minus, 10.0, HudAction::RemoveWorker(id), Peb::Idle).tip(Tip::name("Free the hauler"));
                        r.pebble(Icon::Plus, 10.0, HudAction::AddWorker(id), Peb::Idle).tip(super::add_worker_tip(s, id));
                    });
                }
            }
            _ => {}
        }
        if b.is_complete() {
            repair_row(c, u, id);
        }
    })
}

fn beaver(c: &Ctx, id: BeaverId) -> VNode {
    let s = c.s;
    let sel = Selection::Beaver(id);
    let Some(b) = s.colony.beaver(id) else {
        return frame(c, sel, Icon::Beaver, MUTED, "-".into(), None, Tip::name("Gone"), |_| {});
    };
    let tip = Tip::titled(Some((stage_icon(b.stage), INK)), b.name.clone()).line("Age", format!("{:.1}", b.age_years())).line("Health", format!("{:.0}", b.health * 100.0));
    frame(c, sel, Icon::Beaver, FUR, b.name.clone(), Some(b.health), tip, |u| {
        // What it is doing and where.
        u.row(8.0, |r| {
            match s.work.slot_of(id) {
                Some((bid, _, role)) => {
                    r.icon(role_icon(role), 18.0, MUTED).tip(Tip::name(role_name(role)));
                    r.icon(Icon::Expand, 10.0, MUTED);
                    if let Some(bb) = s.buildings.building(bid) {
                        r.pebble(building_icon(bb.kind), 14.0, HudAction::SelectBuilding(bid), Peb::Idle).tip(Tip::name(bb.kind.spec().name).line("Show", ""));
                    }
                }
                None => match s.agents.get(id).and_then(|a| a.assignment) {
                    Some(_) => {
                        r.icon(Icon::Hammer, 18.0, MUTED).tip(Tip::name("On an order"));
                    }
                    None if b.stage == sim::colony::Stage::Kit => {
                        r.icon(Icon::Kit, 18.0, REST).tip(Tip::name("Too young to work"));
                    }
                    None => {
                        r.icon(Icon::Idle, 18.0, ACCENT).tip(Tip::name("Free"));
                    }
                },
            }
            r.spacer();
            if let Some(l) = s.agents.get(id).and_then(|a| a.load.as_ref()) {
                r.row(4.0, |x| {
                    x.icon(good_icon(l.good), 16.0, good_color(l.good));
                    x.bold(num(l.amount), 13.0, INK);
                })
                .tip(Tip::name("Carrying").line(good_name(l.good), num(l.amount)));
            }
        });
        u.rule();
        let n = &b.needs;
        for (icon, col, name, v) in [(Icon::Food, good_color(Good::Food), "Food", n.food), (Icon::Idle, REST, "Rest", n.rest), (Icon::Home, MUD, "Safety", n.safety)] {
            u.row(10.0, |r| {
                r.icon(icon, 16.0, col);
                r.pips(v, condition_colour(v));
            })
            .tip(Tip::name(name).with_value(format!("{:.0}", v * 100.0)));
        }
        u.rule();
        u.row(8.0, |r| {
            if let Some(f) = s.colony.family(b.family) {
                r.icon(Icon::Home, 16.0, MUD).tip(Tip::name(f.name.clone()));
            }
            if let Some(m) = b.mate.and_then(|m| s.colony.beaver(m)) {
                r.token(Icon::Beaver, 20.0, None).action(HudAction::SelectBeaver(m.id)).tip(Tip::name(m.name.clone()).line("Mate", ""));
            }
            for k in s.colony.beavers.iter().filter(|k| k.stage != sim::colony::Stage::Adult && (k.mother == Some(id) || k.father == Some(id))) {
                r.token(Icon::Beaver, 14.0, None).action(HudAction::SelectBeaver(k.id)).tip(Tip::name(k.name.clone()).line("Young", ""));
            }
            if let Some(p) = &b.pregnancy {
                r.icon(Icon::Kit, 14.0, GOOD);
                r.bold(format!("{:.0}d", (p.due_day - s.time_days).max(0.0)), 12.0, INK).tip(Tip::name(format!("{} kits on the way", p.litter)));
            }
            r.spacer();
            let assigned = s.work.slot_of(id).is_some() || s.agents.get(id).and_then(|a| a.assignment).is_some();
            if s.work.is_locked(id) || s.work.player_orders.contains(&id) {
                r.pebble(Icon::HandLock, 12.0, HudAction::Unlock(id), Peb::On).tip(Tip::name("Placed by hand").line("Click", "back to auto"));
            }
            if assigned {
                r.pebble(Icon::Minus, 11.0, HudAction::FreeBeaver(id), Peb::Idle).tip(Tip::name("Free"));
            }
            r.pebble(Icon::Follow, 13.0, HudAction::Follow(id), if c.hud.follow == Some(id) { Peb::On } else { Peb::Idle }).tip(Tip::name("Follow"));
        });
    })
}

fn lodge(c: &Ctx, id: sim::colony::LodgeId) -> VNode {
    let s = c.s;
    let l = s.colony.lodges.iter().find(|l| l.id == id);
    let name = l.and_then(|l| s.colony.family(l.family)).map_or_else(|| "Lodge".to_string(), |f| f.name.clone());
    let cond = l.map(|l| l.condition);
    frame(c, Selection::Lodge(id), Icon::Lodge, MUD, name, cond, Tip::name("Lodge"), |u| {
        let residents: Vec<&sim::colony::Beaver> = s.colony.beavers.iter().filter(|b| b.lodge == Some(id)).collect();
        resident_tokens(c, u, &residents, sim::colony::LODGE_CAPACITY);
    })
}

#[cfg(test)]
mod tests {
    use super::condition_colour;
    use crate::hud::ui::{BAD, GOOD, WARN};

    #[test]
    fn condition_reads_red_amber_green() {
        assert_eq!(condition_colour(0.1), BAD);
        assert_eq!(condition_colour(0.5), WARN);
        assert_eq!(condition_colour(0.9), GOOD);
    }
}
