//! What the dam and canal panels share. Both are laid out the same way, top to bottom: header,
//! the status line with its one fix, tabs, then Water, Work, Build (Dig), Gates and Upkeep.
//! A button that can't be used now stays where it is, faded, and its tooltip says why.
use super::panels::Ctx;
use super::ui::*;
use super::words::num;
use super::{HudAction, REMOVE_ARM_SECS};
use crate::flow::Selection;
use crate::icons::Icon;
use bevy::prelude::*;
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

/// Name, condition, pin and close.
#[allow(clippy::too_many_arguments)]
pub fn header(c: &Ctx, u: &mut Ui, id: BuildingId, title: &str, selected_here: bool, health: f32, tip: Tip, portrait: impl FnOnce(&mut Ui)) {
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
            y.pebble(Icon::Pin, 10.0, HudAction::Pin(sel), if pinned { Peb::On } else { Peb::Bare }).tip(Tip::name(if pinned { "Unpin" } else { "Pin" }));
            y.close_button(HudAction::ClosePanel(sel, selected_here), selected_here);
        })
        .style(|n| n.align_self = AlignSelf::FlexStart);
    });
}

/// The cells picked on this dam or canal: a quiet mark saying how to pick, or a button with
/// the count that drops them.
pub fn picks(u: &mut Ui, n: usize, what: &str) {
    if n == 0 {
        u.icon(Icon::Focus, 14.0, MUTED).tip(Tip::name("Pick cells").line(format!("Click a cell on the {what}"), "").line("Shift", "add more"));
    } else {
        let word = format!("{n} picked");
        u.small_button(Some(Icon::Focus), Some(&word), Some(HudAction::ClearPicks), Tip::name("Unpick all"), Small::On);
    }
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
        (MUTED, Tip::name("No gate yet").line("Pick a setting to build one", "").line("Sticks", num(sim::canals::GATE_STICKS)))
    };
    let row = u.row(5.0, |r| {
        r.icon(if lock { Icon::LockGates } else { Icon::Lock }, 15.0, col).tip(tip);
        for setting in [GateSetting::Shut, GateSetting::Level(level_at), GateSetting::Open] {
            let on = exists && std::mem::discriminant(&wanted) == std::mem::discriminant(&setting);
            let tip = match setting {
                GateSetting::Shut => Tip::name("Shut").line("No water passes", ""),
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
    let why = if !locks::unlocked(s) {
        Some("Needs a Harbor")
    } else if !g.built {
        Some("Build the gate first")
    } else if locks::check_room(s, id, g.cell).is_err() {
        Some("No room for a lock here")
    } else {
        None
    };
    let tip = what.line("Sticks", num(locks::LOCK_STICKS)).line("Clay", num(locks::LOCK_CLAY));
    match why {
        Some(w) => r.small_button(Some(Icon::LockGates), None, None, tip.line(w, ""), Small::Idle),
        None => r.small_button(Some(Icon::LockGates), None, Some(HudAction::UpgradeLock(id, g.cell)), tip, Small::Idle),
    };
}

/// Maintainers keep it up, or not.
pub fn maintain_button(r: &mut Ui, on: bool, action: HudAction, what: &str, hint: bool) {
    let look = if hint { Small::Hint } else if on { Small::On } else { Small::Idle };
    r.small_button(Some(Icon::Hammer), Some("Maintain"), Some(action), Tip::name("Maintain").line(if on { "On" } else { "Off" }, "").line(what, ""), look);
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
