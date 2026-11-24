//! Synchronizes colony needs and families with physical stores, workers and homes.
use crate::{SimState, buildings, colony, grid, jobs, worldgen};

impl SimState {
    pub(super) fn refresh_store_view(&mut self) {
        self.colony.food = self.buildings.stores.amount(buildings::Good::Food);
        self.colony.wood = self.buildings.stores.amount(buildings::Good::Sticks);
    }

    pub(super) fn sync_homes(&mut self) {
        use buildings::BuildingKind::Shelter;
        if self.colony.is_founded() {
            let new: Vec<(buildings::BuildingId, (f32, f32))> = self
                .buildings
                .buildings
                .iter()
                .filter(|b| b.kind == Shelter && b.is_complete() && !self.work.homes.contains_key(&b.id))
                .map(|b| {
                    let (w, d) = b.kind.spec().footprint;
                    (b.id, (b.pos.0 as f32 + w as f32 / 2.0, b.pos.1 as f32 + d as f32 / 2.0))
                })
                .collect();
            for (id, pos) in new {
                let home = self.colony.add_home(pos);
                self.work.homes.insert(id, home);
                if !self.colony.beavers.iter().any(|b| b.lodge == Some(home)) {
                    // A family still camping where it arrived moves into the first shelter.
                    let built: std::collections::BTreeSet<_> = self.work.homes.values().copied().collect();
                    let previous = self.colony.lodges.iter().map(|l| l.id)
                        .find(|old| !built.contains(old) && self.colony.beavers.iter().any(|b| b.lodge == Some(*old)));
                    if let Some(old) = previous {
                        let family = self.colony.lodges.iter().find(|l| l.id == old).expect("camp household").family;
                        self.colony.lodges.iter_mut().find(|l| l.id == home).unwrap().family = family;
                        for b in self.colony.beavers.iter_mut().filter(|b| b.lodge == Some(old)) { b.lodge = Some(home); b.pos = pos; }
                        self.colony.lodges.retain(|l| l.id != old);
                    }
                }
            }
            self.colony.house_homeless();
        }
        // The arrival stockpile remains physical until its goods have been hauled.
    }

    /// What moves a beaver's needs, health and mood now, on this map (see `colony::Rates`).
    pub fn beaver_rates(&self, id: colony::BeaverId) -> Option<colony::Rates> {
        self.colony.rates(id, &WorldLand { world: &self.world, depth: &self.water.depth })
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
        self.events.step(dt_days, &mut self.colony, &mut self.buildings.stores, mean);
        self.refresh_store_view();
    }

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
