import argparse
import numpy as np
from PIL import Image

T = {"Meadow light": 0xA8D878, "Meadow": 0x85C468, "Meadow deep": 0x5FA85A, "Straw": 0xD9C27A, "Stone": 0xB9AFA8}
PLUM = np.array([0x6B, 0x5A, 0x8A], float)
TOL = 12
PASS_FRAC = 0.95
MIN_PX = 30

ap = argparse.ArgumentParser()
ap.add_argument("lit")
ap.add_argument("measured")
ap.add_argument("mask")
ap.add_argument("--glow", action="store_true")
ap.add_argument("--out")
ap.add_argument("--title", default="")
a = ap.parse_args()

def rgb(h):
    return np.array([(h >> 16) & 255, (h >> 8) & 255, h & 255], float)

def load(p):
    return np.asarray(Image.open(p).convert("RGB")).astype(float)

def hexs(v):
    return "-" if v is None else "#%02X%02X%02X" % tuple(int(round(x)) for x in v)

def is_col(img, c):
    return np.all(np.abs(img - np.array(c, float) * 255) < 6, axis=2)

def core(m):
    out = m.copy()
    for dy in (-1, 0, 1):
        for dx in (-1, 0, 1):
            out &= np.roll(np.roll(m, dy, 0), dx, 1)
    out[0, :] = out[-1, :] = out[:, 0] = out[:, -1] = False
    return out

lit, meas, mk = load(a.lit), load(a.measured), load(a.mask)
H, W, _ = lit.shape
yy, xx = np.mgrid[0:H, 0:W]
skip = np.zeros((H, W), bool)
skip[:50, :] = True
skip[H - 80:, :] = True
if a.glow:
    skip |= np.hypot(xx, yy) < 0.72 * W

shadow = core(is_col(mk, (1, 0, 1))) & ~skip
lit_t = core(is_col(mk, (1, 1, 0))) & ~skip
penumbra = is_col(mk, (0, 1, 1)) & ~skip

hexes = np.stack([rgb(h) for h in T.values()])
d_lit = np.abs(lit[:, :, None, :] - hexes[None, None]).max(axis=3)
near = d_lit.argmin(axis=2)
ts = np.linspace(0.35, 0.45, 21)

rows, ok_all = [], True
for k, (name, h) in enumerate(T.items()):
    c = rgb(h)
    tg = np.stack([c + t * (PLUM - c) for t in ts])
    cls = (near == k) & (d_lit[..., k] <= TOL)

    lpx = meas[lit_t & (near == k) & (d_lit[..., k] <= TOL - 1)]
    ln = len(lpx)
    lfrac = (np.abs(lpx - c).max(axis=1) <= TOL).mean() if ln else None
    lmean = lpx.mean(0) if ln else None
    lit_ok = ln > 0 and lfrac >= PASS_FRAC and np.abs(lmean - c).max() <= TOL

    sm = shadow & cls
    spx = meas[sm]
    n = len(spx)
    samples = []
    if n:
        err = np.abs(spx[:, None, :] - tg[None]).max(axis=2)
        frac = (err.min(axis=1) <= TOL).mean()
        worst = err.min(axis=1).max()
        mean = spx.mean(0)
        t_est = float(np.clip(np.dot(mean - c, PLUM - c) / np.dot(PLUM - c, PLUM - c), 0, 1))

        t_own = float(np.dot(mean - lmean, PLUM - lmean) / np.dot(PLUM - lmean, PLUM - lmean)) if ln else None
        mean_ok = np.abs(mean[None] - tg).max(axis=1).min() <= TOL
        ys, xs = np.nonzero(sm)
        for i in np.linspace(0, n - 1, 3).astype(int):
            j = err[i].argmin()
            samples.append(((int(xs[i]), int(ys[i])), spx[i], tg[j], ts[j]))
    else:
        frac = mean = worst = t_own = None
        t_est, mean_ok = 0.0, False
    shadow_ok = n > 0 and frac >= PASS_FRAC and mean_ok
    if (n >= MIN_PX and not shadow_ok) or (ln >= MIN_PX and not lit_ok):
        ok_all = False
    rows.append(dict(name=name, hex=h, ln=ln, lmean=lmean, lfrac=lfrac, n=n, mean=mean, t=t_est, frac=frac, worst=worst, t_own=t_own,
                     t35=tg[0], t40=c + 0.40 * (PLUM - c), t45=tg[-1], samples=samples))

pc = lambda f: "-" if f is None else f"{f * 100:.1f}%"
pc0 = lambda f: "-" if f is None else f"{f * 100:.0f}%"
out = []
if a.title:
    out += [f"### {a.title}", ""]
out += [
    "| Terrain | STYLE hex | lit px | lit mean | lit within 12 of hex | shadow px | shadow mean (blend vs hex / vs lit mean) | target 35% / 40% / 45% | shadow px within 12 of a 35-45% blend | worst shadow px error | sample (x,y): measured vs nearest target (blend) |",
    "|---|---|---|---|---|---|---|---|---|---|---|",
]
for r in rows:
    smp = "<br>".join(f"{xy}: {hexs(m)} vs {hexs(t)} ({b * 100:.1f}%)" for xy, m, t, b in r["samples"]) or "-"
    out.append(f"| {r['name']} | #{r['hex']:06X} | {r['ln']} | {hexs(r['lmean'])} | {pc(r['lfrac'])} | {r['n']} | "
               f"{hexs(r['mean'])} ({r['t'] * 100:.0f}% / {pc0(r['t_own'])}) | {hexs(r['t35'])} / {hexs(r['t40'])} / {hexs(r['t45'])} | {pc(r['frac'])} | "
               f"{'-' if r['worst'] is None else int(r['worst'])} | {smp} |")
out += ["", f"Penumbra (cyan, excluded) px: {int(penumbra.sum())}. Verdict: {'PASS' if ok_all else 'FAIL'}", ""]
txt = "\n".join(out)
print(txt)
if a.out:
    with open(a.out, "w") as f:
        f.write(txt)
raise SystemExit(0 if ok_all else 1)
