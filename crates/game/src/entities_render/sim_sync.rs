use super::scene::height_at;
use super::sim_view::*;
use super::structures::{DamStage, DAM_STEP};
use super::{models, BeaverSprite, CurrentSeason, Models, PlantLook, SimBeaver, SimEntity, Sway, TreeLod};
use crate::picking::Target;
use crate::sim_driver::SimClock;
use crate::state::Sim;
use bevy::prelude::*;
use sim::buildings::{BuildingId, BuildingKind, Status};
use sim::resources::ResourceId;
use std::collections::{HashMap, HashSet, VecDeque};
use std::hash::Hash;

#[derive(Resource, Default)]
pub struct PendingDays(pub f32);

pub fn update_pending(clock: Option<Res<SimClock>>, mut p: ResMut<PendingDays>) {
    p.0 = clock.map_or(0.0, |c| c.pending_days());
}

/// Shared world-epoch + entity map used by all sim-to-Bevy sync systems.
pub(super) struct SyncedMap<K, V> {
    epoch: Option<(u64, u32)>,
    pub map: HashMap<K, V>,
}

impl<K, V> Default for SyncedMap<K, V> {
    fn default() -> Self { Self { epoch: None, map: HashMap::new() } }
}

impl<K: Eq + Hash + Copy, V> SyncedMap<K, V> {
    /// If the `(seed, generation)` epoch changed, despawn every tracked entry and return `true`.
    pub fn reset_if(&mut self, epoch: (u64, u32), commands: &mut Commands, mut despawn: impl FnMut(&mut Commands, V)) -> bool {
        if self.epoch == Some(epoch) { return false; }
        for (_, v) in self.map.drain() { despawn(commands, v); }
        self.epoch = Some(epoch);
        true
    }

    /// Despawn and remove every entry whose key fails `keep`.
    pub fn retain_keys(&mut self, keep: impl Fn(&K) -> bool, commands: &mut Commands, mut despawn: impl FnMut(&mut Commands, V)) {
        let gone: Vec<K> = self.map.keys().copied().filter(|k| !keep(k)).collect();
        for k in gone {
            if let Some(v) = self.map.remove(&k) { despawn(commands, v); }
        }
    }

    pub fn retain_live(&mut self, live: &HashSet<K>, commands: &mut Commands, despawn: impl FnMut(&mut Commands, V)) {
        self.retain_keys(|k| live.contains(k), commands, despawn);
    }
}

pub(super) fn cell_mid(sim: &sim::SimState, c: (usize, usize)) -> Vec2 {
    cell_xz(sim, (c.0 as f32 + 0.5, c.1 as f32 + 0.5))
}

#[derive(Clone, Copy)]
struct MotionSample {
    t: f64,
    xz: Vec2,
    y: f32,
    /// Ground-relative lift on land; `None` on water/canals/locks so steep banks never pull a floater.
    ground_lift: Option<f32>,
}

#[derive(Component, Default)]
pub struct Motion {
    samples: VecDeque<MotionSample>,
    ahead: Vec<Vec2>,
    instance: u64,
    observed_time: Option<f64>,
}

pub const BEAVER_SIZE: f32 = WORLD_SCALE * 0.9;
pub const TREE_SCALE: f32 = 2.0;
pub const BUILDING_SIZE: f32 = WORLD_SCALE * 0.82;
const SPREAD: f32 = 0.28;

fn spread_in(sim: &sim::SimState, cell: sim::water::Cell, id: u64, game: bool) -> Vec3 {
    if !game || matches!(sim.nav.surface_at(cell), Some(sim::nav::Surface::Canal | sim::nav::Surface::Lock)) {
        return Vec3::ZERO;
    }
    let a = (id as f32 * 2.399_963).rem_euclid(std::f32::consts::TAU);
    Vec3::new(a.cos(), 0.0, a.sin()) * SPREAD
}

impl Motion {
    fn record(&mut self, t: f64, xz: Vec2, y: f32, ground_lift: Option<f32>) {
        let sample = MotionSample { t, xz, y, ground_lift };
        match self.samples.back_mut() {
            Some(b) if b.t == t => *b = sample,
            Some(b) if b.t > t => return,
            _ => self.samples.push_back(sample),
        }
        while self.samples.len() > 512 { self.samples.pop_front(); }
    }

    #[allow(clippy::too_many_arguments)]
    fn record_step(&mut self, sim: &sim::SimState, id: u64, game: bool, ground: &impl Fn(Vec2) -> f32, step_start: f64, end: f64, cells: &[(sim::water::Cell, sim::nav::Gait)], mut prev: Vec2) -> Vec2 {
        if cells.is_empty() { return prev; }
        let mut pts = Vec::with_capacity(cells.len());
        let mut total_w = 0.0f64;
        for &(cell, gait) in cells {
            let next = cell_mid(sim, cell) + spread_in(sim, cell, id, game).xz();
            let dist = prev.distance(next) / crate::terrain_render::CELL_SIZE;
            total_w += if dist > 1e-4 { (dist * gait_cost(gait)) as f64 } else { 1.0 };
            pts.push((cell, next, total_w));
            prev = next;
        }
        let span = (end - step_start).max(1e-7);
        for (i, &(cell, next, cum_w)) in pts.iter().enumerate() {
            let frac = if total_w > 0.0 { cum_w / total_w } else { (i + 1) as f64 / pts.len() as f64 };
            let t = if i + 1 == pts.len() { end } else { step_start + span * frac };
            let y = stand_y(sim, cell);
            self.record(t, next, y, (!on_water(sim, cell)).then(|| y - ground(next)));
        }
        prev
    }

    fn observe(&mut self, sim: &sim::SimState, ag: &sim::agents::Agent, game: bool) {
        let p = agent_world_pos(sim, ag, 0.0) + spread_in(sim, ag.cell, ag.id.0, game);
        let ground = |xz: Vec2| height_at(&sim.height, xz.x, xz.y);
        if self.instance != sim.instance {
            self.samples.clear();
            self.observed_time = None;
            self.instance = sim.instance;
        }
        if self.observed_time.is_some_and(|t| t >= sim.time_days) { return; }
        if let (Some(previous), Some(last)) = (self.observed_time, self.samples.back().copied()) {
            let mut point = last.xz;
            if sim.time_days - previous > sim::STEP_DAYS as f64 + 1e-8 {
                let mut prev_t = previous;
                let mut batch: Vec<(sim::water::Cell, sim::nav::Gait)> = Vec::new();
                let mut batch_end: Option<f64> = None;
                for &(end, cell, gait) in ag.history.iter().filter(|h| h.0 > previous + 1e-9) {
                    if let Some(cur_end) = batch_end.filter(|&ce| (end - ce).abs() > 1e-9) {
                        point = self.record_step(sim, ag.id.0, game, &ground, prev_t.max(cur_end - sim::STEP_DAYS as f64), cur_end, &batch, point);
                        prev_t = cur_end;
                        batch.clear();
                        batch_end = Some(end);
                    } else if batch_end.is_none() {
                        batch_end = Some(end);
                    }
                    batch.push((cell, gait));
                }
                if let Some(cur_end) = batch_end {
                    self.record_step(sim, ag.id.0, game, &ground, prev_t.max(cur_end - sim::STEP_DAYS as f64), cur_end, &batch, point);
                }
            } else if !ag.trail.is_empty() {
                self.record_step(sim, ag.id.0, game, &ground, previous, sim.time_days, &ag.trail, point);
            }
        }
        self.record(sim.time_days, p.xz(), p.y, (!on_water(sim, ag.cell)).then(|| p.y - ground(p.xz())));
        self.observed_time = Some(sim.time_days);
        self.ahead = ag.path.as_ref().map_or(Vec::new(), |path| {
            path.steps.iter().take(4).map(|&(c, _)| cell_mid(sim, c) + spread_in(sim, c, ag.id.0, game).xz()).collect()
        });
    }

    fn raw_at(&self, t: f64) -> Option<(Vec2, f32, Option<f32>, Option<Vec2>)> {
        let first = *self.samples.front()?;
        let Some(k) = self.samples.iter().position(|s| s.t > t) else {
            let last = *self.samples.back()?;
            return Some((last.xz, last.y, last.ground_lift, None));
        };
        if k == 0 { return Some((first.xz, first.y, first.ground_lift, None)); }
        let (a, b) = (self.samples[k - 1], self.samples[k]);
        let u = (((t - a.t) / (b.t - a.t).max(1e-9)) as f32).clamp(0.0, 1.0);
        let chord = b.xz - a.xz;
        let lift = a.ground_lift.zip(b.ground_lift).map(|(la, lb)| la + (lb - la) * u);
        Some((a.xz.lerp(b.xz, u), a.y + (b.y - a.y) * u, lift, (chord.length() > 1e-3).then_some(chord)))
    }

    fn at(&mut self, t: f64, smooth_cells: f32) -> Option<(Vec2, f32, Option<f32>, Option<Vec2>)> {
        while self.samples.len() > 16 && self.samples[10].t <= t { self.samples.pop_front(); }
        let (raw, y, lift, moving) = self.raw_at(t)?;
        let last = *self.samples.back()?;
        let moving = moving.or_else(|| {
            let next = self.ahead.first().copied()?;
            (t >= last.t && next.distance(last.xz) > 1e-3).then_some(next - last.xz)
        });
        let Some(heading) = moving else { return Some((raw, y, lift, None)); };
        if smooth_cells < 1e-3 { return Some((raw, y, lift, Some(heading))); }
        let pts: Vec<Vec2> = self.samples.iter().map(|s| s.xz).chain(self.ahead.iter().copied()).collect();
        let mut cum = Vec::with_capacity(pts.len());
        let mut d = 0.0;
        for (i, p) in pts.iter().enumerate() {
            if i > 0 { d += p.distance(pts[i - 1]); }
            cum.push(d);
        }
        let k = self.samples.iter().position(|s| s.t > t).unwrap_or(self.samples.len() - 1).max(1);
        let s_t = cum[k - 1] + pts[k - 1].distance(raw);
        let (mut lo_bound, mut hi_bound) = (0.0f32, *cum.last()?);
        for i in 1..pts.len().saturating_sub(1) {
            let pause = pts[i].distance(pts[i - 1]) < 1e-3 || pts[i + 1].distance(pts[i]) < 1e-3;
            let reversal = !pause && (pts[i] - pts[i - 1]).normalize_or_zero().dot((pts[i + 1] - pts[i]).normalize_or_zero()) < -0.25;
            if pause || reversal {
                if cum[i] <= s_t { lo_bound = lo_bound.max(cum[i]); }
                if cum[i] >= s_t { hi_bound = hi_bound.min(cum[i]); }
            }
        }
        let h = smooth_cells.min(s_t - lo_bound).min(hi_bound - s_t).max(0.0);
        if h < 1e-3 { return Some((raw, y, lift, Some(heading))); }
        let (pos, dir) = polyline_average(&pts, &cum, s_t - h, s_t + h);
        Some((pos, y, lift, Some(if dir.length() > 1e-4 { dir } else { heading })))
    }
}

fn polyline_average(pts: &[Vec2], cum: &[f32], lo: f32, hi: f32) -> (Vec2, Vec2) {
    let at = |s: f32| {
        let i = cum.partition_point(|&c| c < s).clamp(1, pts.len() - 1);
        pts[i - 1].lerp(pts[i], ((s - cum[i - 1]) / (cum[i] - cum[i - 1]).max(1e-6)).clamp(0.0, 1.0))
    };
    let mut sum = Vec2::ZERO;
    for i in 1..pts.len() {
        let (a, b) = (cum[i - 1].max(lo), cum[i].min(hi));
        if b > a { sum += (at(a) + at(b)) * 0.5 * (b - a); }
    }
    (sum / (hi - lo), at(hi) - at(lo))
}

const SMOOTH_CELLS: f32 = 1.0;
const DIVE_S: f32 = 0.7;
const POP_S: f32 = 0.8;

#[derive(Component)]
pub struct LodgeDive { inside: bool, since: f32, at: Vec3 }

#[derive(Component)]
pub struct Newborn(f32);

fn find_agent(sim: &sim::SimState, id: sim::colony::BeaverId) -> Option<(&sim::agents::Agent, f32)> {
    if let Some(ag) = sim.agents.get(id) {
        return Some((ag, sim.colony.beaver(id).map_or(sim::colony::ADULT_DAYS, |b| b.age_days)));
    }
    sim.agents.remains.leaving.iter().find(|(a, _)| a.id == id).map(|(a, age)| (a, *age))
}

fn ramp(a: f32, b: f32, x: f32) -> f32 { ((x - a) / (b - a)).clamp(0.0, 1.0) }

pub fn look_of(sim: &sim::SimState, id: sim::colony::BeaverId) -> super::Look {
    use sim::tuning::colony::{EAT_BELOW, OLD_AGE_YEARS};
    let mut look = super::Look::default();
    let Some(b) = sim.colony.beaver(id) else { return look };
    let harm = b.harm();
    look.thin = ramp(EAT_BELOW, 0.1, b.needs.food).max(ramp(0.1, 0.6, harm[0]));
    look.cold = ramp(0.6, 0.15, b.needs.warmth).max(ramp(0.1, 0.6, harm[1]));
    look.hurt = ramp(0.05, 0.5, harm[2]);
    look.tired = ramp(0.4, 0.05, b.needs.rest).max(ramp(0.1, 0.6, harm[3]));
    look.old = ramp(OLD_AGE_YEARS * 0.8, OLD_AGE_YEARS + 1.5, b.age_years());
    look.sad = ramp(0.4, 0.12, b.mood);
    if let Some(p) = &b.pregnancy {
        look.belly = (((sim.time_days - p.conceived_day) / (p.due_day - p.conceived_day).max(1e-6)) as f32).clamp(0.0, 1.0);
    }
    look
}

#[allow(clippy::too_many_arguments)]
pub(super) fn sync_sim_beavers(
    mut commands: Commands, sim: Res<Sim>, pending: Res<PendingDays>, clock: Option<Res<SimClock>>,
    render: Res<super::RenderClock>, slaps: Res<super::TailSlapTimes>, time: Res<Time>,
    mut q: Query<(Entity, &mut Transform, &mut BeaverSprite, &mut Visibility, &mut Motion, &mut LodgeDive, &mut super::raft_landing::LockGlide, Option<&Newborn>), With<SimBeaver>>,
    mut seen: Local<HashSet<u64>>, mut drawn: ResMut<super::BeaverDrawn>,
) {
    use super::scene::Pose;
    let _p = ::sim::prof::scope("game/sync sim beavers");
    seen.clear();
    drawn.0.clear();
    let (s, t) = (&sim.0, render.0);
    let now = clock.as_ref().map_or(s.time_days + pending.0 as f64, |c| c.render_days(s.time_days));
    let idle: Vec<_> = s.agents.by_id.values()
        .filter(|a| !a.inside && a.path.is_none() && matches!(a.task, sim::agents::Task::Idle | sim::agents::Task::Rest) && a.assignment.is_none() && s.work.slot_of(a.id).is_none())
        .filter_map(|a| s.colony.beaver(a.id).map(|b| (a.id, b.family, a.cell)))
        .collect();
    let groom_partner = |id: sim::colony::BeaverId| {
        let &(_, fam, at) = idle.iter().find(|x| x.0 == id)?;
        idle.iter().find(|x| x.0 != id && x.1 == fam && x.2.0.abs_diff(at.0).max(x.2.1.abs_diff(at.1)) <= 1).map(|x| x.2)
    };
    let speed = clock.as_ref().map_or(crate::sim_driver::Speed::X1, |c| c.speed);
    let (smooth_cells, dive_s) = (SMOOTH_CELLS * speed.smoothing(), DIVE_S * speed.smoothing());
    let chambers = super::raft_landing::chambers(s);
    let face = |at: Vec3, to: (f32, f32)| { let w = cell_xz(s, to); (w.x - at.x).atan2(w.y - at.z) };
    for (e, mut tr, mut sprite, mut vis, mut motion, mut dive, mut glide, newborn) in &mut q {
        let id = sim::colony::BeaverId(sprite.id);
        let Some((ag, age)) = find_agent(s, id) else { commands.entity(e).despawn(); continue; };
        seen.insert(sprite.id);
        let p = agent_world_pos(s, ag, 0.0) + spread_in(s, ag.cell, sprite.id, clock.is_some());
        motion.observe(s, ag, clock.is_some());
        let mut heading = match motion.at(now, smooth_cells) {
            Some((xz, y, lift, dir)) => {
                tr.translation = Vec3::new(xz.x, lift.map_or(y, |l| height_at(&s.height, xz.x, xz.y) + l), xz.y);
                dir
            }
            None => { tr.translation = p; None }
        };
        if let Some((at, yaw)) = super::raft_landing::lock_pose(s, ag, (now - s.time_days) as f32, tr.translation) {
            tr.translation = at;
            if let Some(yaw) = yaw { heading = Some(Vec2::new(yaw.sin(), yaw.cos())); }
        }
        tr.translation = super::raft_landing::glide(&mut glide, &chambers, tr.translation, time.delta_secs(), speed.smoothing());
        sprite.pose = pose_of(s, ag, heading.is_some() || is_moving(ag));
        sprite.load = load_of(ag);
        sprite.wet = is_wet(s, ag);
        let want_yaw = heading.map(|d| d.x.atan2(d.y)).or_else(|| facing_of(s, ag)).unwrap_or(sprite.yaw);
        sprite.yaw = super::raft_landing::turn(&mut glide, want_yaw, time.delta_secs(), speed.smoothing());
        if let Some(w) = s.predators.wolves.iter().find(|w| matches!(w.state, sim::predators::WolfState::Kill { target, .. } if target == id)) {
            sprite.pose = Pose::Caught;
            sprite.yaw = face(tr.translation, w.pos);
        } else if heading.is_none() {
            if let Some(m) = s.agents.courting.get(&id).filter(|c| c.met.is_some()).and_then(|c| s.agents.get(c.partner)) {
                sprite.pose = Pose::Court;
                sprite.yaw = face(tr.translation, (m.cell.0 as f32 + 0.5, m.cell.1 as f32 + 0.5));
            } else if sprite.pose == Pose::Idle && (t * 0.07 + (sprite.id % 7) as f32 * 0.13).rem_euclid(1.0) < 0.45 {
                if let Some(m) = groom_partner(id) {
                    sprite.pose = Pose::Groom;
                    sprite.yaw = face(tr.translation, (m.0 as f32 + 0.5, m.1 as f32 + 0.5));
                }
            }
        }
        let mut look = look_of(s, id);
        look.slap = slaps.0.get(&sprite.id).copied().unwrap_or(-100.0);
        if ag.inside != dive.inside { dive.inside = ag.inside; dive.since = t; }
        let dt = t - dive.since;
        let shown = if ag.inside {
            if dive_s > 0.0 && dt < dive_s { tr.translation = dive.at; look.sink = ramp(0.0, dive_s, dt); sprite.pose = Pose::Swim; true } else { false }
        } else {
            dive.at = tr.translation;
            if dive_s > 0.0 && dt < dive_s { look.sink = 1.0 - ramp(0.0, dive_s, dt); }
            true
        };
        sprite.look = look;
        let mut size = BEAVER_SIZE * super::age_scale_days(age) * super::looks_size(&sprite.features);
        if let Some(n) = newborn {
            let k = ramp(0.0, POP_S, t - n.0);
            size *= 0.2 + 0.8 * (1.0 - (1.0 - k) * (1.0 - k));
            if k >= 1.0 { commands.entity(e).remove::<Newborn>(); }
        }
        tr.scale = Vec3::splat(size);
        if !ag.inside { drawn.0.insert(sprite.id, tr.translation); }
        vis.set_if_neq(if shown { Visibility::Inherited } else { Visibility::Hidden });
    }
    let agents = s.agents.by_id.values().map(|a| (a, s.colony.beaver(a.id).map_or(sim::colony::ADULT_DAYS, |b| b.age_days)))
        .chain(s.agents.remains.leaving.iter().map(|(a, age)| (a, *age)));
    for (ag, age) in agents {
        if seen.contains(&ag.id.0) { continue; }
        let yaw = facing_of(s, ag).unwrap_or(0.0);
        let mut motion = Motion::default();
        motion.observe(s, ag, clock.is_some());
        let at = agent_world_pos(s, ag, 0.0) + spread_in(s, ag.cell, ag.id.0, clock.is_some());
        let features = s.colony.looks_of(ag.id);
        let mut e = commands.spawn((
            SimBeaver, motion, super::raft_landing::LockGlide::default(),
            BeaverSprite { id: ag.id.0, pose: pose_of(s, ag, is_moving(ag)), load: load_of(ag), yaw, wet: is_wet(s, ag), look: look_of(s, ag.id), features },
            LodgeDive { inside: ag.inside, since: -100.0, at },
            Transform::from_translation(at).with_rotation(Quat::from_rotation_y(yaw)).with_scale(Vec3::splat(BEAVER_SIZE * super::age_scale_days(age) * super::looks_size(&features))),
            if ag.inside { Visibility::Hidden } else { Visibility::Inherited },
        ));
        if age < 2.0 { e.insert(Newborn(t)); }
    }
}

#[derive(Component)]
pub struct BodySprite(u64);

pub(super) fn sync_bodies(mut commands: Commands, sim: Res<Sim>, render: Res<super::RenderClock>, mut q: Query<(Entity, &BodySprite, &mut BeaverSprite)>) {
    let (s, bodies) = (&sim.0, &sim.0.agents.remains.bodies);
    let mut have = HashSet::new();
    for (e, b, mut sprite) in &mut q {
        match bodies.iter().find(|x| x.id.0 == b.0) {
            Some(body) => {
                sprite.look.fade = ramp(0.8, 1.0, ((s.time_days - body.died) / sim::agents::REMAINS_DAYS) as f32);
                have.insert(b.0);
            }
            None => commands.entity(e).despawn(),
        }
    }
    for body in bodies.iter().filter(|b| !have.contains(&b.id.0)) {
        use sim::colony::DeathCause as C;
        let curled = matches!(body.cause, C::Cold | C::OldAge | C::Exhaustion);
        let mut look = super::Look { dead: Some((if s.time_days - body.died < 0.05 { render.0 } else { render.0 - 10.0 }, curled)), ..Default::default() };
        if body.cause == C::Starvation { look.thin = 1.0; }
        let (xz, y) = (cell_mid(s, body.cell), stand_y(s, body.cell));
        let yaw = (body.id.0 as f32 * 2.399).rem_euclid(std::f32::consts::TAU);
        let features = s.colony.looks_of(body.id);
        commands.spawn((
            BodySprite(body.id.0),
            BeaverSprite { id: body.id.0, pose: super::scene::Pose::Dead, load: None, yaw, wet: false, look, features },
            Transform::from_xyz(xz.x, y, xz.y).with_rotation(Quat::from_rotation_y(yaw)).with_scale(Vec3::splat(BEAVER_SIZE * super::age_scale_days(body.age_days) * super::looks_size(&features))),
            Visibility::Inherited,
        ));
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ResSig {
    Tree(u8),
    Felled(u8, bool),
    Plant(u8, u8, bool),
    Floating(u8, u8, i16, bool),
    Pebbles(Option<i16>, u8),
    Mud(bool, u8),
}

fn rides_water(r: &sim::resources::Resource) -> bool {
    r.kind.is_food_plant() || (r.kind.row().model == "pebbles" && r.wet)
}

fn spawn_topple(commands: &mut Commands, models: &Models, sim: &sim::SimState, r: &sim::resources::Resource, season: models::Season, now: f32) {
    let c = r.cell;
    let scale = stage_scale(sim::forestry::TreeStage::Mature) * TREE_SCALE * (0.8 + 0.4 * hash01(c.0, c.1, 5));
    let tr = res_transform(sim, r, scale);
    let variant = ((hash01(c.0, c.1, 17) * 4.0) as usize).min(3);
    commands.spawn((SimEntity, Mesh3d(models.tree(r.species.expect("a tree has a species"), season, 0)), MeshMaterial3d(models.tree_tints[variant].clone()), tr, super::Topple { start: now, base: tr }));
}

pub fn res_sig(r: &sim::resources::Resource, depth: f32) -> ResSig {
    use sim::resources::Look;
    match r.look() {
        Look::Tree(stage, step) => ResSig::Tree(stage as u8 * 16 + step),
        Look::Felled(logs, branches) => ResSig::Felled(logs, branches),
        Look::Fill(q) => match r.kind.row().model.as_str() {
            "mud" => ResSig::Mud(r.kind.flowing(), q),
            "plant" => ResSig::Plant(r.plant.map_or(u8::MAX, |p| p.index() as u8), q, r.seedling),
            "pebbles" => ResSig::Pebbles(r.wet.then(|| (depth * 10.0).round() as i16), q),
            "water_plant" => ResSig::Floating(r.plant.map_or(u8::MAX, |p| p.index() as u8), q, (depth * 10.0).round() as i16, r.seedling),
            m => panic!("resource {}: no model {m}", r.kind.key()),
        },
    }
}

fn hash01(x: usize, y: usize, salt: u32) -> f32 {
    let mut h = (x as u32).wrapping_mul(0x9E37_79B1) ^ (y as u32).wrapping_mul(0x85EB_CA6B) ^ salt.wrapping_mul(0xC2B2_AE35);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2C1B_3C6D);
    ((h ^ (h >> 12)) & 0xffff) as f32 / 65535.0
}

#[derive(Default)]
struct Plants {
    at: Vec<(ResourceId, usize, bool)>,
    known: HashSet<ResourceId>,
    tiles: Vec<(u64, u64)>,
    grids: Option<[(u64, u64); 3]>,
}

#[derive(Default)]
pub struct ResourceSync {
    res: SyncedMap<ResourceId, (Entity, ResSig)>,
    drawn: Option<u64>,
    guessed: Vec<ResourceId>,
    plants: Plants,
    fog: u64,
    rocks: Vec<Option<Entity>>,
}

#[derive(Resource, Default)]
pub struct BuildingAnchors(pub HashMap<BuildingId, f32>);

#[derive(Default)]
pub struct BuildingSync {
    time: f64,
    bld: SyncedMap<BuildingId, (Entity, (BuildingKind, (usize, usize), u8))>,
    wear: HashMap<BuildingId, (u8, f32, f32)>,
}

pub fn stage_scale(st: sim::forestry::TreeStage) -> Vec3 {
    use sim::forestry::TreeStage::*;
    match st { Seedling => Vec3::splat(0.3), Young => Vec3::splat(0.62), Mature | Snag => Vec3::ONE, Old => Vec3::new(1.12, 1.0, 1.12) }
}

pub fn growing_scale(st: sim::forestry::TreeStage, growth: f32) -> Vec3 {
    use sim::forestry::TreeStage::*;
    let next = match st { Seedling => Young, Young => Mature, s => s };
    stage_scale(st).lerp(stage_scale(next), growth)
}

enum Extra { None, Tree(TreeLod), Plant(PlantLook) }

fn res_visual(models: &Models, r: &sim::resources::Resource, season: models::Season) -> (Handle<Mesh>, Vec3, Extra) {
    let (sig, c) = (res_sig(r, 0.0), r.cell);
    let hash_scale = 0.8 + 0.4 * hash01(c.0, c.1, 5);
    match sig {
        ResSig::Tree(_) if r.snag => (models.snag.clone(), Vec3::splat(TREE_SCALE * hash_scale), Extra::None),
        ResSig::Tree(_) => {
            let sp = r.species.expect("a tree has a species");
            let st = r.stage().unwrap_or(sim::forestry::TreeStage::Mature);
            (models.tree(sp, season, 0), growing_scale(st, r.growth()) * TREE_SCALE * hash_scale, Extra::Tree(TreeLod { species: sp, lod: 0, season, cutaway: false }))
        }
        ResSig::Felled(logs, branches) => (models.felled[logs as usize * 2 + branches as usize].clone(), Vec3::splat(WORLD_SCALE), Extra::None),
        ResSig::Plant(_, q, seedling) | ResSig::Floating(_, q, _, seedling) => {
            let plant = r.plant.unwrap_or_else(|| sim::plants::Plant::all().find(|p| p.aquatic() == r.kind.aquatic()).expect("a plant of each habitat"));
            let size = if seedling { stage_scale(sim::forestry::TreeStage::Seedling).x } else { 0.55 + 0.45 * q as f32 / 4.0 };
            (models.plant(plant, season), Vec3::splat(size * (0.9 + 0.2 * hash01(c.0, c.1, 23))), Extra::Plant(PlantLook { plant, season }))
        }
        ResSig::Pebbles(stream, q) => {
            let i = (q.clamp(1, 4) - 1) as usize;
            (if stream.is_some() { models.gravel_bars[i].clone() } else { models.pebble_heaps[i].clone() }, Vec3::splat(0.9 + 0.2 * hash01(c.0, c.1, 29)), Extra::None)
        }
        ResSig::Mud(river, q) => {
            let s = (0.5 + 0.5 * q as f32 / 8.0) * (0.95 + 0.15 * hash01(c.0, c.1, 31));
            (if river { models.river_mud.clone() } else { models.mud_heap.clone() }, Vec3::splat(s), Extra::None)
        }
    }
}

fn res_transform(sim: &sim::SimState, r: &sim::resources::Resource, scale: Vec3) -> Transform {
    let (c, p) = (r.cell, cell_mid(sim, r.cell));
    let ground = height_at(&sim.height, p.x, p.y);
    let depth = sim.water.depth.get(c.0, c.1).copied().unwrap_or(0.0).max(0.0);
    let y = match r.kind {
        k if k.is_tree() => ground - 0.05,
        k if k.is_food_plant() && k.aquatic() => ground + depth,
        k if k.row().model == "pebbles" && r.wet => ground.max(ground + depth + 0.04 - super::deposits::GRAVEL_BAR_H),
        _ => ground,
    };
    Transform::from_xyz(p.x, y, p.y).with_rotation(Quat::from_rotation_y(hash01(c.0, c.1, 9) * std::f32::consts::TAU)).with_scale(scale)
}

fn material_for(models: &Models, r: &sim::resources::Resource) -> Handle<StandardMaterial> {
    if r.stage() == Some(sim::forestry::TreeStage::Old) { models.old_tree.clone() }
    else if r.kind.is_tree() { models.tree_tints[((hash01(r.cell.0, r.cell.1, 17) * 4.0) as usize).min(3)].clone() }
    else if r.kind.row().model == "mud" { models.wet.clone() }
    else { models.white.clone() }
}

fn hidden(sim: &sim::SimState, r: &sim::resources::Resource) -> bool {
    let stands = r.plant.map_or(0.0, |p| p.spec().max_depth);
    (r.kind.is_food_plant() && !r.kind.aquatic())
        && (sim.water.depth.get(r.cell.0, r.cell.1).is_some_and(|&d| if stands > 0.05 { d > stands + 0.1 } else { sim::water::shore_step(d) > 0 })
            || sim.water.riverbed.get(r.cell.0, r.cell.1).is_some_and(|&b| (b * sim::water::RIVERBED_STEPS).round() > 3.0))
}

fn show_resource(commands: &mut Commands, res: &mut HashMap<ResourceId, (Entity, ResSig)>, models: &Models, sim: &sim::SimState, r: &sim::resources::Resource, season: models::Season, now: f32) {
    let sig = res_sig(r, sim.water.depth.get(r.cell.0, r.cell.1).copied().unwrap_or(0.0));
    let old = res.get(&r.id).copied();
    if old.is_some_and(|(_, s)| s == sig) { return; }
    if let Some((_, ResSig::Tree(_))) = old {
        if matches!(sig, ResSig::Felled(..)) && !r.snag { spawn_topple(commands, models, sim, r, season, now); }
    }
    let (mesh, scale, extra) = res_visual(models, r, season);
    let tr = res_transform(sim, r, scale);
    let base = (Mesh3d(mesh), MeshMaterial3d(material_for(models, r)), tr);
    let e = match old {
        Some((e, _)) => {
            let mut ec = commands.entity(e);
            ec.insert(base).remove::<bevy::camera::primitives::Aabb>();
            match extra {
                Extra::Tree(l) => ec.insert((l, Sway(tr.rotation))).remove::<PlantLook>(),
                Extra::Plant(p) => ec.insert(p).remove::<(TreeLod, Sway)>(),
                Extra::None => ec.remove::<(TreeLod, Sway, PlantLook)>(),
            };
            e
        }
        None => {
            let mut ec = commands.spawn((SimEntity, base));
            match extra {
                Extra::Tree(l) => { ec.insert((l, Sway(tr.rotation))); }
                Extra::Plant(p) => { ec.insert(p); }
                Extra::None => {}
            }
            if r.kind.is_tree() || r.kind.is_felled() { ec.insert(Target::Resource(r.id)); }
            ec.id()
        }
    };
    res.insert(r.id, (e, sig));
}

fn plant_water(sim: &sim::SimState) -> Vec<(u64, u64)> {
    let (depth, edge, bed) = (&sim.water.depth, &sim.water.shore, &sim.water.riverbed);
    let (cols, rows) = (depth.width().div_ceil(sim::grid::TILE), depth.height().div_ceil(sim::grid::TILE));
    let mix = |a: u64, b: u64| (a ^ b).wrapping_mul(0x9E37_79B9_7F4A_7C15).rotate_left(29);
    (0..cols * rows).map(|t| {
        let (x, y) = (t % cols, t / cols);
        (mix(mix(mix(edge.origin(), edge.tile_stamp(x, y)), bed.origin()), bed.tile_stamp(x, y)), mix(depth.origin(), depth.tile_stamp(x, y)))
    }).collect()
}

pub(super) fn sync_sim_resources(
    mut commands: Commands, sim: Res<Sim>, season: Res<CurrentSeason>, models: Option<Res<Models>>,
    flow: Option<Res<crate::flow::Flow>>, mut st: Local<ResourceSync>, clock: Res<super::RenderClock>,
) {
    let _p = ::sim::prof::scope("game/sync sim resources");
    let Some(models) = models else { return };
    let (sim, generation) = (&sim.0, flow.map_or(0, |f| f.generation));
    if st.res.reset_if((sim.seed, generation), &mut commands, |c, (e, _)| c.entity(e).despawn()) {
        for e in st.rocks.drain(..).flatten() { commands.entity(e).despawn(); }
        st.drawn = None;
    }
    let st = &mut *st;
    let no_trees = std::env::var_os("BC_NO_TREES").is_some();
    let shown = |r: &sim::resources::Resource| !(no_trees && r.kind.is_tree()) && !hidden(sim, r) && !crate::fog_render::unexplored(sim, r.cell);
    let tile_of = |r: &sim::resources::Resource| (r.cell.1 / sim::grid::TILE) * sim.water.depth.width().div_ceil(sim::grid::TILE) + r.cell.0 / sim::grid::TILE;
    let changes = sim.resources.changes();
    let water = [sim.water.depth.stamp(), sim.water.shore.stamp(), sim.water.riverbed.stamp()];
    match st.drawn.and_then(|drawn| changes.since(drawn)).filter(|_| !st.res.map.is_empty()) {
        None => {
            let _p = ::sim::prof::scope("game/sync sim resources/all of them");
            st.plants = Plants::default();
            let mut live = HashSet::with_capacity(sim.resources.items().len());
            for r in sim.resources.items() {
                if rides_water(r) { st.plants.known.insert(r.id); st.plants.at.push((r.id, tile_of(r), r.kind.aquatic())); }
                if shown(r) { live.insert(r.id); show_resource(&mut commands, &mut st.res.map, &models, sim, r, season.0, clock.0); }
            }
            st.res.retain_live(&live, &mut commands, |c, (e, _)| c.entity(e).despawn());
            st.plants.tiles = plant_water(sim);
        }
        Some(noted) => {
            let mut look: Vec<ResourceId> = noted.chain(st.guessed.drain(..)).collect();
            if st.plants.grids != Some(water) {
                let now = plant_water(sim);
                let was = &st.plants.tiles;
                look.extend(st.plants.at.iter().filter(|(_, t, _)| now.len() != was.len() || now[*t] != was[*t]).map(|(id, _, _)| *id));
                st.plants.tiles = now;
            }
            look.sort_unstable();
            look.dedup();
            let mut lost = false;
            for id in look {
                let r = sim.resources.get(id);
                match r {
                    Some(r) if rides_water(r) => if st.plants.known.insert(id) { st.plants.at.push((id, tile_of(r), r.kind.aquatic())); },
                    _ => lost |= st.plants.known.remove(&id),
                }
                match r.filter(|r| shown(r)) {
                    Some(r) => show_resource(&mut commands, &mut st.res.map, &models, sim, r, season.0, clock.0),
                    None => if let Some((e, _)) = st.res.map.remove(&id) { commands.entity(e).despawn(); },
                }
            }
            if lost { let known = &st.plants.known; st.plants.at.retain(|(id, _, _)| known.contains(id)); }
            if st.fog != sim.fog.revision {
                let _p = ::sim::prof::scope("game/sync sim resources/fog cleared");
                for r in sim.resources.items() {
                    if !st.res.map.contains_key(&r.id) && shown(r) {
                        show_resource(&mut commands, &mut st.res.map, &models, sim, r, season.0, clock.0);
                    }
                }
            }
        }
    }
    if st.rocks.len() != sim.resources.boulders.len() {
        for e in st.rocks.drain(..).flatten() { commands.entity(e).despawn(); }
        st.rocks = vec![None; sim.resources.boulders.len()];
        st.fog = u64::MAX;
    }
    if st.fog != sim.fog.revision {
        for (slot, b) in st.rocks.iter_mut().zip(&sim.resources.boulders) {
            if slot.is_none() && !crate::fog_render::unexplored(sim, b.cell) {
                *slot = Some(commands.spawn((SimEntity, Mesh3d(models.boulders[b.seed as usize % models.boulders.len()].clone()), MeshMaterial3d(models.white.clone()), boulder_transform(sim, b))).id());
            }
        }
    }
    st.fog = sim.fog.revision;
    st.plants.grids = Some(water);
    st.drawn = Some(changes.count());
    st.guessed = changes.guesses().collect();
}

fn boulder_transform(sim: &sim::SimState, b: &sim::resources::Boulder) -> Transform {
    let p = cell_mid(sim, b.cell);
    let ground = [(-0.45, -0.45), (0.45, -0.45), (-0.45, 0.45), (0.45, 0.45)].iter().map(|(dx, dz)| height_at(&sim.height, p.x + dx, p.y + dz)).fold(f32::MAX, f32::min);
    Transform::from_xyz(p.x, ground, p.y).with_rotation(Quat::from_rotation_y((b.seed % 360) as f32 * std::f32::consts::TAU / 360.0)).with_scale(Vec3::new(b.size, b.size * 0.9, b.size))
}

pub fn build_progress(sim: &sim::SimState, b: &sim::buildings::Building) -> f32 {
    if b.status == Status::Complete { 1.0 } else { sim.work.progress(b.id).unwrap_or(0.0).clamp(0.0, 0.999) }
}

#[derive(Default)]
pub struct ReinforcementSync {
    drawn: SyncedMap<(BuildingId, bool), (Entity, f32)>,
    meshes: HashMap<(BuildingKind, bool), Handle<Mesh>>,
}

pub(super) fn sync_reinforcements(
    mut commands: Commands, sim: Res<Sim>, flow: Res<crate::flow::Flow>, models: Option<Res<Models>>,
    anchors: Res<BuildingAnchors>, mut meshes: ResMut<Assets<Mesh>>, mut state: Local<ReinforcementSync>,
) {
    let _p = ::sim::prof::scope("game/sync reinforcements");
    let Some(models) = models else { return };
    let s = &sim.0;
    state.drawn.reset_if((s.seed, flow.generation), &mut commands, |c, (e, _)| c.entity(e).despawn());
    let mut live = HashSet::new();
    for (b, pebbled, _) in s.buildings.buildings.iter().flat_map(|b| [(b, false, b.reinforced), (b, true, !b.kind.is_dam() && b.pebbled)]).filter(|l| l.2) {
        let key = (b.id, pebbled);
        live.insert(key);
        let tr = building_transform(s, b, anchors.0.get(&b.id).copied());
        if let Some(&(e, y)) = state.drawn.map.get(&key) {
            if (tr.translation.y - y).abs() > 0.01 {
                commands.entity(e).insert(tr);
                state.drawn.map.insert(key, (e, tr.translation.y));
            }
            continue;
        }
        let handle = state.meshes.entry((b.kind, pebbled)).or_insert_with(|| meshes.add(if pebbled { models::pebble_facing_mesh(b.kind) } else { models::reinforcement_mesh(b.kind) })).clone();
        let e = commands.spawn((Target::Building(b.id), SimEntity, Mesh3d(handle), MeshMaterial3d(models.white.clone()), tr)).id();
        state.drawn.map.insert(key, (e, tr.translation.y));
    }
    state.drawn.retain_live(&live, &mut commands, |c, (e, _)| c.entity(e).despawn());
}

fn building_transform(sim: &sim::SimState, b: &sim::buildings::Building, anchor: Option<f32>) -> Transform {
    let c = cell_xz(sim, b.centre());
    let grounds: Vec<f32> = b.cells().map(|c| { let p = cell_mid(sim, c); height_at(&sim.height, p.x, p.y) }).collect();
    let y = if models::footing_of(b.kind).is_some() {
        let mean = grounds.iter().sum::<f32>() / grounds.len().max(1) as f32;
        let surface = b.cells().filter_map(|(x, y)| Some(sim.height.get(x, y)? + sim.water.depth.get(x, y).copied().filter(|d| *d > 0.02)?)).fold(f32::MIN, f32::max);
        let dry = if surface == f32::MIN { grounds.iter().copied().fold(f32::MIN, f32::max) - 0.05 } else { f32::MIN };
        mean.max(surface + 0.08).max(dry).max(anchor.unwrap_or(f32::MIN))
    } else {
        grounds.iter().copied().fold(f32::MAX, f32::min)
    };
    Transform::from_xyz(c.x, y, c.y).with_scale(Vec3::splat(BUILDING_SIZE))
}

fn wear_band(condition: f32) -> u8 {
    if condition >= 0.75 { 0 } else if condition >= 0.5 { 1 } else if condition >= 0.25 { 2 } else { 3 }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn sync_sim_buildings(
    mut commands: Commands, sim: Res<Sim>, models: Option<ResMut<Models>>, mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>, mut puff_mesh: ResMut<super::effects::PuffMesh>,
    mut anchors: ResMut<BuildingAnchors>, clock: Res<super::RenderClock>, flow: Option<Res<crate::flow::Flow>>, mut st: Local<BuildingSync>,
) {
    let _p = ::sim::prof::scope("game/sync sim buildings");
    let Some(mut models) = models else { return };
    let (changed, epoch, sim) = (sim.is_changed(), (sim.0.seed, flow.map_or(0, |f| f.generation)), &sim.0);
    if st.bld.reset_if(epoch, &mut commands, |c, (e, _)| c.entity(e).despawn()) {
        anchors.0.clear();
        st.time = f64::NEG_INFINITY;
    }
    if sim.time_days == st.time && !changed && !st.bld.map.is_empty() { return; }
    st.time = sim.time_days;
    let white = models.white.clone();
    let mut live = HashSet::new();
    let mut puffs = Vec::new();
    let mut spawn = |commands: &mut Commands, models: &mut Models, target: Target, kind: BuildingKind, len2: u32, stage: u8, tr: Transform| {
        let mesh = models.structs.entry((kind, len2, stage)).or_insert_with(|| meshes.add(models::struct_mesh(kind, len2 as f32 * 0.5, stage))).clone();
        commands.spawn((target, SimEntity, Mesh3d(mesh), MeshMaterial3d(white.clone()), tr)).id()
    };
    for b in sim.buildings.buildings.iter().filter(|b| !(b.kind.is_dam() && b.is_complete()) && !b.kind.is_canal()) {
        live.insert(b.id);
        let stage = if sim.buildings.stranded(b, &sim.water.depth) { models::DRY_STAGE } else { models::stage_of(build_progress(sim, b)) };
        let sig = (b.kind, b.pos, stage);
        let band = if b.is_complete() { wear_band(b.condition) } else { 0 };
        let (was_band, was_cond, mut puffed) = st.wear.get(&b.id).copied().unwrap_or((band, b.condition, -100.0));
        let c = cell_xz(sim, b.centre());
        if b.condition > was_cond + 0.002 && clock.0 - puffed > 1.2 {
            puffs.push((Vec3::new(c.x, height_at(&sim.height, c.x, c.y) + 0.6, c.y), 1.4));
            puffed = clock.0;
        }
        st.wear.insert(b.id, (band, b.condition, puffed));
        let wear_mat = |models: &Models, b: u8| if b == 0 { white.clone() } else { models.wear[(b - 1) as usize].clone() };
        if let Some(&(e, _)) = st.bld.map.get(&b.id).filter(|(_, s)| *s == sig) {
            let anchor = anchors.0.get(&b.id).copied();
            let tr = building_transform(sim, b, anchor);
            if band != was_band {
                let sag = 1.0 - 0.04 * band as f32;
                commands.entity(e).insert((MeshMaterial3d(wear_mat(&models, band)), super::effects::Grow { base: tr.scale * Vec3::new(1.0, sag, 1.0), from: Vec3::new(1.0, (1.0 - 0.04 * was_band as f32) / sag, 1.0), since: clock.0, secs: 1.5 }));
            }
            if tr.translation.y > anchor.unwrap_or(f32::MIN) + 0.01 {
                commands.entity(e).insert(tr);
                anchors.0.insert(b.id, tr.translation.y);
            }
            continue;
        }
        let replaced = st.bld.map.remove(&b.id).map(|(old, _)| commands.entity(old).despawn()).is_some();
        let tr = building_transform(sim, b, anchors.0.get(&b.id).copied());
        anchors.0.insert(b.id, tr.translation.y);
        let e = spawn(&mut commands, &mut models, Target::Building(b.id), b.kind, (b.kind.spec().footprint.0 as u32 * 2).max(2), stage, tr);
        if band > 0 { commands.entity(e).insert(MeshMaterial3d(wear_mat(&models, band))); }
        if replaced || (!b.is_complete() && stage == 0) {
            commands.entity(e).insert(super::effects::Grow { base: tr.scale, from: Vec3::new(1.02, 0.8, 1.02), since: clock.0, secs: 0.6 });
            puffs.push((tr.translation + Vec3::new(0.0, 0.3, 0.0), 1.8));
        }
        st.bld.map.insert(b.id, (e, sig));
    }
    st.bld.retain_live(&live, &mut commands, |c, (e, _)| c.entity(e).despawn());
    anchors.0.retain(|id, _| live.contains(id));
    drop(spawn);
    for (at, size) in puffs { super::effects::puff(&mut commands, &mut meshes, &mut materials, &mut puff_mesh, at, size); }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub struct Piece { pub cx: i32, pub cz: i32, pub yaw: i32, pub len: u32 }

impl Piece {
    fn new(centre: Vec2, along: Vec2) -> Piece {
        let yaw = (-along.y).atan2(along.x);
        Piece { cx: (centre.x * 100.0).round() as i32, cz: (centre.y * 100.0).round() as i32, yaw: (yaw * 100.0).round() as i32, len: ((along.length().max(0.2) + 0.25) * 10.0).round() as u32 }
    }
    pub fn centre(&self) -> (f32, f32) { (self.cx as f32 / 100.0, self.cz as f32 / 100.0) }
    pub fn yaw(&self) -> f32 { self.yaw as f32 / 100.0 }
    fn face(&self) -> Vec2 { Vec2::new(self.yaw().sin(), self.yaw().cos()) }
}

pub fn smooth_line(pts: &[Vec2], half: f32) -> Vec<Vec2> {
    if pts.len() < 3 { return pts.to_vec(); }
    let mut cum = vec![0.0f32; pts.len()];
    for i in 1..pts.len() { cum[i] = cum[i - 1] + pts[i].distance(pts[i - 1]); }
    let end = cum[pts.len() - 1];
    (0..pts.len()).map(|k| {
        let h = half.min(cum[k]).min(end - cum[k]);
        if h < 1e-3 { pts[k] } else { polyline_average(pts, &cum, cum[k] - h, cum[k] + h).0 }
    }).collect()
}

pub fn wall_pieces(cells: &[(usize, usize)]) -> HashMap<(usize, usize), Piece> {
    let mut out = HashMap::new();
    let mut runs: Vec<Vec<(usize, usize)>> = Vec::new();
    for &i in &sim::dams::chain(cells) {
        let c = cells[i];
        match runs.last_mut() {
            Some(r) if r.last().unwrap().0.abs_diff(c.0) <= 1 && r.last().unwrap().1.abs_diff(c.1) <= 1 => r.push(c),
            _ => runs.push(vec![c]),
        }
    }
    for run in runs {
        let raw: Vec<Vec2> = run.iter().map(|c| Vec2::new(c.0 as f32 + 0.5, c.1 as f32 + 0.5)).collect();
        if run.len() == 1 { out.insert(run[0], Piece::new(raw[0], Vec2::X)); continue; }
        let p = smooth_line(&raw, 1.5);
        let n = p.len();
        for k in 0..n {
            let a = if k > 0 { (p[k - 1] + p[k]) * 0.5 } else { p[0] - (p[1] - p[0]).normalize_or_zero() * 0.5 };
            let b = if k + 1 < n { (p[k] + p[k + 1]) * 0.5 } else { p[n - 1] + (p[n - 1] - p[n - 2]).normalize_or_zero() * 0.5 };
            out.insert(run[k], Piece::new((a + b) * 0.5, b - a));
        }
    }
    out
}

fn faces_downstream(sim: &sim::SimState, c: (usize, usize), piece: &Piece, downstream: Option<(i32, i32)>) -> bool {
    let f = piece.face();
    if let Some(d) = downstream { return f.dot(Vec2::new(d.0 as f32, d.1 as f32)) > 0.0; }
    let surface = |side: f32| (1..=2).filter_map(|k| {
        let (x, y) = (c.0 as f32 + 0.5 + f.x * side * k as f32, c.1 as f32 + 0.5 + f.y * side * k as f32);
        if x < 0.0 || y < 0.0 { return None; }
        let (x, y) = (x.floor() as usize, y.floor() as usize);
        Some(sim.height.get(x, y)? + sim.water.depth.get(x, y).copied().unwrap_or(0.0))
    }).fold(f32::MIN, f32::max);
    surface(-1.0) > surface(1.0) + 0.01
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct DamCellVis { pub steps: u8, pub stage: DamStage, pub piece: Piece, pub flip: bool, pub faced: bool }

fn dam_walls(sim: &sim::SimState) -> HashMap<(usize, usize), (DamCellVis, bool)> {
    let w = sim.water.dam_height.width();
    let integrity = |c: (usize, usize)| sim.water.dam_integrity.get(c.0, c.1).copied().unwrap_or(1.0).clamp(0.0, 1.0);
    let mut cells: HashMap<(usize, usize), (f32, DamStage)> = sim.water.dam_height.as_slice().iter().enumerate()
        .filter(|&(_, &d)| d > 0.0)
        .map(|(i, &d)| { let c = (i % w, i / w); (c, (d * integrity(c), DamStage::Done)) })
        .collect();
    for d in sim.buildings.buildings.iter().filter_map(|b| b.dam.as_deref()) {
        for c in d.cells.iter().filter(|c| c.target > 0.0 && !sim::dams::is_gate(d, c.cell)) {
            let now = sim.water.dam_height.get(c.cell.0, c.cell.1).copied().unwrap_or(0.0);
            if now <= 1e-3 { cells.insert(c.cell, (DAM_STEP, DamStage::Staked)); }
            else if now < c.target - 1e-3 { cells.insert(c.cell, (now * integrity(c.cell), DamStage::Rising)); }
        }
    }
    cells.retain(|c, _| !sim::locks::is_lock(sim, *c) && !super::raft_landing::in_chamber(sim, *c));
    let mut pieces: HashMap<(usize, usize), (Piece, Option<(i32, i32)>)> = HashMap::new();
    let mut faced_cells: HashSet<(usize, usize)> = HashSet::new();
    for b in &sim.buildings.buildings {
        let Some(d) = b.dam.as_deref() else { continue };
        for row in 0..d.rows.max(1) {
            let line: Vec<_> = d.cells.iter().filter(|c| c.row == row && cells.contains_key(&c.cell)).map(|c| c.cell).collect();
            for (c, p) in wall_pieces(&line) { pieces.insert(c, (p, Some(d.downstream))); }
        }
        if !b.pebbled { continue; }
        let shown = |c: (usize, usize)| cells.get(&c).is_some_and(|&(_, stage)| stage != DamStage::Staked);
        for dc in d.cells.iter().filter(|dc| shown(dc.cell)) {
            let below = usize::try_from(dc.cell.0 as i64 + d.downstream.0 as i64).ok().zip(usize::try_from(dc.cell.1 as i64 + d.downstream.1 as i64).ok());
            if !below.is_some_and(|n| shown(n) && d.cells.iter().any(|o| o.cell == n && o.row > dc.row)) {
                faced_cells.insert(dc.cell);
            }
        }
    }
    let loose: Vec<_> = cells.keys().copied().filter(|c| !pieces.contains_key(c)).collect();
    for (c, p) in wall_pieces(&loose) { pieces.insert(c, (p, None)); }
    cells.iter().filter_map(|(&c, &(d, stage))| {
        let (piece, down) = *pieces.get(&c)?;
        let steps = (d / DAM_STEP).round().clamp(1.0, 16.0) as u8;
        Some((c, (DamCellVis { steps, stage, piece, flip: faces_downstream(sim, c, &piece, down), faced: down.is_some() && faced_cells.contains(&c) }, down.is_none())))
    }).collect()
}

fn dams_key(sim: &sim::SimState) -> ((u64, u64), (u64, u64), u64) {
    let mut h = 0xcbf2_9ce4_8422_2325u64;
    let mut add = |v: u64| h = (h ^ v).wrapping_mul(0x100_0000_01b3);
    for b in &sim.buildings.buildings {
        let Some(d) = b.dam.as_deref() else { continue };
        for v in [b.id.0, d.rows as u64, b.pebbled as u64, d.downstream.0 as u64 ^ (d.downstream.1 as u64) << 32] { add(v); }
        for c in &d.cells {
            add(c.cell.0 as u64 ^ (c.cell.1 as u64) << 24 ^ (c.row as u64) << 48);
            add(c.target.to_bits() as u64);
        }
    }
    (sim.water.dam_height.stamp(), sim.water.dam_integrity.stamp(), h)
}

#[derive(Default)]
pub struct DamSync {
    time: f64,
    cells: SyncedMap<(usize, usize), (Entity, DamCellVis)>,
    walls: HashMap<(usize, usize), (DamCellVis, bool)>,
    key: Option<(((u64, u64), (u64, u64), u64), usize)>,
    faced: HashMap<(u8, u32, u32, DamStage), Handle<Mesh>>,
}

fn piece_transform(sim: &sim::SimState, piece: &Piece, flip: bool, lift: impl Fn(f32) -> f32) -> Transform {
    let p = cell_xz(sim, piece.centre());
    Transform::from_xyz(p.x, lift(height_at(&sim.height, p.x, p.y)), p.y)
        .with_rotation(Quat::from_rotation_y(piece.yaw() + if flip { std::f32::consts::PI } else { 0.0 }))
}

pub(super) fn sync_sim_dams(
    mut commands: Commands, sim: Res<Sim>, models: Option<ResMut<Models>>, mut meshes: ResMut<Assets<Mesh>>,
    clock: Res<super::RenderClock>, flow: Option<Res<crate::flow::Flow>>, mut st: Local<DamSync>,
) {
    let _p = ::sim::prof::scope("game/sync sim dams");
    let Some(mut models) = models else { return };
    let (changed, epoch, sim) = (sim.is_changed(), (sim.0.seed, flow.map_or(0, |f| f.generation)), &sim.0);
    if st.cells.reset_if(epoch, &mut commands, |c, (e, _)| c.entity(e).despawn()) { st.time = f64::NEG_INFINITY; }
    if sim.time_days == st.time && !changed { return; }
    st.time = sim.time_days;
    let st = &mut *st;
    let key = (dams_key(sim), sim::locks::built(sim).count());
    if st.key != Some(key) {
        st.walls = dam_walls(sim);
        st.key = Some(key);
    } else {
        let mut turned = false;
        for (c, (vis, _)) in st.walls.iter_mut().filter(|(_, (_, loose))| *loose) {
            let flip = faces_downstream(sim, *c, &vis.piece, None);
            turned |= flip != vis.flip;
            vis.flip = flip;
        }
        if !turned && st.cells.map.len() == st.walls.len() { return; }
    }
    let want: HashMap<(usize, usize), DamCellVis> = st.walls.iter().map(|(c, (vis, _))| (*c, *vis)).collect();
    let mut was_steps = HashMap::new();
    let gone: Vec<_> = st.cells.map.iter().filter(|(c, (_, v))| want.get(*c) != Some(v)).map(|(c, _)| *c).collect();
    for c in gone {
        if let Some((e, v)) = st.cells.map.remove(&c) { commands.entity(e).despawn(); was_steps.insert(c, v.steps); }
    }
    let white = models.white.clone();
    for (&c, &vis) in &want {
        if st.cells.map.contains_key(&c) { continue; }
        let variant = (hash01(c.0, c.1, 21) * 3.0) as u32 % 3;
        let key = (vis.steps, vis.piece.len, variant, vis.stage);
        let mesh = if vis.faced {
            st.faced.entry(key).or_insert_with(|| meshes.add(super::structures::faced_dam_cell_mesh(vis.steps, vis.piece.len as f32 / 10.0, variant, vis.stage))).clone()
        } else {
            models.dams.entry(key).or_insert_with(|| meshes.add(super::structures::dam_cell_mesh(vis.steps, vis.piece.len as f32 / 10.0, variant, vis.stage))).clone()
        };
        let tr = piece_transform(sim, &vis.piece, vis.flip, |g| g);
        let e = commands.spawn((SimEntity, Mesh3d(mesh), MeshMaterial3d(white.clone()), tr)).id();
        if let Some(&old) = was_steps.get(&c).filter(|&&o| o != vis.steps && vis.steps > 0) {
            commands.entity(e).insert(super::effects::Grow { base: tr.scale, from: Vec3::new(1.0, (old as f32 / vis.steps as f32).clamp(0.05, 4.0), 1.0), since: clock.0, secs: 1.0 });
        }
        if let Some(b) = sim.buildings.buildings.iter().rev().find(|b| b.kind.is_dam() && b.cells().any(|cell| cell == c)) {
            commands.entity(e).insert(Target::Building(b.id));
        }
        st.cells.map.insert(c, (e, vis));
    }
}

#[derive(Default)]
pub struct GhostSync {
    mats: Option<(Handle<StandardMaterial>, Handle<StandardMaterial>)>,
    cells: HashMap<(usize, usize), (Entity, (u8, Piece, bool, bool))>,
}

#[allow(clippy::too_many_arguments)]
pub(super) fn sync_dam_ghosts(
    mut commands: Commands, sim: Res<Sim>, hud: Option<Res<crate::hud::HudState>>, models: Option<ResMut<Models>>,
    mut meshes: ResMut<Assets<Mesh>>, mut materials: ResMut<Assets<StandardMaterial>>, mut st: Local<GhostSync>,
) {
    let _p = ::sim::prof::scope("game/sync dam ghosts");
    let Some(mut models) = models else { return };
    let sim = &sim.0;
    let mut want = HashMap::new();
    if let Some(plan) = hud.as_ref().filter(|h| h.tool == crate::hud::Tool::Dam).and_then(|h| h.plan.as_ref().filter(|p| p.line == h.line)) {
        let line: Vec<_> = plan.targets.iter().map(|t| t.0).collect();
        let pieces = wall_pieces(&line);
        for &(c, t) in &plan.targets {
            if let Some(&piece) = pieces.get(&c) {
                let steps = (t.max(DAM_STEP) / DAM_STEP).round().clamp(1.0, 16.0) as u8;
                want.insert(c, (steps, piece, faces_downstream(sim, c, &piece, None), plan.refusal.is_some()));
            }
        }
    }
    let stale: Vec<_> = st.cells.iter().filter(|(c, (_, k))| want.get(*c) != Some(k)).map(|(c, _)| *c).collect();
    for c in stale { if let Some((e, _)) = st.cells.remove(&c) { commands.entity(e).despawn(); } }
    if want.is_empty() { return; }
    let (ok, no) = st.mats.get_or_insert_with(|| {
        let mut m = |c| materials.add(StandardMaterial { base_color: c, alpha_mode: AlphaMode::Blend, perceptual_roughness: 1.0, ..default() });
        (m(Color::srgba(1.0, 1.0, 1.0, 0.7)), m(Color::srgba(1.0, 0.42, 0.36, 0.7)))
    }).clone();
    for (&c, &(steps, piece, flip, bad)) in &want {
        if st.cells.contains_key(&c) { continue; }
        let mesh = models.dams.entry((steps, piece.len, 1, DamStage::Done)).or_insert_with(|| meshes.add(super::structures::dam_cell_mesh(steps, piece.len as f32 / 10.0, 1, DamStage::Done))).clone();
        let surface = sim.height.get(c.0, c.1).copied().unwrap_or(0.0) + sim.water.depth.get(c.0, c.1).copied().unwrap_or(0.0);
        let tr = piece_transform(sim, &piece, flip, |g| g.max(surface + 0.15 - steps as f32 * DAM_STEP));
        let e = commands.spawn((SimEntity, Mesh3d(mesh), MeshMaterial3d(if bad { no.clone() } else { ok.clone() }), tr)).id();
        st.cells.insert(c, (e, (steps, piece, flip, bad)));
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum GroundKey { Pile(u64), Supplies }

#[derive(Default)]
pub struct GroundSync {
    drawn: SyncedMap<GroundKey, (Entity, ((usize, usize), (u8, u8)))>,
    meshes: HashMap<Option<sim::buildings::Good>, Handle<Mesh>>,
}

#[allow(clippy::too_many_arguments)]
pub(super) fn sync_ground_goods(
    mut commands: Commands, sim: Res<Sim>, flow: Res<crate::flow::Flow>, models: Option<Res<Models>>,
    mut meshes: ResMut<Assets<Mesh>>, mut materials: ResMut<Assets<StandardMaterial>>, mut puff_mesh: ResMut<super::effects::PuffMesh>,
    beavers: Query<(Entity, &Transform, &BeaverSprite), With<SimBeaver>>, clock: Res<super::RenderClock>, mut st: Local<GroundSync>,
) {
    let _p = ::sim::prof::scope("game/sync ground goods");
    let Some(models) = models else { return };
    let s = &sim.0;
    let loading = st.drawn.reset_if((s.seed, flow.generation), &mut commands, |c, (e, _)| c.entity(e).despawn());
    let mut want: Vec<_> = s.work.piles.items.iter().map(|p| {
        let rot = (1.0 - p.days_left(s.time_days) / sim::piles::SPOIL_DAYS) as f32;
        let band = if rot < 0.45 { 0 } else if rot < 0.8 { 1 } else { 2 };
        (GroundKey::Pile(p.id), p.cell, Some(p.good), (p.amount / 4.0).cbrt().clamp(0.5, 1.8) * (1.0 - 0.25 * rot.clamp(0.0, 1.0)), band)
    }).collect();
    if let Some(c) = s.work.supplies {
        let start: f32 = sim::content::content().start.iter().map(|g| g.1).sum();
        let left: f32 = sim::buildings::Good::all().map(|g| s.buildings.stores.stock_of(sim::agents::SUPPLIES, g)).sum();
        if left > 0.05 { want.push((GroundKey::Supplies, c, None, (left / start.max(1.0)).sqrt().clamp(0.45, 1.0), 0)); }
    }
    let mut live = HashSet::new();
    for (key, cell, good, scale, band) in want {
        live.insert(key);
        let sig = (cell, ((scale * 20.0).round() as u8, band));
        if st.drawn.map.get(&key).is_some_and(|d| d.1 == sig) { continue; }
        let mat = if band == 0 { models.white.clone() } else { models.rot[(band - 1) as usize].clone() };
        let p = cell_mid(s, cell);
        let ground = Vec3::new(p.x, height_at(&s.height, p.x, p.y), p.y);
        let dropped = !loading && st.drawn.map.get(&key).is_none_or(|d| (d.1).1.0 < sig.1.0) && matches!(key, GroundKey::Pile(_));
        if dropped {
            let by: Vec<_> = s.agents.by_id.values().filter(|a| a.cell.0.abs_diff(cell.0) <= 1 && a.cell.1.abs_diff(cell.1) <= 1).map(|a| a.id.0).collect();
            if let Some((e, btr, _)) = beavers.iter().find(|(_, _, b)| by.contains(&b.id)) {
                commands.entity(e).insert(super::SetDown { until: clock.0 + 1.0 });
                if let Some(g) = good {
                    let from = btr.translation + Vec3::Y * 0.8 * btr.scale.y + btr.rotation * Vec3::Z * 0.6 * btr.scale.z;
                    commands.spawn((SimEntity, Mesh3d(models.loads[g.index()].clone()), MeshMaterial3d(models.white.clone()),
                        Transform::from_translation(from).with_scale(btr.scale * super::LOAD_LOOK),
                        super::effects::Fall { from, to: ground + Vec3::Y * 0.05, since: clock.0 + 0.3, secs: 0.35 }));
                }
            }
            super::effects::puff(&mut commands, &mut meshes, &mut materials, &mut puff_mesh, ground, 0.7);
        }
        let tr = Transform::from_translation(ground).with_rotation(Quat::from_rotation_y(hash01(cell.0, cell.1, 23) * std::f32::consts::TAU)).with_scale(Vec3::splat(scale));
        let mesh = st.meshes.entry(good).or_insert_with(|| meshes.add(match good { Some(g) => models::heap_mesh(g), None => models::supplies_mesh() })).clone();
        match st.drawn.map.get(&key) {
            Some(&(e, (_, (old_scale, _)))) => {
                let from = (old_scale as f32 / 20.0 / scale.max(0.01)).clamp(0.2, 5.0);
                commands.entity(e).insert((tr, MeshMaterial3d(mat), super::effects::Grow { base: tr.scale, from: Vec3::splat(from), since: clock.0, secs: 0.8 }));
                st.drawn.map.insert(key, (e, sig));
            }
            None => {
                let e = commands.spawn((SimEntity, Mesh3d(mesh), MeshMaterial3d(mat), tr)).id();
                if dropped { commands.entity(e).insert(super::effects::Grow { base: tr.scale, from: Vec3::splat(0.05), since: clock.0 + 0.6, secs: 0.5 }); }
                st.drawn.map.insert(key, (e, sig));
            }
        }
    }
    st.drawn.retain_live(&live, &mut commands, |c, (e, _)| c.entity(e).despawn());
}
