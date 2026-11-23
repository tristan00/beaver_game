//! A model gallery for iterating on beavers and buildings: `BC_GALLERY=dist,yaw,pitch[,dz,dx]`
//! lines up every building stage and every beaver pose on the flattest dry ground of the map
//! and aims the camera there. `BC_SELECT_HOME=1` opens the first family's inspector. `BC_GALLERY_WHAT=build|beaver|all` picks the rows and
//! `BC_GALLERY_POSE=n` shows a single pose (`BC_GALLERY_LOAD=n` gives every pose that load). See `tools/gallery_shot.sh`.
use super::scene::{height_at, Pose, StructKind};
use super::{models, BeaverSprite};
use bevy::prelude::*;

/// A gallery beaver: where it stands, and whether it paces back and forth so its gait runs.
#[derive(Component)]
pub(super) struct GalleryBeaver(Vec3, bool);

const POSES: [Pose; 10] = [Pose::Idle, Pose::Walk, Pose::Carry, Pose::Gnaw, Pose::Work, Pose::Dig, Pose::Forage, Pose::Climb, Pose::Rest, Pose::Swim];

pub(super) fn spawn_gallery(
    mut commands: Commands,
    sim: Res<crate::state::Sim>,
    models: Option<Res<super::Models>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut orbit: ResMut<crate::camera::OrbitCamera>,
    mut done: Local<bool>,
) {
    let Some(m) = models else { return };
    let Ok(spec) = std::env::var("BC_GALLERY") else { return };
    let s = &sim.0;
    if *done || s.height.width() < 80 {
        return;
    }
    *done = true;
    let (w, h) = (s.height.width(), s.height.height());
    let mut best = (f32::MAX, (w / 2, h / 2));
    for y in (20..h - 50).step_by(3) {
        for x in (20..w - 50).step_by(3) {
            let (mut lo, mut hi, mut wet) = (f32::MAX, f32::MIN, false);
            for dy in 0..26 {
                for dx in 0..40 {
                    let v = s.height.get(x + dx, y + dy).copied().unwrap_or(0.0);
                    lo = lo.min(v);
                    hi = hi.max(v);
                    wet |= s.water.depth.get(x + dx, y + dy).is_some_and(|d| *d > 0.001);
                }
            }
            if !wet && hi - lo < best.0 {
                best = (hi - lo, (x + 20, y + 13));
            }
        }
    }
    let (cx, cz) = crate::terrain_render::world_pos(s, best.1 .0 as i32, best.1 .1 as i32);
    let ground = |x: f32, z: f32| height_at(&s.height, x, z);
    let cam: Vec<f32> = spec.split(',').filter_map(|t| t.trim().parse().ok()).collect();
    orbit.target = Vec3::new(cx + cam.get(4).copied().unwrap_or(0.0), ground(cx, cz), cz + cam.get(3).copied().unwrap_or(0.0));
    orbit.distance = cam.first().copied().unwrap_or(20.0);
    orbit.yaw = cam.get(1).copied().unwrap_or(0.0);
    orbit.pitch = cam.get(2).copied().unwrap_or(0.75);
    let what = std::env::var("BC_GALLERY_WHAT").unwrap_or_else(|_| "all".into());
    let white = m.white.clone();
    if what == "all" || what == "build" {
        let stages: Vec<u8> = (0..=models::STAGES).chain([models::DRY_STAGE]).collect();
        for (row, kind) in [StructKind::Lodge, StructKind::Warehouse, StructKind::Larder, StructKind::Toolshed, StructKind::Forester, StructKind::Logistics].into_iter().enumerate() {
            for (i, &stage) in stages.iter().enumerate() {
                let (x, z) = (cx + (i as f32 - stages.len() as f32 / 2.0) * 4.0, cz - 3.0 - row as f32 * 4.5);
                let base = (0..9).map(|k| ground(x + (k % 3) as f32 - 1.0, z + (k / 3) as f32 - 1.0)).fold(f32::MIN, f32::max);
                let mesh = meshes.add(models::struct_mesh(kind, 1.0, stage));
                commands.spawn((Mesh3d(mesh), MeshMaterial3d(white.clone()), Transform::from_xyz(x, base, z).with_scale(Vec3::splat(super::sim_sync::BUILDING_SIZE))));
            }
        }
        use super::structures::{dam_cell_mesh, DamStage};
        for (k, st) in [DamStage::Staked, DamStage::Rising, DamStage::Done].into_iter().enumerate() {
            for i in 0..4u8 {
                let (x, z) = (cx - 8.0 + k as f32 * 6.0 + i as f32, cz - 31.0);
                let mesh = meshes.add(dam_cell_mesh(4 + (i % 2) * 2, 1.35, i as u32, st));
                commands.spawn((Mesh3d(mesh), MeshMaterial3d(white.clone()), Transform::from_xyz(x, ground(x, z), z)));
            }
        }
    }
    if what == "all" || what == "beaver" {
        let only: Option<usize> = std::env::var("BC_GALLERY_POSE").ok().and_then(|v| v.parse().ok());
        let yaw = std::env::var("BC_GALLERY_YAW").ok().and_then(|v| v.parse().ok()).unwrap_or(0.0);
        let mut spawn = |id: u64, pose: Pose, load: Option<sim::buildings::Good>, x: f32, z: f32, size: f32| {
            let base = Vec3::new(x, ground(x, z), z);
            let paces = matches!(pose, Pose::Walk | Pose::Carry | Pose::Climb | Pose::Swim);
            commands.spawn((
                BeaverSprite { id, pose, load: load.map(|g| (g, sim::agents::load_size(g))), yaw, wet: pose == Pose::Swim },
                GalleryBeaver(base, paces),
                Transform::from_translation(base).with_scale(Vec3::splat(super::sim_sync::BEAVER_SIZE * size)),
                Visibility::Inherited,
            ));
        };
        for (i, pose) in POSES.into_iter().enumerate() {
            if only.is_some_and(|o| o != i) {
                continue;
            }
            let x = if only.is_some() { cx } else { cx + (i as f32 - POSES.len() as f32 / 2.0) * 1.4 };
            let good = std::env::var("BC_GALLERY_LOAD").ok().and_then(|v| v.parse::<usize>().ok()).and_then(|i| sim::buildings::Good::ALL.get(i).copied());
            // With a load picked, every pose carries it; otherwise only the carrier does.
            let load = if pose == Pose::Carry { Some(good.unwrap_or(sim::buildings::Good::Sticks)) } else { good };
            spawn(i as u64 + 1, pose, load, x, cz + 1.5, 1.0);
        }
        if only.is_none() {
            for (i, g) in sim::buildings::Good::ALL.into_iter().enumerate() {
                spawn(20 + i as u64, Pose::Carry, Some(g), cx + (i as f32 - 1.0) * 1.6, cz + 4.0, 1.0);
            }
            for (i, stage) in [sim::colony::Stage::Kit, sim::colony::Stage::Yearling, sim::colony::Stage::Adult].into_iter().enumerate() {
                spawn(30 + i as u64, Pose::Idle, None, cx + 4.0 + i as f32 * 1.4, cz + 4.0, super::age_scale(stage));
            }
        }
    }
}

/// Pacing poses run back and forth along their facing at full walking speed, so the gait
/// (and its switch to a bound) shows as it does in play.
pub(super) fn shuffle_gallery(time: Res<Time>, mut q: Query<(&mut Transform, &GalleryBeaver, &BeaverSprite)>) {
    let speed: f32 = std::env::var("BC_GALLERY_SPEED").ok().and_then(|v| v.parse().ok()).unwrap_or(1.0);
    for (mut tr, g, s) in &mut q {
        if !g.1 {
            continue;
        }
        let fwd = Vec3::new(s.yaw.sin(), 0.0, s.yaw.cos());
        tr.translation = g.0 + fwd * (time.elapsed_secs() * speed).rem_euclid(2.0);
    }
}

/// `BC_SELECT_HOME=1`: keep the first family's home selected, for screenshots of its inspector.
pub(super) fn select_home(sim: Res<crate::state::Sim>, sel: Option<ResMut<crate::flow::Selection>>) {
    if std::env::var_os("BC_SELECT_HOME").is_none() {
        return;
    }
    let (Some(mut sel), Some(l)) = (sel, sim.0.colony.lodges.first()) else { return };
    let want = match crate::picking::Target::home(&sim.0, l.id) {
        crate::picking::Target::Building(b) => crate::flow::Selection::Site(b),
        _ => crate::flow::Selection::Lodge(l.id),
    };
    if *sel != want {
        *sel = want;
    }
}
