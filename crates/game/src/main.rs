//! Entry point: parses the command line and only adds plugins.
mod audio;
mod build_logic;
mod camera;
mod cli;
mod entities_render;
mod flow;
mod hud;
mod input;
mod opening;
mod palette;
mod screenshot;
mod showcase;
mod sim_driver;
mod startflow;
mod state;
mod terrain_render;
mod water_render;

use bevy::prelude::*;

/// Window size: 1280x720, or `BEAVER_WINDOW=WxH` (screenshots at 1920x1080).
fn window_size() -> (u32, u32) {
    std::env::var("BEAVER_WINDOW")
        .ok()
        .and_then(|v| v.split_once('x').and_then(|(w, h)| Some((w.parse().ok()?, h.parse().ok()?))))
        .unwrap_or((1280, 720))
}

fn main() -> AppExit {
    let cli = match cli::Cli::parse(std::env::args().skip(1)) {
        Ok(cli) => cli,
        Err(msg) => {
            eprintln!("error: {msg}\n\n{}", cli::USAGE);
            std::process::exit(2);
        }
    };
    os_timer::high_resolution();
    let mut app = App::new();
    let mut tp = TaskPoolOptions::default();
    if let Some(n) = std::env::var("BEAVER_THREADS").ok().and_then(|v| v.parse::<usize>().ok()) {
        tp.compute = bevy::app::TaskPoolThreadAssignmentPolicy { min_threads: n, max_threads: n, percent: 1.0, on_thread_spawn: None, on_thread_destroy: None };
    }
    app.add_plugins(DefaultPlugins.set(TaskPoolPlugin { task_pool_options: tp }).set(AssetPlugin {
        // Bevy's default root is the crate folder or the executable folder; our assets live at the workspace root.
        file_path: "assets".to_string(),
        ..default()
    }).set(WindowPlugin {
        primary_window: Some(Window { title: "Beaver Colony".into(), resolution: window_size().into(), ..default() }),
        ..default()
    }))
    .insert_resource(cli)
    .add_plugins((
        state::StatePlugin,
        sim_driver::SimDriverPlugin,
        flow::FlowPlugin,
        camera::CameraPlugin,
        showcase::ShowcasePlugin,
        input::InputPlugin,
        startflow::StartPlugin,
        hud::HudPlugin,
    ))
    .add_plugins((
        terrain_render::TerrainRenderPlugin,
        water_render::WaterRenderPlugin,
        entities_render::EntitiesRenderPlugin,
        audio::AudioPlugin,
        screenshot::ScreenshotPlugin,
    ));
    // Propagate the exit status (e.g. a failed `--stress` budget) to the shell.
    app.run()
}

/// OS scheduling tweaks for smooth frame pacing.
///
/// Bevy spreads each frame over its worker threads and the main thread waits for all of them. On a
/// loaded machine Windows can preempt one worker for a full 15.6 ms scheduler tick, turning a 3 ms
/// frame into a 16 or 32 ms hitch (measured with `--stress`). Like most games we therefore ask for a
/// 1 ms timer resolution and above-normal process priority, and opt out of Windows 11 power throttling
/// (EcoQoS), which otherwise parks a window that is not in the foreground on efficiency cores and
/// ignores its timer request. Windows undoes all of this when the process exits.
mod os_timer {
    #[cfg(windows)]
    pub fn high_resolution() {
        type Handle = *mut core::ffi::c_void;
        const ABOVE_NORMAL_PRIORITY_CLASS: u32 = 0x0000_8000;
        #[link(name = "winmm")]
        unsafe extern "system" {
            fn timeBeginPeriod(period_ms: u32) -> u32;
        }
        #[repr(C)]
        struct PowerThrottlingState {
            version: u32,
            control_mask: u32,
            state_mask: u32,
        }
        const PROCESS_POWER_THROTTLING: i32 = 4;
        const THROTTLE_EXECUTION_SPEED: u32 = 0x1;
        const THROTTLE_IGNORE_TIMER_RESOLUTION: u32 = 0x4;
        #[link(name = "kernel32")]
        unsafe extern "system" {
            fn GetCurrentProcess() -> Handle;
            fn SetPriorityClass(process: Handle, class: u32) -> i32;
            fn SetProcessInformation(process: Handle, class: i32, info: *const core::ffi::c_void, size: u32) -> i32;
        }
        // Control both mechanisms, enable neither: never throttle this process.
        let no_throttle = PowerThrottlingState {
            version: 1,
            control_mask: THROTTLE_EXECUTION_SPEED | THROTTLE_IGNORE_TIMER_RESOLUTION,
            state_mask: 0,
        };
        // SAFETY: Win32 calls on the current-process pseudo handle; `no_throttle` is a live, correctly
        // sized PROCESS_POWER_THROTTLING_STATE for the duration of the call. Failures are harmless (default scheduling).
        unsafe {
            timeBeginPeriod(1);
            SetPriorityClass(GetCurrentProcess(), ABOVE_NORMAL_PRIORITY_CLASS);
            SetProcessInformation(
                GetCurrentProcess(),
                PROCESS_POWER_THROTTLING,
                (&raw const no_throttle).cast(),
                size_of::<PowerThrottlingState>() as u32,
            );
        }
    }

    #[cfg(not(windows))]
    pub fn high_resolution() {}
}
