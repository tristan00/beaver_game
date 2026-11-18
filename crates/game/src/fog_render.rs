//! Fog of war, drawn: a soft cloud sheet floating above the canopy over everything the colony
//! has not seen (`Flow::fog`). Exploring is the colony's own work: only the ground around its
//! beavers (and the start the family overlooks) clears; the camera reveals nothing by itself.
use crate::flow::{Flow, EXPLORED};
use crate::state::Sim;
use crate::terrain_render::CELL_SIZE;
use bevy::asset::RenderAssetUsages;
use bevy::mesh::Indices;
use bevy::render::render_resource::PrimitiveTopology;
use bevy::prelude::*;

/// Height of the cloud sheet above the highest ground nearby: above the treetops.
const LIFT: f32 = 2.8;
/// Opacity over unexplored ground (a hint of the land shows through).
const DENSITY: f32 = 0.9;
/// Cloud colour (sRGB): cool, pale, unlike any terrain or water colour.
const CLOUD: [f32; 3] = [0.78, 0.8, 0.84];
/// Seconds between rebuilds while the fog is changing.
const REBUILD_SECS: f32 = 0.25;

#[derive(Component)]
struct FogSheet;

#[derive(Resource, Default)]
struct FogState {
    /// The world generation the sheet was built for.
    generation: Option<u32>,
    next: f32,
}

pub struct FogRenderPlugin;

impl Plugin for FogRenderPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<FogState>().add_systems(Update, update_fog);
    }
}

fn update_fog(
    mut commands: Commands,
    time: Res<Time>,
    sim: Res<Sim>,
    mut flow: ResMut<Flow>,
    mut state: ResMut<FogState>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    sheets: Query<(Entity, &Mesh3d), With<FogSheet>>,
) {
    let now = time.elapsed_secs();
    let fresh = state.generation != Some(flow.generation);
    if !fresh && (!flow.fog.dirty || now < state.next) {
        return;
    }
    state.next = now + REBUILD_SECS;
    flow.fog.dirty = false;
    let mesh = fog_mesh(&sim.0, &flow.fog);
    if fresh {
        for (e, _) in &sheets {
            commands.entity(e).despawn();
        }
        let material = materials.add(StandardMaterial { base_color: Color::WHITE, unlit: true, alpha_mode: AlphaMode::Blend, double_sided: true, cull_mode: None, ..default() });
        commands.spawn((Mesh3d(meshes.add(mesh)), MeshMaterial3d(material), Transform::IDENTITY, FogSheet, bevy::light::NotShadowCaster));
        state.generation = Some(flow.generation);
    } else if let Some((_, m)) = sheets.iter().next() {
        if let Some(mut old) = meshes.get_mut(&m.0) {
            *old = mesh;
        }
    }
}

/// The sheet: one vertex per cell centre, opacity from how unexplored the cells around it are.
fn fog_mesh(s: &sim::SimState, fog: &crate::flow::Fog) -> Mesh {
    let h = &s.height;
    let n = fog.size;
    let (ox, oz) = (h.width() as f32 * CELL_SIZE / 2.0, h.height() as f32 * CELL_SIZE / 2.0);
    let (mut pos, mut nor, mut col) = (Vec::with_capacity(n * n), Vec::with_capacity(n * n), Vec::with_capacity(n * n));
    let lin = |c: f32| if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) };
    for y in 0..n {
        for x in 0..n {
            // Highest ground and mean fog in the 3x3 around: the sheet clears softly and rides
            // above ridges.
            let (mut top, mut unseen, mut k) = (f32::MIN, 0.0, 0.0);
            for dy in -1i32..=1 {
                for dx in -1i32..=1 {
                    let (cx, cy) = ((x as i32 + dx).clamp(0, n as i32 - 1) as usize, (y as i32 + dy).clamp(0, n as i32 - 1) as usize);
                    top = top.max(h.get(cx, cy).copied().unwrap_or(0.0));
                    unseen += 1.0 - (fog.get((cx, cy)) / EXPLORED).min(1.0);
                    k += 1.0;
                }
            }
            let a = (unseen / k).clamp(0.0, 1.0) * DENSITY;
            pos.push([(x as f32 + 0.5) * CELL_SIZE - ox, top + LIFT, (y as f32 + 0.5) * CELL_SIZE - oz]);
            nor.push([0.0, 1.0, 0.0]);
            col.push([lin(CLOUD[0]), lin(CLOUD[1]), lin(CLOUD[2]), a]);
        }
    }
    let mut idx = Vec::with_capacity((n - 1) * (n - 1) * 6);
    for y in 0..n - 1 {
        for x in 0..n - 1 {
            let i = (y * n + x) as u32;
            let (a, b, c, e) = (i, i + 1, i + n as u32, i + n as u32 + 1);
            idx.extend_from_slice(&[a, c, b, b, c, e]);
        }
    }
    Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, pos)
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, nor)
        .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, col)
        .with_inserted_indices(Indices::U32(idx))
}
