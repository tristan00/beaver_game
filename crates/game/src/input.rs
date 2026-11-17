//! Mouse/keyboard input: right/middle-drag orbits, wheel zooms, Q/E rotate.
use crate::camera::OrbitCamera;
use bevy::input::mouse::{AccumulatedMouseMotion, AccumulatedMouseScroll, MouseScrollUnit};
use crate::flow_view::UiLayout;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;

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
    layout: Res<UiLayout>,
    windows: Query<&Window, With<PrimaryWindow>>,
    mut dragging: Local<bool>,
) {
    let over_ui = windows.single().ok().and_then(|w| w.cursor_position()).is_some_and(|c| layout.over_ui(c));
    // a drag that starts on the world keeps orbiting; one that starts on a panel never does
    if buttons.just_pressed(MouseButton::Left) || buttons.just_pressed(MouseButton::Right) || buttons.just_pressed(MouseButton::Middle) {
        *dragging = !over_ui;
    }
    if !(buttons.pressed(MouseButton::Left) || buttons.pressed(MouseButton::Right) || buttons.pressed(MouseButton::Middle)) {
        *dragging = false;
    }
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
    if !over_ui {
        next.distance *= 0.9f32.powf(lines);
    }
    next.clamp();
    // Only touch the resource when something changed so change detection stays meaningful.
    if next.yaw != orbit.yaw || next.pitch != orbit.pitch || next.distance != orbit.distance {
        *orbit = next;
    }
}
