use crate::SimError;
use crate::grid::{Grid, VegetationGrid, WaterGrid};
use rand_core::{Rng, SeedableRng};
use rand_pcg::Pcg32;
pub use crate::tuning::ecology::*;

const MAX_SUBSTEP_DAYS: f32 = 1.0;
/// Forest, meadow and wetland change over days; the whole-map vegetation refreshes
/// every six hours.
const GRID_EVERY_DAYS: f32 = 0.25;

#[derive(Clone, Debug)]
pub struct Ecology {
    pub vegetation: VegetationGrid,
    pub forest: Grid<f32>,
    pub meadow: Grid<f32>,
    pub wetland: Grid<f32>,
    pub flood_days: Grid<f32>,
    pending_days: f32,
}

fn clamp01(v: f32) -> f32 { v.clamp(0.0, 1.0) }

impl Ecology {

    pub fn seeded(size: usize, seed: u64) -> Self {
        let g = |v: f32| Grid::new(size, size, v);
        let mut rng = Pcg32::seed_from_u64(seed ^ 0xEC01_06E5);
        let mut forest = g(0.0);
        for v in forest.as_mut_slice() { *v = START_FOREST_MIN + START_FOREST_SPREAD * unit(&mut rng); }
        let mut e = Self { vegetation: g(0.0), forest, meadow: g(0.0), wetland: g(0.0), flood_days: g(0.0), pending_days: 0.0 };
        e.refresh_vegetation();
        e
    }

    pub fn size(&self) -> usize { self.forest.width() }

    pub fn set_forest(&mut self, density: &Grid<f32>) {
        let n = self.size();
        if density.width() != n || density.height() != n {
            return;
        }
        for i in 0..n * n {
            let new = clamp01(density.as_slice()[i]).min(1.0 - self.wetland.as_slice()[i]);
            let f = &mut self.forest.as_mut_slice()[i];
            if new < *f {
                self.meadow.as_mut_slice()[i] += *f - new;
            }
            *f = new;
        }
        self.refresh_vegetation();
    }

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
        self.pending_days += dt_days;
        if self.pending_days < GRID_EVERY_DAYS {
            return Ok(());
        }
        let due = std::mem::take(&mut self.pending_days);
        let steps = (due / MAX_SUBSTEP_DAYS).ceil().max(1.0) as usize;
        let dt = due / steps as f32;
        for _ in 0..steps {
            self.substep(dt, water);
        }
        self.refresh_vegetation();
        Ok(())
    }

    fn substep(&mut self, dt: f32, water: &WaterGrid) {
        let n = self.size();
        let wet_days = WETLAND_SEASONS * SEASON_DAYS;
        for i in 0..n * n {
            let depth = water.as_slice()[i];
            let flooded = depth > FLOOD_DEPTH;
            let fd = &mut self.flood_days.as_mut_slice()[i];
            *fd = if flooded { *fd + dt } else { (*fd - FLOOD_DAYS_DRAIN_RATE * dt).max(0.0) };
            let fd = *fd;
            let forest = &mut self.forest.as_mut_slice()[i];
            if flooded && fd >= DROWN_FLOOD_DAYS && depth >= DROWN_DEPTH {
                *forest -= (DROWN_RATE * dt).min(*forest);
            }
            let wetland = &mut self.wetland.as_mut_slice()[i];
            if flooded && fd >= wet_days && depth <= WETLAND_MAX_DEPTH {
                *wetland = clamp01(*wetland + dt / SEASON_DAYS);
            } else if !flooded || depth > WETLAND_MAX_DEPTH {
                let shift = (dt / (WETLAND_DRAIN_SEASONS * SEASON_DAYS)).min(*wetland);
                *wetland -= shift;
                if !flooded { self.meadow.as_mut_slice()[i] += shift; }
            }
        }
        for i in 0..n * n {
            let room = (1.0 - self.forest.as_slice()[i] - self.wetland.as_slice()[i]).max(0.0);
            let m = &mut self.meadow.as_mut_slice()[i];
            *m = (*m - dt / MEADOW_REGROW_DAYS).max(0.0).min(room);
        }
    }

    fn refresh_vegetation(&mut self) {
        let n = self.size();
        for i in 0..n * n {
            let (forest, meadow, wetland) = (self.forest.as_slice()[i], self.meadow.as_slice()[i], self.wetland.as_slice()[i]);
            self.vegetation.as_mut_slice()[i] = clamp01(forest + VEGETATION_MEADOW * meadow + VEGETATION_WETLAND * wetland);
        }
    }
}

fn unit(rng: &mut Pcg32) -> f32 { (rng.next_u32() >> 8) as f32 / (1u32 << 24) as f32 }
