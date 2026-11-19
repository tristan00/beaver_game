import sys
import numpy as np
from PIL import Image

PLUM = np.array([0x6B, 0x5A, 0x8A], float)

def rgb(h):
    return np.array([(h >> 16) & 255, (h >> 8) & 255, h & 255], float)

def shadow(h, t=0.40):
    return rgb(h) * (1 - t) + PLUM * t

S = [
    ("2 Ground", "lodge_seed1_noglow", 800, 250, "Stone, sunlit rock", rgb(0xB9AFA8)),
    ("2 Ground", "lodge_seed2_noglow", 1270, 125, "Stone, sunlit rock", rgb(0xB9AFA8)),
    ("2 Ground", "lodge_seed2_noglow", 1000, 250, "Stone shade, shadowed rock", rgb(0x8C8290)),
    ("2 Ground", "pond_seed1_noglow", 158, 159, "Stone shade, shadowed rock", rgb(0x8C8290)),
    ("2 Ground", "pond_seed1_noglow", 1000, 300, "Meadow deep, valley grass", rgb(0x5FA85A)),
    ("2 Ground", "lodge_seed2_noglow", 451, 710, "Meadow deep, valley floor", rgb(0x5FA85A)),
    ("2 Ground", "lodge_seed2_noglow", 400, 330, "Meadow in sun shadow (floor of a valley)", shadow(0x85C468)),
    ("2 Ground", "lodge_seed2_noglow", 200, 400, "Meadow, base grass", rgb(0x85C468)),
    ("2 Ground", "pond_seed1_noglow", 1100, 600, "Meadow, base grass", rgb(0x85C468)),
    ("2 Ground", "lodge_seed1_noglow", 620, 425, "Meadow light, sun-facing grass", rgb(0xA8D878)),
    ("2 Ground", "lodge_seed1_noglow", 505, 544, "Meadow light, sun-facing grass", rgb(0xA8D878)),
    ("1 Trees", "lodge_seed2_noglow", 480, 410, "pine canopy, Leaf highlight", rgb(0x8CD070)),
    ("1 Trees", "lodge_seed2_noglow", 490, 450, "pine canopy, Leaf highlight", rgb(0x8CD070)),
    ("5 Poses", "fur_poses_1_noglow", 560, 305, "swimmer's ripple arc, Foam", rgb(0xF4FBF6)),
    ("Shared light", "lodge_seed2_noglow", 1100, 40, "sky", rgb(0x9ED1F5)),
    ("Shared light", "pond_seed2_noglow", 1200, 60, "sky behind the pond view's UI strip is covered; see iso", rgb(0x9ED1F5)),
    ("Shared light", "lodge_seed2_noglow", 400, 330, "grass in sun shadow, plum 40%", shadow(0x85C468)),
    ("Shared light", "lodge_seed2_noglow", 1000, 250, "rock in sun shadow, plum 40%", shadow(0xB9AFA8)),
    ("5 Poses", "fur_poses_1_noglow", 570, 230, "beaver back, Fur", rgb(0xB9783F)),
    ("5 Poses", "fur_poses_0_noglow", 560, 330, "beaver back, Fur", rgb(0xB9783F)),
    ("5 Poses", "fur_poses_0_noglow", 470, 352, "beaver tail, Fur dark", rgb(0x8F5A2E)),
]
S = [s for s in S if "see iso" not in s[4]]

def hexs(v):
    return "#%02X%02X%02X" % tuple(int(round(x)) for x in v)

def main(d):
    cache = {}
    print("| Section | Shot | Point (x,y) | What | Measured | Target | Max channel diff | Result |")
    print("|---|---|---|---|---|---|---|---|")
    bad = 0
    for sec, shot, x, y, name, tgt in S:
        if shot not in cache:
            cache[shot] = np.asarray(Image.open(f"{d}/{shot}.png").convert("RGB")).astype(float)
        m = cache[shot][y - 1:y + 2, x - 1:x + 2].reshape(-1, 3).mean(0)
        diff = np.abs(m - tgt).max()
        ok = diff <= 12
        bad += not ok
        print(f"| {sec} | {shot}.png | {x},{y} | {name} | {hexs(m)} | {hexs(tgt)} | {diff:.0f} | {'PASS' if ok else 'FAIL'} |")

    from scipy import ndimage as ndi
    print()
    print("| Terrain floor shot | Darkest 5x5 patch of sun-lit terrain (x,y) | Colour | Floor (lower of Meadow deep / Stone shade, -12/channel) | Result |")
    print("|---|---|---|---|---|")

    floor = np.minimum(rgb(0x5FA85A), rgb(0x8C8290)) - 12
    for shot in ["iso_seed1", "iso_seed2", "lodge_seed1", "lodge_seed2", "pond_seed1", "pond_seed2"]:
        a = np.asarray(Image.open(f"{d}/{shot}_noglow.png").convert("RGB")).astype(float)
        mp = f"{d}/../masks/{shot}_mask.png"
        m = np.asarray(Image.open(mp).convert("RGB")).astype(int)
        lit = np.all(np.abs(m - np.array([255, 255, 0])) < 6, axis=2)
        lit = ndi.binary_erosion(lit, iterations=3)
        patch = np.stack([ndi.uniform_filter(a[..., k], size=5) for k in range(3)], -1)
        lum = patch @ np.array([0.2126, 0.7152, 0.0722])

        l1 = a @ np.array([0.2126, 0.7152, 0.0722])
        spread = ndi.maximum_filter(l1, size=7) - ndi.minimum_filter(l1, size=7)
        lum[~lit | (spread >= 4)] = 1e9
        y, x = np.unravel_index(lum.argmin(), lum.shape)
        c = patch[y, x]
        ok = bool((c >= floor).all())
        bad += not ok
        print(f"| {shot}_noglow.png | {x},{y} | {hexs(c)} | {hexs(floor)} | {'PASS' if ok else 'FAIL'} |")

    print()
    print("| Brightness shot | Mean luminance (0-255) | Result |")
    print("|---|---|---|")
    for shot in ["iso_seed1", "iso_seed2", "lodge_seed1", "lodge_seed2", "pond_seed1", "pond_seed2"]:
        a = np.asarray(Image.open(f"{d}/{shot}_noglow.png").convert("RGB")).astype(float)
        lum = (a @ np.array([0.2126, 0.7152, 0.0722])).mean()
        ok = lum > 128
        bad += not ok
        print(f"| {shot}_noglow.png | {lum:.1f} | {'PASS (> 128)' if ok else 'FAIL'} |")
    return 1 if bad else 0

if __name__ == "__main__":
    sys.exit(main(sys.argv[1]))
