use crate::camera::OrbitCamera;
use crate::sim_driver::SimClock;
use crate::state::Sim;
use crate::terrain_render::CELL_SIZE;
use bevy::prelude::*;
use sim::colony::{BeaverId, LodgeId};
use sim::grid::HeightGrid;
use sim::worldgen::World;
use std::collections::VecDeque;

pub type Cell = (usize, usize);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Scouting,
    Settled,
    GameOver,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FlowEvent {
    Settle,
    Collapse,
}

impl Phase {
    pub fn on(self, ev: FlowEvent) -> Option<Phase> {
        use FlowEvent::*;
        use Phase::*;
        match (self, ev) {
            (Scouting, Settle) => Some(Settled),
            (Settled, Collapse) => Some(GameOver),
            _ => None,
        }
    }
}

#[derive(Resource, Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Selection {
    #[default]
    None,
    Cell(Cell),
    Beaver(BeaverId),
    Lodge(LodgeId),
    Tree(sim::resources::ResourceId),
    Site(sim::buildings::BuildingId),
    Barge(sim::buildings::BargeId),
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
    let (w, d) = (h.width(), h.height());
    let x = x.clamp(0.0, (w - 1) as f32);
    let y = y.clamp(0.0, (d - 1) as f32);
    let (x0, y0) = (x.floor() as usize, y.floor() as usize);
    let (x1, y1) = ((x0 + 1).min(w - 1), (y0 + 1).min(d - 1));
    let (fx, fy) = (x - x0 as f32, y - y0 as f32);
    let g = |x, y| *h.get(x, y).expect("clamped in bounds");
    let top = g(x0, y0) * (1.0 - fx) + g(x1, y0) * fx;
    let bot = g(x0, y1) * (1.0 - fx) + g(x1, y1) * fx;
    top * (1.0 - fy) + bot * fy
}

pub fn ray_heightfield(origin: Vec3, dir: Vec3, h: &HeightGrid) -> Option<(Cell, Vec3, f32)> {
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
        p.y <= height_at(h, cx, cy)
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
            let cell = ((cx.round().max(0.0) as usize).min(h.width() - 1), (cy.round().max(0.0) as usize).min(h.height() - 1));
            return Some((cell, hit, hi));
        }
        prev_t = t;
        t += step;
    }
    None
}

#[allow(unused_imports)]
pub use sim::fog::{open_at, EXPLORED};
pub use sim::site::{can_settle, score as site_hint, SettleError};

#[derive(Resource)]
pub struct Flow {
    pub phase: Phase,
    pub focus: Option<Vec3>,
    pub message: String,
    pub log: VecDeque<String>,
    pub history: Vec<(f64, usize, f32)>,
    pub real_secs: f32,
    pub generation: u32,
    pub settled_at: Option<f32>,
    pub lodge_cell: Option<Cell>,
}

impl Flow {
    pub fn new(_world: &World) -> Flow {
        Flow {
            phase: Phase::Scouting,
            focus: None,
            message: String::new(),
            log: VecDeque::new(),
            history: Vec::new(),
            real_secs: 0.0,
            generation: 0,
            settled_at: None,
            lodge_cell: None,
        }
    }

    pub fn push_log(&mut self, s: impl Into<String>) {
        self.log.push_back(s.into());
        while self.log.len() > 40 {
            self.log.pop_front();
        }
    }

    pub fn fire(&mut self, ev: FlowEvent) -> bool {
        match self.phase.on(ev) {
            Some(p) => {
                self.phase = p;
                true
            }
            None => false,
        }
    }
}

pub fn settle(sim: &mut sim::SimState, flow: &mut Flow, cell: Cell) -> Result<(), String> {
    if flow.phase != Phase::Scouting {
        return Err("The colony can only be founded while choosing the settling place".into());
    }
    can_settle(sim, cell).map_err(|e| format!("{e:?}"))?;
    sim.apply(sim::Command::Settle { cell }).map_err(|e| e.to_string())?;
    after_settle(sim, flow, cell);
    Ok(())
}

pub fn after_settle(sim: &sim::SimState, flow: &mut Flow, cell: Cell) {
    let pos = (cell.0 as f32 + 0.5, cell.1 as f32 + 0.5);
    flow.fire(FlowEvent::Settle);
    flow.settled_at = Some(flow.real_secs);
    flow.lodge_cell = Some(cell);
    flow.focus = Some(cell_world(&sim.height, pos));
    let name = sim.colony.families.first().map(|f| f.name.clone()).unwrap_or_default();
    flow.push_log(format!("The {name} family settled at ({}, {})", cell.0, cell.1));
    flow.message.clear();
}

pub struct FlowPlugin;

impl Plugin for FlowPlugin {
    fn build(&self, app: &mut App) {
        let flow = Flow::new(&app.world().resource::<Sim>().0.world);
        app.insert_resource(flow)
            .init_resource::<Selection>()
            .add_systems(
                Update,
                (advance_flow, watch_collapse, sample_history, ease_camera)
                    .chain(),
            );
    }
}

fn advance_flow(
    time: Res<Time>,
    sim: Res<Sim>,
    orbit: Res<OrbitCamera>,
    mut flow: ResMut<Flow>,
) {
    let dt = time.delta_secs().min(0.1);
    flow.real_secs += dt;
    let _ = (&orbit, &sim);
}

fn watch_collapse(mut sim: ResMut<Sim>, mut flow: ResMut<Flow>, mut clock: ResMut<SimClock>) {
    let events = sim.0.colony.take_events();
    for e in &events {
        flow.push_log(format!("day {:.0}: {:?}", e.day, e.event));
    }
    if flow.phase == Phase::Settled && sim.0.colony.is_collapsed() {
        flow.fire(FlowEvent::Collapse);
        clock.paused = true;
        flow.push_log("The colony collapsed");
    }
}

fn sample_history(sim: Res<Sim>, mut flow: ResMut<Flow>) {
    if !sim.0.colony.is_founded() {
        return;
    }
    let day = sim.0.time_days;
    if flow.history.last().is_none_or(|h| day - h.0 >= 5.0) {
        flow.history.push((day, sim.0.colony.population(), sim.0.colony.food));
        if flow.history.len() > 400 {
            flow.history.remove(0);
        }
    }
}

fn ease_camera(time: Res<Time>, mut flow: ResMut<Flow>, mut orbit: ResMut<OrbitCamera>) {
    let Some(f) = flow.focus else { return };
    let (next, arrived) = ease_step(*orbit, f, (time.delta_secs() * 2.5).min(1.0), flow.settled_at.is_some());
    if arrived {
        flow.focus = None;
    }
    *orbit = next;
}

pub fn ease_step(orbit: OrbitCamera, focus: Vec3, k: f32, settled: bool) -> (OrbitCamera, bool) {
    let mut next = orbit;
    next.target = orbit.target.lerp(focus, k);
    if settled {
        next.distance += (70.0 - next.distance) * k;
    }
    let arrived = next.target.distance(focus) < 0.05;
    if arrived {
        next.target = focus;
    }
    (next, arrived)
}

