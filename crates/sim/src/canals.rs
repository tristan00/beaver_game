//! Canals are single buildings dug one cell at a time.
//!
//! A canal starts at water (its intake) and runs over land. The game sets each cell's target
//! bed from the intake's water level: 0.4 m below it, then falling 1 cm a metre, or following
//! the land down where it drops faster, and never rising downstream. Diggers lower one cell a
//! step per visit and leave the spoil as clay on the bank. The intake carries a gate, so a
//! canal never drains its source unless the player opens it. Dug cells silt back in; with
//! Maintain on, diggers clear them again.
include!(concat!(env!("OUT_DIR"), "/canals_constants.rs"));
use std::collections::VecDeque;

use crate::buildings::{Building, BuildingId, BuildingKind, Status};
use crate::gates::{Gate, GateSetting};
use crate::resources::ResourceKind;
use crate::water::Cell;
use crate::{SimError, SimState};

const EPS: f32 = 1e-3;

#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CanalCell {
    pub cell: Cell,
    /// Elevation the bed is dug down to.
    pub target: f32,
    /// The ground before any digging; silt never fills above it.
    pub ground: f32,
    /// 0 is the drawn route; 1 is the row added by Widen.
    pub row: u8,
    /// Silt settled since the last step was put back into the ground (metres).
    pub silt: f32,
    /// Reached its target at least once; from then on only silt needs clearing.
    pub done: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CanalDay {
    pub floatable: f32,
    pub inflow: f32,
    pub outflow: f32,
    pub silted: f32,
}

/// What the canal holds today, read from the live water.
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CanalStatus {
    pub cells: usize,
    pub wet: usize,
    pub floatable: usize,
    pub dug: usize,
    pub silted: usize,
    pub mean_depth: f32,
    pub max_depth: f32,
    /// Water entering at the intake and leaving at the far end (m³ per day).
    pub inflow: f32,
    pub outflow: f32,
    /// Water level at the intake's source.
    pub source_level: f32,
}

impl CanalStatus {
    pub fn floatable_share(&self) -> f32 {
        if self.cells == 0 { 0.0 } else { self.floatable as f32 / self.cells as f32 }
    }
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Canal {
    /// Row 0 in route order from the intake (the gate cell first), then any widened row.
    pub cells: Vec<CanalCell>,
    /// The water cell the canal draws from.
    pub source: Cell,
    pub gate: Gate,
    /// Ends in other water rather than on land.
    pub through: bool,
    pub maintain: bool,
    pub history: VecDeque<CanalDay>,
    pub status: Option<CanalStatus>,
    status_day: f64,
}

impl Canal {
    pub fn route(&self) -> impl Iterator<Item = &CanalCell> {
        self.cells.iter().filter(|c| c.row == 0)
    }
    pub fn width(&self) -> u8 {
        self.cells.iter().map(|c| c.row + 1).max().unwrap_or(1)
    }
    pub fn end(&self) -> Cell {
        self.route().last().map_or(self.gate.cell, |c| c.cell)
    }
}

/// How a canal grows when the player asks for more.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum CanalGrowth {
    Deepen,
    Widen,
    Extend,
}

fn invalid(m: impl Into<String>) -> SimError {
    SimError::InvalidCommand(m.into())
}

fn ground(sim: &SimState, c: Cell) -> f32 {
    sim.height.get(c.0, c.1).copied().unwrap_or(0.0)
}

fn depth(sim: &SimState, c: Cell) -> f32 {
    sim.water.depth.get(c.0, c.1).copied().unwrap_or(0.0)
}

/// Open water a canal may start from or end in.
pub fn is_water(sim: &SimState, c: Cell) -> bool {
    depth(sim, c) >= WET_DEPTH || sim.world.river.get(c.0, c.1).copied().unwrap_or(false) || sim.world.lake.get(c.0, c.1).copied().unwrap_or(false)
}

pub fn canal(sim: &SimState, id: BuildingId) -> Option<&Canal> {
    sim.buildings.building(id).and_then(|b| b.canal.as_deref())
}

fn canal_mut(sim: &mut SimState, id: BuildingId) -> Result<&mut Canal, SimError> {
    sim.buildings.buildings.iter_mut().find(|b| b.id == id).and_then(|b| b.canal.as_deref_mut()).ok_or_else(|| invalid("no such canal"))
}

/// The building whose gate stands on this cell, dam or canal.
pub fn gate_owner(sim: &SimState, cell: Cell) -> Option<BuildingId> {
    sim.buildings.buildings.iter().find(|b| {
        b.canal.as_ref().is_some_and(|c| c.gate.cell == cell) || b.dam.as_ref().is_some_and(|d| d.gates.iter().any(|g| g.cell == cell))
    }).map(|b| b.id)
}

/// A drawn route with every cell's target, before it is placed.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Plan {
    pub cells: Vec<CanalCell>,
    pub source: Cell,
    /// Water level at the source when drawn.
    pub level: f32,
    pub through: bool,
    /// Cells that would need a cut deeper than `MAX_CUT`.
    pub too_deep: Vec<Cell>,
    /// Depth each cell should hold once dug with the gate open, in route order.
    pub expected: Vec<f32>,
    /// The first land cell: the gate stands here.
    pub gate: Cell,
}

impl Plan {
    /// Digging visits the plan needs.
    pub fn visits(&self) -> usize {
        self.cells.iter().map(|c| visits_for(c.ground, c.target)).sum()
    }
    pub fn labour_days(&self) -> f32 {
        self.visits() as f32 * DIG_WORK
    }
    pub fn clay(&self) -> f32 {
        self.visits() as f32 * CLAY_PER_STEP
    }
    pub fn floatable_share(&self) -> f32 {
        if self.expected.is_empty() { 0.0 } else { self.expected.iter().filter(|&&d| d >= FLOAT_DEPTH).count() as f32 / self.expected.len() as f32 }
    }
}

fn visits_for(from: f32, target: f32) -> usize {
    ((from - target).max(0.0) / DIG_STEP - EPS).ceil().max(0.0) as usize
}

fn face_connected(cells: &[Cell]) -> bool {
    cells.windows(2).all(|w| w[0].0.abs_diff(w[1].0) + w[0].1.abs_diff(w[1].1) == 1)
}

fn owned_by_building(sim: &SimState, c: Cell, except: Option<BuildingId>) -> bool {
    sim.buildings.buildings.iter().any(|b| Some(b.id) != except && b.cells().any(|p| p == c))
        || sim.water.dam_height.get(c.0, c.1).is_some_and(|h| *h > 0.0)
}

/// Targets along a route that starts after the source water: the bed falls from `bed0`
/// by `BED_FALL` a cell, or keeps `MIN_COVER` under the ground where the land drops faster.
fn profile(sim: &SimState, route: &[Cell], bed0: f32) -> Vec<f32> {
    let mut out = Vec::with_capacity(route.len());
    let mut bed = bed0;
    for (i, &c) in route.iter().enumerate() {
        if i > 0 {
            bed = (bed - BED_FALL).min(ground(sim, c) - MIN_COVER);
        } else {
            bed = bed.min(ground(sim, c) - MIN_COVER.min(INTAKE_BELOW_LEVEL));
        }
        out.push(bed);
    }
    out
}

/// What a drawn route would be. `drawn` must be face-connected and start on water.
pub fn plan(sim: &SimState, drawn: &[Cell]) -> Result<Plan, String> {
    plan_from(sim, drawn, None)
}

fn plan_from(sim: &SimState, drawn: &[Cell], extending: Option<BuildingId>) -> Result<Plan, String> {
    if drawn.len() < if extending.is_some() { 1 } else { 2 } {
        return Err("Draw from water onto land".into());
    }
    if !face_connected(drawn) {
        return Err("A canal can't turn on a corner".into());
    }
    let n = sim.height.width();
    if drawn.iter().any(|c| c.0 >= n || c.1 >= n) {
        return Err("Outside the map".into());
    }
    let mut seen = std::collections::BTreeSet::new();
    if !drawn.iter().all(|c| seen.insert(*c)) {
        return Err("A canal can't cross itself".into());
    }
    let start = drawn.iter().position(|&c| !is_water(sim, c)).ok_or("Draw onto land")?;
    if start == 0 && extending.is_none() {
        return Err("Must start at water".into());
    }
    let last = drawn.iter().rposition(|&c| !is_water(sim, c)).expect("one land cell");
    // The drawn water at each end is part of the canal too: a shallow margin left undug
    // would be a second sill above the gate's, or a step at the outlet.
    let mut route: Vec<Cell> = drawn.to_vec();
    let through = last + 1 < drawn.len();
    let (source, level, bed0) = match extending.and_then(|id| canal(sim, id)) {
        Some(c) => {
            let end = c.route().last().copied().ok_or("Nothing to extend")?;
            if end.cell.0.abs_diff(route[0].0) + end.cell.1.abs_diff(route[0].1) != 1 {
                return Err("Draw on from the canal's end".into());
            }
            (c.source, c.status.as_ref().map_or(ground(sim, c.source) + depth(sim, c.source), |s| s.source_level), end.target - BED_FALL)
        }
        None => {
            let level = ground(sim, drawn[0]) + depth(sim, drawn[0]);
            let bed0 = level - INTAKE_BELOW_LEVEL;
            // Reach out to water deeper than the bed, or the shallow margin holds the pond up.
            let approach = approach_channel(sim, drawn[0], bed0, &drawn[1..]);
            let source = approach.first().copied().unwrap_or(drawn[0]);
            if !approach.is_empty() {
                route.splice(0..0, approach);
            }
            (source, level, bed0)
        }
    };
    let (start, last) = (start + route.len() - drawn.len(), last + route.len() - drawn.len());
    if route.len() > MAX_CELLS {
        return Err(format!("Too long (max {MAX_CELLS} cells)"));
    }
    if let Some(c) = route.iter().find(|&&c| owned_by_building(sim, c, extending)) {
        return Err(format!("Something is already built at {},{}", c.0, c.1));
    }
    let mut targets = vec![bed0; start];
    targets.extend(profile(sim, &route[start..=last], bed0));
    let mut bed = *targets.last().expect("one land cell");
    for &c in &route[last + 1..] {
        bed = (bed - BED_FALL).min(ground(sim, c));
        targets.push(bed);
    }
    let too_deep: Vec<Cell> = route.iter().zip(&targets).filter(|(c, t)| ground(sim, **c) - **t > MAX_CUT + EPS).map(|(&c, _)| c).collect();
    let expected = targets.iter().map(|&t| if through { (level - t).clamp(0.0, 0.3) } else { (level - t).max(0.0) }).collect();
    let cells = route.iter().zip(&targets).map(|(&cell, &target)| CanalCell { cell, target, ground: ground(sim, cell), row: 0, silt: 0.0, done: ground(sim, cell) <= target + EPS }).collect();
    Ok(Plan { cells, source, level, through, too_deep, expected, gate: route[start] })
}

const APPROACH_CELLS: usize = 15;

/// The shortest wet path from `from` out to water whose bottom is already below `bed`,
/// deepest end first (empty if `from` is deep enough or no such water is near).
fn approach_channel(sim: &SimState, from: Cell, bed: f32, avoid: &[Cell]) -> Vec<Cell> {
    if ground(sim, from) <= bed + EPS {
        return Vec::new();
    }
    let n = sim.height.width();
    let mut prev: std::collections::HashMap<Cell, Cell> = std::collections::HashMap::new();
    let mut queue = std::collections::VecDeque::from([(from, 0usize)]);
    prev.insert(from, from);
    while let Some((c, d)) = queue.pop_front() {
        if c != from && ground(sim, c) <= bed + EPS {
            let mut path = vec![c];
            let mut at = c;
            while prev[&at] != from {
                at = prev[&at];
                path.push(at);
            }
            return path;
        }
        if d >= APPROACH_CELLS {
            continue;
        }
        for (dx, dy) in [(1i64, 0i64), (-1, 0), (0, 1), (0, -1)] {
            let (x, y) = (c.0 as i64 + dx, c.1 as i64 + dy);
            if x < 0 || y < 0 || x >= n as i64 || y >= n as i64 {
                continue;
            }
            let m = (x as usize, y as usize);
            if prev.contains_key(&m) || avoid.contains(&m) || !is_water(sim, m) || owned_by_building(sim, m, None) {
                continue;
            }
            prev.insert(m, c);
            queue.push_back((m, d + 1));
        }
    }
    Vec::new()
}

pub fn check(plan: &Plan) -> Result<(), String> {
    if plan.too_deep.is_empty() { Ok(()) } else { Err(format!("Cut deeper than {MAX_CUT} m at {} cells", plan.too_deep.len())) }
}

/// Draw a new canal; diggers then work every cell.
pub fn place(sim: &mut SimState, drawn: &[Cell]) -> Result<BuildingId, SimError> {
    sim.unlocked(crate::buildings::BuildingKind::Canal).map_err(|e| SimError::InvalidCommand(e.to_string()))?;
    let p = plan(sim, drawn).map_err(invalid)?;
    check(&p).map_err(invalid)?;
    let first = *p.cells.iter().find(|c| c.cell == p.gate).expect("the gate is on the route");
    let gate = Gate::new(first.cell, first.target, first.ground.max(p.level + GATE_FREEBOARD));
    let id = BuildingId(sim.buildings.fresh_id());
    let canal = Canal { cells: p.cells, source: p.source, gate, through: p.through, maintain: true, history: VecDeque::new(), status: None, status_day: f64::NEG_INFINITY };
    let b = Building { id, kind: BuildingKind::Canal, pos: first.cell, status: Status::Complete, output: 0.0, condition: 1.0, reinforced: false, dam: None, canal: Some(Box::new(canal)) };
    sim.work.add_complete(&b).map_err(|e| invalid(format!("{e:?}")))?;
    sim.buildings.buildings.push(b);
    Ok(id)
}

fn cell_work(sim: &SimState, k: &Canal, c: &CanalCell) -> bool {
    let h = ground(sim, c.cell);
    if !c.done {
        return h > c.target + EPS;
    }
    k.maintain && h > c.target + SILT_TOLERANCE
}

/// Builders dig cells down to their target; maintainers clear silt from finished cells.
fn cell_task(sim: &SimState, k: &Canal, c: &CanalCell, upkeep: bool) -> bool {
    cell_work(sim, k, c) && c.done == upkeep
}

pub fn cell_needs_work(sim: &SimState, id: BuildingId, cell: Cell, upkeep: bool) -> bool {
    canal(sim, id).is_some_and(|k| k.cells.iter().any(|c| c.cell == cell && cell_task(sim, k, c, upkeep)))
}

/// Work for this canal's builders (digging, gate changes) or maintainers (silt).
pub fn has_work_for(sim: &SimState, id: BuildingId, upkeep: bool) -> bool {
    canal(sim, id).is_some_and(|k| (!upkeep && k.gate.built && k.gate.pending.is_some()) || k.cells.iter().any(|c| cell_task(sim, k, c, upkeep)))
}

pub fn gate_needs_visit(sim: &SimState, id: BuildingId) -> bool {
    canal(sim, id).is_some_and(|k| k.gate.built && k.gate.pending.is_some())
}

pub fn has_work(sim: &SimState, id: BuildingId) -> bool {
    canal(sim, id).is_some_and(|k| k.gate.built && k.gate.pending.is_some() || k.cells.iter().any(|c| cell_work(sim, k, c)))
}

/// Visits still to make: digging, silt clearing and a pending gate change.
pub fn visits_left(sim: &SimState, id: BuildingId) -> usize {
    let Some(k) = canal(sim, id) else { return 0 };
    let dig: usize = k.cells.iter().filter(|c| cell_work(sim, k, c)).map(|c| visits_for(ground(sim, c.cell), c.target).max(1)).sum();
    dig + usize::from(k.gate.built && k.gate.pending.is_some())
}

/// Cells other diggers are already heading for.
pub fn claimed(sim: &SimState, id: BuildingId, except: crate::colony::BeaverId) -> Vec<Cell> {
    sim.agents
        .by_id
        .values()
        .filter(|a| a.id != except)
        .filter_map(|a| match a.task {
            crate::agents::Task::Dig { site, cell, .. } | crate::agents::Task::Gate { site, cell, .. } if site == id => Some(cell),
            _ => None,
        })
        .collect()
}

/// What a digger should do next: a pending gate change first, then the undug cell farthest
/// from the intake (so the canal is dug dry and the gate cell, which holds the water back,
/// comes last), then silt clearing.
pub fn next_job(sim: &SimState, id: BuildingId, taken: &[Cell], upkeep: bool) -> Option<(Cell, bool)> {
    let k = canal(sim, id)?;
    if !upkeep && k.gate.built && k.gate.pending.is_some() && !taken.contains(&k.gate.cell) {
        return Some((k.gate.cell, true));
    }
    let gate = k.gate.cell;
    let open: Vec<&CanalCell> = k.cells.iter().filter(|c| !taken.contains(&c.cell) && cell_task(sim, k, c, upkeep)).collect();
    let fresh = open.iter().filter(|c| !c.done && c.cell != gate).max_by_key(|c| (c.row == 0, k.cells.iter().position(|o| o.cell == c.cell)));
    let pick = fresh.or_else(|| open.iter().find(|c| c.cell == gate && !c.done)).or_else(|| open.first());
    pick.map(|c| (c.cell, false))
}

/// One digging visit at `cell`: lower it a step, leave the spoil on the bank. The first
/// visit to the gate cell builds the gate (its sticks are used then), so water stays out.
pub fn dig(sim: &mut SimState, id: BuildingId, cell: Cell) -> Result<(), SimError> {
    let k = canal(sim, id).ok_or_else(|| invalid("no such canal"))?;
    let c = *k.cells.iter().find(|c| c.cell == cell).ok_or_else(|| invalid("not a cell of this canal"))?;
    let gate = k.gate;
    let h = ground(sim, cell);
    let step = (h - c.target).min(DIG_STEP);
    if step <= EPS {
        mark_done(sim, id, cell);
        return Ok(());
    }
    if cell == gate.cell && !gate.built {
        if !crate::gates::take_sticks(sim, cell, GATE_STICKS) {
            return Err(invalid("the gate needs sticks"));
        }
        canal_mut(sim, id)?.gate.built = true;
    }
    fell_trees_at(sim, cell);
    sim.water.dig_canal(&mut sim.height, &[cell], step).map_err(|e| invalid(e.to_string()))?;
    spoil(sim, cell, CLAY_PER_STEP * step / DIG_STEP);
    if cell == gate.cell {
        let g = canal(sim, id).expect("canal").gate;
        crate::gates::apply(sim, &g)?;
    }
    if ground(sim, cell) <= c.target + EPS {
        mark_done(sim, id, cell);
    }
    Ok(())
}

fn mark_done(sim: &mut SimState, id: BuildingId, cell: Cell) {
    if let Ok(k) = canal_mut(sim, id) {
        for c in k.cells.iter_mut().filter(|c| c.cell == cell) {
            c.done = true;
            c.silt = 0.0;
        }
    }
}

/// Make a pending gate change: one beaver visit.
pub fn turn_gate(sim: &mut SimState, id: BuildingId) -> Result<(), SimError> {
    let k = canal_mut(sim, id)?;
    if let Some(s) = k.gate.pending.take() {
        k.gate.setting = s;
    }
    let g = k.gate;
    crate::gates::apply(sim, &g)
}

fn fell_trees_at(sim: &mut SimState, cell: Cell) {
    let trees: Vec<_> = sim.resources.at(cell).filter(|r| r.kind == ResourceKind::Tree).map(|r| r.id).collect();
    for t in trees {
        if sim.resources.fell(t, f32::MAX / 4.0).is_err() {
            if let Some(r) = sim.resources.get(t) { crate::resources::note("GONE", r, format_args!("why=cleared for a canal")); }
            sim.resources.keep(|r| r.id != t);
        }
    }
}

/// Spoil goes onto the drier bank as clay that gatherers bring in.
fn spoil(sim: &mut SimState, cell: Cell, clay: f32) {
    let n = sim.height.width() as i64;
    let mut best: Option<(f32, Cell)> = None;
    for dy in -1i64..=1 {
        for dx in -1i64..=1 {
            let (x, y) = (cell.0 as i64 + dx, cell.1 as i64 + dy);
            if (dx, dy) == (0, 0) || x < 0 || y < 0 || x >= n || y >= n {
                continue;
            }
            let c = (x as usize, y as usize);
            if is_water(sim, c) || sim.water.canal.get(c.0, c.1).copied().unwrap_or(false) || owned_by_building(sim, c, None) {
                continue;
            }
            let h = ground(sim, c);
            if best.is_none_or(|b| h > b.0) {
                best = Some((h, c));
            }
        }
    }
    let Some((_, bank)) = best else { return };
    let near = sim.resources.items().iter().find(|r| r.kind == ResourceKind::MudBank && r.cell.0.abs_diff(bank.0) <= 1 && r.cell.1.abs_diff(bank.1) <= 1).map(|r| r.id);
    match near.and_then(|id| sim.resources.get_mut(id).ok()) {
        Some(r) => {
            r.amount += clay;
            r.max_amount += clay;
        }
        None => {
            sim.resources.add(ResourceKind::MudBank, bank, clay);
        }
    }
}

/// Grow a canal: Deepen picked cells (or all), Widen along picked cells (or all), or Extend
/// along cells drawn from its free end.
pub fn grow(sim: &mut SimState, id: BuildingId, growth: CanalGrowth, cells: &[Cell]) -> Result<(), SimError> {
    let new = plan_growth(sim, id, growth, cells).map_err(invalid)?;
    let k = canal_mut(sim, id)?;
    k.cells = new;
    if let Some(first) = k.cells.iter().find(|c| c.cell == k.gate.cell) {
        k.gate.floor = first.target;
    }
    Ok(())
}

/// Every cell of the canal after a growth, or why it can't grow that way.
pub fn plan_growth(sim: &SimState, id: BuildingId, growth: CanalGrowth, cells: &[Cell]) -> Result<Vec<CanalCell>, String> {
    let k = canal(sim, id).ok_or("no such canal")?;
    let mut out = k.cells.clone();
    match growth {
        CanalGrowth::Deepen => {
            let mut any = false;
            for c in out.iter_mut().filter(|c| cells.is_empty() || cells.contains(&c.cell)) {
                let t = c.target - DIG_STEP;
                if c.ground - t <= MAX_CUT + EPS {
                    c.target = t;
                    c.done = false;
                    any = true;
                }
            }
            if !any {
                return Err(format!("Already {MAX_CUT} m deep"));
            }
        }
        CanalGrowth::Widen => {
            if k.width() as usize >= MAX_WIDTH {
                return Err("Already as wide as it goes".into());
            }
            let route: Vec<CanalCell> = k.route().copied().collect();
            let along: Vec<&CanalCell> = route.iter().filter(|c| cells.is_empty() || cells.contains(&c.cell)).collect();
            // One side for the whole row: the lower bank, which needs less digging.
            let side = |sign: i64| -> Vec<(CanalCell, Cell)> {
                along.iter().filter_map(|c| {
                    let i = route.iter().position(|o| o.cell == c.cell)?;
                    let a = route[i.saturating_sub(1)].cell;
                    let b = route[(i + 1).min(route.len() - 1)].cell;
                    let (dx, dy) = (b.0 as i64 - a.0 as i64, b.1 as i64 - a.1 as i64);
                    let (px, py) = if dx.abs() >= dy.abs() { (0, dx.signum() * sign) } else { (-dy.signum() * sign, 0) };
                    let (x, y) = (c.cell.0 as i64 + px, c.cell.1 as i64 + py);
                    (x >= 0 && y >= 0).then_some((**c, (x as usize, y as usize)))
                }).collect()
            };
            let usable = |v: &Vec<(CanalCell, Cell)>| v.iter().filter(|(_, n)| !out.iter().any(|o| o.cell == *n) && !is_water(sim, *n) && !owned_by_building(sim, *n, Some(id)) && n.0 < sim.height.width() && n.1 < sim.height.width()).map(|&(c, n)| (c, n)).collect::<Vec<_>>();
            let (left, right) = (usable(&side(1)), usable(&side(-1)));
            let cost = |v: &Vec<(CanalCell, Cell)>| v.iter().map(|(c, n)| ground(sim, *n) - c.target).sum::<f32>();
            let pick = if left.len() > right.len() || (left.len() == right.len() && cost(&left) <= cost(&right)) { left } else { right };
            if pick.is_empty() {
                return Err("No room beside the canal".into());
            }
            for (c, n) in pick {
                if out.iter().any(|o| o.cell == n) {
                    continue;
                }
                let g = ground(sim, n);
                let target = c.target.max(g - MAX_CUT);
                out.push(CanalCell { cell: n, target, ground: g, row: 1, silt: 0.0, done: g <= target + EPS });
            }
        }
        CanalGrowth::Extend => {
            let end = k.end();
            let drawn: Vec<Cell> = cells.iter().copied().filter(|&c| c != end).collect();
            let p = plan_from(sim, &drawn, Some(id))?;
            check(&p)?;
            if out.len() + p.cells.len() > MAX_CELLS * MAX_WIDTH {
                return Err(format!("Too long (max {MAX_CELLS} cells)"));
            }
            out.extend(p.cells);
        }
    }
    Ok(out)
}

/// Labour a growth adds, in beaver-days.
pub fn growth_labour(sim: &SimState, id: BuildingId, growth: CanalGrowth, cells: &[Cell]) -> Result<f32, String> {
    let before: usize = canal(sim, id).map(|k| k.cells.iter().map(|c| visits_for(ground(sim, c.cell), c.target)).sum()).unwrap_or(0);
    let after: usize = plan_growth(sim, id, growth, cells)?.iter().map(|c| visits_for(ground(sim, c.cell), c.target)).sum();
    Ok(after.saturating_sub(before) as f32 * DIG_WORK)
}

pub fn set_maintain(sim: &mut SimState, id: BuildingId, on: bool) -> Result<(), SimError> {
    canal_mut(sim, id)?.maintain = on;
    Ok(())
}

pub fn set_gate(sim: &mut SimState, id: BuildingId, setting: GateSetting) -> Result<(), SimError> {
    let k = canal_mut(sim, id)?;
    crate::gates::request(&mut k.gate, setting)
}

/// A canal with nothing dug can be withdrawn. A dug canal stays in the ground; its gate is
/// left standing as an ordinary dam cell that wears away.
pub fn remove(sim: &mut SimState, id: BuildingId) -> Result<(), SimError> {
    let k = canal(sim, id).ok_or_else(|| invalid("no such canal"))?;
    let dug = k.cells.iter().any(|c| ground(sim, c.cell) < c.ground - EPS) || k.gate.built;
    if dug {
        return Err(invalid("part of this canal is already dug"));
    }
    Ok(())
}

/// Silt settles in dug cells; once a day the canal reads its water.
pub fn step(sim: &mut SimState, dt: f32) {
    let today = sim.time_days.floor();
    let ids: Vec<BuildingId> = sim.buildings.buildings.iter().filter(|b| b.canal.is_some()).map(|b| b.id).collect();
    for id in ids {
        silt(sim, id, dt);
        let Some(k) = canal(sim, id) else { continue };
        if k.status_day >= today {
            continue;
        }
        let status = measure(sim, k);
        let new_day = k.status_day.is_finite();
        let Ok(k) = canal_mut(sim, id) else { continue };
        if new_day {
            k.history.push_back(CanalDay { floatable: status.floatable_share(), inflow: status.inflow, outflow: status.outflow, silted: status.silted as f32 });
            while k.history.len() > HISTORY_DAYS {
                k.history.pop_front();
            }
        }
        k.status = Some(status);
        k.status_day = today;
    }
}

fn silt(sim: &mut SimState, id: BuildingId, dt: f32) {
    let Some(k) = canal(sim, id) else { return };
    let mut fills = Vec::new();
    let mut silts = Vec::with_capacity(k.cells.len());
    for c in &k.cells {
        let h = ground(sim, c.cell);
        if h >= c.ground - EPS {
            silts.push(c.silt);
            continue;
        }
        let (u, v) = sim.water.velocity.get(c.cell.0, c.cell.1).copied().unwrap_or((0.0, 0.0));
        let running = depth(sim, c.cell) > WET_DEPTH && u.hypot(v) > 0.02;
        let mut s = c.silt + SILT_PER_DAY * dt * if running { SILT_RUNNING_FACTOR } else { 1.0 };
        if s >= SILT_STEP {
            fills.push((c.cell, SILT_STEP, c.ground));
            s -= SILT_STEP;
        }
        silts.push(s);
    }
    let gate = k.gate;
    if let Ok(k) = canal_mut(sim, id) {
        for (c, s) in k.cells.iter_mut().zip(silts) {
            c.silt = s;
        }
    }
    let mut regate = false;
    for (cell, amount, g) in fills {
        let _ = sim.water.silt_canal(&mut sim.height, cell, amount, g);
        regate |= cell == gate.cell;
    }
    if regate {
        let _ = crate::gates::apply(sim, &gate);
    }
}

fn measure(sim: &SimState, k: &Canal) -> CanalStatus {
    let mut s = CanalStatus { cells: k.cells.len(), source_level: ground(sim, k.source) + depth(sim, k.source), ..Default::default() };
    let mut total = 0.0;
    for c in &k.cells {
        let d = depth(sim, c.cell);
        let h = ground(sim, c.cell);
        if h < c.ground - EPS {
            s.dug += 1;
        }
        if c.done && h > c.target + SILT_TOLERANCE {
            s.silted += 1;
        }
        if d >= WET_DEPTH {
            s.wet += 1;
            total += d;
        }
        if d >= FLOAT_DEPTH {
            s.floatable += 1;
        }
        s.max_depth = s.max_depth.max(d);
    }
    s.mean_depth = if s.wet > 0 { total / s.wet as f32 } else { 0.0 };
    let q = |c: Cell| sim.water.discharge.get(c.0, c.1).copied().unwrap_or(0.0) * 86_400.0;
    s.inflow = q(k.gate.cell);
    s.outflow = q(k.end());
    s
}

/// Mean condition: the share of dug cells not silted, 1 before digging starts.
pub fn health(sim: &SimState, k: &Canal) -> f32 {
    let done: Vec<&CanalCell> = k.cells.iter().filter(|c| c.done).collect();
    if done.is_empty() {
        return 1.0;
    }
    let ok = done.iter().filter(|c| ground(sim, c.cell) <= c.target + SILT_TOLERANCE).count();
    ok as f32 / done.len() as f32
}

/// Beavers working on this canal.
pub fn crew(sim: &SimState, id: BuildingId) -> usize {
    sim.agents.by_id.values().filter(|a| matches!(a.task, crate::agents::Task::Dig { site, .. } | crate::agents::Task::Gate { site, .. } if site == id)).count()
}

#[cfg(test)]
mod tests;
