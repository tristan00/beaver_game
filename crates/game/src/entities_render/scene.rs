use bevy::prelude::*;
use sim::SimState;

pub use crate::flow::height_at_world as height_at;

/// The ways a tree is drawn; each species' row names one (`look.model`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TreeStyle {
    /// A slim pale trunk with dark bark eyes under a small round crown.
    Slim,
    /// Two white stems banded black.
    Twin,
    /// A leaning trunk under a weeping curtain.
    Weeping,
    /// A shrubby clump of stems that hangs cones in winter.
    Clump,
    /// A broad dome that flowers red before its leaves.
    Dome,
    /// A low wide spread.
    Spread,
    /// A tall bare trunk under flat tufts of needles.
    Tufted,
    /// A narrow spire of drooping boughs.
    Spire,
}

impl TreeStyle {
    pub fn named(model: &str) -> Option<TreeStyle> {
        use TreeStyle::*;
        [("slim", Slim), ("twin", Twin), ("weeping", Weeping), ("clump", Clump), ("dome", Dome), ("spread", Spread), ("tufted", Tufted), ("spire", Spire)]
            .into_iter().find(|(n, _)| *n == model).map(|s| s.1)
    }
    pub fn of(t: sim::trees::Tree) -> TreeStyle {
        Self::named(&t.row().look.model).unwrap_or_else(|| panic!("tree {}: no tree model {}", t.key(), t.row().look.model))
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Pose {
    Idle,
    Walk,
    Swim,
    Gnaw,
    Carry,
    Climb,
    /// Building: pressing mud into place.
    Work,
    /// Scooping mud or mud out of a bank.
    Dig,
    /// Sitting up, eating a plant.
    Forage,
    Rest,
    /// Running from a wolf: a flat-out bound, head low, tail up.
    Run,
    /// Taking up or putting down a load at a store, pile or site.
    Handle,
    /// Mending: patting fresh mud on, tamping it with the tail.
    Mend,
    /// A sentry on watch: sitting up tall, looking about.
    Watch,
    /// A new pair meeting at their lodge: nose to nose.
    Court,
    /// Grooming a family member beside it.
    Groom,
    /// Pinned under the wolf, struggling.
    Caught,
    /// Dead: fallen on its side, or curled up.
    Dead,
}

pub fn lodge_view(sim: &SimState) -> (Vec3, f32) {
    let cell = sim.buildings.buildings.iter()
        .find(|b| b.kind.is_home())
        .map(|b| b.pos)
        .unwrap_or((sim.world.size / 2, sim.world.size / 2));
    let xz = super::sim_view::cell_xz(sim, (cell.0 as f32 + 0.5, cell.1 as f32 + 0.5));
    (Vec3::new(xz.x, height_at(&sim.height, xz.x, xz.y), xz.y), 0.8)
}
