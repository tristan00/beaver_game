//! Checks candidate `.map` files the way the bundled maps are tested (`maps::tests`), and prints
//! the numbers: `cargo run -p sim --release --example check_map -- a.map b.map ...`
use sim::buildings::WET_DEPTH;

fn main() {
    for path in std::env::args().skip(1) {
        let bytes = match std::fs::read(&path) {
            Ok(b) => b,
            Err(e) => {
                println!("{path}: cannot read: {e}");
                continue;
            }
        };
        let data = match sim::maps::parse(&bytes) {
            Ok(v) => v,
            Err(e) => {
                println!("{path}: {e}");
                continue;
            }
        };
        let info = data.info.clone();
        let world = match sim::maps::world_from(data) {
            Ok(w) => w,
            Err(e) => {
                println!("{path}: {e}");
                continue;
            }
        };
        let (sim, dam) = match sim::starts::new_game_on_world(&info, world) {
            Ok(v) => v,
            Err(e) => {
                println!("{path}: no start: {e}");
                continue;
            }
        };
        let effect = sim::starts::dam_effect(&sim, &dam.cells, dam.height);
        let streams = sim::starts::dammable_streams(&sim).len();
        let n = sim.world.size * sim.world.size;
        let lake = sim.world.lake.as_slice().iter().filter(|l| **l).count();
        let river = sim.world.river.as_slice().iter().filter(|r| **r).count().max(1);
        let wet = (0..n).filter(|&i| sim.water.depth.as_slice()[i] >= WET_DEPTH && !sim.world.lake.as_slice()[i]).count();
        let visible = effect.is_some_and(|e| e.is_visible());
        let ok = visible && streams >= 2 && (lake as f32) < 0.06 * n as f32 && (wet as f32) < 3.5 * river as f32;
        println!(
            "{path}: {} visible={visible} streams={streams} lake={:.3} wet_per_river={:.2} effect={effect:?}",
            if ok { "PASS" } else { "FAIL" },
            lake as f32 / n as f32,
            wet as f32 / river as f32
        );
    }
}
