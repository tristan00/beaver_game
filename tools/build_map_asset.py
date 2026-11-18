"""Build the Lunde, Telemark map asset (elevation, river channels, forest cover) from real open data.

  python tools/build_map_asset.py            # write assets/maps/lunde/map.json, PROVENANCE.md and the previews
  python tools/build_map_asset.py --check    # rebuild in memory and fail if it differs from the committed asset

Inputs (downloaded once into tmp/mapdata, which is git-ignored; the sha256 of every input is recorded in the asset):
  * Kartverket Nasjonal hoydemodell DTM1 tile 33-117-110 (1 m bare-earth terrain, EPSG:25833), CC BY 4.0
  * NVE Elvenett river network lines (layer elvenett of the Elvenett1 service, kart.nve.no), NLOD 1.0
  * ESA WorldCover 2021 v200 tile N57E009 (10 m land cover, class 10 = tree cover), CC BY 4.0
  * Kartverket topographic WMS image of the same bounds (reference map for the previews), CC BY 4.0
Needs: numpy, scipy, requests, pyproj, tifffile, imagecodecs, pillow, matplotlib. Every transformation is listed in PROVENANCE.md (generated here, so it cannot drift from the code).

The grid is 128 x 128 cells of 25 m in EPSG:25832 (UTM 32N), row 0 = northern edge, column 0 = western edge.
"""
import argparse, hashlib, io, json, math, os, sys
import numpy as np
import requests
from pyproj import Transformer

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
DATA = os.path.join(ROOT, "tmp", "mapdata")
OUT_DIR = os.path.join(ROOT, "assets", "maps", "lunde")
PREVIEW_DIR = os.path.join(ROOT, "docs", "map-lunde")

N = 128
CELL = 25.0
SUB = 5  # samples per cell side when averaging the 1 m / 10 m rasters (5 x 5 = 25 samples per cell)

# Kartverket place-name entry "Lunde" (tettsted, Nome), https://stadnamn.kartverket.no/fakta/609164
CENTRE_LAT, CENTRE_LON = 59.29832, 9.10268
# The box is the 3.2 km square around the centre, snapped to the 25 m grid, then moved north so the
# Vest-vassdraget main river (Lunde/Straumen reach) and the Skoeelva confluence run through its middle
# (chosen by overlaying the Elvenett lines on the original box; see PROVENANCE.md).
SHIFT_N_M = 325.0

DTM_URL = "https://nedlasting.geonorge.no/hoydedata/DTM1/33-117-110.tif"
WC_URL = "https://esa-worldcover.s3.eu-central-1.amazonaws.com/v200/2021/map/ESA_WorldCover_10m_2021_v200_N57E009_Map.tif"
NVE_URL = "https://kart.nve.no/enterprise/rest/services/Elvenett1/FeatureServer/2/query"
WMS_URL = "https://wms.geonorge.no/skwms1/wms.topo"
TREE = 10

to32 = Transformer.from_crs(4326, 25832, always_xy=True)
to33 = Transformer.from_crs(25832, 25833, always_xy=True)
to4326 = Transformer.from_crs(25832, 4326, always_xy=True)


def bounds():
    """(xmin, ymin, xmax, ymax) of the box in EPSG:25832, recomputed with pyproj."""
    cx, cy = to32.transform(CENTRE_LON, CENTRE_LAT)
    half = N * CELL / 2
    x0 = round((cx - half) / CELL) * CELL
    y0 = round((cy - half) / CELL) * CELL + SHIFT_N_M
    return x0, y0, x0 + N * CELL, y0 + N * CELL


def cell_samples(b, sub=SUB):
    """UTM32 coordinates of sub x sub sample points per cell: arrays (N, N, sub, sub) of x and y."""
    x0, y0, x1, y1 = b
    off = (np.arange(sub) + 0.5) / sub * CELL
    cols = x0 + np.arange(N)[:, None] * CELL + off[None, :]      # (N, sub) by column
    rows = y1 - np.arange(N)[:, None] * CELL - off[None, :]      # (N, sub) by row, row 0 = north
    X = cols[None, :, None, :].repeat(N, 0).repeat(sub, 2)
    Y = rows[:, None, :, None].repeat(N, 1).repeat(sub, 3)
    return X, Y


def sha256(path):
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(1 << 24), b""):
            h.update(chunk)
    return h.hexdigest()


def download(url, path):
    if os.path.exists(path):
        return
    os.makedirs(os.path.dirname(path), exist_ok=True)
    tmp = path + ".part"
    with requests.get(url, stream=True, timeout=120) as r:
        r.raise_for_status()
        with open(tmp, "wb") as f:
            for chunk in r.iter_content(1 << 20):
                f.write(chunk)
    os.replace(tmp, path)


def read_window(path, rows, cols):
    """Rows/cols slice of a tiled GeoTIFF page, decoding only the tiles that overlap it."""
    import tifffile
    with tifffile.TiffFile(path) as t:
        p = t.pages[0]
        th, tw = p.tilelength, p.tilewidth
        out = np.zeros((rows[1] - rows[0], cols[1] - cols[0]), p.dtype)
        ntx = -(-p.imagewidth // tw)
        with open(path, "rb") as fh:
            for tr in range(rows[0] // th, (rows[1] - 1) // th + 1):
                for tc in range(cols[0] // tw, (cols[1] - 1) // tw + 1):
                    i = tr * ntx + tc
                    fh.seek(p.dataoffsets[i])
                    raw = fh.read(p.databytecounts[i])
                    tile, _, _ = p.decode(raw, i)
                    tile = np.asarray(tile).reshape(th, tw)
                    ra, rb = max(rows[0], tr * th), min(rows[1], (tr + 1) * th)
                    ca, cb = max(cols[0], tc * tw), min(cols[1], (tc + 1) * tw)
                    out[ra - rows[0]:rb - rows[0], ca - cols[0]:cb - cols[0]] = \
                        tile[ra - tr * th:rb - tr * th, ca - tc * tw:cb - tc * tw]
    return out


# ------------------------------------------------------------------ elevation

def sample_dtm(path, b):
    """Mean DTM1 height (m) in each cell: 5 x 5 nearest-pixel samples, UTM32 points mapped into the UTM33 tile."""
    import tifffile
    X, Y = cell_samples(b)
    X33, Y33 = to33.transform(X, Y)
    with tifffile.TiffFile(path) as t:
        tags = t.pages[0].tags
        x_org, y_org = tags["ModelTiepointTag"].value[3:5]
        nodata = float(tags["GDAL_NODATA"].value)
    c = np.floor(X33 - x_org).astype(int)
    r = np.floor(y_org - Y33).astype(int)
    r0, r1, c0, c1 = r.min(), r.max() + 1, c.min(), c.max() + 1
    win = read_window(path, (r0, r1), (c0, c1)).astype(np.float64)
    v = win[r - r0, c - c0]
    bad = v == nodata
    if bad.any():
        raise SystemExit(f"{int(bad.sum())} DTM1 samples are no-data inside the box")
    return v.mean(axis=(2, 3))


# ------------------------------------------------------------------ forest

def sample_forest(path, b):
    """Percent (0-100) of each cell's 5 x 5 samples that WorldCover 2021 classes as tree cover (10)."""
    import tifffile
    X, Y = cell_samples(b)
    lon, lat = to4326.transform(X, Y)
    with tifffile.TiffFile(path) as t:
        tags = t.pages[0].tags
        sx, sy = tags["ModelPixelScaleTag"].value[:2]
        lon0, lat0 = tags["ModelTiepointTag"].value[3:5]
    c = np.floor((lon - lon0) / sx).astype(int)
    r = np.floor((lat0 - lat) / sy).astype(int)
    win = read_window(path, (r.min(), r.max() + 1), (c.min(), c.max() + 1))
    v = win[r - r.min(), c - c.min()]
    return np.rint((v == TREE).mean(axis=(2, 3)) * 100).astype(int)


# ------------------------------------------------------------------ rivers

def fetch_rivers(path, b):
    """Elvenett lines (EPSG:25832) that touch the box, cached as GeoJSON."""
    if not os.path.exists(path):
        x0, y0, x1, y1 = b
        lo, la = to4326.transform(x0 - 500, y0 - 500)
        hi, ha = to4326.transform(x1 + 500, y1 + 500)
        r = requests.get(NVE_URL, timeout=120, params={
            "f": "geojson", "where": "1=1", "geometry": f"{lo},{la},{hi},{ha}",
            "geometryType": "esriGeometryEnvelope", "inSR": 4326, "outSR": 25832,
            "spatialRel": "esriSpatialRelIntersects", "returnGeometry": "true", "orderByFields": "strekninglnr",
            "outFields": "strekninglnr,elvenavn,elveordenstrahler,lengde_m,vassdragsnr,til_utlop,oppdateringsdato"})
        r.raise_for_status()
        d = r.json()
        if len(d.get("features", [])) >= 2000 or d.get("exceededTransferLimit"):
            raise SystemExit("river query hit the service record limit")
        os.makedirs(os.path.dirname(path), exist_ok=True)
        with open(path, "w", encoding="utf-8") as f:
            json.dump(d, f, ensure_ascii=False, sort_keys=True)
    with open(path, encoding="utf-8") as f:
        return json.load(f)


def cell_of(x, y, b):
    x0, y0, x1, y1 = b
    return int(math.floor((y1 - y) / CELL)), int(math.floor((x - x0) / CELL))


def trace(coords, b):
    """Ordered cells (row, col) a polyline passes through inside the box, sampled every 1 m, no repeats in a row."""
    cells = []
    for (xa, ya), (xb, yb) in zip(coords[:-1], coords[1:]):
        n = max(1, int(math.hypot(xb - xa, yb - ya)))
        for i in range(n + 1):
            t = i / n
            r, c = cell_of(xa + (xb - xa) * t, ya + (yb - ya) * t, b)
            if 0 <= r < N and 0 <= c < N and (not cells or cells[-1] != (r, c)):
                cells.append((r, c))
    return cells


def river_reaches(gj, b, min_cells=2):
    """One reach per Elvenett segment: dict(id, name, order, cells) with cells in the digitised direction.
    Segment ids and endpoints (the nodes joining segments) are kept so the network can be rebuilt."""
    reaches = []
    for f in sorted(gj["features"], key=lambda f: f["properties"]["strekninglnr"]):
        g = f["geometry"]
        lines = [g["coordinates"]] if g["type"] == "LineString" else g["coordinates"]
        for k, line in enumerate(lines):
            cells = trace(line, b)
            if len(cells) >= min_cells:
                p = f["properties"]
                reaches.append(dict(id=p["strekninglnr"] + (f"-{k}" if k else ""), order=p["elveordenstrahler"] or 1,
                                    cells=cells, start=tuple(np.round(line[0], 1)), end=tuple(np.round(line[-1], 1))))
    return reaches


def link_reaches(reaches):
    """Directed cell-to-cell edges of the river network: consecutive cells of each reach, plus a link from the last
    cell of a reach to the first cell of every reach that begins at the node where it ends."""
    by_start = {}
    for rc in reaches:
        by_start.setdefault(rc["start"], []).append(rc)
    edges = set()
    for rc in reaches:
        for a, b_ in zip(rc["cells"][:-1], rc["cells"][1:]):
            edges.add((a, b_))
        for nxt in by_start.get(rc["end"], []):
            if nxt is not rc and rc["cells"][-1] != nxt["cells"][0]:
                edges.add((rc["cells"][-1], nxt["cells"][0]))
    return sorted(edges)


# ------------------------------------------------------------------ monotone rivers

def make_monotone(elev, edges):
    """Return (new elevation grid in metres, stats) so that for every edge a -> b of the river network
    new[b] <= new[a]. Method (per river cell, all other cells untouched):
      1. cells in a flow cycle are set to the cycle's mean (they must be level);
      2. carve = lowest height met on the way down from the sources to the cell (cuts embankments, bridges);
      3. fill  = highest height met on the way from the cell to the outlet (fills pits);
      4. new height = mean of carve and fill. Both are non-increasing downstream, so their mean is too;
         where the terrain already falls downstream the result equals the original."""
    from scipy.sparse import coo_matrix
    from scipy.sparse.csgraph import connected_components
    cells = sorted({c for e in edges for c in e})
    idx = {c: i for i, c in enumerate(cells)}
    n = len(cells)
    e0 = np.array([elev[c] for c in cells], dtype=np.float64)
    src = np.array([idx[a] for a, _ in edges]); dst = np.array([idx[b_] for _, b_ in edges])
    ncomp, comp = connected_components(coo_matrix((np.ones(len(edges)), (src, dst)), shape=(n, n)),
                                       directed=True, connection="strong")
    cyc_cells = int((np.bincount(comp) > 1)[comp].sum())
    ce = np.array([e0[comp == k].mean() for k in range(ncomp)])
    pairs = sorted({(int(comp[s]), int(comp[d])) for s, d in zip(src, dst) if comp[s] != comp[d]})
    down = [[] for _ in range(ncomp)]; up = [[] for _ in range(ncomp)]
    indeg = np.zeros(ncomp, int)
    for a, d in pairs:
        down[a].append(d); up[d].append(a); indeg[d] += 1
    order, stack = [], [k for k in range(ncomp) if indeg[k] == 0]
    while stack:
        k = stack.pop(); order.append(k)
        for d in down[k]:
            indeg[d] -= 1
            if indeg[d] == 0:
                stack.append(d)
    assert len(order) == ncomp, "river network still has a cycle"
    carve = ce.copy()
    for k in order:
        for u in up[k]:
            carve[k] = min(carve[k], carve[u])
    fill = ce.copy()
    for k in reversed(order):
        for d in down[k]:
            fill[k] = max(fill[k], fill[d])
    new = (carve + fill) / 2
    out = elev.copy()
    for c, i in idx.items():
        out[c] = new[comp[i]]
    changed = np.abs(np.array([out[c] - elev[c] for c in cells]))
    return out, dict(river_cells=n, cells_in_cycles=cyc_cells, cells_changed_over_5cm=int((changed > 0.05).sum()),
                     max_change_m=round(float(changed.max()), 2), mean_change_m=round(float(changed.mean()), 3))


# ------------------------------------------------------------------ asset

def build():
    """Return (asset dict, provenance text, grids) from the cached inputs; downloads them when missing."""
    b = bounds()
    dtm = os.path.join(DATA, "33-117-110.tif"); wc = os.path.join(DATA, "ESA_WorldCover_N57E009.tif")
    download(DTM_URL, dtm); download(WC_URL, wc)
    elev_raw = sample_dtm(dtm, b)
    forest = sample_forest(wc, b)
    rivers_path = os.path.join(OUT_DIR, "source", "elvenett.json")
    gj = fetch_rivers(rivers_path, b)
    reaches = river_reaches(gj, b)
    edges = link_reaches(reaches)
    elev, stats = make_monotone(elev_raw, edges)
    elev_cm = np.rint(elev * 100).astype(int)
    for a, c in edges:  # rounding is monotone, but fail loudly if it ever is not
        assert elev_cm[c] <= elev_cm[a]
    names = {f["properties"]["strekninglnr"]: f["properties"].get("elvenavn") for f in gj["features"]}
    mask = np.zeros((N, N), bool)
    for rc in reaches:
        for c in rc["cells"]:
            mask[c] = True
    ids = {rc["id"] for rc in reaches}
    joins = sorted({(rc["id"], nx["id"]) for rc in reaches for nx in reaches
                    if nx is not rc and rc["end"] == nx["start"]})
    x0, y0, x1, y1 = b
    asset = {
        "format": "beaver-map/1",
        "name": "Lunde, Nome, Telemark, Norway",
        "crs": "EPSG:25832",
        "bounds_m": {"xmin": x0, "ymin": y0, "xmax": x1, "ymax": y1},
        "size": N, "cell_m": CELL, "row0": "north", "col0": "west",
        "elevation_cm": elev_cm.tolist(),
        "forest_pct": forest.tolist(),
        "river_mask": ["".join("#" if v else "." for v in row) for row in mask],
        "rivers": [{"id": rc["id"], "name": names.get(rc["id"].split("-")[0]), "strahler": rc["order"],
                    "cells": [list(c) for c in rc["cells"]]} for rc in reaches],
        "river_joins": [list(j) for j in joins if j[0] in ids and j[1] in ids],
        "inputs_sha256": {"DTM1_33-117-110.tif": sha256(dtm), "WorldCover_N57E009.tif": sha256(wc),
                          "elvenett.json": sha256(rivers_path)},
    }
    return asset, stats, dict(elev_raw=elev_raw, elev=elev, forest=forest, mask=mask, reaches=reaches, bounds=b)


def dumps(asset):
    """Compact, stable JSON: one grid row per line."""
    parts = []
    for k, v in asset.items():
        if k in ("elevation_cm", "forest_pct"):
            body = "[\n" + ",\n".join(json.dumps(r, separators=(",", ":")) for r in v) + "\n]"
        elif k == "rivers":
            body = "[\n" + ",\n".join(json.dumps(r, separators=(",", ":"), ensure_ascii=False) for r in v) + "\n]"
        else:
            body = json.dumps(v, separators=(",", ":"), ensure_ascii=False)
        parts.append(json.dumps(k) + ":" + body)
    return "{\n" + ",\n".join(parts) + "\n}\n"


# ------------------------------------------------------------------ previews

def fetch_reference(path, b):
    """Kartverket topographic map (WMS, EPSG:25832) of exactly the box, 512 x 512 px."""
    if os.path.exists(path):
        return
    x0, y0, x1, y1 = b
    r = requests.get(WMS_URL, timeout=120, params={
        "service": "WMS", "version": "1.3.0", "request": "GetMap", "layers": "topo", "styles": "",
        "crs": "EPSG:25832", "bbox": f"{x0:g},{y0:g},{x1:g},{y1:g}", "width": 512, "height": 512, "format": "image/png"})
    r.raise_for_status()
    assert r.headers["content-type"].startswith("image/png")
    os.makedirs(os.path.dirname(path), exist_ok=True)
    with open(path, "wb") as f:
        f.write(r.content)


def hillshade(z):
    gy, gx = np.gradient(z, CELL)
    slope = np.pi / 2 - np.arctan(np.hypot(gx, gy) * 2.0)
    aspect = np.arctan2(-gx, gy)
    az, alt = math.radians(315), math.radians(45)
    return np.clip(np.sin(alt) * np.sin(slope) + np.cos(alt) * np.cos(slope) * np.cos(az - aspect), 0, 1)


def render_previews(g, outdir, ref_path):
    from PIL import Image, ImageDraw
    from matplotlib import colormaps
    S = 4  # preview pixels per cell
    z = g["elev"]
    shade = hillshade(z)
    col = colormaps["terrain"]((z - z.min()) / (z.max() - z.min()) * 0.85)[..., :3]
    elev_img = np.clip(col * (0.55 + 0.45 * shade[..., None]), 0, 1)
    forest = g["forest"] / 100.0
    forest_img = (1 - forest[..., None]) * np.array([0.93, 0.91, 0.82]) + forest[..., None] * np.array([0.13, 0.45, 0.20])
    river_img = np.ones((N, N, 3)) * np.array([0.93, 0.93, 0.93])
    river_img[g["mask"]] = [0.10, 0.35, 0.85]
    rgb = lambda a: Image.fromarray((np.repeat(np.repeat(a, S, 0), S, 1) * 255).astype(np.uint8))
    panels = {"elevation": rgb(elev_img), "rivers": rgb(river_img), "forest": rgb(forest_img)}
    os.makedirs(outdir, exist_ok=True)
    for k, im in panels.items():
        im.save(os.path.join(outdir, f"{k}.png"), optimize=True)
    ref = Image.open(ref_path).convert("RGB")
    assert ref.size == (N * S, N * S)
    # overlay on the reference map: forest >= 50 % tinted green, river cells pink
    layer = np.zeros((N * S, N * S, 4), np.uint8)
    layer[np.kron(g["forest"] >= 50, np.ones((S, S), bool))] = (0, 140, 40, 90)
    layer[np.kron(g["mask"], np.ones((S, S), bool))] = (230, 0, 130, 255)
    Image.alpha_composite(ref.convert("RGBA"), Image.fromarray(layer)).convert("RGB").save(
        os.path.join(outdir, "overlay_on_reference.png"), optimize=True)
    # one sheet: reference | elevation | rivers | forest, same bounds, north up
    names = ["Kartverket topo (reference)", "Elevation (DTM1, hillshade)", "River channels (NVE Elvenett)",
             "Forest cover (WorldCover)"]
    ims = [ref, panels["elevation"], panels["rivers"], panels["forest"]]
    W = N * S
    sheet = Image.new("RGB", (W * 2 + 30, (W + 28) * 2 + 10), "white"); dd = ImageDraw.Draw(sheet)
    for i, (nm, im) in enumerate(zip(names, ims)):
        x, y = 10 + (i % 2) * (W + 10), 5 + (i // 2) * (W + 28)
        dd.text((x, y), nm, fill="black"); sheet.paste(im, (x, y + 16))
    sheet.save(os.path.join(outdir, "side_by_side.png"), optimize=True)


# ------------------------------------------------------------------ provenance text

def provenance(asset, stats, g):
    b = asset["bounds_m"]; x0, y0, x1, y1 = b["xmin"], b["ymin"], b["xmax"], b["ymax"]
    cx, cy = (x0 + x1) / 2, (y0 + y1) / 2
    lo, la = to4326.transform(x0, y0); hi, ha = to4326.transform(x1, y1)
    clo, cla = to4326.transform(cx, cy)
    e33, n33 = to33.transform(cx, cy)
    ox, oy = to32.transform(CENTRE_LON, CENTRE_LAT)
    h = asset["inputs_sha256"]
    raw, new = g["elev_raw"], g["elev"]
    return f"""# Provenance and licences: Lunde map asset

`map.json` is generated by `python tools/build_map_asset.py`; this file is generated by the same run, so it matches
the asset. `python tools/build_map_asset.py --check` rebuilds both and fails on any difference.

## Area

* Site: Lunde, Nome, Telemark (Kartverket place-name entry https://stadnamn.kartverket.no/fakta/609164, 59.29832 N 9.10268 E),
  chosen in `docs/research/beaver-study-area.md`.
* 128 x 128 cells of 25 m = 3.2 km square, EPSG:25832 (UTM 32N). Row 0 is the north edge, column 0 the west edge.
* Bounds recomputed with pyproj (the hand-converted values in the research note were within about 10 m): the place name
  is at E {ox:.1f}, N {oy:.1f}; the box corner is snapped to the 25 m grid, then the box is **shifted {SHIFT_N_M:g} m north** so
  the Vest-vassdraget main river (the Lunde/Straumen reach, Strahler order 7) crosses the middle of the box with the Skoeelva
  joining it. Unshifted box: E 504250-507450, N 6571675-6574875.
* Final bounds: E {x0:.0f}-{x1:.0f}, N {y0:.0f}-{y1:.0f} (EPSG:25832); lat {la:.5f}-{ha:.5f}, lon {lo:.5f}-{hi:.5f} (WGS84);
  centre {cla:.5f} N, {clo:.5f} E.
* Reference maps (links to the centre of the same box): Norgeskart https://www.norgeskart.no/#!?project=norgeskart&layers=1002&zoom=14&lat={n33:.0f}&lon={e33:.0f}
  (EPSG:25833 coordinates, as Norgeskart expects) and OpenStreetMap https://www.openstreetmap.org/#map=14/{cla:.5f}/{clo:.5f}.
  The committed reference image is Kartverket's own topographic map cut to exactly the box (see Sources).

## Sources

| Layer | Source | Licence | Credit |
|---|---|---|---|
| Elevation | Kartverket, Nasjonal hoydemodell DTM1 (1 m terrain model), tile 33-117-110, {DTM_URL}; Geonorge record fd96e6c3-eff6-437c-8fd6-e7c4718b1bf8; sha256 `{h['DTM1_33-117-110.tif']}` | CC BY 4.0 https://creativecommons.org/licenses/by/4.0/ | (c) Kartverket |
| River channels | NVE, Elvenett river network, layer `elvenett` of {NVE_URL.rsplit('/query', 1)[0]}; raw response kept in `source/elvenett.json` (sha256 `{h['elvenett.json']}`); retrieved 2026-11-17 | NLOD 1.0 http://data.norge.no/nlod/no/1.0 | Kilde: NVE |
| Forest cover | ESA WorldCover 2021 v200, tile N57E009, {WC_URL}; sha256 `{h['WorldCover_N57E009.tif']}` | CC BY 4.0 | Contains modified Copernicus Sentinel data (2021) processed by ESA WorldCover consortium |
| Reference map (previews only) | Kartverket topographic WMS {WMS_URL}, layer `topo`, GetMap EPSG:25832, bbox {x0:g},{y0:g},{x1:g},{y1:g}, 512 x 512 px, retrieved 2026-11-17 | CC BY 4.0 | (c) Kartverket |

The asset is a derived work of the three datasets: redistribution, commercial use included, is allowed as long as the credits
above ship with it. Licences were confirmed in `docs/research/beaver-study-area.md` (the NLOD 1.0 text itself was not read there).

## Every change from source to asset

1. **Elevation**: each cell is the mean of 5 x 5 DTM1 pixels (the pixel containing each sample point; sample points are the
   centres of a 5 x 5 sub-grid of the cell, mapped from UTM 32N to the tile's UTM 33N with pyproj; no interpolation). No
   no-data pixel falls inside the box (the tool aborts if one does). Stored in whole centimetres.
   Raw range {raw.min():.2f}-{raw.max():.2f} m; asset range {new.min():.2f}-{new.max():.2f} m.
2. **Rivers**: every Elvenett segment (all Strahler orders) touching the box is traced every 1 m and reduced to the ordered
   list of 25 m cells it crosses, in the digitised direction, which is downstream (segment starts are higher than their ends;
   lake and canal reaches are level). Segments are clipped to the box; reaches shorter than 2 cells are dropped.
   Each channel is one cell wide, a centreline: wide reaches (the Lunde/Straumen water is 100-200 m wide) show as their
   centreline. `river_joins` lists reach pairs meeting at a network node (end of the first = start of the second).
   `river_mask` is the union of all reach cells.
3. **River elevations made non-rising downstream** (the only edit to elevation): raw DTM1 means rise downstream in places
   (road embankments over culverts, noise on flat lake/canal reaches). Along the directed river network (consecutive reach
   cells plus junction links) each river cell is set to the mean of its *carve* height (lowest value met from the sources down
   to it) and its *fill* height (highest value met from it down to the outlet). Both never rise downstream, so neither does their
   mean; where the terrain already falls downstream the cell is unchanged. Cells in a flow cycle are first averaged together.
   Statistics: {stats['river_cells']} river cells, {stats['cells_changed_over_5cm']} changed by more than 5 cm,
   mean change {stats['mean_change_m']} m, largest change {stats['max_change_m']} m, {stats['cells_in_cycles']} cells in cycles.
   Non-river cells are exactly the raw means.
4. **Forest cover**: percent (0-100) of a cell's 25 samples (WorldCover pixel under each sample point; the tile is on a WGS84
   grid, 10 m) whose class is 10 (tree cover); every other class counts as open. Nothing else is changed.
5. **Previews** in `docs/map-lunde/`: elevation (terrain colours + hillshade), river mask, forest percent, the Kartverket
   reference map of the same bounds (`reference_topo.png`, the WMS image as received), an overlay of the river cells (pink) and forest of 50 % or more (green tint) on it, and a
   side-by-side sheet.

## Format of `map.json` (`beaver-map/1`)

`elevation_cm[row][col]` integer cm above sea level, `forest_pct[row][col]` 0-100, `river_mask[row]` string of `#`/`.`,
`rivers[]` ordered downstream cell lists `[row, col]` with Strahler order and name, `river_joins[]`, `bounds_m`, `inputs_sha256`.
"""


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--check", action="store_true", help="rebuild and compare with the committed asset instead of writing")
    a = ap.parse_args()
    asset, stats, g = build()
    text, prov = dumps(asset), provenance(asset, stats, g)
    mp, pp = os.path.join(OUT_DIR, "map.json"), os.path.join(OUT_DIR, "PROVENANCE.md")
    if a.check:
        same = all(os.path.exists(p) and io.open(p, encoding="utf-8", newline="").read() == t
                   for p, t in ((mp, text), (pp, prov)))
        print("asset reproduced" if same else "MISMATCH: rebuilt asset differs from the committed one")
        sys.exit(0 if same else 1)
    os.makedirs(OUT_DIR, exist_ok=True)
    for p, t in ((mp, text), (pp, prov)):
        with io.open(p, "w", encoding="utf-8", newline="") as f:
            f.write(t)
    ref = os.path.join(PREVIEW_DIR, "reference_topo.png")
    fetch_reference(ref, g["bounds"])
    render_previews(g, PREVIEW_DIR, ref)
    print("wrote", mp, "|", stats)


if __name__ == "__main__":
    main()
