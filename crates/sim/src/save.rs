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

/// Added after `Extras3`, written after it: the harbors' rafts.
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

/// Added after `Extras11`, written after it: the plants' species, the map they come from, and
/// the outcrops' boulders.
#[derive(Default, serde::Serialize, serde::Deserialize)]
struct Extras12 {
    flora: String,
    plants: Vec<(crate::resources::ResourceId, crate::plants::Plant)>,
    boulders: Vec<crate::resources::Boulder>,
}

/// Added after `Extras10`, written after it: the run's tallies.
#[derive(Default, serde::Serialize, serde::Deserialize)]
struct Extras11 {
    run: crate::run::RunStats,
}

/// Added after `Extras9`, written after it: the wolves after the first (the first is saved
/// in the world, where the one wolf always was).
#[derive(Default, serde::Serialize, serde::Deserialize)]
struct Extras10 {
    more_wolves: Vec<crate::predators::Wolf>,
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
    let extras10 = Extras10 { more_wolves: sim.predators.wolves.iter().skip(1).cloned().collect() };
    body.extend(postcard::to_stdvec(&extras10).map_err(|e| format!("could not write the settings: {e}"))?);
    let extras11 = Extras11 { run: sim.run.clone() };
    body.extend(postcard::to_stdvec(&extras11).map_err(|e| format!("could not write the settings: {e}"))?);
    let extras12 = Extras12 {
        flora: sim.resources.flora.clone(),
        plants: sim.resources.items().iter().filter_map(|r| r.plant.map(|p| (r.id, p))).collect(),
        boulders: sim.resources.boulders.clone(),
    };
    body.extend(postcard::to_stdvec(&extras12).map_err(|e| format!("could not write the settings: {e}"))?);
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
    let (extras9, rest) = if rest.is_empty() { (Extras9::default(), rest) } else { postcard::take_from_bytes::<Extras9>(rest).map_err(|_| bad())? };
    let (extras10, rest) = if rest.is_empty() { (Extras10::default(), rest) } else { postcard::take_from_bytes::<Extras10>(rest).map_err(|_| bad())? };
    let fresh = rest.is_empty();
    let (extras11, rest) = if rest.is_empty() { (Extras11::default(), rest) } else { postcard::take_from_bytes::<Extras11>(rest).map_err(|_| bad())? };
    let extras12 = match rest {
        [] => Extras12::default(),
        _ => match postcard::take_from_bytes::<Extras12>(rest) {
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
    sim.predators.wolves.extend(extras10.more_wolves);
    sim.run = extras11.run;
    sim.resources.flora = extras12.flora;
    sim.resources.boulders = extras12.boulders;
    let species: std::collections::HashMap<_, _> = extras12.plants.into_iter().collect();
    let ids: Vec<_> = sim.resources.items().iter().filter(|r| species.contains_key(&r.id)).map(|r| r.id).collect();
    for id in ids {
        if let Ok(r) = sim.resources.get_mut(id) {
            r.plant = species.get(&id).copied();
        }
    }
    // A save from before plant species: they come from the default flora (see `wetland::step`).
    // A save from before looks were kept: each beaver gets one now, and keeps it.
    sim.colony.fill_looks();
    for (id, source) in extras8.sources {
        if let Some(a) = sim.agents.by_id.get_mut(&id) {
            a.source = Some(source);
        }
    }
    sim.work.inventory = sim.buildings.stores.inventory.view();
    // The weather is the place's, read from the world's seed.
    sim.water.sky = crate::weather::Sky::for_seed(sim.seed);
    sim.colony.sky = sim.water.sky;
    sim.instance = crate::next_instance();
    // A save from before the run's tallies counts what already stands.
    if fresh {
        let water = sim.census().water;
        crate::run::close_day(&mut sim, water);
    }
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

