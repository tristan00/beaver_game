//! Deterministic terrain generation: seeded fractal value noise.
use crate::grid::HeightGrid;
use rand_core::{Rng, SeedableRng};
use rand_pcg::Pcg32;

/// World generation parameters.
#[derive(Clone, Debug)]
pub struct WorldGen {
    pub seed: u64,
    pub size: usize,
    /// Peak-to-valley height range in world units.
    pub amplitude: f32,
}

impl WorldGen {
    pub const DEFAULT_SIZE: usize = 128;
    pub fn new(seed: u64) -> Self { Self { seed, size: Self::DEFAULT_SIZE, amplitude: 24.0 } }

    /// Generates a `size x size` height grid; identical for identical params.
    pub fn heightmap(&self) -> HeightGrid {
        let n = self.size;
        let mut grid = HeightGrid::new(n, n, 0.0);
        const OCTAVES: u32 = 5;
        let layers: Vec<Lattice> = (0..OCTAVES)
            .map(|o| Lattice::new(self.seed.wrapping_add(o as u64 * 0x9E37_79B9), 4usize << o))
            .collect();
        for y in 0..n {
            for x in 0..n {
                let (u, v) = (x as f32 / n as f32, y as f32 / n as f32);
                let (mut sum, mut amp, mut norm) = (0.0, 1.0, 0.0);
                for l in &layers {
                    sum += amp * l.sample(u, v);
                    norm += amp;
                    amp *= 0.5;
                }
                grid.as_mut_slice()[y * n + x] = sum / norm * self.amplitude;
            }
        }
        grid
    }
}

/// Square lattice of random values in 0..1, sampled with smooth interpolation.
struct Lattice { cells: usize, vals: Vec<f32> }

impl Lattice {
    fn new(seed: u64, cells: usize) -> Self {
        let mut rng = Pcg32::seed_from_u64(seed);
        let side = cells + 1;
        let vals = (0..side * side).map(|_| (rng.next_u32() >> 8) as f32 / (1u32 << 24) as f32).collect();
        Self { cells, vals }
    }

    fn sample(&self, u: f32, v: f32) -> f32 {
        let side = self.cells + 1;
        let (fx, fy) = (u * self.cells as f32, v * self.cells as f32);
        let (x0, y0) = (fx.floor() as usize, fy.floor() as usize);
        let (tx, ty) = (smooth(fx - x0 as f32), smooth(fy - y0 as f32));
        let at = |x: usize, y: usize| self.vals[y.min(self.cells) * side + x.min(self.cells)];
        let top = at(x0, y0) + (at(x0 + 1, y0) - at(x0, y0)) * tx;
        let bot = at(x0, y0 + 1) + (at(x0 + 1, y0 + 1) - at(x0, y0 + 1)) * tx;
        top + (bot - top) * ty
    }
}

fn smooth(t: f32) -> f32 { t * t * (3.0 - 2.0 * t) }

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_seed_same_map() {
        assert_eq!(WorldGen::new(7).heightmap(), WorldGen::new(7).heightmap());
    }

    #[test]
    fn different_seed_differs() {
        assert_ne!(WorldGen::new(7).heightmap(), WorldGen::new(8).heightmap());
    }

    #[test]
    fn heights_within_amplitude() {
        let g = WorldGen::new(1);
        assert!(g.heightmap().as_slice().iter().all(|h| (0.0..=g.amplitude).contains(h)));
    }
}
