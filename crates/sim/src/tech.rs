//! Research tree (see `docs/PROGRESSION.md` section 3). Only `BASE_BUILDINGS` can be placed at
//! first; every other building is unlocked by a tech, learned in one of two ways:
//!
//! * **Achievement:** the first time something is done (`Tech::record`), the tech is granted at once.
//! * **Experience:** once every need is met (`Tech::needs` is empty) it unlocks the same tick
//!   (`Tech::update`). Needs: branch experience (`Colony::experience` summed by `Branch::jobs`),
//!   colony size, prerequisite techs, an elder alive for the den, and lore studied by elders in an
//!   elders' den (`den_lore`, `Tech::study`).
//!
//! `Tech::missing` reports what a locked building still lacks.

use crate::buildings::BuildingKind;
use crate::colony::{Colony, Job};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Branch {
    Water,
    Forest,
    Building,
    Craft,
    Transport,
    Defence,
}

impl Branch {
    pub const ALL: [Branch; 6] =
        [Branch::Water, Branch::Forest, Branch::Building, Branch::Craft, Branch::Transport, Branch::Defence];

    /// Jobs whose colony experience counts towards this branch.
    pub fn jobs(self) -> &'static [Job] {
        match self {
            Branch::Water => &[Job::Digger],
            Branch::Forest => &[Job::Logger, Job::Forager, Job::Scout],
            Branch::Building => &[Job::Builder],
            Branch::Craft => &[Job::Crafter],
            Branch::Transport => &[Job::Hauler],
            Branch::Defence => &[Job::Guard],
        }
    }

    /// Experience of `colony` in this branch.
    pub fn experience(self, colony: &Colony) -> f32 {
        self.jobs().iter().map(|j| colony.experience[j.index()]).sum()
    }
}

/// Multiplicative colony-wide bonuses granted by techs.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Bonus {
    /// Multiplies construction work required (< 1 is faster).
    BuildWork(f32),
    /// Multiplies workshop crafting speed.
    CraftSpeed(f32),
    /// Multiplies storage capacity.
    Storage(f32),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Effect {
    Unlock(BuildingKind),
    Bonus(Bonus),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TechId(pub usize);

/// Things done for the first time that grant research at once.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Achievement {
    FirstBurrow,
    FirstDam,
    FirstWinter,
}

impl Achievement {
    pub const ALL: [Achievement; 3] = [Achievement::FirstBurrow, Achievement::FirstDam, Achievement::FirstWinter];
    fn index(self) -> usize {
        self as usize
    }
}

/// How a tech is learned.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Route {
    Achievement(Achievement),
    Experience,
}

#[derive(Clone, Copy, Debug)]
pub struct TechDef {
    pub name: &'static str,
    /// Branch whose experience counts (`xp`).
    pub branch: Branch,
    /// Display tier within the branch (1 = earliest).
    pub tier: u8,
    pub route: Route,
    /// Branch experience required (experience route).
    pub xp: f32,
    /// Living beavers required (experience route).
    pub size: usize,
    /// Techs that must be known first (experience route).
    pub requires: &'static [TechId],
    pub effects: &'static [Effect],
}

use crate::tuning::tech::*;
use Effect::{Bonus as B, Unlock as U};

const fn exp(
    name: &'static str,
    branch: Branch,
    tier: u8,
    xp: f32,
    size: usize,
    requires: &'static [TechId],
    effects: &'static [Effect],
) -> TechDef {
    TechDef { name, branch, tier, route: Route::Experience, xp, size, requires, effects }
}

const fn ach(name: &'static str, branch: Branch, a: Achievement, effects: &'static [Effect]) -> TechDef {
    TechDef { name, branch, tier: 1, route: Route::Achievement(a), xp: 0.0, size: 0, requires: &[], effects }
}

const FELLING: TechId = TechId(1);
const LODGE_BUILDING: TechId = TechId(3);
const STONEWORK: TechId = TechId(4);
const WEAVING: TechId = TechId(5);
const DOCKS: TechId = TechId(7);
const FLOW_CONTROL: TechId = TechId(8);
const FORESTRY: TechId = TechId(16);

/// Names of the forestry techs whose effects are not building unlocks (`forestry::knows`):
/// Forestry also opens the logging camps' cutting rules, Coppicing the `Coppice` rule.
pub const FORESTRY_TECH: &str = "Forestry";
pub const COPPICING_TECH: &str = "Coppicing";

/// Every tech, indexed by `TechId`.
pub const TECHS: &[TechDef] = &[
    ach("Foraging", Branch::Forest, Achievement::FirstBurrow, &[U(BuildingKind::ForagerHut)]),
    // Wood and mud are needed from the first weeks: felling comes with the first shelter.
    ach("Felling", Branch::Forest, Achievement::FirstBurrow, &[U(BuildingKind::LoggingCamp), U(BuildingKind::MudPit)]),
    ach("Winter stores", Branch::Forest, Achievement::FirstWinter, &[U(BuildingKind::FoodCache)]),
    exp("Lodge building", Branch::Building, 1, 16.0, 5, &[FELLING], &[U(BuildingKind::Lodge)]),
    exp("Stonework", Branch::Forest, 2, 40.0, 6, &[FELLING], &[U(BuildingKind::QuarryCamp)]),
    exp("Weaving", Branch::Forest, 3, 60.0, 8, &[FELLING], &[U(BuildingKind::Workshop)]),
    // Water transport grows with the colony, not with elders' lore: a colony past its first
    // year needs it to reach far deposits.
    exp("Canal digging", Branch::Water, 1, 30.0, 8, &[LODGE_BUILDING], &[U(BuildingKind::Canal)]),
    exp("Docks", Branch::Building, 1, 30.0, 8, &[LODGE_BUILDING], &[U(BuildingKind::Dock)]),
    exp("Flow control", Branch::Water, 2, 60.0, 12, &[DOCKS, STONEWORK], &[U(BuildingKind::Lock), U(BuildingKind::WaterWheel)]),
    exp("Barges", Branch::Building, 2, 60.0, 10, &[DOCKS, WEAVING], &[U(BuildingKind::Barge)]),
    exp("Grand storage", Branch::Building, 3, 100.0, 20, &[LODGE_BUILDING], &[U(BuildingKind::Storehouse)]),
    exp("Industrial milling", Branch::Craft, 3, 100.0, 24, &[WEAVING, FLOW_CONTROL], &[U(BuildingKind::Mill)]),
    exp("Master builders", Branch::Building, 2, 60.0, 10, &[LODGE_BUILDING], &[B(Bonus::BuildWork(BUILD_WORK_BONUS))]),
    exp("Dry stores", Branch::Forest, 4, 100.0, 10, &[STONEWORK], &[B(Bonus::Storage(STORAGE_BONUS))]),
    exp("Fine tools", Branch::Craft, 2, 50.0, 10, &[WEAVING], &[B(Bonus::CraftSpeed(CRAFT_SPEED_BONUS))]),
    exp("Territory marking", Branch::Defence, 1, 3.0, 6, &[FELLING], &[U(BuildingKind::ScentMound), U(BuildingKind::GuardPost)]),
    // Forestry (`forestry`): planting and upkeep, right after Felling.
    exp(FORESTRY_TECH, Branch::Forest, 1, 20.0, 5, &[FELLING], &[U(BuildingKind::ForesterLodge)]),
    exp(COPPICING_TECH, Branch::Forest, 2, 50.0, 8, &[FORESTRY], &[]),
    exp("Tree nursery", Branch::Forest, 3, 80.0, 10, &[FORESTRY, WEAVING], &[U(BuildingKind::TreeNursery)]),
];

/// Index into `TECHS` of the tech that unlocks `kind`, if any.
pub fn unlock_index(kind: BuildingKind) -> Option<usize> {
    TECHS.iter().position(|t| t.effects.contains(&Effect::Unlock(kind)))
}

/// Buildings available without any research.
pub const BASE_BUILDINGS: [BuildingKind; 3] = [BuildingKind::Burrow, BuildingKind::Dam, BuildingKind::Warehouse];

/// One thing a tech (or a building's tech) still lacks.
#[derive(Clone, Debug, PartialEq)]
pub enum Gap {
    /// A prerequisite tech not yet known.
    Research(&'static str),
    /// Colony too small.
    Size { have: usize, need: usize },
    /// Branch experience short.
    Experience { branch: Branch, have: f32, need: f32 },
    /// Only an achievement grants this tech; it has not happened yet.
    Achievement(Achievement),
    /// A building that must stand finished first (`catalogue::prerequisites`).
    Building(BuildingKind),
    /// No tech unlocks this building at all (cannot happen for catalogue kinds).
    NoTech,
}

#[derive(Clone, Debug, Default)]
pub struct Tech {
    unlocked: Vec<bool>,
    achieved: [bool; 3],
}

impl Tech {
    /// Unlocks every tech (tests and scripted scenes).
    pub fn unlock_all(&mut self) {
        self.unlocked = vec![true; TECHS.len()];
        self.achieved = [true; 3];
    }

    pub fn is_unlocked(&self, id: TechId) -> bool {
        self.unlocked.get(id.0).copied().unwrap_or(false)
    }

    pub fn unlocked_ids(&self) -> impl Iterator<Item = TechId> + '_ {
        (0..TECHS.len()).map(TechId).filter(|&i| self.is_unlocked(i))
    }

    pub fn has_achieved(&self, a: Achievement) -> bool {
        self.achieved[a.index()]
    }

    /// Whether `kind` may be built: only `BASE_BUILDINGS`, or a kind unlocked by known research.
    pub fn allows(&self, kind: BuildingKind) -> bool {
        BASE_BUILDINGS.contains(&kind)
            || self.unlocked_ids().any(|id| TECHS[id.0].effects.contains(&Effect::Unlock(kind)))
    }

    /// Records `a`; the first time, unlocks (at once, free) every tech it grants and returns them.
    /// Recording again returns nothing, so each achievement grants research once.
    pub fn record(&mut self, a: Achievement) -> Vec<TechId> {
        if std::mem::replace(&mut self.achieved[a.index()], true) {
            return Vec::new();
        }
        self.unlocked.resize(TECHS.len(), false);
        let mut new = Vec::new();
        for (i, t) in TECHS.iter().enumerate() {
            if t.route == Route::Achievement(a) && !self.unlocked[i] {
                self.unlocked[i] = true;
                new.push(TechId(i));
            }
        }
        new
    }

    /// What tech `id` still lacks (empty = ready to unlock or already known).
    pub fn needs(&self, id: TechId, colony: &Colony) -> Vec<Gap> {
        let t = &TECHS[id.0];
        if self.is_unlocked(id) {
            return Vec::new();
        }
        if let Route::Achievement(a) = t.route {
            return vec![Gap::Achievement(a)];
        }
        let mut gaps: Vec<Gap> =
            t.requires.iter().filter(|r| !self.is_unlocked(**r)).map(|r| Gap::Research(TECHS[r.0].name)).collect();
        if colony.population() < t.size {
            gaps.push(Gap::Size { have: colony.population(), need: t.size });
        }
        let have = t.branch.experience(colony);
        if have < t.xp {
            gaps.push(Gap::Experience { branch: t.branch, have, need: t.xp });
        }
        gaps
    }

    /// What is missing before `kind` can be built: empty if it already can be.
    pub fn missing(&self, kind: BuildingKind, colony: &Colony) -> Vec<Gap> {
        if self.allows(kind) {
            return Vec::new();
        }
        match unlock_index(kind) {
            Some(i) => self.needs(TechId(i), colony),
            None => vec![Gap::NoTech],
        }
    }

    /// Product of every unlocked bonus selected by `pick` (1.0 if none).
    fn product(&self, pick: impl Fn(Bonus) -> Option<f32>) -> f32 {
        self.unlocked_ids()
            .flat_map(|id| TECHS[id.0].effects.iter())
            .filter_map(|e| if let Effect::Bonus(b) = e { pick(*b) } else { None })
            .product()
    }
    pub fn build_work_factor(&self) -> f32 {
        self.product(|b| if let Bonus::BuildWork(f) = b { Some(f) } else { None })
    }
    pub fn craft_speed_factor(&self) -> f32 {
        self.product(|b| if let Bonus::CraftSpeed(f) = b { Some(f) } else { None })
    }
    pub fn storage_factor(&self) -> f32 {
        self.product(|b| if let Bonus::Storage(f) = b { Some(f) } else { None })
    }

    /// Unlocks every experience tech whose needs are all met (chains resolve in one call).
    /// Returns the newly unlocked techs.
    pub fn update(&mut self, colony: &Colony) -> Vec<TechId> {
        self.unlocked.resize(TECHS.len(), false);
        let mut new = Vec::new();
        loop {
            let ready: Vec<usize> = (0..TECHS.len())
                .filter(|&i| TECHS[i].route == Route::Experience && !self.unlocked[i] && self.needs(TechId(i), colony).is_empty())
                .collect();
            if ready.is_empty() {
                return new;
            }
            for i in ready {
                self.unlocked[i] = true;
                new.push(TechId(i));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use BuildingKind::*;

    fn colony_of(n: usize) -> Colony {
        let mut c = Colony::new(1);
        c.add_immigrants(n.saturating_sub(c.population()) as u32);
        c
    }

    #[test]
    fn fresh_colony_builds_only_burrow_dam_and_stockpile() {
        let t = Tech::default();
        for k in BuildingKind::ALL {
            assert_eq!(t.allows(k), matches!(k, Burrow | Dam | Warehouse), "{k:?}");
        }
    }

    #[test]
    fn each_achievement_grants_research_once() {
        // The first burrow brings foraging and felling (food and wood from the first weeks); the
        // first winter brings winter stores; the first dam grants nothing of its own.
        for a in [Achievement::FirstBurrow, Achievement::FirstWinter] {
            let mut t = Tech::default();
            let first = t.record(a);
            assert!(!first.is_empty(), "{a:?} grants research");
            assert!(first.iter().all(|id| t.is_unlocked(*id) && TECHS[id.0].route == Route::Achievement(a)));
            assert!(t.record(a).is_empty(), "{a:?} grants nothing the second time");
        }
        let mut t = Tech::default();
        t.record(Achievement::FirstBurrow);
        assert!(t.allows(ForagerHut) && t.allows(LoggingCamp) && t.allows(MudPit));
    }

    #[test]
    fn missing_report_lists_research_size_and_experience_gaps() {
        let t = Tech::default();
        let mut c = colony_of(2);
        let gaps = t.missing(Lodge, &c);
        assert!(gaps.contains(&Gap::Research("Felling")), "{gaps:?}");
        assert!(gaps.contains(&Gap::Size { have: 2, need: 5 }), "{gaps:?}");
        assert!(gaps.contains(&Gap::Experience { branch: Branch::Building, have: 0.0, need: 16.0 }), "{gaps:?}");
        assert!(t.missing(Dam, &c).is_empty());
        // Meeting every need unlocks it at the next update.
        let mut t = Tech::default();
        t.record(Achievement::FirstBurrow);
        c = colony_of(5);
        c.experience[Job::Builder.index()] = 16.0;
        assert!(t.needs(LODGE_BUILDING, &c).is_empty());
        t.update(&c);
        assert!(t.allows(Lodge) && t.missing(Lodge, &c).is_empty());
    }

    #[test]
    fn every_tech_comes_with_size_experience_and_known_research() {
        // With every need met, everything opens: no elders, no study.
        let mut t = Tech::default();
        t.record(Achievement::FirstBurrow);
        let mut c = colony_of(30);
        for j in Job::ALL {
            c.experience[j.index()] = 500.0;
        }
        t.update(&c);
        for k in [Canal, Dock, Barge, Lock, WaterWheel, Storehouse, Mill, TreeNursery] {
            assert!(t.allows(k), "{k:?}");
        }
    }

    #[test]
    fn founding_year_with_three_achievements_unlocks_a_building() {
        let mut t = Tech::default();
        let count = |t: &Tech| BuildingKind::ALL.iter().filter(|k| t.allows(**k)).count();
        let before = count(&t);
        for a in Achievement::ALL {
            t.record(a);
        }
        t.update(&colony_of(4));
        assert!(count(&t) > before);
        for k in [ForagerHut, LoggingCamp, MudPit, FoodCache] {
            assert!(t.allows(k), "{k:?}");
        }
    }

    #[test]
    fn forest_experience_sums_jobs_and_bonuses_apply() {
        let mut t = Tech::default();
        t.record(Achievement::FirstBurrow);
        let mut c = colony_of(10);
        c.experience[Job::Logger.index()] = 50.0;
        c.experience[Job::Forager.index()] = 40.0;
        t.update(&c);
        assert_eq!(t.storage_factor(), 1.0, "Dry stores needs 100 forest experience");
        c.experience[Job::Logger.index()] = 60.0;
        t.update(&c);
        assert_eq!(t.storage_factor(), 1.25);
        assert_eq!(t.build_work_factor(), 1.0);
    }
}
