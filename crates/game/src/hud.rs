//! The in-game interface (spec `docs/interface_spec.md`).
//!
//! Screen zones (F3): goods strip top left, calendar top right, a right column that shows alerts
//! at rest, the inspector on a selection and the line-tool card while drawing; the view rail on the
//! left edge; the build bar bottom centre; the minimap bottom left; messages in a lane under the
//! strip; the Colony screen docked over the left three quarters. While a tool is active (build
//! mode) the rail, minimap, alerts and building list hide and the bar collapses to one chip.
//!
//! Panels are described as trees each refresh and reconciled in place (`ui.rs`), so hover and
//! clicks survive updates. Words follow Appendix A: icons first, the fewest words possible.
#![allow(deprecated)]
mod colony;
mod inspect;
mod panels;
mod ui;
mod words;
mod world;

use crate::build_logic::{self, BuildOrder, Group, Pending, MAX_DAM_HEIGHT, MIN_DAM_HEIGHT};
use crate::camera::MainCamera;
use crate::flow::{cell_world, Cell, Flow, Selection};
use crate::icons::{Icon, Icons};
use crate::sim_driver::{SimClock, Speed};
use crate::state::Sim;
use bevy::asset::RenderAssetUsages;
use bevy::input::keyboard::{Key, KeyboardInput};
use bevy::input::mouse::{AccumulatedMouseScroll, MouseScrollUnit};
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy::ui::{FocusPolicy, RelativeCursorPosition};
use bevy::window::PrimaryWindow;
use sim::buildings::{BuildingId, BuildingKind, Status};
use sim::colony::BeaverId;
use sim::dam_preview::DamPreview;
use sim::hud::Fix;
use sim::jobs::RoleKind;
use ui::{Mounted, Tag, TagComp, Tip, TipLine, Tips};
use words::*;

pub(crate) use ui::{font, ACCENT, BAD, FAINT, GOOD, INK, MUTED, PANEL, TIMBER, WARN};

/// Spawn helpers for the menus, which still build their few nodes directly.
pub(crate) fn text(p: &mut ChildSpawnerCommands, s: impl Into<String>, size: f32, color: Color) {
    p.spawn((Text::new(s.into()), font(size, false), TextColor(color)));
}

pub(crate) fn bold(p: &mut ChildSpawnerCommands, s: impl Into<String>, size: f32, color: Color) {
    p.spawn((Text::new(s.into()), font(size, true), TextColor(color)));
}

pub(crate) fn row(gap: f32) -> Node {
    ui::row_node(gap)
}

/// Category hotkeys, laid out like the bar itself.
const CAT_KEYS: [KeyCode; 6] = [KeyCode::KeyZ, KeyCode::KeyX, KeyCode::KeyC, KeyCode::KeyV, KeyCode::KeyB, KeyCode::KeyN];
pub(crate) const CAT_KEY_LABELS: [&str; 6] = ["Z", "X", "C", "V", "B", "N"];
/// Slow panels refresh this often (and at once on any interface change); cursor-bound and
/// world-anchored ones every frame.
const REFRESH_SECS: f32 = 0.1;
const MSG_SECS: f32 = 3.0;

// ───────────────────────────── state ─────────────────────────────

#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub enum Tool {
    #[default]
    None,
    Building(BuildingKind),
    Dam,
    Canal,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ColonyTab {
    Population,
    Jobs,
    Goods,
    Water,
    Land,
    Research,
    Log,
}

/// A message in the lane: up to two icons and at most two words (A8).
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
    /// Line tools: the drag start and the drawn cells.
    pub drag_from: Option<Cell>,
    pub line: Vec<Cell>,
    pub dam_height: f32,
    preview: Option<(Vec<Cell>, f32, Option<DamPreview>)>,
    pub msg: Option<(Msg, f32)>,
    idle_cursor: usize,
    /// The Colony screen and its tab.
    pub colony: Option<ColonyTab>,
    /// The building search box, with what has been typed.
    pub search: Option<String>,
    /// Placements that can still be taken back, newest last.
    undo: Vec<UndoItem>,
    /// The inspector's details expander.
    pub details_open: bool,
    pub minimap_collapsed: bool,
    pub profile_collapsed: bool,
    /// Beavers selected for direct orders (W4).
    pub selected: Vec<BeaverId>,
    pub follow: Option<BeaverId>,
    /// Population table sort column.
    pub pop_sort: u8,
    /// Drawing a new work area for this building, and where the drag began.
    pub area_edit: Option<BuildingId>,
    pub area_from: Option<Cell>,
    /// Inspectors kept open while something else is selected (at most three).
    pub pinned: Vec<Selection>,
    /// A box selection being dragged (screen position where it began).
    box_from: Option<Vec2>,
    box_done: Option<(Vec2, Vec2)>,
    /// A world point the camera should bring into the work rect (F3.3).
    frame_req: Option<Vec3>,
}

/// Something the player placed that undo can take back while it is unfinished.
#[derive(Clone, Copy, Debug, PartialEq)]
enum UndoItem {
    Site(BuildingId),
    Canal(sim::colony::OrderId),
}

/// The map view the player chose. Holding Alt peeks at the water lens, and the dam and canal
/// tools switch it on while they are in use.
#[derive(Resource, Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum MapLens {
    #[default]
    World,
    Water,
    Goods,
}

impl MapLens {
    pub fn code(self) -> u8 {
        match self {
            MapLens::World => 0,
            MapLens::Water => 1,
            MapLens::Goods => 2,
        }
    }
}

/// True while a text box has the keyboard (camera, speed and build hotkeys stand down).
#[derive(Resource, Default)]
pub struct TextCapture(pub bool);

/// True while the start, pause or settings menu covers the game: the HUD hides and its keys and
/// world clicks stand down (`menu.rs` sets it).
#[derive(Resource, Default)]
pub struct MenuOpen(pub bool);

/// Forgets the HUD's per-game state (tool, undo list, open panels) when a new game starts.
pub fn reset_for_new_game(world: &mut World) {
    world.insert_resource(HudState::default());
    world.insert_resource(PendingWorks::default());
    world.insert_resource(MapLens::default());
    if let Some(mut s) = world.get_resource_mut::<Selection>() {
        *s = Selection::None;
    }
}

#[derive(Resource)]
struct MinimapImage(Handle<Image>);

impl Default for HudState {
    fn default() -> Self {
        HudState {
            open_cat: None,
            tool: Tool::None,
            drag_from: None,
            line: Vec::new(),
            dam_height: build_logic::DEFAULT_DAM_HEIGHT,
            preview: None,
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
            pop_sort: 0,
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
    fn set_tool(&mut self, t: Tool) {
        self.tool = t;
        self.drag_from = None;
        self.line.clear();
        self.preview = None;
        self.area_edit = None;
    }
    fn say(&mut self, icon: Option<(Icon, Color)>, icon2: Option<(Icon, Color)>, text: impl Into<String>, now: f32) {
        self.msg = Some((Msg { icon, icon2, text: text.into() }, now));
    }
    /// A refusal in the lane: ✕ and the ≤ 2-word reason.
    fn refuse(&mut self, why: impl Into<String>, now: f32) {
        self.say(Some((Icon::Cross, BAD)), None, why, now);
    }
    pub fn dam_preview(&self) -> Option<&DamPreview> {
        self.preview.as_ref().and_then(|(_, _, p)| p.as_ref())
    }
    /// The auto-pause notice: a pause icon in the lane (the alert itself says why).
    pub(crate) fn notify_paused(&mut self, now: f32) {
        self.say(Some((Icon::Pause, WARN)), Some((Icon::Urgent, BAD)), "", now);
    }
    pub fn undo_is_empty(&self) -> bool {
        self.undo.is_empty()
    }
}

/// Cell under the cursor, `None` over the interface or off the map.
#[derive(Resource, Default)]
pub struct HoverCell(pub Option<Cell>);

/// Whether the pointer is over any interface element (world clicks are ignored then).
#[derive(Resource, Default)]
pub struct PointerOverUi(pub bool);

/// The cursor in interface pixels.
#[derive(Resource, Default)]
struct CursorUi(Option<Vec2>);

#[derive(Resource, Default)]
struct PendingWorks(Pending);

#[derive(Resource)]
struct RefreshTimer(Timer);

/// Marks every node that should swallow the pointer.
#[derive(Component)]
pub struct Blocks;

/// A full-screen host whose children are one panel's tree.
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
    /// A problem's fix, with the problem's place (for Focus fixes).
    Fix(Fix, Option<Cell>),
    Focus(Cell),
    CycleIdle,
    Pause,
    Speed(Speed),
    OpenCat(usize),
    Pick(BuildingKind),
    CancelTool,
    DamHeight(f32),
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
    Follow(BeaverId),
    FreeBeaver(BeaverId),
    AddWorker(BuildingId),
    RemoveWorker(BuildingId),
    RoleAdjust(RoleKind, i32),
    RoleUp(RoleKind),
    Priority(BuildingId, sim::work::Priority),
    Paused(BuildingId, bool),
    Unlock(BeaverId),
    AutoStaff(bool),
    SortPop(u8),
    EditArea(BuildingId),
    Pin(Selection),
    Recipe(BuildingId, Option<sim::buildings::Recipe>),
}

pub struct HudPlugin;

/// The HUD's per-frame systems (menus run before them).
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct HudSet;

impl Plugin for HudPlugin {
    fn build(&self, app: &mut App) {
        if app.world().resource::<crate::cli::Cli>().screenshot.is_some() && std::env::var_os("BC_HUD").is_none() {
            // Screenshots of the world stay clean unless asked for (`BC_HUD=1`).
            return;
        }
        app.init_resource::<HudState>()
            .init_resource::<HoverCell>()
            .init_resource::<PointerOverUi>()
            .init_resource::<CursorUi>()
            .init_resource::<PendingWorks>()
            .init_resource::<MapLens>()
            .init_resource::<TextCapture>()
            .init_resource::<MenuOpen>()
            .init_resource::<Tips>()
            .init_resource::<world::HoverThing>()
            .init_resource::<world::HoverPoint>()
            .insert_resource(RefreshTimer(Timer::from_seconds(REFRESH_SECS, TimerMode::Repeating)))
            .add_systems(Startup, (spawn_hosts, ui::spawn_tip_hosts, setup_minimap))
            .add_systems(
                Update,
                (
                    (pointer_over_ui, track_cursor, hover_cell, world::world_hover, hotkeys, search_input, world_input, box_select, frame_camera, follow_beaver),
                    (button_actions, minimap_click, poll_pending_works, sync_lens, update_minimap, refresh_panels, refresh_fast, ui::tip_logic, ui::tip_draw, world::draw_world_overlay),
                )
                    .chain()
                    .in_set(HudSet),
            )
            .add_systems(Update, demo.before(pointer_over_ui));
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

// ───────────────────────────── input ─────────────────────────────

fn pointer_over_ui(q: Query<&Interaction, With<Blocks>>, mut over: ResMut<PointerOverUi>) {
    let v = q.iter().any(|i| *i != Interaction::None);
    if over.0 != v {
        over.0 = v;
    }
}

fn track_cursor(windows: Query<&Window, With<PrimaryWindow>>, scale: Res<UiScale>, mut c: ResMut<CursorUi>) {
    let p = windows.single().ok().and_then(|w| w.cursor_position()).map(|p| p / scale.0.max(0.01));
    if c.0 != p {
        c.0 = p;
    }
}

fn hover_cell(point: Res<world::HoverPoint>, mut hover: ResMut<HoverCell>) {
    let cell = point.0.map(|p| p.0);
    if hover.0 != cell {
        hover.0 = cell;
    }
}

#[allow(clippy::too_many_arguments)]
fn hotkeys(
    keys: Res<ButtonInput<KeyCode>>,
    sim: Res<Sim>,
    typing: Res<TextCapture>,
    mut hud: ResMut<HudState>,
    mut sel: ResMut<Selection>,
    mut lens: ResMut<MapLens>,
    mut flow: ResMut<Flow>,
    menu: Res<MenuOpen>,
) {
    if typing.0 || menu.0 {
        return; // the search box or a menu has the keyboard
    }
    let ctrl = keys.pressed(KeyCode::ControlLeft) || keys.pressed(KeyCode::ControlRight);
    for (k, l) in [(KeyCode::F1, MapLens::World), (KeyCode::F2, MapLens::Water), (KeyCode::F3, MapLens::Goods)] {
        if keys.just_pressed(k) {
            *lens = l;
        }
    }
    if keys.just_pressed(KeyCode::Tab) {
        hud.colony = if hud.colony.is_some() { None } else { Some(ColonyTab::Population) };
    }
    if keys.just_pressed(KeyCode::KeyJ) {
        hud.colony = if hud.colony == Some(ColonyTab::Jobs) { None } else { Some(ColonyTab::Jobs) };
    }
    if keys.just_pressed(KeyCode::KeyL) {
        hud.colony = if hud.colony == Some(ColonyTab::Goods) { None } else { Some(ColonyTab::Goods) };
    }
    if keys.just_pressed(KeyCode::KeyI) {
        next_idle(&sim.0, &mut hud, &mut sel, &mut flow);
    }
    if keys.just_pressed(KeyCode::KeyP) {
        hud.profile_collapsed = !hud.profile_collapsed;
    }
    if keys.just_pressed(KeyCode::Slash) {
        hud.search = Some(String::new());
        return;
    }
    if keys.just_pressed(KeyCode::Escape) {
        if hud.area_edit.is_some() {
            hud.area_edit = None;
            hud.area_from = None;
        } else if hud.tool != Tool::None {
            hud.set_tool(Tool::None);
        } else if hud.colony.is_some() {
            hud.colony = None;
        } else if hud.open_cat.is_some() {
            hud.open_cat = None;
        } else if *sel != Selection::None || !hud.selected.is_empty() {
            *sel = Selection::None;
            hud.selected.clear();
            hud.follow = None;
        }
    }
    let n = panels::categories(&sim.0).len();
    for (i, k) in CAT_KEYS.iter().enumerate().take(n) {
        if !ctrl && keys.just_pressed(*k) {
            hud.open_cat = if hud.open_cat == Some(i) { None } else { Some(i) };
            hud.search = None;
        }
    }
    if matches!(hud.tool, Tool::Dam) {
        for (k, d) in [(KeyCode::BracketRight, 0.25), (KeyCode::BracketLeft, -0.25)] {
            if keys.just_pressed(k) {
                hud.dam_height = (hud.dam_height + d).clamp(MIN_DAM_HEIGHT, MAX_DAM_HEIGHT);
            }
        }
    }
}

fn next_idle(s: &sim::SimState, hud: &mut HudState, sel: &mut Selection, flow: &mut Flow) {
    let idle = sim::hud::idle_beavers(s);
    if idle.is_empty() {
        return;
    }
    let i = hud.idle_cursor % idle.len();
    hud.idle_cursor = i + 1;
    if let Some(b) = s.colony.beaver(idle[i]) {
        flow.focus = Some(cell_world(&s.height, b.pos));
        *sel = Selection::Beaver(b.id);
        hud.selected = vec![b.id];
    }
}

#[allow(clippy::too_many_arguments)]
fn world_input(
    mouse: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
    scroll: Res<AccumulatedMouseScroll>,
    time: Res<Time>,
    hover: Res<HoverCell>,
    over: Res<PointerOverUi>,
    thing: Res<world::HoverThing>,
    point: Res<world::HoverPoint>,
    cursor: Res<CursorUi>,
    mut sim: ResMut<Sim>,
    mut hud: ResMut<HudState>,
    mut sel: ResMut<Selection>,
    mut pending: ResMut<PendingWorks>,
    menu: Res<MenuOpen>,
    mut flow: ResMut<Flow>,
) {
    if menu.0 {
        return;
    }
    let now = time.elapsed_secs();
    // Before the colony exists the only verb is choosing where the family settles.
    if !sim.0.colony.is_founded() {
        if flow.phase == crate::flow::Phase::Scouting && !over.0 && mouse.just_pressed(MouseButton::Left) {
            if let Some(cell) = hover.0 {
                match crate::flow::can_settle(&sim.0.world, &sim.0.water.depth, &flow.fog, cell) {
                    Ok(()) => {
                        if let Err(e) = crate::flow::settle(&mut sim.0, &mut flow, cell) {
                            hud.refuse(sim_error(&e), now);
                        }
                    }
                    Err(e) => hud.refuse(settle_error(e), now),
                }
            }
        }
        return;
    }
    // Ctrl + wheel sets the dam crest (the plain wheel keeps zooming).
    if matches!(hud.tool, Tool::Dam) && keys.pressed(KeyCode::ControlLeft) && scroll.delta.y != 0.0 {
        let lines = match scroll.unit {
            MouseScrollUnit::Line => scroll.delta.y,
            MouseScrollUnit::Pixel => scroll.delta.y / 40.0,
        };
        hud.dam_height = (hud.dam_height + 0.25 * lines.signum()).clamp(MIN_DAM_HEIGHT, MAX_DAM_HEIGHT);
    }
    let ctrl = keys.pressed(KeyCode::ControlLeft) || keys.pressed(KeyCode::ControlRight);
    let shift = keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight);
    if ctrl && keys.just_pressed(KeyCode::KeyZ) {
        undo_last(&mut sim.0, &mut hud, &mut pending.0, now);
        return;
    }
    if keys.just_pressed(KeyCode::Enter) && matches!(hud.tool, Tool::Dam | Tool::Canal) && hud.search.is_none() {
        commit_line(&mut sim.0, &mut hud, &mut pending.0, now);
        return;
    }
    if over.0 {
        return;
    }
    // Right click: cancel a tool, or order the selected beavers (W4).
    if mouse.just_pressed(MouseButton::Right) {
        if hud.tool != Tool::None || hud.area_edit.is_some() {
            hud.set_tool(Tool::None);
            return;
        }
        let who: Vec<BeaverId> = if hud.selected.is_empty() {
            match *sel {
                Selection::Beaver(b) => vec![b],
                _ => Vec::new(),
            }
        } else {
            hud.selected.clone()
        };
        if !who.is_empty() {
            if let Some(t) = thing.0 {
                order(&mut sim.0, &mut hud, &who, t, shift, now);
            }
        }
        return;
    }
    let Some(cell) = hover.0 else { return };
    // Drawing a work area for a gathering building.
    if let Some(b) = hud.area_edit {
        if mouse.just_pressed(MouseButton::Left) {
            hud.area_from = Some(cell);
        } else if mouse.just_released(MouseButton::Left) {
            if let Some(a) = hud.area_from.take() {
                let area = sim::work::Area { min: (a.0.min(cell.0), a.1.min(cell.1)), max: (a.0.max(cell.0), a.1.max(cell.1)) };
                if let Err(e) = sim.0.apply(sim::Command::SetWorkArea { building: b, area }) {
                    hud.refuse(sim_error(&e.to_string()), now);
                }
                hud.area_edit = None;
            }
        }
        return;
    }
    match hud.tool {
        Tool::None => {
            if mouse.just_pressed(MouseButton::Left) {
                hud.box_from = cursor.0;
            }
            if mouse.just_released(MouseButton::Left) {
                let from = hud.box_from.take();
                if let (Some(a), Some(b)) = (from, cursor.0) {
                    if a.distance(b) > 8.0 {
                        hud.box_done = Some((a, b));
                        return;
                    }
                }
                let t = thing.0.unwrap_or(world::Thing::Cell(cell));
                match t {
                    world::Thing::Beaver(id) if shift => {
                        if let Some(i) = hud.selected.iter().position(|x| *x == id) {
                            hud.selected.remove(i);
                        } else {
                            hud.selected.push(id);
                        }
                        *sel = Selection::Beaver(id);
                    }
                    world::Thing::Beaver(id) => {
                        hud.selected = vec![id];
                        *sel = Selection::Beaver(id);
                    }
                    other => {
                        hud.selected.clear();
                        *sel = other.selection();
                    }
                }
                if hud.follow.is_some_and(|f| *sel != Selection::Beaver(f)) {
                    hud.follow = None;
                }
                let _ = point;
            }
        }
        Tool::Building(_) | Tool::Dam | Tool::Canal if mouse.just_pressed(MouseButton::Left) && flow.fog.get(cell) < crate::flow::EXPLORED => {
            hud.refuse("Unexplored", now);
        }
        Tool::Building(kind) => {
            if mouse.just_pressed(MouseButton::Left) {
                let p = build_logic::site_preview(&sim.0, kind, cell);
                match build_logic::emit(&mut sim.0, &mut pending.0, &BuildOrder::Building { kind, pos: p.pos }) {
                    Ok(_) => {
                        if let Some(b) = sim.0.buildings.buildings.last() {
                            hud.undo.push(UndoItem::Site(b.id));
                            hud.frame_req = Some(cell_world(&sim.0.height, (b.pos.0 as f32, b.pos.1 as f32)));
                        }
                    }
                    Err(e) => {
                        let w = place_error(&e);
                        hud.say(Some((Icon::Cross, BAD)), Some((building_icon(kind), INK)), w, now);
                    }
                }
            }
        }
        Tool::Dam | Tool::Canal => {
            if mouse.just_pressed(MouseButton::Left) {
                hud.drag_from = Some(cell);
                hud.line = vec![cell];
            } else if mouse.pressed(MouseButton::Left) {
                if let Some(a) = hud.drag_from {
                    let line = build_logic::line_cells(a, cell);
                    if line != hud.line {
                        hud.line = line;
                    }
                }
            } else if mouse.just_released(MouseButton::Left) {
                hud.drag_from = None;
                if !hud.line.is_empty() {
                    let n = hud.line.len() as f32;
                    let mid = (hud.line.iter().map(|c| c.0 as f32).sum::<f32>() / n, hud.line.iter().map(|c| c.1 as f32).sum::<f32>() / n);
                    hud.frame_req = Some(cell_world(&sim.0.height, mid));
                }
            }
        }
    }
}

/// Direct orders for selected beavers (W4): a resource → gather that kind around it (Shift: just
/// that one); a workplace → fill its free slots.
fn order(s: &mut sim::SimState, hud: &mut HudState, who: &[BeaverId], t: world::Thing, one: bool, now: f32) {
    match t {
        world::Thing::Resource(rid) => {
            let Some(r) = s.resources.get(rid) else { return };
            let to = if one {
                sim::agents::Assignment::Resource(rid)
            } else {
                let rad = 6;
                let max = s.world.size - 1;
                let c = r.cell;
                let area = sim::work::Area { min: (c.0.saturating_sub(rad), c.1.saturating_sub(rad)), max: ((c.0 + rad).min(max), (c.1 + rad).min(max)) };
                sim::agents::Assignment::Area(r.kind, area)
            };
            let kind = r.kind;
            match s.apply(sim::Command::Assign { who: who.to_vec(), to: Some(to) }) {
                Ok(_) => hud.say(Some((resource_icon(kind), GOOD)), None, format!("{}", who.len()), now),
                Err(e) => hud.refuse(sim_error(&e.to_string()), now),
            }
        }
        world::Thing::Cell(c) => {
            // Explore: walk there (the beavers see the ground around them; reaching a valley's
            // edge opens it).
            match s.apply(sim::Command::Assign { who: who.to_vec(), to: Some(sim::agents::Assignment::Go(c)) }) {
                Ok(_) => hud.say(Some((Icon::Walk, INK)), None, format!("{}", who.len()), now),
                Err(e) => hud.refuse(sim_error(&e.to_string()), now),
            }
        }
        world::Thing::Building(b) => {
            let mut placed = 0;
            for &w in who {
                let Some(slot) = s.work.place(b).and_then(|p| p.slots.iter().position(|x| x.worker.is_none())) else { break };
                if s.apply(sim::Command::FillSlot { building: b, slot, who: w }).is_ok() {
                    placed += 1;
                }
            }
            if placed == 0 {
                hud.refuse("Full", now);
            }
        }
        _ => {}
    }
}

/// Selects the beavers inside a dragged box.
fn box_select(mut hud: ResMut<HudState>, sim: Res<Sim>, cams: Query<(&Camera, &GlobalTransform), With<MainCamera>>, scale: Res<UiScale>, mut sel: ResMut<Selection>) {
    let Some((a, b)) = hud.box_done.take() else { return };
    let Ok((cam, gt)) = cams.single() else { return };
    let (lo, hi) = (a.min(b), a.max(b));
    let s = &sim.0;
    let picked: Vec<BeaverId> = s
        .colony
        .beavers
        .iter()
        .filter(|bv| bv.stage != sim::colony::Stage::Kit)
        .filter(|bv| world::beaver_world(s, bv.id).and_then(|w| cam.world_to_viewport(gt, w).ok()).map(|v| v / scale.0.max(0.01)).is_some_and(|v| v.x >= lo.x && v.x <= hi.x && v.y >= lo.y && v.y <= hi.y))
        .map(|bv| bv.id)
        .collect();
    if let Some(first) = picked.first() {
        *sel = Selection::Beaver(*first);
    }
    hud.selected = picked;
}

/// Moves the camera so a new placement sits in the work rect (F3.3).
fn frame_camera(mut hud: ResMut<HudState>, sim: Res<Sim>, cams: Query<(&Camera, &GlobalTransform), With<MainCamera>>, windows: Query<&Window, With<PrimaryWindow>>, orbit: Res<crate::camera::OrbitCamera>, mut flow: ResMut<Flow>) {
    let Some(p) = hud.frame_req.take() else { return };
    let (Ok((cam, gt)), Ok(w)) = (cams.single(), windows.single()) else { return };
    if let Some(t) = world::frame_target(cam, gt, Vec2::new(w.width(), w.height()), &sim.0, orbit.target, p) {
        flow.focus = Some(t);
    }
}

fn follow_beaver(hud: Res<HudState>, sim: Res<Sim>, mut flow: ResMut<Flow>) {
    if let Some(w) = hud.follow.and_then(|id| world::beaver_world(&sim.0, id)) {
        flow.focus = Some(w);
    }
}

fn commit_line(sim: &mut sim::SimState, hud: &mut HudState, pending: &mut Pending, now: f32) {
    if hud.line.is_empty() {
        return;
    }
    let order = match hud.tool {
        Tool::Dam => BuildOrder::Dam { cells: hud.line.clone(), height: hud.dam_height },
        Tool::Canal => BuildOrder::Canal { cells: hud.line.clone() },
        _ => return,
    };
    let canal = matches!(order, BuildOrder::Canal { .. });
    match build_logic::emit(sim, pending, &order) {
        Ok(_) => {
            let item = if canal { pending.0.last().map(|w| UndoItem::Canal(w.order)) } else { sim.buildings.buildings.last().map(|b| UndoItem::Site(b.id)) };
            hud.undo.extend(item);
            hud.line.clear();
            hud.preview = None;
        }
        Err(e) => hud.refuse(place_error(&e), now),
    }
}

/// Takes back the newest placement that is still unfinished, refunding what was delivered.
fn undo_last(sim: &mut sim::SimState, hud: &mut HudState, pending: &mut Pending, now: f32) {
    while let Some(item) = hud.undo.pop() {
        match item {
            UndoItem::Site(id) => {
                let Some(kind) = sim.buildings.building(id).map(|b| b.kind) else { continue };
                match sim.cancel_site(id) {
                    Ok(back) => {
                        let refund = back.iter().filter(|a| a.1 > 0.05).max_by(|a, b| a.1.total_cmp(&b.1)).map(|a| format!("+{}", a.1.round() as i64));
                        let icon2 = back.iter().filter(|a| a.1 > 0.05).max_by(|a, b| a.1.total_cmp(&b.1)).map(|a| (good_icon(a.0), good_color(a.0)));
                        hud.msg = Some((Msg { icon: Some((Icon::Undo, INK)), icon2: icon2.or(Some((building_icon(kind), INK))), text: refund.unwrap_or_default() }, now));
                    }
                    Err(e) => hud.say(Some((Icon::Undo, INK)), Some((Icon::Cross, BAD)), sim_error(&e.to_string()), now),
                }
                return;
            }
            UndoItem::Canal(order) => {
                if build_logic::cancel(sim, pending, build_logic::OrderTarget::Pending(order)).is_ok() {
                    hud.say(Some((Icon::Undo, INK)), Some((Icon::Canal, INK)), "", now);
                    return;
                }
            }
        }
    }
}

/// The ≤ 2-word reason a cell cannot be settled.
pub(crate) fn settle_error(e: crate::flow::SettleError) -> &'static str {
    use crate::flow::SettleError::*;
    match e {
        OutOfBounds => "Off map",
        Locked => "Closed",
        Unexplored => "Unexplored",
        Water => "Water",
    }
}

pub(crate) fn building_at(sim: &sim::SimState, cell: Cell) -> Option<BuildingId> {
    sim.buildings.buildings.iter().find(|b| b.cells().any(|c| c == cell)).map(|b| b.id)
}

#[allow(clippy::too_many_arguments)]
fn button_actions(
    q: Query<(&Interaction, &HudAction), Changed<Interaction>>,
    keys: Res<ButtonInput<KeyCode>>,
    time: Res<Time>,
    mut sim: ResMut<Sim>,
    mut hud: ResMut<HudState>,
    mut clock: ResMut<SimClock>,
    mut flow: ResMut<Flow>,
    mut sel: ResMut<Selection>,
    mut pending: ResMut<PendingWorks>,
    mut lens: ResMut<MapLens>,
) {
    let now = time.elapsed_secs();
    let shift = keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight);
    let actions: Vec<HudAction> = q.iter().filter(|(i, _)| **i == Interaction::Pressed).map(|(_, a)| a.clone()).collect();
    for a in actions {
        let s = &mut sim.0;
        match a {
            HudAction::Fix(fix, at) => match fix {
                Fix::Place(kind) => hud.set_tool(if kind == BuildingKind::Dam { Tool::Dam } else { Tool::Building(kind) }),
                Fix::OpenJobs => hud.colony = Some(ColonyTab::Jobs),
                Fix::Focus => {
                    if let Some(c) = at {
                        flow.focus = Some(cell_world(&s.height, (c.0 as f32, c.1 as f32)));
                    }
                }
                f => {
                    if let Err(e) = sim::hud::apply_fix(s, f) {
                        hud.refuse(sim_error(&e), now);
                    }
                }
            },
            HudAction::Focus(c) => flow.focus = Some(cell_world(&s.height, (c.0 as f32, c.1 as f32))),
            HudAction::CycleIdle => {
                if shift {
                    hud.colony = Some(ColonyTab::Jobs);
                } else {
                    next_idle(s, &mut hud, &mut sel, &mut flow);
                }
            }
            HudAction::Pause => {
                clock.toggle_pause();
                hud.set_changed();
            }
            HudAction::Speed(sp) => {
                clock.set_speed(sp);
                hud.set_changed();
            }
            HudAction::OpenCat(i) => {
                hud.open_cat = if hud.open_cat == Some(i) { None } else { Some(i) };
                hud.search = None;
            }
            HudAction::Pick(kind) => {
                let t = match kind {
                    BuildingKind::Dam => Tool::Dam,
                    BuildingKind::Canal => Tool::Canal,
                    k => Tool::Building(k),
                };
                if s.tech.allows(kind) {
                    let next = if hud.tool == t { Tool::None } else { t };
                    hud.set_tool(next);
                    hud.search = None;
                    *sel = Selection::None;
                }
            }
            HudAction::CancelTool => hud.set_tool(Tool::None),
            HudAction::DamHeight(d) => hud.dam_height = (hud.dam_height + d).clamp(MIN_DAM_HEIGHT, MAX_DAM_HEIGHT),
            HudAction::ConfirmLine => commit_line(s, &mut hud, &mut pending.0, now),
            HudAction::ClearLine => {
                hud.line.clear();
                hud.preview = None;
            }
            HudAction::CloseInspector => {
                *sel = Selection::None;
                hud.selected.clear();
                hud.follow = None;
            }
            HudAction::SetLens(l) => {
                *lens = l;
                hud.set_changed();
            }
            HudAction::Undo => undo_last(s, &mut hud, &mut pending.0, now),
            HudAction::OpenSearch => hud.search = if hud.search.is_some() { None } else { Some(String::new()) },
            HudAction::Colony(t) => hud.colony = t,
            HudAction::ToggleDetails => hud.details_open = !hud.details_open,
            HudAction::ToggleMinimap => hud.minimap_collapsed = !hud.minimap_collapsed,
            HudAction::ToggleProfile => hud.profile_collapsed = !hud.profile_collapsed,
            HudAction::SelectBeaver(id) => {
                *sel = Selection::Beaver(id);
                hud.selected = vec![id];
                if let Some(b) = s.colony.beaver(id) {
                    flow.focus = Some(cell_world(&s.height, b.pos));
                }
            }
            HudAction::SelectBuilding(id) => {
                *sel = Selection::Site(id);
                hud.selected.clear();
                if let Some(b) = s.buildings.building(id) {
                    flow.focus = Some(cell_world(&s.height, (b.pos.0 as f32, b.pos.1 as f32)));
                }
            }
            HudAction::Follow(id) => hud.follow = if hud.follow == Some(id) { None } else { Some(id) },
            HudAction::FreeBeaver(id) => free_beaver(s, &mut hud, id, now),
            HudAction::AddWorker(building) => cmd(s, &mut hud, sim::Command::AddWorker { building }, now),
            HudAction::RemoveWorker(building) => cmd(s, &mut hud, sim::Command::RemoveWorker { building }, now),
            HudAction::RoleAdjust(role, delta) => cmd(s, &mut hud, sim::Command::AdjustRole { role, delta }, now),
            HudAction::RoleUp(role) => {
                let mut order = s.work.role_order.clone();
                if let Some(i) = order.iter().position(|r| *r == role).filter(|i| *i > 0) {
                    order.swap(i, i - 1);
                    cmd(s, &mut hud, sim::Command::SetRoleOrder { order }, now);
                }
            }
            HudAction::Priority(building, priority) => cmd(s, &mut hud, sim::Command::SetPriority { building, priority }, now),
            HudAction::Paused(building, paused) => cmd(s, &mut hud, sim::Command::SetPaused { building, paused }, now),
            HudAction::Unlock(who) => cmd(s, &mut hud, sim::Command::Unlock { who }, now),
            HudAction::AutoStaff(on) => cmd(s, &mut hud, sim::Command::SetAutoStaff { on }, now),
            HudAction::SortPop(k) => hud.pop_sort = k,
            HudAction::Recipe(building, recipe) => cmd(s, &mut hud, sim::Command::SetRecipe { building, recipe }, now),
            HudAction::Pin(p) => {
                if let Some(i) = hud.pinned.iter().position(|x| *x == p) {
                    hud.pinned.remove(i);
                } else {
                    hud.pinned.push(p);
                    if hud.pinned.len() > 3 {
                        hud.pinned.remove(0);
                    }
                }
            }
            HudAction::EditArea(b) => {
                hud.area_edit = if hud.area_edit == Some(b) { None } else { Some(b) };
                hud.area_from = None;
            }
        }
    }
}

/// Applies a command; a refusal shows its ≤ 2-word reason in the lane.
fn cmd(s: &mut sim::SimState, hud: &mut HudState, c: sim::Command, now: f32) {
    if let Err(e) = s.apply(c) {
        hud.refuse(sim_error(&e.to_string()), now);
    }
}

fn free_beaver(s: &mut sim::SimState, hud: &mut HudState, id: BeaverId, now: f32) {
    let res = match s.work.slot_of(id) {
        Some((b, slot, _)) => s.apply(sim::Command::EmptySlot { building: b, slot }),
        None => s.apply(sim::Command::Assign { who: vec![id], to: None }),
    };
    if let Err(e) = res {
        hud.refuse(sim_error(&e.to_string()), now);
    }
}

/// What + on a building would do (W2): who it would take, and from where. No lines: nobody.
pub(crate) fn add_worker_tip(s: &sim::SimState, b: BuildingId) -> Tip {
    let mut t = Tip::name("Add a worker");
    if let Some(p) = sim::jobs::add_worker_preview(s, b) {
        let who = s.colony.beaver(p.who).map(|x| x.name.clone()).unwrap_or_default();
        match p.from {
            Some((_, role)) => t.push(TipLine::new(who, format!("from {}", panels::role_kind_name(role).to_lowercase())).icon(panels::role_kind_icon(role), MUTED)),
            None => t.push(TipLine::new(who, "Free").icon(Icon::Idle, INK)),
        };
    }
    t
}

fn poll_pending_works(mut sim: ResMut<Sim>, mut pending: ResMut<PendingWorks>, mut hud: ResMut<HudState>, time: Res<Time>) {
    if pending.0 .0.is_empty() {
        return;
    }
    for m in build_logic::poll_pending(&mut sim.0, &mut pending.0) {
        // Finished works are visible in the world; only failures get a message.
        if let Some(why) = m.split(" failed: ").nth(1) {
            hud.refuse(sim_error(why), time.elapsed_secs());
        }
    }
}

// ───────────────────────────── refreshing panels ─────────────────────────────

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
    // Refresh the dam preview only when the line or crest changes.
    if hud.tool == Tool::Dam && !hud.line.is_empty() {
        let stale = hud.preview.as_ref().is_none_or(|(c, h, _)| *c != hud.line || *h != hud.dam_height);
        if stale {
            let p = build_logic::dam_preview(s, &hud.line, hud.dam_height);
            hud.preview = Some((hud.line.clone(), hud.dam_height, p));
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
    let founded = s.colony.is_founded();
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
            Panel::Profile => panels::profile(&ctx),
            Panel::Minimap => panels::minimap(&ctx),
            Panel::Lane => panels::lane(&ctx),
            Panel::Colony => colony::screen(&ctx),
            _ => continue,
        };
        ui::reconcile(&mut commands, &icons, e, &mut m, tree, 0);
    }
}

/// Panels bound to the cursor or the world: every frame.
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
            Panel::Cursor if !s.colony.is_founded() => match (hover.0, cursor.0) {
                (Some(cell), Some(cur)) if flow.phase == crate::flow::Phase::Scouting => panels::settle_tag(s, &flow, cell, cur),
                _ => Vec::new(),
            },
            _ if !s.colony.is_founded() => Vec::new(),
            Panel::Cursor => {
                let ctx = panels::Ctx { s, hud: &hud, clock: &clock, sel: Selection::None, lens: MapLens::World, hover: hover.0, problems: &[], minimap: None, view_cell: (0.0, 0.0), log: &flow.log, history: &flow.history, cursor: cursor.0 };
                panels::cursor_tag(&ctx)
            }
            _ => match cams.single() {
                Ok((cam, gt)) => world::markers(s, cam, gt, scale.0, &hud),
                Err(_) => Vec::new(),
            },
        };
        ui::reconcile(&mut commands, &icons, e, &mut m, tree, 0);
    }
}

// ───────────────────────────── search ─────────────────────────────

/// Words players type for what they want, mapped to the buildings that provide it.
const SYNONYMS: [(&str, &[BuildingKind]); 18] = [
    ("food", &[BuildingKind::ForagerHut, BuildingKind::FoodCache]),
    ("eat", &[BuildingKind::ForagerHut, BuildingKind::FoodCache]),
    ("wood", &[BuildingKind::LoggingCamp]),
    ("log", &[BuildingKind::LoggingCamp]),
    ("stick", &[BuildingKind::LoggingCamp]),
    ("tree", &[BuildingKind::LoggingCamp]),
    ("mud", &[BuildingKind::MudPit]),
    ("stone", &[BuildingKind::QuarryCamp]),
    ("rock", &[BuildingKind::QuarryCamp]),
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

/// Menu entries whose name contains the query, then those a synonym points to (unlocked first).
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

fn search_input(mut keys: MessageReader<KeyboardInput>, sim: Res<Sim>, mut hud: ResMut<HudState>, mut typing: ResMut<TextCapture>) {
    let open = hud.search.is_some();
    if typing.0 != open {
        typing.0 = open;
    }
    if !open {
        keys.clear();
        return;
    }
    for ev in keys.read() {
        if !ev.state.is_pressed() {
            continue;
        }
        match &ev.logical_key {
            Key::Escape => hud.search = None,
            Key::Backspace => {
                if let Some(q) = hud.search.as_mut() {
                    q.pop();
                }
            }
            Key::Enter => {
                let q = hud.search.clone().unwrap_or_default();
                if let Some(e) = search_matches(&sim.0, &q).into_iter().find(|e| e.unlocked) {
                    let t = match e.kind {
                        BuildingKind::Dam => Tool::Dam,
                        BuildingKind::Canal => Tool::Canal,
                        k => Tool::Building(k),
                    };
                    hud.set_tool(t);
                }
                hud.search = None;
            }
            _ => {
                if let Some(t) = &ev.text {
                    let add: String = t.chars().filter(|c| c.is_alphanumeric() || *c == ' ').collect();
                    if !add.is_empty() {
                        if let Some(q) = hud.search.as_mut() {
                            q.push_str(&add);
                        }
                    }
                }
            }
        }
    }
    typing.0 = hud.search.is_some();
}

// ───────────────────────────── minimap ─────────────────────────────

fn setup_minimap(mut commands: Commands, sim: Res<Sim>, mut images: ResMut<Assets<Image>>) {
    let n = sim.0.world.size as u32;
    let img = Image::new_fill(Extent3d { width: n, height: n, depth_or_array_layers: 1 }, TextureDimension::D2, &[30, 30, 30, 255], TextureFormat::Rgba8UnormSrgb, RenderAssetUsages::default());
    commands.insert_resource(MinimapImage(images.add(img)));
}

/// Repaints the minimap every couple of seconds: land shaded by height, water by depth,
/// buildings white, beavers orange.
fn update_minimap(time: Res<Time>, mut next: Local<f32>, sim: Res<Sim>, flow: Res<Flow>, img: Option<Res<MinimapImage>>, mut images: ResMut<Assets<Image>>) {
    let Some(img) = img else { return };
    let now = time.elapsed_secs();
    if now < *next {
        return;
    }
    *next = now + 2.0;
    let s = &sim.0;
    let n = s.world.size;
    let maxh = s.height.as_slice().iter().copied().fold(1.0f32, f32::max);
    let mut buf = vec![0u8; n * n * 4];
    for y in 0..n {
        for x in 0..n {
            let i = y * n + x;
            let h = s.height.as_slice()[i] / maxh;
            let d = s.water.depth.as_slice()[i];
            let c: [f32; 3] = if d > sim::buildings::WET_DEPTH {
                let k = (d / 3.0).clamp(0.0, 1.0).sqrt();
                [0.42 - 0.34 * k, 0.70 - 0.36 * k, 0.92 - 0.26 * k]
            } else {
                [0.30 + 0.45 * h, 0.40 + 0.38 * h, 0.26 + 0.40 * h]
            };
            // Unexplored ground is cloud, like the fog over the map.
            let seen = (flow.fog.get((x, y)) / crate::flow::EXPLORED).min(1.0);
            let c = [c[0] * seen + 0.78 * (1.0 - seen), c[1] * seen + 0.8 * (1.0 - seen), c[2] * seen + 0.84 * (1.0 - seen)];
            buf[i * 4..i * 4 + 4].copy_from_slice(&[(c[0] * 255.0) as u8, (c[1] * 255.0) as u8, (c[2] * 255.0) as u8, 255]);
        }
    }
    for b in &s.buildings.buildings {
        for (x, y) in b.cells().filter(|c| flow.fog.get(*c) >= crate::flow::EXPLORED) {
            if x < n && y < n {
                buf[(y * n + x) * 4..(y * n + x) * 4 + 3].copy_from_slice(&[245, 245, 245]);
            }
        }
    }
    for bv in &s.colony.beavers {
        let (x, y) = (bv.pos.0.max(0.0) as usize, bv.pos.1.max(0.0) as usize);
        if x < n && y < n {
            buf[(y * n + x) * 4..(y * n + x) * 4 + 3].copy_from_slice(&[232, 132, 58]);
        }
    }
    if let Some(mut image) = images.get_mut(&img.0) {
        image.data = Some(buf);
    }
}

/// Click or drag on the minimap moves the camera there.
fn minimap_click(q: Query<(&Interaction, &RelativeCursorPosition, &TagComp)>, sim: Res<Sim>, mut orbit: ResMut<crate::camera::OrbitCamera>) {
    for (i, rel, tag) in &q {
        if tag.0 != Tag::Minimap || *i != Interaction::Pressed {
            continue;
        }
        let Some(n) = rel.normalized else { continue };
        let size = sim.0.world.size as f32;
        let cell = (((n.x + 0.5) * size).clamp(0.0, size - 1.0), ((n.y + 0.5) * size).clamp(0.0, size - 1.0));
        orbit.target = cell_world(&sim.0.height, cell);
    }
}

/// Publishes the lens to the renderers: Alt peeks at water, line tools turn the water lens on.
fn sync_lens(keys: Res<ButtonInput<KeyCode>>, lens: Res<MapLens>, hud: Res<HudState>) {
    let alt = keys.pressed(KeyCode::AltLeft) || keys.pressed(KeyCode::AltRight);
    let shown = if alt || matches!(hud.tool, Tool::Dam | Tool::Canal) { MapLens::Water } else { *lens };
    crate::terrain_render::MAP_LENS.store(shown.code(), std::sync::atomic::Ordering::Relaxed);
}

// ───────────────────────────── scripted views for screenshots ─────────────────────────────

/// `BC_HUD_DEMO=rest|dam|canal|place|inspect|camp|beaver|ledger|jobs|pop|research|goods|water|search|tip`
/// (with `BC_HUD=1` and `--screenshot`): once the colony stands, runs `BC_HUD_DAYS` days (default
/// 12) and sets up that view, so each state can be captured without a mouse. `camp`, `ledger`,
/// `jobs` and `goods` first stand up a staffed logging camp, forager hut and warehouse beside the
/// camp (finished by script) so there is real production to show. `tip` opens the food tooltip.
#[allow(clippy::too_many_arguments)]
fn demo(
    mut done: Local<bool>,
    planned: Res<crate::state::PlannedDam>,
    mut sim: ResMut<Sim>,
    mut hud: ResMut<HudState>,
    mut sel: ResMut<Selection>,
    mut orbit: ResMut<crate::camera::OrbitCamera>,
    mut pending: ResMut<PendingWorks>,
    mut lens: ResMut<MapLens>,
    mut tips: ResMut<Tips>,
) {
    if *done || !sim.0.colony.is_founded() {
        return;
    }
    *done = true;
    let Ok(which) = std::env::var("BC_HUD_DEMO") else { return };
    let days: f32 = std::env::var("BC_HUD_DAYS").ok().and_then(|v| v.parse().ok()).unwrap_or(12.0);
    let s = &mut sim.0;
    let camp = planned.0.as_ref().map(|d| d.camp);
    if which == "inspect" {
        if let Some(c) = camp {
            'search: for r in 2..20usize {
                for y in c.1.saturating_sub(r)..=c.1 + r {
                    for x in c.0.saturating_sub(r)..=c.0 + r {
                        let order = BuildOrder::Building { kind: BuildingKind::Burrow, pos: (x, y) };
                        if build_logic::emit(s, &mut pending.0, &order).is_ok() {
                            break 'search;
                        }
                    }
                }
            }
        }
    }
    let look = |o: &mut crate::camera::OrbitCamera, h: &sim::grid::HeightGrid, c: (f32, f32), dist: f32| {
        o.target = cell_world(h, c);
        o.distance = dist;
        o.pitch = 0.95;
    };
    // `play`: the scripted attentive player plays BC_HUD_DAYS days, then BC_HUD_VIEW is shown.
    if which == "play" {
        let dam = planned.0.as_ref().map(|d| (d.cells.clone(), d.height));
        let mut p = sim::autoplay::Player::new(sim::autoplay::Style::Attentive, camp.unwrap_or((0, 0)), dam);
        let steps = (1.0 / crate::sim_driver::FIXED_DT_DAYS) as usize;
        for _ in 0..days as usize {
            p.act(s);
            for _ in 0..steps {
                let _ = s.step(crate::sim_driver::FIXED_DT_DAYS);
            }
        }
        for l in &p.log {
            println!("autoplay {l}");
        }
        let view = std::env::var("BC_HUD_VIEW").unwrap_or_default();
        match view.as_str() {
            "jobs" => hud.colony = Some(ColonyTab::Jobs),
            "pop" => hud.colony = Some(ColonyTab::Population),
            "goods" => hud.colony = Some(ColonyTab::Goods),
            "research" => hud.colony = Some(ColonyTab::Research),
            "log" => hud.colony = Some(ColonyTab::Log),
            "building" => {
                if let Some(b) = s.buildings.buildings.iter().find(|b| b.kind == BuildingKind::ForagerHut).or(s.buildings.buildings.first()) {
                    *sel = Selection::Site(b.id);
                }
            }
            "beaver" => {
                if let Some(b) = s.colony.beavers.iter().find(|b| b.stage == sim::colony::Stage::Adult) {
                    *sel = Selection::Beaver(b.id);
                    hud.selected = vec![b.id];
                }
            }
            _ => {}
        }
        if let Some(c) = camp {
            look(&mut orbit, &s.height, (c.0 as f32, c.1 as f32), 42.0);
        }
        return;
    }
    let mut staffed = Vec::new();
    if matches!(which.as_str(), "camp" | "ledger" | "goods" | "jobs" | "pop" | "beaver" | "research" | "tip") {
        if let Some(c) = camp {
            staffed = stand_up_camp(s, &mut pending.0, c);
        }
    }
    for _ in 0..(days / crate::sim_driver::FIXED_DT_DAYS) as usize {
        let _ = s.step(crate::sim_driver::FIXED_DT_DAYS);
    }
    let water_cat = panels::categories(s).iter().position(|g| *g == Group::Branch(sim::tech::Branch::Water));
    match which.as_str() {
        "dam" | "canal" => {
            if let Some(d) = planned.0.as_ref() {
                hud.open_cat = water_cat;
                hud.set_tool(if which == "dam" { Tool::Dam } else { Tool::Canal });
                hud.line = d.cells.clone();
                let crest = std::env::var("BC_HUD_CREST").ok().and_then(|v| v.parse().ok()).unwrap_or(d.height);
                hud.dam_height = crest.clamp(MIN_DAM_HEIGHT, MAX_DAM_HEIGHT);
                let m = d.cells[d.cells.len() / 2];
                look(&mut orbit, &s.height, (m.0 as f32, m.1 as f32), 40.0);
            }
        }
        "place" => {
            hud.set_tool(Tool::Building(BuildingKind::Warehouse));
            if let Some(c) = camp {
                look(&mut orbit, &s.height, (c.0 as f32, c.1 as f32), 40.0);
            }
        }
        "inspect" => {
            if let Some(b) = s.buildings.buildings.first() {
                *sel = Selection::Site(b.id);
                look(&mut orbit, &s.height, (b.pos.0 as f32, b.pos.1 as f32), 30.0);
            }
        }
        "camp" => {
            if let Some(&id) = staffed.first() {
                *sel = Selection::Site(id);
                hud.details_open = std::env::var_os("BC_HUD_DETAILS").is_some();
                let b = s.buildings.building(id).map(|b| b.pos).unwrap_or((0, 0));
                look(&mut orbit, &s.height, (b.0 as f32, b.1 as f32), 32.0);
            }
        }
        "beaver" => {
            if let Some(b) = s.colony.beavers.iter().find(|b| b.stage == sim::colony::Stage::Adult) {
                *sel = Selection::Beaver(b.id);
                hud.selected = vec![b.id];
                look(&mut orbit, &s.height, b.pos, 28.0);
            }
        }
        other => {
            match other {
                "ledger" => hud.colony = Some(ColonyTab::Goods),
                "goods" => *lens = MapLens::Goods,
                "jobs" => hud.colony = Some(ColonyTab::Jobs),
                "pop" => hud.colony = Some(ColonyTab::Population),
                "research" => hud.colony = Some(ColonyTab::Research),
                "water" => *lens = MapLens::Water,
                "search" => hud.search = Some("food".into()),
                "tip" => {
                    tips.layers.push(ui::Layer {
                        src: ui::TipSrc::World(0),
                        tip: panels::good_tip(s, sim::buildings::Good::Food),
                        pos: Vec2::new(40.0, 52.0),
                        frozen: true,
                        opened: 0.0,
                        last_held: f32::MAX,
                    });
                }
                _ => {}
            }
            if let Some(c) = camp {
                look(&mut orbit, &s.height, (c.0 as f32, c.1 as f32), if other == "water" { 60.0 } else { 45.0 });
            }
        }
    }
}

/// Screenshot scaffolding: places a logging camp, a forager hut and a warehouse near `camp`,
/// finishes them at once and puts one adult in each camp. Returns the two camps.
fn stand_up_camp(s: &mut sim::SimState, pending: &mut Pending, camp: Cell) -> Vec<BuildingId> {
    s.tech.record(sim::tech::Achievement::FirstBurrow);
    s.tech.record(sim::tech::Achievement::FirstDam);
    s.tech.update(&s.colony);
    let mut placed = Vec::new();
    // Only where the family can walk to from where it stands (the river banks can be too steep).
    let walker = s.colony.beavers.iter().find(|b| b.stage == sim::colony::Stage::Adult).map(|b| b.id);
    let from = walker.and_then(|w| s.agents.get(w)).map(|a| a.cell);
    for kind in [BuildingKind::LoggingCamp, BuildingKind::ForagerHut, BuildingKind::Warehouse] {
        'search: for r in 3..30usize {
            for y in camp.1.saturating_sub(r)..=camp.1 + r {
                for x in camp.0.saturating_sub(r)..=camp.0 + r {
                    let reachable = match (walker, from) {
                        (Some(w), Some(f)) => s.nav.find_path(w, f, sim::nav::Goal::Beside((x, y))).is_some(),
                        _ => true,
                    };
                    if !reachable {
                        continue;
                    }
                    if build_logic::emit(s, pending, &BuildOrder::Building { kind, pos: (x, y) }).is_ok() {
                        if let Some(b) = s.buildings.buildings.last() {
                            placed.push(b.id);
                        }
                        break 'search;
                    }
                }
            }
        }
    }
    let adults: Vec<_> = s.colony.beavers.iter().filter(|b| b.stage == sim::colony::Stage::Adult).map(|b| b.id).collect();
    let mut camps = Vec::new();
    for id in placed {
        let Some(b) = s.buildings.buildings.iter_mut().find(|b| b.id == id) else { continue };
        b.status = Status::Complete;
        let kind = b.kind;
        let area = sim::catalogue::work_area(s.buildings.building(id).unwrap());
        let slots = sim::catalogue::def(kind).slots;
        let _ = s.work.finish_site(id, &slots, area);
        if let Some(pl) = s.work.places.iter_mut().find(|p| p.building == id) {
            pl.site = None;
        }
        if !slots.is_empty() {
            camps.push(id);
        }
    }
    for (id, who) in camps.iter().zip(adults) {
        let _ = s.apply(sim::Command::FillSlot { building: *id, slot: 0, who });
    }
    camps // store capacity follows on the next sim step
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn categories_follow_the_menu_order_and_fit_the_hotkeys() {
        let s = sim::SimState::new(1);
        let cats = panels::categories(&s);
        assert_eq!(cats.first(), Some(&Group::Base));
        assert!(cats.len() <= CAT_KEYS.len(), "{} categories but {} keys", cats.len(), CAT_KEYS.len());
    }

    /// Spec F5: no sentences anywhere, at most two words per label, at most six words in any
    /// panel outside the Colony screen, at most six per tooltip line. Names (buildings, beavers,
    /// techs, maps) and key labels do not count.
    #[test]
    fn every_panel_keeps_to_the_text_budget() {
        let (mut s, dam) = sim::starts::new_game_with_dam(1).expect("start");
        s.apply(sim::Command::Settle { cell: dam.camp }).expect("settle");
        for _ in 0..(6.0 / crate::sim_driver::FIXED_DT_DAYS) as usize {
            let _ = s.step(crate::sim_driver::FIXED_DT_DAYS);
        }
        let mut names: Vec<String> = BuildingKind::ALL.iter().map(|k| k.spec().name.to_string()).collect();
        names.extend(s.colony.beavers.iter().map(|b| b.name.clone()));
        names.extend(sim::tech::TECHS.iter().map(|t| t.name.to_string()));
        names.sort_by_key(|n| std::cmp::Reverse(n.len()));
        let keys = ["Ctrl+Z", "Ctrl+wheel", "Enter", "Esc", "Space", "Tab", "LMB", "RMB", "Alt", "[ ]"];
        let count = |t: &str| -> usize {
            let mut t = t.to_string();
            for n in &names {
                t = t.replace(n.as_str(), "");
            }
            if keys.contains(&t.trim()) {
                return 0;
            }
            t.split_whitespace().filter(|w| w.chars().any(|c| c.is_alphabetic()) && w.chars().filter(|c| c.is_alphabetic()).count() > 1).count()
        };
        let clock = SimClock::default();
        let log = std::collections::VecDeque::new();
        let problems = sim::hud::problems(&s);
        let first_building = s.buildings.buildings.first().map(|b| b.id);
        let beaver = s.colony.beavers.first().map(|b| b.id).unwrap();
        let mut states: Vec<(String, HudState, Selection)> = vec![("rest".into(), HudState::default(), Selection::None)];
        let mut dam_state = HudState { line: dam.cells.clone(), ..default() };
        dam_state.tool = Tool::Dam;
        dam_state.preview = Some((dam.cells.clone(), dam_state.dam_height, build_logic::dam_preview(&s, &dam.cells, dam_state.dam_height)));
        states.push(("dam".into(), dam_state, Selection::None));
        states.push(("canal".into(), HudState { tool: Tool::Canal, line: dam.cells.clone(), ..default() }, Selection::None));
        states.push(("place".into(), HudState { tool: Tool::Building(BuildingKind::Warehouse), ..default() }, Selection::None));
        states.push(("beaver".into(), HudState::default(), Selection::Beaver(beaver)));
        states.push(("cell".into(), HudState::default(), Selection::Cell(dam.camp)));
        if let Some(b) = first_building {
            states.push(("building".into(), HudState { details_open: true, ..default() }, Selection::Site(b)));
        }
        states.push(("items".into(), HudState { open_cat: Some(0), ..default() }, Selection::None));
        states.push(("search".into(), HudState { search: Some("food".into()), ..default() }, Selection::None));
        let mut failures = Vec::new();
        for (name, hud, sel) in &states {
            let ctx = panels::Ctx { s: &s, hud, clock: &clock, sel: *sel, lens: MapLens::World, hover: Some(dam.camp), problems: &problems, minimap: None, view_cell: (0.0, 0.0), log: &log, history: &[], cursor: Some(Vec2::new(300.0, 300.0)) };
            let all: Vec<(&str, Vec<ui::VNode>)> = vec![
                ("strip", panels::strip(&ctx)),
                ("calendar", panels::calendar(&ctx)),
                ("right", panels::right(&ctx)),
                ("rail", panels::rail(&ctx)),
                ("bottom", panels::bottom(&ctx)),
                ("profile", panels::profile(&ctx)),
                ("cursor", panels::cursor_tag(&ctx)),
            ];
            for (panel, tree) in &all {
                let texts = ui::words(tree);
                let total: usize = texts.iter().map(|t| count(t)).sum();
                if std::env::var_os("BUDGET_DEBUG").is_some() {
                    eprintln!("{name}/{panel}: {total} {texts:?}");
                }
                if total > 6 {
                    failures.push(format!("{name}/{panel}: {total} words {texts:?}"));
                }
                for t in &texts {
                    if count(t) > 2 || t.trim_end().ends_with('.') {
                        failures.push(format!("{name}/{panel}: label {t:?}"));
                    }
                }
                for tip in tips_of(tree) {
                    for l in tip.lines.iter().map(|l| format!("{} {}", l.label, l.value)).chain(tip.title.iter().map(|t| t.1.clone())) {
                        if count(&l) > 6 || l.trim_end().ends_with('.') {
                            failures.push(format!("{name}/{panel}: tip line {l:?}"));
                        }
                    }
                }
            }
            // The Colony screen is a table: labels stay short, no sentences (the log tab is history).
            for tab in [ColonyTab::Population, ColonyTab::Jobs, ColonyTab::Goods, ColonyTab::Water, ColonyTab::Land, ColonyTab::Research] {
                let h = HudState { colony: Some(tab), ..default() };
                let ctx = panels::Ctx { hud: &h, ..ctx };
                for t in ui::words(&colony::screen(&ctx)) {
                    if count(&t) > 2 || t.trim_end().ends_with('.') {
                        failures.push(format!("colony {tab:?}: label {t:?}"));
                    }
                }
            }
        }
        failures.dedup();
        assert!(failures.is_empty(), "{}", failures.join("\n"));
    }

    fn tips_of(tree: &[ui::VNode]) -> Vec<Tip> {
        let mut out = Vec::new();
        fn walk(v: &ui::VNode, out: &mut Vec<Tip>) {
            if let Some(t) = &v.tip {
                out.push(t.clone());
                for l in &t.lines {
                    if let Some(n) = &l.nested {
                        out.push((**n).clone());
                    }
                }
            }
            for c in &v.children {
                walk(c, out);
            }
        }
        for v in tree {
            walk(v, &mut out);
        }
        out
    }

    #[test]
    fn switching_tools_clears_the_drawn_line() {
        let mut h = HudState { line: vec![(1, 1), (2, 1)], drag_from: Some((1, 1)), ..default() };
        h.set_tool(Tool::Canal);
        assert!(h.line.is_empty() && h.drag_from.is_none());
    }
}
