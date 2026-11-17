//! Building catalogue (owner: `content`): for every `BuildingKind`, its cost, footprint, site rule
//! and build work (`BuildingKind::spec`) plus the worker slots, work area, unlock tech, tier,
//! storage capacity and crafting rate defined here. `def` is an exhaustive match, so a new kind
//! cannot be added without a full definition. Numbers live in `tuning::buildings`.

use crate::buildings::BuildingKind;
use crate::features::System;
use crate::resources::ResourceKind;
use crate::tuning::buildings::{batches_per_day, store_capacity};
use crate::work::Role;

/// Era a building belongs to; start 2 uses `Elementary`, start 3 adds `Industrial`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Tier {
    Elementary,
    Industrial,
}

#[derive(Clone, Debug)]
pub struct Def {
    pub kind: BuildingKind,
    pub tier: Tier,
    /// One entry per worker slot (empty for passive buildings).
    pub slots: Vec<Role>,
    /// Default work-area size (w, h) in cells around the building; `None` if workers do not gather.
    pub area: Option<(usize, usize)>,
    /// Tech index (into `tech::TECHS`) needed, or `None` if always available.
    pub unlock: Option<usize>,
    /// Belongs to a switchable system (guard post, scent mound): hidden when it is disabled.
    pub system: Option<System>,
    /// Storage added per good the building accepts (0 if not a store).
    pub capacity: f32,
    /// Crafting batches per day at base speed (0 if it does not craft).
    pub batches_per_day: f32,
}

impl Def {
    /// Crafting batches per day each worker slot yields (0 without slots or crafting).
    pub fn batches_per_worker(&self) -> f32 {
        if self.slots.is_empty() { 0.0 } else { self.batches_per_day / self.slots.len() as f32 }
    }
}

const GATHER_AREA: (usize, usize) = (9, 9);

fn camp(kind: ResourceKind) -> (Tier, Vec<Role>, Option<(usize, usize)>) {
    (Tier::Elementary, vec![Role::Collect(kind); 2], Some(GATHER_AREA))
}

/// Definition for `kind`.
pub fn def(kind: BuildingKind) -> Def {
    use BuildingKind::*;
    let passive = (Tier::Elementary, vec![], None);
    let (tier, slots, area) = match kind {
        Dam | Lodge | Burrow | ScentMound | GuardPost | Canal | Lock | WaterWheel | Barge => passive,
        FoodCache | Warehouse | Dock => (Tier::Elementary, vec![Role::Haul], None),
        Workshop => (Tier::Elementary, vec![Role::Craft], None),
        LoggingCamp => camp(ResourceKind::Tree),
        ForagerHut => camp(ResourceKind::Plant),
        MudPit => camp(ResourceKind::MudBank),
        QuarryCamp => camp(ResourceKind::StoneOutcrop),
        Mill => (Tier::Industrial, vec![Role::Craft; 2], None),
        Storehouse => (Tier::Industrial, vec![Role::Haul; 2], None),
    };
    Def {
        kind,
        tier,
        slots,
        area,
        unlock: crate::tech::unlock_index(kind),
        system: System::of_building(kind),
        capacity: store_capacity(kind),
        batches_per_day: batches_per_day(kind),
    }
}

/// Kinds the player may see and place given features (and, in `content`, tech).
pub fn available(sim: &crate::SimState) -> Vec<BuildingKind> {
    BuildingKind::ALL
        .iter()
        .copied()
        .filter(|k| def(*k).system.is_none_or(|s| sim.features.enabled(s)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::buildings::{Building, BuildingId, Buildings, Good, Recipe, Status, Terrain};
    use crate::features::Features;
    use crate::grid::Grid;
    use crate::tech::{Effect, Tech, BASE_BUILDINGS, TECHS};

    #[test]
    fn every_building_is_fully_defined_and_unlocks_consistently() {
        for kind in BuildingKind::ALL {
            let (spec, d) = (kind.spec(), def(kind));
            assert!(!spec.cost.is_empty() && spec.cost.iter().all(|a| a.1 > 0.0), "{kind:?} cost");
            assert!(spec.footprint.0 > 0 && spec.footprint.1 > 0 && spec.work > 0.0, "{kind:?} footprint/work");
            assert!(!spec.name.is_empty());
            match d.unlock {
                Some(i) => assert!(
                    TECHS[i].effects.contains(&Effect::Unlock(kind)) && !BASE_BUILDINGS.contains(&kind),
                    "{kind:?} unlock"
                ),
                None => assert!(BASE_BUILDINGS.contains(&kind), "{kind:?} has no unlock and is not a base building"),
            }
            assert_eq!(d.area.is_some(), d.slots.iter().any(|r| matches!(r, Role::Collect(_))), "{kind:?}: area iff collectors");
            assert_eq!(d.batches_per_day > 0.0, d.slots.contains(&Role::Craft), "{kind:?}: crafting needs craft slots");
            if d.capacity > 0.0 {
                assert!(d.slots.contains(&Role::Haul), "{kind:?}: stores take haulers");
            }
        }
        for k in [BuildingKind::LoggingCamp, BuildingKind::ForagerHut, BuildingKind::MudPit, BuildingKind::QuarryCamp] {
            assert_eq!(def(k).tier, Tier::Elementary);
            assert!(!def(k).slots.is_empty() && def(k).area.is_some());
        }
        assert!(def(BuildingKind::Mill).tier == Tier::Industrial && def(BuildingKind::Storehouse).tier == Tier::Industrial);
    }

    #[test]
    fn default_features_hide_every_disabled_system_building() {
        let sim = crate::SimState::new(1);
        assert_eq!(sim.features, Features::scoped());
        let shown = available(&sim);
        for k in BuildingKind::ALL {
            let system = def(k).system;
            assert_eq!(shown.contains(&k), system.is_none(), "{k:?}");
            assert!(system.is_none_or(|s| System::OUT_OF_SCOPE.contains(&s)));
        }
        // Only the guard post (guard) and scent mound (predators) belong to a switchable system.
        let hidden: Vec<_> = BuildingKind::ALL.into_iter().filter(|k| !shown.contains(k)).collect();
        assert_eq!(hidden, [BuildingKind::ScentMound, BuildingKind::GuardPost]);
    }

    fn complete(kind: BuildingKind, recipe: Option<Recipe>) -> Buildings {
        let mut b = Buildings::default();
        b.buildings.push(Building { id: BuildingId(1), kind, pos: (0, 0), status: Status::Complete, recipe, output: 0.0 });
        b
    }

    /// Steps `b` once with default tech on dry land.
    fn step(b: &mut Buildings, dt: f32) {
        let (h, d) = (Grid::new(8, 8, 1.0), Grid::new(8, 8, 0.0));
        b.step(dt, &Tech::default(), Terrain { height: &h, depth: &d }, &crate::colony::Colony::new(1));
    }

    #[test]
    fn advanced_store_holds_more_than_elementary_store() {
        let (elem, adv) = (def(BuildingKind::Warehouse), def(BuildingKind::Storehouse));
        assert!(adv.tier > elem.tier && adv.capacity > elem.capacity && elem.capacity > 0.0);
        let capacity = |kind| {
            let mut b = complete(kind, None);
            step(&mut b, 0.0);
            b.stores.capacity(Good::Logs)
        };
        let base = capacity(BuildingKind::Dam);
        assert_eq!(capacity(BuildingKind::Warehouse) - base, elem.capacity);
        assert_eq!(capacity(BuildingKind::Storehouse) - base, adv.capacity);
        assert!(capacity(BuildingKind::Storehouse) > capacity(BuildingKind::Warehouse));
    }

    #[test]
    fn advanced_production_yields_more_per_worker_than_elementary() {
        let (elem, adv) = (def(BuildingKind::Workshop), def(BuildingKind::Mill));
        assert!(adv.tier > elem.tier && adv.batches_per_worker() > elem.batches_per_worker());
        // Produced rope per day per worker slot, from plentiful inputs.
        let per_worker = |kind: BuildingKind| {
            let mut b = complete(kind, Some(Recipe::Rope));
            b.stores.deposit(Good::Sticks, 100.0).unwrap();
            b.stores.deposit(Good::Mud, 100.0).unwrap();
            step(&mut b, 0.0);
            step(&mut b, 1.0);
            b.stores.produced_total[Good::Rope.index()] / def(kind).slots.len() as f32
        };
        let (w, m) = (per_worker(BuildingKind::Workshop), per_worker(BuildingKind::Mill));
        assert!(w > 0.0 && m > w, "mill {m} per worker vs workshop {w}");
    }
}
