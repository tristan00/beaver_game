use super::meshgen::Mb;
use bevy::prelude::*;

#[derive(Component)]
pub struct Grow {
    pub base: Vec3,
    pub from: Vec3,
    pub since: f32,
    pub secs: f32,
}

#[derive(Component)]
pub struct Puff {
    age: f32,
    life: f32,
    from: Vec3,
    to: Vec3,
    rise: f32,
    color: [f32; 4],
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
    let from = Vec3::splat(size * 0.3);
    commands.spawn((Mesh3d(m), MeshMaterial3d(mat.clone()), Transform::from_translation(at).with_scale(from), Puff { age: 0.0, life: 1.1, from, to: Vec3::splat(size * 1.2), rise: 0.4, color: [0.86, 0.8, 0.7, 0.6], mat }));
}

pub fn splash(commands: &mut Commands, mesh: Handle<Mesh>, materials: &mut Assets<StandardMaterial>, at: Vec3) {
    let mat = materials.add(StandardMaterial { base_color: Color::WHITE, alpha_mode: AlphaMode::Blend, unlit: true, double_sided: true, cull_mode: None, ..default() });
    let from = Vec3::splat(0.3);
    commands.spawn((Mesh3d(mesh), MeshMaterial3d(mat.clone()), Transform::from_translation(at).with_scale(from), Puff { age: 0.0, life: 1.4, from, to: Vec3::new(3.5, 0.6, 3.5), rise: 0.0, color: [1.0, 1.0, 1.0, 1.0], mat }));
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
        tr.scale = p.from.lerp(p.to, 1.0 - (1.0 - k) * (1.0 - k));
        tr.translation.y += dt * p.rise;
        if let Some(mut m) = materials.get_mut(&p.mat) {
            let [r, g, b, a] = p.color;
            m.base_color = Color::srgba(r, g, b, a * (1.0 - k));
        }
    }
}
