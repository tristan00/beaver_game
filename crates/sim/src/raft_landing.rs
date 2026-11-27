include!(concat!(env!("OUT_DIR"), "/raft_landing_constants.rs"));

use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap};

use crate::buildings::{Building, BuildingId};
use crate::colony::BeaverId;
use crate::nav::{Gait, Path, Surface};
use crate::water::Cell;
use crate::SimState;

#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Raft {
    pub id: u32,
    pub raft_landing: BuildingId,
    pub cell: Cell,
    pub hauler: Option<BeaverId>,
}

#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Fleet {
    pub rafts: Vec<Raft>,
    next: u32,
    #[serde(skip)]
    regions: Vec<u32>,
    #[serde(skip)]
    regions_key: Option<(u64, u64, u64, u64, usize, u64)>,
}

impl Fleet {
    pub fn get(&self, id: u32) -> Option<&Raft> {
        self.rafts.iter().find(|b| b.id == id)
    }
    pub fn get_mut(&mut self, id: u32) -> Option<&mut Raft> {
        self.rafts.iter_mut().find(|b| b.id == id)
    }
    pub fn of(&self, who: BeaverId) -> Option<&Raft> {
        self.rafts.iter().find(|b| b.hauler == Some(who))
    }
}

pub const RAFT_ICE_M: f32 = crate::tuning::water::ICE_WALK_M;

pub fn raft_water(sim: &SimState, c: Cell) -> bool {
    match sim.nav.surface_at(c) {
        Some(Surface::Lock) => true,
        Some(Surface::Water | Surface::Canal | Surface::LodgeEntrance) => sim.water.depth.get(c.0, c.1).is_some_and(|&d| d >= RAFT_DEPTH) && sim.water.ice.get(c.0, c.1).is_none_or(|&t| t < RAFT_ICE_M),
        _ => false,
    }
}

pub fn docks(sim: &SimState, b: &Building) -> Vec<Cell> {
    let cells: Vec<Cell> = b.cells().collect();
    let mut out: Vec<Cell> = cells
        .iter()
        .flat_map(|&(x, y)| [(x + 1, y), (x.wrapping_sub(1), y), (x, y + 1), (x, y.wrapping_sub(1))])
        .filter(|c| !cells.contains(c) && raft_water(sim, *c))
        .collect();
    out.sort();
    out.dedup();
    out
}

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
            if !raft_water(sim, n) {
                continue;
            }
            if dx != 0 && dy != 0 && !corner_ok(sim, c, n) {
                continue;
            }
            let Some(step) = sim.nav.move_cost(c, n, RAFT_PACE) else { continue };
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

pub fn joined(sim: &SimState, a: BuildingId, b: BuildingId) -> bool {
    let (Some(x), Some(y)) = (sim.buildings.building(a), sim.buildings.building(b)) else { return false };
    let w = sim.nav.surface.width();
    let region = |c: Cell| sim.fleet.regions.get(c.1 * w + c.0).copied().unwrap_or(0);
    let here: Vec<u32> = docks(sim, x).into_iter().map(region).filter(|&r| r != 0).collect();
    docks(sim, y).into_iter().map(region).any(|r| r != 0 && here.contains(&r))
}

fn corner_ok(sim: &SimState, a: Cell, b: Cell) -> bool {
    let lock = |c: Cell| sim.nav.surface_at(c) == Some(Surface::Lock);
    lock(a) || lock(b) || (raft_water(sim, (b.0, a.1)) && raft_water(sim, (a.0, b.1)))
}

fn label_regions(sim: &mut SimState) {
    let (w, h) = (sim.nav.surface.width(), sim.nav.surface.height());
    let mut regions = vec![0u32; w * h];
    let wet: Vec<bool> = (0..w * h).map(|i| raft_water(sim, (i % w, i / w))).collect();
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

pub fn step(sim: &mut SimState) {
    let key = sim.nav.refresh_key();
    let any = sim.buildings.buildings.iter().any(|b| b.kind.docks_rafts());
    if any && (sim.fleet.regions.is_empty() || sim.fleet.regions_key != key) {
        label_regions(sim);
        sim.fleet.regions_key = key;
    }
    let raft_landings: Vec<(BuildingId, Vec<Cell>)> = sim
        .buildings
        .buildings
        .iter()
        .filter(|b| b.kind.docks_rafts() && sim.finished(b))
        .map(|b| (b.id, docks(sim, b)))
        .collect();
    sim.fleet.rafts.retain(|b| raft_landings.iter().any(|h| h.0 == b.raft_landing));
    for (id, docks) in &raft_landings {
        let have = sim.fleet.rafts.iter().filter(|b| b.raft_landing == *id).count();
        for k in have..RAFTS_PER_RAFT_LANDING {
            let Some(&cell) = docks.get(k % docks.len().max(1)) else { break };
            let n = sim.fleet.next;
            sim.fleet.next += 1;
            sim.fleet.rafts.push(Raft { id: n, raft_landing: *id, cell, hauler: None });
        }
    }
    let using: Vec<(u32, BeaverId)> = sim.fleet.rafts.iter().filter_map(|b| b.hauler.map(|h| (b.id, h))).collect();
    for (raft, who) in using {
        let still = sim.agents.get(who).is_some_and(|a| matches!(a.task, crate::agents::Task::Raft { raft: x, .. } if x == raft));
        if !still {
            if let Some(b) = sim.fleet.get_mut(raft) {
                b.hauler = None;
            }
        }
    }
}

pub fn free_raft(sim: &SimState, at: Cell) -> Option<u32> {
    sim.fleet
        .rafts
        .iter()
        .filter(|b| b.hauler.is_none())
        .min_by_key(|b| (b.cell.0.abs_diff(at.0) + b.cell.1.abs_diff(at.1), b.id))
        .map(|b| b.id)
}
