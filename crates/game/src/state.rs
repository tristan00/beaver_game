//! Holds the simulation as a Bevy resource and advances it each frame.
use bevy::prelude::*;
use sim::SimState;

/// Simulated days per real second.
const DAYS_PER_SECOND: f32 = 0.1;

#[derive(Resource)]
pub struct Sim(pub SimState);

pub struct StatePlugin;

impl Plugin for StatePlugin {
    fn build(&self, app: &mut App) {
        let seed = app.world().resource::<crate::cli::Cli>().seed;
        app.insert_resource(Sim(SimState::new(seed))).add_systems(Update, step_sim);
    }
}

fn step_sim(time: Res<Time>, mut sim: ResMut<Sim>) {
    if let Err(e) = sim.0.step(time.delta_secs() * DAYS_PER_SECOND) {
        error!("sim step failed: {e}");
    }
}
