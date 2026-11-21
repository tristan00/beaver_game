include!(concat!(env!("OUT_DIR"), "/starts_constants.rs"));
use crate::SimState;

#[derive(Clone, Debug, PartialEq)]
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

pub fn dam_near(sim: &SimState, camp: Cell, radius: usize) -> Option<(Vec<Cell>, f32)> {
    dam_candidates(sim, Some((camp, radius + MAX_DAM_CELLS / 2)), 32)
        .into_iter()
        .filter(|(_, cells, _)| {
            let mid = cells[cells.len() / 2];
            dist2(mid, camp) <= radius * radius && cells.iter().all(|&c| sim.fog.explored(c))
        })
        .map(|(_, cells, h)| (cells, h))
        .min_by_key(|(_, h)| (*h > crate::dam_preview::DEFAULT_HEIGHT + 1e-3) as u8)
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

#[derive(Clone, Copy, Debug, PartialEq)]
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
    let mut camps: Vec<Cell> = near_cells(mid, 60).collect();
    camps.sort_by_key(|&c| (dist2(c, mid), c.1, c.0));
    camps.into_iter()
        .filter(|&c| c.0 >= 3 && c.1 >= 3 && c.0 + 3 < size && c.1 + 3 < size)
        .filter(|&c| CAMP_DAM_DIST.contains(&(dist2(c, mid).isqrt())))
        .filter(|&c| near_cells(c, 2).all(|(x, y)| !wet(x, y)))
        .filter(|&c| near_cells(c, BANK_MAX).any(|(x, y)| sim.world.river.get(x, y).copied().unwrap_or(false)))
        .filter(|&c| trees.iter().filter(|&&t| dist2(c, t) <= TREE_RADIUS * TREE_RADIUS && dist2(c, t) > 9).take(TREES_WANTED).count() >= TREES_WANTED)
        .next()
}

fn river_plan(sim: &mut SimState) -> Option<RiverPlan> {
    settle_water(sim);
    let candidates = dam_candidates(sim, None, 32);
    let mut best: Option<((u8, bool, f32), RiverPlan)> = None;
    for (_, dam, height) in candidates {
        let Some(effect) = dam_effect(sim, &dam, height) else { continue };
        let visible = effect.is_visible();
        let key = (effect.fills as u8 + visible as u8, effect.drop >= MIN_DROP, effect.closeness());
        if best.as_ref().is_some_and(|(b, _)| *b >= key) && !(effect.fills && visible) {
            continue;
        }
        if crate::dams::check_line(sim, &dam).is_err() {
            continue;
        }
        let Some(camp) = camp_near(sim, &dam, height) else { continue };
        let plan = RiverPlan { camp, dam, height };
        if effect.fills && visible {
            return Some(plan);
        }
        if best.as_ref().is_none_or(|(b, _)| key > *b) {
            best = Some((key, plan));
        }
    }
    best.map(|(_, plan)| plan)
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
    let occupied: std::collections::HashSet<Cell> = sim.resources.items.iter()
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
    for b in sim.colony.beavers.iter_mut().filter(|b| b.stage == crate::colony::Stage::Yearling) {
        b.age_days = KIT_AGE_DAYS;
        b.stage = crate::colony::Stage::Kit;
    }
    let swim = swim_cells(sim, supplies, sim.colony.beavers.len());
    let near = swim.first().is_some_and(|&c| dist2(c, supplies) <= SWIM_REACH * SWIM_REACH);
    if near && swim.len() == sim.colony.beavers.len() {
        for (b, c) in sim.colony.beavers.iter_mut().zip(swim) {
            b.pos = (c.0 as f32 + 0.5, c.1 as f32 + 0.5);
        }
    }
    let larder = crate::hud::food_need_per_day(sim) * START_LARDER_DAYS;
    sim.colony.food = sim.buildings.stores.deposit(crate::buildings::Good::Food, larder).map_err(|e| invalid(e.to_string()))?;
    for (g, a) in START_BUNDLE {
        sim.buildings.stores.deposit(g, a).map_err(|e| invalid(e.to_string()))?;
    }
    sim.colony.wood = sim.buildings.stores.amount(crate::buildings::Good::Sticks);
    sim.work.supplies = Some(supplies);
    {
        let allowed = crate::fog::open_at(&sim.world);
        sim.fog.reveal((cell.0 as f32 + 0.5, cell.1 as f32 + 0.5), crate::fog::SETTLE_SIGHT, &allowed);
    }
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
    sim.work.auto_organise = true;
    sim.fog = crate::fog::Fog::unseen(sim.world.size);
    {
        let allowed = crate::fog::open_at(&sim.world);
        sim.fog.reveal((plan.camp.0 as f32 + 0.5, plan.camp.1 as f32 + 0.5), crate::fog::ARRIVAL_SIGHT, &allowed);
    }
    Ok((sim, DamSite { cells: plan.dam, height: plan.height, camp: plan.camp }))
}

#[derive(Clone, Debug, PartialEq)]
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
    build_river_on(SimState::from_world(info.seed(), world))
}

pub fn new_game_on_world(info: &crate::maps::MapInfo, world: crate::worldgen::World) -> Result<(SimState, DamSite), StartError> {
    build_river_on(SimState::from_world(info.seed(), world))
}

pub fn new_game(seed: u64) -> Result<SimState, StartError> {
    build_river(seed).map(|(sim, _)| sim)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn starting_supplies_are_on_a_clear_bank_reachable_from_arrival() {
        for seed in [1, 2, 3] {
            let mut sim = SimState::new(seed);
            let camp = crate::site::best_site(&sim).expect("valid founding site");
            settle(&mut sim, camp).expect("settle beside the river");
            let pile = sim.work.supplies.expect("starting supplies");
            assert!(sim.fog.explored(pile));
            assert!(!sim.resources.blocks(pile));
            assert!(sim.water.depth.get(pile.0, pile.1).unwrap() <= &crate::buildings::WET_DEPTH);
            assert!(sim.world.river.as_slice().iter().enumerate().any(|(i, &river)| {
                river && (i % sim.world.size).abs_diff(pile.0) + (i / sim.world.size).abs_diff(pile.1) == 1
            }), "supplies must touch the river, seed {seed}");
            let mut nav = sim.nav.clone();
            nav.rebuild(&sim);
            for beaver in &sim.colony.beavers {
                let from = sim.agents.get(beaver.id).expect("arriving beaver").cell;
                assert!(nav.find_path(beaver.id, from, crate::nav::Goal::Beside(pile)).is_some());
            }
            for (good, amount) in START_BUNDLE {
                assert_eq!(sim.work.stock_of(crate::agents::SUPPLIES, good), amount);
            }
        }
    }
}

use crate::water::Cell;
