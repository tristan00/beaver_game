use bevy::light::CascadeShadowConfigBuilder;
use bevy::prelude::*;
use bevy::core_pipeline::tonemapping::Tonemapping;

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

    pub fn clamp(&mut self) {
        self.pitch = self.pitch.clamp(Self::MIN_PITCH, Self::MAX_PITCH);
        self.distance = self.distance.clamp(Self::MIN_DISTANCE, Self::MAX_DISTANCE);
    }

    pub fn eye(&self) -> Vec3 {
        let dir = Vec3::new(self.yaw.sin() * self.pitch.cos(), self.pitch.sin(), self.yaw.cos() * self.pitch.cos());
        self.target + dir * self.distance
    }

    pub(crate) fn transform(&self) -> Transform {
        Transform::from_translation(self.eye()).looking_at(self.target, Vec3::Y)
    }
}

pub fn preset(name: &str) -> Option<OrbitCamera> {
    let target = Vec3::new(0.0, 6.0, 0.0);
    let (yaw, pitch, distance) = match name {
        "iso" => (0.8, 0.7, 150.0),
        "iso-close" => (0.8, 0.7, 55.0),
        "iso-wide" => (0.8, 0.7, 150.0),
        "top" => (0.0, 1.5, 170.0),
        "low" => (0.5, 0.25, 110.0),
        "lodge" => (0.0, 0.55, 32.0),
        "pond" => (0.6, 1.2, 24.0),
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
        if name == "pond" {
            if let Some(target) = crate::water_render::pond_view(&app.world().resource::<crate::state::Sim>().0) {
                orbit.target = target;
            }
        }
        app.insert_resource(orbit)
            .insert_resource(ClearColor(Color::srgb(0.55, 0.75, 0.92)))
            .add_systems(Startup, spawn_camera)
            .add_systems(Update, (camera_at_cell, apply_orbit).chain());
    }
}

const SUN_SHADOWS: bool = true;

fn camera_at_cell(sim: Res<crate::state::Sim>, mut orbit: ResMut<OrbitCamera>, spec: Local<Option<Option<Vec<f32>>>>) {
    let mut spec = spec;
    let p = spec.get_or_insert_with(|| {
        std::env::var("BC_CAM_AT").ok().map(|v| v.split(',').filter_map(|t| t.trim().parse().ok()).collect::<Vec<f32>>()).filter(|p| p.len() >= 2)
    });
    let Some(p) = p else { return };
    let s = &sim.0;
    let (x, y) = (p[0] as i32, p[1] as i32);
    let (wx, wz) = crate::terrain_render::world_pos(s, x, y);
    let h = s.height.get(x.max(0) as usize, y.max(0) as usize).copied().unwrap_or(0.0);
    orbit.target = Vec3::new(wx, h, wz);
    orbit.distance = p.get(2).copied().unwrap_or(orbit.distance);
    orbit.yaw = p.get(3).copied().unwrap_or(orbit.yaw);
    orbit.pitch = p.get(4).copied().unwrap_or(orbit.pitch);
}

fn spawn_camera(mut commands: Commands, orbit: Res<OrbitCamera>) {
    commands.spawn((Camera3d::default(), Tonemapping::SomewhatBoringDisplayTransform, orbit.transform(), MainCamera));
    commands.spawn((
        DirectionalLight { illuminance: 1_400.0, shadow_maps_enabled: SUN_SHADOWS && std::env::var_os("BC_NO_SUN_SHADOWS").is_none(), ..default() },
        CascadeShadowConfigBuilder { num_cascades: 3, minimum_distance: 1.0, maximum_distance: 260.0, first_cascade_far_bound: 45.0, overlap_proportion: 0.3 }.build(),
        Transform::from_xyz(-70.0, 90.0, -45.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
    if std::env::var_os("BC_NO_GLOW").is_none() {
        spawn_glow(&mut commands);
    }
    commands.insert_resource(GlobalAmbientLight { brightness: crate::terrain_render::AMBIENT_LUX, ..default() });
}

fn spawn_glow(commands: &mut Commands) {
    let glow = |a: f32| Color::srgba(1.0, 0.96, 0.84, a);
    commands.spawn((
        Node { position_type: PositionType::Absolute, width: Val::Percent(100.0), height: Val::Percent(100.0), ..default() },
        BackgroundGradient::from(RadialGradient::new(
            UiPosition::TOP_LEFT,
            RadialGradientShape::Circle(Val::Percent(70.0)),
            vec![ColorStop::new(glow(0.30), Val::Percent(0.0)), ColorStop::new(glow(0.0), Val::Percent(100.0))],
        )),
        GlobalZIndex(-10),
        Pickable::IGNORE,
    ));
}

fn apply_orbit(orbit: Res<OrbitCamera>, mut q: Query<&mut Transform, With<MainCamera>>) {
    if orbit.is_changed() {
        for mut t in &mut q {
            *t = orbit.transform();
        }
    }
}

