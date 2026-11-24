//! Wild animals: rabbits, deer and birds roaming the map. They come in from the edges, stay
//! a few weeks and wander off again; there are only ever a few of each (`*_COUNT`), more
//! than there are wolves.
//!
//! They are soft competition for the beavers' food: they graze the same food plants, deer and
//! rabbits nibble saplings, and birds and rabbits pick at food left lying on the ground. They
//! keep away from the colony (the more it has built, the wider the berth: `AVOID_BASE` plus
//! `AVOID_PER_BUILDING` a building), though a hungry one sometimes slips in for food left out.
//! They bolt from wolves, and rabbits and birds from beavers too. A wolf hunts rabbits and
//! deer as well as beavers (see `predators`); birds fly off.
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
    /// Cells a day ambling and bolting.
    fn paces(self) -> (f32, f32) {
        match self {
            Kind::Rabbit => (RABBIT_PACE, RABBIT_FLEE_PACE),
            Kind::Deer => (DEER_PACE, DEER_FLEE_PACE),
            Kind::Bird => (BIRD_PACE, BIRD_FLY_PACE),
        }
    }
    /// How near a beaver comes before it bolts (deer don't mind beavers).
    fn beaver_fear(self) -> f32 {
        match self {
            Kind::Rabbit => RABBIT_BEAVER_FEAR,
            Kind::Deer => 0.0,
            Kind::Bird => BIRD_BEAVER_FEAR,
        }
    }
    /// Food it eats a day while feeding.
    fn appetite(self) -> f32 {
        match self {
            Kind::Rabbit => RABBIT_EATS_PER_DAY,
            Kind::Deer => DEER_EATS_PER_DAY,
            Kind::Bird => BIRD_EATS_PER_DAY,
        }
    }
    /// A wolf can catch it (birds fly off).
    pub fn prey(self) -> bool {
        self != Kind::Bird
    }
}

/// What an animal is doing.
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum Doing {
    /// Ambling to somewhere new.
    Wander,
    /// Eating where it stands until the day given: a plant, a sapling, or food on the ground.
    Feed { food: Food, until: f64 },
    /// Sitting still a while.
    Rest { until: f64 },
    /// Running (or flying) from danger.
    Flee,
    /// On its way off the map.
    Leave,
    /// Pinned by a wolf; dead when the wolf's kill ends.
    Caught,
}

#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum Food {
    Plant(crate::resources::ResourceId),
    Sapling(crate::resources::ResourceId),
    Pile(u64),
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Animal {
    pub id: u32,
    pub kind: Kind,
    /// Where it is, in cells (cell `c` spans `c..c+1`).
    pub pos: (f32, f32),
    /// Facing, as `atan2(dx, dy)` in cell coordinates.
    pub heading: f32,
    /// Cells a day it moved over the last step (0 standing).
    pub pace: f32,
    pub doing: Doing,
    /// It wanders off the map after this day.
    pub leave_by: f64,
    /// Where it is going (a land route; a bird flies straight to the last cell).
    path: Vec<Cell>,
    /// When it next looks round for danger and for food.
    next_look: f64,
}

/// A dead animal, left lying a while.
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
    /// Food eaten by wild animals, for the colony's books.
    pub eaten: f32,
    pub saplings_eaten: u32,
    /// Where the food plants and saplings are, by area, redrawn daily (see `FoodIndex`).
    #[serde(skip)]
    food: FoodIndex,
}

/// Food plants and saplings bucketed by `FOOD_BUCKET`-cell squares, so an animal looks only
/// at what grows near it rather than at every source on the map. Rebuilt once a day; whether
/// one still has food is checked when it is picked.
#[derive(Clone, Debug, Default)]
struct FoodIndex {
    day: Option<i64>,
    across: usize,
    buckets: Vec<Vec<(Cell, crate::resources::ResourceId, bool)>>,
}

const FOOD_BUCKET: usize = 16;

impl FoodIndex {
    fn refresh(&mut self, sim: &SimState) {
        use crate::resources::ResourceKind;
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
            let sapling = match r.kind {
                ResourceKind::Plant => false,
                ResourceKind::Tree if r.stage() == Some(crate::forestry::TreeStage::Sapling) => true,
                _ => continue,
            };
            let b = (r.cell.1 / FOOD_BUCKET).min(self.across - 1) * self.across + (r.cell.0 / FOOD_BUCKET).min(self.across - 1);
            self.buckets[b].push((r.cell, r.id, sapling));
        }
    }
}

impl Default for Wildlife {
    fn default() -> Self {
        Self::new(0)
    }
}

impl Wildlife {
    pub fn new(seed: u64) -> Self {
        Wildlife { rng: Pcg32::seed_from_u64(seed ^ 0x51D_D33F), animals: Vec::new(), carcasses: Vec::new(), next_id: 1, eaten: 0.0, saplings_eaten: 0, food: FoodIndex::default() }
    }
    fn unit(&mut self) -> f32 {
        (self.rng.next_u32() >> 8) as f32 / (1u32 << 24) as f32
    }
    pub fn get(&self, id: u32) -> Option<&Animal> {
        self.animals.iter().find(|a| a.id == id)
    }
}

/// How far an animal keeps from the colony's buildings.
fn berth(sim: &SimState) -> f32 {
    AVOID_BASE + AVOID_PER_BUILDING * (sim.buildings.buildings.len() as f32).sqrt() * 2.0
}

/// Distance from `p` to the nearest building (any kind), or far.
fn to_colony(sim: &SimState, p: (f32, f32)) -> f32 {
    sim.buildings.buildings.iter().map(|b| {
        let (w, d) = b.kind.spec().footprint;
        dist(p, (b.pos.0 as f32 + w as f32 / 2.0, b.pos.1 as f32 + d as f32 / 2.0))
    }).fold(f32::INFINITY, f32::min)
}

fn cell_of(p: (f32, f32)) -> Cell {
    (p.0.max(0.0) as usize, p.1.max(0.0) as usize)
}

/// Where danger is for this animal: the wolf, or a beaver too close, nearest first.
fn danger(sim: &SimState, a: &Animal) -> Option<(f32, f32)> {
    if let Some(w) = sim.predators.wolf.as_ref() {
        if dist(w.pos, a.pos) <= WOLF_FEAR {
            return Some(w.pos);
        }
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

/// A spot away from `from`, `far` cells off, that the animal can be on.
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

/// Somewhere to feed near the animal: a food plant (or, now and then, a sapling or food lying
/// on the ground), picked at random from what grows within `SEARCH_RADIUS`, outside the
/// colony's berth unless the food is left out on the ground.
fn find_food(sim: &SimState, wl: &mut Wildlife, a: &Animal) -> Option<(Cell, Food)> {
    use crate::resources::ResourceKind;
    let berth = berth(sim);
    let n = sim.nav.surface.width();
    let r = SEARCH_RADIUS as f32;
    // Food left out: birds and rabbits go for it, even close to the colony.
    if a.kind != Kind::Deer && wl.unit() < PILE_CHANCE {
        let pile = sim.work.piles.items.iter().filter(|p| p.good == crate::buildings::Good::Food && dist(centre(p.cell), a.pos) <= r * 2.0).min_by(|x, y| dist(centre(x.cell), a.pos).total_cmp(&dist(centre(y.cell), a.pos)));
        if let Some(p) = pile {
            return Some((p.cell, Food::Pile(p.id)));
        }
    }
    let saplings = a.kind != Kind::Bird && wl.unit() < SAPLING_CHANCE;
    let _ = n;
    // Everything of the right sort within reach, then a few of them at random, checked.
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
                f.2 == saplings && (f.0 .0 as f32 + 0.5 - a.pos.0).abs() <= r && (f.0 .1 as f32 + 0.5 - a.pos.1).abs() <= r
            }));
        }
    }
    for _ in 0..8 {
        if near.is_empty() {
            break;
        }
        let k = (wl.unit() * near.len() as f32) as usize % near.len();
        let (c, id, sapling) = near.swap_remove(k);
        if to_colony(sim, centre(c)) < berth || !walkable(sim, c) {
            continue;
        }
        match sim.resources.get(id) {
            Some(res) if !sapling && res.kind == ResourceKind::Plant && res.amount >= 1.0 => return Some((c, Food::Plant(id))),
            Some(res) if sapling && res.stage() == Some(crate::forestry::TreeStage::Sapling) => return Some((c, Food::Sapling(id))),
            _ => {}
        }
    }
    None
}

/// A place to wander to, outside the colony's berth.
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

/// Head for `to`: a land route for rabbits and deer, a straight flight for birds.
fn head_for(sim: &SimState, a: &mut Animal, to: Cell) -> bool {
    if a.kind == Kind::Bird {
        a.path = vec![to];
        return true;
    }
    let t = centre(to);
    a.path = crate::predators::land_path_toward(sim, cell_of(a.pos), |c| c == to, |c| dist(centre(c), t), SEARCH_NODES);
    !a.path.is_empty()
}

/// Move along the route at `pace` cells a day for `dt`; returns how far it went.
fn travel(sim: &SimState, a: &mut Animal, pace: f32, dt: f32) -> f32 {
    let budget = pace * dt;
    if a.kind == Kind::Bird {
        // Flying: straight there.
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

/// A map-edge cell the animal can come in or go out at, far from every beaver.
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

fn eat(sim: &mut SimState, food: Food, amount: f32) -> f32 {
    match food {
        Food::Plant(id) => match sim.resources.get_mut(id) {
            Ok(r) if r.amount > 0.0 => {
                let got = amount.min(r.amount);
                r.amount -= got;
                sim.resources.remove_if_bare(id);
                got
            }
            _ => 0.0,
        },
        Food::Pile(id) => sim.work.piles.take(id, amount),
        Food::Sapling(id) => {
            // Nibbled to the ground: a sapling eaten is gone.
            let had = sim.resources.get(id).is_some();
            sim.resources.keep(|r| r.id != id);
            if had {
                sim.wildlife.saplings_eaten += 1;
            }
            0.0
        }
    }
}

fn food_left(sim: &SimState, food: Food) -> bool {
    match food {
        Food::Plant(id) => sim.resources.get(id).is_some_and(|r| r.amount > 0.2),
        Food::Sapling(id) => sim.resources.get(id).is_some(),
        Food::Pile(id) => sim.work.piles.get(id).is_some_and(|p| p.amount > 0.05),
    }
}

/// The nearest wild animal a wolf at `at` can see and catch, on land.
pub fn prey_for_wolf(sim: &SimState, at: (f32, f32), sight: f32, skip: &[u32]) -> Option<u32> {
    sim.wildlife
        .animals
        .iter()
        .filter(|a| a.kind.prey() && !skip.contains(&a.id) && a.doing != Doing::Leave && dist(a.pos, at) <= sight && walkable(sim, cell_of(a.pos)))
        .min_by(|x, y| dist(x.pos, at).total_cmp(&dist(y.pos, at)).then(x.id.cmp(&y.id)))
        .map(|a| a.id)
}

/// A wolf has caught this animal: it is pinned until killed.
pub fn catch(sim: &mut SimState, id: u32) {
    if let Some(a) = sim.wildlife.animals.iter_mut().find(|a| a.id == id) {
        a.doing = Doing::Caught;
        a.path.clear();
        a.pace = 0.0;
    }
}

/// A wolf's kill: the animal dies and lies where it fell.
pub fn kill(sim: &mut SimState, id: u32) {
    let now = sim.time_days;
    if let Some(i) = sim.wildlife.animals.iter().position(|a| a.id == id) {
        let a = sim.wildlife.animals.remove(i);
        sim.wildlife.carcasses.push(Carcass { kind: a.kind, pos: a.pos, heading: a.heading, died: now });
    }
}

/// A caught animal the wolf let go (its kill was cut short) runs on.
pub fn release(sim: &mut SimState, id: u32) {
    if let Some(a) = sim.wildlife.animals.iter_mut().find(|a| a.id == id && a.doing == Doing::Caught) {
        a.doing = Doing::Flee;
    }
}

pub fn step(sim: &mut SimState, dt: f32) {
    if !sim.colony.is_founded() {
        return;
    }
    let now = sim.time_days;
    sim.wildlife.carcasses.retain(|c| now - c.died < CARCASS_DAYS);
    // Newcomers wander in from the edges while there are fewer than there should be.
    let new_day = now.floor() > (now - dt as f64).floor();
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
                    wl.animals.push(Animal { id, kind, pos: centre(c), heading: 0.0, pace: 0.0, doing: Doing::Wander, leave_by: now + stay as f64, path: Vec::new(), next_look: look });
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
    // Food the animals ate this step was taken straight from the plants and piles (`act`).
    let eaten = std::mem::take(&mut sim.wildlife.eaten);
    let saplings = std::mem::take(&mut sim.wildlife.saplings_eaten);
    wl.eaten += eaten;
    wl.saplings_eaten += saplings;
    sim.wildlife = wl;
}

fn act(sim: &mut SimState, wl: &mut Wildlife, a: &mut Animal, dt: f32, now: f64) {
    let (amble, bolt) = a.kind.paces();
    a.pace = 0.0;
    if a.doing == Doing::Caught {
        return;
    }
    // Look round now and then: danger first.
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
            let n = sim.nav.surface.width();
            let edge = [(cell_of(a.pos).0, 0), (cell_of(a.pos).0, n - 1), (0, cell_of(a.pos).1), (n - 1, cell_of(a.pos).1)]
                .into_iter()
                .filter(|&c| a.kind == Kind::Bird || walkable(sim, c))
                .min_by(|x, y| dist(centre(*x), a.pos).total_cmp(&dist(centre(*y), a.pos)));
            match edge.filter(|&e| head_for(sim, a, e)) {
                Some(_) => a.doing = Doing::Leave,
                None => {
                    // No way out: it goes at once.
                    a.doing = Doing::Leave;
                    a.path.clear();
                }
            }
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
            let got = eat(sim, food, a.kind.appetite() * dt);
            wl.eaten += got;
            if now >= until || !food_left(sim, food) {
                a.doing = Doing::Rest { until: now + REST_DAYS as f64 * (0.5 + wl.unit() as f64) };
            }
        }
        Doing::Rest { until } => {
            if now >= until {
                a.doing = Doing::Wander;
            }
        }
        Doing::Wander => {
            if a.path.is_empty() {
                // Hungry most of the time: look for food, else wander on.
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

#[cfg(test)]
mod tests {
    use super::*;

    fn colony() -> SimState {
        let (mut s, site) = crate::starts::new_game_on_map("voyageurs").unwrap();
        s.apply(crate::Command::Settle { cell: site.camp }).unwrap();
        s
    }

    #[test]
    fn a_few_animals_come_in_keep_their_distance_and_eat() {
        let mut s = colony();
        crate::sandbox::run_days(&mut s, 30.0);
        let w = &s.wildlife;
        for kind in Kind::ALL {
            let n = w.animals.iter().filter(|a| a.kind == kind).count();
            assert!(n >= 1 && n <= kind.count(), "{kind:?}: {n}");
        }
        assert!(w.animals.len() > 1, "more of them than wolves");
        assert!(w.eaten > 1.0, "they ate only {}", w.eaten);
        // Most of the time they keep away from the colony.
        let berth = berth(&s);
        let (mut near, mut all) = (0, 0);
        for _ in 0..20 {
            crate::sandbox::run_days(&mut s, 0.25);
            for a in &s.wildlife.animals {
                all += 1;
                near += (to_colony(&s, a.pos) < berth * 0.6) as usize;
            }
        }
        assert!(near * 5 < all, "{near} of {all} sightings close in");
    }

    #[test]
    fn a_rabbit_bolts_from_a_beaver_and_a_wolf_hunts_it_down() {
        let mut s = colony();
        crate::sandbox::run_days(&mut s, 0.1);
        let b = s.agents.by_id.values().find(|a| !a.inside).map(|a| a.cell).unwrap_or(s.agents.by_id.values().next().unwrap().cell);
        // A rabbit right beside a beaver, on land.
        let spot = (1..8usize).flat_map(|r| [(b.0 + r, b.1), (b.0, b.1 + r), (b.0 - r, b.1), (b.0, b.1 - r)]).find(|&c| walkable(&s, c)).unwrap();
        s.wildlife.animals.push(Animal { id: 999, kind: Kind::Rabbit, pos: centre(spot), heading: 0.0, pace: 0.0, doing: Doing::Wander, leave_by: f64::MAX, path: Vec::new(), next_look: 0.0 });
        let start = centre(spot);
        let mut fled = false;
        for _ in 0..20 {
            crate::sandbox::run_days(&mut s, crate::STEP_DAYS as f64);
            fled |= s.wildlife.get(999).is_some_and(|a| a.doing == Doing::Flee);
        }
        assert!(fled, "the rabbit sat tight next to a beaver");
        assert!(s.wildlife.get(999).is_some_and(|a| dist(a.pos, start) > 2.0), "it ran");
        // A wolf turns up near it, with no beaver in reach.
        let at = s.wildlife.get(999).unwrap().pos;
        assert!(crate::predators::summon(&mut s));
        let near = (0..32).map(|k| k as f32 / 32.0 * std::f32::consts::TAU).find_map(|t| {
            let c = cell_of((at.0 + 6.0 * t.cos(), at.1 + 6.0 * t.sin()));
            walkable(&s, c).then_some(centre(c))
        }).unwrap();
        s.predators.wolf.as_mut().unwrap().pos = near;
        for a in s.agents.by_id.values_mut() {
            a.inside = true;
        }
        let mut hunted = false;
        for _ in 0..400 {
            crate::sandbox::run_days(&mut s, crate::STEP_DAYS as f64);
            hunted |= matches!(s.predators.wolf.as_ref().map(|w| w.state), Some(crate::predators::WolfState::Hunt { animal: 999, .. }));
            if s.wildlife.get(999).is_none() {
                break;
            }
        }
        assert!(hunted, "the wolf never went for the rabbit");
    }

    #[test]
    fn wild_animals_eat_saplings_and_food_left_out() {
        let mut s = colony();
        let c = s.agents.by_id.values().next().unwrap().cell;
        // Food dropped well out from the colony, and saplings round it.
        let far = (40..80usize).flat_map(|r| [(c.0 + r, c.1), (c.0 - r, c.1), (c.0, c.1 + r), (c.0, c.1 - r)]).find(|&x| walkable(&s, x)).unwrap();
        s.work.piles.drop_at(far, crate::buildings::Good::Food, 6.0, s.time_days);
        let pile = s.work.piles.items.iter().find(|p| p.cell == far).unwrap().id;
        let mut saplings = 0;
        for dy in 0..6 {
            for dx in 0..6 {
                let x = (far.0 + 3 + dx, far.1 + 3 + dy);
                if walkable(&s, x) && s.resources.at(x).next().is_none() {
                    s.resources.add_tree(x, 10.0, crate::trees::Tree::Aspen);
                    saplings += 1;
                }
            }
        }
        assert!(saplings > 5);
        for k in 0..4u32 {
            let kind = if k < 2 { Kind::Deer } else { Kind::Rabbit };
            s.wildlife.animals.push(Animal { id: 900 + k, kind, pos: centre((far.0 + 1, far.1 + 1)), heading: 0.0, pace: 0.0, doing: Doing::Wander, leave_by: f64::MAX, path: Vec::new(), next_look: 0.0 });
        }
        crate::sandbox::run_days(&mut s, 12.0);
        assert!(s.wildlife.saplings_eaten > 0, "no sapling eaten");
        assert!(s.work.piles.get(pile).is_none_or(|p| p.amount < 6.0), "the food left out was not touched");
    }
}
