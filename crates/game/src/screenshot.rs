//! `--screenshot` mode: after N frames save the primary window to a PNG, then exit.
use crate::cli::Cli;
use bevy::app::AppExit;
use bevy::prelude::*;
use bevy::render::view::screenshot::{save_to_disk, Screenshot};
use std::path::PathBuf;

/// Frames to wait for the PNG to appear before failing.
const WRITE_TIMEOUT_FRAMES: u32 = 600;

pub struct ScreenshotPlugin;

impl Plugin for ScreenshotPlugin {
    fn build(&self, app: &mut App) {
        if app.world().resource::<Cli>().screenshot.is_some() {
            app.add_systems(Last, drive_screenshot);
        }
    }
}

#[derive(Default)]
struct Progress {
    frame: u32,
    requested_at: Option<u32>,
    last_size: Option<u64>,
}

fn drive_screenshot(mut commands: Commands, cli: Res<Cli>, mut p: Local<Progress>, mut exit: MessageWriter<AppExit>) {
    let path: &PathBuf = cli.screenshot.as_ref().expect("system only added in screenshot mode");
    p.frame += 1;
    match p.requested_at {
        None if p.frame >= cli.frames => {
            // Remove a stale file so we only ever accept the one we are about to write.
            let _ = std::fs::remove_file(path);
            if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
                if let Err(e) = std::fs::create_dir_all(dir) {
                    error!("cannot create {}: {e}", dir.display());
                    exit.write(AppExit::error());
                    return;
                }
            }
            commands.spawn(Screenshot::primary_window()).observe(save_to_disk(path.clone()));
            p.requested_at = Some(p.frame);
        }
        None => {}
        Some(at) => {
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
        }
    }
}

