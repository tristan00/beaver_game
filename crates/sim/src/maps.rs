use crate::worldgen::{self, World};
use std::fmt;

static FILES: [(&str, &[u8]); 11] = [
    ("algonquin", include_bytes!("../maps/algonquin.map")),
    ("voyageurs", include_bytes!("../maps/voyageurs.map")),
    ("bavarian", include_bytes!("../maps/bavarian.map")),
    ("telemark", include_bytes!("../maps/telemark.map")),
    ("voyageurs_forks", include_bytes!("../maps/voyageurs_forks.map")),
    ("adirondacks_fork", include_bytes!("../maps/adirondacks_fork.map")),
    ("adirondacks_huntington", include_bytes!("../maps/adirondacks_huntington.map")),
    ("agder_tarn", include_bytes!("../maps/agder_tarn.map")),
    ("voyageurs_twin", include_bytes!("../maps/voyageurs_twin.map")),
    ("algonquin_mizzy", include_bytes!("../maps/algonquin_mizzy.map")),
    ("bavarian_lusen", include_bytes!("../maps/bavarian_lusen.map")),
];

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
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
    #[serde(default)]
    pub climate: Option<Climate>,
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Climate {
    pub temp_c: [f32; 12],
    pub precip_mm: [f32; 12],
    pub snow_mm: [f32; 12],
    pub evap_mm: [f32; 12],
    pub runoff_mm: [f32; 12],
    pub ice_days: Option<(u16, u16)>,
}

impl Climate {
    pub fn runoff_year_mm(&self) -> f32 {
        self.runoff_mm.iter().sum()
    }

    pub fn ice_months(&self) -> f32 {
        self.ice_days.map_or(0.0, |(on, off)| ((365 + off as i32 - on as i32) % 365) as f32 / 365.0 * 12.0)
    }
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
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

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
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

fn parse_header(bytes: &[u8]) -> Result<(MapInfo, bool, usize), MapError> {
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
        climate: climate(&kv).map_err(|m| bad(&m))?,
    };
    let layers: Vec<&str> = get("layers").unwrap_or("height").split(',').collect();
    let has_cover = match layers.as_slice() {
        ["height"] => false,
        ["height", "cover"] => true,
        _ => return Err(bad("unknown layers")),
    };
    Ok((info, has_cover, end))
}

pub fn parse(bytes: &[u8]) -> Result<MapData, MapError> {
    let bad = |m: &str| MapError::Corrupt(m.to_string());
    let (info, has_cover, end) = parse_header(bytes)?;
    let size = info.size;
    let data = &bytes[end + 4..];
    let want = size * size * if has_cover { 5 } else { 4 };
    if data.len() != want {
        return Err(bad(&format!("{} data bytes for a {size}x{size} map (want {want})", data.len())));
    }
    let (h, c) = data.split_at(size * size * 4);
    let heights = h.chunks_exact(4).map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]])).collect();
    Ok(MapData { info, heights, cover: has_cover.then(|| c.to_vec()) })
}

fn climate(kv: &std::collections::HashMap<&str, &str>) -> Result<Option<Climate>, String> {
    const MONTHLY: [&str; 5] = ["climate_temp_c", "climate_precip_mm", "climate_snow_mm", "climate_evap_mm", "climate_runoff_mm"];
    if MONTHLY.iter().all(|k| !kv.contains_key(k)) {
        return Ok(None);
    }
    let months = |k: &str| -> Result<[f32; 12], String> {
        let v: Vec<f32> = kv.get(k).ok_or(format!("missing {k}"))?.split(',').map(|t| t.trim().parse::<f32>()).collect::<Result<_, _>>().map_err(|_| format!("bad {k}"))?;
        v.try_into().map_err(|_| format!("{k} needs 12 months"))
    };
    let ice_days = match kv.get("climate_ice_days").map(|v| v.split_once(',')) {
        Some(Some((on, off))) => on.trim().parse().ok().zip(off.trim().parse().ok()),
        _ => None,
    };
    Ok(Some(Climate {
        temp_c: months(MONTHLY[0])?,
        precip_mm: months(MONTHLY[1])?,
        snow_mm: months(MONTHLY[2])?,
        evap_mm: months(MONTHLY[3])?,
        runoff_mm: months(MONTHLY[4])?,
        ice_days,
    }))
}

pub fn ids() -> impl Iterator<Item = &'static str> {
    FILES.iter().map(|f| f.0)
}

pub fn all() -> Vec<MapInfo> {
    infos().to_vec()
}

pub fn infos() -> &'static [MapInfo] {
    static CACHE: std::sync::OnceLock<Vec<MapInfo>> = std::sync::OnceLock::new();
    CACHE.get_or_init(|| FILES.iter().filter_map(|(_, b)| parse_header(b).ok().map(|(i, _, _)| i)).collect())
}

#[derive(Clone, Debug, PartialEq)]
pub struct MapStats {
    pub streams: usize,
    pub forest: f32,
    pub water: f32,
    pub relief_m: f32,
    pub climate: Option<Climate>,
}

pub fn stats(id: &str) -> Option<MapStats> {
    static CACHE: std::sync::OnceLock<Vec<(String, MapStats)>> = std::sync::OnceLock::new();
    CACHE
        .get_or_init(|| FILES.iter().filter_map(|(_, b)| parse(b).ok()).map(|d| (d.info.id.clone(), stats_of(&d))).collect())
        .iter()
        .find(|(k, _)| k == id)
        .map(|(_, s)| s.clone())
}

fn stats_of(d: &MapData) -> MapStats {
    let n = d.heights.len().max(1) as f32;
    let share = |class: u8| d.cover.as_ref().map_or(0.0, |c| c.iter().filter(|&&v| v == class).count() as f32 / n);
    let (lo, hi) = d.heights.iter().fold((f32::INFINITY, f32::NEG_INFINITY), |(lo, hi), &h| (lo.min(h), hi.max(h)));
    MapStats {
        streams: d.info.inflows.len(),
        forest: share(worldgen::TREE_COVER),
        water: share(worldgen::OPEN_WATER),
        relief_m: (hi - lo).max(0.0),
        climate: d.info.climate.clone(),
    }
}

pub fn world_from(data: MapData) -> Result<World, MapError> {
    let MapData { info, heights, cover } = data;
    let real = worldgen::RealMap { cell_m: info.cell_m, inflows: &info.inflows, cover: cover.as_deref(), flora: &info.id };
    worldgen::from_heights(info.seed(), info.size, heights, &real).map_err(|e| MapError::World(e.to_string()))
}

pub fn world(id: &str) -> Result<(MapInfo, World), MapError> {
    let (_, bytes) = FILES.iter().find(|(k, _)| *k == id).ok_or_else(|| MapError::Unknown(id.to_string()))?;
    let data = parse(bytes)?;
    let info = data.info.clone();
    Ok((info, world_from(data)?))
}
