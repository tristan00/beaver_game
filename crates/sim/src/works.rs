//! What each structure has cost: the work and goods put into building it and into keeping it up.
//! Each has a book, kept day by day over the last season and season by season over its life; the
//! colony's own book adds up every structure's.
use std::collections::{BTreeMap, VecDeque};

use crate::buildings::{BuildingId, Good, PerId};
use crate::SimState;

/// What the work or goods went into.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Spend {
    /// Building it, or making it bigger.
    Build,
    /// Keeping it up: repairs, re-packing worn dam cells, clearing silt, refits.
    Upkeep,
}

/// Work and goods spent, each split by what they went into.
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Tally {
    labour: [f32; 2],
    goods: [PerId<Good, f32>; 2],
}

impl Tally {
    /// Beaver-days of work: a beaver at full pace does a day's work in one.
    pub fn work_days(&self, s: Spend) -> f32 {
        self.labour[s as usize] / crate::work::LABOUR_PER_DAY
    }
    pub fn goods(&self, s: Spend, g: Good) -> f32 {
        self.goods[s as usize][g]
    }
    /// The goods spent, each with its amount, in content order.
    pub fn goods_spent(&self, s: Spend) -> Vec<(Good, f32)> {
        Good::all().map(|g| (g, self.goods[s as usize][g])).filter(|g| g.1 > 0.0).collect()
    }
    pub fn is_empty(&self) -> bool {
        self.labour.iter().all(|l| *l <= 0.0) && self.goods.iter().all(|g| g.values().all(|v| *v <= 0.0))
    }
    fn note(&mut self, s: Spend, labour: f32, goods: Option<(Good, f32)>) {
        self.labour[s as usize] += labour;
        if let Some((g, a)) = goods {
            self.goods[s as usize][g] += a;
        }
    }
    pub fn add(&mut self, other: &Tally) {
        for i in 0..2 {
            self.labour[i] += other.labour[i];
            for g in Good::all() {
                self.goods[i][g] += other.goods[i][g];
            }
        }
    }
}

/// One structure's costs (or the colony's).
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Book {
    /// Each day with any spending in the last season, oldest first.
    days: VecDeque<(u64, Tally)>,
    /// Each season with any spending, oldest first (season 0 is the first of the game).
    seasons: Vec<(u64, Tally)>,
    life: Tally,
}

/// Days in the trailing season the books report.
pub const SEASON: u64 = crate::calendar::SEASON_DAYS as u64;

impl Book {
    fn note(&mut self, day: u64, s: Spend, labour: f32, goods: Option<(Good, f32)>) {
        if self.days.back().is_none_or(|d| d.0 != day) {
            self.days.push_back((day, Tally::default()));
        }
        while self.days.front().is_some_and(|d| d.0 + SEASON <= day) {
            self.days.pop_front();
        }
        self.days.back_mut().expect("just pushed").1.note(s, labour, goods);
        let season = day / SEASON;
        if self.seasons.last().is_none_or(|x| x.0 != season) {
            self.seasons.push((season, Tally::default()));
        }
        self.seasons.last_mut().expect("just pushed").1.note(s, labour, goods);
        self.life.note(s, labour, goods);
    }
    /// What it cost over the last season up to `today` (the trailing `SEASON` days).
    pub fn last_season(&self, today: u64) -> Tally {
        let mut t = Tally::default();
        for (_, d) in self.days.iter().filter(|d| d.0 + SEASON > today) {
            t.add(d);
        }
        t
    }
    /// The last season's days, one tally a day (empty on days without spending), oldest first.
    pub fn daily(&self, today: u64) -> Vec<Tally> {
        let first = today.saturating_sub(SEASON - 1);
        let mut out = vec![Tally::default(); (today - first + 1) as usize];
        for (d, t) in self.days.iter().filter(|d| d.0 >= first && d.0 <= today) {
            out[(d - first) as usize] = t.clone();
        }
        out
    }
    /// Every season from its first to the one `today` falls in, oldest first (empty seasons too).
    pub fn by_season(&self, today: u64) -> Vec<(u64, Tally)> {
        let Some(&(first, _)) = self.seasons.first() else { return Vec::new() };
        let now = (today / SEASON).max(first);
        (first..=now).map(|n| (n, self.seasons.iter().find(|s| s.0 == n).map(|s| s.1.clone()).unwrap_or_default())).collect()
    }
    /// Everything it has cost.
    pub fn life(&self) -> &Tally {
        &self.life
    }
}

#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Works {
    books: BTreeMap<BuildingId, Book>,
    /// Every structure's costs added up.
    pub colony: Book,
}

impl Works {
    pub fn book(&self, b: BuildingId) -> Option<&Book> {
        self.books.get(&b)
    }
}

/// Work (in labour units, `work::LABOUR_PER_DAY` a beaver-day) and goods spent on a structure.
pub(crate) fn note(sim: &mut SimState, at: BuildingId, s: Spend, labour: f32, goods: Option<(Good, f32)>) {
    let day = sim.time_days.max(0.0).floor() as u64;
    sim.works.books.entry(at).or_default().note(day, s, labour, goods);
    sim.works.colony.note(day, s, labour, goods);
}

/// Today, as the books count days.
pub fn today(sim: &SimState) -> u64 {
    sim.time_days.max(0.0).floor() as u64
}
