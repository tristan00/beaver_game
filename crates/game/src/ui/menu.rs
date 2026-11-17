//! The start-up menu: one card per start, listed from `shell::start_menu`. Click a card or press
//! its number to choose; `startflow` builds the start and removes the menu.
use super::kit::*;
use super::shell::{start_menu, StartCard};
use bevy::prelude::*;
use bevy::ui::UiGlobalTransform;
use bevy::window::PrimaryWindow;
use sim::starts::Start;

/// The full-screen overlay holding the menu.
#[derive(Component)]
pub struct MenuRoot;

/// A clickable start card.
#[derive(Component, Clone, Copy)]
pub struct MenuCard(pub Start);

const CARD_W: f32 = 290.0;

/// Spawns the overlay with one card per start.
pub fn spawn(commands: &mut Commands, error: Option<&str>) {
    commands
        .spawn((
            MenuRoot,
            Node {
                position_type: PositionType::Absolute,
                width: percent(100),
                height: percent(100),
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                row_gap: px(22),
                ..default()
            },
            BackgroundColor(Color::srgba(0.30, 0.20, 0.13, 0.45)),
            GlobalZIndex(60),
        ))
        .with_children(|root| {
            text(root, "Beaver Colony", 38.0, PANEL);
            text(root, "Choose how your story begins", 16.0, PANEL_SOFT);
            root.spawn(Node { column_gap: px(18), align_items: AlignItems::Stretch, ..default() }).with_children(|row| {
                for card in start_menu() {
                    spawn_card(row, &card);
                }
            });
            if let Some(e) = error {
                text(root, e, 14.0, BAD);
            }
        });
}

fn spawn_card(row: &mut Kids, card: &StartCard) {
    row.spawn((card_node(CARD_W), MenuCard(card.start))).with_children(|c| {
        text(c, format!("{}", card.start.number()), 34.0, WARN);
        text(c, card.title, 20.0, INK);
        text(c, card.pitch, 14.0, DIM);
        text(c, format!("click, or press {}", card.start.number()), 12.0, GOOD);
    });
}

pub type CardQuery<'w, 's> = Query<'w, 's, (&'static MenuCard, &'static ComputedNode, &'static UiGlobalTransform)>;

fn hovered(cards: &CardQuery<'_, '_>, windows: &Query<&Window, With<PrimaryWindow>>) -> Option<Start> {
    let cursor = windows.single().ok()?.physical_cursor_position()?;
    cards.iter().find(|(_, n, t)| n.size.x > 1.0 && n.contains_point(**t, cursor)).map(|(c, _, _)| c.0)
}

/// Tints the card under the cursor.
pub fn hover(cards: CardQuery, windows: Query<&Window, With<PrimaryWindow>>, mut colours: Query<(&MenuCard, &mut BackgroundColor)>) {
    let hot = hovered(&cards, &windows);
    for (c, mut bg) in &mut colours {
        let want = if Some(c.0) == hot { ROW_HOT } else { PANEL };
        if bg.0 != want {
            bg.0 = want;
        }
    }
}

/// The start chosen this frame: a clicked card, or its number key.
pub fn chosen(keys: &ButtonInput<KeyCode>, mouse: &ButtonInput<MouseButton>, cards: &CardQuery<'_, '_>, windows: &Query<&Window, With<PrimaryWindow>>) -> Option<Start> {
    let by_key = [(KeyCode::Digit1, 1u8), (KeyCode::Digit2, 2), (KeyCode::Digit3, 3), (KeyCode::Numpad1, 1), (KeyCode::Numpad2, 2), (KeyCode::Numpad3, 3)]
        .into_iter()
        .find(|(k, _)| keys.just_pressed(*k))
        .and_then(|(_, n)| Start::from_number(n));
    by_key.or_else(|| if mouse.just_pressed(MouseButton::Left) { hovered(cards, windows) } else { None })
}
