use crate::camera::MainCamera;
use crate::state::Sim;
use bevy::app::AppExit;
use bevy::asset::RenderAssetUsages;
use bevy::mesh::Indices;
use bevy::pbr::{ExtendedMaterial, MaterialExtension};
use bevy::prelude::*;
use bevy::render::render_resource::AsBindGroup;
use bevy::shader::ShaderRef;
use bevy::render::render_resource::PrimitiveTopology;
use bevy::window::PresentMode;
use sim::SimState;

pub const CELL_SIZE: f32 = 1.0;
pub const CHUNK: usize = 16;
pub const YEAR_DAYS: f64 = sim::calendar::YEAR_DAYS;
const REBUILD_BUDGET: usize = 10;
const LOD1_DISTANCE: f32 = 150.0;
pub const WET_EPS: f32 = 0.02;

pub struct TerrainRenderPlugin;

#[derive(Component)]
pub struct Terrain;

#[derive(Resource, Clone, Copy, Debug)]
pub struct Season {
    pub phase: f32,
}

impl Season {
    pub fn bucket(&self) -> u32 {
        (self.phase * 120.0) as u32
    }
}

fn season_override() -> Option<f32> {
    let v = std::env::var("BEAVER_SEASON").ok()?;
    match v.as_str() {
        "spring" => Some(0.125),
        "summer" => Some(0.375),
        "autumn" => Some(0.625),
        "winter" => Some(0.875),
        n => n.parse::<f32>().ok().map(|p| p.rem_euclid(1.0)),
    }
}

impl Plugin for TerrainRenderPlugin {
    fn build(&self, app: &mut App) {
        let start = season_override().unwrap_or(0.3);
        bevy::asset::embedded_asset!(app, "terrain_shadow.wesl");
        app.add_plugins(MaterialPlugin::<TerrainMaterial>::default());
        app.insert_resource(Season { phase: start })
            .init_resource::<TerrainChunks>()
            .init_resource::<MeshBacklog>()
            .add_systems(Startup, spawn_terrain)
            .add_systems(Update, (update_season, update_sky_and_light, update_terrain_chunks).chain())
            .add_systems(Update, frame_log);
    }
}

use crate::palette::{self as pal, mix, rgb, Rgb};
fn smooth(e0: f32, e1: f32, x: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}
pub fn lin(c: Rgb, a: f32) -> [f32; 4] {
    let l = Color::srgb(c[0], c[1], c[2]).to_linear();
    [l.red, l.green, l.blue, a]
}

#[derive(Clone, Copy, Debug)]
pub struct Palette {
    pub grass: Rgb,
    pub grass_light: Rgb,
    pub grass_deep: Rgb,
    pub mud: Rgb,
    pub rock: Rgb,
    pub wetland: Rgb,
    pub snow: Rgb,
    pub sky: Rgb,
    pub sun: Rgb,
    pub sun_lux: f32,
    pub ambient: Rgb,
    pub ambient_lux: f32,
    pub water_shallow: Rgb,
    pub water_deep: Rgb,
    pub ice: Rgb,
    pub winter: f32,
}

const GRASS_LIGHT: [Rgb; 4] = [rgb(0xB6E284), pal::MEADOW_LIGHT, rgb(0xE8D890), rgb(0xE2E8E4)];
const GRASS: [Rgb; 4] = [rgb(0x94CE70), pal::MEADOW, pal::STRAW, rgb(0xCBD8D0)];
const GRASS_DEEP: [Rgb; 4] = [rgb(0x68B35C), pal::MEADOW_DEEP, rgb(0xB7AE66), rgb(0xA9BAB6)];
const WETLAND: [Rgb; 4] = [rgb(0x74C48A), rgb(0x6CBE86), rgb(0xBFB574), rgb(0xA8C2BC)];
const SKY: [Rgb; 4] = [pal::SKY, pal::SKY, rgb(0xB4D0EC), pal::SKY_WINTER];
const SUN: [Rgb; 4] = [pal::SUN, pal::SUN, pal::SUN_AUTUMN, rgb(0xF0F2FF)];
const SUN_LUX: [f32; 4] = [1_300.0, 1_400.0, 1_250.0, 1_000.0];
const AMBIENT: [Rgb; 4] = [[0.98, 0.95, 0.99], [0.98, 0.95, 0.99], [1.0, 0.92, 0.93], [0.96, 0.96, 0.98]];
pub const AMBIENT_LUX: f32 = 4_400.0;
const WATER_SHALLOW: [Rgb; 4] = [pal::WATER_SHALLOW, pal::WATER_SHALLOW, rgb(0x7CCBD0), rgb(0x9CCFE2)];
const WATER_DEEP: [Rgb; 4] = [pal::WATER_DEEP, pal::WATER_DEEP, rgb(0x3C84A0), rgb(0x5A9CC0)];

fn cyc<T: Copy>(k: &[T; 4], phase: f32, lerp: impl Fn(T, T, f32) -> T) -> T {
    let p = phase.rem_euclid(1.0) * 4.0 - 0.5;
    let i0 = p.floor();
    let f = smooth(0.0, 1.0, p - i0);
    let a = k[(i0 as i32).rem_euclid(4) as usize];
    let b = k[(i0 as i32 + 1).rem_euclid(4) as usize];
    lerp(a, b, f)
}

impl Palette {
    pub fn at(phase: f32) -> Palette {
        let c = |k: &[Rgb; 4]| cyc(k, phase, mix);
        let d = (phase.rem_euclid(1.0) - 0.875).abs();
        let d = d.min(1.0 - d);
        Palette {
            grass: c(&GRASS),
            grass_light: c(&GRASS_LIGHT),
            grass_deep: c(&GRASS_DEEP),
            mud: pal::MUD,
            rock: pal::STONE,
            wetland: c(&WETLAND),
            snow: [0.97, 0.97, 1.0],
            sky: c(&SKY),
            sun: c(&SUN),
            sun_lux: cyc(&SUN_LUX, phase, |a, b, t| a + (b - a) * t),
            ambient: c(&AMBIENT),
            ambient_lux: AMBIENT_LUX,
            water_shallow: c(&WATER_SHALLOW),
            water_deep: c(&WATER_DEEP),
            ice: [0.86, 0.94, 0.99],
            winter: 1.0 - smooth(0.04, 0.2, d),
        }
    }
}

fn update_season(sim: Res<Sim>, mut season: ResMut<Season>, mut started: Local<Option<f32>>) {
    if let Some(p) = season_override() {
        if season.phase != p {
            season.phase = p;
        }
        return;
    }
    let base = *started.get_or_insert(season.phase);
    let phase = ((base as f64 + sim.0.time_days / YEAR_DAYS).rem_euclid(1.0)) as f32;
    let next = Season { phase };
    if next.bucket() != season.bucket() {
        *season = next;
    }
}

fn update_sky_and_light(
    season: Res<Season>,
    mut clear: ResMut<ClearColor>,
    mut ambient: ResMut<GlobalAmbientLight>,
    mut suns: Query<&mut DirectionalLight>,
) {
    if !season.is_changed() {
        return;
    }
    let p = Palette::at(season.phase);
    clear.0 = Color::srgb(p.sky[0], p.sky[1], p.sky[2]);
    ambient.color = Color::srgb(p.ambient[0], p.ambient[1], p.ambient[2]);
    ambient.brightness = p.ambient_lux;
    for mut s in &mut suns {
        s.color = Color::srgb(p.sun[0], p.sun[1], p.sun[2]);
        s.illuminance = p.sun_lux;
    }
}

pub fn at(g: &sim::Grid<f32>, x: i32, y: i32) -> f32 {
    let x = x.clamp(0, g.width() as i32 - 1) as usize;
    let y = y.clamp(0, g.height() as i32 - 1) as usize;
    g.as_slice()[y * g.width() + x]
}

pub static MAP_LENS: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(0);

pub fn map_lens() -> u8 {
    MAP_LENS.load(std::sync::atomic::Ordering::Relaxed)
}

pub fn region_hash(s: &SimState, cx: usize, cy: usize, pad: i32, extra: u64, with_veg: bool) -> u64 {
    let extra = extra ^ ((map_lens() as u64) << 48);
    let mut h: u64 = 0xcbf2_9ce4_8422_2325 ^ extra.wrapping_mul(0x9E37_79B9_7F4A_7C15);
    let (x0, y0) = ((cx * CHUNK) as i32 - pad, (cy * CHUNK) as i32 - pad);
    let n = CHUNK as i32 + 2 * pad;
    for y in y0..=y0 + n {
        for x in x0..=x0 + n {
            let dq = (at(&s.water.depth, x, y) * 500.0).round() as i32;
            let mut v = at(&s.height, x, y).to_bits() as u64 ^ ((dq as u32 as u64) << 32);
            if with_veg {
                let vq = (at(&s.ecology.vegetation, x, y) * 64.0).round() as i32;
                v ^= (vq as u32 as u64).rotate_left(17);
                let bq = (dried_bed(s, x, y) * 6.0).round() as u64;
                v ^= bq.rotate_left(41);
            }
            h = (h ^ v).wrapping_mul(0x100_0000_01b3);
            h ^= h >> 29;
        }
    }
    h
}

pub fn lod_step(cam: Vec3, centre: Vec3) -> usize {
    if cam.distance(centre) > LOD1_DISTANCE { 2 } else { 1 }
}

pub fn chunk_centre(s: &SimState, cx: usize, cy: usize) -> Vec3 {
    let (w, d) = (s.height.width() as f32, s.height.height() as f32);
    Vec3::new(
        (cx * CHUNK) as f32 + CHUNK as f32 / 2.0 - w / 2.0,
        8.0,
        (cy * CHUNK) as f32 + CHUNK as f32 / 2.0 - d / 2.0,
    ) * Vec3::new(CELL_SIZE, 1.0, CELL_SIZE)
}

pub fn world_pos(s: &SimState, x: i32, y: i32) -> (f32, f32) {
    (
        (x as f32 - s.height.width() as f32 / 2.0) * CELL_SIZE,
        (y as f32 - s.height.height() as f32 / 2.0) * CELL_SIZE,
    )
}

pub fn chunk_dims(s: &SimState) -> (usize, usize) {
    (s.height.width().div_ceil(CHUNK), s.height.height().div_ceil(CHUNK))
}

pub fn max_height(s: &SimState) -> f32 {
    s.height.as_slice().iter().cloned().fold(1.0, f32::max)
}

pub fn min_height(s: &SimState) -> f32 {
    s.height.as_slice().iter().cloned().fold(f32::MAX, f32::min).min(max_height(s) - 0.5)
}

pub const ELEVATION_LENS: u8 = 4;

pub fn elevation_ramp(t: f32) -> Rgb {
    const STOPS: [Rgb; 4] = [[0.25, 0.36, 0.39], [0.45, 0.57, 0.47], [0.76, 0.71, 0.53], [0.96, 0.92, 0.80]];
    let t = t.clamp(0.0, 1.0) * (STOPS.len() - 1) as f32;
    let i = (t.floor() as usize).min(STOPS.len() - 2);
    mix(STOPS[i], STOPS[i + 1], t - i as f32)
}

pub const ELEVATION_WATER: Rgb = [0.31, 0.56, 0.86];

fn noise(x: i32, y: i32) -> f32 {
    let mut h = (x as u32).wrapping_mul(0x85eb_ca6b) ^ (y as u32).wrapping_mul(0xc2b2_ae35);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2c1b_3c6d);
    h ^= h >> 12;
    (h & 0xffff) as f32 / 65535.0 * 2.0 - 1.0
}

const GRADE_MID: f32 = 0.58;
const GRADE_PATCH: f32 = 1.3;
const GRADE_SUN: f32 = 1.2;

#[derive(Default)]
struct Slot {
    entity: Option<Entity>,
    mesh: Handle<Mesh>,
    hash: u64,
    step: usize,
}

#[derive(Resource, Default)]
pub struct TerrainChunks {
    slots: Vec<Slot>,
    material: Handle<TerrainMaterial>,
}

pub type TerrainMaterial = ExtendedMaterial<StandardMaterial, PlumShadow>;

pub const SHADOW_BLEND: f32 = pal::SHADOW_MIX;
const SHADOW_EDGE: [f32; 2] = [0.42, 0.58];
const LIGHT_RELIEF: f32 = 0.5;

#[derive(Asset, AsBindGroup, Reflect, Debug, Clone)]
pub struct PlumShadow {
    #[uniform(100)]
    pub plum: Vec4,
    #[uniform(100)]
    pub edge: Vec4,
}

impl Default for PlumShadow {
    fn default() -> Self {
        let p = pal::PLUM;
        let mask = if std::env::var_os("BC_SHADOW_MASK").is_some() { 1.0 } else { 0.0 };
        PlumShadow { plum: Vec4::new(p[0], p[1], p[2], SHADOW_BLEND), edge: Vec4::new(SHADOW_EDGE[0], SHADOW_EDGE[1], mask, LIGHT_RELIEF) }
    }
}

impl MaterialExtension for PlumShadow {
    fn fragment_shader() -> ShaderRef {
        "embedded://beaver_colony/terrain_shadow.wesl".into()
    }
}

pub fn spawn_terrain(
    mut commands: Commands,
    sim: Res<Sim>,
    mut chunks: ResMut<TerrainChunks>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<TerrainMaterial>>,
    season: Res<Season>,
    cam: Query<&Transform, With<MainCamera>>,
) {
    chunks.material = materials.add(TerrainMaterial {
        base: StandardMaterial { perceptual_roughness: 1.0, reflectance: 0.05, ..default() },
        extension: PlumShadow::default(),
    });
    let (w, d) = (sim.0.height.width() as f32 * CELL_SIZE / 2.0, sim.0.height.height() as f32 * CELL_SIZE / 2.0);
    let slab = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, vec![[-w, -2.5, -d], [w, -2.5, -d], [w, -2.5, d], [-w, -2.5, d]])
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0, -1.0, 0.0]; 4])
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]])
        .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, vec![lin([0.45, 0.38, 0.30], 1.0); 4])
        .with_inserted_indices(Indices::U32(vec![0, 1, 2, 0, 2, 3]));
    commands.spawn((Terrain, bevy::camera::visibility::NoFrustumCulling, Mesh3d(meshes.add(slab)), MeshMaterial3d(chunks.material.clone())));
    let (cols, rows) = chunk_dims(&sim.0);
    chunks.slots = (0..cols * rows).map(|_| Slot::default()).collect();
    let campos = cam.iter().next().map(|t| t.translation).unwrap_or(Vec3::new(0.0, 100.0, 100.0));
    for cy in 0..rows {
        for cx in 0..cols {
            rebuild_chunk(&mut commands, &mut chunks, &mut meshes, &sim.0, &season, campos, cx, cy, cols, true);
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn rebuild_chunk(
    commands: &mut Commands,
    chunks: &mut TerrainChunks,
    meshes: &mut Assets<Mesh>,
    s: &SimState,
    season: &Season,
    campos: Vec3,
    cx: usize,
    cy: usize,
    cols: usize,
    force: bool,
) -> bool {
    let step = lod_step(campos, chunk_centre(s, cx, cy));
    let hash = region_hash(s, cx, cy, 2, season.bucket() as u64, true);
    let idx = cy * cols + cx;
    let slot = &mut chunks.slots[idx];
    if !force && slot.hash == hash && slot.step == step {
        return false;
    }
    MESH_REBUILDS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let built = std::time::Instant::now();
    let mesh = build_chunk_mesh(s, &Palette::at(season.phase), cx, cy, step);
    crate::perf::TERRAIN_MESH_NS.fetch_add(built.elapsed().as_nanos() as u64, std::sync::atomic::Ordering::Relaxed);
    match slot.entity {
        Some(_) => {
            let _ = meshes.insert(&slot.mesh, mesh);
        }
        None => {
            slot.mesh = meshes.add(mesh);
            slot.entity = Some(commands.spawn((Mesh3d(slot.mesh.clone()), MeshMaterial3d(chunks.material.clone()), Transform::default())).id());
        }
    }
    let slot = &mut chunks.slots[idx];
    slot.hash = hash;
    slot.step = step;
    true
}

#[derive(Resource, Default)]
pub struct MeshBacklog {
    pub terrain: bool,
    pub water: bool,
}

#[allow(clippy::too_many_arguments)]
fn update_terrain_chunks(
    mut commands: Commands,
    sim: Res<Sim>,
    season: Res<Season>,
    mut chunks: ResMut<TerrainChunks>,
    mut meshes: ResMut<Assets<Mesh>>,
    cam: Query<&Transform, With<MainCamera>>,
    mut backlog: ResMut<MeshBacklog>,
) {
    backlog.terrain = true;
    let Some(campos) = cam.iter().next().map(|t| t.translation) else { return };
    let (cols, rows) = chunk_dims(&sim.0);
    if chunks.slots.len() != cols * rows {
        return;
    }
    let mut budget = REBUILD_BUDGET;
    for cy in 0..rows {
        for cx in 0..cols {
            if budget == 0 {
                return;
            }
            if rebuild_chunk(&mut commands, &mut chunks, &mut meshes, &sim.0, &season, campos, cx, cy, cols, false) {
                budget -= 1;
            }
        }
    }
    backlog.terrain = budget < REBUILD_BUDGET;
}

const STONE_SHADE_ALBEDO: Rgb = rgb(0x8C8290);

fn patch_noise(x: f32, y: f32, scale: f32, seed: i32) -> f32 {
    let (fx, fy) = (x / scale, y / scale);
    let (ix, iy) = (fx.floor(), fy.floor());
    let (tx, ty) = (smooth(0.0, 1.0, fx - ix), smooth(0.0, 1.0, fy - iy));
    let n = |dx: i32, dy: i32| noise(ix as i32 + dx + seed * 131, iy as i32 + dy - seed * 17);
    let top = n(0, 0) + (n(1, 0) - n(0, 0)) * tx;
    let bot = n(0, 1) + (n(1, 1) - n(0, 1)) * tx;
    top + (bot - top) * ty
}

fn smooth_gradient(s: &SimState, x: i32, y: i32, st: i32) -> (f32, f32) {
    let h = |dx: i32, dy: i32| at(&s.height, x + dx * st, y + dy * st);
    let gx = (h(1, -1) - h(-1, -1) + 2.0 * (h(1, 0) - h(-1, 0)) + h(1, 1) - h(-1, 1)) / 8.0;
    let gz = (h(-1, 1) - h(-1, -1) + 2.0 * (h(0, 1) - h(0, -1)) + h(1, 1) - h(1, -1)) / 8.0;
    (gx / (st as f32 * CELL_SIZE), gz / (st as f32 * CELL_SIZE))
}

fn slope_at(s: &SimState, x: i32, y: i32) -> f32 {
    let (gx, gz) = smooth_gradient(s, x, y, 1);
    (gx * gx + gz * gz).sqrt()
}

pub fn rock_weight(slope: f32) -> f32 {
    if slope <= pal::ROCK_SLOPE_MIN {
        return 0.0;
    }
    smooth(pal::ROCK_SLOPE_MIN, pal::ROCK_SLOPE_MIN + pal::ROCK_SLOPE_BLEND, slope)
}

fn vertex_color(s: &SimState, p: &Palette, (minh, maxh): (f32, f32), x: i32, y: i32) -> Rgb {
    match map_lens() {
        0 => world_color(s, p, maxh, x, y),
        ELEVATION_LENS => elevation_color(s, minh, maxh, x, y),
        lens => lens_color(s, maxh, x, y, lens),
    }
}

fn elevation_color(s: &SimState, minh: f32, maxh: f32, x: i32, y: i32) -> Rgb {
    let t = (at(&s.height, x, y) - minh) / (maxh - minh).max(0.5);
    let (gx, gz) = smooth_gradient(s, x, y, 1);
    let sunny = ((gx + gz) * -0.5).clamp(-0.5, 0.5);
    let c = elevation_ramp(t);
    let k = 0.92 + 0.2 * sunny;
    let land = [(c[0] * k).min(1.0), (c[1] * k).min(1.0), (c[2] * k).min(1.0)];
    mix(land, ELEVATION_WATER, smooth(WET_EPS, 0.15, at(&s.water.depth, x, y)))
}

fn lens_color(s: &SimState, maxh: f32, x: i32, y: i32, lens: u8) -> Rgb {
    let t = (at(&s.height, x, y) / maxh).clamp(0.0, 1.0);
    let depth = at(&s.water.depth, x, y);
    let (gx, gz) = smooth_gradient(s, x, y, 1);
    let sunny = ((gx + gz) * -0.5).clamp(-0.5, 0.5);
    let g = (0.52 + 0.36 * t) * (0.86 + 0.28 * sunny);
    let land = [g, g * 0.985, g * 0.955];
    let wet = smooth(WET_EPS, 0.3, depth);
    let water = if lens == 1 {
        mix([0.66, 0.85, 0.96], [0.08, 0.34, 0.66], (depth / 3.0).clamp(0.0, 1.0).sqrt())
    } else {
        [0.62, 0.69, 0.75]
    };
    mix(land, water, wet)
}

fn world_color(s: &SimState, p: &Palette, maxh: f32, x: i32, y: i32) -> Rgb {
    let h = at(&s.height, x, y);
    let t = h / maxh;
    let depth = at(&s.water.depth, x, y);
    let (gx, gz) = smooth_gradient(s, x, y, 1);
    let slope = (gx * gx + gz * gz).sqrt();
    let veg = at(&s.ecology.vegetation, x, y).clamp(0.0, 1.0);
    let (mut wet2, mut wet1) = (0.0, 0.0);
    for dy in -2..=2 {
        for dx in -2..=2 {
            if at(&s.water.depth, x + dx, y + dy) > WET_EPS {
                wet2 += 1.0 / 25.0;
                if dx.abs() <= 1 && dy.abs() <= 1 {
                    wet1 += 1.0 / 9.0;
                }
            }
        }
    }
    let submerged = smooth(WET_EPS, 0.4, depth);

    let patch = patch_noise(x as f32, y as f32, 14.0, 1) * 0.55 + patch_noise(x as f32, y as f32, 6.0, 2) * 0.25;
    let sunny = ((gx + gz) * -0.5).clamp(-0.5, 0.5);
    let g = (GRADE_MID + patch * GRADE_PATCH + sunny * GRADE_SUN - veg * 0.12 - wet2 * 0.2).clamp(0.0, 1.0);
    let mut c = if g < 0.5 { mix(p.grass_deep, p.grass, g * 2.0) } else { mix(p.grass, p.grass_light, g * 2.0 - 1.0) };
    let rock = rock_weight(slope);
    let rock_c = mix(STONE_SHADE_ALBEDO, p.rock, (0.55 + sunny * 0.9 + patch * 0.3).clamp(0.0, 1.0));
    let soil = smooth(0.35, pal::ROCK_SLOPE_MIN, slope) * (1.0 - rock);
    c = mix(c, pal::MUD, soil * 0.55);
    let autumn = smooth(0.45, 0.58, p.grass[0] - p.grass[2] + 0.2) * (1.0 - p.winter);
    let litter = smooth(0.15, 0.6, patch_noise(x as f32, y as f32, 5.0, 3) * 0.5 + 0.5) * autumn * veg.max(0.4);
    c = mix(c, pal::AUTUMN_RUST, litter * 0.35 * (1.0 - rock));
    c = mix(c, rock_c, rock);
    let wetland = smooth(0.05, 0.3, wet2) * (1.0 - smooth(0.4, 0.6, t));
    c = mix(c, p.wetland, wetland * (1.0 - rock));
    c = mix(c, p.mud, smooth(0.4, 1.0, wet1) * 0.7);
    let channel = stream_bed(s, x, y);
    c = mix(c, mix(STREAM_GRAVEL, STREAM_MUD, smooth(0.0, 0.3, depth)), channel);
    let bed = mix([0.56, 0.78, 0.66], mix(p.water_shallow, p.water_deep, 0.6), (depth / 2.5).max(0.0).sqrt().min(1.0));
    c = mix(c, bed, submerged);
    let dried = dried_bed(s, x, y) * (1.0 - submerged);
    if dried > 0.0 {
        let mottle = smooth(-0.35, 0.45, patch_noise(x as f32, y as f32, 2.5, 4));
        let silt = mix(rgb(0xA88A66), rgb(0xD8C6A2), mottle);
        c = mix(c, silt, dried * 0.9);
    }
    let cap = smooth(0.8, 0.95, t);
    let cover = (p.winter * smooth(0.0, 0.2, 1.0 - slope * 0.5) * (1.0 - 0.6 * smooth(0.5, 1.2, slope))).max(cap * 0.0);
    c = mix(c, p.snow, cover * (1.0 - submerged));
    let lo = mix(p.grass_deep, STONE_SHADE_ALBEDO, rock);
    let f = 1.0 - submerged;
    let lift = |v: f32, floor: f32| v + (v.max(floor * 1.05) - v) * f;
    [lift(c[0], lo[0]), lift(c[1], lo[1]), lift(c[2], lo[2])]
}

const STREAM_GRAVEL: Rgb = [0.55, 0.50, 0.42];
const STREAM_MUD: Rgb = [0.36, 0.31, 0.25];

fn stream_bed(s: &SimState, x: i32, y: i32) -> f32 {
    let n = s.world.size as i32;
    let mut best = 0.0f32;
    for dy in -2..=2 {
        for dx in -2..=2 {
            let (i, j) = (x + dx, y + dy);
            if !(0..n).contains(&i) || !(0..n).contains(&j) || !s.world.river.get(i as usize, j as usize).copied().unwrap_or(false) {
                continue;
            }
            let half = sim::water::bankfull(*s.world.river_flow.get(i as usize, j as usize).unwrap()).0 / s.world.cell_m * 0.5;
            let d = ((dx * dx + dy * dy) as f32).sqrt();
            best = best.max((half + 0.5 - d).clamp(0.0, 1.0));
        }
    }
    best
}

fn dried_bed(s: &SimState, x: i32, y: i32) -> f32 {
    if at(&s.water.depth, x, y) > WET_EPS {
        return 0.0;
    }
    smooth(0.5, 20.0, at(&s.ecology.flood_days, x, y))
}

pub fn build_chunk_mesh(s: &SimState, p: &Palette, cx: usize, cy: usize, step: usize) -> Mesh {
    let maxh = max_height(s);
    let range = (if map_lens() == ELEVATION_LENS { min_height(s) } else { 0.0 }, maxh);
    let (w, d) = (s.height.width() as i32, s.height.height() as i32);
    let n = CHUNK / step;
    let (x0, y0) = ((cx * CHUNK) as i32, (cy * CHUNK) as i32);
    let st = step as i32;
    let (mut pos, mut nor, mut col, mut uv) = (vec![], vec![], vec![], vec![]);
    let mut vert = |x: i32, y: i32, drop: f32, shade: f32| {
        let (wx, wz) = world_pos(s, x, y);
        let h = at(&s.height, x, y);
        pos.push([wx, h - drop, wz]);
        let (gx, gz) = smooth_gradient(s, x, y, st);
        nor.push(Vec3::new(-gx, 1.0, -gz).normalize().to_array());
        let c = vertex_color(s, p, range, x, y);
        col.push(lin([c[0] * shade, c[1] * shade, c[2] * shade], 1.0));
        uv.push([x as f32 / w as f32, y as f32 / d as f32]);
    };
    let side = n + 1;
    for j in 0..=n {
        for i in 0..=n {
            vert(x0 + (i * step) as i32, y0 + (j * step) as i32, 0.0, 1.0);
        }
    }
    let mut idx: Vec<u32> = Vec::with_capacity(n * n * 6);
    let cells = |idx: &mut Vec<u32>| {
        for j in 0..n {
            for i in 0..n {
                let a = (j * side + i) as u32;
                let (b, c, e) = (a + 1, a + side as u32, a + side as u32 + 1);
                idx.extend_from_slice(&[a, c, b, b, c, e]);
            }
        }
    };
    cells(&mut idx);
    let skirt = 2.5;
    let edges: [(Box<dyn Fn(usize) -> (usize, usize)>, bool); 4] = [
        (Box::new(|k| (k, 0)), false),
        (Box::new(move |k| (k, n)), true),
        (Box::new(|k| (0, k)), true),
        (Box::new(move |k| (n, k)), false),
    ];
    for (e, (f, flip)) in edges.into_iter().enumerate() {
        let base = (side * side + e * (n + 1)) as u32;
        for k in 0..=n {
            let (i, j) = f(k);
            vert(x0 + (i * step) as i32, y0 + (j * step) as i32, skirt, 0.78);
        }
        for k in 0..n {
            let (i0, j0) = f(k);
            let (i1, j1) = f(k + 1);
            let (a, b) = ((j0 * side + i0) as u32, (j1 * side + i1) as u32);
            let (sa, sb) = (base + k as u32, base + k as u32 + 1);
            if flip {
                idx.extend_from_slice(&[a, b, sa, b, sb, sa]);
            } else {
                idx.extend_from_slice(&[a, sa, b, b, sa, sb]);
            }
        }
    }
    if step == 1 {
        let mut d = Detail { pos: &mut pos, nor: &mut nor, col: &mut col, uv: &mut uv, idx: &mut idx, uvs: [1.0 / w as f32, 1.0 / d as f32] };
        scatter_detail(s, p, maxh, x0, y0, &mut d);
    }
    Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, pos)
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, nor)
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uv)
        .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, col)
        .with_inserted_indices(Indices::U32(idx))
}

struct Detail<'a> {
    pos: &'a mut Vec<[f32; 3]>,
    nor: &'a mut Vec<[f32; 3]>,
    col: &'a mut Vec<[f32; 4]>,
    uv: &'a mut Vec<[f32; 2]>,
    idx: &'a mut Vec<u32>,
    uvs: [f32; 2],
}

fn blob(d: &mut Detail, centre: Vec3, r: Vec3, yaw: f32, lean: f32, dark: Rgb, light: Rgb) {
    const SEG: usize = 6;
    const RINGS: usize = 3;
    let rot = Quat::from_rotation_y(yaw) * Quat::from_rotation_z(lean);
    let base = d.pos.len() as u32;
    for j in 0..=RINGS {
        let v = j as f32 / RINGS as f32;
        let (sv, cv) = (v * std::f32::consts::PI).sin_cos();
        for i in 0..SEG {
            let a = i as f32 / SEG as f32 * std::f32::consts::TAU;
            let unit = Vec3::new(sv * a.cos(), cv, sv * a.sin());
            let p = centre + rot * (unit * r);
            let n = (rot * (unit / r).normalize_or_zero()).to_array();
            d.pos.push(p.to_array());
            d.nor.push(n);
            d.col.push(lin(mix(light, dark, v), 1.0));
            d.uv.push([p.x * d.uvs[0], p.z * d.uvs[1]]);
        }
    }
    for j in 0..RINGS as u32 {
        for i in 0..SEG as u32 {
            let (a, b) = (base + j * SEG as u32 + i, base + j * SEG as u32 + (i + 1) % SEG as u32);
            let (c, e) = (a + SEG as u32, b + SEG as u32);
            d.idx.extend_from_slice(&[a, c, b, b, c, e]);
        }
    }
}

const DETAIL_DENSITY: f32 = 0.15;

fn scatter_detail(s: &SimState, p: &Palette, maxh: f32, x0: i32, y0: i32, d: &mut Detail) {
    let (w, h) = (s.height.width() as i32, s.height.height() as i32);
    for y in y0..y0 + CHUNK as i32 {
        for x in x0..x0 + CHUNK as i32 {
            if x >= w - 1 || y >= h - 1 {
                continue;
            }
            let r = (noise(x * 7 + 3, y * 5 - 11) * 0.5 + 0.5, noise(x * 13 - 5, y * 3 + 17) * 0.5 + 0.5, noise(x - 29, y * 11 + 2) * 0.5 + 0.5);
            if r.0 > DETAIL_DENSITY {
                continue;
            }
            let (fx, fz) = (r.1 * 0.8 + 0.1, r.2 * 0.8 + 0.1);
            let mut ok = true;
            for dy in -1..=2 {
                for dx in -1..=2 {
                    ok &= at(&s.water.depth, x + dx, y + dy) <= WET_EPS;
                }
            }
            let t = at(&s.height, x, y) / maxh;
            ok &= dried_bed(s, x, y) < 0.35;
            if !ok || slope_at(s, x, y) > 0.45 || t > 0.62 || p.winter > 0.7 {
                continue;
            }
            let hh = |xx: i32, yy: i32| at(&s.height, xx, yy);
            let top = hh(x, y) + (hh(x + 1, y) - hh(x, y)) * fx;
            let bot = hh(x, y + 1) + (hh(x + 1, y + 1) - hh(x, y + 1)) * fx;
            let ground = top + (bot - top) * fz;
            let (wx, wz) = world_pos(s, x, y);
            let c = Vec3::new(wx + fx * CELL_SIZE, ground, wz + fz * CELL_SIZE);
            let kind = r.0 / DETAIL_DENSITY;
            let yaw = r.2 * std::f32::consts::TAU;
            if kind < 0.5 {
                for k in 0..2 {
                    blob(d, c + Vec3::Y * 0.1, Vec3::new(0.04, 0.17, 0.15), yaw + k as f32 * 1.1, (k as f32 - 0.5) * 0.5, pal::MEADOW_DEEP, pal::LILY);
                }
            } else if kind < 0.8 {
                let petal = if r.1 < 0.34 { pal::FLOWER_WHITE } else if r.1 < 0.67 { pal::FLOWER_PINK } else { pal::FLOWER_YELLOW };
                blob(d, c + Vec3::Y * 0.09, Vec3::new(0.025, 0.09, 0.025), 0.0, 0.0, pal::MEADOW_DEEP, pal::MEADOW_DEEP);
                blob(d, c + Vec3::Y * 0.19, Vec3::new(0.12, 0.08, 0.12), yaw, 0.0, petal, petal);
            } else {
                blob(d, c + Vec3::Y * 0.07, Vec3::new(0.16, 0.1, 0.11), yaw, 0.0, pal::STONE_SHADE, pal::STONE);
            }
        }
    }
}

const WARMUP_FRAMES: usize = 60;

pub static MESH_REBUILDS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
const SAMPLE_FRAMES: usize = 300;

fn frame_log(
    time: Res<Time<Real>>,
    cli: Res<crate::cli::Cli>,
    mut windows: Query<&mut Window>,
    mut samples: Local<Vec<f32>>,
    mut frame: Local<usize>,
    mut rebuilds_at_start: Local<usize>,
    mut exit: MessageWriter<AppExit>,
) {
    let Ok(path) = std::env::var("BEAVER_FRAMELOG") else { return };
    *frame += 1;
    if *frame == 1 {
        for mut w in &mut windows {
            w.present_mode = PresentMode::AutoNoVsync;
        }
    }
    if *frame <= WARMUP_FRAMES || samples.len() >= SAMPLE_FRAMES {
        return;
    }
    if samples.is_empty() {
        *rebuilds_at_start = MESH_REBUILDS.load(std::sync::atomic::Ordering::Relaxed);
    }
    samples.push(time.delta_secs() * 1000.0);
    if samples.len() == SAMPLE_FRAMES {
        let mut sorted = samples.clone();
        sorted.sort_by(|a, b| a.total_cmp(b));
        let avg = sorted.iter().sum::<f32>() / sorted.len() as f32;
        let p99 = sorted[(sorted.len() as f32 * 0.99) as usize - 1];
        let max = *sorted.last().unwrap();
        let line = format!(
            "frame_time cam={} seed={} frames={} avg_ms={avg:.2} p99_ms={p99:.2} max_ms={max:.2} chunk_rebuilds={}",
            cli.cam,
            cli.seed,
            sorted.len(),
            MESH_REBUILDS.load(std::sync::atomic::Ordering::Relaxed) - *rebuilds_at_start
        );
        info!("{line}");
        if let Err(e) = std::fs::write(&path, format!("{line}\n")) {
            error!("cannot write frame log {path}: {e}");
        }
        if cli.screenshot.is_none() {
            exit.write(AppExit::Success);
        }
    }
}

