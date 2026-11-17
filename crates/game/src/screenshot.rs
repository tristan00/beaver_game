//! `--screenshot` mode: wait until the terrain is actually renderable, save the primary window
//! to a PNG, then exit.
//!
//! Capture is gated on render-world state, not on a frame count: the render world reports
//! (through [`SharedReadiness`]) whether the terrain entity is extracted, its mesh and material
//! are prepared on the GPU, and the pipeline cache has nothing left to compile. `--frames N` is
//! only the *minimum* number of frames to run before capturing. If the terrain never becomes
//! renderable within [`READY_TIMEOUT`], or a pipeline fails to compile, the app exits with an
//! error instead of writing a blank image.
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

/// Consecutive frames the render world must report "ready" before capturing. Covers the
/// one-frame lag of pipelined rendering and the frame in which a just-compiled pipeline is
/// first used to queue the terrain.
const READY_STREAK: u32 = 3;
/// Wall-clock limit for the terrain to become renderable (cold shader caches can be slow).
const READY_TIMEOUT: Duration = Duration::from_secs(180);
/// Frames to wait for the PNG to appear after requesting it before failing.
const WRITE_TIMEOUT_FRAMES: u32 = 600;

/// Whether the terrain can be drawn, as last observed by the render world.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Readiness {
    /// Not drawable yet; the string names what is still missing.
    Pending(String),
    Ready,
    /// Will never become drawable (e.g. a pipeline failed to compile).
    Failed(String),
}

/// Readiness shared between the main world and the render world.
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

/// Render-world copy of the terrain's ids, refreshed every extract.
#[derive(Resource, Default)]
struct ExtractedTerrain(Option<(MainEntity, AssetId<Mesh>, AssetId<crate::terrain_render::TerrainMaterial>)>);

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
                    .add_systems(ExtractSchedule, extract_terrain)
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

fn report_readiness(
    shared: Res<SharedReadiness>,
    terrain: Res<ExtractedTerrain>,
    meshes: Option<Res<RenderAssets<RenderMesh>>>,
    materials: Option<Res<ErasedRenderAssets<PreparedMaterial>>>,
    instances: Option<Res<RenderMeshInstances>>,
    pipelines: Res<PipelineCache>,
) {
    shared.set(terrain_readiness(&terrain, meshes.as_deref(), materials.as_deref(), instances.as_deref(), &pipelines));
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
            // Bevy re-queues these two itself once the shader (import) asset loads.
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

/// What the main world should do this frame.
#[derive(Debug, PartialEq, Eq)]
enum Step {
    Wait,
    Capture,
    Fail(String),
}

/// Pure capture decision: capture once at least `min_frames` frames have run and the terrain
/// has been ready for `READY_STREAK` consecutive frames; fail on a hard error or timeout.
fn decide(frame: u32, min_frames: u32, streak: u32, readiness: &Readiness, elapsed: Duration) -> Step {
    match readiness {
        Readiness::Failed(msg) => Step::Fail(msg.clone()),
        _ if frame >= min_frames && streak >= READY_STREAK => Step::Capture,
        Readiness::Pending(what) if elapsed > READY_TIMEOUT => {
            Step::Fail(format!("terrain not renderable after {}s ({what})", READY_TIMEOUT.as_secs()))
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
}

impl Default for Progress {
    fn default() -> Self {
        Progress { start: Instant::now(), frame: 0, streak: 0, requested_at: None, last_size: None }
    }
}

fn drive_screenshot(
    mut commands: Commands,
    cli: Res<Cli>,
    shared: Res<SharedReadiness>,
    mut p: Local<Progress>,
    gate: Option<Res<crate::flow::CaptureGate>>,
    mut exit: MessageWriter<AppExit>,
) {
    let path: &PathBuf = cli.screenshot.as_ref().expect("system only added in screenshot mode");
    p.frame += 1;
    if let Some(at) = p.requested_at {
        // Done once the file exists and its size is stable between two frames.
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
    let readiness = shared.get();
    let open = gate.is_none_or(|g| g.0);
    p.streak = if readiness == Readiness::Ready && open { p.streak + 1 } else { 0 };
    match decide(p.frame, cli.frames, p.streak, &readiness, p.start.elapsed()) {
        Step::Wait => {}
        Step::Fail(msg) => {
            error!("screenshot aborted: {msg}");
            exit.write(AppExit::error());
        }
        Step::Capture => {
            // Remove a stale file so we only ever accept the one we are about to write.
            let _ = std::fs::remove_file(path);
            if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
                if let Err(e) = std::fs::create_dir_all(dir) {
                    error!("cannot create {}: {e}", dir.display());
                    exit.write(AppExit::error());
                    return;
                }
            }
            info!("terrain ready at frame {}; capturing", p.frame);
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
    fn fails_loudly() {
        assert!(matches!(decide(1, 1, 0, &Readiness::Failed("x".into()), T0), Step::Fail(_)));
        let late = READY_TIMEOUT + Duration::from_secs(1);
        assert!(matches!(decide(1, 1, 0, &Readiness::Pending("x".into()), late), Step::Fail(_)));
    }
}
