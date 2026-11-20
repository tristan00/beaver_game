use std::collections::{BTreeMap, BTreeSet, HashSet, VecDeque};

use rand_core::{Rng, SeedableRng};
use rand_pcg::Pcg32;

use crate::agents::{Agent, Task};
use crate::buildings::{BuildingId, BuildingKind, Good, Producer};
use crate::calendar::Season;
use crate::colony::BeaverId;
use crate::grid::Grid;
use crate::nav::Goal;
use crate::resources::{Resource, ResourceId, ResourceKind};
use crate::water::Cell;
use crate::work::Area;
use crate::SimState;

pub const YOUNG_AGE: f32 = 360.0;
pub const MATURE_AGE: f32 = 1080.0;
pub const OLD_AGE: f32 = 5400.0;
pub const DEFAULT_TREE_AGE: f32 = 2000.0;
pub const MOIST_GROWTH: f32 = 1.4;
pub const MOIST_RANGE: usize = 15;
pub const DRY_GROWTH: f32 = 0.7;
pub const DRY_MOISTURE: f32 = 0.32;
pub const TENDED_GROWTH: f32 = 1.8;
pub const TEND_DAYS: f32 = 45.0;
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
pub const TEND_STICKS: f32 = 0.3;
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

pub fn can_manage_forest(sim: &SimState) -> bool {
    sim.sandbox || sim.has_building(BuildingKind::ForesterLodge)
}
pub fn can_coppice(sim: &SimState) -> bool {
    sim.sandbox || sim.has_building(BuildingKind::TreeNursery)
}

pub(crate) fn set_cut_rule(sim: &mut SimState, building: BuildingId, rule: CutRule) -> Result<(), crate::SimError> {
    if sim.buildings.building(building).is_none_or(|b| b.kind != BuildingKind::LoggingCamp) {
        return Err(invalid(format!("building {} is not a logging camp", building.0)));
    }
    if rule != CutRule::MatureOnly && !can_manage_forest(sim) {
        return Err(invalid("cutting rules require a finished forester lodge"));
    }
    if rule == CutRule::Coppice && !can_coppice(sim) {
        return Err(invalid("coppicing requires a finished tree nursery"));
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
    let _ = dt_days;
    let today = sim.time_days.floor() as i64;
    let last = *sim.forestry.day.get_or_insert(today);
    for d in last + 1..=today {
        daily(sim, d);
    }
    sim.forestry.day = Some(today.max(last));
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
        if sim.forestry.cut_rule(b) == CutRule::Coppice && can_coppice(sim) {
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
    let done = dt * crate::agents::worker_effort(sim, ag, crate::colony::Job::Logger);
    match task {
        ForestTask::Plant { cell, progress } => {
            let progress = progress + done;
            let stock = sim.buildings.buildings.iter().any(|b| b.kind == BuildingKind::TreeNursery && b.operational() && b.output > 0.25 && crate::stock::dist(cell, b) <= 50);
            let need = if stock { NURSERY_PLANT_WORK } else { PLANT_WORK };
            if progress + 1e-4 < need {
                ag.task = Task::Forest(ForestTask::Plant { cell, progress });
                return;
            }
            let age = if stock {
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
