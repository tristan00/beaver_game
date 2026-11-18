//! Status zone (top): the season and the day, left of the speed buttons. Its own module, built
//! only from `kit`; the day number explains itself on hover.
use super::kit::*;
use super::screens::{time_bar, TimeBar};
use crate::flow_view::{Action, UiLayout};
use crate::state::Sim;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use sim::events::Season;

#[derive(Component)]
pub struct StatusRoot;

pub struct StatusPlugin;

impl Plugin for StatusPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_root).add_systems(Update, (layout, refresh));
    }
}

pub fn spawn_root(mut commands: Commands) {
    commands.spawn((
        StatusRoot,
        Blocker,
        panel_with(Node {
            position_type: PositionType::Absolute,
            height: px(44),
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            column_gap: px(GRID * 2.0),
            padding: UiRect::axes(px(PAD), px(0)),
            ..default()
        }),
        GlobalZIndex(10),
    ));
}

pub fn season_color(s: Season) -> Color {
    match s {
        Season::Spring => MOSS,
        Season::Summer => AMBER,
        Season::Autumn => Color::srgb(0.88, 0.52, 0.25),
        Season::Winter => POND,
    }
}

fn layout(windows: Query<&Window, With<PrimaryWindow>>, layout: Res<UiLayout>, sim: Res<Sim>, mut q: Query<&mut Node, With<StatusRoot>>) {
    let Ok(w) = windows.single() else { return };
    for mut n in &mut q {
        let right = layout.rect(Action::Pause).map(|r| r.min.x - 12.0).unwrap_or(w.width() - 10.0);
        n.display = if sim.0.colony.is_founded() { Display::Flex } else { Display::None };
        n.right = px((w.width() - right).max(8.0));
        n.top = px(10);
    }
}

fn refresh(time: Res<Time>, sim: Res<Sim>, root: Query<Entity, With<StatusRoot>>, mut commands: Commands, mut last: Local<Option<TimeBar>>, mut wait: Local<f32>) {
    if !sim.0.colony.is_founded() {
        *last = None;
        return;
    }
    *wait -= time.delta_secs();
    if *wait > 0.0 && last.is_some() {
        return;
    }
    *wait = 0.25;
    let tb = time_bar(&sim.0);
    if last.as_ref() == Some(&tb) {
        return;
    }
    if let Ok(e) = root.single() {
        commands.entity(e).despawn_children();
        commands.entity(e).with_children(|p| spawn_status(p, &tb));
    }
    *last = Some(tb);
}

pub fn spawn_status(p: &mut Kids, t: &TimeBar) {
    p.spawn((Node { column_gap: px(GRID), align_items: AlignItems::Center, ..default() }, Tip::note("Season"))).with_children(|c| {
        c.spawn((Node { width: px(12), height: px(12), ..default() }, BackgroundColor(season_color(t.season))));
        label_bold(c, format!("{:?}", t.season), T_BODY, PITH);
    });
    p.spawn(Node { align_items: AlignItems::Center, column_gap: px(4), ..default() }).with_children(|c| {
        label(c, "Day", T_BODY, DIM);
        number(c, &t.day, T_TITLE, PITH);
    });
}
