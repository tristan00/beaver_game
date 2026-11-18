//! What each building actually produced: gathering by its workers and crafting at its benches,
//! as a smoothed per-day rate for each good. The inspector's "why this rate" reads it next to the
//! factors that limit the building, so the explanation rests on measured output, not an estimate.
use crate::buildings::{BuildingId, Good};
use std::collections::BTreeMap;

/// Days the per-day rate averages over (exponential smoothing time constant).
pub const SMOOTH_DAYS: f32 = 5.0;

const N: usize = Good::ALL.len();

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Flow {
    /// Smoothed amount per day, per good (a logging camp makes logs and sticks).
    pub per_day: [f32; N],
    pub total: [f32; N],
    /// Days since the building started producing (rates are shaky before `SMOOTH_DAYS`).
    pub age_days: f32,
    pending: [f32; N],
}

impl Flow {
    /// Goods made at a meaningful rate, largest first.
    pub fn rates(&self) -> Vec<(Good, f32)> {
        let mut v: Vec<(Good, f32)> = Good::ALL.iter().map(|g| (*g, self.per_day[g.index()])).filter(|(_, r)| *r > 0.01).collect();
        v.sort_by(|a, b| b.1.total_cmp(&a.1));
        v
    }

    /// All goods together, per day.
    pub fn sum_per_day(&self) -> f32 {
        self.per_day.iter().sum()
    }
}

#[derive(Clone, Debug, Default)]
pub struct Output {
    pub by: BTreeMap<BuildingId, Flow>,
}

impl Output {
    /// Books `amount` of `good` made by building `b` during the current step.
    pub fn record(&mut self, b: BuildingId, good: Good, amount: f32) {
        if !(amount.is_finite() && amount > 0.0) {
            return;
        }
        let f = self.by.entry(b).or_default();
        f.pending[good.index()] += amount;
        f.total[good.index()] += amount;
    }

    /// Ends a step of `dt` days: folds what was booked into each building's smoothed rates.
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

    /// Forgets a building that is gone.
    pub fn forget(&mut self, b: BuildingId) {
        self.by.remove(&b);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn steady_output_settles_on_its_daily_rate_per_good() {
        let mut o = Output::default();
        let b = BuildingId(7);
        for _ in 0..1000 {
            o.record(b, Good::Logs, 0.3); // 6 logs per day in 0.05-day steps
            o.record(b, Good::Sticks, 0.1); // 2 sticks per day
            o.step(0.05);
        }
        let f = o.of(b).unwrap();
        assert!((f.per_day[Good::Logs.index()] - 6.0).abs() < 0.05, "{:?}", f.per_day);
        assert!((f.total[Good::Logs.index()] - 300.0).abs() < 1e-2);
        let r = f.rates();
        assert_eq!(r.iter().map(|x| x.0).collect::<Vec<_>>(), vec![Good::Logs, Good::Sticks]);
        assert!((f.sum_per_day() - 8.0).abs() < 0.1);
    }

    #[test]
    fn output_decays_when_work_stops_and_bad_amounts_are_ignored() {
        let mut o = Output::default();
        let b = BuildingId(1);
        o.record(b, Good::Food, 5.0);
        o.record(b, Good::Food, f32::NAN);
        o.step(1.0);
        let first = o.of(b).unwrap().sum_per_day();
        for _ in 0..20 {
            o.step(1.0);
        }
        assert!(o.of(b).unwrap().sum_per_day() < first * 0.05);
        o.forget(b);
        assert!(o.of(b).is_none());
    }
}
