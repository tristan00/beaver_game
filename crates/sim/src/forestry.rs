include!(concat!(env!("OUT_DIR"), "/forestry_constants.rs"));
use std::collections::{BTreeMap, BTreeSet, HashSet, VecDeque};

use rand_core::{Rng, SeedableRng};
use rand_pcg::Pcg32;

use crate::buildings::{BuildingId, BuildingKind};
use crate::calendar::Season;
use crate::colony::BeaverId;
use crate::grid::Grid;
use crate::resources::{Resource, ResourceId};
use crate::trees::Tree;
use crate::water::Cell;
use crate::work::Area;
use crate::SimState;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
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

    /// Logs, branches and bark (food) from felling a tree of this stage and species.
    pub fn fell_yield(self, species: Tree) -> Option<(f32, f32, f32)> {
        let (logs, branches, bark) = match self {
            TreeStage::Sapling => return None,
            TreeStage::Young => (YOUNG_LOGS, YOUNG_BRANCHES, YOUNG_BARK),
            TreeStage::Mature => (crate::resources::TREE_LOGS, crate::resources::TREE_BRANCHES, crate::resources::TREE_BARK),
            TreeStage::Old => (OLD_LOGS, OLD_BRANCHES, OLD_BARK),
            TreeStage::Snag => (0.0, SNAG_BRANCHES, 0.0),
        };
        let sp = species.spec();
        Some((logs * sp.logs, branches * sp.branches, bark * sp.bark))
    }

    pub fn is_grown(self) -> bool {
        matches!(self, TreeStage::Young | TreeStage::Mature | TreeStage::Old)
    }
}

impl Resource {
    pub fn stage(&self) -> Option<TreeStage> {
        match self.kind {
            k if k.is_tree() && self.snag => Some(TreeStage::Snag),
            k if k.is_tree() => Some(TreeStage::of_age(self.age_days)),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum CutRule {
    #[default]
    MatureOnly,
    Thin,
    Coppice,
}

#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Deed {
    pub day: f64,
    pub cell: Cell,
    pub kind: DeedKind,
    pub by: Option<BuildingId>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum DeedKind {
    Felled,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Forestry {
    pub cut_rules: BTreeMap<BuildingId, CutRule>,
    pub deeds: VecDeque<Deed>,
    pub history: VecDeque<(f64, usize)>,
    size: usize,
    had_tree: Vec<bool>,
    /// Remade whole each day and only read in between, so a snapshot shares them.
    growth: std::sync::Arc<Vec<f32>>,
    flood_plan: std::sync::Arc<BTreeSet<Cell>>,
    stumps: Vec<(Cell, Tree)>,
    thin_age: BTreeMap<BuildingId, f32>,
    unreachable: BTreeMap<Cell, f64>,
    day: Option<i64>,
    rng: Pcg32,
    /// Trees a beaver planted: who, and on what day.
    pub planted: BTreeMap<ResourceId, (BeaverId, f64)>,
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
            growth: Default::default(),
            flood_plan: Default::default(),
            stumps: Vec::new(),
            thin_age: BTreeMap::new(),
            unreachable: BTreeMap::new(),
            day: None,
            rng: Pcg32::seed_from_u64(seed ^ 0xF0E5_7E1D),
            planted: BTreeMap::new(),
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

/// Days in floodwater deeper than the species stands before a tree of this stage drowns.
pub fn drown_days(stage: TreeStage, species: Tree) -> f32 {
    let base = match stage {
        TreeStage::Sapling => SAPLING_FLOOD_DAYS,
        TreeStage::Young => crate::tuning::ecology::DROWN_FLOOD_DAYS,
        TreeStage::Mature | TreeStage::Old | TreeStage::Snag => MATURE_DROWN_DAYS,
    };
    base * species.spec().flood
}

/// How far a standing tree is towards drowning, 0 to 1, while the water over it is too deep.
pub fn drowning(sim: &SimState, r: &Resource) -> Option<f32> {
    let sp = r.species?;
    let stage = r.stage().filter(|s| *s != TreeStage::Snag)?;
    let dp = sim.water.depth.get(r.cell.0, r.cell.1).copied().unwrap_or(0.0);
    if dp <= sp.spec().max_depth {
        return None;
    }
    let fd = sim.ecology.flood_days.get(r.cell.0, r.cell.1).copied().unwrap_or(0.0);
    Some((fd / drown_days(stage, sp)).clamp(0.0, 1.0))
}

/// The ground's growth multiplier at a cell: faster near water, slower on dry ground.
pub fn ground_growth(sim: &SimState, cell: Cell) -> f32 {
    let n = sim.forestry.size;
    if cell.0 < n && cell.1 < n { sim.forestry.growth.get(cell.1 * n + cell.0).copied().unwrap_or(1.0) } else { 1.0 }
}

/// Days of growing season until a tree reaches its next stage, at today's growth rate.
pub fn days_to_next_stage(sim: &SimState, r: &Resource) -> Option<(TreeStage, f32)> {
    let sp = r.species?;
    let (next, at) = match r.stage()? {
        TreeStage::Sapling => (TreeStage::Young, YOUNG_AGE),
        TreeStage::Young => (TreeStage::Mature, MATURE_AGE),
        TreeStage::Mature => (TreeStage::Old, OLD_AGE),
        TreeStage::Old | TreeStage::Snag => return None,
    };
    let rate = ground_growth(sim, r.cell) * sp.spec().growth;
    Some((next, (at - r.age_days).max(0.0) / rate.max(1e-3)))
}

/// Trees the world generator placed where the river already runs too deep for them.
pub fn clear_drowned(sim: &mut SimState) {
    let depth = &sim.water.depth;
    sim.resources.keep(|r| {
        !r.kind.is_tree() || r.species.is_none_or(|sp| depth.get(r.cell.0, r.cell.1).is_none_or(|d| *d <= sp.spec().max_depth))
    });
}

pub fn fellable(r: &Resource) -> bool {
    r.kind.is_tree() && r.stage() != Some(TreeStage::Sapling)
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
    for r in sim.resources.items().iter().filter(|r| r.kind.is_tree()) {
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
    let _p = crate::prof::scope("step/forestry/daily");
    { let _p = crate::prof::scope("step/forestry/daily/moist ground"); refresh_growth(sim); }
    { let _p = crate::prof::scope("step/forestry/daily/flood plan"); refresh_flood_plan(sim); }
    let _trees = crate::prof::scope("step/forestry/daily/trees");
    let winter = Season::at(day as f64 - 0.5) == Season::Winter;
    let n = sim.world.size;
    let occupied: HashSet<Cell> = sim.resources.taken_cells().collect();
    let built = building_cells(sim);
    let mut occupied = occupied;
    let mut gone: HashSet<ResourceId> = HashSet::new();
    let mut seeds: Vec<(Cell, Tree)> = Vec::new();
    let flood_days = &sim.ecology.flood_days;
    let depth = &sim.water.depth;
    {
        let f = &mut sim.forestry;
        sim.resources.each_mut(|r| {
            if !r.kind.is_tree() {
                return;
            }
            let (x, y) = r.cell;
            let sp = r.species.unwrap_or_else(Tree::unknown);
            let fd = flood_days.get(x, y).copied().unwrap_or(0.0);
            let dp = depth.get(x, y).copied().unwrap_or(0.0);
            if r.snag {
                // A dead tree standing in water soon falls; on dry ground it stands for years.
                r.age_days += 1.0;
                let rot = if dp > crate::tuning::ecology::FLOOD_DEPTH { SNAG_WATER_ROT_DAYS } else { SNAG_ROT_DAYS };
                if r.age_days >= rot {
                    gone.insert(r.id);
                }
                return;
            }
            let stage = TreeStage::of_age(r.age_days);
            if dp > sp.spec().max_depth && fd >= drown_days(stage, sp) {
                if matches!(stage, TreeStage::Sapling | TreeStage::Young) {
                    gone.insert(r.id);
                    crate::resources::note("DROWNED", r, format_args!("depth={dp:.2} flood_days={fd:.1}"));
                } else {
                    crate::resources::note("DIED_STANDING", r, format_args!("why=drowned depth={dp:.2} flood_days={fd:.1}"));
                    r.snag = true;
                    r.age_days = 0.0;
                }
                return;
            }
            if !winter {
                let ground = if x < n && y < n { f.growth[y * n + x] } else { 1.0 };
                r.age_days += ground * sp.spec().growth;
            }
            let stage = TreeStage::of_age(r.age_days);
            if stage == TreeStage::Old && chance(&mut f.rng, OLD_DEATH_CHANCE) {
                crate::resources::note("DIED_STANDING", r, format_args!("why=old age"));
                r.snag = true;
                r.age_days = 0.0;
                return;
            }
            if !winter && matches!(stage, TreeStage::Mature | TreeStage::Old) && chance(&mut f.rng, SPREAD_CHANCE) {
                let k = (f.rng.next_u32() % 8) as usize;
                let (dx, dy) = [(-1, -1), (0, -1), (1, -1), (-1, 0), (1, 0), (-1, 1), (0, 1), (1, 1)][k];
                let (nx, ny) = (x as i64 + dx, y as i64 + dy);
                if nx >= 0 && ny >= 0 && (nx as usize) < n && (ny as usize) < n {
                    seeds.push(((nx as usize, ny as usize), sp));
                }
            }
        });
    }
    sim.resources.keep(|r| !gone.contains(&r.id));
    for (c, sp) in seeds {
        let free = !occupied.contains(&c) && !built.contains(&c) && sim.work.supplies != Some(c);
        let flooded = sim.ecology.flood_days.get(c.0, c.1).is_some_and(|d| *d > 0.0);
        if free && !flooded && can_hold_tree(sim, c) && is_dry(sim, c) && !sim.forestry.flood_plan.contains(&c) {
            sim.resources.add_tree(c, 0.0, sp);
            occupied.insert(c);
        }
    }
    regrow_stumps(sim, &built);
    let f = &mut sim.forestry;
    for r in sim.resources.items().iter().filter(|r| r.kind.is_tree()) {
        if r.cell.0 < n && r.cell.1 < n {
            f.had_tree[r.cell.1 * n + r.cell.0] = true;
        }
    }
    let today = day as f64;
    while f.deeds.front().is_some_and(|d| d.day < today - STATS_DAYS) {
        f.deeds.pop_front();
    }
    f.unreachable.retain(|_, until| *until > today);
    // Trees felled or gone are forgotten.
    let planted = std::mem::take(&mut sim.forestry.planted);
    sim.forestry.planted = planted.into_iter().filter(|(id, _)| sim.resources.get(*id).is_some_and(|r| r.kind.is_tree())).collect();
    drop(_trees);
    { let _p = crate::prof::scope("step/forestry/daily/thinning"); refresh_thinning(sim); }
    { let _p = crate::prof::scope("step/forestry/daily/forest cover"); derive_ecology(sim); }
    record_history(sim, today);
}

fn regrow_stumps(sim: &mut SimState, built: &HashSet<Cell>) {
    let stumps = std::mem::take(&mut sim.forestry.stumps);
    for (c, sp) in stumps {
        if built.contains(&c) {
            continue;
        }
        if sim.resources.at(c).next().is_some() {
            sim.forestry.stumps.push((c, sp));
        } else {
            sim.resources.add_tree(c, STUMP_AGE, sp);
        }
    }
}

fn refresh_growth(sim: &mut SimState) {
    let n = sim.world.size;
    let (depth, river, lake) = (sim.water.depth.as_slice(), sim.world.river.as_slice(), sim.world.lake.as_slice());
    let wet = |i: usize| depth.get(i).is_some_and(|d| *d > crate::tuning::ecology::FLOOD_DEPTH)
        || river.get(i).copied().unwrap_or(false)
        || lake.get(i).copied().unwrap_or(false);
    let r = MOIST_RANGE;
    // Water within `r` cells along the row, from running counts of wet cells...
    let mut rows = vec![false; n * n];
    let mut count = vec![0u32; n + 1];
    for y in 0..n {
        for x in 0..n {
            count[x + 1] = count[x] + wet(y * n + x) as u32;
        }
        for x in 0..n {
            rows[y * n + x] = count[(x + r).min(n - 1) + 1] > count[x.saturating_sub(r)];
        }
    }
    // ...then within `r` rows up or down, from how many of those rows have it at this column.
    let mut growth = vec![1.0; n * n];
    let mut near = vec![0u32; n];
    for y in 0..r.min(n) {
        for x in 0..n {
            near[x] += rows[y * n + x] as u32;
        }
    }
    for y in 0..n {
        if y + r < n {
            for x in 0..n {
                near[x] += rows[(y + r) * n + x] as u32;
            }
        }
        if y > r {
            for x in 0..n {
                near[x] -= rows[(y - r - 1) * n + x] as u32;
            }
        }
        for x in 0..n {
            let moist = near[x] > 0;
            let dry = sim.world.moisture.get(x, y).is_some_and(|m| *m < DRY_MOISTURE);
            growth[y * n + x] = if moist { MOIST_GROWTH } else if dry { DRY_GROWTH } else { 1.0 };
        }
    }
    sim.forestry.growth = std::sync::Arc::new(growth);
}

/// The cells the dams being built will flood once finished: nothing is sown there. It only
/// changes when a dam, its plan or the ground does, so yesterday's is kept until then.
fn refresh_flood_plan(sim: &mut SimState) {
    type Kept = (Vec<crate::dam_preview::GroundKey>, std::sync::Arc<BTreeSet<Cell>>);
    thread_local! {
        static LAST: std::cell::RefCell<Option<Kept>> = const { std::cell::RefCell::new(None) };
    }
    let (mut keys, mut ponds) = (Vec::new(), Vec::new());
    for b in sim.buildings.buildings.iter().filter(|b| b.dam.is_some() && crate::dams::has_work(sim, b.id)) {
        let d = b.dam.as_deref().expect("filtered dams");
        let (cells, heights): (Vec<_>, Vec<_>) = d.cells.iter().map(|c| (c.cell, c.target)).filter(|c| c.1 > 0.0).unzip();
        // Only the still pond's cells are wanted: none of the estimate's flows and losses.
        if let Some((key, ground)) = crate::dam_preview::flooded_kept(sim, &cells, &heights) {
            keys.push(key);
            ponds.push(ground);
        }
    }
    let kept = LAST.with(|last| last.borrow().as_ref().filter(|(k, _)| *k == keys).map(|(_, plan)| plan.clone()));
    sim.forestry.flood_plan = kept.unwrap_or_else(|| {
        // Sorted first, the set is built in one sweep instead of cell by cell.
        let mut cells: Vec<Cell> = ponds.iter().flat_map(|g| g.flooded.iter().copied()).collect();
        cells.sort_unstable();
        let plan = std::sync::Arc::new(cells.into_iter().collect::<BTreeSet<Cell>>());
        LAST.with(|last| *last.borrow_mut() = Some((keys, plan.clone())));
        plan
    });
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
            .items()
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
    for r in sim.resources.items().iter().filter(|r| r.stage().is_some_and(TreeStage::is_grown)) {
        if r.cell.0 < n && r.cell.1 < n {
            cover[r.cell.1 * n + r.cell.0] = 1.0;
        }
    }
    // The share of grown trees in each cell's 3x3: row sums first, then the rows above and
    // below (whole numbers, so the order they are added in changes nothing).
    let mut across = vec![0.0f32; n * n];
    for y in 0..n {
        for x in 0..n {
            let mut sum = cover[y * n + x];
            if x > 0 { sum += cover[y * n + x - 1]; }
            if x + 1 < n { sum += cover[y * n + x + 1]; }
            across[y * n + x] = sum;
        }
    }
    let mut g = Grid::new(n, n, 0.0f32);
    let share = g.as_mut_slice();
    for y in 0..n {
        let tall = 1 + (y > 0) as u32 + (y + 1 < n) as u32;
        for x in 0..n {
            let wide = 1 + (x > 0) as u32 + (x + 1 < n) as u32;
            let mut sum = across[y * n + x];
            if y > 0 { sum += across[(y - 1) * n + x]; }
            if y + 1 < n { sum += across[(y + 1) * n + x]; }
            share[y * n + x] = sum / (tall * wide) as f32;
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
            .items()
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

pub(crate) fn felled(sim: &mut SimState, who: BeaverId, cell: Cell, species: Tree) {
    let camp = sim.work.slot_of(who).map(|s| s.0);
    record(sim, cell, DeedKind::Felled, camp);
    if let Some(b) = camp {
        if sim.forestry.cut_rule(b) == CutRule::Coppice && can_coppice(sim) {
            sim.forestry.stumps.push((cell, species));
        }
    }
}

fn planting_blocked(sim: &SimState) -> (HashSet<Cell>, HashSet<Cell>) {
    let occupied = sim.resources.taken_cells().collect();
    let mut near = HashSet::new();
    for b in &sim.buildings.buildings {
        for c in b.cells() {
            for y in c.1.saturating_sub(2)..=c.1 + 2 {
                for x in c.0.saturating_sub(2)..=c.0 + 2 {
                    near.insert((x, y));
                }
            }
        }
    }
    (occupied, near)
}

fn plantable_with(sim: &SimState, c: Cell, occupied: &HashSet<Cell>, near: &HashSet<Cell>) -> bool {
    !occupied.contains(&c) && !near.contains(&c) && plantable_ground(sim, c)
}

/// The ground itself takes a sapling, whatever stands on or round it.
fn plantable_ground(sim: &SimState, c: Cell) -> bool {
    let n = sim.world.size;
    // Planting on every other row and column keeps lanes open through the grown wood.
    c.0 < n && c.1 < n && c.0 % 2 == 0 && c.1 % 2 == 0
        && is_dry(sim, c)
        && sim.work.supplies != Some(c)
        && !sim.forestry.flood_plan.contains(&c)
        && sim.ecology.flood_days.get(c.0, c.1).is_none_or(|d| *d <= 0.0)
        && sim.nav.surface_at(c) == Some(crate::nav::Surface::Land)
        && sim.forestry.unreachable.get(&c).is_none_or(|until| sim.time_days >= *until)
}

/// A sapling can still go here.
pub fn plantable(sim: &SimState, c: Cell) -> bool {
    // One cell: what stands on it and the buildings round it, not the whole map's sets.
    let near_building = sim.buildings.buildings.iter().any(|b| b.cells().any(|k| k.0.abs_diff(c.0) <= 2 && k.1.abs_diff(c.1) <= 2));
    !near_building && sim.resources.at(c).next().is_none() && plantable_ground(sim, c)
}

/// Where a forester puts the next sapling: anywhere in its area that grows well, picked at
/// random so the planting spreads across the area instead of filling in beside the building.
/// The ground's growth counts in broad bands only, so a slightly richer patch doesn't draw
/// every sapling to it.
pub fn plant_cell(sim: &SimState, area: Area, from: Cell, who: BeaverId) -> Option<Cell> {
    let claimed: HashSet<Cell> = sim
        .agents
        .by_id
        .values()
        .filter(|a| a.id != who)
        .filter_map(|a| match a.task { crate::agents::Task::Plant { cell, .. } => Some(cell), _ => None })
        .collect();
    let (occupied, near) = planting_blocked(sim);
    let n = sim.world.size;
    // A fresh draw every few hours and for each planter, the same within one.
    let draw = (sim.time_days * 6.0).floor() as u64 ^ who.0.wrapping_mul(0x9E37_79B9_7F4A_7C15);
    let lot = |c: Cell| {
        let mut z = draw ^ ((c.0 as u64) << 32 | c.1 as u64).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        (z ^ (z >> 31)) as usize
    };
    let mut best: Option<((i32, usize, Cell), Cell)> = None;
    for y in area.min.1..=area.max.1.min(n.saturating_sub(1)) {
        for x in area.min.0..=area.max.0.min(n.saturating_sub(1)) {
            let c = (x, y);
            if claimed.contains(&c) || !area.reaches(c) || !crate::agents::within_shore(sim, crate::jobs::Job::with_duty(crate::content::Duty::Forest), c) {
                continue;
            }
            let key = (-((ground_growth(sim, c) * 3.0).round() as i32), lot(c), c);
            if best.as_ref().is_some_and(|(k, _)| key >= *k) {
                continue;
            }
            if !plantable_with(sim, c, &occupied, &near) || !sim.nav.may_reach_beside(from, c) {
                continue;
            }
            best = Some((key, c));
        }
    }
    best.map(|b| b.1)
}

/// Try this cell again later; nobody could walk to it.
pub(crate) fn mark_unreachable(sim: &mut SimState, c: Cell) {
    let until = sim.time_days + crate::agents::UNREACHABLE_RETRY_DAYS;
    sim.forestry.unreachable.insert(c, until);
}

/// The species of the seedlings a forager finds by a cell: those of the nearest seeding
/// tree within `SEEDLING_RADIUS`, food trees first. None with no such tree about.
pub fn seedling_species(sim: &SimState, c: Cell) -> Option<Tree> {
    sim.resources
        .items()
        .iter()
        .filter(|r| r.kind.is_tree() && matches!(r.stage(), Some(TreeStage::Mature | TreeStage::Old)))
        .filter(|r| r.cell.0.abs_diff(c.0).max(r.cell.1.abs_diff(c.1)) <= SEEDLING_RADIUS)
        .filter_map(|r| Some((!r.species?.feeds(), r.cell.0.abs_diff(c.0).pow(2) + r.cell.1.abs_diff(c.1).pow(2), r.id, r.species?)))
        .min()
        .map(|k| k.3)
}

/// A sapling of no known species takes the species of the nearest food tree; with none about, the
/// content's `sapling`.
pub fn planting_species(sim: &SimState, c: Cell) -> Tree {
    sim.resources
        .items()
        .iter()
        .filter(|r| r.kind.is_tree() && r.species.is_some_and(|s| s.feeds()))
        .min_by_key(|r| (r.cell.0.abs_diff(c.0).pow(2) + r.cell.1.abs_diff(c.1).pow(2), r.id))
        .and_then(|r| r.species)
        .unwrap_or(crate::content::content().sapling)
}
