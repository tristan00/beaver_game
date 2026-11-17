//! Neighbouring AI beaver colonies in other valleys.
//!
//! Colonies grow by simple procedural rules (population, stores, specialty) and are mostly
//! friendly. Linking one by water or path ([`Region::connect`]) opens trade offers
//! ([`TradeOffer`], accepted with [`Region::accept_offer`]) and lets beavers move between
//! colonies ([`Region::send_migrants`], [`Region::take_arrivals`]). Deterministic per seed.

use crate::events::EventHost;
use rand_core::{Rng, SeedableRng};
use rand_pcg::Pcg32;
use std::fmt;

/// Friendliness below this refuses contact.
pub const WARY_BELOW: f32 = 0.3;
pub const MAX_POPULATION: f32 = 40.0;
pub const MAX_STORE: f32 = 500.0;
pub const OFFER_LIFETIME_DAYS: f64 = 15.0;
pub const MAX_OPEN_OFFERS: usize = 3;

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
    /// Position on the regional map.
    pub pos: (f32, f32),
    pub population: f32,
    pub food: f32,
    pub wood: f32,
    pub specialty: Specialty,
    /// `0..=1`; below [`WARY_BELOW`] the colony refuses contact.
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

/// A neighbour offers `give` of one resource for `want` of the other.
#[derive(Clone, Debug, PartialEq)]
pub struct TradeOffer {
    pub id: u64,
    pub colony: u32,
    pub give: (Resource, f32),
    pub want: (Resource, f32),
    pub expires_day: f64,
}

/// Beavers that moved from a neighbour into the player's colony.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Migrants {
    pub from: u32,
    pub count: u32,
}

#[derive(Clone, Debug, PartialEq)]
pub enum RegionError {
    UnknownColony(u32),
    AlreadyConnected(u32),
    /// The colony is wary and refuses to open a route.
    Refused(u32),
    NotConnected(u32),
    UnknownOffer(u64),
    /// The player cannot afford the offer.
    CannotAfford { needed: f32, have: f32 },
    /// The neighbour no longer holds enough to honour the offer (the offer is withdrawn).
    NeighbourShort(u32),
    /// Zero migrants, or more than the destination can hold.
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
    /// Generates 3–5 neighbouring colonies; about one in five is wary.
    pub fn new(seed: u64) -> Self {
        let mut rng = Pcg32::seed_from_u64(seed ^ 0x2E61_0000);
        let n = 3 + rng.next_u32() % 3;
        let mut colonies = Vec::new();
        for i in 0..n {
            let wary = unit(&mut rng) < 0.2;
            let friendliness = if wary { 0.05 + 0.2 * unit(&mut rng) } else { 0.5 + 0.5 * unit(&mut rng) };
            let population = 6.0 + 14.0 * unit(&mut rng);
            let specialty = SPECIALTIES[((unit(&mut rng) * 4.0) as usize).min(3)];
            let pos = (unit(&mut rng) * 100.0, unit(&mut rng) * 100.0);
            let food = population * (2.0 + 4.0 * unit(&mut rng));
            let wood = population * (1.0 + 2.0 * unit(&mut rng));
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

    /// Opens a water or path route to colony `id`.
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

    /// Closes the route (e.g. a path floods); open offers from that colony are withdrawn.
    pub fn disconnect(&mut self, id: u32) -> Result<(), RegionError> {
        let c = self.colony_mut(id)?;
        if c.link.take().is_none() {
            return Err(RegionError::NotConnected(id));
        }
        self.offers.retain(|o| o.colony != id);
        Ok(())
    }

    /// Accepts an offer: the host pays `want`, receives `give`. Nothing changes on error.
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

    /// Moves `count` beavers from the player's colony to neighbour `id` (the caller removes them).
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

    /// Drains neighbours' beavers that moved to the player's colony; the caller adds them.
    pub fn take_arrivals(&mut self) -> Vec<Migrants> {
        std::mem::take(&mut self.arrivals)
    }

    /// Advances `dt_days` (finite, >= 0) in sub-steps of at most one day.
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
            let (food_gain, wood_gain) = match c.specialty {
                Specialty::Fishing => (0.06, 0.0),
                Specialty::Farming => (0.08, 0.0),
                Specialty::Lumber => (0.0, 0.07),
                Specialty::Crafts => (0.0, 0.04),
            };
            // Everyone eats 0.05 food/day and wears 0.02 wood/day; base gathering covers most.
            c.food = (c.food + c.population * (0.04 + food_gain - 0.05) * dt).clamp(0.0, MAX_STORE);
            c.wood = (c.wood + c.population * (0.015 + wood_gain - 0.02) * dt).clamp(0.0, MAX_STORE);
            let rate = if c.food > c.population * 3.0 { 0.01 * (1.0 - c.population / MAX_POPULATION) } else { -0.01 };
            c.population = (c.population * (1.0 + rate * dt)).clamp(2.0, MAX_POPULATION);
        }
        for i in 0..self.colonies.len() {
            let c = &self.colonies[i];
            if !c.is_connected() {
                continue;
            }
            let (id, pop, res, friendliness) = (c.id, c.population, c.surplus_resource(), c.friendliness);
            let surplus = c.stock(res) - pop * 3.0;
            // Crowded neighbours send migrants.
            if pop > 0.8 * MAX_POPULATION && unit(&mut self.rng) < 0.1 * friendliness * dt {
                self.colonies[i].population -= 1.0;
                self.arrivals.push(Migrants { from: id, count: 1 });
            }
            let open = self.offers.iter().filter(|o| o.colony == id).count();
            if open < MAX_OPEN_OFFERS && surplus > 4.0 && unit(&mut self.rng) < 0.15 * dt {
                let give = (surplus * (0.2 + 0.3 * unit(&mut self.rng))).max(2.0);
                let want = give * (0.7 + 0.6 * unit(&mut self.rng));
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::Stockpile;

    fn host() -> Stockpile {
        Stockpile { population: 10, food: 1000.0, wood: 1000.0, health: 1.0 }
    }

    /// Id of a friendly colony.
    fn friendly(r: &Region) -> u32 {
        r.colonies.iter().find(|c| c.friendliness >= WARY_BELOW).unwrap().id
    }

    fn rich_connected(seed: u64) -> (Region, u32) {
        let mut r = Region::new(seed);
        let id = friendly(&r);
        r.connect(id, Link::Path).unwrap();
        r.colonies[id as usize].food = 300.0;
        r.colonies[id as usize].wood = 300.0;
        (r, id)
    }

    /// Steps day by day until an offer is open (offers are random but must appear eventually).
    fn first_offer(r: &mut Region) -> TradeOffer {
        for _ in 0..100 {
            r.step(1.0);
            if let Some(o) = r.offers().first() {
                return o.clone();
            }
        }
        panic!("no offer within 100 days");
    }

    #[test]
    fn region_generation_is_deterministic_and_mostly_friendly() {
        assert_eq!(Region::new(5).colonies, Region::new(5).colonies);
        let all: Vec<_> = (0..50u64).flat_map(|s| Region::new(s).colonies).collect();
        let friendly = all.iter().filter(|c| c.friendliness >= WARY_BELOW).count();
        assert!(friendly * 10 >= all.len() * 7, "{friendly}/{}", all.len());
    }

    #[test]
    fn region_colonies_grow_and_stay_bounded() {
        let mut r = Region::new(2);
        r.step(400.0);
        for c in &r.colonies {
            assert!((2.0..=MAX_POPULATION).contains(&c.population));
            assert!((0.0..=MAX_STORE).contains(&c.food));
        }
    }

    #[test]
    fn region_connection_rules() {
        let mut r = Region::new(2);
        let id = friendly(&r);
        assert_eq!(r.connect(99, Link::Path), Err(RegionError::UnknownColony(99)));
        r.connect(id, Link::Water).unwrap();
        assert_eq!(r.connect(id, Link::Path), Err(RegionError::AlreadyConnected(id)));
        let other = r.colonies.iter().find(|c| !c.is_connected()).unwrap().id;
        r.colonies[other as usize].friendliness = 0.1;
        assert_eq!(r.connect(other, Link::Path), Err(RegionError::Refused(other)));
        assert_eq!(r.disconnect(other), Err(RegionError::NotConnected(other)));
    }

    #[test]
    fn region_connected_colonies_trade() {
        let mut r = Region::new(2);
        r.step(30.0);
        assert!(r.offers().is_empty(), "no offers without a connection");
        let (mut r, _) = rich_connected(2);
        let offer = first_offer(&mut r);
        let mut h = host();
        r.accept_offer(offer.id, &mut h).unwrap();
        let (got, paid) = match offer.give.0 {
            Resource::Food => (h.food - 1000.0, 1000.0 - h.wood),
            Resource::Wood => (h.wood - 1000.0, 1000.0 - h.food),
        };
        assert!((got - offer.give.1).abs() < 1e-3 && (paid - offer.want.1).abs() < 1e-3);
        assert_eq!(r.accept_offer(offer.id, &mut h), Err(RegionError::UnknownOffer(offer.id)));
    }

    #[test]
    fn region_unaffordable_offer_changes_nothing() {
        let (mut r, _) = rich_connected(2);
        let offer = first_offer(&mut r);
        let mut h = Stockpile { food: 0.0, wood: 0.0, ..host() };
        assert!(matches!(r.accept_offer(offer.id, &mut h), Err(RegionError::CannotAfford { .. })));
        assert_eq!((h.food, h.wood), (0.0, 0.0));
        assert!(r.offers().iter().any(|o| o.id == offer.id));
    }

    #[test]
    fn region_migrants_arrive_and_can_be_sent() {
        let (mut r, id) = rich_connected(2);
        r.colonies[id as usize].population = MAX_POPULATION;
        r.step(60.0);
        assert!(r.take_arrivals().iter().any(|m| m.from == id && m.count > 0));
        assert!(r.take_arrivals().is_empty());

        r.colonies[id as usize].population = 10.0;
        r.send_migrants(id, 3).unwrap();
        assert_eq!(r.colonies[id as usize].population, 13.0);
        assert_eq!(r.send_migrants(id, 0), Err(RegionError::InvalidMigrants(0)));
        let other = r.colonies.iter().find(|c| !c.is_connected()).unwrap().id;
        assert_eq!(r.send_migrants(other, 1), Err(RegionError::NotConnected(other)));
    }

    #[test]
    fn region_deterministic_stepping() {
        let run = || {
            let (mut r, _) = rich_connected(9);
            r.step(100.0);
            (r.colonies.clone(), r.offers().to_vec(), r.take_arrivals())
        };
        assert_eq!(run(), run());
    }
}
