//! Ambient arrivals. Player-facing immigration requests live in `society`.
include!(concat!(env!("OUT_DIR"), "/events_constants.rs"));
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

    pub fn step(&mut self, dt_days: f32, colony: &mut Colony, stores: &mut crate::buildings::Stores, wetland: f32) {
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
                if self.time_days - day < WANDERER_TRAVEL_DAYS {
                    return true;
                }
                colony.add_immigrants(count);
                let amount = (ARRIVAL_FOOD_COST * count as f32).min(stores.amount(crate::buildings::Good::Food));
                stores.withdraw_for(crate::buildings::Good::Food, amount, crate::buildings::Use::Other).unwrap();
                false
            });
            let season = Season::at(self.time_days);
            if season != Season::Winter && self.unit() < WANDERER_CHANCE_PER_DAY * sub as f32 {
                let count = 1 + (self.unit() * WANDERER_COUNT_SPREAD) as u32;
                self.wanderers.push((self.time_days, count));
            }
            if wetland > FOOD_EVENT_WETLAND_MIN && matches!(season, Season::Spring | Season::Summer)
                && self.unit() < FOOD_EVENT_CHANCE_PER_DAY * wetland * sub as f32
            {
                stores.deposit(crate::buildings::Good::Food, (FOOD_EVENT_BASE + FOOD_EVENT_SPREAD * self.unit()) * (1.0 + wetland)).unwrap();
            }
        }
    }

    fn unit(&mut self) -> f32 {
        (self.rng.next_u32() >> 8) as f32 / (1u32 << 24) as f32
    }
}
