//! Choosing where to settle: how good a spot is (water, trees, safety, room to build) and whether
//! the family can settle there at all. The settle tag in the game and scripted players both use it.
use crate::buildings::BuildingKind;
use crate::water::Cell;
use crate::SimState;

/// A spot's qualities, each 0..=1 (higher is better).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SiteScore {
    pub water: f32,
    pub trees: f32,
    pub safety: f32,
    /// Clear, dry ground close by: room for a store and a burrow.
    pub room: f32,
    /// Food plants and mud close by: what the family lives and builds on before it can haul.
    pub food: f32,
}

impl SiteScore {
    pub fn overall(&self) -> f32 {
        0.2 * self.water + 0.15 * self.trees + 0.15 * self.safety + 0.25 * self.room + 0.25 * self.food
    }
    pub fn verdict(&self) -> &'static str {
        match self.overall() {
            o if o >= 0.7 => "Excellent",
            o if o >= 0.5 => "Good",
            o if o >= 0.3 => "Poor",
            _ => "Bad",
        }
    }
}

/// Why a spot cannot be settled.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SettleError {
    OutOfBounds,
    Locked,
    Unexplored,
    Water,
    /// No room within reach for a store and a burrow.
    NoRoom,
}

/// Plants within `FOOD_REACH` cells (Manhattan) that make a spot's food full marks, and the mud.
const FOOD_REACH: usize = 14;
const GOOD_PLANTS: f32 = 20.0;
const GOOD_MUD: f32 = 40.0;

/// How far (cells) from the settling spot the camp's first buildings must fit.
pub const CAMP_RADIUS: usize = 8;

fn wet(s: &SimState, c: Cell) -> bool {
    s.world.river.get(c.0, c.1) == Some(&true) || s.world.lake.get(c.0, c.1) == Some(&true) || s.water.depth.get(c.0, c.1).is_some_and(|d| *d > 0.05)
}

/// Clear dry cells near `cell` where a warehouse (3×3) fits, as a share of what a comfortable camp
/// wants (four such spots), and whether at least one exists.
fn room(s: &SimState, cell: Cell) -> (f32, bool) {
    let n = s.world.size;
    let mut spots = 0;
    let r = CAMP_RADIUS;
    for y in cell.1.saturating_sub(r)..(cell.1 + r).min(n.saturating_sub(3)) {
        for x in cell.0.saturating_sub(r)..(cell.0 + r).min(n.saturating_sub(3)) {
            if (x, y) == cell || x.abs_diff(cell.0) + y.abs_diff(cell.1) > r + 2 {
                continue;
            }
            let terrain = crate::buildings::Terrain { height: &s.height, depth: &s.water.depth };
            let clear = (0..3).all(|dy| (0..3).all(|dx| !s.resources.blocks((x + dx, y + dy)) && !wet(s, (x + dx, y + dy))));
            if clear && s.buildings.can_place_site(BuildingKind::Warehouse, (x, y), &s.tech, terrain).is_ok() {
                spots += 1;
                if spots >= 4 {
                    return (1.0, true);
                }
            }
        }
    }
    (spots as f32 / 4.0, spots > 0)
}

/// The qualities of `cell` as a place to settle.
pub fn score(s: &SimState, cell: Cell) -> SiteScore {
    let world = &s.world;
    let n = world.size as i32;
    let (cx, cy) = (cell.0 as i32, cell.1 as i32);
    let (mut nearest, mut river_low) = (f32::MAX, f32::MAX);
    for dy in -12..=12i32 {
        for dx in -12..=12i32 {
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
    } else if nearest <= 5.0 {
        1.0
    } else {
        (1.0 - (nearest - 5.0) / 8.0).clamp(0.0, 1.0)
    };
    let (mut tot, mut forest, mut slope) = (0, 0, 0.0f32);
    for dy in -6..=6i32 {
        for dx in -6..=6i32 {
            let (x, y) = (cx + dx, cy + dy);
            if x < 0 || y < 0 || x >= n || y >= n || dx * dx + dy * dy > 36 {
                continue;
            }
            tot += 1;
            forest += s.resources.blocks((x as usize, y as usize)) as i32;
            slope += world.slope.get(x as usize, y as usize).copied().unwrap_or(0.0);
        }
    }
    let tot = tot.max(1) as f32;
    // Some forest close by is good (wood); a thick one leaves no room.
    let share = forest as f32 / tot;
    let trees = (1.0 - (share - 0.35).abs() / 0.35).clamp(0.0, 1.0);
    let gentle = (1.0 - (slope / tot) / 0.8).clamp(0.0, 1.0);
    let above = if river_low == f32::MAX { 1.0 } else { ((world.surface.get(cell.0, cell.1).copied().unwrap_or(0.0) - river_low) / 1.5).clamp(0.0, 1.0) };
    let edge = cell.0.min(cell.1).min(world.size - 1 - cell.0).min(world.size - 1 - cell.1);
    let room = if edge < 4 { 0.0 } else { room(s, cell).0 };
    // Plants within a forager's easy reach, and some mud for the first buildings.
    use crate::resources::ResourceKind;
    let near = |c: Cell, r: usize| c.0.abs_diff(cell.0) + c.1.abs_diff(cell.1) <= r;
    let plants = s.resources.items.iter().filter(|r| r.kind == ResourceKind::Plant && near(r.cell, FOOD_REACH)).count();
    let mud: f32 = s.resources.items.iter().filter(|r| r.kind == ResourceKind::MudBank && near(r.cell, FOOD_REACH)).map(|r| r.amount).sum();
    let food = 0.8 * (plants as f32 / GOOD_PLANTS).min(1.0) + 0.2 * (mud / GOOD_MUD).min(1.0);
    SiteScore { water, trees, safety: 0.5 * gentle + 0.5 * above, room, food }
}

/// Whether the family can settle at `cell`: on the map, in an open valley, explored, dry, away
/// from the map's edge, with room for a store and a burrow within reach.
pub fn can_settle(s: &SimState, cell: Cell) -> Result<(), SettleError> {
    let valley = s.world.valley_of(cell).ok_or(SettleError::OutOfBounds)?;
    if !s.world.is_open(valley) {
        return Err(SettleError::Locked);
    }
    if !s.fog.explored(cell) {
        return Err(SettleError::Unexplored);
    }
    if wet(s, cell) {
        return Err(SettleError::Water);
    }
    let edge = cell.0.min(cell.1).min(s.world.size - 1 - cell.0).min(s.world.size - 1 - cell.1);
    if edge < 4 || !room(s, cell).1 {
        return Err(SettleError::NoRoom);
    }
    Ok(())
}

/// The best explored spot to settle, by overall score (scripted players, `settle best`).
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
