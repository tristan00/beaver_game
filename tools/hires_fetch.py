"""Fetch fine bare-earth elevation (lidar-derived, 1 m) for the beaver map regions, as 4 m grids.

  python -I tools/hires_fetch.py OUT_DIR [region ...]

For each region a square area (`km` a side, centred on `lat`, `lon`) is read from that country's
open elevation service, resampled to CELL_M cells, and cached as OUT_DIR/<region>/dem.npy (float32,
row 0 north, NaN where there is no data) with dem.json (CRS, origin, cell size). Development aid:
the data stays under tmp/ and the game never reads it; `hires_maps.py` turns it into map files.

Sources (all open data):
  3dep   USGS 3D Elevation Program, dynamic bare-earth service (1 m lidar where flown)
  hrdem  Natural Resources Canada HRDEM mosaic (1-2 m lidar DTM), WCS
  nhm    Kartverket Nasjonal hoydemodell DTM 1 m (Norway), WCS
  dgm1   Bayerische Vermessungsverwaltung DGM1 (1 m lidar DTM), 1 km GeoTIFF tiles
Land cover (OUT_DIR/<region>/cover.npy, uint8 per cell) comes from ESA WorldCover 2021 (10 m, CC BY
4.0): 10 trees, 20 shrubs, 30 grass, 40 crops, 50 built-up, 60 bare, 80 water, 90 wetland, 100 moss.
"""
import io
import json
import math
import os
import sys
import time

import numpy as np
import pyproj
import requests
import tifffile

CELL_M = 4.0
CHUNK = 500  # cells per request side

REGIONS = {
    'algonquin': dict(name='Algonquin Park', place='Ontario, Canada', lat=45.545, lon=-78.425, km=10, src='hrdem', epsg=3979),
    'algonquin_b': dict(name='Algonquin Park', place='Ontario, Canada', lat=45.600, lon=-78.250, km=12, src='hrdem', epsg=3979),
    'voyageurs': dict(name='Voyageurs', place='Minnesota, USA', lat=48.470, lon=-92.880, km=10, src='3dep', epsg=26915),
    'bavarian': dict(name='Bavarian Forest', place='Bavaria, Germany', lat=48.905, lon=13.420, km=8, src='dgm1', epsg=25832),
    'telemark': dict(name='Telemark', place='Norway', lat=59.420, lon=9.080, km=12, src='nhm', epsg=25833),
}
SOURCES = {
    '3dep': 'USGS 3D Elevation Program (3DEP) 1 m lidar DEM',
    'hrdem': 'Natural Resources Canada HRDEM lidar DTM',
    'nhm': 'Kartverket Nasjonal hoydemodell DTM 1 m (CC BY 4.0)',
    'dgm1': 'Bayerische Vermessungsverwaltung DGM1 (CC BY 4.0)',
}


def get(url, params=None, tries=4):
    for k in range(tries):
        try:
            r = requests.get(url, params=params, timeout=300)
            if r.status_code == 200:
                return r
            print(f'  HTTP {r.status_code} for {url}', flush=True)
            if r.status_code == 404:
                return r
        except requests.RequestException as e:
            print(f'  {e}', flush=True)
        time.sleep(3 * (k + 1))
    raise RuntimeError(f'failed: {url} {params}')


def tiff(content, what):
    if content[:2] not in (b'II', b'MM'):
        raise RuntimeError(f'{what}: not a GeoTIFF: {content[:300]!r}')
    return tifffile.imread(io.BytesIO(content)).astype(np.float32)


def chunk(src, x0, y1, n):
    """n x n cells of CELL_M, west edge x0, north edge y1 (projected metres); a few tries, as the
    services sometimes answer a request with an empty body."""
    for k in range(3):
        try:
            return chunk_once(src, x0, y1, n)
        except RuntimeError:
            if k == 2:
                raise
            time.sleep(5)


def chunk_once(src, x0, y1, n):
    x1, y0 = x0 + n * CELL_M, y1 - n * CELL_M
    if src == '3dep':
        r = get('https://elevation.nationalmap.gov/arcgis/rest/services/3DEPElevation/ImageServer/exportImage',
                dict(bbox=f'{x0},{y0},{x1},{y1}', bboxSR=26915, imageSR=26915, size=f'{n},{n}', format='tiff',
                     pixelType='F32', interpolation='RSP_BilinearInterpolation', f='image'))
    elif src == 'hrdem':
        r = get('https://datacube.services.geo.ca/wrapper/ogc/elevation-hrdem-mosaic',
                dict(service='WCS', version='1.1.1', request='GetCoverage', identifier='dtm', format='image/geotiff',
                     boundingbox=f'{x0},{y0},{x1},{y1},urn:ogc:def:crs:EPSG::3979',
                     GridBaseCRS='urn:ogc:def:crs:EPSG::3979', GridOffsets=f'{CELL_M},-{CELL_M}'))
    elif src == 'nhm':
        r = get('https://wcs.geonorge.no/skwms1/wcs.hoyde-dtm-nhm-25833',
                dict(service='WCS', version='1.0.0', request='GetCoverage', coverage='nhm_dtm_topo_25833', format='GeoTIFF',
                     crs='EPSG:25833', bbox=f'{x0},{y0},{x1},{y1}', width=n, height=n))
    else:
        raise ValueError(src)
    a = tiff(r.content, f'{src} {x0},{y1}')
    if a.ndim == 3:
        a = a[..., 0] if a.shape[-1] < a.shape[0] else a[0]
    if a.shape != (n, n):
        raise RuntimeError(f'{src}: got {a.shape} for {n}x{n}')
    return a


def dgm1(cache, x0, y1, n):
    """Bavarian DGM1 1 km tiles (named by the km of their south-west corner), averaged to CELL_M."""
    k = int(CELL_M)
    out = np.full((n, n), np.nan, np.float32)
    for ty in range(int(math.floor((y1 - n * CELL_M) / 1000)), int(math.ceil(y1 / 1000))):
        for tx in range(int(math.floor(x0 / 1000)), int(math.ceil((x0 + n * CELL_M) / 1000))):
            fn = os.path.join(cache, f'dgm1_{tx}_{ty}.tif')
            if not os.path.exists(fn):
                r = get(f'https://download1.bayernwolke.de/a/dgm/dgm1/{tx}_{ty}.tif')
                if r.status_code != 200:
                    continue
                open(fn, 'wb').write(r.content)
            with tifffile.TiffFile(fn) as t:
                p = t.pages[0]
                a = p.asarray().astype(np.float32)
                ox, oy = p.tags['ModelTiepointTag'].value[3:5]
            a[a < -1000] = np.nan
            a = a.reshape(a.shape[0] // k, k, a.shape[1] // k, k).mean(axis=(1, 3))
            # place the tile: its north-west corner (ox, oy) in output cells
            c0, r0 = int(round((ox - x0) / CELL_M)), int(round((y1 - oy) / CELL_M))
            rs, cs = slice(max(0, r0), min(n, r0 + a.shape[0])), slice(max(0, c0), min(n, c0 + a.shape[1]))
            out[rs, cs] = a[rs.start - r0:rs.stop - r0, cs.start - c0:cs.stop - c0]
    return out


WORLDCOVER = 'https://esa-worldcover.s3.eu-central-1.amazonaws.com/v200/2021/map/ESA_WorldCover_10m_2021_v200_{}_Map.tif'


def worldcover_tile_blocks(name, rows, cols):
    """The 1024 x 1024 blocks (row, col) of a WorldCover 3 x 3 degree tile, read by HTTP range
    requests from the cloud-optimised GeoTIFF (only the blocks asked for)."""
    import zlib
    url = WORLDCOVER.format(name)
    head = requests.get(url, headers={'Range': 'bytes=0-1048575'}, timeout=120).content
    with tifffile.TiffFile(io.BytesIO(head + bytes(64))) as t:
        p = t.pages[0]
        offs, counts, T = list(p.dataoffsets), list(p.databytecounts), p.tilewidth
    per_row = -(-36000 // T)
    out = {}
    for br in rows:
        for bc in cols:
            k = br * per_row + bc
            raw = requests.get(url, headers={'Range': f'bytes={offs[k]}-{offs[k] + counts[k] - 1}'}, timeout=120).content
            out[(br, bc)] = np.frombuffer(zlib.decompress(raw), np.uint8).reshape(T, T)
    return out, T


def landcover(epsg, x0, y1, n):
    """WorldCover class of every cell (nearest 10 m pixel to the cell centre)."""
    xs = x0 + (np.arange(n) + 0.5) * CELL_M
    ys = y1 - (np.arange(n) + 0.5) * CELL_M
    X, Y = np.meshgrid(xs, ys)
    lon, lat = pyproj.Transformer.from_crs(epsg, 4326, always_xy=True).transform(X, Y)
    cover = np.zeros((n, n), np.uint8)
    lat0s = np.floor(lat / 3) * 3
    lon0s = np.floor(lon / 3) * 3
    for la, lo in sorted(set(zip(lat0s.ravel()[::997].tolist(), lon0s.ravel()[::997].tolist())) | {(lat0s[0, 0], lon0s[0, 0]), (lat0s[-1, -1], lon0s[-1, -1]), (lat0s[0, -1], lon0s[0, -1]), (lat0s[-1, 0], lon0s[-1, 0])}):
        name = f"{'N' if la >= 0 else 'S'}{abs(int(la)):02d}{'E' if lo >= 0 else 'W'}{abs(int(lo)):03d}"
        m = (lat0s == la) & (lon0s == lo)
        # pixel of each cell in this tile (row 0 at its north edge, la + 3)
        pr = np.floor((la + 3 - lat[m]) / (1 / 12000)).astype(int).clip(0, 35999)
        pc = np.floor((lon[m] - lo) / (1 / 12000)).astype(int).clip(0, 35999)
        blocks, T = worldcover_tile_blocks(name, range(pr.min() // 1024, pr.max() // 1024 + 1), range(pc.min() // 1024, pc.max() // 1024 + 1))
        vals = np.array([blocks[(r // T, c // T)][r % T, c % T] for r, c in zip(pr, pc)], np.uint8)
        cover[m] = vals
    return cover


def fetch(out_dir, rid):
    reg = REGIONS[rid]
    folder = os.path.join(out_dir, rid)
    os.makedirs(folder, exist_ok=True)
    cx, cy = pyproj.Transformer.from_crs(4326, reg['epsg'], always_xy=True).transform(reg['lon'], reg['lat'])
    n = int(reg['km'] * 1000 / CELL_M)
    x0 = math.floor((cx - n * CELL_M / 2) / 1000) * 1000.0
    y1 = math.ceil((cy + n * CELL_M / 2) / 1000) * 1000.0
    if not os.path.exists(os.path.join(folder, 'cover.npy')):
        cover = landcover(reg['epsg'], x0, y1, n)
        np.save(os.path.join(folder, 'cover.npy'), cover)
        print(rid, 'land cover', {int(k): round(float((cover == k).mean()), 3) for k in np.unique(cover)}, flush=True)
    if os.path.exists(os.path.join(folder, 'dem.npy')):
        print(rid, 'cached', flush=True)
        return
    dem = np.full((n, n), np.nan, np.float32)
    t = time.time()
    if reg['src'] == 'dgm1':
        dem[:] = dgm1(folder, x0, y1, n)
    else:
        for r0 in range(0, n, CHUNK):
            for c0 in range(0, n, CHUNK):
                fn = os.path.join(folder, f'chunk_{r0}_{c0}.npy')
                if os.path.exists(fn):
                    a = np.load(fn)
                else:
                    try:
                        a = chunk(reg['src'], x0 + c0 * CELL_M, y1 - r0 * CELL_M, CHUNK)
                    except RuntimeError as e:
                        print(f'  no data: {str(e)[:120]}', flush=True)
                        a = np.full((CHUNK, CHUNK), np.nan, np.float32)
                    np.save(fn, a)
                dem[r0:r0 + CHUNK, c0:c0 + CHUNK] = a[:n - r0, :n - c0]
                print(f'  {rid} chunk {r0},{c0} ({time.time() - t:.0f}s)', flush=True)
    dem[(dem < -1000) | (dem > 9000)] = np.nan
    np.save(os.path.join(folder, 'dem.npy'), dem)
    json.dump(dict(epsg=reg['epsg'], x0=x0, y1=y1, cell_m=CELL_M, n=n, src=reg['src'], source=SOURCES[reg['src']]),
              open(os.path.join(folder, 'dem.json'), 'w'))
    ok = np.isfinite(dem)
    print(f'{rid}: {n}x{n} cells, {ok.mean():.1%} with data, {np.nanmin(dem):.1f}..{np.nanmax(dem):.1f} m ({time.time() - t:.0f}s)', flush=True)


if __name__ == '__main__':
    out = sys.argv[1]
    for rid in sys.argv[2:] or REGIONS:
        fetch(out, rid)
