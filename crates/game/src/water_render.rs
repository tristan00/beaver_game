//! Nearly clear water: a chunked surface mesh over `SimState::water.depth` whose vertex colour
//! carries a light tint and a depth-based alpha, so riverbeds, beavers and lodges show through.
//! A scrolling procedural normal map gives a gentle animated surface; in winter the shallows freeze
//! into pale, mostly opaque ice that spreads with the season.
//!
//! Debug env: `BEAVER_DEMO=1` fills a lake into the sim's water field and drops a test object on
//! its bed (`BEAVER_DEMO_CAM=close` or `mid` also frames it with the camera). Used for screenshots until
//! the water/colony tasks provide real water and beavers.
use crate::camera::{MainCamera, OrbitCamera};
use crate::state::Sim;
use crate::terrain_render::*;
use bevy::asset::RenderAssetUsages;
use bevy::image::{ImageAddressMode, ImageSampler, ImageSamplerDescriptor};
use bevy::math::Affine2;
use bevy::mesh::Indices;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, PrimitiveTopology, TextureDimension, TextureFormat};
use sim::SimState;

pub struct WaterRenderPlugin;

/// Marks a water surface chunk.
#[derive(Component)]
pub struct WaterSurface;

/// Marks the demo underwater test object.
#[derive(Component)]
pub struct DemoObject;

impl Plugin for WaterRenderPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<WaterChunks>()
            .add_systems(Startup, (demo_world.before(spawn_terrain), setup_water))
            .add_systems(Update, (update_water_chunks, animate_water));
    }
}

#[derive(Default)]
struct WSlot {
    entity: Option<Entity>,
    mesh: Handle<Mesh>,
    hash: u64,
    step: usize,
}

#[derive(Resource, Default)]
struct WaterChunks {
    slots: Vec<WSlot>,
    material: Handle<StandardMaterial>,
}

/// Tileable procedural ripple normal map (sum of a few sines, tangent-space).
fn ripple_normal_map() -> Image {
    const N: usize = 128;
    let tau = std::f32::consts::TAU;
    let height = |x: f32, y: f32| -> (f32, f32) {
        // returns d(height)/dx, d(height)/dy for a few tileable waves
        let waves: [(f32, f32, f32, f32); 5] =
            [(1.0, 2.0, 0.0, 1.0), (3.0, 1.0, 1.7, 0.6), (2.0, -3.0, 4.1, 0.5), (5.0, 4.0, 2.3, 0.25), (-4.0, 6.0, 0.9, 0.2)];
        let (mut gx, mut gy) = (0.0, 0.0);
        for (fx, fy, ph, a) in waves {
            let c = (tau * (fx * x + fy * y) + ph).cos() * a * tau;
            gx += c * fx;
            gy += c * fy;
        }
        (gx, gy)
    };
    let mut data = Vec::with_capacity(N * N * 4);
    for j in 0..N {
        for i in 0..N {
            let (gx, gy) = height(i as f32 / N as f32, j as f32 / N as f32);
            let nrm = Vec3::new(-gx * 0.012, -gy * 0.012, 1.0).normalize();
            for v in [nrm.x, nrm.y, nrm.z] {
                data.push(((v * 0.5 + 0.5) * 255.0) as u8);
            }
            data.push(255);
        }
    }
    let mut img = Image::new(
        Extent3d { width: N as u32, height: N as u32, depth_or_array_layers: 1 },
        TextureDimension::D2,
        data,
        TextureFormat::Rgba8Unorm,
        RenderAssetUsages::RENDER_WORLD,
    );
    img.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
        address_mode_u: ImageAddressMode::Repeat,
        address_mode_v: ImageAddressMode::Repeat,
        ..ImageSamplerDescriptor::linear()
    });
    img
}

fn setup_water(
    sim: Res<Sim>,
    mut water: ResMut<WaterChunks>,
    mut images: ResMut<Assets<Image>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    water.material = materials.add(StandardMaterial {
        base_color: Color::WHITE, // colour and alpha come from vertices
        alpha_mode: AlphaMode::Blend,
        perceptual_roughness: 0.22,
        reflectance: 0.12,
        normal_map_texture: Some(images.add(ripple_normal_map())),
        cull_mode: None,
        ..default()
    });
    let (cols, rows) = chunk_dims(&sim.0);
    water.slots = (0..cols * rows).map(|_| WSlot::default()).collect();
}

/// Builds the water mesh for one chunk; `None` if it has no water at all.
pub fn build_water_mesh(s: &SimState, p: &Palette, cx: usize, cy: usize, step: usize) -> Option<Mesh> {
    let n = CHUNK / step;
    let st = step as i32;
    let (x0, y0) = ((cx * CHUNK) as i32, (cy * CHUNK) as i32);
    let depth = |x: i32, y: i32| at(&s.water.depth, x, y);
    // Surface level at a vertex: its own if wet, else the mean of wet neighbours (so the sheet
    // runs flat into the bank and is clipped by the terrain), else None.
    let level = |x: i32, y: i32| -> Option<(f32, f32)> {
        let d = depth(x, y);
        if d > WET_EPS {
            return Some((at(&s.height, x, y) + d, d));
        }
        let (mut sum, mut cnt) = (0.0, 0);
        for (dx, dy) in [(st, 0), (-st, 0), (0, st), (0, -st)] {
            let dd = depth(x + dx, y + dy);
            if dd > WET_EPS {
                sum += at(&s.height, x + dx, y + dy) + dd;
                cnt += 1;
            }
        }
        (cnt > 0).then(|| (sum / cnt as f32, 0.0))
    };
    let side = n + 1;
    let mut any = false;
    let mut verts: Vec<Option<(f32, f32)>> = Vec::with_capacity(side * side);
    for j in 0..=n {
        for i in 0..=n {
            let v = level(x0 + (i * step) as i32, y0 + (j * step) as i32);
            any |= v.is_some_and(|(_, d)| d > 0.0);
            verts.push(v);
        }
    }
    if !any {
        return None;
    }
    let (mut pos, mut nor, mut col, mut uv, mut tan) = (vec![], vec![], vec![], vec![], vec![]);
    for j in 0..=n {
        for i in 0..=n {
            let (x, y) = (x0 + (i * step) as i32, y0 + (j * step) as i32);
            let (wx, wz) = world_pos(s, x, y);
            let (lvl, d) = verts[j * side + i].unwrap_or((at(&s.height, x, y) - 1.0, 0.0));
            pos.push([wx, lvl, wz]);
            nor.push([0.0, 1.0, 0.0]);
            tan.push([1.0, 0.0, 0.0, -1.0]);
            uv.push([wx * 0.045, wz * 0.045]);
            // depth-based tint: clear and pale in shallows, bluer when deep, never opaque
            let deep = 1.0 - (-d * 0.45).exp();
            let mut c = mix_rgb(p.water_shallow, p.water_deep, deep);
            let mut a = if d > 0.0 { 0.30 + 0.14 * deep } else { 0.0 };
            // soft foam-ish pale rim in the shallowest water
            let rim = (1.0 - d / 0.12).clamp(0.0, 1.0) * if d > 0.0 { 1.0 } else { 0.0 };
            c = mix_rgb(c, [1.0, 1.0, 1.0], rim * 0.25);
            // ice: freezes from the shore inwards as winter deepens
            let ice = if d > 0.0 { ((p.winter * 4.5 - d) / 1.2).clamp(0.0, 1.0) } else { p.winter };
            c = mix_rgb(c, p.ice, ice);
            a = a + (0.86 - a) * ice * if d > 0.0 { 1.0 } else { 0.0 };
            col.push(lin(c, a));
        }
    }
    let mut idx = Vec::new();
    for j in 0..n {
        for i in 0..n {
            let (a, b, c, e) = (j * side + i, j * side + i + 1, (j + 1) * side + i, (j + 1) * side + i + 1);
            // per triangle, so the shoreline follows diagonals instead of stair-stepping
            for tri in [[a, c, b], [b, c, e]] {
                if tri.iter().any(|&k| verts[k].is_some_and(|(_, d)| d > 0.0)) && tri.iter().all(|&k| verts[k].is_some()) {
                    idx.extend(tri.iter().map(|&k| k as u32));
                }
            }
        }
    }
    if idx.is_empty() {
        return None;
    }
    Some(
        Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
            .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, pos)
            .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, nor)
            .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uv)
            .with_inserted_attribute(Mesh::ATTRIBUTE_TANGENT, tan)
            .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, col)
            .with_inserted_indices(Indices::U32(idx)),
    )
}

fn mix_rgb(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    let t = t.clamp(0.0, 1.0);
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t]
}

fn update_water_chunks(
    mut commands: Commands,
    sim: Res<Sim>,
    season: Res<Season>,
    mut water: ResMut<WaterChunks>,
    mut meshes: ResMut<Assets<Mesh>>,
    cam: Query<&Transform, With<MainCamera>>,
    mut first: Local<bool>,
) {
    let Some(campos) = cam.iter().next().map(|t| t.translation) else { return };
    let (cols, rows) = chunk_dims(&sim.0);
    if water.slots.len() != cols * rows {
        return;
    }
    let mut budget = if *first { 10 } else { usize::MAX };
    *first = true;
    let palette = Palette::at(season.phase);
    // Ice only depends on the season while it is frozen-ish; recolour in coarse steps.
    let sea_bucket = season.bucket() as u64;
    for cy in 0..rows {
        for cx in 0..cols {
            let idx = cy * cols + cx;
            let step = lod_step(campos, chunk_centre(&sim.0, cx, cy));
            let hash = region_hash(&sim.0, cx, cy, 1, sea_bucket, false);
            let slot = &mut water.slots[idx];
            if slot.hash == hash.max(1) && slot.step == step {
                continue;
            }
            if budget == 0 {
                return;
            }
            budget -= 1;
            MESH_REBUILDS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let mesh = build_water_mesh(&sim.0, &palette, cx, cy, step);
            let material = water.material.clone();
            let slot = &mut water.slots[idx];
            slot.hash = hash.max(1);
            slot.step = step;
            match (mesh, slot.entity) {
                (Some(m), Some(_)) => {
                    let _ = meshes.insert(&slot.mesh, m);
                }
                (Some(m), None) => {
                    slot.mesh = meshes.add(m);
                    slot.entity = Some(commands.spawn((WaterSurface, Mesh3d(slot.mesh.clone()), MeshMaterial3d(material), Transform::default())).id());
                }
                (None, Some(e)) => {
                    commands.entity(e).despawn();
                    slot.entity = None;
                }
                (None, None) => {}
            }
        }
    }
}

/// Slowly drifts and swirls the ripple normal map.
fn animate_water(time: Res<Time>, water: Res<WaterChunks>, mut materials: ResMut<Assets<StandardMaterial>>, season: Res<Season>) {
    let t = time.elapsed_secs();
    let calm = 1.0 - 0.9 * Palette::at(season.phase).winter; // frozen surfaces barely move
    if let Some(mut m) = materials.get_mut(&water.material) {
        m.uv_transform = Affine2::from_scale_angle_translation(
            Vec2::splat(1.0),
            0.12 * (t * 0.15).sin(),
            Vec2::new(t * 0.012 * calm, t * 0.008 * calm + 0.02 * (t * 0.3).sin()),
        );
    }
}

/// Demo scene (`BEAVER_DEMO=1`): a lake in the sim's water field and a test object on its bed.
///
/// Floods everything below the 22nd-percentile height, then always carves a round basin
/// (radius `R`, `DEMO_DEPTH` deep at its centre) at the lowest spot near the middle of the map
/// and fills it to its spill height, so every heightmap (flat floodplains included) gets
/// clearly deep water with the test object resting on its bed.
fn demo_world(mut commands: Commands, mut sim: ResMut<Sim>, mut meshes: ResMut<Assets<Mesh>>, mut materials: ResMut<Assets<StandardMaterial>>, mut orbit: ResMut<OrbitCamera>) {
    if std::env::var("BEAVER_DEMO").is_err() {
        return;
    }
    let s = &mut sim.0;
    let Some((x, y, bed)) = carve_demo_lake(s) else { warn!("demo: world too small for a lake"); return };
    let (wx, wz) = world_pos(s, x as i32, y as i32);
    const SCALE: f32 = 1.6;
    let pos = Vec3::new(wx, bed + 0.9 * SCALE, wz);
    // beaver-ish: warm brown body with a flat tail and a yellow marker
    let body = materials.add(StandardMaterial { base_color: Color::srgb(0.60, 0.33, 0.12), perceptual_roughness: 0.9, ..default() });
    commands
        .spawn((DemoObject, Transform::from_translation(pos).with_scale(Vec3::splat(SCALE)), Visibility::default()))
        .with_children(|c| {
            c.spawn((Mesh3d(meshes.add(Capsule3d::new(0.9, 1.4))), MeshMaterial3d(body.clone()), Transform::from_rotation(Quat::from_rotation_z(1.5))));
            c.spawn((Mesh3d(meshes.add(Cuboid::new(1.6, 0.15, 0.9))), MeshMaterial3d(body.clone()), Transform::from_xyz(-1.9, -0.4, 0.0)));
            c.spawn((Mesh3d(meshes.add(Sphere::new(0.35))), MeshMaterial3d(materials.add(StandardMaterial { base_color: Color::srgb(1.0, 0.8, 0.2), ..default() })), Transform::from_xyz(1.5, 0.7, 0.0)));
        });
    match std::env::var("BEAVER_DEMO_CAM").as_deref() {
        Ok("close") => *orbit = OrbitCamera { target: pos, yaw: 0.6, pitch: 0.9, distance: 18.0 },
        Ok("mid") => *orbit = OrbitCamera { target: pos, yaw: orbit.yaw, pitch: orbit.pitch, distance: 45.0 },
        _ => {}
    }
}

/// Depth of the carved demo basin at its centre.
pub const DEMO_DEPTH: f32 = 4.5;

/// Floods the low ground and carves the guaranteed demo basin; returns its centre cell and bed height.
pub fn carve_demo_lake(s: &mut SimState) -> Option<(usize, usize, f32)> {
    const R: f32 = 11.0;
    let (w, d) = (s.height.width(), s.height.height());
    let margin = R as usize + 3;
    if w <= 2 * margin || d <= 2 * margin {
        return None;
    }
    let mut sorted: Vec<f32> = s.height.as_slice().to_vec();
    sorted.sort_by(|a, b| a.total_cmp(b));
    let level = sorted[sorted.len() * 22 / 100];
    // site: lowest (3x3-averaged) ground in the central half, mildly preferring the middle
    let h = |x: usize, y: usize| s.height.as_slice()[y * w + x];
    // The whole-world camera must see the basin: march from the site towards the "iso" preset
    // camera and reject sites whose sightline dips under the terrain.
    let eye = crate::camera::preset("iso").map(|o| o.eye()).unwrap_or(Vec3::new(100.0, 100.0, 100.0));
    let visible = |x: usize, y: usize| -> bool {
        let (wx, wz) = world_pos(s, x as i32, y as i32);
        let from = Vec3::new(wx, h(x, y) + 0.5, wz);
        let dir = eye - from;
        let steps = (dir.length() / 0.5) as usize;
        (1..steps).all(|k| {
            let p = from + dir * (k as f32 / steps as f32);
            let (gx, gy) = (p.x / CELL_SIZE + w as f32 / 2.0, p.z / CELL_SIZE + d as f32 / 2.0);
            if gx < 0.0 || gy < 0.0 || gx >= (w - 1) as f32 || gy >= (d - 1) as f32 {
                return true;
            }
            p.y > h(gx.round() as usize, gy.round() as usize)
        })
    };
    let mut best = (f32::MAX, w / 2, d / 2);
    for y in (d / 4).max(margin)..(3 * d / 4).min(d - margin) {
        for x in (w / 4).max(margin)..(3 * w / 4).min(w - margin) {
            let mut avg = 0.0;
            for (dx, dy) in [(0, 0), (1, 0), (0, 1), (usize::MAX, 0), (0, usize::MAX)] {
                avg += h(x.wrapping_add(dx), y.wrapping_add(dy)) / 5.0;
            }
            let dist = ((x as f32 - w as f32 / 2.0).powi(2) + (y as f32 - d as f32 / 2.0).powi(2)).sqrt();
            let score = avg + dist * 0.08;
            if score < best.0 && visible(x, y) {
                best = (score, x, y);
            }
        }
    }
    let (_, sx, sy) = best;
    // spill height: lowest terrain on the ring just outside the basin, or the flood level if higher
    let ring = |x: usize, y: usize| ((x as f32 - sx as f32).powi(2) + (y as f32 - sy as f32).powi(2)).sqrt();
    let mut spill = f32::MAX;
    let r = R as usize + 2;
    for y in sy - r..=sy + r {
        for x in sx - r..=sx + r {
            if (R..R + 1.5).contains(&ring(x, y)) {
                spill = spill.min(h(x, y));
            }
        }
    }
    let lake = spill.max(level);
    let hs = s.height.as_mut_slice();
    for y in sy - r..=sy + r {
        for x in sx - r..=sx + r {
            let t = ring(x, y) / R;
            if t < 1.0 {
                let bowl = lake - DEMO_DEPTH * (1.0 - t * t);
                hs[y * w + x] = hs[y * w + x].min(bowl);
            }
        }
    }
    let wd = s.water.depth.as_mut_slice();
    for (i, &hh) in s.height.as_slice().iter().enumerate() {
        let (x, y) = (i % w, i / w);
        let target = if ring(x, y) < R { lake } else { level };
        if hh < target {
            wd[i] = target - hh;
        }
    }
    Some((sx, sy, s.height.as_slice()[sy * w + sx]))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn check(s: &mut SimState) {
        let (x, y, bed) = carve_demo_lake(s).expect("lake");
        let w = s.height.width();
        let dep = s.water.depth.as_slice()[y * w + x];
        assert!((DEMO_DEPTH - 0.01..=DEMO_DEPTH + 3.0).contains(&dep), "centre depth {dep}");
        assert_eq!(bed, s.height.as_slice()[y * w + x]);
        // the basin is filled all the way to its rim: neighbours of the centre are wet too
        assert!(s.water.depth.as_slice()[y * w + x + 5] > 1.0);
    }

    #[test]
    fn demo_lake_on_flat_world() {
        let mut s = SimState::new(1);
        s.height.as_mut_slice().fill(5.0);
        s.water.depth.as_mut_slice().fill(0.0);
        check(&mut s);
    }

    #[test]
    fn demo_lake_on_generated_worlds() {
        for seed in 1..=6 {
            check(&mut SimState::new(seed));
        }
    }
}
