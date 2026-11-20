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

    pub fn experience(self, colony: &Colony) -> f32 {
        self.jobs().iter().map(|j| colony.experience[j.index()]).sum()
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Bonus {
    BuildWork(f32),
    CraftSpeed(f32),
    Storage(f32),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Effect {
    Unlock(BuildingKind),
    Bonus(Bonus),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TechId(pub usize);

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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Route {
    Achievement(Achievement),
    Experience,
}

#[derive(Clone, Copy, Debug)]
pub struct TechDef {
    pub name: &'static str,
    pub branch: Branch,
    pub tier: u8,
    pub route: Route,
    pub xp: f32,
    pub size: usize,
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

pub const FORESTRY_TECH: &str = "Forestry";
pub const COPPICING_TECH: &str = "Coppicing";

pub const TECHS: &[TechDef] = &[
    ach("Foraging", Branch::Forest, Achievement::FirstBurrow, &[U(BuildingKind::ForagerHut)]),
    ach("Felling", Branch::Forest, Achievement::FirstBurrow, &[U(BuildingKind::LoggingCamp), U(BuildingKind::MudPit)]),
    ach("Winter stores", Branch::Forest, Achievement::FirstWinter, &[U(BuildingKind::FoodCache)]),
    exp("Lodge building", Branch::Building, 1, 16.0, 5, &[FELLING], &[U(BuildingKind::Lodge)]),
    exp("Stonework", Branch::Forest, 2, 40.0, 6, &[FELLING], &[U(BuildingKind::QuarryCamp)]),
    exp("Weaving", Branch::Forest, 3, 60.0, 8, &[FELLING], &[U(BuildingKind::Workshop)]),
    exp("Canal digging", Branch::Water, 1, 10.0, 6, &[LODGE_BUILDING], &[U(BuildingKind::Canal)]),
    exp("Docks", Branch::Building, 1, 30.0, 8, &[LODGE_BUILDING], &[U(BuildingKind::Dock)]),
    exp("Flow control", Branch::Water, 2, 60.0, 12, &[DOCKS, STONEWORK], &[U(BuildingKind::Lock), U(BuildingKind::WaterWheel)]),
    exp("Barges", Branch::Building, 2, 60.0, 10, &[DOCKS, WEAVING], &[U(BuildingKind::Barge)]),
    exp("Grand storage", Branch::Building, 3, 100.0, 20, &[LODGE_BUILDING], &[U(BuildingKind::Storehouse)]),
    exp("Industrial milling", Branch::Craft, 3, 100.0, 24, &[WEAVING, FLOW_CONTROL], &[U(BuildingKind::Mill)]),
    exp("Master builders", Branch::Building, 2, 60.0, 10, &[LODGE_BUILDING], &[B(Bonus::BuildWork(BUILD_WORK_BONUS))]),
    exp("Dry stores", Branch::Forest, 4, 100.0, 10, &[STONEWORK], &[B(Bonus::Storage(STORAGE_BONUS))]),
    exp("Fine tools", Branch::Craft, 2, 50.0, 10, &[WEAVING], &[B(Bonus::CraftSpeed(CRAFT_SPEED_BONUS))]),
    exp("Territory marking", Branch::Defence, 1, 3.0, 6, &[FELLING], &[U(BuildingKind::ScentMound), U(BuildingKind::GuardPost)]),
    exp(FORESTRY_TECH, Branch::Forest, 1, 20.0, 5, &[FELLING], &[U(BuildingKind::ForesterLodge)]),
    exp(COPPICING_TECH, Branch::Forest, 2, 50.0, 8, &[FORESTRY], &[]),
    exp("Tree nursery", Branch::Forest, 3, 80.0, 10, &[FORESTRY, WEAVING], &[U(BuildingKind::TreeNursery)]),
];

pub fn unlock_index(kind: BuildingKind) -> Option<usize> {
    TECHS.iter().position(|t| t.effects.contains(&Effect::Unlock(kind)))
}

pub const BASE_BUILDINGS: [BuildingKind; 3] = [BuildingKind::Burrow, BuildingKind::Dam, BuildingKind::Warehouse];

#[derive(Clone, Debug, PartialEq)]
pub enum Gap {
    Research(&'static str),
    Size { have: usize, need: usize },
    Experience { branch: Branch, have: f32, need: f32 },
    Achievement(Achievement),
    Building(BuildingKind),
    NoTech,
}

#[derive(Clone, Debug, Default)]
pub struct Tech {
    unlocked: Vec<bool>,
    achieved: [bool; 3],
}

impl Tech {
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

    pub fn allows(&self, kind: BuildingKind) -> bool {
        BASE_BUILDINGS.contains(&kind)
            || self.unlocked_ids().any(|id| TECHS[id.0].effects.contains(&Effect::Unlock(kind)))
    }

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

    pub fn missing(&self, kind: BuildingKind, colony: &Colony) -> Vec<Gap> {
        if self.allows(kind) {
            return Vec::new();
        }
        match unlock_index(kind) {
            Some(i) => self.needs(TechId(i), colony),
            None => vec![Gap::NoTech],
        }
    }

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

