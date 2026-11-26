use crate::camera::OrbitCamera;
use crate::cli::Cli;
use crate::flow::{cell_world, Flow, Selection};
use crate::sim_driver::SimClock;
use crate::state::{PlannedDam, Sim};
use bevy::prelude::*;
use sim::{Command, SimState};

const ARRIVAL_DISTANCE: f32 = 18.0;
const ARRIVAL_PITCH: f32 = 1.2;

fn family_centre(sim: &SimState) -> (f32, f32) {
    let n = sim.colony.beavers.len().max(1) as f32;
    let sum = sim.colony.beavers.iter().fold((0.0, 0.0), |a, b| (a.0 + b.pos.0, a.1 + b.pos.1));
    (sum.0 / n, sum.1 / n)
}

pub fn begin(sim: &SimState, flow: &mut Flow, clock: &mut SimClock, orbit: &mut OrbitCamera, sel: &mut Selection) {
    flow.arrive_left = 3.0;
    let focus = family_centre(sim);

    flow.focus = None;
    orbit.target = cell_world(&sim.height, focus);
    orbit.distance = ARRIVAL_DISTANCE;
    orbit.pitch = ARRIVAL_PITCH;
    *clock = SimClock::default();
    clock.paused = true;
    *sel = Selection::None;
}

pub fn begin_play(sim: &mut SimState, flow: &mut Flow, clock: &mut SimClock, orbit: &mut OrbitCamera, sel: &mut Selection, camp: Option<(usize, usize)>) -> Result<(), String> {
    if sim.sandbox {
        begin_sandbox(sim, flow, clock, orbit, sel);
    } else {
        if !sim.colony.is_founded() {
            let cell = camp.filter(|&c| sim::site::can_settle(sim, c).is_ok())
                .or_else(|| sim::site::best_site(sim))
                .ok_or_else(|| "No suitable starting spot was found in this valley.".to_string())?;
            crate::flow::settle(sim, flow, cell)?;
        }
        begin(sim, flow, clock, orbit, sel);
    }
    Ok(())
}

pub fn begin_scouting(sim: &SimState, flow: &mut Flow, clock: &mut SimClock, orbit: &mut OrbitCamera, sel: &mut Selection, around: Option<(usize, usize)>) {
    flow.arrive_left = 0.0;
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
    flow.arrive_left = 3.0;
    flow.focus = None;
    let half = sim.world.size as f32 / 2.0;
    let n = sim.world.size;
    let stream = (0..n * n)
        .filter(|&i| sim.water.depth.as_slice()[i] > 0.015 && sim.water.discharge.as_slice()[i] > 0.002)
        .min_by_key(|&i| (i % n).abs_diff(n / 2).pow(2) + (i / n).abs_diff(n / 2).pow(2))
        .map(|i| ((i % n) as f32, (i / n) as f32))
        .unwrap_or((half, half));
    orbit.target = cell_world(&sim.height, stream);
    orbit.distance = SANDBOX_DISTANCE;
    orbit.pitch = SCOUT_PITCH;
    *clock = SimClock::default();
    *sel = Selection::None;
}

const SANDBOX_DISTANCE: f32 = 100.0;
const SCOUT_DISTANCE: f32 = 60.0;
const SCOUT_PITCH: f32 = 1.0;

fn install_valley(
    mut s: SimState,
    dam: Option<sim::starts::DamSite>,
    sim: &mut Sim,
    planned: &mut PlannedDam,
    flow: &mut Flow,
    setup: impl FnOnce(&mut SimState, &mut Flow) -> Result<(), String>,
) -> Result<(), String> {
    start_recording(&mut s);
    let mut next_flow = Flow::default();
    setup(&mut s, &mut next_flow)?;
    sim.0 = s;
    planned.0 = dam;
    *flow = next_flow;
    Ok(())
}

pub fn install_game(
    s: SimState,
    dam: Option<sim::starts::DamSite>,
    sim: &mut Sim,
    planned: &mut PlannedDam,
    flow: &mut Flow,
    clock: &mut SimClock,
    orbit: &mut OrbitCamera,
    sel: &mut Selection,
) -> Result<(), String> {
    let camp = dam.as_ref().map(|d| d.camp);
    install_valley(s, dam, sim, planned, flow, |s, f| begin_play(s, f, clock, orbit, sel, camp))
}

/// Put a freshly built valley behind the start screen, unsettled, as a new game starts on it.
pub fn install_preview(
    s: SimState,
    dam: Option<sim::starts::DamSite>,
    sim: &mut Sim,
    planned: &mut PlannedDam,
    flow: &mut Flow,
    clock: &mut SimClock,
    orbit: &mut OrbitCamera,
    sel: &mut Selection,
) {
    let camp = dam.as_ref().map(|d| d.camp);
    let _ = install_valley(s, dam, sim, planned, flow, |s, f| {
        if s.sandbox {
            begin_sandbox(s, f, clock, orbit, sel);
        } else {
            begin_scouting(s, f, clock, orbit, sel, camp);
        }
        clock.paused = true;
        Ok(())
    });
}

pub struct StartPlugin;

impl Plugin for StartPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, begin_new_game).add_systems(Update, save_recording);
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
    let header = format!("# map seed {}\n", sim.0.seed);
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
    if sim.0.colony.is_founded() {
        // A loaded game: carry on where it was.
        begin(&sim.0, &mut flow, &mut clock, &mut orbit, &mut sel);
        clock.paused = cli.screenshot.is_some();
        return;
    }
    let settle = std::env::var_os("BC_SETTLE").is_some();
    let benchmark = settle && std::env::var_os("BC_PERF").is_some();
    let scripted = cli.colony_script.is_some() || benchmark || cli.screenshot.is_some() && settle;
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
    if cli.colony_script.is_some() {
        if let Some(dam) = &planned.0 {
            orbit.target = cell_world(&sim.0.height, (dam.camp.0 as f32, dam.camp.1 as f32));
            orbit.distance = 42.0;
            orbit.pitch = 0.95;
        }
    }
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
