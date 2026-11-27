use super::meshgen::{build_mesh, mix, v, Mb, Rng};
use super::models::{col, stick, stick_c};
use super::sim_sync::{cell_mid, SyncedMap};
use super::sim_view::cell_xz;
use super::{Models, RenderClock};
use crate::palette;
use crate::seasonal::smooth;
use crate::state::Sim;
use bevy::prelude::*;
use std::collections::HashMap;

pub fn raft_mesh(mb: &mut Mb) {
    let (mut r, len, logs) = (Rng(9191), 1.25f32, 6);
    for i in 0..logs {
        let (x, j) = ((i as f32 - (logs as f32 - 1.0) / 2.0) * 0.21, r.range(-0.06, 0.06));
        mb.log(v(x, 0.0, -len + j), v(x, 0.0, len * 0.85 + j), 0.11, mix(col(palette::BARK), col(palette::BARK_LIGHT), r.next()), col(palette::CUT_WOOD), 7);
    }
    for z in [-0.9f32, 0.0, 0.85] { stick(mb, v(-0.68, 0.1, z), v(0.68, 0.1, z), 0.07, r.next()); }
    for x in [-0.6f32, 0.6] { stick_c(mb, v(x, 0.08, len * 0.85), v(0.0, 0.32, len + 0.2), 0.08, col(palette::PEELED)); }
    for x in [-0.66f32, 0.66] {
        for z in [-1.05f32, -0.2, 0.65] { stick_c(mb, v(x, 0.05, z), v(x, 0.32, z), 0.04, col(palette::PEELED)); }
        stick(mb, v(x, 0.3, -1.1), v(x, 0.3, 0.75), 0.04, r.next());
    }
}

fn wall(mb: &mut Mb, at: Vec3, yaw: f32, len: f32, thick: f32, variant: u32) {
    let start = mb.len();
    super::structures::dam_cell(mb, 1.0, len, variant, super::structures::DamStage::Done);
    mb.place(start, Mat4::from_scale_rotation_translation(Vec3::new(1.0, 1.0, thick), Quat::from_rotation_y(yaw), at));
}

pub fn lock_gate_mesh(mb: &mut Mb) { wall(mb, Vec3::ZERO, 0.0, 1.4, 0.35, 7); }

pub fn lock_walls_mesh(mb: &mut Mb) {
    for s in [-1.0f32, 1.0] {
        wall(mb, Vec3::new(s * 0.96, 0.0, 0.0), -s * std::f32::consts::FRAC_PI_2, 3.2, 0.45, if s > 0.0 { 3 } else { 4 });
    }
}

#[derive(Resource)]
pub(super) struct RaftLandingMeshes {
    pub raft: Handle<Mesh>,
    pub gate: Handle<Mesh>,
    pub walls: Handle<Mesh>,
}

pub(super) fn init_raft_landing_meshes(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>) {
    commands.insert_resource(RaftLandingMeshes {
        raft: build_mesh(&mut meshes, raft_mesh),
        gate: build_mesh(&mut meshes, lock_gate_mesh),
        walls: build_mesh(&mut meshes, lock_walls_mesh),
    });
}

#[derive(Component)]
pub(super) struct LockVis {
    pub gates: [Entity; 2],
    pub h: f32,
    pub water: Entity,
    pub low: f32,
    pub high: f32,
    pub floor: f32,
    pub open: [f32; 2],
    pub level: f32,
    pub start_level: f32,
    pub occupant: Option<u64>,
    pub iced: bool,
    pub sheet: bool,
    pub looping: bool,
    pub k: f32,
    pub up: bool,
}

const WIDTH: f32 = 1.6;
const TRIP_S: f32 = 5.0;
const SHUT_S: f32 = 0.8;
const ENTRY_OPEN: (f32, f32) = (0.15, 0.28);
const SWIM_IN: (f32, f32) = (0.26, 0.52);
const ENTRY_SHUT: (f32, f32) = (0.52, 0.6);
const FILL: (f32, f32) = (0.6, 0.85);
const EXIT_OPEN: (f32, f32) = (0.85, 1.0);
const RAFT_BACK: f32 = 1.2;
const JUMP: f32 = 0.06;
const GLIDE_S: f32 = 0.15;
const TURN: f32 = 2.5;

#[allow(clippy::too_many_arguments)]
pub(super) fn spawn_lock(commands: &mut Commands, hm: &RaftLandingMeshes, m: &Models, at: Vec3, axis_yaw: f32, half: f32, floor: f32, low: f32, high: f32, looping: bool) -> Entity {
    let h = (high + 0.3 - floor).max(0.4);
    let root = commands.spawn((Transform::from_xyz(at.x, floor, at.z).with_rotation(Quat::from_rotation_y(axis_yaw)).with_scale(Vec3::new(WIDTH, 1.0, 1.0)), Visibility::Inherited)).id();
    commands.spawn((Mesh3d(hm.walls.clone()), MeshMaterial3d(m.white.clone()), Transform::from_scale(Vec3::new(1.0, h, half / 1.05)), ChildOf(root)));
    let water = commands.spawn((Transform::from_xyz(0.0, low - floor, 0.0), Visibility::Inherited, ChildOf(root), bevy::light::NotShadowCaster)).id();
    let gates = [-half, half].map(|z| commands.spawn((Mesh3d(hm.gate.clone()), MeshMaterial3d(m.white.clone()), Transform::from_xyz(0.0, 0.0, z).with_scale(Vec3::new(1.0, h * 0.98, 1.0)), ChildOf(root))).id());
    commands.entity(root).insert(LockVis { gates, h, water, low, high, floor, open: [0.0; 2], level: low, start_level: low, occupant: None, iced: false, sheet: false, looping, k: 0.0, up: true });
    root
}

fn trip_look(f: f32, from: f32, entry: f32, exit: f32) -> ([f32; 2], f32) {
    let near = smooth(ENTRY_OPEN.0, ENTRY_OPEN.1, f) * (1.0 - smooth(ENTRY_SHUT.0, ENTRY_SHUT.1, f));
    let far = smooth(EXIT_OPEN.0, EXIT_OPEN.1, f);
    let level = if f < ENTRY_OPEN.0 { from + (entry - from) * smooth(0.0, ENTRY_OPEN.0, f) } else { entry + (exit - entry) * smooth(FILL.0, FILL.1, f) };
    ([near, far], level)
}

#[derive(Clone, Copy, Debug)]
pub(super) struct Chamber {
    pub centre: Vec2,
    pub axis: Vec2,
    pub half: f32,
    pub low: f32,
    pub high: f32,
    pub floor: f32,
    pub iced: bool,
}

pub(super) fn chamber(s: &sim::SimState, l: &sim::locks::Lock) -> Option<Chamber> {
    let floor = s.buildings.building(l.building)?.gates().iter().find(|g| g.cell == l.cell)?.floor;
    let f = sim::locks::frame(s, l)?;
    let (flow, centre, half) = (Vec2::new(f.axis.0, f.axis.1), cell_xz(s, f.centre), f.half * crate::terrain_render::CELL_SIZE);
    let (w, h) = (s.height.width() as f32 / 2.0, s.height.height() as f32 / 2.0);
    let beyond = |axis: Vec2, d: f32| {
        let p = centre + axis * d * (half + 0.8);
        ((p.x / crate::terrain_render::CELL_SIZE + w).max(0.0) as usize, (p.y / crate::terrain_render::CELL_SIZE + h).max(0.0) as usize)
    };
    let level = |c: (usize, usize)| {
        let at = |g: &sim::Grid<f32>| g.get(c.0, c.1).copied().unwrap_or(0.0);
        at(&s.height) + at(&s.water.depth).max(at(&s.water.dam_height))
    };
    let (a, b) = (level(beyond(flow, 1.0)), level(beyond(flow, -1.0)));
    let (axis, low, high) = if a >= b { (flow, b, a) } else { (-flow, a, b) };
    let iced = [1.0, 0.8, -0.8, -1.0, 1.2, -1.2].iter().map(|&d| beyond(axis, d)).any(|c| s.water.ice_cover(c.0, c.1) >= 0.5);
    Some(Chamber { centre, axis, half, low, high, floor, iced })
}

pub(super) fn chambers(s: &sim::SimState) -> Vec<Chamber> { sim::locks::built(s).filter_map(|l| chamber(s, &l)).collect() }

pub(super) fn in_chamber(s: &sim::SimState, c: (usize, usize)) -> bool {
    let p = cell_mid(s, c);
    chambers(s).into_iter().any(|ch| {
        let d = p - ch.centre;
        d.dot(ch.axis).abs() <= ch.half + 0.3 && d.dot(ch.axis.perp()).abs() < 0.66 * WIDTH + 0.55
    })
}

pub(crate) fn chamber_holes(s: &sim::SimState) -> Vec<(Vec2, Vec2, f32, f32)> {
    chambers(s).into_iter().map(|ch| (ch.centre, ch.axis, ch.half, 0.66 * WIDTH)).collect()
}

pub(crate) fn chamber_floors(s: &sim::SimState) -> Vec<(Vec2, Vec2, f32, f32, f32)> {
    chambers(s).into_iter().map(|ch| (ch.centre, ch.axis, ch.half, 0.66 * WIDTH, ch.floor)).collect()
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum Stage { Entering { f: f32, up: bool }, Leaving { up: bool } }

impl Stage {
    fn up(self) -> bool { match self { Stage::Entering { up, .. } | Stage::Leaving { up } => up } }
}

fn lock_at(s: &sim::SimState, c: (usize, usize)) -> Option<sim::locks::Lock> {
    (s.nav.surface_at(c) == Some(sim::nav::Surface::Lock)).then(|| sim::locks::built(s).find(|l| sim::locks::lock_cells(s, l).contains(&c)))?
}

pub(super) fn trip(s: &sim::SimState, ag: &sim::agents::Agent, pending_days: f32) -> Option<(sim::locks::Lock, Chamber, Stage)> {
    let steps = ag.path.as_ref().map_or(&[][..], |p| p.steps.as_slice());
    let next = steps.first().map(|x| x.0);
    let dir = |a: (usize, usize), b: (usize, usize)| Vec2::new(b.0 as f32 - a.0 as f32, b.1 as f32 - a.1 as f32);
    if let Some(l) = lock_at(s, ag.cell) {
        let ch = chamber(s, &l)?;
        let ahead = steps.iter().map(|st| st.0).find(|c| lock_at(s, *c).is_none()).or(next);
        return Some((l, ch, Stage::Leaving { up: ahead.is_none_or(|n| dir(ag.cell, n).dot(ch.axis) >= 0.0) }));
    }
    let n = next?;
    let l = lock_at(s, n)?;
    let ch = chamber(s, &l)?;
    Some((l, ch, Stage::Entering { f: super::sim_view::step_progress(s, ag, n, pending_days), up: dir(ag.cell, n).dot(ch.axis) >= 0.0 }))
}

fn on_lane(ch: &Chamber, p: Vec2, raft_back: bool) -> Option<Vec2> {
    let (u, n, d) = (ch.axis, ch.axis.perp(), p - ch.centre);
    let (a, x) = (d.dot(u), d.dot(n));
    let (end, wide) = (ch.half * sim::locks::WALL_REACH, sim::locks::HALF_WIDTH + 0.6);
    let clear = end + 1.5;
    if a.abs() >= clear || x.abs() >= wide { return None; }
    let x2 = x * (1.0 - (1.0 - smooth(end - 0.5, clear, a.abs())) * (1.0 - smooth(0.8, wide, x.abs())));
    let back = if raft_back && a.abs() > ch.half { RAFT_BACK * (1.0 - smooth(ch.half + 0.5, clear, a.abs())) } else { 0.0 };
    Some(ch.centre + u * (a + a.signum() * back) + n * x2)
}

pub(super) fn lock_pose(s: &sim::SimState, ag: &sim::agents::Agent, pending_days: f32, base: Vec3) -> Option<(Vec3, Option<f32>)> {
    sim::locks::built(s).next()?;
    let raft = super::sim_view::is_pushing_raft(ag);
    if let Some((l, ch, stage)) = trip(s, ag, pending_days) {
        let up = stage.up();
        let ((entry, exit), u) = (if up { (ch.low, ch.high) } else { (ch.high, ch.low) }, if up { ch.axis } else { -ch.axis });
        let (xz, y) = match stage {
            Stage::Entering { f, .. } => {
                let wait = on_lane(&ch, cell_mid(s, ag.cell), raft).unwrap_or(ch.centre - u * (ch.half + 0.5));
                let first = ag.path.as_ref().and_then(|p| p.steps.first()).map_or(l.cell, |st| st.0);
                let inside = on_lane(&ch, cell_mid(s, first), false).unwrap_or(ch.centre - u * (ch.half - 0.5));
                let here = on_lane(&ch, base.xz(), raft).unwrap_or(base.xz());
                let come = if (here - ch.centre).dot(u) > (wait - ch.centre).dot(u) { wait } else { here };
                let xz = if f < SWIM_IN.0 { come } else { come.lerp(inside, smooth(SWIM_IN.0, SWIM_IN.1, f)) };
                (xz, entry + (exit - entry) * smooth(FILL.0, FILL.1, f))
            }
            Stage::Leaving { .. } => {
                let here = on_lane(&ch, base.xz(), false).unwrap_or(base.xz());
                let a = (here - ch.centre).dot(u).max(-(ch.half - 0.5));
                (ch.centre + u * a + ch.axis.perp() * (here - ch.centre).dot(ch.axis.perp()), exit)
            }
        };
        return Some((Vec3::new(xz.x, y, xz.y), Some(u.x.atan2(u.y))));
    }
    if ag.gait != sim::nav::Gait::Swim && !raft { return None; }
    let steps = ag.path.as_ref().map_or(&[][..], |p| p.steps.as_slice());
    sim::locks::built(s).find_map(|l| {
        let ch = chamber(s, &l)?;
        let coming = raft && { let cells = sim::locks::lock_cells(s, &l); steps.iter().take(8).any(|st| cells.contains(&st.0)) };
        on_lane(&ch, base.xz(), coming).map(|xz| (Vec3::new(xz.x, base.y, xz.y), None))
    })
}

#[derive(Component, Default)]
pub(super) struct LockGlide { shown: Option<Vec3>, last: Option<Vec3>, vel: Vec3, yaw: Option<f32>, near: bool }

pub(super) fn turn(g: &mut LockGlide, want: f32, dt: f32, smoothing: f32) -> f32 {
    let yaw = match g.yaw.filter(|_| g.near && smoothing > 0.0) {
        Some(y) => {
            let d = (want - y + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI;
            let max = TURN / smoothing * dt;
            y + d.clamp(-max, max)
        }
        None => want,
    };
    g.yaw = Some(yaw);
    yaw
}

pub(super) fn glide(g: &mut LockGlide, chs: &[Chamber], target: Vec3, dt: f32, smoothing: f32) -> Vec3 {
    let (Some(shown), Some(last)) = (g.shown, g.last) else {
        *g = LockGlide { shown: Some(target), last: Some(target), yaw: g.yaw, ..default() };
        return target;
    };
    if smoothing <= 0.0 {
        *g = LockGlide { shown: Some(target), last: Some(target), yaw: g.yaw, ..default() };
        return target;
    }
    g.last = Some(target);
    g.near = chs.iter().any(|ch| {
        let d = target.xz() - ch.centre;
        d.dot(ch.axis).abs() < ch.half * sim::locks::WALL_REACH + 4.0 && d.dot(ch.axis.perp()).abs() < 5.0
    });
    let step = target - last;
    if step.length() > 12.0 || (shown.distance(target) < 0.01 && !g.near) {
        g.shown = Some(target);
        g.vel = step;
        return target;
    }
    let jump = (step - g.vel).xz().length() > JUMP;
    let from = if jump { shown + g.vel } else { shown + step };
    g.vel = if jump { g.vel * 0.5 } else { step };
    let next = from + (target - from) * crate::sim_driver::ease_k(dt, 1.0 / GLIDE_S, smoothing);
    g.shown = Some(next);
    next
}

pub(super) fn animate_locks(time: Res<Time>, mut locks: Query<&mut LockVis>, mut parts: Query<&mut Transform, Without<LockVis>>) {
    let dt = time.delta_secs();
    for mut l in &mut locks {
        if !l.looping { continue; }
        l.k += dt / TRIP_S;
        if l.k >= 1.0 { l.k -= 1.0; l.up = !l.up; }
        let (entry, exit) = if l.up { (l.low, l.high) } else { (l.high, l.low) };
        let (gates, level) = trip_look(l.k, entry, entry, exit);
        l.open = if l.up { gates } else { [gates[1], gates[0]] };
        l.level = level;
        show(&l, &mut parts);
    }
}

fn show(l: &LockVis, parts: &mut Query<&mut Transform, Without<LockVis>>) {
    for (&e, &open) in l.gates.iter().zip(&l.open) {
        if let Ok(mut t) = parts.get_mut(e) { t.translation.y = -open * 1.05 * l.h; }
    }
    if let Ok(mut t) = parts.get_mut(l.water) { t.translation.y = l.level.max(l.floor + 0.02) + 0.01 - l.floor; }
}

#[derive(Default)]
pub(super) struct LockSync { drawn: SyncedMap<(usize, usize), Entity> }

#[allow(clippy::too_many_arguments)]
pub(super) fn sync_locks(
    mut commands: Commands, sim: Res<Sim>, clock: Option<Res<crate::sim_driver::SimClock>>,
    time: Res<Time>, models: Option<Res<Models>>, hm: Option<Res<RaftLandingMeshes>>, mut st: Local<LockSync>,
    mut q: Query<&mut LockVis>, mut parts: Query<&mut Transform, Without<LockVis>>,
    mut meshes: ResMut<Assets<Mesh>>, water: Option<Res<crate::water_render::SharedWater>>,
) {
    let (Some(m), Some(hm)) = (models, hm) else { return };
    let s = &sim.0;
    st.drawn.reset_if(s.instance, &mut commands, |c, e| c.entity(e).despawn());
    let built: Vec<_> = sim::locks::built(s).collect();
    st.drawn.retain_keys(|cell| built.iter().any(|l| l.cell == *cell), &mut commands, |c, e| c.entity(e).despawn());
    let pending = clock.as_ref().map_or(0.0, |c| (c.render_days(s.time_days) - s.time_days) as f32);
    let mut inside: HashMap<(usize, usize), (u64, Stage)> = HashMap::new();
    for ag in s.agents.by_id.values() {
        if let Some((l, _, stage)) = trip(s, ag, pending) {
            let e = inside.entry(l.cell).or_insert((ag.id.0, stage));
            if matches!(stage, Stage::Leaving { .. }) && matches!(e.1, Stage::Entering { .. }) { *e = (ag.id.0, stage); }
        }
    }
    let dt = time.delta_secs();
    for l in built {
        let Some(ch) = chamber(s, &l) else { continue };
        let Some(&e) = st.drawn.map.get(&l.cell) else {
            let e = spawn_lock(&mut commands, &hm, &m, Vec3::new(ch.centre.x, 0.0, ch.centre.y), ch.axis.x.atan2(ch.axis.y), ch.half, ch.floor, ch.low, ch.high, false);
            st.drawn.map.insert(l.cell, e);
            continue;
        };
        let Ok(mut v) = q.get_mut(e) else { continue };
        (v.low, v.high) = (ch.low, ch.high);
        if v.iced != ch.iced || !v.sheet {
            v.iced = ch.iced;
            v.sheet = true;
            let (hx, hz) = (0.62, ch.half);
            let mesh = meshes.add(crate::water_render::water_sheet([Vec3::new(-hx, 0.0, -hz), Vec3::new(-hx, 0.0, hz), Vec3::new(hx, 0.0, hz), Vec3::new(hx, 0.0, -hz)], (ch.high - ch.floor).max(0.5), if ch.iced { 1.0 } else { 0.0 }));
            if let Some(w) = &water { commands.entity(v.water).insert((Mesh3d(mesh), MeshMaterial3d(w.0.clone()))); }
        }
        match inside.get(&l.cell).copied() {
            Some((who, stage)) => {
                if v.occupant != Some(who) { v.occupant = Some(who); v.start_level = v.level; }
                let up = stage.up();
                let (entry, exit) = if up { (ch.low, ch.high) } else { (ch.high, ch.low) };
                let (gates, level) = match stage {
                    Stage::Entering { f, .. } => trip_look(f, v.start_level, entry, exit),
                    Stage::Leaving { .. } => ([0.0, 1.0], exit),
                };
                v.open = if up { gates } else { [gates[1], gates[0]] };
                v.level = level;
            }
            None => {
                v.occupant = None;
                for o in &mut v.open { *o = (*o - dt / SHUT_S).max(0.0); }
            }
        }
        show(&v, &mut parts);
    }
}

#[derive(Default)]
pub(super) struct RaftSync { drawn: SyncedMap<u32, (Entity, Entity, Option<sim::buildings::Good>)> }

#[allow(clippy::too_many_arguments)]
pub(super) fn sync_rafts(
    mut commands: Commands, sim: Res<Sim>, models: Option<Res<Models>>,
    hm: Option<Res<RaftLandingMeshes>>, clock: Res<RenderClock>,
    mut st: Local<RaftSync>, beavers: Query<(&super::BeaverSprite, &Transform), Without<RaftMark>>,
    mut tr: Query<(&mut Transform, &mut Visibility), With<RaftMark>>,
) {
    let (Some(m), Some(hm)) = (models, hm) else { return };
    let despawn_raft = |c: &mut Commands, (e, h, _): (Entity, Entity, Option<sim::buildings::Good>)| { c.entity(e).despawn(); c.entity(h).despawn(); };
    let s = &sim.0;
    st.drawn.reset_if(s.instance, &mut commands, despawn_raft);
    st.drawn.retain_keys(|id| s.fleet.get(*id).is_some(), &mut commands, despawn_raft);
    for b in &s.fleet.rafts {
        let entry = st.drawn.map.entry(b.id).or_insert_with(|| {
            let e = commands.spawn((Mesh3d(hm.raft.clone()), MeshMaterial3d(m.white.clone()), Transform::default(), Visibility::Inherited, RaftMark)).id();
            let h = commands.spawn((Mesh3d(hm.raft.clone()), MeshMaterial3d(m.white.clone()), Transform::default(), Visibility::Hidden, RaftMark)).id();
            (e, h, None)
        });
        let (e, h) = (entry.0, entry.1);
        let pushed = b.hauler.and_then(|w| s.agents.get(w).map(|a| (w, a))).filter(|(_, a)| matches!(a.task, sim::agents::Task::Raft { leg: 1..=5, .. }));
        let drawn_hauler = pushed.and_then(|(w, _)| beavers.iter().find(|(sp, _)| sp.id == w.0).map(|(sp, t)| (sp.yaw, t.translation)));
        let (pos, yaw) = match drawn_hauler {
            Some((yaw, at)) => (at + Vec3::new(yaw.sin(), 0.0, yaw.cos()) * 0.8 + Vec3::Y * 0.05, yaw),
            None => {
                let xz = cell_mid(s, b.cell);
                let surface = super::sim_view::stand_y(s, b.cell);
                (Vec3::new(xz.x, surface + 0.02, xz.y), (b.id as f32 * 1.3).rem_euclid(std::f32::consts::TAU))
            }
        };
        let bob = 0.03 * (clock.0 * 1.3 + b.id as f32 * 2.1).sin();
        if let Ok((mut t, _)) = tr.get_mut(e) {
            *t = Transform::from_translation(pos + Vec3::Y * bob).with_rotation(Quat::from_rotation_y(yaw) * Quat::from_rotation_z(0.03 * (clock.0 * 1.1 + b.id as f32).sin())).with_scale(Vec3::splat(0.55));
        }
        let good = pushed.and_then(|(_, a)| a.load.map(|l| l.good));
        if entry.2 != good {
            entry.2 = good;
            if let Some(g) = good {
                commands.entity(h).insert(Mesh3d(m.heaps[g.index()].clone()));
            }
        }
        if let Ok((mut t, mut v)) = tr.get_mut(h) {
            *v = if good.is_some() { Visibility::Inherited } else { Visibility::Hidden };
            *t = Transform::from_translation(pos + Vec3::Y * (bob + 0.1)).with_rotation(Quat::from_rotation_y(yaw)).with_scale(Vec3::splat(0.75));
        }
    }
}

#[derive(Component)]
pub(super) struct RaftMark;

pub(super) fn spawn_gallery(commands: &mut Commands, hm: &RaftLandingMeshes, m: &Models, centre: Vec3, ground: impl Fn(f32, f32) -> f32) {
    let goods: Vec<_> = sim::buildings::Good::all().collect();
    for (i, &g) in goods.iter().enumerate() {
        let (x, z) = (centre.x + (i as f32 - (goods.len() as f32 - 1.0) / 2.0) * 1.8, centre.z + 2.0);
        let raft = commands.spawn((Mesh3d(hm.raft.clone()), MeshMaterial3d(m.white.clone()), Transform::from_xyz(x, ground(x, z) + 0.12, z).with_scale(Vec3::splat(0.55)))).id();
        commands.spawn((Mesh3d(m.heaps[g.index()].clone()), MeshMaterial3d(m.white.clone()), Transform::from_xyz(0.0, 0.18, -0.1).with_scale(Vec3::splat(1.3)), ChildOf(raft)));
    }
    let (x, z) = (centre.x, centre.z - 2.5);
    let g = ground(x, z);
    spawn_lock(commands, hm, m, Vec3::new(x, g, z), std::f32::consts::FRAC_PI_2, 2.2, g - 0.2, g + 0.25, g + 1.05, true);
}
