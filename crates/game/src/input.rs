//! Camera input. Left click belongs to the interface and the world (select, place, draw).
//! Right or middle drag orbits, the wheel zooms (not over a panel, not with Ctrl, which sets dam
//! height), Q/E rotate and WASD or the arrow keys pan across the ground.
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

/// Ground distance panned per second at the default zoom; scales with the camera distance.
const PAN_PER_DISTANCE: f32 = 0.9;

/// Pan direction on the ground for held keys, relative to the camera's yaw (unit or zero).
pub fn pan_vector(yaw: f32, forward: f32, right: f32) -> Vec2 {
    // The camera sits at +dir from the target, so "forward" on screen is -dir on the ground.
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
    mut orbit: ResMut<OrbitCamera>,
) {
    let mut next = *orbit;
    if buttons.pressed(MouseButton::Right) || buttons.pressed(MouseButton::Middle) {
        next.yaw -= motion.delta.x * 0.006;
        next.pitch += motion.delta.y * 0.006;
    }
    let spin = time.delta_secs() * 1.5;
    if keys.pressed(KeyCode::KeyQ) { next.yaw -= spin; }
    if keys.pressed(KeyCode::KeyE) { next.yaw += spin; }
    let held = |a: KeyCode, b: KeyCode| (keys.pressed(a) || keys.pressed(b)) as i32 as f32;
    let forward = held(KeyCode::KeyW, KeyCode::ArrowUp) - held(KeyCode::KeyS, KeyCode::ArrowDown);
    let right = held(KeyCode::KeyD, KeyCode::ArrowRight) - held(KeyCode::KeyA, KeyCode::ArrowLeft);
    let pan = pan_vector(next.yaw, forward, right) * PAN_PER_DISTANCE * next.distance * time.delta_secs();
    if pan != Vec2::ZERO {
        let h = &sim.0.height;
        let half = h.width() as f32 / 2.0;
        next.target.x = (next.target.x + pan.x).clamp(-half, half);
        next.target.z = (next.target.z + pan.y).clamp(-half, half);
        let (cx, cy) = world_cell(h, next.target);
        next.target.y = height_at(h, cx, cy);
    }
    if !over_ui.0 && !keys.pressed(KeyCode::ControlLeft) {
        let lines = match scroll.unit {
            MouseScrollUnit::Line => scroll.delta.y,
            MouseScrollUnit::Pixel => scroll.delta.y / 40.0,
        };
        next.distance *= 0.9f32.powf(lines);
    }
    next.clamp();
    // Only touch the resource when something changed so change detection stays meaningful.
    if next.yaw != orbit.yaw || next.pitch != orbit.pitch || next.distance != orbit.distance || next.target != orbit.target {
        *orbit = next;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn panning_forward_moves_away_from_the_camera() {
        // Yaw 0: the camera sits on +z of the target, so forward is -z.
        let v = pan_vector(0.0, 1.0, 0.0);
        assert!((v - Vec2::new(0.0, -1.0)).length() < 1e-5, "{v:?}");
        let r = pan_vector(0.0, 0.0, 1.0);
        assert!((r - Vec2::new(1.0, 0.0)).length() < 1e-5, "{r:?}");
        assert_eq!(pan_vector(1.0, 0.0, 0.0), Vec2::ZERO);
    }
}
