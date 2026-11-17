"""Builds artifacts/R19-18/contact_sheet.png from six `--showcase poses` screenshots.
Usage: python tools/contact_sheet.py OUT.png poses_0.png ... poses_5.png"""
import sys
from PIL import Image, ImageDraw
LABELS = ["walking", "swimming", "climbing over a dam", "working (building)", "resting", "carrying a log by water"]
out, files = sys.argv[1], sys.argv[2:8]
cw, ch = 640, 420
sheet = Image.new("RGB", (cw * 3, ch * 2))
d = ImageDraw.Draw(sheet)
for i, f in enumerate(files):
    im = Image.open(f).convert("RGB")
    w, h = im.size
    c = im.crop((w // 2 - cw // 2, h // 2 - ch // 2 - 20, w // 2 + cw // 2, h // 2 + ch // 2 - 20))
    x, y = (i % 3) * cw, (i // 3) * ch
    sheet.paste(c, (x, y))
    d.rectangle((x, y, x + cw - 1, y + 26), fill=(30, 30, 30))
    d.text((x + 10, y + 7), f"{i + 1}. {LABELS[i]}", fill=(255, 255, 255))
    d.rectangle((x, y, x + cw - 1, y + ch - 1), outline=(30, 30, 30))
sheet.save(out)
