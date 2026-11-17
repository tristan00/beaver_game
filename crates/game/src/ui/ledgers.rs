//! View-models for the four ledgers (Families, Jobs, Stores, Trade), the top bar and the detail
//! tables. Everything here is a pure function from sim state to plain rows, so it is tested
//! without a window; `panels.rs` only draws what these functions return.
use sim::buildings::{Good, Stores};
use sim::colony::{Beaver, BeaverId, Colony, Job, Stage};
use sim::events::Season;
use sim::features::{Features, System};
use sim::region::{Region, Resource, Specialty, WARY_BELOW};
use sim::tech::{Branch, Tech, TECHS};
use sim::SimState;
use std::collections::VecDeque;

use super::events::LogFilter;

// ───────────────────────────── shared types ─────────────────────────────

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Tab {
    Families,
    Beavers,
    Jobs,
    Stores,
    Trade,
    Tech,
    Log,
}

impl Tab {
    pub const ALL: [Tab; 7] = [Tab::Families, Tab::Beavers, Tab::Jobs, Tab::Stores, Tab::Trade, Tab::Tech, Tab::Log];
    /// Tabs the player may open: Trade needs trade and neighbours to be on.
    pub fn available(features: &Features) -> Vec<Tab> {
        Tab::ALL
            .into_iter()
            .filter(|t| *t != Tab::Trade || (features.enabled(System::Trade) && features.enabled(System::Rivals)))
            .collect()
    }
    pub fn label(self) -> &'static str {
        match self {
            Tab::Families => "Families",
            Tab::Beavers => "Beavers",
            Tab::Jobs => "Jobs",
            Tab::Stores => "Stores",
            Tab::Trade => "Trade",
            Tab::Tech => "Tech",
            Tab::Log => "Events",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tone {
    Normal,
    Good,
    Warn,
    Bad,
    Dim,
}

/// Everything the player can click in these panels.
#[derive(Clone, PartialEq, Debug)]
pub enum Act {
    Open(Tab),
    Close,
    Sort(usize),
    Pick(BeaverId),
    Accept(u64),
    Connect(u32),
    Jump(f32, f32),
    Choose(u64, usize),
    Dismiss(u64),
    Filter(LogFilter),
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Trend {
    Up,
    Down,
    Flat,
}

/// Up/down when `now` differs from `before` by more than `rel` of the larger magnitude (and `abs`).
pub fn trend(before: f32, now: f32, rel: f32, abs: f32) -> Trend {
    let d = now - before;
    if d.abs() <= abs.max(rel * before.abs().max(now.abs())) {
        Trend::Flat
    } else if d > 0.0 {
        Trend::Up
    } else {
        Trend::Down
    }
}

// ───────────────────────────── history ─────────────────────────────

#[derive(Clone, Debug, PartialEq)]
pub struct Sample {
    pub day: f64,
    pub pop: f32,
    pub mood: f32,
    pub employed: f32,
    pub food: f32,
    pub wood: f32,
    pub offers: f32,
    pub linked: f32,
}

/// Rolling record of colony numbers, sampled every [`HISTORY_EVERY`] days.
#[derive(Clone, Debug, Default)]
pub struct History {
    pub samples: VecDeque<Sample>,
}

pub const HISTORY_EVERY: f64 = 2.0;
pub const HISTORY_LEN: usize = 120;
/// Samples back that a trend compares against.
pub const TREND_SPAN: usize = 5;

impl History {
    pub fn record(&mut self, sim: &SimState) {
        let day = sim.time_days;
        if self.samples.back().is_some_and(|s| day - s.day < HISTORY_EVERY && day >= s.day) {
            return;
        }
        let c = &sim.colony;
        let n = c.population().max(1) as f32;
        self.samples.push_back(Sample {
            day,
            pop: c.population() as f32,
            mood: c.beavers.iter().map(|b| b.mood).sum::<f32>() / n,
            employed: c.beavers.iter().filter(|b| b.job.or(b.current_job).is_some()).count() as f32,
            food: c.food,
            wood: c.wood,
            offers: sim.region.offers().len() as f32,
            linked: sim.region.colonies.iter().filter(|n| n.is_connected()).count() as f32,
        });
        while self.samples.len() > HISTORY_LEN {
            self.samples.pop_front();
        }
    }

    /// Last `n` values of `f`, oldest first.
    pub fn series(&self, n: usize, f: impl Fn(&Sample) -> f32) -> Vec<f32> {
        let skip = self.samples.len().saturating_sub(n);
        self.samples.iter().skip(skip).map(f).collect()
    }

    /// Trend of `f` over the last [`TREND_SPAN`] samples, plus the signed change.
    pub fn trend_of(&self, f: impl Fn(&Sample) -> f32, rel: f32, abs: f32) -> (Trend, f32) {
        let (Some(last), Some(first)) = (self.samples.back(), self.samples.iter().rev().nth(TREND_SPAN)) else {
            return (Trend::Flat, 0.0);
        };
        let (a, b) = (f(first), f(last));
        (trend(a, b, rel, abs), b - a)
    }

    /// Per-day rate of `f` over the last [`TREND_SPAN`] samples.
    pub fn rate_of(&self, f: impl Fn(&Sample) -> f32) -> f32 {
        let (Some(last), Some(first)) = (self.samples.back(), self.samples.iter().rev().nth(TREND_SPAN)) else {
            return 0.0;
        };
        let dt = (last.day - first.day).max(1e-3) as f32;
        (f(last) - f(first)) / dt
    }
}

// ───────────────────────────── top bar ─────────────────────────────

#[derive(Clone, Debug, PartialEq)]
pub struct TopBar {
    pub date: String,
    pub season: Season,
    pub population: usize,
    pub pop_change: i32,
    pub mood: f32,
    pub mood_tone: Tone,
    pub speed: String,
    pub pending: usize,
}

pub fn mood_tone(m: f32) -> Tone {
    if m >= 0.65 {
        Tone::Good
    } else if m >= 0.4 {
        Tone::Warn
    } else {
        Tone::Bad
    }
}

pub fn top_bar(sim: &SimState, hist: &History, speed: &str) -> TopBar {
    let c = &sim.colony;
    let n = c.population().max(1) as f32;
    let mood = c.beavers.iter().map(|b| b.mood).sum::<f32>() / n;
    let d = sim.events.date();
    TopBar {
        date: format!("Year {}, day {}", d.year + 1, d.day),
        season: d.season,
        population: c.population(),
        pop_change: hist.trend_of(|s| s.pop, 0.0, 0.0).1.round() as i32,
        mood,
        mood_tone: mood_tone(mood),
        speed: speed.to_string(),
        pending: sim.events.pending().len(),
    }
}

// ───────────────────────────── compact ledgers ─────────────────────────────

#[derive(Clone, Debug, PartialEq)]
pub struct CompactRow {
    pub label: String,
    pub value: String,
    pub frac: Option<f32>,
    pub tone: Tone,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Compact {
    pub tab: Tab,
    pub title: &'static str,
    pub headline: String,
    pub sub: String,
    pub trend: Trend,
    pub delta: String,
    pub spark: Vec<f32>,
    pub rows: Vec<CompactRow>,
}

pub const COMPACT_ROWS: usize = 4;
const SPARK_LEN: usize = 21;

pub fn job_of(b: &Beaver) -> Option<Job> {
    b.job.or(b.current_job)
}

pub fn job_name(j: Option<Job>) -> String {
    j.map(|j| format!("{j:?}")).unwrap_or_else(|| "Idle".into())
}

fn delta_text(d: f32, decimals: usize) -> String {
    if d.abs() < 0.5 * 10f32.powi(-(decimals as i32)) {
        "=".into()
    } else {
        format!("{d:+.decimals$}")
    }
}

pub fn families_ledger(c: &Colony, hist: &History) -> Compact {
    let mut fams: Vec<(&str, usize, f32)> = c
        .families
        .iter()
        .map(|f| {
            let m: Vec<&Beaver> = c.beavers.iter().filter(|b| b.family == f.id).collect();
            let mood = if m.is_empty() { 0.0 } else { m.iter().map(|b| b.mood).sum::<f32>() / m.len() as f32 };
            (f.name.as_str(), m.len(), mood)
        })
        .filter(|f| f.1 > 0)
        .collect();
    fams.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));
    let total = fams.len();
    let mut rows: Vec<CompactRow> = fams
        .iter()
        .take(COMPACT_ROWS)
        .map(|(n, k, mood)| CompactRow { label: (*n).to_string(), value: format!("{k}"), frac: Some(*mood), tone: mood_tone(*mood) })
        .collect();
    if total > COMPACT_ROWS && let Some(last) = rows.last_mut() {
        *last = CompactRow { label: format!("+{} more families", total - COMPACT_ROWS + 1), value: String::new(), frac: None, tone: Tone::Dim };
    }
    let (t, d) = hist.trend_of(|s| s.pop, 0.0, 0.0);
    Compact {
        tab: Tab::Families,
        title: "Families",
        headline: format!("{} beavers", c.population()),
        sub: format!("{} families, {} kits", total, c.count_stage(Stage::Kit)),
        trend: t,
        delta: delta_text(d, 0),
        spark: hist.series(SPARK_LEN, |s| s.pop),
        rows,
    }
}

/// Workers per job (index = `Job::index`) and the idle count.
pub fn job_counts(c: &Colony) -> ([usize; 8], usize) {
    let mut n = [0; 8];
    let mut idle = 0;
    for b in c.beavers.iter().filter(|b| b.stage != Stage::Kit) {
        match job_of(b) {
            Some(j) => n[j.index()] += 1,
            None => idle += 1,
        }
    }
    (n, idle)
}

pub fn jobs_ledger(c: &Colony, hist: &History) -> Compact {
    let (n, idle) = job_counts(c);
    let working: usize = n.iter().sum();
    let able = working + idle;
    let mut jobs: Vec<(Job, usize)> = c.enabled_jobs().into_iter().map(|j| (j, n[j.index()])).filter(|x| x.1 > 0).collect();
    jobs.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    let mut rows: Vec<CompactRow> = jobs
        .iter()
        .take(COMPACT_ROWS - 1)
        .map(|(j, k)| CompactRow { label: format!("{j:?}"), value: format!("{k}"), frac: Some(*k as f32 / able.max(1) as f32), tone: Tone::Normal })
        .collect();
    rows.push(CompactRow {
        label: "Idle".into(),
        value: format!("{idle}"),
        frac: Some(idle as f32 / able.max(1) as f32),
        tone: if idle * 3 > able && idle > 1 { Tone::Warn } else { Tone::Dim },
    });
    let (t, d) = hist.trend_of(|s| s.employed, 0.0, 0.0);
    Compact {
        tab: Tab::Jobs,
        title: "Jobs",
        headline: format!("{working}/{able} working"),
        sub: format!("{} orders queued", c.orders.len()),
        trend: t,
        delta: delta_text(d, 0),
        spark: hist.series(SPARK_LEN, |s| s.employed),
        rows,
    }
}

pub fn good_name(g: Good) -> &'static str {
    match g {
        Good::Logs => "Logs",
        Good::Sticks => "Sticks",
        Good::Mud => "Mud",
        Good::Stone => "Stone",
        Good::Food => "Cache food",
        Good::Baskets => "Baskets",
        Good::Rope => "Rope",
        Good::Tools => "Tools",
    }
}

/// Days of food left at the current per-beaver appetite (0.05 food per day), `None` if nobody eats.
pub fn food_days(c: &Colony) -> Option<f32> {
    (c.population() > 0).then(|| c.food / (c.population() as f32 * 0.05))
}

fn store_tone(amount: f32, capacity: f32, net: f32) -> Tone {
    if amount <= 0.01 && net < 0.0 {
        Tone::Bad
    } else if net < -0.05 && amount < 0.25 * capacity {
        Tone::Warn
    } else if capacity > 0.0 && amount >= 0.98 * capacity {
        Tone::Warn
    } else {
        Tone::Normal
    }
}

pub fn stores_ledger(c: &Colony, s: &Stores, hist: &History) -> Compact {
    let (t, d) = hist.trend_of(|x| x.food, 0.03, 0.5);
    let mut rows = vec![
        CompactRow { label: "Larder food".into(), value: format!("{:.0}", c.food), frac: None, tone: if c.food_per_capita() < 1.0 { Tone::Bad } else { Tone::Normal } },
        CompactRow { label: "Wood".into(), value: format!("{:.0}", c.wood), frac: None, tone: Tone::Normal },
    ];
    // Goods that are stocked or moving, most productive first.
    let mut goods: Vec<Good> = Good::ALL.iter().copied().filter(|g| s.amount(*g) > 0.05 || s.net_rate(*g).abs() > 0.001).collect();
    goods.sort_by(|a, b| s.amount(*b).total_cmp(&s.amount(*a)));
    for g in goods.into_iter().take(COMPACT_ROWS - rows.len()) {
        rows.push(CompactRow {
            label: good_name(g).into(),
            value: format!("{:.0} {:+.1}/d", s.amount(g), s.net_rate(g)),
            frac: Some(s.amount(g) / s.capacity(g).max(1.0)),
            tone: store_tone(s.amount(g), s.capacity(g), s.net_rate(g)),
        });
    }
    Compact {
        tab: Tab::Stores,
        title: "Stores",
        headline: match food_days(c) {
            Some(dd) => format!("{dd:.0} days of food"),
            None => "no food".into(),
        },
        sub: format!("{:.1} per beaver", c.food_per_capita()),
        trend: t,
        delta: delta_text(d, 0),
        spark: hist.series(SPARK_LEN, |x| x.food),
        rows,
    }
}

pub fn specialty_name(s: Specialty) -> &'static str {
    match s {
        Specialty::Fishing => "Fishers",
        Specialty::Lumber => "Loggers",
        Specialty::Farming => "Farmers",
        Specialty::Crafts => "Crafters",
    }
}

pub fn resource_name(r: Resource) -> &'static str {
    match r {
        Resource::Food => "food",
        Resource::Wood => "wood",
    }
}

pub fn trade_ledger(r: &Region, hist: &History) -> Compact {
    let offers = r.offers().len();
    let linked = r.colonies.iter().filter(|n| n.is_connected()).count();
    let mut rows = Vec::new();
    let mut order: Vec<&sim::region::Neighbour> = r.colonies.iter().collect();
    order.sort_by(|a, b| b.is_connected().cmp(&a.is_connected()).then(b.friendliness.total_cmp(&a.friendliness)));
    for n in order.into_iter().take(COMPACT_ROWS) {
        let open = r.offers().iter().filter(|o| o.colony == n.id).count();
        let (value, tone) = match (n.link, open) {
            (Some(_), o) if o > 0 => (format!("{o} offer{}", if o > 1 { "s" } else { "" }), Tone::Good),
            (Some(_), _) => ("linked".into(), Tone::Normal),
            (None, _) if n.friendliness < WARY_BELOW => ("wary".into(), Tone::Warn),
            (None, _) => ("no route".into(), Tone::Dim),
        };
        rows.push(CompactRow { label: n.name.clone(), value, frac: Some(n.friendliness), tone });
    }
    let (t, d) = hist.trend_of(|s| s.offers, 0.0, 0.0);
    Compact {
        tab: Tab::Trade,
        title: "Trade",
        headline: format!("{offers} open offer{}", if offers == 1 { "" } else { "s" }),
        sub: format!("{linked} of {} linked", r.colonies.len()),
        trend: t,
        delta: delta_text(d, 0),
        spark: hist.series(SPARK_LEN, |s| s.offers),
        rows,
    }
}

pub fn all_ledgers(sim: &SimState, hist: &History) -> Vec<Compact> {
    let mut v = vec![
        families_ledger(&sim.colony, hist),
        jobs_ledger(&sim.colony, hist),
        stores_ledger(&sim.colony, &sim.buildings.stores, hist),
    ];
    if Tab::available(&sim.features).contains(&Tab::Trade) {
        v.push(trade_ledger(&sim.region, hist));
    }
    v
}

// ───────────────────────────── tables ─────────────────────────────

#[derive(Clone, Debug, PartialEq)]
pub struct Cell {
    pub text: String,
    /// Sort key; ties (and text columns, which leave it 0) fall back to the text.
    pub key: f64,
    pub tone: Tone,
    /// A meter drawn under the text.
    pub bar: Option<f32>,
    /// A button shown instead of text.
    pub button: Option<(String, Act)>,
}

impl Cell {
    pub fn text(s: impl Into<String>) -> Cell {
        Cell { text: s.into(), key: 0.0, tone: Tone::Normal, bar: None, button: None }
    }
    pub fn num(s: impl Into<String>, key: f64) -> Cell {
        Cell { key, ..Cell::text(s) }
    }
    pub fn tone(mut self, t: Tone) -> Cell {
        self.tone = t;
        self
    }
    pub fn bar(mut self, f: f32) -> Cell {
        self.bar = Some(f);
        self
    }
    pub fn button(label: &str, act: Act) -> Cell {
        Cell { button: Some((label.into(), act)), ..Cell::text("") }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    pub cells: Vec<Cell>,
    /// Clicking the row runs this.
    pub act: Option<Act>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Table {
    pub title: String,
    pub cols: Vec<(&'static str, f32)>,
    pub rows: Vec<Row>,
    /// Whether the header cells sort the rows.
    pub sortable: bool,
}

/// Stable sort by column `col` (numeric key, then text), ascending or descending.
pub fn sort_rows(rows: &mut [Row], col: usize, asc: bool) {
    rows.sort_by(|a, b| {
        let (x, y) = (a.cells.get(col), b.cells.get(col));
        let ord = match (x, y) {
            (Some(x), Some(y)) => x.key.total_cmp(&y.key).then_with(|| x.text.to_lowercase().cmp(&y.text.to_lowercase())),
            _ => std::cmp::Ordering::Equal,
        };
        if asc { ord } else { ord.reverse() }
    });
}

fn pct(v: f32) -> Cell {
    Cell::num(format!("{:.0}%", v * 100.0), v as f64).tone(mood_tone(v)).bar(v)
}

fn need_cell(v: f32) -> Cell {
    let tone = if v >= 0.6 { Tone::Good } else if v >= 0.3 { Tone::Warn } else { Tone::Bad };
    Cell::num(format!("{:.0}", v * 100.0), v as f64).tone(tone).bar(v)
}

fn parents(c: &Colony, b: &Beaver) -> String {
    let n = |id: Option<BeaverId>| id.and_then(|i| c.beaver(i)).map(|p| p.name.clone());
    match (n(b.mother), n(b.father)) {
        (Some(m), Some(f)) => format!("{m} & {f}"),
        (Some(p), None) | (None, Some(p)) => p,
        (None, None) => "-".into(),
    }
}

fn skill_abbrev(j: Job) -> &'static str {
    match j {
        Job::Forager => "For",
        Job::Logger => "Log",
        Job::Builder => "Bld",
        Job::Digger => "Dig",
        Job::Hauler => "Hau",
        Job::Guard => "Grd",
        Job::Crafter => "Cft",
        Job::Scout => "Sct",
    }
}

/// The two strongest skills as `"Log 70 Bld 40"`; skills under 5 are not worth listing.
pub fn top_skills(b: &Beaver) -> String {
    let mut s: Vec<(Job, f32)> = Job::ALL.iter().map(|j| (*j, b.skill(*j))).filter(|x| x.1 >= 0.05).collect();
    s.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.cmp(&b.0)));
    if s.is_empty() {
        return "-".into();
    }
    s.iter().take(2).map(|(j, v)| format!("{} {:.0}", skill_abbrev(*j), v * 100.0)).collect::<Vec<_>>().join(" ")
}


/// The family tree: every beaver under their family, eldest first, with parents, mate and skills.
pub fn families_table(c: &Colony) -> Table {
    let mut order: Vec<&Beaver> = c.beavers.iter().collect();
    let fam_name = |b: &Beaver| c.family(b.family).map(|f| f.name.clone()).unwrap_or_default();
    order.sort_by(|a, b| fam_name(a).cmp(&fam_name(b)).then(b.age_days.total_cmp(&a.age_days)).then(a.id.0.cmp(&b.id.0)));
    let rows = order
        .into_iter()
        .map(|b| {
            let mate = b.mate.and_then(|m| c.beaver(m)).map(|m| m.name.clone()).unwrap_or_else(|| "-".into());
            let indent = if b.stage == Stage::Adult { "" } else { "- " };
            Row {
                act: Some(Act::Pick(b.id)),
                cells: vec![
                    Cell::text(format!("{indent}{}", b.name)),
                    Cell::text(fam_name(b)),
                    Cell::text(parents(c, b)),
                    Cell::text(mate),
                    Cell::num(format!("{:.1}", b.age_years()), b.age_days as f64),
                    Cell::num(top_skills(b), b.skills.iter().copied().fold(0.0, f32::max) as f64),
                    pct(b.mood),
                ],
            }
        })
        .collect();
    Table {
        title: "Family tree".into(),
        cols: vec![("Beaver", 96.0), ("Family", 78.0), ("Parents", 100.0), ("Mate", 56.0), ("Age", 34.0), ("Top skills", 112.0), ("Mood", 52.0)],
        rows,
        sortable: true,
    }
}

/// Per-beaver needs, mood and health.
pub fn beavers_table(c: &Colony) -> Table {
    let mut order: Vec<&Beaver> = c.beavers.iter().collect();
    order.sort_by(|a, b| a.name.cmp(&b.name).then(a.id.0.cmp(&b.id.0)));
    let rows = order
        .into_iter()
        .map(|b| Row {
            act: Some(Act::Pick(b.id)),
            cells: vec![
                Cell::text(b.name.clone()),
                Cell::text(job_name(job_of(b))),
                need_cell(b.needs.food),
                need_cell(b.needs.rest),
                need_cell(b.needs.warmth),
                need_cell(b.needs.safety),
                need_cell(b.needs.social),
                pct(b.mood),
                pct(b.health),
            ],
        })
        .collect();
    Table {
        title: "Needs per beaver (0-100)".into(),
        cols: vec![("Beaver", 92.0), ("Job", 66.0), ("Food", 50.0), ("Rest", 50.0), ("Warmth", 54.0), ("Safety", 54.0), ("Social", 54.0), ("Mood", 54.0), ("Health", 54.0)],
        rows,
        sortable: true,
    }
}

pub fn jobs_table(c: &Colony) -> Table {
    let (n, idle) = job_counts(c);
    let able: usize = n.iter().sum::<usize>() + idle;
    let mut rows: Vec<Row> = c
        .enabled_jobs()
        .iter()
        .map(|j| {
            let workers: Vec<&Beaver> = c.beavers.iter().filter(|b| b.stage != Stage::Kit && job_of(b) == Some(*j)).collect();
            let avg = if workers.is_empty() { 0.0 } else { workers.iter().map(|b| b.skill(*j)).sum::<f32>() / workers.len() as f32 };
            let best = c.beavers.iter().filter(|b| b.stage != Stage::Kit).max_by(|a, b| a.skill(*j).total_cmp(&b.skill(*j)).then(b.id.0.cmp(&a.id.0)));
            let share = n[j.index()] as f32 / able.max(1) as f32;
            Row {
                act: None,
                cells: vec![
                    Cell::text(format!("{j:?}")),
                    Cell::num(format!("{}", n[j.index()]), n[j.index()] as f64),
                    Cell::num(format!("{:.0}%", share * 100.0), share as f64).bar(share),
                    Cell::num(format!("{:.0}", avg * 100.0), avg as f64),
                    Cell::num(best.map(|b| format!("{} {:.0}", b.name, b.skill(*j) * 100.0)).unwrap_or_else(|| "-".into()), best.map(|b| b.skill(*j)).unwrap_or(0.0) as f64),
                    Cell::num(format!("{:.1}", c.experience[j.index()]), c.experience[j.index()] as f64),
                ],
            }
        })
        .collect();
    rows.push(Row {
        act: None,
        cells: vec![
            Cell::text("Idle").tone(Tone::Dim),
            Cell::num(format!("{idle}"), idle as f64),
            Cell::num(format!("{:.0}%", idle as f32 / able.max(1) as f32 * 100.0), idle as f64 / able.max(1) as f64).bar(idle as f32 / able.max(1) as f32),
            Cell::text("-"),
            Cell::text("-"),
            Cell::text("-"),
        ],
    });
    Table {
        title: "Jobs".into(),
        cols: vec![("Job", 84.0), ("Workers", 62.0), ("Share", 80.0), ("Avg skill", 66.0), ("Best hand", 120.0), ("XP", 56.0)],
        rows,
        sortable: true,
    }
}

pub fn stores_table(c: &Colony, s: &Stores, hist: &History) -> Table {
    let mut rows = vec![
        Row {
            act: None,
            cells: vec![
                Cell::text("Larder"),
                Cell::num(format!("{:.0}", c.food), c.food as f64),
                Cell::text("-"),
                Cell::text("-"),
                Cell::num(String::new(), 0.0),
                Cell::num(String::new(), 0.0),
                Cell::num(format!("{:+.1}", hist.rate_of(|x| x.food)), hist.rate_of(|x| x.food) as f64).tone(if c.food_per_capita() < 1.0 { Tone::Bad } else { Tone::Normal }),
                Cell::text("-"),
                Cell::text("-"),
            ],
        },
        Row {
            act: None,
            cells: vec![
                Cell::text("Wood"),
                Cell::num(format!("{:.0}", c.wood), c.wood as f64),
                Cell::text("-"),
                Cell::text("-"),
                Cell::num(String::new(), 0.0),
                Cell::num(String::new(), 0.0),
                Cell::num(format!("{:+.1}", hist.rate_of(|x| x.wood)), hist.rate_of(|x| x.wood) as f64),
                Cell::text("-"),
                Cell::text("-"),
            ],
        },
    ];
    for g in Good::ALL {
        let i = g.index();
        let (amount, cap, net) = (s.amount(g), s.capacity(g), s.net_rate(g));
        let fill = amount / cap.max(1.0);
        rows.push(Row {
            act: None,
            cells: vec![
                Cell::text(good_name(g)),
                Cell::num(format!("{amount:.0}"), amount as f64),
                Cell::num(format!("{cap:.0}"), cap as f64),
                Cell::num(format!("{:.0}%", fill * 100.0), fill as f64).bar(fill).tone(store_tone(amount, cap, net)),
                Cell::num(format!("{:.2}", s.production_rate[i]), s.production_rate[i] as f64),
                Cell::num(format!("{:.2}", s.consumption_rate[i]), s.consumption_rate[i] as f64),
                Cell::num(format!("{net:+.2}"), net as f64).tone(if net < -0.001 { Tone::Warn } else if net > 0.001 { Tone::Good } else { Tone::Dim }),
                Cell::num(format!("{:.0}", s.produced_total[i]), s.produced_total[i] as f64),
                Cell::num(format!("{:.0}", s.consumed_total[i]), s.consumed_total[i] as f64),
            ],
        });
    }
    Table {
        title: "Stores (rates per day)".into(),
        cols: vec![("Good", 78.0), ("Have", 44.0), ("Cap", 40.0), ("Full", 62.0), ("Made/d", 52.0), ("Used/d", 52.0), ("Net/d", 52.0), ("Made", 44.0), ("Used", 44.0)],
        rows,
        sortable: true,
    }
}


/// Partners (with a connect button where a route can be opened) and their open offers.
pub fn trade_tables(r: &Region, day: f64) -> Vec<Table> {
    let partners = r
        .colonies
        .iter()
        .map(|n| {
            let action = if n.is_connected() {
                Cell::text("linked").tone(Tone::Good)
            } else if n.friendliness < WARY_BELOW {
                Cell::text("wary").tone(Tone::Warn)
            } else {
                Cell::button("Open path", Act::Connect(n.id))
            };
            Row {
                act: None,
                cells: vec![
                    Cell::text(n.name.clone()),
                    Cell::text(specialty_name(n.specialty)),
                    Cell::num(format!("{:.0}", n.population), n.population as f64),
                    Cell::num(format!("{:.0}", n.food), n.food as f64),
                    Cell::num(format!("{:.0}", n.wood), n.wood as f64),
                    Cell::num(format!("{:.0}%", n.friendliness * 100.0), n.friendliness as f64).tone(if n.friendliness < WARY_BELOW { Tone::Warn } else { Tone::Normal }).bar(n.friendliness),
                    action,
                ],
            }
        })
        .collect();
    let offers = r
        .offers()
        .iter()
        .map(|o| {
            let name = r.colony(o.colony).map(|n| n.name.clone()).unwrap_or_default();
            Row {
                act: None,
                cells: vec![
                    Cell::text(name),
                    Cell::num(format!("{:.0} {}", o.give.1, resource_name(o.give.0)), o.give.1 as f64).tone(Tone::Good),
                    Cell::num(format!("{:.0} {}", o.want.1, resource_name(o.want.0)), o.want.1 as f64),
                    Cell::num(format!("{:.0}d", (o.expires_day - day).max(0.0)), o.expires_day),
                    Cell::button("Accept", Act::Accept(o.id)),
                ],
            }
        })
        .collect();
    vec![
        Table {
            title: "Neighbouring colonies".into(),
            cols: vec![("Colony", 100.0), ("Trade", 62.0), ("Pop", 36.0), ("Food", 40.0), ("Wood", 40.0), ("Friendly", 74.0), ("Route", 88.0)],
            rows: partners,
            sortable: true,
        },
        Table {
            title: "Open offers: they give / they want".into(),
            cols: vec![("From", 110.0), ("They give", 100.0), ("They want", 100.0), ("Left", 56.0), ("", 78.0)],
            rows: offers,
            sortable: true,
        },
    ]
}

/// Experience, next tech and progress per branch; then every tech.
/// A tech is listed unless everything it does is unlocking buildings of a disabled system.
pub fn tech_visible(features: &Features, d: &sim::tech::TechDef) -> bool {
    d.effects.iter().any(|e| match e {
        sim::tech::Effect::Unlock(k) => features.allows_building(*k),
        _ => true,
    })
}

pub fn tech_tables(c: &Colony, t: &Tech, features: &Features) -> Vec<Table> {
    let branches = Branch::ALL
        .iter()
        .filter(|br| TECHS.iter().any(|d| d.branch == **br && tech_visible(features, d)))
        .map(|br| {
            let xp = br.experience(c);
            let next = TECHS.iter().enumerate().filter(|(i, d)| d.branch == *br && tech_visible(features, d) && !t.is_unlocked(sim::tech::TechId(*i))).min_by_key(|(_, d)| d.tier);
            let done = TECHS.iter().enumerate().filter(|(i, d)| d.branch == *br && tech_visible(features, d) && t.is_unlocked(sim::tech::TechId(*i))).count();
            let total = TECHS.iter().filter(|d| d.branch == *br && tech_visible(features, d)).count();
            let (next_name, frac) = match next {
                Some((_, d)) => (format!("{} ({:.0} xp)", d.name, d.xp), (xp / d.xp).clamp(0.0, 1.0)),
                None => ("all unlocked".into(), 1.0),
            };
            Row {
                act: None,
                cells: vec![
                    Cell::text(format!("{br:?}")),
                    Cell::num(format!("{xp:.1}"), xp as f64),
                    Cell::num(format!("{done}/{total}"), done as f64),
                    Cell::num(format!("{:.0}%", frac * 100.0), frac as f64).bar(frac).tone(if frac >= 1.0 { Tone::Good } else { Tone::Normal }),
                    Cell::text(next_name),
                ],
            }
        })
        .collect();
    let techs = TECHS
        .iter()
        .enumerate()
        .filter(|(_, d)| tech_visible(features, d))
        .map(|(i, d)| {
            let on = t.is_unlocked(sim::tech::TechId(i));
            let unlocks = d
                .effects
                .iter()
                .map(|e| match e {
                    sim::tech::Effect::Unlock(k) => k.spec().name.to_string(),
                    sim::tech::Effect::Bonus(sim::tech::Bonus::BuildWork(f)) => format!("build x{f:.2} work"),
                    sim::tech::Effect::Bonus(sim::tech::Bonus::CraftSpeed(f)) => format!("craft x{f:.2}"),
                    sim::tech::Effect::Bonus(sim::tech::Bonus::Storage(f)) => format!("storage x{f:.2}"),
                })
                .collect::<Vec<_>>()
                .join(", ");
            Row {
                act: None,
                cells: vec![
                    Cell::text(d.name),
                    Cell::text(format!("{:?}", d.branch)),
                    Cell::num(format!("{}", d.tier), d.tier as f64),
                    Cell::num(format!("{:.0}", d.xp), d.xp as f64),
                    Cell::text(if on { "unlocked" } else { "locked" }).tone(if on { Tone::Good } else { Tone::Dim }),
                    Cell::text(unlocks),
                ],
            }
        })
        .collect();
    vec![
        Table {
            title: "Branch progress".into(),
            cols: vec![("Branch", 76.0), ("XP", 46.0), ("Techs", 48.0), ("Progress", 90.0), ("Next tech", 250.0)],
            rows: branches,
            sortable: true,
        },
        Table {
            title: "All techs".into(),
            cols: vec![("Tech", 118.0), ("Branch", 70.0), ("Tier", 34.0), ("XP", 34.0), ("State", 64.0), ("Unlocks", 190.0)],
            rows: techs,
            sortable: true,
        },
    ]
}

/// Which rows to draw for a tab: the tables plus nothing else. `Log` is built by `events.rs`.
pub fn detail_tables(sim: &SimState, hist: &History, tab: Tab, filter: LogFilter) -> Vec<Table> {
    match tab {
        Tab::Families => vec![families_table(&sim.colony)],
        Tab::Beavers => vec![beavers_table(&sim.colony)],
        Tab::Jobs => vec![jobs_table(&sim.colony)],
        Tab::Stores => vec![stores_table(&sim.colony, &sim.buildings.stores, hist)],
        Tab::Trade => trade_tables(&sim.region, sim.time_days),
        Tab::Tech => tech_tables(&sim.colony, &sim.tech, &sim.features),
        Tab::Log => vec![super::events::log_table(sim, filter)],
    }
}

// ───────────────────────────── tests ─────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use sim::colony::Colony;
    use sim::region::Link;
    use sim::WorldLand;

    /// A founded colony that has lived a while, with some trade and tech.
    pub fn midgame(seed: u64, days: u32) -> SimState {
        let mut s = SimState::new(seed);
        let cell = (s.world.valleys[s.world.start.0 as usize].center.0 as f32, s.world.valleys[s.world.start.0 as usize].center.1 as f32);
        let land = WorldLand { world: &s.world, depth: &s.water.depth };
        let mut found = s.colony.found(cell, &land);
        if found.is_err() {
            // fall back to any dry cell near the centre
            for dx in 0..30 {
                let land = WorldLand { world: &s.world, depth: &s.water.depth };
                found = s.colony.found((cell.0 + dx as f32, cell.1), &land);
                if found.is_ok() {
                    break;
                }
            }
        }
        found.expect("found a colony");
        let _ = s.region.connect(0, Link::Path);
        for _ in 0..days * 4 {
            s.step(0.25).unwrap();
        }
        s
    }

    fn hist_for(s: &SimState) -> History {
        let mut h = History::default();
        h.record(s);
        h
    }

    #[test]
    fn ledgers_trend_thresholds() {
        assert_eq!(trend(10.0, 10.2, 0.05, 0.0), Trend::Flat);
        assert_eq!(trend(10.0, 12.0, 0.05, 0.0), Trend::Up);
        assert_eq!(trend(10.0, 7.0, 0.05, 0.0), Trend::Down);
        assert_eq!(trend(0.0, 0.0, 0.0, 0.0), Trend::Flat);
        assert_eq!(trend(1.0, 1.4, 0.0, 0.5), Trend::Flat);
    }

    #[test]
    fn ledgers_sort_rows_orders_by_key_then_text_and_reverses() {
        let r = |t: &str, k: f64| Row { act: None, cells: vec![Cell::num(t, k)] };
        let mut rows = vec![r("b", 2.0), r("a", 2.0), r("c", 1.0)];
        sort_rows(&mut rows, 0, true);
        let order: Vec<_> = rows.iter().map(|r| r.cells[0].text.clone()).collect();
        assert_eq!(order, ["c", "a", "b"]);
        sort_rows(&mut rows, 0, false);
        let order: Vec<_> = rows.iter().map(|r| r.cells[0].text.clone()).collect();
        assert_eq!(order, ["b", "a", "c"]);
        // out-of-range column leaves order untouched
        sort_rows(&mut rows, 9, true);
        assert_eq!(rows[0].cells[0].text, "b");
    }

    #[test]
    fn ledgers_families_and_beavers_rows_match_colony() {
        let s = midgame(3, 60);
        let c = &s.colony;
        let ft = families_table(c);
        assert_eq!(ft.rows.len(), c.population());
        assert!(ft.rows.iter().all(|r| r.cells.len() == ft.cols.len()));
        let bt = beavers_table(c);
        assert_eq!(bt.rows.len(), c.population());
        assert!(bt.rows.iter().all(|r| r.cells.len() == bt.cols.len()));
        // sorted by family name first
        let fams: Vec<_> = ft.rows.iter().map(|r| r.cells[1].text.clone()).collect();
        let mut sorted = fams.clone();
        sorted.sort();
        assert_eq!(fams, sorted);
        let h = hist_for(&s);
        let fl = families_ledger(c, &h);
        assert_eq!(fl.headline, format!("{} beavers", c.population()));
        assert!(fl.rows.len() <= COMPACT_ROWS);
    }

    #[test]
    fn ledgers_jobs_counts_add_up() {
        let s = midgame(3, 60);
        let c = &s.colony;
        let (n, idle) = job_counts(c);
        let non_kits = c.beavers.iter().filter(|b| b.stage != Stage::Kit).count();
        assert_eq!(n.iter().sum::<usize>() + idle, non_kits);
        let t = jobs_table(c);
        assert_eq!(t.rows.len(), c.enabled_jobs().len() + 1);
        let workers: f64 = t.rows[..c.enabled_jobs().len()].iter().map(|r| r.cells[1].key).sum();
        assert_eq!(workers as usize, n.iter().sum::<usize>());
        let j = jobs_ledger(c, &History::default());
        assert!(j.headline.contains(&format!("/{non_kits}")), "{}", j.headline);
        assert_eq!(j.rows.last().unwrap().label, "Idle");
    }

    #[test]
    fn ledgers_stores_rows_reflect_the_stores() {
        let mut s = midgame(3, 5);
        s.buildings.stores.deposit(Good::Logs, 40.0).unwrap();
        let h = hist_for(&s);
        let t = stores_table(&s.colony, &s.buildings.stores, &h);
        assert_eq!(t.rows.len(), 2 + Good::ALL.len());
        let logs = t.rows.iter().find(|r| r.cells[0].text == "Logs").unwrap();
        assert_eq!(logs.cells[1].key, 40.0);
        assert!(logs.cells.iter().all(|c| !c.text.is_empty() || c.key == 0.0));
        let l = stores_ledger(&s.colony, &s.buildings.stores, &h);
        assert!(l.rows.iter().any(|r| r.label == "Logs"));
        assert!(l.headline.contains("days of food"));
        assert!(food_days(&Colony::default()).is_none());
    }

    #[test]
    fn ledgers_trade_rows_offers_and_connect_buttons() {
        let mut s = SimState::new(5);
        let id = s.region.colonies.iter().position(|n| n.friendliness >= WARY_BELOW).expect("a friendly colony") as u32;
        let t = trade_tables(&s.region, 0.0);
        assert_eq!(t.len(), 2);
        assert_eq!(t[0].rows.len(), s.region.colonies.len());
        let row = &t[0].rows[id as usize];
        assert_eq!(row.cells[6].button.as_ref().map(|b| b.1.clone()), Some(Act::Connect(id)));
        assert!(t[1].rows.is_empty());
        s.region.connect(id, Link::Path).unwrap();
        for _ in 0..200 {
            s.region.step(1.0);
            if !s.region.offers().is_empty() {
                break;
            }
        }
        let t = trade_tables(&s.region, s.region.time_days);
        let offers = s.region.offers().len();
        assert_eq!(t[1].rows.len(), offers);
        if offers > 0 {
            let o = &s.region.offers()[0];
            assert_eq!(t[1].rows[0].cells[4].button.as_ref().unwrap().1, Act::Accept(o.id));
            let l = trade_ledger(&s.region, &History::default());
            assert!(l.headline.starts_with(&offers.to_string()));
        }
        assert_eq!(t[0].rows[id as usize].cells[6].text, "linked");
    }

    #[test]
    fn ledgers_tech_tables_show_branch_progress() {
        let mut s = midgame(3, 5);
        s.colony.experience[Job::Builder.index()] = 10.0;
        s.tech.update(&s.colony);
        let t = tech_tables(&s.colony, &s.tech, &s.features);
        // Defence's only tech unlocks guard posts and scent mounds, so it is not listed
        assert_eq!(t[0].rows.len(), Branch::ALL.len() - 1);
        assert_eq!(t[1].rows.len(), TECHS.len() - 1);
        let building = t[0].rows.iter().find(|r| r.cells[0].text == "Building").unwrap();
        assert_eq!(building.cells[2].text, "1/3");
        assert!(building.cells[3].key > 0.5 && building.cells[3].key < 1.0, "{:?}", building.cells[3]);
        let joinery = t[1].rows.iter().find(|r| r.cells[0].text == "Joinery").unwrap();
        assert_eq!(joinery.cells[4].text, "unlocked");
    }

    #[test]
    fn ledgers_top_bar_reports_date_population_mood_speed() {
        let s = midgame(3, 25);
        let h = hist_for(&s);
        let tb = top_bar(&s, &h, "3x");
        assert_eq!(tb.population, s.colony.population());
        assert!(tb.date.starts_with("Year 1"), "{}", tb.date);
        assert_eq!(tb.speed, "3x");
        assert!((0.0..=1.0).contains(&tb.mood));
        assert_eq!(mood_tone(0.9), Tone::Good);
        assert_eq!(mood_tone(0.5), Tone::Warn);
        assert_eq!(mood_tone(0.1), Tone::Bad);
    }

    #[test]
    fn ledgers_history_is_bounded_and_spaced() {
        let mut s = midgame(3, 2);
        let mut h = History::default();
        for _ in 0..(HISTORY_LEN * 3) {
            s.step(1.0).unwrap();
            s.step(1.0).unwrap();
            h.record(&s);
        }
        assert_eq!(h.samples.len(), HISTORY_LEN);
        assert!(h.samples.iter().zip(h.samples.iter().skip(1)).all(|(a, b)| b.day - a.day >= HISTORY_EVERY - 1e-6));
        assert_eq!(h.series(10, |x| x.pop).len(), 10);
    }

    /// Every screen's view-model under the shipping features names none of the disabled systems.
    #[test]
    fn scoped_view_models_list_no_disabled_system() {
        use crate::build_logic::{job_rows, menu};
        use crate::ui::shell::{start_menu, NavItem, Shell};
        let mut s = SimState::new(2);
        let cell = s.world.valleys[s.world.start.0 as usize].center;
        let land = sim::WorldLand { world: &s.world, depth: &s.water.depth };
        s.colony.found((cell.0 as f32, cell.1 as f32), &land).expect("found");
        for _ in 0..40 {
            s.step(1.0).unwrap();
        }
        let hist = History::default();
        let mut dump = String::new();
        let mut add = |name: &str, v: String| dump.push_str(&format!("[{name}] {}
", v.to_lowercase()));

        add("ledgers", format!("{:?}", all_ledgers(&s, &hist)));
        add("tabs", format!("{:?}", Tab::available(&s.features)));
        for t in Tab::available(&s.features) {
            for f in LogFilter::available(&s.features) {
                add("detail", format!("{:?}", detail_tables(&s, &hist, t, f)));
            }
        }
        add("filters", format!("{:?}", LogFilter::available(&s.features).iter().map(|f| f.label()).collect::<Vec<_>>()));
        add("build menu", format!("{:?}", menu(&s).iter().map(|e| (e.kind.spec().name, e.need)).collect::<Vec<_>>()));
        add("catalogue", format!("{:?}", sim::catalogue::available(&s)));
        add("jobs", format!("{:?}", job_rows(&s.colony).iter().map(|r| r.job).collect::<Vec<_>>()));
        add("families", format!("{:?}", s.colony.families));
        add("nav", format!("{:?}", Shell::default().bar(&s.features)));
        add("start menu", format!("{:?}", start_menu()));
        add("top bar", format!("{:?}", top_bar(&s, &hist, "x1")));
        let _ = NavItem::ALL;

        for word in ["trade", "guard", "scout", "predator", "flood", "drought", "rival", "scent"] {
            let hits: Vec<&str> = dump.lines().filter(|l| l.contains(word)).collect();
            assert!(hits.is_empty(), "`{word}` shown: {hits:?}");
        }
        assert!(Tab::available(&s.features).iter().all(|t| *t != Tab::Trade));
        assert_eq!(all_ledgers(&s, &hist).len(), 3);
    }

    #[test]
    fn everything_on_still_lists_trade_and_guard() {
        use crate::build_logic::menu;
        let mut s = SimState::new(2);
        s.set_features(sim::features::Features::everything());
        assert!(Tab::available(&s.features).contains(&Tab::Trade));
        assert!(menu(&s).iter().any(|e| e.kind == sim::buildings::BuildingKind::GuardPost));
        assert_eq!(all_ledgers(&s, &History::default()).len(), 4);
    }
}
