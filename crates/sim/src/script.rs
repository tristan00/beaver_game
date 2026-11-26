use std::collections::{BTreeMap, BTreeSet};

use crate::agents::Assignment;
use crate::buildings::{BuildingId, BuildingKind, Good};
use crate::colony::{BeaverId, DeathCause, Event, Stage};
use crate::jobs::Job;
use crate::maintenance::Refit;
use crate::resources::{ResourceId, ResourceKind};
use crate::society::{Answer, RequestId};
use crate::starts::DamSite;
use crate::water::Cell;
use crate::work::Area;
use crate::{Command, SimState, STEP_DAYS};

fn cells_str(v: &[Cell]) -> String { v.iter().map(|c| format!("{},{}", c.0, c.1)).collect::<Vec<_>>().join(" ") }
fn ids_str(v: &[BeaverId]) -> String { v.iter().map(|b| b.0.to_string()).collect::<Vec<_>>().join(",") }
fn flag(b: bool) -> &'static str { if b { "on" } else { "off" } }

fn parse_kind(s: &str) -> Result<BuildingKind, String> {
    BuildingKind::all().find(|k| k.key().eq_ignore_ascii_case(s)).ok_or_else(|| format!("unknown building '{s}'"))
}
fn parse_resource_kind(s: &str) -> Result<ResourceKind, String> {
    ResourceKind::all().find(|k| format!("{k:?}").eq_ignore_ascii_case(s)).ok_or_else(|| format!("unknown resource '{s}'"))
}
fn parse_role(s: &str) -> Result<Job, String> {
    Job::all().find(|k| k.key().eq_ignore_ascii_case(s)).ok_or_else(|| format!("unknown role '{s}'"))
}

pub fn to_line(c: &Command) -> String {
    match c {
        Command::Regenerate { seed } => format!("regenerate {seed}"),
        Command::BuildDam { cells: v, height } => format!("build-dam {height} {}", cells_str(v)),
        Command::PlaceDam { cells: v } => format!("dam {}", cells_str(v)),
        Command::GrowDam { dam, growth, cells: v } => format!("grow-dam {} {growth:?} {}", dam.0, cells_str(v)),
        Command::SetUpkeep { building, on } => format!("upkeep {} {}", building.0, flag(*on)),
        Command::Refit { building, refit: Some(Refit::Repair | Refit::ReinforcedRepair) } => format!("repair {}", building.0),
        Command::Refit { building, refit: Some(Refit::Reinforce) } => format!("reinforce {}", building.0),
        Command::Refit { building, refit: Some(Refit::Facing) } => format!("pebble {}", building.0),
        Command::Refit { building, refit: None } => format!("cancel-refit {}", building.0),
        Command::SetSlotOff { building, slot, off } => format!("slot-off {} {slot} {}", building.0, flag(*off)),
        Command::Settle { cell: c } => format!("settle {} {}", c.0, c.1),
        Command::BreachDam { x, y } => format!("breach {x} {y}"),
        Command::DigCanal { cells: v, depth } => format!("dig-canal {depth} {}", cells_str(v)),
        Command::PlaceCanal { cells: v } => format!("canal {}", cells_str(v)),
        Command::GrowCanal { canal, growth, cells: v } => format!("grow-canal {} {growth:?} {}", canal.0, cells_str(v)),
        Command::AddDamGate { dam, cell } => format!("dam-gate {} {},{}", dam.0, cell.0, cell.1),
        Command::SetGate { building, cell, setting } => match setting {
            crate::gates::GateSetting::Level(e) => format!("gate {} {},{} level {e}", building.0, cell.0, cell.1),
            s => format!("gate {} {},{} {}", building.0, cell.0, cell.1, s.name()),
        },
        Command::UpgradeLock { building, cell } => format!("lock {} {},{}", building.0, cell.0, cell.1),
        Command::RemoveGate { building, cell } => format!("ungate {} {},{}", building.0, cell.0, cell.1),
        Command::Assign { who, to, queue } => {
            let to = match to {
                None => "none".into(),
                Some(Assignment::Resource(r)) => format!("resource {}", r.0),
                Some(Assignment::Area(k, a)) => format!("area {k:?} {} {} {} {}", a.min.0, a.min.1, a.max.0, a.max.1),
                Some(Assignment::Work(b)) => format!("work {}", b.0),
                Some(Assignment::Go(c)) => format!("go {} {}", c.0, c.1),
                Some(Assignment::Mend(None)) => "upkeep".into(),
                Some(Assignment::Mend(Some(b))) => format!("upkeep {}", b.0),
                Some(Assignment::Build(None)) => "build".into(),
                Some(Assignment::Build(Some(b))) => format!("build {}", b.0),
            };
            format!("{} {} {to}", if *queue { "queue" } else { "assign" }, ids_str(who))
        }
        Command::PlaceSite { kind, pos } => format!("place {kind:?} {} {}", pos.0, pos.1),
        Command::StaffBuilding { building, delta } => format!("staff {} {delta}", building.0),
        Command::SetPaused { building, paused } => format!("pause {} {}", building.0, flag(*paused)),
        Command::AdjustRole { role, delta } => format!("role {role:?} {delta}"),
        Command::SetShoreLimit { role, cells } => format!("shore {role:?} {}", cells.map_or("none".into(), |n| n.to_string())),
        Command::SetBalance { building, on } => format!("balance {} {}", building.0, flag(*on)),
        Command::FillStore { store, on } => format!("fill-store {} {}", store.0, flag(*on)),
        Command::AnswerJoin { id, answer } => format!("answer {} {}", id.0, if *answer == Answer::Accept { "accept" } else { "decline" }),
        Command::CancelSite { building } => format!("cancel {}", building.0),
    }
}

fn num<T: std::str::FromStr>(s: Option<&&str>, what: &str) -> Result<T, String> {
    s.ok_or_else(|| format!("missing {what}"))?.parse::<T>().map_err(|_| format!("bad {what} '{}'", s.unwrap()))
}
fn parse_cell(t: &str) -> Result<Cell, String> {
    let (a, b) = t.split_once(',').ok_or_else(|| format!("bad cell '{t}' (want x,y)"))?;
    Ok((a.parse().map_err(|_| format!("bad cell '{t}'"))?, b.parse().map_err(|_| format!("bad cell '{t}'"))?))
}
fn parse_cells(v: &[&str]) -> Result<Vec<Cell>, String> { v.iter().map(|t| parse_cell(t)).collect() }
fn parse_beavers(s: Option<&&str>) -> Result<Vec<BeaverId>, String> {
    s.ok_or("missing beavers")?.split(',').map(|x| x.parse().map(BeaverId).map_err(|_| format!("bad beaver '{x}'"))).collect()
}
fn parse_area(t: &[&str], i: usize) -> Result<Area, String> {
    Ok(Area { min: (num(t.get(i), "x0")?, num(t.get(i + 1), "y0")?), max: (num(t.get(i + 2), "x1")?, num(t.get(i + 3), "y1")?) })
}

pub fn parse_command(line: &str) -> Result<Command, String> {
    let t: Vec<&str> = line.split_whitespace().collect();
    let b = |i| Ok::<_, String>(BuildingId(num(t.get(i), "building id")?));
    let on = |i| match t.get(i).copied() { Some("on" | "true") => Ok(true), Some("off" | "false") => Ok(false), o => Err(format!("want on/off, got {o:?}")) };
    let cell_at = |i| parse_cell(*t.get(i).ok_or("missing cell")?);
    let head = *t.first().ok_or("empty command")?;
    Ok(match head {
        "regenerate" => Command::Regenerate { seed: num(t.get(1), "seed")? },
        "build-dam" => Command::BuildDam { height: num(t.get(1), "height")?, cells: parse_cells(&t[2..])? },
        "dam" => Command::PlaceDam { cells: parse_cells(&t[1..])? },
        "grow-dam" => Command::GrowDam {
            dam: b(1)?,
            growth: [crate::DamGrowth::Raise, crate::DamGrowth::Widen, crate::DamGrowth::Extend].into_iter()
                .find(|g| t.get(2).is_some_and(|x| format!("{g:?}").eq_ignore_ascii_case(x))).ok_or("grow-dam takes raise, widen or extend")?,
            cells: parse_cells(&t[3..])?,
        },
        "upkeep" | "dam-upkeep" | "canal-upkeep" => Command::SetUpkeep { building: b(1)?, on: on(2)? },
        "repair" => Command::Refit { building: b(1)?, refit: Some(Refit::Repair) },
        "reinforce" => Command::Refit { building: b(1)?, refit: Some(Refit::Reinforce) },
        "pebble" => Command::Refit { building: b(1)?, refit: Some(Refit::Facing) },
        "cancel-refit" => Command::Refit { building: b(1)?, refit: None },
        "slot-off" => Command::SetSlotOff { building: b(1)?, slot: num(t.get(2), "slot")?, off: on(3)? },
        "settle" => Command::Settle { cell: (num(t.get(1), "x")?, num(t.get(2), "y")?) },
        "breach" => Command::BreachDam { x: num(t.get(1), "x")?, y: num(t.get(2), "y")? },
        "dig-canal" => Command::DigCanal { depth: num(t.get(1), "depth")?, cells: parse_cells(&t[2..])? },
        "canal" => Command::PlaceCanal { cells: parse_cells(&t[1..])? },
        "grow-canal" => Command::GrowCanal {
            canal: b(1)?,
            growth: [crate::canals::CanalGrowth::Deepen, crate::canals::CanalGrowth::Widen, crate::canals::CanalGrowth::Extend].into_iter()
                .find(|g| t.get(2).is_some_and(|x| format!("{g:?}").eq_ignore_ascii_case(x))).ok_or("grow-canal takes deepen, widen or extend")?,
            cells: parse_cells(&t[3..])?,
        },
        "dam-gate" => Command::AddDamGate { dam: b(1)?, cell: cell_at(2)? },
        "ungate" => Command::RemoveGate { building: b(1)?, cell: cell_at(2)? },
        "lock" => Command::UpgradeLock { building: b(1)?, cell: cell_at(2)? },
        "gate" => Command::SetGate {
            building: b(1)?, cell: cell_at(2)?,
            setting: match t.get(3).copied() {
                Some("shut") => crate::gates::GateSetting::Shut,
                Some("open") => crate::gates::GateSetting::Open,
                Some("level") => crate::gates::GateSetting::Level(num(t.get(4), "level")?),
                o => return Err(format!("gate takes shut, open or level N, got {o:?}")),
            },
        },
        "assign" | "queue" => {
            let queue = head == "queue";
            let to = match t.get(2).copied() {
                Some("none") if !queue => None,
                Some("resource") => Some(Assignment::Resource(ResourceId(num(t.get(3), "resource id")?))),
                Some("area") => Some(Assignment::Area(parse_resource_kind(t.get(3).ok_or("missing kind")?)?, parse_area(&t, 4)?)),
                Some("work" | "slot") => Some(Assignment::Work(b(3)?)),
                Some("go") => Some(Assignment::Go((num(t.get(3), "x")?, num(t.get(4), "y")?))),
                Some("upkeep") => Some(Assignment::Mend(t.get(3).map(|_| b(3)).transpose()?)),
                Some("build") => Some(Assignment::Build(t.get(3).map(|_| b(3)).transpose()?)),
                o => return Err(format!("unknown assignment {o:?}")),
            };
            Command::Assign { who: parse_beavers(t.get(1))?, to, queue }
        }
        "place" => Command::PlaceSite { kind: parse_kind(t.get(1).ok_or("missing kind")?)?, pos: (num(t.get(2), "x")?, num(t.get(3), "y")?) },
        "staff" => Command::StaffBuilding { building: b(1)?, delta: num(t.get(2), "delta")? },
        "add-beaver" => Command::StaffBuilding { building: b(1)?, delta: 1 },
        "remove-beaver" => Command::StaffBuilding { building: b(1)?, delta: -1 },
        "pause" => Command::SetPaused { building: b(1)?, paused: on(2)? },
        "shore" => Command::SetShoreLimit {
            role: parse_role(t.get(1).ok_or("missing role")?)?,
            cells: if t.get(2) == Some(&"none") { None } else { Some(num(t.get(2), "cells")?) },
        },
        "role" => Command::AdjustRole { role: parse_role(t.get(1).ok_or("missing role")?)?, delta: num(t.get(2), "delta")? },
        "balance" => Command::SetBalance { building: b(1)?, on: on(2)? },
        "fill-store" => Command::FillStore { store: b(1)?, on: on(2)? },
        "answer" => Command::AnswerJoin {
            id: RequestId(num(t.get(1), "request id")?),
            answer: match t.get(2).copied() { Some("accept") => Answer::Accept, Some("decline") => Answer::TurnAway, o => return Err(format!("want accept/decline, got {o:?}")) },
        },
        "cancel" => Command::CancelSite { building: b(1)? },
        other => return Err(format!("unknown command '{other}'")),
    })
}

pub fn write_log(log: &[(u64, Command)]) -> String { log.iter().map(|(s, c)| format!("{s} {}\n", to_line(c))).collect() }

pub fn parse_log(text: &str) -> Result<Vec<(u64, Command)>, String> {
    text.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with('#')).enumerate().map(|(i, l)| {
        let (s, rest) = l.split_once(' ').ok_or_else(|| format!("line {}: want 'step command'", i + 1))?;
        Ok((s.parse().map_err(|_| format!("line {}: bad step '{s}'", i + 1))?, parse_command(rest).map_err(|e| format!("line {}: {e}", i + 1))?))
    }).collect()
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
enum Stmt { Settle(Option<Cell>, bool), Place(BuildingKind, Option<Cell>), DesignedDam, FixTop, Do(Command) }

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct Stats {
    pub deaths: BTreeMap<String, u32>, pub births: u32, pub food_zero_days: u32, pub homeless_days: u32,
    pub free_beaver_days: u32, pub min_food: f32, pub completed: Vec<(f64, BuildingKind)>,
    pub stalled: BTreeMap<u64, (BuildingKind, u32)>, pub top_alerts: Vec<(f64, String)>,
    pub alert_days: BTreeMap<String, u32>, pub refused: Vec<(f64, String)>, pub collapsed: Option<f64>,
}

#[derive(Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct Outcome { pub lines: Vec<String>, pub checks: Vec<(String, bool)>, pub stats: Stats, pub events: Vec<crate::colony::LoggedEvent> }
impl Outcome { pub fn all_ok(&self) -> bool { self.checks.iter().all(|c| c.1) } }

struct Runner {
    s: SimState, dam: Option<DamSite>, camp: Option<Cell>,
    at: Vec<(f64, Stmt)>, every: Vec<(f64, f64, Stmt)>, replay: Vec<(u64, Command)>,
    report_every: Option<f64>, next_report: f64, next_day: f64, done: BTreeSet<u64>,
    started: std::time::Instant, out: Outcome,
}

fn parse_stmt(t: &str) -> Result<Stmt, String> {
    let w: Vec<&str> = t.split_whitespace().collect();
    match w.as_slice() {
        ["settle", "designed"] => Ok(Stmt::Settle(None, false)),
        ["settle", "best"] => Ok(Stmt::Settle(None, true)),
        ["settle", x, y] => Ok(Stmt::Settle(Some((x.parse().map_err(|_| "bad x")?, y.parse().map_err(|_| "bad y")?)), false)),
        ["place", k, "near", "designed"] => Ok(Stmt::Place(parse_kind(k)?, None)),
        ["place", k, "near", x, y] => Ok(Stmt::Place(parse_kind(k)?, Some((x.parse().map_err(|_| "bad x")?, y.parse().map_err(|_| "bad y")?)))),
        ["dam", "designed"] => Ok(Stmt::DesignedDam),
        ["fix", "top"] => Ok(Stmt::FixTop),
        ["do", ..] => Ok(Stmt::Do(parse_command(t.trim_start().trim_start_matches("do").trim())?)),
        _ => Err(format!("unknown statement '{t}'")),
    }
}

impl Runner {
    fn note(&mut self, s: String) { self.out.lines.push(format!("day {:>6.1}: {s}", self.s.time_days)); }

    fn exec(&mut self, st: &Stmt) {
        let res: Result<String, String> = match st {
            Stmt::Settle(c, best) => match c.or_else(|| if *best { crate::site::best_site(&self.s) } else { self.dam.as_ref().map(|d| d.camp) }) {
                Some(c) => { self.camp = Some(c); self.s.apply(Command::Settle { cell: c }).map(|_| format!("settled at {c:?}")).map_err(|e| e.to_string()) }
                None => Err("no settling place".into()),
            },
            Stmt::Place(k, near) => crate::construction::place_best(&mut self.s, *k, near.or(self.camp).unwrap_or((0, 0)), &[]).map(|id| format!("placed {k:?} #{}", id.0)),
            Stmt::DesignedDam => match self.dam.clone() {
                Some(d) => self.s.apply(Command::PlaceDam { cells: d.cells }).map(|_| "drew the designed dam".into()).map_err(|e| e.to_string()),
                None => Err("no designed dam".into()),
            },
            Stmt::FixTop => match crate::hud::problems(&self.s).into_iter().next() {
                Some(p) => match p.fix {
                    crate::hud::Fix::Place(k) => crate::construction::place_best(&mut self.s, k, self.camp.unwrap_or((0, 0)), &[]).map(|_| format!("fix: placed {k:?}")),
                    crate::hud::Fix::OpenJobs | crate::hud::Fix::Focus => Ok(String::new()),
                    f => crate::hud::apply_fix(&mut self.s, f).map(|_| format!("fix: {f:?}")),
                },
                None => Ok(String::new()),
            },
            Stmt::Do(c) => self.s.apply(c.clone()).map(|_| to_line(c)).map_err(|e| e.to_string()),
        };
        match res {
            Ok(m) if !m.is_empty() => self.note(m),
            Ok(_) => {}
            Err(e) => { self.out.stats.refused.push((self.s.time_days, e.clone())); self.note(format!("refused: {e}")); }
        }
    }

    fn daily(&mut self) {
        let (s, st) = (&self.s, &mut self.out.stats);
        let food = s.buildings.stores.amount(Good::food());
        st.min_food = st.min_food.min(food);
        st.food_zero_days += u32::from(food < 0.5);
        st.homeless_days += u32::from(s.colony.is_founded() && s.colony.beavers.iter().any(|b| b.lodge.is_none()));
        st.free_beaver_days += crate::jobs::idle_reasons(s).iter().filter(|(_, r)| *r != crate::jobs::IdleReason::Resting).count() as u32;
        let ps = crate::hud::problems(s);
        for p in &ps { *st.alert_days.entry(format!("{:?}", p.kind)).or_default() += 1; }
        let top = ps.first().map_or_else(|| "-".into(), |p| format!("{:?} {:.0} fix={:?}", p.kind, p.value, p.fix));
        let key = |t: &str| t.split(' ').next().unwrap_or("").to_string() + t.split("fix=").nth(1).unwrap_or("");
        if st.top_alerts.last().is_none_or(|(_, last)| key(last) != key(&top)) { st.top_alerts.push((s.time_days, top)); }
        for b in &s.buildings.buildings {
            if b.is_complete() {
                if self.done.insert(b.id.0) { st.completed.push((s.time_days, b.kind)); }
            } else if let Some(site) = s.work.place(b.id).and_then(|p| p.site.as_ref())
                && site.needed.iter().any(|a| site.missing(a.0) > 0.0 && s.buildings.stores.amount(a.0) < 0.05)
            { st.stalled.entry(b.id.0).or_insert((b.kind, 0)).1 += 1; }
        }
    }

    fn report_line(&mut self) {
        let (s, st) = (&self.s, &self.s.buildings.stores);
        let line = format!(
            "day {:>4.0} {:<6} pop {:>2} (kits {}) homeless {} | food {:>5.1} (+{:.1} -{:.1}/d need {:.1}) jobs {} | {} | built {} sites {} dry {} | rafts 0 carried 0 | mature trees {} | alerts {}",
            s.time_days, crate::calendar::Date::at(s.time_days).season.name(), s.colony.population(), s.colony.count_stage(Stage::Kit),
            s.colony.beavers.iter().filter(|b| b.lodge.is_none()).count(),
            st.amount(Good::food()), s.ledger.in_per_day(Good::food()), s.ledger.out_per_day(Good::food()), crate::hud::food_need_per_day(s),
            crate::jobs::jobs_board(s).iter().filter(|r| r.filled > 0).map(|r| format!("{:?}{}", r.role, r.filled)).collect::<Vec<_>>().join(","),
            Good::all().filter(|g| !g.row().edible).map(|g| format!("{}={:.1}", g.key().to_lowercase(), st.amount(g))).collect::<Vec<_>>().join(" "),
            s.buildings.buildings.iter().filter(|b| b.is_complete()).count(), s.buildings.buildings.iter().filter(|b| !b.is_complete()).count(),
            s.buildings.buildings.iter().filter(|b| s.buildings.stranded(b, &s.water.depth)).map(|b| format!("{:?}", b.kind)).collect::<Vec<_>>().join(","),
            metric(s, &self.out.stats, "mature").unwrap_or(0.0),
            crate::hud::problems(s).iter().map(|p| format!("{:?}", p.kind)).collect::<Vec<_>>().join(",")
        );
        if std::env::var_os("PLAY_LIVE").is_some() { eprintln!("[{:>6.1}s] {line}", self.started.elapsed().as_secs_f32()); }
        self.out.lines.push(line);
    }

    fn run_to(&mut self, day: f64) {
        while self.s.time_days + 1e-9 < day {
            while self.replay.first().is_some_and(|(st, _)| *st <= self.s.steps) {
                let (_, c) = self.replay.remove(0);
                if let Err(e) = self.s.apply(c.clone()) { self.note(format!("replay refused '{}': {e}", to_line(&c))); }
            }
            if self.s.time_days + 1e-9 >= self.next_day {
                self.next_day += 1.0;
                let now = self.s.time_days;
                let due: Vec<Stmt> = self.at.iter().filter(|(d, _)| *d <= now + 1e-9).map(|(_, s)| s.clone()).collect();
                self.at.retain(|(d, _)| *d > now + 1e-9);
                for st in due { self.exec(&st); }
                let mut rep = Vec::new();
                for e in &mut self.every { if now + 1e-9 >= e.1 { e.1 += e.0; rep.push(e.2.clone()); } }
                for st in rep { self.exec(&st); }
                self.daily();
                if let Some(n) = self.report_every && now + 1e-9 >= self.next_report { self.next_report += n; self.report_line(); }
            }
            if let Err(e) = self.s.step(STEP_DAYS) { self.note(format!("step failed: {e}")); return; }
            for e in self.s.colony.take_events() {
                self.out.events.push(e.clone());
                match e.event {
                    Event::Died { cause, .. } => *self.out.stats.deaths.entry(format!("{cause:?}")).or_default() += 1,
                    Event::Born { .. } => self.out.stats.births += 1,
                    _ => {}
                }
            }
            if self.s.colony.is_collapsed() && self.out.stats.collapsed.is_none() {
                self.out.stats.collapsed = Some(self.s.time_days);
                self.note("COLLAPSED".into());
            }
        }
    }
}

pub fn metric(s: &SimState, st: &Stats, name: &str) -> Option<f64> {
    Some(match name {
        "pop" => s.colony.population() as f64,
        "starved" => *st.deaths.get(&format!("{:?}", DeathCause::Starvation)).unwrap_or(&0) as f64,
        "deaths" => st.deaths.values().sum::<u32>() as f64,
        "births" => st.births as f64,
        n if let Some(good) = Good::all().find(|g| g.key().eq_ignore_ascii_case(n)) => s.buildings.stores.amount(good) as f64,
        "buildings" => s.buildings.buildings.iter().filter(|b| b.is_complete()).count() as f64,
        "sites" => s.buildings.buildings.iter().filter(|b| !b.is_complete()).count() as f64,
        "homeless" => s.colony.beavers.iter().filter(|b| b.lodge.is_none()).count() as f64,
        "day" => s.time_days,
        "food_zero_days" => st.food_zero_days as f64,
        "mature" => crate::forestry::colony_mature_trees(s).unwrap_or(0) as f64,
        _ => return None,
    })
}

pub fn run_script(text: &str, load: &dyn Fn(&str) -> Result<String, String>) -> Result<Outcome, String> { run_script_with(text, load, &mut |_| {}) }

pub fn run_script_with(text: &str, load: &dyn Fn(&str) -> Result<String, String>, last: &mut dyn FnMut(&SimState)) -> Result<Outcome, String> {
    let (out, state, _) = run_to_state(text, load)?;
    last(&state);
    Ok(out)
}

pub fn run_to_state(text: &str, load: &dyn Fn(&str) -> Result<String, String>) -> Result<(Outcome, SimState, Option<DamSite>), String> {
    let mut r: Option<Runner> = None;
    for (n, raw) in text.lines().enumerate() {
        let line = raw.split('#').next().unwrap_or("").trim();
        if line.is_empty() { continue; }
        let err = |e: String| format!("line {}: {e}", n + 1);
        let w: Vec<&str> = line.split_whitespace().collect();
        if r.is_none() {
            let (mut s, dam) = match w.as_slice() {
                ["map", m] => crate::starts::new_game_on_map(m).map_err(|e| err(format!("{e:?}")))?,
                ["seed", x] => crate::starts::new_game_with_dam(x.parse().map_err(|_| err("bad seed".into()))?).map_err(|e| err(format!("{e:?}")))?,
                _ => return Err(err("a script starts with 'map NAME' or 'seed N'".into())),
            };
            s.recorder = Some(Vec::new());
            r = Some(Runner {
                s, dam: Some(dam), camp: None, at: Vec::new(), every: Vec::new(), replay: Vec::new(),
                report_every: None, next_report: 0.0, next_day: 0.0, done: BTreeSet::new(),
                started: std::time::Instant::now(), out: Outcome { stats: Stats { min_food: f32::MAX, ..Default::default() }, ..Default::default() },
            });
            continue;
        }
        let run = r.as_mut().unwrap();
        match w.as_slice() {
            ["report", "every", d] => { run.report_every = Some(d.parse().map_err(|_| err("bad days".into()))?); run.next_report = run.s.time_days; }
            ["run", d] => run.run_to(d.parse().map_err(|_| err("bad days".into()))?),
            ["replay", f] => { run.replay = parse_log(&load(f).map_err(&err)?).map_err(&err)?; run.run_to(run.s.time_days); }
            ["check", "alive"] => run.out.checks.push(("alive".into(), run.out.stats.collapsed.is_none() && run.s.colony.population() > 0)),
            ["check", m, op, v] => {
                let have = metric(&run.s, &run.out.stats, m).ok_or_else(|| err(format!("unknown metric '{m}'")))?;
                let want: f64 = v.parse().map_err(|_| err("bad value".into()))?;
                let ok = match *op { ">=" => have >= want, "<=" => have <= want, ">" => have > want, "<" => have < want, "==" => (have - want).abs() < 1e-9, o => return Err(err(format!("unknown operator '{o}'"))) };
                run.out.checks.push((format!("{m} {op} {v} (is {have:.1})"), ok));
            }
            _ => if let Some(rest) = line.strip_prefix("at ") {
                let (d, st) = rest.split_once(':').ok_or_else(|| err("want 'at DAY: statement'".into()))?;
                run.at.push((d.trim().parse().map_err(|_| err("bad day".into()))?, parse_stmt(st.trim()).map_err(&err)?));
            } else if let Some(rest) = line.strip_prefix("every ") {
                let (d, st) = rest.split_once(':').ok_or_else(|| err("want 'every DAYS: statement'".into()))?;
                run.every.push((d.trim().parse().map_err(|_| err("bad days".into()))?, run.s.time_days, parse_stmt(st.trim()).map_err(&err)?));
            } else {
                let st = parse_stmt(line).map_err(&err)?;
                run.exec(&st);
            }
        }
    }
    let mut run = r.ok_or("empty script")?;
    let st = &run.out.stats;
    run.out.lines.push(format!(
        "SUMMARY day {:.0}: pop {} births {} deaths {:?} | food<=0 on {} days, min food {:.1} | homeless-days {} | free-beaver-days {} | built {} | {}",
        run.s.time_days, run.s.colony.population(), st.births, st.deaths, st.food_zero_days,
        if st.min_food == f32::MAX { 0.0 } else { st.min_food }, st.homeless_days, st.free_beaver_days,
        st.completed.iter().map(|(d, k)| format!("{k:?}@{d:.0}")).collect::<Vec<_>>().join(" "),
        if st.collapsed.is_some() { "COLLAPSED" } else { "alive" }
    ));
    Ok((run.out, run.s, run.dam))
}
