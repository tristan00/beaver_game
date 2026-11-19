use sim::autoplay::{Player, Style};
use sim::buildings::{BuildingKind, Good};
use sim::colony::{DeathCause, Event, Stage};
use sim::hud::{problems, ProblemKind};
use sim::{Command, SimState};
use std::collections::BTreeMap;

fn main() {
    let days: f64 = std::env::args().nth(1).and_then(|a| a.parse().ok()).unwrap_or(360.0);
    let only = std::env::var("AUDIT_MAP").ok();
    for map in sim::maps::all().into_iter().filter(|m| only.as_ref().is_none_or(|o| *o == m.id)) {
        let styles = if std::env::var_os("AUDIT_ONE").is_some() { vec![(Style::Attentive, false)] } else { vec![(Style::Attentive, false), (Style::IgnoresAlerts, false), (Style::HandsOff, false), (Style::Attentive, true)] };
        for (style, alt) in styles {
            run(&map.id, style, alt, days);
        }
    }
}

fn other_site(s: &SimState, camp: (usize, usize)) -> Option<(usize, usize)> {
    let n = s.world.size;
    let mut best = None;
    for y in camp.1.saturating_sub(16)..(camp.1 + 16).min(n) {
        for x in camp.0.saturating_sub(16)..(camp.0 + 16).min(n) {
            let d = (x.abs_diff(camp.0).pow(2) + y.abs_diff(camp.1).pow(2)) as f32;
            if !(100.0..=200.0).contains(&d) {
                continue;
            }
            let wet = |c: (usize, usize)| s.world.river.get(c.0, c.1) == Some(&true) || s.water.depth.get(c.0, c.1).is_some_and(|d| *d > 0.05);
            if wet((x, y)) || s.world.valley_of((x, y)).is_none_or(|v| !s.world.is_open(v)) {
                continue;
            }
            let near = (y.saturating_sub(6)..(y + 6).min(n)).any(|yy| (x.saturating_sub(6)..(x + 6).min(n)).any(|xx| wet((xx, yy))));
            if near {
                best = Some((x, y));
                break;
            }
        }
        if best.is_some() {
            break;
        }
    }
    best
}

fn run(map: &str, style: Style, alt: bool, days: f64) {
    let (mut s, dam) = sim::starts::new_game_on_map(map).expect("map");
    let camp = if alt { other_site(&s, dam.camp).unwrap_or(dam.camp) } else { dam.camp };
    let settle = s.apply(Command::Settle { cell: camp });
    println!("\n=== {map} | {style:?}{} | settle at {camp:?}: {} ===", if alt { " (other site)" } else { "" }, if settle.is_ok() { "ok".to_string() } else { format!("{settle:?}") });
    if settle.is_err() {
        return;
    }
    let mut p = Player::new(style, camp, if alt { None } else { Some((dam.cells.clone(), dam.height)) });
    if alt {
        p.dam = None;
    }
    let mut done: BTreeMap<u64, (BuildingKind, f64)> = BTreeMap::new();
    let mut techs: Vec<(String, f64)> = Vec::new();
    let mut alerts: BTreeMap<String, u32> = BTreeMap::new();
    let mut top_changes: Vec<(f64, String)> = Vec::new();
    let mut last_top = String::new();
    let (mut zero_food_days, mut homeless_days, mut idle_days, mut min_food) = (0u32, 0u32, 0u32, f32::MAX);
    let mut deaths: BTreeMap<String, u32> = BTreeMap::new();
    let mut births = 0;
    let mut stalled: BTreeMap<u64, u32> = BTreeMap::new();
    let mut day = 0.0;
    let mut placed_alt_dam = false;
    while s.time_days < days {
        if s.time_days >= day {
            day += 1.0;
            if alt && !placed_alt_dam && s.colony.is_founded() {
                p.place(&mut s, BuildingKind::Dam);
                placed_alt_dam = true;
            }
            p.act(&mut s);
            if std::env::var_os("AUDIT_DEBUG").is_some() && (day as i64) % 3 == 1 && day < 45.0 {
                for bb in &s.buildings.buildings { println!("    building {:?} {:?} at {:?} complete {}", bb.id, bb.kind, bb.pos, bb.is_complete()); }
                println!("    supplies {:?}", s.work.supplies);
                for b in &s.colony.beavers {
                    let a = s.agents.get(b.id);
                    println!(
                        "    d{} {} {:?} slot={:?} asg={:?} task={:?} cell={:?} path={:?} stuck={} inside={} rest={:.2} food={:.2} resting={} managed={:?}",
                        day as i64 - 1, b.name, b.stage, s.work.slot_of(b.id).map(|(bb, i, r)| (bb.0, i, r)), a.and_then(|a| a.assignment), a.map(|a| a.task.clone()), a.map(|a| a.cell), a.and_then(|a| a.path.as_ref().map(|p| p.steps.len())), a.map(|a| a.stuck_ticks).unwrap_or(0), a.map(|a| a.inside).unwrap_or(false), b.needs.rest, b.needs.food, b.resting, s.work.managed.get(&b.id)
                    );
                }
            }
            let food = s.buildings.stores.amount(Good::Food);
            min_food = min_food.min(food);
            if food < 0.5 {
                zero_food_days += 1;
            }
            if s.colony.beavers.iter().any(|b| b.lodge.is_none()) {
                homeless_days += 1;
            }
            idle_days += sim::jobs::idle_reasons(&s).iter().filter(|(_, r)| *r != sim::jobs::IdleReason::Resting).count() as u32;
            let ps = problems(&s);
            for pr in &ps {
                *alerts.entry(format!("{:?}", pr.kind)).or_default() += 1;
            }
            let top = ps.first().map(|p| format!("{:?} {:.0} fix={:?}", p.kind, p.value, p.fix)).unwrap_or_else(|| "-".into());
            let top_kind = ps.first().map(|p| format!("{:?}{:?}", p.kind, p.fix)).unwrap_or_default();
            if top_kind != last_top {
                top_changes.push((s.time_days, top));
                last_top = top_kind;
            }
            for b in s.buildings.buildings.iter() {
                if b.is_complete() {
                    done.entry(b.id.0).or_insert((b.kind, s.time_days));
                } else if let Some(site) = s.work.place(b.id).and_then(|pl| pl.site.as_ref()) {
                    let starved = site.needed.iter().any(|a| site.missing(a.0) > 0.0 && s.buildings.stores.amount(a.0) < 0.05);
                    if starved {
                        *stalled.entry(b.id.0).or_default() += 1;
                    }
                }
            }
            for (i, t) in sim::tech::TECHS.iter().enumerate() {
                if s.tech.is_unlocked(sim::tech::TechId(i)) && !techs.iter().any(|(n, _)| n == t.name) {
                    techs.push((t.name.to_string(), s.time_days));
                }
            }
            if (day as i64) % 45 == 1 {
                let st = &s.buildings.stores;
                println!(
                    "  d{:>3} {:<6} pop {} (kits {}) homeless {} | food {:>5.1} logs {:>5.1} sticks {:>5.1} mud {:>4.1} stone {:>4.1} rope {:>3.1} | buildings {} sites {} | alerts {}",
                    day as i64 - 1,
                    sim::calendar::Date::at(s.time_days).season.name(),
                    s.colony.population(),
                    s.colony.count_stage(Stage::Kit),
                    s.colony.beavers.iter().filter(|b| b.lodge.is_none()).count(),
                    st.amount(Good::Food),
                    st.amount(Good::Logs),
                    st.amount(Good::Sticks),
                    st.amount(Good::Mud),
                    st.amount(Good::Stone),
                    st.amount(Good::Rope),
                    s.buildings.buildings.iter().filter(|b| b.is_complete()).count(),
                    s.buildings.buildings.iter().filter(|b| !b.is_complete()).count(),
                    ps.iter().map(|p| format!("{:?}", p.kind)).collect::<Vec<_>>().join(",")
                );
            }
        }
        if s.step(0.1).is_err() {
            println!("  step failed");
            break;
        }
        for e in s.colony.take_events() {
            match e.event {
                Event::Died { cause, .. } => *deaths.entry(format!("{cause:?}")).or_default() += 1,
                Event::Born { .. } => births += 1,
                _ => {}
            }
        }
        if s.colony.is_collapsed() {
            println!("  COLLAPSED on day {:.0}", s.time_days);
            break;
        }
    }
    println!("  -- actions --");
    for l in p.log.iter().take(30) {
        println!("    {l}");
    }
    println!("  -- completed --");
    let mut list: Vec<_> = done.values().collect();
    list.sort_by(|a, b| a.1.total_cmp(&b.1));
    for (k, d) in list {
        println!("    day {:>5.1}: {k:?}", d);
    }
    for (id, n) in &stalled {
        if *n >= 10 {
            let kind = s.buildings.buildings.iter().find(|b| b.id.0 == *id).map(|b| format!("{:?}", b.kind)).unwrap_or("gone".into());
            println!("    STALLED {kind} #{id}: {n} days waiting for a material the colony has none of");
        }
    }
    println!("  -- research -- {}", techs.iter().map(|(n, d)| format!("{n} d{d:.0}")).collect::<Vec<_>>().join(", "));
    println!("  -- top alert changes --");
    for (d, t) in top_changes.iter().take(14) {
        println!("    day {d:>5.1}: {t}");
    }
    println!("  -- alert-days -- {alerts:?}");
    let starved = deaths.get(&format!("{:?}", DeathCause::Starvation)).copied().unwrap_or(0);
    println!(
        "  SUMMARY {map} {style:?}{}: pop {} births {births} deaths {deaths:?} (starved {starved}) | food<=0 on {zero_food_days} days, min food {min_food:.1} | homeless-days {homeless_days} | free-beaver-days {idle_days} | {}",
        if alt { " other-site" } else { "" },
        s.colony.population(),
        if s.colony.is_collapsed() { "COLLAPSED" } else { "alive" }
    );
    let _ = ProblemKind::Food;
}
