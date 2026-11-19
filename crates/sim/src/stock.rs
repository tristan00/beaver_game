use crate::agents::{accepts, SUPPLIES};
use crate::buildings::{Building, BuildingId, Good};
use crate::water::Cell;
use crate::SimState;

pub const WORKSHOP_REACH: usize = 40;
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

pub fn within_reach(sim: &SimState, workshop: &Building, good: Good) -> f32 {
    sim.buildings
        .buildings
        .iter()
        .filter(|b| b.is_complete() && accepts(b.kind, good) && gap(workshop, b) <= WORKSHOP_REACH)
        .map(|b| sim.work.stock_of(b.id, good))
        .sum()
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
        left -= sim.work.stock_take(id, good, left);
    }
    amount - left
}

pub fn put_near(sim: &mut SimState, near: &Building, good: Good, amount: f32) -> bool {
    let best = sim.buildings.buildings.iter().filter(|b| b.is_complete() && accepts(b.kind, good)).min_by_key(|b| (gap(near, b), b.id)).map(|b| b.id);
    match best {
        Some(id) => {
            sim.work.stock_add(id, good, amount);
            true
        }
        None => false,
    }
}

pub const BALANCE_SLACK: f32 = 3.0;

pub fn balance_move(sim: &SimState, at: Option<BuildingId>) -> Option<(BuildingId, BuildingId, Good)> {
    if sim.work.balanced.len() < 2 {
        return None;
    }
    let mut moving: Vec<(BuildingId, BuildingId, Good, f32)> = Vec::new();
    for a in sim.agents.by_id.values() {
        match (&a.task, a.load) {
            (crate::agents::Task::Fetch { from, good, then }, None) if sim.work.balanced.contains(then) => moving.push((*from, *then, *good, crate::agents::LOAD_SIZE)),
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
        let cap = |b: &Building| crate::catalogue::def(b.kind).capacity.max(1.0);
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

pub fn book_crafting(sim: &mut SimState) {
    let crafted = sim.buildings.crafted.clone();
    for (id, out, made) in crafted {
        let Some(b) = sim.buildings.building(id).cloned() else { continue };
        if let Some(r) = b.recipe {
            let (inputs, (_, per)) = r.batch();
            let batches = made / per;
            for &(g, a) in inputs {
                take_near(sim, &b, g, a * batches);
            }
        }
        put_near(sim, &b, out, made);
    }
}

pub fn reconcile(sim: &mut SimState) {
    for g in Good::ALL {
        let holders = holders(sim, g);
        sim.work.stock.retain(|&(b, good), a| good != g || (holders.contains(&b) && *a > EPS));
        let total: f32 = holders.iter().map(|&b| sim.work.stock_of(b, g)).sum();
        let ledger = sim.buildings.stores.amount(g);
        let diff = ledger - total;
        if diff > EPS {
            if let Some(&first) = holders.iter().min() {
                sim.work.stock_add(first, g, diff);
            }
        } else if diff < -EPS && total > 0.0 {
            let keep = (ledger / total).max(0.0);
            for &b in &holders {
                if let Some(a) = sim.work.stock.get_mut(&(b, g)) {
                    *a *= keep;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::buildings::{BuildingKind, Status};

    fn store(sim: &mut SimState, id: u64, pos: Cell) -> BuildingId {
        let b = BuildingId(id);
        sim.buildings.buildings.push(Building { id: b, kind: BuildingKind::Warehouse, pos, status: Status::Complete, recipe: None, output: 0.0 });
        b
    }

    #[test]
    fn balanced_stores_even_out_by_size_and_others_are_left_alone() {
        let mut sim = SimState::new(3);
        let (a, b, c) = (store(&mut sim, 1, (10, 10)), store(&mut sim, 2, (40, 10)), store(&mut sim, 3, (70, 10)));
        sim.buildings.stores.deposit(Good::Stone, 60.0).unwrap();
        sim.work.stock_add(a, Good::Stone, 60.0);
        assert_eq!(balance_move(&sim, None), None, "nothing is balanced yet");
        sim.work.balanced.extend([a, b]);
        assert_eq!(balance_move(&sim, Some(b)), Some((a, b, Good::Stone)), "from the full store to the empty one");
        sim.work.stock_take(a, Good::Stone, 30.0);
        sim.work.stock_add(b, Good::Stone, 30.0);
        assert_eq!(balance_move(&sim, Some(b)), None, "two equal stores hold equal shares");
        sim.work.stock_add(c, Good::Stone, 0.0);
        assert_eq!(balance_move(&sim, Some(a)), None, "a store not set to balance takes no part");
    }

    #[test]
    fn stocks_follow_the_ledger_and_shortfalls_come_from_every_store() {
        let mut sim = SimState::new(3);
        let (a, b) = (store(&mut sim, 1, (10, 10)), store(&mut sim, 2, (40, 10)));
        sim.buildings.stores.deposit(Good::Stone, 30.0).unwrap();
        reconcile(&mut sim);
        assert!((sim.work.stock_of(a, Good::Stone) - 30.0).abs() < 1e-3, "unplaced goods land in the oldest store");
        sim.work.stock_take(a, Good::Stone, 10.0);
        sim.work.stock_add(b, Good::Stone, 10.0);
        sim.buildings.stores.withdraw(Good::Stone, 15.0).unwrap();
        reconcile(&mut sim);
        let (sa, sb) = (sim.work.stock_of(a, Good::Stone), sim.work.stock_of(b, Good::Stone));
        assert!((sa + sb - 15.0).abs() < 1e-3 && (sa - 2.0 * sb).abs() < 1e-3, "{sa} + {sb}");
        assert_eq!(nearest_holding(&sim, (45, 10), Good::Stone, 1.0), Some(b));
        assert_eq!(nearest_holding(&sim, (45, 10), Good::Stone, 9.0), Some(a), "only the far store holds enough");
    }
}
