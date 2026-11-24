use super::ui::*;
use super::words::role_kind_name;
use crate::flow::{cell_world, height_at};
use bevy::prelude::*;
use sim::jobs::RoleKind;
use std::sync::atomic::{AtomicBool, Ordering};

/// Whether the paths view is on screen (set by `sync_lens`, which may show another view over it).
pub static SHOWN: AtomicBool = AtomicBool::new(false);

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
    for (a, role) in walking(s) {
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

/// Beside the rail while the paths view is on: a swatch and name for each role walking now.
pub fn legend(s: &sim::SimState, node: Node) -> Option<VNode> {
    if !SHOWN.load(Ordering::Relaxed) {
        return None;
    }
    let mut roles: Vec<Option<RoleKind>> = walking(s).map(|(_, r)| r).collect();
    roles.sort_by_key(|r| r.map_or(usize::MAX, |r| RoleKind::ALL.iter().position(|k| *k == r).unwrap_or(0)));
    roles.dedup();
    if roles.is_empty() {
        return None;
    }
    let mut ui = Ui::default();
    ui.node(Node { flex_direction: FlexDirection::Column, row_gap: Val::Px(3.0), padding: UiRect::all(Val::Px(8.0)), ..node }, |u| {
        for r in roles {
            u.row(6.0, |x| {
                x.add(VNode::new(Node { width: Val::Px(14.0), height: Val::Px(4.0), ..default() })).bg(role_color(r));
                x.text(r.map_or("Free", role_kind_name), 12.0, INK);
            })
            .style(|n| n.align_items = AlignItems::Center);
        }
    });
    let mut v = ui.nodes.pop()?;
    panel(&mut v);
    Some(v)
}
