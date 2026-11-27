use bevy::prelude::*;
use bevy::render::view::screenshot::{save_to_disk, Screenshot};
use crate::{camera::OrbitCamera, cli::Cli, entities_render::CanopyView, flow::Selection,
    hud::{ColonyTab, HudState, MapLens}, sim_driver::SimClock, state::Sim};

pub fn write_audit(dir: &std::path::Path, s: &sim::SimState, out: &sim::script::Outcome) -> std::io::Result<()> {
    use std::{fmt::Write, fs};
    use sim::colony::Stage;
    fs::create_dir_all(dir)?;
    let mut homes = String::from("home,family,members,adults,yearlings,kits,condition\n");
    let mut violations = Vec::new();
    for l in s.buildings.buildings.iter().filter(|b| b.is_home_ready()) {
        let members: Vec<_> = s.colony.beavers.iter().filter(|b| b.lodge == Some(l.id)).collect();
        let fam = members.first().map(|b| b.family);
        if let Some(fam) = fam && members.iter().any(|b| b.family != fam) { violations.push(format!("mixed family in home {}", l.id.0)); }
        writeln!(homes, "{},{},{},{},{},{},{:.3}", l.id.0, fam.map_or(0, |f| f.0), members.len(),
            members.iter().filter(|b| b.stage == Stage::Adult).count(), members.iter().filter(|b| b.stage == Stage::Yearling).count(),
            members.iter().filter(|b| b.stage == Stage::Kit).count(), l.home_condition(&s.water.depth)).unwrap();
    }
    let mut beavers = String::from("id,name,family,sex,age_days,mother,father,mate,home,pregnancy_due,litter\n");
    for b in &s.colony.beavers {
        if let Some(m) = b.mate {
            if s.colony.beaver(m).is_none_or(|other| other.mate != Some(b.id) || other.family != b.family || other.lodge != b.lodge) { violations.push(format!("invalid pair {} / {}", b.id.0, m.0)); }
        }
        writeln!(beavers, "{},{},{},{:?},{:.2},{},{},{},{},{},{}", b.id.0, b.name, b.family.0, b.sex, b.age_days,
            b.mother.map_or(0,|v|v.0), b.father.map_or(0,|v|v.0), b.mate.map_or(0,|v|v.0), b.lodge.map_or(0,|v|v.0),
            b.pregnancy.as_ref().map_or(-1.0,|p|p.due_day), b.pregnancy.as_ref().map_or(0,|p|p.litter)).unwrap();
    }
    let mut buildings = String::from("id,kind,complete,x,y,wet_cells,cells,stranded,condition,reinforced,active_site,beavers,stuck_beavers\n");
    for b in &s.buildings.buildings {
        let cells: Vec<_> = b.cells().collect();
        let wet = cells.iter().filter(|&&(x,y)| s.water.depth.get(x,y).is_some_and(|&d|d >= 0.02)).count();
        let place=s.work.place(b.id);
        let active=place.is_some_and(|p|p.site.is_some());
        if b.is_complete() && active && !s.maintenance.jobs.contains_key(&b.id){violations.push(format!("finished building {} retains construction record",b.id.0));}
        let stuck=place.map_or(0,|p|p.slots.iter().filter_map(|slot|slot.beaver).filter(|id|s.agents.get(*id).is_some_and(|a|a.stuck_ticks>96)).count());
        writeln!(buildings, "{},{:?},{},{},{},{},{},{},{:.3},{},{},{},{}", b.id.0, b.kind, b.is_complete(), b.pos.0, b.pos.1,
            wet, cells.len(), s.buildings.stranded(b,&s.water.depth),b.condition,b.reinforced,active,place.map_or(0,|p|p.filled()),stuck).unwrap();
    }
    let mut events = String::new();
    let mut litters: std::collections::BTreeMap<(u64,i64), (std::collections::BTreeSet<i64>,usize)> = Default::default();
    for e in &out.events {
        writeln!(events, "day {:.4}: {:?}", e.day, e.event).unwrap();
        if let sim::colony::Event::Born { id, .. } = e.event {
            if let Some(mother) = s.colony.ancestry.get(&id).and_then(|v|v.1) {
                let entry = litters.entry((mother.0,(e.day/sim::calendar::YEAR_DAYS).floor() as i64)).or_default();
                entry.0.insert((e.day*96.0).round() as i64); entry.1 += 1;
            }
        }
    }
    for (key,(days,n)) in &litters { if days.len()!=1 || !(2..=4).contains(n) { violations.push(format!("invalid annual litter {key:?}")); } }
    for (&year,&n) in &s.colony.immigration_by_year {if n>2 {violations.push(format!("migration allowance exceeded in year {}",year+1));}}
    let summary = format!("day={:.3}\npopulation={}\nfamilies={}\nbirths={}\ndeaths={}\nfood={:.3}\nfood_zero_days={}\nhomeless_days={}\nannual_litters={:?}\nviolations={:?}\ntimings={:?}\nimmigrants={}\nmigration_by_year={:?}\nrepairs={}\nreinforcements={}\ncanal_cells={}\n",
        s.time_days, s.colony.population(), s.colony.families.len(), s.colony.stats.births, s.colony.stats.deaths, s.buildings.stores.amount(sim::buildings::Good::food()),
        out.stats.food_zero_days, out.stats.homeless_days, litters, violations, s.step_timings,s.colony.stats.immigrants,s.colony.immigration_by_year,
        s.maintenance.repairs_completed,s.maintenance.reinforcements_completed,s.water.canal.as_slice().iter().filter(|&&v|v).count());
    for (name, data) in [("summary.txt",summary),("homes.csv",homes),("beavers.csv",beavers),("buildings.csv",buildings),("events.txt",events),("run.txt",out.lines.join("\n"))] {
        fs::write(dir.join(name),data)?;
    }
    Ok(())
}

pub struct ReportCapturePlugin;
impl Plugin for ReportCapturePlugin {
    fn build(&self, app: &mut App) {
        if app.world().resource::<Cli>().report_dir.is_some() {
            app.add_systems(Update, capture);
        }
    }
}

const FILES: [&str; 16] = ["01-overview.png", "02-water.png", "03-book-beavers.png", "04-book-work.png", "05-book-water.png", "06-downstream.png", "07-beaver.png", "08-lodge.png", "09-dam-drawing.png", "10-dam-refused.png", "11-dam-closeup.png", "12-stick_pile.png", "13-dam-panel.png", "14-book-stores.png", "15-wide.png", "16-dam-dragging.png"];

fn upstream_line(s: &sim::SimState) -> Option<(Vec<(usize, usize)>, (usize, usize))> {
    let d = s.buildings.buildings.iter().find_map(|b| b.dam.as_deref())?;
    let line: Vec<(usize, usize)> = d.line().map(|c| c.cell).collect();
    let mid = *line.get(line.len() / 2)?;
    let (dx, dy) = d.downstream;
    let n = s.world.size as i64;
    let at = |x: i64, y: i64| (x >= 0 && y >= 0 && x < n && y < n).then_some((x as usize, y as usize));
    let (px, py) = (dy as i64, dx as i64);
    for k in 4..80i64 {
        for side in -15..=15i64 {
            let Some(start) = at(mid.0 as i64 - dx as i64 * k + px * side, mid.1 as i64 - dy as i64 * k + py * side) else { continue };
            if !crate::build_logic::is_wet(s, start.0, start.1) {
                continue;
            }
            let walk = |sign: i64| {
                let mut c = start;
                for _ in 0..20 {
                    let Some(next) = at(c.0 as i64 + px * sign, c.1 as i64 + py * sign) else { break };
                    c = next;
                    if !crate::build_logic::is_wet(s, c.0, c.1) {
                        break;
                    }
                }
                c
            };
            let corners = vec![walk(-1), walk(1)];
            let line = sim::sandbox::line(&corners);
            if line.len() <= 18 && crate::build_logic::check_dam(s, &line).is_ok() {
                return Some((corners, start));
            }
        }
    }
    None
}

fn dry_line(s: &sim::SimState) -> Option<(Vec<(usize, usize)>, (usize, usize))> {
    let home = s.buildings.buildings.iter().find(|b| b.kind.is_home())?.pos;
    for r in 6..30usize {
        for (sx, sy) in [(1i64, 1i64), (-1, 1), (1, -1), (-1, -1)] {
            let at = |o: usize| ((home.0 as i64 + sx * (r + o) as i64).max(0) as usize, (home.1 as i64 + sy * r as i64).max(0) as usize);
            let cells: Vec<(usize, usize)> = (0..5).map(at).collect();
            if cells.iter().all(|c| !crate::build_logic::is_wet(s, c.0, c.1) && s.fog.explored(*c)) {
                return Some((vec![cells[0], cells[4]], cells[2]));
            }
        }
    }
    None
}

#[allow(clippy::too_many_arguments)]
fn capture(mut frame: Local<u32>, cli: Res<Cli>, sim: Res<Sim>, mut clock: ResMut<SimClock>,
    mut orbit: ResMut<OrbitCamera>, mut hud: ResMut<HudState>, mut lens: ResMut<MapLens>,
    mut selection: ResMut<Selection>, mut canopy: ResMut<CanopyView>, mut commands: Commands,
    mut exit: MessageWriter<AppExit>, time: Res<Time<Real>>, mut bench_started: Local<Option<(f64, f64)>>,
    backlog: Res<crate::terrain_render::MeshBacklog>, mut ready: Local<(u32, u32)>) {
    let dir = cli.report_dir.as_ref().expect("report directory");
    *frame += 1;
    if *frame < 120 { return; }
    let shot = ((*frame - 120) / 60) as usize;
    if shot >= FILES.len() {
        if FILES.iter().all(|f| std::fs::metadata(dir.join(f)).is_ok_and(|m| m.len() > 0)) {
            if std::env::var_os("BC_REPORT_BENCH").is_some() {
                if bench_started.is_none() {
                    *bench_started = Some((time.elapsed_secs_f64(), sim.0.time_days));
                    clock.set_speed(crate::sim_driver::Speed::X3);
                    hud.overlay = None;
                    canopy.0 = false;
                    if let Some(home)=sim.0.buildings.buildings.iter().find(|b|b.is_complete()&&b.kind.is_home()) {
                        orbit.target=crate::flow::cell_world(&sim.0.height,(home.pos.0 as f32,home.pos.1 as f32));
                        orbit.distance=90.0;
                    }
                    println!("report: mature-colony benchmark starts at day {:.3}", sim.0.time_days);
                }
                let (started, day0) = bench_started.unwrap();
                let seconds = time.elapsed_secs_f64() - started;
                if seconds < 25.0 { return; }
                let days = sim.0.time_days - day0;
                println!("report: benchmark advanced {days:.3} days in {seconds:.3}s; {:.3}s per 30 days", seconds * 30.0 / days.max(f64::MIN_POSITIVE));
            }
            println!("report: saved {} views at day {:.2} in {}", FILES.len(), sim.0.time_days, dir.display());
            exit.write(AppExit::Success);
        } else if *frame > 900 { exit.write(AppExit::error()); }
        return;
    }
    clock.paused = true;
    if (*frame - 120) % 60 == 0 {
        *ready = (0, 0);
        std::fs::create_dir_all(dir).expect("create report directory");
        canopy.0 = shot != 0 && shot != 14;
        hud.overlay = match shot {
            2 => Some(crate::hud::Overlay::Colony(ColonyTab::Population)),
            3 => Some(crate::hud::Overlay::Colony(ColonyTab::Jobs)),
            4 => Some(crate::hud::Overlay::Ledger(crate::hud::LedgerTab::Dams)),
            13 => Some(crate::hud::Overlay::Ledger(crate::hud::LedgerTab::Stores)),
            _ => None,
        };
        *lens = if shot == 1 { MapLens::Water } else { MapLens::World };
        *selection = Selection::None;
        hud.set_tool_for_report(crate::hud::Tool::None);
        let s = &sim.0;
        let kind = |k: sim::buildings::BuildingKind| s.buildings.buildings.iter().find(|b| b.kind == k);
        let homes: Vec<_> = s.buildings.buildings.iter().filter(|b| b.is_complete() && b.kind.is_home()).collect();
        let dam = s.buildings.buildings.iter().filter(|b| b.kind.is_dam()).max_by_key(|b| b.pos.1);
        let mut focus = homes.first().map(|b| b.pos);
        let mut distance = 65.0;
        match shot {
            5 => {
                focus = dam.map(|b| b.pos);
                distance = 50.0;
            }
            6 => {
                if let Some(b) = s.colony.beavers.iter().find(|b| b.mate.is_some() && s.work.slot_of(b.id).is_some()).or_else(|| s.colony.beavers.first()) {
                    *selection = Selection::Beaver(b.id);
                }
            }
            7 => {
                if let Some(b) = homes.first() {
                    *selection = Selection::Site(b.id);
                }
            }
            8 | 15 => {
                if let Some((corners, mid)) = upstream_line(s) {
                    let dragging = shot == 15;
                    hud.draw_for_report(corners, dragging);
                    focus = Some(mid);
                    distance = 48.0;
                }
            }
            9 => {
                if let Some((corners, mid)) = dry_line(s) {
                    hud.draw_for_report(corners, false);
                    focus = Some(mid);
                    distance = 40.0;
                }
            }
            10 | 12 => {
                focus = dam.map(|b| b.pos);
                distance = 55.0;
                if shot == 12 {
                    if let Some(b) = dam {
                        *selection = Selection::Site(b.id);
                    }
                }
            }
            11 => {
                if let Some(b) = sim::buildings::BuildingKind::store_for(sim::dams::material()).and_then(kind) {
                    *selection = Selection::Site(b.id);
                    focus = Some(b.pos);
                }
            }
            14 => distance = 300.0,
            _ => {}
        }
        if let Some((x, y)) = focus {
            orbit.target = crate::flow::cell_world(&sim.0.height, (x as f32, y as f32));
            orbit.distance = distance;
            orbit.pitch = if shot == 14 { 1.35 } else { 1.1 };
        }
    }
    if (*frame - 120) % 60 == 40 {
        ready.0 = if backlog.terrain || backlog.water { 0 } else { ready.0 + 1 };
        ready.1 += 1;
        if ready.0 < 8 {
            if ready.1 > 1800 {
                error!("report: scene did not settle for {}", FILES[shot]);
                exit.write(AppExit::error());
            }
            *frame -= 1;
            return;
        }
        commands.spawn(Screenshot::primary_window()).observe(save_to_disk(dir.join(FILES[shot])));
    }
}
