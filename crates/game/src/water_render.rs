//! Nearly clear water: a chunked surface mesh over `SimState::water.depth` whose vertex colour
//! carries a light tint and a depth-based alpha, so riverbeds, beavers and lodges show through.
//! A scrolling procedural normal map gives a gentle animated surface; in winter the shallows freeze
//! into pale, mostly opaque ice that spreads with the season.
//!
//! Debug env: `BEAVER_DEMO=1` fills a lake into the sim's water field and drops a test object on
//! its bed (`BEAVER_DEMO_CAM=close` also frames it with the camera). Used for screenshots until
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
        perceptual_roughness: 0.12,
        reflectance: 0.3,
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
            let deep = 1.0 - (-d * 0.35).exp();
            let mut c = mix_rgb(p.water_shallow, p.water_deep, deep);
            let mut a = if d > 0.0 { 0.22 + 0.36 * deep } else { 0.0 };
            // soft foam-ish pale rim in the shallowest water
            let rim = (1.0 - d / 0.35).clamp(0.0, 1.0) * if d > 0.0 { 1.0 } else { 0.0 };
            c = mix_rgb(c, [1.0, 1.0, 1.0], rim * 0.35);
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
            if [a, b, c, e].iter().any(|&k| verts[k].is_some_and(|(_, d)| d > 0.0)) && [a, b, c, e].iter().all(|&k| verts[k].is_some()) {
                let (a, b, c, e) = (a as u32, b as u32, c as u32, e as u32);
                idx.extend_from_slice(&[a, c, b, b, c, e]);
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
fn demo_world(mut commands: Commands, mut sim: ResMut<Sim>, mut meshes: ResMut<Assets<Mesh>>, mut materials: ResMut<Assets<StandardMaterial>>, mut orbit: ResMut<OrbitCamera>) {
    if std::env::var("BEAVER_DEMO").is_err() {
        return;
    }
    let s = &mut sim.0;
    let mut sorted: Vec<f32> = s.height.as_slice().to_vec();
    sorted.sort_by(|a, b| a.total_cmp(b));
    let level = sorted[sorted.len() * 22 / 100];
    let (w, d) = (s.height.width(), s.height.height());
    for y in 0..d {
        for x in 0..w {
            let h = s.height.as_slice()[y * w + x];
            if h < level {
                s.water.depth.as_mut_slice()[y * w + x] = level - h;
            }
        }
    }
    // test object: on the bed, 2..5 units deep, closest to the middle of the world
    let mut best: Option<(f32, usize, usize)> = None;
    for y in 4..d - 4 {
        for x in 4..w - 4 {
            let dep = s.water.depth.as_slice()[y * w + x];
            if (2.0..5.0).contains(&dep) {
                let dist = ((x as f32 - w as f32 / 2.0).powi(2) + (y as f32 - d as f32 / 2.0).powi(2)).sqrt();
                if best.is_none_or(|b| dist < b.0) {
                    best = Some((dist, x, y));
                }
            }
        }
    }
    let Some((_, x, y)) = best else { warn!("demo: no suitable lake"); return };
    let (wx, wz) = world_pos(s, x as i32, y as i32);
    let bed = s.height.as_slice()[y * w + x];
    let pos = Vec3::new(wx, bed + 0.9, wz);
    // beaver-ish: brown body with a flat tail
    let body = materials.add(StandardMaterial { base_color: Color::srgb(0.55, 0.33, 0.16), perceptual_roughness: 0.9, ..default() });
    commands
        .spawn((DemoObject, Transform::from_translation(pos), Visibility::default()))
        .with_children(|c| {
            c.spawn((Mesh3d(meshes.add(Capsule3d::new(0.9, 1.4))), MeshMaterial3d(body.clone()), Transform::from_rotation(Quat::from_rotation_z(1.5))));
            c.spawn((Mesh3d(meshes.add(Cuboid::new(1.6, 0.15, 0.9))), MeshMaterial3d(body.clone()), Transform::from_xyz(-1.9, -0.4, 0.0)));
            c.spawn((Mesh3d(meshes.add(Sphere::new(0.35))), MeshMaterial3d(materials.add(StandardMaterial { base_color: Color::srgb(1.0, 0.8, 0.2), ..default() })), Transform::from_xyz(1.5, 0.7, 0.0)));
        });
    if std::env::var("BEAVER_DEMO_CAM").as_deref() == Ok("close") {
        *orbit = OrbitCamera { target: pos, yaw: 0.6, pitch: 0.9, distance: 16.0 };
    }
}
