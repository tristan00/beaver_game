//! Harbors, barges and locks: their models, the barges moored at a harbor, and a lock working
//! (gates swinging, the chamber filling or draining) while a beaver passes through it.
//! `BC_GALLERY_WHAT=harbor` lines up a harbor's stages, a barge with each load and a lock on a
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

/// A barge: a raft of logs lashed side by side, a low rail along each side and a raised prow,
/// about two and a half cells long. Its cargo is drawn on top separately.
pub fn barge_mesh(mb: &mut Mb) {
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

/// A box from `a` to `b` (opposite corners), its lower face `lo` and its upper `hi`.
fn slab(mb: &mut Mb, a: Vec3, b: Vec3, lo: [f32; 3], hi: [f32; 3]) {
    mb.hex([v(a.x, a.y, a.z), v(b.x, a.y, a.z), v(b.x, a.y, b.z), v(a.x, a.y, b.z), v(a.x, b.y, a.z), v(b.x, b.y, a.z), v(b.x, b.y, b.z), v(a.x, b.y, b.z)], lo, hi);
}

/// One leaf of a lock's mitre gates: a heavy door of planks, hinged on a thick heel post at its
/// left end (x = 0) and reaching along +x, faced with cross planks and a diagonal brace, with a
/// balance beam along its top running back past the hinge, as a real lock's gates have. One
/// unit tall (scaled to the chamber's height).
pub fn lock_leaf_mesh(mb: &mut Mb) {
    let mut r = Rng(5353);
    let (wood, dark) = (col(palette::BARK), col(palette::MUD_DARK));
    // The door: planks side by side, wet and dark low down.
    slab(mb, v(0.02, -0.3, -0.06), v(0.6, 1.0, 0.06), dark, wood);
    for y in [0.12f32, 0.5, 0.88] {
        stick_c(mb, v(0.0, y, 0.08), v(0.6, y, 0.08), 0.06, col(palette::PEELED));
        stick_c(mb, v(0.0, y, -0.08), v(0.6, y, -0.08), 0.06, col(palette::PEELED));
    }
    stick(mb, v(0.05, 0.1, 0.09), v(0.56, 0.86, 0.09), 0.05, r.next());
    // The heel post it swings on, and the mitre post where the pair meet.
    stick_c(mb, v(0.0, -0.3, 0.0), v(0.0, 1.12, 0.0), 0.11, col(palette::PEELED));
    stick(mb, v(0.6, -0.3, 0.0), v(0.6, 1.04, 0.0), 0.08, r.next());
    // The balance beam, out over the wall to push the gate open by.
    stick_c(mb, v(-0.75, 1.1, 0.0), v(0.6, 1.1, 0.0), 0.1, col(palette::PEELED));
}

/// A lock's chamber walls either side of the channel along the flow (z): a face of planks on
/// the inside, for the water's level to show against, backed by stakes and capped with a
/// beam to walk on; and the posts the gates hang on. One unit tall.
pub fn lock_walls_mesh(mb: &mut Mb) {
    let mut r = Rng(7373);
    let (wood, dark) = (col(palette::BARK), col(palette::MUD_DARK));
    for s in [-1.0f32, 1.0] {
        let x = s * 0.66;
        // The plank face, from below the floor to the top.
        let (a, b) = (x + s * 0.02, x + s * 0.1);
        slab(mb, v(a.min(b), -0.3, -1.55), v(a.max(b), 1.0, 1.55), dark, wood);
        for k in 0..9 {
            let y = -0.2 + k as f32 * 0.14;
            stick(mb, v(x + s * 0.005, y, -1.55), v(x + s * 0.005, y, 1.55), 0.03, r.next() * 0.3);
        }
        for k in 0..8 {
            let z = -1.45 + k as f32 * 0.414;
            stick(mb, v(x + s * 0.14, -0.3, z), v(x + s * 0.14, 1.06 + r.range(-0.04, 0.04), z), 0.07, r.next());
        }
        stick_c(mb, v(x + s * 0.08, 1.04, -1.6), v(x + s * 0.08, 1.04, 1.6), 0.12, col(palette::PEELED));
        // Gate posts at both ends.
        for z in [-1.05f32, 1.05] {
            stick_c(mb, v(x, -0.3, z), v(x, 1.25, z), 0.12, col(palette::PEELED));
        }
    }
}

#[derive(Resource)]
pub(super) struct HarborMeshes {
    pub barge: Handle<Mesh>,
    pub leaf: Handle<Mesh>,
    pub walls: Handle<Mesh>,
}

pub(super) fn init_harbor_meshes(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>) {
    let mut build = |f: &dyn Fn(&mut Mb)| {
        let mut mb = Mb::new();
        f(&mut mb);
        meshes.add(mb.build())
    };
    commands.insert_resource(HarborMeshes {
        barge: build(&barge_mesh),
        leaf: build(&lock_leaf_mesh),
        walls: build(&lock_walls_mesh),
    });
}

/// A drawn lock: its parts, its water's levels either side, and what its gates and chamber
/// are doing now.
#[derive(Component)]
pub(super) struct LockVis {
    pub leaves: [Entity; 4],
    pub water: Entity,
    /// Elevations of the water on the low and high side, and of the chamber's floor.
    pub low: f32,
    pub high: f32,
    pub floor: f32,
    /// How far open the gates at the low end and the high end are, 0 shut to 1 open.
    pub open: [f32; 2],
    /// The chamber's water level now, and where it stood when the passage under way began.
    pub level: f32,
    pub start_level: f32,
    /// Who is passing now.
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

/// Seconds a passage takes on the gallery's loop.
const PASSAGE_S: f32 = 5.0;
/// Seconds the far gates take to swing shut behind a beaver that has left.
const SHUT_S: f32 = 0.8;

/// Spawn a lock's parts at `at` (the chamber's middle), the chamber running along `axis_yaw`
/// with the low side at -z, `half` long either way from the middle, its floor at `floor`.
#[allow(clippy::too_many_arguments)]
pub(super) fn spawn_lock(commands: &mut Commands, hm: &HarborMeshes, m: &Models, at: Vec3, axis_yaw: f32, half: f32, floor: f32, low: f32, high: f32, looping: bool) -> Entity {
    // The walls stand a little above the high side's water: high enough to read as a lock,
    // low enough not to hide what is in it.
    let h = (high + 0.3 - floor).max(0.4);
    let w = &m.white;
    // Wide enough to stand clear of the dam either side, with room for a barge.
    let root = commands
        .spawn((Transform::from_translation(Vec3::new(at.x, floor, at.z)).with_rotation(Quat::from_rotation_y(axis_yaw)).with_scale(Vec3::new(WIDTH, 1.0, 1.0)), Visibility::Inherited, super::SimEntity))
        .id();
    // The walls' gate posts stand at ±1.05 in their mesh.
    commands.spawn((Mesh3d(hm.walls.clone()), MeshMaterial3d(w.clone()), Transform::from_scale(Vec3::new(1.0, h, half / 1.05)), ChildOf(root)));
    let water = commands.spawn((Transform::from_xyz(0.0, low, 0.0), Visibility::Inherited, super::SimEntity, bevy::light::NotShadowCaster)).id();
    // Leaves: at each end a pair hinged on the posts, meeting in the middle.
    let mut leaves = [Entity::PLACEHOLDER; 4];
    for (k, (x, z, flip)) in [(-0.62f32, -half, false), (0.62, -half, true), (-0.62, half, false), (0.62, half, true)].into_iter().enumerate() {
        let tr = Transform::from_xyz(x, 0.0, z).with_scale(Vec3::new(if flip { -1.0 } else { 1.0 }, h * 0.98, 1.0));
        leaves[k] = commands.spawn((Mesh3d(hm.leaf.clone()), MeshMaterial3d(w.clone()), tr, ChildOf(root))).id();
    }
    commands.entity(root).insert(LockVis { leaves, water, low, high, floor, open: [0.0; 2], level: low, start_level: low, occupant: None, iced: false, sheet: false, looping, k: 0.0, up: true });
    root
}

fn smooth(a: f32, b: f32, x: f32) -> f32 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

// The steps of a passage, as shares of the wait the simulation gives it (`nav::lock_wait`):
// the chamber levels to the near side while the swimmer waits at the shut gate; the gate
// opens; it swims in; the gate shuts behind it; the chamber fills or drains to the far side's
// level, lifting or lowering it; the far gate opens, and it swims out.
const ENTRY_OPEN: (f32, f32) = (0.2, 0.3);
const SWIM_IN: (f32, f32) = (0.3, 0.45);
const ENTRY_SHUT: (f32, f32) = (0.45, 0.55);
const FILL: (f32, f32) = (0.55, 0.85);
const EXIT_OPEN: (f32, f32) = (0.85, 1.0);

/// The gates (near end, far end) and the chamber's level `f` of the way through a passage:
/// from `from` (where it stood) to `entry`, then to `exit`.
fn passage_look(f: f32, from: f32, entry: f32, exit: f32) -> ([f32; 2], f32) {
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
/// least long enough for a barge with its beaver.
pub(super) fn chamber(s: &sim::SimState, l: &sim::locks::Lock) -> Option<Chamber> {
    let (yaw, _, _, floor, _) = lock_frame(s, l)?;
    let axis = Vec2::new(yaw.sin(), yaw.cos());
    let cells = sim::locks::lock_cells(s, l);
    let pts: Vec<Vec2> = cells.iter().map(|c| cell_xz(s, (c.0 as f32 + 0.5, c.1 as f32 + 0.5))).collect();
    let centre = pts.iter().copied().sum::<Vec2>() / pts.len().max(1) as f32;
    let span = pts.iter().map(|p| (*p - centre).dot(axis).abs()).fold(0.0, f32::max);
    let half = (span + 0.8).max(2.2);
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
/// out through the chamber (how far out), and whether it goes up, from the low side to the high.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum Stage {
    Entering { f: f32, up: bool },
    Leaving { g: f32, up: bool },
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

/// The lock this beaver is passing and how far it has got, `pending_days` after the simulation
/// last moved it (negative while the picture plays catching up).
pub(super) fn passage<'a>(s: &'a sim::SimState, ag: &sim::agents::Agent, pending_days: f32) -> Option<(&'a sim::locks::Lock, Chamber, Stage)> {
    let steps = ag.path.as_ref().map(|p| p.steps.as_slice()).unwrap_or(&[]);
    let next = steps.first().map(|x| x.0);
    let dir = |a: (usize, usize), b: (usize, usize)| Vec2::new(b.0 as f32 - a.0 as f32, b.1 as f32 - a.1 as f32);
    if let Some(l) = lock_at(s, ag.cell) {
        let ch = chamber(s, l)?;
        let up = next.is_none_or(|n| dir(ag.cell, n).dot(ch.axis) >= 0.0);
        // Out through the chamber: the steps still to take to its first cell beyond, against
        // the lock's length in cells.
        let cells = sim::locks::lock_cells(s, l);
        let left = steps.iter().position(|st| !cells.contains(&st.0)).map_or(steps.len(), |i| i + 1) as f32;
        let f = next.map_or(1.0, |n| step_progress(s, ag, n, pending_days));
        let g = (1.0 - (left - f) / cells.len().max(1) as f32).clamp(0.0, 1.0);
        return Some((l, ch, Stage::Leaving { g, up }));
    }
    let n = next?;
    let l = lock_at(s, n)?;
    let ch = chamber(s, l)?;
    let f = step_progress(s, ag, n, pending_days);
    Some((l, ch, Stage::Entering { f, up: dir(ag.cell, n).dot(ch.axis) >= 0.0 }))
}

/// How far outside a gate a beaver waits for it, more with a barge pushed ahead of it.
fn wait_off(ag: &sim::agents::Agent) -> f32 {
    if matches!(ag.task, sim::agents::Task::Barge { leg: 1 | 3 | 5, .. }) { 1.8 } else { 0.45 }
}

/// Where a beaver is drawn by a lock, and the way it faces. Passing it, it waits outside the
/// shut gate, swims in when the gate opens, rides the chamber's water up or down, and swims
/// out straight down the chamber past the far gate. Not passing, it is kept out of the chamber,
/// at the gate nearest it. `None` away from every lock.
pub(super) fn lock_pose(s: &sim::SimState, ag: &sim::agents::Agent, pending_days: f32) -> Option<(Vec3, f32)> {
    let Some((_, ch, stage)) = passage(s, ag, pending_days) else { return keep_out(s, ag, pending_days) };
    let up = stage.up();
    let (entry, exit) = if up { (ch.low, ch.high) } else { (ch.high, ch.low) };
    let u = if up { ch.axis } else { -ch.axis };
    let yaw = u.x.atan2(u.y);
    let barge = wait_off(ag) > 1.0;
    let wait = ch.centre - u * (ch.half + wait_off(ag));
    // In the chamber: with a barge ahead, far enough back for the barge to fit between the gates.
    let hold = ch.centre - u * if barge { 0.45 } else { 0.0 };
    let out = ch.centre + u * (ch.half + 0.45);
    let (xz, y) = match stage {
        Stage::Entering { f, .. } => {
            let xz = if f < SWIM_IN.0 { wait } else { wait.lerp(hold, smooth(SWIM_IN.0, SWIM_IN.1, f)) };
            (xz, entry + (exit - entry) * smooth(FILL.0, FILL.1, f))
        }
        Stage::Leaving { g, .. } => (hold.lerp(out, g), exit),
    };
    Some((Vec3::new(xz.x, y, xz.y), yaw))
}

/// A swimmer by a lock but not passing it is drawn outside its chamber, at the gate it is
/// nearer, so nothing is seen inside the chamber but what is passing through. One about to
/// pass waits there with room for its barge.
fn keep_out(s: &sim::SimState, ag: &sim::agents::Agent, pending_days: f32) -> Option<(Vec3, f32)> {
    let barge = matches!(ag.task, sim::agents::Task::Barge { leg: 1 | 3 | 5, .. });
    if ag.gait != sim::nav::Gait::Swim && !barge {
        return None;
    }
    if s.locks.at.values().all(|l| !l.built) {
        return None;
    }
    let p = super::sim_view::agent_world_xz(s, ag, pending_days);
    let steps = ag.path.as_ref().map(|p| p.steps.as_slice()).unwrap_or(&[]);
    for l in s.locks.at.values().filter(|l| l.built) {
        let Some(ch) = chamber(s, l) else { continue };
        let d = p - ch.centre;
        let (along, across) = (d.dot(ch.axis), d.dot(ch.axis.perp()));
        // Coming up to pass it: wait room on the near side.
        let cells = sim::locks::lock_cells(s, l);
        let coming = steps.iter().take(6).any(|st| cells.contains(&st.0));
        let off = if coming { wait_off(ag) } else { 0.45 };
        if across.abs() >= 0.62 * WIDTH + 0.3 || along.abs() >= ch.half + off {
            continue;
        }
        let side = if along >= 0.0 { 1.0 } else { -1.0 };
        let at = ch.centre + ch.axis * side * (ch.half + off) + ch.axis.perp() * across;
        let y = if side > 0.0 { ch.high } else { ch.low };
        let toward = super::sim_view::facing_of(s, ag).unwrap_or((-ch.axis * side).x.atan2((-ch.axis * side).y));
        return Some((Vec3::new(at.x, y, at.y), toward));
    }
    None
}

/// Gates and chamber on the gallery's loop: a passage up, then one down.
pub(super) fn animate_locks(time: Res<Time>, mut locks: Query<&mut LockVis>, mut parts: Query<&mut Transform, Without<LockVis>>) {
    let dt = time.delta_secs();
    for mut l in &mut locks {
        if !l.looping {
            continue;
        }
        l.k += dt / PASSAGE_S;
        if l.k >= 1.0 {
            l.k -= 1.0;
            l.up = !l.up;
        }
        let (entry, exit) = if l.up { (l.low, l.high) } else { (l.high, l.low) };
        let (gates, level) = passage_look(l.k, entry, entry, exit);
        l.open = if l.up { gates } else { [gates[1], gates[0]] };
        l.level = level;
        show(&l, &mut parts);
    }
}

/// Swing a lock's leaves and set its chamber's water as its state says.
fn show(l: &LockVis, parts: &mut Query<&mut Transform, Without<LockVis>>) {
    for (i, &e) in l.leaves.iter().enumerate() {
        // Leaves 0 and 1 are at the low end (-z), 2 and 3 at the high end.
        let open = l.open[i / 2];
        let side = if i % 2 == 0 { 1.0 } else { -1.0 };
        // Shut: the pair meet in a V pointing upstream; open: folded back against the walls.
        let shut = if i < 2 { 0.35 } else { -0.35 };
        let open_a = if i < 2 { -1.45 } else { 1.45 };
        let a = shut + (open_a - shut) * open;
        if let Ok(mut t) = parts.get_mut(e) {
            t.rotation = Quat::from_rotation_y(side * a);
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
    let (floor, top, along): (f32, f32, Vec<(usize, usize)>) = if let Some(k) = sim::canals::canal(s, l.building) {
        // A canal's gate: the flow runs along the canal.
        (k.gate.floor, k.gate.top, k.cells.iter().filter(|x| x.row == 0).map(|x| x.cell).collect())
    } else {
        let d = sim::dams::dam(s, l.building)?;
        let g = d.gates.iter().find(|g| g.cell == c)?;
        // Along the drawn line only: the rows a widened dam adds lie behind it.
        (g.floor, g.top, d.cells.iter().filter(|x| x.row == 0).map(|x| x.cell).collect())
    };
    let near: Vec<(i32, i32)> = along.iter().filter(|&&o| o != c && o.0.abs_diff(c.0) <= 1 && o.1.abs_diff(c.1) <= 1).map(|o| (o.0 as i32 - c.0 as i32, o.1 as i32 - c.1 as i32)).collect();
    let line = near.first().copied().unwrap_or((1, 0));
    let line = Vec2::new(line.0 as f32, line.1 as f32).normalize_or(Vec2::X);
    // A dam's line runs across the flow; a canal's along it.
    let flow = if sim::canals::canal(s, l.building).is_some() { line } else { Vec2::new(-line.y, line.x) };
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

/// Draw each built lock and work it as the beaver passing it goes: gates and chamber follow
/// that beaver's passage (`passage`), so what is drawn is what the simulation is doing.
#[allow(clippy::too_many_arguments)]
pub(super) fn sync_locks(
    mut commands: Commands,
    sim: Res<Sim>,
    clock: Option<Res<crate::sim_driver::SimClock>>,
    time: Res<Time>,
    models: Option<Res<Models>>,
    hm: Option<Res<HarborMeshes>>,
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
    // Who is passing which lock, and how far. One works it at a time: someone leaving holds
    // the far gate open before the next comes in.
    let mut passing: HashMap<(usize, usize), (u64, Stage)> = HashMap::new();
    for ag in s.agents.by_id.values() {
        if let Some((l, _, stage)) = passage(s, ag, pending) {
            let e = passing.entry(l.cell).or_insert((ag.id.0, stage));
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
        match passing.get(&l.cell).copied() {
            Some((who, stage)) => {
                if v.occupant != Some(who) {
                    v.occupant = Some(who);
                    v.start_level = v.level;
                }
                let up = stage.up();
                let (entry, exit) = if up { (ch.low, ch.high) } else { (ch.high, ch.low) };
                let (gates, level) = match stage {
                    Stage::Entering { f, .. } => passage_look(f, v.start_level, entry, exit),
                    Stage::Leaving { .. } => ([0.0, 1.0], exit),
                };
                v.open = if up { gates } else { [gates[1], gates[0]] };
                v.level = level;
            }
            None => {
                // Empty: the gates swing shut, and the chamber stays where it was left.
                v.occupant = None;
                for o in &mut v.open {
                    *o = (*o - dt / SHUT_S).max(0.0);
                }
            }
        }
        show(&v, &mut parts);
    }
}

/// The drawn barges, by barge id: the barge and the heap of goods on it.
#[derive(Default)]
pub(super) struct BargeSync {
    drawn: HashMap<u32, (Entity, Entity, Option<sim::buildings::Good>)>,
}

/// Each barge: moored where it floats, rocking gently, or pushed along just ahead of its
/// hauler with the hauler's load heaped on it.
#[allow(clippy::too_many_arguments)]
pub(super) fn sync_barges(
    mut commands: Commands,
    sim: Res<Sim>,
    models: Option<Res<Models>>,
    hm: Option<Res<HarborMeshes>>,
    clock: Res<RenderClock>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut st: Local<BargeSync>,
    beavers: Query<(&super::BeaverSprite, &Transform), Without<BargeMark>>,
    mut tr: Query<(&mut Transform, &mut Visibility), With<BargeMark>>,
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
    for b in &s.fleet.barges {
        let entry = st.drawn.entry(b.id).or_insert_with(|| {
            let e = commands.spawn((Mesh3d(hm.barge.clone()), MeshMaterial3d(m.white.clone()), Transform::default(), Visibility::Inherited, BargeMark, super::SimEntity)).id();
            let h = commands.spawn((Mesh3d(hm.barge.clone()), MeshMaterial3d(m.white.clone()), Transform::default(), Visibility::Hidden, BargeMark, super::SimEntity)).id();
            (e, h, None)
        });
        let (e, h) = (entry.0, entry.1);
        let hauler = b.hauler.and_then(|w| s.agents.get(w).map(|a| (w, a)));
        let pushed = hauler.filter(|(_, a)| matches!(a.task, sim::agents::Task::Barge { leg: 1..=5, .. }));
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

/// A barge or the load on one.
#[derive(Component)]
pub(super) struct BargeMark;

/// `BC_GALLERY_WHAT=harbor`: a barge with each load in a row, and a lock passing beavers up
/// and down on a loop beside them.
pub(super) fn spawn_gallery(commands: &mut Commands, hm: &HarborMeshes, m: &Models, meshes: &mut Assets<Mesh>, centre: Vec3, ground: impl Fn(f32, f32) -> f32) {
    let goods = sim::buildings::Good::ALL;
    for (i, g) in goods.into_iter().enumerate() {
        let x = centre.x + (i as f32 - (goods.len() as f32 - 1.0) / 2.0) * 1.8;
        let z = centre.z + 2.0;
        let barge = commands.spawn((Mesh3d(hm.barge.clone()), MeshMaterial3d(m.white.clone()), Transform::from_xyz(x, ground(x, z) + 0.12, z).with_scale(Vec3::splat(0.55)))).id();
        let heap = meshes.add(super::models::heap_mesh(g));
        commands.spawn((Mesh3d(heap), MeshMaterial3d(m.white.clone()), Transform::from_xyz(0.0, 0.18, -0.1).with_scale(Vec3::splat(1.3)), ChildOf(barge)));
    }
    let (x, z) = (centre.x, centre.z - 2.5);
    let g = ground(x, z);
    spawn_lock(commands, hm, m, Vec3::new(x, g, z), std::f32::consts::FRAC_PI_2, 2.2, g - 0.2, g + 0.25, g + 1.05, true);
}
