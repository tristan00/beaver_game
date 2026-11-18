//! Daily goods ledger: what came into and went out of the stores each whole day, per good, over a
//! rolling window. The HUD's "net per day" and "lasts N days" come from here, so they average over
//! days instead of flickering with each sim step.
use crate::buildings::{Good, Stores};
use std::collections::VecDeque;

const N: usize = Good::ALL.len();
/// Days of history kept (graphs and averages).
pub const HISTORY_DAYS: usize = 60;
/// Days averaged for the rates the HUD shows.
pub const RATE_DAYS: usize = 7;

/// One finished day.
#[derive(Clone, Debug, PartialEq)]
pub struct DaySample {
    /// Whole day number (`floor(time_days)`) this sample covers.
    pub day: u64,
    pub produced: [f32; N],
    pub consumed: [f32; N],
    /// Stock at the end of the day.
    pub stock: [f32; N],
}

#[derive(Clone, Debug, Default)]
pub struct Ledger {
    /// The day currently being accumulated, with the stores' lifetime totals at its start.
    open: Option<(u64, [f32; N], [f32; N])>,
    /// Finished days, oldest first, at most [`HISTORY_DAYS`].
    pub days: VecDeque<DaySample>,
}

impl Ledger {
    /// Call after each sim step. Closes every whole day that ended since the last call; a step that
    /// spans several days spreads its flow evenly over them.
    pub fn record(&mut self, time_days: f64, stores: &Stores) {
        let today = time_days.max(0.0).floor() as u64;
        let Some((day, p0, c0)) = self.open else {
            self.open = Some((today, stores.produced_total, stores.consumed_total));
            return;
        };
        if today <= day {
            return;
        }
        let span = (today - day) as f32;
        let stock: [f32; N] = std::array::from_fn(|i| stores.amount(Good::ALL[i]));
        let produced: [f32; N] = std::array::from_fn(|i| (stores.produced_total[i] - p0[i]).max(0.0) / span);
        let consumed: [f32; N] = std::array::from_fn(|i| (stores.consumed_total[i] - c0[i]).max(0.0) / span);
        for d in day..today {
            self.days.push_back(DaySample { day: d, produced, consumed, stock });
            if self.days.len() > HISTORY_DAYS {
                self.days.pop_front();
            }
        }
        self.open = Some((today, stores.produced_total, stores.consumed_total));
    }

    fn mean(&self, f: impl Fn(&DaySample) -> f32) -> f32 {
        let n = self.days.len().min(RATE_DAYS);
        if n == 0 {
            return 0.0;
        }
        self.days.iter().rev().take(n).map(f).sum::<f32>() / n as f32
    }

    /// Mean daily amount added to the stores over the last [`RATE_DAYS`] finished days.
    pub fn in_per_day(&self, g: Good) -> f32 {
        self.mean(|d| d.produced[g.index()])
    }

    /// Mean daily amount taken from the stores over the last [`RATE_DAYS`] finished days.
    pub fn out_per_day(&self, g: Good) -> f32 {
        self.mean(|d| d.consumed[g.index()])
    }

    pub fn net_per_day(&self, g: Good) -> f32 {
        self.in_per_day(g) - self.out_per_day(g)
    }

    /// Days `stock` lasts at the current net rate, or `None` when the good is not falling.
    pub fn days_left(&self, g: Good, stock: f32) -> Option<f32> {
        let net = self.net_per_day(g);
        (net < -1e-3).then(|| (stock.max(0.0) / -net).max(0.0))
    }

    /// Whether at least one whole day has been recorded (rates mean something).
    pub fn has_data(&self) -> bool {
        !self.days.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stores_with(food: f32) -> Stores {
        let mut s = Stores::default();
        s.deposit(Good::Food, food).unwrap();
        s
    }

    #[test]
    fn nothing_until_a_day_closes() {
        let mut l = Ledger::default();
        let s = stores_with(10.0);
        l.record(0.0, &s);
        l.record(0.9, &s);
        assert!(!l.has_data());
        assert_eq!(l.net_per_day(Good::Food), 0.0);
    }

    #[test]
    fn rates_average_whole_days() {
        let mut l = Ledger::default();
        let mut s = stores_with(50.0);
        l.record(0.0, &s);
        for day in 1..=3 {
            s.deposit(Good::Food, 6.0).unwrap();
            s.withdraw(Good::Food, 10.0).unwrap();
            l.record(day as f64, &s);
        }
        assert_eq!(l.days.len(), 3);
        assert!((l.in_per_day(Good::Food) - 6.0).abs() < 1e-4);
        assert!((l.out_per_day(Good::Food) - 10.0).abs() < 1e-4);
        assert!((l.net_per_day(Good::Food) + 4.0).abs() < 1e-4);
        let left = l.days_left(Good::Food, 38.0).unwrap();
        assert!((left - 9.5).abs() < 1e-3, "{left}");
        assert_eq!(l.days_left(Good::Logs, 10.0), None);
    }

    #[test]
    fn a_long_step_spreads_over_its_days_and_history_is_capped() {
        let mut l = Ledger::default();
        let mut s = stores_with(0.0);
        l.record(0.0, &s);
        s.deposit(Good::Logs, 20.0).unwrap();
        l.record(4.0, &s);
        assert_eq!(l.days.len(), 4);
        assert!(l.days.iter().all(|d| (d.produced[Good::Logs.index()] - 5.0).abs() < 1e-4));
        l.record(4.0 + HISTORY_DAYS as f64 * 2.0, &s);
        assert_eq!(l.days.len(), HISTORY_DAYS);
    }
}
