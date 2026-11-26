use std::sync::atomic::{AtomicU64,Ordering};

static NEXT_GRID_ID: AtomicU64 = AtomicU64::new(1);

fn next_identity() -> u64 {
    NEXT_GRID_ID.fetch_add(1, Ordering::Relaxed)
}

/// A grid read from a save keeps the identity it was saved with (cached versions of it are
/// saved too). New grids are numbered after it, so none ever shares an identity with it.
fn loaded_identity<'de, D: serde::Deserializer<'de>>(d: D) -> Result<u64, D::Error> {
    let identity = <u64 as serde::Deserialize>::deserialize(d)?;
    NEXT_GRID_ID.fetch_max(identity.saturating_add(1), Ordering::Relaxed);
    Ok(identity)
}

/// A grid's cells are grouped into square tiles this many cells on a side, and the grid
/// remembers when each tile was last written (`tile_stamp`). Whoever draws the world reads
/// those to find the parts that changed, instead of comparing every cell.
pub const TILE: usize = 16;
/// A write to a cell also stamps any tile within this many cells of it: what is drawn of a
/// tile reads a little beyond its edge.
pub const REACH: usize = 4;

/// Stamp the tile of cell (x, y) and any other within `REACH` cells of it.
#[inline]
fn stamp_cell(tiles: &mut [u64], cols: usize, x: usize, y: usize, stamp: u64) {
    let rows = tiles.len() / cols;
    for ty in y.saturating_sub(REACH) / TILE..=((y + REACH) / TILE).min(rows - 1) {
        for tx in x.saturating_sub(REACH) / TILE..=((x + REACH) / TILE).min(cols - 1) {
            tiles[ty * cols + tx] = stamp;
        }
    }
}

/// The tiles holding, or within `REACH` of, cells written since they were last looked at: a
/// word of bits for each row of tiles. (Past 64 tiles across, a tile always counts.)
#[derive(Debug, Default)]
pub(crate) struct Due {
    rows: Vec<u64>,
}

impl Due {
    /// Make room for a grid `height` cells high; a new list starts with every tile due.
    pub(crate) fn sized(&mut self, height: usize) {
        if self.rows.len() != height.div_ceil(TILE) {
            self.rows = vec![u64::MAX; height.div_ceil(TILE)];
        }
    }

    /// The tiles across that reach cells `a..b` of a row, as bits.
    #[inline]
    pub(crate) fn across(a: usize, b: usize) -> u64 {
        let (ta, tb) = ((a.saturating_sub(REACH) / TILE).min(63), ((b - 1 + REACH) / TILE).min(63));
        (u64::MAX >> (63 - tb)) & (u64::MAX << ta)
    }

    /// Cells of row `y` were written, in the tiles `across` gave.
    #[inline]
    pub(crate) fn row(&mut self, y: usize, across: u64) {
        for ty in y.saturating_sub(REACH) / TILE..=((y + REACH) / TILE).min(self.rows.len() - 1) {
            self.rows[ty] |= across;
        }
    }

    pub(crate) fn all(&mut self) {
        self.rows.fill(u64::MAX);
    }

    fn is(&self, tx: usize, ty: usize) -> bool {
        tx > 63 || self.rows[ty] >> tx & 1 == 1
    }

    pub(crate) fn clear(&mut self) {
        self.rows.fill(0);
    }
}

/// The side of the square of cells a tile reads: its own, and those within `REACH`.
const WINDOW: usize = TILE + 2 * REACH;

/// For a grid that is drawn only so finely: the cells each tile reads, as they were when the
/// tile was last stamped. `note` stamps a tile again once one of them has moved further than
/// is drawn. A value creeping the same way over the whole map is then reported once for each
/// step of what is drawn, where comparing each cell with its last value would report every
/// tile at every change.
#[derive(Debug)]
pub(crate) struct Told<T> {
    cells: Vec<T>,
}

impl<T> Default for Told<T> {
    fn default() -> Self { Told { cells: Vec::new() } }
}

impl<T: Copy> Told<T> {
    /// Look at the tiles that are `due` (or at all of them), and stamp those in which a cell
    /// is now `far` from what the tile last told of it.
    pub(crate) fn note(&mut self, grid: &mut Grid<T>, due: Option<&Due>, far: impl Fn(T, T) -> bool) {
        let (w, h) = (grid.width, grid.height);
        let (cols, rows) = (w.div_ceil(TILE), h.div_ceil(TILE));
        let (cells, mut marks) = grid.cells_and_marks();
        let first = self.cells.len() != cols * rows * WINDOW * WINDOW;
        if first {
            // Nothing told yet (a new world, or a copy of one): all of it is news.
            let Some(&any) = cells.first() else { return };
            self.cells = vec![any; cols * rows * WINDOW * WINDOW];
            marks.all();
        }
        for ty in 0..rows {
            let ys = (ty * TILE).saturating_sub(REACH)..((ty + 1) * TILE + REACH).min(h);
            for tx in 0..cols {
                if !first && due.is_some_and(|d| !d.is(tx, ty)) {
                    continue;
                }
                let (x0, x1) = ((tx * TILE).saturating_sub(REACH), ((tx + 1) * TILE + REACH).min(w));
                let told = &mut self.cells[(ty * cols + tx) * WINDOW * WINDOW..][..WINDOW * WINDOW];
                let moved = first || ys.clone().enumerate().any(|(k, y)| {
                    cells[y * w + x0..y * w + x1].iter().zip(&told[k * WINDOW..]).fold(false, |m, (a, b)| m | far(*a, *b))
                });
                if moved {
                    if !first {
                        marks.tile(tx, ty);
                    }
                    for (k, y) in ys.clone().enumerate() {
                        told[k * WINDOW..k * WINDOW + x1 - x0].copy_from_slice(&cells[y * w + x0..y * w + x1]);
                    }
                }
            }
        }
    }
}

/// Working memory of the simulation's own, which a copy of the world does without: a copy
/// starts with none (and makes its own when it needs to), and copying over one leaves it be.
#[derive(Debug, Default)]
pub(crate) struct Own<T>(pub(crate) T);

impl<T: Default> Clone for Own<T> {
    fn clone(&self) -> Self { Own(T::default()) }
    fn clone_from(&mut self, _: &Self) {}
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct Grid<T> {
    width: usize,
    height: usize,
    data: Vec<T>,
    #[serde(deserialize_with = "loaded_identity")]
    identity: u64,
    revision: u64,
    /// The grid (and its revision) this one was last copied from. While that grid and this
    /// copy are both unchanged since, copying it here again has nothing to do.
    #[serde(skip)]
    copied: (u64, u64),
    /// The revision at which any cell may last have been written (through `as_mut_slice`).
    #[serde(skip)]
    swept: u64,
    /// The revision at which each tile was last stamped; empty until one is. A tile is
    /// stamped when a cell it reads is written, or -- for the few grids that are drawn only so
    /// finely, which say so -- when one has moved further than is drawn (`Told`).
    #[serde(skip)]
    tiles: Vec<u64>,
}

/// Says which cells of a grid were written, for `Grid::cells_and_marks`.
pub(crate) struct Marks<'a> {
    tiles: &'a mut [u64],
    swept: &'a mut u64,
    cols: usize,
    stamp: u64,
}

impl Marks<'_> {
    /// Cell (x, y) took a new value.
    #[inline]
    pub(crate) fn at(&mut self, x: usize, y: usize) {
        stamp_cell(self.tiles, self.cols, x, y, self.stamp);
    }

    /// Any cell may have.
    pub(crate) fn all(&mut self) {
        *self.swept = self.stamp;
    }

    /// Tile (tx, ty) is to be drawn again, whichever of the cells it reads moved.
    pub(crate) fn tile(&mut self, tx: usize, ty: usize) {
        self.tiles[ty * self.cols + tx] = self.stamp;
    }
}

/// An empty grid, for a field filled in when first needed. It has no cells to tell apart
/// from another's, and takes no identity.
impl<T> Default for Grid<T> {
    fn default() -> Self {
        Self { width: 0, height: 0, data: Vec::new(), identity: 0, revision: 0, copied: (0, 0), swept: 0, tiles: Vec::new() }
    }
}

impl<T:Clone> Clone for Grid<T> {
    fn clone(&self)->Self {
        Self{width:self.width,height:self.height,data:self.data.clone(),identity:next_identity(),revision:self.revision,copied:self.version(),swept:self.swept,tiles:self.tiles.clone()}
    }
    /// As `clone`, into this grid's buffer (a copy is a new grid, so it takes a new identity).
    /// Most of a world's grids do not change between two snapshots: one that is still the
    /// copy made last time, of a grid not written since, is left as it is.
    fn clone_from(&mut self,src:&Self) {
        if self.copied==src.version() && self.revision==src.revision && self.data.len()==src.data.len() {
            return;
        }
        self.width=src.width;
        self.height=src.height;
        self.data.clone_from(&src.data);
        self.identity=next_identity();
        self.revision=src.revision;
        self.copied=src.version();
        self.swept=src.swept;
        self.tiles.clone_from(&src.tiles);
    }
}

/// `Clone` for a struct, field by field, with a `clone_from` that clones each field into the
/// one it replaces, so big buffers are copied into memory already held instead of newly
/// allocated (the simulation's snapshots reuse the previous one this way). Every field must be
/// listed: the destructuring fails to compile when one is added and left out.
macro_rules! clone_fields {
    ($t:ident { $($f:ident),* $(,)? }) => {
        impl Clone for $t {
            fn clone(&self) -> Self {
                let $t { $($f),* } = self;
                $t { $($f: $f.clone()),* }
            }
            fn clone_from(&mut self, src: &Self) {
                let $t { $($f),* } = src;
                $( self.$f.clone_from($f); )*
            }
        }
    };
}
pub(crate) use clone_fields;

impl<T:PartialEq> PartialEq for Grid<T> {
    fn eq(&self,other:&Self)->bool {self.width==other.width && self.height==other.height && self.data==other.data}
}

impl<T: Clone> Grid<T> {
    pub fn new(width: usize, height: usize, fill: T) -> Self {
        assert!(width > 0 && height > 0, "Grid dimensions must be non-zero");
        Self { width, height, data: vec![fill; width * height], identity:next_identity(), revision:0, copied:(0,0), swept:0, tiles:Vec::new() }
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
        let (stamp, cols) = (self.revision, self.tile_cols());
        stamp_cell(self.tile_stamps(), cols, x, y, stamp);
        Some(&mut self.data[y*self.width+x])
    }

    pub fn as_slice(&self) -> &[T] { &self.data }
    /// Every cell, to write to. The grid cannot tell which are written, so all of its tiles
    /// count as changed: to change a few cells, index the grid (`grid[i] = v`) or use
    /// `get_mut` or `cells_and_marks`, which note only the tiles they change.
    pub fn as_mut_slice(&mut self) -> &mut [T] {
        self.revision=self.revision.wrapping_add(1);
        self.swept=self.revision;
        &mut self.data
    }
    pub(crate) fn version(&self)->(u64,u64) {(self.identity,self.revision)}

    fn tile_cols(&self) -> usize { self.width.div_ceil(TILE) }

    fn tile_stamps(&mut self) -> &mut [u64] {
        if self.tiles.is_empty() {
            self.tiles = vec![0; self.tile_cols() * self.height.div_ceil(TILE)];
        }
        &mut self.tiles
    }

    /// Every cell, with the means to say which were written: for a sweep that visits many
    /// cells and changes few. A cell written without telling `Marks` is not seen as changed.
    pub(crate) fn cells_and_marks(&mut self) -> (&mut [T], Marks<'_>) {
        self.revision = self.revision.wrapping_add(1);
        let (stamp, cols) = (self.revision, self.tile_cols());
        self.tile_stamps();
        (&mut self.data, Marks { tiles: &mut self.tiles, swept: &mut self.swept, cols, stamp })
    }

    /// Which grid these cells belong to: the grid this one is an untouched copy of, or
    /// itself. Stamps of grids with the same origin can be compared.
    pub fn origin(&self) -> u64 {
        if self.copied.0 != 0 && self.copied.1 == self.revision { self.copied.0 } else { self.identity }
    }

    /// Changes whenever any cell may have changed (within one `origin`, it only grows).
    pub fn stamp(&self) -> (u64, u64) { (self.origin(), self.revision) }

    /// When tile (tx, ty) -- cells tx * TILE.. across, ty * TILE.. down -- or a cell within
    /// `REACH` of it was last written. Within one `origin` it only grows, and it changes
    /// whenever a cell there may have.
    pub fn tile_stamp(&self, tx: usize, ty: usize) -> u64 {
        self.swept.max(self.tiles.get(ty * self.tile_cols() + tx).copied().unwrap_or(0))
    }

}

impl<T> std::ops::Index<usize> for Grid<T> {
    type Output = T;
    fn index(&self, i: usize) -> &T { &self.data[i] }
}

/// Writing one cell by its index notes its tile as changed.
impl<T> std::ops::IndexMut<usize> for Grid<T> {
    fn index_mut(&mut self, i: usize) -> &mut T {
        self.revision = self.revision.wrapping_add(1);
        let (stamp, cols, width) = (self.revision, self.tile_cols(), self.width);
        stamp_cell(self.tile_stamps(), cols, i % width, i / width, stamp);
        &mut self.data[i]
    }
}

pub type HeightGrid = Grid<f32>;
pub type WaterGrid = Grid<f32>;
pub type VegetationGrid = Grid<f32>;

