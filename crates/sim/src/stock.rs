include!(concat!(env!("OUT_DIR"), "/stock_constants.rs"));
use crate::agents::{accepts, SUPPLIES};
use crate::buildings::{Building, BuildingId, Good};
use crate::water::Cell;
use crate::SimState;

const EPS: f32 = 1e-4;

pub fn gap(a: &Building, b: &Building) -> usize {
    a.cells().flat_map(|p| b.cells().map(move |q| p.0.abs_diff(q.0) + p.1.abs_diff(q.1))).min().unwrap_or(usize::MAX)
}

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

fn cell_dist(sim: &SimState, from: Cell, id: BuildingId) -> usize {
    if id == SUPPLIES {
        return sim.work.supplies.map_or(usize::MAX, |p| from.0.abs_diff(p.0) + from.1.abs_diff(p.1));
    }
    sim.buildings.building(id).map_or(usize::MAX, |b| dist(from, b))
}

pub fn nearest_holding(sim: &SimState, from: Cell, good: Good, min: f32) -> Option<BuildingId> {
    holders(sim, good)
        .into_iter()
        .filter(|&id| sim.work.stock_of(id, good) >= min.max(EPS))
        .min_by_key(|&id| (cell_dist(sim, from, id), id))
}


pub fn take_near(sim: &mut SimState, near: &Building, good: Good, amount: f32) -> f32 {
    let mut ids: Vec<(usize, BuildingId)> =
        sim.buildings.buildings.iter().filter(|b| b.is_complete() && accepts(b.kind, good)).map(|b| (gap(near, b), b.id)).collect();
    ids.sort();
    let mut left = amount;
    for (_, id) in ids {
        if left <= EPS {
            break;
        }
        left -= sim.buildings.stores.consume_from(id, good, left, crate::buildings::Use::Crafting);
    }
    amount - left
}


pub fn balance_move(sim: &SimState, at: Option<BuildingId>) -> Option<(BuildingId, BuildingId, Good)> {
    if sim.work.balanced.len() < 2 {
        return None;
    }
    let mut moving: Vec<(BuildingId, BuildingId, Good, f32)> = Vec::new();
    for a in sim.agents.by_id.values() {
        match (&a.task, a.load) {
            (crate::agents::Task::Fetch { from, good, then, site_delivery: false }, None) if sim.work.balanced.contains(then) => moving.push((*from, *then, *good, crate::agents::LOAD_SIZE)),
            (crate::agents::Task::Haul { to }, Some(l)) => moving.push((BuildingId(0), *to, l.good, l.amount)),
            _ => {}
        }
    }
    let mut best: Option<(bool, f32, BuildingId, BuildingId, Good)> = None;
    for g in Good::ALL {
        let stores: Vec<&Building> = sim
            .buildings
            .buildings
            .iter()
            .filter(|b| sim.work.balanced.contains(&b.id) && b.is_complete() && accepts(b.kind, g))
            .collect();
        if stores.len() < 2 {
            continue;
        }
        let cap = |b: &Building| b.kind.spec().capacity.max(1.0);
        let total: f32 = stores.iter().map(|b| sim.work.stock_of(b.id, g)).sum::<f32>()
            + moving.iter().filter(|m| m.2 == g).map(|m| m.3).sum::<f32>();
        let room: f32 = stores.iter().map(|b| cap(b)).sum();
        let off = |b: &Building| {
            let incoming: f32 = moving.iter().filter(|m| m.1 == b.id && m.2 == g).map(|m| m.3).sum();
            let leaving: f32 = moving.iter().filter(|m| m.0 == b.id && m.2 == g).map(|m| m.3).sum();
            sim.work.stock_of(b.id, g) - leaving + incoming - total * cap(b) / room
        };
        let over = stores.iter().map(|b| (off(b), b.id)).max_by(|a, b| a.0.total_cmp(&b.0).then(b.1.cmp(&a.1)))?;
        let under = stores.iter().map(|b| (off(b), b.id)).min_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)))?;
        let gap = over.0.min(-under.0);
        if gap < BALANCE_SLACK || sim.work.stock_of(over.1, g) < 1.0 {
            continue;
        }
        let mine = at.is_some_and(|s| s == over.1 || s == under.1);
        if best.as_ref().is_none_or(|b| (mine, gap) > (b.0, b.1)) {
            best = Some((mine, gap, over.1, under.1, g));
        }
    }
    best.map(|b| (b.2, b.3, b.4))
}
