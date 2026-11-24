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

#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct DamCell {
    pub cell: Cell,
    /// Height above this cell's ground that the dam is meant to reach.
    pub target: f32,
    /// 0 is the drawn line; higher rows thicken the dam downstream.
    pub row: u8,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct DamDay {
    pub level: f32,
    pub inflow: f32,
    pub outflow: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Spill {
    None,
    /// Water pours over the crest at this dam cell.
    Over(Cell),
    /// Water goes around the dam at this cell.
    Around(Cell),
    /// Water leaves the pond through a gate or a canal at this cell: by design, not a leak.
    Gate(Cell),
}

/// The pond a dam holds, refreshed once a day and after every change.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
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

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Dam {
    pub cells: Vec<DamCell>,
    pub rows: u8,
    pub maintain: bool,
    /// Cells turned into gates (see `gates`); packing leaves them alone.
    pub gates: Vec<crate::gates::Gate>,
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
    /// The front row from one end to the other, whatever order its cells were added in.
    pub fn line(&self) -> impl Iterator<Item = &DamCell> {
        in_order(self.cells.iter().filter(|c| c.row == 0))
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

fn is_gate(d: &Dam, c: Cell) -> bool {
    d.gates.iter().any(|g| g.cell == c && g.built)
}

/// What a cell still needs: raising towards its target, or a repair. A built gate's height
/// is the gate's to set.
fn cell_work(sim: &SimState, d: &Dam, c: &DamCell) -> bool {
    if is_gate(d, c.cell) {
        return false;
    }
    let h = built(sim, c.cell);
    h < c.target - EPS || (d.maintain && h > 0.0 && integrity(sim, c.cell) < REPAIR_BELOW)
}

/// Builders raise cells to their target; maintainers re-pack finished cells that have worn.
fn cell_task(sim: &SimState, d: &Dam, c: &DamCell, upkeep: bool) -> bool {
    if is_gate(d, c.cell) {
        return false;
    }
    let h = built(sim, c.cell);
    if upkeep {
        d.maintain && h > 0.0 && h >= c.target - EPS && integrity(sim, c.cell) < REPAIR_BELOW
    } else {
        h < c.target - EPS
    }
}

pub fn cell_needs_work(sim: &SimState, id: BuildingId, cell: Cell, upkeep: bool) -> bool {
    dam(sim, id).is_some_and(|d| d.cells.iter().any(|c| c.cell == cell && cell_task(sim, d, c, upkeep)))
}

/// Work for this dam's builders (`upkeep` false: raising, gates) or maintainers (re-packing).
pub fn has_work_for(sim: &SimState, id: BuildingId, upkeep: bool) -> bool {
    dam(sim, id).is_some_and(|d| d.cells.iter().any(|c| cell_task(sim, d, c, upkeep)) || (!upkeep && d.gates.iter().any(|g| !g.built || g.pending.is_some() || crate::locks::pending(sim, g.cell))))
}

pub fn has_work(sim: &SimState, id: BuildingId) -> bool {
    dam(sim, id).is_some_and(|d| d.cells.iter().any(|c| cell_work(sim, d, c)) || d.gates.iter().any(|g| !g.built || g.pending.is_some() || crate::locks::pending(sim, g.cell)))
}

/// Loads to raise a cell to its target. A cell standing for more than a metre of wall (a
/// diagonal step) takes proportionally more, so a dam costs by its length, not its cells.
fn loads_for(sim: &SimState, c: Cell, target: f32, span: f32) -> usize {
    ((target - built(sim, c)).max(0.0) / STEP_HEIGHT * span - EPS).ceil().max(0.0) as usize
}

/// The order cells run in along a line, as indices into `cells`: from an end, preferring
/// side-by-side neighbours to corner ones. Cells can be stored in any order (extensions are
/// added at the back whichever end they grow from).
pub fn chain(cells: &[Cell]) -> Vec<usize> {
    let n = cells.len();
    let touch = |a: Cell, b: Cell| a != b && a.0.abs_diff(b.0) <= 1 && a.1.abs_diff(b.1) <= 1;
    let degree = |i: usize| cells.iter().filter(|&&c| touch(c, cells[i])).count();
    let mut left: Vec<usize> = (0..n).collect();
    let mut out = Vec::with_capacity(n);
    while !left.is_empty() {
        let (k, _) = left.iter().enumerate().min_by_key(|&(_, &i)| (degree(i), i)).unwrap();
        let mut at = left.remove(k);
        out.push(at);
        loop {
            let next = left.iter().enumerate()
                .filter(|&(_, &j)| touch(cells[at], cells[j]))
                .min_by_key(|&(_, &j)| (cells[at].0.abs_diff(cells[j].0) + cells[at].1.abs_diff(cells[j].1), j));
            let Some((k, _)) = next else { break };
            at = left.remove(k);
            out.push(at);
        }
    }
    out
}

/// Dam cells walked from one end of their line to the other.
pub fn in_order<'a>(cells: impl Iterator<Item = &'a DamCell>) -> impl Iterator<Item = &'a DamCell> {
    let cells: Vec<&DamCell> = cells.collect();
    let at: Vec<Cell> = cells.iter().map(|c| c.cell).collect();
    chain(&at).into_iter().map(move |i| cells[i])
}

/// Metres of wall each cell stands for: half the distance to each neighbour along the line
/// (1 along a straight run, about 1.4 on a diagonal), in the order given.
pub fn spans(cells: &[Cell]) -> Vec<f32> {
    let order = chain(cells);
    let mut out = vec![1.0; cells.len()];
    let dist = |a: Cell, b: Cell| ((a.0 as f32 - b.0 as f32).powi(2) + (a.1 as f32 - b.1 as f32).powi(2)).sqrt();
    let near = |a: Cell, b: Cell| a.0.abs_diff(b.0) <= 1 && a.1.abs_diff(b.1) <= 1;
    for (k, &i) in order.iter().enumerate() {
        let c = cells[i];
        let sides: Vec<f32> = [k.checked_sub(1), (k + 1 < order.len()).then_some(k + 1)]
            .into_iter().flatten().map(|j| cells[order[j]]).filter(|&o| near(c, o)).map(|o| dist(c, o)).collect();
        if !sides.is_empty() {
            out[i] = sides.iter().sum::<f32>() / sides.len() as f32;
        }
    }
    out
}

/// The span of one cell of a dam, measured along its own row.
fn span_in(cells: &[DamCell], cell: Cell) -> f32 {
    let Some(row) = cells.iter().find(|c| c.cell == cell).map(|c| c.row) else { return 1.0 };
    let line: Vec<Cell> = cells.iter().filter(|c| c.row == row).map(|c| c.cell).collect();
    line.iter().position(|&c| c == cell).map_or(1.0, |i| spans(&line)[i])
}

fn row_spans(cells: &[DamCell]) -> Vec<f32> {
    let mut out = vec![1.0; cells.len()];
    let rows: std::collections::BTreeSet<u8> = cells.iter().map(|c| c.row).collect();
    for row in rows {
        let idx: Vec<usize> = (0..cells.len()).filter(|&i| cells[i].row == row).collect();
        let line: Vec<Cell> = idx.iter().map(|&i| cells[i].cell).collect();
        for (k, w) in spans(&line).into_iter().enumerate() {
            out[idx[k]] = w;
        }
    }
    out
}

/// Loads still to carry, including one per worn cell being maintained.
pub fn loads_left(sim: &SimState, id: BuildingId) -> usize {
    let Some(d) = dam(sim, id) else { return 0 };
    let spans = row_spans(&d.cells);
    d.cells
        .iter()
        .zip(spans)
        .filter(|(c, _)| !is_gate(d, c.cell))
        .map(|(c, span)| {
            let raise = loads_for(sim, c.cell, c.target, span);
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
pub fn next_cell(sim: &SimState, id: BuildingId, from: Cell, taken: &[Cell], upkeep: bool) -> Option<Cell> {
    let d = dam(sim, id)?;
    let dist = |c: Cell| c.0.abs_diff(from.0) + c.1.abs_diff(from.1);
    d.cells
        .iter()
        .filter(|c| cell_task(sim, d, c, upkeep) && !taken.contains(&c.cell))
        .min_by(|a, b| {
            // Upkeep mends the weakest cells first.
            if upkeep {
                let w = |c: &DamCell| (integrity(sim, c.cell) * 20.0) as i32;
                return (w(a), dist(a.cell), a.cell).cmp(&(w(b), dist(b.cell), b.cell));
            }
            let (ha, hb) = (built(sim, a.cell) * integrity(sim, a.cell).max(0.01), built(sim, b.cell) * integrity(sim, b.cell).max(0.01));
            ((ha / STEP_HEIGHT).floor() as i32, dist(a.cell), a.cell).cmp(&((hb / STEP_HEIGHT).floor() as i32, dist(b.cell), b.cell))
        })
        .map(|c| c.cell)
}

/// Pack one carried load into a cell. Returns the sticks used.
pub(crate) fn pack(sim: &mut SimState, id: BuildingId, cell: Cell, sticks: f32) -> f32 {
    let Some((target, rows, span)) = dam(sim, id).and_then(|d| d.cells.iter().find(|c| c.cell == cell).map(|c| (c.target, d.rows, span_in(&d.cells, cell)))) else { return 0.0 };
    let h = built(sim, cell);
    let share = (sticks / sticks_per_step()).clamp(0.0, 1.0);
    // A load packs a step of wall a metre long: a longer (diagonal) cell rises less per load.
    let next = if h < target - EPS { (h + STEP_HEIGHT * share / span.max(1.0)).min(target) } else { h };
    if next <= 0.0 {
        return 0.0;
    }
    if sim.water.raise_dam(&[(cell, next)]).is_err() {
        return 0.0;
    }
    let durability = rows as f32 * facing(sim, id);
    if let Some(v) = sim.water.dam_durability.get_mut(cell.0, cell.1) {
        *v = durability;
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
            // Only neighbours off the line count: along the line, dam cells
            // cancel out and the two banks alone would decide.
            let off = |c: Cell, d: (i32, i32), k: i32| offset(c, d, k, n).filter(|o| !cells.contains(o));
            let score = |d: (i32, i32)| -> f32 { cells.iter().filter_map(|&c| Some(surface(sim, off(c, d, -1)?) - surface(sim, off(c, d, 1)?))).sum() };
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
/// Water a dam line may cross: any water the game draws (however shallow a creek), or a river
/// or lake channel.
pub fn water_for_dam(sim: &SimState, c: Cell) -> bool {
    sim.water.shown(c.0 as i32, c.1 as i32)
        || sim.world.river.get(c.0, c.1).copied().unwrap_or(false)
        || sim.world.lake.get(c.0, c.1).copied().unwrap_or(false)
}

pub fn check_line(sim: &SimState, cells: &[Cell]) -> Result<(), String> {
    if cells.is_empty() {
        return Err("draw a line across water".into());
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
    if !sim.sandbox && !cells.iter().any(|&c| water_for_dam(sim, c)) {
        return Err("a dam must cross water".into());
    }
    Ok(())
}

/// Targets for a new line: one level crest, the first stage above the lowest ground.
pub fn first_targets(sim: &SimState, cells: &[Cell]) -> Vec<f32> {
    let low = cells.iter().map(|&c| ground(sim, c)).fold(f32::INFINITY, f32::min);
    let crest = low + DEFAULT_TARGET;
    cells.iter().map(|&c| (crest - ground(sim, c)).clamp(MIN_WALL, HEIGHT_PER_ROW)).collect()
}

/// Every cell of a drawn dam gets at least a footing wall (`MIN_WALL`), even where the ground
/// already stands above the crest: the whole line the player drew is built, and none of it is
/// left staked out with nothing ever to build there.

pub fn place(sim: &mut SimState, cells: &[Cell]) -> Result<BuildingId, SimError> {
    sim.unlocked(BuildingKind::Dam).map_err(|e| SimError::InvalidCommand(e.to_string()))?;
    check_line(sim, cells).map_err(invalid)?;
    let targets = first_targets(sim, cells);
    let id = BuildingId(sim.buildings.fresh_id());
    let dam = Dam {
        cells: cells.iter().zip(&targets).map(|(&cell, &target)| DamCell { cell, target, row: 0 }).collect(),
        rows: 1,
        maintain: true,
        gates: Vec::new(),
        downstream: downstream(sim, cells),
        history: VecDeque::new(),
        pond: None,
        pond_day: f64::NEG_INFINITY,
    };
    let b = Building { id, kind: BuildingKind::Dam, pos: cells[0], status: Status::Complete, output: 0.0, condition: 1.0, reinforced: false, dam: Some(Box::new(dam)), canal: None };
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
    let loads = |cells: &[DamCell]| cells.iter().zip(row_spans(cells)).map(|(c, w)| loads_for(sim, c.cell, c.target, w)).sum::<usize>();
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
        cells.push(DamCell { cell: at, target: c.target.max(MIN_WALL), row: d.rows });
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
        let row: Vec<&DamCell> = in_order(cells.iter()).collect();
        let line: Vec<Cell> = row.iter().map(|c| c.cell).collect();
        let heights: Vec<f32> = row.iter().map(|c| c.target.max(built(sim, c.cell))).collect();
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

/// Lengthen the dam along cells the player drew from one of its ends. Each new
/// cell must touch the dam or a cell added before it; new cells aim for the crest.
pub fn plan_extend_to(sim: &SimState, id: BuildingId, drawn: &[Cell]) -> Result<Plan, String> {
    let d = dam(sim, id).ok_or("no such dam")?;
    let top = crest(sim, d);
    let mut cells = d.cells.clone();
    for &at in drawn {
        if cells.iter().any(|c| c.cell == at) {
            continue;
        }
        let near = cells.iter().any(|c| c.row == 0 && c.cell.0.abs_diff(at.0).max(c.cell.1.abs_diff(at.1)) == 1);
        if !near {
            return Err("the new cells must run on from an end".into());
        }
        if blocked_by_building(sim, at, Some(id)) {
            return Err("something is already built there".into());
        }
        let h = (top - ground(sim, at)).clamp(MIN_WALL, d.cap().max(MIN_WALL));
        cells.push(DamCell { cell: at, target: h, row: 0 });
    }
    if cells.len() == d.cells.len() {
        return Err("draw past an end to extend".into());
    }
    Ok(Plan { cells, rows: d.rows })
}

pub fn apply_plan(sim: &mut SimState, id: BuildingId, plan: Plan) -> Result<(), SimError> {
    let d = dam_mut(sim, id)?;
    d.cells = plan.cells;
    d.rows = plan.rows;
    d.pond_day = f64::NEG_INFINITY;
    refresh_durability(sim, id);
    Ok(())
}

/// How much longer a pebble-faced dam's cells hold against age and overtopping.
fn facing(sim: &SimState, id: BuildingId) -> f32 {
    if sim.buildings.pebbled.contains(&id) { crate::buildings::PEBBLE_DAM_FACTOR } else { 1.0 }
}

/// Set every cell of a dam to its rows' strength, pebble facing included.
pub(crate) fn refresh_durability(sim: &mut SimState, id: BuildingId) {
    let Some(d) = dam(sim, id) else { return };
    let rows = d.rows as f32 * facing(sim, id);
    let cells: Vec<Cell> = d.cells.iter().map(|c| c.cell).collect();
    for c in cells {
        if let Some(v) = sim.water.dam_durability.get_mut(c.0, c.1) {
            *v = rows;
        }
    }
}

pub fn set_maintain(sim: &mut SimState, id: BuildingId, on: bool) -> Result<(), SimError> {
    dam_mut(sim, id)?.maintain = on;
    Ok(())
}

/// The water standing against the dam's upstream face, read from the live water: wet cells
/// reached from the line's upstream side, staying within 0.3 m of the face's level so the
/// river below (which only meets the pond round an end) is never counted.
fn live_pond(sim: &SimState, d: &Dam) -> Option<(f32, Vec<Cell>)> {
    let n = sim.height.width();
    let depth = |c: Cell| sim.water.depth.get(c.0, c.1).copied().unwrap_or(0.0);
    let surface = |c: Cell| ground(sim, c) + depth(c);
    let on_dam: std::collections::HashSet<Cell> = d.cells.iter().map(|c| c.cell).collect();
    let (ux, uy) = (-d.downstream.0, -d.downstream.1);
    let starts: Vec<Cell> = d.line().filter_map(|c| {
        let (x, y) = (c.cell.0 as i32 + ux, c.cell.1 as i32 + uy);
        (x >= 0 && y >= 0 && (x as usize) < n && (y as usize) < n).then_some((x as usize, y as usize))
    }).filter(|c| !on_dam.contains(c) && depth(*c) > crate::buildings::WET_DEPTH).collect();
    let face = starts.iter().map(|&c| surface(c)).fold(f32::NEG_INFINITY, f32::max);
    if !face.is_finite() {
        return None;
    }
    let mut seen: std::collections::HashSet<Cell> = starts.iter().copied().collect();
    let mut stack = starts.clone();
    let mut cells = Vec::new();
    while let Some(c) = stack.pop() {
        cells.push(c);
        if cells.len() > 40_000 {
            break;
        }
        for (dx, dy) in [(1i32, 0i32), (-1, 0), (0, 1), (0, -1)] {
            let (x, y) = (c.0 as i32 + dx, c.1 as i32 + dy);
            if x < 0 || y < 0 || x as usize >= n || y as usize >= n {
                continue;
            }
            let m = (x as usize, y as usize);
            if on_dam.contains(&m) || seen.contains(&m) || depth(m) <= crate::buildings::WET_DEPTH || surface(m) < face - 0.3 {
                continue;
            }
            seen.insert(m);
            stack.push(m);
        }
    }
    let level = cells.iter().map(|&c| surface(c)).sum::<f32>() / cells.len() as f32;
    Some((level, cells))
}

/// The dam's pond: the still-pond estimate for where it will fill to and how fast, and the
/// live water for what it holds now and where water is leaving. Over and Gate come from the
/// live water; Around comes from the estimate only when it agrees with the live level.
fn measure(sim: &SimState, d: &Dam) -> Option<Pond> {
    let (cells, heights): (Vec<Cell>, Vec<f32>) = d.cells.iter().map(|c| (c.cell, built(sim, c.cell))).filter(|c| c.1 > 0.0 || is_gate(d, c.0)).map(|c| (c.0, c.1.max(0.01))).unzip();
    if cells.is_empty() {
        return None;
    }
    let p = { let _p = crate::prof::scope("step/dams/measure pond/still-pond estimate"); crate::dam_preview::preview_heights_kept(sim, &cells, &heights)? };
    let live = { let _p = crate::prof::scope("step/dams/measure pond/live pond"); live_pond(sim, d) };
    let depth_at = |c: Cell| sim.water.depth.get(c.0, c.1).copied().unwrap_or(0.0);
    let wet: Vec<(Cell, f32)> = match &live {
        Some((_, cs)) => cs.iter().map(|&c| (c, depth_at(c))).collect(),
        None => p.flooded.iter().filter_map(|&c| (depth_at(c) > crate::buildings::WET_DEPTH).then_some((c, depth_at(c)))).collect(),
    };
    let bottom = wet.iter().map(|w| ground(sim, w.0)).chain(p.flooded.iter().map(|&c| ground(sim, c))).fold(f32::INFINITY, f32::min);
    let level = match &live { Some((l, _)) => *l, None if wet.is_empty() => bottom, None => wet.iter().map(|&(c, dep)| ground(sim, c) + dep).sum::<f32>() / wet.len() as f32 };
    // Where it will fill to: the estimate, unless the live pond already stands higher.
    let full_level = if p.level.is_finite() && p.level >= level - 0.05 { p.level } else { level.max(p.level.min(crest(sim, d))) };
    let full = level >= full_level - 0.02;

    // Water let out on purpose: a gate (this dam's or a canal's) or a canal reached by the pond.
    let pond_cells: std::collections::HashSet<Cell> = wet.iter().map(|w| w.0).collect();
    let touches = |c: Cell| [(0i32, 0i32), (1, 0), (-1, 0), (0, 1), (0, -1)].iter().any(|(dx, dy)| {
        let (x, y) = (c.0 as i32 + dx, c.1 as i32 + dy);
        x >= 0 && y >= 0 && pond_cells.contains(&(x as usize, y as usize))
    });
    let mut outlet = None;
    for b in &sim.buildings.buildings {
        for g in b.dam.iter().flat_map(|x| x.gates.iter()).filter(|g| g.built && g.sill() < level - 0.01) {
            if touches(g.cell) {
                outlet = Some(g.cell);
            }
        }
        // A through-canal whose gate stands below the pond: its cells at the pond carry water off.
        if let Some(k) = b.canal.as_deref().filter(|k| k.through && k.gate.built && k.gate.sill() < level - 0.01 && k.cells.iter().any(|c| touches(c.cell))) {
            outlet = Some(k.gate.cell);
        }
    }
    let lowest = d.line().filter(|c| !is_gate(d, c.cell) && built(sim, c.cell) > 0.0).min_by(|a, b| (ground(sim, a.cell) + built(sim, a.cell)).total_cmp(&(ground(sim, b.cell) + built(sim, b.cell))));
    let over = lowest.filter(|c| level >= ground(sim, c.cell) + built(sim, c.cell) - 0.03).map(|c| c.cell);
    let estimate_agrees = (p.level - level).abs() < 0.15 || live.is_none();
    let spill = if let Some(c) = outlet {
        Spill::Gate(c)
    } else if let Some(c) = over {
        Spill::Over(c)
    } else {
        match p.spill_at {
            Some(c) if estimate_agrees && !cells.contains(&c) => Spill::Around(c),
            Some(c) if estimate_agrees => Spill::Over(c),
            _ => Spill::None,
        }
    };
    Some(Pond {
        level,
        full_level,
        bottom: if bottom.is_finite() { bottom } else { level },
        area: wet.len(),
        volume: wet.iter().map(|w| w.1).sum(),
        max_depth: wet.iter().map(|w| w.1).fold(0.0, f32::max),
        inflow: p.inflow,
        outflow: (if full { p.inflow.max(p.pond_loss) } else { p.pond_loss }).max(0.0),
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
        // Dams drawn before every cell had a footing get theirs (once a day is plenty).
        if d.cells.iter().any(|c| c.target < MIN_WALL && !is_gate(d, c.cell)) {
            if let Ok(d) = dam_mut(sim, id) {
                let gates: Vec<Cell> = d.gates.iter().map(|g| g.cell).collect();
                for c in d.cells.iter_mut().filter(|c| c.target < MIN_WALL && !gates.contains(&c.cell)) {
                    c.target = MIN_WALL;
                }
            }
        }
        let Some(d) = dam(sim, id) else { continue };
        let new_day = d.pond_day.is_finite() && d.pond_day < today || d.history.is_empty();
        let _p = crate::prof::scope("step/dams/measure pond");
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

/// Mean condition of the built part, 1 when nothing is built yet. Gates don't wear.
pub fn health(sim: &SimState, d: &Dam) -> f32 {
    let built: Vec<f32> = d.cells.iter().filter(|c| built(sim, c.cell) > 0.0 && !is_gate(d, c.cell)).map(|c| integrity(sim, c.cell)).collect();
    if built.is_empty() { 1.0 } else { built.iter().sum::<f32>() / built.len() as f32 }
}

/// Make a dam cell a gate. A builder brings its sticks; it is Shut once built.
pub fn add_gate(sim: &mut SimState, id: BuildingId, cell: Cell) -> Result<(), SimError> {
    plan_gate(sim, id, cell).map_err(invalid)?;
    let g = ground(sim, cell);
    let target = dam(sim, id).and_then(|d| d.line().find(|c| c.cell == cell)).map_or(0.0, |c| c.target);
    dam_mut(sim, id)?.gates.push(crate::gates::Gate::new(cell, g, g + target));
    Ok(())
}

/// Why a cell can't become a gate, or the sticks it costs.
pub fn plan_gate(sim: &SimState, id: BuildingId, cell: Cell) -> Result<f32, String> {
    let d = dam(sim, id).ok_or("no such dam")?;
    if !d.line().any(|c| c.cell == cell) {
        return Err("pick a cell on the dam's line".into());
    }
    if d.gates.iter().any(|g| g.cell == cell) {
        return Err("already a gate".into());
    }
    if built(sim, cell) <= 0.0 {
        return Err("build this cell first".into());
    }
    Ok(crate::canals::GATE_STICKS)
}

pub fn set_gate(sim: &mut SimState, id: BuildingId, cell: Cell, setting: crate::gates::GateSetting) -> Result<(), SimError> {
    let d = dam_mut(sim, id)?;
    let g = d.gates.iter_mut().find(|g| g.cell == cell).ok_or_else(|| invalid("no gate there"))?;
    crate::gates::request(g, setting)?;
    d.pond_day = f64::NEG_INFINITY;
    Ok(())
}

/// A gate waiting to be built or turned, not already claimed.
pub fn gate_job(sim: &SimState, id: BuildingId, taken: &[Cell]) -> Option<Cell> {
    dam(sim, id)?.gates.iter().find(|g| (!g.built || g.pending.is_some() || crate::locks::pending(sim, g.cell)) && !taken.contains(&g.cell)).map(|g| g.cell)
}

pub fn gates_claimed(sim: &SimState, id: BuildingId, except: BeaverId) -> Vec<Cell> {
    sim.agents.by_id.values().filter(|a| a.id != except).filter_map(|a| match a.task {
        crate::agents::Task::Gate { site, cell, .. } if site == id => Some(cell),
        _ => None,
    }).collect()
}

/// One builder visit to a gate: build it (using its sticks) or make the asked-for change.
pub fn work_gate(sim: &mut SimState, id: BuildingId, cell: Cell) -> Result<(), SimError> {
    let g = *dam(sim, id).and_then(|d| d.gates.iter().find(|g| g.cell == cell)).ok_or_else(|| invalid("no gate there"))?;
    if !g.built && !crate::gates::take_sticks(sim, cell, crate::canals::GATE_STICKS) {
        return Err(invalid("the gate needs sticks"));
    }
    let d = dam_mut(sim, id)?;
    let gate = d.gates.iter_mut().find(|g| g.cell == cell).expect("found above");
    gate.built = true;
    if let Some(s) = gate.pending.take() {
        gate.setting = s;
    }
    let gate = *gate;
    d.pond_day = f64::NEG_INFINITY;
    crate::gates::apply(sim, &gate)?;
    crate::locks::build(sim, cell)
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
    fn a_dam_may_cross_any_water_the_game_draws() {
        let mut s = SimState::new(1);
        s.sandbox = false;
        s.fog = crate::fog::Fog::clear(s.world.size);
        s.water.depth.as_mut_slice().fill(0.0);
        s.world.river.as_mut_slice().fill(false);
        s.world.lake.as_mut_slice().fill(false);
        let line: Vec<Cell> = (10..15).map(|x| (x, 20)).collect();
        assert!(check_line(&s, &line).is_err(), "a line over dry ground");
        // A creek a few centimetres deep, one cell wide, running across the line.
        for y in 15..26 {
            *s.water.depth.get_mut(12, y).unwrap() = 0.03;
        }
        assert!(s.water.shown(12, 20), "the creek is drawn");
        assert_eq!(check_line(&s, &line), Ok(()), "a dam across a shallow creek");
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
        let first = next_cell(&s, id, line[4], &[], false).unwrap();
        assert_ne!(first, line[0], "the lowest cells are built first");
    }

    #[test]
    fn builders_raise_dams_and_maintainers_repack_worn_cells() {
        let (mut s, line) = flat_river();
        let id = place(&mut s, &line).unwrap();
        assert!(has_work_for(&s, id, false) && !has_work_for(&s, id, true), "a new dam is builders' work");
        while let Some(c) = next_cell(&s, id, line[0], &[], false) {
            pack(&mut s, id, c, sticks_per_step());
        }
        assert!(!has_work_for(&s, id, false) && !has_work_for(&s, id, true), "a sound, finished dam needs nobody");
        *s.water.dam_integrity.get_mut(line[2].0, line[2].1).unwrap() = REPAIR_BELOW / 2.0;
        assert!(has_work_for(&s, id, true) && !has_work_for(&s, id, false), "a worn cell is maintainers' work");
        assert_eq!(next_cell(&s, id, line[0], &[], true), Some(line[2]));
        pack(&mut s, id, line[2], sticks_per_step());
        assert!(!has_work_for(&s, id, true), "re-packing restores the cell");
        let slots = &s.work.place(id).unwrap().slots;
        assert!(slots.iter().any(|sl| sl.role == crate::work::Role::Build) && slots.iter().all(|sl| sl.role != crate::work::Role::Maintain), "maintainers need no seat");
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

    #[test]
    fn downstream_crosses_the_line_even_when_one_bank_is_far_lower() {
        let (mut s, line) = flat_river();
        // Water stands higher on the upstream side (y < 20) than downstream.
        for x in 8..17 {
            for y in 15..20 {
                *s.water.depth.get_mut(x, y).unwrap() = 0.6;
            }
            for y in 21..26 {
                *s.water.depth.get_mut(x, y).unwrap() = 0.1;
            }
        }
        // One bank sits far below the other: along the line must not win.
        for y in 15..26 {
            *s.height.get_mut(9, y).unwrap() = 1.0;
            *s.height.get_mut(15, y).unwrap() = 9.0;
        }
        assert_eq!(downstream(&s, &line), (0, 1));
    }

    #[test]
    fn a_diagonal_dam_costs_about_the_same_per_metre_as_a_straight_one() {
        let (s, straight) = flat_river();
        let diagonal: Vec<Cell> = (0..4).map(|k| (10 + k, 18 + k)).collect();
        let per_metre = |cells: &[Cell], metres: f32| crate::dam_preview::loads_for(&s, cells, &vec![1.0; cells.len()]) as f32 / metres;
        let (a, b) = (per_metre(&straight, 5.0), per_metre(&diagonal, 4.0 * std::f32::consts::SQRT_2));
        assert!((a - b).abs() / a < 0.1, "straight {a} vs diagonal {b} loads per metre");
    }

    #[test]
    fn a_line_stored_out_of_order_is_walked_end_to_end() {
        // An extension added at the front end is stored at the back.
        let cells: Vec<Cell> = vec![(5, 5), (6, 5), (7, 6), (4, 5), (3, 4)];
        let order: Vec<Cell> = chain(&cells).into_iter().map(|i| cells[i]).collect();
        assert!(order == vec![(3, 4), (4, 5), (5, 5), (6, 5), (7, 6)] || order == vec![(7, 6), (6, 5), (5, 5), (4, 5), (3, 4)], "{order:?}");
        let w = spans(&cells);
        assert!((w[1] - (1.0 + std::f32::consts::SQRT_2) / 2.0).abs() < 1e-4);
        assert!((w[0] - 1.0).abs() < 1e-4);
    }

    #[test]
    fn a_dam_extends_along_cells_drawn_from_its_end() {
        let (mut s, line) = flat_river();
        let id = place(&mut s, &line).unwrap();
        let more: Vec<Cell> = vec![(15, 20), (16, 20)];
        let plan = plan_extend_to(&s, id, &more).unwrap();
        assert_eq!(plan.added(dam(&s, id).unwrap()), more);
        assert!(plan_extend_to(&s, id, &[(15, 24)]).is_err(), "new cells must run on from an end");
        apply_plan(&mut s, id, plan).unwrap();
        assert_eq!(dam(&s, id).unwrap().line().count(), line.len() + 2);
        // Growing from the other end too: the line still runs end to end.
        let plan = plan_extend_to(&s, id, &[(9, 20), (8, 20)]).unwrap();
        apply_plan(&mut s, id, plan).unwrap();
        let ends: Vec<Cell> = dam(&s, id).unwrap().line().map(|c| c.cell).collect();
        let ends = (ends[0], ends[ends.len() - 1]);
        assert!(ends == ((8, 20), (16, 20)) || ends == ((16, 20), (8, 20)), "both ends stay ends: {ends:?}");
    }
}
