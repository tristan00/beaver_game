use crate::water::Cell;
use crate::worldgen::{ValleyId, World};

pub const EXPLORED: f32 = 0.5;
pub const BEAVER_SIGHT: f32 = 30.0;
pub const ARRIVAL_SIGHT: f32 = 80.0;
pub const SETTLE_SIGHT: f32 = 40.0;

#[derive(Clone, Debug)]
pub struct Fog {
    pub size: usize,
    pub seen: Vec<f32>,
    pub dirty: bool,
}

impl Fog {
    pub fn unseen(size: usize) -> Fog {
        Fog { size, seen: vec![0.0; size * size], dirty: true }
    }
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

pub fn open_at(world: &World) -> impl Fn(usize, usize) -> bool + '_ {
    move |x, y| world.valley_of((x, y)).is_some_and(|v| world.is_open(v))
}

