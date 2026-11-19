use crate::buildings::{Amount, BuildingId, BuildingKind, Good};
use crate::resources::ResourceKind;
use crate::water::{preview_dam_heights, Cell};
use std::collections::HashSet;

pub const DAM_SPAN_CELLS: f32 = 20.0;

pub const DEFAULT_HEIGHT: f32 = 1.0;
pub const MAX_DAM_HEIGHT: f32 = 3.0;
pub const UPPER_PER_UNIT: [Amount; 3] = [(Good::Logs, 1.5), (Good::Sticks, 2.5), (Good::Mud, 3.0)];

pub fn build_units(sim: &crate::SimState, cell: Cell, h: f32) -> (f32, f32) {
    let have = sim.water.dam_height.get(cell.0, cell.1).copied().unwrap_or(0.0)
        * sim.water.dam_integrity.get(cell.0, cell.1).copied().unwrap_or(0.0);
    let base = (h.min(1.0) - have.min(1.0)).max(0.0);
    let upper = ((h.max(1.0) - 1.0) - (have.max(1.0) - 1.0)).max(0.0);
    (base, upper)
}

pub fn cost_for(sim: &crate::SimState, cells: &[Cell], heights: &[f32]) -> (Vec<Amount>, f32) {
    let (mut base, mut upper) = (0.0, 0.0);
    for (&c, &h) in cells.iter().zip(heights) {
        let (b, u) = build_units(sim, c, h);
        base += b;
        upper += u;
    }
    let mut cost: Vec<Amount> = BuildingKind::Dam.spec().cost.iter().map(|&(g, a)| (g, a * base / DAM_SPAN_CELLS)).collect();
    for (g, a) in UPPER_PER_UNIT {
        match cost.iter_mut().find(|c| c.0 == g) {
            Some(c) => c.1 += a * upper,
            None => cost.push((g, a * upper)),
        }
    }
    cost.retain(|c| c.1 > 1e-3);
    (cost, BuildingKind::Dam.spec().work * (base + upper) / DAM_SPAN_CELLS)
}

pub fn extend_to_hold(sim: &crate::SimState, cells: &[Cell], heights: &[f32], max_cells: usize) -> Option<(Vec<Cell>, Vec<f32>, DamPreview)> {
    let (mut cells, mut heights) = (cells.to_vec(), heights.to_vec());
    let mut crest = None;
    loop {
        let p = preview_heights(sim, &cells, &heights)?;
        let crest = *crest.get_or_insert(p.crest);
        let Some(at) = p.spill_at else { return Some((cells, heights, p)) };
        let h = crest - sim.height.get(at.0, at.1).copied()?;
        if cells.len() >= max_cells || cells.contains(&at) || h <= 0.0 || h > MAX_DAM_HEIGHT {
            return None;
        }
        cells.push(at);
        heights.push(h);
    }
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

pub fn spot_cells(pos: Cell) -> Vec<Cell> {
    let (w, h) = BuildingKind::Dam.spec().footprint;
    (0..h).flat_map(|dy| (0..w).map(move |dx| (pos.0 + dx, pos.1 + dy))).collect()
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

pub fn length_factor(n: usize) -> f32 {
    n as f32 / DAM_SPAN_CELLS
}

pub fn dam_cost(n: usize) -> Vec<Amount> {
    BuildingKind::Dam.spec().cost.iter().map(|&(g, a)| (g, a * length_factor(n))).collect()
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
    let hydraulic_days_per_calendar_day = (sim.water.hydraulic_seconds_per_day() / 86_400.0) as f32;
    let pressure: f32 = sim.water.sources.iter().filter(|s| seen[s.y * w + s.x])
        .map(|s| s.rate * (1.0 + s.seasonality * (forcing.inflow - 1.0)).max(0.0) * hydraulic_days_per_calendar_day).sum();

    let cfg = &sim.water.config;
    let (fed, ice) = (sim.water.fed.as_slice(), sim.water.ice.as_slice());
    let loss_at = |i: usize| if fed[i] { 0.0 } else { cfg.evaporation * season.evap_mult() * (1.0 - ice[i]) + cfg.seepage };
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fill_estimate_uses_calendar_inflow_at_the_current_hydraulic_rate() {
        let mut s = crate::SimState::sandbox(1, crate::worldgen::fixture(1));
        s.water.weather = crate::water::Weather::Clear;
        let line: Vec<_> = (43..=86).map(|y| (45, y)).collect();
        let (cells, heights) = heights_to(&s, &line, 8.35);
        let reference = preview_heights(&s, &cells, &heights).unwrap();
        assert!(reference.pressure > 0.0);
        s.step_game_at_hydraulic_rate(1e-6, 3600.0).unwrap();
        let paced = preview_heights(&s, &cells, &heights).unwrap();
        assert!((paced.pressure / reference.pressure - 1.0 / 24.0).abs() < 1e-5);
        assert!(paced.fill_days >= reference.fill_days);
    }
}

