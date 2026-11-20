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
    // Beavers can pass one another. Treating a resting beaver as a solid,
    // one-metre roadblock made routes fail and repeatedly search the whole map.
    // Buildings and terrain still enforce all static traversal rules.
    s != Surface::Solid
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

pub(crate) fn minimum_cost(steps: usize) -> f32 {
    (steps as u32).saturating_mul(MIN_STEP) as f32 / COST_UNIT
}

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
    /// What the regions were last labelled from (each cell solid, open or part of a lodge, and the
    /// lodges): relabel only when it differs. Water coming and going does not change regions.
    labelled: Vec<u8>,
    labelled_lodge: Vec<u32>,
    refresh_key: Option<(u64,u64,u64,u64,usize,u64)>,
    dry_surface: Vec<Surface>,
    wet_surface: Vec<Surface>,
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
            labelled: Vec::new(),
            labelled_lodge: Vec::new(),
            refresh_key: None,
            dry_surface: vec![Surface::Land;width*height],
            wet_surface: vec![Surface::Water;width*height],
        }
    }

    pub fn label_regions(&mut self) {
        // Regions depend only on the surface and the lodges; the nav grid is rebuilt every step,
        // but they rarely change.
        let class = |s: &Surface| match s {
            Surface::Solid => 1u8,
            Surface::LodgeInside => 2,
            Surface::LodgeEntrance => 3,
            _ => 0,
        };
        let same = self.region.len() == self.labelled.len()
            && self.labelled.len() == self.surface.as_slice().len()
            && self.labelled.iter().zip(self.surface.as_slice()).all(|(&c, s)| c == class(s))
            && self.labelled_lodge.as_slice() == self.lodge.as_slice();
        if same {
            self.regions_ok = true;
            return;
        }
        self.labelled.clear();
        self.labelled.extend(self.surface.as_slice().iter().map(class));
        self.labelled_lodge.clear();
        self.labelled_lodge.extend_from_slice(self.lodge.as_slice());
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

    fn key(sim: &crate::SimState) -> (u64,u64,u64,u64,usize,u64) {
        let mut buildings=0xcbf29ce484222325u64;
        for b in &sim.buildings.buildings {
            for value in [b.id.0,b.kind as u64,b.pos.0 as u64,b.pos.1 as u64,b.is_complete() as u64] {
                buildings=(buildings^value).wrapping_mul(0x100000001b3);
            }
        }
        (sim.water.nav_revision,sim.water.topology_revision,buildings,sim.resources.nav_revision(),sim.resources.items.len(),sim.time_days.floor() as u64)
    }

    /// Water-level changes only update affected cells. Buildings and vegetation
    /// rebuild the cached dry/wet surfaces when their topology changes.
    pub fn refresh(&mut self, sim: &mut crate::SimState) {
        let key=Self::key(sim);
        let static_key=|k:(u64,u64,u64,u64,usize,u64)|(k.1,k.2,k.3,k.4,k.5);
        if self.refresh_key.is_some_and(|old|static_key(old)==static_key(key)) {
            if self.refresh_key!=Some(key) {
                let mut connectivity=false;
                let blocked=|s:Surface|matches!(s,Surface::Solid|Surface::LodgeInside|Surface::LodgeEntrance);
                for &i in &sim.water.nav_cells {
                    let next=if sim.water.depth.as_slice()[i]>crate::buildings::WET_DEPTH {self.wet_surface[i]} else {self.dry_surface[i]};
                    let old=self.surface.as_slice()[i];
                    connectivity|=blocked(old)!=blocked(next);
                    self.surface.as_mut_slice()[i]=next;
                }
                if connectivity {self.label_regions();}
            }
            self.refresh_key=Some(key);
        } else {self.rebuild(sim);}
        sim.water.clear_navigation_changes();
    }

    pub fn rebuild(&mut self, sim: &crate::SimState) {
        let w=self.surface.width();
        self.lodge.as_mut_slice().fill(0);
        let (depth,dam,canal)=(sim.water.depth.as_slice(),sim.water.dam_height.as_slice(),sim.water.canal.as_slice());
        for i in 0..depth.len() {
            self.dry_surface[i]=if dam[i]>0.0 {Surface::Dam} else {Surface::Land};
            self.wet_surface[i]=if dam[i]>0.0 {Surface::Dam} else if canal[i] {Surface::Canal} else {Surface::Water};
            self.surface.as_mut_slice()[i]=if depth[i]>crate::buildings::WET_DEPTH {self.wet_surface[i]} else {self.dry_surface[i]};
        }
        for b in &sim.buildings.buildings {
            match b.kind {
                BuildingKind::Canal if b.is_complete()=>{
                    for (x,y) in b.cells() {if self.wet_surface[y*w+x]==Surface::Water {self.wet_surface[y*w+x]=Surface::Canal;}}
                }
                BuildingKind::Dam|BuildingKind::Canal|BuildingKind::Lock|BuildingKind::Dock|BuildingKind::Barge=>{}
                BuildingKind::Lodge if b.is_complete()=>{
                    self.add_lodge(b.pos);
                    for (x,y) in b.cells().chain(std::iter::once(lodge_entrance(b.pos))) {
                        let i=y*w+x;
                        self.dry_surface[i]=self.surface.as_slice()[i];self.wet_surface[i]=self.surface.as_slice()[i];
                    }
                }
                _ if !b.is_complete()=>{}
                _=>for (x,y) in b.cells() {self.dry_surface[y*w+x]=Surface::Solid;self.wet_surface[y*w+x]=Surface::Solid;},
            }
        }
        for r in sim.resources.items.iter().filter(|r|r.is_solid()) {
            let i=r.cell.1*w+r.cell.0;
            if self.dry_surface[i]==Surface::Land {self.dry_surface[i]=if r.kind==crate::resources::ResourceKind::Tree {Surface::Thicket} else {Surface::Solid};}
        }
        for (i,&d) in depth.iter().enumerate() {self.surface.as_mut_slice()[i]=if d>crate::buildings::WET_DEPTH {self.wet_surface[i]} else {self.dry_surface[i]};}
        self.label_regions();self.refresh_key=Some(Self::key(sim));
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
