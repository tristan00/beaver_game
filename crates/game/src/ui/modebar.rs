//! The map-mode strip at the bottom left: four mode buttons and, for the active mode, its legend
//! (every number explains itself on hover) and its action buttons. The painted overlay itself is
//! `map_overlay.rs`; what the buttons do is `mapmodes::run_action` via `Do::Mode` / `Do::ModeAct`.
use super::icons::{Icon, IconAssets};
use super::kit::*;
use super::mapmodes::{LegendRow, MapMode};
use super::panels::ShellUi;
use super::screens::Do;
use crate::map_overlay::ActiveOverlay;
use crate::state::Sim;
use bevy::prelude::*;

#[derive(Component)]
pub struct ModeRoot;

pub struct ModeBarPlugin;

impl Plugin for ModeBarPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_root).add_systems(Update, refresh.after(crate::map_overlay::ComputeSet));
    }
}

fn spawn_root(mut commands: Commands) {
    commands.spawn((
        ModeRoot,
        Blocker,
        Node {
            position_type: PositionType::Absolute,
            left: px(12),
            bottom: px(12),
            flex_direction: FlexDirection::Column,
            align_items: AlignItems::FlexStart,
            row_gap: px(6),
            display: Display::None,
            ..default()
        },
        GlobalZIndex(10),
    ));
}

fn text(p: &mut Kids, s: impl Into<String>, size: f32, color: Color) {
    label(p, s, size, color);
}

fn icon(p: &mut Kids, icons: &IconAssets, i: Icon, size: f32) {
    icon_node(p, icons, i, size);
}

fn card() -> impl Bundle {
    panel(Node { min_width: px(260), padding: UiRect::all(px(GRID * 1.5)), row_gap: px(GRID / 2.0), ..default() })
}

fn color([r, g, b, a]: [f32; 4]) -> Color {
    Color::srgba(r, g, b, a)
}

fn legend_row(p: &mut Kids, r: &LegendRow, icons: &IconAssets) {
    p.spawn((Node { align_items: AlignItems::Center, column_gap: px(6), padding: UiRect::axes(px(4), px(1)), ..default() }, Tip::of(&r.num))).with_children(|row| {
        row.spawn((Node { width: px(10), height: px(10), flex_shrink: 0.0, ..default() }, BackgroundColor(color(r.swatch))));
        if let Some(i) = r.icon {
            icon(row, icons, i, 16.0);
        }
        text(row, r.num.label.clone(), T_MIN, PITH);
        row.spawn(Node { flex_grow: 1.0, min_width: px(8), ..default() });
        text(row, r.num.show(), T_BODY, PITH);
    });
}

/// What the strip shows; rebuilt only when this changes.
#[derive(Clone, PartialEq, Default)]
struct Shown {
    mode: MapMode,
    legend: Vec<LegendRow>,
    status: String,
    armed: bool,
    editor: bool,
    founded: bool,
}

fn refresh(mut commands: Commands, sim: Res<Sim>, ui: Res<ShellUi>, ov: Res<ActiveOverlay>, icons: Res<IconAssets>, mut root: Query<(Entity, &mut Node), With<ModeRoot>>, mut last: Local<Option<Shown>>) {
    let Ok((e, mut node)) = root.single_mut() else { return };
    let now = Shown {
        mode: ui.shell.mode,
        legend: ov.0.as_ref().filter(|o| o.mode == ui.shell.mode).map(|o| o.legend.clone()).unwrap_or_default(),
        status: ui.status.clone(),
        armed: ui.shell.armed.is_some(),
        editor: ui.shell.editor.is_some(),
        founded: sim.0.colony.is_founded(),
    };
    if last.as_ref() == Some(&now) {
        return;
    }
    node.display = if now.founded { Display::Flex } else { Display::None };
    commands.entity(e).despawn_children();
    commands.entity(e).with_children(|root| {
        let mode = now.mode;
        if mode != MapMode::Normal {
            root.spawn(card()).with_children(|c| {
                c.spawn((Node { align_items: AlignItems::Center, column_gap: px(6), ..default() }, Tip::note(mode.hint()))).with_children(|h| {
                    icon(h, &icons, mode.icon(), 22.0);
                    label_bold(h, mode.label(), T_TITLE, PITH);
                });
                for r in &now.legend {
                    legend_row(c, r, &icons);
                }
                if !mode.actions().is_empty() {
                    c.spawn(Node { column_gap: px(6), row_gap: px(4), flex_wrap: FlexWrap::Wrap, margin: UiRect::top(px(2)), ..default() }).with_children(|bar| {
                        for a in mode.actions() {
                            spawn_button(bar, &icons, KitButton::new(a.icon(), a.label(), Do::ModeAct(*a)).layer(6).compact().tip(a.hint()));
                        }
                    });
                }
                if now.armed || now.editor {
                    text(c, if now.armed { "Drag on the map to assign. Esc cancels." } else { "Editing work areas. Esc closes." }, T_MIN, DIM);
                } else if !now.status.is_empty() && mode != MapMode::Normal {
                    text(c, now.status.clone(), T_MIN, DIM);
                }
            });
        }
        root.spawn(panel_with(Node { flex_direction: FlexDirection::Row, padding: UiRect::all(px(GRID)), column_gap: px(GRID), align_items: AlignItems::Center, ..default() }))
            .with_children(|strip| {
                for m in MapMode::PAINTED {
                    let kind = if m == mode { ButtonKind::Active } else { ButtonKind::Normal };
                    spawn_button(strip, &icons, KitButton::new(m.icon(), "", Do::Mode(m)).kind(kind).layer(6).tip(format!("{} map mode: {}", m.label(), m.hint())));
                }
            });
    });
    *last = Some(now);
}
