include!(concat!(env!("OUT_DIR"), "/site_constants.rs"));
use crate::water::Cell;
use crate::SimState;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SiteScore {
    pub water: f32,
    pub trees: f32,
    pub safety: f32,
    pub room: f32,
    pub food: f32,
}

impl SiteScore {
    pub fn overall(&self) -> f32 {
        WATER_SCORE_WEIGHT * self.water + TREE_SCORE_WEIGHT * self.trees + SAFETY_SCORE_WEIGHT * self.safety + ROOM_SCORE_WEIGHT * self.room + FOOD_SCORE_WEIGHT * self.food
    }
    pub fn verdict(&self) -> &'static str {
        match self.overall() {
            o if o >= EXCELLENT_SCORE => "Excellent",
            o if o >= GOOD_SCORE => "Good",
            o if o >= POOR_SCORE => "Poor",
            _ => "Bad",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SettleError {
    OutOfBounds,
    Unexplored,
    Water,
    NoRoom,
}



fn wet(s: &SimState, c: Cell) -> bool {
    s.world.river.get(c.0, c.1) == Some(&true) || s.world.lake.get(c.0, c.1) == Some(&true) || s.water.depth.get(c.0, c.1).is_some_and(|d| *d > crate::buildings::WET_DEPTH)
}

fn room(s: &SimState, cell: Cell) -> (f32, bool) {
    let n = s.world.size;
    let mut spots = 0;
    let r = CAMP_RADIUS;
    for y in cell.1.saturating_sub(r)..(cell.1 + r).min(n.saturating_sub(3)) {
        for x in cell.0.saturating_sub(r)..(cell.0 + r).min(n.saturating_sub(3)) {
            if (x, y) == cell || x.abs_diff(cell.0) + y.abs_diff(cell.1) > r + 2 {
                continue;
            }
            let fp: Vec<Cell> = (0..3).flat_map(|dy| (0..3).map(move |dx| (x + dx, y + dy))).collect();
            let n_wet = fp.iter().filter(|&&c| wet(s, c)).count();
            let clear = fp.iter().all(|&c| !s.buildings.buildings.iter().any(|b| b.cells().any(|k| k == c)));
            if clear && n_wet > 0 && n_wet <= 4 {
                spots += 1;
                if spots >= ROOM_TARGET_SPOTS {
                    return (1.0, true);
                }
            }
        }
    }
    (spots as f32 / ROOM_TARGET_SPOTS as f32, spots > 0)
}

pub fn score(s: &SimState, cell: Cell) -> SiteScore {
    let world = &s.world;
    let n = world.size as i32;
    let (cx, cy) = (cell.0 as i32, cell.1 as i32);
    let (mut nearest, mut river_low) = (f32::MAX, f32::MAX);
    for dy in -(WATER_SCAN_RADIUS as i32)..=WATER_SCAN_RADIUS as i32 {
        for dx in -(WATER_SCAN_RADIUS as i32)..=WATER_SCAN_RADIUS as i32 {
            let (x, y) = (cx + dx, cy + dy);
            if x < 0 || y < 0 || x >= n || y >= n {
                continue;
            }
            let c = (x as usize, y as usize);
            if world.river.get(c.0, c.1) == Some(&true) || world.lake.get(c.0, c.1) == Some(&true) {
                nearest = nearest.min(((dx * dx + dy * dy) as f32).sqrt());
                river_low = river_low.min(*world.surface.get(c.0, c.1).unwrap());
            }
        }
    }
    let water = if nearest == f32::MAX || nearest < 1.0 {
        0.0
    } else if nearest <= IDEAL_WATER_DISTANCE {
        1.0
    } else {
        (1.0 - (nearest - IDEAL_WATER_DISTANCE) / WATER_DISTANCE_FALLOFF).clamp(0.0, 1.0)
    };
    let (mut tot, mut forest, mut slope) = (0, 0, 0.0f32);
    for dy in -(LAND_SCAN_RADIUS as i32)..=LAND_SCAN_RADIUS as i32 {
        for dx in -(LAND_SCAN_RADIUS as i32)..=LAND_SCAN_RADIUS as i32 {
            let (x, y) = (cx + dx, cy + dy);
            if x < 0 || y < 0 || x >= n || y >= n || dx * dx + dy * dy > (LAND_SCAN_RADIUS * LAND_SCAN_RADIUS) as i32 {
                continue;
            }
            tot += 1;
            forest += s.resources.blocks((x as usize, y as usize)) as i32;
            slope += world.slope.get(x as usize, y as usize).copied().unwrap_or(0.0);
        }
    }
    let tot = tot.max(1) as f32;
    let share = forest as f32 / tot;
    let trees = (1.0 - (share - IDEAL_FOREST_SHARE).abs() / IDEAL_FOREST_SHARE).clamp(0.0, 1.0);
    let gentle = (1.0 - (slope / tot) / SAFE_SLOPE_RANGE).clamp(0.0, 1.0);
    let above = if river_low == f32::MAX { 1.0 } else { ((world.surface.get(cell.0, cell.1).copied().unwrap_or(0.0) - river_low) / SAFE_ELEVATION_RANGE).clamp(0.0, 1.0) };
    let edge = cell.0.min(cell.1).min(world.size - 1 - cell.0).min(world.size - 1 - cell.1);
    let room = if edge < EDGE_CLEARANCE { 0.0 } else { room(s, cell).0 };
    use crate::resources::ResourceKind;
    let near = |c: Cell, r: usize| c.0.abs_diff(cell.0) + c.1.abs_diff(cell.1) <= r;
    let plants = s.resources.items.iter().filter(|r| r.kind == ResourceKind::Plant && near(r.cell, FOOD_REACH)).count();
    let mud: f32 = s.resources.items.iter().filter(|r| r.kind == ResourceKind::MudBank && near(r.cell, FOOD_REACH)).map(|r| r.amount).sum();
    let food = PLANT_FOOD_SCORE_WEIGHT * (plants as f32 / GOOD_PLANTS).min(1.0) + CLAY_FOOD_SCORE_WEIGHT * (mud / GOOD_MUD).min(1.0);
    SiteScore { water, trees, safety: SLOPE_SAFETY_WEIGHT * gentle + ELEVATION_SAFETY_WEIGHT * above, room, food }
}

pub fn can_settle(s: &SimState, cell: Cell) -> Result<(), SettleError> {
    if !s.height.in_bounds(cell.0, cell.1) {
        return Err(SettleError::OutOfBounds);
    }
    if !s.fog.explored(cell) {
        return Err(SettleError::Unexplored);
    }
    if wet(s, cell) {
        return Err(SettleError::Water);
    }
    let edge = cell.0.min(cell.1).min(s.world.size - 1 - cell.0).min(s.world.size - 1 - cell.1);
    if edge < EDGE_CLEARANCE || !room(s, cell).1 {
        return Err(SettleError::NoRoom);
    }
    Ok(())
}

pub fn best_site(s: &SimState) -> Option<Cell> {
    let mut best: Option<(f32, Cell)> = None;
    for y in (0..s.world.size).step_by(2) {
        for x in (0..s.world.size).step_by(2) {
            if !s.fog.explored((x, y)) {
                continue;
            }
            if can_settle(s, (x, y)).is_ok() {
                let v = score(s, (x, y)).overall();
                if best.is_none_or(|(b, _)| v > b) {
                    best = Some((v, (x, y)));
                }
            }
        }
    }
    best.map(|b| b.1)
}
