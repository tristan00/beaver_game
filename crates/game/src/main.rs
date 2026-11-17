//! Entry point: parses the command line and only adds plugins.
mod audio;
mod build;
mod build_logic;
mod camera;
mod cli;
mod entities_render;
mod flow;
mod flow_view;
mod input;
mod screenshot;
mod sim_driver;
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
    let mut app = App::new();
    app.add_plugins(DefaultPlugins.set(WindowPlugin {
        primary_window: Some(Window { title: "Beaver Colony".into(), resolution: (1280, 720).into(), ..default() }),
        ..default()
    }))
    .insert_resource(cli)
    .add_plugins((
        state::StatePlugin,
        sim_driver::SimDriverPlugin,
        flow::FlowPlugin,
        flow_view::FlowViewPlugin,
        build::BuildPlugin,
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
