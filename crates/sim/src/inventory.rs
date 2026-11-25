//! Physical stored goods. Handles share a store within one world; clones own
//! independent data, so simulation snapshots never expose mutable live stock.
use std::{collections::BTreeMap, sync::{Arc, RwLock}};
use crate::buildings::{BuildingId, Good};
use crate::trees::Tree;

/// What kind of a good: saplings keep the species they will grow into; every other good is `None`.
pub type Kind = Option<Tree>;

#[derive(Debug, Default)]
pub struct Inventory(Arc<RwLock<BTreeMap<(BuildingId, Good, Kind), f32>>>);

/// Saved as its stock; a loaded inventory is its own store (views are re-linked after loading).
impl serde::Serialize for Inventory {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let stock: Vec<((BuildingId, Good, Kind), f32)> = self.0.read().unwrap().iter().map(|(k, v)| (*k, *v)).collect();
        stock.serialize(s)
    }
}
impl<'de> serde::Deserialize<'de> for Inventory {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let stock: Vec<((BuildingId, Good, Kind), f32)> = serde::Deserialize::deserialize(d)?;
        Ok(Self(Arc::new(RwLock::new(stock.into_iter().collect()))))
    }
}

impl Clone for Inventory {
    fn clone(&self) -> Self { Self(Arc::new(RwLock::new(self.0.read().unwrap().clone()))) }
}
impl PartialEq for Inventory {
    fn eq(&self, other: &Self) -> bool { *self.0.read().unwrap() == *other.0.read().unwrap() }
}
impl Inventory {
    pub(crate) fn view(&self) -> Self { Self(Arc::clone(&self.0)) }
    pub fn at(&self, owner: BuildingId, good: Good) -> f32 {
        self.kinds(owner, good).iter().map(|k| k.1).sum()
    }
    /// One store's stock of a good, kind by kind.
    pub fn kinds(&self, owner: BuildingId, good: Good) -> Vec<(Kind, f32)> {
        self.0.read().unwrap().range((owner, good, None)..).take_while(|((o, g, _), _)| (*o, *g) == (owner, good)).map(|((_, _, k), a)| (*k, *a)).collect()
    }
    pub fn total(&self, good: Good) -> f32 {
        self.0.read().unwrap().iter().filter(|((_, g, _), _)| *g == good).map(|(_, a)| a).sum()
    }
    pub(crate) fn put(&self, owner: BuildingId, good: Good, kind: Kind, amount: f32) {
        assert!(amount.is_finite() && amount >= 0.0);
        if amount > 0.0 { *self.0.write().unwrap().entry((owner, good, kind)).or_default() += amount; }
    }
    /// Take of any kinds, the first kinds first.
    pub(crate) fn take(&self, owner: BuildingId, good: Good, amount: f32) -> f32 {
        let mut taken = 0.0;
        while taken < amount {
            let (got, _) = self.take_kind(owner, good, amount - taken);
            if got <= 0.0 { break; }
            taken += got;
        }
        taken
    }
    /// Take of one kind only, the one most stocked, so a load is all one kind.
    pub(crate) fn take_kind(&self, owner: BuildingId, good: Good, amount: f32) -> (f32, Kind) {
        assert!(amount.is_finite() && amount >= 0.0);
        let Some(kind) = self.kinds(owner, good).into_iter().max_by(|a, b| a.1.total_cmp(&b.1).then(b.0.cmp(&a.0))).map(|k| k.0) else { return (0.0, None) };
        let mut stock = self.0.write().unwrap();
        let have = stock.entry((owner, good, kind)).or_default();
        let taken = amount.min(*have);
        *have -= taken;
        if *have == 0.0 { stock.remove(&(owner, good, kind)); }
        (taken, kind)
    }
    pub(crate) fn consume(&self, good: Good, amount: f32) {
        let mut stock = self.0.write().unwrap();
        let mut left = amount;
        for (_, have) in stock.iter_mut().filter(|((_, g, _), _)| *g == good) {
            let take = left.min(*have);
            *have -= take;
            left -= take;
            if left <= 0.0 { break; }
        }
        stock.retain(|_, a| *a > 0.0);
    }
}
