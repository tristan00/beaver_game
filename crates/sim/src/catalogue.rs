//! Building catalogue for the new model (owner: `content`): worker slots, work area, unlock and
//! tier per building, on top of `BuildingKind::spec` (cost, footprint, site rule). New industrial
//! content (canal networks, efficient workshops, big stores) is added as `BuildingKind` variants by
//! `content`; `def` maps every kind.

use crate::buildings::BuildingKind;
use crate::features::System;
use crate::work::Role;

/// Era a building belongs to; start 2 uses `Elementary`, start 3 adds `Industrial`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Tier {
    Elementary,
    Industrial,
}

#[derive(Clone, Debug)]
pub struct Def {
    pub kind: BuildingKind,
    pub tier: Tier,
    /// One entry per worker slot.
    pub slots: Vec<Role>,
    /// Default work-area size (w, h) in cells around the building; `None` if workers do not gather.
    pub area: Option<(usize, usize)>,
    /// Tech index (into `tech::TECHS`) needed, or `None` if always available.
    pub unlock: Option<usize>,
    /// Belongs to a switchable system (guard post, trade dock): hidden when it is disabled.
    pub system: Option<System>,
}

/// Definition for `kind`. (`content` fills the table; the stub gives one build slot.)
pub fn def(kind: BuildingKind) -> Def {
    Def { kind, tier: Tier::Elementary, slots: vec![Role::Build], area: None, unlock: None, system: System::of_building(kind) }
}

/// Kinds the player may see and place given features (and, in `content`, tech).
pub fn available(sim: &crate::SimState) -> Vec<BuildingKind> {
    BuildingKind::ALL
        .iter()
        .copied()
        .filter(|k| def(*k).system.is_none_or(|s| sim.features.enabled(s)))
        .collect()
}
