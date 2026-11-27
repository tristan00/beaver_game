include!(concat!(env!("OUT_DIR"), "/wetland_constants.rs"));
use std::collections::HashSet;

use crate::buildings::WET_DEPTH;
use crate::plants::{Habitat, Plant};
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

fn slow(sim: &SimState, c: Cell, k: f32) -> bool {
    let (vx, vy) = sim.water.velocity.get(c.0, c.1).copied().unwrap_or((0.0, 0.0));
    vx.hypot(vy) < MUD_MIN_VELOCITY * 10.0 * k
}

fn in_still_water(sim: &SimState, c: Cell) -> bool {
    let depth = sim.water.depth.get(c.0, c.1).copied().unwrap_or(0.0);
    depth >= WET_DEPTH && depth <= WATER_PLANT_DEEP && !sim.water.has_dam(c.0, c.1) && slow(sim, c, 1.0)
}

pub fn suits(sim: &SimState, p: Plant, c: Cell) -> bool {
    let spec = p.spec();
    let depth = sim.water.depth.get(c.0, c.1).copied().unwrap_or(0.0);
    let moisture = sim.world.moisture.get(c.0, c.1).copied().unwrap_or(0.0);
    if sim.water.has_dam(c.0, c.1) {
        return false;
    }
    match spec.habitat {
        Habitat::Water => depth >= spec.min_depth.max(WET_DEPTH) && depth <= spec.max_depth && slow(sim, c, p.row().flow),
        Habitat::Shore => is_shore(sim, c) && depth <= spec.max_depth,
        Habitat::Meadow => depth < WET_DEPTH && (moisture >= WET_MOISTURE || is_shore(sim, c)),
        Habitat::Upland => depth < WET_DEPTH && (UPLAND_MOISTURE..WET_MOISTURE).contains(&moisture) && !is_shore(sim, c),
    }
}

fn ground_class(sim: &SimState, c: Cell, kind: ResourceKind) -> Habitat {
    if kind.aquatic() {
        Habitat::Water
    } else if crate::resources::is_bank(&sim.world, c) {
        Habitat::Shore
    } else if crate::resources::is_wet(&sim.world, c) {
        Habitat::Meadow
    } else {
        Habitat::Upland
    }
}

fn flora(sim: &SimState) -> &str {
    if sim.resources.flora.is_empty() { "default" } else { &sim.resources.flora }
}

fn species_for(sim: &SimState, c: Cell, kind: ResourceKind, roll: f32) -> Option<Plant> {
    let map = flora(sim);
    let class = ground_class(sim, c, kind);
    crate::plants::choose(map, roll, |p| p.kind() == kind && p.spec().habitat == class && suits(sim, p, c))
        .or_else(|| crate::plants::choose(map, roll, |p| p.kind() == kind && suits(sim, p, c)))
        .or_else(|| crate::plants::choose(map, roll, |p| p.kind() == kind && p.spec().habitat == class))
        .or_else(|| crate::plants::choose(map, roll, |p| p.kind() == kind))
}

pub fn assign_plants(sim: &mut SimState) {
    let todo: Vec<(usize, Plant)> = sim
        .resources
        .items()
        .iter()
        .enumerate()
        .filter(|(_, r)| r.kind.is_food_plant() && r.plant.is_none())
        .filter_map(|(i, r)| species_for(sim, r.cell, r.kind, draw(sim.seed, 7, r.cell.1 * 4099 + r.cell.0)).map(|p| (i, p)))
        .collect();
    if todo.is_empty() {
        return;
    }
    let ids: Vec<_> = todo.iter().map(|&(i, p)| (sim.resources.items()[i].id, p)).collect();
    for (id, p) in ids {
        if let Ok(r) = sim.resources.get_mut(id) {
            let full = (r.amount / r.max_amount.max(1e-3)).clamp(0.0, 1.0);
            r.plant = Some(p);
            r.max_amount = p.spec().food;
            r.amount = full * r.max_amount;
        }
    }
}

pub fn set_flora(sim: &mut SimState, map: &str) {
    sim.resources.flora = map.to_string();
    let ids: Vec<_> = sim.resources.items().iter().filter(|r| r.kind.is_food_plant()).map(|r| r.id).collect();
    for id in ids {
        if let Ok(r) = sim.resources.get_mut(id) {
            r.plant = None;
        }
    }
    assign_plants(sim);
}

fn add_plant(sim: &mut SimState, c: Cell, p: Plant, share: f32, seedling: bool) -> crate::resources::ResourceId {
    let id = sim.resources.add(p.kind(), c, p.spec().food);
    if let Ok(r) = sim.resources.get_mut(id) {
        r.plant = Some(p);
        r.amount = p.spec().food * share;
        r.wet = true;
        r.seedling = seedling;
    }
    id
}

pub(crate) fn plant_sprout(sim: &mut SimState, c: Cell, p: Plant) -> crate::resources::ResourceId {
    add_plant(sim, c, p, sprout_share(p), true)
}

pub fn spreads(r: &crate::resources::Resource) -> bool {
    r.plant.is_some() && !r.is_seedling() && r.wet && r.amount >= 0.5 * r.max_amount
}

pub(crate) struct Crowding {
    n: usize,
    land: Vec<u8>,
    water: Vec<u8>,
}

impl Crowding {
    pub(crate) fn of(sim: &SimState) -> Self {
        let n = sim.water.depth.width();
        let (mut land, mut water) = (vec![0u8; n * n], vec![0u8; n * n]);
        for r in sim.resources.items().iter().filter(|r| r.kind.is_food_plant() && r.cell.0 < n && r.cell.1 < n) {
            if r.kind.aquatic() { water[r.cell.1 * n + r.cell.0] = 1 } else { land[r.cell.1 * n + r.cell.0] = 1 }
        }
        Self { n, land, water }
    }
    pub(crate) fn crowded(&self, p: Plant, c: Cell) -> bool {
        let (grid, limit, r) = if p.aquatic() { (&self.water, WATER_CROWDED, WATER_CROWD_RADIUS) } else { (&self.land, CROWDED, SPROUT_CROWD_RADIUS) };
        let mut k = 0;
        for y in c.1.saturating_sub(r)..(c.1 + r + 1).min(self.n) {
            for x in c.0.saturating_sub(r)..(c.0 + r + 1).min(self.n) {
                k += grid[y * self.n + x] as usize;
            }
        }
        k >= limit
    }
}

fn sprout_share(p: Plant) -> f32 {
    if p.aquatic() { WATER_PLANT_SPROUT_AMOUNT / WATER_PLANT_MAX } else { SPROUT_AMOUNT / PLANT_MAX }
}

fn taken_cells(sim: &SimState) -> HashSet<Cell> {
    sim.resources.taken_cells()
        .chain(sim.buildings.buildings.iter().flat_map(|b| b.cells()))
        .chain(sim.work.supplies)
        .collect()
}

pub fn seed_water_plants(sim: &mut SimState) {
    if sim.resources.water_plants_seeded || sim.time_days < WATER_PLANT_SEED_DAY as f64 {
        return;
    }
    sim.resources.water_plants_seeded = true;
    let n = sim.water.depth.width();
    let mut taken = taken_cells(sim);
    let mut placed: Vec<(Cell, Plant)> = Vec::new();
    for y in 1..n.saturating_sub(1) {
        for x in 1..n.saturating_sub(1) {
            let c = (x, y);
            let by_land = [(x - 1, y), (x + 1, y), (x, y - 1), (x, y + 1)].iter().any(|n| sim.water.depth.get(n.0, n.1).is_some_and(|&d| d < WET_DEPTH));
            if taken.contains(&c) || !by_land || draw(sim.seed, 0, y * n + x) >= crate::resources::WATER_PLANT_CHANCE || !in_still_water(sim, c) {
                continue;
            }
            if placed.iter().rev().take(8 * n).any(|(o, _)| o.0.abs_diff(x).max(o.1.abs_diff(y)) < crate::resources::WATER_PLANT_SPACING) {
                continue;
            }
            let Some(p) = crate::plants::choose(flora(sim), draw(sim.seed, 1, y * n + x), |p| p.aquatic() && suits(sim, p, c)) else { continue };
            placed.push((c, p));
            taken.insert(c);
        }
    }
    let mut beds = Vec::new();
    for &(c, p) in &placed {
        beds.push((c, p));
        for (k, o) in [(c.0 + 1, c.1), (c.0, c.1 + 1), (c.0.wrapping_sub(1), c.1)].into_iter().enumerate() {
            if draw(sim.seed, 2, (c.1 * n + c.0) * 3 + k) < 0.4 && !taken.contains(&o) && suits(sim, p, o) {
                beds.push((o, p));
                taken.insert(o);
            }
        }
    }
    for (c, p) in beds {
        add_plant(sim, c, p, 1.0, false);
    }
}

fn draw(seed: u64, day: u64, i: usize) -> f32 {
    let mut z = seed ^ day.wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ (i as u64).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    ((z ^ (z >> 31)) >> 40) as f32 / (1u64 << 24) as f32
}

fn river_mud_active(water: &crate::water::Water, (x,y): Cell) -> bool {
    [(x,y),(x.wrapping_sub(1),y),(x+1,y),(x,y.wrapping_sub(1)),(x,y+1)].iter().any(|&(x,y)| {
        let depth=water.depth.get(x,y).copied().unwrap_or(0.0);
        let ice=if water.ice.in_bounds(x,y) {water.ice_cover(x,y)} else {1.0};
        let velocity=water.velocity.get(x,y).copied().unwrap_or((0.0,0.0));
        depth>=WET_DEPTH && ice<MUD_MAX_ICE && velocity.0.hypot(velocity.1)>MUD_MIN_VELOCITY
    })
}

pub fn step(sim: &mut SimState, day: u64) {
    assign_plants(sim);
    let n = sim.water.depth.width();
    let drowned: Vec<usize> = sim
        .resources
        .items()
        .iter()
        .enumerate()
        .filter(|(_, r)| {
            let depth = sim.water.depth.get(r.cell.0, r.cell.1).copied().unwrap_or(0.0);
            match r.kind {
                k if k.is_food_plant() && !k.aquatic() => depth > DROWN.max(r.plant.map_or(0.0, |p| p.spec().max_depth + 0.3)),
                k if k.is_food_plant() => depth > WATER_PLANT_DEEP.max(r.plant.map_or(0.0, |p| p.spec().max_depth + 0.5)),
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
    let wet: Vec<bool> = sim.resources.items().iter().map(|r| match (r.kind, r.plant) {
        (_, Some(p)) => suits(sim, p, r.cell),
        (k, None) if k.is_food_plant() && !k.aquatic() => is_shore(sim, r.cell) || sim.world.moisture.get(r.cell.0, r.cell.1).is_some_and(|&m| m >= WET_MOISTURE),
        (k, None) if k.is_food_plant() => in_still_water(sim, r.cell),
        _ => false,
    }).collect();
    let mut wet = wet.into_iter();
    sim.resources.each_mut(|r| {
        let w = wet.next().unwrap_or(false);
        if r.kind.is_food_plant() {
            r.wet = w;
        }
    });
    let flowing: Vec<_> = sim.resources.items().iter().filter(|r|r.kind.flowing())
        .map(|r|(r.id,river_mud_active(&sim.water,r.cell))).collect();
    for (id,wet) in flowing {if let Ok(r)=sim.resources.get_mut(id) {r.wet=wet;}}
    let growth = crate::calendar::vegetation_growth(day as f64 - 0.5);
    if growth == 0.0 { return; }
    let mut taken = taken_cells(sim);
    let mut crowding = Crowding::of(sim);
    let mut new: Vec<(Cell, Plant)> = Vec::new();
    let parents: Vec<(Cell, Plant, u64)> = sim.resources.items().iter()
        .filter(|r| spreads(r))
        .filter_map(|r| r.plant.map(|p| (r.cell, p, r.id.0)))
        .collect();
    for (c, p, id) in parents {
        let spec = p.spec();
        let i = (id as usize).wrapping_mul(5);
        let mut targets: Vec<Cell> = Vec::new();
        if draw(sim.seed, day, i) < spec.creep * growth {
            let k = (draw(sim.seed, day, i + 1) * 8.0) as usize % 8;
            let (dx, dy) = [(-1, -1), (0, -1), (1, -1), (-1, 0), (1, 0), (-1, 1), (0, 1), (1, 1)][k];
            targets.push(((c.0 as i64 + dx) as usize, (c.1 as i64 + dy) as usize));
        }
        if draw(sim.seed, day, i + 2) < spec.seed * growth {
            let reach = SEED_REACH as f32;
            let dx = ((draw(sim.seed, day, i + 3) * 2.0 - 1.0) * reach).round() as i64;
            let dy = ((draw(sim.seed, day, i + 4) * 2.0 - 1.0) * reach).round() as i64;
            targets.push(((c.0 as i64 + dx) as usize, (c.1 as i64 + dy) as usize));
        }
        for t in targets {
            if t.0 >= n || t.1 >= n || taken.contains(&t) || !suits(sim, p, t) || crowding.crowded(p, t) {
                continue;
            }
            if p.aquatic() { crowding.water[t.1 * n + t.0] = 1; } else { crowding.land[t.1 * n + t.0] = 1; }
            taken.insert(t);
            new.push((t, p));
        }
    }
    let rooting = WATER_PLANT_SPROUT_PER_DAY * growth;
    let reach = 2 * crate::resources::WATER_PLANT_SPACING;
    let alone = |lilies: &[u8], c: Cell| {
        (c.1.saturating_sub(reach)..(c.1 + reach + 1).min(n)).all(|y| (c.0.saturating_sub(reach)..(c.0 + reach + 1).min(n)).all(|x| lilies[y * n + x] == 0))
    };
    for y in 0..n {
        for x in 0..n {
            let c = (x, y);
            if draw(sim.seed, day, y * n + x + (1 << 40)) >= rooting || taken.contains(&c) || !in_still_water(sim, c) || !alone(&crowding.water, c) {
                continue;
            }
            if let Some(p) = crate::plants::choose(flora(sim), draw(sim.seed, day, y * n + x + (1 << 41)), |p| p.aquatic() && suits(sim, p, c)) {
                crowding.water[y * n + x] = 1;
                taken.insert(c);
                new.push((c, p));
            }
        }
    }
    for (c, p) in new {
        add_plant(sim, c, p, sprout_share(p), true);
    }
}
