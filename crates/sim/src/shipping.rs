//! Water transport (owner: `shipping`). A barge runs a standing route between two docks: at the
//! first it loads the listed goods from the dock's stock, sails to the second over water deep
//! enough to float it, unloads into that dock's stock and sails back for more. Docks are stores,
//! so gatherers drop goods at a dock by their camp and builders fetch from a dock by their site:
//! a barge route is how a colony moves stone and clay from a far deposit home.
//!
//! A dam stops a barge; a lock built on the dam lets it through, up or down, at the cost of some
//! time. Ice stops barges in winter. Goods aboard are off the colony ledger until unloaded.

use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashSet};

use crate::buildings::{Amount, BargeId, BargeState, Building, BuildingId, BuildingKind, Good};
use crate::water::Cell;
use crate::SimState;

/// Cells a barge sails per day on open water: slower than a beaver (who walks 30 a day, swims
/// 50, and goes at half that with a load of 3), but it carries a great deal more.
pub const BARGE_SPEED: f32 = 20.0;
/// Goods a barge carries.
pub const BARGE_CAPACITY: f32 = 40.0;
/// Water depth a laden barge needs.
pub const BARGE_DRAFT: f32 = 0.15;
/// Cost of a step along a canal, in open-water steps (canals are straight and still).
pub const CANAL_STEP: f32 = 0.6;
/// A pass through a lock takes as long as this many open-water steps.
pub const LOCK_STEP: f32 = 10.0;
/// Ice this thick stops barges.
pub const ICE_STOPS: f32 = 0.4;
/// Days at a dock to load or unload.
pub const DWELL_DAYS: f32 = 0.1;
/// A barge leaves with at least this much once the dock has nothing more to give it ...
pub const MIN_LOAD: f32 = 6.0;
/// ... or with whatever it has after waiting this long.
pub const MAX_WAIT_DAYS: f32 = 2.0;
/// A barge with no way through tries again after this long.
pub const RETRY_DAYS: f32 = 0.5;

/// A barge's standing route.
#[derive(Clone, Debug, PartialEq)]
pub struct Service {
    pub from: BuildingId,
    pub to: BuildingId,
    pub goods: Vec<Good>,
}

/// Where a barge on a route is in its round.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Leg {
    #[default]
    ToLoad,
    Loading,
    ToUnload,
    Unloading,
}

/// Cells of every finished lock.
pub fn lock_cells(sim: &SimState) -> HashSet<Cell> {
    sim.buildings.buildings.iter().filter(|b| b.kind == BuildingKind::Lock && b.is_complete()).flat_map(|b| b.cells()).collect()
}

/// Cost of sailing into `c` (open-water steps); `None` where a barge cannot go: shallow water,
/// land, a dam (unless through a lock) or, when `ice` is on, thick ice.
pub fn step_cost(sim: &SimState, locks: &HashSet<Cell>, c: Cell, ice: bool) -> Option<f32> {
    let w = &sim.water;
    let depth = *w.depth.get(c.0, c.1)?;
    if ice && w.ice.get(c.0, c.1).is_some_and(|&i| i >= ICE_STOPS) {
        return None;
    }
    if locks.contains(&c) {
        return Some(LOCK_STEP);
    }
    if w.has_dam(c.0, c.1) || depth < BARGE_DRAFT {
        return None;
    }
    Some(if w.canal.get(c.0, c.1) == Some(&true) { CANAL_STEP } else { 1.0 })
}

fn neighbours(c: Cell, n: usize) -> impl Iterator<Item = Cell> {
    [(c.0 > 0).then(|| (c.0 - 1, c.1)), (c.0 + 1 < n).then(|| (c.0 + 1, c.1)), (c.1 > 0).then(|| (c.0, c.1 - 1)), (c.1 + 1 < n).then(|| (c.0, c.1 + 1))]
        .into_iter()
        .flatten()
}

/// Cells where a barge can lie at `dock`: on or beside its footprint, deep enough.
pub fn berths(sim: &SimState, locks: &HashSet<Cell>, dock: &Building) -> Vec<Cell> {
    let n = sim.water.depth.width();
    let mut out: Vec<Cell> = Vec::new();
    for c in dock.cells() {
        for b in std::iter::once(c).chain(neighbours(c, n)) {
            if !out.contains(&b) && step_cost(sim, locks, b, false).is_some() && !locks.contains(&b) {
                out.push(b);
            }
        }
    }
    out
}

/// Cheapest water route from `from` to any of `to` (cells after `from`, in order), with `cost`
/// deciding where a barge may go. `None` if none.
fn route_with(sim: &SimState, from: Cell, to: &[Cell], cost: impl Fn(Cell) -> Option<f32>) -> Option<Vec<Cell>> {
    let n = sim.water.depth.width();
    if to.contains(&from) {
        return Some(Vec::new());
    }
    let idx = |c: Cell| c.1 * n + c.0;
    let mut dist = vec![u32::MAX; n * n];
    let mut prev = vec![u32::MAX; n * n];
    let mut heap = BinaryHeap::new();
    dist[idx(from)] = 0;
    heap.push(Reverse((0u32, idx(from))));
    while let Some(Reverse((d, i))) = heap.pop() {
        if d > dist[i] {
            continue;
        }
        let c = (i % n, i / n);
        if to.contains(&c) {
            let mut path = vec![c];
            let mut j = i;
            while prev[j] != u32::MAX && prev[j] as usize != idx(from) {
                j = prev[j] as usize;
                path.push((j % n, j / n));
            }
            path.reverse();
            return Some(path);
        }
        for m in neighbours(c, n) {
            let Some(k) = cost(m) else { continue };
            let nd = d + (k * 100.0) as u32;
            if nd < dist[idx(m)] {
                dist[idx(m)] = nd;
                prev[idx(m)] = i as u32;
                heap.push(Reverse((nd, idx(m))));
            }
        }
    }
    None
}

/// Water route for a barge (see `step_cost`).
pub fn route(sim: &SimState, locks: &HashSet<Cell>, from: Cell, to: &[Cell], ice: bool) -> Option<Vec<Cell>> {
    route_with(sim, from, to, |c| step_cost(sim, locks, c, ice))
}

/// Why two docks are not joined by water: the first dam a route would have to cross (build a
/// lock there), or `None` if there is no water between them at all.
pub fn blocking_dam(sim: &SimState, from: &Building, to: &Building) -> Option<Cell> {
    let locks = lock_cells(sim);
    let start = *berths(sim, &locks, from).first()?;
    let ends = berths(sim, &locks, to);
    let free = |c: Cell| {
        let w = &sim.water;
        let deep = w.depth.get(c.0, c.1).is_some_and(|&d| d >= BARGE_DRAFT);
        (deep || w.has_dam(c.0, c.1) || locks.contains(&c)).then_some(1.0)
    };
    let path = route_with(sim, start, &ends, free)?;
    path.into_iter().find(|c| sim.water.has_dam(c.0, c.1) && !locks.contains(c))
}

fn dock(sim: &SimState, id: BuildingId) -> Result<&Building, String> {
    sim.buildings
        .building(id)
        .filter(|b| b.kind == BuildingKind::Dock && b.is_complete())
        .ok_or_else(|| format!("building {} is not a finished dock", id.0))
}

/// Sets barge `id` to run between docks `from` and `to` carrying `goods`; checks that water joins
/// the barge, the docks and each other (ice aside), and says which dam needs a lock if one is in
/// the way. Errors leave the barge as it was.
pub fn set_route(sim: &mut SimState, id: BargeId, from: BuildingId, to: BuildingId, goods: Vec<Good>) -> Result<(), String> {
    if from == to {
        return Err("a route needs two different docks".into());
    }
    if goods.is_empty() {
        return Err("name at least one good to carry".into());
    }
    let (a, b) = (dock(sim, from)?.clone(), dock(sim, to)?.clone());
    let barge = sim.buildings.barge(id).ok_or_else(|| format!("no barge {}", id.0))?.clone();
    let locks = lock_cells(sim);
    let (at_a, at_b) = (berths(sim, &locks, &a), berths(sim, &locks, &b));
    if at_a.is_empty() || at_b.is_empty() {
        return Err("a dock has no water deep enough for a barge beside it".into());
    }
    let joined = |x: Cell, ends: &[Cell]| route(sim, &locks, x, ends, false).is_some();
    if !joined(at_a[0], &at_b) {
        return Err(match blocking_dam(sim, &a, &b) {
            Some(d) => format!("a dam at {},{} is in the way: build a lock on it", d.0, d.1),
            None => "no water joins the two docks".into(),
        });
    }
    if !joined(barge.cell, &at_a) {
        return Err("the barge cannot reach the first dock by water".into());
    }
    let bg = sim.buildings.barges.iter_mut().find(|x| x.id == id).expect("checked above");
    bg.service = Some(Service { from, to, goods });
    bg.leg = Leg::ToLoad;
    bg.wait = 0.0;
    bg.route.clear();
    Ok(())
}

/// Takes barge `id` off its route; it stays where it is, with any goods aboard.
pub fn clear_route(sim: &mut SimState, id: BargeId) -> Result<(), String> {
    let bg = sim.buildings.barges.iter_mut().find(|x| x.id == id).ok_or_else(|| format!("no barge {}", id.0))?;
    bg.service = None;
    bg.route.clear();
    bg.state = BargeState::Idle;
    Ok(())
}

fn aboard(hold: &[Amount]) -> f32 {
    hold.iter().map(|h| h.1).sum()
}

/// Advances every barge on a route.
pub fn step(sim: &mut SimState, dt: f32) {
    if sim.buildings.barges.iter().all(|b| b.service.is_none()) {
        return;
    }
    let locks = lock_cells(sim);
    for i in 0..sim.buildings.barges.len() {
        let mut bg = sim.buildings.barges[i].clone();
        if let Some(svc) = bg.service.clone() {
            run(sim, &locks, &mut bg, &svc, dt);
        }
        sim.buildings.barges[i] = bg;
    }
}

fn run(sim: &mut SimState, locks: &HashSet<Cell>, bg: &mut crate::buildings::Barge, svc: &Service, dt: f32) {
    let (Ok(from), Ok(to)) = (dock(sim, svc.from).cloned(), dock(sim, svc.to).cloned()) else {
        // A dock is gone: the route ends where the barge is.
        bg.service = None;
        bg.route.clear();
        bg.state = BargeState::Idle;
        return;
    };
    match bg.leg {
        Leg::ToLoad | Leg::ToUnload => {
            let target = if bg.leg == Leg::ToLoad { &from } else { &to };
            let ends = berths(sim, locks, target);
            if ends.contains(&bg.cell) {
                bg.route.clear();
                bg.state = BargeState::Idle;
                bg.at_dock = Some(target.id);
                bg.leg = if bg.leg == Leg::ToLoad { Leg::Loading } else { Leg::Unloading };
                bg.wait = 0.0;
                return;
            }
            sail(sim, locks, bg, &ends, dt);
        }
        Leg::Loading => {
            bg.wait += dt;
            if bg.wait < DWELL_DAYS {
                return;
            }
            for &g in &svc.goods {
                let space = BARGE_CAPACITY - aboard(&bg.hold);
                let take = space.min(sim.work.stock_of(from.id, g)).min(sim.buildings.stores.amount(g));
                if take > 1e-3 && sim.buildings.stores.take_raw(g, take).is_ok() {
                    sim.work.stock_take(from.id, g, take);
                    match bg.hold.iter_mut().find(|h| h.0 == g) {
                        Some(h) => h.1 += take,
                        None => bg.hold.push((g, take)),
                    }
                }
            }
            let load = aboard(&bg.hold);
            let more = svc.goods.iter().any(|&g| sim.work.stock_of(from.id, g) > 1e-3);
            if load >= BARGE_CAPACITY - 1e-3 || (load >= MIN_LOAD && !more) || (load > 0.0 && bg.wait >= MAX_WAIT_DAYS) {
                bg.leg = Leg::ToUnload;
                bg.at_dock = None;
                bg.route.clear();
            }
        }
        Leg::Unloading => {
            bg.wait += dt;
            if bg.wait < DWELL_DAYS {
                return;
            }
            for h in bg.hold.iter_mut() {
                let put = sim.buildings.stores.put_raw(h.0, h.1).unwrap_or(0.0);
                sim.work.stock_add(to.id, h.0, put);
                h.1 -= put;
                bg.carried += put;
            }
            bg.hold.retain(|h| h.1 > 1e-3);
            if bg.hold.is_empty() {
                bg.leg = Leg::ToLoad;
                bg.at_dock = None;
                bg.route.clear();
            }
        }
    }
}

/// Moves a barge along its route toward `ends`, planning (or re-planning) the route as needed.
fn sail(sim: &SimState, locks: &HashSet<Cell>, bg: &mut crate::buildings::Barge, ends: &[Cell], dt: f32) {
    if bg.state == BargeState::Stranded {
        bg.wait += dt;
        if bg.wait < RETRY_DAYS {
            return;
        }
    }
    if bg.route.is_empty() {
        match route(sim, locks, bg.cell, ends, true) {
            Some(mut r) => {
                r.reverse(); // next cell last
                bg.route = r;
                bg.state = BargeState::Sailing;
                bg.wait = 0.0;
            }
            None => {
                bg.state = BargeState::Stranded; // iced in, dried up or cut off: try again later
                bg.wait = 0.0;
                return;
            }
        }
    }
    bg.progress += BARGE_SPEED * dt;
    while let Some(&next) = bg.route.last() {
        let Some(cost) = step_cost(sim, locks, next, true) else {
            bg.route.clear(); // the way closed: plan again next step
            return;
        };
        if bg.progress < cost {
            break;
        }
        bg.progress -= cost;
        bg.cell = next;
        bg.route.pop();
    }
    if bg.route.is_empty() {
        bg.progress = 0.0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::buildings::{Barge, Status};

    /// A flat map with a straight channel along y = 10 from x = 2 to x = 60, a dam across it at
    /// x = 30, and a dock at each end.
    fn channel() -> (SimState, BuildingId, BuildingId, BargeId) {
        let mut s = SimState::new(1);
        let n = s.water.depth.width();
        s.water = crate::water::Water::new(n);
        for x in 2..=60 {
            *s.water.depth.get_mut(x, 10).unwrap() = 0.5;
        }
        s.water.build_dam(&[(30, 10)], 1.0).unwrap();
        let dock = |s: &mut SimState, id: u64, pos: Cell| {
            s.buildings.buildings.push(Building { id: BuildingId(id), kind: BuildingKind::Dock, pos, status: Status::Complete, recipe: None, output: 0.0 });
            BuildingId(id)
        };
        let (a, b) = (dock(&mut s, 900, (4, 9)), dock(&mut s, 901, (56, 9)));
        let id = BargeId(950);
        s.buildings.barges.push(Barge::new(id, (5, 10), Some(a)));
        (s, a, b, id)
    }

    #[test]
    fn a_dam_stops_a_barge_until_a_lock_is_built_and_then_it_carries_stone_both_ways_round() {
        let (mut s, a, b, id) = channel();
        let err = set_route(&mut s, id, a, b, vec![Good::Stone]).unwrap_err();
        assert!(err.contains("30,10") && err.contains("lock"), "{err}");
        s.buildings.buildings.push(Building { id: BuildingId(960), kind: BuildingKind::Lock, pos: (30, 10), status: Status::Complete, recipe: None, output: 0.0 });
        set_route(&mut s, id, a, b, vec![Good::Stone]).unwrap();
        s.buildings.stores.deposit(Good::Stone, 50.0).unwrap();
        s.work.stock_add(a, Good::Stone, 50.0);
        let mut days = 0.0;
        while s.work.stock_of(b, Good::Stone) < 50.0 - 1e-3 && days < 20.0 {
            step(&mut s, 0.05);
            days += 0.05;
            let held: f32 = s.buildings.barges[0].hold.iter().map(|h| h.1).sum();
            let total = s.work.stock_of(a, Good::Stone) + s.work.stock_of(b, Good::Stone) + held;
            assert!((total - 50.0).abs() < 1e-2, "stone vanished or appeared: {total}");
            assert!((s.buildings.stores.amount(Good::Stone) + held - 50.0).abs() < 1e-2, "aboard is off the ledger");
        }
        assert!(days < 12.0, "two round trips of 52 cells took {days} days");
        assert!(s.water.has_dam(30, 10), "the lock keeps the dam");
    }

    #[test]
    fn ice_holds_a_barge_until_it_melts() {
        let (mut s, a, b, id) = channel();
        s.buildings.buildings.push(Building { id: BuildingId(960), kind: BuildingKind::Lock, pos: (30, 10), status: Status::Complete, recipe: None, output: 0.0 });
        set_route(&mut s, id, a, b, vec![Good::Stone]).unwrap();
        s.buildings.stores.deposit(Good::Stone, 10.0).unwrap();
        s.work.stock_add(a, Good::Stone, 10.0);
        *s.water.ice.get_mut(20, 10).unwrap() = 1.0;
        for _ in 0..100 {
            step(&mut s, 0.05);
        }
        assert!(s.buildings.barges[0].cell.0 < 20 && s.work.stock_of(b, Good::Stone) == 0.0, "iced in");
        *s.water.ice.get_mut(20, 10).unwrap() = 0.0;
        for _ in 0..100 {
            step(&mut s, 0.05);
        }
        assert!(s.work.stock_of(b, Good::Stone) > 9.9, "through once the ice is gone");
    }
}
