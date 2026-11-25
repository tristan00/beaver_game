//! Food plants: the species each map's real wetlands grow that beavers eat, where each roots,
//! how fast it grows and spreads, and what of it lasts the winter.
//!
//! Every map draws its plants from its own region's flora (`plant_flora`): wild raspberry in
//! the clearings, Joe-Pye weed (North America) or meadowsweet (Europe) in the wet meadows,
//! sedge, horsetail and cattail along the waterline, and arrowhead, yellow pond lily, white
//! water lily and pondweed in the water. Each spreads two ways, as it does in life: by creeping
//! roots into the ground next to it, and now and then by seed further off. What it keeps over
//! winter is what beavers dig for then: cattail, arrowhead and lily rhizomes, raspberry canes.
pub use crate::content::Plant;

/// Where a species roots.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Deserialize)]
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
#[derive(Clone, Copy, Debug, PartialEq, serde::Deserialize)]
#[serde(deny_unknown_fields)]
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

impl Habitat {
    pub const ALL: [Habitat; 4] = [Habitat::Upland, Habitat::Meadow, Habitat::Shore, Habitat::Water];
}

impl Plant {
    pub fn spec(self) -> &'static PlantSpec {
        &self.row().spec
    }

    /// Stands in open water: drawn floating or rising from it, and gathered by swimmers.
    pub fn aquatic(self) -> bool {
        self.spec().habitat == Habitat::Water
    }

    pub fn kind(self) -> crate::resources::ResourceKind {
        crate::resources::ResourceKind::food_plant(self.aquatic())
    }
}

/// A map's flora: how common each species is where it can grow.
pub fn flora(map: &str) -> &'static [f32] {
    let f = &crate::content::content().plant_flora;
    f.get(map).unwrap_or(&f["default"])
}

/// A species from the flora for ground of this habitat (`fits` says which species it suits),
/// `draw` in 0..1 choosing among them by their weights.
pub fn choose(map: &str, draw: f32, fits: impl Fn(Plant) -> bool) -> Option<Plant> {
    let mix = flora(map);
    let total: f32 = Plant::all().filter(|p| fits(*p)).map(|p| mix[p.index()]).sum();
    if total <= 0.0 {
        return None;
    }
    let mut left = draw.clamp(0.0, 0.9999) * total;
    for p in Plant::all().filter(|p| fits(*p)) {
        left -= mix[p.index()];
        if left < 0.0 && mix[p.index()] > 0.0 {
            return Some(p);
        }
    }
    Plant::all().rev().find(|p| fits(*p) && mix[p.index()] > 0.0)
}
