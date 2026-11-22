include!(concat!(env!("OUT_DIR"), "/resources_constants.rs"));
use crate::buildings::Good;
use crate::water::Cell;
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
    RiverClay,
}

impl ResourceKind {
    pub fn gather_sources(self) -> &'static [ResourceKind] {
        match self {
            Self::Tree | Self::Felled => &[Self::Felled, Self::Tree],
            Self::Plant => &[Self::Plant, Self::Felled, Self::Tree],
            Self::MudBank | Self::RiverClay => &[Self::RiverClay, Self::MudBank],
        }
    }
    pub fn yields(self) -> &'static [Good] {
        match self {
            ResourceKind::Tree => &[],
            ResourceKind::Felled => &[Good::Sticks, Good::Food],
            ResourceKind::Plant => &[Good::Food],
            ResourceKind::MudBank => &[Good::Clay],
            ResourceKind::RiverClay => &[Good::Clay],
        }
    }
    pub fn is_solid(self) -> bool {
        matches!(self, ResourceKind::Tree)
    }
}


#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResourceError {
    NoSuchResource,
    WrongKind,
    InvalidAmount,
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
    /// Trees and felled trees only.
    pub species: Option<crate::trees::Tree>,
}

impl Resource {
    pub fn is_solid(&self) -> bool {
        self.kind.is_solid() && !(self.kind == ResourceKind::Tree && self.stage() == Some(crate::forestry::TreeStage::Sapling))
    }
    pub fn has_yield(&self) -> bool {
        match self.kind {
            ResourceKind::RiverClay => true,
            ResourceKind::Felled => self.amount > 0.0 || self.branches > 0.0 || self.bark > 0.0,
            _ => self.amount > 0.0,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct ResourceWorld {
    pub items: Vec<Resource>,
    pub river_clay_gathered: f32,
    pub river_clay_recovered: f32,
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
            if beds.len()>=CLAY_BEDS {break;}
        }
        let in_bed=|c:Cell|beds.iter().any(|&b|distance(c,b)<=CLAY_BED_RADIUS);
        let mut river_spots: Vec<Cell> = Vec::new();
        for &(_,c) in banks.iter().rev() {
            let (x,y)=c;
            let near_river=[(x.wrapping_sub(1),y),(x+1,y),(x,y.wrapping_sub(1)),(x,y+1)]
                .iter().any(|p|world.river.get(p.0,p.1)==Some(&true));
            if near_river && !in_bed(c) && river_spots.iter().all(|&b|distance(c,b)>=RIVER_CLAY_SPACING) {river_spots.push(c);}
            if river_spots.len()>=RIVER_CLAY_SPOTS {break;}
        }
        for y in 0..n {for x in 0..n {
            let cell=(x,y);
            if let Some(Some(sp)) = world.forest.get(x,y).copied() {
                out.add_tree(cell,crate::forestry::start_age(seed,cell),sp);
            } else if river_spots.contains(&cell) {
                out.add(ResourceKind::RiverClay,cell,RIVER_CLAY_INITIAL);
            } else if is_bank(world,cell) && in_bed(cell) {
                if unit()<CLAY_FILL {out.add(ResourceKind::MudBank,cell,CLAY_AMOUNT);}
            } else if is_bank(world,cell) && unit()<STRAY_MUD_CHANCE {
                out.add(ResourceKind::MudBank,cell,STRAY_MUD_AMOUNT);
            } else if is_wet(world,cell) && unit()<PLANT_CHANCE {
                let id=out.add(ResourceKind::Plant,cell,PLANT_MAX);
                out.get_mut(id).unwrap().wet=true;
            }
        }}
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

    pub fn fell(&mut self, id: ResourceId, work: f32) -> Result<bool, ResourceError> {
        if !work.is_finite() || work < 0.0 {
            return Err(ResourceError::InvalidAmount);
        }
        let r = self.get_mut(id)?;
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
            ResourceKind::Felled if r.amount > 0.0 => (Good::Sticks, &mut r.amount),
            ResourceKind::Felled => (Good::Sticks, &mut r.branches),
            ResourceKind::Plant => (Good::Food, &mut r.amount),
            ResourceKind::MudBank => (Good::Clay, &mut r.amount),
            ResourceKind::RiverClay => (Good::Clay, &mut r.amount),
        };
        let got = want.min(*slot);
        if got <= 0.0 {
            return None;
        }
        *slot -= got;
        if r.kind == ResourceKind::RiverClay { self.river_clay_gathered += got; }
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
        for r in self.items.iter_mut().filter(|r|r.kind==ResourceKind::RiverClay) {
            let before = r.amount;
            r.amount=river_clay_amount(r.amount,r.max_amount,r.wet,dt_days);
            self.river_clay_recovered += r.amount - before;
        }
        self.items.retain(|r| matches!(r.kind,ResourceKind::Plant | ResourceKind::RiverClay) || r.has_yield());
        Ok(())
    }

    pub fn add(&mut self, kind: ResourceKind, cell: Cell, amount: f32) -> ResourceId {
        self.next_id += 1;
        self.nav_revision = self.nav_revision.wrapping_add(1);
        let id = ResourceId(self.next_id);
        let age_days = if kind == ResourceKind::Tree { crate::forestry::DEFAULT_TREE_AGE } else { 0.0 };
        self.items.push(Resource { id, kind, cell, amount, max_amount: if kind==ResourceKind::RiverClay { RIVER_CLAY_CAPACITY } else { amount }, branches: 0.0, bark: 0.0, wet: false, age_days, snag: false, tended_days: 0.0, species: None });
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

fn distance(a:Cell,b:Cell)->f32 {
    ((a.0 as f32-b.0 as f32).powi(2)+(a.1 as f32-b.1 as f32).powi(2)).sqrt()
}

/// Clay arrives gradually with the flowing river; exhausted spots remain on the map.
pub fn river_clay_amount(amount:f32,capacity:f32,flowing:bool,days:f32)->f32 {
    (amount + if flowing {RIVER_CLAY_PER_DAY*days.max(0.0)} else {0.0}).min(capacity)
}

#[cfg(test)]
mod clay_tests {
    use super::*;
    #[test]
    fn river_clay_is_limited_and_recovers_only_with_flow() {
        assert_eq!(river_clay_amount(0.0,8.0,false,360.0),0.0);
        assert!((river_clay_amount(0.0,8.0,true,25.0)-1.0).abs()<1e-5);
        assert_eq!(river_clay_amount(7.0,8.0,true,360.0),8.0);
        let mut world=ResourceWorld::default();
        let river=world.add(ResourceKind::RiverClay,(2,2),0.0);
        let deposit=world.add(ResourceKind::MudBank,(4,4),0.0);
        world.step(1.0,1.0).unwrap();
        assert!(world.get(river).is_some());assert!(world.get(deposit).is_none());
        world.get_mut(river).unwrap().wet=true;
        world.step(25.0,26.0).unwrap();assert!((world.get(river).unwrap().amount-1.0).abs()<1e-5);
        assert_eq!(world.take(river,2.0),Some((Good::Clay,1.0)));
        assert_eq!(world.take(river,1.0),None);
        world.step(25.0,51.0).unwrap();
        assert_eq!(world.take(river,1.0),Some((Good::Clay,1.0)));
        assert_eq!(world.river_clay_gathered,2.0);
        assert_eq!(world.river_clay_recovered,2.0);
    }
    #[test]
    fn bark_and_shoots_are_food_and_timber_is_sticks() {
        let mut w=ResourceWorld::default();let id=w.add(ResourceKind::Felled,(1,1),3.0);
        w.get_mut(id).unwrap().bark=2.0;
        assert_eq!(w.strip(id,1.0).unwrap(),(Good::Food,1.0));
        assert_eq!(w.take(id,1.0).unwrap(),(Good::Sticks,1.0));
        let plant=w.add(ResourceKind::Plant,(2,2),3.0);
        assert_eq!(w.take(plant,1.0).unwrap(),(Good::Food,1.0));
    }
}
