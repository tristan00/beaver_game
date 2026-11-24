//! Rain and snow falling around the camera's view: a sparse veil of thin streaks or drifting
//! flakes, thicker the harder it comes down, slanted by the wind. Kept light so the valley
//! stays easy to read; the sky's dimming under cloud is in `terrain_render`.
use crate::camera::OrbitCamera;
use crate::state::Sim;
use bevy::light::NotShadowCaster;
use bevy::prelude::*;

pub struct WeatherRenderPlugin;

impl Plugin for WeatherRenderPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_precip).add_systems(Update, fall.after(crate::sim_driver::step_sim));
    }
}

/// How many streaks and flakes there can be at most.
const RAIN: usize = 900;
const SNOW: usize = 700;

/// One streak or flake: which, and its place in the pattern.
#[derive(Component)]
struct Precip {
    snow: bool,
    i: u32,
}

fn spawn_precip(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>, mut materials: ResMut<Assets<StandardMaterial>>) {
    let streak = meshes.add(Cuboid::new(0.025, 1.0, 0.025));
    let flake = meshes.add(Sphere::new(0.5).mesh().ico(1).unwrap());
    let unlit = |c: Color| StandardMaterial { base_color: c, unlit: true, alpha_mode: AlphaMode::Blend, ..default() };
    let rain = materials.add(unlit(Color::srgba(0.8, 0.88, 0.97, 0.45)));
    let snow = materials.add(unlit(Color::srgba(1.0, 1.0, 1.0, 0.85)));
    for (n, is_snow, mesh, mat) in [(RAIN, false, &streak, &rain), (SNOW, true, &flake, &snow)] {
        for i in 0..n {
            commands.spawn((Mesh3d(mesh.clone()), MeshMaterial3d(mat.clone()), Transform::default(), Visibility::Hidden, NotShadowCaster, Precip { snow: is_snow, i: i as u32 }));
        }
    }
}

fn hash(i: u32, salt: u32) -> f32 {
    let mut z = (i as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ (salt as u64).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    ((z ^ (z >> 31)) >> 40) as f32 / (1u64 << 24) as f32
}

/// `x` brought into the span of `width` centred on `centre`, so the pattern stays put in the
/// world as the view moves over it.
fn wrap(x: f32, centre: f32, width: f32) -> f32 {
    centre + (x - centre + width * 0.5).rem_euclid(width) - width * 0.5
}

fn fall(time: Res<Time>, sim: Res<Sim>, orbit: Option<Res<OrbitCamera>>, mut q: Query<(&Precip, &mut Transform, &mut Visibility)>) {
    let Some(orbit) = orbit else { return };
    let s = &sim.0;
    let now = s.water.sky.at(s.time_days);
    let (rain_mm, snow_mm) = (now.rain * 1000.0, now.snow * 1000.0);
    // How full the veil is: a drizzle shows a few streaks, a downpour its full count.
    let amount = |mm: f32| if mm < 0.4 { 0.0 } else { (0.2 + mm / 35.0).min(1.0) };
    let (rain_on, snow_on) = (amount(rain_mm), amount(snow_mm));
    let t = time.elapsed_secs();
    // The box the veil fills: around what the camera looks at, sized to the view.
    let d = orbit.distance;
    let (w, h) = ((d * 1.3).clamp(12.0, 260.0), (d * 0.7).clamp(8.0, 140.0));
    let c = orbit.target;
    let wind = Vec2::from_angle(1.1 + 0.6 * (s.time_days as f32 * 0.37).sin()) * now.wind;
    let scale = (d / 40.0).clamp(0.4, 4.0);
    for (p, mut tr, mut vis) in &mut q {
        let (n, on) = if p.snow { (SNOW, snow_on) } else { (RAIN, rain_on) };
        let shown = (p.i as f32) < on * n as f32;
        let want = if shown { Visibility::Inherited } else { Visibility::Hidden };
        if *vis != want {
            *vis = want;
        }
        if !shown {
            continue;
        }
        let (hx, hz, hy) = (hash(p.i, 1), hash(p.i, 2), hash(p.i, 3));
        if p.snow {
            // Flakes drift down slowly, swaying, blown along by the wind.
            let speed = 1.1 + 0.6 * hash(p.i, 4);
            let fallen = t * speed * scale.sqrt() + hy * h;
            let y = c.y + h * 0.8 - fallen.rem_euclid(h);
            let sway = (t * (0.8 + hash(p.i, 5)) + hx * 6.3).sin() * 0.4 * scale;
            let x = wrap(hx * 600.0 + wind.x * fallen * 1.5 + sway, c.x, w);
            let z = wrap(hz * 600.0 + wind.y * fallen * 1.5, c.z, w);
            *tr = Transform::from_xyz(x, y, z).with_scale(Vec3::splat(0.07 * scale.sqrt()));
        } else {
            // Rain falls fast in straight streaks, slanting with the wind.
            let speed = 11.0 + 4.0 * hash(p.i, 4);
            let fallen = t * speed * scale.sqrt() + hy * h;
            let y = c.y + h * 0.8 - fallen.rem_euclid(h);
            let lean = wind * 0.35;
            let x = wrap(hx * 600.0 + lean.x * fallen, c.x, w);
            let z = wrap(hz * 600.0 + lean.y * fallen, c.z, w);
            let len = 0.5 * scale.sqrt();
            let tilt = Quat::from_rotation_arc(Vec3::Y, Vec3::new(-lean.x, 1.0, -lean.y).normalize());
            *tr = Transform::from_xyz(x, y, z).with_rotation(tilt).with_scale(Vec3::new(scale.sqrt(), len, scale.sqrt()));
        }
    }
}
