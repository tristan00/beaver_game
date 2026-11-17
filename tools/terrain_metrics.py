"""Measure a generated map dump against the numeric targets in docs/TERRAIN_TARGETS.md.

  cargo run -p sim --example terrain_dump -- --seed 1 --out tmp/t1.json
  python tools/terrain_metrics.py tmp/t1.json [--cell-m 10] [--height-m 1] [--strict]

Prints each metric, its measured value, the target range and OK / LOW / HIGH. Exit 0 unless --strict
and a metric is out of range. Needs numpy and scipy. Reads only the dump named on the command line.
Scale assumptions (docs/TERRAIN_TARGETS.md section 1): 1 cell = --cell-m metres, 1 height unit = --height-m metres.
"""
import argparse, json, math, sys
import numpy as np
from scipy import ndimage as ndi

# name: (low, high, unit, source)  Every range is the span of values measured on the three real sites by
# tools/real_landscape.py (M1-M3 in docs/TERRAIN_TARGETS.md) or read from a cited paper. Keep in step with the doc.
TARGETS = {
    "water_fraction":       (0.10, 0.28, "fraction of map", "M2 0.113, M1 0.284; F1 0.13, F11 0.10"),
    "pond_median_area":     (0.13, 0.62, "ha", "M3 0.13, M1 0.435, M2 0.62"),
    "pond_mean_area":       (0.42, 2.9, "ha", "M3 0.42, M2 2.80, M1 2.90"),
    "pond_spacing_median":  (150.0, 270.0, "m (nearest pond centre)", "M3 155, M1 261, M2 268"),
    "valley_floor_width":   (80.0, 230.0, "m", "M1 80/120, M2 225/130 (DTM)"),
    "valley_depth":         (11.0, 24.0, "m", "M2 11.5/23.4, M1 22.1/11.8 (DTM)"),
    "floor_slope_median":   (0.9, 6.5, "% grade, valley floor", "M3 0.9/1.1, M1 2.6, M2 5.6/6.5"),
    "slope_p90":            (6.8, 24.2, "% grade, all land", "M3 6.8, M1 18.5, M2 24.2"),
    "river_sinuosity":      (1.09, 1.22, "path/straight, 300 m windows", "M1 1.09/1.11, M3 1.15/1.22, M2 1.22/1.16"),
    "river_width":          (10.0, 30.0, "m, main river", "M1, M2 < 10 (floored to 1 cell), M3 30"),
    "forest_fraction":      (0.90, 0.98, "fraction of land", "M1 0.93, M2 0.98 (M3 0.22: marsh)"),
    "clearing_median_area": (0.30, 0.43, "ha, patches >= 0.1 ha", "M2 0.30, M3 0.33, M1 0.43"),
}

CLEARING_MIN_HA = 0.1


def load(path):
    with open(path) as f: d = json.load(f)
    n = d["size"]
    g = {k: np.array(v).reshape(n, n) for k, v in d.items() if isinstance(v, list)}
    return d, n, g

def sizes_ha(mask, cell_ha):
    lab, k = ndi.label(mask)
    return (np.bincount(lab.ravel())[1:] * cell_ha) if k else np.array([]), lab, k

def main_path(g, n):
    """Ordered cells of the main river, upstream to downstream.

    The main_river mask can have several headless cells (a side arm or a cell whose flow leaves and
    re-enters the mask). Walk down from every one and keep the longest chain; ties go to the start
    with the larger flow accumulation when the dump has `flow_acc`.
    """
    mr = g["main_river"].astype(bool).ravel(); ft = g["flow_to"].ravel()
    cells = np.flatnonzero(mr)
    if not len(cells): return []
    has_up = set(int(ft[c]) for c in cells if ft[c] >= 0 and mr[ft[c]])
    acc = g["flow_acc"].ravel() if "flow_acc" in g else np.zeros(n * n)
    best = []
    for c in (int(c) for c in cells if int(c) not in has_up):
        path, seen = [c], {c}
        while ft[c] >= 0 and mr[ft[c]] and int(ft[c]) not in seen:
            c = int(ft[c]); path.append(c); seen.add(c)
        if (len(path), acc[path[0]]) > (len(best), acc[best[0]] if best else -1): best = path
    return [(p % n, p // n) for p in best]

def sinuosity(path, win=30):
    vals = []
    for i in range(0, max(1, len(path) - win), win // 2):
        seg = path[i:i + win + 1]
        if len(seg) < win // 2: continue
        L = sum(math.hypot(a[0]-b[0], a[1]-b[1]) for a, b in zip(seg, seg[1:]))
        D = math.hypot(seg[0][0]-seg[-1][0], seg[0][1]-seg[-1][1])
        if D > 0: vals.append(L / D)
    return vals

def valley_profile(g, n, path, cell_m, hm, wall=0.10):
    """Per main-river cell, scan both axes outward: floor ends where grade exceeds `wall`;
    depth = rise from the river bed to the highest height before the terrain falls again (cap 60 cells)."""
    h = g["height"] * hm; sl = g["slope"] * hm / cell_m
    widths, depths = [], []
    for (x, y) in path[::3]:
        best = None
        for dx, dy in ((1, 0), (0, 1)):
            ext = []
            for s in (1, -1):
                k = 0
                while k < 60:
                    xx, yy = x + s*dx*(k+1), y + s*dy*(k+1)
                    if not (0 <= xx < n and 0 <= yy < n) or sl[yy, xx] > wall: break
                    k += 1
                ext.append(k)
            w = (ext[0] + ext[1] + 1) * cell_m
            if best is None or w < best[0]:
                rim = 0.0
                for s in (1, -1):
                    top = h[y, x]
                    for k in range(1, 61):
                        xx, yy = x + s*dx*k, y + s*dy*k
                        if not (0 <= xx < n and 0 <= yy < n): break
                        if h[yy, xx] < top - 0.5: break
                        top = max(top, h[yy, xx])
                    rim = max(rim, top - h[y, x])
                best = (w, rim)
        widths.append(best[0]); depths.append(best[1])
    return widths, depths

def measure(g, n, cell_m, hm, path=None):
    """All metrics for one map. `path` overrides the main-river path (used for real-data grids)."""
    cell_ha = cell_m * cell_m / 1e4
    wet = (g["river"] | g["lake"]).astype(bool)
    lake = g["lake"].astype(bool); river = g["river"].astype(bool)
    land = ~wet
    out = {}
    out["water_fraction"] = wet.mean()
    ps, lab, k = sizes_ha(lake, cell_ha)
    out["pond_median_area"] = float(np.median(ps)) if k else float("nan")
    out["pond_mean_area"] = float(ps.mean()) if k else float("nan")
    if k >= 2:
        cen = np.array(ndi.center_of_mass(lake, lab, range(1, k + 1)))
        d = np.hypot(*(cen[:, None, :] - cen[None, :, :]).transpose(2, 0, 1)); np.fill_diagonal(d, np.inf)
        out["pond_spacing_median"] = float(np.median(d.min(1)) * cell_m)
    else: out["pond_spacing_median"] = float("nan")
    if path is None: path = main_path(g, n)
    w, dpt = valley_profile(g, n, path, cell_m, hm) if path else ([], [])
    out["valley_floor_width"] = float(np.median(w)) if w else float("nan")
    out["valley_depth"] = float(np.median(dpt)) if dpt else float("nan")
    sl = g["slope"] * hm / cell_m * 100
    fl = np.zeros((n, n), bool)
    for (x, y) in path: fl[y, x] = True
    floor = ndi.binary_dilation(fl, iterations=4) & land
    out["floor_slope_median"] = float(np.median(sl[floor])) if floor.any() else float("nan")
    out["slope_p90"] = float(np.percentile(sl[land], 90))
    sv = sinuosity(path)
    out["river_sinuosity"] = float(np.median(sv)) if sv else float("nan")
    wd = []
    for (x, y) in path[::3]:
        wd.append(min(river[y, max(0, x-3):x+4].sum(), river[max(0, y-3):y+4, x].sum()))
    out["river_width"] = float(np.median(wd) * cell_m) if wd else float("nan")
    tree = g["forest"] > 0
    out["forest_fraction"] = float(tree[land].mean())
    # Clearing = connected dry non-forest patch of at least CLEARING_MIN_HA; smaller gaps are mostly
    # single-pixel classification noise in the 10 m land-cover data the target was measured on.
    cs, _, _ = sizes_ha(land & ~tree, cell_ha)
    cs = cs[cs >= CLEARING_MIN_HA - 1e-9]; kc = len(cs)
    out["clearing_median_area"] = float(np.median(cs)) if kc else float("nan")
    out["_counts"] = dict(ponds=int(k), clearings=int(kc), main_river_cells=len(path))
    return out

def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("dump"); ap.add_argument("--cell-m", type=float, default=10.0)
    ap.add_argument("--height-m", type=float, default=1.0); ap.add_argument("--strict", action="store_true")
    a = ap.parse_args()
    d, n, g = load(a.dump)
    m = measure(g, n, a.cell_m, a.height_m)
    print(f"seed {d['seed']}  {n}x{n} cells  cell={a.cell_m:g} m  height unit={a.height_m:g} m  {m.pop('_counts')}")
    print(f"{'metric':22} {'measured':>10}  {'target range':>15}  {'':5} unit [source, docs/TERRAIN_TARGETS.md]")
    bad = 0
    for k, (lo, hi, unit, src) in TARGETS.items():
        v = m[k]
        st = "n/a" if v != v else "OK" if lo <= v <= hi else "LOW" if v < lo else "HIGH"
        bad += st in ("LOW", "HIGH", "n/a")
        print(f"{k:22} {v:10.3f}  {lo:7g}..{hi:<7g}  {st:5} {unit} [{src}]")
    sys.exit(1 if a.strict and bad else 0)

if __name__ == "__main__":
    main()
