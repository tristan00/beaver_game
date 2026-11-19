import sys
import numpy as np
sys.path.insert(0, __file__.rsplit("/", 1)[0].rsplit("\\", 1)[0])
from sandbox_overview import load

n, cell, L = load(sys.argv[1])
x0, y0, x1, y1 = (int(v) for v in sys.argv[2].split(","))
q, d, w, g, dam, ch, speed = L["discharge"], L["depth"], L["width"], L["ground"], L["dam"], L["channel"], L["speed"]
for y in range(y0, y1 + 1):
    for x in range(x0, x1 + 1):
        if q[y, x] > 1e-5 or d[y, x] > 0.005:
            print(f"{x},{y} ground {g[y,x]:.2f} dam {dam[y,x]:.2f} depth {d[y,x]:.3f} surface {g[y,x]+dam[y,x]+d[y,x]:.2f} width {w[y,x]:.2f} q {q[y,x]:.4f} speed {speed[y,x]:.2f} m/s{' channel' if ch[y,x] else ''}{' river' if L['river'][y,x] else ''}")
