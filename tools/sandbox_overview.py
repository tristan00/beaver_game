import sys
import numpy as np
from PIL import Image, ImageDraw

def load(path):
    raw = open(path, "rb").read()
    n = int(np.frombuffer(raw[:4], "<u4")[0])
    cell = float(np.frombuffer(raw[4:8], "<f4")[0])
    layers = np.frombuffer(raw[8:], "<f4").reshape(-1, n, n)
    names = ["ground", "dam", "depth", "width", "discharge", "river", "canal", "channel", "bank"]
    return n, cell, dict(zip(names, layers))

def render(path, out, scale=2, grid=64, box=None):
    n, cell, L = load(path)
    g = L["ground"] + L["dam"]
    gy, gx = np.gradient(g)
    shade = np.clip(0.75 + (gx - gy) * 0.9, 0.35, 1.1)
    lo, hi = np.percentile(g, 1), np.percentile(g, 99)
    t = np.clip((g - lo) / max(hi - lo, 1e-3), 0, 1)
    base = np.stack([0.55 + 0.35 * t, 0.62 + 0.25 * t, 0.45 + 0.25 * t], -1) * shade[..., None]
    d, w, q = L["depth"], L["width"], L["discharge"]
    wet = d > 0.005
    frac = np.where(w > 0, np.clip(w / cell, 0, 1), 1.0)
    a = np.clip(0.35 + np.sqrt(np.clip(d, 0, 4)) * 0.6, 0, 1) * np.where(frac < 1, 0.55 + 0.45 * frac, 1.0)
    blue = np.array([0.12, 0.35, 0.85])
    img = np.where(wet[..., None], base * (1 - a[..., None]) + blue * a[..., None], base)
    dry_flow = (~wet) & (q > 1e-5)
    img[dry_flow] = [0.95, 0.2, 0.75]
    img[L["dam"] > 0] = [0.45, 0.25, 0.08]
    img[(L["canal"] > 0) & ~wet] = [0.95, 0.55, 0.1]
    im = Image.fromarray((np.clip(img, 0, 1) * 255).astype(np.uint8))
    if box:
        x0, y0, x1, y1 = box
        im = im.crop((x0, y0, x1, y1))
        grid = max(8, (x1 - x0) // 8)
        off = (x0, y0)
    else:
        off = (0, 0)
    im = im.resize((im.width * scale, im.height * scale), Image.NEAREST)
    dr = ImageDraw.Draw(im)
    for k in range(0, n + 1, grid):
        for axis in (0, 1):
            p = (k - off[axis]) * scale
            if 0 <= p < (im.width if axis == 0 else im.height):
                if axis == 0:
                    dr.line([(p, 0), (p, im.height)], fill=(255, 255, 255), width=1)
                    dr.text((p + 2, 2), str(k), fill=(255, 255, 0))
                else:
                    dr.line([(0, p), (im.width, p)], fill=(255, 255, 255), width=1)
                    dr.text((2, p + 2), str(k), fill=(255, 255, 0))
    im.save(out)

if __name__ == "__main__":
    args = sys.argv[1:]
    box = None
    if len(args) >= 3 and args[2].count(",") == 3:
        box = tuple(int(v) for v in args[2].split(","))
    render(args[0], args[1], scale=int(args[3]) if len(args) > 3 else (2 if box is None else 6), box=box)
    print(args[1])
