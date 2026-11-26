//! Canals: single buildings dug one cell at a time, with siltation, gates, and growth.
include!(concat!(env!("OUT_DIR"), "/canals_constants.rs"));
use std::collections::{BTreeSet, HashMap, VecDeque};

use crate::agents::WorkKind;
use crate::buildings::{Building, BuildingId, BuildingKind, Status};
use crate::gates::{apply_across, cell_taken, check_cell_open, claimed_work, depth, finish_gate, ground, invalid, is_water as water_cell, push_history, surface, Gate, GateSetting};
use crate::water::Cell;
use crate::{SimError, SimState};

const EPS: f32 = 1e-3;
const APPROACH_CELLS: usize = 15;
const SILT_SETTLES_EVERY_DAYS: f64 = 5.0;

#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CanalCell { pub cell: Cell, pub target: f32, pub ground: f32, pub row: u8, pub silt: f32, pub done: bool }

#[derive(Clone, Copy, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CanalDay { pub floatable: f32, pub inflow: f32, pub outflow: f32, pub silted: f32 }

#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CanalStatus {
    pub cells: usize, pub wet: usize, pub floatable: usize, pub dug: usize, pub silted: usize,
    pub mean_depth: f32, pub max_depth: f32, pub inflow: f32, pub outflow: f32, pub source_level: f32,
}
impl CanalStatus {
    pub fn floatable_share(&self) -> f32 { if self.cells == 0 { 0.0 } else { self.floatable as f32 / self.cells as f32 } }
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Canal {
    pub cells: Vec<CanalCell>,
    pub source: Cell,
    pub gate: Gate,
    pub through: bool,
    pub upkeep: bool,
    pub history: VecDeque<CanalDay>,
    pub status: Option<CanalStatus>,
    status_day: f64,
}
impl Canal {
    pub fn route(&self) -> impl Iterator<Item = &CanalCell> { self.cells.iter().filter(|c| c.row == 0) }
    pub fn width(&self) -> u8 { self.cells.iter().map(|c| c.row + 1).max().unwrap_or(1) }
    pub fn end(&self) -> Cell { self.route().last().map_or(self.gate.cell, |c| c.cell) }
    pub(crate) fn invalidate_status(&mut self) { self.status_day = f64::NEG_INFINITY; }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum CanalGrowth { Deepen, Widen, Extend }

pub fn is_water(sim: &SimState, c: Cell) -> bool { water_cell(sim, c) }
pub fn canal(sim: &SimState, id: BuildingId) -> Option<&Canal> { sim.buildings.building(id).and_then(|b| b.canal.as_deref()) }
pub(crate) fn canal_mut(sim: &mut SimState, id: BuildingId) -> Result<&mut Canal, SimError> {
    sim.buildings.buildings.iter_mut().find(|b| b.id == id).and_then(|b| b.canal.as_deref_mut()).ok_or_else(|| invalid("no such canal"))
}
pub fn end_drop(sim: &SimState, k: &Canal) -> Option<f32> {
    let level = k.status.as_ref()?.source_level;
    let end = k.end();
    (depth(sim, end) > crate::buildings::WET_DEPTH).then(|| (level - surface(sim, end)).max(0.0))
}
pub fn gate_owner(sim: &SimState, cell: Cell) -> Option<BuildingId> {
    sim.buildings.buildings.iter().find(|b| b.gates().iter().any(|g| g.cell == cell)).map(|b| b.id)
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Plan {
    pub cells: Vec<CanalCell>, pub source: Cell, pub level: f32, pub through: bool,
    pub too_deep: Vec<Cell>, pub expected: Vec<f32>, pub gate: Cell,
}
impl Plan {
    pub fn visits(&self) -> usize { self.cells.iter().map(|c| visits_for(c.ground, c.target)).sum() }
    pub fn labour_days(&self) -> f32 { dig_days(self.visits()) }
    pub fn mud(&self) -> f32 { self.visits() as f32 * MUD_PER_STEP }
    pub fn floatable_share(&self) -> f32 {
        if self.expected.is_empty() { 0.0 } else { self.expected.iter().filter(|&&d| d >= FLOAT_DEPTH).count() as f32 / self.expected.len() as f32 }
    }
}

fn visits_for(from: f32, target: f32) -> usize { ((from - target).max(0.0) / DIG_STEP - EPS).ceil().max(0.0) as usize }
fn owned(sim: &SimState, c: Cell, except: Option<BuildingId>) -> bool { cell_taken(sim, c, except, true) }

fn profile(sim: &SimState, route: &[Cell], bed0: f32) -> Vec<f32> {
    let mut bed = bed0;
    route.iter().enumerate().map(|(i, &c)| {
        bed = if i > 0 { (bed - BED_FALL).min(ground(sim, c) - MIN_COVER) } else { bed.min(ground(sim, c) - MIN_COVER.min(INTAKE_BELOW_LEVEL)) };
        bed
    }).collect()
}

pub fn plan(sim: &SimState, drawn: &[Cell]) -> Result<Plan, String> { plan_from(sim, drawn, None) }

pub fn plan_from(sim: &SimState, drawn: &[Cell], extending: Option<BuildingId>) -> Result<Plan, String> {
    if drawn.len() < if extending.is_some() { 1 } else { 2 } { return Err("Draw from water onto land".into()); }
    if !drawn.windows(2).all(|w| w[0].0.abs_diff(w[1].0) + w[0].1.abs_diff(w[1].1) == 1) { return Err("A canal can't turn on a corner".into()); }
    let n = sim.height.width();
    if drawn.iter().any(|c| c.0 >= n || c.1 >= n) { return Err("Outside the map".into()); }
    let mut seen = BTreeSet::new();
    if !drawn.iter().all(|c| seen.insert(*c)) { return Err("A canal can't cross itself".into()); }
    let start = drawn.iter().position(|&c| !is_water(sim, c)).ok_or("Draw onto land")?;
    if start == 0 && extending.is_none() { return Err("Must start at water".into()); }
    let last = drawn.iter().rposition(|&c| !is_water(sim, c)).expect("one land cell");
    let (mut route, through) = (drawn.to_vec(), last + 1 < drawn.len());
    let (source, level, bed0) = match extending.and_then(|id| canal(sim, id)) {
        Some(c) => {
            let end = c.route().last().copied().ok_or("Nothing to extend")?;
            if end.cell.0.abs_diff(route[0].0) + end.cell.1.abs_diff(route[0].1) != 1 { return Err("Draw on from the canal's end".into()); }
            (c.source, c.status.as_ref().map_or(surface(sim, c.source), |s| s.source_level), end.target - BED_FALL)
        }
        None => {
            let level = surface(sim, drawn[0]);
            let bed0 = level - INTAKE_BELOW_LEVEL;
            let approach = approach_channel(sim, drawn[0], bed0, &drawn[1..]);
            let source = approach.first().copied().unwrap_or(drawn[0]);
            if !approach.is_empty() { route.splice(0..0, approach); }
            (source, level, bed0)
        }
    };
    let shift = route.len() - drawn.len();
    let (start, last) = (start + shift, last + shift);
    if route.len() > MAX_CELLS { return Err(format!("Too long (max {MAX_CELLS} cells)")); }
    for &c in &route { check_cell_open(sim, c, extending, true)?; }
    let mut targets = vec![bed0; start];
    targets.extend(profile(sim, &route[start..=last], bed0));
    let mut bed = *targets.last().expect("one land cell");
    for &c in &route[last + 1..] { bed = (bed - BED_FALL).min(ground(sim, c)); targets.push(bed); }
    let too_deep = route.iter().zip(&targets).filter(|(c, t)| ground(sim, **c) - **t > MAX_CUT + EPS).map(|(&c, _)| c).collect();
    let expected = targets.iter().map(|&t| if through { (level - t).clamp(0.0, 0.3) } else { (level - t).max(0.0) }).collect();
    let cells = route.iter().zip(&targets).map(|(&cell, &target)| {
        let g = ground(sim, cell);
        CanalCell { cell, target, ground: g, row: 0, silt: 0.0, done: g <= target + EPS }
    }).collect();
    Ok(Plan { cells, source, level, through, too_deep, expected, gate: route[start] })
}

fn approach_channel(sim: &SimState, from: Cell, bed: f32, avoid: &[Cell]) -> Vec<Cell> {
    if ground(sim, from) <= bed + EPS { return Vec::new(); }
    let n = sim.height.width() as i64;
    let (mut prev, mut queue) = (HashMap::from([(from, from)]), VecDeque::from([(from, 0usize)]));
    while let Some((c, d)) = queue.pop_front() {
        if c != from && ground(sim, c) <= bed + EPS {
            let (mut path, mut at) = (vec![c], c);
            while prev[&at] != from { at = prev[&at]; path.push(at); }
            return path;
        }
        if d >= APPROACH_CELLS { continue; }
        for (dx, dy) in [(1i64, 0), (-1, 0), (0, 1), (0, -1)] {
            let (x, y) = (c.0 as i64 + dx, c.1 as i64 + dy);
            if x < 0 || y < 0 || x >= n || y >= n { continue; }
            let m = (x as usize, y as usize);
            if !prev.contains_key(&m) && !avoid.contains(&m) && is_water(sim, m) && !owned(sim, m, None) {
                prev.insert(m, c);
                queue.push_back((m, d + 1));
            }
        }
    }
    Vec::new()
}

pub fn check(plan: &Plan) -> Result<(), String> {
    if plan.too_deep.is_empty() { return Ok(()); }
    let over = plan.cells.iter().map(|c| c.ground - c.target - MAX_CUT).fold(0.0, f32::max);
    Err(format!("Land {:.1} m too high", (over * 10.0).ceil().max(1.0) / 10.0))
}

pub fn place(sim: &mut SimState, drawn: &[Cell]) -> Result<BuildingId, SimError> {
    sim.unlocked(BuildingKind::canal()).map_err(|e| invalid(e.to_string()))?;
    let p = plan(sim, drawn).map_err(invalid)?;
    check(&p).map_err(invalid)?;
    let first = *p.cells.iter().find(|c| c.cell == p.gate).expect("gate on route");
    let gate = Gate::new(first.cell, first.target, first.ground.max(p.level + GATE_FREEBOARD));
    let id = BuildingId(sim.buildings.fresh_id());
    let mut b = Building::new(id, BuildingKind::canal(), first.cell, Status::Complete);
    b.canal = Some(Box::new(Canal { cells: p.cells, source: p.source, gate, through: p.through, upkeep: true, history: VecDeque::new(), status: None, status_day: f64::NEG_INFINITY }));
    sim.work.add_complete(&b).map_err(|e| invalid(format!("{e:?}")))?;
    sim.buildings.buildings.push(b);
    Ok(id)
}

fn cell_work(sim: &SimState, k: &Canal, c: &CanalCell) -> bool {
    let h = ground(sim, c.cell);
    if !c.done { h > c.target + EPS } else { k.upkeep && h > c.target + SILT_TOLERANCE }
}
fn cell_task(sim: &SimState, k: &Canal, c: &CanalCell, upkeep: bool) -> bool { cell_work(sim, k, c) && c.done == upkeep }
pub fn cell_needs_work(sim: &SimState, id: BuildingId, cell: Cell, upkeep: bool) -> bool {
    canal(sim, id).is_some_and(|k| k.cells.iter().any(|c| c.cell == cell && cell_task(sim, k, c, upkeep)))
}
pub fn has_work_for(sim: &SimState, id: BuildingId, upkeep: bool) -> bool {
    canal(sim, id).is_some_and(|k| (!upkeep && (k.gate.pending.is_some() || crate::locks::pending(sim, k.gate.cell))) || k.cells.iter().any(|c| cell_task(sim, k, c, upkeep)))
}
pub fn has_work(sim: &SimState, id: BuildingId) -> bool {
    canal(sim, id).is_some_and(|k| k.gate.pending.is_some() || crate::locks::pending(sim, k.gate.cell) || k.cells.iter().any(|c| cell_work(sim, k, c)))
}
pub fn claimed(sim: &SimState, id: BuildingId, except: crate::colony::BeaverId) -> Vec<Cell> {
    claimed_work(sim, id, except, &[WorkKind::Dig, WorkKind::Gate])
}

pub fn next_job(sim: &SimState, id: BuildingId, taken: &[Cell], upkeep: bool) -> Option<(Cell, bool)> {
    let k = canal(sim, id)?;
    if !upkeep && crate::gates::visit_ready(sim, id, &k.gate) && !taken.contains(&k.gate.cell) && !crate::predators::in_view(sim, k.gate.cell) {
        return Some((k.gate.cell, true));
    }
    let gate = k.gate.cell;
    let open: Vec<&CanalCell> = k.cells.iter().filter(|c| !taken.contains(&c.cell) && cell_task(sim, k, c, upkeep) && !crate::predators::in_view(sim, c.cell)).collect();
    open.iter().filter(|c| !c.done && c.cell != gate).max_by_key(|c| (c.row == 0, k.cells.iter().position(|o| o.cell == c.cell)))
        .or_else(|| open.iter().find(|c| c.cell == gate && !c.done))
        .or_else(|| open.first())
        .map(|c| (c.cell, false))
}

pub fn dig(sim: &mut SimState, id: BuildingId, cell: Cell) -> Result<(), SimError> {
    let c = *canal(sim, id).ok_or_else(|| invalid("no such canal"))?.cells.iter().find(|c| c.cell == cell).ok_or_else(|| invalid("not a cell of this canal"))?;
    let step = (ground(sim, cell) - c.target).min(DIG_STEP);
    if step <= EPS { mark_done(sim, id, cell); return Ok(()); }
    for t in sim.resources.at(cell).filter(|r| r.kind.is_tree()).map(|r| r.id).collect::<Vec<_>>() {
        if sim.resources.fell(t, f32::MAX / 4.0).is_err() {
            if let Some(r) = sim.resources.get(t) { crate::resources::note("GONE", r, format_args!("why=cleared for a canal")); }
            sim.resources.keep(|r| r.id != t);
        }
    }
    sim.water.dig_canal(&mut sim.height, &[cell], step).map_err(|e| invalid(e.to_string()))?;
    spoil(sim, cell, MUD_PER_STEP * step / DIG_STEP);
    if canal(sim, id).is_some_and(|k| gate_cells(k).contains(&cell)) { apply_gate(sim, id)?; }
    if ground(sim, cell) <= c.target + EPS { mark_done(sim, id, cell); }
    Ok(())
}

fn mark_done(sim: &mut SimState, id: BuildingId, cell: Cell) {
    if let Ok(k) = canal_mut(sim, id) {
        for c in k.cells.iter_mut().filter(|c| c.cell == cell) { (c.done, c.silt) = (true, 0.0); }
    }
}

pub fn turn_gate(sim: &mut SimState, id: BuildingId) -> Result<(), SimError> {
    finish_gate(&mut canal_mut(sim, id)?.gate);
    apply_gate(sim, id)
}

pub fn gate_cells(k: &Canal) -> Vec<Cell> {
    let g = k.gate.cell;
    std::iter::once(g).chain(k.cells.iter().filter(|c| c.row > 0 && c.cell.0.abs_diff(g.0) + c.cell.1.abs_diff(g.1) == 1).map(|c| c.cell)).collect()
}

fn apply_gate(sim: &mut SimState, id: BuildingId) -> Result<(), SimError> {
    let k = canal(sim, id).ok_or_else(|| invalid("no such canal"))?;
    apply_across(sim, k.gate, &gate_cells(k))
}

fn spoil(sim: &mut SimState, cell: Cell, mud: f32) {
    let n = sim.height.width() as i64;
    let best = (-1i64..=1).flat_map(|dy| (-1i64..=1).map(move |dx| (dx, dy)))
        .filter(|&d| d != (0, 0))
        .filter_map(|(dx, dy)| {
            let (x, y) = (cell.0 as i64 + dx, cell.1 as i64 + dy);
            (x >= 0 && y >= 0 && x < n && y < n).then_some((x as usize, y as usize))
        })
        .filter(|&c| !is_water(sim, c) && !sim.water.canal.get(c.0, c.1).copied().unwrap_or(false) && !owned(sim, c, None))
        .max_by(|&a, &b| ground(sim, a).total_cmp(&ground(sim, b)));
    let Some(bank) = best else { return };
    let kind = crate::content::content().roles.spoil;
    let near = sim.resources.items().iter().find(|r| r.kind == kind && r.cell.0.abs_diff(bank.0) <= 1 && r.cell.1.abs_diff(bank.1) <= 1).map(|r| r.id);
    match near.and_then(|id| sim.resources.get_mut(id).ok()) {
        Some(r) => { r.amount += mud; r.max_amount += mud; }
        None => { sim.resources.add(kind, bank, mud); }
    }
}

pub fn grow(sim: &mut SimState, id: BuildingId, growth: CanalGrowth, cells: &[Cell]) -> Result<(), SimError> {
    let (new, through) = plan_growth(sim, id, growth, cells).map_err(invalid)?;
    let k = canal_mut(sim, id)?;
    k.cells = new;
    k.through |= through;
    k.invalidate_status();
    if let Some(first) = k.cells.iter().find(|c| c.cell == k.gate.cell) { k.gate.floor = first.target; }
    apply_gate(sim, id)?;
    Ok(())
}

pub fn plan_growth(sim: &SimState, id: BuildingId, growth: CanalGrowth, cells: &[Cell]) -> Result<(Vec<CanalCell>, bool), String> {
    let k = canal(sim, id).ok_or("no such canal")?;
    let (mut out, mut through) = (k.cells.clone(), k.through);
    match growth {
        CanalGrowth::Deepen => {
            let mut any = false;
            for c in out.iter_mut().filter(|c| cells.is_empty() || cells.contains(&c.cell)) {
                let t = c.target - DIG_STEP;
                if c.ground - t <= MAX_CUT + EPS { (c.target, c.done, any) = (t, false, true); }
            }
            if !any { return Err(format!("Already {MAX_CUT} m deep")); }
        }
        CanalGrowth::Widen => {
            if k.width() as usize >= MAX_WIDTH { return Err("Already as wide as it goes".into()); }
            let route: Vec<CanalCell> = k.route().copied().collect();
            let side = |sign: i64| -> Vec<(CanalCell, Cell)> {
                route.iter().filter(|c| cells.is_empty() || cells.contains(&c.cell)).filter_map(|c| {
                    let i = route.iter().position(|o| o.cell == c.cell)?;
                    let (a, b) = (route[i.saturating_sub(1)].cell, route[(i + 1).min(route.len() - 1)].cell);
                    let (dx, dy) = (b.0 as i64 - a.0 as i64, b.1 as i64 - a.1 as i64);
                    let (px, py) = if dx.abs() >= dy.abs() { (0, dx.signum() * sign) } else { (-dy.signum() * sign, 0) };
                    let (x, y) = (c.cell.0 as i64 + px, c.cell.1 as i64 + py);
                    let n = (x as usize, y as usize);
                    (x >= 0 && y >= 0 && !out.iter().any(|o| o.cell == n) && !is_water(sim, n) && check_cell_open(sim, n, Some(id), true).is_ok()).then_some((*c, n))
                }).collect()
            };
            let (left, right) = (side(1), side(-1));
            let cost = |v: &[(CanalCell, Cell)]| v.iter().map(|(c, n)| ground(sim, *n) - c.target).sum::<f32>();
            let pick = if left.len() > right.len() || (left.len() == right.len() && cost(&left) <= cost(&right)) { left } else { right };
            if pick.is_empty() { return Err("No room beside the canal".into()); }
            for (c, n) in pick {
                if out.iter().any(|o| o.cell == n) { continue; }
                let g = ground(sim, n);
                let target = c.target.max(g - MAX_CUT);
                out.push(CanalCell { cell: n, target, ground: g, row: 1, silt: 0.0, done: g <= target + EPS });
            }
        }
        CanalGrowth::Extend => {
            let drawn: Vec<Cell> = cells.iter().copied().filter(|&c| c != k.end()).collect();
            let p = plan_from(sim, &drawn, Some(id))?;
            check(&p)?;
            if out.len() + p.cells.len() > MAX_CELLS * MAX_WIDTH { return Err(format!("Too long (max {MAX_CELLS} cells)")); }
            through |= p.through;
            out.extend(p.cells);
        }
    }
    Ok((out, through))
}

pub fn dig_days(visits: usize) -> f32 { visits as f32 * DIG_WORK / crate::work::LABOUR_PER_DAY }
pub fn growth_labour(sim: &SimState, id: BuildingId, growth: CanalGrowth, cells: &[Cell]) -> Result<f32, String> {
    let total = |v: &[CanalCell]| -> usize { v.iter().map(|c| visits_for(ground(sim, c.cell), c.target)).sum() };
    let before = canal(sim, id).map_or(0, |k| total(&k.cells));
    let (after, _) = plan_growth(sim, id, growth, cells)?;
    Ok(dig_days(total(&after).saturating_sub(before)))
}
pub fn gates_unlocked(sim: &SimState) -> bool { crate::gates::unlocked(sim) }
pub fn set_gate(sim: &mut SimState, id: BuildingId, setting: GateSetting) -> Result<(), SimError> {
    if canal(sim, id).is_some_and(|k| !k.gate.built) { crate::gates::check_unlocked(sim).map_err(invalid)?; }
    crate::gates::request(&mut canal_mut(sim, id)?.gate, setting)
}
pub fn spoil_good() -> Option<crate::buildings::Good> { crate::content::content().roles.spoil.good() }

pub fn step(sim: &mut SimState, dt: f32) {
    let today = sim.time_days.floor();
    for id in sim.buildings.buildings.iter().filter(|b| b.canal.is_some()).map(|b| b.id).collect::<Vec<_>>() {
        silt(sim, id, dt);
        let Some(k) = canal(sim, id) else { continue };
        if k.status_day >= today { continue; }
        let (status, new_day) = (measure(sim, k), k.status_day.is_finite());
        let Ok(k) = canal_mut(sim, id) else { continue };
        if new_day {
            push_history(&mut k.history, HISTORY_DAYS, CanalDay { floatable: status.floatable_share(), inflow: status.inflow, outflow: status.outflow, silted: status.silted as f32 });
        }
        (k.status, k.status_day) = (Some(status), today);
    }
}

pub fn running(sim: &SimState, c: Cell) -> bool {
    let (u, v) = sim.water.velocity.get(c.0, c.1).copied().unwrap_or((0.0, 0.0));
    depth(sim, c) > WET_DEPTH && u.hypot(v) > 0.02
}
pub fn silt_per_day(sim: &SimState, c: Cell) -> f32 { SILT_PER_DAY * if running(sim, c) { SILT_RUNNING_FACTOR } else { 1.0 } }

fn silt(sim: &mut SimState, id: BuildingId, dt: f32) {
    let settle = (sim.time_days / SILT_SETTLES_EVERY_DAYS).floor() > ((sim.time_days - dt as f64) / SILT_SETTLES_EVERY_DAYS).floor();
    let Some(k) = canal(sim, id) else { return };
    let (mut fills, mut silts) = (Vec::new(), Vec::with_capacity(k.cells.len()));
    for c in &k.cells {
        if ground(sim, c.cell) >= c.ground - EPS { silts.push(c.silt); continue; }
        let mut s = c.silt + silt_per_day(sim, c.cell) * dt;
        if settle && s >= SILT_STEP {
            let steps = (s / SILT_STEP).floor();
            fills.push((c.cell, SILT_STEP * steps, c.ground));
            s -= SILT_STEP * steps;
        }
        silts.push(s);
    }
    let across = gate_cells(k);
    let mut regate = k.gate.built && k.cells.iter().any(|c| across.contains(&c.cell) && sim.water.gate.get(c.cell.0, c.cell.1) == Some(&false) && ground(sim, c.cell) < c.ground - EPS);
    if let Ok(k) = canal_mut(sim, id) { for (c, s) in k.cells.iter_mut().zip(silts) { c.silt = s; } }
    for (cell, amount, g) in fills { let _ = sim.water.silt_canal(&mut sim.height, cell, amount, g); regate |= across.contains(&cell); }
    if regate { let _ = apply_gate(sim, id); }
}

fn measure(sim: &SimState, k: &Canal) -> CanalStatus {
    let mut s = CanalStatus { cells: k.cells.len(), source_level: surface(sim, k.source), ..Default::default() };
    let mut total = 0.0;
    for c in &k.cells {
        let (d, h) = (depth(sim, c.cell), ground(sim, c.cell));
        s.dug += usize::from(h < c.ground - EPS);
        s.silted += usize::from(c.done && h > c.target + SILT_TOLERANCE);
        if d >= WET_DEPTH { s.wet += 1; total += d; }
        s.floatable += usize::from(d >= FLOAT_DEPTH);
        s.max_depth = s.max_depth.max(d);
    }
    s.mean_depth = if s.wet > 0 { total / s.wet as f32 } else { 0.0 };
    let q = |c: Cell| sim.water.discharge.get(c.0, c.1).copied().unwrap_or(0.0) * 86_400.0;
    (s.inflow, s.outflow) = (q(k.gate.cell), q(k.end()));
    s
}

pub fn health(sim: &SimState, k: &Canal) -> f32 {
    let done: Vec<&CanalCell> = k.cells.iter().filter(|c| c.done).collect();
    if done.is_empty() { 1.0 } else { done.iter().filter(|c| ground(sim, c.cell) <= c.target + SILT_TOLERANCE).count() as f32 / done.len() as f32 }
}
