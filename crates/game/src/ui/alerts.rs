//! Alerts zone (top-left, under the clock): what needs attention, newest first. Click one to fly
//! the camera to its place; join requests carry accept and turn-away buttons. Its own module,
//! built only from `kit`.
use super::events::{toasts, Toast};
use super::icons::IconAssets;
use super::kit::*;
use crate::state::Sim;
use bevy::prelude::*;

#[derive(Component)]
pub struct AlertsRoot;

pub struct AlertsPlugin;

impl Plugin for AlertsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_root).add_systems(Update, refresh);
    }
}

fn spawn_root(mut commands: Commands) {
    commands.spawn((
        AlertsRoot,
        Blocker,
        Node {
            position_type: PositionType::Absolute,
            left: px(GRID),
            top: px(ZONE_TOP + 4.0),
            display: Display::None,
            flex_direction: FlexDirection::Column,
            row_gap: px(GRID),
            max_width: px(440),
            ..default()
        },
        GlobalZIndex(9),
    ));
}

fn refresh(
    mut commands: Commands,
    sim: Res<Sim>,
    notices: Res<super::NoticesRes>,
    icons: Res<IconAssets>,
    mut root: Query<(Entity, &mut Node), With<AlertsRoot>>,
    mut shown: Local<Option<(bool, Vec<Toast>)>>,
) {
    let Ok((e, mut node)) = root.single_mut() else { return };
    let founded = sim.0.colony.is_founded();
    let now = if founded { toasts(&sim.0, &notices.0) } else { Vec::new() };
    if shown.as_ref().is_some_and(|(f, t)| *f == founded && *t == now) {
        return;
    }
    node.display = if !founded || (now.is_empty() && !show_empty_zones()) { Display::None } else { Display::Flex };
    commands.entity(e).despawn_children();
    commands.entity(e).with_children(|p| {
        if now.is_empty() && founded {
            p.spawn(panel(Node { padding: UiRect::all(px(GRID * 1.5)), ..default() })).with_children(|c| label(c, "No alerts.", T_BODY, DIM));
        }
        for t in &now {
            p.spawn((
                Node { padding: UiRect::axes(px(GRID), px(GRID)), column_gap: px(GRID), align_items: AlignItems::Center, ..default() },
                BackgroundColor(BARK),
                Corner::Panel,
                hard_shadow(),
                Clickable { act: t.jump.clone(), layer: 3, scrolled: false, base: BARK, hot: LINE },
                Tip::note(format!("{} - click to fly there", t.alert.kind.label())),
            ))
            .with_children(|r| {
                icon_node(r, &icons, t.alert.kind.icon(), 28.0);
                label(r, t.alert.text.clone(), T_BODY, PITH);
                for a in &t.actions {
                    spawn_button(r, &icons, KitButton::new(a.icon, a.label.clone(), a.act.clone()).layer(5).compact().kind(ButtonKind::Primary).tip(a.label.clone()));
                }
            });
        }
    });
    *shown = Some((founded, now));
}
