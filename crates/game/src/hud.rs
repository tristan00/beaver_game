#![allow(deprecated)]
mod actions;
mod input;
mod minimap;
mod colony;
mod canal_panel;
mod dam_panel;
mod elevation;
mod inspect;
mod panels;
mod ui;
mod words;
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
use sim::jobs::RoleKind;
use ui::{Mounted, Tag, TagComp, Tip, TipLine, Tips};
use words::*;

pub(crate) use ui::{font, ACCENT, BAD, FAINT, GOOD, INK, MUTED, PANEL, TIMBER, WATER};

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
const MSG_SECS: f32 = 3.0;

#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub enum Tool {
    #[default]
    None,
    Building(BuildingKind),
    Dam,
    /// Draw a canal: from water, over land.
    Canal,
    /// Send beavers to gather (food or clay) or cut trees: click a spot or drag an area.
    Harvest(Harvest),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Harvest {
    Gather,
    Cut,
}

/// The dam panel's two pages.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum DamTab {
    #[default]
    Status,
    History,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ColonyTab {
    Population,
    Jobs,
    Goods,
    Water,
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct Msg {
    pub icon: Option<(Icon, Color)>,
    pub icon2: Option<(Icon, Color)>,
    pub text: String,
}

pub use crate::build_logic::LinePlan;
pub(crate) use world::{HoverPoint, HoverThing};

#[derive(Resource)]
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
    pub settled: Option<(Vec<Cell>, sim::dam_preview::Settled)>,
    /// What the drawn canal would dig, worked out once per change of line.
    pub canal_plan: Option<(Vec<Cell>, Result<sim::canals::Plan, String>)>,
    /// Set while a dam is being lengthened from one of its ends.
    pub extending: Option<BuildingId>,
    /// Cells picked in the dam panel's cross-section; raises apply to them only.
    pub dam_cells: Vec<Cell>,
    pub dam_tab: DamTab,
    /// The selected dam's suggested growth, ghosted on the map: (dam, day, cells).
    pub dam_ghost: Option<(BuildingId, f64, Vec<Cell>)>,
    pub msg: Option<(Msg, f32)>,
    idle_cursor: usize,
    pub colony: Option<ColonyTab>,
    undo: Vec<UndoItem>,
    pub selected: Vec<BeaverId>,
    pub follow: Option<BeaverId>,
    // Compatibility with the current menu, renderer and input recorder. This
    // HUD uses contextual orders and has no separate order or help screen.
    pub order_mode: Option<()>,
    pub help: bool,
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
    Underwater,
}

impl MapLens {
    pub fn code(self) -> u8 {
        match self {
            MapLens::World => 0,
            MapLens::Water => 1,
            MapLens::Forest => 3,
            MapLens::Elevation => 4,
            MapLens::Deposits => crate::terrain_render::DEPOSIT_LENS,
            MapLens::Underwater => crate::terrain_render::UNDERWATER_LENS,
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
    if let Some(mut s) = world.get_resource_mut::<Selection>() {
        *s = Selection::None;
    }
}

impl Default for HudState {
    fn default() -> Self {
        HudState {
            open_badge: None,
            tool: Tool::None,
            drag_from: None,
            grab: None,
            corners: Vec::new(),
            line: Vec::new(),
            plan: None,
            settled: None,
            canal_plan: None,
            extending: None,
            dam_cells: Vec::new(),
            dam_tab: DamTab::Status,
            dam_ghost: None,
            msg: None,
            idle_cursor: 0,
            colony: None,
            undo: Vec::new(),
            selected: Vec::new(),
            follow: None,
            order_mode: None,
            help: false,
            consume_left_release: false,
            pinned: Vec::new(),
            box_from: None,
            box_done: None,
            frame_req: None,
        }
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
    pub fn dam_settled(&self) -> Option<&sim::dam_preview::Settled> {
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
    pub fn undo_is_empty(&self) -> bool {
        self.undo.is_empty()
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

/// News for the top of the screen: a wolf seen, a beaver dead, a game saved.
#[derive(Clone, Debug)]
pub struct Notice {
    pub id: u64,
    pub icons: Vec<(Icon, Color)>,
    pub text: String,
    pub born: f32,
}

#[derive(Resource, Default)]
pub struct Notices(pub Vec<Notice>, u64);

impl Notice {
    /// The badge's key, as `HudState::open_badge` holds it.
    pub fn key(&self) -> String {
        format!("notice:{}", self.id)
    }
}

/// How long a notice stays up, and how many show at once.
pub const NOTICE_SECS: f32 = 7.0;
pub const NOTICES_SHOWN: usize = 6;

impl Notices {
    pub fn push(&mut self, icons: Vec<(Icon, Color)>, text: impl Into<String>, now: f32) {
        let text = text.into();
        sim::trace::event(format!("NOTICE icons={:?} text={text:?}", icons.iter().map(|(i, _)| format!("{i:?}")).collect::<Vec<_>>()));
        self.1 += 1;
        let id = self.1;
        self.0.push(Notice { id, icons, text, born: now });
    }
}

/// A wolf comes into sight: say so once per visit.
fn notice_wolf(time: Res<Time>, sim: Res<Sim>, mut notices: ResMut<Notices>, mut seen: Local<bool>) {
    let spotted = sim.0.predators.wolf.is_some() && sim.0.predators.spotted;
    if spotted && !*seen {
        notices.push(vec![(Icon::Wolf, BAD)], "Wolf sighted", time.elapsed_secs());
    }
    *seen = spotted;
}

/// News fades after a while, unless its badge is open.
fn expire_notices(time: Res<Time>, hud: Res<HudState>, mut notices: ResMut<Notices>) {
    let now = time.elapsed_secs();
    let open = hud.open_badge.clone();
    notices.0.retain(|n| now - n.born < NOTICE_SECS || open.as_deref() == Some(n.key().as_str()));
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
    Cursor,
    Markers,
}

const SLOW_PANELS: [Panel; 8] = [Panel::Markers, Panel::Strip, Panel::Calendar, Panel::Rail, Panel::Minimap, Panel::Bottom, Panel::Colony, Panel::Right];

#[derive(Component, Clone, Debug, PartialEq)]
pub enum HudAction {
    Fix(Fix, Option<Cell>),
    CycleIdle,
    Pause,
    Speed(Speed),
    Harvest(Harvest),
    Pick(BuildingKind),
    CancelTool,
    GrowDam(BuildingId, sim::DamGrowth),
    DamMaintain(BuildingId, bool),
    /// Make the picked dam cell a gate.
    AddDamGate(BuildingId),
    SetGate(BuildingId, Cell, sim::GateSetting),
    GrowCanal(BuildingId, sim::CanalGrowth),
    CanalMaintain(BuildingId, bool),
    DamCell(Cell),
    DamTab(DamTab),
    ConfirmLine,
    CloseInspector,
    SetLens(MapLens),
    Undo,
    Colony(Option<ColonyTab>),
    SelectBeaver(BeaverId),
    SelectBuilding(BuildingId),
    Follow(BeaverId),
    FreeBeaver(BeaverId),
    AddWorker(BuildingId),
    /// Open or close a top-centre badge to read it.
    Badge(String),
    /// Have haulers fill this store to the brim, or stop.
    FillStore(BuildingId, bool),
    /// A logistics building's haulers balance the stores, or stop.
    Balance(BuildingId, bool),
    RemoveWorker(BuildingId),
    RoleAdjust(RoleKind, i32),
    Paused(BuildingId, bool),
    RepairBuilding(BuildingId),
    ReinforceBuilding(BuildingId),
    PebbleBuilding(BuildingId),
    SlotOff(BuildingId, usize, bool),
    CancelMaintenance(BuildingId),
    Pin(Selection),
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
        app.add_systems(Startup, load_ui_font).init_resource::<Notices>();
        if app.world().resource::<crate::cli::Cli>().screenshot.is_some() && std::env::var_os("BC_HUD").is_none() {
            return;
        }
        app.init_resource::<HudState>()
            .init_resource::<HoverCell>()
            .init_resource::<PointerOverUi>()
            .init_resource::<CursorUi>()
            .init_resource::<MapLens>()
            .init_resource::<TextCapture>()
            .init_resource::<MenuOpen>()
            .init_resource::<Tips>()
            .init_resource::<Pops>()
            .add_systems(Update, (notice_wolf, expire_notices))
            .init_resource::<LastProblems>()
            .init_resource::<world::HoverThing>()
            .init_resource::<world::HoverPoint>()
            .init_resource::<elevation::Contours>()
            .init_resource::<DamSettle>()
            .insert_resource(RefreshTimer(Timer::from_seconds(REFRESH_SECS, TimerMode::Repeating)))
            .add_systems(Startup, (spawn_hosts, ui::spawn_tip_hosts, setup_minimap))
            .add_systems(
                Update,
                (
                    (pointer_over_ui, track_cursor, world::world_hover, hover_cell, hotkeys, world_input, box_select, frame_camera, follow_beaver),
                    (button_actions, collect_pops, minimap_click, sync_lens, update_minimap, refresh_panels, refresh_fast, ui::tip_logic, ui::tip_draw, ui::lift_hover, world::draw_world_overlay),
                )
                    .chain()
                    .in_set(HudSet),
            )
            .add_systems(Update, elevation::update_contours.after(sync_lens).in_set(HudSet))
            .add_systems(Update, settle_dam_line.after(refresh_panels).in_set(HudSet));
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
            hud.settled = result.map(|r| (line, r));
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
    for (i, p) in [Panel::Markers, Panel::Strip, Panel::Calendar, Panel::Rail, Panel::Minimap, Panel::Bottom, Panel::Colony, Panel::Right, Panel::Cursor].into_iter().enumerate() {
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

pub(crate) fn add_worker_tip(s: &sim::SimState, b: BuildingId) -> Tip {
    let mut t = Tip::name("Add a worker");
    if let Some(p) = sim::jobs::add_worker_preview(s, b) {
        let who = s.colony.beaver(p.who).map(|x| x.name.clone()).unwrap_or_default();
        t.push(TipLine::new(who, "Free").icon(Icon::Idle, INK));
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
    minimap: Option<Res<MinimapImage>>,
    menu: Res<MenuOpen>,
    flow: Res<Flow>,
    (cursor, mut last): (Res<CursorUi>, ResMut<LastProblems>),
    mut hosts: Query<(Entity, &Panel, &mut Mounted)>,
) {
    let _p = ::sim::prof::scope("game/refresh panels");
    let due = timer.0.tick(time.delta()).just_finished();
    let changed = hud.is_changed() || sel.is_changed() || lens.is_changed() || menu.is_changed() || clock.is_changed() || (hover.is_changed() && hud.tool != Tool::None);
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
        problems: &problems,
        minimap: minimap.as_deref(),
        view_cell: crate::flow::world_cell(&s.height, orbit.target),
        cursor: cursor.0,
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
            Panel::Right => panels::right(&ctx),
            Panel::Rail if !placing => panels::rail(&ctx),
            Panel::Bottom => panels::bottom(&ctx),
            Panel::Minimap => panels::minimap(&ctx),
            Panel::Colony => colony::screen(&ctx),
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
    (windows, pops, problems, time, notices): (Query<&Window, With<bevy::window::PrimaryWindow>>, Res<Pops>, Res<LastProblems>, Res<Time>, Res<Notices>),
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
                let ctx = panels::Ctx { s, hud: &hud, clock: &clock, sel: Selection::None, lens: *lens, hover: hover.0, problems: &[], minimap: None, view_cell: (0.0, 0.0), cursor: cursor.0 };
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
                tree.extend(world::screen_pops(&pops.0, time.elapsed_secs()));
                let bctx = panels::Ctx { problems: &problems.0, ..ctx };
                tree.extend(panels::badges(&bctx, &notices.0, time.elapsed_secs()));
                tree
            }
            _ => match cams.single() {
                Ok((cam, gt)) => world::markers(s, cam, gt, scale.0, &hud, *sel, &problems.0, &pops.0, time.elapsed_secs()),
                Err(_) => Vec::new(),
            },
        };
        ui::reconcile(&mut commands, &icons, e, &mut m, tree, 0);
    }
}

fn sync_lens(keys: Res<ButtonInput<KeyCode>>, lens: Res<MapLens>, hud: Res<HudState>, sim: Res<Sim>) {
    let alt = keys.pressed(KeyCode::AltLeft) || keys.pressed(KeyCode::AltRight);
    let _ = &sim;
    let shown = if alt || matches!(hud.tool, Tool::Dam | Tool::Canal) { MapLens::Water } else { *lens };
    // `BC_LENS=water|height|deposits|underwater` holds that lens, for screenshots.
    let shown = match std::env::var("BC_LENS").as_deref() {
        Ok("water") => MapLens::Water,
        Ok("height") => MapLens::Elevation,
        Ok("deposits") => MapLens::Deposits,
        Ok("underwater") => MapLens::Underwater,
        _ => shown,
    };
    crate::terrain_render::MAP_LENS.store(shown.code(), std::sync::atomic::Ordering::Relaxed);
}
