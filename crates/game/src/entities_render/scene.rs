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
    Climb,
    Work,
    Rest,
}

#[derive(Clone, Copy, Debug)]
pub struct BeaverState {
    pub pos: Vec2,
    pub yaw: f32,
    pub pose: Pose,
    pub load: Option<(sim::buildings::Good, f32)>,
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
    BargeEmpty,
    FoodCache,
    ScentMound,
    GuardPost,
    ForagerHut,
    MudPit,
    TreeNursery,
    Warehouse,
    Storehouse,
    LoggingCamp,
    QuarryCamp,
    ForesterLodge,
    Mill,
    Lock,
}

#[derive(Clone, Copy, Debug)]
pub struct StructVis {
    pub kind: StructKind,
    pub pos: Vec3,
    pub yaw: f32,
    pub len: f32,
    pub progress: f32,
}

#[derive(Clone, Copy, Debug)]
pub struct Pond {
    pub center: Vec2,
    pub level: f32,
    pub shore_r: [f32; RAYS],
}

impl Pond {
    pub fn radius(&self, a: f32) -> f32 {
        let t = a.rem_euclid(std::f32::consts::TAU) / std::f32::consts::TAU * RAYS as f32;
        let (i, f) = (t.floor() as usize % RAYS, t.fract());
        self.shore_r[i] * (1.0 - f) + self.shore_r[(i + 1) % RAYS] * f
    }

    pub fn contains(&self, p: Vec2, margin: f32) -> bool {
        let d = p - self.center;
        d.length() < self.radius(d.x.atan2(d.y)) + margin
    }

    pub fn swimmable(&self, h: &HeightGrid, p: Vec2) -> bool {
        self.contains(p, 0.0) && height_at(h, p.x, p.y) < self.level - 0.15
    }
}

#[derive(Clone, Copy, Debug)]
enum Leg {
    Move,
    Carry,
    Hold(Pose),
}

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
    pub state: BeaverState,
    pub route: Option<Route>,
}

#[derive(Resource)]
pub struct RenderScene {
    pub trees: Vec<TreeVis>,
    pub structs: Vec<StructVis>,
    pub beavers: Vec<BeaverVis>,
    pub pond: Option<Pond>,
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

fn vnoise(x: f32, z: f32, seed: u32) -> f32 {
    let (xi, zi) = (x.floor(), z.floor());
    let (fx, fz) = (x - xi, z - zi);
    let (sx, sz) = (fx * fx * (3.0 - 2.0 * fx), fz * fz * (3.0 - 2.0 * fz));
    let h = |dx: i32, dz: i32| hash(xi as i32 + dx, zi as i32 + dz, seed);
    let a = h(0, 0) + (h(1, 0) - h(0, 0)) * sx;
    let b = h(0, 1) + (h(1, 1) - h(0, 1)) * sx;
    a + (b - a) * sz
}

pub const RAYS: usize = 24;
const SHORE_MIN: f32 = 6.0;
const SHORE_MAX: f32 = 20.0;
const DAM_MAX_DROP: f32 = 1.1;

struct Site {
    center: Vec2,
    level: f32,
    shore_r: [f32; RAYS],
    open: [bool; RAYS],
    a0: f32,
    mean_r: f32,
}

fn ray_angle(i: usize) -> f32 { i as f32 / RAYS as f32 * std::f32::consts::TAU }

fn ray_dir(i: usize) -> Vec2 {
    let a = ray_angle(i);
    Vec2::new(a.sin(), a.cos())
}

fn banks(h: &HeightGrid, c: Vec2, level: f32) -> [Option<f32>; RAYS] {
    std::array::from_fn(|i| {
        let dir = ray_dir(i);
        let mut r = 1.0;
        while r <= SHORE_MAX + 2.0 {
            if height_at(h, c.x + dir.x * r, c.y + dir.y * r) >= level {
                return Some(r);
            }
            r += 0.5;
        }
        None
    })
}

fn close_gaps(b: &[Option<f32>; RAYS]) -> Option<[f32; RAYS]> {
    if b.iter().all(|r| r.is_none()) {
        return None;
    }
    Some(std::array::from_fn(|i| match b[i] {
        Some(r) => r,
        None => {
            let (mut l, mut dl) = (i, 0);
            while b[l].is_none() {
                l = (l + RAYS - 1) % RAYS;
                dl += 1;
            }
            let (mut r, mut dr) = (i, 0);
            while b[r].is_none() {
                r = (r + 1) % RAYS;
                dr += 1;
            }
            let (rl, rr) = (b[l].unwrap_or(10.0), b[r].unwrap_or(10.0));
            rl + (rr - rl) * dl as f32 / (dl + dr) as f32
        }
    }))
}

impl Site {
    fn find(h: &HeightGrid) -> Site {
        let size = (h.width().min(h.height())) as f32 * CELL;
        let lim = (size * 0.5 - SHORE_MAX - 14.0).max(4.0);
        let mut best: Option<(f32, Vec2, f32, [f32; RAYS], [bool; RAYS])> = None;
        let step = 2.0 * CELL;
        let mut z = -lim;
        while z <= lim {
            let mut x = -lim;
            while x <= lim {
                let c = Vec2::new(x, z);
                x += step;
                let g0 = height_at(h, c.x, c.y);
                for depth in [0.7f32, 0.9, 1.2, 1.5, 1.9] {
                    let level = g0 + depth;
                    let b = banks(h, c, level);
                    let closed = b.iter().filter(|r| r.is_some()).count();
                    if closed < RAYS / 2 {
                        continue;
                    }
                    let Some(mut shore) = close_gaps(&b) else { continue };
                    let mut ok = true;
                    for i in 0..RAYS {
                        shore[i] = shore[i].clamp(SHORE_MIN, SHORE_MAX);
                        if b[i].is_none() {
                            let p = c + ray_dir(i) * shore[i];
                            ok &= height_at(h, p.x, p.y) >= level - DAM_MAX_DROP;
                        }
                    }
                    let mean = shore.iter().sum::<f32>() / RAYS as f32;
                    if !ok || !(8.5..=16.0).contains(&mean) {
                        continue;
                    }
                    let score = closed as f32 + mean * 0.6 - c.length() * 0.05;
                    if best.as_ref().is_none_or(|bb| score > bb.0) {
                        best = Some((score, c, level, shore, std::array::from_fn(|i| b[i].is_none())));
                    }
                }
            }
            z += step;
        }
        let (center, level, mut shore_r, open) = match best {
            Some((_, c, l, s, o)) => (c, l, s, o),
            None => {
                let level = height_at(h, 0.0, 0.0) + 0.8;
                let b = banks(h, Vec2::ZERO, level);
                let s = close_gaps(&b).unwrap_or([10.0; RAYS]).map(|r| r.clamp(SHORE_MIN, 12.0));
                (Vec2::ZERO, level, s, std::array::from_fn(|i| b[i].is_none()))
            }
        };
        for _ in 0..2 {
            let prev = shore_r;
            for i in 0..RAYS {
                shore_r[i] = (prev[(i + RAYS - 1) % RAYS] + prev[i] * 2.0 + prev[(i + 1) % RAYS]) / 4.0;
            }
        }
        let mean_r = shore_r.iter().sum::<f32>() / RAYS as f32;
        let mut site = Site { center, level, shore_r, open, a0: 0.0, mean_r };
        let mut best_score = f32::MAX;
        for i in 0..RAYS {
            if open[i] || open[(i + 1) % RAYS] || open[(i + RAYS - 1) % RAYS] {
                continue;
            }
            let a = ray_angle(i);
            let (p, o) = site.shore(a, 0.0);
            let q = p + o * 5.0;
            if q.abs().max_element() > size * 0.5 - 12.0 {
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

    fn is_open(&self, a: f32) -> bool {
        let t = a.rem_euclid(std::f32::consts::TAU) / std::f32::consts::TAU * RAYS as f32;
        self.open[t.round() as usize % RAYS]
    }

    fn open_runs(&self) -> Vec<(usize, usize)> {
        let Some(start) = (0..RAYS).find(|&i| !self.open[i]) else { return vec![(0, RAYS)] };
        let mut runs = Vec::new();
        let mut k = 0;
        while k < RAYS {
            let i = (start + k) % RAYS;
            if self.open[i] {
                let mut n = 0;
                while n < RAYS && self.open[(i + n) % RAYS] {
                    n += 1;
                }
                runs.push((i, n));
                k += n;
            } else {
                k += 1;
            }
        }
        runs
    }

    fn pond(&self) -> Pond { Pond { center: self.center, level: self.level, shore_r: self.shore_r } }

    fn radius(&self, a: f32) -> f32 { self.pond().radius(a) }

    fn shore(&self, a: f32, inset: f32) -> (Vec2, Vec2) {
        let dir = Vec2::new(a.sin(), a.cos());
        (self.center + dir * (self.radius(a) + inset), dir)
    }

    fn along(&self, arc: f32, inset: f32) -> (Vec2, Vec2) {
        let mut a = arc;
        for k in 1..(self.mean_r * std::f32::consts::TAU) as i32 {
            if !self.is_open(self.a0 + a / self.mean_r) {
                break;
            }
            let d = (k + 1) / 2;
            a = arc + if k % 2 == 1 { d as f32 } else { -d as f32 };
        }
        self.shore(self.a0 + a / self.mean_r, inset)
    }
}

pub fn lodge_view(h: &HeightGrid) -> (Vec3, f32) {
    let site = Site::find(h);
    let (p, o) = site.along(0.0, 3.6);
    let y = height_at(h, p.x, p.y);
    let t = p - o * 3.0;
    (Vec3::new(t.x, y + 1.2, t.y), ((-o.x).atan2(-o.y) + 0.4).rem_euclid(std::f32::consts::TAU))
}

pub fn scene_from_sim(sim: &SimState, cfg: SceneConfig) -> RenderScene {
    stub_scene(sim, cfg)
}

fn stub_scene(sim: &SimState, cfg: SceneConfig) -> RenderScene {
    let h = &sim.height;
    let site = Site::find(h);
    let seed = sim.seed as u32 ^ 0x5eed;
    let gy = |p: Vec2| height_at(h, p.x, p.y);
    let foot = |p: Vec2, r: f32| {
        (0..8).fold(gy(p), |m, k| {
            let a = k as f32 * std::f32::consts::FRAC_PI_4;
            m.min(gy(p + Vec2::new(a.sin(), a.cos()) * r))
        })
    };
    let lvl = site.level;
    let mut structs: Vec<StructVis> = Vec::new();
    let yaw_of = |dir: Vec2| dir.x.atan2(dir.y);
    let mut sites: Vec<Vec2> = Vec::new();

    let mut taken: Vec<(Vec2, f32)> = Vec::new();
    let mut place = |arc: f32, inset: f32, r: f32, dir: f32| -> (f32, Vec2, Vec2) {
        let a = (0..40)
            .map(|k| arc + dir * k as f32)
            .find(|&a| {
                let (p, _) = site.along(a, inset);
                taken.iter().all(|(c, cr)| p.distance(*c) >= cr + r)
            })
            .unwrap_or(arc);
        let (p, o) = site.along(a, inset);
        taken.push((p, r));
        (a, p, o)
    };

    let (_, lp, lo) = place(0.0, 3.8, 3.6, 1.0);
    structs.push(StructVis { kind: StructKind::Lodge, pos: Vec3::new(lp.x, foot(lp, 2.6).max(lvl - 0.3), lp.y), yaw: yaw_of(-lo), len: 1.0, progress: 1.0 });
    sites.push(lp - lo * 4.2);
    let (_, p, o) = place(8.5, 0.3, 3.0, 1.0);
    structs.push(StructVis { kind: StructKind::Dock, pos: Vec3::new(p.x, lvl, p.y), yaw: yaw_of(-o), len: 6.0, progress: 1.0 });
    sites.push(p + o * 0.5);
    let bp = p - o * 4.6 + Vec2::new(-o.y, o.x) * 3.2;
    structs.push(StructVis { kind: StructKind::Barge, pos: Vec3::new(bp.x, lvl, bp.y), yaw: yaw_of(-o) + 0.12, len: 7.0, progress: 1.0 });
    let (wa, p, o) = place(-9.0, 5.5, 3.4, -1.0);
    structs.push(StructVis { kind: StructKind::Workshop, pos: Vec3::new(p.x, foot(p, 0.9), p.y), yaw: yaw_of(-o), len: 1.0, progress: 1.0 });
    sites.push(p - o * 3.5);
    let (_, p, o) = place(wa - 4.2, 1.3, 2.4, -1.0);
    structs.push(StructVis { kind: StructKind::WaterWheel, pos: Vec3::new(p.x, lvl, p.y), yaw: yaw_of(-o), len: 1.0, progress: 1.0 });
    let (_, p, o) = place(-17.0, 4.5, 3.6, -1.0);
    structs.push(StructVis { kind: StructKind::Lodge, pos: Vec3::new(p.x, foot(p, 2.6).max(lvl - 0.3), p.y), yaw: yaw_of(-o), len: 1.0, progress: 0.5 });
    sites.push(p - o * 4.5);
    let (_, p, o) = place(-27.0, 4.5, 3.4, -1.0);
    structs.push(StructVis { kind: StructKind::Workshop, pos: Vec3::new(p.x, foot(p, 0.9), p.y), yaw: yaw_of(-o), len: 1.0, progress: 0.3 });
    let mut runs: Vec<(f32, f32)> = site
        .open_runs()
        .iter()
        .map(|&(i, n)| {
            let half = std::f32::consts::PI / RAYS as f32;
            (ray_angle(i) - half * 1.6, ray_angle(i + n - 1) + half * 1.6)
        })
        .collect();
    if runs.is_empty() {
        let a = site.a0 + 15.0 / site.mean_r;
        runs.push((a, a + 20.0 / site.mean_r));
    }
    let longest = (0..runs.len()).max_by(|&a, &b| (runs[a].1 - runs[a].0).total_cmp(&(runs[b].1 - runs[b].0))).unwrap_or(0);
    for (ri, &(a0, a1)) in runs.iter().enumerate() {
        let n = (((a1 - a0) * site.mean_r) / 4.0).ceil().max(1.0) as usize;
        for k in 0..n {
            let (a, _) = site.shore(a0 + (a1 - a0) * k as f32 / n as f32, -0.4);
            let (b, _) = site.shore(a0 + (a1 - a0) * (k + 1) as f32 / n as f32, -0.4);
            let m = (a + b) * 0.5;
            let d = b - a;
            structs.push(StructVis {
                kind: StructKind::Dam,
                pos: Vec3::new(m.x, gy(m).min(lvl - 0.1), m.y),
                yaw: -d.y.atan2(d.x),
                len: d.length().clamp(2.5, 5.0) + 0.4,
                progress: if ri == longest && k == n - 1 { 0.4 } else { 1.0 },
            });
        }
        let (p, o) = site.shore((a0 + a1) * 0.5, 1.0);
        sites.push(p - o * 2.0);
    }
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
    for (arc, inset) in [(-22.0f32, 6.5), (-24.5, 8.5), (28.0, 6.5)] {
        let (_, p, o) = place(arc, inset, 1.8, arc.signum());
        structs.push(StructVis { kind: StructKind::Burrow, pos: Vec3::new(p.x, foot(p, 1.2), p.y), yaw: yaw_of(-o), len: 1.0, progress: 1.0 });
        sites.push(p - o * 1.6);
    }

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
        if clear.iter().any(|(c, r)| p.distance(*c) < *r + 1.0) {
            continue;
        }
        let ground = gy(p);
        let near_lodge = p.distance(lodge2);
        if cfg.trees <= SceneConfig::NORMAL.trees && near_lodge > 12.0 && vnoise(p.x * 0.07, p.y * 0.07, seed) < 0.38 {
            continue;
        }
        let (cx, cy) = (p.x / crate::terrain_render::CELL_SIZE + sim.height.width() as f32 / 2.0, p.y / crate::terrain_render::CELL_SIZE + sim.height.height() as f32 / 2.0);
        let wet_near = (-1..=1).any(|dy| (-1..=1).any(|dx| crate::terrain_render::at(&sim.water.depth, cx.round() as i32 + dx, cy.round() as i32 + dy) > crate::terrain_render::WET_EPS));
        if wet_near && !site.pond().contains(p, 0.8) {
            continue;
        }
        let depth = if site.pond().contains(p, 0.8) { lvl - ground } else { -1.0 };
        let state = if depth > 0.3 {
            if depth < 1.4 && rng.next() < 0.18 { TreeState::Snag } else { continue }
        } else if depth > -0.4 {
            continue;
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
        let ring = extra.range(0.0, std::f32::consts::TAU);
        let home = homes[id as usize % homes.len()] + Vec2::new(ring.sin(), ring.cos()) * extra.range(0.6, 2.4);
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
            vec![Leg::Move, Leg::Hold(Pose::Gnaw), Leg::Carry, Leg::Hold(if id % 3 == 0 { Pose::Work } else { Pose::Idle }), Leg::Move, Leg::Move, Leg::Move],
            vec![0.0, t_gnaw, 0.0, extra.range(2.0, 4.0), 0.0, 0.0, 0.0],
            vec![0.0, hold_dir(stand, tp), 0.0, hold_dir(dest, pc), 0.0, 0.0, 0.0],
            extra.range(0.0, 200.0),
        );
        beavers.push(BeaverVis { state: BeaverState { pos: dest, yaw: 0.0, pose: Pose::Idle, load: None }, route: Some(route) });
    }

    RenderScene { trees, structs, beavers, pond: Some(site.pond()) }
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

    pub fn eval(&self, time: f32, in_water: impl Fn(Vec2) -> bool) -> BeaverState {
        let mut u = (time + self.offset).rem_euclid(self.total);
        let n = self.pts.len();
        for i in 0..n {
            let d = self.durs[i];
            if u <= d || i == n - 1 {
                let f = if d > 1e-4 { (u / d).clamp(0.0, 1.0) } else { 1.0 };
                let (a, b) = (self.pts[i], self.pts[(i + 1) % n]);
                return match self.legs[i] {
                    Leg::Hold(pose) => BeaverState { pos: a, yaw: self.hold_yaw[i], pose, load: None },
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
                        BeaverState { pos, yaw: dir.x.atan2(dir.y), pose, load: matches!(leg, Leg::Carry).then_some((sim::buildings::Good::Sticks, 3.0)) }
                    }
                };
            }
            u -= d;
        }
        BeaverState { pos: self.pts[0], yaw: 0.0, pose: Pose::Idle, load: None }
    }
}
