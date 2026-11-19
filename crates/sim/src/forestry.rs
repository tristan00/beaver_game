use std::collections::{BTreeMap, BTreeSet, HashSet, VecDeque};

use rand_core::{Rng, SeedableRng};
use rand_pcg::Pcg32;

use crate::agents::{Agent, Task};
use crate::buildings::{BuildingId, BuildingKind, Good, Producer};
use crate::calendar::Season;
use crate::colony::BeaverId;
use crate::grid::Grid;
use crate::nav::Goal;
use crate::resources::{Resource, ResourceId, ResourceKind, ResourceWorld};
use crate::water::Cell;
use crate::work::{Area, Role};
use crate::SimState;

pub const YOUNG_AGE: f32 = 360.0;
pub const MATURE_AGE: f32 = 1080.0;
pub const OLD_AGE: f32 = 5400.0;
pub const DEFAULT_TREE_AGE: f32 = 2000.0;
pub const MOIST_GROWTH: f32 = 1.4;
pub const MOIST_RANGE: usize = 15;
pub const DRY_GROWTH: f32 = 0.7;
pub const DRY_MOISTURE: f32 = 0.32;
pub const TENDED_GROWTH: f32 = 1.5;
pub const TEND_DAYS: f32 = 30.0;
pub const NURSERY_AGE: f32 = 120.0;
pub const STUMP_AGE: f32 = 400.0;
pub const SPREAD_CHANCE: f32 = 5.5e-5;
pub const OLD_DEATH_CHANCE: f32 = 1.0 / 2000.0;
pub const SNAG_ROT_DAYS: f32 = 720.0;
pub const SAPLING_FLOOD_DAYS: f32 = 5.0;
pub const MATURE_DROWN_DAYS: f32 = 60.0;
pub const PLANT_WORK: f32 = 0.5;
pub const NURSERY_PLANT_WORK: f32 = PLANT_WORK / 4.0;
pub const TEND_WORK: f32 = 0.25;
pub const TEND_STICKS: f32 = 0.2;
pub const NURSERY_PER_DAY: f32 = 1.0;
pub const NURSERY_MAX_STOCK: f32 = 60.0;
pub const MAX_AREA: usize = 61;
pub const RUNNING_OUT: usize = 10;
pub const FALLING_SHARE: f32 = 0.05;
pub const FALLING_DAYS: f64 = 90.0;
pub const WATCH_RADIUS: usize = 120;
pub const STATS_DAYS: f64 = 30.0;
const UNREACHABLE_DAYS: f64 = 10.0;
const CLIFF_SLOPE: f32 = 1.2;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum TreeStage {
    Sapling,
    Young,
    Mature,
    Old,
    Snag,
}

impl TreeStage {
    pub const ALL: [TreeStage; 5] = [TreeStage::Sapling, TreeStage::Young, TreeStage::Mature, TreeStage::Old, TreeStage::Snag];

    pub fn of_age(age: f32) -> TreeStage {
        if age < YOUNG_AGE {
            TreeStage::Sapling
        } else if age < MATURE_AGE {
            TreeStage::Young
        } else if age < OLD_AGE {
            TreeStage::Mature
        } else {
            TreeStage::Old
        }
    }

    pub fn fell_yield(self) -> Option<(f32, f32, f32)> {
        match self {
            TreeStage::Sapling => None,
            TreeStage::Young => Some((1.0, 2.0, 1.0)),
            TreeStage::Mature => Some((crate::resources::TREE_LOGS, crate::resources::TREE_BRANCHES, crate::resources::TREE_BARK)),
            TreeStage::Old => Some((2.0, 4.0, 1.0)),
            TreeStage::Snag => Some((0.0, 3.0, 0.0)),
        }
    }

    pub fn is_grown(self) -> bool {
        matches!(self, TreeStage::Young | TreeStage::Mature | TreeStage::Old)
    }
}

impl Resource {
    pub fn stage(&self) -> Option<TreeStage> {
        match self.kind {
            ResourceKind::Tree if self.snag => Some(TreeStage::Snag),
            ResourceKind::Tree => Some(TreeStage::of_age(self.age_days)),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum CutRule {
    #[default]
    MatureOnly,
    ClearCut,
    Thin,
    Coppice,
}

impl CutRule {
    pub const ALL: [CutRule; 4] = [CutRule::MatureOnly, CutRule::ClearCut, CutRule::Thin, CutRule::Coppice];
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum PlantRule {
    #[default]
    FillGaps,
    PlantAll,
    TendOnly,
}

impl PlantRule {
    pub const ALL: [PlantRule; 3] = [PlantRule::FillGaps, PlantRule::PlantAll, PlantRule::TendOnly];
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ForestTask {
    Plant { cell: Cell, progress: f32 },
    Tend { tree: ResourceId, progress: f32 },
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Deed {
    pub day: f64,
    pub cell: Cell,
    pub kind: DeedKind,
    pub by: Option<BuildingId>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeedKind {
    Planted,
    Tended,
    Felled,
}

#[derive(Clone, Debug)]
pub struct Forestry {
    pub cut_rules: BTreeMap<BuildingId, CutRule>,
    pub plant_rules: BTreeMap<BuildingId, PlantRule>,
    pub saplings: f32,
    pub deeds: VecDeque<Deed>,
    pub history: VecDeque<(f64, usize)>,
    size: usize,
    had_tree: Vec<bool>,
    growth: Vec<f32>,
    flood_plan: BTreeSet<Cell>,
    stumps: Vec<Cell>,
    thin_age: BTreeMap<BuildingId, f32>,
    unreachable: BTreeMap<Cell, f64>,
    day: Option<i64>,
    rng: Pcg32,
}

impl Default for Forestry {
    fn default() -> Self {
        Self::new(0)
    }
}

impl Forestry {
    pub fn new(seed: u64) -> Self {
        Forestry {
            cut_rules: BTreeMap::new(),
            plant_rules: BTreeMap::new(),
            saplings: 0.0,
            deeds: VecDeque::new(),
            history: VecDeque::new(),
            size: 0,
            had_tree: Vec::new(),
            growth: Vec::new(),
            flood_plan: BTreeSet::new(),
            stumps: Vec::new(),
            thin_age: BTreeMap::new(),
            unreachable: BTreeMap::new(),
            day: None,
            rng: Pcg32::seed_from_u64(seed ^ 0xF0E5_7E1D),
        }
    }

    pub fn cut_rule(&self, b: BuildingId) -> CutRule {
        self.cut_rules.get(&b).copied().unwrap_or_default()
    }

    pub fn plant_rule(&self, b: BuildingId) -> PlantRule {
        self.plant_rules.get(&b).copied().unwrap_or_default()
    }

    pub fn had_tree(&self, cell: Cell) -> bool {
        cell.0 < self.size && cell.1 < self.size && self.had_tree[cell.1 * self.size + cell.0]
    }

    pub fn ground_growth(&self, cell: Cell) -> f32 {
        if cell.0 < self.size && cell.1 < self.size { self.growth[cell.1 * self.size + cell.0] } else { 1.0 }
    }
}

pub fn start_age(seed: u64, cell: Cell) -> f32 {
    let mut h = seed ^ 0xA6E5_7A6E_u64 ^ ((cell.0 as u64) << 32) ^ (cell.1 as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15);
    let mut next = || {
        h = h.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = h;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        ((z ^ (z >> 31)) >> 40) as f32 / (1u64 << 24) as f32
    };
    let (u, v) = (next(), next());
    if u < 0.15 {
        YOUNG_AGE + v * (MATURE_AGE - YOUNG_AGE)
    } else if u < 0.25 {
        OLD_AGE + v * 8.0 * 360.0
    } else {
        MATURE_AGE + v * (12.0 * 360.0 - MATURE_AGE)
    }
}

pub fn fellable(r: &Resource) -> bool {
    r.kind == ResourceKind::Tree && r.stage() != Some(TreeStage::Sapling)
}

pub fn cuttable(r: &Resource, min_age: f32) -> bool {
    fellable(r) && (r.snag || r.age_days >= min_age)
}

pub fn cut_min_age(sim: &SimState, camp: Option<BuildingId>) -> f32 {
    let Some(b) = camp.filter(|b| sim.work.place(*b).is_some_and(|p| p.kind == BuildingKind::LoggingCamp)) else {
        return MATURE_AGE;
    };
    match sim.forestry.cut_rule(b) {
        CutRule::MatureOnly | CutRule::Coppice => MATURE_AGE,
        CutRule::ClearCut => YOUNG_AGE,
        CutRule::Thin => sim.forestry.thin_age.get(&b).copied().unwrap_or(f32::INFINITY),
    }
}

fn invalid(m: impl Into<String>) -> crate::SimError {
    crate::SimError::InvalidCommand(m.into())
}

pub fn knows(sim: &SimState, name: &str) -> bool {
    crate::tech::TECHS.iter().position(|t| t.name == name).is_some_and(|i| sim.tech.is_unlocked(crate::tech::TechId(i)))
}

pub(crate) fn set_cut_rule(sim: &mut SimState, building: BuildingId, rule: CutRule) -> Result<(), crate::SimError> {
    if sim.buildings.building(building).is_none_or(|b| b.kind != BuildingKind::LoggingCamp) {
        return Err(invalid(format!("building {} is not a logging camp", building.0)));
    }
    if rule != CutRule::MatureOnly && !knows(sim, crate::tech::FORESTRY_TECH) {
        return Err(invalid("cutting rules need Forestry (locked)"));
    }
    if rule == CutRule::Coppice && !knows(sim, crate::tech::COPPICING_TECH) {
        return Err(invalid("coppicing needs Coppicing (locked)"));
    }
    sim.forestry.cut_rules.insert(building, rule);
    refresh_thinning(sim);
    Ok(())
}

pub(crate) fn set_plant_rule(sim: &mut SimState, building: BuildingId, rule: PlantRule) -> Result<(), crate::SimError> {
    if sim.buildings.building(building).is_none_or(|b| b.kind != BuildingKind::ForesterLodge) {
        return Err(invalid(format!("building {} is not a forester's lodge", building.0)));
    }
    sim.forestry.plant_rules.insert(building, rule);
    Ok(())
}

pub(crate) fn check_area(sim: &SimState, building: BuildingId, area: &Area) -> Result<(), crate::SimError> {
    let limited = sim.buildings.building(building).is_some_and(|b| max_area(b.kind).is_some());
    let (w, h) = (area.max.0.saturating_sub(area.min.0) + 1, area.max.1.saturating_sub(area.min.1) + 1);
    if limited && (w > MAX_AREA || h > MAX_AREA) {
        return Err(invalid(format!("InvalidArea: {w}x{h} is larger than {MAX_AREA}x{MAX_AREA}")));
    }
    Ok(())
}

pub fn max_area(kind: BuildingKind) -> Option<usize> {
    matches!(kind, BuildingKind::LoggingCamp | BuildingKind::ForesterLodge).then_some(MAX_AREA)
}

pub fn init(sim: &mut SimState) {
    let n = sim.world.size;
    let f = &mut sim.forestry;
    f.size = n;
    f.had_tree = vec![false; n * n];
    for r in sim.resources.items.iter().filter(|r| r.kind == ResourceKind::Tree) {
        if r.cell.0 < n && r.cell.1 < n {
            f.had_tree[r.cell.1 * n + r.cell.0] = true;
        }
    }
    f.day = Some(sim.time_days.floor() as i64);
    refresh_growth(sim);
    derive_ecology(sim);
}

pub fn step(sim: &mut SimState, dt_days: f32) {
    if sim.forestry.size != sim.world.size {
        init(sim);
    }
    grow_nursery_stock(sim, dt_days);
    let today = sim.time_days.floor() as i64;
    let last = *sim.forestry.day.get_or_insert(today);
    for d in last + 1..=today {
        daily(sim, d);
    }
    sim.forestry.day = Some(today.max(last));
}

fn grow_nursery_stock(sim: &mut SimState, dt: f32) {
    let staffed: usize = sim
        .work
        .places
        .iter()
        .filter(|p| p.kind == BuildingKind::TreeNursery && !p.paused && p.site.is_none())
        .filter(|p| sim.buildings.building(p.building).is_some_and(|b| b.is_complete()))
        .map(|p| p.slots.iter().filter(|s| s.role == Role::Forester && s.worker.is_some()).count())
        .sum();
    let f = &mut sim.forestry;
    f.saplings = (f.saplings + staffed as f32 * NURSERY_PER_DAY * dt).min(NURSERY_MAX_STOCK.max(f.saplings));
}

fn chance(rng: &mut Pcg32, p: f32) -> bool {
    ((rng.next_u32() >> 8) as f32 / (1u32 << 24) as f32) < p
}

fn can_hold_tree(sim: &SimState, cell: Cell) -> bool {
    let (x, y) = cell;
    sim.world.forest.get(x, y).is_some_and(|t| t.is_some())
        && !sim.world.river.get(x, y).copied().unwrap_or(false)
        && !sim.world.lake.get(x, y).copied().unwrap_or(false)
}

fn is_dry(sim: &SimState, cell: Cell) -> bool {
    sim.water.depth.get(cell.0, cell.1).is_some_and(|d| *d < crate::buildings::WET_DEPTH)
        && !sim.world.river.get(cell.0, cell.1).copied().unwrap_or(false)
        && !sim.world.lake.get(cell.0, cell.1).copied().unwrap_or(false)
}

fn building_cells(sim: &SimState) -> HashSet<Cell> {
    sim.buildings.buildings.iter().flat_map(|b| b.cells().collect::<Vec<_>>()).collect()
}

fn daily(sim: &mut SimState, day: i64) {
    refresh_growth(sim);
    refresh_flood_plan(sim);
    let winter = Season::at(day as f64 - 0.5) == Season::Winter;
    let n = sim.world.size;
    let occupied: HashSet<Cell> = sim.resources.items.iter().map(|r| r.cell).collect();
    let built = building_cells(sim);
    let mut occupied = occupied;
    let mut gone: HashSet<ResourceId> = HashSet::new();
    let mut seeds: Vec<Cell> = Vec::new();
    let flood_days = &sim.ecology.flood_days;
    let depth = &sim.water.depth;
    {
        let f = &mut sim.forestry;
        for r in sim.resources.items.iter_mut().filter(|r| r.kind == ResourceKind::Tree) {
            let (x, y) = r.cell;
            let fd = flood_days.get(x, y).copied().unwrap_or(0.0);
            let dp = depth.get(x, y).copied().unwrap_or(0.0);
            if r.snag {
                r.age_days += 1.0;
                if r.age_days >= SNAG_ROT_DAYS {
                    gone.insert(r.id);
                }
                continue;
            }
            match TreeStage::of_age(r.age_days) {
                TreeStage::Sapling if fd >= SAPLING_FLOOD_DAYS && dp > crate::tuning::ecology::FLOOD_DEPTH => {
                    gone.insert(r.id);
                    continue;
                }
                TreeStage::Young if fd >= crate::tuning::ecology::DROWN_FLOOD_DAYS && dp >= crate::tuning::ecology::DROWN_DEPTH => {
                    gone.insert(r.id);
                    continue;
                }
                TreeStage::Mature | TreeStage::Old if fd >= MATURE_DROWN_DAYS && dp >= crate::tuning::ecology::DROWN_DEPTH => {
                    r.snag = true;
                    r.age_days = 0.0;
                    continue;
                }
                _ => {}
            }
            if !winter {
                let ground = if x < n && y < n { f.growth[y * n + x] } else { 1.0 };
                let tend = if r.tended_days > 0.0 { TENDED_GROWTH } else { 1.0 };
                r.age_days += ground * tend;
            }
            r.tended_days = (r.tended_days - 1.0).max(0.0);
            let stage = TreeStage::of_age(r.age_days);
            if stage == TreeStage::Old && chance(&mut f.rng, OLD_DEATH_CHANCE) {
                r.snag = true;
                r.age_days = 0.0;
                continue;
            }
            if !winter && matches!(stage, TreeStage::Mature | TreeStage::Old) && chance(&mut f.rng, SPREAD_CHANCE) {
                let k = (f.rng.next_u32() % 8) as usize;
                let (dx, dy) = [(-1, -1), (0, -1), (1, -1), (-1, 0), (1, 0), (-1, 1), (0, 1), (1, 1)][k];
                let (nx, ny) = (x as i64 + dx, y as i64 + dy);
                if nx >= 0 && ny >= 0 && (nx as usize) < n && (ny as usize) < n {
                    seeds.push((nx as usize, ny as usize));
                }
            }
        }
    }
    sim.resources.items.retain(|r| !gone.contains(&r.id));
    for c in seeds {
        let free = !occupied.contains(&c) && !built.contains(&c) && sim.work.supplies != Some(c);
        let flooded = sim.ecology.flood_days.get(c.0, c.1).is_some_and(|d| *d > 0.0);
        if free && !flooded && can_hold_tree(sim, c) && is_dry(sim, c) && !sim.forestry.flood_plan.contains(&c) {
            sim.resources.add_tree(c, 0.0);
            occupied.insert(c);
        }
    }
    regrow_stumps(sim, &built);
    let f = &mut sim.forestry;
    for r in sim.resources.items.iter().filter(|r| r.kind == ResourceKind::Tree) {
        if r.cell.0 < n && r.cell.1 < n {
            f.had_tree[r.cell.1 * n + r.cell.0] = true;
        }
    }
    let today = day as f64;
    while f.deeds.front().is_some_and(|d| d.day < today - STATS_DAYS) {
        f.deeds.pop_front();
    }
    f.unreachable.retain(|_, until| *until > today);
    refresh_thinning(sim);
    derive_ecology(sim);
    record_history(sim, today);
}

fn regrow_stumps(sim: &mut SimState, built: &HashSet<Cell>) {
    let stumps = std::mem::take(&mut sim.forestry.stumps);
    for c in stumps {
        if built.contains(&c) {
            continue;
        }
        if sim.resources.at(c).next().is_some() {
            sim.forestry.stumps.push(c);
        } else {
            sim.resources.add_tree(c, STUMP_AGE);
        }
    }
}

fn refresh_growth(sim: &mut SimState) {
    let n = sim.world.size;
    let wet: Vec<bool> = (0..n * n)
        .map(|i| {
            let (x, y) = (i % n, i / n);
            sim.water.depth.get(x, y).is_some_and(|d| *d > crate::tuning::ecology::FLOOD_DEPTH)
                || sim.world.river.get(x, y).copied().unwrap_or(false)
                || sim.world.lake.get(x, y).copied().unwrap_or(false)
        })
        .collect();
    let r = MOIST_RANGE;
    let mut rows = vec![false; n * n];
    for y in 0..n {
        for x in 0..n {
            rows[y * n + x] = (x.saturating_sub(r)..=(x + r).min(n - 1)).any(|xx| wet[y * n + xx]);
        }
    }
    let mut growth = vec![1.0; n * n];
    for y in 0..n {
        for x in 0..n {
            let moist = (y.saturating_sub(r)..=(y + r).min(n - 1)).any(|yy| rows[yy * n + x]);
            let dry = sim.world.moisture.get(x, y).is_some_and(|m| *m < DRY_MOISTURE);
            growth[y * n + x] = if moist { MOIST_GROWTH } else if dry { DRY_GROWTH } else { 1.0 };
        }
    }
    sim.forestry.growth = growth;
}

fn refresh_flood_plan(sim: &mut SimState) {
    let mut plan = BTreeSet::new();
    for p in &sim.work.dam_plans {
        if let Some(pv) = crate::dam_preview::preview(sim, &p.cells, p.height) {
            plan.extend(pv.flooded);
        }
    }
    sim.forestry.flood_plan = plan;
}

fn refresh_thinning(sim: &mut SimState) {
    let camps: Vec<(BuildingId, Area)> = sim
        .forestry
        .cut_rules
        .iter()
        .filter(|(_, r)| **r == CutRule::Thin)
        .filter_map(|(b, _)| sim.work.place(*b).and_then(|p| p.area).map(|a| (*b, a)))
        .collect();
    let mut out = BTreeMap::new();
    let n = sim.world.size;
    for (b, a) in camps {
        let cells = (a.min.0.min(n)..=a.max.0.min(n.saturating_sub(1))).count() * (a.min.1.min(n)..=a.max.1.min(n.saturating_sub(1))).count();
        let mut ages: Vec<f32> = sim
            .resources
            .items
            .iter()
            .filter(|r| a.contains(r.cell) && r.stage().is_some_and(TreeStage::is_grown))
            .map(|r| r.age_days)
            .collect();
        let keep = cells.div_ceil(2);
        let allow = ages.len().saturating_sub(keep);
        let threshold = if allow == 0 {
            f32::INFINITY
        } else {
            ages.sort_by(|x, y| y.total_cmp(x));
            ages[allow - 1].max(YOUNG_AGE)
        };
        out.insert(b, threshold);
    }
    sim.forestry.thin_age = out;
}

fn derive_ecology(sim: &mut SimState) {
    let n = sim.ecology.size();
    if n == 0 {
        return;
    }
    let mut cover = vec![0.0f32; n * n];
    for r in sim.resources.items.iter().filter(|r| r.stage().is_some_and(TreeStage::is_grown)) {
        if r.cell.0 < n && r.cell.1 < n {
            cover[r.cell.1 * n + r.cell.0] = 1.0;
        }
    }
    let mut g = Grid::new(n, n, 0.0f32);
    for y in 0..n {
        for x in 0..n {
            let (mut sum, mut cnt) = (0.0, 0.0);
            for yy in y.saturating_sub(1)..=(y + 1).min(n - 1) {
                for xx in x.saturating_sub(1)..=(x + 1).min(n - 1) {
                    sum += cover[yy * n + xx];
                    cnt += 1.0;
                }
            }
            *g.get_mut(x, y).expect("in bounds") = sum / cnt;
        }
    }
    sim.ecology.set_forest(&g);
}

fn home(sim: &SimState) -> Option<Cell> {
    let p = sim.colony.lodges.first().map(|l| l.pos).or(sim.colony.beavers.first().map(|b| b.pos))?;
    Some((p.0.max(0.0) as usize, p.1.max(0.0) as usize))
}

pub fn colony_mature_trees(sim: &SimState) -> Option<usize> {
    let h = home(sim)?;
    Some(
        sim.resources
            .items
            .iter()
            .filter(|r| r.cell.0.abs_diff(h.0).max(r.cell.1.abs_diff(h.1)) <= WATCH_RADIUS)
            .filter(|r| matches!(r.stage(), Some(TreeStage::Mature | TreeStage::Old)))
            .count(),
    )
}

fn record_history(sim: &mut SimState, today: f64) {
    if !sim.colony.is_founded() {
        return;
    }
    let Some(count) = colony_mature_trees(sim) else { return };
    let h = &mut sim.forestry.history;
    h.push_back((today, count));
    while h.front().is_some_and(|(d, _)| *d < today - FALLING_DAYS) {
        h.pop_front();
    }
}

pub fn forest_trend(sim: &SimState) -> Option<(usize, usize)> {
    let h = &sim.forestry.history;
    let (first, last) = (h.front()?, h.back()?);
    (last.0 - first.0 >= FALLING_DAYS - 0.5).then_some((first.1, last.1))
}

impl ResourceWorld {
    pub fn grow_trees(&mut self, days: f32, growth: impl Fn(Cell) -> f32) {
        for r in self.items.iter_mut().filter(|r| r.kind == ResourceKind::Tree && !r.snag) {
            let tend = if r.tended_days > 0.0 { TENDED_GROWTH } else { 1.0 };
            r.age_days += days * growth(r.cell) * tend;
            r.tended_days = (r.tended_days - days).max(0.0);
        }
    }
}

fn plantable(sim: &SimState, cell: Cell, rule: PlantRule, occupied: &HashSet<Cell>, built: &HashSet<Cell>) -> bool {
    let (x, y) = cell;
    let ground = match rule {
        PlantRule::TendOnly => false,
        PlantRule::FillGaps => sim.forestry.had_tree(cell),
        PlantRule::PlantAll => sim.world.slope.get(x, y).is_some_and(|s| *s < CLIFF_SLOPE),
    };
    ground
        && is_dry(sim, cell)
        && sim.ecology.flood_days.get(x, y).is_none_or(|d| *d <= 0.0)
        && !occupied.contains(&cell)
        && !built.contains(&cell)
        && sim.work.supplies != Some(cell)
        && !sim.forestry.flood_plan.contains(&cell)
        && sim.forestry.unreachable.get(&cell).is_none_or(|until| sim.time_days >= *until)
}

fn claimed(sim: &SimState, me: BeaverId) -> (HashSet<Cell>, HashSet<ResourceId>) {
    let (mut cells, mut trees) = (HashSet::new(), HashSet::new());
    for a in sim.agents.by_id.values().filter(|a| a.id != me) {
        match a.task {
            Task::Forest(ForestTask::Plant { cell, .. }) => {
                cells.insert(cell);
            }
            Task::Forest(ForestTask::Tend { tree, .. }) => {
                trees.insert(tree);
            }
            _ => {}
        }
    }
    (cells, trees)
}

fn d2(a: Cell, b: Cell) -> usize {
    a.0.abs_diff(b.0).pow(2) + a.1.abs_diff(b.1).pow(2)
}

pub fn plant_spot(sim: &SimState, area: &Area, rule: PlantRule, from: Cell, taken: &HashSet<Cell>) -> Option<Cell> {
    if rule == PlantRule::TendOnly {
        return None;
    }
    let n = sim.world.size;
    let occupied: HashSet<Cell> = sim.resources.items.iter().filter(|r| area.contains(r.cell)).map(|r| r.cell).collect();
    let built = building_cells(sim);
    (area.min.1..=area.max.1.min(n.saturating_sub(1)))
        .flat_map(|y| (area.min.0..=area.max.0.min(n.saturating_sub(1))).map(move |x| (x, y)))
        .filter(|c| !taken.contains(c) && plantable(sim, *c, rule, &occupied, &built))
        .min_by_key(|&c| (d2(c, from), c.1, c.0))
}

pub fn tend_target(sim: &SimState, area: &Area, from: Cell, taken: &HashSet<ResourceId>) -> Option<ResourceId> {
    sim.resources
        .items
        .iter()
        .filter(|r| area.contains(r.cell) && !taken.contains(&r.id) && r.tended_days <= 0.0)
        .filter(|r| matches!(r.stage(), Some(TreeStage::Sapling | TreeStage::Young)))
        .min_by(|a, b| a.age_days.total_cmp(&b.age_days).then(d2(a.cell, from).cmp(&d2(b.cell, from))).then(a.id.cmp(&b.id)))
        .map(|r| r.id)
}

pub(crate) fn next_task(sim: &SimState, lodge: BuildingId, slot: usize, who: BeaverId, from: Cell) -> Option<ForestTask> {
    let place = sim.work.place(lodge)?;
    if place.kind != BuildingKind::ForesterLodge || !sim.buildings.building(lodge).is_some_and(|b| b.is_complete()) {
        return None;
    }
    let area = place.area?;
    let rule = sim.forestry.plant_rule(lodge);
    let (cells, trees) = claimed(sim, who);
    let plant = || plant_spot(sim, &area, rule, from, &cells).map(|cell| ForestTask::Plant { cell, progress: 0.0 });
    let tend = || tend_target(sim, &area, from, &trees).map(|tree| ForestTask::Tend { tree, progress: 0.0 });
    if slot % 2 == 0 { plant().or_else(tend) } else { tend().or_else(plant) }
}

fn record(sim: &mut SimState, cell: Cell, kind: DeedKind, by: Option<BuildingId>) {
    let day = sim.time_days;
    sim.forestry.deeds.push_back(Deed { day, cell, kind, by });
}

pub(crate) fn felled(sim: &mut SimState, who: BeaverId, cell: Cell) {
    let camp = sim.work.slot_of(who).map(|s| s.0);
    record(sim, cell, DeedKind::Felled, camp);
    if let Some(b) = camp {
        if sim.forestry.cut_rule(b) == CutRule::Coppice && knows(sim, crate::tech::COPPICING_TECH) {
            sim.forestry.stumps.push(cell);
        }
    }
}

pub(crate) fn work(sim: &mut SimState, ag: &mut Agent, dt: f32, lodge: Option<BuildingId>, task: ForestTask) {
    let quit = |ag: &mut Agent| {
        ag.path = None;
        ag.task = Task::Idle;
    };
    let Some(lodge) = lodge else { return quit(ag) };
    let Some(area) = sim.work.place(lodge).and_then(|p| p.area) else { return quit(ag) };
    let rule = sim.forestry.plant_rule(lodge);
    let target = match task {
        ForestTask::Plant { cell, .. } => {
            let occupied: HashSet<Cell> = sim.resources.at(cell).map(|r| r.cell).collect();
            if !area.contains(cell) || !plantable(sim, cell, rule, &occupied, &building_cells(sim)) {
                return quit(ag);
            }
            cell
        }
        ForestTask::Tend { tree, .. } => match sim.resources.get(tree) {
            Some(r) if area.contains(r.cell) && r.tended_days <= 0.0 && matches!(r.stage(), Some(TreeStage::Sapling | TreeStage::Young)) => r.cell,
            _ => return quit(ag),
        },
    };
    let near = target.0.abs_diff(ag.cell.0) + target.1.abs_diff(ag.cell.1) <= 1;
    if !near {
        if ag.path.is_none() {
            ag.path = sim.nav.find_path(ag.id, ag.cell, Goal::Beside(target));
            if ag.path.is_none() {
                if let ForestTask::Plant { cell, .. } = task {
                    sim.forestry.unreachable.insert(cell, sim.time_days + UNREACHABLE_DAYS);
                }
                crate::agents::note(ag, false);
                return quit(ag);
            }
        }
        let moved = crate::agents::follow(&mut sim.nav, ag, dt);
        crate::agents::note(ag, moved);
        return;
    }
    ag.path = None;
    crate::agents::note(ag, true);
    let done = dt * ag.speed;
    match task {
        ForestTask::Plant { cell, progress } => {
            let progress = progress + done;
            let stock = sim.forestry.saplings >= 1.0;
            let need = if stock { NURSERY_PLANT_WORK } else { PLANT_WORK };
            if progress + 1e-4 < need {
                ag.task = Task::Forest(ForestTask::Plant { cell, progress });
                return;
            }
            let age = if stock {
                sim.forestry.saplings -= 1.0;
                NURSERY_AGE
            } else {
                0.0
            };
            sim.resources.add_tree(cell, age);
            if cell.0 < sim.forestry.size && cell.1 < sim.forestry.size {
                let n = sim.forestry.size;
                sim.forestry.had_tree[cell.1 * n + cell.0] = true;
            }
            record(sim, cell, DeedKind::Planted, Some(lodge));
            quit(ag);
        }
        ForestTask::Tend { tree, progress } => {
            let progress = progress + done;
            if progress + 1e-4 < TEND_WORK {
                ag.task = Task::Forest(ForestTask::Tend { tree, progress });
                return;
            }
            if let Some(r) = sim.resources.items.iter_mut().find(|r| r.id == tree) {
                r.tended_days = TEND_DAYS;
            }
            if let Ok(got) = sim.buildings.stores.deposit_by(Good::Sticks, TEND_STICKS, Producer::Building(lodge)) {
                if got > 0.0 {
                    sim.output.record(lodge, Good::Sticks, got);
                }
            }
            record(sim, target, DeedKind::Tended, Some(lodge));
            quit(ag);
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ForestReport {
    pub saplings: usize,
    pub young: usize,
    pub mature: usize,
    pub old: usize,
    pub planted_per_day: f32,
    pub tended_per_day: f32,
    pub logs_ready_in_days: Option<f32>,
}

fn per_day(sim: &SimState, area: &Area, kind: DeedKind) -> f32 {
    let window = STATS_DAYS.min(sim.time_days.max(1.0));
    let n = sim.forestry.deeds.iter().filter(|d| d.kind == kind && area.contains(d.cell) && d.day >= sim.time_days - STATS_DAYS).count();
    n as f32 / window as f32
}

fn days_to_mature(sim: &SimState, r: &Resource) -> f32 {
    let rate = sim.forestry.ground_growth(r.cell) * 0.75;
    let tended = r.tended_days.min(TEND_DAYS) * (TENDED_GROWTH - 1.0) * rate;
    ((MATURE_AGE - r.age_days - tended).max(0.0) / rate.max(1e-3)).max(0.0)
}

pub fn forest_report(sim: &SimState, area: &Area) -> ForestReport {
    let mut rep = ForestReport::default();
    let mut soonest: Option<f32> = None;
    for r in sim.resources.items.iter().filter(|r| area.contains(r.cell)) {
        match r.stage() {
            Some(TreeStage::Sapling) => rep.saplings += 1,
            Some(TreeStage::Young) => rep.young += 1,
            Some(TreeStage::Mature) => rep.mature += 1,
            Some(TreeStage::Old) => rep.old += 1,
            _ => continue,
        }
        if matches!(r.stage(), Some(TreeStage::Sapling | TreeStage::Young)) {
            let d = days_to_mature(sim, r);
            soonest = Some(soonest.map_or(d, |s: f32| s.min(d)));
        }
    }
    rep.planted_per_day = per_day(sim, area, DeedKind::Planted);
    rep.tended_per_day = per_day(sim, area, DeedKind::Tended);
    rep.logs_ready_in_days = soonest;
    rep
}

pub fn cuttable_in_area(sim: &SimState, camp: BuildingId) -> usize {
    let Some(area) = sim.work.place(camp).and_then(|p| p.area) else { return 0 };
    let min_age = cut_min_age(sim, Some(camp));
    sim.resources.items.iter().filter(|r| area.contains(r.cell) && cuttable(r, min_age)).count()
}

pub fn felled_per_day(sim: &SimState, camp: BuildingId) -> f32 {
    let window = STATS_DAYS.min(sim.time_days.max(1.0));
    let n = sim.forestry.deeds.iter().filter(|d| d.kind == DeedKind::Felled && d.by == Some(camp) && d.day >= sim.time_days - STATS_DAYS).count();
    n as f32 / window as f32
}

pub fn days_until_out(sim: &SimState, camp: BuildingId) -> Option<f32> {
    let rate = felled_per_day(sim, camp);
    (rate > 1e-3).then(|| cuttable_in_area(sim, camp) as f32 / rate)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::buildings::{Building, Status};
    use crate::work::{Priority, Slot, Workplace};

    fn bare(seed: u64) -> SimState {
        let mut s = SimState::new(seed);
        s.resources = Default::default();
        s.water.depth.as_mut_slice().iter_mut().for_each(|d| *d = 0.0);
        s.ecology.flood_days.as_mut_slice().iter_mut().for_each(|d| *d = 0.0);
        for v in s.world.river.as_mut_slice() {
            *v = false;
        }
        for v in s.world.lake.as_mut_slice() {
            *v = false;
        }
        s.world.moisture.as_mut_slice().iter_mut().for_each(|m| *m = 0.5);
        for t in s.world.forest.as_mut_slice() {
            *t = Some(crate::worldgen::Tree::Aspen);
        }
        s.world.slope.as_mut_slice().iter_mut().for_each(|v| *v = 0.1);
        init(&mut s);
        s
    }

    fn days(s: &mut SimState, n: usize) {
        for _ in 0..n {
            s.time_days += 1.0;
            step(s, 1.0);
        }
    }

    fn age(s: &SimState, id: ResourceId) -> f32 {
        s.resources.get(id).unwrap().age_days
    }

    #[test]
    fn trees_age_by_ground_and_tending_and_stop_in_winter() {
        let mut s = bare(1);
        let plain = s.resources.add_tree((40, 40), 0.0);
        let tended = s.resources.add_tree((44, 40), 0.0);
        s.resources.items.iter_mut().find(|r| r.id == tended).unwrap().tended_days = TEND_DAYS;
        *s.water.depth.get_mut(60, 60).unwrap() = 1.0;
        let moist = s.resources.add_tree((62, 60), 0.0);
        *s.world.moisture.get_mut(20, 20).unwrap() = 0.1;
        let dry = s.resources.add_tree((20, 20), 0.0);
        days(&mut s, 10);
        assert!((age(&s, plain) - 10.0).abs() < 1e-3, "{}", age(&s, plain));
        assert!((age(&s, tended) - 15.0).abs() < 1e-3);
        assert!((age(&s, moist) - 14.0).abs() < 1e-3);
        assert!((age(&s, dry) - 7.0).abs() < 1e-3);
        s.time_days = 270.0;
        s.forestry.day = Some(270);
        let before = age(&s, plain);
        days(&mut s, 30);
        assert_eq!(age(&s, plain), before, "no growth in winter");
        assert_eq!(s.resources.get(plain).unwrap().stage(), Some(TreeStage::Sapling));
        s.resources.items.iter_mut().find(|r| r.id == plain).unwrap().age_days = YOUNG_AGE;
        assert_eq!(s.resources.get(plain).unwrap().stage(), Some(TreeStage::Young));
        assert!(fellable(s.resources.get(plain).unwrap()));
    }

    #[test]
    fn floods_kill_saplings_and_young_trees_but_not_at_once_mature_ones() {
        let mut s = bare(2);
        let sapling = s.resources.add_tree((30, 30), 10.0);
        let young = s.resources.add_tree((31, 30), 500.0);
        let mature = s.resources.add_tree((32, 30), 2000.0);
        let dry = s.resources.add_tree((50, 50), 10.0);
        for x in 30..=32 {
            *s.water.depth.get_mut(x, 30).unwrap() = 0.5;
        }
        for d in 1..=25 {
            for x in 30..=32 {
                *s.ecology.flood_days.get_mut(x, 30).unwrap() = d as f32;
            }
            days(&mut s, 1);
        }
        assert!(s.resources.get(sapling).is_none(), "a flooded sapling drowns");
        assert!(s.resources.get(young).is_none(), "a young tree drowns after the ecology drowning days");
        assert_eq!(s.resources.get(mature).unwrap().stage(), Some(TreeStage::Mature), "mature trees stand longer");
        assert!(s.resources.get(dry).is_some());
        for d in 26..=70 {
            *s.ecology.flood_days.get_mut(32, 30).unwrap() = d as f32;
            days(&mut s, 1);
        }
        assert_eq!(s.resources.get(mature).unwrap().stage(), Some(TreeStage::Snag), "a long flood leaves a snag");
    }

    #[test]
    fn felling_yields_by_stage_and_saplings_cannot_be_felled() {
        let mut w = ResourceWorld::default();
        let s = w.add_tree((1, 1), 100.0);
        assert_eq!(w.fell(s, 5.0), Err(crate::resources::ResourceError::TooYoung));
        assert!(!w.blocks((1, 1)), "a sapling does not block");
        for (age, logs, sticks) in [(500.0, 1.0, 2.0), (2000.0, 3.0, 4.0), (6000.0, 2.0, 4.0)] {
            let t = w.add_tree((5, 5), age);
            assert!(w.blocks((5, 5)));
            assert_eq!(w.fell(t, 5.0), Ok(true));
            let r = w.get(t).unwrap();
            assert_eq!((r.amount, r.branches), (logs, sticks), "age {age}");
            w.items.retain(|r| r.id != t);
        }
    }

    #[test]
    fn starting_forest_is_mostly_mature_with_young_and_old_and_seeded() {
        let s = SimState::new(5);
        let stages: Vec<TreeStage> = s.resources.items.iter().filter_map(|r| r.stage()).collect();
        let share = |st| stages.iter().filter(|x| **x == st).count() as f32 / stages.len() as f32;
        assert!((0.12..0.18).contains(&share(TreeStage::Young)), "young {}", share(TreeStage::Young));
        assert!((0.07..0.13).contains(&share(TreeStage::Old)), "old {}", share(TreeStage::Old));
        assert!(share(TreeStage::Mature) > 0.7 && share(TreeStage::Sapling) == 0.0);
        let again = SimState::new(5);
        assert!(s.resources.items.iter().zip(&again.resources.items).all(|(a, b)| a.age_days == b.age_days));
        assert_ne!(start_age(5, (10, 10)), start_age(6, (10, 10)));
    }

    #[test]
    fn spread_seeds_about_one_or_two_saplings_per_hundred_mature_trees_a_year() {
        let mut s = bare(3);
        for i in 0..400 {
            s.resources.add_tree((5 + (i % 20) * 5, 5 + (i / 20) * 5), 2000.0);
        }
        days(&mut s, 360 * 3);
        let saplings = s.resources.items.iter().filter(|r| r.age_days < 1500.0).count();
        let per_100_year = saplings as f32 / 4.0 / 3.0;
        assert!((0.8..2.5).contains(&per_100_year), "{saplings} saplings: {per_100_year} per 100 a year");
    }

    #[test]
    fn ecology_forest_density_follows_the_trees_and_falls_with_felling() {
        let mut s = bare(4);
        for y in 20..40 {
            for x in 20..40 {
                s.resources.add_tree((x, y), 2000.0);
            }
        }
        days(&mut s, 1);
        let dense = *s.ecology.forest.get(30, 30).unwrap();
        assert!(dense > 0.99, "{dense}");
        assert_eq!(*s.ecology.forest.get(60, 60).unwrap(), 0.0, "no trees, no forest");
        let ids: Vec<ResourceId> = s.resources.items.iter().filter(|r| r.cell.0 < 35).map(|r| r.id).collect();
        for id in ids {
            s.resources.fell(id, 5.0).unwrap();
        }
        days(&mut s, 1);
        assert!(*s.ecology.forest.get(25, 30).unwrap() < 0.01, "felled ground has no forest");
        assert!(*s.ecology.forest.get(38, 30).unwrap() > 0.99);
        let mean = |s: &SimState| s.ecology.forest.as_slice().iter().sum::<f32>();
        assert!(mean(&s) < dense * 400.0 * 0.5);
    }

    fn place(s: &mut SimState, id: u64, kind: BuildingKind, pos: Cell, area: Area) -> BuildingId {
        let b = BuildingId(id);
        s.buildings.buildings.push(Building { id: b, kind, pos, status: Status::Complete, recipe: None, output: 0.0 });
        let slots = crate::catalogue::def(kind).slots.iter().map(|&r| Slot::empty(r)).collect();
        let mut p = Workplace::new(b, kind, slots, Some(area), None);
        p.priority = Priority::Normal;
        s.work.places.push(p);
        b
    }

    #[test]
    fn mature_only_never_offers_young_trees_and_thin_keeps_half() {
        let mut s = bare(5);
        s.tech.unlock_all();
        let area = Area { min: (10, 10), max: (13, 13) };
        let camp = place(&mut s, 900, BuildingKind::LoggingCamp, (20, 20), area);
        for (i, age) in [500.0, 700.0, 900.0, 1500.0, 3000.0, 6000.0].into_iter().enumerate() {
            s.resources.add_tree((10 + i % 4, 10 + i / 4), age);
        }
        for i in 6..12 {
            s.resources.add_tree((10 + i % 4, 10 + i / 4), 400.0 + i as f32);
        }
        let ok = |s: &SimState| {
            let m = cut_min_age(s, Some(camp));
            s.resources.items.iter().filter(|r| cuttable(r, m)).map(|r| r.age_days).collect::<Vec<_>>()
        };
        assert!(ok(&s).iter().all(|a| *a >= MATURE_AGE), "mature only: {:?}", ok(&s));
        assert_eq!(ok(&s).len(), 3);
        s.apply(crate::Command::SetCutRule { building: camp, rule: CutRule::ClearCut }).unwrap();
        assert_eq!(ok(&s).len(), 12);
        s.apply(crate::Command::SetCutRule { building: camp, rule: CutRule::Thin }).unwrap();
        let mut thin = ok(&s);
        thin.sort_by(f32::total_cmp);
        assert_eq!(thin, vec![900.0, 1500.0, 3000.0, 6000.0]);
        assert!(s.apply(crate::Command::SetCutRule { building: BuildingId(12345), rule: CutRule::Thin }).is_err());
        let mut locked = bare(5);
        let camp2 = place(&mut locked, 901, BuildingKind::LoggingCamp, (20, 20), area);
        assert!(locked.apply(crate::Command::SetCutRule { building: camp2, rule: CutRule::Thin }).is_err(), "rules need Forestry");
    }

    #[test]
    fn mature_only_loggers_never_fell_young_trees() {
        use crate::jobs::fixtures::{colony, near, workplace};
        let (mut s, ids) = colony();
        forestry_ready(&mut s);
        let camp = workplace(&mut s, 50, BuildingKind::LoggingCamp, (2, 2), Priority::Normal);
        let mut young = Vec::new();
        for dx in 1..6 {
            young.push(s.resources.add_tree(near(&s, dx, 3), 600.0));
        }
        let old = s.resources.add_tree(near(&s, 3, 5), 3000.0);
        s.apply(crate::Command::FillSlot { building: camp, slot: 0, who: ids[0] }).unwrap();
        for _ in 0..200 {
            s.step(0.05).unwrap();
        }
        assert!(young.iter().all(|t| s.resources.get(*t).is_some_and(|r| r.kind == ResourceKind::Tree)), "young trees stand");
        assert!(s.resources.get(old).is_none_or(|r| r.kind != ResourceKind::Tree), "the mature tree was felled");
    }

    #[test]
    fn wood_alerts_warn_of_a_cut_out_camp_and_a_falling_forest() {
        use crate::hud::{problems, Fix, ProblemKind};
        use crate::jobs::fixtures::{colony, near, workplace};
        let (mut s, _) = colony();
        forestry_ready(&mut s);
        s.buildings.stores.deposit(Good::Food, 80.0).unwrap();
        let camp = workplace(&mut s, 50, BuildingKind::LoggingCamp, (2, 2), Priority::Normal);
        for dx in 0..4 {
            s.resources.add_tree(near(&s, dx, 3), 2000.0);
            s.resources.add_tree(near(&s, dx, 4), 100.0);
        }
        let alert = |s: &SimState, k: ProblemKind| problems(s).into_iter().find(|p| p.kind == k);
        let p = alert(&s, ProblemKind::LoggingAreaRunningOut).expect("a camp with 4 mature trees is running out");
        assert_eq!((p.value, p.fix), (4.0, Fix::Focus), "no forester's lodge to offer before Forestry");
        s.tech.unlock_all();
        assert_eq!(alert(&s, ProblemKind::LoggingAreaRunningOut).unwrap().fix, Fix::Place(BuildingKind::ForesterLodge));
        let area = s.work.place(camp).unwrap().area.unwrap();
        for y in area.min.1..=area.max.1 {
            for x in area.min.0..area.max.0.min(area.min.0 + 2) {
                s.resources.add_tree((x, y), 3000.0);
            }
        }
        assert!(alert(&s, ProblemKind::LoggingAreaRunningOut).is_none(), "enough mature trees again");
        assert!(alert(&s, ProblemKind::ForestFalling).is_none());
        s.forestry.history = [(0.0, 200), (90.0, 195)].into_iter().collect();
        assert!(alert(&s, ProblemKind::ForestFalling).is_none(), "2.5 % is not falling");
        s.forestry.history = [(0.0, 200), (45.0, 190), (90.0, 180)].into_iter().collect();
        let p = alert(&s, ProblemKind::ForestFalling).expect("down 10 %");
        assert_eq!((p.value, p.fix), (10.0, Fix::Focus));
        s.forestry.history = [(30.0, 200), (90.0, 100)].into_iter().collect();
        assert!(alert(&s, ProblemKind::ForestFalling).is_none(), "not a season of history yet");
    }

    #[test]
    fn coppiced_stumps_regrow_young_and_nursery_saplings_plant_fast_with_a_head_start() {
        let mut s = bare(7);
        s.tech.unlock_all();
        let area = Area { min: (10, 10), max: (20, 20) };
        let camp = place(&mut s, 900, BuildingKind::LoggingCamp, (30, 30), area);
        s.apply(crate::Command::SetCutRule { building: camp, rule: CutRule::Coppice }).unwrap();
        s.work.assign(camp, 0, BeaverId(77)).unwrap();
        let t = s.resources.add_tree((12, 12), 2000.0);
        s.resources.fell(t, 5.0).unwrap();
        felled(&mut s, BeaverId(77), (12, 12));
        days(&mut s, 1);
        assert!(s.resources.at((12, 12)).all(|r| r.kind == ResourceKind::Felled), "the stump waits for the wood to be cleared");
        s.resources.items.retain(|r| r.id != t);
        days(&mut s, 1);
        let stump = s.resources.at((12, 12)).next().expect("the stump regrew");
        assert_eq!((stump.stage(), stump.age_days), (Some(TreeStage::Young), STUMP_AGE));
        assert_eq!(felled_per_day(&s, camp) * STATS_DAYS.min(s.time_days) as f32, 1.0);
        let nursery = place(&mut s, 902, BuildingKind::TreeNursery, (40, 40), area);
        s.work.assign(nursery, 0, BeaverId(78)).unwrap();
        step(&mut s, 2.0);
        assert!((s.forestry.saplings - 2.0 * NURSERY_PER_DAY).abs() < 1e-4);
        let lodge = place(&mut s, 901, BuildingKind::ForesterLodge, (30, 40), area);
        s.apply(crate::Command::SetPlantRule { building: lodge, rule: PlantRule::PlantAll }).unwrap();
        s.agents.spawn(BeaverId(79), (15, 14));
        let mut ag = s.agents.by_id.remove(&BeaverId(79)).unwrap();
        let mut ticks = 0;
        let mut task = Some(ForestTask::Plant { cell: (15, 15), progress: 0.0 });
        while let Some(t) = task {
            work(&mut s, &mut ag, 0.05, Some(lodge), t);
            task = if let Task::Forest(t) = ag.task { Some(t) } else { None };
            ticks += 1;
        }
        let planted = s.resources.at((15, 15)).next().expect("planted");
        assert_eq!(planted.age_days, NURSERY_AGE);
        assert!(ticks as f32 * 0.05 <= NURSERY_PLANT_WORK + 0.06, "{ticks} ticks");
        assert!((s.forestry.saplings - 1.0).abs() < 1e-4, "one sapling used");
    }

    fn forestry_ready(s: &mut SimState) {
        init(s);
    }

    #[test]
    fn a_forester_plants_saplings_only_inside_its_area() {
        use crate::jobs::fixtures::{colony, near, workplace};
        let (mut s, ids) = colony();
        s.tech.unlock_all();
        forestry_ready(&mut s);
        let lodge = workplace(&mut s, 60, BuildingKind::ForesterLodge, (2, 2), Priority::Normal);
        let c = near(&s, 2, 4);
        let area = Area { min: c, max: (c.0 + 3, c.1 + 3) };
        s.apply(crate::Command::SetWorkArea { building: lodge, area }).unwrap();
        s.apply(crate::Command::SetPlantRule { building: lodge, rule: PlantRule::PlantAll }).unwrap();
        let saplings = |s: &SimState, inside: bool| s.resources.items.iter().filter(|r| r.stage() == Some(TreeStage::Sapling) && area.contains(r.cell) == inside).count();
        s.apply(crate::Command::FillSlot { building: lodge, slot: 0, who: ids[0] }).unwrap();
        for _ in 0..120 {
            s.step(0.05).unwrap();
        }
        assert!(saplings(&s, true) >= 3, "planted {}", saplings(&s, true));
        assert_eq!(saplings(&s, false), 0, "nothing planted outside the area");
        let rep = forest_report(&s, &area);
        assert_eq!(rep.saplings, saplings(&s, true));
        assert!(rep.planted_per_day > 0.0 && rep.logs_ready_in_days.is_some());
        let too_big = Area { min: (0, 0), max: (MAX_AREA, 3) };
        assert!(s.apply(crate::Command::SetWorkArea { building: lodge, area: too_big }).is_err());
    }

    fn camp_run(lodge_on: bool, rate: f32) -> (usize, usize, usize) {
        let mut s = bare(6);
        s.tech.unlock_all();
        let area = Area { min: (30, 30), max: (44, 44) };
        for y in 30..=44 {
            for x in 30..=44 {
                if (x + y) % 3 != 0 {
                    s.resources.add_tree((x, y), start_age(6, (x, y)));
                }
            }
        }
        for y in 30..=44 {
            *s.water.depth.get_mut(46, y).unwrap() = 1.0;
        }
        init(&mut s);
        let camp = place(&mut s, 900, BuildingKind::LoggingCamp, (50, 50), area);
        let lodge = place(&mut s, 901, BuildingKind::ForesterLodge, (50, 54), area);
        s.forestry.plant_rules.insert(lodge, PlantRule::PlantAll);
        let mature = |s: &SimState| {
            let r = forest_report(s, &area);
            r.mature + r.old
        };
        let start = mature(&s);
        let mut low = start;
        let (mut cut, mut work) = (0.0f32, [0.0f32; 2]);
        for _ in 0..3 * 360 {
            s.time_days += 1.0;
            step(&mut s, 1.0);
            cut += rate;
            while cut >= 1.0 {
                cut -= 1.0;
                let m = cut_min_age(&s, Some(camp));
                if let Some(id) = s.resources.items.iter().filter(|r| area.contains(r.cell) && cuttable(r, m)).min_by_key(|r| d2(r.cell, (45, 37))).map(|r| r.id) {
                    s.resources.fell(id, 5.0).unwrap();
                    s.resources.items.retain(|r| r.id != id);
                }
            }
            low = low.min(mature(&s));
            if !lodge_on {
                continue;
            }
            s.forestry.saplings += 1.0;
            for (slot, w) in work.iter_mut().enumerate() {
                *w += 0.8;
                while *w > 0.0 {
                    let Some(t) = next_task(&s, lodge, slot, BeaverId(999), (45, 37)) else {
                        *w = 0.0;
                        break;
                    };
                    match t {
                        ForestTask::Plant { cell, .. } => {
                            let stock = s.forestry.saplings >= 1.0;
                            *w -= if stock { NURSERY_PLANT_WORK } else { PLANT_WORK } + 0.1;
                            if stock {
                                s.forestry.saplings -= 1.0;
                            }
                            s.resources.add_tree(cell, if stock { NURSERY_AGE } else { 0.0 });
                        }
                        ForestTask::Tend { tree, .. } => {
                            *w -= TEND_WORK + 0.1;
                            s.resources.items.iter_mut().find(|r| r.id == tree).unwrap().tended_days = TEND_DAYS;
                        }
                    }
                }
            }
        }
        (start, low, mature(&s))
    }

    #[test]
    fn a_camp_and_a_forester_lodge_keep_the_area_from_collapsing_over_three_years() {
        let (start, low, kept) = camp_run(true, 0.125);
        let (_, _, bare_end) = camp_run(false, 0.125);
        assert!(start > 100, "the area starts as a forest ({start} mature)");
        assert!(low * 3 >= start && kept * 2 >= start, "with the lodge: low {low}, end {kept} of {start}");
        assert!(bare_end * 3 < start && kept > bare_end + 20, "with the lodge {kept}, without {bare_end}");
    }
}
