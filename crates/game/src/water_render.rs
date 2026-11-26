use crate::camera::{MainCamera};
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

impl Plugin for WaterRenderPlugin {
    fn build(&self, app: &mut App) {
        bevy::asset::embedded_asset!(app, "water.wesl");
        app.add_plugins(MaterialPlugin::<WaterMaterial>::default())
            .init_resource::<WaterChunks>()
            .add_systems(Startup, setup_water)
            .add_systems(Update, (update_water_chunks, update_water_style, hide_water_under_lens));
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
    /// The chunk's `water_stamp` and the view when its cells were last hashed (as the
    /// terrain's slots keep theirs).
    seen: Option<(u64, crate::terrain_render::View)>,
    /// `wet_ground`, as of `seen`.
    wet: bool,
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

/// The water surface's material, for water drawn elsewhere to look the same (a lock's chamber).
#[derive(Resource, Clone)]
pub struct SharedWater(pub Handle<WaterMaterial>);

/// A flat sheet of water in the water surface's own look: corners in world space at height 0
/// (lift it by its transform), `depth` metres deep, `ice` 0 open to 1 frozen over.
pub fn water_sheet(corners: [Vec3; 4], depth: f32, ice: f32) -> Mesh {
    let tri = [0usize, 1, 2, 0, 2, 3];
    let pos: Vec<[f32; 3]> = tri.iter().map(|&i| corners[i].to_array()).collect();
    let n = pos.len();
    Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0f32, 1.0, 0.0]; n])
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, pos.iter().map(|p| [p[0], p[2]]).collect::<Vec<_>>())
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_1, vec![[0.0f32, 0.0]; n])
        .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, vec![[depth, 0.0, ice, 1.0]; n])
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, pos)
        .with_inserted_indices(Indices::U32((0..n as u32).collect()))
}

fn setup_water(mut commands: Commands, sim: Res<Sim>, season: Res<Season>, mut water: ResMut<WaterChunks>, mut materials: ResMut<Assets<WaterMaterial>>) {
    water.material = materials.add(WaterMaterial {
        base: StandardMaterial { base_color: Color::WHITE, alpha_mode: AlphaMode::Blend, perceptual_roughness: 0.3, reflectance: 0.06, cull_mode: None, ..default() },
        extension: style_for(&Palette::at(season.phase)),
    });
    commands.insert_resource(SharedWater(water.material.clone()));
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

/// The water lens takes the water away altogether, so the bed shows its depth bands.
fn hide_water_under_lens(mut q: Query<&mut Visibility, With<WaterSurface>>) {
    let hide = crate::terrain_render::map_lens() == crate::terrain_render::WATER_LENS;
    let want = if hide { Visibility::Hidden } else { Visibility::Inherited };
    for mut v in &mut q {
        v.set_if_neq(want);
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

/// 1 where a cell's water is drawn at all (lone films left out; dam crests are drawn by
/// `vertex_water`, which leaves out their films too).
fn shows_water(s: &SimState, x: i32, y: i32) -> f32 {
    let h = at(&s.water.depth, x, y);
    (h > SHOW_MIN_M && !s.water.lone_film(x, y)) as i32 as f32
}

/// Wet cells softened over their four neighbours (WaterField::wet) and interpolated between
/// cell centres. The water's outline is where this falls to WET_EDGE, so a stream one cell wide
/// is a smooth band rather than the staircase of its cells, even where the bank is no higher
/// than the water.
const WET_EDGE: f32 = 0.3;
/// Metres of outline margin per unit of wetness above WET_EDGE.
const EDGE_M: f32 = 0.4;

use sim::water::SHOW_MIN_M;
use crate::seasonal::smooth as smooth_step;
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
            if !s.water.shown(cx, cy) {
                continue;
            }
            let (u, v) = s.water.velocity.get(cx as usize, cy as usize).copied().unwrap_or((0.0, 0.0));
            let speed = (u * u + v * v).sqrt();
            let froude = speed / (9.81 * h).sqrt();
            white = white.max(smooth_step(0.55, 1.0, froude) * smooth_step(0.2, 0.8, speed));
            level += at(&s.height, cx, cy) + at(&s.water.dam_height, cx, cy) + h;
            vx += u;
            vy += v;
            ice += (at(&s.water.ice, cx, cy) / sim::tuning::water::ICE_COVER_M).min(1.0);
            k += 1.0;
        }
    }
    (k > 0.0).then(|| (level / k, [vx / k, vy / k], white, ice / k))
}

/// Cells either side of a wet cell whose levels are averaged into its drawn surface.
const SMOOTH_CELLS: i32 = 2;
/// A level difference to a neighbour beyond this is a fall, left out of the smoothing.
const FALL_KEEP_M: f32 = 0.25;
/// Simulated water is drawn at least this far above the drawn ground, so a creek a few
/// centimetres deep is never hidden where the drawn ground, smoothed or coarser at a distance,
/// stands a little higher than the simulated bed.
const SHOWN_ABOVE_GROUND_M: f32 = 0.03;

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
        // A stream's surface, smoothed over the wet cells around each: the simulated level follows
        // every bump of the bed a few centimetres up, which draws as tilted plates at odd heights.
        // A neighbour more than FALL_KEEP_M away is a real fall (a crest, a weir) and stays sharp.
        let raw: Vec<Option<f32>> = cells.iter().map(|c| c.map(|w| w.0)).collect();
        for j in 0..n {
            for i in 0..n {
                let Some(l0) = raw[(j * n + i) as usize] else { continue };
                let (mut sum, mut k) = (0.0f32, 0.0f32);
                for dj in -SMOOTH_CELLS..=SMOOTH_CELLS {
                    for di in -SMOOTH_CELLS..=SMOOTH_CELLS {
                        let (a, b) = (i + di, j + dj);
                        if a < 0 || b < 0 || a >= n || b >= n { continue; }
                        if let Some(l) = raw[(b * n + a) as usize].filter(|l| (l - l0).abs() < FALL_KEEP_M) {
                            let w = 1.0 / (1.0 + (di * di + dj * dj) as f32);
                            sum += l * w;
                            k += w;
                        }
                    }
                }
                if let Some(c) = cells[(j * n + i) as usize].as_mut() { c.0 = sum / k; }
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

fn water_sample(s: &SimState, field: &WaterField, x: f32, y: f32, step: usize) -> (CellWater, bool) {
    let (ix, iy) = (x.floor() as i32, y.floor() as i32);
    let (u, v) = (x - ix as f32, y - iy as f32);
    if u == 0.0 && v == 0.0 {
        let w = if step == 1 { field.cell(s, ix, iy) } else { vertex_water(s, ix, iy, step as i32) };
        return (w, w.is_some());
    }
    let (mut level, mut vx, mut vy, mut white, mut ice, mut total) = (0.0, 0.0, 0.0, 0.0, 0.0, 0.0);
    let mut all_wet = true;
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
        } else {
            all_wet = false;
        }
    } }
    ((total > 0.0).then(|| (level / total, [vx / total, vy / total], white / total, ice / total)), total > 0.0 && all_wet)
}

pub fn build_water_mesh(s: &SimState, cx: usize, cy: usize, step: usize) -> Option<Mesh> {
    let subdivisions = canal_subdivisions(s, cx, cy);
    let spacing = step as f32 / subdivisions as f32;
    let side = CHUNK / step * subdivisions + 1;
    let (x0, y0) = ((cx * CHUNK) as i32, (cy * CHUNK) as i32);
    // Sample one vertex beyond the chunk on every side, so shorelines extrapolate across seams.
    let ext = side + 2;
    let field = WaterField::new(s, cx, cy);
    let ground_field = GroundField::new(s, cx, cy, subdivisions == 4);
    let mut own = Vec::with_capacity(ext * ext);
    let mut any = false;
    for j in -1..=side as i32 {
        for i in -1..=side as i32 {
            let (w, wet) = water_sample(s, &field, x0 as f32 + i as f32 * spacing, y0 as f32 + j as f32 * spacing, step);
            any |= w.is_some();
            own.push((w, wet));
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
            let at_ext = |di: i32, dj: i32| own[(j as i32 + 1 + dj) as usize * ext + (i as i32 + 1 + di) as usize].0;
            let all_wet = own[(j + 1) * ext + (i + 1)].1;
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
            let bare = if subdivisions > 1 { ground_field.fine(s, x, y) } else { terrain_sample(s, x, y, step) };
            let ground = bare + at(&s.water.dam_height, x.round() as i32, y.round() as i32);
            // Lift only vertices wholly inside wet cells; bank-straddling sub-cell vertices must
            // keep the real water level so shore triangles clip flat against steep canal banks.
            let level = if all_wet { level.max(ground + SHOWN_ABOVE_GROUND_M) } else { level };
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
    // A lock's chamber holds its own water (drawn with the lock), not the pond's.
    let holes = crate::entities_render::raft_landing::chamber_holes(s);
    let in_hole = |t: &[WV; 3]| {
        let c = Vec2::new(t[0].p[0] + t[1].p[0] + t[2].p[0], t[0].p[2] + t[1].p[2] + t[2].p[2]) / 3.0;
        holes.iter().any(|&(m, a, half, w)| (c - m).dot(a).abs() < half && (c - m).dot(a.perp()).abs() < w)
    };
    for j in 0..side - 1 {
        for i in 0..side - 1 {
            let (a, b, c, e) = (j * side + i, j * side + i + 1, (j + 1) * side + i, (j + 1) * side + i + 1);
            // Split each square along the same diagonal as the terrain under it.
            let tris = if split_ae(grounds[a], grounds[b], grounds[c], grounds[e]) { [[a, c, e], [a, e, b]] } else { [[a, c, b], [b, c, e]] };
            for tri in tris {
                let t = [verts[tri[0]], verts[tri[1]], verts[tri[2]]];
                if t.iter().all(|v| v.edge <= 0.0) || in_hole(&t) {
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

/// Rebuild the chunks whose water changed, found by the simulation's stamps as the terrain's
/// are (`update_terrain_chunks`).
#[allow(clippy::too_many_arguments)]
fn update_water_chunks(
    mut commands: Commands,
    sim: Res<Sim>,
    mut water: ResMut<WaterChunks>,
    mut meshes: ResMut<Assets<Mesh>>,
    cam: Query<&Transform, With<MainCamera>>,
    mut first: Local<bool>,
    mut cursor: Local<usize>,
    mut backlog: ResMut<MeshBacklog>,
    mut settled: Local<Option<[(u64, u64); 9]>>,
) {
    let _p = ::sim::prof::scope("game/update water chunks");
    backlog.water = true;
    let Some(campos) = cam.iter().next().map(|t| t.translation) else { return };
    let (cols, rows) = chunk_dims(&sim.0);
    if water.slots.len() != cols * rows {
        return;
    }
    let s = &sim.0;
    let mut budget = if *first { 10 } else { usize::MAX };
    *first = true;
    let started = std::time::Instant::now();
    let n_chunks = cols * rows;
    let mut rebuilt = false;
    // The season colours the water through its material, not its mesh.
    let view = (crate::terrain_render::map_lens(), 0);
    let stamps = crate::terrain_render::water_stamps(s);
    let still = *settled == Some(stamps);
    for k in 0..n_chunks {
        let idx = (*cursor + k) % n_chunks;
        let (cx, cy) = (idx % cols, idx / cols);
        let slot = &mut water.slots[idx];
        let stamp = match slot.seen {
            Some((stamp, _)) if still => stamp,
            _ => crate::terrain_render::water_stamp(s, cx, cy),
        };
        if slot.seen.map(|seen| seen.0) != Some(stamp) {
            slot.wet = crate::terrain_render::wet_ground(s, cx, cy);
        }
        let step = if slot.wet { 1 } else { crate::terrain_render::lod_step(campos, crate::terrain_render::chunk_centre(s, cx, cy)) };
        let known = slot.seen == Some((stamp, view));
        if known && slot.step == step {
            continue;
        }
        let hash = if known { slot.hash } else { region_hash(s, cx, cy, WATER_PAD, 0, false).max(1) };
        if slot.hash == hash && slot.step == step {
            slot.seen = Some((stamp, view));
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
        let mesh = build_water_mesh(s, cx, cy, step);
        crate::perf::WATER_MESH_NS.fetch_add(built.elapsed().as_nanos() as u64, std::sync::atomic::Ordering::Relaxed);
        let material = water.material.clone();
        let slot = &mut water.slots[idx];
        (slot.hash, slot.step, slot.seen) = (hash, step, Some((stamp, view)));
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
    *settled = Some(stamps);
    backlog.water = rebuilt;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canal_water_mesh_stays_flat_and_never_climbs_steep_banks() {
        let mut s = SimState::new(1);
        let w = s.height.width();
        let (cx, cy) = (0usize, 0usize);
        // Clear chunk (0, 0) to flat dry hillside at 12.0 m, then carve a 1-cell canal at x = 8
        // with bed 9.5 m and water depth 0.4 m (true water surface = 9.9 m).
        for y in 0..CHUNK + 4 {
            for x in 0..CHUNK + 4 {
                s.height[y * w + x] = 12.0;
                s.water.depth[y * w + x] = 0.0;
                s.water.dam_height[y * w + x] = 0.0;
                s.water.canal[y * w + x] = false;
                s.world.river[y * w + x] = false;
            }
        }
        for y in 4..12 {
            s.height[y * w + 8] = 9.5;
            s.water.depth[y * w + 8] = 0.4;
            s.water.canal[y * w + 8] = true;
        }

        let mesh = build_water_mesh(&s, cx, cy, 1).expect("canal water mesh");
        let positions = mesh.attribute(Mesh::ATTRIBUTE_POSITION).expect("positions").as_float3().expect("float3");
        assert!(!positions.is_empty());
        let max_y = positions.iter().map(|p| p[1]).fold(f32::NEG_INFINITY, f32::max);
        // Water surface is 9.9 m + SURFACE_LIFT (0.01 m) = 9.91 m; bank-climbing bug used to push
        // vertices above 11 m.
        assert!(max_y < 9.95, "canal water mesh climbed up the dry banks to y={max_y:.3} m (surface is 9.91 m)");
    }
}
