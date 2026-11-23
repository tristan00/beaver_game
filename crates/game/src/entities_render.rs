mod effects;
mod gallery;
mod harbor;
mod meshgen;
mod models;
mod scene;
mod structures;
mod wildlife;
pub mod sim_sync;
pub mod sim_view;
mod wolf;

use crate::camera::MainCamera;
use crate::cli::Cli;
use bevy::prelude::*;
use meshgen::Mb;
use models::Season;
use scene::*;
use std::collections::HashMap;

pub use scene::{height_at, lodge_view};
pub use wolf::{AlarmSlaps, WolfDrawn};

/// Beavers nearer the camera than this are fully rigged; the default camera sits ~150 away.
const BEAVER_NEAR: f32 = 260.0;
const TREE_LOD0: f32 = 220.0;
const TREE_LOD1: f32 = 480.0;
const SCREENSHOT_CLOCK: f32 = 31.0;
/// How much bigger than its share of the load a carried good is drawn.
const LOAD_LOOK: f32 = 1.5;
pub struct EntitiesRenderPlugin;

impl Plugin for EntitiesRenderPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<RenderClock>()
            .init_resource::<TailSlapTimes>()
            .init_resource::<effects::PuffMesh>()
            .init_resource::<CurrentSeason>()
            .init_resource::<CanopyView>()
            .init_resource::<sim_sync::PendingDays>()
            .add_plugins(wolf::WolfPlugin)
            .add_plugins(wildlife::WildlifePlugin)
            .add_systems(Startup, init_models)
            .add_systems(
                Update,
                (
                    update_clock_and_season,
                    sim_sync::update_pending,
                    sim_sync::sync_sim_beavers,
                    sim_sync::sync_bodies,
                    sim_sync::sync_sim_resources,
                    sim_sync::sync_sim_buildings,
                    sim_sync::sync_sim_dams,
                    sim_sync::sync_dam_ghosts,
                    sim_sync::sync_reinforcements,
                    sim_sync::sync_ground_goods,
                    gallery::spawn_gallery,
                    gallery::shuffle_gallery,
                    gallery::select_home,
                    attach_rigs,
                    animate_beavers,
                    canopy_key,
                    update_tree_lod,
                    sway_trees,
                    topple_trees,
                )
                    .chain()
                    .after(crate::terrain_render::update_season),
            )
            .add_systems(Update, (effects::grow, effects::puffs).chain().after(topple_trees))
            .add_systems(Startup, harbor::init_harbor_meshes)
            .add_systems(Update, (harbor::sync_locks, harbor::sync_barges, harbor::animate_locks).chain().after(sim_sync::sync_sim_dams));
    }
}

#[derive(Resource, Default)]
struct RenderClock(f32);

/// When each beaver last slapped its tail (render clock), by beaver id.
#[derive(Resource, Default)]
pub(crate) struct TailSlapTimes(pub HashMap<u64, f32>);

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
    wake_mat: Handle<StandardMaterial>,
    /// A building's surface as it wears: worn, more worn, falling apart.
    wear: [Handle<StandardMaterial>; 3],
    /// Goods on the ground as they rot: going off, rotten.
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
    felled: Vec<Handle<Mesh>>,
    mud: Handle<Mesh>,
    plant: Handle<Mesh>,
    water_plant: Handle<Mesh>,
    pebbles: Handle<Mesh>,
    far: Handle<Mesh>,
    wake: Handle<Mesh>,
    blob: Handle<Mesh>,
    structs: HashMap<(StructKind, u32, u8), Handle<Mesh>>,
    dams: HashMap<(u8, u32, u32, structures::DamStage), Handle<Mesh>>,
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
    pub look: Look,
}

/// How a beaver's state shows on whatever it is doing, each 0 (not at all) to 1 (badly).
#[derive(Clone, Copy, Debug)]
pub struct Look {
    pub thin: f32,
    pub cold: f32,
    pub hurt: f32,
    pub tired: f32,
    pub old: f32,
    pub sad: f32,
    /// Pregnancy, 0 at conception to 1 at birth.
    pub belly: f32,
    /// Render-clock time of its last tail slap.
    pub slap: f32,
    /// For a body: render-clock time it fell, and whether it lies curled up.
    pub dead: Option<(f32, bool)>,
    /// How far a body has sunk away, 0 to 1.
    pub fade: f32,
    /// How far under the water it is, diving into or out of its lodge, 0 to 1.
    pub sink: f32,
}

impl Default for Look {
    fn default() -> Self {
        Look { thin: 0.0, cold: 0.0, hurt: 0.0, tired: 0.0, old: 0.0, sad: 0.0, belly: 0.0, slap: -100.0, dead: None, fade: 0.0, sink: 0.0 }
    }
}

/// A beaver is a pivot (the body's motion) with the body mesh, head, tail and legs under it.
/// Squash and stretch go on the body mesh alone, so the limbs never shear.
#[derive(Component)]
pub(crate) struct BeaverRig {
    body: Entity,
    body_mesh: Entity,
    far: Entity,
    parts: [Entity; 6],
    mouth: Entity,
    paws: Entity,
    lids: Entity,
    chips: Entity,
    grey: Entity,
    wake: Entity,
    blob: Entity,
    prev: Pose,
    cur: Pose,
    blend: f32,
    near: bool,
    init: bool,
    /// What the mouth and the paws are showing.
    shown: (Option<sim::buildings::Good>, Option<sim::buildings::Good>),
    stride: f32,
    speed: f32,
    last_pos: Option<Vec3>,
}

const SNAP_JUMP: f32 = 3.0;
/// World distance covered by one walking stride of a full-grown beaver; the leg swing in
/// `pose_rig` sweeps about this far, so planted feet barely slide.
const STRIDE: f32 = 0.42;
/// Above this speed (world units a second, full-grown) the walk turns into a bound.
const BOUND_FROM: f32 = 1.3;
const BOUND_FULL: f32 = 2.8;
const MAX_STRIDE_HZ: f32 = 5.5;

pub const ENTITY_GAIN: f32 = 0.55;

/// Kits and yearlings are drawn smaller than adults.
pub fn age_scale(stage: sim::colony::Stage) -> f32 {
    match stage {
        sim::colony::Stage::Kit => 0.68,
        sim::colony::Stage::Yearling => 0.8,
        sim::colony::Stage::Adult => 1.0,
    }
}

/// A beaver's size by its age: growing steadily from a newborn to full size, with no jump
/// when it moves up a stage.
pub fn age_scale_days(age_days: f32) -> f32 {
    use sim::colony::{ADULT_DAYS, KIT_DAYS};
    if age_days < KIT_DAYS {
        0.55 + (0.74 - 0.55) * (age_days / KIT_DAYS).clamp(0.0, 1.0)
    } else {
        0.74 + (1.0 - 0.74) * ((age_days - KIT_DAYS) / (ADULT_DAYS - KIT_DAYS)).clamp(0.0, 1.0)
    }
}

/// Clay and food are hugged to the chest; sticks are carried in the teeth.
fn held_in_paws(good: sim::buildings::Good) -> bool {
    good != sim::buildings::Good::Sticks
}

fn init_models(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>, mut materials: ResMut<Assets<StandardMaterial>>) {
    let white = materials.add(StandardMaterial { base_color: Color::linear_rgb(ENTITY_GAIN, ENTITY_GAIN, ENTITY_GAIN), perceptual_roughness: 1.0, reflectance: 0.0, ..default() });
    let tree_tints = [[1.0, 1.0, 1.0], [0.86, 0.96, 0.84], [1.05, 0.95, 0.80], [0.80, 0.88, 0.86]].map(|[r,g,b]| {
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
    let wake_mat = materials.add(StandardMaterial {
        base_color: Color::srgba(0.95, 1.0, 0.98, 0.55),
        alpha_mode: AlphaMode::Blend,
        unlit: true,
        ..default()
    });
    let tint = |m: &mut Assets<StandardMaterial>, r: f32, g: f32, b: f32| m.add(StandardMaterial { base_color: Color::linear_rgb(ENTITY_GAIN * r, ENTITY_GAIN * g, ENTITY_GAIN * b), perceptual_roughness: 1.0, reflectance: 0.0, ..default() });
    let wear = [tint(&mut materials, 0.86, 0.84, 0.8), tint(&mut materials, 0.72, 0.7, 0.66), tint(&mut materials, 0.58, 0.56, 0.54)];
    let rot = [tint(&mut materials, 0.84, 0.86, 0.66), tint(&mut materials, 0.56, 0.6, 0.42)];
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
    let grey = build(&models::beaver_grey);
    let chips = build(&models::gnaw_chips);
    let far = build(&models::beaver_static);
    let wake = build(&models::wake);
    let blob = build(&models::blob_shadow);
    commands.insert_resource(Models {
        white,
        tree_tints,
        old_tree,
        blob_mat,
        wake_mat,
        wear,
        rot,
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
        grey,
        loads: sim::buildings::Good::ALL.iter().map(|g| meshes.add(models::load_mesh(*g))).collect(),
        paw_loads: sim::buildings::Good::ALL.iter().map(|g| meshes.add(models::paw_load_mesh(*g))).collect(),
        felled: (0..4u8).flat_map(|l| [false, true].map(|b| meshes.add(models::felled_mesh(l, b)))).collect(),
        mud: meshes.add(models::mud_mesh()),
        plant: meshes.add(models::plant_mesh()),
        water_plant: meshes.add(models::water_plant_mesh()),
        pebbles: meshes.add(models::pebble_mesh()),
        far,
        wake,
        blob,
        structs: HashMap::new(),
        dams: HashMap::new(),
    });
}

fn update_clock_and_season(time: Res<Time>, cli: Res<Cli>, visual: Res<crate::terrain_render::Season>, mut clock: ResMut<RenderClock>, mut season: ResMut<CurrentSeason>) {
    let shot = || std::env::var("BC_CLOCK").ok().and_then(|s| s.parse().ok()).unwrap_or(SCREENSHOT_CLOCK);
    let frozen = cli.screenshot.is_some() && std::env::var_os("BC_BURST").is_none() && std::env::var_os("BC_GALLERY").is_none();
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
        let target = crate::picking::Target::Beaver(sim::colony::BeaverId(sprite.id));
        let mut body = Entity::PLACEHOLDER;
        commands.entity(root).with_children(|p| {
            body = p.spawn((Transform::default(), Visibility::Inherited)).id();
        });
        let mut parts = [Entity::PLACEHOLDER; 6];
        let (mut body_mesh, mut paws, mut far, mut wake, mut blob) = (Entity::PLACEHOLDER, Entity::PLACEHOLDER, Entity::PLACEHOLDER, Entity::PLACEHOLDER, Entity::PLACEHOLDER);
        commands.entity(body).with_children(|p| {
            body_mesh = p.spawn((target, Mesh3d(m.body.clone()), MeshMaterial3d(w.clone()), Transform::default())).id();
            parts[0] = p.spawn((target, Mesh3d(m.head.clone()), MeshMaterial3d(w.clone()), Transform::from_translation(models::HEAD_PIVOT))).id();
            parts[1] = p.spawn((target, Mesh3d(m.tail.clone()), MeshMaterial3d(w.clone()), Transform::from_translation(models::TAIL_PIVOT))).id();
            for k in 0..4 {
                let mesh = if k < 2 { m.leg_front.clone() } else { m.leg_hind.clone() };
                parts[2 + k] = p.spawn((target, Mesh3d(mesh), MeshMaterial3d(w.clone()), Transform::from_translation(models::LEG_PIVOTS[k]))).id();
            }
            paws = p.spawn((Mesh3d(m.paw_loads[0].clone()), MeshMaterial3d(w.clone()), Transform::from_translation(models::CHEST_HOLD), Visibility::Hidden)).id();
        });
        let (mut mouth, mut lids, mut chips, mut grey) = (Entity::PLACEHOLDER, Entity::PLACEHOLDER, Entity::PLACEHOLDER, Entity::PLACEHOLDER);
        commands.entity(parts[0]).with_children(|p| {
            mouth = p.spawn((Mesh3d(m.loads[0].clone()), MeshMaterial3d(w.clone()), Transform::from_translation(models::MOUTH), Visibility::Hidden)).id();
            lids = p.spawn((Mesh3d(m.lids.clone()), MeshMaterial3d(w.clone()), Transform::default(), Visibility::Hidden)).id();
            chips = p.spawn((Mesh3d(m.chips.clone()), MeshMaterial3d(w.clone()), Transform::default(), Visibility::Hidden)).id();
            grey = p.spawn((Mesh3d(m.grey.clone()), MeshMaterial3d(w.clone()), Transform::default(), Visibility::Hidden)).id();
        });
        commands.entity(root).with_children(|p| {
            far = p.spawn((target, Mesh3d(m.far.clone()), MeshMaterial3d(w.clone()), Transform::default(), Visibility::Hidden)).id();
            wake = p.spawn((Mesh3d(m.wake.clone()), MeshMaterial3d(m.wake_mat.clone()), Transform::from_xyz(0.0, 0.04, 1.15), Visibility::Hidden)).id();
            blob = p.spawn((Mesh3d(m.blob.clone()), MeshMaterial3d(m.blob_mat.clone()), Transform::from_xyz(0.0, 0.05, 0.0))).id();
        });
        commands.entity(root).insert(BeaverRig {
            body, body_mesh, far, parts, mouth, paws, lids, chips, grey, wake, blob,
            prev: sprite.pose, cur: sprite.pose, blend: 1.0, near: true, init: false, shown: (None, None),
            stride: sprite.id as f32 * 1.7, speed: 0.0, last_pos: None,
        });
    }
}

/// Body, head, tail, four legs; the squash of the body mesh; where the paws hold things.
#[derive(Clone, Copy)]
struct Rig {
    parts: [(Vec3, Quat); 7],
    squash: Vec3,
    hold: Vec3,
}

impl Rig {
    fn rest() -> Rig {
        Rig {
            parts: [
                (Vec3::ZERO, Quat::IDENTITY),
                (models::HEAD_PIVOT, Quat::IDENTITY),
                (models::TAIL_PIVOT, Quat::from_rotation_x(TAIL_DOWN)),
                (models::LEG_PIVOTS[0], Quat::IDENTITY),
                (models::LEG_PIVOTS[1], Quat::IDENTITY),
                (models::LEG_PIVOTS[2], Quat::IDENTITY),
                (models::LEG_PIVOTS[3], Quat::IDENTITY),
            ],
            squash: Vec3::ONE,
            hold: models::CHEST_HOLD,
        }
    }

    fn lerp(&self, o: &Rig, w: f32) -> Rig {
        let mut r = *self;
        for k in 0..7 {
            r.parts[k] = (self.parts[k].0.lerp(o.parts[k].0, w), self.parts[k].1.slerp(o.parts[k].1, w));
        }
        r.squash = self.squash.lerp(o.squash, w);
        r.hold = self.hold.lerp(o.hold, w);
        r
    }

    fn pivot_body(&mut self, pivot: Vec3, rot: Quat) {
        self.parts[0] = (self.parts[0].0 + pivot - rot * pivot, rot * self.parts[0].1);
    }

    /// Where a front paw's tip is, in body space, with the leg turned `a` about x.
    fn paw_tip(a: f32) -> Vec3 {
        let p = models::LEG_PIVOTS[1];
        Vec3::new(0.0, p.y, p.z) + Quat::from_rotation_x(a) * Vec3::new(0.0, -0.3, 0.1)
    }
}

/// A tail at rest lies on the ground behind (its pivot is above the ground).
const TAIL_DOWN: f32 = -0.26;
const HIPS: Vec3 = Vec3::new(0.0, 0.12, -0.42);

fn smooth(a: f32, b: f32, x: f32) -> f32 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn window(x: f32, a: f32, b: f32, ramp: f32) -> f32 { smooth(a, a + ramp, x) * (1.0 - smooth(b - ramp, b, x)) }

const IDLE_LOOP: f32 = 14.0;

pub(crate) fn blinking(t: f32, id: f32) -> bool { (t * 0.29 + id * 0.37).rem_euclid(1.0) < 0.04 }

/// Sitting up on the haunches, tail flat behind; front paws free.
fn sit_rig(lean: f32, br: f32) -> Rig {
    let rx = Quat::from_rotation_x;
    let mut s = Rig::rest();
    s.pivot_body(HIPS, rx(lean));
    s.parts[0].0.y += 0.04 + 0.01 * br;
    s.parts[2].1 = rx(-lean + TAIL_DOWN + 0.1);
    s.parts[5].1 = rx(-lean - 0.35);
    s.parts[6].1 = rx(-lean - 0.35);
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
    let (i, f) = (k.floor(), k.fract());
    let dir = |n: f32| ((n * 12.9898 + id).sin() * 43758.547).fract() * 2.0 - 1.0;
    let look = dir(i) + (dir(i + 1.0) - dir(i)) * smooth(0.7, 1.0, f);
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
        s.parts[3].1 = rx(-1.95 + 0.25 * g);
        s.parts[4].1 = rx(-1.95 - 0.25 * g);
        r = r.lerp(&s, sit);
    }
    r
}

/// How a beaver moves this frame: stride phase, how far into a bound (0 walk, 1 bound),
/// and whether what it carries is hugged in its paws.
#[derive(Clone, Copy)]
struct Gait {
    phase: f32,
    run: f32,
    hug: bool,
}

/// Walking is a diagonal trot; fast travel is a bound, front feet together then hind feet,
/// the back flexing. `run` blends the two.
fn travel_rig(g: Gait, lift_head: f32) -> Rig {
    let (rx, ry, rz) = (Quat::from_rotation_x, Quat::from_rotation_y, Quat::from_rotation_z);
    let mut r = Rig::rest();
    let ph = g.phase;
    let run = g.run;
    let (s, c2) = (ph.sin(), (2.0 * ph).cos());
    let hs = (ph - 0.3).sin();
    let bound_f = (ph + 0.4).sin();
    let bound_h = (ph - 2.3).sin();
    let amp = 0.75 + 0.3 * run;
    let fl = amp * (s + (bound_f - s) * run);
    let fr = amp * (-s + (bound_f + s) * run);
    let hl = amp * 1.05 * (-hs + (bound_h + hs) * run);
    let hr = amp * 1.05 * (hs + (bound_h - hs) * run);
    let trot_bob = 0.05 * (2.0 * ph).sin().abs();
    let bound_bob = 0.14 * (ph + 0.9).sin().max(0.0);
    r.parts[0].0.y = trot_bob + (bound_bob - trot_bob) * run;
    let flex = 0.16 * (ph - 0.6).sin() * run;
    r.parts[0].1 = rz(0.08 * s * (1.0 - run)) * rx(0.03 * (2.0 * ph).sin() + flex);
    r.squash = Vec3::new(1.0 + 0.025 * c2, 1.0 - 0.04 * c2, 1.0 + 0.015 * c2 + 0.06 * run * (ph - 0.6).sin());
    r.parts[1].1 = rx(-lift_head + 0.07 * (2.0 * ph + 1.0).sin() - flex) * rz(-0.06 * s * (1.0 - run));
    r.parts[2].1 = rx(TAIL_DOWN + 0.2 * run + 0.05 * (2.0 * ph + 0.8).sin() - flex) * ry(0.12 * s * (1.0 - run));
    r.parts[3].1 = rx(fl);
    r.parts[4].1 = rx(fr);
    r.parts[5].1 = rx(hl);
    r.parts[6].1 = rx(hr);
    r
}

/// Upright on the hind legs with a load hugged to the chest: short waddling steps.
fn hug_walk_rig(ph: f32) -> Rig {
    let (rx, ry, rz) = (Quat::from_rotation_x, Quat::from_rotation_y, Quat::from_rotation_z);
    let lean = -0.75;
    let s = ph.sin();
    let mut r = sit_rig(lean, 0.0);
    r.parts[0].0.y += 0.035 * (2.0 * ph).sin().abs();
    r.parts[0].1 = rz(0.1 * s) * r.parts[0].1;
    r.parts[1].1 = rx(0.45 + 0.05 * (2.0 * ph).sin()) * rz(-0.08 * s);
    r.parts[2].1 = rx(-lean + TAIL_DOWN + 0.1) * ry(0.18 * s);
    let hug = -0.85;
    r.parts[3].1 = rx(hug + 0.05 * s);
    r.parts[4].1 = rx(hug - 0.05 * s);
    r.parts[5].1 = rx(-lean - 0.15 + 0.55 * s);
    r.parts[6].1 = rx(-lean - 0.15 - 0.55 * s);
    r.hold = Rig::paw_tip(hug) + Vec3::new(0.0, 0.08, 0.0);
    r
}

fn pose_rig(pose: Pose, t: f32, id: f32, g: Gait, look: &Look) -> Rig {
    let rx = Quat::from_rotation_x;
    let ry = Quat::from_rotation_y;
    let rz = Quat::from_rotation_z;
    let mut r = Rig::rest();
    match pose {
        // Standing about with a load: upright, still hugging it, looking around.
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
        Pose::Gnaw => {
            let chew = (t * 15.0 + id).sin();
            r.pivot_body(HIPS, rx(-0.5) * rz(0.05 * (t * 7.5).sin()));
            r.parts[0].0.y += 0.02 + 0.015 * chew;
            r.squash = Vec3::new(1.0, 1.0 + 0.02 * chew, 1.0);
            r.parts[1].1 = rx(0.45 + 0.17 * chew) * rz(0.12 * (t * 7.5 + id).sin()) * ry(0.06 * (t * 7.0).sin());
            r.parts[2].1 = rx(0.5 + TAIL_DOWN) * ry(0.08 * (t * 2.0 + id).sin());
            r.parts[3].1 = rx(-1.25 + 0.08 * (t * 15.0).sin());
            r.parts[4].1 = rx(-1.25 - 0.08 * (t * 15.0).sin());
            r.parts[5].1 = rx(0.35);
            r.parts[6].1 = rx(0.35);
        }
        Pose::Climb => {
            let s = g.phase.sin();
            let c2 = (2.0 * g.phase).cos();
            r.pivot_body(HIPS, rx(-0.38) * rz(0.06 * s));
            r.parts[0].0.y += 0.04 * (2.0 * g.phase).sin().abs();
            r.squash = Vec3::new(1.0, 1.0 - 0.03 * c2, 1.0 + 0.03 * c2);
            r.parts[1].1 = rx(0.2 + 0.08 * s);
            r.parts[2].1 = rx(0.38 + TAIL_DOWN) * ry(0.2 * s);
            r.parts[3].1 = rx(-1.0 + 0.6 * s);
            r.parts[4].1 = rx(-1.0 - 0.6 * s);
            r.parts[5].1 = rx(0.38 - 0.6 * s);
            r.parts[6].1 = rx(0.38 + 0.6 * s);
        }
        // Building: both forepaws press a lump of mud into place, again and again.
        Pose::Work => {
            let k = (t * 3.2 + id).rem_euclid(1.0);
            let press = smooth(0.0, 0.35, k) * (1.0 - smooth(0.55, 1.0, k));
            r.pivot_body(HIPS, rx(0.08 + 0.1 * press));
            r.parts[0].0.y += 0.02 * (1.0 - press);
            r.squash = Vec3::new(1.0 + 0.03 * press, 1.0 - 0.04 * press, 1.0);
            r.parts[1].1 = rx(0.4 + 0.12 * press) * ry(0.1 * (t * 1.7 + id).sin());
            r.parts[2].1 = rx(TAIL_DOWN - 0.1) * ry(0.1 * (t * 1.3 + id).sin());
            let a = -1.2 + 0.55 * press;
            r.parts[3].1 = rx(a);
            r.parts[4].1 = rx(a);
            r.parts[5].1 = rx(-0.15);
            r.parts[6].1 = rx(-0.15);
            r.hold = Rig::paw_tip(a) + Vec3::new(0.0, 0.02, 0.06);
        }
        // Digging: nose down, forepaws scooping in turn, mud gathering between them.
        Pose::Dig => {
            let w = t * 7.0 + id;
            let (sl, sr) = (w.sin(), (w + std::f32::consts::PI).sin());
            r.pivot_body(Vec3::new(0.0, 0.1, 0.45), rx(0.22));
            r.parts[0].0.y += 0.01 * sl.abs();
            r.squash = Vec3::new(1.0, 1.0 - 0.02 * sl.abs(), 1.0);
            r.parts[1].1 = rx(0.3 + 0.06 * sl) * ry(0.12 * (t * 1.1 + id).sin());
            r.parts[2].1 = rx(TAIL_DOWN - 0.15) * ry(0.25 * (t * 2.2 + id).sin());
            r.parts[3].1 = rx(-0.35 + 0.75 * sl);
            r.parts[4].1 = rx(-0.35 + 0.75 * sr);
            r.parts[5].1 = rx(-0.25);
            r.parts[6].1 = rx(-0.25);
            r.hold = Vec3::new(0.0, 0.08, 0.9);
        }
        // Foraging: sitting up, turning a sprig in the forepaws and chewing it.
        Pose::Forage => {
            let br = (t * 1.8 + id).sin();
            r = sit_rig(-0.8, br);
            let chew = (t * 13.0 + id).sin();
            r.parts[1].1 = rx(0.6 + 0.05 * chew) * ry(0.1 * (t * 0.8 + id).sin());
            let a = -2.25 + 0.12 * (t * 2.6 + id).sin();
            r.parts[3].1 = rx(a);
            r.parts[4].1 = rx(a - 0.05);
            // What has been gathered so far is held up in the paws.
            r.hold = Rig::paw_tip(a) + Vec3::new(0.0, 0.0, 0.08);
        }
        Pose::Rest => {
            let b = (t * 1.5 + id).sin();
            r.parts[0].0.y = -0.2 + 0.012 * b;
            r.parts[0].1 = rz(0.04);
            r.squash = Vec3::new(1.0 + 0.025 * b, 1.0 + 0.035 * b, 1.0);
            r.parts[1].0.y -= 0.08;
            r.parts[1].1 = rx(0.6) * ry(0.5 + 0.05 * b);
            r.parts[2].1 = rx(0.2 + TAIL_DOWN) * ry(1.1);
            r.parts[3].1 = rx(-0.5);
            r.parts[4].1 = rx(-0.5);
            r.parts[5].1 = rx(-0.5);
            r.parts[6].1 = rx(-0.5);
        }
        // Flat out from a wolf: a full bound, head stretched low, tail up behind.
        Pose::Run => {
            r = travel_rig(Gait { run: 1.0, ..g }, -0.15);
            r.parts[2].1 = rx(TAIL_DOWN + 0.55 + 0.1 * (g.phase - 0.6).sin());
        }
        // Taking up or setting down a load: a bow to the ground, forepaws gathering, and up.
        Pose::Handle => {
            let k = (t * 1.1 + id).rem_euclid(1.0);
            let bow = smooth(0.0, 0.35, k) * (1.0 - smooth(0.6, 1.0, k));
            r.pivot_body(HIPS, rx(0.32 * bow));
            r.parts[1].1 = rx(0.35 * bow) * ry(0.08 * (t * 1.3 + id).sin());
            let a = -0.2 - 1.0 * bow;
            r.parts[3].1 = rx(a + 0.1 * (t * 9.0).sin() * bow);
            r.parts[4].1 = rx(a - 0.1 * (t * 9.0).sin() * bow);
            r.parts[2].1 = rx(TAIL_DOWN - 0.05) * ry(0.1 * (t * 1.7 + id).sin());
            r.hold = Rig::paw_tip(a) + Vec3::new(0.0, 0.04, 0.05);
        }
        // Mending: patting mud onto the wall in quick dabs, then a slap of the tail to tamp it.
        Pose::Mend => {
            let k = (t * 0.55 + id).rem_euclid(1.0);
            let dab = (t * 6.0 + id).sin().max(0.0);
            let tamp = smooth(0.72, 0.8, k) * (1.0 - smooth(0.82, 0.92, k));
            r.pivot_body(HIPS, rx(0.06 + 0.06 * dab));
            r.parts[1].1 = rx(0.32 + 0.08 * dab) * ry(0.12 * (t * 0.9 + id).sin());
            r.parts[3].1 = rx(-1.25 + 0.5 * dab);
            r.parts[4].1 = rx(-1.25 + 0.5 * (t * 6.0 + id + 1.6).sin().max(0.0));
            r.parts[5].1 = rx(-0.15);
            r.parts[6].1 = rx(-0.15);
            // The tail swings up, then smacks flat.
            r.parts[2].1 = rx(TAIL_DOWN + 1.1 * tamp - 0.15 * smooth(0.8, 0.84, k) * (1.0 - smooth(0.86, 0.95, k)));
            r.hold = Rig::paw_tip(-1.25 + 0.5 * dab) + Vec3::new(0.0, 0.02, 0.06);
        }
        // On watch: sitting up tall, turning slowly to scan, now and then lifting the nose.
        Pose::Watch => {
            let br = (t * 1.6 + id).sin();
            r = sit_rig(-1.05, br);
            let scan = (t * 0.45 + id).sin();
            let sniff = window((t * 0.13 + id).rem_euclid(1.0), 0.6, 0.75, 0.04);
            r.parts[1].1 = ry(0.85 * scan) * rx(0.85 - 0.35 * sniff + 0.05 * (t * 14.0).sin() * sniff);
            r.parts[3].1 = rx(-1.7);
            r.parts[4].1 = rx(-1.7);
        }
        // Courting: nose to nose with its mate, nuzzling, and sitting up together now and then.
        Pose::Court => {
            let br = (t * 1.6 + id).sin();
            let c = (t * 0.25).rem_euclid(1.0);
            let up = window(c, 0.45, 0.85, 0.08);
            let nuzzle = (t * 2.4).sin();
            r.pivot_body(HIPS, rx(0.12 * (1.0 - up)));
            r.parts[0].0.y += 0.01 * br;
            r.parts[1].1 = rx(0.2 + 0.12 * nuzzle) * rz(0.18 * (t * 1.2 + id).sin());
            r.parts[2].1 = rx(TAIL_DOWN) * ry(0.35 * (t * 2.0 + id).sin());
            if up > 0.0 {
                let mut s = sit_rig(-0.9, br);
                s.parts[1].1 = rx(0.4 + 0.08 * nuzzle);
                s.parts[3].1 = rx(-1.8 + 0.12 * nuzzle);
                s.parts[4].1 = rx(-1.8 - 0.12 * nuzzle);
                r = r.lerp(&s, up);
            }
        }
        // Grooming: sitting, turned to the other, combing its fur with the forepaws.
        Pose::Groom => {
            let br = (t * 1.6 + id).sin();
            r = sit_rig(-0.6, br);
            let comb = (t * 5.5 + id).sin();
            r.parts[1].1 = rx(0.55 + 0.1 * comb) * ry(0.1 * (t * 0.7).sin());
            r.parts[3].1 = rx(-1.6 + 0.35 * comb);
            r.parts[4].1 = rx(-1.6 - 0.35 * comb);
        }
        // Pinned: thrown on its side, legs paddling, head thrashing.
        Pose::Caught => {
            let w = t * 24.0 + id;
            r.parts[0].0.y = -0.12 + 0.03 * (t * 13.0).sin();
            r.parts[0].1 = rz(1.25 + 0.12 * (t * 9.0).sin());
            r.parts[1].1 = rx(-0.2 + 0.35 * (t * 11.0).sin()) * ry(0.4 * (t * 7.0).sin());
            r.parts[2].1 = rx(TAIL_DOWN + 0.6 * (t * 10.0).sin());
            for (k, ph) in [(3usize, 0.0f32), (4, 1.6), (5, 3.1), (6, 4.7)] {
                r.parts[k].1 = rx(0.9 * (w + ph).sin());
            }
        }
        // Dead: it falls over in a moment and lies still, on its side or curled up.
        Pose::Dead => {
            let (since, curled) = look.dead.unwrap_or((t - 10.0, false));
            let k = smooth(0.0, 0.8, t - since);
            let bounce = 0.04 * (smooth(0.55, 0.7, t - since) * (1.0 - smooth(0.7, 0.95, t - since)));
            let mut d = Rig::rest();
            d.parts[0].0.y = if curled { -0.16 } else { -0.02 } + bounce;
            if curled {
                d.parts[0].1 = rz(0.5) * ry(0.25);
                d.parts[1].1 = ry(-0.9) * rx(0.5);
                d.parts[2].1 = rx(TAIL_DOWN) * ry(1.2);
                for leg in 3..7 {
                    d.parts[leg].1 = rx(-1.0);
                }
            } else {
                d.parts[0].1 = rz(1.4);
                d.parts[1].1 = rx(0.3) * rz(0.2);
                d.parts[2].1 = rx(TAIL_DOWN) * ry(0.3);
                for (leg, a) in [(3usize, 0.5f32), (4, 0.2), (5, -0.4), (6, -0.1)] {
                    d.parts[leg].1 = rx(a);
                }
            }
            d.squash = Vec3::new(1.03, 0.94, 1.0);
            r = Rig::rest().lerp(&d, k);
            // Sinking away at the end.
            r.parts[0].0.y -= 0.5 * look.fade;
        }
        // Swimming low: only the head, a strip of back and the tail at the surface.
        Pose::Swim => {
            let p2 = g.phase + t * 3.0;
            r.parts[0].0.y = SWIM_DEPTH + 0.02 * (t * 2.5 + id).sin();
            r.parts[0].1 = rx(-0.1 + 0.025 * (2.0 * p2).sin());
            r.squash = Vec3::new(1.0, 1.0, 1.0 + 0.025 * (2.0 * p2).sin());
            r.parts[1].0 += Vec3::new(0.0, 0.18, -0.06);
            r.parts[1].1 = rx(-0.3 + 0.04 * (2.0 * p2 + 0.6).sin());
            r.parts[2].1 = rx(0.12 + 0.2 * (p2 - 1.2).sin()) * ry(0.3 * (t * 1.4 + id).sin());
            r.parts[3].1 = rx(0.9);
            r.parts[4].1 = rx(0.9);
            r.parts[5].1 = rx(1.0 * p2.sin());
            r.parts[6].1 = rx(-1.0 * p2.sin());
        }
    }
    r
}

/// How far a swimming beaver's body sinks below the surface (model units).
const SWIM_DEPTH: f32 = -0.68;

/// How long a tail slap takes, in seconds.
const SLAP_S: f32 = 0.6;

/// A beaver's state shown over what it is doing: thin and head down when hungry, hunched and
/// shivering when cold, limping when hurt, drooping when worn out, stooped when old, head and
/// tail low when unhappy, a round belly late in pregnancy; and a tail slap, and sinking under
/// the water at a lodge.
fn apply_look(r: &mut Rig, look: &Look, pose: Pose, t: f32, id: f32, g: Gait) {
    let (rx, ry, rz) = (Quat::from_rotation_x, Quat::from_rotation_y, Quat::from_rotation_z);
    if pose == Pose::Dead {
        return;
    }
    let travel = matches!(pose, Pose::Walk | Pose::Carry | Pose::Run | Pose::Climb);
    let droop = 0.18 * look.thin + 0.22 * look.tired + 0.25 * look.sad + 0.1 * look.old;
    r.parts[1].1 = rx(droop) * r.parts[1].1;
    r.parts[2].1 = rx(-0.15 * look.sad - 0.08 * look.tired) * r.parts[2].1;
    // Hunger shows as a lean, narrow body: a hungry beaver looks it from any side.
    r.squash.x *= 1.0 - 0.2 * look.thin + 0.16 * look.belly;
    r.squash.y *= 1.0 - 0.13 * look.thin - 0.05 * look.cold + 0.08 * look.belly;
    r.squash.z *= 1.0 - 0.06 * look.thin;
    if look.cold > 0.0 {
        // Hunched, shivering in quick small shakes.
        let shiver = 0.035 * look.cold * (t * 42.0 + id).sin();
        r.parts[0].1 = rz(shiver) * rx(0.06 * look.cold) * r.parts[0].1;
        r.parts[1].0.z -= 0.06 * look.cold;
        r.parts[1].0.y -= 0.04 * look.cold;
    }
    if look.old > 0.0 {
        r.parts[0].1 = rx(0.07 * look.old) * r.parts[0].1;
    }
    if look.hurt > 0.0 {
        // Holding up the hurt hind leg; a hobbling roll on the move.
        let held = rx(-0.75);
        r.parts[6].1 = r.parts[6].1.slerp(held, (if travel { 0.85 } else { 0.5 }) * look.hurt);
        if travel {
            r.parts[0].1 = rz(0.09 * look.hurt * g.phase.sin()) * r.parts[0].1;
            r.parts[0].0.y += 0.03 * look.hurt * g.phase.sin().abs();
        }
    }
    let since = t - look.slap;
    if (0.0..SLAP_S).contains(&since) {
        // Up high, then down flat on the water with a jolt of the body.
        let up = smooth(0.0, 0.18, since) * (1.0 - smooth(0.22, 0.3, since));
        let hit = smooth(0.24, 0.3, since) * (1.0 - smooth(0.4, SLAP_S, since));
        r.parts[2].1 = rx(TAIL_DOWN + 1.5 * up - 0.35 * hit) * ry(0.0);
        r.parts[0].0.y += 0.06 * up - 0.05 * hit;
        r.parts[0].1 = rx(0.12 * up) * r.parts[0].1;
    }
    r.parts[0].0.y -= 1.3 * look.sink;
    let _ = ry;
}

fn chip_flight(t: f32, id: f32) -> Transform {
    let k = t * 2.3 + id;
    let (u, side) = (k.rem_euclid(1.0), if k.floor().rem_euclid(2.0) < 1.0 { 1.0 } else { -1.0 });
    let p = Vec3::new(side * 0.55 * u, -0.28 + 0.75 * u - 1.1 * u * u, 0.72 + 0.25 * u);
    Transform::from_translation(p)
        .with_rotation(Quat::from_rotation_z(side * u * 5.0) * Quat::from_rotation_x(u * 3.0))
        .with_scale(Vec3::splat((1.0 - u * u).max(0.05)))
}

pub(crate) fn shows_wake(pose: Pose, wet: bool) -> bool {
    pose == Pose::Swim && wet
}

#[allow(clippy::too_many_arguments)]
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
    let _p = ::sim::prof::scope("game/animate beavers");
    let Some(models) = models else { return };
    let cam_pos = cam.iter().next().map(|c| c.translation());
    let dt = time.delta_secs();
    let turn = 1.0 - (-dt * 8.0).exp();
    let ease = 1.0 - (-dt * 6.0).exp();
    let show = |v: &mut Query<&mut Visibility, Without<BeaverRig>>, e: Entity, on: bool| {
        if let Ok(mut v) = v.get_mut(e) {
            v.set_if_neq(if on { Visibility::Inherited } else { Visibility::Hidden });
        }
    };
    for (mut tr, mut rig, sprite) in &mut roots {
        let target_rot = Quat::from_rotation_y(sprite.yaw);
        tr.rotation = if rig.init { tr.rotation.slerp(target_rot, turn) } else { target_rot };
        let size = (tr.scale.x / sim_sync::BEAVER_SIZE).max(0.1);
        let id = sprite.id as f32 * 1.7;

        let moved = rig.last_pos.map_or(0.0, |p| (tr.translation - p).xz().length());
        rig.last_pos = Some(tr.translation);
        let moved = if moved < SNAP_JUMP { moved } else { 0.0 };
        let v = if dt > 1e-4 { moved / dt } else { 0.0 };
        rig.speed += (v - rig.speed) * ease;
        // Gait normalised to a full-grown beaver, so kits take quicker, shorter steps.
        let vn = rig.speed / size;
        let run = smooth(BOUND_FROM, BOUND_FULL, vn);
        let stride = STRIDE * size * (1.0 + 1.2 * run);
        rig.stride += (moved / stride * std::f32::consts::TAU).min(MAX_STRIDE_HZ * std::f32::consts::TAU * dt.max(1e-4));

        let near = cam_pos.is_none_or(|c| c.distance(tr.translation) < BEAVER_NEAR);
        let swimming = sprite.pose == Pose::Swim;
        let wet = shows_wake(sprite.pose, sprite.wet);
        if near != rig.near || !rig.init {
            rig.near = near;
            show(&mut vis, rig.body, near);
            show(&mut vis, rig.far, !near);
        }
        if !near {
            if let Ok(mut t) = parts.get_mut(rig.far) {
                // Far away there is no rig, but a moving beaver still hops along.
                let hop = if rig.speed > 0.2 { 0.08 * (rig.stride).sin().abs() } else { 0.0 };
                t.translation.y = if swimming { SWIM_DEPTH } else { hop };
            }
        }
        if sprite.pose != rig.cur {
            rig.prev = rig.cur;
            rig.cur = sprite.pose;
            rig.blend = if rig.init { 0.0 } else { 1.0 };
        }
        rig.blend = (rig.blend + dt * 5.0).min(1.0);
        show(&mut vis, rig.wake, wet);
        show(&mut vis, rig.blob, !wet);
        if wet {
            if let Ok(mut t) = parts.get_mut(rig.wake) {
                // The V opens out behind a beaver under way and shrinks to a bow ripple at rest.
                let len = 0.3 + 0.7 * (rig.speed / (2.5 * size)).clamp(0.0, 1.0);
                let pulse = 1.0 + 0.06 * (clock.0 * 3.0 + id).sin();
                t.scale = Vec3::new(pulse, 1.0, len * pulse);
            }
        }
        let good = sprite.load.map(|l| l.0);
        // Swimming and climbing need the forepaws, so then everything is carried in the teeth.
        let paws_free = !matches!(sprite.pose, Pose::Swim | Pose::Climb);
        let hug = paws_free && good.is_some_and(held_in_paws);
        // Loads are drawn larger than life so what a beaver carries reads from the usual camera.
        let load_scale = sprite.load.map_or(1.0, |(g, amount)| LOAD_LOOK * (0.75 + 0.5 * (amount / sim::agents::load_size(g)).clamp(0.0, 1.0)));
        rig.init = true;
        if !near {
            continue;
        }
        let g = Gait { phase: rig.stride, run, hug };
        let look = &sprite.look;
        let a = pose_rig(rig.cur, clock.0, id, g, look);
        let blend = rig.blend * rig.blend * (3.0 - 2.0 * rig.blend);
        let mut r = if blend < 1.0 { pose_rig(rig.prev, clock.0, id, g, look).lerp(&a, blend) } else { a };
        apply_look(&mut r, look, rig.cur, clock.0, id, g);
        let ents = [rig.body, rig.parts[0], rig.parts[1], rig.parts[2], rig.parts[3], rig.parts[4], rig.parts[5]];
        for (e, (pos, rot)) in ents.iter().zip(r.parts.iter()) {
            if let Ok(mut t) = parts.get_mut(*e) {
                t.translation = *pos;
                t.rotation = *rot;
            }
        }
        if let Ok(mut t) = parts.get_mut(rig.body_mesh) {
            t.scale = r.squash;
        }
        // What shows in the mouth and in the paws.
        use sim::buildings::Good;
        let working_mud = matches!(rig.cur, Pose::Work | Pose::Dig);
        let foraging = rig.cur == Pose::Forage;
        let mouth_good = if foraging { Some(Good::Food) } else { good.filter(|_| !hug) };
        let paws_good = if hug { good } else if working_mud { Some(Good::Clay) } else { None };
        if rig.shown != (mouth_good, paws_good) {
            rig.shown = (mouth_good, paws_good);
            for (e, g, set) in [(rig.mouth, mouth_good, &models.loads), (rig.paws, paws_good, &models.paw_loads)] {
                show(&mut vis, e, g.is_some());
                if let (Some(g), Ok(mut m)) = (g, meshes.get_mut(e)) {
                    m.0 = set[g.index()].clone();
                }
            }
        }
        if let Ok(mut t) = parts.get_mut(rig.mouth) {
            t.scale = Vec3::splat(if foraging { 0.45 } else { load_scale });
        }
        if let Ok(mut t) = parts.get_mut(rig.paws) {
            let gather = if rig.cur == Pose::Dig { 0.45 + 0.35 * (clock.0 * 0.4 + id).rem_euclid(1.0) } else if working_mud { 0.7 } else { load_scale };
            // A big armful sits lower, under the chin rather than over it.
            t.translation = r.hold - Vec3::Y * 0.2 * (gather - 1.0).max(0.0);
            t.scale = Vec3::splat(gather);
        }
        let heavy = sprite.look.tired > 0.6 && (clock.0 * 0.4 + id).rem_euclid(1.0) < 0.35;
        let shut = matches!(rig.cur, Pose::Rest | Pose::Dead) || blinking(clock.0, id) || heavy;
        show(&mut vis, rig.lids, shut);
        show(&mut vis, rig.grey, sprite.look.old > 0.4);
        let gnawing = rig.cur == Pose::Gnaw;
        show(&mut vis, rig.chips, gnawing);
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
    sim: Res<crate::state::Sim>,
    hud: Option<Res<crate::hud::HudState>>,
    hover: Option<Res<crate::hud::HoverCell>>,
    selection: Option<Res<crate::flow::Selection>>,
    mut trees: Query<(&Transform, &mut TreeLod, &mut Mesh3d)>,
) {
    let _p = ::sim::prof::scope("game/update tree lod");
    let (Some(models), Some(cam)) = (models, cam.iter().next()) else { return };
    let c = cam.translation();
    let (l0, l1) = (TREE_LOD0 * TREE_LOD0, TREE_LOD1 * TREE_LOD1);
    let mut clear: Vec<Vec3> = sim.0.agents.by_id.values().filter(|a| !a.inside)
        .map(|a| sim_view::agent_world_pos(&sim.0, a, 0.0) + Vec3::Y * 0.4).collect();
    if let Some(crate::flow::Selection::Site(id)) = selection.as_deref().copied() {
        if let Some(b) = sim.0.buildings.building(id) {
            clear.push(crate::flow::cell_world(&sim.0.height, (b.pos.0 as f32 + 0.5, b.pos.1 as f32 + 0.5)));
        }
    }
    if !sim.0.is_running() || hud.as_ref().is_some_and(|h| h.tool != crate::hud::Tool::None || h.order_mode.is_some()) {
        if let Some(cell) = hover.and_then(|h| h.0) {
            clear.push(crate::flow::cell_world(&sim.0.height, (cell.0 as f32 + 0.5, cell.1 as f32 + 0.5)));
        }
    }
    for (t, mut lod, mut mesh) in &mut trees {
        let d2 = t.translation.distance_squared(c);
        let want = if d2 < l0 { 0 } else if d2 < l1 { 1 } else { 2 };
        let offset = (t.translation.x * 0.754_877_7 + t.translation.z * 0.569_840_3).rem_euclid(1.0) - 0.5;
        let tree_season = Season::at(season.phase + offset / Season::COUNT as f32);
        let crown = t.translation + Vec3::Y * 3.0 * t.scale.y;
        let radius = 2.5 * t.scale.x.max(t.scale.z);
        // Only trees whose crown stands between the camera and something to keep in view.
        let cutaway = canopy.0 || c.xz().distance_squared(t.translation.xz()) < radius * radius || clear.iter().any(|p| {
            let ray = c - *p;
            let along = ((crown - *p).dot(ray) / ray.length_squared().max(0.01)).clamp(0.0, 1.0);
            crown.distance_squared(*p + ray * along) < radius * radius
        });
        if want != lod.lod || lod.season != tree_season || lod.cutaway != cutaway {
            lod.lod = want;
            lod.season = tree_season;
            lod.cutaway = cutaway;
            mesh.0 = if cutaway { models.cutaway_trees[species_index(lod.species)].clone() }
                else { models.tree(lod.species, tree_season, want) };
        }
    }
}

/// A falling tree (see `sim_sync::spawn_topple`).
#[derive(Component)]
struct Topple {
    start: f32,
    base: Transform,
}

const TOPPLE_S: f32 = 1.5;

/// The tree leans slowly, then falls fast (gravity), bounces once and is gone as the logs show.
fn topple_trees(mut commands: Commands, clock: Res<RenderClock>, mut q: Query<(Entity, &mut Transform, &Topple)>) {
    for (e, mut tr, t) in &mut q {
        let u = ((clock.0 - t.start) / TOPPLE_S).max(0.0);
        if u >= 1.15 {
            commands.entity(e).despawn();
            continue;
        }
        let fall = if u < 1.0 { u * u * u } else { 1.0 - 0.06 * ((u - 1.0) / 0.15 * std::f32::consts::PI).sin() };
        let axis = t.base.rotation * Vec3::Z;
        tr.rotation = Quat::from_axis_angle(axis, -fall * std::f32::consts::FRAC_PI_2 * 0.97) * t.base.rotation;
    }
}

fn sway_trees(clock: Res<RenderClock>, mut trees: Query<(&mut Transform, &TreeLod, &Sway)>) {
    let t = clock.0;
    for (mut tr, lod, base) in &mut trees {
        let want = if lod.lod == 0 {
            let p = tr.translation;
            let ph = p.x * 0.21 + p.z * 0.13;
            // Gusts roll across the forest as a slow wave, so neighbouring crowns move together.
            let gust = 1.0 + 0.7 * (t * 0.45 - p.x * 0.04 - p.z * 0.025).sin().max(-0.6);
            let a = 0.026 * gust * ((t * 1.1 + ph).sin() + 0.35 * (t * 2.6 + ph * 1.7).sin());
            let b = 0.013 * gust * (t * 0.9 + ph * 1.3).cos();
            Quat::from_rotation_z(a) * Quat::from_rotation_x(b) * base.0
        } else {
            base.0
        };
        if tr.rotation != want {
            tr.rotation = want;
        }
    }
}


