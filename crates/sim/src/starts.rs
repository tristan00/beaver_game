//! The three deterministic starts (owners: `start1`, `start23`). Built directly, never simulated
//! forward. Same (start, seed) gives an identical `SimState`.
//!
//! * `Start::River` (1): two adults + two kits arrive by a river; no lodge or dam; the player picks
//!   a dam spot; the family fells, gathers, builds; the pond fills.
//! * `Start::EarlyLake` (2): built dam and lake, a few elementary buildings, ~15 beavers (some
//!   joined, some born there).
//! * `Start::IndustrialLake` (3): larger dam, much larger lake, advanced tech, 50+ beavers with
//!   specialists, canals, efficient production, big storage.

use crate::SimState;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Start {
    River,
    EarlyLake,
    IndustrialLake,
}

impl Start {
    pub const ALL: [Start; 3] = [Start::River, Start::EarlyLake, Start::IndustrialLake];
    /// 1-based number used by `--start N`.
    pub fn number(self) -> u8 {
        self as u8 + 1
    }
    pub fn from_number(n: u8) -> Option<Start> {
        Start::ALL.get((n as usize).wrapping_sub(1)).copied()
    }
    /// Menu title and one-line pitch.
    pub fn title(self) -> (&'static str, &'static str) {
        match self {
            Start::River => ("A river and a family", "Four beavers, no home. Build the first dam."),
            Start::EarlyLake => ("The early lake", "A dam holds a young lake; fifteen beavers need work."),
            Start::IndustrialLake => ("The industrial lake", "Fifty beavers, canals and workshops. Keep it humming."),
        }
    }
}

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

fn build_err(e: impl std::fmt::Debug) -> StartError {
    StartError::Build(format!("{e:?}"))
}

/// Builds a start for `seed` directly (no simulation step runs: `time_days` stays 0). The same
/// (start, seed) always gives an identical state. Starts 2 and 3 are filled in by `start23`;
/// until then they return a fresh world.
pub fn build(start: Start, seed: u64) -> Result<SimState, StartError> {
    match start {
        Start::River => build_river(seed).map(|(sim, _)| sim),
        Start::EarlyLake | Start::IndustrialLake => Ok(SimState::new(seed)),
    }
}

/// Dams tried, in cells above the river bed.
const DAM_HEIGHTS: [f32; 6] = [0.5, 0.6, 0.8, 1.0, 1.2, 1.5];
/// A first pond of this many cells: big enough to see, small enough to fill in days.
const POND_AREA: usize = 400;
const POND_AREA_RANGE: std::ops::RangeInclusive<usize> = 100..=700;
/// The valley floor is clay: seepage is a quarter of the default, so a river can hold a pond.
/// Evaporation and seepage then take about 0.02 depth per wet cell per day, so the river above
/// the dam is given one unit of inflow per `CELLS_PER_INFLOW` pond cells (half the steady-state
/// capacity, for the seasons) for the pond to stay full.
const CLAY_SEEPAGE: f32 = 0.01;
/// Springs are put on at most this many river cells, found by following the river at most `RIVER_WALK` cells.
const MAX_SPRINGS: usize = 20;
const RIVER_WALK: usize = 60;
const CELLS_PER_INFLOW: f32 = 15.0;
const MIN_RISE: f32 = 0.3;
const MAX_DAM_CELLS: usize = 10;
/// The camp is this far from the dam centre, this close to the river bank, with this many trees about.
const CAMP_DAM_DIST: std::ops::RangeInclusive<usize> = 5..=16;
const BANK_MAX: usize = 7;
const TREES_WANTED: usize = 10;
const TREE_RADIUS: usize = 14;
/// Kits are this many days old (under a year).
const KIT_AGE_DAYS: f32 = 150.0;

fn dist2(a: Cell, b: Cell) -> usize {
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
    let mut out = Vec::new();
    for y in (4..size - 4).step_by(2) {
        for x in (4..size - 4).step_by(2) {
            if !is_river(x, y) {
                continue;
            }
            for horizontal in [true, false] {
                let at = |t: isize| if horizontal { ((x as isize + t) as usize, y) } else { (x, (y as isize + t) as usize) };
                let reach = |dir: isize| (1..12).find(|&k| !is_river(at(dir * k).0, at(dir * k).1)).unwrap_or(12);
                // Both banks, plus one cell of bank on each side.
                let cells: Vec<Cell> = (-reach(-1)..=reach(1)).map(at).collect();
                if cells.len() > MAX_DAM_CELLS {
                    continue;
                }
                for height in DAM_HEIGHTS {
                    let Some(p) = crate::dam_preview::preview(sim, &cells, height) else { continue };
                    let area = p.flooded.len();
                    if p.rise >= MIN_RISE && POND_AREA_RANGE.contains(&area) {
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

/// The best (camp, dam) pair for a state as `SimState::new` leaves it.
fn river_plan(sim: &SimState) -> Option<RiverPlan> {
    dam_candidates(sim).into_iter().find_map(|(_, dam, height)| {
        let camp = camp_near(sim, &dam, height)?;
        Some(RiverPlan { camp, dam, height })
    })
}

/// Start 1: a family of two adults and two kits camps by the river with a stockpile, no lodge
/// and no dam. The player orders a dam (`Command::OrderDam`); automatic organisation then has
/// the family fell trees, carry wood to the stockpile and the dam site, and build it.
///
/// Returns the state and the dam the start is designed around (cells across the river and its
/// height), which leaves the camp dry. Springs across the future pond are added so the river can
/// keep that pond full against evaporation and seepage; they flow from day 0.
fn build_river(seed: u64) -> Result<(SimState, DamSite), StartError> {
    let mut sim = SimState::new(seed);
    let plan = river_plan(&sim).ok_or(StartError::NoSite)?;
    let camp = plan.camp;
    sim.water.config.seepage = CLAY_SEEPAGE;
    feed_river(&mut sim, &plan)?;
    let land = crate::WorldLand { world: &sim.world, depth: &sim.water.depth };
    sim.colony.found((camp.0 as f32 + 0.5, camp.1 as f32 + 0.5), &land).map_err(build_err)?;
    for b in sim.colony.beavers.iter_mut().filter(|b| b.stage == crate::colony::Stage::Yearling) {
        b.age_days = KIT_AGE_DAYS;
        b.stage = crate::colony::Stage::Kit;
    }
    // The stockpile covers the 3x3 around the camp cell (checked dry); tech does not gate it.
    let terrain = crate::buildings::Terrain { height: &sim.height, depth: &sim.water.depth };
    sim.buildings
        .add_complete(crate::buildings::BuildingKind::Warehouse, (camp.0 - 1, camp.1 - 1), terrain)
        .map_err(build_err)?;
    // Founding opens the camp's valley and its neighbours, as settling does.
    if let Some(v) = sim.world.valley_of(camp) {
        sim.world.valleys[v.0 as usize].open = true;
    }
    sim.world.unlock(camp).map_err(build_err)?;
    sim.work.auto_organise = true;
    crate::agents::place_beavers(&mut sim);
    Ok((sim, DamSite { cells: plan.dam, height: plan.height }))
}

/// Tops up the river above the planned dam so the pond can fill and stay full: steady springs
/// on the river cells that flow into it, nearest first. They sit in the channel, so before the dam
/// the extra water simply runs past.
fn feed_river(sim: &mut SimState, plan: &RiverPlan) -> Result<(), StartError> {
    let p = crate::dam_preview::preview(sim, &plan.dam, plan.height).ok_or(StartError::NoSite)?;
    let needed = p.flooded.len() as f32 / CELLS_PER_INFLOW - p.pressure;
    if needed <= 0.0 {
        return Ok(());
    }
    let pond: std::collections::HashSet<Cell> = p.flooded.iter().chain(&plan.dam).copied().collect();
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
    if upstream.is_empty() {
        return Err(StartError::NoSite);
    }
    let rate = needed / upstream.len() as f32;
    for (_, (x, y)) in upstream {
        sim.water.sources.push(crate::water::Source { x, y, rate, seasonality: 0.0 });
    }
    Ok(())
}

/// A dam a start is designed around: a line of cells across the river and its height.
#[derive(Clone, Debug, PartialEq)]
pub struct DamSite {
    pub cells: Vec<Cell>,
    pub height: f32,
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

/// Builds start 1 and also returns the dam it is designed around. Tests and the scripted demo
/// order that dam; the player picks their own spot.
pub fn build_river_with_dam(seed: u64) -> Result<(SimState, DamSite), StartError> {
    build_river(seed)
}

use crate::water::Cell;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{agents, Command};

    #[test]
    fn numbers_round_trip_and_bad_numbers_are_none() {
        for s in Start::ALL {
            assert_eq!(Start::from_number(s.number()), Some(s));
        }
        assert_eq!([1, 2, 3], Start::ALL.map(|s| s.number()));
        assert_eq!((Start::from_number(0), Start::from_number(4)), (None, None));
    }

    /// Everything a test can see of a state, for equality.
    fn fingerprint(s: &SimState) -> String {
        let beavers: Vec<_> = s.colony.beavers.iter().map(|b| (b.id, b.age_days.to_bits(), b.stage, b.pos.0.to_bits(), b.pos.1.to_bits())).collect();
        let trees: Vec<_> = s.resources.items.iter().map(|r| (r.id, r.kind, r.cell)).collect();
        let agents: Vec<_> = s.agents.by_id.values().map(|a| (a.id, a.cell)).collect();
        format!("{} {} {:?} {:?} {:?} {:?} {:?}", s.seed, s.time_days, s.height.as_slice().iter().map(|h| h.to_bits()).fold(0u64, |a, b| a.wrapping_mul(31).wrapping_add(b as u64)), beavers, trees, agents, s.buildings.buildings)
    }

    #[test]
    fn start_one_is_identical_per_seed_differs_between_seeds_and_runs_no_step() {
        let (a, b, c) = (build(Start::River, 7).unwrap(), build(Start::River, 7).unwrap(), build(Start::River, 8).unwrap());
        assert_eq!(fingerprint(&a), fingerprint(&b));
        assert_ne!(fingerprint(&a), fingerprint(&c));
        for s in [&a, &b, &c] {
            assert_eq!(s.time_days, 0.0, "building a start runs no sim step");
        }
    }

    #[test]
    fn start_one_is_two_adults_and_two_kits_with_no_lodge_or_dam() {
        use crate::colony::Stage;
        let s = build(Start::River, 3).unwrap();
        assert_eq!((s.colony.count_stage(Stage::Adult), s.colony.count_stage(Stage::Kit), s.colony.population()), (2, 2, 4));
        assert!(s.buildings.buildings.iter().all(|b| b.kind != crate::buildings::BuildingKind::Lodge));
        assert!(s.water.dam_height.as_slice().iter().all(|h| *h <= 0.0));
        assert_eq!(s.agents.by_id.len(), 4, "all four stand on the map");
    }

    /// Runs start 1 for `days`; with `order` the planned dam is ordered at once and nothing else is
    /// ever commanded. Returns the day the dam stood, the day the pond first held its level
    /// (after the dam stood), and the end state.
    fn run(seed: u64, order: bool, days: f32) -> (SimState, Option<f64>, Option<f64>) {
        let (mut s, dam) = build_river_with_dam(seed).unwrap();
        let watch = dam.watch(&s).unwrap();
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
    fn dam_command_is_built_by_the_family_and_the_pond_fills_with_no_other_input() {
        for seed in [1, 2, 3] {
            let (s, built, held) = run(seed, true, 120.0);
            let built = built.unwrap_or_else(|| panic!("seed {seed}: the dam was never built; plans {:?}, failures {:?}", s.work.dam_plans, s.work.dam_failures));
            assert!(s.work.dam_failures.is_empty() && s.work.dam_plans.is_empty());
            assert!(built < 60.0, "seed {seed}: built on day {built}");
            assert!(held.is_some_and(|d| d >= built), "seed {seed}: pond never held its level (dam on day {built})");
        }
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
        let (mut s, _) = build_river_with_dam(1).unwrap();
        assert!(s.apply(Command::OrderDam { cells: vec![], height: 1.0 }).is_err());
        assert!(s.apply(Command::OrderDam { cells: vec![(5, 5), (5, 5)], height: 1.0 }).is_err());
        assert!(s.apply(Command::OrderDam { cells: vec![(5, 5), (5, 6)], height: -1.0 }).is_err());
        assert!(s.work.places.is_empty() && s.work.dam_plans.is_empty(), "a rejected order leaves no site");
        for _ in 0..100 {
            s.step(0.05).unwrap();
        }
        assert!(s.agents.by_id.values().all(|a| a.load.is_none() && matches!(a.task, agents::Task::Idle)), "nothing to do, nothing done");
    }
}
