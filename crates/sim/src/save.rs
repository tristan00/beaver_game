//! Saved games: the whole world, written so a game can be stopped and carried on later.
//!
//! A save is a short header (what, when, which map) and then the full simulation state, both
//! in postcard. It is written to a temporary file and renamed into place, so a crash while
//! saving never leaves a broken save. A save made by a build whose world layout differs will
//! not load: decoding has to use every byte exactly, and the header carries the format.
use crate::SimState;
use std::io::{Read, Write};
use std::path::Path;

/// Bumped by hand when the save layout changes in a way decoding would not catch.
pub const FORMAT: u32 = 10;
const MAGIC: [u8; 8] = *b"BEAVSAVE";

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Header {
    pub format: u32,
    /// The game version that wrote it.
    pub version: String,
    /// The map it was played on, as the interface names it.
    pub map: String,
    pub day: f64,
    pub population: usize,
    /// Seconds since 1970 when it was written.
    pub saved_at: u64,
}

fn header_for(sim: &SimState, map: &str) -> Header {
    Header {
        format: FORMAT,
        version: env!("CARGO_PKG_VERSION").to_string(),
        map: map.to_string(),
        day: sim.time_days,
        population: sim.colony.population(),
        saved_at: std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs()),
    }
}

/// The bytes of a save.
pub fn encode(sim: &SimState, map: &str) -> Result<Vec<u8>, String> {
    encode_with_header(sim, &header_for(sim, map))
}

fn encode_with_header(sim: &SimState, header: &Header) -> Result<Vec<u8>, String> {
    let head = postcard::to_stdvec(header).map_err(|e| format!("could not write the header: {e}"))?;
    let body = postcard::to_stdvec(sim).map_err(|e| format!("could not write the world: {e}"))?;
    let mut out = Vec::with_capacity(8 + 4 + head.len() + body.len());
    out.extend_from_slice(&MAGIC);
    out.extend_from_slice(&(head.len() as u32).to_le_bytes());
    out.extend_from_slice(&head);
    out.extend_from_slice(&body);
    Ok(out)
}

fn split(bytes: &[u8]) -> Result<(Header, &[u8]), String> {
    if bytes.len() < 12 || bytes[..8] != MAGIC {
        return Err("not a saved game".into());
    }
    let n = u32::from_le_bytes(bytes[8..12].try_into().unwrap()) as usize;
    let head = bytes.get(12..12 + n).ok_or("the save is cut short")?;
    let header: Header = postcard::from_bytes(head).map_err(|e| format!("unreadable header: {e}"))?;
    if header.format != FORMAT {
        return Err(format!("saved in format {} by version {}; this build reads format {FORMAT}", header.format, header.version));
    }
    Ok((header, &bytes[12 + n..]))
}

/// A world from save bytes, ready to run: shared views re-linked and a fresh instance.
pub fn decode(bytes: &[u8]) -> Result<(Header, SimState), String> {
    let (header, body) = split(bytes)?;
    let mut sim: SimState = postcard::from_bytes(body)
        .map_err(|e| format!("this save was made by a different build ({}) and cannot be read: {e}", header.version))?;
    // The weather is the place's, read from the world's seed.
    sim.water.sky = crate::weather::Sky::for_seed(sim.seed);
    sim.colony.sky = sim.water.sky;
    sim.instance = crate::next_instance();
    Ok((header, sim))
}

/// Write a save to `path`, through a temporary file renamed into place.
pub fn save(sim: &SimState, map: &str, path: &Path) -> Result<Header, String> {
    let header = header_for(sim, map);
    let bytes = encode_with_header(sim, &header)?;
    if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
        std::fs::create_dir_all(dir).map_err(|e| format!("could not make {}: {e}", dir.display()))?;
    }
    let tmp = path.with_extension("tmp");
    let mut f = std::fs::File::create(&tmp).map_err(|e| format!("could not write {}: {e}", tmp.display()))?;
    f.write_all(&bytes).and_then(|_| f.sync_all()).map_err(|e| format!("could not write {}: {e}", tmp.display()))?;
    drop(f);
    std::fs::rename(&tmp, path).map_err(|e| format!("could not replace {}: {e}", path.display()))?;
    Ok(header)
}

fn read(path: &Path) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    std::fs::File::open(path).and_then(|mut f| f.read_to_end(&mut bytes)).map_err(|e| format!("could not read {}: {e}", path.display()))?;
    Ok(bytes)
}

/// Read a save from `path`.
pub fn load(path: &Path) -> Result<(Header, SimState), String> {
    decode(&read(path)?)
}

/// Just the header of the save at `path`, to show what it holds.
pub fn peek(path: &Path) -> Result<Header, String> {
    let fail = |e: std::io::Error| format!("could not read {}: {e}", path.display());
    let mut f = std::fs::File::open(path).map_err(fail)?;
    let mut start = [0u8; 12];
    f.read_exact(&mut start).map_err(|_| "not a saved game".to_string())?;
    let n = u32::from_le_bytes(start[8..12].try_into().unwrap()) as usize;
    if start[..8] != MAGIC || n > 1 << 20 {
        return Err("not a saved game".into());
    }
    let mut bytes = start.to_vec();
    bytes.resize(12 + n, 0);
    f.read_exact(&mut bytes[12..]).map_err(fail)?;
    split(&bytes).map(|(h, _)| h)
}
