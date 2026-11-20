use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashSet};

use crate::buildings::{Amount, BargeId, BargeState, Building, BuildingId, BuildingKind, Good};
use crate::water::Cell;
use crate::SimState;

pub const BARGE_SPEED: f32 = 43_000.0;
pub const BARGE_CAPACITY: f32 = 40.0;
pub const BARGE_DRAFT: f32 = 0.12;
pub const BERTH_REACH: usize = 4;
pub const CANAL_STEP: f32 = 0.6;
pub const LOCK_STEP: f32 = 10.0;
pub const ICE_STOPS: f32 = 0.4;
pub const DWELL_DAYS: f32 = 0.1;
pub const MIN_LOAD: f32 = 6.0;
pub const MAX_WAIT_DAYS: f32 = 2.0;
pub const RETRY_DAYS: f32 = 0.5;

#[cfg(test)]
mod ice_tests {
    use super::*;
    #[test]
    fn frozen_channel_waits_and_resumes_after_thaw() {
        let mut s=SimState::new(1);s.water.depth.as_mut_slice().fill(0.5);s.water.ice.as_mut_slice().fill(1.0);
        let mut b=crate::buildings::Barge::new(BargeId(1),(5,5),None);let locks=HashSet::new();
        sail(&s,&locks,&mut b,&[(8,5)],0.01);assert_eq!(b.state,BargeState::Icebound);assert_eq!(b.cell,(5,5));
        s.water.ice.as_mut_slice().fill(0.0);sail(&s,&locks,&mut b,&[(8,5)],RETRY_DAYS);
        assert_eq!(b.state,BargeState::Sailing);assert_eq!(b.cell,(8,5));
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Service {
    pub from: BuildingId,
    pub to: BuildingId,
    pub goods: Vec<Good>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Leg {
    #[default]
    ToLoad,
    Loading,
    ToUnload,
    Unloading,
}

pub fn lock_cells(sim: &SimState) -> HashSet<Cell> {
    sim.buildings.buildings.iter().filter(|b| b.kind == BuildingKind::Lock && b.is_complete()).flat_map(|b| b.cells()).collect()
}

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

pub fn berths(sim: &SimState, locks: &HashSet<Cell>, dock: &Building) -> Vec<Cell> {
    let n = sim.water.depth.width();
    let r = BERTH_REACH;
    let mut out: Vec<(usize, Cell)> = Vec::new();
    let (x0, y0) = (dock.pos.0.saturating_sub(r), dock.pos.1.saturating_sub(r));
    let (w, h) = dock.kind.spec().footprint;
    for y in y0..(dock.pos.1 + h + r).min(n) {
        for x in x0..(dock.pos.0 + w + r).min(n) {
            let d = crate::stock::dist((x, y), dock);
            if d <= r && step_cost(sim, locks, (x, y), false).is_some() && !locks.contains(&(x, y)) {
                out.push((d, (x, y)));
            }
        }
    }
    out.sort();
    out.into_iter().map(|o| o.1).collect()
}

pub fn berths_at(sim: &SimState, pos: Cell) -> Vec<Cell> {
    let probe = Building { id: BuildingId(0), kind: BuildingKind::Dock, pos, status: crate::buildings::Status::Complete, output: 0.0, condition: 1.0, reinforced: false, dam_profile: Vec::new() };
    berths(sim, &lock_cells(sim), &probe)
}

pub fn joined(sim: &SimState, a: &[Cell], b: &[Cell]) -> bool {
    let locks = lock_cells(sim);
    a.first().is_some_and(|&s| route(sim, &locks, s, b, false).is_some())
}

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

pub fn route(sim: &SimState, locks: &HashSet<Cell>, from: Cell, to: &[Cell], ice: bool) -> Option<Vec<Cell>> {
    route_with(sim, from, to, |c| step_cost(sim, locks, c, ice))
}

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

fn sail(sim: &SimState, locks: &HashSet<Cell>, bg: &mut crate::buildings::Barge, ends: &[Cell], dt: f32) {
    if matches!(bg.state,BargeState::Stranded|BargeState::Icebound) {
        bg.wait += dt;
        if bg.wait < RETRY_DAYS {
            return;
        }
    }
    if bg.route.is_empty() {
        match route(sim, locks, bg.cell, ends, true) {
            Some(mut r) => {
                r.reverse();
                bg.route = r;
                bg.state = BargeState::Sailing;
                bg.wait = 0.0;
            }
            None => {
                bg.state = if sim.water.ice.get(bg.cell.0,bg.cell.1).is_some_and(|&ice|ice>=ICE_STOPS)
                    || route(sim,locks,bg.cell,ends,false).is_some() {BargeState::Icebound} else {BargeState::Stranded};
                bg.wait = 0.0;
                return;
            }
        }
    }
    bg.progress += BARGE_SPEED * dt;
    while let Some(&next) = bg.route.last() {
        let Some(cost) = step_cost(sim, locks, next, true) else {
            bg.route.clear();
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
