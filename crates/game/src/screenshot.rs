use crate::cli::Cli;
use crate::terrain_render::Terrain;
use bevy::app::AppExit;
use bevy::pbr::{PreparedMaterial, RenderMeshInstances};
use bevy::prelude::*;
use bevy::render::erased_render_asset::ErasedRenderAssets;
use bevy::render::mesh::RenderMesh;
use bevy::render::render_asset::RenderAssets;
use bevy::render::render_resource::{CachedPipelineState, PipelineCache};
use bevy::render::sync_world::MainEntity;
use bevy::render::view::screenshot::{save_to_disk, Screenshot};
use bevy::shader::ShaderCacheError;
use bevy::render::{Extract, ExtractSchedule, Render, RenderApp, RenderSystems};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const READY_STREAK: u32 = 3;
const READY_TIMEOUT: Duration = Duration::from_secs(180);
const WRITE_TIMEOUT_FRAMES: u32 = 600;
const STABLE_FRAMES: u32 = 20;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Readiness {
    Pending(String),
    Ready,
    Failed(String),
}

#[derive(Resource, Clone)]
struct SharedReadiness(Arc<Mutex<Readiness>>);

impl SharedReadiness {
    fn get(&self) -> Readiness {
        self.0.lock().map(|r| r.clone()).unwrap_or_else(|_| Readiness::Failed("readiness lock poisoned".into()))
    }
    fn set(&self, r: Readiness) {
        if let Ok(mut g) = self.0.lock() {
            *g = r;
        }
    }
}

#[derive(Resource, Default)]
struct ExtractedTerrain(Option<(MainEntity, AssetId<Mesh>, AssetId<crate::terrain_render::TerrainMaterial>)>);

#[derive(Resource, Default)]
struct ExtractedMeshes(std::collections::HashSet<AssetId<Mesh>>);

pub struct ScreenshotPlugin;

impl Plugin for ScreenshotPlugin {
    fn build(&self, app: &mut App) {
        if app.world().resource::<Cli>().screenshot.is_none() {
            return;
        }
        let shared = SharedReadiness(Arc::new(Mutex::new(Readiness::Pending("render world not started".into()))));
        app.insert_resource(shared.clone()).add_systems(Last, drive_screenshot);
        match app.get_sub_app_mut(RenderApp) {
            Some(render_app) => {
                render_app
                    .insert_resource(shared)
                    .init_resource::<ExtractedTerrain>()
                    .init_resource::<ExtractedMeshes>()
                    .add_systems(ExtractSchedule, (extract_terrain, extract_meshes))
                    .add_systems(Render, report_readiness.in_set(RenderSystems::Cleanup));
            }
            None => shared.set(Readiness::Failed("no render app: screenshot mode needs a renderer".into())),
        }
    }
}

fn extract_terrain(
    mut out: ResMut<ExtractedTerrain>,
    q: Extract<Query<(Entity, &Mesh3d, &MeshMaterial3d<crate::terrain_render::TerrainMaterial>), With<Terrain>>>,
) {
    out.0 = q.iter().next().map(|(e, m, mat)| (MainEntity::from(e), m.0.id(), mat.0.id()));
}

fn extract_meshes(mut out: ResMut<ExtractedMeshes>, q: Extract<Query<&Mesh3d>>) {
    out.0.clear();
    out.0.extend(q.iter().map(|m| m.0.id()));
}

fn report_readiness(
    shared: Res<SharedReadiness>,
    terrain: Res<ExtractedTerrain>,
    used: Res<ExtractedMeshes>,
    meshes: Option<Res<RenderAssets<RenderMesh>>>,
    materials: Option<Res<ErasedRenderAssets<PreparedMaterial>>>,
    instances: Option<Res<RenderMeshInstances>>,
    pipelines: Res<PipelineCache>,
) {
    let mut r = terrain_readiness(&terrain, meshes.as_deref(), materials.as_deref(), instances.as_deref(), &pipelines);
    if r == Readiness::Ready {
        let missing = used.0.iter().filter(|id| meshes.as_deref().and_then(|m| m.get(**id)).is_none()).count();
        if missing > 0 {
            r = Readiness::Pending(format!("{missing} meshes not uploaded"));
        }
    }
    shared.set(r);
}

fn terrain_readiness(
    terrain: &ExtractedTerrain,
    meshes: Option<&RenderAssets<RenderMesh>>,
    materials: Option<&ErasedRenderAssets<PreparedMaterial>>,
    instances: Option<&RenderMeshInstances>,
    pipelines: &PipelineCache,
) -> Readiness {
    let pending = |what: &str| Readiness::Pending(what.to_string());
    let Some((entity, mesh, material)) = terrain.0 else { return pending("terrain entity not extracted") };
    if instances.and_then(|i| i.mesh_asset_id(entity)).is_none() {
        return pending("terrain mesh instance not extracted");
    }
    if meshes.and_then(|m| m.get(mesh)).is_none() {
        return pending("terrain mesh not uploaded");
    }
    if materials.and_then(|m| m.get(material)).is_none() {
        return pending("terrain material not prepared");
    }
    let mut total = 0;
    for p in pipelines.pipelines() {
        total += 1;
        match &p.state {
            CachedPipelineState::Ok(_) => {}
            CachedPipelineState::Err(ShaderCacheError::ShaderNotLoaded(_) | ShaderCacheError::ShaderImportNotYetAvailable) => {
                return pending("shaders loading")
            }
            CachedPipelineState::Err(e) => return Readiness::Failed(format!("a render pipeline failed to compile: {e}")),
            CachedPipelineState::Queued | CachedPipelineState::Creating(_) => return pending("pipelines compiling"),
        }
    }
    if total == 0 || pipelines.waiting_pipelines().next().is_some() {
        return pending("pipelines compiling");
    }
    Readiness::Ready
}

#[derive(Debug, PartialEq, Eq)]
enum Step {
    Wait,
    Capture,
    Fail(String),
}

fn decide(frame: u32, min_frames: u32, streak: u32, readiness: &Readiness, elapsed: Duration) -> Step {
    match readiness {
        Readiness::Failed(msg) => Step::Fail(msg.clone()),
        _ if frame >= min_frames && streak >= READY_STREAK => Step::Capture,
        Readiness::Pending(what) if elapsed > READY_TIMEOUT => {
            Step::Fail(format!("scene not loaded after {}s ({what})", READY_TIMEOUT.as_secs()))
        }
        _ => Step::Wait,
    }
}

struct Progress {
    start: Instant,
    frame: u32,
    streak: u32,
    requested_at: Option<u32>,
    last_size: Option<u64>,
    burst: Option<(u32, u32)>,
}

impl Default for Progress {
    fn default() -> Self {
        Progress { start: Instant::now(), frame: 0, streak: 0, requested_at: None, last_size: None, burst: None }
    }
}

fn scene_pending(cli: &Cli, sim: &sim::SimState, sprites: usize, rigged: usize, drawn: usize, stable: &mut (usize, u32), wait_days: f64, meshes_settling: bool) -> Option<String> {
    if !cli.plain_world() && !sim.is_running() {
        return Some("the colony is not founded yet".into());
    }
    let agents = sim.agents.by_id.len();
    if sprites < agents || rigged < sprites {
        return Some(format!("beavers: {rigged} of {agents} drawn"));
    }
    if sim.time_days < wait_days {
        return Some(format!("sim at day {:.2} of {wait_days}", sim.time_days));
    }
    if meshes_settling {
        *stable = (drawn, 0);
        return Some("terrain or water meshes still rebuilding".into());
    }
    if stable.0 != drawn {
        *stable = (drawn, 0);
    } else {
        stable.1 += 1;
    }
    (stable.1 < STABLE_FRAMES).then(|| format!("scene still spawning ({drawn} drawn)"))
}

#[allow(clippy::too_many_arguments)]
fn drive_screenshot(
    mut commands: Commands,
    cli: Res<Cli>,
    shared: Res<SharedReadiness>,
    mut p: Local<Progress>,
    gate: Option<Res<crate::flow::CaptureGate>>,
    mut sim: ResMut<crate::state::Sim>,
    sprites: Query<(), With<crate::entities_render::BeaverSprite>>,
    rigged: Query<(), With<crate::entities_render::BeaverRig>>,
    drawn: Query<(), With<Mesh3d>>,
    mut stable: Local<(usize, u32)>,
    mut clock: ResMut<crate::sim_driver::SimClock>,
    mut exit: MessageWriter<AppExit>,
    backlog: Option<Res<crate::terrain_render::MeshBacklog>>,
) {
    let path: &PathBuf = cli.screenshot.as_ref().expect("system only added in screenshot mode");
    p.frame += 1;
    if let Some((from, taken)) = p.burst {
        let n: u32 = std::env::var("BC_BURST").ok().and_then(|v| v.parse().ok()).unwrap_or(30);
        let every: u32 = std::env::var("BC_BURST_EVERY").ok().and_then(|v| v.parse().ok()).unwrap_or(2).max(1);
        if taken < n {
            if (p.frame - from) % every == 0 {
                let stem = path.with_extension("");
                let file = PathBuf::from(format!("{}_{:03}.png", stem.display(), taken));
                commands.spawn(Screenshot::primary_window()).observe(save_to_disk(file));
                p.burst = Some((from, taken + 1));
            }
        } else if p.frame - from > n * every + 60 {
            info!("burst of {n} frames saved next to {}", path.display());
            exit.write(AppExit::Success);
        }
        return;
    }
    if let Some(at) = p.requested_at {
        let size = std::fs::metadata(path).ok().map(|m| m.len()).filter(|&n| n > 0);
        if size.is_some() && size == p.last_size {
            info!("screenshot saved to {}", path.display());
            exit.write(AppExit::Success);
        } else if p.frame - at > WRITE_TIMEOUT_FRAMES {
            error!("screenshot was not written to {}", path.display());
            exit.write(AppExit::error());
        }
        p.last_size = size;
        return;
    }
    let wait_days = std::env::var("BC_WAIT_DAYS").ok().and_then(|d| d.parse::<f64>().ok()).unwrap_or(0.0);
    if sim.0.is_running() && sim.0.time_days < wait_days {
        for _ in 0..4 {
            if sim.0.time_days >= wait_days {
                break;
            }
            if let Err(e) = sim.0.step(crate::sim_driver::FIXED_DT_DAYS) {
                error!("screenshot aborted: sim step failed: {e}");
                exit.write(AppExit::error());
                return;
            }
        }
    }
    let readiness = match (shared.get(), scene_pending(&cli, &sim.0, sprites.iter().count(), rigged.iter().count(), drawn.iter().count(), &mut stable, wait_days, backlog.is_some_and(|b| b.terrain || b.water))) {
        (Readiness::Ready, Some(what)) => Readiness::Pending(what),
        (r, _) => r,
    };
    if let Readiness::Pending(what) = &readiness {
        if p.frame % 120 == 0 {
            info!("screenshot waiting ({:.0}s): {what}", p.start.elapsed().as_secs_f32());
        }
    }
    let open = gate.is_none_or(|g| g.0);
    p.streak = if readiness == Readiness::Ready && open { p.streak + 1 } else { 0 };
    match decide(p.frame, cli.frames, p.streak, &readiness, p.start.elapsed()) {
        Step::Wait => {}
        Step::Fail(msg) => {
            error!("screenshot aborted: {msg}");
            exit.write(AppExit::error());
        }
        Step::Capture => {
            let _ = std::fs::remove_file(path);
            if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
                if let Err(e) = std::fs::create_dir_all(dir) {
                    error!("cannot create {}: {e}", dir.display());
                    exit.write(AppExit::error());
                    return;
                }
            }
            info!("scene loaded at frame {} (sim day {:.2}); capturing", p.frame, sim.0.time_days);
            if std::env::var_os("BC_BURST").is_some() {
                clock.paused = false;
                clock.speed = crate::sim_driver::Speed::X1;
                p.burst = Some((p.frame, 0));
                return;
            }
            commands.spawn(Screenshot::primary_window()).observe(save_to_disk(path.clone()));
            p.requested_at = Some(p.frame);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const T0: Duration = Duration::ZERO;

    #[test]
    fn waits_for_readiness_even_after_min_frames() {
        let pending = Readiness::Pending("pipelines compiling".into());
        assert_eq!(decide(1000, 1, 0, &pending, T0), Step::Wait);
        assert_eq!(decide(1000, 1, READY_STREAK - 1, &Readiness::Ready, T0), Step::Wait);
        assert_eq!(decide(1000, 1, READY_STREAK, &Readiness::Ready, T0), Step::Capture);
    }

    #[test]
    fn frames_is_a_minimum() {
        assert_eq!(decide(5, 10, 50, &Readiness::Ready, T0), Step::Wait);
        assert_eq!(decide(10, 10, 50, &Readiness::Ready, T0), Step::Capture);
    }

    #[test]
    fn waits_while_terrain_or_water_meshes_rebuild() {
        let cli = Cli::parse(["--sandbox".to_string()]).unwrap();
        let sim = sim::sandbox::on_map("fixture").unwrap();
        let mut stable = (0, 0);
        assert!(scene_pending(&cli, &sim, 0, 0, 5, &mut stable, 0.0, true).is_some());
        for _ in 1..STABLE_FRAMES {
            assert!(scene_pending(&cli, &sim, 0, 0, 5, &mut stable, 0.0, false).is_some());
        }
        assert!(scene_pending(&cli, &sim, 0, 0, 5, &mut stable, 0.0, false).is_none());
    }

    #[test]
    fn fails_loudly() {
        assert!(matches!(decide(1, 1, 0, &Readiness::Failed("x".into()), T0), Step::Fail(_)));
        let late = READY_TIMEOUT + Duration::from_secs(1);
        assert!(matches!(decide(1, 1, 0, &Readiness::Pending("x".into()), late), Step::Fail(_)));
    }
}
