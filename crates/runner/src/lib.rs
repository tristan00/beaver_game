//! Playing the colony from a separate process.
//!
//! The `runner` binary runs a map at a fixed game speed and starts a policy program beside it.
//! The game never waits for the policy: the policy is sent the latest state whenever it is
//! ready for one, and whatever it sends back is carried out at the next step. A slow policy
//! acts late; the clock does not stop for it.
//!
//! The policy reads states from its stdin and writes commands to its stdout:
//! - runner to policy: frames of a little-endian `u32` length followed by a save
//!   (`sim::save::encode`), read with [`Policy::next_state`];
//! - policy to runner: one command per line in the script format (`sim::script::to_line`),
//!   written with [`Policy::send`]. Blank lines and lines starting with `#` are ignored.
//!
//! What the policy writes to stderr goes to the runner's log.
use std::io::{BufWriter, Read, Write};

/// What a run is scored on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Goal {
    /// Wet cells gained since the colony settled (`score`).
    Wet,
    /// Beavers alive at the end.
    Beavers,
}

impl Goal {
    pub fn parse(s: &str) -> Option<Goal> {
        match s {
            "wet" => Some(Goal::Wet),
            "beavers" => Some(Goal::Beavers),
            _ => None,
        }
    }

    pub fn score(self, start_wet: usize, s: &sim::SimState) -> i64 {
        match self {
            Goal::Wet => score(start_wet, s),
            Goal::Beavers => s.colony.beavers.len() as i64,
        }
    }
}

/// Cells of the map with water at least `sim::buildings::WET_DEPTH` (5 cm) deep.
pub fn wet_cells(depth: &sim::grid::WaterGrid) -> usize {
    depth.as_slice().iter().filter(|&&d| d >= sim::buildings::WET_DEPTH).count()
}

/// The score: wet cells at the end minus wet cells when the colony settled. It can be negative.
pub fn score(start_wet: usize, s: &sim::SimState) -> i64 {
    wet_cells(&s.water.depth) as i64 - start_wet as i64
}

/// Game days per real second: the game's fastest speed.
pub const DEFAULT_SPEED: f64 = 6.0;
/// Five years of 360 days: the score is taken at the end of year 5.
pub const DEFAULT_DAYS: f64 = 1800.0;

pub fn write_frame(w: &mut impl Write, bytes: &[u8]) -> std::io::Result<()> {
    w.write_all(&(bytes.len() as u32).to_le_bytes())?;
    w.write_all(bytes)?;
    w.flush()
}

/// The next frame, or None once the other side has closed.
pub fn read_frame(r: &mut impl Read) -> Option<Vec<u8>> {
    let mut len = [0u8; 4];
    r.read_exact(&mut len).ok()?;
    let mut bytes = vec![0u8; u32::from_le_bytes(len) as usize];
    r.read_exact(&mut bytes).ok()?;
    Some(bytes)
}

/// The policy's end of the connection.
pub struct Policy {
    input: std::io::StdinLock<'static>,
    output: BufWriter<std::io::Stdout>,
}

impl Default for Policy {
    fn default() -> Self {
        Self::new()
    }
}

impl Policy {
    pub fn new() -> Self {
        Policy { input: std::io::stdin().lock(), output: BufWriter::new(std::io::stdout()) }
    }

    /// Wait for the latest state. None when the run is over.
    pub fn next_state(&mut self) -> Option<sim::SimState> {
        let bytes = read_frame(&mut self.input)?;
        sim::save::decode(&bytes).ok().map(|(_, s)| s)
    }

    /// Send a command; it is carried out at the next step after it arrives.
    pub fn send(&mut self, c: &sim::Command) -> std::io::Result<()> {
        writeln!(self.output, "{}", sim::script::to_line(c))?;
        self.output.flush()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frames_round_trip_and_end_cleanly() {
        let mut buf = Vec::new();
        write_frame(&mut buf, b"abc").unwrap();
        write_frame(&mut buf, b"").unwrap();
        let mut r = buf.as_slice();
        assert_eq!(read_frame(&mut r).as_deref(), Some(&b"abc"[..]));
        assert_eq!(read_frame(&mut r).as_deref(), Some(&b""[..]));
        assert_eq!(read_frame(&mut r), None);
    }

    #[test]
    fn the_score_is_wet_cells_gained() {
        let (mut s, site) = sim::starts::new_game_on_map("algonquin").unwrap();
        s.apply(sim::Command::Settle { cell: site.camp }).unwrap();
        let start = wet_cells(&s.water.depth);
        assert!(start > 0, "the map has rivers");
        assert_eq!(score(start, &s), 0);
        let n = s.water.depth.as_slice().len();
        let dry: Vec<usize> = (0..n).filter(|&i| s.water.depth.as_slice()[i] < sim::buildings::WET_DEPTH).take(3).collect();
        let w = s.water.depth.width();
        for &i in &dry {
            *s.water.depth.get_mut(i % w, i / w).unwrap() = sim::buildings::WET_DEPTH;
        }
        assert_eq!(score(start, &s), 3, "a cell exactly 5 cm deep is wet");
    }

    #[test]
    fn a_sent_state_decodes_to_the_same_day() {
        let (mut s, site) = sim::starts::new_game_on_map("algonquin").unwrap();
        s.apply(sim::Command::Settle { cell: site.camp }).unwrap();
        let mut buf = Vec::new();
        write_frame(&mut buf, &sim::save::encode(&s, "algonquin").unwrap()).unwrap();
        let (_, back) = sim::save::decode(&read_frame(&mut buf.as_slice()).unwrap()).unwrap();
        assert_eq!((back.time_days, back.colony.beavers.len()), (s.time_days, s.colony.beavers.len()));
    }
}
