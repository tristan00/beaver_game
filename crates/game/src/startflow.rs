//! Entering a start: `--start N` builds it at launch, otherwise the menu opens and builds the
//! chosen start. `--demo-pond` scripts start 1 for screenshots: order the designed dam, let the
//! family build it, hold the picture once the pond is full.
use crate::camera::OrbitCamera;
use crate::cli::Cli;
use crate::lake_cam::{LakeCells, LakeView};
use crate::flow::{cell_world, open_at, CaptureGate, Flow, Phase, Selection};
use crate::sim_driver::{SimClock, FIXED_DT_DAYS};
use crate::state::{PlannedDam, Sim};
use crate::ui::menu::{self, MenuRoot};
use bevy::window::PrimaryWindow;
use bevy::app::AppExit;
use bevy::prelude::*;
use sim::starts::{self, DamSite, PondWatch, Start};
use sim::{Command, SimState};

/// Camera distance when a start begins.
const START_DISTANCE: f32 = 85.0;
/// Seconds the menu is on screen before a `--screenshot` may capture it (fonts and layout settle).
const MENU_CAPTURE_SECS: f32 = 1.5;
/// Seconds the finished demo holds before capture.
const DEMO_HOLD_SECS: f32 = 2.0;
/// The demo gives up (exits with an error) if the pond is not full after this many days.
const DEMO_MAX_DAYS: f64 = 200.0;
/// Sim steps the demo runs per frame while fast-forwarding.
const DEMO_STEPS_PER_FRAME: usize = 100;

/// Builds `start` for `seed`; start 1 also returns the dam it is designed around.
pub fn build_start(start: Start, seed: u64) -> Result<(SimState, Option<DamSite>), String> {
    build_start_lake(start, seed).map(|(s, d, _)| (s, d))
}

/// As [`build_start`], and starts 2 and 3 also return their dam and lake (for `--cam lake`).
pub fn build_start_lake(start: Start, seed: u64) -> Result<(SimState, Option<DamSite>, Option<LakeCells>), String> {
    match start {
        Start::River => starts::build_river_with_dam(seed).map(|(s, d)| (s, Some(d), None)).map_err(|e| e.to_string()),
        other => starts::build_lake(other, seed).map(|(s, l)| (s, None, Some(LakeCells::from(&l)))).map_err(|e| e.to_string()),
    }
}

/// `--cam lake` once a lake start has begun: centre and zoom on its dam and lake. Every other preset is left alone.
fn frame_lake(cli: &Cli, lake: &LakeView, sim: &SimState, window: Option<&Window>, orbit: &mut OrbitCamera) {
    let (Some(cells), true) = (lake.0.as_ref(), cli.cam == "lake") else { return };
    let size = window.map_or(Vec2::new(1280.0, 720.0), |w| Vec2::new(w.width(), w.height()));
    let base = crate::camera::preset("lake").expect("lake preset");
    *orbit = crate::lake_cam::frame(base, cells, &sim.height, size);
    info!("--cam lake: {} dam cells, {} lake cells at level {:.2}; camera target {:?}, distance {:.1}", cells.dam.len(), cells.lake.len(), cells.level, orbit.target, orbit.distance);
}

/// Where the camera looks when `sim` begins: the designed dam if there is one (the family camps
/// within a few cells of it), else the family.
fn focus_cell(sim: &SimState, dam: Option<&DamSite>) -> (f32, f32) {
    match dam {
        Some(d) => {
            let m = d.cells[d.cells.len() / 2];
            (m.0 as f32, m.1 as f32)
        }
        None => sim.colony.beavers.first().map_or((0.0, 0.0), |b| b.pos),
    }
}

/// Starts play on a freshly built `sim`: the opening phases are skipped, the family's valley is
/// in view and the camera looks at the start.
pub fn begin(sim: &SimState, dam: Option<&DamSite>, flow: &mut Flow, clock: &mut SimClock, orbit: &mut OrbitCamera, sel: &mut Selection) {
    flow.phase = Phase::Settled;
    flow.settled_at = Some(flow.real_secs);
    let focus = focus_cell(sim, dam);
    let camp = (focus.0 as usize, focus.1 as usize);
    if let Some(v) = sim.world.valley_of(camp) {
        flow.fog.reveal_valley(&sim.world, v);
    }
    flow.fog.reveal(focus, 25.0, &open_at(&sim.world));
    flow.focus = None;
    orbit.target = cell_world(&sim.height, focus);
    orbit.distance = START_DISTANCE;
    *clock = SimClock::default();
    *sel = Selection::None;
}

pub struct StartPlugin;

impl Plugin for StartPlugin {
    fn build(&self, app: &mut App) {
        let cli = app.world().resource::<Cli>();
        // `BC_NO_MENU=1`: no start menu over a staged `--showcase` still, so its colours can be sampled.
        let scripted_opening = cli.autoplay_settle || cli.stress || std::env::var_os("BC_NO_MENU").is_some();
        let (has_start, demo_pond) = (cli.start.is_some(), cli.demo_pond);
        if has_start {
            app.add_systems(Startup, begin_cli_start);
            if demo_pond {
                app.insert_resource(CaptureGate(false)).init_resource::<DemoPond>().add_systems(Update, demo_pond_script);
            }
        } else if !scripted_opening {
            app.insert_resource(CaptureGate(false))
                .add_systems(Startup, open_menu)
                .add_systems(Update, (menu::hover, choose_start, open_menu_gate));
        }
    }
}

fn begin_cli_start(
    cli: Res<Cli>,
    sim: Res<Sim>,
    planned: Res<PlannedDam>,
    mut flow: ResMut<Flow>,
    mut clock: ResMut<SimClock>,
    mut orbit: ResMut<OrbitCamera>,
    mut sel: ResMut<Selection>,
    lake: Res<LakeView>,
    windows: Query<&Window, With<PrimaryWindow>>,
) {
    // `--cam lodge` / `pond`: keep the view the camera plugin framed (no menu veil over the shot).
    let framed = orbit.clone();
    begin(&sim.0, planned.0.as_ref(), &mut flow, &mut clock, &mut orbit, &mut sel);
    if matches!(cli.cam.as_str(), "lodge" | "pond") {
        *orbit = framed;
    }
    frame_lake(&cli, &lake, &sim.0, windows.iter().next(), &mut orbit);
    // `--cam iso-close` / `iso-wide`: fixed views of the map centre, whatever the start (shadow checks, docs/STYLE.md section 7).
    if matches!(cli.cam.as_str(), "iso-close" | "iso-wide") {
        if let Some(p) = crate::camera::preset(&cli.cam) {
            *orbit = p;
        }
    }
}

fn open_menu(mut commands: Commands, mut flow: ResMut<Flow>) {
    flow.phase = Phase::Menu;
    menu::spawn(&mut commands, None);
}

fn open_menu_gate(flow: Res<Flow>, mut gate: ResMut<CaptureGate>) {
    gate.0 = flow.phase != Phase::Menu || flow.real_secs >= MENU_CAPTURE_SECS;
}

/// Builds the chosen start and replaces the menu with the game. A start that cannot be built
/// leaves the menu up with the reason on it.
#[allow(clippy::too_many_arguments)]
fn choose_start(
    mut commands: Commands,
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    cards: menu::CardQuery,
    roots: Query<Entity, With<MenuRoot>>,
    cli: Res<Cli>,
    mut sim: ResMut<Sim>,
    mut planned: ResMut<PlannedDam>,
    mut flow: ResMut<Flow>,
    mut clock: ResMut<SimClock>,
    mut orbit: ResMut<OrbitCamera>,
    mut sel: ResMut<Selection>,
    mut lake: ResMut<LakeView>,
) {
    if flow.phase != Phase::Menu {
        return;
    }
    let Some(start) = menu::chosen(&keys, &mouse, &cards, &windows) else { return };
    for old in &roots {
        commands.entity(old).despawn();
    }
    match build_start_lake(start, cli.seed) {
        Ok((state, dam, cells)) => {
            let (generation, real_secs) = (flow.generation + 1, flow.real_secs);
            *flow = Flow::new(&state.world);
            flow.generation = generation;
            flow.real_secs = real_secs;
            begin(&state, dam.as_ref(), &mut flow, &mut clock, &mut orbit, &mut sel);
            lake.0 = cells;
            frame_lake(&cli, &lake, &state, windows.iter().next(), &mut orbit);
            sim.0 = state;
            planned.0 = dam;
        }
        Err(e) => {
            error!("cannot build start {}: {e}", start.number());
            menu::spawn(&mut commands, Some(&format!("Start {} could not be built: {e}", start.number())));
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

