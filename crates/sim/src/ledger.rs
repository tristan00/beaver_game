include!(concat!(env!("OUT_DIR"), "/ledger_constants.rs"));
use crate::buildings::{Good, Producer, Stores, Use};
use std::collections::{BTreeMap, VecDeque};

const N: usize = Good::ALL.len();

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct DaySample {
    pub day: u64,
    pub produced: [f32; N],
    pub consumed: [f32; N],
    pub stock: [f32; N],
    pub produced_by: Vec<(Producer, Good, f32)>,
    pub consumed_by: [[f32; N]; Use::COUNT],
}

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
struct Open {
    day: u64,
    produced: [f32; N],
    consumed: [f32; N],
    produced_by: BTreeMap<(Producer, Good), f32>,
    consumed_by: [[f32; N]; Use::COUNT],
}

impl Open {
    fn at(day: u64, s: &Stores) -> Self {
        Open { day, produced: s.produced_total, consumed: s.consumed_total, produced_by: s.produced_by.clone(), consumed_by: s.consumed_by }
    }
}

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct Ledger {
    open: Option<Open>,
    pub days: VecDeque<DaySample>,
}

impl Ledger {
    pub fn record(&mut self, time_days: f64, stores: &Stores) {
        let today = time_days.max(0.0).floor() as u64;
        let Some(open) = &self.open else {
            self.open = Some(Open::at(today, stores));
            return;
        };
        if today <= open.day {
            return;
        }
        let day = open.day;
        let span = (today - day) as f32;
        let stock: [f32; N] = std::array::from_fn(|i| stores.amount(Good::ALL[i]));
        let produced: [f32; N] = std::array::from_fn(|i| (stores.produced_total[i] - open.produced[i]).max(0.0) / span);
        let consumed: [f32; N] = std::array::from_fn(|i| (stores.consumed_total[i] - open.consumed[i]).max(0.0) / span);
        let produced_by: Vec<(Producer, Good, f32)> = stores
            .produced_by
            .iter()
            .map(|(&(p, g), &total)| (p, g, (total - open.produced_by.get(&(p, g)).copied().unwrap_or(0.0)).max(0.0) / span))
            .filter(|e| e.2 > 0.0)
            .collect();
        let consumed_by: [[f32; N]; Use::COUNT] =
            std::array::from_fn(|u| std::array::from_fn(|i| (stores.consumed_by[u][i] - open.consumed_by[u][i]).max(0.0) / span));
        for d in day..today {
            self.days.push_back(DaySample { day: d, produced, consumed, stock, produced_by: produced_by.clone(), consumed_by });
            if self.days.len() > HISTORY_DAYS {
                self.days.pop_front();
            }
        }
        self.open = Some(Open::at(today, stores));
    }

    pub fn in_by(&self, g: Good) -> Vec<(Producer, f32)> {
        let n = self.days.len().min(RATE_DAYS);
        let mut by: BTreeMap<Producer, f32> = BTreeMap::new();
        for d in self.days.iter().rev().take(n) {
            for &(p, pg, a) in &d.produced_by {
                if pg == g {
                    *by.entry(p).or_insert(0.0) += a / n as f32;
                }
            }
        }
        let mut v: Vec<(Producer, f32)> = by.into_iter().collect();
        v.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.cmp(&b.0)));
        v
    }

    pub fn out_for(&self, g: Good, use_: Use) -> f32 {
        self.mean(|d| d.consumed_by[use_.index()][g.index()])
    }

    fn mean(&self, f: impl Fn(&DaySample) -> f32) -> f32 {
        let n = self.days.len().min(RATE_DAYS);
        if n == 0 {
            return 0.0;
        }
        self.days.iter().rev().take(n).map(f).sum::<f32>() / n as f32
    }

    pub fn in_per_day(&self, g: Good) -> f32 {
        self.mean(|d| d.produced[g.index()])
    }

    pub fn out_per_day(&self, g: Good) -> f32 {
        self.mean(|d| d.consumed[g.index()])
    }

    pub fn net_per_day(&self, g: Good) -> f32 {
        self.in_per_day(g) - self.out_per_day(g)
    }

    pub fn days_left(&self, g: Good, stock: f32) -> Option<f32> {
        let net = self.net_per_day(g);
        (net < -1e-3).then(|| (stock.max(0.0) / -net).max(0.0))
    }

    pub fn has_data(&self) -> bool {
        !self.days.is_empty()
    }
}

