//! Food plants: the species each map's real wetlands grow that beavers eat, where each roots,
//! how fast it grows and spreads, and what of it lasts the winter.
//!
//! Every map draws its plants from its own region's flora (`plant_flora`): wild raspberry in
//! the clearings, Joe-Pye weed (North America) or meadowsweet (Europe) in the wet meadows,
//! sedge, horsetail and cattail along the waterline, and arrowhead, yellow pond lily, white
//! water lily and pondweed in the water. Each spreads two ways, as it does in life: by creeping
//! roots into the ground next to it, and now and then by seed further off. What it keeps over
//! winter is what beavers dig for then: cattail, arrowhead and lily rhizomes, raspberry canes.
include!(concat!(env!("OUT_DIR"), "/plants_constants.rs"));

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
pub enum Plant { Sedge, CanaryGrass, Meadowsweet, JoePye, Raspberry, Horsetail, Cattail, Arrowhead, YellowLily, WhiteLily, Pondweed }

/// Where a species roots.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Habitat {
    /// Open, moist ground away from the water: clearings and forest edges.
    Upland,
    /// Wet ground: meadows and banks.
    Meadow,
    /// The waterline: wet banks and the shallowest water.
    Shore,
    /// Standing in water between its depths.
    Water,
}

/// One species' food and growth. Rates are a day in full growing season.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlantSpec {
    pub habitat: Habitat,
    /// The most food one plant holds.
    pub food: f32,
    pub regrow: f32,
    /// Chance a grown plant sends up a new one next to it from its roots.
    pub creep: f32,
    /// Chance a grown plant's seed takes root up to `SEED_REACH` cells off.
    pub seed: f32,
    /// The share of its food a plant keeps through winter, in roots, rhizomes or canes.
    pub keep: f32,
    pub min_depth: f32,
    pub max_depth: f32,
}

impl Plant {
    pub const ALL: [Plant; 11] = [
        Plant::Sedge, Plant::CanaryGrass, Plant::Meadowsweet, Plant::JoePye, Plant::Raspberry, Plant::Horsetail,
        Plant::Cattail, Plant::Arrowhead, Plant::YellowLily, Plant::WhiteLily, Plant::Pondweed,
    ];

    pub fn spec(self) -> &'static PlantSpec {
        &SPECS[self as usize]
    }

    pub fn name(self) -> &'static str {
        match self {
            Plant::Sedge => "Sedge",
            Plant::CanaryGrass => "Reed canary grass",
            Plant::Meadowsweet => "Meadowsweet",
            Plant::JoePye => "Joe-Pye weed",
            Plant::Raspberry => "Wild raspberry",
            Plant::Horsetail => "Water horsetail",
            Plant::Cattail => "Cattail",
            Plant::Arrowhead => "Arrowhead",
            Plant::YellowLily => "Yellow pond lily",
            Plant::WhiteLily => "White water lily",
            Plant::Pondweed => "Pondweed",
        }
    }

    /// Stands in open water: drawn floating or rising from it, and gathered by swimmers.
    pub fn aquatic(self) -> bool {
        self.spec().habitat == Habitat::Water
    }

    pub fn kind(self) -> crate::resources::ResourceKind {
        if self.aquatic() { crate::resources::ResourceKind::WaterPlant } else { crate::resources::ResourceKind::Plant }
    }
}

/// A map's flora: how common each species is where it can grow.
pub fn flora(map: &str) -> &'static [f32; 11] {
    let find = |m: &str| FLORA.iter().find(|f| f.0 == m).map(|f| &f.1);
    find(map).or_else(|| find("default")).expect("content defines a default plant flora")
}

/// A species from the flora for ground of this habitat (`fits` says which species it suits),
/// `draw` in 0..1 choosing among them by their weights.
pub fn choose(map: &str, draw: f32, fits: impl Fn(Plant) -> bool) -> Option<Plant> {
    let mix = flora(map);
    let total: f32 = Plant::ALL.iter().filter(|p| fits(**p)).map(|p| mix[*p as usize]).sum();
    if total <= 0.0 {
        return None;
    }
    let mut left = draw.clamp(0.0, 0.9999) * total;
    for p in Plant::ALL.into_iter().filter(|p| fits(*p)) {
        left -= mix[p as usize];
        if left < 0.0 && mix[p as usize] > 0.0 {
            return Some(p);
        }
    }
    Plant::ALL.into_iter().rev().find(|p| fits(*p) && mix[*p as usize] > 0.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_flora_has_a_plant_for_every_habitat_and_regions_differ() {
        for h in [Habitat::Upland, Habitat::Meadow, Habitat::Shore, Habitat::Water] {
            assert!(choose("algonquin", 0.5, |p| p.spec().habitat == h).is_some());
            assert!(choose("telemark", 0.5, |p| p.spec().habitat == h).is_some());
        }
        assert_eq!(flora("algonquin")[Plant::Meadowsweet as usize], 0.0, "meadowsweet is European");
        assert_eq!(flora("bavarian")[Plant::JoePye as usize], 0.0, "Joe-Pye weed is North American");
        assert_eq!(flora("nowhere"), flora("default"));
    }

    #[test]
    fn water_plants_are_rich_and_rhizomes_last_the_winter() {
        assert!(Plant::YellowLily.spec().food > Plant::Sedge.spec().food);
        assert!(Plant::Cattail.spec().keep > Plant::CanaryGrass.spec().keep);
        assert!(Plant::ALL.iter().all(|p| p.aquatic() == (p.kind() == crate::resources::ResourceKind::WaterPlant)));
    }
}
