//! Diagnostic (not a test): where stone and mud lie on each map, relative to the designed camp.
use sim::resources::ResourceKind;
fn main() {
    for map in ["algonquin", "voyageurs", "bavarian", "telemark"] {
        let (s, dam) = sim::starts::new_game_on_map(map).unwrap();
        let size = s.water.depth.width();
        for k in [ResourceKind::StoneOutcrop, ResourceKind::MudBank, ResourceKind::Plant] {
            let rs: Vec<_> = s.resources.items.iter().filter(|r| r.kind == k).collect();
            let total: f32 = rs.iter().map(|r| r.amount).sum();
            let d = |c: (usize, usize)| ((c.0 as f32 - dam.camp.0 as f32).powi(2) + (c.1 as f32 - dam.camp.1 as f32).powi(2)).sqrt();
            let near = rs.iter().filter(|r| d(r.cell) < 15.0).count();
            let mid = rs.iter().filter(|r| d(r.cell) < 30.0).count();
            println!("{map} size {size} {k:?}: {} spots, total {total:.0}, within 15: {near}, within 30: {mid}", rs.len());
        }
    }
}
