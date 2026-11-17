//! Dam preview (owner: `dam`): what a candidate dam would do, as a pure function of the state.
//!
//! Contract: `preview(sim, cells, height)` never mutates `sim`. It returns `None` when the cells
//! are not a valid dam line (empty, out of bounds, duplicated, or `height` not finite and > 0).
//! The pond is the steady one from `water::preview_dam`; this adds what the player must weigh:
//! trees and buildings lost, material cost, the push of the river on the dam and the fill time.

use crate::buildings::{Amount, BuildingId, BuildingKind};
use crate::resources::ResourceKind;
use crate::water::{preview_dam, Cell};
use std::collections::HashSet;

/// Dam cells one catalogue cost buys (a standard six-cell span); cost scales linearly with length.
pub const DAM_SPAN_CELLS: f32 = 6.0;

#[derive(Clone, Debug, PartialEq)]
pub struct DamPreview {
    /// Water surface the pond fills to (the lowest crest of the dam), in world units.
    pub level: f32,
    /// Rise of the pond surface above the current water surface at the dam, in world units.
    pub rise: f32,
    /// Cells that would be under water afterwards, sorted by index (y, then x).
    pub flooded: Vec<Cell>,
    /// Standing trees on flooded cells.
    pub flooded_trees: usize,
    /// Buildings (finished or under construction) with a footprint cell flooded.
    pub flooded_buildings: Vec<BuildingId>,
    /// Materials for this many dam cells (the dam spec cost scaled by length).
    pub cost: Vec<Amount>,
    /// Inflow pushing on the dam, water units per day (main river above tributary): the sum of
    /// the current seasonal rates of every spring/source whose water drains to the dam.
    pub pressure: f32,
    /// Days for the pond to reach `level` at the current inflow; 0 if it already holds that much
    /// water, `f32::INFINITY` if nothing flows in. Ignores rain, evaporation and seepage.
    pub fill_days: f32,
}

/// Cost of a dam of `n` cells.
pub fn dam_cost(n: usize) -> Vec<Amount> {
    BuildingKind::Dam.spec().cost.iter().map(|&(g, a)| (g, a * n as f32 / DAM_SPAN_CELLS)).collect()
}

/// Evaluates a dam of `height` on `cells`; `None` if the cells are not a valid dam line.
pub fn preview(sim: &crate::SimState, cells: &[Cell], height: f32) -> Option<DamPreview> {
    let unique: HashSet<&Cell> = cells.iter().collect();
    if unique.len() != cells.len() {
        return None;
    }
    let pond = preview_dam(sim, cells, height).ok()?;
    let (w, h) = (sim.height.width(), sim.height.height());
    let idx = |&(x, y): &Cell| y * w + x;
    let flooded_set: HashSet<Cell> = pond.flooded_cells.iter().copied().collect();

    let flooded_trees = sim.resources.items.iter()
        .filter(|r| r.kind == ResourceKind::Tree && flooded_set.contains(&r.cell)).count();
    let flooded_buildings = sim.buildings.buildings.iter()
        .filter(|b| b.cells().any(|c| flooded_set.contains(&c))).map(|b| b.id).collect();

    // Current surface at the dam cell that sets the level (lowest crest).
    let terrain = sim.height.as_slice();
    let (water, dams) = (sim.water.depth.as_slice(), sim.water.dam_height.as_slice());
    let crest = |c: &Cell| terrain[idx(c)] + height;
    let low = cells.iter().min_by(|a, b| crest(a).total_cmp(&crest(b)))?;
    let rise = (pond.level - (terrain[idx(low)] + dams[idx(low)] + water[idx(low)])).max(0.0);

    // Catchment: cells whose steepest descent reaches the dam, plus the pond itself.
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
    let pressure: f32 = sim.water.sources.iter().filter(|s| seen[s.y * w + s.x])
        .map(|s| s.rate * (1.0 + s.seasonality * (season.inflow_mult() - 1.0))).sum();

    // Water still missing from the pond.
    let missing: f32 = pond.flooded_cells.iter().map(idx)
        .map(|i| (pond.level - terrain[i] - dams[i] - water[i]).max(0.0)).sum();
    let fill_days = if missing <= 0.0 { 0.0 } else if pressure > 0.0 { missing / pressure } else { f32::INFINITY };

    Some(DamPreview {
        level: pond.level,
        rise,
        flooded: pond.flooded_cells,
        flooded_trees,
        flooded_buildings,
        cost: dam_cost(cells.len()),
        pressure,
        fill_days,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::buildings::{Building, Status};
    use crate::water::{Source, Water, WaterConfig};
    use crate::SimState;

    /// Dry valley sloping toward +x with rising sides, no seeded water, no climate losses.
    fn valley() -> SimState {
        let mut s = SimState::new(1);
        let n = s.height.width();
        for y in 0..n {
            for x in 0..n {
                let dy = (y as f32 - 64.0).abs();
                *s.height.get_mut(x, y).unwrap() = 0.2 * (n - x) as f32 + 0.15 * dy;
            }
        }
        s.resources.items.clear();
        s.water = Water::new(n);
        s.water.config = WaterConfig {
            rain: 0.0, evaporation: 0.0, seepage: 0.0, dam_decay_per_day: 0.0,
            overtop_erosion: 0.0, open_edges: false, ..Default::default()
        };
        s
    }

    fn row() -> Vec<Cell> { (0..128).map(|y| (40, y)).collect() }
    fn src(x: usize, y: usize, rate: f32) -> Source { Source { x, y, rate, seasonality: 0.0 } }

    #[test]
    fn preview_reports_level_flooding_assets_and_cost() {
        let mut s = valley();
        let (tree_cells, b_cell) = ([(35, 64), (37, 66), (33, 63)], (34, 64));
        for c in tree_cells { s.resources.add(ResourceKind::Tree, c, 1.0); }
        s.resources.add(ResourceKind::Tree, (60, 64), 1.0); // downstream: dry
        s.resources.add(ResourceKind::Tree, (35, 120), 1.0); // too high up the bank: dry
        s.buildings.buildings.push(Building {
            id: BuildingId(7), kind: BuildingKind::Burrow, pos: b_cell, status: Status::Complete, recipe: None, output: 0.0,
        });
        let before = (s.water.depth.clone(), s.water.dam_height.clone(), s.height.clone());
        let p = preview(&s, &row(), 2.0).unwrap();
        assert_eq!(before, (s.water.depth.clone(), s.water.dam_height.clone(), s.height.clone()));

        let lowest = row().iter().map(|&(x, y)| s.height.get(x, y).unwrap() + 2.0).fold(f32::INFINITY, f32::min);
        assert!((p.level - lowest).abs() < 0.05, "level {}", p.level);

        // Exactly the upstream (x < 40) cells whose bed is below the crest.
        let mut want: Vec<Cell> = (0..40).flat_map(|x| (0..128).map(move |y| (x, y)))
            .filter(|&(x, y)| *s.height.get(x, y).unwrap() < p.level).collect();
        want.sort_by_key(|&(x, y)| (y, x));
        let mut got = p.flooded.clone();
        got.sort_by_key(|&(x, y)| (y, x));
        assert!(!want.is_empty());
        assert_eq!(got, want);

        assert_eq!(p.flooded_trees, 3);
        assert_eq!(p.flooded_buildings, vec![BuildingId(7)]);
        assert!((p.rise - 2.0).abs() < 0.05, "rise {}", p.rise);
        assert_eq!(p.cost, dam_cost(128));
        for (c, sp) in p.cost.iter().zip(BuildingKind::Dam.spec().cost) {
            assert_eq!(c.0, sp.0);
            assert!((c.1 - sp.1 * 128.0 / DAM_SPAN_CELLS).abs() < 1e-3);
        }
        assert_eq!(p.pressure, 0.0);
        assert!(p.fill_days.is_infinite());
    }

    #[test]
    fn invalid_dams_are_rejected() {
        let s = valley();
        assert!(preview(&s, &[], 1.0).is_none());
        assert!(preview(&s, &[(999, 0)], 1.0).is_none());
        assert!(preview(&s, &[(40, 1)], 0.0).is_none());
        assert!(preview(&s, &[(40, 1), (40, 1)], 1.0).is_none());
    }

    /// Main valley along y = 30 flowing toward +x; a tributary valley along x = 90 flowing toward
    /// -y joins it at (90, 30). Walls rise 0.5 per cell either side of each channel.
    fn junction() -> SimState {
        let mut s = valley();
        let n = s.height.width();
        for y in 0..n {
            for x in 0..n {
                let main = 0.2 * (128 - x) as f32 + 0.5 * (y as f32 - 30.0).abs();
                let trib = 7.6 + 0.2 * (y as f32 - 30.0).max(0.0) + 0.5 * (x as f32 - 90.0).abs();
                *s.height.get_mut(x, y).unwrap() = main.min(trib);
            }
        }
        s
    }

    #[test]
    fn main_river_pushes_harder_than_a_tributary() {
        let mut s = junction();
        s.water.sources.push(src(70, 30, 2.0)); // main river spring
        s.water.sources.push(src(90, 80, 0.5)); // tributary spring
        let main_cells: Vec<Cell> = (10..51).map(|y| (110, y)).collect(); // below the junction
        let trib_cells: Vec<Cell> = (85..96).map(|x| (x, 60)).collect();
        let main = preview(&s, &main_cells, 2.0).unwrap();
        let trib = preview(&s, &trib_cells, 2.0).unwrap();
        assert!(!main.flooded.is_empty() && !trib.flooded.is_empty());
        assert_eq!(main.pressure, 2.5, "main river carries both springs");
        assert_eq!(trib.pressure, 0.5, "tributary carries only its own");
        assert!(main.pressure > trib.pressure);
    }

    #[test]
    fn finished_dam_fills_pond_as_previewed() {
        let mut s = valley();
        s.water.sources.push(src(10, 64, 0.5));
        let cells = row();
        let p = preview(&s, &cells, 1.0).unwrap();
        assert!(p.fill_days.is_finite() && p.fill_days > 5.0, "fill {}", p.fill_days);
        s.apply(crate::Command::BuildDam { cells, height: 1.0 }).unwrap();
        let need: f32 = p.flooded.iter().map(|&(x, y)| p.level - s.height.get(x, y).unwrap()).sum();
        let (dt, mut t, mut done) = (0.05, 0.0, false);
        while t < p.fill_days * 2.0 {
            s.step(dt).unwrap();
            t += dt;
            if s.water.total_volume() as f32 >= need * 0.999 { done = true; break; }
        }
        assert!(done, "never filled");
        assert!(((t - p.fill_days) / p.fill_days).abs() < 0.10, "filled in {t}, previewed {}", p.fill_days);
        let w = s.height.width();
        let outside = (0..w * w).map(|i| (i % w, i / w))
            .filter(|&(x, y)| *s.water.depth.get(x, y).unwrap() > 0.02 && !s.water.has_dam(x, y))
            .filter(|&(x, y)| *s.height.get(x, y).unwrap() < p.level && !p.flooded.contains(&(x, y))).count();
        // (The trickle of the stream feeding the pond lies above the level and is not flooding.)
        assert_eq!(outside, 0, "water ponded outside the previewed cells");
    }
}
