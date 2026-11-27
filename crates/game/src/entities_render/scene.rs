use bevy::prelude::*;
use sim::SimState;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TreeStyle {
    Slim,
    Twin,
    Weeping,
    Clump,
    Dome,
    Spread,
    Tufted,
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
    Work,
    Dig,
    Forage,
    Rest,
    Run,
    Handle,
    Mend,
    Watch,
    Court,
    Groom,
    Caught,
    Dead,
}

pub fn lodge_view(sim: &SimState) -> (Vec3, f32) {
    let cell = sim.buildings.buildings.iter()
        .find(|b| b.kind.is_home())
        .map(|b| b.pos)
        .unwrap_or((sim.world.size / 2, sim.world.size / 2));
    let xz = super::sim_view::cell_xz(sim, (cell.0 as f32 + 0.5, cell.1 as f32 + 0.5));
    (Vec3::new(xz.x, crate::flow::height_at_world(&sim.height, xz.x, xz.y), xz.y), 0.8)
}
