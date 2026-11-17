//! Shared look and widgets: the soft cream-and-leaf palette, rounded cards, pills, bars, and
//! hit-testing for clickable nodes. Everything is plain `bevy_ui`.
use bevy::prelude::*;
use bevy::ui::UiGlobalTransform;

use super::screens::{Do, Tone};

pub const PANEL: Color = Color::srgba(0.99, 0.95, 0.87, 0.95);
pub const PANEL_SOFT: Color = Color::srgba(0.96, 0.89, 0.76, 1.0);
pub const ROW_A: Color = Color::srgba(1.0, 0.98, 0.93, 0.9);
pub const ROW_B: Color = Color::srgba(0.97, 0.91, 0.80, 0.9);
pub const ROW_HOT: Color = Color::srgba(0.93, 0.83, 0.62, 1.0);
pub const TRACK: Color = Color::srgba(0.88, 0.80, 0.66, 1.0);
pub const INK: Color = Color::srgb(0.30, 0.20, 0.13);
pub const DIM: Color = Color::srgb(0.58, 0.46, 0.36);
pub const GOOD: Color = Color::srgb(0.36, 0.64, 0.38);
pub const WARN: Color = Color::srgb(0.90, 0.60, 0.20);
pub const BAD: Color = Color::srgb(0.84, 0.34, 0.34);
pub const CHIP: Color = Color::srgb(0.93, 0.84, 0.66);
pub const CHIP_ON: Color = Color::srgb(0.55, 0.75, 0.50);
pub const BTN: Color = Color::srgb(0.62, 0.80, 0.55);
pub const BTN_HOT: Color = Color::srgb(0.72, 0.88, 0.62);

pub const FONT: f32 = 12.0;

/// A node that reacts to a click. Higher `layer` wins where nodes overlap; `scrolled` nodes
/// only count while the cursor is inside the scroll area.
#[derive(Component, Clone)]
pub struct Clickable {
    pub act: Do,
    pub layer: u8,
    pub scrolled: bool,
    pub base: Color,
    pub hot: Color,
}

/// Nodes that swallow mouse input so the world underneath is not clicked or orbited.
#[derive(Component)]
pub struct Blocker;

/// The scrolling body of the detail panel.
#[derive(Component)]
pub struct ScrollArea;

pub type Kids<'a> = ChildSpawnerCommands<'a>;

pub fn tone_color(t: Tone) -> Color {
    use Tone::*;
    match t {
        Normal => INK,
        Good => GOOD,
        Warn => WARN,
        Bad => BAD,
        Dim => DIM,
    }
}

/// A thin horizontal meter: `frac` of the track is filled with `color`.
pub fn bar(p: &mut Kids, frac: f32, width: f32, height: f32, color: Color) {
    p.spawn((
        Node { width: px(width), height: px(height), flex_shrink: 0.0, ..default() },
        BackgroundColor(TRACK),
        Round(height),
    ))
    .with_children(|b| {
        b.spawn((
            Node { width: percent(frac.clamp(0.0, 1.0) * 100.0), height: percent(100), ..default() },
            BackgroundColor(color),
            Round(height),
        ));
    });
}

fn contains(node: &ComputedNode, t: &UiGlobalTransform, p: Vec2) -> bool {
    node.size.x > 1.0 && node.contains_point(*t, p)
}

/// The action under `cursor` (physical pixels), if any.
pub fn hit_test(
    cursor: Vec2,
    clickables: &Query<(&Clickable, &ComputedNode, &UiGlobalTransform)>,
    scroll: &Query<(&ComputedNode, &UiGlobalTransform), With<ScrollArea>>,
) -> Option<(Do, u8)> {
    let in_scroll = scroll.iter().any(|(n, t)| contains(n, t, cursor));
    clickables
        .iter()
        .filter(|(c, n, t)| (!c.scrolled || in_scroll) && contains(n, t, cursor))
        .max_by_key(|(c, _, _)| c.layer)
        .map(|(c, _, _)| (c.act.clone(), c.layer))
}

/// Tints clickable nodes under the cursor.
pub fn hover_tint(
    windows: Query<&Window, With<bevy::window::PrimaryWindow>>,
    mut q: Query<(&Clickable, &ComputedNode, &UiGlobalTransform, &mut BackgroundColor)>,
    scroll: Query<(&ComputedNode, &UiGlobalTransform), With<ScrollArea>>,
) {
    let cursor = windows.single().ok().and_then(|w| w.physical_cursor_position());
    let in_scroll = cursor.is_some_and(|c| scroll.iter().any(|(n, t)| contains(n, t, c)));
    for (c, n, t, mut bg) in &mut q {
        let hot = cursor.is_some_and(|p| (!c.scrolled || in_scroll) && contains(n, t, p));
        let want = if hot { c.hot } else { c.base };
        if bg.0 != want {
            bg.0 = want;
        }
    }
}

/// Corner radius in logical pixels, applied to the node's `border_radius` before layout.
#[derive(Component, Clone, Copy)]
pub struct Round(pub f32);

pub fn apply_round(mut q: Query<(&Round, &mut Node), Added<Round>>) {
    for (r, mut n) in &mut q {
        n.border_radius = BorderRadius::all(px(r.0));
    }
}
