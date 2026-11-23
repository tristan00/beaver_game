//! The wolf: its model, a trot, a gallop when it charges, a sniff when it stands, and one
//! pounce for the kill. `BC_WOLF` (see `force_wolf`) summons one for testing.
use super::meshgen::{hex, mix, Mb};
use super::scene::height_at;
use super::sim_view::cell_xz;
use super::Models;
use crate::state::Sim;
use bevy::prelude::*;
use sim::predators::WolfState;

/// World size of one model unit: about one and a half cells nose to tail.
const WOLF_SIZE: f32 = 0.55;
const HEAD_PIVOT: Vec3 = Vec3::new(0.0, 1.32, 0.98);
const TAIL_PIVOT: Vec3 = Vec3::new(0.0, 1.08, -0.92);
const LEG_PIVOTS: [Vec3; 4] = [Vec3::new(-0.17, 0.9, 0.52), Vec3::new(0.17, 0.9, 0.52), Vec3::new(-0.17, 0.92, -0.58), Vec3::new(0.17, 0.92, -0.58)];

pub struct WolfPlugin;

impl Plugin for WolfPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, init_wolf_meshes).add_systems(Update, (force_wolf, sync_wolf, animate_wolf, follow_wolf).chain());
    }
}

#[derive(Resource)]
struct WolfMeshes {
    body: Handle<Mesh>,
    head: Handle<Mesh>,
    tail: Handle<Mesh>,
    leg_front: Handle<Mesh>,
    leg_hind: Handle<Mesh>,
}

#[derive(Component)]
struct WolfRig {
    body: Entity,
    head: Entity,
    tail: Entity,
    legs: [Entity; 4],
    phase: f32,
    speed: f32,
    kill_t: Option<f32>,
}

fn v(x: f32, y: f32, z: f32) -> Vec3 {
    Vec3::new(x, y, z)
}

fn smooth01(a: f32, b: f32, x: f32) -> f32 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Grey fur: a dark saddle on the back, buff sides, a pale belly.
fn fur(n: Vec3, old: [f32; 3]) -> [f32; 3] {
    let (back, side, belly) = (hex(0x4a4741), hex(0x8c8579), hex(0xd8cfbf));
    let c = if n.y > 0.0 { mix(side, back, smooth01(0.15, 0.85, n.y)) } else { mix(side, belly, smooth01(0.05, 0.6, -n.y)) };
    mix(old, c, 0.85)
}

fn body_mesh(mb: &mut Mb) {
    let (d, s) = (hex(0x5a5650), hex(0x8c8579));
    let start = mb.len();
    mb.ellipsoid_g(v(0.0, 0.98, -0.08), v(0.27, 0.29, 0.78), d, s, 14, 9);
    mb.ellipsoid_g(v(0.0, 1.0, 0.42), v(0.31, 0.38, 0.4), d, s, 12, 8);
    mb.ellipsoid_g(v(0.0, 0.98, -0.58), v(0.26, 0.29, 0.34), d, s, 10, 7);
    // A thick ruff round the neck, rising to the head.
    mb.ellipsoid_r(v(0.0, 1.18, 0.74), v(0.24, 0.3, 0.3), Quat::from_rotation_x(-0.6), s);
    mb.paint(start, |_, n, c| fur(n, c));
}

fn head_mesh(mb: &mut Mb) {
    let start = mb.len();
    mb.ellipsoid_g(v(0.0, 0.04, 0.1), v(0.22, 0.2, 0.24), hex(0x5a5650), hex(0x9a9285), 12, 8);
    for s in [-1.0f32, 1.0] {
        // Cheek tufts.
        mb.ellipsoid(v(0.15 * s, -0.04, 0.12), v(0.1, 0.11, 0.12), hex(0xb8ae9e));
    }
    mb.paint(start, |p, n, c| if p.z < 0.28 { fur(n, c) } else { c });
    let pale = hex(0xd8cfbf);
    mb.ellipsoid_r(v(0.0, -0.04, 0.4), v(0.11, 0.1, 0.24), Quat::from_rotation_x(0.08), pale);
    mb.ellipsoid(v(0.0, 0.02, 0.4), v(0.08, 0.06, 0.2), hex(0x7d766b));
    mb.ellipsoid(v(0.0, -0.13, 0.36), v(0.08, 0.04, 0.17), pale);
    mb.ellipsoid(v(0.0, 0.0, 0.63), v(0.055, 0.045, 0.04), hex(0x1c1a18));
    for s in [-1.0f32, 1.0] {
        mb.ellipsoid_r(v(0.12 * s, 0.26, 0.0), v(0.07, 0.15, 0.035), Quat::from_rotation_z(-0.25 * s), hex(0x4a4741));
        mb.ellipsoid_r(v(0.115 * s, 0.24, 0.02), v(0.04, 0.1, 0.02), Quat::from_rotation_z(-0.25 * s), hex(0xc9a990));
        mb.ellipsoid(v(0.1 * s, 0.1, 0.27), v(0.04, 0.03, 0.025), hex(0xd9a441));
        mb.ellipsoid(v(0.1 * s, 0.1, 0.29), v(0.02, 0.02, 0.012), hex(0x1c1a18));
    }
}

fn tail_mesh(mb: &mut Mb) {
    let start = mb.len();
    mb.ellipsoid_r(v(0.0, -0.22, -0.32), v(0.11, 0.13, 0.42), Quat::from_rotation_x(-0.75), hex(0x6f6a62));
    mb.paint(start, |_, n, c| fur(n, c));
    mb.ellipsoid_r(v(0.0, -0.5, -0.56), v(0.07, 0.08, 0.13), Quat::from_rotation_x(-0.75), hex(0x2a2825));
}

fn leg_mesh(mb: &mut Mb, front: bool) {
    let start = mb.len();
    let (d, s) = (hex(0x6f6a62), hex(0xa39b8d));
    if front {
        mb.ellipsoid_g(v(0.0, -0.18, 0.0), v(0.1, 0.24, 0.12), d, s, 8, 5);
    } else {
        mb.ellipsoid_g(v(0.0, -0.18, -0.02), v(0.12, 0.27, 0.17), d, s, 8, 5);
    }
    mb.paint(start, |_, n, c| fur(n, c));
    mb.cyl(v(0.0, -0.3, 0.0), v(0.0, -0.86, 0.02), 0.065, 0.05, s, hex(0xbdb3a3), 6);
    mb.ellipsoid(v(0.0, -0.88, 0.06), v(0.065, 0.04, 0.09), hex(0x3a3733));
}

fn init_wolf_meshes(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>) {
    let mut build = |f: &dyn Fn(&mut Mb)| {
        let mut mb = Mb::new();
        f(&mut mb);
        meshes.add(mb.build())
    };
    commands.insert_resource(WolfMeshes {
        body: build(&body_mesh),
        head: build(&head_mesh),
        tail: build(&tail_mesh),
        leg_front: build(&|m| leg_mesh(m, true)),
        leg_hind: build(&|m| leg_mesh(m, false)),
    });
}

/// `BC_WOLF`: summon the year's wolf as soon as the colony is founded. `near` waits for a
/// beaver out on land (one away from water if there is one) and puts the wolf within sight of
/// it; `near:DAYS` also waits for that day.
fn force_wolf(mut sim: ResMut<Sim>, mut done: Local<bool>) {
    if *done || !sim.0.colony.is_founded() || sim.0.agents.by_id.is_empty() {
        return;
    }
    let Ok(how) = std::env::var("BC_WOLF") else {
        *done = true;
        return;
    };
    let s = &mut sim.0;
    let near = how.starts_with("near");
    let day: f64 = how.split_once(':').and_then(|(_, d)| d.parse().ok()).unwrap_or(0.0);
    if s.time_days < day {
        return;
    }
    let land = |c: (usize, usize)| matches!(s.nav.surface_at(c), Some(sim::nav::Surface::Land));
    let dry_round = |c: (usize, usize)| {
        (-8i32..=8).all(|dy| (-8i32..=8).all(|dx| {
            let q = ((c.0 as i32 + dx).max(0) as usize, (c.1 as i32 + dy).max(0) as usize);
            !matches!(s.nav.surface_at(q), Some(sim::nav::Surface::Water | sim::nav::Surface::Canal))
        }))
    };
    let outdoor: Vec<_> = s.agents.by_id.values().filter(|a| !a.inside && land(a.cell)).map(|a| a.cell).collect();
    let prey = outdoor.iter().copied().find(|&c| dry_round(c)).or(outdoor.first().copied());
    if near && prey.is_none() {
        return;
    }
    *done = true;
    if !sim::predators::summon(s) {
        warn!("BC_WOLF: no edge of the map a wolf could walk in from");
        return;
    }
    if let (true, Some(home)) = (near, prey) {
        let ring = sim::predators::WOLF_SIGHT - 2.0;
        let spot = (0..32).map(|k| k as f32 / 32.0 * std::f32::consts::TAU).find_map(|a| {
            let c = (home.0 as f32 + ring * a.cos(), home.1 as f32 + ring * a.sin());
            let cell = (c.0.max(0.0) as usize, c.1.max(0.0) as usize);
            sim::predators::walkable(s, cell).then_some((cell.0 as f32 + 0.5, cell.1 as f32 + 0.5))
        });
        if let (Some(p), Some(w)) = (spot, s.predators.wolf.as_mut()) {
            w.pos = p;
        }
    }
    info!("BC_WOLF: a wolf is on the map");
}

/// `BC_WOLF_CAM=DIST[,YAW[,PITCH]]`: keep the camera on the wolf (for screenshots).
fn follow_wolf(sim: Res<Sim>, mut orbit: ResMut<crate::camera::OrbitCamera>, rigs: Query<&Transform, With<WolfRig>>, spec: Local<Option<Vec<f32>>>) {
    let mut spec = spec;
    let p = spec.get_or_insert_with(|| std::env::var("BC_WOLF_CAM").map(|v| v.split(',').filter_map(|t| t.trim().parse().ok()).collect()).unwrap_or_default());
    if p.is_empty() || sim.0.predators.wolf.is_none() {
        return;
    }
    if let Ok(tr) = rigs.single() {
        orbit.target = tr.translation;
        orbit.distance = p[0];
        orbit.yaw = p.get(1).copied().unwrap_or(orbit.yaw);
        orbit.pitch = p.get(2).copied().unwrap_or(orbit.pitch);
    }
}

fn ground(sim: &sim::SimState, pos: (f32, f32)) -> Vec3 {
    let xz = cell_xz(sim, pos);
    Vec3::new(xz.x, height_at(&sim.height, xz.x, xz.y), xz.y)
}

fn sync_wolf(
    mut commands: Commands,
    sim: Res<Sim>,
    time: Res<Time>,
    meshes: Option<Res<WolfMeshes>>,
    models: Option<Res<Models>>,
    mut q: Query<(Entity, &mut Transform), With<WolfRig>>,
) {
    let _p = ::sim::prof::scope("game/sync wolf");
    let (Some(wm), Some(m)) = (meshes, models) else { return };
    let Some(wolf) = sim.0.predators.wolf.as_ref() else {
        for (e, _) in &q {
            commands.entity(e).despawn();
        }
        return;
    };
    let target = ground(&sim.0, wolf.pos);
    let want = Quat::from_rotation_y(wolf.heading);
    if let Ok((_, mut tr)) = q.single_mut() {
        let dt = time.delta_secs();
        // The sim moves the wolf once a step; glide between steps.
        let far = tr.translation.xz().distance(target.xz()) > 6.0;
        let k = if far { 1.0 } else { 1.0 - (-dt * 10.0).exp() };
        tr.translation = tr.translation.lerp(target, k);
        tr.translation.y = height_at(&sim.0.height, tr.translation.x, tr.translation.z);
        tr.rotation = tr.rotation.slerp(want, if far { 1.0 } else { 1.0 - (-dt * 9.0).exp() });
        return;
    }
    let w = &m.white;
    let root = commands.spawn((Transform::from_translation(target).with_rotation(want).with_scale(Vec3::splat(WOLF_SIZE)), Visibility::Inherited)).id();
    let mut body = Entity::PLACEHOLDER;
    commands.entity(root).with_children(|p| {
        body = p.spawn((Transform::default(), Visibility::Inherited)).id();
        p.spawn((Mesh3d(m.blob.clone()), MeshMaterial3d(m.blob_mat.clone()), Transform::from_xyz(0.0, 0.05, 0.0).with_scale(Vec3::new(0.55, 1.0, 1.1))));
    });
    let (mut head, mut tail, mut legs) = (Entity::PLACEHOLDER, Entity::PLACEHOLDER, [Entity::PLACEHOLDER; 4]);
    commands.entity(body).with_children(|p| {
        p.spawn((Mesh3d(wm.body.clone()), MeshMaterial3d(w.clone()), Transform::default()));
        head = p.spawn((Mesh3d(wm.head.clone()), MeshMaterial3d(w.clone()), Transform::from_translation(HEAD_PIVOT))).id();
        tail = p.spawn((Mesh3d(wm.tail.clone()), MeshMaterial3d(w.clone()), Transform::from_translation(TAIL_PIVOT))).id();
        for (k, pivot) in LEG_PIVOTS.iter().enumerate() {
            let mesh = if k < 2 { wm.leg_front.clone() } else { wm.leg_hind.clone() };
            legs[k] = p.spawn((Mesh3d(mesh), MeshMaterial3d(w.clone()), Transform::from_translation(*pivot))).id();
        }
    });
    commands.entity(root).insert(WolfRig { body, head, tail, legs, phase: 0.0, speed: 0.0, kill_t: None });
}

fn animate_wolf(time: Res<Time>, sim: Res<Sim>, mut rigs: Query<&mut WolfRig>, mut parts: Query<&mut Transform, Without<WolfRig>>) {
    let Some(wolf) = sim.0.predators.wolf.as_ref() else { return };
    let dt = time.delta_secs();
    let t = time.elapsed_secs();
    for mut rig in &mut rigs {
        // Cells per day → model strides; a charge reads as a gallop.
        let pace = wolf.pace;
        rig.speed += (pace - rig.speed) * (1.0 - (-dt * 6.0).exp());
        let gallop = smooth01(90.0, 160.0, rig.speed);
        let moving = smooth01(3.0, 20.0, rig.speed);
        rig.phase += dt * (5.0 + 9.0 * gallop) * moving.max(0.0001);
        let ph = rig.phase;
        let killing = matches!(wolf.state, WolfState::Kill { .. });
        rig.kill_t = if killing { Some(rig.kill_t.unwrap_or(0.0) + dt) } else { None };

        let mut body = Transform::default();
        let mut head = Transform::from_translation(HEAD_PIVOT);
        let mut tail = Transform::from_translation(TAIL_PIVOT);
        let mut legs: [Transform; 4] = LEG_PIVOTS.map(Transform::from_translation);

        if let Some(k) = rig.kill_t {
            // One pounce: rear up, spring forward and down onto the prey, then shake.
            let rise = smooth01(0.0, 0.25, k) * (1.0 - smooth01(0.25, 0.5, k));
            let land = smooth01(0.3, 0.55, k);
            body.translation = v(0.0, 0.5 * rise - 0.18 * land, 0.9 * land);
            body.rotation = Quat::from_rotation_x(-0.5 * rise + 0.3 * land);
            let shake = if k > 0.55 { (k * 30.0).sin() * 0.35 * (1.0 - smooth01(1.1, 1.5, k)) } else { 0.0 };
            head.rotation = Quat::from_rotation_x(0.55 * land) * Quat::from_rotation_y(shake);
            for (i, l) in legs.iter_mut().enumerate() {
                let front = i < 2;
                l.rotation = Quat::from_rotation_x(if front { -0.9 * rise + 0.5 * land } else { 0.5 * rise - 0.3 * land });
            }
            tail.rotation = Quat::from_rotation_x(0.6 * rise);
        } else {
            let amp = 0.45 + 0.35 * gallop;
            // Trot: diagonal pairs together. Gallop: fore pair then hind pair, a bounding stride.
            let offsets = [0.0, std::f32::consts::PI, std::f32::consts::PI, 0.0];
            let gallop_off = [0.0, 0.35, std::f32::consts::PI, std::f32::consts::PI + 0.35];
            for (i, l) in legs.iter_mut().enumerate() {
                let o = offsets[i] * (1.0 - gallop) + gallop_off[i] * gallop;
                l.rotation = Quat::from_rotation_x((ph + o).sin() * amp * moving);
            }
            let bob = (ph * 2.0).sin().abs() * 0.04 * moving * (1.0 - gallop) + (ph).sin() * 0.08 * gallop;
            body.translation = v(0.0, bob - 0.06 * gallop, 0.0);
            body.rotation = Quat::from_rotation_x(ph.cos() * 0.1 * gallop);
            // Standing, it sniffs the ground; moving, the head is carried low and level; charging, stretched out.
            let sniff = (1.0 - moving) * (0.45 + 0.12 * (t * 1.7).sin());
            head.rotation = Quat::from_rotation_x(sniff + 0.12 * moving - 0.15 * gallop) * Quat::from_rotation_y((1.0 - moving) * 0.3 * (t * 0.6).sin());
            tail.rotation = Quat::from_rotation_x(-0.5 * gallop) * Quat::from_rotation_y((t * 2.3).sin() * 0.15 + (ph).sin() * 0.1 * moving);
        }
        let entities = [rig.body, rig.head, rig.tail, rig.legs[0], rig.legs[1], rig.legs[2], rig.legs[3]];
        let poses = [body, head, tail, legs[0], legs[1], legs[2], legs[3]];
        for (e, p) in entities.into_iter().zip(poses) {
            if let Ok(mut tr) = parts.get_mut(e) {
                *tr = p;
            }
        }
    }
}
