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

use crate::buildings::{BuildingKind, Buildings};
use crate::colony::{Colony, Job, Stage};
use crate::work::{Role, Work};

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
    /// Lore to study at an elders' den (0 = none).
    pub lore: f32,
    /// An elder must be alive.
    pub needs_elder: bool,
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
    lore: f32,
    effects: &'static [Effect],
) -> TechDef {
    TechDef { name, branch, tier, route: Route::Experience, xp, size, requires, lore, needs_elder: false, effects }
}

const fn ach(name: &'static str, branch: Branch, a: Achievement, effects: &'static [Effect]) -> TechDef {
    TechDef { name, branch, tier: 1, route: Route::Achievement(a), xp: 0.0, size: 0, requires: &[], lore: 0.0, needs_elder: false, effects }
}

const FELLING: TechId = TechId(1);
const LODGE_BUILDING: TechId = TechId(3);
const STONEWORK: TechId = TechId(4);
const WEAVING: TechId = TechId(5);
const CANAL_DIGGING: TechId = TechId(7);
const DOCKS: TechId = TechId(8);
const FLOW_CONTROL: TechId = TechId(9);
const FORESTRY: TechId = TechId(17);

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
    exp("Lodge building", Branch::Building, 1, 16.0, 5, &[FELLING], 0.0, &[U(BuildingKind::Lodge)]),
    exp("Stonework", Branch::Forest, 2, 40.0, 6, &[FELLING], 0.0, &[U(BuildingKind::QuarryCamp)]),
    exp("Weaving", Branch::Forest, 3, 60.0, 8, &[FELLING], 0.0, &[U(BuildingKind::Workshop)]),
    TechDef {
        name: "Elders' lore",
        branch: Branch::Building,
        tier: 2,
        route: Route::Experience,
        xp: 0.0,
        size: 8,
        requires: &[LODGE_BUILDING],
        lore: 0.0,
        needs_elder: true,
        effects: &[U(BuildingKind::EldersDen)],
    },
    exp("Canal digging", Branch::Water, 1, 60.0, 12, &[STONEWORK, LODGE_BUILDING], 30.0, &[U(BuildingKind::Canal)]),
    exp("Docks", Branch::Transport, 1, 40.0, 14, &[CANAL_DIGGING], 40.0, &[U(BuildingKind::Dock)]),
    exp("Flow control", Branch::Water, 2, 120.0, 18, &[CANAL_DIGGING], 60.0, &[U(BuildingKind::Lock), U(BuildingKind::WaterWheel)]),
    exp("Barges", Branch::Transport, 2, 80.0, 20, &[DOCKS], 60.0, &[U(BuildingKind::Barge)]),
    exp("Grand storage", Branch::Building, 3, 100.0, 20, &[LODGE_BUILDING], 60.0, &[U(BuildingKind::Storehouse)]),
    exp("Industrial milling", Branch::Craft, 3, 100.0, 24, &[WEAVING, FLOW_CONTROL], 80.0, &[U(BuildingKind::Mill)]),
    exp("Master builders", Branch::Building, 2, 60.0, 10, &[LODGE_BUILDING], 40.0, &[B(Bonus::BuildWork(BUILD_WORK_BONUS))]),
    exp("Forest lore", Branch::Forest, 4, 100.0, 10, &[STONEWORK], 30.0, &[B(Bonus::Storage(STORAGE_BONUS))]),
    exp("Fine tools", Branch::Craft, 2, 50.0, 10, &[WEAVING], 40.0, &[B(Bonus::CraftSpeed(CRAFT_SPEED_BONUS))]),
    exp("Territory marking", Branch::Defence, 1, 3.0, 6, &[FELLING], 0.0, &[U(BuildingKind::ScentMound), U(BuildingKind::GuardPost)]),
    // Forestry (`forestry`): planting and upkeep, right after Felling.
    exp(FORESTRY_TECH, Branch::Forest, 1, 20.0, 5, &[FELLING], 0.0, &[U(BuildingKind::ForesterLodge)]),
    exp(COPPICING_TECH, Branch::Forest, 2, 50.0, 8, &[FORESTRY], 0.0, &[]),
    exp("Tree nursery", Branch::Forest, 3, 80.0, 10, &[FORESTRY, WEAVING], 30.0, &[U(BuildingKind::TreeNursery)]),
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
    /// Lore not yet studied at an elders' den.
    Lore { have: f32, need: f32 },
    /// No elder alive.
    Elder,
    /// Only an achievement grants this tech; it has not happened yet.
    Achievement(Achievement),
    /// No tech unlocks this building at all (cannot happen for catalogue kinds).
    NoTech,
}

/// Lore one studying elder yields per day.
pub const LORE_PER_ELDER_DAY: f32 = 1.0;

/// Lore earned in `dt_days`: every complete elders' den counts the elders in its `Role::Study`
/// slots. An empty den, a den staffed by non-elders, or no den yields 0.
pub fn den_lore(buildings: &Buildings, work: &Work, colony: &Colony, dt_days: f32) -> f32 {
    let elders = work
        .places
        .iter()
        .filter(|p| p.kind == BuildingKind::EldersDen && buildings.buildings.iter().any(|b| b.id == p.building && b.is_complete()))
        .flat_map(|p| p.slots.iter())
        .filter(|s| s.role == Role::Study)
        .filter_map(|s| s.worker)
        .filter(|&w| colony.beaver(w).is_some_and(|b| b.stage == Stage::Elder))
        .count();
    elders as f32 * LORE_PER_ELDER_DAY * dt_days
}

#[derive(Clone, Debug, Default)]
pub struct Tech {
    unlocked: Vec<bool>,
    achieved: [bool; 3],
    lore: Vec<f32>,
    study_target: Option<TechId>,
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

    /// Lore studied so far towards `id`.
    pub fn lore(&self, id: TechId) -> f32 {
        self.lore.get(id.0).copied().unwrap_or(0.0)
    }

    pub fn study_target(&self) -> Option<TechId> {
        self.study_target
    }

    /// Chooses the tech elders study. Errors on an unknown tech, a known one, or one without a lore cost.
    pub fn set_study(&mut self, id: TechId) -> Result<(), String> {
        let t = TECHS.get(id.0).ok_or_else(|| format!("no such tech {}", id.0))?;
        if self.is_unlocked(id) {
            return Err(format!("{} is already known", t.name));
        }
        if t.lore <= 0.0 {
            return Err(format!("{} needs no study", t.name));
        }
        self.study_target = Some(id);
        Ok(())
    }

    /// Adds `lore` to the study target (capped at its cost). Returns false if lore was earned but
    /// no target is chosen (the lore is dropped), so callers can tell the player to pick one.
    pub fn study(&mut self, lore: f32) -> bool {
        let Some(id) = self.study_target else { return lore <= 0.0 };
        self.lore.resize(TECHS.len(), 0.0);
        self.lore[id.0] = (self.lore[id.0] + lore.max(0.0)).min(TECHS[id.0].lore);
        true
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
        if t.needs_elder && colony.count_stage(Stage::Elder) == 0 {
            gaps.push(Gap::Elder);
        }
        if t.lore > 0.0 && self.lore(id) < t.lore {
            gaps.push(Gap::Lore { have: self.lore(id), need: t.lore });
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
        self.lore.resize(TECHS.len(), 0.0);
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
                if self.study_target == Some(TechId(i)) {
                    self.study_target = None;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::buildings::{Building, BuildingId, Status};
    use crate::work::{Slot, Workplace};
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

    fn den_world(elder: bool) -> (Buildings, Work, Colony) {
        let mut c = colony_of(3);
        let id = c.beavers[0].id;
        if elder {
            c.beavers[0].stage = Stage::Elder;
        }
        let mut b = Buildings::default();
        b.buildings.push(Building { id: BuildingId(1), kind: EldersDen, pos: (0, 0), status: Status::Complete, recipe: None, output: 0.0 });
        let mut w = Work::default();
        let slots = vec![Slot { worker: Some(id), ..Slot::empty(Role::Study) }, Slot::empty(Role::Study)];
        w.places.push(Workplace::new(BuildingId(1), EldersDen, slots, None, None));
        (b, w, c)
    }

    #[test]
    fn den_with_elders_yields_research_and_an_empty_one_none() {
        let (b, w, c) = den_world(true);
        let lore = den_lore(&b, &w, &c, 10.0);
        assert_eq!(lore, 10.0 * LORE_PER_ELDER_DAY);
        let mut t = Tech::default();
        t.set_study(CANAL_DIGGING).unwrap();
        assert!(t.study(lore));
        assert_eq!(t.lore(CANAL_DIGGING), 10.0);
        // An empty den and a den staffed by a non-elder yield nothing.
        let (b, mut w, c) = den_world(true);
        w.places[0].slots[0].worker = None;
        assert_eq!(den_lore(&b, &w, &c, 10.0), 0.0);
        let (b, w, c) = den_world(false);
        assert_eq!(den_lore(&b, &w, &c, 10.0), 0.0);
    }

    #[test]
    fn canals_and_every_industry_building_need_den_research() {
        for k in [Canal, Dock, Lock, WaterWheel, Barge, Storehouse, Mill] {
            assert!(TECHS[unlock_index(k).unwrap()].lore > 0.0, "{k:?} needs lore");
        }
        // Even with every other need met, a canal stays locked until the lore is studied.
        let mut t = Tech::default();
        t.record(Achievement::FirstBurrow);
        let mut c = colony_of(30);
        for j in Job::ALL {
            c.experience[j.index()] = 500.0;
        }
        t.update(&c);
        assert!(t.allows(Lodge) && t.allows(QuarryCamp) && !t.allows(Canal));
        assert_eq!(t.missing(Canal, &c), vec![Gap::Lore { have: 0.0, need: 30.0 }]);
        t.set_study(CANAL_DIGGING).unwrap();
        t.study(30.0);
        t.update(&c);
        assert!(t.allows(Canal));
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
    fn study_rejects_bad_targets() {
        let mut t = Tech::default();
        assert!(t.set_study(TechId(999)).is_err());
        assert!(t.set_study(TechId(0)).is_err(), "no lore cost");
        assert!(!t.study(1.0), "lore with no target is reported");
    }

    #[test]
    fn forest_experience_sums_jobs_and_bonuses_apply() {
        let mut t = Tech::default();
        t.record(Achievement::FirstBurrow);
        let mut c = colony_of(10);
        c.experience[Job::Logger.index()] = 90.0;
        c.experience[Job::Forager.index()] = 40.0;
        t.update(&c);
        assert_eq!(t.storage_factor(), 1.0); // Forest lore still needs 30 lore
        t.set_study(TechId(14)).unwrap();
        t.study(30.0);
        t.update(&c);
        assert_eq!(t.storage_factor(), 1.25);
        assert_eq!(t.build_work_factor(), 1.0);
    }
}
