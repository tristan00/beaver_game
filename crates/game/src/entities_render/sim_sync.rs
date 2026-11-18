//! Systems that keep the drawn world equal to `SimState`: one sprite per beaver agent at its sim
//! position and pose, one entity per map resource (standing trees, felled trees and logs, mud banks,
//! stone outcrops, plants) and one per building or construction site (rising with progress).
//!
//! Everything is diffed against the sim each frame by id and a small signature, so only things that
//! changed touch the ECS. Beaver sprites carry no meshes here; `attach_rigs` in the parent module
//! adds the animated rig, which keeps these systems testable without a renderer.
use super::scene::{height_at, Species, StructKind};
use super::sim_view::*;
use super::{models, BeaverSprite, CurrentSeason, Models, SimBeaver, SimEntity, TreeLod, WheelSpin};
use crate::cli::Cli;
use crate::sim_driver::SimClock;
use crate::state::Sim;
use bevy::prelude::*;
use sim::buildings::{BuildingId, BuildingKind, Status};
use sim::resources::{ResourceId, ResourceKind};
use sim::worldgen::Tree;
use std::collections::{HashMap, HashSet};

/// Sim time that has passed since the last fixed step, in days, so drawn motion is continuous
/// between steps. Zero when the sim has just stepped (and always in tests).
#[derive(Resource, Default)]
pub struct PendingDays(pub f32);

pub fn update_pending(clock: Option<Res<SimClock>>, mut p: ResMut<PendingDays>) {
    p.0 = clock.map_or(0.0, |c| c.pending_days());
}

/// Spawns, updates and removes one beaver sprite per sim agent.
pub fn sync_sim_beavers(
    mut commands: Commands,
    cli: Res<Cli>,
    sim: Res<Sim>,
    pending: Res<PendingDays>,
    mut q: Query<(Entity, &mut Transform, &mut BeaverSprite, &mut Visibility), With<SimBeaver>>,
    mut seen: Local<HashSet<u64>>,
) {
    if super::stub_mode(&cli) {
        return;
    }
    seen.clear();
    for (e, mut tr, mut sprite, mut vis) in &mut q {
        let Some(ag) = sim.0.agents.get(sim::colony::BeaverId(sprite.id)) else {
            commands.entity(e).despawn();
            continue;
        };
        seen.insert(sprite.id);
        tr.translation = agent_world_pos(&sim.0, ag, pending.0);
        sprite.pose = pose_of(&sim.0, ag);
        sprite.load = load_of(ag);
        sprite.wet = is_wet(&sim.0, ag);
        if let Some(y) = facing_of(&sim.0, ag) {
            sprite.yaw = y;
        }
        let want = if ag.inside { Visibility::Hidden } else { Visibility::Inherited };
        if *vis != want {
            *vis = want;
        }
    }
    for (&id, ag) in &sim.0.agents.by_id {
        if seen.contains(&id.0) {
            continue;
        }
        let yaw = facing_of(&sim.0, ag).unwrap_or(0.0);
        commands.spawn((
            SimBeaver,
            BeaverSprite { id: id.0, pose: pose_of(&sim.0, ag), load: load_of(ag), yaw, wet: is_wet(&sim.0, ag) },
            Transform::from_translation(agent_world_pos(&sim.0, ag, 0.0)).with_rotation(Quat::from_rotation_y(yaw)).with_scale(Vec3::splat(WORLD_SCALE * 1.3)),
            if ag.inside { Visibility::Hidden } else { Visibility::Inherited },
        ));
    }
}

// ---------------------------------------------------------------------------------------------
// Resources
// ---------------------------------------------------------------------------------------------

/// What a resource entity currently shows; the entity is only touched when this changes.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ResSig {
    /// A standing tree and its stage (`sim::forestry::TreeStage as u8`).
    Tree(u8),
    /// Logs still lying there (whole logs, rounded up) and whether the branches remain.
    Felled(u8, bool),
    /// Remaining stock quantised to eighths (mud, stone) or quarters (plants).
    Stock(ResourceKindTag, u8),
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ResourceKindTag {
    Mud,
    Stone,
    Plant,
}

pub fn res_sig(r: &sim::resources::Resource) -> ResSig {
    let q = |n: f32| ((r.amount / r.max_amount.max(1e-3)).clamp(0.0, 1.0) * n).ceil() as u8;
    match r.kind {
        ResourceKind::Tree => ResSig::Tree(r.stage().map_or(2, |s| s as u8)),
        ResourceKind::Felled => ResSig::Felled(r.amount.ceil().clamp(0.0, 3.0) as u8, r.branches > 0.0),
        ResourceKind::MudBank => ResSig::Stock(ResourceKindTag::Mud, q(8.0)),
        ResourceKind::StoneOutcrop => ResSig::Stock(ResourceKindTag::Stone, q(8.0)),
        ResourceKind::Plant => ResSig::Stock(ResourceKindTag::Plant, q(4.0)),
    }
}

fn hash01(x: usize, y: usize, salt: u32) -> f32 {
    let mut h = (x as u32).wrapping_mul(0x9E37_79B1) ^ (y as u32).wrapping_mul(0x85EB_CA6B) ^ salt.wrapping_mul(0xC2B2_AE35);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2C1B_3C6D);
    h ^= h >> 12;
    (h & 0xffff) as f32 / 65535.0
}

fn species_at(sim: &sim::SimState, c: (usize, usize)) -> Species {
    match sim.world.forest.get(c.0, c.1).copied().flatten() {
        Some(Tree::Aspen | Tree::Birch) => Species::Aspen,
        Some(Tree::Willow) => Species::Willow,
        Some(Tree::Alder) => Species::Maple,
        Some(Tree::Pine) => Species::Pine,
        None => Species::Maple,
    }
}

#[derive(Default)]
pub struct WorldSync {
    seed: Option<u64>,
    time: f64,
    res: HashMap<ResourceId, (Entity, ResSig)>,
    bld: HashMap<BuildingId, (Entity, (BuildingKind, (usize, usize), u8))>,
    lodges: HashMap<u64, Entity>,
}

/// A tree's size by stage: saplings small, young trees half grown, old ones a little broader.
pub fn stage_scale(st: sim::forestry::TreeStage) -> Vec3 {
    use sim::forestry::TreeStage::*;
    match st {
        Sapling => Vec3::splat(0.3),
        Young => Vec3::splat(0.62),
        Mature | Snag => Vec3::ONE,
        Old => Vec3::new(1.12, 1.0, 1.12),
    }
}

fn res_visual(models: &Models, sim: &sim::SimState, r: &sim::resources::Resource, season: models::Season) -> (Handle<Mesh>, Vec3, Option<TreeLod>) {
    let sig = res_sig(r);
    let c = r.cell;
    let hash_scale = 0.8 + 0.4 * hash01(c.0, c.1, 5);
    let k = WORLD_SCALE;
    match sig {
        ResSig::Tree(_) if r.snag => (models.snag.clone(), Vec3::splat(k * hash_scale), None),
        ResSig::Tree(_) => {
            let sp = species_at(sim, c);
            let st = r.stage().unwrap_or(sim::forestry::TreeStage::Mature);
            (models.tree(sp, season, 0), stage_scale(st) * k * hash_scale, Some(TreeLod { species: sp, lod: 0, season }))
        }
        ResSig::Felled(logs, branches) => (models.felled[logs as usize * 2 + branches as usize].clone(), Vec3::splat(k), None),
        ResSig::Stock(tag, q) => {
            let n = if tag == ResourceKindTag::Plant { 4.0 } else { 8.0 };
            let s = 0.45 + 0.55 * (q as f32 / n);
            let (mesh, base) = match tag {
                ResourceKindTag::Mud => (models.mud.clone(), 0.62),
                ResourceKindTag::Stone => (models.stone.clone(), 0.55),
                ResourceKindTag::Plant => (models.plant.clone(), 0.5),
            };
            (mesh, Vec3::splat(base * s), None)
        }
    }
}

fn res_transform(sim: &sim::SimState, r: &sim::resources::Resource, scale: Vec3) -> Transform {
    let c = r.cell;
    let p = cell_xz(sim, (c.0 as f32 + 0.5, c.1 as f32 + 0.5));
    let y = height_at(&sim.height, p.x, p.y) - if r.kind == ResourceKind::Tree { 0.05 } else { 0.0 };
    let yaw = hash01(c.0, c.1, 9) * std::f32::consts::TAU;
    Transform::from_xyz(p.x, y, p.y).with_rotation(Quat::from_rotation_y(yaw)).with_scale(scale)
}

/// Old trees are drawn darker; everything else with the shared material.
fn material_for(models: &Models, r: &sim::resources::Resource) -> Handle<StandardMaterial> {
    if r.stage() == Some(sim::forestry::TreeStage::Old) { models.old_tree.clone() } else { models.white.clone() }
}

/// Standing trees (scaled by stage, old ones darker, snags as bare trunks), felled trees with
/// their logs, mud banks, stone deposits and plants.
pub fn sync_sim_resources(
    mut commands: Commands,
    cli: Res<Cli>,
    sim: Res<Sim>,
    season: Res<CurrentSeason>,
    models: Option<Res<Models>>,
    mut st: Local<WorldSync>,
) {
    let Some(models) = models else { return };
    if super::stub_mode(&cli) {
        return;
    }
    let sim = &sim.0;
    if st.seed != Some(sim.seed) {
        for (e, _) in st.res.drain().map(|(_, v)| v) {
            commands.entity(e).despawn();
        }
        st.seed = Some(sim.seed);
        st.time = f64::NEG_INFINITY;
    }
    if sim.time_days == st.time && !st.res.is_empty() {
        return;
    }
    st.time = sim.time_days;
    let mut live: HashSet<ResourceId> = HashSet::with_capacity(sim.resources.items.len());
    for r in &sim.resources.items {
        live.insert(r.id);
        let sig = res_sig(r);
        match st.res.get(&r.id).copied() {
            Some((_, old)) if old == sig => {}
            Some((e, _)) => {
                let (mesh, scale, lod) = res_visual(&models, sim, r, season.0);
                let mut ec = commands.entity(e);
                ec.insert((Mesh3d(mesh), MeshMaterial3d(material_for(&models, r)), res_transform(sim, r, scale)));
                match lod {
                    Some(l) => ec.insert(l),
                    None => ec.remove::<TreeLod>(),
                };
                st.res.insert(r.id, (e, sig));
            }
            None => {
                let (mesh, scale, lod) = res_visual(&models, sim, r, season.0);
                let mut ec = commands.spawn((SimEntity, Mesh3d(mesh), MeshMaterial3d(material_for(&models, r)), res_transform(sim, r, scale)));
                if let Some(l) = lod {
                    ec.insert(l);
                }
                let e = ec.id();
                st.res.insert(r.id, (e, sig));
            }
        }
    }
    let gone: Vec<ResourceId> = st.res.keys().copied().filter(|id| !live.contains(id)).collect();
    for id in gone {
        if let Some((e, _)) = st.res.remove(&id) {
            commands.entity(e).despawn();
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Buildings and sites
// ---------------------------------------------------------------------------------------------

/// The model that stands for a sim building kind (kinds without their own model borrow the nearest).
pub fn model_kind(k: BuildingKind) -> StructKind {
    match k {
        BuildingKind::Dam | BuildingKind::Lock => StructKind::Dam,
        BuildingKind::Lodge => StructKind::Lodge,
        BuildingKind::Burrow | BuildingKind::EldersDen | BuildingKind::FoodCache | BuildingKind::ScentMound | BuildingKind::GuardPost | BuildingKind::ForagerHut | BuildingKind::MudPit | BuildingKind::TreeNursery => StructKind::Burrow,
        BuildingKind::Canal => StructKind::Canal,
        BuildingKind::WaterWheel | BuildingKind::Mill => StructKind::WaterWheel,
        BuildingKind::Workshop | BuildingKind::Warehouse | BuildingKind::Storehouse | BuildingKind::LoggingCamp | BuildingKind::QuarryCamp | BuildingKind::ForesterLodge => StructKind::Workshop,
        BuildingKind::Dock => StructKind::Dock,
        BuildingKind::Barge => StructKind::Barge,
    }
}

/// Construction progress `0..=1` of a building as shown (1 once complete).
pub fn build_progress(sim: &sim::SimState, b: &sim::buildings::Building) -> f32 {
    if b.status == Status::Complete { 1.0 } else { sim.work.progress(b.id).unwrap_or(0.0).clamp(0.0, 0.999) }
}

fn building_transform(sim: &sim::SimState, b: &sim::buildings::Building) -> Transform {
    let (w, h) = b.kind.spec().footprint;
    let c = cell_xz(sim, (b.pos.0 as f32 + w as f32 / 2.0, b.pos.1 as f32 + h as f32 / 2.0));
    let ground = |p: Vec2| height_at(&sim.height, p.x, p.y);
    let lowest = b.cells().map(|c| ground(cell_xz(sim, (c.0 as f32 + 0.5, c.1 as f32 + 0.5)))).fold(f32::MAX, f32::min);
    let on_water = matches!(b.kind, BuildingKind::Dock | BuildingKind::Barge | BuildingKind::WaterWheel);
    let y = if on_water { stand_y(sim, b.cells().next().unwrap_or(b.pos)).max(lowest) } else { lowest };
    Transform::from_xyz(c.x, y, c.y).with_scale(Vec3::splat(WORLD_SCALE))
}

pub fn sync_sim_buildings(mut commands: Commands, cli: Res<Cli>, sim: Res<Sim>, models: Option<ResMut<Models>>, mut meshes: ResMut<Assets<Mesh>>, mut st: Local<WorldSync>) {
    let Some(mut models) = models else { return };
    if super::stub_mode(&cli) {
        return;
    }
    let sim = &sim.0;
    if st.seed != Some(sim.seed) {
        for (e, _) in st.bld.drain().map(|(_, v)| v) {
            commands.entity(e).despawn();
        }
        for (_, e) in st.lodges.drain() {
            commands.entity(e).despawn();
        }
        st.seed = Some(sim.seed);
        st.time = f64::NEG_INFINITY;
    }
    if sim.time_days == st.time && !(st.bld.is_empty() && st.lodges.is_empty()) {
        return;
    }
    st.time = sim.time_days;
    let white = models.white.clone();
    let mut live = HashSet::new();
    let mut spawn = |commands: &mut Commands, models: &mut Models, kind: StructKind, len2: u32, stage: u8, tr: Transform| -> Entity {
        let mesh = models.structs.entry((kind, len2, stage)).or_insert_with(|| meshes.add(models::struct_mesh(kind, len2 as f32 * 0.5, stage))).clone();
        let wheel = (kind == StructKind::WaterWheel && stage == 3).then(|| models.wheel.clone());
        let mut e = commands.spawn((SimEntity, Mesh3d(mesh), MeshMaterial3d(white.clone()), tr));
        if let Some(w) = wheel {
            let w2 = white.clone();
            e.with_children(|p| {
                p.spawn((Mesh3d(w), MeshMaterial3d(w2), Transform::from_xyz(0.0, 2.0, 0.0), WheelSpin(0.9)));
            });
        }
        e.id()
    };
    for b in &sim.buildings.buildings {
        live.insert(b.id);
        let stage = models::stage_of(build_progress(sim, b));
        let sig = (b.kind, b.pos, stage);
        if st.bld.get(&b.id).is_some_and(|(_, s)| *s == sig) {
            continue;
        }
        if let Some((old, _)) = st.bld.remove(&b.id) {
            commands.entity(old).despawn();
        }
        let (w, _) = b.kind.spec().footprint;
        let e = spawn(&mut commands, &mut models, model_kind(b.kind), (w as u32 * 2).max(2), stage, building_transform(sim, b));
        st.bld.insert(b.id, (e, sig));
    }
    let gone: Vec<BuildingId> = st.bld.keys().copied().filter(|id| !live.contains(id)).collect();
    for id in gone {
        if let Some((e, _)) = st.bld.remove(&id) {
            commands.entity(e).despawn();
        }
    }
    // The founding lodge lives in the colony, not the building list.
    let lodge_blds: Vec<(usize, usize)> = sim.buildings.buildings.iter().filter(|b| b.kind == BuildingKind::Lodge).map(|b| b.pos).collect();
    for l in &sim.colony.lodges {
        if st.lodges.contains_key(&l.id.0) || lodge_blds.iter().any(|p| (p.0 as f32 + 1.5 - l.pos.0).abs() < 3.0 && (p.1 as f32 + 1.5 - l.pos.1).abs() < 3.0) {
            continue;
        }
        let c = cell_xz(sim, l.pos);
        let y = (0..9).map(|i| height_at(&sim.height, c.x + (i % 3) as f32 - 1.0, c.y + (i / 3) as f32 - 1.0)).fold(f32::MAX, f32::min);
        let e = spawn(&mut commands, &mut models, StructKind::Lodge, 6, 3, Transform::from_xyz(c.x, y, c.y).with_scale(Vec3::splat(WORLD_SCALE)));
        st.lodges.insert(l.id.0, e);
    }
}
