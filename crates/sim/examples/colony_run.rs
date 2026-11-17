//! Headless 10-year run of a cared-for colony. Exits non-zero if the population did not grow.
//! Usage: cargo run -p sim --example colony_run --release -- [seed] [years]
use sim::colony::{Colony, RiverLand, Stage};

fn main() {
    let mut args = std::env::args().skip(1);
    let seed: u64 = args.next().map_or(1, |s| s.parse().expect("seed must be an integer"));
    let years: usize = args.next().map_or(10, |s| s.parse().expect("years must be an integer"));
    let land = RiverLand::default();
    let mut colony = Colony::new(seed);
    colony.found((0.0, 5.0), &land).expect("founding failed");
    let start = colony.population();
    for day in 0..years * 365 {
        colony.step_on(1.0, &land).expect("step failed");
        if day % 365 == 364 {
            println!(
                "year {:>2}: pop {:>3} (kits {}, yearlings {}, adults {}) lodges {} food {:.0} wood {:.0}",
                day / 365 + 1,
                colony.population(),
                colony.count_stage(Stage::Kit),
                colony.count_stage(Stage::Yearling),
                colony.count_stage(Stage::Adult),
                colony.lodges.len(),
                colony.food,
                colony.wood
            );
        }
    }
    println!("{:?}", colony.stats);
    if let Some(c) = colony.collapse() {
        println!("COLLAPSED: {c:?}");
    }
    let end = colony.population();
    println!("population {start} -> {end}");
    if end <= start {
        eprintln!("FAIL: population did not grow");
        std::process::exit(1);
    }
}
