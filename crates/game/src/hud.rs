//! The in-game interface. The world keeps the middle of the screen; the always-on parts are a
//! goods strip (top left), the calendar with the year ribbon and pond cover (top right), problem
//! cards that name their cause and carry a fix (right), and a flat build bar (bottom). Tools open a
//! card that previews exact effects before the click; the dam tool also opens the river profile.
//! Panels are rebuilt from the sim a few times a second; buttons carry a [`HudAction`].
//!
//! Buttons use `Interaction`, which Bevy 0.20 deprecates but still drives; the newer widgets need
//! a picking-observer rewrite that can follow later.
#![allow(deprecated)]
use crate::build_logic::{self, BuildOrder, Group, Pending, MAX_DAM_HEIGHT, MIN_DAM_HEIGHT};
use crate::camera::MainCamera;
use crate::flow::{cell_world, height_at, ray_heightfield, Cell, Flow, Selection};
use crate::sim_driver::{SimClock, Speed};
use crate::state::Sim;
use bevy::asset::RenderAssetUsages;
use bevy::input::keyboard::{Key, KeyboardInput};
use bevy::input::mouse::{AccumulatedMouseScroll, MouseScrollUnit};
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy::ui::RelativeCursorPosition;
use bevy::prelude::*;
use bevy::text::{FontSize, FontWeight};
use bevy::ui::FocusPolicy;
use bevy::window::PrimaryWindow;
use sim::buildings::{BuildingId, BuildingKind, Good, Status};
use sim::calendar::{self, Date, Season};
use sim::dam_preview::DamPreview;
use sim::hud::{Fix, Problem, Severity};
use sim::work::Role;

// ───────────────────────────── look ─────────────────────────────
// Theme lives in the frame (dark, warm glass with a timber top edge); data stays neutral.

pub(crate) const PANEL: Color = Color::srgba(0.106, 0.09, 0.075, 0.93);
pub(crate) const TIMBER: Color = Color::srgb(0.59, 0.41, 0.24);
pub(crate) const INK: Color = Color::srgb(0.94, 0.91, 0.86);
pub(crate) const MUTED: Color = Color::srgb(0.76, 0.71, 0.64);
pub(crate) const FAINT: Color = Color::srgba(1.0, 1.0, 1.0, 0.07);
pub(crate) const GOOD: Color = Color::srgb(0.39, 0.79, 0.55);
pub(crate) const WARN: Color = Color::srgb(0.94, 0.75, 0.29);
pub(crate) const BAD: Color = Color::srgb(0.94, 0.45, 0.38);
/// Water only: never a button, never a status.
const WATER: Color = Color::srgb(0.29, 0.64, 0.87);
/// Fix and commit buttons only.
pub(crate) const ACCENT: Color = Color::srgb(0.91, 0.52, 0.23);
const DAM_WOOD: Color = Color::srgb(0.54, 0.35, 0.2);
const BED: Color = Color::srgb(0.42, 0.3, 0.2);

fn good_color(g: Good) -> Color {
    match g {
        Good::Logs => Color::srgb(0.79, 0.54, 0.29),
        Good::Sticks => Color::srgb(0.85, 0.72, 0.48),
        Good::Mud => Color::srgb(0.65, 0.6, 0.42),
        Good::Stone => Color::srgb(0.62, 0.64, 0.67),
        Good::Food => Color::srgb(0.85, 0.33, 0.43),
        Good::Baskets => Color::srgb(0.78, 0.63, 0.29),
        Good::Rope => Color::srgb(0.73, 0.64, 0.49),
        Good::Tools => Color::srgb(0.56, 0.64, 0.72),
    }
}

fn good_name(g: Good) -> &'static str {
    match g {
        Good::Logs => "Logs",
        Good::Sticks => "Sticks",
        Good::Mud => "Mud",
        Good::Stone => "Stone",
        Good::Food => "Food",
        Good::Baskets => "Baskets",
        Good::Rope => "Rope",
        Good::Tools => "Tools",
    }
}

/// Goods always in the strip; the crafted ones join once the colony has any.
const STRIP_GOODS: [Good; 5] = [Good::Food, Good::Logs, Good::Sticks, Good::Mud, Good::Stone];
/// Category hotkeys, laid out like the bar itself.
const CAT_KEYS: [KeyCode; 6] = [KeyCode::KeyZ, KeyCode::KeyX, KeyCode::KeyC, KeyCode::KeyV, KeyCode::KeyB, KeyCode::KeyN];
const CAT_KEY_LABELS: [&str; 6] = ["Z", "X", "C", "V", "B", "N"];
const REBUILD_SECS: f32 = 0.25;
const MSG_SECS: f32 = 4.0;

// ───────────────────────────── state ─────────────────────────────

#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub enum Tool {
    #[default]
    None,
    Building(BuildingKind),
    Dam,
    Canal,
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
    pub msg: Option<(String, f32)>,
    idle_cursor: usize,
    /// The flow ledger screen is open.
    pub ledger_open: bool,
    /// The building search box, with what has been typed.
    pub search: Option<String>,
    /// Placements that can still be taken back, newest last.
    undo: Vec<UndoItem>,
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
    fn code(self) -> u8 {
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

#[derive(Component)]
struct MinimapPicture;

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
            ledger_open: false,
            search: None,
            undo: Vec::new(),
        }
    }
}

impl HudState {
    fn set_tool(&mut self, t: Tool) {
        self.tool = t;
        self.drag_from = None;
        self.line.clear();
        self.preview = None;
    }
    fn say(&mut self, s: impl Into<String>, now: f32) {
        self.msg = Some((s.into(), now));
    }
}

/// Cell under the cursor, `None` over the interface or off the map.
#[derive(Resource, Default)]
pub struct HoverCell(pub Option<Cell>);

/// Whether the pointer is over any interface element (world clicks are ignored then).
#[derive(Resource, Default)]
pub struct PointerOverUi(pub bool);

#[derive(Resource, Default)]
struct HoverGood(Option<Good>);

#[derive(Resource, Default)]
struct PendingWorks(Pending);

#[derive(Resource)]
struct RebuildTimer(Timer);

/// Marks every node that should swallow the pointer.
#[derive(Component)]
struct Blocks;

#[derive(Component, Clone, Copy, PartialEq, Eq, Hash, Debug)]
enum Panel {
    Ledger,
    LedgerTip,
    Calendar,
    Problems,
    BuildBar,
    Items,
    ToolCard,
    Profile,
    Inspector,
    Toast,
    Rail,
    FlowLedger,
    Minimap,
}

#[derive(Component)]
struct GoodItem(Good);

#[derive(Component, Clone, Debug, PartialEq)]
enum HudAction {
    Fix(Fix),
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
    FillSlot(BuildingId, usize),
    EmptySlot(BuildingId, usize),
    SetLens(MapLens),
    ToggleLedger,
    Undo,
    OpenSearch,
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
            .init_resource::<HoverGood>()
            .init_resource::<PendingWorks>()
            .init_resource::<MapLens>()
            .init_resource::<TextCapture>()
            .init_resource::<MenuOpen>()
            .insert_resource(RebuildTimer(Timer::from_seconds(REBUILD_SECS, TimerMode::Repeating)))
            .add_systems(Startup, (spawn_panels, setup_minimap))
            .add_systems(
                Update,
                (
                    pointer_over_ui,
                    hover_cell,
                    hotkeys,
                    search_input,
                    world_input,
                    button_actions,
                    minimap_click,
                    track_good_hover,
                    poll_pending_works,
                    sync_lens,
                    update_minimap,
                    rebuild_panels,
                    draw_world_overlay,
                )
                    .chain()
                    .in_set(HudSet),
            )
            .add_systems(Update, demo.before(pointer_over_ui));
    }
}

// ───────────────────────────── building blocks ─────────────────────────────

pub(crate) fn font(size: f32, bold: bool) -> TextFont {
    TextFont { font_size: FontSize::Px(size), weight: if bold { FontWeight::BOLD } else { FontWeight::NORMAL }, ..default() }
}

pub(crate) fn text(p: &mut ChildSpawnerCommands, s: impl Into<String>, size: f32, color: Color) {
    p.spawn((Text::new(s.into()), font(size, false), TextColor(color)));
}

pub(crate) fn bold(p: &mut ChildSpawnerCommands, s: impl Into<String>, size: f32, color: Color) {
    p.spawn((Text::new(s.into()), font(size, true), TextColor(color)));
}

pub(crate) fn row(gap: f32) -> Node {
    Node { flex_direction: FlexDirection::Row, align_items: AlignItems::Center, column_gap: Val::Px(gap), ..default() }
}

pub(crate) fn swatch(p: &mut ChildSpawnerCommands, c: Color, w: f32, h: f32) {
    p.spawn((Node { width: Val::Px(w), height: Val::Px(h), border_radius: BorderRadius::all(Val::Px(2.0)), ..default() }, BackgroundColor(c)));
}

/// A button: `fill` is the background (`ACCENT` for fixes), `on` draws the selection outline.
fn button(p: &mut ChildSpawnerCommands, label: impl Into<String>, action: HudAction, fill: Color, on: bool) {
    let ink = if fill == ACCENT { Color::srgb(0.13, 0.09, 0.05) } else { INK };
    p.spawn((
        Interaction::default(),
        FocusPolicy::Block,
        Blocks,
        action,
        Node {
            padding: UiRect::axes(Val::Px(8.0), Val::Px(3.0)),
            border: UiRect::all(Val::Px(if on { 2.0 } else { 1.0 })),
            border_radius: BorderRadius::all(Val::Px(4.0)),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            ..default()
        },
        BackgroundColor(fill),
        BorderColor::all(if on { Color::WHITE } else { Color::srgba(1.0, 1.0, 1.0, 0.14) }),
    ))
    .with_children(|b| bold(b, label, 11.0, ink));
}

fn panel_node(pos: UiRect, width: Option<f32>, flex: FlexDirection) -> Node {
    Node {
        position_type: PositionType::Absolute,
        left: pos.left,
        right: pos.right,
        top: pos.top,
        bottom: pos.bottom,
        width: width.map_or(Val::Auto, Val::Px),
        flex_direction: flex,
        align_items: if flex == FlexDirection::Row { AlignItems::Center } else { AlignItems::Stretch },
        padding: UiRect::all(Val::Px(8.0)),
        row_gap: Val::Px(6.0),
        column_gap: Val::Px(8.0),
        border: UiRect { top: Val::Px(2.0), ..UiRect::all(Val::Px(1.0)) },
        border_radius: BorderRadius::all(Val::Px(7.0)),
        ..default()
    }
}

fn spawn_panels(mut commands: Commands) {
    let edge = |l: Option<f32>, r: Option<f32>, t: Option<f32>, b: Option<f32>| UiRect {
        left: l.map_or(Val::Auto, Val::Px),
        right: r.map_or(Val::Auto, Val::Px),
        top: t.map_or(Val::Auto, Val::Px),
        bottom: b.map_or(Val::Auto, Val::Px),
    };
    let border = BorderColor { top: TIMBER, ..BorderColor::all(Color::srgba(1.0, 1.0, 1.0, 0.06)) };
    let mut spawn = |panel: Panel, node: Node| {
        let centred = node.left == Val::Px(0.0) && node.right == Val::Px(0.0);
        let bg = if panel == Panel::Problems { Color::NONE } else { PANEL };
        let bd = if panel == Panel::Problems { BorderColor::all(Color::NONE) } else { border };
        if centred {
            // A transparent full-width strip that centres the panel and lets the pointer through.
            let wrap = Node { position_type: PositionType::Absolute, left: Val::Px(0.0), right: Val::Px(0.0), bottom: node.bottom, justify_content: JustifyContent::Center, ..default() };
            let inner = Node { position_type: PositionType::Relative, left: Val::Auto, right: Val::Auto, bottom: Val::Auto, ..node };
            commands.spawn((wrap, FocusPolicy::Pass)).with_children(|w| {
                w.spawn((panel, inner, BackgroundColor(bg), bd, Interaction::default(), FocusPolicy::Block, Blocks, Visibility::Hidden));
            });
        } else {
            commands.spawn((panel, node, BackgroundColor(bg), bd, Interaction::default(), FocusPolicy::Block, Blocks, Visibility::Hidden));
        }
    };
    spawn(Panel::Ledger, panel_node(edge(Some(12.0), None, Some(10.0), None), None, FlexDirection::Row));
    spawn(Panel::LedgerTip, panel_node(edge(Some(12.0), None, Some(62.0), None), Some(300.0), FlexDirection::Column));
    spawn(Panel::Calendar, panel_node(edge(None, Some(12.0), Some(10.0), None), Some(360.0), FlexDirection::Column));
    spawn(Panel::Problems, Node { padding: UiRect::ZERO, border: UiRect::ZERO, ..panel_node(edge(None, Some(12.0), Some(124.0), None), Some(360.0), FlexDirection::Column) });
    spawn(Panel::ToolCard, panel_node(edge(None, Some(12.0), Some(124.0), None), Some(360.0), FlexDirection::Column));
    spawn(Panel::Inspector, panel_node(edge(None, Some(12.0), Some(124.0), None), Some(360.0), FlexDirection::Column));
    spawn(Panel::Rail, panel_node(edge(Some(12.0), None, Some(200.0), None), Some(116.0), FlexDirection::Column));
    spawn(Panel::FlowLedger, panel_node(edge(Some(140.0), Some(384.0), Some(62.0), None), None, FlexDirection::Column));
    spawn(Panel::Minimap, panel_node(edge(Some(12.0), None, None, Some(12.0)), Some(176.0), FlexDirection::Column));
    spawn(Panel::BuildBar, panel_node(edge(Some(0.0), Some(0.0), None, Some(12.0)), None, FlexDirection::Row));
    spawn(Panel::Items, panel_node(edge(Some(0.0), Some(0.0), None, Some(70.0)), None, FlexDirection::Row));
    spawn(Panel::Profile, panel_node(edge(Some(0.0), Some(0.0), None, Some(150.0)), Some(600.0), FlexDirection::Column));
    spawn(Panel::Toast, panel_node(edge(Some(0.0), Some(0.0), None, Some(70.0)), None, FlexDirection::Row));
}

// ───────────────────────────── input ─────────────────────────────

fn pointer_over_ui(q: Query<&Interaction, With<Blocks>>, mut over: ResMut<PointerOverUi>) {
    let v = q.iter().any(|i| *i != Interaction::None);
    if over.0 != v {
        over.0 = v;
    }
}

fn hover_cell(
    windows: Query<&Window, With<PrimaryWindow>>,
    cams: Query<(&Camera, &GlobalTransform), With<MainCamera>>,
    sim: Res<Sim>,
    over: Res<PointerOverUi>,
    mut hover: ResMut<HoverCell>,
) {
    let cell = (|| {
        if over.0 {
            return None;
        }
        let cursor = windows.single().ok()?.cursor_position()?;
        let (cam, gt) = cams.single().ok()?;
        let ray = cam.viewport_to_world(gt, cursor).ok()?;
        ray_heightfield(ray.origin, *ray.direction, &sim.0.height).map(|(c, _, _)| c)
    })();
    if hover.0 != cell {
        hover.0 = cell;
    }
}

fn hotkeys(
    keys: Res<ButtonInput<KeyCode>>,
    sim: Res<Sim>,
    typing: Res<TextCapture>,
    mut hud: ResMut<HudState>,
    mut sel: ResMut<Selection>,
    mut lens: ResMut<MapLens>,
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
    if keys.just_pressed(KeyCode::KeyL) {
        hud.ledger_open = !hud.ledger_open;
    }
    if keys.just_pressed(KeyCode::Slash) {
        hud.search = Some(String::new());
        return;
    }
    if keys.just_pressed(KeyCode::Escape) && hud.ledger_open {
        hud.ledger_open = false;
        return;
    }
    if keys.just_pressed(KeyCode::Escape) {
        if hud.tool != Tool::None {
            hud.set_tool(Tool::None);
        } else if hud.open_cat.is_some() {
            hud.open_cat = None;
        } else if *sel != Selection::None {
            *sel = Selection::None;
        }
    }
    let n = categories(&sim.0).len();
    for (i, k) in CAT_KEYS.iter().enumerate().take(n) {
        if !ctrl && keys.just_pressed(*k) {
            hud.open_cat = if hud.open_cat == Some(i) { None } else { Some(i) };
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

#[allow(clippy::too_many_arguments)]
fn world_input(
    mouse: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
    scroll: Res<AccumulatedMouseScroll>,
    time: Res<Time>,
    hover: Res<HoverCell>,
    over: Res<PointerOverUi>,
    mut sim: ResMut<Sim>,
    mut hud: ResMut<HudState>,
    mut sel: ResMut<Selection>,
    mut pending: ResMut<PendingWorks>,
    menu: Res<MenuOpen>,
) {
    if menu.0 {
        return;
    }
    let now = time.elapsed_secs();
    // Ctrl + wheel sets the dam crest (the plain wheel keeps zooming).
    if matches!(hud.tool, Tool::Dam) && keys.pressed(KeyCode::ControlLeft) && scroll.delta.y != 0.0 {
        let lines = match scroll.unit {
            MouseScrollUnit::Line => scroll.delta.y,
            MouseScrollUnit::Pixel => scroll.delta.y / 40.0,
        };
        hud.dam_height = (hud.dam_height + 0.25 * lines.signum()).clamp(MIN_DAM_HEIGHT, MAX_DAM_HEIGHT);
    }
    if mouse.just_pressed(MouseButton::Right) && hud.tool != Tool::None && !over.0 {
        hud.set_tool(Tool::None);
        return;
    }
    let ctrl = keys.pressed(KeyCode::ControlLeft) || keys.pressed(KeyCode::ControlRight);
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
    let Some(cell) = hover.0 else { return };
    match hud.tool {
        Tool::None => {
            if mouse.just_pressed(MouseButton::Left) {
                *sel = match building_at(&sim.0, cell) {
                    Some(id) => Selection::Site(id),
                    None => Selection::None,
                };
            }
        }
        Tool::Building(kind) => {
            if mouse.just_pressed(MouseButton::Left) {
                let p = build_logic::site_preview(&sim.0, kind, cell);
                match build_logic::emit(&mut sim.0, &mut pending.0, &BuildOrder::Building { kind, pos: p.pos }) {
                    Ok(m) => {
                        if let Some(b) = sim.0.buildings.buildings.last() {
                            hud.undo.push(UndoItem::Site(b.id));
                        }
                        hud.say(format!("{m} (Ctrl+Z undoes)"), now)
                    }
                    Err(e) => hud.say(e.to_string(), now),
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
            }
        }
    }
}

fn commit_line(sim: &mut sim::SimState, hud: &mut HudState, pending: &mut Pending, now: f32) {
    if hud.line.is_empty() {
        hud.say("Drag across the water first", now);
        return;
    }
    let order = match hud.tool {
        Tool::Dam => BuildOrder::Dam { cells: hud.line.clone(), height: hud.dam_height },
        Tool::Canal => BuildOrder::Canal { cells: hud.line.clone() },
        _ => return,
    };
    let canal = matches!(order, BuildOrder::Canal { .. });
    match build_logic::emit(sim, pending, &order) {
        Ok(m) => {
            let item = if canal { pending.0.last().map(|w| UndoItem::Canal(w.order)) } else { sim.buildings.buildings.last().map(|b| UndoItem::Site(b.id)) };
            hud.undo.extend(item);
            hud.say(format!("{m} (Ctrl+Z undoes)"), now);
            hud.line.clear();
            hud.preview = None;
        }
        Err(e) => hud.say(e.to_string(), now),
    }
}

/// Takes back the newest placement that is still unfinished, refunding what was delivered.
fn undo_last(sim: &mut sim::SimState, hud: &mut HudState, pending: &mut Pending, now: f32) {
    while let Some(item) = hud.undo.pop() {
        match item {
            UndoItem::Site(id) => {
                let Some(name) = sim.buildings.building(id).map(|b| b.kind.spec().name) else { continue };
                match sim.cancel_site(id) {
                    Ok(back) => {
                        let back: Vec<String> = back.iter().filter(|a| a.1 > 0.05).map(|a| format!("{:.0} {}", a.1, good_name(a.0).to_lowercase())).collect();
                        let refund = if back.is_empty() { String::new() } else { format!(", {} back in store", back.join(", ")) };
                        hud.say(format!("Undid the {}{refund}", name.to_lowercase()), now);
                    }
                    Err(e) => hud.say(format!("Cannot undo: {e}"), now),
                }
                return;
            }
            UndoItem::Canal(order) => {
                if build_logic::cancel(sim, pending, build_logic::OrderTarget::Pending(order)).is_ok() {
                    hud.say("Undid the canal, materials refunded", now);
                    return;
                }
            }
        }
    }
    hud.say("Nothing unfinished to undo", now);
}

fn building_at(sim: &sim::SimState, cell: Cell) -> Option<BuildingId> {
    sim.buildings.buildings.iter().find(|b| b.cells().any(|c| c == cell)).map(|b| b.id)
}

#[allow(clippy::too_many_arguments)]
fn button_actions(
    q: Query<(&Interaction, &HudAction), Changed<Interaction>>,
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
    let actions: Vec<HudAction> = q.iter().filter(|(i, _)| **i == Interaction::Pressed).map(|(_, a)| a.clone()).collect();
    for a in actions {
        let s = &mut sim.0;
        match a {
            HudAction::Fix(fix) => apply_fix(s, &mut hud, fix, now),
            HudAction::Focus(c) => flow.focus = Some(cell_world(&s.height, (c.0 as f32, c.1 as f32))),
            HudAction::CycleIdle => {
                let idle = sim::hud::idle_beavers(s);
                if !idle.is_empty() {
                    let i = hud.idle_cursor % idle.len();
                    hud.idle_cursor = i + 1;
                    if let Some(b) = s.colony.beaver(idle[i]) {
                        flow.focus = Some(cell_world(&s.height, b.pos));
                    }
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
            HudAction::OpenCat(i) => hud.open_cat = if hud.open_cat == Some(i) { None } else { Some(i) },
            HudAction::Pick(kind) => {
                let t = match kind {
                    BuildingKind::Dam => Tool::Dam,
                    BuildingKind::Canal => Tool::Canal,
                    k => Tool::Building(k),
                };
                let next = if hud.tool == t { Tool::None } else { t };
                hud.set_tool(next);
                *sel = Selection::None;
            }
            HudAction::CancelTool => hud.set_tool(Tool::None),
            HudAction::DamHeight(d) => hud.dam_height = (hud.dam_height + d).clamp(MIN_DAM_HEIGHT, MAX_DAM_HEIGHT),
            HudAction::ConfirmLine => commit_line(s, &mut hud, &mut pending.0, now),
            HudAction::ClearLine => {
                hud.line.clear();
                hud.preview = None;
            }
            HudAction::CloseInspector => *sel = Selection::None,
            HudAction::FillSlot(b, slot) => apply_fix(s, &mut hud, Fix::FillSlot { building: b, slot }, now),
            HudAction::SetLens(l) => {
                *lens = l;
                hud.set_changed();
            }
            HudAction::ToggleLedger => hud.ledger_open = !hud.ledger_open,
            HudAction::Undo => undo_last(s, &mut hud, &mut pending.0, now),
            HudAction::OpenSearch => hud.search = Some(String::new()),
            HudAction::EmptySlot(b, slot) => {
                if let Err(e) = s.apply(sim::Command::EmptySlot { building: b, slot }) {
                    hud.say(e.to_string(), now);
                }
            }
        }
    }
}

/// Turns a problem's fix into commands.
fn apply_fix(s: &mut sim::SimState, hud: &mut HudState, fix: Fix, now: f32) {
    let idle = sim::hud::idle_beavers(s);
    let res = match fix {
        Fix::FillSlot { building, slot } => match idle.first() {
            Some(&who) => s.apply(sim::Command::FillSlot { building, slot, who }).map(|_| "A beaver took the job".to_string()),
            None => Ok("Nobody is idle".to_string()),
        },
        Fix::Gather(kind) => {
            let centre = s.colony.beavers.first().map(|b| (b.pos.0.max(0.0) as usize, b.pos.1.max(0.0) as usize)).unwrap_or((0, 0));
            let r = 10;
            let max = s.world.size - 1;
            let area = sim::work::Area { min: (centre.0.saturating_sub(r), centre.1.saturating_sub(r)), max: ((centre.0 + r).min(max), (centre.1 + r).min(max)) };
            let n = idle.len();
            s.apply(sim::Command::Assign { who: idle, to: Some(sim::agents::Assignment::Area(kind, area)) }).map(|_| format!("{n} beavers sent to gather"))
        }
        Fix::Place(kind) => {
            hud.set_tool(if kind == BuildingKind::Dam { Tool::Dam } else { Tool::Building(kind) });
            Ok(format!("Place the {}", kind.spec().name.to_lowercase()))
        }
        Fix::Accept(id) => s.apply(sim::Command::AnswerJoin { id, answer: sim::society::Answer::Accept }).map(|_| "Welcomed".to_string()),
        Fix::None => Ok(String::new()),
    };
    match res {
        Ok(m) if !m.is_empty() => hud.say(m, now),
        Ok(_) => {}
        Err(e) => hud.say(e.to_string(), now),
    }
}

fn track_good_hover(q: Query<(&Interaction, &GoodItem)>, mut hg: ResMut<HoverGood>) {
    let g = q.iter().find(|(i, _)| **i != Interaction::None).map(|(_, g)| g.0);
    if hg.0 != g {
        hg.0 = g;
    }
}

fn poll_pending_works(mut sim: ResMut<Sim>, mut pending: ResMut<PendingWorks>, mut hud: ResMut<HudState>, time: Res<Time>) {
    if pending.0 .0.is_empty() {
        return;
    }
    for m in build_logic::poll_pending(&mut sim.0, &mut pending.0) {
        hud.say(m, time.elapsed_secs());
    }
}

// ───────────────────────────── panels ─────────────────────────────

fn categories(sim: &sim::SimState) -> Vec<Group> {
    let mut out: Vec<Group> = Vec::new();
    for e in build_logic::menu(sim) {
        if !out.contains(&e.group) {
            out.push(e.group);
        }
    }
    out
}

#[allow(clippy::too_many_arguments)]
fn rebuild_panels(
    mut commands: Commands,
    time: Res<Time>,
    mut timer: ResMut<RebuildTimer>,
    sim: Res<Sim>,
    mut hud: ResMut<HudState>,
    clock: Res<SimClock>,
    sel: Res<Selection>,
    hover: Res<HoverCell>,
    hg: Res<HoverGood>,
    lens: Res<MapLens>,
    orbit: Res<crate::camera::OrbitCamera>,
    minimap: Option<Res<MinimapImage>>,
    menu: Res<MenuOpen>,
    mut panels: Query<(Entity, &Panel, &mut Visibility)>,
) {
    let due = timer.0.tick(time.delta()).just_finished();
    let all = due || hud.is_changed() || sel.is_changed() || lens.is_changed() || menu.is_changed() || (hover.is_changed() && hud.tool != Tool::None);
    // A hovered good only redraws its tooltip, so the strip under the pointer is never rebuilt
    // while it is being hovered.
    let tip_only = !all && hg.is_changed();
    if !all && !tip_only {
        return;
    }
    let s = &sim.0;
    let now = time.elapsed_secs();
    if hud.msg.as_ref().is_some_and(|(_, t)| now - t > MSG_SECS) {
        hud.msg = None;
    }
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
    for (e, panel, mut vis) in &mut panels {
        if tip_only && *panel != Panel::LedgerTip {
            continue;
        }
        let show = match panel {
            Panel::Ledger | Panel::Calendar | Panel::BuildBar => s.colony.is_founded(),
            Panel::LedgerTip => hg.0.is_some(),
            Panel::Problems => !problems.is_empty() && hud.tool == Tool::None && *sel == Selection::None && !hud.ledger_open,
            Panel::Items => hud.open_cat.is_some() || hud.search.is_some(),
            Panel::ToolCard => hud.tool != Tool::None,
            Panel::Profile => hud.tool == Tool::Dam,
            Panel::Inspector => matches!(*sel, Selection::Site(_)) && hud.tool == Tool::None,
            Panel::Toast => hud.msg.is_some(),
            Panel::Rail | Panel::Minimap => s.colony.is_founded(),
            Panel::FlowLedger => hud.ledger_open,
        } && !menu.0;
        let want = if show { Visibility::Inherited } else { Visibility::Hidden };
        if *vis != want {
            *vis = want;
        }
        if !show {
            continue;
        }
        commands.entity(e).despawn_related::<Children>();
        commands.entity(e).with_children(|p| match panel {
            Panel::Ledger => ledger(p, s),
            Panel::LedgerTip => ledger_tip(p, s, hg.0.unwrap()),
            Panel::Calendar => calendar_panel(p, s, &clock),
            Panel::Problems => problem_cards(p, &problems),
            Panel::BuildBar => build_bar(p, s, hud),
            Panel::Items => items_row(p, s, hud),
            Panel::ToolCard => tool_card(p, s, hud, hover.0),
            Panel::Profile => profile_panel(p, s, hud),
            Panel::Inspector => {
                if let Selection::Site(id) = *sel {
                    inspector(p, s, id)
                }
            }
            Panel::Toast => {
                if let Some((m, _)) = &hud.msg {
                    text(p, m.clone(), 12.0, INK)
                }
            }
            Panel::Rail => rail(p, hud, *lens),
            Panel::FlowLedger => flow_ledger(p, s),
            Panel::Minimap => {
                if let Some(img) = minimap.as_ref() {
                    minimap_panel(p, s, &img.0, orbit.target)
                }
            }
        });
    }
}

fn fmt_rate(r: f32) -> (String, Color) {
    if r > 0.05 {
        (format!("+{r:.1}/d"), GOOD)
    } else if r < -0.05 {
        (format!("{r:.1}/d"), BAD)
    } else {
        ("0".into(), MUTED)
    }
}

fn ledger(p: &mut ChildSpawnerCommands, s: &sim::SimState) {
    let stores = &s.buildings.stores;
    let extra = [Good::Baskets, Good::Rope, Good::Tools].into_iter().filter(|g| stores.amount(*g) > 0.0);
    for g in STRIP_GOODS.into_iter().chain(extra) {
        let have = stores.amount(g);
        let (rate, rc) = fmt_rate(s.ledger.net_per_day(g));
        let lasts = if g == Good::Food { food_days(s) } else { s.ledger.days_left(g, have) };
        p.spawn((GoodItem(g), Interaction::default(), Blocks, row(4.0))).with_children(|r| {
            swatch(r, good_color(g), 8.0, 14.0);
            text(r, good_name(g), 11.0, MUTED);
            bold(r, format!("{}", have.round() as i64), 14.0, INK);
            if rc != MUTED {
                text(r, rate, 11.0, rc);
            }
            if let Some(d) = lasts.filter(|d| *d < 60.0) {
                let c = if d < 7.0 { BAD } else { WARN };
                let label = if have < 0.5 { "empty".to_string() } else { format!("lasts {}d", d.floor() as i64) };
                text(r, label, 10.0, c);
            }
        });
    }
    // Beavers and the idle count (click cycles the camera through them).
    let idle = sim::hud::idle_beavers(s).len();
    p.spawn(row(5.0)).with_children(|r| {
        swatch(r, Color::srgb(0.65, 0.48, 0.32), 10.0, 10.0);
        bold(r, format!("{} beavers", s.colony.population()), 14.0, INK);
        if idle > 0 {
            button(r, format!("{idle} idle"), HudAction::CycleIdle, Color::srgb(0.45, 0.36, 0.12), false);
        }
    });
}

/// Days the food lasts at what the colony needs, after what gatherers bring.
fn food_days(s: &sim::SimState) -> Option<f32> {
    let need = sim::hud::food_need_per_day(s);
    let gain = s.ledger.in_per_day(Good::Food);
    (need > gain + 1e-3).then(|| s.buildings.stores.amount(Good::Food) / (need - gain))
}

fn ledger_tip(p: &mut ChildSpawnerCommands, s: &sim::SimState, g: Good) {
    let st = &s.buildings.stores;
    let (inn, out) = (s.ledger.in_per_day(g), s.ledger.out_per_day(g));
    p.spawn(row(6.0)).with_children(|r| {
        swatch(r, good_color(g), 10.0, 10.0);
        bold(r, format!("{} - {} of {} stored", good_name(g), st.amount(g).round() as i64, st.capacity(g).round() as i64), 13.0, INK);
    });
    let line = |p: &mut ChildSpawnerCommands, v: String, c: Color, what: String| {
        p.spawn(row(8.0)).with_children(|r| {
            r.spawn(Node { width: Val::Px(52.0), justify_content: JustifyContent::FlexEnd, ..default() }).with_children(|b| bold(b, v, 12.0, c));
            text(r, what, 12.0, INK);
        });
    };
    line(p, format!("+{inn:.1}"), GOOD, "brought in per day (7-day average)".into());
    if g == Good::Food {
        let need = sim::hud::food_need_per_day(s);
        line(p, format!("-{need:.1}"), BAD, format!("eaten per day by {} beavers", s.colony.population()));
        match food_days(s) {
            Some(d) => line(p, format!("{:.0} d", d), WARN, "until the store is empty".into()),
            None => line(p, "ok".into(), GOOD, "gatherers keep up with eating".into()),
        }
        let gap = need - inn;
        if gap > 0.05 {
            p.spawn((Node { padding: UiRect::all(Val::Px(5.0)), border_radius: BorderRadius::all(Val::Px(4.0)), ..default() }, BackgroundColor(FAINT)))
                .with_children(|b| text(b, format!("To break even: +{gap:.1} food/day"), 11.0, INK));
        }
    } else {
        line(p, format!("-{out:.1}"), BAD, "used per day (building, crafting)".into());
        if let Some(d) = s.ledger.days_left(g, st.amount(g)) {
            line(p, format!("{:.0} d", d), WARN, "until the store is empty".into());
        }
    }
    if !s.ledger.has_data() {
        text(p, "Rates appear after the first whole day.", 10.0, MUTED);
    }
}

fn season_color(se: Season) -> Color {
    match se {
        Season::Spring => Color::srgb(0.36, 0.62, 0.8),
        Season::Summer => Color::srgb(0.84, 0.68, 0.35),
        Season::Autumn => Color::srgb(0.6, 0.48, 0.36),
        Season::Winter => Color::srgb(0.78, 0.82, 0.88),
    }
}

fn calendar_panel(p: &mut ChildSpawnerCommands, s: &sim::SimState, clock: &SimClock) {
    let t = s.time_days;
    let d = Date::at(t);
    p.spawn(row(8.0)).with_children(|r| {
        bold(r, format!("{} {}", d.season.name(), d.day), 14.0, INK);
        text(r, format!("Year {}", d.year), 11.0, MUTED);
        r.spawn(Node { flex_grow: 1.0, ..default() });
        button(r, "||", HudAction::Pause, if clock.paused { Color::srgb(0.94, 0.91, 0.86) } else { FAINT }, false);
        for sp in Speed::ALL {
            button(r, sp.label(), HudAction::Speed(sp), FAINT, !clock.paused && clock.speed == sp);
        }
    });
    // The year ribbon: four seasons, a white tick for today.
    let phase = calendar::year_phase(t);
    p.spawn(Node { width: Val::Percent(100.0), height: Val::Px(12.0), flex_direction: FlexDirection::Row, column_gap: Val::Px(2.0), ..default() })
        .with_children(|r| {
            for se in Season::ALL {
                r.spawn((Node { flex_grow: 1.0, height: Val::Percent(100.0), border_radius: BorderRadius::all(Val::Px(2.0)), ..default() }, BackgroundColor(season_color(se))));
            }
            r.spawn((
                Node { position_type: PositionType::Absolute, left: Val::Percent(phase * 100.0), top: Val::Px(-2.0), width: Val::Px(2.0), height: Val::Px(16.0), ..default() },
                BackgroundColor(Color::WHITE),
            ));
        });
    let outlook = match d.season {
        Season::Summer => format!("Dry summer: ponds evaporate fastest - {} d left", calendar::days_until(t, Season::Autumn).ceil() as i64),
        Season::Winter => format!("Winter: ice, little inflow - spring in {} d", calendar::days_until(t, Season::Spring).ceil() as i64),
        _ => format!("Dry summer in {} d", calendar::days_until(t, Season::Summer).ceil() as i64),
    };
    text(p, outlook, 11.0, MUTED);
    // Pond cover: stored water against a summer's losses.
    let ponds = sim::hud::ponds(s);
    p.spawn((Node { padding: UiRect::axes(Val::Px(6.0), Val::Px(3.0)), border: UiRect::all(Val::Px(1.0)), border_radius: BorderRadius::all(Val::Px(4.0)), column_gap: Val::Px(6.0), ..row(6.0) }, BackgroundColor(WATER.with_alpha(0.15)), BorderColor::all(WATER.with_alpha(0.45))))
        .with_children(|r| {
            swatch(r, WATER, 8.0, 10.0);
            match ponds.summer_cover_days() {
                Some(days) => {
                    let ok = days >= calendar::SEASON_DAYS as f32;
                    bold(r, format!("Ponds hold {:.0}: {:.0} of {:.0} summer days", ponds.volume, days.min(999.0), calendar::SEASON_DAYS), 11.0, INK);
                    text(r, if ok { "ok" } else { "short" }, 11.0, if ok { GOOD } else { WARN });
                }
                None => text(r, "No dam yet: dam the river to store water", 11.0, INK),
            }
        });
}

fn problem_cards(p: &mut ChildSpawnerCommands, problems: &[Problem]) {
    for pr in problems {
        let c = if pr.severity == Severity::Urgent { BAD } else { WARN };
        p.spawn((
            Node {
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(3.0),
                padding: UiRect::all(Val::Px(8.0)),
                border: UiRect::left(Val::Px(3.0)),
                border_radius: BorderRadius::all(Val::Px(6.0)),
                margin: UiRect::bottom(Val::Px(6.0)),
                ..default()
            },
            BackgroundColor(PANEL),
            BorderColor::all(c),
        ))
        .with_children(|card| {
            card.spawn(row(6.0)).with_children(|r| {
                text(r, if pr.severity == Severity::Urgent { "!!" } else { "!" }, 11.0, c);
                bold(r, pr.title.clone(), 13.0, INK);
            });
            text(card, pr.cause.clone(), 11.0, MUTED);
            card.spawn(Node { margin: UiRect::top(Val::Px(4.0)), ..row(5.0) }).with_children(|r| {
                if pr.fix != Fix::None {
                    button(r, pr.fix_label.clone(), HudAction::Fix(pr.fix), ACCENT, false);
                }
                if let Some(f) = pr.focus {
                    button(r, "Show >", HudAction::Focus(f), FAINT, false);
                }
            });
        });
    }
}

fn build_bar(p: &mut ChildSpawnerCommands, s: &sim::SimState, hud: &HudState) {
    // Centre the bar: the panel spans the width, its content sits in the middle.
    for (i, g) in categories(s).into_iter().enumerate() {
        let key = CAT_KEY_LABELS.get(i).copied().unwrap_or("");
        let colour = match g {
            Group::Base => Color::srgb(0.65, 0.62, 0.55),
            Group::Branch(sim::tech::Branch::Water) => WATER,
            Group::Branch(sim::tech::Branch::Forest) => Color::srgb(0.5, 0.69, 0.41),
            Group::Branch(sim::tech::Branch::Building) => Color::srgb(0.79, 0.56, 0.42),
            Group::Branch(sim::tech::Branch::Craft) => Color::srgb(0.78, 0.63, 0.29),
            Group::Branch(_) => Color::srgb(0.55, 0.52, 0.46),
        };
        let open = hud.open_cat == Some(i);
        p.spawn((
            Interaction::default(),
            FocusPolicy::Block,
            Blocks,
            HudAction::OpenCat(i),
            Node {
                width: Val::Px(84.0),
                height: Val::Px(38.0),
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                border: UiRect { top: Val::Px(3.0), ..UiRect::all(Val::Px(if open { 2.0 } else { 0.0 })) },
                border_radius: BorderRadius::all(Val::Px(5.0)),
                ..default()
            },
            BackgroundColor(if open { Color::srgba(1.0, 1.0, 1.0, 0.14) } else { FAINT }),
            BorderColor { top: colour, ..BorderColor::all(if open { Color::WHITE } else { Color::NONE }) },
        ))
        .with_children(|b| {
            bold(b, g.name(), 12.0, INK);
            text(b, key, 9.0, MUTED);
        });
    }
}

fn items_row(p: &mut ChildSpawnerCommands, s: &sim::SimState, hud: &HudState) {
    let entries: Vec<build_logic::MenuEntry> = if let Some(q) = &hud.search {
        p.spawn(Node { width: Val::Px(150.0), flex_direction: FlexDirection::Column, row_gap: Val::Px(2.0), ..default() }).with_children(|c| {
            bold(c, format!("Search: {q}_"), 12.0, INK);
            text(c, "Enter picks the first - Esc closes", 9.5, MUTED);
        });
        let found = search_matches(s, q);
        if found.is_empty() {
            text(p, "No building matches", 11.0, MUTED);
        }
        found
    } else {
        let Some(cat) = hud.open_cat else { return };
        let cats = categories(s);
        let Some(group) = cats.get(cat) else { return };
        build_logic::menu(s).into_iter().filter(|e| e.group == *group).collect()
    };
    for e in entries {
        let kind = e.kind;
        let selected = match hud.tool {
            Tool::Building(k) => k == kind,
            Tool::Dam => kind == BuildingKind::Dam,
            Tool::Canal => kind == BuildingKind::Canal,
            Tool::None => false,
        };
        let cost = if kind == BuildingKind::Dam { "per 6 cells: ".to_string() + &build_logic::cost_text(kind.spec().cost) } else { build_logic::cost_text(kind.spec().cost) };
        p.spawn((
            Interaction::default(),
            FocusPolicy::Block,
            Blocks,
            HudAction::Pick(kind),
            Node {
                width: Val::Px(112.0),
                flex_direction: FlexDirection::Column,
                padding: UiRect::all(Val::Px(6.0)),
                row_gap: Val::Px(2.0),
                border: UiRect::all(Val::Px(if selected { 2.0 } else { 0.0 })),
                border_radius: BorderRadius::all(Val::Px(5.0)),
                ..default()
            },
            BackgroundColor(if selected { Color::srgba(1.0, 1.0, 1.0, 0.14) } else { FAINT }),
            BorderColor::all(Color::WHITE),
        ))
        .with_children(|b| {
            bold(b, kind.spec().name, 12.0, if e.unlocked { INK } else { MUTED });
            match e.need {
                Some((tech, need, _)) if need <= 0.0 => text(b, format!("Locked: needs {tech}"), 9.5, MUTED),
                Some((tech, need, have)) => text(b, format!("Locked: {tech} {:.0}/{:.0} xp", have, need), 9.5, MUTED),
                None => text(b, if cost.is_empty() { "free".to_string() } else { cost }, 9.5, MUTED),
            }
        });
    }
}

fn kv(p: &mut ChildSpawnerCommands, k: impl Into<String>, v: impl Into<String>, vc: Color) {
    p.spawn(Node { justify_content: JustifyContent::SpaceBetween, ..row(8.0) }).with_children(|r| {
        text(r, k, 11.5, MUTED);
        bold(r, v, 11.5, vc);
    });
}

fn hint(p: &mut ChildSpawnerCommands, s: &str) {
    text(p, s.to_string(), 10.0, MUTED);
}

fn tool_card(p: &mut ChildSpawnerCommands, s: &sim::SimState, hud: &HudState, hover: Option<Cell>) {
    match hud.tool {
        Tool::None => {}
        Tool::Building(kind) => {
            bold(p, kind.spec().name, 14.0, INK);
            kv(p, "Materials (hauled in by builders)", build_logic::cost_text(kind.spec().cost), INK);
            let (w, h) = kind.spec().footprint;
            kv(p, "Footprint", format!("{w} x {h} cells"), INK);
            match hover.map(|c| build_logic::site_preview(s, kind, c)) {
                Some(pv) if pv.is_valid() => kv(p, "Here", "can be placed ok", GOOD),
                Some(pv) => kv(p, "Here", pv.verdict.unwrap_err().to_string(), BAD),
                None => kv(p, "Here", "point at the ground", MUTED),
            }
            hint(p, "Click to place - keeps the tool - RMB or Esc to stop");
            p.spawn(row(5.0)).with_children(|r| button(r, "Done", HudAction::CancelTool, FAINT, false));
        }
        Tool::Canal => {
            bold(p, format!("Canal - {} cells", hud.line.len()), 14.0, INK);
            if hud.line.is_empty() {
                hint(p, "Drag from water across the land to draw the canal");
            } else {
                kv(p, "Cost", build_logic::cost_text(&build_logic::path_cost(BuildingKind::Canal, hud.line.len())), INK);
                match build_logic::check_canal(s, &hud.line) {
                    Ok(()) => kv(p, "Check", "ready ok", GOOD),
                    Err(e) => kv(p, "Check", e.to_string(), BAD),
                }
            }
            p.spawn(row(5.0)).with_children(|r| {
                if !hud.line.is_empty() {
                    button(r, "Dig canal", HudAction::ConfirmLine, ACCENT, false);
                    button(r, "Clear", HudAction::ClearLine, FAINT, false);
                }
                button(r, "Done", HudAction::CancelTool, FAINT, false);
            });
            hint(p, "Enter digs - RMB or Esc to stop");
        }
        Tool::Dam => {
            bold(p, format!("Dam - {} cells - crest {:.2}", hud.line.len(), hud.dam_height), 14.0, INK);
            p.spawn(row(5.0)).with_children(|r| {
                text(r, "Crest", 11.0, MUTED);
                button(r, "-", HudAction::DamHeight(-0.25), FAINT, false);
                button(r, "+", HudAction::DamHeight(0.25), FAINT, false);
                text(r, format!("{MIN_DAM_HEIGHT}-{MAX_DAM_HEIGHT} - [ ] or Ctrl+wheel"), 10.0, MUTED);
            });
            if hud.line.is_empty() {
                hint(p, "Drag across the river to draw the dam. The pond it makes is drawn before you build.");
            } else {
                match build_logic::check_dam(s, &hud.line, hud.dam_height) {
                    Ok(()) => {}
                    Err(e) => kv(p, "Check", e.to_string(), BAD),
                }
                match hud.preview.as_ref().and_then(|(_, _, pv)| pv.as_ref()) {
                    Some(pv) => {
                        let r = build_logic::DamReadout::from_preview(pv);
                        kv(p, "Water rises", format!("+{:.2} (to level {:.2})", r.rise, r.level), WATER);
                        let new = pv.flooded.iter().filter(|&&(x, y)| !build_logic::is_wet(s, x, y)).count();
                        kv(p, "Newly flooded", format!("{new} cells ({} under water)", r.flooded_cells), INK);
                        kv(p, "Trees drowned", format!("{}", r.flooded_trees), if r.flooded_trees > 0 { BAD } else { INK });
                        kv(p, "Materials", build_logic::cost_text(&r.cost), INK);
                        kv(p, "River pushing", format!("{:.1} / day", r.pressure), INK);
                        let fill = if r.fill_days.is_finite() { format!("{:.0} days", r.fill_days) } else { "never (no inflow)".into() };
                        kv(p, "Fills in", fill, INK);
                        if r.flooded_buildings > 0 {
                            p.spawn((Node { padding: UiRect::all(Val::Px(5.0)), border: UiRect::all(Val::Px(1.0)), border_radius: BorderRadius::all(Val::Px(4.0)), ..default() }, BackgroundColor(WARN.with_alpha(0.12)), BorderColor::all(WARN.with_alpha(0.4))))
                                .with_children(|b| text(b, format!("! Floods {} building(s)", r.flooded_buildings), 11.0, INK));
                        }
                    }
                    None => kv(p, "Pond", "not a valid dam line", BAD),
                }
                p.spawn(row(5.0)).with_children(|r| {
                    button(r, "Build dam", HudAction::ConfirmLine, ACCENT, false);
                    button(r, "Clear", HudAction::ClearLine, FAINT, false);
                    button(r, "Done", HudAction::CancelTool, FAINT, false);
                });
            }
            hint(p, "Enter builds - RMB or Esc to stop");
        }
    }
}

/// Side view along the main river around the dam line (or the colony): bed, dam crests, water,
/// and the previewed pond in light blue. Auto-scaled to the window so one dam's step shows.
fn profile_panel(p: &mut ChildSpawnerCommands, s: &sim::SimState, hud: &HudState) {
    const H: f32 = 110.0;
    const UP: usize = 70;
    const DOWN: usize = 40;
    const COL_W: f32 = 5.0;
    p.spawn(Node { justify_content: JustifyContent::SpaceBetween, ..row(8.0) }).with_children(|r| {
        bold(r, "River profile", 12.0, INK);
        text(r, "side view along the river, upstream on the left", 10.0, MUTED);
    });
    // Follow the river through the drawn line (or the colony).
    let anchor = if hud.line.is_empty() {
        s.colony.beavers.first().map(|b| b.pos).unwrap_or((0.0, 0.0))
    } else {
        let n = hud.line.len() as f32;
        (hud.line.iter().map(|c| c.0 as f32).sum::<f32>() / n, hud.line.iter().map(|c| c.1 as f32).sum::<f32>() / n)
    };
    let Some((win, mid)) = sim::hud::river_profile_through(s, anchor, UP, DOWN) else {
        text(p, "No river near here", 11.0, MUTED);
        return;
    };
    let near_line = !hud.line.is_empty();
    let a = 0usize;
    let preview = hud.preview.as_ref().and_then(|(_, _, pv)| pv.as_ref());
    let new_surface = preview.map(|pv| sim::hud::profile_with_preview(&win, pv));
    let lo = win.iter().map(|q| q.bed).fold(f32::MAX, f32::min) - 0.3;
    let hi = win.iter().map(|q| q.surface).chain(new_surface.iter().flatten().copied()).fold(f32::MIN, f32::max) + 0.3;
    let span = (hi - lo).max(0.5);
    let px = |v: f32| (v / span * H).max(0.0);
    p.spawn(Node { height: Val::Px(H), flex_direction: FlexDirection::Row, align_items: AlignItems::FlexEnd, ..default() }).with_children(|r| {
        for (i, q) in win.iter().enumerate() {
            let ns = new_surface.as_ref().map(|v| v[i]).unwrap_or(q.surface);
            let here = near_line && a + i == mid;
            r.spawn(Node { width: Val::Px(COL_W), flex_direction: FlexDirection::Column, ..default() }).with_children(|c| {
                if ns > q.surface + 0.005 {
                    c.spawn((Node { height: Val::Px(px(ns - q.surface)), ..default() }, BackgroundColor(Color::srgb(0.68, 0.84, 0.95))));
                }
                c.spawn((Node { height: Val::Px(px(q.surface - q.crest)), ..default() }, BackgroundColor(WATER)));
                c.spawn((Node { height: Val::Px(px(q.crest - q.bed)), ..default() }, BackgroundColor(DAM_WOOD)));
                c.spawn((Node { height: Val::Px(px(q.bed - lo)), ..default() }, BackgroundColor(if here { Color::WHITE } else { BED })));
            });
        }
    });
    p.spawn(row(10.0)).with_children(|r| {
        swatch(r, WATER, 10.0, 8.0);
        text(r, "water", 10.0, MUTED);
        swatch(r, Color::srgb(0.68, 0.84, 0.95), 10.0, 8.0);
        text(r, "added by this dam", 10.0, MUTED);
        swatch(r, DAM_WOOD, 10.0, 8.0);
        text(r, "dams", 10.0, MUTED);
        swatch(r, Color::WHITE, 10.0, 8.0);
        text(r, if near_line { "your line" } else { "nearest river" }, 10.0, MUTED);
        text(r, format!("height {:.1}-{:.1}", lo, hi), 10.0, MUTED);
    });
}

fn role_name(r: Role) -> &'static str {
    match r {
        Role::Collect(k) => match k {
            sim::resources::ResourceKind::Tree | sim::resources::ResourceKind::Felled => "Logger",
            sim::resources::ResourceKind::Plant => "Forager",
            sim::resources::ResourceKind::MudBank => "Mud digger",
            sim::resources::ResourceKind::StoneOutcrop => "Quarrier",
        },
        Role::Build => "Builder",
        Role::Haul => "Hauler",
        Role::Craft => "Crafter",
        Role::Study => "Scholar",
    }
}

fn inspector(p: &mut ChildSpawnerCommands, s: &sim::SimState, id: BuildingId) {
    let Some(b) = s.buildings.building(id) else {
        text(p, "Gone", 12.0, MUTED);
        return;
    };
    let place = s.work.place(id);
    p.spawn(Node { justify_content: JustifyContent::SpaceBetween, ..row(8.0) }).with_children(|r| {
        bold(r, b.kind.spec().name, 15.0, INK);
        button(r, "x", HudAction::CloseInspector, FAINT, false);
    });
    // First line: is it working, and if not, why.
    let empty = place.map_or(0, |pl| pl.slots.iter().filter(|sl| sl.worker.is_none()).count());
    let slots = place.map_or(0, |pl| pl.slots.len());
    let (status, colour) = match (b.status, place.and_then(|pl| pl.site.as_ref())) {
        (Status::Complete, _) if b.kind == BuildingKind::WaterWheel => {
            if b.output > 0.0 { (format!("Turning at {:.0}%", b.output * 100.0), GOOD) } else { ("Still: no current here".into(), WARN) }
        }
        (Status::Complete, _) if slots > 0 && empty == slots => ("Idle: nobody works here".into(), WARN),
        (Status::Complete, _) if empty > 0 => (format!("Working, {empty} of {slots} places empty"), WARN),
        (Status::Complete, _) => ("Working".into(), GOOD),
        (_, Some(site)) => {
            let waiting = site.needed.iter().filter(|a| site.missing(a.0) > 0.0).map(|a| good_name(a.0)).collect::<Vec<_>>();
            if slots > 0 && empty == slots {
                ("Waiting: no builders assigned".into(), WARN)
            } else if !waiting.is_empty() {
                (format!("Building - waiting for {}", waiting.join(", ").to_lowercase()), WARN)
            } else {
                ("Building".into(), GOOD)
            }
        }
        _ => ("Planned".into(), MUTED),
    };
    p.spawn((Node { padding: UiRect::all(Val::Px(6.0)), border: UiRect::all(Val::Px(1.0)), border_radius: BorderRadius::all(Val::Px(5.0)), ..default() }, BackgroundColor(colour.with_alpha(0.12)), BorderColor::all(colour.with_alpha(0.4))))
        .with_children(|r| bold(r, status, 12.0, INK));
    if let Some(site) = place.and_then(|pl| pl.site.as_ref()) {
        bold(p, "CONSTRUCTION", 10.0, MUTED);
        for a in &site.needed {
            let got = a.1 - site.missing(a.0);
            kv(p, good_name(a.0), format!("{:.0} / {:.0}", got, a.1), if site.missing(a.0) > 0.0 { WARN } else { GOOD });
        }
        kv(p, "Labour", format!("{:.1} / {:.1}", site.labour_done, site.labour_needed), INK);
        kv(p, "Done", format!("{:.0}%", site.fraction() * 100.0), INK);
    }
    if let Some(pl) = place.filter(|pl| !pl.slots.is_empty()) {
        bold(p, "WORKERS", 10.0, MUTED);
        for (i, sl) in pl.slots.iter().enumerate() {
            p.spawn(Node { justify_content: JustifyContent::SpaceBetween, ..row(6.0) }).with_children(|r| {
                text(r, role_name(sl.role), 11.5, MUTED);
                match sl.worker.and_then(|w| s.colony.beaver(w)) {
                    Some(bv) => {
                        bold(r, bv.name.clone(), 11.5, INK);
                        button(r, "-", HudAction::EmptySlot(id, i), FAINT, false);
                    }
                    None => {
                        text(r, "empty", 11.5, WARN);
                        button(r, "+ idle beaver", HudAction::FillSlot(id, i), FAINT, false);
                    }
                }
            });
        }
    }
    if let Some(rep) = sim::hud::building_report(s, id) {
        bold(p, "OUTPUT", 10.0, MUTED);
        if rep.measured.is_empty() {
            kv(p, "Measured", "nothing made yet", WARN);
        } else {
            let made: Vec<String> = rep.measured.iter().map(|(g, r)| format!("{r:.1} {}", good_name(*g).to_lowercase())).collect();
            let note = if rep.steady { "" } else { " (still settling)" };
            kv(p, "Measured", format!("{} /day{note}", made.join(" + ")), GOOD);
        }
        for f in &rep.factors {
            let c = match f.ok {
                Some(true) => GOOD,
                Some(false) => WARN,
                None => INK,
            };
            kv(p, f.label.clone(), f.value.clone(), c);
        }
        if let Some((g, max)) = rep.ceiling {
            kv(p, "At most", format!("{max:.1} {}/day, every slot filled, no walking", good_name(g).to_lowercase()), MUTED);
        }
    }
    hint(p, "Esc closes");
}

// ───────────────────────────── rail, search, flow ledger, minimap ─────────────────────────────

fn rail(p: &mut ChildSpawnerCommands, hud: &HudState, lens: MapLens) {
    bold(p, "VIEW", 9.5, MUTED);
    for (l, name) in [(MapLens::World, "World  F1"), (MapLens::Water, "Water  F2"), (MapLens::Goods, "Goods  F3")] {
        button(p, name, HudAction::SetLens(l), if lens == l { Color::srgba(1.0, 1.0, 1.0, 0.14) } else { FAINT }, lens == l);
    }
    bold(p, "TOOLS", 9.5, MUTED);
    button(p, "Ledger  L", HudAction::ToggleLedger, FAINT, hud.ledger_open);
    button(p, "Search  /", HudAction::OpenSearch, FAINT, hud.search.is_some());
    button(p, "Undo  ^Z", HudAction::Undo, FAINT, false);
}

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
fn search_matches(s: &sim::SimState, q: &str) -> Vec<build_logic::MenuEntry> {
    let q = q.trim().to_lowercase();
    if q.is_empty() {
        return Vec::new();
    }
    let menu = build_logic::menu(s);
    let mut out: Vec<build_logic::MenuEntry> = menu.iter().filter(|e| e.kind.spec().name.to_lowercase().contains(&q)).cloned().collect();
    for (word, kinds) in SYNONYMS {
        if word.starts_with(&q) || q.starts_with(word) {
            for k in kinds.iter() {
                if !out.iter().any(|e| e.kind == *k) {
                    out.extend(menu.iter().filter(|e| e.kind == *k).cloned());
                }
            }
        }
    }
    out.sort_by_key(|e| !e.unlocked);
    out.truncate(8);
    out
}

fn search_input(
    mut keys: MessageReader<KeyboardInput>,
    sim: Res<Sim>,
    mut hud: ResMut<HudState>,
    mut typing: ResMut<TextCapture>,
) {
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

fn cell_text(p: &mut ChildSpawnerCommands, w: f32, s: impl Into<String>, c: Color, right: bool) {
    p.spawn(Node { width: Val::Px(w), justify_content: if right { JustifyContent::FlexEnd } else { JustifyContent::FlexStart }, ..default() })
        .with_children(|b| text(b, s, 11.5, c));
}

/// The whole colony's goods per day, how long each lasts, 60 days of stock, who makes what,
/// and the water held in ponds.
fn flow_ledger(p: &mut ChildSpawnerCommands, s: &sim::SimState) {
    p.spawn(Node { justify_content: JustifyContent::SpaceBetween, ..row(8.0) }).with_children(|r| {
        r.spawn(row(8.0)).with_children(|r| {
            bold(r, "Flow ledger", 15.0, INK);
            text(r, "per day, 7-day average - L or Esc closes", 10.5, MUTED);
        });
        button(r, "x", HudAction::ToggleLedger, FAINT, false);
    });
    let widths = [96.0, 58.0, 58.0, 58.0, 58.0, 58.0];
    p.spawn(row(6.0)).with_children(|r| {
        for (i, h) in ["GOOD", "STORED", "IN", "OUT", "NET", "LASTS"].iter().enumerate() {
            r.spawn(Node { width: Val::Px(widths[i]), justify_content: if i == 0 { JustifyContent::FlexStart } else { JustifyContent::FlexEnd }, ..default() })
                .with_children(|b| bold(b, *h, 9.5, MUTED));
        }
        bold(r, "STOCK, LAST 60 DAYS", 9.5, MUTED);
    });
    let st = &s.buildings.stores;
    for g in Good::ALL {
        let (have, inn, out) = (st.amount(g), s.ledger.in_per_day(g), s.ledger.out_per_day(g));
        if have < 0.05 && inn < 0.01 && out < 0.01 {
            continue;
        }
        let net = inn - out;
        let lasts = if g == Good::Food { food_days(s) } else { s.ledger.days_left(g, have) };
        p.spawn(row(6.0)).with_children(|r| {
            r.spawn(Node { width: Val::Px(widths[0]), ..row(5.0) }).with_children(|c| {
                swatch(c, good_color(g), 8.0, 12.0);
                text(c, good_name(g), 11.5, INK);
            });
            cell_text(r, widths[1], format!("{:.0}", have), INK, true);
            cell_text(r, widths[2], format!("{inn:.1}"), if inn > 0.0 { GOOD } else { MUTED }, true);
            cell_text(r, widths[3], format!("{out:.1}"), if out > 0.0 { BAD } else { MUTED }, true);
            let (t, c) = fmt_rate(net);
            cell_text(r, widths[4], t, c, true);
            cell_text(r, widths[5], lasts.map_or("-".to_string(), |d| format!("{:.0} d", d)), if lasts.is_some_and(|d| d < 20.0) { WARN } else { MUTED }, true);
            // Sparkline of end-of-day stock.
            let days: Vec<f32> = s.ledger.days.iter().map(|d| d.stock[g.index()]).collect();
            let top = days.iter().copied().fold(1.0f32, f32::max);
            r.spawn(Node { height: Val::Px(18.0), align_items: AlignItems::FlexEnd, column_gap: Val::Px(1.0), ..row(1.0) }).with_children(|sp| {
                for v in &days {
                    sp.spawn((Node { width: Val::Px(2.0), height: Val::Px((v / top * 18.0).max(1.0)), ..default() }, BackgroundColor(good_color(g))));
                }
            });
        });
    }
    if !s.ledger.has_data() {
        text(p, "Rates appear after the first whole day.", 10.5, MUTED);
    }
    bold(p, "MADE BY", 9.5, MUTED);
    let mut makers: Vec<(BuildingId, sim::output::Flow)> = s.output.by.iter().map(|(b, f)| (*b, *f)).filter(|(_, f)| f.sum_per_day() > 0.01).collect();
    makers.sort_by(|a, b| b.1.sum_per_day().total_cmp(&a.1.sum_per_day()));
    if makers.is_empty() {
        text(p, "No workplace has produced yet. Beavers gathering by order (not from a building) count only in the totals above.", 10.5, MUTED);
    }
    for (id, f) in makers.into_iter().take(8) {
        let Some(b) = s.buildings.building(id) else { continue };
        p.spawn(row(8.0)).with_children(|r| {
            cell_text(r, 150.0, format!("{} #{}", b.kind.spec().name, id.0), INK, false);
            let made: Vec<String> = f.rates().iter().map(|(g, r)| format!("{r:.1} {}", good_name(*g).to_lowercase())).collect();
            cell_text(r, 200.0, format!("{} /day", made.join(" + ")), GOOD, true);
            button(r, "Show >", HudAction::Focus(b.pos), FAINT, false);
        });
    }
    bold(p, "WATER", 9.5, MUTED);
    let ponds = sim::hud::ponds(s);
    let dams = s.water.dam_height.as_slice().iter().filter(|h| **h > 0.0).count();
    let cover = ponds.summer_cover_days().map_or("no dam yet".to_string(), |d| format!("{:.0} of {:.0} summer days", d.min(999.0), calendar::SEASON_DAYS));
    kv(p, "Dam cells", format!("{dams}"), INK);
    if ponds.dammed {
        kv(p, "Ponds behind dams", format!("{:.0} water on {} cells", ponds.volume, ponds.cells), WATER);
    } else {
        kv(p, "Ponds behind dams", "no dam yet", MUTED);
    }
    kv(p, "Summer cover", cover, INK);
}

fn setup_minimap(mut commands: Commands, sim: Res<Sim>, mut images: ResMut<Assets<Image>>) {
    let n = sim.0.world.size as u32;
    let img = Image::new_fill(
        Extent3d { width: n, height: n, depth_or_array_layers: 1 },
        TextureDimension::D2,
        &[30, 30, 30, 255],
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::default(),
    );
    commands.insert_resource(MinimapImage(images.add(img)));
}

/// Repaints the minimap every couple of seconds: land shaded by height, water by depth,
/// buildings white, beavers orange.
fn update_minimap(time: Res<Time>, mut next: Local<f32>, sim: Res<Sim>, img: Option<Res<MinimapImage>>, mut images: ResMut<Assets<Image>>) {
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
            buf[i * 4..i * 4 + 4].copy_from_slice(&[(c[0] * 255.0) as u8, (c[1] * 255.0) as u8, (c[2] * 255.0) as u8, 255]);
        }
    }
    for b in &s.buildings.buildings {
        for (x, y) in b.cells() {
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

fn minimap_panel(p: &mut ChildSpawnerCommands, s: &sim::SimState, img: &Handle<Image>, target: Vec3) {
    const SIZE: f32 = 160.0;
    let (cx, cy) = crate::flow::world_cell(&s.height, target);
    let n = s.world.size as f32;
    p.spawn((
        ImageNode::new(img.clone()),
        Node { width: Val::Px(SIZE), height: Val::Px(SIZE), ..default() },
        MinimapPicture,
        RelativeCursorPosition::default(),
        Interaction::default(),
        FocusPolicy::Block,
        Blocks,
    ))
    .with_children(|m| {
        m.spawn((
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px((cx / n * SIZE - 7.0).clamp(0.0, SIZE - 14.0)),
                top: Val::Px((cy / n * SIZE - 7.0).clamp(0.0, SIZE - 14.0)),
                width: Val::Px(14.0),
                height: Val::Px(14.0),
                border: UiRect::all(Val::Px(2.0)),
                ..default()
            },
            BorderColor::all(Color::WHITE),
        ));
    });
    text(p, "North up - click or drag to look", 9.5, MUTED);
}

/// Click or drag on the minimap moves the camera there.
fn minimap_click(q: Query<(&Interaction, &RelativeCursorPosition), With<MinimapPicture>>, sim: Res<Sim>, mut orbit: ResMut<crate::camera::OrbitCamera>) {
    for (i, rel) in &q {
        if *i != Interaction::Pressed {
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

/// `BC_HUD_DEMO=rest|dam|inspect|camp|ledger|goods|water|search` (with `BC_HUD=1` and
/// `--screenshot`): once the colony stands, runs `BC_HUD_DAYS` days (default 12) and sets up that
/// view, so each state can be captured without a mouse. `camp`, `ledger` and `goods` first stand
/// up a staffed logging camp, forager hut and warehouse beside the camp (finished by script) so
/// there is real production to show.
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
    let mut staffed = Vec::new();
    if matches!(which.as_str(), "camp" | "ledger" | "goods") {
        if let Some(c) = camp {
            staffed = stand_up_camp(s, &mut pending.0, c);
        }
    }
    for _ in 0..(days / crate::sim_driver::FIXED_DT_DAYS) as usize {
        let _ = s.step(crate::sim_driver::FIXED_DT_DAYS);
    }
    let look = |o: &mut crate::camera::OrbitCamera, h: &sim::grid::HeightGrid, c: (f32, f32), dist: f32| {
        o.target = cell_world(h, c);
        o.distance = dist;
        o.pitch = 0.95;
    };
    match which.as_str() {
        "dam" => {
            if let Some(d) = planned.0.as_ref() {
                hud.open_cat = categories(s).iter().position(|g| *g == Group::Base);
                hud.set_tool(Tool::Dam);
                hud.line = d.cells.clone();
                let crest = std::env::var("BC_HUD_CREST").ok().and_then(|v| v.parse().ok()).unwrap_or(d.height);
                hud.dam_height = crest.clamp(MIN_DAM_HEIGHT, MAX_DAM_HEIGHT);
                let m = d.cells[d.cells.len() / 2];
                look(&mut orbit, &s.height, (m.0 as f32, m.1 as f32), 40.0);
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
                let b = s.buildings.building(id).map(|b| b.pos).unwrap_or((0, 0));
                look(&mut orbit, &s.height, (b.0 as f32, b.1 as f32), 32.0);
            }
        }
        other => {
            match other {
                "ledger" => hud.ledger_open = true,
                "goods" => *lens = MapLens::Goods,
                "water" => *lens = MapLens::Water,
                "search" => hud.search = Some("food".into()),
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

// ───────────────────────────── world overlay ─────────────────────────────

/// Stores that take a good (mirrors the sim's store rule).
fn store_takes(kind: BuildingKind, g: Good) -> bool {
    match kind {
        BuildingKind::Warehouse | BuildingKind::Storehouse => true,
        BuildingKind::FoodCache => g == Good::Food,
        _ => false,
    }
}

fn cell_outline(g: &mut Gizmos, h: &sim::grid::HeightGrid, c: Cell, y: Option<f32>, colour: Color) {
    let (x, z) = (c.0 as f32, c.1 as f32);
    let corners = [(x - 0.5, z - 0.5), (x + 0.5, z - 0.5), (x + 0.5, z + 0.5), (x - 0.5, z + 0.5)];
    let pts: Vec<Vec3> = corners
        .iter()
        .map(|&(a, b)| {
            let mut v = cell_world(h, (a, b));
            v.y = y.unwrap_or(height_at(h, a, b)) + 0.08;
            v
        })
        .collect();
    for i in 0..4 {
        g.line(pts[i], pts[(i + 1) % 4], colour);
    }
}

fn draw_world_overlay(mut g: Gizmos, sim: Res<Sim>, hud: Res<HudState>, hover: Res<HoverCell>, sel: Res<Selection>, orbit: Res<crate::camera::OrbitCamera>) {
    let s = &sim.0;
    let h = &s.height;
    let lens = crate::terrain_render::map_lens();
    let (tx, ty) = crate::flow::world_cell(h, orbit.target);
    if lens == 1 {
        // Flow arrows on water near the view centre: direction of net flow, length by strength.
        let r = 40i32;
        for y in ((ty as i32 - r).max(1)..(ty as i32 + r).min(s.world.size as i32 - 1)).step_by(3) {
            for x in ((tx as i32 - r).max(1)..(tx as i32 + r).min(s.world.size as i32 - 1)).step_by(3) {
                let (cx, cy) = (x as usize, y as usize);
                let d = s.water.depth.get(cx, cy).copied().unwrap_or(0.0);
                if d < 0.05 {
                    continue;
                }
                let (fx, fy) = s.water.flow_at(cx, cy);
                let m = (fx * fx + fy * fy).sqrt();
                if m < 1e-5 {
                    continue;
                }
                let len = (m.sqrt() * 12.0).clamp(0.35, 1.4);
                let dir = Vec2::new(fx, fy) / m;
                let mut a = cell_world(h, (cx as f32, cy as f32));
                a.y = h.get(cx, cy).copied().unwrap_or(0.0) + d + 0.12;
                let b = a + Vec3::new(dir.x, 0.0, dir.y) * len;
                let side = Vec3::new(-dir.y, 0.0, dir.x) * 0.25;
                let back = Vec3::new(dir.x, 0.0, dir.y) * 0.3;
                g.line(a, b, Color::WHITE);
                g.line(b, b - back + side, Color::WHITE);
                g.line(b, b - back - side, Color::WHITE);
            }
        }
    }
    if lens == 2 {
        // Goods lens: each workplace outlined by how it is doing, with a line to its store.
        for b in s.buildings.buildings.iter().filter(|b| b.is_complete()) {
            let Some(rep) = sim::hud::building_report(s, b.id) else { continue };
            let col = if rep.factors.first().is_some_and(|f| f.value.starts_with("0 of")) {
                BAD
            } else if rep.factors.iter().any(|f| f.ok == Some(false)) {
                WARN
            } else {
                GOOD
            };
            for c in b.cells() {
                cell_outline(&mut g, h, c, None, col);
            }
            if let Some((good, _)) = rep.ceiling {
                let (w, d) = b.kind.spec().footprint;
                let from = (b.pos.0 as f32 + w as f32 / 2.0 - 0.5, b.pos.1 as f32 + d as f32 / 2.0 - 0.5);
                let store = s.buildings.buildings.iter().filter(|o| o.is_complete() && o.id != b.id && store_takes(o.kind, good)).min_by_key(|o| o.pos.0.abs_diff(b.pos.0) + o.pos.1.abs_diff(b.pos.1));
                if let Some(o) = store {
                    let to = (o.pos.0 as f32, o.pos.1 as f32);
                    let lift = Vec3::Y * 1.2;
                    g.line(cell_world(h, from) + lift, cell_world(h, to) + lift, good_color(good));
                }
            }
        }
    }
    if let Selection::Site(id) = *sel {
        if let Some(b) = s.buildings.building(id) {
            for c in b.cells() {
                cell_outline(&mut g, h, c, None, Color::WHITE);
            }
        }
    }
    match hud.tool {
        Tool::None => {}
        Tool::Building(kind) => {
            if let Some(c) = hover.0 {
                let pv = build_logic::site_preview(s, kind, c);
                let col = if pv.is_valid() { Color::WHITE } else { BAD };
                for c in &pv.cells {
                    cell_outline(&mut g, h, *c, None, col);
                }
            }
        }
        Tool::Dam | Tool::Canal => {
            if let Some((_, _, Some(pv))) = hud.preview.as_ref().filter(|_| hud.tool == Tool::Dam) {
                for c in &pv.flooded {
                    cell_outline(&mut g, h, *c, Some(pv.level), WATER);
                }
            }
            let ok = match hud.tool {
                Tool::Dam => build_logic::check_dam(s, &hud.line, hud.dam_height).is_ok(),
                _ => build_logic::check_canal(s, &hud.line).is_ok(),
            };
            for c in &hud.line {
                cell_outline(&mut g, h, *c, None, if ok { Color::WHITE } else { BAD });
            }
            if hud.line.is_empty() {
                if let Some(c) = hover.0 {
                    cell_outline(&mut g, h, c, None, Color::WHITE.with_alpha(0.7));
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rates_read_as_signed_per_day() {
        assert_eq!(fmt_rate(1.24).0, "+1.2/d");
        assert_eq!(fmt_rate(-0.8).0, "-0.8/d");
        assert_eq!(fmt_rate(0.01).0, "0");
    }

    #[test]
    fn categories_follow_the_menu_order_and_fit_the_hotkeys() {
        let s = sim::SimState::new(1);
        let cats = categories(&s);
        assert_eq!(cats.first(), Some(&Group::Base));
        assert!(cats.len() <= CAT_KEYS.len(), "{} categories but {} keys", cats.len(), CAT_KEYS.len());
    }

    #[test]
    fn switching_tools_clears_the_drawn_line() {
        let mut h = HudState { line: vec![(1, 1), (2, 1)], drag_from: Some((1, 1)), ..default() };
        h.set_tool(Tool::Canal);
        assert!(h.line.is_empty() && h.drag_from.is_none() && h.tool == Tool::Canal);
    }
}
