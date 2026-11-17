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
        app.insert_resource(Sim(SimState::new(seed))).init_resource::<SimStepMs>().add_systems(Update, step_sim);
    }
}

/// Wall time of the last `step_sim`, in ms (read by the stress report).
#[derive(Resource, Default)]
pub struct SimStepMs(pub f32);

fn step_sim(time: Res<Time>, mut sim: ResMut<Sim>, mut ms: ResMut<SimStepMs>) {
    let t0 = std::time::Instant::now();
    let r = sim.0.step(time.delta_secs() * DAYS_PER_SECOND);
    ms.0 = t0.elapsed().as_secs_f32() * 1000.0;
    if let Err(e) = r {
        error!("sim step failed: {e}");
    }
}
