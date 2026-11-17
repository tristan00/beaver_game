//! Entry point: parses the command line and only adds plugins.
mod audio;
mod camera;
mod cli;
mod entities_render;
mod input;
mod screenshot;
mod state;
mod terrain_render;
mod ui;
mod water_render;

use bevy::prelude::*;

fn main() {
    let cli = match cli::Cli::parse(std::env::args().skip(1)) {
        Ok(cli) => cli,
        Err(msg) => {
            eprintln!("error: {msg}\n\n{}", cli::USAGE);
            std::process::exit(2);
        }
    };
    os_timer::high_resolution();
    let mut app = App::new();
    app.add_plugins(DefaultPlugins.set(WindowPlugin {
        primary_window: Some(Window { title: "Beaver Colony".into(), resolution: (1280, 720).into(), ..default() }),
        ..default()
    }))
    .insert_resource(cli)
    .add_plugins((
        state::StatePlugin,
        camera::CameraPlugin,
        input::InputPlugin,
        terrain_render::TerrainRenderPlugin,
        water_render::WaterRenderPlugin,
        entities_render::EntitiesRenderPlugin,
        ui::UiPlugin,
        audio::AudioPlugin,
        screenshot::ScreenshotPlugin,
    ));
    app.run();
}

/// OS scheduling tweaks for smooth frame pacing.
///
/// Bevy spreads each frame over its worker threads and the main thread waits for all of them. On a
/// loaded machine Windows can preempt one worker for a full 15.6 ms scheduler tick, turning a 3 ms
/// frame into a 16 or 32 ms hitch (measured with `--stress`). Like most games we therefore ask for a
/// 1 ms timer resolution and above-normal process priority; Windows undoes both when the process exits.
mod os_timer {
    #[cfg(windows)]
    pub fn high_resolution() {
        type Handle = *mut core::ffi::c_void;
        const ABOVE_NORMAL_PRIORITY_CLASS: u32 = 0x0000_8000;
        #[link(name = "winmm")]
        unsafe extern "system" {
            fn timeBeginPeriod(period_ms: u32) -> u32;
        }
        #[link(name = "kernel32")]
        unsafe extern "system" {
            fn GetCurrentProcess() -> Handle;
            fn SetPriorityClass(process: Handle, class: u32) -> i32;
        }
        // SAFETY: plain Win32 calls with constant arguments and the current-process pseudo handle;
        // none of them has memory-safety preconditions. Failures are harmless (default scheduling).
        unsafe {
            timeBeginPeriod(1);
            SetPriorityClass(GetCurrentProcess(), ABOVE_NORMAL_PRIORITY_CLASS);
        }
    }

    #[cfg(not(windows))]
    pub fn high_resolution() {}
}
