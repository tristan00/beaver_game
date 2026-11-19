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
pub const PLANT_REGROW_PER_DAY: f32 = 0.25;
pub const WINTER_PLANT_DECAY_PER_DAY: f32 = 0.04;
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
    nav_revision: u64,
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
        self.nav_revision = self.nav_revision.wrapping_add(1);
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

    pub fn step(&mut self, dt_days: f32, time_days: f64) -> Result<(), ResourceError> {
        if !dt_days.is_finite() || dt_days < 0.0 {
            return Err(ResourceError::InvalidAmount);
        }
        let growth = crate::calendar::vegetation_growth(time_days - dt_days as f64 * 0.5);
        let decay = WINTER_PLANT_DECAY_PER_DAY * (1.0-growth);
        let survival = (-decay*dt_days).exp();
        for r in self.items.iter_mut().filter(|r| r.kind == ResourceKind::Plant) {
            let rate = if r.wet { PLANT_REGROW_PER_DAY*growth } else {0.0};
            r.amount = if decay > 1e-7 {
                r.amount*survival + rate/decay*(1.0-survival)
            } else {r.amount+rate*dt_days}.min(r.max_amount).max(0.0);
        }
        self.items.retain(|r| r.kind == ResourceKind::Plant || r.has_yield());
        Ok(())
    }

    pub fn add(&mut self, kind: ResourceKind, cell: Cell, amount: f32) -> ResourceId {
        self.next_id += 1;
        self.nav_revision = self.nav_revision.wrapping_add(1);
        let id = ResourceId(self.next_id);
        let age_days = if kind == ResourceKind::Tree { crate::forestry::DEFAULT_TREE_AGE } else { 0.0 };
        self.items.push(Resource { id, kind, cell, amount, max_amount: amount, branches: 0.0, bark: 0.0, wet: false, age_days, snag: false, tended_days: 0.0 });
        id
    }

    pub(crate) fn nav_revision(&self) -> u64 { self.nav_revision }

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
mod food_season_tests {
    use super::*;
    #[test]
    fn wet_plants_regrow_in_summer_but_draw_down_in_winter() {
        let mut summer=ResourceWorld::default();
        let id=summer.add(ResourceKind::Plant,(1,1),4.0);
        summer.get_mut(id).unwrap().wet=true;
        summer.take(id,3.0);
        let mut winter=summer.clone();
        summer.step(1.0,100.0).unwrap();winter.step(1.0,300.0).unwrap();
        assert!((summer.get(id).unwrap().amount-1.25).abs()<1e-6);
        assert!(winter.get(id).unwrap().amount<1.0);
        let mut split=winter.clone();let mut whole=winter.clone();
        whole.step(10.0,320.0).unwrap();
        for day in 311..=320 {split.step(1.0,day as f64).unwrap();}
        assert!((whole.get(id).unwrap().amount-split.get(id).unwrap().amount).abs()<1e-5);
    }
}

