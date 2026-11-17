//! Starts 2 and 3 (owner: `start23`): a dam and lake that already exist, with a colony living
//! around them. Built directly from the seed and never simulated forward (`time_days` stays 0).
//!
//! * Start 2, the early lake: a short dam holds a young lake; two lodges (about fifteen beavers,
//!   some born here, some who joined), a few elementary buildings and early tech.
//! * Start 3, the industrial lake: a dam at least 1.5 times start 2's in length and crest holds a
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

/// Dam heights tried for start 2: crests low enough that start 3 can stand 1.5 times higher
/// within `START3_HEIGHTS`.
const START2_HEIGHTS: [f32; 4] = [0.5, 0.6, 0.8, 1.0];
const START3_HEIGHTS: [f32; 8] = [0.8, 1.0, 1.2, 1.5, 2.0, 2.5, 3.0, 4.0];
/// Start 2: a lake near this many cells, 4 to 10 dam cells long.
const START2_AREA: usize = 150;
const START2_AREA_RANGE: RangeInclusive<usize> = 60..=400;
const START2_LEN: RangeInclusive<usize> = 4..=10;
/// Start 3: dam up to this long and a lake up to this many cells.
const START3_MAX_LEN: usize = 30;
const START3_MAX_AREA: usize = 4000;
const START3_LEN_FACTOR: f32 = 1.5;
const START3_HEIGHT_FACTOR: f32 = 1.5;
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
                        let Some(p) = crate::dam_preview::preview(sim, &cells, height) else { continue };
                        let on_edge = p.flooded.iter().any(|c| c.0 < 2 || c.1 < 2 || c.0 + 2 >= size || c.1 + 2 >= size);
                        if p.rise >= MIN_RISE && s.area.contains(&p.flooded.len()) && !on_edge {
                            out.push(Plan { cells: cells.clone(), height, area: p.flooded.len() });
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
    let p = crate::dam_preview::preview(sim, &plan.cells, plan.height).ok_or_else(|| err("dam line is not valid"))?;
    sim.water.config.seepage = CLAY_SEEPAGE;
    feed_river(sim, &plan.cells, plan.height)?;
    sim.water.build_dam(&plan.cells, plan.height).map_err(|e| err(e.to_string()))?;
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
    Ok((p.flooded, p.level))
}

// ───────────────────────────── placing things ─────────────────────────────

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
    fn spot(&mut self, sim: &mut SimState, anchor: Cell, (w, h): (usize, usize), water: RangeInclusive<u8>, ok: &dyn Fn(&SimState, Cell) -> bool) -> Option<Cell> {
        let size = self.size;
        let (x0, x1) = (anchor.0.saturating_sub(SEARCH_RADIUS).max(2), (anchor.0 + SEARCH_RADIUS).min(size - w - 3));
        let (y0, y1) = (anchor.1.saturating_sub(SEARCH_RADIUS).max(2), (anchor.1 + SEARCH_RADIUS).min(size - h - 3));
        let mut candidates: Vec<Cell> = (y0..=y1).flat_map(|y| (x0..=x1).map(move |x| (x, y))).collect();
        candidates.sort_by_key(|&c| (dist2((c.0 + w / 2, c.1 + h / 2), anchor), c.1, c.0));
        let found = candidates.into_iter().find(|&c| {
            water.contains(&self.dist((c.0 + w / 2, c.1 + h / 2)))
                && self.cells(c, (w, h), 1).all(|k| !self.taken.contains(&k) && self.is_dry_ground(sim, k))
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
    let at = placer
        .spot(sim, site.anchor, fp, 3..=14, &|s, c| !logging || trees_near(s, c) >= CAMP_TREES)
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
            .spot(sim, anchor, (3, 3), 3..=3, &|_, _| true)
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
    sim.colony.food = food_per_head * n;
    sim.colony.wood = wood_per_head * n;
    sim.work.auto_organise = true;
    strip_disabled(sim);
    sim.buildings.update_capacity(&sim.tech);
    crate::agents::place_beavers(sim);
}

// ───────────────────────────── start 2 ─────────────────────────────

/// Start 2: a young lake, two lodges, four elementary buildings.
pub fn early_lake(seed: u64) -> Result<(SimState, LakeInfo), StartError> {
    let probe = SimState::new(seed);
    let candidates = plans(&probe, &Search { heights: &START2_HEIGHTS, len: START2_LEN, area: START2_AREA_RANGE });
    let candidates = best(candidates, |p| p.area.abs_diff(START2_AREA) as f32 + 15.0 * p.cells.len() as f32);
    drop(probe);
    let mut last = StartError::NoSite;
    for plan in candidates {
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
    sim.buildings.update_capacity(&sim.tech);
    for (good, amount) in [(Good::Logs, 30.0), (Good::Sticks, 25.0), (Good::Mud, 15.0)] {
        stock(&mut sim, warehouse, good, amount)?;
    }
    finish(&mut sim, 12.0, 4.0);
    let info = LakeInfo { dam: DamSite { cells: plan.cells.clone(), height: plan.height }, level, lake, canals: Vec::new() };
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

/// Start 3 is shaped by start 2's dam for the same seed.
pub fn industrial_lake(seed: u64) -> Result<(SimState, LakeInfo), StartError> {
    let (_, small) = early_lake(seed)?;
    let len2 = small.dam.cells.len();
    let min_len = (START3_LEN_FACTOR * len2 as f32).ceil() as usize;
    let min_height = START3_HEIGHT_FACTOR * small.dam.height;
    let min_area = (START3_AREA_FACTOR * small.area() as f32).ceil() as usize + 1;
    let heights: Vec<f32> = START3_HEIGHTS.iter().copied().filter(|h| *h >= min_height - 1e-4).collect();
    let probe = SimState::new(seed);
    let candidates = plans(&probe, &Search { heights: &heights, len: min_len..=START3_MAX_LEN, area: min_area..=START3_MAX_AREA });
    // The smallest lake that is big enough keeps the dam low and the valley intact.
    let candidates = best(candidates, |p| (p.area - min_area) as f32 + 10.0 * p.cells.len() as f32 + 100.0 * p.height);
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
    sim.buildings.update_capacity(&sim.tech);
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
    finish(&mut sim, 14.0, 5.0);
    let info = LakeInfo { dam: DamSite { cells: plan.cells.clone(), height: plan.height }, level, lake, canals };
    Ok((sim, info))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agents::Task;
    use crate::starts::{build_lake, Start};

    const SEEDS: [u64; 3] = [1, 2, 3];

    fn lake(start: Start, seed: u64) -> (SimState, LakeInfo) {
        build_lake(start, seed).unwrap_or_else(|e| panic!("{start:?} seed {seed}: {e}"))
    }

    /// Everything a test can see of a state, for equality.
    fn fingerprint(s: &SimState) -> String {
        let fold = |g: &[f32]| g.iter().fold(0u64, |a, v| a.wrapping_mul(31).wrapping_add(v.to_bits() as u64));
        let beavers: Vec<_> = s.colony.beavers.iter().map(|b| (b.id, &b.name, b.age_days.to_bits(), b.stage, b.family, b.mother, b.skills.map(f32::to_bits), b.pos.0.to_bits(), b.pos.1.to_bits())).collect();
        let trees: Vec<_> = s.resources.items.iter().map(|r| (r.id, r.kind, r.cell, r.amount.to_bits())).collect();
        let agents: Vec<_> = s.agents.by_id.values().map(|a| (a.id, a.cell)).collect();
        let slots: Vec<_> = s.work.places.iter().map(|p| (p.building, p.slots.clone())).collect();
        format!(
            "{} {} {:?} {:?} {:?} {:?} {:?} {:?} {} {} {} {:?} {:?}",
            s.seed, s.time_days, beavers, trees, agents, s.buildings.buildings, slots, s.colony.lodges,
            fold(s.height.as_slice()), fold(s.water.depth.as_slice()), fold(s.water.dam_height.as_slice()),
            s.water.sources, s.tech.unlocked_ids().collect::<Vec<_>>(),
        )
    }

    #[test]
    fn start_two_has_a_dam_a_lake_at_its_level_a_few_buildings_and_a_mixed_colony() {
        for seed in SEEDS {
            let (s, info) = lake(Start::EarlyLake, seed);
            assert!(info.dam.cells.iter().all(|c| s.water.has_dam(c.0, c.1)), "seed {seed}: dam is complete");
            assert!(info.dam.cells.iter().all(|c| s.water.dam_height.get(c.0, c.1) == Some(&info.dam.height)));
            assert!(info.area() > 0);
            for &(x, y) in &info.lake {
                let surface = s.height.get(x, y).unwrap() + s.water.depth.get(x, y).unwrap();
                assert!((surface - info.level).abs() < 1e-3, "seed {seed}: lake cell {:?} at {surface}, level {}", (x, y), info.level);
            }
            let pre = crate::dam_preview::preview(&SimState::new(seed), &info.dam.cells, info.dam.height).unwrap();
            assert_eq!((pre.level, pre.flooded.len()), (info.level, info.area()), "the lake is the one previewed");

            let kinds: Vec<_> = s.buildings.buildings.iter().map(|b| b.kind).collect();
            assert!((3..=5).contains(&kinds.len()), "seed {seed}: {kinds:?}");
            assert!(s.buildings.buildings.iter().all(|b| b.is_complete() && crate::catalogue::def(b.kind).tier == crate::catalogue::Tier::Elementary), "{kinds:?}");

            let c = &s.colony;
            assert!((13..=17).contains(&c.population()), "seed {seed}: {} beavers", c.population());
            let families: HashSet<_> = c.beavers.iter().map(|b| b.family).collect();
            assert!(families.len() >= 2 && c.lodges.len() >= 2, "seed {seed}: {} families", families.len());
            let born = c.beavers.iter().filter(|b| b.mother.is_some()).count();
            assert!(born >= 2 && c.stats.births as usize == born, "seed {seed}: {born} born, births {}", c.stats.births);
            assert!(c.stats.immigrants >= 2, "seed {seed}: {} joined", c.stats.immigrants);
            let events = c.events.iter().map(|e| &e.event);
            assert!(events.clone().filter(|e| matches!(e, crate::colony::Event::Immigrated { .. })).count() >= 2);
            assert!(events.filter(|e| matches!(e, crate::colony::Event::Born { .. })).count() >= 2);
        }
    }

    fn neighbours(c: Cell) -> [Cell; 4] {
        [(c.0.wrapping_sub(1), c.1), (c.0 + 1, c.1), (c.0, c.1.wrapping_sub(1)), (c.0, c.1 + 1)]
    }

    /// Connected groups of canal cells.
    fn canal_groups(s: &SimState) -> Vec<Vec<Cell>> {
        let size = s.world.size;
        let is_canal = |c: Cell| s.water.canal.get(c.0, c.1).copied().unwrap_or(false);
        let mut seen = HashSet::new();
        let mut groups = Vec::new();
        for start in (0..size * size).map(|i| (i % size, i / size)) {
            if !is_canal(start) || !seen.insert(start) {
                continue;
            }
            let (mut group, mut stack) = (Vec::new(), vec![start]);
            while let Some(c) = stack.pop() {
                group.push(c);
                stack.extend(neighbours(c).into_iter().filter(|&n| is_canal(n) && seen.insert(n)));
            }
            groups.push(group);
        }
        groups
    }

    #[test]
    fn start_three_is_a_bigger_dam_and_lake_with_canals_specialists_and_advanced_buildings() {
        for seed in SEEDS {
            let small = lake(Start::EarlyLake, seed).1;
            let (s, big) = lake(Start::IndustrialLake, seed);
            assert!(big.dam.cells.iter().all(|c| s.water.has_dam(c.0, c.1)));
            assert!(big.dam.height >= 1.5 * small.dam.height, "seed {seed}: crest {} vs {}", big.dam.height, small.dam.height);
            assert!(big.dam.cells.len() as f32 >= 1.5 * small.dam.cells.len() as f32, "seed {seed}: length");
            assert!(big.area() >= 3 * small.area(), "seed {seed}: lake {} vs {}", big.area(), small.area());
            assert!(big.lake.iter().all(|&(x, y)| *s.water.depth.get(x, y).unwrap() > 0.0), "every lake cell holds water");
            let wet = big.lake.iter().filter(|&&(x, y)| *s.water.depth.get(x, y).unwrap() >= WET_DEPTH).count();
            assert!(wet * 10 >= big.area() * 9, "seed {seed}: {wet} of {} lake cells are wet", big.area());

            let c = &s.colony;
            assert!(c.population() >= 50, "seed {seed}: {} beavers", c.population());
            let specialists: Vec<_> = c.beavers.iter().filter_map(|b| b.specialty()).collect();
            let kinds: HashSet<_> = specialists.iter().collect();
            assert!(specialists.len() >= 5 && kinds.len() >= 3, "seed {seed}: specialists {specialists:?}");

            let groups = canal_groups(&s);
            assert!(groups.len() >= 2, "seed {seed}: {} canals", groups.len());
            let lake_cells: HashSet<Cell> = big.lake.iter().copied().collect();
            for g in &groups {
                assert!(g.iter().all(|c| *s.water.depth.get(c.0, c.1).unwrap() >= WET_DEPTH), "canal is filled");
                assert!(g.iter().any(|&c| neighbours(c).iter().any(|n| lake_cells.contains(n))), "seed {seed}: canal {g:?} is not linked to the lake");
            }

            use crate::catalogue::{def, Tier};
            let done = |k: BuildingKind| s.buildings.buildings.iter().filter(|b| b.kind == k && b.is_complete()).count();
            assert_eq!((done(BuildingKind::Mill), done(BuildingKind::Storehouse)), (1, 1), "seed {seed}");
            assert!(def(BuildingKind::Mill).tier == Tier::Industrial && def(BuildingKind::Storehouse).tier == Tier::Industrial);
            assert!(s.tech.allows(BuildingKind::Mill) && s.tech.allows(BuildingKind::Storehouse));
            let mill = s.buildings.buildings.iter().find(|b| b.kind == BuildingKind::Mill).unwrap();
            assert!(mill.recipe.is_some());
            let staffed = s.work.place(mill.id).unwrap().slots.iter().filter(|sl| sl.worker.is_some()).count();
            assert_eq!(staffed, 2, "the mill is fully staffed");
            let store = s.buildings.stores.capacity(Good::Logs);
            assert!(store > 500.0 && s.buildings.stores.amount(Good::Logs) > 100.0, "seed {seed}: big storage, {store}");
        }
    }

    #[test]
    fn starts_are_identical_per_seed_differ_between_seeds_and_run_no_step() {
        for start in [Start::EarlyLake, Start::IndustrialLake] {
            let (a, b, c) = (lake(start, 7).0, lake(start, 7).0, lake(start, 8).0);
            assert_eq!(fingerprint(&a), fingerprint(&b), "{start:?} is identical twice for a seed");
            assert_ne!(fingerprint(&a), fingerprint(&c), "{start:?} differs between seeds");
            for s in [&a, &b, &c] {
                assert_eq!((s.time_days, s.colony.time_days, s.water.time_days), (0.0, 0.0, 0.0), "no sim step ran");
                assert_eq!((s.colony.stats.deaths, s.colony.stats.departures), (0, 0));
            }
        }
    }

    #[test]
    fn every_disabled_system_is_off_and_absent_from_the_starts() {
        use crate::features::{Features, System};
        for start in [Start::River, Start::EarlyLake, Start::IndustrialLake] {
            let s = crate::starts::build(start, 5).unwrap_or_else(|e| panic!("{start:?}: {e}"));
            assert_eq!(s.features, Features::scoped(), "{start:?}");
            assert!(System::OUT_OF_SCOPE.iter().all(|sys| !s.features.enabled(*sys)));
            assert!(s.ecology.predators.is_empty(), "{start:?}: predators");
            assert!(s.region.colonies.is_empty() && s.region.offers().is_empty(), "{start:?}: rivals and trade");
            assert!(s.events.log().is_empty());
            assert!(s.buildings.buildings.iter().all(|b| s.features.allows_building(b.kind)), "{start:?}: guard or scent buildings");
            assert!(!s.colony.job_enabled(Job::Guard) && !s.colony.job_enabled(Job::Scout));
            assert!(s.colony.families.iter().all(|f| !f.priorities.contains(&Job::Guard) && !f.priorities.contains(&Job::Scout)), "{start:?}: priorities");
            assert!(s.colony.beavers.iter().all(|b| b.job.is_none_or(|j| s.features.allows_job(j)) && b.current_job.is_none_or(|j| s.features.allows_job(j))));
            assert_eq!([Job::Guard, Job::Scout].map(|j| s.colony.experience[j.index()]), [0.0, 0.0]);
            assert_eq!(s.colony.scouted, 0.0);
            assert!(crate::catalogue::available(&s).iter().all(|k| crate::catalogue::def(*k).system.is_none()));
        }
    }

    /// Beavers a start sends out to work in the world: collectors (a slot or an area).
    fn field_workers(s: &SimState) -> Vec<BeaverId> {
        s.colony
            .beavers
            .iter()
            .filter(|b| match s.work.slot_of(b.id) {
                Some((_, _, role)) => matches!(role, crate::work::Role::Collect(_)),
                None => s.agents.get(b.id).is_some_and(|a| a.assignment.is_some()),
            })
            .map(|b| b.id)
            .collect()
    }

    /// Runs `start` for 30 days; no field worker may stand idle (no task, or resting) in one place for a day.
    fn thirty_days(start: Start, seed: u64, min_workers: usize) {
        let (mut s, info) = lake(start, seed);
        let workers = field_workers(&s);
        assert!(workers.len() >= min_workers, "{start:?}: only {} field workers", workers.len());
        let crafting = s.buildings.buildings.iter().any(|b| b.recipe.is_some());
        let produced = |s: &SimState| Good::ALL.map(|g| s.buildings.stores.produced_total[g.index()]);
        let before = produced(&s);
        let still = |s: &SimState, id: BeaverId| s.agents.get(id).map(|a| (a.cell, matches!(a.task, Task::Idle | Task::Rest)));
        let mut since: Vec<(Option<Cell>, f64)> = workers.iter().map(|_| (None, 0.0)).collect();
        while s.time_days < 30.0 {
            s.step(0.05).unwrap();
            for (i, &id) in workers.iter().enumerate() {
                match still(&s, id) {
                    Some((cell, true)) if since[i].0 == Some(cell) => {
                        let b = s.colony.beaver(id).unwrap();
                        assert!(s.time_days - since[i].1 < 1.0, "{start:?} seed {seed}: {} ({:?}) idle at {cell:?} since day {:.2} (now {:.2}), resting flag {}", b.name, s.agents.get(id).unwrap().task, since[i].1, s.time_days, b.resting);
                    }
                    Some((cell, true)) => since[i] = (Some(cell), s.time_days),
                    _ => since[i] = (None, s.time_days),
                }
            }
        }
        assert!(s.colony.population() >= workers.len(), "{start:?}: the colony survived");
        let after = produced(&s);
        let made = [Good::Baskets, Good::Rope, Good::Tools].iter().any(|g| after[g.index()] > before[g.index()]);
        assert_eq!(crafting, made, "{start:?}: crafting buildings make goods ({before:?} -> {after:?})");
        let held = info.lake.iter().filter(|c| *s.water.depth.get(c.0, c.1).unwrap() >= WET_DEPTH).count();
        assert!(held * 10 >= info.area() * 8, "{start:?}: only {held} of {} lake cells still wet after 30 days", info.area());
    }

    #[test]
    fn start_two_runs_thirty_days_with_no_assigned_beaver_idle_in_place() {
        thirty_days(Start::EarlyLake, 1, 4);
    }

    #[test]
    fn start_three_runs_thirty_days_with_no_assigned_beaver_idle_in_place() {
        thirty_days(Start::IndustrialLake, 1, 8);
    }
}
