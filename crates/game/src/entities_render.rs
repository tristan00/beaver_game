mod meshgen;
mod models;
mod scene;
mod structures;
pub mod sim_sync;
pub mod sim_view;

use crate::camera::MainCamera;
use crate::cli::Cli;
use bevy::prelude::*;
use meshgen::Mb;
use models::Season;
use scene::*;
use std::collections::HashMap;

pub use scene::{height_at, lodge_view};

const BEAVER_NEAR: f32 = 85.0;
const TREE_LOD0: f32 = 220.0;
const TREE_LOD1: f32 = 480.0;
const SCREENSHOT_CLOCK: f32 = 31.0;
pub struct EntitiesRenderPlugin;

impl Plugin for EntitiesRenderPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<RenderClock>()
            .init_resource::<CurrentSeason>()
            .init_resource::<CanopyView>()
            .init_resource::<sim_sync::PendingDays>()
            .add_systems(Startup, init_models)
            .add_systems(
                Update,
                (
                    update_clock_and_season,
                    sim_sync::update_pending,
                    sim_sync::sync_sim_beavers,
                    sim_sync::sync_sim_resources,
                    sim_sync::sync_sim_buildings,
                    sim_sync::sync_sim_dams,
                    sim_sync::sync_reinforcements,
                    sim_sync::sync_sim_barges,
                    attach_rigs,
                    animate_beavers,
                    canopy_key,
                    update_tree_lod,
                    sway_trees,
                    spin_wheels,
                )
                    .chain()
                    .after(crate::terrain_render::update_season),
            );
    }
}

#[derive(Resource, Default)]
struct RenderClock(f32);

#[derive(Resource)]
struct CurrentSeason(Season);
impl Default for CurrentSeason {
    fn default() -> Self { Self(Season::at(0.0)) }
}

#[derive(Resource)]
pub(crate) struct CanopyView(pub bool);
impl Default for CanopyView {
    fn default() -> Self { Self(std::env::var_os("BC_CUTAWAY").is_some()) }
}

fn canopy_key(keys: Res<ButtonInput<KeyCode>>, typing: Option<Res<crate::hud::TextCapture>>,
    menu: Option<Res<crate::hud::MenuOpen>>, mut view: ResMut<CanopyView>) {
    if typing.is_some_and(|t|t.0) || menu.is_some_and(|m|m.0) { return; }
    if keys.just_pressed(KeyCode::F8) { view.0 = !view.0; }
}

#[derive(Resource)]
struct Models {
    white: Handle<StandardMaterial>,
    tree_tints: [Handle<StandardMaterial>; 4],
    old_tree: Handle<StandardMaterial>,
    blob_mat: Handle<StandardMaterial>,
    trees: Vec<Handle<Mesh>>,
    cutaway_trees: Vec<Handle<Mesh>>,
    snag: Handle<Mesh>,
    body: Handle<Mesh>,
    head: Handle<Mesh>,
    tail: Handle<Mesh>,
    leg_front: Handle<Mesh>,
    leg_hind: Handle<Mesh>,
    lids: Handle<Mesh>,
    chips: Handle<Mesh>,
    loads: Vec<Handle<Mesh>>,
    felled: Vec<Handle<Mesh>>,
    mud: Handle<Mesh>,
    plant: Handle<Mesh>,
    far: Handle<Mesh>,
    ripple: Handle<Mesh>,
    blob: Handle<Mesh>,
    wheel: Handle<Mesh>,
    structs: HashMap<(StructKind, u32, u8), Handle<Mesh>>,
    dams: HashMap<(u8, bool, u32, bool), Handle<Mesh>>,
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
        self.trees[(species_index(sp) * Season::COUNT + season.index()) * 3 + lod as usize].clone()
    }
}


#[derive(Component)]
struct TreeLod {
    species: Species,
    lod: u8,
    season: Season,
    cutaway: bool,
}

#[derive(Component)]
struct Sway(Quat);

#[derive(Component)]
struct WheelSpin(f32);

#[derive(Component)]
struct SimEntity;

#[derive(Component)]
struct SimBeaver;

#[derive(Component, Clone, Debug)]
pub struct BeaverSprite {
    pub id: u64,
    pub pose: Pose,
    pub load: Option<(sim::buildings::Good, f32)>,
    pub yaw: f32,
    pub wet: bool,
}

#[derive(Component)]
pub(crate) struct BeaverRig {
    body: Entity,
    far: Entity,
    parts: [Entity; 6],
    log: Entity,
    lids: Entity,
    chips: Entity,
    ripple: Entity,
    blob: Entity,
    prev: Pose,
    cur: Pose,
    blend: f32,
    near: bool,
    init: bool,
    load: Option<sim::buildings::Good>,
    stride: f32,
    last_pos: Option<Vec3>,
}

const SNAP_JUMP: f32 = 3.0;
const STRIDE: f32 = 0.66;
const MAX_STRIDE_HZ: f32 = 4.5;

pub const ENTITY_GAIN: f32 = 0.55;

fn init_models(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>, mut materials: ResMut<Assets<StandardMaterial>>) {
    let white = materials.add(StandardMaterial { base_color: Color::linear_rgb(ENTITY_GAIN, ENTITY_GAIN, ENTITY_GAIN), perceptual_roughness: 1.0, reflectance: 0.0, ..default() });
    let tree_tints = [[1.0, 1.0, 1.0], [0.94, 0.98, 0.93], [0.98, 0.95, 0.90], [0.91, 0.94, 0.91]].map(|[r,g,b]| {
        materials.add(StandardMaterial { base_color: Color::linear_rgb(ENTITY_GAIN * r, ENTITY_GAIN * g, ENTITY_GAIN * b), perceptual_roughness: 1.0, reflectance: 0.0, ..default() })
    });
    // Old living trees keep healthy foliage; dead snags have their own bare mesh.
    let old_tree = materials.add(StandardMaterial { base_color: Color::linear_rgb(ENTITY_GAIN * 0.86, ENTITY_GAIN * 0.90, ENTITY_GAIN * 0.82), perceptual_roughness: 1.0, reflectance: 0.0, ..default() });
    let blob_mat = materials.add(StandardMaterial {
        base_color: Color::srgba(0.42, 0.353, 0.541, 0.40),
        alpha_mode: AlphaMode::Blend,
        unlit: true,
        ..default()
    });
    let mut trees = Vec::new();
    for sp in SPECIES {
        for season in Season::all() {
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
    let lids = build(&models::beaver_lids);
    let chips = build(&models::gnaw_chips);
    let far = build(&models::beaver_static);
    let ripple = build(&models::ripple);
    let blob = build(&models::blob_shadow);
    commands.insert_resource(Models {
        white,
        tree_tints,
        old_tree,
        blob_mat,
        trees,
        cutaway_trees: SPECIES.into_iter().map(|sp|meshes.add(models::tree_cutaway_mesh(sp))).collect(),
        snag: meshes.add(models::snag_mesh()),
        body,
        head,
        tail,
        leg_front,
        leg_hind,
        lids,
        chips,
        loads: sim::buildings::Good::ALL.iter().map(|g| meshes.add(models::load_mesh(*g))).collect(),
        felled: (0..4u8).flat_map(|l| [false, true].map(|b| meshes.add(models::felled_mesh(l, b)))).collect(),
        mud: meshes.add(models::mud_mesh()),
        plant: meshes.add(models::plant_mesh()),
        far,
        ripple,
        blob,
        wheel: meshes.add(models::wheel_mesh()),
        structs: HashMap::new(),
        dams: HashMap::new(),
    });
}

fn update_clock_and_season(time: Res<Time>, cli: Res<Cli>, visual: Res<crate::terrain_render::Season>, mut clock: ResMut<RenderClock>, mut season: ResMut<CurrentSeason>) {
    let shot = || std::env::var("BC_CLOCK").ok().and_then(|s| s.parse().ok()).unwrap_or(SCREENSHOT_CLOCK);
    let frozen = cli.screenshot.is_some() && std::env::var_os("BC_BURST").is_none();
    clock.0 = if frozen { shot() } else { time.elapsed_secs() };
    let next = Season::at(visual.phase);
    if season.0 != next {
        season.0 = next;
    }
}

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
        let (mut lids, mut chips) = (Entity::PLACEHOLDER, Entity::PLACEHOLDER);
        commands.entity(parts[0]).with_children(|p| {
            log = p.spawn((Mesh3d(m.loads[0].clone()), MeshMaterial3d(w.clone()), Transform::default(), Visibility::Hidden)).id();
            lids = p.spawn((Mesh3d(m.lids.clone()), MeshMaterial3d(w.clone()), Transform::default(), Visibility::Hidden)).id();
            chips = p.spawn((Mesh3d(m.chips.clone()), MeshMaterial3d(w.clone()), Transform::default(), Visibility::Hidden)).id();
        });
        commands.entity(root).with_children(|p| {
            far = p.spawn((Mesh3d(m.far.clone()), MeshMaterial3d(w.clone()), Transform::default(), Visibility::Hidden)).id();
            ripple = p.spawn((Mesh3d(m.ripple.clone()), MeshMaterial3d(w.clone()), Transform::from_xyz(0.0, 0.06, 0.2), Visibility::Hidden)).id();
            blob = p.spawn((Mesh3d(m.blob.clone()), MeshMaterial3d(m.blob_mat.clone()), Transform::from_xyz(0.0, 0.05, 0.0))).id();
        });
        commands.entity(root).insert(BeaverRig { body, far, parts, log, lids, chips, ripple, blob, prev: sprite.pose, cur: sprite.pose, blend: 1.0, near: true, init: false, load: None, stride: sprite.id as f32 * 1.7, last_pos: None });
    }
}

#[derive(Clone, Copy)]
struct Rig {
    parts: [(Vec3, Quat); 7],
    squash: Vec3,
}

impl Rig {
    fn rest() -> Rig {
        Rig {
            parts: [
                (Vec3::ZERO, Quat::IDENTITY),
                (models::HEAD_PIVOT, Quat::IDENTITY),
                (models::TAIL_PIVOT, Quat::IDENTITY),
                (models::LEG_PIVOTS[0], Quat::IDENTITY),
                (models::LEG_PIVOTS[1], Quat::IDENTITY),
                (models::LEG_PIVOTS[2], Quat::IDENTITY),
                (models::LEG_PIVOTS[3], Quat::IDENTITY),
            ],
            squash: Vec3::ONE,
        }
    }

    fn lerp(&self, o: &Rig, w: f32) -> Rig {
        let mut r = *self;
        for k in 0..7 {
            r.parts[k] = (self.parts[k].0.lerp(o.parts[k].0, w), self.parts[k].1.slerp(o.parts[k].1, w));
        }
        r.squash = self.squash.lerp(o.squash, w);
        r
    }

    fn pivot_body(&mut self, pivot: Vec3, rot: Quat) {
        self.parts[0] = (self.parts[0].0 + pivot - rot * pivot, rot * self.parts[0].1);
    }
}

fn smooth(a: f32, b: f32, x: f32) -> f32 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn window(x: f32, a: f32, b: f32, ramp: f32) -> f32 { smooth(a, a + ramp, x) * (1.0 - smooth(b - ramp, b, x)) }

const IDLE_LOOP: f32 = 14.0;

pub(crate) fn blinking(t: f32, id: f32) -> bool { (t * 0.29 + id * 0.37).rem_euclid(1.0) < 0.04 }

fn idle_rig(t: f32, id: f32) -> Rig {
    let (rx, ry) = (Quat::from_rotation_x, Quat::from_rotation_y);
    let mut r = Rig::rest();
    let br = (t * 1.8 + id).sin();
    r.parts[0].0.y = 0.015 * br;
    r.squash = Vec3::new(1.0 + 0.012 * br, 1.0 + 0.018 * br, 1.0);
    let k = t * 0.32 + id;
    let (i, f) = (k.floor(), k.fract());
    let dir = |n: f32| ((n * 12.9898 + id).sin() * 43758.547).fract() * 2.0 - 1.0;
    let look = dir(i) + (dir(i + 1.0) - dir(i)) * smooth(0.7, 1.0, f);
    r.parts[1].1 = ry(0.55 * look) * rx(0.05 * (t * 1.8).sin() - 0.05);
    r.parts[2].1 = rx(0.1) * ry(0.15 * (t * 0.9 + id).sin());
    let c = (t + id * 3.1).rem_euclid(IDLE_LOOP);
    let sniff = window(c, 3.0, 5.2, 0.35);
    if sniff > 0.0 {
        let mut s = r;
        s.parts[1].1 = ry(0.2 * look) * rx(-0.32 + 0.06 * (t * 24.0).sin());
        s.parts[0].1 = rx(-0.05);
        r = r.lerp(&s, sniff);
    }
    let sit = window(c, 7.0, 11.8, 0.5);
    if sit > 0.0 {
        let mut s = Rig::rest();
        let lean = -0.8;
        s.pivot_body(Vec3::new(0.0, 0.12, -0.42), rx(lean));
        s.parts[0].0.y += 0.04 + 0.01 * br;
        let g = (t * 11.0 + id).sin();
        s.parts[1].1 = rx(0.55 + 0.08 * g) * ry(0.12 * (t * 3.0).sin());
        s.parts[2].1 = rx(-lean - 0.12);
        s.parts[3].1 = rx(-1.95 + 0.25 * g);
        s.parts[4].1 = rx(-1.95 - 0.25 * g);
        s.parts[5].1 = rx(-lean - 0.35);
        s.parts[6].1 = rx(-lean - 0.35);
        s.squash = Vec3::new(0.98, 1.03, 0.98);
        r = r.lerp(&s, sit);
    }
    r
}

fn pose_rig(pose: Pose, t: f32, id: f32, gait: f32) -> Rig {
    let rx = Quat::from_rotation_x;
    let ry = Quat::from_rotation_y;
    let rz = Quat::from_rotation_z;
    let mut r = Rig::rest();
    let ph = gait;
    match pose {
        Pose::Idle => r = idle_rig(t, id),
        Pose::Walk | Pose::Carry => {
            let (s, c2) = (ph.sin(), (2.0 * ph).cos());
            r.parts[0].0.y = 0.06 * (2.0 * ph).sin().abs();
            r.parts[0].1 = rz(0.08 * s) * rx(0.03 * (2.0 * ph).sin());
            r.squash = Vec3::new(1.0 + 0.025 * c2, 1.0 - 0.04 * c2, 1.0 + 0.015 * c2);
            r.parts[1].1 = rx(0.07 * (2.0 * ph + 1.0).sin()) * rz(-0.06 * s);
            r.parts[2].1 = rx(0.12 + 0.06 * (2.0 * ph + 0.8).sin()) * ry(0.35 * (ph * 0.5).sin() + 0.15 * s);
            r.parts[3].1 = rx(0.75 * s);
            r.parts[4].1 = rx(-0.75 * s);
            r.parts[5].1 = rx(-0.8 * (ph - 0.3).sin());
            r.parts[6].1 = rx(0.8 * (ph - 0.3).sin());
            if pose == Pose::Carry {
                r.parts[1].1 = rx(-0.3 + 0.05 * (2.0 * ph).sin());
                r.parts[3].1 = rx(-1.0 + 0.1 * s);
                r.parts[4].1 = rx(-1.0 - 0.1 * s);
                r.parts[0].1 = rz(0.05 * s) * rx(-0.06);
                r.squash = Vec3::new(1.0 + 0.035 * c2, 1.0 - 0.05 * c2, 1.0);
            }
        }
        Pose::Gnaw => {
            let chew = (t * 15.0 + id).sin();
            r.parts[0].1 = rx(-0.5) * rz(0.05 * (t * 7.5).sin());
            r.parts[0].0.y = 0.1 + 0.015 * chew;
            r.squash = Vec3::new(1.0, 1.0 + 0.02 * chew, 1.0);
            r.parts[1].1 = rx(0.45 + 0.17 * chew) * rz(0.12 * (t * 7.5 + id).sin()) * ry(0.06 * (t * 7.0).sin());
            r.parts[2].1 = rx(0.55) * ry(0.08 * (t * 2.0 + id).sin());
            r.parts[3].1 = rx(-1.25 + 0.08 * (t * 15.0).sin());
            r.parts[4].1 = rx(-1.25 - 0.08 * (t * 15.0).sin());
            r.parts[5].1 = rx(-0.55);
            r.parts[6].1 = rx(-0.55);
        }
        Pose::Climb => {
            let s = gait.sin();
            let c2 = (2.0 * gait).cos();
            r.parts[0].0.y = 0.1 + 0.04 * (2.0 * gait).sin().abs();
            r.parts[0].1 = rx(-0.38) * rz(0.06 * s);
            r.squash = Vec3::new(1.0, 1.0 - 0.03 * c2, 1.0 + 0.03 * c2);
            r.parts[1].1 = rx(-0.1 + 0.08 * s);
            r.parts[2].1 = rx(0.45) * ry(0.2 * s);
            r.parts[3].1 = rx(-1.45 + 0.5 * s);
            r.parts[4].1 = rx(-1.45 - 0.5 * s);
            r.parts[5].1 = rx(0.5 - 0.6 * s);
            r.parts[6].1 = rx(0.5 + 0.6 * s);
        }
        Pose::Work => {
            let s = (t * 8.0 + id).sin();
            r.parts[0].0.y = 0.025 * s.abs();
            r.parts[0].1 = rx(0.1) * rz(0.04 * s);
            r.squash = Vec3::new(1.0 + 0.02 * s.abs(), 1.0 - 0.03 * s.abs(), 1.0);
            r.parts[1].1 = rx(0.35 + 0.08 * (t * 4.0 + id).sin()) * ry(0.1 * (t * 2.0).sin());
            r.parts[2].1 = rx(0.35) * ry(0.1 * (t * 1.3 + id).sin());
            r.parts[3].1 = rx(-0.9 + 0.5 * s);
            r.parts[4].1 = rx(-0.9 - 0.5 * s);
            r.parts[5].1 = rx(-0.2);
            r.parts[6].1 = rx(-0.2);
        }
        Pose::Rest => {
            let b = (t * 1.5 + id).sin();
            r.parts[0].0.y = -0.2 + 0.012 * b;
            r.parts[0].1 = rz(0.04);
            r.squash = Vec3::new(1.0 + 0.025 * b, 1.0 + 0.035 * b, 1.0);
            r.parts[1].0.y -= 0.08;
            r.parts[1].1 = rx(0.6) * ry(0.5 + 0.05 * b);
            r.parts[2].1 = rx(0.2) * ry(1.1);
            r.parts[3].1 = rx(-0.5);
            r.parts[4].1 = rx(-0.5);
            r.parts[5].1 = rx(-0.5);
            r.parts[6].1 = rx(-0.5);
        }
        Pose::Swim => {
            let p2 = gait + t * 3.0;
            r.parts[0].0.y = -0.34 + 0.02 * (t * 2.5 + id).sin();
            r.parts[0].1 = rx(-0.06 + 0.025 * (2.0 * p2).sin());
            r.squash = Vec3::new(1.0, 1.0, 1.0 + 0.025 * (2.0 * p2).sin());
            r.parts[1].1 = rx(-0.16 + 0.04 * (2.0 * p2 + 0.6).sin());
            r.parts[2].1 = rx(0.05 + 0.2 * (p2 - 1.2).sin()) * ry(0.3 * (t * 1.4 + id).sin());
            r.parts[3].1 = rx(0.9);
            r.parts[4].1 = rx(0.9);
            r.parts[5].1 = rx(1.0 * p2.sin());
            r.parts[6].1 = rx(-1.0 * p2.sin());
        }
    }
    r
}

fn chip_flight(t: f32, id: f32) -> Transform {
    let k = t * 2.3 + id;
    let (u, side) = (k.rem_euclid(1.0), if k.floor().rem_euclid(2.0) < 1.0 { 1.0 } else { -1.0 });
    let p = Vec3::new(side * 0.55 * u, -0.28 + 0.75 * u - 1.1 * u * u, 0.72 + 0.25 * u);
    Transform::from_translation(p)
        .with_rotation(Quat::from_rotation_z(side * u * 5.0) * Quat::from_rotation_x(u * 3.0))
        .with_scale(Vec3::splat((1.0 - u * u).max(0.05)))
}

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
    let turn = 1.0 - (-dt * 8.0).exp();
    for (mut tr, mut rig, sprite) in &mut roots {
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
        let moved = rig.last_pos.map_or(0.0, |p| (tr.translation - p).xz().length());
        rig.last_pos = Some(tr.translation);
        if moved < SNAP_JUMP {
            rig.stride += (moved / STRIDE * std::f32::consts::TAU).min(MAX_STRIDE_HZ * std::f32::consts::TAU * dt.max(1e-4));
        }
        let a = pose_rig(rig.cur, clock.0, id, rig.stride);
        let blend = rig.blend * rig.blend * (3.0 - 2.0 * rig.blend);
        let r = if blend < 1.0 { pose_rig(rig.prev, clock.0, id, rig.stride).lerp(&a, blend) } else { a };
        let ents = [rig.body, rig.parts[0], rig.parts[1], rig.parts[2], rig.parts[3], rig.parts[4], rig.parts[5]];
        for (k, (e, (pos, rot))) in ents.iter().zip(r.parts.iter()).enumerate() {
            if let Ok(mut t) = parts.get_mut(*e) {
                t.translation = *pos;
                t.rotation = *rot;
                if k == 0 {
                    t.scale = r.squash;
                }
            }
        }
        let shut = rig.cur == Pose::Rest || blinking(clock.0, id);
        if let Ok(mut v) = vis.get_mut(rig.lids) {
            v.set_if_neq(if shut { Visibility::Inherited } else { Visibility::Hidden });
        }
        let gnawing = rig.cur == Pose::Gnaw;
        if let Ok(mut v) = vis.get_mut(rig.chips) {
            v.set_if_neq(if gnawing { Visibility::Inherited } else { Visibility::Hidden });
        }
        if gnawing {
            if let Ok(mut t) = parts.get_mut(rig.chips) {
                *t = chip_flight(clock.0, id);
            }
        }
    }
}

fn update_tree_lod(
    season: Res<crate::terrain_render::Season>,
    canopy: Res<CanopyView>,
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
        let offset = (t.translation.x * 0.754_877_7 + t.translation.z * 0.569_840_3).rem_euclid(1.0) - 0.5;
        let tree_season = Season::at(season.phase + offset / Season::COUNT as f32);
        if want != lod.lod || lod.season != tree_season || lod.cutaway != canopy.0 {
            lod.lod = want;
            lod.season = tree_season;
            lod.cutaway = canopy.0;
            mesh.0 = if canopy.0 { models.cutaway_trees[species_index(lod.species)].clone() }
                else { models.tree(lod.species, tree_season, want) };
        }
    }
}

fn sway_trees(clock: Res<RenderClock>, mut trees: Query<(&mut Transform, &TreeLod, &Sway)>) {
    let t = clock.0;
    let gust = 1.0 + 0.45 * (t * 0.23).sin();
    for (mut tr, lod, base) in &mut trees {
        let want = if lod.lod == 0 {
            let p = tr.translation;
            let ph = p.x * 0.21 + p.z * 0.13;
            let a = 0.016 * gust * ((t * 1.1 + ph).sin() + 0.35 * (t * 2.6 + ph * 1.7).sin());
            let b = 0.008 * gust * (t * 0.9 + ph * 1.3).cos();
            Quat::from_rotation_z(a) * Quat::from_rotation_x(b) * base.0
        } else {
            base.0
        };
        if tr.rotation != want {
            tr.rotation = want;
        }
    }
}

fn spin_wheels(time: Res<Time>, mut q: Query<(&mut Transform, &WheelSpin)>) {
    for (mut t, w) in &mut q {
        t.rotation = Quat::from_rotation_x(time.elapsed_secs() * w.0);
    }
}

