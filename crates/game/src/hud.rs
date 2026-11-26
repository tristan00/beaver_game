#![allow(deprecated)]
mod actions;
mod canal_panel;
mod colony;
mod costs;
mod dam_panel;
mod elevation;
mod family;
mod input;
mod inspect;
mod journal;
mod kin;
mod ledger;
mod life;
mod minimap;
mod panels;
mod paths;
pub(crate) mod scrollbar;
mod ui;
mod water_legend;
mod welcome;
pub(crate) mod words;
mod works;
mod world;

pub(crate) use ui::{font, ACCENT, BAD, BARK, FAINT, GOOD, INK, MUTED, PANEL, WARN, WATER};
pub(crate) use words::job_icon;
pub(crate) use world::{HoverPoint, HoverThing};
pub use crate::build_logic::LinePlan;

use actions::{button_actions, commit_line, undo_last};
use input::{box_select, follow_beaver, frame_camera, hotkeys, hover_cell, next_idle, pointer_over_ui, track_cursor, world_input};
use minimap::{minimap_click, setup_minimap, update_minimap, MinimapImage};
use crate::build_logic;
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
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use ui::{Mounted, Tag, TagComp, Tip, TipLine, Tips};
use words::*;

pub(crate) fn text(p: &mut ChildSpawnerCommands, s: impl Into<String>, size: f32, color: Color) { p.spawn((Text::new(s.into()), font(size, false), TextColor(color))); }
pub(crate) fn bold(p: &mut ChildSpawnerCommands, s: impl Into<String>, size: f32, color: Color) { p.spawn((Text::new(s.into()), font(size, true), TextColor(color))); }
pub(crate) fn row(gap: f32) -> Node { ui::row_node(gap) }
pub(crate) fn hover_tip(label: impl Into<String>) -> (Interaction, ui::TipComp) { (Interaction::default(), ui::TipComp { tip: Tip::name(label), depth: 0 }) }

const REFRESH_SECS: f32 = 0.1;
pub(crate) const REMOVE_ARM_SECS: f32 = 3.0;
const MSG_SECS: f32 = 3.0;
pub const POP_SECS: f32 = 2.4;
pub const NOTICE_SECS: f32 = 7.0;
pub const JOURNAL_KEPT: usize = 200;
const UI_DESIGN_HEIGHT: f32 = 900.0;
const SETTLE_AFTER_S: f32 = 0.2;

#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub enum Tool { #[default] None, Building(BuildingKind), Dam, Canal, Harvest(Harvest) }

impl Tool {
    pub fn of(kind: BuildingKind) -> Tool {
        if kind.is_dam() { Tool::Dam } else if kind.is_canal() { Tool::Canal } else { Tool::Building(kind) }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Harvest { Gather, Cut }
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Opening { Gate, Lock }
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum DamTab { #[default] Status, History }
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ColonyTab { Population, Jobs, Beavers, Run }
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LedgerTab { Charts, Dams, Canals, Buildings, Stores, Upkeep }
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Span { Season, Year, #[default] All }
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum BeaverCol { #[default] Family, Name, Age, Job, Doing, Skill, Pace }
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Page { Beaver(BeaverId), Lodge(BuildingId), Works(BuildingId) }
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Msg { pub icon: Option<(Icon, Color)>, pub icon2: Option<(Icon, Color)>, pub text: String }
#[derive(Clone, Copy, Debug, PartialEq)]
enum UndoItem { Site(BuildingId) }

#[derive(Resource, Default)]
pub struct HudState {
    pub open_badge: Option<String>,
    pub tool: Tool,
    pub drag_from: Option<Cell>,
    pub grab: Option<(BuildingId, Cell)>,
    pub corners: Vec<Cell>,
    pub line: Vec<Cell>,
    pub plan: Option<LinePlan>,
    pub settled: Option<(Vec<Cell>, DamLook)>,
    pub canal_plan: Option<(Vec<Cell>, Result<sim::canals::Plan, String>)>,
    pub extending: Option<BuildingId>,
    pub dam_cells: Vec<Cell>,
    pub dam_tab: DamTab,
    pub opening: Option<(BuildingId, Opening)>,
    pub gate_sel: Option<(BuildingId, Cell)>,
    pub remove_armed: Option<(BuildingId, f32)>,
    pub dam_ghost: Option<(BuildingId, f64, Vec<Cell>)>,
    pub msg: Option<(Msg, f32)>,
    idle_cursor: usize,
    pub colony: Option<ColonyTab>,
    pub page: Option<Page>,
    pub kin: kin::KinFilter,
    pub back: Vec<Page>,
    pub ledger: Option<LedgerTab>,
    pub span: Span,
    pub sort: (BeaverCol, bool),
    pub scroll: usize,
    pub peek: Option<BeaverId>,
    pub journal_open: bool,
    pub welcome: bool,
    pub drowned: Option<(Vec<Cell>, Vec<BuildingId>)>,
    undo: Vec<UndoItem>,
    pub selected: Vec<BeaverId>,
    pub follow: Option<BeaverId>,
    consume_left_release: bool,
    pub pinned: Vec<Selection>,
    box_from: Option<Vec2>,
    box_done: Option<(Vec2, Vec2)>,
    frame_req: Option<Vec3>,
}

#[derive(Resource, Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum MapLens { #[default] World, Water, Forest, Elevation, Deposits, Paths }

impl MapLens {
    pub fn code(self) -> u8 {
        match self { MapLens::World | MapLens::Paths => 0, MapLens::Water => 1, MapLens::Forest => 3, MapLens::Elevation => 4, MapLens::Deposits => crate::terrain_render::DEPOSIT_LENS }
    }
}

#[derive(Resource, Default)]
pub struct TextCapture(pub bool);
#[derive(Resource, Default)]
pub struct MenuOpen(pub bool);
#[derive(Resource, Default)]
pub struct HoverCell(pub Option<Cell>);
#[derive(Clone, Debug)]
pub struct Pop { pub at: PopAt, pub icons: Vec<(Icon, Color)>, pub text: String, pub born: f32 }
#[derive(Clone, Copy, Debug)]
pub enum PopAt { Screen(Vec2), Cell((f32, f32)) }
#[derive(Resource, Default)]
pub struct Pops(pub Vec<Pop>);
#[derive(Clone, Debug)]
pub struct Notice { pub icons: Vec<(Icon, Color)>, pub text: String, pub born: f32, pub day: f64, pub focus: Option<NoticeAt> }
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum NoticeAt { Beaver(BeaverId), Building(BuildingId), Spot((f32, f32)), Wolf((f32, f32)) }
#[derive(Resource, Default)]
pub struct Notices(pub Vec<Notice>, pub f64);
#[derive(Resource, Default)]
pub struct Floods(pub u32);
#[derive(Resource, Default)]
pub struct WolfCues { pub sighted: u32, pub kills: u32 }
#[derive(Resource, Default)]
pub struct LastProblems(pub Vec<sim::hud::Problem>);
#[derive(Resource, Default)]
pub struct PointerOverUi(pub bool);
#[derive(Resource, Default)]
pub struct PointerOverPin(pub bool);
#[derive(Resource, Default)]
struct CursorUi(Option<Vec2>);
#[derive(Resource)]
struct RefreshTimer(Timer);
#[derive(Component)]
pub struct Blocks;

#[derive(Component, Clone, Copy, PartialEq, Eq, Hash, Debug)]
enum Panel { Strip, Calendar, Right, Rail, Bottom, Minimap, Colony, Journal, Cursor, Markers, Welcome }

const SLOW_PANELS: [Panel; 10] = [Panel::Markers, Panel::Strip, Panel::Calendar, Panel::Rail, Panel::Minimap, Panel::Bottom, Panel::Colony, Panel::Journal, Panel::Right, Panel::Welcome];

#[derive(Component, Clone, Debug, PartialEq)]
pub enum HudAction {
    Fix(Fix, Option<Cell>), OpenPage(Option<Page>), Kin(kin::KinFilter), FocusWolf, CycleIdle,
    Pause, Speed(Speed), Pick(BuildingKind), CancelTool, GrowDam(BuildingId, sim::DamGrowth),
    Upkeep(BuildingId, bool), Opening(BuildingId, Opening), GateChip(BuildingId, Cell),
    RemoveGate(BuildingId, Cell), SetGate(BuildingId, Cell, sim::GateSetting), UpgradeLock(BuildingId, Cell),
    GrowCanal(BuildingId, sim::CanalGrowth), DamCell(Cell), DamTab(DamTab),
    ConfirmLine, SetLens(MapLens), PathsOnly(Option<Job>), Colony(Option<ColonyTab>), Ledger(Option<LedgerTab>),
    Span(Span), Sort(BeaverCol), Scroll(i32), Journal(bool), GoTo(NoticeAt), SelectBeaver(BeaverId),
    SelectBuilding(BuildingId), Follow(BeaverId), FreeBeaver(BeaverId), Staff(BuildingId, i32), Badge(String),
    FillStore(BuildingId, bool), Balance(BuildingId, bool), CancelBuild(BuildingId),
    RoleAdjust(Job, i32), ShoreLimit(Job, Option<u16>), Paused(BuildingId, bool),
    Refit(BuildingId, Option<sim::maintenance::Refit>), SlotOff(BuildingId, usize, bool),
    Pin(Selection), ClearPicks, PageBack, CloseWelcome,
    ClosePanel(Selection, bool), RemoveWorks(BuildingId),
}

pub fn reset_for_new_game(world: &mut World) {
    world.insert_resource(HudState::default());
    world.insert_resource(MapLens::default());
    if let Some(mut n) = world.get_resource_mut::<Notices>() { n.0.clear(); }
    if let Some(mut s) = world.get_resource_mut::<Selection>() { *s = Selection::None; }
}

impl HudState {
    pub(crate) fn swallow_click(&mut self) { self.consume_left_release = true; }
    fn set_tool(&mut self, t: Tool) {
        self.tool = t;
        (self.drag_from, self.plan, self.canal_plan, self.extending, self.opening) = (None, None, None, None, None);
        self.line.clear();
        self.corners.clear();
    }
    fn finish_placement(&mut self) { self.set_tool(Tool::None); }
    pub fn page(&self) -> Option<Page> { self.page }
    fn show_page(&mut self, p: Option<Page>) {
        if matches!(p, Some(Page::Beaver(_))) && self.page != p { self.kin = kin::KinFilter::All; }
        self.page = p;
        self.scroll = 0;
        if p.is_some() { self.ledger = None; }
    }
    pub fn is_busy(&self, sel: Selection, lens: MapLens) -> bool {
        self.welcome || self.opening.is_some() || self.tool != Tool::None || self.page.is_some() || self.colony.is_some()
            || self.ledger.is_some() || self.open_badge.is_some() || self.journal_open || self.follow.is_some()
            || sel != Selection::None || !self.selected.is_empty() || lens != MapLens::World
    }
    pub fn dismiss_top(&mut self, sel: &mut Selection, lens: &mut MapLens) {
        if self.welcome { self.welcome = false; }
        else if self.opening.is_some() { self.opening = None; }
        else if self.tool != Tool::None { self.set_tool(Tool::None); }
        else if self.page.is_some() { self.page_back(); }
        else if self.colony.is_some() || self.ledger.is_some() { (self.colony, self.ledger) = (None, None); }
        else if self.open_badge.is_some() { self.open_badge = None; }
        else if self.journal_open { self.journal_open = false; }
        else if self.follow.is_some() { self.follow = None; }
        else if *sel != Selection::None || !self.selected.is_empty() { self.clear_selection(sel); }
        else if *lens != MapLens::World { *lens = MapLens::World; }
    }
    pub(crate) fn clear_selection(&mut self, sel: &mut Selection) {
        *sel = Selection::None;
        self.selected.clear();
        self.follow = None;
        self.dam_cells.clear();
    }
    pub(crate) fn select_beaver(&mut self, s: &sim::SimState, sel: &mut Selection, flow: &mut Flow, id: BeaverId) {
        let Some(b) = s.colony.beaver(id) else { return };
        *sel = Selection::Beaver(id);
        self.selected = vec![id];
        flow.focus = Some(cell_world(&s.height, b.pos));
    }
    pub(crate) fn select_building(&mut self, s: &sim::SimState, sel: &mut Selection, flow: &mut Flow, id: BuildingId) {
        let Some(b) = s.buildings.building(id) else { return };
        *sel = Selection::Site(id);
        self.selected.clear();
        self.dam_cells.clear();
        flow.focus = Some(cell_world(&s.height, b.centre()));
    }
    pub(crate) fn open_page(&mut self, p: Page) {
        if let Some(now) = self.page().filter(|now| *now != p) {
            self.back.push(now);
            if self.back.len() > 16 { self.back.remove(0); }
        }
        self.show_page(Some(p));
    }
    pub(crate) fn page_back(&mut self) { let p = self.back.pop(); self.show_page(p); }
    pub(crate) fn close_pages(&mut self) { self.back.clear(); self.show_page(None); }
    pub(crate) fn open_book(&mut self, colony: Option<ColonyTab>, ledger: Option<LedgerTab>) {
        self.close_pages();
        if ledger != self.ledger || colony != self.colony { self.scroll = 0; }
        (self.colony, self.ledger) = (colony, ledger);
    }
    fn say(&mut self, icon: Option<(Icon, Color)>, icon2: Option<(Icon, Color)>, text: impl Into<String>, now: f32) {
        self.msg = Some((Msg { icon, icon2, text: text.into() }, now));
    }
    fn refuse(&mut self, why: impl Into<String>, now: f32) { self.say(Some((Icon::Cross, BAD)), None, why, now); }
    pub fn dam_preview(&self) -> Option<&DamPreview> { self.plan.as_ref().and_then(|p| p.preview.as_ref()) }
    pub fn dam_settled(&self) -> Option<&DamLook> { let plan = self.plan.as_ref()?; self.settled.as_ref().filter(|(line, _)| *line == plan.line).map(|(_, s)| s) }
    pub fn set_tool_for_report(&mut self, t: Tool) { if self.tool != t { self.set_tool(t); } }
    pub fn draw_for_report(&mut self, corners: Vec<Cell>, dragging: bool) {
        self.set_tool(Tool::Dam);
        self.line = build_logic::polyline(&corners);
        self.drag_from = dragging.then(|| corners[0]);
        self.corners = if dragging { corners[..1].to_vec() } else { corners };
    }
}

impl Notices {
    pub fn push(&mut self, icons: Vec<(Icon, Color)>, text: impl Into<String>, now: f32) { self.push_at(icons, text, now, None); }
    pub fn push_at(&mut self, icons: Vec<(Icon, Color)>, text: impl Into<String>, now: f32, focus: Option<NoticeAt>) {
        let text = text.into();
        sim::trace::event(format!("NOTICE icons={:?} text={text:?}", icons.iter().map(|(i, _)| format!("{i:?}")).collect::<Vec<_>>()));
        self.0.push(Notice { icons, text, born: now, day: self.1, focus });
        if self.0.len() > JOURNAL_KEPT { self.0.remove(0); }
    }
}

fn notice_wolf(time: Res<Time>, sim: Res<Sim>, mut notices: ResMut<Notices>, mut cues: ResMut<WolfCues>, mut told: Local<(u64, Vec<(u32, f64)>)>) {
    let s = &sim.0;
    if notices.1 != s.time_days { notices.1 = s.time_days; }
    if told.0 != s.instance { told.1.clear(); told.0 = s.instance; }
    told.1.retain(|(id, at)| s.predators.wolf(*id).is_some() && s.time_days >= *at);
    for w in sim::predators::seen(s) {
        if !told.1.iter().any(|(id, _)| *id == w.id) {
            told.1.push((w.id, s.time_days));
            notices.push_at(vec![(Icon::Wolf, BAD)], "Wolf sighted", time.elapsed_secs(), Some(NoticeAt::Wolf(w.pos)));
            cues.sighted += 1;
        }
    }
}

type BuildingsSeen = BTreeMap<BuildingId, (BuildingKind, bool, (f32, f32))>;

fn notice_buildings(time: Res<Time>, sim: Res<Sim>, mut notices: ResMut<Notices>, mut was: Local<Option<(u64, BuildingsSeen)>>) {
    let s = &sim.0;
    if !s.is_running() { *was = None; return; }
    if !was.as_ref().is_none_or(|(i, b)| *i != s.instance || b.len() != s.buildings.buildings.len() || s.buildings.buildings.iter().any(|x| b.get(&x.id).is_none_or(|v| v.1 != x.is_complete()))) { return; }
    let now: BuildingsSeen = s.buildings.buildings.iter().map(|b| (b.id, (b.kind, b.is_complete(), b.centre()))).collect();
    if let Some((_, before)) = was.as_ref().filter(|(i, _)| *i == s.instance) {
        let t = time.elapsed_secs();
        for (&id, &(kind, done, _)) in &now {
            if done && before.get(&id).is_some_and(|b| !b.1) {
                notices.push_at(vec![(building_icon(kind), INK), (Icon::Check, GOOD)], format!("{} built", kind.spec().name.as_str()), t, Some(NoticeAt::Building(id)));
            }
        }
        for (&id, &(kind, done, pos)) in before {
            if done && !now.contains_key(&id) {
                let how = if s.buildings.wrecked.contains(&id) { "fell apart" } else { "lost" };
                notices.push_at(vec![(building_icon(kind), INK), (Icon::Cross, BAD)], format!("{} {how}", kind.spec().name.as_str()), t, Some(NoticeAt::Spot(pos)));
            }
        }
    }
    *was = Some((s.instance, now));
}

fn notice_floods(time: Res<Time>, sim: Res<Sim>, mut notices: ResMut<Notices>, mut floods: ResMut<Floods>, mut was: Local<(u64, f64, BTreeMap<BuildingId, (bool, f64)>)>) {
    let s = &sim.0;
    if !s.is_running() || (was.0 == s.instance && was.1 == s.time_days) { return; }
    let fresh = was.0 != s.instance;
    *was = (s.instance, s.time_days, if fresh { Default::default() } else { std::mem::take(&mut was.2) });
    for b in &s.buildings.buildings {
        let flooded = sim::buildings::flooded(b, &s.water.depth);
        let (before, mut told_at) = was.2.get(&b.id).copied().unwrap_or((flooded || fresh, f64::NEG_INFINITY));
        if flooded && !before && s.time_days - told_at >= 3.0 {
            notices.push_at(vec![(building_icon(b.kind), INK), (Icon::Flood, BAD)], format!("{} flooded", b.kind.spec().name.as_str()), time.elapsed_secs(), Some(NoticeAt::Building(b.id)));
            told_at = s.time_days;
            floods.0 = floods.0.wrapping_add(1);
        }
        was.2.insert(b.id, (flooded, told_at));
    }
    let standing: BTreeSet<BuildingId> = s.buildings.buildings.iter().map(|b| b.id).collect();
    was.2.retain(|id, _| standing.contains(id));
}

fn peek_hover(q: Query<(&Interaction, &ui::Peek)>, mut hud: ResMut<HudState>) {
    let on = q.iter().find(|(i, _)| **i != Interaction::None).map(|(_, p)| p.0);
    if hud.peek != on { hud.peek = on; }
}

pub(crate) fn scroll_max(s: &sim::SimState, hud: &HudState) -> usize {
    match (hud.page, hud.ledger) {
        (Some(Page::Beaver(id)), _) => s.lives.of.get(&id).map_or(0, |l| l.entries.len().saturating_sub(life::ROWS)),
        (Some(_), _) => 0,
        (None, Some(tab)) => ledger::scroll_max(s, tab),
        (None, None) if hud.colony == Some(ColonyTab::Beavers) => ledger::beavers_scroll_max(s),
        _ => 0,
    }
}

fn scroll_lists(scroll: Res<bevy::input::mouse::AccumulatedMouseScroll>, q: Query<(&TagComp, &bevy::ui::RelativeCursorPosition)>, sim: Res<Sim>, mut hud: ResMut<HudState>) {
    if scroll.delta.y == 0.0 || (hud.ledger.is_none() && hud.page().is_none() && hud.colony != Some(ColonyTab::Beavers)) { return; }
    if !q.iter().any(|(t, r)| t.0 == Tag::Scroll && r.normalized.is_some_and(|p| p.x.abs() <= 0.5 && p.y.abs() <= 0.5)) { return; }
    let lines = match scroll.unit { bevy::input::mouse::MouseScrollUnit::Line => scroll.delta.y, bevy::input::mouse::MouseScrollUnit::Pixel => scroll.delta.y / 20.0 };
    let next = ((hud.scroll as i64 + (-lines * 2.0).round() as i64).max(0) as usize).min(scroll_max(&sim.0, &hud));
    if next != hud.scroll { hud.scroll = next; }
}

#[derive(Default)]
pub struct PopWatch { ready: bool, beavers: HashSet<u64>, worn: HashSet<Cell> }

fn collect_pops(time: Res<Time>, cursor: Res<CursorUi>, mut hud: ResMut<HudState>, sim: Res<Sim>, mut pops: ResMut<Pops>, mut w: Local<PopWatch>) {
    let _p = ::sim::prof::scope("game/collect pops");
    let now = time.elapsed_secs();
    pops.0.retain(|p| now - p.born < POP_SECS);
    let before = pops.0.len();
    if let Some((m, _)) = hud.msg.take() {
        let at = cursor.0.map_or(Vec2::new(640.0, 300.0), |c| c + Vec2::new(0.0, -28.0));
        pops.0.push(Pop { at: PopAt::Screen(at), icons: [m.icon, m.icon2].into_iter().flatten().collect(), text: m.text, born: now });
    }
    let s = &sim.0;
    if !s.is_running() { w.ready = false; } else {
        let ids: HashSet<u64> = s.colony.beavers.iter().map(|b| b.id.0).collect();
        let worn: HashSet<Cell> = s.buildings.buildings.iter().filter_map(|b| b.dam.as_deref()).flat_map(|d| d.cells.iter().map(|c| c.cell))
            .filter(|&c| sim::dams::built(s, c) > 0.0 && sim::dams::integrity(s, c) < sim::dams::REPAIR_BELOW).collect();
        if w.ready {
            for b in s.colony.beavers.iter().filter(|b| !w.beavers.contains(&b.id.0)) {
                let at = b.lodge.and_then(|id| s.buildings.building(id)).map_or(b.pos, |l| l.centre());
                pops.0.push(Pop { at: PopAt::Cell(at), icons: vec![(if b.stage == sim::colony::Stage::Kit { Icon::Kit } else { Icon::Adult }, GOOD)], text: "+1".into(), born: now });
            }
            for &c in worn.difference(&w.worn) {
                pops.0.push(Pop { at: PopAt::Cell((c.0 as f32 + 0.5, c.1 as f32 + 0.5)), icons: vec![(Icon::Drop, WATER), (Icon::Warn, BAD)], text: String::new(), born: now });
            }
        }
        (w.beavers, w.worn, w.ready) = (ids, worn, true);
    }
    for p in &pops.0[before.min(pops.0.len())..] {
        let icons: Vec<_> = p.icons.iter().map(|(i, _)| format!("{i:?}")).collect();
        sim::trace::event(format!("POP day={:.4} icons={icons:?} text={:?} at={:?}", sim.0.time_days, p.text, match p.at { PopAt::Screen(v) => format!("screen({:.0},{:.0})", v.x, v.y), PopAt::Cell(c) => format!("cell({:.1},{:.1})", c.0, c.1) }));
    }
}

pub struct HudPlugin;
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct HudSet;

fn load_ui_font(mut fonts: ResMut<Assets<Font>>) {
    for path in ["C:/Windows/Fonts/bahnschrift.ttf", "/System/Library/Fonts/Supplemental/Futura.ttc", "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf"] {
        if let Ok(bytes) = std::fs::read(path) { let _ = fonts.insert(Handle::<Font>::default().id(), Font::from_bytes(bytes)); return; }
    }
}

impl Plugin for HudPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, load_ui_font).init_resource::<Notices>().init_resource::<WolfCues>();
        if app.world().resource::<crate::cli::Cli>().screenshot.is_some() && std::env::var_os("BC_HUD").is_none() { return; }
        paths::setup(app);
        app.init_resource::<HudState>().init_resource::<HoverCell>().init_resource::<PointerOverUi>()
            .init_resource::<PointerOverPin>().init_resource::<CursorUi>().init_resource::<MapLens>()
            .init_resource::<TextCapture>().init_resource::<MenuOpen>().init_resource::<Tips>()
            .init_resource::<Pops>().init_resource::<Floods>().init_resource::<LastProblems>()
            .init_resource::<world::HoverThing>().init_resource::<world::HoverPoint>().init_resource::<world::WorksSnap>()
            .init_resource::<elevation::Contours>().init_resource::<DamSettle>().init_resource::<scrollbar::ScrollDrag>()
            .insert_resource(RefreshTimer(Timer::from_seconds(REFRESH_SECS, TimerMode::Repeating)))
            .add_systems(Startup, (spawn_hosts, ui::spawn_tip_hosts, setup_minimap))
            .add_systems(Update, (notice_wolf, notice_buildings, notice_floods))
            .add_systems(
                Update,
                (
                    (pointer_over_ui, track_cursor, world::world_hover, hover_cell, hotkeys, world_input, box_select, frame_camera, follow_beaver, peek_hover, scroll_lists, scrollbar::scroll_wheel, scrollbar::scroll_drag),
                    (button_actions, collect_pops, minimap_click, sync_lens, update_minimap, refresh_panels, refresh_fast, scrollbar::sync_scrollbars, ui::tip_logic, ui::tip_draw, ui::lift_hover, world::draw_world_overlay, world::lookout_range, paths::draw_paths),
                ).chain().in_set(HudSet),
            )
            .add_systems(Update, elevation::update_contours.after(sync_lens).in_set(HudSet))
            .add_systems(Update, settle_dam_line.after(refresh_panels).in_set(HudSet))
            .add_systems(PreUpdate, fit_ui_scale);
    }
}

fn fit_ui_scale(windows: Query<&Window, With<bevy::window::PrimaryWindow>>, menu: Option<Res<crate::menu::Menu>>, mut scale: ResMut<UiScale>) {
    let Ok(w) = windows.single() else { return };
    let want = (w.height() / UI_DESIGN_HEIGHT).max(1.0) * menu.map_or(1.0, |m| m.ui_scale);
    if (scale.0 - want).abs() > 0.005 { scale.0 = want; }
}

#[derive(Clone, Debug, Default)]
pub struct DamLook { pub water: sim::dam_preview::Settled, pub new_wet: usize, pub trees: Vec<Cell> }

impl DamLook {
    fn new(s: &sim::SimState, water: sim::dam_preview::Settled) -> Self {
        let dry: HashSet<Cell> = water.flooded.iter().copied().filter(|&(x, y)| !build_logic::is_wet(s, x, y)).collect();
        let trees = s.resources.items().iter().filter(|r| r.kind.is_tree() && r.amount > 0.0 && dry.contains(&r.cell)).map(|r| r.cell).collect();
        DamLook { water, new_wet: dry.len(), trees }
    }
}

#[derive(Resource, Default)]
struct DamSettle(Option<(Vec<Cell>, bevy::tasks::Task<Option<sim::dam_preview::Settled>>)>, Option<(Vec<Cell>, f32)>);

fn settle_dam_line(time: Res<Time>, sim: Res<Sim>, mut hud: ResMut<HudState>, mut job: ResMut<DamSettle>) {
    let Some(plan) = (hud.tool == Tool::Dam).then(|| hud.plan.as_ref()).flatten().filter(|p| p.refusal.is_none() && !p.settle_cells.is_empty()) else {
        job.0 = None;
        return;
    };
    if let Some((line, task)) = job.0.as_mut() {
        if *line != plan.line { job.0 = None; }
        else if let Some(result) = bevy::tasks::block_on(bevy::tasks::futures_lite::future::poll_once(task)) {
            let line = std::mem::take(line);
            job.0 = None;
            hud.settled = Some((line, result.map(|r| DamLook::new(&sim.0, r)).unwrap_or_default()));
            return;
        } else { return; }
    }
    if hud.settled.as_ref().is_some_and(|(line, _)| *line == plan.line) { return; }
    let now = time.elapsed_secs();
    match &job.1 {
        Some((line, since)) if *line == plan.line => if now - since < SETTLE_AFTER_S { return; },
        _ => { job.1 = Some((plan.line.clone(), now)); return; }
    }
    let ((water, height), (cells, heights)) = ((sim.0.water.clone(), sim.0.height.clone()), (plan.settle_cells.clone(), plan.settle_heights.clone()));
    job.0 = Some((plan.line.clone(), bevy::tasks::AsyncComputeTaskPool::get().spawn(async move { sim::dam_preview::settle(water, &height, &cells, &heights) })));
}

fn spawn_hosts(mut commands: Commands) {
    for (i, p) in [Panel::Markers, Panel::Strip, Panel::Calendar, Panel::Rail, Panel::Minimap, Panel::Bottom, Panel::Journal, Panel::Colony, Panel::Right, Panel::Cursor, Panel::Welcome].into_iter().enumerate() {
        commands.spawn((Node { position_type: PositionType::Absolute, left: Val::Px(0.0), right: Val::Px(0.0), top: Val::Px(0.0), bottom: Val::Px(0.0), ..default() }, GlobalZIndex(10 + i as i32), FocusPolicy::Pass, p, Mounted::default()));
    }
}

pub(crate) fn settle_error(e: crate::flow::SettleError) -> &'static str {
    use crate::flow::SettleError::*;
    match e { OutOfBounds => "Off map", Unexplored => "Unexplored", Water => "Water", NoRoom => "No room" }
}

pub(crate) fn add_beaver_tip(s: &sim::SimState, b: BuildingId) -> Tip {
    let mut t = Tip::name("Add a beaver");
    match sim::jobs::add_beaver_check(s, b) {
        Ok(p) => { t.push(TipLine::new(s.colony.beaver(p.who).map(|x| x.name.clone()).unwrap_or_default(), "Free").icon(Icon::Idle, INK)); }
        Err(why) => { t.push(TipLine::new(why, "").color(WARN)); }
    }
    t
}

#[allow(clippy::too_many_arguments)]
fn refresh_panels(
    mut commands: Commands, icons: Res<Icons>, time: Res<Time>, mut timer: ResMut<RefreshTimer>, sim: Res<Sim>,
    mut hud: ResMut<HudState>, clock: Res<SimClock>, sel: Res<Selection>, hover: Res<HoverCell>, lens: Res<MapLens>,
    orbit: Res<crate::camera::OrbitCamera>, (minimap, portrait): (Option<Res<MinimapImage>>, Option<Res<crate::entities_render::portrait::PortraitImage>>),
    menu: Res<MenuOpen>, flow: Res<Flow>, (cursor, mut last, notices, snap): (Res<CursorUi>, ResMut<LastProblems>, Res<Notices>, Res<world::WorksSnap>),
    mut hosts: Query<(Entity, &Panel, &mut Mounted)>,
) {
    let _p = ::sim::prof::scope("game/refresh panels");
    let due = timer.0.tick(time.delta()).just_finished();
    let changed = hud.is_changed() || sel.is_changed() || lens.is_changed() || menu.is_changed() || clock.is_changed() || (hover.is_changed() && hud.tool != Tool::None) || notices.is_changed();
    if !due && !changed { return; }
    let (now, s) = (time.elapsed_secs(), &sim.0);
    if hud.msg.as_ref().is_some_and(|(_, t)| now - t > MSG_SECS) { hud.msg = None; }
    if hud.tool == Tool::Dam && !hud.line.is_empty() && hud.plan.as_ref().is_none_or(|p| p.line != hud.line) {
        hud.plan = Some(build_logic::line_plan(s, &hud.line, hud.extending));
    }
    if hud.tool == Tool::Canal && hud.line.len() > 1 && hud.canal_plan.as_ref().is_none_or(|p| p.0 != hud.line) {
        let line = hud.line.clone();
        let drawn = match hud.extending.and_then(|id| sim::canals::canal(s, id)) {
            Some(k) if line.first() == Some(&k.end()) => &line[1..],
            _ => &line[..],
        };
        let plan = sim::canals::plan_from(s, drawn, hud.extending);
        hud.canal_plan = Some((line, plan));
    }
    update_drowned(s, &mut hud);
    if hud.opening.is_some_and(|(id, _)| *sel != Selection::Site(id) || sim::dams::dam(s, id).is_none()) { hud.opening = None; }
    if let Selection::Site(id) = *sel {
        if let Some(d) = sim::dams::dam(s, id) {
            let day = s.time_days.floor();
            if hud.dam_ghost.as_ref().is_none_or(|g| g.0 != id || g.1 != day) {
                let cells = if d.pond.as_ref().is_some_and(|p| matches!(p.spill, sim::dams::Spill::Around(_))) { sim::dams::plan_extend(s, id).map(|p| p.added(d)).unwrap_or_default() } else { Vec::new() };
                hud.dam_ghost = Some((id, day, cells));
            }
        }
    }
    let problems = sim::hud::problems(s);
    last.0 = problems.clone();
    let ctx = panels::Ctx {
        s, hud: &hud, clock: &clock, sel: *sel, lens: *lens, hover: hover.0, works_cell: snap.0, problems: &problems,
        minimap: minimap.as_deref(), portrait: portrait.as_deref(), view_cell: crate::flow::world_cell(&s.height, orbit.target),
        cursor: cursor.0, journal: &notices.0, now,
    };
    let founded = s.is_running();
    for (e, p, mut m) in &mut hosts {
        if !SLOW_PANELS.contains(p) { continue; }
        if menu.0 || (!founded && flow.phase != crate::flow::Phase::Scouting) { ui::clear(&mut commands, e, &mut m); continue; }
        let tree = match p {
            Panel::Bottom if !founded => panels::settle_chip(&ctx),
            _ if !founded => Vec::new(),
            Panel::Strip => panels::strip(&ctx),
            Panel::Calendar => panels::calendar(&ctx),
            Panel::Right if ctx.hud.page().is_some() => Vec::new(),
            Panel::Right => panels::right(&ctx),
            Panel::Rail if !ctx.placing() => panels::rail(&ctx),
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
    mut commands: Commands, icons: Res<Icons>, sim: Res<Sim>, hud: Res<HudState>, clock: Res<SimClock>,
    hover: Res<HoverCell>, cursor: Res<CursorUi>, cams: Query<(&Camera, &GlobalTransform), With<MainCamera>>,
    scale: Res<UiScale>, menu: Res<MenuOpen>, flow: Res<Flow>, lens: Res<MapLens>, sel: Res<Selection>,
    (windows, pops, problems, time, wolf_drawn, orbit, beaver_drawn): (Query<&Window, With<bevy::window::PrimaryWindow>>, Res<Pops>, Res<LastProblems>, Res<Time>, Option<Res<crate::entities_render::WolfDrawn>>, Res<crate::camera::OrbitCamera>, Option<Res<crate::entities_render::BeaverDrawn>>),
    mut hosts: Query<(Entity, &Panel, &mut Mounted)>,
) {
    let _p = ::sim::prof::scope("game/refresh fast");
    let s = &sim.0;
    for (e, p, mut m) in &mut hosts {
        if !matches!(p, Panel::Cursor | Panel::Markers) { continue; }
        if menu.0 { ui::clear(&mut commands, e, &mut m); continue; }
        let tree = match p {
            Panel::Cursor if !s.is_running() => match (hover.0, cursor.0) {
                (Some(cell), Some(cur)) if flow.phase == crate::flow::Phase::Scouting => panels::settle_tag(s, &flow, cell, cur),
                _ => Vec::new(),
            },
            _ if !s.is_running() => Vec::new(),
            Panel::Cursor => {
                let ctx = panels::Ctx { s, hud: &hud, clock: &clock, sel: Selection::None, lens: *lens, hover: hover.0, works_cell: None, problems: &[], minimap: None, portrait: None, view_cell: (0.0, 0.0), cursor: cursor.0, journal: &[], now: time.elapsed_secs() };
                let mut tree = if matches!(hud.tool, Tool::Dam | Tool::Canal) {
                    let win = windows.single().map(|w| Vec2::new(w.width(), w.height()) / scale.0.max(0.01)).unwrap_or(Vec2::new(1280.0, 720.0));
                    let corner = hud.corners.last().and_then(|c| {
                        let (cam, gt) = cams.single().ok()?;
                        cam.world_to_viewport(gt, cell_world(&s.height, (c.0 as f32 + 0.5, c.1 as f32 + 0.5)) + Vec3::Y * 0.6).ok().map(|v| v / scale.0.max(0.01))
                    });
                    let tag = |at, pinned| if hud.tool == Tool::Canal { panels::canal_tag(&ctx, at, pinned, win) } else { panels::dam_tag(&ctx, at, pinned, win) };
                    match (hud.drag_from.is_some(), cursor.0, corner) {
                        (true, Some(at), _) => tag(at, false),
                        (false, _, Some(at)) => tag(at, true),
                        _ => Vec::new(),
                    }
                } else { panels::cursor_tag(&ctx) };
                if let (Tool::None, None, Some(a), Some(b)) = (hud.tool, hud.grab, hud.box_from, cursor.0) {
                    if a.distance(b) > 8.0 {
                        let (lo, hi) = (a.min(b), a.max(b));
                        let mut v = ui::VNode::new(Node { position_type: PositionType::Absolute, left: Val::Px(lo.x), top: Val::Px(lo.y), width: Val::Px(hi.x - lo.x), height: Val::Px(hi.y - lo.y), border: UiRect::all(Val::Px(1.5)), ..default() });
                        v.bg(ui::SELECT.with_alpha(0.08)).border(BorderColor::all(ui::SELECT.with_alpha(0.8)));
                        tree.push(v);
                    }
                }
                tree.extend(world::screen_pops(&pops.0, time.elapsed_secs()));
                tree.extend(panels::badges(&panels::Ctx { problems: &problems.0, ..ctx }));
                tree
            }
            _ => match cams.single() {
                Ok((cam, gt)) => {
                    let mut tree = world::water_labels(s, cam, gt, scale.0, hover.0, &orbit, *lens);
                    tree.extend(world::markers(s, beaver_drawn.as_deref(), cam, gt, scale.0, &hud, *sel, &problems.0, &pops.0, time.elapsed_secs(), wolf_drawn.as_ref().and_then(|w| w.at(sim::predators::nearest_seen(s)?.id))));
                    tree
                }
                Err(_) => Vec::new(),
            },
        };
        ui::reconcile(&mut commands, &icons, e, &mut m, tree, 0);
    }
}

fn update_drowned(s: &sim::SimState, hud: &mut HudState) {
    let Some(r) = hud.plan.as_ref().filter(|p| hud.tool == Tool::Dam && p.refusal.is_none()).and_then(|_| hud.dam_settled()) else {
        hud.drowned = None;
        return;
    };
    let key = hud.plan.as_ref().map(|p| p.line.clone()).unwrap_or_default();
    if hud.drowned.as_ref().is_some_and(|d| d.0 == key) { return; }
    let depth: HashMap<Cell, f32> = r.water.flooded.iter().copied().zip(r.water.depths.iter().copied()).collect();
    hud.drowned = Some((key, sim::dam_preview::drowned_by(s, |x, y| depth.get(&(x, y)).copied().unwrap_or(0.0))));
}

fn sync_lens(keys: Res<ButtonInput<KeyCode>>, lens: Res<MapLens>, hud: Res<HudState>) {
    let alt = keys.pressed(KeyCode::AltLeft) || keys.pressed(KeyCode::AltRight);
    let shown = match std::env::var("BC_LENS").as_deref() {
        Ok("water") => MapLens::Water,
        Ok("height") => MapLens::Elevation,
        Ok("deposits") => MapLens::Deposits,
        Ok("paths") => MapLens::Paths,
        _ if alt || matches!(hud.tool, Tool::Dam | Tool::Canal) => MapLens::Water,
        _ => *lens,
    };
    paths::SHOWN.store(shown == MapLens::Paths, std::sync::atomic::Ordering::Relaxed);
    crate::terrain_render::MAP_LENS.store(shown.code(), std::sync::atomic::Ordering::Relaxed);
}
