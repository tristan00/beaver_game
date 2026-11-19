import math, os, re, sys
from PIL import Image, ImageDraw, ImageFont

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
SS = 2

def load_palette():
    src = open(os.path.join(ROOT, "crates/game/src/palette.rs"), encoding="utf-8").read()
    return {n: "#%06X" % int(h, 16) for n, h in re.findall(r"pub const (\w+): Rgb = rgb\(0x([0-9A-Fa-f]{6})\)", src)}

def load_floats():
    src = open(os.path.join(ROOT, "crates/game/src/palette.rs"), encoding="utf-8").read()
    return {n: float(v) for n, v in re.findall(r"pub const (\w+): f32 = ([0-9.]+);", src)}

P = load_palette()
F = load_floats()

def mix_hex(a, b, t):
    ca, cb = c(a), c(b)
    return tuple(round(ca[i] + (cb[i] - ca[i]) * t) for i in range(3))

def water_band(depth):

    if depth == 0:
        return c("WATER_CLEAR"), round(F["WATER_TINT_SHALLOW"] * 255)
    if depth == 1:
        return c("WATER_MID_GLOW"), round(F["WATER_TINT_MID"] * 255)
    return mix_hex("WATER_CLEAR", "WATER_DEEP", F["WATER_DEEP_NUDGE"]), round(F["WATER_TINT_DEEP"] * 255)

def c(name, alpha=None):
    h = P[name].lstrip("#")
    rgb = tuple(int(h[i:i + 2], 16) for i in (0, 2, 4))
    return rgb + ((alpha,) if alpha is not None else ())

def font(size):
    for f in ("arial.ttf", "DejaVuSans.ttf"):
        try:
            return ImageFont.truetype(f, size * SS)
        except OSError:
            pass
    return ImageFont.load_default()

class Sheet:
    def __init__(self, title, rule, w=1200, h=760):
        self.w, self.h = w, h
        self.img = Image.new("RGBA", (w * SS, h * SS), c("MEADOW_LIGHT") + (255,))
        self.img.paste(Image.new("RGBA", self.img.size, (252, 242, 222, 255)))
        self.d = ImageDraw.Draw(self.img, "RGBA")
        self.text(24, 18, title, 28, "INK")
        self.text(24, 54, rule, 15, "BARK")

    def text(self, x, y, s, size=14, col="INK"):
        self.d.text((x * SS, y * SS), s, font=font(size), fill=c(col))

    def _blend(self, draw_fn, col, a):

        fill = c(col, a) if isinstance(col, str) else (tuple(col[:3]) + (a,) if len(col) == 3 else tuple(col))
        if len(fill) == 4 and fill[3] < 255:
            layer = Image.new("RGBA", self.img.size, (0, 0, 0, 0))
            draw_fn(ImageDraw.Draw(layer), fill)
            self.img.alpha_composite(layer)
        else:
            draw_fn(self.d, fill)

    def ell(self, cx, cy, rx, ry, col, a=255):
        box = [(cx - rx) * SS, (cy - ry) * SS, (cx + rx) * SS, (cy + ry) * SS]
        self._blend(lambda d, f: d.ellipse(box, fill=f), col, a)

    def rr(self, x0, y0, x1, y1, r, col, a=255):
        box = [x0 * SS, y0 * SS, x1 * SS, y1 * SS]
        self._blend(lambda d, f: d.rounded_rectangle(box, r * SS, fill=f), col, a)

    def swatches(self, y, names, x0=24, w=96):
        x = x0
        for n in names:
            self.rr(x, y, x + w - 8, y + 54, 14, n)
            self.text(x + 2, y + 58, n.replace("_", " ").title(), 11)
            self.text(x + 2, y + 73, P[n], 11, "BARK")
            x += w

    def panel(self, x0, y0, x1, y1, bg="MEADOW", label=None):
        self.rr(x0, y0, x1, y1, 22, bg)
        if label:
            self.text(x0 + 12, y1 - 24, label, 13, "INK")

    def shadow(self, cx, cy, rx, ry):
        self.ell(cx, cy, rx, ry, c("PLUM", round(F["SHADOW_MIX"] * 255)))

    def save(self, path):
        self.img.resize((self.w, self.h), Image.LANCZOS).convert("RGB").save(path)

def tree(s, x, y, r=34, base="MEADOW_DEEP", hi="LEAF_HIGHLIGHT", trunk=True):
    s.shadow(x + r * 0.35, y + 2, r * 0.9, r * 0.3)
    if trunk:
        s.rr(x - r * .13, y - r * .5, x + r * .13, y + 2, 4, "BARK")
    for dx, dy, k in ((-.5, -.9, .62), (.5, -.9, .62), (0, -1.5, .66), (0, -1.05, .72)):
        s.ell(x + dx * r, y + dy * r, r * k, r * k, base)
    s.ell(x - r * .22, y - r * 1.55, r * .28, r * .22, hi)
    s.ell(x + r * .4, y - r * 1.0, r * .2, r * .16, hi)

def stump(s, x, y, r=14):
    s.shadow(x + r * .4, y + 3, r * 1.1, r * .35)
    s.rr(x - r, y - r * .9, x + r, y + 2, r * .5, "BARK")
    s.ell(x, y - r * .9, r, r * .45, "CUT_WOOD")
    s.ell(x, y - r * .9, r * .55, r * .25, "STICK_LIGHT", 160)

def rock(s, x, y, r=22, top="STONE", side="STONE_SHADE"):
    s.shadow(x + r * .3, y + 3, r * 1.1, r * .3)
    s.ell(x, y - r * .35, r, r * .72, side)
    s.ell(x - r * .1, y - r * .55, r * .82, r * .5, top)

def tuft(s, x, y, col="MEADOW_DEEP"):
    for dx in (-5, 0, 5):
        s.d.line([(x + dx) * SS, y * SS, (x + dx * 1.5) * SS, (y - 10) * SS], fill=c(col), width=3 * SS)

def flower(s, x, y, col="FLOWER_WHITE"):
    for a in range(5):
        s.ell(x + 4 * math.cos(a * 1.257), y + 4 * math.sin(a * 1.257), 2.6, 2.6, col)
    s.ell(x, y, 2.2, 2.2, "FLOWER_YELLOW")

def water(s, cx, cy, rx, ry, bed="SAND", bands=3, foam=True, bed_detail=True):

    s.ell(cx, cy, rx, ry, bed)
    if bed_detail:
        s.ell(cx - rx * .25, cy + ry * .15, rx * .3, ry * .28, "MUD")
        s.ell(cx - rx * .35, cy + ry * .25, rx * .1, ry * .1, "STONE")
        s.ell(cx + rx * .3, cy - ry * .2, rx * .18, ry * .16, "LILY")
        rock(s, cx + rx * .05, cy + ry * .35, max(4, rx * .05))

    for d in range(bands):
        k = 1 - d * .3
        col, a = water_band(d)
        layer = Image.new("RGBA", s.img.size, (0, 0, 0, 0))
        mask = Image.new("L", s.img.size, 0)
        ImageDraw.Draw(mask).ellipse([(cx - rx * k) * SS, (cy - ry * k) * SS, (cx + rx * k) * SS, (cy + ry * k) * SS], fill=255)
        if d == 0:
            base = s.img.copy()
            s._bed = base
        bed_img = s._bed
        tinted = Image.alpha_composite(bed_img, Image.new("RGBA", s.img.size, col + (a,)))
        s.img.paste(tinted, (0, 0), mask)
    s.d = ImageDraw.Draw(s.img, "RGBA")
    if foam:
        s.d.ellipse([(cx - rx) * SS, (cy - ry) * SS, (cx + rx) * SS, (cy + ry) * SS], outline=c("FOAM"), width=5 * SS)

def lodge(s, x, y, stage, scale=1.0):

    k = scale
    s.shadow(x + 20 * k, y + 4, 80 * k, 16 * k)
    rx, ry = (34, 20) if stage == 1 else (52, 36) if stage == 2 else (74, 52)
    s.ell(x, y - ry * k * .35, rx * k, ry * k, "DAM_MUD")
    s.ell(x - rx * k * .1, y - ry * k * .55, rx * k * .88, ry * k * .8, "STICK")
    for i in range(7 + stage * 5):
        a = i * 2.4
        px, py = x + math.cos(a) * rx * k * .7 * ((i * 37) % 10) / 10, y - ry * k * .5 - abs(math.sin(a)) * ry * k * .8 * ((i * 53) % 10) / 10
        s.d.line([(px - 9 * k) * SS, (py + 3 * k) * SS, (px + 9 * k) * SS, (py - 3 * k) * SS], fill=c("STICK_LIGHT"), width=int(3 * k * SS))
    if stage >= 2:
        s.ell(x + rx * k * .1, y - 4 * k, 11 * k, 9 * k, "INK", 200)
    if stage >= 3:
        s.ell(x - rx * k * .55, y - ry * k * 1.0, 6 * k, 4 * k, "INK", 120)
        for i in range(4):
            s.rr(x + rx * k * .7 + i * 7 * k - 6, y - 3 * k, x + rx * k * .7 + i * 7 * k + 8, y + 5 * k, 3, "FUR_DARK")

def dam(s, x, y, stage):
    n = (2, 4, 7)[stage - 1]
    for i in range(n):
        yy = y - (stage - 1) * 6 - i % 2 * 5
        s.rr(x - 70 + i * (140 / n), yy - 8, x - 70 + i * (140 / n) + 34, yy + 6, 7, "STICK" if i % 2 else "STICK_LIGHT")
    s.ell(x, y + 6, 74, 8, "DAM_MUD", 220)

def beaver(s, x, y, pose, sc=1.0):
    k = sc
    if pose != "swim":
        s.shadow(x + 6 * k, y + 14 * k, 30 * k, 8 * k)

    by = y + 8 * k if pose == "swim" else y
    under = s.img.copy() if pose == "swim" else None
    tail_dx = -30 * k if pose != "sleep" else -24 * k
    s.ell(x + tail_dx, by + 4 * k, 17 * k, 7 * k, "FUR_DARK")
    bw, bh = (24, 18) if pose != "sleep" else (30, 14)
    if pose == "gnaw":
        bh = 16
    s.ell(x, by - 6 * k, bw * k, bh * k, "FUR")
    if pose == "swim":

        wl = by - 12 * k

        mask = Image.new("L", s.img.size, 0)
        ImageDraw.Draw(mask).rectangle([(x - 60 * k) * SS, wl * SS, (x + 40 * k) * SS, (by + 30 * k) * SS], fill=255)
        s.img.paste(Image.blend(s.img, under, 0.7), (0, 0), mask)
        s.d = ImageDraw.Draw(s.img, "RGBA")
        for dx, w in ((-34, 30), (24, 26), (-6, 44)):
            cx = x + dx * k
            s.d.arc([(cx - w / 2 * k) * SS, (wl - 4 * k) * SS, (cx + w / 2 * k) * SS, (wl + 6 * k) * SS], 10, 170, fill=c("FOAM"), width=max(2, int(1.2 * k)) * SS)
    else:
        s.ell(x + 2 * k, by - 2 * k, bw * .55 * k, bh * .55 * k, "FUR_LIGHT")
    hx, hy = x + (14 * k if pose != "sleep" else 22 * k), by - (24 * k if pose in ("stand", "carry", "swim") else 14 * k)
    if pose == "sleep":
        hy = by - 6 * k
    s.ell(hx, hy, 15 * k, 13 * k, "FUR")
    s.ell(hx + 3 * k, hy + 4 * k, 8 * k, 6 * k, "FUR_LIGHT")
    s.ell(hx - 9 * k, hy - 11 * k, 4 * k, 4 * k, "FUR_DARK")
    if pose == "sleep":
        s.d.arc([(hx + 2 * k) * SS, (hy - 4 * k) * SS, (hx + 10 * k) * SS, (hy + 2 * k) * SS], 0, 180, fill=c("INK"), width=2 * SS)
    else:
        s.ell(hx + 6 * k, hy - 3 * k, 2.2 * k, 2.2 * k, "INK")
    s.ell(hx + 13 * k, hy + 2 * k, 3 * k, 2.4 * k, "INK")
    s.rr(hx + 8 * k, hy + 5 * k, hx + 12 * k, hy + 11 * k, 2, "CUT_WOOD")
    if pose == "carry":
        s.rr(x - 20 * k, by - 40 * k, x + 34 * k, by - 28 * k, 6, "STICK")
        s.ell(x + 34 * k, by - 34 * k, 6 * k, 6 * k, "CUT_WOOD")
    if pose == "gnaw":
        s.rr(hx + 8 * k, hy + 2 * k, hx + 44 * k, hy + 14 * k, 6, "BARK")
        s.ell(hx + 44 * k, hy + 8 * k, 5 * k, 6 * k, "CUT_WOOD")
    if pose == "build":
        s.rr(hx + 10 * k, hy + 6 * k, hx + 38 * k, hy + 14 * k, 4, "STICK_LIGHT")
        s.ell(x + 20 * k, by + 8 * k, 7 * k, 5 * k, "DAM_MUD")

def sec_trees(o):
    s = Sheet("1 Trees", "Clusters of 3 to 9 trees with clearings between them, stumps where beavers have felled. Never a grid, never a lone row.")
    s.swatches(90, ["MEADOW_DEEP", "LEAF_HIGHLIGHT", "MEADOW", "BARK", "CUT_WOOD", "PLUM", "STICK"])
    s.panel(24, 210, 760, 730, "MEADOW", "Cluster, clearing, felled edge")
    rng = [(120, 360, 40), (190, 330, 34), (240, 400, 38), (160, 450, 32), (300, 340, 30), (110, 520, 36), (230, 520, 40), (330, 480, 28)]
    for x, y, r in sorted(rng, key=lambda t: t[1]):
        tree(s, x, y, r)
    s.ell(450, 560, 90, 55, "MEADOW_LIGHT", 160)
    s.text(396, 548, "clearing", 13, "INK")
    for x, y, r in ((560, 470, 15), (610, 520, 13), (520, 590, 17)):
        stump(s, x, y, r)
    tree(s, 650, 400, 36); tree(s, 700, 440, 28); tree(s, 610, 360, 30)
    for x, y in ((500, 420), (430, 640), (560, 660)):
        tuft(s, x, y)
    s.panel(790, 210, 1176, 730, "MEADOW_LIGHT", "Silhouette types")
    tree(s, 880, 400, 46); s.text(850, 420, "round", 12)
    tree(s, 1010, 400, 36, base="MEADOW", hi="MEADOW_LIGHT"); s.text(985, 420, "young", 12)
    s.ell(1110, 330, 15, 40, "MEADOW_DEEP"); s.ell(1110, 300, 12, 30, "LEAF_HIGHLIGHT"); s.rr(1106, 360, 1114, 405, 3, "BARK"); s.text(1088, 420, "poplar", 12)
    stump(s, 880, 560, 22); s.text(850, 580, "stump", 12)
    s.rr(960, 550, 1060, 570, 9, "BARK"); s.ell(1060, 560, 6, 10, "CUT_WOOD"); s.text(985, 580, "felled log", 12)
    s.save(o("trees"))

def sec_ground(o):
    s = Sheet("2 Ground", "Natural ground: meadow, straw, mud banks and sand shore carry the map. Rock only where terrain calls: ridges and cliffs, never as filler.")
    s.swatches(90, ["MEADOW_LIGHT", "MEADOW", "MEADOW_DEEP", "STRAW", "MUD", "SAND", "STONE", "STONE_SHADE"], w=96)
    s.panel(24, 210, 760, 730, "MEADOW")
    for (x, y, rx, ry, col) in ((200, 330, 150, 70, "MEADOW_LIGHT"), (520, 300, 120, 60, "MEADOW_LIGHT"), (350, 520, 170, 80, "MEADOW_DEEP"), (620, 560, 90, 55, "STRAW")):
        s.ell(x, y, rx, ry, col, 150)
    s.ell(560, 420, 110, 36, "MUD", 220); s.text(520, 412, "mud bank", 12)
    for x, y in ((120, 280), (300, 380), (430, 300), (640, 640), (250, 620), (700, 380), (160, 450)):
        tuft(s, x, y)
    for i, (x, y, col) in enumerate(((220, 360, "FLOWER_WHITE"), (480, 470, "FLOWER_PINK"), (400, 600, "FLOWER_YELLOW"), (580, 260, "FLOWER_PINK"), (120, 600, "FLOWER_WHITE"))):
        flower(s, x, y, col)
    rock(s, 690, 300, 14); s.text(665, 312, "pebble", 12)
    s.text(40, 700, "Open meadow, about 1 detail per 40x40 px", 13)
    s.panel(790, 210, 1176, 730, "MEADOW_LIGHT", "Rock: only at a ridge or cliff")
    for (x, y, r) in ((880, 520, 40), (960, 540, 52), (1060, 520, 36), (1010, 470, 30), (920, 470, 28)):
        rock(s, x, y, r)
    s.ell(980, 600, 140, 30, "MEADOW_DEEP", 200)
    s.text(800, 330, "Rock is bevelled and rounded,", 13); s.text(800, 350, "lilac in shade, never grey-blue.", 13)
    s.text(800, 380, "No rock inside open meadow.", 13)
    s.save(o("ground"))

def sec_water(o):
    s = Sheet("3 Water", "Nearly clear: the bed shows through a thin tint, 40 to 60% at most. Foam edge, soft ripples, a few lilies and reeds. No opaque blue slab.")
    s.swatches(90, ["WATER_CLEAR", "WATER_MID_GLOW", "WATER_DEEP", "FOAM", "LILY", "REED", "SAND", "MUD"], w=96)
    s.panel(24, 210, 760, 730, "MEADOW")
    s.ell(390, 470, 300, 190, "MUD")
    water(s, 390, 470, 280, 172)
    for (x, y, w) in ((300, 410, 60), (470, 520, 70), (420, 440, 40)):
        s.d.arc([(x - w / 2) * SS, (y - 5) * SS, (x + w / 2) * SS, (y + 7) * SS], 190, 350, fill=c("FOAM"), width=2 * SS)
    for x, y in ((250, 480), (520, 470), (440, 380)):
        s.ell(x, y, 18, 10, "LILY"); s.ell(x + 3, y - 2, 6, 3, "FLOWER_PINK")
    for x in (120, 140, 660, 680):
        s.d.line([x * SS, 560 * SS, (x + 6) * SS, 500 * SS], fill=c("REED"), width=3 * SS)
    s.text(40, 700, "Bed, stones and weed read through the water, even at the centre", 13)
    s.panel(790, 210, 1176, 730, "MEADOW_LIGHT", "Depth reads by tint, not by blue")
    for i, nm in enumerate(("shallow", "mid", "deep")):
        y = 250 + i * 110
        col, a = water_band(i)
        s.rr(810, y, 1156, y + 90, 22, "SAND")
        s.rr(980, y, 1156, y + 90, 22, "MUD")
        for k, bx in enumerate(range(840, 1150, 70)):
            rock(s, bx, y + 60, 14 if k % 2 else 10)
        s.rr(810, y, 1156, y + 90, 22, col, a)
        s.d.line([815 * SS, (y + 4) * SS, 1150 * SS, (y + 4) * SS], fill=c("FOAM", 200), width=3 * SS)
        tint = {0: "Water clear", 1: "Water mid glow", 2: "Water clear + 25% Water deep"}[i]
        s.text(824, y + 10, f"{nm}: {tint} at {round(a / 2.55)}%", 13)
    s.save(o("water"))

def sec_structures(o):
    s = Sheet("4 Structures", "Stick-and-mud, built by paws. They start as a small heap and grow in three stages. Stages 1 to 3 add sticks, mud and detail, never new materials.")
    s.swatches(90, ["STICK", "STICK_LIGHT", "DAM_MUD", "BARK", "CUT_WOOD", "FUR_DARK", "PLUM"])
    for i in range(3):
        x0 = 24 + i * 392
        s.panel(x0, 210, x0 + 376, 470, "MEADOW", f"Lodge stage {i + 1}")
        lodge(s, x0 + 188, 400, i + 1, 1.0)
    for i in range(3):
        x0 = 24 + i * 392
        s.panel(x0, 490, x0 + 376, 730, "MEADOW", f"Dam stage {i + 1}")
        water(s, x0 + 188, 640, 170, 40, bands=2, bed_detail=False)
        dam(s, x0 + 188, 610, i + 1)
    s.save(o("structures"))

def sec_poses(o):
    s = Sheet("5 Beaver poses", "Chibi: head about 60% of body width, flat oval tail, dot eyes, two teeth. Plush, never sharp. One silhouette per job.")
    s.swatches(90, ["FUR", "FUR_LIGHT", "FUR_DARK", "BARK", "CUT_WOOD", "INK", "FOAM"])
    names = ["stand", "carry", "gnaw", "build", "swim", "sleep"]
    for i, n in enumerate(names):
        x0 = 24 + (i % 3) * 392
        y0 = 210 + (i // 3) * 265
        s.panel(x0, y0, x0 + 376, y0 + 250, "MEADOW", n)
        if n == "swim":
            water(s, x0 + 188, y0 + 120, 170, 100, bands=2)
        beaver(s, x0 + 170, y0 + 165, n, 2.2)
    s.save(o("beaver_poses"))

def sec_seasons(o):
    s = Sheet("6 Seasons", "Hue shifts, brightness stays high. Same shapes in every season; only colour and a few season props change.")
    seasons = [
        ("Spring", "MEADOW", "MEADOW_LIGHT", "LEAF_HIGHLIGHT", "SKY", "BLOSSOM"),
        ("Summer", "MEADOW", "MEADOW_DEEP", "LEAF_HIGHLIGHT", "SKY", None),
        ("Autumn", "STRAW", "AUTUMN_GOLD", "AUTUMN_RUST", "SUN_AUTUMN", None),
        ("Winter", "SNOW", "SNOW_SHADE", "SNOW", "SKY_WINTER", None),
    ]
    for i, (nm, ground, canopy, hi, sky, extra) in enumerate(seasons):
        x0 = 24 + i * 292
        s.rr(x0, 90, x0 + 276, 130, 14, sky)
        s.text(x0 + 12, 98, nm, 18)
        s.panel(x0, 140, x0 + 276, 470, ground, f"{ground.title().replace('_', ' ')} ground")
        tree(s, x0 + 90, 330, 42, base=canopy, hi=hi)
        tree(s, x0 + 190, 380, 32, base=canopy, hi=hi)
        stump(s, x0 + 60, 430, 12)
        if extra:
            for dx, dy in ((70, 260), (110, 290), (160, 300), (200, 330), (80, 340)):
                s.ell(x0 + dx, dy, 4, 4, extra)
        if nm == "Autumn":
            for dx, dy in ((40, 400), (140, 420), (220, 440)):
                s.ell(x0 + dx, dy, 6, 4, "AUTUMN_RUST")
        if nm == "Winter":
            s.ell(x0 + 90, 296, 28, 10, "SNOW")
            s.ell(x0 + 190, 358, 22, 8, "SNOW")
            s.ell(x0 + 60, 424, 10, 4, "SNOW")
        s.swatches(480, list(dict.fromkeys([ground, canopy, hi])), x0=x0, w=92)
    s.rr(24, 640, 1176, 730, 22, "SKY_WINTER" if False else "SNOW")
    s.text(40, 650, "Ice: winter water gets a pale Snow rim and keeps its clear tint. Night: Plum overlay 30%, dusk not dark.", 14)
    s.text(40, 680, "Sky: spring SKY, summer SKY, autumn sun SUN_AUTUMN, winter SKY_WINTER.", 14)
    s.save(o("seasons"))

def sec_camera(o):
    s = Sheet("7 Camera at every zoom", "Same world, three readings: far = colour and clusters, mid = shapes and structures, close = detail and faces.")
    for i, nm in enumerate(["Far", "Mid", "Close"]):
        x0 = 24 + i * 392
        s.panel(x0, 100, x0 + 376, 640, "MEADOW", nm)
        if i == 0:
            for (dx, dy, r) in ((90, 200, 50), (150, 240, 40), (260, 360, 55), (300, 440, 35), (80, 470, 45)):
                s.ell(x0 + dx, 100 + dy, r, r * .8, "MEADOW_DEEP")
            water(s, x0 + 200, 100 + 140, 120, 55, bands=2, foam=False, bed_detail=False); s.ell(x0 + 190, 100 + 300, 16, 12, "STICK")
            s.text(x0 + 12, 112, "trees = soft green blobs, water = pale, lodge = one dot", 12)
        elif i == 1:
            tree(s, x0 + 90, 300, 34); tree(s, x0 + 160, 280, 28); stump(s, x0 + 120, 340, 10)
            lodge(s, x0 + 270, 450, 2, .75)
            water(s, x0 + 140, 520, 120, 40, bands=2, bed_detail=False)
            beaver(s, x0 + 200, 400, "carry", .6)
            s.text(x0 + 12, 112, "trees, structures and beaver poses read", 12)
        else:
            beaver(s, x0 + 150, 400, "gnaw", 2.2)
            stump(s, x0 + 300, 440, 22)
            for x, y, col in ((60, 500, "FLOWER_PINK"), (120, 540, "FLOWER_WHITE"), (320, 520, "FLOWER_YELLOW")):
                flower(s, x0 + x, y, col)
            tuft(s, x0 + 230, 540); tuft(s, x0 + 40, 440)
            s.text(x0 + 12, 112, "fur, teeth, rings and flowers read", 12)
    s.rr(24, 650, 1176, 735, 22, "SNOW")
    s.text(40, 658, "Scale holds: beaver 1 cell, canopy 2x a beaver, lodge 4x. Soft shadows, plum 35-45%, at every zoom.", 14)
    s.text(40, 685, "Zoom adds detail, never new colours. Far hides tufts, flowers, ripples; close shows all of them.", 14)
    s.save(o("camera"))

def check_doc():
    doc = open(os.path.join(ROOT, "docs/STYLE.md"), encoding="utf-8").read()
    have = {v.upper() for v in P.values()}
    missing = sorted({h.upper() for h in re.findall(r"`(#[0-9A-Fa-f]{6})`", doc)} - have)
    print("STYLE.md hexes not in palette.rs:", missing or "none")
    return not missing

def main():
    args = [a for a in sys.argv[1:] if not a.startswith("--")]
    out = args[0] if args else os.path.join(ROOT, "docs", "style")
    os.makedirs(out, exist_ok=True)
    o = lambda n: os.path.join(out, n + ".png")
    for f in (sec_trees, sec_ground, sec_water, sec_structures, sec_poses, sec_seasons, sec_camera):
        f(o)
    print("wrote", out)
    if "--check" in sys.argv:
        sys.exit(0 if check_doc() else 1)

if __name__ == "__main__":
    main()
