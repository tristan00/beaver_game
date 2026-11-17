"""Draws docs/ui_wireframe.png (1920x1080) from the zone table in docs/UI_SPEC.md section 1.
Usage: python tools/make_ui_wireframe.py [out.png]   Needs Pillow and assets/fonts."""
import sys
from PIL import Image, ImageDraw, ImageFont

W, H = 1920, 1080
BARK, RAIL, LINE = "#3F2E24", "#2A1F19", "#5B4636"
PITH, DIM, POND, MOSS, AMBER, BERRY = "#F1E3C6", "#BFA88A", "#5FC7D6", "#8DBF5A", "#F0A93B", "#E0556B"
F = "assets/fonts/AtkinsonHyperlegible-"
font = lambda s, b=False: ImageFont.truetype(F + ("Bold" if b else "Regular") + ".ttf", s)

img = Image.new("RGB", (W, H), "#A9CF86")
d = ImageDraw.Draw(img)
# map placeholder: pond, bank, trees
d.ellipse((560, 380, 1240, 760), fill="#6FD3E0", outline="#F4FBF6", width=5)
d.ellipse((760, 470, 1040, 650), fill="#8FE0E6")
for x, y in [(300, 300), (380, 520), (1420, 300), (1500, 700), (240, 820), (1600, 430)]:
    d.ellipse((x - 46, y - 46, x + 46, y + 46), fill="#5FA85A")
d.polygon([(900, 330), (948, 360), (900, 392), (852, 360)], fill="#B9783F")  # lodge

def plank(box, cut=8, fill=BARK):
    x0, y0, x1, y1 = box
    pts = [(x0 + cut, y0), (x1, y0), (x1, y1 - cut), (x1 - cut, y1), (x0, y1), (x0, y0 + cut)]
    d.polygon([(x + 3, y + 3) for x, y in pts], fill="#1E150F")  # hard offset shadow, no blur
    d.polygon(pts, fill=fill)

def tag(box, text, hot=False, size=15):
    plank(box, 6, "#4D3A2D" if not hot else "#4D3A2D")
    x0, y0, x1, y1 = box
    if hot:
        d.rectangle((x0 + 6, y1 - 4, x1 - 6, y1 - 1), fill=POND)  # waterline
    d.text(((x0 + x1) / 2, (y0 + y1) / 2 - 2), text, font=font(size, True), fill=PITH, anchor="mm")

def glyph(x, y, s=28, c=DIM):
    d.rounded_rectangle((x, y, x + s, y + s), 8, fill=c)  # icon placeholder tile

def label(box, num, title):
    x0, y0, x1, y1 = box
    d.text((x0 + 10, y0 - 22), f"{num}  {title}", font=font(15, True), fill=RAIL, stroke_width=3, stroke_fill="#F1E3C6")

zones = {}
def zone(n, title, box):
    zones[n] = (title, box); plank(box); label(box, n, title)

# Z1 clock + needs
zone("Z1", "Clock and needs", (16, 40, 556, 100))
glyph(30, 56, 28, AMBER); d.text((68, 70), "Autumn  day 42", font=font(16, True), fill=PITH, anchor="lm")
for i in range(5):
    x = 214 + i * 66; glyph(x, 52, 24, DIM); d.rectangle((x, 82, x + 50, 88), fill=LINE); d.rectangle((x, 82, x + 14 + i * 7, 88), fill=MOSS)
# Z2 stores ribbon
zone("Z2", "Stores ribbon", (620, 40, 1500, 100))
for i, n in enumerate(["142", "37", "20", "58", "12", "9", "4", "2"]):
    x = 640 + i * 108; glyph(x, 56, 28, "#A07F60"); d.text((x + 36, 70), n, font=font(17, True), fill=PITH, anchor="lm")
# Z3 speed
zone("Z3", "Speed", (1672, 40, 1904, 100))
for i, t in enumerate(["II", "1x", "2x", "3x"]):
    tag((1684 + i * 54, 52, 1730 + i * 54, 88), t, hot=(i == 1), size=14)
# Z4 alerts
zone("Z4", "Alerts", (16, 132, 316, 324))
for i, t in enumerate(["Winter in 6 days", "Lodge: no food", "Dam site stalled", "Beaver asks to join"]):
    y = 142 + i * 44; d.polygon([(30, y + 30), (46, y), (62, y + 30)], fill=AMBER); d.text((74, y + 15), t, font=font(15), fill=PITH, anchor="lm")
# Z5 inspector (building first)
zone("Z5", "Inspector: building first", (1536, 132, 1904, 700))
d.text((1556, 162), "Lodge", font=font(22, True), fill=PITH, anchor="lm"); d.text((1556, 190), "Working. 5 of 6 beds used", font=font(15), fill=DIM, anchor="lm")
for r, t in enumerate(["Workers  3 / 4", "Needs  stores wood 12", "Makes  shelter", "Work area  8 x 6"]):
    y = 226 + r * 52; d.rectangle((1556, y, 1884, y + 40), fill="#4D3A2D"); glyph(1564, y + 6, 28, DIM); d.text((1604, y + 20), t, font=font(15), fill=PITH, anchor="lm")
for k in range(4):
    x = 1556 + k * 82; d.rectangle((x, 450, x + 72, 520), fill="#4D3A2D", outline=LINE); d.text((x + 36, 540), "slot " + str(k + 1), font=font(13), fill=DIM, anchor="mm")
tag((1556, 580, 1706, 624), "Assign", True); tag((1722, 580, 1884, 624), "Move area"); tag((1556, 636, 1706, 680), "Demolish")
# Z6 toolbar, Z7 modes
zone("Z6", "Build and orders", (560, 996, 1360, 1064))
for i, t in enumerate(["Build", "Assign", "Move area", "Haul", "Demolish", "Screens"]):
    tag((574 + i * 130, 1008, 692 + i * 130, 1052), t, hot=(i == 0))
zone("Z7", "Map modes", (16, 996, 296, 1064))
for i, t in enumerate(["Water", "Forest", "Food", "Work"]):
    tag((28 + i * 66, 1008, 88 + i * 66, 1052), t[:3], hot=(i == 0), size=13)
# L3 screen, dashed
x0, y0, x1, y1 = 330, 150, 1510, 900
for x in range(x0, x1, 24): d.line((x, y0, x + 12, y0), fill=RAIL, width=3); d.line((x, y1, x + 12, y1), fill=RAIL, width=3)
for y in range(y0, y1, 24): d.line((x0, y, x0, y + 12), fill=RAIL, width=3); d.line((x1, y, x1, y + 12), fill=RAIL, width=3)
d.text(((x0 + x1) / 2, 172), "L3 screen: opens centred on demand, one at a time, Esc closes (not drawn)", font=font(16, True), fill=RAIL, anchor="mm", stroke_width=3, stroke_fill="#F1E3C6")
# map feedback examples
d.rectangle((600, 790, 760, 880), outline="#FFFFFF", width=3); d.text((680, 835), "ghost: valid", font=font(14, True), fill="#FFFFFF", anchor="mm", stroke_width=3, stroke_fill=RAIL)
for x in range(380, 520, 16): d.line((x, 420, x + 8, 420), fill=RAIL, width=3); d.line((x, 480, x + 8, 480), fill=RAIL, width=3)
d.text((450, 450), "work area", font=font(14, True), fill=PITH, anchor="mm", stroke_width=3, stroke_fill=RAIL)
d.ellipse((880, 640, 950, 680), outline=POND, width=4); d.text((915, 700), "selection ring", font=font(14, True), fill="#FFFFFF", anchor="mm", stroke_width=3, stroke_fill=RAIL)
d.polygon([(330, 640), (380, 612), (380, 668)], fill=AMBER); d.text((420, 640), "alert edge arrow", font=font(14, True), fill="#FFFFFF", anchor="lm", stroke_width=3, stroke_fill=RAIL)
d.text((1120, 330), "+3 logs", font=font(18, True), fill=PITH, anchor="mm", stroke_width=4, stroke_fill=RAIL)
d.text((960, 540), "MAP: hero", font=font(34, True), fill="#FFFFFF", anchor="mm", stroke_width=4, stroke_fill=RAIL)

cover = sum((b[2] - b[0]) * (b[3] - b[1]) for _, b in zones.values())
print("always-on zones cover %.1f%% of the screen" % (100 * (sum((b[2]-b[0])*(b[3]-b[1]) for n,(t,b) in zones.items() if n!="Z5")) / (W*H)),
      "; with inspector %.1f%%" % (100 * cover / (W * H)))
img.save(sys.argv[1] if len(sys.argv) > 1 else "docs/ui_wireframe.png")
