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

#[derive(Component)]
pub struct WaterSurface;

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

fn ripple_normal_map() -> Image {
    const N: usize = 128;
    let tau = std::f32::consts::TAU;
    let height = |x: f32, y: f32| -> (f32, f32) {
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
        base_color: Color::WHITE,
        alpha_mode: AlphaMode::Blend,
        perceptual_roughness: 0.35,
        reflectance: 0.06,
        normal_map_texture: Some(images.add(ripple_normal_map())),
        cull_mode: None,
        ..default()
    });
    let (cols, rows) = chunk_dims(&sim.0);
    water.slots = (0..cols * rows).map(|_| WSlot::default()).collect();
}

const WATER_REBUILD_BUDGET_S: f32 = 0.002;
pub const WATER_PAD: i32 = SHORE_SEARCH + 1;
const SHORE_SEARCH: i32 = 3;
const WATER_GAIN: f32 = crate::entities_render::ENTITY_GAIN;

#[derive(Clone, Copy)]
struct WV {
    p: [f32; 3],
    f: f32,
    d: f32,
    s: f32,
}

fn lerp_wv(a: &WV, b: &WV, t: f32) -> WV {
    let l = |x: f32, y: f32| x + (y - x) * t;
    WV { p: [l(a.p[0], b.p[0]), l(a.p[1], b.p[1]), l(a.p[2], b.p[2])], f: l(a.f, b.f), d: l(a.d, b.d), s: l(a.s, b.s) }
}

fn clip_poly(poly: &[WV], level: f32, above: bool) -> Vec<WV> {
    let inside = |v: &WV| if above { v.f >= level } else { v.f <= level };
    let mut out = Vec::with_capacity(poly.len() + 2);
    for k in 0..poly.len() {
        let (a, b) = (&poly[k], &poly[(k + 1) % poly.len()]);
        let (ia, ib) = (inside(a), inside(b));
        if ia {
            out.push(*a);
        }
        if ia != ib {
            out.push(lerp_wv(a, b, (level - a.f) / (b.f - a.f)));
        }
    }
    out
}

fn terrain_shown(s: &SimState, x: f32, y: f32, step: usize) -> f32 {
    let st = step as f32;
    let (gx, gy) = ((x / st).floor(), (y / st).floor());
    let (u, v) = (x / st - gx, y / st - gy);
    let (ix, iy) = (gx as i32 * step as i32, gy as i32 * step as i32);
    let s1 = step as i32;
    let h = |dx: i32, dy: i32| at(&s.height, ix + dx * s1, iy + dy * s1);
    if u + v <= 1.0 {
        let (ha, hb, hc) = (h(0, 0), h(1, 0), h(0, 1));
        ha + u * (hb - ha) + v * (hc - ha)
    } else {
        let (hb, hc, he) = (h(1, 0), h(0, 1), h(1, 1));
        he + (1.0 - u) * (hc - he) + (1.0 - v) * (hb - he)
    }
}

const FULL_WATER_M: f32 = 0.02;
const SURFACE_LIFT: f32 = 0.01;
const WHITE_WATER_FROUDE: f32 = 0.9;

fn surface_sample(s: &SimState, x: i32, y: i32, step: i32) -> (f32, f32, f32, f32) {
    let n = s.height.width() as i32;
    let ground = |x: i32, y: i32| at(&s.height, x, y) + at(&s.water.dam_height, x, y);
    let speed = |x: i32, y: i32| s.water.velocity.get(x.clamp(0, n - 1) as usize, y.clamp(0, n - 1) as usize).map_or(0.0, |v| (v.0 * v.0 + v.1 * v.1).sqrt());
    let (mut field, mut level, mut depth, mut fast, mut wet) = (-0.5f32, 0.0f32, 0.0f32, 0.0f32, 0.0f32);
    for by in 0..step {
        for bx in 0..step {
            let (cx, cy) = (x + bx, y + by);
            let h = at(&s.water.depth, cx, cy);
            field = field.max((h / FULL_WATER_M).min(1.0) - 0.5);
            if h > 0.0 {
                level += ground(cx, cy) + h;
                depth += h;
                fast += speed(cx, cy);
                wet += 1.0;
            }
        }
    }
    if wet > 0.0 {
        return (field, level / wet, depth / wet, fast / wet);
    }
    let (mut l, mut k) = (0.0f32, 0.0f32);
    for (dx, dy) in [(-1, 0), (1, 0), (0, -1), (0, 1), (-1, -1), (1, 1), (-1, 1), (1, -1)] {
        let (cx, cy) = (x + dx * step, y + dy * step);
        let h = at(&s.water.depth, cx, cy);
        if h > 0.0 {
            l += ground(cx, cy) + h;
            k += 1.0;
        }
    }
    (field, if k > 0.0 { l / k } else { ground(x, y) }, 0.0, 0.0)
}

pub fn surface_color(p: &Palette, d: f32, speed: f32) -> ([f32; 3], f32) {
    use crate::palette::FOAM;
    match crate::terrain_render::map_lens() {
        1 => return (mix_rgb([0.66, 0.85, 0.96], [0.08, 0.34, 0.66], (d / 3.0).clamp(0.0, 1.0).sqrt()), 0.93),
        2 => return ([0.62, 0.69, 0.75], 0.9),
        crate::terrain_render::ELEVATION_LENS => return (crate::terrain_render::ELEVATION_WATER, 0.95),
        _ => {}
    }
    let deep = smooth_step(0.02, 1.2, d).sqrt();
    let mut c = mix_rgb(p.water_shallow, p.water_deep, deep);
    let froude = if d > 1.0e-4 { speed / (9.81 * d).sqrt() } else { 0.0 };
    let white = smooth_step(WHITE_WATER_FROUDE, 1.6, froude) * smooth_step(0.15, 0.6, speed);
    c = mix_rgb(c, FOAM, white * 0.85);
    let mut a = 0.35 + 0.55 * smooth_step(0.0, 0.3, d) + 0.35 * white;
    let ice = if p.winter > 0.0 { ((p.winter * 4.5 - d) / 1.2).clamp(0.0, 1.0) } else { 0.0 };
    c = mix_rgb(c, p.ice, ice);
    a += (0.86 - a) * ice;
    (c, a.min(0.95))
}

pub fn build_water_mesh(s: &SimState, p: &Palette, cx: usize, cy: usize, step: usize) -> Option<Mesh> {
    let st = step as i32;
    let side = CHUNK / step + 1;
    let (x0, y0) = ((cx * CHUNK) as i32, (cy * CHUNK) as i32);
    let mut verts: Vec<WV> = Vec::with_capacity(side * side);
    let mut any = false;
    for j in 0..side as i32 {
        for i in 0..side as i32 {
            let (x, y) = (x0 + i * st, y0 + j * st);
            let (f, level, d, sp) = surface_sample(s, x, y, st);
            any |= f > 0.0;
            let (wx, wz) = world_pos(s, x, y);
            let ground = terrain_shown(s, x as f32, y as f32, step);
            verts.push(WV { p: [wx, level.max(ground) + SURFACE_LIFT, wz], f, d, s: sp });
        }
    }
    if !any {
        return None;
    }
    let (mut pos, mut col, mut uv) = (vec![], vec![], vec![]);
    for j in 0..side - 1 {
        for i in 0..side - 1 {
            let (a, b, c, e) = (j * side + i, j * side + i + 1, (j + 1) * side + i, (j + 1) * side + i + 1);
            for tri in [[a, c, b], [b, c, e]] {
                let t = [verts[tri[0]], verts[tri[1]], verts[tri[2]]];
                if t.iter().all(|v| v.f <= 0.0) {
                    continue;
                }
                let poly = clip_poly(&t, 0.0, true);
                for k in 1..poly.len().saturating_sub(1) {
                    for v in [&poly[0], &poly[k], &poly[k + 1]] {
                        pos.push(v.p);
                        uv.push([v.p[0] * 0.045, v.p[2] * 0.045]);
                        let (c, al) = surface_color(p, v.d, v.s);
                        let lin = crate::palette::display_linear(c).map(|q| q * WATER_GAIN);
                        col.push([lin[0], lin[1], lin[2], al]);
                    }
                }
            }
        }
    }
    if pos.is_empty() {
        return None;
    }
    let count = pos.len();
    Some(
        Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
            .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, pos)
            .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0f32, 1.0, 0.0]; count])
            .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uv)
            .with_inserted_attribute(Mesh::ATTRIBUTE_TANGENT, vec![[1.0f32, 0.0, 0.0, -1.0]; count])
            .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, col)
            .with_inserted_indices(Indices::U32((0..count as u32).collect())),
    )
}

pub fn pond_view(s: &SimState) -> Option<Vec3> {
    let g = &s.world.pond;
    let (w, h) = (g.width(), g.height());
    let is_pond = |x: usize, y: usize| g.get(x, y).copied().unwrap_or(false);
    let mut seeds: Vec<(usize, usize)> = Vec::new();
    for y in 0..h {
        for x in 0..w {
            if is_pond(x, y) && !seeds.iter().any(|&(sx, sy)| sx.abs_diff(x) + sy.abs_diff(y) < 12) {
                seeds.push((x, y));
            }
        }
    }
    let (cxm, cym) = (w as f32 / 2.0, h as f32 / 2.0);
    let (fx, fy) = seeds
        .iter()
        .map(|&(px, py)| {
            let (mut sx, mut sy, mut c) = (0.0, 0.0, 0.0);
            for y in py.saturating_sub(10)..(py + 11).min(h) {
                for x in px.saturating_sub(10)..(px + 11).min(w) {
                    if is_pond(x, y) {
                        sx += x as f32;
                        sy += y as f32;
                        c += 1.0;
                    }
                }
            }
            let (fx, fy) = ((sx / c).round() as i32, (sy / c).round() as i32);
            let mut veg = 0.0;
            for dy in -8..=8 {
                for dx in -8..=8 {
                    veg += at(&s.ecology.vegetation, fx + dx, fy + dy).clamp(0.0, 1.0);
                }
            }
            let centre = ((fx as f32 - cxm).powi(2) + (fy as f32 - cym).powi(2)).sqrt() / w as f32;
            (veg + centre * 40.0, fx, fy)
        })
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(_, x, y)| (x, y))?;
    let (wx, wz) = world_pos(s, fx, fy);
    Some(Vec3::new(wx, at(&s.height, fx, fy) + at(&s.water.depth, fx, fy), wz))
}

fn smooth_step(e0: f32, e1: f32, x: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
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
    mut cursor: Local<usize>,
    mut backlog: ResMut<MeshBacklog>,
) {
    backlog.water = true;
    let Some(campos) = cam.iter().next().map(|t| t.translation) else { return };
    let (cols, rows) = chunk_dims(&sim.0);
    if water.slots.len() != cols * rows {
        return;
    }
    let mut budget = if *first { 10 } else { usize::MAX };
    *first = true;
    let started = std::time::Instant::now();
    let n_chunks = cols * rows;
    let mut rebuilt = false;
    let palette = Palette::at(season.phase);
    let sea_bucket = season.bucket() as u64;
    for k in 0..n_chunks {
        {
            let idx = (*cursor + k) % n_chunks;
            let (cx, cy) = (idx % cols, idx / cols);
            let step = lod_step(campos, chunk_centre(&sim.0, cx, cy));
            let hash = region_hash(&sim.0, cx, cy, WATER_PAD, sea_bucket, false);
            let slot = &mut water.slots[idx];
            if slot.hash == hash.max(1) && slot.step == step {
                continue;
            }
            if budget == 0 || started.elapsed().as_secs_f32() > WATER_REBUILD_BUDGET_S {
                *cursor = idx;
                return;
            }
            budget -= 1;
            rebuilt = true;
            MESH_REBUILDS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let built = std::time::Instant::now();
            let mesh = build_water_mesh(&sim.0, &palette, cx, cy, step);
            crate::perf::WATER_MESH_NS.fetch_add(built.elapsed().as_nanos() as u64, std::sync::atomic::Ordering::Relaxed);
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
    backlog.water = rebuilt;
}

fn animate_water(time: Res<Time>, water: Res<WaterChunks>, mut materials: ResMut<Assets<StandardMaterial>>, season: Res<Season>) {
    let t = time.elapsed_secs();
    let calm = 1.0 - 0.9 * Palette::at(season.phase).winter;
    if let Some(mut m) = materials.get_mut(&water.material) {
        m.uv_transform = Affine2::from_scale_angle_translation(
            Vec2::splat(1.0),
            0.12 * (t * 0.15).sin(),
            Vec2::new(t * 0.012 * calm, t * 0.008 * calm + 0.02 * (t * 0.3).sin()),
        );
    }
}

fn demo_world(mut commands: Commands, mut sim: ResMut<Sim>, mut meshes: ResMut<Assets<Mesh>>, mut materials: ResMut<Assets<StandardMaterial>>, mut orbit: ResMut<OrbitCamera>) {
    if std::env::var("BEAVER_DEMO").is_err() {
        return;
    }
    let s = &mut sim.0;
    let Some((x, y, bed)) = carve_demo_lake(s) else { warn!("demo: world too small for a lake"); return };
    let (wx, wz) = world_pos(s, x as i32, y as i32);
    const SCALE: f32 = 1.6;
    let pos = Vec3::new(wx, bed + 0.9 * SCALE, wz);
    let body = materials.add(StandardMaterial { base_color: Color::srgb(crate::palette::FUR[0], crate::palette::FUR[1], crate::palette::FUR[2]), perceptual_roughness: 0.9, ..default() });
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

pub const DEMO_DEPTH: f32 = 4.5;

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
    let h = |x: usize, y: usize| s.height.as_slice()[y * w + x];
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
    let channel = s.water.channel.as_mut_slice();
    for (i, &hh) in s.height.as_slice().iter().enumerate() {
        let (x, y) = (i % w, i / w);
        let target = if ring(x, y) < R { lake } else { level };
        if hh < target {
            wd[i] = target - hh;
            channel[i] = false;
        }
    }
    Some((sx, sy, s.height.as_slice()[sy * w + sx]))
}

