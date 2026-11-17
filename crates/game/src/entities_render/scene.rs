//! Render-side description of everything that is drawn (`RenderScene`) and the stub layout used while
//! the sim crates do not yet expose beavers, trees or buildings.
use super::meshgen::Rng;
use crate::terrain_render::CELL_SIZE as CELL;
use bevy::prelude::*;
use sim::grid::HeightGrid;
use sim::SimState;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum Species {
    Aspen,
    Maple,
    Pine,
    Willow,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum TreeState {
    Alive,
    Stump,
    /// Drowned dead tree standing in water.
    Snag,
}

#[derive(Clone, Copy, Debug)]
pub struct TreeVis {
    pub pos: Vec3,
    pub species: Species,
    pub state: TreeState,
    pub scale: f32,
    pub yaw: f32,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Pose {
    Idle,
    Walk,
    Swim,
    Gnaw,
    Carry,
}

#[derive(Clone, Copy, Debug)]
pub struct BeaverState {
    /// Ground position (world xz) and facing (radians around +Y, 0 = +Z).
    pub pos: Vec2,
    pub yaw: f32,
    pub pose: Pose,
    pub moving: bool,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum StructKind {
    Lodge,
    Dam,
    Canal,
    Burrow,
    Workshop,
    WaterWheel,
    Dock,
    Barge,
}

#[derive(Clone, Copy, Debug)]
pub struct StructVis {
    pub kind: StructKind,
    /// Base position; `y` is the ground (or water) height the model stands on.
    pub pos: Vec3,
    pub yaw: f32,
    /// Length along local X for dams, canals, docks, barges.
    pub len: f32,
    /// 0..1; below 1 the construction-site look is used.
    pub progress: f32,
}

#[derive(Clone, Copy, Debug)]
pub struct Pond {
    pub center: Vec2,
    pub level: f32,
}

#[derive(Clone, Copy, Debug)]
enum Leg {
    Move,
    Carry,
    Hold(Pose),
}

/// Looping stub itinerary: shore -> tree -> gnaw -> carry home -> swim -> ...
#[derive(Clone, Debug)]
pub struct Route {
    pts: Vec<Vec2>,
    legs: Vec<Leg>,
    durs: Vec<f32>,
    hold_yaw: Vec<f32>,
    total: f32,
    offset: f32,
}

pub struct BeaverVis {
    pub id: u32,
    pub state: BeaverState,
    pub route: Option<Route>,
}

#[derive(Resource)]
pub struct RenderScene {
    pub trees: Vec<TreeVis>,
    pub structs: Vec<StructVis>,
    pub beavers: Vec<BeaverVis>,
    pub pond: Option<Pond>,
    /// True while the data is stub data rather than sim state.
    pub stub: bool,
}

#[derive(Clone, Copy)]
pub struct SceneConfig {
    pub trees: usize,
    pub beavers: usize,
}

impl SceneConfig {
    pub const NORMAL: Self = Self { trees: 1800, beavers: 40 };
    pub const STRESS: Self = Self { trees: 5000, beavers: 300 };
}

/// Bilinear terrain height at world (x, z); matches the terrain mesh layout.
pub fn height_at(h: &HeightGrid, x: f32, z: f32) -> f32 {
    let (w, d) = (h.width(), h.height());
    let gx = (x / CELL + w as f32 / 2.0).clamp(0.0, w as f32 - 1.001);
    let gz = (z / CELL + d as f32 / 2.0).clamp(0.0, d as f32 - 1.001);
    let (x0, z0) = (gx.floor() as usize, gz.floor() as usize);
    let (fx, fz) = (gx - x0 as f32, gz - z0 as f32);
    let at = |x: usize, z: usize| *h.get(x, z).unwrap_or(&0.0);
    let top = at(x0, z0) + (at(x0 + 1, z0) - at(x0, z0)) * fx;
    let bot = at(x0, z0 + 1) + (at(x0 + 1, z0 + 1) - at(x0, z0 + 1)) * fx;
    top + (bot - top) * fz
}

fn hash(a: i32, b: i32, seed: u32) -> f32 {
    let mut x = (a as u32).wrapping_mul(0x9E37_79B1) ^ (b as u32).wrapping_mul(0x85EB_CA6B) ^ seed.wrapping_mul(0xC2B2_AE35);
    x ^= x >> 15;
    x = x.wrapping_mul(0x2C1B_3C6D);
    x ^= x >> 12;
    x = x.wrapping_mul(0x297A_2D39);
    x ^= x >> 15;
    (x & 0xffff) as f32 / 65535.0
}

/// Smooth low-frequency noise in 0..1.
fn vnoise(x: f32, z: f32, seed: u32) -> f32 {
    let (xi, zi) = (x.floor(), z.floor());
    let (fx, fz) = (x - xi, z - zi);
    let (sx, sz) = (fx * fx * (3.0 - 2.0 * fx), fz * fz * (3.0 - 2.0 * fz));
    let h = |dx: i32, dz: i32| hash(xi as i32 + dx, zi as i32 + dz, seed);
    let a = h(0, 0) + (h(1, 0) - h(0, 0)) * sx;
    let b = h(0, 1) + (h(1, 1) - h(0, 1)) * sx;
    a + (b - a) * sz
}

/// Where the stub colony lives: a pond in the lowest part of the map and a lodge on its flattest bank.
struct Site {
    center: Vec2,
    level: f32,
    shore_r: [f32; 24],
    /// Angle of the lodge on the shore.
    a0: f32,
    mean_r: f32,
}

impl Site {
    fn find(h: &HeightGrid) -> Site {
        let (w, d) = (h.width() as i32, h.height() as i32);
        let to_world = |ix: i32, iz: i32| Vec2::new(ix as f32 * CELL - w as f32 * CELL / 2.0, iz as f32 * CELL - d as f32 * CELL / 2.0);
        let at = |ix: i32, iz: i32| *h.get(ix.clamp(0, w - 1) as usize, iz.clamp(0, d - 1) as usize).unwrap_or(&0.0);
        let (mut best, mut best_c) = (f32::MAX, Vec2::ZERO);
        let lim = 30.0;
        for iz in 0..d {
            for ix in 0..w {
                let p = to_world(ix, iz);
                if p.x.abs() > lim || p.y.abs() > lim {
                    continue;
                }
                let mut s = 0.0;
                for dz in -3..=3 {
                    for dx in -3..=3 {
                        s += at(ix + dx, iz + dz);
                    }
                }
                s /= 49.0;
                if s < best {
                    best = s;
                    best_c = p;
                }
            }
        }
        let hmin = height_at(h, best_c.x, best_c.y);
        let mut level = hmin + 0.8;
        loop {
            let mut n = 0;
            for iz in 0..d {
                for ix in 0..w {
                    if (to_world(ix, iz) - best_c).length() < 14.0 && at(ix, iz) < level {
                        n += 1;
                    }
                }
            }
            if n >= 300 || level > hmin + 4.5 {
                break;
            }
            level += 0.25;
        }
        let mut shore_r = [8.0f32; 24];
        for (i, sr) in shore_r.iter_mut().enumerate() {
            let a = i as f32 / 24.0 * std::f32::consts::TAU;
            let dir = Vec2::new(a.sin(), a.cos());
            let mut r = 1.0;
            while r < 26.0 {
                let p = best_c + dir * r;
                if height_at(h, p.x, p.y) >= level {
                    break;
                }
                r += 0.5;
            }
            *sr = r.clamp(6.0, 22.0);
        }
        for _ in 0..3 {
            let prev = shore_r;
            for i in 0..24 {
                shore_r[i] = (prev[(i + 23) % 24] + prev[i] * 2.0 + prev[(i + 1) % 24]) / 4.0;
            }
        }
        let mean_r = shore_r.iter().sum::<f32>() / 24.0;
        let mut site = Site { center: best_c, level, shore_r, a0: 0.0, mean_r };
        // Lodge on the flattest bank that still leaves room (and stays inside the map).
        let mut best_score = f32::MAX;
        for i in 0..24 {
            let a = i as f32 / 24.0 * std::f32::consts::TAU;
            let (p, o) = site.shore(a, 0.0);
            let q = p + o * 5.0;
            if q.abs().max_element() > w as f32 * CELL * 0.5 - 12.0 {
                continue;
            }
            let (h0, h1) = (height_at(h, p.x, p.y), height_at(h, q.x, q.y));
            let far = p + o * 14.0;
            let score = (h1 - h0 - 0.8).abs() + (height_at(h, far.x, far.y) - h1).abs() * 0.4;
            if score < best_score {
                best_score = score;
                site.a0 = a;
            }
        }
        site
    }

    fn radius(&self, a: f32) -> f32 {
        let t = a.rem_euclid(std::f32::consts::TAU) / std::f32::consts::TAU * 24.0;
        let (i, f) = (t.floor() as usize % 24, t.fract());
        self.shore_r[i] * (1.0 - f) + self.shore_r[(i + 1) % 24] * f
    }

    /// Shore point at angle `a`, pushed `inset` outward (onto land), and the outward direction.
    fn shore(&self, a: f32, inset: f32) -> (Vec2, Vec2) {
        let dir = Vec2::new(a.sin(), a.cos());
        (self.center + dir * (self.radius(a) + inset), dir)
    }

    /// Shore position `arc` world units along the bank from the lodge.
    fn along(&self, arc: f32, inset: f32) -> (Vec2, Vec2) { self.shore(self.a0 + arc / self.mean_r, inset) }
}

/// Camera focus for the `lodge` preset: (look-at point, yaw that views the lodge from the pond).
pub fn lodge_view(h: &HeightGrid) -> (Vec3, f32) {
    let site = Site::find(h);
    let (p, o) = site.along(0.0, 3.6);
    let y = height_at(h, p.x, p.y);
    // Look slightly toward the pond so the dock, dam and swimmers are in frame too.
    let t = p - o * 3.0;
    (Vec3::new(t.x, y + 1.2, t.y), ((-o.x).atan2(-o.y) + 0.4).rem_euclid(std::f32::consts::TAU))
}

/// Builds the scene from sim state. The sim has no beaver/tree/building data yet, so this falls back
/// to a deterministic stub colony derived from the seed and terrain.
pub fn scene_from_sim(sim: &SimState, cfg: SceneConfig) -> RenderScene {
    // When `sim.colony` / `sim.buildings` / `sim.ecology` expose entities, convert them here and return.
    stub_scene(sim, cfg)
}

fn stub_scene(sim: &SimState, cfg: SceneConfig) -> RenderScene {
    let h = &sim.height;
    let site = Site::find(h);
    let seed = sim.seed as u32 ^ 0x5eed;
    let gy = |p: Vec2| height_at(h, p.x, p.y);
    let lvl = site.level;
    let mut structs: Vec<StructVis> = Vec::new();
    let yaw_of = |dir: Vec2| dir.x.atan2(dir.y); // model +Z faces `dir`
    let mut sites: Vec<Vec2> = Vec::new();

    // Lodge (finished) facing the water.
    let (lp, lo) = site.along(0.0, 3.8);
    structs.push(StructVis { kind: StructKind::Lodge, pos: Vec3::new(lp.x, gy(lp).max(lvl - 0.3), lp.y), yaw: yaw_of(-lo), len: 1.0, progress: 1.0 });
    sites.push(lp - lo * 4.2);
    // Second lodge going up.
    let (p, o) = site.along(-17.0, 4.5);
    structs.push(StructVis { kind: StructKind::Lodge, pos: Vec3::new(p.x, gy(p), p.y), yaw: yaw_of(-o), len: 1.0, progress: 0.5 });
    sites.push(p - o * 4.5);
    // Dock with a barge alongside.
    let (p, o) = site.along(8.5, 0.3);
    structs.push(StructVis { kind: StructKind::Dock, pos: Vec3::new(p.x, lvl, p.y), yaw: yaw_of(-o), len: 6.0, progress: 1.0 });
    sites.push(p + o * 0.5);
    let bp = p - o * 4.6 + Vec2::new(-o.y, o.x) * 3.2;
    structs.push(StructVis { kind: StructKind::Barge, pos: Vec3::new(bp.x, lvl, bp.y), yaw: yaw_of(-o) + 0.12, len: 7.0, progress: 1.0 });
    // Workshop (finished) with a water wheel, plus one under construction.
    let (p, o) = site.along(-9.0, 5.5);
    structs.push(StructVis { kind: StructKind::Workshop, pos: Vec3::new(p.x, gy(p), p.y), yaw: yaw_of(-o), len: 1.0, progress: 1.0 });
    sites.push(p - o * 3.5);
    let (p, o) = site.along(-9.0 - 4.2, 1.3);
    structs.push(StructVis { kind: StructKind::WaterWheel, pos: Vec3::new(p.x, lvl, p.y), yaw: yaw_of(-o), len: 1.0, progress: 1.0 });
    let (p, o) = site.along(-27.0, 4.5);
    structs.push(StructVis { kind: StructKind::Workshop, pos: Vec3::new(p.x, gy(p), p.y), yaw: yaw_of(-o), len: 1.0, progress: 0.3 });
    // Dam: a run of 4-unit segments along the bank, the last still being built.
    for k in 0..5 {
        let (a, _) = site.along(15.0 + k as f32 * 4.0, -0.4);
        let (b, _) = site.along(15.0 + (k + 1) as f32 * 4.0, -0.4);
        let m = (a + b) * 0.5;
        let d = b - a;
        structs.push(StructVis {
            kind: StructKind::Dam,
            pos: Vec3::new(m.x, lvl - 0.1, m.y),
            yaw: -d.y.atan2(d.x),
            len: d.length().clamp(3.0, 5.0),
            progress: if k == 4 { 0.4 } else { 1.0 },
        });
    }
    sites.push(site.along(23.0, 1.0).0);
    // Canal leading away from the pond, last stretch unfinished.
    let (cs, co) = site.along(-34.0, 0.0);
    for k in 0..6 {
        let m = cs + co * (2.0 + 4.0 * k as f32) + co * 0.0;
        structs.push(StructVis {
            kind: StructKind::Canal,
            pos: Vec3::new(m.x, gy(m).max(lvl) - 0.25, m.y),
            yaw: -co.y.atan2(co.x),
            len: 4.0,
            progress: if k >= 4 { 0.3 } else { 1.0 },
        });
    }
    // Burrows in the bank.
    for (arc, inset) in [(-22.0, 6.5), (-24.5, 8.5), (28.0, 6.5)] {
        let (p, o) = site.along(arc, inset);
        structs.push(StructVis { kind: StructKind::Burrow, pos: Vec3::new(p.x, gy(p), p.y), yaw: yaw_of(-o), len: 1.0, progress: 1.0 });
        sites.push(p - o * 1.6);
    }

    // ---- trees -------------------------------------------------------------------------------
    let half = h.width() as f32 * CELL * 0.5 - 2.0;
    let mut rng = Rng(seed | 1);
    let clear: Vec<(Vec2, f32)> = structs
        .iter()
        .map(|s| {
            let r = match s.kind {
                StructKind::Lodge => 8.0,
                StructKind::Workshop => 4.0,
                StructKind::Dam | StructKind::Canal => 2.6,
                StructKind::Burrow => 2.8,
                _ => 3.5,
            };
            (Vec2::new(s.pos.x, s.pos.z), r)
        })
        .collect();
    let lodge2 = Vec2::new(structs[0].pos.x, structs[0].pos.z);
    let mut trees: Vec<TreeVis> = Vec::with_capacity(cfg.trees);
    // A hand-placed grove behind the lodge and a few stumps in front, so the hero view reads well.
    let back = lodge2 - (site.center - lodge2).normalize_or_zero();
    for i in 0..26 {
        let a = rng.range(-1.4, 1.4);
        let base = (back - lodge2).normalize_or_zero();
        let dir = Vec2::new(base.x * a.cos() - base.y * a.sin(), base.x * a.sin() + base.y * a.cos());
        let p = lodge2 + dir * rng.range(8.5, 19.0);
        if p.abs().max_element() > half || clear.iter().any(|(c, r)| p.distance(*c) < *r + 1.0) || gy(p) < lvl + 0.2 {
            continue;
        }
        let species = [Species::Maple, Species::Pine, Species::Aspen, Species::Maple, Species::Willow][i % 5];
        trees.push(TreeVis { pos: Vec3::new(p.x, gy(p) - 0.1, p.y), species, state: TreeState::Alive, scale: rng.range(0.85, 1.35), yaw: rng.range(0.0, 6.28) });
    }
    for _ in 0..5 {
        let a = rng.range(-1.2, 1.2);
        let base = (back - lodge2).normalize_or_zero();
        let dir = Vec2::new(base.x * a.cos() - base.y * a.sin(), base.x * a.sin() + base.y * a.cos());
        let p = lodge2 + dir * rng.range(6.8, 9.0);
        if gy(p) < lvl + 0.1 || clear.iter().skip(1).any(|(c, r)| p.distance(*c) < *r) {
            continue;
        }
        trees.push(TreeVis { pos: Vec3::new(p.x, gy(p) - 0.05, p.y), species: Species::Maple, state: TreeState::Stump, scale: rng.range(0.9, 1.3), yaw: rng.range(0.0, 6.28) });
    }

    let mut tries = 0;
    while trees.len() < cfg.trees && tries < cfg.trees * 6 {
        tries += 1;
        let p = Vec2::new(rng.range(-half, half), rng.range(-half, half));
        // Denser woods inland, thinner near the lodge clearing.
        if clear.iter().any(|(c, r)| p.distance(*c) < *r + 1.0) {
            continue;
        }
        let ground = gy(p);
        let near_lodge = p.distance(lodge2);
        if cfg.trees <= SceneConfig::NORMAL.trees && near_lodge > 12.0 && vnoise(p.x * 0.07, p.y * 0.07, seed) < 0.38 {
            continue; // meadows
        }
        let depth = lvl - ground;
        let state = if depth > 0.3 {
            if depth < 1.4 && rng.next() < 0.18 { TreeState::Snag } else { continue }
        } else if depth > -0.4 {
            continue; // soggy shore
        } else if near_lodge < 26.0 && rng.next() < 0.10 {
            TreeState::Stump
        } else {
            TreeState::Alive
        };
        let n = vnoise(p.x * 0.05 + 11.0, p.y * 0.05, seed ^ 7);
        let species = if ground > lvl + 6.0 && n > 0.35 {
            Species::Pine
        } else if ground < lvl + 1.6 {
            if rng.next() < 0.6 { Species::Willow } else { Species::Aspen }
        } else if n < 0.3 {
            Species::Pine
        } else if n < 0.65 {
            Species::Aspen
        } else {
            Species::Maple
        };
        let y = if state == TreeState::Snag { ground } else { ground - 0.1 };
        trees.push(TreeVis { pos: Vec3::new(p.x, y, p.y), species, state, scale: rng.range(0.75, 1.3), yaw: rng.range(0.0, 6.28), });
    }

    // ---- beavers -----------------------------------------------------------------------------
    let alive: Vec<usize> = (0..trees.len()).filter(|&i| trees[i].state == TreeState::Alive).collect();
    let pc = site.center;
    let mut homes = sites.clone();
    let mut extra = Rng(seed ^ 0xbea7);
    while homes.len() < 10.max(cfg.beavers / 4) {
        let a = extra.range(0.0, std::f32::consts::TAU);
        let (p, _) = site.shore(a, extra.range(1.0, 10.0));
        homes.push(p);
    }
    let mut beavers = Vec::with_capacity(cfg.beavers);
    for id in 0..cfg.beavers as u32 {
        let home = homes[id as usize % homes.len()] + Vec2::new(extra.range(-1.0, 1.0), extra.range(-1.0, 1.0));
        // k-th nearest alive tree so beavers at the same home spread over different trees.
        let mut cand: Vec<(f32, usize)> = alive
            .iter()
            .map(|&i| (Vec2::new(trees[i].pos.x, trees[i].pos.z).distance(home), i))
            .filter(|(d, _)| *d > 4.0)
            .collect();
        cand.sort_by(|a, b| a.0.total_cmp(&b.0));
        let rank = (id as usize / homes.len()) % 6;
        let Some(&(_, ti)) = cand.get(rank.min(cand.len().saturating_sub(1))) else { continue };
        let tp = Vec2::new(trees[ti].pos.x, trees[ti].pos.z);
        let to_home = (home - tp).normalize_or_zero();
        let stand = tp + to_home * 1.3;
        let dest = home + Vec2::new(extra.range(-1.2, 1.2), extra.range(-1.2, 1.2));
        let out = (home - pc).normalize_or_zero();
        let w1 = pc + out * (site.radius(out.x.atan2(out.y)) * 0.45);
        let w2 = pc + Vec2::new(-out.y, out.x) * (site.radius(out.x.atan2(out.y)) * 0.35) - out * 2.0;
        let hold_dir = |from: Vec2, to: Vec2| {
            let d = to - from;
            d.x.atan2(d.y)
        };
        let t_gnaw = extra.range(6.0, 10.0);
        let route = Route::new(
            vec![dest, stand, stand, dest, dest, w1, w2],
            vec![Leg::Move, Leg::Hold(Pose::Gnaw), Leg::Carry, Leg::Hold(Pose::Idle), Leg::Move, Leg::Move, Leg::Move],
            vec![0.0, t_gnaw, 0.0, extra.range(2.0, 4.0), 0.0, 0.0, 0.0],
            vec![0.0, hold_dir(stand, tp), 0.0, hold_dir(dest, pc), 0.0, 0.0, 0.0],
            extra.range(0.0, 200.0),
        );
        beavers.push(BeaverVis { id, state: BeaverState { pos: dest, yaw: 0.0, pose: Pose::Idle, moving: false }, route: Some(route) });
    }

    RenderScene { trees, structs, beavers, pond: Some(Pond { center: site.center, level: site.level }), stub: true }
}

const WALK_SPEED: f32 = 1.9;

impl Route {
    fn new(pts: Vec<Vec2>, legs: Vec<Leg>, hold: Vec<f32>, hold_yaw: Vec<f32>, offset: f32) -> Route {
        let n = pts.len();
        let durs: Vec<f32> = (0..n)
            .map(|i| match legs[i] {
                Leg::Hold(_) => hold[i],
                _ => (pts[(i + 1) % n] - pts[i]).length() / WALK_SPEED,
            })
            .collect();
        let total = durs.iter().sum::<f32>().max(1.0);
        Route { pts, legs, durs, hold_yaw, total, offset }
    }

    /// Where the itinerary puts the beaver at `time` seconds. `water_at` says whether a point is in water.
    pub fn eval(&self, time: f32, in_water: impl Fn(Vec2) -> bool) -> BeaverState {
        let mut u = (time + self.offset).rem_euclid(self.total);
        let n = self.pts.len();
        for i in 0..n {
            let d = self.durs[i];
            if u <= d || i == n - 1 {
                let f = if d > 1e-4 { (u / d).clamp(0.0, 1.0) } else { 1.0 };
                let (a, b) = (self.pts[i], self.pts[(i + 1) % n]);
                return match self.legs[i] {
                    Leg::Hold(pose) => BeaverState { pos: a, yaw: self.hold_yaw[i], pose, moving: false },
                    leg => {
                        let pos = a.lerp(b, f);
                        let dir = b - a;
                        let swimming = in_water(pos);
                        let pose = if swimming {
                            Pose::Swim
                        } else if matches!(leg, Leg::Carry) {
                            Pose::Carry
                        } else {
                            Pose::Walk
                        };
                        BeaverState { pos, yaw: dir.x.atan2(dir.y), pose, moving: true }
                    }
                };
            }
            u -= d;
        }
        BeaverState { pos: self.pts[0], yaw: 0.0, pose: Pose::Idle, moving: false }
    }
}
