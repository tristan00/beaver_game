fn main() {
    let map = std::env::args().nth(1).unwrap_or_else(|| "telemark".into());
    let (mut s, dam) = sim::starts::new_game_on_map(&map).unwrap();
    s.apply(sim::Command::Settle { cell: dam.camp }).unwrap();
    s.work.auto_organise = true;
    s.apply(sim::Command::OrderDam { cells: dam.cells.clone(), height: dam.height }).unwrap();
    for days in [1.0f32, 5.0] {
        let t = std::time::Instant::now();
        let steps = (days / sim::STEP_DAYS) as usize;
        for _ in 0..steps {
            s.step(sim::STEP_DAYS).unwrap();
        }
        let ms = t.elapsed().as_secs_f64() * 1000.0;
        println!("{map}: {days} days, {steps} steps, {:.3} ms/step, day {:.1}, dam built {}", ms / steps as f64, s.time_days, dam.is_built(&s));
    }
}
