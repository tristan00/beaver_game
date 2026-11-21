//! Dams are single buildings that grow one cell at a time.
//!
//! The dam building owns its cells and their target heights. The water grid
//! holds what has actually been built, so the pond always reacts to the work
//! done so far. Builders carry one load of sticks to one cell; packing it
//! raises that cell by one step, or restores a worn cell.
include!(concat!(env!("OUT_DIR"), "/dams_constants.rs"));
use std::collections::VecDeque;

use crate::buildings::{Building, BuildingId, BuildingKind, Good, Status};
use crate::colony::BeaverId;
use crate::water::Cell;
use crate::{SimError, SimState};

const EPS: f32 = 1e-3;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DamCell {
    pub cell: Cell,
    /// Height above this cell's ground that the dam is meant to reach.
    pub target: f32,
    /// 0 is the drawn line; higher rows thicken the dam downstream.
    pub row: u8,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct DamDay {
    pub level: f32,
    pub inflow: f32,
    pub outflow: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Spill {
    None,
    /// Water pours over the crest at this dam cell.
    Over(Cell),
    /// Water goes around the dam at this cell.
    Around(Cell),
}

/// The pond a dam holds, refreshed once a day and after every change.
#[derive(Clone, Debug, PartialEq)]
pub struct Pond {
    pub level: f32,
    pub full_level: f32,
    pub bottom: f32,
    pub area: usize,
    pub volume: f32,
    pub max_depth: f32,
    pub inflow: f32,
    pub outflow: f32,
    pub days_to_full: f32,
    pub spill: Spill,
    pub flooded: Vec<Cell>,
}

#[derive(Clone, Debug)]
pub struct Dam {
    pub cells: Vec<DamCell>,
    pub rows: u8,
    pub maintain: bool,
    pub downstream: (i32, i32),
    pub history: VecDeque<DamDay>,
    pub pond: Option<Pond>,
    pond_day: f64,
}

impl Dam {
    /// Highest a cell may stand: each row of thickness carries one step of height limit.
    pub fn cap(&self) -> f32 {
        (self.rows as f32 * HEIGHT_PER_ROW).min(crate::dam_preview::MAX_DAM_HEIGHT)
    }
    pub fn line(&self) -> impl Iterator<Item = &DamCell> {
        self.cells.iter().filter(|c| c.row == 0)
    }
}

fn invalid(m: impl Into<String>) -> SimError {
    SimError::InvalidCommand(m.into())
}

pub fn sticks_per_step() -> f32 {
    BuildingKind::Dam.spec().cost.iter().filter(|c| c.0 == Good::Sticks).map(|c| c.1).sum::<f32>().max(1.0)
}

pub fn pack_labour() -> f32 {
    BuildingKind::Dam.spec().work
}

pub fn built(sim: &SimState, c: Cell) -> f32 {
    sim.water.dam_height.get(c.0, c.1).copied().unwrap_or(0.0)
}

pub fn integrity(sim: &SimState, c: Cell) -> f32 {
    if built(sim, c) <= 0.0 { 0.0 } else { sim.water.dam_integrity.get(c.0, c.1).copied().unwrap_or(0.0) }
}

fn ground(sim: &SimState, c: Cell) -> f32 {
    sim.height.get(c.0, c.1).copied().unwrap_or(0.0)
}

pub fn dam(sim: &SimState, id: BuildingId) -> Option<&Dam> {
    sim.buildings.building(id).and_then(|b| b.dam.as_deref())
}

fn dam_mut(sim: &mut SimState, id: BuildingId) -> Result<&mut Dam, SimError> {
    sim.buildings.buildings.iter_mut().find(|b| b.id == id).and_then(|b| b.dam.as_deref_mut()).ok_or_else(|| invalid("no such dam"))
}

/// What a cell still needs: raising towards its target, or a repair.
fn cell_work(sim: &SimState, d: &Dam, c: &DamCell) -> bool {
    let h = built(sim, c.cell);
    h < c.target - EPS || (d.maintain && h > 0.0 && integrity(sim, c.cell) < REPAIR_BELOW)
}

pub fn cell_needs_work(sim: &SimState, id: BuildingId, cell: Cell) -> bool {
    dam(sim, id).is_some_and(|d| d.cells.iter().any(|c| c.cell == cell && cell_work(sim, d, c)))
}

pub fn has_work(sim: &SimState, id: BuildingId) -> bool {
    dam(sim, id).is_some_and(|d| d.cells.iter().any(|c| cell_work(sim, d, c)))
}

fn loads_for(sim: &SimState, c: Cell, target: f32) -> usize {
    ((target - built(sim, c)).max(0.0) / STEP_HEIGHT - EPS).ceil().max(0.0) as usize
}

/// Loads still to carry, including one per worn cell being maintained.
pub fn loads_left(sim: &SimState, id: BuildingId) -> usize {
    let Some(d) = dam(sim, id) else { return 0 };
    d.cells
        .iter()
        .map(|c| {
            let raise = loads_for(sim, c.cell, c.target);
            if raise == 0 && d.maintain && built(sim, c.cell) > 0.0 && integrity(sim, c.cell) < REPAIR_BELOW { 1 } else { raise }
        })
        .sum()
}

/// Cells other builders are already working on.
pub fn claimed(sim: &SimState, id: BuildingId, except: BeaverId) -> Vec<Cell> {
    sim.agents
        .by_id
        .values()
        .filter(|a| a.id != except)
        .filter_map(|a| match a.task {
            crate::agents::Task::DamWork { dam, cell, .. } if dam == id => Some(cell),
            _ => None,
        })
        .collect()
}

/// The lowest unfinished cell, nearest first among equals.
pub fn next_cell(sim: &SimState, id: BuildingId, from: Cell, taken: &[Cell]) -> Option<Cell> {
    let d = dam(sim, id)?;
    let dist = |c: Cell| c.0.abs_diff(from.0) + c.1.abs_diff(from.1);
    d.cells
        .iter()
        .filter(|c| cell_work(sim, d, c) && !taken.contains(&c.cell))
        .min_by(|a, b| {
            let (ha, hb) = (built(sim, a.cell) * integrity(sim, a.cell).max(0.01), built(sim, b.cell) * integrity(sim, b.cell).max(0.01));
            ((ha / STEP_HEIGHT).floor() as i32, dist(a.cell), a.cell).cmp(&((hb / STEP_HEIGHT).floor() as i32, dist(b.cell), b.cell))
        })
        .map(|c| c.cell)
}

/// Pack one carried load into a cell. Returns the sticks used.
pub(crate) fn pack(sim: &mut SimState, id: BuildingId, cell: Cell, sticks: f32) -> f32 {
    let Some((target, rows)) = dam(sim, id).and_then(|d| d.cells.iter().find(|c| c.cell == cell).map(|c| (c.target, d.rows))) else { return 0.0 };
    let h = built(sim, cell);
    let share = (sticks / sticks_per_step()).clamp(0.0, 1.0);
    let next = if h < target - EPS { (h + STEP_HEIGHT * share).min(target) } else { h };
    if next <= 0.0 {
        return 0.0;
    }
    if sim.water.raise_dam(&[(cell, next)]).is_err() {
        return 0.0;
    }
    if let Some(v) = sim.water.dam_durability.get_mut(cell.0, cell.1) {
        *v = rows as f32;
    }
    sim.buildings.stores.book_site_delivery(Good::Sticks, sticks, None);
    if let Ok(d) = dam_mut(sim, id) {
        d.pond_day = f64::NEG_INFINITY;
    }
    sticks
}

fn surface(sim: &SimState, c: Cell) -> f32 {
    ground(sim, c) + sim.water.depth.get(c.0, c.1).copied().unwrap_or(0.0)
}

fn offset(c: Cell, d: (i32, i32), k: i32, n: usize) -> Option<Cell> {
    let (x, y) = (c.0 as i64 + (d.0 * k) as i64, c.1 as i64 + (d.1 * k) as i64);
    (x >= 0 && y >= 0 && (x as usize) < n && (y as usize) < n).then_some((x as usize, y as usize))
}

/// The side the water leaves by: the direction with the lowest water beyond the line.
fn downstream(sim: &SimState, cells: &[Cell]) -> (i32, i32) {
    let n = sim.height.width();
    [(1, 0), (-1, 0), (0, 1), (0, -1)]
        .into_iter()
        .max_by(|&a, &b| {
            let score = |d: (i32, i32)| -> f32 {
                cells.iter().filter_map(|&c| Some(surface(sim, offset(c, d, -1, n)?) - surface(sim, offset(c, d, 1, n)?))).sum()
            };
            score(a).total_cmp(&score(b))
        })
        .unwrap_or((0, 1))
}

fn connected(cells: &[Cell]) -> bool {
    cells.windows(2).all(|w| w[0].0.abs_diff(w[1].0).max(w[0].1.abs_diff(w[1].1)) == 1)
}

fn blocked_by_building(sim: &SimState, c: Cell, except: Option<BuildingId>) -> bool {
    sim.buildings.buildings.iter().any(|b| Some(b.id) != except && b.cells().any(|x| x == c))
}

/// Check a drawn line before it becomes a dam.
pub fn check_line(sim: &SimState, cells: &[Cell]) -> Result<(), String> {
    if cells.is_empty() {
        return Err("draw a line across water".into());
    }
    if cells.len() > MAX_CELLS {
        return Err(format!("at most {MAX_CELLS} cells"));
    }
    let mut seen = std::collections::HashSet::new();
    if !cells.iter().all(|c| seen.insert(*c)) || !connected(cells) {
        return Err("the line must be one unbroken run of cells".into());
    }
    if cells.iter().any(|&(x, y)| !sim.water.depth.in_bounds(x, y)) {
        return Err("off the map".into());
    }
    if !sim.sandbox && cells.iter().any(|&c| !sim.fog.explored(c)) {
        return Err("Unexplored".into());
    }
    if sim.work.supplies.is_some_and(|p| cells.contains(&p)) {
        return Err("the supply pile is there".into());
    }
    if cells.iter().any(|&c| blocked_by_building(sim, c, None)) {
        return Err("something is already built there".into());
    }
    let wet = |c: &Cell| sim.water.depth.get(c.0, c.1).is_some_and(|&d| d >= crate::buildings::WET_DEPTH);
    if !sim.sandbox && !cells.iter().any(wet) {
        return Err("a dam must cross water".into());
    }
    Ok(())
}

/// Targets for a new line: one level crest, the first stage above the lowest ground.
pub fn first_targets(sim: &SimState, cells: &[Cell]) -> Vec<f32> {
    let low = cells.iter().map(|&c| ground(sim, c)).fold(f32::INFINITY, f32::min);
    let crest = low + DEFAULT_TARGET;
    cells.iter().map(|&c| (crest - ground(sim, c)).clamp(0.0, HEIGHT_PER_ROW)).collect()
}

pub fn place(sim: &mut SimState, cells: &[Cell]) -> Result<BuildingId, SimError> {
    check_line(sim, cells).map_err(invalid)?;
    let targets = first_targets(sim, cells);
    let id = BuildingId(sim.buildings.fresh_id());
    let dam = Dam {
        cells: cells.iter().zip(&targets).map(|(&cell, &target)| DamCell { cell, target, row: 0 }).collect(),
        rows: 1,
        maintain: true,
        downstream: downstream(sim, cells),
        history: VecDeque::new(),
        pond: None,
        pond_day: f64::NEG_INFINITY,
    };
    let b = Building { id, kind: BuildingKind::Dam, pos: cells[0], status: Status::Complete, output: 0.0, condition: 1.0, reinforced: false, dam: Some(Box::new(dam)) };
    sim.work.add_complete(&b).map_err(|e| invalid(format!("{e:?}")))?;
    sim.buildings.buildings.push(b);
    Ok(id)
}

/// A planned change: every cell with its new target.
pub struct Plan {
    pub cells: Vec<DamCell>,
    pub rows: u8,
}

impl Plan {
    pub fn added(&self, d: &Dam) -> Vec<Cell> {
        self.cells.iter().filter(|c| !d.cells.iter().any(|o| o.cell == c.cell)).map(|c| c.cell).collect()
    }
}

/// Sticks a plan costs beyond what the dam already needs.
pub fn plan_cost(sim: &SimState, d: &Dam, plan: &Plan) -> f32 {
    let loads = |cells: &[DamCell]| cells.iter().map(|c| loads_for(sim, c.cell, c.target)).sum::<usize>();
    (loads(&plan.cells).saturating_sub(loads(&d.cells))) as f32 * sticks_per_step()
}

fn crest(sim: &SimState, d: &Dam) -> f32 {
    d.line().map(|c| ground(sim, c.cell) + c.target).fold(f32::NEG_INFINITY, f32::max)
}

pub fn plan_raise(sim: &SimState, id: BuildingId, only: &[Cell]) -> Result<Plan, String> {
    let d = dam(sim, id).ok_or("no such dam")?;
    let cap = d.cap();
    let new_crest = crest(sim, d) + STEP_HEIGHT;
    let mut cells = d.cells.clone();
    let mut changed = false;
    for c in cells.iter_mut().filter(|c| only.is_empty() || only.contains(&c.cell)) {
        let want = if only.is_empty() { (new_crest - ground(sim, c.cell)).max(c.target) } else { c.target + STEP_HEIGHT };
        let t = want.clamp(0.0, cap);
        if t > c.target + EPS {
            c.target = t;
            changed = true;
        }
    }
    if !changed {
        return Err("at its height limit: widen it first".into());
    }
    Ok(Plan { cells, rows: d.rows })
}

pub fn plan_widen(sim: &SimState, id: BuildingId) -> Result<Plan, String> {
    let d = dam(sim, id).ok_or("no such dam")?;
    if d.rows as usize >= MAX_ROWS {
        return Err("already as wide as a dam gets".into());
    }
    let n = sim.height.width();
    let mut cells = d.cells.clone();
    for c in d.line() {
        let Some(at) = offset(c.cell, d.downstream, d.rows as i32, n) else { continue };
        if cells.iter().any(|x| x.cell == at) || blocked_by_building(sim, at, Some(id)) {
            continue;
        }
        cells.push(DamCell { cell: at, target: c.target, row: d.rows });
    }
    if cells.len() == d.cells.len() {
        return Err("no room downstream to widen".into());
    }
    Ok(Plan { cells, rows: d.rows + 1 })
}

pub fn plan_extend(sim: &SimState, id: BuildingId) -> Result<Plan, String> {
    let d = dam(sim, id).ok_or("no such dam")?;
    let top = crest(sim, d);
    let mut cells = d.cells.clone();
    for _ in 0..EXTEND_CELLS {
        let line: Vec<Cell> = cells.iter().map(|c| c.cell).collect();
        let heights: Vec<f32> = cells.iter().map(|c| c.target.max(built(sim, c.cell))).collect();
        let Some(at) = crate::dam_preview::preview_heights(sim, &line, &heights).and_then(|p| p.spill_at) else { break };
        let near = line.iter().any(|c| c.0.abs_diff(at.0).max(c.1.abs_diff(at.1)) == 1);
        let h = (top - ground(sim, at)).clamp(0.0, d.cap());
        if line.contains(&at) || !near || h <= EPS || blocked_by_building(sim, at, Some(id)) {
            break;
        }
        cells.push(DamCell { cell: at, target: h, row: 0 });
    }
    if cells.len() == d.cells.len() {
        return Err("water is not getting around the ends".into());
    }
    Ok(Plan { cells, rows: d.rows })
}

pub fn apply_plan(sim: &mut SimState, id: BuildingId, plan: Plan) -> Result<(), SimError> {
    let d = dam_mut(sim, id)?;
    d.cells = plan.cells;
    d.rows = plan.rows;
    d.pond_day = f64::NEG_INFINITY;
    let (rows, cells): (u8, Vec<Cell>) = (d.rows, d.cells.iter().map(|c| c.cell).collect());
    for c in cells {
        if let Some(v) = sim.water.dam_durability.get_mut(c.0, c.1) {
            *v = rows as f32;
        }
    }
    Ok(())
}

pub fn set_maintain(sim: &mut SimState, id: BuildingId, on: bool) -> Result<(), SimError> {
    dam_mut(sim, id)?.maintain = on;
    Ok(())
}

fn measure(sim: &SimState, d: &Dam) -> Option<Pond> {
    let (cells, heights): (Vec<Cell>, Vec<f32>) = d.cells.iter().map(|c| (c.cell, built(sim, c.cell))).filter(|c| c.1 > 0.0).unzip();
    if cells.is_empty() {
        return None;
    }
    let p = crate::dam_preview::preview_heights(sim, &cells, &heights)?;
    let wet: Vec<(Cell, f32)> = p.flooded.iter().filter_map(|&c| {
        let depth = sim.water.depth.get(c.0, c.1).copied().unwrap_or(0.0);
        (depth > crate::buildings::WET_DEPTH).then_some((c, depth))
    }).collect();
    let bottom = p.flooded.iter().map(|&c| ground(sim, c)).fold(f32::INFINITY, f32::min);
    let level = if wet.is_empty() { bottom } else { wet.iter().map(|&(c, dep)| ground(sim, c) + dep).sum::<f32>() / wet.len() as f32 };
    let full = level >= p.level - 0.02;
    let spill = match p.spill_at {
        None => Spill::None,
        Some(c) if cells.contains(&c) => Spill::Over(c),
        Some(c) => Spill::Around(c),
    };
    Some(Pond {
        level,
        full_level: p.level,
        bottom: if bottom.is_finite() { bottom } else { level },
        area: wet.len(),
        volume: wet.iter().map(|w| w.1).sum(),
        max_depth: wet.iter().map(|w| w.1).fold(0.0, f32::max),
        inflow: p.inflow,
        outflow: if full { p.inflow.max(p.pond_loss) } else { p.pond_loss },
        days_to_full: if full { 0.0 } else { p.fill_days },
        spill,
        flooded: p.flooded,
    })
}

pub fn step(sim: &mut SimState) {
    let today = sim.time_days.floor();
    let ids: Vec<BuildingId> = sim.buildings.buildings.iter().filter(|b| b.dam.is_some()).map(|b| b.id).collect();
    for id in ids {
        let Some(d) = dam(sim, id) else { continue };
        if d.pond_day >= today {
            continue;
        }
        let new_day = d.pond_day.is_finite() && d.pond_day < today || d.history.is_empty();
        let pond = measure(sim, d);
        let Ok(d) = dam_mut(sim, id) else { continue };
        if new_day {
            if let Some(p) = &pond {
                d.history.push_back(DamDay { level: p.level, inflow: p.inflow, outflow: p.outflow });
                while d.history.len() > HISTORY_DAYS {
                    d.history.pop_front();
                }
            }
        }
        d.pond = pond;
        d.pond_day = today;
    }
}

/// Mean condition of the built part, 1 when nothing is built yet.
pub fn health(sim: &SimState, d: &Dam) -> f32 {
    let built: Vec<f32> = d.cells.iter().filter(|c| built(sim, c.cell) > 0.0).map(|c| integrity(sim, c.cell)).collect();
    if built.is_empty() { 1.0 } else { built.iter().sum::<f32>() / built.len() as f32 }
}

/// Beavers working on this dam and loads on their way to it.
pub fn crew(sim: &SimState, id: BuildingId) -> (usize, usize) {
    let mut builders = 0;
    let mut loads = 0;
    for a in sim.agents.by_id.values() {
        match a.task {
            crate::agents::Task::DamWork { dam, .. } if dam == id => {
                builders += 1;
                if a.load.is_some() {
                    loads += 1;
                }
            }
            crate::agents::Task::Fetch { then, .. } if then == id => builders += 1,
            _ => {}
        }
    }
    (builders, loads)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flat_river() -> (SimState, Vec<Cell>) {
        let mut s = SimState::new(1);
        s.sandbox = false;
        let line: Vec<Cell> = (10..15).map(|x| (x, 20)).collect();
        for y in 15..26 {
            for x in 8..17 {
                *s.height.get_mut(x, y).unwrap() = 5.0;
                *s.water.depth.get_mut(x, y).unwrap() = 0.3;
            }
        }
        s.fog = crate::fog::Fog::clear(s.world.size);
        (s, line)
    }

    #[test]
    fn a_drawn_dam_is_one_building_built_a_load_at_a_time() {
        let (mut s, line) = flat_river();
        let id = place(&mut s, &line).unwrap();
        assert_eq!(s.buildings.buildings.iter().filter(|b| b.kind == BuildingKind::Dam).count(), 1);
        assert_eq!(s.buildings.building(id).unwrap().cells().count(), line.len());
        assert_eq!(loads_left(&s, id), line.len() * 2);
        assert!(check_line(&s, &line).is_err(), "the same cells cannot hold two dams");
        let used = pack(&mut s, id, line[0], sticks_per_step());
        assert_eq!(used, sticks_per_step());
        assert!((built(&s, line[0]) - STEP_HEIGHT).abs() < 1e-5);
        assert_eq!(loads_left(&s, id), line.len() * 2 - 1);
        let first = next_cell(&s, id, line[4], &[]).unwrap();
        assert_ne!(first, line[0], "the lowest cells are built first");
    }

    #[test]
    fn height_needs_width_and_widening_thickens_downstream() {
        let (mut s, line) = flat_river();
        let id = place(&mut s, &line).unwrap();
        assert!(plan_raise(&s, id, &[]).is_err(), "one row tops out at its first stage");
        let wide = plan_widen(&s, id).unwrap();
        assert_eq!(wide.rows, 2);
        assert_eq!(wide.added(dam(&s, id).unwrap()).len(), line.len());
        apply_plan(&mut s, id, wide).unwrap();
        let raised = plan_raise(&s, id, &[]).unwrap();
        assert!(raised.cells.iter().all(|c| (c.target - 1.5).abs() < 1e-5));
        assert!(plan_cost(&s, dam(&s, id).unwrap(), &raised) > 0.0);
    }
}
