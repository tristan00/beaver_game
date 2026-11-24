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
/// A chunk of terrain or water is one of the simulation's tiles, so the stamp the simulation
/// keeps for each tile of a grid says whether the chunk's cells were written.
pub const CHUNK: usize = sim::grid::TILE;
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

impl Plugin for TerrainRenderPlugin {
    fn build(&self, app: &mut App) {
        let start = crate::seasonal::phase(app.world().resource::<Sim>().0.time_days);
        bevy::asset::embedded_asset!(app, "terrain_shadow.wesl");
        app.add_plugins(MaterialPlugin::<TerrainMaterial>::default());
        app.insert_resource(Season { phase: start })
            .init_resource::<TerrainChunks>()
            .init_resource::<MeshBacklog>()
            .add_systems(Startup, spawn_terrain)
            .add_systems(Update, (update_season, update_sky_and_light, update_terrain_chunks).chain().after(crate::sim_driver::step_sim))
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
    pub ice: Rgb,
    pub winter: f32,
    pub autumn: f32,
    pub bloom: f32,
}

const GRASS_LIGHT: [Rgb; 4] = [rgb(0xAFD97A), rgb(0x91C66B), rgb(0xD7BC70), rgb(0xA7B397)];
const GRASS: [Rgb; 4] = [rgb(0x82BE64), rgb(0x69A65A), rgb(0xB39A59), rgb(0x788F7B)];
const GRASS_DEEP: [Rgb; 4] = [rgb(0x519B58), rgb(0x408568), rgb(0x827F48), rgb(0x476E69)];
const WETLAND: [Rgb; 4] = [rgb(0x64AD82), rgb(0x4B997E), rgb(0x9C9D59), rgb(0x527E7F)];
const SKY: [Rgb; 4] = [rgb(0xA7D9ED), rgb(0x84C6ED), rgb(0xC5D1D9), rgb(0xAAC6DB)];
const SUN: [Rgb; 4] = [pal::SUN, rgb(0xFFF2CC), pal::SUN_AUTUMN, rgb(0xE5EEFF)];
const SUN_LUX: [f32; 4] = [1_300.0, 1_400.0, 1_250.0, 1_000.0];
const AMBIENT: [Rgb; 4] = [[0.98, 0.95, 0.99], [0.98, 0.95, 0.99], [1.0, 0.92, 0.93], [0.96, 0.96, 0.98]];
pub const AMBIENT_LUX: f32 = 4_400.0;

use crate::seasonal::blend as cyc;

impl Palette {
    pub fn at(phase: f32) -> Palette {
        let c = |k: &[Rgb; 4]| cyc(k, phase, mix);
        Palette {
            grass: c(&GRASS),
            grass_light: c(&GRASS_LIGHT),
            grass_deep: c(&GRASS_DEEP),
            mud: pal::MUD,
            rock: pal::STONE,
            wetland: c(&WETLAND),
            snow: [0.80, 0.86, 0.89],
            sky: c(&SKY),
            sun: c(&SUN),
            sun_lux: cyc(&SUN_LUX, phase, |a, b, t| a + (b - a) * t),
            ambient: c(&AMBIENT),
            ambient_lux: AMBIENT_LUX,
            ice: [0.43, 0.68, 0.79],
            winter: crate::seasonal::snow(phase),
            autumn: crate::seasonal::autumn(phase),
            bloom: crate::seasonal::bloom(phase),
        }
    }
}

pub(crate) fn update_season(sim: Res<Sim>, mut season: ResMut<Season>) {
    let phase = crate::seasonal::phase(sim.0.time_days);
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
            // Terrain colour needs depth only to a few centimetres (wet ground, submerged bed);
            // water meshes follow it to 2 mm. These are the steps the simulation stamps its
            // tiles by (`ground_stamp`): a change it does not stamp must not change the hash.
            let d = at(&s.water.depth, x, y);
            let dq = if !with_veg {
                (d * 500.0).round() as i32
            } else if map_lens() == WATER_LENS {
                // The water lens colours water by band (`water_band`).
                sim::water::shore_step(d) as i32 | (water_band(d) as i32) << 4
            } else {
                sim::water::shore_step(d) as i32
            };
            let mut v = at(&s.height, x, y).to_bits() as u64 ^ ((dq as u32 as u64) << 32);
            v ^= (at(&s.water.dam_height, x, y).to_bits() as u64).rotate_left(9);
            if with_veg {
                // Ground this deep under water is drawn as bed alone (`world_color`).
                let vq = if d >= 0.03 { 0 } else { (at(&s.ecology.vegetation, x, y) * sim::ecology::COVER_STEPS).round() as i32 };
                v ^= (vq as u32 as u64).rotate_left(17);
                let bq = (at(&s.water.riverbed, x, y) * sim::water::RIVERBED_STEPS).round() as u64;
                v ^= bq.rotate_left(41);
            } else {
                let ix = x.clamp(0, s.height.width() as i32 - 1) as usize;
                let iy = y.clamp(0, s.height.height() as i32 - 1) as usize;
                let (vx, vy) = *s.water.velocity.get(ix, iy).unwrap();
                v ^= ((vx * 20.0).round() as i32 as u64).rotate_left(13);
                v ^= ((vy * 20.0).round() as i32 as u64).rotate_left(37);
                v ^= ((at(&s.water.ice, x, y) * 100.0).round() as u64).rotate_left(51);
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

/// Water, a riverbed, a canal or a mapped river in the chunk or within two cells of it. Such
/// a chunk is never drawn coarsely: a one-cell stream or dug canal would be smoothed into its
/// banks. The halo also keeps the terrain on both sides of a chunk seam at the same resolution.
pub fn wet_ground(s: &SimState, cx: usize, cy: usize) -> bool {
    for y in (cy * CHUNK).saturating_sub(2)..((cy + 1) * CHUNK + 3).min(s.height.height()) {
        for x in (cx * CHUNK).saturating_sub(2)..((cx + 1) * CHUNK + 3).min(s.height.width()) {
            let i = y * s.height.width() + x;
            // In the steps the simulation stamps tiles by: water on the cell, a bed part bare.
            let bed = (s.water.riverbed.as_slice()[i] * sim::water::RIVERBED_STEPS).round() >= 2.0;
            if sim::water::shore_step(s.water.depth.as_slice()[i]) > 0 || bed || s.water.canal.as_slice()[i] || s.world.river.as_slice()[i] {
                return true;
            }
        }
    }
    false
}

fn fold(h: u64, v: u64) -> u64 {
    let h = (h ^ v).wrapping_mul(0x9E37_79B9_7F4A_7C15);
    h ^ h >> 29
}

/// One number for the stamps of several grids at chunk (cx, cy): it changes whenever the
/// simulation has written a cell of any of them in the chunk or within `sim::grid::REACH`
/// cells of it (or the grid is another).
macro_rules! stamp_near {
    ($cx:expr, $cy:expr, $($g:expr),+) => {{
        let mut h = 0xcbf2_9ce4_8422_2325u64;
        $( h = fold(fold(h, $g.origin()), $g.tile_stamp($cx, $cy)); )+
        h
    }};
}

/// The lens and the season's step a chunk is drawn for.
pub(crate) type View = (u8, u32);

/// Changes when anything the ground of chunk (cx, cy) is drawn from may have: the cells
/// `region_hash` reads for terrain, and those `wet_ground` reads. Of the water only its edge
/// counts (`Water::shore`), so a pond rising and falling over its bed changes nothing here,
/// except in the water lens, which colours water by its depth.
fn ground_stamp(s: &SimState, cx: usize, cy: usize) -> u64 {
    let h = stamp_near!(cx, cy, s.height, s.water.dam_height, s.water.shore, s.ecology.vegetation, s.water.riverbed, s.water.canal);
    if map_lens() == WATER_LENS { fold(h, stamp_near!(cx, cy, s.water.depth)) } else { h }
}

/// The same for the whole map: while it stands, no chunk's `ground_stamp` has moved.
fn ground_stamps(s: &SimState) -> [(u64, u64); 7] {
    let depth = if map_lens() == WATER_LENS { s.water.depth.stamp() } else { (0, 0) };
    [s.height.stamp(), s.water.dam_height.stamp(), s.water.shore.stamp(), s.ecology.vegetation.stamp(), s.water.riverbed.stamp(), s.water.canal.stamp(), depth]
}

/// As `ground_stamp`, for the water's surface.
pub(crate) fn water_stamp(s: &SimState, cx: usize, cy: usize) -> u64 {
    stamp_near!(cx, cy, s.height, s.water.dam_height, s.water.depth, s.water.velocity, s.water.ice, s.water.shore, s.water.riverbed, s.water.canal)
}

pub(crate) fn water_stamps(s: &SimState) -> [(u64, u64); 8] {
    [s.height.stamp(), s.water.dam_height.stamp(), s.water.depth.stamp(), s.water.velocity.stamp(), s.water.ice.stamp(), s.water.shore.stamp(), s.water.riverbed.stamp(), s.water.canal.stamp()]
}

pub fn chunk_centre(s: &SimState, cx: usize, cy: usize) -> Vec3 {
    let (w, d) = (s.height.width() as f32, s.height.height() as f32);
    Vec3::new(
        (cx * CHUNK) as f32 + CHUNK as f32 / 2.0 + GRID_OFFSET - w / 2.0,
        8.0,
        (cy * CHUNK) as f32 + CHUNK as f32 / 2.0 + GRID_OFFSET - d / 2.0,
    ) * Vec3::new(CELL_SIZE, 1.0, CELL_SIZE)
}

/// Grid value (x, y) is drawn at the centre of cell (x, y), where that cell's beavers, plants
/// and buildings stand: world x = (x + GRID_OFFSET - width / 2) * CELL_SIZE.
pub const GRID_OFFSET: f32 = 0.5;

pub fn world_pos(s: &SimState, x: i32, y: i32) -> (f32, f32) {
    (
        (x as f32 + GRID_OFFSET - s.height.width() as f32 / 2.0) * CELL_SIZE,
        (y as f32 + GRID_OFFSET - s.height.height() as f32 / 2.0) * CELL_SIZE,
    )
}

/// The drawn ground on the map's river cells sits this far below the simulated ground, so a
/// creek a few centimetres deep reads as water in a channel with banks. Display only: the water
/// level, volumes and walking heights come from the simulation.
pub const CHANNEL_CUT_M: f32 = 0.3;

fn river_cell(s: &SimState, x: i32, y: i32) -> f32 {
    let (w, h) = (s.height.width() as i32, s.height.height() as i32);
    if x < 0 || y < 0 || x >= w || y >= h {
        return 0.0;
    }
    let i = (y * w + x) as usize;
    (s.world.river.as_slice()[i] && !s.water.canal.as_slice()[i] && s.water.dam_height.as_slice()[i] <= 0.0) as i32 as f32
}

/// The river mask softened over each cell's four neighbours, so the channel's outline is a
/// smooth band along the cells instead of their square staircase.
fn river_mask(s: &SimState, x: i32, y: i32) -> f32 {
    0.4 * river_cell(s, x, y) + 0.15 * (river_cell(s, x - 1, y) + river_cell(s, x + 1, y) + river_cell(s, x, y - 1) + river_cell(s, x, y + 1))
}

fn cut_of_mask(m: f32) -> f32 {
    CHANNEL_CUT_M * smooth(0.25, 0.7, m)
}

pub fn channel_cut(s: &SimState, x: i32, y: i32) -> f32 {
    cut_of_mask(river_mask(s, x, y))
}

pub fn bilinear(f: impl Fn(i32, i32) -> f32, x: f32, y: f32) -> f32 {
    let (ix, iy) = (x.floor() as i32, y.floor() as i32);
    let (u, v) = (x - ix as f32, y - iy as f32);
    let top = f(ix, iy) + (f(ix + 1, iy) - f(ix, iy)) * u;
    let bot = f(ix, iy + 1) + (f(ix + 1, iy + 1) - f(ix, iy + 1)) * u;
    top + (bot - top) * v
}

/// The drawn ground at grid value (x, y): simulated height less the channel cut.
pub fn ground_at(s: &SimState, x: i32, y: i32) -> f32 {
    at(&s.height, x, y) - channel_cut(s, x, y)
}

/// Which diagonal splits a grid square with corners a (x, y), b (x+1, y), c (x, y+1),
/// e (x+1, y+1): true joins a-e, false joins b-c. The lower pair is joined so valleys and
/// creeks run along triangle edges instead of being cut into a chain of diamonds.
pub fn split_ae(a: f32, b: f32, c: f32, e: f32) -> bool {
    a + e < b + c - 1e-4
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

/// The water lens (`MapLens::Water`): the water taken away to show its depth in bands, the
/// shallows barges cannot use apart, and water near or past the depth that floods buildings
/// in colours of their own; currents, the bed's deposits and its depth lines are drawn over it
/// (hud::world).
pub const WATER_LENS: u8 = 1;
pub const ELEVATION_LENS: u8 = 4;

pub fn elevation_ramp(t: f32) -> Rgb {
    const STOPS: [Rgb; 4] = [[0.25, 0.36, 0.39], [0.45, 0.57, 0.47], [0.76, 0.71, 0.53], [0.96, 0.92, 0.80]];
    let t = t.clamp(0.0, 1.0) * (STOPS.len() - 1) as f32;
    let i = (t.floor() as usize).min(STOPS.len() - 2);
    mix(STOPS[i], STOPS[i + 1], t - i as f32)
}

pub const ELEVATION_WATER: Rgb = [0.31, 0.56, 0.86];

pub const DEPOSIT_LENS: u8 = 5;
/// The shoreline drawn over the water lens: water deep enough to count as water (`WET_DEPTH`).
pub const SHORE_DEPTH: f32 = sim::buildings::WET_DEPTH;
/// The thinnest film still drawn as bed rather than land.
pub const BED_FILM: f32 = 0.003;

/// Deposits the current lens tints into the ground: all of them in the deposits lens.
fn lens_deposits(s: &SimState) -> impl Iterator<Item = &sim::resources::Resource> {
    let on = map_lens() == DEPOSIT_LENS;
    shown_deposits(s).filter(move |_| on)
}

/// A deposit worth outlining on the bed in the water lens: water plants, clay and pebbles
/// that lie under water.
pub fn underwater_deposit(s: &SimState, r: &sim::resources::Resource) -> bool {
    use sim::resources::ResourceKind::*;
    matches!(r.kind, WaterPlant | RiverClay | MudBank | Pebbles) && at(&s.water.depth, r.cell.0 as i32, r.cell.1 as i32) > BED_FILM
}

/// The deposits lens: one saturated colour per kind, none grey or blue, so they stand
/// apart from the lens's grey land and blue-grey water.
pub fn deposit_color(k: sim::resources::ResourceKind) -> Rgb {
    use sim::resources::ResourceKind::*;
    match k {
        Tree => [0.10, 0.42, 0.16],
        Felled => [0.92, 0.16, 0.12],
        Plant => [0.62, 0.90, 0.16],
        MudBank => [0.80, 0.46, 0.14],
        RiverClay => [0.86, 0.22, 0.70],
        Pebbles => [1.0, 0.90, 0.10],
        WaterPlant => [1.0, 1.0, 1.0],
    }
}

/// Deposits that still hold something, as the deposits lens shows them.
pub fn shown_deposits(s: &SimState) -> impl Iterator<Item = &sim::resources::Resource> {
    s.resources.items().iter().filter(|r| r.has_yield())
}

/// Per-chunk fingerprint of the deposits in it (empty outside the deposits lens), so a
/// chunk recolours when one appears, runs out or changes kind.
fn deposit_hashes(s: &SimState, cols: usize, rows: usize) -> Vec<u64> {
    if map_lens() != DEPOSIT_LENS {
        return Vec::new();
    }
    let mut out = vec![0u64; cols * rows];
    for r in lens_deposits(s) {
        let v = (r.cell.0 as u64) | (r.cell.1 as u64) << 20 | (r.kind as u64) << 40;
        let v = v.wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ v >> 7;
        // A cell on a chunk's low edge is also the last colour sample of the chunk before it.
        let span = |c: usize, n: usize| {
            let k = (c / CHUNK).min(n - 1);
            (if c % CHUNK == 0 && k > 0 { k - 1 } else { k })..=k
        };
        for cy in span(r.cell.1, rows) {
            for cx in span(r.cell.0, cols) {
                out[cy * cols + cx] = out[cy * cols + cx].wrapping_add(v);
            }
        }
    }
    out
}

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
    /// The deposits tinted into it (`deposit_hashes`).
    deposits: u64,
    /// The chunk's `ground_stamp` and the view when its cells were last hashed: until either
    /// changes, `hash` stands and the cells are not read again.
    seen: Option<(u64, View)>,
    /// `wet_ground`, as of `seen`.
    wet: bool,
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
    let range = terrain_range(&sim.0);
    let deposits = deposit_hashes(&sim.0, cols, rows);
    let view = (map_lens(), season.bucket());
    for idx in 0..cols * rows {
        let (cx, cy) = (idx % cols, idx / cols);
        let slot = &mut chunks.slots[idx];
        slot.wet = wet_ground(&sim.0, cx, cy);
        slot.step = if slot.wet { 1 } else { lod_step(campos, chunk_centre(&sim.0, cx, cy)) };
        slot.hash = region_hash(&sim.0, cx, cy, 2, season.bucket() as u64, true);
        slot.deposits = deposits.get(idx).copied().unwrap_or(0);
        slot.seen = Some((ground_stamp(&sim.0, cx, cy), view));
        build_chunk(&mut commands, &mut chunks, &mut meshes, &sim.0, &season, cx, cy, cols, range);
    }
}

/// Make chunk (cx, cy)'s mesh afresh, at the detail its slot asks for.
#[allow(clippy::too_many_arguments)]
fn build_chunk(
    commands: &mut Commands,
    chunks: &mut TerrainChunks,
    meshes: &mut Assets<Mesh>,
    s: &SimState,
    season: &Season,
    cx: usize,
    cy: usize,
    cols: usize,
    range: (f32, f32),
) {
    let idx = cy * cols + cx;
    MESH_REBUILDS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let built = std::time::Instant::now();
    let mesh = build_chunk_mesh(s, &Palette::at(season.phase), cx, cy, chunks.slots[idx].step, range);
    crate::perf::TERRAIN_MESH_NS.fetch_add(built.elapsed().as_nanos() as u64, std::sync::atomic::Ordering::Relaxed);
    let material = chunks.material.clone();
    let slot = &mut chunks.slots[idx];
    match slot.entity {
        Some(_) => {
            let _ = meshes.insert(&slot.mesh, mesh);
        }
        None => {
            slot.mesh = meshes.add(mesh);
            slot.entity = Some(commands.spawn((Mesh3d(slot.mesh.clone()), MeshMaterial3d(material), Transform::default())).id());
        }
    }
}

#[derive(Resource, Default)]
pub struct MeshBacklog {
    pub terrain: bool,
    pub water: bool,
}

/// Rebuild the chunks whose ground changed. The simulation stamps each tile of a grid it
/// writes to, so only chunks with a new stamp have their cells read (and hashed, to see
/// whether what is drawn of them changed); the rest cost a comparison.
#[allow(clippy::too_many_arguments)]
fn update_terrain_chunks(
    mut commands: Commands,
    sim: Res<Sim>,
    season: Res<Season>,
    mut chunks: ResMut<TerrainChunks>,
    mut meshes: ResMut<Assets<Mesh>>,
    cam: Query<&Transform, With<MainCamera>>,
    mut backlog: ResMut<MeshBacklog>,
    mut cursor: Local<usize>,
    // The grids' stamps when every chunk was last up to date.
    mut settled: Local<Option<[(u64, u64); 7]>>,
) {
    let _p = ::sim::prof::scope("game/update terrain chunks");
    backlog.terrain = true;
    let Some(campos) = cam.iter().next().map(|t| t.translation) else { return };
    let (cols, rows) = chunk_dims(&sim.0);
    if chunks.slots.len() != cols * rows {
        return;
    }
    let s = &sim.0;
    let view = (map_lens(), season.bucket());
    let stamps = ground_stamps(s);
    // Nothing written since every chunk's stamp was read: they need not be read again.
    let still = *settled == Some(stamps);
    let mut budget = REBUILD_BUDGET;
    let total = cols * rows;
    let mut range = None;
    let deposits = deposit_hashes(s, cols, rows);
    for k in 0..total {
        let idx = (*cursor + k) % total;
        let (cx, cy) = (idx % cols, idx / cols);
        let slot = &mut chunks.slots[idx];
        let stamp = match slot.seen {
            Some((stamp, _)) if still => stamp,
            _ => ground_stamp(s, cx, cy),
        };
        if slot.seen.map(|seen| seen.0) != Some(stamp) {
            slot.wet = wet_ground(s, cx, cy);
        }
        let step = if slot.wet { 1 } else { lod_step(campos, chunk_centre(s, cx, cy)) };
        let dep = deposits.get(idx).copied().unwrap_or(0);
        let known = slot.seen == Some((stamp, view));
        if known && slot.step == step && slot.deposits == dep {
            continue;
        }
        let hash = if known { slot.hash } else { region_hash(s, cx, cy, 2, season.bucket() as u64, true) };
        if slot.entity.is_some() && slot.hash == hash && slot.step == step && slot.deposits == dep {
            slot.seen = Some((stamp, view));
            continue;
        }
        if budget == 0 {
            *cursor = idx;
            return;
        }
        budget -= 1;
        (slot.hash, slot.step, slot.deposits, slot.seen) = (hash, step, dep, Some((stamp, view)));
        let range = *range.get_or_insert_with(|| terrain_range(s));
        build_chunk(&mut commands, &mut chunks, &mut meshes, s, &season, cx, cy, cols, range);
    }
    *settled = Some(stamps);
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
        WATER_LENS => water_lens_color(s, maxh, x, y),
        _ => lens_color(s, maxh, x, y),
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

/// Depth bands of the water lens, from shallow to deep: each band's deepest edge and colour.
/// Water shallower than barges need is a pale sand-green; water within `FLOOD_NEAR` of the
/// depth that floods buildings (`MAX_BUILDING_DEPTH`) is amber, and from there on red.
pub const BARGE_SHALLOWS: Rgb = [0.78, 0.86, 0.70];
pub const DEPTH_BANDS: [(f32, Rgb); 2] = [(0.5, [0.55, 0.78, 0.93]), (f32::INFINITY, [0.30, 0.56, 0.86])];
pub const FLOOD_RISK: Rgb = [0.96, 0.62, 0.18];
pub const FLOODED: Rgb = [0.86, 0.20, 0.24];
/// How far short of flooding buildings water counts as a risk (m).
pub const FLOOD_NEAR: f32 = 0.2;

/// Depth (m) from which water is drawn as a flood risk, and from which it floods buildings.
pub fn flood_depths() -> (f32, f32) {
    let max = sim::buildings::MAX_BUILDING_DEPTH;
    ((max - FLOOD_NEAR).max(0.0), max)
}

/// The water lens's band for a depth: 0 the barge shallows, then `DEPTH_BANDS`, then the flood
/// risk, then flooded.
pub fn water_band(depth: f32) -> u8 {
    let (near, max) = flood_depths();
    if depth >= max {
        return DEPTH_BANDS.len() as u8 + 2;
    }
    if depth >= near {
        return DEPTH_BANDS.len() as u8 + 1;
    }
    if depth < sim::harbor::BARGE_DEPTH {
        return 0;
    }
    1 + DEPTH_BANDS.iter().position(|b| depth < b.0).unwrap_or(DEPTH_BANDS.len() - 1) as u8
}

/// The colour of a `water_band`.
pub fn band_color(band: u8) -> Rgb {
    match band as usize {
        0 => BARGE_SHALLOWS,
        b if b <= DEPTH_BANDS.len() => DEPTH_BANDS[b - 1].1,
        b if b == DEPTH_BANDS.len() + 1 => FLOOD_RISK,
        _ => FLOODED,
    }
}

/// The water lens's colour for a point: water by its band (a lock is never the shallows'),
/// dry ground grey.
fn water_lens_color(s: &SimState, maxh: f32, x: i32, y: i32) -> Rgb {
    let ground = at(&s.height, x, y);
    let depth = at(&s.water.depth, x, y);
    let (gx, gz) = smooth_gradient(s, x, y, 1);
    let sunny = ((gx + gz) * -0.5).clamp(-0.5, 0.5);
    let shade = |c: Rgb, k: f32| [(c[0] * k).min(1.0), (c[1] * k).min(1.0), (c[2] * k).min(1.0)];
    let lock = sim::locks::is_lock(s, (x.max(0) as usize, y.max(0) as usize));
    if depth > WET_EPS || lock {
        let band = water_band(depth).max(lock as u8);
        return shade(band_color(band), 0.9 + 0.3 * sunny);
    }
    let t = (ground / maxh).clamp(0.0, 1.0);
    let g = (0.56 + 0.3 * t) * (0.86 + 0.28 * sunny);
    [g, g * 0.985, g * 0.955]
}

fn lens_color(s: &SimState, maxh: f32, x: i32, y: i32) -> Rgb {
    let t = (at(&s.height, x, y) / maxh).clamp(0.0, 1.0);
    let depth = at(&s.water.depth, x, y);
    let (gx, gz) = smooth_gradient(s, x, y, 1);
    let sunny = ((gx + gz) * -0.5).clamp(-0.5, 0.5);
    let g = (0.52 + 0.36 * t) * (0.86 + 0.28 * sunny);
    let land = [g, g * 0.985, g * 0.955];
    let wet = smooth(WET_EPS, 0.3, depth);
    mix(land, [0.62, 0.69, 0.75], wet)
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
    let submerged = smooth(0.003, 0.03, depth);

    let patch = patch_noise(x as f32, y as f32, 14.0, 1) * 0.55 + patch_noise(x as f32, y as f32, 6.0, 2) * 0.25;
    let sunny = ((gx + gz) * -0.5).clamp(-0.5, 0.5);
    let g = (GRADE_MID + patch * GRADE_PATCH + sunny * GRADE_SUN - veg * 0.12 - wet2 * 0.1).clamp(0.0, 1.0);
    let mut c = if g < 0.5 { mix(p.grass_deep, p.grass, g * 2.0) } else { mix(p.grass, p.grass_light, g * 2.0 - 1.0) };
    let rock = rock_weight(slope);
    let rock_c = mix(STONE_SHADE_ALBEDO, p.rock, (0.55 + sunny * 0.9 + patch * 0.3).clamp(0.0, 1.0));
    let soil = smooth(0.35, pal::ROCK_SLOPE_MIN, slope) * (1.0 - rock);
    c = mix(c, pal::MUD, soil * 0.55);
    let litter = smooth(0.15, 0.6, patch_noise(x as f32, y as f32, 5.0, 3) * 0.5 + 0.5) * p.autumn * veg.max(0.4);
    c = mix(c, pal::AUTUMN_RUST, litter * 0.35 * (1.0 - rock));
    c = mix(c, rock_c, rock);
    let wetland = smooth(0.12, 0.45, wet2) * 0.7 * (1.0 - smooth(0.4, 0.6, t));
    c = mix(c, p.wetland, wetland * (1.0 - rock));
    c = mix(c, p.mud, smooth(0.4, 1.0, wet1) * 0.4);
    let channel = stream_bed(s, x, y);
    c = mix(c, mix(STREAM_GRAVEL, STREAM_MUD, smooth(0.0, 0.3, depth)), channel);
    let bed = mix(STREAM_GRAVEL, STREAM_MUD, smooth(0.0, 0.35, depth));
    c = mix(c, bed, submerged);
    let dried = dried_bed(s, x, y) * (1.0 - submerged);
    if dried > 0.0 {
        let mottle = smooth(-0.35, 0.45, patch_noise(x as f32, y as f32, 2.5, 4));
        let silt = mix(rgb(0xA88A66), rgb(0xD8C6A2), mottle);
        c = mix(c, silt, dried * 0.9);
    }
    // Snow collects in irregular drifts instead of bleaching every dry tile.
    // Exposed rock, wet banks, and sunny slopes retain their underlying colour.
    let drift = smooth(-0.35, 0.4, patch_noise(x as f32, y as f32, 12.0, 11) * 0.7
        + patch_noise(x as f32, y as f32, 4.0, 12) * 0.3);
    let cover = p.winter * 0.72 * drift * (1.0 - rock * 0.7)
        * (1.0 - wetland * 0.45) * (1.0 - sunny * 0.4)
        * (1.0 - 0.6 * smooth(0.5, 1.2, slope));
    c = mix(c, p.snow, cover * (1.0 - submerged));
    let lo = mix(p.grass_deep, STONE_SHADE_ALBEDO, rock);
    let f = (1.0 - submerged) * (1.0 - channel) * (1.0 - dried);
    let lift = |v: f32, floor: f32| v + (v.max(floor * 1.05) - v) * f;
    [lift(c[0], lo[0]), lift(c[1], lo[1]), lift(c[2], lo[2])]
}

const STREAM_GRAVEL: Rgb = [0.55, 0.50, 0.42];
const STREAM_MUD: Rgb = [0.36, 0.31, 0.25];

fn stream_bed(s: &SimState, x: i32, y: i32) -> f32 {
    let mut bed = at(&s.water.riverbed, x, y).max(smooth(0.003, 0.03, at(&s.water.depth, x, y)));
    let ground = at(&s.height, x, y);
    for (dx, dy) in [(-1, 0), (1, 0), (0, -1), (0, 1)] {
        let wet = at(&s.water.depth, x + dx, y + dy);
        let level = at(&s.height, x + dx, y + dy) + wet;
        if wet > 0.003 && ground < level + 0.35 {
            bed = bed.max(at(&s.water.riverbed, x + dx, y + dy) * 0.65);
        }
    }
    bed
}

fn dried_bed(s: &SimState, x: i32, y: i32) -> f32 {
    if at(&s.water.depth, x, y) > WET_EPS {
        return 0.0;
    }
    at(&s.water.riverbed, x, y)
}

fn terrain_range(s: &SimState) -> (f32, f32) {
    (if map_lens() == ELEVATION_LENS { min_height(s) } else { 0.0 }, max_height(s))
}

/// How finely a chunk is drawn: canals 4 vertices per cell, chunks with a mapped river (or
/// one within two cells) RIVER_SUBDIVISIONS, so banks curve instead of following the cells'
/// staircase.
pub const RIVER_SUBDIVISIONS: usize = 2;
pub fn canal_subdivisions(s: &SimState, cx: usize, cy: usize) -> usize {
    let mut fine = 1;
    for y in (cy * CHUNK).saturating_sub(2)..((cy + 1) * CHUNK + 3).min(s.height.height()) {
        for x in (cx * CHUNK).saturating_sub(2)..((cx + 1) * CHUNK + 3).min(s.height.width()) {
            if *s.water.canal.get(x, y).unwrap() { return 4; }
            if *s.world.river.get(x, y).unwrap() { fine = RIVER_SUBDIVISIONS; }
        }
    }
    fine
}

pub fn terrain_sample(s: &SimState, x: f32, y: f32, step: usize) -> f32 {
    let st = step as f32;
    let (gx, gy) = ((x / st).floor(), (y / st).floor());
    let (u, v) = (x / st - gx, y / st - gy);
    let (ix, iy) = (gx as i32 * step as i32, gy as i32 * step as i32);
    let h = |dx: i32, dy: i32| ground_at(s, ix + dx * step as i32, iy + dy * step as i32);
    let (a, b, c, e) = (h(0, 0), h(1, 0), h(0, 1), h(1, 1));
    if split_ae(a, b, c, e) {
        if u >= v { a + u * (b - a) + v * (e - b) } else { a + v * (c - a) + u * (e - c) }
    } else if u + v <= 1.0 {
        a + u * (b - a) + v * (c - a)
    } else {
        e + (1.0 - u) * (c - e) + (1.0 - v) * (b - e)
    }
}

/// Grid values over one chunk and a margin, read once so a finely drawn chunk doesn't
/// recompute the river mask and heights for every vertex.
pub struct Field {
    x0: i32,
    y0: i32,
    n: i32,
    v: Vec<f32>,
}

impl Field {
    pub const PAD: i32 = 4;

    pub fn new(cx: usize, cy: usize, f: impl Fn(i32, i32) -> f32) -> Field {
        let (x0, y0, n) = ((cx * CHUNK) as i32 - Self::PAD, (cy * CHUNK) as i32 - Self::PAD, CHUNK as i32 + 2 * Self::PAD + 1);
        let mut v = Vec::with_capacity((n * n) as usize);
        for j in 0..n {
            for i in 0..n {
                v.push(f(x0 + i, y0 + j));
            }
        }
        Field { x0, y0, n, v }
    }

    pub fn get(&self, x: i32, y: i32) -> f32 {
        let (i, j) = ((x - self.x0).clamp(0, self.n - 1), (y - self.y0).clamp(0, self.n - 1));
        self.v[(j * self.n + i) as usize]
    }

    pub fn bilinear(&self, x: f32, y: f32) -> f32 {
        bilinear(|i, j| self.get(i, j), x, y)
    }
}

/// The drawn ground around one chunk: heights and the softened river mask.
pub struct GroundField {
    height: Field,
    mask: Field,
    canal: bool,
}

impl GroundField {
    pub fn new(s: &SimState, cx: usize, cy: usize) -> GroundField {
        GroundField { height: Field::new(cx, cy, |x, y| at(&s.height, x, y)), mask: Field::new(cx, cy, |x, y| river_mask(s, x, y)), canal: canal_subdivisions(s, cx, cy) == 4 }
    }

    /// `ground_at` from the cached values.
    pub fn at(&self, x: i32, y: i32) -> f32 {
        self.height.get(x, y) - cut_of_mask(self.mask.get(x, y))
    }

    /// `canal_ground` from the cached values.
    pub fn fine(&self, s: &SimState, x: f32, y: f32) -> f32 {
        let base = self.height.bilinear(x, y) - cut_of_mask(self.mask.bilinear(x, y));
        if self.canal { canal_blend(s, x, y, base) } else { base }
    }

    fn gradient(&self, x: i32, y: i32, st: i32) -> (f32, f32) {
        let h = |dx: i32, dy: i32| self.at(x + dx * st, y + dy * st);
        let gx = (h(1, -1) - h(-1, -1) + 2.0 * (h(1, 0) - h(-1, 0)) + h(1, 1) - h(-1, 1)) / 8.0;
        let gz = (h(-1, 1) - h(-1, -1) + 2.0 * (h(0, 1) - h(0, -1)) + h(1, 1) - h(1, -1)) / 8.0;
        (gx / (st as f32 * CELL_SIZE), gz / (st as f32 * CELL_SIZE))
    }
}

/// Give excavated cells a visible floor instead of a V-shaped point. This is a
/// local reconstruction of the rendered bed; simulation heights/volumes stay unchanged.
fn canal_blend(s: &SimState, x: f32, y: f32, base: f32) -> f32 {
    let (ix, iy) = (x.floor() as i32, y.floor() as i32);
    let (mut sum, mut weight) = (0.0, 0.0);
    for dy in 0..=1 { for dx in 0..=1 {
        let (cx, cy) = (ix + dx, iy + dy);
        if cx < 0 || cy < 0 || !s.water.canal.get(cx as usize, cy as usize).copied().unwrap_or(false) { continue; }
        let d = (x - cx as f32).abs().max((y - cy as f32).abs());
        let w = 1.0 - smooth(0.25, 0.75, d);
        sum += at(&s.height, cx, cy) * w;
        weight += w;
    } }
    if weight > 0.0 { base + (sum / weight - base) * weight.min(1.0) } else { base }
}

pub fn build_chunk_mesh(s: &SimState, p: &Palette, cx: usize, cy: usize, step: usize, range: (f32, f32)) -> Mesh {
    let maxh = range.1;
    let (w, d) = (s.height.width() as i32, s.height.height() as i32);
    let subdivisions = canal_subdivisions(s, cx, cy);
    let n = CHUNK / step * subdivisions;
    let spacing = step as f32 / subdivisions as f32;
    let (x0, y0) = ((cx * CHUNK) as i32, (cy * CHUNK) as i32);
    let st = step as i32;
    // Terrain colour is low frequency; interpolate its metre samples instead of
    // recomputing vegetation/noise for every refined canal vertex.
    let base_side = CHUNK / step + 1;
    let mut colours = Vec::with_capacity(base_side * base_side);
    for j in 0..base_side { for i in 0..base_side {
        colours.push(vertex_color(s, p, range, x0 + (i * step) as i32, y0 + (j * step) as i32));
    } }
    if map_lens() == DEPOSIT_LENS {
        let span = (CHUNK + step) as i32;
        for r in lens_deposits(s) {
            let (dx, dy) = (r.cell.0 as i32 - x0, r.cell.1 as i32 - y0);
            if dx < 0 || dy < 0 || dx >= span || dy >= span {
                continue;
            }
            let (i, j) = (((dx + st / 2) / st) as usize, ((dy + st / 2) / st) as usize);
            if i < base_side && j < base_side {
                let c = &mut colours[j * base_side + i];
                *c = deposit_color(r.kind);
            }
        }
    }
    let (mut pos, mut nor, mut col, mut uv) = (vec![], vec![], vec![], vec![]);
    let field = GroundField::new(s, cx, cy);
    let mut vert = |x: f32, y: f32, drop: f32, shade: f32| {
        let (wx, wz) = ((x + GRID_OFFSET - w as f32 / 2.0) * CELL_SIZE, (y + GRID_OFFSET - d as f32 / 2.0) * CELL_SIZE);
        let h = if subdivisions > 1 { field.fine(s, x, y) } else { field.at(x as i32, y as i32) };
        pos.push([wx, h - drop, wz]);
        let (gx, gz) = field.gradient(x.round() as i32, y.round() as i32, st);
        nor.push(Vec3::new(-gx, 1.0, -gz).normalize().to_array());
        let (fx, fy) = ((x - x0 as f32) / step as f32, (y - y0 as f32) / step as f32);
        let (i, j) = ((fx.floor() as usize).min(base_side - 1), (fy.floor() as usize).min(base_side - 1));
        let get = |di: usize, dj: usize| colours[(j + dj).min(base_side - 1) * base_side + (i + di).min(base_side - 1)];
        let c = mix(mix(get(0, 0), get(1, 0), fx.fract()), mix(get(0, 1), get(1, 1), fx.fract()), fy.fract());
        col.push(lin([c[0] * shade, c[1] * shade, c[2] * shade], 1.0));
        uv.push([x as f32 / w as f32, y as f32 / d as f32]);
    };
    let side = n + 1;
    for j in 0..=n {
        for i in 0..=n {
            vert(x0 as f32 + i as f32 * spacing, y0 as f32 + j as f32 * spacing, 0.0, 1.0);
        }
    }
    let mut idx: Vec<u32> = Vec::with_capacity(n * n * 6);
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
            vert(x0 as f32 + i as f32 * spacing, y0 as f32 + j as f32 * spacing, skirt, 0.78);
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
    for j in 0..n {
        for i in 0..n {
            let a = (j * side + i) as u32;
            let (b, c, e) = (a + 1, a + side as u32, a + side as u32 + 1);
            let y = |k: u32| pos[k as usize][1];
            if split_ae(y(a), y(b), y(c), y(e)) {
                idx.extend_from_slice(&[a, c, e, a, e, b]);
            } else {
                idx.extend_from_slice(&[a, c, b, b, c, e]);
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
                    ok &= at(&s.water.depth, x + dx, y + dy) <= 0.003;
                    ok &= at(&s.water.riverbed, x + dx, y + dy) < 0.2;
                }
            }
            let t = at(&s.height, x, y) / maxh;
            ok &= dried_bed(s, x, y) < 0.35;
            if !ok || slope_at(s, x, y) > 0.45 || t > 0.62 {
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
            if kind < 0.5 || (kind < 0.8 && r.2 > p.bloom) {
                let height = 1.0 - p.winter * 0.6;
                for k in 0..2 {
                    blob(d, c + Vec3::Y * (0.1 * height), Vec3::new(0.04, 0.17 * height, 0.15), yaw + k as f32 * 1.1, (k as f32 - 0.5) * 0.5, p.grass_deep, p.grass_light);
                }
            } else if kind < 0.8 {
                let petal = if r.1 < 0.34 { pal::FLOWER_WHITE } else if r.1 < 0.67 { pal::FLOWER_PINK } else { pal::FLOWER_YELLOW };
                blob(d, c + Vec3::Y * 0.09, Vec3::new(0.025, 0.09, 0.025), 0.0, 0.0, p.grass_deep, p.grass_deep);
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

