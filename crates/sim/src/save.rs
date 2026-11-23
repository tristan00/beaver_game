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
pub const FORMAT: u32 = 3;
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
    let head = postcard::to_stdvec(&header_for(sim, map)).map_err(|e| format!("could not write the header: {e}"))?;
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
    let (mut sim, rest): (SimState, &[u8]) = postcard::take_from_bytes(body)
        .map_err(|e| format!("this save was made by a different build ({}) and cannot be read: {e}", header.version))?;
    if !rest.is_empty() {
        return Err(format!("this save was made by a different build ({}) and cannot be read", header.version));
    }
    sim.work.inventory = sim.buildings.stores.inventory.view();
    sim.instance = crate::next_instance();
    Ok((header, sim))
}

/// Write a save to `path`, through a temporary file renamed into place.
pub fn save(sim: &SimState, map: &str, path: &Path) -> Result<Header, String> {
    let bytes = encode(sim, map)?;
    if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
        std::fs::create_dir_all(dir).map_err(|e| format!("could not make {}: {e}", dir.display()))?;
    }
    let tmp = path.with_extension("tmp");
    let mut f = std::fs::File::create(&tmp).map_err(|e| format!("could not write {}: {e}", tmp.display()))?;
    f.write_all(&bytes).and_then(|_| f.sync_all()).map_err(|e| format!("could not write {}: {e}", tmp.display()))?;
    drop(f);
    std::fs::rename(&tmp, path).map_err(|e| format!("could not replace {}: {e}", path.display()))?;
    Ok(header_for(sim, map))
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

#[cfg(test)]
mod tests {
    use super::*;

    /// A game saved part-way and loaded again carries on exactly as if it had never stopped.
    #[test]
    fn a_saved_game_resumes_exactly() {
        let (mut a, site) = crate::starts::new_game_on_map("telemark").unwrap();
        crate::starts::settle(&mut a, site.camp).unwrap();
        let _ = crate::construction::place_best(&mut a, crate::buildings::BuildingKind::Shelter, site.camp, &[]);
        let _ = crate::construction::place_best(&mut a, crate::buildings::BuildingKind::Larder, site.camp, &[]);
        for _ in 0..(12 * 96) {
            a.step(crate::STEP_DAYS).unwrap();
        }
        let bytes = encode(&a, "telemark").unwrap();
        let (header, mut b) = decode(&bytes).unwrap();
        assert_eq!((header.map.as_str(), header.day), ("telemark", a.time_days));
        assert_ne!(a.instance, b.instance, "a loaded world is a new instance");
        for _ in 0..(8 * 96) {
            a.step(crate::STEP_DAYS).unwrap();
            b.step(crate::STEP_DAYS).unwrap();
        }
        assert_eq!(a.time_days, b.time_days);
        assert_eq!(a.colony.population(), b.colony.population());
        let agents = |s: &SimState| s.agents.by_id.values().map(|ag| (ag.id, ag.cell, format!("{:?}", ag.task))).collect::<Vec<_>>();
        assert_eq!(agents(&a), agents(&b), "every beaver is where it would have been");
        assert_eq!(a.water.depth.as_slice(), b.water.depth.as_slice(), "and so is every drop of water");
        assert_eq!(a.resources.items(), b.resources.items());
        for g in crate::buildings::Good::ALL {
            assert_eq!(a.buildings.stores.amount(g), b.buildings.stores.amount(g));
        }
        // The loaded world's stores and its work planner share one stock, as a fresh one's do.
        let b2 = b.buildings.buildings.iter().find(|x| crate::agents::is_store(x.kind)).map(|x| x.id);
        if let Some(id) = b2 {
            b.buildings.stores.put_at(id, crate::buildings::Good::Food, 1.0).unwrap();
            assert_eq!(b.work.stock_of(id, crate::buildings::Good::Food), b.buildings.stores.inventory.at(id, crate::buildings::Good::Food));
        }
    }

    #[test]
    fn broken_or_foreign_files_are_refused() {
        let (a, _) = crate::starts::new_game_on_map("telemark").unwrap();
        let bytes = encode(&a, "telemark").unwrap();
        assert!(decode(&bytes[..bytes.len() / 2]).is_err(), "a cut-short save");
        assert!(decode(b"not a save at all").is_err());
        let dir = std::env::temp_dir().join(format!("beaver_save_test_{}", std::process::id()));
        let path = dir.join("t.sav");
        save(&a, "telemark", &path).unwrap();
        assert_eq!(peek(&path).unwrap().map, "telemark", "the header reads on its own");
        assert_eq!(load(&path).unwrap().1.time_days, a.time_days);
        std::fs::remove_dir_all(&dir).unwrap();
        let mut extra = bytes.clone();
        extra.push(0);
        assert!(decode(&extra).is_err(), "trailing bytes mean a different layout");
    }
}
