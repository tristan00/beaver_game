//! Renders `SimState::height` as a vertex-coloured triangle mesh centred on the origin.
use crate::state::Sim;
use bevy::asset::RenderAssetUsages;
use bevy::mesh::Indices;
use bevy::prelude::*;
use bevy::render::render_resource::PrimitiveTopology;
use sim::grid::HeightGrid;

/// World units between neighbouring grid cells.
pub const CELL_SIZE: f32 = 1.0;

pub struct TerrainRenderPlugin;

impl Plugin for TerrainRenderPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_terrain);
    }
}

fn spawn_terrain(
    mut commands: Commands,
    sim: Res<Sim>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let mesh = build_mesh(&sim.0.height);
    commands.spawn((
        Mesh3d(meshes.add(mesh)),
        MeshMaterial3d(materials.add(StandardMaterial { perceptual_roughness: 1.0, ..default() })),
    ));
}

fn height_color(t: f32) -> [f32; 4] {
    // low = sand, mid = grass, high = rock
    let stops = [(0.0, [0.76, 0.70, 0.50]), (0.35, [0.30, 0.55, 0.22]), (0.7, [0.22, 0.42, 0.18]), (1.0, [0.60, 0.58, 0.55])];
    let t = t.clamp(0.0, 1.0);
    for w in stops.windows(2) {
        let ((t0, c0), (t1, c1)) = (w[0], w[1]);
        if t <= t1 {
            let f = (t - t0) / (t1 - t0);
            return [c0[0] + (c1[0] - c0[0]) * f, c0[1] + (c1[1] - c0[1]) * f, c0[2] + (c1[2] - c0[2]) * f, 1.0];
        }
    }
    [0.6, 0.58, 0.55, 1.0]
}


pub fn build_mesh(h: &HeightGrid) -> Mesh {
    let max_height = h.as_slice().iter().cloned().fold(f32::EPSILON, f32::max);
    let (w, d) = (h.width(), h.height());
    let at = |x: usize, y: usize| *h.get(x.min(w - 1), y.min(d - 1)).expect("clamped in bounds");
    let (ox, oz) = (w as f32 * CELL_SIZE / 2.0, d as f32 * CELL_SIZE / 2.0);
    let (mut pos, mut nor, mut col, mut uv) = (vec![], vec![], vec![], vec![]);
    for y in 0..d {
        for x in 0..w {
            let ht = at(x, y);
            pos.push([x as f32 * CELL_SIZE - ox, ht, y as f32 * CELL_SIZE - oz]);
            let dx = at(x + 1, y) - at(x.saturating_sub(1), y);
            let dz = at(x, y + 1) - at(x, y.saturating_sub(1));
            nor.push(Vec3::new(-dx, 2.0 * CELL_SIZE, -dz).normalize().to_array());
            col.push(height_color(ht / max_height));
            uv.push([x as f32 / w as f32, y as f32 / d as f32]);
        }
    }
    let mut idx = Vec::with_capacity((w - 1) * (d - 1) * 6);
    for y in 0..d - 1 {
        for x in 0..w - 1 {
            let i = (y * w + x) as u32;
            let (a, b, c, e) = (i, i + 1, i + w as u32, i + w as u32 + 1);
            idx.extend_from_slice(&[a, c, b, b, c, e]);
        }
    }
    Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, pos)
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, nor)
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uv)
        .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, col)
        .with_inserted_indices(Indices::U32(idx))
}

