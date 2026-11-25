include!(concat!(env!("OUT_DIR"), "/resources_constants.rs"));
use crate::buildings::Good;
use crate::water::Cell;
use crate::worldgen::World;
use rand_core::{Rng, SeedableRng};
use rand_pcg::Pcg32;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
pub struct ResourceId(pub u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum ResourceKind {
    Tree,
    Felled,
    Plant,
    MudBank,
    RiverMud,
    /// Pebbles: heaped round the boulders of rocky outcrops on high dry ground, in heaps of
    /// their own, and in gravel bars along the streams; heavy to carry.
    Pebbles,
    /// Plants rooted in open water (lilies, pondweed, arrowhead): rich in food.
    WaterPlant,
}

impl ResourceKind {
    pub fn gather_sources(self) -> &'static [ResourceKind] {
        match self {
            Self::Tree | Self::Felled => &[Self::Felled, Self::Tree],
            // What lies on the ground comes first: bark on fallen trees, then plants.
            // Foragers never fell a tree.
            Self::Plant | Self::WaterPlant => &[Self::Felled, Self::Plant, Self::WaterPlant],
            Self::MudBank | Self::RiverMud => &[Self::RiverMud, Self::MudBank],
            Self::Pebbles => &[Self::Pebbles],
        }
    }
    pub fn yields(self) -> &'static [Good] {
        match self {
            ResourceKind::Tree => &[],
            ResourceKind::Felled => &[Good::Sticks, Good::Food],
            ResourceKind::Plant | ResourceKind::WaterPlant => &[Good::Food],
            ResourceKind::MudBank => &[Good::Mud],
            ResourceKind::RiverMud => &[Good::Mud],
            ResourceKind::Pebbles => &[Good::Pebbles],
        }
    }
    /// Stands in the way. Pebbles are a low heap, walked over; the boulders they lie round
    /// stand in the way (see `Boulder`).
    pub fn is_solid(self) -> bool {
        matches!(self, ResourceKind::Tree)
    }
    /// Food plants, on land or in water: they regrow and stay when eaten bare.
    pub fn is_food_plant(self) -> bool {
        matches!(self, ResourceKind::Plant | ResourceKind::WaterPlant)
    }
}


#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ResourceError {
    NoSuchResource,
    WrongKind,
    InvalidAmount,
    TooYoung,
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
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
    /// Trees and felled trees only.
    pub species: Option<crate::trees::Tree>,
    /// Food plants only; set from the map's flora (see `wetland::assign_plants`). Saved after
    /// the world (see `save`).
    #[serde(skip)]
    pub plant: Option<crate::plants::Plant>,
}

/// A boulder of a rocky outcrop: scenery, never gathered, standing in its cell's way. The
/// pebbles round it are the resource.
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Boulder {
    pub cell: Cell,
    /// Roughly how many metres across.
    pub size: f32,
    /// Picks its shape and turn.
    pub seed: u32,
}

/// What can be seen of a source: its kind, and how much of it there is in the steps it is
/// drawn in. Sources that look alike have equal looks; one whose look has not changed needs no
/// redrawing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Look {
    /// A tree's stage, and for a sapling or young tree how far it has grown towards the next
    /// (in `GROWTH_STEPS`), so it is redrawn a little bigger as it grows.
    Tree(crate::forestry::TreeStage, u8),
    /// Logs left (0 to 3), and whether there are branches.
    Felled(u8, bool),
    /// Eighths of the mud left; a river's mud bed never shows less than one.
    Mud(u8),
    /// Quarters left.
    Plant(u8),
    Pebbles(u8),
    WaterPlant(u8),
}

/// Steps a growing tree is redrawn in through each stage.
pub const GROWTH_STEPS: u8 = 12;

/// How full, in `steps` steps, rounded up.
fn fill(amount: f32, max_amount: f32, steps: f32) -> u8 {
    let full = (amount / max_amount.max(1e-3)).clamp(0.0, 1.0) * steps;
    let whole = full as u8;
    whole + (full > whole as f32) as u8
}

impl Resource {
    /// How far a sapling or young tree has grown through its stage, 0 to 1; 0 for any other.
    pub fn growth(&self) -> f32 {
        use crate::forestry::{MATURE_AGE, YOUNG_AGE};
        if self.kind != ResourceKind::Tree || self.snag {
            return 0.0;
        }
        let a = self.age_days;
        if a < YOUNG_AGE {
            (a / YOUNG_AGE).clamp(0.0, 0.999)
        } else if a < MATURE_AGE {
            ((a - YOUNG_AGE) / (MATURE_AGE - YOUNG_AGE)).clamp(0.0, 0.999)
        } else {
            0.0
        }
    }
    pub fn look(&self) -> Look {
        let q = |steps: f32| fill(self.amount, self.max_amount, steps);
        match self.kind {
            ResourceKind::Tree => {
                let stage = self.stage().unwrap_or(crate::forestry::TreeStage::Mature);
                Look::Tree(stage, (self.growth() * GROWTH_STEPS as f32) as u8)
            }
            ResourceKind::Felled => Look::Felled(self.amount.ceil().clamp(0.0, 3.0) as u8, self.branches > 0.0),
            ResourceKind::MudBank => Look::Mud(q(8.0)),
            ResourceKind::RiverMud => Look::Mud(q(8.0).max(1)),
            ResourceKind::Plant => Look::Plant(q(4.0)),
            ResourceKind::Pebbles => Look::Pebbles(q(4.0)),
            ResourceKind::WaterPlant => Look::WaterPlant(q(4.0)),
        }
    }
    pub fn is_solid(&self) -> bool {
        self.kind.is_solid() && !(self.kind == ResourceKind::Tree && self.stage() == Some(crate::forestry::TreeStage::Sapling))
    }
    pub fn has_yield(&self) -> bool {
        match self.kind {
            ResourceKind::RiverMud => true,
            ResourceKind::Felled => self.amount > 0.0 || self.branches > 0.0 || self.bark > 0.0,
            _ => self.amount > 0.0,
        }
    }
}

/// The sources that were added, removed or came to look different (`Resource::look`), oldest
/// first. Whoever draws them keeps up by reading what was noted since it last looked, instead
/// of comparing every source. Only so many notes are kept: a reader further behind than that
/// is told so, and looks at everything once.
#[derive(Clone, Debug, Default)]
pub struct Changes {
    /// How many notes were dropped from the front.
    dropped: u64,
    ids: std::collections::VecDeque<ResourceId>,
    /// Notes from this count on are guesses (see `SimState::guess_ahead`).
    guessed: Option<u64>,
}

impl Changes {
    const KEPT: usize = 8192;

    fn note(&mut self, id: ResourceId) {
        self.ids.push_back(id);
        if self.ids.len() > Self::KEPT {
            self.ids.pop_front();
            self.dropped += 1;
        }
    }

    fn noted(&self) -> u64 { self.dropped + self.ids.len() as u64 }

    /// How many changes the simulation has noted. Keep it, and hand it to `since` next time.
    pub fn count(&self) -> u64 { self.guessed.unwrap_or(self.noted()) }

    /// The sources noted after the first `count` changes, guesses included; `None` if the
    /// list no longer reaches back that far (or `count` came from another world).
    pub fn since(&self, count: u64) -> Option<impl Iterator<Item = ResourceId> + '_> {
        (count >= self.dropped && count <= self.noted()).then(|| self.ids.iter().skip((count - self.dropped) as usize).copied())
    }

    /// The sources this copy of the world only guesses have changed: look at them again when
    /// the simulation's next word arrives, whatever it says.
    pub fn guesses(&self) -> impl Iterator<Item = ResourceId> + '_ {
        let from = self.guessed.map_or(self.ids.len(), |g| g.saturating_sub(self.dropped) as usize);
        self.ids.iter().skip(from).copied()
    }

    pub(crate) fn guess_ahead(&mut self) {
        self.guessed.get_or_insert(self.dropped + self.ids.len() as u64);
    }
}

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct ResourceWorld {
    /// In order of id. Private so that every change to a source goes through a method
    /// here, which notes it in `changes`.
    items: Vec<Resource>,
    #[serde(skip)]
    changes: Changes,
    pub river_mud_gathered: f32,
    pub river_mud_recovered: f32,
    /// Water plants are set out once, when the starting water has settled.
    pub water_plants_seeded: bool,
    /// Write changes to the session journal (off while the world is first filled in).
    pub log_changes: bool,
    /// The map whose flora the plants come from (`plants::flora`); empty for the default.
    /// Saved after the world (see `save`).
    #[serde(skip)]
    pub flora: String,
    /// Saved after the world (see `save`).
    #[serde(skip)]
    pub boulders: Vec<Boulder>,
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



pub fn is_wet(world: &World, (x, y): Cell) -> bool {
    !is_water(world, (x, y)) && world.moisture.get(x, y).is_some_and(|&m| m >= WET_MOISTURE)
}

/// For the session journal: one resource and what happened to it.
pub fn note(what: &str, r: &Resource, detail: std::fmt::Arguments) {
    if crate::trace::enabled() {
        crate::trace::event(format!(
            "RESOURCE_{what} {} id={} kind={:?} cell=({},{}) amount={:.2} branches={:.2} bark={:.2} age_days={:.0} species={:?} snag={} wet={} {detail}",
            crate::trace::sim_stamp(), r.id.0, r.kind, r.cell.0, r.cell.1, r.amount, r.branches, r.bark, r.age_days, r.species, r.snag, r.wet
        ));
    }
}

impl ResourceWorld {
    pub fn seeded(seed: u64, world: &World) -> Self {
        let mut rng = Pcg32::seed_from_u64(seed ^ 0x5E50_0C35);
        let mut out = Self::default();
        let mut unit = move || (rng.next_u32() >> 8) as f32 / (1u32 << 24) as f32;
        let n = world.size;
        let treeless = |x: usize,y: usize| world.forest.get(x,y).is_some_and(|t|t.is_none());
        let mut banks = Vec::new();
        for y in 0..n {for x in 0..n {
            if is_bank(world,(x,y)) && treeless(x,y) {banks.push((unit(),(x,y)));}
        }}
        banks.sort_by(|a,b|b.0.total_cmp(&a.0).then(a.1.cmp(&b.1)));
        let mut beds: Vec<Cell> = Vec::new();
        for &(_,c) in &banks {
            if beds.iter().all(|&b|distance(c,b)>=DEPOSIT_SPACING) {beds.push(c);}
            if beds.len()>=MUD_BEDS {break;}
        }
        let in_bed=|c:Cell|beds.iter().any(|&b|distance(c,b)<=MUD_BED_RADIUS);
        let mut river_spots: Vec<Cell> = Vec::new();
        for &(_,c) in banks.iter().rev() {
            let (x,y)=c;
            let near_river=[(x.wrapping_sub(1),y),(x+1,y),(x,y.wrapping_sub(1)),(x,y+1)]
                .iter().any(|p|world.river.get(p.0,p.1)==Some(&true));
            if near_river && !in_bed(c) && river_spots.iter().all(|&b|distance(c,b)>=RIVER_MUD_SPACING) {river_spots.push(c);}
            if river_spots.len()>=RIVER_MUD_SPOTS {break;}
        }
        let reach = water_reach(world, PEBBLE_WATER_REACH.max(MUD_POCKET_WATER_REACH));
        let pockets = mud_pockets(world, &reach, &mut unit, &beds);
        let (pebbles, boulders) = outcrops(world, &reach, &mut unit);
        let keep_off: Vec<Cell> = river_spots.iter().chain(&beds).chain(&pockets).copied().collect();
        let gravel = stream_pebbles(world, &mut unit, &keep_off);
        let rocks: std::collections::HashSet<Cell> = boulders.iter().map(|b| b.cell).collect();
        let in_pocket = |c: Cell, unit: &mut dyn FnMut() -> f32| pockets.iter().any(|&p| p == c || (distance(c, p) <= MUD_POCKET_RADIUS && unit() < 0.75));
        let upland = |c: Cell| !is_water(world, c) && !is_bank(world, c) && !is_wet(world, c) && treeless(c.0, c.1)
            && world.moisture.get(c.0, c.1).is_some_and(|&m| m >= crate::wetland::UPLAND_MOISTURE);
        for y in 0..n {for x in 0..n {
            let cell=(x,y);
            if rocks.contains(&cell) {
                continue;
            }
            if pebbles.contains(&cell) {
                out.add(ResourceKind::Pebbles,cell,PEBBLE_AMOUNT);
            } else if gravel.contains(&cell) {
                let id = out.add(ResourceKind::Pebbles,cell,STREAM_PEBBLE_AMOUNT);
                out.items.iter_mut().find(|r| r.id == id).expect("just added").wet = true;
            } else if let Some(Some(sp)) = world.forest.get(x,y).copied() {
                out.add_tree(cell,crate::forestry::start_age(seed,cell),sp);
            } else if river_spots.contains(&cell) {
                out.add(ResourceKind::RiverMud,cell,RIVER_MUD_INITIAL);
            } else if is_bank(world,cell) && in_bed(cell) {
                if unit()<MUD_FILL {out.add(ResourceKind::MudBank,cell,MUD_AMOUNT);}
            } else if !is_water(world,cell) && in_pocket(cell, &mut unit) {
                out.add(ResourceKind::MudBank,cell,MUD_POCKET_AMOUNT);
            } else if is_bank(world,cell) && unit()<STRAY_MUD_CHANCE {
                out.add(ResourceKind::MudBank,cell,STRAY_MUD_AMOUNT);
            } else if is_wet(world,cell) && unit()<PLANT_CHANCE {
                let id=out.add(ResourceKind::Plant,cell,PLANT_MAX);
                out.get_mut(id).unwrap().wet=true;
            } else if upland(cell) && unit()<UPLAND_PLANT_CHANCE {
                out.add(ResourceKind::Plant,cell,PLANT_MAX);
            }
        }}
        out.boulders = boulders;
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
    /// Every source, in order of id.
    pub fn items(&self) -> &[Resource] { &self.items }
    /// What has changed, for whoever draws the sources.
    pub fn changes(&self) -> &Changes { &self.changes }
    pub(crate) fn guess_ahead(&mut self) { self.changes.guess_ahead(); }

    /// A source to change, noted as changed whether or not it comes to look different.
    pub(crate) fn get_mut(&mut self, id: ResourceId) -> Result<&mut Resource, ResourceError> {
        let i = self.index_of(id).ok_or(ResourceError::NoSuchResource)?;
        self.changes.note(id);
        Ok(&mut self.items[i])
    }
    /// Note the source at this place in `items` if it no longer looks as it did (`was`).
    fn note_look(&mut self, i: usize, was: Look) {
        if self.items[i].look() != was {
            self.changes.note(self.items[i].id);
        }
    }
    /// Change any of the sources, noting those that come to look different.
    pub(crate) fn each_mut(&mut self, mut f: impl FnMut(&mut Resource)) {
        for r in &mut self.items {
            let was = r.look();
            f(r);
            if r.look() != was {
                self.changes.note(r.id);
            }
        }
    }
    /// Remove the sources `keep` turns down.
    pub(crate) fn keep(&mut self, mut keep: impl FnMut(&Resource) -> bool) {
        let changes = &mut self.changes;
        self.items.retain(|r| {
            let stays = keep(r);
            if !stays {
                changes.note(r.id);
            }
            stays
        });
    }
    /// Remove the source at this place in `items`.
    pub(crate) fn remove_at(&mut self, i: usize) -> Resource {
        self.changes.note(self.items[i].id);
        self.items.remove(i)
    }
    pub fn clear(&mut self) {
        self.keep(|_| false);
    }
    pub fn at(&self, cell: Cell) -> impl Iterator<Item = &Resource> {
        self.items.iter().filter(move |r| r.cell == cell)
    }
    pub fn blocks(&self, cell: Cell) -> bool {
        self.at(cell).any(|r| r.is_solid() && r.has_yield()) || self.boulder_at(cell)
    }
    pub fn boulder_at(&self, cell: Cell) -> bool {
        self.boulders.iter().any(|b| b.cell == cell)
    }
    /// Cells taken by a source or a boulder: nothing else roots or is set there.
    pub fn taken_cells(&self) -> impl Iterator<Item = Cell> + '_ {
        self.items.iter().map(|r| r.cell).chain(self.boulders.iter().map(|b| b.cell))
    }

    pub fn fell(&mut self, id: ResourceId, work: f32) -> Result<bool, ResourceError> {
        if !work.is_finite() || work < 0.0 {
            return Err(ResourceError::InvalidAmount);
        }
        let i = self.index_of(id).ok_or(ResourceError::NoSuchResource)?;
        let was = self.items[i].look();
        let out = self.fell_at(i, work);
        self.note_look(i, was);
        out
    }

    fn fell_at(&mut self, i: usize, work: f32) -> Result<bool, ResourceError> {
        let r = &mut self.items[i];
        if r.kind != ResourceKind::Tree {
            return Err(ResourceError::WrongKind);
        }
        let Some((logs, branches, bark)) = r.stage().and_then(|s| s.fell_yield(r.species.unwrap_or(crate::trees::Tree::Aspen))) else {
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
        let log = self.log_changes;
        if log { note("FELLED", &self.items[i], format_args!("")); }
        self.nav_revision = self.nav_revision.wrapping_add(1);
        Ok(true)
    }

    pub fn strip(&mut self, id: ResourceId, want: f32) -> Result<(Good, f32), ResourceError> {
        if !want.is_finite() || want < 0.0 {
            return Err(ResourceError::InvalidAmount);
        }
        let i = self.index_of(id).ok_or(ResourceError::NoSuchResource)?;
        let was = self.items[i].look();
        let out = self.strip_at(i, want);
        self.note_look(i, was);
        out
    }

    fn strip_at(&mut self, i: usize, want: f32) -> Result<(Good, f32), ResourceError> {
        let r = &mut self.items[i];
        if r.kind != ResourceKind::Felled {
            return Err(ResourceError::WrongKind);
        }
        let got = want.min(r.bark);
        r.bark -= got;
        if self.log_changes { note("STRIPPED", &self.items[i], format_args!("asked={want:.3} got={got:.3}")); }
        Ok((Good::Food, got))
    }

    pub fn take(&mut self, id: ResourceId, want: f32) -> Option<(Good, f32)> {
        if !want.is_finite() || want <= 0.0 {
            return None;
        }
        let i = self.index_of(id)?;
        let was = self.items[i].look();
        let out = self.take_at(i, want);
        self.note_look(i, was);
        self.remove_if_bare(id);
        out
    }

    /// A food plant picked bare dies: new ones only spread in from living plants nearby (see
    /// `wetland::step`), so gathering clears the ground round a colony as felling does.
    pub(crate) fn remove_if_bare(&mut self, id: ResourceId) {
        let Some(i) = self.index_of(id) else { return };
        let r = &self.items[i];
        if r.kind.is_food_plant() && r.amount < PLANT_BARE {
            if self.log_changes { note("GONE", r, format_args!("why=picked bare")); }
            self.remove_at(i);
        }
    }

    fn take_at(&mut self, i: usize, want: f32) -> Option<(Good, f32)> {
        let r = &mut self.items[i];
        let (good, slot) = match r.kind {
            ResourceKind::Tree => return None,
            ResourceKind::Felled if r.amount > 0.0 => (Good::Sticks, &mut r.amount),
            ResourceKind::Felled => (Good::Sticks, &mut r.branches),
            ResourceKind::Plant | ResourceKind::WaterPlant => (Good::Food, &mut r.amount),
            ResourceKind::MudBank => (Good::Mud, &mut r.amount),
            ResourceKind::RiverMud => (Good::Mud, &mut r.amount),
            ResourceKind::Pebbles => (Good::Pebbles, &mut r.amount),
        };
        let got = want.min(*slot);
        if got <= 0.0 {
            return None;
        }
        *slot -= got;
        if r.kind == ResourceKind::RiverMud { self.river_mud_gathered += got; }
        if self.log_changes { note("TAKEN", &self.items[i], format_args!("good={good:?} asked={want:.3} got={got:.3}")); }
        Some((good, got))
    }

    pub fn step(&mut self, dt_days: f32, time_days: f64) -> Result<(), ResourceError> {
        if !dt_days.is_finite() || dt_days < 0.0 {
            return Err(ResourceError::InvalidAmount);
        }
        let growth = crate::calendar::vegetation_growth(time_days - dt_days as f64 * 0.5);
        let decay = plant_decay(growth);
        let survival = (-decay*dt_days).exp();
        let keep = |r: &Resource| r.kind.is_food_plant() || r.kind == ResourceKind::RiverMud || r.has_yield();
        // One sweep over every source: plants grow, mud beds refill, and whether any is used up.
        let mut any_gone = false;
        let changes = &mut self.changes;
        // A plant or mud bed is noted when its amount crosses a step it is drawn in (most
        // stand at their full amount, and nothing else changes here).
        let mut grew = |r: &Resource, was: f32, steps: f32| {
            if r.amount.to_bits() != was.to_bits() && fill(was, r.max_amount, steps) != fill(r.amount, r.max_amount, steps) {
                changes.note(r.id);
            }
        };
        for r in self.items.iter_mut() {
            if r.kind.is_food_plant() {
                let was = r.amount;
                // Toward what it keeps over winter, not to nothing.
                let rate = plant_regrow(r, growth) + decay * plant_winter_floor(r);
                r.amount = if decay > 1e-7 {
                    r.amount*survival + rate/decay*(1.0-survival)
                } else {r.amount+rate*dt_days}.min(r.max_amount).max(0.0);
                grew(r, was, 4.0);
            } else if r.kind == ResourceKind::RiverMud {
                let before = r.amount;
                r.amount=river_mud_amount(r.amount,r.max_amount,r.wet,dt_days);
                self.river_mud_recovered += r.amount - before;
                grew(r, before, 8.0);
            }
            any_gone |= !keep(r);
        }
        if !any_gone {
            return Ok(());
        }
        let before = self.items.len();
        let solid_gone = self.items.iter().any(|r| r.kind == ResourceKind::Pebbles && !r.has_yield());
        if self.log_changes {
            for r in self.items.iter().filter(|r| !keep(r)) { note("GONE", r, format_args!("why=used up")); }
        }
        self.keep(keep);
        if solid_gone && self.items.len() != before { self.nav_revision = self.nav_revision.wrapping_add(1); }
        Ok(())
    }

    pub fn add(&mut self, kind: ResourceKind, cell: Cell, amount: f32) -> ResourceId {
        self.next_id += 1;
        self.nav_revision = self.nav_revision.wrapping_add(1);
        let id = ResourceId(self.next_id);
        let age_days = if kind == ResourceKind::Tree { crate::forestry::DEFAULT_TREE_AGE } else { 0.0 };
        self.changes.note(id);
        self.items.push(Resource { id, kind, cell, amount, max_amount: if kind==ResourceKind::RiverMud { RIVER_MUD_CAPACITY } else { amount }, branches: 0.0, bark: 0.0, wet: false, age_days, snag: false, species: None, plant: None });
        if self.log_changes { note("NEW", self.items.last().expect("just added"), format_args!("")); }
        id
    }

    pub(crate) fn nav_revision(&self) -> u64 { self.nav_revision }

    pub fn add_tree(&mut self, cell: Cell, age_days: f32, species: crate::trees::Tree) -> ResourceId {
        let work = FELL_WORK * species.spec().fell_work;
        let id = self.add(ResourceKind::Tree, cell, work);
        let r = self.items.last_mut().expect("just added");
        r.age_days = age_days.max(0.0);
        r.species = Some(species);
        id
    }

}

/// Steps from the nearest water, up to `max` (further cells read `usize::MAX`).
fn water_reach(world: &World, max: usize) -> Vec<usize> {
    let n = world.size;
    let mut reach = vec![usize::MAX; n * n];
    let mut queue = std::collections::VecDeque::new();
    for y in 0..n { for x in 0..n { if is_water(world, (x, y)) { reach[y * n + x] = 0; queue.push_back((x, y)); } } }
    while let Some((x, y)) = queue.pop_front() {
        let d = reach[y * n + x];
        if d >= max { continue; }
        for (nx, ny) in [(x.wrapping_sub(1), y), (x + 1, y), (x, y.wrapping_sub(1)), (x, y + 1)] {
            if nx < n && ny < n && reach[ny * n + nx] == usize::MAX { reach[ny * n + nx] = d + 1; queue.push_back((nx, ny)); }
        }
    }
    reach
}

/// Rocky outcrops on the highest, steepest dry ground near water, well apart. Most are a few
/// boulders with pebbles lying round them; some are a heap of pebbles alone. Returns the
/// pebble cells and the boulders.
fn outcrops(world: &World, reach: &[usize], unit: &mut impl FnMut() -> f32) -> (std::collections::HashSet<Cell>, Vec<Boulder>) {
    let n = world.size;
    let dry = |c: Cell| !is_water(world, c) && !is_bank(world, c) && world.moisture.get(c.0, c.1).is_some_and(|&m| m < WET_MOISTURE);
    let mut spots: Vec<(f32, Cell)> = Vec::new();
    let edge = 6;
    for y in (edge..n.saturating_sub(edge)).step_by(3) {
        for x in (edge..n.saturating_sub(edge)).step_by(3) {
            // Near enough the water that a pebble heap on the bank reaches it.
            if !dry((x, y)) || reach[y * n + x] > PEBBLE_WATER_REACH { continue; }
            let h = world.height.get(x, y).copied().unwrap_or(0.0);
            let slope = world.slope.get(x, y).copied().unwrap_or(0.0);
            spots.push((h + 20.0 * slope + 4.0 * unit(), (x, y)));
        }
    }
    spots.sort_by(|a, b| b.0.total_cmp(&a.0).then(a.1.cmp(&b.1)));
    let mut centres: Vec<Cell> = Vec::new();
    for &(_, c) in &spots {
        if centres.iter().all(|&o| distance(c, o) >= PEBBLE_SPACING) { centres.push(c); }
        if centres.len() >= PEBBLE_OUTCROPS { break; }
    }
    let mut pebbles = std::collections::HashSet::new();
    let mut boulders: Vec<Boulder> = Vec::new();
    for c in centres {
        let pile = unit() < PEBBLE_PILE_SHARE;
        let radius = if pile { PEBBLE_PILE_RADIUS } else { PEBBLE_RADIUS };
        let mut rocks: Vec<Cell> = Vec::new();
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
                    let size = if k == 0 { 1.2 + 0.5 * unit() } else { 0.55 + 0.6 * unit() };
                    boulders.push(Boulder { cell, size, seed: (unit() * 65535.0) as u32 });
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

/// Gravel bars: running water washes pebbles out of the bed and heaps them along the edges of
/// the streams. A few small ones, well apart and clear of the mud.
fn stream_pebbles(world: &World, unit: &mut impl FnMut() -> f32, keep_off: &[Cell]) -> std::collections::HashSet<Cell> {
    let n = world.size;
    let river = |c: Cell| world.river.get(c.0, c.1).copied().unwrap_or(false) && !world.lake.get(c.0, c.1).copied().unwrap_or(false);
    let near4 = |(x, y): Cell| [(x.wrapping_sub(1), y), (x + 1, y), (x, y.wrapping_sub(1)), (x, y + 1)];
    let mut spots: Vec<(f32, Cell)> = Vec::new();
    for y in 2..n.saturating_sub(2) {
        for x in 2..n.saturating_sub(2) {
            let c = (x, y);
            if river(c) && near4(c).iter().any(|&o| o.0 < n && o.1 < n && !is_water(world, o)) {
                spots.push((unit(), c));
            }
        }
    }
    spots.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
    let mut beds: Vec<Cell> = Vec::new();
    for &(_, c) in &spots {
        if beds.len() >= STREAM_PEBBLE_BEDS { break; }
        if beds.iter().all(|&b| distance(c, b) >= STREAM_PEBBLE_SPACING) && keep_off.iter().all(|&k| distance(c, k) >= STREAM_PEBBLE_MUD_GAP) {
            beds.push(c);
        }
    }
    let mut out = std::collections::HashSet::new();
    for b in beds {
        out.insert(b);
        for o in near4(b) {
            if o.0 < n && o.1 < n && river(o) && unit() < 0.5 { out.insert(o); }
        }
    }
    out
}

/// Small mud pockets, mostly along the water a step or three from it, a few in wet low ground
/// further off; clear of each other and of the big mud beds. Returns their centres.
fn mud_pockets(world: &World, reach: &[usize], unit: &mut impl FnMut() -> f32, beds: &[Cell]) -> Vec<Cell> {
    let n = world.size;
    let (mut shore, mut inland): (Vec<(f32, Cell)>, Vec<(f32, Cell)>) = (Vec::new(), Vec::new());
    for y in (4..n.saturating_sub(4)).step_by(2) {
        for x in (4..n.saturating_sub(4)).step_by(2) {
            let c = (x, y);
            if is_water(world, c) { continue; }
            let d = reach[y * n + x];
            if (1..=MUD_POCKET_WATER_REACH).contains(&d) { shore.push((unit(), c)); }
            else if d > MUD_POCKET_WATER_REACH && is_wet(world, c) { inland.push((unit(), c)); }
        }
    }
    let inland_n = (MUD_POCKETS as f32 * MUD_POCKET_INLAND_SHARE).round() as usize;
    let mut out: Vec<Cell> = Vec::new();
    for (pool, want) in [(&mut inland, inland_n), (&mut shore, MUD_POCKETS)] {
        pool.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
        for &(_, c) in pool.iter() {
            if out.len() >= want { break; }
            if out.iter().all(|&o| distance(c, o) >= MUD_POCKET_SPACING) && beds.iter().all(|&b| distance(c, b) >= MUD_BED_RADIUS + MUD_POCKET_SPACING * 0.5) {
                out.push(c);
            }
        }
    }
    out
}

fn distance(a:Cell,b:Cell)->f32 {
    ((a.0 as f32-b.0 as f32).powi(2)+(a.1 as f32-b.1 as f32).powi(2)).sqrt()
}

/// Mud arrives gradually with the flowing river; exhausted spots remain on the map.
/// Food a plant regrows a day at the season's `growth`: none out of its habitat (`wet`).
pub fn plant_regrow(r: &Resource, growth: f32) -> f32 {
    let regrow = match r.plant {
        Some(p) => p.spec().regrow,
        None if r.kind == ResourceKind::WaterPlant => WATER_PLANT_REGROW_PER_DAY,
        None => PLANT_REGROW_PER_DAY,
    };
    if r.wet { regrow * growth } else { 0.0 }
}

/// The food a plant keeps through winter, in its roots, rhizomes or canes: the cold takes the
/// rest.
pub fn plant_winter_floor(r: &Resource) -> f32 {
    r.plant.map_or(0.0, |p| p.spec().keep * r.max_amount)
}

/// The share of a plant's food lost a day to the cold at the season's `growth`.
pub fn plant_decay(growth: f32) -> f32 {
    WINTER_PLANT_DECAY_PER_DAY * (1.0 - growth)
}

pub fn river_mud_amount(amount:f32,capacity:f32,flowing:bool,days:f32)->f32 {
    (amount + if flowing {RIVER_MUD_PER_DAY*days.max(0.0)} else {0.0}).min(capacity)
}

#[cfg(test)]
mod mud_tests {
    use super::*;
    #[test]
    fn river_mud_is_limited_and_recovers_only_with_flow() {
        assert_eq!(river_mud_amount(0.0,8.0,false,360.0),0.0);
        assert!((river_mud_amount(0.0,8.0,true,25.0)-1.0).abs()<1e-5);
        assert_eq!(river_mud_amount(7.0,8.0,true,360.0),8.0);
        let mut world=ResourceWorld::default();
        let river=world.add(ResourceKind::RiverMud,(2,2),0.0);
        let deposit=world.add(ResourceKind::MudBank,(4,4),0.0);
        world.step(1.0,1.0).unwrap();
        assert!(world.get(river).is_some());assert!(world.get(deposit).is_none());
        world.get_mut(river).unwrap().wet=true;
        world.step(25.0,26.0).unwrap();assert!((world.get(river).unwrap().amount-1.0).abs()<1e-5);
        assert_eq!(world.take(river,2.0),Some((Good::Mud,1.0)));
        assert_eq!(world.take(river,1.0),None);
        world.step(25.0,51.0).unwrap();
        assert_eq!(world.take(river,1.0),Some((Good::Mud,1.0)));
        assert_eq!(world.river_mud_gathered,2.0);
        assert_eq!(world.river_mud_recovered,2.0);
    }
    #[test]
    fn bark_and_shoots_are_food_and_bark_is_sticks() {
        let mut w=ResourceWorld::default();let id=w.add(ResourceKind::Felled,(1,1),3.0);
        w.get_mut(id).unwrap().bark=2.0;
        assert_eq!(w.strip(id,1.0).unwrap(),(Good::Food,1.0));
        assert_eq!(w.take(id,1.0).unwrap(),(Good::Sticks,1.0));
        let plant=w.add(ResourceKind::Plant,(2,2),3.0);
        assert_eq!(w.take(plant,1.0).unwrap(),(Good::Food,1.0));
    }
}
