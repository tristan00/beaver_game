//! Explicit diagnostic scenes used by screenshot and scripted runs.
use super::*;
use sim::buildings::Status;

#[allow(clippy::too_many_arguments)]
pub(super) fn setup(
    cli: Res<crate::cli::Cli>,
    mut done: Local<bool>,
    planned: Res<crate::state::PlannedDam>,
    mut sim: ResMut<Sim>,
    mut hud: ResMut<HudState>,
    mut sel: ResMut<Selection>,
    mut orbit: ResMut<crate::camera::OrbitCamera>,
    mut lens: ResMut<MapLens>,
    mut tips: ResMut<Tips>,
) {
    if *done || !sim.0.colony.is_founded() {
        return;
    }
    *done = true;
    let which = if cli.colony_script.is_some() { "script".to_string() } else { let Ok(v) = std::env::var("BC_HUD_DEMO") else { return }; v };
    let days: f32 = std::env::var("BC_HUD_DAYS").ok().and_then(|v| v.parse().ok()).unwrap_or(12.0);
    let s = &mut sim.0;
    let camp = planned.0.as_ref().map(|d| d.camp);
    if which == "inspect" {
        if let Some(c) = camp {
            'search: for r in 2..20usize {
                for y in c.1.saturating_sub(r)..=c.1 + r {
                    for x in c.0.saturating_sub(r)..=c.0 + r {
                        let order = BuildOrder::Building { kind: BuildingKind::Burrow, pos: (x, y) };
                        if build_logic::emit(s, &order).is_ok() {
                            break 'search;
                        }
                    }
                }
            }
        }
    }
    let look = |o: &mut crate::camera::OrbitCamera, h: &sim::grid::HeightGrid, c: (f32, f32), dist: f32| {
        o.target = cell_world(h, c);
        o.distance = dist;
        o.pitch = 0.95;
    };
    if which == "play" || which == "script" {
        let dam = planned.0.as_ref().map(|d| (d.cells.clone(), d.height));
        let mut p = sim::autoplay::Player::new(sim::autoplay::Style::Attentive, camp.unwrap_or((0, 0)), dam);
        let steps = (1.0 / crate::sim_driver::FIXED_DT_DAYS) as usize;
        for _ in 0..if which == "script" { 0 } else { days as usize } {
            p.act(s);
            for _ in 0..steps {
                let _ = s.step(crate::sim_driver::FIXED_DT_DAYS);
            }
        }
        for l in &p.log {
            println!("autoplay {l}");
        }
        let view = std::env::var("BC_HUD_VIEW").unwrap_or_default();
        match view.as_str() {
            "jobs" => hud.colony = Some(ColonyTab::Jobs),
            "pop" => hud.colony = Some(ColonyTab::Population),
            "goods" => hud.colony = Some(ColonyTab::Goods),
            "research" => hud.colony = Some(ColonyTab::Buildings),
            "log" => hud.colony = Some(ColonyTab::Log),
            "elevation" => *lens = MapLens::Elevation,
            "building" => {
                if let Some(b) = s.buildings.buildings.iter().find(|b| b.kind == BuildingKind::ForagerHut).or(s.buildings.buildings.first()) {
                    *sel = Selection::Site(b.id);
                }
            }
            "beaver" => {
                if let Some(b) = s.colony.beavers.iter().find(|b| b.stage == sim::colony::Stage::Adult) {
                    *sel = Selection::Beaver(b.id);
                    hud.selected = vec![b.id];
                }
            }
            _ => {}
        }
        let focus = std::env::var("BC_HUD_CELL").ok().and_then(|v| v.split_once(',').and_then(|(x,y)| Some((x.parse::<f32>().ok()?,y.parse::<f32>().ok()?))));
        if let Some(c) = focus.or(camp.map(|c|(c.0 as f32,c.1 as f32))) {
            let dist = std::env::var("BC_HUD_DIST").ok().and_then(|v|v.parse().ok()).unwrap_or(if view == "elevation" { 60.0 } else { 42.0 });
            look(&mut orbit, &s.height, c, dist);
        }
        return;
    }
    if which == "barge" {
        let Some(c) = camp else { return };
        let Some((a, b, barge, path)) = barge_demo(s, c) else {
            println!("barge demo: no two docks joined by water near the camp");
            return;
        };
        let steps = (days.min(3.0) / crate::sim_driver::FIXED_DT_DAYS) as usize;
        for _ in 0..steps {
            let _ = s.step(crate::sim_driver::FIXED_DT_DAYS);
        }
        let view = std::env::var("BC_HUD_VIEW").unwrap_or_default();
        let mid = path[path.len() / 2];
        match view.as_str() {
            "dock" => *sel = Selection::Site(a),
            "lock" | "raise" => {
                let i = path.len() / 2;
                let (p, q) = (path[i.saturating_sub(1)], path[i]);
                let dir = (q.0 as i64 - p.0 as i64, q.1 as i64 - p.1 as i64);
                let cells: Vec<Cell> = (-6i64..=6)
                    .map(|k| (mid.0 as i64 - dir.1 * k, mid.1 as i64 + dir.0 * k))
                    .filter(|&(x, y)| x >= 0 && y >= 0 && s.water.depth.in_bounds(x as usize, y as usize))
                    .map(|(x, y)| (x as usize, y as usize))
                    .collect();
                let _ = s.apply(sim::Command::BuildDam { cells, height: 1.0 });
                if view == "raise" {
                    *sel = Selection::Cell(mid);
                    let dist: f32 = std::env::var("BC_HUD_DIST").ok().and_then(|v| v.parse().ok()).unwrap_or(30.0);
                    look(&mut orbit, &s.height, (mid.0 as f32, mid.1 as f32), dist);
                    return;
                }
                *sel = Selection::Barge(barge);
                hud.route_draft = Some(RouteDraft { barge: Some(barge), from: Some(a), to: Some(b), goods: vec![Good::Sticks, Good::Food] });
                actions::set_route(s, &mut hud, barge, 0.0);
                println!("barge demo: route refused: {:?}", hud.route_err);
            }
            _ => *sel = Selection::Barge(barge),
        }
        let mut at = mid;
        if let Some(bg) = s.buildings.barge(barge) {
            println!("barge demo: barge at {:?}, {:?} {:?}, hold {:?}", bg.cell, bg.state, bg.leg, bg.hold);
            at = bg.cell;
        }
        let dist: f32 = std::env::var("BC_HUD_DIST").ok().and_then(|v| v.parse().ok()).unwrap_or(38.0);
        look(&mut orbit, &s.height, (at.0 as f32, at.1 as f32), dist);
        if let Some(p) = std::env::var("BC_HUD_PITCH").ok().and_then(|v| v.parse().ok()) {
            orbit.pitch = p;
        }
        return;
    }
    let mut staffed = Vec::new();
    if matches!(which.as_str(), "camp" | "ledger" | "goods" | "jobs" | "pop" | "beaver" | "research" | "tip") {
        if let Some(c) = camp {
            staffed = stand_up_camp(s, c);
        }
    }
    for _ in 0..(days / crate::sim_driver::FIXED_DT_DAYS) as usize {
        let _ = s.step(crate::sim_driver::FIXED_DT_DAYS);
    }
    let water_cat = panels::categories(s).iter().position(|g| *g == Group::Water);
    match which.as_str() {
        "dam" | "canal" => {
            if let Some(d) = planned.0.as_ref() {
                hud.open_cat = water_cat;
                hud.set_tool(if which == "dam" { Tool::Dam } else { Tool::Canal });
                let trim: usize = std::env::var("BC_HUD_DAM_TRIM").ok().and_then(|v| v.parse().ok()).unwrap_or(0).min(d.cells.len().saturating_sub(1) / 2);
                hud.line = d.cells[trim..d.cells.len() - trim].to_vec();
                let crest = std::env::var("BC_HUD_CREST").ok().and_then(|v| v.parse().ok()).unwrap_or(d.height);
                hud.dam_height = ((crest / DAM_STEP).round() * DAM_STEP).clamp(MIN_DAM_HEIGHT, MAX_DAM_HEIGHT);
                if std::env::var_os("BC_HUD_DAM_EXTEND").is_some() {
                    extend_dam(s, &mut hud);
                }
                let m = d.cells[d.cells.len() / 2];
                look(&mut orbit, &s.height, (m.0 as f32, m.1 as f32), 40.0);
            }
        }
        "place" => {
            hud.set_tool(Tool::Building(BuildingKind::Warehouse));
            if let Some(c) = camp {
                look(&mut orbit, &s.height, (c.0 as f32, c.1 as f32), 40.0);
            }
        }
        "inspect" => {
            if let Some(b) = s.buildings.buildings.first() {
                *sel = Selection::Site(b.id);
                look(&mut orbit, &s.height, (b.pos.0 as f32, b.pos.1 as f32), 30.0);
            }
        }
        "camp" => {
            if let Some(&id) = staffed.first() {
                *sel = Selection::Site(id);
                hud.details_open = std::env::var_os("BC_HUD_DETAILS").is_some();
                let b = s.buildings.building(id).map(|b| b.pos).unwrap_or((0, 0));
                look(&mut orbit, &s.height, (b.0 as f32, b.1 as f32), 32.0);
            }
        }
        "beaver" => {
            if let Some(b) = s.colony.beavers.iter().find(|b| b.stage == sim::colony::Stage::Adult) {
                *sel = Selection::Beaver(b.id);
                hud.selected = vec![b.id];
                look(&mut orbit, &s.height, b.pos, 28.0);
            }
        }
        other => {
            match other {
                "ledger" => hud.colony = Some(ColonyTab::Goods),
                "goods" => *lens = MapLens::Goods,
                "jobs" => hud.colony = Some(ColonyTab::Jobs),
                "pop" => hud.colony = Some(ColonyTab::Population),
                "research" => hud.colony = Some(ColonyTab::Buildings),
                "water" => *lens = MapLens::Water,
                "forest" => *lens = MapLens::Forest,
                "elevation" => *lens = MapLens::Elevation,
                "search" => hud.search = Some("food".into()),
                "tip" => {
                    tips.layers.push(ui::Layer {
                        src: ui::TipSrc::World(0),
                        tip: panels::good_tip(s, sim::buildings::Good::Sticks),
                        pos: Vec2::new(40.0, 52.0),
                        frozen: true,
                        opened: 0.0,
                        last_held: f32::MAX,
                    });
                }
                _ => {}
            }
            if let Some(c) = camp {
                look(&mut orbit, &s.height, (c.0 as f32, c.1 as f32), if matches!(other, "water" | "elevation") { 60.0 } else { 45.0 });
            }
        }
    }
}

fn barge_demo(s: &mut sim::SimState, camp: Cell) -> Option<(BuildingId, BuildingId, BargeId, Vec<Cell>)> {
    s.sandbox = true;
    let n = s.world.size;
    let terrain_ok = |s: &sim::SimState, p: Cell| s.check_site(BuildingKind::Dock, p).is_ok();
    let probe = |p: Cell| sim::buildings::Building { id: BuildingId(0), kind: BuildingKind::Dock, pos: p, status: Status::Complete, output: 0.0, condition: 1.0, reinforced: false, dam_profile: Vec::new() };
    let locks = sim::shipping::lock_cells(s);
    let near = |r: usize| (camp.1.saturating_sub(r)..(camp.1 + r).min(n - 2)).flat_map(move |y| (camp.0.saturating_sub(r)..(camp.0 + r).min(n - 2)).map(move |x| (x, y)));
    let first = near(30)
        .filter(|&p| terrain_ok(s, p) && !sim::shipping::berths(s, &locks, &probe(p)).is_empty())
        .min_by_key(|p| p.0.abs_diff(camp.0) + p.1.abs_diff(camp.1))?;
    let start = sim::shipping::berths(s, &locks, &probe(first))[0];
    let idx = |c: Cell| c.1 * n + c.0;
    let mut dist = vec![u32::MAX; n * n];
    let mut queue = std::collections::VecDeque::from([start]);
    dist[idx(start)] = 0;
    while let Some(c) = queue.pop_front() {
        for (dx, dy) in [(-1i64, 0i64), (1, 0), (0, -1), (0, 1)] {
            let (x, y) = (c.0 as i64 + dx, c.1 as i64 + dy);
            if x < 0 || y < 0 || x as usize >= n || y as usize >= n {
                continue;
            }
            let m = (x as usize, y as usize);
            if dist[idx(m)] == u32::MAX && sim::shipping::step_cost(s, &locks, m, false).is_some() {
                dist[idx(m)] = dist[idx(c)] + 1;
                queue.push_back(m);
            }
        }
    }
    let second = near(45)
        .filter(|&p| p.0.abs_diff(first.0) + p.1.abs_diff(first.1) > 4 && terrain_ok(s, p))
        .filter_map(|p| {
            let d = sim::shipping::berths(s, &locks, &probe(p)).iter().map(|b| dist[idx(*b)]).min()?;
            (15..=40).contains(&d).then_some((p, d))
        })
        .min_by_key(|(_, d)| d.abs_diff(26))?
        .0;
    let a = s.buildings.add_complete(BuildingKind::Dock, first, sim::buildings::Terrain { height: &s.height, depth: &s.water.depth }).ok()?;
    let b = s.buildings.add_complete(BuildingKind::Dock, second, sim::buildings::Terrain { height: &s.height, depth: &s.water.depth }).ok()?;
    let bg = BargeId(9_000_001);
    s.buildings.barges.push(sim::buildings::Barge::new(bg, start, Some(a)));
    for (g, amount) in [(Good::Sticks, 30.0), (Good::Food, 20.0)] {
        let put = s.buildings.stores.deposit(g, amount).unwrap_or(0.0);
        s.work.stock_add(a, g, put);
    }
    if let Err(e) = s.apply(sim::Command::SetBargeRoute { barge: bg, from: a, to: b, goods: vec![Good::Sticks, Good::Food] }) {
        println!("barge demo: {e}");
    }
    let ends = sim::shipping::berths(s, &locks, s.buildings.building(b)?);
    let path = sim::shipping::route(s, &locks, start, &ends, false).filter(|p| !p.is_empty()).unwrap_or(vec![start]);
    Some((a, b, bg, path))
}

fn stand_up_camp(s: &mut sim::SimState, camp: Cell) -> Vec<BuildingId> {
    let mut placed = Vec::new();
    let walker = s.colony.beavers.iter().find(|b| b.stage == sim::colony::Stage::Adult).map(|b| b.id);
    let from = walker.and_then(|w| s.agents.get(w)).map(|a| a.cell);
    for kind in [BuildingKind::LoggingCamp, BuildingKind::ForagerHut, BuildingKind::Warehouse] {
        'search: for r in 3..30usize {
            for y in camp.1.saturating_sub(r)..=camp.1 + r {
                for x in camp.0.saturating_sub(r)..=camp.0 + r {
                    let reachable = match (walker, from) {
                        (Some(w), Some(f)) => s.nav.find_path(w, f, sim::nav::Goal::Beside((x, y))).is_some(),
                        _ => true,
                    };
                    if !reachable {
                        continue;
                    }
                    if build_logic::emit(s, &BuildOrder::Building { kind, pos: (x, y) }).is_ok() {
                        if let Some(b) = s.buildings.buildings.last() {
                            placed.push(b.id);
                        }
                        break 'search;
                    }
                }
            }
        }
    }
    let adults: Vec<_> = s.colony.beavers.iter().filter(|b| b.stage == sim::colony::Stage::Adult).map(|b| b.id).collect();
    let mut camps = Vec::new();
    for id in placed {
        let Some(b) = s.buildings.buildings.iter_mut().find(|b| b.id == id) else { continue };
        b.status = Status::Complete;
        let kind = b.kind;
        let area = sim::catalogue::work_area(s.buildings.building(id).unwrap());
        let slots = sim::catalogue::def(kind).slots;
        let _ = s.work.finish_site(id, &slots, area);
        if let Some(pl) = s.work.places.iter_mut().find(|p| p.building == id) {
            pl.site = None;
        }
        if !slots.is_empty() {
            camps.push(id);
        }
    }
    for (id, who) in camps.iter().zip(adults) {
        let _ = s.apply(sim::Command::FillSlot { building: *id, slot: 0, who });
    }
    camps
}
