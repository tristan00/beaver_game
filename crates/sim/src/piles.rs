//! Goods left on the ground. With no store to take them, gatherers drop what they gather
//! where they are; builders pick it up for their work and hungry beavers come to eat it.
//! A pile rots away `SPOIL_DAYS` after it was dropped.
include!(concat!(env!("OUT_DIR"), "/piles_constants.rs"));
use crate::buildings::Good;
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

    /// Leave goods on a cell, on top of what is already lying there.
    pub fn drop_at(&mut self, cell: Cell, good: Good, amount: f32, now: f64) {
        if amount <= 0.0 {
            return;
        }
        if let Some(p) = self.items.iter_mut().find(|p| p.cell == cell && p.good == good) {
            p.day += (now - p.day) * (amount / (p.amount + amount)) as f64;
            p.amount += amount;
            return;
        }
        self.next_id += 1;
        self.items.push(Pile { id: self.next_id, cell, good, amount, day: now });
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

    /// Piles older than `SPOIL_DAYS` rot away.
    pub fn spoil(&mut self, now: f64) {
        self.items.retain(|p| p.days_left(now) > 0.0);
    }
}

/// How much of a good lies on the ground.
pub fn lying(sim: &SimState, good: Good) -> f32 {
    sim.work.piles.items.iter().filter(|p| p.good == good).map(|p| p.amount).sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drops_merge_take_empties_and_old_piles_rot() {
        let mut p = Piles::default();
        p.drop_at((3, 3), Good::Sticks, 2.0, 0.0);
        p.drop_at((3, 3), Good::Sticks, 2.0, 10.0);
        p.drop_at((3, 3), Good::Food, 1.0, 10.0);
        assert_eq!(p.items.len(), 2, "same cell and good merge; another good is its own pile");
        let sticks = p.items[0].clone();
        assert_eq!(sticks.amount, 4.0);
        assert!((sticks.day - 5.0).abs() < 1e-9, "a top-up moves the age on in proportion");
        assert_eq!(p.take(sticks.id, 3.0), 3.0);
        assert_eq!(p.take(sticks.id, 3.0), 1.0);
        assert!(p.get(sticks.id).is_none(), "an emptied pile is gone");
        p.spoil(10.0 + SPOIL_DAYS - 0.1);
        assert_eq!(p.items.len(), 1);
        p.spoil(10.0 + SPOIL_DAYS);
        assert!(p.items.is_empty(), "food dropped on day 10 has rotted by day {}", 10.0 + SPOIL_DAYS);
    }
}
