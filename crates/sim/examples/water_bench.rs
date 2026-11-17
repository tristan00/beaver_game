//! Benchmark: `cargo run --release -p sim --example water_bench`.
//! Times one `Water::step` on a 256x256 grid; exits non-zero if the average is 2 ms or more.
use sim::{grid::HeightGrid, water::Water, worldgen::WorldGen};
use std::time::Instant;

fn main() {
    const N: usize = 256;
    const LIMIT_MS: f64 = 2.0;
    let mut gen_ = WorldGen::new(7);
    gen_.size = N;
    let height: HeightGrid = gen_.heightmap();
    let mut water = Water::new(N);
    water.add_source(10, 10, 20.0).unwrap();
    water.add_source(200, 120, 20.0).unwrap();
    for _ in 0..200 { water.step(&height, 0.01); } // warm up with water on the map
    let steps = 500;
    let t = Instant::now();
    for _ in 0..steps { water.step(&height, 0.01); }
    let ms = t.elapsed().as_secs_f64() * 1000.0 / steps as f64;
    println!("{N}x{N} step: {ms:.3} ms (limit {LIMIT_MS} ms), volume {:.1}", water.total_volume());
    if ms >= LIMIT_MS {
        eprintln!("FAIL: too slow");
        std::process::exit(1);
    }
}
