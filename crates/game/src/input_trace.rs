//! Records every input of a play session, with what the game saw and decided, to
//! `captures/session-*.log`. F12 saves a screenshot and the last few minutes of that record
//! beside it, so a problem can be replayed from what was pressed and where.
use bevy::{
    input::{keyboard::KeyboardInput, mouse::{MouseButtonInput, MouseWheel}},
    prelude::*,
    render::view::screenshot::{save_to_disk, Screenshot},
    window::{PrimaryWindow, WindowFocused},
};
use std::path::PathBuf;

/// Inputs written beside each screenshot.
const CAPTURE_SECONDS: f64 = 180.0;

pub struct InputTracePlugin;
impl Plugin for InputTracePlugin {
    fn build(&self, app: &mut App) {
        let cli = app.world().resource::<crate::cli::Cli>();
        let automated = cli.screenshot.is_some() || cli.report_dir.is_some();
        if !automated {
            sim::trace::start(&captures().join(format!("session-{}.log", stamp())));
        }
        if sim::trace::enabled() {
            app.add_systems(Last, record).add_systems(Update, capture);
        }
    }
}

pub(crate) fn captures() -> PathBuf {
    std::env::current_dir().unwrap_or_default().join("captures")
}

/// UTC date and time for file names, YYYYMMDD-HHMMSS.
pub(crate) fn stamp() -> String {
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs() as i64;
    let (days, rem) = (secs.div_euclid(86_400), secs.rem_euclid(86_400));
    // Civil date from days since 1970-01-01 (Howard Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}{m:02}{d:02}-{:02}{:02}{:02}", rem / 3600, rem / 60 % 60, rem % 60)
}

/// What the game sees at the cursor and what it would do there, as one line.
#[allow(clippy::too_many_arguments)]
fn scene(
    sim: &sim::SimState, window: &Window, cam: Option<&GlobalTransform>,
    hud: &crate::hud::HudState, selection: &crate::flow::Selection,
    hover: &crate::hud::Hover, over: &crate::hud::PointerOverUi,
    clock: &crate::sim_driver::SimClock,
) -> String {
    let mut s = format!("cursor={:?} over_ui={}", window.cursor_position().map(|p| (p.x.round(), p.y.round())), over.0);
    if let Some((c, p)) = hover.point {
        let h = sim.height.get(c.0, c.1).copied().unwrap_or(0.0);
        let d = sim.water.depth.get(c.0, c.1).copied().unwrap_or(0.0);
        s += &format!(" cell={c:?} at=({:.2},{:.2}) ground={h:.2} depth={d:.2} surface={:.2} explored={}", p.0, p.1, h + d, sim.fog.explored(c));
        if let crate::hud::Tool::Building(kind) = hud.tool {
            let p = crate::build_logic::site_preview(sim, kind, c);
            s += &format!(" place={kind:?}@{:?} verdict={:?}", p.pos, p.verdict);
        }
    } else {
        s += " cell=None";
    }
    s += &format!(" thing={:?} tool={:?}", hover.thing, hud.tool);
    if !hud.corners.is_empty() || !hud.line.is_empty() {
        s += &format!(" corners={:?} line={}..{:?}({} cells) extending={:?}", hud.corners, hud.line.first().map_or(String::new(), |c| format!("{c:?}")), hud.line.last(), hud.line.len(), hud.extending);
    }
    s += &format!(" selection={selection:?} selected={:?} day={:.3} step={} paused={} speed={:?}", hud.selected, sim.time_days, sim.steps, clock.paused, clock.speed);
    if let Some(t) = cam {
        let (p, f) = (t.translation(), t.forward());
        s += &format!(" camera=({:.1},{:.1},{:.1}) looking=({:.2},{:.2},{:.2})", p.x, p.y, p.z, f.x, f.y, f.z);
    }
    s
}

#[allow(clippy::too_many_arguments)]
fn record(
    mut keys: MessageReader<KeyboardInput>, mut buttons: MessageReader<MouseButtonInput>,
    mut wheel: MessageReader<MouseWheel>, mut focus: MessageReader<WindowFocused>,
    windows: Query<&Window, With<PrimaryWindow>>, cams: Query<&GlobalTransform, With<crate::camera::MainCamera>>,
    time: Res<Time<Real>>, sim: Res<crate::state::Sim>, clock: Res<crate::sim_driver::SimClock>,
    hud: Res<crate::hud::HudState>, selection: Res<crate::flow::Selection>,
    hover: Res<crate::hud::Hover>, over: Res<crate::hud::PointerOverUi>,
    held: Res<ButtonInput<KeyCode>>,
    mut last: Local<(Option<Vec2>, f64, Option<String>, String)>,
) {
    for f in focus.read() { sim::trace::event(format!("FOCUS {}", f.focused)); }
    let Ok(window) = windows.single() else { return };
    let cam = cams.single().ok();
    let here = || scene(&sim.0, window, cam, &hud, &selection, &hover, &over, &clock);
    let mods = || {
        let on = |a, b| held.pressed(a) || held.pressed(b);
        [("ctrl", on(KeyCode::ControlLeft, KeyCode::ControlRight)), ("shift", on(KeyCode::ShiftLeft, KeyCode::ShiftRight)), ("alt", on(KeyCode::AltLeft, KeyCode::AltRight))]
            .iter().filter(|m| m.1).map(|m| m.0).collect::<Vec<_>>().join("+")
    };
    let mut acted = false;
    for key in keys.read() {
        if key.repeat { continue; }
        sim::trace::event(format!("KEY {:?} {:?} mods={} | {}", key.key_code, key.state, mods(), here()));
        acted = true;
    }
    for b in buttons.read() {
        sim::trace::event(format!("MOUSE {:?} {:?} mods={} | {}", b.button, b.state, mods(), here()));
        acted = true;
    }
    for w in wheel.read() {
        sim::trace::event(format!("WHEEL x={} y={} unit={:?}", w.x, w.y, w.unit));
    }
    // Pointer travel, at most ten lines a second, so a drag can be followed cell by cell.
    let cursor = window.cursor_position();
    let now = time.elapsed_secs_f64();
    if cursor != last.0 && now - last.1 >= 0.1 {
        sim::trace::event(format!("POINTER {}", here()));
        last.0 = cursor;
        last.1 = now;
    }
    // Every message the game shows, when it first appears.
    let msg = hud.msg.as_ref().map(|m| format!("{:?} {:?} {:?}", m.icon.map(|i| i.0), m.icon2.map(|i| i.0), m.text));
    if msg != last.2 {
        if let Some(m) = &msg { sim::trace::event(format!("MESSAGE {m}")); }
        last.2 = msg;
    }
    // The state an input left behind, once it has been handled.
    let after = format!("tool={:?} selection={:?} corners={:?} line_cells={} extending={:?}", hud.tool, *selection, hud.corners, hud.line.len(), hud.extending);
    if acted || after != last.3 {
        sim::trace::event(format!("STATE {after}"));
        last.3 = after;
    }
}

#[allow(clippy::too_many_arguments)]
fn capture(
    mut commands: Commands, keys: Res<ButtonInput<KeyCode>>, time: Res<Time>,
    windows: Query<&Window, With<PrimaryWindow>>, cams: Query<&GlobalTransform, With<crate::camera::MainCamera>>,
    sim: Res<crate::state::Sim>, clock: Res<crate::sim_driver::SimClock>,
    mut hud: ResMut<crate::hud::HudState>, selection: Res<crate::flow::Selection>,
    hover: Res<crate::hud::Hover>, over: Res<crate::hud::PointerOverUi>,
    mut toast: Local<Option<f32>>,
) {
    let now = time.elapsed_secs();
    // Confirm a moment later, so the picture shows the screen as it was.
    if toast.is_some_and(|at| now >= at) {
        *toast = None;
        hud.msg = Some(crate::hud::Msg { icon: Some((crate::icons::Icon::Check, crate::hud::GOOD)), icon2: None, text: String::new() });
    }
    if !keys.just_pressed(KeyCode::F12) {
        return;
    }
    let Ok(window) = windows.single() else { return };
    let name = format!("shot-{}", stamp());
    let dir = captures();
    let png = dir.join(format!("{name}.png"));
    let here = scene(&sim.0, window, cams.single().ok(), &hud, &selection, &hover, &over, &clock);
    sim::trace::event(format!("SCREENSHOT {} | {here}", png.display()));
    sim::trace::flush();
    let mut text = format!("Screenshot {}\nAt the moment of F12: {here}\n\nLast {CAPTURE_SECONDS:.0} s of input (seconds since the session started):\n", png.display());
    for line in sim::trace::recent(CAPTURE_SECONDS) {
        text.push_str(&line);
        text.push('\n');
    }
    if let Err(e) = std::fs::create_dir_all(&dir).and_then(|_| std::fs::write(dir.join(format!("{name}.txt")), text)) {
        sim::trace::event(format!("CAPTURE_FAILED {e}"));
        return;
    }
    commands.spawn(Screenshot::primary_window()).observe(save_to_disk(png));
    *toast = Some(now + 0.3);
}
