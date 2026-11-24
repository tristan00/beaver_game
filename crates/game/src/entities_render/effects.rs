//! Small transitions in the world: a piece easing from its old height to its new one (a gate
//! going up or down, a dam cell built up or worn down, a building stage rising into place), and
//! puffs of dust where building or mending goes on.
use super::meshgen::Mb;
use bevy::prelude::*;

/// Eases this piece's scale from `from` (relative to its own) to its own over `secs`.
#[derive(Component)]
pub struct Grow {
    pub base: Vec3,
    pub from: Vec3,
    pub since: f32,
    pub secs: f32,
}

/// A puff of dust spreading and fading.
#[derive(Component)]
pub struct Puff {
    age: f32,
    life: f32,
    size: f32,
    mat: Handle<StandardMaterial>,
}

#[derive(Resource, Default)]
pub struct PuffMesh(Option<Handle<Mesh>>);

pub fn grow(clock: Res<super::RenderClock>, mut commands: Commands, mut q: Query<(Entity, &Grow, &mut Transform)>) {
    for (e, g, mut tr) in &mut q {
        let k = ((clock.0 - g.since) / g.secs).clamp(0.0, 1.0);
        let k = k * k * (3.0 - 2.0 * k);
        tr.scale = g.base * g.from.lerp(Vec3::ONE, k);
        if k >= 1.0 {
            commands.entity(e).remove::<Grow>();
        }
    }
}

/// Something let go: it drops from `from` to `to` (gathering speed, turning over a little)
/// from `since` for `secs`, then is gone.
#[derive(Component)]
pub struct Fall {
    pub from: Vec3,
    pub to: Vec3,
    pub since: f32,
    pub secs: f32,
}

pub fn falls(clock: Res<super::RenderClock>, mut commands: Commands, mut q: Query<(Entity, &Fall, &mut Transform)>) {
    for (e, f, mut tr) in &mut q {
        let k = ((clock.0 - f.since) / f.secs).clamp(0.0, 1.0);
        if clock.0 - f.since >= f.secs {
            commands.entity(e).despawn();
            continue;
        }
        tr.translation = f.from.lerp(f.to, k * k);
        tr.rotation = Quat::from_rotation_x(0.9 * k);
    }
}

/// A puff of dust at `at`, `size` across.
pub fn puff(commands: &mut Commands, meshes: &mut Assets<Mesh>, materials: &mut Assets<StandardMaterial>, mesh: &mut PuffMesh, at: Vec3, size: f32) {
    let m = mesh.0.get_or_insert_with(|| {
        let mut mb = Mb::new();
        let c = [0.78, 0.7, 0.58];
        for (x, z, r) in [(0.0, 0.0, 0.55), (0.45, 0.2, 0.4), (-0.4, 0.25, 0.42), (0.1, -0.45, 0.38), (-0.25, -0.3, 0.35)] {
            mb.ellipsoid(Vec3::new(x, r * 0.6, z), Vec3::new(r, r * 0.7, r), c);
        }
        meshes.add(mb.build())
    }).clone();
    let mat = materials.add(StandardMaterial { base_color: Color::srgba(0.86, 0.8, 0.7, 0.6), alpha_mode: AlphaMode::Blend, unlit: true, ..default() });
    commands.spawn((Mesh3d(m), MeshMaterial3d(mat.clone()), Transform::from_translation(at).with_scale(Vec3::splat(size * 0.3)), Puff { age: 0.0, life: 1.1, size, mat }));
}

pub fn puffs(time: Res<Time>, mut commands: Commands, mut materials: ResMut<Assets<StandardMaterial>>, mut q: Query<(Entity, &mut Puff, &mut Transform)>) {
    let dt = time.delta_secs();
    for (e, mut p, mut tr) in &mut q {
        p.age += dt;
        let k = (p.age / p.life).min(1.0);
        if k >= 1.0 {
            materials.remove(&p.mat);
            commands.entity(e).despawn();
            continue;
        }
        tr.scale = Vec3::splat(p.size * (0.3 + 0.9 * (1.0 - (1.0 - k) * (1.0 - k))));
        tr.translation.y += dt * 0.4;
        if let Some(mut m) = materials.get_mut(&p.mat) {
            m.base_color = Color::srgba(0.86, 0.8, 0.7, 0.6 * (1.0 - k));
        }
    }
}
