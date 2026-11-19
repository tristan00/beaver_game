use super::scene::{height_at, Pose};
use super::sim_sync::*;
use super::sim_view::DAM_TOP;
use super::BeaverSprite;
use crate::state::Sim;
use bevy::prelude::*;
use sim::agents::{Assignment, Task};
use sim::buildings::{Building, BuildingId, BuildingKind, Good, Status};
use sim::colony::BeaverId;
use sim::nav::Surface;
use sim::resources::{ResourceKind, FELL_WORK};
use std::collections::BTreeSet;

const DT: f32 = sim::STEP_DAYS;

fn scene_sim() -> (sim::SimState, Vec<BeaverId>) {
    let mut sim = sim::SimState::new(3);
    sim.resources = Default::default();
    let spot = (0..sim.world.size)
        .map(|i| (i as f32, 100.0))
        .find(|&(x, y)| !sim::colony::Landscape::is_water(&sim::WorldLand { world: &sim.world, depth: &sim.water.depth }, x, y))
        .expect("a dry cell");
    let land = sim::WorldLand { world: &sim.world, depth: &sim.water.depth };
    sim.colony.found(spot, &land).unwrap();
    let ids: Vec<BeaverId> = sim.colony.beavers.iter().map(|b| b.id).collect();
    let mut dam = Vec::new();
    for y in 60..sim.world.size.min(160) {
        for x in 3..60 {
            if (20..22).contains(&x) {
                *sim.water.depth.get_mut(x, y).unwrap() = 1.0;
            } else if x == 26 {
                dam.push((x, y));
            } else if *sim.water.depth.get(x, y).unwrap() > 0.0 {
                *sim.water.depth.get_mut(x, y).unwrap() = 0.0;
            }
        }
    }
    sim.water.build_dam(&dam, 1.0).unwrap();
    sim.buildings.buildings.push(Building { id: BuildingId(1), kind: BuildingKind::Warehouse, pos: (32, 100), status: Status::Complete, recipe: None, output: 0.0 });
    (sim, ids)
}

fn app_with(sim: sim::SimState) -> App {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, TransformPlugin))
        .insert_resource(crate::cli::Cli::parse(Vec::<String>::new()).unwrap())
        .insert_resource(Sim(sim))
        .init_resource::<PendingDays>()
        .add_systems(Update, sync_sim_beavers);
    app
}

fn expected_pose(sim: &sim::SimState, ag: &sim::agents::Agent) -> Pose {
    use sim::nav::Gait;
    let moving = !ag.trail.is_empty() || ag.path.as_ref().is_some_and(|p| !p.steps.is_empty());
    if moving {
        return match (super::sim_view::moving_gait(ag), ag.load.is_some()) {
            (Gait::Swim, _) => Pose::Swim,
            (Gait::Climb, _) => Pose::Climb,
            (Gait::Walk, true) => Pose::Carry,
            (Gait::Walk, false) => Pose::Walk,
        };
    }
    if sim.nav.surface_at(ag.cell) == Some(Surface::Water) {
        return Pose::Swim;
    }
    match &ag.task {
        Task::Collect { target, .. } if sim.resources.get(*target).is_some_and(|r| r.kind == ResourceKind::Tree) => Pose::Gnaw,
        Task::Collect { .. } | Task::Build { .. } => Pose::Work,
        Task::Rest => Pose::Rest,
        _ => Pose::Idle,
    }
}

#[test]
#[ignore = "beavers: off until the water engine is done"]
fn sprites_match_sim_positions_and_task_poses_while_stepping() {
    let (mut sim, ids) = scene_sim();
    let tree = sim.resources.add(ResourceKind::Tree, (10, 100), FELL_WORK);
    sim::agents::spawn_on(&mut sim, ids[0], (8, 100)).unwrap();
    sim::agents::spawn_on(&mut sim, ids[1], (8, 106)).unwrap();
    sim::agents::spawn_on(&mut sim, ids[2], (6, 110)).unwrap();
    sim.apply(sim::Command::Assign { who: vec![ids[0]], to: Some(Assignment::Resource(tree)) }).unwrap();
    let site = BuildingId(2);
    sim.buildings.buildings.push(Building { id: site, kind: BuildingKind::Burrow, pos: (11, 106), status: Status::Site, recipe: None, output: 0.0 });
    let spec = BuildingKind::Burrow.spec();
    sim.work.add_site(site, BuildingKind::Burrow, spec.cost, spec.work).unwrap();
    for &(g, a) in spec.cost {
        sim.work.deliver(site, g, a).unwrap();
    }
    sim.apply(sim::Command::FillSlot { building: site, slot: 0, who: ids[1] }).unwrap();

    let mut app = app_with(sim);
    let mut seen: BTreeSet<String> = BTreeSet::new();
    let mut loads = BTreeSet::new();
    let mut checked = 0;
    for i in 0..(60.0 / DT) as usize {
        sim::agents::step(&mut app.world_mut().resource_mut::<Sim>().0, DT);
        app.update();
        let mut q = app.world_mut().query::<(&BeaverSprite, &GlobalTransform, &Visibility)>();
        let sprites: Vec<(BeaverSprite, Vec3, Visibility)> = q.iter(app.world()).map(|(s, g, v)| (s.clone(), g.translation(), *v)).collect();
        let sim = &app.world().resource::<Sim>().0;
        assert_eq!(sprites.len(), sim.agents.by_id.len(), "one sprite per agent");
        for (s, pos, vis) in sprites {
            let ag = sim.agents.get(BeaverId(s.id)).expect("sprite has an agent");
            let (cx, cy) = (ag.cell.0 as f32 + 0.5, ag.cell.1 as f32 + 0.5);
            let (nx, ny, f) = match ag.path.as_ref().and_then(|p| p.steps.first()) {
                Some(&(n, _)) => (n.0 as f32 + 0.5, n.1 as f32 + 0.5, ag.step_frac.clamp(0.0, 0.98)),
                None => (cx, cy, 0.0),
            };
            let (w, h) = (sim.height.width() as f32, sim.height.height() as f32);
            let want = Vec2::new(cx + (nx - cx) * f - w / 2.0, cy + (ny - cy) * f - h / 2.0);
            assert!((pos.x - want.x).abs() < 1e-4 && (pos.z - want.y).abs() < 1e-4, "step {i}: sprite {:?} vs sim {:?}", (pos.x, pos.z), want);
            let ground = height_at(&sim.height, pos.x, pos.z);
            assert!(pos.y >= ground - 1e-3 && pos.y <= ground + 1.0 + DAM_TOP.max(sim::dam_preview::MAX_DAM_HEIGHT) + 1e-3, "step {i}: y {} vs ground {ground}", pos.y);
            assert_eq!(s.pose, expected_pose(sim, ag), "step {i}: task {:?} gait {:?}", ag.task, ag.gait);
            assert_eq!(s.load.map(|l| l.0), ag.load.map(|l| l.good));
            assert_eq!(vis == Visibility::Hidden, ag.inside);
            seen.insert(format!("{:?}", s.pose));
            if let Some(l) = s.load {
                loads.insert((format!("{:?}", s.pose), l.0.index()));
            }
            checked += 1;
        }
    }
    for p in ["Walk", "Swim", "Climb", "Carry", "Gnaw", "Work", "Idle"] {
        assert!(seen.contains(p), "pose {p} never seen; saw {seen:?}");
    }
    assert!(loads.contains(&("Swim".to_string(), Good::Logs.index())), "a log was carried through the water: {loads:?}");
    assert!(checked > 500);
}

#[test]
#[ignore = "beavers: off until the water engine is done"]
fn rest_pose_and_despawn_follow_the_sim() {
    let (mut sim, ids) = scene_sim();
    sim::agents::spawn_on(&mut sim, ids[0], (8, 100)).unwrap();
    sim.agents.by_id.get_mut(&ids[0]).unwrap().task = Task::Rest;
    let mut app = app_with(sim);
    app.update();
    let poses = |app: &mut App| app.world_mut().query::<&BeaverSprite>().iter(app.world()).map(|s| s.pose).collect::<Vec<_>>();
    assert_eq!(poses(&mut app), vec![Pose::Rest]);
    app.world_mut().resource_mut::<Sim>().0.agents.by_id.clear();
    app.update();
    assert!(poses(&mut app).is_empty());
}

#[test]
#[ignore = "beavers: off until the water engine is done"]
fn ripples_only_on_beavers_over_water() {
    use super::sim_view::{is_wet, pose_of};
    use super::shows_ripple;
    use sim::nav::{Gait, Path};
    let (mut sim, ids) = scene_sim();
    let mut nav = sim.nav.clone();
    nav.rebuild(&sim);
    sim.nav = nav;
    let (land, water) = ((19, 100), (20, 100));
    assert_eq!(sim.nav.surface_at(land), Some(Surface::Land));
    assert_eq!(sim.nav.surface_at(water), Some(Surface::Water));
    let mut ag = sim.agents.get(ids[0]).cloned().unwrap_or_else(|| {
        sim::agents::step(&mut sim, DT);
        sim.agents.get(ids[0]).expect("agent spawned").clone()
    });
    let check = |ag: &sim::agents::Agent, wet: bool| {
        assert_eq!(is_wet(&sim, ag), wet, "cell {:?} frac {} path {:?}", ag.cell, ag.step_frac, ag.path);
        assert_eq!(shows_ripple(pose_of(&sim, ag), is_wet(&sim, ag)), wet);
    };

    ag.cell = land;
    ag.path = None;
    ag.gait = Gait::Walk;
    ag.step_frac = 0.0;
    check(&ag, false);
    ag.path = Some(Path { steps: vec![((19, 101), Gait::Walk)], cost: 1.0 });
    ag.step_frac = 0.7;
    check(&ag, false);

    ag.path = Some(Path { steps: vec![(water, Gait::Swim)], cost: 1.0 });
    ag.gait = Gait::Swim;
    for frac in [0.0, 0.2, 0.49] {
        ag.step_frac = frac;
        assert_eq!(pose_of(&sim, &ag), Pose::Swim);
        check(&ag, false);
    }
    ag.step_frac = 0.5;
    check(&ag, true);

    ag.cell = water;
    ag.step_frac = 0.0;
    ag.path = Some(Path { steps: vec![((21, 100), Gait::Swim)], cost: 1.0 });
    check(&ag, true);
    ag.path = None;
    check(&ag, true);

    assert!(!shows_ripple(Pose::Swim, false));
    assert!(!shows_ripple(Pose::Walk, true));
    assert!(shows_ripple(Pose::Swim, true));
}

#[test]
fn a_barge_glides_between_cells_and_faces_its_way() {
    let a = Vec3::new(0.0, 1.0, 0.0);
    let mut v = BargeVis { id: 1, cell: (0, 0), from: a, to: a, t0: 0.0, dur: 0.0, yaw: 0.0, loaded: false };
    assert_eq!(glide(&v, 5.0), a, "a still barge stays put");
    retarget(&mut v, (1, 0), Vec3::new(1.0, 1.0, 0.0), 0.1);
    assert!((v.dur - 0.1).abs() < 1e-4, "{}", v.dur);
    assert_eq!(glide(&v, 0.1), a);
    assert!((glide(&v, 0.15).x - 0.5).abs() < 1e-3, "halfway");
    assert_eq!(glide(&v, 1.0), Vec3::new(1.0, 1.0, 0.0), "arrives and stops");
    assert!((v.yaw - std::f32::consts::FRAC_PI_2).abs() < 1e-4, "bow (+Z) turned to +X: {}", v.yaw);
    let mut w = v.clone();
    retarget(&mut w, (1, 1), Vec3::new(1.0, 1.0, 1.0), 0.15);
    assert!((w.from.x - 0.5).abs() < 1e-3);
}

fn walk_steps(time: Res<Time>, mut clock: ResMut<crate::sim_driver::SimClock>, mut sim: ResMut<Sim>, mut walked: Local<f32>) {
    for _ in 0..clock.advance(time.delta_secs()) {
        let s = &mut sim.0;
        s.time_days += sim::STEP_DAYS as f64;
        let before = walked.floor() as usize;
        *walked = (*walked + sim::agents::MOVE_COST_PER_DAY * sim::STEP_DAYS).min(100.0);
        let (from, done) = ((10usize, 14usize), *walked);
        let ag = s.agents.by_id.values_mut().next().expect("a beaver");
        ag.trail = (before + 1..=done.floor() as usize).map(|k| ((from.0, from.1 + k), sim::nav::Gait::Walk)).collect();
        ag.cell = (from.0, from.1 + done.floor() as usize);
        ag.step_frac = done.fract();
        ag.gait = sim::nav::Gait::Walk;
        let left = 100 - done.floor() as usize;
        let steps: Vec<_> = (1..=left).map(|k| ((ag.cell.0, ag.cell.1 + k), sim::nav::Gait::Walk)).collect();
        ag.path = (!steps.is_empty()).then(|| sim::nav::Path { cost: steps.len() as f32, steps });
    }
}

#[test]
#[ignore = "beavers: off until the water engine is done"]
fn a_walking_beaver_moves_smoothly_frame_to_frame() {
    let (mut sim, _) = scene_sim();
    sim.agents = Default::default();
    sim.agents.spawn(BeaverId(1), (10, 14));
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, TransformPlugin))
        .insert_resource(bevy::time::TimeUpdateStrategy::ManualDuration(std::time::Duration::from_micros(16_667)))
        .insert_resource(crate::cli::Cli::parse(Vec::<String>::new()).unwrap())
        .insert_resource(Sim(sim))
        .init_resource::<crate::sim_driver::SimClock>()
        .init_resource::<PendingDays>()
        .add_systems(Update, (walk_steps, update_pending, sync_sim_beavers).chain());
    let mut track = Vec::new();
    for _ in 0..480 {
        app.update();
        let mut q = app.world_mut().query::<(&BeaverSprite, &Transform)>();
        if let Some((_, t)) = q.iter(app.world()).next() {
            track.push(t.translation.xz());
        }
    }
    let steps: Vec<f32> = track.windows(2).map(|w| w[0].distance(w[1])).collect();
    let first = steps.iter().position(|d| *d > 1e-4).expect("the beaver walked");
    let last = steps.iter().rposition(|d| *d > 1e-4).unwrap();
    let walk = &steps[first + 8..last.saturating_sub(8)];
    let mut sorted = walk.to_vec();
    sorted.sort_by(|a, b| a.total_cmp(b));
    let median = sorted[sorted.len() / 2];
    let stalls = walk.iter().filter(|d| **d < median * 0.5).count();
    let jumps = walk.iter().filter(|d| **d > median * 1.5).count();
    println!("walk frames {} median step {median:.3} stalls {stalls} jumps {jumps} steps {:?}", walk.len(), &walk[..walk.len().min(40)]);
    assert!(walk.len() > 20, "the beaver walked");
    assert_eq!((stalls, jumps), (0, 0), "every frame of the walk moves about the same distance");
}
