//! Founding helper: settles the founding family at the best explored site (used by the
//! `--stress` scene and tests; the start menu and `--start N` build the real starts).
use crate::flow::{best_site, settle, Flow, Phase};
use sim::fog::Fog;

/// Settles the founders at the best site of the start valley; returns the lodge cell.
pub fn found(sim: &mut sim::SimState, flow: &mut Flow) -> Result<(usize, usize), String> {
    flow.phase = Phase::Scouting;
    sim.fog.reveal_valley(&sim.world.clone(), sim.world.start);
    let _ = Fog::clear;
    let site = best_site(sim).ok_or("no settleable site")?;
    settle(sim, flow, site)?;
    Ok(site)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_start_has_a_family_and_no_home() {
        let mut sim = sim::SimState::new(1);
        let mut flow = Flow::new(&sim.world);
        found(&mut sim, &mut flow).expect("founds");
        assert_eq!(flow.phase, Phase::Settled);
        assert_eq!(sim.colony.population(), 4);
        assert!(sim.colony.lodges.is_empty());
    }
}
