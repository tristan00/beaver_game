//! Goods left on the ground. With no store to take them, gatherers drop what they gather
//! where they are; builders pick it up for their work and hungry beavers eat from them when stores run dry.
//! A pile rots away `SPOIL_DAYS` after it was dropped.
include!(concat!(env!("OUT_DIR"), "/piles_constants.rs"));
use crate::buildings::Good;
use crate::inventory::Species;
use crate::water::Cell;
use crate::SimState;

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Pile {
    pub id: u64,
    pub cell: Cell,
    pub good: Good,
    pub amount: f32,
    /// The day it was dropped; goods added later move it on in proportion to how much they add.
    pub day: f64,
    /// Whether this good was already counted as colony production (refunded/wrecked/hauled).
    #[serde(default)] pub booked: bool,
    /// Preserved seedling species, if applicable.
    #[serde(default)] pub species: Option<Species>,
}

impl Pile {
    pub fn days_left(&self, now: f64) -> f64 {
        (self.day + SPOIL_DAYS - now).max(0.0)
    }
}

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct Piles {
    pub items: Vec<Pile>,
    next_id: u64,
}

impl Piles {
    pub fn get(&self, id: u64) -> Option<&Pile> {
        self.items.iter().find(|p| p.id == id)
    }

    /// Leave goods on a cell, on top of what is already lying there (preserving `booked` and `species`).
    pub fn drop_load(&mut self, cell: Cell, good: Good, amount: f32, now: f64, booked: bool, species: Option<Species>) {
        if amount <= 0.0 {
            return;
        }
        if let Some(p) = self.items.iter_mut().find(|p| p.cell == cell && p.good == good && p.booked == booked && p.species == species) {
            p.day += (now - p.day) * (amount / (p.amount + amount)) as f64;
            p.amount += amount;
            return;
        }
        self.next_id += 1;
        self.items.push(Pile { id: self.next_id, cell, good, amount, day: now, booked, species });
    }

    /// Leave unbooked goods on a cell, on top of what is already lying there.
    pub fn drop_at(&mut self, cell: Cell, good: Good, amount: f32, now: f64) {
        self.drop_load(cell, good, amount, now, false, None);
    }

    /// Take up to `amount` from a pile; an emptied pile is gone.
    pub fn take(&mut self, id: u64, amount: f32) -> f32 {
        let Some(i) = self.items.iter().position(|p| p.id == id) else { return 0.0 };
        let taken = amount.min(self.items[i].amount).max(0.0);
        self.items[i].amount -= taken;
        if self.items[i].amount <= 1e-4 {
            self.items.remove(i);
        }
        taken
    }

    /// Take up to `amount` of `good` across any piles lying on the ground.
    /// Returns `(booked, unbooked)` amounts taken so callers can book ledger usage accurately.
    pub fn take_good(&mut self, good: Good, mut want: f32) -> (f32, f32) {
        let (mut booked, mut unbooked) = (0.0, 0.0);
        for p in &mut self.items {
            if p.good != good || want <= 1e-4 { continue; }
            let take = want.min(p.amount).max(0.0);
            p.amount -= take;
            want -= take;
            if p.booked { booked += take; } else { unbooked += take; }
        }
        self.items.retain(|p| p.amount > 1e-4);
        (booked, unbooked)
    }

    /// Piles older than `SPOIL_DAYS` rot away.
    pub fn spoil(&mut self, now: f64) {
        self.items.retain(|p| p.days_left(now) > 0.0);
    }
}

/// How much of a good lies on the ground.
pub fn lying(sim: &SimState, good: Good) -> f32 {
    sim.work.piles.items.iter().filter(|p| p.good == good).map(|p| p.amount).sum()
}
