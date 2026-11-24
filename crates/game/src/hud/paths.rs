use super::ui::*;
use super::words::{role_kind_icon, role_kind_name};
use crate::icons::Icon;
use crate::flow::{cell_world, height_at};
use bevy::prelude::*;
use sim::jobs::RoleKind;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};

/// Whether the paths view is on screen (set by `sync_lens`, which may show another view over it).
pub static SHOWN: AtomicBool = AtomicBool::new(false);

/// The one role whose paths are shown, picked in the legend: `ALL_ROLES` for every path, `FREE`
/// for free beavers', otherwise the role's place in `RoleKind::ALL`.
static ONLY: AtomicU8 = AtomicU8::new(ALL_ROLES);
const ALL_ROLES: u8 = u8::MAX;
const FREE_ROLE: u8 = u8::MAX - 1;

fn code(r: Option<RoleKind>) -> u8 {
    r.map_or(FREE_ROLE, |r| RoleKind::ALL.iter().position(|k| *k == r).unwrap_or(0) as u8)
}

/// Show only this role's paths, or every path again if it is the one shown.
pub fn toggle_only(r: Option<RoleKind>) {
    let c = code(r);
    let now = ONLY.load(Ordering::Relaxed);
    ONLY.store(if now == c { ALL_ROLES } else { c }, Ordering::Relaxed);
}

/// Whether this role's paths are drawn.
fn shown(r: Option<RoleKind>) -> bool {
    let only = ONLY.load(Ordering::Relaxed);
    only == ALL_ROLES || only == code(r)
}

const LIFT: f32 = 0.18;
const WIDTH: f32 = 3.0;
const FREE: Color = Color::srgb(0.6, 0.55, 0.7);

pub fn role_color(r: Option<RoleKind>) -> Color {
    let Some(r) = r else { return FREE };
    match r {
        RoleKind::Forager => Color::srgb(0.45, 0.85, 0.25),
        RoleKind::Logger => Color::srgb(0.95, 0.55, 0.15),
        RoleKind::MudDigger => Color::srgb(0.75, 0.45, 0.3),
        RoleKind::Builder => Color::srgb(1.0, 0.88, 0.2),
        RoleKind::Hauler => Color::srgb(0.25, 0.65, 1.0),
        RoleKind::Forester => Color::srgb(0.1, 0.6, 0.35),
        RoleKind::Maintainer => Color::srgb(0.85, 0.35, 0.85),
        RoleKind::Guard => Color::srgb(0.95, 0.25, 0.25),
        RoleKind::PebbleCollector => Color::srgb(0.6, 0.75, 0.8),
    }
}

/// The paths' own line width, thicker than the other overlays'.
#[derive(Default, Reflect, GizmoConfigGroup)]
pub struct PathGizmos;

pub fn setup(app: &mut App) {
    app.insert_gizmo_config(PathGizmos, GizmoConfig { line: GizmoLineConfig { width: WIDTH, ..default() }, depth_bias: -0.01, ..default() });
}

/// Each beaver's path still to walk, with its role.
fn walking(s: &sim::SimState) -> impl Iterator<Item = (&sim::agents::Agent, Option<RoleKind>)> {
    s.agents.by_id.values().filter(|a| a.path.as_ref().is_some_and(|p| !p.steps.is_empty())).map(|a| (a, sim::jobs::role_of(s, a.id)))
}

/// The paths view: every beaver's way still to go, in its role's colour, ringed where it ends.
pub fn draw_paths(mut g: Gizmos<PathGizmos>, sim: Res<crate::state::Sim>) {
    if !SHOWN.load(Ordering::Relaxed) {
        return;
    }
    let s = &sim.0;
    let h = &s.height;
    let at = |x: f32, y: f32| {
        let wet = s.water.depth.get(x as usize, y as usize).copied().unwrap_or(0.0).max(0.0);
        cell_world(h, (x, y)).with_y(height_at(h, x, y) + wet + LIFT)
    };
    for (a, role) in walking(s).filter(|(_, r)| shown(*r)) {
        let col = role_color(role);
        let start = s.colony.beaver(a.id).map_or((a.cell.0 as f32 + 0.5, a.cell.1 as f32 + 0.5), |b| b.pos);
        let steps = &a.path.as_ref().expect("walking").steps;
        let mut prev = at(start.0, start.1);
        for &(c, _) in steps {
            let p = at(c.0 as f32 + 0.5, c.1 as f32 + 0.5);
            g.line(prev, p, col);
            prev = p;
        }
        let n = 14;
        for i in 0..n {
            let (a0, a1) = (i as f32 / n as f32 * std::f32::consts::TAU, (i + 1) as f32 / n as f32 * std::f32::consts::TAU);
            g.line(prev + Vec3::new(0.3 * a0.cos(), 0.0, 0.3 * a0.sin()), prev + Vec3::new(0.3 * a1.cos(), 0.0, 0.3 * a1.sin()), col);
        }
    }
}

/// Over the lens rail while the paths view is on, in one row: every role's icon on its colour,
/// each a button that shows only that role's paths (and every path again on a second click).
/// Roles no one is walking for now are dimmed; the name is in the tooltip.
pub fn legend(s: &sim::SimState, node: Node) -> Option<VNode> {
    if !SHOWN.load(Ordering::Relaxed) {
        return None;
    }
    let walking_now: Vec<Option<RoleKind>> = walking(s).map(|(_, r)| r).collect();
    let picked = ONLY.load(Ordering::Relaxed) != ALL_ROLES;
    let roles = RoleKind::ALL.iter().map(|r| Some(*r)).chain(std::iter::once(None));
    let mut ui = Ui::default();
    ui.node(Node { flex_direction: FlexDirection::Row, align_items: AlignItems::Center, column_gap: Val::Px(8.0), padding: UiRect::axes(Val::Px(10.0), Val::Px(4.0)), ..node }, |u| {
        u.icon(Icon::Walk, 14.0, INK).tip(Tip::name("Paths").key("F5"));
        for r in roles {
            let on = shown(r);
            let busy = walking_now.contains(&r);
            let swatch = if on { role_color(r) } else { role_color(r).with_alpha(0.25) };
            let ink = if !on { FAINT.with_alpha(0.5) } else if busy { INK } else { MUTED };
            u.col(2.0, |x| {
                x.icon(r.map_or(Icon::Idle, role_kind_icon), 14.0, ink);
                x.add(VNode::new(Node { width: Val::Px(14.0), height: Val::Px(if picked && on { 5.0 } else { 3.0 }), ..default() })).bg(swatch);
            })
            .style(|n| n.align_items = AlignItems::Center)
            .tip(Tip::name(r.map_or("Free", role_kind_name)).line(if picked && on { "Show all" } else { "Show only these" }, ""))
            .action(super::HudAction::PathsOnly(r));
        }
    });
    let mut v = ui.nodes.pop()?;
    panel(&mut v);
    Some(v)
}
