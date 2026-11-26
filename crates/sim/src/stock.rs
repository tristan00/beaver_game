include!(concat!(env!("OUT_DIR"), "/stock_constants.rs"));
use crate::agents::{accepts, is_store, load_size, Task, SUPPLIES};
use crate::buildings::{Building, BuildingId, Good};
use crate::water::Cell;
use crate::SimState;

pub fn dist(c: Cell, b: &Building) -> usize {
    b.cells().map(|q| c.0.abs_diff(q.0) + c.1.abs_diff(q.1)).min().unwrap_or(usize::MAX)
}

pub fn holders(sim: &SimState, good: Good) -> Vec<BuildingId> {
    let mut out: Vec<BuildingId> = sim.buildings.buildings.iter().filter(|b| b.is_complete() && accepts(b.kind, good)).map(|b| b.id).collect();
    if sim.work.supplies.is_some() {
        out.push(SUPPLIES);
    }
    out
}

/// A haul mound has its haulers balancing the stores (from when it is finished, until
/// switched off).
pub fn balancing(sim: &SimState) -> bool {
    sim.buildings.buildings.iter().any(|b| b.kind.balances() && b.is_complete() && sim.work.balanced.contains(&b.id))
}

fn open_store(sim: &SimState, b: &Building) -> bool {
    b.is_complete() && is_store(b.kind) && !sim.buildings.unusable(b, &sim.water.depth)
}

/// Store-to-store loads currently in flight (foot hauls and raft trips): `(from, to, good, amount)`.
fn in_flight(sim: &SimState) -> Vec<(BuildingId, BuildingId, Good, f32)> {
    use crate::agents::{Dest, Source};
    let mut out = Vec::new();
    for a in sim.agents.by_id.values() {
        match (&a.task, a.load) {
            (Task::Move { from: Some(Source::Store(from)), to: Dest::Store(to), good, haul: true }, None) => {
                out.push((*from, *to, *good, load_size(*good)));
            }
            (Task::Move { to: Dest::Store(to), haul: true, .. }, Some(l)) => out.push((BuildingId(0), *to, l.good, l.amount)),
            (Task::Raft { from, to, good, leg, .. }, None) if *leg <= 2 => {
                out.push((*from, *to, *good, (crate::raft_landing::RAFT_LOADS * load_size(*good)).min(sim.buildings.stores.stock_of(*from, *good))));
            }
            (Task::Raft { to, leg, .. }, Some(l)) if *leg <= 4 => out.push((BuildingId(0), *to, l.good, l.amount)),
            _ => {}
        }
    }
    out
}

/// The next load for a store the player asked to have filled: its good from the store that
/// has the most of it, until the store is full or no other store has any.
pub fn fill_move(sim: &SimState) -> Option<(BuildingId, BuildingId, Good)> {
    if sim.work.fills.is_empty() {
        return None;
    }
    let moving = in_flight(sim);
    for &to in &sim.work.fills {
        let Some(target) = sim.buildings.building(to).filter(|b| open_store(sim, b) && !crate::predators::building_in_view(sim, b.id)) else { continue };
        for g in Good::all().filter(|&g| accepts(target.kind, g)) {
            let incoming: f32 = moving.iter().filter(|m| m.1 == to && m.2 == g).map(|m| m.3).sum();
            let room = target.kind.spec().capacity - sim.buildings.stores.stock_of(to, g) - incoming;
            if room < 1.0 {
                continue;
            }
            let from = sim.buildings.buildings.iter()
                .filter(|b| b.id != to && b.is_complete() && accepts(b.kind, g) && !sim.work.fills.contains(&b.id) && !crate::predators::building_in_view(sim, b.id))
                .map(|b| (sim.buildings.stores.stock_of(b.id, g), b.id))
                .filter(|(s, _)| *s >= 1.0)
                .max_by(|a, b| a.0.total_cmp(&b.0).then(b.1.cmp(&a.1)));
            if let Some((_, from)) = from {
                return Some((from, to, g));
            }
        }
    }
    None
}

/// A fill order is done once its store is full or gone.
pub fn retire_fills(sim: &mut SimState) {
    let done: Vec<BuildingId> = sim.work.fills.iter().copied().filter(|&id| {
        sim.buildings.building(id).is_none_or(|b| {
            Good::all().filter(|&g| accepts(b.kind, g)).all(|g| b.kind.spec().capacity - sim.buildings.stores.stock_of(id, g) < 1.0)
        })
    }).collect();
    for id in done {
        sim.work.fills.remove(&id);
        crate::trace::event(format!("FILL_DONE {} store={}", crate::trace::sim_stamp(), id.0));
    }
}

/// The next load that evens out the stores, when a haul mound's haulers balance.
pub fn balance_move(sim: &SimState, at: Option<BuildingId>) -> Option<(BuildingId, BuildingId, Good)> {
    if !balancing(sim) {
        return None;
    }
    let moving = in_flight(sim);
    let mut best: Option<(bool, f32, BuildingId, BuildingId, Good)> = None;
    for g in Good::all() {
        let stores: Vec<&Building> = sim.buildings.buildings.iter()
            .filter(|b| open_store(sim, b) && accepts(b.kind, g) && !crate::predators::building_in_view(sim, b.id))
            .collect();
        if stores.len() < 2 {
            continue;
        }
        let cap = |b: &Building| b.kind.spec().capacity.max(1.0);
        let total: f32 = stores.iter().map(|b| sim.buildings.stores.stock_of(b.id, g)).sum::<f32>()
            + moving.iter().filter(|m| m.2 == g).map(|m| m.3).sum::<f32>();
        let room: f32 = stores.iter().map(|b| cap(b)).sum();
        let off = |b: &Building| {
            let incoming: f32 = moving.iter().filter(|m| m.1 == b.id && m.2 == g).map(|m| m.3).sum();
            let leaving: f32 = moving.iter().filter(|m| m.0 == b.id && m.2 == g).map(|m| m.3).sum();
            sim.buildings.stores.stock_of(b.id, g) - leaving + incoming - total * cap(b) / room
        };
        let over = stores.iter().map(|b| (off(b), b.id)).max_by(|a, b| a.0.total_cmp(&b.0).then(b.1.cmp(&a.1)))?;
        let under = stores.iter().map(|b| (off(b), b.id)).min_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)))?;
        let gap = over.0.min(-under.0);
        if gap < BALANCE_SLACK || sim.buildings.stores.stock_of(over.1, g) < 1.0 {
            continue;
        }
        let mine = at.is_some_and(|s| s == over.1 || s == under.1);
        if best.as_ref().is_none_or(|b| (mine, gap) > (b.0, b.1)) {
            best = Some((mine, gap, over.1, under.1, g));
        }
    }
    best.map(|b| (b.2, b.3, b.4))
}
