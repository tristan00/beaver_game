//! The one founding start: two adults and two kits swim in along a river. Nothing is built and
//! nothing is chosen for them; the player scouts, places a burrow (the adults dig it) and a dam
//! (the adults fell trees and haul the wood), and the pond fills. Built directly, never simulated
//! forward: the same seed gives an identical `SimState`.

use crate::SimState;

/// Why a start could not be built.
#[derive(Clone, Debug, PartialEq)]
pub enum StartError {
    /// The generated world has no usable spot (dry ground beside the river with trees nearby).
    NoSite,
    /// A step of building the start failed; the message names it.
    Build(String),
}

impl std::fmt::Display for StartError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StartError::NoSite => write!(f, "no founding site beside the river"),
            StartError::Build(m) => write!(f, "could not build the start: {m}"),
        }
    }
}
impl std::error::Error for StartError {}

/// Removes every disabled system's state from a start (`Features` says which): no rival
/// neighbours or arrivals, no predators, no scouting or guard experience. The sim never fires
/// them either way (`SimState::apply_features`); this keeps them out of the state a screen reads.
pub(crate) fn strip_disabled(sim: &mut SimState) {
    use crate::features::System;
    let f = sim.features;
    if !f.enabled(System::Rivals) || !f.enabled(System::Trade) {
        sim.region = crate::region::Region::none(sim.seed);
    }
    if !f.enabled(System::Predators) {
        sim.ecology.predators.clear();
    }
    for job in crate::colony::Job::ALL.into_iter().filter(|j| !f.allows_job(*j)) {
        sim.colony.experience[job.index()] = 0.0;
        if job == crate::colony::Job::Scout {
            sim.colony.scouted = 0.0;
        }
    }
    sim.buildings.buildings.retain(|b| f.allows_building(b.kind));
}

/// Dams tried, in cells above the river bed.
const DAM_HEIGHTS: [f32; 7] = [0.5, 0.6, 0.8, 1.0, 1.2, 1.5, 2.0];
/// A first pond of this many cells: big enough to see, small enough to fill in days.
const POND_AREA: usize = 150;
const POND_AREA_RANGE: std::ops::RangeInclusive<usize> = 60..=250;
/// The valley floor is clay: seepage is a quarter of the default, so a river can hold a pond.
/// Evaporation and seepage then take about 0.02 depth per wet cell per day, so the river above
/// the dam is given one unit of inflow per `CELLS_PER_INFLOW` pond cells (half the steady-state
/// capacity, for the seasons) for the pond to stay full.
pub(crate) const CLAY_SEEPAGE: f32 = 0.01;
/// Springs are put on at most this many river cells, found by following the river at most `RIVER_WALK` cells.
#[cfg(any(test, feature = "fixtures"))]
const MAX_SPRINGS: usize = 20;
const RIVER_WALK: usize = 60;
#[cfg(any(test, feature = "fixtures"))]
const CELLS_PER_INFLOW: f32 = 15.0;
const MAX_DAM_CELLS: usize = 10;
/// Start 1 is a flowing stream a dam visibly changes. Its planned dam must raise the water at least
/// `START_RISE` above the river as it runs once the springs have settled, and flood at least
/// `START_NEW` cells (and a quarter of its pond) that are dry ground now.
pub const START_RISE: f32 = 0.6;
pub const START_NEW: usize = 40;
/// The river must fall at least `MIN_DROP` over the `REACH` cells either side of the dam (bed
/// before, water surface once settled): a filled flat falls about a tenth of that, and a dam
/// there would sit in standing water.
pub const MIN_DROP: f32 = 0.1;
pub const REACH: usize = 6;
/// Start 1's springs bring the full pond this many times its daily loss (spring evaporation plus
/// clay seepage, per cell): enough to fill it, little enough that the river stays in its bed.
const START_INFLOW_MARGIN: f32 = 1.5;
/// Start 1's springs sit on at most this many river cells just above the dam.
const SILL_SPRINGS: usize = 6;
/// Dam sites settled and checked before the best one tried is taken anyway.
const MAX_TRIES: usize = 16;
/// Dam lines are looked for this far inside the map.
const EDGE_MARGIN: usize = 4;
/// The camp is this far from the dam centre, this close to the river bank, with this many trees about.
const CAMP_DAM_DIST: std::ops::RangeInclusive<usize> = 5..=16;
const BANK_MAX: usize = 7;
const TREES_WANTED: usize = 10;
const TREE_RADIUS: usize = 14;
/// Kits are this many days old (under a year).
const KIT_AGE_DAYS: f32 = 150.0;

pub(crate) fn dist2(a: Cell, b: Cell) -> usize {
    a.0.abs_diff(b.0).pow(2) + a.1.abs_diff(b.1).pow(2)
}

/// Where start 1 camps and the dam it is meant to build.
struct RiverPlan {
    camp: Cell,
    dam: Vec<Cell>,
    height: f32,
}

/// Straight cross-river dam lines with a pond of a sensible size, best first (score, cells,
/// height). The score prefers a pond near `POND_AREA`, a short dam and a quick fill.
fn dam_candidates(sim: &SimState) -> Vec<(f32, Vec<Cell>, f32)> {
    let size = sim.world.size;
    let is_river = |x: usize, y: usize| sim.world.river.get(x, y).copied().unwrap_or(false);
    let up = upstream_map(sim);
    let bed = |c: Cell| sim.height.get(c.0, c.1).copied().unwrap_or(0.0);
    let mut out = Vec::new();
    for y in EDGE_MARGIN..size - EDGE_MARGIN {
        for x in EDGE_MARGIN..size - EDGE_MARGIN {
            if !is_river(x, y) {
                continue;
            }
            if reach_drop(sim, &up, (x, y), bed) < MIN_DROP {
                continue;
            }
            for horizontal in [true, false] {
                let at = |t: isize| if horizontal { ((x as isize + t) as usize, y) } else { (x, (y as isize + t) as usize) };
                let reach = |dir: isize| (1..12).find(|&k| !is_river(at(dir * k).0, at(dir * k).1)).unwrap_or(12);
                let (left, right) = (-reach(-1), reach(1));
                let low = (left + 1..right).map(|t| bed(at(t))).fold(f32::INFINITY, f32::min);
                for height in DAM_HEIGHTS {
                    // Both banks, plus bank on each side until the line ties into ground above
                    // the pond level, so the water cannot walk round its ends.
                    let ties_in = |t: isize| bed(at(t)) >= low + height;
                    let (Some(l), Some(r)) = ((0..MAX_DAM_CELLS as isize).map(|k| left - k).find(|&t| ties_in(t - 1)), (0..MAX_DAM_CELLS as isize).map(|k| right + k).find(|&t| ties_in(t + 1))) else { continue };
                    let cells: Vec<Cell> = (l..=r).map(at).collect();
                    if cells.len() > MAX_DAM_CELLS {
                        continue;
                    }
                    let Some(p) = crate::dam_preview::preview(sim, &cells, height) else { continue };
                    let area = p.flooded.len();
                    if p.rise >= START_RISE && POND_AREA_RANGE.contains(&area) && holds(sim, &cells, &p.flooded) {
                        let score = area.abs_diff(POND_AREA) as f32 + cells.len() as f32 * 20.0;
                        out.push((score, cells.clone(), height));
                    }
                }
            }
        }
    }
    out.sort_by(|a, b| a.0.total_cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
    out
}

/// Whether a pond of `flooded` cells is held by the dam: it stays off the map edge and off the
/// river below the dam (a dam the water walks round floods its own tailwater).
fn holds(sim: &SimState, dam: &[Cell], flooded: &[Cell]) -> bool {
    let size = sim.world.size;
    let pond: std::collections::HashSet<Cell> = flooded.iter().copied().collect();
    if flooded.iter().any(|&(x, y)| x == 0 || y == 0 || x + 1 == size || y + 1 == size) {
        return false;
    }
    let Some(mut at) = dam_river_cell(sim, dam) else { return false };
    for _ in 0..2 * REACH {
        match sim.world.downstream(at.0, at.1) {
            Some(d) if !dam.contains(&d) => {
                if pond.contains(&d) {
                    return false;
                }
                at = d;
            }
            Some(d) => at = d,
            None => break,
        }
    }
    true
}

/// For each river cell (by index), the river cell draining into it with the largest catchment:
/// the stem upstream.
fn upstream_map(sim: &SimState) -> Vec<Option<Cell>> {
    let size = sim.world.size;
    let is_river = |(x, y): Cell| sim.world.river.get(x, y).copied().unwrap_or(false);
    let acc = |(x, y): Cell| sim.world.flow_acc.get(x, y).copied().unwrap_or(0);
    let mut up: Vec<Option<Cell>> = vec![None; size * size];
    for c in (0..size * size).map(|i| (i % size, i / size)).filter(|&c| is_river(c)) {
        if let Some(d) = sim.world.downstream(c.0, c.1).filter(|&d| is_river(d)) {
            let slot = &mut up[d.1 * size + d.0];
            if slot.is_none_or(|u| (acc(c), c) > (acc(u), u)) {
                *slot = Some(c);
            }
        }
    }
    up
}

/// How far `level` falls along the river from `REACH` cells above `at` to `REACH` cells below it
/// (fewer where the river starts or leaves the map).
fn reach_drop(sim: &SimState, up: &[Option<Cell>], at: Cell, level: impl Fn(Cell) -> f32) -> f32 {
    let size = sim.world.size;
    let (mut top, mut bottom) = (at, at);
    for _ in 0..REACH {
        if let Some(u) = up[top.1 * size + top.0] {
            top = u;
        }
        if let Some(d) = sim.world.downstream(bottom.0, bottom.1) {
            bottom = d;
        }
    }
    level(top) - level(bottom)
}

/// The dam's river cell: the lowest river cell of the line.
fn dam_river_cell(sim: &SimState, dam: &[Cell]) -> Option<Cell> {
    let bed = |c: &Cell| sim.height.get(c.0, c.1).copied().unwrap_or(f32::INFINITY);
    dam.iter().copied().filter(|&(x, y)| sim.world.river.get(x, y) == Some(&true)).min_by(|a, b| bed(a).total_cmp(&bed(b)))
}

/// What a planned dam does to the river as it runs now.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DamEffect {
    /// Rise of the water at the dam (the HUD preview's "Water rises").
    pub rise: f32,
    /// Cells the pond would cover.
    pub flooded: usize,
    /// Of those, cells that are dry ground now (not river, lake or standing water).
    pub newly_flooded: usize,
    /// Fall of the water surface along the river through the dam site (see `MIN_DROP`).
    pub drop: f32,
}

impl DamEffect {
    /// How near this comes to `is_visible` (1 or more when rise and new ground both suffice).
    fn closeness(&self) -> f32 {
        (self.rise / START_RISE).min(self.newly_flooded as f32 / START_NEW.max(self.flooded / 4) as f32)
    }

    /// Whether this is the flowing stream a dam visibly changes that start 1 is built around.
    pub fn is_visible(&self) -> bool {
        self.rise >= START_RISE && self.newly_flooded >= START_NEW.max(self.flooded / 4) && self.drop >= MIN_DROP
    }
}

/// What a dam of `height` on `dam` does to the river in `sim` as it stands. `None` if the line is
/// not a valid dam or crosses no river.
pub fn dam_effect(sim: &SimState, dam: &[Cell], height: f32) -> Option<DamEffect> {
    let p = crate::dam_preview::preview(sim, dam, height)?;
    let wet = |&&(x, y): &&Cell| {
        sim.water.depth.get(x, y).is_some_and(|d| *d >= crate::buildings::WET_DEPTH)
            || sim.world.river.get(x, y).copied().unwrap_or(false)
            || sim.world.lake.get(x, y).copied().unwrap_or(false)
    };
    let surface = |(x, y): Cell| sim.height.get(x, y).copied().unwrap_or(0.0) + sim.water.depth.get(x, y).copied().unwrap_or(0.0);
    let drop = reach_drop(sim, &upstream_map(sim), dam_river_cell(sim, dam)?, surface);
    Some(DamEffect { rise: p.rise, flooded: p.flooded.len(), newly_flooded: p.flooded.iter().filter(|c| !wet(c)).count(), drop })
}

/// Dam sites a dam visibly changes (`DamEffect::is_visible`) on separate streams: two sites count
/// as separate when neither lies within `SEPARATE_REACH` river cells downstream of the other. The
/// map checks use it to make sure a map offers more than one river worth damming.
pub fn dammable_streams(sim: &SimState) -> Vec<Cell> {
    const SEPARATE_REACH: usize = 40;
    let below = |from: Cell, to: Cell| {
        let mut at = from;
        for _ in 0..SEPARATE_REACH {
            match sim.world.downstream(at.0, at.1) {
                Some(d) if d == to => return true,
                Some(d) => at = d,
                None => return false,
            }
        }
        false
    };
    let mut picked: Vec<Cell> = Vec::new();
    for (_, dam, height) in dam_candidates(sim) {
        let (Some(effect), Some(at)) = (dam_effect(sim, &dam, height), dam_river_cell(sim, &dam)) else { continue };
        if effect.is_visible() && picked.iter().all(|&p| !below(at, p) && !below(p, at) && dist2(p, at) > 64) {
            picked.push(at);
        }
    }
    picked
}

/// Runs the water alone until the rivers carry what the springs now give, then starts its clock
/// and books afresh (as `Water::from_world` does for the generated springs).
fn settle_water(sim: &mut SimState) {
    use crate::water::{SETTLE_DAYS, SETTLE_DT};
    for _ in 0..(SETTLE_DAYS / SETTLE_DT).round() as usize {
        sim.water.step(&sim.height, SETTLE_DT);
    }
    sim.water.time_days = 0.0;
    (sim.water.added_volume, sim.water.lost_volume) = (0.0, 0.0);
}

/// Depth a full pond loses per cell per day in spring: evaporation plus clay seepage.
fn pond_loss() -> f32 {
    crate::water::EVAPORATION * crate::water::Season::Spring.evap_mult() + CLAY_SEEPAGE
}

/// Dry ground within reach of the dam, outside the future pond, beside the river with trees
/// around; the cell nearest the dam among those qualifying (ties by cell order).
fn camp_near(sim: &SimState, dam: &[Cell], height: f32) -> Option<Cell> {
    let size = sim.world.size;
    let pond: std::collections::HashSet<Cell> =
        crate::dam_preview::preview(sim, dam, height)?.flooded.into_iter().collect();
    let mid = dam[dam.len() / 2];
    let wet = |x: usize, y: usize| {
        sim.water.depth.get(x, y).is_some_and(|d| *d > crate::buildings::WET_DEPTH)
            || sim.world.river.get(x, y).copied().unwrap_or(false)
            || sim.world.lake.get(x, y).copied().unwrap_or(false)
            || pond.contains(&(x, y))
    };
    let near_cells = |c: Cell, r: usize| {
        (c.1.saturating_sub(r)..=(c.1 + r).min(size - 1)).flat_map(move |y| (c.0.saturating_sub(r)..=(c.0 + r).min(size - 1)).map(move |x| (x, y)))
    };
    let trees: Vec<Cell> = sim.resources.items.iter().filter(|r| r.kind == crate::resources::ResourceKind::Tree).map(|r| r.cell).collect();
    near_cells(mid, 16)
        .filter(|&c| c.0 >= 3 && c.1 >= 3 && c.0 + 3 < size && c.1 + 3 < size)
        .filter(|&c| CAMP_DAM_DIST.contains(&(dist2(c, mid).isqrt())))
        .filter(|&c| near_cells(c, 2).all(|(x, y)| !wet(x, y)))
        .filter(|&c| near_cells(c, BANK_MAX).any(|(x, y)| sim.world.river.get(x, y).copied().unwrap_or(false)))
        .filter(|&c| trees.iter().filter(|&&t| dist2(c, t) <= TREE_RADIUS * TREE_RADIUS && dist2(c, t) > 9).count() >= TREES_WANTED)
        .min_by_key(|&c| (dist2(c, mid), c.1, c.0))
}

/// The best (camp, dam) pair for a state as `SimState::new` leaves it (with clay seepage). Leaves
/// `sim` with the springs that keep that pond full and the water settled into them: the first
/// candidate whose settled river the dam still visibly changes, else the one tried that comes
/// nearest (`DamEffect::closeness`).
fn river_plan(sim: &mut SimState) -> Option<RiverPlan> {
    let base = sim.water.clone();
    let mut best: Option<(f32, RiverPlan, crate::water::Water)> = None;
    let mut tries = 0;
    // Settling is dear: dams across the world's narrows go first, then the ones that already
    // visibly change the river before the springs are on (best score first within each group).
    let mut candidates = dam_candidates(sim);
    let at_narrows = |dam: &[Cell]| sim.world.narrows.is_some_and(|nw| dam.iter().any(|&c| dist2(c, nw) <= 2));
    candidates.sort_by_key(|(_, dam, height)| (!at_narrows(dam), !dam_effect(sim, dam, *height).is_some_and(|e| e.is_visible())));
    for (_, dam, height) in candidates {
        if tries == MAX_TRIES {
            break;
        }
        if camp_near(sim, &dam, height).is_none() {
            continue;
        }
        tries += 1;
        sim.water = base.clone();
        let fed = feed_at_sill(sim, &dam, height);
        settle_water(sim);
        let placeable = sim.dam_site(&dam).is_ok();
        let (Ok(()), true, Some(effect), Some(camp)) = (fed, placeable, dam_effect(sim, &dam, height), camp_near(sim, &dam, height)) else { continue };
        let plan = RiverPlan { camp, dam, height };
        if effect.is_visible() {
            return Some(plan);
        }
        if best.as_ref().is_none_or(|b| effect.closeness() > b.0) {
            best = Some((effect.closeness(), plan, sim.water.clone()));
        }
    }
    let (_, plan, water) = best?;
    sim.water = water;
    Some(plan)
}

/// The river cells the family swims in on: one per beaver, packed around the river cell nearest
/// the landing bank (nearest to that anchor first, ties by cell order). Cells with a walkable
/// route to the landing come first, so a pocket of river walled in by cliffs does not strand
/// them; a landing buried in forest (no route until trees fall) falls back to plain river cells.
fn swim_cells(sim: &SimState, landing: Cell, n: usize) -> Vec<Cell> {
    use crate::nav::Goal;
    let size = sim.world.size;
    let mut nav = sim.nav.clone();
    nav.rebuild(sim);
    let who = sim.colony.beavers[0].id;
    let mut river: Vec<Cell> = (0..size * size)
        .map(|i| (i % size, i / size))
        .filter(|&(x, y)| sim.world.river.get(x, y).copied().unwrap_or(false))
        .collect();
    river.sort_by_key(|&c| (dist2(c, landing), c.1, c.0));
    let linked: Vec<Cell> = river.iter().copied().filter(|&c| nav.find_path(who, c, Goal::Beside(landing)).is_some()).collect();
    let pool = if linked.len() >= n { linked } else { river };
    let Some(&anchor) = pool.first() else { return Vec::new() };
    let mut pack = pool;
    pack.sort_by_key(|&c| (dist2(c, anchor), c.1, c.0));
    pack.truncate(n);
    pack
}

/// A river cell within this many cells of the settling cell is where the family swims in from;
/// with none that close they start on the cell itself.
const SWIM_REACH: usize = 12;

/// Founds the colony at `cell`: two adults and two kits (swimming in from the river if one is
/// near, else standing on the cell) with a lodge, and opens the cell's valley and its
/// neighbours. Errors (state unchanged) if the colony is already founded, `cell` is off the map,
/// in a locked valley or in water.
/// Days of food the founding family carries in.
pub const START_LARDER_DAYS: f32 = 30.0;

pub(crate) fn settle(sim: &mut SimState, cell: Cell) -> Result<(), crate::SimError> {
    let invalid = |m: String| crate::SimError::InvalidCommand(m);
    if sim.colony.is_founded() {
        return Err(invalid("the colony is already founded".into()));
    }
    let valley = sim.world.valley_of(cell).ok_or_else(|| invalid(format!("cell {cell:?} is off the map")))?;
    if !sim.world.is_open(valley) {
        return Err(invalid(format!("cell {cell:?} is in a valley that is not open yet")));
    }
    if sim.water.depth.get(cell.0, cell.1).is_some_and(|d| *d > crate::buildings::WET_DEPTH)
        || sim.world.river.get(cell.0, cell.1).copied().unwrap_or(false)
        || sim.world.lake.get(cell.0, cell.1).copied().unwrap_or(false)
    {
        return Err(invalid(format!("cell {cell:?} is water: beavers cannot settle there")));
    }
    let land = crate::WorldLand { world: &sim.world, depth: &sim.water.depth };
    sim.colony.found((cell.0 as f32 + 0.5, cell.1 as f32 + 0.5), &land).map_err(|e| invalid(format!("{e:?}")))?;
    for b in sim.colony.beavers.iter_mut().filter(|b| b.stage == crate::colony::Stage::Yearling) {
        b.age_days = KIT_AGE_DAYS;
        b.stage = crate::colony::Stage::Kit;
    }
    let swim = swim_cells(sim, cell, sim.colony.beavers.len());
    let near = swim.first().is_some_and(|&c| dist2(c, cell) <= SWIM_REACH * SWIM_REACH);
    if near && swim.len() == sim.colony.beavers.len() {
        for (b, c) in sim.colony.beavers.iter_mut().zip(swim) {
            b.pos = (c.0 as f32 + 0.5, c.1 as f32 + 0.5);
        }
    }
    // The family brings a small larder of food (enough for START_LARDER_DAYS) and nothing else:
    // no wood, and nothing in any paw. It buys time to set up foraging before hunger bites.
    let larder = crate::hud::food_need_per_day(sim) * START_LARDER_DAYS;
    sim.colony.food = sim.buildings.stores.deposit(crate::buildings::Good::Food, larder).map_err(|e| invalid(e.to_string()))?;
    sim.colony.wood = 0.0;
    // Founding opens the landing's valley and its neighbours.
    sim.world.unlock(cell).map_err(|e| invalid(format!("{e:?}")))?;
    strip_disabled(sim);
    crate::agents::place_beavers(sim);
    Ok(())
}

/// The new game: the world with its river, no colony yet, no building of any kind and no dam.
/// The player picks the settling cell (`Command::Settle`, which founds two adults and two kits
/// swimming in beside it), then places a burrow and a dam (`SimState::place_site`,
/// `SimState::order_dam`). Beavers act only on orders: `auto_organise` is off, so the adults dig,
/// fell and haul only once the player assigns them (`Command::Assign`, `Command::FillSlot`); the
/// kits never work.
///
/// Returns the state and the dam and camp the game is designed around (the camp leaves its
/// landing dry). The dam sits where the river runs downhill, not on a flat where it would stand.
/// Springs above it are added so the river can fill that pond against evaporation and seepage,
/// and the water is settled into them before day 0, so the river runs as it will.
fn build_river(seed: u64) -> Result<(SimState, DamSite), StartError> {
    build_river_on(SimState::new(seed))
}

/// The start on any world: picks the designed dam and camp and settles the water (see `build_river`).
fn build_river_on(mut sim: SimState) -> Result<(SimState, DamSite), StartError> {
    sim.water.config.seepage = CLAY_SEEPAGE;
    let plan = river_plan(&mut sim).ok_or(StartError::NoSite)?;
    // The designed camp must be settleable.
    if let Some(v) = sim.world.valley_of(plan.camp) {
        sim.world.valleys[v.0 as usize].open = true;
    }
    sim.work.auto_organise = false;
    Ok((sim, DamSite { cells: plan.dam, height: plan.height, camp: plan.camp }))
}

/// Tops up the river above the planned dam so the pond can fill and stay full: steady springs
/// on the river cells that flow into it, nearest first (under the water if the river rises inside
/// the pond). They sit in the channel, so before the dam
/// the extra water simply runs past.
#[cfg(any(test, feature = "fixtures"))]
pub(crate) fn feed_river(sim: &mut SimState, dam: &[Cell], height: f32) -> Result<(), StartError> {
    let p = crate::dam_preview::preview(sim, dam, height).ok_or(StartError::NoSite)?;
    let needed = p.flooded.len() as f32 / CELLS_PER_INFLOW - p.pressure;
    if needed <= 0.0 {
        return Ok(());
    }
    let pond: std::collections::HashSet<Cell> = p.flooded.iter().chain(dam).copied().collect();
    let size = sim.world.size;
    let mut upstream: Vec<(usize, Cell)> = (0..size * size)
        .map(|i| (i % size, i / size))
        .filter(|&(x, y)| sim.world.river.get(x, y).copied().unwrap_or(false) && !pond.contains(&(x, y)))
        .filter_map(|start| {
            let mut at = start;
            (1..=RIVER_WALK).find_map(|steps| {
                at = sim.world.downstream(at.0, at.1)?;
                pond.contains(&at).then_some((steps, start))
            })
        })
        .collect();
    upstream.sort();
    upstream.truncate(MAX_SPRINGS);
    let mut cells: Vec<Cell> = upstream.into_iter().map(|(_, c)| c).collect();
    if cells.is_empty() {
        // The river rises inside the pond: springs under the water, spread over the flooded cells.
        let mut flooded = p.flooded.clone();
        flooded.sort_by_key(|&(x, y)| (y, x));
        let n = MAX_SPRINGS.min(flooded.len());
        cells = (0..n).map(|k| flooded[k * flooded.len() / n]).collect();
    }
    if cells.is_empty() {
        return Err(StartError::NoSite);
    }
    let rate = needed / cells.len() as f32;
    for (x, y) in cells {
        sim.water.sources.push(crate::water::Source { x, y, rate, seasonality: 0.0 });
    }
    Ok(())
}

/// Start 1's springs: steady ones on the river cells just above the dam, inside the pond to be
/// (groundwater the rock sill forces up), bringing `START_INFLOW_MARGIN` times the full pond's
/// daily loss. (The river's own water is not counted: on the flats it spreads and dries before it
/// reaches the pond.) Before the dam the springs' water runs out through the gap at once, so the
/// river above stays as it was.
fn feed_at_sill(sim: &mut SimState, dam: &[Cell], height: f32) -> Result<(), StartError> {
    let p = crate::dam_preview::preview(sim, dam, height).ok_or(StartError::NoSite)?;
    let needed = p.flooded.len() as f32 * pond_loss() * START_INFLOW_MARGIN;
    if needed <= 0.0 {
        return Ok(());
    }
    let mut near: Vec<(usize, Cell)> = p
        .flooded
        .iter()
        .copied()
        .filter(|&(x, y)| sim.world.river.get(x, y).copied().unwrap_or(false))
        .filter_map(|start| {
            let mut at = start;
            (1..=RIVER_WALK).find_map(|steps| {
                at = sim.world.downstream(at.0, at.1)?;
                dam.contains(&at).then_some((steps, start))
            })
        })
        .collect();
    near.sort();
    near.truncate(SILL_SPRINGS);
    if near.is_empty() {
        return Err(StartError::NoSite);
    }
    let rate = needed / near.len() as f32;
    for (_, (x, y)) in near {
        sim.water.sources.push(crate::water::Source { x, y, rate, seasonality: 0.0 });
    }
    Ok(())
}

/// A dam a start is designed around: a line of cells across the river and its height.
#[derive(Clone, Debug, PartialEq)]
pub struct DamSite {
    pub cells: Vec<Cell>,
    pub height: f32,
    /// The settling cell the start is designed around (dry, beside the river, trees about).
    pub camp: Cell,
}

/// How close (depth units) the water must come to the dam crest for the pond to count as full.
const FULL_WITHIN: f32 = 0.05;
/// Pond cells this close (cells) to the dam are the ones inspected.
const WATCH_RADIUS: usize = 8;

impl DamSite {
    /// Whether the dam stands: every cell of the line holds a water dam.
    pub fn is_built(&self, sim: &SimState) -> bool {
        self.cells.iter().all(|c| sim.water.has_dam(c.0, c.1))
    }

    /// Watches the pond this dam would make, as `sim` is now (before it is built). `None` if the
    /// line is not a valid dam.
    pub fn watch(&self, sim: &SimState) -> Option<PondWatch> {
        let p = crate::dam_preview::preview(sim, &self.cells, self.height)?;
        let mid = self.cells[self.cells.len() / 2];
        Some(PondWatch { level: p.level, near: p.flooded.into_iter().filter(|&c| dist2(c, mid) <= WATCH_RADIUS * WATCH_RADIUS).collect() })
    }
}

/// The pond cells beside a dam and the level they should reach.
#[derive(Clone, Debug, PartialEq)]
pub struct PondWatch {
    /// Water surface height of a full pond (the dam crest).
    pub level: f32,
    near: Vec<Cell>,
}

impl PondWatch {
    /// The pond is full when at least three wet pond cells beside the dam have their water
    /// surface within `FULL_WITHIN` of the crest.
    pub fn is_full(&self, sim: &SimState) -> bool {
        let full = |c: &&Cell| {
            let depth = sim.water.depth.get(c.0, c.1).copied().unwrap_or(0.0);
            depth > crate::buildings::WET_DEPTH && sim.height.get(c.0, c.1).copied().unwrap_or(0.0) + depth >= self.level - FULL_WITHIN
        };
        self.near.iter().filter(full).count() >= 3
    }
}

/// Builds the new game for `seed` and also returns the dam it is designed around. Tests and the
/// scripted demo order that dam; the player picks their own spot.
pub fn new_game_with_dam(seed: u64) -> Result<(SimState, DamSite), StartError> {
    build_river(seed)
}

/// Builds the new game on the real-terrain map `id` (see `maps::all`) and returns the dam it is
/// designed around, like `new_game_with_dam`.
pub fn new_game_on_map(id: &str) -> Result<(SimState, DamSite), StartError> {
    let (info, world) = crate::maps::world(id).map_err(|e| StartError::Build(e.to_string()))?;
    build_river_on(SimState::from_world(info.seed(), world))
}

/// Builds the new game on a parsed map (bundled or a candidate being checked).
pub fn new_game_on_world(info: &crate::maps::MapInfo, world: crate::worldgen::World) -> Result<(SimState, DamSite), StartError> {
    build_river_on(SimState::from_world(info.seed(), world))
}

/// Builds the new game for `seed` (see `new_game_with_dam`).
pub fn new_game(seed: u64) -> Result<SimState, StartError> {
    build_river(seed).map(|(sim, _)| sim)
}

use crate::water::Cell;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{agents, Command};

    /// Everything a test can see of a state, for equality.
    fn fingerprint(s: &SimState) -> String {
        let beavers: Vec<_> = s.colony.beavers.iter().map(|b| (b.id, b.age_days.to_bits(), b.stage, b.pos.0.to_bits(), b.pos.1.to_bits())).collect();
        let trees: Vec<_> = s.resources.items.iter().map(|r| (r.id, r.kind, r.cell)).collect();
        let agents: Vec<_> = s.agents.by_id.values().map(|a| (a.id, a.cell)).collect();
        format!("{} {} {:?} {:?} {:?} {:?} {:?}", s.seed, s.time_days, s.height.as_slice().iter().map(|h| h.to_bits()).fold(0u64, |a, b| a.wrapping_mul(31).wrapping_add(b as u64)), beavers, trees, agents, s.buildings.buildings)
    }

    /// The new game with its colony founded at the designed camp (as the player's settle would).
    fn settled(seed: u64) -> (SimState, DamSite) {
        let (mut s, dam) = new_game_with_dam(seed).unwrap();
        s.apply(Command::Settle { cell: dam.camp }).unwrap();
        (s, dam)
    }

    #[test]
    fn new_game_is_identical_per_seed_differs_between_seeds_and_runs_no_step() {
        let (a, b, c) = (new_game(7).unwrap(), new_game(7).unwrap(), new_game(8).unwrap());
        assert_eq!(fingerprint(&a), fingerprint(&b));
        assert_ne!(fingerprint(&a), fingerprint(&c));
        for s in [&a, &b, &c] {
            assert_eq!(s.time_days, 0.0, "building a start runs no sim step");
        }
    }

    #[test]
    fn new_game_is_two_adults_and_two_kits_in_the_river_and_no_buildings() {
        use crate::colony::Stage;
        let (s, _) = settled(3);
        assert_eq!((s.colony.count_stage(Stage::Adult), s.colony.count_stage(Stage::Kit), s.colony.population()), (2, 2, 4));
        assert!(s.buildings.buildings.is_empty(), "no stockpile, lodge or burrow: {:?}", s.buildings.buildings);
        assert!(s.work.places.is_empty());
        assert!(s.water.dam_height.as_slice().iter().all(|h| *h <= 0.0));
        assert_eq!(s.agents.by_id.len(), 4, "all four stand on the map");
        assert!(s.agents.by_id.values().all(|a| s.world.river.get(a.cell.0, a.cell.1) == Some(&true)), "all four swim in the river");
    }

    /// Places `kind` on the first free cell scanning outwards from the first beaver.
    fn place_near(s: &mut SimState, kind: crate::buildings::BuildingKind) -> crate::buildings::BuildingId {
        let c = s.colony.beavers[0].pos;
        let (cx, cy) = (c.0 as usize, c.1 as usize);
        for r in 2..30usize {
            for (x, y) in (cx.saturating_sub(r)..=cx + r).flat_map(|x| (cy.saturating_sub(r)..=cy + r).map(move |y| (x, y))) {
                let reachable = s.nav.find_path(s.colony.beavers[0].id, s.agents.get(s.colony.beavers[0].id).unwrap().cell, crate::nav::Goal::Beside((x, y))).is_some();
                if !reachable {
                    continue;
                }
                if let Ok(id) = s.place_site(kind, (x, y)) {
                    return id;
                }
            }
        }
        panic!("no room for a {kind:?}");
    }

    fn stage_of(s: &SimState, id: crate::colony::BeaverId) -> crate::colony::Stage {
        s.colony.beaver(id).unwrap().stage
    }

    #[test]
    fn only_adults_dig_a_placed_burrow_and_nothing_is_built_before_the_command() {
        use crate::{buildings::{BuildingKind, Status}, colony::Stage};
        let (mut s, _) = settled(1);
        // Fed, so the test watches building rather than hunger (the family arrives empty-handed).
        s.buildings.stores.deposit(crate::buildings::Good::Food, 60.0).unwrap();
        s.work.auto_organise = true; // the player opts the family into organising itself
        for _ in 0..200 {
            s.step(0.05).unwrap();
        }
        assert!(s.buildings.buildings.is_empty() && s.work.places.is_empty(), "nothing is built before its command");
        let id = place_near(&mut s, BuildingKind::Burrow);
        let (mut diggers, mut done_day) = (std::collections::BTreeSet::new(), None);
        for _ in 0..600 {
            s.step(0.05).unwrap();
            for a in s.agents.by_id.values() {
                if matches!(a.task, agents::Task::Build { .. }) {
                    diggers.insert(a.id);
                }
            }
            if done_day.is_none() && s.buildings.building(id).unwrap().status == Status::Complete {
                done_day = Some(s.time_days);
            }
        }
        assert!(done_day.is_some(), "the adults dig the burrow");
        assert!(!diggers.is_empty() && diggers.iter().all(|&d| stage_of(&s, d) == Stage::Adult), "diggers: {diggers:?}");
        assert!(s.agents.by_id.values().filter(|a| stage_of(&s, a.id) == Stage::Kit).all(|a| matches!(a.task, agents::Task::Idle) && a.load.is_none()));
    }

    #[test]
    fn kits_cannot_be_ordered_to_work() {
        use crate::colony::Stage;
        let (mut s, _) = settled(1);
        let kit = s.colony.beavers.iter().find(|b| b.stage == Stage::Kit).unwrap().id;
        let cell = s.agents.get(kit).unwrap().cell;
        let area = crate::work::Area { min: cell, max: cell };
        let to = Some(agents::Assignment::Area(crate::resources::ResourceKind::Tree, area));
        assert!(s.apply(Command::Assign { who: vec![kit], to }).is_err());
        assert_eq!(s.agents.get(kit).unwrap().assignment, None);
    }

    /// Runs start 1 for `days`; with `order` the planned dam is ordered at once and nothing else is
    /// ever commanded. Returns the day the dam stood, the day the pond first held its level
    /// (after the dam stood), and the end state.
    fn run(seed: u64, order: bool, days: f32) -> (SimState, Option<f64>, Option<f64>) {
        let (mut s, dam) = settled(seed);
        let watch = dam.watch(&s).unwrap();
        s.work.auto_organise = order;
        if order {
            s.apply(Command::OrderDam { cells: dam.cells.clone(), height: dam.height }).unwrap();
        }
        let (mut built, mut held) = (None, None);
        while s.time_days < days as f64 {
            s.step(0.05).unwrap();
            if built.is_none() && dam.is_built(&s) {
                built = Some(s.time_days);
            }
            if held.is_none() && built.is_some() && watch.is_full(&s) {
                held = Some(s.time_days);
            }
        }
        (s, built, held)
    }

    #[test]
    fn adults_fell_and_haul_wood_to_a_placed_dam_which_completes_only_from_delivered_wood_and_the_pond_rises() {
        use crate::buildings::Good;
        use crate::colony::Stage;
        let (mut s, dam) = settled(1);
        s.work.auto_organise = true;
        let watch = dam.watch(&s).unwrap();
        let trees_before = s.resources.items.iter().filter(|r| r.kind == crate::resources::ResourceKind::Tree).count();
        let id = s.order_dam(&dam.cells, dam.height).unwrap();
        let (mut built, mut held) = (None, None);
        while s.time_days < 200.0 && held.is_none() {
            s.step(0.05).unwrap();
            let site = s.work.place(id).and_then(|p| p.site.as_ref()).unwrap();
            if dam.is_built(&s) {
                built.get_or_insert(s.time_days);
                assert!(site.is_done() && site.missing(Good::Logs) <= 1e-3 && site.missing(Good::Sticks) <= 1e-3, "built before its wood arrived");
            }
            assert!(!dam.is_built(&s) || built.is_some());
            for a in s.agents.by_id.values().filter(|a| stage_of(&s, a.id) == Stage::Kit) {
                assert!(matches!(a.task, agents::Task::Idle) && a.load.is_none(), "a kit worked: {:?}", a.task);
            }
            if built.is_some() && watch.is_full(&s) {
                held = Some(s.time_days);
            }
        }
        assert!(built.is_some(), "the dam was never finished");
        assert!(held.is_some(), "the pond never rose to the dam crest");
        let trees_after = s.resources.items.iter().filter(|r| r.kind == crate::resources::ResourceKind::Tree).count();
        assert!(trees_after < trees_before, "the adults felled trees");
        assert_eq!(s.buildings.stores.amount(Good::Logs), 0.0, "wood went to the dam, not a stockpile");
    }

    #[test]
    fn without_the_dam_command_nothing_is_built() {
        let (s, built, _) = run(1, false, 60.0);
        assert_eq!(built, None);
        assert!(s.work.places.iter().all(|p| p.site.is_none()));
        assert!(s.water.dam_height.as_slice().iter().all(|h| *h <= 0.0));
    }

    #[test]
    fn dam_order_is_rejected_for_a_bad_line_and_the_family_idles_without_orders() {
        let (mut s, _) = settled(1);
        assert!(s.apply(Command::OrderDam { cells: vec![], height: 1.0 }).is_err());
        assert!(s.apply(Command::OrderDam { cells: vec![(5, 5), (5, 5)], height: 1.0 }).is_err());
        assert!(s.apply(Command::OrderDam { cells: vec![(5, 5), (5, 6)], height: -1.0 }).is_err());
        assert!(s.work.places.is_empty() && s.work.dam_plans.is_empty(), "a rejected order leaves no site");
        for _ in 0..100 {
            s.step(0.05).unwrap();
        }
        assert!(s.agents.by_id.values().all(|a| a.load.is_none() && matches!(a.task, agents::Task::Idle)), "nothing to do, nothing done");
    }

    #[test]
    fn no_colony_is_founded_until_settle_which_founds_at_the_chosen_cell_and_rejects_an_unusable_one() {
        let (mut s, dam) = new_game_with_dam(1).unwrap();
        assert!(!s.colony.is_founded() && s.colony.beavers.is_empty() && s.agents.by_id.is_empty());
        for _ in 0..100 {
            s.step(0.05).unwrap();
        }
        assert!(!s.colony.is_founded(), "stepping never founds a colony");
        // Unusable: off the map, in the river, and in a locked valley.
        let size = s.world.size;
        assert!(s.apply(Command::Settle { cell: (size, 0) }).is_err());
        let river = (0..size * size).map(|i| (i % size, i / size)).find(|&(x, y)| s.world.river.get(x, y) == Some(&true)).unwrap();
        assert!(s.apply(Command::Settle { cell: river }).is_err());
        if let Some(c) = (0..size * size).map(|i| (i % size, i / size)).find(|&c| s.world.valley_of(c).is_some_and(|v| !s.world.is_open(v))) {
            assert!(s.apply(Command::Settle { cell: c }).is_err(), "a locked valley cannot be settled");
        }
        assert!(!s.colony.is_founded() && s.agents.by_id.is_empty(), "a rejected settle changes nothing");
        // A usable cell founds the colony there.
        s.apply(Command::Settle { cell: dam.camp }).unwrap();
        assert!(s.colony.is_founded());
        assert_eq!(s.colony.beavers.len(), 4);
        assert_eq!(s.colony.lodges[0].pos, (dam.camp.0 as f32 + 0.5, dam.camp.1 as f32 + 0.5));
        assert!(s.apply(Command::Settle { cell: dam.camp }).is_err(), "only one colony");
    }

    #[test]
    fn a_long_run_with_no_orders_gives_no_duties_and_builds_nothing() {
        let (mut s, _) = settled(2);
        for _ in 0..1200 {
            s.step(0.05).unwrap();
        }
        assert!(!s.work.auto_organise && s.work.managed.is_empty());
        assert!(s.buildings.buildings.is_empty() && s.work.places.is_empty(), "nothing built");
        assert!(s.agents.by_id.values().all(|a| a.assignment.is_none() && a.load.is_none()), "no gather duty");
        assert!(s.work.places.iter().all(|p| p.site.is_none()), "no build duty");
    }

    #[test]
    fn an_assign_gather_order_moves_only_idle_beavers_and_a_job_keeps_being_worked_unordered() {
        use crate::{buildings::BuildingKind, colony::Stage, resources::ResourceKind, work::Area};
        let (mut s, _) = settled(1);
        let adults: Vec<_> = s.colony.beavers.iter().filter(|b| b.stage == Stage::Adult).map(|b| b.id).collect();
        let kit = s.colony.beavers.iter().find(|b| b.stage == Stage::Kit).unwrap().id;
        let (worker, idle) = (adults[0], adults[1]);
        let site = place_near(&mut s, BuildingKind::Burrow);
        s.apply(Command::FillSlot { building: site, slot: 0, who: worker }).unwrap();
        let start = |s: &SimState, id| s.agents.get(id).unwrap().cell;
        let before = (start(&s, worker), start(&s, idle), start(&s, kit));
        let c = before.1;
        let area = Area { min: (c.0.saturating_sub(25), c.1.saturating_sub(25)), max: (c.0 + 25, c.1 + 25) };
        s.apply(Command::Assign { who: vec![idle], to: Some(agents::Assignment::Area(ResourceKind::Tree, area)) }).unwrap();
        let labour = |s: &SimState| s.work.place(site).unwrap().site.as_ref().unwrap().labour_done;
        let (mut gathered, mut worked) = (false, false);
        for _ in 0..400 {
            s.step(0.05).unwrap();
            gathered |= s.agents.get(idle).unwrap().task != agents::Task::Idle;
            worked |= labour(&s) > 0.0;
        }
        assert!(gathered, "the idle beaver took the gather order");
        assert!(worked && s.agents.get(worker).unwrap().assignment.is_none(), "the slot holder kept working its job, never ordered");
        assert_eq!(s.agents.get(kit).unwrap().task, agents::Task::Idle);
        assert_eq!(start(&s, kit), before.2, "the kit was not moved");
    }

    #[test]
    fn the_family_arrives_with_a_small_larder_and_nothing_else() {
        use crate::buildings::Good;
        let (mut s, _) = settled(1);
        let need = crate::hud::food_need_per_day(&s);
        let food = s.buildings.stores.amount(Good::Food);
        assert!((food - need * START_LARDER_DAYS).abs() < 1e-3 && food > 0.0, "larder {food} for need {need}/day");
        assert_eq!(s.colony.food, food, "the colony eats from that same store");
        assert_eq!((s.colony.wood, s.buildings.stores.amount(Good::Logs)), (0.0, 0.0));
        assert!(s.agents.by_id.values().all(|a| a.load.is_none()));
        // With no foraging ordered, the larder runs low but nobody starves in the first two weeks.
        while s.time_days < 14.0 {
            s.step(0.5).unwrap();
        }
        assert_eq!(s.colony.population(), 4, "nobody died in the first two weeks");
        assert!(s.buildings.stores.amount(Good::Food) < food, "they ate from the larder");
    }

    /// The start is a flowing stream a dam visibly changes: for several seeds the planned dam, at
    /// its own crest, raises the water at least half a unit above the river as it runs and floods
    /// ground that is dry now; the pond is held behind the dam (not round it), no lake stands in
    /// it, and the dam can be ordered.
    #[test]
    fn the_start_river_flows_and_its_planned_dam_visibly_raises_it() {
        for seed in [1, 2, 4, 6, 8] {
            let (s, dam) = new_game_with_dam(seed).unwrap();
            let e = dam_effect(&s, &dam.cells, dam.height).unwrap();
            assert!(e.rise >= 0.5, "seed {seed}: the dam raises the water only {:.2}", e.rise);
            assert!(e.newly_flooded >= 30, "seed {seed}: only {} of {} pond cells are dry now", e.newly_flooded, e.flooded);
            assert!(e.drop >= MIN_DROP, "seed {seed}: the river falls only {:.2} through the site", e.drop);
            let p = crate::dam_preview::preview(&s, &dam.cells, dam.height).unwrap();
            assert!(holds(&s, &dam.cells, &p.flooded), "seed {seed}: the water walks round the dam");
            assert!(p.flooded.iter().all(|&(x, y)| s.world.lake.get(x, y) == Some(&false)), "seed {seed}: a lake stands in the pond");
            assert!(s.dam_site(&dam.cells).is_ok(), "seed {seed}: the planned dam cannot be ordered");
        }
    }

    /// What the HUD shows after its demo's twelve days: the river runs as it did at the start, so
    /// the planned dam still raises it visibly.
    #[test]
    fn the_planned_dam_still_raises_the_river_after_twelve_days() {
        let (mut s, dam) = settled(1);
        let before = dam_effect(&s, &dam.cells, dam.height).unwrap();
        while s.time_days < 12.0 {
            s.step(0.05).unwrap();
        }
        let after = dam_effect(&s, &dam.cells, dam.height).unwrap();
        assert!(after.rise >= 0.5 && after.rise >= before.rise - 0.1, "rise {:.2} -> {:.2}", before.rise, after.rise);
        assert!(after.newly_flooded >= 30, "{} of {} pond cells dry after 12 days", after.newly_flooded, after.flooded);
    }

    /// A valley sloping down toward +x with sides rising 0.15 per cell, its river along y = 64.
    fn sloping_valley() -> SimState {
        let mut s = SimState::new(1);
        let n = s.world.size;
        for y in 0..n {
            for x in 0..n {
                let h = 0.2 * (n - x) as f32 + 0.15 * (y as f32 - 64.0).abs();
                *s.height.get_mut(x, y).unwrap() = h;
                *s.world.river.get_mut(x, y).unwrap() = y == 64;
                *s.world.flow_to.get_mut(x, y).unwrap() = (y == 64 && x + 1 < n).then_some(y * n + x + 1);
            }
        }
        s.water = crate::water::Water::new(n);
        s
    }

    #[test]
    fn a_dam_the_water_walks_round_does_not_hold_its_pond() {
        let s = sloping_valley();
        let line = |half: usize| -> Vec<Cell> { (64 - half..=64 + half).map(|y| (40, y)).collect() };
        // At crest 1 the banks rise above the pond seven cells out: a shorter line leaks round its ends.
        let short = crate::dam_preview::preview(&s, &line(1), 1.0).unwrap();
        assert!(!holds(&s, &line(1), &short.flooded), "a three-cell dam cannot hold a pond in this valley");
        let full = crate::dam_preview::preview(&s, &line(7), 1.0).unwrap();
        assert!(holds(&s, &line(7), &full.flooded), "a dam tied into both banks holds");
        assert!(full.flooded.iter().all(|c| c.0 < 40), "the held pond lies upstream");
    }

    #[test]
    fn a_dam_effect_tells_a_running_river_from_a_flat() {
        let mut s = sloping_valley();
        // Crest 2, tied into banks that rise above it 14 cells out.
        let line: Vec<Cell> = (50..=78).map(|y| (40, y)).collect();
        let running = dam_effect(&s, &line, 2.0).unwrap();
        assert!(running.drop >= 2.0, "0.2 per cell over 12 cells: {}", running.drop);
        assert!((running.rise - 2.0).abs() < 0.01 && running.is_visible(), "{running:?}");
        // The same river standing on a filled flat: no fall, and a dam on it is not the start's.
        for x in 0..s.world.size {
            *s.height.get_mut(x, 64).unwrap() = 17.6;
        }
        let flat = dam_effect(&s, &line, 2.0).unwrap();
        assert!(flat.drop < MIN_DROP && !flat.is_visible(), "{flat:?}");
    }

    /// Three years with no order at all: nothing is built, nothing hostile or foreign ever appears.
    #[test]
    fn a_three_year_unordered_run_builds_nothing_and_has_no_predators_rivals_or_disasters() {
        use crate::events::EventKind;
        for seed in [1] {
            let (mut s, _) = settled(seed);
            let days = 3.0 * crate::tuning::tech::YEAR_DAYS;
            while s.time_days < days {
                s.step(1.0).unwrap();
                assert!(s.buildings.buildings.is_empty() && s.work.places.is_empty() && s.work.dam_plans.is_empty(), "built unordered on day {}", s.time_days);
                assert!(s.ecology.predators.is_empty(), "predator on day {}", s.time_days);
                assert!(s.region.offers().is_empty());
                if (s.time_days as u32) % 30 == 0 {
                    assert!(s.water.dam_height.as_slice().iter().all(|h| *h <= 0.0));
                }
            }
            for r in s.events.log() {
                assert!(matches!(r.kind, EventKind::WanderingBeaver | EventKind::WetlandArrival), "{:?} fired", r.kind);
            }
            assert_eq!(s.region.time_days, 0.0, "no other colony advances");
        }
    }
}
