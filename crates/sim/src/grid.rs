//! Shared 2D grid container and the world's base field aliases.

/// Dense row-major grid. Invariant: `data.len() == width * height`.
#[derive(Clone, Debug, PartialEq)]
pub struct Grid<T> {
    width: usize,
    height: usize,
    data: Vec<T>,
}

impl<T: Clone> Grid<T> {
    /// Creates a grid filled with `fill`. Panics if either dimension is zero.
    pub fn new(width: usize, height: usize, fill: T) -> Self {
        assert!(width > 0 && height > 0, "Grid dimensions must be non-zero");
        Self { width, height, data: vec![fill; width * height] }
    }
}

impl<T> Grid<T> {
    pub fn width(&self) -> usize { self.width }
    pub fn height(&self) -> usize { self.height }
    pub fn in_bounds(&self, x: usize, y: usize) -> bool { x < self.width && y < self.height }

    /// Returns the cell at (x, y), or `None` if out of bounds.
    pub fn get(&self, x: usize, y: usize) -> Option<&T> {
        self.in_bounds(x, y).then(|| &self.data[y * self.width + x])
    }

    /// Mutable access to the cell at (x, y), or `None` if out of bounds.
    pub fn get_mut(&mut self, x: usize, y: usize) -> Option<&mut T> {
        if self.in_bounds(x, y) { Some(&mut self.data[y * self.width + x]) } else { None }
    }

    /// Row-major cell slice.
    pub fn as_slice(&self) -> &[T] { &self.data }
    pub fn as_mut_slice(&mut self) -> &mut [T] { &mut self.data }
}

/// Terrain height in world units.
pub type HeightGrid = Grid<f32>;
/// Water depth above terrain (0 = dry).
pub type WaterGrid = Grid<f32>;
/// Vegetation density in 0..=1.
pub type VegetationGrid = Grid<f32>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounds_checked_access() {
        let mut g = Grid::new(3, 2, 0u8);
        *g.get_mut(2, 1).unwrap() = 7;
        assert_eq!(g.get(2, 1), Some(&7));
        assert_eq!(g.get(3, 0), None);
        assert_eq!(g.get(0, 2), None);
    }

    #[test]
    #[should_panic]
    fn zero_size_rejected() { Grid::new(0, 4, 0u8); }
}
