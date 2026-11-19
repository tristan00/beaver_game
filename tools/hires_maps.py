"""Choose beaver map windows from fine elevation and export them as game maps.

  python tools/hires_maps.py DATA_DIR OUT_DIR PREVIEW_DIR [region ...]

DATA_DIR holds `hires_fetch.py`'s 4 m grids (DATA_DIR/<region>/dem.npy + dem.json). A map is a
128 x 128 window of 4 m cells: 512 m a side, about one beaver family's stretch of stream. At that
scale a stream only flows all year if it drains a real catchment (a few tenths of a square
kilometre or more), which no 512 m window holds on its own: every river on a map enters from
upstream across the map edge and leaves downstream across another. So the drainage is routed
over the whole region (8 m cells), and a window is a good beaver map when:

  * at least two dammable streams enter it from outside (catchments A_MIN..A_MAX km2: big
    enough to flow, small enough for a beaver to dam; no bigger river crosses it);
  * the streams run gently (median gradient GRAD_RANGE) and do not hug the map edge;
  * there is valley floor beside them a beaver can reach (ground a few metres above the stream
    within a couple of hundred metres), little open water, little steep ground and modest relief;
  * it is natural ground (ESA WorldCover: almost no cropland or built-up land) with a mix of
    forest and open land (TREE_RANGE of it under trees).

The best windows of each region (not overlapping) are written as OUT_DIR/<region>_<rank>.map:
a BEAVERMAP 2 text header (cell size, real coordinates, the inflows as border cell + catchment
in km2) and 128 x 128 little-endian f32 heights in metres. PREVIEW_DIR gets a hillshade PNG of
each with the streams and inflows marked.
"""
import heapq
import json
import math
import os
import sys

import numpy as np
import pyproj
from PIL import Image
from scipy import ndimage

N = 128                      # map side, cells
CELL_M = 4.0                 # map cell, metres
COARSE = 2                   # routing cells are COARSE x COARSE map cells (8 m)
A_MIN, A_MAX = 0.2, 25.0     # dammable stream catchments, km2
GRAD_RANGE = (0.002, 0.04)   # median stream gradient, m per m
MAX_RELIEF = 40.0            # metres
TREE_RANGE = (0.35, 0.9)     # share of the window under trees
MAX_FARMED = 0.03            # share of cropland and built-up land
TOP = int(os.environ.get('MAPS_TOP', 4))                     # windows exported per region
BASE = 3.0                   # metres added under the lowest cell (room to carve channels)
D8 = [(-1, -1), (0, -1), (1, -1), (-1, 0), (1, 0), (-1, 1), (0, 1), (1, 1)]


def fill_and_route(h, closed=()):
    """Priority-flood fill (tiny gradient on flats), D8 flow on the filled surface and flow
    accumulation in cells. Cells with no data (NaN) are walls; flow to them is not routed. Border
    cells in `closed` (flat indices) are not outlets: a river enters there from upstream, so the
    ground beyond is higher and water cannot leave by them."""
    rows, cols = h.shape
    closed = set(closed)
    hv = np.where(np.isfinite(h), h, np.inf).ravel()
    filled = np.full(rows * cols, np.inf)
    done = np.zeros(rows * cols, bool)
    flow = np.full(rows * cols, -1, np.int64)
    pq = []
    for y in range(rows):
        for x in range(cols):
            if y in (0, rows - 1) or x in (0, cols - 1):
                i = y * cols + x
                if np.isfinite(hv[i]) and i not in closed:
                    pq.append((hv[i], i))
                    done[i] = True
                    filled[i] = hv[i]
    heapq.heapify(pq)
    offs = [(dx, dy, dy * cols + dx) for dx, dy in D8]
    while pq:
        e, i = heapq.heappop(pq)
        y, x = divmod(i, cols)
        for dx, dy, o in offs:
            nx, ny = x + dx, y + dy
            if 0 <= nx < cols and 0 <= ny < rows:
                j = i + o
                if not done[j] and np.isfinite(hv[j]):
                    done[j] = True
                    v = max(hv[j], e + 1e-4)
                    filled[j] = v
                    flow[j] = i  # the cell it was reached from drains it (always lower)
                    heapq.heappush(pq, (v, j))
    # Steepest descent where it is defined (keeps routing out of filled pits the flood order chose).
    f2 = filled.reshape(rows, cols)
    pad = np.pad(f2, 1, constant_values=np.inf)
    # (an outlet border cell may also drain off the map: only an inner neighbour lower than it
    # can take its flow, so `best` starts at zero)
    best = np.zeros((rows, cols))
    tgt = flow.reshape(rows, cols).copy()
    yy, xx = np.mgrid[0:rows, 0:cols]
    for dx, dy in D8:
        nb = pad[1 + dy:1 + dy + rows, 1 + dx:1 + dx + cols]
        drop = (f2 - nb) / math.hypot(dx, dy)
        better = np.isfinite(drop) & (drop > best)
        best = np.where(better, drop, best)
        tgt = np.where(better, (yy + dy) * cols + (xx + dx), tgt)
    flow = tgt.ravel()
    order = np.argsort(-np.where(np.isfinite(filled), filled, -np.inf))
    acc = np.ones(rows * cols)
    for i in order:
        t = flow[i]
        if t >= 0 and np.isfinite(filled[i]):
            acc[t] += acc[i]
    return filled.reshape(rows, cols), flow, acc.reshape(rows, cols)


def height_above_stream(h, filled, flow, stream):
    """For each cell, the height above and the flow-path distance (cells) to the first stream
    cell its water reaches ('height above nearest drainage')."""
    rows, cols = h.shape
    ref = np.full(rows * cols, -1, np.int64)
    dist = np.full(rows * cols, np.inf)
    s = stream.ravel()
    order = np.argsort(np.where(np.isfinite(filled), filled, np.inf).ravel())
    for i in order:
        if s[i]:
            ref[i], dist[i] = i, 0.0
        else:
            t = flow[i]
            if t >= 0 and ref[t] >= 0:
                ref[i], dist[i] = ref[t], dist[t] + 1.0
    hv = h.ravel()
    hand = np.where(ref >= 0, hv - hv[np.maximum(ref, 0)], np.inf)
    return hand.reshape(rows, cols), dist.reshape(rows, cols)


def block_mean(a, k):
    r, c = a.shape[0] // k * k, a.shape[1] // k * k
    return np.nanmean(a[:r, :c].reshape(r // k, k, c // k, k), axis=(1, 3))


def scan(rid, data_dir):
    meta = json.load(open(os.path.join(data_dir, rid, 'dem.json')))
    dem = np.load(os.path.join(data_dir, rid, 'dem.npy')).astype(np.float64)
    cover = np.load(os.path.join(data_dir, rid, 'cover.npy'))
    import warnings
    with warnings.catch_warnings():
        warnings.simplefilter('ignore')
        g = block_mean(dem, COARSE)
    cm = CELL_M * COARSE
    filled, flow, acc = fill_and_route(g)
    km2 = acc * cm * cm / 1e6
    stream = (km2 >= A_MIN) & np.isfinite(g)
    # Streams whose catchment reaches the edge of the downloaded area have an unknown (maybe much
    # bigger) catchment: windows they cross are not used.
    unknown = np.zeros(g.size, bool)
    unknown.reshape(g.shape)[[0, -1], :] = True
    unknown.reshape(g.shape)[:, [0, -1]] = True
    for i in np.argsort(-np.where(np.isfinite(filled), filled, -np.inf).ravel()):
        t = flow[i]
        if t >= 0 and unknown[i]:
            unknown[t] = True
    hand, hdist = height_above_stream(g, filled, flow, stream)
    gy, gx = np.gradient(np.where(np.isfinite(g), g, np.nanmean(g)), cm)
    slope = np.hypot(gx, gy)
    # Open water in a lidar model is flat (water surfaces are flattened): big level patches.
    flat = slope < 0.002
    lab, nl = ndimage.label(flat)
    sizes = ndimage.sum(flat, lab, range(1, nl + 1)) if nl else []
    water = np.isin(lab, [k + 1 for k, s in enumerate(sizes) if s >= 25]) | ((filled - g) > 0.5)
    rows, cols = g.shape
    # Stream edges c -> t (flat indices) with their catchment and gradient.
    sc = np.flatnonzero(stream.ravel() & (flow >= 0))
    st = flow[sc]
    scy, scx, sty, stx = sc // cols, sc % cols, st // cols, st % cols
    s_km2 = km2.ravel()[sc]
    s_unknown = unknown[sc]
    s_grad = (filled.ravel()[sc] - filled.ravel()[st]) / (np.hypot(scx - stx, scy - sty) * cm)
    w = N // COARSE
    cands = []
    for y0 in range(0, rows - w + 1, 4):
        for x0 in range(0, cols - w + 1, 4):
            win = (slice(y0, y0 + w), slice(x0, x0 + w))
            if not np.isfinite(g[win]).all():
                continue
            relief = float(g[win].max() - g[win].min())
            if relief > MAX_RELIEF:
                continue
            cin = (scx >= x0) & (scx < x0 + w) & (scy >= y0) & (scy < y0 + w)
            tin = (stx >= x0) & (stx < x0 + w) & (sty >= y0) & (sty < y0 + w)
            enter = ~cin & tin
            if (s_unknown & (cin | enter)).any():
                continue
            if enter.sum() < 2 or s_km2[cin].max(initial=0) > A_MAX or s_km2[enter].max(initial=0) > A_MAX:
                continue
            # one inflow per stream: crossings next to each other are the same stream
            ins = sorted(zip(s_km2[enter], stx[enter], sty[enter]), reverse=True)
            kept = []
            for a, x, y in ins:
                if all(abs(x - k[1]) + abs(y - k[2]) > 6 for k in kept):
                    kept.append((a, x, y))
            if len(kept) < 2:
                continue
            leave = cin & ~tin
            if not leave.any():
                continue
            # a river that leaves and comes back in would bring its water in twice
            back = False
            for t in st[leave]:
                for _ in range(80):
                    t = flow[t]
                    if t < 0:
                        break
                    if x0 <= t % cols < x0 + w and y0 <= t // cols < y0 + w:
                        back = True
                        break
                if back:
                    break
            if back:
                continue
            inside = cin & tin
            grad = float(np.median(s_grad[cin])) if cin.any() else 0.0
            if not GRAD_RANGE[0] <= grad <= GRAD_RANGE[1]:
                continue
            # streams hugging the edge (away from where they cross it)
            ex, ey = scx[inside] - x0, scy[inside] - y0
            edge = np.minimum.reduce([ex, ey, w - 1 - ex, w - 1 - ey]) <= 1
            crossings = [(k[1] - x0, k[2] - y0) for k in kept] + list(zip(stx[leave] - x0, sty[leave] - y0)) + list(zip(scx[leave] - x0, scy[leave] - y0))
            hug = sum(1 for x, y in zip(ex[edge], ey[edge]) if all(abs(x - cx) + abs(y - cy) > 4 for cx, cy in crossings))
            cw = cover[y0 * COARSE:(y0 + w) * COARSE, x0 * COARSE:(x0 + w) * COARSE]
            farmed = float(np.isin(cw, (40, 50)).mean())
            trees = float((cw == 10).mean())
            if farmed > MAX_FARMED or not TREE_RANGE[0] <= trees <= TREE_RANGE[1]:
                continue
            lake = float(water[win].mean())
            steep = float((slope[win] > 0.4).mean())
            floor = float(((hand[win] < 2.0) & (hdist[win] < 25)).mean())
            length = int(inside.sum())
            score = (5.0 * min(len(kept), 4) + 20.0 * min(floor, 0.5) + 0.03 * min(length, 300)
                     - 60.0 * max(0.0, lake - 0.04) - 30.0 * steep - 0.5 * hug - 0.1 * max(0.0, relief - 20.0)
                     - 15.0 * abs(math.log(grad / 0.01)))
            cands.append((score, y0, x0, dict(inflows=[(round(a, 2), x - x0, y - y0) for a, x, y in kept], grad=round(grad, 4),
                                               lake=round(lake, 3), steep=round(steep, 3), floor=round(floor, 3), relief=round(relief, 1),
                                               length=length, hug=hug, trees=round(trees, 2), farmed=round(farmed, 3))))
    cands.sort(key=lambda c: -c[0])
    return meta, dem, cover, cands



def fine_inflows(win4, coarse_inflows):
    """Inflows on the 4 m window border: for each coarse crossing, the lowest border cell on the
    same side within a few cells (the stream's channel), with its catchment."""
    out = []
    for a, cx, cy in coarse_inflows:
        x, y = cx * COARSE + COARSE // 2, cy * COARSE + COARSE // 2
        dists = {'w': x, 'e': N - 1 - x, 'n': y, 's': N - 1 - y}
        side = min(dists, key=dists.get)
        if side in 'we':
            bx = 0 if side == 'w' else N - 1
            cells = [(bx, yy) for yy in range(max(0, y - 5), min(N, y + 6))]
        else:
            by = 0 if side == 'n' else N - 1
            cells = [(xx, by) for xx in range(max(0, x - 5), min(N, x + 6))]
        bx, by = min(cells, key=lambda c: win4[c[1], c[0]])
        if all(abs(bx - o[0]) + abs(by - o[1]) > 4 for o in out):
            out.append((bx, by, a))
    return out


MIN_PATH = 48      # cells a river runs inside the map before it leaves
MIN_OWN = 24       # cells a second stream runs on its own before it joins another
STEEP = 0.05       # a river reach steeper than this (m per m) runs as a thin film in the game
MAX_STEEP = 0.15   # share of the river cells that may be that steep (riffles)...
MAX_STEEP_RUN = 10 # ...and the longest steep run (cells): a steep creek is no beaver stream
INFLOW_WALL = 3    # border cells either side of an inflow that are not outlets


def closed_cells(inflows):
    out = []
    for x, y, _ in inflows:
        for k in range(-INFLOW_WALL, INFLOW_WALL + 1):
            for cx, cy in ((x + k, y), (x, y + k)):
                if 0 <= cx < N and 0 <= cy < N and (cx in (0, N - 1) or cy in (0, N - 1)):
                    out.append(cy * N + cx)
    return out


def game_route(h, closed):
    """The game's own drainage (`worldgen::fill_depressions`): a priority flood from the open
    border cells in float32 with a 1 mm step, each cell draining to the cell it was reached from.
    The window checks use it so a river runs here exactly where it will run in the game."""
    n = h.shape[0]
    hv = h.astype(np.float32).ravel()
    surface = hv.copy()
    flow = np.full(n * n, -1, np.int64)
    done = np.zeros(n * n, bool)
    closed = set(closed)
    pq = []
    for i in range(n * n):
        x, y = i % n, i // n
        if (x in (0, n - 1) or y in (0, n - 1)) and i not in closed:
            done[i] = True
            pq.append((float(surface[i]), i))
    heapq.heapify(pq)
    eps = np.float32(1e-3)
    while pq:
        _, c = heapq.heappop(pq)
        cx, cy = c % n, c // n
        for dy in (-1, 0, 1):
            for dx in (-1, 0, 1):
                nx, ny = cx + dx, cy + dy
                if (dx or dy) and 0 <= nx < n and 0 <= ny < n:
                    j = ny * n + nx
                    if not done[j]:
                        done[j] = True
                        surface[j] = max(hv[j], np.float32(surface[c] + eps))
                        flow[j] = c
                        heapq.heappush(pq, (float(surface[j]), j))
    return surface.reshape(n, n), flow


def river_paths(win, inflows):
    """Each inflow's river through the window: the cells from its border cell down the game's
    drainage (inflow edges closed) to where it leaves the map."""
    _, flow = game_route(win, closed_cells(inflows))
    paths = []
    for x, y, _ in inflows:
        i, path = y * N + x, []
        while i >= 0 and len(path) < N * N:
            path.append(i)
            i = flow[i]
        paths.append(path)
    return paths


def fine_check(win, inflows):
    """Whether the rivers really run through the window at 4 m: every inflow's river runs at
    least MIN_PATH cells before it leaves, and at least two of them run MIN_OWN cells on their own
    (separate streams, not one stream entering twice)."""
    if len(inflows) < 2:
        return False, 'fewer than two inflows'
    paths = river_paths(win, inflows)
    short = [len(p) for p in paths if len(p) < MIN_PATH]
    if short:
        return False, f'river paths too short: {short}'
    own, seen = 0, set()
    for p in sorted(paths, key=len, reverse=True):
        run = 0
        for i in p:
            if i in seen:
                break
            run += 1
        own += run >= MIN_OWN
        seen.update(p)
    if own < 2:
        return False, 'only one separate stream'
    # how steep the rivers run, over 3 cells (from the filled surface the game's drainage uses)
    filled, _ = game_route(win, closed_cells(inflows))
    f = filled.ravel().astype(np.float64)
    steep = total = longest = 0
    for p in paths:
        run = 0
        for a, b in zip(p, p[3:]):
            ay, ax, by, bx = a // N, a % N, b // N, b % N
            total += 1
            s = (f[a] - f[b]) / (math.hypot(ax - bx, ay - by) * CELL_M) > STEEP
            steep += s
            run = run + 1 if s else 0
            longest = max(longest, run)
    if steep > MAX_STEEP * max(total, 1) or longest > MAX_STEEP_RUN:
        return False, f'{steep} of {total} river cells steeper than {STEEP:.0%}, longest run {longest}'
    return True, ''


def map_heights(win):
    """A window's heights as the map file holds them (metres above its lowest cell plus BASE)."""
    return (win - win.min() + BASE).astype('<f4')


def export(rid, rank, meta, dem, cover, cand, inflows, out_dir, prev_dir, name, place):
    score, y0, x0, info = cand
    Y0, X0 = y0 * COARSE, x0 * COARSE
    win = dem[Y0:Y0 + N, X0:X0 + N]
    heights = map_heights(win)
    land = cover[Y0:Y0 + N, X0:X0 + N].astype(np.uint8)
    cx = meta['x0'] + (X0 + N / 2) * CELL_M
    cy = meta['y1'] - (Y0 + N / 2) * CELL_M
    lon, lat = pyproj.Transformer.from_crs(meta['epsg'], 4326, always_xy=True).transform(cx, cy)
    header = [
        'BEAVERMAP 2', f'id={rid}', f'name={name}', f'place={place}', f'lat={lat:.5f}', f'lon={lon:.5f}',
        f'cell_m={CELL_M:.2f}', f'size={N}', 'height_unit_m=1',
        f'source={meta["source"]}; land cover ESA WorldCover 2021 (CC BY 4.0)',
        'layers=height,cover',
        'inflows=' + ';'.join(f'{x},{y},{a:.2f}' for x, y, a in inflows),
        f'score={score:.2f}', '---',
    ]
    with open(os.path.join(out_dir, f'{rid}_{rank}.map'), 'wb') as f:
        f.write(('\n'.join(header) + '\n').encode('utf-8'))
        f.write(heights.tobytes())
        f.write(land.tobytes())
    # Preview: hillshade (forest tinted green), the rivers from the inflows blue, inflows red.
    gy, gx = np.gradient(win)
    shade = np.clip(0.6 + (gx - gy) * 0.35, 0, 1)
    rgb = np.dstack([shade * 200 + 30] * 3)
    rgb[land == 10] *= [0.7, 1.0, 0.7]
    for p in river_paths(heights, inflows):
        rgb.reshape(-1, 3)[p] = [30, 90, 210]
    for x, y, _ in inflows:
        rgb[max(0, y - 1):y + 2, max(0, x - 1):x + 2] = [220, 40, 40]
    Image.fromarray(rgb.astype(np.uint8)).resize((N * 4, N * 4), Image.NEAREST).save(os.path.join(prev_dir, f'{rid}_{rank}.png'))
    print(f'{rid}_{rank} score={score:.1f} at ({lat:.5f}, {lon:.5f})', {k: v for k, v in info.items() if k != 'inflows'}, 'inflows', [(x, y, round(float(a), 2)) for x, y, a in inflows], flush=True)


REGION_NAMES = {
    'algonquin': ('Algonquin Park', 'Ontario, Canada'),
    'algonquin_b': ('Algonquin Park', 'Ontario, Canada'),
    'voyageurs': ('Voyageurs', 'Minnesota, USA'),
    'bavarian': ('Bavarian Forest', 'Bavaria, Germany'),
    'telemark': ('Telemark', 'Norway'),
}


def main(data_dir, out_dir, prev_dir, regions):
    os.makedirs(out_dir, exist_ok=True)
    os.makedirs(prev_dir, exist_ok=True)
    for rid in regions or REGION_NAMES:
        if not os.path.exists(os.path.join(data_dir, rid, 'dem.npy')):
            print(rid, 'no data', flush=True)
            continue
        meta, dem, cover, cands = scan(rid, data_dir)
        w = N // COARSE
        chosen, rejected = [], 0
        for c in cands:
            if not all(abs(c[1] - k[1]) >= w * 3 // 4 or abs(c[2] - k[2]) >= w * 3 // 4 for k in chosen):
                continue
            win = map_heights(dem[c[1] * COARSE:c[1] * COARSE + N, c[2] * COARSE:c[2] * COARSE + N])
            inflows = fine_inflows(win, c[3]['inflows'])
            ok, why = fine_check(win, inflows)
            if not ok:
                rejected += 1
                if os.environ.get('MAPS_WHY'):
                    print(f'  rejected ({c[2] * COARSE}, {c[1] * COARSE}) score {c[0]:.1f}: {why}', flush=True)
                continue
            chosen.append(c)
            export(rid, len(chosen), meta, dem, cover, c, inflows, out_dir, prev_dir, *REGION_NAMES[rid])
            if len(chosen) == TOP:
                break
        print(f'{rid}: {len(cands)} candidate windows, {rejected} failed the 4 m check, {len(chosen)} exported', flush=True)


if __name__ == '__main__':
    main(sys.argv[1], sys.argv[2], sys.argv[3], sys.argv[4:])
