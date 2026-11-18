//! Diagnostic (not a test): what a forager does all day on a map. Usage: diag_forager [map]
use sim::buildings::BuildingKind;
use sim::resources::ResourceKind;
use sim::{Command, STEP_DAYS};

fn main() {
    let map = std::env::args().nth(1).unwrap_or("algonquin".into());
    let (mut s, dam) = sim::starts::new_game_on_map(&map).unwrap();
    s.apply(Command::Settle { cell: dam.camp }).unwrap();
    let _ = sim::autoplay::place_near(&mut s, BuildingKind::Burrow, dam.camp);
    while s.time_days < 8.0 {
        s.step(STEP_DAYS).unwrap();
    }
    let hut = sim::autoplay::place_near(&mut s, BuildingKind::ForagerHut, dam.camp).unwrap();
    while !s.buildings.building(hut).unwrap().is_complete() && s.time_days < 40.0 {
        s.step(STEP_DAYS).unwrap();
    }
    let area = s.work.place(hut).and_then(|p| p.area).unwrap();
    let plants: Vec<_> = s.resources.items.iter().filter(|r| r.kind == ResourceKind::Plant && area.contains(r.cell)).collect();
    println!("hut done day {:.1} at {:?}, area {:?}, plants in area {} (total food {:.1})", s.time_days, s.buildings.building(hut).unwrap().pos, area, plants.len(), plants.iter().map(|p| p.amount).sum::<f32>());
    let stores: Vec<_> = s.buildings.buildings.iter().filter(|b| b.is_complete()).map(|b| (b.kind, b.pos)).collect();
    println!("complete: {stores:?}; supplies {:?}; lodges {:?}", s.work.supplies, s.colony.lodges.iter().map(|l| l.pos).collect::<Vec<_>>());
    let start = s.time_days;
    let mut last: std::collections::BTreeMap<u64, String> = Default::default();
    let food0 = s.buildings.stores.produced_total[sim::buildings::Good::Food.index()];
    while s.time_days < start + 10.0 {
        s.step(STEP_DAYS).unwrap();
        for a in s.agents.by_id.values() {
            let role = sim::jobs::role_of(&s, a.id);
            if role != Some(sim::jobs::RoleKind::Forager) {
                continue;
            }
            let t = format!("{:?} load={:?}", a.task, a.load.map(|l| (l.good, (l.amount * 10.0).round() / 10.0)));
            let k = t.split(' ').next().unwrap_or("").to_string() + &format!("{:?}", a.load.is_some());
            if last.get(&a.id.0).map(|x| x.split(' ').next().unwrap_or("").to_string() + &format!("{}", x.contains("Some"))) != Some(k) {
                println!("  d{:.2} #{} at {:?} path {:?} {t}", s.time_days, a.id.0, a.cell, a.path.as_ref().map(|p| p.steps.len()));
                last.insert(a.id.0, t);
            }
        }
    }
    let food1 = s.buildings.stores.produced_total[sim::buildings::Good::Food.index()];
    println!("food gathered in 10 days: {:.1}", food1 - food0);
}
