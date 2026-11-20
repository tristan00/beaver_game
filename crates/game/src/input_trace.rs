//! Records only events delivered to this focused game window. Enable with BC_INPUT_LOG.
use bevy::{input::{keyboard::KeyboardInput, mouse::{MouseButtonInput, MouseWheel}}, prelude::*, window::PrimaryWindow};

pub struct InputTracePlugin;
impl Plugin for InputTracePlugin {
    fn build(&self, app: &mut App) {
        if app.world().resource::<crate::cli::Cli>().screenshot.is_some() && std::env::var_os("BC_HUD").is_none() { return; }
        if sim::trace::enabled() { app.add_systems(Last, record); }
    }
}

#[allow(clippy::too_many_arguments)]
fn record(
    mut keys: MessageReader<KeyboardInput>, mut buttons: MessageReader<MouseButtonInput>,
    mut wheel: MessageReader<MouseWheel>, windows: Query<&Window, With<PrimaryWindow>>,
    time: Res<Time<Real>>, sim: Res<crate::state::Sim>, clock: Res<crate::sim_driver::SimClock>,
    hud: Res<crate::hud::HudState>, selection: Res<crate::flow::Selection>,
    hover: Res<crate::hud::HoverCell>, over: Res<crate::hud::PointerOverUi>,
    mut pointer: Local<(Option<Vec2>, f64)>,
) {
    let Ok(window) = windows.single() else { return };
    if !window.focused { keys.clear(); buttons.clear(); wheel.clear(); return; }
    let mut changed = false;
    for key in keys.read() { sim::trace::event(format!("KEY {:?} {:?} repeat={}", key.key_code, key.state, key.repeat)); changed = true; }
    for button in buttons.read() { sim::trace::event(format!("MOUSE {:?} {:?} cursor={:?}", button.button, button.state, window.cursor_position())); changed = true; }
    for scroll in wheel.read() { sim::trace::event(format!("WHEEL x={} y={} unit={:?}", scroll.x, scroll.y, scroll.unit)); changed = true; }
    let cursor = window.cursor_position();
    if cursor != pointer.0 && time.elapsed_secs_f64() - pointer.1 >= 0.1 {
        sim::trace::event(format!("POINTER {cursor:?}"));
        *pointer = (cursor, time.elapsed_secs_f64());
    }
    if changed {
        sim::trace::event(format!("AFTER_FRAME step={} day={:.4} paused={} speed={:?} over_ui={} cell={:?} selection={:?} selected={:?} tool={:?} order={:?}",
            sim.0.steps, sim.0.time_days, clock.paused, clock.speed, over.0, hover.0, *selection, hud.selected, hud.tool, hud.order_mode));
    }
}
