use super::scene::{height_at, Species, StructKind};
use super::sim_view::*;
use super::{models, BeaverSprite, CurrentSeason, Models, SimBeaver, SimEntity, Sway, TreeLod, WheelSpin};
use crate::cli::Cli;
use crate::sim_driver::SimClock;
use crate::state::Sim;
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
}

pub const BEAVER_SIZE: f32 = WORLD_SCALE * 0.9;
pub const BUILDING_SIZE: f32 = WORLD_SCALE * 0.82;

const SPREAD: f32 = 0.28;

fn spread(id: u64, game: bool) -> Vec3 {
    if !game {
        return Vec3::ZERO;
    }
    let a = (id as f32 * 2.399_963).rem_euclid(std::f32::consts::TAU);
    Vec3::new(a.cos(), 0.0, a.sin()) * SPREAD
}

const SNAP_DISTANCE: f32 = 4.0;

impl Motion {
    fn record(&mut self, t: f64, xz: Vec2, lift: f32) {
        match self.samples.back_mut() {
            Some(b) if b.0 == t => *b = (t, xz, lift),
            Some(b) if b.1.distance(xz) > SNAP_DISTANCE => {
                self.samples.clear();
                self.samples.push_back((t, xz, lift));
            }
            _ => self.samples.push_back((t, xz, lift)),
        }
        while self.samples.len() > 256 {
            self.samples.pop_front();
        }
    }

    fn at(&mut self, t: f64) -> Option<(Vec2, f32, Option<Vec2>)> {
        while self.samples.len() > 2 && self.samples[1].0 <= t {
            self.samples.pop_front();
        }
        let a = *self.samples.front()?;
        let Some(&b) = self.samples.get(1) else { return Some((a.1, a.2, None)) };
        let k = (((t - a.0) / (b.0 - a.0).max(1e-9)) as f32).clamp(0.0, 1.0);
        let d = b.1 - a.1;
        Some((a.1.lerp(b.1, k), a.2 + (b.2 - a.2) * k, (d.length() > 1e-3).then_some(d)))
    }
}

#[allow(clippy::too_many_arguments)]
pub fn sync_sim_beavers(
    mut commands: Commands,
    cli: Res<Cli>,
    sim: Res<Sim>,
    pending: Res<PendingDays>,
    clock: Option<Res<SimClock>>,
    mut q: Query<(Entity, &mut Transform, &mut BeaverSprite, &mut Visibility, &mut Motion), With<SimBeaver>>,
    mut seen: Local<HashSet<u64>>,
) {
    if super::stub_mode(&cli) {
        return;
    }
    seen.clear();
    let delay = if clock.is_some() { sim::STEP_DAYS as f64 } else { 0.0 };
    let pending = clock.as_ref().map_or(pending.0, |c| c.pending_days());
    let now = sim.0.time_days + pending as f64 - delay;
    for (e, mut tr, mut sprite, mut vis, mut motion) in &mut q {
        let Some(ag) = sim.0.agents.get(sim::colony::BeaverId(sprite.id)) else {
            commands.entity(e).despawn();
            continue;
        };
        seen.insert(sprite.id);
        let p = agent_world_pos(&sim.0, ag, 0.0) + spread(sprite.id, clock.is_some());
        let ground = |xz: Vec2| height_at(&sim.0.height, xz.x, xz.y);
        let mut end = sim.0.time_days;
        if motion.samples.back().is_none_or(|b| b.0 < sim.0.time_days) && !ag.trail.is_empty() {
            let pace = sim::agents::MOVE_COST_PER_DAY * if ag.load.is_some() { sim::agents::LADEN_PACE } else { 1.0 };
            let mut t = sim.0.time_days - sim::STEP_DAYS as f64;
            for &(c, gait) in &ag.trail {
                t = (t + (super::sim_view::gait_cost(gait) / pace) as f64).min(sim.0.time_days);
                let q = super::sim_view::cell_xz(&sim.0, (c.0 as f32 + 0.5, c.1 as f32 + 0.5)) + spread(sprite.id, clock.is_some()).xz();
                motion.record(t, q, super::sim_view::stand_y(&sim.0, c) - ground(q));
            }
            end = t;
        }
        let last = motion.samples.back().map_or(end, |b| b.0);
        motion.record(end.max(last), p.xz(), p.y - ground(p.xz()));
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
        sprite.pose = pose_of(&sim.0, ag);
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
        commands.spawn((
            SimBeaver,
            Motion::default(),
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

fn material_for(models: &Models, r: &sim::resources::Resource) -> Handle<StandardMaterial> {
    if r.stage() == Some(sim::forestry::TreeStage::Old) { models.old_tree.clone() } else { models.white.clone() }
}

pub fn sync_sim_resources(
    mut commands: Commands,
    cli: Res<Cli>,
    sim: Res<Sim>,
    season: Res<CurrentSeason>,
    models: Option<Res<Models>>,
    flow: Option<Res<crate::flow::Flow>>, mut st: Local<WorldSync>,
) {
    let Some(models) = models else { return };
    if super::stub_mode(&cli) {
        return;
    }
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
        BuildingKind::Lock => StructKind::Lock,
        BuildingKind::Lodge => StructKind::Lodge,
        BuildingKind::Burrow => StructKind::Burrow,
        BuildingKind::FoodCache => StructKind::FoodCache,
        BuildingKind::ScentMound => StructKind::ScentMound,
        BuildingKind::GuardPost => StructKind::GuardPost,
        BuildingKind::ForagerHut => StructKind::ForagerHut,
        BuildingKind::MudPit => StructKind::MudPit,
        BuildingKind::TreeNursery => StructKind::TreeNursery,
        BuildingKind::Canal => StructKind::Canal,
        BuildingKind::WaterWheel => StructKind::WaterWheel,
        BuildingKind::Mill => StructKind::Mill,
        BuildingKind::Workshop => StructKind::Workshop,
        BuildingKind::Warehouse => StructKind::Warehouse,
        BuildingKind::Storehouse => StructKind::Storehouse,
        BuildingKind::LoggingCamp => StructKind::LoggingCamp,
        BuildingKind::QuarryCamp => StructKind::QuarryCamp,
        BuildingKind::ForesterLodge => StructKind::ForesterLodge,
        BuildingKind::Dock => StructKind::Dock,
        BuildingKind::Barge => StructKind::Barge,
    }
}

pub fn build_progress(sim: &sim::SimState, b: &sim::buildings::Building) -> f32 {
    if b.status == Status::Complete { 1.0 } else { sim.work.progress(b.id).unwrap_or(0.0).clamp(0.0, 0.999) }
}

fn building_transform(sim: &sim::SimState, b: &sim::buildings::Building, anchor: Option<f32>) -> Transform {
    let (w, h) = b.kind.spec().footprint;
    let c = cell_xz(sim, (b.pos.0 as f32 + w as f32 / 2.0, b.pos.1 as f32 + h as f32 / 2.0));
    let ground = |p: Vec2| height_at(&sim.height, p.x, p.y);
    let grounds: Vec<f32> = b.cells().map(|c| ground(cell_xz(sim, (c.0 as f32 + 0.5, c.1 as f32 + 0.5)))).collect();
    let lowest = grounds.iter().copied().fold(f32::MAX, f32::min);
    let on_water = matches!(b.kind, BuildingKind::Dock | BuildingKind::Barge | BuildingKind::WaterWheel);
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

pub fn sync_sim_buildings(mut commands: Commands, cli: Res<Cli>, sim: Res<Sim>, models: Option<ResMut<Models>>, mut meshes: ResMut<Assets<Mesh>>, flow: Option<Res<crate::flow::Flow>>, mut st: Local<WorldSync>) {
    let Some(mut models) = models else { return };
    if super::stub_mode(&cli) {
        return;
    }
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
        let e = spawn(&mut commands, &mut models, model_kind(b.kind), (w as u32 * 2).max(2), stage, tr);
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
    let lodge_blds: Vec<(usize, usize)> = sim.buildings.buildings.iter().filter(|b| b.kind == BuildingKind::Lodge).map(|b| b.pos).collect();
    for l in &sim.colony.lodges {
        if st.lodges.contains_key(&l.id.0) || lodge_blds.iter().any(|p| (p.0 as f32 + 1.5 - l.pos.0).abs() < 3.0 && (p.1 as f32 + 1.5 - l.pos.1).abs() < 3.0) {
            continue;
        }
        let c = cell_xz(sim, l.pos);
        let y = (0..9).map(|i| height_at(&sim.height, c.x + (i % 3) as f32 - 1.0, c.y + (i / 3) as f32 - 1.0)).fold(f32::MAX, f32::min);
        let e = spawn(&mut commands, &mut models, StructKind::Lodge, 6, 3, Transform::from_xyz(c.x, y, c.y).with_scale(Vec3::splat(BUILDING_SIZE)));
        st.lodges.insert(l.id.0, e);
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct DamCellVis {
    pub steps: u8,
    pub dir: u8,
    pub site: bool,
}

pub fn dam_cells(sim: &sim::SimState) -> HashMap<(usize, usize), DamCellVis> {
    let (w, h) = (sim.water.dam_height.width(), sim.water.dam_height.height());
    let mut height: HashMap<(usize, usize), (f32, bool)> = HashMap::new();
    for (i, &d) in sim.water.dam_height.as_slice().iter().enumerate() {
        if d > 0.0 {
            height.insert((i % w, i / w), (d, false));
        }
    }
    for plan in &sim.work.dam_plans {
        let Some(b) = sim.buildings.building(plan.building) else { continue };
        if b.is_complete() {
            continue;
        }
        let frac = [0.0, 0.35, 0.7, 1.0][models::stage_of(build_progress(sim, b)) as usize];
        for (&c, &target) in plan.cells.iter().zip(&plan.heights) {
            let now = height.get(&c).map_or(0.0, |v| v.0);
            let shown = (now + (target - now).max(0.0) * frac).max(super::structures::DAM_STEP * 0.5);
            height.insert(c, (shown, true));
        }
    }
    let is_dam = |c: (usize, usize), dx: i32, dy: i32| {
        let (x, y) = (c.0 as i64 + dx as i64, c.1 as i64 + dy as i64);
        x >= 0 && y >= 0 && (x as usize) < w && (y as usize) < h && height.contains_key(&(x as usize, y as usize))
    };
    height
        .iter()
        .map(|(&c, &(d, site))| {
            let steps = (d / super::structures::DAM_STEP).round().clamp(1.0, 16.0) as u8;
            (c, DamCellVis { steps, dir: super::structures::dam_dir(|dx, dy| is_dam(c, dx, dy)), site })
        })
        .collect()
}

#[derive(Default)]
pub struct DamSync {
    seed: Option<(u64, u32)>,
    time: f64,
    cells: HashMap<(usize, usize), (Entity, DamCellVis)>,
}

pub fn sync_sim_dams(mut commands: Commands, cli: Res<Cli>, sim: Res<Sim>, models: Option<ResMut<Models>>, mut meshes: ResMut<Assets<Mesh>>, flow: Option<Res<crate::flow::Flow>>, mut st: Local<DamSync>) {
    let Some(mut models) = models else { return };
    if super::stub_mode(&cli) {
        return;
    }
    let generation = flow.map_or(0, |f| f.generation);
    let sim = &sim.0;
    if st.seed != Some((sim.seed, generation)) {
        for (e, _) in st.cells.drain().map(|(_, v)| v) {
            commands.entity(e).despawn();
        }
        st.seed = Some((sim.seed, generation));
        st.time = f64::NEG_INFINITY;
    }
    if sim.time_days == st.time {
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
            .entry((vis.steps, diag, variant, vis.site))
            .or_insert_with(|| meshes.add(super::structures::dam_cell_mesh(vis.steps, diag, variant, vis.site)))
            .clone();
        let p = cell_xz(sim, (c.0 as f32 + 0.5, c.1 as f32 + 0.5));
        let y = height_at(&sim.height, p.x, p.y);
        let yaw = -(vis.dir as f32) * std::f32::consts::FRAC_PI_4;
        let e = commands.spawn((SimEntity, Mesh3d(mesh), MeshMaterial3d(white.clone()), Transform::from_xyz(p.x, y, p.y).with_rotation(Quat::from_rotation_y(yaw)))).id();
        st.cells.insert(c, (e, vis));
    }
}

const BARGE_LEN2: u32 = 6;
const BARGE_SCALE: f32 = WORLD_SCALE * 1.4;

#[derive(Component, Clone, Debug)]
pub struct BargeVis {
    pub id: u64,
    pub cell: (usize, usize),
    pub from: Vec3,
    pub to: Vec3,
    pub t0: f64,
    pub dur: f32,
    pub yaw: f32,
    pub loaded: bool,
}

pub fn barge_point(sim: &sim::SimState, c: (usize, usize)) -> Vec3 {
    let p = cell_xz(sim, (c.0 as f32 + 0.5, c.1 as f32 + 0.5));
    let ground = height_at(&sim.height, p.x, p.y);
    let water = sim.height.get(c.0, c.1).copied().unwrap_or(0.0) + sim.water.depth.get(c.0, c.1).copied().unwrap_or(0.0);
    Vec3::new(p.x, ground.max(water), p.y)
}

pub fn glide(v: &BargeVis, now: f64) -> Vec3 {
    let t = if v.dur <= 0.0 { 1.0 } else { (((now - v.t0) as f32) / v.dur).clamp(0.0, 1.0) };
    v.from.lerp(v.to, t)
}

pub fn retarget(v: &mut BargeVis, cell: (usize, usize), to: Vec3, now: f64) {
    let here = glide(v, now);
    v.dur = ((now - v.t0) as f32).clamp(1.0 / sim::shipping::BARGE_SPEED, 0.6);
    v.from = here;
    v.to = to;
    v.t0 = now;
    v.cell = cell;
    let d = to - here;
    if d.x.abs() + d.z.abs() > 1e-3 {
        v.yaw = d.x.atan2(d.z);
    }
}

fn is_loaded(b: &sim::buildings::Barge) -> bool {
    b.hold.iter().any(|h| h.1 > 0.05) || b.cargo.is_some_and(|c| c.1 > 0.05)
}

pub fn sync_sim_barges(
    mut commands: Commands,
    cli: Res<Cli>,
    sim: Res<Sim>,
    pending: Res<PendingDays>,
    models: Option<ResMut<Models>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut q: Query<(Entity, &mut Transform, &mut BargeVis, &mut Mesh3d)>,
) {
    let Some(mut models) = models else { return };
    if super::stub_mode(&cli) {
        return;
    }
    let s = &sim.0;
    let now = s.time_days + pending.0 as f64;
    let white = models.white.clone();
    let mut mesh_for = |loaded: bool| {
        let kind = if loaded { StructKind::Barge } else { StructKind::BargeEmpty };
        models.structs.entry((kind, BARGE_LEN2, 3)).or_insert_with(|| meshes.add(models::struct_mesh(kind, BARGE_LEN2 as f32 * 0.5, 3))).clone()
    };
    let mut seen = HashSet::new();
    for (e, mut tr, mut v, mut mesh) in &mut q {
        let Some(b) = s.buildings.barge(sim::buildings::BargeId(v.id)) else {
            commands.entity(e).despawn();
            continue;
        };
        seen.insert(v.id);
        if b.cell != v.cell {
            retarget(&mut v, b.cell, barge_point(s, b.cell), now);
        }
        let loaded = is_loaded(b);
        if loaded != v.loaded {
            v.loaded = loaded;
            mesh.0 = mesh_for(loaded);
        }
        tr.translation = glide(&v, now);
        tr.rotation = tr.rotation.slerp(Quat::from_rotation_y(v.yaw), 0.15);
    }
    for b in &s.buildings.barges {
        if seen.contains(&b.id.0) {
            continue;
        }
        let p = barge_point(s, b.cell);
        let loaded = is_loaded(b);
        let v = BargeVis { id: b.id.0, cell: b.cell, from: p, to: p, t0: now, dur: 0.0, yaw: 0.0, loaded };
        commands.spawn((SimEntity, Mesh3d(mesh_for(loaded)), MeshMaterial3d(white.clone()), Transform::from_translation(p).with_scale(Vec3::splat(BARGE_SCALE)), v));
    }
}

#[cfg(test)]
mod motion_tests {
    use super::*;

    #[test]
    fn a_sprite_moves_continuously_between_steps_and_snaps_on_a_jump() {
        let mut m = Motion::default();
        m.record(0.0, Vec2::new(0.0, 0.0), 0.0);
        m.record(0.05, Vec2::new(2.0, 0.0), 0.2);
        let (p, lift, dir) = m.at(0.025).unwrap();
        assert!((p.x - 1.0).abs() < 1e-4 && (lift - 0.1).abs() < 1e-4, "half way between the steps");
        assert_eq!(dir.map(|d| d.normalize()), Some(Vec2::X), "heading is the way it is going");
        m.record(0.1, Vec2::new(2.0, 0.0), 0.2);
        assert_eq!(m.at(0.075).unwrap().2, None);
        m.record(0.15, Vec2::new(40.0, 0.0), 0.0);
        assert_eq!(m.at(0.12).unwrap().0, Vec2::new(40.0, 0.0));
    }
}
