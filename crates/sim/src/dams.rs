//! Dams: single buildings grown one cell at a time, with pond measurement and gates.
include!(concat!(env!("OUT_DIR"), "/dams_constants.rs"));
use std::collections::{BTreeSet, HashSet, VecDeque};

use crate::agents::{Dest, Task, WorkKind};
use crate::buildings::{Building, BuildingId, BuildingKind, Good, Status};
use crate::colony::BeaverId;
use crate::gates::{apply_across, cell_taken, claimed_work, depth, finish_gate, ground, invalid, is_water, push_history, surface, Gate, GateSetting};
use crate::water::Cell;
use crate::{SimError, SimState};

const EPS: f32 = 1e-3;

#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct DamCell { pub cell: Cell, pub target: f32, pub row: u8 }

#[derive(Clone, Copy, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct DamDay { pub level: f32, pub inflow: f32, pub outflow: f32 }

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Spill { None, Over(Cell), Around(Cell), Gate(Cell) }

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Pond {
    pub level: f32, pub full_level: f32, pub bottom: f32, pub area: usize, pub volume: f32,
    pub max_depth: f32, pub inflow: f32, pub outflow: f32, pub days_to_full: f32,
    pub spill: Spill, pub flooded: Vec<Cell>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Dam {
    pub cells: Vec<DamCell>,
    pub rows: u8,
    pub upkeep: bool,
    pub gates: Vec<Gate>,
    pub downstream: (i32, i32),
    pub history: VecDeque<DamDay>,
    pub pond: Option<Pond>,
    pond_day: f64,
}

impl Dam {
    pub fn cap(&self) -> f32 { (self.rows as f32 * HEIGHT_PER_ROW).min(crate::dam_preview::MAX_DAM_HEIGHT) }
    pub fn line(&self) -> impl Iterator<Item = &DamCell> { in_order(self.cells.iter().filter(|c| c.row == 0)) }
}

pub fn material() -> Good { BuildingKind::dam().spec().cost.first().map(|c| c.0).expect("dam material") }
pub fn sticks_per_step() -> f32 { BuildingKind::dam().spec().cost.iter().filter(|c| c.0 == material()).map(|c| c.1).sum::<f32>().max(1.0) }
pub fn pack_labour() -> f32 { BuildingKind::dam().spec().work }
pub fn built(sim: &SimState, c: Cell) -> f32 { sim.water.dam_height.get(c.0, c.1).copied().unwrap_or(0.0) }
pub fn integrity(sim: &SimState, c: Cell) -> f32 { if built(sim, c) <= 0.0 { 0.0 } else { sim.water.dam_integrity.get(c.0, c.1).copied().unwrap_or(0.0) } }
pub fn dam(sim: &SimState, id: BuildingId) -> Option<&Dam> { sim.buildings.building(id).and_then(|b| b.dam.as_deref()) }
fn dam_mut(sim: &mut SimState, id: BuildingId) -> Result<&mut Dam, SimError> {
    sim.buildings.buildings.iter_mut().find(|b| b.id == id).and_then(|b| b.dam.as_deref_mut()).ok_or_else(|| invalid("no such dam"))
}
pub fn water_for_dam(sim: &SimState, c: Cell) -> bool { is_water(sim, c) }

pub fn drop(sim: &SimState, d: &Dam) -> Option<f32> {
    let level = d.pond.as_ref()?.level;
    let n = sim.height.width();
    let on_dam: HashSet<Cell> = d.cells.iter().map(|c| c.cell).collect();
    let below: Vec<Cell> = d.line().filter_map(|c| offset(c.cell, d.downstream, d.rows as i32, n)).filter(|c| !on_dam.contains(c)).collect();
    if below.is_empty() { return None; }
    let wet: Vec<f32> = below.iter().filter(|&&c| depth(sim, c) > crate::buildings::WET_DEPTH).map(|&c| surface(sim, c)).collect();
    let tail = if wet.is_empty() { below.iter().map(|&c| ground(sim, c)).fold(f32::INFINITY, f32::min) } else { wet.iter().sum::<f32>() / wet.len() as f32 };
    Some((level - tail).max(0.0))
}

pub fn gate_cells(d: &Dam, cell: Cell) -> Vec<Cell> {
    let mut out = vec![cell];
    for k in 1..d.rows as i64 {
        let (x, y) = (cell.0 as i64 + d.downstream.0 as i64 * k, cell.1 as i64 + d.downstream.1 as i64 * k);
        if x < 0 || y < 0 { break; }
        let c = (x as usize, y as usize);
        if !d.cells.iter().any(|dc| dc.cell == c) { break; }
        out.push(c);
    }
    out
}

pub fn is_gate(d: &Dam, c: Cell) -> bool { d.gates.iter().any(|g| g.built && gate_cells(d, g.cell).contains(&c)) }

fn apply_gate(sim: &mut SimState, id: BuildingId, cell: Cell) -> Result<(), SimError> {
    let d = dam(sim, id).ok_or_else(|| invalid("no such dam"))?;
    let Some(g) = d.gates.iter().find(|g| g.cell == cell).copied() else { return Ok(()) };
    apply_across(sim, g, &gate_cells(d, cell))
}

fn cell_work(sim: &SimState, d: &Dam, c: &DamCell) -> bool {
    !is_gate(d, c.cell) && { let h = built(sim, c.cell); h < c.target - EPS || (d.upkeep && h > 0.0 && integrity(sim, c.cell) < REPAIR_BELOW) }
}
fn cell_task(sim: &SimState, d: &Dam, c: &DamCell, upkeep: bool) -> bool {
    !is_gate(d, c.cell) && {
        let h = built(sim, c.cell);
        if upkeep { d.upkeep && h > 0.0 && h >= c.target - EPS && integrity(sim, c.cell) < REPAIR_BELOW } else { h < c.target - EPS }
    }
}

pub fn cell_needs_work(sim: &SimState, id: BuildingId, cell: Cell, upkeep: bool) -> bool {
    dam(sim, id).is_some_and(|d| d.cells.iter().any(|c| c.cell == cell && cell_task(sim, d, c, upkeep)))
}
pub fn has_work_for(sim: &SimState, id: BuildingId, upkeep: bool) -> bool {
    dam(sim, id).is_some_and(|d| d.cells.iter().any(|c| cell_task(sim, d, c, upkeep)) || (!upkeep && d.gates.iter().any(|g| !g.built || g.pending.is_some() || crate::locks::pending(sim, g.cell))))
}
pub fn has_work(sim: &SimState, id: BuildingId) -> bool {
    dam(sim, id).is_some_and(|d| d.cells.iter().any(|c| cell_work(sim, d, c)) || d.gates.iter().any(|g| !g.built || g.pending.is_some() || crate::locks::pending(sim, g.cell)))
}

fn loads_for(sim: &SimState, c: Cell, target: f32, span: f32) -> usize {
    ((target - built(sim, c)).max(0.0) / STEP_HEIGHT * span - EPS).ceil().max(0.0) as usize
}

pub fn chain(cells: &[Cell]) -> Vec<usize> {
    let touch = |a: Cell, b: Cell| a != b && a.0.abs_diff(b.0) <= 1 && a.1.abs_diff(b.1) <= 1;
    let degree = |i: usize| cells.iter().filter(|&&c| touch(c, cells[i])).count();
    let (mut left, mut out) = ((0..cells.len()).collect::<Vec<_>>(), Vec::with_capacity(cells.len()));
    while !left.is_empty() {
        let (k, _) = left.iter().enumerate().min_by_key(|&(_, &i)| (degree(i), i)).unwrap();
        let mut at = left.remove(k);
        out.push(at);
        while let Some((k, _)) = left.iter().enumerate().filter(|&(_, &j)| touch(cells[at], cells[j]))
            .min_by_key(|&(_, &j)| (cells[at].0.abs_diff(cells[j].0) + cells[at].1.abs_diff(cells[j].1), j))
        {
            at = left.remove(k);
            out.push(at);
        }
    }
    out
}

pub fn in_order<'a>(cells: impl Iterator<Item = &'a DamCell>) -> impl Iterator<Item = &'a DamCell> {
    let cells: Vec<&DamCell> = cells.collect();
    let at: Vec<Cell> = cells.iter().map(|c| c.cell).collect();
    chain(&at).into_iter().map(move |i| cells[i])
}

pub fn spans(cells: &[Cell]) -> Vec<f32> {
    let order = chain(cells);
    let mut out = vec![1.0; cells.len()];
    let dist = |a: Cell, b: Cell| ((a.0 as f32 - b.0 as f32).powi(2) + (a.1 as f32 - b.1 as f32).powi(2)).sqrt();
    for (k, &i) in order.iter().enumerate() {
        let c = cells[i];
        let sides: Vec<f32> = [k.checked_sub(1), (k + 1 < order.len()).then_some(k + 1)].into_iter().flatten()
            .map(|j| cells[order[j]]).filter(|&o| c.0.abs_diff(o.0) <= 1 && c.1.abs_diff(o.1) <= 1).map(|o| dist(c, o)).collect();
        if !sides.is_empty() { out[i] = sides.iter().sum::<f32>() / sides.len() as f32; }
    }
    out
}

fn span_in(cells: &[DamCell], cell: Cell) -> f32 {
    let Some(row) = cells.iter().find(|c| c.cell == cell).map(|c| c.row) else { return 1.0 };
    let line: Vec<Cell> = cells.iter().filter(|c| c.row == row).map(|c| c.cell).collect();
    line.iter().position(|&c| c == cell).map_or(1.0, |i| spans(&line)[i])
}

fn row_spans(cells: &[DamCell]) -> Vec<f32> {
    let mut out = vec![1.0; cells.len()];
    for row in cells.iter().map(|c| c.row).collect::<BTreeSet<_>>() {
        let idx: Vec<usize> = (0..cells.len()).filter(|&i| cells[i].row == row).collect();
        let line: Vec<Cell> = idx.iter().map(|&i| cells[i].cell).collect();
        for (k, w) in spans(&line).into_iter().enumerate() { out[idx[k]] = w; }
    }
    out
}

pub fn loads_left(sim: &SimState, id: BuildingId) -> usize {
    let Some(d) = dam(sim, id) else { return 0 };
    d.cells.iter().zip(row_spans(&d.cells)).filter(|(c, _)| !is_gate(d, c.cell)).map(|(c, span)| {
        let raise = loads_for(sim, c.cell, c.target, span);
        if raise == 0 && d.upkeep && built(sim, c.cell) > 0.0 && integrity(sim, c.cell) < REPAIR_BELOW { 1 } else { raise }
    }).sum()
}

pub fn claimed(sim: &SimState, id: BuildingId, except: BeaverId) -> Vec<Cell> { claimed_work(sim, id, except, &[WorkKind::Dam]) }
pub fn gates_claimed(sim: &SimState, id: BuildingId, except: BeaverId) -> Vec<Cell> { claimed_work(sim, id, except, &[WorkKind::Gate]) }

pub fn next_cell(sim: &SimState, id: BuildingId, from: Cell, taken: &[Cell], upkeep: bool) -> Option<Cell> {
    let d = dam(sim, id)?;
    let dist = |c: Cell| c.0.abs_diff(from.0) + c.1.abs_diff(from.1);
    d.cells.iter().filter(|c| cell_task(sim, d, c, upkeep) && !taken.contains(&c.cell) && !crate::predators::in_view(sim, c.cell))
        .min_by_key(|c| {
            let tier = if upkeep { (integrity(sim, c.cell) * 20.0) as i32 }
                else { (built(sim, c.cell) * integrity(sim, c.cell).max(0.01) / STEP_HEIGHT).floor() as i32 };
            (tier, dist(c.cell), c.cell)
        })
        .map(|c| c.cell)
}

pub(crate) fn pack(sim: &mut SimState, id: BuildingId, cell: Cell, sticks: f32) -> f32 {
    let Some((target, rows, span)) = dam(sim, id).and_then(|d| d.cells.iter().find(|c| c.cell == cell).map(|c| (c.target, d.rows, span_in(&d.cells, cell)))) else { return 0.0 };
    let h = built(sim, cell);
    let share = (sticks / sticks_per_step()).clamp(0.0, 1.0);
    let next = if h < target - EPS { (h + STEP_HEIGHT * share / span.max(1.0)).min(target) } else { h };
    if next <= 0.0 || sim.water.raise_dam(&[(cell, next)]).is_err() { return 0.0; }
    let dur = rows as f32 * facing(sim, id);
    if let Some(v) = sim.water.dam_durability.get_mut(cell.0, cell.1) { *v = dur; }
    sim.buildings.stores.book_site_delivery(material(), sticks, None);
    if let Ok(d) = dam_mut(sim, id) { d.pond_day = f64::NEG_INFINITY; }
    sticks
}

fn offset(c: Cell, d: (i32, i32), k: i32, n: usize) -> Option<Cell> {
    let (x, y) = (c.0 as i64 + (d.0 * k) as i64, c.1 as i64 + (d.1 * k) as i64);
    (x >= 0 && y >= 0 && (x as usize) < n && (y as usize) < n).then_some((x as usize, y as usize))
}

fn downstream(sim: &SimState, cells: &[Cell]) -> (i32, i32) {
    let n = sim.height.width();
    [(1, 0), (-1, 0), (0, 1), (0, -1)].into_iter().max_by(|&a, &b| {
        let off = |c: Cell, d, k| offset(c, d, k, n).filter(|o| !cells.contains(o));
        let score = |d| -> f32 { cells.iter().filter_map(|&c| Some(surface(sim, off(c, d, -1)?) - surface(sim, off(c, d, 1)?))).sum() };
        score(a).total_cmp(&score(b))
    }).unwrap_or((0, 1))
}

pub fn line_crosses_water(sim: &SimState, cells: &[Cell]) -> bool {
    cells.iter().any(|&c| is_water(sim, c))
        || cells.iter().any(|&a| cells.iter().any(|&b| {
            a.0.abs_diff(b.0) == 1 && a.1.abs_diff(b.1) == 1 && [(a.0, b.1), (b.0, a.1)].into_iter().any(|k| !cells.contains(&k) && is_water(sim, k))
        }))
}

pub fn check_line(sim: &SimState, cells: &[Cell]) -> Result<(), String> {
    if cells.is_empty() { return Err("draw a line across water".into()); }
    let mut seen = HashSet::new();
    let conn = cells.windows(2).all(|w| w[0].0.abs_diff(w[1].0).max(w[0].1.abs_diff(w[1].1)) == 1);
    if !cells.iter().all(|c| seen.insert(*c)) || !conn { return Err("the line must be one unbroken run of cells".into()); }
    if cells.iter().any(|&(x, y)| !sim.water.depth.in_bounds(x, y)) { return Err("off the map".into()); }
    if !sim.sandbox && cells.iter().any(|&c| !sim.fog.explored(c)) { return Err("Unexplored".into()); }
    if sim.work.supplies.is_some_and(|p| cells.contains(&p)) { return Err("the supply pile is there".into()); }
    if cells.iter().any(|&c| cell_taken(sim, c, None, false)) { return Err("something is already built there".into()); }
    if !sim.sandbox && !line_crosses_water(sim, cells) { return Err("a dam must cross water".into()); }
    Ok(())
}

pub fn first_targets(sim: &SimState, cells: &[Cell]) -> Vec<f32> {
    let crest = cells.iter().map(|&c| ground(sim, c)).fold(f32::INFINITY, f32::min) + DEFAULT_TARGET;
    cells.iter().map(|&c| (crest - ground(sim, c)).clamp(MIN_WALL, HEIGHT_PER_ROW)).collect()
}

pub fn place(sim: &mut SimState, cells: &[Cell]) -> Result<BuildingId, SimError> {
    sim.unlocked(BuildingKind::dam()).map_err(|e| invalid(e.to_string()))?;
    check_line(sim, cells).map_err(invalid)?;
    let id = BuildingId(sim.buildings.fresh_id());
    let dam = Dam {
        cells: cells.iter().zip(first_targets(sim, cells)).map(|(&cell, target)| DamCell { cell, target, row: 0 }).collect(),
        rows: 1, upkeep: true, gates: Vec::new(), downstream: downstream(sim, cells),
        history: VecDeque::new(), pond: None, pond_day: f64::NEG_INFINITY,
    };
    let mut b = Building::new(id, BuildingKind::dam(), cells[0], Status::Complete);
    b.dam = Some(Box::new(dam));
    sim.work.add_complete(&b).map_err(|e| invalid(format!("{e:?}")))?;
    sim.buildings.buildings.push(b);
    Ok(id)
}

pub struct Plan { pub cells: Vec<DamCell>, pub rows: u8 }
impl Plan {
    pub fn added(&self, d: &Dam) -> Vec<Cell> {
        self.cells.iter().filter(|c| !d.cells.iter().any(|o| o.cell == c.cell)).map(|c| c.cell).collect()
    }
}

pub fn plan_cost(sim: &SimState, d: &Dam, plan: &Plan) -> f32 {
    let loads = |cells: &[DamCell]| cells.iter().zip(row_spans(cells)).map(|(c, w)| loads_for(sim, c.cell, c.target, w)).sum::<usize>();
    (loads(&plan.cells).saturating_sub(loads(&d.cells))) as f32 * sticks_per_step()
}

fn crest(sim: &SimState, d: &Dam) -> f32 { d.line().map(|c| ground(sim, c.cell) + c.target).fold(f32::NEG_INFINITY, f32::max) }

pub fn plan_raise(sim: &SimState, id: BuildingId, only: &[Cell]) -> Result<Plan, String> {
    let d = dam(sim, id).ok_or("no such dam")?;
    let (cap, new_crest) = (d.cap(), crest(sim, d) + STEP_HEIGHT);
    let (mut cells, mut changed) = (d.cells.clone(), false);
    for c in cells.iter_mut().filter(|c| only.is_empty() || only.contains(&c.cell)) {
        let t = (if only.is_empty() { (new_crest - ground(sim, c.cell)).max(c.target) } else { c.target + STEP_HEIGHT }).clamp(0.0, cap);
        if t > c.target + EPS { c.target = t; changed = true; }
    }
    if !changed { return Err("at its height limit: widen it first".into()); }
    Ok(Plan { cells, rows: d.rows })
}

pub fn plan_widen(sim: &SimState, id: BuildingId) -> Result<Plan, String> {
    let d = dam(sim, id).ok_or("no such dam")?;
    if d.rows as usize >= MAX_ROWS { return Err("already as wide as a dam gets".into()); }
    let (n, mut cells) = (sim.height.width(), d.cells.clone());
    for c in d.line() {
        let Some(at) = offset(c.cell, d.downstream, d.rows as i32, n) else { continue };
        if !cells.iter().any(|x| x.cell == at) && !cell_taken(sim, at, Some(id), false) {
            cells.push(DamCell { cell: at, target: c.target.max(MIN_WALL), row: d.rows });
        }
    }
    if cells.len() == d.cells.len() { return Err("no room downstream to widen".into()); }
    Ok(Plan { cells, rows: d.rows + 1 })
}

pub fn plan_extend(sim: &SimState, id: BuildingId) -> Result<Plan, String> {
    let d = dam(sim, id).ok_or("no such dam")?;
    let (top, mut cells) = (crest(sim, d), d.cells.clone());
    for _ in 0..EXTEND_CELLS {
        let row: Vec<&DamCell> = in_order(cells.iter()).collect();
        let line: Vec<Cell> = row.iter().map(|c| c.cell).collect();
        let heights: Vec<f32> = row.iter().map(|c| c.target.max(built(sim, c.cell))).collect();
        let Some(at) = crate::dam_preview::preview_heights(sim, &line, &heights).and_then(|p| p.spill_at) else { break };
        let near = line.iter().any(|c| c.0.abs_diff(at.0).max(c.1.abs_diff(at.1)) == 1);
        let h = (top - ground(sim, at)).clamp(0.0, d.cap());
        if line.contains(&at) || !near || h <= EPS || cell_taken(sim, at, Some(id), false) { break; }
        cells.push(DamCell { cell: at, target: h, row: 0 });
    }
    if cells.len() == d.cells.len() { return Err("water is not getting around the ends".into()); }
    Ok(Plan { cells, rows: d.rows })
}

pub fn plan_extend_to(sim: &SimState, id: BuildingId, drawn: &[Cell]) -> Result<Plan, String> {
    let d = dam(sim, id).ok_or("no such dam")?;
    let (top, mut cells) = (crest(sim, d), d.cells.clone());
    for &at in drawn {
        if cells.iter().any(|c| c.cell == at) { continue; }
        if !cells.iter().any(|c| c.row == 0 && c.cell.0.abs_diff(at.0).max(c.cell.1.abs_diff(at.1)) == 1) { return Err("the new cells must run on from an end".into()); }
        if cell_taken(sim, at, Some(id), false) { return Err("something is already built there".into()); }
        cells.push(DamCell { cell: at, target: (top - ground(sim, at)).clamp(MIN_WALL, d.cap().max(MIN_WALL)), row: 0 });
    }
    if cells.len() == d.cells.len() { return Err("draw past an end to extend".into()); }
    Ok(Plan { cells, rows: d.rows })
}

pub fn apply_plan(sim: &mut SimState, id: BuildingId, plan: Plan) -> Result<(), SimError> {
    let d = dam_mut(sim, id)?;
    (d.cells, d.rows, d.pond_day) = (plan.cells, plan.rows, f64::NEG_INFINITY);
    let tops: Vec<(Cell, f32)> = d.cells.iter().filter(|c| c.row == 0).map(|c| (c.cell, c.target)).collect();
    let mut gates = Vec::new();
    for g in &mut d.gates {
        if let Some(&(_, t)) = tops.iter().find(|t| t.0 == g.cell) { g.top = (g.floor + t).max(g.top); }
        gates.push(g.cell);
    }
    refresh_durability(sim, id);
    for c in gates { apply_gate(sim, id, c)?; }
    Ok(())
}

fn facing(sim: &SimState, id: BuildingId) -> f32 {
    if sim.buildings.building(id).is_some_and(|b| b.pebbled) { crate::maintenance::lasts(BuildingKind::dam(), crate::maintenance::Refit::Facing) } else { 1.0 }
}

pub(crate) fn refresh_durability(sim: &mut SimState, id: BuildingId) {
    let Some(d) = dam(sim, id) else { return };
    let rows = d.rows as f32 * facing(sim, id);
    for c in d.cells.iter().map(|c| c.cell).collect::<Vec<_>>() {
        if let Some(v) = sim.water.dam_durability.get_mut(c.0, c.1) { *v = rows; }
    }
}

pub fn set_upkeep(sim: &mut SimState, id: BuildingId, on: bool) -> Result<(), SimError> { dam_mut(sim, id)?.upkeep = on; Ok(()) }

fn live_pond(sim: &SimState, d: &Dam) -> Option<(f32, Vec<Cell>)> {
    let n = sim.height.width();
    let on_dam: HashSet<Cell> = d.cells.iter().map(|c| c.cell).collect();
    let starts: Vec<Cell> = d.line()
        .filter_map(|c| offset(c.cell, (-d.downstream.0, -d.downstream.1), 1, n))
        .filter(|c| !on_dam.contains(c) && depth(sim, *c) > crate::buildings::WET_DEPTH).collect();
    let face = starts.iter().map(|&c| surface(sim, c)).fold(f32::NEG_INFINITY, f32::max);
    if !face.is_finite() { return None; }
    let (mut seen, mut stack, mut cells) = (starts.iter().copied().collect::<HashSet<_>>(), starts, Vec::new());
    while let Some(c) = stack.pop() {
        cells.push(c);
        if cells.len() > 40_000 { break; }
        for dir in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
            let Some(m) = offset(c, dir, 1, n) else { continue };
            if !on_dam.contains(&m) && !seen.contains(&m) && depth(sim, m) > crate::buildings::WET_DEPTH && surface(sim, m) >= face - 0.3 {
                seen.insert(m);
                stack.push(m);
            }
        }
    }
    Some((cells.iter().map(|&c| surface(sim, c)).sum::<f32>() / cells.len() as f32, cells))
}

fn measure(sim: &SimState, d: &Dam) -> Option<Pond> {
    let (cells, heights): (Vec<Cell>, Vec<f32>) = d.cells.iter().map(|c| (c.cell, built(sim, c.cell))).filter(|c| c.1 > 0.0 || is_gate(d, c.0)).map(|c| (c.0, c.1.max(0.01))).unzip();
    if cells.is_empty() { return None; }
    let p = { let _p = crate::prof::scope("step/dams/measure pond/still-pond estimate"); crate::dam_preview::preview_heights_kept(sim, &cells, &heights)? };
    let live = { let _p = crate::prof::scope("step/dams/measure pond/live pond"); live_pond(sim, d) };
    let wet: Vec<(Cell, f32)> = match &live {
        Some((_, cs)) => cs.iter().map(|&c| (c, depth(sim, c))).collect(),
        None => p.flooded.iter().filter_map(|&c| { let dp = depth(sim, c); (dp > crate::buildings::WET_DEPTH).then_some((c, dp)) }).collect(),
    };
    let bottom = wet.iter().map(|w| ground(sim, w.0)).chain(p.flooded.iter().map(|&c| ground(sim, c))).fold(f32::INFINITY, f32::min);
    let level = match &live { Some((l, _)) => *l, None if wet.is_empty() => bottom, None => wet.iter().map(|&(c, dp)| ground(sim, c) + dp).sum::<f32>() / wet.len() as f32 };
    let full_level = if p.level.is_finite() && p.level >= level - 0.05 { p.level } else { level.max(p.level.min(crest(sim, d))) };
    let full = level >= full_level - 0.02;

    let pond_cells: HashSet<Cell> = wet.iter().map(|w| w.0).collect();
    let touches = |c: Cell| [(0i32, 0i32), (1, 0), (-1, 0), (0, 1), (0, -1)].iter().any(|&(dx, dy)| {
        let (x, y) = (c.0 as i32 + dx, c.1 as i32 + dy);
        x >= 0 && y >= 0 && pond_cells.contains(&(x as usize, y as usize))
    });
    let mut outlet = None;
    for b in &sim.buildings.buildings {
        for g in b.dam.iter().flat_map(|x| &x.gates).filter(|g| g.built && g.sill() < level - 0.01 && touches(g.cell)) { outlet = Some(g.cell); }
        if let Some(k) = b.canal.as_deref().filter(|k| k.through && k.gate.built && k.gate.sill() < level - 0.01 && k.cells.iter().any(|c| touches(c.cell))) { outlet = Some(k.gate.cell); }
    }
    let lowest = d.line().filter(|c| !is_gate(d, c.cell) && built(sim, c.cell) > 0.0)
        .min_by(|a, b| (ground(sim, a.cell) + built(sim, a.cell)).total_cmp(&(ground(sim, b.cell) + built(sim, b.cell))));
    let over = lowest.filter(|c| level >= ground(sim, c.cell) + built(sim, c.cell) - 0.03).map(|c| c.cell);
    let agrees = (p.level - level).abs() < 0.15 || live.is_none();
    let spill = if let Some(c) = outlet { Spill::Gate(c) }
        else if let Some(c) = over { Spill::Over(c) }
        else { match p.spill_at { Some(c) if agrees && !cells.contains(&c) => Spill::Around(c), Some(c) if agrees => Spill::Over(c), _ => Spill::None } };
    Some(Pond {
        level, full_level, bottom: if bottom.is_finite() { bottom } else { level },
        area: wet.len(), volume: wet.iter().map(|w| w.1).sum(), max_depth: wet.iter().map(|w| w.1).fold(0.0, f32::max),
        inflow: p.inflow, outflow: (if full { p.inflow.max(p.pond_loss) } else { p.pond_loss }).max(0.0),
        days_to_full: if full { 0.0 } else { p.fill_days }, spill, flooded: p.flooded,
    })
}

pub fn step(sim: &mut SimState) {
    let today = sim.time_days.floor();
    for id in sim.buildings.buildings.iter().filter(|b| b.dam.is_some()).map(|b| b.id).collect::<Vec<_>>() {
        let Some(d) = dam(sim, id) else { continue };
        if d.pond_day >= today { continue; }
        if d.cells.iter().any(|c| c.target < MIN_WALL && !is_gate(d, c.cell)) {
            if let Ok(d) = dam_mut(sim, id) {
                let gates: Vec<Cell> = d.gates.iter().map(|g| g.cell).collect();
                for c in d.cells.iter_mut().filter(|c| c.target < MIN_WALL && !gates.contains(&c.cell)) { c.target = MIN_WALL; }
            }
        }
        let Some(d) = dam(sim, id) else { continue };
        let new_day = (d.pond_day.is_finite() && d.pond_day < today) || d.history.is_empty();
        let pond = { let _p = crate::prof::scope("step/dams/measure pond"); measure(sim, d) };
        let Ok(d) = dam_mut(sim, id) else { continue };
        if new_day && let Some(p) = &pond { push_history(&mut d.history, HISTORY_DAYS, DamDay { level: p.level, inflow: p.inflow, outflow: p.outflow }); }
        (d.pond, d.pond_day) = (pond, today);
    }
}

pub struct DamWear { pub age: f32, pub overtop: f32, pub faced: f32, pub overtopped: usize }

pub fn wear(sim: &SimState, d: &Dam) -> Option<DamWear> {
    let n = sim.height.width();
    let cells: Vec<Cell> = d.cells.iter().map(|c| c.cell).filter(|&c| built(sim, c) > 0.0 && !is_gate(d, c)).collect();
    if cells.is_empty() { return None; }
    let (mut age, mut overtop, mut faced, mut overtopped) = (0.0, 0.0, 0.0, 0);
    for c in &cells {
        let (a, o) = sim.water.dam_cell_wear(&sim.height, c.1 * n + c.0);
        (age, overtop, overtopped) = (age + a, overtop + o, overtopped + usize::from(o > 0.0));
        faced += sim.water.dam_durability.get(c.0, c.1).copied().unwrap_or(1.0).max(1.0);
    }
    let k = cells.len() as f32;
    Some(DamWear { age: age / k, overtop: overtop / k, faced: faced / k, overtopped })
}

pub fn health(sim: &SimState, d: &Dam) -> f32 {
    let b: Vec<f32> = d.cells.iter().filter(|c| built(sim, c.cell) > 0.0 && !is_gate(d, c.cell)).map(|c| integrity(sim, c.cell)).collect();
    if b.is_empty() { 1.0 } else { b.iter().sum::<f32>() / b.len() as f32 }
}

pub fn add_gate(sim: &mut SimState, id: BuildingId, cell: Cell) -> Result<(), SimError> {
    plan_gate(sim, id, cell).map_err(invalid)?;
    let g = ground(sim, cell);
    let target = dam(sim, id).and_then(|d| d.line().find(|c| c.cell == cell)).map_or(0.0, |c| c.target);
    dam_mut(sim, id)?.gates.push(Gate::new(cell, g, g + target));
    Ok(())
}

pub fn plan_gate(sim: &SimState, id: BuildingId, cell: Cell) -> Result<f32, String> {
    let d = dam(sim, id).ok_or("no such dam")?;
    if !d.line().any(|c| c.cell == cell) { return Err("pick a cell on the dam's line".into()); }
    if d.gates.iter().any(|g| g.cell == cell) { return Err("already a gate".into()); }
    if built(sim, cell) <= 0.0 { return Err("build this cell first".into()); }
    Ok(crate::content::content().works.gate.iter().map(|c| c.1).sum())
}

pub fn set_gate(sim: &mut SimState, id: BuildingId, cell: Cell, setting: GateSetting) -> Result<(), SimError> {
    let d = dam_mut(sim, id)?;
    let g = d.gates.iter_mut().find(|g| g.cell == cell).ok_or_else(|| invalid("no gate there"))?;
    crate::gates::request(g, setting)?;
    d.pond_day = f64::NEG_INFINITY;
    Ok(())
}

pub(crate) fn drop_gate(sim: &mut SimState, id: BuildingId, cell: Cell) -> Result<(), SimError> {
    let d = dam_mut(sim, id)?;
    d.gates.retain(|g| g.cell != cell);
    d.pond_day = f64::NEG_INFINITY;
    Ok(())
}

pub fn gate_job(sim: &SimState, id: BuildingId, taken: &[Cell]) -> Option<Cell> {
    dam(sim, id)?.gates.iter().find(|g| crate::gates::visit_ready(sim, id, g) && !taken.contains(&g.cell) && !crate::predators::in_view(sim, g.cell)).map(|g| g.cell)
}

pub fn work_gate(sim: &mut SimState, id: BuildingId, cell: Cell) -> Result<(), SimError> {
    let d = dam_mut(sim, id)?;
    let g = d.gates.iter_mut().find(|g| g.cell == cell).ok_or_else(|| invalid("no gate there"))?;
    finish_gate(g);
    d.pond_day = f64::NEG_INFINITY;
    apply_gate(sim, id, cell)
}

pub fn crew(sim: &SimState, id: BuildingId) -> (usize, usize) {
    sim.agents.by_id.values().fold((0, 0), |(b, l), a| match a.task {
        Task::Work { site, kind: WorkKind::Dam, .. } if site == id => (b + 1, l + usize::from(a.load.is_some())),
        Task::Move { to: Dest::Dam(site), .. } if site == id => (b + 1, l),
        _ => (b, l),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::{Duty, Job};
    use crate::maintenance::{self, Refit};
    use crate::work::Slot;

    #[test]
    fn pebbling_a_dam_keeps_its_builders_seated() {
        let mut s = SimState::new(1);
        let id = BuildingId(s.buildings.fresh_id());
        let mut b = Building::new(id, BuildingKind::dam(), (20, 20), Status::Complete);
        b.dam = Some(Box::new(Dam {
            cells: vec![DamCell { cell: (20, 20), target: 1.0, row: 0 }], rows: 1, upkeep: true,
            gates: Vec::new(), downstream: (1, 0), history: VecDeque::new(), pond: None, pond_day: f64::NEG_INFINITY,
        }));
        s.work.add_complete(&b).expect("workplace");
        s.buildings.buildings.push(b);
        let builder = Job::with_duty(Duty::Build);
        let p = s.work.place_mut(id).expect("place");
        p.slots.extend([Slot { role: builder, beaver: Some(BeaverId(901)), off: false }, Slot { role: builder, beaver: Some(BeaverId(902)), off: false }]);
        maintenance::order(&mut s, id, Refit::Facing).expect("facing ordered");
        let crew: Vec<BeaverId> = s.work.crew(id).iter().filter_map(|c| c.beaver).collect();
        assert_eq!(crew, vec![BeaverId(901), BeaverId(902)]);
        assert!(s.work.place(id).is_some_and(|p| p.site.is_some()), "the dam is a site for the facing");
    }
}
