use crate::camera::OrbitCamera;
use crate::sim_driver::SimClock;
use crate::state::Sim;
use crate::terrain_render::CELL_SIZE;
use bevy::prelude::*;
use sim::colony::BeaverId;
use sim::grid::HeightGrid;

pub type Cell = (usize, usize);

#[derive(Resource, Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Selection {
    #[default]
    None,
    Cell(Cell),
    Beaver(BeaverId),
    Tree(sim::resources::ResourceId),
    Site(sim::buildings::BuildingId),
    /// A wolf, by its id in `sim::predators`.
    Wolf(u32),
    /// A wild animal, by its id in `sim::wildlife`.
    Animal(u32),
}

impl From<crate::picking::Target> for Selection {
    fn from(t: crate::picking::Target) -> Self {
        use crate::picking::Target;
        match t {
            Target::Building(id) => Self::Site(id),
            Target::Beaver(id) => Self::Beaver(id),
            Target::Resource(id) => Self::Tree(id),
        }
    }
}

impl Selection {
    pub fn tip_key(self) -> u64 {
        match self {
            Selection::None => 0,
            Selection::Beaver(b) => 1 << 60 | b.0,
            Selection::Site(b) => 2 << 60 | b.0 as u64,
            Selection::Tree(r) => 3 << 60 | r.0 as u64,
            Selection::Cell(c) => 5 << 60 | ((c.0 as u64) << 20) | c.1 as u64,
            Selection::Wolf(id) => 6 << 60 | id as u64,
            Selection::Animal(id) => 7 << 60 | id as u64,
        }
    }
}

pub fn cell_world(h: &HeightGrid, pos: (f32, f32)) -> Vec3 {
    let (ox, oz) = (h.width() as f32 * CELL_SIZE / 2.0, h.height() as f32 * CELL_SIZE / 2.0);
    Vec3::new(pos.0 * CELL_SIZE - ox, height_at(h, pos.0, pos.1), pos.1 * CELL_SIZE - oz)
}

pub fn world_cell(h: &HeightGrid, p: Vec3) -> (f32, f32) {
    let (ox, oz) = (h.width() as f32 * CELL_SIZE / 2.0, h.height() as f32 * CELL_SIZE / 2.0);
    ((p.x + ox) / CELL_SIZE, (p.z + oz) / CELL_SIZE)
}

pub fn height_at(h: &HeightGrid, x: f32, y: f32) -> f32 {
    bilinear_at(h, x, y)
}

pub fn ray_heightfield(origin: Vec3, dir: Vec3, h: &HeightGrid) -> Option<(Cell, Vec3, f32)> {
    ray_level(origin, dir, h, |x, y| height_at(h, x, y))
}

/// Where the ray meets the ground or, over water, the water's surface: the place the pointer is
/// seen to touch. Testing the ground alone lands a deep pond's cells away from the pointer.
pub fn ray_surface(origin: Vec3, dir: Vec3, h: &HeightGrid, depth: &sim::grid::Grid<f32>) -> Option<(Cell, Vec3, f32)> {
    ray_level(origin, dir, h, |x, y| height_at(h, x, y) + bilinear_at(depth, x, y))
}

fn bilinear_at(g: &sim::grid::Grid<f32>, x: f32, y: f32) -> f32 {
    let (w, d) = (g.width(), g.height());
    let x = (x - crate::terrain_render::GRID_OFFSET).clamp(0.0, (w - 1) as f32);
    let y = (y - crate::terrain_render::GRID_OFFSET).clamp(0.0, (d - 1) as f32);
    let (x0, y0) = (x.floor() as usize, y.floor() as usize);
    let (x1, y1) = ((x0 + 1).min(w - 1), (y0 + 1).min(d - 1));
    let (fx, fy) = (x - x0 as f32, y - y0 as f32);
    let v = |x, y| g.get(x, y).copied().unwrap_or(0.0);
    let top = v(x0, y0) * (1.0 - fx) + v(x1, y0) * fx;
    let bot = v(x0, y1) * (1.0 - fx) + v(x1, y1) * fx;
    top * (1.0 - fy) + bot * fy
}

fn ray_level(origin: Vec3, dir: Vec3, h: &HeightGrid, level: impl Fn(f32, f32) -> f32) -> Option<(Cell, Vec3, f32)> {
    let dir = dir.try_normalize()?;
    let (w, d) = (h.width() as f32, h.height() as f32);
    let max_t = 4000.0;
    let step = 0.5;
    let inside = |p: Vec3| {
        let (cx, cy) = world_cell(h, p);
        cx >= -0.5 && cy >= -0.5 && cx <= w - 0.5 && cy <= d - 0.5
    };
    let below = |p: Vec3| {
        let (cx, cy) = world_cell(h, p);
        p.y <= level(cx, cy)
    };
    let mut t = 0.0;
    let mut prev_t = 0.0;
    while t < max_t {
        let p = origin + dir * t;
        if inside(p) && below(p) {
            if t == 0.0 {
                return None;
            }
            let (mut lo, mut hi) = (prev_t, t);
            for _ in 0..24 {
                let mid = 0.5 * (lo + hi);
                if below(origin + dir * mid) { hi = mid } else { lo = mid }
            }
            let hit = origin + dir * hi;
            let (cx, cy) = world_cell(h, hit);
            let cell = ((cx.floor().max(0.0) as usize).min(h.width() - 1), (cy.floor().max(0.0) as usize).min(h.height() - 1));
            return Some((cell, hit, hi));
        }
        prev_t = t;
        t += step;
    }
    None
}

pub use sim::site::{can_settle, score as site_hint, SettleError};

#[derive(Resource, Default)]
pub struct Flow {
    pub focus: Option<Vec3>,
    pub arrive_left: f32,
    /// A new colony has just settled: the HUD shows its one welcome card.
    pub welcome: bool,
}

pub fn settle(sim: &mut sim::SimState, flow: &mut Flow, cell: Cell) -> Result<(), String> {
    if sim.is_running() {
        return Err("The colony can only be founded while choosing the settling place".into());
    }
    can_settle(sim, cell).map_err(|e| format!("{e:?}"))?;
    sim.apply(sim::Command::Settle { cell }).map_err(|e| e.to_string())?;
    after_settle(sim, flow, cell);
    // Every new campaign settles here, by Play or by the player's own pick: it opens on the
    // welcome card.
    flow.welcome = true;
    Ok(())
}

pub fn after_settle(sim: &sim::SimState, flow: &mut Flow, cell: Cell) {
    let pos = (cell.0 as f32 + 0.5, cell.1 as f32 + 0.5);
    flow.arrive_left = 3.0;
    let n = sim.colony.beavers.len().max(1) as f32;
    let centre = sim.colony.beavers.iter().fold((0.0, 0.0), |p, b| (p.0 + b.pos.0 / n, p.1 + b.pos.1 / n));
    flow.focus = Some(cell_world(&sim.height, if n > 0.0 { centre } else { pos }));
}

pub struct FlowPlugin;

impl Plugin for FlowPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Flow>()
            .init_resource::<Selection>()
            .add_systems(Update, (watch_collapse, ease_camera).chain());
    }
}

fn watch_collapse(mut sim: ResMut<Sim>, mut clock: ResMut<SimClock>, mut notices: ResMut<crate::hud::Notices>, mut wolf_cues: ResMut<crate::hud::WolfCues>, time: Res<Time>, mut names: Local<std::collections::HashMap<u64, String>>) {
    use crate::hud::{NoticeAt, BAD, GOOD, INK, MUTED};
    use crate::icons::Icon;
    // Leave the world untouched (and so unchanged, for what is drawn from it) when it has
    // nothing to tell.
    let events = if sim.0.colony.events.is_empty() { Vec::new() } else { sim.0.colony.take_events() };
    let now = time.elapsed_secs();
    // A beaver that has died is gone from the colony by now: its name is kept from before.
    let name_of = |names: &std::collections::HashMap<u64, String>, s: &sim::SimState, id: sim::colony::BeaverId| {
        s.colony.beaver(id).map(|b| b.name.clone()).or_else(|| names.get(&id.0).cloned()).unwrap_or_default()
    };
    for e in &events {
        use sim::colony::Event as E;
        let s = &sim.0;
        match &e.event {
            E::Died { id, cause, at } => {
                use sim::colony::DeathCause as C;
                let (icon, why) = match cause {
                    C::Starvation => (crate::hud::words::good_icon(sim::buildings::Good::food()), "starved"),
                    C::Cold => (Icon::Winter, "froze"),
                    C::Injury => (Icon::Health, "died of injury"),
                    C::Exhaustion => (Icon::Hourglass, "worked to death"),
                    C::OldAge => (Icon::Adult, "died of old age"),
                    C::Predator => (Icon::Wolf, "killed by the wolf"),
                };
                notices.push_at(vec![(Icon::Beaver, BAD), (icon, BAD)], format!("{} {why}", name_of(&names, s, *id)), now, Some(NoticeAt::Spot(*at)));
                if matches!(cause, C::Predator) {
                    wolf_cues.kills += 1;
                }
            }
            E::Born { id, .. } => notices.push_at(vec![(Icon::Kit, GOOD)], format!("{} born", name_of(&names, s, *id)), now, Some(NoticeAt::Beaver(*id))),
            E::Matured { id, stage: sim::colony::Stage::Adult } => notices.push_at(vec![(Icon::Adult, INK)], format!("{} grown", name_of(&names, s, *id)), now, Some(NoticeAt::Beaver(*id))),
            E::Departed { id } => notices.push_at(vec![(Icon::Beaver, MUTED), (Icon::Back, MUTED)], format!("{} left", name_of(&names, s, *id)), now, None),
            E::Immigrated { id } => notices.push_at(vec![(Icon::Beaver, GOOD), (Icon::Plus, GOOD)], format!("{} joined", name_of(&names, s, *id)), now, Some(NoticeAt::Beaver(*id))),
            E::LeftHome { id, .. } => notices.push_at(vec![(Icon::Adult, INK), (Icon::Home, INK)], format!("{} moved out", name_of(&names, s, *id)), now, Some(NoticeAt::Beaver(*id))),
            // An epithet earned: the beaver by its new name, with the work it was earned in.
            E::Epithet { id } => {
                if let (Some(b), Some(e)) = (s.colony.beaver(*id), s.deeds.get(*id).and_then(|r| r.epithet.as_ref())) {
                    // A greater epithet replacing one held: the old name, and the new beside it.
                    let (icons, text) = match &e.was {
                        Some(was) => (vec![(crate::hud::job_icon(e.job), GOOD), (Icon::Up, GOOD)], format!("{} {was} → {}", b.name, e.word)),
                        None => (vec![(crate::hud::job_icon(e.job), GOOD)], format!("{} {}", b.name, e.word)),
                    };
                    notices.push_at(icons, text, now, Some(NoticeAt::Beaver(*id)));
                }
            }
            E::NewFamily { family, a, .. } => {
                let name = s.colony.family(*family).map(|f| f.name.clone()).unwrap_or_default();
                notices.push_at(vec![(Icon::Home, GOOD)], format!("{name} family"), now, Some(NoticeAt::Beaver(*a)));
            }
            _ => {}
        }
    }
    if names.len() != sim.0.colony.beavers.len() || !events.is_empty() {
        *names = sim.0.colony.beavers.iter().map(|b| (b.id.0, b.name.clone())).collect();
    }
    if sim.0.colony.is_collapsed() && !clock.paused {
        clock.paused = true;
    }
}

fn ease_camera(time: Res<Time>, mut flow: ResMut<Flow>, mut orbit: ResMut<OrbitCamera>) {
    if flow.arrive_left > 0.0 {
        flow.arrive_left = (flow.arrive_left - time.delta_secs().min(0.1)).max(0.0);
    }
    let Some(f) = flow.focus else { return };
    let arriving = flow.arrive_left > 0.0;
    let (next, arrived) = ease_step(*orbit, f, (time.delta_secs() * 2.5).min(1.0), arriving);
    if arrived {
        flow.focus = None;
    }
    *orbit = next;
}

pub fn ease_step(orbit: OrbitCamera, focus: Vec3, k: f32, settled: bool) -> (OrbitCamera, bool) {
    let mut next = orbit;
    next.target = orbit.target.lerp(focus, k);
    if settled {
        next.distance += (38.0 - next.distance) * k;
    }
    let arrived = next.target.distance(focus) < 0.05;
    if arrived {
        next.target = focus;
    }
    (next, arrived)
}
