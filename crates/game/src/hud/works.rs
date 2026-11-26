//! What the dam and canal panels share. Both are laid out the same way, top to bottom: header
//! (history behind its hourglass), the side view with the state drawn on it and a chip for each
//! gate and lock, the build buttons, the strip of the gate picked, and the footer.
//! A button that can't be used now stays where it is, faded, and its tooltip says why.
use super::panels::Ctx;
use super::ui::*;
use super::words::num;
use super::{DamTab, HudAction, Opening, REMOVE_ARM_SECS};
use crate::flow::Selection;
use crate::icons::Icon;
use bevy::prelude::*;
use crate::flow::Cell;
use sim::buildings::BuildingId;
use sim::GateSetting;

pub const WOOD: Color = Color::srgb(0.95, 0.85, 0.66);
pub const DEEP: Color = Color::srgb(0.12, 0.54, 0.72);
pub const SHALLOW: Color = Color::srgb(0.44, 0.83, 0.88);
/// The ground under a side view.
pub const EARTH: Color = Color::srgb(0.29, 0.23, 0.16);
/// The width a side view or chart takes inside the panel.
pub const VIEW_W: f32 = 294.0;

/// One colour scale for a dam's or a canal's condition.
pub fn condition_colour(h: f32) -> Color {
    if h < 0.5 { BAD } else if h < 0.8 { WARN } else { GOOD }
}

/// One history sparkline row for a dam or canal panel.
pub fn history_row(u: &mut Ui, icon: Icon, col: Color, name: &str, days: usize, max_days: usize, fracs: impl IntoIterator<Item = f32>) {
    let w = ((VIEW_W - 20.0) / max_days as f32).floor().max(2.0);
    u.row(6.0, |r| {
        r.icon(icon, 13.0, col);
        r.sparkline(fracs, w - 1.0, 36.0, col);
    })
    .tip(Tip::name(name).line("Days", format!("{days}")));
}

/// Standard right-hand panel shell shared by dams and canals.
#[allow(clippy::too_many_arguments)]
pub fn works_panel(
    c: &Ctx,
    id: BuildingId,
    title: &str,
    selected_here: bool,
    health: f32,
    cond_tip: Tip,
    portrait: impl FnOnce(&mut Ui),
    body: impl FnOnce(&mut Ui),
) -> VNode {
    let mut ui = Ui::default();
    ui.node(Node { width: Val::Px(super::panels::RIGHT_W), flex_direction: FlexDirection::Column, row_gap: Val::Px(8.0), ..default() }, |u| {
        header(c, u, id, title, selected_here, health, cond_tip, portrait);
        body(u);
    });
    let mut v = ui.nodes.pop().expect("panel node");
    panel(&mut v);
    v.node.padding = UiRect::all(Val::Px(12.0));
    v
}

/// A box placed by its top left corner inside a side view.
pub fn abs_box(left: f32, top: f32, w: f32, h: f32) -> Node {
    Node { position_type: PositionType::Absolute, left: Val::Px(left), top: Val::Px(top), width: Val::Px(w.max(0.0)), height: Val::Px(h.max(0.0)), ..default() }
}

/// A short dashed line across a side view.
pub fn dashes(u: &mut Ui, from: f32, to: f32, top: f32, col: Color) {
    let mut x = from;
    while x < to {
        u.add(VNode::new(abs_box(x, top, (to - x).min(6.0), 1.0))).bg(col);
        x += 10.0;
    }
}

/// A word on a side view, its right end at `right`.
pub fn label_left_of(u: &mut Ui, right: f32, top: f32, text: String, col: Color, tip: Tip) {
    u.node(Node { position_type: PositionType::Absolute, left: Val::Px(0.0), width: Val::Px(right.max(0.0)), top: Val::Px(top), justify_content: JustifyContent::FlexEnd, ..default() }, |l| {
        l.bold(text, 12.0, col).tip(tip);
    });
}

/// Name, condition, history, pin and close.
#[allow(clippy::too_many_arguments)]
pub fn header(c: &Ctx, u: &mut Ui, id: BuildingId, title: &str, selected_here: bool, health: f32, tip: Tip, portrait: impl FnOnce(&mut Ui)) {
    let history = c.hud.dam_tab == DamTab::History;
    let sel = Selection::Site(id);
    let pinned = c.hud.pinned.contains(&sel);
    u.row(10.0, |r| {
        r.node(Node { width: Val::Px(52.0), height: Val::Px(52.0), padding: UiRect::all(Val::Px(5.0)), flex_shrink: 0.0, ..default() }, portrait).skin(Skin::Portrait);
        r.col(7.0, |x| {
            x.bold(title, 16.0, INK);
            x.row(5.0, |y| {
                y.pips(health, condition_colour(health));
                y.text(format!("{:.0}%", health * 100.0), 11.0, MUTED);
            })
            .tip(tip);
        });
        r.spacer();
        r.row(4.0, |y| {
            let next = if history { DamTab::Status } else { DamTab::History };
            y.pebble(Icon::Hourglass, 11.0, HudAction::DamTab(next), if history { Peb::On } else { Peb::Bare }).tip(Tip::name(if history { "Back" } else { "History" }));
            y.pebble(Icon::Pin, 10.0, HudAction::Pin(sel), if pinned { Peb::On } else { Peb::Bare }).tip(Tip::name(if pinned { "Unpin" } else { "Pin" }));
            y.close_button(HudAction::ClosePanel(sel, selected_here), selected_here);
        })
        .style(|n| n.align_self = AlignSelf::FlexStart);
    });
}

fn setting_name(st: GateSetting) -> &'static str {
    match st {
        GateSetting::Shut => "Shut",
        GateSetting::Level(_) => "Level",
        GateSetting::Open => "Open",
    }
}

/// One gate: its state, Shut / Level / Open, the sill while it holds a level, and its lock.
/// `ordered`: the gate stands in the works' list (a dam's), so its setting shows even before
/// it is built. `usable`: why the settings can't be touched yet, if they can't.
pub fn gate_row(u: &mut Ui, s: &sim::SimState, id: BuildingId, g: &sim::gates::Gate, pond: Option<f32>, ordered: bool, usable: Result<(), &str>) {
    let brought = supplied(s, id, g.cell);
    let wanted = g.wanted();
    let exists = ordered || g.built || g.pending.is_some();
    let level_at = match wanted {
        GateSetting::Level(e) => e,
        _ => pond.unwrap_or(g.sill()),
    };
    let lock = sim::locks::is_lock(s, g.cell);
    let (col, tip) = if g.built && g.pending.is_none() {
        (WOOD, Tip::name(setting_name(g.setting)).line("Sill", format!("{} m", num(g.sill()))))
    } else if g.built {
        (WARN, Tip::name(format!("To be {}", setting_name(wanted))).line("A builder will set it", ""))
    } else if exists {
        (WARN, Tip::name("Not built yet").line("A builder brings its sticks", "").line("Then", setting_name(wanted)))
    } else {
        (MUTED, super::words::cost_lines(Tip::name("No gate yet").line("Pick a setting to build one", ""), &sim::content::content().works.gate))
    };
    let row = u.row(5.0, |r| {
        r.icon(if lock { Icon::LockGates } else { Icon::Lock }, 15.0, col).tip(tip);
        if let Some((got, need)) = brought {
            r.bold(format!("{}/{}", got.floor() as i64, need.round() as i64), 12.0, WARN).tip(Tip::name("Brought").line("Of what it needs", ""));
        }
        for setting in [GateSetting::Shut, GateSetting::Level(level_at), GateSetting::Open] {
            let on = exists && std::mem::discriminant(&wanted) == std::mem::discriminant(&setting);
            let tip = match setting {
                GateSetting::Shut => Tip::name("Shut").line("No water gets through", ""),
                GateSetting::Level(_) => Tip::name("Level").line("Holds the pond at the sill", "").line("Sill", format!("{} m", num(g.sill_of(setting)))),
                GateSetting::Open => Tip::name("Open").line("Lets the pond down", "").line("Floor", format!("{} m", num(g.floor))),
            };
            let look = if on { Small::On } else { Small::Idle };
            match usable {
                Ok(()) => r.small_button(None, Some(setting_name(setting)), Some(HudAction::SetGate(id, g.cell, setting)), tip, look),
                Err(why) => r.small_button(None, Some(setting_name(setting)), None, tip.line(why, ""), look),
            };
        }
        if exists && matches!(wanted, GateSetting::Level(_)) {
            sill_steps(r, id, g, level_at);
        }
        r.spacer();
        lock_button(r, s, id, g);
        if exists {
            let what = if lock || sim::locks::pending(s, g.cell) { "Remove the gate and its lock" } else { "Remove the gate" };
            r.small_button(Some(Icon::Remove), None, Some(HudAction::RemoveGate(id, g.cell)), Tip::name(what).line("What was brought goes back", ""), Small::Danger);
        }
    });
    row.node.padding = UiRect::axes(Val::Px(6.0), Val::Px(4.0));
    row.node.border_radius = BorderRadius::all(Val::Px(6.0));
    row.bg(FAINT);
}

/// Lower and raise a Level gate's sill a step from where it stands; at its floor or top the
/// button stays, faded.
fn sill_steps(r: &mut Ui, id: BuildingId, g: &sim::gates::Gate, level_at: f32) {
    let sill = g.sill_of(GateSetting::Level(level_at));
    let step = sim::canals::GATE_STEP;
    let down = sill > g.floor + 1e-3;
    let up = sill < g.top - 1e-3;
    let cm = format!("{:.0} cm", step * 100.0);
    r.small_button(Some(Icon::Minus), None, down.then_some(HudAction::SetGate(id, g.cell, GateSetting::Level(sill - step))), Tip::name("Lower the sill").line(if down { cm.as_str() } else { "As low as it goes" }, ""), Small::Idle);
    r.bold(format!("{} m", num(sill)), 12.0, INK).tip(Tip::name("Sill").line("The pond is held here", ""));
    r.small_button(Some(Icon::Plus), None, up.then_some(HudAction::SetGate(id, g.cell, GateSetting::Level(sill + step))), Tip::name("Raise the sill").line(if up { cm.as_str() } else { "As high as it goes" }, ""), Small::Idle);
}

/// A gate's lock: one already, one coming, the upgrade, or why it can't be had yet.
fn lock_button(r: &mut Ui, s: &sim::SimState, id: BuildingId, g: &sim::gates::Gate) {
    use sim::locks;
    let what = Tip::name("Lock").line("Beavers and rafts get past", "");
    if locks::is_lock(s, g.cell) {
        r.icon(Icon::LockGates, 15.0, GOOD).tip(what);
        return;
    }
    if locks::pending(s, g.cell) {
        r.icon(Icon::LockGates, 15.0, WARN).tip(what.line("A builder will build it", ""));
        return;
    }
    let tip = super::words::cost_lines(what, &locks::order_cost(s, id, g.cell));
    match locks::check_order(s, id, g.cell) {
        Ok(()) => r.small_button(Some(Icon::LockGates), None, Some(HudAction::UpgradeLock(id, g.cell)), tip, Small::Idle),
        Err(why) => r.small_button(Some(Icon::LockGates), None, None, tip.line(cap(&why), ""), Small::Idle),
    };
}

/// Menders keep it up, or not.
pub fn upkeep_button(r: &mut Ui, on: bool, action: HudAction, what: &str, hint: bool) {
    let look = if hint { Small::Hint } else if on { Small::On } else { Small::Idle };
    r.small_button(Some(Icon::Hammer), Some("Mend"), Some(action), Tip::name("Mend").line(if on { "On" } else { "Off" }, "").line(what, ""), look);
}

pub fn pause_button(r: &mut Ui, s: &sim::SimState, id: BuildingId) {
    let paused = s.work.place(id).is_some_and(|p| p.paused);
    r.small_button(Some(if paused { Icon::Play } else { Icon::Pause }), None, Some(HudAction::Paused(id, !paused)), Tip::name(if paused { "Resume work" } else { "Pause work" }), if paused { Small::On } else { Small::Idle });
}

/// Take the dam or canal away: red, apart from the rest, and only on a second click.
pub fn remove_button(c: &Ctx, r: &mut Ui, id: BuildingId) {
    let s = c.s;
    let Some(b) = s.buildings.building(id) else { return };
    let dam = b.dam.is_some();
    let what = if dam { "dam" } else { "canal" };
    let armed = c.hud.remove_armed.is_some_and(|(x, t)| x == id && c.now - t <= REMOVE_ARM_SECS);
    let finished = s.finished(b);
    let tip = if armed {
        Tip::name("Click again")
    } else {
        let effect = match (dam, finished) {
            (true, true) => "The whole dam comes down",
            (true, false) => "Built cells come down",
            (false, _) => "Dug ground stays dug",
        };
        Tip::name(format!("{} the {what}", if finished { "Remove" } else { "Cancel" })).line(effect, "")
    };
    r.small_button(Some(Icon::Remove), armed.then_some("Sure?"), Some(HudAction::RemoveWorks(id)), tip, if armed { Small::Armed } else { Small::Danger });
}

/// Upper-case the first letter of a sim message for a tooltip.
pub fn cap(s: &str) -> String {
    let mut c = s.chars();
    c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default()
}

/// What has been brought to a gate for the work ordered there, against what it needs; None
/// when it needs nothing brought.
pub fn supplied(s: &sim::SimState, id: BuildingId, cell: Cell) -> Option<(f32, f32)> {
    let spot = sim::supply::Spot { building: id, cell };
    let needs = sim::supply::needs(s, spot);
    let need: f32 = needs.iter().map(|a| a.1).sum();
    let got: f32 = needs.iter().map(|&(g, a)| sim::supply::delivered(s, spot, g).min(a)).sum();
    (need > 0.0).then_some((got, need))
}

/// Why a gate or lock can't go on this cell of a dam.
pub fn opening_fits(s: &sim::SimState, id: BuildingId, cell: Cell, o: Opening) -> Result<(), String> {
    match o {
        Opening::Gate => sim::dams::plan_gate(s, id, cell).map(|_| ()).map_err(|e| cap(&e)),
        Opening::Lock => sim::locks::check_order(s, id, cell).map_err(|e| cap(&e)),
    }
}

/// The build button for a gate or a lock on a dam: lit while it is being placed, faded with
/// the reason when it fits nowhere on the dam.
pub fn opening_button(c: &Ctx, r: &mut Ui, id: BuildingId, d: &sim::dams::Dam, o: Opening) {
    let s = c.s;
    let fits: Vec<Result<(), String>> = d.line().map(|x| opening_fits(s, id, x.cell, o)).collect();
    let ok = fits.iter().any(|f| f.is_ok());
    let why = fits.iter().find_map(|f| f.clone().err()).unwrap_or_else(|| "Build the dam first".into());
    let on = c.hud.opening == Some((id, o));
    let w = &sim::content::content().works;
    let (icon, word, cost, tip) = match o {
        Opening::Gate => (Icon::Lock, "Gate", w.gate.clone(), Tip::name("Gate").line("Let water through here", "")),
        Opening::Lock => (Icon::LockGates, "Lock", super::words::cost_sum(&w.gate, &w.lock), Tip::name("Lock").line("A gate beavers and rafts get past", "")),
    };
    let tip = super::words::cost_lines(tip, &cost);
    let tip = if ok { tip.line("Click a lit cell", "").line("Shift", "place more").key("Esc") } else { tip.line(why, "") };
    let action = (ok || on).then_some(HudAction::Opening(id, o));
    r.big_button(BigButton { icon, word, cost: super::words::cost_chip(&cost), action, tip, hint: false, on });
}

/// A chip for each gate and lock over a side view: its height, or what has been brought for
/// it; a click shows its settings and takes the camera there. Chips that don't fit merge
/// into the last, with a count.
pub fn chips(c: &Ctx, u: &mut Ui, id: BuildingId, gates: &[sim::gates::Gate], room: f32) {
    let s = c.s;
    const CHIP_W: f32 = 52.0;
    let fit = ((room / (CHIP_W + 4.0)).floor() as usize).max(1);
    let shown = if gates.len() > fit { fit - 1 } else { gates.len() };
    u.node(Node { flex_direction: FlexDirection::Row, column_gap: Val::Px(4.0), ..default() }, |r| {
        for g in &gates[..shown] {
            let lock = sim::locks::has(s, g.cell);
            let brought = supplied(s, id, g.cell);
            let busy = s.agents.by_id.values().any(|a| matches!(a.task, sim::agents::Task::Work { site, cell: Some(cell), kind: sim::agents::WorkKind::Gate, .. } if site == id && cell == g.cell));
            let (text, col) = match brought {
                Some((got, need)) => (format!("{}/{}", got.floor() as i64, need.round() as i64), WARN),
                None => (format!("{} m", num(g.sill() - g.floor)), INK),
            };
            let picked = c.hud.gate_sel == Some((id, g.cell));
            let v = r.row(3.0, |x| {
                x.icon(if lock { Icon::LockGates } else { Icon::Lock }, 12.0, col);
                x.bold(text, 11.0, col);
                if busy {
                    x.icon(Icon::Beaver, 11.0, FUR);
                }
            });
            v.node.height = Val::Px(22.0);
            v.node.padding = UiRect::axes(Val::Px(6.0), Val::Px(0.0));
            v.node.flex_shrink = 0.0;
            v.skin(if picked { Skin::PebbleOn } else { Skin::Pebble });
            let name = if lock { "Lock" } else { "Gate" };
            let tip = match brought {
                Some(_) => Tip::name(name).line("Brought", "of what it needs"),
                None => Tip::name(name).line(setting_name(g.setting), format!("sill {} m", num(g.sill()))),
            };
            v.tip(tip).action(HudAction::GateChip(id, g.cell));
        }
        if gates.len() > shown {
            let rest = &gates[shown..];
            let v = r.row(3.0, |x| {
                x.icon(Icon::Lock, 12.0, INK);
                x.bold(format!("+{}", rest.len()), 11.0, INK);
            });
            v.node.height = Val::Px(22.0);
            v.node.padding = UiRect::axes(Val::Px(6.0), Val::Px(0.0));
            v.skin(Skin::Pebble);
            v.tip(Tip::name(format!("{} more", rest.len())).line("Click", "the next one")).action(HudAction::GateChip(id, rest[0].cell));
        }
    });
}

/// The builders or diggers on the works, as a button to the jobs; marked when work waits for
/// one.
pub fn crew_button(r: &mut Ui, n: usize, icon: Icon, what: &str, hint: bool) {
    let word = format!("{n}");
    r.small_button(Some(icon), Some(&word), Some(HudAction::Colony(Some(super::ColonyTab::Jobs))), Tip::name(what).line("Open the jobs", ""), if hint { Small::Hint } else { Small::Idle });
}
