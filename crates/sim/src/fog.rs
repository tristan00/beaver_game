//! Fog of war: how much of each cell the colony has seen, 0 (never) ..= 1 (seen). Exploring is the
//! colony's own work: the ground around every beaver outside a lodge is seen as it goes; the family
//! has looked over the ground around where it came in. Building and settling need seen ground.
//!
//! A bare `SimState` sees everything (tests and tools); new games start in fog (`starts`).
use crate::water::Cell;
use crate::worldgen::{ValleyId, World};

/// Seen at least this much counts as explored.
pub const EXPLORED: f32 = 0.5;
/// How far (cells) a beaver sees.
pub const BEAVER_SIGHT: f32 = 6.0;
/// How far (cells) the family has looked around where it arrives.
pub const ARRIVAL_SIGHT: f32 = 22.0;
/// How far (cells) settling reveals around the chosen spot.
pub const SETTLE_SIGHT: f32 = 10.0;

#[derive(Clone, Debug)]
pub struct Fog {
    pub size: usize,
    pub seen: Vec<f32>,
    /// Set when `seen` changed (renderers clear it once they have redrawn).
    pub dirty: bool,
}

impl Fog {
    /// Nothing seen.
    pub fn unseen(size: usize) -> Fog {
        Fog { size, seen: vec![0.0; size * size], dirty: true }
    }
    /// Everything seen.
    pub fn clear(size: usize) -> Fog {
        Fog { size, seen: vec![1.0; size * size], dirty: true }
    }
    pub fn get(&self, c: Cell) -> f32 {
        if c.0 < self.size && c.1 < self.size { self.seen[c.1 * self.size + c.0] } else { 0.0 }
    }
    pub fn explored(&self, c: Cell) -> bool {
        self.get(c) >= EXPLORED
    }
    pub fn explored_cells(&self) -> usize {
        self.seen.iter().filter(|&&v| v >= EXPLORED).count()
    }
    /// Lifts fog in a soft-edged disc, only where `allowed`. Returns whether anything changed.
    pub fn reveal(&mut self, centre: (f32, f32), radius: f32, allowed: &dyn Fn(usize, usize) -> bool) -> bool {
        if self.size == 0 {
            return false;
        }
        let r = radius + 2.0;
        let (x0, x1) = ((centre.0 - r).floor().max(0.0) as usize, ((centre.0 + r).ceil().max(0.0) as usize).min(self.size - 1));
        let (y0, y1) = ((centre.1 - r).floor().max(0.0) as usize, ((centre.1 + r).ceil().max(0.0) as usize).min(self.size - 1));
        let mut changed = false;
        for y in y0..=y1 {
            for x in x0..=x1 {
                let dist = ((x as f32 - centre.0).powi(2) + (y as f32 - centre.1).powi(2)).sqrt();
                let v = ((radius + 2.0 - dist) / 2.0).clamp(0.0, 1.0);
                let i = y * self.size + x;
                if v > self.seen[i] && allowed(x, y) {
                    self.seen[i] = v;
                    changed = true;
                }
            }
        }
        self.dirty |= changed;
        changed
    }
    /// Sees a whole valley.
    pub fn reveal_valley(&mut self, world: &World, id: ValleyId) {
        for y in 0..self.size {
            for x in 0..self.size {
                if world.valley_of((x, y)) == Some(id) {
                    self.seen[y * self.size + x] = 1.0;
                }
            }
        }
        self.dirty = true;
    }
}

/// Cells in open valleys (fog only lifts where the colony may go).
pub fn open_at(world: &World) -> impl Fn(usize, usize) -> bool + '_ {
    move |x, y| world.valley_of((x, y)).is_some_and(|v| world.is_open(v))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fog_lifts_softly_and_only_where_allowed() {
        let mut fog = Fog::unseen(16);
        assert_eq!(fog.explored_cells(), 0);
        let left = |x: usize, _y: usize| x < 8;
        assert!(fog.reveal((8.0, 8.0), 4.0, &left));
        assert!(fog.explored((6, 8)));
        assert_eq!(fog.get((10, 8)), 0.0, "the locked side stays fogged");
        assert!(!fog.reveal((8.0, 8.0), 4.0, &left), "a second reveal changes nothing");
    }
}
