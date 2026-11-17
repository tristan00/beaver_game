//! The default start: the founding family has already settled the best explored site, so the first
//! frame is the map with the colony on it. Scripted modes (`--midgame`, `--autoplay-settle`, `--showcase`,
//! `--stress`) keep their own openings. The full start menu (`--start N`) replaces this later.
use crate::cli::Cli;
use crate::flow::{best_site, settle, Flow, Phase, Selection};
use bevy::prelude::*;

use crate::state::Sim;

pub struct OpeningPlugin;

impl Plugin for OpeningPlugin {
    fn build(&self, app: &mut App) {
        let cli = app.world().resource::<Cli>();
        if cli.midgame || cli.autoplay_settle || cli.stress || cli.showcase.is_some() {
            return;
        }
        app.add_systems(Startup, settle_family);
    }
}

fn settle_family(mut sim: ResMut<Sim>, mut flow: ResMut<Flow>, mut sel: ResMut<Selection>) {
    match found(&mut sim.0, &mut flow) {
        Ok(_) => {
            // the camera keeps its `--cam` preset: the map, not a zoom, is the first view
            flow.focus = None;
            *sel = Selection::None;
        }
        Err(e) => error!("default start failed: {e}"),
    }
}

/// Settles the founders at the best site of the start valley; returns the lodge cell.
pub fn found(sim: &mut sim::SimState, flow: &mut Flow) -> Result<(usize, usize), String> {
    flow.phase = Phase::Scouting;
    flow.fog.reveal_valley(&sim.world, sim.world.start);
    let site = best_site(&sim.world, &sim.water.depth, &flow.fog).ok_or("no settleable site")?;
    settle(sim, flow, site)?;
    Ok(site)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_start_has_a_family_and_a_lodge() {
        let mut sim = sim::SimState::new(1);
        let mut flow = Flow::new(&sim.world);
        found(&mut sim, &mut flow).expect("founds");
        assert_eq!(flow.phase, Phase::Settled);
        assert_eq!(sim.colony.population(), 4);
        assert_eq!(sim.colony.lodges.len(), 1);
    }
}
