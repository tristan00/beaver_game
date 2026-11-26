include!(concat!(env!("OUT_DIR"), "/starts_constants.rs"));
use crate::SimState;

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum StartError {
    NoSite,
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

// Spring arrival: these are the previous spring's offspring. They become
// yearlings around the arrival of this year's litter, then adults next spring.

pub(crate) fn dist2(a: Cell, b: Cell) -> usize {
    a.0.abs_diff(b.0).pow(2) + a.1.abs_diff(b.1).pow(2)
}

struct RiverPlan {
    camp: Cell,
    dam: Vec<Cell>,
    height: f32,
}

fn dam_candidates(sim: &SimState, around: Option<(Cell, usize)>, limit: usize) -> Vec<(f32, Vec<Cell>, f32)> {
    let size = sim.world.size;
    let is_river = |x: usize, y: usize| sim.world.river.get(x, y).copied().unwrap_or(false);
    let up = upstream_map(sim);
    let bed = |c: Cell| sim.height.get(c.0, c.1).copied().unwrap_or(0.0);
    let mut out = Vec::new();
    'search: for y in EDGE_MARGIN..size - EDGE_MARGIN {
        for x in EDGE_MARGIN..size - EDGE_MARGIN {
            if !is_river(x, y) || (x + y) % CANDIDATE_STRIDE != 0
                || around.is_some_and(|(camp, radius)| dist2((x,y),camp)>radius*radius) {
                continue;
            }
            if reach_drop(sim, &up, (x, y), bed) < MIN_DROP {
                continue;
            }
            for (dx, dy) in [(1isize, 0isize), (0, 1), (1, 1), (1, -1)] {
                let at = |t: isize| ((x as isize + dx * t) as usize, (y as isize + dy * t) as usize);
                let reach = |dir: isize| (1..12).find(|&k| !is_river(at(dir * k).0, at(dir * k).1)).unwrap_or(12);
                let (left, right) = (-reach(-1), reach(1));
                let low = (left + 1..right).map(|t| bed(at(t))).fold(f32::INFINITY, f32::min);
                for height in DAM_HEIGHTS {
                    let head = crate::water::spill_head(SPILL_FLOW_MARGIN * sim.world.river_flow.get(x, y).copied().unwrap_or(0.0), sim.water.cell_m);
                    let ties_in = |t: isize| bed(at(t)) >= low + height + head;
                    let (Some(l), Some(r)) = ((0..MAX_DAM_CELLS as isize).map(|k| left - k).find(|&t| ties_in(t - 1)), (0..MAX_DAM_CELLS as isize).map(|k| right + k).find(|&t| ties_in(t + 1))) else { continue };
                    let cells: Vec<Cell> = (l..=r).map(at).collect();
                    if cells.len() > MAX_DAM_CELLS {
                        continue;
                    }
                    let Some(p) = crate::dam_preview::preview(sim, &cells, height) else { continue };
                    if p.spill_at.is_some() {
                        continue;
                    }
                    let area = p.flooded.len();
                    if p.rise >= START_RISE && POND_AREA_RANGE.contains(&area) && holds(sim, &cells, &p.flooded) {
                        let score = area.abs_diff(POND_AREA) as f32 + cells.len() as f32 * 20.0;
                        out.push((score, cells.clone(), height));
                        if out.len() >= limit { break 'search; }
                    }
                }
            }
        }
    }
    out.sort_by(|a, b| a.0.total_cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
    out
}

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

fn dam_river_cell(sim: &SimState, dam: &[Cell]) -> Option<Cell> {
    let bed = |c: &Cell| sim.height.get(c.0, c.1).copied().unwrap_or(f32::INFINITY);
    dam.iter().copied().filter(|&(x, y)| sim.world.river.get(x, y) == Some(&true)).min_by(|a, b| bed(a).total_cmp(&bed(b)))
}

#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct DamEffect {
    pub rise: f32,
    pub flooded: usize,
    pub newly_flooded: usize,
    pub drop: f32,
    pub fills: bool,
}

impl DamEffect {
    fn closeness(&self) -> f32 {
        (self.rise / START_RISE).min(self.newly_flooded as f32 / START_NEW.max(self.flooded / 4) as f32)
    }

    pub fn is_visible(&self) -> bool {
        self.rise >= START_RISE && self.newly_flooded >= START_NEW.max(self.flooded / 4) && self.drop >= MIN_DROP
    }
}

pub fn dam_effect(sim: &SimState, dam: &[Cell], height: f32) -> Option<DamEffect> {
    let p = crate::dam_preview::preview(sim, dam, height)?;
    let wet = |&&(x, y): &&Cell| {
        sim.water.depth.get(x, y).is_some_and(|d| *d >= crate::buildings::WET_DEPTH)
            || sim.world.river.get(x, y).copied().unwrap_or(false)
            || sim.world.lake.get(x, y).copied().unwrap_or(false)
    };
    let surface = |(x, y): Cell| sim.height.get(x, y).copied().unwrap_or(0.0) + sim.water.depth.get(x, y).copied().unwrap_or(0.0);
    let drop = reach_drop(sim, &upstream_map(sim), dam_river_cell(sim, dam)?, surface);
    Some(DamEffect { rise: p.rise, flooded: p.flooded.len(), newly_flooded: p.flooded.iter().filter(|c| !wet(c)).count(), drop, fills: p.fills(START_INFLOW_MARGIN) })
}

fn settle_water(sim: &mut SimState) {
    sim.water.refresh(&sim.height);
    sim.water.time_days = 0.0;
    sim.water.reset_books();
}

/// Where the river network is richest: dry banks with the most separate river branches
/// within `RIVERS_RADIUS`, such as a junction or two streams running side by side. A branch
/// counts once however often it meanders in and out; the reach below a junction adds nothing.
/// Best first, at least a radius apart, with the river cells in reach breaking ties.
fn river_spots(sim: &SimState) -> Vec<(usize, Cell)> {
    const STRIDE: usize = 4;
    const BANK: usize = 10;
    let (n, r) = (sim.world.size, RIVERS_RADIUS);
    let is_river = |(x, y): Cell| sim.world.river.get(x, y).copied().unwrap_or(false);
    let river: Vec<Cell> = (0..n * n).map(|i| (i % n, i / n)).filter(|&c| is_river(c)).collect();
    let mut fed: std::collections::HashMap<Cell, Vec<Cell>> = std::collections::HashMap::new();
    for &c in &river {
        if let Some(d) = sim.world.downstream(c.0, c.1) {
            fed.entry(d).or_default().push(c);
        }
    }
    let dry = |(x, y): Cell| !is_river((x, y)) && !sim.world.lake.get(x, y).copied().unwrap_or(false)
        && sim.water.depth.get(x, y).is_none_or(|d| *d <= crate::buildings::WET_DEPTH);
    let mut scored = Vec::new();
    for y in (r..n.saturating_sub(r)).step_by(STRIDE) {
        for x in (r..n.saturating_sub(r)).step_by(STRIDE) {
            if !dry((x, y)) || !river.iter().any(|&c| dist2(c, (x, y)) <= BANK * BANK) {
                continue;
            }
            let near: std::collections::HashSet<Cell> = river.iter().copied().filter(|&c| dist2(c, (x, y)) <= r * r).collect();
            let entries: Vec<Cell> = near.iter().copied()
                .filter(|c| !fed.get(c).is_some_and(|ups| ups.iter().any(|u| near.contains(u))))
                .collect();
            let mut below = std::collections::HashSet::new();
            for &e in &entries {
                let mut at = sim.world.downstream(e.0, e.1);
                for _ in 0..6 * r {
                    let Some(c) = at else { break };
                    below.insert(c);
                    at = sim.world.downstream(c.0, c.1);
                }
            }
            let branches = entries.iter().filter(|e| !below.contains(e)).count();
            scored.push(((branches, near.len()), (x, y)));
        }
    }
    scored.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.1.cmp(&b.1.1)).then(a.1.0.cmp(&b.1.0)));
    let mut spots: Vec<(usize, Cell)> = Vec::new();
    for ((branches, _), c) in scored {
        if spots.iter().all(|&(_, s)| dist2(s, c) > r * r) {
            spots.push((branches, c));
        }
    }
    spots
}

/// A dry camp with a river bank and trees in reach, outside `pond`.
fn camp_fits(sim: &SimState, c: Cell, pond: &std::collections::HashSet<Cell>, trees: &[Cell]) -> bool {
    let size = sim.world.size;
    let wet = |(x, y): Cell| {
        sim.water.depth.get(x, y).is_some_and(|d| *d > crate::buildings::WET_DEPTH)
            || sim.world.river.get(x, y).copied().unwrap_or(false)
            || sim.world.lake.get(x, y).copied().unwrap_or(false)
            || pond.contains(&(x, y))
    };
    let near = |r: usize| (c.1.saturating_sub(r)..=(c.1 + r).min(size - 1)).flat_map(move |y| (c.0.saturating_sub(r)..=(c.0 + r).min(size - 1)).map(move |x| (x, y)));
    c.0 >= 3 && c.1 >= 3 && c.0 + 3 < size && c.1 + 3 < size
        && near(2).all(|n| !wet(n))
        && near(BANK_MAX).any(|(x, y)| sim.world.river.get(x, y).copied().unwrap_or(false))
        && trees.iter().filter(|&&t| dist2(c, t) <= TREE_RADIUS * TREE_RADIUS && dist2(c, t) > 9).take(TREES_WANTED).count() >= TREES_WANTED
}

fn trees(sim: &SimState) -> Vec<Cell> {
    sim.resources.items().iter().filter(|r| r.kind.is_tree()).map(|r| r.cell).collect()
}

fn cells_by_distance(sim: &SimState, from: Cell, r: usize) -> Vec<Cell> {
    let size = sim.world.size;
    let mut cells: Vec<Cell> = (from.1.saturating_sub(r)..=(from.1 + r).min(size - 1))
        .flat_map(|y| (from.0.saturating_sub(r)..=(from.0 + r).min(size - 1)).map(move |x| (x, y)))
        .filter(|&c| dist2(c, from) <= r * r)
        .collect();
    cells.sort_by_key(|&c| (dist2(c, from), c.1, c.0));
    cells
}

fn camp_near(sim: &SimState, dam: &[Cell], height: f32) -> Option<Cell> {
    let pond = crate::dam_preview::preview(sim, dam, height)?.flooded.into_iter().collect();
    let mid = dam[dam.len() / 2];
    let trees = trees(sim);
    cells_by_distance(sim, mid, 60).into_iter()
        .filter(|&c| CAMP_DAM_DIST.contains(&(dist2(c, mid).isqrt())))
        .find(|&c| camp_fits(sim, c, &pond, &trees))
}

/// Start where the rivers are richest: a camp beside the best river spot, and the first dam
/// suggested as near that camp as one can be found. Only a map with no rich spot that has room
/// for a camp falls back to the best dam anywhere and a camp beside it.
fn river_plan(sim: &mut SimState) -> Option<RiverPlan> {
    settle_water(sim);
    let reach = RIVERS_RADIUS;
    let trees = trees(sim);
    let dry = std::collections::HashSet::new();
    for (_, spot) in river_spots(sim).into_iter().take(SPOTS_TRIED) {
        let Some(camp) = cells_by_distance(sim, spot, reach).into_iter().find(|&c| camp_fits(sim, c, &dry, &trees)) else { continue };
        for radius in [2 * reach, 4 * reach, 8 * reach] {
            if let Some((dam, height)) = best_dam(sim, dam_candidates(sim, Some((camp, radius)), 32), camp) {
                return Some(RiverPlan { camp, dam, height });
            }
        }
    }
    let mut best: Option<(DamKey, RiverPlan)> = None;
    for (_, dam, height) in dam_candidates(sim, None, 32) {
        let Some(key) = dam_key(sim, &dam, height) else { continue };
        if best.as_ref().is_some_and(|(b, _)| *b >= key) {
            continue;
        }
        let Some(camp) = camp_near(sim, &dam, height) else { continue };
        best = Some((key, RiverPlan { camp, dam, height }));
    }
    best.map(|(_, plan)| plan)
}

const SPOTS_TRIED: usize = 6;

/// How good a first dam is: fills and shows, keeps a fall below it, then how close it comes.
type DamKey = (u8, bool, f32);

fn dam_key(sim: &SimState, dam: &[Cell], height: f32) -> Option<DamKey> {
    let effect = dam_effect(sim, dam, height)?;
    crate::dams::check_line(sim, dam).ok()?;
    Some((effect.fills as u8 + effect.is_visible() as u8, effect.drop >= MIN_DROP, effect.closeness()))
}

/// The nearest dam that fills and shows a pond and leaves the camp dry; failing that, the
/// best of the rest.
fn best_dam(sim: &SimState, candidates: Vec<(f32, Vec<Cell>, f32)>, camp: Cell) -> Option<(Vec<Cell>, f32)> {
    let trees = trees(sim);
    let mut fit: Vec<(DamKey, usize, Vec<Cell>, f32)> = candidates.into_iter()
        .filter_map(|(_, dam, height)| {
            let away = dist2(dam[dam.len() / 2], camp).isqrt();
            if away < *CAMP_DAM_DIST.start() {
                return None;
            }
            let key = dam_key(sim, &dam, height)?;
            let pond = crate::dam_preview::preview(sim, &dam, height)?.flooded.into_iter().collect();
            camp_fits(sim, camp, &pond, &trees).then_some((key, away, dam, height))
        })
        .collect();
    fit.sort_by(|a, b| (b.0.0, b.0.1).cmp(&(a.0.0, a.0.1)).then(a.1.cmp(&b.1)).then(b.0.2.total_cmp(&a.0.2)));
    fit.into_iter().next().map(|(_, _, dam, height)| (dam, height))
}

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
    // Find enough nearby landing cells for the family, not a path from every
    // river cell on the map before the player can start.
    let linked: Vec<Cell> = river.iter().copied()
        .filter(|&c| dist2(c, landing) <= SWIM_REACH * SWIM_REACH)
        .filter(|&c| nav.find_path(who, c, Goal::Beside(landing)).is_some())
        .take(n).collect();
    let pool = if linked.len() >= n { linked } else { river };
    let Some(&anchor) = pool.first() else { return Vec::new() };
    let mut pack = pool;
    pack.sort_by_key(|&c| (dist2(c, anchor), c.1, c.0));
    pack.truncate(n);
    pack
}

fn supplies_bank(sim: &SimState, camp: Cell) -> Option<Cell> {
    let occupied: std::collections::HashSet<Cell> = sim.resources.items().iter()
        .filter(|r| r.is_solid()).map(|r| r.cell)
        .chain(sim.buildings.buildings.iter().flat_map(|b| b.cells()))
        .collect();
    (0..sim.world.size * sim.world.size)
        .map(|i| (i % sim.world.size, i / sim.world.size))
        .filter(|&c| sim.fog.explored(c) && !occupied.contains(&c))
        .filter(|&c| crate::resources::is_bank(&sim.world, c))
        .filter(|&(x, y)| [(x.wrapping_sub(1), y), (x + 1, y), (x, y.wrapping_sub(1)), (x, y + 1)]
            .into_iter().any(|(nx, ny)| sim.world.river.get(nx, ny) == Some(&true)))
        .filter(|&(x, y)| sim.water.depth.get(x, y).is_some_and(|d| *d <= crate::buildings::WET_DEPTH))
        .min_by_key(|&c| (dist2(c, camp), c.1, c.0))
}

pub(crate) fn settle(sim: &mut SimState, cell: Cell) -> Result<(), crate::SimError> {
    let invalid = |m: String| crate::SimError::InvalidCommand(m);
    if sim.colony.is_founded() {
        return Err(invalid("the colony is already founded".into()));
    }
    if !sim.height.in_bounds(cell.0, cell.1) {
        return Err(invalid(format!("cell {cell:?} is off the map")));
    }
    if !sim.fog.explored(cell) {
        return Err(invalid(format!("cell {cell:?} is unexplored")));
    }
    if let Err(e) = crate::site::can_settle(sim, cell) {
        if e == crate::site::SettleError::NoRoom {
            return Err(invalid(format!("cell {cell:?}: NoRoom for a store and a burrow nearby")));
        }
    }
    if sim.water.depth.get(cell.0, cell.1).is_some_and(|d| *d > crate::buildings::WET_DEPTH)
        || sim.world.river.get(cell.0, cell.1).copied().unwrap_or(false)
        || sim.world.lake.get(cell.0, cell.1).copied().unwrap_or(false)
    {
        return Err(invalid(format!("cell {cell:?} is water: beavers cannot settle there")));
    }
    let supplies = supplies_bank(sim, cell)
        .ok_or_else(|| invalid("no clear, dry riverbank for starting supplies".into()))?;
    let land = crate::WorldLand { world: &sim.world, depth: &sim.water.depth };
    sim.colony.arrive((cell.0 as f32 + 0.5, cell.1 as f32 + 0.5), &land).map_err(|e| invalid(format!("{e:?}")))?;
    let swim = swim_cells(sim, supplies, sim.colony.beavers.len());
    let near = swim.first().is_some_and(|&c| dist2(c, supplies) <= SWIM_REACH * SWIM_REACH);
    if near && swim.len() == sim.colony.beavers.len() {
        for (b, c) in sim.colony.beavers.iter_mut().zip(swim) {
            b.pos = (c.0 as f32 + 0.5, c.1 as f32 + 0.5);
        }
    }
    for &(g, a) in &crate::content::content().start {
        sim.buildings.stores.deposit(g, a).map_err(|e| invalid(e.to_string()))?;
    }
    sim.work.supplies = Some(supplies);
    sim.fog.reveal((cell.0 as f32 + 0.5, cell.1 as f32 + 0.5), crate::fog::SETTLE_SIGHT);
    crate::agents::place_beavers(sim);
    Ok(())
}

fn build_river(seed: u64) -> Result<(SimState, DamSite), StartError> {
    build_river_on(SimState::new(seed))
}

fn build_river_on(mut sim: SimState) -> Result<(SimState, DamSite), StartError> {
    let mut plan = river_plan(&mut sim).ok_or(StartError::NoSite)?;
    if crate::site::can_settle(&sim, plan.camp).is_err() {
        let (cx, cy) = plan.camp;
        let r = 50usize;
        let mut best: Option<(f32, Cell)> = None;
        for y in cy.saturating_sub(r)..(cy + r).min(sim.world.size) {
            for x in cx.saturating_sub(r)..(cx + r).min(sim.world.size) {
                if crate::site::can_settle(&sim, (x, y)).is_ok() {
                    let v = crate::site::score(&sim, (x, y)).overall() - 0.01 * (x.abs_diff(cx) + y.abs_diff(cy)) as f32;
                    if best.is_none_or(|(b, _)| v > b) {
                        best = Some((v, (x, y)));
                    }
                }
            }
        }
        if let Some((_, c)) = best {
            plan.camp = c;
        }
    }
    sim.fog = crate::fog::Fog::unseen(sim.world.size);
    sim.fog.reveal((plan.camp.0 as f32 + 0.5, plan.camp.1 as f32 + 0.5), crate::fog::ARRIVAL_SIGHT);
    mud_near_camp(&mut sim, plan.camp, &plan.dam, plan.height);
    Ok((sim, DamSite { cells: plan.dam, height: plan.height, camp: plan.camp }))
}

/// Mud banks within `START_MUD_REACH` of the camp, counted as `mud_near_camp` counts them.
pub(crate) fn mud_near(sim: &SimState, camp: Cell) -> f32 {
    let r = START_MUD_REACH;
    sim.resources.items().iter()
        .filter(|x| x.kind == crate::content::content().roles.bank_mud && dist2(x.cell, camp) <= r * r)
        .map(|x| x.amount)
        .sum()
}

/// Early mud: where the map left too little near the camp, a few mud banks along the
/// river nearest it, clear of the camp itself, the starting supplies and the first pond.
fn mud_near_camp(sim: &mut SimState, camp: Cell, dam: &[Cell], height: f32) {
    let mut have = mud_near(sim, camp);
    if have >= START_MUD {
        return;
    }
    let mut clear: std::collections::HashSet<Cell> = dam.iter().copied()
        .chain(crate::dam_preview::preview(sim, dam, height).map(|p| p.flooded).unwrap_or_default())
        .chain(sim.resources.items().iter().map(|r| r.cell))
        .chain(sim.buildings.buildings.iter().flat_map(|b| b.cells()))
        .collect();
    clear.extend(supplies_bank(sim, camp));
    let river = |(x, y): Cell| sim.world.river.get(x, y) == Some(&true);
    let spots: Vec<Cell> = cells_by_distance(sim, camp, START_MUD_REACH).into_iter()
        .filter(|&c| dist2(c, camp) > START_MUD_CLEAR * START_MUD_CLEAR && !clear.contains(&c) && sim.fog.explored(c))
        .filter(|&c| crate::resources::is_bank(&sim.world, c) && sim.water.depth.get(c.0, c.1).is_some_and(|d| *d <= crate::buildings::WET_DEPTH))
        .filter(|&(x, y)| [(x.wrapping_sub(1), y), (x + 1, y), (x, y.wrapping_sub(1)), (x, y + 1)].into_iter().any(river))
        .collect();
    let mut placed: Vec<Cell> = Vec::new();
    for c in spots {
        if have >= START_MUD {
            break;
        }
        if placed.iter().all(|&p| dist2(p, c) >= START_MUD_SPACING * START_MUD_SPACING) {
            sim.resources.add(crate::content::content().roles.bank_mud, c, crate::resources::MUD_AMOUNT);
            placed.push(c);
            have += crate::resources::MUD_AMOUNT;
        }
    }
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct DamSite {
    pub cells: Vec<Cell>,
    pub height: f32,
    pub camp: Cell,
}

pub fn new_game_with_dam(seed: u64) -> Result<(SimState, DamSite), StartError> {
    build_river(seed)
}

pub fn new_game_on_map(id: &str) -> Result<(SimState, DamSite), StartError> {
    let (info, world) = crate::maps::world(id).map_err(|e| StartError::Build(e.to_string()))?;
    build_river_on(on_flora(SimState::from_world(info.seed(), world), &info.id))
}

pub fn new_game_on_world(info: &crate::maps::MapInfo, world: crate::worldgen::World) -> Result<(SimState, DamSite), StartError> {
    build_river_on(on_flora(SimState::from_world(info.seed(), world), &info.id))
}

/// The world with its plants drawn from this map's flora.
fn on_flora(mut sim: SimState, map: &str) -> SimState {
    crate::wetland::set_flora(&mut sim, map);
    sim
}

use crate::water::Cell;
