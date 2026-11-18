//! The one start: the game opens on the founding family swimming in along the river, camera on
//! them. Nothing is built for them. `--demo-pond` scripts it for screenshots: order the designed
//! dam, let the family build it, hold the picture once the pond is full.
use crate::camera::OrbitCamera;
use crate::cli::Cli;
use crate::flow::{cell_world, open_at, CaptureGate, Flow, Phase, Selection};
use crate::sim_driver::{SimClock, FIXED_DT_DAYS};
use crate::state::{PlannedDam, Sim};
use bevy::app::AppExit;
use bevy::prelude::*;
use sim::starts::PondWatch;
use sim::{Command, SimState};

/// Camera distance when the family arrives: close enough to see the four swimmers.
const ARRIVAL_DISTANCE: f32 = 18.0;
/// Looking steeply down so the canopy does not hide them.
const ARRIVAL_PITCH: f32 = 1.2;
/// Seconds the finished demo holds before capture.
const DEMO_HOLD_SECS: f32 = 2.0;
/// The demo gives up (exits with an error) if the pond is not full after this many days.
const DEMO_MAX_DAYS: f64 = 200.0;
/// Sim steps the demo runs per frame while fast-forwarding.
const DEMO_STEPS_PER_FRAME: usize = 100;

/// Where the camera looks when the game begins: the middle of the family (the swimmers).
fn family_centre(sim: &SimState) -> (f32, f32) {
    let n = sim.colony.beavers.len().max(1) as f32;
    let sum = sim.colony.beavers.iter().fold((0.0, 0.0), |a, b| (a.0 + b.pos.0, a.1 + b.pos.1));
    (sum.0 / n, sum.1 / n)
}

/// Starts play on the freshly built `sim`: the family's valley is in view and the camera looks at
/// the swimmers.
pub fn begin(sim: &SimState, flow: &mut Flow, clock: &mut SimClock, orbit: &mut OrbitCamera, sel: &mut Selection) {
    flow.phase = Phase::Settled;
    flow.settled_at = Some(flow.real_secs);
    let focus = family_centre(sim);
    if let Some(v) = sim.world.valley_of((focus.0 as usize, focus.1 as usize)) {
        flow.fog.reveal_valley(&sim.world, v);
    }
    flow.fog.reveal(focus, 25.0, &open_at(&sim.world));
    flow.focus = None;
    orbit.target = cell_world(&sim.height, focus);
    orbit.distance = ARRIVAL_DISTANCE;
    orbit.pitch = ARRIVAL_PITCH;
    *clock = SimClock::default();
    *sel = Selection::None;
}

pub struct StartPlugin;

impl Plugin for StartPlugin {
    fn build(&self, app: &mut App) {
        let cli = app.world().resource::<Cli>();
        if cli.plain_world() {
            return; // scripted scenes found their own colony
        }
        let demo_pond = cli.demo_pond;
        app.add_systems(Startup, begin_new_game);
        if demo_pond {
            app.insert_resource(CaptureGate(false)).init_resource::<DemoPond>().add_systems(Update, demo_pond_script);
        }
    }
}

fn begin_new_game(
    cli: Res<Cli>,
    planned: Res<PlannedDam>,
    mut sim: ResMut<Sim>,
    mut flow: ResMut<Flow>,
    mut clock: ResMut<SimClock>,
    mut orbit: ResMut<OrbitCamera>,
    mut sel: ResMut<Selection>,
) {
    // The sim opens with no colony; the designed camp is settled for the player.
    if let Some(dam) = planned.0.as_ref().filter(|_| !sim.0.colony.is_founded()) {
        if let Err(e) = sim.0.apply(Command::Settle { cell: dam.camp }) {
            eprintln!("error: cannot settle the designed camp {:?}: {e}", dam.camp);
            std::process::exit(1);
        }
    }
    // `--cam lodge` / `pond`: keep the view the camera plugin framed.
    let framed = orbit.clone();
    begin(&sim.0, &mut flow, &mut clock, &mut orbit, &mut sel);
    if matches!(cli.cam.as_str(), "lodge" | "pond") {
        *orbit = framed;
    }
    // `--cam iso-close` / `iso-wide`: fixed views of the map centre (shadow checks, docs/STYLE.md section 7).
    if matches!(cli.cam.as_str(), "iso-close" | "iso-wide") {
        if let Some(p) = crate::camera::preset(&cli.cam) {
            *orbit = p;
        }
    }
}

// ───────────────────────────── --demo-pond ─────────────────────────────

#[derive(Resource, Default)]
struct DemoPond {
    watch: Option<PondWatch>,
    done_at: Option<f32>,
}

/// Orders the designed dam once, then fast-forwards the sim (no other input) until the dam
/// stands and the pond is full; opens the capture gate a moment later.
fn demo_pond_script(
    planned: Res<PlannedDam>,
    flow: Res<Flow>,
    mut demo: ResMut<DemoPond>,
    mut sim: ResMut<Sim>,
    mut clock: ResMut<SimClock>,
    mut gate: ResMut<CaptureGate>,
    mut exit: MessageWriter<AppExit>,
) {
    if flow.phase != Phase::Settled {
        return;
    }
    let Some(dam) = planned.0.as_ref() else {
        error!("--demo-pond: the start has no designed dam");
        exit.write(AppExit::error());
        return;
    };
    if let Some(at) = demo.done_at {
        gate.0 = flow.real_secs - at >= DEMO_HOLD_SECS;
        return;
    }
    clock.paused = true; // the script drives the sim itself
    if demo.watch.is_none() {
        demo.watch = dam.watch(&sim.0);
        if demo.watch.is_none() {
            error!("--demo-pond: the designed dam is not a valid dam line");
            exit.write(AppExit::error());
            return;
        }
        sim.0.work.auto_organise = true; // the scripted demo has no player to order the family about
        if let Err(e) = sim.0.apply(Command::OrderDam { cells: dam.cells.clone(), height: dam.height }) {
            error!("--demo-pond: the dam order was refused: {e}");
            exit.write(AppExit::error());
        }
        return;
    }
    for _ in 0..DEMO_STEPS_PER_FRAME {
        if let Err(e) = sim.0.step(FIXED_DT_DAYS) {
            error!("--demo-pond: sim step failed: {e}");
            exit.write(AppExit::error());
            return;
        }
        if dam.is_built(&sim.0) && demo.watch.as_ref().is_some_and(|w| w.is_full(&sim.0)) {
            info!("--demo-pond: dam built and pond full on day {:.1}", sim.0.time_days);
            demo.done_at = Some(flow.real_secs);
            return;
        }
        if sim.0.time_days > DEMO_MAX_DAYS {
            error!("--demo-pond: no full pond after {DEMO_MAX_DAYS} days (dam built: {})", dam.is_built(&sim.0));
            exit.write(AppExit::error());
            return;
        }
    }
}

