//! Tree species: how fast they grow, what they yield, and where they grow.
//! Stats and each map's species mix live in `content/game.sqlite3`
//! (`tree_species`, `tree_flora`).
include!(concat!(env!("OUT_DIR"), "/trees_constants.rs"));

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Tree { Aspen, Willow, Birch, Alder, Pine, Maple, Spruce, Beech }

/// Multipliers against the reference tree in the forestry and resource constants.
#[derive(Clone, Copy, Debug, PartialEq)]
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
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Mix {
    pub dry: f32,
    pub wet: f32,
    pub upland: f32,
    pub open: f32,
}

impl Tree {
    pub const ALL: [Tree; 8] = [Tree::Aspen, Tree::Willow, Tree::Birch, Tree::Alder, Tree::Pine, Tree::Maple, Tree::Spruce, Tree::Beech];

    pub fn spec(self) -> &'static TreeSpec {
        &SPECIES[self as usize]
    }
    pub fn name(self) -> &'static str {
        match self {
            Tree::Aspen => "Aspen",
            Tree::Willow => "Willow",
            Tree::Birch => "Birch",
            Tree::Alder => "Alder",
            Tree::Pine => "Pine",
            Tree::Maple => "Maple",
            Tree::Spruce => "Spruce",
            Tree::Beech => "Beech",
        }
    }
    pub fn feeds(self) -> bool {
        self.spec().bark > 0.0
    }
}

/// Where a tree stands, as the world generator sees it.
#[derive(Clone, Copy, Debug)]
pub struct Habitat {
    pub moisture: f32,
    pub slope: f32,
    /// 0 at the lowest land on the map, 1 at the highest.
    pub height: f32,
    /// ESA WorldCover class (10 tree cover, 20 shrubland, 90 wetland, ...).
    pub cover: u8,
}

/// The species mix for a map, or the generic valley mix.
pub fn flora(map: &str) -> &'static [Mix; 8] {
    let find = |m: &str| FLORA.iter().find(|f| f.0 == m).map(|f| &f.1);
    find(map).or_else(|| find("default")).expect("content defines a default flora")
}

/// Habitat shifts the odds rather than drawing single-species bands, so even
/// wet banks and dry ridges hold several species.
pub fn choose(mix: &[Mix; 8], h: Habitat, draw: f32) -> Tree {
    let steep = h.slope.clamp(0.0, 1.0);
    let high = ((h.height - 0.55) / 0.4).clamp(0.0, 1.0);
    let upland = steep.max(high * high * (3.0 - 2.0 * high));
    let wet = ((h.moisture - 0.15) / 0.70).clamp(0.0, 1.0);
    let mut wet = wet * wet * (3.0 - 2.0 * wet) * (1.0 - 0.65 * steep);
    if h.cover == 90 {
        wet = wet.max(0.85);
    }
    let open = if h.cover == 10 { 0.0 } else { 1.0 };
    let weights: [f32; 8] = std::array::from_fn(|i| {
        let m = mix[i];
        (m.dry + (m.wet - m.dry) * wet + m.upland * upland + m.open * open).max(0.0)
    });
    let mut pick = draw.clamp(0.0, 1.0) * weights.iter().sum::<f32>();
    for (tree, weight) in Tree::ALL.into_iter().zip(weights) {
        if weight > 0.0 && pick < weight {
            return tree;
        }
        pick -= weight;
    }
    Tree::ALL.into_iter().zip(weights).rev().find(|(_, w)| *w > 0.0).map_or(Tree::Pine, |(t, _)| t)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn share(map: &str, h: Habitat, pred: impl Fn(Tree) -> bool) -> f32 {
        let n = 2000;
        (0..n).filter(|i| pred(choose(flora(map), h, (*i as f32 + 0.5) / n as f32))).count() as f32 / n as f32
    }

    #[test]
    fn each_map_grows_its_own_forest() {
        let dry = Habitat { moisture: 0.2, slope: 0.05, height: 0.3, cover: 10 };
        let conifer = |t| matches!(t, Tree::Pine | Tree::Spruce);
        assert!(share("bavarian", dry, |t| t == Tree::Beech) > 0.3);
        assert_eq!(share("telemark", dry, |t| t == Tree::Beech), 0.0);
        assert!(share("voyageurs", dry, |t| t == Tree::Aspen) > share("bavarian", dry, |t| t == Tree::Aspen) * 5.0);
        assert!(share("telemark", dry, conifer) > share("algonquin", dry, conifer));
        assert_eq!(flora("nowhere"), flora("default"));
    }

    #[test]
    fn wet_banks_favour_willow_and_alder_and_ridges_conifers() {
        let bank = Habitat { moisture: 0.95, slope: 0.02, height: 0.1, cover: 90 };
        let ridge = Habitat { moisture: 0.2, slope: 0.6, height: 0.95, cover: 10 };
        for map in ["algonquin", "voyageurs", "bavarian", "telemark"] {
            let riparian = |t| matches!(t, Tree::Willow | Tree::Alder);
            let conifer = |t| matches!(t, Tree::Pine | Tree::Spruce);
            assert!(share(map, bank, riparian) > share(map, ridge, riparian) + 0.2, "{map}");
            assert!(share(map, ridge, conifer) > share(map, bank, conifer), "{map}");
        }
    }

    #[test]
    fn broadleaves_feed_beavers_and_conifers_do_not() {
        assert!(Tree::Aspen.feeds() && Tree::Willow.feeds() && Tree::Birch.feeds());
        assert!(!Tree::Pine.feeds() && !Tree::Spruce.feeds());
        assert!(Tree::Willow.spec().growth > Tree::Aspen.spec().growth && Tree::Aspen.spec().growth > Tree::Spruce.spec().growth);
        assert!(Tree::Aspen.spec().bark > Tree::Birch.spec().bark);
    }
}
