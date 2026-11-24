include!(concat!(env!("OUT_DIR"), "/dam_preview_constants.rs"));
use crate::buildings::{Amount, BuildingId, BuildingKind};
use crate::resources::ResourceKind;
use crate::water::Cell;
use std::collections::HashSet;



/// Loads needed to bring a drawn line to these heights: one load per step per metre of wall
/// (a diagonal cell stands for about 1.4 m).
pub fn loads_for(sim: &crate::SimState, cells: &[Cell], heights: &[f32]) -> usize {
    let spans = crate::dams::spans(cells);
    cells.iter().zip(heights).zip(spans).map(|((&c, &h), w)| {
        let have = sim.water.dam_height.get(c.0, c.1).copied().unwrap_or(0.0);
        ((h - have).max(0.0) / crate::dams::STEP_HEIGHT * w - 1e-3).ceil().max(0.0) as usize
    }).sum()
}

/// Materials and packing labour to build these heights.
pub fn cost_for(sim: &crate::SimState, cells: &[Cell], heights: &[f32]) -> (Vec<Amount>, f32) {
    let loads = loads_for(sim, cells, heights) as f32;
    let cost = BuildingKind::Dam.spec().cost.iter().map(|&(g, a)| (g, a * loads)).filter(|c| c.1 > 1e-3).collect();
    (cost, BuildingKind::Dam.spec().work * loads)
}

pub fn heights_to(sim: &crate::SimState, cells: &[Cell], crest: f32) -> (Vec<Cell>, Vec<f32>) {
    cells
        .iter()
        .filter_map(|&c| {
            let ground = *sim.height.get(c.0, c.1)?;
            let h = crest - ground;
            (h > 0.05).then_some((c, h))
        })
        .unzip()
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct DamPreview {
    pub level: f32,
    pub rise: f32,
    pub flooded: Vec<Cell>,
    pub flooded_trees: usize,
    pub flooded_buildings: Vec<BuildingId>,
    pub cost: Vec<Amount>,
    pub crest: f32,
    pub spill_at: Option<Cell>,
    pub pressure: f32,
    pub fill_days: f32,
    pub inflow: f32,
    pub pond_loss: f32,
    /// Where the water settles once the river runs past the dam: the still level plus the
    /// depth it takes to push the inflow over the spill (weir law), and never below today's
    /// surface beside the dam.
    pub flow_level: f32,
    /// How much higher that is than the water beside the dam today.
    pub flow_rise: f32,
    /// The cells that level covers.
    pub flow_flooded: Vec<Cell>,
}

impl DamPreview {
    pub fn fills(&self, margin: f32) -> bool {
        self.inflow >= margin * self.pond_loss
    }
}


pub fn preview(sim: &crate::SimState, cells: &[Cell], height: f32) -> Option<DamPreview> {
    preview_heights(sim, cells, &vec![height; cells.len()])
}

/// The part of a dam's estimate that follows from the ground alone (terrain and dams): the
/// still pond and the land draining to it. It is most of the work, and a dam nobody is
/// building on asks for the same one every day.
pub(crate) struct Ground {
    still: crate::water::StillPond,
    level: f32,
    crest: f32,
    spill_at: Option<Cell>,
    /// The still pond's cells, in map order and as a mask.
    pub(crate) flooded: Vec<Cell>,
    in_pond: Vec<bool>,
    /// The dam, its pond and every cell draining to them.
    watershed: Vec<bool>,
}

/// The line as the estimate takes it: no repeated cell, and only cells that need a wall
/// (a line drawn up onto a bank always has some whose ground already stands at the crest).
fn wall(cells: &[Cell], heights: &[f32]) -> Option<(Vec<Cell>, Vec<f32>)> {
    let unique: HashSet<&Cell> = cells.iter().collect();
    if unique.len() != cells.len() || heights.len() != cells.len() {
        return None;
    }
    let (cells, heights): (Vec<Cell>, Vec<f32>) = cells.iter().copied().zip(heights.iter().copied()).filter(|p| p.1 > 0.0).unzip();
    (!cells.is_empty()).then_some((cells, heights))
}

fn ground(sim: &crate::SimState, cells: &[Cell], heights: &[f32]) -> Option<Ground> {
    let still = crate::water::StillPond::behind(sim, cells, heights).ok()?;
    let pond = still.preview(sim, 0.0);
    let (w, h) = (sim.height.width(), sim.height.height());
    let idx = |&(x, y): &Cell| y * w + x;
    let mut in_pond = vec![false; w * h];
    pond.flooded_cells.iter().for_each(|c| in_pond[idx(c)] = true);
    let terrain = sim.height.as_slice();
    let neighbours = |i: usize| {
        let (x, y) = (i % w, i / w);
        [(x > 0).then(|| i - 1), (x + 1 < w).then(|| i + 1), (y > 0).then(|| i - w), (y + 1 < h).then(|| i + w)]
            .into_iter().flatten()
    };
    let drains_to = |i: usize| neighbours(i).filter(|&j| terrain[j] < terrain[i])
        .min_by(|&a, &b| terrain[a].total_cmp(&terrain[b]));
    let mut seen = vec![false; w * h];
    let mut stack: Vec<usize> = cells.iter().map(idx).collect();
    stack.extend(pond.flooded_cells.iter().map(idx));
    for &i in &stack { seen[i] = true; }
    while let Some(i) = stack.pop() {
        for n in neighbours(i) {
            if !seen[n] && drains_to(n).is_some_and(|d| seen[d]) {
                seen[n] = true;
                stack.push(n);
            }
        }
    }
    Some(Ground { still, level: pond.level, crest: pond.crest, spill_at: pond.spill_at, flooded: pond.flooded_cells, in_pond, watershed: seen })
}

pub fn preview_heights(sim: &crate::SimState, cells: &[Cell], heights: &[f32]) -> Option<DamPreview> {
    let (cells, heights) = wall(cells, heights)?;
    estimate(sim, &cells, &heights, &ground(sim, &cells, &heights)?)
}

/// What `Ground` was worked out from: the world, its terrain and dam grids as they stood, and
/// the line.
pub(crate) type GroundKey = (u64, (u64, u64), (u64, u64), Vec<Cell>, Vec<u32>);
/// Grounds kept, each a few megabytes on a full map.
const GROUNDS_KEPT: usize = 12;

thread_local! {
    static GROUNDS: std::cell::RefCell<Vec<(GroundKey, std::rc::Rc<Ground>)>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// As `preview_heights`, keeping the ground's part for the next call: a dam's daily look at
/// its pond repeats it until terrain or a dam somewhere changes.
pub fn preview_heights_kept(sim: &crate::SimState, cells: &[Cell], heights: &[f32]) -> Option<DamPreview> {
    let (cells, heights) = wall(cells, heights)?;
    let (_, ground) = kept_ground(sim, &cells, &heights)?;
    estimate(sim, &cells, &heights, &ground)
}

/// The ground's part of the estimate for this line (already through `wall`), from those kept
/// or worked out and kept; with what it was worked out from.
fn kept_ground(sim: &crate::SimState, cells: &[Cell], heights: &[f32]) -> Option<(GroundKey, std::rc::Rc<Ground>)> {
    let key: GroundKey = (sim.instance, sim.height.version(), sim.water.dam_height.version(), cells.to_vec(), heights.iter().map(|h| h.to_bits()).collect());
    let kept = GROUNDS.with(|g| {
        let mut g = g.borrow_mut();
        let at = g.iter().position(|(k, _)| *k == key)?;
        let entry = g.remove(at);
        let ground = entry.1.clone();
        g.push(entry);
        Some(ground)
    });
    let ground = match kept {
        Some(ground) => ground,
        None => {
            let ground = std::rc::Rc::new(ground(sim, cells, heights)?);
            GROUNDS.with(|g| {
                let mut g = g.borrow_mut();
                if g.len() >= GROUNDS_KEPT {
                    g.remove(0);
                }
                g.push((key.clone(), ground.clone()));
            });
            ground
        }
    };
    Some((key, ground))
}

/// The still pond behind these heights, kept as `preview_heights_kept` keeps it: its cells in
/// map order, with what they were worked out from.
pub(crate) fn flooded_kept(sim: &crate::SimState, cells: &[Cell], heights: &[f32]) -> Option<(GroundKey, std::rc::Rc<Ground>)> {
    let (cells, heights) = wall(cells, heights)?;
    kept_ground(sim, &cells, &heights)
}

fn estimate(sim: &crate::SimState, cells: &[Cell], heights: &[f32], ground: &Ground) -> Option<DamPreview> {
    let (w, h) = (sim.height.width(), sim.height.height());
    let idx = |&(x, y): &Cell| y * w + x;
    let (still, seen) = (&ground.still, &ground.watershed);
    let in_pond = ground.in_pond.clone();
    let under = |c: &Cell| c.0 < w && c.1 < h && in_pond[idx(c)];

    let flooded_trees = sim.resources.items().iter()
        .filter(|r| r.kind == ResourceKind::Tree && under(&r.cell)).count();
    let flooded_buildings = sim.buildings.buildings.iter()
        .filter(|b| b.cells().any(|c| under(&c))).map(|b| b.id).collect();

    let terrain = sim.height.as_slice();
    let (water, dams) = (sim.water.depth.as_slice(), sim.water.dam_height.as_slice());
    let by_cell: std::collections::HashMap<Cell, f32> = cells.iter().copied().zip(heights.iter().copied()).collect();
    let crest = |c: &Cell| terrain[idx(c)] + by_cell[c];
    let low = cells.iter().min_by(|a, b| crest(a).total_cmp(&crest(b)))?;
    let rise = (ground.level - (terrain[idx(low)] + dams[idx(low)] + water[idx(low)])).max(0.0);

    let forcing = sim.water.forcing();
    let pressure: f32 = sim.water.sources.iter().filter(|s| seen[s.y * w + s.x])
        .map(|s| s.rate * (1.0 + s.seasonality * (forcing.inflow - 1.0)).max(0.0)).sum();

    let cfg = &sim.water.config;
    let (fed, ice) = (sim.water.fed.as_slice(), sim.water.ice.as_slice());
    let loss_at = |i: usize| if fed[i] { 0.0 } else { cfg.evaporation * forcing.evaporation * (1.0 - (ice[i] / crate::tuning::water::ICE_COVER_M).min(1.0)) + cfg.seepage };
    let in_pond: Vec<bool> = {
        let mut v = in_pond;
        cells.iter().for_each(|c| v[idx(c)] = true);
        v
    };
    let lost_on_way: f32 = (0..w * h).filter(|&i| seen[i] && !in_pond[i] && water[i] > crate::water::WET_EPS).map(loss_at).sum();
    let inflow = (pressure - lost_on_way).max(0.0);
    let pond_loss: f32 = ground.flooded.iter().map(idx).map(loss_at).sum();

    // Flowing water stands above the spill by the weir head for the inflow it passes. Over the
    // crest it spreads along the crest cells at the low point; around the dam, over the
    // notch the water finds (as wide as the water engine lets a spill be).
    // The discharge reaching the dam: what the catchment walk finds, or the river the world
    // traced through these cells (its walk stops at small dips in real terrain), by season.
    let river_q = cells.iter()
        .flat_map(|&(x, y)| (-1i64..=1).flat_map(move |dx| (-1i64..=1).map(move |dy| (x as i64 + dx, y as i64 + dy))))
        .filter(|&(x, y)| x >= 0 && y >= 0)
        .filter_map(|(x, y)| sim.world.river_flow.get(x as usize, y as usize).copied())
        .fold(0.0f32, f32::max) * forcing.inflow.max(0.0);
    let q = (inflow * sim.water.cell_m * sim.water.cell_m / 86_400.0).max(river_q);
    let low_crest = cells.iter().map(crest).fold(f32::INFINITY, f32::min);
    let weir_cells = match ground.spill_at {
        Some(_) => crate::water::equilibrium::WEIR_MAX_M,
        None => cells.iter().filter(|c| crest(c) < low_crest + 0.02).count().clamp(1, crate::water::equilibrium::WEIR_MAX_M),
    };
    let head = crate::water::spill_head(q, weir_cells as f32 * sim.water.cell_m);
    let flowing = still.preview(sim, head);
    // The rise is how much deeper the water gets where the dam backs it up most: over every
    // cell the flowing level covers, its level less today's surface there (a dry cell's is
    // its ground).
    let surface = |c: &Cell| terrain[idx(c)] + water[idx(c)];
    let flow_level = flowing.flow_level;
    let flow_rise = flowing.flow_cells.iter().map(|c| flow_level - surface(c)).fold(0.0f32, f32::max);

    let missing: f32 = ground.flooded.iter().map(idx)
        .map(|i| (ground.level - terrain[i] - dams[i] - water[i]).max(0.0)).sum();
    let net = inflow - 0.5 * pond_loss;
    let fill_days = if missing <= 0.0 { 0.0 } else if inflow > pond_loss && net > 0.0 { missing / net } else { f32::INFINITY };

    Some(DamPreview {
        level: ground.level,
        rise,
        flooded: ground.flooded.clone(),
        flooded_trees,
        flooded_buildings,
        cost: cost_for(sim, cells, heights).0,
        crest: ground.crest,
        spill_at: ground.spill_at,
        pressure,
        fill_days,
        inflow,
        pond_loss,
        flow_level,
        flow_rise,
        flow_flooded: flowing.flow_cells,
    })
}

/// The buildings the water would put too deep to use (see `buildings::MAX_BUILDING_DEPTH`) if
/// each cell's depth became `then(x, y)`, leaving out those already that deep.
pub fn drowned_by(sim: &crate::SimState, then: impl Fn(usize, usize) -> f32) -> Vec<BuildingId> {
    let now = |x: usize, y: usize| sim.water.depth.get(x, y).copied().unwrap_or(0.0);
    let max = crate::buildings::MAX_BUILDING_DEPTH;
    sim.buildings.buildings.iter()
        .filter(|b| !b.kind.is_waterworks())
        .filter(|b| crate::buildings::footprint_depth(b.kind, b.pos, now) <= max && crate::buildings::footprint_depth(b.kind, b.pos, |x, y| then(x, y).max(now(x, y))) > max)
        .map(|b| b.id)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The fixture valley runs along y = 64 with its river fed from x = 0.
    fn running_river() -> crate::SimState {
        let mut s = crate::sandbox::on_map("fixture").unwrap();
        crate::sandbox::run_days(&mut s, 2.0);
        s
    }

    #[test]
    fn a_dam_the_river_runs_around_still_raises_the_water_beside_it() {
        let s = running_river();
        // Across the river where it runs at x = 64, but too short to reach the valley sides.
        let y0 = (0..128).max_by(|&a, &b| s.world.river_flow.get(64, a).unwrap().total_cmp(s.world.river_flow.get(64, b).unwrap())).unwrap();
        let short: Vec<Cell> = (y0 - 3..=y0 + 3).map(|y| (64, y)).collect();
        let p = preview_heights(&s, &short, &vec![1.0; short.len()]).unwrap();
        assert!(p.spill_at.is_some(), "the river finds a way round a short dam");
        assert!(p.flow_level > p.level, "flowing water stands above the still level");
        assert!(p.flow_rise > 0.05, "the badge shows the water rising: {}", p.flow_rise);
    }

    #[test]
    fn a_pond_deep_over_a_building_drowns_it() {
        let mut s = running_river();
        let n = s.height.width();
        let dry = |s: &crate::SimState, x: usize, y: usize| (0..3).all(|dy| (0..3).all(|dx| s.water.depth.get(x + dx, y + dy).is_some_and(|&d| d < 0.01)));
        let pos = (4..n - 4).flat_map(|y| (4..n - 4).map(move |x| (x, y))).find(|&(x, y)| dry(&s, x, y)).expect("dry ground");
        s.buildings.buildings.push(crate::buildings::Building { id: BuildingId(77), kind: BuildingKind::Shelter, pos, status: crate::buildings::Status::Complete, output: 0.0, condition: 1.0, reinforced: false, dam: None, canal: None });
        let over = |depth: f32| move |x: usize, y: usize| if (pos.0..pos.0 + 3).contains(&x) && (pos.1..pos.1 + 3).contains(&y) { depth } else { 0.0 };
        assert!(drowned_by(&s, over(0.5)).is_empty(), "shallow water is a beaver's home");
        assert_eq!(drowned_by(&s, over(crate::buildings::MAX_BUILDING_DEPTH + 0.3)), vec![BuildingId(77)]);
    }

    #[test]
    fn water_pouring_over_a_full_dam_stands_above_its_crest() {
        let s = running_river();
        let across: Vec<Cell> = (30..99).map(|y| (64, y)).collect();
        let p = preview_heights(&s, &across, &vec![0.5; across.len()]).unwrap();
        assert!(p.flow_level > p.crest, "{} over a crest at {}", p.flow_level, p.crest);
        assert!(p.flow_flooded.len() >= p.flooded.len());
    }
}

/// What the water really does once a dam stands: a copy of the water runs on with the dam in
/// place, and every cell near it is compared with today. The game's own engine, so it matches
/// play exactly where the quick estimate (`preview_heights`) guesses wrong, for instance a line
/// drawn along a channel rather than across it. It takes about a tenth of a second, so the
/// game runs it in the background.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Settled {
    /// The most the water deepens anywhere near the dam.
    pub rise: f32,
    /// Cells it deepens by a noticeable amount, and the highest surface among them.
    pub flooded: Vec<Cell>,
    pub level: f32,
    /// The depth each of `flooded` settles to.
    pub depths: Vec<f32>,
}

pub fn settle(mut water: crate::water::Water, height: &crate::grid::HeightGrid, cells: &[Cell], heights: &[f32]) -> Option<Settled> {
    let pairs: Vec<(Cell, f32)> = cells.iter().copied().zip(heights.iter().copied())
        .filter(|&(c, h)| h > water.dam_height.get(c.0, c.1).copied().unwrap_or(0.0))
        .collect();
    if pairs.is_empty() {
        return None;
    }
    let (w, h) = (height.width(), height.height());
    let (x0, x1) = (cells.iter().map(|c| c.0).min()?.saturating_sub(SETTLE_REACH), (cells.iter().map(|c| c.0).max()? + SETTLE_REACH).min(w - 1));
    let (y0, y1) = (cells.iter().map(|c| c.1).min()?.saturating_sub(SETTLE_REACH), (cells.iter().map(|c| c.1).max()? + SETTLE_REACH).min(h - 1));
    let dam: HashSet<Cell> = cells.iter().copied().collect();
    let near: Vec<Cell> = (y0..=y1).flat_map(|y| (x0..=x1).map(move |x| (x, y))).filter(|c| !dam.contains(c)).collect();
    let surface = |wt: &crate::water::Water, c: Cell| height.get(c.0, c.1).copied().unwrap_or(0.0) + wt.depth.get(c.0, c.1).copied().unwrap_or(0.0);
    let before: Vec<f32> = near.iter().map(|&c| surface(&water, c)).collect();
    water.raise_dam(&pairs).ok()?;
    let mut t = 0.0;
    while t < SETTLE_DAYS {
        let dt = crate::STEP_DAYS.min(SETTLE_DAYS - t);
        water.step(height, dt);
        t += dt;
    }
    let mut rise = 0.0f32;
    let mut flooded = Vec::new();
    let mut depths = Vec::new();
    let mut level = f32::NEG_INFINITY;
    for (&c, &b) in near.iter().zip(&before) {
        let after = surface(&water, c);
        let r = after - b;
        rise = rise.max(r);
        if r > 0.02 && water.depth.get(c.0, c.1).is_some_and(|&d| d > crate::water::WET_EPS) {
            flooded.push(c);
            depths.push(water.depth.get(c.0, c.1).copied().unwrap_or(0.0));
            level = level.max(after);
        }
    }
    Some(Settled { rise, flooded, level, depths })
}

/// How far (cells) round a dam `settle` compares the water, and for how long it runs.
const SETTLE_REACH: usize = 40;
const SETTLE_DAYS: f32 = 0.05;

#[cfg(test)]
mod settle_tests {
    use super::*;

    /// A line drawn along a creek on the Bavarian map, where the quick estimate saw no rise
    /// but the water rose a third of a metre in play.
    #[test]
    fn settling_finds_the_rise_play_shows() {
        let (mut s, _) = crate::starts::new_game_on_map("bavarian").unwrap();
        // Other tests switch the process-wide default engine: use the game's own.
        s.water.set_engine(crate::water::DEFAULT_ENGINE);
        crate::sandbox::run_days(&mut s, 0.01);
        let cells = crate::sandbox::line(&[(387, 69), (384, 54)]);
        let h = crate::dams::first_targets(&s, &cells);
        let got = settle(s.water.clone(), &s.height, &cells, &h).unwrap();
        assert!(got.rise > 0.25, "settled rise {}", got.rise);
        assert!(!got.flooded.is_empty());
    }
}
