//! Holds the simulation as a Bevy resource and advances it each frame.
use bevy::prelude::*;
use sim::starts::DamSite;
use sim::SimState;

#[derive(Resource)]
pub struct Sim(pub SimState);

/// The dam the new game is designed around, for scripted demos. Players pick their own.
#[derive(Resource, Default)]
pub struct PlannedDam(pub Option<DamSite>);

pub struct StatePlugin;

impl Plugin for StatePlugin {
    fn build(&self, app: &mut App) {
        let cli = app.world().resource::<crate::cli::Cli>();
        let seed = cli.seed;
        let (sim, dam) = if cli.plain_world() {
            // Scripted scenes (stress, autoplay, showcase) found their own colony on a plain world.
            (SimState::new(seed), None)
        } else {
            sim::starts::new_game_with_dam(seed).map(|(s, d)| (s, Some(d))).unwrap_or_else(|e| {
                eprintln!("error: cannot build the new game for seed {seed}: {e}");
                std::process::exit(1);
            })
        };
        app.insert_resource(Sim(sim)).insert_resource(PlannedDam(dam));
    }
}
