import json
import os
import sys

import numpy as np
import pyproj

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import hires_fetch as hf
import hires_maps as hm

N = 512
CELL = 1.0
SNAP = 28

def fetch(rid, data_dir, x0, y1):
    reg = hf.REGIONS[rid]
    hf.CELL_M = CELL
    folder = os.path.join(data_dir, rid)
    if reg['src'] == 'dgm1':
        dem = hf.dgm1(folder, x0, y1, N)
    else:
        dem = np.full((N, N), np.nan, np.float32)
        for r0 in range(0, N, 256):
            for c0 in range(0, N, 256):
                dem[r0:r0 + 256, c0:c0 + 256] = hf.chunk(reg['src'], x0 + c0 * CELL, y1 - r0 * CELL, 256)
    cover = hf.landcover(reg['epsg'], x0, y1, N)
    return dem, cover

def snap(dem, inflows4, n4):
    scale = N / n4
    spans = []
    for x, y, km2 in inflows4:
        cx, cy = int((x + 0.5) * scale), int((y + 0.5) * scale)
        side = min({'w': cx, 'e': N - 1 - cx, 'n': cy, 's': N - 1 - cy}.items(), key=lambda kv: kv[1])[0]
        if side in 'we':
            bx = 0 if side == 'w' else N - 1
            cells = [(bx, yy) for yy in range(max(0, cy - SNAP), min(N, cy + SNAP + 1))]
        else:
            by = 0 if side == 'n' else N - 1
            cells = [(xx, by) for xx in range(max(0, cx - SNAP), min(N, cx + SNAP + 1))]
        spans.append((cells, km2))
    chosen = [(*min(cells, key=lambda c: dem[c[1], c[0]]), km2) for cells, km2 in spans]
    for k, (cells, km2) in enumerate(spans):
        for c in sorted(cells, key=lambda c: dem[c[1], c[0]])[:12]:
            trial = chosen[:k] + [(c[0], c[1], km2)] + chosen[k + 1:]
            path = hm.river_paths(dem, trial)[k]
            if len(path) >= hm.MIN_PATH:
                chosen = trial
                break
    return chosen


def main(rid, map4, data_dir, out_path, preview):
    raw = open(map4, 'rb').read()
    head = raw[:raw.index(b'---\n')].decode('utf-8').split('\n')
    kv = dict(l.split('=', 1) for l in head[1:] if '=' in l)
    n4 = int(kv['size'])
    inflows4 = [(int(a), int(b), float(c)) for a, b, c in (t.split(',') for t in kv['inflows'].split(';') if t)]
    epsg = hf.REGIONS[rid]['epsg']
    cx, cy = pyproj.Transformer.from_crs(4326, epsg, always_xy=True).transform(float(kv['lon']), float(kv['lat']))
    x0, y1 = round(cx - N * CELL / 2), round(cy + N * CELL / 2)
    dem, cover = fetch(rid, data_dir, x0, y1)
    if not np.isfinite(dem).all():
        raise SystemExit(f'{rid}: {int((~np.isfinite(dem)).sum())} cells without data')
    hm.N = N
    hm.CELL_M = CELL
    hm.INFLOW_WALL = 12
    hm.MIN_PATH, hm.MIN_OWN, hm.MAX_STEEP_RUN = 192, 96, 40
    heights = hm.map_heights(dem)
    inflows = snap(heights, inflows4, n4)
    ok, why = hm.fine_check(heights, inflows)
    print(rid, 'check', ok, why, inflows, flush=True)
    header = [
        'BEAVERMAP 2', f'id={rid}', f'name={kv["name"]}', f'place={kv["place"]}', f'lat={kv["lat"]}', f'lon={kv["lon"]}',
        f'cell_m={CELL:.2f}', f'size={N}', 'height_unit_m=1', f'source={kv["source"]}', 'layers=height,cover',
        'inflows=' + ';'.join(f'{x},{y},{a:.2f}' for x, y, a in inflows), '---',
    ]
    with open(out_path, 'wb') as f:
        f.write(('\n'.join(header) + '\n').encode('utf-8'))
        f.write(heights.tobytes())
        f.write(cover.astype(np.uint8).tobytes())
    from PIL import Image
    gy, gx = np.gradient(heights)
    shade = np.clip(0.6 + (gx - gy) * 1.2, 0, 1)
    rgb = np.dstack([shade * 200 + 30] * 3)
    rgb[cover == 10] *= [0.7, 1.0, 0.7]
    for p in hm.river_paths(heights, inflows):
        rgb.reshape(-1, 3)[p] = [30, 90, 210]
    Image.fromarray(rgb.astype(np.uint8)).save(preview)

if __name__ == '__main__':
    main(*sys.argv[1:6])
