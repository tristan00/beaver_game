use super::structures::{DamStage, DAM_STEP};
use super::scene::{height_at, Species, StructKind};
use super::sim_view::*;
use super::{models, BeaverSprite, CurrentSeason, Models, SimBeaver, SimEntity, Sway, TreeLod, WheelSpin};
use crate::sim_driver::SimClock;
use crate::state::Sim;
use crate::picking::Target;
use bevy::prelude::*;
use sim::buildings::{BuildingId, BuildingKind, Status};
use sim::resources::{ResourceId, ResourceKind};
use sim::worldgen::Tree;
use std::collections::{HashMap, HashSet};

#[derive(Resource, Default)]
pub struct PendingDays(pub f32);

pub fn update_pending(clock: Option<Res<SimClock>>, mut p: ResMut<PendingDays>) {
    p.0 = clock.map_or(0.0, |c| c.pending_days());
}

#[derive(Component, Default)]
pub struct Motion {
    samples: std::collections::VecDeque<(f64, Vec2, f32)>,
    instance: u64,
    observed_time: Option<f64>,
}

pub const BEAVER_SIZE: f32 = WORLD_SCALE * 0.9;
pub const TREE_SCALE: f32 = 3.5;
pub const BUILDING_SIZE: f32 = WORLD_SCALE * 0.82;

const SPREAD: f32 = 0.28;

fn spread(id: u64, game: bool) -> Vec3 {
    if !game {
        return Vec3::ZERO;
    }
    let a = (id as f32 * 2.399_963).rem_euclid(std::f32::consts::TAU);
    Vec3::new(a.cos(), 0.0, a.sin()) * SPREAD
}

impl Motion {
    fn record(&mut self, t: f64, xz: Vec2, lift: f32) {
        match self.samples.back_mut() {
            Some(b) if b.0 == t => *b = (t, xz, lift),
            Some(b) if b.0 > t => return,
            _ => self.samples.push_back((t, xz, lift)),
        }
        while self.samples.len() > 256 {
            self.samples.pop_front();
        }
    }

    fn observe(&mut self, sim: &sim::SimState, ag: &sim::agents::Agent, game: bool) {
        let offset = spread(ag.id.0, game);
        let p = agent_world_pos(sim, ag, 0.0) + offset;
        let ground = |xz: Vec2| height_at(&sim.height, xz.x, xz.y);
        if self.instance != sim.instance {
            self.samples.clear();
            self.observed_time = None;
            self.instance = sim.instance;
        }
        if self.observed_time.is_some_and(|t| t >= sim.time_days) { return; }
        if let (Some(previous), Some(&(_, mut point, _))) = (self.observed_time, self.samples.back()) {
            // Trails describe only the last simulation step. A skipped snapshot
            // interpolates between the known endpoints instead of inventing or
            // replaying intermediate trips.
            if sim.time_days - previous <= sim::STEP_DAYS as f64 + 1e-8 {
                let pace = sim::agents::MOVE_COST_PER_DAY
                    * if ag.load.is_some() { sim::agents::LADEN_PACE } else { 1.0 };
                let mut t = previous;
                for &(cell, gait) in &ag.trail {
                    let next = cell_xz(sim, (cell.0 as f32 + 0.5, cell.1 as f32 + 0.5)) + offset.xz();
                    let cost = sim.nav.surface_at(cell).and_then(sim::nav::NavGrid::step_cost).map_or(gait_cost(gait), |c| c.0);
                    let cells = point.distance(next) / crate::terrain_render::CELL_SIZE;
                    t = (t + (cells * cost / pace) as f64).min(sim.time_days);
                    self.record(t, next, stand_y(sim, cell) - ground(next));
                    point = next;
                }
            }
        }
        self.record(sim.time_days, p.xz(), p.y - ground(p.xz()));
        self.observed_time = Some(sim.time_days);
    }

    fn at(&mut self, t: f64) -> Option<(Vec2, f32, Option<Vec2>)> {
        while self.samples.len() > 2 && self.samples[1].0 <= t {
            self.samples.pop_front();
        }
        let a = *self.samples.front()?;
        let Some(&b) = self.samples.get(1) else { return Some((a.1, a.2, None)) };
        let k = (((t - a.0) / (b.0 - a.0).max(1e-9)) as f32).clamp(0.0, 1.0);
        let d = b.1 - a.1;
        Some((a.1.lerp(b.1, k), a.2 + (b.2 - a.2) * k, (t < b.0 && d.length() > 1e-3).then_some(d)))
    }
}

#[cfg(test)]
mod motion_tests {
    use super::*;

    #[test]
    fn sparse_snapshots_interpolate_instead_of_snapping_across_distance() {
        let mut motion = Motion::default();
        motion.record(0.0, Vec2::ZERO, 0.0);
        motion.record(1.0, Vec2::new(10.0, 0.0), 2.0);
        let (point, height, direction) = motion.at(0.5).unwrap();
        assert_eq!(point, Vec2::new(5.0, 0.0));
        assert_eq!(height, 1.0);
        assert!(direction.is_some());
    }

    #[test]
    fn a_completed_trail_is_recorded_once_with_an_authoritative_endpoint() {
        let mut sim = sim::SimState::new(1);
        let id = sim::colony::BeaverId(1);
        sim.agents.spawn(id, (2, 2));
        let mut agent = sim.agents.get(id).unwrap().clone();
        sim.nav.set((3, 2), sim::nav::Surface::Water);
        sim.nav.set((3, 3), sim::nav::Surface::Water);
        let mut motion = Motion::default();
        motion.observe(&sim, &agent, false);
        agent.cell = (3, 3);
        agent.trail = vec![((3, 2), sim::nav::Gait::Swim), ((3, 3), sim::nav::Gait::Swim)];
        sim.time_days = sim::STEP_DAYS as f64;
        motion.observe(&sim, &agent, false);
        let original = motion.samples.clone();
        for _ in 0..60 { motion.observe(&sim, &agent, false); }
        assert_eq!(motion.samples, original);
        assert_eq!(motion.samples.back().unwrap().0, sim.time_days);
        assert!(motion.samples.iter().zip(motion.samples.iter().skip(1)).all(|(a,b)|a.0 < b.0));
        let corner_time = sim::nav::SWIM_COST as f64 / sim::agents::MOVE_COST_PER_DAY as f64;
        let corner = motion.at(corner_time).unwrap().0;
        assert!(corner.distance(cell_xz(&sim, (3.5, 2.5))) < 1e-4);
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn sync_sim_beavers(
    mut commands: Commands,
    sim: Res<Sim>,
    pending: Res<PendingDays>,
    clock: Option<Res<SimClock>>,
    mut q: Query<(Entity, &mut Transform, &mut BeaverSprite, &mut Visibility, &mut Motion), With<SimBeaver>>,
    mut seen: Local<HashSet<u64>>,
) {
    seen.clear();
    let now = clock.as_ref().map_or(sim.0.time_days + pending.0 as f64, |c| c.render_days(sim.0.time_days));
    for (e, mut tr, mut sprite, mut vis, mut motion) in &mut q {
        let Some(ag) = sim.0.agents.get(sim::colony::BeaverId(sprite.id)) else {
            commands.entity(e).despawn();
            continue;
        };
        seen.insert(sprite.id);
        let p = agent_world_pos(&sim.0, ag, 0.0) + spread(sprite.id, clock.is_some());
        let ground = |xz: Vec2| height_at(&sim.0.height, xz.x, xz.y);
        motion.observe(&sim.0, ag, clock.is_some());
        let heading = match motion.at(now) {
            Some((xz, lift, dir)) => {
                tr.translation = Vec3::new(xz.x, ground(xz) + lift, xz.y);
                dir
            }
            None => {
                tr.translation = p;
                None
            }
        };
        sprite.pose = if heading.is_some() {
            use super::scene::Pose;
            match ag.gait {
                sim::nav::Gait::Swim => Pose::Swim,
                sim::nav::Gait::Climb => Pose::Climb,
                sim::nav::Gait::Walk if ag.load.is_some() => Pose::Carry,
                sim::nav::Gait::Walk => Pose::Walk,
            }
        } else { pose_of(&sim.0, ag) };
        sprite.load = load_of(ag);
        sprite.wet = is_wet(&sim.0, ag);
        if let Some(d) = heading {
            sprite.yaw = d.x.atan2(d.y);
        } else if let Some(y) = facing_of(&sim.0, ag) {
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
        let mut motion = Motion::default();
        motion.observe(&sim.0, ag, clock.is_some());
        commands.spawn((
            SimBeaver,
            motion,
            BeaverSprite { id: id.0, pose: pose_of(&sim.0, ag), load: load_of(ag), yaw, wet: is_wet(&sim.0, ag) },
            Transform::from_translation(agent_world_pos(&sim.0, ag, 0.0) + spread(id.0, clock.is_some())).with_rotation(Quat::from_rotation_y(yaw)).with_scale(Vec3::splat(BEAVER_SIZE)),
            if ag.inside { Visibility::Hidden } else { Visibility::Inherited },
        ));
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ResSig {
    Tree(u8),
    Felled(u8, bool),
    Stock(ResourceKindTag, u8),
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ResourceKindTag {
    Mud,
    Plant,
}

pub fn res_sig(r: &sim::resources::Resource) -> ResSig {
    let q = |n: f32| ((r.amount / r.max_amount.max(1e-3)).clamp(0.0, 1.0) * n).ceil() as u8;
    match r.kind {
        ResourceKind::Tree => ResSig::Tree(r.stage().map_or(2, |s| s as u8)),
        ResourceKind::Felled => ResSig::Felled(r.amount.ceil().clamp(0.0, 3.0) as u8, r.branches > 0.0),
        ResourceKind::MudBank => ResSig::Stock(ResourceKindTag::Mud, q(8.0)),
        ResourceKind::RiverClay => ResSig::Stock(ResourceKindTag::Mud, q(8.0).max(1)),
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
    seed: Option<(u64, u32)>,
    time: f64,
    res: HashMap<ResourceId, (Entity, ResSig)>,
    bld: HashMap<BuildingId, (Entity, (BuildingKind, (usize, usize), u8))>,
    anchors: HashMap<BuildingId, f32>,
    lodges: HashMap<u64, Entity>,
}

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
    let tree = TREE_SCALE;
    match sig {
        ResSig::Tree(_) if r.snag => (models.snag.clone(), Vec3::splat(tree * hash_scale), None),
        ResSig::Tree(_) => {
            let sp = species_at(sim, c);
            let st = r.stage().unwrap_or(sim::forestry::TreeStage::Mature);
            (models.tree(sp, season, 0), stage_scale(st) * tree * hash_scale, Some(TreeLod { species: sp, lod: 0, season, cutaway: false }))
        }
        ResSig::Felled(logs, branches) => (models.felled[logs as usize * 2 + branches as usize].clone(), Vec3::splat(k), None),
        ResSig::Stock(tag, q) => {
            let n = if tag == ResourceKindTag::Plant { 4.0 } else { 8.0 };
            let s = 0.45 + 0.55 * (q as f32 / n);
            let (mesh, base) = match tag {
                ResourceKindTag::Mud => (models.mud.clone(), 0.62),
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

fn material_for(models: &Models, r: &sim::resources::Resource) -> Handle<StandardMaterial> {
    if r.stage() == Some(sim::forestry::TreeStage::Old) { models.old_tree.clone() }
    else if r.kind == ResourceKind::Tree {
        let variant = (hash01(r.cell.0, r.cell.1, 17) * 4.0) as usize;
        models.tree_tints[variant.min(3)].clone()
    } else { models.white.clone() }
}

pub(super) fn sync_sim_resources(
    mut commands: Commands,
    sim: Res<Sim>,
    season: Res<CurrentSeason>,
    models: Option<Res<Models>>,
    flow: Option<Res<crate::flow::Flow>>, mut st: Local<WorldSync>,
) {
    let Some(models) = models else { return };
    let generation = flow.map_or(0, |f| f.generation);
    let sim = &sim.0;
    if st.seed != Some((sim.seed, generation)) {
        for (e, _) in st.res.drain().map(|(_, v)| v) {
            commands.entity(e).despawn();
        }
        st.seed = Some((sim.seed, generation));
        st.time = f64::NEG_INFINITY;
    }
    if sim.time_days == st.time && !st.res.is_empty() {
        return;
    }
    st.time = sim.time_days;
    let mut live: HashSet<ResourceId> = HashSet::with_capacity(sim.resources.items.len());
    let no_trees = std::env::var_os("BC_NO_TREES").is_some();
    for r in sim.resources.items.iter().filter(|r| !(no_trees && r.kind == ResourceKind::Tree)) {
        if r.kind == ResourceKind::Plant && (
            sim.water.depth.get(r.cell.0, r.cell.1).is_some_and(|&d| d > 0.003)
            || sim.water.riverbed.get(r.cell.0, r.cell.1).is_some_and(|&b| b > 0.2)
        ) {
            continue;
        }
        live.insert(r.id);
        let sig = res_sig(r);
        match st.res.get(&r.id).copied() {
            Some((_, old)) if old == sig => {}
            Some((e, _)) => {
                let (mesh, scale, lod) = res_visual(&models, sim, r, season.0);
                let mut ec = commands.entity(e);
                let tr = res_transform(sim, r, scale);
                ec.insert((Mesh3d(mesh), MeshMaterial3d(material_for(&models, r)), tr));
                match lod {
                    Some(l) => ec.insert((l, Sway(tr.rotation))),
                    None => ec.remove::<(TreeLod, Sway)>(),
                };
                st.res.insert(r.id, (e, sig));
            }
            None => {
                let (mesh, scale, lod) = res_visual(&models, sim, r, season.0);
                let tr = res_transform(sim, r, scale);
                let mut ec = commands.spawn((SimEntity, Mesh3d(mesh), MeshMaterial3d(material_for(&models, r)), tr));
                if let Some(l) = lod {
                    ec.insert((l, Sway(tr.rotation)));
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

pub fn model_kind(k: BuildingKind) -> StructKind {
    match k {
        BuildingKind::Dam => StructKind::Dam,
        BuildingKind::Shelter => StructKind::Lodge,
        BuildingKind::Warehouse => StructKind::Warehouse,
    }
}

pub fn build_progress(sim: &sim::SimState, b: &sim::buildings::Building) -> f32 {
    if b.status == Status::Complete { 1.0 } else { sim.work.progress(b.id).unwrap_or(0.0).clamp(0.0, 0.999) }
}

#[derive(Default)]
pub struct ReinforcementSync {
    generation: Option<(u64,u32)>,
    drawn: HashMap<BuildingId,(Entity,u64)>,
    meshes: HashMap<BuildingKind,Handle<Mesh>>,
}

pub(super) fn sync_reinforcements(mut commands:Commands,sim:Res<Sim>,flow:Res<crate::flow::Flow>,models:Option<Res<Models>>,
    mut meshes:ResMut<Assets<Mesh>>,mut state:Local<ReinforcementSync>) {
    let Some(models)=models else{return};let s=&sim.0;
    let generation=(s.seed,flow.generation);
    if state.generation!=Some(generation) {
        for (_, (e,_)) in state.drawn.drain(){commands.entity(e).despawn();}
        state.generation=Some(generation);
    }
    let mut live=HashSet::new();
    for b in s.buildings.buildings.iter().filter(|b|b.reinforced) {
        live.insert(b.id);
        let signature=0u64;
        if state.drawn.get(&b.id).is_some_and(|v|v.1==signature){continue;}
        if let Some((e,_))=state.drawn.remove(&b.id){commands.entity(e).despawn();}
        let handle=state.meshes.entry(b.kind).or_insert_with(||meshes.add(models::reinforcement_mesh(model_kind(b.kind)))).clone();
        let (mesh,tr)=(handle,building_transform(s,b,None));
        let entity=commands.spawn((Target::Building(b.id),SimEntity,Mesh3d(mesh),MeshMaterial3d(models.white.clone()),tr)).id();
        state.drawn.insert(b.id,(entity,signature));
    }
    let gone:Vec<_>=state.drawn.keys().copied().filter(|id|!live.contains(id)).collect();
    for id in gone {if let Some((e,_))=state.drawn.remove(&id){commands.entity(e).despawn();}}
}

fn building_transform(sim: &sim::SimState, b: &sim::buildings::Building, anchor: Option<f32>) -> Transform {
    let (w, h) = b.kind.spec().footprint;
    let c = cell_xz(sim, (b.pos.0 as f32 + w as f32 / 2.0, b.pos.1 as f32 + h as f32 / 2.0));
    let ground = |p: Vec2| height_at(&sim.height, p.x, p.y);
    let grounds: Vec<f32> = b.cells().map(|c| ground(cell_xz(sim, (c.0 as f32 + 0.5, c.1 as f32 + 0.5)))).collect();
    let lowest = grounds.iter().copied().fold(f32::MAX, f32::min);
    let on_water = false;
    let y = if on_water {
        stand_y(sim, b.cells().next().unwrap_or(b.pos)).max(lowest)
    } else if models::footing_of(model_kind(b.kind)).is_some() {
        let mean = grounds.iter().sum::<f32>() / grounds.len().max(1) as f32;
        let surface = b
            .cells()
            .filter_map(|(x, y)| Some(sim.height.get(x, y)? + sim.water.depth.get(x, y).copied().filter(|d| *d > 0.02)?))
            .fold(f32::MIN, f32::max);
        let dry = if surface == f32::MIN { grounds.iter().copied().fold(f32::MIN, f32::max) - 0.05 } else { f32::MIN };
        mean.max(surface + 0.08).max(dry).max(anchor.unwrap_or(f32::MIN))
    } else {
        lowest
    };
    Transform::from_xyz(c.x, y, c.y).with_scale(Vec3::splat(BUILDING_SIZE))
}

pub(super) fn sync_sim_buildings(mut commands: Commands, sim: Res<Sim>, models: Option<ResMut<Models>>, mut meshes: ResMut<Assets<Mesh>>, flow: Option<Res<crate::flow::Flow>>, mut st: Local<WorldSync>) {
    let Some(mut models) = models else { return };
    let changed = sim.is_changed();
    let generation = flow.map_or(0, |f| f.generation);
    let sim = &sim.0;
    if st.seed != Some((sim.seed, generation)) {
        for (e, _) in st.bld.drain().map(|(_, v)| v) {
            commands.entity(e).despawn();
        }
        for (_, e) in st.lodges.drain() {
            commands.entity(e).despawn();
        }
        st.anchors.clear();
        st.seed = Some((sim.seed, generation));
        st.time = f64::NEG_INFINITY;
    }
    if sim.time_days == st.time && !changed && !(st.bld.is_empty() && st.lodges.is_empty()) {
        return;
    }
    st.time = sim.time_days;
    let white = models.white.clone();
    let mut live = HashSet::new();
    let mut spawn = |commands: &mut Commands, models: &mut Models, target: Target, kind: StructKind, len2: u32, stage: u8, tr: Transform| -> Entity {
        let mesh = models.structs.entry((kind, len2, stage)).or_insert_with(|| meshes.add(models::struct_mesh(kind, len2 as f32 * 0.5, stage))).clone();
        let wheel = (kind == StructKind::WaterWheel && stage == 3).then(|| models.wheel.clone());
        let mut e = commands.spawn((target, SimEntity, Mesh3d(mesh), MeshMaterial3d(white.clone()), tr));
        if let Some(w) = wheel {
            let w2 = white.clone();
            e.with_children(|p| {
                p.spawn((target, Mesh3d(w), MeshMaterial3d(w2), Transform::from_xyz(0.0, 2.0, 0.0), WheelSpin(0.9)));
            });
        }
        e.id()
    };
    for b in &sim.buildings.buildings {
        if b.kind == BuildingKind::Dam && (b.is_complete() || sim.work.dam_plans.iter().any(|p| p.building == b.id)) {
            continue;
        }
        live.insert(b.id);
        let stage = if sim.buildings.stranded(b, &sim.water.depth) { models::DRY_STAGE } else { models::stage_of(build_progress(sim, b)) };
        let sig = (b.kind, b.pos, stage);
        if st.bld.get(&b.id).is_some_and(|(_, s)| *s == sig) {
            continue;
        }
        if let Some((old, _)) = st.bld.remove(&b.id) {
            commands.entity(old).despawn();
        }
        let (w, _) = b.kind.spec().footprint;
        let tr = building_transform(sim, b, st.anchors.get(&b.id).copied());
        st.anchors.insert(b.id, tr.translation.y);
        let e = spawn(&mut commands, &mut models, Target::Building(b.id), model_kind(b.kind), (w as u32 * 2).max(2), stage, tr);
        st.bld.insert(b.id, (e, sig));
    }
    let gone: Vec<BuildingId> = st.bld.keys().copied().filter(|id| !live.contains(id)).collect();
    for id in gone {
        if let Some((e, _)) = st.bld.remove(&id) {
            commands.entity(e).despawn();
        }
    }
    let homes: HashSet<u64> = sim.colony.lodges.iter().map(|l| l.id.0).collect();
    let stale: Vec<u64> = st.lodges.keys().copied().filter(|id| !homes.contains(id)).collect();
    for id in stale {
        if let Some(e) = st.lodges.remove(&id) {
            commands.entity(e).despawn();
        }
    }
    let lodge_blds: Vec<(usize, usize)> = sim.buildings.buildings.iter().filter(|b| b.kind == BuildingKind::Shelter).map(|b| b.pos).collect();
    for l in &sim.colony.lodges {
        if st.lodges.contains_key(&l.id.0) || lodge_blds.iter().any(|p| (p.0 as f32 + 1.5 - l.pos.0).abs() < 3.0 && (p.1 as f32 + 1.5 - l.pos.1).abs() < 3.0) {
            continue;
        }
        let c = cell_xz(sim, l.pos);
        let y = (0..9).map(|i| height_at(&sim.height, c.x + (i % 3) as f32 - 1.0, c.y + (i / 3) as f32 - 1.0)).fold(f32::MAX, f32::min);
        let target = Target::home(sim, l.id);
        let e = spawn(&mut commands, &mut models, target, StructKind::Lodge, 6, 3, Transform::from_xyz(c.x, y, c.y).with_scale(Vec3::splat(BUILDING_SIZE)));
        st.lodges.insert(l.id.0, e);
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct DamCellVis {
    pub steps: u8,
    pub dir: u8,
    pub stage: DamStage,
}

/// Every dam cell as it should look: staked, rising beside its pile, or done.
/// A worn cell is drawn as low as the water now finds it.
pub fn dam_cells(sim: &sim::SimState) -> HashMap<(usize, usize), DamCellVis> {
    let (w, h) = (sim.water.dam_height.width(), sim.water.dam_height.height());
    let mut cells: HashMap<(usize, usize), (f32, DamStage)> = HashMap::new();
    let integrity = |c: (usize, usize)| sim.water.dam_integrity.get(c.0, c.1).copied().unwrap_or(1.0).clamp(0.0, 1.0);
    for (i, &d) in sim.water.dam_height.as_slice().iter().enumerate() {
        if d > 0.0 {
            let c = (i % w, i / w);
            cells.insert(c, (d * integrity(c), DamStage::Done));
        }
    }
    for d in sim.buildings.buildings.iter().filter_map(|b| b.dam.as_deref()) {
        for c in d.cells.iter().filter(|c| c.target > 0.0) {
            let now = sim.water.dam_height.get(c.cell.0, c.cell.1).copied().unwrap_or(0.0);
            if now <= 1e-3 {
                cells.insert(c.cell, (DAM_STEP, DamStage::Staked));
            } else if now < c.target - 1e-3 {
                cells.insert(c.cell, (now * integrity(c.cell), DamStage::Rising));
            }
        }
    }
    let is_dam = |c: (usize, usize), dx: i32, dy: i32| {
        let (x, y) = (c.0 as i64 + dx as i64, c.1 as i64 + dy as i64);
        x >= 0 && y >= 0 && (x as usize) < w && (y as usize) < h && cells.contains_key(&(x as usize, y as usize))
    };
    cells
        .iter()
        .map(|(&c, &(d, stage))| {
            let steps = (d / DAM_STEP).round().clamp(1.0, 16.0) as u8;
            (c, DamCellVis { steps, dir: super::structures::dam_dir(|dx, dy| is_dam(c, dx, dy)), stage })
        })
        .collect()
}

#[derive(Default)]
pub struct DamSync {
    seed: Option<(u64, u32)>,
    time: f64,
    cells: HashMap<(usize, usize), (Entity, DamCellVis)>,
}

pub(super) fn sync_sim_dams(mut commands: Commands, sim: Res<Sim>, models: Option<ResMut<Models>>, mut meshes: ResMut<Assets<Mesh>>, flow: Option<Res<crate::flow::Flow>>, mut st: Local<DamSync>) {
    let Some(mut models) = models else { return };
    let generation = flow.map_or(0, |f| f.generation);
    let changed = sim.is_changed();
    let sim = &sim.0;
    if st.seed != Some((sim.seed, generation)) {
        for (e, _) in st.cells.drain().map(|(_, v)| v) {
            commands.entity(e).despawn();
        }
        st.seed = Some((sim.seed, generation));
        st.time = f64::NEG_INFINITY;
    }
    if sim.time_days == st.time && !changed {
        return;
    }
    st.time = sim.time_days;
    let want = dam_cells(sim);
    let gone: Vec<(usize, usize)> = st.cells.iter().filter(|(c, (_, v))| want.get(*c) != Some(v)).map(|(c, _)| *c).collect();
    for c in gone {
        if let Some((e, _)) = st.cells.remove(&c) {
            commands.entity(e).despawn();
        }
    }
    let white = models.white.clone();
    for (&c, &vis) in &want {
        if st.cells.contains_key(&c) {
            continue;
        }
        let variant = (hash01(c.0, c.1, 21) * 3.0) as u32 % 3;
        let diag = vis.dir % 2 == 1;
        let mesh = models
            .dams
            .entry((vis.steps, diag, variant, vis.stage))
            .or_insert_with(|| meshes.add(super::structures::dam_cell_mesh(vis.steps, diag, variant, vis.stage)))
            .clone();
        let p = cell_xz(sim, (c.0 as f32 + 0.5, c.1 as f32 + 0.5));
        let y = height_at(&sim.height, p.x, p.y);
        let yaw = -(vis.dir as f32) * std::f32::consts::FRAC_PI_4;
        let e = commands.spawn((SimEntity, Mesh3d(mesh), MeshMaterial3d(white.clone()), Transform::from_xyz(p.x, y, p.y).with_rotation(Quat::from_rotation_y(yaw)))).id();
        let owner = sim.buildings.buildings.iter().rev().find(|b| b.kind == BuildingKind::Dam && b.cells().any(|cell| cell == c)).map(|b| b.id);
        if let Some(id) = owner { commands.entity(e).insert(Target::Building(id)); }
        st.cells.insert(c, (e, vis));
    }
}

/// While a dam is being drawn, each cell shows as a see-through wall at the
/// height its first stage will reach; red when the line cannot be built.
#[derive(Default)]
pub struct GhostSync {
    mats: Option<(Handle<StandardMaterial>, Handle<StandardMaterial>)>,
    cells: HashMap<(usize, usize), (Entity, (u8, u8, bool))>,
}

#[allow(clippy::too_many_arguments)]
pub(super) fn sync_dam_ghosts(
    mut commands: Commands,
    sim: Res<Sim>,
    hud: Option<Res<crate::hud::HudState>>,
    models: Option<ResMut<Models>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut st: Local<GhostSync>,
) {
    let Some(mut models) = models else { return };
    let sim = &sim.0;
    let mut want: HashMap<(usize, usize), (u8, u8, bool)> = HashMap::new();
    if let Some(plan) = hud.as_ref().filter(|h| h.tool == crate::hud::Tool::Dam).and_then(|h| h.plan.as_ref().filter(|p| p.line == h.line)) {
        let bad = plan.refusal.is_some();
        let set: HashSet<(usize, usize)> = plan.targets.iter().map(|t| t.0).collect();
        let near = |c: (usize, usize), dx: i32, dy: i32| {
            let (x, y) = (c.0 as i64 + dx as i64, c.1 as i64 + dy as i64);
            x >= 0 && y >= 0 && (set.contains(&(x as usize, y as usize)) || sim.water.dam_height.get(x as usize, y as usize).is_some_and(|h| *h > 0.0))
        };
        for &(c, t) in &plan.targets {
            let steps = (t.max(DAM_STEP) / DAM_STEP).round().clamp(1.0, 16.0) as u8;
            want.insert(c, (steps, super::structures::dam_dir(|dx, dy| near(c, dx, dy)), bad));
        }
    }
    let stale: Vec<(usize, usize)> = st.cells.iter().filter(|(c, (_, k))| want.get(*c) != Some(k)).map(|(c, _)| *c).collect();
    for c in stale {
        if let Some((e, _)) = st.cells.remove(&c) {
            commands.entity(e).despawn();
        }
    }
    if want.is_empty() {
        return;
    }
    let (ok, no) = st
        .mats
        .get_or_insert_with(|| {
            let mut m = |c: Color| materials.add(StandardMaterial { base_color: c, alpha_mode: AlphaMode::Blend, perceptual_roughness: 1.0, ..default() });
            (m(Color::srgba(1.0, 1.0, 1.0, 0.7)), m(Color::srgba(1.0, 0.42, 0.36, 0.7)))
        })
        .clone();
    for (&c, &(steps, dir, bad)) in &want {
        if st.cells.contains_key(&c) {
            continue;
        }
        let diag = dir % 2 == 1;
        let mesh = models.dams.entry((steps, diag, 1, DamStage::Done)).or_insert_with(|| meshes.add(super::structures::dam_cell_mesh(steps, diag, 1, DamStage::Done))).clone();
        let p = cell_xz(sim, (c.0 as f32 + 0.5, c.1 as f32 + 0.5));
        // A ghost in a deep pond would hide under the water: keep its top showing.
        let ground = height_at(&sim.height, p.x, p.y);
        let surface = ground + sim.water.depth.get(c.0, c.1).copied().unwrap_or(0.0);
        let y = ground.max(surface + 0.15 - steps as f32 * DAM_STEP);
        let yaw = -(dir as f32) * std::f32::consts::FRAC_PI_4;
        let e = commands
            .spawn((SimEntity, Mesh3d(mesh), MeshMaterial3d(if bad { no.clone() } else { ok.clone() }), Transform::from_xyz(p.x, y, p.y).with_rotation(Quat::from_rotation_y(yaw))))
            .id();
        st.cells.insert(c, (e, (steps, dir, bad)));
    }
}
