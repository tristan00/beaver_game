//! Harbors, barges and locks: their models, the barges moored at a harbor, and a lock working
//! (gates swinging, the chamber filling or draining) while a beaver passes through it.
//! `BC_GALLERY_WHAT=harbor` lines up a harbor's stages, a barge with each load and a lock on a
//! loop (see `gallery.rs`).
use super::meshgen::{hex, mix, Mb, Rng};
use super::models::{col, stick, stick_c};
use super::scene::height_at;
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

/// One leaf of a lock's mitre gates: a panel of upright sticks, hinged at its left end
/// (x = 0) and reaching along +x, one unit tall (scaled to the gate's height).
pub fn lock_leaf_mesh(mb: &mut Mb) {
    let mut r = Rng(5353);
    let n = 6;
    for i in 0..n {
        let x = 0.04 + i as f32 / (n - 1) as f32 * 0.52;
        stick(mb, v(x, 0.0, 0.0), v(x, 1.0, 0.0), 0.045, r.next());
    }
    for y in [0.15f32, 0.55, 0.92] {
        stick_c(mb, v(0.0, y, 0.0), v(0.6, y, 0.0), 0.04, col(palette::PEELED));
    }
    // A brace from the hinge foot to the far top.
    stick(mb, v(0.03, 0.08, 0.0), v(0.55, 0.9, 0.0), 0.035, r.next());
}

/// A lock's chamber walls: stick-and-mud banks either side of the channel, along the flow
/// (z), one unit tall; and the posts the gates hang on.
pub fn lock_walls_mesh(mb: &mut Mb) {
    let mut r = Rng(7373);
    let mud = col(palette::DAM_MUD);
    for s in [-1.0f32, 1.0] {
        let x = s * 0.66;
        // A bank sloping away outside, faced inside with upright stakes.
        let prof = [(-1.55f32, 0.0f32), (1.55, 0.0)];
        let _ = prof;
        mb.hex(
            [
                v(x, -0.3, -1.55), v(x + s * 0.7, -0.3, -1.55), v(x + s * 0.7, -0.3, 1.55), v(x, -0.3, 1.55),
                v(x, 1.0, -1.55), v(x + s * 0.18, 1.0, -1.55), v(x + s * 0.18, 1.0, 1.55), v(x, 1.0, 1.55),
            ],
            mix(mud, col(palette::MUD_DARK), 0.5),
            mud,
        );
        for k in 0..12 {
            let z = -1.45 + k as f32 * 0.264;
            stick(mb, v(x - s * 0.02, -0.3, z), v(x - s * 0.02, 1.05 + r.range(-0.04, 0.04), z), 0.045, r.next());
        }
        stick(mb, v(x, 1.03, -1.55), v(x, 1.03, 1.55), 0.06, r.next());
        // Gate posts at both ends.
        for z in [-1.05f32, 1.05] {
            stick_c(mb, v(x, -0.3, z), v(x, 1.25, z), 0.08, col(palette::PEELED));
        }
    }
}

/// The water standing in a lock's chamber: a flat sheet (lifted or lowered as it fills).
pub fn chamber_water_mesh(mb: &mut Mb) {
    let c = hex(0x6fb4c4);
    mb.quad2([v(-0.6, 0.0, -1.0), v(0.6, 0.0, -1.0), v(0.6, 0.0, 1.0), v(-0.6, 0.0, 1.0)], c);
}

#[derive(Resource)]
pub(super) struct HarborMeshes {
    pub barge: Handle<Mesh>,
    pub leaf: Handle<Mesh>,
    pub walls: Handle<Mesh>,
    pub water: Handle<Mesh>,
    pub water_mat: Handle<StandardMaterial>,
}

pub(super) fn init_harbor_meshes(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>, mut materials: ResMut<Assets<StandardMaterial>>) {
    let mut build = |f: &dyn Fn(&mut Mb)| {
        let mut mb = Mb::new();
        f(&mut mb);
        meshes.add(mb.build())
    };
    commands.insert_resource(HarborMeshes {
        barge: build(&barge_mesh),
        leaf: build(&lock_leaf_mesh),
        walls: build(&lock_walls_mesh),
        water: build(&chamber_water_mesh),
        water_mat: materials.add(StandardMaterial { base_color: Color::srgba(0.45, 0.72, 0.8, 0.8), alpha_mode: AlphaMode::Blend, perceptual_roughness: 0.3, double_sided: true, cull_mode: None, ..default() }),
    });
}

/// A drawn lock and how far through letting a beaver past it is.
#[derive(Component)]
pub(super) struct LockVis {
    pub leaves: [Entity; 4],
    pub water: Entity,
    /// Elevations of the water on the low and high side, and of the gate's floor and top.
    pub low: f32,
    pub high: f32,
    pub floor: f32,
    pub top: f32,
    /// 0 at rest; runs to 1 over a passage.
    pub k: f32,
    /// Whether the one passing goes up (from the low side to the high).
    pub up: bool,
    /// Shown on a loop (the gallery) rather than driven by beavers.
    pub looping: bool,
}

/// Seconds a passage takes to draw: in, gates shut, fill or drain, out.
const PASSAGE_S: f32 = 4.0;

/// Spawn a lock's parts at `at` (on its floor), the chamber running along `axis_yaw`, and the
/// low side at -z.
pub(super) fn spawn_lock(commands: &mut Commands, hm: &HarborMeshes, m: &Models, at: Vec3, axis_yaw: f32, floor: f32, top: f32, low: f32, high: f32, looping: bool) -> Entity {
    let h = (top - floor).max(0.3);
    let w = &m.white;
    let root = commands.spawn((Transform::from_translation(Vec3::new(at.x, floor, at.z)).with_rotation(Quat::from_rotation_y(axis_yaw)), Visibility::Inherited, super::SimEntity)).id();
    commands.spawn((Mesh3d(hm.walls.clone()), MeshMaterial3d(w.clone()), Transform::from_scale(Vec3::new(1.0, h, 1.0)), ChildOf(root)));
    let water = commands.spawn((Mesh3d(hm.water.clone()), MeshMaterial3d(hm.water_mat.clone()), Transform::from_xyz(0.0, (low - floor).max(0.02), 0.0), ChildOf(root))).id();
    // Leaves: at each end a pair hinged on the posts, meeting in the middle.
    let mut leaves = [Entity::PLACEHOLDER; 4];
    for (k, (x, z, flip)) in [(-0.62f32, -1.05f32, false), (0.62, -1.05, true), (-0.62, 1.05, false), (0.62, 1.05, true)].into_iter().enumerate() {
        let tr = Transform::from_xyz(x, 0.0, z).with_scale(Vec3::new(if flip { -1.0 } else { 1.0 }, h * 0.98, 1.0));
        leaves[k] = commands.spawn((Mesh3d(hm.leaf.clone()), MeshMaterial3d(w.clone()), tr, ChildOf(root))).id();
    }
    commands.entity(root).insert(LockVis { leaves, water, low, high, floor, top, k: 0.0, up: true, looping });
    root
}

fn smooth(a: f32, b: f32, x: f32) -> f32 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// A passage: the entry gates open, close behind, the chamber fills (or drains) to the far
/// side's level, the exit gates open, then close. At rest both pairs stand shut and the
/// chamber stands at the low side's level.
pub(super) fn animate_locks(time: Res<Time>, mut locks: Query<&mut LockVis>, mut parts: Query<&mut Transform, Without<LockVis>>) {
    let dt = time.delta_secs();
    for mut l in &mut locks {
        if l.looping {
            l.k = (l.k + dt / PASSAGE_S).rem_euclid(1.0);
            if l.k < dt / PASSAGE_S {
                l.up = !l.up;
            }
        } else if l.k > 0.0 {
            l.k = (l.k + dt / PASSAGE_S).min(1.0);
            if l.k >= 1.0 {
                l.k = 0.0;
            }
        }
        let k = l.k;
        let entry_open = smooth(0.0, 0.12, k) * (1.0 - smooth(0.2, 0.3, k));
        let fill = smooth(0.32, 0.62, k);
        let exit_open = smooth(0.66, 0.78, k) * (1.0 - smooth(0.86, 0.98, k));
        let (from, to) = if l.up { (l.low, l.high) } else { (l.high, l.low) };
        let level = if k == 0.0 { l.low } else if k < 0.32 { from } else { from + (to - from) * fill };
        // Leaves 0 and 1 are at the low end (-z), 2 and 3 at the high end.
        let (low_open, high_open) = if l.up { (entry_open, exit_open) } else { (exit_open, entry_open) };
        for (i, &e) in l.leaves.iter().enumerate() {
            let open = if i < 2 { low_open } else { high_open };
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
            t.translation.y = (level - l.floor).clamp(0.02, l.top - l.floor);
        }
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

/// Draw each built lock, and start a passage when a beaver is in it.
pub(super) fn sync_locks(
    mut commands: Commands,
    sim: Res<Sim>,
    models: Option<Res<Models>>,
    hm: Option<Res<HarborMeshes>>,
    mut st: Local<LockSync>,
    mut q: Query<&mut LockVis>,
) {
    let (Some(m), Some(hm)) = (models, hm) else { return };
    let s = &sim.0;
    let built: Vec<&sim::locks::Lock> = s.locks.at.values().filter(|l| l.built).collect();
    st.drawn.retain(|c, e| {
        let keep = built.iter().any(|l| l.cell == *c);
        if !keep {
            commands.entity(*e).despawn();
        }
        keep
    });
    for l in built {
        let Some((yaw, low, high, floor, top)) = lock_frame(s, l) else { continue };
        match st.drawn.get(&l.cell) {
            Some(&e) => {
                if let Ok(mut v) = q.get_mut(e) {
                    (v.low, v.high) = (low, high);
                    // A beaver in the lock: a passage, the way its route goes on.
                    let inside = s.agents.by_id.values().find(|a| a.cell == l.cell);
                    if let (Some(a), 0.0) = (inside, v.k) {
                        let next = a.path.as_ref().and_then(|p| p.steps.first()).map(|x| x.0);
                        let up = next.is_none_or(|n| {
                            let d = Vec2::new(n.0 as f32 - l.cell.0 as f32, n.1 as f32 - l.cell.1 as f32);
                            d.dot(Vec2::new(yaw.sin(), yaw.cos())) >= 0.0
                        });
                        v.up = up;
                        v.k = 1e-4;
                    }
                }
            }
            None => {
                let xz = cell_xz(s, (l.cell.0 as f32 + 0.5, l.cell.1 as f32 + 0.5));
                let at = Vec3::new(xz.x, height_at(&s.height, xz.x, xz.y), xz.y);
                let e = spawn_lock(&mut commands, &hm, &m, at, yaw, floor, top, low, high, false);
                st.drawn.insert(l.cell, e);
            }
        }
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
    spawn_lock(commands, hm, m, Vec3::new(x, g, z), std::f32::consts::FRAC_PI_2, g - 0.2, g + 1.3, g + 0.25, g + 1.05, true);
}
