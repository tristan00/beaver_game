//! Map gestures as pure logic: right-click and drag orders for selected beavers, and moving or
//! resizing a workplace's work area. Everything works on cells and `SimState`, so it is tested
//! without a window; `map_orders.rs` feeds it the pointer.

use sim::agents::Assignment;
use sim::buildings::BuildingId;
use sim::colony::BeaverId;
use sim::resources::ResourceKind;
use sim::water::Cell;
use sim::work::{Area, Role};
use sim::{Command, SimState};

use super::orders::Gesture;

/// Whether two resource kinds are the same thing to a collector (a felled tree is still a tree).
fn same_kind(a: ResourceKind, b: ResourceKind) -> bool {
    let tree = |k| matches!(k, ResourceKind::Tree | ResourceKind::Felled);
    a == b || (tree(a) && tree(b))
}

/// Normalises a rectangle so `min <= max`.
pub fn area_between(a: Cell, b: Cell) -> Area {
    Area { min: (a.0.min(b.0), a.1.min(b.1)), max: (a.0.max(b.0), a.1.max(b.1)) }
}

/// The commands a gesture produces for the selected beavers, in order; empty if nothing applies.
/// A right-click on a site fills one free builder slot per selected beaver.
pub fn gesture_commands(sim: &SimState, selected: &[BeaverId], g: &Gesture) -> Vec<Command> {
    match g {
        Gesture::RightClickResource { kind, cell } => {
            let found = sim.resources.at(*cell).find(|r| same_kind(r.kind, *kind) && r.has_yield());
            match found {
                Some(r) if !selected.is_empty() => {
                    vec![Command::Assign { who: selected.to_vec(), to: Some(Assignment::Resource(r.id)) }]
                }
                _ => Vec::new(),
            }
        }
        Gesture::RightClickSite { building } => {
            let Some(place) = sim.work.place(*building).filter(|p| p.site.is_some()) else { return Vec::new() };
            let free = place.slots.iter().enumerate().filter(|(_, s)| s.worker.is_none() && s.role == Role::Build).map(|(i, _)| i);
            let already = |b: &BeaverId| place.slots.iter().any(|s| s.worker == Some(*b));
            selected
                .iter()
                .filter(|b| !already(b))
                .zip(free)
                .map(|(&who, slot)| Command::FillSlot { building: *building, slot, who })
                .collect()
        }
        Gesture::DragArea { kind, area } => {
            if selected.is_empty() {
                return Vec::new();
            }
            vec![Command::Assign { who: selected.to_vec(), to: Some(Assignment::Area(*kind, area_between(area.min, area.max))) }]
        }
        Gesture::EditWorkArea { building, area } => {
            if sim.work.place(*building).is_some_and(|p| p.area.is_some()) {
                vec![Command::SetWorkArea { building: *building, area: area_between(area.min, area.max) }]
            } else {
                Vec::new()
            }
        }
        Gesture::Answer { id, accept } => {
            let answer = if *accept { sim::society::Answer::Accept } else { sim::society::Answer::TurnAway };
            vec![Command::AnswerJoin { id: *id, answer }]
        }
    }
}

/// Which part of a work area the pointer grabbed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Handle {
    /// Inside the rectangle: the whole area moves.
    Move,
    /// On the border: the flagged sides follow the pointer (two flags on a corner).
    Edge { west: bool, east: bool, north: bool, south: bool },
}

/// The work area under `cell` and the handle it offers: the border resizes, the inside moves.
/// Of overlapping areas the smallest wins.
pub fn work_area_hit(sim: &SimState, cell: Cell) -> Option<(BuildingId, Handle)> {
    let (b, a) = sim
        .work
        .places
        .iter()
        .filter_map(|p| p.area.map(|a| (p.building, a)))
        .filter(|(_, a)| a.contains(cell))
        .min_by_key(|(b, a)| ((a.max.0 - a.min.0 + 1) * (a.max.1 - a.min.1 + 1), *b))?;
    let (west, east, north, south) = (cell.0 == a.min.0, cell.0 == a.max.0, cell.1 == a.min.1, cell.1 == a.max.1);
    let handle = if west || east || north || south { Handle::Edge { west, east, north, south } } else { Handle::Move };
    Some((b, handle))
}

/// The rectangle after dragging `handle` of `orig` from `start` to `now`, kept on a map of
/// `size` cells and at least one cell wide.
pub fn drag_work_area(orig: Area, handle: Handle, start: Cell, now: Cell, size: usize) -> Area {
    let last = size.saturating_sub(1) as i64;
    let (dx, dy) = (now.0 as i64 - start.0 as i64, now.1 as i64 - start.1 as i64);
    let (mut x0, mut x1, mut y0, mut y1) = (orig.min.0 as i64, orig.max.0 as i64, orig.min.1 as i64, orig.max.1 as i64);
    match handle {
        Handle::Move => {
            let dx = dx.clamp(-x0, last - x1);
            let dy = dy.clamp(-y0, last - y1);
            (x0, x1, y0, y1) = (x0 + dx, x1 + dx, y0 + dy, y1 + dy);
        }
        Handle::Edge { west, east, north, south } => {
            if west {
                x0 = (x0 + dx).clamp(0, x1);
            }
            if east {
                x1 = (x1 + dx).clamp(x0, last);
            }
            if north {
                y0 = (y0 + dy).clamp(0, y1);
            }
            if south {
                y1 = (y1 + dy).clamp(y0, last);
            }
        }
    }
    Area { min: (x0 as usize, y0 as usize), max: (x1 as usize, y1 as usize) }
}

/// The kind with most gatherable resources inside `area` (trees, plants, mud, stone).
pub fn dominant_kind(sim: &SimState, area: &Area) -> Option<ResourceKind> {
    const KINDS: [ResourceKind; 4] = [ResourceKind::Tree, ResourceKind::Plant, ResourceKind::MudBank, ResourceKind::StoneOutcrop];
    KINDS
        .iter()
        .map(|&k| (sim.resources.items.iter().filter(|r| same_kind(r.kind, k) && r.has_yield() && area.contains(r.cell)).count(), k))
        .filter(|(n, _)| *n > 0)
        .max_by_key(|(n, _)| *n)
        .map(|(_, k)| k)
}

/// The construction site whose footprint covers `cell`.
pub fn site_at(sim: &SimState, cell: Cell) -> Option<BuildingId> {
    sim.buildings
        .buildings
        .iter()
        .find(|b| b.cells().any(|c| c == cell) && sim.work.place(b.id).is_some_and(|p| p.site.is_some()))
        .map(|b| b.id)
}

#[derive(Clone, Debug, PartialEq)]
enum Drag {
    /// Left-drag over free ground with beavers selected.
    Collect { start: Cell, now: Cell },
    /// Left-drag on a work area.
    WorkArea { building: BuildingId, handle: Handle, orig: Area, start: Cell, now: Cell },
}

/// What the map shows while a drag is under way.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DragPreview {
    pub area: Area,
    /// `Some` while editing that building's work area; `None` for a collect rectangle.
    pub editing: Option<BuildingId>,
}

/// Left/right pointer gestures on the map as a state machine over cells. The caller supplies the
/// cell under the pointer; a right press that moved too far (an orbit) never reaches `right_click`.
#[derive(Clone, Debug, Default)]
pub struct MapPointer {
    drag: Option<Drag>,
}

impl MapPointer {
    /// Left button pressed on `cell`. Returns true when the press starts an order drag (the
    /// camera must not orbit): a work area is grabbed, or beavers are selected.
    pub fn left_down(&mut self, sim: &SimState, has_selection: bool, cell: Cell) -> bool {
        self.drag = if let Some((building, handle)) = work_area_hit(sim, cell) {
            sim.work.place(building).and_then(|p| p.area).map(|orig| Drag::WorkArea { building, handle, orig, start: cell, now: cell })
        } else if has_selection {
            Some(Drag::Collect { start: cell, now: cell })
        } else {
            None
        };
        self.drag.is_some()
    }

    pub fn moved(&mut self, cell: Cell) {
        match &mut self.drag {
            Some(Drag::Collect { now, .. }) | Some(Drag::WorkArea { now, .. }) => *now = cell,
            None => {}
        }
    }

    pub fn dragging(&self) -> bool {
        self.drag.is_some()
    }

    /// The rectangle to draw for the drag in progress.
    pub fn preview(&self, sim: &SimState) -> Option<DragPreview> {
        match self.drag.as_ref()? {
            Drag::Collect { start, now } => Some(DragPreview { area: area_between(*start, *now), editing: None }),
            Drag::WorkArea { building, handle, orig, start, now } => {
                Some(DragPreview { area: drag_work_area(*orig, *handle, *start, *now, sim.world.size), editing: Some(*building) })
            }
        }
    }

    pub fn cancel(&mut self) {
        self.drag = None;
    }

    /// Left button released on `cell`: the commands the drag issues. A press and release on one
    /// cell is a plain click and issues none.
    pub fn left_up(&mut self, sim: &SimState, selected: &[BeaverId], cell: Cell) -> Vec<Command> {
        self.moved(cell);
        let Some(drag) = self.drag.take() else { return Vec::new() };
        match drag {
            Drag::Collect { start, now } if start != now => {
                let area = area_between(start, now);
                match dominant_kind(sim, &area) {
                    Some(kind) => gesture_commands(sim, selected, &Gesture::DragArea { kind, area }),
                    None => Vec::new(),
                }
            }
            Drag::WorkArea { building, handle, orig, start, now } => {
                let area = drag_work_area(orig, handle, start, now, sim.world.size);
                if area == orig {
                    Vec::new()
                } else {
                    gesture_commands(sim, selected, &Gesture::EditWorkArea { building, area })
                }
            }
            Drag::Collect { .. } => Vec::new(),
        }
    }

    /// Right button clicked (not dragged) on `cell` with `selected` beavers: a site takes
    /// builders, a resource takes collectors.
    pub fn right_click(&self, sim: &SimState, selected: &[BeaverId], cell: Cell) -> Vec<Command> {
        if let Some(building) = site_at(sim, cell) {
            return gesture_commands(sim, selected, &Gesture::RightClickSite { building });
        }
        match sim.resources.at(cell).find(|r| r.has_yield()).map(|r| r.kind) {
            Some(kind) => gesture_commands(sim, selected, &Gesture::RightClickResource { kind, cell }),
            None => Vec::new(),
        }
    }
}

#[cfg(test)]
#[path = "gestures_tests.rs"]
mod tests;
