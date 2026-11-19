use std::cell::RefCell;
use std::cmp::Reverse;
use std::collections::BinaryHeap;

use crate::buildings::BuildingKind;
use crate::colony::BeaverId;
use crate::grid::Grid;
use crate::water::Cell;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Surface {
    Land,
    Water,
    Canal,
    Dam,
    Solid,
    Thicket,
    LodgeEntrance,
    LodgeInside,
}

pub fn shared(s: Surface) -> bool {
    matches!(s, Surface::LodgeInside | Surface::LodgeEntrance)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Gait {
    Walk,
    Swim,
    Climb,
}

pub const CLIMB_COST: f32 = 3.0;
pub const THICKET_COST: f32 = 2.5;
pub const WALK_COST: f32 = 1.0;
pub const SWIM_COST: f32 = 0.6;
pub const CANAL_COST: f32 = 0.3;
const COST_UNIT: f32 = 1000.0;
const MIN_STEP: u32 = (CANAL_COST * COST_UNIT) as u32;

pub fn lodge_entrance(pos: Cell) -> Cell {
    (pos.0 + 1, pos.1 + 3)
}

#[derive(Clone, Debug, PartialEq)]
pub struct Path {
    pub steps: Vec<(Cell, Gait)>,
    pub cost: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Goal {
    Cell(Cell),
    Beside(Cell),
    Inside(Cell),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NavError {
    NotHolder(Cell),
    NotAdjacent,
    OutOfBounds,
    Forbidden(Surface),
    Occupied(Cell),
}

#[derive(Clone, Debug)]
pub struct NavGrid {
    pub surface: Grid<Surface>,
    pub reserved: Grid<Option<BeaverId>>,
    pub lodge: Grid<u32>,
    region: Vec<u32>,
    regions_ok: bool,
}

#[derive(Default)]
struct Scratch {
    stamp: u32,
    seen: Vec<u32>,
    g: Vec<u32>,
    parent: Vec<u32>,
    heap: BinaryHeap<Reverse<u64>>,
}

impl Scratch {
    fn begin(&mut self, n: usize) {
        if self.seen.len() != n {
            self.seen = vec![0; n];
            self.g = vec![0; n];
            self.parent = vec![0; n];
            self.stamp = 0;
        }
        self.stamp = self.stamp.wrapping_add(1);
        if self.stamp == 0 {
            self.seen.fill(0);
            self.stamp = 1;
        }
        self.heap.clear();
    }
    fn set(&mut self, i: usize, g: u32, parent: u32) {
        self.seen[i] = self.stamp;
        self.g[i] = g;
        self.parent[i] = parent;
    }
    fn best(&self, i: usize) -> Option<u32> {
        (self.seen[i] == self.stamp).then_some(self.g[i])
    }
    fn build(&self, goal: usize, start: usize, w: usize, surf: &[Surface]) -> Path {
        let mut steps = Vec::new();
        let mut i = goal;
        while i != start {
            let gait = NavGrid::step_cost(surf[i]).map_or(Gait::Walk, |(_, g)| g);
            steps.push(((i % w, i / w), gait));
            i = self.parent[i] as usize;
        }
        steps.reverse();
        Path { steps, cost: self.g[goal] as f32 / COST_UNIT }
    }
}

fn pack(f: u32, i: usize) -> u64 {
    (f as u64) << 32 | i as u64
}

thread_local! {
    static SCRATCH: RefCell<Scratch> = RefCell::new(Scratch::default());
}

impl NavGrid {
    pub fn new(width: usize, height: usize) -> Self {
        Self {
            surface: Grid::new(width, height, Surface::Land),
            reserved: Grid::new(width, height, None),
            lodge: Grid::new(width, height, 0),
            region: Vec::new(),
            regions_ok: false,
        }
    }

    pub fn label_regions(&mut self) {
        let (w, h) = (self.surface.width(), self.surface.height());
        let surf = self.surface.as_slice();
        let mut region = vec![0u32; w * h];
        let mut next = 0u32;
        let mut stack = Vec::new();
        for start in 0..w * h {
            if region[start] != 0 || surf[start] == Surface::Solid {
                continue;
            }
            next += 1;
            region[start] = next;
            stack.push(start);
            while let Some(i) = stack.pop() {
                let (x, y) = (i % w, i / w);
                let nbrs = [(y > 0).then(|| i - w), (x > 0).then(|| i - 1), (x + 1 < w).then(|| i + 1), (y + 1 < h).then(|| i + w)];
                for j in nbrs.into_iter().flatten() {
                    if region[j] == 0 && self.can_step_surf(surf[i], surf[j], (x, y), (j % w, j / w)) {
                        region[j] = next;
                        stack.push(j);
                    }
                }
            }
        }
        self.region = region;
        self.regions_ok = true;
    }

    fn region_of(&self, c: Cell) -> u32 {
        self.region[c.1 * self.surface.width() + c.0]
    }

    fn start_regions(&self, from: Cell) -> [u32; 4] {
        let mut out = [0; 4];
        if self.region_of(from) != 0 {
            out[0] = self.region_of(from);
            return out;
        }
        for (k, n) in self.neighbours(from).into_iter().enumerate() {
            if let Some(n) = n.filter(|&n| self.can_step(from, n)) {
                out[k] = self.region_of(n);
            }
        }
        out
    }

    fn neighbours(&self, c: Cell) -> [Option<Cell>; 4] {
        let (w, h) = (self.surface.width(), self.surface.height());
        [
            (c.1 > 0).then(|| (c.0, c.1 - 1)),
            (c.0 > 0).then(|| (c.0 - 1, c.1)),
            (c.0 + 1 < w).then(|| (c.0 + 1, c.1)),
            (c.1 + 1 < h).then(|| (c.0, c.1 + 1)),
        ]
    }

    pub fn may_reach(&self, from: Cell, ends: impl IntoIterator<Item = Cell>) -> bool {
        if !self.regions_ok || self.surface_at(from).is_none() {
            return true;
        }
        let starts = self.start_regions(from);
        ends.into_iter().any(|e| self.surface_at(e).is_some() && self.region_of(e) != 0 && starts.contains(&self.region_of(e)))
    }

    pub fn may_reach_beside(&self, from: Cell, target: Cell) -> bool {
        self.may_reach(from, self.neighbours(target).into_iter().flatten())
    }

    pub fn rebuild(&mut self, sim: &crate::SimState) {
        let w = self.surface.width();
        self.lodge.as_mut_slice().fill(0);
        let depth = &sim.water.depth;
        for (i, slot) in self.surface.as_mut_slice().iter_mut().enumerate() {
            let (x, y) = (i % w, i / w);
            let wet = depth.get(x, y).is_some_and(|&d| d > crate::buildings::WET_DEPTH);
            *slot = if sim.water.has_dam(x, y) {
                Surface::Dam
            } else if wet && sim.water.canal.get(x, y) == Some(&true) {
                Surface::Canal
            } else if wet {
                Surface::Water
            } else {
                Surface::Land
            };
        }
        for b in &sim.buildings.buildings {
            match b.kind {
                BuildingKind::Canal if b.is_complete() => {
                    for c in b.cells() {
                        if self.surface.get(c.0, c.1) == Some(&Surface::Water) {
                            self.set(c, Surface::Canal);
                        }
                    }
                }
                BuildingKind::Dam | BuildingKind::Canal | BuildingKind::Lock | BuildingKind::Dock | BuildingKind::Barge => {}
                BuildingKind::Lodge if b.is_complete() => {
                    self.add_lodge(b.pos);
                }
                _ if !b.is_complete() => {}
                _ => b.cells().for_each(|c| self.set(c, Surface::Solid)),
            }
        }
        for r in sim.resources.items.iter().filter(|r| r.is_solid()) {
            let s = if r.kind == crate::resources::ResourceKind::Tree { Surface::Thicket } else { Surface::Solid };
            if self.surface.get(r.cell.0, r.cell.1) == Some(&Surface::Land) {
                self.set(r.cell, s);
            }
        }
        self.label_regions();
    }

    pub fn set(&mut self, cell: Cell, s: Surface) {
        if let Some(slot) = self.surface.get_mut(cell.0, cell.1) {
            let open = |s: Surface| !matches!(s, Surface::Solid | Surface::LodgeInside | Surface::LodgeEntrance);
            if !(open(*slot) && open(s)) {
                self.regions_ok = false;
            }
            *slot = s;
        }
    }

    pub fn add_lodge(&mut self, pos: Cell) {
        let id = (pos.1 * self.surface.width() + pos.0 + 1) as u32;
        for dy in 0..3 {
            for dx in 0..3 {
                self.set_lodge_cell((pos.0 + dx, pos.1 + dy), Surface::LodgeInside, id);
            }
        }
        self.set_lodge_cell(lodge_entrance(pos), Surface::LodgeEntrance, id);
    }

    pub fn set_lodge_cell(&mut self, cell: Cell, s: Surface, id: u32) {
        debug_assert!(id != 0 && matches!(s, Surface::LodgeInside | Surface::LodgeEntrance));
        self.set(cell, s);
        if let Some(slot) = self.lodge.get_mut(cell.0, cell.1) {
            *slot = id;
        }
    }

    pub fn step_cost(to: Surface) -> Option<(f32, Gait)> {
        match to {
            Surface::Land | Surface::LodgeInside => Some((WALK_COST, Gait::Walk)),
            Surface::Water | Surface::LodgeEntrance => Some((SWIM_COST, Gait::Swim)),
            Surface::Canal => Some((CANAL_COST, Gait::Swim)),
            Surface::Dam => Some((CLIMB_COST, Gait::Climb)),
            Surface::Thicket => Some((THICKET_COST, Gait::Walk)),
            Surface::Solid => None,
        }
    }

    pub fn can_step(&self, from: Cell, to: Cell) -> bool {
        let (Some(a), Some(b)) = (self.surface_at(from), self.surface_at(to)) else { return false };
        self.can_step_surf(a, b, from, to)
    }

    fn can_step_surf(&self, a: Surface, b: Surface, from: Cell, to: Cell) -> bool {
        use Surface::*;
        if b == Solid {
            return false;
        }
        let same_lodge = || self.lodge.get(from.0, from.1) == self.lodge.get(to.0, to.1);
        match (a, b) {
            (LodgeInside, LodgeInside) => same_lodge(),
            (LodgeEntrance, LodgeInside) => same_lodge() && to == (from.0, from.1 - 1),
            (LodgeInside, LodgeEntrance) => same_lodge() && from == (to.0, to.1 - 1),
            (LodgeInside, _) | (_, LodgeInside) => false,
            _ => true,
        }
    }

    pub fn find_path(&self, who: BeaverId, from: Cell, goal: Goal) -> Option<Path> {
        self.surface_at(from)?;
        let (target, beside) = match goal {
            Goal::Cell(c) | Goal::Inside(c) => (c, false),
            Goal::Beside(c) => (c, true),
        };
        match (goal, self.surface_at(target)?) {
            (Goal::Cell(_), Surface::Solid) => return None,
            (Goal::Inside(_), s) if s != Surface::LodgeInside => return None,
            _ => {}
        }
        let connected = if beside { self.may_reach_beside(from, target) } else { from == target || self.may_reach(from, [target]) };
        let free = |c: Cell| self.surface_at(c).is_some_and(|s| s != Surface::Solid) && !self.held_by_other(who, c);
        let open_end = if beside { self.neighbours(target).into_iter().flatten().any(free) } else { from == target || free(target) };
        if !connected || !open_end {
            return None;
        }
        let is_goal = |c: Cell| {
            if beside { c.0.abs_diff(target.0) + c.1.abs_diff(target.1) == 1 } else { c == target }
        };
        let heur = |c: Cell| {
            let d = (c.0.abs_diff(target.0) + c.1.abs_diff(target.1)) as u32;
            d.saturating_sub(beside as u32) * MIN_STEP
        };
        self.search(who, from, is_goal, heur)
    }

    pub fn find_path_beside_any(&self, who: BeaverId, from: Cell, targets: &[Cell]) -> Option<Path> {
        self.surface_at(from)?;
        if targets.is_empty() || !targets.iter().any(|&t| self.may_reach_beside(from, t)) {
            return None;
        }
        let free = |c: Cell| self.surface_at(c).is_some_and(|s| s != Surface::Solid) && !self.held_by_other(who, c);
        if !targets.iter().any(|&t| self.neighbours(t).into_iter().flatten().any(free)) {
            return None;
        }
        let (x0, x1) = (targets.iter().map(|t| t.0).min()?, targets.iter().map(|t| t.0).max()?);
        let (y0, y1) = (targets.iter().map(|t| t.1).min()?, targets.iter().map(|t| t.1).max()?);
        let is_goal = |c: Cell| targets.iter().any(|t| c.0.abs_diff(t.0) + c.1.abs_diff(t.1) == 1);
        let heur = |c: Cell| {
            let dx = if c.0 < x0 { x0 - c.0 } else { c.0.saturating_sub(x1) };
            let dy = if c.1 < y0 { y0 - c.1 } else { c.1.saturating_sub(y1) };
            ((dx + dy) as u32).saturating_sub(1) * MIN_STEP
        };
        self.search(who, from, is_goal, heur)
    }

    pub fn find_path_onto_any(&self, who: BeaverId, from: Cell, cells: &[Cell]) -> Option<Path> {
        self.surface_at(from)?;
        if cells.contains(&from) {
            return Some(Path { steps: Vec::new(), cost: 0.0 });
        }
        let free = |c: &&Cell| self.surface_at(**c).is_some_and(|s| s != Surface::Solid) && !self.held_by_other(who, **c);
        let ends: Vec<Cell> = cells.iter().filter(free).copied().collect();
        if ends.is_empty() || !self.may_reach(from, ends.iter().copied()) {
            return None;
        }
        let (x0, x1) = (ends.iter().map(|t| t.0).min()?, ends.iter().map(|t| t.0).max()?);
        let (y0, y1) = (ends.iter().map(|t| t.1).min()?, ends.iter().map(|t| t.1).max()?);
        let heur = |c: Cell| {
            let dx = if c.0 < x0 { x0 - c.0 } else { c.0.saturating_sub(x1) };
            let dy = if c.1 < y0 { y0 - c.1 } else { c.1.saturating_sub(y1) };
            (dx + dy) as u32 * MIN_STEP
        };
        self.search(who, from, |c| ends.contains(&c), heur)
    }

    fn held_by_other(&self, who: BeaverId, c: Cell) -> bool {
        !self.surface_at(c).is_some_and(shared) && matches!(self.reserved.get(c.0, c.1), Some(Some(o)) if *o != who)
    }

    fn search(&self, who: BeaverId, from: Cell, is_goal: impl Fn(Cell) -> bool, heur: impl Fn(Cell) -> u32) -> Option<Path> {
        let (w, h) = (self.surface.width(), self.surface.height());
        let surf = self.surface.as_slice();
        let blocked = |i: usize| !shared(surf[i]) && matches!(self.reserved.as_slice()[i], Some(o) if o != who);
        let start = from.1 * w + from.0;

        SCRATCH.with(|sc| {
            let sc = &mut *sc.borrow_mut();
            sc.begin(w * h);
            sc.set(start, 0, u32::MAX);
            sc.heap.push(Reverse(pack(heur(from), start)));
            while let Some(Reverse(key)) = sc.heap.pop() {
                let (f, i) = ((key >> 32) as u32, key as u32 as usize);
                let g = sc.g[i];
                let cur = (i % w, i / w);
                if f > g + heur(cur) {
                    continue;
                }
                if is_goal(cur) {
                    return Some(sc.build(i, start, w, surf));
                }
                let (x, y) = cur;
                let nbrs = [
                    (y > 0).then(|| i - w),
                    (x > 0).then(|| i - 1),
                    (x + 1 < w).then(|| i + 1),
                    (y + 1 < h).then(|| i + w),
                ];
                for j in nbrs.into_iter().flatten() {
                    let Some((c, _)) = Self::step_cost(surf[j]) else { continue };
                    if !self.can_step_surf(surf[i], surf[j], cur, (j % w, j / w)) || blocked(j) {
                        continue;
                    }
                    let ng = g + (c * COST_UNIT).round() as u32;
                    if sc.best(j).is_none_or(|old| ng < old) {
                        sc.set(j, ng, i as u32);
                        sc.heap.push(Reverse(pack(ng + heur((j % w, j / w)), j)));
                    }
                }
            }
            None
        })
    }

    pub fn advance(&mut self, who: BeaverId, from: Cell, to: Cell) -> Result<(), NavError> {
        let from_shared = self.surface_at(from).is_some_and(shared);
        if !from_shared && self.reserved.get(from.0, from.1).copied().flatten() != Some(who) {
            return Err(NavError::NotHolder(from));
        }
        if from.0.abs_diff(to.0) + from.1.abs_diff(to.1) != 1 {
            return Err(NavError::NotAdjacent);
        }
        let (Some(a), Some(b)) = (self.surface_at(from), self.surface_at(to)) else {
            return Err(NavError::OutOfBounds);
        };
        if !self.can_step_surf(a, b, from, to) {
            return Err(NavError::Forbidden(b));
        }
        if !self.reserve(who, to) {
            return Err(NavError::Occupied(to));
        }
        self.release(who, from);
        Ok(())
    }

    pub fn reserve(&mut self, who: BeaverId, cell: Cell) -> bool {
        if self.surface_at(cell).is_some_and(shared) {
            return true;
        }
        match self.reserved.get_mut(cell.0, cell.1) {
            Some(slot) if slot.is_none() || *slot == Some(who) => {
                *slot = Some(who);
                true
            }
            _ => false,
        }
    }
    pub fn release(&mut self, who: BeaverId, cell: Cell) {
        if let Some(slot) = self.reserved.get_mut(cell.0, cell.1) {
            if *slot == Some(who) {
                *slot = None;
            }
        }
    }
    pub fn surface_at(&self, cell: Cell) -> Option<Surface> {
        self.surface.get(cell.0, cell.1).copied()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const A: BeaverId = BeaverId(1);

    fn map(rows: &[&str]) -> NavGrid {
        let mut n = NavGrid::new(rows[0].len(), rows.len());
        for (y, r) in rows.iter().enumerate() {
            for (x, ch) in r.chars().enumerate() {
                if ch == 'L' || ch == 'E' {
                    n.set_lodge_cell((x, y), if ch == 'L' { Surface::LodgeInside } else { Surface::LodgeEntrance }, 1);
                    continue;
                }
                n.set((x, y), match ch {
                    '.' => Surface::Land,
                    '~' => Surface::Water,
                    '#' => Surface::Solid,
                    'D' => Surface::Dam,
                    'L' => Surface::LodgeInside,
                    'E' => Surface::LodgeEntrance,
                    _ => panic!("bad map char {ch}"),
                });
            }
        }
        n
    }

    fn cells(p: &Path) -> Vec<Cell> {
        p.steps.iter().map(|s| s.0).collect()
    }

    #[test]
    #[ignore = "beavers: off until the water engine is done"]
    fn goes_around_tree_and_building() {
        let n = map(&["......", ".###..", "......"]);
        let p = n.find_path(A, (0, 1), Goal::Cell((5, 1))).unwrap();
        assert!(cells(&p).iter().all(|&c| n.surface_at(c) != Some(Surface::Solid)));
        assert_eq!(p.steps.len(), 7);
        assert_eq!(p.steps.last().unwrap().0, (5, 1));
        assert!(n.find_path(A, (0, 0), Goal::Cell((2, 1))).is_none(), "solid goal rejected");
        let walled = map(&[".#.", ".#.", ".#."]);
        assert!(walled.find_path(A, (0, 0), Goal::Cell((2, 0))).is_none());
    }

    #[test]
    #[ignore = "beavers: off until the water engine is done"]
    fn beside_goal_stops_next_to_solid() {
        let n = map(&["...", ".#.", "..."]);
        let p = n.find_path(A, (0, 0), Goal::Beside((1, 1))).unwrap();
        assert_eq!(p.steps.len(), 1);
        let end = p.steps[0].0;
        assert_eq!(end.0.abs_diff(1) + end.1.abs_diff(1), 1);
    }

    #[test]
    #[ignore = "beavers: off until the water engine is done"]
    fn crosses_dam_by_climbing() {
        let n = map(&["..D..", "..D..", "..D.."]);
        let p = n.find_path(A, (0, 1), Goal::Cell((4, 1))).unwrap();
        assert_eq!(p.steps[1], ((2, 1), Gait::Climb));
        assert_eq!(p.steps.iter().filter(|s| s.1 == Gait::Climb).count(), 1);
        assert!((p.cost - 6.0).abs() < 1e-3, "cost {}", p.cost);
    }

    #[test]
    #[ignore = "beavers: off until the water engine is done"]
    fn lodge_only_by_underwater_entrance() {
        let n = map(&["LLL.", "LLL.", "~E~.", "...."]);
        let p = n.find_path(A, (3, 3), Goal::Inside((1, 0))).unwrap();
        let c = cells(&p);
        let e = c.iter().position(|&x| x == (1, 2)).expect("passes entrance");
        assert_eq!(c[e + 1], (1, 1));
        assert!(n.can_step((0, 2), (1, 2)));
        assert!(!n.can_step((3, 0), (2, 0)), "land to interior");
        assert!(!n.can_step((0, 2), (0, 1)), "water to interior");
        assert!(!n.can_step((2, 0), (3, 0)), "interior to land");
        assert!(n.can_step((1, 2), (1, 1)));
        let mut closed = n.clone();
        closed.set((1, 2), Surface::Solid);
        assert!(closed.find_path(A, (3, 3), Goal::Inside((1, 0))).is_none());
        assert!(n.find_path(A, (3, 3), Goal::Inside((3, 3))).is_none());
    }

    #[test]
    #[ignore = "beavers: off until the water engine is done"]
    fn lodges_are_entered_only_through_their_own_entrance() {
        let mut n = NavGrid::new(8, 6);
        n.add_lodge((0, 0));
        n.add_lodge((3, 0));
        assert_eq!(n.surface_at((1, 3)), Some(Surface::LodgeEntrance));
        assert_eq!(n.surface_at((4, 3)), Some(Surface::LodgeEntrance));
        assert!(!n.can_step((2, 1), (3, 1)));
        assert!(!n.can_step((3, 1), (2, 1)));
        assert!(n.can_step((1, 3), (1, 2)) && n.can_step((1, 2), (1, 3)));
        assert!(!n.can_step((4, 3), (1, 2)) && !n.can_step((1, 3), (4, 2)));
        let mut m = NavGrid::new(8, 8);
        m.add_lodge((0, 0));
        m.add_lodge((2, 3));
        assert!(!m.can_step((1, 3), (2, 3)), "A's entrance must not open into B");
        let p = m.find_path(A, (7, 7), Goal::Inside((0, 0))).unwrap();
        assert!(p.steps.iter().all(|s| m.lodge.get(s.0 .0, s.0 .1) != Some(&9)), "path crossed lodge B");
    }

    #[test]
    #[ignore = "beavers: off until the water engine is done"]
    fn takes_water_when_cheaper() {
        let n = map(&["#.....#", "#.~~~.#", "#.~~~.#", "#.....#"]);
        let swim = n.find_path(A, (1, 1), Goal::Cell((5, 1))).unwrap();
        assert!(swim.steps.iter().any(|s| s.1 == Gait::Swim));
        assert!((swim.cost - 2.8).abs() < 1e-3, "cost {}", swim.cost);
        let land = map(&["#.....#", "#.....#", "#.....#", "#.....#"]);
        let lp = land.find_path(A, (1, 1), Goal::Cell((5, 1))).unwrap();
        assert!(swim.cost < lp.cost);
        let d = map(&["...D...", "~~~D~~~"]);
        let p = d.find_path(A, (0, 1), Goal::Cell((6, 1))).unwrap();
        assert!(p.steps.iter().any(|s| s.1 == Gait::Climb));
    }

    #[test]
    #[ignore = "beavers: off until the water engine is done"]
    fn reservations_block_others_but_not_holder() {
        let mut n = map(&["...", "###", "..."]);
        n.set((1, 1), Surface::Land);
        let b = BeaverId(2);
        assert!(n.reserve(b, (1, 1)));
        assert!(n.find_path(A, (0, 0), Goal::Cell((0, 2))).is_none(), "corridor held by B");
        assert!(n.find_path(b, (0, 0), Goal::Cell((0, 2))).is_some());
        assert!(!n.reserve(A, (1, 1)));
        n.release(A, (1, 1));
        assert_eq!(n.reserved.get(1, 1), Some(&Some(b)));
        n.release(b, (1, 1));
        assert!(n.find_path(A, (0, 0), Goal::Cell((0, 2))).is_some());
    }

    #[test]
    #[ignore = "beavers: off until the water engine is done"]
    fn a_walled_off_goal_is_refused_without_a_search_and_regions_follow_changes() {
        let mut n = map(&["..#..", "..#..", "..#.."]);
        n.label_regions();
        assert!(!n.may_reach((0, 0), [(4, 0)]), "left and right are cut apart");
        assert!(n.find_path(A, (0, 0), Goal::Cell((4, 2))).is_none());
        assert!(n.find_path_beside_any(A, (0, 0), &[(4, 1)]).is_none());
        assert!(n.find_path_beside_any(A, (0, 0), &[(2, 1)]).is_some(), "beside the wall is on this side");
        n.set((2, 1), Surface::Land);
        assert!(n.find_path(A, (0, 0), Goal::Cell((4, 2))).is_some());
        n.label_regions();
        assert!(n.may_reach((0, 0), [(4, 0)]));
    }

    #[test]
    #[ignore = "beavers: off until the water engine is done"]
    fn a_beaver_resting_by_the_door_never_shuts_the_lodge() {
        let b = BeaverId(2);
        let mut n = map(&["LLL", "LLL", "LLL", "~E~", "~~~"]);
        n.label_regions();
        assert!(n.reserve(b, (1, 2)), "the cell just inside the door");
        let p = n.find_path_onto_any(A, (0, 4), &[(0, 0), (1, 0), (2, 0)]).expect("in past the resting beaver");
        assert!(p.steps.iter().any(|s| s.0 == (1, 2)));
        assert!(n.reserve(A, (0, 4)));
        for &(c, _) in &p.steps {
            let from = if c == p.steps[0].0 { (0, 4) } else { p.steps[p.steps.iter().position(|s| s.0 == c).unwrap() - 1].0 };
            n.advance(A, from, c).expect("lodge cells are shared");
        }
    }

    #[test]
    #[ignore = "beavers: off until the water engine is done"]
    fn advance_enforces_rules() {
        let mut n = map(&[".#E", "L.~"]);
        assert!(n.reserve(A, (0, 0)));
        assert_eq!(n.advance(A, (0, 0), (1, 0)), Err(NavError::Forbidden(Surface::Solid)));
        assert_eq!(n.advance(A, (0, 0), (1, 1)), Err(NavError::NotAdjacent));
        assert_eq!(n.advance(A, (2, 1), (1, 1)), Err(NavError::NotHolder((2, 1))));
        assert_eq!(n.advance(A, (0, 0), (0, 1)), Err(NavError::Forbidden(Surface::LodgeInside)));
        assert_eq!(n.advance(A, (0, 0), (9, 0)), Err(NavError::NotAdjacent));
        assert_eq!(n.reserved.get(0, 0), Some(&Some(A)));
    }

    #[test]
    #[ignore = "beavers: off until the water engine is done"]
    fn rebuild_from_sim_marks_solids_dams_water() {
        use crate::resources::ResourceKind;
        let mut sim = crate::SimState::new(7);
        sim.resources = Default::default();
        sim.resources.add(ResourceKind::Tree, (3, 3), 1.0);
        sim.water.dam_height.get_mut(5, 5).map(|h| *h = 1.0);
        sim.water.depth.get_mut(8, 8).map(|d| *d = 1.0);
        let mut nav = sim.nav.clone();
        nav.rebuild(&sim);
        sim.nav = nav;
        assert_eq!(sim.nav.surface_at((3, 3)), Some(Surface::Thicket), "trees slow beavers but never wall them in");
        assert_eq!(sim.nav.surface_at((5, 5)), Some(Surface::Dam));
        assert_eq!(sim.nav.surface_at((8, 8)), Some(Surface::Water));
    }

    struct Rng(u64);
    impl Rng {
        fn next(&mut self) -> u64 {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            self.0
        }
        fn below(&mut self, n: usize) -> usize {
            (self.next() % n as u64) as usize
        }
    }

    fn random_world(rng: &mut Rng, size: usize) -> NavGrid {
        let mut n = NavGrid::new(size, size);
        for y in 0..size {
            for x in 0..size {
                let r = rng.below(100);
                let s = match r {
                    0..=7 => Surface::Solid,
                    8..=12 => Surface::Dam,
                    13..=35 => Surface::Water,
                    _ => Surface::Land,
                };
                n.set((x, y), s);
            }
        }
        for _ in 0..6 {
            let (x, y) = (rng.below(size - 4), rng.below(size - 4));
            for dy in 0..3 {
                for dx in 0..3 {
                    n.set((x + dx, y + dy), Surface::Land);
                }
            }
            n.add_lodge((x, y));
        }
        n
    }

    #[test]
    #[ignore = "beavers: off until the water engine is done"]
    fn stress_200_beavers_1000_ticks_no_overlap() {
        let size = 64;
        let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
        let mut n = random_world(&mut rng, size);
        let ids: Vec<BeaverId> = (0..200).map(BeaverId).collect();
        let mut pos: Vec<Cell> = Vec::new();
        for &id in &ids {
            loop {
                let c = (rng.below(size), rng.below(size));
                if n.surface_at(c) != Some(Surface::Solid) && n.reserve(id, c) {
                    pos.push(c);
                    break;
                }
            }
        }
        let mut goals: Vec<Option<Goal>> = vec![None; ids.len()];
        let mut paths: Vec<Vec<(Cell, Gait)>> = vec![Vec::new(); ids.len()];
        let mut moved = 0usize;
        for tick in 0..1000 {
            for k in 0..ids.len() {
                if paths[k].is_empty() {
                    let c = (rng.below(size), rng.below(size));
                    let g = match (n.surface_at(c), rng.below(3)) {
                        (Some(Surface::LodgeInside), _) => Goal::Inside(c),
                        (_, 0) => Goal::Beside(c),
                        _ => Goal::Cell(c),
                    };
                    goals[k] = Some(g);
                    if let Some(p) = n.find_path(ids[k], pos[k], g) {
                        paths[k] = p.steps;
                        paths[k].reverse();
                    }
                }
                if let Some(&(next, _)) = paths[k].last() {
                    match n.advance(ids[k], pos[k], next) {
                        Ok(()) => {
                            pos[k] = next;
                            paths[k].pop();
                            moved += 1;
                        }
                        Err(NavError::Occupied(_)) => paths[k].clear(),
                        Err(e) => panic!("planned an illegal step {e:?} at tick {tick}"),
                    }
                }
            }
            let mut seen = std::collections::HashSet::new();
            let mut outside = 0;
            for (k, &c) in pos.iter().enumerate() {
                assert_ne!(n.surface_at(c), Some(Surface::Solid), "beaver in solid at tick {tick}");
                if n.surface_at(c).is_some_and(shared) {
                    continue;
                }
                outside += 1;
                assert!(seen.insert(c), "two beavers share {c:?} at tick {tick}");
                assert_eq!(n.reserved.get(c.0, c.1), Some(&Some(ids[k])));
            }
            let held = n.reserved.as_slice().iter().flatten().count();
            assert_eq!(held, outside, "leaked reservation at tick {tick}");
        }
        assert!(moved > 20_000, "beavers barely moved: {moved}");
    }

}
