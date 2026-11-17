//! Tech tree: six branches whose techs unlock from the colony's branch experience.
//!
//! Colony experience is tracked per job (`Colony::experience`); each branch sums the experience of
//! the jobs mapped to it by `Branch::jobs`. A tech unlocks once its branch experience reaches its
//! threshold and the previous tier of the same branch is unlocked.

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

#[derive(Clone, Copy, Debug)]
pub struct TechDef {
    pub name: &'static str,
    pub branch: Branch,
    /// 1-based tier within the branch; tier n needs tier n-1 unlocked.
    pub tier: u8,
    /// Branch experience required.
    pub xp: f32,
    pub effects: &'static [Effect],
}

use crate::tuning::tech::*;
use Effect::{Bonus as B, Unlock as U};

/// Every tech, indexed by `TechId`.
pub const TECHS: &[TechDef] = &[
    TechDef { name: "Flow Control", branch: Branch::Water, tier: 1, xp: TIER1_XP, effects: &[U(BuildingKind::Lock), U(BuildingKind::WaterWheel)] },
    TechDef { name: "Wetland Husbandry", branch: Branch::Forest, tier: 1, xp: TIER1_XP, effects: &[U(BuildingKind::FoodCache)] },
    TechDef { name: "Forest Lore", branch: Branch::Forest, tier: 2, xp: TIER2_XP, effects: &[B(Bonus::Storage(STORAGE_BONUS))] },
    TechDef { name: "Joinery", branch: Branch::Building, tier: 1, xp: TIER1_XP, effects: &[U(BuildingKind::Warehouse)] },
    TechDef { name: "Master Builders", branch: Branch::Building, tier: 2, xp: TIER2_XP, effects: &[B(Bonus::BuildWork(BUILD_WORK_BONUS))] },
    TechDef { name: "Weaving", branch: Branch::Craft, tier: 1, xp: TIER1_XP, effects: &[U(BuildingKind::Workshop)] },
    TechDef { name: "Fine Tools", branch: Branch::Craft, tier: 2, xp: TIER2_XP, effects: &[B(Bonus::CraftSpeed(CRAFT_SPEED_BONUS))] },
    TechDef { name: "Docks", branch: Branch::Transport, tier: 1, xp: TIER1_XP, effects: &[U(BuildingKind::Dock)] },
    TechDef { name: "Barges", branch: Branch::Transport, tier: 2, xp: TIER2_XP, effects: &[U(BuildingKind::Barge)] },
    TechDef { name: "Territory Marking", branch: Branch::Defence, tier: 1, xp: TIER1_XP, effects: &[U(BuildingKind::ScentMound), U(BuildingKind::GuardPost)] },
    TechDef { name: "Grand Storage", branch: Branch::Building, tier: 3, xp: TIER3_XP, effects: &[U(BuildingKind::Storehouse)] },
    TechDef { name: "Industrial Milling", branch: Branch::Craft, tier: 3, xp: TIER3_XP, effects: &[U(BuildingKind::Mill)] },
];

/// Index into `TECHS` of the tech that unlocks `kind`, if any.
pub fn unlock_index(kind: BuildingKind) -> Option<usize> {
    TECHS.iter().position(|t| t.effects.contains(&Effect::Unlock(kind)))
}

/// Buildings available without any tech.
pub const BASE_BUILDINGS: [BuildingKind; 8] = [
    BuildingKind::Dam, BuildingKind::Lodge, BuildingKind::Burrow, BuildingKind::Canal,
    BuildingKind::LoggingCamp, BuildingKind::ForagerHut, BuildingKind::MudPit, BuildingKind::QuarryCamp,
];

#[derive(Clone, Debug, Default)]
pub struct Tech {
    unlocked: Vec<bool>,
}

impl Tech {
    /// Unlocks every tech (tests and scripted scenes).
    pub fn unlock_all(&mut self) {
        self.unlocked = vec![true; TECHS.len()];
    }

    pub fn is_unlocked(&self, id: TechId) -> bool {
        self.unlocked.get(id.0).copied().unwrap_or(false)
    }

    pub fn unlocked_ids(&self) -> impl Iterator<Item = TechId> + '_ {
        (0..TECHS.len()).map(TechId).filter(|&i| self.is_unlocked(i))
    }

    /// Whether `kind` may be built: base buildings always, others once a tech unlocks them.
    pub fn allows(&self, kind: BuildingKind) -> bool {
        BASE_BUILDINGS.contains(&kind)
            || self.unlocked_ids().any(|id| TECHS[id.0].effects.contains(&Effect::Unlock(kind)))
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

    /// Unlocks every tech whose experience threshold and prerequisite tier are met.
    /// Returns the newly unlocked techs.
    pub fn update(&mut self, colony: &Colony) -> Vec<TechId> {
        self.unlocked.resize(TECHS.len(), false);
        let mut new = Vec::new();
        // TECHS lists tiers in ascending order per branch, so one pass resolves chains.
        for (i, t) in TECHS.iter().enumerate() {
            if self.unlocked[i] || t.xp > t.branch.experience(colony) {
                continue;
            }
            let prereq = t.tier <= 1
                || TECHS.iter().enumerate().any(|(j, p)| p.branch == t.branch && p.tier == t.tier - 1 && self.unlocked[j]);
            if prereq {
                self.unlocked[i] = true;
                new.push(TechId(i));
            }
        }
        new
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn experience_unlocks_branch_tech() {
        let mut t = Tech::default();
        let mut c = Colony::new(1);
        assert!(!t.allows(BuildingKind::Workshop));
        assert!(t.allows(BuildingKind::Dam));
        c.experience[Job::Crafter.index()] = 2.0;
        assert!(t.update(&c).is_empty());
        c.experience[Job::Crafter.index()] = 3.5;
        let new = t.update(&c);
        assert_eq!(new.len(), 1);
        assert!(t.allows(BuildingKind::Workshop));
        // other branches stay locked
        assert!(!t.allows(BuildingKind::Warehouse));
        assert!(!t.allows(BuildingKind::WaterWheel));
    }

    #[test]
    fn tier_two_needs_tier_one_and_more_xp() {
        let mut t = Tech::default();
        let mut c = Colony::new(1);
        c.experience[Job::Hauler.index()] = 20.0;
        let new = t.update(&c);
        // both tiers resolve in one pass because tier 1 is listed first
        assert_eq!(new.len(), 2);
        assert!(t.allows(BuildingKind::Dock) && t.allows(BuildingKind::Barge));
        assert_eq!(t.update(&c).len(), 0);
    }

    #[test]
    fn forest_experience_sums_jobs_and_bonuses_apply() {
        let mut t = Tech::default();
        let mut c = Colony::new(1);
        c.experience[Job::Logger.index()] = 9.0;
        c.experience[Job::Forager.index()] = 7.0;
        t.update(&c);
        assert_eq!(t.storage_factor(), 1.25);
        assert_eq!(t.build_work_factor(), 1.0);
    }
}
