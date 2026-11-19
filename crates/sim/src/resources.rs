use crate::buildings::Good;
use crate::water::Cell;
use crate::work::Area;
use crate::worldgen::World;
use rand_core::{Rng, SeedableRng};
use rand_pcg::Pcg32;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ResourceId(pub u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ResourceKind {
    Tree,
    Felled,
    Plant,
    MudBank,
    StoneOutcrop,
}

impl ResourceKind {
    pub fn yields(self) -> &'static [Good] {
        match self {
            ResourceKind::Tree => &[],
            ResourceKind::Felled => &[Good::Logs, Good::Sticks, Good::Food],
            ResourceKind::Plant => &[Good::Food],
            ResourceKind::MudBank => &[Good::Mud],
            ResourceKind::StoneOutcrop => &[Good::Stone],
        }
    }
    pub fn is_solid(self) -> bool {
        matches!(self, ResourceKind::Tree | ResourceKind::StoneOutcrop)
    }
}

pub const FELL_WORK: f32 = 2.0;
pub const TREE_LOGS: f32 = 3.0;
pub const TREE_BRANCHES: f32 = 4.0;
pub const TREE_BARK: f32 = 2.0;
pub const PLANT_MAX: f32 = 4.0;
pub const PLANT_REGROW_PER_DAY: f32 = 0.5;
pub const WET_MOISTURE: f32 = 0.5;
pub const OUTCROP_SLOPE: f32 = 0.6;
pub const MUD_AMOUNT: f32 = 20.0;
pub const STONE_AMOUNT: f32 = 30.0;
const PLANT_CHANCE: f32 = 0.25;
pub const STONE_DEPOSITS: usize = 3;
pub const STONE_DEPOSIT_RADIUS: f32 = 8.0;
pub const STONE_DEPOSIT_AMOUNT: f32 = 40.0;
const STONE_DEPOSIT_FILL: f32 = 0.45;
pub const CLAY_BEDS: usize = 4;
pub const CLAY_BED_RADIUS: f32 = 10.0;
pub const CLAY_AMOUNT: f32 = 30.0;
const CLAY_FILL: f32 = 0.8;
const STRAY_MUD_CHANCE: f32 = 0.01;
pub const STRAY_MUD_AMOUNT: f32 = 10.0;
pub const DEPOSIT_SPACING: f32 = 100.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResourceError {
    NoSuchResource,
    WrongKind,
    InvalidAmount,
    NotWet,
    TooYoung,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Resource {
    pub id: ResourceId,
    pub kind: ResourceKind,
    pub cell: Cell,
    pub amount: f32,
    pub max_amount: f32,
    pub branches: f32,
    pub bark: f32,
    pub wet: bool,
    pub age_days: f32,
    pub snag: bool,
    pub tended_days: f32,
}

impl Resource {
    pub fn is_solid(&self) -> bool {
        self.kind.is_solid() && !(self.kind == ResourceKind::Tree && self.stage() == Some(crate::forestry::TreeStage::Sapling))
    }
    pub fn has_yield(&self) -> bool {
        match self.kind {
            ResourceKind::Felled => self.amount > 0.0 || self.branches > 0.0 || self.bark > 0.0,
            _ => self.amount > 0.0,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct ResourceWorld {
    pub items: Vec<Resource>,
    next_id: u64,
}

fn is_water(world: &World, (x, y): Cell) -> bool {
    world.river.get(x, y).copied().unwrap_or(false) || world.lake.get(x, y).copied().unwrap_or(false) || world.lake_level.get(x, y).is_some_and(|l| l.is_finite())
}

pub fn is_bank(world: &World, cell: Cell) -> bool {
    let (x, y) = cell;
    if !world.moisture.in_bounds(x, y) || is_water(world, cell) {
        return false;
    }
    let near = [(x.wrapping_sub(1), y), (x + 1, y), (x, y.wrapping_sub(1)), (x, y + 1)];
    near.iter().any(|&(nx, ny)| world.moisture.in_bounds(nx, ny) && is_water(world, (nx, ny)))
}

pub fn is_outcrop(world: &World, cell: Cell) -> bool {
    let (x, y) = cell;
    world.slope.get(x, y).is_some_and(|&s| s >= OUTCROP_SLOPE)
        && !is_water(world, cell)
        && world.forest.get(x, y).is_some_and(|t| t.is_none())
}

pub fn is_wet(world: &World, (x, y): Cell) -> bool {
    !is_water(world, (x, y)) && world.moisture.get(x, y).is_some_and(|&m| m >= WET_MOISTURE)
}

impl ResourceWorld {
    pub fn seeded(seed: u64, world: &World) -> Self {
        let mut rng = Pcg32::seed_from_u64(seed ^ 0x5E50_0C35);
        let mut out = Self::default();
        let mut unit = move || (rng.next_u32() >> 8) as f32 / (1u32 << 24) as f32;
        let n = world.size;
        let treeless = |x: usize, y: usize| world.forest.get(x, y).is_some_and(|t| t.is_none());
        let rock_ok = |c: Cell| !is_water(world, c) && treeless(c.0, c.1);
        let pick = |cands: Vec<(f32, Cell)>, count: usize, apart: &[Cell]| {
            let mut cands = cands;
            cands.sort_by(|a, b| b.0.total_cmp(&a.0).then(a.1.cmp(&b.1)));
            let mut chosen: Vec<Cell> = Vec::new();
            for (_, c) in cands {
                let far = |o: &Cell| ((c.0 as f32 - o.0 as f32).powi(2) + (c.1 as f32 - o.1 as f32).powi(2)).sqrt() >= DEPOSIT_SPACING;
                if chosen.iter().chain(apart).all(far) {
                    chosen.push(c);
                    if chosen.len() == count {
                        break;
                    }
                }
            }
            chosen
        };
        let disc = |c: Cell, r: f32| {
            let ri = r.ceil() as isize;
            let mut v = Vec::new();
            for dy in -ri..=ri {
                for dx in -ri..=ri {
                    let (x, y) = (c.0 as isize + dx, c.1 as isize + dy);
                    if x >= 0 && y >= 0 && (x as usize) < n && (y as usize) < n && ((dx * dx + dy * dy) as f32) <= r * r {
                        v.push((x as usize, y as usize));
                    }
                }
            }
            v
        };
        let mut cands = Vec::new();
        for y in (2..n.saturating_sub(2)).step_by(2) {
            for x in (2..n.saturating_sub(2)).step_by(2) {
                if !rock_ok((x, y)) {
                    continue;
                }
                let room = disc((x, y), STONE_DEPOSIT_RADIUS).into_iter().filter(|&c| rock_ok(c)).count();
                if room < 12 {
                    continue;
                }
                let slope = world.slope.get(x, y).copied().unwrap_or(0.0);
                let h = world.height.get(x, y).copied().unwrap_or(0.0);
                cands.push((slope * 2.0 + h * 0.05 + unit() * 0.5, (x, y)));
            }
        }
        let stone_at = pick(cands, STONE_DEPOSITS, &[]);
        let mut stone = std::collections::BTreeSet::new();
        for &c in &stone_at {
            for cell in disc(c, STONE_DEPOSIT_RADIUS) {
                if rock_ok(cell) && (cell == c || unit() < STONE_DEPOSIT_FILL) {
                    stone.insert(cell);
                }
            }
        }
        let mut cands = Vec::new();
        for y in 0..n {
            for x in 0..n {
                if is_bank(world, (x, y)) && treeless(x, y) && !stone.contains(&(x, y)) {
                    cands.push((unit(), (x, y)));
                }
            }
        }
        let beds = pick(cands, CLAY_BEDS, &stone_at);
        let in_bed = |c: Cell| beds.iter().any(|b| ((c.0 as f32 - b.0 as f32).powi(2) + (c.1 as f32 - b.1 as f32).powi(2)).sqrt() <= CLAY_BED_RADIUS);
        for y in 0..n {
            for x in 0..n {
                let cell = (x, y);
                if world.forest.get(x, y).is_some_and(|t| t.is_some()) {
                    out.add_tree(cell, crate::forestry::start_age(seed, cell));
                } else if stone.contains(&cell) {
                    out.add(ResourceKind::StoneOutcrop, cell, STONE_DEPOSIT_AMOUNT);
                } else if is_bank(world, cell) && in_bed(cell) {
                    if unit() < CLAY_FILL {
                        out.add(ResourceKind::MudBank, cell, CLAY_AMOUNT);
                    }
                } else if is_bank(world, cell) && unit() < STRAY_MUD_CHANCE {
                    out.add(ResourceKind::MudBank, cell, STRAY_MUD_AMOUNT);
                } else if is_wet(world, cell) && unit() < PLANT_CHANCE {
                    let id = out.add(ResourceKind::Plant, cell, PLANT_MAX);
                    out.items.iter_mut().find(|r| r.id == id).expect("just added").wet = true;
                }
            }
        }
        out
    }

    fn index_of(&self, id: ResourceId) -> Option<usize> {
        match self.items.binary_search_by_key(&id, |r| r.id) {
            Ok(i) => Some(i),
            Err(_) => self.items.iter().position(|r| r.id == id),
        }
    }
    pub fn get(&self, id: ResourceId) -> Option<&Resource> {
        self.index_of(id).map(|i| &self.items[i])
    }
    pub(crate) fn get_mut(&mut self, id: ResourceId) -> Result<&mut Resource, ResourceError> {
        let i = self.index_of(id).ok_or(ResourceError::NoSuchResource)?;
        Ok(&mut self.items[i])
    }
    pub fn at(&self, cell: Cell) -> impl Iterator<Item = &Resource> {
        self.items.iter().filter(move |r| r.cell == cell)
    }
    pub fn blocks(&self, cell: Cell) -> bool {
        self.at(cell).any(|r| r.is_solid() && r.has_yield())
    }

    pub fn nearest(&self, kind: ResourceKind, from: Cell, area: Option<&Area>) -> Option<ResourceId> {
        let d = |c: Cell| c.0.abs_diff(from.0).pow(2) + c.1.abs_diff(from.1).pow(2);
        self.items
            .iter()
            .filter(|r| r.kind == kind && r.has_yield() && area.is_none_or(|a| a.contains(r.cell)))
            .min_by_key(|r| (d(r.cell), r.id))
            .map(|r| r.id)
    }

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

    pub fn add(&mut self, kind: ResourceKind, cell: Cell, amount: f32) -> ResourceId {
        self.next_id += 1;
        let id = ResourceId(self.next_id);
        let age_days = if kind == ResourceKind::Tree { crate::forestry::DEFAULT_TREE_AGE } else { 0.0 };
        self.items.push(Resource { id, kind, cell, amount, max_amount: amount, branches: 0.0, bark: 0.0, wet: false, age_days, snag: false, tended_days: 0.0 });
        id
    }

    pub fn add_tree(&mut self, cell: Cell, age_days: f32) -> ResourceId {
        let id = self.add(ResourceKind::Tree, cell, FELL_WORK);
        self.items.last_mut().expect("just added").age_days = age_days.max(0.0);
        id
    }

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

    fn world(seed: u64) -> World {
        crate::worldgen::fixture(seed)
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
    fn stone_lies_in_a_few_deposits_far_apart() {
        let wa = world(11);
        let a = ResourceWorld::seeded(11, &wa);
        let stone: Vec<Cell> = a.items.iter().filter(|r| r.kind == ResourceKind::StoneOutcrop).map(|r| r.cell).collect();
        assert!(!stone.is_empty());
        let mut groups: Vec<Cell> = Vec::new();
        for &c in &stone {
            let near = |g: &Cell| c.0.abs_diff(g.0) + c.1.abs_diff(g.1) <= (4.0 * STONE_DEPOSIT_RADIUS) as usize;
            if !groups.iter().any(near) {
                groups.push(c);
            }
        }
        assert!(groups.len() <= STONE_DEPOSITS, "{} stone groups", groups.len());
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
                ResourceKind::StoneOutcrop => assert!(!water(x, y) && wa.forest.get(x, y).unwrap().is_none(), "stone {:?} on water or forest", r.cell),
                ResourceKind::Plant => assert!(is_wet(&wa, r.cell) && r.wet),
                ResourceKind::Tree => assert!(wa.forest.get(x, y).unwrap().is_some()),
                ResourceKind::Felled => panic!("no felled wood at start"),
            }
        }
    }
}
