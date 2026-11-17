"""Sample sun-shadow pixels per terrain colour. usage: shadow_check.py lit.png shadowed.png [out.md]
lit.png is rendered with BC_NO_SUN_SHADOWS=1, shadowed.png without; same seed and camera.
Shadowed pixel = differs from lit render; terrain = nearest STYLE hex of the lit pixel (within 12)."""
import sys, numpy as np
from PIL import Image
T = {"Meadow light": 0xA8D878, "Meadow": 0x85C468, "Meadow deep": 0x5FA85A, "Straw": 0xD9C27A, "Stone": 0xB9AFA8}
PLUM = np.array([0x6B, 0x5A, 0x8A], float)
tol = 12
def rgb(h): return np.array([(h >> 16) & 255, (h >> 8) & 255, h & 255], float)
lit = np.asarray(Image.open(sys.argv[1]).convert("RGB")).astype(float)
sh = np.asarray(Image.open(sys.argv[2]).convert("RGB")).astype(float)
H, W, _ = lit.shape
ui = np.zeros((H, W), bool); ui[:60, :] = True; ui[550:, :400] = True  # HUD
yy, xx = np.mgrid[0:H, 0:W]; ui |= np.hypot(xx, yy) < 1000  # warm upper-left glow overlay tints these pixels
changed = (np.abs(lit - sh).max(axis=2) > 8) & ~ui
rows = []; ok_all = True
for name, h in T.items():
    c = rgb(h)
    islit = (np.abs(lit - c).max(axis=2) <= tol) & ~ui & ~changed
    inshade = changed & (np.abs(lit - c).max(axis=2) <= tol)
    # lit check
    nl = islit.sum(); mean_l = lit[islit].mean(0) if nl else None
    # shadow check: exists t in [.35,.45] with all channels within tol
    ts = np.linspace(.35, .45, 11)
    tg = np.stack([c + t * (PLUM - c) for t in ts])            # (11,3)
    px = sh[inshade]
    n = len(px)
    if n:
        d = np.abs(px[:, None, :] - tg[None]).max(axis=2).min(axis=1)
        frac = (d <= tol).mean(); mean_s = px.mean(0)
        t_est = np.clip(((mean_s - c) / (PLUM - c)).mean(), 0, 1)
        ys, xs = np.nonzero(inshade); i = np.argsort(np.abs(px - px.mean(0)).sum(1))[0]
        coord = (int(xs[i]), int(ys[i])); meas = px[i]
    else:
        frac = 0; mean_s = None; t_est = 0; coord = None; meas = None
    rows.append((name, h, nl, mean_l, n, mean_s, frac, coord, meas, tg[0], tg[-1], t_est))
fmt = lambda a: "-" if a is None else "#%02X%02X%02X" % tuple(int(round(v)) for v in a)
out = ["| Terrain | STYLE hex | lit px | lit mean | shadow px | shadow mean (blend%) | target 35% / 45% | sample (x,y) | sample measured | pass frac |", "|---|---|---|---|---|---|---|---|---|---|"]
for r in rows:
    out.append(f"| {r[0]} | #{r[1]:06X} | {r[2]} | {fmt(r[3])} | {r[4]} | {fmt(r[5])} ({r[11]*100:.0f}%) | {fmt(r[9])} / {fmt(r[10])} | {r[7]} | {fmt(r[8])} | {r[6]*100:.0f}% |")
txt = "\n".join(out); print(txt)
if len(sys.argv) > 3: open(sys.argv[3], "w").write(txt + "\n")
