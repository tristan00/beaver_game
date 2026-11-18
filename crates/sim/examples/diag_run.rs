//! Diagnostic (not a test): runs a script to a day and prints every beaver's task and assignment.
//! Usage: diag_run SCRIPT DAY
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let text = std::fs::read_to_string(&a[1]).unwrap();
    let day: f64 = a[2].parse().unwrap();
    let text = text.lines().map(|l| if l.starts_with("run ") { format!("run {day}") } else { l.to_string() }).filter(|l| !l.starts_with("check")).collect::<Vec<_>>().join("\n");
    let load = |p: &str| std::fs::read_to_string(p).map_err(|e| e.to_string());
    sim::script::run_script_with(&text, &load, &mut |s: &sim::SimState| {
        for b in &s.colony.beavers {
            let ag = s.agents.get(b.id);
            println!("#{} {:?} slot {:?} managed {:?} task {:?} assign {:?} at {:?} load {:?}", b.id.0, b.stage, s.work.slot_of(b.id).map(|x| (x.0 .0, x.1)), s.work.managed.get(&b.id), ag.map(|a| &a.task), ag.and_then(|a| a.assignment), ag.map(|a| a.cell), ag.and_then(|a| a.load));
        }
        for p in &s.work.places {
            let b = s.buildings.building(p.building).unwrap();
            println!("place {} {:?} at {:?} complete {} site {:?}", p.building.0, b.kind, b.pos, b.is_complete(), p.site.as_ref().map(|st| st.needed.iter().map(|n| (n.0, st.missing(n.0))).collect::<Vec<_>>()));
        }
        println!("stores {:?}", sim::buildings::Good::ALL.iter().map(|&g| (g, s.buildings.stores.amount(g))).collect::<Vec<_>>());
    }).unwrap();
}
