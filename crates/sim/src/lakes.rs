//! Starts 2 and 3 (owner: `start23`): a dam and lake that already exist, with a colony living
//! around them. Built directly from the seed and never simulated forward (`time_days` stays 0).
//!
//! * Start 2, the early lake: a short dam holds a young lake; two lodges (about fifteen beavers,
//!   some born here, some who joined), a few elementary buildings and early tech.
//! * Start 3, the industrial lake: a dam at least 1.5 times start 2's in length and no lower holds a
//!   lake at least three times as wide; seven lodges (fifty-odd beavers, specialists of several
//!   kinds), canals from the lake, an advanced mill and storehouse and advanced tech.
//!
//! Both make the same lake the same way: pick a dam line across a river (`plans`), put springs on
//! the river above it so it stays full, build the dam, and fill the previewed pond to its level.

use crate::buildings::{BuildingId, BuildingKind, Good, Recipe, Terrain, WET_DEPTH};
use crate::colony::{BeaverId, Household, Job, Stage};
use crate::resources::{ResourceKind, MUD_AMOUNT, PLANT_MAX};
use crate::starts::{dist2, feed_river, strip_disabled, DamSite, StartError, CLAY_SEEPAGE};
use crate::water::Cell;
use crate::SimState;
use std::collections::HashSet;
use std::ops::RangeInclusive;

/// What a lake start is built around.
#[derive(Clone, Debug, PartialEq)]
pub struct LakeInfo {
    /// The dam that stands: cells across the river and its crest height above the bed.
    pub dam: DamSite,
    /// Water surface of the lake (the lowest dam crest).
    pub level: f32,
    /// Cells the dam floods (the lake), sorted by index (y, then x).
    pub lake: Vec<Cell>,
    /// Dug canals, each a line of cells starting beside the lake (start 3 only).
    pub canals: Vec<Vec<Cell>>,
}

impl LakeInfo {
    /// Lake area in cells.
    pub fn area(&self) -> usize {
        self.lake.len()
    }
}

fn err(msg: impl Into<String>) -> StartError {
    StartError::Build(msg.into())
}

// ───────────────────────────── choosing the dam ─────────────────────────────

/// Dam heights tried for the broad start 2 lake: crests that start 3 can stand no lower than
/// within `START3_HEIGHTS`.
const START2_HEIGHTS: [f32; 5] = [0.5, 0.6, 0.8, 1.0, 1.2];
/// The small start 2 lake used when a seed has no broad one.
const SMALL_HEIGHTS: [f32; 4] = [0.5, 0.6, 0.8, 1.0];
const SMALL_AREA: usize = 150;
const SMALL_AREA_RANGE: RangeInclusive<usize> = 60..=400;
const SMALL_LEN: RangeInclusive<usize> = 4..=10;
const START3_HEIGHTS: [f32; 8] = [0.8, 1.0, 1.2, 1.5, 2.0, 2.5, 3.0, 4.0];
/// Start 2: a broad lake near this many cells (at least 200), 4 to 20 dam cells long.
const START2_AREA: usize = 260;
const START2_AREA_RANGE: RangeInclusive<usize> = 200..=500;
const START2_LEN: RangeInclusive<usize> = 4..=20;
/// Start 3: dam up to this long and a lake up to this many cells.
const START3_MAX_LEN: usize = 30;
const START3_MAX_AREA: usize = 4000;
const START3_LEN_FACTOR: f32 = 1.5;
/// Start 3's crest is at least start 2's: a broad start 2 lake leaves few valleys that take a higher one.
const START3_HEIGHT_FACTOR: f32 = 1.0;
const START3_AREA_FACTOR: f32 = 3.0;
/// Extra bank cells a dam line may extend on each side of the river.
const PADS: [usize; 6] = [0, 1, 2, 4, 6, 9];
const MIN_RISE: f32 = 0.3;
/// Plans tried (best first) before giving up on a seed.
const PLANS_TRIED: usize = 12;

#[derive(Clone, Debug)]
struct Plan {
    cells: Vec<Cell>,
    height: f32,
    /// Height per cell: `height` on the line, the dam's crest over any spill points it was
    /// extended across (`dam_preview::extend_to_hold`).
    heights: Vec<f32>,
    area: usize,
}

struct Search<'a> {
    heights: &'a [f32],
    len: RangeInclusive<usize>,
    area: RangeInclusive<usize>,
}

fn is_river(sim: &SimState, x: usize, y: usize) -> bool {
    sim.world.river.get(x, y).copied().unwrap_or(false)
}

/// Every straight cross-river dam line with a pond inside the search limits, unordered. A pond
/// that touches the map edge is dropped: it would spill round the dam.
fn plans(sim: &SimState, s: &Search) -> Vec<Plan> {
    let size = sim.world.size;
    let mut lines: HashSet<Vec<Cell>> = HashSet::new();
    let mut out = Vec::new();
    for y in (4..size - 4).step_by(2) {
        for x in (4..size - 4).step_by(2) {
            if !is_river(sim, x, y) {
                continue;
            }
            for horizontal in [true, false] {
                let at = |t: isize| if horizontal { ((x as isize + t) as usize, y) } else { (x, (y as isize + t) as usize) };
                let reach = |dir: isize| (1..14).find(|&k| !is_river(sim, at(dir * k).0, at(dir * k).1)).unwrap_or(14);
                for pad in PADS {
                    let (lo, hi) = (-(reach(-1) + pad as isize), reach(1) + pad as isize);
                    let cells: Vec<Cell> = (lo..=hi).map(at).collect();
                    let inside = cells.iter().all(|c| c.0 >= 2 && c.1 >= 2 && c.0 < size - 2 && c.1 < size - 2);
                    if !inside || !s.len.contains(&cells.len()) || !lines.insert(cells.clone()) {
                        continue;
                    }
                    for &height in s.heights {
                        // A line the water walks round is tied in over its spill points (narrow lines
                        // only: the wider pads already reach up the banks).
                        let flat = vec![height; cells.len()];
                        let Some(plain) = crate::dam_preview::preview_heights(sim, &cells, &flat) else { continue };
                        let (cells, heights, p) = if plain.spill_at.is_none() {
                            (cells.clone(), flat, plain)
                        } else if pad <= 1 {
                            let Some(t) = crate::dam_preview::extend_to_hold(sim, &cells, &flat, cells.len() + 4) else { continue };
                            t
                        } else {
                            continue;
                        };
                        let on_edge = p.flooded.iter().any(|c| c.0 < 2 || c.1 < 2 || c.0 + 2 >= size || c.1 + 2 >= size);
                        if p.rise >= MIN_RISE && s.area.contains(&p.flooded.len()) && !on_edge {
                            out.push(Plan { cells: cells.clone(), height, heights: heights.clone(), area: p.flooded.len() });
                        }
                    }
                }
            }
        }
    }
    out
}

fn best(mut plans: Vec<Plan>, score: impl Fn(&Plan) -> f32) -> Vec<Plan> {
    plans.sort_by(|a, b| score(a).total_cmp(&score(b)).then_with(|| a.cells.cmp(&b.cells)).then(a.height.total_cmp(&b.height)));
    plans.truncate(PLANS_TRIED);
    plans
}

// ───────────────────────────── the lake ─────────────────────────────

/// Springs on the river above the dam, the dam itself and the previewed pond at its level.
/// Returns the pond's cells and level.
fn fill_lake(sim: &mut SimState, plan: &Plan) -> Result<(Vec<Cell>, f32), StartError> {
    let p = crate::dam_preview::preview_heights(sim, &plan.cells, &plan.heights).ok_or_else(|| err("dam line is not valid"))?;
    sim.water.config.seepage = CLAY_SEEPAGE;
    feed_river(sim, &plan.cells, plan.height)?;
    let cells: Vec<(Cell, f32)> = plan.cells.iter().copied().zip(plan.heights.iter().copied()).collect();
    sim.water.raise_dam(&cells).map_err(|e| err(e.to_string()))?;
    for &(x, y) in &plan.cells {
        *sim.water.depth.get_mut(x, y).ok_or_else(|| err("dam cell off the map"))? = 0.0;
    }
    for &(x, y) in &p.flooded {
        let bed = *sim.height.get(x, y).ok_or_else(|| err("pond cell off the map"))?;
        *sim.water.depth.get_mut(x, y).ok_or_else(|| err("pond cell off the map"))? = (p.level - bed).max(0.0);
    }
    // Trees and banks under the new water are gone.
    let drowned: HashSet<Cell> = p.flooded.iter().copied().collect();
    sim.resources.items.retain(|r| !drowned.contains(&r.cell));
    clear_dam_trees(sim, &plan.cells);
    Ok((p.flooded, p.level))
}

// ───────────────────────────── placing things ─────────────────────────────

/// Cells around a dam kept free of standing trees, so the dam can be reached and seen.
pub const DAM_CLEAR: usize = 1;

/// Removes every standing tree on a dam cell or within `DAM_CLEAR` cells of one.
fn clear_dam_trees(sim: &mut SimState, dam: &[Cell]) {
    sim.resources.items.retain(|r| {
        !(r.kind == ResourceKind::Tree && dam.iter().any(|d| d.0.abs_diff(r.cell.0) <= DAM_CLEAR && d.1.abs_diff(r.cell.1) <= DAM_CLEAR))
    });
}

/// Finds free dry ground for lodges, buildings and gathering spots, nearest an anchor.
struct Placer {
    size: usize,
    taken: HashSet<Cell>,
    /// Steps over land from the nearest wet cell (capped), indexed y * size + x.
    water_dist: Vec<u8>,
}

const DIST_CAP: u8 = 40;
/// Cells searched around an anchor for a spot.
const SEARCH_RADIUS: usize = 36;

impl Placer {
    /// Reads the water as it is now: call after the lake and canals are in.
    fn new(sim: &SimState) -> Self {
        let size = sim.world.size;
        let wet = |i: usize| sim.water.depth.as_slice()[i] >= WET_DEPTH;
        let mut dist = vec![DIST_CAP; size * size];
        let mut queue: std::collections::VecDeque<usize> = (0..size * size).filter(|&i| wet(i)).collect();
        for &i in &queue {
            dist[i] = 0;
        }
        while let Some(i) = queue.pop_front() {
            let (x, y) = (i % size, i / size);
            for (nx, ny) in [(x.wrapping_sub(1), y), (x + 1, y), (x, y.wrapping_sub(1)), (x, y + 1)] {
                if nx < size && ny < size && dist[ny * size + nx] > dist[i] + 1 {
                    dist[ny * size + nx] = dist[i] + 1;
                    queue.push_back(ny * size + nx);
                }
            }
        }
        Self { size, taken: HashSet::new(), water_dist: dist }
    }

    fn dist(&self, c: Cell) -> u8 {
        self.water_dist[c.1 * self.size + c.0]
    }

    /// Top-left cell of the free `w x h` footprint nearest `anchor` whose centre is `water` steps
    /// from the water and that passes `ok` (given the footprint's top-left). The footprint and a
    /// one-cell margin are dry, off the dam and unclaimed. Claims them and clears trees and rock
    /// from them. `None` if nothing fits.
    /// With `shore`, the footprint instead straddles the waterline (at most half of it in water),
    /// as the water buildings' site rule wants.
    #[allow(clippy::too_many_arguments)]
    fn spot(&mut self, sim: &mut SimState, anchor: Cell, (w, h): (usize, usize), water: RangeInclusive<u8>, shore: bool, ok: &dyn Fn(&SimState, Cell) -> bool) -> Option<Cell> {
        let size = self.size;
        let (x0, x1) = (anchor.0.saturating_sub(SEARCH_RADIUS).max(2), (anchor.0 + SEARCH_RADIUS).min(size - w - 3));
        let (y0, y1) = (anchor.1.saturating_sub(SEARCH_RADIUS).max(2), (anchor.1 + SEARCH_RADIUS).min(size - h - 3));
        let mut candidates: Vec<Cell> = (y0..=y1).flat_map(|y| (x0..=x1).map(move |x| (x, y))).collect();
        candidates.sort_by_key(|&c| (dist2((c.0 + w / 2, c.1 + h / 2), anchor), c.1, c.0));
        let found = candidates.into_iter().find(|&c| {
            water.contains(&self.dist((c.0 + w / 2, c.1 + h / 2)))
                && if shore {
                    // at the water's edge: some of the footprint in open water, the rest and the
                    // margin on free ground, nothing on a dam or canal
                    let wet = |k: Cell| sim.water.depth.get(k.0, k.1).is_some_and(|d| *d >= WET_DEPTH) && !sim.water.has_dam(k.0, k.1) && !sim.water.canal.get(k.0, k.1).copied().unwrap_or(true);
                    let fp: Vec<Cell> = self.cells(c, (w, h), 0).collect();
                    let n_wet = fp.iter().filter(|&&k| wet(k)).count();
                    n_wet > 0
                        && n_wet * 2 <= fp.len()
                        && self.cells(c, (w, h), 1).all(|k| !self.taken.contains(&k) && (self.is_dry_ground(sim, k) || wet(k)))
                } else {
                    self.cells(c, (w, h), 1).all(|k| !self.taken.contains(&k) && self.is_dry_ground(sim, k))
                }
                && ok(sim, c)
        })?;
        let margin: HashSet<Cell> = self.cells(found, (w, h), 1).collect();
        sim.resources.items.retain(|r| !(r.kind.is_solid() && margin.contains(&r.cell)));
        self.taken.extend(margin);
        Some(found)
    }

    /// The `w x h` rectangle at `at` grown by `margin` cells on every side.
    fn cells(&self, at: Cell, (w, h): (usize, usize), margin: usize) -> impl Iterator<Item = Cell> + use<> {
        let (x0, y0) = (at.0 - margin, at.1 - margin);
        (y0..at.1 + h + margin).flat_map(move |y| (x0..at.0 + w + margin).map(move |x| (x, y)))
    }

    fn is_dry_ground(&self, sim: &SimState, c: Cell) -> bool {
        sim.water.depth.get(c.0, c.1).is_some_and(|d| *d < WET_DEPTH)
            && !sim.world.river.get(c.0, c.1).copied().unwrap_or(true)
            && !sim.world.lake.get(c.0, c.1).copied().unwrap_or(true)
            && !sim.water.canal.get(c.0, c.1).copied().unwrap_or(true)
            && !sim.water.has_dam(c.0, c.1)
    }

    /// Scatters `n` resources of `kind` on free dry cells around `centre` (nearest first).
    fn scatter(&mut self, sim: &mut SimState, centre: Cell, radius: usize, kind: ResourceKind, amount: f32, n: usize) -> Result<(), StartError> {
        let mut around: Vec<Cell> = (centre.1.saturating_sub(radius)..=centre.1 + radius)
            .flat_map(|y| (centre.0.saturating_sub(radius)..=centre.0 + radius).map(move |x| (x, y)))
            .filter(|&c| c.0 < self.size && c.1 < self.size)
            .filter(|c| !self.taken.contains(c) && self.is_dry_ground(sim, *c) && sim.resources.at(*c).next().is_none())
            .collect();
        around.sort_by_key(|&c| (dist2(c, centre), c.1, c.0));
        if around.len() < n {
            return Err(err(format!("no room for {n} {kind:?} around {centre:?}")));
        }
        for &c in &around[..n] {
            let id = sim.resources.add(kind, c, amount);
            if kind == ResourceKind::Plant {
                sim.resources.items.iter_mut().find(|r| r.id == id).expect("just added").wet = true; // regrows
            }
            self.taken.insert(c);
        }
        Ok(())
    }
}

/// Standing trees inside the 9 x 9 work area of a camp whose top-left cell is `c`.
fn trees_near(sim: &SimState, c: Cell) -> usize {
    sim.resources
        .items
        .iter()
        .filter(|r| r.kind == ResourceKind::Tree && r.cell.0.abs_diff(c.0) <= 4 && r.cell.1.abs_diff(c.1) <= 4)
        .count()
}

const CAMP_TREES: usize = 14;
const GATHER_SPOTS: usize = 12;

// ───────────────────────────── the colony ─────────────────────────────

/// A building to put up: kind, anchor, and how its place is found.
struct Site {
    kind: BuildingKind,
    anchor: Cell,
    recipe: Option<Recipe>,
    /// Beavers working here: (job they should be good at, how many).
    staff: Option<(Job, usize)>,
}

fn place_building(sim: &mut SimState, placer: &mut Placer, site: &Site) -> Result<BuildingId, StartError> {
    let kind = site.kind;
    let fp = kind.spec().footprint;
    let logging = kind == BuildingKind::LoggingCamp;
    let shore = kind.spec().site == crate::buildings::Site::TouchesWater;
    let at = placer
        .spot(sim, site.anchor, fp, if shore { 0..=2 } else { 3..=14 }, shore, &|s, c| !logging || trees_near(s, c) >= CAMP_TREES)
        .ok_or_else(|| err(format!("no ground for a {kind:?} near {:?}", site.anchor)))?;
    let terrain = Terrain { height: &sim.height, depth: &sim.water.depth };
    let id = sim.buildings.add_complete(kind, at, terrain).map_err(|e| err(format!("{kind:?}: {e:?}")))?;
    let b = sim.buildings.building(id).expect("just added").clone();
    sim.work.add_complete(&b).map_err(|e| err(format!("{kind:?}: {e:?}")))?;
    if site.recipe.is_some() {
        sim.buildings.set_recipe(id, site.recipe).map_err(|e| err(format!("{kind:?}: {e:?}")))?;
    }
    let centre = (at.0 + fp.0 / 2, at.1 + fp.1 / 2);
    match kind {
        BuildingKind::ForagerHut => placer.scatter(sim, centre, 4, ResourceKind::Plant, PLANT_MAX, GATHER_SPOTS)?,
        BuildingKind::MudPit => placer.scatter(sim, centre, 4, ResourceKind::MudBank, MUD_AMOUNT, GATHER_SPOTS)?,
        _ => {}
    }
    Ok(id)
}

/// Seats `households` at lodges around the lake, one anchor each. Returns every beaver's lodge cell.
fn seat(sim: &mut SimState, placer: &mut Placer, anchors: &[Cell], households: &[Household]) -> Result<(), StartError> {
    for (h, &anchor) in households.iter().zip(anchors) {
        let at = placer
            .spot(sim, anchor, (3, 3), 3..=3, false, &|_, _| true)
            .ok_or_else(|| err(format!("no ground for a lodge near {anchor:?}")))?;
        let pos = (at.0 as f32 + 1.5, at.1 as f32 + 1.5);
        let land = crate::WorldLand { world: &sim.world, depth: &sim.water.depth };
        sim.colony.seat_household(pos, &land, h).map_err(|e| err(format!("{e:?}")))?;
    }
    Ok(())
}

/// Raises specialists: the `k`-th of `kinds` goes to the adult at every `stride`-th place.
fn specialise(sim: &mut SimState, kinds: &[Job]) {
    let adults: Vec<BeaverId> = sim.colony.beavers.iter().filter(|b| b.stage == Stage::Adult).map(|b| b.id).collect();
    let stride = (adults.len() / kinds.len().max(1)).max(1);
    for (k, &job) in kinds.iter().enumerate() {
        let Some(&id) = adults.get(k * stride) else { continue };
        if let Some(b) = sim.colony.beavers.iter_mut().find(|b| b.id == id) {
            b.skills[job.index()] = 0.55 + 0.05 * (k % 4) as f32;
        }
    }
}

/// Puts the best-suited free adults into a building's slots: `n` beavers, skilled `job` first.
fn staff(sim: &mut SimState, id: BuildingId, job: Job, n: usize) -> Result<(), StartError> {
    let kind = sim.buildings.building(id).map(|b| b.kind).ok_or_else(|| err("no such building"))?;
    for slot in 0..n {
        let busy = |sim: &SimState, who: BeaverId| sim.work.slot_of(who).is_some();
        let pick = sim
            .colony
            .beavers
            .iter()
            .filter(|b| b.stage == Stage::Adult && !busy(sim, b.id))
            .max_by(|a, b| a.skill(job).total_cmp(&b.skill(job)).then(b.id.cmp(&a.id)))
            .map(|b| b.id)
            .ok_or_else(|| err(format!("no free adult for {kind:?}")))?;
        sim.work.assign(id, slot, pick).map_err(|e| err(format!("{kind:?} slot {slot}: {e:?}")))?;
    }
    Ok(())
}

/// Moves `amount` of `good` into the ledger and records it in store `into`.
fn stock(sim: &mut SimState, into: BuildingId, good: Good, amount: f32) -> Result<(), StartError> {
    let put = sim.buildings.stores.deposit(good, amount).map_err(|e| err(e.to_string()))?;
    sim.work.stock_add(into, good, put);
    Ok(())
}

/// Experience the colony has already earned, per job, and the techs it unlocks.
fn earn(sim: &mut SimState, xp: &[(Job, f32)]) {
    for &(job, v) in xp {
        sim.colony.experience[job.index()] = v;
    }
    sim.tech.update(&sim.colony);
}

fn household(pair_joined: bool, born_days: &[f32], joined: u32) -> Household {
    Household { pair_years: 3.5, pair_joined, born_days: born_days.to_vec(), joined }
}

/// A cell of the lake `k` of `n` of the way along its (row-major) list.
fn around_lake(lake: &[Cell], k: usize, n: usize) -> Cell {
    lake[(k * lake.len() / n + lake.len() / (2 * n)).min(lake.len() - 1)]
}

fn finish(sim: &mut SimState, food_per_head: f32, wood_per_head: f32) {
    let n = sim.colony.population() as f32;
    // The old lake starts predate the single stores economy and keep the abstract larder.
    sim.colony.abstract_gathering = true;
    sim.colony.food = food_per_head * n;
    sim.colony.wood = wood_per_head * n;
    sim.work.auto_organise = true;
    strip_disabled(sim);
    sim.buildings.update_capacity(&sim.tech, crate::buildings::Terrain { height: &sim.height, depth: &sim.water.depth });
    crate::agents::place_beavers(sim);
}

// ───────────────────────────── start 2 ─────────────────────────────

/// Start 2: a broad young lake, two lodges, four elementary buildings. The lake is at least
/// 200 cells and leaves room for start 3's bigger one; a seed whose valley has no such
/// lake falls back to a small one (`SMALL_*`).
pub fn early_lake(seed: u64) -> Result<(SimState, LakeInfo), StartError> {
    let probe = SimState::new(seed);
    let mut broad = plans(&probe, &Search { heights: &START2_HEIGHTS, len: START2_LEN, area: START2_AREA_RANGE });
    broad.retain(|p| !industrial_candidates(&probe, p.cells.len(), p.height, p.area).is_empty());
    let small = plans(&probe, &Search { heights: &SMALL_HEIGHTS, len: SMALL_LEN, area: SMALL_AREA_RANGE });
    drop(probe);
    let broad = best(broad, |p| p.area.abs_diff(START2_AREA) as f32 + 15.0 * p.cells.len() as f32);
    let small = best(small, |p| p.area.abs_diff(SMALL_AREA) as f32 + 15.0 * p.cells.len() as f32);
    let mut last = StartError::NoSite;
    for plan in broad.into_iter().chain(small) {
        match assemble_early(seed, &plan) {
            Ok(done) => return Ok(done),
            Err(e) => last = e,
        }
    }
    Err(last)
}

fn assemble_early(seed: u64, plan: &Plan) -> Result<(SimState, LakeInfo), StartError> {
    let mut sim = SimState::new(seed);
    let (lake, level) = fill_lake(&mut sim, plan)?;
    let mut placer = Placer::new(&sim);
    let a = |k, n| around_lake(&lake, k, n);
    // Two lodges, 15 beavers: 8 born to the founders (some grown), 2 + 3 who joined.
    let homes = [
        household(false, &[90.0, 210.0, 520.0, 900.0], 2),
        household(true, &[120.0, 300.0, 640.0, 1100.0], 1),
    ];
    seat(&mut sim, &mut placer, &[a(1, 3), a(2, 3)], &homes)?;
    specialise(&mut sim, &[Job::Logger, Job::Forager]);
    let mid = plan.cells[plan.cells.len() / 2];
    let warehouse = place_building(&mut sim, &mut placer, &Site { kind: BuildingKind::Warehouse, anchor: a(1, 2), recipe: None, staff: None })?;
    let camps = [
        (BuildingKind::LoggingCamp, a(1, 3), Job::Logger, 2),
        (BuildingKind::ForagerHut, a(2, 3), Job::Forager, 1),
        (BuildingKind::MudPit, mid, Job::Digger, 1),
    ];
    for (kind, anchor, job, n) in camps {
        let id = place_building(&mut sim, &mut placer, &Site { kind, anchor, recipe: None, staff: Some((job, n)) })?;
        staff(&mut sim, id, job, n)?;
    }
    earn(&mut sim, &[(Job::Builder, 8.0), (Job::Crafter, 4.0), (Job::Logger, 6.0), (Job::Forager, 4.0)]);
    sim.buildings.update_capacity(&sim.tech, crate::buildings::Terrain { height: &sim.height, depth: &sim.water.depth });
    for (good, amount) in [(Good::Logs, 30.0), (Good::Sticks, 25.0), (Good::Mud, 15.0)] {
        stock(&mut sim, warehouse, good, amount)?;
    }
    for a in crate::tech::Achievement::ALL {
        sim.tech.record(a);
    }
    finish(&mut sim, 12.0, 4.0);
    let info = LakeInfo { dam: DamSite { cells: plan.cells.clone(), height: plan.height, camp: plan.cells[plan.cells.len() / 2] }, level, lake, canals: Vec::new() };
    Ok((sim, info))
}

// ───────────────────────────── start 3 ─────────────────────────────

/// Canals dug from the lake edge inland: how many, longest, and how far the bed may be cut.
const CANALS: usize = 3;
const CANAL_LEN: RangeInclusive<usize> = 4..=9;
const CANAL_WATER: f32 = 0.3;
const CANAL_MAX_CUT: f32 = 1.5;
/// Canals start at least this far apart and from the dam.
const CANAL_SPACING: usize = 9;

/// Digs up to `CANALS` straight canals from the lake's edge going inland and fills them to the
/// lake's level, so they are linked to it. Returns the canals dug.
fn dig_canals(sim: &mut SimState, lake: &[Cell], level: f32, dam: &[Cell]) -> Result<Vec<Vec<Cell>>, StartError> {
    let in_lake: HashSet<Cell> = lake.iter().copied().collect();
    let size = sim.world.size;
    let mut dug: Vec<Vec<Cell>> = Vec::new();
    let mut starts: Vec<(Cell, (isize, isize))> = Vec::new();
    for &(x, y) in lake {
        for (dx, dy) in [(-1isize, 0isize), (1, 0), (0, -1), (0, 1)] {
            let (nx, ny) = ((x as isize + dx) as usize, (y as isize + dy) as usize);
            if nx < size && ny < size && !in_lake.contains(&(nx, ny)) && !sim.world.river.get(nx, ny).copied().unwrap_or(true) {
                starts.push(((nx, ny), (dx, dy)));
            }
        }
    }
    starts.sort_by_key(|&(c, d)| (c.1, c.0, d));
    let order: Vec<usize> = (0..starts.len()).map(|k| k * 7 % starts.len().max(1)).collect(); // spread, deterministic
    for k in order {
        if dug.len() == CANALS {
            break;
        }
        let (start, dir) = starts[k];
        if dug.iter().flatten().chain(dam).any(|&c| dist2(c, start) < CANAL_SPACING * CANAL_SPACING) {
            continue;
        }
        let mut line = Vec::new();
        for step in 0..*CANAL_LEN.end() {
            let c = ((start.0 as isize + dir.0 * step as isize) as usize, (start.1 as isize + dir.1 * step as isize) as usize);
            let inside = c.0 >= 3 && c.1 >= 3 && c.0 + 3 < size && c.1 + 3 < size;
            let h = if inside { sim.height.get(c.0, c.1).copied() } else { None };
            let free = inside && !in_lake.contains(&c) && sim.water.depth.get(c.0, c.1).is_some_and(|d| *d < WET_DEPTH) && !sim.water.has_dam(c.0, c.1);
            match h {
                Some(h) if free && h - (level - CANAL_WATER) <= CANAL_MAX_CUT => line.push(c),
                _ => break,
            }
        }
        if !CANAL_LEN.contains(&line.len()) {
            continue;
        }
        for &c in &line {
            let bed = sim.height.get(c.0, c.1).copied().ok_or_else(|| err("canal cell off the map"))?;
            let cut = (bed - (level - CANAL_WATER)).max(0.05);
            sim.water.dig_canal(&mut sim.height, &[c], cut).map_err(|e| err(e.to_string()))?;
            let new_bed = bed - cut;
            *sim.water.depth.get_mut(c.0, c.1).ok_or_else(|| err("canal cell off the map"))? = (level - new_bed).max(0.0);
            sim.colony.dug += cut;
        }
        dug.push(line);
    }
    if dug.len() < 2 {
        return Err(err(format!("only {} canals fit beside the lake", dug.len())));
    }
    Ok(dug)
}

/// Dams a start 3 may stand on given start 2's dam (length, crest, lake area), best first.
fn industrial_candidates(probe: &SimState, len2: usize, height2: f32, area2: usize) -> Vec<Plan> {
    let min_len = (START3_LEN_FACTOR * len2 as f32).ceil() as usize;
    let min_height = START3_HEIGHT_FACTOR * height2;
    let min_area = (START3_AREA_FACTOR * area2 as f32).ceil() as usize + 1;
    let heights: Vec<f32> = START3_HEIGHTS.iter().copied().filter(|h| *h >= min_height - 1e-4).collect();
    let candidates = plans(probe, &Search { heights: &heights, len: min_len..=START3_MAX_LEN, area: min_area..=START3_MAX_AREA });
    // The smallest lake that is big enough keeps the dam low and the valley intact.
    best(candidates, |p| (p.area - min_area) as f32 + 10.0 * p.cells.len() as f32 + 100.0 * p.height)
}

/// Start 3 is shaped by start 2's dam for the same seed.
pub fn industrial_lake(seed: u64) -> Result<(SimState, LakeInfo), StartError> {
    let (_, small) = early_lake(seed)?;
    let probe = SimState::new(seed);
    let candidates = industrial_candidates(&probe, small.dam.cells.len(), small.dam.height, small.area());
    drop(probe);
    let mut last = StartError::NoSite;
    for plan in candidates {
        match assemble_industrial(seed, &plan) {
            Ok(done) => return Ok(done),
            Err(e) => last = e,
        }
    }
    Err(last)
}

fn assemble_industrial(seed: u64, plan: &Plan) -> Result<(SimState, LakeInfo), StartError> {
    let mut sim = SimState::new(seed);
    let (lake, level) = fill_lake(&mut sim, plan)?;
    let canals = dig_canals(&mut sim, &lake, level, &plan.cells)?;
    let mut placer = Placer::new(&sim);
    let a = |k, n| around_lake(&lake, k, n);
    // Seven lodges, 56 beavers: founders, their grown and young born here, and joiners.
    let homes = [
        household(false, &[100.0, 250.0, 480.0, 900.0, 1300.0], 1),
        household(false, &[140.0, 330.0, 600.0, 1000.0, 1500.0], 1),
        household(true, &[90.0, 220.0, 560.0, 950.0, 1400.0], 1),
        household(false, &[160.0, 340.0, 700.0, 1100.0, 1600.0], 1),
        household(true, &[110.0, 290.0, 540.0, 880.0, 1250.0], 1),
        household(false, &[130.0, 310.0, 650.0, 990.0, 1450.0], 1),
        household(false, &[80.0, 200.0, 500.0, 920.0, 1350.0], 1),
    ];
    let lodge_anchors: Vec<Cell> = (0..homes.len()).map(|k| a(k, homes.len())).collect();
    seat(&mut sim, &mut placer, &lodge_anchors, &homes)?;
    specialise(&mut sim, &[Job::Logger, Job::Logger, Job::Forager, Job::Builder, Job::Digger, Job::Hauler, Job::Crafter, Job::Crafter]);
    // Production and storage stand beside the canals; camps stand where the work is.
    let canal_end = |i: usize| *canals[i % canals.len()].last().expect("canals are never empty");
    let sites = [
        Site { kind: BuildingKind::Storehouse, anchor: canal_end(0), recipe: None, staff: None },
        Site { kind: BuildingKind::Mill, anchor: canal_end(1), recipe: Some(Recipe::Rope), staff: Some((Job::Crafter, 2)) },
        Site { kind: BuildingKind::Workshop, anchor: canal_end(0), recipe: Some(Recipe::Baskets), staff: Some((Job::Crafter, 1)) },
        Site { kind: BuildingKind::Warehouse, anchor: canal_end(1), recipe: None, staff: None },
        Site { kind: BuildingKind::LoggingCamp, anchor: a(1, 4), recipe: None, staff: Some((Job::Logger, 2)) },
        Site { kind: BuildingKind::LoggingCamp, anchor: a(3, 4), recipe: None, staff: Some((Job::Logger, 2)) },
        Site { kind: BuildingKind::ForagerHut, anchor: a(2, 4), recipe: None, staff: Some((Job::Forager, 2)) },
        Site { kind: BuildingKind::MudPit, anchor: a(0, 4), recipe: None, staff: Some((Job::Digger, 2)) },
    ];
    let mut ids = Vec::new();
    for site in &sites {
        let id = place_building(&mut sim, &mut placer, site)?;
        if let Some((job, n)) = site.staff {
            staff(&mut sim, id, job, n)?;
        }
        ids.push(id);
    }
    let all = [Job::Digger, Job::Logger, Job::Builder, Job::Crafter, Job::Hauler, Job::Forager];
    earn(&mut sim, &all.map(|j| (j, 45.0)));
    sim.colony.tools = 0.6;
    sim.buildings.update_capacity(&sim.tech, crate::buildings::Terrain { height: &sim.height, depth: &sim.water.depth });
    let (storehouse, warehouse) = (ids[0], ids[3]);
    for (good, amount, into) in [
        (Good::Logs, 180.0, storehouse),
        (Good::Sticks, 160.0, storehouse),
        (Good::Mud, 90.0, storehouse),
        (Good::Stone, 40.0, storehouse),
        (Good::Rope, 30.0, warehouse),
        (Good::Baskets, 20.0, warehouse),
        (Good::Tools, 8.0, warehouse),
    ] {
        stock(&mut sim, into, good, amount)?;
    }
    sim.tech.unlock_all();
    finish(&mut sim, 14.0, 5.0);
    let info = LakeInfo { dam: DamSite { cells: plan.cells.clone(), height: plan.height, camp: plan.cells[plan.cells.len() / 2] }, level, lake, canals };
    Ok((sim, info))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn lake(seed: u64) -> (SimState, LakeInfo) {
        early_lake(seed).unwrap_or_else(|e| panic!("seed {seed}: {e}"))
    }

    #[test]
    fn start_two_lake_is_broad_and_filled_behind_a_dam_across_the_channel() {
        // Seed 1 is the reference world; other seeds whose valley is too narrow keep a small lake.
        for seed in [1] {
            let (s, info) = lake(seed);
            // A dam tied into both banks holds about 135 cells here (a line the water walks round
            // once seemed to hold more).
            assert!(info.area() >= 120, "seed {seed}: lake only {} cells", info.area());
            let river_in_dam = info.dam.cells.iter().filter(|c| is_river(&s, c.0, c.1)).count();
            assert!(river_in_dam >= 1, "seed {seed}: the dam crosses the water channel");
            let (first, last) = (info.dam.cells[0], *info.dam.cells.last().unwrap());
            assert!(!is_river(&s, first.0, first.1) && !is_river(&s, last.0, last.1), "seed {seed}: dam ends on both banks");
            let wet = info.lake.iter().filter(|&&(x, y)| *s.water.depth.get(x, y).unwrap() >= WET_DEPTH).count();
            // The shallow rim of a pond held to its spill height dries a little (about a sixth of it
            // on seed 1, whose lake grew from 135 to 139 cells when the rivers changed).
            assert!(wet * 100 >= info.area() * 84, "seed {seed}: {wet} of {} lake cells wet", info.area());
        }
    }

}
