//! Native screenshots of one scripted colony state, with no desktop automation.
use bevy::prelude::*;
use bevy::render::view::screenshot::{save_to_disk, Screenshot};
use crate::{camera::OrbitCamera, cli::Cli, entities_render::CanopyView, flow::Selection,
    hud::{ColonyTab, HudState, MapLens}, sim_driver::SimClock, state::Sim};

pub fn write_audit(dir: &std::path::Path, s: &sim::SimState, out: &sim::script::Outcome) -> std::io::Result<()> {
    use std::{fmt::Write, fs};
    use sim::{buildings::Site, colony::Stage};
    fs::create_dir_all(dir)?;
    let mut homes = String::from("home,family,members,adults,yearlings,kits,condition\n");
    let mut violations = Vec::new();
    for l in &s.colony.lodges {
        let members: Vec<_> = s.colony.beavers.iter().filter(|b| b.lodge == Some(l.id)).collect();
        if members.iter().any(|b| b.family != l.family) { violations.push(format!("mixed family in home {}", l.id.0)); }
        writeln!(homes, "{},{},{},{},{},{},{:.3}", l.id.0, l.family.0, members.len(),
            members.iter().filter(|b| b.stage == Stage::Adult).count(), members.iter().filter(|b| b.stage == Stage::Yearling).count(),
            members.iter().filter(|b| b.stage == Stage::Kit).count(), l.condition).unwrap();
    }
    let mut beavers = String::from("id,name,family,sex,age_days,mother,father,mate,home,pregnancy_due,litter\n");
    for b in &s.colony.beavers {
        if let Some(m) = b.mate {
            if s.colony.beaver(m).is_none_or(|other| other.mate != Some(b.id) || other.family != b.family || other.lodge != b.lodge)
                || s.colony.close_relatives(b.id, m) { violations.push(format!("invalid pair {} / {}", b.id.0, m.0)); }
        }
        writeln!(beavers, "{},{},{},{:?},{:.2},{},{},{},{},{},{}", b.id.0, b.name, b.family.0, b.sex, b.age_days,
            b.mother.map_or(0,|v|v.0), b.father.map_or(0,|v|v.0), b.mate.map_or(0,|v|v.0), b.lodge.map_or(0,|v|v.0),
            b.pregnancy.as_ref().map_or(-1.0,|p|p.due_day), b.pregnancy.as_ref().map_or(0,|p|p.litter)).unwrap();
    }
    let mut buildings = String::from("id,kind,complete,x,y,wet_cells,cells,pond_building,stranded,condition,stone,active_site,workers,stuck_workers\n");
    let (mut pond, mut wet_pond) = (0, 0);
    for b in &s.buildings.buildings {
        let cells: Vec<_> = b.cells().collect();
        let wet = cells.iter().filter(|&&(x,y)| s.water.depth.get(x,y).is_some_and(|&d|d >= 0.02)).count();
        let in_pond = b.kind.spec().site == Site::Pond;
        if b.is_complete() && in_pond { pond += 1; if wet > cells.len()/2 { wet_pond += 1; } }
        let place=s.work.place(b.id);
        let active=place.is_some_and(|p|p.site.is_some());
        if b.is_complete() && active && !s.maintenance.jobs.contains_key(&b.id){violations.push(format!("finished building {} retains construction record",b.id.0));}
        let stuck=place.map_or(0,|p|p.slots.iter().filter_map(|slot|slot.worker).filter(|id|s.agents.get(*id).is_some_and(|a|a.stuck_ticks>96)).count());
        writeln!(buildings, "{},{:?},{},{},{},{},{},{},{},{:.3},{},{},{},{}", b.id.0, b.kind, b.is_complete(), b.pos.0, b.pos.1,
            wet, cells.len(), in_pond, s.buildings.stranded(b,&s.water.depth),b.condition,b.reinforced,active,place.map_or(0,|p|p.filled()),stuck).unwrap();
    }
    let mut events = String::new();
    let mut litters: std::collections::BTreeMap<(u64,i64), (std::collections::BTreeSet<i64>,usize)> = Default::default();
    for e in &out.events {
        writeln!(events, "day {:.4}: {:?}", e.day, e.event).unwrap();
        if let sim::colony::Event::Born { id, .. } = e.event {
            if let Some(mother) = s.colony.ancestry.get(&id).and_then(|v|v.1) {
                let entry = litters.entry((mother.0,(e.day/360.0).floor() as i64)).or_default();
                entry.0.insert((e.day*96.0).round() as i64); entry.1 += 1;
            }
        }
    }
    for (key,(days,n)) in &litters { if days.len()!=1 || !(2..=4).contains(n) { violations.push(format!("invalid annual litter {key:?}")); } }
    for (&year,&n) in &s.colony.immigration_by_year {if n>2 {violations.push(format!("migration allowance exceeded in year {}",year+1));}}
    let mut barges=String::from("id,from,to,delivered,state\n");for b in &s.buildings.barges{writeln!(barges,"{},{},{},{:.3},{:?}",b.id.0,b.service.as_ref().map_or(0,|v|v.from.0),b.service.as_ref().map_or(0,|v|v.to.0),b.carried,b.state).unwrap();}
    let summary = format!("day={:.3}\npopulation={}\nfamilies={}\nbirths={}\ndeaths={}\nfood={:.3}\nfood_zero_days={}\nhomeless_days={}\npond_buildings={}\nwet_pond_buildings={}\nannual_litters={:?}\nviolations={:?}\ntimings={:?}\nimmigrants={}\nmigration_by_year={:?}\nrepairs={}\nreinforcements={}\ncanal_cells={}\nbarges={}\ndelivered={:.3}\n",
        s.time_days, s.colony.population(), s.colony.families.len(), s.colony.stats.births, s.colony.stats.deaths, s.colony.food,
        out.stats.food_zero_days, out.stats.homeless_days, pond, wet_pond, litters, violations, s.step_timings,s.colony.stats.immigrants,s.colony.immigration_by_year,
        s.maintenance.repairs_completed,s.maintenance.reinforcements_completed,s.water.canal.as_slice().iter().filter(|&&v|v).count(),s.buildings.barges.len(),s.buildings.barges.iter().map(|b|b.carried).sum::<f32>());
    for (name, data) in [("summary.txt",summary),("homes.csv",homes),("beavers.csv",beavers),("buildings.csv",buildings),("events.txt",events),("run.txt",out.lines.join("\n")),("barges.csv",barges)] {
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

const FILES: [&str; 15] = ["01-overview.png", "02-water.png", "03-families.png", "04-jobs.png", "05-buildings.png", "06-downstream.png", "07-parentage.png", "08-household.png", "09-families-page-two.png", "10-network.png", "11-quarry-port.png", "12-stone-lodge.png", "13-barge-service.png", "14-resources.png", "15-fog-boundary.png"];

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
                    hud.colony = None;
                    canopy.0 = false;
                    if let Some(home)=sim.0.buildings.buildings.iter().find(|b|b.is_complete()&&b.kind==sim::buildings::BuildingKind::Lodge) {
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
        hud.colony = match shot { 2 | 8 => Some(ColonyTab::Population), 3 => Some(ColonyTab::Jobs), 4 => Some(ColonyTab::Buildings), 13 => Some(ColonyTab::Goods), _ => None };
        hud.pop_page = usize::from(shot == 8);
        *lens = if shot == 1 || shot == 9 { MapLens::Water } else if shot==12 {MapLens::Goods} else { MapLens::World };
        *selection = Selection::None;
        if shot == 6 {
            if let Some(b) = sim.0.colony.beavers.iter().find(|b| b.pregnancy.is_some())
                .or_else(|| sim.0.colony.beavers.iter().find(|b| b.mother.is_some())) {
                *selection = Selection::Beaver(b.id);
            }
        } else if shot == 7 {
            if let Some(b) = sim.0.buildings.buildings.iter().find(|b| b.is_complete() && b.kind == sim::buildings::BuildingKind::Lodge) {
                *selection = Selection::Site(b.id);
            }
        } else if shot==11 {
            if let Some(b)=sim.0.buildings.buildings.iter().find(|b|b.reinforced && b.kind==sim::buildings::BuildingKind::Lodge) {*selection=Selection::Site(b.id);}
        } else if shot==12 {
            if let Some(b)=sim.0.buildings.barges.iter().max_by(|a,b|a.carried.total_cmp(&b.carried)) {*selection=Selection::Barge(b.id);}
        }
        let homes: Vec<_> = sim.0.buildings.buildings.iter().filter(|b| b.is_complete()
            && matches!(b.kind, sim::buildings::BuildingKind::Lodge | sim::buildings::BuildingKind::Burrow)).collect();
        let focus = if shot == 5 { sim.0.buildings.buildings.iter().filter(|b| b.kind == sim::buildings::BuildingKind::Dam)
            .max_by_key(|b| b.pos.1).map(|b| b.pos) } else { homes.first().map(|b| b.pos) };
        if let Some((x,y)) = focus {
            orbit.target = crate::flow::cell_world(&sim.0.height, (x as f32, y as f32));
            orbit.distance = if shot == 5 { 50.0 } else { 65.0 };
            orbit.pitch = 1.1;
        }
        if shot==9 {
            let dams:Vec<_>=sim.0.buildings.buildings.iter().filter(|b|b.kind==sim::buildings::BuildingKind::Dam).collect();
            if !dams.is_empty(){let x=dams.iter().map(|b|b.pos.0 as f32).sum::<f32>()/dams.len()as f32;let y=dams.iter().map(|b|b.pos.1 as f32).sum::<f32>()/dams.len()as f32;
                orbit.target=crate::flow::cell_world(&sim.0.height,(x,y));orbit.distance=230.0;orbit.pitch=1.35;}
        }
        if shot==14 {orbit.distance=300.0;orbit.pitch=1.35;}
        let close=match shot {
            10=>sim.0.buildings.buildings.iter().filter(|b|b.kind==sim::buildings::BuildingKind::Dock).max_by_key(|b|b.pos.1).map(|b|b.pos),
            11=>sim.0.buildings.buildings.iter().find(|b|b.reinforced&&b.kind==sim::buildings::BuildingKind::Lodge).map(|b|b.pos),
            12=>sim.0.buildings.barges.iter().max_by(|a,b|a.carried.total_cmp(&b.carried)).map(|b|b.cell),
            _=>None,
        };
        if let Some((x,y))=close{orbit.target=crate::flow::cell_world(&sim.0.height,(x as f32,y as f32));orbit.distance=if shot==11{28.0}else{55.0};}
    }
    if (*frame - 120) % 60 == 40 {
        // A real map has more chunks than forty frames can recolour. Wait for
        // both mesh queues and several upload frames before capturing a lens.
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
