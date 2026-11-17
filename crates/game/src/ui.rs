//! bevy_ui overlay: seed and sim-time readout.
use crate::state::Sim;
use bevy::prelude::*;

#[derive(Component)]
struct StatusText;

pub struct UiPlugin;

impl Plugin for UiPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_ui).add_systems(Update, update_status);
    }
}

fn spawn_ui(mut commands: Commands) {
    commands.spawn((
        Text::new(""),
        Node { position_type: PositionType::Absolute, top: px(10), left: px(10), ..default() },
        StatusText,
    ));
}

fn update_status(sim: Res<Sim>, mut q: Query<&mut Text, With<StatusText>>) {
    for mut t in &mut q {
        t.0 = format!("Beaver Colony  seed {}  day {:.1}", sim.0.seed, sim.0.time_days);
    }
}
