use crate::events::EventHost;
use rand_core::{Rng, SeedableRng};
use rand_pcg::Pcg32;
use std::fmt;
pub use crate::tuning::region::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Specialty {
    Fishing,
    Lumber,
    Farming,
    Crafts,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Resource {
    Food,
    Wood,
}

impl Resource {
    fn other(self) -> Resource {
        match self {
            Resource::Food => Resource::Wood,
            Resource::Wood => Resource::Food,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Link {
    Water,
    Path,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Neighbour {
    pub id: u32,
    pub name: String,
    pub pos: (f32, f32),
    pub population: f32,
    pub food: f32,
    pub wood: f32,
    pub specialty: Specialty,
    pub friendliness: f32,
    pub link: Option<Link>,
}

impl Neighbour {
    pub fn is_connected(&self) -> bool {
        self.link.is_some()
    }
    fn surplus_resource(&self) -> Resource {
        match self.specialty {
            Specialty::Fishing | Specialty::Farming => Resource::Food,
            Specialty::Lumber | Specialty::Crafts => Resource::Wood,
        }
    }
    fn stock_mut(&mut self, r: Resource) -> &mut f32 {
        match r {
            Resource::Food => &mut self.food,
            Resource::Wood => &mut self.wood,
        }
    }
    pub fn stock(&self, r: Resource) -> f32 {
        match r {
            Resource::Food => self.food,
            Resource::Wood => self.wood,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct TradeOffer {
    pub id: u64,
    pub colony: u32,
    pub give: (Resource, f32),
    pub want: (Resource, f32),
    pub expires_day: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Migrants {
    pub from: u32,
    pub count: u32,
}

#[derive(Clone, Debug, PartialEq)]
pub enum RegionError {
    UnknownColony(u32),
    AlreadyConnected(u32),
    Refused(u32),
    NotConnected(u32),
    UnknownOffer(u64),
    CannotAfford { needed: f32, have: f32 },
    NeighbourShort(u32),
    InvalidMigrants(u32),
}

impl fmt::Display for RegionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RegionError::UnknownColony(i) => write!(f, "no neighbouring colony {i}"),
            RegionError::AlreadyConnected(i) => write!(f, "colony {i} is already connected"),
            RegionError::Refused(i) => write!(f, "colony {i} refuses contact"),
            RegionError::NotConnected(i) => write!(f, "colony {i} is not connected"),
            RegionError::UnknownOffer(i) => write!(f, "no open trade offer {i}"),
            RegionError::CannotAfford { needed, have } => write!(f, "need {needed:.1} but only have {have:.1}"),
            RegionError::NeighbourShort(i) => write!(f, "colony {i} can no longer supply the offer"),
            RegionError::InvalidMigrants(n) => write!(f, "cannot move {n} migrants"),
        }
    }
}
impl std::error::Error for RegionError {}

const NAMES: [&str; 5] = ["Alder Bend", "Cattail Run", "Birchwater", "Mudslide Hollow", "Willow Reach"];
const SPECIALTIES: [Specialty; 4] = [Specialty::Fishing, Specialty::Lumber, Specialty::Farming, Specialty::Crafts];
const _: () = assert!((MIN_NEIGHBOURS + NEIGHBOUR_SPREAD - 1) as usize <= NAMES.len());

fn unit(rng: &mut Pcg32) -> f32 {
    (rng.next_u32() >> 8) as f32 / (1u32 << 24) as f32
}

#[derive(Clone, Debug)]
pub struct Region {
    rng: Pcg32,
    pub time_days: f64,
    pub colonies: Vec<Neighbour>,
    offers: Vec<TradeOffer>,
    arrivals: Vec<Migrants>,
    next_offer: u64,
}

impl Default for Region {
    fn default() -> Self {
        Self::new(0)
    }
}

impl Region {
    pub fn none(seed: u64) -> Self {
        Self { rng: Pcg32::seed_from_u64(seed ^ 0x2E61_0000), time_days: 0.0, colonies: Vec::new(), offers: Vec::new(), arrivals: Vec::new(), next_offer: 0 }
    }

    pub fn new(seed: u64) -> Self {
        let mut rng = Pcg32::seed_from_u64(seed ^ 0x2E61_0000);
        let n = MIN_NEIGHBOURS + rng.next_u32() % NEIGHBOUR_SPREAD;
        let mut colonies = Vec::new();
        for i in 0..n {
            let wary = unit(&mut rng) < WARY_CHANCE;
            let friendliness = if wary {
                WARY_FRIENDLINESS_MIN + WARY_FRIENDLINESS_SPREAD * unit(&mut rng)
            } else {
                FRIENDLINESS_MIN + FRIENDLINESS_SPREAD * unit(&mut rng)
            };
            let population = START_POPULATION_MIN + START_POPULATION_SPREAD * unit(&mut rng);
            let specialty = SPECIALTIES[((unit(&mut rng) * 4.0) as usize).min(3)];
            let pos = (unit(&mut rng) * REGION_EXTENT, unit(&mut rng) * REGION_EXTENT);
            let food = population * (START_FOOD_PER_HEAD_MIN + START_FOOD_PER_HEAD_SPREAD * unit(&mut rng));
            let wood = population * (START_WOOD_PER_HEAD_MIN + START_WOOD_PER_HEAD_SPREAD * unit(&mut rng));
            colonies.push(Neighbour {
                id: i,
                name: NAMES[i as usize].to_string(),
                pos,
                population,
                food,
                wood,
                specialty,
                friendliness,
                link: None,
            });
        }
        Self { rng, time_days: 0.0, colonies, offers: Vec::new(), arrivals: Vec::new(), next_offer: 1 }
    }

    pub fn colony(&self, id: u32) -> Result<&Neighbour, RegionError> {
        self.colonies.get(id as usize).ok_or(RegionError::UnknownColony(id))
    }
    fn colony_mut(&mut self, id: u32) -> Result<&mut Neighbour, RegionError> {
        self.colonies.get_mut(id as usize).ok_or(RegionError::UnknownColony(id))
    }
    pub fn offers(&self) -> &[TradeOffer] {
        &self.offers
    }

    pub fn connect(&mut self, id: u32, link: Link) -> Result<(), RegionError> {
        let c = self.colony_mut(id)?;
        if c.link.is_some() {
            return Err(RegionError::AlreadyConnected(id));
        }
        if c.friendliness < WARY_BELOW {
            return Err(RegionError::Refused(id));
        }
        c.link = Some(link);
        Ok(())
    }

    pub fn disconnect(&mut self, id: u32) -> Result<(), RegionError> {
        let c = self.colony_mut(id)?;
        if c.link.take().is_none() {
            return Err(RegionError::NotConnected(id));
        }
        self.offers.retain(|o| o.colony != id);
        Ok(())
    }

    pub fn accept_offer(&mut self, offer_id: u64, host: &mut dyn EventHost) -> Result<(), RegionError> {
        let idx = self.offers.iter().position(|o| o.id == offer_id).ok_or(RegionError::UnknownOffer(offer_id))?;
        let o = self.offers[idx].clone();
        let have = match o.want.0 {
            Resource::Food => host.food(),
            Resource::Wood => host.wood(),
        };
        if have < o.want.1 {
            return Err(RegionError::CannotAfford { needed: o.want.1, have });
        }
        if self.colony(o.colony)?.stock(o.give.0) < o.give.1 {
            self.offers.remove(idx);
            return Err(RegionError::NeighbourShort(o.colony));
        }
        let c = self.colony_mut(o.colony)?;
        *c.stock_mut(o.give.0) -= o.give.1;
        let got = (c.stock(o.want.0) + o.want.1).min(MAX_STORE);
        *c.stock_mut(o.want.0) = got;
        for (r, amt) in [(o.want.0, -o.want.1), (o.give.0, o.give.1)] {
            match r {
                Resource::Food => host.add_food(amt),
                Resource::Wood => host.add_wood(amt),
            }
        }
        self.offers.remove(idx);
        Ok(())
    }

    pub fn send_migrants(&mut self, id: u32, count: u32) -> Result<(), RegionError> {
        let c = self.colony_mut(id)?;
        if c.link.is_none() {
            return Err(RegionError::NotConnected(id));
        }
        if count == 0 || c.population + count as f32 > MAX_POPULATION {
            return Err(RegionError::InvalidMigrants(count));
        }
        c.population += count as f32;
        Ok(())
    }

    pub fn take_arrivals(&mut self) -> Vec<Migrants> {
        std::mem::take(&mut self.arrivals)
    }

    pub fn step(&mut self, dt_days: f32) {
        let mut left = dt_days as f64;
        while left > 0.0 {
            let sub = left.min(1.0);
            left -= sub;
            self.time_days += sub;
            self.day(sub as f32);
        }
    }

    fn day(&mut self, dt: f32) {
        let now = self.time_days;
        self.offers.retain(|o| o.expires_day > now);
        for c in &mut self.colonies {
            let (food_gain, wood_gain) = specialty_gain(c.specialty);
            c.food = (c.food + c.population * (BASE_FOOD_GAIN + food_gain - FOOD_USE) * dt).clamp(0.0, MAX_STORE);
            c.wood = (c.wood + c.population * (BASE_WOOD_GAIN + wood_gain - WOOD_USE) * dt).clamp(0.0, MAX_STORE);
            let rate = if c.food > c.population * RESERVE_PER_HEAD {
                GROWTH_RATE * (1.0 - c.population / MAX_POPULATION)
            } else {
                -DECLINE_RATE
            };
            c.population = (c.population * (1.0 + rate * dt)).clamp(MIN_POPULATION, MAX_POPULATION);
        }
        for i in 0..self.colonies.len() {
            let c = &self.colonies[i];
            if !c.is_connected() {
                continue;
            }
            let (id, pop, res, friendliness) = (c.id, c.population, c.surplus_resource(), c.friendliness);
            let surplus = c.stock(res) - pop * RESERVE_PER_HEAD;
            if pop > CROWDED_SHARE * MAX_POPULATION && unit(&mut self.rng) < MIGRATION_CHANCE * friendliness * dt {
                self.colonies[i].population -= 1.0;
                self.arrivals.push(Migrants { from: id, count: 1 });
            }
            let open = self.offers.iter().filter(|o| o.colony == id).count();
            if open < MAX_OPEN_OFFERS && surplus > MIN_OFFER_SURPLUS && unit(&mut self.rng) < OFFER_CHANCE * dt {
                let give = (surplus * (OFFER_SHARE_MIN + OFFER_SHARE_SPREAD * unit(&mut self.rng))).max(MIN_OFFER);
                let want = give * (PRICE_MIN + PRICE_SPREAD * unit(&mut self.rng));
                self.offers.push(TradeOffer {
                    id: self.next_offer,
                    colony: id,
                    give: (res, give),
                    want: (res.other(), want),
                    expires_day: now + OFFER_LIFETIME_DAYS,
                });
                self.next_offer += 1;
            }
        }
    }
}

