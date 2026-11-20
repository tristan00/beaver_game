//! Ambient arrivals. Player-facing immigration requests live in `society`.
use crate::{colony::Colony, water::Season};
use rand_core::{Rng, SeedableRng};
use rand_pcg::Pcg32;

#[derive(Clone, Debug)]
pub struct Events {
    rng: Pcg32,
    time_days: f64,
    wanderers: Vec<(f64, u32)>,
}

impl Events {
    pub fn new(seed: u64) -> Self {
        Self {
            rng: Pcg32::seed_from_u64(seed ^ 0xE7E1_7500),
            time_days: 0.0,
            wanderers: Vec::new(),
        }
    }

    pub fn step(&mut self, dt_days: f32, colony: &mut Colony, wetland: f32) {
        if !colony.is_founded() {
            self.time_days += dt_days as f64;
            return;
        }
        let mut left = dt_days as f64;
        let wetland = wetland.clamp(0.0, 1.0);
        while left > 0.0 {
            let sub = left.min(1.0);
            left -= sub;
            self.time_days += sub;
            self.wanderers.retain(|&(day, count)| {
                if self.time_days - day < 5.0 {
                    return true;
                }
                colony.add_immigrants(count);
                colony.food = (colony.food - 2.0 * count as f32).max(0.0);
                false
            });
            let season = Season::at(self.time_days);
            if season != Season::Winter && self.unit() < 0.03 * sub as f32 {
                let count = 1 + (self.unit() * 2.0) as u32;
                self.wanderers.push((self.time_days, count));
            }
            if wetland > 0.1 && matches!(season, Season::Spring | Season::Summer)
                && self.unit() < 0.03 * wetland * sub as f32
            {
                colony.food += (3.0 + 5.0 * self.unit()) * (1.0 + wetland);
            }
        }
    }

    fn unit(&mut self) -> f32 {
        (self.rng.next_u32() >> 8) as f32 / (1u32 << 24) as f32
    }
}
