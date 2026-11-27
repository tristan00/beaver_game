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
    pub day: f64,
    #[serde(default)] pub booked: bool,
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

    pub fn drop_at(&mut self, cell: Cell, good: Good, amount: f32, now: f64) {
        self.drop_load(cell, good, amount, now, false, None);
    }

    pub fn take(&mut self, id: u64, amount: f32) -> f32 {
        let Some(i) = self.items.iter().position(|p| p.id == id) else { return 0.0 };
        let taken = amount.min(self.items[i].amount).max(0.0);
        self.items[i].amount -= taken;
        if self.items[i].amount <= 1e-4 {
            self.items.remove(i);
        }
        taken
    }

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

    pub fn spoil(&mut self, now: f64) {
        self.items.retain(|p| p.days_left(now) > 0.0);
    }
}

pub fn lying(sim: &SimState, good: Good) -> f32 {
    sim.work.piles.items.iter().filter(|p| p.good == good).map(|p| p.amount).sum()
}
