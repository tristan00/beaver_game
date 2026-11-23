//! The wolf: one smooth body on two-jointed legs. Its feet step through a walk, a trot and a
//! gallop chosen by its pace, it sniffs and looks about when it stands, and it crouches,
//! springs and shakes for a kill. `BC_WOLF` (see `force_wolf`) summons one for testing, and
//! `BC_GALLERY_WHAT=wolf` lines up every gait (see `gallery.rs`).
use super::meshgen::{hex, mix, Mb};
use super::scene::height_at;
use super::sim_view::cell_xz;
use super::Models;
use crate::sim_driver::BASE_DAYS_PER_SECOND;
use crate::state::Sim;
use bevy::prelude::*;
use sim::predators::WolfState;
use std::f32::consts::{PI, TAU};

/// World size of one model unit: the model is about two and a half units nose to tail.
const WOLF_SIZE: f32 = 0.55;
const NECK_PIVOT: Vec3 = Vec3::new(0.0, 1.24, 0.62);
/// The head's pivot, from the neck's.
const HEAD_ON_NECK: Vec3 = Vec3::new(0.0, 0.34, 0.3);
const TAIL_PIVOT: Vec3 = Vec3::new(0.0, 1.27, -0.84);
/// Shoulders and hips, front pair first.
const LEG_PIVOTS: [Vec3; 4] = [Vec3::new(-0.12, 1.06, 0.46), Vec3::new(0.12, 1.06, 0.46), Vec3::new(-0.12, 1.12, -0.6), Vec3::new(0.12, 1.12, -0.6)];
/// Upper and lower bone lengths: shoulder to elbow and elbow to ground; hip to stifle and
/// stifle to ground.
const FRONT_BONES: (f32, f32) = (0.4, 0.72);
const HIND_BONES: (f32, f32) = (0.46, 0.74);

pub struct WolfPlugin;

impl Plugin for WolfPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<AlarmSlaps>()
            .init_resource::<WolfDrawn>()
            .add_systems(Startup, init_wolf_meshes)
            .add_systems(Update, (force_wolf, sync_wolf, animate_wolf, follow_wolf).chain())
            .add_systems(Update, (tail_slaps, force_lookout, select_wolf));
    }
}

#[derive(Resource)]
pub(super) struct WolfMeshes {
    body: Handle<Mesh>,
    neck: Handle<Mesh>,
    head: Handle<Mesh>,
    tail: Handle<Mesh>,
    upper: [Handle<Mesh>; 2],
    lower: [Handle<Mesh>; 2],
}

#[derive(Component)]
struct WolfRig {
    body: Entity,
    neck: Entity,
    head: Entity,
    tail: Entity,
    upper: [Entity; 4],
    lower: [Entity; 4],
    /// Share of the gait cycle run, and how much the wolf is moving (0 still, 1 on the move).
    phase: f32,
    moving: f32,
    /// Walk / trot / gallop weights, eased.
    gait: Vec3,
    last: Vec3,
    /// Time into the pounce, and how far the pose has blended into it (it eases in from the
    /// charge and back out once the kill is over).
    kill_t: f32,
    kill_w: f32,
}

/// The wolf the sim drives (gallery wolves have a `GalleryWolf` instead).
#[derive(Component)]
struct SimWolf;

/// A gallery wolf: its pace in cells a day, or a pounce on a loop.
#[derive(Component)]
struct GalleryWolf {
    pace: f32,
    kill: bool,
}

fn v(x: f32, y: f32, z: f32) -> Vec3 {
    Vec3::new(x, y, z)
}

fn smooth01(a: f32, b: f32, x: f32) -> f32 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Grizzled grey: a dark saddle along the back, tawny flanks, a cream belly and throat.
fn fur(p: Vec3, n: Vec3) -> [f32; 3] {
    let (saddle, flank, belly) = (hex(0x46433f), hex(0x8e8981), hex(0xd9d3c7));
    let c = if n.y > 0.0 { mix(flank, saddle, smooth01(0.25, 0.9, n.y)) } else { mix(flank, belly, smooth01(0.0, 0.55, -n.y)) };
    // A faint grizzle so the coat isn't flat.
    let g = 1.0 + (p.x * 41.0).sin() * (p.y * 37.0).sin() * (p.z * 29.0).sin() * 0.05;
    [c[0] * g, c[1] * g, c[2] * g]
}

fn furred(mb: &mut Mb, f: impl FnOnce(&mut Mb)) {
    let start = mb.len();
    f(mb);
    mb.paint(start, |p, n, _| fur(p, n));
}

fn body_mesh(mb: &mut Mb) {
    // Tail to chest along +z: lean hips, a tucked waist, a deep, narrow chest.
    let rings = [
        (v(0.0, 1.17, -0.93), 0.02, 0.02),
        (v(0.0, 1.18, -0.88), 0.1, 0.12),
        (v(0.0, 1.19, -0.76), 0.16, 0.19),
        (v(0.0, 1.19, -0.58), 0.18, 0.22),
        (v(0.0, 1.19, -0.38), 0.16, 0.2),
        (v(0.0, 1.16, -0.15), 0.17, 0.25),
        (v(0.0, 1.12, 0.1), 0.19, 0.32),
        (v(0.0, 1.11, 0.32), 0.2, 0.35),
        (v(0.0, 1.15, 0.52), 0.19, 0.31),
        (v(0.0, 1.21, 0.66), 0.15, 0.23),
        (v(0.0, 1.26, 0.72), 0.06, 0.08),
    ];
    furred(mb, |mb| mb.loft(&rings, 16, [0.0; 3]));
}

fn neck_mesh(mb: &mut Mb) {
    let up = HEAD_ON_NECK;
    furred(mb, |mb| {
        // Thick at the shoulders, with a ruff of fur under the throat.
        mb.loft(&[(v(0.0, -0.2, -0.2), 0.04, 0.04), (v(0.0, -0.12, -0.1), 0.15, 0.22), (up * 0.2, 0.16, 0.22), (up * 0.6, 0.14, 0.18), (up * 0.95, 0.12, 0.13), (up * 1.05, 0.04, 0.04)], 14, [0.0; 3]);
    });
    let start = mb.len();
    mb.ellipsoid_r(v(0.0, 0.0, 0.12), v(0.11, 0.16, 0.1), Quat::from_rotation_x(-0.7), [0.0; 3]);
    mb.paint(start, |p, n, _| mix(fur(p, n), hex(0xd9d3c7), 0.35));
}

fn head_mesh(mb: &mut Mb) {
    let (dark, pale, cream) = (hex(0x4b4843), hex(0xa29c92), hex(0xe2d9c8));
    furred(mb, |mb| {
        // A broad skull, then a long, tapering muzzle.
        mb.ellipsoid_g(v(0.0, 0.06, 0.06), v(0.16, 0.15, 0.18), dark, dark, 14, 9);
        mb.loft(&[(v(0.0, 0.05, 0.08), 0.03, 0.03), (v(0.0, 0.04, 0.14), 0.13, 0.12), (v(0.0, 0.02, 0.28), 0.1, 0.095), (v(0.0, -0.01, 0.42), 0.075, 0.075), (v(0.0, -0.02, 0.5), 0.06, 0.062), (v(0.0, -0.02, 0.53), 0.025, 0.025)], 12, dark);
    });
    let start = mb.len();
    for s in [-1.0f32, 1.0] {
        // Cheek ruffs flare the face.
        mb.ellipsoid_r(v(0.1 * s, -0.04, 0.06), v(0.08, 0.1, 0.11), Quat::from_rotation_z(0.3 * s), pale);
    }
    // A pale lower jaw.
    mb.ellipsoid_r(v(0.0, -0.07, 0.3), v(0.065, 0.04, 0.2), Quat::from_rotation_x(-0.06), cream);
    mb.paint(start, |p, n, c| if p.y < -0.06 { cream } else { mix(c, fur(p, n), 0.6) });
    mb.ellipsoid(v(0.0, -0.005, 0.53), v(0.045, 0.035, 0.03), hex(0x1a1816));
    for s in [-1.0f32, 1.0] {
        // Tall, pointed ears set wide.
        let tilt = Quat::from_rotation_z(-0.35 * s) * Quat::from_rotation_x(0.15);
        let ear = Vec3::new(0.09 * s, 0.2, 0.0);
        mb.cyl(ear, ear + tilt * Vec3::new(0.0, 0.14, 0.0), 0.075, 0.008, dark, dark, 4);
        mb.cyl(ear + tilt * Vec3::new(0.0, 0.0, 0.015), ear + tilt * Vec3::new(0.0, 0.11, 0.015), 0.05, 0.005, hex(0xc9b7a2), hex(0xc9b7a2), 4);
        // Amber eyes under a dark brow.
        mb.ellipsoid(v(0.075 * s, 0.09, 0.2), v(0.035, 0.024, 0.02), hex(0xd49a32));
        mb.ellipsoid(v(0.078 * s, 0.09, 0.215), v(0.016, 0.016, 0.01), hex(0x1a1816));
        mb.ellipsoid_r(v(0.075 * s, 0.12, 0.2), v(0.045, 0.012, 0.025), Quat::from_rotation_z(0.25 * s), dark);
    }
}

fn tail_mesh(mb: &mut Mb) {
    // A bushy brush hanging to the hocks, black at the tip.
    let rings = [
        (v(0.0, 0.03, 0.05), 0.03, 0.03),
        (v(0.0, 0.0, -0.02), 0.07, 0.07),
        (v(0.0, -0.12, -0.12), 0.1, 0.1),
        (v(0.0, -0.3, -0.2), 0.12, 0.12),
        (v(0.0, -0.5, -0.24), 0.11, 0.11),
        (v(0.0, -0.66, -0.25), 0.07, 0.07),
        (v(0.0, -0.74, -0.25), 0.02, 0.02),
    ];
    let start = mb.len();
    mb.loft(&rings, 12, [0.0; 3]);
    mb.paint(start, |p, n, _| mix(fur(p, n), hex(0x1f1d1a), smooth01(-0.5, -0.62, p.y)));
}

/// An upper leg, hanging from its pivot to the elbow or stifle at (0, -len, 0).
fn upper_mesh(mb: &mut Mb, front: bool) {
    let len = if front { FRONT_BONES.0 } else { HIND_BONES.0 };
    furred(mb, |mb| {
        if front {
            mb.loft(&[(v(0.0, 0.14, 0.02), 0.02, 0.02), (v(0.0, 0.08, 0.02), 0.1, 0.13), (v(0.0, -0.15, 0.0), 0.09, 0.11), (v(0.0, -len, -0.01), 0.065, 0.065), (v(0.0, -len - 0.05, -0.01), 0.02, 0.02)], 10, [0.0; 3]);
        } else {
            // A heavy thigh.
            mb.loft(&[(v(0.0, 0.16, -0.02), 0.02, 0.02), (v(0.0, 0.08, -0.03), 0.12, 0.17), (v(0.0, -0.14, 0.0), 0.11, 0.15), (v(0.0, -len, 0.02), 0.06, 0.07), (v(0.0, -len - 0.05, 0.02), 0.02, 0.02)], 10, [0.0; 3]);
        }
    });
}

/// A lower leg, from the elbow or stifle down to the paw, whose sole is at (0, -len, 0). The
/// hind one bends back to the hock on the way.
fn lower_mesh(mb: &mut Mb, front: bool) {
    let len = if front { FRONT_BONES.1 } else { HIND_BONES.1 };
    let leg = hex(0xb3ab9e);
    let start = mb.len();
    if front {
        mb.loft(&[(v(0.0, 0.05, 0.0), 0.02, 0.02), (v(0.0, 0.0, 0.0), 0.075, 0.08), (v(0.0, -0.22, 0.0), 0.06, 0.065), (v(0.0, -len + 0.14, 0.0), 0.042, 0.048), (v(0.0, -len + 0.06, 0.02), 0.042, 0.046), (v(0.0, -len + 0.03, 0.03), 0.02, 0.02)], 8, leg);
    } else {
        let hock = v(0.0, -0.36, -0.15);
        mb.loft(&[(v(0.0, 0.05, 0.01), 0.02, 0.02), (v(0.0, 0.0, 0.0), 0.08, 0.1), (hock * 0.5, 0.06, 0.075), (hock, 0.045, 0.055), (v(0.0, -len + 0.08, -0.02), 0.04, 0.045), (v(0.0, -len + 0.03, 0.0), 0.02, 0.02)], 8, leg);
    }
    mb.paint(start, |p, n, c| mix(fur(p, n), c, smooth01(-0.05, -0.3, p.y)));
    // The paw.
    mb.ellipsoid(v(0.0, -len + 0.035, 0.05), v(0.058, 0.038, 0.085), hex(0x86807a));
    mb.ellipsoid(v(0.0, -len + 0.012, 0.06), v(0.05, 0.012, 0.07), hex(0x2c2926));
}

fn init_wolf_meshes(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>) {
    let mut build = |f: &dyn Fn(&mut Mb)| {
        let mut mb = Mb::new();
        f(&mut mb);
        meshes.add(mb.build())
    };
    commands.insert_resource(WolfMeshes {
        body: build(&body_mesh),
        neck: build(&neck_mesh),
        head: build(&head_mesh),
        tail: build(&tail_mesh),
        upper: [build(&|m| upper_mesh(m, true)), build(&|m| upper_mesh(m, false))],
        lower: [build(&|m| lower_mesh(m, true)), build(&|m| lower_mesh(m, false))],
    });
}

/// Where the wolf is drawn this frame, for anything shown over it (its alert pin).
#[derive(Resource, Default)]
pub struct WolfDrawn(pub Option<Vec3>);

/// Tail slaps so far, for the sound: their count, and how loud the last one is from where the
/// camera looks.
#[derive(Resource, Default)]
pub struct AlarmSlaps {
    pub count: u32,
    pub amp: f32,
}

/// A splash ring spreading from a tail slap.
#[derive(Component)]
struct Splash {
    age: f32,
    mat: Handle<StandardMaterial>,
}

const SLAP_EVERY: f32 = 2.5;
const SPLASH_SECS: f32 = 1.4;

/// While a lookout sees the wolf, its guard slaps its tail on the water beside it every
/// `SLAP_EVERY` seconds: a ring of spray spreads and fades there, and a slap sounds.
fn tail_slaps(
    mut commands: Commands,
    time: Res<Time>,
    sim: Res<Sim>,
    orbit: Res<crate::camera::OrbitCamera>,
    mut slaps: ResMut<AlarmSlaps>,
    render: Res<super::RenderClock>,
    mut slap_times: ResMut<super::TailSlapTimes>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut ring: Local<Option<Handle<Mesh>>>,
    mut next: Local<std::collections::HashMap<u64, f32>>,
    mut splashes: Query<(Entity, &mut Splash, &mut Transform)>,
) {
    let dt = time.delta_secs();
    let now = time.elapsed_secs();
    for (e, mut sp, mut tr) in &mut splashes {
        sp.age += dt;
        let k = (sp.age / SPLASH_SECS).min(1.0);
        if k >= 1.0 {
            commands.entity(e).despawn();
            materials.remove(&sp.mat);
            continue;
        }
        tr.scale = Vec3::splat(0.3 + 3.2 * (1.0 - (1.0 - k) * (1.0 - k)));
        if let Some(mut m) = materials.get_mut(&sp.mat) {
            m.base_color = Color::srgba(1.0, 1.0, 1.0, (1.0 - k) * (1.0 - k).sqrt());
        }
    }
    let s = &sim.0;
    next.retain(|id, _| s.predators.alarms.iter().any(|b| b.0 == *id));
    for &id in &s.predators.alarms {
        let due = next.entry(id.0).or_insert(now);
        if now < *due {
            continue;
        }
        *due = now + SLAP_EVERY;
        let Some(at) = slap_spot(s, id) else { continue };
        if let Some(guard) = s.work.place(id).and_then(|p| p.slots.iter().find_map(|sl| sl.worker)) {
            slap_times.0.insert(guard.0, render.0);
        }
        let mesh = ring.get_or_insert_with(|| {
            let mut mb = Mb::new();
            mb.lathe(Mat4::IDENTITY, &[(1.0, 0.0), (0.72, 0.0)], 48, [1.0; 3], [1.0; 3], false);
            mb.lathe(Mat4::IDENTITY, &[(0.55, 0.0), (0.35, 0.0)], 32, [1.0; 3], [1.0; 3], false);
            // Spray thrown up in the middle.
            mb.ellipsoid(Vec3::new(0.0, 0.15, 0.0), Vec3::new(0.2, 0.45, 0.2), [1.0; 3]);
            meshes.add(mb.build())
        });
        let mat = materials.add(StandardMaterial { base_color: Color::WHITE, emissive: LinearRgba::rgb(1.2, 1.25, 1.25), alpha_mode: AlphaMode::Blend, unlit: true, double_sided: true, cull_mode: None, ..default() });
        commands.spawn((Mesh3d(mesh.clone()), MeshMaterial3d(mat.clone()), Transform::from_translation(at + Vec3::Y * 0.25).with_scale(Vec3::splat(0.3)), Splash { age: 0.0, mat }));
        slaps.count = slaps.count.wrapping_add(1);
        slaps.amp = (1.3 - orbit.target.distance(at) / 90.0).clamp(0.25, 1.2);
    }
}

/// Where a lookout's guard slaps: on the water nearest the guard (or the lookout), at the
/// water's surface.
fn slap_spot(s: &sim::SimState, id: sim::buildings::BuildingId) -> Option<Vec3> {
    let b = s.buildings.building(id)?;
    let guard = s.work.place(id)?.slots.iter().find_map(|sl| sl.worker).and_then(|w| s.agents.get(w));
    let from = guard.map_or(b.pos, |a| a.cell);
    let wet = |c: (usize, usize)| s.water.depth.get(c.0, c.1).is_some_and(|d| *d > 0.05);
    let mut best: Option<((usize, usize), usize)> = None;
    for dy in -5i32..=5 {
        for dx in -5i32..=5 {
            let c = ((from.0 as i32 + dx).max(0) as usize, (from.1 as i32 + dy).max(0) as usize);
            let d = (dx * dx + dy * dy) as usize;
            if wet(c) && best.is_none_or(|(_, bd)| d < bd) {
                best = Some((c, d));
            }
        }
    }
    let (c, _) = best?;
    let p = (c.0 as f32 + 0.5, c.1 as f32 + 0.5);
    let xz = cell_xz(s, p);
    let depth = s.water.depth.get(c.0, c.1).copied().unwrap_or(0.0);
    Some(Vec3::new(xz.x, height_at(&s.height, xz.x, xz.y) + depth, xz.y))
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

/// `BC_LOOKOUT=DIST[,PITCH[,YAW]]`: once the colony is founded, put up a finished shelter and lookout by the
/// first beaver, seat a guard, aim the camera there and move any wolf into the lookout's sight
/// (for screenshots, with `BC_WOLF`).
fn force_lookout(mut sim: ResMut<Sim>, mut orbit: ResMut<crate::camera::OrbitCamera>, mut stage: Local<u8>) {
    use sim::buildings::BuildingKind;
    if *stage >= 3 || std::env::var_os("BC_LOOKOUT").is_none() || !sim.0.colony.is_founded() {
        return;
    }
    let s = &mut sim.0;
    let Some(near) = s.agents.by_id.values().next().map(|a| a.cell) else { return };
    let build = |s: &mut sim::SimState, kind: BuildingKind| -> Option<sim::buildings::BuildingId> {
        let pos = (3..40usize).flat_map(|r| (0..=2 * r).flat_map(move |i| [(near.0 + i - r, near.1 - r), (near.0 + i - r, near.1 + r), (near.0 - r, near.1 + i - r), (near.0 + r, near.1 + i - r)])).find(|&p| s.check_site(kind, p).is_ok())?;
        let id = s.place_site(kind, pos).ok()?;
        for &(g, a) in kind.spec().cost {
            let _ = s.work.deliver(id, g, a);
        }
        let _ = s.work.add_labour(id, kind.spec().work);
        Some(id)
    };
    match *stage {
        0 => {
            if !s.has_building(BuildingKind::Shelter) {
                build(s, BuildingKind::Shelter);
            }
            *stage = 1;
        }
        1 if s.has_building(BuildingKind::Shelter) => {
            if build(s, BuildingKind::Lookout).is_some() {
                *stage = 2;
            } else {
                warn!("BC_LOOKOUT: no site for a lookout");
                *stage = 3;
            }
        }
        2 => {
            let Some(b) = s.buildings.buildings.iter().find(|b| b.kind == BuildingKind::Lookout && b.is_complete()).map(|b| (b.id, sim::predators::lookout_centre(b))) else { return };
            let _ = s.apply(sim::Command::AddWorker { building: b.0 });
            let xz = cell_xz(s, b.1);
            orbit.target = Vec3::new(xz.x, height_at(&s.height, xz.x, xz.y), xz.y);
            let cam: Vec<f32> = std::env::var("BC_LOOKOUT").unwrap_or_default().split(',').filter_map(|t| t.trim().parse().ok()).collect();
            orbit.distance = cam.first().copied().unwrap_or(90.0);
            if let Some(&p) = cam.get(1) {
                orbit.pitch = p;
            }
            if let Some(&y) = cam.get(2) {
                orbit.yaw = y;
            }
            info!("BC_LOOKOUT: a manned lookout at {:?}", b.1);
            *stage = 3;
            // A wolf about is moved into the lookout's sight, beyond a beaver's.
            let ring = sim::predators::LOOKOUT_SIGHT - 8.0;
            let spot = (0..64).map(|k| k as f32 / 64.0 * std::f32::consts::TAU).find_map(|a| {
                let c = (b.1 .0 + ring * a.cos(), b.1 .1 + ring * a.sin());
                let cell = (c.0.max(0.0) as usize, c.1.max(0.0) as usize);
                sim::predators::walkable(s, cell).then_some((cell.0 as f32 + 0.5, cell.1 as f32 + 0.5))
            });
            if let (Some(p), Some(w)) = (spot, s.predators.wolf.as_mut()) {
                w.pos = p;
            }
        }
        _ => {}
    }
}

/// `BC_SELECT_WOLF=1`: keep the wolf selected while there is one (for screenshots of its sight).
fn select_wolf(sim: Res<Sim>, sel: Option<ResMut<crate::flow::Selection>>) {
    if std::env::var_os("BC_SELECT_WOLF").is_none() || sim.0.predators.wolf.is_none() {
        return;
    }
    if let Some(mut sel) = sel {
        if *sel != crate::flow::Selection::Wolf {
            *sel = crate::flow::Selection::Wolf;
        }
    }
}

/// `BC_WOLF_CAM=DIST[,YAW[,PITCH]]`: keep the camera on the wolf (for screenshots).
fn follow_wolf(sim: Res<Sim>, mut orbit: ResMut<crate::camera::OrbitCamera>, rigs: Query<&Transform, With<SimWolf>>, spec: Local<Option<Vec<f32>>>) {
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

/// Where the sim has put the wolf, step by step, so it can be replayed smoothly at the
/// presentation time like the beavers (the game only sees a new world a few times a second).
#[derive(Component, Default)]
pub(super) struct WolfTrack {
    samples: std::collections::VecDeque<(f64, Vec2)>,
    instance: u64,
    /// Where it is drawn: eased toward the replayed position, so the replay's small changes
    /// of pace between sim updates don't read as surges.
    shown: Option<Vec2>,
}

impl WolfTrack {
    pub(super) fn record(&mut self, instance: u64, t: f64, p: Vec2) {
        self.record_with(instance, t, p, 8.0);
    }

    /// `record`, starting afresh after a jump of more than `jump` (a fast runner, or a bird in
    /// flight, can cover more than the wolf between two updates).
    pub(super) fn record_with(&mut self, instance: u64, t: f64, p: Vec2, jump: f32) {
        if self.instance != instance {
            self.samples.clear();
            self.instance = instance;
        }
        match self.samples.back() {
            Some(&(last, _)) if last >= t => return,
            Some(&(_, q)) if q.distance(p) > jump => self.samples.clear(),
            _ => {}
        }
        self.samples.push_back((t, p));
        while self.samples.len() > 64 {
            self.samples.pop_front();
        }
    }

    /// The position at `t` along straight lines between samples, and the way it is going.
    pub(super) fn at(&self, t: f64) -> Option<(Vec2, Option<Vec2>)> {
        let &(t0, first) = self.samples.front()?;
        if t <= t0 {
            return Some((first, None));
        }
        let Some(k) = self.samples.iter().position(|s| s.0 > t) else {
            return Some((self.samples.back()?.1, None));
        };
        let (ta, a) = self.samples[k - 1];
        let (tb, b) = self.samples[k];
        let f = ((t - ta) / (tb - ta)).clamp(0.0, 1.0) as f32;
        let d = b - a;
        Some((a.lerp(b, f), (d.length() > 1e-4).then(|| d.normalize())))
    }
}

fn sync_wolf(
    mut commands: Commands,
    sim: Res<Sim>,
    time: Res<Time>,
    clock: Option<Res<crate::sim_driver::SimClock>>,
    meshes: Option<Res<WolfMeshes>>,
    models: Option<Res<Models>>,
    mut drawn: ResMut<WolfDrawn>,
    mut q: Query<(Entity, &mut Transform, &mut WolfTrack), (With<WolfRig>, With<SimWolf>)>,
) {
    let _p = ::sim::prof::scope("game/sync wolf");
    let (Some(wm), Some(m)) = (meshes, models) else { return };
    let s = &sim.0;
    let Some(wolf) = s.predators.wolf.as_ref() else {
        drawn.0 = None;
        for (e, _, _) in &q {
            commands.entity(e).despawn();
        }
        return;
    };
    let here = cell_xz(s, wolf.pos);
    if let Ok((_, mut tr, mut track)) = q.single_mut() {
        track.record(s.instance, s.time_days, here);
        let now = clock.as_ref().map_or(s.time_days, |c| c.render_days(s.time_days));
        let (aim, going) = track.at(now).unwrap_or((here, None));
        let k = 1.0 - (-time.delta_secs() * 10.0).exp();
        let xz = match track.shown {
            Some(p) if p.distance(aim) < 4.0 => p.lerp(aim, k),
            _ => aim,
        };
        track.shown = Some(xz);
        tr.translation = Vec3::new(xz.x, height_at(&s.height, xz.x, xz.y), xz.y);
        drawn.0 = Some(tr.translation);
        // Face the way it is going; standing, the way the sim last turned it (at its prey).
        let want = Quat::from_rotation_y(going.map_or(wolf.heading, |d| d.x.atan2(d.y)));
        tr.rotation = tr.rotation.slerp(want, 1.0 - (-time.delta_secs() * 8.0).exp());
        return;
    }
    let root = spawn_rig(&mut commands, &wm, &m, Transform::from_translation(ground(s, wolf.pos)).with_rotation(Quat::from_rotation_y(wolf.heading)));
    let mut track = WolfTrack::default();
    track.record(s.instance, s.time_days, here);
    commands.entity(root).insert((SimWolf, track));
}

fn spawn_rig(commands: &mut Commands, wm: &WolfMeshes, m: &Models, at: Transform) -> Entity {
    let w = &m.white;
    let mesh = |h: &Handle<Mesh>| (Mesh3d(h.clone()), MeshMaterial3d(w.clone()));
    let root = commands.spawn((at.with_scale(Vec3::splat(WOLF_SIZE)), Visibility::Inherited)).id();
    let body = commands.spawn((Transform::default(), Visibility::Inherited, ChildOf(root))).id();
    commands.spawn((Mesh3d(m.blob.clone()), MeshMaterial3d(m.blob_mat.clone()), Transform::from_xyz(0.0, 0.05, 0.0).with_scale(Vec3::new(0.5, 1.0, 1.25)), ChildOf(root)));
    commands.spawn((mesh(&wm.body), Transform::default(), ChildOf(body)));
    let neck = commands.spawn((mesh(&wm.neck), Transform::from_translation(NECK_PIVOT), ChildOf(body))).id();
    let head = commands.spawn((mesh(&wm.head), Transform::from_translation(HEAD_ON_NECK), ChildOf(neck))).id();
    let tail = commands.spawn((mesh(&wm.tail), Transform::from_translation(TAIL_PIVOT), ChildOf(body))).id();
    let (mut upper, mut lower) = ([Entity::PLACEHOLDER; 4], [Entity::PLACEHOLDER; 4]);
    for (k, pivot) in LEG_PIVOTS.iter().enumerate() {
        let (f, len) = if k < 2 { (0, FRONT_BONES.0) } else { (1, HIND_BONES.0) };
        upper[k] = commands.spawn((mesh(&wm.upper[f]), Transform::from_translation(*pivot), ChildOf(body))).id();
        lower[k] = commands.spawn((mesh(&wm.lower[f]), Transform::from_xyz(0.0, -len, 0.0), ChildOf(upper[k]))).id();
    }
    commands.entity(root).insert(WolfRig { body, neck, head, tail, upper, lower, phase: 0.0, moving: 0.0, gait: Vec3::Y, last: at.translation, kill_t: 0.0, kill_w: 0.0 });
    root
}

/// `BC_GALLERY_WHAT=wolf`: a wolf standing, walking, trotting, galloping and pouncing, in a row
/// across `centre`, side on to a camera at yaw 0. `BC_GALLERY_POSE=n` shows one of them.
pub(super) fn spawn_gallery_wolves(commands: &mut Commands, wm: &WolfMeshes, m: &Models, centre: Vec3, ground: impl Fn(f32, f32) -> f32) {
    let yaw: f32 = std::env::var("BC_GALLERY_YAW").ok().and_then(|v| v.parse().ok()).unwrap_or(std::f32::consts::FRAC_PI_2);
    let only: Option<usize> = std::env::var("BC_GALLERY_POSE").ok().and_then(|v| v.parse().ok());
    for (i, (pace, kill)) in [(0.0, false), (25.0, false), (60.0, false), (200.0, false), (0.0, true)].into_iter().enumerate() {
        if only.is_some_and(|o| o != i) {
            continue;
        }
        let x = if only.is_some() { centre.x } else { centre.x + (i as f32 - 2.0) * 2.2 };
        let at = Vec3::new(x, ground(x, centre.z), centre.z);
        let root = spawn_rig(commands, wm, m, Transform::from_translation(at).with_rotation(Quat::from_rotation_y(yaw)));
        commands.entity(root).insert(GalleryWolf { pace, kill });
    }
}

/// Two-bone leg: the angles (about x; 0 hangs straight down, positive swings the foot back) of
/// the upper bone and of the lower bone relative to it that put the foot at `foot` from a
/// pivot at `hip`, both (y, z). `knee_back` bends the joint backwards, like an elbow, rather
/// than forwards, like a stifle.
pub(super) fn leg_ik(hip: Vec2, foot: Vec2, (a, b): (f32, f32), knee_back: bool) -> (f32, f32) {
    let d = foot - hip;
    let dist = d.length().clamp((a - b).abs() + 1e-3, a + b - 1e-3);
    let reach = (-d.y).atan2(-d.x);
    let at_hip = ((a * a + dist * dist - b * b) / (2.0 * a * dist)).clamp(-1.0, 1.0).acos();
    let at_knee = ((a * a + b * b - dist * dist) / (2.0 * a * b)).clamp(-1.0, 1.0).acos();
    let s = if knee_back { 1.0 } else { -1.0 };
    (reach + s * at_hip, -s * (PI - at_knee))
}

fn angle_x(a: f32) -> Quat {
    Quat::from_rotation_x(a)
}

/// Every part's pose, worked out before it is applied.
#[derive(Clone, Copy)]
struct Pose {
    body: Transform,
    neck: Quat,
    head: Quat,
    tail: Quat,
    /// Foot targets (y, z) in the root's frame, ground at y = 0.
    feet: [Vec2; 4],
}

impl Pose {
    fn lerp(self, o: Pose, k: f32) -> Pose {
        let body = Transform::from_translation(self.body.translation.lerp(o.body.translation, k)).with_rotation(self.body.rotation.slerp(o.body.rotation, k));
        let feet = std::array::from_fn(|i| self.feet[i].lerp(o.feet[i], k));
        Pose { body, neck: self.neck.slerp(o.neck, k), head: self.head.slerp(o.head, k), tail: self.tail.slerp(o.tail, k), feet }
    }
}

fn rest_feet() -> [Vec2; 4] {
    LEG_PIVOTS.map(|p| Vec2::new(0.0, p.z + if p.z > 0.0 { 0.04 } else { -0.02 }))
}

/// Walk (lateral sequence), trot (diagonal pairs) and rotary gallop: each leg's offset in the
/// cycle, the share of the cycle a foot is down, cycles a second, and how high a foot lifts.
const GAITS: [([f32; 4], f32, f32, f32); 3] = [([0.25, 0.75, 0.0, 0.5], 0.62, 1.2, 0.12), ([0.0, 0.5, 0.5, 0.0], 0.42, 1.9, 0.17), ([0.55, 0.65, 0.0, 0.1], 0.3, 2.6, 0.24)];

fn gait_mix(w: Vec3, f: impl Fn(&([f32; 4], f32, f32, f32)) -> f32) -> f32 {
    f(&GAITS[0]) * w.x + f(&GAITS[1]) * w.y + f(&GAITS[2]) * w.z
}

fn gait_pose(rig: &WolfRig, speed: f32, t: f32) -> Pose {
    let (w, m) = (rig.gait, rig.moving);
    let duty = gait_mix(w, |g| g.1);
    let lift = gait_mix(w, |g| g.3) * m;
    // How far a foot travels while it is down: the body covers that in `duty` of a cycle.
    let stride = (speed * duty / gait_mix(w, |g| g.2)).min(1.15) * m;
    let ph = rig.phase;
    let mut feet = rest_feet();
    for (i, f) in feet.iter_mut().enumerate() {
        let p = (ph + gait_mix(w, |g| g.0[i])).rem_euclid(1.0);
        let reach = if i < 2 { 0.08 } else { -0.04 } * w.z;
        if p < duty {
            f.y += reach + stride * (0.5 - p / duty);
        } else {
            let s = (p - duty) / (1.0 - duty);
            f.y += reach + stride * (smooth01(0.0, 1.0, s) - 0.5);
            f.x = lift * (PI * s).sin().powf(0.8) * if i < 2 { 1.1 } else { 1.0 };
        }
    }
    let c = ph * TAU;
    // Walk and trot: a small bounce twice a stride. Gallop: one big rock, nose down as the
    // fore feet land, crouched lower.
    let bob = (-0.02 * w.x - 0.03 * w.y) * (2.0 * c).cos() + 0.06 * w.z * (c + 0.6).sin() - 0.05 * w.z;
    let pitch = 0.11 * w.z * (c - 0.9).sin() + 0.02 * w.x * (2.0 * c).sin();
    let idle = 1.0 - m;
    let breathe = idle * 0.006 * (t * 2.4).sin();
    let body = Transform::from_xyz(0.0, bob * m + breathe, 0.0).with_rotation(angle_x(pitch * m));
    // Standing, it looks about and now and then puts its nose down to sniff. Moving, the neck
    // is carried low and level, stretched right out at a gallop.
    let sniff = smooth01(0.55, 0.75, (t * 0.21).sin() * 0.5 + 0.5);
    let look = (t * 0.37).sin() * 0.55 + (t * 0.83).sin() * 0.15;
    let neck = Quat::from_rotation_y(idle * look * (1.0 - sniff)) * angle_x(idle * (-0.1 + 0.85 * sniff) + m * (0.25 * w.x + 0.35 * w.y + 0.6 * w.z + 0.05 * w.x * (2.0 * c).sin()));
    let head = angle_x(idle * (0.1 + 0.35 * sniff + 0.04 * (t * 7.0).sin() * sniff) + m * (0.05 * w.y - 0.25 * w.z));
    // The tail hangs at rest, lifts a little on the move and streams out behind at a gallop.
    let tail = angle_x(0.15 + m * (0.15 * w.x + 0.3 * w.y + 0.95 * w.z + 0.05 * (2.0 * c).sin())) * Quat::from_rotation_z((t * 1.3).sin() * 0.1 * idle + c.sin() * 0.12 * m);
    Pose { body, neck, head, tail, feet }
}

/// One pounce over about a second and a half (the sim's kill): crouch, spring, land on the
/// prey jaws first, shake it, then stand over it.
fn kill_pose(k: f32) -> Pose {
    let crouch = smooth01(0.0, 0.2, k) * (1.0 - smooth01(0.2, 0.32, k));
    let leap = smooth01(0.2, 0.5, k);
    let air = (PI * smooth01(0.22, 0.5, k)).sin();
    let landed = smooth01(0.45, 0.6, k);
    let shake = if k > 0.6 { (k * 26.0).sin() * 0.4 * (1.0 - smooth01(1.1, 1.35, k)) } else { 0.0 };
    let reach = 0.85;
    let body = Transform::from_xyz(0.0, -0.16 * crouch + 0.22 * air - 0.1 * landed, reach * leap).with_rotation(angle_x(0.1 * crouch - 0.25 * air + 0.22 * landed));
    let mut feet = rest_feet();
    for (i, f) in feet.iter_mut().enumerate() {
        // Fore paws reach out and come down on the prey; hind paws drive off, then follow.
        let (s, ahead, high) = if i < 2 { (smooth01(0.2, 0.5, k), reach + 0.1, 0.3) } else { (smooth01(0.3, 0.62, k), reach, 0.2) };
        f.y += ahead * s;
        f.x = high * (PI * s).sin();
    }
    let neck = Quat::from_rotation_y(shake * 0.5) * angle_x(0.15 * crouch - 0.2 * air + 0.75 * landed);
    let head = Quat::from_rotation_y(shake) * Quat::from_rotation_z(shake * 0.4) * angle_x(0.2 * crouch + 0.45 * landed);
    let tail = angle_x(0.2 + 0.9 * air + 0.35 * landed) * Quat::from_rotation_z(shake * 0.3);
    Pose { body, neck, head, tail, feet }
}

fn animate_wolf(
    time: Res<Time>,
    sim: Res<Sim>,
    clock: Option<Res<crate::sim_driver::SimClock>>,
    mut rigs: Query<(&mut WolfRig, &Transform, Option<&GalleryWolf>)>,
    mut parts: Query<&mut Transform, Without<WolfRig>>,
) {
    let wolf = sim.0.predators.wolf.as_ref();
    let dt = time.delta_secs().max(1e-4);
    let t = time.elapsed_secs();
    let paused = clock.is_some_and(|c| c.paused);
    for (mut rig, root, gallery) in &mut rigs {
        let (pace, killing) = match (gallery, wolf) {
            (Some(g), _) => (g.pace, g.kill),
            (None, Some(w)) => (w.pace, matches!(w.state, WolfState::Kill { .. } | WolfState::Feed { .. })),
            (None, None) => continue,
        };
        // The gait follows the pace the sim asks for, at the game's base speed, in model units
        // a second. A sim wolf that isn't getting anywhere (paused, or stuck) stands.
        let speed = pace * BASE_DAYS_PER_SECOND / WOLF_SIZE;
        let moved = root.translation.xz().distance(rig.last.xz()) / dt;
        rig.last = root.translation;
        let going = speed > 0.1 && (gallery.is_some() || (!paused && moved > 0.05));
        let ease = 1.0 - (-dt * 5.0).exp();
        rig.moving += (if going { 1.0 } else { 0.0 } - rig.moving) * ease;
        let (walk, gallop) = (1.0 - smooth01(1.2, 2.4, speed), smooth01(6.0, 8.5, speed));
        rig.gait = rig.gait.lerp(Vec3::new(walk, 1.0 - walk - gallop, gallop), ease);
        let freq = gait_mix(rig.gait, |g| g.2);
        rig.phase = (rig.phase + dt * freq * rig.moving).rem_euclid(1.0);
        if killing {
            // A fresh pounce starts from the top; the gallery's repeats every 2.2 s.
            rig.kill_t = if rig.kill_w < 0.01 { 0.0 } else { rig.kill_t + dt };
            if gallery.is_some() {
                rig.kill_t %= 2.2;
            }
        }
        rig.kill_w += (if killing { 1.0 } else { 0.0 } - rig.kill_w) * (1.0 - (-dt * if killing { 14.0 } else { 4.0 }).exp());
        let mut pose = gait_pose(&rig, speed, t);
        if rig.kill_w > 0.001 {
            pose = pose.lerp(kill_pose(rig.kill_t), rig.kill_w);
        }
        let mut set = |e: Entity, f: &dyn Fn(&mut Transform)| {
            if let Ok(mut tr) = parts.get_mut(e) {
                f(&mut tr);
            }
        };
        set(rig.body, &|tr| *tr = pose.body);
        set(rig.neck, &|tr| tr.rotation = pose.neck);
        set(rig.head, &|tr| tr.rotation = pose.head);
        set(rig.tail, &|tr| tr.rotation = pose.tail);
        let (body_pitch, _, _) = pose.body.rotation.to_euler(EulerRot::XYZ);
        for k in 0..4 {
            let hip = pose.body.transform_point(LEG_PIVOTS[k]);
            let front = k < 2;
            let (up, low) = leg_ik(Vec2::new(hip.y, hip.z), pose.feet[k], if front { FRONT_BONES } else { HIND_BONES }, front);
            set(rig.upper[k], &|tr| tr.rotation = angle_x(up - body_pitch));
            set(rig.lower[k], &|tr| tr.rotation = angle_x(low));
        }
    }
}
