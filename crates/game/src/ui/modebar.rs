//! The map-mode strip at the bottom left: four mode buttons and, for the active mode, its legend
//! (every number explains itself on hover) and its action buttons. The painted overlay itself is
//! `map_overlay.rs`; what the buttons do is `mapmodes::run_action` via `Do::Mode` / `Do::ModeAct`.
use super::icons::{Icon, IconAssets};
use super::kit::*;
use super::mapmodes::{LegendRow, MapMode};
use super::panels::{ShellUi, Tip};
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
    p.spawn((Text::new(s), TextFont::from_font_size(size), TextColor(color), TextLayout::no_wrap()));
}

fn icon(p: &mut Kids, icons: &IconAssets, i: Icon, size: f32) {
    p.spawn((Node { width: px(size), height: px(size), flex_shrink: 0.0, ..default() }, ImageNode::new(icons.get(i))));
}

fn card() -> impl Bundle {
    (
        Node { flex_direction: FlexDirection::Column, padding: UiRect::all(px(8)), row_gap: px(4), min_width: px(236), ..default() },
        BackgroundColor(PANEL),
        Round(16.0),
        BoxShadow::new(Color::srgba(0.25, 0.15, 0.05, 0.30), px(0), px(3), px(0), px(9)),
    )
}

fn color([r, g, b, a]: [f32; 4]) -> Color {
    Color::srgba(r, g, b, a)
}

fn legend_row(p: &mut Kids, r: &LegendRow, icons: &IconAssets) {
    p.spawn((Node { align_items: AlignItems::Center, column_gap: px(6), padding: UiRect::axes(px(4), px(1)), ..default() }, Tip::of(&r.num))).with_children(|row| {
        row.spawn((Node { width: px(10), height: px(10), flex_shrink: 0.0, ..default() }, BackgroundColor(color(r.swatch)), Round(3.0)));
        if let Some(i) = r.icon {
            icon(row, icons, i, 16.0);
        }
        text(row, r.num.label.clone(), 12.0, INK);
        row.spawn(Node { flex_grow: 1.0, min_width: px(8), ..default() });
        text(row, r.num.show(), 13.0, INK);
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
                    text(h, mode.label(), 15.0, INK);
                });
                for r in &now.legend {
                    legend_row(c, r, &icons);
                }
                if !mode.actions().is_empty() {
                    c.spawn(Node { column_gap: px(6), row_gap: px(4), flex_wrap: FlexWrap::Wrap, margin: UiRect::top(px(2)), ..default() }).with_children(|bar| {
                        for a in mode.actions() {
                            bar.spawn((
                                Node { padding: UiRect::axes(px(8), px(4)), column_gap: px(5), align_items: AlignItems::Center, ..default() },
                                BackgroundColor(BTN),
                                Round(99.0),
                                Clickable { act: Do::ModeAct(*a), layer: 6, scrolled: false, base: BTN, hot: BTN_HOT },
                                Tip::note(a.hint()),
                            ))
                            .with_children(|b| {
                                icon(b, &icons, a.icon(), 18.0);
                                text(b, a.label(), 12.0, INK);
                            });
                        }
                    });
                }
                if now.armed || now.editor {
                    text(c, if now.armed { "Drag on the map to assign. Esc cancels." } else { "Editing work areas. Esc closes." }, 11.0, DIM);
                } else if !now.status.is_empty() && mode != MapMode::Normal {
                    text(c, now.status.clone(), 11.0, DIM);
                }
            });
        }
        root.spawn((Node { padding: UiRect::axes(px(8), px(4)), column_gap: px(4), align_items: AlignItems::Center, ..default() }, BackgroundColor(PANEL), Round(20.0), BoxShadow::new(Color::srgba(0.25, 0.15, 0.05, 0.30), px(0), px(3), px(0), px(9))))
            .with_children(|strip| {
                for m in MapMode::PAINTED {
                    let on = m == mode;
                    let base = if on { CHIP_ON } else { Color::srgba(0.0, 0.0, 0.0, 0.0) };
                    strip
                        .spawn((
                            Node { width: px(54), height: px(46), flex_direction: FlexDirection::Column, align_items: AlignItems::Center, justify_content: JustifyContent::Center, ..default() },
                            BackgroundColor(base),
                            Round(12.0),
                            Clickable { act: Do::Mode(m), layer: 6, scrolled: false, base, hot: ROW_HOT },
                            Tip::note(format!("{} map mode: {}", m.label(), m.hint())),
                        ))
                        .with_children(|b| {
                            icon(b, &icons, m.icon(), 26.0);
                            text(b, m.label(), 10.0, INK);
                        });
                }
            });
    });
    *last = Some(now);
}
