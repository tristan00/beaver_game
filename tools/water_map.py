import sys

import numpy as np
from PIL import Image, ImageDraw

def panel(path, scale=4):
    b = open(path, 'rb').read()
    n = int.from_bytes(b[:4], 'little')
    h = np.frombuffer(b[4:4 + 4 * n * n], '<f4').reshape(n, n)
    d = np.frombuffer(b[4 + 4 * n * n:4 + 8 * n * n], '<f4').reshape(n, n)
    f = np.frombuffer(b[4 + 8 * n * n:], np.uint8).reshape(n, n)
    gy, gx = np.gradient(h)
    shade = np.clip(0.62 + (gx - gy) * 0.3, 0.2, 1.0)
    land = np.array([196, 186, 150], float)
    rgb = shade[..., None] * land
    tree = (f & 4) > 0
    rgb[tree] = shade[tree][..., None] * np.array([90, 130, 80])
    wet = d > 0.01
    t = np.clip(d / 1.2, 0, 1)[..., None]
    water = (1 - t) * np.array([150, 200, 235]) + t * np.array([20, 60, 150])
    rgb[wet] = water[wet]
    dry_river = ((f & 1) > 0) & ~wet
    rgb[dry_river] = [230, 120, 40]
    rgb[(f & 8) > 0] = [210, 30, 30]
    rgb[(f & 16) > 0] = [250, 220, 0]
    rgb[(f & 32) > 0] = [220, 0, 220]
    im = Image.fromarray(rgb.clip(0, 255).astype(np.uint8)).resize((n * scale, n * scale), Image.NEAREST)
    return im

def main(out, dumps):
    ims = [panel(p) for p in dumps]
    w = sum(i.width for i in ims) + 6 * (len(ims) - 1)
    sheet = Image.new('RGB', (w, ims[0].height + 16), 'white')
    draw = ImageDraw.Draw(sheet)
    x = 0
    for p, im in zip(dumps, ims):
        sheet.paste(im, (x, 16))
        draw.text((x + 2, 2), p.replace('\\', '/').split('/')[-1], fill='black')
        x += im.width + 6
    sheet.save(out)

if __name__ == '__main__':
    main(sys.argv[1], sys.argv[2:])
