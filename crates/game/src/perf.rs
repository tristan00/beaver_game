//! Where each frame's time goes, logged every few seconds when `BC_PERF=SECONDS` is set.
//!
//! `BC_SPEED=1|2|3` starts the clock at that game speed and `BC_PERF_EXIT=SECONDS` quits after
//! that long. Set `BC_SETTLE=1` to found the selected map automatically; for example,
//! `BC_PERF=5 BC_SPEED=3 BC_PERF_EXIT=30 BC_SETTLE=1 beaver_colony --map telemark`.

use crate::sim_driver::{SimClock, Speed, BASE_DAYS_PER_SECOND};
use crate::state::Sim;
use bevy::prelude::*;
use std::sync::atomic::{AtomicU64, Ordering};

/// Nanoseconds spent building water meshes (added to by the water renderer).
pub static WATER_MESH_NS: AtomicU64 = AtomicU64::new(0);
/// Nanoseconds spent building terrain meshes.
pub static TERRAIN_MESH_NS: AtomicU64 = AtomicU64::new(0);

pub struct PerfPlugin;

impl Plugin for PerfPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, start_speed).add_systems(Last, report);
    }
}

fn env_f32(name: &str) -> Option<f32> {
    std::env::var(name).ok().and_then(|v| v.parse().ok())
}

/// While the colony runs, hold the speed `BC_SPEED` asks for (the start flow pauses the clock
/// when it settles the family, so this keeps applying it).
fn start_speed(mut clock: ResMut<SimClock>, sim: Res<Sim>) {
    if !sim.0.is_running() {
        return;
    }
    let want = match std::env::var("BC_SPEED").as_deref() {
        Ok("1") => Speed::X1,
        Ok("2") => Speed::X2,
        Ok("3") => Speed::X3,
        _ => return,
    };
    if clock.paused || clock.speed != want {
        clock.set_speed(want);
    }
}

#[derive(Default)]
struct Window {
    since: f32,
    frames: u32,
    frame_max: f32,
    days0: f64,
    sim0: sim::StepTimings,
    water0: sim::water::WaterTimings,
    water_mesh0: u64,
    terrain_mesh0: u64,
    started: bool,
}

fn report(time: Res<Time<Real>>, sim: Res<Sim>, clock: Res<SimClock>, mut w: Local<Window>, mut exit: MessageWriter<AppExit>) {
    let Some(every) = env_f32("BC_PERF") else { return };
    let s = &sim.0;
    if !w.started {
        *w = Window { days0: s.time_days, sim0: s.step_timings, water0: s.water.timings, started: true, ..default() };
        w.water_mesh0 = WATER_MESH_NS.load(Ordering::Relaxed);
        w.terrain_mesh0 = TERRAIN_MESH_NS.load(Ordering::Relaxed);
        return;
    }
    let dt = time.delta_secs();
    w.since += dt;
    w.frames += 1;
    w.frame_max = w.frame_max.max(dt);
    if w.since >= every {
        let secs = w.since as f64;
        let st = s.step_timings;
        let ms = |a: f64, b: f64| (a - b) * 1000.0 / secs;
        let speed = (s.time_days - w.days0) / secs / BASE_DAYS_PER_SECOND as f64;
        let day_seconds = secs / (s.time_days - w.days0).max(f64::MIN_POSITIVE);
        let hydraulic_day_seconds = (s.water.timings.physics_s - w.water0.physics_s) / (s.time_days - w.days0).max(f64::MIN_POSITIVE);
        let wanted = if clock.paused { 0.0 } else { clock.speed.multiplier() as f64 };
        let wm = (WATER_MESH_NS.load(Ordering::Relaxed) - w.water_mesh0) as f64 / 1e6 / secs;
        let tm = (TERRAIN_MESH_NS.load(Ordering::Relaxed) - w.terrain_mesh0) as f64 / 1e6 / secs;
        let wt = s.water.timings;
        let d = sim::water::WaterTimings {
            faces_s: wt.faces_s - w.water0.faces_s,
            faces_x_s: wt.faces_x_s - w.water0.faces_x_s,
            limiter_s: wt.limiter_s - w.water0.limiter_s,
            continuity_s: wt.continuity_s - w.water0.continuity_s,
            pools_s: wt.pools_s - w.water0.pools_s,
            activity_s: wt.activity_s - w.water0.activity_s,
            publish_s: wt.publish_s - w.water0.publish_s,
            upkeep_s: wt.upkeep_s - w.water0.upkeep_s,
            substeps: wt.substeps - w.water0.substeps,
            cell_updates: wt.cell_updates - w.water0.cell_updates,
            physics_s: wt.physics_s - w.water0.physics_s,
            pool_cells: wt.pool_cells - w.water0.pool_cells,
            pool_samples: wt.pool_samples - w.water0.pool_samples,
        };
        info!(
            "perf: {:.0} fps (frame avg {:.1} ms, max {:.1} ms) | speed x{:.1} of x{:.0} wanted | {day_seconds:.2} real seconds/day | {hydraulic_day_seconds:.0} hydraulic seconds/day | per second: sim {:.1} ms = water {:.1} + ecology {:.1} + colony {:.1} + buildings {:.1} + agents {:.1} + shipping {:.1} + fog {:.1} + other {:.1}; meshes water {:.1} terrain {:.1}",
            w.frames as f64 / secs,
            secs * 1000.0 / w.frames as f64,
            w.frame_max * 1000.0,
            speed,
            wanted,
            ms(st.total_s(), w.sim0.total_s()),
            ms(st.water_s, w.sim0.water_s),
            ms(st.ecology_s, w.sim0.ecology_s),
            ms(st.colony_s, w.sim0.colony_s),
            ms(st.buildings_s, w.sim0.buildings_s),
            ms(st.agents_s, w.sim0.agents_s),
            ms(st.shipping_s, w.sim0.shipping_s),
            ms(st.fog_s, w.sim0.fog_s),
            ms(st.other_s, w.sim0.other_s),
            wm,
            tm,
        );
        info!("perf: {}", d.report(secs));
        *w = Window { days0: s.time_days, sim0: st, water0: wt, started: true, ..default() };
        w.water_mesh0 = WATER_MESH_NS.load(Ordering::Relaxed);
        w.terrain_mesh0 = TERRAIN_MESH_NS.load(Ordering::Relaxed);
    }
    if let Some(limit) = env_f32("BC_PERF_EXIT") {
        if time.elapsed_secs() >= limit {
            exit.write(AppExit::Success);
        }
    }
}
