pub use crate::content::Plant;

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Deserialize)]
pub enum Habitat {
    Upland,
    Meadow,
    Shore,
    Water,
}

#[derive(Clone, Copy, Debug, PartialEq, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlantSpec {
    pub habitat: Habitat,
    pub food: f32,
    pub regrow: f32,
    pub creep: f32,
    pub seed: f32,
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

    pub fn aquatic(self) -> bool {
        self.spec().habitat == Habitat::Water
    }

    pub fn kind(self) -> crate::resources::ResourceKind {
        crate::resources::ResourceKind::food_plant(self.aquatic())
    }
}

pub fn flora(map: &str) -> &'static [f32] {
    let f = &crate::content::content().plant_flora;
    f.get(map).unwrap_or(&f["default"])
}

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
