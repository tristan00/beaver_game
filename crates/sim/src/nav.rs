include!(concat!(env!("OUT_DIR"), "/nav_constants.rs"));
use std::cell::RefCell;
use std::cmp::Reverse;
use std::collections::BinaryHeap;

use crate::colony::BeaverId;
use crate::grid::Grid;
use crate::water::Cell;

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Surface {
    Land,
    Water,
    Canal,
    Dam,
    Solid,
    Thicket,
    LodgeEntrance,
    LodgeInside,
    /// A gate upgraded into a lock: swum through, one beaver at a time (see `locks`).
    Lock,
}

pub fn shared(s: Surface) -> bool {
    // Beavers can slip past one another. Treating a resting beaver as a solid,
    // one-metre roadblock made routes fail and repeatedly search the whole map.
    // Buildings and terrain still enforce all static traversal rules.
    s != Surface::Solid
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Gait {
    Walk,
    Swim,
    Climb,
}

const COST_UNIT: f32 = 1000.0;

/// Where a beaver swims (and the current carries it): along a canal (0) or other water (1).
fn lane(s: Surface) -> Option<usize> {
    match NavGrid::step_cost(s) {
        Some((_, Gait::Swim)) => Some((s != Surface::Canal) as usize),
        _ => None,
    }
}

/// What a swimmer at `pace` spends on each cell of a step onto a cell costing `base` in still
/// water, with the current `drift` (in swim speeds, `SWIM_SPEED_M_S`) along the step: its own
/// speed there plus the current's, never under `MIN_SWIM_SHARE` of its own, so it can still
/// work its way up a current faster than it swims. Movement is budgeted at `pace` (see
/// `agents::follow`), so the cost is that budget over the speed.
fn swim_cost(base: f32, drift: f32, pace: f32) -> f32 {
    if drift == 0.0 {
        return base;
    }
    let own = pace / base;
    pace / (own + drift / SWIM_COST).max(MIN_SWIM_SHARE * own)
}

pub fn lodge_entrance(pos: Cell) -> Cell {
    (pos.0 + 1, pos.1 + 3)
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Path {
    pub steps: Vec<(Cell, Gait)>,
    pub cost: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Goal {
    Cell(Cell),
    Beside(Cell),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum NavError {
    NotAdjacent,
    OutOfBounds,
    Forbidden(Surface),
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct NavGrid {
    pub surface: Grid<Surface>,
    pub lodge: Grid<u32>,
    #[serde(skip)]
    region: Vec<u32>,
    #[serde(skip)]
    regions_ok: bool,
    #[serde(skip)]
    labelled: Vec<u8>,
    #[serde(skip)]
    labelled_lodge: Vec<u32>,
    #[serde(skip)]
    refresh_key: Option<(u64,u64,u64,u64,usize,u64)>,
    #[serde(skip)]
    dry_surface: Vec<Surface>,
    #[serde(skip)]
    wet_surface: Vec<Surface>,
    #[serde(skip)]
    floating: Vec<bool>,
    #[serde(skip)]
    current: Vec<(f32, f32)>,
    #[serde(skip)]
    fastest: [f32; 2],
}

crate::grid::clone_fields!(NavGrid { surface, lodge, region, regions_ok, labelled, labelled_lodge, refresh_key, dry_surface, wet_surface, floating, current, fastest });

#[derive(Default, serde::Serialize, serde::Deserialize)]
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

/// Lower bound on the cost of a route `dx` by `dy` cells when a step costs at least `min` a
/// cell and diagonal steps are √2 long.
fn octile(dx: usize, dy: usize, min: u32) -> u32 {
    let (a, b) = (dx.max(dy) as f32, dx.min(dy) as f32);
    ((a + (std::f32::consts::SQRT_2 - 1.0) * b) * min as f32) as u32
}

/// Cost factor of a step: √2 for a diagonal one.
/// Going into a lock from outside, the wait while the gates and the chamber work, in movement
/// cost at full pace: the gate opens, the swimmer comes in, the gate shuts behind it and the
/// chamber fills or drains to the far side's level before the far gate opens.
pub fn lock_wait(from: Surface, to: Surface) -> f32 {
    if to == Surface::Lock && from != Surface::Lock { crate::locks::LOCK_WAIT_DAYS * crate::agents::MOVE_COST_PER_DAY } else { 0.0 }
}

pub fn step_length(from: Cell, to: Cell) -> f32 {
    if from.0 != to.0 && from.1 != to.1 { std::f32::consts::SQRT_2 } else { 1.0 }
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
            lodge: Grid::new(width, height, 0),
            region: Vec::new(),
            regions_ok: false,
            labelled: Vec::new(),
            labelled_lodge: Vec::new(),
            refresh_key: None,
            dry_surface: vec![Surface::Land;width*height],
            wet_surface: vec![Surface::Water;width*height],
            floating: vec![false;width*height],
            current: Vec::new(),
            fastest: [0.0; 2],
        }
    }

    /// The least a step can cost a cell (in `COST_UNIT`s, rounded down): swimming a canal or
    /// other water with the fastest current there behind it. Bounds the cost of a route,
    /// keeping the search exact.
    fn min_step(&self) -> u32 {
        let cheapest = swim_cost(CANAL_COST, self.fastest[0], 1.0).min(swim_cost(SWIM_COST, self.fastest[1], 1.0));
        ((cheapest * COST_UNIT) as u32).saturating_sub(1).max(1)
    }

    /// Least cost of a route of `steps` steps.
    pub(crate) fn minimum_cost(&self, steps: usize) -> f32 {
        (steps as u32).saturating_mul(self.min_step()) as f32 / COST_UNIT
    }

    /// The water's current at a cell, in swim speeds along x and y.
    fn current_of(&self, i: usize) -> (f32, f32) {
        self.current.get(i).copied().unwrap_or((0.0, 0.0))
    }

    /// Takes the water's currents: every cell's (`all`), or those changed since last taken.
    /// `fastest` is kept above the current of every cell swum (a thin sheet of water running
    /// over dry ground can be far faster, but is walked).
    fn take_currents(&mut self, water: &crate::water::Water, all: bool) {
        let v = water.velocity.as_slice();
        let scale = 1.0 / SWIM_SPEED_M_S;
        let speed2 = |c: (f32, f32)| c.0 * c.0 + c.1 * c.1;
        let surf = self.surface.as_slice();
        let mut top = [0.0f32; 2];
        if all || self.current.len() != v.len() || surf.len() != v.len() {
            self.current.clear();
            self.current.extend(v.iter().map(|&(x, y)| (x * scale, y * scale)));
            for (&c, &s) in self.current.iter().zip(surf) {
                if let Some(k) = lane(s) {
                    top[k] = top[k].max(speed2(c));
                }
            }
        } else {
            top = self.fastest.map(|f| f * f);
            for &i in water.current_changes() {
                let (x, y) = v[i as usize];
                self.current[i as usize] = (x * scale, y * scale);
            }
            // Cells whose current changed, or newly under water.
            for i in water.current_changes().iter().map(|&i| i as usize).chain(water.nav_cells.iter().copied()) {
                if let Some(k) = lane(surf[i]) {
                    top[k] = top[k].max(speed2(self.current[i]));
                }
            }
        }
        self.fastest = top.map(f32::sqrt);
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

    pub fn floats(&self, c: Cell) -> bool {
        let w=self.surface.width();
        self.floating.get(c.1*w+c.0).copied().unwrap_or(false)
    }

    pub fn may_reach_beside(&self, from: Cell, target: Cell) -> bool {
        self.may_reach(from, self.neighbours(target).into_iter().flatten())
    }

    fn key(sim: &crate::SimState) -> (u64,u64,u64,u64,usize,u64) {
        let mut buildings=0xcbf29ce484222325u64;
        for b in &sim.buildings.buildings {
            for value in [b.id.0,b.kind.index() as u64,b.pos.0 as u64,b.pos.1 as u64,b.is_complete() as u64] {
                buildings=(buildings^value).wrapping_mul(0x100000001b3);
            }
        }
        for l in crate::locks::built(sim) {
            buildings=(buildings^((l.cell.0 as u64)<<32|l.cell.1 as u64)).wrapping_mul(0x100000001b3);
        }
        (sim.water.nav_revision,sim.water.topology_revision,buildings,sim.resources.nav_revision(),sim.resources.items().len(),sim.time_days.floor() as u64)
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
                    let next=self.surface_now(i,sim.water.depth.as_slice()[i],sim.water.ice.as_slice()[i]);
                    let old=self.surface.as_slice()[i];
                    connectivity|=blocked(old)!=blocked(next);
                    self.surface.as_mut_slice()[i]=next;
                }
                if connectivity {self.label_regions();}
            }
            self.refresh_key=Some(key);
            self.take_currents(&sim.water,false);
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
            self.floating[i]=canal[i] && depth[i]>=crate::canals::FLOAT_DEPTH;
        }
        for b in &sim.buildings.buildings {
            match b.kind {
                k if k.is_waterworks()=>{}
                k if k.is_home() && b.is_complete()=>{
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
        // A lock is swum through, whatever the water there, right through a widened dam, and
        // only down its lane: nothing goes through its walls.
        for l in crate::locks::built(sim) {
            for c in crate::locks::wall_cells(sim,&l) {
                let i=c.1*w+c.0;
                if i<self.dry_surface.len() {self.dry_surface[i]=Surface::Solid;self.wet_surface[i]=Surface::Solid;}
            }
            for c in crate::locks::lock_cells(sim,&l) {
                let i=c.1*w+c.0;
                if i<self.dry_surface.len() {self.dry_surface[i]=Surface::Lock;self.wet_surface[i]=Surface::Lock;}
            }
        }
        for b in &sim.resources.boulders {
            let i=b.cell.1*w+b.cell.0;
            if i<self.dry_surface.len() && self.dry_surface[i]==Surface::Land {self.dry_surface[i]=Surface::Solid;}
        }
        for r in sim.resources.items().iter().filter(|r|r.is_solid()) {
            let i=r.cell.1*w+r.cell.0;
            if self.dry_surface[i]==Surface::Land {self.dry_surface[i]=if r.kind.is_tree() {Surface::Thicket} else {Surface::Solid};}
        }
        let ice=sim.water.ice.as_slice();
        for (i,&d) in depth.iter().enumerate() {self.surface.as_mut_slice()[i]=self.surface_now(i,d,ice[i]);}
        self.label_regions();self.refresh_key=Some(Self::key(sim));
        self.take_currents(&sim.water,true);
    }

    /// What a cell is now: its wet or its dry surface by the water standing there, and water (a
    /// canal, a lock) frozen hard enough to bear a beaver is ground, walked over, not swum.
    fn surface_now(&self, i: usize, depth: f32, ice: f32) -> Surface {
        let s = if depth > crate::buildings::WET_DEPTH { self.wet_surface[i] } else { self.dry_surface[i] };
        if ice >= crate::tuning::water::ICE_WALK_M && matches!(s, Surface::Water | Surface::Canal | Surface::Lock) { Surface::Land } else { s }
    }

    pub fn set(&mut self, cell: Cell, s: Surface) {
        if let Some(slot) = self.surface.get_mut(cell.0, cell.1) {
            let open = |s: Surface| !matches!(s, Surface::Solid | Surface::LodgeInside | Surface::LodgeEntrance);
            if !(open(*slot) && open(s)) {
                self.regions_ok = false;
            }
            *slot = s;
            if let (Some(k), Some(c)) = (lane(s), self.current.get(cell.1 * self.surface.width() + cell.0)) {
                self.fastest[k] = self.fastest[k].max(c.0.hypot(c.1));
            }
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

    /// What a step from `from` onto the neighbouring cell `to` costs a beaver moving at `pace`
    /// (`agents::pace`), in the movement budget it spends: the surface's cost (`step_cost`), a
    /// swimmer's sped up or slowed by the current along the step. `None` for a solid cell.
    pub fn move_cost(&self, from: Cell, to: Cell, pace: f32) -> Option<f32> {
        let (dx, dy) = (to.0 as f32 - from.0 as f32, to.1 as f32 - from.1 as f32);
        let len = step_length(from, to);
        let wait = lock_wait(self.surface_at(from)?, self.surface_at(to)?) * pace;
        Some(len * self.cost_toward(to, (dx / len, dy / len), pace)? + wait)
    }

    /// As `move_cost`, for each cell of a move onto `to` heading `dir` (a unit vector in cells).
    pub fn cost_toward(&self, to: Cell, dir: (f32, f32), pace: f32) -> Option<f32> {
        let (base, gait) = Self::step_cost(self.surface_at(to)?)?;
        if gait != Gait::Swim {
            return Some(base);
        }
        let c = self.current_of(to.1 * self.surface.width() + to.0);
        Some(swim_cost(base, c.0 * dir.0 + c.1 * dir.1, pace))
    }

    pub fn step_cost(to: Surface) -> Option<(f32, Gait)> {
        match to {
            Surface::Land | Surface::LodgeInside => Some((WALK_COST, Gait::Walk)),
            Surface::Water | Surface::LodgeEntrance => Some((SWIM_COST, Gait::Swim)),
            Surface::Canal => Some((CANAL_COST, Gait::Swim)),
            Surface::Dam => Some((CLIMB_COST, Gait::Climb)),
            // Swum through; the wait for the gates is paid going in (`lock_wait`).
            Surface::Lock => Some((SWIM_COST, Gait::Swim)),
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
            Goal::Cell(c) => (c, false),
            Goal::Beside(c) => (c, true),
        };
        match (goal, self.surface_at(target)?) {
            (Goal::Cell(_), Surface::Solid) => return None,
            _ => {}
        }
        let connected = if beside { self.may_reach_beside(from, target) } else { from == target || self.may_reach(from, [target]) };
        let free = |c: Cell| self.surface_at(c).is_some_and(|s| s != Surface::Solid);
        let open_end = if beside { self.neighbours(target).into_iter().flatten().any(free) } else { from == target || free(target) };
        if !connected || !open_end {
            return None;
        }
        let is_goal = |c: Cell| {
            if beside { c.0.abs_diff(target.0) + c.1.abs_diff(target.1) == 1 } else { c == target }
        };
        let min = self.min_step();
        let heur = |c: Cell| octile(c.0.abs_diff(target.0), c.1.abs_diff(target.1), min).saturating_sub(beside as u32 * min);
        self.search(who, from, is_goal, heur)
    }

    pub fn find_path_beside_any(&self, who: BeaverId, from: Cell, targets: &[Cell]) -> Option<Path> {
        self.surface_at(from)?;
        if targets.is_empty() || !targets.iter().any(|&t| self.may_reach_beside(from, t)) {
            return None;
        }
        let free = |c: Cell| self.surface_at(c).is_some_and(|s| s != Surface::Solid);
        if !targets.iter().any(|&t| self.neighbours(t).into_iter().flatten().any(free)) {
            return None;
        }
        let (x0, x1) = (targets.iter().map(|t| t.0).min()?, targets.iter().map(|t| t.0).max()?);
        let (y0, y1) = (targets.iter().map(|t| t.1).min()?, targets.iter().map(|t| t.1).max()?);
        let is_goal = |c: Cell| targets.iter().any(|t| c.0.abs_diff(t.0) + c.1.abs_diff(t.1) == 1);
        let min = self.min_step();
        let heur = |c: Cell| {
            let dx = if c.0 < x0 { x0 - c.0 } else { c.0.saturating_sub(x1) };
            let dy = if c.1 < y0 { y0 - c.1 } else { c.1.saturating_sub(y1) };
            octile(dx, dy, min).saturating_sub(min)
        };
        self.search(who, from, is_goal, heur)
    }

    pub fn find_path_onto_any(&self, who: BeaverId, from: Cell, cells: &[Cell]) -> Option<Path> {
        self.surface_at(from)?;
        if cells.contains(&from) {
            return Some(Path { steps: Vec::new(), cost: 0.0 });
        }
        let free = |c: &&Cell| self.surface_at(**c).is_some_and(|s| s != Surface::Solid);
        let ends: Vec<Cell> = cells.iter().filter(free).copied().collect();
        if ends.is_empty() || !self.may_reach(from, ends.iter().copied()) {
            return None;
        }
        let (x0, x1) = (ends.iter().map(|t| t.0).min()?, ends.iter().map(|t| t.0).max()?);
        let (y0, y1) = (ends.iter().map(|t| t.1).min()?, ends.iter().map(|t| t.1).max()?);
        let min = self.min_step();
        let heur = |c: Cell| {
            let dx = if c.0 < x0 { x0 - c.0 } else { c.0.saturating_sub(x1) };
            let dy = if c.1 < y0 { y0 - c.1 } else { c.1.saturating_sub(y1) };
            octile(dx, dy, min)
        };
        self.search(who, from, |c| ends.contains(&c), heur)
    }

    fn search(&self, _who: BeaverId, from: Cell, is_goal: impl Fn(Cell) -> bool, heur: impl Fn(Cell) -> u32) -> Option<Path> {
        let (w, h) = (self.surface.width(), self.surface.height());
        let surf = self.surface.as_slice();
        let start = from.1 * w + from.0;
        let _p = crate::prof::scope("+path search");

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
                    (y > 0 && x > 0).then(|| i - w - 1),
                    (y > 0 && x + 1 < w).then(|| i - w + 1),
                    (y + 1 < h && x > 0).then(|| i + w - 1),
                    (y + 1 < h && x + 1 < w).then(|| i + w + 1),
                ];
                let open = |k: usize| Self::step_cost(surf[k]).is_some() && !matches!(surf[k], Surface::LodgeInside | Surface::LodgeEntrance);
                for j in nbrs.into_iter().flatten() {
                    let Some((c, gait)) = Self::step_cost(surf[j]) else { continue };
                    let to = (j % w, j / w);
                    if !self.can_step_surf(surf[i], surf[j], cur, to) {
                        continue;
                    }
                    let diagonal = to.0 != x && to.1 != y;
                    if diagonal && !(open(i) && open(j) && open(y * w + to.0) && open(to.1 * w + x)) {
                        continue;
                    }
                    let len = if diagonal { std::f32::consts::SQRT_2 } else { 1.0 };
                    let c = if gait == Gait::Swim {
                        let v = self.current_of(j);
                        swim_cost(c, (v.0 * (to.0 as f32 - x as f32) + v.1 * (to.1 as f32 - y as f32)) / len, 1.0)
                    } else {
                        c
                    };
                    let c = c * len + lock_wait(surf[i], surf[j]);
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

    pub fn advance(&mut self, _who: BeaverId, from: Cell, to: Cell) -> Result<(), NavError> {
        if from == to || from.0.abs_diff(to.0) > 1 || from.1.abs_diff(to.1) > 1 {
            return Err(NavError::NotAdjacent);
        }
        let (Some(a), Some(b)) = (self.surface_at(from), self.surface_at(to)) else {
            return Err(NavError::OutOfBounds);
        };
        if !self.can_step_surf(a, b, from, to) {
            return Err(NavError::Forbidden(b));
        }
        Ok(())
    }

    pub fn surface_at(&self, cell: Cell) -> Option<Surface> {
        self.surface.get(cell.0, cell.1).copied()
    }
}
