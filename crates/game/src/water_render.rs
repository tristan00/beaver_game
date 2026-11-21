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
    #[uniform(100)]
    pub ice: Vec4,
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
        shallow: lin(crate::palette::mix(SURFACE_SHALLOW, crate::palette::WATER_SHALLOW, 0.15)),
        deep: lin(crate::palette::mix(SURFACE_DEEP, crate::palette::WATER_DEEP, 0.15)),
        foam: lin(crate::palette::FOAM),
        ice: lin(p.ice),
        look: Vec4::new(WATER_DEEP_M, WATER_FILM_ALPHA, WATER_DEEP_ALPHA, (p.winter * 1.5).clamp(0.0, 1.0)),
    }
}

/// Water colours: a clear green-teal where the bed shows through, river blue where it is deep.
const SURFACE_SHALLOW: [f32; 3] = [0.32, 0.68, 0.72];
const SURFACE_DEEP: [f32; 3] = [0.13, 0.40, 0.55];
/// Depth at which water shows its own deep colour rather than its bed.
const WATER_DEEP_M: f32 = 0.6;
const WATER_FILM_ALPHA: f32 = 0.72;
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
pub const WATER_PAD: i32 = 3;
#[derive(Clone, Copy)]
struct WV {
    p: [f32; 3],
    /// Where the water is drawn: positive inside, zero on the outline.
    edge: f32,
    depth: f32,
    white: f32,
    vel: [f32; 2],
    ice: f32,
}

fn lerp_wv(a: &WV, b: &WV, t: f32) -> WV {
    let l = |x: f32, y: f32| x + (y - x) * t;
    WV { p: [l(a.p[0], b.p[0]), l(a.p[1], b.p[1]), l(a.p[2], b.p[2])], edge: l(a.edge, b.edge), depth: l(a.depth, b.depth), white: l(a.white, b.white), vel: [l(a.vel[0], b.vel[0]), l(a.vel[1], b.vel[1])], ice: l(a.ice, b.ice) }
}

/// The part of a polygon inside the water's outline.
fn clip_poly(poly: &[WV]) -> Vec<WV> {
    let mut out = Vec::with_capacity(poly.len() + 2);
    for k in 0..poly.len() {
        let (a, b) = (&poly[k], &poly[(k + 1) % poly.len()]);
        let (ia, ib) = (a.edge >= 0.0, b.edge >= 0.0);
        if ia {
            out.push(*a);
        }
        if ia != ib {
            out.push(lerp_wv(a, b, -a.edge / (b.edge - a.edge)));
        }
    }
    out
}

/// 1 where a cell's water is drawn at all.
fn shows_water(s: &SimState, x: i32, y: i32) -> f32 {
    let h = at(&s.water.depth, x, y);
    (h > SHOW_MIN_M && !lone_film(s, x, y, h)) as i32 as f32
}

/// Wet cells softened over their four neighbours (WaterField::wet) and interpolated between
/// cell centres. The water's outline is where this falls to WET_EDGE, so a stream one cell wide
/// is a smooth band rather than the staircase of its cells, even where the bank is no higher
/// than the water.
const WET_EDGE: f32 = 0.3;
/// Metres of outline margin per unit of wetness above WET_EDGE.
const EDGE_M: f32 = 0.4;

fn terrain_shown(s: &SimState, x: f32, y: f32, step: usize) -> f32 {
    terrain_sample(s, x, y, step)
}

/// Water thinner than this is not drawn (a damp film reads as wet ground, not water).
const SHOW_MIN_M: f32 = 0.003;
const SURFACE_LIFT: f32 = 0.01;

/// Water at one mesh vertex: the surface level, velocity and white water of the visibly wet
/// cells it stands for (the step x step block at and after it).
fn vertex_water(s: &SimState, x: i32, y: i32, step: i32) -> Option<(f32, [f32; 2], f32, f32)> {
    let n = s.height.width() as i32;
    let (mut level, mut vx, mut vy, mut white, mut k) = (0.0f32, 0.0f32, 0.0f32, 0.0f32, 0.0f32);
    let mut ice = 0.0;
    for by in 0..step {
        for bx in 0..step {
            let (cx, cy) = ((x + bx).clamp(0, n - 1), (y + by).clamp(0, n - 1));
            let h = at(&s.water.depth, cx, cy);
            if h <= SHOW_MIN_M || lone_film(s, cx, cy, h) {
                continue;
            }
            let (u, v) = s.water.velocity.get(cx as usize, cy as usize).copied().unwrap_or((0.0, 0.0));
            let speed = (u * u + v * v).sqrt();
            let froude = speed / (9.81 * h).sqrt();
            white = white.max(smooth_step(0.55, 1.0, froude) * smooth_step(0.2, 0.8, speed));
            level += at(&s.height, cx, cy) + at(&s.water.dam_height, cx, cy) + h;
            vx += u;
            vy += v;
            ice += at(&s.water.ice, cx, cy);
            k += 1.0;
        }
    }
    (k > 0.0).then(|| (level / k, [vx / k, vy / k], white, ice / k))
}

/// A thin film on a cell with no wet neighbour is damp ground, not a pool: drawing it leaves a
/// loose chip of water beside the stream.
const LONE_FILM_M: f32 = 0.02;

fn lone_film(s: &SimState, x: i32, y: i32, h: f32) -> bool {
    h < LONE_FILM_M && [(-1, 0), (1, 0), (0, -1), (0, 1)].iter().all(|&(dx, dy)| at(&s.water.depth, x + dx, y + dy) <= SHOW_MIN_M)
}

/// Where the surface drops this much between neighbouring vertices (a dam crest, a weir, a
/// riffle) the water is drawn white, so a spill reads as a fall instead of a tilted pane.
const FALL_WHITE_M: [f32; 2] = [0.4, 0.8];

type CellWater = Option<(f32, [f32; 2], f32, f32)>;

/// `vertex_water` at step 1 for one chunk and a margin, read once per mesh build.
struct WaterField {
    x0: i32,
    y0: i32,
    n: i32,
    cells: Vec<CellWater>,
    wet: Field,
}

impl WaterField {
    fn new(s: &SimState, cx: usize, cy: usize) -> WaterField {
        let (x0, y0, n) = ((cx * CHUNK) as i32 - Field::PAD, (cy * CHUNK) as i32 - Field::PAD, CHUNK as i32 + 2 * Field::PAD + 1);
        let mut cells = Vec::with_capacity((n * n) as usize);
        for j in 0..n {
            for i in 0..n {
                cells.push(vertex_water(s, x0 + i, y0 + j, 1));
            }
        }
        // The drawn current, per cell: the surface slope over a 3x3 block of wet cells, then
        // averaged with the wet neighbours, so it turns smoothly from vertex to vertex (a field that
        // jumps shears the moving ripples into triangle seams).
        let get = |cells: &Vec<CellWater>, i: i32, j: i32| if i < 0 || j < 0 || i >= n || j >= n { None } else { cells[(j * n + i) as usize] };
        let mut flow = vec![[0.0f32; 2]; (n * n) as usize];
        for j in 0..n {
            for i in 0..n {
                let Some(w) = get(&cells, i, j) else { continue };
                let (mut gx, mut kx, mut gy, mut ky) = (0.0, 0.0, 0.0, 0.0);
                for d in -1..=1 {
                    if let (Some(a), Some(b)) = (get(&cells, i - 1, j + d), get(&cells, i + 1, j + d)) { gx += (b.0 - a.0) / 2.0; kx += 1.0; }
                    if let (Some(a), Some(b)) = (get(&cells, i + d, j - 1), get(&cells, i + d, j + 1)) { gy += (b.0 - a.0) / 2.0; ky += 1.0; }
                }
                if kx == 0.0 {
                    if let Some(b) = get(&cells, i + 1, j) { gx = b.0 - w.0; kx = 1.0; } else if let Some(a) = get(&cells, i - 1, j) { gx = w.0 - a.0; kx = 1.0; }
                }
                if ky == 0.0 {
                    if let Some(b) = get(&cells, i, j + 1) { gy = b.0 - w.0; ky = 1.0; } else if let Some(a) = get(&cells, i, j - 1) { gy = w.0 - a.0; ky = 1.0; }
                }
                let grad = [if kx > 0.0 { gx / kx } else { 0.0 }, if ky > 0.0 { gy / ky } else { 0.0 }];
                flow[(j * n + i) as usize] = shown_flow(w.1, grad);
            }
        }
        for j in 0..n {
            for i in 0..n {
                if get(&cells, i, j).is_none() { continue; }
                let (mut sum, mut k) = ([0.0f32; 2], 0.0f32);
                for dj in -1..=1 { for di in -1..=1 {
                    if get(&cells, i + di, j + dj).is_some() {
                        let f = flow[((j + dj) * n + i + di) as usize];
                        sum = [sum[0] + f[0], sum[1] + f[1]];
                        k += 1.0;
                    }
                } }
                if let Some(c) = cells[(j * n + i) as usize].as_mut() { c.1 = [sum[0] / k, sum[1] / k]; }
            }
        }
        let shown = Field::new(cx, cy, |x, y| shows_water(s, x, y));
        let wet = Field::new(cx, cy, |x, y| 0.4 * shown.get(x, y) + 0.15 * (shown.get(x - 1, y) + shown.get(x + 1, y) + shown.get(x, y - 1) + shown.get(x, y + 1)));
        WaterField { x0, y0, n, cells, wet }
    }

    fn cell(&self, s: &SimState, x: i32, y: i32) -> CellWater {
        let (i, j) = (x - self.x0, y - self.y0);
        if i < 0 || j < 0 || i >= self.n || j >= self.n {
            return vertex_water(s, x, y, 1);
        }
        self.cells[(j * self.n + i) as usize]
    }
}

/// The current drawn on the surface. Engines report creek speeds of a few cm/s or none (much of
/// a reach is routed cell to cell without face fluxes), which reads as still water. Where the
/// surface slopes, the water is drawn running downhill at a speed that grows with the slope;
/// level lakes and ponds stay still.
fn shown_flow(vel: [f32; 2], grad: [f32; 2]) -> [f32; 2] {
    let slope = (grad[0] * grad[0] + grad[1] * grad[1]).sqrt();
    let speed = smooth_step(FLOW_SLOPE[0], FLOW_SLOPE[1], slope) * (0.3 + 3.0 * slope).min(0.8);
    let sim = (vel[0] * vel[0] + vel[1] * vel[1]).sqrt();
    if sim >= speed || slope <= 0.0 {
        return vel;
    }
    [-grad[0] / slope * speed, -grad[1] / slope * speed]
}
const FLOW_SLOPE: [f32; 2] = [0.0008, 0.006];

fn water_sample(s: &SimState, field: &WaterField, x: f32, y: f32, step: usize) -> CellWater {
    let (ix, iy) = (x.floor() as i32, y.floor() as i32);
    let (u, v) = (x - ix as f32, y - iy as f32);
    if u == 0.0 && v == 0.0 { return if step == 1 { field.cell(s, ix, iy) } else { vertex_water(s, ix, iy, step as i32) }; }
    let (mut level, mut vx, mut vy, mut white, mut ice, mut total) = (0.0, 0.0, 0.0, 0.0, 0.0, 0.0);
    for dy in 0..=1 { for dx in 0..=1 {
        let weight = (if dx == 0 { 1.0 - u } else { u }) * (if dy == 0 { 1.0 - v } else { v });
        if weight <= 0.0 { continue; }
        if let Some(w) = field.cell(s, ix + dx, iy + dy) {
            level += w.0 * weight;
            vx += w.1[0] * weight;
            vy += w.1[1] * weight;
            white += w.2 * weight;
            ice += w.3 * weight;
            total += weight;
        }
    } }
    (total > 0.0).then(|| (level / total, [vx / total, vy / total], white / total, ice / total))
}

pub fn build_water_mesh(s: &SimState, cx: usize, cy: usize, step: usize) -> Option<Mesh> {
    let subdivisions = canal_subdivisions(s, cx, cy);
    let spacing = step as f32 / subdivisions as f32;
    let side = CHUNK / step * subdivisions + 1;
    let (x0, y0) = ((cx * CHUNK) as i32, (cy * CHUNK) as i32);
    // Sample one vertex beyond the chunk on every side, so shorelines extrapolate across seams.
    let ext = side + 2;
    let field = WaterField::new(s, cx, cy);
    let ground_field = GroundField::new(s, cx, cy);
    let mut own = Vec::with_capacity(ext * ext);
    let mut any = false;
    for j in -1..=side as i32 {
        for i in -1..=side as i32 {
            let w = water_sample(s, &field, x0 as f32 + i as f32 * spacing, y0 as f32 + j as f32 * spacing, step);
            any |= w.is_some();
            own.push(w);
        }
    }
    if !any {
        return None;
    }
    let mut verts: Vec<WV> = Vec::with_capacity(side * side);
    let mut grounds: Vec<f32> = Vec::with_capacity(side * side);
    for j in 0..side {
        for i in 0..side {
            let (x, y) = (x0 as f32 + i as f32 * spacing, y0 as f32 + j as f32 * spacing);
            let at_ext = |di: i32, dj: i32| own[(j as i32 + 1 + dj) as usize * ext + (i as i32 + 1 + di) as usize];
            // A dry vertex takes the level of the water beside it, so the waterline falls where
            // that level meets the ground between them.
            let (level, vel, white, ice) = match at_ext(0, 0) {
                Some(w) => {
                    let mut drop = 0.0f32;
                    for (di, dj) in [(-1, 0), (1, 0), (0, -1), (0, 1)] {
                        if let Some(o) = at_ext(di, dj) {
                            drop = drop.max((w.0 - o.0).abs() / spacing);
                        }
                    }
                    (w.0, w.1, w.2.max(smooth_step(FALL_WHITE_M[0], FALL_WHITE_M[1], drop)), w.3)
                }
                None => {
                    let (mut l, mut k, mut vx, mut vy, mut ice) = (0.0f32, 0.0f32, 0.0f32, 0.0f32, 0.0f32);
                    for (di, dj) in [(-1, 0), (1, 0), (0, -1), (0, 1), (-1, -1), (1, 1), (-1, 1), (1, -1)] {
                        if let Some(w) = at_ext(di, dj) {
                            l += w.0;
                            vx += w.1[0];
                            vy += w.1[1];
                            ice += w.3;
                            k += 1.0;
                        }
                    }
                    if k > 0.0 { (l / k, [vx / k, vy / k], 0.0, ice / k) } else { (f32::NEG_INFINITY, [0.0, 0.0], 0.0, 0.0) }
                }
            };
            let bare = if subdivisions > 1 { ground_field.fine(s, x, y) } else { terrain_shown(s, x, y, step) };
            let ground = bare + at(&s.water.dam_height, x.round() as i32, y.round() as i32);
            let depth = if level.is_finite() { level - ground } else { -1.0 };
            let edge = depth.min((field.wet.bilinear(x, y) - WET_EDGE) * EDGE_M);
            let (wx, wz) = ((x + GRID_OFFSET - s.height.width() as f32 / 2.0) * CELL_SIZE, (y + GRID_OFFSET - s.height.height() as f32 / 2.0) * CELL_SIZE);
            // Clip the actual surface against the bank; lifting dry vertices to the bank
            // before clipping bends shore triangles up over the soil.
            let shown = if level.is_finite() { level } else { ground - 1.0 };
            // Colour depth fades to nothing at the outline, so shallow tint and lace sit on the edge.
            let depth = depth.min(edge.max(0.0) * 6.0);
            verts.push(WV { p: [wx, shown + SURFACE_LIFT, wz], edge, depth, white, vel, ice });
            grounds.push(bare);
        }
    }
    let (mut pos, mut col, mut uv, mut uv1) = (vec![], vec![], vec![], vec![]);
    for j in 0..side - 1 {
        for i in 0..side - 1 {
            let (a, b, c, e) = (j * side + i, j * side + i + 1, (j + 1) * side + i, (j + 1) * side + i + 1);
            // Split each square along the same diagonal as the terrain under it.
            let tris = if split_ae(grounds[a], grounds[b], grounds[c], grounds[e]) { [[a, c, e], [a, e, b]] } else { [[a, c, b], [b, c, e]] };
            for tri in tris {
                let t = [verts[tri[0]], verts[tri[1]], verts[tri[2]]];
                if t.iter().all(|v| v.edge <= 0.0) {
                    continue;
                }
                let poly = clip_poly(&t);
                for k in 1..poly.len().saturating_sub(1) {
                    for v in [&poly[0], &poly[k], &poly[k + 1]] {
                        pos.push(v.p);
                        uv.push([v.p[0], v.p[2]]);
                        uv1.push(v.vel);
                        col.push([v.depth.max(0.0), v.white, v.ice, 1.0]);
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
            let step = hydrology_lod(&sim.0, cx, cy, campos);
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
