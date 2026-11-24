//! A model gallery for iterating on beavers and buildings: `BC_GALLERY=dist,yaw,pitch[,dz,dx]`
//! lines up every building stage and every beaver pose on the flattest dry ground of the map
//! and aims the camera there. `BC_SELECT_HOME=1` opens the first family's inspector. `BC_GALLERY_WHAT=build|beaver|wolf|wild|life|harbor|all` picks the rows and
//! `BC_GALLERY_POSE=n` shows a single pose (`BC_GALLERY_LOAD=n` gives every pose that load).
//! `BC_GALLERY_WHAT=life` lines up the life poses (running, handling, mending, watch, courting,
//! grooming, caught, dead) and, behind, every look (hungry, cold, hurt, worn out, old, unhappy,
//! pregnant, a tail slap, a dive), the one-off ones on a loop. See `tools/gallery_shot.sh`.
use super::scene::{height_at, Pose, StructKind};
use super::{models, BeaverSprite};
use bevy::prelude::*;

/// A gallery beaver: where it stands, and whether it paces back and forth so its gait runs.
#[derive(Component)]
pub(super) struct GalleryBeaver(Vec3, bool);

/// A one-off shown again and again in the gallery: a fall, a tail slap, a dive.
#[derive(Component, Clone, Copy, PartialEq)]
pub(super) enum GalleryLoop {
    Fall,
    Slap,
    Dive,
}

const POSES: [Pose; 10] = [Pose::Idle, Pose::Walk, Pose::Carry, Pose::Gnaw, Pose::Work, Pose::Dig, Pose::Forage, Pose::Climb, Pose::Rest, Pose::Swim];

pub(super) fn spawn_gallery(
    mut commands: Commands,
    sim: Res<crate::state::Sim>,
    models: Option<Res<super::Models>>,
    wolf_meshes: Option<Res<super::wolf::WolfMeshes>>,
    wild_meshes: Option<Res<super::wildlife::WildMeshes>>,
    harbor_meshes: Option<Res<super::harbor::HarborMeshes>>,
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
        for (row, kind) in [StructKind::Lodge, StructKind::Warehouse, StructKind::Larder, StructKind::Toolshed, StructKind::Forester, StructKind::Logistics, StructKind::Lookout, StructKind::Harbor].into_iter().enumerate() {
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
                let (x, z) = (cx - 8.0 + k as f32 * 6.0 + i as f32, cz - 40.0);
                let mesh = meshes.add(dam_cell_mesh(4 + (i % 2) * 2, 1.35, i as u32, st));
                commands.spawn((Mesh3d(mesh), MeshMaterial3d(white.clone()), Transform::from_xyz(x, ground(x, z), z)));
            }
        }
    }
    // Every species (across) in spring, summer, autumn and winter (rows, nearest first).
    if what == "trees" {
        let tree = super::sim_sync::TREE_SCALE;
        for (row, phase) in [0.125f32, 0.375, 0.6, 0.875].into_iter().enumerate() {
            for (i, sp) in super::scene::Species::ALL.into_iter().enumerate() {
                let (x, z) = (cx + (i as f32 - 3.5) * 9.0, cz - row as f32 * 11.0);
                let mesh = m.tree(sp, models::Season::at(phase), 0);
                commands.spawn((Mesh3d(mesh), MeshMaterial3d(m.tree_tints[0].clone()), Transform::from_xyz(x, ground(x, z) - 0.05, z).with_scale(Vec3::splat(tree))));
            }
        }
    }
    // Food plants (across) in spring, summer, autumn and winter (rows, nearest first); behind
    // them the boulders, pebble heaps and gravel bars by quarters left, and the two clays.
    if what == "res" {
        let place = |commands: &mut Commands, mesh: Handle<Mesh>, mat: &Handle<StandardMaterial>, x: f32, z: f32, s: f32| {
            commands.spawn((Mesh3d(mesh), MeshMaterial3d(mat.clone()), Transform::from_xyz(x, ground(x, z), z).with_scale(Vec3::splat(s))));
        };
        for (row, phase) in [0.125f32, 0.4, 0.62, 0.875].into_iter().enumerate() {
            for (i, p) in sim::plants::Plant::ALL.into_iter().enumerate() {
                let (x, z) = (cx + (i as f32 - 5.0) * 1.3, cz - row as f32 * 1.6);
                place(&mut commands, m.plant(p, models::Season::at(phase)), &white, x, z, 1.0);
            }
        }
        for (i, b) in m.boulders.iter().enumerate() {
            place(&mut commands, b.clone(), &white, cx + (i as f32 - 3.5) * 1.8, cz - 8.5, 1.0 + 0.1 * (i % 3) as f32);
        }
        for q in 0..4 {
            place(&mut commands, m.pebble_heaps[q].clone(), &white, cx - 6.0 + q as f32 * 1.4, cz - 11.0, 1.0);
            place(&mut commands, m.gravel_bars[q].clone(), &white, cx + 1.0 + q as f32 * 1.4, cz - 11.0, 1.0);
        }
        place(&mut commands, m.clay_heap.clone(), &m.wet, cx - 2.0, cz - 13.0, 1.0);
        place(&mut commands, m.river_clay.clone(), &m.wet, cx + 1.0, cz - 13.0, 1.0);
    }
    if let (true, Some(hm)) = (what == "harbor", harbor_meshes.as_deref()) {
        super::harbor::spawn_gallery(&mut commands, hm, &m, &mut meshes, Vec3::new(cx, 0.0, cz), ground);
    }
    if let (true, Some(wm)) = (what == "wild", wild_meshes.as_deref()) {
        super::wildlife::spawn_gallery(&mut commands, wm, &m, Vec3::new(cx, 0.0, cz), ground);
    }
    if let (true, Some(wm)) = (what == "wolf", wolf_meshes.as_deref()) {
        super::wolf::spawn_gallery_wolves(&mut commands, wm, &m, Vec3::new(cx, 0.0, cz), ground);
    }
    if what == "life" {
        use super::Look;
        let yaw = std::env::var("BC_GALLERY_YAW").ok().and_then(|v| v.parse().ok()).unwrap_or(0.0f32);
        // `BC_GALLERY_POSE=n` shows entry n alone at the centre (a pair counts as one).
        let only: Option<u64> = std::env::var("BC_GALLERY_POSE").ok().and_then(|v| v.parse().ok());
        let entry = |id: u64| if id >= 140 { id - 140 + 9 } else if id >= 120 { 7 + (id - 120) / 2 } else { id - 100 };
        let mut put = |id: u64, pose: Pose, look: Look, x: f32, z: f32, yaw: f32, paces: bool, lp: Option<GalleryLoop>| {
            let (x, z) = match only {
                Some(o) if o != entry(id) => return,
                Some(_) => (cx + if (120..140).contains(&id) { if id % 2 == 0 { -0.45 } else { 0.45 } } else { 0.0 }, cz),
                None => (x, z),
            };
            let base = Vec3::new(x, ground(x, z), z);
            let mut e = commands.spawn((
                BeaverSprite { id, pose, load: None, yaw, wet: pose == Pose::Swim, look, features: Default::default() },
                GalleryBeaver(base, paces),
                Transform::from_translation(base).with_scale(Vec3::splat(super::sim_sync::BEAVER_SIZE)),
                Visibility::Inherited,
            ));
            if let Some(l) = lp {
                e.insert(l);
            }
        };
        let d = Look::default();
        let step = 1.6;
        let row = [
            (Pose::Run, d, true, None),
            (Pose::Handle, d, false, None),
            (Pose::Mend, d, false, None),
            (Pose::Watch, d, false, None),
            (Pose::Caught, d, false, None),
            (Pose::Dead, Look { dead: Some((0.0, false)), ..d }, false, Some(GalleryLoop::Fall)),
            (Pose::Dead, Look { dead: Some((0.0, true)), ..d }, false, Some(GalleryLoop::Fall)),
        ];
        for (i, (pose, look, paces, lp)) in row.into_iter().enumerate() {
            put(100 + i as u64, pose, look, cx + (i as f32 - 3.0) * step, cz + 1.5, yaw, paces, lp);
        }
        // Pairs facing each other: courting, and grooming.
        for (k, pose) in [Pose::Court, Pose::Groom].into_iter().enumerate() {
            let x = cx + (k as f32 * 2.0 - 1.0) * 2.4;
            let z = cz - 1.5;
            put(120 + 2 * k as u64, pose, d, x - 0.45, z, std::f32::consts::FRAC_PI_2, false, None);
            put(121 + 2 * k as u64, pose, d, x + 0.45, z, -std::f32::consts::FRAC_PI_2, false, None);
        }
        let looks = [
            (Pose::Walk, Look { thin: 1.0, ..d }, true, None),
            (Pose::Idle, Look { cold: 1.0, ..d }, false, None),
            (Pose::Walk, Look { hurt: 1.0, ..d }, true, None),
            (Pose::Idle, Look { tired: 1.0, ..d }, false, None),
            (Pose::Idle, Look { old: 1.0, ..d }, false, None),
            (Pose::Walk, Look { sad: 1.0, ..d }, true, None),
            (Pose::Idle, Look { belly: 1.0, ..d }, false, None),
            (Pose::Idle, d, false, Some(GalleryLoop::Slap)),
            (Pose::Idle, d, false, Some(GalleryLoop::Dive)),
        ];
        for (i, (pose, look, paces, lp)) in looks.into_iter().enumerate() {
            put(140 + i as u64, pose, look, cx + (i as f32 - 4.0) * step, cz + 4.5, yaw, paces, lp);
        }
    }
    if what == "all" || what == "beaver" {
        let only: Option<usize> = std::env::var("BC_GALLERY_POSE").ok().and_then(|v| v.parse().ok());
        let yaw = std::env::var("BC_GALLERY_YAW").ok().and_then(|v| v.parse().ok()).unwrap_or(0.0);
        let mut spawn = |id: u64, pose: Pose, load: Option<sim::buildings::Good>, x: f32, z: f32, size: f32| {
            let base = Vec3::new(x, ground(x, z), z);
            let paces = matches!(pose, Pose::Walk | Pose::Carry | Pose::Climb | Pose::Swim);
            commands.spawn((
                BeaverSprite { id, pose, load: load.map(|g| (g, sim::agents::load_size(g))), yaw, wet: pose == Pose::Swim, look: Default::default(), features: Default::default() },
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
pub(super) fn shuffle_gallery(time: Res<Time>, clock: Res<super::RenderClock>, mut q: Query<(&mut Transform, &GalleryBeaver, &mut BeaverSprite, Option<&GalleryLoop>)>) {
    let speed: f32 = std::env::var("BC_GALLERY_SPEED").ok().and_then(|v| v.parse().ok()).unwrap_or(1.0);
    let t = clock.0;
    for (mut tr, g, mut s, lp) in &mut q {
        // One-offs again every few seconds.
        match lp {
            Some(GalleryLoop::Fall) => s.look.dead = s.look.dead.map(|(_, c)| (t - t.rem_euclid(3.0), c)),
            Some(GalleryLoop::Slap) => s.look.slap = t - t.rem_euclid(1.5),
            Some(GalleryLoop::Dive) => {
                let k = t.rem_euclid(3.0);
                s.look.sink = (k / 0.7).min(1.0) * (1.0 - ((k - 1.5) / 0.7).clamp(0.0, 1.0));
            }
            None => {}
        }
        if !g.1 {
            continue;
        }
        // A runner covers ground as fast as it runs.
        let pace = if s.pose == Pose::Run { 3.2 } else { 1.0 };
        let fwd = Vec3::new(s.yaw.sin(), 0.0, s.yaw.cos());
        tr.translation = g.0 + fwd * (time.elapsed_secs() * speed * pace).rem_euclid(2.0 * pace);
    }
}

/// `BC_SELECT_HOME=1`: keep the first family's home selected, for screenshots of its inspector;
/// with `BC_DETAILS=1` too, its family page is opened (`BC_SELECT_HOME=2` for the second lodge...).
pub(super) fn select_home(sim: Res<crate::state::Sim>, sel: Option<ResMut<crate::flow::Selection>>, hud: Option<ResMut<crate::hud::HudState>>) {
    if std::env::var_os("BC_SELECT_HOME").is_none() {
        return;
    }
    let nth = std::env::var("BC_SELECT_HOME").ok().and_then(|v| v.parse::<usize>().ok()).unwrap_or(1).max(1) - 1;
    let (Some(mut sel), Some(l)) = (sel, sim.0.colony.lodges.get(nth)) else { return };
    if let Some(mut hud) = hud.filter(|h| std::env::var_os("BC_DETAILS").is_some() && h.lodge_page != Some(l.id)) {
        hud.lodge_page = Some(l.id);
    }
    let want = match crate::picking::Target::home(&sim.0, l.id) {
        crate::picking::Target::Building(b) => crate::flow::Selection::Site(b),
        _ => crate::flow::Selection::Lodge(l.id),
    };
    if *sel != want {
        *sel = want;
    }
}

/// `BC_SELECT_BEAVER=1`: keep the colony's first working beaver selected (`BC_SELECT_BUILDING=id`
/// a building instead), and `BC_SELECT_LOOKOUT=1`
/// its first lookout, for screenshots of the inspector and the lookout's range.
/// `BC_SELECT_BEAVER=out` picks the first beaver out of its lodge instead, `=load` the first carrying
/// something, `=swim` the first swimming. With `BC_DETAILS=1` too, the beaver's story is opened.
/// `BC_LEDGER=page` keeps that page of the colony ledger open.
pub(super) fn select_debug(sim: Res<crate::state::Sim>, sel: Option<ResMut<crate::flow::Selection>>, mut hud: Option<ResMut<crate::hud::HudState>>) {
    // `BC_LEDGER=charts|beavers|dams|buildings|run`: that page of the ledger stays open.
    if let (Some(page), Some(h)) = (std::env::var("BC_LEDGER").ok(), hud.as_mut()) {
        use crate::hud::LedgerTab::*;
        let want = match page.as_str() { "beavers" => Beavers, "dams" => Dams, "buildings" => Buildings, "run" => Run, _ => Charts };
        if h.ledger != Some(want) {
            h.ledger = Some(want);
        }
    }
    let Some(mut sel) = sel else { return };
    let s = &sim.0;
    // `BC_SELECT_BUILDING=id`: that building (a dam, a canal) stays selected, for its panel.
    if let Some(id) = std::env::var("BC_SELECT_BUILDING").ok().and_then(|v| v.parse::<u64>().ok()) {
        let want = crate::flow::Selection::Site(sim::buildings::BuildingId(id));
        if *sel != want {
            *sel = want;
        }
        return;
    }
    let pick = std::env::var("BC_SELECT_BEAVER").ok();
    let out = |b: &&sim::colony::Beaver, f: &dyn Fn(&sim::agents::Agent) -> bool| s.agents.get(b.id).is_some_and(|a| !a.inside && f(a));
    let want = if let Some(p) = pick.as_deref().filter(|p| matches!(*p, "out" | "load" | "swim")) {
        s.colony.beavers.iter().find(|b| match p {
            "load" => out(b, &|a| a.load.is_some()),
            "swim" => out(b, &|a| a.gait == sim::nav::Gait::Swim),
            _ => out(b, &|_| true),
        }).map(|b| crate::flow::Selection::Beaver(b.id))
    } else if pick.is_some() {
        s.colony.beavers.iter().find(|b| b.can_work() && (s.work.slot_of(b.id).is_some() || s.agents.get(b.id).is_some_and(|a| a.assignment.is_some()))).map(|b| crate::flow::Selection::Beaver(b.id))
    } else if std::env::var_os("BC_SELECT_LOOKOUT").is_some() {
        s.buildings.buildings.iter().find(|b| b.kind == sim::buildings::BuildingKind::Lookout).map(|b| crate::flow::Selection::Site(b.id))
    } else {
        None
    };
    if let Some(w) = want.filter(|w| *sel != *w) {
        *sel = w;
    }
    if let (Some(mut hud), Some(crate::flow::Selection::Beaver(id))) = (hud, want) {
        if std::env::var_os("BC_DETAILS").is_some() && hud.details != Some(id) {
            hud.details = Some(id);
        }
    }
}
