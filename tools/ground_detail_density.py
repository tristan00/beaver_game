import sys, numpy as np
from PIL import Image, ImageDraw
from scipy import ndimage as ndi

def count(path, box, out):
    im = Image.open(path).convert("RGB"); a = np.asarray(im).astype(float)
    x0, y0, x1, y1 = box
    c = a[y0:y1, x0:x1]
    bg = np.stack([ndi.median_filter(c[..., k], size=21) for k in range(3)], -1)
    dev = np.abs(c - bg).max(2) > 12
    dev = ndi.binary_dilation(dev, iterations=2)
    lab, n = ndi.label(dev)
    sizes = ndi.sum(dev, lab, range(1, n + 1))
    keep = [i + 1 for i, s in enumerate(sizes) if s >= 20]
    area = (x1 - x0) * (y1 - y0)
    cells = area / 1600
    side = (area / max(1, len(keep))) ** .5
    kinds = {"tuft": 0, "flower": 0, "pebble": 0}
    for i in keep:
        px = c[lab == i]
        r, g, b = px[:, 0], px[:, 1], px[:, 2]
        flower = ((r > g + 20) | ((r > 215) & (g > 200) & (b > 190)) | ((r > 200) & (g > 150) & (b < 120))).sum() >= 4
        pebble = ((np.abs(r - g) < 22) & (np.abs(g - b) < 22) & (r > 130)).sum() >= 4
        kinds["flower" if flower else "pebble" if pebble else "tuft"] += 1
    print(kinds)
    print(path.split('/')[-1], box, "items", len(keep), "per 40x40: %.2f" % (len(keep) / cells), "=> one per %.0f x %.0f px" % (side, side))
    d = ImageDraw.Draw(im)
    for i in keep:
        ys, xs = np.nonzero(lab == i)
        d.rectangle([x0 + xs.min() - 2, y0 + ys.min() - 2, x0 + xs.max() + 2, y0 + ys.max() + 2], outline=(255, 0, 255))
    d.rectangle(box, outline=(255, 255, 0))
    im.crop((x0 - 10, y0 - 10, x1 + 10, y1 + 10)).resize(((x1 - x0 + 20) * 3, (y1 - y0 + 20) * 3), Image.NEAREST).save(out)

A = "artifacts/R19-29/after/"
count(A + "pond_seed1_noglow.png", (900, 330, 1040, 520), "tmp/d1.png")
count(A + "pond_seed2_noglow.png", (1000, 330, 1160, 470), "tmp/d2.png")

