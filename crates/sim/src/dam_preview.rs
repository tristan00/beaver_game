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
    Some(Ground { level: pond.level, crest: pond.crest, spill_at: pond.spill_at, flooded: pond.flooded_cells, in_pond, watershed: seen })
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
    let seen = &ground.watershed;
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

    /// The badge's rise is what the water does days after the dam stands, beside the same water
    /// left without it.
    #[test]
    fn the_settled_rise_is_what_the_water_does() {
        let s = running_river();
        let across: Vec<Cell> = (30..99).map(|y| (64, y)).collect();
        let heights = crate::dams::first_targets(&s, &across);
        let got = settle(s.water.clone(), &s.height, &across, &heights).unwrap();
        let (mut with, mut without) = (s.water.clone(), s.water.clone());
        let pairs: Vec<(Cell, f32)> = across.iter().copied().zip(heights.iter().copied()).collect();
        with.raise_dam(&pairs).unwrap();
        run(&mut with, &s.height, 0.0, 5.0);
        run(&mut without, &s.height, 0.0, 5.0);
        let truth = pond(&with, &without, &s.height, &across);
        assert!(truth.rise > 0.3, "the dam backs the river up: {}", truth.rise);
        assert!((got.rise - truth.rise).abs() < 0.05, "badge {} against {}", got.rise, truth.rise);
    }
}

/// What the water really does once a dam stands: the game's own engine runs on from today
/// twice, with the dam and without it, until the water behind the dam stops rising. The pond
/// is every wet cell, reached from the dam, standing higher than it would without it, so the
/// river's own rise and fall over those hours is not counted. It takes a fraction of a second,
/// so the game runs it in the background.
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Settled {
    /// The most the dam deepens the water anywhere in its pond.
    pub rise: f32,
    /// The pond's cells, and the highest surface among them.
    pub flooded: Vec<Cell>,
    pub level: f32,
    /// The depth each of `flooded` settles to.
    pub depths: Vec<f32>,
}

pub fn settle(water: crate::water::Water, height: &crate::grid::HeightGrid, cells: &[Cell], heights: &[f32]) -> Option<Settled> {
    let pairs: Vec<(Cell, f32)> = cells.iter().copied().zip(heights.iter().copied())
        .filter(|&(c, h)| h > water.dam_height.get(c.0, c.1).copied().unwrap_or(0.0))
        .collect();
    if pairs.is_empty() {
        return None;
    }
    let mut without = water.clone();
    let mut with = water;
    with.raise_dam(&pairs).ok()?;
    // The dam stands from the first step, not from the water's next look at the ground.
    with.refresh(height);
    let mut t = 0.0;
    let mut last: Option<f32> = None;
    loop {
        let until = t + SETTLE_CHUNK_DAYS;
        std::thread::scope(|sc| {
            sc.spawn(|| run(&mut without, height, t, until));
            run(&mut with, height, t, until);
        });
        t = until;
        let pond = pond(&with, &without, height, cells);
        let still = last.is_some_and(|l| (pond.rise - l).abs() < SETTLE_STILL_M);
        if still || t >= SETTLE_MAX_DAYS - 1e-6 {
            return Some(pond);
        }
        last = Some(pond.rise);
    }
}

fn run(water: &mut crate::water::Water, height: &crate::grid::HeightGrid, from: f32, until: f32) {
    let mut t = from;
    while t < until - 1e-6 {
        let dt = crate::STEP_DAYS.min(until - t);
        water.step(height, dt);
        t += dt;
    }
}

/// The cells the dam holds water over: from beside the dam, through every cell whose water it
/// changes by more than `POND_RISE_M` (the channel it starves below, the way round it finds),
/// the wet ones standing that much above the water without it.
fn pond(with: &crate::water::Water, without: &crate::water::Water, height: &crate::grid::HeightGrid, cells: &[Cell]) -> Settled {
    let n = height.width();
    let (ground, on, off) = (height.as_slice(), with.depth.as_slice(), without.depth.as_slice());
    let mut seen = vec![false; n * n];
    cells.iter().for_each(|c| seen[c.1 * n + c.0] = true);
    let mut stack: Vec<usize> = cells.iter().flat_map(|c| crate::water::neighbours8(n, c.1 * n + c.0)).collect();
    let mut out = Settled { rise: 0.0, flooded: Vec::new(), level: f32::NEG_INFINITY, depths: Vec::new() };
    while let Some(i) = stack.pop() {
        let change = on[i] - off[i];
        if seen[i] || change.abs() <= POND_RISE_M {
            continue;
        }
        seen[i] = true;
        stack.extend(crate::water::neighbours8(n, i));
        if change > 0.0 && on[i] > crate::water::WET_EPS {
            out.rise = out.rise.max(change);
            out.level = out.level.max(ground[i] + on[i]);
            out.flooded.push((i % n, i / n));
            out.depths.push(on[i]);
        }
    }
    out
}

/// How the background run settles: it compares the water every `SETTLE_CHUNK_DAYS`, stops
/// once the dam's rise moves less than `SETTLE_STILL_M` between looks (a big pond takes a day
/// or more to fill), and stops waiting at `SETTLE_MAX_DAYS`. A cell counts as pond when it
/// stands `POND_RISE_M` above the water without the dam.
const SETTLE_CHUNK_DAYS: f32 = 0.25;
const SETTLE_STILL_M: f32 = 0.01;
const SETTLE_MAX_DAYS: f32 = 3.0;
const POND_RISE_M: f32 = 0.02;
