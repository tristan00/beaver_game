//! Wolves: roaming, hunting beavers and wildlife, lookout alarms, and fleeing.
include!(concat!(env!("OUT_DIR"), "/predators_constants.rs"));

use std::cmp::Reverse;
use std::collections::{BinaryHeap, VecDeque};

use rand_core::{Rng, SeedableRng};
use rand_pcg::Pcg32;

use crate::agents::{Agent, Task};
use crate::buildings::{Building, BuildingId};
use crate::colony::{BeaverId, DeathCause};
use crate::nav::{Goal, NavGrid, Surface};
use crate::water::Cell;
use crate::SimState;

const ROOT2: f32 = std::f32::consts::SQRT_2;
const STRAIGHTEN_AHEAD: usize = 8;

#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum WolfState {
    Roam,
    Charge { target: BeaverId, since: f64 },
    Kill { target: BeaverId, until: f64 },
    Leave,
    Hunt { animal: u32, since: f64 },
    Feed { animal: u32, until: f64 },
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Wolf {
    pub pos: (f32, f32),
    pub heading: f32,
    pub pace: f32,
    pub state: WolfState,
    pub arrived: f64,
    pub leave_by: f64,
    path: Vec<Cell>,
    rest_until: f64,
    #[serde(skip)] pub seen: bool,
    #[serde(skip)] pub track: crate::wildlife::Track,
    #[serde(skip)] last_seen: Option<((f32, f32), f64)>,
}

impl Wolf {
    pub fn route_left(&self) -> usize { self.path.len() }
    pub fn rest_until(&self) -> f64 { self.rest_until }
    pub fn cell(&self) -> Cell { (self.pos.0.max(0.0) as usize, self.pos.1.max(0.0) as usize) }
    pub fn key(&self) -> u64 { self.arrived.to_bits() }
    fn face(&mut self, to: (f32, f32)) {
        let d = (to.0 - self.pos.0, to.1 - self.pos.1);
        if d.0.abs() + d.1.abs() > 1e-4 { self.heading = d.0.atan2(d.1); }
    }
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Predators {
    rng: Pcg32,
    pub wolves: Vec<Wolf>,
    pub spotted: bool,
    pub chance_per_day: f32,
    pub visits: u32,
    pub kills: u32,
    #[serde(skip)] pub alarms: Vec<BuildingId>,
    #[serde(skip)] given_up: Vec<(BeaverId, f64)>,
    #[serde(skip)] given_up_animals: Vec<(u32, f64)>,
    #[serde(skip)] pub in_sight: bool,
}

impl Predators {
    pub fn new(seed: u64) -> Self {
        Self {
            rng: Pcg32::seed_from_u64(seed ^ 0x3017_F00D), wolves: Vec::new(), spotted: false,
            chance_per_day: WOLF_CHANCE_PER_DAY, visits: 0, kills: 0, alarms: Vec::new(),
            given_up: Vec::new(), given_up_animals: Vec::new(), in_sight: false,
        }
    }
    pub fn wolf(&self, key: u64) -> Option<&Wolf> { self.wolves.iter().find(|w| w.key() == key) }
    fn unit(&mut self) -> f32 { (self.rng.next_u32() >> 8) as f32 / (1u32 << 24) as f32 }
}

pub fn walkable(sim: &SimState, c: Cell) -> bool {
    matches!(sim.nav.surface_at(c), Some(Surface::Land | Surface::Thicket))
        && sim.water.depth.get(c.0, c.1).is_some_and(|&d| d <= WOLF_MAX_DEPTH)
}
pub(crate) fn centre(c: Cell) -> (f32, f32) { (c.0 as f32 + 0.5, c.1 as f32 + 0.5) }
pub(crate) fn dist(a: (f32, f32), b: (f32, f32)) -> f32 { ((a.0 - b.0).powi(2) + (a.1 - b.1).powi(2)).sqrt() }

fn land_path(sim: &SimState, from: Cell, goal: impl Fn(Cell) -> bool, heur: impl Fn(Cell) -> f32, max_nodes: usize) -> Option<Vec<Cell>> {
    land_search(sim, from, goal, heur, max_nodes, false)
}
pub(crate) fn land_path_toward(sim: &SimState, from: Cell, goal: impl Fn(Cell) -> bool, heur: impl Fn(Cell) -> f32, max_nodes: usize) -> Vec<Cell> {
    land_search(sim, from, goal, heur, max_nodes, true).unwrap_or_default()
}

fn land_search(sim: &SimState, from: Cell, goal: impl Fn(Cell) -> bool, heur: impl Fn(Cell) -> f32, max_nodes: usize, partial: bool) -> Option<Vec<Cell>> {
    struct Scratch { stamp: u32, seen: Vec<u32>, g: Vec<u32>, parent: Vec<u32>, heap: BinaryHeap<Reverse<(u32, u32, u32)>> }
    thread_local! { static SCRATCH: std::cell::RefCell<Scratch> = const { std::cell::RefCell::new(Scratch { stamp: 0, seen: Vec::new(), g: Vec::new(), parent: Vec::new(), heap: BinaryHeap::new() }) }; }
    let (w, h) = (sim.nav.surface.width(), sim.nav.surface.height());
    let (len, start, key) = (w * h, from.1 * w + from.0, |f: f32| (f * 1000.0) as u32);
    SCRATCH.with(|sc| {
        let sc = &mut *sc.borrow_mut();
        if sc.seen.len() < len { sc.seen.resize(len, 0); sc.g.resize(len, 0); sc.parent.resize(len, 0); }
        sc.stamp = sc.stamp.wrapping_add(1);
        if sc.stamp == 0 { sc.seen.fill(0); sc.stamp = 1; }
        let stamp = sc.stamp;
        sc.heap.clear();
        (sc.seen[start], sc.g[start], sc.parent[start]) = (stamp, 0, start as u32);
        sc.heap.push(Reverse((key(heur(from)), 0, start as u32)));
        let route = |parent: &[u32], mut i: usize| {
            let mut out = Vec::new();
            while i != start { out.push((i % w, i / w)); i = parent[i] as usize; }
            out.reverse();
            out
        };
        let (mut nearest, mut expanded) = ((heur(from), start), 0);
        while let Some(Reverse((_, g, cur_u))) = sc.heap.pop() {
            let cur_i = cur_u as usize;
            if sc.g[cur_i] < g { continue; }
            let cur = (cur_i % w, cur_i / w);
            if goal(cur) { return Some(route(&sc.parent, cur_i)); }
            let left = heur(cur);
            if left < nearest.0 { nearest = (left, cur_i); }
            expanded += 1;
            if expanded > max_nodes { return partial.then(|| route(&sc.parent, nearest.1)); }
            for (dx, dy) in [(-1i32, 0), (1, 0), (0, -1), (0, 1), (-1, -1), (1, -1), (-1, 1), (1, 1)] {
                let (x, y) = (cur.0 as i32 + dx, cur.1 as i32 + dy);
                if x < 0 || y < 0 || x as usize >= w || y as usize >= h { continue; }
                let to = (x as usize, y as usize);
                let diag = dx != 0 && dy != 0;
                if !walkable(sim, to) || (diag && !(walkable(sim, (to.0, cur.1)) && walkable(sim, (cur.0, to.1)))) { continue; }
                let (to_i, ng) = (to.1 * w + to.0, g + key(if diag { ROOT2 } else { 1.0 }));
                if sc.seen[to_i] != stamp || ng < sc.g[to_i] {
                    (sc.seen[to_i], sc.g[to_i], sc.parent[to_i]) = (stamp, ng, cur_u);
                    sc.heap.push(Reverse((ng + key(heur(to)), ng, to_i as u32)));
                }
            }
        }
        None
    })
}

fn on_land(sim: &SimState, a: &Agent) -> bool {
    let iced = sim.water.ice.get(a.cell.0, a.cell.1).is_some_and(|&t| t >= crate::tuning::water::ICE_WALK_M);
    let deep = sim.water.depth.get(a.cell.0, a.cell.1).is_some_and(|&d| d > WOLF_MAX_DEPTH);
    !a.inside && !(iced && deep) && matches!(sim.nav.surface_at(a.cell), Some(Surface::Land | Surface::Thicket))
}

pub(crate) fn path_within_reach(sim: &SimState, from: Cell, to: Cell, max_nodes: usize) -> Option<Vec<Cell>> {
    let t = centre(to);
    land_path(sim, from, |c| dist(centre(c), t) <= WOLF_CATCH, |c| (dist(centre(c), t) - WOLF_CATCH).max(0.0), max_nodes)
}

fn edge_gap(nav: &NavGrid, c: Cell) -> usize {
    c.0.min(c.1).min(nav.surface.width() - 1 - c.0).min(nav.surface.height() - 1 - c.1)
}

fn note(sim: &SimState, wolf: Option<&Wolf>, what: &str, detail: String) {
    if !crate::trace::enabled() { return; }
    let at = wolf.map_or(String::new(), |w| format!(" pos=({:.1},{:.1}) state={:?}", w.pos.0, w.pos.1, w.state));
    let home = crate::agents::colony_home(sim).map_or(String::new(), |h| format!(" home_dist={:.1}", wolf.map_or(0.0, |w| dist(w.pos, centre(h)))));
    crate::trace::event(format!("WOLF_{what} day={:.4} step={}{at}{home} {detail}", sim.time_days, sim.steps));
}

pub fn summon(sim: &mut SimState) -> bool {
    let (w, h) = (sim.nav.surface.width(), sim.nav.surface.height());
    if sim.predators.wolves.len() >= MAX_WOLVES || w < 3 || h < 3 { return false; }
    let beavers: Vec<(f32, f32)> = sim.agents.by_id.values().map(|a| centre(a.cell)).collect();
    let mut pick: Option<(f32, Cell)> = None;
    for _ in 0..400 {
        let c = (((w - 1) as f32 * sim.predators.unit()) as usize, ((h - 1) as f32 * sim.predators.unit()) as usize);
        if !walkable(sim, c) { continue; }
        let far = beavers.iter().map(|&b| dist(b, centre(c))).fold(f32::INFINITY, f32::min);
        if far >= WOLF_SPAWN_MIN_DIST { pick = Some((far, c)); break; }
        if pick.is_none_or(|(d, _)| far > d) { pick = Some((far, c)); }
    }
    let Some((_, at)) = pick else { return false };
    let now = sim.time_days;
    let p = &mut sim.predators;
    let arrived = p.wolves.iter().map(|w| w.arrived).fold(now, |a, b| if b >= a { b + 1e-9 } else { a });
    p.wolves.push(Wolf { pos: centre(at), heading: 0.0, pace: 0.0, state: WolfState::Roam, arrived, leave_by: now + WOLF_VISIT_DAYS, path: Vec::new(), rest_until: 0.0, seen: false, last_seen: None, track: Default::default() });
    p.visits += 1;
    note(sim, sim.predators.wolves.last(), "ARRIVED", format!("visit={} wolves={} leave_by={:.2}", sim.predators.visits, sim.predators.wolves.len(), now + WOLF_VISIT_DAYS));
    true
}

pub fn step(sim: &mut SimState, dt_days: f32) {
    if !sim.colony.is_founded() { return; }
    let now = sim.time_days;
    if sim.predators.wolves.len() < MAX_WOLVES && now >= WOLF_GRACE_DAYS
        && now.floor() > (now - dt_days as f64).floor()
        && sim.predators.unit() < sim.predators.chance_per_day.max(WOLF_CHANCE_PER_DAY)
    {
        summon(sim);
    }
    let mut wolves = std::mem::take(&mut sim.predators.wolves);
    wolves.retain_mut(|w| {
        let gone = act(sim, w, dt_days);
        if gone { note(sim, Some(w), "GONE", format!("kills_total={} arrived={:.2}", sim.predators.kills, w.arrived)); }
        !gone
    });
    sim.predators.wolves = wolves;
    look(sim);
}

fn look(sim: &mut SimState) {
    let mut wolves = std::mem::take(&mut sim.predators.wolves);
    let sentries = |id: BuildingId| -> Vec<BeaverId> {
        sim.work.place(id).map(|p| p.slots.iter().filter(|s| s.role.duty() == crate::content::Duty::Watch).filter_map(|s| s.beaver).collect()).unwrap_or_default()
    };
    let watching: Vec<(BuildingId, (f32, f32), f32)> = if wolves.is_empty() { Vec::new() } else {
        sim.buildings.buildings.iter().filter(|b| on_watch(sim, b)).map(|b| {
            let keen = sentries(b.id).into_iter().filter_map(|g| sim.colony.beaver(g)).map(crate::growth::sight_x).fold(1.0, f32::max);
            (b.id, lookout_centre(b), LOOKOUT_SIGHT * keen)
        }).collect()
    };
    let alarms: Vec<BuildingId> = watching.iter().filter(|(_, at, s)| wolves.iter().any(|w| dist(*at, w.pos) <= *s)).map(|l| l.0).collect();
    let in_range = |id: BuildingId| sim.buildings.building(id).map_or(0, |b| {
        let at = lookout_centre(b);
        sim.agents.by_id.values().filter(|a| !a.inside && dist(at, centre(a.cell)) <= LOOKOUT_SIGHT).count() as u32
    });
    let mut spotted: Vec<(BeaverId, u64, u32)> = Vec::new();
    for w in &mut wolves {
        let by_lookout = watching.iter().any(|(_, at, s)| dist(*at, w.pos) <= *s);
        w.seen = by_lookout || sim.agents.by_id.values().any(|a| sees(sim, a, w));
        if w.seen || (sim.predators.spotted && w.last_seen.is_none()) { w.last_seen = Some((w.pos, sim.time_days)); }
        if w.seen {
            let key = w.key();
            spotted.extend(sim.agents.by_id.values().filter(|a| sees(sim, a, w)).map(|a| (a.id, key, 0)));
            for &(id, _, _) in watching.iter().filter(|(_, at, s)| dist(*at, w.pos) <= *s) {
                let n = in_range(id);
                spotted.extend(sentries(id).into_iter().map(|g| (g, key, n)));
            }
        }
    }
    for (id, key, warned) in spotted {
        if sim.deeds.spotted(id, key) && warned > 0 { sim.deeds.of(id).warned += warned; }
    }
    let now = sim.time_days;
    let p = &mut sim.predators;
    (p.alarms, p.in_sight, p.spotted, p.wolves) = (
        alarms,
        wolves.iter().any(|w| w.seen),
        wolves.iter().any(|w| w.last_seen.is_some_and(|(_, t)| now - t <= WOLF_SEEN_DAYS)),
        wolves,
    );
}

pub fn lookout_centre(b: &Building) -> (f32, f32) { b.centre() }

pub fn on_watch(sim: &SimState, b: &Building) -> bool {
    if !b.kind.watches() || !b.is_complete() { return false; }
    let reach = SENTRY_POST_REACH as usize;
    let at_post = |a: &Agent| !a.inside && b.cells().any(|c| c.0.abs_diff(a.cell.0).max(c.1.abs_diff(a.cell.1)) <= reach);
    sim.work.place(b.id).is_some_and(|p| p.slots.iter().any(|s| s.role.duty() == crate::content::Duty::Watch && s.beaver.and_then(|w| sim.agents.get(w)).is_some_and(at_post)))
}

fn warned(sim: &SimState, a: &Agent) -> bool {
    !a.inside && sim.predators.alarms.iter().filter_map(|&id| sim.buildings.building(id)).any(|b| dist(lookout_centre(b), centre(a.cell)) <= LOOKOUT_SIGHT)
}

pub fn seen(sim: &SimState) -> impl Iterator<Item = &Wolf> { sim.predators.wolves.iter().filter(|w| w.seen) }
pub fn nearest_seen(sim: &SimState) -> Option<&Wolf> {
    let home = crate::agents::colony_home(sim).map(centre);
    seen(sim).min_by(|a, b| home.map_or(0.0, |h| dist(a.pos, h)).total_cmp(&home.map_or(0.0, |h| dist(b.pos, h))))
}
pub fn in_view(sim: &SimState, c: Cell) -> bool {
    seen(sim).any(|w| dist(w.pos, centre(c)) <= crate::fog::BEAVER_SIGHT)
        || sim.predators.alarms.iter().filter_map(|&id| sim.buildings.building(id)).any(|b| dist(lookout_centre(b), centre(c)) <= LOOKOUT_SIGHT)
}
pub fn building_in_view(sim: &SimState, id: BuildingId) -> bool {
    if id == crate::agents::SUPPLIES { return sim.work.supplies.is_some_and(|c| in_view(sim, c)); }
    sim.buildings.building(id).is_some_and(|b| b.cells().any(|c| in_view(sim, c)))
}
pub fn in_sight(sim: &SimState) -> Option<(f32, f32)> { nearest_seen(sim).map(|w| w.pos) }
pub fn last_seen(sim: &SimState) -> Option<(f32, f32)> {
    let now = sim.time_days;
    sim.predators.wolves.iter().filter_map(|w| w.last_seen).filter(|(_, t)| now - t <= WOLF_SEEN_DAYS).max_by(|a, b| a.1.total_cmp(&b.1)).map(|l| l.0)
}

fn clear_line(sim: &SimState, a: (f32, f32), b: (f32, f32)) -> bool {
    let n = (dist(a, b) * 4.0).ceil().max(1.0) as usize;
    (0..=n).all(|i| {
        let t = i as f32 / n as f32;
        let (x, y) = (a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t);
        x >= 0.0 && y >= 0.0 && walkable(sim, (x as usize, y as usize))
    })
}

pub(crate) fn walk_route(sim: &SimState, pos: &mut (f32, f32), heading: &mut f32, path: &mut Vec<Cell>, budget: f32) -> f32 {
    if let Some(k) = path.iter().take(STRAIGHTEN_AHEAD + 1).rposition(|&c| clear_line(sim, *pos, centre(c))) { path.drain(..k); }
    let (start, mut left) = (*pos, budget);
    while left > 1e-4 {
        let Some(&next) = path.first() else { break };
        let (t, d) = (centre(next), dist(*pos, centre(next)));
        if d <= left { *pos = t; left -= d; path.remove(0); }
        else { let k = left / d; *pos = (pos.0 + (t.0 - pos.0) * k, pos.1 + (t.1 - pos.1) * k); left = 0.0; }
    }
    let moved = dist(start, *pos);
    if moved > 1e-4 { *heading = (pos.0 - start.0).atan2(pos.1 - start.1); }
    moved
}

fn walk(sim: &SimState, w: &mut Wolf, speed: f32, dt: f32) {
    w.pace = walk_route(sim, &mut w.pos, &mut w.heading, &mut w.path, speed * dt) / dt.max(1e-6);
}

fn prey(sim: &SimState, wolf: &Wolf) -> Option<BeaverId> {
    sim.agents.by_id.values()
        .filter(|a| on_land(sim, a) && !sim.predators.given_up.iter().any(|&(id, u)| id == a.id && sim.time_days < u))
        .map(|a| (dist(centre(a.cell), wolf.pos), a.id))
        .filter(|(d, _)| *d <= WOLF_SIGHT)
        .min_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1))).map(|(_, id)| id)
}

/// Shared pursuit step for both `WolfState::Charge` (beaver) and `WolfState::Hunt` (wildlife).
/// Returns `Some(true)` if caught, `Some(false)` if still pursuing, `None` if given up.
fn pursue(sim: &SimState, wolf: &mut Wolf, dt: f32, since: f64, target_pos: Option<(f32, f32)>) -> Option<bool> {
    let p = target_pos.filter(|_| sim.time_days - since <= WOLF_CHARGE_DAYS)?;
    let cell = (p.0.max(0.0) as usize, p.1.max(0.0) as usize);
    wolf.path = path_within_reach(sim, wolf.cell(), cell, 6000)?;
    walk(sim, wolf, WOLF_CHARGE_PER_DAY, dt);
    if dist(wolf.pos, p) <= WOLF_CATCH {
        wolf.path.clear();
        wolf.face(p);
        return Some(true);
    }
    Some(false)
}

fn act(sim: &mut SimState, wolf: &mut Wolf, dt: f32) -> bool {
    let now = sim.time_days;
    wolf.pace = 0.0;
    match wolf.state {
        WolfState::Roam => {
            if now >= wolf.leave_by {
                wolf.state = WolfState::Leave;
                wolf.path = land_path(sim, wolf.cell(), |c| edge_gap(&sim.nav, c) == 0, |c| edge_gap(&sim.nav, c) as f32, 40_000).unwrap_or_default();
                note(sim, Some(wolf), "LEAVING", format!("route_cells={}", wolf.path.len()));
                return wolf.path.is_empty();
            }
            if let Some(target) = prey(sim, wolf) {
                note(sim, Some(wolf), "CHARGE", format!("target={}", target.0));
                wolf.state = WolfState::Charge { target, since: now };
                wolf.path.clear();
                if let Some(a) = sim.agents.by_id.get_mut(&target) { (a.path, a.task) = (None, Task::Idle); }
                return false;
            }
            let skip: Vec<u32> = sim.predators.given_up_animals.iter().filter(|g| now < g.1).map(|g| g.0).collect();
            if let Some(animal) = crate::wildlife::prey_for_wolf(sim, wolf.pos, WOLF_SIGHT, &skip) {
                note(sim, Some(wolf), "HUNT", format!("animal={animal}"));
                (wolf.state, wolf.path) = (WolfState::Hunt { animal, since: now }, Vec::new());
                return false;
            }
            if wolf.path.is_empty() && now >= wolf.rest_until {
                wolf.path = roam_goal(sim, wolf);
                if wolf.path.is_empty() {
                    wolf.rest_until = now + WOLF_STUCK_WAIT_DAYS;
                    note(sim, Some(wolf), "STUCK", format!("waits until {:.2}", wolf.rest_until));
                }
            }
            walk(sim, wolf, WOLF_ROAM_PER_DAY, dt);
            false
        }
        WolfState::Charge { target, since } => {
            let pos = sim.agents.get(target).filter(|a| on_land(sim, a)).map(|a| centre(a.cell));
            match pursue(sim, wolf, dt, since, pos) {
                Some(true) => {
                    note(sim, Some(wolf), "CAUGHT", format!("target={}", target.0));
                    wolf.state = WolfState::Kill { target, until: now + WOLF_KILL_DAYS };
                }
                Some(false) => {}
                None => {
                    note(sim, Some(wolf), "GAVE_UP", format!("target={}", target.0));
                    sim.predators.given_up.retain(|&(id, u)| id != target && now < u);
                    sim.predators.given_up.push((target, now + WOLF_STUCK_WAIT_DAYS));
                    (wolf.state, wolf.path) = (WolfState::Roam, Vec::new());
                    if let Some(a) = sim.agents.by_id.get_mut(&target) { (a.path, a.task) = (None, Task::Idle); }
                }
            }
            false
        }
        WolfState::Hunt { animal, since } => {
            let pos = sim.wildlife.get(animal).filter(|a| a.doing != crate::wildlife::Doing::Leave).map(|a| a.pos);
            match pursue(sim, wolf, dt, since, pos) {
                Some(true) => {
                    note(sim, Some(wolf), "CAUGHT", format!("animal={animal}"));
                    crate::wildlife::catch(sim, animal);
                    wolf.state = WolfState::Feed { animal, until: now + WOLF_KILL_DAYS };
                }
                Some(false) => {}
                None => {
                    note(sim, Some(wolf), "GAVE_UP", format!("animal={animal}"));
                    sim.predators.given_up_animals.retain(|g| g.0 != animal && now < g.1);
                    sim.predators.given_up_animals.push((animal, now + WOLF_STUCK_WAIT_DAYS));
                    (wolf.state, wolf.path) = (WolfState::Roam, Vec::new());
                }
            }
            false
        }
        WolfState::Kill { target, until } if now + 1e-9 >= until => {
            let killed = sim.colony.kill(target, DeathCause::Predator);
            if killed { sim.predators.kills += 1; }
            note(sim, Some(wolf), "KILLED", format!("target={} killed={killed}", target.0));
            (wolf.state, wolf.path) = (WolfState::Roam, Vec::new());
            false
        }
        WolfState::Feed { animal, until } if now + 1e-9 >= until => {
            crate::wildlife::kill(sim, animal);
            note(sim, Some(wolf), "KILLED", format!("animal={animal}"));
            (wolf.state, wolf.path) = (WolfState::Roam, Vec::new());
            false
        }
        WolfState::Kill { .. } | WolfState::Feed { .. } => false,
        WolfState::Leave => {
            if wolf.path.is_empty() { return true; }
            walk(sim, wolf, WOLF_ROAM_PER_DAY, dt);
            false
        }
    }
}

fn roam_goal(sim: &mut SimState, wolf: &Wolf) -> Vec<Cell> {
    let ((w, h), here) = ((sim.nav.surface.width(), sim.nav.surface.height()), wolf.cell());
    for _ in 0..6 {
        let (u, v) = (sim.predators.unit(), sim.predators.unit());
        let c = (
            (here.0 as f32 + (u * 2.0 - 1.0) * WOLF_WANDER_RADIUS).clamp(0.0, (w - 1) as f32) as usize,
            (here.1 as f32 + (v * 2.0 - 1.0) * WOLF_WANDER_RADIUS).clamp(0.0, (h - 1) as f32) as usize,
        );
        if walkable(sim, c) && c != here {
            let p = land_path_toward(sim, here, |x| x == c, |x| dist(centre(x), centre(c)), WOLF_ROAM_SEARCH);
            if !p.is_empty() { return p; }
        }
    }
    Vec::new()
}

fn is_refuge(s: Option<Surface>) -> bool { matches!(s, Some(Surface::Water | Surface::Canal | Surface::LodgeEntrance | Surface::Lock)) }

fn nearest_water(nav: &NavGrid, from: Cell) -> Option<Cell> {
    let (w, h) = (nav.surface.width(), nav.surface.height());
    let (mut seen, mut queue) = (vec![false; w * h], VecDeque::from([from]));
    seen[from.1 * w + from.0] = true;
    while let Some(c) = queue.pop_front() {
        if is_refuge(nav.surface_at(c)) { return Some(c); }
        for (dx, dy) in [(-1i32, 0), (1, 0), (0, -1), (0, 1), (-1, -1), (1, -1), (-1, 1), (1, 1)] {
            let (x, y) = (c.0 as i32 + dx, c.1 as i32 + dy);
            if x < 0 || y < 0 || x as usize >= w || y as usize >= h { continue; }
            let i = y as usize * w + x as usize;
            if !matches!(nav.surface.as_slice()[i], Surface::Solid | Surface::LodgeInside) && !seen[i] {
                seen[i] = true;
                queue.push_back((x as usize, y as usize));
            }
        }
    }
    None
}

fn sees(sim: &SimState, a: &Agent, wolf: &Wolf) -> bool {
    !a.inside && dist(centre(a.cell), wolf.pos) <= crate::fog::BEAVER_SIGHT * sim.colony.beaver(a.id).map_or(1.0, crate::growth::sight_x)
}

pub fn flee(sim: &mut SimState, ag: &mut Agent, dt: f32) -> bool {
    if sim.predators.wolves.iter().any(|w| matches!(w.state, WolfState::Kill { target, .. } if target == ag.id)) {
        ag.path = None;
        ag.trail.clear();
        return true;
    }
    if let Task::Watch { lookout } = ag.task
        && sim.buildings.building(lookout).is_some_and(|b| b.cells().any(|c| c.0.abs_diff(ag.cell.0).max(c.1.abs_diff(ag.cell.1)) <= SENTRY_POST_REACH as usize))
    {
        return false;
    }
    let in_sight = sim.predators.wolves.iter().any(|w| sees(sim, ag, w)) || warned(sim, ag);
    if !in_sight {
        if sim.agents.fleeing.remove(&ag.id) { (ag.path, ag.task) = (None, Task::Idle); }
        return false;
    }
    if sim.agents.fleeing.insert(ag.id) { (ag.path, ag.task) = (None, Task::Idle); }
    if !matches!(ag.task, Task::Idle | Task::Rest) { return false; }
    ag.trail.clear();
    if is_refuge(sim.nav.surface_at(ag.cell)) { (ag.path, ag.task) = (None, Task::Idle); return false; }
    if !ag.path.as_ref().and_then(|p| p.steps.last()).is_some_and(|s| is_refuge(sim.nav.surface_at(s.0))) {
        let Some(r) = nearest_water(&sim.nav, ag.cell) else { return false };
        ag.path = sim.nav.find_path(ag.id, ag.cell, Goal::Cell(r));
    }
    let load = ag.load.take();
    ag.step_frac += dt * crate::agents::MOVE_COST_PER_DAY * (FLEE_PACE - 1.0);
    let moved = crate::agents::follow(&mut sim.nav, ag, dt);
    ag.load = load;
    crate::agents::note(ag, moved);
    true
}
