include!(concat!(env!("OUT_DIR"), "/output_constants.rs"));
use crate::buildings::{BuildingId, Good};
use std::collections::BTreeMap;


const N: usize = Good::ALL.len();

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Flow {
    pub per_day: [f32; N],
    pub total: [f32; N],
    pub age_days: f32,
    pending: [f32; N],
}

impl Flow {
    pub fn rates(&self) -> Vec<(Good, f32)> {
        let mut v: Vec<(Good, f32)> = Good::ALL.iter().map(|g| (*g, self.per_day[g.index()])).filter(|(_, r)| *r > 0.01).collect();
        v.sort_by(|a, b| b.1.total_cmp(&a.1));
        v
    }

}

#[derive(Clone, Debug, Default)]
pub struct Output {
    pub by: BTreeMap<BuildingId, Flow>,
}

impl Output {
    pub fn record(&mut self, b: BuildingId, good: Good, amount: f32) {
        if !(amount.is_finite() && amount > 0.0) {
            return;
        }
        let f = self.by.entry(b).or_default();
        f.pending[good.index()] += amount;
        f.total[good.index()] += amount;
    }

    pub fn step(&mut self, dt: f32) {
        if !(dt.is_finite() && dt > 0.0) {
            return;
        }
        let k = 1.0 - (-dt / SMOOTH_DAYS).exp();
        for f in self.by.values_mut() {
            for i in 0..N {
                f.per_day[i] += (f.pending[i] / dt - f.per_day[i]) * k;
                f.pending[i] = 0.0;
            }
            f.age_days += dt;
        }
    }

    pub fn of(&self, b: BuildingId) -> Option<&Flow> {
        self.by.get(&b)
    }

    pub fn forget(&mut self, b: BuildingId) {
        self.by.remove(&b);
    }
}

