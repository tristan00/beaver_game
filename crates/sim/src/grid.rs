#[derive(Clone, Debug, PartialEq)]
pub struct Grid<T> {
    width: usize,
    height: usize,
    data: Vec<T>,
}

impl<T: Clone> Grid<T> {
    pub fn new(width: usize, height: usize, fill: T) -> Self {
        assert!(width > 0 && height > 0, "Grid dimensions must be non-zero");
        Self { width, height, data: vec![fill; width * height] }
    }
}

impl<T> Grid<T> {
    pub fn width(&self) -> usize { self.width }
    pub fn height(&self) -> usize { self.height }
    pub fn in_bounds(&self, x: usize, y: usize) -> bool { x < self.width && y < self.height }

    pub fn get(&self, x: usize, y: usize) -> Option<&T> {
        self.in_bounds(x, y).then(|| &self.data[y * self.width + x])
    }

    pub fn get_mut(&mut self, x: usize, y: usize) -> Option<&mut T> {
        if self.in_bounds(x, y) { Some(&mut self.data[y * self.width + x]) } else { None }
    }

    pub fn as_slice(&self) -> &[T] { &self.data }
    pub fn as_mut_slice(&mut self) -> &mut [T] { &mut self.data }
}

pub type HeightGrid = Grid<f32>;
pub type WaterGrid = Grid<f32>;
pub type VegetationGrid = Grid<f32>;

