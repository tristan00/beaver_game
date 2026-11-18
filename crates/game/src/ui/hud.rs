//! View-models of the always-on HUD zones (`docs/UI_SPEC.md` §1-4): the clock and needs (Z1), the
//! stores ribbon (Z2), the time controls (Z3), the toolbar (Z6), the build menu, the Screens list
//! and the job panel. Pure functions from `SimState` and the shell to plain data, tested without a
//! window; `topbar.rs`, `toolbar.rs`, `buildmenu.rs` and `jobs.rs` draw them. Every number is an
//! [`Explained`] that shows its parts on hover, like the ledger screens.

use sim::buildings::{BuildingKind, Good};
use sim::colony::{BeaverId, FamilyId, Job, Stage};
use sim::SimState;

use super::explain::{Explained, Numbers};
use super::icons::Icon;
use super::screens::{building_icon, colony_kpis, job_icon, nav_bar, time_bar, NavBar, Tone, TimeBar};
use super::shell::{HudTool, Shell};
use crate::build_logic::{cost_text, menu, Group};
use crate::sim_driver::{SimClock, Speed};

// ───────────────────────────── Z1 clock and needs ─────────────────────────────

/// One need meter beside the clock: a headline percentage with its segments.
#[derive(Clone, Debug, PartialEq)]
pub struct NeedMeter {
    pub icon: Icon,
    pub num: Explained,
    pub frac: f32,
    pub tone: Tone,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ClockNeeds {
    pub time: TimeBar,
    pub needs: Vec<NeedMeter>,
}

impl Numbers for ClockNeeds {
    fn numbers(&self) -> Vec<&Explained> {
        std::iter::once(&self.time.day).chain(self.needs.iter().map(|n| &n.num)).collect()
    }
}

/// Season, day and the colony's mood and four needs (the colony screen's own headline numbers).
pub fn clock_needs(sim: &SimState) -> ClockNeeds {
    let needs = colony_kpis(sim)
        .into_iter()
        .skip(1) // head count is on the Colony screen
        .map(|k| NeedMeter { icon: k.icon, frac: k.frac.unwrap_or(0.0), tone: k.tone, num: k.num })
        .collect();
    ClockNeeds { time: time_bar(sim), needs }
}

// ───────────────────────────── Z2 stores ribbon ─────────────────────────────

#[derive(Clone, Debug, PartialEq)]
pub struct StoreItem {
    pub icon: Icon,
    pub num: Explained,
    pub tone: Tone,
}

/// The eight main goods in a row; the rest wait on the Stores screen.
#[derive(Clone, Debug, PartialEq)]
pub struct StoresRibbon {
    pub items: Vec<StoreItem>,
    /// Goods not shown, named on hover.
    pub more: Explained,
}

impl Numbers for StoresRibbon {
    fn numbers(&self) -> Vec<&Explained> {
        self.items.iter().map(|i| &i.num).collect()
    }
}

/// Goods on the ribbon, in order: the larder and woodpile of the colony, then the store goods.
pub const RIBBON_GOODS: [Good; 6] = [Good::Logs, Good::Sticks, Good::Mud, Good::Stone, Good::Baskets, Good::Rope];

pub fn stores_ribbon(sim: &SimState) -> StoresRibbon {
    let (c, s) = (&sim.colony, &sim.buildings.stores);
    let tone = |amount: f32| if amount <= 0.01 { Tone::Bad } else { Tone::Normal };
    let mut items = vec![
        StoreItem { icon: Icon::Food, num: Explained::reading("Food in the larder", "", c.food, "Colony larder"), tone: if c.food_per_capita() < 1.0 { Tone::Bad } else { Tone::Normal } },
        StoreItem { icon: Icon::Logs, num: Explained::reading("Wood in the woodpile", "", c.wood, "Colony woodpile"), tone: tone(c.wood) },
    ];
    for g in RIBBON_GOODS {
        let name = super::screens::good_name(g);
        let made = s.produced_total[g.index()];
        let used = s.consumed_total[g.index()];
        let amount = s.amount(g);
        let num = Explained::sum(&format!("{name} in store"), "", vec![Explained::part("Made so far", made), Explained::part("Used so far", -used), Explained::part("Started with", amount - made + used)]);
        items.push(StoreItem { icon: g.into(), tone: tone(amount), num });
    }
    let left: Vec<Good> = Good::ALL.into_iter().filter(|g| !RIBBON_GOODS.contains(g)).collect();
    let more = Explained::sum_or_zero(
        "Other goods in store",
        "",
        left.iter().map(|g| Explained::part(super::screens::good_name(*g), s.amount(*g))).collect(),
    );
    StoresRibbon { items, more }
}

// ───────────────────────────── Z3 speed ─────────────────────────────

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClockCmd {
    TogglePause,
    Speed(Speed),
}

#[derive(Clone, Debug, PartialEq)]
pub struct SpeedButton {
    pub cmd: ClockCmd,
    pub label: &'static str,
    pub key: &'static str,
    pub active: bool,
}

/// Pause, 1x, 2x, 3x. Exactly one is active: Pause while paused, else the running speed.
pub fn speed_bar(clock: &SimClock) -> Vec<SpeedButton> {
    let mut v = vec![SpeedButton { cmd: ClockCmd::TogglePause, label: "Pause", key: "Space", active: clock.paused }];
    for (s, key) in Speed::ALL.into_iter().zip(["1", "2", "3"]) {
        v.push(SpeedButton { cmd: ClockCmd::Speed(s), label: s.label(), key, active: !clock.paused && clock.speed == s });
    }
    v
}

/// What a clock button does to the clock.
pub fn run_clock(cmd: ClockCmd, clock: &mut SimClock) {
    match cmd {
        ClockCmd::TogglePause => clock.toggle_pause(),
        ClockCmd::Speed(s) => clock.set_speed(s),
    }
}

// ───────────────────────────── Z6 toolbar ─────────────────────────────

#[derive(Clone, Debug, PartialEq)]
pub struct ToolEntry {
    pub tool: HudTool,
    pub active: bool,
    /// Why it does nothing, if it does nothing.
    pub off: Option<&'static str>,
    /// A count that wants attention (decisions waiting on Screens).
    pub badge: Option<Explained>,
}

impl Numbers for Vec<ToolEntry> {
    fn numbers(&self) -> Vec<&Explained> {
        self.iter().filter_map(|e| e.badge.as_ref()).collect()
    }
}

pub fn toolbar(sim: &SimState, shell: &Shell) -> Vec<ToolEntry> {
    let decisions = super::screens::decisions(sim);
    HudTool::ALL
        .into_iter()
        .map(|tool| ToolEntry {
            tool,
            active: shell.tool == Some(tool) || (tool == HudTool::Screens && shell.open.is_some()),
            off: tool.off_reason(),
            badge: if tool == HudTool::Screens { decisions.clone() } else { None },
        })
        .collect()
}

/// The Screens list: the five ledger screens, the open one marked.
pub fn screens_list(sim: &SimState, shell: &Shell) -> NavBar {
    nav_bar(sim, shell.open)
}

// ───────────────────────────── build menu ─────────────────────────────

#[derive(Clone, Debug, PartialEq)]
pub struct BuildTab {
    pub group: Option<Group>,
    pub label: &'static str,
    pub active: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub enum TileState {
    Ready,
    /// The current placement tool.
    Chosen,
    /// Locked behind a tech; the reason is shown on hover.
    Locked(String),
}

#[derive(Clone, Debug, PartialEq)]
pub struct BuildTile {
    pub kind: BuildingKind,
    pub icon: Icon,
    pub name: &'static str,
    pub cost: String,
    pub affordable: bool,
    pub state: TileState,
}

#[derive(Clone, Debug, PartialEq)]
pub struct BuildMenu {
    pub tabs: Vec<BuildTab>,
    pub tiles: Vec<BuildTile>,
}

fn group_of(tab: Option<Group>) -> Group {
    tab.unwrap_or(Group::Base)
}

/// Tabs are the tech branches that hold a building (Basics first); the tiles are the buildings of
/// the open tab, locked ones included.
pub fn build_menu(sim: &SimState, tab: Option<Group>, chosen: Option<BuildingKind>) -> BuildMenu {
    let entries = menu(sim);
    let open = group_of(tab);
    let mut groups: Vec<Group> = Vec::new();
    for e in &entries {
        if !groups.contains(&e.group) {
            groups.push(e.group);
        }
    }
    let tabs = groups.iter().map(|g| BuildTab { group: (*g != Group::Base).then_some(*g), label: g.name(), active: *g == open }).collect();
    let tiles = entries
        .iter()
        .filter(|e| e.group == open)
        .map(|e| {
            let spec = e.kind.spec();
            let cost = spec.cost.to_vec();
            let affordable = cost.iter().all(|&(g, a)| sim.buildings.stores.amount(g) + 1e-4 >= a);
            let state = match e.need {
                Some((tech, need, have)) if !e.unlocked => TileState::Locked(format!("Locked: {tech} needs {need:.0} experience, the colony has {have:.1}")),
                _ if chosen == Some(e.kind) => TileState::Chosen,
                _ => TileState::Ready,
            };
            BuildTile { kind: e.kind, icon: building_icon(e.kind), name: spec.name, cost: cost_text(&cost), affordable, state }
        })
        .collect();
    BuildMenu { tabs, tiles }
}

// ───────────────────────────── Gather menu ─────────────────────────────

/// What the Gather menu offers; each is a real area order (`mapmodes::run_action`).
pub fn gather_actions() -> [super::mapmodes::ModeAction; 2] {
    [super::mapmodes::ModeAction::AssignFellers, super::mapmodes::ModeAction::AssignForagers]
}

// ───────────────────────────── job panel ─────────────────────────────

#[derive(Clone, Debug, PartialEq)]
pub struct JobLine {
    pub job: Job,
    pub icon: Icon,
    pub assigned: Explained,
    pub working: usize,
    pub can_fewer: bool,
    pub can_more: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct BeaverChip {
    pub id: BeaverId,
    pub name: String,
    pub doing: Option<Job>,
    pub idle: bool,
    pub selected: bool,
}

/// The selected adult's own controls: set their job, and their family's priority order.
#[derive(Clone, Debug, PartialEq)]
pub struct SelectedBeaver {
    pub id: BeaverId,
    pub name: String,
    pub set: Option<Job>,
    pub options: Vec<Job>,
    pub family: FamilyId,
    pub family_name: String,
    pub priorities: Vec<Job>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct JobPanel {
    pub adults: Explained,
    pub idle: Explained,
    pub lines: Vec<JobLine>,
    pub chips: Vec<BeaverChip>,
    pub selected: Option<SelectedBeaver>,
}

impl Numbers for JobPanel {
    fn numbers(&self) -> Vec<&Explained> {
        [&self.adults, &self.idle].into_iter().chain(self.lines.iter().map(|l| &l.assigned)).collect()
    }
}

/// One row per open job with its worker count, then a strip of the beavers (idle ones flagged).
pub fn job_panel(sim: &SimState, selected: Option<BeaverId>) -> JobPanel {
    let c = &sim.colony;
    let adults: Vec<_> = c.beavers.iter().filter(|b| b.stage == Stage::Adult).collect();
    let idle: Vec<_> = adults.iter().filter(|b| b.current_job.is_none()).collect();
    let n = adults.len();
    let lines = crate::build_logic::job_rows(c)
        .into_iter()
        .map(|r| {
            let who: Vec<_> = adults.iter().filter(|b| b.job == Some(r.job)).collect();
            JobLine {
                job: r.job,
                icon: job_icon(r.job),
                assigned: Explained::sum_or_zero(&format!("{:?} workers", r.job), "", who.iter().map(|b| Explained::part(b.name.clone(), 1.0)).collect()),
                working: r.working,
                can_fewer: r.assigned > 0,
                can_more: r.assigned < n,
            }
        })
        .collect();
    let chips = adults
        .iter()
        .map(|b| BeaverChip { id: b.id, name: b.name.clone(), doing: b.current_job, idle: b.current_job.is_none(), selected: selected == Some(b.id) })
        .collect();
    let selected = selected.and_then(|id| c.beaver(id)).filter(|b| b.stage == Stage::Adult).map(|b| {
        let fam = c.family(b.family);
        SelectedBeaver {
            id: b.id,
            name: b.name.clone(),
            set: b.job,
            options: c.enabled_jobs(),
            family: b.family,
            family_name: fam.map_or("?".into(), |f| f.name.clone()),
            priorities: fam.map_or(vec![], |f| f.priorities.clone()),
        }
    });
    JobPanel {
        adults: Explained::sum_or_zero("Adults", "", adults.iter().map(|b| Explained::part(b.name.clone(), 1.0)).collect()),
        idle: Explained::sum_or_zero("Idle adults", "", idle.iter().map(|b| Explained::part(b.name.clone(), 1.0)).collect()),
        lines,
        chips,
        selected,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::explain::audit;
    use crate::ui::screens::tests::lived_in;

    #[test]
    fn clock_shows_mood_and_the_four_needs_and_every_number_explains_itself() {
        let sim = lived_in(3);
        let c = clock_needs(&sim);
        let icons: Vec<Icon> = c.needs.iter().map(|n| n.icon).collect();
        assert_eq!(icons, vec![Icon::Mood, Icon::Food, Icon::Rest, Icon::Shelter, Icon::Social]);
        assert!(c.needs.iter().all(|n| (0.0..=1.0).contains(&n.frac)));
        assert_eq!(audit(&c), Vec::<String>::new());
    }

    #[test]
    fn ribbon_has_eight_goods_and_its_figures_are_the_sims_stock() {
        let mut sim = lived_in(3);
        sim.buildings.stores.deposit(Good::Stone, 17.0).unwrap();
        let r = stores_ribbon(&sim);
        assert_eq!(r.items.len(), 8);
        let stone = r.items.iter().find(|i| i.num.label == "Stone in store").unwrap();
        assert!((stone.num.value - sim.buildings.stores.amount(Good::Stone)).abs() < 0.01);
        assert!(stone.num.sums());
        assert_eq!(audit(&r), Vec::<String>::new());
        assert_eq!(r.more.parts.len(), 2, "tools and the food good wait on the Stores screen");
    }

    #[test]
    fn empty_goods_read_as_bad() {
        let mut sim = lived_in(3);
        sim.colony.wood = 0.0;
        let r = stores_ribbon(&sim);
        assert_eq!(r.items[1].tone, Tone::Bad);
    }

    #[test]
    fn speed_bar_has_pause_and_three_speeds_with_one_active() {
        let mut clock = SimClock::default();
        let b = speed_bar(&clock);
        assert_eq!(b.iter().map(|x| x.label).collect::<Vec<_>>(), ["Pause", "1x", "2x", "3x"]);
        assert_eq!(b.iter().filter(|x| x.active).map(|x| x.label).collect::<Vec<_>>(), ["1x"]);
        run_clock(ClockCmd::Speed(Speed::X3), &mut clock);
        assert_eq!(speed_bar(&clock).iter().filter(|x| x.active).map(|x| x.label).collect::<Vec<_>>(), ["3x"]);
        run_clock(ClockCmd::TogglePause, &mut clock);
        assert_eq!(speed_bar(&clock).iter().filter(|x| x.active).map(|x| x.label).collect::<Vec<_>>(), ["Pause"]);
        run_clock(ClockCmd::Speed(Speed::X2), &mut clock);
        assert!(!clock.paused, "choosing a speed resumes");
    }

    #[test]
    fn toolbar_lists_six_tools_marks_the_open_one_and_turns_two_off() {
        let sim = lived_in(3);
        let mut shell = Shell::default();
        shell.toggle_tool(HudTool::Gather);
        let t = toolbar(&sim, &shell);
        assert_eq!(t.iter().map(|e| e.tool.label()).collect::<Vec<_>>(), ["Build", "Gather", "Haul", "Jobs", "Demolish", "Screens"]);
        assert_eq!(t.iter().filter(|e| e.active).map(|e| e.tool).collect::<Vec<_>>(), [HudTool::Gather]);
        assert_eq!(t.iter().filter(|e| e.off.is_some()).map(|e| e.tool).collect::<Vec<_>>(), [HudTool::Haul, HudTool::Demolish]);
        assert_eq!(t.iter().filter_map(|e| e.tool.key()).collect::<String>(), "BGHJX");
    }

    #[test]
    fn open_screen_marks_the_screens_button() {
        let sim = lived_in(3);
        let mut shell = Shell::default();
        shell.toggle(super::super::shell::NavItem::Stores);
        assert!(toolbar(&sim, &shell).iter().find(|e| e.tool == HudTool::Screens).unwrap().active);
        assert_eq!(screens_list(&sim, &shell).entries.len(), 5);
    }

    #[test]
    fn build_menu_has_basics_first_and_a_tile_per_building_of_the_open_tab() {
        let sim = lived_in(3);
        let m = build_menu(&sim, None, None);
        assert!(m.tabs[0].active && m.tabs[0].label == "Basics" && m.tabs[0].group.is_none());
        assert_eq!(m.tabs.iter().filter(|t| t.active).count(), 1);
        assert!(!m.tiles.is_empty());
        let total: usize = m.tabs.iter().map(|t| build_menu(&sim, t.group, None).tiles.len()).sum();
        assert_eq!(total, menu(&sim).len(), "every building is on exactly one tab");
    }

    #[test]
    fn locked_tiles_say_why_and_the_chosen_tool_is_marked() {
        let mut sim = sim::SimState::new(1);
        let mut flow = crate::flow::Flow::new(&sim.world);
        crate::opening::found(&mut sim, &mut flow).expect("founds");
        let all: Vec<BuildTile> = menu(&sim).iter().flat_map(|e| build_menu(&sim, (e.group != Group::Base).then_some(e.group), None).tiles).collect();
        let locked = all.iter().find(|t| matches!(t.state, TileState::Locked(_))).expect("a fresh colony has locked buildings");
        let TileState::Locked(why) = &locked.state else { unreachable!() };
        assert!(why.starts_with("Locked:") && why.contains("experience"), "{why}");
        let ready = all.iter().find(|t| t.state == TileState::Ready).unwrap();
        let again = build_menu(&sim, menu(&sim).iter().find(|e| e.kind == ready.kind).map(|e| e.group).filter(|g| *g != Group::Base), Some(ready.kind));
        assert_eq!(again.tiles.iter().find(|t| t.kind == ready.kind).unwrap().state, TileState::Chosen);
    }

    #[test]
    fn job_panel_counts_match_the_colony_and_flag_idle_adults() {
        let sim = lived_in(3);
        let p = job_panel(&sim, None);
        let adults = sim.colony.beavers.iter().filter(|b| b.stage == Stage::Adult).count();
        assert_eq!(p.adults.value as usize, adults);
        assert_eq!(p.chips.len(), adults);
        assert_eq!(p.chips.iter().filter(|c| c.idle).count(), p.idle.value as usize);
        assert_eq!(p.lines.len(), sim.colony.enabled_jobs().len());
        for l in &p.lines {
            let want = sim.colony.beavers.iter().filter(|b| b.stage == Stage::Adult && b.job == Some(l.job)).count();
            assert_eq!(l.assigned.value as usize, want, "{:?}", l.job);
        }
        assert_eq!(audit(&p), Vec::<String>::new());
    }

    #[test]
    fn job_panel_selected_beaver_gets_job_buttons_and_family_priorities() {
        let sim = lived_in(3);
        let id = sim.colony.beavers.iter().find(|b| b.stage == Stage::Adult).unwrap().id;
        let p = job_panel(&sim, Some(id));
        let s = p.selected.expect("adult is selected");
        assert_eq!(s.id, id);
        assert_eq!(s.options, sim.colony.enabled_jobs());
        assert!(!s.priorities.is_empty());
        assert!(p.chips.iter().any(|c| c.id == id && c.selected));
        let kit = sim.colony.beavers.iter().find(|b| b.stage != Stage::Adult).map(|b| b.id);
        if let Some(k) = kit {
            assert!(job_panel(&sim, Some(k)).selected.is_none(), "kits take no jobs");
        }
    }

    #[test]
    fn staffing_through_the_panel_changes_the_next_view() {
        let mut sim = lived_in(3);
        let before = job_panel(&sim, None).lines.iter().find(|l| l.job == Job::Logger).unwrap().assigned.value;
        crate::build_logic::set_job_count(&mut sim.colony, Job::Logger, before as usize + 1).unwrap();
        let after = job_panel(&sim, None).lines.iter().find(|l| l.job == Job::Logger).unwrap().assigned.value;
        assert!(after > before || !job_panel(&sim, None).lines.iter().find(|l| l.job == Job::Logger).unwrap().can_more);
    }
}
