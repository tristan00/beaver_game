//! Orbit camera: `OrbitCamera` holds the parameters, this plugin applies them to the transform.
use bevy::prelude::*;

#[derive(Resource, Clone, Copy, Debug)]
pub struct OrbitCamera {
    pub target: Vec3,
    pub yaw: f32,
    pub pitch: f32,
    pub distance: f32,
}

impl OrbitCamera {
    pub const MIN_DISTANCE: f32 = 5.0;
    pub const MAX_DISTANCE: f32 = 400.0;
    pub const MAX_PITCH: f32 = 1.55;
    pub const MIN_PITCH: f32 = 0.05;

    /// Clamps pitch and distance into their valid ranges.
    pub fn clamp(&mut self) {
        self.pitch = self.pitch.clamp(Self::MIN_PITCH, Self::MAX_PITCH);
        self.distance = self.distance.clamp(Self::MIN_DISTANCE, Self::MAX_DISTANCE);
    }

    /// World-space camera position.
    pub fn eye(&self) -> Vec3 {
        let dir = Vec3::new(self.yaw.sin() * self.pitch.cos(), self.pitch.sin(), self.yaw.cos() * self.pitch.cos());
        self.target + dir * self.distance
    }

    fn transform(&self) -> Transform {
        Transform::from_translation(self.eye()).looking_at(self.target, Vec3::Y)
    }
}

/// Named camera presets for `--cam`: "iso", "top", "low".
pub fn preset(name: &str) -> Option<OrbitCamera> {
    let target = Vec3::new(0.0, 6.0, 0.0);
    let (yaw, pitch, distance) = match name {
        "iso" => (0.8, 0.7, 150.0),
        "top" => (0.0, 1.5, 170.0),
        "low" => (0.5, 0.25, 110.0),
        // Target and yaw are refined from the terrain in `CameraPlugin::build` (see `entities_render::lodge_view`).
        "lodge" => (0.0, 0.55, 32.0),
        _ => return None,
    };
    Some(OrbitCamera { target, yaw, pitch, distance })
}

#[derive(Component)]
pub struct MainCamera;

pub struct CameraPlugin;

impl Plugin for CameraPlugin {
    fn build(&self, app: &mut App) {
        let name = app.world().resource::<crate::cli::Cli>().cam.clone();
        let mut orbit = preset(&name).expect("preset validated by Cli::parse");
        if name == "lodge" {
            let (target, yaw) = crate::entities_render::lodge_view(&app.world().resource::<crate::state::Sim>().0.height);
            orbit.target = target;
            orbit.yaw = yaw;
        }
        app.insert_resource(orbit)
            .insert_resource(ClearColor(Color::srgb(0.55, 0.75, 0.92)))
            .add_systems(Startup, spawn_camera)
            .add_systems(Update, apply_orbit);
    }
}

fn spawn_camera(mut commands: Commands, orbit: Res<OrbitCamera>) {
    commands.spawn((Camera3d::default(), orbit.transform(), MainCamera));
    commands.spawn((
        DirectionalLight { illuminance: 12_000.0, shadow_maps_enabled: false, ..default() },
        Transform::from_xyz(60.0, 100.0, 40.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
    commands.insert_resource(GlobalAmbientLight { brightness: 350.0, ..default() });
}

fn apply_orbit(orbit: Res<OrbitCamera>, mut q: Query<&mut Transform, With<MainCamera>>) {
    if orbit.is_changed() {
        for mut t in &mut q {
            *t = orbit.transform();
        }
    }
}

