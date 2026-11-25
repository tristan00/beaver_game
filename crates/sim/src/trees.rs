//! Tree species: how fast they grow, what they yield, and where they grow.
//! Stats live in `content/trees.toml` and each map's species mix in `content/flora.toml`.
pub use crate::content::Tree;

/// Multipliers against the reference tree in the forestry and resource constants.
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TreeSpec {
    pub growth: f32,
    pub logs: f32,
    pub branches: f32,
    /// Bark is beaver food, stripped from felled trees; conifers give none.
    pub bark: f32,
    pub fell_work: f32,
    /// Scales how long the tree survives standing in floodwater.
    pub flood: f32,
    /// Metres of standing water the tree lives in indefinitely; deeper water drowns it.
    pub max_depth: f32,
}

/// One species' share of a map's forest on dry ground, on wet ground, extra on
/// steep or high ground, and extra where the land cover is not closed forest.
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Mix {
    pub dry: f32,
    pub wet: f32,
    pub upland: f32,
    pub open: f32,
}

impl Tree {
    pub fn spec(self) -> &'static TreeSpec {
        &self.row().spec
    }
    /// The species taken for a tree of no known species.
    pub fn unknown() -> Tree {
        crate::content::content().unknown
    }
    pub fn feeds(self) -> bool {
        self.spec().bark > 0.0
    }
}

/// Where a tree stands, as the world generator sees it.
#[derive(Clone, Copy, Debug, serde::Serialize, serde::Deserialize)]
pub struct Habitat {
    pub moisture: f32,
    pub slope: f32,
    /// 0 at the lowest land on the map, 1 at the highest.
    pub height: f32,
    /// ESA WorldCover class (10 tree cover, 20 shrubland, 90 wetland, ...).
    pub cover: u8,
}

/// The species mix for a map, or the generic valley mix.
pub fn flora(map: &str) -> &'static [Mix] {
    let f = &crate::content::content().tree_flora;
    f.get(map).unwrap_or(&f["default"])
}

/// Habitat shifts the odds rather than drawing single-species bands, so even
/// wet banks and dry ridges hold several species.
pub fn choose(mix: &[Mix], h: Habitat, draw: f32) -> Tree {
    let steep = h.slope.clamp(0.0, 1.0);
    let high = ((h.height - 0.55) / 0.4).clamp(0.0, 1.0);
    let upland = steep.max(high * high * (3.0 - 2.0 * high));
    let wet = ((h.moisture - 0.15) / 0.70).clamp(0.0, 1.0);
    let mut wet = wet * wet * (3.0 - 2.0 * wet) * (1.0 - 0.65 * steep);
    if h.cover == 90 {
        wet = wet.max(0.85);
    }
    let open = if h.cover == 10 { 0.0 } else { 1.0 };
    let weights: Vec<f32> = mix.iter().map(|m| (m.dry + (m.wet - m.dry) * wet + m.upland * upland + m.open * open).max(0.0)).collect();
    let mut pick = draw.clamp(0.0, 1.0) * weights.iter().sum::<f32>();
    for (tree, weight) in Tree::all().zip(weights.iter().copied()) {
        if weight > 0.0 && pick < weight {
            return tree;
        }
        pick -= weight;
    }
    Tree::all().zip(weights).rev().find(|(_, w)| *w > 0.0).map_or(Tree::all().last().expect("content has trees"), |(t, _)| t)
}
