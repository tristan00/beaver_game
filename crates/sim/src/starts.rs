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

const DAM_HEIGHTS: [f32; 7] = [0.5, 0.6, 0.8, 1.0, 1.2, 1.5, 2.0];
const POND_AREA: usize = 1500;
const POND_AREA_RANGE: std::ops::RangeInclusive<usize> = 300..=6000;
const MAX_DAM_CELLS: usize = 50;
const CANDIDATE_STRIDE: usize = 3;
const SPILL_FLOW_MARGIN: f32 = 1.5;
pub const START_RISE: f32 = 0.6;
pub const START_NEW: usize = 100;
pub const MIN_DROP: f32 = 0.1;
pub const REACH: usize = 24;
pub const START_INFLOW_MARGIN: f32 = 1.25;
const EDGE_MARGIN: usize = 16;
const CAMP_DAM_DIST: std::ops::RangeInclusive<usize> = 20..=60;
const BANK_MAX: usize = 25;
const TREES_WANTED: usize = 10;
const TREE_RADIUS: usize = 30;
const KIT_AGE_DAYS: f32 = 150.0;

pub(crate) fn dist2(a: Cell, b: Cell) -> usize {
    a.0.abs_diff(b.0).pow(2) + a.1.abs_diff(b.1).pow(2)
}

struct RiverPlan {
    camp: Cell,
    dam: Vec<Cell>,
    height: f32,
}

pub fn dam_near(sim: &SimState, camp: Cell, radius: usize) -> Option<(Vec<Cell>, f32)> {
    dam_candidates(sim)
        .into_iter()
        .filter(|(_, cells, _)| {
            let mid = cells[cells.len() / 2];
            dist2(mid, camp) <= radius * radius && cells.iter().all(|&c| sim.fog.explored(c))
        })
        .map(|(_, cells, h)| (cells, h))
        .min_by_key(|(_, h)| (*h > crate::dam_preview::DEFAULT_HEIGHT + 1e-3) as u8)
}

fn dam_candidates(sim: &SimState) -> Vec<(f32, Vec<Cell>, f32)> {
    let size = sim.world.size;
    let is_river = |x: usize, y: usize| sim.world.river.get(x, y).copied().unwrap_or(false);
    let up = upstream_map(sim);
    let bed = |c: Cell| sim.height.get(c.0, c.1).copied().unwrap_or(0.0);
    let mut out = Vec::new();
    for y in EDGE_MARGIN..size - EDGE_MARGIN {
        for x in EDGE_MARGIN..size - EDGE_MARGIN {
            if !is_river(x, y) || (x + y) % CANDIDATE_STRIDE != 0 {
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

pub fn dammable_streams(sim: &SimState) -> Vec<Cell> {
    const SEPARATE_REACH: usize = 150;
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
        if effect.is_visible() && picked.iter().all(|&p| !below(at, p) && !below(p, at) && dist2(p, at) > 900) {
            picked.push(at);
        }
    }
    picked
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
    near_cells(mid, 60)
        .filter(|&c| c.0 >= 3 && c.1 >= 3 && c.0 + 3 < size && c.1 + 3 < size)
        .filter(|&c| CAMP_DAM_DIST.contains(&(dist2(c, mid).isqrt())))
        .filter(|&c| near_cells(c, 2).all(|(x, y)| !wet(x, y)))
        .filter(|&c| near_cells(c, BANK_MAX).any(|(x, y)| sim.world.river.get(x, y).copied().unwrap_or(false)))
        .filter(|&c| trees.iter().filter(|&&t| dist2(c, t) <= TREE_RADIUS * TREE_RADIUS && dist2(c, t) > 9).count() >= TREES_WANTED)
        .min_by_key(|&c| (dist2(c, mid), c.1, c.0))
}

fn river_plan(sim: &mut SimState) -> Option<RiverPlan> {
    settle_water(sim);
    let candidates = dam_candidates(sim);
    let mut best: Option<((u8, bool, f32), RiverPlan)> = None;
    for (_, dam, height) in candidates {
        let Some(effect) = dam_effect(sim, &dam, height) else { continue };
        let visible = effect.is_visible();
        let key = (effect.fills as u8 + visible as u8, effect.drop >= MIN_DROP, effect.closeness());
        if best.as_ref().is_some_and(|(b, _)| *b >= key) && !(effect.fills && visible) {
            continue;
        }
        if sim.dam_site(&dam).is_err() {
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
    let linked: Vec<Cell> = river.iter().copied().filter(|&c| nav.find_path(who, c, Goal::Beside(landing)).is_some()).collect();
    let pool = if linked.len() >= n { linked } else { river };
    let Some(&anchor) = pool.first() else { return Vec::new() };
    let mut pack = pool;
    pack.sort_by_key(|&c| (dist2(c, anchor), c.1, c.0));
    pack.truncate(n);
    pack
}

const SWIM_REACH: usize = 40;

pub const START_LARDER_DAYS: f32 = 45.0;
pub const START_BUNDLE: [(crate::buildings::Good, f32); 4] =
    [(crate::buildings::Good::Logs, 60.0), (crate::buildings::Good::Sticks, 60.0), (crate::buildings::Good::Mud, 30.0), (crate::buildings::Good::Rope, 2.0)];

pub(crate) fn settle(sim: &mut SimState, cell: Cell) -> Result<(), crate::SimError> {
    let invalid = |m: String| crate::SimError::InvalidCommand(m);
    if sim.colony.is_founded() {
        return Err(invalid("the colony is already founded".into()));
    }
    let valley = sim.world.valley_of(cell).ok_or_else(|| invalid(format!("cell {cell:?} is off the map")))?;
    if !sim.world.is_open(valley) {
        return Err(invalid(format!("cell {cell:?} is in a valley that is not open yet")));
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
    let land = crate::WorldLand { world: &sim.world, depth: &sim.water.depth };
    sim.colony.arrive((cell.0 as f32 + 0.5, cell.1 as f32 + 0.5), &land).map_err(|e| invalid(format!("{e:?}")))?;
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
    let larder = crate::hud::food_need_per_day(sim) * START_LARDER_DAYS;
    sim.colony.food = sim.buildings.stores.deposit(crate::buildings::Good::Food, larder).map_err(|e| invalid(e.to_string()))?;
    for (g, a) in START_BUNDLE {
        sim.buildings.stores.deposit(g, a).map_err(|e| invalid(e.to_string()))?;
    }
    sim.colony.wood = sim.buildings.stores.amount(crate::buildings::Good::Logs);
    sim.work.supplies = Some(cell);
    {
        let allowed = crate::fog::open_at(&sim.world);
        sim.fog.reveal((cell.0 as f32 + 0.5, cell.1 as f32 + 0.5), crate::fog::SETTLE_SIGHT, &allowed);
    }
    sim.world.unlock(cell).map_err(|e| invalid(format!("{e:?}")))?;
    strip_disabled(sim);
    crate::agents::place_beavers(sim);
    Ok(())
}

fn build_river(seed: u64) -> Result<(SimState, DamSite), StartError> {
    build_river_on(SimState::new(seed))
}

fn build_river_on(mut sim: SimState) -> Result<(SimState, DamSite), StartError> {
    let mut plan = river_plan(&mut sim).ok_or(StartError::NoSite)?;
    if let Some(v) = sim.world.valley_of(plan.camp) {
        sim.world.valleys[v.0 as usize].open = true;
    }
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

const FULL_WITHIN: f32 = 0.05;
const WATCH_RADIUS: usize = 30;

impl DamSite {
    pub fn is_built(&self, sim: &SimState) -> bool {
        self.cells.iter().all(|c| sim.water.dam_height.get(c.0, c.1).is_some_and(|h| *h >= self.height - 1e-3))
    }

    pub fn watch(&self, sim: &SimState) -> Option<PondWatch> {
        let p = crate::dam_preview::preview(sim, &self.cells, self.height)?;
        let mid = self.cells[self.cells.len() / 2];
        Some(PondWatch { level: p.level, near: p.flooded.into_iter().filter(|&c| dist2(c, mid) <= WATCH_RADIUS * WATCH_RADIUS).collect() })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct PondWatch {
    pub level: f32,
    near: Vec<Cell>,
}

impl PondWatch {
    pub fn is_full(&self, sim: &SimState) -> bool {
        let full = |c: &&Cell| {
            let depth = sim.water.depth.get(c.0, c.1).copied().unwrap_or(0.0);
            depth > crate::buildings::WET_DEPTH && sim.height.get(c.0, c.1).copied().unwrap_or(0.0) + depth >= self.level - FULL_WITHIN
        };
        self.near.iter().filter(full).count() >= 3
    }
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

use crate::water::Cell;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{agents, Command};

    fn fingerprint(s: &SimState) -> String {
        let beavers: Vec<_> = s.colony.beavers.iter().map(|b| (b.id, b.age_days.to_bits(), b.stage, b.pos.0.to_bits(), b.pos.1.to_bits())).collect();
        let trees: Vec<_> = s.resources.items.iter().map(|r| (r.id, r.kind, r.cell)).collect();
        let agents: Vec<_> = s.agents.by_id.values().map(|a| (a.id, a.cell)).collect();
        format!("{} {} {:?} {:?} {:?} {:?} {:?}", s.seed, s.time_days, s.height.as_slice().iter().map(|h| h.to_bits()).fold(0u64, |a, b| a.wrapping_mul(31).wrapping_add(b as u64)), beavers, trees, agents, s.buildings.buildings)
    }

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
    #[ignore = "beavers: off until the water engine is done"]
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
    #[ignore = "beavers: off until the water engine is done"]
    fn only_adults_dig_a_placed_burrow_and_nothing_is_built_before_the_command() {
        use crate::{buildings::{BuildingKind, Status}, colony::Stage};
        let (mut s, _) = settled(1);
        s.buildings.stores.deposit(crate::buildings::Good::Food, 60.0).unwrap();
        s.work.auto_organise = true;
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
    #[ignore = "beavers: off until the water engine is done"]
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
    #[ignore = "beavers: off until the water engine is done"]
    fn adults_haul_the_bundle_to_a_placed_dam_which_completes_only_from_delivered_wood_and_the_pond_rises() {
        use crate::buildings::Good;
        use crate::colony::Stage;
        let seed = (1..20)
            .find(|&seed| new_game_with_dam(seed).is_ok_and(|(s, d)| crate::dam_preview::preview(&s, &d.cells, d.height).is_some_and(|p| p.fill_days < 120.0)))
            .expect("some seed's river fills its planned pond");
        let (mut s, dam) = settled(seed);
        s.work.auto_organise = true;
        let watch = dam.watch(&s).unwrap();
        let logs_before = s.buildings.stores.amount(Good::Logs);
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
        assert!(s.buildings.stores.amount(Good::Logs) < logs_before, "the dam was built from the bundle on the pile");
    }

    #[test]
    #[ignore = "beavers: off until the water engine is done"]
    fn without_the_dam_command_nothing_is_built() {
        let (s, built, _) = run(1, false, 60.0);
        assert_eq!(built, None);
        assert!(s.work.places.iter().all(|p| p.site.is_none()));
        assert!(s.water.dam_height.as_slice().iter().all(|h| *h <= 0.0));
    }

    #[test]
    #[ignore = "beavers: off until the water engine is done"]
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
    #[ignore = "beavers: off until the water engine is done"]
    fn no_colony_is_founded_until_settle_which_founds_at_the_chosen_cell_and_rejects_an_unusable_one() {
        let (mut s, dam) = new_game_with_dam(1).unwrap();
        assert!(!s.colony.is_founded() && s.colony.beavers.is_empty() && s.agents.by_id.is_empty());
        for _ in 0..100 {
            s.step(0.05).unwrap();
        }
        assert!(!s.colony.is_founded(), "stepping never founds a colony");
        let size = s.world.size;
        assert!(s.apply(Command::Settle { cell: (size, 0) }).is_err());
        let river = (0..size * size).map(|i| (i % size, i / size)).find(|&(x, y)| s.world.river.get(x, y) == Some(&true)).unwrap();
        assert!(s.apply(Command::Settle { cell: river }).is_err());
        if let Some(c) = (0..size * size).map(|i| (i % size, i / size)).find(|&c| s.world.valley_of(c).is_some_and(|v| !s.world.is_open(v))) {
            assert!(s.apply(Command::Settle { cell: c }).is_err(), "a locked valley cannot be settled");
        }
        assert!(!s.colony.is_founded() && s.agents.by_id.is_empty(), "a rejected settle changes nothing");
        s.apply(Command::Settle { cell: dam.camp }).unwrap();
        assert!(s.colony.is_founded());
        assert_eq!(s.colony.beavers.len(), 4);
        assert!(s.colony.lodges.is_empty() && s.buildings.buildings.is_empty(), "they arrive with no home and nothing built");
        assert_eq!(s.work.supplies, Some(dam.camp), "the bundle lies where they chose to settle");
        assert!(s.apply(Command::Settle { cell: dam.camp }).is_err(), "only one colony");
    }

    #[test]
    #[ignore = "beavers: off until the water engine is done"]
    fn a_long_run_with_no_orders_gives_no_duties_and_builds_nothing() {
        let (mut s, _) = settled(2);
        for _ in 0..1200 {
            s.step(0.05).unwrap();
        }
        assert!(s.work.auto_organise && s.work.managed.is_empty());
        assert!(s.buildings.buildings.is_empty() && s.work.places.is_empty(), "nothing built");
        assert!(s.agents.by_id.values().all(|a| a.assignment.is_none() && a.load.is_none()), "no gather duty");
        assert!(s.work.places.iter().all(|p| p.site.is_none()), "no build duty");
    }

    #[test]
    #[ignore = "beavers: off until the water engine is done"]
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
    #[ignore = "beavers: off until the water engine is done"]
    fn the_bundle_builds_a_shelter_a_store_and_the_first_dam() {
        use crate::buildings::BuildingKind;
        let (mut s, dam) = settled(1);
        let burrow = place_near(&mut s, BuildingKind::Burrow);
        let store = place_near(&mut s, BuildingKind::Warehouse);
        let dam_site = s.order_dam(&dam.cells, dam.height).unwrap();
        let done = |s: &SimState, id| s.buildings.building(id).is_some_and(|b| b.is_complete());
        let mut next = 0.0;
        while s.time_days < 90.0 && !(done(&s, burrow) && done(&s, store) && done(&s, dam_site)) {
            if s.time_days >= next {
                next += 1.0;
                if let Some(p) = crate::hud::problems(&s).first() {
                    if !matches!(p.fix, crate::hud::Fix::Place(_)) {
                        let _ = crate::hud::apply_fix(&mut s, p.fix);
                    }
                }
            }
            s.step(0.1).unwrap();
        }
        assert!(done(&s, burrow), "burrow by day {:.0}", s.time_days);
        assert!(done(&s, store), "warehouse by day {:.0}", s.time_days);
        assert!(done(&s, dam_site), "dam by day {:.0}", s.time_days);
        s.step(0.1).unwrap();
        assert!(s.work.supplies.is_none(), "the pile is gone once the store stands");
        assert_eq!(s.colony.population(), 4, "nobody died building the camp");
    }

    #[test]
    #[ignore = "beavers: off until the water engine is done"]
    fn a_finished_burrow_houses_the_family_and_a_store_replaces_the_pile() {
        let (mut s, _) = settled(1);
        let burrow = place_near(&mut s, crate::buildings::BuildingKind::Burrow);
        for _ in 0..(40.0 / 0.1) as usize {
            s.step(0.1).unwrap();
            if s.buildings.building(burrow).is_some_and(|b| b.is_complete()) {
                break;
            }
        }
        assert!(s.buildings.building(burrow).is_some_and(|b| b.is_complete()), "the burrow was dug");
        s.step(0.1).unwrap();
        assert_eq!(s.colony.lodges.len(), 1, "the burrow is the family's home");
        assert!(s.colony.beavers.iter().all(|b| b.lodge == Some(s.colony.lodges[0].id)));
        assert!(s.work.supplies.is_some(), "no store yet, the pile stays");
    }

    #[test]
    #[ignore = "beavers: off until the water engine is done"]
    fn the_family_arrives_with_a_bundle_and_no_home() {
        use crate::buildings::Good;
        let (mut s, _) = settled(1);
        let need = crate::hud::food_need_per_day(&s);
        let food = s.buildings.stores.amount(Good::Food);
        assert!((food - need * START_LARDER_DAYS).abs() < 1e-3 && food > 0.0, "larder {food} for need {need}/day");
        assert_eq!(s.colony.food, food, "the colony eats from that same store");
        for (g, a) in START_BUNDLE {
            assert_eq!(s.buildings.stores.amount(g), a, "{g:?} in the bundle");
        }
        assert!(s.colony.lodges.is_empty() && s.colony.beavers.iter().all(|b| b.lodge.is_none()), "no home until they build one");
        assert!(s.agents.by_id.values().all(|a| a.load.is_none()));
        while s.time_days < 14.0 {
            s.step(0.5).unwrap();
        }
        assert_eq!(s.colony.population(), 4, "nobody died in the first two weeks");
        assert!(s.buildings.stores.amount(Good::Food) < food, "they ate from the larder");
    }

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

    #[test]
    #[ignore = "beavers: off until the water engine is done"]
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
        let short = crate::dam_preview::preview(&s, &line(1), 1.0).unwrap();
        let at = short.spill_at.expect("a three-cell dam is walked round in this valley");
        assert!(short.level < short.crest - 0.4, "the pond only rises to the saddle: {} of {}", short.level, short.crest);
        assert!(at.0.abs_diff(40) <= 1 && at.1.abs_diff(64) <= 3, "it spills just past the dam end, not at {at:?}");
        let full = crate::dam_preview::preview(&s, &line(7), 1.0).unwrap();
        assert!(full.spill_at.is_none() && holds(&s, &line(7), &full.flooded), "a dam tied into both banks holds");
        assert!(full.flooded.iter().all(|c| c.0 < 40), "the held pond lies upstream");
        let (cells, _, _) = crate::dam_preview::extend_to_hold(&s, &line(1), &[1.0; 3], 20).expect("can be tied in");
        assert!(cells.len() > 3 && crate::dam_preview::preview_heights(&s, &cells, &vec![1.0; cells.len()]).is_some());
    }

    #[test]
    fn a_dam_effect_tells_a_running_river_from_a_flat() {
        let mut s = sloping_valley();
        let line: Vec<Cell> = (50..=78).map(|y| (40, y)).collect();
        let running = dam_effect(&s, &line, 2.0).unwrap();
        assert!(running.drop >= 2.0, "0.2 per cell over 12 cells: {}", running.drop);
        assert!((running.rise - 2.0).abs() < 0.01 && running.is_visible(), "{running:?}");
        for x in 0..s.world.size {
            *s.height.get_mut(x, 64).unwrap() = 17.6;
        }
        let flat = dam_effect(&s, &line, 2.0).unwrap();
        assert!(flat.drop < MIN_DROP && !flat.is_visible(), "{flat:?}");
    }

    #[test]
    #[ignore = "beavers: off until the water engine is done"]
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
