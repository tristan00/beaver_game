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
        perceptual_roughness: 0.35,
        reflectance: 0.06,
        normal_map_texture: Some(images.add(ripple_normal_map())),
        cull_mode: None,
        ..default()
    });
    let (cols, rows) = chunk_dims(&sim.0);
    water.slots = (0..cols * rows).map(|_| WSlot::default()).collect();
}

/// Margin (cells) of sim data outside a chunk that its water mesh reads: shore distances look up to
/// `SHORE_SEARCH` cells away, and the field's gradient one cell further.
/// Main-thread time per frame for water chunk rebuilds.
const WATER_REBUILD_BUDGET_S: f32 = 0.002;
pub const WATER_PAD: i32 = SHORE_SEARCH + 1;
const SHORE_SEARCH: i32 = 3;
/// Rings measured inward from the shore, in cells (current look; `docs/STYLE.md` section 3 sets the nearly-clear target):
/// a solid Foam ring, then a Deep rim, the Shallow body and a Mid glow centre.
pub const FOAM_W: f32 = 0.2;
/// Water vertex colours are the palette colours seen through the tone mapping (`palette::display_linear`);
/// the lit surface then multiplies them by this gain (same sun and ambient as `ENTITY_GAIN`) so a flat
/// sun-facing patch shows its palette hex (docs/STYLE.md section 3).
const WATER_GAIN: f32 = crate::entities_render::ENTITY_GAIN;
/// Extra cuts through the surface (shore distance, cells) so colour ramps follow the rings
/// instead of zig-zagging across the grid's triangles.
const RING_CUTS: [f32; 6] = [0.6, 0.9, 1.2, 1.6, 2.0, 2.5];
/// The shore is where the Gaussian-blurred wet mask crosses this value.
const SHORE_LEVEL: f32 = 0.3;
/// 5-tap binomial (σ ≈ 1 cell).
const BLUR5: [f32; 5] = [1.0 / 16.0, 4.0 / 16.0, 6.0 / 16.0, 4.0 / 16.0, 1.0 / 16.0];

/// Smooth signed distance to the shore, in cells, at every cell of a `(n+1)²` window starting at
/// `(x0, y0)`: positive in water, negative on land.
///
/// The shore line is a level of the wet mask blurred over about a cell, so one-cell streams that
/// run diagonally as 4-connected staircases become smooth ribbons and pond corners round off. Near
/// it the blurred value is turned into a distance by dividing by its gradient, so the Foam and Deep
/// rings keep a constant width; further in, the Euclidean distance to the nearest dry cell takes
/// over so wide ponds reach their Mid glow centre.
pub fn shore_field(s: &SimState, x0: i32, y0: i32, n: usize) -> Vec<f32> {
    let r = SHORE_SEARCH;
    let pad = 1; // for the gradient
    let side = n as i32 + 1 + 2 * pad;
    // wet mask over the window plus the search margin
    let m = side + 2 * r;
    let (mx0, my0) = (x0 - pad - r, y0 - pad - r);
    let mut wet = vec![0.0f32; (m * m) as usize];
    for j in 0..m {
        for i in 0..m {
            wet[(j * m + i) as usize] = (at(&s.water.depth, mx0 + i, my0 + j) > WET_EPS) as u8 as f32;
        }
    }
    // separable blur: horizontal over the whole mask, then vertical into the window
    let mut hb = vec![0.0f32; (m * m) as usize];
    for j in 0..m {
        for i in 2..m - 2 {
            hb[(j * m + i) as usize] = (0..5).map(|k| BLUR5[k] * wet[(j * m + i + k as i32 - 2) as usize]).sum();
        }
    }
    let mut blur = vec![0.0f32; (side * side) as usize];
    let mut dist = vec![0.0f32; (side * side) as usize];
    for j in 0..side {
        for i in 0..side {
            let (ci, cj) = (i + r, j + r);
            blur[(j * side + i) as usize] = (0..5).map(|k| BLUR5[k] * hb[((cj + k as i32 - 2) * m + ci) as usize]).sum();
            let w = wet[(cj * m + ci) as usize] > 0.5;
            let mut best = (r * r + 1) as f32;
            for dy in -r..=r {
                for dx in -r..=r {
                    let d2 = (dx * dx + dy * dy) as f32;
                    if d2 < best && (wet[((cj + dy) * m + ci + dx) as usize] > 0.5) != w {
                        best = d2;
                    }
                }
            }
            let d = best.sqrt() - 0.5;
            dist[(j * side + i) as usize] = if w { d } else { -d };
        }
    }
    let mut out = Vec::with_capacity((n + 1) * (n + 1));
    for j in 0..=n as i32 {
        for i in 0..=n as i32 {
            let (ci, cj) = (i + pad, j + pad);
            let b = |dx: i32, dy: i32| blur[((cj + dy) * side + ci + dx) as usize];
            let (gx, gy) = ((b(1, 0) - b(-1, 0)) * 0.5, (b(0, 1) - b(0, -1)) * 0.5);
            let grad = (gx * gx + gy * gy).sqrt().max(0.1);
            let near = ((b(0, 0) - SHORE_LEVEL) / grad).clamp(-3.0, 3.0);
            let d = dist[(cj * side + ci) as usize];
            // the gradient estimate overshoots where the blur flattens out
            out.push(near.min(d + 1.0));
        }
    }
    out
}

#[derive(Clone, Copy)]
struct WV {
    p: [f32; 3],
    f: f32,
    d: f32,
}

fn lerp_wv(a: &WV, b: &WV, t: f32) -> WV {
    let l = |x: f32, y: f32| x + (y - x) * t;
    WV { p: [l(a.p[0], b.p[0]), l(a.p[1], b.p[1]), l(a.p[2], b.p[2])], f: l(a.f, b.f), d: l(a.d, b.d) }
}

/// Sutherland–Hodgman clip of a convex polygon to `f >= level` (`above`) or `f <= level`.
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

/// Colour (sRGB) and alpha of the water at shore distance `f` (cells) and depth `d`: Foam ring at
/// the shore, Deep rim just inside it (and in really deep water), Shallow body, Mid glow centre.
pub fn water_color(p: &Palette, f: f32, d: f32, in_foam: bool) -> ([f32; 3], f32) {
    use crate::palette::{FOAM, WATER_MID_GLOW};
    let foam = if in_foam { 1.0 } else { 0.0 };
    let rim = 1.0 - smooth_step(0.6, 1.2, f);
    let very_deep = smooth_step(2.2, 5.0, d);
    let deep = rim.max(very_deep);
    let glow = smooth_step(1.5, 2.5, f) * (1.0 - very_deep);
    let mut c = mix_rgb(p.water_shallow, WATER_MID_GLOW, glow * 0.85);
    c = mix_rgb(c, p.water_deep, deep);
    c = mix_rgb(c, FOAM, foam);
    let mut a = 0.8 + 0.2 * deep + 0.15 * foam;
    // ice: freezes from the shore inwards as winter deepens
    let ice = if p.winter > 0.0 { ((p.winter * 4.5 - d) / 1.2).clamp(0.0, 1.0) } else { 0.0 };
    c = mix_rgb(c, p.ice, ice);
    a += (0.86 - a) * ice;
    (c, a.min(0.97))
}

/// Height of the displayed terrain at cell coordinates `(x, y)` when its chunk is meshed at `step`
/// cells per vertex: the same triangles (diagonal from (1,0) to (0,1)) as `build_chunk_mesh`.
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

/// Builds the water mesh for one chunk; `None` if it has no water at all.
///
/// The surface is a fine vertex grid (two vertices per cell when the terrain is at full detail)
/// clipped along the zero line of a smooth shore-distance field (marching triangles), so outlines
/// are smooth curves rather than cell staircases. A second cut at `FOAM_W` gives a crisp Foam ring
/// of constant width along every bank; inside it the Deep rim, Shallow body and Mid glow centre
/// follow the same distance. The sheet never dips below the terrain as displayed at `step`, so
/// the ground never clips the foam away, at any level of detail.
pub fn build_water_mesh(s: &SimState, p: &Palette, cx: usize, cy: usize, step: usize) -> Option<Mesh> {
    let (x0, y0) = ((cx * CHUNK) as i32, (cy * CHUNK) as i32);
    let depth = |x: i32, y: i32| at(&s.water.depth, x, y);
    let full = shore_field(s, x0, y0, CHUNK);
    if !full.iter().any(|&f| f > 0.0) {
        return None;
    }
    // per cell: (shore distance, water level, depth)
    let cs = CHUNK + 1;
    let mut cell = Vec::with_capacity(cs * cs);
    for j in 0..cs as i32 {
        for i in 0..cs as i32 {
            let (x, y) = (x0 + i, y0 + j);
            let h = at(&s.height, x, y);
            let d = depth(x, y);
            // water level: own if wet, else the mean of wet cells within two cells
            let level = if d > WET_EPS {
                h + d
            } else {
                let (mut sum, mut cnt) = (0.0, 0);
                for dy in -2..=2 {
                    for dx in -2..=2 {
                        let dd = depth(x + dx, y + dy);
                        if dd > WET_EPS {
                            sum += at(&s.height, x + dx, y + dy) + dd;
                            cnt += 1;
                        }
                    }
                }
                if cnt > 0 { sum / cnt as f32 } else { h }
            };
            cell.push([full[j as usize * cs + i as usize], level, d.max(0.0)]);
        }
    }
    let sub = if step == 1 { 2 } else { 1 };
    let n = CHUNK * sub;
    let side = n + 1;
    let mut verts: Vec<WV> = Vec::with_capacity(side * side);
    for j in 0..=n {
        for i in 0..=n {
            let (fx, fy) = (i as f32 / sub as f32, j as f32 / sub as f32);
            let (ci, cj) = ((i / sub).min(CHUNK - 1), (j / sub).min(CHUNK - 1));
            let (u, v) = (fx - ci as f32, fy - cj as f32);
            let g = |di: usize, dj: usize| cell[(cj + dj) * cs + ci + di];
            let (c00, c10, c01, c11) = (g(0, 0), g(1, 0), g(0, 1), g(1, 1));
            let bil = |k: usize| (c00[k] * (1.0 - u) + c10[k] * u) * (1.0 - v) + (c01[k] * (1.0 - u) + c11[k] * u) * v;
            let (x, y) = (x0 as f32 + fx, y0 as f32 + fy);
            let (wx, wz) = world_pos(s, x0, y0);
            let ground = terrain_shown(s, x, y, step);
            verts.push(WV { p: [wx + fx * CELL_SIZE, bil(1).max(ground + 0.05), wz + fy * CELL_SIZE], f: bil(0), d: bil(2) });
        }
    }
    let (mut pos, mut col, mut uv) = (vec![], vec![], vec![]);
    let mut emit = |poly: &[WV], in_foam: bool| {
        for k in 1..poly.len().saturating_sub(1) {
            for v in [&poly[0], &poly[k], &poly[k + 1]] {
                pos.push(v.p);
                uv.push([v.p[0] * 0.045, v.p[2] * 0.045]);
                let (c, a) = water_color(p, v.f, v.d, in_foam);
                let s = crate::palette::display_linear(c).map(|v| v * WATER_GAIN);
                col.push([s[0], s[1], s[2], a]);
            }
        }
    };
    for j in 0..n {
        for i in 0..n {
            let (a, b, c, e) = (j * side + i, j * side + i + 1, (j + 1) * side + i, (j + 1) * side + i + 1);
            // same diagonal as the terrain mesh, so the sheet stays above the ground inside triangles too
            for tri in [[a, c, b], [b, c, e]] {
                let t = [verts[tri[0]], verts[tri[1]], verts[tri[2]]];
                if t.iter().all(|v| v.f <= 0.0) {
                    continue;
                }
                // shore cut, then the hard-edged Foam band, then one band per ring level
                let mut rest = clip_poly(&t, 0.0, true);
                for (k, lvl) in std::iter::once(FOAM_W).chain(RING_CUTS).enumerate() {
                    if rest.len() < 3 {
                        break;
                    }
                    if rest.iter().all(|v| v.f >= lvl) {
                        continue;
                    }
                    if rest.iter().all(|v| v.f <= lvl) {
                        emit(&rest, k == 0);
                        rest.clear();
                        break;
                    }
                    emit(&clip_poly(&rest, lvl, false), k == 0);
                    rest = clip_poly(&rest, lvl, true);
                }
                emit(&rest, false);
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

/// Camera focus for the `pond` preset: the still pond with the most open banks (look-at point on its
/// surface), so close screenshots show a whole pond with its rings.
pub fn pond_view(s: &SimState) -> Option<Vec3> {
    let g = &s.world.pond;
    let (w, h) = (g.width(), g.height());
    let is_pond = |x: usize, y: usize| g.get(x, y).copied().unwrap_or(false);
    // one seed cell per pond (ponds are at least 16 cells apart)
    let mut seeds: Vec<(usize, usize)> = Vec::new();
    for y in 0..h {
        for x in 0..w {
            if is_pond(x, y) && !seeds.iter().any(|&(sx, sy)| sx.abs_diff(x) + sy.abs_diff(y) < 12) {
                seeds.push((x, y));
            }
        }
    }
    // centroid of each pond, scored by how much vegetation (trees) crowds its banks
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
) {
    let Some(campos) = cam.iter().next().map(|t| t.translation) else { return };
    let (cols, rows) = chunk_dims(&sim.0);
    if water.slots.len() != cols * rows {
        return;
    }
    let mut budget = if *first { 10 } else { usize::MAX };
    *first = true;
    // Rebuilds cost ~0.1-0.4 ms a chunk; spread a map-wide change (season step, flood) over frames.
    let started = std::time::Instant::now();
    let n_chunks = cols * rows;
    let palette = Palette::at(season.phase);
    // Ice only depends on the season while it is frozen-ish; recolour in coarse steps.
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
                *cursor = idx; // resume here next frame
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

    /// Every wet chunk gets a mesh with a Foam ring at its shore and Deep/Shallow rings inside,
    /// and rebuilding the whole map's water stays cheap (printed; `--nocapture`).
    #[test]
    fn water_rings_and_rebuild_cost() {
        let s = SimState::new(1);
        let p = Palette::at(0.3);
        let (cols, rows) = chunk_dims(&s);
        let foam = crate::palette::display_linear(crate::palette::FOAM).map(|v| v * WATER_GAIN);
        for step in [1, 2] {
            let t = std::time::Instant::now();
            let (mut wet, mut with_foam, mut tris) = (0, 0, 0);
            for cy in 0..rows {
                for cx in 0..cols {
                    let Some(m) = build_water_mesh(&s, &p, cx, cy, step) else { continue };
                    wet += 1;
                    let Some(bevy::mesh::VertexAttributeValues::Float32x4(c)) = m.attribute(Mesh::ATTRIBUTE_COLOR) else { panic!("colours") };
                    tris += c.len() / 3;
                    if c.iter().any(|v| (v[0] - foam[0]).abs() < 0.02 && (v[2] - foam[2]).abs() < 0.02) {
                        with_foam += 1;
                    }
                }
            }
            let ms = t.elapsed().as_secs_f64() * 1e3;
            println!("step {step}: {wet} wet chunks, {tris} triangles, {ms:.1} ms total, {:.3} ms/chunk", ms / wet.max(1) as f64);
            assert!(wet > 0);
            assert!(with_foam * 10 >= wet * 9, "foam ring in only {with_foam} of {wet} wet chunks");
        }
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
