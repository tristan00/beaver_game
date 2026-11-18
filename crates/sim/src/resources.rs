//! Gatherable resource entities (owner: `resources`): individual trees, felled logs and branches,
//! bark, plants, mud banks and stone outcrops. Pure data with queries; agents act through `Task`s.
//!
//! Contract summary:
//! - `fell(id, work)` applies felling work to a standing `Tree`; at `FELL_WORK` it becomes `Felled`
//!   (logs, branches, bark) and stops blocking its cell.
//! - `take(id, want)` gathers logs, then branches, from `Felled`, or food/mud/stone from the rest.
//! - `strip(id, want)` takes bark (`Good::Food`) from `Felled` wood.
//! - `step(dt)` regrows plants on wet ground and removes used-up deposits and wood.
//! - Trees have an age and a stage (`crate::forestry`): `fell` yields by stage, saplings cannot be
//!   felled and do not block, and `forestry` ages, spreads, drowns and kills them day by day.

use crate::buildings::Good;
use crate::water::Cell;
use crate::work::Area;
use crate::worldgen::World;
use rand_core::{Rng, SeedableRng};
use rand_pcg::Pcg32;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ResourceId(pub u64);

/// What a map resource is; agents are assigned by kind.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ResourceKind {
    /// Standing tree: solid, fells into `Felled`.
    Tree,
    /// Felled tree: yields logs, branches and bark.
    Felled,
    /// Edible plant on wet ground; regrows.
    Plant,
    MudBank,
    StoneOutcrop,
}

impl ResourceKind {
    /// Goods a load of this kind can yield (`Tree` yields nothing until felled). Bark counts as
    /// `Food` and is taken with `ResourceWorld::strip`, not `take`.
    pub fn yields(self) -> &'static [Good] {
        match self {
            ResourceKind::Tree => &[],
            ResourceKind::Felled => &[Good::Logs, Good::Sticks, Good::Food],
            ResourceKind::Plant => &[Good::Food],
            ResourceKind::MudBank => &[Good::Mud],
            ResourceKind::StoneOutcrop => &[Good::Stone],
        }
    }
    /// True if the entity blocks navigation.
    pub fn is_solid(self) -> bool {
        matches!(self, ResourceKind::Tree | ResourceKind::StoneOutcrop)
    }
}

/// Felling work (work days) a standing tree needs.
pub const FELL_WORK: f32 = 2.0;
/// What a felled mature tree leaves behind (other stages: `forestry::TreeStage::fell_yield`).
pub const TREE_LOGS: f32 = 3.0;
pub const TREE_BRANCHES: f32 = 4.0;
pub const TREE_BARK: f32 = 2.0;
/// Plant stock per cell and regrowth per day (a harvested plant is full again after 8 days).
pub const PLANT_MAX: f32 = 4.0;
pub const PLANT_REGROW_PER_DAY: f32 = 0.5;
/// Minimum soil moisture for a plant to grow.
pub const WET_MOISTURE: f32 = 0.5;
/// Minimum surface slope (world units per cell) for a treeless dry cell to be a stone outcrop.
pub const OUTCROP_SLOPE: f32 = 0.6;
pub const MUD_AMOUNT: f32 = 20.0;
pub const STONE_AMOUNT: f32 = 30.0;
/// Chance per eligible cell of holding a plant / mud bank / stone deposit.
const PLANT_CHANCE: f32 = 0.25;
const MUD_CHANCE: f32 = 0.25;
const STONE_CHANCE: f32 = 0.2;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResourceError {
    NoSuchResource,
    /// The operation does not apply to this kind (e.g. felling a plant).
    WrongKind,
    /// Work, amount or time was negative or not finite.
    InvalidAmount,
    /// Plants only grow on wet cells.
    NotWet,
    /// A sapling cannot be felled yet.
    TooYoung,
}

/// A map resource. `branches`/`bark` are only non-zero on `Felled`.
#[derive(Clone, Debug, PartialEq)]
pub struct Resource {
    pub id: ResourceId,
    pub kind: ResourceKind,
    pub cell: Cell,
    /// Units left to gather. `Tree`: felling work still needed. `Felled`: logs left.
    pub amount: f32,
    /// Plants regrow up to this amount.
    pub max_amount: f32,
    /// `Felled` only: branches (`Good::Sticks`) left.
    pub branches: f32,
    /// `Felled` only: bark (`Good::Food`) left until stripped.
    pub bark: f32,
    /// `Plant` only: whether the ground is wet, so it regrows.
    pub wet: bool,
    /// `Tree` only: growth age in days (`forestry::TreeStage`); for a snag, days since it died.
    pub age_days: f32,
    /// `Tree` only: a dead standing tree (sticks only), which rots away and frees the cell.
    pub snag: bool,
    /// `Tree` only: days of a forester's tending left (it grows faster meanwhile).
    pub tended_days: f32,
}

impl Resource {
    /// Whether it blocks navigation: solid kinds, except saplings (beavers step over them).
    pub fn is_solid(&self) -> bool {
        self.kind.is_solid() && !(self.kind == ResourceKind::Tree && self.stage() == Some(crate::forestry::TreeStage::Sapling))
    }
    /// Whether anything is left to gather (a standing tree always has felling work left).
    pub fn has_yield(&self) -> bool {
        match self.kind {
            ResourceKind::Felled => self.amount > 0.0 || self.branches > 0.0 || self.bark > 0.0,
            _ => self.amount > 0.0,
        }
    }
}

/// All resource entities.
#[derive(Clone, Debug, Default)]
pub struct ResourceWorld {
    pub items: Vec<Resource>,
    next_id: u64,
}

fn is_water(world: &World, (x, y): Cell) -> bool {
    world.river.get(x, y).copied().unwrap_or(false) || world.lake.get(x, y).copied().unwrap_or(false)
}

/// Bank cell: not water itself but 4-adjacent to water.
pub fn is_bank(world: &World, cell: Cell) -> bool {
    let (x, y) = cell;
    if !world.moisture.in_bounds(x, y) || is_water(world, cell) {
        return false;
    }
    let near = [(x.wrapping_sub(1), y), (x + 1, y), (x, y.wrapping_sub(1)), (x, y + 1)];
    near.iter().any(|&(nx, ny)| world.moisture.in_bounds(nx, ny) && is_water(world, (nx, ny)))
}

/// Outcrop cell: dry, steep, treeless rock.
pub fn is_outcrop(world: &World, cell: Cell) -> bool {
    let (x, y) = cell;
    world.slope.get(x, y).is_some_and(|&s| s >= OUTCROP_SLOPE)
        && !is_water(world, cell)
        && world.forest.get(x, y).is_some_and(|t| t.is_none())
}

/// Wet ground: moist, not under water.
pub fn is_wet(world: &World, (x, y): Cell) -> bool {
    !is_water(world, (x, y)) && world.moisture.get(x, y).is_some_and(|&m| m >= WET_MOISTURE)
}

impl ResourceWorld {
    /// Deterministic placement for a seed: a tree on every forest cell, stone on outcrops, mud on
    /// banks, plants on wet ground. Same `(seed, world)` gives the same placement.
    pub fn seeded(seed: u64, world: &World) -> Self {
        let mut rng = Pcg32::seed_from_u64(seed ^ 0x5E50_0C35);
        let mut out = Self::default();
        let mut chance = |p: f32| (rng.next_u32() >> 8) as f32 / (1u32 << 24) as f32 <= p;
        for y in 0..world.size {
            for x in 0..world.size {
                let cell = (x, y);
                if world.forest.get(x, y).is_some_and(|t| t.is_some()) {
                    out.add_tree(cell, crate::forestry::start_age(seed, cell));
                } else if is_outcrop(world, cell) {
                    if chance(STONE_CHANCE) {
                        out.add(ResourceKind::StoneOutcrop, cell, STONE_AMOUNT);
                    }
                } else if is_bank(world, cell) && chance(MUD_CHANCE) {
                    out.add(ResourceKind::MudBank, cell, MUD_AMOUNT);
                } else if is_wet(world, cell) && chance(PLANT_CHANCE) {
                    let id = out.add(ResourceKind::Plant, cell, PLANT_MAX);
                    out.items.iter_mut().find(|r| r.id == id).expect("just added").wet = true;
                }
            }
        }
        out
    }

    pub fn get(&self, id: ResourceId) -> Option<&Resource> {
        self.items.iter().find(|r| r.id == id)
    }
    fn get_mut(&mut self, id: ResourceId) -> Result<&mut Resource, ResourceError> {
        self.items.iter_mut().find(|r| r.id == id).ok_or(ResourceError::NoSuchResource)
    }
    /// All resources on `cell`.
    pub fn at(&self, cell: Cell) -> impl Iterator<Item = &Resource> {
        self.items.iter().filter(move |r| r.cell == cell)
    }
    /// True if a standing tree (not a sapling) or stone outcrop occupies `cell`.
    pub fn blocks(&self, cell: Cell) -> bool {
        self.at(cell).any(|r| r.is_solid() && r.has_yield())
    }

    /// Nearest resource of `kind` to `from` (squared distance, ties by id; optionally inside
    /// `area`) that still has something to gather. A standing tree counts as having felling work.
    pub fn nearest(&self, kind: ResourceKind, from: Cell, area: Option<&Area>) -> Option<ResourceId> {
        let d = |c: Cell| c.0.abs_diff(from.0).pow(2) + c.1.abs_diff(from.1).pow(2);
        self.items
            .iter()
            .filter(|r| r.kind == kind && r.has_yield() && area.is_none_or(|a| a.contains(r.cell)))
            .min_by_key(|r| (d(r.cell), r.id))
            .map(|r| r.id)
    }

    /// Applies `work` days of felling to a standing tree. Returns `Ok(true)` when it falls: the
    /// tree becomes `Felled` with logs, branches and bark and no longer blocks its cell.
    pub fn fell(&mut self, id: ResourceId, work: f32) -> Result<bool, ResourceError> {
        if !work.is_finite() || work < 0.0 {
            return Err(ResourceError::InvalidAmount);
        }
        let r = self.get_mut(id)?;
        if r.kind != ResourceKind::Tree {
            return Err(ResourceError::WrongKind);
        }
        let Some((logs, branches, bark)) = r.stage().and_then(crate::forestry::TreeStage::fell_yield) else {
            return Err(ResourceError::TooYoung);
        };
        r.amount -= work;
        if r.amount > 0.0 {
            return Ok(false);
        }
        r.kind = ResourceKind::Felled;
        r.amount = logs;
        r.max_amount = logs;
        r.branches = branches;
        r.bark = bark;
        Ok(true)
    }

    /// Strips bark from felled wood; bark is food. Returns `(Good::Food, taken)` (0 when none left).
    pub fn strip(&mut self, id: ResourceId, want: f32) -> Result<(Good, f32), ResourceError> {
        if !want.is_finite() || want < 0.0 {
            return Err(ResourceError::InvalidAmount);
        }
        let r = self.get_mut(id)?;
        if r.kind != ResourceKind::Felled {
            return Err(ResourceError::WrongKind);
        }
        let got = want.min(r.bark);
        r.bark -= got;
        Ok((Good::Food, got))
    }

    /// Takes up to `want` from a resource and returns what was taken. `None` if the resource does
    /// not exist, is a standing tree (use `fell`), or has nothing left. Felled wood gives logs
    /// first, then branches; bark is taken with `strip`.
    pub fn take(&mut self, id: ResourceId, want: f32) -> Option<(Good, f32)> {
        if !want.is_finite() || want <= 0.0 {
            return None;
        }
        let r = self.get_mut(id).ok()?;
        let (good, slot) = match r.kind {
            ResourceKind::Tree => return None,
            ResourceKind::Felled if r.amount > 0.0 => (Good::Logs, &mut r.amount),
            ResourceKind::Felled => (Good::Sticks, &mut r.branches),
            ResourceKind::Plant => (Good::Food, &mut r.amount),
            ResourceKind::MudBank => (Good::Mud, &mut r.amount),
            ResourceKind::StoneOutcrop => (Good::Stone, &mut r.amount),
        };
        let got = want.min(*slot);
        if got <= 0.0 {
            return None;
        }
        *slot -= got;
        Some((good, got))
    }

    /// Plant regrowth on wet ground, then removal of used-up deposits and wood. Plants stay when
    /// empty so they can regrow. Errors if `dt_days` is not finite and >= 0.
    pub fn step(&mut self, dt_days: f32) -> Result<(), ResourceError> {
        if !dt_days.is_finite() || dt_days < 0.0 {
            return Err(ResourceError::InvalidAmount);
        }
        for r in self.items.iter_mut().filter(|r| r.kind == ResourceKind::Plant && r.wet) {
            r.amount = (r.amount + PLANT_REGROW_PER_DAY * dt_days).min(r.max_amount);
        }
        self.items.retain(|r| r.kind == ResourceKind::Plant || r.has_yield());
        Ok(())
    }

    /// Adds a resource with `amount` (also its maximum). Plants added this way do not regrow;
    /// use `add_plant` for that. A tree added this way is mature (`forestry::DEFAULT_TREE_AGE`);
    /// use `add_tree` for another age.
    pub fn add(&mut self, kind: ResourceKind, cell: Cell, amount: f32) -> ResourceId {
        self.next_id += 1;
        let id = ResourceId(self.next_id);
        let age_days = if kind == ResourceKind::Tree { crate::forestry::DEFAULT_TREE_AGE } else { 0.0 };
        self.items.push(Resource { id, kind, cell, amount, max_amount: amount, branches: 0.0, bark: 0.0, wet: false, age_days, snag: false, tended_days: 0.0 });
        id
    }

    /// Adds a standing tree of growth age `age_days` (a sapling below `forestry::YOUNG_AGE`).
    pub fn add_tree(&mut self, cell: Cell, age_days: f32) -> ResourceId {
        let id = self.add(ResourceKind::Tree, cell, FELL_WORK);
        self.items.last_mut().expect("just added").age_days = age_days.max(0.0);
        id
    }

    /// Adds a regrowing plant; only wet cells of `world` can hold one.
    pub fn add_plant(&mut self, world: &World, cell: Cell) -> Result<ResourceId, ResourceError> {
        if !is_wet(world, cell) {
            return Err(ResourceError::NotWet);
        }
        let id = self.add(ResourceKind::Plant, cell, PLANT_MAX);
        self.get_mut(id)?.wet = true;
        Ok(id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::worldgen::WorldGen;

    fn world(seed: u64) -> World {
        WorldGen::new(seed).generate().unwrap()
    }

    fn cells(w: &World) -> impl Iterator<Item = Cell> + '_ {
        (0..w.size * w.size).map(|i| (i % w.size, i / w.size))
    }

    #[test]
    fn tree_blocks_until_felled_then_yields() {
        let mut w = ResourceWorld::default();
        let t = w.add(ResourceKind::Tree, (3, 3), FELL_WORK);
        assert!(w.blocks((3, 3)));
        assert_eq!(w.take(t, 1.0), None, "standing tree yields nothing");
        assert_eq!(w.fell(t, FELL_WORK / 2.0), Ok(false));
        assert!(w.blocks((3, 3)), "still an obstacle mid-felling");
        assert_eq!(w.fell(t, FELL_WORK / 2.0), Ok(true));
        assert!(!w.blocks((3, 3)), "felling frees the cell");
        assert_eq!(w.get(t).unwrap().kind, ResourceKind::Felled);
        assert_eq!(w.take(t, 10.0), Some((Good::Logs, TREE_LOGS)));
        assert_eq!(w.take(t, 10.0), Some((Good::Sticks, TREE_BRANCHES)));
        assert_eq!(w.fell(t, 1.0), Err(ResourceError::WrongKind));
        assert_eq!(w.fell(t, -1.0), Err(ResourceError::InvalidAmount));
        assert_eq!(w.fell(ResourceId(99), 1.0), Err(ResourceError::NoSuchResource));
    }

    #[test]
    fn stripping_yields_bark_food_until_gone() {
        let mut w = ResourceWorld::default();
        let t = w.add(ResourceKind::Tree, (1, 1), 1.0);
        assert_eq!(w.strip(t, 1.0), Err(ResourceError::WrongKind), "cannot strip a standing tree");
        w.fell(t, 5.0).unwrap();
        assert_eq!(w.strip(t, 1.5), Ok((Good::Food, 1.5)));
        assert_eq!(w.strip(t, 5.0), Ok((Good::Food, TREE_BARK - 1.5)));
        assert_eq!(w.strip(t, 1.0), Ok((Good::Food, 0.0)));
        assert_eq!(w.strip(ResourceId(999), 1.0), Err(ResourceError::NoSuchResource));
    }

    #[test]
    fn used_up_wood_and_deposits_are_removed() {
        let mut w = ResourceWorld::default();
        let t = w.add(ResourceKind::Tree, (1, 1), 1.0);
        let m = w.add(ResourceKind::MudBank, (2, 2), 5.0);
        w.fell(t, 1.0).unwrap();
        w.take(t, 100.0);
        w.take(t, 100.0);
        w.step(0.0).unwrap();
        assert!(w.get(t).is_some(), "bark still left");
        w.strip(t, 100.0).unwrap();
        assert_eq!(w.take(m, 3.0), Some((Good::Mud, 3.0)));
        assert_eq!(w.take(m, 3.0), Some((Good::Mud, 2.0)));
        assert_eq!(w.take(m, 3.0), None);
        w.step(0.0).unwrap();
        assert!(w.get(t).is_none() && w.get(m).is_none(), "deposits run out");
        assert_eq!(w.nearest(ResourceKind::MudBank, (0, 0), None), None);
    }

    #[test]
    fn plants_only_on_wet_cells_and_regrow() {
        let world = world(7);
        let wet = cells(&world).find(|&c| is_wet(&world, c)).unwrap();
        let dry = cells(&world).find(|&c| !is_wet(&world, c)).unwrap();
        let mut w = ResourceWorld::default();
        assert_eq!(w.add_plant(&world, dry), Err(ResourceError::NotWet));
        let p = w.add_plant(&world, wet).unwrap();
        assert_eq!(w.take(p, 100.0), Some((Good::Food, PLANT_MAX)));
        w.step(1.0).unwrap();
        assert_eq!(w.get(p).unwrap().amount, PLANT_REGROW_PER_DAY, "empty plants stay and regrow");
        w.step(100.0).unwrap();
        assert_eq!(w.get(p).unwrap().amount, PLANT_MAX, "regrowth is capped");
        let d = w.add(ResourceKind::Plant, dry, 1.0);
        w.take(d, 1.0);
        w.step(10.0).unwrap();
        assert_eq!(w.get(d).unwrap().amount, 0.0, "no regrowth off wet ground");
        assert_eq!(w.step(f32::NAN), Err(ResourceError::InvalidAmount));
    }

    #[test]
    fn nearest_respects_kind_area_and_depletion() {
        let mut w = ResourceWorld::default();
        let far = w.add(ResourceKind::StoneOutcrop, (10, 10), 1.0);
        let near = w.add(ResourceKind::StoneOutcrop, (2, 1), 1.0);
        w.add(ResourceKind::MudBank, (0, 1), 1.0);
        assert_eq!(w.nearest(ResourceKind::StoneOutcrop, (0, 0), None), Some(near));
        let area = Area { min: (8, 8), max: (12, 12) };
        assert_eq!(w.nearest(ResourceKind::StoneOutcrop, (0, 0), Some(&area)), Some(far));
        w.take(near, 1.0);
        assert_eq!(w.nearest(ResourceKind::StoneOutcrop, (0, 0), None), Some(far));
    }

    #[test]
    fn seeded_placement_rules_and_determinism() {
        let (wa, wb) = (world(11), world(12));
        let a = ResourceWorld::seeded(11, &wa);
        let key = |r: &ResourceWorld| r.items.iter().map(|i| (i.kind as u8, i.cell)).collect::<Vec<_>>();
        assert_eq!(key(&a), key(&ResourceWorld::seeded(11, &wa)), "same seed, same placement");
        let b = ResourceWorld::seeded(12, &wb);
        assert_ne!(key(&a), key(&b), "different seeds differ");
        let count = |k| a.items.iter().filter(|i| i.kind == k).count();
        assert!(count(ResourceKind::MudBank) > 0, "world has mud");
        assert!(count(ResourceKind::StoneOutcrop) > 0, "world has stone");
        assert!(count(ResourceKind::Tree) > 0 && count(ResourceKind::Plant) > 0);
        let water = |x: usize, y: usize| {
            wa.river.get(x, y).copied().unwrap_or(false) || wa.lake.get(x, y).copied().unwrap_or(false)
        };
        for r in &a.items {
            let (x, y) = r.cell;
            match r.kind {
                ResourceKind::MudBank => assert!(
                    water(x + 1, y) || water(x, y + 1) || (x > 0 && water(x - 1, y)) || (y > 0 && water(x, y - 1)),
                    "mud {:?} must touch water",
                    r.cell
                ),
                ResourceKind::StoneOutcrop => assert!(is_outcrop(&wa, r.cell), "stone {:?} off outcrop", r.cell),
                ResourceKind::Plant => assert!(is_wet(&wa, r.cell) && r.wet),
                ResourceKind::Tree => assert!(wa.forest.get(x, y).unwrap().is_some()),
                ResourceKind::Felled => panic!("no felled wood at start"),
            }
        }
    }
}
