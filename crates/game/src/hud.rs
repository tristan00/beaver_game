#![allow(deprecated)]
mod actions;
mod input;
mod minimap;
mod colony;
mod elevation;
mod inspect;
mod panels;
mod ui;
mod words;
mod world;

use actions::{button_actions, commit_line, extend_dam, forest_kind, undo_last};
pub use actions::drawn_area;
use input::{pointer_over_ui, track_cursor, hover_cell, hotkeys, next_idle, world_input, box_select, frame_camera, follow_beaver, search_input};
use minimap::{MinimapImage, setup_minimap, update_minimap, minimap_click};

use crate::build_logic::{self, BuildOrder, DAM_STEP, MAX_DAM_HEIGHT, MIN_DAM_HEIGHT};
use crate::camera::MainCamera;
use crate::flow::{cell_world, Cell, Flow, Selection};
use crate::icons::{Icon, Icons};
use crate::sim_driver::{SimClock, Speed};
use crate::state::Sim;
use bevy::prelude::*;
use bevy::ui::FocusPolicy;
use sim::buildings::{BargeId, BuildingId, BuildingKind, Good};
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
    Canal,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum OrderMode { Move, Gather, Work }

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

#[derive(Resource)]
pub struct HudState {
    pub open_cat: Option<usize>,
    pub tool: Tool,
    pub drag_from: Option<Cell>,
    pub line: Vec<Cell>,
    pub dam_height: f32,
    pub dam_crest: Option<f32>,
    preview: Option<(Vec<Cell>, f32, Option<f32>, Option<DamPreview>)>,
    pub route_draft: Option<RouteDraft>,
    pub route_err: Option<(BargeId, String)>,
    pub msg: Option<(Msg, f32)>,
    idle_cursor: usize,
    pub colony: Option<ColonyTab>,
    pub search: Option<String>,
    undo: Vec<UndoItem>,
    pub details_open: bool,
    pub minimap_collapsed: bool,
    pub profile_collapsed: bool,
    pub selected: Vec<BeaverId>,
    pub follow: Option<BeaverId>,
    pub order_mode: Option<OrderMode>,
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

#[derive(Clone, Debug, PartialEq, Default)]
pub struct RouteDraft {
    pub barge: Option<BargeId>,
    pub from: Option<BuildingId>,
    pub to: Option<BuildingId>,
    pub goods: Vec<Good>,
}

impl RouteDraft {
    pub fn of(s: &sim::SimState, hud: &HudState, barge: BargeId) -> RouteDraft {
        if let Some(d) = hud.route_draft.as_ref().filter(|d| d.barge == Some(barge)) {
            return d.clone();
        }
        let svc = s.buildings.barge(barge).and_then(|b| b.service.clone());
        RouteDraft { barge: Some(barge), from: svc.as_ref().map(|v| v.from), to: svc.as_ref().map(|v| v.to), goods: svc.map(|v| v.goods).unwrap_or_default() }
    }
    pub fn ready(&self) -> bool {
        self.from.is_some() && self.to.is_some() && self.from != self.to && !self.goods.is_empty()
    }
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
            line: Vec::new(),
            dam_height: build_logic::DEFAULT_DAM_HEIGHT,
            dam_crest: None,
            preview: None,
            route_draft: None,
            route_err: None,
            msg: None,
            idle_cursor: 0,
            colony: None,
            search: None,
            undo: Vec::new(),
            details_open: false,
            minimap_collapsed: false,
            profile_collapsed: false,
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
        self.order_mode = None;
        self.open_cat = None;
        self.drag_from = None;
        self.line.clear();
        self.dam_crest = None;
        self.preview = None;
        self.area_edit = None;
    }
    fn step_height(&mut self, dir: f32) {
        let h = (self.dam_height / DAM_STEP).round() * DAM_STEP + DAM_STEP * dir.signum();
        self.dam_height = h.clamp(MIN_DAM_HEIGHT, MAX_DAM_HEIGHT);
        self.dam_crest = None;
    }
    fn say(&mut self, icon: Option<(Icon, Color)>, icon2: Option<(Icon, Color)>, text: impl Into<String>, now: f32) {
        let text = text.into();
        if sim::trace::enabled() { sim::trace::event(format!("FEEDBACK {text}")); }
        self.msg = Some((Msg { icon, icon2, text }, now));
    }
    fn refuse(&mut self, why: impl Into<String>, now: f32) {
        self.say(Some((Icon::Cross, BAD)), None, why, now);
    }
    pub fn dam_preview(&self) -> Option<&DamPreview> {
        self.preview.as_ref().and_then(|(_, _, _, p)| p.as_ref())
    }
    pub(crate) fn notify_paused(&mut self, now: f32) {
        self.say(Some((Icon::Pause, WARN)), Some((Icon::Urgent, BAD)), "Paused for an urgent alert. Space resumes.", now);
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
    Profile,
    Minimap,
    Lane,
    Colony,
    Cursor,
    Markers,
}

const SLOW_PANELS: [Panel; 9] = [Panel::Markers, Panel::Strip, Panel::Calendar, Panel::Rail, Panel::Minimap, Panel::Bottom, Panel::Profile, Panel::Colony, Panel::Right];

#[derive(Component, Clone, Debug, PartialEq)]
pub enum HudAction {
    Order(OrderMode),
    StopSelected,
    SelectFamily,
    Help,
    Fix(Fix, Option<Cell>),
    Focus(Cell),
    CycleIdle,
    Pause,
    Speed(Speed),
    OpenCat(usize),
    Pick(BuildingKind),
    CancelTool,
    DamHeight(f32),
    ExtendDam,
    RaiseDam(Cell),
    ConfirmLine,
    ClearLine,
    CloseInspector,
    SetLens(MapLens),
    Undo,
    OpenSearch,
    Colony(Option<ColonyTab>),
    ToggleDetails,
    ToggleMinimap,
    ToggleProfile,
    SelectBeaver(BeaverId),
    SelectBuilding(BuildingId),
    SelectBarge(BargeId),
    RouteFrom(BargeId, BuildingId),
    RouteTo(BargeId, BuildingId),
    RouteGood(BargeId, Good),
    SetRoute(BargeId),
    ClearRoute(BargeId),
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
    CutRule(BuildingId, sim::forestry::CutRule),
    PlantRule(BuildingId, sim::forestry::PlantRule),
}

pub struct HudPlugin;

#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct HudSet;

impl Plugin for HudPlugin {
    fn build(&self, app: &mut App) {
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
                    (pointer_over_ui, track_cursor, world::world_hover, hover_cell, hotkeys, search_input, world_input, box_select, frame_camera, follow_beaver).chain(),
                    (button_actions, minimap_click, sync_lens, update_minimap, refresh_panels, refresh_fast, ui::scroll_panels, ui::tip_logic, ui::tip_draw, world::draw_world_overlay).chain(),
                )
                    .chain()
                    .in_set(HudSet),
            )
            .add_systems(Update, elevation::update_contours.after(sync_lens).in_set(HudSet));
    }
}

fn spawn_hosts(mut commands: Commands) {
    for (i, p) in [Panel::Markers, Panel::Strip, Panel::Calendar, Panel::Rail, Panel::Minimap, Panel::Bottom, Panel::Profile, Panel::Colony, Panel::Right, Panel::Lane, Panel::Cursor].into_iter().enumerate() {
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
        let stale = hud.preview.as_ref().is_none_or(|(c, h, k, _)| *c != hud.line || *h != hud.dam_height || *k != hud.dam_crest);
        if stale {
            let p = build_logic::line_preview(s, &hud.line, hud.dam_height, hud.dam_crest);
            hud.preview = Some((hud.line.clone(), hud.dam_height, hud.dam_crest, p));
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
            Panel::Rail => if placing { Vec::new() } else { panels::rail(&ctx) },
            Panel::Bottom => panels::bottom(&ctx),
            Panel::Profile => panels::profile(&ctx),
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
                panels::cursor_tag(&ctx)
            }
            _ => match cams.single() {
                Ok((cam, gt)) => world::markers(s, cam, gt, scale.0, &hud, *sel),
                Err(_) => Vec::new(),
            },
        };
        ui::reconcile(&mut commands, &icons, e, &mut m, tree, 0);
    }
}

const SYNONYMS: [(&str, &[BuildingKind]); 17] = [
    ("food", &[BuildingKind::ForagerHut, BuildingKind::FoodCache]),
    ("eat", &[BuildingKind::ForagerHut, BuildingKind::FoodCache]),
    ("wood", &[BuildingKind::LoggingCamp]),
    ("log", &[BuildingKind::LoggingCamp]),
    ("stick", &[BuildingKind::LoggingCamp]),
    ("tree", &[BuildingKind::LoggingCamp]),
    ("mud", &[BuildingKind::MudPit]),
    ("clay", &[BuildingKind::MudPit]),
    ("stor", &[BuildingKind::Warehouse, BuildingKind::Storehouse, BuildingKind::FoodCache]),
    ("home", &[BuildingKind::Lodge, BuildingKind::Burrow]),
    ("house", &[BuildingKind::Lodge, BuildingKind::Burrow]),
    ("water", &[BuildingKind::Dam, BuildingKind::Canal, BuildingKind::WaterWheel, BuildingKind::Lock, BuildingKind::Dock]),
    ("power", &[BuildingKind::WaterWheel]),
    ("craft", &[BuildingKind::Workshop, BuildingKind::Mill]),
    ("rope", &[BuildingKind::Workshop]),
    ("tool", &[BuildingKind::Workshop, BuildingKind::Mill]),
    ("boat", &[BuildingKind::Dock, BuildingKind::Barge]),
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
    let forest_tool = matches!(hud.tool, Tool::Building(k) if forest_kind(k))
        || hud.area_edit.and_then(|b| sim.0.buildings.building(b)).is_some_and(|b| forest_kind(b.kind));
    let shown = if alt || matches!(hud.tool, Tool::Dam | Tool::Canal) {
        MapLens::Water
    } else if forest_tool {
        MapLens::Forest
    } else {
        *lens
    };
    crate::terrain_render::MAP_LENS.store(shown.code(), std::sync::atomic::Ordering::Relaxed);
}
