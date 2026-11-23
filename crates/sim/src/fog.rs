include!(concat!(env!("OUT_DIR"), "/fog_constants.rs"));
use crate::water::Cell;
use crate::worldgen::World;

pub const EXPLORED: f32 = 0.5;

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct Fog {
    pub size: usize,
    pub seen: Vec<f32>,
    pub dirty: bool,
    /// The cell each beaver last looked around from. Sight only ever clears fog, so looking
    /// again from the same cell changes nothing and is skipped.
    #[serde(skip)]
    sighted: std::collections::HashMap<crate::colony::BeaverId, Cell>,
}

crate::grid::clone_fields!(Fog { size, seen, dirty, sighted });

impl Fog {
    pub fn unseen(size: usize) -> Fog {
        Fog { size, seen: vec![0.0; size * size], dirty: true, sighted: Default::default() }
    }
    pub fn clear(size: usize) -> Fog {
        Fog { size, seen: vec![1.0; size * size], dirty: true, sighted: Default::default() }
    }
    pub fn get(&self, c: Cell) -> f32 {
        if c.0 < self.size && c.1 < self.size { self.seen[c.1 * self.size + c.0] } else { 0.0 }
    }
    pub fn explored(&self, c: Cell) -> bool {
        self.get(c) >= EXPLORED
    }
    /// What a beaver at `cell` sees (`BEAVER_SIGHT` around the cell's centre).
    pub fn sight(&mut self, who: crate::colony::BeaverId, cell: Cell, allowed: &dyn Fn(usize, usize) -> bool) {
        if self.sighted.get(&who) == Some(&cell) {
            return;
        }
        self.reveal((cell.0 as f32 + 0.5, cell.1 as f32 + 0.5), BEAVER_SIGHT, allowed);
        self.sighted.insert(who, cell);
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
}

pub fn open_at(world: &World) -> impl Fn(usize, usize) -> bool + '_ {
    move |x, y| world.height.in_bounds(x, y)
}

