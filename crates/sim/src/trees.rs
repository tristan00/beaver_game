pub use crate::content::Tree;

#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TreeSpec {
    pub growth: f32,
    pub logs: f32,
    pub branches: f32,
    pub bark: f32,
    pub fell_work: f32,
    pub flood: f32,
    pub max_depth: f32,
}

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
}

#[derive(Clone, Copy, Debug, serde::Serialize, serde::Deserialize)]
pub struct Habitat {
    pub moisture: f32,
    pub slope: f32,
    pub height: f32,
    pub cover: u8,
}

pub fn flora(map: &str) -> &'static [Mix] {
    let f = &crate::content::content().tree_flora;
    f.get(map).unwrap_or(&f["default"])
}

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
