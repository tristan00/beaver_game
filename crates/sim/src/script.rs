//! Scripting: drive the game through everything a player can do, headless and deterministic.
//!
//! - Every player action that changes the world is a [`Command`]. Each has a one-line text form
//!   ([`to_line`], [`parse_command`]); a recorded game (`SimState::recorder`) is a list of
//!   `step line` pairs ([`write_log`], [`parse_log`]) that replays step for step.
//! - A script ([`run_script`]) builds a start, settles, applies commands, scheduled or repeating
//!   statements and scripted players (`autoplay`), runs days, and checks results. It reports a
//!   timeline and a summary. `cargo run --release -p sim --example play -- file.script`.
//!
//! Script statements, one per line (`#` starts a comment):
//! ```text
//! map algonquin | seed 7             # the start (first statement)
//! policy attentive|ignores|handsoff   # a scripted player acting once a day
//! settle designed | settle best | settle X Y
//! place KIND near designed|X Y        # nearest buildable reachable cell
//! dam designed                        # order the start's designed dam
//! fix top                             # the top alert's fix, as its button does
//! do COMMAND                          # any command in its text form (see to_line)
//! at DAY: STATEMENT                   # once, on that day
//! every DAYS: STATEMENT               # repeatedly
//! replay FILE                         # recorded commands at their steps
//! report every DAYS                   # a timeline line every so often
//! run DAYS                            # advance to that day
//! check METRIC OP VALUE | check alive # METRIC: pop starved deaths births food logs sticks mud
//!                                     # stone rope buildings sites homeless day mature
//! ```
use crate::agents::Assignment;
use crate::autoplay::{Player, Style};
use crate::buildings::{BuildingId, BuildingKind, Good, Recipe};
use crate::colony::{BeaverId, DeathCause, Event, Stage};
use crate::jobs::RoleKind;
use crate::resources::{ResourceId, ResourceKind};
use crate::society::{Answer, RequestId};
use crate::starts::DamSite;
use crate::water::Cell;
use crate::work::{Area, HaulOrder, Priority};
use crate::{Command, SimState, STEP_DAYS};
use std::collections::BTreeMap;

// ───────────────────────────── commands as text ─────────────────────────────

fn cell(c: Cell) -> String {
    format!("{},{}", c.0, c.1)
}

fn cells(v: &[Cell]) -> String {
    v.iter().map(|c| cell(*c)).collect::<Vec<_>>().join(" ")
}

fn ids(v: &[BeaverId]) -> String {
    v.iter().map(|b| b.0.to_string()).collect::<Vec<_>>().join(",")
}

fn kind_name(k: BuildingKind) -> String {
    format!("{k:?}")
}

fn parse_kind(s: &str) -> Result<BuildingKind, String> {
    BuildingKind::ALL.into_iter().find(|k| format!("{k:?}").eq_ignore_ascii_case(s)).ok_or_else(|| format!("unknown building '{s}'"))
}

fn parse_resource_kind(s: &str) -> Result<ResourceKind, String> {
    [ResourceKind::Tree, ResourceKind::Felled, ResourceKind::Plant, ResourceKind::MudBank, ResourceKind::StoneOutcrop]
        .into_iter()
        .find(|k| format!("{k:?}").eq_ignore_ascii_case(s))
        .ok_or_else(|| format!("unknown resource '{s}'"))
}

fn parse_role(s: &str) -> Result<RoleKind, String> {
    RoleKind::ALL.into_iter().find(|k| format!("{k:?}").eq_ignore_ascii_case(s)).ok_or_else(|| format!("unknown role '{s}'"))
}

fn parse_good(s: &str) -> Result<Good, String> {
    Good::ALL.into_iter().find(|k| format!("{k:?}").eq_ignore_ascii_case(s)).ok_or_else(|| format!("unknown good '{s}'"))
}

/// The one-line text form of a command.
pub fn to_line(c: &Command) -> String {
    match c {
        Command::Regenerate { seed } => format!("regenerate {seed}"),
        Command::BuildDam { cells: v, height } => format!("build-dam {height} {}", cells(v)),
        Command::OrderDam { cells: v, height } => format!("dam {height} {}", cells(v)),
        Command::OrderDamTo { cells: v, crest } => format!("dam-to {crest} {}", cells(v)),
        Command::Settle { cell: c } => format!("settle {} {}", c.0, c.1),
        Command::BreachDam { x, y } => format!("breach {x} {y}"),
        Command::DigCanal { cells: v, depth } => format!("dig-canal {depth} {}", cells(v)),
        Command::Assign { who, to } => {
            let to = match to {
                None => "none".to_string(),
                Some(Assignment::Resource(r)) => format!("resource {}", r.0),
                Some(Assignment::Area(k, a)) => format!("area {k:?} {} {} {} {}", a.min.0, a.min.1, a.max.0, a.max.1),
                Some(Assignment::Slot(b, s)) => format!("slot {} {s}", b.0),
                Some(Assignment::Go(c)) => format!("go {} {}", c.0, c.1),
            };
            format!("assign {} {to}", ids(who))
        }
        Command::PlaceSite { kind, pos } => format!("place {} {} {}", kind_name(*kind), pos.0, pos.1),
        Command::FillSlot { building, slot, who } => format!("fill {} {slot} {}", building.0, who.0),
        Command::EmptySlot { building, slot } => format!("empty {} {slot}", building.0),
        Command::AddWorker { building } => format!("add-worker {}", building.0),
        Command::RemoveWorker { building } => format!("remove-worker {}", building.0),
        Command::SetPriority { building, priority } => format!("priority {} {priority:?}", building.0),
        Command::SetPaused { building, paused } => format!("pause {} {}", building.0, if *paused { "on" } else { "off" }),
        Command::AdjustRole { role, delta } => format!("role {role:?} {delta}"),
        Command::SetRoleOrder { order } => format!("role-order {}", order.iter().map(|r| format!("{r:?}")).collect::<Vec<_>>().join(",")),
        Command::SetAutoStaff { on } => format!("auto {}", if *on { "on" } else { "off" }),
        Command::Unlock { who } => format!("unlock {}", who.0),
        Command::SetWorkArea { building, area } => format!("work-area {} {} {} {} {}", building.0, area.min.0, area.min.1, area.max.0, area.max.1),
        Command::SetHaul { order } => format!("haul {} {} {:?}", order.from.0, order.to.0, order.good),
        Command::SetBalance { building, on } => format!("balance {} {}", building.0, if *on { "on" } else { "off" }),
        Command::SetBargeRoute { barge, from, to, goods } => {
            format!("barge-route {} {} {} {}", barge.0, from.0, to.0, goods.iter().map(|g| format!("{g:?}")).collect::<Vec<_>>().join(","))
        }
        Command::ClearBargeRoute { barge } => format!("barge-route {} none", barge.0),
        Command::AnswerJoin { id, answer } => format!("answer {} {}", id.0, if *answer == Answer::Accept { "accept" } else { "decline" }),
        Command::SetRecipe { building, recipe } => format!("recipe {} {}", building.0, recipe.map_or("none".to_string(), |r| format!("{r:?}"))),
        Command::OrderCanal { cells: v } => format!("canal {}", cells(v)),
        Command::CancelSite { building } => format!("cancel {}", building.0),
        Command::SetCutRule { building, rule } => format!("cut-rule {} {rule:?}", building.0),
        Command::SetPlantRule { building, rule } => format!("plant-rule {} {rule:?}", building.0),
    }
}

fn num<T: std::str::FromStr>(s: Option<&&str>, what: &str) -> Result<T, String> {
    s.ok_or_else(|| format!("missing {what}"))?.parse::<T>().map_err(|_| format!("bad {what} '{}'", s.unwrap()))
}

fn parse_cells(v: &[&str]) -> Result<Vec<Cell>, String> {
    v.iter()
        .map(|t| {
            let (a, b) = t.split_once(',').ok_or_else(|| format!("bad cell '{t}' (want x,y)"))?;
            Ok((a.parse().map_err(|_| format!("bad cell '{t}'"))?, b.parse().map_err(|_| format!("bad cell '{t}'"))?))
        })
        .collect()
}

/// Parses the text form of a command ([`to_line`]).
pub fn parse_command(line: &str) -> Result<Command, String> {
    let t: Vec<&str> = line.split_whitespace().collect();
    let b = |i: usize| -> Result<BuildingId, String> { Ok(BuildingId(num(t.get(i), "building id")?)) };
    let on = |i: usize| -> Result<bool, String> {
        match t.get(i).copied() {
            Some("on") | Some("true") => Ok(true),
            Some("off") | Some("false") => Ok(false),
            other => Err(format!("want on/off, got {other:?}")),
        }
    };
    let head = *t.first().ok_or("empty command")?;
    Ok(match head {
        "regenerate" => Command::Regenerate { seed: num(t.get(1), "seed")? },
        "build-dam" => Command::BuildDam { height: num(t.get(1), "height")?, cells: parse_cells(&t[2..])? },
        "dam" => Command::OrderDam { height: num(t.get(1), "height")?, cells: parse_cells(&t[2..])? },
        "dam-to" => Command::OrderDamTo { crest: num(t.get(1), "crest")?, cells: parse_cells(&t[2..])? },
        "settle" => Command::Settle { cell: (num(t.get(1), "x")?, num(t.get(2), "y")?) },
        "breach" => Command::BreachDam { x: num(t.get(1), "x")?, y: num(t.get(2), "y")? },
        "dig-canal" => Command::DigCanal { depth: num(t.get(1), "depth")?, cells: parse_cells(&t[2..])? },
        "canal" => Command::OrderCanal { cells: parse_cells(&t[1..])? },
        "assign" => {
            let who = t.get(1).ok_or("missing beavers")?.split(',').map(|s| s.parse().map(BeaverId).map_err(|_| format!("bad beaver '{s}'"))).collect::<Result<Vec<_>, _>>()?;
            let to = match t.get(2).copied() {
                Some("none") => None,
                Some("resource") => Some(Assignment::Resource(ResourceId(num(t.get(3), "resource id")?))),
                Some("area") => Some(Assignment::Area(
                    parse_resource_kind(t.get(3).ok_or("missing kind")?)?,
                    Area { min: (num(t.get(4), "x0")?, num(t.get(5), "y0")?), max: (num(t.get(6), "x1")?, num(t.get(7), "y1")?) },
                )),
                Some("slot") => Some(Assignment::Slot(b(3)?, num(t.get(4), "slot")?)),
                Some("go") => Some(Assignment::Go((num(t.get(3), "x")?, num(t.get(4), "y")?))),
                other => return Err(format!("unknown assignment {other:?}")),
            };
            Command::Assign { who, to }
        }
        "place" => Command::PlaceSite { kind: parse_kind(t.get(1).ok_or("missing kind")?)?, pos: (num(t.get(2), "x")?, num(t.get(3), "y")?) },
        "fill" => Command::FillSlot { building: b(1)?, slot: num(t.get(2), "slot")?, who: BeaverId(num(t.get(3), "beaver")?) },
        "empty" => Command::EmptySlot { building: b(1)?, slot: num(t.get(2), "slot")? },
        "add-worker" => Command::AddWorker { building: b(1)? },
        "remove-worker" => Command::RemoveWorker { building: b(1)? },
        "priority" => Command::SetPriority {
            building: b(1)?,
            priority: match t.get(2).map(|s| s.to_ascii_lowercase()).as_deref() {
                Some("low") => Priority::Low,
                Some("normal") => Priority::Normal,
                Some("high") => Priority::High,
                other => return Err(format!("unknown priority {other:?}")),
            },
        },
        "pause" => Command::SetPaused { building: b(1)?, paused: on(2)? },
        "role" => Command::AdjustRole { role: parse_role(t.get(1).ok_or("missing role")?)?, delta: num(t.get(2), "delta")? },
        "role-order" => Command::SetRoleOrder { order: t.get(1).ok_or("missing roles")?.split(',').map(parse_role).collect::<Result<_, _>>()? },
        "auto" => Command::SetAutoStaff { on: on(1)? },
        "unlock" => Command::Unlock { who: BeaverId(num(t.get(1), "beaver")?) },
        "work-area" => Command::SetWorkArea { building: b(1)?, area: Area { min: (num(t.get(2), "x0")?, num(t.get(3), "y0")?), max: (num(t.get(4), "x1")?, num(t.get(5), "y1")?) } },
        "barge-route" if t.get(2) == Some(&"none") => Command::ClearBargeRoute { barge: crate::buildings::BargeId(num(t.get(1), "barge")?) },
        "barge-route" => Command::SetBargeRoute {
            barge: crate::buildings::BargeId(num(t.get(1), "barge")?),
            from: b(2)?,
            to: b(3)?,
            goods: t.get(4).ok_or("missing goods")?.split(',').map(parse_good).collect::<Result<_, _>>()?,
        },
        "balance" => Command::SetBalance {
            building: b(1)?,
            on: match t.get(2).copied() {
                Some("on") => true,
                Some("off") => false,
                other => return Err(format!("balance takes on or off, not {other:?}")),
            },
        },
        "haul" => Command::SetHaul { order: HaulOrder { from: b(1)?, to: b(2)?, good: parse_good(t.get(3).ok_or("missing good")?)? } },
        "answer" => Command::AnswerJoin {
            id: RequestId(num(t.get(1), "request id")?),
            answer: match t.get(2).copied() {
                Some("accept") => Answer::Accept,
                Some("decline") => Answer::TurnAway,
                other => return Err(format!("want accept/decline, got {other:?}")),
            },
        },
        "recipe" => Command::SetRecipe {
            building: b(1)?,
            recipe: match t.get(2).map(|s| s.to_ascii_lowercase()).as_deref() {
                Some("none") => None,
                Some("rope") => Some(Recipe::Rope),
                Some("baskets") => Some(Recipe::Baskets),
                Some("tools") => Some(Recipe::Tools),
                other => return Err(format!("unknown recipe {other:?}")),
            },
        },
        "cancel" => Command::CancelSite { building: b(1)? },
        "cut-rule" => Command::SetCutRule {
            building: b(1)?,
            rule: [crate::forestry::CutRule::MatureOnly, crate::forestry::CutRule::ClearCut, crate::forestry::CutRule::Thin, crate::forestry::CutRule::Coppice]
                .into_iter()
                .find(|r| t.get(2).is_some_and(|x| format!("{r:?}").eq_ignore_ascii_case(x)))
                .ok_or("unknown cut rule")?,
        },
        "plant-rule" => Command::SetPlantRule {
            building: b(1)?,
            rule: [crate::forestry::PlantRule::FillGaps, crate::forestry::PlantRule::PlantAll, crate::forestry::PlantRule::TendOnly]
                .into_iter()
                .find(|r| t.get(2).is_some_and(|x| format!("{r:?}").eq_ignore_ascii_case(x)))
                .ok_or("unknown plant rule")?,
        },
        other => return Err(format!("unknown command '{other}'")),
    })
}

/// A recorded game as text: one `step command` line per command.
pub fn write_log(log: &[(u64, Command)]) -> String {
    log.iter().map(|(s, c)| format!("{s} {}\n", to_line(c))).collect()
}

/// Parses a recorded game ([`write_log`]).
pub fn parse_log(text: &str) -> Result<Vec<(u64, Command)>, String> {
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .enumerate()
        .map(|(i, l)| {
            let (s, rest) = l.split_once(' ').ok_or_else(|| format!("line {}: want 'step command'", i + 1))?;
            Ok((s.parse().map_err(|_| format!("line {}: bad step '{s}'", i + 1))?, parse_command(rest).map_err(|e| format!("line {}: {e}", i + 1))?))
        })
        .collect()
}

// ───────────────────────────── scripts ─────────────────────────────

#[derive(Clone, Debug)]
enum Stmt {
    Settle(Option<Cell>, bool),
    Place(BuildingKind, Option<Cell>),
    DesignedDam,
    FixTop,
    Do(Command),
}

/// What a run measured.
#[derive(Clone, Debug, Default)]
pub struct Stats {
    pub deaths: BTreeMap<String, u32>,
    pub births: u32,
    pub food_zero_days: u32,
    pub homeless_days: u32,
    pub free_beaver_days: u32,
    pub min_food: f32,
    /// Buildings finished, with the day.
    pub completed: Vec<(f64, BuildingKind)>,
    /// Sites waiting more than 10 days for a material the colony had none of.
    pub stalled: BTreeMap<u64, (BuildingKind, u32)>,
    pub techs: Vec<(f64, String)>,
    /// When the top alert changed, and to what.
    pub top_alerts: Vec<(f64, String)>,
    pub alert_days: BTreeMap<String, u32>,
    pub refused: Vec<(f64, String)>,
    pub collapsed: Option<f64>,
}

/// The outcome of a script.
#[derive(Debug, Default)]
pub struct Outcome {
    pub lines: Vec<String>,
    pub checks: Vec<(String, bool)>,
    pub stats: Stats,
}

impl Outcome {
    pub fn passed(&self) -> bool {
        self.checks.iter().all(|c| c.1)
    }
}

struct Runner {
    s: SimState,
    dam: Option<DamSite>,
    camp: Option<Cell>,
    player: Option<Player>,
    at: Vec<(f64, Stmt)>,
    every: Vec<(f64, f64, Stmt)>,
    replay: Vec<(u64, Command)>,
    report_every: Option<f64>,
    next_report: f64,
    next_day: f64,
    done: std::collections::BTreeSet<u64>,
    started: std::time::Instant,
    out: Outcome,
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
    fn note(&mut self, s: String) {
        let line = format!("day {:>6.1}: {s}", self.s.time_days);
        self.out.lines.push(line);
    }

    fn exec(&mut self, st: &Stmt) {
        let res: Result<String, String> = match st {
            Stmt::Settle(c, best) => {
                let target = match (c, best) {
                    (Some(c), _) => Some(*c),
                    (None, true) => crate::site::best_site(&self.s),
                    (None, false) => self.dam.as_ref().map(|d| d.camp),
                };
                match target {
                    Some(c) => {
                        self.camp = Some(c);
                        if let Some(p) = self.player.as_mut() {
                            p.camp = c;
                            if Some(c) != self.dam.as_ref().map(|d| d.camp) {
                                p.dam = None;
                            }
                        }
                        self.s.apply(Command::Settle { cell: c }).map(|_| format!("settled at {c:?}")).map_err(|e| e.to_string())
                    }
                    None => Err("no settling place".into()),
                }
            }
            Stmt::Place(k, near) => {
                let around = near.or(self.camp).unwrap_or((0, 0));
                crate::autoplay::place_best(&mut self.s, *k, around).map(|id| format!("placed {k:?} #{}", id.0))
            }
            Stmt::DesignedDam => match self.dam.clone() {
                Some(d) => self.s.apply(Command::OrderDam { cells: d.cells, height: d.height }).map(|_| "ordered the designed dam".into()).map_err(|e| e.to_string()),
                None => Err("no designed dam".into()),
            },
            Stmt::FixTop => match crate::hud::problems(&self.s).into_iter().next() {
                Some(p) => match p.fix {
                    crate::hud::Fix::Place(k) => crate::autoplay::place_best(&mut self.s, k, self.camp.unwrap_or((0, 0))).map(|_| format!("fix: placed {k:?}")),
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
            Err(e) => {
                let day = self.s.time_days;
                self.out.stats.refused.push((day, e.clone()));
                self.note(format!("refused: {e}"));
            }
        }
    }

    fn daily(&mut self) {
        let s = &self.s;
        let st = &mut self.out.stats;
        let food = s.buildings.stores.amount(Good::Food);
        st.min_food = st.min_food.min(food);
        if food < 0.5 {
            st.food_zero_days += 1;
        }
        if s.colony.beavers.iter().any(|b| b.lodge.is_none()) && s.colony.is_founded() {
            st.homeless_days += 1;
        }
        st.free_beaver_days += crate::jobs::idle_reasons(s).iter().filter(|(_, r)| *r != crate::jobs::IdleReason::Resting).count() as u32;
        let ps = crate::hud::problems(s);
        for p in &ps {
            *st.alert_days.entry(format!("{:?}", p.kind)).or_default() += 1;
        }
        let top = ps.first().map(|p| format!("{:?} {:.0} fix={:?}", p.kind, p.value, p.fix)).unwrap_or_else(|| "-".into());
        let key = |t: &str| t.split(' ').next().unwrap_or("").to_string() + t.split("fix=").nth(1).unwrap_or("");
        if st.top_alerts.last().is_none_or(|(_, last)| key(last) != key(&top)) {
            st.top_alerts.push((s.time_days, top));
        }
        for b in &s.buildings.buildings {
            if b.is_complete() {
                if self.done.insert(b.id.0) {
                    st.completed.push((s.time_days, b.kind));
                }
            } else if let Some(site) = s.work.place(b.id).and_then(|p| p.site.as_ref()) {
                if site.needed.iter().any(|a| site.missing(a.0) > 0.0 && s.buildings.stores.amount(a.0) < 0.05) {
                    st.stalled.entry(b.id.0).or_insert((b.kind, 0)).1 += 1;
                }
            }
        }
        for (i, t) in crate::tech::TECHS.iter().enumerate() {
            if s.tech.is_unlocked(crate::tech::TechId(i)) && !st.techs.iter().any(|(_, n)| n == t.name) {
                st.techs.push((s.time_days, t.name.to_string()));
            }
        }
    }

    fn report_line(&mut self) {
        let s = &self.s;
        let st = &s.buildings.stores;
        let line = format!(
            "day {:>4.0} {:<6} pop {:>2} (kits {}) homeless {} | food {:>5.1} (+{:.1} -{:.1}/d need {:.1}) jobs {} | logs {:>5.1} sticks {:>5.1} mud {:>4.1} stone {:>4.1} rope {:>3.1} | built {} sites {} dry {} | barges {} carried {:.0} | mature trees {} | alerts {}",
            s.time_days,
            crate::calendar::Date::at(s.time_days).season.name(),
            s.colony.population(),
            s.colony.count_stage(Stage::Kit),
            s.colony.beavers.iter().filter(|b| b.lodge.is_none()).count(),
            st.amount(Good::Food),
            s.ledger.in_per_day(Good::Food),
            s.ledger.out_per_day(Good::Food),
            crate::hud::food_need_per_day(s),
            crate::jobs::jobs_board(s).iter().filter(|r| r.filled > 0).map(|r| format!("{:?}{}", r.role, r.filled)).collect::<Vec<_>>().join(","),
            st.amount(Good::Logs),
            st.amount(Good::Sticks),
            st.amount(Good::Mud),
            st.amount(Good::Stone),
            st.amount(Good::Rope),
            s.buildings.buildings.iter().filter(|b| b.is_complete()).count(),
            s.buildings.buildings.iter().filter(|b| !b.is_complete()).count(),
            s.buildings.buildings.iter().filter(|b| s.buildings.stranded(b, &s.water.depth)).map(|b| format!("{:?}", b.kind)).collect::<Vec<_>>().join(","),
            s.buildings.barges.len(),
            s.buildings.barges.iter().map(|b| b.carried).sum::<f32>(),
            metric(s, &self.out.stats, "mature").unwrap_or(0.0),
            crate::hud::problems(s).iter().map(|p| format!("{:?}", p.kind)).collect::<Vec<_>>().join(",")
        );
        if std::env::var_os("PLAY_LIVE").is_some() {
            eprintln!("[{:>6.1}s] {line}", self.started.elapsed().as_secs_f32());
        }
        self.out.lines.push(line);
    }

    fn run_to(&mut self, day: f64) {
        while self.s.time_days + 1e-9 < day {
            // Replayed commands at their exact step.
            while self.replay.first().is_some_and(|(st, _)| *st <= self.s.steps) {
                let (_, c) = self.replay.remove(0);
                if let Err(e) = self.s.apply(c.clone()) {
                    self.note(format!("replay refused '{}': {e}", to_line(&c)));
                }
            }
            if self.s.time_days + 1e-9 >= self.next_day {
                self.next_day += 1.0;
                let now = self.s.time_days;
                let due: Vec<Stmt> = self.at.iter().filter(|(d, _)| *d <= now + 1e-9).map(|(_, s)| s.clone()).collect();
                self.at.retain(|(d, _)| *d > now + 1e-9);
                for st in due {
                    self.exec(&st);
                }
                let mut rep = Vec::new();
                for e in self.every.iter_mut() {
                    if now + 1e-9 >= e.1 {
                        e.1 += e.0;
                        rep.push(e.2.clone());
                    }
                }
                for st in rep {
                    self.exec(&st);
                }
                if let Some(mut p) = self.player.take() {
                    let before = p.log.len();
                    p.act(&mut self.s);
                    let new: Vec<String> = p.log[before..].to_vec();
                    self.player = Some(p);
                    for l in new {
                        self.out.lines.push(l);
                    }
                }
                self.daily();
                if let Some(n) = self.report_every {
                    if now + 1e-9 >= self.next_report {
                        self.next_report += n;
                        self.report_line();
                    }
                }
            }
            if let Err(e) = self.s.step(STEP_DAYS) {
                self.note(format!("step failed: {e}"));
                return;
            }
            for e in self.s.colony.take_events() {
                match e.event {
                    Event::Died { cause, .. } => *self.out.stats.deaths.entry(format!("{cause:?}")).or_default() += 1,
                    Event::Born { .. } => self.out.stats.births += 1,
                    _ => {}
                }
            }
            if self.s.colony.is_collapsed() && self.out.stats.collapsed.is_none() {
                self.out.stats.collapsed = Some(self.s.time_days);
                self.note("COLLAPSED".into());
                return;
            }
        }
    }
}

/// A named number about the colony, for `check` and reports.
pub fn metric(s: &SimState, st: &Stats, name: &str) -> Option<f64> {
    let g = |g: Good| s.buildings.stores.amount(g) as f64;
    Some(match name {
        "pop" => s.colony.population() as f64,
        "starved" => *st.deaths.get(&format!("{:?}", DeathCause::Starvation)).unwrap_or(&0) as f64,
        "deaths" => st.deaths.values().sum::<u32>() as f64,
        "births" => st.births as f64,
        "food" => g(Good::Food),
        "logs" => g(Good::Logs),
        "sticks" => g(Good::Sticks),
        "mud" => g(Good::Mud),
        "stone" => g(Good::Stone),
        "rope" => g(Good::Rope),
        "buildings" => s.buildings.buildings.iter().filter(|b| b.is_complete()).count() as f64,
        "sites" => s.buildings.buildings.iter().filter(|b| !b.is_complete()).count() as f64,
        "homeless" => s.colony.beavers.iter().filter(|b| b.lodge.is_none()).count() as f64,
        "day" => s.time_days,
        "food_zero_days" => st.food_zero_days as f64,
        "mature" => crate::forestry::colony_mature_trees(s).unwrap_or(0) as f64,
        _ => return None,
    })
}

/// Runs a script; `load` reads files named by `replay`.
pub fn run_script(text: &str, load: &dyn Fn(&str) -> Result<String, String>) -> Result<Outcome, String> {
    run_script_with(text, load, &mut |_| {})
}

/// Like [`run_script`], and hands the final state to `last` (for recordings and comparisons).
pub fn run_script_with(text: &str, load: &dyn Fn(&str) -> Result<String, String>, last: &mut dyn FnMut(&SimState)) -> Result<Outcome, String> {
    let mut r: Option<Runner> = None;
    for (n, raw) in text.lines().enumerate() {
        let line = raw.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        let err = |e: String| format!("line {}: {e}", n + 1);
        let w: Vec<&str> = line.split_whitespace().collect();
        if r.is_none() {
            let (s, dam) = match w.as_slice() {
                ["map", m] => crate::starts::new_game_on_map(m).map_err(|e| err(format!("{e:?}")))?,
                ["seed", x] => crate::starts::new_game_with_dam(x.parse().map_err(|_| err("bad seed".into()))?).map_err(|e| err(format!("{e:?}")))?,
                _ => return Err(err("a script starts with 'map NAME' or 'seed N'".into())),
            };
            r = Some(Runner {
                s,
                dam: Some(dam),
                camp: None,
                player: None,
                at: Vec::new(),
                every: Vec::new(),
                replay: Vec::new(),
                report_every: None,
                next_report: 0.0,
                next_day: 0.0,
                done: Default::default(),
                started: std::time::Instant::now(),
                out: Outcome { stats: Stats { min_food: f32::MAX, ..Default::default() }, ..Default::default() },
            });
            r.as_mut().unwrap().s.recorder = Some(Vec::new());
            continue;
        }
        let run = r.as_mut().unwrap();
        match w.as_slice() {
            ["policy", p] => {
                let style = match *p {
                    "attentive" => Style::Attentive,
                    "ignores" => Style::IgnoresAlerts,
                    "handsoff" => Style::HandsOff,
                    other => return Err(err(format!("unknown policy '{other}'"))),
                };
                let dam = run.dam.as_ref().map(|d| (d.cells.clone(), d.height));
                run.player = Some(Player::new(style, run.camp.or(run.dam.as_ref().map(|d| d.camp)).unwrap_or((0, 0)), dam));
            }
            ["report", "every", d] => {
                run.report_every = Some(d.parse().map_err(|_| err("bad days".into()))?);
                run.next_report = run.s.time_days;
            }
            ["run", d] => {
                let d: f64 = d.parse().map_err(|_| err("bad days".into()))?;
                run.run_to(d);
            }
            ["replay", f] => {
                run.replay = parse_log(&load(f).map_err(&err)?).map_err(&err)?;
                // Apply what was recorded before the first step now.
                run.run_to(run.s.time_days);
            }
            ["check", "alive"] => {
                let ok = run.out.stats.collapsed.is_none() && run.s.colony.population() > 0;
                run.out.checks.push(("alive".into(), ok));
            }
            ["check", m, op, v] => {
                let have = metric(&run.s, &run.out.stats, m).ok_or_else(|| err(format!("unknown metric '{m}'")))?;
                let want: f64 = v.parse().map_err(|_| err("bad value".into()))?;
                let ok = match *op {
                    ">=" => have >= want,
                    "<=" => have <= want,
                    ">" => have > want,
                    "<" => have < want,
                    "==" => (have - want).abs() < 1e-9,
                    other => return Err(err(format!("unknown operator '{other}'"))),
                };
                run.out.checks.push((format!("{m} {op} {v} (is {have:.1})"), ok));
            }
            _ => {
                if let Some(rest) = line.strip_prefix("at ") {
                    let (d, st) = rest.split_once(':').ok_or_else(|| err("want 'at DAY: statement'".into()))?;
                    run.at.push((d.trim().parse().map_err(|_| err("bad day".into()))?, parse_stmt(st.trim()).map_err(&err)?));
                } else if let Some(rest) = line.strip_prefix("every ") {
                    let (d, st) = rest.split_once(':').ok_or_else(|| err("want 'every DAYS: statement'".into()))?;
                    let d: f64 = d.trim().parse().map_err(|_| err("bad days".into()))?;
                    run.every.push((d, run.s.time_days, parse_stmt(st.trim()).map_err(&err)?));
                } else {
                    let st = parse_stmt(line).map_err(&err)?;
                    run.exec(&st);
                }
            }
        }
    }
    let mut run = r.ok_or("empty script")?;
    let st = &run.out.stats;
    let summary = format!(
        "SUMMARY day {:.0}: pop {} births {} deaths {:?} | food<=0 on {} days, min food {:.1} | homeless-days {} | free-beaver-days {} | built {} | {}",
        run.s.time_days,
        run.s.colony.population(),
        st.births,
        st.deaths,
        st.food_zero_days,
        if st.min_food == f32::MAX { 0.0 } else { st.min_food },
        st.homeless_days,
        st.free_beaver_days,
        st.completed.iter().map(|(d, k)| format!("{k:?}@{d:.0}")).collect::<Vec<_>>().join(" "),
        if st.collapsed.is_some() { "COLLAPSED" } else { "alive" }
    );
    run.out.lines.push(summary);
    last(&run.s);
    Ok(run.out)
}

/// Runs a script and returns its outcome with the game it recorded (a replayable log).
pub fn run_and_record(text: &str, load: &dyn Fn(&str) -> Result<String, String>) -> Result<(Outcome, String), String> {
    let mut log = String::new();
    let out = run_script_with(text, load, &mut |s| log = write_log(s.recorder.as_deref().unwrap_or(&[])))?;
    Ok((out, log))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_command_round_trips_through_text() {
        let samples = vec![
            Command::Settle { cell: (3, 4) },
            Command::PlaceSite { kind: BuildingKind::Warehouse, pos: (10, 12) },
            Command::OrderDam { cells: vec![(1, 2), (2, 2)], height: 1.5 },
            Command::OrderDamTo { cells: vec![(1, 2), (2, 2)], crest: 12.5 },
            Command::OrderCanal { cells: vec![(5, 5), (6, 5)] },
            Command::Assign { who: vec![BeaverId(1), BeaverId(2)], to: Some(Assignment::Go((8, 9))) },
            Command::Assign { who: vec![BeaverId(1)], to: Some(Assignment::Area(ResourceKind::Plant, Area { min: (1, 2), max: (3, 4) })) },
            Command::Assign { who: vec![BeaverId(3)], to: None },
            Command::AddWorker { building: BuildingId(4) },
            Command::SetPriority { building: BuildingId(4), priority: Priority::High },
            Command::SetPaused { building: BuildingId(4), paused: true },
            Command::AdjustRole { role: RoleKind::Forager, delta: -1 },
            Command::SetRoleOrder { order: vec![RoleKind::Forager, RoleKind::Builder] },
            Command::SetRecipe { building: BuildingId(9), recipe: Some(Recipe::Rope) },
            Command::AnswerJoin { id: RequestId(2), answer: Answer::TurnAway },
            Command::SetHaul { order: HaulOrder { from: BuildingId(1), to: BuildingId(2), good: Good::Logs } },
            Command::SetBalance { building: BuildingId(3), on: true },
            Command::SetBalance { building: BuildingId(3), on: false },
            Command::SetBargeRoute { barge: crate::buildings::BargeId(4), from: BuildingId(1), to: BuildingId(2), goods: vec![Good::Stone, Good::Mud] },
            Command::ClearBargeRoute { barge: crate::buildings::BargeId(4) },
            Command::CancelSite { building: BuildingId(7) },
            Command::SetCutRule { building: BuildingId(3), rule: crate::forestry::CutRule::Thin },
            Command::SetPlantRule { building: BuildingId(3), rule: crate::forestry::PlantRule::PlantAll },
        ];
        for c in samples {
            assert_eq!(parse_command(&to_line(&c)).unwrap(), c, "{}", to_line(&c));
        }
    }

    #[test]
    fn a_recorded_game_replays_to_the_same_state() {
        let script = "seed 1
settle designed
place Burrow near designed
place Warehouse near designed
dam designed
every 1: fix top
run 15
";
        let fingerprint = |s: &SimState| {
            let b: Vec<_> = s.buildings.buildings.iter().map(|b| (b.id.0, b.pos, b.is_complete())).collect();
            let a: Vec<_> = s.agents.by_id.values().map(|a| (a.id.0, a.cell)).collect();
            format!("{} {} {:?} {:?} {:.4}", s.steps, s.colony.population(), b, a, s.buildings.stores.amount(Good::Food))
        };
        let mut first = String::new();
        let (_, log) = {
            let mut log = String::new();
            let out = run_script_with(script, &|_| Err("no files".into()), &mut |s| {
                first = fingerprint(s);
                log = write_log(s.recorder.as_deref().unwrap());
            })
            .unwrap();
            (out, log)
        };
        assert!(log.lines().count() >= 4, "the game was recorded: {log}");
        let mut second = String::new();
        let out = run_script_with("seed 1
replay game.log
run 15
", &|_| Ok(log.clone()), &mut |s| second = fingerprint(s)).unwrap();
        assert!(out.stats.refused.is_empty(), "{:?}", out.stats.refused);
        assert_eq!(first, second, "the replay reaches the same state");
    }
}
