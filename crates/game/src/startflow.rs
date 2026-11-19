use crate::camera::OrbitCamera;
use crate::cli::Cli;
use crate::flow::{cell_world, CaptureGate, Flow, Phase, Selection};
use crate::sim_driver::{SimClock, FIXED_DT_DAYS};
use crate::state::{PlannedDam, Sim};
use bevy::app::AppExit;
use bevy::prelude::*;
use sim::starts::PondWatch;
use sim::{Command, SimState};

const ARRIVAL_DISTANCE: f32 = 18.0;
const ARRIVAL_PITCH: f32 = 1.2;
const DEMO_HOLD_SECS: f32 = 2.0;
const DEMO_MAX_DAYS: f64 = 200.0;
const DEMO_STEPS_PER_FRAME: usize = 720;

fn family_centre(sim: &SimState) -> (f32, f32) {
    let n = sim.colony.beavers.len().max(1) as f32;
    let sum = sim.colony.beavers.iter().fold((0.0, 0.0), |a, b| (a.0 + b.pos.0, a.1 + b.pos.1));
    (sum.0 / n, sum.1 / n)
}

pub fn begin(sim: &SimState, flow: &mut Flow, clock: &mut SimClock, orbit: &mut OrbitCamera, sel: &mut Selection) {
    flow.phase = Phase::Settled;
    flow.settled_at = Some(flow.real_secs);
    let focus = family_centre(sim);

    flow.focus = None;
    orbit.target = cell_world(&sim.height, focus);
    orbit.distance = ARRIVAL_DISTANCE;
    orbit.pitch = ARRIVAL_PITCH;
    *clock = SimClock::default();
    clock.paused = true;
    *sel = Selection::None;
}

pub fn begin_scouting(sim: &SimState, flow: &mut Flow, clock: &mut SimClock, orbit: &mut OrbitCamera, sel: &mut Selection, around: Option<(usize, usize)>) {
    flow.phase = Phase::Scouting;
    flow.settled_at = None;
    let focus = around.map(|c| (c.0 as f32 + 0.5, c.1 as f32 + 0.5)).unwrap_or((sim.world.size as f32 / 2.0, sim.world.size as f32 / 2.0));
    flow.focus = None;
    orbit.target = cell_world(&sim.height, focus);
    orbit.distance = SCOUT_DISTANCE;
    orbit.pitch = SCOUT_PITCH;
    *clock = SimClock::default();
    clock.paused = true;
    *sel = Selection::None;
}

pub fn begin_sandbox(sim: &SimState, flow: &mut Flow, clock: &mut SimClock, orbit: &mut OrbitCamera, sel: &mut Selection) {
    flow.phase = Phase::Settled;
    flow.settled_at = Some(flow.real_secs);
    flow.focus = None;
    let half = sim.world.size as f32 / 2.0;
    orbit.target = cell_world(&sim.height, (half, half));
    orbit.distance = SANDBOX_DISTANCE;
    orbit.pitch = SCOUT_PITCH;
    *clock = SimClock::default();
    *sel = Selection::None;
}

const SANDBOX_DISTANCE: f32 = 220.0;
const SCOUT_DISTANCE: f32 = 60.0;
const SCOUT_PITCH: f32 = 1.0;

pub fn new_game(
    map: &crate::state::MapChoice,
    sim: &mut Sim,
    planned: &mut PlannedDam,
    flow: &mut Flow,
    clock: &mut SimClock,
    orbit: &mut OrbitCamera,
    sel: &mut Selection,
) -> Result<(), String> {
    let (mut s, dam) = if sim.0.sandbox {
        (map.sandbox().map_err(|e| format!("cannot build the sandbox: {e}"))?, None)
    } else {
        map.build().map(|(s, d)| (s, Some(d))).map_err(|e| format!("cannot build the new game: {e}"))?
    };
    let camp = dam.as_ref().map(|d| d.camp);
    start_recording(&mut s);
    sim.0 = s;
    planned.0 = dam;
    let generation = flow.generation + 1;
    *flow = Flow::new(&sim.0.world);
    flow.generation = generation;
    if sim.0.sandbox {
        begin_sandbox(&sim.0, flow, clock, orbit, sel);
    } else {
        begin_scouting(&sim.0, flow, clock, orbit, sel, camp);
    }
    Ok(())
}

pub struct StartPlugin;

impl Plugin for StartPlugin {
    fn build(&self, app: &mut App) {
        let cli = app.world().resource::<Cli>();
        if cli.plain_world() {
            return;
        }
        let demo_pond = cli.demo_pond;
        app.add_systems(Startup, begin_new_game).add_systems(Update, save_recording);
        if demo_pond {
            app.insert_resource(CaptureGate(false)).init_resource::<DemoPond>().add_systems(Update, demo_pond_script);
        }
    }
}

pub fn start_recording(s: &mut SimState) {
    if std::env::var_os("BC_RECORD").is_some() {
        s.recorder = Some(Vec::new());
    }
}

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
    start_recording(&mut sim.0);
    if sim.0.sandbox {
        begin_sandbox(&sim.0, &mut flow, &mut clock, &mut orbit, &mut sel);
        clock.paused = cli.screenshot.is_some();
        return;
    }
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
    let framed = orbit.clone();
    begin(&sim.0, &mut flow, &mut clock, &mut orbit, &mut sel);
    if matches!(cli.cam.as_str(), "lodge" | "pond") {
        *orbit = framed;
    }
    if matches!(cli.cam.as_str(), "iso-close" | "iso-wide") {
        if let Some(p) = crate::camera::preset(&cli.cam) {
            *orbit = p;
        }
    }
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

#[derive(Resource, Default)]
struct DemoPond {
    watch: Option<PondWatch>,
    done_at: Option<f32>,
}

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
    clock.paused = true;
    if demo.watch.is_none() {
        demo.watch = dam.watch(&sim.0);
        if demo.watch.is_none() {
            error!("--demo-pond: the designed dam is not a valid dam line");
            exit.write(AppExit::error());
            return;
        }
        sim.0.work.auto_organise = true;
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
