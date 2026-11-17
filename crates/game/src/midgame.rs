//! `--midgame`: skips scouting and starts from a seeded colony that has lived through its first
//! year: settled at the best site, two trade routes open, events logged and decisions waiting.
//! Deterministic for a seed, so `--screenshot` of it is repeatable.
use crate::camera::OrbitCamera;
use crate::cli::Cli;
use crate::flow::{best_site, cell_world, settle, CaptureGate, Flow, Phase, Selection};
use crate::sim_driver::SimClock;
use crate::state::Sim;
use bevy::prelude::*;
use sim::events::EventKind;
use sim::region::Link;
use sim::ColonyHost;

/// Simulated days lived before the first frame.
pub const MIDGAME_DAYS: u32 = 420;
const STEP: f32 = 0.25;

pub struct MidgamePlugin;

impl Plugin for MidgamePlugin {
    fn build(&self, app: &mut App) {
        if !app.world().resource::<Cli>().midgame {
            return;
        }
        app.insert_resource(CaptureGate(false)).add_systems(Startup, setup).add_systems(Update, open_gate);
    }
}

/// Settles and fast-forwards `sim`; returns the lodge cell.
pub fn build_midgame(sim: &mut sim::SimState, flow: &mut Flow, hist: &mut crate::ui::ledgers::History) -> Result<(usize, usize), String> {
    flow.phase = Phase::Scouting;

    flow.fog.reveal_valley(&sim.world, sim.world.start);
    let site = best_site(&sim.world, &sim.water.depth, &flow.fog).ok_or("no settleable site")?;
    settle(sim, flow, site)?;
    for id in 0..2 {
        let _ = sim.region.connect(id, Link::Path);
    }
    for _ in 0..(MIDGAME_DAYS as f32 / STEP) as u32 {
        sim.step(STEP).map_err(|e| e.to_string())?;
        hist.record(sim);
    }
    // a little stock so the stores ledger has numbers to compare
    {
        use sim::buildings::Good;
        let st = &mut sim.buildings.stores;
        for (g, a) in [(Good::Logs, 64.0), (Good::Sticks, 41.0), (Good::Mud, 28.0), (Good::Stone, 12.0), (Good::Baskets, 6.0)] {
            let _ = st.deposit(g, a);
        }
        let _ = st.withdraw(Good::Logs, 18.0);
    }
    // make sure the player has something to decide and something to read
    let ctx = sim.event_context();
    for kind in [EventKind::WanderingBeaver, EventKind::Fire] {
        let mut host = ColonyHost { colony: &mut sim.colony };
        sim.events.trigger(kind, &ctx, &mut host).map_err(|e| e.to_string())?;
    }
    Ok(site)
}

fn setup(mut sim: ResMut<Sim>, mut ui: ResMut<crate::ui::LedgerUi>, mut flow: ResMut<Flow>, mut clock: ResMut<SimClock>, mut orbit: ResMut<OrbitCamera>, mut sel: ResMut<Selection>) {
    match build_midgame(&mut sim.0, &mut flow, &mut ui.hist) {
        Ok(site) => {
            let p = cell_world(&sim.0.height, (site.0 as f32, site.1 as f32));
            orbit.target = p;
            orbit.distance = 70.0;
            flow.focus = None;
            clock.paused = true;
            *sel = Selection::None;
            info!("midgame: settled at {site:?}, day {:.0}, {} beavers", sim.0.time_days, sim.0.colony.population());
        }
        Err(e) => error!("midgame setup failed: {e}"),
    }
}

fn open_gate(flow: Res<Flow>, mut gate: ResMut<CaptureGate>) {
    gate.0 = flow.phase == Phase::Settled && flow.real_secs - flow.settled_at.unwrap_or(f32::MAX) >= 2.0;
}
