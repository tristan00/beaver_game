"""Measure the terrain_metrics.py metrics on real beaver landscapes (development aid; nothing ships).

  python tools/real_landscape.py --data tmp/realdata [--site voyageurs|algonquin|biebrza|all] [--km 10]

Builds, for each site, a grid of 10 m cells in the same layout terrain_metrics.measure() uses for a
generated map, then runs the same metric code on it. Inputs (download once into --data/dl, see
docs/TERRAIN_TARGETS.md section 3):
  * Copernicus GLO-30 DSM tiles (30 m, surface model, includes tree canopy)
  * USGS 3DEP 1/3 arc-second DTM tile n49w093 (10 m, bare earth; Voyageurs)
  * NRCan MRDEM-30 DTM (30 m, bare earth; Algonquin), range-read from its cloud-optimised GeoTIFF
  * ESA WorldCover 2021 v200 tiles (10 m land cover: 10 tree cover, 80 permanent water)
  * OpenStreetMap waterway lines, fetched from the Overpass API and cached as JSON in --data
Real data is read only here and stays under tmp/. crates/game and crates/sim never read it.
"""
import argparse, io, json, math, os, sys
import numpy as np
import time
import imagecodecs, requests, tifffile
from PIL import Image, ImageDraw
from scipy import ndimage as ndi

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import terrain_metrics as tm  # noqa: E402

CELL_M = 10.0
SITES = {
    # centre lat, lon; DEM tile; bare-earth DTM tile; WorldCover tile; OSM name of the river measured as the main river
    "voyageurs": dict(lat=48.52, lon=-92.86, dem="Copernicus_DSM_COG_10_N48_00_W093_00_DEM.tif",
                      dtm="USGS_13_n49w093.tif", wc="ESA_WorldCover_10m_2021_v200_N48W093_Map.tif", river="Cranberry Creek"),
    "algonquin": dict(lat=45.56, lon=-78.42, dem="Copernicus_DSM_COG_10_N45_00_W079_00_DEM.tif",
                      dtm="MRDEM", wc="ESA_WorldCover_10m_2021_v200_N45W081_Map.tif", river="Madawaska River"),
    "biebrza":   dict(lat=53.53, lon=22.82, dem="Copernicus_DSM_COG_10_N53_00_E022_00_DEM.tif",
                      dtm=None, wc="ESA_WorldCover_10m_2021_v200_N51E021_Map.tif", river="Biebrza"),
}


class Raster:
    """A north-up GeoTIFF in geographic coordinates, read whole (tiles are 40-180 MB)."""
    def __init__(self, path):
        with tifffile.TiffFile(path) as t:
            p = t.pages[0]
            sx, sy = p.tags["ModelPixelScaleTag"].value[:2]
            _, _, _, lon0, lat0 = p.tags["ModelTiepointTag"].value[:5]
            self.a = p.asarray()
        self.lon0, self.lat0, self.sx, self.sy = lon0, lat0, sx, sy

    def sample(self, lat, lon, bilinear):
        c = (lon - self.lon0) / self.sx - 0.5; r = (self.lat0 - lat) / self.sy - 0.5
        if not bilinear:
            return self.a[np.clip(np.rint(r).astype(int), 0, self.a.shape[0] - 1),
                          np.clip(np.rint(c).astype(int), 0, self.a.shape[1] - 1)]
        return ndi.map_coordinates(self.a.astype(np.float64), [r, c], order=1, mode="nearest")


MRDEM_URL = "https://canelevation-dem.s3.ca-central-1.amazonaws.com/mrdem-30/mrdem-30-dtm.tif"


def lcc_3979(lat, lon):
    """EPSG:3979 (NAD83(CSRS) Canada Atlas Lambert) forward projection, Snyder 1987 eqs. 15-1..15-10, GRS80."""
    a, f = 6378137.0, 1 / 298.257222101
    e = math.sqrt(2 * f - f * f)
    m = lambda p: math.cos(p) / math.sqrt(1 - (e * math.sin(p)) ** 2)
    t = lambda p: np.tan(np.pi / 4 - p / 2) / ((1 - e * np.sin(p)) / (1 + e * np.sin(p))) ** (e / 2)
    p1, p2, p0, l0 = map(math.radians, (49.0, 77.0, 49.0, -95.0))
    nn = (math.log(m(p1)) - math.log(m(p2))) / (math.log(t(p1)) - math.log(t(p2)))
    F = m(p1) / (nn * t(p1) ** nn)
    rho = lambda p: a * F * t(p) ** nn
    th = nn * (np.radians(lon) - l0)
    r = rho(np.radians(lat))
    return r * np.sin(th), rho(p0) - r * np.cos(th)


class Mrdem:
    """Window of the NRCan MRDEM-30 DTM (30 m bare earth, EPSG:3979, LZW tiles), read by HTTP range requests from the
    83 GB cloud-optimised GeoTIFF; only the 512 x 512 tiles under the window are fetched (cached in --data)."""
    def __init__(self, cache):
        self.cache = cache

    def sample(self, lat, lon):
        os.makedirs(self.cache, exist_ok=True)
        x, y = lcc_3979(lat, lon)
        c = (x + 2500020.0) / 30.0 - 0.5; r = (4000020.0 - y) / 30.0 - 0.5
        r0, r1, c0, c1 = int(r.min()) - 1, int(r.max()) + 2, int(c.min()) - 1, int(c.max()) + 2
        meta = os.path.join(self.cache, "mrdem_meta.json")
        if not os.path.exists(meta):
            with tifffile.TiffFile(io.BufferedReader(_HttpFile(MRDEM_URL), buffer_size=1 << 20)) as t:
                p = t.pages[0]
                json.dump(dict(shape=p.shape, tile=p.tilewidth, offs=[int(v) for v in p.dataoffsets],
                               counts=[int(v) for v in p.databytecounts]), open(meta, "w"))
        md = json.load(open(meta)); T = md["tile"]; ntx = -(-md["shape"][1] // T)
        out = np.zeros((r1 - r0, c1 - c0), np.float32)
        for tr in range(r0 // T, (r1 - 1) // T + 1):
            for tc in range(c0 // T, (c1 - 1) // T + 1):
                fn = os.path.join(self.cache, f"mrdem_{tr}_{tc}.npy")
                if not os.path.exists(fn):
                    i = tr * ntx + tc; o, k = md["offs"][i], md["counts"][i]
                    raw = requests.get(MRDEM_URL, headers={"Range": f"bytes={o}-{o + k - 1}"}, timeout=120).content
                    tile = np.frombuffer(imagecodecs.lzw_decode(raw), "<f4").reshape(T, T)
                    np.save(fn, tile)
                tile = np.load(fn)
                ra, rb = max(r0, tr * T), min(r1, (tr + 1) * T); ca, cb = max(c0, tc * T), min(c1, (tc + 1) * T)
                out[ra - r0:rb - r0, ca - c0:cb - c0] = tile[ra - tr * T:rb - tr * T, ca - tc * T:cb - tc * T]
        return ndi.map_coordinates(out.astype(np.float64), [r - r0, c - c0], order=1, mode="nearest")


class _HttpFile(io.RawIOBase):
    """Read-only, seekable file over HTTP range requests (enough for tifffile to parse the header)."""
    def __init__(self, url):
        self.url, self.pos = url, 0
        self.size = int(requests.head(url, timeout=60).headers["Content-Length"])
    def seekable(self): return True
    def readable(self): return True
    def tell(self): return self.pos
    def seek(self, o, w=0):
        self.pos = o if w == 0 else self.pos + o if w == 1 else self.size + o
        return self.pos
    def readinto(self, b):
        if not len(b): return 0
        d = requests.get(self.url, headers={"Range": f"bytes={self.pos}-{self.pos + len(b) - 1}"}, timeout=120).content
        b[:len(d)] = d; self.pos += len(d)
        return len(d)


def grid_latlon(site, n):
    """Local equirectangular grid: row 0 north, column 0 west, CELL_M apart, centred on the site."""
    k = (np.arange(n) - (n - 1) / 2) * CELL_M
    mlat = 111132.0; mlon = 111320.0 * math.cos(math.radians(site["lat"]))
    lat = site["lat"] - k[:, None] / mlat + 0 * k[None, :]
    lon = site["lon"] + k[None, :] / mlon + 0 * k[:, None]
    return lat, lon, mlat, mlon


def bbox(site, n):
    half = n * CELL_M / 2
    _, _, mlat, mlon = grid_latlon(site, 1)
    return (site["lat"] - half / mlat, site["lon"] - half / mlon, site["lat"] + half / mlat, site["lon"] + half / mlon)


def overpass(query, cache):
    """POST an Overpass QL query (cached as JSON); tries two public endpoints, three times each."""
    if os.path.exists(cache):
        with open(cache) as f: return json.load(f)
    err = None
    for attempt in range(3):
        for url in ("https://overpass-api.de/api/interpreter", "https://overpass.kumi.systems/api/interpreter"):
            try:
                r = requests.post(url, data={"data": query}, headers={"User-Agent": "terrain-metrics-dev/1.0"}, timeout=180)
                r.raise_for_status(); d = r.json(); break
            except (requests.RequestException, ValueError) as e:
                err = e; d = None
        if d is not None: break
        time.sleep(10 * (attempt + 1))
    if d is None: raise SystemExit(f"Overpass failed: {err}")
    with open(cache, "w") as f: json.dump(d, f)
    return d


def water_polygons(site, n, cache):
    """Rasterise OSM water areas (natural=water, waterway=riverbank, multipolygon outers minus inners)."""
    b = ",".join(map(str, bbox(site, n)))
    d = overpass(f'[out:json][timeout:120];(way["natural"="water"]({b});relation["natural"="water"]({b});'
                 f'way["waterway"="riverbank"]({b}););out geom;', cache)
    _, _, mlat, mlon = grid_latlon(site, 1)
    xy = lambda geom: [((p["lon"] - site["lon"]) * mlon / CELL_M + (n - 1) / 2,
                        (site["lat"] - p["lat"]) * mlat / CELL_M + (n - 1) / 2) for p in geom]
    outer = Image.new("1", (n, n), 0); inner = Image.new("1", (n, n), 0)
    for e in d["elements"]:
        if e["type"] == "way" and len(e.get("geometry", [])) >= 3:
            ImageDraw.Draw(outer).polygon(xy(e["geometry"]), fill=1)
        elif e["type"] == "relation":
            for role, im in (("outer", outer), ("inner", inner)):
                for c in chains([m for m in e.get("members", []) if m.get("role") == role and m["type"] == "way"]):
                    if len(c) >= 3: ImageDraw.Draw(im).polygon(xy(c), fill=1)
    return np.array(outer, bool) & ~np.array(inner, bool)


def to_cells(geom, site, n):
    _, _, mlat, mlon = grid_latlon(site, 1)
    pts = [((p["lon"] - site["lon"]) * mlon / CELL_M + (n - 1) / 2, (site["lat"] - p["lat"]) * mlat / CELL_M + (n - 1) / 2)
           for p in geom]
    cells = []
    for (x0, y0), (x1, y1) in zip(pts, pts[1:]):
        m = max(1, int(math.ceil(max(abs(x1 - x0), abs(y1 - y0)) * 2)))
        for t in np.linspace(0, 1, m, endpoint=False):
            c = (int(round(x0 + t * (x1 - x0))), int(round(y0 + t * (y1 - y0))))
            if not cells or cells[-1] != c: cells.append(c)
    if pts:
        c = (int(round(pts[-1][0])), int(round(pts[-1][1])))
        if not cells or cells[-1] != c: cells.append(c)
    # keep 8-connected steps only (drop zig-zag duplicates), clip to the window
    out = []
    for c in cells:
        if out and max(abs(c[0] - out[-1][0]), abs(c[1] - out[-1][1])) == 0: continue
        if len(out) >= 2 and max(abs(c[0] - out[-2][0]), abs(c[1] - out[-2][1])) <= 1: out[-1] = c; continue
        out.append(c)
    return out


def chains(ways):
    """Stitch OSM ways that share end nodes into maximal polylines (list of geometry lists)."""
    segs = [w["geometry"] for w in ways if len(w.get("geometry", [])) >= 2]
    key = lambda p: (round(p["lat"], 7), round(p["lon"], 7))
    merged = True
    while merged:
        merged = False
        for i in range(len(segs)):
            for j in range(len(segs)):
                if i == j: continue
                a, b = segs[i], segs[j]
                if key(a[-1]) == key(b[0]): segs[i] = a + b[1:]
                elif key(a[-1]) == key(b[-1]): segs[i] = a + b[::-1][1:]
                else: continue
                segs.pop(j); merged = True; break
            if merged: break
    return segs


def inside(path, n):
    """Longest run of consecutive path cells inside the window."""
    best, cur = [], []
    for c in path:
        if 0 <= c[0] < n and 0 <= c[1] < n: cur.append(c)
        else:
            if len(cur) > len(best): best = cur
            cur = []
    return cur if len(cur) > len(best) else best


def build(site, data, n):
    dl = os.path.join(data, "dl")
    lat, lon, _, _ = grid_latlon(site, n)
    wc = Raster(os.path.join(dl, site["wc"])).sample(lat, lon, False)
    water = wc == 80  # WorldCover permanent water; OSM water areas are added below
    g = {"lake": water.astype(np.uint8), "river": np.zeros((n, n), np.uint8),
         "forest": (wc == 10).astype(np.uint8)}
    heights = {"Copernicus GLO-30 DSM": Raster(os.path.join(dl, site["dem"])).sample(lat, lon, True)}
    if site["dtm"] == "MRDEM":
        heights["NRCan MRDEM-30 DTM"] = Mrdem(os.path.join(data, "mrdem")).sample(lat, lon)
    elif site["dtm"]:
        heights["USGS 3DEP 1/3\" DTM"] = Raster(os.path.join(dl, site["dtm"])).sample(lat, lon, True)
    b = ",".join(map(str, bbox(site, n)))
    osm = overpass(f'[out:json][timeout:90];way["waterway"~"^(river|stream|canal)$"]({b});out geom tags;',
                   os.path.join(data, f"osm_{site['name']}.json"))
    osm_water = water_polygons(site, n, os.path.join(data, f"osm_water_{site['name']}.json"))
    g["lake"] = (water | osm_water).astype(np.uint8)
    ways = [w for w in osm["elements"] if w["type"] == "way"]
    if site["river"]:
        ways_main = [w for w in ways if w.get("tags", {}).get("name") == site["river"]]
    else:
        ways_main = [w for w in ways if w["tags"].get("waterway") == "river"] or ways
    cand = [inside(to_cells(c, site, n), n) for c in chains(ways_main)]
    path = max(cand, key=len) if cand else []
    all_paths = [inside(to_cells(c, site, n), n) for c in chains(ways)]
    widths_tag = [w["tags"]["width"] for w in ways_main if "width" in w.get("tags", {})]
    g["_wc_water"] = water; g["_osm_water"] = osm_water
    return g, heights, path, all_paths, widths_tag, wc


def slope_grid(h):
    gy, gx = np.gradient(h)
    return np.hypot(gx, gy)


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--data", default="tmp/realdata"); ap.add_argument("--site", default="all")
    ap.add_argument("--km", type=float, default=10.0, help="window side, km")
    a = ap.parse_args()
    n = int(a.km * 1000 / CELL_M)
    for name in (SITES if a.site == "all" else [a.site]):
        site = dict(SITES[name], name=name)
        g, heights, path, all_paths, wtag, wc = build(site, a.data, n)
        # rivers too narrow for WorldCover water: the river grid is the OSM centre line on non-water cells
        for p in all_paths:
            for x, y in p: g["river"][y, x] = 1
        g["river"] &= 1 - g["lake"]
        land = ~(g["lake"].astype(bool) | g["river"].astype(bool))
        print(f"== {name}  centre {site['lat']},{site['lon']}  {a.km:g} km window, {n}x{n} cells of {CELL_M:g} m")
        print(f"   main river path {len(path)} cells; OSM waterway lines {len(all_paths)}; OSM width tags {wtag[:8]}")
        wcw, ow = g.pop("_wc_water"), g.pop("_osm_water")
        print(f"   water: WorldCover {wcw.mean():.3f}, OSM areas {ow.mean():.3f}, union {(wcw | ow).mean():.3f} of window")
        cls, cnt = np.unique(wc, return_counts=True)
        print("   WorldCover classes (% of window): " + ", ".join(f"{c}:{100 * k / wc.size:.1f}" for c, k in zip(cls, cnt)))
        for hname, h in heights.items():
            gg = dict(g, height=h, slope=slope_grid(h))
            m = tm.measure(gg, n, CELL_M, 1.0, path=path)
            # pond metrics again without the water bodies cut by the window edge (their size is unknown)
            lab, k = ndi.label(g["lake"])
            edge = np.unique(np.concatenate([lab[0], lab[-1], lab[:, 0], lab[:, -1]]))
            ponds = np.where(np.isin(lab, edge[edge > 0]), 0, g["lake"]).astype(np.uint8)
            mp = tm.measure(dict(gg, lake=ponds), n, CELL_M, 1.0, path=[])
            for key in ("pond_median_area", "pond_mean_area", "pond_spacing_median"): m[key] = mp[key]
            # Valley metrics only where the main river runs on land: in the game the main river never
            # crosses a pond, but real rivers run through lakes, where the profile would measure lake width.
            dry = [c for c in path if wcw[c[1], c[0]] == 0]
            w, dpt = tm.valley_profile(gg, n, dry, CELL_M, 1.0)
            m["valley_floor_width"] = float(np.median(w)) if w else float("nan")
            m["valley_depth"] = float(np.median(dpt)) if dpt else float("nan")
            fl = np.zeros((n, n), bool)
            for x, y in dry: fl[y, x] = True
            floor = ndi.binary_dilation(fl, iterations=4) & land
            m["floor_slope_median"] = float(np.median(gg["slope"][floor] / CELL_M * 100)) if floor.any() else float("nan")
            m["main_river_dry_cells"] = len(dry)
            # the same valley transects on every OSM river/stream cell on land (bigger sample, beaver-stream scale)
            dry_all = [c for p in all_paths for c in p if wcw[c[1], c[0]] == 0]
            w, dpt = tm.valley_profile(gg, n, dry_all, CELL_M, 1.0)
            m["valley_floor_width_all_streams"] = float(np.median(w)) if w else float("nan")
            m["valley_depth_all_streams"] = float(np.median(dpt)) if dpt else float("nan")
            fl[:] = False
            for x, y in dry_all: fl[y, x] = True
            floor = ndi.binary_dilation(fl, iterations=4) & land
            m["floor_slope_median_all_streams"] = float(np.median(gg["slope"][floor] / CELL_M * 100)) if floor.any() else float("nan")
            # river width from water cells across the path (narrow rivers: 0, below the 10 m pixel)
            # (only where the river is not inside a WorldCover lake, which would measure the lake)
            wd = [min(g["lake"][y, max(0, x - 3):x + 4].sum(), g["lake"][max(0, y - 3):y + 4, x].sum()) for x, y in dry[::3]]
            m["river_width"] = float(np.median(wd) * CELL_M) if wd else float("nan")
            sv = [v for p in all_paths for v in tm.sinuosity(p)]
            m["river_sinuosity_all"] = float(np.median(sv)) if sv else float("nan")
            m["water_fraction"] = float((~land).mean())
            cs, _, kc = tm.sizes_ha(land & (g["forest"] == 0), CELL_M * CELL_M / 1e4)
            m["clearing_median_area_all_patches"] = float(np.median(cs)) if len(cs) else float("nan")
            m["clearing_p75_area"] = float(np.percentile(cs, 75)) if len(cs) else float("nan")
            sl = gg["slope"] / CELL_M * 100
            for q in (50, 75, 90, 95): m[f"slope_p{q}"] = float(np.percentile(sl[land], q))
            ps, _, _ = tm.sizes_ha(ponds.astype(bool), CELL_M * CELL_M / 1e4)
            m["pond_count"] = int(len(ps)); m["pond_density_per_km2"] = len(ps) / (a.km * a.km)
            m["pond_p25_p75_ha"] = (float(np.percentile(ps, 25)), float(np.percentile(ps, 75))) if len(ps) else None
            m["relief_window_m"] = float(np.percentile(h, 99) - np.percentile(h, 1))
            print(f"   heights: {hname}  counts {m.pop('_counts')}")
            for key, v in m.items():
                print(f"     {key:32} {v if isinstance(v, (tuple, type(None), int)) else round(v, 3)}")


if __name__ == "__main__":
    main()
