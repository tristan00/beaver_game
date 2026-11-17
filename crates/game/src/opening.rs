//! Founding helper: settles the founding family at the best explored site (used by the
//! `--stress` scene and tests; the start menu and `--start N` build the real starts).
use crate::flow::{best_site, settle, Flow, Phase};

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
