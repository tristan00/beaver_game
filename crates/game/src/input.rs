use crate::camera::OrbitCamera;
use crate::flow::{height_at, world_cell};
use crate::hud::PointerOverUi;
use crate::state::Sim;
use bevy::input::mouse::{AccumulatedMouseMotion, AccumulatedMouseScroll, MouseScrollUnit};
use bevy::prelude::*;

pub struct InputPlugin;

impl Plugin for InputPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PointerOverUi>().add_systems(Update, orbit_input);
    }
}

const PAN_PER_DISTANCE: f32 = 0.9;
/// Cursor within this many pixels of a window edge pans the camera that way.
const EDGE_PX: f32 = 8.0;

/// Pan direction (forward, right) from the cursor touching a window edge.
fn edge_pan(window: &Window) -> (f32, f32) {
    let Some(p) = window.cursor_position().filter(|_| window.focused) else { return (0.0, 0.0) };
    let (w, h) = (window.width(), window.height());
    let axis = |v: f32, max: f32| if v <= EDGE_PX { -1.0 } else if v >= max - EDGE_PX { 1.0 } else { 0.0 };
    (-axis(p.y, h), axis(p.x, w))
}

pub fn pan_vector(yaw: f32, forward: f32, right: f32) -> Vec2 {
    let fwd = Vec2::new(-yaw.sin(), -yaw.cos());
    let side = Vec2::new(fwd.y, -fwd.x) * -1.0;
    (fwd * forward + side * right).normalize_or_zero()
}

#[allow(clippy::too_many_arguments)]
fn orbit_input(
    buttons: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
    motion: Res<AccumulatedMouseMotion>,
    scroll: Res<AccumulatedMouseScroll>,
    time: Res<Time>,
    over_ui: Res<PointerOverUi>,
    sim: Res<Sim>,
    windows: Query<&Window, With<bevy::window::PrimaryWindow>>,
    menu: Option<Res<crate::hud::MenuOpen>>,
    mut orbit: ResMut<OrbitCamera>,
    (flow, hud): (Option<ResMut<crate::flow::Flow>>, Option<ResMut<crate::hud::HudState>>),
) {
    if menu.as_ref().is_some_and(|m| m.0) {
        return;
    }
    let mut next = *orbit;
    if buttons.pressed(MouseButton::Middle) {
        next.yaw -= motion.delta.x * 0.006;
        next.pitch += motion.delta.y * 0.006;
    }
    let spin = time.delta_secs() * 1.5;
    if keys.pressed(KeyCode::KeyQ) { next.yaw -= spin; }
    if keys.pressed(KeyCode::KeyE) { next.yaw += spin; }
    let held = |a: KeyCode, b: KeyCode| (keys.pressed(a) || keys.pressed(b)) as i32 as f32;
    let (edge_fwd, edge_right) = match windows.single() {
        Ok(w) if !buttons.pressed(MouseButton::Middle) => edge_pan(w),
        _ => (0.0, 0.0),
    };
    let forward = (held(KeyCode::KeyW, KeyCode::ArrowUp) - held(KeyCode::KeyS, KeyCode::ArrowDown) + edge_fwd).clamp(-1.0, 1.0);
    let right = (held(KeyCode::KeyD, KeyCode::ArrowRight) - held(KeyCode::KeyA, KeyCode::ArrowLeft) + edge_right).clamp(-1.0, 1.0);
    let pan = pan_vector(next.yaw, forward, right) * PAN_PER_DISTANCE * next.distance * time.delta_secs();
    let turned = next.yaw != orbit.yaw || next.pitch != orbit.pitch;
    if pan != Vec2::ZERO || turned {
        if let Some(mut flow) = flow {
            if flow.focus.is_some() {
                flow.focus = None;
            }
        }
        if let Some(mut hud) = hud {
            if hud.follow.is_some() {
                hud.follow = None;
            }
        }
    }
    if pan != Vec2::ZERO {
        let h = &sim.0.height;
        let half = h.width() as f32 / 2.0;
        next.target.x = (next.target.x + pan.x).clamp(-half, half);
        next.target.z = (next.target.z + pan.y).clamp(-half, half);
        let (cx, cy) = world_cell(h, next.target);
        next.target.y = height_at(h, cx, cy);
    }
    let ctrl = keys.pressed(KeyCode::ControlLeft) || keys.pressed(KeyCode::ControlRight);
    if !over_ui.0 && !ctrl {
        let lines = match scroll.unit {
            MouseScrollUnit::Line => scroll.delta.y,
            MouseScrollUnit::Pixel => scroll.delta.y / 40.0,
        };
        next.distance *= 0.9f32.powf(lines);
    }
    next.clamp();
    if next.yaw != orbit.yaw || next.pitch != orbit.pitch || next.distance != orbit.distance || next.target != orbit.target {
        *orbit = next;
    }
}
