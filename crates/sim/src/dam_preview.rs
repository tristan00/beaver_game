include!(concat!(env!("OUT_DIR"), "/dam_preview_constants.rs"));
use crate::buildings::{Amount, BuildingId, BuildingKind};
use crate::resources::ResourceKind;
use crate::water::{preview_dam_heights, Cell};
use std::collections::HashSet;



/// Loads needed to bring these cells to these heights, one load per step per cell.
pub fn loads_for(sim: &crate::SimState, cells: &[Cell], heights: &[f32]) -> usize {
    cells.iter().zip(heights).map(|(&c, &h)| {
        let have = sim.water.dam_height.get(c.0, c.1).copied().unwrap_or(0.0);
        ((h - have).max(0.0) / crate::dams::STEP_HEIGHT - 1e-3).ceil().max(0.0) as usize
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
    let pond = preview_dam_heights(sim, cells, heights).ok()?;
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
    })
}
