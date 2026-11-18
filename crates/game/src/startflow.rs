//! The one start: nothing stands on the map and no colony exists. The player looks over the valley
//! and picks where the family settles (never chosen for them); the family swims in there and drops
//! the bundle it carried, from which it builds its first dam, shelter and store. `--demo-pond` scripts it for screenshots: order the designed
//! dam, let the family build it, hold the picture once the pond is full.
use crate::camera::OrbitCamera;
use crate::cli::Cli;
use crate::flow::{cell_world, CaptureGate, Flow, Phase, Selection};
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

    flow.focus = None;
    orbit.target = cell_world(&sim.height, focus);
    orbit.distance = ARRIVAL_DISTANCE;
    orbit.pitch = ARRIVAL_PITCH;
    // A new game waits for the player: it starts paused (Space or 1-3 to run).
    *clock = SimClock::default();
    clock.paused = true;
    *sel = Selection::None;
}

/// Opens the choice of a settling place: the start valley is revealed, the camera overlooks it
/// around `around` (the designed camp, only a framing hint), the clock waits.
pub fn begin_scouting(sim: &SimState, flow: &mut Flow, clock: &mut SimClock, orbit: &mut OrbitCamera, sel: &mut Selection, around: Option<(usize, usize)>) {
    flow.phase = Phase::Scouting;
    flow.settled_at = None;
    let focus = around.map(|c| (c.0 as f32 + 0.5, c.1 as f32 + 0.5)).unwrap_or((sim.world.size as f32 / 2.0, sim.world.size as f32 / 2.0));
    // The family has looked over only the ground around where it came in (the sim revealed it);
    // the rest is explored by its beavers once it has settled.
    flow.focus = None;
    orbit.target = cell_world(&sim.height, focus);
    orbit.distance = SCOUT_DISTANCE;
    orbit.pitch = SCOUT_PITCH;
    *clock = SimClock::default();
    clock.paused = true;
    *sel = Selection::None;
}

/// Camera while choosing where to settle: wide enough to compare banks.
const SCOUT_DISTANCE: f32 = 60.0;
const SCOUT_PITCH: f32 = 1.0;

/// Builds a fresh new game on `map` (world, river start and designed dam) and opens the choice of
/// a settling place. Nothing is settled for the player. The renderers follow the new world on their own.
pub fn new_game(
    map: &crate::state::MapChoice,
    sim: &mut Sim,
    planned: &mut PlannedDam,
    flow: &mut Flow,
    clock: &mut SimClock,
    orbit: &mut OrbitCamera,
    sel: &mut Selection,
) -> Result<(), String> {
    let (mut s, dam) = map.build().map_err(|e| format!("cannot build the new game: {e}"))?;
    let camp = dam.camp;
    start_recording(&mut s);
    sim.0 = s;
    planned.0 = Some(dam);
    let generation = flow.generation + 1;
    *flow = Flow::new(&sim.0.world);
    flow.generation = generation;
    begin_scouting(&sim.0, flow, clock, orbit, sel, Some(camp));
    Ok(())
}

pub struct StartPlugin;

impl Plugin for StartPlugin {
    fn build(&self, app: &mut App) {
        let cli = app.world().resource::<Cli>();
        if cli.plain_world() {
            return; // scripted scenes found their own colony
        }
        let demo_pond = cli.demo_pond;
        app.add_systems(Startup, begin_new_game).add_systems(Update, save_recording);
        if demo_pond {
            app.insert_resource(CaptureGate(false)).init_resource::<DemoPond>().add_systems(Update, demo_pond_script);
        }
    }
}

/// `BC_RECORD=FILE`: record every command of the game (`sim::script` log, replayable headless
/// with `cargo run -p sim --example play`).
pub fn start_recording(s: &mut SimState) {
    if std::env::var_os("BC_RECORD").is_some() {
        s.recorder = Some(Vec::new());
    }
}

/// Writes the recording when it grows (every couple of seconds at most).
pub fn save_recording(time: Res<Time>, mut last: Local<(usize, f32)>, sim: Res<Sim>) {
    let (Ok(path), Some(log)) = (std::env::var("BC_RECORD"), sim.0.recorder.as_ref()) else { return };
    let now = time.elapsed_secs();
    if log.len() == last.0 || now - last.1 < 2.0 {
        return;
    }
    *last = (log.len(), now);
    let header = format!("# map seed {} — replay: cargo run --release -p sim --example play -- <script with 'replay {}'>\n", sim.0.seed, path);
    let _ = std::fs::write(&path, header + &sim::script::write_log(log));
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
    // The sim opens with no colony and a played game never settles for the player. Only scripted
    // screenshot runs (developer tooling: `--screenshot` with `--demo-pond`, `BC_HUD_DEMO` except
    // the `settle` view, or `BC_SETTLE=1` for the plain settled start) found the colony at the
    // designed camp by script.
    start_recording(&mut sim.0);
    let settle = std::env::var_os("BC_SETTLE").is_some();
    let scripted = cli.screenshot.is_some() && (cli.demo_pond || settle || std::env::var("BC_HUD_DEMO").is_ok_and(|d| d != "settle"));
    if !scripted {
        let camp = planned.0.as_ref().map(|d| d.camp);
        begin_scouting(&sim.0, &mut flow, &mut clock, &mut orbit, &mut sel, camp);
        return;
    }
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
    // `BC_CLOSE=1`: a close look at the family where it landed, as a player first sees it.
    if std::env::var_os("BC_CLOSE").is_some() {
        let bs = &sim.0.colony.beavers;
        if !bs.is_empty() {
            let n = bs.len() as f32;
            let c = (bs.iter().map(|b| b.pos.0).sum::<f32>() / n, bs.iter().map(|b| b.pos.1).sum::<f32>() / n);
            let p = crate::entities_render::sim_view::cell_xz(&sim.0, c);
            orbit.target = Vec3::new(p.x, crate::entities_render::height_at(&sim.0.height, p.x, p.y), p.y);
            orbit.distance = 16.0;
            orbit.pitch = 0.75;
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

