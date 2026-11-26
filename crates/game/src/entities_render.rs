mod deposits;
mod effects;
mod gallery;
mod islands;
mod meshgen;
mod models;
mod plants;
pub mod portrait;
pub(crate) mod raft_landing;
mod scene;
pub mod sim_sync;
pub mod sim_view;
mod structures;
mod trees;
mod wildlife;
mod wolf;

use crate::camera::MainCamera;
use crate::cli::Cli;
use crate::seasonal::smooth;
use bevy::prelude::*;
use meshgen::build_mesh;
use models::Season;
use scene::*;
use std::collections::HashMap;

pub use scene::lodge_view;
pub use wolf::{AlarmSlaps, WolfDrawn};

const BEAVER_NEAR: f32 = 260.0;
const TREE_LOD0: f32 = 220.0;
const TREE_LOD1: f32 = 480.0;
const SCREENSHOT_CLOCK: f32 = 31.0;
const LOAD_LOOK: f32 = 1.5;
const SNAP_JUMP: f32 = 3.0;
const STRIDE: f32 = 0.42;
const BOUND_FROM: f32 = 1.3;
const BOUND_FULL: f32 = 2.8;
const MAX_STRIDE_HZ: f32 = 5.5;
const TAIL_DOWN: f32 = -0.26;
const HIPS: Vec3 = Vec3::new(0.0, 0.12, -0.42);
const IDLE_LOOP: f32 = 14.0;
const SWIM_DEPTH: f32 = -0.68;
const SLAP_S: f32 = 0.6;
const TOPPLE_S: f32 = 1.5;
pub const ENTITY_GAIN: f32 = 0.55;

#[derive(Component)]
pub struct SetDown { pub until: f32 }

pub struct EntitiesRenderPlugin;

impl Plugin for EntitiesRenderPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<RenderClock>()
            .init_resource::<TailSlapTimes>()
            .init_resource::<effects::PuffMesh>()
            .init_resource::<CanopyView>()
            .init_resource::<BeaverDrawn>()
            .add_plugins((wolf::WolfPlugin, wildlife::WildlifePlugin, portrait::PortraitPlugin))
            .add_systems(Startup, (init_models, raft_landing::init_raft_landing_meshes))
            .add_systems(Update, gallery::show_welcome)
            .add_systems(
                Update,
                (
                    update_clock, sim_sync::sync_sim_beavers, sim_sync::sync_bodies,
                    sim_sync::sync_sim_resources, sim_sync::sync_sim_buildings, sim_sync::sync_sim_dams,
                    sim_sync::sync_dam_ghosts, sim_sync::sync_ground_goods,
                    gallery::spawn_gallery, gallery::shuffle_gallery, gallery::select_home, gallery::select_debug,
                    attach_rigs, animate_beavers, canopy_key, update_tree_lod, sway_trees, topple_trees,
                ).chain().after(crate::terrain_render::update_season),
            )
            .add_systems(Update, (effects::grow, effects::puffs, effects::falls).chain().after(topple_trees))
            .add_systems(Update, update_plant_season.after(update_tree_lod))
            .add_systems(Update, (raft_landing::sync_locks, raft_landing::sync_rafts, raft_landing::animate_locks).chain().after(sim_sync::sync_sim_dams));
    }
}

#[derive(Resource, Default)]
struct RenderClock(f32);

#[derive(Resource, Default)]
pub struct BeaverDrawn(pub HashMap<u64, Vec3>);

impl BeaverDrawn {
    pub fn at(&self, id: sim::colony::BeaverId) -> Option<Vec3> { self.0.get(&id.0).copied() }
}

#[derive(Resource, Default)]
pub(crate) struct TailSlapTimes(pub HashMap<u64, f32>);

#[derive(Resource)]
pub(crate) struct CanopyView(pub bool);
impl Default for CanopyView { fn default() -> Self { Self(std::env::var_os("BC_CUTAWAY").is_some()) } }

fn canopy_key(keys: Res<ButtonInput<KeyCode>>, menu: Option<Res<crate::hud::MenuOpen>>, mut view: ResMut<CanopyView>) {
    if !menu.is_some_and(|m| m.0) && keys.just_pressed(KeyCode::F8) { view.0 = !view.0; }
}

#[derive(Resource)]
struct Models {
    white: Handle<StandardMaterial>,
    tree_tints: [Handle<StandardMaterial>; 4],
    old_tree: Handle<StandardMaterial>,
    blob_mat: Handle<StandardMaterial>,
    wake_mat: Handle<StandardMaterial>,
    wear: [Handle<StandardMaterial>; 3],
    rot: [Handle<StandardMaterial>; 2],
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
    grey: Handle<Mesh>,
    loads: Vec<Handle<Mesh>>,
    paw_loads: Vec<Handle<Mesh>>,
    heaps: Vec<Handle<Mesh>>,
    supplies: Handle<Mesh>,
    felled: Vec<Handle<Mesh>>,
    plants: Vec<Handle<Mesh>>,
    pebble_heaps: Vec<Handle<Mesh>>,
    gravel_bars: Vec<Handle<Mesh>>,
    boulders: Vec<Handle<Mesh>>,
    mud_heap: Handle<Mesh>,
    river_mud: Handle<Mesh>,
    wet: Handle<StandardMaterial>,
    far: Handle<Mesh>,
    wake: Handle<Mesh>,
    blob: Handle<Mesh>,
    structs: HashMap<(sim::buildings::BuildingKind, u8), Handle<Mesh>>,
    reinforcements: HashMap<(sim::buildings::BuildingKind, bool), Handle<Mesh>>,
    dams: HashMap<(u8, u32, u32, structures::DamStage, bool), Handle<Mesh>>,
}

impl Models {
    fn plant(&self, p: sim::plants::Plant, season: Season) -> Handle<Mesh> { self.plants[p.index() * Season::COUNT + season.index()].clone() }
    fn tree(&self, sp: sim::trees::Tree, season: Season, lod: u8) -> Handle<Mesh> { self.trees[(sp.index() * Season::COUNT + season.index()) * 3 + lod as usize].clone() }
}

#[derive(Component)]
struct TreeLod { species: sim::trees::Tree, lod: u8, season: Season, cutaway: bool }

#[derive(Component)]
struct PlantLook { plant: sim::plants::Plant, season: Season }

#[derive(Component)]
struct Sway(Quat);
#[derive(Component)]
struct SimBeaver;

#[derive(Component, Clone, Debug)]
pub struct BeaverSprite {
    pub id: u64,
    pub pose: Pose,
    pub load: Option<(sim::buildings::Good, f32)>,
    pub yaw: f32,
    pub wet: bool,
    pub look: Look,
    pub features: sim::looks::Looks,
}

pub fn looks_size(l: &sim::looks::Looks) -> f32 { 1.0 + 0.06 * l.size }

pub fn fur_tint(l: &sim::looks::Looks) -> Color {
    let b = 1.0 + 0.16 * l.shade;
    Color::LinearRgba(LinearRgba::rgb(b * (1.0 + 0.1 * l.warmth), b * (1.0 + 0.01 * l.warmth), b * (1.0 - 0.12 * l.warmth)))
}

pub fn looks_shape(l: &sim::looks::Looks) -> (Vec3, Vec3, Vec3) {
    (Vec3::new(1.0 + 0.08 * l.build, 1.0 + 0.06 * l.build, 1.0 - 0.02 * l.build), Vec3::splat(1.0 + 0.07 * l.head), Vec3::new(1.0 + 0.14 * l.tail, 1.0, 1.0 + 0.1 * l.tail))
}

#[derive(Clone, Copy, Debug)]
pub struct Look {
    pub thin: f32, pub cold: f32, pub hurt: f32, pub tired: f32, pub old: f32, pub sad: f32,
    pub belly: f32, pub slap: f32, pub dead: Option<(f32, bool)>, pub fade: f32, pub sink: f32,
}

impl Default for Look {
    fn default() -> Self {
        Self { thin: 0.0, cold: 0.0, hurt: 0.0, tired: 0.0, old: 0.0, sad: 0.0, belly: 0.0, slap: -100.0, dead: None, fade: 0.0, sink: 0.0 }
    }
}

#[derive(Component)]
pub(crate) struct BeaverRig {
    body: Entity, body_mesh: Entity, far: Entity, parts: [Entity; 6],
    mouth: Entity, paws: Entity, lids: Entity, chips: Entity, grey: Entity, wake: Entity, blob: Entity,
    prev: Pose, cur: Pose, blend: f32, near: bool, init: bool,
    shown: (Option<sim::buildings::Good>, Option<sim::buildings::Good>),
    stride: f32, speed: f32, last_pos: Option<Vec3>,
}

pub fn age_scale_days(age_days: f32) -> f32 {
    use sim::colony::{ADULT_DAYS, KIT_DAYS};
    if age_days < KIT_DAYS { 0.55 + 0.19 * (age_days / KIT_DAYS).clamp(0.0, 1.0) }
    else { 0.74 + 0.26 * ((age_days - KIT_DAYS) / (ADULT_DAYS - KIT_DAYS)).clamp(0.0, 1.0) }
}

fn init_models(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>, mut materials: ResMut<Assets<StandardMaterial>>) {
    let tint = |m: &mut Assets<StandardMaterial>, r: f32, g: f32, b: f32| {
        m.add(StandardMaterial { base_color: Color::linear_rgb(ENTITY_GAIN * r, ENTITY_GAIN * g, ENTITY_GAIN * b), perceptual_roughness: 1.0, reflectance: 0.0, ..default() })
    };
    let blend = |m: &mut Assets<StandardMaterial>, c: Color| m.add(StandardMaterial { base_color: c, alpha_mode: AlphaMode::Blend, unlit: true, ..default() });
    let white = tint(&mut materials, 1.0, 1.0, 1.0);
    let tree_tints = [[1.0, 1.0, 1.0], [0.86, 0.96, 0.84], [1.05, 0.95, 0.80], [0.80, 0.88, 0.86]].map(|[r, g, b]| tint(&mut materials, r, g, b));
    let old_tree = tint(&mut materials, 0.86, 0.90, 0.82);
    let blob_mat = blend(&mut materials, Color::srgba(0.42, 0.353, 0.541, 0.40));
    let wake_mat = blend(&mut materials, Color::srgba(0.95, 1.0, 0.98, 0.55));
    let wear = [[0.86, 0.84, 0.8], [0.72, 0.7, 0.66], [0.58, 0.56, 0.54]].map(|[r, g, b]| tint(&mut materials, r, g, b));
    let rot = [[0.84, 0.86, 0.66], [0.56, 0.6, 0.42]].map(|[r, g, b]| tint(&mut materials, r, g, b));
    let wet = materials.add(StandardMaterial { base_color: Color::linear_rgb(ENTITY_GAIN, ENTITY_GAIN, ENTITY_GAIN), perceptual_roughness: 0.32, reflectance: 0.3, ..default() });
    let trees = sim::trees::Tree::all().flat_map(|sp| Season::all().flat_map(move |s| (0..3u8).map(move |l| (sp, s, l)))).map(|(sp, s, l)| meshes.add(trees::tree_mesh(sp, s, l))).collect();
    commands.insert_resource(Models {
        white, tree_tints, old_tree, blob_mat, wake_mat, wear, rot, wet, trees,
        cutaway_trees: sim::trees::Tree::all().map(|sp| meshes.add(trees::tree_cutaway_mesh(sp))).collect(),
        snag: meshes.add(trees::snag_mesh()),
        body: build_mesh(&mut meshes, |m| models::beaver_body(m, Vec3::ZERO)),
        head: build_mesh(&mut meshes, |m| models::beaver_head(m, Vec3::ZERO)),
        tail: build_mesh(&mut meshes, |m| models::beaver_tail(m, Vec3::ZERO)),
        leg_front: build_mesh(&mut meshes, |m| models::beaver_leg(m, true)),
        leg_hind: build_mesh(&mut meshes, |m| models::beaver_leg(m, false)),
        lids: build_mesh(&mut meshes, models::beaver_lids),
        chips: build_mesh(&mut meshes, models::gnaw_chips),
        grey: build_mesh(&mut meshes, models::beaver_grey),
        far: build_mesh(&mut meshes, models::beaver_static),
        wake: build_mesh(&mut meshes, models::wake),
        blob: build_mesh(&mut meshes, models::blob_shadow),
        loads: sim::buildings::Good::all().map(|g| meshes.add(models::load_mesh(g))).collect(),
        paw_loads: sim::buildings::Good::all().map(|g| meshes.add(models::paw_load_mesh(g))).collect(),
        heaps: sim::buildings::Good::all().map(|g| meshes.add(models::heap_mesh(g))).collect(),
        supplies: meshes.add(models::supplies_mesh()),
        felled: (0..4u8).flat_map(|l| [false, true].map(|b| meshes.add(models::felled_mesh(l, b)))).collect(),
        plants: sim::plants::Plant::all().flat_map(|p| Season::all().map(move |s| (p, s))).map(|(p, s)| meshes.add(plants::plant_mesh(p, s))).collect(),
        pebble_heaps: (1..=4).map(|q| meshes.add(deposits::pebble_heap_mesh(q))).collect(),
        gravel_bars: (1..=4).map(|q| meshes.add(deposits::gravel_bar_mesh(q))).collect(),
        boulders: (0..deposits::BOULDER_VARIANTS).map(|v| meshes.add(deposits::boulder_mesh(v))).collect(),
        mud_heap: meshes.add(deposits::mud_heap_mesh()),
        river_mud: meshes.add(deposits::river_mud_mesh()),
        structs: HashMap::new(),
        reinforcements: HashMap::new(),
        dams: HashMap::new(),
    });
}

fn update_clock(time: Res<Time>, cli: Res<Cli>, mut clock: ResMut<RenderClock>) {
    let frozen = cli.screenshot.is_some() && std::env::var_os("BC_BURST").is_none() && std::env::var_os("BC_GALLERY").is_none();
    clock.0 = if frozen { std::env::var("BC_CLOCK").ok().and_then(|s| s.parse().ok()).unwrap_or(SCREENSHOT_CLOCK) } else { time.elapsed_secs() };
}

fn attach_rigs(mut commands: Commands, models: Option<Res<Models>>, mut mats: ResMut<Assets<StandardMaterial>>, q: Query<(Entity, &BeaverSprite), Without<BeaverRig>>) {
    let Some(m) = models else { return };
    let plain = mats.get(&m.white).cloned().unwrap_or_default();
    for (root, sprite) in &q {
        let w = mats.add(StandardMaterial { base_color: fur_tint(&sprite.features), ..plain.clone() });
        let target = crate::picking::Target::Beaver(sim::colony::BeaverId(sprite.id));
        let body = commands.spawn((Transform::default(), Visibility::Inherited, ChildOf(root))).id();
        let body_mesh = commands.spawn((target, Mesh3d(m.body.clone()), MeshMaterial3d(w.clone()), Transform::default(), ChildOf(body))).id();
        let head = commands.spawn((target, Mesh3d(m.head.clone()), MeshMaterial3d(w.clone()), Transform::from_translation(models::HEAD_PIVOT), ChildOf(body))).id();
        let tail = commands.spawn((target, Mesh3d(m.tail.clone()), MeshMaterial3d(w.clone()), Transform::from_translation(models::TAIL_PIVOT), ChildOf(body))).id();
        let legs: [Entity; 4] = std::array::from_fn(|k| {
            let mesh = if k < 2 { m.leg_front.clone() } else { m.leg_hind.clone() };
            commands.spawn((target, Mesh3d(mesh), MeshMaterial3d(w.clone()), Transform::from_translation(models::LEG_PIVOTS[k]), ChildOf(body))).id()
        });
        let paws = commands.spawn((Mesh3d(m.paw_loads[0].clone()), MeshMaterial3d(m.white.clone()), Transform::from_translation(models::CHEST_HOLD), Visibility::Hidden, ChildOf(body))).id();
        let mouth = commands.spawn((Mesh3d(m.loads[0].clone()), MeshMaterial3d(m.white.clone()), Transform::from_translation(models::MOUTH), Visibility::Hidden, ChildOf(head))).id();
        let lids = commands.spawn((Mesh3d(m.lids.clone()), MeshMaterial3d(w.clone()), Transform::default(), Visibility::Hidden, ChildOf(head))).id();
        let chips = commands.spawn((Mesh3d(m.chips.clone()), MeshMaterial3d(m.white.clone()), Transform::default(), Visibility::Hidden, ChildOf(head))).id();
        let grey = commands.spawn((Mesh3d(m.grey.clone()), MeshMaterial3d(w.clone()), Transform::default(), Visibility::Hidden, ChildOf(head))).id();
        let far = commands.spawn((target, Mesh3d(m.far.clone()), MeshMaterial3d(w), Transform::default(), Visibility::Hidden, ChildOf(root))).id();
        let wake = commands.spawn((Mesh3d(m.wake.clone()), MeshMaterial3d(m.wake_mat.clone()), Transform::from_xyz(0.0, 0.04, 1.15), Visibility::Hidden, ChildOf(root))).id();
        let blob = commands.spawn((Mesh3d(m.blob.clone()), MeshMaterial3d(m.blob_mat.clone()), Transform::from_xyz(0.0, 0.05, 0.0), ChildOf(root))).id();
        commands.entity(root).insert(BeaverRig {
            body, body_mesh, far, parts: [head, tail, legs[0], legs[1], legs[2], legs[3]],
            mouth, paws, lids, chips, grey, wake, blob,
            prev: sprite.pose, cur: sprite.pose, blend: 1.0, near: true, init: false, shown: (None, None),
            stride: sprite.id as f32 * 1.7, speed: 0.0, last_pos: None,
        });
    }
}

#[derive(Clone, Copy)]
struct Rig { parts: [(Vec3, Quat); 7], squash: Vec3, hold: Vec3 }

impl Rig {
    fn rest() -> Rig {
        Rig {
            parts: [(Vec3::ZERO, Quat::IDENTITY), (models::HEAD_PIVOT, Quat::IDENTITY), (models::TAIL_PIVOT, Quat::from_rotation_x(TAIL_DOWN)),
                (models::LEG_PIVOTS[0], Quat::IDENTITY), (models::LEG_PIVOTS[1], Quat::IDENTITY), (models::LEG_PIVOTS[2], Quat::IDENTITY), (models::LEG_PIVOTS[3], Quat::IDENTITY)],
            squash: Vec3::ONE, hold: models::CHEST_HOLD,
        }
    }
    fn lerp(&self, o: &Rig, w: f32) -> Rig {
        Rig {
            parts: std::array::from_fn(|k| (self.parts[k].0.lerp(o.parts[k].0, w), self.parts[k].1.slerp(o.parts[k].1, w))),
            squash: self.squash.lerp(o.squash, w),
            hold: self.hold.lerp(o.hold, w),
        }
    }
    fn pivot_body(&mut self, pivot: Vec3, rot: Quat) { self.parts[0] = (self.parts[0].0 + pivot - rot * pivot, rot * self.parts[0].1); }
    fn paw_tip(a: f32) -> Vec3 { let p = models::LEG_PIVOTS[1]; Vec3::new(0.0, p.y, p.z) + Quat::from_rotation_x(a) * Vec3::new(0.0, -0.3, 0.1) }
}

fn window(x: f32, a: f32, b: f32, ramp: f32) -> f32 { smooth(a, a + ramp, x) * (1.0 - smooth(b - ramp, b, x)) }

pub(crate) fn blinking(t: f32, id: f32) -> bool { (t * 0.29 + id * 0.37).rem_euclid(1.0) < 0.04 }

fn sit_rig(lean: f32, br: f32) -> Rig {
    let rx = Quat::from_rotation_x;
    let mut s = Rig::rest();
    s.pivot_body(HIPS, rx(lean));
    s.parts[0].0.y += 0.04 + 0.01 * br;
    s.parts[2].1 = rx(-lean + TAIL_DOWN + 0.1);
    (s.parts[5].1, s.parts[6].1) = (rx(-lean - 0.35), rx(-lean - 0.35));
    s.squash = Vec3::new(0.98, 1.03, 0.98);
    s
}

fn idle_rig(t: f32, id: f32) -> Rig {
    let (rx, ry) = (Quat::from_rotation_x, Quat::from_rotation_y);
    let mut r = Rig::rest();
    let br = (t * 1.8 + id).sin();
    r.parts[0].0.y = 0.015 * br;
    r.squash = Vec3::new(1.0 + 0.012 * br, 1.0 + 0.018 * br, 1.0);
    let k = t * 0.32 + id;
    let dir = |n: f32| ((n * 12.9898 + id).sin() * 43758.547).fract() * 2.0 - 1.0;
    let look = dir(k.floor()) + (dir(k.floor() + 1.0) - dir(k.floor())) * smooth(0.7, 1.0, k.fract());
    r.parts[1].1 = ry(0.55 * look) * rx(0.05 * (t * 1.8).sin() - 0.05);
    r.parts[2].1 = rx(TAIL_DOWN) * ry(0.15 * (t * 0.9 + id).sin());
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
        let mut s = sit_rig(-0.8, br);
        let g = (t * 11.0 + id).sin();
        s.parts[1].1 = rx(0.55 + 0.08 * g) * ry(0.12 * (t * 3.0).sin());
        (s.parts[3].1, s.parts[4].1) = (rx(-1.95 + 0.25 * g), rx(-1.95 - 0.25 * g));
        r = r.lerp(&s, sit);
    }
    r
}

#[derive(Clone, Copy)]
struct Gait { phase: f32, run: f32, hug: bool }

fn travel_rig(g: Gait, lift_head: f32) -> Rig {
    let (rx, ry, rz) = (Quat::from_rotation_x, Quat::from_rotation_y, Quat::from_rotation_z);
    let (mut r, ph, run) = (Rig::rest(), g.phase, g.run);
    let (s, c2, hs) = (ph.sin(), (2.0 * ph).cos(), (ph - 0.3).sin());
    let (bound_f, bound_h, amp) = ((ph + 0.4).sin(), (ph - 2.3).sin(), 0.75 + 0.3 * run);
    let (trot_bob, bound_bob) = (0.05 * (2.0 * ph).sin().abs(), 0.14 * (ph + 0.9).sin().max(0.0));
    let flex = 0.16 * (ph - 0.6).sin() * run;
    r.parts[0].0.y = trot_bob + (bound_bob - trot_bob) * run;
    r.parts[0].1 = rz(0.08 * s * (1.0 - run)) * rx(0.03 * (2.0 * ph).sin() + flex);
    r.squash = Vec3::new(1.0 + 0.025 * c2, 1.0 - 0.04 * c2, 1.0 + 0.015 * c2 + 0.06 * run * (ph - 0.6).sin());
    r.parts[1].1 = rx(-lift_head + 0.07 * (2.0 * ph + 1.0).sin() - flex) * rz(-0.06 * s * (1.0 - run));
    r.parts[2].1 = rx(TAIL_DOWN + 0.2 * run + 0.05 * (2.0 * ph + 0.8).sin() - flex) * ry(0.12 * s * (1.0 - run));
    r.parts[3].1 = rx(amp * (s + (bound_f - s) * run));
    r.parts[4].1 = rx(amp * (-s + (bound_f + s) * run));
    r.parts[5].1 = rx(amp * 1.05 * (-hs + (bound_h + hs) * run));
    r.parts[6].1 = rx(amp * 1.05 * (hs + (bound_h - hs) * run));
    r
}

fn hug_walk_rig(ph: f32) -> Rig {
    let (rx, ry, rz) = (Quat::from_rotation_x, Quat::from_rotation_y, Quat::from_rotation_z);
    let (lean, s, hug) = (-0.75, ph.sin(), -0.85);
    let mut r = sit_rig(lean, 0.0);
    r.parts[0].0.y += 0.035 * (2.0 * ph).sin().abs();
    r.parts[0].1 = rz(0.1 * s) * r.parts[0].1;
    r.parts[1].1 = rx(0.45 + 0.05 * (2.0 * ph).sin()) * rz(-0.08 * s);
    r.parts[2].1 = rx(-lean + TAIL_DOWN + 0.1) * ry(0.18 * s);
    (r.parts[3].1, r.parts[4].1) = (rx(hug + 0.05 * s), rx(hug - 0.05 * s));
    (r.parts[5].1, r.parts[6].1) = (rx(-lean - 0.15 + 0.55 * s), rx(-lean - 0.15 - 0.55 * s));
    r.hold = Rig::paw_tip(hug) + Vec3::new(0.0, 0.08, 0.0);
    r
}

fn pose_rig(pose: Pose, t: f32, id: f32, g: Gait, look: &Look) -> Rig {
    let (rx, ry, rz) = (Quat::from_rotation_x, Quat::from_rotation_y, Quat::from_rotation_z);
    let mut r = Rig::rest();
    match pose {
        Pose::Idle if g.hug => {
            r = hug_walk_rig(0.0);
            let br = (t * 1.8 + id).sin();
            r.parts[0].0.y += 0.012 * br;
            r.squash = Vec3::new(1.0 + 0.012 * br, 1.0 + 0.018 * br, 1.0);
            r.parts[1].1 = ry(0.4 * (t * 0.35 + id).sin()) * r.parts[1].1;
        }
        Pose::Idle => r = idle_rig(t, id),
        Pose::Walk => r = travel_rig(g, 0.0),
        Pose::Carry if g.hug => r = hug_walk_rig(g.phase),
        Pose::Carry => r = travel_rig(Gait { run: g.run * 0.5, ..g }, 0.25),
        Pose::Run => {
            r = travel_rig(Gait { run: 1.0, ..g }, -0.15);
            r.parts[2].1 = rx(TAIL_DOWN + 0.55 + 0.1 * (g.phase - 0.6).sin());
        }
        Pose::Gnaw => {
            let chew = (t * 15.0 + id).sin();
            r.pivot_body(HIPS, rx(-0.5) * rz(0.05 * (t * 7.5).sin()));
            r.parts[0].0.y += 0.02 + 0.015 * chew;
            r.squash = Vec3::new(1.0, 1.0 + 0.02 * chew, 1.0);
            r.parts[1].1 = rx(0.45 + 0.17 * chew) * rz(0.12 * (t * 7.5 + id).sin()) * ry(0.06 * (t * 7.0).sin());
            r.parts[2].1 = rx(0.5 + TAIL_DOWN) * ry(0.08 * (t * 2.0 + id).sin());
            (r.parts[3].1, r.parts[4].1) = (rx(-1.25 + 0.08 * (t * 15.0).sin()), rx(-1.25 - 0.08 * (t * 15.0).sin()));
            (r.parts[5].1, r.parts[6].1) = (rx(0.35), rx(0.35));
        }
        Pose::Climb => {
            let (s, c2) = (g.phase.sin(), (2.0 * g.phase).cos());
            r.pivot_body(HIPS, rx(-0.38) * rz(0.06 * s));
            r.parts[0].0.y += 0.04 * (2.0 * g.phase).sin().abs();
            r.squash = Vec3::new(1.0, 1.0 - 0.03 * c2, 1.0 + 0.03 * c2);
            r.parts[1].1 = rx(0.2 + 0.08 * s);
            r.parts[2].1 = rx(0.38 + TAIL_DOWN) * ry(0.2 * s);
            (r.parts[3].1, r.parts[4].1, r.parts[5].1, r.parts[6].1) = (rx(-1.0 + 0.6 * s), rx(-1.0 - 0.6 * s), rx(0.38 - 0.6 * s), rx(0.38 + 0.6 * s));
        }
        Pose::Work => {
            let k = (t * 3.2 + id).rem_euclid(1.0);
            let press = smooth(0.0, 0.35, k) * (1.0 - smooth(0.55, 1.0, k));
            r.pivot_body(HIPS, rx(0.08 + 0.1 * press));
            r.parts[0].0.y += 0.02 * (1.0 - press);
            r.squash = Vec3::new(1.0 + 0.03 * press, 1.0 - 0.04 * press, 1.0);
            r.parts[1].1 = rx(0.4 + 0.12 * press) * ry(0.1 * (t * 1.7 + id).sin());
            r.parts[2].1 = rx(TAIL_DOWN - 0.1) * ry(0.1 * (t * 1.3 + id).sin());
            let a = -1.2 + 0.55 * press;
            (r.parts[3].1, r.parts[4].1, r.parts[5].1, r.parts[6].1) = (rx(a), rx(a), rx(-0.15), rx(-0.15));
            r.hold = Rig::paw_tip(a) + Vec3::new(0.0, 0.02, 0.06);
        }
        Pose::Dig => {
            let w = t * 7.0 + id;
            let (sl, sr) = (w.sin(), (w + std::f32::consts::PI).sin());
            r.pivot_body(Vec3::new(0.0, 0.1, 0.45), rx(0.22));
            r.parts[0].0.y += 0.01 * sl.abs();
            r.squash = Vec3::new(1.0, 1.0 - 0.02 * sl.abs(), 1.0);
            r.parts[1].1 = rx(0.3 + 0.06 * sl) * ry(0.12 * (t * 1.1 + id).sin());
            r.parts[2].1 = rx(TAIL_DOWN - 0.15) * ry(0.25 * (t * 2.2 + id).sin());
            (r.parts[3].1, r.parts[4].1, r.parts[5].1, r.parts[6].1) = (rx(-0.35 + 0.75 * sl), rx(-0.35 + 0.75 * sr), rx(-0.25), rx(-0.25));
            r.hold = Vec3::new(0.0, 0.08, 0.9);
        }
        Pose::Forage => {
            r = sit_rig(-0.8, (t * 1.8 + id).sin());
            r.parts[1].1 = rx(0.6 + 0.05 * (t * 13.0 + id).sin()) * ry(0.1 * (t * 0.8 + id).sin());
            let a = -2.25 + 0.12 * (t * 2.6 + id).sin();
            (r.parts[3].1, r.parts[4].1) = (rx(a), rx(a - 0.05));
            r.hold = Rig::paw_tip(a) + Vec3::new(0.0, 0.0, 0.08);
        }
        Pose::Rest => {
            let b = (t * 1.5 + id).sin();
            (r.parts[0].0.y, r.parts[0].1) = (-0.2 + 0.012 * b, rz(0.04));
            r.squash = Vec3::new(1.0 + 0.025 * b, 1.0 + 0.035 * b, 1.0);
            r.parts[1].0.y -= 0.08;
            r.parts[1].1 = rx(0.6) * ry(0.5 + 0.05 * b);
            r.parts[2].1 = rx(0.2 + TAIL_DOWN) * ry(1.1);
            for leg in 3..7 { r.parts[leg].1 = rx(-0.5); }
        }
        Pose::Handle => {
            let k = (t * 1.1 + id).rem_euclid(1.0);
            let bow = smooth(0.0, 0.35, k) * (1.0 - smooth(0.6, 1.0, k));
            r.pivot_body(HIPS, rx(0.32 * bow));
            r.parts[1].1 = rx(0.35 * bow) * ry(0.08 * (t * 1.3 + id).sin());
            let a = -0.2 - 1.0 * bow;
            (r.parts[3].1, r.parts[4].1) = (rx(a + 0.1 * (t * 9.0).sin() * bow), rx(a - 0.1 * (t * 9.0).sin() * bow));
            r.parts[2].1 = rx(TAIL_DOWN - 0.05) * ry(0.1 * (t * 1.7 + id).sin());
            r.hold = Rig::paw_tip(a) + Vec3::new(0.0, 0.04, 0.05);
        }
        Pose::Mend => {
            let (k, dab) = ((t * 0.55 + id).rem_euclid(1.0), (t * 6.0 + id).sin().max(0.0));
            let tamp = smooth(0.72, 0.8, k) * (1.0 - smooth(0.82, 0.92, k));
            r.pivot_body(HIPS, rx(0.06 + 0.06 * dab));
            r.parts[1].1 = rx(0.32 + 0.08 * dab) * ry(0.12 * (t * 0.9 + id).sin());
            (r.parts[3].1, r.parts[4].1) = (rx(-1.25 + 0.5 * dab), rx(-1.25 + 0.5 * (t * 6.0 + id + 1.6).sin().max(0.0)));
            (r.parts[5].1, r.parts[6].1) = (rx(-0.15), rx(-0.15));
            r.parts[2].1 = rx(TAIL_DOWN + 1.1 * tamp - 0.15 * smooth(0.8, 0.84, k) * (1.0 - smooth(0.86, 0.95, k)));
            r.hold = Rig::paw_tip(-1.25 + 0.5 * dab) + Vec3::new(0.0, 0.02, 0.06);
        }
        Pose::Watch => {
            r = sit_rig(-1.05, (t * 1.6 + id).sin());
            let sniff = window((t * 0.13 + id).rem_euclid(1.0), 0.6, 0.75, 0.04);
            r.parts[1].1 = ry(0.85 * (t * 0.45 + id).sin()) * rx(0.85 - 0.35 * sniff + 0.05 * (t * 14.0).sin() * sniff);
            (r.parts[3].1, r.parts[4].1) = (rx(-1.7), rx(-1.7));
        }
        Pose::Court => {
            let (br, up, nuzzle) = ((t * 1.6 + id).sin(), window((t * 0.25).rem_euclid(1.0), 0.45, 0.85, 0.08), (t * 2.4).sin());
            r.pivot_body(HIPS, rx(0.12 * (1.0 - up)));
            r.parts[0].0.y += 0.01 * br;
            r.parts[1].1 = rx(0.2 + 0.12 * nuzzle) * rz(0.18 * (t * 1.2 + id).sin());
            r.parts[2].1 = rx(TAIL_DOWN) * ry(0.35 * (t * 2.0 + id).sin());
            if up > 0.0 {
                let mut s = sit_rig(-0.9, br);
                s.parts[1].1 = rx(0.4 + 0.08 * nuzzle);
                (s.parts[3].1, s.parts[4].1) = (rx(-1.8 + 0.12 * nuzzle), rx(-1.8 - 0.12 * nuzzle));
                r = r.lerp(&s, up);
            }
        }
        Pose::Groom => {
            r = sit_rig(-0.6, (t * 1.6 + id).sin());
            let comb = (t * 5.5 + id).sin();
            r.parts[1].1 = rx(0.55 + 0.1 * comb) * ry(0.1 * (t * 0.7).sin());
            (r.parts[3].1, r.parts[4].1) = (rx(-1.6 + 0.35 * comb), rx(-1.6 - 0.35 * comb));
        }
        Pose::Caught => {
            let w = t * 24.0 + id;
            (r.parts[0].0.y, r.parts[0].1) = (-0.12 + 0.03 * (t * 13.0).sin(), rz(1.25 + 0.12 * (t * 9.0).sin()));
            r.parts[1].1 = rx(-0.2 + 0.35 * (t * 11.0).sin()) * ry(0.4 * (t * 7.0).sin());
            r.parts[2].1 = rx(TAIL_DOWN + 0.6 * (t * 10.0).sin());
            for (k, ph) in [(3usize, 0.0f32), (4, 1.6), (5, 3.1), (6, 4.7)] { r.parts[k].1 = rx(0.9 * (w + ph).sin()); }
        }
        Pose::Dead => {
            let (since, curled) = look.dead.unwrap_or((t - 10.0, false));
            let bounce = 0.04 * (smooth(0.55, 0.7, t - since) * (1.0 - smooth(0.7, 0.95, t - since)));
            let mut d = Rig::rest();
            d.parts[0].0.y = if curled { -0.16 } else { -0.02 } + bounce;
            if curled {
                (d.parts[0].1, d.parts[1].1, d.parts[2].1) = (rz(0.5) * ry(0.25), ry(-0.9) * rx(0.5), rx(TAIL_DOWN) * ry(1.2));
                for leg in 3..7 { d.parts[leg].1 = rx(-1.0); }
            } else {
                (d.parts[0].1, d.parts[1].1, d.parts[2].1) = (rz(1.4), rx(0.3) * rz(0.2), rx(TAIL_DOWN) * ry(0.3));
                for (leg, a) in [(3usize, 0.5f32), (4, 0.2), (5, -0.4), (6, -0.1)] { d.parts[leg].1 = rx(a); }
            }
            d.squash = Vec3::new(1.03, 0.94, 1.0);
            r = Rig::rest().lerp(&d, smooth(0.0, 0.8, t - since));
            r.parts[0].0.y -= 0.5 * look.fade;
        }
        Pose::Swim => {
            let p2 = g.phase + t * 3.0;
            (r.parts[0].0.y, r.parts[0].1) = (SWIM_DEPTH + 0.02 * (t * 2.5 + id).sin(), rx(-0.1 + 0.025 * (2.0 * p2).sin()));
            r.squash = Vec3::new(1.0, 1.0, 1.0 + 0.025 * (2.0 * p2).sin());
            r.parts[1].0 += Vec3::new(0.0, 0.18, -0.06);
            r.parts[1].1 = rx(-0.3 + 0.04 * (2.0 * p2 + 0.6).sin());
            r.parts[2].1 = rx(0.12 + 0.2 * (p2 - 1.2).sin()) * ry(0.3 * (t * 1.4 + id).sin());
            (r.parts[3].1, r.parts[4].1, r.parts[5].1, r.parts[6].1) = (rx(0.9), rx(0.9), rx(p2.sin()), rx(-p2.sin()));
        }
    }
    r
}

fn apply_look(r: &mut Rig, look: &Look, pose: Pose, t: f32, id: f32, g: Gait) {
    if pose == Pose::Dead { return; }
    let (rx, rz) = (Quat::from_rotation_x, Quat::from_rotation_z);
    let travel = matches!(pose, Pose::Walk | Pose::Carry | Pose::Run | Pose::Climb);
    r.parts[1].1 = rx(0.18 * look.thin + 0.22 * look.tired + 0.25 * look.sad + 0.1 * look.old) * r.parts[1].1;
    r.parts[2].1 = rx(-0.15 * look.sad - 0.08 * look.tired) * r.parts[2].1;
    r.squash *= Vec3::new(1.0 - 0.2 * look.thin + 0.16 * look.belly, 1.0 - 0.13 * look.thin - 0.05 * look.cold + 0.08 * look.belly, 1.0 - 0.06 * look.thin);
    if look.cold > 0.0 {
        r.parts[0].1 = rz(0.035 * look.cold * (t * 42.0 + id).sin()) * rx(0.06 * look.cold) * r.parts[0].1;
        r.parts[1].0 -= Vec3::new(0.0, 0.04 * look.cold, 0.06 * look.cold);
    }
    if look.old > 0.0 { r.parts[0].1 = rx(0.07 * look.old) * r.parts[0].1; }
    if look.hurt > 0.0 {
        r.parts[6].1 = r.parts[6].1.slerp(rx(-0.75), (if travel { 0.85 } else { 0.5 }) * look.hurt);
        if travel {
            r.parts[0].1 = rz(0.09 * look.hurt * g.phase.sin()) * r.parts[0].1;
            r.parts[0].0.y += 0.03 * look.hurt * g.phase.sin().abs();
        }
    }
    let since = t - look.slap;
    if (0.0..SLAP_S).contains(&since) {
        let up = smooth(0.0, 0.18, since) * (1.0 - smooth(0.22, 0.3, since));
        let hit = smooth(0.24, 0.3, since) * (1.0 - smooth(0.4, SLAP_S, since));
        r.parts[2].1 = rx(TAIL_DOWN + 1.5 * up - 0.35 * hit);
        r.parts[0].0.y += 0.06 * up - 0.05 * hit;
        r.parts[0].1 = rx(0.12 * up) * r.parts[0].1;
    }
    r.parts[0].0.y -= 1.3 * look.sink;
}

fn chip_flight(t: f32, id: f32) -> Transform {
    let k = t * 2.3 + id;
    let (u, side) = (k.rem_euclid(1.0), if k.floor().rem_euclid(2.0) < 1.0 { 1.0 } else { -1.0 });
    Transform::from_xyz(side * 0.55 * u, -0.28 + 0.75 * u - 1.1 * u * u, 0.72 + 0.25 * u)
        .with_rotation(Quat::from_rotation_z(side * u * 5.0) * Quat::from_rotation_x(u * 3.0))
        .with_scale(Vec3::splat((1.0 - u * u).max(0.05)))
}

pub(crate) fn shows_wake(pose: Pose, wet: bool) -> bool { pose == Pose::Swim && wet }

#[allow(clippy::too_many_arguments)]
fn animate_beavers(
    time: Res<Time>, clock: Res<RenderClock>, sim_clock: Option<Res<crate::sim_driver::SimClock>>,
    models: Option<Res<Models>>, cam: Query<&GlobalTransform, With<MainCamera>>, focus: Res<portrait::PortraitFocus>,
    mut roots: Query<(&mut Transform, &mut BeaverRig, &BeaverSprite, Option<&SetDown>)>,
    mut parts: Query<&mut Transform, Without<BeaverRig>>, mut vis: Query<&mut Visibility, Without<BeaverRig>>,
    mut meshes: Query<&mut Mesh3d, Without<BeaverRig>>,
) {
    let _p = ::sim::prof::scope("game/animate beavers");
    let Some(models) = models else { return };
    let (cam_pos, dt) = (cam.iter().next().map(|c| c.translation()), time.delta_secs());
    let smoothing = sim_clock.as_ref().map_or(1.0, |c| c.speed.smoothing());
    let (turn, ease) = (crate::sim_driver::ease_k(dt, 8.0, smoothing), 1.0 - (-dt * 6.0).exp());
    let show = |v: &mut Query<&mut Visibility, Without<BeaverRig>>, e: Entity, on: bool| {
        if let Ok(mut v) = v.get_mut(e) { v.set_if_neq(if on { Visibility::Inherited } else { Visibility::Hidden }); }
    };
    for (mut tr, mut rig, sprite, set_down) in &mut roots {
        let pose = if set_down.is_some_and(|d| clock.0 < d.until) { Pose::Handle } else { sprite.pose };
        let target_rot = Quat::from_rotation_y(sprite.yaw);
        tr.rotation = if rig.init { tr.rotation.slerp(target_rot, turn) } else { target_rot };
        let (size, id) = ((tr.scale.x / sim_sync::BEAVER_SIZE).max(0.1), sprite.id as f32 * 1.7);
        let moved = rig.last_pos.map_or(0.0, |p| (tr.translation - p).xz().length());
        rig.last_pos = Some(tr.translation);
        let moved = if moved < SNAP_JUMP { moved } else { 0.0 };
        rig.speed += (if dt > 1e-4 { moved / dt } else { 0.0 } - rig.speed) * ease;
        let run = smooth(BOUND_FROM, BOUND_FULL, rig.speed / size);
        let stride = STRIDE * size * (1.0 + 1.2 * run);
        rig.stride += (moved / stride * std::f32::consts::TAU).min(MAX_STRIDE_HZ * std::f32::consts::TAU * dt.max(1e-4));
        let near = focus.0 == Some(sprite.id) || cam_pos.is_none_or(|c| c.distance(tr.translation) < BEAVER_NEAR);
        let wet = shows_wake(pose, sprite.wet);
        if near != rig.near || !rig.init { rig.near = near; show(&mut vis, rig.body, near); show(&mut vis, rig.far, !near); }
        if !near {
            if let Ok(mut t) = parts.get_mut(rig.far) {
                t.translation.y = if pose == Pose::Swim { SWIM_DEPTH } else if rig.speed > 0.2 { 0.08 * rig.stride.sin().abs() } else { 0.0 };
            }
        }
        if pose != rig.cur { rig.prev = rig.cur; rig.cur = pose; rig.blend = if rig.init && smoothing > 0.0 { 0.0 } else { 1.0 }; }
        rig.blend = if smoothing > 0.0 { (rig.blend + dt * 5.0 / smoothing).min(1.0) } else { 1.0 };
        show(&mut vis, rig.wake, wet);
        show(&mut vis, rig.blob, !wet);
        if wet {
            if let Ok(mut t) = parts.get_mut(rig.wake) {
                let len = 0.3 + 0.7 * (rig.speed / (2.5 * size)).clamp(0.0, 1.0);
                let pulse = 1.0 + 0.06 * (clock.0 * 3.0 + id).sin();
                t.scale = Vec3::new(pulse, 1.0, len * pulse);
            }
        }
        let good = sprite.load.map(|l| l.0);
        let hug = !matches!(pose, Pose::Swim | Pose::Climb) && good.is_some_and(|g| g.row().held_in_paws);
        let load_scale = sprite.load.map_or(1.0, |(g, amount)| LOAD_LOOK * (0.75 + 0.5 * (amount / sim::agents::load_size(g)).clamp(0.0, 1.0)));
        rig.init = true;
        if !near { continue; }
        let g = Gait { phase: rig.stride, run, hug };
        let a = pose_rig(rig.cur, clock.0, id, g, &sprite.look);
        let blend = rig.blend * rig.blend * (3.0 - 2.0 * rig.blend);
        let mut r = if blend < 1.0 { pose_rig(rig.prev, clock.0, id, g, &sprite.look).lerp(&a, blend) } else { a };
        apply_look(&mut r, &sprite.look, rig.cur, clock.0, id, g);
        let ents = [rig.body, rig.parts[0], rig.parts[1], rig.parts[2], rig.parts[3], rig.parts[4], rig.parts[5]];
        for (&e, &(pos, rot)) in ents.iter().zip(r.parts.iter()) {
            if let Ok(mut t) = parts.get_mut(e) { (t.translation, t.rotation) = (pos, rot); }
        }
        let (build, head, tail) = looks_shape(&sprite.features);
        if let Ok(mut t) = parts.get_mut(rig.body_mesh) { t.scale = r.squash * build; }
        for (e, s) in [(rig.parts[0], head), (rig.parts[1], tail)] { if let Ok(mut t) = parts.get_mut(e) { t.scale = s; } }
        use sim::buildings::Good;
        let (working_mud, foraging) = (matches!(rig.cur, Pose::Work | Pose::Dig), rig.cur == Pose::Forage);
        let mouth_good = if foraging { Some(Good::food()) } else { good.filter(|_| !hug) };
        let paws_good = if hug { good } else if working_mud { Good::all().find(|g| g.row().worked_in_paws) } else { None };
        if rig.shown != (mouth_good, paws_good) {
            rig.shown = (mouth_good, paws_good);
            for (e, g, set) in [(rig.mouth, mouth_good, &models.loads), (rig.paws, paws_good, &models.paw_loads)] {
                show(&mut vis, e, g.is_some());
                if let (Some(g), Ok(mut m)) = (g, meshes.get_mut(e)) { m.0 = set[g.index()].clone(); }
            }
        }
        if let Ok(mut t) = parts.get_mut(rig.mouth) { t.scale = Vec3::splat(if foraging { 0.45 } else { load_scale }); }
        if let Ok(mut t) = parts.get_mut(rig.paws) {
            let gather = if rig.cur == Pose::Dig { 0.45 + 0.35 * (clock.0 * 0.4 + id).rem_euclid(1.0) } else if working_mud { 0.7 } else { load_scale };
            t.translation = r.hold - Vec3::Y * 0.2 * (gather - 1.0).max(0.0);
            t.scale = Vec3::splat(gather);
        }
        let heavy = sprite.look.tired > 0.6 && (clock.0 * 0.4 + id).rem_euclid(1.0) < 0.35;
        show(&mut vis, rig.lids, matches!(rig.cur, Pose::Rest | Pose::Dead) || blinking(clock.0, id) || heavy);
        show(&mut vis, rig.grey, sprite.look.old > 0.4);
        let gnawing = rig.cur == Pose::Gnaw;
        show(&mut vis, rig.chips, gnawing);
        if gnawing { if let Ok(mut t) = parts.get_mut(rig.chips) { *t = chip_flight(clock.0, id); } }
    }
}

fn season_at_pos(pos: Vec3, phase: f32) -> Season {
    let offset = (pos.x * 0.754_877_7 + pos.z * 0.569_840_3).rem_euclid(1.0) - 0.5;
    Season::at(phase + offset / Season::COUNT as f32)
}

#[allow(clippy::too_many_arguments)]
fn update_tree_lod(
    season: Res<crate::terrain_render::Season>, canopy: Res<CanopyView>, models: Option<Res<Models>>,
    cam: Query<&GlobalTransform, With<MainCamera>>, sim: Res<crate::state::Sim>, hud: Option<Res<crate::hud::HudState>>,
    hover: Option<Res<crate::hud::Hover>>, selection: Option<Res<crate::flow::Selection>>,
    mut trees: Query<(&Transform, &mut TreeLod, &mut Mesh3d)>,
) {
    let _p = ::sim::prof::scope("game/update tree lod");
    let (Some(models), Some(cam)) = (models, cam.iter().next()) else { return };
    let (c, l0, l1) = (cam.translation(), TREE_LOD0 * TREE_LOD0, TREE_LOD1 * TREE_LOD1);
    let mut clear = Vec::new();
    if let Some(crate::flow::Selection::Site(id)) = selection.as_deref().copied() {
        if let Some(b) = sim.0.buildings.building(id) { clear.push(crate::flow::cell_world(&sim.0.height, (b.pos.0 as f32 + 0.5, b.pos.1 as f32 + 0.5))); }
    }
    if !sim.0.is_running() || hud.as_ref().is_some_and(|h| h.tool != crate::hud::Tool::None) {
        if let Some(cell) = hover.and_then(|h| h.cell()) { clear.push(crate::flow::cell_world(&sim.0.height, (cell.0 as f32 + 0.5, cell.1 as f32 + 0.5))); }
    }
    for (t, mut lod, mut mesh) in &mut trees {
        let d2 = t.translation.distance_squared(c);
        let want = if d2 < l0 { 0 } else if d2 < l1 { 1 } else { 2 };
        let tree_season = season_at_pos(t.translation, season.phase);
        let (crown, r2) = (t.translation + Vec3::Y * 3.0 * t.scale.y, (2.5 * t.scale.x.max(t.scale.z)).powi(2));
        let cutaway = canopy.0 || c.xz().distance_squared(t.translation.xz()) < r2 || clear.iter().any(|p| {
            let ray = c - *p;
            crown.distance_squared(*p + ray * ((crown - *p).dot(ray) / ray.length_squared().max(0.01)).clamp(0.0, 1.0)) < r2
        });
        if (want, tree_season, cutaway) != (lod.lod, lod.season, lod.cutaway) {
            (lod.lod, lod.season, lod.cutaway) = (want, tree_season, cutaway);
            mesh.0 = if cutaway { models.cutaway_trees[lod.species.index()].clone() } else { models.tree(lod.species, tree_season, want) };
        }
    }
}

fn update_plant_season(season: Res<crate::terrain_render::Season>, models: Option<Res<Models>>, mut plants: Query<(&Transform, &mut PlantLook, &mut Mesh3d)>) {
    let Some(models) = models else { return };
    for (t, mut look, mut mesh) in &mut plants {
        let s = season_at_pos(t.translation, season.phase);
        if s != look.season { look.season = s; mesh.0 = models.plant(look.plant, s); }
    }
}

#[derive(Component)]
struct Topple { start: f32, base: Transform }

fn topple_trees(mut commands: Commands, clock: Res<RenderClock>, mut q: Query<(Entity, &mut Transform, &Topple)>) {
    for (e, mut tr, t) in &mut q {
        let u = ((clock.0 - t.start) / TOPPLE_S).max(0.0);
        if u >= 1.15 { commands.entity(e).despawn(); continue; }
        let fall = if u < 1.0 { u * u * u } else { 1.0 - 0.06 * ((u - 1.0) / 0.15 * std::f32::consts::PI).sin() };
        tr.rotation = Quat::from_axis_angle(t.base.rotation * Vec3::Z, -fall * std::f32::consts::FRAC_PI_2 * 0.97) * t.base.rotation;
    }
}

fn sway_trees(clock: Res<RenderClock>, mut trees: Query<(&mut Transform, &TreeLod, &Sway)>) {
    let t = clock.0;
    for (mut tr, lod, base) in &mut trees {
        let want = if lod.lod == 0 {
            let p = tr.translation;
            let ph = p.x * 0.21 + p.z * 0.13;
            let gust = 1.0 + 0.7 * (t * 0.45 - p.x * 0.04 - p.z * 0.025).sin().max(-0.6);
            Quat::from_rotation_z(0.026 * gust * ((t * 1.1 + ph).sin() + 0.35 * (t * 2.6 + ph * 1.7).sin()))
                * Quat::from_rotation_x(0.013 * gust * (t * 0.9 + ph * 1.3).cos()) * base.0
        } else { base.0 };
        if tr.rotation != want { tr.rotation = want; }
    }
}
