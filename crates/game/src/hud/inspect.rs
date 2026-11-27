use super::colony::doing;
use super::panels::{stage_icon, Ctx, RIGHT_W};
use super::ui::*;
use super::words::*;
use super::HudAction;
use crate::flow::Selection;
use crate::icons::Icon;
use bevy::prelude::*;
use sim::buildings::{BuildingId, BuildingKind, Good};
use sim::colony::BeaverId;

#[allow(clippy::too_many_arguments)]
pub(super) fn panel_shell(
    c: &Ctx,
    sel: Selection,
    selected_here: bool,
    portrait: impl FnOnce(&mut Ui),
    title: impl FnOnce(&mut Ui),
    tools: impl FnOnce(&mut Ui),
    divider: bool,
    body: impl FnOnce(&mut Ui),
) -> VNode {
    let pinned = c.hud.pinned.contains(&sel);
    let mut ui = Ui::default();
    ui.node(Node { width: Val::Px(RIGHT_W), flex_direction: FlexDirection::Column, row_gap: Val::Px(8.0), ..default() }, |u| {
        u.row(10.0, |r| {
            portrait(r);
            title(r);
            r.spacer();
            r.row(4.0, |x| {
                tools(x);
                x.pebble(Icon::Pin, 10.0, HudAction::Pin(sel), if pinned { Peb::On } else { Peb::Bare }).tip(Tip::name(if pinned { "Unpin" } else { "Pin" }));
                x.close_button(HudAction::ClosePanel(sel, selected_here), selected_here);
            }).style(|n| n.align_self = AlignSelf::FlexStart);
        });
        if divider { u.rule(); }
        body(u);
    });
    let mut v = ui.nodes.pop().expect("panel_shell");
    panel(&mut v);
    v.node.padding = UiRect::all(Val::Px(12.0));
    v
}

#[allow(clippy::too_many_arguments)]
fn frame(c: &Ctx, sel: Selection, icon: Icon, icon_col: Color, name: String, condition: Option<f32>, tip: Tip, body: impl FnOnce(&mut Ui)) -> VNode {
    panel_shell(
        c, sel, c.sel == sel,
        |r| { r.portrait(icon, icon_col); },
        |r| {
            r.col(7.0, |x| {
                x.bold(name, 16.0, INK);
                if let Some(cond) = condition { x.pips(cond, level_color(cond)); }
            }).tip(tip);
        },
        |_| {},
        true,
        body,
    )
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
        Selection::Tree(id) => resource(c, id),
        Selection::Wolf(key) => wolf(c, key),
        Selection::Animal(id) => animal(c, id),
        Selection::Cell(_) | Selection::None => None,
    }
}

pub(super) fn job(s: &sim::SimState, b: &sim::colony::Beaver) -> (Icon, Color, &'static str, Option<BuildingId>) {
    use sim::agents::Assignment;
    if !b.can_work() { return (Icon::Kit, REST, "Too young", None); }
    if let Some((bid, _, role)) = s.work.slot_of(b.id) { return (job_icon(role), GOOD, role.name(), Some(bid)); }
    let gatherer = |kind| { let j = sim::jobs::Job::gathering(kind); (job_icon(j), GOOD, j.name(), None) };
    let by_duty = |d| { let j = sim::jobs::Job::with_duty(d); (job_icon(j), GOOD, j.name()) };
    match s.agents.get(b.id).and_then(|a| a.assignment) {
        Some(Assignment::Resource(_, kind) | Assignment::Area(kind, _)) => gatherer(kind),
        Some(Assignment::Mend(target)) => { let (i, c, n) = by_duty(sim::content::Duty::Mend); (i, c, n, target) }
        Some(Assignment::Build(target)) => { let (i, c, n) = by_duty(sim::content::Duty::Build); (i, c, n, target) }
        Some(Assignment::Go(_)) => (Icon::Walk, GOOD, "Sent", None),
        Some(Assignment::Work(_)) | None => (Icon::Idle, ACCENT, "Free", None),
    }
}

pub(super) fn activity(s: &sim::SimState, b: &sim::colony::Beaver) -> (Icon, Color, &'static str) {
    use sim::agents::{Task, WorkKind};
    use sim::content::Duty;
    if !b.can_work() { return (Icon::Kit, REST, "Too young to work"); }
    if b.resting { return (Icon::Idle, REST, "Resting"); }
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
        Some(Task::Collect { target, .. }) => s.resources.get(*target).map_or((Icon::Walk, GOOD, "Going to gather"), |r| (job_icon(sim::jobs::Job::gathering(r.kind)), GOOD, r.kind.row().doing.as_str())),
        Some(Task::Move { .. }) => (Icon::Haul, GOOD, "Hauling"),
        Some(Task::Work { kind: WorkKind::Build, .. }) => (job_icon(sim::jobs::Job::with_duty(Duty::Build)), GOOD, "Building"),
        Some(Task::Work { kind: WorkKind::Mend, .. }) => (job_icon(sim::jobs::Job::with_duty(Duty::Mend)), GOOD, "Repairing"),
        Some(Task::Work { kind: WorkKind::Dam, .. }) => (building_icon(BuildingKind::dam()), GOOD, "Building the dam"),
        Some(Task::Work { kind: WorkKind::Dig, .. }) => (building_icon(BuildingKind::canal()), GOOD, "Digging a canal"),
        Some(Task::Work { kind: WorkKind::Gate, .. }) => (building_icon(BuildingKind::dam()), GOOD, "Setting a gate"),
        Some(Task::Seed { .. }) => (good_icon(Good::seedling()), GOOD, "Taking seedlings"),
        Some(Task::Plant { .. }) => (good_icon(Good::seedling()), GOOD, "Planting"),
        Some(Task::Rest) if !sim::jobs::is_busy(s, b.id) => (Icon::Idle, ACCENT, "Free"),
        Some(Task::Rest) => (Icon::Idle, REST, "Resting"),
        Some(Task::Watch { .. }) => (job_icon(sim::jobs::Job::with_duty(Duty::Watch)), GOOD, "On watch"),
        Some(Task::Raft { .. }) => (Icon::Raft, GOOD, "Rafting"),
        Some(Task::Idle) | None => match s.work.slot_of(b.id) {
            Some((_, _, role)) => (job_icon(role), GOOD, role.name()),
            None if s.agents.get(b.id).is_some_and(|a| a.assignment.is_some()) => (Icon::Walk, GOOD, "On an order"),
            None => (Icon::Idle, ACCENT, "Free"),
        },
    }
}

fn family_rows(c: &Ctx, u: &mut Ui, residents: &[&sim::colony::Beaver], room: usize) {
    let s = c.s;
    let mut members = residents.to_vec();
    members.sort_by_key(|b| (std::cmp::Reverse(b.stage), b.id.0));
    for b in members {
        let px = match b.stage { sim::colony::Stage::Adult => 22.0, sim::colony::Stage::Yearling => 18.0, sim::colony::Stage::Kit => 14.0 };
        let (icon, col, word) = activity(s, b);
        u.row(8.0, |r| {
            r.node(Node { width: Val::Px(24.0), justify_content: JustifyContent::Center, ..default() }, |t| { t.token(Icon::Beaver, px, Some(doing(s, b))); });
            r.bold(b.name.clone(), 13.0, INK);
            r.spacer();
            if let Some(l) = s.agents.get(b.id).and_then(|a| a.load.as_ref()) {
                r.icon(good_icon(l.good), 14.0, good_color(l.good)).tip(Tip::name("Carrying").line(l.good.name(), num(l.amount)));
            }
            r.icon(icon, 16.0, col).tip(Tip::name(word));
        }).action(HudAction::GoTo(super::NoticeAt::Beaver(b.id))).tip(Tip::titled(Some((stage_icon(b.stage), INK)), b.name.clone()).line(word, ""));
    }
    if residents.len() < room {
        u.row(6.0, |r| for _ in residents.len()..room { r.empty_token(16.0); }).tip(Tip::name("Room").with_value(format!("{}", room - residents.len())));
    }
}

pub(super) fn refit_tip(b: &sim::buildings::Building, refit: sim::maintenance::Refit, title: &str, show_lasts: bool) -> Tip {
    let mut tip = Tip::name(title);
    if show_lasts { tip = tip.line("Lasts", format!("{}×", num(sim::maintenance::lasts(b.kind, refit)))); }
    for (g, a) in sim::maintenance::cost(b, refit).0 { tip.push(TipLine::new(g.name(), num(a)).icon(good_icon(g), good_color(g))); }
    tip
}

fn repair_row(c: &Ctx, u: &mut Ui, id: BuildingId) {
    use sim::maintenance::{self, Refit};
    let s = c.s;
    let Some(b) = s.buildings.building(id).filter(|b| b.is_complete()) else { return };
    u.row(8.0, |r| {
        if let Some(&kind) = s.maintenance.jobs.get(&id) {
            let what = match kind { Refit::Reinforce => "Reinforcing with mud", Refit::Facing => "Facing with pebbles", _ => "Repairing" };
            r.icon(Icon::Hammer, 18.0, GOOD).tip(Tip::name(what));
            r.pebble(Icon::Undo, 12.0, HudAction::Cmd(sim::Command::Refit { building: id, refit: None }), Peb::Idle).tip(Tip::name("Stop").line("Delivered materials come back", ""));
            return;
        }
        if b.condition < 0.98 {
            r.pebble(Icon::Hammer, 15.0, HudAction::Cmd(sim::Command::Refit { building: id, refit: Some(Refit::Repair) }), if b.condition < 0.55 { Peb::Primary } else { Peb::Good }).tip(refit_tip(b, Refit::Repair, "Repair", false));
        }
        if !b.reinforced && maintenance::can_reinforce(b.kind) {
            let icon = maintenance::cost(b, Refit::Reinforce).0.last().map_or(Icon::Hammer, |c| good_icon(c.0));
            r.pebble(icon, 15.0, HudAction::Cmd(sim::Command::Refit { building: id, refit: Some(Refit::Reinforce) }), Peb::Idle).tip(refit_tip(b, Refit::Reinforce, "Reinforce with mud", true));
        }
        if !b.pebbled && maintenance::can_pebble(b.kind) {
            r.pebble(Icon::Pebbles, 15.0, HudAction::Cmd(sim::Command::Refit { building: id, refit: Some(Refit::Facing) }), Peb::Idle).tip(refit_tip(b, Refit::Facing, "Face with pebbles", true));
        }
    });
}

fn site_rows(c: &Ctx, u: &mut Ui, id: BuildingId) -> bool {
    let s = c.s;
    let Some(site) = s.work.place(id).and_then(|pl| pl.site.as_ref()).filter(|_| !s.maintenance.jobs.contains_key(&id)) else { return false };
    let mut tip = Tip::name("Built").line("Labour", format!("{} / {}", num(site.labour_done), num(site.labour_needed)));
    let (crew, rate) = sim::agents::site_crew(s, id);
    if crew > 0 && rate > 0.0 { tip = tip.line("Builders", format!("{crew}")).line("A day", num(rate)).line("Done in", days((site.labour_needed - site.labour_done).max(0.0) / rate)); }
    u.row(8.0, |r| { r.icon(Icon::Hammer, 16.0, MUTED); r.ticks(site.fraction(), 10, 200.0, 12.0, ACCENT); r.spacer(); cancel_build(r, s, id); }).tip(tip);
    u.row(14.0, |r| {
        for &(g, need) in &site.needed {
            let got = need - site.missing(g);
            r.row(4.0, |x| { x.icon(good_icon(g), 16.0, good_color(g)); x.bold(format!("{}/{}", got.round() as i64, need.round() as i64), 13.0, if site.missing(g) > 0.0 { WARN } else { INK }); })
                .tip(Tip::name(g.name()).line("Delivered of needed", ""));
        }
    });
    true
}

fn toggle_row(u: &mut Ui, icon: Icon, on: bool, tip: Tip, action: HudAction, off_word: &str, on_word: &str) {
    u.row(8.0, |r| {
        r.icon(icon, 17.0, if on { GOOD } else { MUTED }).tip(tip);
        r.spacer();
        r.pebble(if on { Icon::Pause } else { Icon::Play }, 11.0, action, if on { Peb::Good } else { Peb::Idle }).tip(Tip::name(if on { off_word } else { on_word }));
    });
}

fn building(c: &Ctx, id: BuildingId) -> VNode {
    let s = c.s;
    let b = s.buildings.building(id).expect("building");
    let residents: Vec<_> = if b.is_home_ready() { s.colony.beavers.iter().filter(|x| x.lodge == Some(id)).collect() } else { Vec::new() };
    let name = if b.is_home_ready() {
        s.colony.lodge_family(id).and_then(|f| s.colony.family(f)).map_or_else(|| b.kind.spec().name.to_string(), |f| f.name.clone())
    } else {
        b.kind.spec().name.to_string()
    };
    let made = if b.pebbled { "Pebble faced" } else if b.reinforced { "Mud reinforced" } else { b.kind.spec().cost.first().map_or("", |c| c.0.name()) };
    let mut tip = Tip::titled(Some((building_icon(b.kind), INK)), b.kind.spec().name.as_str()).line(made, format!("{:.0}%", b.condition * 100.0));
    if let Some(w) = sim::maintenance::wear(s, b) {
        tip = tip.line("Wear", format!("{:.2}%/day", w.per_day() * 100.0)).line("Base", format!("{:.2}%/day", w.base * 100.0));
        for (label, f, op) in [("Cold", w.cold, "×"), ("Flooded", w.flooded, "×"), ("Mud reinforced", w.reinforced, "÷"), ("Pebble faced", w.pebbled, "÷")] {
            if (f - 1.0).abs() > 0.005 { tip = tip.line(label, format!("{op}{f:.2}")); }
        }
    }
    frame(c, Selection::Site(id), building_icon(b.kind), MUD, name, b.is_complete().then_some(b.condition), tip, |u| {
        let building = site_rows(c, u, id);
        if b.kind.is_home() {
            if b.is_complete() {
                family_rows(c, u, &residents, sim::colony::LODGE_CAPACITY);
                u.row(6.0, |r| {
                    r.spacer();
                    r.pebble(Icon::Journal, 13.0, HudAction::Overlay(Some(super::Overlay::Page(super::Page::Lodge(id)))), if c.hud.page() == Some(super::Page::Lodge(id)) { Peb::On } else { Peb::Idle }).tip(Tip::name("Family").line("Who lives here, and the family tree", ""));
                });
            } else if !building { u.empty_token(22.0).tip(Tip::name("No family yet")); }
        }
        if !b.is_complete() { return; }
        super::costs::upkeep_row(c, u, id);
        if b.kind.balances() {
            let on = s.work.balanced.contains(&id);
            toggle_row(u, Icon::Haul, on, Tip::name("Balance stores").line("Haulers even out every store", ""), HudAction::Cmd(sim::Command::SetBalance { building: id, on: !on }), "Stop balancing", "Balance");
        } else if !b.kind.spec().stores.is_empty() {
            let cap = b.kind.spec().capacity.max(1.0);
            for g in Good::all().filter(|&g| sim::agents::accepts(b.kind, g)) {
                let have = s.buildings.stores.stock_of(id, g);
                u.row(10.0, |r| { r.icon(good_icon(g), 18.0, good_color(g)); r.ticks(have / cap, 8, 170.0, 12.0, ACCENT); r.spacer(); r.bold(format!("{}", have.round() as i64), 14.0, INK); })
                    .tip(Tip::name(g.name()).with_value(format!("{} / {}", have.round() as i64, cap.round() as i64)));
            }
            let on = s.work.fills.contains(&id);
            toggle_row(u, Icon::Up, on, Tip::name("Fill").line("Haulers bring it from other stores", ""), HudAction::Cmd(sim::Command::FillStore { store: id, on: !on }), "Stop filling", "Fill");
        }
        if let Some(pl) = s.work.place(id).filter(|pl| !pl.slots.is_empty() && pl.site.is_none() && !b.kind.is_waterworks()) {
            if b.kind.seats(sim::content::Duty::Forest) {
                for (i, sl) in pl.slots.iter().enumerate() {
                    let name = sl.role.name();
                    u.row(8.0, |r| {
                        r.icon(job_icon(sl.role), 18.0, if sl.off { MUTED } else { INK }).tip(Tip::name(name));
                        match sl.beaver.and_then(|w| s.colony.beaver(w)) {
                            Some(bv) => { r.token(Icon::Beaver, 22.0, Some(GOOD)).action(HudAction::GoTo(super::NoticeAt::Beaver(bv.id))).tip(Tip::titled(Some((stage_icon(bv.stage), INK)), bv.name.clone()).line(name, "")); }
                            None => { r.empty_token(22.0).tip(Tip::name(if sl.off { "Off" } else { "Nobody yet" })); }
                        }
                        r.spacer();
                        r.pebble(if sl.off { Icon::Play } else { Icon::Pause }, 11.0, HudAction::Cmd(sim::Command::SetSlotOff { building: id, slot: i, off: !sl.off }), if sl.off { Peb::Idle } else { Peb::Good }).tip(Tip::name(if sl.off { "Turn on" } else { "Turn off" }));
                    });
                }
            } else {
                u.rule();
                u.row(8.0, |r| {
                    for sl in &pl.slots {
                        match sl.beaver.and_then(|w| s.colony.beaver(w)) {
                            Some(bv) => { r.token(Icon::Beaver, 22.0, Some(GOOD)).action(HudAction::GoTo(super::NoticeAt::Beaver(bv.id))).tip(Tip::titled(Some((stage_icon(bv.stage), INK)), bv.name.clone()).line(sl.role.name(), "")); }
                            None => { r.empty_token(22.0).tip(Tip::name("Nobody yet").line(sl.role.name(), "")); }
                        }
                    }
                    r.spacer();
                    r.pebble(Icon::Minus, 10.0, HudAction::Cmd(sim::Command::StaffBuilding { building: id, delta: -1 }), if pl.filled() > 0 { Peb::Idle } else { Peb::Faded }).tip(Tip::name("Free a beaver"));
                    r.pebble(Icon::Plus, 10.0, HudAction::Cmd(sim::Command::StaffBuilding { building: id, delta: 1 }), if sim::jobs::add_beaver_check(s, id).is_ok() { Peb::Idle } else { Peb::Faded }).tip(super::add_beaver_tip(s, id));
                });
            }
        }
        repair_row(c, u, id);
    })
}

fn beaver(c: &Ctx, id: BeaverId) -> VNode {
    let (s, sel) = (c.s, Selection::Beaver(id));
    let Some(b) = s.colony.beaver(id) else { return frame(c, sel, Icon::Beaver, MUTED, "-".into(), None, Tip::name("Gone"), |_| {}); };
    let name = full_name(s, id, &b.name);
    let tip = Tip::titled(Some((stage_icon(b.stage), INK)), name.clone()).line("Age", span(b.age_days as f64)).line("Health", format!("{:.0}", b.health * 100.0));
    frame(c, sel, Icon::Beaver, FUR, name, Some(b.health), tip, |u| {
        let ((job_i, job_col, job_name, place), (now_i, now_col, now_name)) = (job(s, b), activity(s, b));
        u.row(10.0, |r| {
            r.node(Node { width: Val::Px(40.0), height: Val::Px(40.0), justify_content: JustifyContent::Center, align_items: AlignItems::Center, flex_shrink: 0.0, ..default() }, |p| { p.icon(job_i, 26.0, job_col); })
                .skin(Skin::Portrait).tip(Tip::name(job_name));
            r.col(4.0, |x| { x.bold(job_name, 14.0, job_col); if now_i != job_i { x.icon(now_i, 14.0, now_col).tip(Tip::name(now_name)); } });
            if let Some(bb) = place.and_then(|bid| s.buildings.building(bid)) {
                r.pebble(building_icon(bb.kind), 14.0, HudAction::GoTo(super::NoticeAt::Building(bb.id)), Peb::Idle).tip(Tip::name(bb.kind.spec().name.as_str()).line("Show", ""));
            }
            r.spacer();
            if let Some(l) = s.agents.get(id).and_then(|a| a.load.as_ref()) {
                r.row(4.0, |x| { x.icon(good_icon(l.good), 16.0, good_color(l.good)); x.bold(num(l.amount), 13.0, INK); }).tip(Tip::name("Carrying").line(l.good.name(), num(l.amount)));
            }
        });
        u.rule();
        let (n, rates) = (&b.needs, s.beaver_rates(id));
        for (icon, col, v, tip) in [
            (good_icon(Good::food()), good_color(Good::food()), n.food, rates.as_ref().map_or_else(|| Tip::name(Good::food().name()).with_value(format!("{:.0}", n.food * 100.0)), |r| super::life::food_tip(b, r))),
            (Icon::Idle, REST, n.rest, rates.as_ref().map_or_else(|| Tip::name("Rest").with_value(format!("{:.0}", n.rest * 100.0)), |r| super::life::rest_tip(b, r))),
            (Icon::Home, MUD, n.safety, rates.as_ref().map_or_else(|| Tip::name("Safety").with_value(format!("{:.0}", n.safety * 100.0)), |r| super::life::safety_tip(b, r))),
        ] { u.row(10.0, |r| { r.icon(icon, 16.0, col); r.pips(v, level_color(v)); }).tip(tip); }
        u.rule();
        u.row(8.0, |r| {
            r.node(Node { flex_direction: FlexDirection::Row, flex_wrap: FlexWrap::Wrap, flex_grow: 1.0, flex_shrink: 1.0, min_width: Val::Px(0.0), column_gap: Val::Px(8.0), row_gap: Val::Px(4.0), align_items: AlignItems::Center, ..default() }, |r| {
                if let Some(f) = s.colony.family(b.family) { r.icon(Icon::Home, 16.0, MUD).tip(Tip::name(f.name.clone())); }
                if let Some(m) = b.mate.and_then(|m| s.colony.beaver(m)) { r.token(Icon::Beaver, 20.0, None).action(HudAction::GoTo(super::NoticeAt::Beaver(m.id))).tip(Tip::name(m.name.clone()).line("Mate", "")); }
                for k in s.colony.beavers.iter().filter(|k| k.stage != sim::colony::Stage::Adult && (k.mother == Some(id) || k.father == Some(id))) {
                    r.token(Icon::Beaver, 14.0, None).action(HudAction::GoTo(super::NoticeAt::Beaver(k.id))).tip(Tip::name(k.name.clone()).line("Young", ""));
                }
                if let Some(p) = &b.pregnancy { r.icon(Icon::Kit, 14.0, GOOD); r.bold(format!("{:.0}d", (p.due_day - s.time_days).max(0.0)), 12.0, INK).tip(Tip::name(format!("{} kits on the way", p.litter))); }
            });
            if sim::jobs::is_busy(s, id) {
                r.pebble(Icon::Minus, 11.0, HudAction::Cmd(sim::Command::Assign { who: vec![id], to: None, queue: false }), Peb::Idle).tip(Tip::name("Free"));
            }
            r.pebble(Icon::Journal, 13.0, HudAction::Overlay(Some(super::Overlay::Page(super::Page::Beaver(id)))), if c.hud.page() == Some(super::Page::Beaver(id)) { Peb::On } else { Peb::Idle }).tip(Tip::name("Details").line("Life, family, skills", ""));
            r.pebble(Icon::Follow, 13.0, HudAction::Follow(id), if c.hud.follow == Some(id) { Peb::On } else { Peb::Idle }).tip(Tip::name("Follow"));
        });
    })
}

fn stage_name(st: sim::forestry::TreeStage) -> &'static str {
    use sim::forestry::TreeStage::*;
    match st { Seedling => "Seedling", Young => "Young", Mature => "Mature", Old => "Old", Snag => "Dead" }
}

fn yield_row(u: &mut Ui, sticks: f32, food: f32, tip: Tip) {
    u.row(14.0, |r| {
        for (g, a) in [(sim::resources::ResourceKind::felled().yields()[0], sticks), (Good::food(), food)] {
            if a > 0.0 { r.row(4.0, |x| { x.icon(good_icon(g), 16.0, good_color(g)); x.bold(num(a), 13.0, INK); }); }
        }
    }).tip(tip);
}

fn resource(c: &Ctx, id: sim::resources::ResourceId) -> Option<VNode> {
    use sim::forestry::{self, TreeStage};
    let (s, r, sel) = (c.s, c.s.resources.get(id)?, Selection::Tree(id));
    let ti = resource_icon(sim::resources::ResourceKind::tree());
    if r.kind.is_tree() {
        let (sp, stage, drown) = (r.species?, r.stage()?, forestry::drowning(s, r));
        let mut tip = Tip::titled(Some((ti, GOOD)), sp.name()).line(stage_name(stage), "");
        if stage != TreeStage::Snag { tip = tip.line("Age", format!("{:.1} y", r.age_days / sim::calendar::YEAR_DAYS as f32)); }
        let name = if stage == TreeStage::Snag { format!("Dead {}", sp.name().to_lowercase()) } else { sp.name().into() };
        return Some(frame(c, sel, ti, if stage == TreeStage::Snag { MUTED } else { GOOD }, name, drown.map(|d| 1.0 - d), tip, |u| {
            if let Some((next, left)) = forestry::days_to_next_stage(s, r) {
                let (from, to) = match stage { TreeStage::Seedling => (0.0, forestry::YOUNG_AGE), TreeStage::Young => (forestry::YOUNG_AGE, forestry::MATURE_AGE), _ => (forestry::MATURE_AGE, forestry::OLD_AGE) };
                let mut grow = Tip::name(format!("{} in", stage_name(next))).with_value(days(left));
                for (label, f) in [("Species", sp.spec().growth), ("Ground", forestry::ground_growth(s, r.cell))] {
                    if (f - 1.0).abs() > 0.005 { grow = grow.line(label, format!("×{}", num(f))); }
                }
                if sim::calendar::Season::at(s.time_days) == sim::calendar::Season::Winter { grow = grow.line("Winter", "×0"); }
                u.row(8.0, |x| { x.icon(Icon::Hourglass, 16.0, MUTED); x.ticks((r.age_days - from) / (to - from), 10, 170.0, 12.0, GOOD); x.spacer(); x.bold(days(left), 13.0, INK); }).tip(grow);
            }
            if let Some(&(who, day)) = s.forestry.planted.get(&id) {
                let name = s.colony.beaver(who).map(|b| b.name.clone());
                u.row(8.0, |x| {
                    x.icon(good_icon(Good::seedling()), 16.0, GOOD);
                    match &name {
                        Some(n) => { x.token(Icon::Beaver, 16.0, None).action(HudAction::GoTo(super::NoticeAt::Beaver(who))).tip(Tip::name(n.clone()).line("Show", "")); x.bold(n.clone(), 13.0, INK); }
                        None => { x.icon(Icon::Beaver, 16.0, MUTED); }
                    }
                    x.spacer();
                    x.bold(days((s.time_days - day) as f32), 13.0, MUTED);
                }).tip(Tip::name("Planted").line("By", name.unwrap_or_else(|| "a beaver now gone".into())).line("Ago", days((s.time_days - day) as f32)));
            }
            if let Some((logs, branches, bark)) = stage.fell_yield(sp) {
                yield_row(u, logs + branches, bark, Tip::name("Felled now").line(sim::resources::ResourceKind::felled().yields()[0].name(), num(logs + branches)).line(Good::food().name(), num(bark)).line("Felling work", format!("×{}", num(sp.spec().fell_work))));
            } else {
                u.row(8.0, |x| { x.icon(ti, 16.0, MUTED); }).tip(Tip::name("Too young to fell"));
            }
            let (depth, limit) = (s.water.depth.get(r.cell.0, r.cell.1).copied().unwrap_or(0.0), sp.spec().max_depth);
            let mut wtip = Tip::name(if drown.is_some() { "Drowning" } else { "Water" }).line("Here", format!("{depth:.2} m")).line("Stands up to", format!("{} m", num(limit)));
            if let Some(d) = drown { wtip = wtip.line("Dies in", days((1.0 - d) * forestry::drown_days(stage, sp))); }
            u.row(8.0, |x| {
                match drown {
                    Some(d) => { x.icon(Icon::Flood, 16.0, BAD); x.ticks(d, 10, 170.0, 12.0, BAD); }
                    None => { x.icon(Icon::Drop, 16.0, WATER); x.ticks((depth / limit).min(1.0), 10, 170.0, 12.0, WATER); }
                }
                x.spacer();
                x.bold(format!("{} m", num(limit)), 13.0, INK);
            }).tip(wtip);
        }));
    }
    if r.kind.is_felled() {
        let name = r.species.map_or_else(|| resource_name(r.kind).into(), |t| format!("Felled {}", t.name().to_lowercase()));
        return Some(frame(c, sel, ti, MUD, name, None, Tip::name(resource_name(r.kind)), |u| yield_row(u, r.amount + r.branches, r.bark, Tip::name("Left to gather"))));
    }
    let good = r.kind.yields().first().copied().unwrap_or(Good::food());
    let mut amount = Tip::name(good.name()).with_value(format!("{} / {}", num(r.amount), num(r.max_amount)));
    if r.kind.is_food_plant() {
        let growth = sim::calendar::vegetation_growth(s.time_days);
        amount = amount.line("A day", format!("{:+.2}", sim::resources::plant_regrow(r, growth) - sim::resources::plant_decay(growth) * r.amount));
        if !r.wet { amount = amount.line("Dry ground", "×0"); } else if (growth - 1.0).abs() > 0.005 { amount = amount.line("Season", format!("×{growth:.2}")); }
    } else if r.kind.flowing() {
        amount = if r.wet { amount.line("A day", format!("+{:.2}", sim::resources::RIVER_MUD_PER_DAY)) } else { amount.line("Not flowing", "×0") };
    }
    let name = r.plant.map_or(resource_name(r.kind), |p| p.name());
    Some(frame(c, sel, resource_icon(r.kind), good_color(good), name.into(), None, Tip::name(name), |u| {
        u.row(8.0, |x| { x.icon(good_icon(good), 16.0, good_color(good)); x.ticks(r.amount / r.max_amount.max(0.01), 10, 170.0, 12.0, ACCENT); x.spacer(); x.bold(num(r.amount), 13.0, INK); }).tip(amount);
    }))
}

fn line(u: &mut Ui, icon: Icon, col: Color, words: impl Into<String>, value: String) -> &mut VNode {
    u.row(8.0, |x| { x.icon(icon, 16.0, col); x.text(words, 13.0, INK); x.spacer(); x.bold(value, 13.0, INK); })
}

fn animal(c: &Ctx, id: u32) -> Option<VNode> {
    let (s, a) = (c.s, c.s.wildlife.get(id)?);
    let ((icon, doing), name) = (animal_doing(a), animal_name(a.kind));
    let tip = Tip::titled(Some((animal_icon(a.kind), INK)), name).line(if a.kind.prey() { "Prey for wolves" } else { "Flies from wolves" }, "");
    Some(frame(c, Selection::Animal(id), animal_icon(a.kind), FUR, name.into(), None, tip, |u| {
        line(u, icon, MUTED, doing, if a.pace > 0.0 { format!("{}/d", num(a.pace)) } else { String::new() }).tip(Tip::name(doing).line("Cells a day", num(a.pace)));
        line(u, good_icon(Good::food()), good_color(Good::food()), "Eats", format!("{}/d", num(a.kind.appetite()))).tip(Tip::name("Eats while feeding").line("Food a day", num(a.kind.appetite())));
        if !matches!(a.doing, sim::wildlife::Doing::Leave | sim::wildlife::Doing::Caught) {
            line(u, Icon::Hourglass, MUTED, "Moves on in", days((a.leave_by - s.time_days) as f32)).tip(Tip::name("Wanders off the map"));
        }
    }))
}

fn wolf(c: &Ctx, id: u32) -> Option<VNode> {
    use sim::predators::{Prey, WolfState};
    let (s, w) = (c.s, c.s.predators.wolf(id)?);
    let beaver = |id| s.colony.beaver(id).map_or_else(|| "a beaver".into(), |b| b.name.clone());
    let prey = |id| s.wildlife.get(id).map_or("animal", |a| animal_name(a.kind)).to_lowercase();
    let (icon, col, doing) = match w.state {
        WolfState::Roam => (Icon::Walk, MUTED, "Roaming".into()),
        WolfState::Pursue { prey: Prey::Beaver(target), .. } => (Icon::Fast2, BAD, format!("Charging {}", beaver(target))),
        WolfState::Attack { prey: Prey::Beaver(target), .. } => (Icon::Cross, BAD, format!("Killing {}", beaver(target))),
        WolfState::Pursue { prey: Prey::Animal(animal), .. } => (Icon::Fast2, WARN, format!("Hunting a {}", prey(animal))),
        WolfState::Attack { prey: Prey::Animal(animal), .. } => (good_icon(Good::food()), WARN, format!("Eating a {}", prey(animal))),
        WolfState::Leave => (Icon::Walk, MUTED, "Leaving".into()),
    };
    Some(frame(c, Selection::Wolf(id), Icon::Wolf, BAD, "Wolf".into(), None, Tip::titled(Some((Icon::Wolf, BAD)), "Wolf").line("Hunts beavers on land", ""), |u| {
        line(u, icon, col, doing, if w.pace > 0.0 { format!("{}/d", num(w.pace)) } else { String::new() });
        if w.state != WolfState::Leave { line(u, Icon::Hourglass, MUTED, "Leaves in", days((w.leave_by - s.time_days) as f32)).tip(Tip::name("Heads off the map")); }
        line(u, Icon::Focus, MUTED, "Sees", format!("{} cells", num(sim::predators::WOLF_SIGHT))).tip(Tip::name("Sight").line("Circle on the map", ""));
    }))
}
