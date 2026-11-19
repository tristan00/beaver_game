use sim::water::Water;

fn main() {
    let mut args = std::env::args().skip(1);
    let map = args.next().expect("map id or .map file");
    let (mut days, mut dam, mut out) = (30.0f32, false, None);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--days" => days = args.next().unwrap().parse().unwrap(),
            "--dam" => dam = true,
            "--out" => out = args.next(),
            _ => panic!("unknown argument {a}"),
        }
    }
    let (mut sim, site) = if map.ends_with(".map") {
        let data = sim::maps::parse(&std::fs::read(&map).unwrap()).unwrap();
        let info = data.info.clone();
        let world = sim::maps::world_from(data).unwrap();
        sim::starts::new_game_on_world(&info, world).unwrap()
    } else if std::env::var_os("NOSTART").is_some() {
        let (info, world) = sim::maps::world(&map).unwrap();
        let t = std::time::Instant::now();
        let s = sim::SimState::from_world(info.seed(), world);
        println!("  world + water built in {:.0} ms", t.elapsed().as_secs_f64() * 1000.0);
        (s, sim::starts::DamSite { cells: vec![(0, 0)], height: 1.0, camp: (0, 0) })
    } else {
        let t = std::time::Instant::now();
        let r = sim::starts::new_game_on_map(&map).unwrap();
        println!("  new game (world, water spin-up, colony) in {:.0} ms; spin-up {}", t.elapsed().as_secs_f64() * 1000.0, r.0.water.timings.report(t.elapsed().as_secs_f64()));
        r
    };
    let n = sim.world.size;
    if dam {
        sim.water.build_dam(&site.cells, site.height).unwrap();
    }
    println!("  planned dam {:?} height {}", site.cells, site.height);
    let mut w: Water = sim.water.clone();
    w.reset_timings();
    let steps = (days / sim::STEP_DAYS).round() as usize;
    let last_day = (1.0 / sim::STEP_DAYS).round() as usize;
    let (mut a0, mut l0, mut v0) = (0.0, 0.0, 0.0);
    let clock = std::time::Instant::now();
    for k in 0..steps {
        if k + last_day == steps {
            (a0, l0, v0) = (w.added_volume, w.lost_volume, w.total_volume());
        }
        w.step(&sim.height, sim::STEP_DAYS);
    }
    let ms_per_day = clock.elapsed().as_secs_f64() * 1000.0 / days as f64;
    let wall = clock.elapsed().as_secs_f64();
    println!("  {}", w.timings.report(wall));
    println!("  game speed reachable on water alone: x{:.1} (x1 = 288 s of physics per second)", days as f64 * 86_400.0 / wall / 288.0);
    println!("  active cells now {} in {} row segments", w.active_cells(), w.active_segments());
    let (inn, out_d, stored) = (w.added_volume - a0, w.lost_volume - l0, w.total_volume() - v0);
    let d = w.depth.as_slice();
    let (river, lake) = (sim.world.river.as_slice(), sim.world.lake.as_slice());
    let h = sim.height.as_slice();
    let surf = sim.world.surface.as_slice();
    let wet = |i: usize| d[i] >= sim::buildings::WET_DEPTH;
    let river_n = river.iter().filter(|r| **r).count();
    let dry = (0..n * n).filter(|&i| river[i] && d[i] < 0.02).count();
    let over = (0..n * n).filter(|&i| river[i] && h[i] + d[i] > surf[i] + 0.02).count();
    let mut near = vec![false; n * n];
    for i in (0..n * n).filter(|&i| river[i] || lake[i]) {
        let (x, y) = ((i % n) as isize, (i / n) as isize);
        for dy in -2..=2isize {
            for dx in -2..=2isize {
                let (nx, ny) = (x + dx, y + dy);
                if nx >= 0 && ny >= 0 && (nx as usize) < n && (ny as usize) < n {
                    near[ny as usize * n + nx as usize] = true;
                }
            }
        }
    }
    let wet_off = (0..n * n).filter(|&i| wet(i) && !river[i] && !lake[i]).count();
    let wet_far = (0..n * n).filter(|&i| wet(i) && !near[i]).count();
    let mut depths: Vec<f32> = (0..n * n).filter(|&i| river[i]).map(|i| d[i]).collect();
    depths.sort_by(|a, b| a.total_cmp(b));
    let pct = |p: usize| depths[(depths.len() - 1) * p / 100];
    println!(
        "{map}: day {days} ({ms_per_day:.1} ms/day): in {inn:.2}/day out {out_d:.2}/day stored {stored:+.2}/day | river cells {river_n}: dry {dry}, over banks {over}, depth p10 {:.3} p50 {:.3} p90 {:.3} | wet off-river {wet_off}, beyond 2 cells of water {wet_far}",
        pct(10),
        pct(50),
        pct(90)
    );
    if std::env::var("DEEP").is_ok() {
        let mut deep: Vec<usize> = (0..n * n).filter(|&i| river[i]).collect();
        deep.sort_by(|&a, &b| d[b].total_cmp(&d[a]));
        for &i in deep.iter().take(12) {
            let t = sim.world.flow_to.as_slice()[i];
            println!(
                "  deep ({}, {}) depth {:.2} bed {:.2} surf {:.2} q {:.2} -> {:?} bed {:.2} lake {} surf {:.2}",
                i % n, i / n, d[i], h[i], surf[i], sim.world.river_flow.as_slice()[i],
                t.map(|t| (t % n, t / n)), t.map_or(f32::NAN, |t| h[t]), t.is_some_and(|t| lake[t]), t.map_or(f32::NAN, |t| surf[t])
            );
        }
    }
    if let Ok(v) = std::env::var("TRACE") {
        let xy: Vec<usize> = v.split(',').map(|t| t.parse().unwrap()).collect();
        let mut i = xy[1] * n + xy[0];
        for _ in 0..400 {
            let lvl = h[i] + d[i];
            println!("  trace ({:>3}, {:>3}) bed {:.2} depth {:.3} width {:.2} chan {} q {:.4} level {:.3} river {}", i % n, i / n, h[i], d[i], w.width.as_slice()[i], w.channel.as_slice()[i], w.discharge.as_slice()[i], lvl, river[i]);
            match sim.world.flow_to.as_slice()[i] { Some(t) => i = t, None => break }
        }
    }
    if dam {
        let integ: Vec<String> = site.cells.iter().map(|&(x, y)| format!("{:.2}", w.dam_integrity.get(x, y).unwrap())).collect();
        println!("  dam integrity {}", integ.join(" "));
    }
    let mut cuts: Vec<f32> = (0..n * n).filter(|&i| river[i]).map(|i| surf[i] - h[i]).collect();
    cuts.sort_by(|a, b| a.total_cmp(b));
    println!("  channel depth below the land: p50 {:.2} p90 {:.2} max {:.2}", cuts[cuts.len() / 2], cuts[cuts.len() * 9 / 10], cuts[cuts.len() - 1]);
    if let Some(path) = out {
        let mut bytes = (n as u32).to_le_bytes().to_vec();
        for &v in h {
            bytes.extend_from_slice(&v.to_le_bytes());
        }
        for &v in d {
            bytes.extend_from_slice(&v.to_le_bytes());
        }
        for i in 0..n * n {
            let c = (i % n, i / n);
            let mut f = 0u8;
            f |= river[i] as u8;
            f |= (lake[i] as u8) << 1;
            f |= (sim.world.forest.as_slice()[i].is_some() as u8) << 2;
            f |= ((w.dam_height.as_slice()[i] > 0.0 || (!dam && site.cells.contains(&c))) as u8) << 3;
            f |= ((c == site.camp) as u8) << 4;
            f |= (w.sources.iter().any(|s| (s.x, s.y) == c) as u8) << 5;
            bytes.push(f);
        }
        std::fs::write(&path, bytes).unwrap();
    }
}
