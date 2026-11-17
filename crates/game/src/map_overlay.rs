//! Paints the active map mode on the map: one translucent mesh of tinted cells, plus gizmos for
//! flow arrows, marks and outlines, and the handles of the open work-area editor. What to paint
//! comes from `ui::mapmodes::overlay`, which reads the sim; this file only draws it.
use crate::cli::Cli;
use crate::flow::{cell_world, height_at};
use crate::state::Sim;
use crate::terrain_render::CELL_SIZE;
use crate::ui::mapmodes::{overlay, MapMode, MarkKind, Overlay};
use crate::ui::ShellUi;
use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;
use sim::water::Cell;
use sim::work::Area;

/// The overlay of the current mode, recomputed from the sim a few times a second.
#[derive(Resource, Default)]
pub struct ActiveOverlay(pub Option<Overlay>);

#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct ComputeSet;

#[derive(Default, Reflect, GizmoConfigGroup)]
struct ModeGizmos;

#[derive(Component)]
struct TintMesh;

pub struct MapOverlayPlugin;

impl Plugin for MapOverlayPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ActiveOverlay>()
            .init_gizmo_group::<ModeGizmos>()
            .add_systems(Startup, (spawn_mesh, configure))
            .add_systems(Update, (compute.in_set(ComputeSet), (paint_mesh, draw_marks).after(ComputeSet)));
    }
}

fn configure(mut store: ResMut<GizmoConfigStore>) {
    let (config, _) = store.config_mut::<ModeGizmos>();
    config.line.width = 3.0;
    config.depth_bias = -1.0;
}

fn compute(time: Res<Time<Real>>, sim: Res<Sim>, ui: Res<ShellUi>, cli: Res<Cli>, mut ov: ResMut<ActiveOverlay>, mut since: Local<f32>) {
    *since += time.delta_secs();
    let mode = ui.shell.mode;
    let stale = ov.0.as_ref().map(|o| o.mode) != Some(mode);
    // screenshot runs are deterministic, so they compute once per frame like anything else
    let due = *since >= 0.3 || cli.screenshot.is_some();
    if !sim.0.colony.is_founded() || mode == MapMode::Normal {
        if ov.0.is_some() {
            ov.0 = None;
        }
        return;
    }
    if stale || due {
        *since = 0.0;
        let fresh = overlay(&sim.0, mode);
        if ov.0.as_ref() != Some(&fresh) {
            ov.0 = Some(fresh);
        }
    }
}

fn spawn_mesh(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>, mut mats: ResMut<Assets<StandardMaterial>>) {
    let mesh = meshes.add(Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default()));
    let mat = mats.add(StandardMaterial { base_color: Color::WHITE, unlit: true, alpha_mode: AlphaMode::Blend, cull_mode: None, ..default() });
    commands.spawn((TintMesh, Mesh3d(mesh), MeshMaterial3d(mat), Transform::default(), Visibility::Hidden));
}

/// Height the tint floats at: above the ground, and above the water standing on it.
fn lift(sim: &sim::SimState, x: f32, y: f32) -> f32 {
    let d = sim.water.depth.get(x.round().max(0.0) as usize, y.round().max(0.0) as usize).copied().unwrap_or(0.0);
    height_at(&sim.height, x, y) + d + 0.07
}

/// One flat translucent quad per tinted cell, following the terrain at its corners.
pub fn tint_mesh(sim: &sim::SimState, o: &Overlay) -> Mesh {
    let (w, d) = (sim.height.width() as f32, sim.height.height() as f32);
    let (ox, oz) = (w * CELL_SIZE / 2.0, d * CELL_SIZE / 2.0);
    let (mut pos, mut col, mut idx) = (Vec::new(), Vec::new(), Vec::new());
    for t in &o.tints {
        let (cx, cy) = (t.cell.0 as f32, t.cell.1 as f32);
        let i = pos.len() as u32;
        // the cell's surface height is shared by its four corners so water stays flat
        let y = lift(sim, cx, cy);
        for (dx, dy) in [(-0.5, -0.5), (-0.5, 0.5), (0.5, 0.5), (0.5, -0.5)] {
            pos.push([(cx + dx) * CELL_SIZE - ox, y.max(height_at(&sim.height, cx + dx, cy + dy) + 0.04) + t.up, (cy + dy) * CELL_SIZE - oz]);
            col.push(t.rgba);
        }
        idx.extend([i, i + 1, i + 2, i, i + 2, i + 3]);
    }
    let n = pos.len();
    Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, pos)
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0f32, 1.0, 0.0]; n])
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, vec![[0.0f32, 0.0]; n])
        .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, col)
        .with_inserted_indices(Indices::U32(idx))
}

fn paint_mesh(ov: Res<ActiveOverlay>, sim: Res<Sim>, mut meshes: ResMut<Assets<Mesh>>, q: Query<(&Mesh3d, Entity), With<TintMesh>>, mut vis: Query<&mut Visibility, With<TintMesh>>) {
    if !ov.is_changed() {
        return;
    }
    let Ok((handle, _)) = q.single() else { return };
    match &ov.0 {
        Some(o) if !o.tints.is_empty() => {
            if let Some(mut m) = meshes.get_mut(&handle.0) {
                *m = tint_mesh(&sim.0, o);
            }
            for mut v in &mut vis {
                *v = Visibility::Inherited;
            }
        }
        _ => {
            for mut v in &mut vis {
                *v = Visibility::Hidden;
            }
        }
    }
}

fn rgba([r, g, b, a]: [f32; 4]) -> Color {
    Color::srgba(r, g, b, a)
}

/// Outline of `area` following the terrain, `up` above it.
fn outline(g: &mut Gizmos<ModeGizmos>, sim: &sim::SimState, a: Area, up: f32, col: Color) {
    let at = |x: f32, y: f32| cell_world(&sim.height, (x, y)) + Vec3::Y * up;
    let (x0, x1, y0, y1) = (a.min.0 as f32 - 0.5, a.max.0 as f32 + 0.5, a.min.1 as f32 - 0.5, a.max.1 as f32 + 0.5);
    let mut edge = |f: (f32, f32), t: (f32, f32)| {
        let n = ((t.0 - f.0).abs() + (t.1 - f.1).abs()).ceil().max(1.0) as usize;
        let p = |i: usize| {
            let s = i as f32 / n as f32;
            at(f.0 + (t.0 - f.0) * s, f.1 + (t.1 - f.1) * s)
        };
        for i in 0..n {
            g.line(p(i), p(i + 1), col);
        }
    };
    edge((x0, y0), (x1, y0));
    edge((x1, y0), (x1, y1));
    edge((x1, y1), (x0, y1));
    edge((x0, y1), (x0, y0));
}

fn draw_marks(mut g: Gizmos<ModeGizmos>, ov: Res<ActiveOverlay>, sim: Res<Sim>, ui: Res<ShellUi>) {
    let s = &sim.0;
    let at = |c: Cell, up: f32| cell_world(&s.height, (c.0 as f32, c.1 as f32)) + Vec3::Y * (up + s.water.depth.get(c.0, c.1).copied().unwrap_or(0.0));
    if let Some(o) = &ov.0 {
        for a in &o.arrows {
            let from = at(a.cell, 0.3);
            let dir = Vec3::new(a.dir.0, 0.0, a.dir.1);
            let len = 0.9 + 1.4 * a.strength;
            let to = from + dir * len;
            let col = Color::srgba(1.0, 1.0, 1.0, 0.55 + 0.45 * a.strength);
            g.line(from - dir * 0.2, to, col);
            let side = Vec3::new(-dir.z, 0.0, dir.x) * 0.3;
            g.line(to, to - dir * 0.45 + side, col);
            g.line(to, to - dir * 0.45 - side, col);
        }
        for m in &o.marks {
            let (col, up, r) = match m.kind {
                MarkKind::Dam => (Color::srgb(0.55, 0.35, 0.12), 1.0, 0.28),
                MarkKind::Canal => (Color::srgb(0.75, 0.55, 0.25), 0.3, 0.2),
                MarkKind::Felling => (Color::srgb(1.0, 0.85, 0.2), 2.4, 0.3),
                MarkKind::Felled => (Color::srgb(0.95, 0.5, 0.1), 0.6, 0.3),
                MarkKind::Bark => (Color::srgb(0.6, 0.38, 0.2), 0.9, 0.3),
                MarkKind::Worker => (Color::srgb(1.0, 1.0, 1.0), 1.6, 0.35),
                MarkKind::OpenSlot => (Color::srgb(1.0, 0.4, 0.3), 2.0, 0.4),
            };
            g.sphere(Isometry3d::from_translation(at(m.cell, up)), r, col);
        }
        for ol in &o.outlines {
            outline(&mut g, s, ol.area, 0.4, rgba(ol.rgba));
            outline(&mut g, s, ol.area, 0.55, rgba(ol.rgba));
        }
    }
    // the work-area editor: the area being edited, with a handle on every corner and edge middle
    if let Some(a) = ui.shell.editor.and_then(|b| s.work.place(b)).and_then(|p| p.area) {
        let gold = Color::srgb(1.0, 0.92, 0.35);
        outline(&mut g, s, a, 0.7, gold);
        outline(&mut g, s, a, 0.9, gold);
        let (cx, cy) = a.centre();
        let spots = [
            (a.min.0, a.min.1), (a.max.0, a.min.1), (a.min.0, a.max.1), (a.max.0, a.max.1),
            (cx, a.min.1), (cx, a.max.1), (a.min.0, cy), (a.max.0, cy),
        ];
        for c in spots {
            g.sphere(Isometry3d::from_translation(at(c, 0.9)), 0.35, gold);
        }
    }
}
