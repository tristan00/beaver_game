//! Raft landings, rafts and locks: their models, the rafts moored at a raft landing, and a lock working
//! (gates sinking to open and rising to shut, the chamber filling or draining) while a beaver goes through it.
//! `BC_GALLERY_WHAT=raft landing` lines up a raft landing's stages, a raft with each load and a lock on a
//! loop (see `gallery.rs`).
use super::meshgen::{mix, Mb, Rng};
use super::models::{col, stick, stick_c};

use super::sim_view::cell_xz;
use super::{Models, RenderClock};
use crate::palette;
use crate::state::Sim;
use bevy::prelude::*;
use std::collections::HashMap;

fn v(x: f32, y: f32, z: f32) -> Vec3 {
    Vec3::new(x, y, z)
}

/// A raft: logs lashed side by side, a low rail along each side and a raised prow,
/// about two and a half cells long. Its cargo is drawn on top separately.
pub fn raft_mesh(mb: &mut Mb) {
    let mut r = Rng(9191);
    let (len, logs) = (1.25f32, 6);
    for i in 0..logs {
        let x = (i as f32 - (logs as f32 - 1.0) / 2.0) * 0.21;
        let j = r.range(-0.06, 0.06);
        mb.log(v(x, 0.0, -len + j), v(x, 0.0, len * 0.85 + j), 0.11, mix(col(palette::BARK), col(palette::BARK_LIGHT), r.next()), col(palette::CUT_WOOD), 7);
    }
    // Cross bars lashing it, and the prow lifting at the front.
    for z in [-0.9f32, 0.0, 0.85] {
        stick(mb, v(-0.68, 0.1, z), v(0.68, 0.1, z), 0.07, r.next());
    }
    stick_c(mb, v(-0.6, 0.08, len * 0.85), v(0.0, 0.32, len + 0.2), 0.08, col(palette::PEELED));
    stick_c(mb, v(0.6, 0.08, len * 0.85), v(0.0, 0.32, len + 0.2), 0.08, col(palette::PEELED));
    // Low rails.
    for x in [-0.66f32, 0.66] {
        for z in [-1.05f32, -0.2, 0.65] {
            stick_c(mb, v(x, 0.05, z), v(x, 0.32, z), 0.04, col(palette::PEELED));
        }
        stick(mb, v(x, 0.3, -1.1), v(x, 0.3, 0.75), 0.04, r.next());
    }
}

/// A section of dam wall built in its own frame (running along x, its smooth mud face +z),
/// thinned to `thick` of a dam's width, turned by `yaw` and set at `at`.
fn wall(mb: &mut Mb, at: Vec3, yaw: f32, len: f32, thick: f32, variant: u32) {
    let start = mb.len();
    super::structures::dam_cell(mb, 1.0, len, variant, super::structures::DamStage::Done);
    mb.place(start, Mat4::from_scale_rotation_translation(Vec3::new(1.0, 1.0, thick), Quat::from_rotation_y(yaw), at));
}

/// A lock's gate: a short section of dam across the chamber, the beavers' own way of holding
/// water back. Sunk into the bed it lets a swimmer over; raised it shuts. One unit tall
/// (scaled to the chamber's height).
pub fn lock_gate_mesh(mb: &mut Mb) {
    wall(mb, Vec3::ZERO, 0.0, 1.4, 0.35, 7);
}

/// A lock's chamber walls either side of the channel along the flow (z): stick-and-mud dam
/// faces like the dam they stand in, smooth wet mud on the inside for the water's level to
/// show against, sticks laid up the outside. One unit tall.
pub fn lock_walls_mesh(mb: &mut Mb) {
    for s in [-1.0f32, 1.0] {
        // The mud face turned inward, toward the channel.
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
    let mut build = |f: &dyn Fn(&mut Mb)| {
        let mut mb = Mb::new();
        f(&mut mb);
        meshes.add(mb.build())
    };
    commands.insert_resource(RaftLandingMeshes {
        raft: build(&raft_mesh),
        gate: build(&lock_gate_mesh),
        walls: build(&lock_walls_mesh),
    });
}

/// A drawn lock: its parts, its water's levels either side, and what its gates and chamber
/// are doing now.
#[derive(Component)]
pub(super) struct LockVis {
    /// The gates at the low end and the high end.
    pub gates: [Entity; 2],
    /// The chamber's height, the gates' and walls' scale.
    pub h: f32,
    pub water: Entity,
    /// Elevations of the water on the low and high side, and of the chamber's floor.
    pub low: f32,
    pub high: f32,
    pub floor: f32,
    /// How far open the gates at the low end and the high end are, 0 shut to 1 open.
    pub open: [f32; 2],
    /// The chamber's water level now, and where it stood when the trip under way began.
    pub level: f32,
    pub start_level: f32,
    /// Who is in it now.
    pub occupant: Option<u64>,
    /// Frozen over, as the water either side is.
    pub iced: bool,
    /// Whether its water's sheet has been made.
    pub sheet: bool,
    /// Shown on a loop (the gallery) rather than driven by beavers: how far through, and which way.
    pub looping: bool,
    pub k: f32,
    pub up: bool,
}

/// How much wider than its meshes a lock is drawn: its walls stand `0.66 * WIDTH` either side
/// of the middle.
const WIDTH: f32 = 1.6;

/// Seconds a trip takes on the gallery's loop.
const TRIP_S: f32 = 5.0;
/// Seconds a gate takes to drop shut behind a beaver that has left.
const SHUT_S: f32 = 0.8;

/// Spawn a lock's parts at `at` (the chamber's middle), the chamber running along `axis_yaw`
/// with the low side at -z, `half` long either way from the middle, its floor at `floor`.
#[allow(clippy::too_many_arguments)]
pub(super) fn spawn_lock(commands: &mut Commands, hm: &RaftLandingMeshes, m: &Models, at: Vec3, axis_yaw: f32, half: f32, floor: f32, low: f32, high: f32, looping: bool) -> Entity {
    // The walls stand a little above the high side's water: high enough to read as a lock,
    // low enough not to hide what is in it.
    let h = (high + 0.3 - floor).max(0.4);
    let w = &m.white;
    // Wide enough to stand clear of the dam either side, with room for a raft.
    let root = commands
        .spawn((Transform::from_translation(Vec3::new(at.x, floor, at.z)).with_rotation(Quat::from_rotation_y(axis_yaw)).with_scale(Vec3::new(WIDTH, 1.0, 1.0)), Visibility::Inherited, super::SimEntity))
        .id();
    // The walls' gate posts stand at ±1.05 in their mesh.
    commands.spawn((Mesh3d(hm.walls.clone()), MeshMaterial3d(w.clone()), Transform::from_scale(Vec3::new(1.0, h, half / 1.05)), ChildOf(root)));
    let water = commands.spawn((Transform::from_xyz(0.0, low, 0.0), Visibility::Inherited, super::SimEntity, bevy::light::NotShadowCaster)).id();
    // A gate at each end, between the posts.
    let gates = [-half, half].map(|z| commands.spawn((Mesh3d(hm.gate.clone()), MeshMaterial3d(w.clone()), Transform::from_xyz(0.0, 0.0, z).with_scale(Vec3::new(1.0, h * 0.98, 1.0)), ChildOf(root))).id());
    commands.entity(root).insert(LockVis { gates, h, water, low, high, floor, open: [0.0; 2], level: low, start_level: low, occupant: None, iced: false, sheet: false, looping, k: 0.0, up: true });
    root
}

fn smooth(a: f32, b: f32, x: f32) -> f32 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

// The steps of a trip, as shares of the wait the simulation gives it (`nav::lock_wait`):
// the chamber levels to the near side while the swimmer waits at the shut gate; the gate
// opens; it swims in; the gate shuts behind it; the chamber fills or drains to the far side's
// level, lifting or lowering it; the far gate opens, and it swims out.
const ENTRY_OPEN: (f32, f32) = (0.15, 0.28);
const SWIM_IN: (f32, f32) = (0.26, 0.52);
const ENTRY_SHUT: (f32, f32) = (0.52, 0.6);
const FILL: (f32, f32) = (0.6, 0.85);
const EXIT_OPEN: (f32, f32) = (0.85, 1.0);

/// The gates (near end, far end) and the chamber's level `f` of the way through a trip:
/// from `from` (where it stood) to `entry`, then to `exit`.
fn trip_look(f: f32, from: f32, entry: f32, exit: f32) -> ([f32; 2], f32) {
    let near = smooth(ENTRY_OPEN.0, ENTRY_OPEN.1, f) * (1.0 - smooth(ENTRY_SHUT.0, ENTRY_SHUT.1, f));
    let far = smooth(EXIT_OPEN.0, EXIT_OPEN.1, f);
    let level = if f < ENTRY_OPEN.0 { from + (entry - from) * smooth(0.0, ENTRY_OPEN.0, f) } else { entry + (exit - entry) * smooth(FILL.0, FILL.1, f) };
    ([near, far], level)
}

/// A lock's chamber: its middle, the way it runs (towards the high side), half its length,
/// the water levels either side and its floor.
#[derive(Clone, Copy, Debug)]
pub(super) struct Chamber {
    pub centre: Vec2,
    pub axis: Vec2,
    pub half: f32,
    pub low: f32,
    pub high: f32,
    pub floor: f32,
    /// The water either side is frozen over.
    pub iced: bool,
}

/// The chamber of a built lock: right through the dam (every row of a widened one), and at
/// least long enough for a raft with its beaver.
pub(super) fn chamber(s: &sim::SimState, l: &sim::locks::Lock) -> Option<Chamber> {
    let (yaw, _, _, floor, _) = lock_frame(s, l)?;
    let axis = Vec2::new(yaw.sin(), yaw.cos());
    let f = sim::locks::frame(s, l)?;
    let centre = cell_xz(s, f.centre);
    let half = f.half * crate::terrain_render::CELL_SIZE;
    // The water just beyond each end.
    let (w, h) = (s.height.width() as f32 / 2.0, s.height.height() as f32 / 2.0);
    let beyond = |d: f32| {
        let p = centre + axis * d * (half + 0.8);
        ((p.x / crate::terrain_render::CELL_SIZE + w).max(0.0) as usize, (p.y / crate::terrain_render::CELL_SIZE + h).max(0.0) as usize)
    };
    let level = |c: (usize, usize)| {
        let at = |g: &sim::Grid<f32>| g.get(c.0, c.1).copied().unwrap_or(0.0);
        at(&s.height) + at(&s.water.dam_height) + at(&s.water.depth)
    };
    let (a, b) = (level(beyond(1.0)), level(beyond(-1.0)));
    // Frozen over when the water off either end is.
    let iced = [1.0, 0.8, -0.8, -1.0, 1.2, -1.2].iter().map(|&d| beyond(d)).any(|c| s.water.ice_cover(c.0, c.1) >= 0.5);
    Some(Chamber { centre, axis, half, low: a.min(b), high: a.max(b), floor, iced })
}

/// Whether a cell's middle lies inside a built lock's chamber, where the dam is not drawn.
pub(super) fn in_chamber(s: &sim::SimState, c: (usize, usize)) -> bool {
    let p = cell_xz(s, (c.0 as f32 + 0.5, c.1 as f32 + 0.5));
    s.locks.at.values().filter(|l| l.built).filter_map(|l| chamber(s, l)).any(|ch| {
        let d = p - ch.centre;
        d.dot(ch.axis).abs() <= ch.half + 0.3 && d.dot(ch.axis.perp()).abs() < 0.66 * WIDTH + 0.55
    })
}

/// Each built lock's chamber as the water surface leaves it out: middle, way along, half its
/// length and half its width inside the walls. Only the chamber's own water shows in it.
pub(crate) fn chamber_holes(s: &sim::SimState) -> Vec<(Vec2, Vec2, f32, f32)> {
    s.locks.at.values().filter(|l| l.built).filter_map(|l| chamber(s, l)).map(|ch| (ch.centre, ch.axis, ch.half, 0.66 * WIDTH)).collect()
}

/// Where a beaver is in a lock's working: coming in (how far through its wait), or swimming
/// out down the lane, and whether it goes up, from the low side to the high.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum Stage {
    Entering { f: f32, up: bool },
    Leaving { up: bool },
}

impl Stage {
    fn up(self) -> bool {
        match self {
            Stage::Entering { up, .. } | Stage::Leaving { up, .. } => up,
        }
    }
}

/// The built lock whose chamber this cell is in.
fn lock_at(s: &sim::SimState, c: (usize, usize)) -> Option<&sim::locks::Lock> {
    if s.nav.surface_at(c) != Some(sim::nav::Surface::Lock) {
        return None;
    }
    s.locks.at.values().find(|l| l.built && sim::locks::lock_cells(s, l).contains(&c))
}

/// How far through its step to the next cell a beaver is, `pending_days` after the simulation
/// last moved it.
fn step_progress(s: &sim::SimState, ag: &sim::agents::Agent, next: (usize, usize), pending_days: f32) -> f32 {
    let pace = sim::agents::travel_pace(&s.nav, ag);
    let cost = s.nav.move_cost(ag.cell, next, pace).unwrap_or(1.0).max(1e-3);
    ((ag.step_frac + pending_days * sim::agents::MOVE_COST_PER_DAY * pace) / cost).clamp(0.0, 1.0)
}

/// The lock this beaver is going through and how far it has got, `pending_days` after the simulation
/// last moved it (negative while the picture plays catching up).
pub(super) fn trip<'a>(s: &'a sim::SimState, ag: &sim::agents::Agent, pending_days: f32) -> Option<(&'a sim::locks::Lock, Chamber, Stage)> {
    let steps = ag.path.as_ref().map(|p| p.steps.as_slice()).unwrap_or(&[]);
    let next = steps.first().map(|x| x.0);
    let dir = |a: (usize, usize), b: (usize, usize)| Vec2::new(b.0 as f32 - a.0 as f32, b.1 as f32 - a.1 as f32);
    if let Some(l) = lock_at(s, ag.cell) {
        let ch = chamber(s, l)?;
        // Which way it is going: by the lane's end it is making for.
        let ahead = steps.iter().map(|st| st.0).find(|c| lock_at(s, *c).is_none()).or(next);
        let up = ahead.is_none_or(|n| dir(ag.cell, n).dot(ch.axis) >= 0.0);
        return Some((l, ch, Stage::Leaving { up }));
    }
    let n = next?;
    let l = lock_at(s, n)?;
    let ch = chamber(s, l)?;
    let f = step_progress(s, ag, n, pending_days);
    Some((l, ch, Stage::Entering { f, up: dir(ag.cell, n).dot(ch.axis) >= 0.0 }))
}

/// How far back a hauler waits for a lock with a raft ahead of it, past where a swimmer
/// alone waits, so the raft waits clear of the shut gate.
const RAFT_BACK: f32 = 1.2;

/// Where a swimmer at `p` near a lock is drawn: straight down the middle of the chamber and of
/// its way in between the walls, easing back onto its own line as it comes clear of them. The
/// simulation keeps it to the lock's lane and out of the walls (`sim::locks`), so this only
/// straightens the lane's steps. With `raft_back`, a hauler bringing a raft up to the gate is
/// drawn further back, so the raft ahead of it waits clear of the gate. `None` clear of the lock.
fn on_lane(ch: &Chamber, p: Vec2, raft_back: bool) -> Option<Vec2> {
    let (u, n) = (ch.axis, ch.axis.perp());
    let d = p - ch.centre;
    let (a, x) = (d.dot(u), d.dot(n));
    let end = ch.half * sim::locks::WALL_REACH;
    let clear = end + 1.5;
    let wide = sim::locks::HALF_WIDTH + 0.6;
    if a.abs() >= clear || x.abs() >= wide {
        return None;
    }
    let free = smooth(end - 0.5, clear, a.abs());
    let lane = 1.0 - smooth(0.8, wide, x.abs());
    let x2 = x * (1.0 - (1.0 - free) * lane);
    let back = if raft_back && a.abs() > ch.half { RAFT_BACK * (1.0 - smooth(ch.half + 0.5, clear, a.abs())) } else { 0.0 };
    let a2 = a + a.signum() * back;
    Some(ch.centre + u * a2 + n * x2)
}

/// Whether a beaver is pushing a raft.
fn with_raft(ag: &sim::agents::Agent) -> bool {
    matches!(ag.task, sim::agents::Task::Raft { leg: 1 | 3 | 5, .. })
}

fn cell_mid(s: &sim::SimState, c: (usize, usize)) -> Vec2 {
    cell_xz(s, (c.0 as f32 + 0.5, c.1 as f32 + 0.5))
}

/// Where a beaver is drawn by a lock, given where it would be drawn on its way (`base`), and
/// the way it faces when the lock decides that. Going through, it waits at the shut gate, swims in
/// when the gate sinks, rides the chamber's water up or down while the gates work, then swims
/// on down the lane and out over the far gate. Near a lock and not going through, it keeps to the
/// lane's middle (`on_lane`). `None` away from every lock.
pub(super) fn lock_pose(s: &sim::SimState, ag: &sim::agents::Agent, pending_days: f32, base: Vec3) -> Option<(Vec3, Option<f32>)> {
    if s.locks.at.values().all(|l| !l.built) {
        return None;
    }
    let raft = with_raft(ag);
    if let Some((l, ch, stage)) = trip(s, ag, pending_days) {
        let up = stage.up();
        let (entry, exit) = if up { (ch.low, ch.high) } else { (ch.high, ch.low) };
        let u = if up { ch.axis } else { -ch.axis };
        let yaw = u.x.atan2(u.y);
        let (xz, y) = match stage {
            Stage::Entering { f, .. } => {
                // Waiting where it came up to the gate, then into the lane's first cell.
                let wait = on_lane(&ch, cell_mid(s, ag.cell), raft).unwrap_or(ch.centre - u * (ch.half + 0.5));
                let first = ag.path.as_ref().and_then(|p| p.steps.first()).map(|st| st.0).unwrap_or(l.cell);
                let inside = on_lane(&ch, cell_mid(s, first), false).unwrap_or(ch.centre - u * (ch.half - 0.5));
                // Coming up to the wait as it was drawn on its way, and no further than it.
                let here = on_lane(&ch, base.xz(), raft).unwrap_or(base.xz());
                let come = if (here - ch.centre).dot(u) > (wait - ch.centre).dot(u) { wait } else { here };
                let xz = if f < SWIM_IN.0 { come } else { come.lerp(inside, smooth(SWIM_IN.0, SWIM_IN.1, f)) };
                (xz, entry + (exit - entry) * smooth(FILL.0, FILL.1, f))
            }
            Stage::Leaving { .. } => {
                // On down the lane as drawn on its way, never back behind where it came in.
                let here = on_lane(&ch, base.xz(), false).unwrap_or(base.xz());
                let came_in = ch.centre - u * (ch.half - 0.5);
                let a = (here - ch.centre).dot(u).max((came_in - ch.centre).dot(u));
                (ch.centre + u * a + ch.axis.perp() * (here - ch.centre).dot(ch.axis.perp()), exit)
            }
        };
        return Some((Vec3::new(xz.x, y, xz.y), Some(yaw)));
    }
    if ag.gait != sim::nav::Gait::Swim && !raft {
        return None;
    }
    let steps = ag.path.as_ref().map(|p| p.steps.as_slice()).unwrap_or(&[]);
    s.locks.at.values().filter(|l| l.built).find_map(|l| {
        let ch = chamber(s, l)?;
        let coming = raft && {
            let cells = sim::locks::lock_cells(s, l);
            steps.iter().take(8).any(|st| cells.contains(&st.0))
        };
        on_lane(&ch, base.xz(), coming).map(|xz| (Vec3::new(xz.x, base.y, xz.y), None))
    })
}

/// Each built lock's chamber.
pub(super) fn chambers(s: &sim::SimState) -> Vec<Chamber> {
    s.locks.at.values().filter(|l| l.built).filter_map(|l| chamber(s, l)).collect()
}

/// A beaver drawn near a lock, glided over any jump in where it would be drawn and turned
/// rather than snapped round: where it was last drawn and last put, how it was last moving
/// a frame, the way it faces, and whether it is near a lock.
#[derive(Component, Default)]
pub(super) struct LockGlide {
    shown: Option<Vec3>,
    last: Option<Vec3>,
    vel: Vec3,
    yaw: Option<f32>,
    near: bool,
}

/// A move in one frame that differs from the one before by more than this is a jump, glided
/// over: a swimmer speeds up, slows and turns far more gently.
const JUMP: f32 = 0.06;
/// Seconds a jump is glided over.
const GLIDE_S: f32 = 0.15;
/// How fast a beaver near a lock turns, radians a second.
const TURN: f32 = 2.5;

/// The way a beaver faces this frame, wanting to face `want`: near a lock it turns toward it
/// at `TURN`, so neither it nor the raft ahead of it swings round in a frame.
pub(super) fn turn(g: &mut LockGlide, want: f32, dt: f32) -> f32 {
    let yaw = match g.yaw.filter(|_| g.near) {
        Some(y) => {
            let d = (want - y + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI;
            y + d.clamp(-TURN * dt, TURN * dt)
        }
        None => want,
    };
    g.yaw = Some(yaw);
    yaw
}

/// Where a beaver is drawn this frame, given where it would be (`target`). Away from locks it is
/// drawn where it is; near one, a jump in where it would be is glided over, and a smooth move
/// is followed exactly.
pub(super) fn glide(g: &mut LockGlide, chs: &[Chamber], target: Vec3, dt: f32) -> Vec3 {
    let (Some(shown), Some(last)) = (g.shown, g.last) else {
        *g = LockGlide { shown: Some(target), last: Some(target), ..default() };
        return target;
    };
    g.last = Some(target);
    let near = |p: Vec2| {
        chs.iter().any(|ch| {
            let d = p - ch.centre;
            d.dot(ch.axis).abs() < ch.half * sim::locks::WALL_REACH + 4.0 && d.dot(ch.axis.perp()).abs() < 5.0
        })
    };
    let moved = (target - last).length();
    g.near = near(target.xz());
    if moved > 12.0 || (shown.distance(target) < 0.01 && !g.near) {
        g.shown = Some(target);
        g.vel = target - last;
        return target;
    }
    let k = 1.0 - (-dt / GLIDE_S).exp();
    // A smooth move is followed as it goes; a jump is glided over.
    let step = target - last;
    let jump = (step - g.vel).xz().length() > JUMP;
    // Through a jump it carries on as it was going, easing to where it now is.
    let from = if jump { shown + g.vel } else { shown + step };
    g.vel = if jump { g.vel * 0.5 } else { step };
    let next = from + (target - from) * k;
    g.shown = Some(next);
    next
}

/// Gates and chamber on the gallery's loop: a trip up, then one down.
pub(super) fn animate_locks(time: Res<Time>, mut locks: Query<&mut LockVis>, mut parts: Query<&mut Transform, Without<LockVis>>) {
    let dt = time.delta_secs();
    for mut l in &mut locks {
        if !l.looping {
            continue;
        }
        l.k += dt / TRIP_S;
        if l.k >= 1.0 {
            l.k -= 1.0;
            l.up = !l.up;
        }
        let (entry, exit) = if l.up { (l.low, l.high) } else { (l.high, l.low) };
        let (gates, level) = trip_look(l.k, entry, entry, exit);
        l.open = if l.up { gates } else { [gates[1], gates[0]] };
        l.level = level;
        show(&l, &mut parts);
    }
}

/// Sink or raise a lock's gates and set its chamber's water as its state says.
fn show(l: &LockVis, parts: &mut Query<&mut Transform, Without<LockVis>>) {
    for (&e, &open) in l.gates.iter().zip(&l.open) {
        if let Ok(mut t) = parts.get_mut(e) {
            t.translation.y = -open * 1.05 * l.h;
        }
    }
    if let Ok(mut t) = parts.get_mut(l.water) {
        t.translation.y = l.level.max(l.floor + 0.02) + 0.01;
    }
}

/// What the sim's locks look like now, keyed by cell.
#[derive(Default)]
pub(super) struct LockSync {
    drawn: HashMap<(usize, usize), Entity>,
}

/// A lock's flow axis (yaw of +z, from its low side to its high side), the water elevations
/// either side, and its gate's floor and top.
fn lock_frame(s: &sim::SimState, l: &sim::locks::Lock) -> Option<(f32, f32, f32, f32, f32)> {
    let c = l.cell;
    let (floor, top) = if let Some(k) = sim::canals::canal(s, l.building) {
        (k.gate.floor, k.gate.top)
    } else {
        let g = sim::dams::dam(s, l.building)?.gates.iter().find(|g| g.cell == c)?;
        (g.floor, g.top)
    };
    let f = sim::locks::frame(s, l)?;
    let flow = Vec2::new(f.axis.0, f.axis.1);
    let level = |d: Vec2| {
        let p = (c.0 as f32 + 0.5 + d.x * 1.6, c.1 as f32 + 0.5 + d.y * 1.6);
        let q = (p.0.max(0.0) as usize, p.1.max(0.0) as usize);
        s.height.get(q.0, q.1).copied().unwrap_or(0.0) + s.water.depth.get(q.0, q.1).copied().unwrap_or(0.0)
    };
    let (a, b) = (level(flow), level(-flow));
    // +z points to the high side.
    let (dir, low, high) = if a >= b { (flow, b, a) } else { (-flow, a, b) };
    Some((dir.x.atan2(dir.y), low, high, floor, top))
}

/// Draw each built lock and work it as the beaver going through it goes: gates and chamber follow
/// that beaver's trip (`trip`), so what is drawn is what the simulation is doing.
#[allow(clippy::too_many_arguments)]
pub(super) fn sync_locks(
    mut commands: Commands,
    sim: Res<Sim>,
    clock: Option<Res<crate::sim_driver::SimClock>>,
    time: Res<Time>,
    models: Option<Res<Models>>,
    hm: Option<Res<RaftLandingMeshes>>,
    mut st: Local<LockSync>,
    mut q: Query<&mut LockVis>,
    mut parts: Query<&mut Transform, Without<LockVis>>,
    mut meshes: ResMut<Assets<Mesh>>,
    water: Option<Res<crate::water_render::SharedWater>>,
) {
    let (Some(m), Some(hm)) = (models, hm) else { return };
    let s = &sim.0;
    let pending = clock.as_ref().map_or(0.0, |c| (c.render_days(s.time_days) - s.time_days) as f32);
    let built: Vec<&sim::locks::Lock> = s.locks.at.values().filter(|l| l.built).collect();
    st.drawn.retain(|c, e| {
        let keep = built.iter().any(|l| l.cell == *c);
        if !keep {
            if let Ok(v) = q.get(*e) {
                commands.entity(v.water).despawn();
            }
            commands.entity(*e).despawn();
        }
        keep
    });
    // Who is going through which lock, and how far. One works it at a time: someone leaving holds
    // the far gate open before the next comes in.
    let mut inside: HashMap<(usize, usize), (u64, Stage)> = HashMap::new();
    for ag in s.agents.by_id.values() {
        if let Some((l, _, stage)) = trip(s, ag, pending) {
            let e = inside.entry(l.cell).or_insert((ag.id.0, stage));
            if matches!(stage, Stage::Leaving { .. }) && matches!(e.1, Stage::Entering { .. }) {
                *e = (ag.id.0, stage);
            }
        }
    }
    let dt = time.delta_secs();
    for l in built {
        let Some(ch) = chamber(s, l) else { continue };
        let Some(&e) = st.drawn.get(&l.cell) else {
            let yaw = ch.axis.x.atan2(ch.axis.y);
            let e = spawn_lock(&mut commands, &hm, &m, Vec3::new(ch.centre.x, 0.0, ch.centre.y), yaw, ch.half, ch.floor, ch.low, ch.high, false);
            st.drawn.insert(l.cell, e);
            continue;
        };
        let Ok(mut v) = q.get_mut(e) else { continue };
        (v.low, v.high) = (ch.low, ch.high);
        // The chamber's water in the pond's own look: rebuilt when it freezes or thaws.
        if v.iced != ch.iced || !v.sheet {
            v.iced = ch.iced;
            v.sheet = true;
            let (a, p) = (ch.axis, ch.axis.perp() * 0.62 * WIDTH);
            let corner = |k: f32, j: f32| { let c = ch.centre + a * ch.half * k + p * j; Vec3::new(c.x, 0.0, c.y) };
            let mesh = meshes.add(crate::water_render::water_sheet([corner(-1.0, -1.0), corner(1.0, -1.0), corner(1.0, 1.0), corner(-1.0, 1.0)], (ch.high - ch.floor).max(0.5), if ch.iced { 1.0 } else { 0.0 }));
            if let Some(w) = &water {
                commands.entity(v.water).insert((Mesh3d(mesh), MeshMaterial3d(w.0.clone())));
            }
        }
        match inside.get(&l.cell).copied() {
            Some((who, stage)) => {
                if v.occupant != Some(who) {
                    v.occupant = Some(who);
                    v.start_level = v.level;
                }
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
                // Empty: the gates drop shut, and the chamber stays where it was left.
                v.occupant = None;
                for o in &mut v.open {
                    *o = (*o - dt / SHUT_S).max(0.0);
                }
            }
        }
        show(&v, &mut parts);
    }
}

/// The drawn rafts, by raft id: the raft and the heap of goods on it.
#[derive(Default)]
pub(super) struct RaftSync {
    drawn: HashMap<u32, (Entity, Entity, Option<sim::buildings::Good>)>,
}

/// Each raft: moored where it floats, rocking gently, or pushed along just ahead of its
/// hauler with the hauler's load heaped on it.
#[allow(clippy::too_many_arguments)]
pub(super) fn sync_rafts(
    mut commands: Commands,
    sim: Res<Sim>,
    models: Option<Res<Models>>,
    hm: Option<Res<RaftLandingMeshes>>,
    clock: Res<RenderClock>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut st: Local<RaftSync>,
    beavers: Query<(&super::BeaverSprite, &Transform), Without<RaftMark>>,
    mut tr: Query<(&mut Transform, &mut Visibility), With<RaftMark>>,
    mut heap_mesh: Local<HashMap<sim::buildings::Good, Handle<Mesh>>>,
) {
    let (Some(m), Some(hm)) = (models, hm) else { return };
    let s = &sim.0;
    st.drawn.retain(|id, (e, h, _)| {
        let keep = s.fleet.get(*id).is_some();
        if !keep {
            commands.entity(*e).despawn();
            commands.entity(*h).despawn();
        }
        keep
    });
    for b in &s.fleet.rafts {
        let entry = st.drawn.entry(b.id).or_insert_with(|| {
            let e = commands.spawn((Mesh3d(hm.raft.clone()), MeshMaterial3d(m.white.clone()), Transform::default(), Visibility::Inherited, RaftMark, super::SimEntity)).id();
            let h = commands.spawn((Mesh3d(hm.raft.clone()), MeshMaterial3d(m.white.clone()), Transform::default(), Visibility::Hidden, RaftMark, super::SimEntity)).id();
            (e, h, None)
        });
        let (e, h) = (entry.0, entry.1);
        let hauler = b.hauler.and_then(|w| s.agents.get(w).map(|a| (w, a)));
        let pushed = hauler.filter(|(_, a)| matches!(a.task, sim::agents::Task::Raft { leg: 1..=5, .. }));
        let drawn_hauler = pushed.and_then(|(w, _)| beavers.iter().find(|(sp, _)| sp.id == w.0).map(|(sp, t)| (sp.yaw, t.translation)));
        let (pos, yaw) = match drawn_hauler {
            // Just ahead of the beaver pushing it, the way it faces.
            Some((yaw, at)) => (at + Vec3::new(yaw.sin(), 0.0, yaw.cos()) * 0.8 + Vec3::Y * 0.05, yaw),
            None => {
                let c = (b.cell.0 as f32 + 0.5, b.cell.1 as f32 + 0.5);
                let xz = cell_xz(s, c);
                let surface = s.height.get(b.cell.0, b.cell.1).copied().unwrap_or(0.0) + s.water.depth.get(b.cell.0, b.cell.1).copied().unwrap_or(0.0);
                (Vec3::new(xz.x, surface + 0.02, xz.y), (b.id as f32 * 1.3).rem_euclid(std::f32::consts::TAU))
            }
        };
        let bob = 0.03 * (clock.0 * 1.3 + b.id as f32 * 2.1).sin();
        if let Ok((mut t, _)) = tr.get_mut(e) {
            *t = Transform::from_translation(pos + Vec3::Y * bob).with_rotation(Quat::from_rotation_y(yaw) * Quat::from_rotation_z(0.03 * (clock.0 * 1.1 + b.id as f32).sin())).with_scale(Vec3::splat(0.55));
        }
        // The load, heaped on the deck.
        let good = pushed.and_then(|(_, a)| a.load.map(|l| l.good));
        if entry.2 != good {
            entry.2 = good;
            if let Some(g) = good {
                let mesh = heap_mesh.entry(g).or_insert_with(|| meshes.add(super::models::heap_mesh(g))).clone();
                commands.entity(h).insert(Mesh3d(mesh));
            }
        }
        if let Ok((mut t, mut v)) = tr.get_mut(h) {
            *v = if good.is_some() { Visibility::Inherited } else { Visibility::Hidden };
            *t = Transform::from_translation(pos + Vec3::Y * (bob + 0.1)).with_rotation(Quat::from_rotation_y(yaw)).with_scale(Vec3::splat(0.75));
        }
    }
}

/// A raft or the load on one.
#[derive(Component)]
pub(super) struct RaftMark;

/// `BC_GALLERY_WHAT=raft landing`: a raft with each load in a row, and a lock taking beavers up
/// and down on a loop beside them.
pub(super) fn spawn_gallery(commands: &mut Commands, hm: &RaftLandingMeshes, m: &Models, meshes: &mut Assets<Mesh>, centre: Vec3, ground: impl Fn(f32, f32) -> f32) {
    let goods = sim::buildings::Good::ALL;
    for (i, g) in goods.into_iter().enumerate() {
        let x = centre.x + (i as f32 - (goods.len() as f32 - 1.0) / 2.0) * 1.8;
        let z = centre.z + 2.0;
        let raft = commands.spawn((Mesh3d(hm.raft.clone()), MeshMaterial3d(m.white.clone()), Transform::from_xyz(x, ground(x, z) + 0.12, z).with_scale(Vec3::splat(0.55)))).id();
        let heap = meshes.add(super::models::heap_mesh(g));
        commands.spawn((Mesh3d(heap), MeshMaterial3d(m.white.clone()), Transform::from_xyz(0.0, 0.18, -0.1).with_scale(Vec3::splat(1.3)), ChildOf(raft)));
    }
    let (x, z) = (centre.x, centre.z - 2.5);
    let g = ground(x, z);
    spawn_lock(commands, hm, m, Vec3::new(x, g, z), std::f32::consts::FRAC_PI_2, 2.2, g - 0.2, g + 0.25, g + 1.05, true);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lock() -> Chamber {
        Chamber { centre: Vec2::ZERO, axis: Vec2::Y, half: 2.2, low: 0.0, high: 1.0, floor: -0.5, iced: false }
    }

    #[test]
    fn a_swimmer_setting_off_from_rest_is_followed_and_a_hop_is_glided() {
        let ch = lock();
        let mut g = LockGlide::default();
        let mut p = Vec3::new(0.0, 1.0, -5.0);
        for _ in 0..30 {
            glide(&mut g, &[ch], p, 1.0 / 60.0);
        }
        for _ in 0..60 {
            p.z += 0.06;
            let at = glide(&mut g, &[ch], p, 1.0 / 60.0);
            assert!(at.distance(p) < 0.03, "keeps up: {at} vs {p}");
        }
        // Stopped, then put 0.13 to one side: no hop in one frame.
        for _ in 0..30 {
            glide(&mut g, &[ch], p, 1.0 / 60.0);
        }
        let before = glide(&mut g, &[ch], p, 1.0 / 60.0);
        let after = glide(&mut g, &[ch], p + Vec3::X * 0.13, 1.0 / 60.0);
        assert!(after.distance(before) < 0.03);
    }

    #[test]
    fn a_swimmer_stopping_short_stops_where_it_is() {
        let ch = lock();
        let mut g = LockGlide::default();
        let mut p = Vec3::new(0.0, 1.0, -6.0);
        for _ in 0..60 {
            p.z += 0.07;
            glide(&mut g, &[ch], p, 1.0 / 60.0);
        }
        let mut at = Vec3::ZERO;
        for _ in 0..90 {
            at = glide(&mut g, &[ch], p, 1.0 / 60.0);
            assert!(at.z <= p.z + 0.15, "no overshoot: {at} vs {p}");
        }
        assert!(at.distance(p) < 0.01);
    }

    #[test]
    fn a_beaver_by_a_lock_turns_rather_than_snapping_round() {
        let ch = lock();
        let mut g = LockGlide::default();
        glide(&mut g, &[ch], Vec3::new(0.0, 1.0, -4.0), 1.0 / 60.0);
        glide(&mut g, &[ch], Vec3::new(0.0, 1.0, -4.0), 1.0 / 60.0);
        turn(&mut g, 0.5, 1.0 / 60.0);
        let y = turn(&mut g, 0.0, 1.0 / 60.0);
        assert!((y - (0.5 - TURN / 60.0)).abs() < 1e-4);
    }

    #[test]
    fn a_smooth_swim_is_followed_exactly() {
        let ch = lock();
        let mut g = LockGlide::default();
        for i in 0..60 {
            let p = Vec3::new(5.0, 1.0, -3.0 + i as f32 * 0.05);
            let at = glide(&mut g, &[ch], p, 1.0 / 60.0);
            assert!(at.distance(p) < 1e-4);
        }
    }

    #[test]
    fn the_lane_is_drawn_straight_and_meets_the_swimmer_s_own_line_past_the_walls() {
        let ch = lock();
        let inside = on_lane(&ch, Vec2::new(0.45, 1.0), false).unwrap();
        assert!(inside.x.abs() < 1e-4 && (inside.y - 1.0).abs() < 1e-4);
        // Moving out along a slanted lane: no step bigger than the move itself, out to its line.
        let mut last = on_lane(&ch, Vec2::new(0.4, 2.0), false).unwrap();
        for i in 1..=200 {
            let p = Vec2::new(0.4, 2.0 + i as f32 * 0.02);
            let at = on_lane(&ch, p, false).unwrap_or(p);
            assert!(at.distance(last) < 0.05, "jump at {p}");
            last = at;
        }
    }

    #[test]
    fn a_hauler_waits_back_far_enough_for_its_raft_and_comes_on_without_a_jump() {
        let ch = lock();
        let wait = on_lane(&ch, Vec2::new(0.0, -(ch.half + 0.5)), true).unwrap();
        assert!(-wait.y >= ch.half + 0.5 + RAFT_BACK - 1e-3);
        let mut last = wait;
        for i in 1..=200 {
            let p = Vec2::new(0.0, -(ch.half + 0.5) - i as f32 * 0.02);
            let at = on_lane(&ch, p, true).unwrap_or(p);
            assert!(at.distance(last) < 0.05 && at.y <= last.y, "jump at {p}");
            last = at;
        }
    }
}
