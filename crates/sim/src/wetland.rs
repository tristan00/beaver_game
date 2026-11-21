include!(concat!(env!("OUT_DIR"), "/wetland_constants.rs"));
use std::collections::HashSet;

use crate::buildings::WET_DEPTH;
use crate::resources::{ResourceKind, PLANT_MAX, WET_MOISTURE};
use crate::water::Cell;
use crate::SimState;


pub fn is_shore(sim: &SimState, c: Cell) -> bool {
    let d = &sim.water.depth;
    let Some(&here) = d.get(c.0, c.1) else { return false };
    if here > SHALLOW || sim.water.has_dam(c.0, c.1) {
        return false;
    }
    here >= WET_DEPTH
        || [(c.0.wrapping_sub(1), c.1), (c.0 + 1, c.1), (c.0, c.1.wrapping_sub(1)), (c.0, c.1 + 1)]
            .iter()
            .any(|n| d.get(n.0, n.1).is_some_and(|&x| x >= WET_DEPTH))
}

fn draw(seed: u64, day: u64, i: usize) -> f32 {
    let mut z = seed ^ day.wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ (i as u64).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    ((z ^ (z >> 31)) >> 40) as f32 / (1u64 << 24) as f32
}

fn river_clay_active(water: &crate::water::Water, (x,y): Cell) -> bool {
    [(x,y),(x.wrapping_sub(1),y),(x+1,y),(x,y.wrapping_sub(1)),(x,y+1)].iter().any(|&(x,y)| {
        let depth=water.depth.get(x,y).copied().unwrap_or(0.0);
        let ice=water.ice.get(x,y).copied().unwrap_or(1.0);
        let velocity=water.velocity.get(x,y).copied().unwrap_or((0.0,0.0));
        depth>=WET_DEPTH && ice<CLAY_MAX_ICE && velocity.0.hypot(velocity.1)>CLAY_MIN_VELOCITY
    })
}

pub fn step(sim: &mut SimState, day: u64) {
    let n = sim.water.depth.width();
    let moist = |sim: &SimState, c: Cell| sim.world.moisture.get(c.0, c.1).is_some_and(|&m| m >= WET_MOISTURE);
    let drowned: Vec<usize> = sim
        .resources
        .items
        .iter()
        .enumerate()
        .filter(|(_, r)| r.kind == ResourceKind::Plant && sim.water.depth.get(r.cell.0, r.cell.1).is_some_and(|&d| d > DROWN))
        .map(|(i, _)| i)
        .collect();
    for i in drowned.into_iter().rev() {
        sim.resources.items.remove(i);
    }
    let wet: Vec<bool> = sim.resources.items.iter().map(|r| r.kind == ResourceKind::Plant && (is_shore(sim, r.cell) || moist(sim, r.cell))).collect();
    for (r, w) in sim.resources.items.iter_mut().zip(wet) {
        if r.kind == ResourceKind::Plant {
            r.wet = w;
        }
    }
    let flowing: Vec<_> = sim.resources.items.iter().filter(|r|r.kind==ResourceKind::RiverClay)
        .map(|r|(r.id,river_clay_active(&sim.water,r.cell))).collect();
    for (id,wet) in flowing {if let Ok(r)=sim.resources.get_mut(id) {r.wet=wet;}}
    let sprouting = SPROUT_PER_DAY * crate::calendar::vegetation_growth(day as f64 - 0.5);
    if sprouting == 0.0 { return; }
    let taken: HashSet<Cell> = sim.resources.items.iter().map(|r| r.cell).chain(sim.buildings.buildings.iter().flat_map(|b| b.cells())).collect();
    let mut plants = vec![0u8; n * n];
    for r in sim.resources.items.iter().filter(|r| r.kind == ResourceKind::Plant) {
        plants[r.cell.1 * n + r.cell.0] = 1;
    }
    let crowd = |plants: &[u8], c: Cell| {
        let mut k = 0;
        for y in c.1.saturating_sub(SPROUT_CROWD_RADIUS)..(c.1 + SPROUT_CROWD_RADIUS + 1).min(n) {
            for x in c.0.saturating_sub(SPROUT_CROWD_RADIUS)..(c.0 + SPROUT_CROWD_RADIUS + 1).min(n) {
                k += plants[y * n + x] as usize;
            }
        }
        k
    };
    let mut new = Vec::new();
    for y in 0..n {
        for x in 0..n {
            let c = (x, y);
            if taken.contains(&c) || draw(sim.seed, day, y * n + x) >= sprouting || !is_shore(sim, c) || crowd(&plants, c) >= CROWDED {
                continue;
            }
            plants[y * n + x] = 1;
            new.push(c);
        }
    }
    for c in new {
        let id = sim.resources.add(ResourceKind::Plant, c, PLANT_MAX);
        if let Ok(r) = sim.resources.get_mut(id) {
            r.amount = SPROUT_AMOUNT;
            r.wet = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn river_clay_needs_adjacent_unfrozen_flow() {
        let mut water=crate::water::Water::new(4);
        let bank=(0,1);
        *water.velocity.get_mut(1,1).unwrap()=(0.01,0.0);
        assert!(!river_clay_active(&water,bank));
        *water.depth.get_mut(1,1).unwrap()=0.1;
        assert!(river_clay_active(&water,bank));
        *water.ice.get_mut(1,1).unwrap()=0.5;
        assert!(!river_clay_active(&water,bank));
        *water.ice.get_mut(1,1).unwrap()=0.0;
        *water.velocity.get_mut(1,1).unwrap()=(0.0,0.0);
        assert!(!river_clay_active(&water,bank));
    }
}

