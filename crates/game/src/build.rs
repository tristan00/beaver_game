//! Build UI: menu grouped by tech branch, placement ghost, dam line + pond preview with flood
//! summary, canal paths, work orders (cancel / priority), beaver and family job panels and the
//! jobs panel. Rules live in `build_logic`; this file is input, layout and drawing.
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
use sim::colony::{BeaverId, FamilyId, Job, OrderId};
use sim::water::DamPreview;
use sim::SimState;

const PANEL_BG: Color = Color::srgba(0.05, 0.07, 0.10, 0.86);
const BTN_BG: Color = Color::srgba(0.16, 0.20, 0.26, 0.94);
const BTN_HOT: Color = Color::srgba(0.30, 0.38, 0.50, 0.96);
const BTN_ON: Color = Color::srgba(0.20, 0.48, 0.30, 0.96);
const BTN_OFF: Color = Color::srgba(0.12, 0.13, 0.15, 0.80);
const TXT: Color = Color::srgb(0.92, 0.93, 0.95);
const DIM: Color = Color::srgb(0.52, 0.55, 0.60);
const WARN: Color = Color::srgb(1.0, 0.72, 0.28);
const BAD: Color = Color::srgb(1.0, 0.42, 0.40);
const GOOD: Color = Color::srgb(0.50, 0.92, 0.58);
const POND: Color = Color::srgb(0.45, 0.72, 1.0);

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
    pub menu_open: bool,
    pub jobs_open: bool,
    pub family_open: bool,
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
    dam_cache: Option<(Vec<Cell>, u32, Option<DamPreview>)>,
}

impl Default for BuildUi {
    fn default() -> Self {
        BuildUi {
            menu_open: true,
            jobs_open: false,
            family_open: false,
            tool: None,
            dam_height: DEFAULT_DAM_HEIGHT,
            path: vec![],
            drawing: false,
            awaiting: false,
            hover: None,
            message: String::new(),
            preview: None,
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
    fn select_tool(&mut self, k: Option<BuildingKind>) {
        self.tool = k;
        self.clear_path();
        self.preview = None;
    }
}

#[derive(Resource, Default)]
pub struct PendingRes(pub Pending);

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum BAction {
    ToggleMenu,
    ToggleJobs,
    Pick(BuildingKind),
    Confirm,
    CancelTool,
    Height(i32),
    CancelOrder(OrderTarget),
    Prioritise(OrderId),
    SetJob(BeaverId, Option<Job>),
    JobCount(Job, i32),
    ToggleFamily,
    Promote(FamilyId, usize),
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

fn job_name(j: Job) -> String {
    format!("{j:?}")
}

fn pond_headline(d: &DamPreview) -> (String, Color) {
    let n = d.flooded_trees + d.flooded_buildings;
    let text = format!(
        "POND  {} cells   {} trees flooded   {} buildings flooded",
        d.pond_area, d.flooded_trees, d.flooded_buildings
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
    l.button(rc(10.0, 36.0, 92.0, 26.0), BAction::ToggleMenu, "Build  B", ui.menu_open);
    l.button(rc(106.0, 36.0, 84.0, 26.0), BAction::ToggleJobs, "Jobs  J", ui.jobs_open);
    let colony = &sim.colony;

    // ── build menu ──
    if ui.menu_open {
        let (x, y0, w) = (10.0, 66.0, 390.0);
        let entries = menu(sim);
        let mut groups = 0;
        let mut last = None;
        for e in &entries {
            if last != Some(e.group) {
                groups += 1;
                last = Some(e.group);
            }
        }
        let h = 10.0 + groups as f32 * 22.0 + entries.len() as f32 * 24.0;
        l.panel(rc(x, y0, w, h), PANEL_BG);
        let mut y = y0 + 6.0;
        let mut last = None;
        for e in &entries {
            if last != Some(e.group) {
                last = Some(e.group);
                let head = match e.group {
                    Group::Base => "BASICS".to_string(),
                    Group::Branch(b) => format!("{}   experience {:.1}", e.group.name().to_uppercase(), b.experience(colony)),
                };
                l.text(rc(x + 8.0, y + 2.0, w - 16.0, 20.0), head, Color::srgb(0.78, 0.84, 0.55), 12.0);
                y += 22.0;
            }
            let spec = e.kind.spec();
            let cost = if e.group == Group::Base || e.unlocked { path_cost_unit(e.kind) } else { vec![] };
            let state = if !e.unlocked {
                BtnState::Locked
            } else if ui.tool == Some(e.kind) {
                BtnState::On
            } else if cursor.is_some_and(|c| rc(x + 6.0, y, 112.0, 22.0).contains(c)) {
                BtnState::Hot
            } else {
                BtnState::Normal
            };
            l.button_state(rc(x + 6.0, y, 112.0, 22.0), BAction::Pick(e.kind), spec.name, state);
            let (txt, col) = match e.need {
                Some((tech, need, have)) => (format!("locked: {tech}  xp {have:.1}/{need:.0}"), DIM),
                None => {
                    let ok = cost.iter().all(|&(g, a)| sim.buildings.stores.amount(g) + 1e-4 >= a);
                    (format!("{}{}", cost_text(&cost), match e.kind { BuildingKind::Dam => "  /6 cells", BuildingKind::Canal => "  /cell", _ => "" }), if ok { TXT } else { WARN })
                }
            };
            l.text(rc(x + 124.0, y + 4.0, w - 130.0, 18.0), txt, col, 11.0);
            y += 24.0;
        }
    }

    // ── jobs panel ──
    if ui.jobs_open {
        let (x, y0, w) = (if ui.menu_open { 410.0 } else { 10.0 }, 66.0, 340.0);
        let n_adults = adults(colony);
        let h = 62.0 + Job::ALL.len() as f32 * 26.0 + 22.0;
        l.panel(rc(x, y0, w, h), PANEL_BG);
        l.text(rc(x + 10.0, y0 + 8.0, w - 20.0, 20.0), format!("JOBS   adults {}   unassigned {}", n_adults, unassigned(colony)), TXT, 13.0);
        l.text(rc(x + 10.0, y0 + 30.0, w - 20.0, 16.0), "job            now      assigned", DIM, 11.0);
        let mut y = y0 + 48.0;
        for r in job_rows(colony) {
            l.text(rc(x + 10.0, y + 5.0, 80.0, 18.0), job_name(r.job), TXT, 13.0);
            l.text(rc(x + 96.0, y + 5.0, 40.0, 18.0), format!("{}", r.working), if r.working > 0 { GOOD } else { DIM }, 13.0);
            l.bar(rc(x + 140.0, y + 9.0, 90.0, 10.0), r.assigned as f32 / n_adults.max(1) as f32, Color::srgb(0.45, 0.75, 0.50));
            l.text(rc(x + 240.0, y + 5.0, 26.0, 18.0), format!("{}", r.assigned), TXT, 13.0);
            l.button(rc(x + 270.0, y + 2.0, 26.0, 22.0), BAction::JobCount(r.job, -1), "-", false);
            l.button(rc(x + 300.0, y + 2.0, 26.0, 22.0), BAction::JobCount(r.job, 1), "+", false);
            y += 26.0;
        }
        l.text(rc(x + 10.0, y + 4.0, w - 20.0, 16.0), "Unassigned adults follow their family's priorities.", DIM, 11.0);
    }

    // ── tool panel (bottom centre) ──
    if let Some(kind) = ui.tool {
        let spec = kind.spec();
        let w = 520.0;
        let x = ((win.x - w) / 2.0).max(410.0);
        let mut lines: Vec<(String, Color, f32)> = vec![];
        let hint = match kind {
            BuildingKind::Dam => "drag a line across the water".to_string(),
            BuildingKind::Canal => "drag a path from the water".to_string(),
            _ => format!("click to place ({}x{})", spec.footprint.0, spec.footprint.1),
        };
        lines.push((format!("{}  -  {}   (Esc to stop)", spec.name, hint), TXT, 14.0));
        if let Some(p) = &ui.preview {
            if let Some(d) = &p.dam {
                let (t, c) = pond_headline(d);
                lines.push((t, c, 14.0));
                lines.push((format!("water level {:.1}   volume {:.0}   dam {} cells x {:.1} high", d.level, d.volume, p.cells.len(), ui.dam_height), POND, 12.0));
                if d.reaches_map_edge {
                    lines.push(("Does not close the valley: the water would run off the map edge.".into(), WARN, 12.0));
                }
            }
            if !p.cost.is_empty() {
                let ok = p.cost.iter().all(|&(g, a)| sim.buildings.stores.amount(g) + 1e-4 >= a);
                lines.push((format!("Cost: {}", cost_text(&p.cost)), if ok { TXT } else { WARN }, 12.0));
            }
            match &p.result {
                Ok(()) if p.order.is_some() && is_path_tool(kind) && !ui.awaiting => lines.push(("Release to review".into(), DIM, 12.0)),
                Ok(()) if is_path_tool(kind) => lines.push(("Ready. Confirm with Enter or the button.".into(), GOOD, 12.0)),
                Ok(()) => lines.push(("Click to build here".into(), GOOD, 12.0)),
                Err(PlaceError::NoCells) => lines.push(("Nothing drawn yet.".into(), DIM, 12.0)),
                Err(e) => lines.push((format!("Cannot build here: {e}"), BAD, 12.0)),
            }
        } else if is_path_tool(kind) {
            lines.push(("Press and drag on the map.".into(), DIM, 12.0));
        } else {
            lines.push(("Move over the map to see the ghost.".into(), DIM, 12.0));
        }
        if !ui.message.is_empty() {
            lines.push((ui.message.clone(), WARN, 12.0));
        }
        let h = 14.0 + lines.iter().map(|l| l.2 + 6.0).sum::<f32>() + if is_path_tool(kind) { 34.0 } else { 0.0 };
        let y0 = win.y - 10.0 - h;
        l.panel(rc(x, y0, w, h), PANEL_BG);
        let mut y = y0 + 8.0;
        for (t, c, s) in lines {
            l.text(rc(x + 10.0, y, w - 20.0, s + 4.0), t, c, s);
            y += s + 6.0;
        }
        if is_path_tool(kind) {
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

    // ── right column, stacked from the bottom: beaver/family, then orders ──
    let right_w = 330.0;
    let rx = win.x - 10.0 - right_w;
    let mut bottom = win.y - 10.0;
    if let Selection::Beaver(id) = sel {
        if let Some(b) = colony.beaver(id) {
            let fam = colony.family(b.family);
            let fam_rows = if ui.family_open { fam.map_or(0, |f| f.priorities.len()) } else { 0 };
            let h = 10.0 + 58.0 + 3.0 * 28.0 + 30.0 + if ui.family_open { 22.0 + fam_rows as f32 * 26.0 } else { 0.0 };
            let y0 = bottom - h;
            l.panel(rc(rx, y0, right_w, h), PANEL_BG);
            let cur = b.current_job.map_or("idle".to_string(), job_name);
            let set = b.job.map_or("Auto".to_string(), job_name);
            l.text(
                rc(rx + 10.0, y0 + 8.0, right_w - 20.0, 54.0),
                format!(
                    "{}   {:?}, {:.1} years\nFamily {}   mood {:.0}%   health {:.0}%\nDoing: {}   Set to: {}",
                    b.name, b.stage, b.age_years(), fam.map_or("?", |f| f.name.as_str()), b.mood * 100.0, b.health * 100.0, cur, set
                ),
                TXT,
                12.0,
            );
            let mut opts: Vec<Option<Job>> = vec![None];
            opts.extend(Job::ALL.iter().map(|j| Some(*j)));
            for (i, o) in opts.iter().enumerate() {
                let (cx, cy) = (rx + 8.0 + (i % 3) as f32 * 106.0, y0 + 64.0 + (i / 3) as f32 * 28.0);
                l.button(rc(cx, cy, 102.0, 24.0), BAction::SetJob(id, *o), o.map_or("Auto".to_string(), job_name), b.job == *o);
            }
            let fy = y0 + 64.0 + 3.0 * 28.0 + 2.0;
            l.button(rc(rx + 8.0, fy, 200.0, 24.0), BAction::ToggleFamily, if ui.family_open { "Family priorities  v" } else { "Family priorities  >" }, ui.family_open);
            if let (true, Some(f)) = (ui.family_open, fam) {
                let mut y = fy + 28.0;
                l.text(rc(rx + 10.0, y, right_w - 20.0, 16.0), "first = most wanted", DIM, 11.0);
                y += 18.0;
                for (i, j) in f.priorities.iter().enumerate() {
                    l.text(rc(rx + 12.0, y + 4.0, 200.0, 18.0), format!("{}. {}", i + 1, job_name(*j)), TXT, 13.0);
                    if i > 0 {
                        l.button(rc(rx + 250.0, y, 60.0, 22.0), BAction::Promote(f.id, i), "up", false);
                    }
                    y += 26.0;
                }
            }
            bottom = y0 - 8.0;
        }
    }
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
        BuildingKind::Dam => {
            let cells = if ui.path.len() >= 2 { line_cells(ui.path[0], *ui.path.last().unwrap()) } else { ui.path.clone() };
            if cells.is_empty() {
                None
            } else {
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
        }
        BuildingKind::Canal => {
            if ui.path.is_empty() {
                None
            } else {
                let cells = ui.path.clone();
                Some(Preview { result: check_canal(sim, &cells), cost: path_cost(kind, cells.len()), order: Some(BuildOrder::Canal { cells: cells.clone() }), cells, dam: None })
            }
        }
        _ => ui.hover.map(|c| {
            let pos = anchor(kind, c);
            Preview {
                result: check_building(sim, kind, pos),
                cost: kind.spec().cost.to_vec(),
                order: Some(BuildOrder::Building { kind, pos }),
                cells: footprint(kind, pos),
                dam: None,
            }
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

pub struct BuildPlugin;

impl Plugin for BuildPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<BuildUi>()
            .init_resource::<PendingRes>()
            .init_resource::<BuildItems>()
            .add_systems(Startup, spawn_overlays)
            .add_systems(Update, (poll_orders, render_items, update_overlays, draw_gizmos).after(ClickSet));
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
    let new = build_items(win, &sim.0, &ui, &pending.0, *sel, w.cursor_position());
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
    if keys.just_pressed(KeyCode::KeyB) {
        ui.menu_open = !ui.menu_open;
    }
    if keys.just_pressed(KeyCode::KeyJ) {
        ui.jobs_open = !ui.jobs_open;
    }
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
            ui.drawing = false;
            ui.awaiting = ui.path.len() > 1 || tool == BuildingKind::Canal;
            ui.message.clear();
        }
    } else if clicked && !over_ui {
        if let Some(Preview { order: Some(o), .. }) = ui.preview.clone() {
            match emit(&mut sim.0, &mut pending.0, &o) {
                Ok(m) => ui.message = m,
                Err(e) => ui.message = format!("{e}"),
            }
        }
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
        BAction::ToggleMenu => ui.menu_open = !ui.menu_open,
        BAction::ToggleJobs => ui.jobs_open = !ui.jobs_open,
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
        BAction::SetJob(id, job) => {
            if let Err(e) = sim.colony.set_job(id, job) {
                ui.message = e.to_string();
            }
        }
        BAction::JobCount(job, d) => {
            let cur = sim.colony.beavers.iter().filter(|b| b.job == Some(job)).count() as i32;
            if let Err(e) = set_job_count(&mut sim.colony, job, (cur + d).max(0) as usize) {
                ui.message = e;
            }
        }
        BAction::ToggleFamily => ui.family_open = !ui.family_open,
        BAction::Promote(f, i) => {
            if let Err(e) = promote_priority(&mut sim.colony, f, i) {
                ui.message = e;
            }
        }
    }
    let _ = sel;
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
                commands.spawn((BuildNode, node(it.rect), BackgroundColor(*c), GlobalZIndex(5)));
            }
            ItemKind::Text { text, color, size } => {
                commands.spawn((BuildNode, node(it.rect), Text::new(text.clone()), TextFont::from_font_size(*size), TextColor(*color), GlobalZIndex(6)));
            }
            ItemKind::Button { label, state, .. } => {
                let (bg, fg) = match state {
                    BtnState::Normal => (BTN_BG, TXT),
                    BtnState::Hot => (BTN_HOT, TXT),
                    BtnState::On => (BTN_ON, TXT),
                    BtnState::Locked => (BTN_OFF, DIM),
                    BtnState::Disabled => (BTN_OFF, DIM),
                };
                let mut n = node(it.rect);
                n.justify_content = JustifyContent::Center;
                n.align_items = AlignItems::Center;
                commands
                    .spawn((BuildNode, n, BackgroundColor(bg), GlobalZIndex(6)))
                    .with_children(|p| {
                        p.spawn((Text::new(label.clone()), TextFont::from_font_size(13.0), TextColor(fg)));
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
    let key = preview.map(|p| (p.cells.clone(), p.result.is_ok(), p.dam.as_ref().map_or(0, |d| d.pond_area), p.dam.as_ref().map_or(0, |d| d.level.to_bits())));
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
            Some(d) if !d.flooded_cells.is_empty() => {
                let quads: Vec<_> = d.flooded_cells.iter().map(|&c| (c, d.level, d.level + 0.04)).collect();
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

/// Best dam line near `near`: a short line across the river that makes a decent pond.
pub fn find_demo_dam(sim: &SimState, near: Cell, height: f32) -> Option<(Cell, Cell, DamPreview)> {
    let n = sim.world.size as i64;
    let mut best: Option<(f32, Cell, Cell, DamPreview)> = None;
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
                    let Some(d) = dam_preview(sim, &cells, height) else { continue };
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
                    ui.menu_open = true;
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
                    // Debug aid: BC_DEMO_UI=jobs,beaver,order also opens those panels.
                    let extra = std::env::var("BC_DEMO_UI").unwrap_or_default();
                    ui.jobs_open = extra.contains("jobs");
                    if extra.contains("beaver") {
                        ui.family_open = true;
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
    fn items_show_menu_groups_locked_hint_and_pond_summary() {
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
        assert!(texts.iter().any(|t| t.starts_with("WATER")), "branch header");
        assert!(texts.iter().any(|t| t.contains("locked: Flow Control  xp 0.0/3")), "locked item shows experience needed");
        assert!(texts.iter().any(|t| t.contains(&format!("POND  {} cells", d.pond_area)) && t.contains("trees flooded") && t.contains("buildings flooded")));
        assert!(items.iter().any(|i| matches!(&i.kind, ItemKind::Button { action: BAction::Confirm, state: BtnState::On, .. })));
    }

    #[test]
    fn invalid_ghost_gives_reason_and_building_click_emits_order() {
        let mut sim = founded();
        let mut ui = BuildUi::default();
        ui.tool = Some(BuildingKind::Workshop);
        ui.hover = Some((60, 60));
        compute_preview(&sim, &mut ui);
        assert!(matches!(ui.preview.as_ref().unwrap().result, Err(PlaceError::Locked(..))));
        ui.tool = Some(BuildingKind::Lodge);
        compute_preview(&sim, &mut ui);
        let p = ui.preview.clone().unwrap();
        assert!(p.result.is_ok());
        let mut pend = Pending::default();
        let before = sim.colony.orders.len();
        emit(&mut sim, &mut pend, p.order.as_ref().unwrap()).unwrap();
        assert_eq!(sim.colony.orders.len(), before + 1);
        let items = build_items(Vec2::new(1280.0, 720.0), &sim, &ui, &pend, Selection::None, None);
        assert!(items.iter().any(|i| matches!(&i.kind, ItemKind::Button { action: BAction::CancelOrder(_), .. })), "orders panel has cancel");
    }
}
