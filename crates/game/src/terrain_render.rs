//! Soft, light-hearted terrain: chunked, vertex-coloured heightmap with seasonal palette, gentle
//! sun/ambient/sky, incremental updates and distance LOD.
//!
//! * The world is cut into `CHUNK`-cell chunks; each chunk is its own mesh entity.
//! * Every frame each chunk's input fields (height, water depth, vegetation) are hashed; only
//!   chunks whose hash (or LOD / season bucket) changed are rebuilt, at most `REBUILD_BUDGET` per frame.
//! * Chunks far from the camera use a coarser grid (LOD); small skirts hide LOD cracks.
//! * `Season` (resource) is derived from `SimState::time_days`; `Palette` turns it into colours.
//!
//! Debug env vars (all optional): `BEAVER_SEASON=spring|summer|autumn|winter|0..1` pins the season,
//! `BEAVER_FRAMELOG=<file>` measures frame time (vsync off) and writes the result.
use crate::camera::MainCamera;
use crate::state::Sim;
use bevy::app::AppExit;
use bevy::asset::RenderAssetUsages;
use bevy::mesh::Indices;
use bevy::prelude::*;
use bevy::render::render_resource::PrimitiveTopology;
use bevy::window::PresentMode;
use sim::grid::HeightGrid;
use sim::SimState;

/// World units between neighbouring grid cells.
pub const CELL_SIZE: f32 = 1.0;
/// Cells per chunk edge.
pub const CHUNK: usize = 16;
/// Simulated days per in-game year.
pub const YEAR_DAYS: f64 = 120.0;
/// Chunk rebuilds allowed per frame (the first build is unlimited).
const REBUILD_BUDGET: usize = 10;
/// Camera distance beyond which a chunk uses 2x coarser cells.
const LOD1_DISTANCE: f32 = 150.0;
/// Depth below which a cell counts as dry.
pub const WET_EPS: f32 = 0.02;

pub struct TerrainRenderPlugin;

/// Marks every terrain chunk entity (used e.g. by screenshot mode to know when it is drawn).
#[derive(Component)]
pub struct Terrain;

/// Position in the year: 0..1, spring is centred on 0.125, summer 0.375, autumn 0.625, winter 0.875.
#[derive(Resource, Clone, Copy, Debug)]
pub struct Season {
    pub phase: f32,
}

impl Season {
    /// Bucketed phase so the world is only recoloured in small steps.
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
        app.insert_resource(Season { phase: start })
            .init_resource::<TerrainChunks>()
            .add_systems(Startup, spawn_terrain)
            .add_systems(Update, (update_season, update_sky_and_light, update_terrain_chunks).chain())
            .add_systems(Update, frame_log);
    }
}

// ───────────────────────────── palette ─────────────────────────────

type Rgb = [f32; 3];

fn mix(a: Rgb, b: Rgb, t: f32) -> Rgb {
    let t = t.clamp(0.0, 1.0);
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t]
}
fn smooth(e0: f32, e1: f32, x: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}
/// sRGB triple to linear RGBA (vertex colours are linear).
pub fn lin(c: Rgb, a: f32) -> [f32; 4] {
    let l = Color::srgb(c[0], c[1], c[2]).to_linear();
    [l.red, l.green, l.blue, a]
}

/// All season-dependent colours, in sRGB.
#[derive(Clone, Copy, Debug)]
pub struct Palette {
    pub grass: Rgb,
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
    /// 0..1: how wintry (snow cover, ice).
    pub winter: f32,
}

// Keyframes: spring, summer, autumn, winter.
const GRASS: [Rgb; 4] = [[0.60, 0.82, 0.40], [0.45, 0.74, 0.34], [0.84, 0.66, 0.32], [0.66, 0.72, 0.60]];
const WETLAND: [Rgb; 4] = [[0.42, 0.68, 0.50], [0.36, 0.62, 0.46], [0.60, 0.58, 0.34], [0.52, 0.60, 0.56]];
const SKY: [Rgb; 4] = [[0.62, 0.82, 0.96], [0.52, 0.76, 0.96], [0.80, 0.80, 0.88], [0.78, 0.84, 0.92]];
const SUN: [Rgb; 4] = [[1.0, 0.96, 0.88], [1.0, 0.95, 0.84], [1.0, 0.86, 0.70], [0.88, 0.92, 1.0]];
const SUN_LUX: [f32; 4] = [8_500.0, 9_500.0, 7_500.0, 6_500.0];
const AMBIENT: [Rgb; 4] = [[0.80, 0.90, 1.0], [0.82, 0.90, 1.0], [0.95, 0.88, 0.90], [0.84, 0.90, 1.0]];
const WATER_SHALLOW: [Rgb; 4] = [[0.30, 0.80, 0.92], [0.26, 0.80, 0.94], [0.38, 0.72, 0.80], [0.52, 0.74, 0.88]];
const WATER_DEEP: [Rgb; 4] = [[0.08, 0.50, 0.70], [0.06, 0.50, 0.72], [0.16, 0.44, 0.56], [0.24, 0.44, 0.62]];

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
        // Winter peaks at 0.875 and fades over ~0.17 of a year either side (snow lingers a bit).
        let d = (phase.rem_euclid(1.0) - 0.875).abs();
        let d = d.min(1.0 - d);
        Palette {
            grass: c(&GRASS),
            mud: [0.62, 0.50, 0.38],
            rock: [0.64, 0.62, 0.64],
            wetland: c(&WETLAND),
            snow: [0.97, 0.98, 1.0],
            sky: c(&SKY),
            sun: c(&SUN),
            sun_lux: cyc(&SUN_LUX, phase, |a, b, t| a + (b - a) * t),
            ambient: c(&AMBIENT),
            ambient_lux: 300.0,
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
    // Only publish changes in coarse steps so change detection stays meaningful.
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

// ───────────────────────────── field access / hashing ─────────────────────────────

/// Reads a grid with coordinates clamped into range (works for ±1 borders).
pub fn at(g: &sim::Grid<f32>, x: i32, y: i32) -> f32 {
    let x = x.clamp(0, g.width() as i32 - 1) as usize;
    let y = y.clamp(0, g.height() as i32 - 1) as usize;
    g.as_slice()[y * g.width() + x]
}

/// Hash of every field value in the chunk region padded by `pad` cells, mixed with `extra`.
pub fn region_hash(s: &SimState, cx: usize, cy: usize, pad: i32, extra: u64, with_veg: bool) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325 ^ extra.wrapping_mul(0x9E37_79B9_7F4A_7C15);
    let (x0, y0) = ((cx * CHUNK) as i32 - pad, (cy * CHUNK) as i32 - pad);
    let n = CHUNK as i32 + 2 * pad;
    for y in y0..=y0 + n {
        for x in x0..=x0 + n {
            // depth quantised to 1/64 unit: rain/evaporation drift far below what is visible must
            // not rebuild every wet chunk every frame
            let dq = (at(&s.water.depth, x, y) * 64.0).round() as i32;
            let mut v = at(&s.height, x, y).to_bits() as u64 ^ ((dq as u32 as u64) << 32);
            if with_veg {
                let vq = (at(&s.ecology.vegetation, x, y) * 64.0).round() as i32;
                v ^= (vq as u32 as u64).rotate_left(17);
            }
            h = (h ^ v).wrapping_mul(0x100_0000_01b3);
            h ^= h >> 29;
        }
    }
    h
}

/// LOD step (cells per vertex) for a chunk at `centre` seen from `cam`.
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

fn noise(x: i32, y: i32) -> f32 {
    let mut h = (x as u32).wrapping_mul(0x85eb_ca6b) ^ (y as u32).wrapping_mul(0xc2b2_ae35);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2c1b_3c6d);
    h ^= h >> 12;
    (h & 0xffff) as f32 / 65535.0 * 2.0 - 1.0
}

// ───────────────────────────── terrain meshing ─────────────────────────────

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
    material: Handle<StandardMaterial>,
}

pub fn spawn_terrain(
    mut commands: Commands,
    sim: Res<Sim>,
    mut chunks: ResMut<TerrainChunks>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    season: Res<Season>,
    cam: Query<&Transform, With<MainCamera>>,
) {
    chunks.material = materials.add(StandardMaterial {
        perceptual_roughness: 1.0,
        reflectance: 0.05,
        ..default()
    });
    // Underside of the world slab. Always drawn (no frustum culling), so it doubles as the
    // `Terrain` marker screenshot mode waits on even when the camera is zoomed into one corner.
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
    let mesh = build_chunk_mesh(s, &Palette::at(season.phase), cx, cy, step);
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

fn update_terrain_chunks(
    mut commands: Commands,
    sim: Res<Sim>,
    season: Res<Season>,
    mut chunks: ResMut<TerrainChunks>,
    mut meshes: ResMut<Assets<Mesh>>,
    cam: Query<&Transform, With<MainCamera>>,
) {
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
}

/// Colour of one terrain vertex.
fn vertex_color(s: &SimState, p: &Palette, maxh: f32, x: i32, y: i32) -> Rgb {
    let h = at(&s.height, x, y);
    let t = h / maxh;
    let depth = at(&s.water.depth, x, y);
    let slope = {
        let dx = at(&s.height, x + 1, y) - at(&s.height, x - 1, y);
        let dz = at(&s.height, x, y + 1) - at(&s.height, x, y - 1);
        (dx * dx + dz * dz).sqrt() / (2.0 * CELL_SIZE)
    };
    let veg = at(&s.ecology.vegetation, x, y).clamp(0.0, 1.0);
    // Share of wet cells within 2 cells: drives wetland and the muddy shore band.
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

    let lush = [p.grass[0] * 0.9, p.grass[1] * 1.04, p.grass[2] * 0.9];
    let mut c = mix(p.grass, lush, veg);
    // soft per-vertex variation so slopes read as gentle patches rather than flat colour
    let n = noise(x, y) * 0.035 + noise(x / 4 + 91, y / 4 - 7) * 0.05;
    c = [c[0] * (1.0 + n), c[1] * (1.0 + n), c[2] * (1.0 + n)];
    // rock: steep or high
    let rock = (smooth(0.5, 1.0, slope) * smooth(0.3, 0.6, t)).max(smooth(0.72, 0.9, t));
    c = mix(c, p.rock, rock * 0.8);
    // wetland on low ground near water
    let wetland = smooth(0.05, 0.3, wet2) * (1.0 - smooth(0.4, 0.6, t));
    c = mix(c, p.wetland, wetland * (1.0 - rock));
    // muddy bank
    c = mix(c, p.mud, smooth(0.4, 1.0, wet1) * 0.8);
    // river/lake bed: warm sandy-mud in the shallows, cooling to a soft blue-grey when deep, so
    // water reads as water from far away while beavers and lodges still stand out against it
    // (a warm bed under cyan water would cancel to grey, so it cools quickly with depth)
    let bed = mix([0.62, 0.60, 0.46], [0.20, 0.40, 0.54], (depth / 3.0).max(0.0).sqrt().min(1.0));
    c = mix(c, bed, submerged);
    // snow: seasonal blanket above the waterline + permanent caps
    let cap = smooth(0.8, 0.95, t);
    let cover = (p.winter * smooth(0.0, 0.2, 1.0 - slope * 0.5) * (1.0 - 0.6 * smooth(0.5, 1.2, slope))).max(cap * 0.9);
    c = mix(c, p.snow, cover * (1.0 - submerged));
    c
}

/// Builds the mesh of one chunk at `step` cells per vertex, with skirts on its border.
pub fn build_chunk_mesh(s: &SimState, p: &Palette, cx: usize, cy: usize, step: usize) -> Mesh {
    let maxh = max_height(s);
    let (w, d) = (s.height.width() as i32, s.height.height() as i32);
    let n = CHUNK / step;
    let (x0, y0) = ((cx * CHUNK) as i32, (cy * CHUNK) as i32);
    let st = step as i32;
    let (mut pos, mut nor, mut col, mut uv) = (vec![], vec![], vec![], vec![]);
    let mut vert = |x: i32, y: i32, drop: f32, shade: f32| {
        let (wx, wz) = world_pos(s, x, y);
        let h = at(&s.height, x, y);
        pos.push([wx, h - drop, wz]);
        let dx = at(&s.height, x + st, y) - at(&s.height, x - st, y);
        let dz = at(&s.height, x, y + st) - at(&s.height, x, y - st);
        nor.push(Vec3::new(-dx, 2.0 * CELL_SIZE * st as f32, -dz).normalize().to_array());
        let c = vertex_color(s, p, maxh, x, y);
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
    // Skirts: duplicate each border vertex lower down and join with a strip.
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
    Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, pos)
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, nor)
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uv)
        .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, col)
        .with_inserted_indices(Indices::U32(idx))
}

// ───────────────────────────── frame-time logging ─────────────────────────────

const WARMUP_FRAMES: usize = 60;

/// Terrain + water chunk meshes rebuilt so far (reported by the frame log).
pub static MESH_REBUILDS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
const SAMPLE_FRAMES: usize = 300;

/// With `BEAVER_FRAMELOG=<file>`: turns vsync off, samples frame time after warm-up, logs and
/// writes avg / p99 / max in ms to the file, then exits (unless screenshot mode is active).
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

fn legacy_height_color(t: f32) -> [f32; 4] {
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


/// Full-resolution single mesh of a height field (height-tinted vertex colours), used by the fog overlay in `flow_view`.
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
            col.push(legacy_height_color(ht / max_height));
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

