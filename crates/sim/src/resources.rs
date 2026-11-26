include!(concat!(env!("OUT_DIR"), "/resources_constants.rs"));
use std::collections::{HashSet, VecDeque};

use rand_core::{Rng, SeedableRng};
use rand_pcg::Pcg32;

use crate::buildings::Good;
pub use crate::content::{Behaviour, ResourceKind};
use crate::water::Cell;
use crate::worldgen::World;

pub const SEEDLING_GROWN: f32 = 0.5;
pub const GROWTH_STEPS: u8 = 12;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
pub struct ResourceId(pub u64);

impl ResourceKind {
    pub fn gather_sources(self) -> &'static [ResourceKind] { &self.row().sources }
    pub fn yields(self) -> &'static [Good] { &self.row().yields }
    pub fn behaviour(self) -> Behaviour { self.row().behaviour }
    pub fn is_tree(self) -> bool { self.behaviour() == Behaviour::Tree }
    pub fn is_felled(self) -> bool { self.behaviour() == Behaviour::Felled }
    pub fn is_food_plant(self) -> bool { self.behaviour() == Behaviour::FoodPlant }
    pub fn aquatic(self) -> bool { self.row().aquatic }
    pub fn flowing(self) -> bool { self.row().flowing }
    pub fn is_solid(self) -> bool { self.row().solid }
    pub fn tree() -> ResourceKind { Self::all().find(|k| k.is_tree()).expect("validated on load") }
    pub fn felled() -> ResourceKind { Self::all().find(|k| k.is_felled()).expect("validated on load") }
    pub fn food_plant(aquatic: bool) -> ResourceKind { Self::all().find(|k| k.is_food_plant() && k.aquatic() == aquatic).expect("validated on load") }
    pub fn good(self) -> Option<Good> { (if self.is_tree() { Self::felled() } else { self }).yields().first().copied() }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ResourceError { NoSuchResource, WrongKind, InvalidAmount, TooYoung }

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Resource {
    pub id: ResourceId, pub kind: ResourceKind, pub cell: Cell, pub amount: f32, pub max_amount: f32,
    pub branches: f32, pub bark: f32, pub wet: bool, pub age_days: f32, pub snag: bool,
    pub species: Option<crate::trees::Tree>, pub plant: Option<crate::plants::Plant>, pub seedling: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Boulder { pub cell: Cell, pub size: f32, pub seed: u32 }

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Look { Tree(crate::forestry::TreeStage, u8), Felled(u8, bool), Fill(u8) }

fn fill(amount: f32, max_amount: f32, steps: f32) -> u8 {
    let full = (amount / max_amount.max(1e-3)).clamp(0.0, 1.0) * steps;
    let w = full as u8;
    w + (full > w as f32) as u8
}

impl Resource {
    pub fn growth(&self) -> f32 {
        use crate::forestry::{MATURE_AGE, YOUNG_AGE};
        if !self.kind.is_tree() || self.snag { return 0.0; }
        if self.age_days < YOUNG_AGE { (self.age_days / YOUNG_AGE).clamp(0.0, 0.999) }
        else if self.age_days < MATURE_AGE { ((self.age_days - YOUNG_AGE) / (MATURE_AGE - YOUNG_AGE)).clamp(0.0, 0.999) }
        else { 0.0 }
    }
    pub fn look(&self) -> Look {
        match self.kind.behaviour() {
            Behaviour::Tree => Look::Tree(self.stage().unwrap_or(crate::forestry::TreeStage::Mature), (self.growth() * GROWTH_STEPS as f32) as u8),
            Behaviour::Felled => Look::Felled(self.amount.ceil().clamp(0.0, 3.0) as u8, self.branches > 0.0),
            _ => Look::Fill(fill(self.amount, self.max_amount, self.kind.row().fill_steps as f32).max(self.kind.flowing() as u8)),
        }
    }
    pub fn is_solid(&self) -> bool { self.kind.is_solid() && !(self.kind.is_tree() && self.stage() == Some(crate::forestry::TreeStage::Seedling)) }
    pub fn has_yield(&self) -> bool {
        match self.kind.behaviour() {
            _ if self.kind.flowing() => true,
            Behaviour::Felled => self.amount > 0.0 || self.branches > 0.0 || self.bark > 0.0,
            _ => self.amount > 0.0,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct Changes { dropped: u64, ids: VecDeque<ResourceId>, guessed: Option<u64> }

impl Changes {
    const KEPT: usize = 8192;
    fn note(&mut self, id: ResourceId) {
        self.ids.push_back(id);
        if self.ids.len() > Self::KEPT { self.ids.pop_front(); self.dropped += 1; }
    }
    fn noted(&self) -> u64 { self.dropped + self.ids.len() as u64 }
    pub fn count(&self) -> u64 { self.guessed.unwrap_or(self.noted()) }
    pub fn since(&self, count: u64) -> Option<impl Iterator<Item = ResourceId> + '_> {
        (count >= self.dropped && count <= self.noted()).then(|| self.ids.iter().skip((count - self.dropped) as usize).copied())
    }
    pub fn guesses(&self) -> impl Iterator<Item = ResourceId> + '_ {
        let from = self.guessed.map_or(self.ids.len(), |g| g.saturating_sub(self.dropped) as usize);
        self.ids.iter().skip(from).copied()
    }
    pub(crate) fn guess_ahead(&mut self) { self.guessed.get_or_insert(self.noted()); }
}

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct ResourceWorld {
    items: Vec<Resource>,
    #[serde(skip)] changes: Changes,
    pub water_plants_seeded: bool,
    pub log_changes: bool,
    pub flora: String,
    pub boulders: Vec<Boulder>,
    next_id: u64,
    nav_revision: u64,
}

fn is_water(world: &World, (x, y): Cell) -> bool {
    world.river.get(x, y).copied().unwrap_or(false) || world.lake.get(x, y).copied().unwrap_or(false) || world.lake_level.get(x, y).is_some_and(|l| l.is_finite())
}
fn near4((x, y): Cell) -> [Cell; 4] { [(x.wrapping_sub(1), y), (x + 1, y), (x, y.wrapping_sub(1)), (x, y + 1)] }
fn distance(a: Cell, b: Cell) -> f32 { ((a.0 as f32 - b.0 as f32).powi(2) + (a.1 as f32 - b.1 as f32).powi(2)).sqrt() }
fn pick_spaced(spots: &mut [(f32, Cell)], limit: usize, spacing: f32, avoid: &[(Cell, f32)], extra: impl Fn(Cell) -> bool) -> Vec<Cell> {
    spots.sort_by(|a, b| b.0.total_cmp(&a.0).then(a.1.cmp(&b.1)));
    let mut out = Vec::new();
    for &(_, c) in spots.iter() {
        if out.len() >= limit { break; }
        if extra(c) && out.iter().all(|&o| distance(c, o) >= spacing) && avoid.iter().all(|&(k, d)| distance(c, k) >= d) { out.push(c); }
    }
    out
}

pub fn is_bank(world: &World, cell: Cell) -> bool {
    world.moisture.in_bounds(cell.0, cell.1) && !is_water(world, cell)
        && near4(cell).iter().any(|&(nx, ny)| world.moisture.in_bounds(nx, ny) && is_water(world, (nx, ny)))
}
pub fn is_wet(world: &World, (x, y): Cell) -> bool {
    !is_water(world, (x, y)) && world.moisture.get(x, y).is_some_and(|&m| m >= WET_MOISTURE)
}

pub fn note(what: &str, r: &Resource, detail: std::fmt::Arguments) {
    if crate::trace::enabled() {
        crate::trace::event(format!("RESOURCE_{what} {} id={} kind={:?} cell=({},{}) amount={:.2} branches={:.2} bark={:.2} age_days={:.0} species={:?} snag={} wet={} {detail}",
            crate::trace::sim_stamp(), r.id.0, r.kind, r.cell.0, r.cell.1, r.amount, r.branches, r.bark, r.age_days, r.species, r.snag, r.wet));
    }
}

impl ResourceWorld {
    pub fn seeded(seed: u64, world: &World) -> Self {
        let mut rng = Pcg32::seed_from_u64(seed ^ 0x5E50_0C35);
        let mut out = Self::default();
        let mut unit = move || (rng.next_u32() >> 8) as f32 / (1u32 << 24) as f32;
        let n = world.size;
        let treeless = |x, y| world.forest.get(x, y).is_some_and(Option::is_none);
        let mut banks: Vec<_> = (0..n).flat_map(|y| (0..n).map(move |x| (x, y)))
            .filter(|&(x, y)| is_bank(world, (x, y)) && treeless(x, y)).map(|c| (unit(), c)).collect();
        let beds = pick_spaced(&mut banks, MUD_BEDS, DEPOSIT_SPACING, &[], |_| true);
        let in_bed = |c| beds.iter().any(|&b| distance(c, b) <= MUD_BED_RADIUS);
        let mut river_spots = Vec::new();
        for &(_, c) in banks.iter().rev() {
            if river_spots.len() >= RIVER_MUD_SPOTS { break; }
            if near4(c).iter().any(|p| world.river.get(p.0, p.1) == Some(&true)) && !in_bed(c) && river_spots.iter().all(|&b| distance(c, b) >= RIVER_MUD_SPACING) {
                river_spots.push(c);
            }
        }
        let reach = water_reach(world, PEBBLE_WATER_REACH.max(MUD_POCKET_WATER_REACH));
        let pockets = mud_pockets(world, &reach, &mut unit, &beds);
        let (pebbles, boulders) = outcrops(world, &reach, &mut unit);
        let keep_off: Vec<_> = river_spots.iter().chain(&beds).chain(&pockets).map(|&c| (c, STREAM_PEBBLE_MUD_GAP)).collect();
        let gravel = stream_pebbles(world, &mut unit, &keep_off);
        let rocks: HashSet<Cell> = boulders.iter().map(|b| b.cell).collect();
        let upland = |c: Cell| !is_water(world, c) && !is_bank(world, c) && !is_wet(world, c) && treeless(c.0, c.1)
            && world.moisture.get(c.0, c.1).is_some_and(|&m| m >= crate::wetland::UPLAND_MOISTURE);
        let roles = &crate::content::content().roles;
        for y in 0..n { for x in 0..n {
            let cell = (x, y);
            if rocks.contains(&cell) { continue; }
            if pebbles.contains(&cell) { out.add(roles.outcrop, cell, PEBBLE_AMOUNT); }
            else if gravel.contains(&cell) { let id = out.add(roles.gravel, cell, STREAM_PEBBLE_AMOUNT); out.items.iter_mut().find(|r| r.id == id).unwrap().wet = true; }
            else if let Some(Some(sp)) = world.forest.get(x, y).copied() { out.add_tree(cell, crate::forestry::start_age(seed, cell), sp); }
            else if river_spots.contains(&cell) { out.add(roles.stream_mud, cell, RIVER_MUD_INITIAL); }
            else if is_bank(world, cell) && in_bed(cell) { if unit() < MUD_FILL { out.add(roles.bank_mud, cell, MUD_AMOUNT); } }
            else if !is_water(world, cell) && pockets.iter().any(|&p| p == cell || (distance(cell, p) <= MUD_POCKET_RADIUS && unit() < 0.75)) { out.add(roles.bank_mud, cell, MUD_POCKET_AMOUNT); }
            else if is_bank(world, cell) && unit() < STRAY_MUD_CHANCE { out.add(roles.bank_mud, cell, STRAY_MUD_AMOUNT); }
            else if is_wet(world, cell) && unit() < PLANT_CHANCE { let id = out.add(ResourceKind::food_plant(false), cell, PLANT_MAX); out.get_mut(id).unwrap().wet = true; }
            else if upland(cell) && unit() < UPLAND_PLANT_CHANCE { out.add(ResourceKind::food_plant(false), cell, PLANT_MAX); }
        }}
        out.boulders = boulders;
        out
    }

    fn index_of(&self, id: ResourceId) -> Option<usize> {
        self.items.binary_search_by_key(&id, |r| r.id).ok().or_else(|| self.items.iter().position(|r| r.id == id))
    }
    pub fn get(&self, id: ResourceId) -> Option<&Resource> { self.index_of(id).map(|i| &self.items[i]) }
    pub fn items(&self) -> &[Resource] { &self.items }
    pub fn changes(&self) -> &Changes { &self.changes }
    pub(crate) fn guess_ahead(&mut self) { self.changes.guess_ahead(); }
    pub(crate) fn nav_revision(&self) -> u64 { self.nav_revision }
    pub(crate) fn get_mut(&mut self, id: ResourceId) -> Result<&mut Resource, ResourceError> {
        let i = self.index_of(id).ok_or(ResourceError::NoSuchResource)?;
        self.changes.note(id);
        Ok(&mut self.items[i])
    }
    fn note_look(&mut self, i: usize, was: Look) { if self.items[i].look() != was { self.changes.note(self.items[i].id); } }
    pub(crate) fn each_mut(&mut self, mut f: impl FnMut(&mut Resource)) {
        for r in &mut self.items { let was = r.look(); f(r); if r.look() != was { self.changes.note(r.id); } }
    }
    pub(crate) fn keep(&mut self, mut keep: impl FnMut(&Resource) -> bool) {
        let changes = &mut self.changes;
        self.items.retain(|r| { let stays = keep(r); if !stays { changes.note(r.id); } stays });
    }
    pub(crate) fn remove_at(&mut self, i: usize) -> Resource { self.changes.note(self.items[i].id); self.items.remove(i) }
    pub fn clear(&mut self) { self.keep(|_| false); }
    pub fn at(&self, cell: Cell) -> impl Iterator<Item = &Resource> { self.items.iter().filter(move |r| r.cell == cell) }
    pub fn blocks(&self, cell: Cell) -> bool { self.at(cell).any(|r| r.is_solid() && r.has_yield()) || self.boulder_at(cell) }
    pub fn boulder_at(&self, cell: Cell) -> bool { self.boulders.iter().any(|b| b.cell == cell) }
    pub fn taken_cells(&self) -> impl Iterator<Item = Cell> + '_ { self.items.iter().map(|r| r.cell).chain(self.boulders.iter().map(|b| b.cell)) }

    pub fn fell(&mut self, id: ResourceId, work: f32) -> Result<bool, ResourceError> {
        if !work.is_finite() || work < 0.0 { return Err(ResourceError::InvalidAmount); }
        let i = self.index_of(id).ok_or(ResourceError::NoSuchResource)?;
        let was = self.items[i].look();
        let r = &mut self.items[i];
        if !r.kind.is_tree() { return Err(ResourceError::WrongKind); }
        let Some((logs, branches, bark)) = r.stage().and_then(|s| s.fell_yield(r.species.expect("tree species"))) else { return Err(ResourceError::TooYoung); };
        r.amount -= work;
        let felled = if r.amount <= 0.0 {
            (r.kind, r.amount, r.max_amount, r.branches, r.bark) = (ResourceKind::felled(), logs, logs, branches, bark);
            if self.log_changes { note("FELLED", &self.items[i], format_args!("")); }
            self.nav_revision = self.nav_revision.wrapping_add(1);
            true
        } else { false };
        self.note_look(i, was);
        Ok(felled)
    }

    pub fn strip(&mut self, id: ResourceId, want: f32) -> Result<(Good, f32), ResourceError> {
        if !want.is_finite() || want < 0.0 { return Err(ResourceError::InvalidAmount); }
        let i = self.index_of(id).ok_or(ResourceError::NoSuchResource)?;
        let was = self.items[i].look();
        let r = &mut self.items[i];
        if !r.kind.is_felled() { return Err(ResourceError::WrongKind); }
        let got = want.min(r.bark);
        r.bark -= got;
        if self.log_changes { note("STRIPPED", &self.items[i], format_args!("asked={want:.3} got={got:.3}")); }
        self.note_look(i, was);
        Ok((Good::food(), got))
    }

    pub fn take(&mut self, id: ResourceId, want: f32) -> Option<(Good, f32)> {
        if !want.is_finite() || want <= 0.0 { return None; }
        let i = self.index_of(id)?;
        let was = self.items[i].look();
        let r = &mut self.items[i];
        let good = *r.kind.yields().first()?;
        let slot = if r.kind.is_felled() && r.amount <= 0.0 { &mut r.branches } else { &mut r.amount };
        let got = want.min(*slot);
        let out = if got > 0.0 {
            *slot -= got;
            if self.log_changes { note("TAKEN", &self.items[i], format_args!("good={good:?} asked={want:.3} got={got:.3}")); }
            Some((good, got))
        } else { None };
        self.note_look(i, was);
        self.remove_if_bare(id);
        out
    }

    pub(crate) fn remove_if_bare(&mut self, id: ResourceId) {
        let Some(i) = self.index_of(id) else { return };
        if self.items[i].kind.is_food_plant() && self.items[i].amount < PLANT_BARE {
            if self.log_changes { note("GONE", &self.items[i], format_args!("why=picked bare")); }
            self.remove_at(i);
        }
    }

    pub fn step(&mut self, dt_days: f32, time_days: f64) -> Result<(), ResourceError> {
        if !dt_days.is_finite() || dt_days < 0.0 { return Err(ResourceError::InvalidAmount); }
        let growth = crate::calendar::vegetation_growth(time_days - dt_days as f64 * 0.5);
        let (decay, keep) = (plant_decay(growth), |r: &Resource| r.kind.is_food_plant() || r.kind.flowing() || r.has_yield());
        let (survival, mut any_gone, changes) = ((-decay * dt_days).exp(), false, &mut self.changes);
        let mut grew = |r: &Resource, was: f32, grown_out: bool| {
            let steps = r.kind.row().fill_steps as f32;
            if grown_out || (r.amount.to_bits() != was.to_bits() && fill(was, r.max_amount, steps) != fill(r.amount, r.max_amount, steps)) { changes.note(r.id); }
        };
        for r in &mut self.items {
            if r.kind.is_food_plant() {
                let (was, rate) = (r.amount, plant_regrow(r, growth) + decay * plant_winter_floor(r));
                r.amount = (if decay > 1e-7 { r.amount * survival + rate / decay * (1.0 - survival) } else { r.amount + rate * dt_days }).clamp(0.0, r.max_amount);
                let grown_out = r.seedling && r.amount >= SEEDLING_GROWN * r.max_amount;
                if grown_out { r.seedling = false; }
                grew(r, was, grown_out);
            } else if r.kind.flowing() {
                let was = r.amount;
                r.amount = river_mud_amount(r.amount, r.max_amount, r.wet, dt_days);
                grew(r, was, false);
            }
            any_gone |= !keep(r);
        }
        if !any_gone { return Ok(()); }
        let (before, solid_gone) = (self.items.len(), self.items.iter().any(|r| r.kind.is_solid() && !r.has_yield()));
        if self.log_changes { for r in self.items.iter().filter(|r| !keep(r)) { note("GONE", r, format_args!("why=used up")); } }
        self.keep(keep);
        if solid_gone && self.items.len() != before { self.nav_revision = self.nav_revision.wrapping_add(1); }
        Ok(())
    }

    pub fn add(&mut self, kind: ResourceKind, cell: Cell, amount: f32) -> ResourceId {
        self.next_id += 1;
        self.nav_revision = self.nav_revision.wrapping_add(1);
        let id = ResourceId(self.next_id);
        self.changes.note(id);
        self.items.push(Resource {
            id, kind, cell, amount, max_amount: if kind.flowing() { RIVER_MUD_CAPACITY } else { amount },
            branches: 0.0, bark: 0.0, wet: false, age_days: if kind.is_tree() { crate::forestry::DEFAULT_TREE_AGE } else { 0.0 },
            snag: false, species: None, plant: None, seedling: false,
        });
        if self.log_changes { note("NEW", self.items.last().unwrap(), format_args!("")); }
        id
    }

    pub fn add_tree(&mut self, cell: Cell, age_days: f32, species: crate::trees::Tree) -> ResourceId {
        let id = self.add(ResourceKind::tree(), cell, FELL_WORK * species.spec().fell_work);
        let r = self.items.last_mut().unwrap();
        (r.age_days, r.species) = (age_days.max(0.0), Some(species));
        id
    }
}

fn water_reach(world: &World, max: usize) -> Vec<usize> {
    let (n, mut reach, mut q) = (world.size, vec![usize::MAX; world.size * world.size], VecDeque::new());
    for y in 0..n { for x in 0..n { if is_water(world, (x, y)) { reach[y * n + x] = 0; q.push_back((x, y)); } } }
    while let Some((x, y)) = q.pop_front() {
        let d = reach[y * n + x];
        if d >= max { continue; }
        for (nx, ny) in near4((x, y)) {
            if nx < n && ny < n && reach[ny * n + nx] == usize::MAX { reach[ny * n + nx] = d + 1; q.push_back((nx, ny)); }
        }
    }
    reach
}

fn outcrops(world: &World, reach: &[usize], unit: &mut impl FnMut() -> f32) -> (HashSet<Cell>, Vec<Boulder>) {
    let n = world.size;
    let dry = |c: Cell| !is_water(world, c) && !is_bank(world, c) && world.moisture.get(c.0, c.1).is_some_and(|&m| m < WET_MOISTURE);
    let mut spots = Vec::new();
    for y in (6..n.saturating_sub(6)).step_by(3) {
        for x in (6..n.saturating_sub(6)).step_by(3) {
            if dry((x, y)) && reach[y * n + x] <= PEBBLE_WATER_REACH {
                spots.push((world.height.get(x, y).copied().unwrap_or(0.0) + 20.0 * world.slope.get(x, y).copied().unwrap_or(0.0) + 4.0 * unit(), (x, y)));
            }
        }
    }
    let centres = pick_spaced(&mut spots, PEBBLE_OUTCROPS, PEBBLE_SPACING, &[], |_| true);
    let (mut pebbles, mut boulders) = (HashSet::new(), Vec::new());
    for c in centres {
        let pile = unit() < PEBBLE_PILE_SHARE;
        let radius = if pile { PEBBLE_PILE_RADIUS } else { PEBBLE_RADIUS };
        let mut rocks = Vec::new();
        if !pile {
            let span = BOULDERS_PER_OUTCROP.end() - BOULDERS_PER_OUTCROP.start() + 1;
            let count = BOULDERS_PER_OUTCROP.start() + ((unit() * span as f32) as usize).min(span - 1);
            for k in 0..count * 3 {
                if rocks.len() >= count { break; }
                let cell = if k == 0 { c } else {
                    let (a, d) = (unit() * std::f32::consts::TAU, 1.0 + unit() * 1.4);
                    ((c.0 as f32 + a.cos() * d).round() as usize, (c.1 as f32 + a.sin() * d).round() as usize)
                };
                if cell.0 < n && cell.1 < n && dry(cell) && !rocks.contains(&cell) {
                    rocks.push(cell);
                    boulders.push(Boulder { cell, size: if k == 0 { 1.2 + 0.5 * unit() } else { 0.55 + 0.6 * unit() }, seed: (unit() * 65535.0) as u32 });
                }
            }
        }
        let r = radius.ceil() as usize;
        for y in c.1.saturating_sub(r)..=(c.1 + r).min(n - 1) {
            for x in c.0.saturating_sub(r)..=(c.0 + r).min(n - 1) {
                let cell = (x, y);
                if rocks.contains(&cell) || distance(cell, c) > radius || !dry(cell) { continue; }
                let fill = if cell == c || (pile && distance(cell, c) < 1.1) { 1.0 } else { PEBBLE_FILL };
                if unit() < fill { pebbles.insert(cell); }
            }
        }
    }
    (pebbles, boulders)
}

fn stream_pebbles(world: &World, unit: &mut impl FnMut() -> f32, keep_off: &[(Cell, f32)]) -> HashSet<Cell> {
    let n = world.size;
    let river = |c: Cell| world.river.get(c.0, c.1).copied().unwrap_or(false) && !world.lake.get(c.0, c.1).copied().unwrap_or(false);
    let mut spots: Vec<_> = (2..n.saturating_sub(2)).flat_map(|y| (2..n.saturating_sub(2)).map(move |x| (x, y)))
        .filter(|&c| river(c) && near4(c).iter().any(|&o| o.0 < n && o.1 < n && !is_water(world, o)))
        .map(|c| (unit(), c)).collect();
    let mut out = HashSet::new();
    for b in pick_spaced(&mut spots, STREAM_PEBBLE_BEDS, STREAM_PEBBLE_SPACING, keep_off, |_| true) {
        out.insert(b);
        for o in near4(b) { if o.0 < n && o.1 < n && river(o) && unit() < 0.5 { out.insert(o); } }
    }
    out
}

fn mud_pockets(world: &World, reach: &[usize], unit: &mut impl FnMut() -> f32, beds: &[Cell]) -> Vec<Cell> {
    let (n, mut shore, mut inland) = (world.size, Vec::new(), Vec::new());
    for y in (4..n.saturating_sub(4)).step_by(2) {
        for x in (4..n.saturating_sub(4)).step_by(2) {
            let c = (x, y);
            if is_water(world, c) { continue; }
            let d = reach[y * n + x];
            if (1..=MUD_POCKET_WATER_REACH).contains(&d) { shore.push((unit(), c)); }
            else if d > MUD_POCKET_WATER_REACH && is_wet(world, c) { inland.push((unit(), c)); }
        }
    }
    let bed_avoid: Vec<_> = beds.iter().map(|&b| (b, MUD_BED_RADIUS + MUD_POCKET_SPACING * 0.5)).collect();
    let mut out = pick_spaced(&mut inland, (MUD_POCKETS as f32 * MUD_POCKET_INLAND_SHARE).round() as usize, MUD_POCKET_SPACING, &bed_avoid, |_| true);
    let extra = pick_spaced(&mut shore, MUD_POCKETS.saturating_sub(out.len()), MUD_POCKET_SPACING, &bed_avoid, |c| out.iter().all(|&o| distance(c, o) >= MUD_POCKET_SPACING));
    out.extend(extra);
    out
}

pub fn plant_regrow(r: &Resource, growth: f32) -> f32 {
    if !r.wet { return 0.0; }
    growth * match r.plant { Some(p) => p.spec().regrow, None if r.kind.aquatic() => WATER_PLANT_REGROW_PER_DAY, None => PLANT_REGROW_PER_DAY }
}
pub fn plant_winter_floor(r: &Resource) -> f32 { r.plant.map_or(0.0, |p| p.spec().keep * r.max_amount) }
pub fn plant_decay(growth: f32) -> f32 { WINTER_PLANT_DECAY_PER_DAY * (1.0 - growth) }
pub fn river_mud_amount(amount: f32, capacity: f32, flowing: bool, days: f32) -> f32 {
    (amount + if flowing { RIVER_MUD_PER_DAY * days.max(0.0) } else { 0.0 }).min(capacity)
}
