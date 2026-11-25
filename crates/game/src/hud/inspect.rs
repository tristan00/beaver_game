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
                x.close_button(HudAction::ClosePanel(sel, c.sel == sel), c.sel == sel);
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
        Selection::Tree(id) => resource(c, id),
        Selection::Wolf(key) => wolf(c, key),
        Selection::Animal(id) => animal(c, id),
        Selection::None => None,
    }
}

/// What a beaver is doing, as a pip colour: working, free, or resting.
fn doing(s: &sim::SimState, b: &sim::colony::Beaver) -> Color {
    if !b.can_work() || b.resting {
        return REST;
    }
    if s.work.slot_of(b.id).is_some() || s.agents.get(b.id).is_some_and(|a| a.assignment.is_some()) {
        GOOD
    } else {
        ACCENT
    }
}

/// A beaver's job: its icon, colour and name, and the building it works at. A seat gives the
/// job; so does an order to gather (that sort of gatherer), to keep things mended, or to go
/// somewhere.
fn job(s: &sim::SimState, b: &sim::colony::Beaver) -> (Icon, Color, &'static str, Option<BuildingId>) {
    use sim::agents::Assignment;
    if !b.can_work() {
        return (Icon::Kit, REST, "Too young", None);
    }
    if let Some((bid, _, role)) = s.work.slot_of(b.id) {
        return (job_icon(role), GOOD, role.name(), Some(bid));
    }
    let gatherer = |kind| (job_icon(sim::jobs::Job::gathering(kind)), GOOD, sim::jobs::Job::gathering(kind).name(), None);
    match s.agents.get(b.id).and_then(|a| a.assignment) {
        Some(Assignment::Resource(r)) => match s.resources.get(r) {
            Some(res) => gatherer(res.kind),
            None => (Icon::Hammer, GOOD, "Gatherer", None),
        },
        Some(Assignment::Area(kind, _)) => gatherer(kind),
        Some(Assignment::Slot(bid, _)) => (Icon::Hammer, GOOD, "Working here", Some(bid)),
        Some(Assignment::Mend) => (job_icon(sim::jobs::Job::with_duty(sim::content::Duty::Mend)), GOOD, sim::jobs::Job::with_duty(sim::content::Duty::Mend).name(), None),
        Some(Assignment::Build) => (
            job_icon(sim::jobs::Job::with_duty(sim::content::Duty::Build)),
            GOOD,
            sim::jobs::Job::with_duty(sim::content::Duty::Build).name(),
            s.agents.build_targets.get(&b.id).copied(),
        ),
        Some(Assignment::Go(_)) => (Icon::Walk, GOOD, "Sent", None),
        None => (Icon::Idle, ACCENT, "Free", None),
    }
}

/// What a beaver is busy with, as an icon, its colour and a word for the tooltip.
pub(super) fn activity(s: &sim::SimState, b: &sim::colony::Beaver) -> (Icon, Color, &'static str) {
    use sim::agents::Task;

    if !b.can_work() {
        return (Icon::Kit, REST, "Too young to work");
    }
    if b.resting {
        return (Icon::Idle, REST, "Resting");
    }
    // A job it is not doing just now: why, never the job's name as if it were at it.
    if let Some(w) = sim::jobs::waiting(s, b.id) {
        use sim::jobs::Wait;
        return match w {
            Wait::Good(g) => (good_icon(g), WARN, good_phrase("Waiting for", g)),
            Wait::Frozen => (Icon::Winter, WARN, "Ground frozen"),
            Wait::StoreFull(g) => (good_icon(g), WARN, "Stores full"),
            Wait::Paused => (Icon::Pause, WARN, "Job paused"),
            Wait::NoWork => (Icon::Idle, WARN, "No work"),
        };
    }
    match s.agents.get(b.id).map(|a| &a.task) {
        Some(Task::Collect { target, .. }) => match s.resources.get(*target).map(|r| r.kind) {
            Some(k) => (job_icon(sim::jobs::Job::gathering(k)), GOOD, k.row().doing.as_str()),
            None => (Icon::Walk, GOOD, "Going to gather"),
        },
        Some(Task::Carry { .. } | Task::Fetch { .. } | Task::Haul { .. } | Task::Pick { .. } | Task::Supply { .. }) => (Icon::Haul, GOOD, "Hauling"),
        Some(Task::Eat { .. }) => (crate::hud::words::good_icon(sim::buildings::Good::food()), GOOD, "Eating"),
        Some(Task::Build { .. }) => (crate::hud::words::job_icon(sim::jobs::Job::with_duty(sim::content::Duty::Build)), GOOD, "Building"),
        Some(Task::Mend { .. }) => (crate::hud::words::job_icon(sim::jobs::Job::with_duty(sim::content::Duty::Mend)), GOOD, "Repairing"),
        Some(Task::DamWork { .. }) => (crate::hud::words::building_icon(sim::buildings::BuildingKind::dam()), GOOD, "Building the dam"),
        Some(Task::Dig { .. }) => (crate::hud::words::building_icon(sim::buildings::BuildingKind::canal()), GOOD, "Digging a canal"),
        Some(Task::Gate { .. }) => (crate::hud::words::building_icon(sim::buildings::BuildingKind::dam()), GOOD, "Setting a gate"),
        Some(Task::Seed { .. }) => (crate::hud::words::good_icon(sim::buildings::Good::seedling()), GOOD, "Collecting seedlings"),
        Some(Task::Plant { .. }) => (crate::hud::words::good_icon(sim::buildings::Good::seedling()), GOOD, "Planting"),
        // Home with no job: free for one.
        Some(Task::Rest) if sim::jobs::role_of(s, b.id).is_none() => (Icon::Idle, ACCENT, "Free"),
        Some(Task::Rest) => (Icon::Idle, REST, "Resting"),
        Some(Task::Watch { .. }) => (crate::hud::words::job_icon(sim::jobs::Job::with_duty(sim::content::Duty::Watch)), GOOD, "On watch"),
        Some(Task::Raft { .. }) => (Icon::Raft, GOOD, "Rafting"),
        Some(Task::Idle) | None => match s.work.slot_of(b.id) {
            Some((_, _, role)) => (job_icon(role), GOOD, role.name()),
            None if s.agents.get(b.id).is_some_and(|a| a.assignment.is_some()) => (Icon::Walk, GOOD, "On an order"),
            None => (Icon::Idle, ACCENT, "Free"),
        },
    }
}

/// A family: one row per member (size by age, name, what it is doing, what it carries), then
/// the room left in the lodge.
fn family_rows(c: &Ctx, u: &mut Ui, residents: &[&sim::colony::Beaver], room: usize) {
    let s = c.s;
    let mut members: Vec<&sim::colony::Beaver> = residents.to_vec();
    members.sort_by_key(|b| (std::cmp::Reverse(b.stage), b.id.0));
    for b in members {
        let px = match b.stage {
            sim::colony::Stage::Adult => 22.0,
            sim::colony::Stage::Yearling => 18.0,
            sim::colony::Stage::Kit => 14.0,
        };
        let (icon, col, word) = activity(s, b);
        u.row(8.0, |r| {
            r.node(Node { width: Val::Px(24.0), justify_content: JustifyContent::Center, ..default() }, |t| {
                t.token(Icon::Beaver, px, Some(doing(s, b)));
            });
            r.bold(b.name.clone(), 13.0, INK);
            r.spacer();
            if let Some(l) = s.agents.get(b.id).and_then(|a| a.load.as_ref()) {
                r.icon(good_icon(l.good), 14.0, good_color(l.good)).tip(Tip::name("Carrying").line(l.good.name(), num(l.amount)));
            }
            r.icon(icon, 16.0, col).tip(Tip::name(word));
        })
        .action(HudAction::SelectBeaver(b.id))
        .tip(Tip::titled(Some((stage_icon(b.stage), INK)), b.name.clone()).line(word, ""));
    }
    if residents.len() < room {
        u.row(6.0, |r| {
            for _ in residents.len()..room {
                r.empty_token(16.0);
            }
        })
        .tip(Tip::name("Room").with_value(format!("{}", room - residents.len())));
    }
}

fn repair_row(c: &Ctx, u: &mut Ui, id: BuildingId) {
    let s = c.s;
    let Some(b) = s.buildings.building(id) else { return };
    if !b.is_complete() {
        return;
    }
    u.row(8.0, |r| {
        if let Some(job) = s.maintenance.jobs.get(&id) {
            let what = match job.kind {
                sim::maintenance::Refit::Reinforce => "Reinforcing with mud",
                sim::maintenance::Refit::Facing => "Facing with pebbles",
                sim::maintenance::Refit::Repair | sim::maintenance::Refit::ReinforcedRepair => "Repairing",
            };
            r.icon(Icon::Hammer, 18.0, GOOD).tip(Tip::name(what));
            r.pebble(Icon::Undo, 12.0, HudAction::CancelMaintenance(id), Peb::Idle).tip(Tip::name("Stop").line("Delivered materials come back", ""));
            return;
        }
        if b.condition < 0.98 {
            let mut tip = Tip::name("Repair");
            for (g, a) in sim::maintenance::quoted_cost(s, b, sim::maintenance::Refit::Repair).0 {
                tip.push(TipLine::new(g.name(), num(a)).icon(good_icon(g), good_color(g)));
            }
            r.pebble(Icon::Hammer, 15.0, HudAction::RepairBuilding(id), if b.condition < 0.55 { Peb::Primary } else { Peb::Good }).tip(tip);
        }
        if !b.reinforced && sim::maintenance::can_reinforce(b.kind) {
            let mut tip = Tip::name("Reinforce with mud").line("Lasts", format!("{}×", num(sim::maintenance::lasts(b.kind, sim::maintenance::Refit::Reinforce))));
            for (g, a) in sim::maintenance::cost(b, sim::maintenance::Refit::Reinforce).0 {
                tip.push(TipLine::new(g.name(), num(a)).icon(good_icon(g), good_color(g)));
            }
            let icon = sim::maintenance::cost(b, sim::maintenance::Refit::Reinforce).0.last().map_or(Icon::Hammer, |c| good_icon(c.0));
            r.pebble(icon, 15.0, HudAction::ReinforceBuilding(id), Peb::Idle).tip(tip);
        }
        if !s.buildings.pebbled.contains(&id) && sim::maintenance::can_pebble(b.kind) {
            let mut tip = Tip::name("Face with pebbles").line("Lasts", format!("{}×", num(sim::maintenance::lasts(b.kind, sim::maintenance::Refit::Facing))));
            for (g, a) in sim::maintenance::cost(b, sim::maintenance::Refit::Facing).0 {
                tip.push(TipLine::new(g.name(), num(a)).icon(good_icon(g), good_color(g)));
            }
            r.pebble(Icon::Pebbles, 15.0, HudAction::PebbleBuilding(id), Peb::Idle).tip(tip);
        }
    });
}

/// A building still going up: the work done in ticks, and what it waits for.
fn site_rows(c: &Ctx, u: &mut Ui, id: BuildingId) -> bool {
    let s = c.s;
    let Some(site) = s.work.place(id).and_then(|pl| pl.site.as_ref()) else { return false };
    if s.maintenance.jobs.contains_key(&id) {
        return false;
    }
    u.row(8.0, |r| {
        r.icon(Icon::Hammer, 16.0, MUTED);
        r.ticks(site.fraction(), 10, 200.0, 12.0, ACCENT);
        r.spacer();
        cancel_build(r, s, id);
    })
    .tip({
        let mut tip = Tip::name("Built").line("Labour", format!("{} / {}", num(site.labour_done), num(site.labour_needed)));
        let (crew, rate) = sim::agents::site_crew(s, id);
        if crew > 0 && rate > 0.0 {
            tip = tip.line("Builders", format!("{crew}")).line("A day", num(rate)).line("Done in", days((site.labour_needed - site.labour_done).max(0.0) / rate));
        }
        tip
    });
    u.row(14.0, |r| {
        for a in &site.needed {
            let got = a.1 - site.missing(a.0);
            r.row(4.0, |x| {
                x.icon(good_icon(a.0), 16.0, good_color(a.0));
                x.bold(format!("{}/{}", got.round() as i64, a.1.round() as i64), 13.0, if site.missing(a.0) > 0.0 { WARN } else { INK });
            })
            .tip(Tip::name(a.0.name()).line("Delivered of needed", ""));
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
        (k, Some(l)) if k.is_home() && s.colony.beavers.iter().any(|x| x.lodge == Some(l.id)) => {
            s.colony.family(l.family).map_or_else(|| b.kind.spec().name.to_string(), |f| f.name.clone())
        }
        _ => b.kind.spec().name.to_string(),
    };
    let made = match (s.buildings.pebbled.contains(&id), b.reinforced) {
        (true, _) => "Pebble faced",
        (false, true) => "Mud reinforced",
        _ => b.kind.spec().cost.first().map_or("", |c| c.0.name()),
    };
    let mut tip = Tip::titled(Some((building_icon(b.kind), INK)), b.kind.spec().name.as_str()).line(made, format!("{:.0}%", b.condition * 100.0));
    // Wear: the daily rate, then only the factors acting on it now.
    if let Some(w) = sim::maintenance::wear(s, b) {
        tip = tip.line("Wear", format!("{:.2}%/day", w.per_day() * 100.0)).line("Base", format!("{:.2}%/day", w.base * 100.0));
        for (label, f, op) in [("Cold", w.cold, "×"), ("Flooded", w.flooded, "×"), ("Mud reinforced", w.reinforced, "÷"), ("Pebble faced", w.pebbled, "÷")] {
            if (f - 1.0).abs() > 0.005 {
                tip = tip.line(label, format!("{op}{f:.2}"));
            }
        }
    }
    let cond = b.is_complete().then_some(b.condition);
    frame(c, sel, building_icon(b.kind), MUD, name, cond, tip, |u| {
        let building = site_rows(c, u, id);
        // What a building has: a family, stores (or the balancing of all stores), seats.
        if b.kind.is_home() {
            if let Some(l) = home {
                let residents: Vec<&sim::colony::Beaver> = s.colony.beavers.iter().filter(|x| x.lodge == Some(l.id)).collect();
                family_rows(c, u, &residents, sim::colony::LODGE_CAPACITY);
            } else if !building {
                u.empty_token(22.0).tip(Tip::name("No family yet"));
            }
        }
        if !b.is_complete() {
            return;
        }
        if b.kind.balances() {
            let on = s.work.balanced.contains(&id);
            u.row(8.0, |r| {
                r.icon(Icon::Haul, 18.0, if on { GOOD } else { MUTED }).tip(Tip::name("Balance stores").line("Haulers even out every store", ""));
                r.spacer();
                let (icon, look) = if on { (Icon::Pause, Peb::Good) } else { (Icon::Play, Peb::Idle) };
                r.pebble(icon, 11.0, HudAction::Balance(id, !on), look).tip(Tip::name(if on { "Stop balancing" } else { "Balance" }));
            });
        } else if !b.kind.spec().stores.is_empty() {
            stock_rows(s, u, id, b.kind);
            fill_row(s, u, id);
        }
        let Some(pl) = s.work.place(id).filter(|pl| !pl.slots.is_empty() && !b.kind.is_waterworks()) else { return };
        if b.kind.seats(sim::content::Duty::Forest) {
            // Seats turned on and off one by one.
            for (i, sl) in pl.slots.iter().enumerate() {
                let name = sl.role.name();
                u.row(8.0, |r| {
                    r.icon(job_icon(sl.role), 18.0, if sl.off { MUTED } else { INK }).tip(Tip::name(name));
                    match sl.beaver.and_then(|w| s.colony.beaver(w)) {
                        Some(bv) => {
                            r.token(Icon::Beaver, 22.0, Some(GOOD)).action(HudAction::SelectBeaver(bv.id)).tip(Tip::titled(Some((stage_icon(bv.stage), INK)), bv.name.clone()).line(name, ""));
                        }
                        None => {
                            r.empty_token(22.0).tip(Tip::name(if sl.off { "Off" } else { "Nobody yet" }));
                        }
                    }
                    r.spacer();
                    let (icon, look, what) = if sl.off { (Icon::Play, Peb::Idle, "Turn on") } else { (Icon::Pause, Peb::Good, "Turn off") };
                    r.pebble(icon, 11.0, HudAction::SlotOff(id, i, !sl.off), look).tip(Tip::name(what));
                });
            }
            return;
        }
        u.rule();
        u.row(8.0, |r| {
            for sl in &pl.slots {
                match sl.beaver.and_then(|w| s.colony.beaver(w)) {
                    Some(bv) => {
                        let t = Tip::titled(Some((stage_icon(bv.stage), INK)), bv.name.clone()).line(sl.role.name(), "");
                        r.token(Icon::Beaver, 22.0, Some(GOOD)).action(HudAction::SelectBeaver(bv.id)).tip(t);
                    }
                    None => {
                        r.empty_token(22.0).tip(Tip::name("Nobody yet").line(sl.role.name(), ""));
                    }
                }
            }
            r.spacer();
            let add_ok = sim::jobs::add_beaver_check(s, id).is_ok();
            r.pebble(Icon::Minus, 10.0, HudAction::RemoveBeaver(id), if pl.filled() > 0 { Peb::Idle } else { Peb::Faded }).tip(Tip::name("Free a beaver"));
            r.pebble(Icon::Plus, 10.0, HudAction::AddBeaver(id), if add_ok { Peb::Idle } else { Peb::Faded }).tip(super::add_beaver_tip(s, id));
        });
        if b.is_complete() {
            repair_row(c, u, id);
        }
    })
}

/// Fill this store: haulers bring its goods from the other stores until it is full.
fn fill_row(s: &sim::SimState, u: &mut Ui, id: BuildingId) {
    let on = s.work.fills.contains(&id);
    u.row(8.0, |r| {
        r.icon(Icon::Up, 16.0, if on { GOOD } else { MUTED }).tip(Tip::name("Fill").line("Haulers bring it from other stores", ""));
        r.spacer();
        let (icon, look) = if on { (Icon::Pause, Peb::Good) } else { (Icon::Play, Peb::Idle) };
        r.pebble(icon, 11.0, HudAction::FillStore(id, !on), look).tip(Tip::name(if on { "Stop filling" } else { "Fill" }));
    });
}

/// One row per good the store keeps: ticks of its room, and the count.
fn stock_rows(s: &sim::SimState, u: &mut Ui, id: BuildingId, kind: BuildingKind) {
    let cap = kind.spec().capacity.max(1.0);
    for g in Good::all().filter(|&g| sim::agents::accepts(kind, g)) {
        let have = s.work.stock_of(id, g);
        u.row(10.0, |r| {
            r.icon(good_icon(g), 18.0, good_color(g));
            r.ticks(have / cap, 8, 170.0, 12.0, ACCENT);
            r.spacer();
            r.bold(format!("{}", have.round() as i64), 14.0, INK);
        })
        .tip(Tip::name(g.name()).with_value(format!("{} / {}", have.round() as i64, cap.round() as i64)));
    }
}

fn beaver(c: &Ctx, id: BeaverId) -> VNode {
    let s = c.s;
    let sel = Selection::Beaver(id);
    let Some(b) = s.colony.beaver(id) else {
        return frame(c, sel, Icon::Beaver, MUTED, "-".into(), None, Tip::name("Gone"), |_| {});
    };
    let name = full_name(s, id, &b.name);
    let tip = Tip::titled(Some((stage_icon(b.stage), INK)), name.clone()).line("Age", span(b.age_days as f64)).line("Health", format!("{:.0}", b.health * 100.0));
    frame(c, sel, Icon::Beaver, FUR, name, Some(b.health), tip, |u| {
        // Its job, big and named, with the workplace; what it is doing this moment; its load.
        let (job_icon, job_col, job_name, place) = job(s, b);
        let (now_icon, now_col, now_name) = activity(s, b);
        u.row(10.0, |r| {
            r.node(Node { width: Val::Px(40.0), height: Val::Px(40.0), justify_content: JustifyContent::Center, align_items: AlignItems::Center, flex_shrink: 0.0, ..default() }, |p| {
                p.icon(job_icon, 26.0, job_col);
            })
            .skin(Skin::Portrait)
            .tip(Tip::name(job_name));
            r.col(4.0, |x| {
                x.bold(job_name, 14.0, job_col);
                if now_icon != job_icon {
                    x.icon(now_icon, 14.0, now_col).tip(Tip::name(now_name));
                }
            });
            if let Some(bb) = place.and_then(|bid| s.buildings.building(bid)) {
                r.pebble(building_icon(bb.kind), 14.0, HudAction::SelectBuilding(bb.id), Peb::Idle).tip(Tip::name(bb.kind.spec().name.as_str()).line("Show", ""));
            }
            r.spacer();
            if let Some(l) = s.agents.get(id).and_then(|a| a.load.as_ref()) {
                r.row(4.0, |x| {
                    x.icon(good_icon(l.good), 16.0, good_color(l.good));
                    x.bold(num(l.amount), 13.0, INK);
                })
                .tip(Tip::name("Carrying").line(l.good.name(), num(l.amount)));
            }
        });
        u.rule();
        let n = &b.needs;
        for (icon, col, name, v) in [(crate::hud::words::good_icon(sim::buildings::Good::food()), good_color(Good::food()), Good::food().name(), n.food), (Icon::Idle, REST, "Rest", n.rest), (Icon::Home, MUD, "Safety", n.safety)] {
            u.row(10.0, |r| {
                r.icon(icon, 16.0, col);
                r.pips(v, condition_colour(v));
            })
            .tip(Tip::name(name).with_value(format!("{:.0}", v * 100.0)));
        }
        u.rule();
        u.row(8.0, |r| {
            // Family first, wrapping onto more lines when the young are many, so the buttons
            // after it stay in the panel.
            r.node(Node { flex_direction: FlexDirection::Row, flex_wrap: FlexWrap::Wrap, flex_grow: 1.0, flex_shrink: 1.0, min_width: Val::Px(0.0), column_gap: Val::Px(8.0), row_gap: Val::Px(4.0), align_items: AlignItems::Center, ..default() }, |r| {
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
            });
            let assigned = s.work.slot_of(id).is_some() || s.agents.get(id).and_then(|a| a.assignment).is_some();
            if assigned {
                r.pebble(Icon::Minus, 11.0, HudAction::FreeBeaver(id), Peb::Idle).tip(Tip::name("Free"));
            }
            r.pebble(Icon::Journal, 13.0, HudAction::BeaverDetails(Some(id)), if c.hud.details == Some(id) { Peb::On } else { Peb::Idle }).tip(Tip::name("Details").line("Life, family, skills", ""));
            r.pebble(Icon::Follow, 13.0, HudAction::Follow(id), if c.hud.follow == Some(id) { Peb::On } else { Peb::Idle }).tip(Tip::name("Follow"));
        });
    })
}

fn stage_name(st: sim::forestry::TreeStage) -> &'static str {
    use sim::forestry::TreeStage::*;
    match st {
        Sapling => "Sapling",
        Young => "Young",
        Mature => "Mature",
        Old => "Old",
        Snag => "Dead",
    }
}

/// Sticks (logs and branches) and food (bark) as a row of icons and amounts.
fn yield_row(u: &mut Ui, sticks: f32, food: f32, tip: Tip) {
    u.row(14.0, |r| {
        for (g, a) in [(sim::resources::ResourceKind::felled().yields()[0], sticks), (Good::food(), food)] {
            if a > 0.0 {
                r.row(4.0, |x| {
                    x.icon(good_icon(g), 16.0, good_color(g));
                    x.bold(num(a), 13.0, INK);
                });
            }
        }
    })
    .tip(tip);
}

/// A tree, a felled trunk, or a patch of food or mud on the map.
fn resource(c: &Ctx, id: sim::resources::ResourceId) -> Option<VNode> {
    use sim::forestry::{self, TreeStage};

    let s = c.s;
    let r = s.resources.get(id)?;
    let sel = Selection::Tree(id);
    let species = r.species.map(|t| t.name());
    match r.kind {
        k if k.is_tree() => {
            let sp = r.species?;
            let stage = r.stage()?;
            let drown = forestry::drowning(s, r);
            let years = r.age_days / sim::calendar::YEAR_DAYS as f32;
            let mut tip = Tip::titled(Some((crate::hud::words::resource_icon(sim::resources::ResourceKind::tree()), GOOD)), sp.name()).line(stage_name(stage), "");
            if stage != TreeStage::Snag {
                tip = tip.line("Age", format!("{years:.1} y"));
            }
            let name = if stage == TreeStage::Snag { format!("Dead {}", sp.name().to_lowercase()) } else { sp.name().to_string() };
            let col = if stage == TreeStage::Snag { MUTED } else { GOOD };
            Some(frame(c, sel, crate::hud::words::resource_icon(sim::resources::ResourceKind::tree()), col, name, drown.map(|d| 1.0 - d), tip, |u| {
                // Growth: how far through this stage, and the days to the next.
                if let Some((next, left)) = forestry::days_to_next_stage(s, r) {
                    let (from, to) = match stage {
                        TreeStage::Sapling => (0.0, forestry::YOUNG_AGE),
                        TreeStage::Young => (forestry::YOUNG_AGE, forestry::MATURE_AGE),
                        _ => (forestry::MATURE_AGE, forestry::OLD_AGE),
                    };
                    // Only the factors acting on its growth now.
                    let mut grow = Tip::name(format!("{} in", stage_name(next))).with_value(days(left));
                    for (label, f) in [("Species", sp.spec().growth), ("Ground", forestry::ground_growth(s, r.cell))] {
                        if (f - 1.0).abs() > 0.005 {
                            grow = grow.line(label, format!("×{}", num(f)));
                        }
                    }
                    if sim::calendar::Season::at(s.time_days) == sim::calendar::Season::Winter {
                        grow = grow.line("Winter", "×0");
                    }
                    u.row(8.0, |x| {
                        x.icon(Icon::Hourglass, 16.0, MUTED);
                        x.ticks((r.age_days - from) / (to - from), 10, 170.0, 12.0, GOOD);
                        x.spacer();
                        x.bold(days(left), 13.0, INK);
                    })
                    .tip(grow);
                }
                // Who planted it, and when.
                if let Some(&(who, day)) = s.forestry.planted.get(&id) {
                    let name = s.colony.beaver(who).map(|b| b.name.clone());
                    u.row(8.0, |x| {
                        x.icon(crate::hud::words::good_icon(sim::buildings::Good::seedling()), 16.0, GOOD);
                        match &name {
                            Some(n) => {
                                x.token(Icon::Beaver, 16.0, None).action(HudAction::SelectBeaver(who)).tip(Tip::name(n.clone()).line("Show", ""));
                                x.bold(n.clone(), 13.0, INK);
                            }
                            None => {
                                x.icon(Icon::Beaver, 16.0, MUTED);
                            }
                        }
                        x.spacer();
                        x.bold(days((s.time_days - day) as f32), 13.0, MUTED);
                    })
                    .tip(Tip::name("Planted").line("By", name.unwrap_or_else(|| "a beaver now gone".into())).line("Ago", days((s.time_days - day) as f32)));
                }
                // What felling it now gives.
                if let Some((logs, branches, bark)) = stage.fell_yield(sp) {
                    let tip = Tip::name("Felled now").line(sim::resources::ResourceKind::felled().yields()[0].name(), num(logs + branches)).line(Good::food().name(), num(bark)).line("Felling work", format!("×{}", num(sp.spec().fell_work)));
                    yield_row(u, logs + branches, bark, tip);
                } else {
                    u.row(8.0, |x| {
                        x.icon(crate::hud::words::resource_icon(sim::resources::ResourceKind::tree()), 16.0, MUTED);
                    })
                    .tip(Tip::name("Too young to fell"));
                }
                // Water: depth here against what the species stands.
                let depth = s.water.depth.get(r.cell.0, r.cell.1).copied().unwrap_or(0.0);
                let limit = sp.spec().max_depth;
                u.row(8.0, |x| {
                    match drown {
                        Some(d) => {
                            x.icon(Icon::Flood, 16.0, BAD);
                            x.ticks(d, 10, 170.0, 12.0, BAD);
                        }
                        None => {
                            x.icon(Icon::Drop, 16.0, WATER);
                            x.ticks((depth / limit).min(1.0), 10, 170.0, 12.0, WATER);
                        }
                    }
                    x.spacer();
                    x.bold(format!("{} m", num(limit)), 13.0, INK);
                })
                .tip({
                    let tip = Tip::name(if drown.is_some() { "Drowning" } else { "Water" }).line("Here", format!("{:.2} m", depth)).line("Stands up to", format!("{} m", num(limit)));
                    match drown {
                        Some(d) => tip.line("Dies in", days((1.0 - d) * forestry::drown_days(stage, sp))),
                        None => tip,
                    }
                });
            }))
        }
        k if k.is_felled() => {
            let name = species.map_or_else(|| resource_name(r.kind).to_string(), |n| format!("Felled {}", n.to_lowercase()));
            Some(frame(c, sel, crate::hud::words::resource_icon(sim::resources::ResourceKind::tree()), MUD, name, None, Tip::name(resource_name(r.kind)), |u| {
                yield_row(u, r.amount + r.branches, r.bark, Tip::name("Left to gather"));
            }))
        }
        _ => {
            let good = r.kind.yields().first().copied().unwrap_or(Good::food());
            // What refills it a day now.
            let mut amount = Tip::name(good.name()).with_value(format!("{} / {}", num(r.amount), num(r.max_amount)));
            if r.kind.is_food_plant() {
                let growth = sim::calendar::vegetation_growth(s.time_days);
                let net = sim::resources::plant_regrow(r, growth) - sim::resources::plant_decay(growth) * r.amount;
                amount = amount.line("A day", format!("{net:+.2}"));
                if !r.wet {
                    amount = amount.line("Dry ground", "×0");
                } else if (growth - 1.0).abs() > 0.005 {
                    amount = amount.line("Season", format!("×{growth:.2}"));
                }
            } else if r.kind.flowing() {
                amount = if r.wet { amount.line("A day", format!("+{:.2}", sim::resources::RIVER_MUD_PER_DAY)) } else { amount.line("Not flowing", "×0") };
            }
            let name = r.plant.map_or(resource_name(r.kind), |p| p.name());
            Some(frame(c, sel, resource_icon(r.kind), good_color(good), name.to_string(), None, Tip::name(name), |u| {
                u.row(8.0, |x| {
                    x.icon(good_icon(good), 16.0, good_color(good));
                    x.ticks(r.amount / r.max_amount.max(0.01), 10, 170.0, 12.0, ACCENT);
                    x.spacer();
                    x.bold(num(r.amount), 13.0, INK);
                })
                .tip(amount);
            }))
        }
    }
}

/// One plain line on a card: icon, words, and a value at the right.
fn line(u: &mut Ui, icon: Icon, col: Color, words: String, value: String) -> &mut VNode {
    u.row(8.0, |x| {
        x.icon(icon, 16.0, col);
        x.text(words, 13.0, INK);
        x.spacer();
        x.bold(value, 13.0, INK);
    })
}

/// A deer, rabbit or bird: what it is doing, how much it eats, and when it moves on.
fn animal(c: &Ctx, id: u32) -> Option<VNode> {
    let s = c.s;
    let a = s.wildlife.get(id)?;
    let (icon, doing) = animal_doing(a);
    let name = animal_name(a.kind);
    let tip = Tip::titled(Some((animal_icon(a.kind), INK)), name).line(if a.kind.prey() { "Prey for wolves" } else { "Flies from wolves" }, "");
    Some(frame(c, Selection::Animal(id), animal_icon(a.kind), FUR, name.to_string(), None, tip, |u| {
        let pace = if a.pace > 0.0 { format!("{}/d", num(a.pace)) } else { String::new() };
        line(u, icon, MUTED, doing.to_string(), pace).tip(Tip::name(doing).line("Cells a day", num(a.pace)));
        line(u, crate::hud::words::good_icon(sim::buildings::Good::food()), good_color(Good::food()), "Eats".to_string(), format!("{}/d", num(a.kind.appetite()))).tip(Tip::name("Eats while feeding").line("Food a day", num(a.kind.appetite())));
        if !matches!(a.doing, sim::wildlife::Doing::Leave | sim::wildlife::Doing::Caught) {
            line(u, Icon::Hourglass, MUTED, "Moves on in".to_string(), days((a.leave_by - s.time_days) as f32)).tip(Tip::name("Wanders off the map"));
        }
    }))
}

/// A wolf, while the colony sees it: what it is after, and when it leaves.
fn wolf(c: &Ctx, key: u64) -> Option<VNode> {
    use sim::predators::WolfState;
    let s = c.s;
    let w = s.predators.wolf(key)?;
    let beaver = |id| s.colony.beaver(id).map_or_else(|| "a beaver".to_string(), |b| b.name.clone());
    let prey = |id| s.wildlife.get(id).map_or("animal", |a| animal_name(a.kind)).to_lowercase();
    let (icon, col, doing) = match w.state {
        WolfState::Roam => (Icon::Walk, MUTED, "Roaming".to_string()),
        WolfState::Charge { target, .. } => (Icon::Fast2, BAD, format!("Charging {}", beaver(target))),
        WolfState::Kill { target, .. } => (Icon::Cross, BAD, format!("Killing {}", beaver(target))),
        WolfState::Leave => (Icon::Walk, MUTED, "Leaving".to_string()),
        WolfState::Hunt { animal, .. } => (Icon::Fast2, WARN, format!("Hunting a {}", prey(animal))),
        WolfState::Feed { animal, .. } => (crate::hud::words::good_icon(sim::buildings::Good::food()), WARN, format!("Eating a {}", prey(animal))),
    };
    let tip = Tip::titled(Some((Icon::Wolf, BAD)), "Wolf").line("Hunts beavers on land", "");
    Some(frame(c, Selection::Wolf(key), Icon::Wolf, BAD, "Wolf".to_string(), None, tip, |u| {
        line(u, icon, col, doing, if w.pace > 0.0 { format!("{}/d", num(w.pace)) } else { String::new() });
        if w.state != WolfState::Leave {
            line(u, Icon::Hourglass, MUTED, "Leaves in".to_string(), days((w.leave_by - s.time_days) as f32)).tip(Tip::name("Heads off the map"));
        }
        line(u, Icon::Focus, MUTED, "Sees".to_string(), format!("{} cells", num(sim::predators::WOLF_SIGHT))).tip(Tip::name("Sight").line("Circle on the map", ""));
    }))
}

fn lodge(c: &Ctx, id: sim::colony::LodgeId) -> VNode {
    let s = c.s;
    let l = s.colony.lodges.iter().find(|l| l.id == id);
    let name = l.and_then(|l| s.colony.family(l.family)).map_or_else(|| sim::buildings::BuildingKind::home().name().to_string(), |f| f.name.clone());
    let cond = l.map(|l| l.condition);
    frame(c, Selection::Lodge(id), crate::hud::words::building_icon(sim::buildings::BuildingKind::home()), MUD, name, cond, Tip::name(sim::buildings::BuildingKind::home().name()), |u| {
        let residents: Vec<&sim::colony::Beaver> = s.colony.beavers.iter().filter(|b| b.lodge == Some(id)).collect();
        family_rows(c, u, &residents, sim::colony::LODGE_CAPACITY);
        u.row(6.0, |r| {
            r.spacer();
            r.pebble(Icon::Journal, 13.0, HudAction::LodgePage(Some(id)), if c.hud.lodge_page == Some(id) { Peb::On } else { Peb::Idle }).tip(Tip::name("Family").line("Who lives here, and the family tree", ""));
        });
    })
}
