//! Wolves. While fewer than `MAX_WOLVES` are about, another may come any day (after the
//! colony's first weeks). Each appears on land well away from every beaver, wanders the map at
//! random and hunts on its own. Every beaver out
//! of its lodge with the wolf within its own sight (`BEAVER_SIGHT`) drops what it was doing,
//! runs for the nearest water and waits there till the wolf is out of sight; the wolf never
//! follows into water. A beaver the wolf sees on land within `WOLF_SIGHT` is charged, and a
//! caught beaver dies in one short kill. A lookout with its sentry at the post sees farther
//! (`LOOKOUT_SIGHT`, more than the wolf's own); while it sees the wolf, the sentry slaps its tail
//! on the water and every beaver within that range runs as if it saw the wolf itself.
//! Kill or miss, the wolf wanders on and hunts again until its visit is over, then walks off
//! the map. Beavers working far over land meet wolves; those near water get away.
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
    /// Running at a wild animal it saw (see `wildlife`).
    Hunt { animal: u32, since: f64 },
    /// Caught one: it dies when the kill ends.
    Feed { animal: u32, until: f64 },
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
    /// A beaver out of its lodge, or a lookout's sentry, sees this wolf this step: only then
    /// does the player see where it is. Worked out every step, so not saved.
    #[serde(skip)]
    pub seen: bool,
    /// Where it has been over its last steps (`wildlife::Track`). Not saved.
    #[serde(skip)]
    pub track: crate::wildlife::Track,
    /// Where and when the colony last saw it. Not saved: a loaded wolf the colony had spotted
    /// counts as seen where it stands.
    #[serde(skip)]
    last_seen: Option<((f32, f32), f64)>,
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
    /// Tells this wolf from the others while it is about.
    pub fn key(&self) -> u64 {
        self.arrived.to_bits()
    }
}

/// The first wolf is saved where the one wolf always was, so older saves load; the others
/// are saved after the world (`save.rs`).
mod first_wolf {
    use super::Wolf;
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    pub fn serialize<S: Serializer>(wolves: &[Wolf], s: S) -> Result<S::Ok, S::Error> {
        wolves.first().serialize(s)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<Wolf>, D::Error> {
        Ok(Option::<Wolf>::deserialize(d)?.into_iter().collect())
    }
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Predators {
    rng: Pcg32,
    /// The wolves about, at most `MAX_WOLVES`, in the order they came.
    #[serde(with = "first_wolf")]
    pub wolves: Vec<Wolf>,
    /// Calendar year of the last wolf (kept for older saves; wolves may come any year).
    last_year: Option<u32>,
    /// The colony has seen a wolf lately (within `WOLF_SEEN_DAYS`): the HUD alert shows while
    /// it has.
    pub spotted: bool,
    /// Unused (each fleeing beaver keeps its own route); kept so older saves load.
    refuge: Option<Cell>,
    pub chance_per_day: f32,
    pub visits: u32,
    pub kills: u32,
    /// Lookouts whose sentry sees a wolf now, slapping the alarm. Worked out every step, so
    /// not saved.
    #[serde(skip)]
    pub alarms: Vec<crate::buildings::BuildingId>,
    /// Beavers the wolf gave up a charge at, and until when it leaves them be: one it cannot
    /// reach over land would otherwise be charged and given up on every step, the wolf
    /// standing still. Not saved; a reload only forgets a few hours of it.
    #[serde(skip)]
    given_up: Vec<(BeaverId, f64)>,
    /// Wild animals it gave up on, likewise.
    #[serde(skip)]
    given_up_animals: Vec<(u32, f64)>,
    /// A beaver out of its lodge, or a lookout's sentry, sees some wolf this step.
    #[serde(skip)]
    pub in_sight: bool,
}

impl Predators {
    pub fn new(seed: u64) -> Self {
        Self { rng: Pcg32::seed_from_u64(seed ^ 0x3017_F00D), wolves: Vec::new(), last_year: None, spotted: false, refuge: None,
            chance_per_day: WOLF_CHANCE_PER_DAY, visits: 0, kills: 0, alarms: Vec::new(), given_up: Vec::new(), given_up_animals: Vec::new(), in_sight: false }
    }

    /// A beaver being charged or killed, if any.
    pub fn hunted(&self) -> Option<BeaverId> {
        self.wolves.iter().find_map(|w| match w.state {
            WolfState::Charge { target, .. } | WolfState::Kill { target, .. } => Some(target),
            _ => None,
        })
    }

    /// The wolf `key` names (`Wolf::key`), while it is about.
    pub fn wolf(&self, key: u64) -> Option<&Wolf> {
        self.wolves.iter().find(|w| w.key() == key)
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

pub(crate) fn centre(c: Cell) -> (f32, f32) {
    (c.0 as f32 + 0.5, c.1 as f32 + 0.5)
}

pub(crate) fn dist(a: (f32, f32), b: (f32, f32)) -> f32 {
    ((a.0 - b.0).powi(2) + (a.1 - b.1).powi(2)).sqrt()
}

const ROOT2: f32 = std::f32::consts::SQRT_2;

/// A land-only route (8 directions, no shortcuts across corners) to a cell `goal` accepts, searching
/// at most `max_nodes` cells. The route leaves out the start.
fn land_path(sim: &SimState, from: Cell, goal: impl Fn(Cell) -> bool, heur: impl Fn(Cell) -> f32, max_nodes: usize) -> Option<Vec<Cell>> {
    land_search(sim, from, goal, heur, max_nodes, false)
}

/// As `land_path`, but when the search runs out it returns the route to the searched cell
/// nearest the goal (empty if that is the start), so a long way is walked a stretch at a time.
pub(crate) fn land_path_toward(sim: &SimState, from: Cell, goal: impl Fn(Cell) -> bool, heur: impl Fn(Cell) -> f32, max_nodes: usize) -> Vec<Cell> {
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
pub(crate) fn path_within_reach(sim: &SimState, from: Cell, to: Cell, max_nodes: usize) -> Option<Vec<Cell>> {
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

/// Brings a wolf in now, whatever the calendar says (debugging and tests), while fewer than
/// `MAX_WOLVES` are about: at a random spot on land at least `WOLF_SPAWN_MIN_DIST` from every
/// beaver, or the farthest one found if no spot is that far.
pub fn summon(sim: &mut SimState) -> bool {
    if sim.predators.wolves.len() >= MAX_WOLVES {
        return false;
    }
    let (w, h) = (sim.nav.surface.width(), sim.nav.surface.height());
    if w < 3 || h < 3 {
        return false;
    }
    let beavers: Vec<(f32, f32)> = sim.agents.by_id.values().map(|a| centre(a.cell)).collect();
    let mut pick: Option<(f32, Cell)> = None;
    for _ in 0..400 {
        let c = (((w - 1) as f32 * sim.predators.unit()) as usize, ((h - 1) as f32 * sim.predators.unit()) as usize);
        if !walkable(sim, c) {
            continue;
        }
        let far = beavers.iter().map(|&b| dist(b, centre(c))).fold(f32::INFINITY, f32::min);
        if far >= WOLF_SPAWN_MIN_DIST {
            pick = Some((far, c));
            break;
        }
        if pick.is_none_or(|(d, _)| far > d) {
            pick = Some((far, c));
        }
    }
    let Some((_, at)) = pick else { return false };
    let now = sim.time_days;
    let p = &mut sim.predators;
    // Two summoned at the same moment (tests, debugging) still tell apart by `key`.
    let arrived = p.wolves.iter().map(|w| w.arrived).fold(now, |a, b| if b >= a { b + 1e-9 } else { a });
    p.wolves.push(Wolf { pos: centre(at), heading: 0.0, pace: 0.0, state: WolfState::Roam, arrived, leave_by: now + WOLF_VISIT_DAYS, path: Vec::new(), rest_until: 0.0, seen: false, last_seen: None, track: Default::default() });
    p.last_year = Some(year(now));
    p.visits += 1;
    note(sim, sim.predators.wolves.last(), "ARRIVED", format!("visit={} wolves={} leave_by={:.2}", sim.predators.visits, sim.predators.wolves.len(), now + WOLF_VISIT_DAYS));
    true
}

/// Rolls once a day for another wolf while fewer than `MAX_WOLVES` are about, never in the
/// colony's first weeks.
fn maybe_arrive(sim: &mut SimState, dt_days: f32) {
    let now = sim.time_days;
    if sim.predators.wolves.len() >= MAX_WOLVES || now < WOLF_GRACE_DAYS {
        return;
    }
    let new_day = now.floor() > (now - dt_days as f64).floor();
    // A save keeps the chance it was made with; it never falls below today's.
    let chance = sim.predators.chance_per_day.max(WOLF_CHANCE_PER_DAY);
    if new_day && sim.predators.unit() < chance {
        summon(sim);
    }
}

pub fn step(sim: &mut SimState, dt_days: f32) {
    if !sim.colony.is_founded() {
        return;
    }
    maybe_arrive(sim, dt_days);
    // Each wolf acts on its own.
    let mut wolves = std::mem::take(&mut sim.predators.wolves);
    wolves.retain_mut(|wolf| {
        let gone = act(sim, wolf, dt_days);
        if gone {
            note(sim, Some(wolf), "GONE", format!("kills_total={} arrived={:.2}", sim.predators.kills, wolf.arrived));
        }
        !gone
    });
    sim.predators.wolves = wolves;
    look(sim);
}

/// Which wolves the colony sees now, which lookouts slap the alarm, and whether the wolf alert
/// stands.
fn look(sim: &mut SimState) {
    let mut wolves = std::mem::take(&mut sim.predators.wolves);
    let watching: Vec<(crate::buildings::BuildingId, (f32, f32))> = if wolves.is_empty() {
        Vec::new()
    } else {
        sim.buildings.buildings.iter().filter(|b| on_watch(sim, b)).map(|b| (b.id, lookout_centre(b))).collect()
    };
    let alarms: Vec<crate::buildings::BuildingId> = watching.iter().filter(|(_, at)| wolves.iter().any(|w| dist(*at, w.pos) <= LOOKOUT_SIGHT)).map(|l| l.0).collect();
    let sentries = |id: crate::buildings::BuildingId| -> Vec<crate::colony::BeaverId> {
        sim.work.place(id).map(|p| p.slots.iter().filter(|s| s.role == crate::work::Role::Sentry).filter_map(|s| s.beaver).collect()).unwrap_or_default()
    };
    // For the records: who has each wolf in sight (beavers seeing it, and the sentries of the
    // lookouts in range), each wolf counted once for each beaver however often it is seen.
    // The beavers out in a lookout's range, warned by its alarm.
    let in_range = |id: crate::buildings::BuildingId| -> u32 {
        sim.buildings.building(id).map_or(0, |b| {
            let at = lookout_centre(b);
            sim.agents.by_id.values().filter(|a| !a.inside && dist(at, centre(a.cell)) <= LOOKOUT_SIGHT).count() as u32
        })
    };
    let mut spotted: Vec<(crate::colony::BeaverId, u64, u32)> = Vec::new();
    for wolf in &mut wolves {
        let by_lookout = watching.iter().any(|(_, at)| dist(*at, wolf.pos) <= LOOKOUT_SIGHT);
        spot(sim, wolf, by_lookout);
        if wolf.seen {
            let key = wolf.key();
            spotted.extend(sim.agents.by_id.values().filter(|a| sees(a, wolf)).map(|a| (a.id, key, 0)));
            for (id, _) in watching.iter().filter(|(_, at)| dist(*at, wolf.pos) <= LOOKOUT_SIGHT) {
                let n = in_range(*id);
                spotted.extend(sentries(*id).into_iter().map(|g| (g, key, n)));
            }
        }
    }
    // Each wolf counted once for each beaver however often it is seen; a sentry's warning
    // counted the first time it sees that wolf.
    for (id, key, warned) in spotted {
        if sim.deeds.spotted(id, key) && warned > 0 {
            sim.deeds.of(id).warned += warned;
        }
    }
    let now = sim.time_days;
    let p = &mut sim.predators;
    p.alarms = alarms;
    p.in_sight = wolves.iter().any(|w| w.seen);
    p.spotted = wolves.iter().any(|w| w.last_seen.is_some_and(|(_, t)| now - t <= WOLF_SEEN_DAYS));
    p.wolves = wolves;
}

/// The middle of a lookout, where its sight is measured from.
pub fn lookout_centre(b: &crate::buildings::Building) -> (f32, f32) {
    let (w, d) = b.kind.spec().footprint;
    (b.pos.0 as f32 + w as f32 / 2.0, b.pos.1 as f32 + d as f32 / 2.0)
}

/// A finished lookout with its sentry at the post (within `SENTRY_POST_REACH` of it, in water
/// or on land, not in a lodge).
pub fn on_watch(sim: &SimState, b: &crate::buildings::Building) -> bool {
    if b.kind != crate::buildings::BuildingKind::Lookout || !b.is_complete() {
        return false;
    }
    let reach = SENTRY_POST_REACH as usize;
    let at_post = |a: &Agent| !a.inside && b.cells().any(|c| c.0.abs_diff(a.cell.0).max(c.1.abs_diff(a.cell.1)) <= reach);
    sim.work.place(b.id).is_some_and(|p| {
        p.slots.iter().any(|s| s.role == crate::work::Role::Sentry && s.beaver.and_then(|w| sim.agents.get(w)).is_some_and(at_post))
    })
}

/// Whether a lookout slapping the alarm has this beaver (out of its lodge) in its range.
fn warned(sim: &SimState, a: &Agent) -> bool {
    !a.inside
        && sim.predators.alarms.iter().filter_map(|&id| sim.buildings.building(id)).any(|b| dist(lookout_centre(b), centre(a.cell)) <= LOOKOUT_SIGHT)
}

/// Whether the colony sees this wolf now: a beaver out of its lodge has it in sight, or a
/// lookout on watch does (`by_lookout`). The alert stays a while after it is lost from sight,
/// pointing at where it was last seen, then clears.
fn spot(sim: &SimState, wolf: &mut Wolf, by_lookout: bool) {
    let now = sim.time_days;
    // Only sight counts: being charged or caught tells the colony nothing.
    wolf.seen = by_lookout || sim.agents.by_id.values().any(|a| sees(a, wolf));
    // A loaded save that had a wolf spotted counts it as seen where it stands.
    if wolf.seen || (sim.predators.spotted && wolf.last_seen.is_none()) {
        wolf.last_seen = Some((wolf.pos, now));
    }
}

/// The wolves the colony sees now.
pub fn seen(sim: &SimState) -> impl Iterator<Item = &Wolf> {
    sim.predators.wolves.iter().filter(|w| w.seen)
}

/// The wolf the colony sees nearest its home, if it sees any.
pub fn nearest_seen(sim: &SimState) -> Option<&Wolf> {
    let home = crate::agents::colony_home(sim).map(centre);
    let away = |w: &Wolf| home.map_or(0.0, |h| dist(w.pos, h));
    seen(sim).min_by(|a, b| away(a).total_cmp(&away(b)))
}

/// Where a wolf is, while the colony sees one: the one nearest the colony's home.
pub fn in_sight(sim: &SimState) -> Option<(f32, f32)> {
    nearest_seen(sim).map(|w| w.pos)
}

/// Where the colony last saw a wolf, while its alert stands: the latest sighting.
pub fn last_seen(sim: &SimState) -> Option<(f32, f32)> {
    let now = sim.time_days;
    sim.predators.wolves.iter().filter_map(|w| w.last_seen).filter(|(_, t)| now - t <= WOLF_SEEN_DAYS).max_by(|a, b| a.1.total_cmp(&b.1)).map(|l| l.0)
}

/// Whether the wolf can go straight from `a` to `b`: every cell the line crosses is ground it
/// walks.
fn clear_line(sim: &SimState, a: (f32, f32), b: (f32, f32)) -> bool {
    let n = (dist(a, b) * 4.0).ceil().max(1.0) as usize;
    (0..=n).all(|i| {
        let t = i as f32 / n as f32;
        let (x, y) = (a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t);
        x >= 0.0 && y >= 0.0 && walkable(sim, (x as usize, y as usize))
    })
}

/// How many route cells ahead the wolf looks for a straight way.
const STRAIGHTEN_AHEAD: usize = 8;

/// Walks `budget` cells along the route; returns how far it went. Cells it can cut straight
/// past are skipped, so it runs in straight lines rather than stepping cell to cell on the
/// grid's eight headings.
fn walk(sim: &SimState, wolf: &mut Wolf, budget: f32) -> f32 {
    walk_route(sim, &mut wolf.pos, &mut wolf.heading, &mut wolf.path, budget)
}

/// Walks `budget` cells along `path` from `pos` (on land), going straight past cells it can
/// (see `walk`); turns `heading` the way it went. Returns how far it went.
pub(crate) fn walk_route(sim: &SimState, pos: &mut (f32, f32), heading: &mut f32, path: &mut Vec<Cell>, budget: f32) -> f32 {
    let far = path.iter().take(STRAIGHTEN_AHEAD + 1).rposition(|&c| clear_line(sim, *pos, centre(c)));
    if let Some(k) = far {
        path.drain(..k);
    }
    let start = *pos;
    let mut left = budget;
    while left > 1e-4 {
        let Some(&next) = path.first() else { break };
        let t = centre(next);
        let d = dist(*pos, t);
        if d <= left {
            *pos = t;
            left -= d;
            path.remove(0);
        } else {
            let k = left / d;
            *pos = (pos.0 + (t.0 - pos.0) * k, pos.1 + (t.1 - pos.1) * k);
            left = 0.0;
        }
    }
    let moved = dist(start, *pos);
    if moved > 1e-4 {
        *heading = (pos.0 - start.0).atan2(pos.1 - start.1);
    }
    moved
}

/// The nearest beaver out on land within sight.
fn prey(sim: &SimState, wolf: &Wolf) -> Option<BeaverId> {
    sim.agents
        .by_id
        .values()
        .filter(|a| on_land(sim, a) && !sim.predators.given_up.iter().any(|&(id, until)| id == a.id && sim.time_days < until))
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
            // No beaver about: a rabbit or a deer will do.
            let skip: Vec<u32> = sim.predators.given_up_animals.iter().filter(|g| now < g.1).map(|g| g.0).collect();
            if let Some(animal) = crate::wildlife::prey_for_wolf(sim, wolf.pos, WOLF_SIGHT, &skip) {
                note(sim, Some(wolf), "HUNT", format!("animal={animal}"));
                wolf.state = WolfState::Hunt { animal, since: now };
                wolf.path.clear();
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
            wolf.pace = walk(sim, wolf, WOLF_ROAM_PER_DAY * dt) / dt.max(1e-6);
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
                note(sim, Some(wolf), "GAVE_UP", format!("target={} why={why}", target.0));
                // It reached water, got home or outran the charge: the wolf gives up, leaves that
                // beaver be for a while and wanders on.
                sim.predators.given_up.retain(|&(id, until)| id != target && now < until);
                sim.predators.given_up.push((target, now + WOLF_STUCK_WAIT_DAYS));
                wolf.state = WolfState::Roam;
                wolf.path.clear();
                calm(sim, target);
                return false;
            };
            wolf.path = path;
            wolf.pace = walk(sim, wolf, WOLF_CHARGE_PER_DAY * dt) / dt.max(1e-6);
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
            // It hunts on until its visit is over.
            wolf.state = WolfState::Roam;
            wolf.path.clear();
            false
        }
        WolfState::Hunt { animal, since } => {
            let at = sim.wildlife.get(animal).filter(|a| a.doing != crate::wildlife::Doing::Leave).map(|a| a.pos);
            let route = at.filter(|_| now - since <= WOLF_CHARGE_DAYS).and_then(|p| {
                let c = (p.0.max(0.0) as usize, p.1.max(0.0) as usize);
                path_within_reach(sim, wolf.cell(), c, 6000).map(|path| (p, path))
            });
            let Some((p, path)) = route else {
                note(sim, Some(wolf), "GAVE_UP", format!("animal={animal}"));
                sim.predators.given_up_animals.retain(|g| g.0 != animal && now < g.1);
                sim.predators.given_up_animals.push((animal, now + WOLF_STUCK_WAIT_DAYS));
                wolf.state = WolfState::Roam;
                wolf.path.clear();
                return false;
            };
            wolf.path = path;
            wolf.pace = walk(sim, wolf, WOLF_CHARGE_PER_DAY * dt) / dt.max(1e-6);
            if dist(wolf.pos, p) <= WOLF_CATCH {
                note(sim, Some(wolf), "CAUGHT", format!("animal={animal}"));
                crate::wildlife::catch(sim, animal);
                wolf.path.clear();
                wolf.state = WolfState::Feed { animal, until: now + WOLF_KILL_DAYS };
                let d = (p.0 - wolf.pos.0, p.1 - wolf.pos.1);
                if d.0.abs() + d.1.abs() > 1e-4 {
                    wolf.heading = d.0.atan2(d.1);
                }
            }
            false
        }
        WolfState::Feed { animal, until } => {
            if now + 1e-9 < until {
                return false;
            }
            crate::wildlife::kill(sim, animal);
            note(sim, Some(wolf), "KILLED", format!("animal={animal}"));
            wolf.state = WolfState::Roam;
            wolf.path.clear();
            false
        }
        WolfState::Leave => {
            if wolf.path.is_empty() {
                return true;
            }
            wolf.pace = walk(sim, wolf, WOLF_ROAM_PER_DAY * dt) / dt.max(1e-6);
            false
        }
    }
}

fn start_leaving(sim: &SimState, wolf: &mut Wolf) -> bool {
    let why = "visit over";
    wolf.state = WolfState::Leave;
    let from = wolf.cell();
    wolf.path = land_path(sim, from, |c| edge_gap(&sim.nav, c) == 0, |c| edge_gap(&sim.nav, c) as f32, 40_000).unwrap_or_default();
    let exit = wolf.path.last().map_or("none: slips away where it stands".to_string(), |c| format!("({},{}) {} cells", c.0, c.1, wolf.path.len()));
    note(sim, Some(wolf), "LEAVING", format!("why={why} route_out={exit}"));
    // Nowhere to walk out: it slips away where it stands.
    wolf.path.is_empty()
}

/// A route toward a random spot within `WOLF_WANDER_RADIUS` of where the wolf stands, so it
/// wanders the map with no regard for where the colony is; when the way is long, the stretch
/// of it one bounded search finds.
fn roam_goal(sim: &mut SimState, wolf: &Wolf) -> Vec<Cell> {
    let (w, h) = (sim.nav.surface.width(), sim.nav.surface.height());
    let here = wolf.cell();
    for _ in 0..6 {
        let (u, v) = (sim.predators.unit(), sim.predators.unit());
        let r = WOLF_WANDER_RADIUS;
        let x = (here.0 as f32 + (u * 2.0 - 1.0) * r).clamp(0.0, (w - 1) as f32);
        let y = (here.1 as f32 + (v * 2.0 - 1.0) * r).clamp(0.0, (h - 1) as f32);
        let c = (x as usize, y as usize);
        if walkable(sim, c) && c != here {
            let t = centre(c);
            let p = land_path_toward(sim, here, |x| x == c, |x| dist(centre(x), t), WOLF_ROAM_SEARCH);
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
    if let Some(a) = sim.agents.by_id.get_mut(&target) {
        a.path = None;
        a.task = Task::Idle;
    }
}

/// The charge is over: the beaver goes back to what it was told to do.
fn calm(sim: &mut SimState, target: BeaverId) {
    if let Some(a) = sim.agents.by_id.get_mut(&target) {
        a.path = None;
        a.task = Task::Idle;
    }
}

fn is_refuge(s: Option<Surface>) -> bool {
    matches!(s, Some(Surface::Water | Surface::Canal | Surface::LodgeEntrance | Surface::Lock))
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

/// Whether a beaver has the wolf in sight: out of its lodge, within `BEAVER_SIGHT`.
fn sees(a: &Agent, wolf: &Wolf) -> bool {
    !a.inside && dist(centre(a.cell), wolf.pos) <= crate::fog::BEAVER_SIGHT
}

/// A beaver's turn while it has the wolf in sight, or a lookout in range slaps the alarm: run
/// for the nearest water and tread water once there; pinned during its own kill. Returns false, and leaves the beaver to its work,
/// when it doesn't see the wolf. Once the wolf is out of sight it starts its work afresh.
pub fn flee(sim: &mut SimState, ag: &mut Agent, dt: f32) -> bool {
    let caught = sim.predators.wolves.iter().any(|w| matches!(w.state, WolfState::Kill { target, .. } if target == ag.id));
    if caught {
        ag.path = None;
        ag.trail.clear();
        return true;
    }
    let in_sight = sim.predators.wolves.iter().any(|w| sees(ag, w)) || warned(sim, ag);
    if !in_sight {
        if sim.agents.fleeing.remove(&ag.id) {
            ag.path = None;
            ag.task = Task::Idle;
        }
        return false;
    }
    if sim.agents.fleeing.insert(ag.id) {
        ag.path = None;
        ag.task = Task::Idle;
    }
    ag.trail.clear();
    if is_refuge(sim.nav.surface_at(ag.cell)) {
        ag.path = None;
        return true;
    }
    let on_route = ag.path.as_ref().and_then(|p| p.steps.last()).is_some_and(|s| is_refuge(sim.nav.surface_at(s.0)));
    if !on_route {
        let Some(r) = nearest_water(&sim.nav, ag.cell) else { return false };
        ag.path = sim.nav.find_path(ag.id, ag.cell, Goal::Cell(r));
    }
    // Fear lends speed: a beaver running for its life runs (`FLEE_PACE`), and a load in its
    // teeth does not slow it.
    let load = ag.load.take();
    ag.step_frac += dt * crate::agents::MOVE_COST_PER_DAY * (FLEE_PACE - 1.0);
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
        s.predators.wolves = vec![Wolf { pos: centre(at), heading: 0.0, pace: 0.0, state: WolfState::Roam, arrived: s.time_days, leave_by: s.time_days + WOLF_VISIT_DAYS, path: Vec::new(), rest_until: 0.0, seen: false, last_seen: None, track: Default::default() }];
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
    fn wolves_come_one_a_day_up_to_the_most_at_once_and_save_with_the_world() {
        let (mut s, _) = setup(None, (60, 60));
        s.time_days = WOLF_GRACE_DAYS;
        s.predators.chance_per_day = 1.0;
        let mut counts = Vec::new();
        for _ in 0..MAX_WOLVES + 3 {
            s.time_days += 1.0;
            step(&mut s, crate::STEP_DAYS);
            counts.push(s.predators.wolves.len());
        }
        assert_eq!(counts[..MAX_WOLVES], (1..=MAX_WOLVES).collect::<Vec<_>>()[..], "one more each day: {counts:?}");
        assert!(counts.iter().all(|&n| n <= MAX_WOLVES), "never more than {MAX_WOLVES}: {counts:?}");
        assert!(!summon(&mut s), "no more come while {MAX_WOLVES} are about");
        let keys: std::collections::BTreeSet<u64> = s.predators.wolves.iter().map(Wolf::key).collect();
        assert_eq!(keys.len(), MAX_WOLVES, "each wolf tells apart");
        for w in &s.predators.wolves {
            assert!(walkable(&s, w.cell()));
        }

        // A save keeps every wolf, where it was, doing what it was.
        let bytes = crate::save::encode(&s, "test").unwrap();
        let (_, back) = crate::save::decode(&bytes).unwrap();
        let was: Vec<_> = s.predators.wolves.iter().map(|w| (w.key(), w.pos, w.state, w.leave_by)).collect();
        let now: Vec<_> = back.predators.wolves.iter().map(|w| (w.key(), w.pos, w.state, w.leave_by)).collect();
        assert_eq!(was, now);
    }

    #[test]
    fn wolves_keep_coming_and_appear_far_from_every_beaver() {
        let (mut s, _) = setup(None, (60, 60));
        s.time_days = 0.0;
        s.predators.chance_per_day = 1.0;
        let mut arrivals = 0;
        for _ in 0..(crate::calendar::YEAR_DAYS) as usize {
            s.time_days += 1.0;
            step(&mut s, 1.0);
            if s.predators.wolves.pop().is_some() {
                arrivals += 1;
                assert!(s.time_days >= WOLF_GRACE_DAYS);
            }
        }
        assert!(arrivals > 100, "a sure chance brings one each day there is none: {arrivals}");
        for _ in 0..50 {
            assert!(summon(&mut s));
            let w = s.predators.wolves.pop().unwrap();
            assert!(dist(w.pos, centre((60, 60))) >= WOLF_SPAWN_MIN_DIST, "a wolf appeared near the beaver at {:?}", w.pos);
            assert!(walkable(&s, w.cell()));
        }
    }

    #[test]
    fn a_beaver_in_sight_is_charged_and_one_out_of_sight_is_not() {
        let (mut s, who) = setup(None, (60, 60));
        place_wolf(&mut s, (60 + WOLF_SIGHT as usize + 8, 60));
        s.predators.wolves.first_mut().unwrap().leave_by = f64::MAX;
        s.predators.wolves.first_mut().unwrap().path = vec![(60 + WOLF_SIGHT as usize + 8, 61)];
        step(&mut s, crate::STEP_DAYS);
        assert_eq!(s.predators.hunted(), None, "charged a beaver out of sight");
        assert!(!matches!(s.predators.wolves.first().unwrap().state, WolfState::Charge { .. }));
        s.predators.wolves.first_mut().unwrap().pos = centre((60 + WOLF_SIGHT as usize - 2, 60));
        step(&mut s, crate::STEP_DAYS);
        assert_eq!(s.predators.hunted(), Some(who));
        assert!(!s.predators.spotted, "a charge from beyond the beaver's sight tells the colony nothing");
    }

    #[test]
    fn a_beaver_runs_from_a_wolf_it_sees_and_not_from_one_beyond_its_sight() {
        let sight = crate::fog::BEAVER_SIGHT as usize;
        assert!(WOLF_SIGHT > crate::fog::BEAVER_SIGHT, "the wolf sees farther than a beaver");
        let (mut s, who) = setup(Some(64), (60, 60));
        // Beyond its sight, though the wolf sees it and charges: the beaver carries on, and
        // the colony knows nothing.
        place_wolf(&mut s, (60 - sight - 5, 60));
        s.predators.wolves.first_mut().unwrap().leave_by = f64::MAX;
        s.predators.wolves.first_mut().unwrap().rest_until = f64::MAX;
        s.predators.wolves.first_mut().unwrap().state = WolfState::Charge { target: who, since: s.time_days };
        let mut a = s.agents.by_id.remove(&who).unwrap();
        assert!(!flee(&mut s, &mut a, crate::STEP_DAYS), "being charged told it of a wolf it cannot see");
        s.agents.by_id.insert(who, a);
        look(&mut s);
        assert!(!s.predators.spotted);
        // In its sight: it runs for water.
        s.predators.wolves.first_mut().unwrap().pos = centre((60 - sight + 5, 60));
        for _ in 0..200 {
            tick(&mut s, who);
        }
        assert!(s.predators.spotted);
        let a = s.agents.get(who).unwrap();
        assert!(is_refuge(s.nav.surface_at(a.cell)), "it is not in the water: {:?}", a.cell);
        // Out of sight again: it goes back to its work.
        s.predators.wolves.clear();
        let mut a = s.agents.by_id.remove(&who).unwrap();
        assert!(!flee(&mut s, &mut a, crate::STEP_DAYS));
        assert!(!s.agents.fleeing.contains(&who));
    }

    #[test]
    fn the_wolf_alert_clears_a_while_after_the_wolf_is_lost_from_sight() {
        let sight = crate::fog::BEAVER_SIGHT as usize;
        let (mut s, _) = setup(Some(64), (60, 60));
        place_wolf(&mut s, (60 - sight + 5, 60));
        let w = s.predators.wolves.first().cloned().unwrap();
        look(&mut s);
        assert!(s.predators.spotted);
        assert_eq!(in_sight(&s), Some(w.pos));
        let seen_at = w.pos;
        // Gone from sight: the alert stands a while, at the place it was seen, then clears.
        s.predators.wolves.first_mut().unwrap().pos = centre((60 - sight - 20, 60));
        s.time_days += WOLF_SEEN_DAYS * 0.5;
        look(&mut s);
        assert!(s.predators.spotted);
        assert_eq!(in_sight(&s), None, "out of sight, where it is stays hidden");
        assert_eq!(last_seen(&s), Some(seen_at), "the alert points where it was seen, not where it is");
        s.time_days += WOLF_SEEN_DAYS;
        look(&mut s);
        assert!(!s.predators.spotted);
        assert_eq!(last_seen(&s), None);
        // Seen again: it is back.
        s.predators.wolves.first_mut().unwrap().pos = centre((60 - sight + 5, 60));
        look(&mut s);
        assert!(s.predators.spotted);
    }

    #[test]
    fn a_beaver_runs_from_a_wolf_faster_than_it_walks() {
        let (mut s, who) = setup(Some(100), (40, 60));
        place_wolf(&mut s, (40 + 6 - crate::fog::BEAVER_SIGHT as usize, 60));
        s.predators.wolves.first_mut().unwrap().rest_until = f64::MAX;
        let steps = 30;
        for _ in 0..steps {
            // The wolf keeps in sight behind it.
            let at = s.agents.get(who).unwrap().cell;
            s.predators.wolves.first_mut().unwrap().pos = centre((at.0 - 20, at.1));
            let mut a = s.agents.by_id.remove(&who).unwrap();
            assert!(flee(&mut s, &mut a, crate::STEP_DAYS));
            s.agents.by_id.insert(who, a);
        }
        let ran = s.agents.get(who).unwrap().cell.0.abs_diff(40) as f32;
        let walk = steps as f32 * crate::STEP_DAYS * crate::agents::MOVE_COST_PER_DAY / crate::nav::WALK_COST;
        assert!(ran >= walk * (FLEE_PACE - 0.25), "ran {ran} cells where a walk covers {walk}");
    }

    #[test]
    fn a_manned_lookout_warns_beavers_in_its_range() {
        let (mut s, who) = setup(Some(64), (60, 60));
        // A finished lookout 20 cells off, and a sentry beside it.
        let id = crate::buildings::BuildingId(77);
        let pos = (60, 80);
        s.buildings.buildings.push(crate::buildings::Building { id, kind: crate::buildings::BuildingKind::Lookout, pos, status: crate::buildings::Status::Complete, output: 0.0, condition: 1.0, reinforced: false, dam: None, canal: None });
        s.work.add_site(id, crate::buildings::BuildingKind::Lookout, &[], 1.0).unwrap();
        s.work.add_labour(id, 1.0).unwrap();
        s.work.finish_site(id, &[crate::work::Role::Sentry], None).unwrap();
        let sentry = s.colony.beavers[1].id;
        s.agents.spawn(sentry, (pos.0, pos.1 - 1));
        // The wolf, out of the beaver's sight and the wolf's own, but in the lookout's.
        let lookout = lookout_centre(s.buildings.building(id).unwrap());
        let at = (lookout.0 as usize, lookout.1 as usize + LOOKOUT_SIGHT as usize - 4);
        assert!(dist(centre(at), centre((60, 60))) > WOLF_SIGHT.max(crate::fog::BEAVER_SIGHT));
        place_wolf(&mut s, at);
        s.predators.wolves.first_mut().unwrap().rest_until = f64::MAX;
        step(&mut s, crate::STEP_DAYS);
        assert!(s.predators.alarms.is_empty(), "nobody keeps watch");
        s.work.assign(id, 0, sentry).unwrap();
        step(&mut s, crate::STEP_DAYS);
        assert_eq!(s.predators.alarms, vec![id], "the sentry sees the wolf");
        assert!(s.predators.spotted);
        let mut a = s.agents.by_id.remove(&who).unwrap();
        assert!(flee(&mut s, &mut a, crate::STEP_DAYS), "the warned beaver carries on working");
        s.agents.by_id.insert(who, a);
        // Away from its post the sentry sees nothing.
        s.agents.by_id.get_mut(&sentry).unwrap().cell = (pos.0, pos.1 - 20);
        step(&mut s, crate::STEP_DAYS);
        assert!(s.predators.alarms.is_empty());
    }

    #[test]
    fn a_beaver_that_reaches_water_escapes_and_the_wolf_stays_on_land() {
        let (mut s, who) = setup(Some(64), (60, 60));
        place_wolf(&mut s, (52, 60));
        let mut wolf_cells = Vec::new();
        for _ in 0..200 {
            tick(&mut s, who);
            if let Some(w) = s.predators.wolves.first() {
                wolf_cells.push(w.cell());
            }
            if s.predators.hunted().is_none() && s.predators.wolves.first().is_some_and(|w| w.state == WolfState::Roam) && wolf_cells.len() > 2 {
                break;
            }
        }
        assert!(s.colony.beaver(who).is_some(), "the beaver was caught");
        let a = s.agents.get(who).unwrap();
        assert!(is_refuge(s.nav.surface_at(a.cell)), "the beaver is not in the water: {:?}", a.cell);
        assert_eq!(s.predators.hunted(), None, "the wolf kept up the charge");
        assert!(wolf_cells.iter().all(|&c| walkable(&s, c)), "the wolf went into the water");
        assert_eq!(s.predators.wolves.first().map(|w| w.state), Some(WolfState::Roam), "it wanders on");
    }

    #[test]
    fn a_caught_beaver_dies_and_the_wolf_hunts_on_until_its_visit_is_over() {
        // Nowhere to swim: the beaver is caught.
        let (mut s, who) = setup(None, (60, 60));
        place_wolf(&mut s, (54, 60));
        s.colony.take_events();
        let mut caught = false;
        for _ in 0..400 {
            tick(&mut s, who);
            caught |= matches!(s.predators.wolves.first().map(|w| w.state), Some(WolfState::Kill { .. }));
            if s.colony.beaver(who).is_none() {
                break;
            }
        }
        assert!(caught, "no kill animation");
        assert!(s.colony.beaver(who).is_none(), "the beaver survived");
        assert!(s.colony.take_events().iter().any(|e| matches!(e.event, crate::colony::Event::Died { id, cause: DeathCause::Predator, .. } if id == who)));
        assert_eq!(s.predators.kills, 1);
        assert_eq!(s.predators.wolves.first().map(|w| w.state), Some(WolfState::Roam), "it hunts on");
        s.predators.wolves.first_mut().unwrap().leave_by = s.time_days;
        // Others may come meanwhile; this one goes.
        let key = s.predators.wolves[0].key();
        for _ in 0..2000 {
            if s.predators.wolf(key).is_none() {
                break;
            }
            tick(&mut s, who);
        }
        assert!(s.predators.wolf(key).is_none(), "the wolf never left");
    }

    #[test]
    fn a_wolf_wanders_near_itself_with_no_regard_for_the_colony() {
        let (mut s, _) = setup(None, (10, 10));
        let far = (s.world.size - 20, s.world.size - 20);
        place_wolf(&mut s, far);
        let wolf = s.predators.wolves.first().cloned().unwrap();
        for _ in 0..20 {
            let route = roam_goal(&mut s, &wolf);
            let end = *route.last().expect("a way on");
            assert!(dist(centre(end), wolf.pos) <= WOLF_WANDER_RADIUS * std::f32::consts::SQRT_2 + 1.0, "it aimed beyond its wander: {end:?}");
            assert!(dist(centre(end), centre((10, 10))) > dist(wolf.pos, centre((10, 10))) - WOLF_WANDER_RADIUS * 1.5, "it made for the colony");
        }
    }
}
