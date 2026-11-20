mod audio;
mod build_logic;
mod camera;
mod cli;
mod entities_render;
mod flow;
mod fog_render;
mod hud;
mod icons;
mod input;
mod input_trace;
mod menu;
mod palette;
mod perf;
mod report_capture;
mod sandbox;
mod screenshot;
mod seasonal;
mod sim_driver;
mod startflow;
mod state;
mod terrain_render;
mod water_render;

use bevy::prelude::*;

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
        file_path: "assets".to_string(),
        ..default()
    }).set(WindowPlugin {
        primary_window: Some(Window { title: if sim::trace::enabled() { "Beaver Colony - input recording" } else { "Beaver Colony" }.into(), resolution: window_size().into(), ..default() }),
        ..default()
    }).set(bevy::render::RenderPlugin {
        render_creation: bevy::render::settings::WgpuSettings {
            backends: Some(bevy::render::settings::Backends::from_env().unwrap_or(if cfg!(windows) {
                bevy::render::settings::Backends::VULKAN
            } else {
                bevy::render::settings::Backends::PRIMARY
            })),
            ..default()
        }
        .into(),
        ..default()
    }))
    .insert_resource(cli)
    .add_systems(Last, log_slow_frames)
    .add_plugins((
        state::StatePlugin,
        sim_driver::SimDriverPlugin,
        flow::FlowPlugin,
        camera::CameraPlugin,
        input::InputPlugin,
        input_trace::InputTracePlugin,
        startflow::StartPlugin,
        icons::IconsPlugin,
        hud::HudPlugin,
        menu::MenuPlugin,
        sandbox::SandboxPlugin,
        perf::PerfPlugin,
        report_capture::ReportCapturePlugin,
    ))
    .add_plugins((
        terrain_render::TerrainRenderPlugin,
        fog_render::FogRenderPlugin,
        water_render::WaterRenderPlugin,
        entities_render::EntitiesRenderPlugin,
        audio::AudioPlugin,
        screenshot::ScreenshotPlugin,
    ));
    app.run()
}

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
        let no_throttle = PowerThrottlingState {
            version: 1,
            control_mask: THROTTLE_EXECUTION_SPEED | THROTTLE_IGNORE_TIMER_RESOLUTION,
            state_mask: 0,
        };
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

fn log_slow_frames(time: Res<Time<Real>>, mut n: Local<u64>) {
    *n += 1;
    if std::env::var_os("BC_FRAME_LOG").is_none() {
        return;
    }
    let dt = time.delta_secs();
    if dt > 0.05 || *n % 300 == 0 {
        info!("frame {} at {:.2}s took {:.0} ms", *n, time.elapsed_secs(), dt * 1000.0);
    }
}
