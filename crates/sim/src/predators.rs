//! One wolf, now and then. At most once a calendar year a single wolf comes in from the
//! map's edge and roams. A beaver it sees on land within `WOLF_SIGHT` is charged: that
//! beaver drops what it was doing and runs for the nearest water, where the wolf never
//! follows. A caught beaver dies in one short kill. The wolf leaves after a kill, or a
//! couple of days after a charge that caught nothing.
include!(concat!(env!("OUT_DIR"), "/predators_constants.rs"));

use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap, VecDeque};

use rand_core::{Rng, SeedableRng};
use rand_pcg::Pcg32;

use crate::agents::{Agent, Task};
use crate::colony::{BeaverId, DeathCause};
use crate::nav::{Goal, NavGrid, Surface};
use crate::water::Cell;
use crate::SimState;

#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum WolfState {
    /// Wandering, looking for a beaver on land.
    Roam,
    /// Running at a beaver it saw.
    Charge { target: BeaverId, since: f64 },
    /// Caught one: the kill is one short animation, then the beaver is dead.
    Kill { target: BeaverId, until: f64 },
    /// Heading for the map's edge.
    Leave,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Wolf {
    /// Where it stands, in cells (cell `c` spans `c..c+1`).
    pub pos: (f32, f32),
    /// Facing, as `atan2(dx, dy)` in cell coordinates.
    pub heading: f32,
    /// How fast it moved over the last step, cells per day (0 standing).
    pub pace: f32,
    pub state: WolfState,
    pub arrived: f64,
    /// It heads off once this day comes (and it is not mid-charge).
    pub leave_by: f64,
    path: Vec<Cell>,
    /// No new roaming route is looked for before this day (after a search found no way on).
    rest_until: f64,
}

impl Wolf {
    /// Cells left on the route it is walking.
    pub fn route_left(&self) -> usize {
        self.path.len()
    }
    pub fn rest_until(&self) -> f64 {
        self.rest_until
    }
    pub fn cell(&self) -> Cell {
        (self.pos.0.max(0.0) as usize, self.pos.1.max(0.0) as usize)
    }
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Predators {
    rng: Pcg32,
    pub wolf: Option<Wolf>,
    /// Calendar year of the last wolf, so there is never more than one a year.
    last_year: Option<u32>,
    /// The colony has seen this visit's wolf: the HUD notification shows from then on.
    pub spotted: bool,
    /// Where the charged beaver is running to.
    refuge: Option<Cell>,
    pub chance_per_day: f32,
    pub visits: u32,
    pub kills: u32,
}

impl Predators {
    pub fn new(seed: u64) -> Self {
        Self { rng: Pcg32::seed_from_u64(seed ^ 0x3017_F00D), wolf: None, last_year: None, spotted: false, refuge: None,
            chance_per_day: WOLF_CHANCE_PER_DAY, visits: 0, kills: 0 }
    }

    /// The beaver being charged or killed, if any.
    pub fn hunted(&self) -> Option<BeaverId> {
        match self.wolf.as_ref()?.state {
            WolfState::Charge { target, .. } | WolfState::Kill { target, .. } => Some(target),
            _ => None,
        }
    }

    fn unit(&mut self) -> f32 {
        (self.rng.next_u32() >> 8) as f32 / (1u32 << 24) as f32
    }
}

fn year(time_days: f64) -> u32 {
    crate::calendar::Date::at(time_days).year
}

/// Ground a wolf walks: dry land and thickets, never water (not even a film on the
/// ground), dams or lodges.
pub fn walkable(sim: &SimState, c: Cell) -> bool {
    matches!(sim.nav.surface_at(c), Some(Surface::Land | Surface::Thicket))
        && sim.water.depth.get(c.0, c.1).is_some_and(|&d| d <= WOLF_MAX_DEPTH)
}

fn centre(c: Cell) -> (f32, f32) {
    (c.0 as f32 + 0.5, c.1 as f32 + 0.5)
}

fn dist(a: (f32, f32), b: (f32, f32)) -> f32 {
    ((a.0 - b.0).powi(2) + (a.1 - b.1).powi(2)).sqrt()
}

const ROOT2: f32 = std::f32::consts::SQRT_2;

/// A land-only route (8 directions, no corner cutting) to a cell `goal` accepts, searching
/// at most `max_nodes` cells. The route leaves out the start.
fn land_path(sim: &SimState, from: Cell, goal: impl Fn(Cell) -> bool, heur: impl Fn(Cell) -> f32, max_nodes: usize) -> Option<Vec<Cell>> {
    land_search(sim, from, goal, heur, max_nodes, false)
}

/// As `land_path`, but when the search runs out it returns the route to the searched cell
/// nearest the goal (empty if that is the start), so a long way is walked a stretch at a time.
fn land_path_toward(sim: &SimState, from: Cell, goal: impl Fn(Cell) -> bool, heur: impl Fn(Cell) -> f32, max_nodes: usize) -> Vec<Cell> {
    land_search(sim, from, goal, heur, max_nodes, true).unwrap_or_default()
}

fn land_search(sim: &SimState, from: Cell, goal: impl Fn(Cell) -> bool, heur: impl Fn(Cell) -> f32, max_nodes: usize, partial: bool) -> Option<Vec<Cell>> {
    let (w, h) = (sim.nav.surface.width(), sim.nav.surface.height());
    let mut best: HashMap<Cell, (u32, Cell)> = HashMap::new();
    let mut heap = BinaryHeap::new();
    let key = |f: f32| (f * 1000.0) as u32;
    best.insert(from, (0, from));
    heap.push(Reverse((key(heur(from)), 0u32, from)));
    let route = |best: &HashMap<Cell, (u32, Cell)>, end: Cell| {
        let mut out = Vec::new();
        let mut c = end;
        while c != from {
            out.push(c);
            c = best[&c].1;
        }
        out.reverse();
        out
    };
    let mut nearest = (heur(from), from);
    let mut expanded = 0;
    while let Some(Reverse((_, g, cur))) = heap.pop() {
        if best.get(&cur).is_some_and(|b| b.0 < g) {
            continue;
        }
        if goal(cur) {
            return Some(route(&best, cur));
        }
        let left = heur(cur);
        if left < nearest.0 {
            nearest = (left, cur);
        }
        expanded += 1;
        if expanded > max_nodes {
            return partial.then(|| route(&best, nearest.1));
        }
        for (dx, dy) in [(-1i32, 0i32), (1, 0), (0, -1), (0, 1), (-1, -1), (1, -1), (-1, 1), (1, 1)] {
            let (x, y) = (cur.0 as i32 + dx, cur.1 as i32 + dy);
            if x < 0 || y < 0 || x as usize >= w || y as usize >= h {
                continue;
            }
            let to = (x as usize, y as usize);
            if !walkable(sim, to) {
                continue;
            }
            let diagonal = dx != 0 && dy != 0;
            if diagonal && !(walkable(sim, (x as usize, cur.1)) && walkable(sim, (cur.0, y as usize))) {
                continue;
            }
            let ng = g + key(if diagonal { ROOT2 } else { 1.0 });
            if best.get(&to).is_none_or(|b| ng < b.0) {
                best.insert(to, (ng, cur));
                heap.push(Reverse((ng + key(heur(to)), ng, to)));
            }
        }
    }
    None
}

/// Out on land (not swimming, not in a lodge), though maybe on a wet margin the wolf will not
/// set foot on: it can still be caught from the dry ground beside it.
fn on_land(sim: &SimState, a: &Agent) -> bool {
    !a.inside && matches!(sim.nav.surface_at(a.cell), Some(Surface::Land | Surface::Thicket))
}

/// A route to dry ground within reach of `to`.
fn path_within_reach(sim: &SimState, from: Cell, to: Cell, max_nodes: usize) -> Option<Vec<Cell>> {
    let t = centre(to);
    land_path(sim, from, |c| dist(centre(c), t) <= WOLF_CATCH, |c| (dist(centre(c), t) - WOLF_CATCH).max(0.0), max_nodes)
}

fn edge_gap(nav: &NavGrid, c: Cell) -> usize {
    let (w, h) = (nav.surface.width(), nav.surface.height());
    c.0.min(c.1).min(w - 1 - c.0).min(h - 1 - c.1)
}

/// For the session journal: what the wolf did and why.
fn note(sim: &SimState, wolf: Option<&Wolf>, what: &str, detail: String) {
    if !crate::trace::enabled() {
        return;
    }
    let at = wolf.map_or(String::new(), |w| format!(" pos=({:.1},{:.1}) state={:?}", w.pos.0, w.pos.1, w.state));
    let home = crate::agents::colony_home(sim).map_or(String::new(), |h| format!(" home_dist={:.1}", wolf.map_or(0.0, |w| dist(w.pos, centre(h)))));
    crate::trace::event(format!("WOLF_{what} day={:.4} step={}{at}{home} {detail}", sim.time_days, sim.steps));
}

/// Brings a wolf in from the map's edge now, whatever the calendar says (debugging and tests).
pub fn summon(sim: &mut SimState) -> bool {
    if sim.predators.wolf.is_some() {
        return false;
    }
    let (w, h) = (sim.nav.surface.width(), sim.nav.surface.height());
    if w < 3 || h < 3 {
        return false;
    }
    let home = crate::agents::colony_home(sim).map(centre);
    // Try edge cells at random; keep the one farthest from home so it walks in.
    let mut pick: Option<(f32, Cell)> = None;
    for _ in 0..200 {
        let along = sim.predators.unit();
        let side = (sim.predators.unit() * 4.0) as u32;
        let c = match side {
            0 => (((w - 1) as f32 * along) as usize, 0),
            1 => (((w - 1) as f32 * along) as usize, h - 1),
            2 => (0, ((h - 1) as f32 * along) as usize),
            _ => (w - 1, ((h - 1) as f32 * along) as usize),
        };
        if !walkable(sim, c) {
            continue;
        }
        let far = home.map_or(0.0, |p| dist(p, centre(c)));
        if pick.is_none_or(|(d, _)| far > d) {
            pick = Some((far, c));
        }
    }
    let Some((_, at)) = pick else { return false };
    let now = sim.time_days;
    let p = &mut sim.predators;
    p.wolf = Some(Wolf { pos: centre(at), heading: 0.0, pace: 0.0, state: WolfState::Roam, arrived: now, leave_by: now + WOLF_VISIT_DAYS, path: Vec::new(), rest_until: 0.0 });
    p.last_year = Some(year(now));
    p.spotted = false;
    p.refuge = None;
    p.visits += 1;
    note(sim, sim.predators.wolf.as_ref(), "ARRIVED", format!("visit={} leave_by={:.2}", sim.predators.visits, now + WOLF_VISIT_DAYS));
    true
}

/// Rolls once a day for a wolf: never in the colony's first weeks, never twice in a year.
fn maybe_arrive(sim: &mut SimState, dt_days: f32) {
    let now = sim.time_days;
    if sim.predators.wolf.is_some() || now < WOLF_GRACE_DAYS || sim.predators.last_year == Some(year(now)) {
        return;
    }
    let new_day = now.floor() > (now - dt_days as f64).floor();
    if new_day && sim.predators.unit() < sim.predators.chance_per_day {
        summon(sim);
    }
}

pub fn step(sim: &mut SimState, dt_days: f32) {
    if !sim.colony.is_founded() {
        return;
    }
    maybe_arrive(sim, dt_days);
    let Some(mut wolf) = sim.predators.wolf.take() else { return };
    let gone = act(sim, &mut wolf, dt_days);
    if gone {
        note(sim, Some(&wolf), "GONE", format!("kills_total={} arrived={:.2}", sim.predators.kills, wolf.arrived));
    }
    if !gone {
        spot(sim, &wolf);
        sim.predators.wolf = Some(wolf);
    } else {
        sim.predators.refuge = None;
        sim.predators.spotted = false;
    }
}

fn spot(sim: &mut SimState, wolf: &Wolf) {
    if sim.predators.spotted {
        return;
    }
    let seen = matches!(wolf.state, WolfState::Charge { .. } | WolfState::Kill { .. })
        || sim.agents.by_id.values().any(|a| !a.inside && dist(centre(a.cell), wolf.pos) <= crate::fog::BEAVER_SIGHT);
    sim.predators.spotted = seen;
}

/// Walks `dist` cells along the route; returns how far it went.
fn walk(wolf: &mut Wolf, budget: f32) -> f32 {
    let start = wolf.pos;
    let mut left = budget;
    while left > 1e-4 {
        let Some(&next) = wolf.path.first() else { break };
        let t = centre(next);
        let d = dist(wolf.pos, t);
        if d <= left {
            wolf.pos = t;
            left -= d;
            wolf.path.remove(0);
        } else {
            let k = left / d;
            wolf.pos = (wolf.pos.0 + (t.0 - wolf.pos.0) * k, wolf.pos.1 + (t.1 - wolf.pos.1) * k);
            left = 0.0;
        }
    }
    let moved = dist(start, wolf.pos);
    if moved > 1e-4 {
        wolf.heading = (wolf.pos.0 - start.0).atan2(wolf.pos.1 - start.1);
    }
    moved
}

/// The nearest beaver out on land within sight.
fn prey(sim: &SimState, wolf: &Wolf) -> Option<BeaverId> {
    sim.agents
        .by_id
        .values()
        .filter(|a| on_land(sim, a))
        .map(|a| (dist(centre(a.cell), wolf.pos), a.id))
        .filter(|(d, _)| *d <= WOLF_SIGHT)
        .min_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)))
        .map(|(_, id)| id)
}

/// One step of the wolf. Returns true once it has left the map.
fn act(sim: &mut SimState, wolf: &mut Wolf, dt: f32) -> bool {
    let now = sim.time_days;
    wolf.pace = 0.0;
    match wolf.state {
        WolfState::Roam => {
            if now >= wolf.leave_by {
                return start_leaving(sim, wolf);
            }
            if let Some(target) = prey(sim, wolf) {
                charge(sim, wolf, target);
                return false;
            }
            if wolf.path.is_empty() && now >= wolf.rest_until {
                wolf.path = roam_goal(sim, wolf);
                if wolf.path.is_empty() {
                    // No way on from here: wait a while, then try somewhere else.
                    wolf.rest_until = now + WOLF_STUCK_WAIT_DAYS;
                    note(sim, Some(wolf), "STUCK", format!("no roaming route; waits until {:.2}", wolf.rest_until));
                }
            }
            wolf.pace = walk(wolf, WOLF_ROAM_PER_DAY * dt) / dt.max(1e-6);
            false
        }
        WolfState::Charge { target, since } => {
            let beaver = sim.agents.get(target).filter(|a| on_land(sim, a)).map(|a| a.cell);
            let route = beaver.filter(|_| now - since <= WOLF_CHARGE_DAYS).and_then(|b| path_within_reach(sim, wolf.cell(), b, 6000).map(|p| (b, p)));
            let Some((at, path)) = route else {
                let why = match (sim.agents.get(target), now - since > WOLF_CHARGE_DAYS) {
                    (None, _) => "beaver gone".to_string(),
                    (Some(a), _) if a.inside => "beaver got home".to_string(),
                    (Some(a), _) if !on_land(sim, a) => format!("beaver reached water at ({},{})", a.cell.0, a.cell.1),
                    (Some(_), true) => "charge ran too long".to_string(),
                    (Some(a), false) => format!("no land route to the beaver at ({},{})", a.cell.0, a.cell.1),
                };
                note(sim, Some(wolf), "GAVE_UP", format!("target={} why={why} linger_until={:.2}", target.0, wolf.leave_by.min(now + WOLF_LINGER_DAYS)));
                // It reached water, got home or outran the charge: the wolf gives up.
                wolf.state = WolfState::Roam;
                wolf.path.clear();
                wolf.leave_by = wolf.leave_by.min(now + WOLF_LINGER_DAYS);
                calm(sim, target);
                return false;
            };
            wolf.path = path;
            wolf.pace = walk(wolf, WOLF_CHARGE_PER_DAY * dt) / dt.max(1e-6);
            if dist(wolf.pos, centre(at)) <= WOLF_CATCH {
                note(sim, Some(wolf), "CAUGHT", format!("target={} at=({},{})", target.0, at.0, at.1));
                wolf.path.clear();
                wolf.state = WolfState::Kill { target, until: now + WOLF_KILL_DAYS };
                let d = (centre(at).0 - wolf.pos.0, centre(at).1 - wolf.pos.1);
                if d.0.abs() + d.1.abs() > 1e-4 {
                    wolf.heading = d.0.atan2(d.1);
                }
            }
            false
        }
        WolfState::Kill { target, until } => {
            if now + 1e-9 < until {
                return false;
            }
            let killed = sim.colony.kill(target, DeathCause::Predator);
            if killed {
                sim.predators.kills += 1;
            }
            note(sim, Some(wolf), "KILLED", format!("target={} killed={killed}", target.0));
            sim.predators.refuge = None;
            start_leaving(sim, wolf)
        }
        WolfState::Leave => {
            if wolf.path.is_empty() {
                return true;
            }
            wolf.pace = walk(wolf, WOLF_ROAM_PER_DAY * dt) / dt.max(1e-6);
            false
        }
    }
}

fn start_leaving(sim: &SimState, wolf: &mut Wolf) -> bool {
    let why = if matches!(wolf.state, WolfState::Kill { .. }) { "made a kill" } else { "visit over" };
    wolf.state = WolfState::Leave;
    let from = wolf.cell();
    wolf.path = land_path(sim, from, |c| edge_gap(&sim.nav, c) == 0, |c| edge_gap(&sim.nav, c) as f32, 40_000).unwrap_or_default();
    let exit = wolf.path.last().map_or("none: slips away where it stands".to_string(), |c| format!("({},{}) {} cells", c.0, c.1, wolf.path.len()));
    note(sim, Some(wolf), "LEAVING", format!("why={why} route_out={exit}"));
    // Nowhere to walk out: it slips away where it stands.
    wolf.path.is_empty()
}

/// A route toward a random spot near the colony; when it is far or the way is long, the
/// stretch of it one bounded search finds.
fn roam_goal(sim: &mut SimState, wolf: &Wolf) -> Vec<Cell> {
    let (w, h) = (sim.nav.surface.width(), sim.nav.surface.height());
    let home = crate::agents::colony_home(sim);
    for _ in 0..4 {
        let (u, v) = (sim.predators.unit(), sim.predators.unit());
        let c = match home {
            Some(hc) => {
                let r = WOLF_HUNT_RADIUS as f32;
                let x = (hc.0 as f32 + (u * 2.0 - 1.0) * r).clamp(0.0, (w - 1) as f32);
                let y = (hc.1 as f32 + (v * 2.0 - 1.0) * r).clamp(0.0, (h - 1) as f32);
                (x as usize, y as usize)
            }
            None => ((u * (w - 1) as f32) as usize, (v * (h - 1) as f32) as usize),
        };
        if walkable(sim, c) && c != wolf.cell() {
            let t = centre(c);
            let p = land_path_toward(sim, wolf.cell(), |x| x == c, |x| dist(centre(x), t), WOLF_ROAM_SEARCH);
            if !p.is_empty() {
                return p;
            }
        }
    }
    Vec::new()
}

fn charge(sim: &mut SimState, wolf: &mut Wolf, target: BeaverId) {
    let at = sim.agents.get(target).map(|a| a.cell);
    note(sim, Some(wolf), "CHARGE", format!("target={} beaver_at={at:?} dist={:.1}", target.0, at.map_or(0.0, |c| dist(wolf.pos, centre(c)))));
    wolf.state = WolfState::Charge { target, since: sim.time_days };
    wolf.path.clear();
    sim.predators.spotted = true;
    sim.predators.refuge = None;
    if let Some(a) = sim.agents.by_id.get_mut(&target) {
        a.path = None;
        a.task = Task::Idle;
    }
}

/// The charge is over: the beaver goes back to what it was told to do.
fn calm(sim: &mut SimState, target: BeaverId) {
    sim.predators.refuge = None;
    if let Some(a) = sim.agents.by_id.get_mut(&target) {
        a.path = None;
        a.task = Task::Idle;
    }
}

fn is_refuge(s: Option<Surface>) -> bool {
    matches!(s, Some(Surface::Water | Surface::Canal | Surface::LodgeEntrance))
}

/// The nearest water a beaver can swim in, by a breadth-first walk over open ground.
fn nearest_water(nav: &NavGrid, from: Cell) -> Option<Cell> {
    let (w, h) = (nav.surface.width(), nav.surface.height());
    let mut seen = vec![false; w * h];
    let mut queue = VecDeque::from([from]);
    seen[from.1 * w + from.0] = true;
    while let Some(c) = queue.pop_front() {
        if is_refuge(nav.surface_at(c)) {
            return Some(c);
        }
        for (dx, dy) in [(-1i32, 0i32), (1, 0), (0, -1), (0, 1), (-1, -1), (1, -1), (-1, 1), (1, 1)] {
            let (x, y) = (c.0 as i32 + dx, c.1 as i32 + dy);
            if x < 0 || y < 0 || x as usize >= w || y as usize >= h {
                continue;
            }
            let i = y as usize * w + x as usize;
            let open = !matches!(nav.surface.as_slice()[i], Surface::Solid | Surface::LodgeInside);
            if open && !seen[i] {
                seen[i] = true;
                queue.push_back((x as usize, y as usize));
            }
        }
    }
    None
}

/// A beaver's turn while the wolf is after it: run for the nearest water, tread water once
/// there, or stay pinned during the kill. Returns false for every beaver the wolf is not after.
pub fn flee(sim: &mut SimState, ag: &mut Agent, dt: f32) -> bool {
    let Some(wolf) = sim.predators.wolf.as_ref() else { return false };
    match wolf.state {
        WolfState::Kill { target, .. } if target == ag.id => {
            ag.path = None;
            ag.trail.clear();
            return true;
        }
        WolfState::Charge { target, .. } if target == ag.id => {}
        _ => return false,
    }
    if ag.inside {
        return false;
    }
    ag.trail.clear();
    if is_refuge(sim.nav.surface_at(ag.cell)) {
        ag.path = None;
        return true;
    }
    let on_route = sim.predators.refuge.is_some_and(|r| ag.path.as_ref().is_some_and(|p| p.steps.last().map(|s| s.0) == Some(r)));
    if !on_route {
        let Some(r) = nearest_water(&sim.nav, ag.cell) else { return false };
        ag.path = sim.nav.find_path(ag.id, ag.cell, Goal::Cell(r));
        sim.predators.refuge = Some(r);
    }
    // Fear lends speed: a load in the teeth does not slow a beaver running for its life.
    let load = ag.load.take();
    let moved = crate::agents::follow(&mut sim.nav, ag, dt);
    ag.load = load;
    crate::agents::note(ag, moved);
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agents::Agents;

    /// A founded colony on open land with a strip of water down column `water_x`, and one
    /// beaver at `at`.
    fn setup(water_x: Option<usize>, at: Cell) -> (SimState, BeaverId) {
        let mut s = SimState::new(1);
        s.colony.found((0.0, 3.0), &crate::colony::RiverLand::default()).unwrap();
        let n = s.world.size;
        s.nav = NavGrid::new(n, n);
        s.water.depth.as_mut_slice().fill(0.0);
        if let Some(x) = water_x {
            for y in 0..n {
                for dx in 0..3 {
                    s.nav.set((x + dx, y), Surface::Water);
                    *s.water.depth.get_mut(x + dx, y).unwrap() = 1.0;
                }
            }
        }
        s.agents = Agents::default();
        let who = s.colony.beavers[0].id;
        s.agents.spawn(who, at);
        s.nav.reserve(who, at);
        s.time_days = 100.0;
        (s, who)
    }

    fn place_wolf(s: &mut SimState, at: Cell) {
        s.predators.wolf = Some(Wolf { pos: centre(at), heading: 0.0, pace: 0.0, state: WolfState::Roam, arrived: s.time_days, leave_by: s.time_days + WOLF_VISIT_DAYS, path: Vec::new(), rest_until: 0.0 });
        s.predators.last_year = Some(year(s.time_days));
    }

    /// One sim step of the beaver (just its flight) and of the wolf.
    fn tick(s: &mut SimState, who: BeaverId) {
        let dt = crate::STEP_DAYS;
        s.time_days += dt as f64;
        if let Some(mut a) = s.agents.by_id.remove(&who) {
            flee(s, &mut a, dt);
            s.agents.by_id.insert(who, a);
        }
        step(s, dt);
    }

    #[test]
    fn at_most_one_wolf_comes_each_year() {
        let (mut s, _) = setup(None, (10, 10));
        s.time_days = 0.0;
        s.predators.chance_per_day = 1.0;
        let mut arrivals: Vec<u32> = Vec::new();
        for _ in 0..(4.0 * crate::calendar::YEAR_DAYS) as usize {
            s.time_days += 1.0;
            step(&mut s, 1.0);
            if s.predators.wolf.is_some() {
                arrivals.push(year(s.time_days));
                s.predators.wolf = None; // sent away at once
            }
        }
        assert!(!arrivals.is_empty());
        let mut years = arrivals.clone();
        years.dedup();
        assert_eq!(years, arrivals, "two wolves in one year: {arrivals:?}");
        assert!(arrivals.len() >= 3, "a sure chance brings one every year: {arrivals:?}");
        assert!(s.time_days > WOLF_GRACE_DAYS);
    }

    #[test]
    fn a_beaver_in_sight_is_charged_and_one_out_of_sight_is_not() {
        let (mut s, who) = setup(None, (60, 60));
        place_wolf(&mut s, (60 + WOLF_SIGHT as usize + 8, 60));
        s.predators.wolf.as_mut().unwrap().leave_by = f64::MAX;
        s.predators.wolf.as_mut().unwrap().path = vec![(60 + WOLF_SIGHT as usize + 8, 61)];
        step(&mut s, crate::STEP_DAYS);
        assert_eq!(s.predators.hunted(), None, "charged a beaver out of sight");
        assert!(!matches!(s.predators.wolf.as_ref().unwrap().state, WolfState::Charge { .. }));
        s.predators.wolf.as_mut().unwrap().pos = centre((60 + WOLF_SIGHT as usize - 2, 60));
        step(&mut s, crate::STEP_DAYS);
        assert_eq!(s.predators.hunted(), Some(who));
        assert!(s.predators.spotted);
    }

    #[test]
    fn a_beaver_that_reaches_water_escapes_and_the_wolf_stays_on_land() {
        let (mut s, who) = setup(Some(64), (60, 60));
        place_wolf(&mut s, (52, 60));
        let mut wolf_cells = Vec::new();
        for _ in 0..200 {
            tick(&mut s, who);
            if let Some(w) = &s.predators.wolf {
                wolf_cells.push(w.cell());
            }
            if s.predators.hunted().is_none() && s.predators.wolf.as_ref().is_some_and(|w| w.state == WolfState::Roam) && wolf_cells.len() > 2 {
                break;
            }
        }
        assert!(s.colony.beaver(who).is_some(), "the beaver was caught");
        let a = s.agents.get(who).unwrap();
        assert!(is_refuge(s.nav.surface_at(a.cell)), "the beaver is not in the water: {:?}", a.cell);
        assert_eq!(s.predators.hunted(), None, "the wolf kept up the charge");
        assert!(wolf_cells.iter().all(|&c| walkable(&s, c)), "the wolf went into the water");
        let w = s.predators.wolf.as_ref().unwrap();
        assert!(w.leave_by <= s.time_days + WOLF_LINGER_DAYS + 1e-6);
    }

    #[test]
    fn a_caught_beaver_dies_and_the_wolf_leaves() {
        // Nowhere to swim: the beaver is caught.
        let (mut s, who) = setup(None, (60, 60));
        place_wolf(&mut s, (54, 60));
        s.colony.take_events();
        let mut caught = false;
        for _ in 0..400 {
            tick(&mut s, who);
            caught |= matches!(s.predators.wolf.as_ref().map(|w| w.state), Some(WolfState::Kill { .. }));
            if s.colony.beaver(who).is_none() {
                break;
            }
        }
        assert!(caught, "no kill animation");
        assert!(s.colony.beaver(who).is_none(), "the beaver survived");
        assert!(s.colony.take_events().iter().any(|e| matches!(e.event, crate::colony::Event::Died { id, cause: DeathCause::Predator, .. } if id == who)));
        assert_eq!(s.predators.kills, 1);
        assert_eq!(s.predators.wolf.as_ref().map(|w| w.state), Some(WolfState::Leave));
        for _ in 0..2000 {
            if s.predators.wolf.is_none() {
                break;
            }
            tick(&mut s, who);
        }
        assert!(s.predators.wolf.is_none(), "the wolf never left");
    }

    #[test]
    fn a_wolf_that_caught_nothing_leaves_a_couple_of_days_after_its_charge() {
        let (mut s, who) = setup(Some(64), (62, 60));
        place_wolf(&mut s, (55, 60));
        let mut charged_at = None;
        let mut gone_at = None;
        for _ in 0..(10.0 / crate::STEP_DAYS) as usize {
            tick(&mut s, who);
            if charged_at.is_none() && s.predators.hunted().is_some() {
                charged_at = Some(s.time_days);
            }
            // Out of reach in the water, the beaver is not charged again.
            if s.predators.wolf.is_none() {
                gone_at = Some(s.time_days);
                break;
            }
        }
        let (charged, gone) = (charged_at.expect("no charge"), gone_at.expect("the wolf stayed"));
        assert!(s.colony.beaver(who).is_some());
        assert!(gone - charged >= WOLF_LINGER_DAYS * 0.9 && gone - charged <= WOLF_LINGER_DAYS + 3.0, "left {} days after the charge", gone - charged);
    }
}
