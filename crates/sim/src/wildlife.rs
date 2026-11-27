include!(concat!(env!("OUT_DIR"), "/wildlife_constants.rs"));

use rand_core::{Rng, SeedableRng};
use rand_pcg::Pcg32;

use crate::predators::{centre, dist, walkable};
use crate::water::Cell;
use crate::SimState;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum Kind {
    Rabbit,
    Deer,
    Bird,
}

impl Kind {
    pub const ALL: [Kind; 3] = [Kind::Rabbit, Kind::Deer, Kind::Bird];
    fn count(self) -> usize {
        match self {
            Kind::Rabbit => RABBIT_COUNT,
            Kind::Deer => DEER_COUNT,
            Kind::Bird => BIRD_COUNT,
        }
    }
    fn paces(self) -> (f32, f32) {
        match self {
            Kind::Rabbit => (RABBIT_PACE, RABBIT_FLEE_PACE),
            Kind::Deer => (DEER_PACE, DEER_FLEE_PACE),
            Kind::Bird => (BIRD_PACE, BIRD_FLY_PACE),
        }
    }
    fn beaver_fear(self) -> f32 {
        match self {
            Kind::Rabbit => RABBIT_BEAVER_FEAR,
            Kind::Deer => 0.0,
            Kind::Bird => BIRD_BEAVER_FEAR,
        }
    }
    pub fn appetite(self) -> f32 {
        match self {
            Kind::Rabbit => RABBIT_EATS_PER_DAY,
            Kind::Deer => DEER_EATS_PER_DAY,
            Kind::Bird => BIRD_EATS_PER_DAY,
        }
    }
    pub fn prey(self) -> bool {
        self != Kind::Bird
    }
}

#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum Doing {
    Wander,
    Feed { food: Food, until: f64 },
    Rest { until: f64 },
    Flee,
    Leave,
    Caught,
    Digest { food: Food, until: f64 },
}

#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum Food {
    Plant(crate::resources::ResourceId),
    Seedling(crate::resources::ResourceId),
    Pile(u64),
}

#[derive(Clone, Debug, Default)]
pub struct Track(pub std::collections::VecDeque<(f64, (f32, f32))>);

impl Track {
    const KEEP: usize = 64;

    pub fn note(&mut self, t: f64, p: (f32, f32)) {
        if self.0.back().is_some_and(|b| b.1 == p) {
            return;
        }
        self.0.push_back((t, p));
        while self.0.len() > Self::KEEP {
            self.0.pop_front();
        }
    }
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Animal {
    pub id: u32,
    pub kind: Kind,
    pub pos: (f32, f32),
    pub heading: f32,
    pub pace: f32,
    pub doing: Doing,
    pub leave_by: f64,
    path: Vec<Cell>,
    next_look: f64,
    #[serde(skip)]
    pub track: Track,
}

#[derive(Clone, Copy, Debug, serde::Serialize, serde::Deserialize)]
pub struct Carcass {
    pub kind: Kind,
    pub pos: (f32, f32),
    pub heading: f32,
    pub died: f64,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Wildlife {
    rng: Pcg32,
    pub animals: Vec<Animal>,
    pub carcasses: Vec<Carcass>,
    next_id: u32,
    pub eaten: f32,
    pub seedlings_eaten: u32,
    #[serde(skip)]
    food: FoodIndex,
}

#[derive(Clone, Debug, Default)]
struct FoodIndex {
    day: Option<i64>,
    across: usize,
    buckets: Vec<Vec<(Cell, crate::resources::ResourceId, bool)>>,
}

const FOOD_BUCKET: usize = 16;

impl FoodIndex {
    fn refresh(&mut self, sim: &SimState) {

        let day = sim.time_days.floor() as i64;
        if self.day == Some(day) {
            return;
        }
        self.day = Some(day);
        let n = sim.nav.surface.width().max(1);
        self.across = n.div_ceil(FOOD_BUCKET);
        self.buckets.iter_mut().for_each(Vec::clear);
        self.buckets.resize(self.across * self.across, Vec::new());
        for r in sim.resources.items() {
            let Some(seedling) = browse_kind(r) else { continue };
            let b = (r.cell.1 / FOOD_BUCKET).min(self.across - 1) * self.across + (r.cell.0 / FOOD_BUCKET).min(self.across - 1);
            self.buckets[b].push((r.cell, r.id, seedling));
        }
    }
}

fn browse_kind(r: &crate::resources::Resource) -> Option<bool> {
    if r.kind.is_food_plant() && !r.kind.aquatic() && !r.is_seedling() { Some(false) }
    else if r.kind.is_tree() && r.is_seedling() { Some(true) }
    else { None }
}

impl Default for Wildlife {
    fn default() -> Self {
        Self::new(0)
    }
}

impl Wildlife {
    pub fn new(seed: u64) -> Self {
        Wildlife { rng: Pcg32::seed_from_u64(seed ^ 0x51D_D33F), animals: Vec::new(), carcasses: Vec::new(), next_id: 1, eaten: 0.0, seedlings_eaten: 0, food: FoodIndex::default() }
    }
    fn unit(&mut self) -> f32 {
        (self.rng.next_u32() >> 8) as f32 / (1u32 << 24) as f32
    }
    pub fn get(&self, id: u32) -> Option<&Animal> {
        self.animals.iter().find(|a| a.id == id)
    }
}

fn berth(sim: &SimState) -> f32 {
    AVOID_BASE + AVOID_PER_BUILDING * (sim.buildings.buildings.len() as f32).sqrt() * 2.0
}

fn to_colony(sim: &SimState, p: (f32, f32)) -> f32 {
    sim.buildings.buildings.iter()
        .flat_map(|b| b.cells())
        .map(|c| dist(p, centre(c)))
        .fold(f32::INFINITY, f32::min)
}

fn cell_of(p: (f32, f32)) -> Cell {
    (p.0.max(0.0) as usize, p.1.max(0.0) as usize)
}

fn danger(sim: &SimState, a: &Animal) -> Option<(f32, f32)> {
    let wolf = sim.predators.wolves.iter().map(|w| w.pos).filter(|&p| dist(p, a.pos) <= WOLF_FEAR).min_by(|x, y| dist(*x, a.pos).total_cmp(&dist(*y, a.pos)));
    if wolf.is_some() {
        return wolf;
    }
    let fear = a.kind.beaver_fear();
    if fear > 0.0 {
        let near = sim.agents.by_id.values().filter(|b| !b.inside).map(|b| centre(b.cell)).filter(|&p| dist(p, a.pos) <= fear).min_by(|x, y| dist(*x, a.pos).total_cmp(&dist(*y, a.pos)));
        if near.is_some() {
            return near;
        }
    }
    None
}

fn away(sim: &SimState, wl: &mut Wildlife, a: &Animal, from: (f32, f32), far: f32) -> Option<Cell> {
    let base = (a.pos.1 - from.1).atan2(a.pos.0 - from.0);
    let n = sim.nav.surface.width() as f32;
    for k in 0..10 {
        let ang = base + (wl.unit() - 0.5) * (0.6 + k as f32 * 0.25);
        let p = (a.pos.0 + ang.cos() * far, a.pos.1 + ang.sin() * far);
        if p.0 < 1.0 || p.1 < 1.0 || p.0 >= n - 1.0 || p.1 >= n - 1.0 {
            continue;
        }
        let c = cell_of(p);
        if walkable(sim, c) {
            return Some(c);
        }
    }
    None
}

fn find_food(sim: &SimState, wl: &mut Wildlife, a: &Animal) -> Option<(Cell, Food)> {

    let berth = berth(sim);
    let n = sim.nav.surface.width();
    let r = SEARCH_RADIUS as f32;
    if wl.unit() < PILE_CHANCE {
        let pile = sim.work.piles.items.iter().filter(|p| p.good == crate::buildings::Good::food() && dist(centre(p.cell), a.pos) <= r * 2.0).min_by(|x, y| dist(centre(x.cell), a.pos).total_cmp(&dist(centre(y.cell), a.pos)));
        if let Some(p) = pile {
            return Some((p.cell, Food::Pile(p.id)));
        }
    }
    let seedlings = a.kind != Kind::Bird && wl.unit() < SEEDLING_CHANCE;
    let _ = n;
    let mut near: Vec<(Cell, crate::resources::ResourceId, bool)> = Vec::new();
    let (lo, hi) = ((a.pos.0 - r, a.pos.1 - r), (a.pos.0 + r, a.pos.1 + r));
    let across = wl.food.across;
    if across == 0 {
        return None;
    }
    let bucket = |v: f32| ((v.max(0.0) as usize) / FOOD_BUCKET).min(across - 1);
    for by in bucket(lo.1)..=bucket(hi.1) {
        for bx in bucket(lo.0)..=bucket(hi.0) {
            near.extend(wl.food.buckets[by * across + bx].iter().filter(|f| {
                f.2 == seedlings && (f.0 .0 as f32 + 0.5 - a.pos.0).abs() <= r && (f.0 .1 as f32 + 0.5 - a.pos.1).abs() <= r
            }));
        }
    }
    for _ in 0..8 {
        if near.is_empty() {
            break;
        }
        let k = (wl.unit() * near.len() as f32) as usize % near.len();
        let (c, id, seedling) = near.swap_remove(k);
        if to_colony(sim, centre(c)) < berth || !walkable(sim, c) {
            continue;
        }
        match sim.resources.get(id).and_then(|res| browse_kind(res).map(|s| (res, s))) {
            Some((res, false)) if !seedling && res.amount >= 1.0 => return Some((c, Food::Plant(id))),
            Some((_, true)) if seedling => return Some((c, Food::Seedling(id))),
            _ => {}
        }
    }
    None
}

fn wander_spot(sim: &SimState, wl: &mut Wildlife, a: &Animal) -> Option<Cell> {
    let berth = berth(sim);
    let n = sim.nav.surface.width() as f32;
    let r = WANDER_RADIUS;
    for _ in 0..8 {
        let p = (a.pos.0 + (wl.unit() * 2.0 - 1.0) * r, a.pos.1 + (wl.unit() * 2.0 - 1.0) * r);
        if p.0 < 1.0 || p.1 < 1.0 || p.0 >= n - 1.0 || p.1 >= n - 1.0 {
            continue;
        }
        let c = cell_of(p);
        if walkable(sim, c) && to_colony(sim, centre(c)) >= berth {
            return Some(c);
        }
    }
    None
}

fn head_for(sim: &SimState, a: &mut Animal, to: Cell) -> bool {
    if a.kind == Kind::Bird {
        a.path = vec![to];
        return true;
    }
    let t = centre(to);
    a.path = crate::predators::land_path_toward(sim, cell_of(a.pos), |c| c == to, |c| dist(centre(c), t), SEARCH_NODES);
    !a.path.is_empty()
}

fn travel(sim: &SimState, a: &mut Animal, pace: f32, dt: f32) -> f32 {
    let budget = pace * dt;
    if a.kind == Kind::Bird {
        let Some(&to) = a.path.last() else { return 0.0 };
        let t = centre(to);
        let d = dist(a.pos, t);
        let step = budget.min(d);
        if d > 1e-4 {
            a.heading = (t.0 - a.pos.0).atan2(t.1 - a.pos.1);
            a.pos = (a.pos.0 + (t.0 - a.pos.0) / d * step, a.pos.1 + (t.1 - a.pos.1) / d * step);
        }
        if d - step < 1e-3 {
            a.path.clear();
        }
        return step;
    }
    crate::predators::walk_route(sim, &mut a.pos, &mut a.heading, &mut a.path, budget)
}

fn edge_cell(sim: &SimState, wl: &mut Wildlife) -> Option<Cell> {
    let n = sim.nav.surface.width();
    for _ in 0..40 {
        let t = (wl.unit() * n as f32) as usize % n;
        let c = match (wl.unit() * 4.0) as u32 {
            0 => (t, 0),
            1 => (t, n - 1),
            2 => (0, t),
            _ => (n - 1, t),
        };
        if walkable(sim, c) && sim.agents.by_id.values().all(|b| dist(centre(b.cell), centre(c)) > 40.0) {
            return Some(c);
        }
    }
    None
}

fn eat(sim: &mut SimState, wl: &mut Wildlife, food: Food, amount: f32) -> f32 {
    match food {
        Food::Plant(id) => {
            let got = sim.resources.take(id, amount).map_or(0.0, |(_, g)| g);
            sim.resources.remove_if_bare(id);
            got
        }
        Food::Pile(id) => sim.work.piles.take(id, amount),
        Food::Seedling(id) => {
            if let Some(r) = sim.resources.get(id) {
                crate::resources::note("GONE", r, format_args!("why=eaten by wildlife"));
                sim.resources.keep(|r| r.id != id);
                wl.seedlings_eaten += 1;
            }
            0.0
        }
    }
}

fn food_left(sim: &SimState, food: Food) -> bool {
    match food {
        Food::Plant(id) => sim.resources.get(id).is_some(),
        Food::Seedling(id) => sim.resources.get(id).is_some(),
        Food::Pile(id) => sim.work.piles.get(id).is_some_and(|p| p.amount > 0.05),
    }
}

pub fn prey_for_wolf(sim: &SimState, at: (f32, f32), sight: f32, skip: &[u32]) -> Option<u32> {
    sim.wildlife
        .animals
        .iter()
        .filter(|a| a.kind.prey() && !skip.contains(&a.id) && !matches!(a.doing, Doing::Leave | Doing::Caught) && dist(a.pos, at) <= sight && walkable(sim, cell_of(a.pos)))
        .min_by(|x, y| dist(x.pos, at).total_cmp(&dist(y.pos, at)).then(x.id.cmp(&y.id)))
        .map(|a| a.id)
}

pub fn catch(sim: &mut SimState, id: u32) {
    if let Some(a) = sim.wildlife.animals.iter_mut().find(|a| a.id == id) {
        a.doing = Doing::Caught;
        a.path.clear();
        a.pace = 0.0;
    }
}

pub fn kill(sim: &mut SimState, id: u32) {
    let now = sim.time_days;
    if let Some(i) = sim.wildlife.animals.iter().position(|a| a.id == id) {
        let a = sim.wildlife.animals.remove(i);
        sim.wildlife.carcasses.push(Carcass { kind: a.kind, pos: a.pos, heading: a.heading, died: now });
    }
}

pub fn note_tracks(sim: &mut SimState) {
    let now = sim.time_days;
    for a in &mut sim.wildlife.animals {
        a.track.note(now, a.pos);
    }
    for w in &mut sim.predators.wolves {
        w.track.note(now, w.pos);
    }
}

pub fn step(sim: &mut SimState, dt: f32, new_day: bool) {
    if !sim.colony.is_founded() {
        return;
    }
    let now = sim.time_days;
    sim.wildlife.carcasses.retain(|c| now - c.died < CARCASS_DAYS);
    if new_day {
        for kind in Kind::ALL {
            let have = sim.wildlife.animals.iter().filter(|a| a.kind == kind).count();
            if have < kind.count() {
                let mut wl = std::mem::take(&mut sim.wildlife);
                let roll = wl.unit();
                let at = if roll < ARRIVE_CHANCE || have * 2 < kind.count() { edge_cell(sim, &mut wl) } else { None };
                if let Some(c) = at {
                    let id = wl.next_id;
                    wl.next_id += 1;
                    let stay = STAY_MIN_DAYS + wl.unit() * (STAY_MAX_DAYS - STAY_MIN_DAYS);
                    let look = now + wl.unit() as f64 * 0.05;
                    wl.animals.push(Animal { id, kind, pos: centre(c), heading: 0.0, pace: 0.0, doing: Doing::Wander, leave_by: now + stay as f64, path: Vec::new(), next_look: look, track: Track::default() });
                }
                sim.wildlife = wl;
            }
        }
    }
    let mut wl = std::mem::take(&mut sim.wildlife);
    wl.food.refresh(sim);
    let mut animals = std::mem::take(&mut wl.animals);
    let mut gone = Vec::new();
    for a in &mut animals {
        act(sim, &mut wl, a, dt, now);
        if a.doing == Doing::Leave && a.path.is_empty() {
            gone.push(a.id);
        }
    }
    animals.retain(|a| !gone.contains(&a.id));
    wl.animals = animals;
    sim.wildlife = wl;
}

fn act(sim: &mut SimState, wl: &mut Wildlife, a: &mut Animal, dt: f32, now: f64) {
    let (amble, bolt) = a.kind.paces();
    a.pace = 0.0;
    if a.doing == Doing::Caught {
        return;
    }
    if now >= a.next_look {
        a.next_look = now + LOOK_DAYS as f64;
        if let Some(from) = danger(sim, a) {
            if a.doing != Doing::Flee || a.path.is_empty() {
                let far = if a.kind == Kind::Bird { 40.0 } else { 18.0 };
                if let Some(to) = away(sim, wl, a, from, far) {
                    if head_for(sim, a, to) {
                        a.doing = Doing::Flee;
                    }
                }
            }
        } else if a.doing == Doing::Flee && a.path.is_empty() {
            a.doing = Doing::Rest { until: now + 0.05 };
        }
        if now >= a.leave_by && !matches!(a.doing, Doing::Flee | Doing::Leave) {
            let ok = if a.kind == Kind::Bird {
                let n = sim.nav.surface.width();
                let edge = [(cell_of(a.pos).0, 0), (cell_of(a.pos).0, n - 1), (0, cell_of(a.pos).1), (n - 1, cell_of(a.pos).1)]
                    .into_iter()
                    .min_by(|x, y| dist(centre(*x), a.pos).total_cmp(&dist(centre(*y), a.pos)));
                edge.is_some_and(|e| head_for(sim, a, e))
            } else {
                a.path = crate::predators::land_path(sim, cell_of(a.pos), |c| crate::predators::edge_gap(&sim.nav, c) == 0, |c| crate::predators::edge_gap(&sim.nav, c) as f32, SEARCH_NODES * 4).unwrap_or_default();
                !a.path.is_empty()
            };
            a.doing = Doing::Leave;
            if !ok { a.path.clear(); }
        }
    }
    match a.doing {
        Doing::Flee => {
            a.pace = travel(sim, a, bolt, dt) / dt.max(1e-6);
        }
        Doing::Leave => {
            a.pace = travel(sim, a, if a.kind == Kind::Bird { bolt * 0.5 } else { amble }, dt) / dt.max(1e-6);
        }
        Doing::Feed { food, until } => {
            if !a.path.is_empty() {
                a.pace = travel(sim, a, amble, dt) / dt.max(1e-6);
                return;
            }
            let got = eat(sim, wl, food, a.kind.appetite() * dt);
            wl.eaten += got;
            if !food_left(sim, food) {
                a.doing = Doing::Rest { until: now + REST_DAYS as f64 * (0.5 + wl.unit() as f64) };
            } else if now >= until {
                a.doing = Doing::Digest { food, until: now + REST_DAYS as f64 * (0.5 + wl.unit() as f64) };
            }
        }
        Doing::Digest { food, until } => {
            if !food_left(sim, food) {
                a.doing = Doing::Wander;
            } else if now >= until {
                a.doing = Doing::Feed { food, until: now + FEED_DAYS as f64 * (0.5 + wl.unit() as f64) };
            }
        }
        Doing::Rest { until } => {
            if now >= until {
                a.doing = Doing::Wander;
            }
        }
        Doing::Wander => {
            if a.path.is_empty() {
                if let Some((c, food)) = find_food(sim, wl, a) {
                    if head_for(sim, a, c) || cell_of(a.pos) == c {
                        a.doing = Doing::Feed { food, until: now + FEED_DAYS as f64 * (0.5 + wl.unit() as f64) };
                        return;
                    }
                }
                match wander_spot(sim, wl, a) {
                    Some(c) if head_for(sim, a, c) => {}
                    _ => a.doing = Doing::Rest { until: now + REST_DAYS as f64 },
                }
            }
            a.pace = travel(sim, a, amble, dt) / dt.max(1e-6);
            if a.path.is_empty() {
                a.doing = Doing::Rest { until: now + REST_DAYS as f64 * wl.unit() as f64 };
            }
        }
        Doing::Caught => {}
    }
}
