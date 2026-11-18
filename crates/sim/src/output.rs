//! What each building actually produced: gathering by its workers and crafting at its benches,
//! as a smoothed per-day rate. The inspector's "why this rate" reads it next to the factors that
//! limit the building, so the explanation rests on measured output, not an estimate.
use crate::buildings::{BuildingId, Good};
use std::collections::BTreeMap;

/// Days the per-day rate averages over (exponential smoothing time constant).
pub const SMOOTH_DAYS: f32 = 5.0;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Flow {
    /// The good last produced (a logging camp makes logs and sticks; the latest one is shown).
    pub good: Option<Good>,
    /// Smoothed amount per day.
    pub per_day: f32,
    pub total: f32,
    /// Days since the building started producing (rates are shaky before `SMOOTH_DAYS`).
    pub age_days: f32,
    pending: f32,
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
        f.good = Some(good);
        f.pending += amount;
        f.total += amount;
    }

    /// Ends a step of `dt` days: folds what was booked into each building's smoothed rate.
    pub fn step(&mut self, dt: f32) {
        if !(dt.is_finite() && dt > 0.0) {
            return;
        }
        let k = 1.0 - (-dt / SMOOTH_DAYS).exp();
        for f in self.by.values_mut() {
            f.per_day += (f.pending / dt - f.per_day) * k;
            f.pending = 0.0;
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
    fn steady_output_settles_on_its_daily_rate() {
        let mut o = Output::default();
        let b = BuildingId(7);
        for _ in 0..1000 {
            o.record(b, Good::Logs, 0.3); // 6 per day in 0.05-day steps
            o.step(0.05);
        }
        let f = o.of(b).unwrap();
        assert!((f.per_day - 6.0).abs() < 0.05, "{}", f.per_day);
        assert!((f.total - 300.0).abs() < 1e-2);
        assert_eq!(f.good, Some(Good::Logs));
    }

    #[test]
    fn output_decays_when_work_stops_and_bad_amounts_are_ignored() {
        let mut o = Output::default();
        let b = BuildingId(1);
        o.record(b, Good::Food, 5.0);
        o.record(b, Good::Food, f32::NAN);
        o.step(1.0);
        let first = o.of(b).unwrap().per_day;
        for _ in 0..20 {
            o.step(1.0);
        }
        assert!(o.of(b).unwrap().per_day < first * 0.05);
        o.forget(b);
        assert!(o.of(b).is_none());
    }
}
