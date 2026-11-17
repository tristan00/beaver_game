//! Holds the simulation as a Bevy resource and advances it each frame.
use bevy::prelude::*;
use sim::starts::DamSite;
use sim::SimState;

#[derive(Resource)]
pub struct Sim(pub SimState);

/// The dam the chosen start is designed around (start 1), for scripted demos. Players pick their own.
#[derive(Resource, Default)]
pub struct PlannedDam(pub Option<DamSite>);

pub struct StatePlugin;

impl Plugin for StatePlugin {
    fn build(&self, app: &mut App) {
        let cli = app.world().resource::<crate::cli::Cli>();
        let (seed, start) = (cli.seed, cli.start);
        let (sim, dam) = match start {
            // `--start N`: built directly, no menu.
            Some(start) => crate::startflow::build_start(start, seed).unwrap_or_else(|e| {
                eprintln!("error: cannot build start {}: {e}", start.number());
                std::process::exit(1);
            }),
            // The menu builds the start when one is chosen; behind it sits a plain world.
            None => (SimState::new(seed), None),
        };
        app.insert_resource(Sim(sim)).insert_resource(PlannedDam(dam));
    }
}
