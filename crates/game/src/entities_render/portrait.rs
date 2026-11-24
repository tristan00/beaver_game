//! A beaver's portrait for its details page: the beaver's own model (its fur, build, head and
//! tail) drawn by a small camera of its own into an image the page shows. The model stands far
//! off on a render layer only that camera sees, and the camera runs only while a page is open.
use bevy::camera::visibility::RenderLayers;
use bevy::camera::RenderTarget;
use bevy::prelude::*;
use bevy::render::render_resource::TextureFormat;

use super::{fur_tint, looks_shape, models, Models, Rig};

/// The image the details page shows.
#[derive(Resource)]
pub struct PortraitImage(pub Handle<Image>);

#[derive(Component)]
struct PortraitCam;

/// The model being drawn, and whose.
#[derive(Component)]
struct PortraitModel(u64);

const LAYER: usize = 6;
const SIZE: u32 = 256;
/// Well away from the map, so nothing else is near it.
const AT: Vec3 = Vec3::new(0.0, -4000.0, 0.0);

pub struct PortraitPlugin;

impl Plugin for PortraitPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, setup).add_systems(Update, show);
    }
}

fn setup(mut commands: Commands, mut images: ResMut<Assets<Image>>) {
    let image = images.add(Image::new_target_texture(SIZE, SIZE, TextureFormat::Rgba8Unorm, Some(TextureFormat::Rgba8UnormSrgb)));
    let layer = RenderLayers::layer(LAYER);
    commands.spawn((
        PortraitCam,
        Camera3d::default(),
        Camera { order: -5, is_active: false, clear_color: ClearColorConfig::Custom(Color::srgb(0.16, 0.14, 0.12)), ..default() },
        RenderTarget::Image(image.clone().into()),
        AmbientLight { color: Color::WHITE, brightness: 500.0, ..default() },
        Transform::from_translation(AT + Vec3::new(1.05, 1.05, 1.85)).looking_at(AT + Vec3::new(0.0, 0.62, 0.25), Vec3::Y),
        layer.clone(),
    ));
    commands.spawn((
        DirectionalLight { illuminance: 9000.0, shadow_maps_enabled: false, ..default() },
        Transform::from_translation(AT + Vec3::new(2.0, 3.0, 2.5)).looking_at(AT, Vec3::Y),
        layer,
    ));
    commands.insert_resource(PortraitImage(image));
}

/// Draw the beaver whose page is open, and nothing while none is.
fn show(
    mut commands: Commands,
    hud: Option<Res<crate::hud::HudState>>,
    sim: Res<crate::state::Sim>,
    models: Option<Res<Models>>,
    mut mats: ResMut<Assets<StandardMaterial>>,
    mut cam: Query<&mut Camera, With<PortraitCam>>,
    drawn: Query<(Entity, &PortraitModel)>,
) {
    let want = hud.as_ref().and_then(|h| h.details);
    if let Ok(mut c) = cam.single_mut() {
        if c.is_active != want.is_some() {
            c.is_active = want.is_some();
        }
    }
    let have = drawn.iter().next().map(|(e, m)| (e, m.0));
    if have.map(|h| h.1) == want.map(|w| w.0) {
        return;
    }
    if let Some((e, _)) = have {
        commands.entity(e).despawn();
    }
    let (Some(id), Some(m)) = (want, models) else { return };
    let looks = sim.0.colony.looks_of(id);
    let plain = mats.get(&m.white).cloned().unwrap_or_default();
    let fur = mats.add(StandardMaterial { base_color: fur_tint(&looks), ..plain });
    let (build, head, tail) = looks_shape(&looks);
    let rest = Rig::rest();
    let layer = RenderLayers::layer(LAYER);
    // Turned a little towards the camera, head up.
    let root = commands.spawn((PortraitModel(id.0), Transform::from_translation(AT).with_rotation(Quat::from_rotation_y(0.25)), Visibility::Inherited)).id();
    commands.entity(root).with_children(|p| {
        p.spawn((Transform::from_translation(rest.parts[0].0).with_rotation(rest.parts[0].1), Visibility::Inherited)).with_children(|b| {
            let part = |mesh: &Handle<Mesh>, i: usize, scale: Vec3| {
                (Mesh3d(mesh.clone()), MeshMaterial3d(fur.clone()), Transform::from_translation(rest.parts[i].0).with_rotation(rest.parts[i].1).with_scale(scale), layer.clone())
            };
            b.spawn((Mesh3d(m.body.clone()), MeshMaterial3d(fur.clone()), Transform::from_scale(build), layer.clone()));
            b.spawn(part(&m.head, 1, head)).insert(Transform::from_translation(models::HEAD_PIVOT).with_rotation(Quat::from_rotation_x(-0.12) * Quat::from_rotation_y(0.2)).with_scale(head));
            b.spawn(part(&m.tail, 2, tail));
            for k in 0..4 {
                let mesh = if k < 2 { &m.leg_front } else { &m.leg_hind };
                b.spawn(part(mesh, 3 + k, Vec3::ONE));
            }
        });
    });
}
