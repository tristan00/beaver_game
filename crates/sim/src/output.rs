include!(concat!(env!("OUT_DIR"), "/output_constants.rs"));
use crate::buildings::{BuildingId, Good, PerId};
use std::collections::BTreeMap;

#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Flow {
    pub per_day: PerId<Good, f32>,
    pub total: PerId<Good, f32>,
    pub age_days: f32,
    pending: PerId<Good, f32>,
}

impl Flow {
    pub fn rates(&self) -> Vec<(Good, f32)> {
        let mut v: Vec<(Good, f32)> = Good::all().map(|g| (g, self.per_day[g])).filter(|(_, r)| *r > 0.01).collect();
        v.sort_by(|a, b| b.1.total_cmp(&a.1));
        v
    }

}

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct Output {
    pub by: BTreeMap<BuildingId, Flow>,
}

impl Output {
    pub fn record(&mut self, b: BuildingId, good: Good, amount: f32) {
        if !(amount.is_finite() && amount > 0.0) {
            return;
        }
        let f = self.by.entry(b).or_default();
        f.pending[good] += amount;
        f.total[good] += amount;
    }

    pub fn step(&mut self, dt: f32) {
        if !(dt.is_finite() && dt > 0.0) {
            return;
        }
        let k = 1.0 - (-dt / SMOOTH_DAYS).exp();
        for f in self.by.values_mut() {
            for g in Good::all() {
                f.per_day[g] += (f.pending[g] / dt - f.per_day[g]) * k;
                f.pending[g] = 0.0;
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
