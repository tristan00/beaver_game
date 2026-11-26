//! Physical stored goods.
use std::collections::BTreeMap;
use crate::buildings::{BuildingId, Good};
use crate::plants::Plant;
use crate::trees::Tree;

/// What a seedling grows into: a tree, or a food plant on land or in water.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
pub enum Species {
    Tree(Tree),
    Plant(Plant),
}

impl Species {
    pub fn name(self) -> &'static str {
        match self {
            Species::Tree(t) => t.name(),
            Species::Plant(p) => p.name(),
        }
    }
    /// Which of the three sorts of plant it is: a tree, a land plant or a water plant.
    pub fn sort(self) -> SeedlingSort {
        match self {
            Species::Tree(_) => SeedlingSort::Tree,
            Species::Plant(p) if p.aquatic() => SeedlingSort::Water,
            Species::Plant(_) => SeedlingSort::Land,
        }
    }
}

/// The three sorts of plant a seedling bed raises, kept in balance (see `agents::seed_task`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum SeedlingSort {
    Tree,
    Land,
    Water,
}

/// What kind of a good: seedlings keep the species they will grow into; every other good is `None`.
pub type Kind = Option<Species>;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Inventory(BTreeMap<(BuildingId, Good, Kind), f32>);

impl serde::Serialize for Inventory {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let stock: Vec<((BuildingId, Good, Kind), f32)> = self.0.iter().map(|(k, v)| (*k, *v)).collect();
        stock.serialize(s)
    }
}
impl<'de> serde::Deserialize<'de> for Inventory {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let stock: Vec<((BuildingId, Good, Kind), f32)> = serde::Deserialize::deserialize(d)?;
        Ok(Self(stock.into_iter().collect()))
    }
}

impl Inventory {
    pub fn at(&self, owner: BuildingId, good: Good) -> f32 {
        self.0.range((owner, good, None)..).take_while(|((o, g, _), _)| (*o, *g) == (owner, good)).map(|(_, a)| *a).sum()
    }
    /// One store's stock of a good, kind by kind.
    pub fn kinds(&self, owner: BuildingId, good: Good) -> Vec<(Kind, f32)> {
        self.0.range((owner, good, None)..).take_while(|((o, g, _), _)| (*o, *g) == (owner, good)).map(|((_, _, k), a)| (*k, *a)).collect()
    }
    pub fn total(&self, good: Good) -> f32 {
        self.0.iter().filter(|((_, g, _), _)| *g == good).map(|(_, a)| a).sum()
    }
    pub(crate) fn put(&mut self, owner: BuildingId, good: Good, kind: Kind, amount: f32) {
        assert!(amount.is_finite() && amount >= 0.0);
        if amount > 0.0 { *self.0.entry((owner, good, kind)).or_default() += amount; }
    }
    /// Take of any kinds, the first kinds first.
    pub(crate) fn take(&mut self, owner: BuildingId, good: Good, amount: f32) -> f32 {
        let mut taken = 0.0;
        while taken < amount {
            let (got, _) = self.take_kind(owner, good, amount - taken);
            if got <= 0.0 { break; }
            taken += got;
        }
        taken
    }
    /// Take of one kind only, the one most stocked, so a load is all one kind.
    pub(crate) fn take_kind(&mut self, owner: BuildingId, good: Good, amount: f32) -> (f32, Kind) {
        let Some(kind) = self.kinds(owner, good).into_iter().max_by(|a, b| a.1.total_cmp(&b.1).then(b.0.cmp(&a.0))).map(|k| k.0) else { return (0.0, None) };
        (self.take_this(owner, good, kind, amount), kind)
    }
    /// Take of this one kind only.
    pub(crate) fn take_this(&mut self, owner: BuildingId, good: Good, kind: Kind, amount: f32) -> f32 {
        assert!(amount.is_finite() && amount >= 0.0);
        let have = self.0.entry((owner, good, kind)).or_default();
        let taken = amount.min(*have);
        *have -= taken;
        if *have == 0.0 { self.0.remove(&(owner, good, kind)); }
        taken
    }
    pub(crate) fn consume(&mut self, good: Good, amount: f32) {
        let mut left = amount;
        for (_, have) in self.0.iter_mut().filter(|((_, g, _), _)| *g == good) {
            let take = left.min(*have);
            *have -= take;
            left -= take;
            if left <= 0.0 { break; }
        }
        self.0.retain(|_, a| *a > 0.0);
    }
}
