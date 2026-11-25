include!(concat!(env!("OUT_DIR"), "/ledger_constants.rs"));
use crate::buildings::{Good, PerId, Producer, Stores, Use};
use std::collections::{BTreeMap, VecDeque};

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct DaySample {
    pub day: u64,
    pub produced: PerId<Good, f32>,
    pub consumed: PerId<Good, f32>,
    pub stock: PerId<Good, f32>,
    pub produced_by: Vec<(Producer, Good, f32)>,
    pub consumed_by: [PerId<Good, f32>; Use::COUNT],
}

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
struct Open {
    day: u64,
    produced: PerId<Good, f32>,
    consumed: PerId<Good, f32>,
    produced_by: BTreeMap<(Producer, Good), f32>,
    consumed_by: [PerId<Good, f32>; Use::COUNT],
}

impl Open {
    fn at(day: u64, s: &Stores) -> Self {
        Open { day, produced: s.produced_total.clone(), consumed: s.consumed_total.clone(), produced_by: s.produced_by.clone(), consumed_by: s.consumed_by.clone() }
    }
}

/// How many days the colony's own record keeps: four years.
pub const COLONY_DAYS: usize = 4 * crate::calendar::YEAR_DAYS as usize;

/// The colony counted at the close of a day.
#[derive(Clone, Copy, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Census {
    pub beavers: u16,
    /// Kits and yearlings among them.
    pub young: u16,
    /// Water held in dam ponds (m³).
    pub water: f32,
    /// Living trees standing.
    pub trees: u32,
    /// Saplings planted since the colony began.
    pub planted: u32,
}

/// One day of the colony's long record: its census and each good's flow and stock.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ColonyDay {
    pub day: u64,
    pub census: Census,
    pub produced: PerId<Good, f32>,
    pub consumed: PerId<Good, f32>,
    pub stock: PerId<Good, f32>,
}

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct Ledger {
    open: Option<Open>,
    pub days: VecDeque<DaySample>,
    /// The long record, a day at a time, for the colony ledger's charts.
    pub colony: VecDeque<ColonyDay>,
    /// Saplings planted so far.
    pub planted: u32,
}

impl Ledger {
    /// Whether the next `record` at this time closes a day (and so asks for a census).
    pub fn closes_day(&self, time_days: f64) -> bool {
        let today = time_days.max(0.0).floor() as u64;
        self.open.as_ref().is_some_and(|o| today > o.day)
    }

    pub fn record(&mut self, time_days: f64, stores: &Stores, census: impl FnOnce() -> Census) {
        let today = time_days.max(0.0).floor() as u64;
        let Some(open) = &self.open else {
            self.open = Some(Open::at(today, stores));
            return;
        };
        if today <= open.day {
            return;
        }
        let census = census();
        let day = open.day;
        let span = (today - day) as f32;
        let stock: PerId<Good, f32> = PerId::from_fn(|g| stores.amount(g));
        let produced: PerId<Good, f32> = PerId::from_fn(|g| (stores.produced_total[g] - open.produced[g]).max(0.0) / span);
        let consumed: PerId<Good, f32> = PerId::from_fn(|g| (stores.consumed_total[g] - open.consumed[g]).max(0.0) / span);
        let produced_by: Vec<(Producer, Good, f32)> = stores
            .produced_by
            .iter()
            .map(|(&(p, g), &total)| (p, g, (total - open.produced_by.get(&(p, g)).copied().unwrap_or(0.0)).max(0.0) / span))
            .filter(|e| e.2 > 0.0)
            .collect();
        let consumed_by: [PerId<Good, f32>; Use::COUNT] =
            std::array::from_fn(|u| PerId::from_fn(|g| (stores.consumed_by[u][g] - open.consumed_by[u][g]).max(0.0) / span));
        for d in day..today {
            self.days.push_back(DaySample { day: d, produced: produced.clone(), consumed: consumed.clone(), stock: stock.clone(), produced_by: produced_by.clone(), consumed_by: consumed_by.clone() });
            if self.days.len() > HISTORY_DAYS {
                self.days.pop_front();
            }
            self.colony.push_back(ColonyDay { day: d, census, produced: produced.clone(), consumed: consumed.clone(), stock: stock.clone() });
            if self.colony.len() > COLONY_DAYS {
                self.colony.pop_front();
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
        self.mean(|d| d.consumed_by[use_.index()][g])
    }

    fn mean(&self, f: impl Fn(&DaySample) -> f32) -> f32 {
        let n = self.days.len().min(RATE_DAYS);
        if n == 0 {
            return 0.0;
        }
        self.days.iter().rev().take(n).map(f).sum::<f32>() / n as f32
    }

    pub fn in_per_day(&self, g: Good) -> f32 {
        self.mean(|d| d.produced[g])
    }

    pub fn out_per_day(&self, g: Good) -> f32 {
        self.mean(|d| d.consumed[g])
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
