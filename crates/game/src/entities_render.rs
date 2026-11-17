//! Renders beavers, trees, lodges, dams, canals, burrows, workshops, water wheels, docks and barges
//! as round, cute, procedural low-poly models.
//!
//! * `entities_render/meshgen.rs` – vertex-coloured mesh toolkit.
//! * `entities_render/models.rs`  – the actual models (beaver parts, trees per species/season/LOD, structures).
//! * `entities_render/scene.rs`   – `RenderScene`, the render-side view of the world, plus the stub
//!   colony used while the sim crates do not expose entities yet.
//!
//! Performance: every model is a vertex-coloured mesh drawn with one shared white material, so Bevy
//! batches/instances all trees of a kind into few draw calls. Trees pick one of three meshes by distance
//! (full, reduced, blob); beavers switch from a 9-part animated rig to a single static mesh and stop
//! animating when far away.
mod meshgen;
mod models;
mod scene;
pub mod sim_sync;
pub mod sim_view;
#[cfg(test)]
mod sim_tests;

use crate::camera::MainCamera;
use crate::cli::Cli;
use crate::state::Sim;
use bevy::app::AppExit;
use bevy::prelude::*;
use bevy::window::{PresentMode, PrimaryWindow, WindowLevel};
use meshgen::Mb;
use models::Season;
use scene::*;
use std::collections::HashMap;

pub use scene::{height_at, lodge_view};

const BEAVER_SCALE: f32 = 1.3;
/// Beavers further than this from the camera use the single static mesh and are not animated.
const BEAVER_NEAR: f32 = 85.0;
/// Tree LOD switch distances.
const TREE_LOD0: f32 = 55.0;
const TREE_LOD1: f32 = 120.0;
/// Seconds of render time shown in `--screenshot` mode (chosen so the hero shot has varied poses).
const SCREENSHOT_CLOCK: f32 = 31.0;
/// Whether the stress scene's itinerary beavers and stub trees are drawn instead of the live sim.
/// `--stress` measures that fixed scene; set `BC_STRESS_SIM=1` to measure the sim-driven world (use with `--midgame`).
fn stub_mode(cli: &Cli) -> bool {
    cli.stress && std::env::var_os("BC_STRESS_SIM").is_none()
}
const DAYS_PER_SEASON: f64 = 30.0;

pub struct EntitiesRenderPlugin;

impl Plugin for EntitiesRenderPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<RenderClock>()
            .init_resource::<CurrentSeason>()
            .init_resource::<FrameWork>()
            .init_resource::<sim_sync::PendingDays>()
            .add_systems(Startup, (init_models, setup_stress_window))
            .add_systems(First, frame_work_start)
            .add_systems(Last, frame_work_end)
            .add_systems(
                Update,
                (
                    update_clock_and_season,
                    spawn_scene,
                    sim_sync::update_pending,
                    sim_sync::sync_sim_beavers,
                    sim_sync::sync_sim_resources,
                    sim_sync::sync_sim_buildings,
                    sync_stub_beavers,
                    attach_rigs,
                    animate_beavers,
                    update_tree_lod,
                    spin_wheels,
                    stress_report,
                )
                    .chain(),
            );
    }
}

// ---------------------------------------------------------------------------------------------
// Resources and components
// ---------------------------------------------------------------------------------------------

#[derive(Resource, Default)]
struct RenderClock(f32);

#[derive(Resource)]
struct CurrentSeason(Season);
impl Default for CurrentSeason {
    fn default() -> Self { Self(Season::Summer) }
}

#[derive(Resource)]
struct Models {
    white: Handle<StandardMaterial>,
    blob_mat: Handle<StandardMaterial>,
    water_mat: Handle<StandardMaterial>,
    /// Indexed by (species, season, lod).
    trees: Vec<Handle<Mesh>>,
    stump: Handle<Mesh>,
    snag: Handle<Mesh>,
    body: Handle<Mesh>,
    head: Handle<Mesh>,
    tail: Handle<Mesh>,
    leg_front: Handle<Mesh>,
    leg_hind: Handle<Mesh>,
    /// Loads held in the mouth, indexed by `Good::index`.
    loads: Vec<Handle<Mesh>>,
    /// Felled trees by (whole logs left * 2 + branches left).
    felled: Vec<Handle<Mesh>>,
    mud: Handle<Mesh>,
    stone: Handle<Mesh>,
    plant: Handle<Mesh>,
    far: Handle<Mesh>,
    ripple: Handle<Mesh>,
    blob: Handle<Mesh>,
    wheel: Handle<Mesh>,
    structs: HashMap<(StructKind, u32, u8), Handle<Mesh>>,
}

fn species_index(s: Species) -> usize {
    match s {
        Species::Aspen => 0,
        Species::Maple => 1,
        Species::Pine => 2,
        Species::Willow => 3,
    }
}

const SPECIES: [Species; 4] = [Species::Aspen, Species::Maple, Species::Pine, Species::Willow];

impl Models {
    fn tree(&self, sp: Species, season: Season, lod: u8) -> Handle<Mesh> {
        self.trees[(species_index(sp) * 4 + season.index()) * 3 + lod as usize].clone()
    }
}

/// Everything spawned from the scene, so it can be rebuilt when the world is regenerated.
#[derive(Component)]
struct SceneEntity;

#[derive(Component)]
struct TreeLod {
    species: Species,
    lod: u8,
    season: Season,
}

#[derive(Component)]
struct WheelSpin(f32);

/// Marks entities drawn from the live sim (resources, buildings).
#[derive(Component)]
struct SimEntity;

/// Marks beaver sprites that follow a sim agent (as opposed to the stress scene's itineraries).
#[derive(Component)]
struct SimBeaver;

/// What one beaver sprite shows, copied from the sim each frame. The sprite's `Transform` translation
/// is the beaver's world position; the rig (see `BeaverRig`) animates the body for `pose`.
#[derive(Component, Clone, Debug)]
pub struct BeaverSprite {
    /// `BeaverId` of the sim agent (or the stress scene's beaver index).
    pub id: u64,
    pub pose: Pose,
    pub load: Option<(sim::buildings::Good, f32)>,
    /// Heading about +Y the beaver is turning towards.
    pub yaw: f32,
    /// True only when the beaver is actually over water; foam ripples show for this, not for the swim pose alone.
    pub wet: bool,
}

#[derive(Component)]
struct BeaverRig {
    body: Entity,
    far: Entity,
    parts: [Entity; 6], // head, tail, 4 legs
    log: Entity,
    ripple: Entity,
    blob: Entity,
    prev: Pose,
    cur: Pose,
    blend: f32,
    near: bool,
    init: bool,
    load: Option<sim::buildings::Good>,
}

// ---------------------------------------------------------------------------------------------
// Startup
// ---------------------------------------------------------------------------------------------

fn init_models(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>, mut materials: ResMut<Assets<StandardMaterial>>) {
    let white = materials.add(StandardMaterial { base_color: Color::WHITE, perceptual_roughness: 0.85, reflectance: 0.12, ..default() });
    let blob_mat = materials.add(StandardMaterial {
        base_color: Color::srgba(0.42, 0.353, 0.541, 0.40), // plum shadow, never black
        alpha_mode: AlphaMode::Blend,
        unlit: true,
        ..default()
    });
    let water_mat = materials.add(StandardMaterial {
        base_color: Color::srgba(0.957, 0.984, 0.965, 0.92), // Foam
        alpha_mode: AlphaMode::Blend,
        unlit: true,
        ..default()
    });
    let mut trees = Vec::new();
    for sp in SPECIES {
        for season in Season::ALL {
            for lod in 0..3u8 {
                trees.push(meshes.add(models::tree_mesh(sp, season, lod)));
            }
        }
    }
    let mut build = |f: &dyn Fn(&mut Mb)| {
        let mut mb = Mb::new();
        f(&mut mb);
        meshes.add(mb.build())
    };
    let body = build(&|m| models::beaver_body(m, Vec3::ZERO));
    let head = build(&|m| models::beaver_head(m, Vec3::ZERO));
    let tail = build(&|m| models::beaver_tail(m, Vec3::ZERO));
    let leg_front = build(&|m| models::beaver_leg(m, true));
    let leg_hind = build(&|m| models::beaver_leg(m, false));
    let far = build(&models::beaver_static);
    let ripple = build(&models::ripple);
    let blob = build(&models::blob_shadow);
    commands.insert_resource(Models {
        white,
        blob_mat,
        water_mat,
        trees,
        stump: meshes.add(models::stump_mesh()),
        snag: meshes.add(models::snag_mesh()),
        body,
        head,
        tail,
        leg_front,
        leg_hind,
        loads: sim::buildings::Good::ALL.iter().map(|g| meshes.add(models::load_mesh(*g))).collect(),
        felled: (0..4u8).flat_map(|l| [false, true].map(|b| meshes.add(models::felled_mesh(l, b)))).collect(),
        mud: meshes.add(models::mud_mesh()),
        stone: meshes.add(models::stone_mesh()),
        plant: meshes.add(models::plant_mesh()),
        far,
        ripple,
        blob,
        wheel: meshes.add(models::wheel_mesh()),
        structs: HashMap::new(),
    });
}

/// Stress mode measures real throughput, so turn vsync off. Keep the window on top and focused: covered
/// windows are throttled to the display refresh by Windows, and unfocused ones can be capped by the
/// GPU driver's "background application max frame rate", which would measure the desktop, not us.
fn setup_stress_window(cli: Res<Cli>, mut windows: Query<&mut Window, With<PrimaryWindow>>) {
    if cli.stress {
        for mut w in &mut windows {
            w.present_mode = PresentMode::AutoNoVsync;
            w.window_level = WindowLevel::AlwaysOnTop;
            w.focused = true;
        }
    }
}

fn update_clock_and_season(time: Res<Time>, cli: Res<Cli>, sim: Res<Sim>, mut clock: ResMut<RenderClock>, mut season: ResMut<CurrentSeason>) {
    clock.0 = if cli.screenshot.is_some() { SCREENSHOT_CLOCK } else { time.elapsed_secs() };
    let idx = match std::env::var("BEAVER_SEASON").ok().and_then(|s| s.parse::<usize>().ok()) {
        Some(i) => i % 4,
        // Year starts in early summer.
        None => ((sim.0.time_days / DAYS_PER_SEASON).floor() as usize + 1) % 4,
    };
    if season.0 != Season::ALL[idx] {
        season.0 = Season::ALL[idx];
    }
}

// ---------------------------------------------------------------------------------------------
// Spawning
// ---------------------------------------------------------------------------------------------

fn spawn_scene(
    mut commands: Commands,
    cli: Res<Cli>,
    sim: Res<Sim>,
    season: Res<CurrentSeason>,
    clock: Res<RenderClock>,
    models: Option<ResMut<Models>>,
    mut meshes: ResMut<Assets<Mesh>>,
    old: Query<Entity, With<SceneEntity>>,
    mut built_for: Local<Option<u64>>,
) {
    let Some(mut models) = models else { return };
    if !stub_mode(&cli) || *built_for == Some(sim.0.seed) {
        return;
    }
    *built_for = Some(sim.0.seed);
    for e in &old {
        commands.entity(e).despawn();
    }
    let cfg = if cli.stress { SceneConfig::STRESS } else { SceneConfig::NORMAL };
    let mut scene = scene_from_sim(&sim.0, cfg);
    let water_dry = sim.0.water.depth.as_slice().iter().all(|&d| d <= 0.0);
    let white = models.white.clone();

    for t in &scene.trees {
        let (mesh, lod) = match t.state {
            TreeState::Alive => (models.tree(t.species, season.0, 0), Some(TreeLod { species: t.species, lod: 0, season: season.0 })),
            TreeState::Stump => (models.stump.clone(), None),
            TreeState::Snag => (models.snag.clone(), None),
        };
        let mut e = commands.spawn((
            SceneEntity,
            Mesh3d(mesh),
            MeshMaterial3d(white.clone()),
            Transform::from_translation(t.pos).with_rotation(Quat::from_rotation_y(t.yaw)).with_scale(Vec3::splat(t.scale)),
        ));
        if let Some(l) = lod {
            e.insert(l);
        }
    }

    for s in &scene.structs {
        let stage = models::stage_of(s.progress);
        let key = (s.kind, (s.len * 2.0).round() as u32, stage);
        let mesh = models.structs.entry(key).or_insert_with(|| meshes.add(models::struct_mesh(s.kind, key.1 as f32 * 0.5, stage))).clone();
        let mut e = commands.spawn((
            SceneEntity,
            Mesh3d(mesh),
            MeshMaterial3d(white.clone()),
            Transform::from_translation(s.pos).with_rotation(Quat::from_rotation_y(s.yaw)),
        ));
        if s.kind == StructKind::WaterWheel && stage == 3 {
            e.with_children(|p| {
                p.spawn((Mesh3d(models.wheel.clone()), MeshMaterial3d(white.clone()), Transform::from_xyz(0.0, 2.0, 0.0), WheelSpin(0.9)));
            });
        }
    }

    if let (Some(pond), true) = (scene.pond, water_dry) {
        // Stub water until the water renderer draws the sim's water field.
        let surface = meshes.add(models::pond_surface(&pond));
        commands.spawn((SceneEntity, Mesh3d(surface), MeshMaterial3d(models.water_mat.clone()), Transform::from_xyz(pond.center.x, pond.level, pond.center.y)));
    }

    // Stress-scene beavers: sprites driven by itineraries; `attach_rigs` gives them their bodies.
    let pond = scene.pond;
    for i in 0..scene.beavers.len() {
        let st = eval_beaver(&scene, i, clock.0, &sim);
        scene.beavers[i].state = st;
        let ground = ground_y(&sim, st, pond);
        commands.spawn((
            SceneEntity,
            StubBeaver(i),
            BeaverSprite { id: i as u64, pose: st.pose, load: st.load, yaw: st.yaw, wet: st.pose == Pose::Swim },
            Transform::from_xyz(st.pos.x, ground, st.pos.y).with_rotation(Quat::from_rotation_y(st.yaw)).with_scale(Vec3::splat(BEAVER_SCALE)),
            Visibility::default(),
        ));
    }
    commands.insert_resource(scene);
}

fn ground_y(sim: &Sim, st: BeaverState, pond: Option<Pond>) -> f32 {
    let g = height_at(&sim.0.height, st.pos.x, st.pos.y);
    match (st.pose, pond) {
        (Pose::Swim, Some(p)) if p.contains(st.pos, 0.5) => p.level.max(g),
        _ => g,
    }
}

fn eval_beaver(scene: &RenderScene, i: usize, t: f32, sim: &Sim) -> BeaverState {
    let b = &scene.beavers[i];
    let pond = scene.pond;
    match &b.route {
        Some(r) => r.eval(t, |p| pond.is_some_and(|po| po.swimmable(&sim.0.height, p))),
        None => b.state,
    }
}

// ---------------------------------------------------------------------------------------------
// Per-frame systems
// ---------------------------------------------------------------------------------------------

#[derive(Component)]
struct StubBeaver(usize);

/// Stress scene only: beavers walk looping itineraries instead of following sim agents.
fn sync_stub_beavers(clock: Res<RenderClock>, sim: Res<Sim>, scene: Option<ResMut<RenderScene>>, mut q: Query<(&StubBeaver, &mut Transform, &mut BeaverSprite)>) {
    let Some(mut scene) = scene else { return };
    for i in 0..scene.beavers.len() {
        let st = eval_beaver(&scene, i, clock.0, &sim);
        scene.beavers[i].state = st;
    }
    for (b, mut tr, mut sprite) in &mut q {
        let Some(v) = scene.beavers.get(b.0) else { continue };
        let st = v.state;
        tr.translation = Vec3::new(st.pos.x, ground_y(&sim, st, scene.pond), st.pos.y);
        sprite.pose = st.pose;
        sprite.load = st.load;
        sprite.yaw = st.yaw;
        sprite.wet = st.pose == Pose::Swim;
    }
}

/// Gives every new sprite its animated body: head, tail, legs, a held load, far LOD mesh, ripple, shadow.
fn attach_rigs(mut commands: Commands, models: Option<Res<Models>>, q: Query<(Entity, &BeaverSprite), (With<BeaverSprite>, Without<BeaverRig>)>) {
    let Some(m) = models else { return };
    let w = &m.white;
    for (root, sprite) in &q {
        let mut body = Entity::PLACEHOLDER;
        commands.entity(root).with_children(|p| {
            body = p.spawn((Mesh3d(m.body.clone()), MeshMaterial3d(w.clone()), Transform::default())).id();
        });
        let mut parts = [Entity::PLACEHOLDER; 6];
        let (mut log, mut far, mut ripple, mut blob) = (Entity::PLACEHOLDER, Entity::PLACEHOLDER, Entity::PLACEHOLDER, Entity::PLACEHOLDER);
        commands.entity(body).with_children(|p| {
            parts[0] = p.spawn((Mesh3d(m.head.clone()), MeshMaterial3d(w.clone()), Transform::from_translation(models::HEAD_PIVOT))).id();
            parts[1] = p.spawn((Mesh3d(m.tail.clone()), MeshMaterial3d(w.clone()), Transform::from_translation(models::TAIL_PIVOT))).id();
            for k in 0..4 {
                let mesh = if k < 2 { m.leg_front.clone() } else { m.leg_hind.clone() };
                parts[2 + k] = p.spawn((Mesh3d(mesh), MeshMaterial3d(w.clone()), Transform::from_translation(models::LEG_PIVOTS[k]))).id();
            }
        });
        commands.entity(parts[0]).with_children(|p| {
            log = p.spawn((Mesh3d(m.loads[0].clone()), MeshMaterial3d(w.clone()), Transform::default(), Visibility::Hidden)).id();
        });
        commands.entity(root).with_children(|p| {
            far = p.spawn((Mesh3d(m.far.clone()), MeshMaterial3d(w.clone()), Transform::default(), Visibility::Hidden)).id();
            ripple = p.spawn((Mesh3d(m.ripple.clone()), MeshMaterial3d(w.clone()), Transform::from_xyz(0.0, 0.06, 0.2), Visibility::Hidden)).id();
            blob = p.spawn((Mesh3d(m.blob.clone()), MeshMaterial3d(m.blob_mat.clone()), Transform::from_xyz(0.0, 0.05, 0.0))).id();
        });
        commands.entity(root).insert(BeaverRig { body, far, parts, log, ripple, blob, prev: sprite.pose, cur: sprite.pose, blend: 1.0, near: true, init: false, load: None });
    }
}

type Rig = [(Vec3, Quat); 7]; // body, head, tail, 4 legs

fn pose_rig(pose: Pose, t: f32, id: f32) -> Rig {
    let rx = Quat::from_rotation_x;
    let ry = Quat::from_rotation_y;
    let rz = Quat::from_rotation_z;
    let mut r: Rig = [
        (Vec3::ZERO, Quat::IDENTITY),
        (models::HEAD_PIVOT, Quat::IDENTITY),
        (models::TAIL_PIVOT, Quat::IDENTITY),
        (models::LEG_PIVOTS[0], Quat::IDENTITY),
        (models::LEG_PIVOTS[1], Quat::IDENTITY),
        (models::LEG_PIVOTS[2], Quat::IDENTITY),
        (models::LEG_PIVOTS[3], Quat::IDENTITY),
    ];
    let ph = t * 9.0 + id;
    match pose {
        Pose::Idle => {
            r[0].0.y = 0.015 * (t * 1.8 + id).sin();
            r[1].1 = ry(0.45 * (t * 0.7 + id).sin()) * rx(0.05 * (t * 1.8).sin() - 0.05);
            r[2].1 = rx(0.1) * ry(0.12 * (t * 0.9 + id).sin());
        }
        Pose::Walk | Pose::Carry => {
            let s = ph.sin();
            r[0].0.y = 0.05 * (2.0 * ph).sin().abs();
            r[0].1 = rz(0.05 * s) * rx(0.03 * (2.0 * ph).sin());
            r[1].1 = rx(0.07 * (2.0 * ph + 1.0).sin());
            r[2].1 = rx(0.12) * ry(0.35 * (ph * 0.5).sin());
            r[3].1 = rx(0.75 * s);
            r[4].1 = rx(-0.75 * s);
            r[5].1 = rx(-0.75 * s);
            r[6].1 = rx(0.75 * s);
            if pose == Pose::Carry {
                // paws up holding a log in the teeth, head high
                r[1].1 = rx(-0.3 + 0.05 * (2.0 * ph).sin());
                r[3].1 = rx(-1.0 + 0.1 * s);
                r[4].1 = rx(-1.0 - 0.1 * s);
                r[0].1 = rx(-0.06);
            }
        }
        Pose::Gnaw => {
            r[0].1 = rx(-0.5);
            r[0].0.y = 0.1;
            r[1].1 = rx(0.45 + 0.17 * (t * 15.0 + id).sin()) * ry(0.06 * (t * 7.0).sin());
            r[2].1 = rx(0.55);
            r[3].1 = rx(-1.25 + 0.08 * (t * 15.0).sin());
            r[4].1 = rx(-1.25 - 0.08 * (t * 15.0).sin());
            r[5].1 = rx(-0.55);
            r[6].1 = rx(-0.55);
        }
        Pose::Climb => {
            // hauling up the dam: body pitched up, forepaws reaching, hind legs pushing, alternating
            let s = (t * 7.0 + id).sin();
            r[0].0.y = 0.1 + 0.04 * (2.0 * (t * 7.0 + id)).sin().abs();
            r[0].1 = rx(-0.38) * rz(0.06 * s);
            r[1].1 = rx(-0.1 + 0.08 * s);
            r[2].1 = rx(0.45) * ry(0.2 * s);
            r[3].1 = rx(-1.45 + 0.5 * s);
            r[4].1 = rx(-1.45 - 0.5 * s);
            r[5].1 = rx(0.5 - 0.6 * s);
            r[6].1 = rx(0.5 + 0.6 * s);
        }
        Pose::Work => {
            // crouched over the job, forepaws patting/packing in alternation, tail braced
            let s = (t * 8.0 + id).sin();
            r[0].0.y = 0.0;
            r[0].1 = rx(0.1);
            r[1].1 = rx(0.35 + 0.08 * (t * 4.0 + id).sin()) * ry(0.1 * (t * 2.0).sin());
            r[2].1 = rx(0.35);
            r[3].1 = rx(-0.9 + 0.5 * s);
            r[4].1 = rx(-0.9 - 0.5 * s);
            r[5].1 = rx(-0.2);
            r[6].1 = rx(-0.2);
        }
        Pose::Rest => {
            // lying curled up: belly low, head tucked on the paws, slow breathing
            let b = (t * 1.5 + id).sin();
            r[0].0.y = -0.2 + 0.012 * b;
            r[0].1 = rz(0.04);
            r[1].0.y -= 0.08;
            r[1].1 = rx(0.6) * ry(0.5 + 0.05 * b);
            r[2].1 = rx(0.2) * ry(0.9);
            r[3].1 = rx(-0.5);
            r[4].1 = rx(-0.5);
            r[5].1 = rx(-0.5);
            r[6].1 = rx(-0.5);
        }
        Pose::Swim => {
            let p2 = t * 7.0 + id;
            r[0].0.y = -0.34 + 0.02 * (t * 2.5 + id).sin();
            r[0].1 = rx(-0.06);
            r[1].1 = rx(-0.14);
            r[2].1 = rx(0.05) * ry(0.4 * (t * 3.0 + id).sin());
            r[3].1 = rx(0.9);
            r[4].1 = rx(0.9);
            r[5].1 = rx(0.9 * p2.sin());
            r[6].1 = rx(-0.9 * p2.sin());
        }
    }
    r
}

/// Foam ripple arcs (and no ground shadow) only for a swimming beaver that is actually over water:
/// a beaver on land, even one in the swim gait about to enter the water, shows none.
pub(crate) fn shows_ripple(pose: Pose, wet: bool) -> bool {
    pose == Pose::Swim && wet
}

fn animate_beavers(
    time: Res<Time>,
    clock: Res<RenderClock>,
    models: Option<Res<Models>>,
    cam: Query<&GlobalTransform, With<MainCamera>>,
    mut roots: Query<(&mut Transform, &mut BeaverRig, &BeaverSprite)>,
    mut parts: Query<&mut Transform, Without<BeaverRig>>,
    mut vis: Query<&mut Visibility, Without<BeaverRig>>,
    mut meshes: Query<&mut Mesh3d, Without<BeaverRig>>,
) {
    let Some(models) = models else { return };
    let cam_pos = cam.iter().next().map(|c| c.translation());
    let dt = time.delta_secs();
    let turn = 1.0 - (-dt * 10.0).exp();
    for (mut tr, mut rig, sprite) in &mut roots {
        // The sprite's translation is the sim position and is never smoothed; only the heading eases.
        let target_rot = Quat::from_rotation_y(sprite.yaw);
        tr.rotation = if rig.init { tr.rotation.slerp(target_rot, turn) } else { target_rot };

        let near = cam_pos.is_none_or(|c| c.distance(tr.translation) < BEAVER_NEAR);
        let swimming = sprite.pose == Pose::Swim;
        let wet = shows_ripple(sprite.pose, sprite.wet);
        if near != rig.near || !rig.init {
            rig.near = near;
            if let Ok(mut v) = vis.get_mut(rig.body) {
                *v = if near { Visibility::Inherited } else { Visibility::Hidden };
            }
            if let Ok(mut v) = vis.get_mut(rig.far) {
                *v = if near { Visibility::Hidden } else { Visibility::Inherited };
            }
        }
        if !near {
            if let Ok(mut t) = parts.get_mut(rig.far) {
                t.translation.y = if swimming { -0.34 } else { 0.0 };
            }
        }
        if sprite.pose != rig.cur {
            rig.prev = rig.cur;
            rig.cur = sprite.pose;
            rig.blend = if rig.init { 0.0 } else { 1.0 };
        }
        rig.blend = (rig.blend + dt * 5.0).min(1.0);
        if let Ok(mut v) = vis.get_mut(rig.ripple) {
            *v = if wet { Visibility::Inherited } else { Visibility::Hidden };
        }
        if let Ok(mut v) = vis.get_mut(rig.blob) {
            *v = if wet { Visibility::Hidden } else { Visibility::Inherited };
        }
        // The load follows the head; its model and size show what it is and how much.
        let good = sprite.load.map(|l| l.0);
        if good != rig.load || !rig.init {
            rig.load = good;
            if let Ok(mut v) = vis.get_mut(rig.log) {
                *v = if good.is_some() { Visibility::Inherited } else { Visibility::Hidden };
            }
            if let (Some(g), Ok(mut m)) = (good, meshes.get_mut(rig.log)) {
                m.0 = models.loads[g.index()].clone();
            }
        }
        if let (Some((_, amount)), Ok(mut t)) = (sprite.load, parts.get_mut(rig.log)) {
            let s = 0.6 + 0.4 * (amount / sim::agents::LOAD_SIZE).clamp(0.0, 1.0);
            t.scale = Vec3::splat(s);
        }
        if wet {
            if let Ok(mut t) = parts.get_mut(rig.ripple) {
                let s = 1.0 + 0.12 * (clock.0 * 3.0 + sprite.id as f32).sin();
                t.scale = Vec3::new(s, 1.0, s);
            }
        }
        rig.init = true;
        if !near {
            continue;
        }
        let id = sprite.id as f32 * 1.7;
        let a = pose_rig(rig.cur, clock.0, id);
        let blend = rig.blend * rig.blend * (3.0 - 2.0 * rig.blend);
        let r = if blend < 1.0 {
            let p = pose_rig(rig.prev, clock.0, id);
            let mut o = a;
            for k in 0..7 {
                o[k] = (p[k].0.lerp(a[k].0, blend), p[k].1.slerp(a[k].1, blend));
            }
            o
        } else {
            a
        };
        let ents = [rig.body, rig.parts[0], rig.parts[1], rig.parts[2], rig.parts[3], rig.parts[4], rig.parts[5]];
        for (e, (pos, rot)) in ents.iter().zip(r.iter()) {
            if let Ok(mut t) = parts.get_mut(*e) {
                t.translation = *pos;
                t.rotation = *rot;
            }
        }
    }
}

/// Swaps tree meshes with distance and season.
fn update_tree_lod(
    season: Res<CurrentSeason>,
    models: Option<Res<Models>>,
    cam: Query<&GlobalTransform, With<MainCamera>>,
    mut trees: Query<(&Transform, &mut TreeLod, &mut Mesh3d)>,
) {
    let (Some(models), Some(cam)) = (models, cam.iter().next()) else { return };
    let c = cam.translation();
    let (l0, l1) = (TREE_LOD0 * TREE_LOD0, TREE_LOD1 * TREE_LOD1);
    for (t, mut lod, mut mesh) in &mut trees {
        let d2 = t.translation.distance_squared(c);
        let want = if d2 < l0 { 0 } else if d2 < l1 { 1 } else { 2 };
        if want != lod.lod || lod.season != season.0 {
            lod.lod = want;
            lod.season = season.0;
            mesh.0 = models.tree(lod.species, season.0, want);
        }
    }
}

fn spin_wheels(time: Res<Time>, mut q: Query<(&mut Transform, &WheelSpin)>) {
    for (mut t, w) in &mut q {
        t.rotation = Quat::from_rotation_x(time.elapsed_secs() * w.0);
    }
}

// ---------------------------------------------------------------------------------------------
// Stress mode: log frame times, then exit.
// ---------------------------------------------------------------------------------------------

const STRESS_WARMUP: u32 = 240;

/// Main-world CPU time of the current frame (First..Last), to tell our own work apart from
/// render/present/OS stalls when a frame goes over budget.
#[derive(Resource, Default)]
struct FrameWork {
    start: Option<std::time::Instant>,
    last_ms: f32,
    last_focused: bool,
}

fn frame_work_start(mut w: ResMut<FrameWork>) {
    w.start = Some(std::time::Instant::now());
}

fn frame_work_end(mut w: ResMut<FrameWork>, win: Query<&Window, With<PrimaryWindow>>) {
    if let Some(s) = w.start {
        w.last_ms = s.elapsed().as_secs_f32() * 1000.0;
    }
    w.last_focused = win.iter().next().is_some_and(|w| w.focused);
}
const STRESS_FRAMES: usize = 900;
const STRESS_BUDGET_MS: f32 = 16.0;

fn stress_report(cli: Res<Cli>, time: Res<Time<Real>>, drawn_beavers: Query<(), With<BeaverSprite>>, drawn_trees: Query<(), With<TreeLod>>, work: Res<FrameWork>, mut samples: Local<Vec<f32>>, mut works: Local<Vec<(f32, bool)>>, mut frame: Local<u32>, mut exit: MessageWriter<AppExit>) {
    if !cli.stress {
        return;
    }
    *frame += 1;
    if *frame <= STRESS_WARMUP {
        return;
    }
    samples.push(time.delta_secs() * 1000.0);
    // `delta` measures the previous frame, whose main-world work is `work.last_ms`.
    works.push((work.last_ms, work.last_focused));
    if samples.len() < STRESS_FRAMES {
        return;
    }
    let mut s = samples.clone();
    s.sort_by(|a, b| a.total_cmp(b));
    let avg = s.iter().sum::<f32>() / s.len() as f32;
    let pct = |p: f32| s[((s.len() as f32 * p) as usize).min(s.len() - 1)];
    let max = s[s.len() - 1];
    // Every frame counts: a single hitch over budget is a visible stutter, so the gate is the max.
    let over: Vec<String> = samples
        .iter()
        .enumerate()
        .filter(|(_, ms)| **ms >= STRESS_BUDGET_MS)
        .map(|(i, ms)| format!("#{i}={ms:.2}(cpu {:.2}{})", works[i].0, if works[i].1 { "" } else { ", unfocused" }))
        .collect();
    let unfocused = works.iter().filter(|w| !w.1).count();
    let mut w: Vec<f32> = works.iter().map(|w| w.0).collect();
    w.sort_by(|a, b| a.total_cmp(b));
    // Count what is actually drawn, so the sim-driven run (`BC_STRESS_SIM=1`) reports its own world.
    let (trees, beavers) = (drawn_trees.iter().count(), drawn_beavers.iter().count());
    let source = if stub_mode(&cli) { "stub itinerary scene" } else { "live sim world" };
    let line = format!(
        "stress scene ({source}): {trees} trees, {beavers} beavers | {} frames | frame time ms: avg {avg:.2}, median {:.2}, p95 {:.2}, p99 {:.2}, max {max:.2} | main-world cpu ms: median {:.2}, max {:.2} | unfocused frames: {unfocused} | frames >= {STRESS_BUDGET_MS} ms: {}{} | budget {STRESS_BUDGET_MS} ms: {}",
        s.len(),
        pct(0.5),
        pct(0.95),
        pct(0.99),
        w[w.len() / 2],
        w[w.len() - 1],
        over.len(),
        if over.is_empty() { String::new() } else { format!(" [{}]", over.join(" ")) },
        if over.is_empty() { "PASS (every frame < 16 ms)" } else { "FAIL" },
    );
    info!("{line}");
    let path = std::env::var("BEAVER_STRESS_LOG").unwrap_or_else(|_| "stress_frame_times.log".into());
    if let Err(e) = std::fs::write(&path, format!("{line}\n")) {
        error!("cannot write {path}: {e}");
    }
    exit.write(if over.is_empty() { AppExit::Success } else { AppExit::error() });
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::mesh::VertexAttributeValues;

    fn check(name: &str, m: &Mesh) {
        let n = m.count_vertices();
        assert!(n > 0, "{name}: empty");
        let Some(bevy::mesh::Indices::U32(idx)) = m.indices() else { panic!("{name}: no indices") };
        assert!(idx.iter().all(|&i| (i as usize) < n), "{name}: index out of range");
        let Some(VertexAttributeValues::Float32x3(p)) = m.attribute(Mesh::ATTRIBUTE_POSITION) else { panic!("{name}: no positions") };
        assert!(p.iter().all(|v| v.iter().all(|c| c.is_finite())), "{name}: non-finite position");
        let Some(VertexAttributeValues::Float32x3(nm)) = m.attribute(Mesh::ATTRIBUTE_NORMAL) else { panic!("{name}: no normals") };
        assert_eq!(nm.len(), n);
        assert!(nm.iter().all(|v| (v[0] * v[0] + v[1] * v[1] + v[2] * v[2] - 1.0).abs() < 1e-3 || *v == [0.0; 3]), "{name}: bad normal");
    }

    #[test]
    fn all_models_are_valid() {
        for sp in SPECIES {
            for s in Season::ALL {
                for lod in 0..3 {
                    check("tree", &models::tree_mesh(sp, s, lod));
                }
            }
        }
        check("stump", &models::stump_mesh());
        check("snag", &models::snag_mesh());
        check("wheel", &models::wheel_mesh());
        for kind in [StructKind::Lodge, StructKind::Dam, StructKind::Canal, StructKind::Burrow, StructKind::Workshop, StructKind::WaterWheel, StructKind::Dock, StructKind::Barge] {
            for stage in 0..4 {
                check("struct", &models::struct_mesh(kind, 4.0, stage));
            }
        }
    }

    /// The stub pond must sit in the terrain, not flood it: most trees stay alive, the pond is
    /// pond-sized, and the buildings do not overlap.
    #[test]
    fn stub_pond_fits_the_terrain() {
        for seed in 1..=4 {
            let sim = sim::SimState::new(seed);
            let sc = scene_from_sim(&sim, SceneConfig::NORMAL);
            let snags = sc.trees.iter().filter(|t| t.state == TreeState::Snag).count();
            let alive = sc.trees.iter().filter(|t| t.state == TreeState::Alive).count();
            assert!(alive > sc.trees.len() * 3 / 4, "seed {seed}: only {alive}/{} trees alive", sc.trees.len());
            assert!(snags * 20 < sc.trees.len(), "seed {seed}: {snags} drowned snags");
            let pond = sc.pond.expect("stub pond");
            let mean = pond.shore_r.iter().sum::<f32>() / pond.shore_r.len() as f32;
            assert!((6.0..=20.0).contains(&mean), "seed {seed}: pond radius {mean}");
            let big: Vec<_> = sc.structs.iter().filter(|s| matches!(s.kind, StructKind::Lodge | StructKind::Workshop)).collect();
            for (i, a) in big.iter().enumerate() {
                for b in &big[i + 1..] {
                    let d = Vec2::new(a.pos.x - b.pos.x, a.pos.z - b.pos.z).length();
                    assert!(d > 5.0, "seed {seed}: {:?} and {:?} overlap ({d:.1})", a.kind, b.kind);
                }
            }
        }
    }

    #[test]
    fn stub_scene_is_deterministic_and_sized() {
        let sim = sim::SimState::new(1);
        let a = scene_from_sim(&sim, SceneConfig::STRESS);
        let b = scene_from_sim(&sim, SceneConfig::STRESS);
        assert_eq!(a.trees.len(), 5000);
        
        assert!(a.beavers.len() >= 295, "got {}", a.beavers.len());
        assert_eq!(a.trees[17].pos, b.trees[17].pos);
        assert!(a.structs.len() >= 20);
    }
}
