include!(concat!(env!("OUT_DIR"), "/dam_preview_constants.rs"));
use crate::buildings::{Amount, BuildingId, BuildingKind};
use crate::water::Cell;
use std::collections::HashSet;

pub fn loads_for(sim: &crate::SimState, cells: &[Cell], heights: &[f32]) -> usize {
    let spans = crate::dams::spans(cells);
    cells.iter().zip(heights).zip(spans).map(|((&c, &h), w)| {
        let have = sim.water.dam_height.get(c.0, c.1).copied().unwrap_or(0.0);
        ((h - have).max(0.0) / crate::dams::STEP_HEIGHT * w - 1e-3).ceil().max(0.0) as usize
    }).sum()
}

pub fn cost_for(sim: &crate::SimState, cells: &[Cell], heights: &[f32]) -> (Vec<Amount>, f32) {
    let loads = loads_for(sim, cells, heights) as f32;
    let cost = BuildingKind::dam().spec().cost.iter().map(|&(g, a)| (g, a * loads)).filter(|c| c.1 > 1e-3).collect();
    (cost, BuildingKind::dam().spec().work * loads)
}

pub fn heights_to(sim: &crate::SimState, cells: &[Cell], crest: f32) -> (Vec<Cell>, Vec<f32>) {
    cells
        .iter()
        .filter_map(|&c| {
            let ground = *sim.height.get(c.0, c.1)?;
            let h = crest - ground;
            (h > 0.05).then_some((c, h))
        })
        .unzip()
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct DamPreview {
    pub level: f32,
    pub rise: f32,
    pub flooded: Vec<Cell>,
    pub flooded_trees: usize,
    pub flooded_buildings: Vec<BuildingId>,
    pub cost: Vec<Amount>,
    pub crest: f32,
    pub spill_at: Option<Cell>,
    pub pressure: f32,
    pub fill_days: f32,
    pub inflow: f32,
    pub pond_loss: f32,
}

impl DamPreview {
    pub fn fills(&self, margin: f32) -> bool {
        self.inflow >= margin * self.pond_loss
    }
}

pub fn preview(sim: &crate::SimState, cells: &[Cell], height: f32) -> Option<DamPreview> {
    preview_heights(sim, cells, &vec![height; cells.len()])
}

pub(crate) struct Ground {
    level: f32,
    crest: f32,
    spill_at: Option<Cell>,
    pub(crate) flooded: Vec<Cell>,
    in_pond: Vec<bool>,
    watershed: Vec<bool>,
}

fn wall(cells: &[Cell], heights: &[f32]) -> Option<(Vec<Cell>, Vec<f32>)> {
    let unique: HashSet<&Cell> = cells.iter().collect();
    if unique.len() != cells.len() || heights.len() != cells.len() {
        return None;
    }
    let (cells, heights): (Vec<Cell>, Vec<f32>) = cells.iter().copied().zip(heights.iter().copied()).filter(|p| p.1 > 0.0).unzip();
    (!cells.is_empty()).then_some((cells, heights))
}

fn ground(sim: &crate::SimState, cells: &[Cell], heights: &[f32]) -> Option<Ground> {
    let still = crate::water::StillPond::behind(sim, cells, heights).ok()?;
    let (w, h) = (sim.height.width(), sim.height.height());
    let idx = |&(x, y): &Cell| y * w + x;
    let flooded: Vec<Cell> = still.flooded_cells();
    let mut in_pond = vec![false; w * h];
    flooded.iter().for_each(|c| in_pond[idx(c)] = true);
    let terrain = sim.height.as_slice();
    let neighbours = |i: usize| {
        let (x, y) = (i % w, i / w);
        [(x > 0).then(|| i - 1), (x + 1 < w).then(|| i + 1), (y > 0).then(|| i - w), (y + 1 < h).then(|| i + w)]
            .into_iter().flatten()
    };
    let drains_to = |i: usize| neighbours(i).filter(|&j| terrain[j] < terrain[i])
        .min_by(|&a, &b| terrain[a].total_cmp(&terrain[b]));
    let mut seen = vec![false; w * h];
    let mut stack: Vec<usize> = cells.iter().map(idx).collect();
    stack.extend(flooded.iter().map(idx));
    for &i in &stack { seen[i] = true; }
    while let Some(i) = stack.pop() {
        for n in neighbours(i) {
            if !seen[n] && drains_to(n).is_some_and(|d| seen[d]) {
                seen[n] = true;
                stack.push(n);
            }
        }
    }
    Some(Ground { level: still.level, crest: still.crest, spill_at: still.spill_at, flooded, in_pond, watershed: seen })
}

pub fn preview_heights(sim: &crate::SimState, cells: &[Cell], heights: &[f32]) -> Option<DamPreview> {
    let (cells, heights) = wall(cells, heights)?;
    estimate(sim, &cells, &heights, &ground(sim, &cells, &heights)?)
}

pub(crate) type GroundKey = (u64, (u64, u64), (u64, u64), Vec<Cell>, Vec<u32>);
const GROUNDS_KEPT: usize = 12;

thread_local! {
    static GROUNDS: std::cell::RefCell<Vec<(GroundKey, std::rc::Rc<Ground>)>> = const { std::cell::RefCell::new(Vec::new()) };
}

pub fn preview_heights_kept(sim: &crate::SimState, cells: &[Cell], heights: &[f32]) -> Option<DamPreview> {
    let (cells, heights) = wall(cells, heights)?;
    let (_, ground) = kept_ground(sim, &cells, &heights)?;
    estimate(sim, &cells, &heights, &ground)
}

fn kept_ground(sim: &crate::SimState, cells: &[Cell], heights: &[f32]) -> Option<(GroundKey, std::rc::Rc<Ground>)> {
    let key: GroundKey = (sim.instance, sim.height.version(), sim.water.dam_height.version(), cells.to_vec(), heights.iter().map(|h| h.to_bits()).collect());
    let kept = GROUNDS.with(|g| {
        let mut g = g.borrow_mut();
        let at = g.iter().position(|(k, _)| *k == key)?;
        let entry = g.remove(at);
        let ground = entry.1.clone();
        g.push(entry);
        Some(ground)
    });
    let ground = match kept {
        Some(ground) => ground,
        None => {
            let ground = std::rc::Rc::new(ground(sim, cells, heights)?);
            GROUNDS.with(|g| {
                let mut g = g.borrow_mut();
                if g.len() >= GROUNDS_KEPT {
                    g.remove(0);
                }
                g.push((key.clone(), ground.clone()));
            });
            ground
        }
    };
    Some((key, ground))
}

pub(crate) fn flooded_kept(sim: &crate::SimState, cells: &[Cell], heights: &[f32]) -> Option<(GroundKey, std::rc::Rc<Ground>)> {
    let (cells, heights) = wall(cells, heights)?;
    kept_ground(sim, &cells, &heights)
}

fn estimate(sim: &crate::SimState, cells: &[Cell], heights: &[f32], ground: &Ground) -> Option<DamPreview> {
    let (w, h) = (sim.height.width(), sim.height.height());
    let idx = |&(x, y): &Cell| y * w + x;
    let seen = &ground.watershed;
    let in_pond = ground.in_pond.clone();
    let under = |c: &Cell| c.0 < w && c.1 < h && in_pond[idx(c)];

    let flooded_trees = sim.resources.items().iter()
        .filter(|r| r.kind.is_tree() && under(&r.cell)).count();
    let flooded_buildings = sim.buildings.buildings.iter()
        .filter(|b| b.cells().any(|c| under(&c))).map(|b| b.id).collect();

    let terrain = sim.height.as_slice();
    let (water, dams) = (sim.water.depth.as_slice(), sim.water.dam_height.as_slice());
    let by_cell: std::collections::HashMap<Cell, f32> = cells.iter().copied().zip(heights.iter().copied()).collect();
    let crest = |c: &Cell| terrain[idx(c)] + by_cell[c];
    let low = cells.iter().min_by(|a, b| crest(a).total_cmp(&crest(b)))?;
    let rise = (ground.level - (terrain[idx(low)] + dams[idx(low)] + water[idx(low)])).max(0.0);

    let forcing = sim.water.forcing();
    let pressure: f32 = sim.water.sources.iter().filter(|s| seen[s.y * w + s.x])
        .map(|s| s.rate * (1.0 + s.seasonality * (forcing.inflow - 1.0)).max(0.0)).sum();

    let cfg = &sim.water.config;
    let (fed, ice) = (sim.world.lake.as_slice(), sim.water.ice.as_slice());
    let loss_at = |i: usize| if fed[i] { 0.0 } else { cfg.evaporation * forcing.evaporation * (1.0 - (ice[i] / crate::tuning::water::ICE_COVER_M).min(1.0)) + cfg.seepage };
    let in_pond: Vec<bool> = {
        let mut v = in_pond;
        cells.iter().for_each(|c| v[idx(c)] = true);
        v
    };
    let lost_on_way: f32 = (0..w * h).filter(|&i| seen[i] && !in_pond[i] && water[i] > crate::water::WET_EPS).map(loss_at).sum();
    let inflow = (pressure - lost_on_way).max(0.0);
    let pond_loss: f32 = ground.flooded.iter().map(idx).map(loss_at).sum();

    let missing: f32 = ground.flooded.iter().map(idx)
        .map(|i| (ground.level - terrain[i] - dams[i] - water[i]).max(0.0)).sum();
    let net = inflow - 0.5 * pond_loss;
    let fill_days = if missing <= 0.0 { 0.0 } else if inflow > pond_loss && net > 0.0 { missing / net } else { f32::INFINITY };

    Some(DamPreview {
        level: ground.level,
        rise,
        flooded: ground.flooded.clone(),
        flooded_trees,
        flooded_buildings,
        cost: cost_for(sim, cells, heights).0,
        crest: ground.crest,
        spill_at: ground.spill_at,
        pressure,
        fill_days,
        inflow,
        pond_loss,
    })
}

pub fn drowned_by(sim: &crate::SimState, then: impl Fn(usize, usize) -> f32) -> Vec<BuildingId> {
    let now = |x: usize, y: usize| sim.water.depth.get(x, y).copied().unwrap_or(0.0);
    let max = crate::buildings::MAX_BUILDING_DEPTH;
    sim.buildings.buildings.iter()
        .filter(|b| !b.kind.is_waterworks())
        .filter(|b| crate::buildings::footprint_depth(b.kind, b.pos, now) <= max && crate::buildings::footprint_depth(b.kind, b.pos, |x, y| then(x, y).max(now(x, y))) > max)
        .map(|b| b.id)
        .collect()
}

#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Settled {
    pub rise: f32,
    pub flooded: Vec<Cell>,
    pub level: f32,
    pub depths: Vec<f32>,
}

pub fn settle(water: crate::water::Water, height: &crate::grid::HeightGrid, cells: &[Cell], heights: &[f32]) -> Option<Settled> {
    let pairs: Vec<(Cell, f32)> = cells.iter().copied().zip(heights.iter().copied())
        .filter(|&(c, h)| h > water.dam_height.get(c.0, c.1).copied().unwrap_or(0.0))
        .collect();
    if pairs.is_empty() {
        return None;
    }
    let mut without = water.clone();
    let mut with = water;
    with.raise_dam(&pairs).ok()?;
    with.refresh(height);
    let mut t = 0.0;
    let mut last: Option<f32> = None;
    loop {
        let until = t + SETTLE_CHUNK_DAYS;
        std::thread::scope(|sc| {
            sc.spawn(|| run(&mut without, height, t, until));
            run(&mut with, height, t, until);
        });
        t = until;
        let pond = pond(&with, &without, height, cells);
        let still = last.is_some_and(|l| (pond.rise - l).abs() < SETTLE_STILL_M);
        if still || t >= SETTLE_MAX_DAYS - 1e-6 {
            return Some(pond);
        }
        last = Some(pond.rise);
    }
}

fn run(water: &mut crate::water::Water, height: &crate::grid::HeightGrid, from: f32, until: f32) {
    let mut t = from;
    while t < until - 1e-6 {
        let dt = crate::STEP_DAYS.min(until - t);
        water.step(height, dt);
        t += dt;
    }
}

fn pond(with: &crate::water::Water, without: &crate::water::Water, height: &crate::grid::HeightGrid, cells: &[Cell]) -> Settled {
    let n = height.width();
    let (ground, on, off) = (height.as_slice(), with.depth.as_slice(), without.depth.as_slice());
    let mut seen = vec![false; n * n];
    cells.iter().for_each(|c| seen[c.1 * n + c.0] = true);
    let mut stack: Vec<usize> = cells.iter().flat_map(|c| crate::water::neighbours8(n, c.1 * n + c.0)).collect();
    let mut out = Settled { rise: 0.0, flooded: Vec::new(), level: f32::NEG_INFINITY, depths: Vec::new() };
    while let Some(i) = stack.pop() {
        let change = on[i] - off[i];
        if seen[i] || change.abs() <= POND_RISE_M {
            continue;
        }
        seen[i] = true;
        stack.extend(crate::water::neighbours8(n, i));
        if change > 0.0 && on[i] > crate::water::WET_EPS {
            out.rise = out.rise.max(change);
            out.level = out.level.max(ground[i] + on[i]);
            out.flooded.push((i % n, i / n));
            out.depths.push(on[i]);
        }
    }
    out
}

const SETTLE_CHUNK_DAYS: f32 = 0.25;
const SETTLE_STILL_M: f32 = 0.01;
const SETTLE_MAX_DAYS: f32 = 3.0;
const POND_RISE_M: f32 = 0.02;
