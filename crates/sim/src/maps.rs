//! Real-terrain maps: 512 m windows of lidar elevation (4 m cells) from places beavers live, each
//! crossed by at least two dammable streams that enter from upstream across the map edge, with
//! gentle valley floors, a mix of forest and open land and almost no farmland
//! (`tools/hires_fetch.py` downloads the data, `tools/hires_maps.py` chooses and exports windows).
//! Each `.map` file is a short text header (`key=value` lines, ended by `---`) followed by
//! `size * size` little-endian f32 heights in metres and, if the header lists the `cover` layer,
//! `size * size` land-cover bytes (ESA WorldCover classes). Rivers, lakes and forest come from
//! these via `worldgen::from_heights`.
use crate::worldgen::{self, World};
use std::fmt;

/// Every bundled map, in menu order.
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
    /// Real metres per cell.
    pub cell_m: f32,
    pub size: usize,
    /// Where the elevation comes from (shown in the menu as credit).
    pub source: String,
    /// Border cells where a river enters from outside, with the catchment it drains out there (km2).
    pub inflows: Vec<(usize, usize, f32)>,
}

/// A map file's contents.
#[derive(Clone, Debug, PartialEq)]
pub struct MapData {
    pub info: MapInfo,
    /// Heights in metres, row-major.
    pub heights: Vec<f32>,
    /// Land cover per cell (ESA WorldCover classes), if the file has the layer.
    pub cover: Option<Vec<u8>>,
}

impl MapInfo {
    /// A stable seed for this map (trees, beaver names and other random draws).
    pub fn seed(&self) -> u64 {
        self.id.bytes().fold(0xCBF2_9CE4_8422_2325u64, |h, b| (h ^ b as u64).wrapping_mul(0x100_0000_01B3))
    }
    /// Side of the map in real kilometres.
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

/// Reads a `.map` file's bytes (the bundled ones, or a candidate being checked).
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

/// The bundled maps' descriptions, in menu order.
pub fn all() -> Vec<MapInfo> {
    FILES.iter().filter_map(|(_, b)| parse(b).ok().map(|d| d.info)).collect()
}

/// The world for parsed map data.
pub fn world_from(data: MapData) -> Result<World, MapError> {
    let MapData { info, heights, cover } = data;
    let real = worldgen::RealMap { cell_m: info.cell_m, inflows: &info.inflows, cover: cover.as_deref() };
    worldgen::from_heights(info.seed(), info.size, heights, &real).map_err(|e| MapError::World(e.to_string()))
}

/// The world for map `id`.
pub fn world(id: &str) -> Result<(MapInfo, World), MapError> {
    let (_, bytes) = FILES.iter().find(|(k, _)| *k == id).ok_or_else(|| MapError::Unknown(id.to_string()))?;
    let data = parse(bytes)?;
    let info = data.info.clone();
    Ok((info, world_from(data)?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_bundled_map_parses_and_is_described() {
        let maps = all();
        assert_eq!(maps.len(), FILES.len(), "a bundled map failed to parse");
        for m in &maps {
            assert!(!m.name.is_empty() && !m.place.is_empty() && !m.source.is_empty());
            // a map is about one beaver family's stretch of stream: a few hundred metres across
            assert!((3.0..=6.0).contains(&m.cell_m), "{}: {} m cells", m.id, m.cell_m);
            assert!((300.0..=800.0).contains(&(m.km() * 1000.0)), "{}: {} km across", m.id, m.km());
            assert!(m.inflows.len() >= 2, "{}: fewer than two rivers enter it", m.id);
        }
    }

    /// What makes a map worth playing, checked on the real start the game builds on it: the
    /// designed dam visibly changes a flowing stream, more than one stream has such a site, the
    /// rivers stay narrow and the map is not mostly lake.
    fn check_map(id: &str) {
        use crate::buildings::WET_DEPTH;
        let (sim, dam) = crate::starts::new_game_on_map(id).unwrap_or_else(|e| panic!("{id}: no start: {e}"));
        let effect = crate::starts::dam_effect(&sim, &dam.cells, dam.height).expect("the planned dam crosses the river");
        assert!(effect.is_visible(), "{id}: the planned dam barely changes the river: {effect:?}");
        let streams = crate::starts::dammable_streams(&sim);
        assert!(streams.len() >= 2, "{id}: only {} stream(s) with a good dam site: {streams:?}", streams.len());
        let n = sim.world.size * sim.world.size;
        let lake = sim.world.lake.as_slice().iter().filter(|l| **l).count();
        assert!((lake as f32) < 0.06 * n as f32, "{id}: {lake} lake cells of {n}");
        let river = sim.world.river.as_slice().iter().filter(|r| **r).count();
        let wet = (0..n).filter(|&i| sim.water.depth.as_slice()[i] >= WET_DEPTH && !sim.world.lake.as_slice()[i]).count();
        assert!((wet as f32) < 3.5 * river as f32, "{id}: {wet} wet cells for {river} river cells: the rivers spread wide");
    }

    #[test]
    fn bavarian_is_a_good_beaver_map() {
        check_map("bavarian");
    }

    #[test]
    fn algonquin_is_a_good_beaver_map() {
        check_map("algonquin");
    }

    #[test]
    fn telemark_is_a_good_beaver_map() {
        check_map("telemark");
    }

    #[test]
    fn voyageurs_is_a_good_beaver_map() {
        check_map("voyageurs");
    }

    #[test]
    fn a_damaged_file_is_refused_with_a_reason() {
        assert!(matches!(parse(b"nonsense"), Err(MapError::Corrupt(_))));
        let mut short = b"BEAVERMAP 2\nid=x\nname=x\nplace=x\nlat=1\nlon=1\ncell_m=4\nsize=64\n---\n".to_vec();
        short.extend_from_slice(&[0; 16]);
        assert!(matches!(parse(&short), Err(MapError::Corrupt(_))));
        assert!(matches!(world("atlantis"), Err(MapError::Unknown(_))));
    }
}
