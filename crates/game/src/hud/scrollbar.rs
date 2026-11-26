//! Themed vertical scrollbars for any panel whose content extends past its visible height.
//!
//! Styled to match the carved-plate HUD: a recessed dark groove track with a warm bark thumb
//! that brightens to amber on hover or drag, and hides automatically when everything fits.

use super::ui::{ACCENT, BARK, GROOVE};
use bevy::input::mouse::{AccumulatedMouseScroll, MouseScrollUnit};
use bevy::prelude::*;
use bevy::ui::{FocusPolicy, RelativeCursorPosition, UiGlobalTransform};
use bevy::window::PrimaryWindow;

pub const TRACK_W: f32 = 8.0;
const THUMB_W: f32 = 6.0;
const MIN_THUMB: f32 = 24.0;
const WHEEL_PX_PER_LINE: f32 = 44.0;

/// Marks a scrollable viewport (`Overflow::scroll_y()` + `ScrollPosition`).
#[derive(Component)]
pub struct ScrollViewport;

/// Recessed scrollbar track anchored beside a `ScrollViewport`.
#[derive(Component)]
pub struct ScrollTrack {
    pub target: Entity,
}

/// Draggable thumb pill inside a `ScrollTrack`.
#[derive(Component)]
pub struct ScrollThumb {
    pub target: Entity,
}

#[derive(Clone, Copy)]
struct DragState {
    target: Entity,
    track: Entity,
    grab_offset: f32,
}

#[derive(Resource, Default)]
pub struct ScrollDrag(Option<DragState>);

/// Pure scrollbar thumb geometry in logical pixels: `Some((thumb_h, thumb_top, max_scroll))`
/// when `content_h` overflows `view_h`, or `None` when everything fits.
pub fn bar_geometry(view_h: f32, content_h: f32, scroll_y: f32, track_h: f32) -> Option<(f32, f32, f32)> {
    let max_scroll = content_h - view_h;
    if view_h <= 1.0 || track_h <= 1.0 || max_scroll <= 1.5 {
        return None;
    }
    let thumb_h = ((view_h / content_h) * track_h).clamp(MIN_THUMB.min(track_h), track_h);
    let clamped = scroll_y.clamp(0.0, max_scroll);
    let travel = (track_h - thumb_h).max(0.0);
    let thumb_top = (clamped / max_scroll) * travel;
    Some((thumb_h, thumb_top, max_scroll))
}

/// Spawn a themed scrollbar track + thumb positioned inside the right edge of a panel shell.
pub fn spawn_scrollbar(parent: &mut ChildSpawnerCommands, target: Entity, top: f32, bottom: f32, right: f32) {
    parent
        .spawn((
            ScrollTrack { target },
            Node {
                position_type: PositionType::Absolute,
                top: Val::Px(top),
                bottom: Val::Px(bottom),
                right: Val::Px(right),
                width: Val::Px(TRACK_W),
                border: UiRect::all(Val::Px(1.0)),
                border_radius: BorderRadius::all(Val::Px(TRACK_W * 0.5)),
                ..default()
            },
            BackgroundColor(GROOVE),
            BorderColor::all(Color::srgba(1.0, 1.0, 1.0, 0.08)),
            Interaction::default(),
            FocusPolicy::Block,
            super::Blocks,
            Visibility::Hidden,
        ))
        .with_children(|track| {
            track.spawn((
                ScrollThumb { target },
                Node {
                    position_type: PositionType::Absolute,
                    left: Val::Px(0.0),
                    top: Val::Px(0.0),
                    width: Val::Px(THUMB_W),
                    height: Val::Px(MIN_THUMB),
                    border_radius: BorderRadius::all(Val::Px(THUMB_W * 0.5)),
                    ..default()
                },
                BackgroundColor(BARK.with_alpha(0.84)),
                Interaction::default(),
                FocusPolicy::Block,
                super::Blocks,
            ));
        });
}

/// Mouse wheel scrolls the overflowing `ScrollViewport` under the pointer.
pub fn scroll_wheel(
    scroll: Res<AccumulatedMouseScroll>,
    mut viewports: Query<(&ComputedNode, &RelativeCursorPosition, &mut ScrollPosition), With<ScrollViewport>>,
) {
    if scroll.delta.y == 0.0 {
        return;
    }
    let dy = match scroll.unit {
        MouseScrollUnit::Line => -scroll.delta.y * WHEEL_PX_PER_LINE,
        MouseScrollUnit::Pixel => -scroll.delta.y,
    };
    // Pick the smallest (innermost) overflowing viewport under the pointer.
    let mut best: Option<(f32, f32, Mut<ScrollPosition>)> = None;
    for (computed, rel, pos) in &mut viewports {
        let over = rel.normalized.is_some_and(|p| p.x.abs() <= 0.5 && p.y.abs() <= 0.5);
        if !over {
            continue;
        }
        let inv = computed.inverse_scale_factor();
        let view_h = computed.size().y * inv;
        let content_h = computed.content_size().y * inv;
        let max_scroll = content_h - view_h;
        if max_scroll <= 1.5 {
            continue;
        }
        let area = computed.size().x * computed.size().y;
        if best.as_ref().is_none_or(|b| area < b.0) {
            best = Some((area, max_scroll, pos));
        }
    }
    if let Some((_, max_scroll, mut pos)) = best {
        pos.y = (pos.y + dy).clamp(0.0, max_scroll);
    }
}

/// Click or drag on any themed scrollbar track or thumb to scroll smoothly.
#[allow(clippy::too_many_arguments)]
pub fn scroll_drag(
    mouse: Res<ButtonInput<MouseButton>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    ui_scale: Res<UiScale>,
    mut drag: ResMut<ScrollDrag>,
    thumbs: Query<(&ScrollThumb, &Interaction, &ComputedNode, &UiGlobalTransform, &ChildOf)>,
    tracks: Query<(Entity, &ScrollTrack, &Interaction, &ComputedNode, &UiGlobalTransform)>,
    mut viewports: Query<(&ComputedNode, &mut ScrollPosition), With<ScrollViewport>>,
) {
    let cursor_phys = windows.single().ok().and_then(|w| w.cursor_position()).map(|p| p * windows.single().map(|w| w.scale_factor()).unwrap_or(1.0));
    let scale = ui_scale.0.max(0.01);

    if mouse.just_pressed(MouseButton::Left) {
        if let Some(cp) = cursor_phys {
            // Check thumb press first (preserves grab offset).
            for (thumb, inter, thumb_comp, thumb_tf, parent) in &thumbs {
                if *inter != Interaction::None {
                    let thumb_top_phys = thumb_tf.translation.y - thumb_comp.size().y * 0.5;
                    let grab_offset = (cp.y - thumb_top_phys) * thumb_comp.inverse_scale_factor();
                    drag.0 = Some(DragState { target: thumb.target, track: parent.parent(), grab_offset });
                    break;
                }
            }
            // Otherwise check track press (jumps thumb center to click point).
            if drag.0.is_none() {
                for (track_e, track, inter, track_comp, track_tf) in &tracks {
                    if *inter != Interaction::None {
                        if let Ok((vp_comp, mut pos)) = viewports.get_mut(track.target) {
                            let inv = vp_comp.inverse_scale_factor();
                            let view_h = vp_comp.size().y * inv;
                            let content_h = vp_comp.content_size().y * inv;
                            let track_h = track_comp.size().y * track_comp.inverse_scale_factor();
                            if let Some((thumb_h, _, max_scroll)) = bar_geometry(view_h, content_h, pos.y, track_h) {
                                let track_top_phys = track_tf.translation.y - track_comp.size().y * 0.5;
                                let local_y = (cp.y - track_top_phys) * track_comp.inverse_scale_factor();
                                let travel = (track_h - thumb_h).max(1.0);
                                let frac = ((local_y - thumb_h * 0.5) / travel).clamp(0.0, 1.0);
                                pos.y = frac * max_scroll;
                                drag.0 = Some(DragState { target: track.target, track: track_e, grab_offset: thumb_h * 0.5 });
                            }
                        }
                        break;
                    }
                }
            }
        }
    }

    if !mouse.pressed(MouseButton::Left) {
        drag.0 = None;
        return;
    }

    let Some(state) = drag.0 else { return };
    let Some(cp) = cursor_phys else { return };
    let Ok((_, _, _, track_comp, track_tf)) = tracks.get(state.track) else {
        drag.0 = None;
        return;
    };
    let Ok((vp_comp, mut pos)) = viewports.get_mut(state.target) else {
        drag.0 = None;
        return;
    };
    let inv = vp_comp.inverse_scale_factor();
    let _ = scale;
    let view_h = vp_comp.size().y * inv;
    let content_h = vp_comp.content_size().y * inv;
    let track_h = track_comp.size().y * track_comp.inverse_scale_factor();
    if let Some((thumb_h, _, max_scroll)) = bar_geometry(view_h, content_h, pos.y, track_h) {
        let track_top_phys = track_tf.translation.y - track_comp.size().y * 0.5;
        let local_y = (cp.y - track_top_phys) * track_comp.inverse_scale_factor();
        let travel = (track_h - thumb_h).max(1.0);
        let frac = ((local_y - state.grab_offset) / travel).clamp(0.0, 1.0);
        pos.y = frac * max_scroll;
    }
}

/// Sync scrollbar visibility, thumb size/position, and hover highlight with each viewport's layout.
pub fn sync_scrollbars(
    drag: Res<ScrollDrag>,
    mut viewports: Query<(&ComputedNode, &mut ScrollPosition), With<ScrollViewport>>,
    mut tracks: Query<(&ScrollTrack, &ComputedNode, &mut Visibility)>,
    mut thumbs: Query<(&ScrollThumb, &Interaction, &ChildOf, &mut Node, &mut BackgroundColor)>,
) {
    let mut track_heights = std::collections::HashMap::new();
    for (track, track_comp, mut vis) in &mut tracks {
        let Ok((vp_comp, mut pos)) = viewports.get_mut(track.target) else {
            if *vis != Visibility::Hidden {
                *vis = Visibility::Hidden;
            }
            continue;
        };
        let inv = vp_comp.inverse_scale_factor();
        let view_h = vp_comp.size().y * inv;
        let content_h = vp_comp.content_size().y * inv;
        let track_h = (track_comp.size().y * track_comp.inverse_scale_factor()).max(view_h - 8.0);
        if let Some((_, _, max_scroll)) = bar_geometry(view_h, content_h, pos.y, track_h) {
            if pos.y > max_scroll {
                pos.y = max_scroll;
            } else if pos.y < 0.0 {
                pos.y = 0.0;
            }
            if *vis != Visibility::Inherited {
                *vis = Visibility::Inherited;
            }
            track_heights.insert(track.target, track_h);
        } else {
            if pos.y != 0.0 {
                pos.y = 0.0;
            }
            if *vis != Visibility::Hidden {
                *vis = Visibility::Hidden;
            }
        }
    }

    for (thumb, inter, _parent, mut node, mut bg) in &mut thumbs {
        let Some(&track_h) = track_heights.get(&thumb.target) else { continue };
        let Ok((vp_comp, pos)) = viewports.get(thumb.target) else { continue };
        let inv = vp_comp.inverse_scale_factor();
        let view_h = vp_comp.size().y * inv;
        let content_h = vp_comp.content_size().y * inv;
        if let Some((thumb_h, thumb_top, _)) = bar_geometry(view_h, content_h, pos.y, track_h) {
            node.height = Val::Px(thumb_h);
            node.top = Val::Px(thumb_top);
        }
        let active = drag.0.is_some_and(|d| d.target == thumb.target) || *inter != Interaction::None;
        let want = if active { ACCENT } else { BARK.with_alpha(0.84) };
        if bg.0 != want {
            bg.0 = want;
        }
    }
}
