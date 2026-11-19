use crate::camera::{MainCamera, OrbitCamera};
use crate::state::Sim;
use crate::terrain_render::*;
use bevy::asset::RenderAssetUsages;
use bevy::mesh::Indices;
use bevy::pbr::{ExtendedMaterial, MaterialExtension};
use bevy::prelude::*;
use bevy::render::render_resource::{AsBindGroup, PrimitiveTopology};
use bevy::shader::ShaderRef;
use sim::SimState;

pub struct WaterRenderPlugin;

#[derive(Component)]
pub struct WaterSurface;

#[derive(Component)]
pub struct DemoObject;

impl Plugin for WaterRenderPlugin {
    fn build(&self, app: &mut App) {
        bevy::asset::embedded_asset!(app, "water.wesl");
        app.add_plugins(MaterialPlugin::<WaterMaterial>::default())
            .init_resource::<WaterChunks>()
            .add_systems(Startup, (demo_world.before(spawn_terrain), setup_water))
            .add_systems(Update, (update_water_chunks, update_water_style));
    }
}

pub type WaterMaterial = ExtendedMaterial<StandardMaterial, WaterStyle>;

/// Colours and look of the water surface (see water.wesl).
#[derive(Asset, AsBindGroup, Reflect, Debug, Clone)]
pub struct WaterStyle {
    #[uniform(100)]
    pub shallow: Vec4,
    #[uniform(100)]
    pub deep: Vec4,
    #[uniform(100)]
    pub foam: Vec4,
    /// x: depth at which water reads deep (m), y: alpha of a film, z: alpha when deep, w: ice.
    #[uniform(100)]
    pub look: Vec4,
}

impl MaterialExtension for WaterStyle {
    fn fragment_shader() -> ShaderRef {
        "embedded://beaver_colony/water.wesl".into()
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
    material: Handle<WaterMaterial>,
}

fn style_for(p: &Palette) -> WaterStyle {
    let lin = |c: [f32; 3]| {
        let l = crate::palette::display_linear(c);
        Vec4::new(l[0], l[1], l[2], 1.0)
    };
    WaterStyle {
        shallow: lin(SURFACE_SHALLOW),
        deep: lin(SURFACE_DEEP),
        foam: lin(crate::palette::FOAM),
        look: Vec4::new(WATER_DEEP_M, WATER_FILM_ALPHA, WATER_DEEP_ALPHA, (p.winter * 1.5).clamp(0.0, 1.0)),
    }
}

/// Water colours: a clear green-teal where the bed shows through, river blue where it is deep.
const SURFACE_SHALLOW: [f32; 3] = [0.30, 0.58, 0.56];
const SURFACE_DEEP: [f32; 3] = [0.10, 0.31, 0.45];
/// Depth at which water shows its own deep colour rather than its bed.
const WATER_DEEP_M: f32 = 0.6;
const WATER_FILM_ALPHA: f32 = 0.5;
const WATER_DEEP_ALPHA: f32 = 0.93;

fn setup_water(sim: Res<Sim>, season: Res<Season>, mut water: ResMut<WaterChunks>, mut materials: ResMut<Assets<WaterMaterial>>) {
    water.material = materials.add(WaterMaterial {
        base: StandardMaterial { base_color: Color::WHITE, alpha_mode: AlphaMode::Blend, perceptual_roughness: 0.3, reflectance: 0.06, cull_mode: None, ..default() },
        extension: style_for(&Palette::at(season.phase)),
    });
    let (cols, rows) = chunk_dims(&sim.0);
    water.slots = (0..cols * rows).map(|_| WSlot::default()).collect();
}

fn update_water_style(season: Res<Season>, water: Res<WaterChunks>, mut materials: ResMut<Assets<WaterMaterial>>, mut last: Local<Option<u32>>) {
    let bucket = (season.phase * 400.0) as u32;
    if *last == Some(bucket) {
        return;
    }
    *last = Some(bucket);
    if let Some(mut m) = materials.get_mut(&water.material) {
        m.extension = style_for(&Palette::at(season.phase));
    }
}

const WATER_REBUILD_BUDGET_S: f32 = 0.002;
pub const WATER_PAD: i32 = SHORE_SEARCH + 1;
const SHORE_SEARCH: i32 = 3;
#[derive(Clone, Copy)]
struct WV {
    p: [f32; 3],
    depth: f32,
    white: f32,
    vel: [f32; 2],
}

fn lerp_wv(a: &WV, b: &WV, t: f32) -> WV {
    let l = |x: f32, y: f32| x + (y - x) * t;
    WV { p: [l(a.p[0], b.p[0]), l(a.p[1], b.p[1]), l(a.p[2], b.p[2])], depth: l(a.depth, b.depth), white: l(a.white, b.white), vel: [l(a.vel[0], b.vel[0]), l(a.vel[1], b.vel[1])] }
}

/// The part of a polygon where the water is at least `level` deep.
fn clip_poly(poly: &[WV], level: f32) -> Vec<WV> {
    let mut out = Vec::with_capacity(poly.len() + 2);
    for k in 0..poly.len() {
        let (a, b) = (&poly[k], &poly[(k + 1) % poly.len()]);
        let (ia, ib) = (a.depth >= level, b.depth >= level);
        if ia {
            out.push(*a);
        }
        if ia != ib {
            out.push(lerp_wv(a, b, (level - a.depth) / (b.depth - a.depth)));
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

/// Water thinner than this is not drawn (a damp film reads as wet ground, not water).
const SHOW_MIN_M: f32 = 0.03;
const SURFACE_LIFT: f32 = 0.01;

/// Water at one mesh vertex: the surface level, velocity and white water of the visibly wet
/// cells it stands for (the step x step block at and after it).
fn vertex_water(s: &SimState, x: i32, y: i32, step: i32) -> Option<(f32, [f32; 2], f32)> {
    let n = s.height.width() as i32;
    let (mut level, mut vx, mut vy, mut white, mut k) = (0.0f32, 0.0f32, 0.0f32, 0.0f32, 0.0f32);
    for by in 0..step {
        for bx in 0..step {
            let (cx, cy) = ((x + bx).clamp(0, n - 1), (y + by).clamp(0, n - 1));
            let h = at(&s.water.depth, cx, cy);
            if h <= SHOW_MIN_M {
                continue;
            }
            let (u, v) = s.water.velocity.get(cx as usize, cy as usize).copied().unwrap_or((0.0, 0.0));
            let speed = (u * u + v * v).sqrt();
            let froude = speed / (9.81 * h).sqrt();
            white = white.max(smooth_step(0.55, 1.0, froude) * smooth_step(0.2, 0.8, speed));
            level += at(&s.height, cx, cy) + at(&s.water.dam_height, cx, cy) + h;
            vx += u;
            vy += v;
            k += 1.0;
        }
    }
    (k > 0.0).then(|| (level / k, [vx / k, vy / k], white))
}

pub fn build_water_mesh(s: &SimState, cx: usize, cy: usize, step: usize) -> Option<Mesh> {
    let st = step as i32;
    let side = CHUNK / step + 1;
    let (x0, y0) = ((cx * CHUNK) as i32, (cy * CHUNK) as i32);
    // Sample one vertex beyond the chunk on every side, so shorelines extrapolate across seams.
    let ext = side + 2;
    let mut own: Vec<Option<(f32, [f32; 2], f32)>> = Vec::with_capacity(ext * ext);
    let mut any = false;
    for j in -1..=side as i32 {
        for i in -1..=side as i32 {
            let w = vertex_water(s, x0 + i * st, y0 + j * st, st);
            any |= w.is_some();
            own.push(w);
        }
    }
    if !any {
        return None;
    }
    let mut verts: Vec<WV> = Vec::with_capacity(side * side);
    for j in 0..side {
        for i in 0..side {
            let (x, y) = (x0 + i as i32 * st, y0 + j as i32 * st);
            let at_ext = |di: i32, dj: i32| own[(j as i32 + 1 + dj) as usize * ext + (i as i32 + 1 + di) as usize];
            // A dry vertex takes the level of the water beside it, so the waterline falls where
            // that level meets the ground between them.
            let (level, vel, white) = match at_ext(0, 0) {
                Some(w) => w,
                None => {
                    let (mut l, mut k) = (0.0f32, 0.0f32);
                    for (di, dj) in [(-1, 0), (1, 0), (0, -1), (0, 1), (-1, -1), (1, 1), (-1, 1), (1, -1)] {
                        if let Some(w) = at_ext(di, dj) {
                            l += w.0;
                            k += 1.0;
                        }
                    }
                    if k > 0.0 { (l / k, [0.0, 0.0], 0.0) } else { (f32::NEG_INFINITY, [0.0, 0.0], 0.0) }
                }
            };
            let ground = terrain_shown(s, x as f32, y as f32, step);
            let depth = if level.is_finite() { level - ground } else { -1.0 };
            let (wx, wz) = world_pos(s, x, y);
            verts.push(WV { p: [wx, level.max(ground) + SURFACE_LIFT, wz], depth, white, vel });
        }
    }
    let (mut pos, mut col, mut uv, mut uv1) = (vec![], vec![], vec![], vec![]);
    for j in 0..side - 1 {
        for i in 0..side - 1 {
            let (a, b, c, e) = (j * side + i, j * side + i + 1, (j + 1) * side + i, (j + 1) * side + i + 1);
            for tri in [[a, c, b], [b, c, e]] {
                let t = [verts[tri[0]], verts[tri[1]], verts[tri[2]]];
                if t.iter().all(|v| v.depth <= 0.0) {
                    continue;
                }
                let poly = clip_poly(&t, 0.0);
                for k in 1..poly.len().saturating_sub(1) {
                    for v in [&poly[0], &poly[k], &poly[k + 1]] {
                        pos.push(v.p);
                        uv.push([v.p[0], v.p[2]]);
                        uv1.push(v.vel);
                        col.push([v.depth.max(0.0), v.white, 0.0, 1.0]);
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
            .with_inserted_attribute(Mesh::ATTRIBUTE_UV_1, uv1)
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
            let mesh = build_water_mesh(&sim.0, cx, cy, step);
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
                    slot.entity = Some(commands.spawn((WaterSurface, Mesh3d(slot.mesh.clone()), MeshMaterial3d(material), Transform::default(), bevy::light::NotShadowCaster)).id());
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

