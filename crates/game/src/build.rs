//! Placement: the tool panel, placement ghost, dam line + pond preview with flood summary, canal
//! paths and the work-order list (cancel / priority). The build menu and job panel are in
//! `ui/buildmenu.rs` and `ui/jobs.rs`. Rules live in `build_logic`; this file is input, layout and drawing.
use crate::build_logic::*;
use crate::camera::{MainCamera, OrbitCamera};
use crate::flow::{cell_world, height_at, ray_heightfield, CaptureGate, Cell, Flow, Phase, Selection};
use crate::flow_view::{ClickSet, UiLayout, WorldClick};
use crate::state::Sim;
use crate::terrain_render::CELL_SIZE;
use bevy::asset::RenderAssetUsages;
use bevy::mesh::Indices;
use bevy::prelude::*;
use bevy::render::render_resource::PrimitiveTopology;
use bevy::window::PrimaryWindow;
use sim::buildings::{Amount, BuildingKind};
use sim::colony::OrderId;
use sim::dam_preview::DamPreview;
use sim::SimState;

use crate::ui::kit::{self, Corner};
const PANEL_BG: Color = kit::BARK;
const BTN_BG: Color = kit::RAISED;
const BTN_HOT: Color = kit::LINE;
const BTN_ON: Color = kit::POND;
const BTN_OFF: Color = kit::BARK;
const TXT: Color = kit::PITH;
const DIM: Color = kit::DIM;
const WARN: Color = kit::AMBER;
const BAD: Color = kit::BERRY;
const GOOD: Color = kit::MOSS;
/// Space kept free at the bottom for the navigation bar.
const NAV_CLEARANCE: f32 = 96.0;
const POND: Color = kit::POND;

// ───────────────────────────── state ─────────────────────────────

#[derive(Clone, Debug)]
pub struct Preview {
    pub order: Option<BuildOrder>,
    pub result: Result<(), PlaceError>,
    pub cells: Vec<Cell>,
    pub cost: Vec<Amount>,
    pub dam: Option<DamPreview>,
}

#[derive(Resource)]
pub struct BuildUi {
    pub tool: Option<BuildingKind>,
    pub dam_height: f32,
    /// Cells drawn so far for a dam (two ends) or canal (full path).
    pub path: Vec<Cell>,
    pub drawing: bool,
    /// A finished drag waiting for Confirm.
    pub awaiting: bool,
    pub hover: Option<Cell>,
    pub message: String,
    pub preview: Option<Preview>,
    /// Stand-in cursor for scripted screenshots (there is no real cursor).
    pub fake_cursor: Option<Vec2>,
    dam_cache: Option<(Vec<Cell>, u32, Option<DamPreview>)>,
}

impl Default for BuildUi {
    fn default() -> Self {
        BuildUi {
            tool: None,
            dam_height: DEFAULT_DAM_HEIGHT,
            path: vec![],
            drawing: false,
            awaiting: false,
            hover: None,
            message: String::new(),
            preview: None,
            fake_cursor: None,
            dam_cache: None,
        }
    }
}

impl BuildUi {
    /// Left-drag draws instead of orbiting while a dam or canal tool is active.
    pub fn path_tool_active(&self) -> bool {
        self.tool.is_some_and(is_path_tool)
    }
    fn clear_path(&mut self) {
        self.path.clear();
        self.drawing = false;
        self.awaiting = false;
    }
    pub fn select_tool(&mut self, k: Option<BuildingKind>) {
        self.tool = k;
        self.clear_path();
        self.preview = None;
    }
}

#[derive(Resource, Default)]
pub struct PendingRes(pub Pending);

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum BAction {
    Pick(BuildingKind),
    Confirm,
    CancelTool,
    Height(i32),
    CancelOrder(OrderTarget),
    Prioritise(OrderId),
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum BtnState {
    Normal,
    On,
    Locked,
    Disabled,
    Hot,
}

#[derive(Clone, PartialEq)]
enum ItemKind {
    Panel(Color),
    Text { text: String, color: Color, size: f32 },
    Button { action: BAction, label: String, state: BtnState },
}

#[derive(Clone, PartialEq)]
struct Item {
    rect: Rect,
    kind: ItemKind,
}

#[derive(Resource, Default)]
pub struct BuildItems(Vec<Item>);

#[derive(Component)]
struct BuildNode;

fn rc(x: f32, y: f32, w: f32, h: f32) -> Rect {
    Rect::new(x, y, x + w, y + h)
}

// ───────────────────────────── layout ─────────────────────────────

struct Lay {
    items: Vec<Item>,
    cursor: Option<Vec2>,
}

impl Lay {
    fn panel(&mut self, r: Rect, c: Color) {
        self.items.push(Item { rect: r, kind: ItemKind::Panel(c) });
    }
    fn text(&mut self, r: Rect, t: impl Into<String>, color: Color, size: f32) {
        self.items.push(Item { rect: r, kind: ItemKind::Text { text: t.into(), color, size } });
    }
    fn button(&mut self, r: Rect, action: BAction, label: impl Into<String>, on: bool) {
        let state = if on { BtnState::On } else if self.cursor.is_some_and(|c| r.contains(c)) { BtnState::Hot } else { BtnState::Normal };
        self.items.push(Item { rect: r, kind: ItemKind::Button { action, label: label.into(), state } });
    }
    fn button_state(&mut self, r: Rect, action: BAction, label: impl Into<String>, state: BtnState) {
        self.items.push(Item { rect: r, kind: ItemKind::Button { action, label: label.into(), state } });
    }
    fn bar(&mut self, r: Rect, frac: f32, color: Color) {
        self.panel(r, Color::srgba(0.25, 0.28, 0.32, 0.9));
        let f = frac.clamp(0.0, 1.0);
        if f > 0.0 {
            self.panel(Rect::new(r.min.x, r.min.y, r.min.x + r.width() * f, r.max.y), color);
        }
    }
}

fn pond_headline(d: &DamPreview) -> (String, Color) {
    let n = d.flooded_trees + d.flooded_buildings.len();
    let text = format!(
        "POND  {} cells   {} trees flooded   {} buildings flooded",
        d.flooded.len(), d.flooded_trees, d.flooded_buildings.len()
    );
    (text, if n > 0 { WARN } else { POND })
}

#[allow(clippy::too_many_arguments)]
fn build_items(
    win: Vec2,
    sim: &SimState,
    ui: &BuildUi,
    pending: &Pending,
    sel: Selection,
    cursor: Option<Vec2>,
) -> Vec<Item> {
    let mut l = Lay { items: vec![], cursor };
    if !sim.colony.is_founded() {
        return l.items;
    }
    // ── tool panel (bottom centre) ──
    if let Some(kind) = ui.tool {
        let spec = kind.spec();
        let w = 560.0;
        let x = ((win.x - w) / 2.0).max(10.0);
        let mut lines: Vec<(String, Color, f32)> = vec![];
        // A dam spot under the cursor is read from the tooltip; the panel details a dragged line.
        let dragged = kind == BuildingKind::Dam && (ui.path.len() >= 2 || ui.awaiting);
        let hint = match kind {
            BuildingKind::Dam => "hover to preview, click to place".to_string(),
            BuildingKind::Canal => "drag a path from the water".to_string(),
            _ => format!("click to place ({}x{})", spec.footprint.0, spec.footprint.1),
        };
        lines.push((format!("{}  -  {}   (Esc to stop)", spec.name, hint), TXT, 14.0));
        if let Some(p) = &ui.preview {
            if let Some(d) = p.dam.as_ref().filter(|_| kind != BuildingKind::Dam || dragged) {
                let (t, c) = pond_headline(d);
                lines.push((t, c, 14.0));
                lines.push((format!("water level {:.1}   dam {} cells x {:.1} high", d.level, p.cells.len(), ui.dam_height), POND, 12.0));
            }
            if !p.cost.is_empty() && (kind != BuildingKind::Dam || dragged) {
                let ok = p.cost.iter().all(|&(g, a)| sim.buildings.stores.amount(g) + 1e-4 >= a);
                lines.push((format!("Cost: {}", cost_text(&p.cost)), if ok { TXT } else { WARN }, 12.0));
            }
            match &p.result {
                Ok(()) if kind == BuildingKind::Dam && !dragged => lines.push(("Click to place a dam site here, or drag a longer line".into(), GOOD, 12.0)),
                Ok(()) if p.order.is_some() && is_path_tool(kind) && !ui.awaiting => lines.push(("Release to review".into(), DIM, 12.0)),
                Ok(()) if is_path_tool(kind) => lines.push(("Ready. Confirm with Enter or the button.".into(), GOOD, 12.0)),
                Ok(()) => lines.push(("Click to build here".into(), GOOD, 12.0)),
                Err(PlaceError::NoCells) => lines.push(("Nothing drawn yet.".into(), DIM, 12.0)),
                Err(e) => lines.push((format!("Cannot build here: {e}"), BAD, 12.0)),
            }
        } else if is_path_tool(kind) {
            lines.push(("Hover the map, or press and drag.".into(), DIM, 12.0));
        } else {
            lines.push(("Move over the map to see the ghost.".into(), DIM, 12.0));
        }
        if !ui.message.is_empty() {
            lines.push((ui.message.clone(), WARN, 12.0));
        }
        let show_row = is_path_tool(kind) && (kind != BuildingKind::Dam || dragged);
        let h = 14.0 + lines.iter().map(|l| l.2 + 6.0).sum::<f32>() + if show_row { 34.0 } else { 0.0 };
        let y0 = win.y - NAV_CLEARANCE - h;
        l.panel(rc(x, y0, w, h), PANEL_BG);
        let mut y = y0 + 8.0;
        for (t, c, s) in lines {
            l.text(rc(x + 10.0, y, w - 20.0, s + 4.0), t, c, s);
            y += s + 6.0;
        }
        if show_row {
            let by = y0 + h - 32.0;
            let valid = ui.preview.as_ref().is_some_and(|p| p.result.is_ok()) && ui.awaiting;
            l.button_state(rc(x + 10.0, by, 140.0, 26.0), BAction::Confirm, "Confirm  Enter", if valid { BtnState::On } else { BtnState::Disabled });
            l.button(rc(x + 156.0, by, 110.0, 26.0), BAction::CancelTool, "Clear  Esc", false);
            if kind == BuildingKind::Dam {
                l.text(rc(x + 290.0, by + 5.0, 110.0, 18.0), format!("Height {:.1}", ui.dam_height), TXT, 13.0);
                l.button(rc(x + 380.0, by, 28.0, 26.0), BAction::Height(-1), "-", false);
                l.button(rc(x + 412.0, by, 28.0, 26.0), BAction::Height(1), "+", false);
                l.text(rc(x + 448.0, by + 6.0, 100.0, 16.0), "keys [ and ]", DIM, 11.0);
            }
        }
    }

    // ── hover tooltip next to the cursor: a building's verdict, or the sim's dam preview ──
    if let (Some(c), Some(kind), Some(p)) = (cursor, ui.tool.filter(|k| !is_path_tool(*k) || !ui.awaiting), ui.preview.as_ref()) {
        let (lines, headline_ok): (Vec<String>, bool) = match (&p.dam, &p.result) {
            (Some(d), _) => (DamReadout::from_preview(d).lines(), true),
            (None, Ok(())) if !is_path_tool(kind) => (vec![format!("{}: click to place a site", kind.spec().name)], true),
            (None, Err(e)) if !is_path_tool(kind) => (vec![format!("Cannot build here: {e}")], false),
            _ => (vec![], true),
        };
        let dam_hurts = p.dam.as_ref().is_some_and(|d| d.flooded_trees + d.flooded_buildings.len() > 0);
        let d = p.dam.as_ref().filter(|_| !lines.is_empty());
        let (w, lh) = (330.0, 17.0);
        let h = 10.0 + lines.len() as f32 * lh;
        let x = (c.x + 22.0).min(win.x - w - 8.0);
        let y = (c.y + 22.0).min(win.y - h - 8.0);
        if !lines.is_empty() {
            l.panel(rc(x, y, w, h), PANEL_BG);
        }
        for (i, t) in lines.into_iter().enumerate() {
            let col = if d.is_none() { if headline_ok { GOOD } else { BAD } } else if i == 1 && dam_hurts { WARN } else if i == 0 { POND } else { TXT };
            l.text(rc(x + 8.0, y + 5.0 + i as f32 * lh, w - 16.0, lh), t, col, 12.0);
        }
    }

    // ── right column, stacked from the bottom: the work orders ──
    let right_w = 330.0;
    let rx = win.x - 10.0 - right_w;
    let bottom = win.y - 10.0;
    let rows = order_rows(sim, pending);
    if !rows.is_empty() {
        let shown = rows.len().min(7);
        let h = 30.0 + shown as f32 * 26.0 + if rows.len() > shown { 18.0 } else { 0.0 };
        let y0 = bottom - h;
        l.panel(rc(rx, y0, right_w, h), PANEL_BG);
        l.text(rc(rx + 10.0, y0 + 7.0, right_w - 20.0, 18.0), format!("WORK ORDERS  {}   (builders take #1 first)", rows.len()), TXT, 12.0);
        let mut y = y0 + 28.0;
        for (i, r) in rows.iter().take(shown).enumerate() {
            l.text(rc(rx + 10.0, y + 5.0, 154.0, 18.0), format!("{}. {}", i + 1, r.label), TXT, 11.0);
            l.bar(rc(rx + 166.0, y + 9.0, 70.0, 9.0), r.progress, Color::srgb(0.9, 0.7, 0.3));
            l.text(rc(rx + 240.0, y + 5.0, 34.0, 18.0), format!("{:.0}%", r.progress * 100.0), DIM, 11.0);
            if i > 0 {
                l.button(rc(rx + 276.0, y + 2.0, 22.0, 22.0), BAction::Prioritise(r.order), "^", false);
            }
            l.button(rc(rx + 300.0, y + 2.0, 22.0, 22.0), BAction::CancelOrder(r.target), "x", false);
            y += 26.0;
        }
        if rows.len() > shown {
            l.text(rc(rx + 10.0, y + 2.0, right_w - 20.0, 16.0), format!("+{} more", rows.len() - shown), DIM, 11.0);
        }
    }
    l.items
}

/// Catalogue cost for one unit (dam: per 3 cells, canal: per cell).
fn path_cost_unit(kind: BuildingKind) -> Vec<Amount> {
    kind.spec().cost.to_vec()
}

// ───────────────────────────── preview ─────────────────────────────

fn compute_preview(sim: &SimState, ui: &mut BuildUi) {
    let Some(kind) = ui.tool else {
        ui.preview = None;
        return;
    };
    let height = ui.dam_height;
    let p = match kind {
        BuildingKind::Dam if ui.path.len() < 2 => ui.hover.map(|c| {
            let sp = site_preview(sim, kind, c);
            Preview { result: sp.verdict, cost: sp.cost, order: Some(BuildOrder::Building { kind, pos: sp.pos }), cells: sp.cells, dam: sp.dam }
        }),
        BuildingKind::Dam => {
            let cells = line_cells(ui.path[0], *ui.path.last().unwrap());
            let key = (cells.clone(), height.to_bits());
            let dam = match &ui.dam_cache {
                Some((c, h, d)) if *c == key.0 && *h == key.1 => d.clone(),
                _ => {
                    let d = dam_preview(sim, &cells, height);
                    ui.dam_cache = Some((key.0, key.1, d.clone()));
                    d
                }
            };
            Some(Preview {
                result: check_dam(sim, &cells, height),
                cost: path_cost(kind, cells.len()),
                order: Some(BuildOrder::Dam { cells: cells.clone(), height }),
                cells,
                dam,
            })
        }
        // No path drawn yet: the canal under the cursor, judged by the catalogue site rule.
        BuildingKind::Canal if ui.path.is_empty() => ui.hover.map(|c| {
            let sp = site_preview(sim, kind, c);
            Preview { result: sp.verdict, cost: sp.cost, order: Some(BuildOrder::Building { kind, pos: sp.pos }), cells: sp.cells, dam: None }
        }),
        BuildingKind::Canal => {
            {
                let cells = ui.path.clone();
                Some(Preview { result: check_canal(sim, &cells), cost: path_cost(kind, cells.len()), order: Some(BuildOrder::Canal { cells: cells.clone() }), cells, dam: None })
            }
        }
        _ => ui.hover.map(|c| {
            let sp = site_preview(sim, kind, c);
            Preview { result: sp.verdict, cost: sp.cost, order: Some(BuildOrder::Building { kind, pos: sp.pos }), cells: sp.cells, dam: None }
        }),
    };
    ui.preview = p;
}

// ───────────────────────────── plugin ─────────────────────────────

/// `--demo-dam`: scripted dam preview for screenshots.
#[derive(Resource, Default)]
pub struct DemoDam {
    stage: u8,
    since: f32,
}

/// `--demo-place`: scripted placement previews for screenshots.
#[derive(Resource, Default)]
pub struct DemoPlace {
    since: f32,
    done: bool,
}

pub struct BuildPlugin;

impl Plugin for BuildPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<BuildUi>()
            .init_resource::<PendingRes>()
            .init_resource::<BuildItems>()
            .add_systems(Startup, spawn_overlays)
            .add_systems(Update, (poll_orders, render_items, update_overlays, draw_gizmos).after(ClickSet));
        if app.world().resource::<crate::cli::Cli>().demo_place.is_some() {
            app.insert_resource(crate::flow::CaptureGate(false)).init_resource::<DemoPlace>().add_systems(Update, demo_place.after(ClickSet));
        }
        if app.world().resource::<crate::cli::Cli>().demo_dam {
            app.init_resource::<DemoDam>().add_systems(Update, demo_dam.after(ClickSet));
        }
    }
}

pub fn layout_build(
    windows: Query<&Window, With<PrimaryWindow>>,
    sim: Res<Sim>,
    mut ui: ResMut<BuildUi>,
    pending: Res<PendingRes>,
    sel: Res<Selection>,
    mut layout: ResMut<UiLayout>,
    mut items: ResMut<BuildItems>,
) {
    let Ok(w) = windows.single() else { return };
    let win = Vec2::new(w.width(), w.height());
    compute_preview(&sim.0, &mut ui);
    let new = build_items(win, &sim.0, &ui, &pending.0, *sel, w.cursor_position().or(ui.fake_cursor));
    for it in &new {
        if !matches!(it.kind, ItemKind::Text { .. }) && !matches!(&it.kind, ItemKind::Panel(c) if *c != PANEL_BG) {
            layout.extra.push(it.rect);
        }
    }
    if items.0 != new {
        items.0 = new;
    }
}

#[allow(clippy::too_many_arguments)]
pub fn build_input(
    buttons: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    cams: Query<(&Camera, &GlobalTransform), With<MainCamera>>,
    items: Res<BuildItems>,
    layout: Res<UiLayout>,
    flow: Res<Flow>,
    mut wc: ResMut<WorldClick>,
    mut press: Local<Option<Vec2>>,
    mut sim: ResMut<Sim>,
    mut ui: ResMut<BuildUi>,
    mut pending: ResMut<PendingRes>,
    mut sel: ResMut<Selection>,
) {
    if !sim.0.colony.is_founded() || matches!(flow.phase, Phase::GameOver) {
        return;
    }
    // keys
    if keys.just_pressed(KeyCode::Escape) {
        if ui.tool.is_some() && !ui.path.is_empty() {
            ui.clear_path();
        } else {
            ui.select_tool(None);
        }
    }
    if keys.just_pressed(KeyCode::BracketLeft) {
        adjust_height(&mut ui, -1);
    }
    if keys.just_pressed(KeyCode::BracketRight) {
        adjust_height(&mut ui, 1);
    }
    if keys.just_pressed(KeyCode::Enter) || keys.just_pressed(KeyCode::NumpadEnter) {
        confirm(&mut sim.0, &mut pending.0, &mut ui);
    }
    let Ok(window) = windows.single() else { return };
    let Some(cursor) = window.cursor_position() else { return };
    let over_ui = layout.over_ui(cursor);
    // UI button clicks (press and release without a drag)
    if buttons.just_pressed(MouseButton::Left) {
        *press = Some(cursor);
    }
    if buttons.just_released(MouseButton::Left) {
        if let Some(start) = press.take() {
            if start.distance(cursor) <= 5.0 {
                let hit = items.0.iter().rev().find_map(|it| match &it.kind {
                    ItemKind::Button { action, state, .. } if it.rect.contains(cursor) && *state != BtnState::Disabled => Some(*action),
                    _ => None,
                });
                if let Some(a) = hit {
                    do_action(a, &mut sim.0, &mut pending.0, &mut ui, &mut sel);
                }
            }
        }
    }
    // world: hover cell
    let hover = if over_ui {
        None
    } else {
        cams.single().ok().and_then(|(camera, gt)| {
            let ray = camera.viewport_to_world(gt, cursor).ok()?;
            ray_heightfield(ray.origin, *ray.direction, &sim.0.height).map(|t| t.0)
        })
    };
    ui.hover = hover;
    let Some(tool) = ui.tool else { return };
    // The tool owns world clicks so they do not select things underneath.
    let clicked = wc.0.take().is_some();
    if is_path_tool(tool) {
        if buttons.just_pressed(MouseButton::Left) && !over_ui {
            if let Some(c) = hover {
                ui.path = vec![c];
                ui.drawing = true;
                ui.awaiting = false;
            }
        }
        if ui.drawing && buttons.pressed(MouseButton::Left) {
            if let Some(c) = hover {
                match tool {
                    BuildingKind::Dam => {
                        let first = ui.path[0];
                        ui.path = vec![first, c];
                    }
                    _ => {
                        let last = *ui.path.last().unwrap();
                        if last != c {
                            let seg = line_cells(last, c);
                            for s in seg.into_iter().skip(1) {
                                if ui.path.last() != Some(&s) && !ui.path.contains(&s) {
                                    ui.path.push(s);
                                }
                            }
                        }
                    }
                }
            }
        }
        if ui.drawing && buttons.just_released(MouseButton::Left) {
            end_stroke(&mut sim.0, &mut ui);
        }
    } else if clicked && !over_ui {
        place_hovered(&mut sim.0, &mut ui);
    }
}

/// Mouse release after a path tool's press. A drag leaves the path waiting for confirmation;
/// a plain click (one cell) places the standard site under the cursor, like any other building.
fn end_stroke(sim: &mut SimState, ui: &mut BuildUi) {
    ui.drawing = false;
    ui.awaiting = ui.path.len() > 1;
    ui.message.clear();
    if !ui.awaiting {
        ui.path.clear();
        place_hovered(sim, ui);
    }
}

/// A click on the hovered spot: the one place-site command the preview allows, or nothing.
fn place_hovered(sim: &mut SimState, ui: &mut BuildUi) {
    let (Some(kind), Some(hover)) = (ui.tool, ui.hover) else { return };
    let sp = site_preview(sim, kind, hover);
    match click_command(&sp) {
        Some(cmd) => {
            ui.message = match sim.apply(cmd) {
                Ok(()) => format!("{} site placed at ({}, {})", kind.spec().name, sp.pos.0, sp.pos.1),
                Err(e) => e.to_string(),
            }
        }
        None => ui.message = sp.verdict.err().map(|e| format!("{e}")).unwrap_or_default(),
    }
}

fn adjust_height(ui: &mut BuildUi, dir: i32) {
    ui.dam_height = (ui.dam_height + 0.25 * dir as f32).clamp(MIN_DAM_HEIGHT, MAX_DAM_HEIGHT);
}

fn confirm(sim: &mut SimState, pending: &mut Pending, ui: &mut BuildUi) {
    if !ui.awaiting {
        return;
    }
    let Some(Preview { order: Some(o), .. }) = ui.preview.clone() else { return };
    match emit(sim, pending, &o) {
        Ok(m) => {
            ui.message = m;
            ui.clear_path();
        }
        Err(e) => ui.message = format!("{e}"),
    }
}

fn do_action(a: BAction, sim: &mut SimState, pending: &mut Pending, ui: &mut BuildUi, sel: &mut Selection) {
    match a {
        BAction::Pick(k) => match check_locked(sim, k) {
            Some(e) => ui.message = format!("{e}"),
            None => {
                ui.message.clear();
                let same = ui.tool == Some(k);
                ui.select_tool(if same { None } else { Some(k) });
            }
        },
        BAction::Confirm => confirm(sim, pending, ui),
        BAction::CancelTool => {
            if ui.path.is_empty() {
                ui.select_tool(None);
            } else {
                ui.clear_path();
            }
        }
        BAction::Height(d) => adjust_height(ui, d),
        BAction::CancelOrder(t) => {
            ui.message = match cancel(sim, pending, t) {
                Ok(()) => "Order cancelled, cost refunded".into(),
                Err(e) => e,
            }
        }
        BAction::Prioritise(o) => {
            ui.message = if prioritise(&mut sim.colony, o) { "Moved to the front of the queue".into() } else { "Order already finished".into() }
        }
    }
    let _ = sel;
}

/// Picks (or un-picks) a building tool from the Buildings screen; says why when it is locked.
pub fn pick_tool(sim: &SimState, ui: &mut BuildUi, k: BuildingKind) {
    match check_locked(sim, k) {
        Some(e) => ui.message = format!("{e}"),
        None => {
            ui.message.clear();
            let same = ui.tool == Some(k);
            ui.select_tool(if same { None } else { Some(k) });
        }
    }
}

fn check_locked(sim: &SimState, k: BuildingKind) -> Option<PlaceError> {
    menu(sim).into_iter().find(|e| e.kind == k).and_then(|e| e.need).map(|(t, n, h)| PlaceError::Locked(t, n, h)).filter(|_| !sim.tech.allows(k))
}

fn poll_orders(mut sim: ResMut<Sim>, mut pending: ResMut<PendingRes>, mut flow: ResMut<Flow>) {
    if pending.0 .0.is_empty() {
        return;
    }
    for m in poll_pending(&mut sim.0, &mut pending.0) {
        flow.push_log(m);
    }
}

// ───────────────────────────── drawing the panels ─────────────────────────────

fn render_items(
    mut commands: Commands,
    items: Res<BuildItems>,
    mut last: Local<Vec<Item>>,
    old: Query<Entity, With<BuildNode>>,
) {
    if *last == items.0 && !old.is_empty() || items.0.is_empty() && old.is_empty() {
        return;
    }
    *last = items.0.clone();
    for e in &old {
        commands.entity(e).despawn();
    }
    for it in &items.0 {
        let node = |r: Rect| Node { position_type: PositionType::Absolute, left: px(r.min.x), top: px(r.min.y), width: px(r.width()), height: px(r.height()), ..default() };
        match &it.kind {
            ItemKind::Panel(c) => {
                let mut e = commands.spawn((BuildNode, node(it.rect), BackgroundColor(*c), GlobalZIndex(5)));
                if *c == PANEL_BG {
                    e.insert((Corner::Panel, kit::hard_shadow()));
                }
            }
            ItemKind::Text { text, color, size } => {
                commands.spawn((BuildNode, node(it.rect), Text::new(text.clone()), kit::font(*size), TextColor(*color), GlobalZIndex(6)));
            }
            ItemKind::Button { label, state, .. } => {
                let (bg, fg) = match state {
                    BtnState::Normal => (BTN_BG, TXT),
                    BtnState::Hot => (BTN_HOT, TXT),
                    BtnState::On => (BTN_ON, kit::RAIL),
                    BtnState::Locked => (BTN_OFF, DIM),
                    BtnState::Disabled => (BTN_OFF, DIM),
                };
                let mut n = node(it.rect);
                n.justify_content = JustifyContent::Center;
                n.align_items = AlignItems::Center;
                commands
                    .spawn((BuildNode, n, BackgroundColor(bg), Corner::Button, GlobalZIndex(6)))
                    .with_children(|p| {
                        p.spawn((Text::new(label.clone()), kit::font(kit::T_BODY), TextColor(fg)));
                    });
            }
        }
    }
}

// ───────────────────────────── world overlays ─────────────────────────────

#[derive(Resource)]
struct Overlays {
    pond: Handle<Mesh>,
    ghost: Handle<Mesh>,
    ok: Handle<StandardMaterial>,
    bad: Handle<StandardMaterial>,
}

#[derive(Component)]
struct PondOverlay;
#[derive(Component)]
struct GhostOverlay;

fn spawn_overlays(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>, mut mats: ResMut<Assets<StandardMaterial>>) {
    let glass = |c: Color| StandardMaterial { base_color: c, unlit: true, alpha_mode: AlphaMode::Blend, cull_mode: None, ..default() };
    let pond_mat = mats.add(glass(Color::srgba(0.12, 0.45, 0.95, 0.55)));
    let ok = mats.add(glass(Color::srgba(0.25, 0.95, 0.40, 0.55)));
    let bad = mats.add(glass(Color::srgba(1.0, 0.25, 0.22, 0.55)));
    let pond = meshes.add(Cuboid::new(0.1, 0.1, 0.1));
    let ghost = meshes.add(Cuboid::new(0.1, 0.1, 0.1));
    commands.spawn((PondOverlay, Mesh3d(pond.clone()), MeshMaterial3d(pond_mat), Transform::default(), Visibility::Hidden));
    commands.spawn((GhostOverlay, Mesh3d(ghost.clone()), MeshMaterial3d(ok.clone()), Transform::default(), Visibility::Hidden));
    commands.insert_resource(Overlays { pond, ghost, ok, bad });
}

/// One axis-aligned box per cell (top and sides; `sides` false = flat top only).
fn cells_mesh(sim: &SimState, cells: &[(Cell, f32, f32)], sides: bool) -> Mesh {
    let (w, d) = (sim.height.width() as f32, sim.height.height() as f32);
    let (ox, oz) = (w * CELL_SIZE / 2.0, d * CELL_SIZE / 2.0);
    let (mut pos, mut nor, mut idx) = (vec![], vec![], vec![]);
    let mut quad = |p: [[f32; 3]; 4], n: [f32; 3]| {
        let i = pos.len() as u32;
        pos.extend(p);
        nor.extend([n; 4]);
        idx.extend([i, i + 2, i + 1, i, i + 3, i + 2]);
    };
    for &((x, y), bot, top) in cells {
        let (x0, x1) = (x as f32 * CELL_SIZE - ox - 0.5, x as f32 * CELL_SIZE - ox + 0.5);
        let (z0, z1) = (y as f32 * CELL_SIZE - oz - 0.5, y as f32 * CELL_SIZE - oz + 0.5);
        quad([[x0, top, z0], [x0, top, z1], [x1, top, z1], [x1, top, z0]], [0.0, 1.0, 0.0]);
        if sides {
            quad([[x0, bot, z0], [x1, bot, z0], [x1, top, z0], [x0, top, z0]], [0.0, 0.0, -1.0]);
            quad([[x1, bot, z1], [x0, bot, z1], [x0, top, z1], [x1, top, z1]], [0.0, 0.0, 1.0]);
            quad([[x0, bot, z1], [x0, bot, z0], [x0, top, z0], [x0, top, z1]], [-1.0, 0.0, 0.0]);
            quad([[x1, bot, z0], [x1, bot, z1], [x1, top, z1], [x1, top, z0]], [1.0, 0.0, 0.0]);
        }
    }
    let uv = vec![[0.0f32, 0.0]; pos.len()];
    Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, pos)
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, nor)
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uv)
        .with_inserted_indices(Indices::U32(idx))
}

#[allow(clippy::too_many_arguments)]
fn update_overlays(
    ui: Res<BuildUi>,
    sim: Res<Sim>,
    ov: Option<Res<Overlays>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut last: Local<Option<(Vec<Cell>, bool, usize, u32)>>,
    mut pond: Query<&mut Visibility, (With<PondOverlay>, Without<GhostOverlay>)>,
    mut ghost: Query<(&mut Visibility, &mut MeshMaterial3d<StandardMaterial>), (With<GhostOverlay>, Without<PondOverlay>)>,
) {
    let Some(ov) = ov else { return };
    let preview = ui.preview.as_ref().filter(|_| ui.tool.is_some());
    let key = preview.map(|p| (p.cells.clone(), p.result.is_ok(), p.dam.as_ref().map_or(0, |d| d.flooded.len()), p.dam.as_ref().map_or(0, |d| d.level.to_bits())));
    if *last == key {
        return;
    }
    *last = key;
    let h = &sim.0.height;
    let ground = |c: Cell| height_at(h, c.0 as f32, c.1 as f32);
    if let (Some(p), Some(kind)) = (preview, ui.tool) {
        let ext = match kind {
            BuildingKind::Dam => ui.dam_height,
            BuildingKind::Canal => 0.15,
            _ => 1.2,
        };
        let level = p.cells.iter().map(|&c| ground(c)).fold(f32::MIN, f32::max);
        let boxes: Vec<_> = p.cells.iter().map(|&c| {
            let g = ground(c);
            let top = if is_path_tool(kind) { g + ext } else { level + ext };
            (c, g - 0.1, top)
        }).collect();
        if let Some(mut m) = meshes.get_mut(&ov.ghost) {
            *m = cells_mesh(&sim.0, &boxes, true);
        }
        for (mut v, mut mat) in &mut ghost {
            *v = Visibility::Inherited;
            mat.0 = if p.result.is_ok() { ov.ok.clone() } else { ov.bad.clone() };
        }
        match &p.dam {
            Some(d) if !d.flooded.is_empty() => {
                let quads: Vec<_> = d.flooded.iter().map(|&c| (c, d.level, d.level + 0.04)).collect();
                if let Some(mut m) = meshes.get_mut(&ov.pond) {
                    *m = cells_mesh(&sim.0, &quads, false);
                }
                for mut v in &mut pond {
                    *v = Visibility::Inherited;
                }
            }
            _ => {
                for mut v in &mut pond {
                    *v = Visibility::Hidden;
                }
            }
        }
    } else {
        for mut v in &mut pond {
            *v = Visibility::Hidden;
        }
        for (mut v, _) in &mut ghost {
            *v = Visibility::Hidden;
        }
    }
}

fn draw_gizmos(mut gizmos: Gizmos, ui: Res<BuildUi>, sim: Res<Sim>, pending: Res<PendingRes>) {
    let h = &sim.0.height;
    let at = |c: Cell, up: f32| cell_world(h, (c.0 as f32, c.1 as f32)) + Vec3::Y * up;
    // dam / canal lines: the crest of the line the player drew
    if let (Some(p), Some(kind)) = (&ui.preview, ui.tool) {
        if is_path_tool(kind) && p.cells.len() >= 1 {
            let col = if p.result.is_ok() { Color::srgb(0.35, 1.0, 0.45) } else { Color::srgb(1.0, 0.35, 0.3) };
            let up = if kind == BuildingKind::Dam { ui.dam_height + 0.3 } else { 0.3 };
            for w in p.cells.windows(2) {
                gizmos.line(at(w[0], up), at(w[1], up), col);
            }
            if let (Some(a), Some(b)) = (p.cells.first(), p.cells.last()) {
                gizmos.sphere(Isometry3d::from_translation(at(*a, up)), 0.6, col);
                gizmos.sphere(Isometry3d::from_translation(at(*b, up)), 0.6, col);
            }
        }
    }
    // ordered but unbuilt dams and canals
    for w in &pending.0 .0 {
        let up = if w.kind == BuildingKind::Dam { 1.2 } else { 0.3 };
        for s in w.cells.windows(2) {
            gizmos.line(at(s[0], up), at(s[1], up), Color::srgb(1.0, 0.75, 0.25));
        }
    }
}

// ───────────────────────────── demo ─────────────────────────────

/// Nearest cell (by ring distance from `near`) where `want` holds.
fn nearest_cell(near: Cell, radius: usize, want: impl Fn(Cell) -> bool) -> Option<Cell> {
    let mut best: Option<(usize, Cell)> = None;
    for dy in -(radius as i64)..=radius as i64 {
        for dx in -(radius as i64)..=radius as i64 {
            let (x, y) = (near.0 as i64 + dx, near.1 as i64 + dy);
            if x < 0 || y < 0 {
                continue;
            }
            let c = (x as usize, y as usize);
            let d = dx.unsigned_abs().max(dy.unsigned_abs()) as usize;
            if best.is_none_or(|b| d < b.0) && want(c) {
                best = Some((d, c));
            }
        }
    }
    best.map(|b| b.1)
}

/// The cell and tool the `--demo-place` scenes show, from the camp at `near`.
pub fn demo_place_scene(sim: &SimState, scene: &str, near: Cell) -> Option<(BuildingKind, Cell)> {
    let n = sim.world.size;
    let inside = |c: Cell| c.0 > 6 && c.1 > 6 && c.0 + 6 < n && c.1 + 6 < n;
    match scene {
        "valid" => nearest_cell((near.0 + 6, near.1), 30, |c| inside(c) && site_preview(sim, BuildingKind::Lodge, c).is_valid()).map(|c| (BuildingKind::Lodge, c)),
        "invalid" => {
            // A lodge ghost half in the water: the nearest cell where the sim says "needs dry ground".
            nearest_cell(near, 40, |c| inside(c) && matches!(site_preview(sim, BuildingKind::Lodge, c).verdict, Err(PlaceError::NeedsDry)))
                .map(|c| (BuildingKind::Lodge, c))
        }
        _ => {
            // The dam spot near the camp whose pond is largest but not the whole map.
            let mut best: Option<(f32, Cell)> = None;
            for y in near.1.saturating_sub(30)..near.1 + 30 {
                for x in near.0.saturating_sub(30)..near.0 + 30 {
                    if !inside((x, y)) {
                        continue;
                    }
                    let p = site_preview(sim, BuildingKind::Dam, (x, y));
                    let Some(d) = p.dam.as_ref().filter(|_| p.is_valid()) else { continue };
                    let area = d.flooded.len();
                    if !(20..=900).contains(&area) {
                        continue;
                    }
                    let score = area as f32 + 30.0 * d.flooded_trees as f32 + 400.0 * d.flooded_buildings.len() as f32;
                    if best.is_none_or(|b| score > b.0) {
                        best = Some((score, (x, y)));
                    }
                }
            }
            best.map(|b| (BuildingKind::Dam, b.1))
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn demo_place(
    time: Res<Time>,
    cli: Res<crate::cli::Cli>,
    mut demo: ResMut<DemoPlace>,
    sim: Res<Sim>,
    mut flow: ResMut<Flow>,
    mut ui: ResMut<BuildUi>,
    mut orbit: ResMut<OrbitCamera>,
    mut gate: ResMut<CaptureGate>,
    windows: Query<&Window, With<PrimaryWindow>>,
    mut exit: MessageWriter<bevy::app::AppExit>,
) {
    if flow.phase != Phase::Settled || demo.done {
        return;
    }
    demo.since += time.delta_secs();
    if demo.since < 0.5 {
        return;
    }
    let scene = cli.demo_place.clone().unwrap_or_default();
    let near = flow.lodge_cell.or_else(|| sim.0.colony.beavers.first().map(|b| (b.pos.0 as usize, b.pos.1 as usize))).unwrap_or((64, 64));
    let Some((kind, hover)) = demo_place_scene(&sim.0, &scene, near) else {
        error!("demo-place {scene}: no suitable cell near the camp");
        exit.write(bevy::app::AppExit::error());
        return;
    };
    info!("demo-place {scene}: {kind:?} hovering {hover:?}");
    ui.tool = Some(kind);
    ui.hover = Some(hover);
    if let Ok(w) = windows.single() {
        ui.fake_cursor = Some(Vec2::new(w.width() / 2.0, w.height() / 2.0));
    }
    *orbit = OrbitCamera { target: cell_world(&sim.0.height, (hover.0 as f32, hover.1 as f32)), yaw: 0.8, pitch: 0.85, distance: if kind == BuildingKind::Dam { 50.0 } else { 30.0 } };
    let all = |_: usize, _: usize| true;
    flow.fog.reveal((hover.0 as f32, hover.1 as f32), 30.0, &all);
    demo.done = true;
    gate.0 = true;
}

/// Best dam line near `near`: a short line across the river that makes a decent pond.
pub fn find_demo_dam(sim: &SimState, near: Cell, height: f32) -> Option<(Cell, Cell, sim::water::DamPreview)> {
    let n = sim.world.size as i64;
    let mut best: Option<(f32, Cell, Cell, sim::water::DamPreview)> = None;
    for dy in -40i64..=40 {
        for dx in -40i64..=40 {
            let (x, y) = (near.0 as i64 + dx, near.1 as i64 + dy);
            if x < 8 || y < 8 || x >= n - 8 || y >= n - 8 {
                continue;
            }
            let c = (x as usize, y as usize);
            if !sim.world.river.get(c.0, c.1).copied().unwrap_or(false) {
                continue;
            }
            for r in [5i64, 8, 11] {
                for (ax, ay) in [(1i64, 0i64), (0, 1), (1, 1), (1, -1)] {
                    let a = ((x - ax * r) as usize, (y - ay * r) as usize);
                    let b = ((x + ax * r) as usize, (y + ay * r) as usize);
                    let cells = line_cells(a, b);
                    let Some(d) = sim::water::preview_dam(sim, &cells, height).ok() else { continue };
                    if d.reaches_map_edge || d.pond_area < 25 || d.pond_area > 700 {
                        continue;
                    }
                    let dist = ((dx * dx + dy * dy) as f32).sqrt();
                    let score = d.pond_area as f32 + 25.0 * d.flooded_trees as f32 - 3.0 * dist;
                    if best.as_ref().is_none_or(|b| score > b.0) {
                        best = Some((score, a, b, d));
                    }
                }
            }
        }
    }
    best.map(|(_, a, b, d)| (a, b, d))
}

#[allow(clippy::too_many_arguments)]
fn demo_dam(
    time: Res<Time>,
    mut demo: ResMut<DemoDam>,
    mut sim: ResMut<Sim>,
    mut flow: ResMut<Flow>,
    mut ui: ResMut<BuildUi>,
    mut orbit: ResMut<OrbitCamera>,
    mut gate: ResMut<CaptureGate>,
    mut sel: ResMut<Selection>,
    mut exit: MessageWriter<bevy::app::AppExit>,
) {
    if flow.phase != Phase::Settled || flow.focus.is_some() {
        gate.0 = false;
        return;
    }
    demo.since += time.delta_secs();
    match demo.stage {
        0 => {
            // Stock the stores so the dam is affordable, wait for water to spin up a little.
            for g in sim::buildings::Good::ALL {
                let _ = sim.0.buildings.stores.deposit(g, 100.0);
            }
            demo.stage = 1;
            demo.since = 0.0;
        }
        1 if demo.since > 1.0 => {
            let near = flow.lodge_cell.unwrap_or((64, 64));
            let h = 1.5;
            match find_demo_dam(&sim.0, near, h) {
                Some((a, b, d)) => {
                    info!("demo-dam: line {a:?} -> {b:?}, pond {} cells, {} trees", d.pond_area, d.flooded_trees);
                    ui.dam_height = h;
                    ui.tool = Some(BuildingKind::Dam);
                    ui.path = vec![a, b];
                    ui.awaiting = true;
                    let mid = ((a.0 + b.0) as f32 / 2.0, (a.1 + b.1) as f32 / 2.0);
                    let target = cell_world(&sim.0.height, mid);
                    *orbit = OrbitCamera { target, yaw: 0.8, pitch: 0.85, distance: 48.0 };
                    let all = |_: usize, _: usize| true;
                    flow.fog.reveal(mid, 30.0, &all);
                    *sel = Selection::None;
                    // Debug aid: BC_DEMO_UI=beaver,order also selects a beaver / fills the order list.
                    let extra = std::env::var("BC_DEMO_UI").unwrap_or_default();
                    if extra.contains("beaver") {
                        if let Some(b) = sim.0.colony.beavers.first() {
                            *sel = Selection::Beaver(b.id);
                        }
                    }
                    if extra.contains("order") {
                        let mut p = Pending::default();
                        for (i, kind) in [BuildingKind::Lodge, BuildingKind::Burrow, BuildingKind::FoodCache].into_iter().enumerate() {
                            for k in 0..30usize {
                                let pos = (near.0 + 4 + k, near.1 + 6 * i);
                                if emit(&mut sim.0, &mut p, &BuildOrder::Building { kind, pos }).is_ok() {
                                    break;
                                }
                            }
                        }
                    }
                    demo.stage = 2;
                    demo.since = 0.0;
                }
                None => {
                    error!("demo-dam: no suitable dam site near the lodge");
                    exit.write(bevy::app::AppExit::error());
                }
            }
        }
        2 => {
            gate.0 = demo.since > 2.0 && ui.preview.as_ref().is_some_and(|p| p.dam.is_some());
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn founded() -> SimState {
        let mut sim = SimState::new(1);
        struct Dry;
        impl sim::colony::Landscape for Dry {
            fn is_water(&self, _: f32, _: f32) -> bool { false }
        }
        let c = sim.world.valleys[sim.world.start.0 as usize].center;
        sim.colony.found(c, &Dry).unwrap();
        for g in sim::buildings::Good::ALL {
            sim.buildings.stores.deposit(g, 100.0).unwrap();
        }
        sim
    }

    #[test]
    fn items_show_the_pond_summary_and_confirm() {
        let sim = founded();
        let mut ui = BuildUi::default();
        let near = sim.world.valleys[sim.world.start.0 as usize].center;
        let (a, b, d) = find_demo_dam(&sim, (near.0 as usize, near.1 as usize), 1.5).expect("a dam site exists");
        ui.tool = Some(BuildingKind::Dam);
        ui.path = vec![a, b];
        ui.awaiting = true;
        ui.dam_height = 1.5;
        compute_preview(&sim, &mut ui);
        assert!(ui.preview.as_ref().unwrap().result.is_ok());
        let items = build_items(Vec2::new(1280.0, 720.0), &sim, &ui, &Pending::default(), Selection::None, None);
        let texts: Vec<&str> = items.iter().filter_map(|i| if let ItemKind::Text { text, .. } = &i.kind { Some(text.as_str()) } else { None }).collect();
        assert!(texts.iter().any(|t| t.contains(&format!("POND  {} cells", d.pond_area)) && t.contains("trees flooded") && t.contains("buildings flooded")));
        assert!(items.iter().any(|i| matches!(&i.kind, ItemKind::Button { action: BAction::Confirm, state: BtnState::On, .. })));
    }

    #[test]
    fn invalid_ghost_gives_reason_and_click_places_one_site() {
        let mut sim = founded();
        let mut ui = BuildUi::default();
        ui.tool = Some(BuildingKind::Workshop);
        ui.hover = Some(crate::build_logic::dry_sites(&sim)[0]);
        compute_preview(&sim, &mut ui);
        assert!(matches!(ui.preview.as_ref().unwrap().result, Err(PlaceError::Locked(..))));
        sim.tech.unlock_all();
        ui.tool = Some(BuildingKind::Lodge);
        compute_preview(&sim, &mut ui);
        assert!(ui.preview.as_ref().unwrap().result.is_ok());
        let before = sim.buildings.buildings.len();
        place_hovered(&mut sim, &mut ui);
        assert_eq!(sim.buildings.buildings.len(), before + 1, "exactly one site");
        // the same spot is now taken: the preview turns invalid and a click does nothing
        compute_preview(&sim, &mut ui);
        assert!(matches!(ui.preview.as_ref().unwrap().result, Err(PlaceError::Blocked)));
        place_hovered(&mut sim, &mut ui);
        assert_eq!(sim.buildings.buildings.len(), before + 1);
    }

    #[test]
    fn dam_hover_preview_is_the_sims_dam_preview() {
        let sim = founded();
        let near = sim.world.valleys[sim.world.start.0 as usize].center;
        let (a, _, _) = find_demo_dam(&sim, (near.0 as usize, near.1 as usize), 1.5).expect("a dam site exists");
        let mut ui = BuildUi::default();
        ui.tool = Some(BuildingKind::Dam);
        ui.hover = Some(a);
        compute_preview(&sim, &mut ui);
        let p = ui.preview.clone().unwrap();
        let want = sim::dam_preview::preview(&sim, &p.cells, sim::dam_preview::DEFAULT_HEIGHT);
        assert!(want.is_some() && p.dam == want);
        let items = build_items(Vec2::new(1280.0, 720.0), &sim, &ui, &Pending::default(), Selection::None, Some(Vec2::new(300.0, 300.0)));
        let d = want.unwrap();
        let texts: Vec<&str> = items.iter().filter_map(|i| if let ItemKind::Text { text, .. } = &i.kind { Some(text.as_str()) } else { None }).collect();
        for line in DamReadout::from_preview(&d).lines() {
            assert!(texts.contains(&line.as_str()), "tooltip shows {line:?}");
        }
    }

}
