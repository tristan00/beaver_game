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
pub const FORMAT: u32 = 4;
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
/// Settings added after the save format was fixed, written after the world. A save that ends
/// at the world is from before them and loads with their defaults.
#[derive(Default, serde::Serialize, serde::Deserialize)]
struct Extras {
    shore_limits: std::collections::BTreeMap<crate::jobs::RoleKind, u16>,
}

/// Added after `Extras`, written after it: bodies and beavers walking off the map.
#[derive(Default, serde::Serialize, serde::Deserialize)]
struct Extras2 {
    remains: crate::agents::Remains,
}

/// Added after `Extras2`, written after it: gates upgraded into locks.
#[derive(Default, serde::Serialize, serde::Deserialize)]
struct Extras3 {
    locks: crate::locks::Locks,
}

/// Added after `Extras3`, written after it: the harbors' barges.
#[derive(Default, serde::Serialize, serde::Deserialize)]
struct Extras4 {
    fleet: crate::harbor::Fleet,
}

/// Added after `Extras4`, written after it: wild animals.
#[derive(serde::Serialize, serde::Deserialize)]
struct Extras5 {
    wildlife: crate::wildlife::Wildlife,
}

/// Added after `Extras6`, written after it: every beaver's life story.
#[derive(Default, serde::Serialize, serde::Deserialize)]
struct Extras7 {
    lives: crate::lives::Lives,
}

/// Added after `Extras8`, written after it: each beaver's own look.
#[derive(Default, serde::Serialize, serde::Deserialize)]
struct Extras9 {
    looks: std::collections::BTreeMap<crate::colony::BeaverId, crate::looks::Looks>,
}

/// Added after `Extras7`, written after it: the source each gatherer is working.
#[derive(Default, serde::Serialize, serde::Deserialize)]
struct Extras8 {
    sources: std::collections::BTreeMap<crate::colony::BeaverId, crate::resources::ResourceId>,
}

/// Added after `Extras5`, written after it: which beaver planted which tree.
#[derive(Default, serde::Serialize, serde::Deserialize)]
struct Extras6 {
    planted: std::collections::BTreeMap<crate::resources::ResourceId, (crate::colony::BeaverId, f64)>,
}

pub fn encode(sim: &SimState, map: &str) -> Result<Vec<u8>, String> {
    let head = postcard::to_stdvec(&header_for(sim, map)).map_err(|e| format!("could not write the header: {e}"))?;
    let mut body = postcard::to_stdvec(sim).map_err(|e| format!("could not write the world: {e}"))?;
    let extras = Extras { shore_limits: sim.work.shore_limits.clone() };
    body.extend(postcard::to_stdvec(&extras).map_err(|e| format!("could not write the settings: {e}"))?);
    let extras2 = Extras2 { remains: sim.agents.remains.clone() };
    body.extend(postcard::to_stdvec(&extras2).map_err(|e| format!("could not write the settings: {e}"))?);
    let extras3 = Extras3 { locks: sim.locks.clone() };
    body.extend(postcard::to_stdvec(&extras3).map_err(|e| format!("could not write the settings: {e}"))?);
    let extras4 = Extras4 { fleet: sim.fleet.clone() };
    body.extend(postcard::to_stdvec(&extras4).map_err(|e| format!("could not write the settings: {e}"))?);
    let extras5 = Extras5 { wildlife: sim.wildlife.clone() };
    body.extend(postcard::to_stdvec(&extras5).map_err(|e| format!("could not write the settings: {e}"))?);
    let extras6 = Extras6 { planted: sim.forestry.planted.clone() };
    body.extend(postcard::to_stdvec(&extras6).map_err(|e| format!("could not write the settings: {e}"))?);
    let extras7 = Extras7 { lives: sim.lives.clone() };
    body.extend(postcard::to_stdvec(&extras7).map_err(|e| format!("could not write the settings: {e}"))?);
    let extras8 = Extras8 { sources: sim.agents.by_id.iter().filter_map(|(id, a)| Some((*id, a.source?))).collect() };
    body.extend(postcard::to_stdvec(&extras8).map_err(|e| format!("could not write the settings: {e}"))?);
    let extras9 = Extras9 { looks: sim.colony.looks.clone() };
    body.extend(postcard::to_stdvec(&extras9).map_err(|e| format!("could not write the settings: {e}"))?);
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
    let bad = || format!("this save was made by a different build ({}) and cannot be read", header.version);
    let (extras, rest) = if rest.is_empty() { (Extras::default(), rest) } else { postcard::take_from_bytes::<Extras>(rest).map_err(|_| bad())? };
    let (extras2, rest) = if rest.is_empty() { (Extras2::default(), rest) } else { postcard::take_from_bytes::<Extras2>(rest).map_err(|_| bad())? };
    let (extras3, rest) = if rest.is_empty() { (Extras3::default(), rest) } else { postcard::take_from_bytes::<Extras3>(rest).map_err(|_| bad())? };
    let (extras4, rest) = if rest.is_empty() { (Extras4::default(), rest) } else { postcard::take_from_bytes::<Extras4>(rest).map_err(|_| bad())? };
    // A save from before the wild animals starts with none; they wander in.
    let (wildlife, rest) = match rest {
        [] => (crate::wildlife::Wildlife::new(sim.seed), rest),
        _ => postcard::take_from_bytes::<Extras5>(rest).map(|(e, r)| (e.wildlife, r)).map_err(|_| bad())?,
    };
    let (extras6, rest) = if rest.is_empty() { (Extras6::default(), rest) } else { postcard::take_from_bytes::<Extras6>(rest).map_err(|_| bad())? };
    let (extras7, rest) = if rest.is_empty() { (Extras7::default(), rest) } else { postcard::take_from_bytes::<Extras7>(rest).map_err(|_| bad())? };
    let (extras8, rest) = if rest.is_empty() { (Extras8::default(), rest) } else { postcard::take_from_bytes::<Extras8>(rest).map_err(|_| bad())? };
    let extras9 = match rest {
        [] => Extras9::default(),
        _ => match postcard::take_from_bytes::<Extras9>(rest) {
            Ok((e, [])) => e,
            _ => return Err(bad()),
        },
    };
    sim.work.shore_limits = extras.shore_limits;
    sim.agents.remains = extras2.remains;
    sim.locks = extras3.locks;
    sim.fleet = extras4.fleet;
    sim.wildlife = wildlife;
    sim.forestry.planted = extras6.planted;
    sim.lives = extras7.lives;
    sim.colony.looks = extras9.looks;
    // A save from before looks were kept: each beaver gets one now, and keeps it.
    sim.colony.fill_looks();
    for (id, source) in extras8.sources {
        if let Some(a) = sim.agents.by_id.get_mut(&id) {
            a.source = Some(source);
        }
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

    #[test]
    fn settings_ride_after_the_world_and_older_saves_load_without_them() {
        let (mut a, _) = crate::starts::new_game_on_map("telemark").unwrap();
        let plain = encode(&a, "telemark").unwrap();
        a.apply(crate::Command::SetShoreLimit { role: crate::jobs::RoleKind::Logger, cells: Some(10) }).unwrap();
        let (_, b) = decode(&encode(&a, "telemark").unwrap()).unwrap();
        assert_eq!(b.work.shore_limits.get(&crate::jobs::RoleKind::Logger), Some(&10));
        // Older saves end before the locks (one empty map), before the bodies (two empty
        // lists), or before the settings too.
        // The wild animals come last; then the barges (an empty list and a counter, two bytes).
        // The life stories come last (an empty map and a day, nine bytes), then who planted
        // what (an empty map, one byte), then the wild animals.
        // Last of all, the beavers' looks, then the gatherers' sources (an empty map, one byte).
        let looks = postcard::to_stdvec(&Extras9 { looks: a.colony.looks.clone() }).unwrap().len();
        let plain = &plain[..plain.len() - looks];
        let before_looks = decode(plain).unwrap().1;
        assert_eq!(before_looks.colony.looks.len(), a.colony.looks.len(), "a save from before the looks gives every beaver one");
        let plain = &plain[..plain.len() - 1];
        assert!(decode(plain).unwrap().1.agents.by_id.values().all(|a| a.source.is_none()), "a save from before the gatherers' sources loads");
        let lives = postcard::to_stdvec(&Extras7 { lives: a.lives.clone() }).unwrap().len();
        let plain = &plain[..plain.len() - lives];
        assert!(decode(plain).unwrap().1.lives.of.is_empty(), "a save from before the life stories loads");
        let plain = &plain[..plain.len() - 1];
        assert!(decode(plain).unwrap().1.forestry.planted.is_empty(), "a save from before the planting record loads");
        let wild = postcard::to_stdvec(&Extras5 { wildlife: a.wildlife.clone() }).unwrap().len();
        let plain = &plain[..plain.len() - wild];
        assert!(decode(plain).unwrap().1.wildlife.animals.is_empty(), "a save from before the wild animals loads");
        let newer = &plain[..plain.len() - 2];
        assert!(decode(newer).unwrap().1.fleet.barges.is_empty(), "a save from before the barges loads");
        let old = &plain[..plain.len() - 3];
        assert!(decode(old).unwrap().1.locks.at.is_empty(), "a save from before the locks loads");
        let older = &plain[..plain.len() - 5];
        assert!(decode(older).unwrap().1.agents.remains.bodies.is_empty(), "a save from before the bodies loads");
        let oldest = &plain[..plain.len() - 6];
        assert!(decode(oldest).unwrap().1.work.shore_limits.is_empty(), "a save from before the settings loads");
        // Bodies ride along.
        let mut c = b.clone();
        c.agents.remains.bodies.push(crate::agents::Body { id: crate::colony::BeaverId(9), cell: (3, 4), cause: crate::colony::DeathCause::Cold, age_days: 400.0, died: c.time_days });
        let (_, d) = decode(&encode(&c, "telemark").unwrap()).unwrap();
        assert_eq!(d.agents.remains.bodies.len(), 1);
    }
}
