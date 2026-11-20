//! Synchronizes colony needs and families with physical stores, workers and homes.
use crate::{SimState, agents, buildings, colony, grid, jobs, worldgen};

impl SimState {
    pub(super) fn mirror_colony_stores(&mut self) -> Mirror {
        let s = &self.buildings.stores;
        let (food, wood) = (s.amount(buildings::Good::Food), s.amount(buildings::Good::Sticks));
        (self.colony.food, self.colony.wood) = (food, wood);
        let c = &self.colony;
        Mirror { food, wood, eaten: c.eaten, spoiled: c.spoiled, repair_wood: c.repair_wood }
    }

    pub(super) fn settle_colony_stores(&mut self, before: Mirror) {
        use buildings::{Good, Use};
        let m = before;
        let c = &self.colony;
        let stages = [colony::Stage::Kit, colony::Stage::Yearling, colony::Stage::Adult];
        let mut food_uses: Vec<(Use, f32)> = stages.iter().map(|&st| (Use::eaten_by(st), c.eaten[st as usize] - m.eaten[st as usize])).collect();
        food_uses.push((Use::Spoiled, c.spoiled - m.spoiled));
        let wood_uses = vec![(Use::Repairs, c.repair_wood - m.repair_wood)];
        let stores = &mut self.buildings.stores;
        for (good, was, now, uses) in [(Good::Food, m.food, c.food, food_uses), (Good::Sticks, m.wood, c.wood, wood_uses)] {
            let d = now - was;
            if d < 0.0 {
                let take = (-d).min(stores.amount(good));
                let known: f32 = uses.iter().map(|u| u.1.max(0.0)).sum();
                let scale = if known > take { take / known } else { 1.0 };
                let mut left = take;
                for (u, a) in uses {
                    let a = (a.max(0.0) * scale).min(left);
                    if a > 0.0 {
                        stores.withdraw_for(good, a, u).expect("clamped to stock");
                        left -= a;
                    }
                }
                if left > 0.0 {
                    stores.withdraw_for(good, left.min(stores.amount(good)), Use::Other).expect("clamped to stock");
                }
            } else if d > 0.0 {
                stores.deposit(good, d).expect("finite positive amount");
            }
        }
        self.colony.food = stores.amount(Good::Food);
        self.colony.wood = stores.amount(Good::Sticks);
    }

    pub(super) fn sync_homes(&mut self) {
        use buildings::BuildingKind::{Burrow, Lodge};
        if self.colony.is_founded() {
            let new: Vec<(buildings::BuildingId, (f32, f32))> = self
                .buildings
                .buildings
                .iter()
                .filter(|b| matches!(b.kind, Burrow | Lodge) && b.is_complete() && !self.work.homes.contains_key(&b.id))
                .map(|b| {
                    let (w, d) = b.kind.spec().footprint;
                    (b.id, (b.pos.0 as f32 + w as f32 / 2.0, b.pos.1 as f32 + d as f32 / 2.0))
                })
                .collect();
            for (id, pos) in new {
                let home = self.colony.add_home(pos);
                self.work.homes.insert(id, home);
                if self.buildings.building(id).is_some_and(|b| b.kind == Lodge)
                    && !self.colony.beavers.iter().any(|b| b.lodge == Some(home)) {
                    let previous = self.buildings.buildings.iter().filter(|b| b.kind == Burrow)
                        .filter_map(|b| self.work.homes.get(&b.id).copied())
                        .find(|old| self.colony.beavers.iter().any(|b| b.lodge == Some(*old)));
                    if let Some(old) = previous {
                        let family = self.colony.lodges.iter().find(|l| l.id == old).expect("burrow household").family;
                        self.colony.lodges.iter_mut().find(|l| l.id == home).unwrap().family = family;
                        for b in self.colony.beavers.iter_mut().filter(|b| b.lodge == Some(old)) { b.lodge = Some(home); b.pos = pos; }
                        // The old burrow remains a refuge, not a second family house.
                        self.colony.lodges.retain(|l| l.id != old);
                    }
                }
            }
            self.colony.house_homeless();
        }
        if self.work.supplies.is_some() && self.buildings.buildings.iter().any(|b| b.is_complete() && agents::accepts(b.kind, buildings::Good::Sticks)) {
            self.work.supplies = None;
        }
    }

    pub(super) fn derive_colony_jobs(&mut self) {
        let jobs = jobs::slot_jobs(self);
        for b in self.colony.beavers.iter_mut() {
            b.job = jobs.get(&b.id).map(|j| j.0);
            if let Some(a) = self.agents.get(b.id) {
                b.pos = (a.cell.0 as f32 + 0.5, a.cell.1 as f32 + 0.5);
            }
        }
        self.colony.slot_jobs = jobs;
    }

    pub(super) fn step_events(&mut self, dt_days: f32) {
        let wetland = self.ecology.wetland.as_slice();
        let mean = wetland.iter().step_by(7).sum::<f32>()
            / wetland.iter().step_by(7).count().max(1) as f32;
        let before = self.mirror_colony_stores();
        self.events.step(dt_days, &mut self.colony, mean);
        self.settle_colony_stores(before);
    }

}

pub(super) struct Mirror {
    food: f32,
    wood: f32,
    eaten: [f32; 4],
    spoiled: f32,
    repair_wood: f32,
}

pub struct WorldLand<'a> {
    pub world: &'a worldgen::World,
    pub depth: &'a grid::Grid<f32>,
}

impl colony::Landscape for WorldLand<'_> {
    fn is_water(&self, x: f32, y: f32) -> bool {
        if !(x >= 0.0 && y >= 0.0) {
            return false;
        }
        let (cx, cy) = (x as usize, y as usize);
        self.world.river.get(cx, cy).copied().unwrap_or(false)
            || self.world.lake.get(cx, cy).copied().unwrap_or(false)
            || self.depth.get(cx, cy).is_some_and(|d| *d > 0.05)
    }
}
