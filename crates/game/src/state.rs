//! Holds the simulation as a Bevy resource and advances it each frame.
use bevy::prelude::*;
use sim::SimState;

#[derive(Resource)]
pub struct Sim(pub SimState);

pub struct StatePlugin;

impl Plugin for StatePlugin {
    fn build(&self, app: &mut App) {
        let seed = app.world().resource::<crate::cli::Cli>().seed;
        app.insert_resource(Sim(SimState::new(seed)));
    }
}
