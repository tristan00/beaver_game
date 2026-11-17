//! Ecology: forest growth/felling/drowning, wetlands, wetland wildlife, roaming predators and the
//! danger field. All state is deterministic (seeded `Pcg32`); inputs from other systems are
//! passed to `step` (water depth) or set through the public fields (`lodges`, `defence`, `hideout`).
use crate::SimError;
use crate::grid::{Grid, VegetationGrid, WaterGrid};
use rand_core::{Rng, SeedableRng};
use rand_pcg::Pcg32;
pub use crate::tuning::ecology::*;

const MAX_SUBSTEP_DAYS: f32 = 1.0;

/// Predator species. Eagles fly over water; the rest roam land only.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PredatorKind { Wolf, Lynx, Bear, Eagle }

impl PredatorKind {
    /// Cells moved per day.
    fn speed(self) -> f32 {
        match self { Self::Wolf => 2.5, Self::Lynx => 1.5, Self::Bear => 1.0, Self::Eagle => 4.0 }
    }
    /// Threat weight in the danger field.
    fn threat(self) -> f32 {
        match self { Self::Wolf => 1.0, Self::Lynx => 0.7, Self::Bear => 0.9, Self::Eagle => 0.5 }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Predator { pub kind: PredatorKind, pub x: f32, pub y: f32 }

/// Colony-wide effects of wetland life (each >= 0; normalised by map area, roughly 0..1).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct WetlandEffects {
    /// Extra food from fish, waterfowl and frogs.
    pub food: f32,
    /// Morale from waterfowl and herons.
    pub morale: f32,
    /// Disease pressure from mosquitoes.
    pub disease: f32,
    /// Competition from otters for fish.
    pub otter_competition: f32,
}

/// Map layers the UI can show.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MapMode { Food, Forest, Wetland, Danger }

/// Per-cell ecology fields. All grids are `size x size`.
#[derive(Clone, Debug)]
pub struct Ecology {
    /// Overall ground cover 0..=1 (forest + half meadow + half wetland).
    pub vegetation: VegetationGrid,
    /// Living tree density 0..=1.
    pub forest: Grid<f32>,
    /// Dead standing trees left by drowning 0..=1.
    pub snags: Grid<f32>,
    pub meadow: Grid<f32>,
    pub wetland: Grid<f32>,
    /// Consecutive days flooded (drains at twice the rate when dry).
    pub flood_days: Grid<f32>,
    pub fish: Grid<f32>,
    pub waterfowl: Grid<f32>,
    pub frogs: Grid<f32>,
    pub herons: Grid<f32>,
    pub mosquitoes: Grid<f32>,
    pub otters: Grid<f32>,
    /// Food availability map 0..=1.
    pub food: Grid<f32>,
    /// Predator danger 0..=1.
    pub danger: Grid<f32>,
    /// Defence strength 0..=1 per cell (written by the buildings system).
    pub defence: Grid<f32>,
    /// Hideout strength 0..=1 per cell (written by the buildings system).
    pub hideout: Grid<f32>,
    /// Lodge cells; protect their surroundings. Use `add_lodge`.
    pub lodges: Vec<(usize, usize)>,
    pub predators: Vec<Predator>,
    pub effects: WetlandEffects,
    cut_cooldown: Grid<f32>,
    rng: Pcg32,
}

fn clamp01(v: f32) -> f32 { v.clamp(0.0, 1.0) }

impl Ecology {
    /// Ecology of a `size x size` world with default seed 0. Panics if `size == 0`.
    pub fn new(size: usize) -> Self { Self::seeded(size, 0) }

    /// Deterministic ecology: dense forest with seeded variation and a few predators.
    pub fn seeded(size: usize, seed: u64) -> Self {
        let g = |v: f32| Grid::new(size, size, v);
        let mut rng = Pcg32::seed_from_u64(seed ^ 0xEC01_06E5);
        let mut forest = g(0.8);
        for v in forest.as_mut_slice() { *v = 0.6 + 0.4 * unit(&mut rng); }
        let mut predators = Vec::new();
        for kind in [PredatorKind::Wolf, PredatorKind::Wolf, PredatorKind::Lynx, PredatorKind::Bear,
                     PredatorKind::Eagle, PredatorKind::Eagle] {
            predators.push(Predator { kind, x: unit(&mut rng) * size as f32, y: unit(&mut rng) * size as f32 });
        }
        let mut e = Self {
            vegetation: g(0.0), forest, snags: g(0.0), meadow: g(0.0), wetland: g(0.0), flood_days: g(0.0),
            fish: g(0.0), waterfowl: g(0.0), frogs: g(0.0), herons: g(0.0), mosquitoes: g(0.0),
            otters: g(0.0), food: g(0.0), danger: g(0.0), defence: g(0.0), hideout: g(0.0),
            lodges: Vec::new(), predators, effects: WetlandEffects::default(), cut_cooldown: g(0.0), rng,
        };
        e.refresh_derived(&g(0.0));
        e
    }

    pub fn size(&self) -> usize { self.forest.width() }

    /// Layer for a map mode.
    pub fn layer(&self, mode: MapMode) -> &Grid<f32> {
        match mode {
            MapMode::Food => &self.food,
            MapMode::Forest => &self.forest,
            MapMode::Wetland => &self.wetland,
            MapMode::Danger => &self.danger,
        }
    }

    /// Registers a lodge at (x, y). Errors if out of bounds.
    pub fn add_lodge(&mut self, x: usize, y: usize) -> Result<(), SimError> {
        self.check_cell(x, y)?;
        self.lodges.push((x, y));
        Ok(())
    }

    /// Fells up to `amount` (>= 0) of forest density at (x, y); returns the density actually
    /// removed (wood). Blocks regrowth there for `CUT_COOLDOWN_DAYS`.
    pub fn fell(&mut self, x: usize, y: usize, amount: f32) -> Result<f32, SimError> {
        self.check_cell(x, y)?;
        if !amount.is_finite() || amount < 0.0 {
            return Err(SimError::InvalidCommand(format!("fell amount must be finite and >= 0, got {amount}")));
        }
        let f = self.forest.get_mut(x, y).unwrap();
        let removed = amount.min(*f);
        *f -= removed;
        *self.meadow.get_mut(x, y).unwrap() += removed;
        *self.cut_cooldown.get_mut(x, y).unwrap() = CUT_COOLDOWN_DAYS;
        Ok(removed)
    }

    fn check_cell(&self, x: usize, y: usize) -> Result<(), SimError> {
        if self.forest.in_bounds(x, y) { Ok(()) } else {
            Err(SimError::InvalidCommand(format!("cell ({x}, {y}) out of bounds for size {}", self.size())))
        }
    }

    /// Advances ecology by `dt_days` (finite, >= 0) given current water depth.
    /// Errors if the water grid size differs from the ecology size.
    pub fn step(&mut self, dt_days: f32, water: &WaterGrid) -> Result<(), SimError> {
        if !dt_days.is_finite() || dt_days < 0.0 {
            return Err(SimError::InvalidDt(dt_days));
        }
        let n = self.size();
        if water.width() != n || water.height() != n {
            return Err(SimError::InvalidCommand(format!(
                "water grid {}x{} does not match ecology size {n}", water.width(), water.height())));
        }
        if dt_days == 0.0 { return Ok(()); }
        let steps = (dt_days / MAX_SUBSTEP_DAYS).ceil().max(1.0) as usize;
        let dt = dt_days / steps as f32;
        for _ in 0..steps {
            self.substep(dt, water);
        }
        self.refresh_derived(water);
        Ok(())
    }

    fn substep(&mut self, dt: f32, water: &WaterGrid) {
        let n = self.size();
        let wet_days = WETLAND_SEASONS * SEASON_DAYS;
        let old_forest = self.forest.as_slice().to_vec();
        for i in 0..n * n {
            let depth = water.as_slice()[i];
            let flooded = depth > FLOOD_DEPTH;
            let fd = &mut self.flood_days.as_mut_slice()[i];
            *fd = if flooded { *fd + dt } else { (*fd - 2.0 * dt).max(0.0) };
            let fd = *fd;
            // Drowning: flooded trees die and leave snags.
            let forest = &mut self.forest.as_mut_slice()[i];
            let snags = &mut self.snags.as_mut_slice()[i];
            if flooded && fd >= DROWN_FLOOD_DAYS && depth >= DROWN_DEPTH {
                let lost = (DROWN_RATE * dt).min(*forest);
                *forest -= lost;
                *snags = clamp01(*snags + lost);
            }
            // Snags rot over about two years.
            *snags = (*snags - dt / 730.0).max(0.0);
            // Wetland forms on long-flooded shallow cells; drained wetland becomes meadow.
            let wetland = &mut self.wetland.as_mut_slice()[i];
            if flooded && fd >= wet_days && depth <= WETLAND_MAX_DEPTH {
                *wetland = clamp01(*wetland + dt / SEASON_DAYS);
            } else if !flooded || depth > WETLAND_MAX_DEPTH {
                let shift = (dt / (2.0 * SEASON_DAYS)).min(*wetland);
                *wetland -= shift;
                if !flooded { self.meadow.as_mut_slice()[i] += shift; }
            }
            let c = &mut self.cut_cooldown.as_mut_slice()[i];
            *c = (*c - dt).max(0.0);
        }
        // Growth and spread from pre-step neighbours so cell order does not matter.
        for y in 0..n {
            for x in 0..n {
                let i = y * n + x;
                if water.as_slice()[i] > FLOOD_DEPTH || self.cut_cooldown.as_slice()[i] > 0.0 { continue; }
                let nb = |dx: isize, dy: isize| {
                    let (nx, ny) = (x as isize + dx, y as isize + dy);
                    if nx < 0 || ny < 0 || nx >= n as isize || ny >= n as isize { old_forest[i] }
                    else { old_forest[ny as usize * n + nx as usize] }
                };
                let neighbours = (nb(1, 0) + nb(-1, 0) + nb(0, 1) + nb(0, -1)) / 4.0;
                let cap = 1.0 - self.wetland.as_slice()[i];
                let f = &mut self.forest.as_mut_slice()[i];
                if *f < cap {
                    let rate = 0.0005 + 0.01 * *f + 0.01 * neighbours;
                    *f = (*f + dt * rate * (cap - *f)).min(cap);
                }
            }
        }
        for i in 0..n * n {
            let room = (1.0 - self.forest.as_slice()[i] - self.wetland.as_slice()[i]).max(0.0);
            let m = &mut self.meadow.as_mut_slice()[i];
            // Meadow gives way to forest slowly and never exceeds free ground.
            *m = (*m - dt / 730.0).max(0.0).min(room);
        }
        self.move_predators(dt, water);
        self.relax_wildlife(dt);
    }

    fn move_predators(&mut self, dt: f32, water: &WaterGrid) {
        let n = self.size() as f32;
        for p in &mut self.predators {
            let ang = unit(&mut self.rng) * std::f32::consts::TAU;
            let step = p.kind.speed() * dt;
            let nx = (p.x + ang.cos() * step).clamp(0.0, n - 0.001);
            let ny = (p.y + ang.sin() * step).clamp(0.0, n - 0.001);
            let blocked = p.kind != PredatorKind::Eagle
                && water.get(nx as usize, ny as usize).is_some_and(|d| *d > 0.3);
            if !blocked { p.x = nx; p.y = ny; }
        }
    }

    /// Wetland-linked wildlife relaxes toward targets set by nearby wetland.
    fn relax_wildlife(&mut self, dt: f32) {
        let n = self.size();
        let k = clamp01(dt * 0.05);
        let blurred = blur(&self.wetland);
        for i in 0..n * n {
            let w = blurred.as_slice()[i];
            let forest = self.forest.as_slice()[i];
            let stagnation = clamp01(self.flood_days.as_slice()[i] / 120.0);
            let targets = [
                (&mut self.fish, w),
                (&mut self.waterfowl, w * (1.0 - 0.5 * forest)),
                (&mut self.frogs, w),
                (&mut self.herons, w * 0.5 * (w + w)),
                (&mut self.mosquitoes, w * (0.3 + 0.7 * stagnation)),
                (&mut self.otters, w * 0.6 * w),
            ];
            for (g, t) in targets {
                let v = &mut g.as_mut_slice()[i];
                *v += (t - *v) * k;
            }
        }
    }

    /// Recomputes derived layers: vegetation, food, danger, colony effects.
    fn refresh_derived(&mut self, water: &WaterGrid) {
        let n = self.size();
        let cells = (n * n) as f32;
        let mut sums = WetlandEffects::default();
        for i in 0..n * n {
            let s = |g: &Grid<f32>| g.as_slice()[i];
            let (forest, meadow, wetland) = (s(&self.forest), s(&self.meadow), s(&self.wetland));
            self.vegetation.as_mut_slice()[i] = clamp01(forest + 0.5 * meadow + 0.5 * wetland);
            let wild_food = 0.8 * s(&self.fish) + 0.6 * s(&self.waterfowl) + 0.2 * s(&self.frogs);
            self.food.as_mut_slice()[i] = clamp01(0.5 * forest + 0.6 * meadow + wild_food);
            sums.food += wild_food;
            sums.morale += s(&self.waterfowl) + s(&self.herons);
            sums.disease += s(&self.mosquitoes);
            sums.otter_competition += s(&self.otters);
        }
        self.effects = WetlandEffects {
            food: sums.food / cells, morale: sums.morale / cells,
            disease: sums.disease / cells, otter_competition: sums.otter_competition / cells,
        };
        self.update_danger(water);
    }

    /// Danger is high far from water and lodges, raised near predators, and lowered by
    /// defences and hideouts.
    fn update_danger(&mut self, water: &WaterGrid) {
        let n = self.size();
        let mut sources: Vec<bool> = water.as_slice().iter().map(|d| *d > FLOOD_DEPTH).collect();
        for &(x, y) in &self.lodges { sources[y * n + x] = true; }
        let dist = distance_field(&sources, n);
        let mut pred = vec![0.0f32; n * n];
        for p in &self.predators {
            let (px, py) = (p.x, p.y);
            let (x0, x1) = ((px - PREDATOR_RANGE).max(0.0) as usize, ((px + PREDATOR_RANGE) as usize).min(n - 1));
            let (y0, y1) = ((py - PREDATOR_RANGE).max(0.0) as usize, ((py + PREDATOR_RANGE) as usize).min(n - 1));
            for y in y0..=y1 {
                for x in x0..=x1 {
                    let d = ((x as f32 - px).powi(2) + (y as f32 - py).powi(2)).sqrt();
                    if d < PREDATOR_RANGE { pred[y * n + x] += p.kind.threat() * (1.0 - d / PREDATOR_RANGE); }
                }
            }
        }
        for i in 0..n * n {
            let base = 0.05 + 0.95 * (dist[i] / SAFE_RANGE).min(1.0);
            let raw = base * (0.5 + 0.5 * pred[i].min(1.0))
                * (1.0 - clamp01(self.defence.as_slice()[i]))
                * (1.0 - 0.5 * clamp01(self.hideout.as_slice()[i]));
            self.danger.as_mut_slice()[i] = clamp01(raw);
        }
    }
}

/// Uniform f32 in [0, 1).
fn unit(rng: &mut Pcg32) -> f32 { (rng.next_u32() >> 8) as f32 / (1u32 << 24) as f32 }

/// 3x3 box blur with edge clamping.
fn blur(g: &Grid<f32>) -> Grid<f32> {
    let n = g.width();
    let mut out = g.clone();
    for y in 0..n {
        for x in 0..n {
            let (mut sum, mut cnt) = (0.0, 0.0);
            for yy in y.saturating_sub(1)..=(y + 1).min(n - 1) {
                for xx in x.saturating_sub(1)..=(x + 1).min(n - 1) {
                    sum += g.as_slice()[yy * n + xx];
                    cnt += 1.0;
                }
            }
            out.as_mut_slice()[y * n + x] = sum / cnt;
        }
    }
    out
}

/// Chamfer distance (cells) to the nearest source; very large when there are no sources.
fn distance_field(sources: &[bool], n: usize) -> Vec<f32> {
    const INF: f32 = f32::MAX / 4.0;
    let mut d: Vec<f32> = sources.iter().map(|&s| if s { 0.0 } else { INF }).collect();
    let diag = std::f32::consts::SQRT_2;
    for y in 0..n {
        for x in 0..n {
            let mut v = d[y * n + x];
            if x > 0 { v = v.min(d[y * n + x - 1] + 1.0); }
            if y > 0 {
                v = v.min(d[(y - 1) * n + x] + 1.0);
                if x > 0 { v = v.min(d[(y - 1) * n + x - 1] + diag); }
                if x + 1 < n { v = v.min(d[(y - 1) * n + x + 1] + diag); }
            }
            d[y * n + x] = v;
        }
    }
    for y in (0..n).rev() {
        for x in (0..n).rev() {
            let mut v = d[y * n + x];
            if x + 1 < n { v = v.min(d[y * n + x + 1] + 1.0); }
            if y + 1 < n {
                v = v.min(d[(y + 1) * n + x] + 1.0);
                if x + 1 < n { v = v.min(d[(y + 1) * n + x + 1] + diag); }
                if x > 0 { v = v.min(d[(y + 1) * n + x - 1] + diag); }
            }
            d[y * n + x] = v;
        }
    }
    d
}

#[cfg(test)]
mod ecology {
    use super::*;

    const N: usize = 24;
    fn flood(depth: f32) -> WaterGrid { WaterGrid::new(N, N, depth) }
    fn run(e: &mut Ecology, w: &WaterGrid, days: f32) {
        let mut left = days;
        while left > 0.0 { let d = left.min(5.0); e.step(d, w).unwrap(); left -= d; }
    }
    fn mean(g: &Grid<f32>) -> f32 { g.as_slice().iter().sum::<f32>() / g.as_slice().len() as f32 }

    #[test]
    fn flooded_trees_die_and_leave_snags() {
        let mut e = Ecology::seeded(N, 1);
        let before = mean(&e.forest);
        run(&mut e, &flood(0.5), 40.0);
        assert!(mean(&e.forest) < before * 0.3, "forest {} -> {}", before, mean(&e.forest));
        assert!(mean(&e.snags) > 0.2, "snags {}", mean(&e.snags));
    }

    #[test]
    fn short_flood_does_not_drown() {
        let mut e = Ecology::seeded(N, 1);
        let before = mean(&e.forest);
        run(&mut e, &flood(0.5), 10.0);
        assert!((mean(&e.forest) - before).abs() < 0.01);
    }

    #[test]
    fn wetland_forms_after_seasons_of_flooding_then_meadow_after_drainage() {
        let mut e = Ecology::seeded(N, 2);
        run(&mut e, &flood(0.5), SEASON_DAYS * 1.5);
        assert_eq!(mean(&e.wetland), 0.0, "no wetland before N seasons");
        run(&mut e, &flood(0.5), SEASON_DAYS * 4.5);
        assert!(mean(&e.wetland) > 0.9, "wetland {}", mean(&e.wetland));
        assert!(e.effects.food > 0.0 && e.effects.disease > 0.0 && e.effects.morale > 0.0);
        run(&mut e, &flood(0.0), SEASON_DAYS * 8.0);
        assert!(mean(&e.wetland) < 0.05);
        assert!(mean(&e.meadow) > 0.2, "meadow {}", mean(&e.meadow));
    }

    #[test]
    fn deep_water_never_becomes_wetland() {
        let mut e = Ecology::seeded(N, 2);
        run(&mut e, &flood(3.0), SEASON_DAYS * 6.0);
        assert_eq!(mean(&e.wetland), 0.0);
    }

    #[test]
    fn overcutting_destroys_forest_and_it_regrows_when_cutting_stops() {
        let mut e = Ecology::seeded(N, 3);
        let dry = flood(0.0);
        let start = mean(&e.forest);
        for _ in 0..24 { // two years of monthly clear-cutting
            for y in 0..N { for x in 0..N { e.fell(x, y, 1.0).unwrap(); } }
            run(&mut e, &dry, 30.0);
        }
        let low = mean(&e.forest);
        assert!(low < 0.1, "forest after overcutting {low}");
        run(&mut e, &dry, 365.0 * 8.0);
        let regrown = mean(&e.forest);
        assert!(regrown > start * 0.8 && regrown > low + 0.4, "regrown {regrown} (start {start}, low {low})");
    }

    #[test]
    fn danger_is_lower_near_water_and_lodges_than_deep_forest() {
        let mut e = Ecology::seeded(N, 4);
        let mut w = flood(0.0);
        for y in 0..N { *w.get_mut(0, y).unwrap() = 1.0; } // river on the west edge
        e.add_lodge(N - 1, N - 1).unwrap();
        run(&mut e, &w, 5.0);
        let at = |x, y| *e.danger.get(x, y).unwrap();
        let deep = at(N / 2 + 2, N / 2 - 6).max(at(N / 2, N / 2));
        assert!(at(1, N / 2) < deep, "near water {} vs deep {}", at(1, N / 2), deep);
        assert!(at(N - 2, N - 2) < deep, "near lodge {} vs deep {}", at(N - 2, N - 2), deep);
    }

    #[test]
    fn defences_and_hideouts_lower_danger() {
        let mut e = Ecology::seeded(N, 4);
        let w = flood(0.0);
        run(&mut e, &w, 1.0);
        let base = *e.danger.get(12, 12).unwrap();
        *e.defence.get_mut(12, 12).unwrap() = 0.5;
        *e.hideout.get_mut(12, 12).unwrap() = 1.0;
        run(&mut e, &w, 1.0);
        assert!(*e.danger.get(12, 12).unwrap() < base * 0.8);
    }

    #[test]
    fn predators_roam_stay_in_bounds_and_are_deterministic() {
        let mut a = Ecology::seeded(N, 5);
        let mut b = Ecology::seeded(N, 5);
        let w = flood(0.0);
        let start = a.predators.clone();
        run(&mut a, &w, 100.0);
        run(&mut b, &w, 100.0);
        assert_eq!(a.predators, b.predators);
        assert_ne!(a.predators, start);
        assert!(a.predators.iter().all(|p| p.x >= 0.0 && p.y >= 0.0 && p.x < N as f32 && p.y < N as f32));
    }

    #[test]
    fn invalid_inputs_are_errors() {
        let mut e = Ecology::seeded(N, 1);
        assert!(matches!(e.step(f32::NAN, &flood(0.0)), Err(SimError::InvalidDt(_))));
        assert!(matches!(e.step(1.0, &WaterGrid::new(4, 4, 0.0)), Err(SimError::InvalidCommand(_))));
        assert!(e.fell(N, 0, 1.0).is_err());
        assert!(e.fell(0, 0, -1.0).is_err());
        assert!(e.add_lodge(0, N).is_err());
    }

    #[test]
    fn map_layers_are_exposed_and_in_range() {
        let mut e = Ecology::seeded(N, 6);
        run(&mut e, &flood(0.5), 120.0);
        for m in [MapMode::Food, MapMode::Forest, MapMode::Wetland, MapMode::Danger] {
            assert!(e.layer(m).as_slice().iter().all(|v| (0.0..=1.0).contains(v)), "{m:?}");
        }
    }
}
