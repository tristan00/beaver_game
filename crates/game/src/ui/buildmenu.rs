//! The build menu (`docs/UI_SPEC.md` §2): a row of tabs, the tech branches that hold buildings,
//! then one tile per building under the open tab with its icon, name and cost. Locked tiles are
//! dimmed with the reason on hover. Choosing a tile arms the placement ghost (`build::pick_tool`);
//! drawn from `hud::BuildMenu`.
use super::hud::{BuildMenu, BuildTile, TileState};
use super::icons::IconAssets;
use super::kit::*;
use super::screens::Do;
use bevy::prelude::*;

const TILE_W: f32 = 112.0;

pub fn spawn_build_menu(p: &mut Kids, m: &BuildMenu, icons: &IconAssets, status: &str) {
    p.spawn(Node { column_gap: px(GRID), align_items: AlignItems::Center, ..default() }).with_children(|tabs| {
        label_bold(tabs, "Build", T_TITLE, PITH);
        for t in &m.tabs {
            let kind = if t.active { ButtonKind::Active } else { ButtonKind::Normal };
            let mut b = KitButton::new(super::icons::Icon::Build, t.label, Do::Tab(t.group)).kind(kind).layer(6).compact().tip(format!("{} buildings", t.label));
            b.icon = None;
            spawn_button(tabs, icons, b);
        }
    });
    p.spawn(Node { column_gap: px(GRID), row_gap: px(GRID), flex_wrap: FlexWrap::Wrap, ..default() }).with_children(|row| {
        for t in &m.tiles {
            spawn_tile(row, t, icons);
        }
    });
    if !status.is_empty() {
        label(p, status, T_MIN, AMBER);
    }
}

fn spawn_tile(p: &mut Kids, t: &BuildTile, icons: &IconAssets) {
    let locked = matches!(t.state, TileState::Locked(_));
    let chosen = t.state == TileState::Chosen;
    let fill = if locked { BARK } else { RAISED };
    let tip = match &t.state {
        TileState::Locked(why) => Tip::note(why.clone()),
        _ => Tip::note(format!("{}: {}", t.name, t.cost)),
    };
    p.spawn((
        Node { min_width: px(TILE_W), padding: UiRect::all(px(GRID)), flex_direction: FlexDirection::Column, align_items: AlignItems::Center, row_gap: px(4), flex_shrink: 0.0, ..default() },
        BackgroundColor(fill),
        Corner::Button,
        Clickable { act: Do::Place(t.kind), layer: 6, scrolled: false, base: fill, hot: if locked { fill } else { LINE } },
        tip,
    ))
    .with_children(|c| {
        icon_node(c, icons, t.icon, 32.0);
        label(c, t.name, T_BODY, if locked { DIM } else { PITH });
        let (text, color) = if locked { ("locked".to_string(), DIM) } else { (t.cost.clone(), if t.affordable { DIM } else { AMBER }) };
        label(c, text, T_MIN, color);
        if chosen {
            c.spawn((Node { position_type: PositionType::Absolute, left: px(0), right: px(0), bottom: px(0), height: px(ACTIVE_BAR), ..default() }, BackgroundColor(POND)));
        }
    });
}
