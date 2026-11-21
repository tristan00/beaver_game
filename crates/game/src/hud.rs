#![allow(deprecated)]
mod actions;
mod input;
mod minimap;
mod colony;
mod dam_panel;
mod elevation;
mod inspect;
mod panels;
mod ui;
mod words;
mod world;

use actions::{button_actions, commit_line, undo_last};
pub use actions::drawn_area;
use input::{pointer_over_ui, track_cursor, hover_cell, hotkeys, next_idle, world_input, box_select, frame_camera, follow_beaver, search_input};
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

pub(crate) use ui::{font, ACCENT, BAD, FAINT, GOOD, INK, MUTED, PANEL, TIMBER, WARN};

pub(crate) fn text(p: &mut ChildSpawnerCommands, s: impl Into<String>, size: f32, color: Color) {
    p.spawn((Text::new(s.into()), font(size, false), TextColor(color)));
}

pub(crate) fn bold(p: &mut ChildSpawnerCommands, s: impl Into<String>, size: f32, color: Color) {
    p.spawn((Text::new(s.into()), font(size, true), TextColor(color)));
}

pub(crate) fn row(gap: f32) -> Node {
    ui::row_node(gap)
}

const CAT_KEYS: [KeyCode; 6] = [KeyCode::KeyZ, KeyCode::KeyX, KeyCode::KeyC, KeyCode::KeyV, KeyCode::KeyB, KeyCode::KeyN];
pub(crate) const CAT_KEY_LABELS: [&str; 6] = ["Z", "X", "C", "V", "B", "N"];
const REFRESH_SECS: f32 = 0.1;
const MSG_SECS: f32 = 3.0;

#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub enum Tool {
    #[default]
    None,
    Building(BuildingKind),
    Dam,
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
    Land,
    Buildings,
    Log,
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct Msg {
    pub icon: Option<(Icon, Color)>,
    pub icon2: Option<(Icon, Color)>,
    pub text: String,
}

pub use crate::build_logic::LinePlan;

#[derive(Resource)]
pub struct HudState {
    pub open_cat: Option<usize>,
    pub tool: Tool,
    pub drag_from: Option<Cell>,
    /// Corners of the dam line being drawn; `line` is the cells through them.
    pub corners: Vec<Cell>,
    pub line: Vec<Cell>,
    /// What the drawn line would build, worked out once per change of line.
    pub plan: Option<LinePlan>,
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
    pub search: Option<String>,
    undo: Vec<UndoItem>,
    pub details_open: bool,
    pub minimap_collapsed: bool,
    pub selected: Vec<BeaverId>,
    pub follow: Option<BeaverId>,
    // Compatibility with the current menu, renderer and input recorder. This
    // HUD uses contextual orders and has no separate order or help screen.
    pub order_mode: Option<()>,
    pub help: bool,
    consume_left_release: bool,
    pub pop_sort: u8,
    pub pop_page: usize,
    pub area_edit: Option<BuildingId>,
    pub area_from: Option<Cell>,
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
    Goods,
    Forest,
    Elevation,
}

impl MapLens {
    pub fn code(self) -> u8 {
        match self {
            MapLens::World => 0,
            MapLens::Water => 1,
            MapLens::Goods => 2,
            MapLens::Forest => 3,
            MapLens::Elevation => 4,
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
            open_cat: None,
            tool: Tool::None,
            drag_from: None,
            corners: Vec::new(),
            line: Vec::new(),
            plan: None,
            extending: None,
            dam_cells: Vec::new(),
            dam_tab: DamTab::Status,
            dam_ghost: None,
            msg: None,
            idle_cursor: 0,
            colony: None,
            search: None,
            undo: Vec::new(),
            details_open: false,
            minimap_collapsed: false,
            selected: Vec::new(),
            follow: None,
            order_mode: None,
            help: false,
            consume_left_release: false,
            pop_sort: 5,
            pop_page: 0,
            area_edit: None,
            area_from: None,
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
        self.extending = None;
        self.area_edit = None;
    }
    fn finish_placement(&mut self) {
        self.set_tool(Tool::None);
        self.open_cat = None;
        self.search = None;
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
    pub(crate) fn notify_paused(&mut self, now: f32) {
        self.say(Some((Icon::Pause, WARN)), Some((Icon::Urgent, BAD)), "", now);
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
    Lane,
    Colony,
    Cursor,
    Markers,
}

const SLOW_PANELS: [Panel; 8] = [Panel::Markers, Panel::Strip, Panel::Calendar, Panel::Rail, Panel::Minimap, Panel::Bottom, Panel::Colony, Panel::Right];

#[derive(Component, Clone, Debug, PartialEq)]
pub enum HudAction {
    Fix(Fix, Option<Cell>),
    Focus(Cell),
    CycleIdle,
    Pause,
    Speed(Speed),
    OpenCat(usize),
    Pick(BuildingKind),
    CancelTool,
    GrowDam(BuildingId, sim::DamGrowth),
    DamMaintain(BuildingId, bool),
    DamCell(Cell),
    DamTab(DamTab),
    ConfirmLine,
    CloseInspector,
    SetLens(MapLens),
    Undo,
    Colony(Option<ColonyTab>),
    ToggleDetails,
    ToggleMinimap,
    SelectBeaver(BeaverId),
    SelectBuilding(BuildingId),
    Follow(BeaverId),
    FreeBeaver(BeaverId),
    AddWorker(BuildingId),
    RemoveWorker(BuildingId),
    RoleAdjust(RoleKind, i32),
    RoleWeight(RoleKind, f32),
    RoleUp(RoleKind),
    Priority(BuildingId, sim::work::Priority),
    Paused(BuildingId, bool),
    Unlock(BeaverId),
    RepairBuilding(BuildingId),
    ReinforceBuilding(BuildingId),
    CancelMaintenance(BuildingId),
    AutoStaff(bool),
    SortPop(u8),
    PopPage(i32),
    EditArea(BuildingId),
    Pin(Selection),
    Balance(BuildingId, bool),
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
        app.add_systems(Startup, load_ui_font);
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
            .init_resource::<world::HoverThing>()
            .init_resource::<world::HoverPoint>()
            .init_resource::<elevation::Contours>()
            .insert_resource(RefreshTimer(Timer::from_seconds(REFRESH_SECS, TimerMode::Repeating)))
            .add_systems(Startup, (spawn_hosts, ui::spawn_tip_hosts, setup_minimap))
            .add_systems(
                Update,
                (
                    (pointer_over_ui, track_cursor, world::world_hover, hover_cell, hotkeys, search_input, world_input, box_select, frame_camera, follow_beaver),
                    (button_actions, minimap_click, sync_lens, update_minimap, refresh_panels, refresh_fast, ui::tip_logic, ui::tip_draw, ui::lift_hover, world::draw_world_overlay),
                )
                    .chain()
                    .in_set(HudSet),
            )
            .add_systems(Update, elevation::update_contours.after(sync_lens).in_set(HudSet));
    }
}

fn spawn_hosts(mut commands: Commands) {
    for (i, p) in [Panel::Markers, Panel::Strip, Panel::Calendar, Panel::Rail, Panel::Minimap, Panel::Bottom, Panel::Colony, Panel::Right, Panel::Lane, Panel::Cursor].into_iter().enumerate() {
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
        match p.from {
            Some((_, role)) => t.push(TipLine::new(who, format!("from {}", role_kind_name(role).to_lowercase())).icon(role_kind_icon(role), MUTED)),
            None => t.push(TipLine::new(who, "Free").icon(Icon::Idle, INK)),
        };
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
    cursor: Res<CursorUi>,
    mut hosts: Query<(Entity, &Panel, &mut Mounted)>,
) {
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
    let hud: &HudState = &hud;
    let ctx = panels::Ctx {
        s,
        hud,
        clock: &clock,
        sel: *sel,
        lens: *lens,
        hover: hover.0,
        problems: &problems,
        minimap: minimap.as_ref().map(|m| &m.0),
        view_cell: crate::flow::world_cell(&s.height, orbit.target),
        log: &flow.log,
        history: &flow.history,
        cursor: cursor.0,
    };
    let founded = s.is_running();
    for (e, p, mut m) in &mut hosts {
        if !SLOW_PANELS.contains(p) && *p != Panel::Lane {
            continue;
        }
        if menu.0 || (!founded && flow.phase != crate::flow::Phase::Scouting) {
            ui::clear(&mut commands, e, &mut m);
            continue;
        }
        let placing = ctx.placing();
        let tree = match p {
            Panel::Bottom if !founded => panels::settle_chip(&ctx),
            Panel::Lane if !founded => panels::lane(&ctx),
            _ if !founded => Vec::new(),
            Panel::Strip => panels::strip(&ctx),
            Panel::Calendar => panels::calendar(&ctx),
            Panel::Right => panels::right(&ctx),
            Panel::Rail if !placing => panels::rail(&ctx),
            Panel::Bottom => panels::bottom(&ctx),
            Panel::Minimap => panels::minimap(&ctx),
            Panel::Lane => panels::lane(&ctx),
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
    windows: Query<&Window, With<bevy::window::PrimaryWindow>>,
    mut hosts: Query<(Entity, &Panel, &mut Mounted)>,
) {
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
                let ctx = panels::Ctx { s, hud: &hud, clock: &clock, sel: Selection::None, lens: *lens, hover: hover.0, problems: &[], minimap: None, view_cell: (0.0, 0.0), log: &flow.log, history: &flow.history, cursor: cursor.0 };
                if hud.tool == Tool::Dam {
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
                }
            }
            _ => match cams.single() {
                Ok((cam, gt)) => world::markers(s, cam, gt, scale.0, &hud, *sel),
                Err(_) => Vec::new(),
            },
        };
        ui::reconcile(&mut commands, &icons, e, &mut m, tree, 0);
    }
}

const SYNONYMS: [(&str, &[BuildingKind]); 6] = [
    ("food", &[BuildingKind::Warehouse]),
    ("stor", &[BuildingKind::Warehouse]),
    ("home", &[BuildingKind::Shelter]),
    ("house", &[BuildingKind::Shelter]),
    ("lodge", &[BuildingKind::Shelter]),
    ("water", &[BuildingKind::Dam]),
];

pub(crate) fn search_matches(s: &sim::SimState, q: &str) -> Vec<build_logic::MenuEntry> {
    let q = q.trim().to_lowercase();
    if q.is_empty() {
        return Vec::new();
    }
    let menu = build_logic::menu(s);
    let mut out: Vec<build_logic::MenuEntry> = menu.iter().filter(|e| e.kind.spec().name.to_lowercase().contains(&q)).cloned().collect();
    for (word, kinds) in SYNONYMS {
        if word.starts_with(&q) || q.starts_with(word) {
            for k in kinds {
                if !out.iter().any(|e| e.kind == *k) {
                    if let Some(e) = menu.iter().find(|e| e.kind == *k) {
                        out.push(e.clone());
                    }
                }
            }
        }
    }
    out.sort_by_key(|e| !e.unlocked);
    out
}

fn sync_lens(keys: Res<ButtonInput<KeyCode>>, lens: Res<MapLens>, hud: Res<HudState>, sim: Res<Sim>) {
    let alt = keys.pressed(KeyCode::AltLeft) || keys.pressed(KeyCode::AltRight);
    let _ = &sim;
    let shown = if alt || hud.tool == Tool::Dam { MapLens::Water } else { *lens };
    crate::terrain_render::MAP_LENS.store(shown.code(), std::sync::atomic::Ordering::Relaxed);
}
