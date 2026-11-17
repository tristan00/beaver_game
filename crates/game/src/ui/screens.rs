//! View-models of the navigation bar, the time controls and the six screens (owner: `ui_shell`).
//! Pure functions from `SimState` to plain data, tested without a window; `panels.rs` draws them.
//!
//! Rules every model here follows (and `tests` checks):
//! * every number is an [`Explained`] whose parts add up to it, shown on hover;
//! * no label+value appears twice on one screen;
//! * every resource, need, job and action entry carries an [`Icon`], so nothing is text-only.

use sim::buildings::{Good, Status};
use sim::colony::{Beaver, BeaverId, Colony, Job, Stage};
use sim::events::Season;
use sim::society::RequestId;
use sim::tech::{Bonus, Branch, Effect, TechId, TECHS};
use sim::SimState;

use super::explain::{Explained, Numbers, Part};
use super::icons::Icon;
use super::shell::NavItem;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Tone {
    Normal,
    Good,
    Warn,
    Bad,
    Dim,
}

/// What a click on a screen does.
#[derive(Clone, Debug, PartialEq)]
pub enum Do {
    Nav(NavItem),
    /// Select a beaver and move the camera to it.
    Pick(BeaverId),
    /// Move the camera to a map cell.
    Jump(f32, f32),
    /// Answer option `n` of pending event `id`.
    Choose(u64, usize),
    /// Accept (true) or turn away (false) a wandering beaver.
    Answer(RequestId, bool),
    /// Pick a building tool and close the screen so it can be placed on the map.
    Place(sim::buildings::BuildingKind),
    /// Put one more (+1) or one fewer (-1) adult on a job.
    Staff(Job, i32),
    /// Sort table `.0` by column `.1`.
    Sort(usize, usize),
}

/// A button: always has an icon, the label is a short caption.
#[derive(Clone, Debug, PartialEq)]
pub struct Action {
    pub icon: Icon,
    pub label: String,
    pub act: Do,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Cell {
    Text(String, Tone),
    /// A number: `frac` draws a bar under it.
    Num { num: Explained, tone: Tone, frac: Option<f32> },
    /// Text led by an icon (a job, a resource).
    Entry(Icon, String),
    Actions(Vec<Action>),
}

impl Cell {
    fn text(s: impl Into<String>) -> Cell {
        Cell::Text(s.into(), Tone::Normal)
    }
    fn num(n: Explained) -> Cell {
        Cell::Num { num: n, tone: Tone::Normal, frac: None }
    }
    fn tone(self, t: Tone) -> Cell {
        match self {
            Cell::Num { num, frac, .. } => Cell::Num { num, tone: t, frac },
            Cell::Text(s, _) => Cell::Text(s, t),
            other => other,
        }
    }
    fn bar(self, f: f32) -> Cell {
        match self {
            Cell::Num { num, tone, .. } => Cell::Num { num, tone, frac: Some(f) },
            other => other,
        }
    }
    fn show(label: &str, act: Do) -> Cell {
        Cell::Actions(vec![Action { icon: Icon::MoveArea, label: label.into(), act }])
    }
    fn sort_key(&self) -> (f64, String) {
        match self {
            Cell::Text(s, _) | Cell::Entry(_, s) => (0.0, s.to_lowercase()),
            Cell::Num { num, .. } => (num.value as f64, String::new()),
            Cell::Actions(_) => (0.0, String::new()),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Col {
    pub name: &'static str,
    pub width: f32,
}

/// A table row: every row names a thing and carries its icon.
#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    pub icon: Icon,
    pub name: String,
    pub cells: Vec<Cell>,
    /// Clicking the row does this.
    pub click: Option<Do>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Table {
    pub title: &'static str,
    /// Header of the name column, then of each cell column.
    pub name_col: &'static str,
    /// Width of the name column.
    pub name_w: f32,
    pub cols: Vec<Col>,
    pub rows: Vec<Row>,
    pub empty: &'static str,
}

pub const NAME_W: f32 = 132.0;

/// A headline number at the top of a screen.
#[derive(Clone, Debug, PartialEq)]
pub struct Kpi {
    pub icon: Icon,
    pub num: Explained,
    pub tone: Tone,
    pub frac: Option<f32>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Screen {
    pub item: NavItem,
    pub kpis: Vec<Kpi>,
    pub tables: Vec<Table>,
}

impl Numbers for Screen {
    fn numbers(&self) -> Vec<&Explained> {
        let mut v: Vec<&Explained> = self.kpis.iter().map(|k| &k.num).collect();
        for t in &self.tables {
            for r in &t.rows {
                for c in &r.cells {
                    if let Cell::Num { num, .. } = c {
                        v.push(num);
                    }
                }
            }
        }
        v
    }
}

#[cfg_attr(not(test), allow(dead_code))]
impl Screen {
    /// Every action button on the screen.
    pub fn actions(&self) -> Vec<&Action> {
        let mut v = Vec::new();
        for t in &self.tables {
            for r in &t.rows {
                for c in &r.cells {
                    if let Cell::Actions(a) = c {
                        v.extend(a.iter());
                    }
                }
            }
        }
        v
    }
    /// Every icon the screen draws (headline numbers, rows, entries, actions).
    pub fn icons(&self) -> Vec<Icon> {
        let mut v: Vec<Icon> = self.kpis.iter().map(|k| k.icon).collect();
        for t in &self.tables {
            for r in &t.rows {
                v.push(r.icon);
                for c in &r.cells {
                    match c {
                        Cell::Entry(i, _) => v.push(*i),
                        Cell::Actions(a) => v.extend(a.iter().map(|x| x.icon)),
                        _ => {}
                    }
                }
            }
        }
        v
    }
}

/// Sorts a table's rows by column `col` (0 is the name column).
pub fn sort_rows(rows: &mut [Row], col: usize, asc: bool) {
    let key = |r: &Row| if col == 0 { (0.0, r.name.to_lowercase()) } else { r.cells.get(col - 1).map(|c| c.sort_key()).unwrap_or_default() };
    rows.sort_by(|a, b| {
        let (ka, kb) = (key(a), key(b));
        let o = ka.0.total_cmp(&kb.0).then(ka.1.cmp(&kb.1)).then(a.name.cmp(&b.name));
        if asc { o } else { o.reverse() }
    });
}

// ───────────────────────────── navigation bar & time ─────────────────────────────

#[derive(Clone, Debug, PartialEq)]
pub struct NavEntry {
    pub item: NavItem,
    pub icon: Icon,
    pub label: &'static str,
    pub open: bool,
    /// A count that wants attention (decisions waiting).
    pub badge: Option<Explained>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct NavBar {
    pub entries: Vec<NavEntry>,
}

impl Numbers for NavBar {
    fn numbers(&self) -> Vec<&Explained> {
        self.entries.iter().filter_map(|e| e.badge.as_ref()).collect()
    }
}

/// Decisions waiting: pending event choices and strangers asking to join.
pub fn decisions(sim: &SimState) -> Option<Explained> {
    let mut parts: Vec<Part> = sim.events.pending().iter().map(|p| Explained::part(format!("{} event", super::screens::kind_label(p.kind)), 1.0)).collect();
    parts.extend(sim.society.pending().iter().map(|r| Explained::part(format!("{} asks to join", r.name), 1.0)));
    (!parts.is_empty()).then(|| Explained::sum("Decisions waiting", "", parts))
}

pub fn nav_bar(sim: &SimState, open: Option<NavItem>) -> NavBar {
    let entries = NavItem::ALL
        .iter()
        .copied()
        .filter(|i| i.visible(&sim.features))
        .map(|item| NavEntry {
            item,
            icon: item.icon(),
            label: item.label(),
            open: open == Some(item),
            badge: if item == NavItem::Events { decisions(sim) } else { None },
        })
        .collect();
    NavBar { entries }
}

/// Season and colony day beside the time controls.
#[derive(Clone, Debug, PartialEq)]
pub struct TimeBar {
    pub season: Season,
    pub day: Explained,
}

impl Numbers for TimeBar {
    fn numbers(&self) -> Vec<&Explained> {
        vec![&self.day]
    }
}

pub fn time_bar(sim: &SimState) -> TimeBar {
    let d = sim.events.date();
    let dps = sim.events.calendar.days_per_season() as f32;
    let season_idx = Season::ALL.iter().position(|s| *s == d.season).unwrap_or(0) as f32;
    let day = Explained::sum(
        "Colony day",
        "",
        vec![
            Explained::part("Full years", d.year as f32 * 4.0 * dps),
            Explained::part("Full seasons this year", season_idx * dps),
            Explained::part("Day of this season", d.day as f32),
        ],
    );
    TimeBar { season: d.season, day }
}

// ───────────────────────────── helpers ─────────────────────────────

pub fn kind_label(k: sim::events::EventKind) -> String {
    format!("{k:?}")
}

fn job_icon(j: Job) -> Icon {
    match j {
        Job::Forager => Icon::Food,
        Job::Logger => Icon::Logs,
        Job::Builder => Icon::Builder,
        Job::Digger => Icon::Mud,
        Job::Hauler => Icon::Hauler,
        Job::Guard => Icon::Shelter,
        Job::Crafter => Icon::Craft,
        Job::Scout => Icon::MoveArea,
    }
}

fn job_of(b: &Beaver) -> Option<Job> {
    b.job.or(b.current_job)
}

fn pct_tone(p: f32) -> Tone {
    if p >= 65.0 {
        Tone::Good
    } else if p >= 40.0 {
        Tone::Warn
    } else {
        Tone::Bad
    }
}

fn members<'a>(c: &'a Colony, fam: sim::colony::FamilyId) -> Vec<&'a Beaver> {
    c.beavers.iter().filter(|b| b.family == fam).collect()
}

fn family_name(c: &Colony, id: sim::colony::FamilyId) -> String {
    c.family(id).map(|f| f.name.clone()).unwrap_or_else(|| "Unknown".into())
}

/// Colony-wide average of `f` (0..1) as a percentage whose parts are each family's weighted share.
fn colony_pct(c: &Colony, label: &str, f: impl Fn(&Beaver) -> f32) -> Explained {
    let n = c.population().max(1) as f32;
    let parts = c
        .families
        .iter()
        .filter_map(|fam| {
            let m = members(c, fam.id);
            (!m.is_empty()).then(|| Explained::part(format!("{} family", fam.name), m.iter().map(|b| f(b)).sum::<f32>() / n * 100.0))
        })
        .collect();
    Explained::sum_or_zero(label, "%", parts)
}

fn need_kpi(c: &Colony, label: &str, icon: Icon, f: impl Fn(&Beaver) -> f32) -> Kpi {
    let num = colony_pct(c, label, f);
    Kpi { icon, tone: pct_tone(num.value), frac: Some(num.value / 100.0), num }
}

/// A beaver's mood as the weighted needs that feed it plus what health and settling add.
fn mood_explained(b: &Beaver) -> Explained {
    use sim::colony::*;
    let n = &b.needs;
    let terms = [
        ("Food", MOOD_FOOD_WEIGHT * n.food),
        ("Warmth", MOOD_WARMTH_WEIGHT * n.warmth),
        ("Rest", MOOD_REST_WEIGHT * n.rest),
        ("Safety", MOOD_SAFETY_WEIGHT * n.safety),
        ("Company", MOOD_SOCIAL_WEIGHT * n.social),
    ];
    let mut parts: Vec<Part> = terms.iter().map(|(l, v)| Explained::part(*l, v * 100.0)).collect();
    let rest = b.mood * 100.0 - parts.iter().map(|p| p.value).sum::<f32>();
    parts.push(Explained::part("Health and settling", rest));
    Explained::sum(&format!("{} mood", b.name), "%", parts)
}

/// Counts of adults, yearlings and kits as parts.
fn stage_parts(m: &[&Beaver]) -> Vec<Part> {
    [(Stage::Adult, "Adults"), (Stage::Yearling, "Yearlings"), (Stage::Kit, "Kits")]
        .iter()
        .map(|(s, l)| Explained::part(*l, m.iter().filter(|b| b.stage == *s).count() as f32))
        .filter(|p| p.value > 0.0)
        .collect()
}

// ───────────────────────────── screens ─────────────────────────────

/// Slow-moving rates the screens cannot read from the sim directly.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rates {
    pub larder_per_day: f32,
    pub wood_per_day: f32,
}

pub fn screen(sim: &SimState, item: NavItem, rates: Rates) -> Screen {
    match item {
        NavItem::Colony => colony_screen(sim),
        NavItem::Work => work_screen(sim),
        NavItem::Stores => stores_screen(sim, rates),
        NavItem::Buildings => buildings_screen(sim),
        NavItem::Tech => tech_screen(sim),
        NavItem::Events => events_screen(sim),
    }
}

fn colony_screen(sim: &SimState) -> Screen {
    let c = &sim.colony;
    let all: Vec<&Beaver> = c.beavers.iter().collect();
    let mood = colony_pct(c, "Colony mood", |b| b.mood);
    let kpis = vec![
        Kpi { icon: Icon::NavBeavers, num: Explained::sum_or_zero("Beavers", "", stage_parts(&all)), tone: Tone::Normal, frac: None },
        Kpi { icon: Icon::Mood, tone: pct_tone(mood.value), frac: Some(mood.value / 100.0), num: mood },
        need_kpi(c, "Food need met", Icon::Food, |b| b.needs.food),
        need_kpi(c, "Rest need met", Icon::Rest, |b| b.needs.rest),
        need_kpi(c, "Shelter need met", Icon::Shelter, |b| 0.5 * (b.needs.warmth + b.needs.safety)),
        need_kpi(c, "Company need met", Icon::Social, |b| b.needs.social),
    ];
    let families = Table {
        title: "Families",
        name_col: "Family",
        name_w: NAME_W,
        cols: vec![Col { name: "Beavers", width: 64.0 }, Col { name: "Mood", width: 64.0 }, Col { name: "Lodge", width: 64.0 }],
        rows: c
            .families
            .iter()
            .filter(|f| !members(c, f.id).is_empty())
            .map(|f| {
                let m = members(c, f.id);
                let mood = Explained::sum(
                    &format!("{} family mood", f.name),
                    "%",
                    m.iter().map(|b| Explained::part(b.name.clone(), b.mood * 100.0 / m.len() as f32)).collect(),
                );
                let lodge = c.lodges.iter().find(|l| l.family == f.id);
                Row {
                    icon: Icon::Social,
                    name: f.name.clone(),
                    click: m.first().map(|b| Do::Pick(b.id)),
                    cells: vec![
                        Cell::num(Explained::sum(&format!("{} family size", f.name), "", stage_parts(&m))),
                        Cell::Num { tone: pct_tone(mood.value), frac: Some(mood.value / 100.0), num: mood },
                        match lodge {
                            Some(l) => Cell::num(Explained::fact(&format!("{} lodge condition", f.name), "%", l.condition * 100.0)).bar(l.condition),
                            None => Cell::Text("none".into(), Tone::Dim),
                        },
                    ],
                }
            })
            .collect(),
        empty: "No family has settled yet.",
    };
    let mut order: Vec<&Beaver> = c.beavers.iter().collect();
    order.sort_by(|a, b| a.name.cmp(&b.name).then(a.id.0.cmp(&b.id.0)));
    let beavers = Table {
        title: "Beavers",
        name_col: "Beaver",
        name_w: NAME_W,
        cols: vec![
            Col { name: "Family", width: 84.0 },
            Col { name: "Age (yrs)", width: 66.0 },
            Col { name: "Job", width: 86.0 },
            Col { name: "Mood", width: 64.0 },
            Col { name: "", width: 70.0 },
        ],
        rows: order
            .into_iter()
            .map(|b| {
                let mood = mood_explained(b);
                Row {
                    icon: if b.stage == Stage::Kit { Icon::Rest } else { Icon::Social },
                    name: b.name.clone(),
                    click: Some(Do::Pick(b.id)),
                    cells: vec![
                        Cell::text(family_name(c, b.family)),
                        Cell::num(Explained::fact(&format!("{} age", b.name), "", b.age_years()).dp(1)),
                        match job_of(b) {
                            Some(j) => Cell::Entry(job_icon(j), format!("{j:?}")),
                            None => Cell::Entry(Icon::Rest, if b.stage == Stage::Kit { "Kit".into() } else { "Idle".into() }),
                        },
                        Cell::Num { tone: pct_tone(mood.value), frac: Some(mood.value / 100.0), num: mood },
                        Cell::show("Show", Do::Pick(b.id)),
                    ],
                }
            })
            .collect(),
        empty: "No beavers yet.",
    };
    Screen { item: NavItem::Colony, kpis, tables: vec![families, beavers] }
}

fn work_screen(sim: &SimState) -> Screen {
    let c = &sim.colony;
    let adults: Vec<&Beaver> = c.beavers.iter().filter(|b| b.stage != Stage::Kit).collect();
    let jobs = c.enabled_jobs();
    let working: Vec<Part> = jobs
        .iter()
        .map(|j| Explained::part(format!("{j:?}"), adults.iter().filter(|b| job_of(b) == Some(*j)).count() as f32))
        .filter(|p| p.value > 0.0)
        .collect();
    let idle: Vec<Part> = adults.iter().filter(|b| job_of(b).is_none()).map(|b| Explained::part(b.name.clone(), 1.0)).collect();
    let kpis = vec![
        Kpi { icon: Icon::Collector, num: Explained::sum_or_zero("Working", "", working), tone: Tone::Good, frac: None },
        Kpi { icon: Icon::Rest, tone: if idle.len() > 1 { Tone::Warn } else { Tone::Dim }, frac: None, num: Explained::sum_or_zero("Idle", "", idle) },
    ];
    let rows = jobs
        .iter()
        .map(|j| {
            let w: Vec<&&Beaver> = adults.iter().filter(|b| job_of(b) == Some(*j)).collect();
            let skill = Explained::sum_or_zero(
                &format!("{j:?} skill"),
                "%",
                w.iter().map(|b| Explained::part(b.name.clone(), b.skill(*j) * 100.0 / w.len() as f32)).collect(),
            );
            let best = adults.iter().max_by(|a, b| a.skill(*j).total_cmp(&b.skill(*j)).then(b.id.0.cmp(&a.id.0)));
            Row {
                icon: job_icon(*j),
                name: format!("{j:?}"),
                click: None,
                cells: vec![
                    Cell::num(Explained::sum_or_zero(&format!("{j:?} workers"), "", w.iter().map(|b| Explained::part(b.name.clone(), 1.0)).collect())),
                    Cell::num(skill),
                    Cell::num(Explained::fact(&format!("{j:?} experience"), "", c.experience[j.index()]).dp(1)),
                    Cell::Actions(vec![
                        Action { icon: Icon::Assign, label: "More".into(), act: Do::Staff(*j, 1) },
                        Action { icon: Icon::Cancel, label: "Fewer".into(), act: Do::Staff(*j, -1) },
                    ]),
                    match best {
                        Some(b) => Cell::Text(b.name.clone(), Tone::Normal),
                        None => Cell::Text("-".into(), Tone::Dim),
                    },
                ],
            }
        })
        .collect();
    let jobs_table = Table {
        title: "Jobs",
        name_col: "Job",
        name_w: NAME_W,
        cols: vec![Col { name: "Workers", width: 66.0 }, Col { name: "Skill", width: 64.0 }, Col { name: "XP", width: 56.0 }, Col { name: "Staff", width: 130.0 }, Col { name: "Best hand", width: 80.0 }],
        rows,
        empty: "No jobs are open.",
    };
    let orders = Table {
        title: "Open work",
        name_col: "Order",
        name_w: NAME_W,
        cols: vec![Col { name: "Work left", width: 76.0 }, Col { name: "", width: 70.0 }],
        rows: c
            .orders
            .iter()
            .map(|o| Row {
                icon: job_icon(o.job),
                name: format!("{:?} order {}", o.job, o.id.0),
                click: Some(Do::Jump(o.pos.0, o.pos.1)),
                cells: vec![Cell::num(Explained::fact(&format!("Order {} work left", o.id.0), "", o.remaining).dp(1)), Cell::show("Show", Do::Jump(o.pos.0, o.pos.1))],
            })
            .collect(),
        empty: "Nothing queued; beavers choose their own work.",
    };
    Screen { item: NavItem::Work, kpis, tables: vec![jobs_table, orders] }
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

fn net_tone(net: f32) -> Tone {
    if net < -0.001 {
        Tone::Warn
    } else if net > 0.001 {
        Tone::Good
    } else {
        Tone::Dim
    }
}

fn stores_screen(sim: &SimState, rates: Rates) -> Screen {
    let c = &sim.colony;
    let s = &sim.buildings.stores;
    let dash = || Cell::Text("-".into(), Tone::Dim);
    let mut rows = vec![
        Row {
            icon: Icon::Food,
            name: "Larder".into(),
            click: None,
            cells: vec![
                Cell::num(Explained::fact("Larder stock", "", c.food)).tone(if c.food_per_capita() < 1.0 { Tone::Bad } else { Tone::Normal }),
                dash(),
                dash(),
                dash(),
                Cell::num(Explained::fact("Larder net per day", "", rates.larder_per_day).dp(1)).tone(net_tone(rates.larder_per_day)),
            ],
        },
        Row {
            icon: Icon::Logs,
            name: "Wood".into(),
            click: None,
            cells: vec![
                Cell::num(Explained::fact("Wood stock", "", c.wood)),
                dash(),
                dash(),
                dash(),
                Cell::num(Explained::fact("Wood net per day", "", rates.wood_per_day).dp(1)).tone(net_tone(rates.wood_per_day)),
            ],
        },
    ];
    for g in Good::ALL {
        let i = g.index();
        let (amount, cap, net) = (s.amount(g), s.capacity(g), s.net_rate(g));
        let name = good_name(g);
        let base = sim::buildings::BASE_CAPACITY;
        let stock = Explained::sum(
            &format!("{name} stock"),
            "",
            vec![
                Explained::part("Made so far", s.produced_total[i]),
                Explained::part("Used so far", -s.consumed_total[i]),
                Explained::part("Started with", amount - s.produced_total[i] + s.consumed_total[i]),
            ],
        );
        rows.push(Row {
            icon: g.into(),
            name: name.into(),
            click: None,
            cells: vec![
                Cell::Num { tone: store_tone(amount, cap, net), frac: Some(amount / cap.max(1.0)), num: stock },
                Cell::num(Explained::sum(&format!("{name} capacity"), "", vec![Explained::part("Base store", base.min(cap)), Explained::part("Warehouses", cap - base.min(cap))])),
                Cell::num(Explained::fact(&format!("{name} made per day"), "", s.production_rate[i]).dp(1)),
                Cell::num(Explained::fact(&format!("{name} used per day"), "", s.consumption_rate[i]).dp(1)),
                Cell::num(Explained::sum(&format!("{name} net per day"), "", vec![Explained::part("Made", s.production_rate[i]), Explained::part("Used", -s.consumption_rate[i])]).dp(1)).tone(net_tone(net)),
            ],
        });
    }
    let table = Table {
        title: "Stores",
        name_col: "Good",
        name_w: NAME_W,
        cols: vec![
            Col { name: "Stock", width: 74.0 },
            Col { name: "Capacity", width: 66.0 },
            Col { name: "Made/day", width: 66.0 },
            Col { name: "Used/day", width: 66.0 },
            Col { name: "Net/day", width: 66.0 },
        ],
        rows,
        empty: "Nothing stored.",
    };
    Screen { item: NavItem::Stores, kpis: Vec::new(), tables: vec![table] }
}

fn building_icon(k: sim::buildings::BuildingKind) -> Icon {
    use sim::buildings::BuildingKind::*;
    match k {
        Lodge | Burrow | GuardPost => Icon::Shelter,
        FoodCache => Icon::Food,
        Workshop | WaterWheel => Icon::Craft,
        Warehouse => Icon::Baskets,
        _ => Icon::Build,
    }
}

fn buildings_screen(sim: &SimState) -> Screen {
    let c = &sim.colony;
    let b = &sim.buildings;
    let sites: Vec<&sim::buildings::Building> = b.buildings.iter().filter(|x| !x.is_complete()).collect();
    let kpis = vec![
        Kpi {
            icon: Icon::Shelter,
            num: Explained::sum_or_zero("Lodges", "", c.lodges.iter().map(|l| Explained::part(format!("{} lodge", family_name(c, l.family)), 1.0)).collect()),
            tone: Tone::Normal,
            frac: None,
        },
        Kpi {
            icon: Icon::Build,
            num: Explained::sum_or_zero(
                "Buildings done",
                "",
                sim::buildings::BuildingKind::ALL.iter().map(|k| Explained::part(k.spec().name, b.count_complete(*k) as f32)).filter(|p| p.value > 0.0).collect(),
            ),
            tone: Tone::Normal,
            frac: None,
        },
        Kpi {
            icon: Icon::Builder,
            num: Explained::sum_or_zero("Being built", "", sites.iter().map(|s| Explained::part(s.kind.spec().name, 1.0)).collect()),
            tone: if sites.is_empty() { Tone::Dim } else { Tone::Warn },
            frac: None,
        },
    ];
    let lodges = Table {
        title: "Lodges",
        name_col: "Lodge",
        name_w: NAME_W,
        cols: vec![Col { name: "Residents", width: 72.0 }, Col { name: "Condition", width: 80.0 }, Col { name: "", width: 70.0 }],
        rows: c
            .lodges
            .iter()
            .map(|l| {
                let name = format!("{} lodge", family_name(c, l.family));
                let res: Vec<Part> = c.beavers.iter().filter(|x| x.lodge == Some(l.id)).map(|x| Explained::part(x.name.clone(), 1.0)).collect();
                Row {
                    icon: Icon::Shelter,
                    click: Some(Do::Jump(l.pos.0, l.pos.1)),
                    cells: vec![
                        Cell::num(Explained::sum_or_zero(&format!("{name} residents"), "", res)),
                        Cell::num(Explained::fact(&format!("{name} condition"), "%", l.condition * 100.0)).bar(l.condition).tone(pct_tone(l.condition * 100.0)),
                        Cell::show("Show", Do::Jump(l.pos.0, l.pos.1)),
                    ],
                    name,
                }
            })
            .collect(),
        empty: "No lodge yet.",
    };
    let built = Table {
        title: "Buildings and sites",
        name_col: "Building",
        name_w: NAME_W,
        cols: vec![Col { name: "Progress", width: 80.0 }, Col { name: "", width: 70.0 }],
        rows: b
            .buildings
            .iter()
            .map(|x| {
                let spec = x.kind.spec();
                let name = format!("{} {}", spec.name, x.id.0);
                let at = (x.pos.0 as f32, x.pos.1 as f32);
                let progress = match x.status {
                    Status::Complete => Cell::Text("Complete".into(), Tone::Good),
                    Status::UnderConstruction(o) => {
                        let left = c.orders.iter().find(|w| w.id == o).map(|w| w.remaining).unwrap_or(0.0).clamp(0.0, spec.work);
                        let done = (spec.work - left) / spec.work.max(0.001) * 100.0;
                        Cell::num(Explained::sum(&format!("{name} progress"), "%", vec![Explained::part("Work done", done)])).bar(done / 100.0)
                    }
                };
                Row { icon: building_icon(x.kind), name, click: Some(Do::Jump(at.0, at.1)), cells: vec![progress, Cell::show("Show", Do::Jump(at.0, at.1))] }
            })
            .collect(),
        empty: "Nothing built yet besides the lodge.",
    };
    let mut build_rows = Vec::new();
    for e in crate::build_logic::menu(sim) {
        let spec = e.kind.spec();
        let cost = Explained::sum(&format!("{} cost", spec.name), "", spec.cost.iter().map(|(g, a)| Explained::part(good_name(*g), *a)).collect());
        let (state, action) = match e.need {
            None => (Cell::Text("ready".into(), Tone::Good), Cell::Actions(vec![Action { icon: Icon::Build, label: "Build".into(), act: Do::Place(e.kind) }])),
            Some((tech, _, _)) => (Cell::Text(format!("needs {tech}"), Tone::Dim), Cell::Text(String::new(), Tone::Dim)),
        };
        build_rows.push(Row { icon: building_icon(e.kind), name: spec.name.into(), click: None, cells: vec![Cell::num(cost), state, action] });
    }
    let menu_table = Table {
        title: "Build something",
        name_col: "Building",
        name_w: NAME_W,
        cols: vec![Col { name: "Cost", width: 56.0 }, Col { name: "State", width: 150.0 }, Col { name: "", width: 80.0 }],
        rows: build_rows,
        empty: "Nothing can be built yet.",
    };
    Screen { item: NavItem::Buildings, kpis, tables: vec![lodges, built, menu_table] }
}

fn effect_text(e: &Effect) -> String {
    match e {
        Effect::Unlock(k) => k.spec().name.to_string(),
        Effect::Bonus(Bonus::BuildWork(_)) => "faster building".into(),
        Effect::Bonus(Bonus::CraftSpeed(_)) => "faster crafting".into(),
        Effect::Bonus(Bonus::Storage(_)) => "bigger stores".into(),
    }
}

fn tech_screen(sim: &SimState) -> Screen {
    let c = &sim.colony;
    let visible = |d: &sim::tech::TechDef| super::screens::tech_visible(&sim.features, d);
    let branch_rows = Branch::ALL
        .iter()
        .filter(|br| TECHS.iter().any(|d| d.branch == **br && visible(d)))
        .map(|br| {
            let xp = br.experience(c);
            let done: Vec<Part> = TECHS
                .iter()
                .enumerate()
                .filter(|(i, d)| d.branch == *br && visible(d) && sim.tech.is_unlocked(TechId(*i)))
                .map(|(_, d)| Explained::part(d.name, 1.0))
                .collect();
            Row {
                icon: Icon::NavTech,
                name: format!("{br:?}"),
                click: None,
                cells: vec![Cell::num(Explained::fact(&format!("{br:?} experience"), "", xp).dp(1)), Cell::num(Explained::sum_or_zero(&format!("{br:?} techs unlocked"), "", done))],
            }
        })
        .collect();
    let branches = Table {
        title: "Branches",
        name_col: "Branch",
        name_w: NAME_W,
        cols: vec![Col { name: "XP", width: 60.0 }, Col { name: "Unlocked", width: 70.0 }],
        rows: branch_rows,
        empty: "No techs.",
    };
    let techs = Table {
        title: "All techs",
        name_col: "Tech",
        name_w: 150.0,
        cols: vec![Col { name: "Tier", width: 44.0 }, Col { name: "XP needed", width: 70.0 }, Col { name: "State", width: 70.0 }, Col { name: "Gives", width: 150.0 }],
        rows: TECHS
            .iter()
            .enumerate()
            .filter(|(_, d)| visible(d))
            .map(|(i, d)| {
                let on = sim.tech.is_unlocked(TechId(i));
                Row {
                    icon: Icon::NavTech,
                    name: d.name.to_string(),
                    click: None,
                    cells: vec![
                        Cell::num(Explained::fact(&format!("{} tier", d.name), "", d.tier as f32)),
                        Cell::num(Explained::fact(&format!("{} xp needed", d.name), "", d.xp)),
                        Cell::Text(if on { "unlocked" } else { "locked" }.into(), if on { Tone::Good } else { Tone::Dim }),
                        Cell::text(d.effects.iter().map(effect_text).collect::<Vec<_>>().join(", ")),
                    ],
                }
            })
            .collect(),
        empty: "No techs.",
    };
    Screen { item: NavItem::Tech, kpis: Vec::new(), tables: vec![branches, techs] }
}

/// A tech is listed unless everything it does is unlocking buildings of a disabled system.
pub fn tech_visible(features: &sim::features::Features, d: &sim::tech::TechDef) -> bool {
    d.effects.iter().any(|e| match e {
        Effect::Unlock(k) => features.allows_building(*k),
        _ => true,
    })
}

fn option_icon(n: usize) -> Icon {
    match n {
        0 => Icon::Accept,
        1 => Icon::TurnAway,
        _ => Icon::Assign,
    }
}

fn events_screen(sim: &SimState) -> Screen {
    let expiry = sim::events::CHOICE_EXPIRY_DAYS as f32;
    let now = sim.events.calendar.elapsed_days();
    let mut waiting: Vec<Row> = sim
        .events
        .pending()
        .iter()
        .map(|p| {
            let waited = (now - p.day) as f32;
            Row {
                icon: Icon::NavEvents,
                name: format!("{} - {}", kind_label(p.kind), p.prompt),
                click: None,
                cells: vec![
                    Cell::num(Explained::sum(
                        &format!("{} days left", kind_label(p.kind)),
                        "",
                        vec![Explained::part("Time allowed", expiry), Explained::part("Already waited", -waited)],
                    ).dp(1)),
                    Cell::Actions(
                        p.options.iter().enumerate().map(|(n, o)| Action { icon: option_icon(n), label: o.label.clone(), act: Do::Choose(p.id, n) }).collect(),
                    ),
                ],
            }
        })
        .collect();
    waiting.extend(sim.society.pending().iter().map(|r| Row {
        icon: Icon::Social,
        name: format!("{} asks to join", r.name),
        click: Some(Do::Jump(r.at.0 as f32, r.at.1 as f32)),
        cells: vec![
            Cell::Text(if r.adult { "adult".into() } else { "kit".into() }, Tone::Dim),
            Cell::Actions(vec![
                Action { icon: Icon::Accept, label: "Welcome".into(), act: Do::Answer(r.id, true) },
                Action { icon: Icon::TurnAway, label: "Turn away".into(), act: Do::Answer(r.id, false) },
            ]),
        ],
    }));
    let decisions = Table {
        title: "Decisions",
        name_col: "Waiting for you",
        name_w: 200.0,
        cols: vec![Col { name: "Days left", width: 70.0 }, Col { name: "Choose", width: 200.0 }],
        rows: waiting,
        empty: "Nothing needs a decision.",
    };
    let log = Table {
        title: "Event log",
        name_col: "Event",
        name_w: 110.0,
        cols: vec![Col { name: "When", width: 96.0 }, Col { name: "What happened", width: 240.0 }],
        rows: sim
            .events
            .log()
            .iter()
            .rev()
            .take(40)
            .map(|r| Row {
                icon: Icon::NavEvents,
                name: kind_label(r.kind),
                click: None,
                cells: vec![Cell::text(format!("{:?} year {}", r.date.season, r.date.year + 1)), Cell::text(r.description.clone())],
            })
            .collect(),
        empty: "Nothing has happened yet.",
    };
    Screen { item: NavItem::Events, kpis: Vec::new(), tables: vec![decisions, log] }
}

// ───────────────────────────── history ─────────────────────────────

/// Larder and wood at recent days, to turn into per-day rates.
#[derive(Clone, Debug, Default)]
pub struct History {
    samples: std::collections::VecDeque<(f64, f32, f32)>,
}

impl History {
    pub fn record(&mut self, sim: &SimState) {
        let s = (sim.time_days, sim.colony.food, sim.colony.wood);
        if self.samples.back().is_some_and(|l| l.0 > s.0) {
            self.samples.clear(); // a restarted world rewinds time
        }
        if self.samples.back().is_none_or(|l| s.0 - l.0 >= 0.5) {
            self.samples.push_back(s);
            while self.samples.len() > 20 {
                self.samples.pop_front();
            }
        }
    }
    pub fn rates(&self) -> Rates {
        match (self.samples.front(), self.samples.back()) {
            (Some(a), Some(b)) if b.0 - a.0 > 1e-3 => {
                let dt = (b.0 - a.0) as f32;
                Rates { larder_per_day: (b.1 - a.1) / dt, wood_per_day: (b.2 - a.2) / dt }
            }
            _ => Rates::default(),
        }
    }
}

// ───────────────────────────── tests ─────────────────────────────

#[cfg(test)]
pub mod tests {
    use super::*;
    use crate::ui::explain::audit;

    /// A settled colony that has lived a while, with stock, so every table has rows.
    pub fn lived_in(seed: u64) -> SimState {
        let mut sim = SimState::new(seed);
        let mut flow = crate::flow::Flow::new(&sim.world);
        crate::opening::found(&mut sim, &mut flow).expect("founds");
        for _ in 0..800 {
            sim.step(0.25).expect("step");
        }
        for (g, a) in [(Good::Logs, 64.0), (Good::Sticks, 41.0), (Good::Mud, 28.0), (Good::Stone, 12.0), (Good::Baskets, 6.0)] {
            let _ = sim.buildings.stores.deposit(g, a);
        }
        let _ = sim.buildings.stores.withdraw(Good::Logs, 18.0);
        let ctx = sim.event_context();
        let mut host = sim::ColonyHost { colony: &mut sim.colony };
        let _ = sim.events.trigger(sim::events::EventKind::WanderingBeaver, &ctx, &mut host);
        sim
    }

    fn states() -> Vec<SimState> {
        let mut fresh = SimState::new(1);
        let mut flow = crate::flow::Flow::new(&fresh.world);
        crate::opening::found(&mut fresh, &mut flow).unwrap();
        vec![fresh, lived_in(3)]
    }

    #[test]
    fn every_number_adds_up_and_nothing_is_shown_twice() {
        for sim in states() {
            for item in NavItem::ALL {
                let s = screen(&sim, item, Rates { larder_per_day: 1.5, wood_per_day: -0.5 });
                let bad = audit(&s);
                assert!(bad.is_empty(), "{item:?}: {bad:?}");
                assert!(!s.numbers().is_empty() || item == NavItem::Events || item == NavItem::Tech, "{item:?} shows no numbers");
            }
            let nav = nav_bar(&sim, None);
            assert!(audit(&nav).is_empty());
            assert!(audit(&time_bar(&sim)).is_empty());
        }
    }

    #[test]
    fn numbers_on_screens_are_not_trivially_unexplained() {
        // The colony screen's population and mood break down into several terms.
        let sim = lived_in(3);
        let s = screen(&sim, NavItem::Colony, Rates::default());
        assert!(s.kpis[0].num.parts.len() >= 2, "{:?}", s.kpis[0].num);
        assert!(s.kpis[1].num.parts.len() >= 1);
        let beavers = &s.tables[1];
        for r in &beavers.rows {
            let Cell::Num { num, .. } = &r.cells[3] else { panic!("mood cell") };
            assert!(num.parts.len() >= 5, "{} mood breaks into the needs", r.name);
        }
    }

    #[test]
    fn every_resource_and_action_entry_has_an_icon_file() {
        let root = crate::ui::icons::asset_root();
        let mut checked = 0;
        for sim in states() {
            let mut screens: Vec<Screen> = NavItem::ALL.iter().map(|i| screen(&sim, *i, Rates::default())).collect();
            screens.retain(|s| !s.tables.is_empty());
            for s in &screens {
                for a in s.actions() {
                    assert!(root.join(a.icon.file()).is_file(), "{:?} action '{}' lacks its icon file", s.item, a.label);
                    assert!(!a.label.is_empty() || a.icon != Icon::Cancel);
                    checked += 1;
                }
                for i in s.icons() {
                    assert!(Icon::ALL.contains(&i));
                    assert!(root.join(i.file()).is_file(), "{:?}: {}", s.item, i.file());
                    checked += 1;
                }
                for t in &s.tables {
                    for r in &t.rows {
                        // a row is a thing in the colony: its icon is the first thing drawn
                        assert!(root.join(r.icon.file()).is_file(), "{}: row '{}'", t.title, r.name);
                    }
                }
            }
            for e in nav_bar(&sim, None).entries {
                assert_eq!(e.icon, e.item.icon());
                assert!(root.join(e.icon.file()).is_file(), "nav {}", e.label);
                checked += 1;
            }
        }
        assert!(checked > 40, "only {checked} icons checked");
    }

    #[test]
    fn stores_list_every_good_with_its_resource_icon() {
        let sim = lived_in(3);
        let s = screen(&sim, NavItem::Stores, Rates::default());
        let rows = &s.tables[0].rows;
        for g in Good::ALL {
            let row = rows.iter().find(|r| r.name == good_name(g)).expect("good listed");
            assert_eq!(row.icon, Icon::from(g));
        }
    }

    #[test]
    fn sorting_a_table_reorders_rows_and_reverses() {
        let sim = lived_in(3);
        let mut s = screen(&sim, NavItem::Stores, Rates::default());
        let rows = &mut s.tables[0].rows;
        sort_rows(rows, 1, false);
        let stock: Vec<f32> = rows.iter().map(|r| match &r.cells[0] { Cell::Num { num, .. } => num.value, _ => 0.0 }).collect();
        assert!(stock.windows(2).all(|w| w[0] >= w[1]), "{stock:?}");
        sort_rows(rows, 1, true);
        let up: Vec<f32> = rows.iter().map(|r| match &r.cells[0] { Cell::Num { num, .. } => num.value, _ => 0.0 }).collect();
        assert!(up.windows(2).all(|w| w[0] <= w[1]));
        sort_rows(rows, 0, true);
        assert!(rows.windows(2).all(|w| w[0].name.to_lowercase() <= w[1].name.to_lowercase()));
    }

    #[test]
    fn time_bar_day_counts_from_one() {
        let sim = SimState::new(1);
        let t = time_bar(&sim);
        assert_eq!(t.day.value, 1.0);
        assert!(t.day.sums());
    }

    #[test]
    fn decisions_badge_counts_pending_choices() {
        let sim = lived_in(3);
        let n = decisions(&sim).map(|d| d.value).unwrap_or(0.0);
        assert_eq!(n as usize, sim.events.pending().len() + sim.society.pending().len());
        let nav = nav_bar(&sim, Some(NavItem::Events));
        assert!(nav.entries.iter().find(|e| e.item == NavItem::Events).unwrap().open);
    }
}
