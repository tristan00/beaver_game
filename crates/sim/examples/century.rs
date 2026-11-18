//! Century run: four scripted players (careful, neglectful, overcutter, over-damming) each play the
//! same world for N years; one stats line per strategy per year shows how their choices diverge,
//! including losses by cause (starve, cold, predator, disease, flood, age, emigr) and food/wood stores.
//!
//! Usage: cargo run --release -p sim --example century -- [years=30] [seed=1]
//!
//! Strategies act only through the public `SimState` surface: `SimState::apply` commands
//! (dams, canals), `buildings.place` / `set_recipe`, `ecology.fell`, and `events.resolve`.
//!
//! `SimState::step` runs the colony on a built-in river and never rolls random events, so this
//! example advances the same subsystems itself (`advance`) with the real water grid as the colony's
//! landscape and with events rolled against the live world. The colony does not feed
//! `buildings.stores`, so felling and daily gathering stand in for hauling (`Player::fell`, `gather`).
//! Event "arrivals" cannot be applied (the colony exposes no immigration API) and are reported as such.
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Display;
use std::process::ExitCode;
use std::time::Instant;

use sim::buildings::{BuildError, BuildingKind, Good, Recipe, Status, Terrain};
use sim::colony::{BeaverId, Colony, DeathCause, Event, Job, Stats};
use sim::events::{Effect, EventContext, EventHost, EventKind, PendingChoice};
use sim::tech::TECHS;
use sim::water::Cell;
use sim::{Command, SimState};

const DAYS_PER_YEAR: u32 = sim::calendar::YEAR_DAYS as u32;
const SPIN_UP_DAYS: u32 = 40;
/// Day step used once a colony has collapsed (the world keeps evolving, nobody acts).
const COLLAPSED_STEP_DAYS: u32 = 5;
const SOURCE_RATE: f32 = 8.0;
/// Depth above which a cell counts as pond (the colony's own water threshold).
const WATER_DEPTH: f32 = 0.05;
const EVENT_DAYS_PER_DAY: f32 = 40.0 / 365.0;
const RAIN: f32 = 0.003;

// ───────────────────────────── site ─────────────────────────────

/// Where things happen in the start valley, derived once from the world.
struct Site {
    home: (usize, usize),
    /// River cells from the head of the valley's longest river downstream.
    river: Vec<(usize, usize)>,
    /// Dam sites as indices into `river`, in the order an enthusiastic dammer would use them.
    dam_order: Vec<usize>,
    /// Start-valley cells nearest `home` first.
    near_home: Vec<(usize, usize)>,
    in_valley: Vec<bool>,
    valley_cells: usize,
}

impl Site {
    /// Errors (never panics) when the start valley has no usable river or dry home.
    fn find(sim: &SimState) -> Result<Site, String> {
        let w = &sim.world;
        let n = w.size;
        let in_valley: Vec<bool> = w.valley.as_slice().iter().map(|&v| v == w.start).collect();
        let is_river = |x: usize, y: usize| w.river.get(x, y).copied().unwrap_or(false) && in_valley[y * n + x];
        let mut has_inflow = vec![false; n * n];
        let next = |x: usize, y: usize| w.downstream(x, y).filter(|&(a, b)| is_river(a, b));
        for y in 0..n {
            for x in 0..n {
                if is_river(x, y) {
                    if let Some((a, b)) = next(x, y) {
                        has_inflow[b * n + a] = true;
                    }
                }
            }
        }
        let mut best: Vec<(usize, usize)> = Vec::new();
        for y in 0..n {
            for x in 0..n {
                if !is_river(x, y) || has_inflow[y * n + x] {
                    continue;
                }
                let mut path = vec![(x, y)];
                while let Some(c) = next(path.last().unwrap().0, path.last().unwrap().1) {
                    if path.len() > n * n {
                        return Err("river path loops".into());
                    }
                    path.push(c);
                }
                if path.len() > best.len() {
                    best = path;
                }
            }
        }
        if best.len() < 12 {
            return Err(format!("start valley river too short ({} cells)", best.len()));
        }
        let len = best.len();
        let at = |f: f32| ((len as f32 * f) as usize).min(len - 2).max(1);
        let dam_order = [0.75, 0.66, 0.85, 0.5, 0.92, 0.4, 0.6, 0.3].iter().map(|&f| at(f)).collect();
        // Home: dry ground a few cells from the river, mid-valley.
        let mid = best[len / 2];
        let mut home = None;
        'search: for r in 4..=12i32 {
            for dy in -r..=r {
                for dx in -r..=r {
                    if dx.abs().max(dy.abs()) != r {
                        continue;
                    }
                    let (x, y) = (mid.0 as i32 + dx, mid.1 as i32 + dy);
                    if x < 2 || y < 2 || x >= n as i32 - 2 || y >= n as i32 - 2 {
                        continue;
                    }
                    let (x, y) = (x as usize, y as usize);
                    if in_valley[y * n + x] && sim.water.depth.get(x, y).is_some_and(|&d| d <= 0.05) {
                        home = Some((x, y));
                        break 'search;
                    }
                }
            }
        }
        let home = home.ok_or("no dry home site near the river")?;
        let mut near_home: Vec<(usize, usize)> =
            (0..n * n).filter(|&i| in_valley[i]).map(|i| (i % n, i / n)).collect();
        let d2 = |c: &(usize, usize)| (c.0 as i64 - home.0 as i64).pow(2) + (c.1 as i64 - home.1 as i64).pow(2);
        near_home.sort_by_key(|c| (d2(c), c.1, c.0));
        let valley_cells = near_home.len();
        Ok(Site { home, river: best, dam_order, near_home, in_valley, valley_cells })
    }

    /// A dam row of `2 * half + 1` cells across the river at river index `i`.
    fn dam_cells(&self, i: usize, half: i32, size: usize) -> Vec<Cell> {
        let (px, py) = self.river[i - 1];
        let (nx, ny) = self.river[(i + 1).min(self.river.len() - 1)];
        let horizontal_flow = (nx as i32 - px as i32).abs() >= (ny as i32 - py as i32).abs();
        let (x, y) = self.river[i];
        (-half..=half)
            .map(|k| if horizontal_flow { (x as i32, y as i32 + k) } else { (x as i32 + k, y as i32) })
            .filter(|&(a, b)| a >= 0 && b >= 0 && (a as usize) < size && (b as usize) < size)
            .map(|(a, b)| (a as usize, b as usize))
            .collect()
    }
}

// ───────────────────────────── advancing the world ─────────────────────────────

/// Adapter that lets events act on the live colony and stores.
struct ColonyHost<'a> {
    colony: &'a mut Colony,
    side: &'a mut HostSide,
}

/// What event effects did that the colony itself does not record.
#[derive(Default)]
struct HostSide {
    arrivals_ignored: u32,
    /// Beavers killed by an event effect (their `Died` log entry names the wrong cause).
    killed: BTreeSet<BeaverId>,
}

impl EventHost for ColonyHost<'_> {
    fn population(&self) -> u32 {
        self.colony.population() as u32
    }
    fn food(&self) -> f32 {
        self.colony.food
    }
    fn wood(&self) -> f32 {
        self.colony.wood
    }
    fn add_food(&mut self, amount: f32) {
        self.colony.food = (self.colony.food + amount).max(0.0);
    }
    fn add_wood(&mut self, amount: f32) {
        self.colony.wood = (self.colony.wood + amount).max(0.0);
    }
    /// Zeroes the health of the weakest `n` beavers; the colony's own step removes them.
    fn kill_beavers(&mut self, n: u32) -> u32 {
        let mut order: Vec<usize> = (0..self.colony.beavers.len()).collect();
        order.sort_by(|&a, &b| self.colony.beavers[a].health.total_cmp(&self.colony.beavers[b].health));
        let k = (n as usize).min(order.len());
        for &i in &order[..k] {
            self.colony.beavers[i].health = 0.0;
            self.side.killed.insert(self.colony.beavers[i].id);
        }
        k as u32
    }
    fn damage_health(&mut self, amount: f32) {
        for b in &mut self.colony.beavers {
            b.health = (b.health - amount).clamp(0.0, 1.0);
        }
    }
    fn add_beavers(&mut self, n: u32) -> u32 {
        self.side.arrivals_ignored += n;
        0
    }
}

fn mean_in(grid: &sim::Grid<f32>, mask: &[bool]) -> f32 {
    let (sum, cnt) = grid.as_slice().iter().zip(mask).filter(|p| *p.1).fold((0.0, 0u32), |a, p| (a.0 + p.0, a.1 + 1));
    if cnt == 0 { 0.0 } else { sum / cnt as f32 }
}

fn count_in(grid: &sim::Grid<f32>, mask: &[bool], above: f32) -> usize {
    grid.as_slice().iter().zip(mask).filter(|p| *p.1 && *p.0 > above).count()
}

/// One day (or `dt`) of the whole world, wired the way a game frame would.
fn advance(sim: &mut SimState, site: &Site, base_rates: &[f32], dt: f32, side: &mut HostSide) -> Result<(), String> {
    let factor = sim.events.water_inflow_factor();
    for (s, &r) in sim.water.sources.iter_mut().zip(base_rates) {
        s.rate = r * factor;
    }
    sim.time_days += dt as f64;
    sim.water.step(&sim.height, dt);
    sim.ecology.step(dt, &sim.water.depth).map_err(|e| e.to_string())?;
    sim.colony.step_on(dt, &sim.water.depth).map_err(|e| e.to_string())?;
    sim.tech.update(&sim.colony);
    let terrain = Terrain { height: &sim.height, depth: &sim.water.depth };
    sim.buildings.step(dt, &sim.tech, terrain, &sim.colony, &sim.work);
    sim.region.step(dt);
    let ctx = EventContext {
        population: sim.colony.population() as u32,
        wetland: count_in(&sim.ecology.wetland, &site.in_valley, 0.3) as f32 / site.valley_cells as f32 * 4.0,
        forest: mean_in(&sim.ecology.forest, &site.in_valley),
        danger: mean_in(&sim.ecology.danger, &site.in_valley),
        mosquitoes: mean_in(&sim.ecology.mosquitoes, &site.in_valley),
        water_level: count_in(&sim.water.depth, &site.in_valley, WATER_DEPTH) as f32 / site.valley_cells as f32 * 8.0,
    };
    let mut host = ColonyHost { colony: &mut sim.colony, side };
    // Events are tuned for a 40-day year (10-day seasons); stretch it over the 365-day colony year.
    sim.events.step_with(dt * EVENT_DAYS_PER_DAY, &ctx, &mut host).map_err(|e| e.to_string())?;
    Ok(())
}

// ───────────────────────────── losses by cause ─────────────────────────────

/// Why a beaver left the colony. Six are deaths; emigration is a departure.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Loss {
    Starvation,
    Cold,
    Predator,
    Disease,
    Flood,
    Age,
    Emigration,
}

const LOSSES: [Loss; 7] =
    [Loss::Starvation, Loss::Cold, Loss::Predator, Loss::Disease, Loss::Flood, Loss::Age, Loss::Emigration];

impl Loss {
    fn name(self) -> &'static str {
        match self {
            Loss::Starvation => "starve",
            Loss::Cold => "cold",
            Loss::Predator => "predator",
            Loss::Disease => "disease",
            Loss::Flood => "flood",
            Loss::Age => "age",
            Loss::Emigration => "emigr",
        }
    }
}

/// Loss counts per cause.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Losses([u32; 7]);

impl Losses {
    fn add(&mut self, l: Loss) {
        self.0[l as usize] += 1;
    }
    fn get(&self, l: Loss) -> u32 {
        self.0[l as usize]
    }
    /// Deaths only (everything but emigration).
    fn deaths(&self) -> u32 {
        self.total() - self.get(Loss::Emigration)
    }
    fn total(&self) -> u32 {
        self.0.iter().sum()
    }
}

/// Maps a colony death cause to the reported one. Work injuries come from the wild (predators and
/// rough ground) and exhaustion is overwork illness, so they report as predator and disease.
fn classify_death(cause: DeathCause) -> Loss {
    match cause {
        DeathCause::Starvation => Loss::Starvation,
        DeathCause::Cold => Loss::Cold,
        DeathCause::Injury => Loss::Predator,
        DeathCause::Exhaustion => Loss::Disease,
        DeathCause::OldAge => Loss::Age,
    }
}

/// Cause of a beaver killed by an event effect: the kind of the latest event that carried deaths.
fn classify_event_kill(kind: EventKind) -> Result<Loss, String> {
    match kind {
        EventKind::Disease => Ok(Loss::Disease),
        EventKind::PredatorRaid => Ok(Loss::Predator),
        EventKind::Flood => Ok(Loss::Flood),
        k => Err(format!("event {k:?} killed beavers but has no death cause")),
    }
}

// ───────────────────────────── player actions ─────────────────────────────

/// Everything a strategy may do, with every refusal counted and kept (never silent).
struct Player {
    sim: SimState,
    site: Site,
    base_rates: Vec<f32>,
    acts: u32,
    refusals: BTreeMap<String, u32>,
    side: HostSide,
    /// Losses since the last yearly report, and the beavers' running totals.
    year_losses: Losses,
    fell_cursor: usize,
    lodges_registered: usize,
}

impl Player {
    /// Reads the colony's event log since the last call and adds each death and departure to the
    /// year's losses. Errors if a kill cannot be attributed.
    fn tally_losses(&mut self) -> Result<(), String> {
        for rec in self.sim.colony.take_events() {
            match rec.event {
                Event::Died { id, cause, .. } => {
                    let loss = if self.side.killed.remove(&id) {
                        let kind = self
                            .sim
                            .events
                            .log()
                            .iter()
                            .rev()
                            .find(|r| r.effects.iter().any(|e| matches!(e, Effect::Deaths(_))))
                            .map(|r| r.kind)
                            .ok_or("beaver killed by an event but the event log has no deadly event")?;
                        classify_event_kill(kind)?
                    } else {
                        classify_death(cause)
                    };
                    self.year_losses.add(loss);
                }
                Event::Departed { .. } => self.year_losses.add(Loss::Emigration),
                _ => {}
            }
        }
        Ok(())
    }

    /// Counts a refusal; numbers are masked so equal reasons group together.
    fn refuse(&mut self, reason: String) {
        let mut key = String::new();
        for c in reason.chars() {
            if c.is_ascii_digit() || (c == '.' && key.ends_with('#')) {
                if !key.ends_with('#') {
                    key.push('#');
                }
            } else {
                key.push(c);
            }
        }
        *self.refusals.entry(key).or_default() += 1;
    }

    fn note<T, E: Display>(&mut self, what: &str, r: Result<T, E>) -> Option<T> {
        match r {
            Ok(v) => {
                self.acts += 1;
                Some(v)
            }
            Err(e) => {
                self.refuse(format!("{what}: {e}"));
                None
            }
        }
    }

    /// Dams the river at `river_index` with a row of `2 * half_width + 1` cells `height` high.
    fn dam(&mut self, river_index: usize, half_width: i32, height: f32) -> bool {
        let cells = self.site.dam_cells(river_index, half_width, self.sim.world.size);
        let r = self.sim.apply(Command::BuildDam { cells, height });
        self.note("dam", r).is_some()
    }

    fn canal(&mut self, from: usize, to: usize, depth: f32) {
        let cells: Vec<Cell> = self.site.river[from..to.min(self.site.river.len())].to_vec();
        let r = self.sim.apply(Command::DigCanal { cells, depth });
        self.note("canal", r);
    }

    /// Fells up to `per_cell` forest density in up to `cells` start-valley cells near home, skipping
    /// cells with less than `keep`. `rotate` walks round the valley instead of always starting at home.
    /// Converts the harvest to logs, sticks and colony wood. Returns the density felled.
    fn fell(&mut self, cells: usize, per_cell: f32, keep: f32, reach: usize, rotate: bool) -> f32 {
        let limit = self.site.near_home.len().min(reach);
        let mut felled = 0.0;
        let mut done = 0;
        for k in 0..limit {
            if done == cells {
                break;
            }
            let idx = if rotate { (self.fell_cursor + k) % limit } else { k };
            let (x, y) = self.site.near_home[idx];
            let Some(&density) = self.sim.ecology.forest.get(x, y) else { continue };
            if density <= keep {
                continue;
            }
            let amount = per_cell.min(density - keep);
            let r = self.sim.ecology.fell(x, y, amount);
            if let Some(removed) = self.note("fell", r) {
                felled += removed;
                done += 1;
                if rotate {
                    self.fell_cursor = (idx + 1) % limit;
                }
            }
        }
        // Hauling stand-in: a felled density unit yields logs, sticks and colony wood.
        let st = &mut self.sim.buildings.stores;
        let _ = st.deposit(Good::Logs, 3.0 * felled);
        let _ = st.deposit(Good::Sticks, 4.0 * felled);
        self.sim.colony.wood += 5.0 * felled;
        felled
    }

    /// Daily mud and stone gathering shared by every strategy (colony jobs do not feed stores).
    fn gather(&mut self) {
        let st = &mut self.sim.buildings.stores;
        let _ = st.deposit(Good::Mud, 2.0);
        let _ = st.deposit(Good::Stone, 0.3);
    }

    /// Places `kind` on the first free spot found around home. Returns whether it was placed.
    fn build(&mut self, kind: BuildingKind) -> bool {
        let (hx, hy) = self.site.home;
        for r in (2..=16usize).step_by(2) {
            for k in 0..(8 * r / 2) {
                let a = k as f32 * std::f32::consts::TAU / (4 * r) as f32;
                let (x, y) = ((hx as f32 + r as f32 * a.cos()).round(), (hy as f32 + r as f32 * a.sin()).round());
                if x < 0.0 || y < 0.0 {
                    continue;
                }
                let sim = &mut self.sim;
                let terrain = Terrain { height: &sim.height, depth: &sim.water.depth };
                match sim.buildings.place(kind, (x as usize, y as usize), &sim.tech, terrain, &mut sim.colony) {
                    Ok(_) => {
                        self.acts += 1;
                        return true;
                    }
                    // Terrain trouble is local: try the next spot.
                    Err(BuildError::Blocked | BuildError::BadTerrain(_) | BuildError::OutOfBounds) => {}
                    Err(e) => {
                        self.refuse(format!("build {kind:?}: {e}"));
                        return false;
                    }
                }
            }
        }
        self.refuse(format!("build {kind:?}: no free site"));
        false
    }

    fn built(&self, kind: BuildingKind) -> usize {
        self.sim.buildings.buildings.iter().filter(|b| b.kind == kind).count()
    }

    fn tool_workshop(&mut self) {
        let ids: Vec<_> = self
            .sim
            .buildings
            .buildings
            .iter()
            .filter(|b| b.kind == BuildingKind::Workshop && b.is_complete() && b.recipe.is_none())
            .map(|b| b.id)
            .collect();
        for id in ids {
            let r = self.sim.buildings.set_recipe(id, Some(Recipe::Tools));
            self.note("recipe", r);
        }
    }

    /// Settles every pending event popup with the option `score` likes best.
    fn answer_events(&mut self, score: impl Fn(&PendingChoice, usize) -> f32) {
        let pending: Vec<PendingChoice> = self.sim.events.pending().to_vec();
        for c in pending {
            let best = (0..c.options.len()).max_by(|&a, &b| score(&c, a).total_cmp(&score(&c, b))).unwrap_or(0);
            let mut host = ColonyHost { colony: &mut self.sim.colony, side: &mut self.side };
            let r = self.sim.events.resolve(c.id, best, &mut host);
            self.note("event choice", r);
        }
    }

    /// Registers newly completed lodge buildings with the ecology (they shelter their surroundings).
    fn sync_lodges(&mut self) {
        let done: Vec<(usize, usize)> = self
            .sim
            .buildings
            .buildings
            .iter()
            .filter(|b| b.kind == BuildingKind::Lodge && matches!(b.status, Status::Complete))
            .map(|b| (b.pos.0 + 1, b.pos.1 + 1))
            .collect();
        for &(x, y) in &done[self.lodges_registered.min(done.len())..] {
            let r = self.sim.ecology.add_lodge(x, y);
            self.note("register lodge", r);
        }
        self.lodges_registered = done.len();
    }
}

// ───────────────────────────── strategies ─────────────────────────────

trait Strategy {
    fn name(&self) -> &'static str;
    /// Called once per simulated day (day 0 is the founding day) before the world advances.
    fn act(&mut self, p: &mut Player, day: u32);
}

/// Crude value of an event option: food and wood help, deaths and illness hurt.
fn option_value(c: &PendingChoice, i: usize) -> f32 {
    c.options[i]
        .effects
        .iter()
        .map(|e| match *e {
            Effect::Food(a) => a * 0.5,
            Effect::Wood(a) => a * 0.3,
            Effect::Deaths(n) => -30.0 * n as f32,
            Effect::Illness(a) => -40.0 * a,
            Effect::Arrivals(n) => 15.0 * n as f32,
            Effect::Inflow { factor, days } => (factor - 1.0) * days * 0.2,
        })
        .sum()
}

/// Sets the same job priorities for every family.
fn set_priorities(p: &mut Player, jobs: Vec<Job>) {
    let ids: Vec<_> = p.sim.colony.families.iter().map(|f| f.id).collect();
    for id in ids {
        let r = p.sim.colony.set_family_priorities(id, jobs.clone());
        p.note("priorities", r);
    }
}

/// Modest dam, canal, selective felling, regular building and yearly dam repair; answers events with the best-valued option.
struct Careful;
impl Strategy for Careful {
    fn name(&self) -> &'static str {
        "careful"
    }
    fn act(&mut self, p: &mut Player, day: u32) {
        let dam = p.site.dam_order[0];
        if day == 0 {
            p.dam(dam, 6, 0.8);
        }
        if day == 150 {
            p.canal(dam + 1, dam + 5, 0.4);
        }
        if day % DAYS_PER_YEAR == 200 {
            p.dam(dam, 6, 0.8); // rebuild to full integrity before decay bites
        }
        if p.sim.buildings.stores.amount(Good::Logs) < 60.0 {
            p.fell(3, 0.2, 0.5, 400, true);
        }
        if day % 20 == 10 {
            let want = [
                (BuildingKind::Workshop, 1),
                (BuildingKind::FoodCache, 1),
                (BuildingKind::Lodge, 1 + p.sim.colony.population() / 8),
                (BuildingKind::Warehouse, 1),
                (BuildingKind::GuardPost, 1),
            ];
            for (kind, n) in want {
                if p.built(kind) < n && p.build(kind) {
                    break;
                }
            }
            p.tool_workshop();
        }
        p.answer_events(option_value);
    }
}

/// Builds one small dam and a single lodge, then leaves everything alone, even event popups.
struct Neglectful;
impl Strategy for Neglectful {
    fn name(&self) -> &'static str {
        "neglectful"
    }
    fn act(&mut self, p: &mut Player, day: u32) {
        if day == 0 {
            p.dam(p.site.dam_order[0], 6, 0.5);
            set_priorities(p, vec![Job::Forager]);
        }
        if day == 30 {
            p.fell(2, 0.3, 0.3, 100, false);
            p.build(BuildingKind::Lodge);
        }
    }
}

/// Strips the forest nearest home bare every day and builds with the proceeds.
struct Overcutter;
impl Strategy for Overcutter {
    fn name(&self) -> &'static str {
        "overcutter"
    }
    fn act(&mut self, p: &mut Player, day: u32) {
        if day == 0 {
            p.dam(p.site.dam_order[0], 6, 0.8);
            set_priorities(p, vec![Job::Logger, Job::Forager]);
        }
        p.fell(12, 1.0, 0.0, 1500, false);
        if day % 20 == 10 {
            for kind in [BuildingKind::Lodge, BuildingKind::Warehouse, BuildingKind::Workshop] {
                if p.build(kind) {
                    break;
                }
            }
            p.tool_workshop();
        }
    }
}

/// Keeps adding tall, wide dams along the river and never breaches any of them.
struct OverDammer;
impl Strategy for OverDammer {
    fn name(&self) -> &'static str {
        "overdammer"
    }
    fn act(&mut self, p: &mut Player, day: u32) {
        if day % 60 == 0 {
            if let Some(&i) = p.site.dam_order.get((day / 60) as usize) {
                p.dam(i, 14, 3.0);
            }
        }
        if day % DAYS_PER_YEAR == 200 {
            for i in p.site.dam_order.clone() {
                p.dam(i, 14, 3.0); // keep them all at full strength
            }
        }
        if day % 40 == 20 {
            p.fell(2, 0.2, 0.5, 400, true);
            p.build(BuildingKind::Lodge);
        }
    }
}

// ───────────────────────────── reporting ─────────────────────────────

/// Start-valley stats at the end of a year (forest, wetland and pond are limited to the start valley).
struct Row {
    pop: usize,
    lodges: usize,
    lodge_buildings: usize,
    forest: f32,
    pond: usize,
    wetland: usize,
    logs: f32,
    sticks: f32,
    mud: f32,
    food: f32,
    wood: f32,
    losses: Losses,
    new_tech: Vec<&'static str>,
    events: BTreeMap<String, u32>,
    collapsed: bool,
}

fn snapshot(p: &Player, losses: Losses, events: BTreeMap<String, u32>, new_tech: Vec<&'static str>) -> Row {
    let s = &p.sim;
    let v = &p.site.in_valley;
    Row {
        pop: s.colony.population(),
        lodges: s.colony.lodges.len(),
        lodge_buildings: s.buildings.count_complete(BuildingKind::Lodge),
        forest: mean_in(&s.ecology.forest, v),
        pond: count_in(&s.water.depth, v, WATER_DEPTH),
        wetland: count_in(&s.ecology.wetland, v, 0.3),
        logs: s.buildings.stores.amount(Good::Logs),
        sticks: s.buildings.stores.amount(Good::Sticks),
        mud: s.buildings.stores.amount(Good::Mud),
        food: s.colony.food,
        wood: s.colony.wood,
        losses,
        new_tech,
        events,
        collapsed: s.colony.is_collapsed(),
    }
}

fn print_row(name: &str, r: &Row, tech_total: usize) {
    let ev = if r.events.is_empty() {
        "-".to_string()
    } else {
        r.events.iter().map(|(k, n)| if *n > 1 { format!("{k}x{n}") } else { k.clone() }).collect::<Vec<_>>().join(",")
    };
    let causes = LOSSES.iter().map(|&l| format!("{} {}", l.name(), r.losses.get(l))).collect::<Vec<_>>().join(" ");
    let tech = if r.new_tech.is_empty() { String::new() } else { format!(" (+{})", r.new_tech.join(",")) };
    println!(
        "  {name:<10} lost [{}] pop {:>3} lodges {:>2}(+{} built) forest {:.2} pond {:>4} wetland {:>4} stores L{:.0} S{:.0} M{:.0} food {:.0} wood {:.0} tech {}/{}{} events {}{}",
        causes, r.pop, r.lodges, r.lodge_buildings, r.forest, r.pond, r.wetland, r.logs, r.sticks, r.mud, r.food, r.wood,
        tech_total, TECHS.len(), tech, ev, if r.collapsed { " COLLAPSED" } else { "" }
    );
}

/// The per-cause counts of a year must equal the colony's own death and departure counters.
fn check_losses(name: &str, year: u32, year_losses: &Losses, now: Stats, prev: &mut Stats) -> Result<(), String> {
    let deaths = now.deaths - prev.deaths;
    let departures = now.departures - prev.departures;
    *prev = now;
    if year_losses.deaths() != deaths || year_losses.get(Loss::Emigration) != departures {
        return Err(format!(
            "{name} year {year}: cause counts {:?} do not match the colony's {deaths} deaths and {departures} departures",
            year_losses.0
        ));
    }
    Ok(())
}

// ───────────────────────────── main ─────────────────────────────

fn setup(seed: u64) -> Result<Player, String> {
    let mut sim = SimState::new(seed);
    let mut site = Site::find(&sim)?;
    sim.water.config.rain = RAIN;
    let head = site.river[0];
    sim.water.add_source(head.0, head.1, SOURCE_RATE).map_err(|e| e.to_string())?;
    let base_rates = vec![SOURCE_RATE];
    let mut ignored = HostSide::default();
    for _ in 0..SPIN_UP_DAYS {
        advance(&mut sim, &site, &base_rates, 1.0, &mut ignored)?;
    }
    // Re-pick home now that the river actually carries water.
    site = Site::find(&sim)?;
    let home = site.home;
    sim.colony.found((home.0 as f32, home.1 as f32), &sim.water.depth).map_err(|e| e.to_string())?;
    sim.ecology.add_lodge(home.0, home.1).map_err(|e| e.to_string())?;
    // Starting materials for the first buildings.
    for (g, a) in [(Good::Logs, 40.0), (Good::Sticks, 50.0), (Good::Mud, 40.0), (Good::Stone, 10.0)] {
        sim.buildings.stores.deposit(g, a).map_err(|e| e.to_string())?;
    }
    Ok(Player {
        sim,
        site,
        base_rates,
        acts: 0,
        refusals: BTreeMap::new(),
        side: HostSide::default(),
        year_losses: Losses::default(),
        fell_cursor: 0,
        lodges_registered: 0,
    })
}

fn run() -> Result<(), String> {
    let mut args = std::env::args().skip(1);
    let years: u32 =
        args.next().map_or(Ok(30), |s| s.parse().map_err(|_| format!("years must be an integer, got {s:?}")))?;
    let seed: u64 =
        args.next().map_or(Ok(1), |s| s.parse().map_err(|_| format!("seed must be an integer, got {s:?}")))?;
    if years == 0 {
        return Err("years must be at least 1".into());
    }
    let start = Instant::now();
    let mut strategies: Vec<Box<dyn Strategy>> =
        vec![Box::new(Careful), Box::new(Neglectful), Box::new(Overcutter), Box::new(OverDammer)];
    let mut players = Vec::new();
    for _ in &strategies {
        players.push(setup(seed)?);
    }
    println!("century: seed {seed}, {years} years, {} strategies", strategies.len());
    let mut seen_events = vec![0usize; players.len()];
    let mut seen_tech = vec![0usize; players.len()];
    let mut prev_stats = vec![Stats::default(); players.len()];
    for year in 1..=years {
        println!("year {year}");
        for (i, (p, strat)) in players.iter_mut().zip(strategies.iter_mut()).enumerate() {
            let mut d = 0;
            while d < DAYS_PER_YEAR {
                let day = (year - 1) * DAYS_PER_YEAR + d;
                // A collapsed colony has nothing left to command: let the land recover in coarse steps.
                let dt = if p.sim.colony.is_collapsed() { COLLAPSED_STEP_DAYS.min(DAYS_PER_YEAR - d) } else { 1 };
                if dt == 1 {
                    strat.act(p, day);
                    p.gather();
                    p.sync_lodges();
                }
                advance(&mut p.sim, &p.site, &p.base_rates, dt as f32, &mut p.side)
                    .and_then(|()| p.tally_losses())
                    .map_err(|e| format!("{} year {year} day {d}: {e}", strat.name()))?;
                d += dt;
            }
            let mut ev = BTreeMap::new();
            for rec in &p.sim.events.log()[seen_events[i]..] {
                *ev.entry(format!("{:?}", rec.kind)).or_default() += 1;
            }
            seen_events[i] = p.sim.events.log().len();
            let unlocked: Vec<&'static str> = p.sim.tech.unlocked_ids().map(|t| TECHS[t.0].name).collect();
            let new_tech = unlocked[seen_tech[i]..].to_vec();
            seen_tech[i] = unlocked.len();
            let losses = std::mem::take(&mut p.year_losses);
            check_losses(strat.name(), year, &losses, p.sim.colony.stats, &mut prev_stats[i])?;
            print_row(strat.name(), &snapshot(p, losses, ev, new_tech), unlocked.len());
        }
    }
    println!("\nsummary after {years} years:");
    for (p, strat) in players.iter().zip(&strategies) {
        println!(
            "  {:<10} accepted actions {}, refused {}, event arrivals not applied {}",
            strat.name(),
            p.acts,
            p.refusals.values().sum::<u32>(),
            p.side.arrivals_ignored
        );
        if let Some(c) = p.sim.colony.collapse() {
            println!("      collapsed in year {:.1}: {:?} (peak population {})", c.day / DAYS_PER_YEAR as f64, c.cause, c.peak_population);
        }
        for (why, n) in &p.refusals {
            println!("      refused x{n}: {why}");
        }
    }
    println!("elapsed {:.2}s", start.elapsed().as_secs_f32());
    Ok(())
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("century failed: {e}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cause_counts_sum_to_total() {
        let mut l = Losses::default();
        for (i, &c) in LOSSES.iter().enumerate() {
            for _ in 0..=i {
                l.add(c);
            }
        }
        assert_eq!(l.total(), (1..=7).sum::<u32>());
        assert_eq!(l.deaths(), l.total() - l.get(Loss::Emigration));
        assert_eq!(LOSSES.iter().map(|&c| l.get(c)).sum::<u32>(), l.total());
    }

    #[test]
    fn every_colony_cause_maps_and_event_kills_need_a_deadly_kind() {
        for c in [DeathCause::Starvation, DeathCause::Cold, DeathCause::Injury, DeathCause::Exhaustion, DeathCause::OldAge] {
            assert_ne!(classify_death(c), Loss::Emigration);
        }
        assert_eq!(classify_event_kill(EventKind::PredatorRaid), Ok(Loss::Predator));
        assert!(classify_event_kill(EventKind::Drought).is_err());
    }

    #[test]
    fn check_losses_rejects_mismatch() {
        let mut l = Losses::default();
        l.add(Loss::Cold);
        let now = Stats { deaths: 1, ..Stats::default() };
        assert!(check_losses("t", 1, &l, now, &mut Stats::default()).is_ok());
        assert!(check_losses("t", 1, &l, Stats { deaths: 2, ..now }, &mut Stats::default()).is_err());
    }
}
