//! Ambient arrivals. Player-facing immigration requests live in `society`.
include!(concat!(env!("OUT_DIR"), "/events_constants.rs"));
use crate::{colony::Colony, water::Season};
use rand_core::{Rng, SeedableRng};
use rand_pcg::Pcg32;

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
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
                crate::trace::event(format!("WANDERERS_ARRIVE day={:.4} count={count} set_off={day:.4} food_cost={amount:.1}", self.time_days));
                stores.withdraw_for(crate::buildings::Good::Food, amount, crate::buildings::Use::Other).unwrap();
                false
            });
            let season = Season::at(self.time_days);
            if season != Season::Winter && self.unit() < WANDERER_CHANCE_PER_DAY * sub as f32 {
                let count = 1 + (self.unit() * WANDERER_COUNT_SPREAD) as u32;
                self.wanderers.push((self.time_days, count));
                crate::trace::event(format!("WANDERERS_SET_OFF day={:.4} count={count} arrive_in_days={WANDERER_TRAVEL_DAYS}", self.time_days));
            }
            if wetland > FOOD_EVENT_WETLAND_MIN && matches!(season, Season::Spring | Season::Summer)
                && self.unit() < FOOD_EVENT_CHANCE_PER_DAY * wetland * sub as f32
            {
                let amount = (FOOD_EVENT_BASE + FOOD_EVENT_SPREAD * self.unit()) * (1.0 + wetland);
                let got = stores.deposit(crate::buildings::Good::Food, amount).unwrap();
                crate::trace::event(format!("FOOD_WINDFALL day={:.4} offered={amount:.1} stored={got:.1} wetland={wetland:.2}", self.time_days));
            }
        }
    }

    fn unit(&mut self) -> f32 {
        (self.rng.next_u32() >> 8) as f32 / (1u32 << 24) as f32
    }
}
