//! Harbors and barges. A finished Harbor keeps `BARGES_PER_HARBOR` barges moored at its
//! waterfront and lets gates be upgraded into locks (see `locks`).
//!
//! A hauler with at least two loads of a good to move between two stores that both stand on
//! barge water (at least `BARGE_DEPTH` deep), joined by it, takes a free barge: it fetches the
//! barge, pushes it to the first store, loads up to `BARGE_LOADS` loads, pushes it to the
//! second, unloads, and takes it back to its harbor. A barge goes at `BARGE_PACE` of a
//! swimmer's speed (with the current, as a swimmer does) and passes dams only through locks;
//! it carries far more than a beaver, so over any distance it moves goods faster.
include!(concat!(env!("OUT_DIR"), "/harbor_constants.rs"));

use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap};

use crate::buildings::{Building, BuildingId, BuildingKind};
use crate::colony::BeaverId;
use crate::nav::{Gait, Path, Surface};
use crate::water::Cell;
use crate::SimState;

#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Barge {
    pub id: u32,
    pub harbor: BuildingId,
    /// Where it floats: moored, or wherever its hauler has pushed it.
    pub cell: Cell,
    /// The hauler using it, if any.
    pub hauler: Option<BeaverId>,
}

#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Fleet {
    pub barges: Vec<Barge>,
    next: u32,
    /// Which stretch of barge water each cell is on (0: none), worked out once a day, and the
    /// day it was.
    #[serde(skip)]
    regions: Vec<u32>,
    #[serde(skip)]
    regions_day: Option<i64>,
}

impl Fleet {
    pub fn get(&self, id: u32) -> Option<&Barge> {
        self.barges.iter().find(|b| b.id == id)
    }
    pub fn get_mut(&mut self, id: u32) -> Option<&mut Barge> {
        self.barges.iter_mut().find(|b| b.id == id)
    }
    /// The barge this hauler is pushing.
    pub fn of(&self, who: BeaverId) -> Option<&Barge> {
        self.barges.iter().find(|b| b.hauler == Some(who))
    }
}

/// Ice this thick (m) stops a barge.
pub const BARGE_ICE_M: f32 = 0.04;

/// Water a barge can float on: open water, a canal or a lock, at least `BARGE_DEPTH` deep (a
/// lock always is), and not iced over.

pub fn barge_water(sim: &SimState, c: Cell) -> bool {
    match sim.nav.surface_at(c) {
        Some(Surface::Lock) => true,
        // Ice thick enough to walk on holds a barge fast.
        Some(Surface::Water | Surface::Canal | Surface::LodgeEntrance) => sim.water.depth.get(c.0, c.1).is_some_and(|&d| d >= BARGE_DEPTH) && sim.water.ice.get(c.0, c.1).is_none_or(|&t| t < BARGE_ICE_M),
        _ => false,
    }
}

/// Barge water beside a building, where a barge ties up to it.
pub fn docks(sim: &SimState, b: &Building) -> Vec<Cell> {
    let cells: Vec<Cell> = b.cells().collect();
    let mut out: Vec<Cell> = cells
        .iter()
        .flat_map(|&(x, y)| [(x + 1, y), (x.wrapping_sub(1), y), (x, y + 1), (x, y.wrapping_sub(1))])
        .filter(|c| !cells.contains(c) && barge_water(sim, *c))
        .collect();
    out.sort();
    out.dedup();
    out
}

/// The quickest barge route from `from` to any of `to`, on barge water only, at barge pace.
pub fn route(sim: &SimState, from: Cell, to: &[Cell]) -> Option<Path> {
    if to.is_empty() {
        return None;
    }
    if to.contains(&from) {
        return Some(Path { steps: Vec::new(), cost: 0.0 });
    }
    let (w, h) = (sim.nav.surface.width(), sim.nav.surface.height());
    let idx = |c: Cell| c.1 * w + c.0;
    let mut best: HashMap<usize, (f32, Option<Cell>)> = HashMap::new();
    let mut heap = BinaryHeap::new();
    best.insert(idx(from), (0.0, None));
    heap.push((Reverse((0.0f32 * 1e4) as u64), from));
    let mut done = None;
    let mut seen = 0usize;
    while let Some((Reverse(k), c)) = heap.pop() {
        let cost = k as f32 / 1e4;
        if best.get(&idx(c)).is_some_and(|b| b.0 + 1e-4 < cost) {
            continue;
        }
        if to.contains(&c) {
            done = Some(c);
            break;
        }
        seen += 1;
        if seen > 60_000 {
            return None;
        }
        for (dx, dy) in [(1i32, 0i32), (-1, 0), (0, 1), (0, -1), (1, 1), (1, -1), (-1, 1), (-1, -1)] {
            let (x, y) = (c.0 as i32 + dx, c.1 as i32 + dy);
            if x < 0 || y < 0 || x as usize >= w || y as usize >= h {
                continue;
            }
            let n = (x as usize, y as usize);
            if !barge_water(sim, n) {
                continue;
            }
            // No cutting a corner past a bank, except into or out of a lock: on a diagonal dam
            // the water either side of a lock meets it only at its corners.
            if dx != 0 && dy != 0 && !corner_ok(sim, c, n) {
                continue;
            }
            let Some(step) = sim.nav.move_cost(c, n, BARGE_PACE) else { continue };
            let next = cost + step;
            if best.get(&idx(n)).is_none_or(|b| next + 1e-4 < b.0) {
                best.insert(idx(n), (next, Some(c)));
                heap.push((Reverse((next * 1e4) as u64), n));
            }
        }
    }
    let end = done?;
    let total = best[&idx(end)].0;
    let mut steps = Vec::new();
    let mut c = end;
    while let Some(&(_, Some(prev))) = best.get(&idx(c)) {
        steps.push((c, Gait::Swim));
        c = prev;
    }
    steps.reverse();
    Some(Path { steps, cost: total })
}

/// Whether barge water joins these two stores (worked out at most once a day for each pair).
pub fn joined(sim: &SimState, a: BuildingId, b: BuildingId) -> bool {
    let (Some(x), Some(y)) = (sim.buildings.building(a), sim.buildings.building(b)) else { return false };
    let w = sim.nav.surface.width();
    let region = |c: Cell| sim.fleet.regions.get(c.1 * w + c.0).copied().unwrap_or(0);
    let here: Vec<u32> = docks(sim, x).into_iter().map(region).filter(|&r| r != 0).collect();
    docks(sim, y).into_iter().map(region).any(|r| r != 0 && here.contains(&r))
}

/// Whether a barge may go corner to corner from `a` to `b` (both barge water): round no bank,
/// unless one end is a lock, whose far side on a diagonal dam touches it only at a corner.
fn corner_ok(sim: &SimState, a: Cell, b: Cell) -> bool {
    let lock = |c: Cell| sim.nav.surface_at(c) == Some(Surface::Lock);
    lock(a) || lock(b) || (barge_water(sim, (b.0, a.1)) && barge_water(sim, (a.0, b.1)))
}

/// Label the stretches of barge water, each joined cell to cell (diagonals round no bank, but
/// into and out of a lock; see `corner_ok`).
fn label_regions(sim: &mut SimState) {
    let (w, h) = (sim.nav.surface.width(), sim.nav.surface.height());
    let mut regions = vec![0u32; w * h];
    let wet: Vec<bool> = (0..w * h).map(|i| barge_water(sim, (i % w, i / w))).collect();
    let mut next = 0u32;
    let mut queue = std::collections::VecDeque::new();
    for start in 0..w * h {
        if !wet[start] || regions[start] != 0 {
            continue;
        }
        next += 1;
        regions[start] = next;
        queue.push_back(start);
        while let Some(i) = queue.pop_front() {
            let (x, y) = ((i % w) as i32, (i / w) as i32);
            for (dx, dy) in [(1i32, 0i32), (-1, 0), (0, 1), (0, -1), (1, 1), (1, -1), (-1, 1), (-1, -1)] {
                let (nx, ny) = (x + dx, y + dy);
                if nx < 0 || ny < 0 || nx as usize >= w || ny as usize >= h {
                    continue;
                }
                let j = ny as usize * w + nx as usize;
                let corner = dx != 0 && dy != 0;
                if wet[j] && regions[j] == 0 && (!corner || corner_ok(sim, (x as usize, y as usize), (nx as usize, ny as usize))) {
                    regions[j] = next;
                    queue.push_back(j);
                }
            }
        }
    }
    sim.fleet.regions = regions;
}

/// Keep each finished harbor's barges: new ones moored at its docks, those of a harbor that
/// is gone taken away, and any whose hauler has stopped using it let go.
pub fn step(sim: &mut SimState) {
    let day = sim.time_days.floor() as i64;
    let any = sim.buildings.buildings.iter().any(|b| b.kind == BuildingKind::Harbor);
    if any && sim.fleet.regions_day != Some(day) {
        label_regions(sim);
        sim.fleet.regions_day = Some(day);
    }
    let harbors: Vec<(BuildingId, Vec<Cell>)> = sim
        .buildings
        .buildings
        .iter()
        .filter(|b| b.kind == BuildingKind::Harbor && sim.finished(b))
        .map(|b| (b.id, docks(sim, b)))
        .collect();
    sim.fleet.barges.retain(|b| harbors.iter().any(|h| h.0 == b.harbor));
    for (id, docks) in &harbors {
        let have = sim.fleet.barges.iter().filter(|b| b.harbor == *id).count();
        for k in have..BARGES_PER_HARBOR {
            let Some(&cell) = docks.get(k % docks.len().max(1)) else { break };
            let n = sim.fleet.next;
            sim.fleet.next += 1;
            sim.fleet.barges.push(Barge { id: n, harbor: *id, cell, hauler: None });
        }
    }
    let using: Vec<(u32, BeaverId)> = sim.fleet.barges.iter().filter_map(|b| b.hauler.map(|h| (b.id, h))).collect();
    for (barge, who) in using {
        let still = sim.agents.get(who).is_some_and(|a| matches!(a.task, crate::agents::Task::Barge { barge: x, .. } if x == barge));
        if !still {
            if let Some(b) = sim.fleet.get_mut(barge) {
                b.hauler = None;
            }
        }
    }
}

/// A free barge for a trip from `from` to `to`, the nearest one to `at`, if barge water joins
/// them and the barge can get to `from`.
pub fn free_barge(sim: &SimState, at: Cell) -> Option<u32> {
    sim.fleet
        .barges
        .iter()
        .filter(|b| b.hauler.is_none())
        .min_by_key(|b| (b.cell.0.abs_diff(at.0) + b.cell.1.abs_diff(at.1), b.id))
        .map(|b| b.id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::buildings::{BuildingKind as K, Good, Status};
    use crate::nav::NavGrid;

    fn put(s: &mut SimState, id: u64, kind: K, pos: (usize, usize)) -> BuildingId {
        let b = Building { id: BuildingId(id), kind, pos, status: Status::Complete, output: 0.0, condition: 1.0, reinforced: false, dam: None, canal: None };
        s.work.add_complete(&b).unwrap();
        s.buildings.buildings.push(b);
        BuildingId(id)
    }

    /// A long channel of deep water (rows 30 to 34) with stores on its bank at both ends, a
    /// harbor between them, and a logistics building with one hauler.
    fn waterway() -> (SimState, BuildingId, BuildingId, crate::colony::BeaverId) {
        let mut s = SimState::new(1);
        s.colony.found((40.0, 22.0), &crate::colony::RiverLand::default()).unwrap();
        let n = s.world.size;
        s.water.depth.as_mut_slice().fill(0.0);
        for y in 30..35 {
            for x in 2..110 {
                *s.water.depth.get_mut(x, y).unwrap() = 1.0;
            }
        }
        let a = put(&mut s, 1, K::Warehouse, (10, 27));
        let b = put(&mut s, 2, K::Warehouse, (90, 27));
        put(&mut s, 3, K::Harbor, (45, 27));
        let l = put(&mut s, 4, K::Logistics, (40, 20));
        s.buildings.update_capacity(crate::buildings::Terrain { depth: &s.water.depth });
        s.buildings.stores.put_at(a, Good::Sticks, 60.0).unwrap();
        s.nav = NavGrid::new(n, n);
        crate::agents::place_beavers(&mut s);
        let who = s.colony.beavers.iter().find(|b| b.can_work()).unwrap().id;
        s.work.assign(l, 0, who).unwrap();
        s.apply(crate::Command::FillStore { store: b, on: true }).unwrap();
        (s, a, b, who)
    }

    #[test]
    fn a_hauler_moves_many_loads_at_once_by_barge_along_a_waterway() {
        let (mut s, a, b, who) = waterway();
        let dt = crate::STEP_DAYS;
        let (mut biggest, mut used_barge) = (0.0f32, false);
        for _ in 0..(6.0 / dt) as usize {
            s.time_days += dt as f64;
            step(&mut s);
            crate::agents::step(&mut s, dt);
            if let Some(ag) = s.agents.get(who) {
                if matches!(ag.task, crate::agents::Task::Barge { .. }) {
                    used_barge = true;
                    biggest = biggest.max(ag.load.map_or(0.0, |l| l.amount));
                }
            }
            if s.work.stock_of(b, Good::Sticks) >= 20.0 {
                break;
            }
        }
        assert_eq!(s.fleet.barges.len(), BARGES_PER_HARBOR, "the harbor's barges are moored");
        assert!(used_barge, "the hauler never took a barge");
        assert!(biggest > 2.5 * crate::agents::load_size(Good::Sticks), "a barge took only {biggest}");
        assert!(s.work.stock_of(b, Good::Sticks) >= 2.0 * crate::agents::load_size(Good::Sticks), "the far store got {}", s.work.stock_of(b, Good::Sticks));
        assert!(s.work.stock_of(a, Good::Sticks) < 60.0);
    }

    #[test]
    fn a_barge_stays_on_deep_water_and_passes_a_dam_only_by_its_lock() {
        let (mut s, _, _, _) = waterway();
        // A dam across the channel at x 60.
        for y in 30..35 {
            *s.water.dam_height.get_mut(60, y).unwrap() = 1.0;
        }
        s.nav = NavGrid::new(s.world.size, s.world.size);
        crate::agents::place_beavers(&mut s);
        let to = [(70usize, 32usize)];
        assert!(route(&s, (50, 32), &to).is_none(), "a dam blocks a barge");
        // A lock in the dam lets it through.
        s.locks.at.insert((60, 32), crate::locks::Lock { cell: (60, 32), building: BuildingId(99), built: true });
        s.nav = NavGrid::new(s.world.size, s.world.size);
        crate::agents::place_beavers(&mut s);
        let p = route(&s, (50, 32), &to).expect("through the lock");
        assert!(p.steps.iter().any(|st| st.0 == (60, 32)), "by the lock cell");
        assert!(p.steps.iter().all(|st| barge_water(&s, st.0)), "on barge water all the way");
        // A swimming beaver could wait out the lock too, but over a dam this thin it is quicker
        // to climb.
        let path = s.nav.find_path(crate::colony::BeaverId(1), (50, 32), crate::nav::Goal::Cell((70, 32))).unwrap();
        assert!(path.steps.iter().all(|st| st.0 != (60, 32)), "the beaver goes over or round, not through the lock");
        let wait = crate::nav::lock_wait(Surface::Water, Surface::Lock);
        assert!(wait > crate::nav::CLIMB_COST, "a lock's wait ({wait}) is longer than a climb");
    }

    #[test]
    fn a_barge_passes_a_lock_in_a_diagonal_dam() {
        let (mut s, _, _, _) = waterway();
        for y in 20..46 {
            for x in 40..80 {
                *s.water.depth.get_mut(x, y).unwrap() = 1.0;
            }
        }
        // A dam running corner to corner right across the pond (staircased, so no water gets
        // by), with a lock in it: the water on its far side meets the lock only at a corner.
        for y in 20..46usize {
            let x = 60 + y - 32;
            for c in [(x, y), (x + 1, y)] {
                *s.water.dam_height.get_mut(c.0, c.1).unwrap() = 1.0;
            }
        }
        let lock = (60usize, 32usize);
        s.locks.at.insert(lock, crate::locks::Lock { cell: lock, building: BuildingId(99), built: true });
        s.nav = NavGrid::new(s.world.size, s.world.size);
        crate::agents::place_beavers(&mut s);
        assert!(barge_water(&s, (lock.0 + 1, lock.1 - 1)), "open water off the lock's far corner");
        assert!(!barge_water(&s, (lock.0 + 1, lock.1)) && !barge_water(&s, (lock.0, lock.1 - 1)), "dam on its far sides");
        let to = [(70usize, 20usize)];
        let p = route(&s, (50, 40), &to).expect("through the lock");
        assert!(p.steps.iter().any(|st| st.0 == lock), "by the lock cell");
        label_regions(&mut s);
        let w = s.nav.surface.width();
        let region = |c: Cell| s.fleet.regions[c.1 * w + c.0];
        assert_eq!(region((50, 40)), region((70, 20)), "one stretch of barge water through the lock");
    }
}
