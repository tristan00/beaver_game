//! Mouse/keyboard input: right/middle-drag orbits, wheel zooms, Q/E rotate.
use crate::camera::OrbitCamera;
use bevy::input::mouse::{AccumulatedMouseMotion, AccumulatedMouseScroll, MouseScrollUnit};
use bevy::prelude::*;

pub struct InputPlugin;

impl Plugin for InputPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, orbit_input);
    }
}

fn orbit_input(
    buttons: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
    motion: Res<AccumulatedMouseMotion>,
    scroll: Res<AccumulatedMouseScroll>,
    time: Res<Time>,
    mut orbit: ResMut<OrbitCamera>,
    mut dragging: Local<bool>,
) {
    let dragging_now = buttons.pressed(MouseButton::Left) || buttons.pressed(MouseButton::Right) || buttons.pressed(MouseButton::Middle);
    *dragging = dragging_now;
    let mut next = *orbit;
    if *dragging {
        next.yaw -= motion.delta.x * 0.006;
        next.pitch += motion.delta.y * 0.006;
    }
    let spin = time.delta_secs() * 1.5;
    if keys.pressed(KeyCode::KeyQ) { next.yaw -= spin; }
    if keys.pressed(KeyCode::KeyE) { next.yaw += spin; }
    let lines = match scroll.unit {
        MouseScrollUnit::Line => scroll.delta.y,
        MouseScrollUnit::Pixel => scroll.delta.y / 40.0,
    };
    next.distance *= 0.9f32.powf(lines);
    next.clamp();
    // Only touch the resource when something changed so change detection stays meaningful.
    if next.yaw != orbit.yaw || next.pitch != orbit.pitch || next.distance != orbit.distance {
        *orbit = next;
    }
}
