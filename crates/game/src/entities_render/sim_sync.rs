use super::structures::{DamStage, DAM_STEP};
use super::scene::{height_at, Species, StructKind};
use super::sim_view::*;
use super::{models, BeaverSprite, CurrentSeason, Models, PlantLook, SimBeaver, SimEntity, Sway, TreeLod};
use crate::sim_driver::SimClock;
use crate::state::Sim;
use crate::picking::Target;
use bevy::prelude::*;
use sim::buildings::{BuildingId, BuildingKind, Status};
use sim::resources::{ResourceId, ResourceKind};
use std::collections::{HashMap, HashSet};

#[derive(Resource, Default)]
pub struct PendingDays(pub f32);

pub fn update_pending(clock: Option<Res<SimClock>>, mut p: ResMut<PendingDays>) {
    p.0 = clock.map_or(0.0, |c| c.pending_days());
}

#[derive(Component, Default)]
pub struct Motion {
    samples: std::collections::VecDeque<(f64, Vec2, f32)>,
    /// The next few cells of the beaver's planned path, after the latest sample.
    ahead: Vec<Vec2>,
    instance: u64,
    observed_time: Option<f64>,
}

pub const BEAVER_SIZE: f32 = WORLD_SCALE * 0.9;
/// Trees are drawn smaller than life (a real aspen is ~20 beavers tall) so crowns about 4 m
/// wide leave the ground, streams and beavers visible between them.
pub const TREE_SCALE: f32 = 2.0;
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
            // A trail is the last step's cells. Seen only every few steps (at speed), the way
            // since comes from the history the sim keeps, each step's cells timed within it.
            if sim.time_days - previous > sim::STEP_DAYS as f64 + 1e-8 {
                let pace = sim::agents::travel_pace(&sim.nav, ag);
                let rate = sim::agents::MOVE_COST_PER_DAY * pace;
                let mut t = previous;
                for &(end, cell, gait) in ag.history.iter().filter(|h| h.0 > previous + 1e-9) {
                    let next = cell_xz(sim, (cell.0 as f32 + 0.5, cell.1 as f32 + 0.5)) + offset.xz();
                    let dir = (next - point).normalize_or_zero();
                    let cost = sim.nav.cost_toward(cell, (dir.x, dir.y), pace).unwrap_or(gait_cost(gait));
                    let cells = point.distance(next) / crate::terrain_render::CELL_SIZE;
                    t = t.max(end - sim::STEP_DAYS as f64);
                    t = (t + (cells * cost / rate) as f64).min(end);
                    self.record(t, next, stand_y(sim, cell) - ground(next));
                    point = next;
                }
            } else {
                let pace = sim::agents::travel_pace(&sim.nav, ag);
                let rate = sim::agents::MOVE_COST_PER_DAY * pace;
                let mut t = previous;
                for &(cell, gait) in &ag.trail {
                    let next = cell_xz(sim, (cell.0 as f32 + 0.5, cell.1 as f32 + 0.5)) + offset.xz();
                    // Timed as the simulation moved it: with or against the current there.
                    let dir = (next - point).normalize_or_zero();
                    let cost = sim.nav.cost_toward(cell, (dir.x, dir.y), pace).unwrap_or(gait_cost(gait));
                    let cells = point.distance(next) / crate::terrain_render::CELL_SIZE;
                    t = (t + (cells * cost / rate) as f64).min(sim.time_days);
                    self.record(t, next, stand_y(sim, cell) - ground(next));
                    point = next;
                }
            }
        }
        self.record(sim.time_days, p.xz(), p.y - ground(p.xz()));
        self.observed_time = Some(sim.time_days);
        self.ahead = ag.path.as_ref().map_or(Vec::new(), |path| {
            path.steps.iter().take(4).map(|&(c, _)| cell_xz(sim, (c.0 as f32 + 0.5, c.1 as f32 + 0.5)) + offset.xz()).collect()
        });
    }

    /// Where the record puts the beaver at `t` (straight lines between cell centres), its
    /// height, and whether it is moving.
    fn raw_at(&self, t: f64) -> Option<(Vec2, f32, Option<Vec2>)> {
        let first = *self.samples.front()?;
        let Some(k) = self.samples.iter().position(|s| s.0 > t) else {
            let last = *self.samples.back()?;
            return Some((last.1, last.2, None));
        };
        if k == 0 {
            return Some((first.1, first.2, None));
        }
        let (a, b) = (self.samples[k - 1], self.samples[k]);
        let u = (((t - a.0) / (b.0 - a.0).max(1e-9)) as f32).clamp(0.0, 1.0);
        let chord = b.1 - a.1;
        Some((a.1.lerp(b.1, u), a.2 + (b.2 - a.2) * u, (chord.length() > 1e-3).then_some(chord)))
    }

    /// Where the beaver is drawn at `t`. Paths run cell to cell in four directions, so a
    /// diagonal route is a staircase; the drawn beaver takes the average of the route over a
    /// cell either side of where it is (by distance travelled, using the planned path ahead),
    /// which straightens staircases and rounds corners. The window narrows to nothing at the
    /// route's ends, so a beaver still stops exactly where it works.
    fn at(&mut self, t: f64) -> Option<(Vec2, f32, Option<Vec2>)> {
        // Keep a few cells of history behind for the window.
        while self.samples.len() > 8 && self.samples[6].0 <= t {
            self.samples.pop_front();
        }
        let (raw, lift, moving) = self.raw_at(t)?;
        // Caught up with the latest step but still under way: keep to the smoothed route.
        let last = *self.samples.back()?;
        let moving = moving.or_else(|| {
            let next = self.ahead.first().copied()?;
            (t >= last.0 && next.distance(last.1) > 1e-3).then_some(next - last.1)
        });
        let Some(heading) = moving else { return Some((raw, lift, None)) };
        // The route as a polyline: recorded points, then the plan ahead.
        let pts: Vec<Vec2> = self.samples.iter().map(|s| s.1).chain(self.ahead.iter().copied()).collect();
        let mut cum = Vec::with_capacity(pts.len());
        let mut d = 0.0;
        for (i, p) in pts.iter().enumerate() {
            if i > 0 {
                d += p.distance(pts[i - 1]);
            }
            cum.push(d);
        }
        // Distance travelled at `t`: along the recorded segment it is on.
        let k = self.samples.iter().position(|s| s.0 > t).unwrap_or(self.samples.len() - 1).max(1);
        let s_t = cum[k - 1] + pts[k - 1].distance(raw);
        let end = *cum.last()?;
        let h = SMOOTH_CELLS.min(s_t).min(end - s_t).max(0.0);
        if h < 1e-3 {
            return Some((raw, lift, Some(heading)));
        }
        let point_at = |s: f32| {
            let i = cum.partition_point(|&c| c < s).clamp(1, pts.len() - 1);
            let seg = (cum[i] - cum[i - 1]).max(1e-6);
            pts[i - 1].lerp(pts[i], ((s - cum[i - 1]) / seg).clamp(0.0, 1.0))
        };
        // The mean of the polyline over [s_t - h, s_t + h]: each piece contributes its length
        // times its midpoint.
        let (lo, hi) = (s_t - h, s_t + h);
        let mut sum = Vec2::ZERO;
        for i in 1..pts.len() {
            let (a, b) = (cum[i - 1].max(lo), cum[i].min(hi));
            if b > a {
                sum += (point_at(a) + point_at(b)) * 0.5 * (b - a);
            }
        }
        let pos = sum / (hi - lo);
        let dir = point_at(hi) - point_at(lo);
        Some((pos, lift, Some(if dir.length() > 1e-4 { dir } else { heading })))
    }
}

/// Half-width, in cells of travel, of the window a drawn beaver's route is averaged over.
const SMOOTH_CELLS: f32 = 1.0;

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
    fn a_staircase_is_drawn_as_a_straight_diagonal() {
        let mut motion = Motion::default();
        for i in 0..9 {
            let p = Vec2::new(((i + 1) / 2) as f32, (i / 2) as f32);
            motion.record(i as f64, p, 0.0);
        }
        for k in 0..12 {
            let t = 2.0 + k as f64 * 0.35;
            let (p, _, dir) = motion.at(t).unwrap();
            // On the diagonal (x - y = 0.5 between the steps), heading diagonally.
            assert!((p.x - p.y - 0.5).abs() < 0.02, "t {t}: {p}");
            let d = dir.unwrap().normalize();
            assert!((d.x - d.y).abs() < 0.05, "t {t}: {d}");
        }
    }

    #[test]
    fn corners_are_cut_and_the_route_ends_exactly_where_it_stops() {
        let mut motion = Motion::default();
        motion.record(0.0, Vec2::ZERO, 0.0);
        motion.record(1.0, Vec2::new(1.0, 0.0), 0.0);
        motion.record(2.0, Vec2::new(1.0, 1.0), 0.0);
        let (corner, _, dir) = motion.at(1.0).unwrap();
        assert!(corner.x < 1.0 - 0.1 && corner.y > 0.1, "{corner}");
        let d = dir.unwrap().normalize();
        assert!(d.x > 0.3 && d.y > 0.3, "{d}");
        let (end, _, _) = motion.at(1.999).unwrap();
        assert!(end.distance(Vec2::new(1.0, 1.0)) < 0.01, "{end}");
        let (end, _, dir) = motion.at(2.5).unwrap();
        assert_eq!(end, Vec2::new(1.0, 1.0));
        assert!(dir.is_none());
    }

    #[test]
    fn a_pause_stays_still() {
        let mut motion = Motion::default();
        motion.record(0.0, Vec2::ZERO, 0.0);
        motion.record(1.0, Vec2::new(1.0, 0.0), 0.0);
        motion.record(2.0, Vec2::new(1.0, 0.0), 0.0);
        motion.record(3.0, Vec2::new(2.0, 0.0), 0.0);
        let (p, _, dir) = motion.at(1.5).unwrap();
        assert_eq!(p, Vec2::new(1.0, 0.0));
        assert!(dir.is_none());
        let (p, _, _) = motion.at(2.5).unwrap();
        assert!(p.y.abs() < 1e-5 && p.x > 1.0 && p.x < 2.0);
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
        let corner = motion.raw_at(corner_time).unwrap().0;
        assert!(corner.distance(cell_xz(&sim, (3.5, 2.5))) < 1e-4);
    }

    #[test]
    fn seen_only_every_few_steps_a_beaver_is_drawn_round_the_corner_it_took() {
        let mut sim = sim::SimState::new(1);
        let id = sim::colony::BeaverId(1);
        sim.agents.spawn(id, (2, 2));
        let mut agent = sim.agents.get(id).unwrap().clone();
        for c in [(3, 2), (4, 2), (5, 2), (5, 3), (5, 4), (5, 5)] {
            sim.nav.set(c, sim::nav::Surface::Water);
        }
        let mut motion = Motion::default();
        motion.observe(&sim, &agent, false);
        // Six cells over six steps, round a corner at (5, 2); the game sees only the end.
        let step = sim::STEP_DAYS as f64;
        for (i, c) in [(3, 2), (4, 2), (5, 2), (5, 3), (5, 4), (5, 5)].into_iter().enumerate() {
            agent.history.push_back((step * (i + 1) as f64, c, sim::nav::Gait::Swim));
        }
        agent.cell = (5, 5);
        agent.trail = vec![((5, 5), sim::nav::Gait::Swim)];
        sim.time_days = step * 6.0;
        motion.observe(&sim, &agent, false);
        let corner = cell_xz(&sim, (5.5, 2.5));
        let closest = (0..=60).map(|k| motion.raw_at(sim.time_days * k as f64 / 60.0).unwrap().0.distance(corner)).fold(f32::MAX, f32::min);
        assert!(closest < 0.05, "it passes the corner, not across it: {closest}");
    }

    #[test]
    fn a_swimmer_is_drawn_at_the_speed_the_current_gives_it() {
        let move_cost = sim::agents::MOVE_COST_PER_DAY as f64;
        for (drift, from, cells) in [(0.5, (2, 2), [(3, 2), (4, 2)]), (-0.5, (2, 2), [(3, 2), (4, 2)])] {
            let mut sim = sim::SimState::new(1);
            let id = sim::colony::BeaverId(1);
            sim.agents.spawn(id, from);
            let mut agent = sim.agents.get(id).unwrap().clone();
            for c in cells {
                sim.nav.set(c, sim::nav::Surface::Water);
                sim.nav.set_current(c, (drift * sim::nav::SWIM_SPEED_M_S, 0.0));
            }
            let step = sim.nav.move_cost(from, cells[0], 1.0).unwrap();
            let want = sim::nav::SWIM_COST / (1.0 + drift);
            assert!((step - want).abs() < 1e-5, "{step} vs {want}");
            // Halfway through the step's budget, it is drawn halfway there.
            agent.path = Some(sim::nav::Path { steps: cells.iter().map(|&c| (c, sim::nav::Gait::Swim)).collect(), cost: 0.0 });
            agent.step_frac = step / 2.0;
            let half = agent_world_pos(&sim, &agent, 0.0);
            assert!(Vec2::new(half.x, half.z).distance(cell_xz(&sim, (3.0, 2.5))) < 1e-4);
            // And the trail reaches the first cell when the simulation did.
            agent.step_frac = 0.0;
            let mut motion = Motion::default();
            motion.observe(&sim, &agent, false);
            agent.cell = cells[1];
            agent.path = None;
            agent.trail = cells.iter().map(|&c| (c, sim::nav::Gait::Swim)).collect();
            sim.time_days = sim::STEP_DAYS as f64;
            motion.observe(&sim, &agent, false);
            let arrived = motion.samples[1].0;
            assert!((arrived - step as f64 / move_cost).abs() < 1e-7, "{arrived}");
        }
    }
}

/// A beaver diving into its lodge or coming up out of it: when it went in or out, and where it
/// was last seen outside.
#[derive(Component)]
pub struct LodgeDive {
    inside: bool,
    since: f32,
    at: Vec3,
}

/// A newborn's first appearance: it grows from small to its size in a moment.
#[derive(Component)]
pub struct Newborn(f32);

/// Seconds a dive into or out of a lodge takes.
const DIVE_S: f32 = 0.7;
const POP_S: f32 = 0.8;

/// The agent for a beaver: one of the colony's, or one walking off the map, with its age.
fn find_agent(sim: &sim::SimState, id: sim::colony::BeaverId) -> Option<(&sim::agents::Agent, f32)> {
    if let Some(ag) = sim.agents.get(id) {
        let age = sim.colony.beaver(id).map_or(sim::colony::ADULT_DAYS, |b| b.age_days);
        return Some((ag, age));
    }
    sim.agents.remains.leaving.iter().find(|(a, _)| a.id == id).map(|(a, age)| (a, *age))
}

fn ramp(a: f32, b: f32, x: f32) -> f32 {
    ((x - a) / (b - a)).clamp(0.0, 1.0)
}

/// How a beaver's needs, health, age and mood show.
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
    mut commands: Commands,
    sim: Res<Sim>,
    pending: Res<PendingDays>,
    clock: Option<Res<SimClock>>,
    render: Res<super::RenderClock>,
    slaps: Res<super::TailSlapTimes>,
    time: Res<Time>,
    mut q: Query<(Entity, &mut Transform, &mut BeaverSprite, &mut Visibility, &mut Motion, &mut LodgeDive, &mut super::harbor::LockGlide, Option<&Newborn>), With<SimBeaver>>,
    mut seen: Local<HashSet<u64>>,
) {
    use super::scene::Pose;
    let _p = ::sim::prof::scope("game/sync sim beavers");
    seen.clear();
    let s = &sim.0;
    let t = render.0;
    let now = clock.as_ref().map_or(s.time_days + pending.0 as f64, |c| c.render_days(s.time_days));
    // Who is idle where, by family, for grooming.
    let idle: Vec<(sim::colony::BeaverId, sim::colony::FamilyId, (usize, usize))> = s
        .agents
        .by_id
        .values()
        .filter(|a| !a.inside && a.path.is_none() && matches!(a.task, sim::agents::Task::Idle | sim::agents::Task::Rest) && a.assignment.is_none() && s.work.slot_of(a.id).is_none())
        .filter_map(|a| s.colony.beaver(a.id).map(|b| (a.id, b.family, a.cell)))
        .collect();
    let groom_partner = |id: sim::colony::BeaverId| -> Option<(usize, usize)> {
        let &(_, fam, at) = idle.iter().find(|x| x.0 == id)?;
        idle.iter().filter(|x| x.0 != id && x.1 == fam && x.2.0.abs_diff(at.0).max(x.2.1.abs_diff(at.1)) <= 1).map(|x| x.2).next()
    };
    let chambers = super::harbor::chambers(s);
    for (e, mut tr, mut sprite, mut vis, mut motion, mut dive, mut glide, newborn) in &mut q {
        let id = sim::colony::BeaverId(sprite.id);
        let Some((ag, age)) = find_agent(s, id) else {
            commands.entity(e).despawn();
            continue;
        };
        seen.insert(sprite.id);
        let p = agent_world_pos(s, ag, 0.0) + spread(sprite.id, clock.is_some());
        let ground = |xz: Vec2| height_at(&s.height, xz.x, xz.y);
        motion.observe(s, ag, clock.is_some());
        let mut heading = match motion.at(now) {
            Some((xz, lift, dir)) => {
                tr.translation = Vec3::new(xz.x, ground(xz) + lift, xz.y);
                dir
            }
            None => {
                tr.translation = p;
                None
            }
        };
        // In a lock's working it waits, rides the chamber and swims out as the lock goes, at the
        // moment shown.
        if let Some((at, yaw)) = super::harbor::lock_pose(s, ag, (now - s.time_days) as f32, tr.translation) {
            tr.translation = at;
            if let Some(yaw) = yaw {
                heading = Some(Vec2::new(yaw.sin(), yaw.cos()));
            }
        }
        // Glided over any jump by a lock.
        let at = super::harbor::glide(&mut glide, &chambers, tr.translation, time.delta_secs());
        tr.translation = at;
        let fleeing = s.agents.fleeing.contains(&id);
        sprite.pose = if heading.is_some() {
            match ag.gait {
                sim::nav::Gait::Swim => Pose::Swim,
                sim::nav::Gait::Climb => Pose::Climb,
                sim::nav::Gait::Walk if fleeing => Pose::Run,
                sim::nav::Gait::Walk if ag.load.is_some() => Pose::Carry,
                sim::nav::Gait::Walk => Pose::Walk,
            }
        } else { pose_of(s, ag) };
        sprite.load = if matches!(ag.task, sim::agents::Task::Raft { .. }) { None } else { load_of(ag) };
        sprite.wet = is_wet(s, ag);
        let want = heading.map(|d| d.x.atan2(d.y)).or_else(|| facing_of(s, ag)).unwrap_or(sprite.yaw);
        sprite.yaw = super::harbor::turn(&mut glide, want, time.delta_secs());
        let face = |at: Vec3, to: (f32, f32)| {
            let w = cell_xz(s, to);
            (w.x - at.x).atan2(w.y - at.z)
        };
        // Pinned under the wolf, facing it.
        if let Some(w) = s.predators.wolves.iter().find(|w| matches!(w.state, sim::predators::WolfState::Kill { target, .. } if target == id)) {
            sprite.pose = Pose::Caught;
            sprite.yaw = face(tr.translation, w.pos);
        }
        // A new pair together at their lodge, facing each other.
        if heading.is_none() {
            if let Some(c) = s.agents.courting.get(&id).filter(|c| c.met.is_some()) {
                if let Some(m) = s.agents.get(c.partner) {
                    sprite.pose = Pose::Court;
                    sprite.yaw = face(tr.translation, (m.cell.0 as f32 + 0.5, m.cell.1 as f32 + 0.5));
                }
            } else if sprite.pose == Pose::Idle {
                // Grooming a family member idling beside it, now and then.
                if let Some(m) = groom_partner(id) {
                    let turn = (t * 0.07 + (sprite.id % 7) as f32 * 0.13).rem_euclid(1.0) < 0.45;
                    if turn {
                        sprite.pose = Pose::Groom;
                        sprite.yaw = face(tr.translation, (m.0 as f32 + 0.5, m.1 as f32 + 0.5));
                    }
                }
            }
        }
        let mut look = look_of(s, id);
        look.slap = slaps.0.get(&sprite.id).copied().unwrap_or(-100.0);
        // Into the lodge: it dives where it was last seen outside, then is gone; out of it: it
        // comes up from under the water.
        if ag.inside != dive.inside {
            dive.inside = ag.inside;
            dive.since = t;
        }
        let dt = t - dive.since;
        let shown = if ag.inside {
            if dt < DIVE_S {
                tr.translation = dive.at;
                look.sink = ramp(0.0, DIVE_S, dt);
                sprite.pose = Pose::Swim;
                true
            } else {
                false
            }
        } else {
            dive.at = tr.translation;
            if dt < DIVE_S {
                look.sink = 1.0 - ramp(0.0, DIVE_S, dt);
            }
            true
        };
        sprite.look = look;
        let mut size = BEAVER_SIZE * super::age_scale_days(age) * super::looks_size(&sprite.features);
        if let Some(n) = newborn {
            let k = ramp(0.0, POP_S, t - n.0);
            size *= 0.2 + 0.8 * (1.0 - (1.0 - k) * (1.0 - k));
            if k >= 1.0 {
                commands.entity(e).remove::<Newborn>();
            }
        }
        tr.scale = Vec3::splat(size);
        let want = if shown { Visibility::Inherited } else { Visibility::Hidden };
        if *vis != want {
            *vis = want;
        }
    }
    let agents = s.agents.by_id.values().map(|a| (a, s.colony.beaver(a.id).map_or(sim::colony::ADULT_DAYS, |b| b.age_days)))
        .chain(s.agents.remains.leaving.iter().map(|(a, age)| (a, *age)));
    for (ag, age) in agents {
        let id = ag.id;
        if seen.contains(&id.0) {
            continue;
        }
        let yaw = facing_of(s, ag).unwrap_or(0.0);
        let mut motion = Motion::default();
        motion.observe(s, ag, clock.is_some());
        let at = agent_world_pos(s, ag, 0.0) + spread(id.0, clock.is_some());
        let mut e = commands.spawn((
            SimBeaver,
            motion,
            super::harbor::LockGlide::default(),
            BeaverSprite { id: id.0, pose: pose_of(s, ag), load: load_of(ag), yaw, wet: is_wet(s, ag), look: look_of(s, id), features: s.colony.looks_of(id) },
            LodgeDive { inside: ag.inside, since: -100.0, at },
            Transform::from_translation(at).with_rotation(Quat::from_rotation_y(yaw)).with_scale(Vec3::splat(BEAVER_SIZE * super::age_scale_days(age) * super::looks_size(&s.colony.looks_of(id)))),
            if ag.inside { Visibility::Hidden } else { Visibility::Inherited },
        ));
        // A beaver a day or two old is just born: it pops into view.
        if age < 2.0 {
            e.insert(Newborn(t));
        }
    }
}

/// A body on the ground (see `sim::agents::Remains`).
#[derive(Component)]
pub struct BodySprite(u64);

/// Bodies: each falls where the beaver died and lies there, sinking away at the end.
pub(super) fn sync_bodies(
    mut commands: Commands,
    sim: Res<Sim>,
    render: Res<super::RenderClock>,
    mut q: Query<(Entity, &BodySprite, &mut BeaverSprite)>,
) {
    use super::scene::Pose;
    let s = &sim.0;
    let bodies = &s.agents.remains.bodies;
    let mut have = HashSet::new();
    for (e, b, mut sprite) in &mut q {
        match bodies.iter().find(|x| x.id.0 == b.0) {
            Some(body) => {
                let k = ((s.time_days - body.died) / sim::agents::REMAINS_DAYS) as f32;
                sprite.look.fade = ramp(0.8, 1.0, k);
                have.insert(b.0);
            }
            None => commands.entity(e).despawn(),
        }
    }
    for body in bodies.iter().filter(|b| !have.contains(&b.id.0)) {
        use sim::colony::DeathCause as C;
        // Cold, old age and exhaustion leave it curled up; the rest on its side.
        let curled = matches!(body.cause, C::Cold | C::OldAge | C::Exhaustion);
        // One seen falling now; one already lying there (after a load) has long fallen.
        let fresh = s.time_days - body.died < 0.05;
        let mut look = super::Look { dead: Some((if fresh { render.0 } else { render.0 - 10.0 }, curled)), ..Default::default() };
        if body.cause == C::Starvation {
            look.thin = 1.0;
        }
        let c = (body.cell.0 as f32 + 0.5, body.cell.1 as f32 + 0.5);
        let xz = cell_xz(s, c);
        let y = stand_y(s, body.cell);
        let yaw = (body.id.0 as f32 * 2.399).rem_euclid(std::f32::consts::TAU);
        commands.spawn((
            BodySprite(body.id.0),
            BeaverSprite { id: body.id.0, pose: Pose::Dead, load: None, yaw, wet: false, look, features: s.colony.looks_of(body.id) },
            Transform::from_xyz(xz.x, y, xz.y).with_rotation(Quat::from_rotation_y(yaw)).with_scale(Vec3::splat(BEAVER_SIZE * super::age_scale_days(body.age_days) * super::looks_size(&s.colony.looks_of(body.id)))),
            Visibility::Inherited,
        ));
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ResSig {
    Tree(u8),
    Felled(u8, bool),
    /// A food plant on land: its species and the quarters left.
    Plant(u8, u8),
    /// Water plants: species, how much is left, and the water level in tenths so they ride on it.
    Floating(u8, u8, i16),
    /// Pebbles: the water level in tenths for a gravel bar in a stream (none on land), and the
    /// quarters left.
    Pebbles(Option<i16>, u8),
    /// Clay: a river's seam or a bank's heap, and the eighths left.
    Clay(bool, u8),
}

/// Drawn by the water's level: water plants, and gravel bars in the streams.
fn rides_water(r: &sim::resources::Resource) -> bool {
    r.kind.is_food_plant() || (r.kind == ResourceKind::Pebbles && r.wet)
}

/// A felled tree falls before its logs show: the standing tree is drawn again, tipping toward
/// where the logs lie (+x of the felled mesh), and removed when it lands.
fn spawn_topple(commands: &mut Commands, models: &Models, sim: &sim::SimState, r: &sim::resources::Resource, season: models::Season, now: f32) {
    let c = r.cell;
    let sp = species_of(r);
    let scale = stage_scale(sim::forestry::TreeStage::Mature) * TREE_SCALE * (0.8 + 0.4 * hash01(c.0, c.1, 5));
    let tr = res_transform(sim, r, scale);
    let variant = ((hash01(c.0, c.1, 17) * 4.0) as usize).min(3);
    commands.spawn((SimEntity, Mesh3d(models.tree(sp, season, 0)), MeshMaterial3d(models.tree_tints[variant].clone()), tr, super::Topple { start: now, base: tr }));
}

/// `depth` is the water over the resource's cell; only water plants use it. The rest is the
/// simulation's own account of how the resource looks, which is also what it reports changes of.
pub fn res_sig(r: &sim::resources::Resource, depth: f32) -> ResSig {
    use sim::resources::Look;
    match r.look() {
        Look::Tree(stage, step) => ResSig::Tree(stage as u8 * 16 + step),
        Look::Felled(logs, branches) => ResSig::Felled(logs, branches),
        Look::Clay(q) => ResSig::Clay(r.kind == ResourceKind::RiverClay, q),
        Look::Plant(q) => ResSig::Plant(r.plant.map_or(u8::MAX, |p| p as u8), q),
        Look::Pebbles(q) => ResSig::Pebbles(r.wet.then(|| (depth * 10.0).round() as i16), q),
        Look::WaterPlant(q) => ResSig::Floating(r.plant.map_or(u8::MAX, |p| p as u8), q, (depth * 10.0).round() as i16),
    }
}

fn hash01(x: usize, y: usize, salt: u32) -> f32 {
    let mut h = (x as u32).wrapping_mul(0x9E37_79B1) ^ (y as u32).wrapping_mul(0x85EB_CA6B) ^ salt.wrapping_mul(0xC2B2_AE35);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2C1B_3C6D);
    h ^= h >> 12;
    (h & 0xffff) as f32 / 65535.0
}

fn species_of(r: &sim::resources::Resource) -> Species {
    r.species.map_or(Species::Maple, Species::of)
}

/// The plants: what shows of them follows the water over their cell as well as their look.
#[derive(Default)]
struct Plants {
    /// Each plant, the tile its cell is in, and whether it floats (a water plant).
    at: Vec<(ResourceId, usize, bool)>,
    known: HashSet<ResourceId>,
    /// Each tile's water stamps when its plants were last looked at (`plant_water`), and the
    /// stamps of the whole grids then.
    tiles: Vec<(u64, u64)>,
    grids: Option<[(u64, u64); 3]>,
}

#[derive(Default)]
pub struct WorldSync {
    seed: Option<(u64, u32)>,
    time: f64,
    res: HashMap<ResourceId, (Entity, ResSig)>,
    /// How many of the simulation's noted changes have been drawn, and the sources it had
    /// only guessed at then.
    drawn: Option<u64>,
    guessed: Vec<ResourceId>,
    plants: Plants,
    bld: HashMap<BuildingId, (Entity, (BuildingKind, (usize, usize), u8))>,
    anchors: HashMap<BuildingId, f32>,
    /// Each building's wear band and condition as last drawn, and when it last puffed dust.
    wear: HashMap<BuildingId, (u8, f32, f32)>,
    lodges: HashMap<u64, (Entity, f32)>,
    /// The fog's revision when the sources were last drawn.
    fog: u64,
    /// The outcrops' boulders drawn so far (drawn once their ground is explored).
    rocks: Vec<Option<Entity>>,
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

/// A growing tree's size: from its stage's towards the next's as it grows through the stage.
pub fn growing_scale(st: sim::forestry::TreeStage, growth: f32) -> Vec3 {
    use sim::forestry::TreeStage::*;
    let next = match st {
        Sapling => Young,
        Young => Mature,
        s => s,
    };
    stage_scale(st).lerp(stage_scale(next), growth)
}

/// What else a source's entity carries: a tree's level of detail, a plant's season.
enum Extra {
    None,
    Tree(TreeLod),
    Plant(PlantLook),
}

fn res_visual(models: &Models, r: &sim::resources::Resource, season: models::Season) -> (Handle<Mesh>, Vec3, Extra) {
    let sig = res_sig(r, 0.0);
    let c = r.cell;
    let hash_scale = 0.8 + 0.4 * hash01(c.0, c.1, 5);
    let k = WORLD_SCALE;
    let tree = TREE_SCALE;
    match sig {
        ResSig::Tree(_) if r.snag => (models.snag.clone(), Vec3::splat(tree * hash_scale), Extra::None),
        ResSig::Tree(_) => {
            let sp = species_of(r);
            let st = r.stage().unwrap_or(sim::forestry::TreeStage::Mature);
            (models.tree(sp, season, 0), growing_scale(st, r.growth()) * tree * hash_scale, Extra::Tree(TreeLod { species: sp, lod: 0, season, cutaway: false }))
        }
        ResSig::Felled(logs, branches) => (models.felled[logs as usize * 2 + branches as usize].clone(), Vec3::splat(k), Extra::None),
        ResSig::Plant(_, q) | ResSig::Floating(_, q, _) => {
            use sim::plants::Plant;
            let plant = r.plant.unwrap_or(if r.kind == ResourceKind::WaterPlant { Plant::YellowLily } else { Plant::Sedge });
            let s = (0.55 + 0.45 * q as f32 / 4.0) * (0.9 + 0.2 * hash01(c.0, c.1, 23));
            (models.plant(plant, season), Vec3::splat(s), Extra::Plant(PlantLook { plant, season }))
        }
        ResSig::Pebbles(stream, q) => {
            let i = (q.clamp(1, 4) - 1) as usize;
            (if stream.is_some() { models.gravel_bars[i].clone() } else { models.pebble_heaps[i].clone() }, Vec3::splat(0.9 + 0.2 * hash01(c.0, c.1, 29)), Extra::None)
        }
        ResSig::Clay(river, q) => {
            let s = (0.5 + 0.5 * q as f32 / 8.0) * (0.95 + 0.15 * hash01(c.0, c.1, 31));
            (if river { models.river_clay.clone() } else { models.clay_heap.clone() }, Vec3::splat(s), Extra::None)
        }
    }
}

fn res_transform(sim: &sim::SimState, r: &sim::resources::Resource, scale: Vec3) -> Transform {
    let c = r.cell;
    let p = cell_xz(sim, (c.0 as f32 + 0.5, c.1 as f32 + 0.5));
    let ground = height_at(&sim.height, p.x, p.y);
    // Water plants float on the surface, so they rise and fall with the water.
    let y = match r.kind {
        ResourceKind::Tree => ground - 0.05,
        ResourceKind::WaterPlant => ground + sim.water.depth.get(c.0, c.1).copied().unwrap_or(0.0).max(0.0),
        // A gravel bar breaks the surface of a shallow stream; in a deeper one its top shows
        // just above the water.
        ResourceKind::Pebbles if r.wet => {
            let surface = ground + sim.water.depth.get(c.0, c.1).copied().unwrap_or(0.0).max(0.0);
            ground.max(surface + 0.04 - super::deposits::GRAVEL_BAR_H)
        }
        _ => ground,
    };
    let yaw = hash01(c.0, c.1, 9) * std::f32::consts::TAU;
    Transform::from_xyz(p.x, y, p.y).with_rotation(Quat::from_rotation_y(yaw)).with_scale(scale)
}

fn material_for(models: &Models, r: &sim::resources::Resource) -> Handle<StandardMaterial> {
    if r.stage() == Some(sim::forestry::TreeStage::Old) { models.old_tree.clone() }
    else if r.kind == ResourceKind::Tree {
        let variant = (hash01(r.cell.0, r.cell.1, 17) * 4.0) as usize;
        models.tree_tints[variant.min(3)].clone()
    } else if matches!(r.kind, ResourceKind::MudBank | ResourceKind::RiverClay) { models.wet.clone() }
    else { models.white.clone() }
}

/// A plant under water, or on a riverbed, does not show. Both are taken in the steps the
/// simulation stamps its tiles by, so a plant is looked at again exactly when this can change.
/// Plants that stand in the shallows (cattail, horsetail) show until the water tops them.
fn hidden(sim: &sim::SimState, r: &sim::resources::Resource) -> bool {
    let stands = r.plant.map_or(0.0, |p| p.spec().max_depth);
    r.kind == ResourceKind::Plant
        && (sim.water.depth.get(r.cell.0, r.cell.1).is_some_and(|&d| if stands > 0.05 { d > stands + 0.1 } else { sim::water::shore_step(d) > 0 })
            || sim.water.riverbed.get(r.cell.0, r.cell.1).is_some_and(|&b| (b * sim::water::RIVERBED_STEPS).round() > 3.0))
}

/// Draw one resource as it now is: spawn it, change its model, or leave it.
fn show_resource(commands: &mut Commands, res: &mut HashMap<ResourceId, (Entity, ResSig)>, models: &Models, sim: &sim::SimState, r: &sim::resources::Resource, season: models::Season, now: f32) {
    let sig = res_sig(r, sim.water.depth.get(r.cell.0, r.cell.1).copied().unwrap_or(0.0));
    match res.get(&r.id).copied() {
        Some((_, old)) if old == sig => {}
        Some((e, old)) => {
            if matches!(old, ResSig::Tree(_)) && matches!(sig, ResSig::Felled(..)) && !r.snag {
                spawn_topple(commands, models, sim, r, season, now);
            }
            let (mesh, scale, extra) = res_visual(models, r, season);
            let mut ec = commands.entity(e);
            let tr = res_transform(sim, r, scale);
            // A new model needs new bounds for picking and culling.
            ec.insert((Mesh3d(mesh), MeshMaterial3d(material_for(models, r)), tr)).remove::<bevy::camera::primitives::Aabb>();
            match extra {
                Extra::Tree(l) => ec.insert((l, Sway(tr.rotation))).remove::<PlantLook>(),
                Extra::Plant(p) => ec.insert(p).remove::<(TreeLod, Sway)>(),
                Extra::None => ec.remove::<(TreeLod, Sway, PlantLook)>(),
            };
            res.insert(r.id, (e, sig));
        }
        None => {
            let (mesh, scale, extra) = res_visual(models, r, season);
            let tr = res_transform(sim, r, scale);
            let mut ec = commands.spawn((SimEntity, Mesh3d(mesh), MeshMaterial3d(material_for(models, r)), tr));
            match extra {
                Extra::Tree(l) => { ec.insert((l, Sway(tr.rotation))); }
                Extra::Plant(p) => { ec.insert(p); }
                Extra::None => {}
            }
            if matches!(r.kind, ResourceKind::Tree | ResourceKind::Felled) {
                ec.insert(Target::Resource(r.id));
            }
            let e = ec.id();
            res.insert(r.id, (e, sig));
        }
    }
}

/// Each tile's water stamps, as far as plants care: the water's edge and the riverbed, for
/// plants on land; the depth, for those that float on it.
fn plant_water(sim: &sim::SimState) -> Vec<(u64, u64)> {
    let (depth, edge, bed) = (&sim.water.depth, &sim.water.shore, &sim.water.riverbed);
    let (cols, rows) = (depth.width().div_ceil(sim::grid::TILE), depth.height().div_ceil(sim::grid::TILE));
    let mix = |a: u64, b: u64| (a ^ b).wrapping_mul(0x9E37_79B9_7F4A_7C15).rotate_left(29);
    (0..cols * rows)
        .map(|t| {
            let (x, y) = (t % cols, t / cols);
            (mix(mix(mix(edge.origin(), edge.tile_stamp(x, y)), bed.origin()), bed.tile_stamp(x, y)), mix(depth.origin(), depth.tile_stamp(x, y)))
        })
        .collect()
}

/// Trees, plants and deposits. The simulation notes each one that is added, removed or comes
/// to look different, and stamps the tiles where water changed, so only those are looked at;
/// all of them are compared only at the start, or when the notes no longer reach back.
pub(super) fn sync_sim_resources(
    mut commands: Commands,
    sim: Res<Sim>,
    season: Res<CurrentSeason>,
    models: Option<Res<Models>>,
    flow: Option<Res<crate::flow::Flow>>, mut st: Local<WorldSync>,
    clock: Res<super::RenderClock>,
) {
    let _p = ::sim::prof::scope("game/sync sim resources");
    let Some(models) = models else { return };
    let generation = flow.map_or(0, |f| f.generation);
    let sim = &sim.0;
    if st.seed != Some((sim.seed, generation)) {
        for (e, _) in st.res.drain().map(|(_, v)| v) {
            commands.entity(e).despawn();
        }
        for e in st.rocks.drain(..).flatten() {
            commands.entity(e).despawn();
        }
        st.seed = Some((sim.seed, generation));
        st.drawn = None;
    }
    let st = &mut *st;
    let no_trees = std::env::var_os("BC_NO_TREES").is_some();
    let shown = |r: &sim::resources::Resource| !(no_trees && r.kind == ResourceKind::Tree) && !hidden(sim, r) && !crate::fog_render::unexplored(sim, r.cell);
    let tile_of = |r: &sim::resources::Resource| (r.cell.1 / sim::grid::TILE) * sim.water.depth.width().div_ceil(sim::grid::TILE) + r.cell.0 / sim::grid::TILE;
    let changes = sim.resources.changes();
    let water = [sim.water.depth.stamp(), sim.water.shore.stamp(), sim.water.riverbed.stamp()];
    let noted = st.drawn.and_then(|drawn| changes.since(drawn)).filter(|_| !st.res.is_empty());
    match noted {
        None => {
            // Everything, once: what is there, and which of it are plants.
            let _p = ::sim::prof::scope("game/sync sim resources/all of them");
            st.plants = Plants::default();
            let mut live: HashSet<ResourceId> = HashSet::with_capacity(sim.resources.items().len());
            for r in sim.resources.items() {
                if rides_water(r) {
                    st.plants.known.insert(r.id);
                    st.plants.at.push((r.id, tile_of(r), r.kind == ResourceKind::WaterPlant));
                }
                if shown(r) {
                    live.insert(r.id);
                    show_resource(&mut commands, &mut st.res, &models, sim, r, season.0, clock.0);
                }
            }
            let gone: Vec<ResourceId> = st.res.keys().copied().filter(|id| !live.contains(id)).collect();
            for id in gone {
                if let Some((e, _)) = st.res.remove(&id) {
                    commands.entity(e).despawn();
                }
            }
            st.plants.tiles = plant_water(sim);
        }
        Some(noted) => {
            let mut look: Vec<ResourceId> = noted.chain(st.guessed.drain(..)).collect();
            if st.plants.grids != Some(water) {
                // Water was written somewhere: the plants in the tiles where it was.
                let now = plant_water(sim);
                let was = &st.plants.tiles;
                let moved = |t: usize| now.len() != was.len() || now[t] != was[t];
                look.extend(st.plants.at.iter().filter(|(_, t, _)| moved(*t)).map(|(id, _, _)| *id));
                st.plants.tiles = now;
            }
            look.sort_unstable();
            look.dedup();
            let mut lost = false;
            for id in look {
                let r = sim.resources.get(id);
                match r {
                    Some(r) if rides_water(r) => {
                        if st.plants.known.insert(id) {
                            st.plants.at.push((id, tile_of(r), r.kind == ResourceKind::WaterPlant));
                        }
                    }
                    _ => lost |= st.plants.known.remove(&id),
                }
                match r.filter(|r| shown(r)) {
                    Some(r) => show_resource(&mut commands, &mut st.res, &models, sim, r, season.0, clock.0),
                    None => {
                        if let Some((e, _)) = st.res.remove(&id) {
                            commands.entity(e).despawn();
                        }
                    }
                }
            }
            if lost {
                let known = &st.plants.known;
                st.plants.at.retain(|(id, _, _)| known.contains(id));
            }
            // More ground explored: draw what grows there.
            if st.fog != sim.fog.revision {
                let _p = ::sim::prof::scope("game/sync sim resources/fog cleared");
                for r in sim.resources.items() {
                    if !st.res.contains_key(&r.id) && shown(r) {
                        show_resource(&mut commands, &mut st.res, &models, sim, r, season.0, clock.0);
                    }
                }
            }
        }
    }
    // The boulders: scenery that never changes once the world is made.
    if st.rocks.len() != sim.resources.boulders.len() {
        for e in st.rocks.drain(..).flatten() {
            commands.entity(e).despawn();
        }
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
    let p = cell_xz(sim, (b.cell.0 as f32 + 0.5, b.cell.1 as f32 + 0.5));
    // Bedded at the lowest corner of its cell, so it never floats on a slope.
    let ground = [(-0.45, -0.45), (0.45, -0.45), (-0.45, 0.45), (0.45, 0.45)].iter().map(|(dx, dz)| height_at(&sim.height, p.x + dx, p.y + dz)).fold(f32::MAX, f32::min);
    let yaw = (b.seed % 360) as f32 * std::f32::consts::TAU / 360.0;
    Transform::from_xyz(p.x, ground, p.y).with_rotation(Quat::from_rotation_y(yaw)).with_scale(Vec3::new(b.size, b.size * 0.9, b.size))
}

pub fn model_kind(k: BuildingKind) -> StructKind {
    match k {
        BuildingKind::Dam => StructKind::Dam,
        BuildingKind::Shelter => StructKind::Lodge,
        BuildingKind::Warehouse => StructKind::Warehouse,
        BuildingKind::Larder => StructKind::Larder,
        BuildingKind::Toolshed => StructKind::Toolshed,
        BuildingKind::Forester => StructKind::Forester,
        BuildingKind::Logistics => StructKind::Logistics,
        BuildingKind::Lookout => StructKind::Lookout,
        BuildingKind::Harbor => StructKind::Harbor,
        BuildingKind::Canal => StructKind::Canal,
    }
}

pub fn build_progress(sim: &sim::SimState, b: &sim::buildings::Building) -> f32 {
    if b.status == Status::Complete { 1.0 } else { sim.work.progress(b.id).unwrap_or(0.0).clamp(0.0, 0.999) }
}

#[derive(Default)]
pub struct ReinforcementSync {
    generation: Option<(u64,u32)>,
    drawn: HashMap<(BuildingId,bool),(Entity,f32)>,
    meshes: HashMap<(BuildingKind,bool),Handle<Mesh>>,
}

pub(super) fn sync_reinforcements(mut commands:Commands,sim:Res<Sim>,flow:Res<crate::flow::Flow>,models:Option<Res<Models>>,
    mut meshes:ResMut<Assets<Mesh>>,mut state:Local<ReinforcementSync>) {
    let _p = ::sim::prof::scope("game/sync reinforcements");
    let Some(models)=models else{return};let s=&sim.0;
    let generation=(s.seed,flow.generation);
    if state.generation!=Some(generation) {
        for (_, (e,_)) in state.drawn.drain(){commands.entity(e).despawn();}
        state.generation=Some(generation);
    }
    let mut live=HashSet::new();
    // Clay reinforcement and pebble facing, each its own ring round the footing.
    let layers=s.buildings.buildings.iter().flat_map(|b|[(b,false,b.reinforced),(b,true,b.kind!=BuildingKind::Dam&&s.buildings.pebbled.contains(&b.id))]).filter(|l|l.2);
    for (b,pebbled,_) in layers {
        let key=(b.id,pebbled);
        live.insert(key);
        // Rises with the building when the pond does, never down.
        let was=state.drawn.get(&key).map(|v|v.1);
        let tr=building_transform(s,b,was);
        if let Some(&(e,y))=state.drawn.get(&key) {
            if tr.translation.y>y+0.01 {
                commands.entity(e).insert(tr);
                state.drawn.insert(key,(e,tr.translation.y));
            }
            continue;
        }
        let handle=state.meshes.entry((b.kind,pebbled)).or_insert_with(||meshes.add(if pebbled {models::pebble_facing_mesh(model_kind(b.kind))} else {models::reinforcement_mesh(model_kind(b.kind))})).clone();
        let entity=commands.spawn((Target::Building(b.id),SimEntity,Mesh3d(handle),MeshMaterial3d(models.white.clone()),tr)).id();
        state.drawn.insert(key,(entity,tr.translation.y));
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

#[cfg(test)]
mod building_height_tests {
    use super::*;

    #[test]
    fn a_rising_pond_lifts_a_building_and_a_falling_one_leaves_it() {
        let mut s = sim::SimState::new(1);
        let b = sim::buildings::Building { id: BuildingId(1), kind: BuildingKind::Warehouse, pos: (10, 10), status: Status::Complete, output: 0.0, condition: 1.0, reinforced: false, dam: None, canal: None };
        let set_depth = |s: &mut sim::SimState, d: f32| {
            for c in b.cells().collect::<Vec<_>>() {
                *s.water.depth.get_mut(c.0, c.1).unwrap() = d;
            }
        };
        set_depth(&mut s, 0.3);
        let low = building_transform(&s, &b, None).translation.y;
        set_depth(&mut s, 1.5);
        let high = building_transform(&s, &b, Some(low)).translation.y;
        assert!(high > low + 1.0, "{low} -> {high}");
        set_depth(&mut s, 0.3);
        let after = building_transform(&s, &b, Some(high)).translation.y;
        assert_eq!(after, high);
    }
}

/// How worn a building looks: 0 sound, then worn, more worn, falling apart.
fn wear_band(condition: f32) -> u8 {
    if condition >= 0.75 { 0 } else if condition >= 0.5 { 1 } else if condition >= 0.25 { 2 } else { 3 }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn sync_sim_buildings(mut commands: Commands, sim: Res<Sim>, models: Option<ResMut<Models>>, mut meshes: ResMut<Assets<Mesh>>, mut materials: ResMut<Assets<StandardMaterial>>, mut puff_mesh: ResMut<super::effects::PuffMesh>, clock: Res<super::RenderClock>, flow: Option<Res<crate::flow::Flow>>, mut st: Local<WorldSync>) {
    let _p = ::sim::prof::scope("game/sync sim buildings");
    let Some(mut models) = models else { return };
    let changed = sim.is_changed();
    let generation = flow.map_or(0, |f| f.generation);
    let sim = &sim.0;
    if st.seed != Some((sim.seed, generation)) {
        for (e, _) in st.bld.drain().map(|(_, v)| v) {
            commands.entity(e).despawn();
        }
        for (_, (e, _)) in st.lodges.drain() {
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
    let mut puffs: Vec<(Vec3, f32)> = Vec::new();
    let mut spawn = |commands: &mut Commands, models: &mut Models, target: Target, kind: StructKind, len2: u32, stage: u8, tr: Transform| -> Entity {
        let mesh = models.structs.entry((kind, len2, stage)).or_insert_with(|| meshes.add(models::struct_mesh(kind, len2 as f32 * 0.5, stage))).clone();
        commands.spawn((target, SimEntity, Mesh3d(mesh), MeshMaterial3d(white.clone()), tr)).id()
    };
    for b in &sim.buildings.buildings {
        if b.kind == BuildingKind::Dam && (b.is_complete() || sim.work.dam_plans.iter().any(|p| p.building == b.id)) {
            continue;
        }
        // A canal is the ground it digs; its gate shows as a dam cell.
        if b.kind == BuildingKind::Canal {
            continue;
        }
        live.insert(b.id);
        let stage = if sim.buildings.stranded(b, &sim.water.depth) { models::DRY_STAGE } else { models::stage_of(build_progress(sim, b)) };
        let sig = (b.kind, b.pos, stage);
        // Wear: a finished building darkens and sags as it wears; mending it puffs dust.
        let band = if b.is_complete() { wear_band(b.condition) } else { 0 };
        let (was_band, was_cond, last_puff) = st.wear.get(&b.id).copied().unwrap_or((band, b.condition, -100.0));
        let centre = {
            let (w, d) = b.kind.spec().footprint;
            let c = cell_xz(sim, (b.pos.0 as f32 + w as f32 / 2.0, b.pos.1 as f32 + d as f32 / 2.0));
            Vec3::new(c.x, height_at(&sim.height, c.x, c.y), c.y)
        };
        let mut puffed = last_puff;
        if b.condition > was_cond + 0.002 && clock.0 - last_puff > 1.2 {
            puffs.push((centre + Vec3::new(0.0, 0.6, 0.0), 1.4));
            puffed = clock.0;
        }
        st.wear.insert(b.id, (band, b.condition, puffed));
        if let Some(&(e, _)) = st.bld.get(&b.id).filter(|(_, s)| *s == sig) {
            if band != was_band {
                let mat = if band == 0 { white.clone() } else { models.wear[(band - 1) as usize].clone() };
                commands.entity(e).insert(MeshMaterial3d(mat));
                let tr = building_transform(sim, b, st.anchors.get(&b.id).copied());
                let sag = 1.0 - 0.04 * band as f32;
                commands.entity(e).insert(super::effects::Grow { base: tr.scale * Vec3::new(1.0, sag, 1.0), from: Vec3::new(1.0, (1.0 - 0.04 * was_band as f32) / sag, 1.0), since: clock.0, secs: 1.5 });
            }
            // A pond rising after the building went up lifts it with the water (never down).
            let anchor = st.anchors.get(&b.id).copied().unwrap_or(f32::MIN);
            let tr = building_transform(sim, b, Some(anchor));
            if tr.translation.y > anchor + 0.01 {
                commands.entity(e).insert(tr);
                st.anchors.insert(b.id, tr.translation.y);
            }
            continue;
        }
        let replaced = st.bld.remove(&b.id).map(|(old, _)| commands.entity(old).despawn()).is_some();
        let (w, _) = b.kind.spec().footprint;
        let tr = building_transform(sim, b, st.anchors.get(&b.id).copied());
        st.anchors.insert(b.id, tr.translation.y);
        let e = spawn(&mut commands, &mut models, Target::Building(b.id), model_kind(b.kind), (w as u32 * 2).max(2), stage, tr);
        if band > 0 {
            commands.entity(e).insert(MeshMaterial3d(models.wear[(band - 1) as usize].clone()));
        }
        // A new stage (or the change to high and dry, or back) rises into place in a puff of
        // dust; so does a new site.
        if replaced || (!b.is_complete() && stage == 0) {
            commands.entity(e).insert(super::effects::Grow { base: tr.scale, from: Vec3::new(1.02, 0.8, 1.02), since: clock.0, secs: 0.6 });
            puffs.push((tr.translation + Vec3::new(0.0, 0.3, 0.0), 1.8));
        }
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
        if let Some((e, _)) = st.lodges.remove(&id) {
            commands.entity(e).despawn();
        }
    }
    let lodge_blds: Vec<(usize, usize)> = sim.buildings.buildings.iter().filter(|b| b.kind == BuildingKind::Shelter).map(|b| b.pos).collect();
    for l in &sim.colony.lodges {
        if lodge_blds.iter().any(|p| (p.0 as f32 + 1.5 - l.pos.0).abs() < 3.0 && (p.1 as f32 + 1.5 - l.pos.1).abs() < 3.0) {
            continue;
        }
        let c = cell_xz(sim, l.pos);
        let ground = (0..9).map(|i| height_at(&sim.height, c.x + (i % 3) as f32 - 1.0, c.y + (i / 3) as f32 - 1.0)).fold(f32::MAX, f32::min);
        // Kept above the pond around it, as that rises.
        let (lx, ly) = (l.pos.0.floor() as i64, l.pos.1.floor() as i64);
        let surface = (0..9)
            .filter_map(|i| {
                let (x, y) = (lx + i % 3 - 1, ly + i / 3 - 1);
                if x < 0 || y < 0 { return None; }
                let (x, y) = (x as usize, y as usize);
                Some(sim.height.get(x, y)? + sim.water.depth.get(x, y).copied().filter(|d| *d > 0.02)?)
            })
            .fold(f32::MIN, f32::max);
        let y = ground.max(surface + 0.08);
        match st.lodges.get(&l.id.0).copied() {
            Some((e, was)) => {
                if y > was + 0.01 {
                    commands.entity(e).insert(Transform::from_xyz(c.x, y, c.y).with_scale(Vec3::splat(BUILDING_SIZE)));
                    st.lodges.insert(l.id.0, (e, y));
                }
            }
            None => {
                let target = Target::home(sim, l.id);
                let e = spawn(&mut commands, &mut models, target, StructKind::Lodge, 6, models::STAGES, Transform::from_xyz(c.x, y, c.y).with_scale(Vec3::splat(BUILDING_SIZE)));
                st.lodges.insert(l.id.0, (e, y));
            }
        }
    }
    drop(spawn);
    for (at, size) in puffs {
        super::effects::puff(&mut commands, &mut meshes, &mut materials, &mut puff_mesh, at, size);
    }
}

/// One cell's stretch of a continuous dam wall: its centre (grid coordinates, hundredths),
/// direction (hundredths of a radian) and length (tenths of a cell), all rounded so a wall
/// that has not moved compares equal.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub struct Piece {
    pub cx: i32,
    pub cz: i32,
    pub yaw: i32,
    pub len: u32,
}

impl Piece {
    fn new(centre: Vec2, along: Vec2) -> Piece {
        let l = along.length().max(0.2);
        // The mesh runs along its x axis: turned by yaw about y, x goes to (cos yaw, -sin yaw).
        let yaw = (-along.y).atan2(along.x);
        Piece { cx: (centre.x * 100.0).round() as i32, cz: (centre.y * 100.0).round() as i32, yaw: (yaw * 100.0).round() as i32, len: ((l + 0.25) * 10.0).round() as u32 }
    }
    pub fn centre(&self) -> (f32, f32) { (self.cx as f32 / 100.0, self.cz as f32 / 100.0) }
    pub fn yaw(&self) -> f32 { self.yaw as f32 / 100.0 }
    /// Which way the mesh's mud face (+z) points, in grid coordinates.
    fn face(&self) -> Vec2 { Vec2::new(self.yaw().sin(), self.yaw().cos()) }
}

/// A line of points averaged over a stretch either side (by distance along it), narrowing to
/// nothing at its ends: a staircase of cells comes out as the straight line it stands for,
/// and corners round off, while the ends stay put.
pub fn smooth_line(pts: &[Vec2], half: f32) -> Vec<Vec2> {
    if pts.len() < 3 {
        return pts.to_vec();
    }
    let mut cum = vec![0.0f32; pts.len()];
    for i in 1..pts.len() {
        cum[i] = cum[i - 1] + pts[i].distance(pts[i - 1]);
    }
    let end = cum[pts.len() - 1];
    let at = |s: f32| {
        let i = cum.partition_point(|&c| c < s).clamp(1, pts.len() - 1);
        let seg = (cum[i] - cum[i - 1]).max(1e-6);
        pts[i - 1].lerp(pts[i], ((s - cum[i - 1]) / seg).clamp(0.0, 1.0))
    };
    (0..pts.len())
        .map(|k| {
            let s = cum[k];
            let h = half.min(s).min(end - s);
            if h < 1e-3 {
                return pts[k];
            }
            let (lo, hi) = (s - h, s + h);
            let mut sum = Vec2::ZERO;
            for i in 1..pts.len() {
                let (a, b) = (cum[i - 1].max(lo), cum[i].min(hi));
                if b > a {
                    sum += (at(a) + at(b)) * 0.5 * (b - a);
                }
            }
            sum / (hi - lo)
        })
        .collect()
}

/// The wall pieces for a set of dam cells (any order): each run of touching cells is walked
/// end to end and smoothed, and every cell owns the stretch of the smoothed line between the
/// midpoints to its neighbours, so the pieces join into one wall at any angle.
pub fn wall_pieces(cells: &[(usize, usize)]) -> HashMap<(usize, usize), Piece> {
    let mut out = HashMap::new();
    let order = sim::dams::chain(cells);
    let touch = |a: (usize, usize), b: (usize, usize)| a.0.abs_diff(b.0) <= 1 && a.1.abs_diff(b.1) <= 1;
    let mut runs: Vec<Vec<(usize, usize)>> = Vec::new();
    for &i in &order {
        match runs.last_mut() {
            Some(r) if touch(*r.last().unwrap(), cells[i]) => r.push(cells[i]),
            _ => runs.push(vec![cells[i]]),
        }
    }
    for run in runs {
        let raw: Vec<Vec2> = run.iter().map(|c| Vec2::new(c.0 as f32 + 0.5, c.1 as f32 + 0.5)).collect();
        if run.len() == 1 {
            out.insert(run[0], Piece::new(raw[0], Vec2::X));
            continue;
        }
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

/// Whether a piece's mud face (+z) should turn round: toward the dam's downstream side, or
/// (for cells no dam owns) toward the lower water.
fn faces_downstream(sim: &sim::SimState, c: (usize, usize), piece: &Piece, downstream: Option<(i32, i32)>) -> bool {
    let f = piece.face();
    if let Some(d) = downstream {
        return f.dot(Vec2::new(d.0 as f32, d.1 as f32)) > 0.0;
    }
    let surface = |side: f32| {
        (1..=2).filter_map(|k| {
            let (x, y) = ((c.0 as f32 + 0.5 + f.x * side * k as f32).floor(), (c.1 as f32 + 0.5 + f.y * side * k as f32).floor());
            if x < 0.0 || y < 0.0 { return None; }
            let (x, y) = (x as usize, y as usize);
            Some(sim.height.get(x, y)? + sim.water.depth.get(x, y).copied().unwrap_or(0.0))
        }).fold(f32::MIN, f32::max)
    };
    surface(-1.0) > surface(1.0) + 0.01
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct DamCellVis {
    pub steps: u8,
    pub stage: DamStage,
    pub piece: Piece,
    /// Turned half round, so the mud-faced side meets the higher water.
    pub flip: bool,
}

/// Every dam cell as it should look: staked, rising beside its pile, or done, as its stretch
/// of a continuous wall. A worn cell is drawn as low as the water now finds it. With each,
/// whether it belongs to no dam (a gate, a leftover): such a cell faces the lower water, which
/// can change sides, where a dam's cells face downstream.
fn dam_walls(sim: &sim::SimState) -> HashMap<(usize, usize), (DamCellVis, bool)> {
    let w = sim.water.dam_height.width();
    let mut cells: HashMap<(usize, usize), (f32, DamStage)> = HashMap::new();
    let integrity = |c: (usize, usize)| sim.water.dam_integrity.get(c.0, c.1).copied().unwrap_or(1.0).clamp(0.0, 1.0);
    for (i, &d) in sim.water.dam_height.as_slice().iter().enumerate() {
        if d > 0.0 {
            let c = (i % w, i / w);
            cells.insert(c, (d * integrity(c), DamStage::Done));
        }
    }
    for d in sim.buildings.buildings.iter().filter_map(|b| b.dam.as_deref()) {
        // A gate's cells stand at its sill, whatever their target.
        for c in d.cells.iter().filter(|c| c.target > 0.0 && !sim::dams::is_gate(d, c.cell)) {
            let now = sim.water.dam_height.get(c.cell.0, c.cell.1).copied().unwrap_or(0.0);
            if now <= 1e-3 {
                cells.insert(c.cell, (DAM_STEP, DamStage::Staked));
            } else if now < c.target - 1e-3 {
                cells.insert(c.cell, (now * integrity(c.cell), DamStage::Rising));
            }
        }
    }
    // A lock stands in place of its gate's wall: the wall stops either side of its chamber.
    cells.retain(|c, _| !sim::locks::is_lock(sim, *c) && !super::harbor::in_chamber(sim, *c));
    // Each dam's rows are walls of their own; cells no dam owns (gates, leftovers) form theirs.
    let mut pieces: HashMap<(usize, usize), (Piece, Option<(i32, i32)>)> = HashMap::new();
    for d in sim.buildings.buildings.iter().filter_map(|b| b.dam.as_deref()) {
        for row in 0..d.rows.max(1) {
            let line: Vec<(usize, usize)> = d.cells.iter().filter(|c| c.row == row && cells.contains_key(&c.cell)).map(|c| c.cell).collect();
            for (c, p) in wall_pieces(&line) {
                pieces.insert(c, (p, Some(d.downstream)));
            }
        }
    }
    let loose: Vec<(usize, usize)> = cells.keys().copied().filter(|c| !pieces.contains_key(c)).collect();
    for (c, p) in wall_pieces(&loose) {
        pieces.insert(c, (p, None));
    }
    cells
        .iter()
        .filter_map(|(&c, &(d, stage))| {
            let (piece, down) = *pieces.get(&c)?;
            let steps = (d / DAM_STEP).round().clamp(1.0, 16.0) as u8;
            Some((c, (DamCellVis { steps, stage, piece, flip: faces_downstream(sim, c, &piece, down) }, down.is_none())))
        })
        .collect()
}

/// Changes whenever a dam does: the simulation's stamps of the two dam grids, and every
/// planned dam's cells and heights.
fn dams_key(sim: &sim::SimState) -> ((u64, u64), (u64, u64), u64) {
    let mut h = 0xcbf2_9ce4_8422_2325u64;
    let mut add = |v: u64| h = (h ^ v).wrapping_mul(0x100_0000_01b3);
    for b in &sim.buildings.buildings {
        let Some(d) = b.dam.as_deref() else { continue };
        add(b.id.0);
        add(d.rows as u64);
        add(d.downstream.0 as u64 ^ (d.downstream.1 as u64) << 32);
        for c in &d.cells {
            add(c.cell.0 as u64 ^ (c.cell.1 as u64) << 24 ^ (c.row as u64) << 48);
            add(c.target.to_bits() as u64);
        }
    }
    (sim.water.dam_height.stamp(), sim.water.dam_integrity.stamp(), h)
}

#[derive(Default)]
pub struct DamSync {
    seed: Option<(u64, u32)>,
    time: f64,
    cells: HashMap<(usize, usize), (Entity, DamCellVis)>,
    /// The walls as last worked out, and the `dams_key` they were worked out for.
    walls: HashMap<(usize, usize), (DamCellVis, bool)>,
    key: Option<(((u64, u64), (u64, u64), u64), usize)>,
}

/// Where a piece of wall stands in the world, turned to its line (and round, if flipped).
fn piece_transform(sim: &sim::SimState, piece: &Piece, flip: bool, lift: impl Fn(f32) -> f32) -> Transform {
    let p = cell_xz(sim, piece.centre());
    let y = lift(height_at(&sim.height, p.x, p.y));
    let yaw = piece.yaw() + if flip { std::f32::consts::PI } else { 0.0 };
    Transform::from_xyz(p.x, y, p.y).with_rotation(Quat::from_rotation_y(yaw))
}

pub(super) fn sync_sim_dams(mut commands: Commands, sim: Res<Sim>, models: Option<ResMut<Models>>, mut meshes: ResMut<Assets<Mesh>>, clock: Res<super::RenderClock>, flow: Option<Res<crate::flow::Flow>>, mut st: Local<DamSync>) {
    let _p = ::sim::prof::scope("game/sync sim dams");
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
    let st = &mut *st;
    let key = (dams_key(sim), sim.locks.at.values().filter(|l| l.built).count());
    if st.key != Some(key) {
        st.walls = dam_walls(sim);
        st.key = Some(key);
    } else {
        // No dam changed: only the cells that turn to face the lower water can have.
        let mut turned = false;
        for (c, (vis, _)) in st.walls.iter_mut().filter(|(_, (_, loose))| *loose) {
            let flip = faces_downstream(sim, *c, &vis.piece, None);
            turned |= flip != vis.flip;
            vis.flip = flip;
        }
        if !turned && st.cells.len() == st.walls.len() {
            return;
        }
    }
    let want: HashMap<(usize, usize), DamCellVis> = st.walls.iter().map(|(c, (vis, _))| (*c, *vis)).collect();
    let gone: Vec<(usize, usize)> = st.cells.iter().filter(|(c, (_, v))| want.get(*c) != Some(v)).map(|(c, _)| *c).collect();
    let mut was_steps: HashMap<(usize, usize), u8> = HashMap::new();
    for c in gone {
        if let Some((e, v)) = st.cells.remove(&c) {
            commands.entity(e).despawn();
            was_steps.insert(c, v.steps);
        }
    }
    let white = models.white.clone();
    for (&c, &vis) in &want {
        if st.cells.contains_key(&c) {
            continue;
        }
        let variant = (hash01(c.0, c.1, 21) * 3.0) as u32 % 3;
        let len = vis.piece.len;
        let mesh = models
            .dams
            .entry((vis.steps, len, variant, vis.stage))
            .or_insert_with(|| meshes.add(super::structures::dam_cell_mesh(vis.steps, len as f32 / 10.0, variant, vis.stage)))
            .clone();
        let tr = piece_transform(sim, &vis.piece, vis.flip, |g| g);
        let e = commands.spawn((SimEntity, Mesh3d(mesh), MeshMaterial3d(white.clone()), tr)).id();
        // A gate going up or down, or a cell built up or worn down, eases to its new height.
        if let Some(&old) = was_steps.get(&c).filter(|&&o| o != vis.steps && vis.steps > 0) {
            let from = (old as f32 / vis.steps as f32).clamp(0.05, 4.0);
            commands.entity(e).insert(super::effects::Grow { base: tr.scale, from: Vec3::new(1.0, from, 1.0), since: clock.0, secs: 1.0 });
        }
        let owner = sim.buildings.buildings.iter().rev().find(|b| b.kind == BuildingKind::Dam && b.cells().any(|cell| cell == c)).map(|b| b.id);
        if let Some(id) = owner { commands.entity(e).insert(Target::Building(id)); }
        st.cells.insert(c, (e, vis));
    }
}

/// While a dam is being drawn, it shows as a see-through wall along the line at the height
/// its first stage will reach; red when the line cannot be built.
#[derive(Default)]
pub struct GhostSync {
    mats: Option<(Handle<StandardMaterial>, Handle<StandardMaterial>)>,
    cells: HashMap<(usize, usize), (Entity, (u8, Piece, bool, bool))>,
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
    let _p = ::sim::prof::scope("game/sync dam ghosts");
    let Some(mut models) = models else { return };
    let sim = &sim.0;
    let mut want: HashMap<(usize, usize), (u8, Piece, bool, bool)> = HashMap::new();
    if let Some(plan) = hud.as_ref().filter(|h| h.tool == crate::hud::Tool::Dam).and_then(|h| h.plan.as_ref().filter(|p| p.line == h.line)) {
        let bad = plan.refusal.is_some();
        let line: Vec<(usize, usize)> = plan.targets.iter().map(|t| t.0).collect();
        let pieces = wall_pieces(&line);
        for &(c, t) in &plan.targets {
            let Some(&piece) = pieces.get(&c) else { continue };
            let steps = (t.max(DAM_STEP) / DAM_STEP).round().clamp(1.0, 16.0) as u8;
            want.insert(c, (steps, piece, faces_downstream(sim, c, &piece, None), bad));
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
    for (&c, &(steps, piece, flip, bad)) in &want {
        if st.cells.contains_key(&c) {
            continue;
        }
        let len = piece.len;
        let mesh = models.dams.entry((steps, len, 1, DamStage::Done)).or_insert_with(|| meshes.add(super::structures::dam_cell_mesh(steps, len as f32 / 10.0, 1, DamStage::Done))).clone();
        // A ghost in a deep pond would hide under the water: keep its top showing.
        let surface = sim.height.get(c.0, c.1).copied().unwrap_or(0.0) + sim.water.depth.get(c.0, c.1).copied().unwrap_or(0.0);
        let tr = piece_transform(sim, &piece, flip, |ground| ground.max(surface + 0.15 - steps as f32 * DAM_STEP));
        let e = commands.spawn((SimEntity, Mesh3d(mesh), MeshMaterial3d(if bad { no.clone() } else { ok.clone() }), tr)).id();
        st.cells.insert(c, (e, (steps, piece, flip, bad)));
    }
}

#[cfg(test)]
mod wall_tests {
    use super::*;

    #[test]
    fn a_shallow_staircase_of_cells_is_drawn_as_one_straight_wall() {
        // 18 cells along, 6 down: runs of three with a step between them.
        let cells: Vec<(usize, usize)> = (0..19).map(|x| (10 + x, 20 + x / 3)).collect();
        let pieces = wall_pieces(&cells);
        let yaws: Vec<f32> = cells[3..16].iter().map(|c| pieces[c].yaw()).collect();
        let want = (-1.0f32 / 3.0).atan();
        for y in &yaws {
            assert!((y - want).abs() < 0.12, "piece turned {y}, line runs at {want}");
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum GroundKey {
    Pile(u64),
    Supplies,
}

#[derive(Default)]
pub struct GroundSync {
    generation: Option<(u64, u32)>,
    drawn: HashMap<GroundKey, (Entity, ((usize, usize), (u8, u8)))>,
    meshes: HashMap<Option<sim::buildings::Good>, Handle<Mesh>>,
}

/// Goods lying on the ground and the family's starting supplies, each a heap sized by how
/// much is left; an emptied heap is gone.
#[allow(clippy::too_many_arguments)]
pub(super) fn sync_ground_goods(mut commands: Commands, sim: Res<Sim>, flow: Res<crate::flow::Flow>, models: Option<Res<Models>>,
    mut meshes: ResMut<Assets<Mesh>>, mut materials: ResMut<Assets<StandardMaterial>>, mut puff_mesh: ResMut<super::effects::PuffMesh>,
    beavers: Query<(Entity, &Transform, &BeaverSprite), With<SimBeaver>>, clock: Res<super::RenderClock>, mut st: Local<GroundSync>) {
    let _p = ::sim::prof::scope("game/sync ground goods");
    let Some(models) = models else { return };
    let s = &sim.0;
    let generation = (s.seed, flow.generation);
    // Goods already lying about when a game is loaded just appear; only new drops are shown.
    let loading = st.generation != Some(generation);
    if loading {
        for (_, (e, _)) in st.drawn.drain() {
            commands.entity(e).despawn();
        }
        st.generation = Some(generation);
    }
    let load = sim::agents::LOAD_SIZE;
    let mut want: Vec<(GroundKey, (usize, usize), Option<sim::buildings::Good>, f32)> = s
        .work
        .piles
        .items
        .iter()
        .map(|p| {
            // Rotting goods slump as they go.
            let rot = (1.0 - p.days_left(s.time_days) / sim::piles::SPOIL_DAYS) as f32;
            (GroundKey::Pile(p.id), p.cell, Some(p.good), (p.amount / load).cbrt().clamp(0.5, 1.8) * (1.0 - 0.25 * rot.clamp(0.0, 1.0)))
        })
        .collect();
    // How far each pile has rotted, by band: fresh, going off, rotten.
    let rot_band = |key: GroundKey| -> u8 {
        let GroundKey::Pile(id) = key else { return 0 };
        let Some(p) = s.work.piles.get(id) else { return 0 };
        let k = 1.0 - p.days_left(s.time_days) / sim::piles::SPOIL_DAYS;
        if k < 0.45 { 0 } else if k < 0.8 { 1 } else { 2 }
    };
    if let Some(c) = s.work.supplies {
        let start: f32 = sim::starts::START_BUNDLE.iter().map(|g| g.1).sum();
        let left: f32 = sim::buildings::Good::ALL.iter().map(|&g| s.work.stock_of(sim::agents::SUPPLIES, g)).sum();
        if left > 0.05 {
            want.push((GroundKey::Supplies, c, None, (left / start.max(1.0)).sqrt().clamp(0.45, 1.0)));
        }
    }
    let mut live = HashSet::new();
    for (key, cell, good, scale) in want {
        live.insert(key);
        let band = rot_band(key);
        let sig = (cell, ((scale * 20.0).round() as u8, band));
        if st.drawn.get(&key).is_some_and(|d| d.1 == sig) {
            continue;
        }
        let mat = if band == 0 { models.white.clone() } else { models.rot[(band - 1) as usize].clone() };
        let p = cell_xz(s, (cell.0 as f32 + 0.5, cell.1 as f32 + 0.5));
        // A load set down here (a new heap, or one grown): the beaver beside it bows to put it
        // down, the bundle drops from its mouth with a puff, and the heap grows in after it.
        let grew = st.drawn.get(&key).is_none_or(|d| (d.1).1.0 < sig.1.0);
        let dropped = !loading && grew && matches!(key, GroundKey::Pile(_));
        let ground = Vec3::new(p.x, height_at(&s.height, p.x, p.y), p.y);
        if dropped {
            let by = s.agents.by_id.values().filter(|a| a.cell.0.abs_diff(cell.0) <= 1 && a.cell.1.abs_diff(cell.1) <= 1).map(|a| a.id.0).collect::<Vec<_>>();
            if let Some((e, btr, _)) = beavers.iter().find(|(_, _, b)| by.contains(&b.id)) {
                commands.entity(e).insert(super::SetDown { until: clock.0 + 1.0 });
                if let Some(g) = good {
                    let from = btr.translation + Vec3::Y * 0.8 * btr.scale.y + btr.rotation * Vec3::Z * 0.6 * btr.scale.z;
                    commands.spawn((
                        SimEntity,
                        Mesh3d(models.loads[g.index()].clone()),
                        MeshMaterial3d(models.white.clone()),
                        Transform::from_translation(from).with_scale(btr.scale * super::LOAD_LOOK),
                        super::effects::Fall { from, to: ground + Vec3::Y * 0.05, since: clock.0 + 0.3, secs: 0.35 },
                    ));
                }
            }
            super::effects::puff(&mut commands, &mut meshes, &mut materials, &mut puff_mesh, ground, 0.7);
        }
        let yaw = hash01(cell.0, cell.1, 23) * std::f32::consts::TAU;
        let tr = Transform::from_xyz(p.x, height_at(&s.height, p.x, p.y), p.y).with_rotation(Quat::from_rotation_y(yaw)).with_scale(Vec3::splat(scale));
        let mesh = st.meshes.entry(good).or_insert_with(|| meshes.add(match good { Some(g) => models::heap_mesh(g), None => models::supplies_mesh() })).clone();
        match st.drawn.get(&key) {
            Some(&(e, (_, (old_scale, _)))) => {
                // Shrinks smoothly to its new size.
                let from = (old_scale as f32 / 20.0 / scale.max(0.01)).clamp(0.2, 5.0);
                commands.entity(e).insert((tr, MeshMaterial3d(mat), super::effects::Grow { base: tr.scale, from: Vec3::splat(from), since: clock.0, secs: 0.8 }));
                st.drawn.insert(key, (e, sig));
            }
            None => {
                let e = commands.spawn((SimEntity, Mesh3d(mesh), MeshMaterial3d(mat), tr)).id();
                if dropped {
                    // Grows in once the bundle has landed.
                    commands.entity(e).insert(super::effects::Grow { base: tr.scale, from: Vec3::splat(0.05), since: clock.0 + 0.6, secs: 0.5 });
                }
                st.drawn.insert(key, (e, sig));
            }
        }
    }
    let gone: Vec<GroundKey> = st.drawn.keys().copied().filter(|k| !live.contains(k)).collect();
    for k in gone {
        if let Some((e, _)) = st.drawn.remove(&k) {
            commands.entity(e).despawn();
        }
    }
}
