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
    /// Beavers × wet cells over the start × the share of top speed the sim held (`blend`).
    Blend,
}

impl Goal {
    pub fn parse(s: &str) -> Option<Goal> {
        match s {
            "wet" => Some(Goal::Wet),
            "beavers" => Some(Goal::Beavers),
            "blend" => Some(Goal::Blend),
            _ => None,
        }
    }

    /// `speed`: game days per second the sim spent stepping over the run's last year.
    pub fn score(self, start_wet: usize, s: &sim::SimState, speed: f64) -> f64 {
        match self {
            Goal::Wet => score(start_wet, s) as f64,
            Goal::Beavers => s.colony.beavers.len() as f64,
            Goal::Blend => blend(start_wet, s, speed),
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

/// Beavers × (wet cells now / wet cells at the start) × min(1, speed / `DEFAULT_SPEED`),
/// to one decimal.
pub fn blend(start_wet: usize, s: &sim::SimState, speed: f64) -> f64 {
    let water = wet_cells(&s.water.depth) as f64 / start_wet.max(1) as f64;
    let pace = (speed / DEFAULT_SPEED).min(1.0);
    (s.colony.beavers.len() as f64 * water * pace * 10.0).round() / 10.0
}

/// The speed of the run's last `SPEED_WINDOW_DAYS` is what the blend scores.
pub const SPEED_WINDOW_DAYS: f64 = 360.0;

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

}
