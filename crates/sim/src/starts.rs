//! The three deterministic starts (owners: `start1`, `start23`). Built directly, never simulated
//! forward. Same (start, seed) gives an identical `SimState`.
//!
//! * `Start::River` (1): two adults + two kits arrive by a river; no lodge or dam; the player picks
//!   a dam spot; the family fells, gathers, builds; the pond fills.
//! * `Start::EarlyLake` (2): built dam and lake, a few elementary buildings, ~15 beavers (some
//!   joined, some born there).
//! * `Start::IndustrialLake` (3): larger dam, much larger lake, advanced tech, 50+ beavers with
//!   specialists, canals, efficient production, big storage.

use crate::SimState;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Start {
    River,
    EarlyLake,
    IndustrialLake,
}

impl Start {
    pub const ALL: [Start; 3] = [Start::River, Start::EarlyLake, Start::IndustrialLake];
    /// 1-based number used by `--start N`.
    pub fn number(self) -> u8 {
        self as u8 + 1
    }
    pub fn from_number(n: u8) -> Option<Start> {
        Start::ALL.get((n as usize).wrapping_sub(1)).copied()
    }
    /// Menu title and one-line pitch.
    pub fn title(self) -> (&'static str, &'static str) {
        match self {
            Start::River => ("A river and a family", "Four beavers, no home. Build the first dam."),
            Start::EarlyLake => ("The early lake", "A dam holds a young lake; fifteen beavers need work."),
            Start::IndustrialLake => ("The industrial lake", "Fifty beavers, canals and workshops. Keep it humming."),
        }
    }
}

/// Builds a start for `seed` with scoped features. (`start1`/`start23` fill in; the stub returns a
/// fresh world so callers compile.)
pub fn build(_start: Start, seed: u64) -> SimState {
    SimState::new(seed)
}
