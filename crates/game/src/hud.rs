#![allow(deprecated)]
mod actions;
mod input;
mod minimap;
mod colony;
mod canal_panel;
mod dam_panel;
mod elevation;
mod inspect;
mod journal;
mod life;
mod kin;
mod family;
mod ledger;
mod panels;
mod paths;
mod water_legend;
mod welcome;
mod ui;
pub(crate) mod words;
pub(crate) use words::job_icon;
mod works;
mod world;

use actions::{button_actions, commit_line, undo_last};
use input::{pointer_over_ui, track_cursor, hover_cell, hotkeys, next_idle, world_input, box_select, frame_camera, follow_beaver};
use minimap::{MinimapImage, setup_minimap, update_minimap, minimap_click};

use crate::build_logic::{self, BuildOrder};
use crate::camera::MainCamera;
use crate::flow::{cell_world, Cell, Flow, Selection};
use crate::icons::{Icon, Icons};
use crate::sim_driver::{SimClock, Speed};
use crate::state::Sim;
use bevy::prelude::*;
use bevy::ui::FocusPolicy;
use sim::buildings::{BuildingId, BuildingKind};
use sim::colony::BeaverId;
use sim::dam_preview::DamPreview;
use sim::hud::Fix;
use sim::jobs::Job;
use ui::{Mounted, Tag, TagComp, Tip, TipLine, Tips};
use words::*;

pub(crate) use ui::{font, ACCENT, BAD, FAINT, GOOD, INK, MUTED, PANEL, BARK, WARN, WATER};

pub(crate) fn text(p: &mut ChildSpawnerCommands, s: impl Into<String>, size: f32, color: Color) {
    p.spawn((Text::new(s.into()), font(size, false), TextColor(color)));
}

pub(crate) fn bold(p: &mut ChildSpawnerCommands, s: impl Into<String>, size: f32, color: Color) {
    p.spawn((Text::new(s.into()), font(size, true), TextColor(color)));
}

pub(crate) fn row(gap: f32) -> Node {
    ui::row_node(gap)
}

/// Hover text for a plain UI node, such as a stat in the start menu.
pub(crate) fn hover_tip(label: impl Into<String>) -> (Interaction, ui::TipComp) {
    (Interaction::default(), ui::TipComp { tip: Tip::name(label), depth: 0 })
}

const REFRESH_SECS: f32 = 0.1;
/// How long a dam or canal's Remove stays armed after the first click.
pub(crate) const REMOVE_ARM_SECS: f32 = 3.0;
const MSG_SECS: f32 = 3.0;

#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub enum Tool {
    #[default]
    None,
    Building(BuildingKind),
    Dam,
    /// Draw a canal: from water, over land.
    Canal,
    /// Send beavers to gather (food or mud) or cut trees: click a spot or drag an area.
    Harvest(Harvest),
}

impl Tool {
    /// The tool a kind of building is placed with: a dam or canal is drawn as a line.
    pub fn of(kind: BuildingKind) -> Tool {
        if kind.is_dam() { Tool::Dam } else if kind.is_canal() { Tool::Canal } else { Tool::Building(kind) }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Harvest {
    Gather,
    Cut,
}

/// What is being placed on a dam from its panel: its cells light up on the map, a click on
/// one places it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Opening {
    Gate,
    Lock,
}

/// The dam panel's two pages.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum DamTab {
    #[default]
    Status,
    History,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// The colony's pages (Tab): everything about the beavers.
pub enum ColonyTab {
    Population,
    Jobs,
    Beavers,
    Run,
}

/// The territory's pages (L): everything about the land and the works.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LedgerTab {
    Charts,
    Dams,
    Buildings,
    Stores,
}

/// How far back the ledger's charts look.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Span {
    Season,
    Year,
    #[default]
    All,
}

/// A column of the ledger's beaver table, to sort by.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum BeaverCol {
    #[default]
    Family,
    Name,
    Age,
    Job,
    /// What it is doing this moment.
    Doing,
    Skill,
    Pace,
}

/// A full page: a beaver's story or a lodge's family page.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Page {
    Beaver(BeaverId),
    Lodge(sim::colony::LodgeId),
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct Msg {
    pub icon: Option<(Icon, Color)>,
    pub icon2: Option<(Icon, Color)>,
    pub text: String,
}

pub use crate::build_logic::LinePlan;
pub(crate) use world::{HoverPoint, HoverThing};

#[derive(Resource, Default)]
pub struct HudState {
    /// The top-centre badge opened by a click, showing its words.
    pub open_badge: Option<String>,
    pub tool: Tool,
    pub drag_from: Option<Cell>,
    /// A dam end pressed on: dragging from it extends that dam.
    pub grab: Option<(BuildingId, Cell)>,
    /// Corners of the dam line being drawn; `line` is the cells through them.
    pub corners: Vec<Cell>,
    pub line: Vec<Cell>,
    /// What the drawn line would build, worked out once per change of line.
    pub plan: Option<LinePlan>,
    /// What the water engine itself makes of the drawn line, once its background run finishes.
    pub settled: Option<(Vec<Cell>, DamLook)>,
    /// What the drawn canal would dig, worked out once per change of line.
    pub canal_plan: Option<(Vec<Cell>, Result<sim::canals::Plan, String>)>,
    /// Set while a dam is being lengthened from one of its ends.
    pub extending: Option<BuildingId>,
    /// Cells picked in the dam panel's cross-section; raises apply to them only.
    pub dam_cells: Vec<Cell>,
    pub dam_tab: DamTab,
    /// A gate or lock being placed on this dam (see `Opening`).
    pub opening: Option<(BuildingId, Opening)>,
    /// The gate or lock whose settings its dam's or canal's panel shows.
    pub gate_sel: Option<(BuildingId, Cell)>,
    /// A dam or canal whose Remove was clicked once, and when: a second click removes it.
    pub remove_armed: Option<(BuildingId, f32)>,
    /// The selected dam's suggested growth, ghosted on the map: (dam, day, cells).
    pub dam_ghost: Option<(BuildingId, f64, Vec<Cell>)>,
    pub msg: Option<(Msg, f32)>,
    idle_cursor: usize,
    pub colony: Option<ColonyTab>,
    /// The beaver whose whole story is open in the drawer.
    pub details: Option<BeaverId>,
    /// The lodge whose family page is open.
    pub lodge_page: Option<sim::colony::LodgeId>,
    /// Which ties the family table on a beaver's page shows.
    pub kin: kin::KinFilter,
    /// The pages gone through to reach the open one, for Back (and Esc).
    pub back: Vec<Page>,
    /// The colony ledger's open page, if it is open.
    pub ledger: Option<LedgerTab>,
    pub span: Span,
    /// The beaver table's sort column, and whether it runs high to low.
    pub sort: (BeaverCol, bool),
    /// Rows scrolled past at the top of the ledger's long lists.
    pub scroll: usize,
    /// A beaver whose inspector shows while the pointer is on it in the book or ledger.
    pub peek: Option<BeaverId>,
    /// The journal, bottom right: a few lines, or many.
    pub journal_open: bool,
    /// The one guidance card, shown when a new colony settles until it is closed.
    pub welcome: bool,
    /// The buildings the drawn dam would drown, worked out for this line from the engine's answer.
    pub drowned: Option<(Vec<Cell>, Vec<BuildingId>)>,
    undo: Vec<UndoItem>,
    pub selected: Vec<BeaverId>,
    pub follow: Option<BeaverId>,
    // Compatibility with the current menu, renderer and input recorder. This
    // HUD uses contextual orders and has no separate order or help screen.
    consume_left_release: bool,
    pub pinned: Vec<Selection>,
    box_from: Option<Vec2>,
    box_done: Option<(Vec2, Vec2)>,
    frame_req: Option<Vec3>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum UndoItem {
    Site(BuildingId),
}

#[derive(Resource, Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum MapLens {
    #[default]
    World,
    Water,
    Forest,
    Elevation,
    Deposits,
    /// Each beaver's way still to walk, over the plain ground.
    Paths,
}

impl MapLens {
    pub fn code(self) -> u8 {
        match self {
            MapLens::World => 0,
            MapLens::Water => 1,
            MapLens::Forest => 3,
            MapLens::Elevation => 4,
            MapLens::Deposits => crate::terrain_render::DEPOSIT_LENS,
            MapLens::Paths => 0,
        }
    }
}

#[derive(Resource, Default)]
pub struct TextCapture(pub bool);

#[derive(Resource, Default)]
pub struct MenuOpen(pub bool);

pub fn reset_for_new_game(world: &mut World) {
    world.insert_resource(HudState::default());
    world.insert_resource(MapLens::default());
    if let Some(mut n) = world.get_resource_mut::<Notices>() {
        n.0.clear();
    }
    if let Some(mut s) = world.get_resource_mut::<Selection>() {
        *s = Selection::None;
    }
}


impl HudState {
    pub(crate) fn swallow_click(&mut self) { self.consume_left_release = true; }
    fn set_tool(&mut self, t: Tool) {
        self.tool = t;
        self.drag_from = None;
        self.line.clear();
        self.corners.clear();
        self.plan = None;
        self.canal_plan = None;
        self.extending = None;
    }
    fn finish_placement(&mut self) {
        self.set_tool(Tool::None);
    }
    /// The full page open now, if any.
    pub fn page(&self) -> Option<Page> {
        self.details.map(Page::Beaver).or(self.lodge_page.map(Page::Lodge))
    }
    fn show_page(&mut self, p: Option<Page>) {
        if let Some(Page::Beaver(id)) = p {
            if self.details != Some(id) {
                self.kin = kin::KinFilter::All;
            }
        }
        self.details = match p { Some(Page::Beaver(id)) => Some(id), _ => None };
        self.lodge_page = match p { Some(Page::Lodge(l)) => Some(l), _ => None };
        self.scroll = 0;
        if p.is_some() {
            self.ledger = None;
        }
    }
    /// Open a full page; the one it was opened from is remembered for Back.
    pub(crate) fn open_page(&mut self, p: Page) {
        if let Some(now) = self.page().filter(|now| *now != p) {
            self.back.push(now);
            if self.back.len() > 16 {
                self.back.remove(0);
            }
        }
        self.show_page(Some(p));
    }
    /// Back to the page before, or closed when there was none.
    pub(crate) fn page_back(&mut self) {
        let p = self.back.pop();
        self.show_page(p);
    }
    pub(crate) fn close_pages(&mut self) {
        self.back.clear();
        self.show_page(None);
    }
    /// Open the colony book or the ledger: they replace any full page.
    pub(crate) fn open_book(&mut self, colony: Option<ColonyTab>, ledger: Option<LedgerTab>) {
        self.close_pages();
        if ledger != self.ledger || colony != self.colony {
            self.scroll = 0;
        }
        self.colony = colony;
        self.ledger = ledger;
    }

    fn say(&mut self, icon: Option<(Icon, Color)>, icon2: Option<(Icon, Color)>, text: impl Into<String>, now: f32) {
        self.msg = Some((Msg { icon, icon2, text: text.into() }, now));
    }
    fn refuse(&mut self, why: impl Into<String>, now: f32) {
        self.say(Some((Icon::Cross, BAD)), None, why, now);
    }
    pub fn dam_preview(&self) -> Option<&DamPreview> {
        self.plan.as_ref().and_then(|p| p.preview.as_ref())
    }
    /// The water engine's answer for the line being drawn, once it is in.
    pub fn dam_settled(&self) -> Option<&DamLook> {
        let plan = self.plan.as_ref()?;
        self.settled.as_ref().filter(|(line, _)| *line == plan.line).map(|(_, s)| s)
    }
    pub fn set_tool_for_report(&mut self, t: Tool) {
        if self.tool != t {
            self.set_tool(t);
        }
    }
    /// Put a drawn dam line on screen for a report: corners, and whether a run is still being dragged.
    pub fn draw_for_report(&mut self, corners: Vec<Cell>, dragging: bool) {
        self.set_tool(Tool::Dam);
        self.line = build_logic::polyline(&corners);
        self.drag_from = dragging.then(|| corners[0]);
        self.corners = if dragging { corners[..1].to_vec() } else { corners };
    }
}

#[derive(Resource, Default)]
pub struct HoverCell(pub Option<Cell>);

/// Short-lived words and icons on the map where something just happened.
#[derive(Clone, Debug)]
pub struct Pop {
    pub at: PopAt,
    pub icons: Vec<(Icon, Color)>,
    pub text: String,
    pub born: f32,
}

#[derive(Clone, Copy, Debug)]
pub enum PopAt {
    Screen(Vec2),
    Cell((f32, f32)),
}

#[derive(Resource, Default)]
pub struct Pops(pub Vec<Pop>);

pub const POP_SECS: f32 = 2.4;

/// What happened, for the journal at the bottom right: births and deaths, buildings finished,
/// a wolf seen, a game saved. Problems that want the player are the badges at the top.
#[derive(Clone, Debug)]
pub struct Notice {
    pub icons: Vec<(Icon, Color)>,
    pub text: String,
    pub born: f32,
    /// The game day it happened.
    pub day: f64,
    /// Where a click on it takes the camera.
    pub focus: Option<NoticeAt>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum NoticeAt {
    Beaver(BeaverId),
    Building(BuildingId),
    Spot((f32, f32)),
    /// A wolf sighting: to the wolf while it is seen, else to where it was last seen, else
    /// where this sighting was.
    Wolf((f32, f32)),
}

/// The journal's lines, and today's game day (stamped on new lines).
#[derive(Resource, Default)]
pub struct Notices(pub Vec<Notice>, pub f64);

/// Buildings that have gone under water so far, for the sound.
#[derive(Resource, Default)]
pub struct Floods(pub u32);

/// Wolves sighted and beavers killed by them so far, for their sounds.
#[derive(Resource, Default)]
pub struct WolfCues {
    pub sighted: u32,
    pub kills: u32,
}

/// How long a new line stands out in the journal, and how many lines it keeps.
pub const NOTICE_SECS: f32 = 7.0;
pub const JOURNAL_KEPT: usize = 200;

impl Notices {
    pub fn push(&mut self, icons: Vec<(Icon, Color)>, text: impl Into<String>, now: f32) {
        self.push_at(icons, text, now, None);
    }
    pub fn push_at(&mut self, icons: Vec<(Icon, Color)>, text: impl Into<String>, now: f32, focus: Option<NoticeAt>) {
        let text = text.into();
        sim::trace::event(format!("NOTICE icons={:?} text={text:?}", icons.iter().map(|(i, _)| format!("{i:?}")).collect::<Vec<_>>()));
        let day = self.1;
        self.0.push(Notice { icons, text, born: now, day, focus });
        if self.0.len() > JOURNAL_KEPT {
            self.0.remove(0);
        }
    }
}

/// A beaver (or a lookout's sentry) spots a wolf: say so once for that wolf, however often it
/// slips in and out of sight while it is about.
fn notice_wolf(time: Res<Time>, sim: Res<Sim>, mut notices: ResMut<Notices>, mut cues: ResMut<WolfCues>, mut told: Local<Vec<(u64, f64)>>) {
    let s = &sim.0;
    if notices.1 != s.time_days {
        notices.1 = s.time_days;
    }
    let now = s.time_days;
    told.retain(|(k, at)| s.predators.wolf(*k).is_some() && now >= *at);
    for w in sim::predators::seen(s) {
        if !told.iter().any(|(k, _)| *k == w.key()) {
            told.push((w.key(), now));
            notices.push_at(vec![(Icon::Wolf, BAD)], "Wolf sighted", time.elapsed_secs(), Some(NoticeAt::Wolf(w.pos)));
            cues.sighted += 1;
        }
    }
}

type BuildingsSeen = std::collections::BTreeMap<BuildingId, (BuildingKind, bool, (usize, usize))>;

/// Buildings finished, and finished ones lost, into the journal.
fn notice_buildings(time: Res<Time>, sim: Res<Sim>, mut notices: ResMut<Notices>, mut was: Local<Option<(u64, BuildingsSeen)>>) {
    let s = &sim.0;
    if !s.is_running() {
        *was = None;
        return;
    }
    let changed = was.as_ref().is_none_or(|(i, before)| *i != s.instance || before.len() != s.buildings.buildings.len() || s.buildings.buildings.iter().any(|b| before.get(&b.id).is_none_or(|x| x.1 != b.is_complete())));
    if !changed {
        return;
    }
    let now: BuildingsSeen = s.buildings.buildings.iter().map(|b| (b.id, (b.kind, b.is_complete(), b.pos))).collect();
    if let Some((_, before)) = was.as_ref().filter(|(i, _)| *i == s.instance) {
        let t = time.elapsed_secs();
        for (id, (kind, done, _)) in &now {
            if *done && before.get(id).is_some_and(|b| !b.1) {
                notices.push_at(vec![(building_icon(*kind), INK), (Icon::Check, GOOD)], format!("{} built", kind.spec().name.as_str()), t, Some(NoticeAt::Building(*id)));
            }
        }
        for (id, (kind, done, pos)) in before {
            if *done && !now.contains_key(id) {
                let how = if s.buildings.wrecked.contains(id) { "fell apart" } else { "lost" };
                notices.push_at(vec![(building_icon(*kind), INK), (Icon::Cross, BAD)], format!("{} {how}", kind.spec().name.as_str()), t, Some(NoticeAt::Spot((pos.0 as f32, pos.1 as f32))));
            }
        }
    }
    *was = Some((s.instance, now));
}

/// A building going under water, into the journal: once, and not again for a few days while
/// the water hovers about the limit.
fn notice_floods(time: Res<Time>, sim: Res<Sim>, mut notices: ResMut<Notices>, mut floods: ResMut<Floods>, mut was: Local<(u64, f64, std::collections::BTreeMap<BuildingId, (bool, f64)>)>) {
    const AGAIN_DAYS: f64 = 3.0;
    let s = &sim.0;
    if !s.is_running() || (was.0 == s.instance && was.1 == s.time_days) {
        return;
    }
    let fresh = was.0 != s.instance;
    *was = (s.instance, s.time_days, if fresh { Default::default() } else { std::mem::take(&mut was.2) });
    for b in &s.buildings.buildings {
        let flooded = sim::buildings::flooded(b, &s.water.depth);
        let (before, told) = was.2.get(&b.id).copied().unwrap_or((flooded || fresh, f64::NEG_INFINITY));
        let mut told_at = told;
        if flooded && !before && s.time_days - told >= AGAIN_DAYS {
            notices.push_at(vec![(building_icon(b.kind), INK), (Icon::Flood, BAD)], format!("{} flooded", b.kind.spec().name.as_str()), time.elapsed_secs(), Some(NoticeAt::Building(b.id)));
            told_at = s.time_days;
            floods.0 = floods.0.wrapping_add(1);
        }
        was.2.insert(b.id, (flooded, told_at));
    }
    let standing: std::collections::BTreeSet<BuildingId> = s.buildings.buildings.iter().map(|b| b.id).collect();
    was.2.retain(|id, _| standing.contains(id));
}

/// The pointer on a beaver in the colony book or ledger shows its inspector.
fn peek_hover(q: Query<(&Interaction, &ui::Peek)>, mut hud: ResMut<HudState>) {
    let on = q.iter().find(|(i, _)| **i != Interaction::None).map(|(_, p)| p.0);
    if hud.peek != on {
        hud.peek = on;
    }
}

/// How far the open page's long list can scroll: the beaver's story, the family tree, or the
/// ledger page's own list.
pub(crate) fn scroll_max(s: &sim::SimState, hud: &HudState) -> usize {
    match (hud.details, hud.lodge_page, hud.ledger) {
        (Some(id), _, _) => s.lives.of.get(&id).map_or(0, |l| l.entries.len().saturating_sub(life::ROWS)),
        (None, Some(l), _) => family::tree_len(s, l),
        (None, None, Some(tab)) => ledger::scroll_max(s, tab),
        (None, None, None) if hud.colony == Some(ColonyTab::Beavers) => ledger::beavers_scroll_max(s),
        _ => 0,
    }
}

/// The mouse wheel over a long list in the ledger scrolls it.
fn scroll_lists(scroll: Res<bevy::input::mouse::AccumulatedMouseScroll>, q: Query<(&ui::TagComp, &bevy::ui::RelativeCursorPosition)>, sim: Res<Sim>, mut hud: ResMut<HudState>) {
    if scroll.delta.y == 0.0 || (hud.ledger.is_none() && hud.details.is_none() && hud.lodge_page.is_none() && hud.colony != Some(ColonyTab::Beavers)) {
        return;
    }
    let over = q.iter().any(|(t, r)| t.0 == Tag::Scroll && r.normalized.is_some_and(|p| p.x.abs() <= 0.5 && p.y.abs() <= 0.5));
    if !over {
        return;
    }
    let lines = match scroll.unit {
        bevy::input::mouse::MouseScrollUnit::Line => scroll.delta.y,
        bevy::input::mouse::MouseScrollUnit::Pixel => scroll.delta.y / 20.0,
    };
    let step = (-lines * 2.0).round() as i64;
    let longest = scroll_max(&sim.0, &hud);
    let next = ((hud.scroll as i64 + step).max(0) as usize).min(longest);
    if next != hud.scroll {
        hud.scroll = next;
    }
}

/// The problems found at the last panel refresh, for the markers drawn every frame.
#[derive(Resource, Default)]
pub struct LastProblems(pub Vec<sim::hud::Problem>);

#[derive(Default)]
pub struct PopWatch {
    ready: bool,
    beavers: std::collections::HashSet<u64>,
    worn: std::collections::HashSet<Cell>,
}

/// Turns messages and colony events into pops: refusals at the cursor, a kit
/// over its home, a splash where a dam cell wears through.
fn collect_pops(time: Res<Time>, cursor: Res<CursorUi>, mut hud: ResMut<HudState>, sim: Res<Sim>, mut pops: ResMut<Pops>, mut w: Local<PopWatch>) {
    let _p = ::sim::prof::scope("game/collect pops");
    let now = time.elapsed_secs();
    pops.0.retain(|p| now - p.born < POP_SECS);
    let before = pops.0.len();
    collect_new_pops(now, &cursor, &mut hud, &sim, &mut pops, &mut w);
    for p in &pops.0[before.min(pops.0.len())..] {
        let icons: Vec<_> = p.icons.iter().map(|(i, _)| format!("{i:?}")).collect();
        sim::trace::event(format!("POP day={:.4} icons={icons:?} text={:?} at={:?}", sim.0.time_days, p.text, match p.at { PopAt::Screen(v) => format!("screen({:.0},{:.0})", v.x, v.y), PopAt::Cell(c) => format!("cell({:.1},{:.1})", c.0, c.1) }));
    }
}

fn collect_new_pops(now: f32, cursor: &CursorUi, hud: &mut HudState, sim: &Sim, pops: &mut Pops, w: &mut PopWatch) {
    if hud.msg.is_some() {
        if let Some((m, _)) = hud.msg.take() {
            let at = cursor.0.map(|c| c + Vec2::new(0.0, -28.0)).unwrap_or(Vec2::new(640.0, 300.0));
            let icons = [m.icon, m.icon2].into_iter().flatten().collect();
            pops.0.push(Pop { at: PopAt::Screen(at), icons, text: m.text, born: now });
        }
    }
    let s = &sim.0;
    if !s.is_running() {
        w.ready = false;
        return;
    }
    let ids: std::collections::HashSet<u64> = s.colony.beavers.iter().map(|b| b.id.0).collect();
    let worn: std::collections::HashSet<Cell> = s
        .buildings
        .buildings
        .iter()
        .filter_map(|b| b.dam.as_deref())
        .flat_map(|d| d.cells.iter().map(|c| c.cell))
        .filter(|&c| sim::dams::built(s, c) > 0.0 && sim::dams::integrity(s, c) < sim::dams::REPAIR_BELOW)
        .collect();
    if w.ready {
        for b in s.colony.beavers.iter().filter(|b| !w.beavers.contains(&b.id.0)) {
            let at = s.colony.lodges.iter().find(|l| Some(l.id) == b.lodge).map(|l| (l.pos.0 + 0.5, l.pos.1 + 0.5)).unwrap_or(b.pos);
            let icon = if b.stage == sim::colony::Stage::Kit { Icon::Kit } else { Icon::Adult };
            pops.0.push(Pop { at: PopAt::Cell(at), icons: vec![(icon, GOOD)], text: "+1".into(), born: now });
        }
        for c in worn.difference(&w.worn) {
            pops.0.push(Pop { at: PopAt::Cell((c.0 as f32 + 0.5, c.1 as f32 + 0.5)), icons: vec![(Icon::Drop, WATER), (Icon::Warn, BAD)], text: String::new(), born: now });
        }
    }
    w.beavers = ids;
    w.worn = worn;
    w.ready = true;
}

#[derive(Resource, Default)]
pub struct PointerOverUi(pub bool);

/// The pointer is on a pin over the map (see `ui::VNode::pin`).
#[derive(Resource, Default)]
pub struct PointerOverPin(pub bool);

#[derive(Resource, Default)]
struct CursorUi(Option<Vec2>);

#[derive(Resource)]
struct RefreshTimer(Timer);

#[derive(Component)]
pub struct Blocks;

#[derive(Component, Clone, Copy, PartialEq, Eq, Hash, Debug)]
enum Panel {
    Strip,
    Calendar,
    Right,
    Rail,
    Bottom,
    Minimap,
    Colony,
    Journal,
    Cursor,
    Markers,
    Welcome,
}

const SLOW_PANELS: [Panel; 10] = [Panel::Markers, Panel::Strip, Panel::Calendar, Panel::Rail, Panel::Minimap, Panel::Bottom, Panel::Colony, Panel::Journal, Panel::Right, Panel::Welcome];

#[derive(Component, Clone, Debug, PartialEq)]
pub enum HudAction {
    Fix(Fix, Option<Cell>),
    /// Open (or, with `None`, close) a beaver's story.
    BeaverDetails(Option<BeaverId>),
    /// Open (or close) a lodge's family page.
    LodgePage(Option<sim::colony::LodgeId>),
    /// Which ties the family table shows.
    Kin(kin::KinFilter),
    /// To the wolf while it is seen, else to where it was last seen.
    FocusWolf,
    CycleIdle,
    Pause,
    Speed(Speed),
    Pick(BuildingKind),
    CancelTool,
    GrowDam(BuildingId, sim::DamGrowth),
    DamUpkeep(BuildingId, bool),
    /// Start placing a gate or lock on a dam, or stop if it is already being placed.
    Opening(BuildingId, Opening),
    /// Show this gate's settings in its panel, and take the camera to it.
    GateChip(BuildingId, Cell),
    /// Take a gate (and its lock) off its dam or canal.
    RemoveGate(BuildingId, Cell),
    SetGate(BuildingId, Cell, sim::GateSetting),
    UpgradeLock(BuildingId, Cell),
    GrowCanal(BuildingId, sim::CanalGrowth),
    CanalUpkeep(BuildingId, bool),
    DamCell(Cell),
    DamTab(DamTab),
    ConfirmLine,
    SetLens(MapLens),
    /// Show only this role's paths (`None`: free beavers'), or every path again if it is the
    /// one shown already.
    PathsOnly(Option<Job>),
    Colony(Option<ColonyTab>),
    Ledger(Option<LedgerTab>),
    Span(Span),
    Sort(BeaverCol),
    /// Scroll the ledger's long list by rows.
    Scroll(i32),
    /// Show many lines of the journal, or few.
    Journal(bool),
    /// Take the camera to where a journal line happened.
    GoTo(NoticeAt),
    SelectBeaver(BeaverId),
    SelectBuilding(BuildingId),
    Follow(BeaverId),
    FreeBeaver(BeaverId),
    AddBeaver(BuildingId),
    /// Open or close a top-centre badge to read it.
    Badge(String),
    /// Have haulers fill this store to the brim, or stop.
    FillStore(BuildingId, bool),
    /// A haul mound's haulers balance the stores, or stop.
    Balance(BuildingId, bool),
    /// Cancel a building site, or take a dam or canal away altogether.
    CancelBuild(BuildingId),
    RemoveBeaver(BuildingId),
    RoleAdjust(Job, i32),
    /// Keep a role's beavers within this many cells of water (None: anywhere).
    ShoreLimit(Job, Option<u16>),
    Paused(BuildingId, bool),
    RepairBuilding(BuildingId),
    ReinforceBuilding(BuildingId),
    PebbleBuilding(BuildingId),
    SlotOff(BuildingId, usize, bool),
    CancelMaintenance(BuildingId),
    Pin(Selection),
    /// Drop every cell picked on a dam or canal.
    ClearPicks,
    /// Back to the full page this one was opened from.
    PageBack,
    /// Close the guidance card shown when the colony settled.
    CloseWelcome,
    /// Close a dam or canal panel: unpin it, and deselect it too if it is the selection.
    ClosePanel(Selection, bool),
    /// Take a dam or canal away: the first click arms it, a second within a few seconds acts.
    RemoveWorks(BuildingId),
}

pub struct HudPlugin;

#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct HudSet;

/// Swap Bevy's monospace default for a proportional face, when the system has one.
fn load_ui_font(mut fonts: ResMut<Assets<Font>>) {
    let candidates = ["C:/Windows/Fonts/bahnschrift.ttf", "/System/Library/Fonts/Supplemental/Futura.ttc", "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf"];
    for path in candidates {
        if let Ok(bytes) = std::fs::read(path) {
            let _ = fonts.insert(Handle::<Font>::default().id(), Font::from_bytes(bytes));
            return;
        }
    }
}

impl Plugin for HudPlugin {
    fn build(&self, app: &mut App) {
        // Read by the colony's flow too, so they exist even when the HUD is not drawn.
        app.add_systems(Startup, load_ui_font).init_resource::<Notices>().init_resource::<WolfCues>();
        if app.world().resource::<crate::cli::Cli>().screenshot.is_some() && std::env::var_os("BC_HUD").is_none() {
            return;
        }
        paths::setup(app);
        app.init_resource::<HudState>()
            .init_resource::<HoverCell>()
            .init_resource::<PointerOverUi>()
            .init_resource::<PointerOverPin>()
            .init_resource::<CursorUi>()
            .init_resource::<MapLens>()
            .init_resource::<TextCapture>()
            .init_resource::<MenuOpen>()
            .init_resource::<Tips>()
            .init_resource::<Pops>()
            .init_resource::<Floods>()
            .add_systems(Update, (notice_wolf, notice_buildings, notice_floods))
            .init_resource::<LastProblems>()
            .init_resource::<world::HoverThing>()
            .init_resource::<world::HoverPoint>()
            .init_resource::<world::WorksSnap>()
            .init_resource::<elevation::Contours>()
            .init_resource::<DamSettle>()
            .insert_resource(RefreshTimer(Timer::from_seconds(REFRESH_SECS, TimerMode::Repeating)))
            .add_systems(Startup, (spawn_hosts, ui::spawn_tip_hosts, setup_minimap))
            .add_systems(
                Update,
                (
                    (pointer_over_ui, track_cursor, world::world_hover, hover_cell, hotkeys, world_input, box_select, frame_camera, follow_beaver, peek_hover, scroll_lists),
                    (button_actions, collect_pops, minimap_click, sync_lens, update_minimap, refresh_panels, refresh_fast, ui::tip_logic, ui::tip_draw, ui::lift_hover, world::draw_world_overlay, world::lookout_range, paths::draw_paths),
                )
                    .chain()
                    .in_set(HudSet),
            )
            .add_systems(Update, elevation::update_contours.after(sync_lens).in_set(HudSet))
            .add_systems(Update, settle_dam_line.after(refresh_panels).in_set(HudSet));
    }
}

/// The water engine's answer for a drawn dam line (see `sim::dam_preview::settle`), with the
/// pond cells that are dry today and the trees standing on them.
#[derive(Clone, Debug, Default)]
pub struct DamLook {
    pub water: sim::dam_preview::Settled,
    pub new_wet: usize,
    pub trees: Vec<Cell>,
}

impl DamLook {
    fn new(s: &sim::SimState, water: sim::dam_preview::Settled) -> Self {
        let dry: Vec<Cell> = water.flooded.iter().copied().filter(|&(x, y)| !build_logic::is_wet(s, x, y)).collect();
        let dry_set: std::collections::HashSet<Cell> = dry.iter().copied().collect();
        let trees = s.resources.items().iter().filter(|r| r.kind.is_tree() && r.amount > 0.0 && dry_set.contains(&r.cell)).map(|r| r.cell).collect();
        DamLook { water, new_wet: dry.len(), trees }
    }
}

/// The background run of the water engine for the dam line being drawn (see
/// `sim::dam_preview::settle`): one at a time, restarted whenever the line changes.
#[derive(Resource, Default)]
struct DamSettle(Option<(Vec<Cell>, bevy::tasks::Task<Option<sim::dam_preview::Settled>>)>, Option<(Vec<Cell>, f32)>);

/// How long a drawn line must hold still before the water engine is asked about it (copying
/// the water takes a few milliseconds, too many to do on every cell of a drag).
const SETTLE_AFTER_S: f32 = 0.2;

fn settle_dam_line(time: Res<Time>, sim: Res<crate::state::Sim>, mut hud: ResMut<HudState>, mut job: ResMut<DamSettle>) {
    let want = (hud.tool == Tool::Dam).then(|| hud.plan.as_ref()).flatten().filter(|p| p.refusal.is_none() && !p.settle_cells.is_empty());
    let Some(plan) = want else {
        job.0 = None;
        return;
    };
    if let Some((line, task)) = job.0.as_mut() {
        if *line != plan.line {
            job.0 = None;
        } else if let Some(result) = bevy::tasks::block_on(bevy::tasks::futures_lite::future::poll_once(task)) {
            let line = std::mem::take(line);
            job.0 = None;
            // Nothing left to raise on this line: the water stays as it is.
            hud.settled = Some((line, result.map(|r| DamLook::new(&sim.0, r)).unwrap_or_default()));
            return;
        } else {
            return;
        }
    }
    if hud.settled.as_ref().is_some_and(|(line, _)| *line == plan.line) {
        return;
    }
    let now = time.elapsed_secs();
    match &job.1 {
        Some((line, since)) if *line == plan.line => {
            if now - since < SETTLE_AFTER_S {
                return;
            }
        }
        _ => {
            job.1 = Some((plan.line.clone(), now));
            return;
        }
    }
    let (water, height) = (sim.0.water.clone(), sim.0.height.clone());
    let (cells, heights) = (plan.settle_cells.clone(), plan.settle_heights.clone());
    let task = bevy::tasks::AsyncComputeTaskPool::get().spawn(async move { sim::dam_preview::settle(water, &height, &cells, &heights) });
    job.0 = Some((plan.line.clone(), task));
}

fn spawn_hosts(mut commands: Commands) {
    for (i, p) in [Panel::Markers, Panel::Strip, Panel::Calendar, Panel::Rail, Panel::Minimap, Panel::Bottom, Panel::Journal, Panel::Colony, Panel::Right, Panel::Cursor, Panel::Welcome].into_iter().enumerate() {
        commands.spawn((
            Node { position_type: PositionType::Absolute, left: Val::Px(0.0), right: Val::Px(0.0), top: Val::Px(0.0), bottom: Val::Px(0.0), ..default() },
            GlobalZIndex(10 + i as i32),
            FocusPolicy::Pass,
            p,
            Mounted::default(),
        ));
    }
}

pub(crate) fn settle_error(e: crate::flow::SettleError) -> &'static str {
    use crate::flow::SettleError::*;
    match e {
        OutOfBounds => "Off map",
        Unexplored => "Unexplored",
        Water => "Water",
        NoRoom => "No room",
    }
}

pub(crate) fn add_beaver_tip(s: &sim::SimState, b: BuildingId) -> Tip {
    let mut t = Tip::name("Add a beaver");
    match sim::jobs::add_beaver_check(s, b) {
        Ok(p) => {
            let who = s.colony.beaver(p.who).map(|x| x.name.clone()).unwrap_or_default();
            t.push(TipLine::new(who, "Free").icon(Icon::Idle, INK));
        }
        Err(why) => {
            t.push(TipLine::new(why, "").color(WARN));
        }
    }
    t
}

#[allow(clippy::too_many_arguments)]
fn refresh_panels(
    mut commands: Commands,
    icons: Res<Icons>,
    time: Res<Time>,
    mut timer: ResMut<RefreshTimer>,
    sim: Res<Sim>,
    mut hud: ResMut<HudState>,
    clock: Res<SimClock>,
    sel: Res<Selection>,
    hover: Res<HoverCell>,
    lens: Res<MapLens>,
    orbit: Res<crate::camera::OrbitCamera>,
    (minimap, portrait): (Option<Res<MinimapImage>>, Option<Res<crate::entities_render::portrait::PortraitImage>>),
    menu: Res<MenuOpen>,
    flow: Res<Flow>,
    (cursor, mut last, notices, snap): (Res<CursorUi>, ResMut<LastProblems>, Res<Notices>, Res<world::WorksSnap>),
    mut hosts: Query<(Entity, &Panel, &mut Mounted)>,
) {
    let _p = ::sim::prof::scope("game/refresh panels");
    let due = timer.0.tick(time.delta()).just_finished();
    let changed = hud.is_changed() || sel.is_changed() || lens.is_changed() || menu.is_changed() || clock.is_changed() || (hover.is_changed() && hud.tool != Tool::None) || notices.is_changed();
    if !due && !changed {
        return;
    }
    let now = time.elapsed_secs();
    if hud.msg.as_ref().is_some_and(|(_, t)| now - t > MSG_SECS) {
        hud.msg = None;
    }
    let s = &sim.0;
    if hud.tool == Tool::Dam && !hud.line.is_empty() {
        let stale = hud.plan.as_ref().is_none_or(|p| p.line != hud.line);
        if stale {
            hud.plan = Some(build_logic::line_plan(s, &hud.line, hud.extending));
        }
    }
    if hud.tool == Tool::Canal && hud.line.len() > 1 && hud.extending.is_none() {
        let stale = hud.canal_plan.as_ref().is_none_or(|p| p.0 != hud.line);
        if stale {
            let line = hud.line.clone();
            hud.canal_plan = Some((line.clone(), sim::canals::plan(s, &line)));
        }
    }
    update_drowned(s, &mut hud);
    // Placing a gate or lock stops once its dam is no longer the one selected.
    if hud.opening.is_some_and(|(id, _)| *sel != Selection::Site(id) || sim::dams::dam(s, id).is_none()) {
        hud.opening = None;
    }
    if let Selection::Site(id) = *sel {
        if let Some(d) = sim::dams::dam(s, id) {
            let day = s.time_days.floor();
            if hud.dam_ghost.as_ref().is_none_or(|g| g.0 != id || g.1 != day) {
                let around = d.pond.as_ref().is_some_and(|p| matches!(p.spill, sim::dams::Spill::Around(_)));
                let cells = if around { sim::dams::plan_extend(s, id).map(|p| p.added(d)).unwrap_or_default() } else { Vec::new() };
                hud.dam_ghost = Some((id, day, cells));
            }
        }
    }
    let problems = sim::hud::problems(s);
    last.0 = problems.clone();
    let hud: &HudState = &hud;
    let ctx = panels::Ctx {
        s,
        hud,
        clock: &clock,
        sel: *sel,
        lens: *lens,
        hover: hover.0,
        works_cell: snap.0,
        problems: &problems,
        minimap: minimap.as_deref(),
        portrait: portrait.as_deref(),
        view_cell: crate::flow::world_cell(&s.height, orbit.target),
        cursor: cursor.0,
        journal: &notices.0,
        now,
    };
    let founded = s.is_running();
    for (e, p, mut m) in &mut hosts {
        if !SLOW_PANELS.contains(p) {
            continue;
        }
        if menu.0 || (!founded && flow.phase != crate::flow::Phase::Scouting) {
            ui::clear(&mut commands, e, &mut m);
            continue;
        }
        let placing = ctx.placing();
        let tree = match p {
            Panel::Bottom if !founded => panels::settle_chip(&ctx),
            _ if !founded => Vec::new(),
            Panel::Strip => panels::strip(&ctx),
            Panel::Calendar => panels::calendar(&ctx),
            // A beaver's full page covers the map; the card would only sit on top of it.
            Panel::Right if ctx.hud.details.is_some() || ctx.hud.lodge_page.is_some() => Vec::new(),
            Panel::Right => panels::right(&ctx),
            Panel::Rail if !placing => panels::rail(&ctx),
            Panel::Bottom => panels::bottom(&ctx),
            Panel::Minimap => panels::minimap(&ctx),
            Panel::Colony if ctx.hud.ledger.is_some() => ledger::screen(&ctx),
            Panel::Colony => colony::screen(&ctx),
            Panel::Journal => journal::screen(&ctx),
            Panel::Welcome => welcome::screen(&ctx),
            _ => continue,
        };
        ui::reconcile(&mut commands, &icons, e, &mut m, tree, 0);
    }
}

#[allow(clippy::too_many_arguments)]
fn refresh_fast(
    mut commands: Commands,
    icons: Res<Icons>,
    sim: Res<Sim>,
    hud: Res<HudState>,
    clock: Res<SimClock>,
    hover: Res<HoverCell>,
    cursor: Res<CursorUi>,
    cams: Query<(&Camera, &GlobalTransform), With<MainCamera>>,
    scale: Res<UiScale>,
    menu: Res<MenuOpen>,
    flow: Res<Flow>,
    lens: Res<MapLens>,
    sel: Res<Selection>,
    (windows, pops, problems, time, wolf_drawn, orbit): (Query<&Window, With<bevy::window::PrimaryWindow>>, Res<Pops>, Res<LastProblems>, Res<Time>, Option<Res<crate::entities_render::WolfDrawn>>, Res<crate::camera::OrbitCamera>),
    mut hosts: Query<(Entity, &Panel, &mut Mounted)>,
) {
    let _p = ::sim::prof::scope("game/refresh fast");
    let s = &sim.0;
    for (e, p, mut m) in &mut hosts {
        if !matches!(p, Panel::Cursor | Panel::Markers) {
            continue;
        }
        if menu.0 {
            ui::clear(&mut commands, e, &mut m);
            continue;
        }
        let tree = match p {
            Panel::Cursor if !s.is_running() => match (hover.0, cursor.0) {
                (Some(cell), Some(cur)) if flow.phase == crate::flow::Phase::Scouting => panels::settle_tag(s, &flow, cell, cur),
                _ => Vec::new(),
            },
            _ if !s.is_running() => Vec::new(),
            Panel::Cursor => {
                let ctx = panels::Ctx { s, hud: &hud, clock: &clock, sel: Selection::None, lens: *lens, hover: hover.0, works_cell: None, problems: &[], minimap: None, portrait: None, view_cell: (0.0, 0.0), cursor: cursor.0, journal: &[], now: time.elapsed_secs() };
                let tree = if hud.tool == Tool::Canal {
                    let win = windows.single().map(|w| Vec2::new(w.width(), w.height()) / scale.0.max(0.01)).unwrap_or(Vec2::new(1280.0, 720.0));
                    let corner = hud.corners.last().and_then(|c| {
                        let (cam, gt) = cams.single().ok()?;
                        let mut w = cell_world(&s.height, (c.0 as f32 + 0.5, c.1 as f32 + 0.5));
                        w.y += 0.6;
                        cam.world_to_viewport(gt, w).ok().map(|v| v / scale.0.max(0.01))
                    });
                    match (hud.drag_from.is_some(), cursor.0, corner) {
                        (true, Some(at), _) => panels::canal_tag(&ctx, at, false, win),
                        (false, _, Some(at)) => panels::canal_tag(&ctx, at, true, win),
                        _ => Vec::new(),
                    }
                } else if hud.tool == Tool::Dam {
                    let win = windows.single().map(|w| Vec2::new(w.width(), w.height()) / scale.0.max(0.01)).unwrap_or(Vec2::new(1280.0, 720.0));
                    let dragging = hud.drag_from.is_some();
                    let corner = hud.corners.last().and_then(|c| {
                        let (cam, gt) = cams.single().ok()?;
                        let mut w = cell_world(&s.height, (c.0 as f32, c.1 as f32));
                        w.y += 0.6;
                        cam.world_to_viewport(gt, w).ok().map(|v| v / scale.0.max(0.01))
                    });
                    match (dragging, cursor.0, corner) {
                        (true, Some(at), _) => panels::dam_tag(&ctx, at, false, win),
                        (false, _, Some(at)) => panels::dam_tag(&ctx, at, true, win),
                        _ => Vec::new(),
                    }
                } else {
                    panels::cursor_tag(&ctx)
                };
                let mut tree = tree;
                // The box being dragged out to select beavers.
                if let (Tool::None, None, Some(a), Some(b)) = (hud.tool, hud.grab, hud.box_from, cursor.0) {
                    if a.distance(b) > 8.0 {
                        let (lo, hi) = (a.min(b), a.max(b));
                        let mut v = ui::VNode::new(Node { position_type: PositionType::Absolute, left: Val::Px(lo.x), top: Val::Px(lo.y), width: Val::Px(hi.x - lo.x), height: Val::Px(hi.y - lo.y), border: UiRect::all(Val::Px(1.5)), ..default() });
                        v.bg(ui::SELECT.with_alpha(0.08)).border(BorderColor::all(ui::SELECT.with_alpha(0.8)));
                        tree.push(v);
                    }
                }
                tree.extend(world::screen_pops(&pops.0, time.elapsed_secs()));
                let bctx = panels::Ctx { problems: &problems.0, ..ctx };
                tree.extend(panels::badges(&bctx));
                tree
            }
            _ => match cams.single() {
                Ok((cam, gt)) => {
                    let mut tree = world::water_labels(s, cam, gt, scale.0, hover.0, &orbit, *lens);
                    tree.extend(world::markers(s, cam, gt, scale.0, &hud, *sel, &problems.0, &pops.0, time.elapsed_secs(), wolf_drawn.as_ref().and_then(|w| w.at(sim::predators::nearest_seen(s)?.key()))));
                    tree
                }
                Err(_) => Vec::new(),
            },
        };
        ui::reconcile(&mut commands, &icons, e, &mut m, tree, 0);
    }
}

/// Which buildings the dam being drawn would drown, from the water engine's own run; worked out
/// again only when that changes.
fn update_drowned(s: &sim::SimState, hud: &mut HudState) {
    let Some(r) = hud.plan.as_ref().filter(|p| hud.tool == Tool::Dam && p.refusal.is_none()).and_then(|_| hud.dam_settled()) else {
        if hud.drowned.is_some() {
            hud.drowned = None;
        }
        return;
    };
    let key = hud.plan.as_ref().map(|p| p.line.clone()).unwrap_or_default();
    if hud.drowned.as_ref().is_some_and(|d| d.0 == key) {
        return;
    }
    let depth: std::collections::HashMap<Cell, f32> = r.water.flooded.iter().copied().zip(r.water.depths.iter().copied()).collect();
    let drowned = sim::dam_preview::drowned_by(s, |x, y| depth.get(&(x, y)).copied().unwrap_or(0.0));
    hud.drowned = Some((key, drowned));
}

fn sync_lens(keys: Res<ButtonInput<KeyCode>>, lens: Res<MapLens>, hud: Res<HudState>, sim: Res<Sim>) {
    let alt = keys.pressed(KeyCode::AltLeft) || keys.pressed(KeyCode::AltRight);
    let _ = &sim;
    let shown = if alt || matches!(hud.tool, Tool::Dam | Tool::Canal) { MapLens::Water } else { *lens };
    // `BC_LENS=water|height|deposits|paths` holds that lens, for screenshots.
    let shown = match std::env::var("BC_LENS").as_deref() {
        Ok("water") => MapLens::Water,
        Ok("height") => MapLens::Elevation,
        Ok("deposits") => MapLens::Deposits,
        Ok("paths") => MapLens::Paths,
        _ => shown,
    };
    paths::SHOWN.store(shown == MapLens::Paths, std::sync::atomic::Ordering::Relaxed);
    crate::terrain_render::MAP_LENS.store(shown.code(), std::sync::atomic::Ordering::Relaxed);
}
