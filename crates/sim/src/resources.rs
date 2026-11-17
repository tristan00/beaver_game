//! Gatherable resource entities (owner: `resources`): individual trees, felled logs and branches,
//! bark, plants, mud banks and stone outcrops. Pure data with queries; agents act through `Task`s.

use crate::buildings::Good;
use crate::water::Cell;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ResourceId(pub u64);

/// What a map resource is; agents are assigned by kind.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ResourceKind {
    /// Standing tree: solid, fells into `Felled`.
    Tree,
    /// Felled tree: yields logs, branches and bark.
    Felled,
    /// Edible plant on wet ground; regrows.
    Plant,
    MudBank,
    StoneOutcrop,
}

impl ResourceKind {
    /// Goods a load of this kind can yield (`Tree` yields nothing until felled).
    pub fn yields(self) -> &'static [Good] {
        match self {
            ResourceKind::Tree => &[],
            ResourceKind::Felled => &[Good::Logs, Good::Sticks],
            ResourceKind::Plant => &[Good::Food],
            ResourceKind::MudBank => &[Good::Mud],
            ResourceKind::StoneOutcrop => &[Good::Stone],
        }
    }
    /// True if the entity blocks navigation.
    pub fn is_solid(self) -> bool {
        matches!(self, ResourceKind::Tree | ResourceKind::StoneOutcrop)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Resource {
    pub id: ResourceId,
    pub kind: ResourceKind,
    pub cell: Cell,
    /// Units left to gather (for `Tree`, work days needed to fell).
    pub amount: f32,
    /// Plants regrow up to this amount.
    pub max_amount: f32,
}

/// All resource entities.
#[derive(Clone, Debug, Default)]
pub struct ResourceWorld {
    pub items: Vec<Resource>,
    next_id: u64,
}

impl ResourceWorld {
    /// Deterministic placement for a seed (trees from forest density, plants on wet ground, mud on
    /// banks, stone at outcrops). (`resources` task.)
    pub fn seeded(_seed: u64, _world: &crate::worldgen::World) -> Self {
        Self::default()
    }
    pub fn get(&self, id: ResourceId) -> Option<&Resource> {
        self.items.iter().find(|r| r.id == id)
    }
    /// Nearest resource of `kind` to `from` (optionally inside `area`) with something left.
    pub fn nearest(&self, _kind: ResourceKind, _from: Cell, _area: Option<&crate::work::Area>) -> Option<ResourceId> {
        None
    }
    /// Takes up to `want` from a resource; returns what was taken (felling converts a tree to `Felled`).
    pub fn take(&mut self, _id: ResourceId, _want: f32) -> Option<(Good, f32)> {
        None
    }
    /// Plant regrowth and depletion cleanup.
    pub fn step(&mut self, _dt_days: f32) {}
    pub fn add(&mut self, kind: ResourceKind, cell: Cell, amount: f32) -> ResourceId {
        self.next_id += 1;
        let id = ResourceId(self.next_id);
        self.items.push(Resource { id, kind, cell, amount, max_amount: amount });
        id
    }
}
