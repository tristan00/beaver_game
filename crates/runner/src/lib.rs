use std::io::{BufWriter, Read, Write};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Goal {
    Wet,
    Beavers,
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

    pub fn score(self, start_wet: usize, s: &sim::SimState, speed: f64) -> f64 {
        match self {
            Goal::Wet => score(start_wet, s) as f64,
            Goal::Beavers => s.colony.beavers.len() as f64,
            Goal::Blend => blend(start_wet, s, speed),
        }
    }
}

pub fn wet_cells(depth: &sim::grid::WaterGrid) -> usize {
    depth.as_slice().iter().filter(|&&d| d >= sim::buildings::WET_DEPTH).count()
}

pub fn score(start_wet: usize, s: &sim::SimState) -> i64 {
    wet_cells(&s.water.depth) as i64 - start_wet as i64
}

pub fn blend(start_wet: usize, s: &sim::SimState, speed: f64) -> f64 {
    let water = wet_cells(&s.water.depth) as f64 / start_wet.max(1) as f64;
    let pace = (speed / DEFAULT_SPEED).min(1.0);
    (s.colony.beavers.len() as f64 * water * pace * 10.0).round() / 10.0
}

pub const SPEED_WINDOW_DAYS: f64 = 360.0;

pub const DEFAULT_SPEED: f64 = 6.0;
pub const DEFAULT_DAYS: f64 = 1800.0;

pub fn write_frame(w: &mut impl Write, bytes: &[u8]) -> std::io::Result<()> {
    w.write_all(&(bytes.len() as u32).to_le_bytes())?;
    w.write_all(bytes)?;
    w.flush()
}

pub fn read_frame(r: &mut impl Read) -> Option<Vec<u8>> {
    let mut len = [0u8; 4];
    r.read_exact(&mut len).ok()?;
    let mut bytes = vec![0u8; u32::from_le_bytes(len) as usize];
    r.read_exact(&mut bytes).ok()?;
    Some(bytes)
}

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

    pub fn next_state(&mut self) -> Option<sim::SimState> {
        let bytes = read_frame(&mut self.input)?;
        sim::save::decode(&bytes).ok().map(|(_, s)| s)
    }

    pub fn send(&mut self, c: &sim::Command) -> std::io::Result<()> {
        writeln!(self.output, "{}", sim::script::to_line(c))?;
        self.output.flush()
    }
}
