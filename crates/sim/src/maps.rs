use crate::worldgen::{self, World};
use std::fmt;

const FILES: [(&str, &[u8]); 4] = [
    ("algonquin", include_bytes!("../maps/algonquin.map")),
    ("voyageurs", include_bytes!("../maps/voyageurs.map")),
    ("bavarian", include_bytes!("../maps/bavarian.map")),
    ("telemark", include_bytes!("../maps/telemark.map")),
];

#[derive(Clone, Debug, PartialEq)]
pub struct MapInfo {
    pub id: String,
    pub name: String,
    pub place: String,
    pub lat: f32,
    pub lon: f32,
    pub cell_m: f32,
    pub size: usize,
    pub source: String,
    pub inflows: Vec<(usize, usize, f32)>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MapData {
    pub info: MapInfo,
    pub heights: Vec<f32>,
    pub cover: Option<Vec<u8>>,
}

impl MapInfo {
    pub fn seed(&self) -> u64 {
        self.id.bytes().fold(0xCBF2_9CE4_8422_2325u64, |h, b| (h ^ b as u64).wrapping_mul(0x100_0000_01B3))
    }
    pub fn km(&self) -> f32 {
        self.cell_m * self.size as f32 / 1000.0
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum MapError {
    Unknown(String),
    Corrupt(String),
    World(String),
}

impl fmt::Display for MapError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MapError::Unknown(id) => write!(f, "no map called '{id}'"),
            MapError::Corrupt(m) => write!(f, "map file is damaged: {m}"),
            MapError::World(m) => write!(f, "map terrain is unusable: {m}"),
        }
    }
}

impl std::error::Error for MapError {}

pub fn parse(bytes: &[u8]) -> Result<MapData, MapError> {
    let bad = |m: &str| MapError::Corrupt(m.to_string());
    let end = bytes.windows(4).position(|w| w == b"---\n").ok_or_else(|| bad("no header end"))?;
    let header = std::str::from_utf8(&bytes[..end]).map_err(|_| bad("header is not text"))?;
    let mut lines = header.lines();
    if lines.next() != Some("BEAVERMAP 2") {
        return Err(bad("not a BEAVERMAP 2 file"));
    }
    let kv: std::collections::HashMap<&str, &str> = lines.filter_map(|l| l.split_once('=')).collect();
    let get = |k: &str| kv.get(k).copied().ok_or_else(|| bad(&format!("missing {k}")));
    let num = |k: &str| get(k).and_then(|v| v.parse::<f32>().map_err(|_| bad(&format!("bad {k}"))));
    let size: usize = get("size")?.parse().map_err(|_| bad("bad size"))?;
    let inflows = get("inflows")
        .unwrap_or("")
        .split(';')
        .filter(|s| !s.is_empty())
        .map(|t| {
            let v: Vec<f32> = t.split(',').filter_map(|n| n.parse().ok()).collect();
            let ok = v.len() == 3 && v[0] >= 0.0 && v[1] >= 0.0 && (v[0] as usize) < size && (v[1] as usize) < size && v[2] > 0.0;
            if ok { Ok((v[0] as usize, v[1] as usize, v[2])) } else { Err(bad("bad inflow")) }
        })
        .collect::<Result<Vec<_>, _>>()?;
    let info = MapInfo {
        id: get("id")?.to_string(),
        name: get("name")?.to_string(),
        place: get("place")?.to_string(),
        lat: num("lat")?,
        lon: num("lon")?,
        cell_m: num("cell_m")?,
        size,
        source: get("source").unwrap_or("").to_string(),
        inflows,
    };
    let layers: Vec<&str> = get("layers").unwrap_or("height").split(',').collect();
    let has_cover = match layers.as_slice() {
        ["height"] => false,
        ["height", "cover"] => true,
        _ => return Err(bad("unknown layers")),
    };
    let data = &bytes[end + 4..];
    let want = size * size * if has_cover { 5 } else { 4 };
    if data.len() != want {
        return Err(bad(&format!("{} data bytes for a {size}x{size} map (want {want})", data.len())));
    }
    let (h, c) = data.split_at(size * size * 4);
    let heights = h.chunks_exact(4).map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]])).collect();
    Ok(MapData { info, heights, cover: has_cover.then(|| c.to_vec()) })
}

pub fn all() -> Vec<MapInfo> {
    FILES.iter().filter_map(|(_, b)| parse(b).ok().map(|d| d.info)).collect()
}

pub fn world_from(data: MapData) -> Result<World, MapError> {
    let MapData { info, heights, cover } = data;
    let real = worldgen::RealMap { cell_m: info.cell_m, inflows: &info.inflows, cover: cover.as_deref() };
    worldgen::from_heights(info.seed(), info.size, heights, &real).map_err(|e| MapError::World(e.to_string()))
}

pub fn world(id: &str) -> Result<(MapInfo, World), MapError> {
    let (_, bytes) = FILES.iter().find(|(k, _)| *k == id).ok_or_else(|| MapError::Unknown(id.to_string()))?;
    let data = parse(bytes)?;
    let info = data.info.clone();
    Ok((info, world_from(data)?))
}

