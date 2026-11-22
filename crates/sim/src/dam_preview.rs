include!(concat!(env!("OUT_DIR"), "/dam_preview_constants.rs"));
use crate::buildings::{Amount, BuildingId, BuildingKind};
use crate::resources::ResourceKind;
use crate::water::{preview_dam_heights, Cell};
use std::collections::HashSet;



/// Loads needed to bring a drawn line to these heights: one load per step per metre of wall
/// (a diagonal cell stands for about 1.4 m).
pub fn loads_for(sim: &crate::SimState, cells: &[Cell], heights: &[f32]) -> usize {
    let spans = crate::dams::spans(cells);
    cells.iter().zip(heights).zip(spans).map(|((&c, &h), w)| {
        let have = sim.water.dam_height.get(c.0, c.1).copied().unwrap_or(0.0);
        ((h - have).max(0.0) / crate::dams::STEP_HEIGHT * w - 1e-3).ceil().max(0.0) as usize
    }).sum()
}

/// Materials and packing labour to build these heights.
pub fn cost_for(sim: &crate::SimState, cells: &[Cell], heights: &[f32]) -> (Vec<Amount>, f32) {
    let loads = loads_for(sim, cells, heights) as f32;
    let cost = BuildingKind::Dam.spec().cost.iter().map(|&(g, a)| (g, a * loads)).filter(|c| c.1 > 1e-3).collect();
    (cost, BuildingKind::Dam.spec().work * loads)
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

#[derive(Clone, Debug, PartialEq)]
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
    /// Where the water settles once the river runs past the dam: the still level plus the
    /// depth it takes to push the inflow over the spill (weir law), and never below today's
    /// surface beside the dam.
    pub flow_level: f32,
    /// How much higher that is than the water beside the dam today.
    pub flow_rise: f32,
    /// The cells that level covers.
    pub flow_flooded: Vec<Cell>,
}

impl DamPreview {
    pub fn fills(&self, margin: f32) -> bool {
        self.inflow >= margin * self.pond_loss
    }
}


pub fn preview(sim: &crate::SimState, cells: &[Cell], height: f32) -> Option<DamPreview> {
    preview_heights(sim, cells, &vec![height; cells.len()])
}

pub fn preview_heights(sim: &crate::SimState, cells: &[Cell], heights: &[f32]) -> Option<DamPreview> {
    let unique: HashSet<&Cell> = cells.iter().collect();
    if unique.len() != cells.len() || heights.len() != cells.len() {
        return None;
    }
    // Cells whose ground already stands at the crest need no wall: leave them out rather than
    // refuse the whole line (a line drawn up onto a bank always has some).
    let (cells, heights): (Vec<Cell>, Vec<f32>) = cells.iter().copied().zip(heights.iter().copied()).filter(|p| p.1 > 0.0).unzip();
    if cells.is_empty() {
        return None;
    }
    let (cells, heights) = (&cells[..], &heights[..]);
    let pond = preview_dam_heights(sim, cells, heights, 0.0).ok()?;
    let (w, h) = (sim.height.width(), sim.height.height());
    let idx = |&(x, y): &Cell| y * w + x;
    let flooded_set: HashSet<Cell> = pond.flooded_cells.iter().copied().collect();

    let flooded_trees = sim.resources.items.iter()
        .filter(|r| r.kind == ResourceKind::Tree && flooded_set.contains(&r.cell)).count();
    let flooded_buildings = sim.buildings.buildings.iter()
        .filter(|b| b.cells().any(|c| flooded_set.contains(&c))).map(|b| b.id).collect();

    let terrain = sim.height.as_slice();
    let (water, dams) = (sim.water.depth.as_slice(), sim.water.dam_height.as_slice());
    let by_cell: std::collections::HashMap<Cell, f32> = cells.iter().copied().zip(heights.iter().copied()).collect();
    let crest = |c: &Cell| terrain[idx(c)] + by_cell[c];
    let low = cells.iter().min_by(|a, b| crest(a).total_cmp(&crest(b)))?;
    let rise = (pond.level - (terrain[idx(low)] + dams[idx(low)] + water[idx(low)])).max(0.0);

    let neighbours = |i: usize| {
        let (x, y) = (i % w, i / w);
        [(x > 0).then(|| i - 1), (x + 1 < w).then(|| i + 1), (y > 0).then(|| i - w), (y + 1 < h).then(|| i + w)]
            .into_iter().flatten()
    };
    let drains_to = |i: usize| neighbours(i).filter(|&j| terrain[j] < terrain[i])
        .min_by(|&a, &b| terrain[a].total_cmp(&terrain[b]));
    let mut seen = vec![false; w * h];
    let mut stack: Vec<usize> = cells.iter().map(idx).collect();
    stack.extend(pond.flooded_cells.iter().map(idx));
    for &i in &stack { seen[i] = true; }
    while let Some(i) = stack.pop() {
        for n in neighbours(i) {
            if !seen[n] && drains_to(n).is_some_and(|d| seen[d]) {
                seen[n] = true;
                stack.push(n);
            }
        }
    }
    let season = sim.water.season();
    let forcing = sim.water.forcing();
    let pressure: f32 = sim.water.sources.iter().filter(|s| seen[s.y * w + s.x])
        .map(|s| s.rate * (1.0 + s.seasonality * (forcing.inflow - 1.0)).max(0.0)).sum();

    let cfg = &sim.water.config;
    let (fed, ice) = (sim.water.fed.as_slice(), sim.water.ice.as_slice());
    let loss_at = |i: usize| if fed[i] { 0.0 } else { cfg.evaporation * crate::tuning::water::EVAP_MULT[season as usize] * (1.0 - ice[i]) + cfg.seepage };
    let in_pond: Vec<bool> = {
        let mut v = vec![false; w * h];
        pond.flooded_cells.iter().chain(cells).for_each(|c| v[idx(c)] = true);
        v
    };
    let lost_on_way: f32 = (0..w * h).filter(|&i| seen[i] && !in_pond[i] && water[i] > crate::water::WET_EPS).map(loss_at).sum();
    let inflow = (pressure - lost_on_way).max(0.0);
    let pond_loss: f32 = pond.flooded_cells.iter().map(idx).map(loss_at).sum();

    // Flowing water stands above the spill by the weir head for the inflow it passes. Over the
    // crest it spreads along the crest cells at the low point; around the dam, over the
    // notch the water finds (as wide as the water engine lets a spill be).
    // The discharge reaching the dam: what the catchment walk finds, or the river the world
    // traced through these cells (its walk stops at small dips in real terrain), by season.
    let river_q = cells.iter()
        .flat_map(|&(x, y)| (-1i64..=1).flat_map(move |dx| (-1i64..=1).map(move |dy| (x as i64 + dx, y as i64 + dy))))
        .filter(|&(x, y)| x >= 0 && y >= 0)
        .filter_map(|(x, y)| sim.world.river_flow.get(x as usize, y as usize).copied())
        .fold(0.0f32, f32::max) * forcing.inflow.max(0.0);
    let q = (inflow * sim.water.cell_m * sim.water.cell_m / 86_400.0).max(river_q);
    let low_crest = cells.iter().map(crest).fold(f32::INFINITY, f32::min);
    let weir_cells = match pond.spill_at {
        Some(_) => crate::water::equilibrium::WEIR_MAX_M,
        None => cells.iter().filter(|c| crest(c) < low_crest + 0.02).count().clamp(1, crate::water::equilibrium::WEIR_MAX_M),
    };
    let head = crate::water::spill_head(q, weir_cells as f32 * sim.water.cell_m);
    let flowing = preview_dam_heights(sim, cells, heights, head).ok()?;
    // The rise is how much deeper the water gets where the dam backs it up most: over every
    // cell the flowing level covers, its level less today's surface there (a dry cell's is
    // its ground).
    let surface = |c: &Cell| terrain[idx(c)] + water[idx(c)];
    let flow_level = flowing.flow_level;
    let flow_rise = flowing.flow_cells.iter().map(|c| flow_level - surface(c)).fold(0.0f32, f32::max);

    let missing: f32 = pond.flooded_cells.iter().map(idx)
        .map(|i| (pond.level - terrain[i] - dams[i] - water[i]).max(0.0)).sum();
    let net = inflow - 0.5 * pond_loss;
    let fill_days = if missing <= 0.0 { 0.0 } else if inflow > pond_loss && net > 0.0 { missing / net } else { f32::INFINITY };

    Some(DamPreview {
        level: pond.level,
        rise,
        flooded: pond.flooded_cells,
        flooded_trees,
        flooded_buildings,
        cost: cost_for(sim, cells, heights).0,
        crest: pond.crest,
        spill_at: pond.spill_at,
        pressure,
        fill_days,
        inflow,
        pond_loss,
        flow_level,
        flow_rise,
        flow_flooded: flowing.flow_cells,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The fixture valley runs along y = 64 with its river fed from x = 0.
    fn running_river() -> crate::SimState {
        let mut s = crate::sandbox::on_map("fixture").unwrap();
        crate::sandbox::run_days(&mut s, 2.0);
        s
    }

    #[test]
    fn a_dam_the_river_runs_around_still_raises_the_water_beside_it() {
        let s = running_river();
        // Across the river where it runs at x = 64, but too short to reach the valley sides.
        let y0 = (0..128).max_by(|&a, &b| s.world.river_flow.get(64, a).unwrap().total_cmp(s.world.river_flow.get(64, b).unwrap())).unwrap();
        let short: Vec<Cell> = (y0 - 3..=y0 + 3).map(|y| (64, y)).collect();
        let p = preview_heights(&s, &short, &vec![1.0; short.len()]).unwrap();
        assert!(p.spill_at.is_some(), "the river finds a way round a short dam");
        assert!(p.flow_level > p.level, "flowing water stands above the still level");
        assert!(p.flow_rise > 0.05, "the badge shows the water rising: {}", p.flow_rise);
    }

    #[test]
    fn water_pouring_over_a_full_dam_stands_above_its_crest() {
        let s = running_river();
        let across: Vec<Cell> = (30..99).map(|y| (64, y)).collect();
        let p = preview_heights(&s, &across, &vec![0.5; across.len()]).unwrap();
        assert!(p.flow_level > p.crest, "{} over a crest at {}", p.flow_level, p.crest);
        assert!(p.flow_flooded.len() >= p.flooded.len());
    }
}

/// What the water really does once a dam stands: a copy of the water runs on with the dam in
/// place, and every cell near it is compared with today. The game's own engine, so it matches
/// play exactly where the quick estimate (`preview_heights`) guesses wrong, for instance a line
/// drawn along a channel rather than across it. It takes about a tenth of a second, so the
/// game runs it in the background.
#[derive(Clone, Debug, PartialEq)]
pub struct Settled {
    /// The most the water deepens anywhere near the dam.
    pub rise: f32,
    /// Cells it deepens by a noticeable amount, and the highest surface among them.
    pub flooded: Vec<Cell>,
    pub level: f32,
}

pub fn settle(mut water: crate::water::Water, height: &crate::grid::HeightGrid, cells: &[Cell], heights: &[f32]) -> Option<Settled> {
    let pairs: Vec<(Cell, f32)> = cells.iter().copied().zip(heights.iter().copied())
        .filter(|&(c, h)| h > water.dam_height.get(c.0, c.1).copied().unwrap_or(0.0))
        .collect();
    if pairs.is_empty() {
        return None;
    }
    let (w, h) = (height.width(), height.height());
    let (x0, x1) = (cells.iter().map(|c| c.0).min()?.saturating_sub(SETTLE_REACH), (cells.iter().map(|c| c.0).max()? + SETTLE_REACH).min(w - 1));
    let (y0, y1) = (cells.iter().map(|c| c.1).min()?.saturating_sub(SETTLE_REACH), (cells.iter().map(|c| c.1).max()? + SETTLE_REACH).min(h - 1));
    let dam: HashSet<Cell> = cells.iter().copied().collect();
    let near: Vec<Cell> = (y0..=y1).flat_map(|y| (x0..=x1).map(move |x| (x, y))).filter(|c| !dam.contains(c)).collect();
    let surface = |wt: &crate::water::Water, c: Cell| height.get(c.0, c.1).copied().unwrap_or(0.0) + wt.depth.get(c.0, c.1).copied().unwrap_or(0.0);
    let before: Vec<f32> = near.iter().map(|&c| surface(&water, c)).collect();
    water.raise_dam(&pairs).ok()?;
    let mut t = 0.0;
    while t < SETTLE_DAYS {
        let dt = crate::STEP_DAYS.min(SETTLE_DAYS - t);
        water.step(height, dt);
        t += dt;
    }
    let mut rise = 0.0f32;
    let mut flooded = Vec::new();
    let mut level = f32::NEG_INFINITY;
    for (&c, &b) in near.iter().zip(&before) {
        let after = surface(&water, c);
        let r = after - b;
        rise = rise.max(r);
        if r > 0.02 && water.depth.get(c.0, c.1).is_some_and(|&d| d > crate::water::WET_EPS) {
            flooded.push(c);
            level = level.max(after);
        }
    }
    Some(Settled { rise, flooded, level })
}

/// How far (cells) round a dam `settle` compares the water, and for how long it runs.
const SETTLE_REACH: usize = 40;
const SETTLE_DAYS: f32 = 0.05;

#[cfg(test)]
mod settle_tests {
    use super::*;

    /// A line drawn along a creek on the Bavarian map, where the quick estimate saw no rise
    /// but the water rose a third of a metre in play.
    #[test]
    fn settling_finds_the_rise_play_shows() {
        let (mut s, _) = crate::starts::new_game_on_map("bavarian").unwrap();
        // Other tests switch the process-wide default engine: use the game's own.
        s.water.set_engine(crate::water::DEFAULT_ENGINE);
        crate::sandbox::run_days(&mut s, 0.01);
        let cells = crate::sandbox::line(&[(387, 69), (384, 54)]);
        let h = crate::dams::first_targets(&s, &cells);
        let got = settle(s.water.clone(), &s.height, &cells, &h).unwrap();
        assert!(got.rise > 0.25, "settled rise {}", got.rise);
        assert!(!got.flooded.is_empty());
    }
}
