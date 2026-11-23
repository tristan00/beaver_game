//! How long a month of colony takes to simulate, and where the time goes.
//!
//! `speed copy [MAP|SAVE]` prints what copying each part of the world for a snapshot costs.
//! `speed [MAP] [MONTHS] [worker]` plays the map's designed start with the attentive caretaker
//! and prints, for every 30 days, the real seconds spent and the share of each part of a step.
//! With `worker` the colony runs on the game's own simulation thread at the fastest game speed,
//! snapshots included, which is what a player waits for.
use sim::{Command, SimState, StepTimings, STEP_DAYS};
use std::time::Instant;

const MONTH_DAYS: f64 = 30.0;
/// The fastest game speed, in days a second (the interface's ×3).
const TOP_DAYS_PER_SECOND: f64 = 6.0;

fn parts(t: &StepTimings) -> [(&'static str, f64); 8] {
    [
        ("water", t.water_s), ("ecology", t.ecology_s), ("colony", t.colony_s), ("buildings", t.buildings_s),
        ("agents", t.agents_s), ("dams+canals", t.shipping_s), ("fog", t.fog_s), ("other", t.other_s),
    ]
}

fn fingerprint(s: &mut SimState) -> u64 {
    // Left out: the water's wall-clock timings, each grid's per-process identity, and the
    // order hash maps happen to iterate in. None of them is part of the world.
    s.water.reset_timings();
    s.water.canonical();
    sim::grid::CONTENT_ONLY.store(true, std::sync::atomic::Ordering::Relaxed);
    let hash = |b: Vec<u8>| b.iter().fold(0xCBF2_9CE4_8422_2325u64, |h, b| (h ^ *b as u64).wrapping_mul(0x100_0000_01B3));
    let mut parts: Vec<(&str, u64)> = Vec::new();
    macro_rules! part { ($($f:ident),*) => {$( parts.push((stringify!($f), hash(postcard::to_stdvec(&s.$f).expect("encode")))); )*}; }
    part!(time_days, height, world, water, ecology, buildings, events, nav, resources, agents, work, maintenance, society, ledger, output, fog, steps, forestry, predators);
    let mut colony = s.colony.clone();
    let ancestry: std::collections::BTreeMap<_, _> = std::mem::take(&mut colony.ancestry).into_iter().collect();
    let slot_jobs: std::collections::BTreeMap<_, _> = std::mem::take(&mut colony.slot_jobs).into_iter().collect();
    parts.push(("colony", hash(postcard::to_stdvec(&(colony, ancestry, slot_jobs)).expect("encode"))));
    sim::grid::CONTENT_ONLY.store(false, std::sync::atomic::Ordering::Relaxed);
    if std::env::var_os("SPEED_PARTS").is_some() {
        for (name, h) in &parts { println!("  {name:<12} {h:016x}"); }
    }
    parts.iter().fold(0xCBF2_9CE4_8422_2325u64, |h, p| (h ^ p.1).wrapping_mul(0x100_0000_01B3))
}

/// A saved colony, or a newly settled one; either carries on with nobody at the controls.
fn start(map: &str) -> SimState {
    if map.ends_with(".sav") {
        return sim::save::load(std::path::Path::new(map)).unwrap_or_else(|e| panic!("{map}: {e}")).1;
    }
    let (mut s, dam) = sim::starts::new_game_on_map(map).unwrap_or_else(|e| panic!("map {map}: {e:?}"));
    s.apply(Command::Settle { cell: dam.camp }).expect("settle");
    s
}

fn direct(map: &str, months: u32) {
    let mut s = start(map);
    s.water.note_on_request();
    if std::env::var_os("SPEED_PROFILE").is_some() { sim::prof::enable(); }
    println!("{map}: {} cells, {} steps a day", s.world.size * s.world.size, (1.0 / STEP_DAYS) as u32);
    let mut total = 0.0;
    for month in 1..=months {
        let before = s.step_timings;
        let begun = Instant::now();
        for _ in 0..MONTH_DAYS as u32 {
            for step in 0..(1.0 / STEP_DAYS) as u32 {
                s.step(STEP_DAYS).expect("step");
                s.colony.take_events();
                // As the game does before each snapshot, about four times a game day.
                if step % 24 == 23 { s.water.note_changes(); }
            }
        }
        let secs = begun.elapsed().as_secs_f64();
        total += secs;
        let now = s.step_timings;
        let split: Vec<String> = parts(&now).iter().zip(parts(&before)).map(|(a, b)| format!("{} {:.2}", a.0, a.1 - b.1)).collect();
        println!(
            "month {month:>2}: {secs:>6.2} s ({:.3} ms a step) pop {:>2} buildings {:>2} | {}",
            secs * 1000.0 / (MONTH_DAYS / STEP_DAYS as f64),
            s.colony.population(),
            s.buildings.buildings.len(),
            split.join(", "),
        );
    }
    println!("{map}: {:.2} s a month over {months} months; state {:016x}", total / months as f64, fingerprint(&mut s));
    if std::env::var_os("SPEED_PROFILE").is_some() {
        print!("{}", sim::prof::report(MONTH_DAYS * months as f64));
    }
}

fn worker(map: &str, months: u32) {
    let s = start(map);
    let worker = sim::runtime::Worker::spawn(s.clone());
    worker.clock(false, TOP_DAYS_PER_SECOND).expect("clock");
    let mut local = s;
    let (mut month_start, mut next_month) = (Instant::now(), MONTH_DAYS);
    let (mut snapshots, mut total) = (0u32, 0.0);
    let end = MONTH_DAYS * months as f64;
    while local.time_days < end {
        // The interface takes a snapshot each frame.
        std::thread::sleep(std::time::Duration::from_millis(16));
        let Some(snapshot) = worker.poll() else { continue };
        snapshots += 1;
        let old = std::mem::replace(&mut local, snapshot.state);
        worker.recycle(old);
        if local.time_days >= next_month {
            let secs = month_start.elapsed().as_secs_f64();
            total += secs;
            println!(
                "month {:>2}: {secs:>6.2} s on the game's thread at top speed ({snapshots} snapshots) pop {}",
                (next_month / MONTH_DAYS) as u32,
                local.colony.population()
            );
            month_start = Instant::now();
            next_month += MONTH_DAYS;
            snapshots = 0;
        }
    }
    println!("{map}: {:.2} s a month on the game's thread; asked for {:.2}", total / months as f64, MONTH_DAYS / TOP_DAYS_PER_SECOND);
}

/// What a snapshot for the interface costs: the time to copy each part of the world into a
/// world already holding the same buffers, as the simulation thread does many times a second.
fn copy(source: &str) {
    let s = if source.ends_with(".sav") {
        sim::save::load(std::path::Path::new(source)).unwrap_or_else(|e| panic!("{source}: {e}")).1
    } else {
        start(source)
    };
    println!(
        "{source}: day {:.0}, {} beavers, {} buildings ({} dams, {} canals), {} resources",
        s.time_days,
        s.colony.population(),
        s.buildings.buildings.len(),
        s.buildings.buildings.iter().filter(|b| b.dam.is_some()).count(),
        s.buildings.buildings.iter().filter(|b| b.canal.is_some()).count(),
        s.resources.items().len(),
    );
    let mut spare = s.clone();
    let mut rows: Vec<(&str, f64)> = Vec::new();
    const ROUNDS: u32 = 20;
    macro_rules! part {
        ($($f:ident),*) => {$(
            let begun = Instant::now();
            for _ in 0..ROUNDS { spare.$f.clone_from(&s.$f); }
            rows.push((stringify!($f), begun.elapsed().as_secs_f64() * 1000.0 / ROUNDS as f64));
        )*};
    }
    part!(height, world, water, ecology, colony, buildings, events, nav, resources, agents, work, maintenance, society, ledger, output, fog, forestry, predators);
    let begun = Instant::now();
    for _ in 0..ROUNDS { spare.clone_from(&s); }
    let whole = begun.elapsed().as_secs_f64() * 1000.0 / ROUNDS as f64;
    rows.sort_by(|a, b| b.1.total_cmp(&a.1));
    println!("copy of the whole world: {whole:.3} ms; save size {:.1} MB", sim::save::encode(&s, "speed").map_or(0.0, |b| b.len() as f64 / 1e6));
    for (name, ms) in rows { println!("  {name:<12} {ms:>8.3} ms"); }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().is_some_and(|a| a == "copy") {
        return copy(args.get(1).map_or("algonquin", |s| s.as_str()));
    }
    let map = args.first().map_or("algonquin", |s| s.as_str());
    let months: u32 = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(12);
    if args.get(2).is_some_and(|a| a == "worker") { worker(map, months) } else { direct(map, months) }
}
