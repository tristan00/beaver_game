//! The smallest policy: shows the protocol, not a way to play.
//! It sends two foragers out on the first state it sees, then only reports what it sees.
//!
//! `cargo build --release -p runner --example sample_policy`
//! `target/release/runner --map algonquin --policy target/release/examples/sample_policy`
fn main() {
    let mut p = runner::Policy::new();
    let mut first = true;
    let mut next_report = 0.0;
    while let Some(s) = p.next_state() {
        if first {
            first = false;
            let _ = p.send(&sim::Command::AdjustRole { role: sim::jobs::Job::gathering(sim::resources::ResourceKind::food_plant(false)), delta: 2 });
        }
        if s.time_days >= next_report {
            let c = s.census();
            eprintln!("policy sees day {:.2}: beavers {} water {:.0} m3", s.time_days, c.beavers, c.water);
            next_report += 30.0;
        }
    }
}
