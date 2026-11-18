"""Scan real elevation for beaver-friendly map windows and export the best one per region.

usage: python -I scan_maps.py RAW_DIR OUT_DIR PREVIEW_DIR

RAW_DIR holds one folder per region with Terrarium tiles named z_x_y.png. For each region the
tiles are stitched, resampled to game cells, and every 128x128 window is scored on what the
game needs (see `score`). The best window is written as OUT_DIR/<region>.map (text header +
little-endian f32 heights in game units) and a preview PNG goes to PREVIEW_DIR.
"""
import heapq
import math
import os
import sys

import numpy as np
from PIL import Image
from scipy import ndimage

N = 128                 # game map side, cells
TOP = 6                 # windows exported per region
T = 40                  # river threshold (cells drained), as the game's RIVER_MIN_ACCUMULATION
EXAG = 2.5              # vertical exaggeration so hills read in game units
DAM_M = 2.0             # test dam height, metres
REGIONS = {
    'algonquin': ('Algonquin Park', 'Ontario, Canada', 45.58, -78.40),
    'voyageurs': ('Voyageurs', 'Minnesota, USA', 48.47, -92.83),
    'telemark': ('Telemark', 'Norway', 59.43, 8.45),
    'bavarian': ('Bavarian Forest', 'Bavaria, Germany', 48.93, 13.42),
}
D8 = [(-1, -1), (0, -1), (1, -1), (-1, 0), (1, 0), (-1, 1), (0, 1), (1, 1)]


def load_region(folder):
    tiles = {}
    for f in os.listdir(folder):
        if f.endswith('.png'):
            z, x, y = (int(v) for v in f[:-4].split('_'))
            tiles[(x, y)] = os.path.join(folder, f)
    xs = sorted({k[0] for k in tiles})
    ys = sorted({k[1] for k in tiles})
    rows = []
    for y in ys:
        row = []
        for x in xs:
            a = np.asarray(Image.open(tiles[(x, y)]).convert('RGB')).astype(np.float64)
            row.append(a[..., 0] * 256 + a[..., 1] + a[..., 2] / 256 - 32768)
        rows.append(np.hstack(row))
    return np.vstack(rows), z


def fill_and_route(h):
    """Priority-flood fill (with a tiny gradient on flats) and D8 flow on the filled surface."""
    rows, cols = h.shape
    filled = np.full(h.shape, np.inf)
    done = np.zeros(h.shape, bool)
    pq = []
    for y in range(rows):
        for x in (0, cols - 1):
            heapq.heappush(pq, (h[y, x], y, x))
            done[y, x] = True
            filled[y, x] = h[y, x]
    for x in range(1, cols - 1):
        for y in (0, rows - 1):
            heapq.heappush(pq, (h[y, x], y, x))
            done[y, x] = True
            filled[y, x] = h[y, x]
    while pq:
        e, y, x = heapq.heappop(pq)
        for dx, dy in D8:
            nx, ny = x + dx, y + dy
            if 0 <= nx < cols and 0 <= ny < rows and not done[ny, nx]:
                done[ny, nx] = True
                v = max(h[ny, nx], e + 1e-4)
                filled[ny, nx] = v
                heapq.heappush(pq, (v, ny, nx))
    # D8 steepest descent on the filled surface.
    flow = np.full(h.shape + (2,), -1, np.int32)
    best = np.zeros(h.shape)
    pad = np.pad(filled, 1, constant_values=-np.inf)
    for dx, dy in D8:
        d = math.hypot(dx, dy)
        nb = pad[1 + dy:1 + dy + rows, 1 + dx:1 + dx + cols]
        drop = (filled - nb) / d
        better = drop > best
        best = np.where(better, drop, best)
        flow[..., 0] = np.where(better, dx, flow[..., 0])
        flow[..., 1] = np.where(better, dy, flow[..., 1])
    order = np.argsort(-filled, axis=None)
    acc = np.ones(rows * cols)
    fdx, fdy = flow[..., 0].ravel(), flow[..., 1].ravel()
    for i in order:
        dx, dy = fdx[i], fdy[i]
        if dx == -1 and dy == -1 and best.ravel()[i] == 0:
            continue
        if best.ravel()[i] <= 0:
            continue
        y, x = divmod(i, cols)
        nx, ny = x + dx, y + dy
        if 0 <= nx < cols and 0 <= ny < rows:
            acc[ny * cols + nx] += acc[i]
    return filled, flow, acc.reshape(h.shape), best > 0


def flat_water(h, cell_m):
    """Real lakes are flat in a DEM: connected near-level patches of at least 40 cells."""
    gy, gx = np.gradient(h, cell_m)
    flat = np.hypot(gx, gy) < 0.002
    lab, n = ndimage.label(flat)
    if n == 0:
        return np.zeros(h.shape, bool)
    sizes = ndimage.sum(flat, lab, range(1, n + 1))
    big = np.isin(lab, [i + 1 for i, s in enumerate(sizes) if s >= 40])
    return big


def dam_sites(h, flow, acc, stream, cell_m):
    """Candidate dams on streams: a DAM_M-high line across the valley, short, holding a pond."""
    rows, cols = h.shape
    sites = []
    ys, xs = np.nonzero(stream & (acc < 40 * T))
    for y, x in zip(ys[::3], xs[::3]):
        dx, dy = flow[y, x]
        if dx == -1 and dy == -1:
            continue
        top = h[y, x] + DAM_M
        # Line across the flow (perpendicular), cells below the crest that the dam must span.
        px, py = -dy, dx
        length = 1
        for sgn in (1, -1):
            for k in range(1, 20):
                cx, cy = x + sgn * px * k, y + sgn * py * k
                if not (0 <= cx < cols and 0 <= cy < rows) or h[cy, cx] >= top:
                    break
                length += 1
            else:
                length = 99
        if length > 12:
            continue
        # Pond: cells upstream (against the flow) below the crest, connected to the site.
        seen = {(y, x)}
        q = [(y, x)]
        while q and len(seen) < 3000:
            cy, cx = q.pop()
            for ddx, ddy in D8:
                ny, nx = cy + ddy, cx + ddx
                if (ny, nx) in seen or not (0 <= nx < cols and 0 <= ny < rows):
                    continue
                if flow[ny, nx, 0] == -ddx and flow[ny, nx, 1] == -ddy and h[ny, nx] < top:
                    seen.add((ny, nx))
                    q.append((ny, nx))
        # Widen to the whole level-below-crest basin around the flowing cells.
        lab, _ = ndimage.label(h < top)
        basin = lab == lab[y, x]
        pond_cells = len(seen)
        area = min(int(basin.sum()), pond_cells * 6)
        if area >= 30 and area / length >= 6:
            sites.append((y, x, length, area))
    return sites


def lakes_of(h, filled, acc, flat):
    """Standing water: flat patches, filled hollows deeper than 0.5 m, and thick blobs of
    drainage (a stream is a thin line; a lake fills an area)."""
    wet = acc >= T
    thick = ndimage.binary_opening(wet, structure=np.ones((3, 3)))
    hollow = (filled - h) > 0.5
    return flat | hollow | thick


def score(win):
    """What makes a good beaver map (higher is better), with the measurements behind it."""
    h, flow, acc, routed, flat, cell_m = win
    stream = (acc >= T) & ~flat
    lake_frac = flat.mean()
    relief = h.max() - h.min()
    # Streams: count separate reaches (between junctions) that carry water through the window.
    lab, n_reaches = ndimage.label(stream, structure=np.ones((3, 3)))
    sites = dam_sites(h, flow, acc, stream, cell_m)
    # Distinct dammable streams: good sites at least 12 cells apart on different upstream paths.
    picked = []
    for s in sorted(sites, key=lambda s: -s[3] / s[2]):
        if all(math.hypot(s[0] - p[0], s[1] - p[1]) > 12 for p in picked):
            picked.append(s)
    distinct_streams = len({int(acc[p[0], p[1]] // 200) for p in picked})  # rough: different flow sizes = different streams
    # Gradient along stream cells (m per m), from the steepest-descent drop.
    gy, gx = np.gradient(h, cell_m)
    slope = np.hypot(gx, gy)
    sgrad = np.median(slope[stream]) if stream.any() else 0.0
    # River width proxy: cells next to streams that are within 0.5 m of the stream level.
    widen = ndimage.grey_erosion(np.where(stream, h, np.inf), size=(5, 5))
    floor = (h <= widen + 0.5) & ~stream
    width = 1 + floor.sum() / max(1, stream.sum())
    steep_land = (slope * EXAG > 1.5)[~stream].mean()
    s = 0.0
    s += 3.0 * min(len(picked), 6) + 2.0 * min(distinct_streams, 4)
    s -= 120.0 * max(0.0, lake_frac - 0.05)
    s -= 4.0 * max(0.0, width - 3.0)
    s -= 30.0 * max(0.0, abs(math.log(max(sgrad, 1e-4) / 0.015)) - 1.0)
    s -= 20.0 * steep_land
    s -= 0.05 * max(0.0, relief - 150.0)
    if stream.sum() < 60:
        s -= 50
    return s, dict(sites=len(picked), streams=distinct_streams, lake=round(float(lake_frac), 3), width=round(float(width), 2),
                   grad=round(float(sgrad), 4), steep=round(float(steep_land), 3), relief=round(float(relief), 1), picked=picked)


def inflows(acc_full, flow_full, y0, x0):
    """River cells on the window border fed from outside: (x, y, upstream cells outside)."""
    out = []
    for k in range(N):
        for (y, x) in ((y0, x0 + k), (y0 + N - 1, x0 + k), (y0 + k, x0), (y0 + k, x0 + N - 1)):
            if acc_full[y, x] < T:
                continue
            outside = 0
            for dx, dy in D8:
                ny, nx = y + dy, x + dx
                if not (y0 <= ny < y0 + N and x0 <= nx < x0 + N) and 0 <= ny < acc_full.shape[0] and 0 <= nx < acc_full.shape[1]:
                    if flow_full[ny, nx, 0] == -dx and flow_full[ny, nx, 1] == -dy:
                        outside += acc_full[ny, nx]
            if outside >= T:
                out.append((x - x0, y - y0, int(outside)))
    # One inflow per stream: border cells next to each other belong to the same stream.
    out = sorted(set(out), key=lambda t: -t[2])
    kept = []
    for c in out:
        if all(abs(c[0] - k[0]) + abs(c[1] - k[1]) > 3 for k in kept):
            kept.append(c)
    return kept


def main(raw_dir, out_dir, prev_dir):
    os.makedirs(out_dir, exist_ok=True)
    os.makedirs(prev_dir, exist_ok=True)
    for rid, (name, place, lat, lon) in REGIONS.items():
        h0, z = load_region(os.path.join(raw_dir, rid))
        px_m = 40075016.686 * math.cos(math.radians(lat)) / 2 ** (z + 8)
        cands = []
        for s in (1.5, 2.0):
            g = ndimage.zoom(h0, 1 / s, order=1)
            g = ndimage.gaussian_filter(g, 0.6)
            cell_m = px_m * s
            filled, flow, acc, routed = fill_and_route(g)
            flat = lakes_of(g, filled, acc, flat_water(g, cell_m))
            for y0 in range(0, g.shape[0] - N + 1, 16):
                for x0 in range(0, g.shape[1] - N + 1, 16):
                    sl = (slice(y0, y0 + N), slice(x0, x0 + N))
                    sc, info = score((g[sl], flow[sl], acc[sl], routed[sl], flat[sl], cell_m))
                    cands.append((sc, info, s, cell_m, y0, x0, g, flow, acc, flat))
        # Top windows that do not overlap much (by at least half a map), best first.
        cands.sort(key=lambda c: -c[0])
        chosen = []
        for c in cands:
            if all(c[2] != k[2] or abs(c[4] - k[4]) >= N // 2 or abs(c[5] - k[5]) >= N // 2 for k in chosen):
                chosen.append(c)
            if len(chosen) == TOP:
                break
        for rank, best in enumerate(chosen, 1):
            export(rid, rank, best, h0, lat, lon, px_m, name, place, out_dir, prev_dir)


def export(rid, rank, best, h0, lat, lon, px_m, name, place, out_dir, prev_dir):
        sc, info, s, cell_m, y0, x0, g, flow, acc, flat = best
        win = g[y0:y0 + N, x0:x0 + N]
        units = (win - win.min()) / cell_m * EXAG
        if units.max() > 30.0:
            units *= 30.0 / units.max()
        inf = inflows(acc, flow, y0, x0)
        # Real coordinates of the window centre (for the credits and the menu).
        mlat = lat + (h0.shape[0] / 2 - (y0 + N / 2) * s) * px_m / 111320.0
        mlon = lon + ((x0 + N / 2) * s - h0.shape[1] / 2) * px_m / (111320.0 * math.cos(math.radians(lat)))
        header = [
            'BEAVERMAP 1', f'id={rid}', f'name={name}', f'place={place}', f'lat={mlat:.4f}', f'lon={mlon:.4f}',
            f'cell_m={cell_m:.2f}', f'size={N}', f'exaggeration={EXAG}',
            'source=AWS Open Data Terrain Tiles (Terrarium), from SRTM, USGS 3DEP, NRCan CDEM, EU-DEM and others',
            'inflows=' + ';'.join(f'{x},{y},{a}' for x, y, a in inf),
            f'score={sc:.2f}', '---',
        ]
        with open(os.path.join(out_dir, f'{rid}_{rank}.map'), 'wb') as f:
            f.write(('\n'.join(header) + '\n').encode('ascii'))
            f.write(units.astype('<f4').tobytes())
        # Preview: hillshade, streams blue, flat water light blue, chosen dam sites red.
        gy, gx = np.gradient(win)
        shade = np.clip(0.6 + (gx - gy) / (np.abs(gx).max() + np.abs(gy).max() + 1e-6) * 2.0, 0, 1)
        rgb = np.dstack([shade * 200 + 30] * 3)
        rgb[flat[y0:y0 + N, x0:x0 + N]] = [150, 190, 230]
        rgb[acc[y0:y0 + N, x0:x0 + N] >= T] = [30, 90, 200]
        for (yy, xx, ln, ar) in info['picked']:
            rgb[max(0, yy - 1):yy + 2, max(0, xx - 1):xx + 2] = [220, 40, 40]
        Image.fromarray(rgb.astype(np.uint8)).resize((N * 4, N * 4), Image.NEAREST).save(os.path.join(prev_dir, f'{rid}_{rank}.png'))
        info.pop('picked')
        print(f'{rid}_{rank}', f'score={sc:.1f}', f'scale={s}', f'cell_m={cell_m:.1f}', f'window=({x0},{y0})', info, f'inflows={len(inf)}', flush=True)


if __name__ == '__main__':
    main(*sys.argv[1:4])
