use crate::buildings::BuildingKind;
use crate::features::System;
use crate::resources::ResourceKind;
use crate::tuning::buildings::{batches_per_day, store_capacity};
use crate::work::Role;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Tier {
    Elementary,
    Industrial,
}

#[derive(Clone, Debug)]
pub struct Def {
    pub kind: BuildingKind,
    pub tier: Tier,
    pub slots: Vec<Role>,
    pub area: Option<(usize, usize)>,
    pub unlock: Option<usize>,
    pub system: Option<System>,
    pub capacity: f32,
    pub batches_per_day: f32,
}

impl Def {
    pub fn batches_per_worker(&self) -> f32 {
        if self.slots.is_empty() { 0.0 } else { self.batches_per_day / self.slots.len() as f32 }
    }
}

const GATHER_AREA: (usize, usize) = (31, 31);

fn camp(kind: ResourceKind) -> (Tier, Vec<Role>, Option<(usize, usize)>) {
    (Tier::Elementary, vec![Role::Collect(kind); 2], Some(GATHER_AREA))
}

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
        ForesterLodge => (Tier::Elementary, vec![Role::Forester; 2], Some(GATHER_AREA)),
        TreeNursery => (Tier::Elementary, vec![Role::Forester], None),
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

pub fn work_area(b: &crate::buildings::Building) -> Option<crate::work::Area> {
    let c = b.cells().next().unwrap_or(b.pos);
    def(b.kind).area.map(|(w, h)| crate::work::Area {
        min: (c.0.saturating_sub(w / 2), c.1.saturating_sub(h / 2)),
        max: (c.0 + w / 2, c.1 + h / 2),
    })
}

pub fn prerequisites(kind: BuildingKind) -> &'static [BuildingKind] {
    use BuildingKind::*;
    match kind {
        Lodge => &[Burrow],
        FoodCache => &[ForagerHut],
        Workshop | Dock | QuarryCamp | Storehouse => &[Warehouse],
        WaterWheel => &[Workshop],
        Lock => &[Dock],
        Mill => &[Workshop, WaterWheel],
        TreeNursery => &[ForesterLodge],
        GuardPost => &[ScentMound],
        _ => &[],
    }
}

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
    use crate::colony::BeaverId;
    use crate::tech::{Effect, Tech, BASE_BUILDINGS, TECHS};
    use crate::work::{Slot, Work, Workplace};

    #[test]
    fn prerequisites_never_loop_and_the_first_buildings_need_none() {
        for k in crate::tech::BASE_BUILDINGS {
            assert!(prerequisites(k).is_empty(), "{k:?}");
        }
        for k in BuildingKind::ALL {
            let mut seen = vec![k];
            let mut todo = prerequisites(k).to_vec();
            while let Some(p) = todo.pop() {
                assert_ne!(p, k, "{k:?} needs itself");
                if !seen.contains(&p) {
                    seen.push(p);
                    todo.extend_from_slice(prerequisites(p));
                }
            }
        }
    }

    #[test]
    fn every_building_is_fully_defined_and_unlocks_consistently() {
        for kind in BuildingKind::ALL {
            let (spec, d) = (kind.spec(), def(kind));
            assert!((kind == BuildingKind::Burrow || !spec.cost.is_empty()) && spec.cost.iter().all(|a| a.1 > 0.0), "{kind:?} cost");
            assert!(spec.footprint.0 > 0 && spec.footprint.1 > 0 && spec.work > 0.0, "{kind:?} footprint/work");
            assert!(!spec.name.is_empty());
            match d.unlock {
                Some(i) => assert!(
                    TECHS[i].effects.contains(&Effect::Unlock(kind)) && !BASE_BUILDINGS.contains(&kind),
                    "{kind:?} unlock"
                ),
                None => assert!(BASE_BUILDINGS.contains(&kind), "{kind:?} has no unlock and is not a base building"),
            }
            let in_area = kind == BuildingKind::ForesterLodge || d.slots.iter().any(|r| matches!(r, Role::Collect(_)));
            assert_eq!(d.area.is_some(), in_area, "{kind:?}: area iff collectors or foresters");
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
        let hidden: Vec<_> = BuildingKind::ALL.into_iter().filter(|k| !shown.contains(k)).collect();
        assert_eq!(hidden, [BuildingKind::ScentMound, BuildingKind::GuardPost]);
    }

    fn complete(kind: BuildingKind, recipe: Option<Recipe>) -> Buildings {
        let mut b = Buildings::default();
        b.buildings.push(Building { id: BuildingId(1), kind, pos: (0, 0), status: Status::Complete, recipe, output: 0.0 });
        b
    }

    fn step_with(b: &mut Buildings, dt: f32, work: &Work) {
        let (h, d) = (Grid::new(8, 8, 1.0), Grid::new(8, 8, 0.0));
        b.step(dt, &Tech::default(), Terrain { height: &h, depth: &d }, &crate::colony::Colony::new(1), work);
    }

    fn step(b: &mut Buildings, dt: f32) {
        step_with(b, dt, &Work::default());
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

    fn rope_in_a_day(kind: BuildingKind, staff: usize) -> f32 {
        let mut b = complete(kind, Some(Recipe::Rope));
        b.stores.deposit(Good::Sticks, 100.0).unwrap();
        b.stores.deposit(Good::Mud, 100.0).unwrap();
        let slots = def(kind).slots.iter().map(|&role| Slot::empty(role)).collect();
        let mut work = Work::default();
        work.places.push(Workplace::new(BuildingId(1), kind, slots, None, None));
        for i in 0..staff {
            work.assign(BuildingId(1), i, BeaverId(i as u64 + 1)).unwrap();
        }
        step_with(&mut b, 0.0, &work);
        step_with(&mut b, 1.0, &work);
        b.stores.produced_total[Good::Rope.index()]
    }

    #[test]
    fn production_follows_staffed_craft_slots() {
        for kind in [BuildingKind::Workshop, BuildingKind::Mill] {
            assert_eq!(rope_in_a_day(kind, 0), 0.0, "{kind:?}: no worker, no output");
        }
        let one = rope_in_a_day(BuildingKind::Mill, 1);
        let two = rope_in_a_day(BuildingKind::Mill, 2);
        assert!(one > 0.0 && (two - 2.0 * one).abs() < 1e-4, "mill: one worker {one}, two workers {two}");
    }

    #[test]
    fn advanced_production_yields_more_per_worker_than_elementary() {
        let (elem, adv) = (def(BuildingKind::Workshop), def(BuildingKind::Mill));
        assert!(adv.tier > elem.tier && adv.batches_per_worker() > elem.batches_per_worker());
        let per_worker = |kind: BuildingKind| {
            let staff = def(kind).slots.iter().filter(|r| **r == Role::Craft).count();
            assert!(staff > 0);
            rope_in_a_day(kind, staff) / staff as f32
        };
        let (w, m) = (per_worker(BuildingKind::Workshop), per_worker(BuildingKind::Mill));
        assert!(w > 0.0 && m > w, "mill {m} per worker vs workshop {w}");
        let (w1, m1) = (rope_in_a_day(BuildingKind::Workshop, 1), rope_in_a_day(BuildingKind::Mill, 1));
        assert!(m1 > w1, "one mill worker {m1} vs one workshop worker {w1}");
    }
}
