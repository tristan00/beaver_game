//! Entity selection and panels (owner: `panels`). `panel` is a pure function from sim state and a
//! selection to a view-model, so it is testable without a window. What matters is at the top
//! (title, one line of character, headline numbers); the rest sits in groups and rows the person
//! reads further down. Every number is an [`Explained`], so hovering it shows what adds up to it.

use sim::buildings::{BuildingId, BuildingKind, Good, Status};
use sim::colony::{Beaver, BeaverId, FamilyId, Job, Stage};
use sim::resources::{Resource, ResourceId, ResourceKind, FELL_WORK};
use sim::tuning::colony::{MOOD_FOOD_WEIGHT, MOOD_REST_WEIGHT, MOOD_SAFETY_WEIGHT, MOOD_SOCIAL_WEIGHT, MOOD_WARMTH_WEIGHT};
use sim::work::{Role, Workplace};
use sim::{Command, SimState};

use super::explain::{Explained, Numbers};
use super::icons::Icon;
use super::screens::Tone;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Selection {
    Beaver(BeaverId),
    Family(FamilyId),
    /// A building or construction site (including a dam).
    Building(BuildingId),
    /// A water dam, picked by one of its cells (start 2's dam has no building record).
    Dam((usize, usize)),
    Tree(ResourceId),
}

/// A button on a panel; `command` is what clicking sends to `SimState::apply`. A button with no
/// command is shown dimmed, and its label says why.
#[derive(Clone, Debug, PartialEq)]
pub struct PanelAction {
    pub icon: Icon,
    pub label: String,
    pub command: Option<Command>,
}

/// One number with an optional meter under it.
#[derive(Clone, Debug, PartialEq)]
pub struct Stat {
    pub num: Explained,
    pub frac: Option<f32>,
    pub tone: Tone,
}

impl Stat {
    fn new(num: Explained) -> Self {
        Stat { num, frac: None, tone: Tone::Normal }
    }
    fn bar(mut self, f: f32) -> Self {
        self.frac = Some(f.clamp(0.0, 1.0));
        self
    }
    fn tone(mut self, t: Tone) -> Self {
        self.tone = t;
        self
    }
}

/// A titled block of numbers (needs, skills, costs…), read on request.
#[derive(Clone, Debug, PartialEq)]
pub struct Group {
    pub title: &'static str,
    pub stats: Vec<Stat>,
}

/// A fact that is a word, not a number (stage, task, species).
#[derive(Clone, Debug, PartialEq)]
pub struct Fact {
    pub icon: Icon,
    pub label: &'static str,
    pub value: String,
}

/// A family member row; clicking it selects the beaver.
#[derive(Clone, Debug, PartialEq)]
pub struct Member {
    pub id: BeaverId,
    pub name: String,
    pub stage: Stage,
    pub mood: f32,
}

/// A beaver that may be put into a slot; picking one issues `fill`.
#[derive(Clone, Debug, PartialEq)]
pub struct Candidate {
    pub who: BeaverId,
    pub name: String,
    pub fill: Command,
}

/// A worker slot row on building and site panels.
#[derive(Clone, Debug, PartialEq)]
pub struct SlotRow {
    pub slot: usize,
    pub role: &'static str,
    pub icon: Icon,
    pub worker: Option<(BeaverId, String)>,
    /// Frees the slot (filled slots only).
    pub empty: Option<Command>,
    /// Beavers that can fill it (free slots only; kits never appear), nearest first.
    pub candidates: Vec<Candidate>,
}

/// What a site needs of one good.
#[derive(Clone, Debug, PartialEq)]
pub struct CostRow {
    pub icon: Icon,
    pub good: Good,
    pub delivered: f32,
    pub needed: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Panel {
    pub kind: &'static str,
    pub icon: Icon,
    pub title: String,
    /// One line of character; never mechanics.
    pub prose: String,
    pub facts: Vec<Fact>,
    /// Headline numbers.
    pub stats: Vec<Stat>,
    /// Detail further down: needs, skills, household…
    pub groups: Vec<Group>,
    pub members: Vec<Member>,
    pub costs: Vec<CostRow>,
    pub slots: Vec<SlotRow>,
    pub actions: Vec<PanelAction>,
    /// Where the camera flies on "show on map".
    pub at: (usize, usize),
}

impl Panel {
    pub fn stat(&self, label: &str) -> Option<&Stat> {
        self.stats.iter().chain(self.groups.iter().flat_map(|g| g.stats.iter())).find(|s| s.num.label == label)
    }
    pub fn fact(&self, label: &str) -> Option<&str> {
        self.facts.iter().find(|f| f.label == label).map(|f| f.value.as_str())
    }
    pub fn group(&self, title: &str) -> Option<&Group> {
        self.groups.iter().find(|g| g.title == title)
    }
}

impl Numbers for Panel {
    fn numbers(&self) -> Vec<&Explained> {
        self.stats.iter().chain(self.groups.iter().flat_map(|g| g.stats.iter())).map(|s| &s.num).collect()
    }
}

/// Builds the panel for a selection; `None` if the entity no longer exists.
pub fn panel(sim: &SimState, sel: Selection) -> Option<Panel> {
    match sel {
        Selection::Beaver(id) => beaver_panel(sim, id),
        Selection::Family(id) => family_panel(sim, id),
        Selection::Building(id) => building_panel(sim, id),
        Selection::Dam(cell) => sim.water.has_dam(cell.0, cell.1).then(|| dam_panel(sim, cell, None)),
        Selection::Tree(id) => tree_panel(sim, id),
    }
}

/// Where a selection is on the map, if it still exists (the camera flies here).
pub fn locate(sim: &SimState, sel: Selection) -> Option<(usize, usize)> {
    panel(sim, sel).map(|p| p.at)
}

// ───────────────────────────── shared helpers ─────────────────────────────

fn pct(v: f32) -> f32 {
    (v * 100.0).clamp(0.0, 100.0)
}

fn tone_of(v: f32) -> Tone {
    if v < 0.3 { Tone::Bad } else if v < 0.55 { Tone::Warn } else { Tone::Good }
}

/// A stored `0..=1` figure shown as a percentage, read from `origin`.
fn level(label: &str, v: f32, origin: &str) -> Stat {
    Stat::new(Explained::reading(label, "%", pct(v), origin)).bar(v).tone(tone_of(v))
}

fn stage_name(s: Stage) -> &'static str {
    match s {
        Stage::Kit => "Kit",
        Stage::Yearling => "Yearling",
        Stage::Adult => "Adult",
        Stage::Elder => "Elder",
    }
}

fn job_name(j: Job) -> &'static str {
    match j {
        Job::Forager => "Forager",
        Job::Logger => "Logger",
        Job::Builder => "Builder",
        Job::Digger => "Digger",
        Job::Hauler => "Hauler",
        Job::Guard => "Guard",
        Job::Crafter => "Crafter",
        Job::Scout => "Scout",
    }
}

fn job_icon(j: Job) -> Icon {
    match j {
        Job::Forager | Job::Logger => Icon::Collector,
        Job::Builder | Job::Digger => Icon::Builder,
        Job::Hauler => Icon::Hauler,
        Job::Guard | Job::Crafter | Job::Scout => Icon::Specialist,
    }
}

fn kind_name(k: ResourceKind) -> &'static str {
    match k {
        ResourceKind::Tree => "trees",
        ResourceKind::Felled => "felled logs",
        ResourceKind::Plant => "plants",
        ResourceKind::MudBank => "mud",
        ResourceKind::StoneOutcrop => "stone",
    }
}

fn role_name(r: Role) -> String {
    match r {
        Role::Collect(k) => format!("Collect {}", kind_name(k)),
        Role::Build => "Build".into(),
        Role::Haul => "Haul".into(),
        Role::Craft => "Craft".into(),
        Role::Study => "Study".into(),
    }
}

fn role_icon(r: Role) -> Icon {
    match r {
        Role::Collect(_) => Icon::Collector,
        Role::Build => Icon::Builder,
        Role::Haul => Icon::Hauler,
        Role::Craft => Icon::Specialist,
        Role::Study => Icon::Specialist,
    }
}

fn cell_of(p: (f32, f32)) -> (usize, usize) {
    (p.0.max(0.0).round() as usize, p.1.max(0.0).round() as usize)
}

fn dist2(a: (f32, f32), b: (usize, usize)) -> f32 {
    let (dx, dy) = (a.0 - b.0 as f32, a.1 - b.1 as f32);
    dx * dx + dy * dy
}

fn family_name(sim: &SimState, id: FamilyId) -> String {
    sim.colony.family(id).map_or_else(|| "unknown".into(), |f| f.name.clone())
}

fn show_stat(g: &mut Vec<Stat>, s: Stat) {
    g.push(s);
}

// ───────────────────────────── beaver ─────────────────────────────

fn beaver_panel(sim: &SimState, id: BeaverId) -> Option<Panel> {
    let b = sim.colony.beaver(id)?;
    let agent = sim.agents.get(id);
    let slot = sim.work.slot_of(id);
    let task = match (agent.map(|a| a.task.label()), slot) {
        (Some(t), Some((site, _, role))) => format!("{t} ({} at {})", role_name(role).to_lowercase(), building_name(sim, site)),
        (Some(t), None) => t.to_string(),
        (None, _) => "out of sight".to_string(),
    };
    let family = family_name(sim, b.family);

    let facts = vec![
        Fact { icon: Icon::Social, label: "Stage", value: stage_name(b.stage).into() },
        Fact { icon: Icon::Shelter, label: "Family", value: format!("{family} family") },
        Fact { icon: Icon::Assign, label: "Task", value: task },
    ];
    let mut stats = vec![
        Stat::new(Explained::reading("Age", " yr", b.age_years(), "Beaver record").dp(1)),
        level("Mood", b.mood, "Beaver record").tone(tone_of(b.mood)),
    ];
    show_stat(&mut stats, level("Health", b.health, "Beaver record"));

    let n = b.needs;
    let needs = Group {
        title: "Needs",
        stats: vec![
            level("Food", n.food, "Needs record"),
            level("Rest", n.rest, "Needs record"),
            level("Warmth", n.warmth, "Needs record"),
            level("Safety", n.safety, "Needs record"),
            level("Company", n.social, "Needs record"),
        ],
    };
    let skills = Group {
        title: "Skills",
        stats: Job::ALL.iter().map(|&j| level(job_name(j), b.skill(j), "Skill record")).collect(),
    };

    Some(Panel {
        kind: "Beaver",
        icon: Icon::NavBeavers,
        title: b.name.clone(),
        prose: beaver_prose(b, &family),
        facts,
        stats,
        groups: vec![needs, skills],
        members: Vec::new(),
        costs: Vec::new(),
        slots: Vec::new(),
        actions: beaver_actions(sim, b),
        at: cell_of(b.pos),
    })
}

fn beaver_prose(b: &Beaver, family: &str) -> String {
    let manner = if b.mood >= 0.7 { "bright-eyed" } else if b.mood >= 0.4 { "steady" } else { "weary" };
    let craft = b.specialty().map_or(String::new(), |j| format!(", known along the pond as a fine {}", job_name(j).to_lowercase()));
    let who = match b.stage {
        Stage::Kit => "a kit, all paws and curiosity",
        Stage::Yearling => "a yearling, still finding their feet",
        Stage::Adult => "a grown beaver",
        Stage::Elder => "an elder whose tail has seen many floods",
    };
    format!("{} is {manner} - {who} of the {family} lodge{craft}.", b.name)
}

/// Where this beaver could work: every workplace with a free slot, nearest first (at most
/// three). A kit gets one dimmed button that says why it cannot.
fn beaver_actions(sim: &SimState, b: &Beaver) -> Vec<PanelAction> {
    if b.stage == Stage::Kit {
        return vec![PanelAction { icon: Icon::Assign, label: "Too young to work".into(), command: None }];
    }
    let mut open: Vec<(f32, BuildingId, usize, Role)> = sim
        .work
        .places
        .iter()
        .filter_map(|p| {
            let at = sim.buildings.building(p.building)?.pos;
            let i = p.slots.iter().position(|s| s.worker.is_none())?;
            Some((dist2(b.pos, at), p.building, i, p.slots[i].role))
        })
        .collect();
    open.sort_by(|a, c| a.0.total_cmp(&c.0));
    let mut out: Vec<PanelAction> = open
        .into_iter()
        .take(3)
        .map(|(_, building, slot, role)| PanelAction {
            icon: Icon::Assign,
            label: format!("Assign: {} at {}", role_name(role).to_lowercase(), building_name(sim, building)),
            command: Some(Command::FillSlot { building, slot, who: b.id }),
        })
        .collect();
    if let Some((building, slot, _)) = sim.work.slot_of(b.id) {
        out.push(PanelAction { icon: Icon::Cancel, label: "Stand down".into(), command: Some(Command::EmptySlot { building, slot }) });
    }
    if out.is_empty() {
        out.push(PanelAction { icon: Icon::Assign, label: "No open work slots".into(), command: None });
    }
    out
}

// ───────────────────────────── family ─────────────────────────────

fn family_panel(sim: &SimState, id: FamilyId) -> Option<Panel> {
    let fam = sim.colony.family(id)?;
    let folk: Vec<&Beaver> = sim.colony.beavers.iter().filter(|b| b.family == id).collect();
    let count = |s: Stage| folk.iter().filter(|b| b.stage == s).count() as f32;
    let members = Explained::sum(
        "Members",
        "",
        vec![
            Explained::part("Adults", count(Stage::Adult)),
            Explained::part("Elders", count(Stage::Elder)),
            Explained::part("Yearlings", count(Stage::Yearling)),
            Explained::part("Kits", count(Stage::Kit)),
        ],
    );
    let kits: Vec<&&Beaver> = folk.iter().filter(|b| b.stage == Stage::Kit).collect();
    let kit_num = Explained::sum_or_zero("Kits", "", kits.iter().map(|k| Explained::part(k.name.clone(), 1.0)).collect());
    let mood_each = |b: &Beaver| Explained::part(b.name.clone(), pct(b.mood) / folk.len().max(1) as f32);
    let mood = Explained::sum_or_zero("Family mood", "%", folk.iter().map(|b| mood_each(b)).collect()).dp(0);
    let mood_frac = mood.value / 100.0;

    let lodge = sim.colony.lodges.iter().find(|l| l.family == id);
    let mut facts = vec![Fact { icon: Icon::Shelter, label: "Lodge", value: lodge.map_or("none yet".into(), |l| format!("{} lodge", fam.name)) }];
    let jobs: Vec<&str> = fam.priorities.iter().map(|&j| job_name(j)).collect();
    if !jobs.is_empty() {
        facts.push(Fact { icon: Icon::Assign, label: "Priorities", value: jobs.join(", ") });
    }
    let mut stats = vec![Stat::new(members), Stat::new(kit_num), Stat::new(mood).bar(mood_frac).tone(tone_of(mood_frac))];
    let mut groups = Vec::new();
    if let Some(l) = lodge {
        groups.push(Group { title: "Lodge", stats: vec![level("Lodge condition", l.condition, "Lodge record")] });
    } else {
        show_stat(&mut stats, Stat::new(Explained::reading("Lodge condition", "%", 0.0, "No lodge")));
    }
    let mut folk_rows: Vec<Member> =
        folk.iter().map(|b| Member { id: b.id, name: b.name.clone(), stage: b.stage, mood: b.mood }).collect();
    folk_rows.sort_by_key(|m| (std::cmp::Reverse(m.stage), m.name.clone()));
    let at = lodge.map(|l| cell_of(l.pos)).or_else(|| folk.first().map(|b| cell_of(b.pos))).unwrap_or((0, 0));

    Some(Panel {
        kind: "Family",
        icon: Icon::Social,
        title: format!("The {} family", fam.name),
        prose: family_prose(&fam.name, folk.len(), kits.len(), mood_frac),
        facts,
        stats,
        groups,
        members: folk_rows,
        costs: Vec::new(),
        slots: Vec::new(),
        actions: Vec::new(),
        at,
    })
}

fn family_prose(name: &str, n: usize, kits: usize, mood: f32) -> String {
    let feel = if mood >= 0.7 { "content" } else if mood >= 0.4 { "getting by" } else { "restless" };
    let kids = if kits > 0 { "with small paws underfoot" } else { "with a quiet lodge" };
    format!("The {name} family, {n} strong, are {feel}, {kids}.")
}

// ───────────────────────────── buildings, sites and dams ─────────────────────────────

fn building_name(sim: &SimState, id: BuildingId) -> String {
    sim.buildings.building(id).map_or_else(|| "a building".into(), |b| b.kind.spec().name.to_string())
}

fn building_panel(sim: &SimState, id: BuildingId) -> Option<Panel> {
    let b = sim.buildings.building(id)?;
    let place = sim.work.place(id);
    let at = b.pos;
    let name = b.kind.spec().name;
    let is_site = b.status != Status::Complete;
    if b.kind == BuildingKind::Dam && !is_site {
        return Some(dam_panel(sim, b.pos, place));
    }

    let mut stats = Vec::new();
    let mut costs = Vec::new();
    match place.and_then(|p| p.site.as_ref()) {
        Some(s) => {
            let mats = {
                let need: f32 = s.needed.iter().map(|a| a.1).sum();
                let got: f32 = s.delivered.iter().map(|a| a.1).sum();
                if need > 0.0 { (got / need).min(1.0) } else { 1.0 }
            };
            let labour = if s.labour_needed > 0.0 { (s.labour_done / s.labour_needed).min(1.0) } else { 1.0 };
            let progress = Explained::sum(
                "Progress",
                "%",
                vec![Explained::part("Materials (half)", 50.0 * mats), Explained::part("Labour (half)", 50.0 * labour)],
            );
            stats.push(Stat::new(progress).bar(s.fraction()).tone(Tone::Good));
            let delivered = Explained::sum_or_zero(
                "Cost delivered",
                "",
                s.delivered.iter().filter(|d| d.1 > 0.0).map(|d| Explained::part(good_label(d.0), d.1)).collect(),
            )
            .dp(1);
            let needed: f32 = s.needed.iter().map(|a| a.1).sum();
            stats.push(Stat::new(delivered).bar(if needed > 0.0 { s.delivered.iter().map(|a| a.1).sum::<f32>() / needed } else { 1.0 }));
            stats.push(Stat::new(Explained::reading("Labour done", "", s.labour_done, "Site ledger").dp(1)).bar(labour));
            for n in &s.needed {
                let got = s.delivered.iter().find(|d| d.0 == n.0).map_or(0.0, |d| d.1);
                costs.push(CostRow { icon: n.0.into(), good: n.0, delivered: got, needed: n.1 });
            }
        }
        None => {
            // A finished building: everything delivered, all labour done.
            let spec = b.kind.spec();
            stats.push(Stat::new(Explained::reading("Progress", "%", 100.0, "Finished building")).bar(1.0).tone(Tone::Good));
            let total: f32 = spec.cost.iter().map(|a| a.1).sum();
            stats.push(
                Stat::new(Explained::sum_or_zero("Cost delivered", "", spec.cost.iter().map(|a| Explained::part(good_label(a.0), a.1)).collect()).dp(1))
                    .bar(if total > 0.0 { 1.0 } else { 0.0 }),
            );
            for a in spec.cost {
                costs.push(CostRow { icon: a.0.into(), good: a.0, delivered: a.1, needed: a.1 });
            }
        }
    }
    let slots = place.map(|p| slot_rows(sim, p, at)).unwrap_or_default();
    let (filled, total) = (slots.iter().filter(|s| s.worker.is_some()).count(), slots.len());
    let workers = Explained::sum_or_zero(
        "Workers",
        "",
        slots.iter().filter_map(|s| s.worker.as_ref()).map(|(_, n)| Explained::part(n.clone(), 1.0)).collect(),
    );
    stats.push(Stat::new(workers).bar(if total > 0 { filled as f32 / total as f32 } else { 0.0 }));

    let title = if is_site { format!("{name} site") } else { name.to_string() };
    Some(Panel {
        kind: if is_site { "Construction site" } else { "Building" },
        icon: if is_site { Icon::Build } else { Icon::NavBuild },
        title,
        prose: building_prose(name, is_site, filled, total),
        facts: vec![Fact { icon: Icon::MoveArea, label: "Position", value: format!("({}, {})", at.0, at.1) }],
        stats,
        groups: Vec::new(),
        members: Vec::new(),
        costs,
        slots,
        actions: Vec::new(),
        at,
    })
}

fn good_label(g: Good) -> &'static str {
    super::screens::good_name(g)
}

fn building_prose(name: &str, site: bool, filled: usize, total: usize) -> String {
    let lower = name.to_lowercase();
    match (site, filled) {
        (true, 0) => format!("A {lower} is staked out here, waiting for willing paws."),
        (true, _) => format!("Builders are bent over the {lower}, chewing and stacking."),
        (false, 0) => format!("The {lower} stands finished and quiet - nobody works here yet."),
        (false, n) if n == total => format!("The {lower} hums with a full crew."),
        _ => format!("The {lower} stands finished, with room for more hands."),
    }
}

/// Slot rows for a workplace; free slots list the beavers who could fill them.
fn slot_rows(sim: &SimState, p: &Workplace, at: (usize, usize)) -> Vec<SlotRow> {
    let mut near: Vec<&Beaver> = sim.colony.beavers.iter().filter(|b| b.stage != Stage::Kit).collect();
    near.sort_by(|a, b| {
        // free hands first, then the nearest
        let busy = |x: &Beaver| sim.work.slot_of(x.id).is_some();
        (busy(a), dist2(a.pos, at)).partial_cmp(&(busy(b), dist2(b.pos, at))).unwrap_or(std::cmp::Ordering::Equal)
    });
    p.slots
        .iter()
        .enumerate()
        .map(|(i, s)| {
            let worker = s.worker.and_then(|w| sim.colony.beaver(w).map(|b| (w, b.name.clone())));
            let free = s.worker.is_none();
            SlotRow {
                slot: i,
                role: match s.role {
                    Role::Collect(_) => "Collect",
                    Role::Build => "Build",
                    Role::Haul => "Haul",
                    Role::Craft => "Craft",
                    Role::Study => "Study",
                },
                icon: role_icon(s.role),
                worker,
                empty: (!free).then_some(Command::EmptySlot { building: p.building, slot: i }),
                candidates: if free {
                    near.iter()
                        .take(4)
                        .map(|b| Candidate { who: b.id, name: b.name.clone(), fill: Command::FillSlot { building: p.building, slot: i, who: b.id } })
                        .collect()
                } else {
                    Vec::new()
                },
            }
        })
        .collect()
}

/// The cells of the water dam at `start` (8-connected dam cells around it).
fn dam_cells(sim: &SimState, start: (usize, usize)) -> Vec<(usize, usize)> {
    let mut seen: Vec<(usize, usize)> = Some(start).filter(|&(x, y)| sim.water.has_dam(x, y)).into_iter().collect();
    let mut i = 0;
    while i < seen.len() && seen.len() < 600 {
        let (x, y) = seen[i];
        i += 1;
        for dy in -1i32..=1 {
            for dx in -1i32..=1 {
                let (nx, ny) = (x as i32 + dx, y as i32 + dy);
                if nx < 0 || ny < 0 {
                    continue;
                }
                let c = (nx as usize, ny as usize);
                if sim.water.has_dam(c.0, c.1) && !seen.contains(&c) {
                    seen.push(c);
                }
            }
        }
    }
    seen
}

fn dam_panel(sim: &SimState, at: (usize, usize), place: Option<&Workplace>) -> Panel {
    let cells = dam_cells(sim, at);
    let h = |c: (usize, usize)| sim.height.get(c.0, c.1).copied().unwrap_or(0.0);
    let base = cells.iter().map(|&c| h(c)).fold(f32::INFINITY, f32::min);
    let crest = cells.iter().map(|&c| h(c) + sim.water.dam_height.get(c.0, c.1).copied().unwrap_or(0.0)).fold(f32::INFINITY, f32::min);
    // The pond surface: the highest water on the cells beside the dam.
    let mut surface = base;
    for &(x, y) in &cells {
        for (nx, ny) in [(x.wrapping_sub(1), y), (x + 1, y), (x, y.wrapping_sub(1)), (x, y + 1)] {
            if sim.water.has_dam(nx, ny) {
                continue;
            }
            let (Some(&t), Some(&d)) = (sim.height.get(nx, ny), sim.water.depth.get(nx, ny)) else { continue };
            if d > 0.0 {
                surface = surface.max(t + d);
            }
        }
    }
    let (level_v, crest_v) = ((surface - base).max(0.0), (crest - base).max(0.0));
    let dam_h = cells.iter().map(|&c| sim.water.dam_height.get(c.0, c.1).copied().unwrap_or(0.0)).fold(0.0, f32::max).max(0.1);
    let pressure = sim::dam_preview::preview(sim, &cells, dam_h).map_or(0.0, |p| p.pressure);
    let integrity = if cells.is_empty() {
        0.0
    } else {
        cells.iter().map(|&c| sim.water.dam_integrity.get(c.0, c.1).copied().unwrap_or(0.0)).sum::<f32>() / cells.len() as f32
    };
    let frac = if crest_v > 0.0 { (level_v / crest_v).min(1.0) } else { 0.0 };

    let stats = vec![
        Stat::new(Explained::reading("Water level", "", level_v, "Pond surface above the dam base").dp(2))
            .bar(frac)
            .tone(if frac > 0.95 { Tone::Warn } else { Tone::Good }),
        Stat::new(Explained::reading("Current pressure", "/day", pressure, "Inflow of the springs draining here").dp(2)),
        level("Condition", integrity, "Dam integrity"),
    ];
    let slots = place.map(|p| slot_rows(sim, p, at)).unwrap_or_default();
    let prose = if integrity < 0.5 {
        "The dam groans and seeps; it wants mending before the next flood."
    } else if frac > 0.95 {
        "Water brims at the crest and the whole dam holds its breath."
    } else {
        "The dam holds the pond, its sticks and mud packed tight."
    };
    Panel {
        kind: "Dam",
        icon: Icon::ModeWater,
        title: "Dam".into(),
        prose: prose.into(),
        facts: vec![
            Fact { icon: Icon::MoveArea, label: "Position", value: format!("({}, {})", at.0, at.1) },
            Fact { icon: Icon::Build, label: "Length", value: format!("{} cells", cells.len()) },
        ],
        stats,
        groups: vec![Group { title: "Crest", stats: vec![Stat::new(Explained::reading("Crest height", "", crest_v, "Dam grid").dp(2))] }],
        members: Vec::new(),
        costs: Vec::new(),
        slots,
        actions: Vec::new(),
        at,
    }
}

// ───────────────────────────── tree ─────────────────────────────

fn tree_species(sim: &SimState, r: &Resource) -> &'static str {
    use sim::worldgen::Tree;
    match sim.world.forest.get(r.cell.0, r.cell.1).copied().flatten() {
        Some(Tree::Aspen) => "Aspen",
        Some(Tree::Willow) => "Willow",
        Some(Tree::Birch) => "Birch",
        Some(Tree::Alder) => "Alder",
        Some(Tree::Pine) => "Pine",
        None => "Tree",
    }
}

fn tree_panel(sim: &SimState, id: ResourceId) -> Option<Panel> {
    let r = sim.resources.get(id).filter(|r| r.kind == ResourceKind::Tree)?;
    let species = tree_species(sim, r);
    let done = (1.0 - r.amount / FELL_WORK).clamp(0.0, 1.0);
    let stats = vec![
        Stat::new(Explained::reading("Felling progress", "%", pct(done), "Felling work done on this tree").dp(0)).bar(done),
        Stat::new(Explained::sum(
            "Yield",
            "",
            vec![
                Explained::part("Logs", sim::resources::TREE_LOGS),
                Explained::part("Branches", sim::resources::TREE_BRANCHES),
                Explained::part("Bark", sim::resources::TREE_BARK),
            ],
        )),
    ];
    // The nearest free adult fells it.
    let nearest = sim
        .colony
        .beavers
        .iter()
        .filter(|b| matches!(b.stage, Stage::Adult | Stage::Elder) && sim.work.slot_of(b.id).is_none())
        .min_by(|a, b| dist2(a.pos, r.cell).total_cmp(&dist2(b.pos, r.cell)));
    let action = match nearest {
        Some(b) => PanelAction {
            icon: Icon::Collect,
            label: format!("Fell with {}", b.name),
            command: Some(Command::Assign { who: vec![b.id], to: Some(sim::agents::Assignment::Resource(id)) }),
        },
        None => PanelAction { icon: Icon::Collect, label: "No one to fell it".into(), command: None },
    };
    Some(Panel {
        kind: "Tree",
        icon: Icon::ModeForest,
        title: format!("{species} tree"),
        prose: format!("A standing {} with good timber in it, if someone has the teeth for it.", species.to_lowercase()),
        facts: vec![
            Fact { icon: Icon::ModeForest, label: "Species", value: species.into() },
            Fact { icon: Icon::MoveArea, label: "Position", value: format!("({}, {})", r.cell.0, r.cell.1) },
        ],
        stats,
        groups: Vec::new(),
        members: Vec::new(),
        costs: Vec::new(),
        slots: Vec::new(),
        actions: vec![action],
        at: r.cell,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::explain::audit;
    use sim::starts::{build_lake, Start};

    fn start2() -> SimState {
        let mut sim = build_lake(Start::EarlyLake, 1).expect("start 2").0;
        sim.tech.unlock_all();
        sim
    }

    fn assert_audited(p: &Panel) {
        let bad = audit(p);
        assert!(bad.is_empty(), "{}: {bad:?}", p.title);
    }

    #[test]
    fn beaver_panel_shows_stage_age_family_task_needs_mood_skills_and_an_assign_action() {
        let s = start2();
        let adult = s.colony.beavers.iter().find(|b| b.stage == Stage::Adult).unwrap();
        let p = panel(&s, Selection::Beaver(adult.id)).unwrap();
        assert_eq!(p.title, adult.name);
        assert_eq!(p.fact("Stage"), Some("Adult"));
        assert!(p.fact("Family").unwrap().contains(&family_name(&s, adult.family)));
        assert!(p.fact("Task").is_some());
        assert!((p.stat("Age").unwrap().num.value - adult.age_years()).abs() < 1e-3);
        assert!((p.stat("Mood").unwrap().num.value - adult.mood * 100.0).abs() < 1e-3);
        let needs = p.group("Needs").unwrap();
        assert_eq!(needs.stats.iter().map(|s| s.num.label.as_str()).collect::<Vec<_>>(), ["Food", "Rest", "Warmth", "Safety", "Company"]);
        assert_eq!(p.group("Skills").unwrap().stats.len(), Job::ALL.len());
        assert!(p.actions.iter().any(|a| matches!(a.command, Some(Command::FillSlot { who, .. }) if who == adult.id)), "{:?}", p.actions);
        assert!(!p.prose.is_empty() && !p.prose.contains('\n'));
        assert_audited(&p);
    }

    #[test]
    fn a_kit_cannot_be_assigned() {
        let mut s = start2();
        s.colony.admit_wanderer(true).ok();
        let kit = s.colony.beavers.iter().find(|b| b.stage == Stage::Kit);
        let Some(kit) = kit else { return };
        let p = panel(&s, Selection::Beaver(kit.id)).unwrap();
        assert!(p.actions.iter().all(|a| a.command.is_none()));
    }

    #[test]
    fn family_panel_shows_members_lodge_and_kits() {
        let s = start2();
        let fam = s.colony.families[0].id;
        let p = panel(&s, Selection::Family(fam)).unwrap();
        let folk = s.colony.beavers.iter().filter(|b| b.family == fam).count();
        assert_eq!(p.members.len(), folk);
        assert_eq!(p.stat("Members").unwrap().num.value as usize, folk);
        let kits = s.colony.beavers.iter().filter(|b| b.family == fam && b.stage == Stage::Kit).count();
        assert_eq!(p.stat("Kits").unwrap().num.value as usize, kits);
        assert!(p.fact("Lodge").unwrap().contains("lodge"));
        let lodge = s.colony.lodges.iter().find(|l| l.family == fam).unwrap();
        assert!((p.stat("Lodge condition").unwrap().num.value - lodge.condition * 100.0).abs() < 1e-3);
        assert_audited(&p);
    }

    #[test]
    fn tree_panel_shows_species_felling_progress_and_a_fell_action() {
        let mut s = start2();
        let from = s.colony.beavers[0].pos;
        let tree = s.resources.items.iter().filter(|r| r.kind == ResourceKind::Tree)
            .min_by(|a, b| dist2(from, a.cell).total_cmp(&dist2(from, b.cell))).unwrap().id;
        let p = panel(&s, Selection::Tree(tree)).unwrap();
        assert!(["Aspen", "Willow", "Birch", "Alder", "Pine", "Tree"].contains(&p.fact("Species").unwrap()));
        assert_eq!(p.stat("Felling progress").unwrap().num.value, 0.0);
        let fell = p.actions[0].command.clone().expect("a fell command");
        assert!(matches!(fell, Command::Assign { to: Some(sim::agents::Assignment::Resource(t)), .. } if t == tree));
        s.apply(fell).unwrap();
        for _ in 0..40 {
            s.step(0.25).unwrap();
        }
        let after = panel(&s, Selection::Tree(tree)).map_or(1.0, |p| p.stat("Felling progress").unwrap().num.value);
        assert!(after > 0.0, "the beaver felled some of it: {after}");
        assert_audited(&p);
    }

    fn site_of(s: &mut SimState, kind: BuildingKind) -> BuildingId {
        let near = s.colony.beavers[0].pos;
        let (cx, cy) = (near.0 as usize, near.1 as usize);
        for r in 3..40usize {
            for (x, y) in (cx.saturating_sub(r)..cx + r).flat_map(|x| (cy.saturating_sub(r)..cy + r).map(move |y| (x, y))) {
                if let Ok(id) = s.place_site(kind, (x, y)) {
                    return id;
                }
            }
        }
        panic!("no room for a site");
    }

    #[test]
    fn building_and_site_panels_show_progress_cost_delivered_and_worker_slots() {
        let mut s = start2();
        let site = site_of(&mut s, BuildingKind::Workshop);
        let p = panel(&s, Selection::Building(site)).unwrap();
        assert_eq!(p.kind, "Construction site");
        assert_eq!(p.stat("Progress").unwrap().num.value, 0.0);
        assert_eq!(p.stat("Cost delivered").unwrap().num.value, 0.0);
        assert_eq!(p.slots.len(), sim::work::SITE_SLOTS);
        assert!(p.costs.iter().all(|c| c.delivered == 0.0 && c.needed > 0.0));
        let g = p.costs[0].good;
        s.work.deliver(site, g, 1.0).unwrap();
        let p2 = panel(&s, Selection::Building(site)).unwrap();
        assert!(p2.stat("Progress").unwrap().num.value > 0.0);
        assert!((p2.stat("Cost delivered").unwrap().num.value - 1.0).abs() < 1e-3);
        assert_audited(&p2);

        let done = s.buildings.buildings.iter().find(|b| b.is_complete() && b.kind != BuildingKind::Dam).unwrap().id;
        let q = panel(&s, Selection::Building(done)).unwrap();
        assert_eq!(q.kind, "Building");
        assert_eq!(q.stat("Progress").unwrap().num.value, 100.0);
        assert!(q.stat("Cost delivered").unwrap().num.value > 0.0);
        assert_eq!(q.slots.len(), sim::catalogue::def(s.buildings.building(done).unwrap().kind).slots.len());
        assert_audited(&q);
    }

    #[test]
    fn dam_panel_shows_water_level_current_pressure_and_condition() {
        let s = start2();
        let w = s.height.width();
        let cell = (0..w * w).map(|i| (i % w, i / w)).find(|&(x, y)| s.water.has_dam(x, y)).expect("start 2 has a dam");
        let p = panel(&s, Selection::Dam(cell)).unwrap();
        assert_eq!(p.kind, "Dam");
        assert!(p.stat("Water level").unwrap().num.value > 0.0, "the lake stands against the dam");
        assert!(p.stat("Current pressure").is_some());
        let c = p.stat("Condition").unwrap().num.value;
        assert!(c > 0.0 && c <= 100.0, "{c}");
        assert_audited(&p);
    }

    #[test]
    fn a_gone_entity_has_no_panel() {
        let s = start2();
        assert!(panel(&s, Selection::Beaver(BeaverId(u64::MAX))).is_none());
        assert!(panel(&s, Selection::Family(FamilyId(u64::MAX))).is_none());
        assert!(panel(&s, Selection::Building(BuildingId(u64::MAX))).is_none());
        assert!(panel(&s, Selection::Tree(ResourceId(u64::MAX))).is_none());
    }
}
