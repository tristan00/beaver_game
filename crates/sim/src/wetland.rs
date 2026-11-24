include!(concat!(env!("OUT_DIR"), "/wetland_constants.rs"));
use std::collections::HashSet;

use crate::buildings::WET_DEPTH;
use crate::resources::{ResourceKind, PLANT_MAX, WATER_PLANT_MAX, WET_MOISTURE};
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

/// Water a plant can root in: deep enough to stay wet, not so deep it drowns, barely flowing.
fn in_still_water(sim: &SimState, c: Cell) -> bool {
    let depth = sim.water.depth.get(c.0, c.1).copied().unwrap_or(0.0);
    let (vx, vy) = sim.water.velocity.get(c.0, c.1).copied().unwrap_or((0.0, 0.0));
    depth >= WET_DEPTH && depth <= WATER_PLANT_DEEP && !sim.water.has_dam(c.0, c.1) && vx.hypot(vy) < CLAY_MIN_VELOCITY * 10.0
}

/// The first water plants: a few patches in still shallows along the shore, a few cells apart.
pub fn seed_water_plants(sim: &mut SimState) {
    if sim.resources.water_plants_seeded || sim.time_days < WATER_PLANT_SEED_DAY as f64 {
        return;
    }
    sim.resources.water_plants_seeded = true;
    let n = sim.water.depth.width();
    let taken: HashSet<Cell> = sim.resources.items().iter().map(|r| r.cell).chain(sim.buildings.buildings.iter().flat_map(|b| b.cells())).collect();
    let mut placed: Vec<Cell> = Vec::new();
    for y in 1..n.saturating_sub(1) {
        for x in 1..n.saturating_sub(1) {
            let c = (x, y);
            let by_land = [(x - 1, y), (x + 1, y), (x, y - 1), (x, y + 1)].iter().any(|n| sim.water.depth.get(n.0, n.1).is_some_and(|&d| d < WET_DEPTH));
            if taken.contains(&c) || !by_land || draw(sim.seed, 0, y * n + x) >= crate::resources::WATER_PLANT_CHANCE || !in_still_water(sim, c) {
                continue;
            }
            if placed.iter().rev().take(8 * n).any(|o| o.0.abs_diff(x).max(o.1.abs_diff(y)) < crate::resources::WATER_PLANT_SPACING) {
                continue;
            }
            placed.push(c);
        }
    }
    for c in placed {
        let id = sim.resources.add(ResourceKind::WaterPlant, c, WATER_PLANT_MAX);
        if let Ok(r) = sim.resources.get_mut(id) {
            r.wet = true;
        }
    }
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
        let ice=if water.ice.in_bounds(x,y) {water.ice_cover(x,y)} else {1.0};
        let velocity=water.velocity.get(x,y).copied().unwrap_or((0.0,0.0));
        depth>=WET_DEPTH && ice<CLAY_MAX_ICE && velocity.0.hypot(velocity.1)>CLAY_MIN_VELOCITY
    })
}

pub fn step(sim: &mut SimState, day: u64) {
    let n = sim.water.depth.width();
    let moist = |sim: &SimState, c: Cell| sim.world.moisture.get(c.0, c.1).is_some_and(|&m| m >= WET_MOISTURE);
    let drowned: Vec<usize> = sim
        .resources
        .items()
        .iter()
        .enumerate()
        .filter(|(_, r)| {
            let depth = sim.water.depth.get(r.cell.0, r.cell.1).copied().unwrap_or(0.0);
            match r.kind {
                ResourceKind::Plant => depth > DROWN,
                ResourceKind::WaterPlant => depth > WATER_PLANT_DEEP,
                _ => false,
            }
        })
        .map(|(i, _)| i)
        .collect();
    for i in drowned.into_iter().rev() {
        let depth = sim.water.depth.get(sim.resources.items()[i].cell.0, sim.resources.items()[i].cell.1).copied().unwrap_or(0.0);
        crate::resources::note("DROWNED", &sim.resources.items()[i], format_args!("depth={depth:.2}"));
        sim.resources.remove_at(i);
    }
    // Shoots grow on wet shores; water plants only while they stand in water.
    let wet: Vec<bool> = sim.resources.items().iter().map(|r| match r.kind {
        ResourceKind::Plant => is_shore(sim, r.cell) || moist(sim, r.cell),
        ResourceKind::WaterPlant => in_still_water(sim, r.cell),
        _ => false,
    }).collect();
    let mut wet = wet.into_iter();
    sim.resources.each_mut(|r| {
        let w = wet.next().unwrap_or(false);
        if r.kind.is_food_plant() {
            r.wet = w;
        }
    });
    let flowing: Vec<_> = sim.resources.items().iter().filter(|r|r.kind==ResourceKind::RiverClay)
        .map(|r|(r.id,river_clay_active(&sim.water,r.cell))).collect();
    for (id,wet) in flowing {if let Ok(r)=sim.resources.get_mut(id) {r.wet=wet;}}
    let growth = crate::calendar::vegetation_growth(day as f64 - 0.5);
    let sprouting = SPROUT_PER_DAY * growth;
    if sprouting == 0.0 { return; }
    let rooting = WATER_PLANT_SPROUT_PER_DAY * growth;
    let taken: HashSet<Cell> = sim.resources.items().iter().map(|r| r.cell).chain(sim.buildings.buildings.iter().flat_map(|b| b.cells())).collect();
    let mut plants = vec![0u8; n * n];
    let mut lilies = vec![0u8; n * n];
    for r in sim.resources.items().iter() {
        match r.kind {
            ResourceKind::Plant => plants[r.cell.1 * n + r.cell.0] = 1,
            ResourceKind::WaterPlant => lilies[r.cell.1 * n + r.cell.0] = 1,
            _ => {}
        }
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
    // A water plant roots only where no other grows within twice the starting spacing.
    let reach = 2 * crate::resources::WATER_PLANT_SPACING;
    let alone = |lilies: &[u8], c: Cell| {
        (c.1.saturating_sub(reach)..(c.1 + reach + 1).min(n)).all(|y| (c.0.saturating_sub(reach)..(c.0 + reach + 1).min(n)).all(|x| lilies[y * n + x] == 0))
    };
    let mut new = Vec::new();
    for y in 0..n {
        for x in 0..n {
            let c = (x, y);
            if taken.contains(&c) {
                continue;
            }
            let roll = draw(sim.seed, day, y * n + x);
            // Shoots spread from living plants: none come up where there is none within reach.
            if roll < sprouting && is_shore(sim, c) && (1..CROWDED).contains(&crowd(&plants, c)) {
                plants[y * n + x] = 1;
                new.push((c, ResourceKind::Plant));
            } else if roll < rooting && in_still_water(sim, c) && alone(&lilies, c) {
                // Water plants take root, one to a patch, in still shallows such as a new pond.
                lilies[y * n + x] = 1;
                new.push((c, ResourceKind::WaterPlant));
            }
        }
    }
    for (c, kind) in new {
        let (max, start) = if kind == ResourceKind::WaterPlant { (WATER_PLANT_MAX, WATER_PLANT_SPROUT_AMOUNT) } else { (PLANT_MAX, SPROUT_AMOUNT) };
        let id = sim.resources.add(kind, c, max);
        if let Ok(r) = sim.resources.get_mut(id) {
            r.amount = start;
            r.wet = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn water_plants_are_rich_and_grow_only_in_still_shallows() {
        assert!(WATER_PLANT_MAX > PLANT_MAX);
        let mut w = crate::resources::ResourceWorld::default();
        let id = w.add(ResourceKind::WaterPlant, (2, 2), WATER_PLANT_MAX);
        assert_eq!(w.take(id, 2.0), Some((crate::buildings::Good::Food, 2.0)));
        w.get_mut(id).unwrap().wet = true;
        w.step(10.0, 150.0).unwrap();
        assert!(w.get(id).unwrap().amount > WATER_PLANT_MAX - 2.0, "regrows while it stands in water");
        let mut sim = SimState::new(1);
        let c = (5, 5);
        *sim.water.depth.get_mut(c.0, c.1).unwrap() = 1.0;
        *sim.water.velocity.get_mut(c.0, c.1).unwrap() = (0.0, 0.0);
        assert!(in_still_water(&sim, c));
        *sim.water.depth.get_mut(c.0, c.1).unwrap() = WATER_PLANT_DEEP + 0.5;
        assert!(!in_still_water(&sim, c), "too deep");
        *sim.water.depth.get_mut(c.0, c.1).unwrap() = 1.0;
        *sim.water.velocity.get_mut(c.0, c.1).unwrap() = (0.5, 0.0);
        assert!(!in_still_water(&sim, c), "flowing");
    }
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
