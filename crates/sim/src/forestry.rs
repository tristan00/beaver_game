include!(concat!(env!("OUT_DIR"), "/forestry_constants.rs"));
use std::collections::{BTreeMap, BTreeSet, HashSet, VecDeque};

use rand_core::{Rng, SeedableRng};
use rand_pcg::Pcg32;

use crate::buildings::{BuildingId, BuildingKind};
use crate::calendar::Season;
use crate::colony::BeaverId;
use crate::grid::Grid;
use crate::resources::{Resource, ResourceId, ResourceKind};
use crate::water::Cell;
use crate::work::Area;
use crate::SimState;


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
            TreeStage::Young => Some((YOUNG_LOGS, YOUNG_BRANCHES, YOUNG_BARK)),
            TreeStage::Mature => Some((crate::resources::TREE_LOGS, crate::resources::TREE_BRANCHES, crate::resources::TREE_BARK)),
            TreeStage::Old => Some((OLD_LOGS, OLD_BRANCHES, OLD_BARK)),
            TreeStage::Snag => Some((0.0, SNAG_BRANCHES, 0.0)),
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
    Thin,
    Coppice,
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
    Felled,
}

#[derive(Clone, Debug)]
pub struct Forestry {
    pub cut_rules: BTreeMap<BuildingId, CutRule>,
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
    if u < START_YOUNG_SHARE {
        YOUNG_AGE + v * (MATURE_AGE - YOUNG_AGE)
    } else if u < START_OLD_CUMULATIVE_SHARE {
        OLD_AGE + v * START_OLD_SPREAD_YEARS * crate::calendar::YEAR_DAYS as f32
    } else {
        MATURE_AGE + v * (START_MATURE_MAX_YEARS * crate::calendar::YEAR_DAYS as f32 - MATURE_AGE)
    }
}

pub fn fellable(r: &Resource) -> bool {
    r.kind == ResourceKind::Tree && r.stage() != Some(TreeStage::Sapling)
}

pub fn cuttable(r: &Resource, min_age: f32) -> bool {
    fellable(r) && (r.snag || r.age_days >= min_age)
}

pub fn cut_min_age(sim: &SimState, camp: Option<BuildingId>) -> f32 {
    let _ = (sim, camp);
    MATURE_AGE
}

fn invalid(m: impl Into<String>) -> crate::SimError {
    crate::SimError::InvalidCommand(m.into())
}

pub fn can_coppice(sim: &SimState) -> bool {
    sim.sandbox
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
    let _ = kind;
    None
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
    for b in sim.buildings.buildings.iter().filter(|b| b.dam.is_some() && crate::dams::has_work(sim, b.id)) {
        let d = b.dam.as_deref().expect("filtered dams");
        let (cells, heights): (Vec<_>, Vec<_>) = d.cells.iter().map(|c| (c.cell, c.target)).filter(|c| c.1 > 0.0).unzip();
        if let Some(pv) = crate::dam_preview::preview_heights(sim, &cells, &heights) {
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
