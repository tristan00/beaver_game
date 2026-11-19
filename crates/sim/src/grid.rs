use std::sync::atomic::{AtomicU64,Ordering};

static NEXT_GRID_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Debug)]
pub struct Grid<T> {
    width: usize,
    height: usize,
    data: Vec<T>,
    identity: u64,
    revision: u64,
}

impl<T:Clone> Clone for Grid<T> {
    fn clone(&self)->Self {
        Self{width:self.width,height:self.height,data:self.data.clone(),identity:NEXT_GRID_ID.fetch_add(1,Ordering::Relaxed),revision:self.revision}
    }
}

impl<T:PartialEq> PartialEq for Grid<T> {
    fn eq(&self,other:&Self)->bool {self.width==other.width && self.height==other.height && self.data==other.data}
}

impl<T: Clone> Grid<T> {
    pub fn new(width: usize, height: usize, fill: T) -> Self {
        assert!(width > 0 && height > 0, "Grid dimensions must be non-zero");
        Self { width, height, data: vec![fill; width * height], identity:NEXT_GRID_ID.fetch_add(1,Ordering::Relaxed), revision:0 }
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
        if !self.in_bounds(x,y) {return None;}
        self.revision=self.revision.wrapping_add(1);
        Some(&mut self.data[y*self.width+x])
    }

    pub fn as_slice(&self) -> &[T] { &self.data }
    pub fn as_mut_slice(&mut self) -> &mut [T] {
        self.revision=self.revision.wrapping_add(1);
        &mut self.data
    }
    pub(crate) fn version(&self)->(u64,u64) {(self.identity,self.revision)}
}

pub type HeightGrid = Grid<f32>;
pub type WaterGrid = Grid<f32>;
pub type VegetationGrid = Grid<f32>;

