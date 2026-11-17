//! Orders on the map: right-click and drag with beavers selected, and moving or resizing work
//! areas by dragging them. The pointer is turned into cells here; what a gesture means is decided
//! by `ui::gestures` (pure, tested without a window).
//!
//! `--demo-orders drag|resize` (with `--start 2`) plays one gesture through the same code for
//! screenshots, since a headless run has no mouse.
use crate::build::BuildUi;
use crate::camera::{MainCamera, OrbitCamera};
use crate::cli::Cli;
use crate::flow::{cell_world, ray_heightfield, CaptureGate, Flow, Phase, Selection};
use crate::flow_view::{ClickSet, UiLayout};
use crate::state::Sim;
use crate::ui::gestures::{area_between, dominant_kind, work_area_hit, Handle, MapPointer};
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use sim::agents::Assignment;
use sim::colony::{BeaverId, Stage};
use sim::resources::ResourceKind;
use sim::water::Cell;
use sim::work::{Area, Role};
use sim::Command;

/// A press that moves less than this many pixels is a click, not a drag or an orbit.
const CLICK_SLOP: f32 = 5.0;

/// Gizmos for work-area outlines: thick, and drawn over trees and hills so the area stays readable.
#[derive(Default, Reflect, GizmoConfigGroup)]
struct OrderGizmos;

fn configure_gizmos(mut store: ResMut<GizmoConfigStore>) {
    let (config, _) = store.config_mut::<OrderGizmos>();
    config.line.width = 4.0;
    config.depth_bias = -1.0;
}

#[derive(Resource, Default)]
pub struct MapOrders {
    pub pointer: MapPointer,
    /// True while a left-drag belongs to an order, so the camera must not orbit.
    pub capture: bool,
    right_press: Option<Vec2>,
    /// The work area as it was before the last edit, shown faintly for the demo screenshot.
    ghost: Option<Area>,
}

pub struct MapOrdersPlugin;

impl Plugin for MapOrdersPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<MapOrders>().init_gizmo_group::<OrderGizmos>().add_systems(Startup, configure_gizmos).add_systems(Update, map_orders.after(ClickSet)).add_systems(Update, draw_areas.after(map_orders));
        if app.world().resource::<Cli>().demo_orders.is_some() {
            app.insert_resource(CaptureGate(false)).init_resource::<DemoOrders>().add_systems(Update, demo_orders.after(map_orders));
        }
    }
}

/// The beavers the player has selected (the colony selection is one beaver at a time).
fn selected(sel: &Selection) -> Vec<BeaverId> {
    match sel {
        Selection::Beaver(id) => vec![*id],
        _ => Vec::new(),
    }
}

fn apply(sim: &mut sim::SimState, ui: &mut BuildUi, cmds: Vec<Command>) {
    let n = cmds.len();
    for c in cmds {
        let what = describe(&c);
        match sim.apply(c) {
            Ok(()) => ui.message = what,
            Err(e) => {
                ui.message = e.to_string();
                return;
            }
        }
    }
    if n == 0 {
        ui.message.clear();
    }
}

fn describe(c: &Command) -> String {
    match c {
        Command::Assign { who, to: Some(Assignment::Resource(_)) } => format!("{} beaver(s) sent to collect", who.len()),
        Command::Assign { who, to: Some(Assignment::Area(k, a)) } => format!("{} beaver(s) collect {} in {}x{} cells", who.len(), kind_name(*k), a.max.0 - a.min.0 + 1, a.max.1 - a.min.1 + 1),
        Command::FillSlot { .. } => "Builder assigned to the site".into(),
        Command::SetWorkArea { area, .. } => format!("Work area now {}x{} cells", area.max.0 - area.min.0 + 1, area.max.1 - area.min.1 + 1),
        _ => "Order sent".into(),
    }
}

fn kind_name(k: ResourceKind) -> &'static str {
    match k {
        ResourceKind::Tree | ResourceKind::Felled => "wood",
        ResourceKind::Plant => "plants",
        ResourceKind::MudBank => "mud",
        ResourceKind::StoneOutcrop => "stone",
    }
}

#[allow(clippy::too_many_arguments)]
pub fn map_orders(
    buttons: Res<ButtonInput<MouseButton>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    cams: Query<(&Camera, &GlobalTransform), With<MainCamera>>,
    layout: Res<UiLayout>,
    flow: Res<Flow>,
    sel: Res<Selection>,
    mut sim: ResMut<Sim>,
    mut ui: ResMut<BuildUi>,
    mut orders: ResMut<MapOrders>,
) {
    let o = &mut *orders;
    if !sim.0.colony.is_founded() || matches!(flow.phase, Phase::GameOver | Phase::Scouting) || ui.tool.is_some() {
        o.pointer.cancel();
        o.capture = false;
        return;
    }
    let Ok(window) = windows.single() else { return };
    let Some(cursor) = window.cursor_position() else { return };
    let over_ui = layout.over_ui(cursor);
    let cell: Option<Cell> = if over_ui {
        None
    } else {
        cams.single().ok().and_then(|(camera, gt)| {
            let ray = camera.viewport_to_world(gt, cursor).ok()?;
            ray_heightfield(ray.origin, *ray.direction, &sim.0.height).map(|t| t.0)
        })
    };
    let who = selected(&sel);
    if buttons.just_pressed(MouseButton::Left) {
        o.capture = cell.is_some_and(|c| o.pointer.left_down(&sim.0, !who.is_empty(), c));
    }
    if let (true, Some(c)) = (buttons.pressed(MouseButton::Left), cell) {
        o.pointer.moved(c);
    }
    if buttons.just_released(MouseButton::Left) {
        let cmds = match cell {
            Some(c) => o.pointer.left_up(&sim.0, &who, c),
            None => Vec::new(),
        };
        o.pointer.cancel();
        o.capture = false;
        apply(&mut sim.0, &mut ui, cmds);
    }
    if buttons.just_pressed(MouseButton::Right) && !over_ui {
        o.right_press = Some(cursor);
    }
    if buttons.just_released(MouseButton::Right) {
        if let (Some(start), Some(c)) = (o.right_press.take(), cell) {
            if start.distance(cursor) <= CLICK_SLOP {
                let cmds = o.pointer.right_click(&sim.0, &who, c);
                apply(&mut sim.0, &mut ui, cmds);
            }
        }
    }
}

// ───────────────────────────── drawing ─────────────────────────────

fn area_colour(role: Option<Role>) -> Color {
    match role {
        Some(Role::Collect(ResourceKind::Tree | ResourceKind::Felled)) => Color::srgb(0.45, 0.95, 0.4),
        Some(Role::Collect(ResourceKind::Plant)) => Color::srgb(0.7, 1.0, 0.55),
        Some(Role::Collect(ResourceKind::MudBank)) => Color::srgb(0.95, 0.7, 0.4),
        Some(Role::Collect(ResourceKind::StoneOutcrop)) => Color::srgb(0.8, 0.85, 0.95),
        _ => Color::srgb(0.9, 0.9, 0.5),
    }
}

/// Outline of `area` following the terrain, `lift` above it.
fn outline(gizmos: &mut Gizmos<OrderGizmos>, sim: &sim::SimState, area: Area, lift: f32, col: Color) {
    let h = &sim.height;
    let at = |x: f32, y: f32| cell_world(h, (x, y)) + Vec3::Y * lift;
    let (x0, x1) = (area.min.0 as f32, area.max.0 as f32 + 1.0);
    let (y0, y1) = (area.min.1 as f32, area.max.1 as f32 + 1.0);
    let mut edge = |from: (f32, f32), to: (f32, f32)| {
        let n = ((to.0 - from.0).abs() + (to.1 - from.1).abs()).ceil().max(1.0) as usize;
        let p = |i: usize| {
            let t = i as f32 / n as f32;
            at(from.0 + (to.0 - from.0) * t, from.1 + (to.1 - from.1) * t)
        };
        for i in 0..n {
            gizmos.line(p(i), p(i + 1), col);
        }
    };
    edge((x0, y0), (x1, y0));
    edge((x1, y0), (x1, y1));
    edge((x1, y1), (x0, y1));
    edge((x0, y1), (x0, y0));
}

fn draw_areas(mut gizmos: Gizmos<OrderGizmos>, sim: Res<Sim>, sel: Res<Selection>, ui: Res<BuildUi>, orders: Res<MapOrders>) {
    if !sim.0.colony.is_founded() {
        return;
    }
    let preview = orders.pointer.preview(&sim.0);
    let hover_hit = ui.hover.and_then(|c| work_area_hit(&sim.0, c)).map(|(b, _)| b);
    for p in &sim.0.work.places {
        let Some(area) = p.area else { continue };
        let being_edited = preview.is_some_and(|d| d.editing == Some(p.building));
        let lit = being_edited || hover_hit == Some(p.building);
        let col = area_colour(p.slots.first().map(|s| s.role));
        let col = if being_edited { col.with_alpha(0.3) } else if lit { col } else { col.with_alpha(0.55) };
        outline(&mut gizmos, &sim.0, area, 0.35, col);
        if lit {
            outline(&mut gizmos, &sim.0, area, 0.5, col);
        }
    }
    if let Some(a) = orders.ghost {
        outline(&mut gizmos, &sim.0, a, 0.45, Color::srgba(1.0, 1.0, 1.0, 0.5));
    }
    // the drag in progress: a new collect rectangle or the work area as it would become
    if let Some(d) = preview {
        let col = if d.editing.is_some() { Color::srgb(1.0, 0.95, 0.4) } else { Color::srgb(0.4, 0.9, 1.0) };
        outline(&mut gizmos, &sim.0, d.area, 0.6, col);
        outline(&mut gizmos, &sim.0, d.area, 0.8, col);
    }
    // where the selected beavers were sent
    if let Selection::Beaver(id) = *sel {
        if let Some(Assignment::Area(k, a)) = sim.0.agents.get(id).and_then(|a| a.assignment) {
            let c = area_colour(Some(Role::Collect(k)));
            outline(&mut gizmos, &sim.0, a, 0.7, c);
            outline(&mut gizmos, &sim.0, a, 0.9, c);
        }
    }
}

// ───────────────────────────── scripted demo ─────────────────────────────

#[derive(Resource, Default)]
struct DemoOrders {
    since: f32,
    done_at: Option<f32>,
    phase: u8,
}

/// The cells of a nine-wide window of trees near `from`, reachable within a short walk.
fn tree_window(sim: &sim::SimState, from: (f32, f32)) -> Option<Area> {
    let trees: Vec<Cell> = sim.resources.items.iter().filter(|r| r.kind == ResourceKind::Tree).map(|r| r.cell).collect();
    let near = |c: &Cell| (c.0 as f32 - from.0).abs() < 14.0 && (c.1 as f32 - from.1).abs() < 14.0;
    let dense = |c: &Cell| trees.iter().filter(|t| t.0.abs_diff(c.0) <= 4 && t.1.abs_diff(c.1) <= 4).count();
    let best = trees.iter().copied().filter(near).max_by_key(dense)?;
    Some(area_between((best.0.saturating_sub(4), best.1.saturating_sub(4)), (best.0 + 4, best.1 + 4)))
}

#[allow(clippy::too_many_arguments)]
fn demo_orders(
    time: Res<Time>,
    cli: Res<Cli>,
    mut demo: ResMut<DemoOrders>,
    mut sim: ResMut<Sim>,
    mut flow: ResMut<Flow>,
    mut ui: ResMut<BuildUi>,
    mut sel: ResMut<Selection>,
    mut orbit: ResMut<OrbitCamera>,
    mut orders: ResMut<MapOrders>,
    mut gate: ResMut<CaptureGate>,
    mut exit: MessageWriter<bevy::app::AppExit>,
) {
    if flow.phase != Phase::Settled {
        return;
    }
    demo.since += time.delta_secs();
    if demo.since < 0.5 {
        return;
    }
    let scene = cli.demo_orders.clone().unwrap_or_default();
    let fail = |msg: &str, exit: &mut MessageWriter<bevy::app::AppExit>| {
        error!("demo-orders {scene}: {msg}");
        exit.write(bevy::app::AppExit::error());
    };
    if demo.phase == 0 {
        demo.phase = 1;
        sim.0.work.auto_organise = false; // the picture shows the player's order, not the family's own plans
        let (focus, cmds) = if scene == "drag" {
            let Some(b) = sim.0.colony.beavers.iter().find(|b| b.stage == Stage::Adult).map(|b| (b.id, b.pos)) else { return fail("no adult beaver", &mut exit) };
            let Some(area) = tree_window(&sim.0, b.1) else { return fail("no trees near the beaver", &mut exit) };
            *sel = Selection::Beaver(b.0);
            let who = vec![b.0];
            // the same path a mouse drag takes
            let started = orders.pointer.left_down(&sim.0, true, area.min);
            debug_assert!(started);
            orders.pointer.moved((area.centre().0, area.min.1));
            let cmds = orders.pointer.left_up(&sim.0, &who, area.max);
            (area.centre(), cmds)
        } else {
            let Some((building, area)) = sim.0.work.places.iter().find_map(|p| p.area.map(|a| (p.building, a))) else { return fail("no work area in this start", &mut exit) };
            // grab the middle of the east edge and pull it four cells outwards
            let edge = (area.max.0, area.centre().1);
            let started = orders.pointer.left_down(&sim.0, false, edge);
            let hit = work_area_hit(&sim.0, edge);
            debug_assert!(started && matches!(hit, Some((b, Handle::Edge { east: true, .. })) if b == building));
            orders.ghost = Some(area);
            let to = (edge.0 + 4, edge.1);
            orders.pointer.moved(to);
            let cmds = orders.pointer.left_up(&sim.0, &[], to);
            (area.centre(), cmds)
        };
        if cmds.is_empty() {
            return fail("the gesture issued no command", &mut exit);
        }
        info!("demo-orders {scene}: {cmds:?}");
        apply(&mut sim.0, &mut ui, cmds);
        let target = cell_world(&sim.0.height, (focus.0 as f32, focus.1 as f32));
        *orbit = OrbitCamera { target, yaw: 0.8, pitch: 0.9, distance: 24.0 };
        let all = |_: usize, _: usize| true;
        flow.fog.reveal((focus.0 as f32, focus.1 as f32), 30.0, &all);
        demo.done_at = Some(flow.real_secs);
    }
    // hold a moment so the beaver is on its way and the outline is up
    if let Some(at) = demo.done_at {
        gate.0 = flow.real_secs - at >= 1.5;
    }
}
