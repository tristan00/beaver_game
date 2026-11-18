"""Draws the UI wireframes (1920x1080) described in docs/UI_SPEC.md.
Usage: python tools/make_ui_wireframe.py [outdir]     (default docs/)   Needs Pillow; run from the repo root.
Writes ui_wireframe.png (full HUD), ui_wireframe_dam.png (dam preview), ui_wireframe_settle.png
(settle choice), ui_wireframe_jobs.png (drag to gather + job panel). Icons are the game's own
assets/icons/*.png; fonts are assets/fonts. Nothing from any other game."""
import os, sys
from PIL import Image, ImageDraw, ImageFont

W, H = 1920, 1080
BARK, RAISED, RAIL, LINE = "#3F2E24", "#4D3A2D", "#2A1F19", "#5B4636"
PITH, DIM, POND, MOSS, AMBER, BERRY = "#F1E3C6", "#BFA88A", "#5FC7D6", "#8DBF5A", "#F0A93B", "#E0556B"
F = "assets/fonts/AtkinsonHyperlegible-"
font = lambda s, b=False: ImageFont.truetype(F + ("Bold" if b else "Regular") + ".ttf", s)
_icons = {}
def icon_img(n, s):
    k = (n, s)
    if k not in _icons:
        p = f"assets/icons/{n}.png"
        _icons[k] = Image.open(p).convert("RGBA").resize((s, s), Image.LANCZOS) if os.path.exists(p) else None
    return _icons[k]


class Canvas:
    def __init__(self):
        self.img = Image.new("RGBA", (W, H), "#A9CF86")
        self.d = ImageDraw.Draw(self.img, "RGBA")
        self.zones = {}

    # ---- primitives
    def plank(self, box, cut=8, fill=BARK):
        x0, y0, x1, y1 = box
        pts = [(x0 + cut, y0), (x1, y0), (x1, y1 - cut), (x1 - cut, y1), (x0, y1), (x0, y0 + cut)]
        self.d.polygon([(x + 3, y + 3) for x, y in pts], fill="#1E150F")  # hard offset shadow
        self.d.polygon(pts, fill=fill)

    def ico(self, n, x, y, s=28):
        im = icon_img(n, s)
        if im: self.img.alpha_composite(im, (int(x), int(y)))
        else: self.d.rounded_rectangle((x, y, x + s, y + s), 8, fill=DIM)

    def text(self, xy, t, size=15, bold=False, fill=PITH, anchor="lm", **k):
        self.d.text(xy, t, font=font(size, bold), fill=fill, anchor=anchor, **k)

    def onmap(self, xy, t, size=15, fill="#FFFFFF", anchor="mm"):
        self.text(xy, t, size, True, fill, anchor, stroke_width=3, stroke_fill=RAIL)

    def btn(self, box, t, ic=None, hot=False, primary=False, size=15):
        x0, y0, x1, y1 = box
        self.plank(box, 6, MOSS if primary else RAISED)
        if hot: self.d.rectangle((x0 + 6, y1 - 4, x1 - 6, y1 - 1), fill=POND)
        col = RAIL if primary else PITH
        if ic:
            self.ico(ic, x0 + 8, (y0 + y1) / 2 - 14, 28)
            self.text((x0 + 42, (y0 + y1) / 2 - 1), t, size, True, col)
        else:
            self.text(((x0 + x1) / 2, (y0 + y1) / 2 - 1), t, size, True, col, "mm")

    def meter(self, x, y, n, of, col=MOSS, w=10, h=8):
        for i in range(of):
            self.d.rectangle((x + i * (w + 4), y, x + i * (w + 4) + w, y + h), fill=col if i < n else LINE)

    def zone(self, n, title, box, tag=True):
        self.zones[n] = box
        self.plank(box)
        if tag:
            self.text((box[0] + 10, box[1] - 14), f"{n}  {title}", 14, True, RAIL, "lm", stroke_width=3, stroke_fill=PITH)

    def row(self, x, y, w, ic, t, v=None, col=PITH):
        self.d.rectangle((x, y, x + w, y + 40), fill=RAISED)
        self.ico(ic, x + 6, y + 6, 28)
        self.text((x + 44, y + 20), t, 15, False, col)
        if v: self.text((x + w - 10, y + 20), v, 15, True, col, "rm")

    # ---- map
    def map(self, pond=True, lodge=True):
        d = self.d
        d.ellipse((520, 400, 1260, 790), fill="#6FD3E0", outline="#F4FBF6", width=5)
        d.ellipse((760, 500, 1040, 680), fill="#8FE0E6")
        for x, y in [(300, 300), (380, 520), (1420, 300), (1500, 700), (240, 820), (1600, 430), (1380, 560), (330, 640)]:
            d.ellipse((x - 46, y - 46, x + 46, y + 46), fill="#5FA85A")
        if lodge:
            d.polygon([(900, 350), (948, 380), (900, 412), (852, 380)], fill="#B9783F", outline=RAIL)

    # ---- fixed zones
    def top(self, speed=1, stores=True):
        self.zone("Z1", "Clock and needs", (16, 32, 556, 92))
        self.ico("autumn", 28, 48, 28); self.text((66, 62), "Autumn  day 42", 16, True)
        for i, ic in enumerate(["food", "shelter", "rest", "mood", "social"]):
            x = 214 + i * 66; self.ico(ic, x, 42, 24); self.meter(x, 72, (4, 5, 3, 4, 2)[i], 4 if i != 1 else 4, w=8, h=6) if False else self.meter(x, 72, min(3, (i % 4) + 1) + (i == 1), 4, w=9, h=6)
        self.zone("Z2", "Stores ribbon (click: full list)", (620, 32, 1500, 92))
        for i, (ic, n) in enumerate([("logs", "142"), ("branches", "37"), ("bark", "20"), ("plants", "58"), ("mud", "12"), ("stone", "9"), ("baskets", "4"), ("food", "31")]):
            x = 640 + i * 108; self.ico(ic, x, 48, 28); self.text((x + 36, 62), n, 17, True)
        self.zone("Z3", "Speed", (1672, 32, 1904, 92))
        for i, ic in enumerate(["pause", "play1", "play2", "play3"]):
            self.btn((1684 + i * 54, 44, 1730 + i * 54, 80), "", None, hot=(i == speed))
            self.ico(ic, 1684 + i * 54 + 9, 48, 28)

    def alerts(self):
        self.zone("Z4", "Alerts", (16, 124, 316, 300))
        for i, (ic, t, col) in enumerate([("alertwinter", "Winter in 6 days", AMBER), ("alertfood", "Burrow: no food", BERRY), ("alertstalled", "Dam site stalled", AMBER), ("alertjoin", "A beaver asks to join", POND)]):
            y = 134 + i * 40; self.ico(ic, 28, y, 28); self.text((66, y + 14), t, 15)

    def modes(self, active=0):
        self.zone("Z7", "Map modes", (16, 996, 296, 1064))
        for i, ic in enumerate(["modewater", "modeforest", "modefood", "modework"]):
            self.btn((28 + i * 66, 1008, 88 + i * 66, 1052), "", None, hot=(i == active)); self.ico(ic, 28 + i * 66 + 16, 1016, 28)

    def toolbar(self, active=None, menu=False, menu_hot=None):
        self.zone("Z6", "Toolbar", (480, 996, 1440, 1064))
        tools = [("build", "Build  B"), ("collect", "Gather  G"), ("haul", "Haul  H"), ("builder", "Jobs  J"), ("demolish", "Demolish  X"), ("navcolony", "Screens")]
        for i, (ic, t) in enumerate(tools):
            self.btn((494 + i * 156, 1008, 640 + i * 156, 1052), t, ic, hot=(active == i), size=14)
        if menu:
            self.plank((500, 836, 1420, 980))
            self.text((516, 852), "Build menu opens above the toolbar: tabs, then buildings", 14, True, DIM)
            tabs = ["Basics", "Food", "Wood", "Water", "Craft"]
            for i, t in enumerate(tabs):
                self.btn((516 + i * 112, 868, 616 + i * 112, 900), t, None, hot=(i == 3), size=14)
            for i, (ic, t) in enumerate([("dam", "Dam"), ("canal", "Canal"), ("lock", "Lock"), ("waterwheel", "Wheel")]):
                bx = 516 + i * 90
                self.btn((bx, 906, bx + 80, 972), "", None, hot=(menu_hot == i)); self.ico(ic, bx + 22, 910, 36); self.text((bx + 40, 960), t, 13, False, PITH, "mm")

    def inspector(self):
        self.zone("Z5", "Inspector (selection)", (1536, 108, 1904, 620))
        x = 1556
        self.ico("lodge", x, 124, 40); self.text((x + 52, 138), "Burrow", 22, True); self.text((x + 52, 162), "Working. 3 of 4 beds used", 14, False, DIM)
        self.row(x, 190, 328, "shelter", "Needs", "stores logs 12")
        self.row(x, 236, 328, "craft", "Makes", "shelter")
        self.row(x, 282, 328, "movearea", "Work area", "8 x 6")
        self.text((x, 346), "Worker slots", 14, True, DIM)
        for k in range(4):
            xx = x + k * 82; self.d.rectangle((xx, 362, xx + 72, 432), fill=RAISED, outline=LINE)
            if k < 3: self.ico("collector", xx + 22, 372, 28); self.text((xx + 36, 416), ["Iris", "Moss", "Fen"][k], 13, False, PITH, "mm")
            else: self.ico("fillslot", xx + 22, 372, 28); self.text((xx + 36, 416), "open", 13, False, AMBER, "mm")
        self.btn((x, 456, x + 156, 500), "Assign", "assign", hot=True, size=14); self.btn((x + 172, 456, x + 328, 500), "Move area", "movearea", size=14)
        self.btn((x, 512, x + 156, 556), "Demolish", "demolish", size=14)
        self.btn((x + 172, 512, x + 328, 556), "Locate", "locate", size=14)

    def finish(self, path):
        self.img.convert("RGB").save(path)


def hud():
    c = Canvas(); c.map(); c.top(); c.alerts(); c.inspector(); c.modes(); c.toolbar(active=None)
    c.d.ellipse((860, 560, 940, 600), outline=POND, width=4); c.onmap((900, 620), "selection ring")
    c.onmap((900, 330), "Burrow")
    c.onmap((1120, 330), "+3 logs", 18, PITH)
    c.d.polygon([(330, 640), (380, 612), (380, 668)], fill=AMBER, outline=RAIL); c.onmap((420, 640), "edge arrow: alert off screen", 14, anchor="lm")
    c.onmap((960, 450), "MAP: hero. Always clear in the middle 1100x700", 20)
    return c


def dam():
    c = Canvas(); c.map(); c.top(); c.alerts(); c.modes(); c.toolbar(active=0, menu=True, menu_hot=0)
    d = c.d
    # flooded cells: Pond hatch + dashed new shoreline, flooded trees get Amber triangles
    ov = Image.new("RGBA", (W, H), (0, 0, 0, 0)); od = ImageDraw.Draw(ov)
    od.ellipse((340, 400, 1440, 830), fill=(40, 110, 200, 70))
    for x in range(340, 1440, 26): od.line((x, 400, x - 90, 830), fill=(40, 110, 200, 90), width=2)
    c.img.alpha_composite(ov)
    import math
    for a in range(0, 360, 4):
        r = math.radians(a); x = 890 + 550 * math.cos(r); y = 615 + 215 * math.sin(r)
        if y > 405: d.ellipse((x - 3, y - 3, x + 3, y + 3), fill=POND)
    for x, y in [(380, 520), (330, 640), (1380, 560)]:
        d.polygon([(x - 14, y + 12), (x, y - 14), (x + 14, y + 12)], fill=AMBER, outline=RAIL)
    d.line((520, 400, 1260, 400), fill="#FFFFFF", width=6); d.line((520, 400, 1260, 400), fill=(255, 255, 255), width=6)
    for x in (520, 1260): d.rectangle((x - 9, 391, x + 9, 409), fill="#FFFFFF", outline=RAIL)
    c.onmap((890, 372), "dam line: drag ends to change; ghost is white", 14)
    c.onmap((430, 800), "water will rise here", 15, POND)
    c.onmap((330, 560), "flooded trees", 14, AMBER)
    # side tool panel: replaces the inspector in Z5
    c.zone("Z5", "Tool panel (replaces inspector)", (1536, 108, 1904, 600))
    x = 1556
    c.ico("dam", x, 124, 40); c.text((x + 52, 138), "Dam", 22, True); c.text((x + 52, 162), "Preview. Nothing is built yet", 14, False, DIM)
    c.row(x, 190, 328, "modewater", "Water rises", "0.38")
    c.row(x, 236, 328, "modewater", "Floods", "874 cells")
    c.row(x, 282, 328, "modeforest", "Trees flooded", "535", AMBER)
    c.row(x, 328, 328, "shelter", "Buildings flooded", "0", MOSS)
    c.row(x, 374, 328, "logs", "Cost", "8 logs, 10 sticks")
    c.row(x, 420, 328, "rest", "Pond fills in", "16 days")
    c.text((x, 478), "Height", 14, True, DIM); c.btn((x + 70, 462, x + 114, 494), "-", None, size=16); c.btn((x + 124, 462, x + 168, 494), "+", None, size=16)
    c.btn((x, 520, x + 156, 570), "Confirm  Enter", "accept", primary=True, size=14); c.btn((x + 172, 520, x + 328, 570), "Cancel  Esc", "cancel", size=14)
    c.onmap((1030, 1100 - 300), "Right click also cancels. Confirm lays the site; beavers build it", 14)
    return c


def settle():
    c = Canvas(); c.map(lodge=False); c.top(speed=0); c.modes()
    d = c.d
    cands = [(420, 560, "West bank"), (960, 320, "North shore"), (1500, 560, "East meadow")]
    for i, (x, y, n) in enumerate(cands):
        sel = i == 1
        d.rounded_rectangle((x - 70, y - 40, x + 70, y + 40), 6, outline=POND if sel else "#FFFFFF", width=5 if sel else 3)
        for k in range(-60, 60, 14): d.line((x + k, y + 40, x + k + 8, y + 40), fill=RAIL, width=2)
        d.rectangle((x - 14, y - 14, x + 14, y + 14), fill="#FFFFFF", outline=RAIL, width=3)  # info marker: square
        c.onmap((x, y + 62), n, 16)
    c.onmap((960, 150), "1  Choose where the family settles", 24, PITH)
    c.zone("Z6", "Settle card (replaces toolbar while choosing)", (560, 900, 1360, 1064))
    x = 584
    c.text((x, 930), "North shore", 22, True); c.text((x, 956), "Dry ground, 12 trees within 8 cells, 6 cells from deep water", 14, False, DIM)
    for i, (ic, t, v) in enumerate([("logs", "Wood nearby", "good"), ("food", "Forage", "fair"), ("modewater", "Flood risk", "low")]):
        c.row(x + i * 250, 976, 236, ic, t, v)
    c.btn((1090, 1018, 1230, 1058), "Settle here", "accept", primary=True, size=14); c.btn((1240, 1018, 1340, 1058), "Cancel", None, size=14)
    c.onmap((1000, 850), "Time is paused until the family settles. Click a bank to choose it", 14)
    return c


def jobs():
    c = Canvas(); c.map(); c.top(); c.alerts(); c.modes(); c.toolbar(active=1)
    d = c.d
    # drag-gather rectangle (selected area)
    ov = Image.new("RGBA", (W, H), (0, 0, 0, 0)); ImageDraw.Draw(ov).rectangle((1240, 250, 1500, 400), fill=(95, 199, 214, 70)); c.img.alpha_composite(ov)
    for x in range(1240, 1500, 16): d.line((x, 250, x + 8, 250), fill=POND, width=4); d.line((x, 400, x + 8, 400), fill=POND, width=4)
    for y in range(250, 400, 16): d.line((1240, y, 1240, y + 8), fill=POND, width=4); d.line((1500, y, 1500, y + 8), fill=POND, width=4)
    for x, y in [(1260, 400), (1500, 400), (1500, 250), (1240, 250)]: d.rectangle((x - 7, y - 7, x + 7, y + 7), fill="#FFFFFF", outline=RAIL, width=2)
    c.onmap((1370, 220), "12 trees, 2 beavers will gather", 15)
    c.plank((1514, 400, 1660, 434)); c.ico("collect", 1520, 403, 28); c.text((1554, 417), "Gather", 14, True)
    # jobs panel: centred screen
    box = (440, 460, 1480, 980)
    c.zone("L3", "Jobs panel (J). Esc closes", box)
    x0 = 464
    c.text((x0, 490), "Jobs", 22, True); c.text((x0 + 80, 490), "Idle adults 1", 15, False, DIM)
    c.text((x0 + 330, 520), "Workers", 13, True, DIM, "mm"); c.text((x0 + 560, 520), "Priority (1 first, off)", 13, True, DIM, "mm")
    jobs_ = [("collector", "Logger", 2, 1), ("collector", "Forager", 2, 2), ("builder", "Builder", 1, 1), ("specialist", "Digger", 0, 3), ("hauler", "Hauler", 1, 2), ("craft", "Crafter", 0, 0)]
    for i, (ic, n, w, p) in enumerate(jobs_):
        y = 536 + i * 50; c.d.rectangle((x0, y, x0 + 1000, y + 42), fill=RAISED); c.ico(ic, x0 + 8, y + 7, 28); c.text((x0 + 48, y + 21), n, 16, True)
        c.btn((x0 + 270, y + 5, x0 + 300, y + 37), "-", None, size=16); c.text((x0 + 330, y + 21), str(w), 17, True, PITH, "mm"); c.btn((x0 + 360, y + 5, x0 + 390, y + 37), "+", None, size=16)
        for k in range(5):
            hot = (k == p)
            lab = ["off", "1", "2", "3", "4"][k]
            bx = x0 + 470 + k * 60
            c.btn((bx, y + 5, bx + 52, y + 37), lab, None, hot=hot, size=14)
    c.text((x0, 850), "Hover a number to see where it comes from. Per-building slots are in the inspector.", 14, False, DIM)
    c.text((x0, 880), "Beavers", 14, True, DIM)
    for i, (n, j) in enumerate([("Iris", "Logger"), ("Moss", "Logger"), ("Fen", "Builder"), ("Ash", "Idle")]):
        xx = x0 + i * 240; c.d.rectangle((xx, 900, xx + 226, 940), fill=RAISED); c.ico("collector", xx + 6, 906, 28); c.text((xx + 44, 920), f"{n}  {j}", 14, False, AMBER if j == "Idle" else PITH)
    c.onmap((560, 420), "composite: gather drag (top right) and the Jobs panel (J) shown together", 14)
    return c


if __name__ == "__main__":
    out = sys.argv[1] if len(sys.argv) > 1 else "docs"
    for name, fn in [("ui_wireframe", hud), ("ui_wireframe_dam", dam), ("ui_wireframe_settle", settle), ("ui_wireframe_jobs", jobs)]:
        c = fn(); c.finish(os.path.join(out, name + ".png"))
        cover = sum((b[2] - b[0]) * (b[3] - b[1]) for k, b in c.zones.items())
        print(name, "zones cover %.1f%%" % (100 * cover / (W * H)))
