#!/usr/bin/env python3
"""Build the stacked terrain texture arrays used by terrain.wgsl.

Downloads four CC0 Poly Haven texture sets at 2K (grass, dirt, cliff rock,
sand) into assets/textures/terrain_src/ (gitignored) and packs each map type
into one vertically stacked 1024 x 4096 JPEG in assets/textures/terrain/.
Layer order MUST match terrain.wgsl: 0 grass, 1 dirt, 2 cliff, 3 sand.

All Poly Haven assets are CC0 (public domain, commercial use OK).
The API 403s without a User-Agent header.
"""
import json, os, urllib.request
from PIL import Image

ROOT = os.path.join(os.path.dirname(__file__), "..", "assets", "textures")
SRC = os.path.join(ROOT, "terrain_src")
OUT = os.path.join(ROOT, "terrain")
LAYERS = ["leafy_grass", "dirt_floor", "cliff_side", "coast_sand_01"]
MAPS = [("Diffuse", "diffuse", 90), ("nor_gl", "nor_gl", 95), ("arm", "arm", 90)]
UA = {"User-Agent": "antediluvia-assets"}
SIZE = 1024

os.makedirs(SRC, exist_ok=True)
os.makedirs(OUT, exist_ok=True)
for slug in LAYERS:
    files = json.load(urllib.request.urlopen(urllib.request.Request(f"https://api.polyhaven.com/files/{slug}", headers=UA)))
    for key, name, _ in MAPS:
        path = os.path.join(SRC, f"{slug}_{name}_2k.jpg")
        if not os.path.exists(path):
            url = files[key]["2k"]["jpg"]["url"]
            open(path, "wb").write(urllib.request.urlopen(urllib.request.Request(url, headers=UA)).read())
for _, name, q in MAPS:
    sheet = Image.new("RGB", (SIZE, SIZE * len(LAYERS)))
    for i, slug in enumerate(LAYERS):
        img = Image.open(os.path.join(SRC, f"{slug}_{name}_2k.jpg")).convert("RGB")
        sheet.paste(img.resize((SIZE, SIZE), Image.LANCZOS), (0, SIZE * i))
    dst = os.path.join(OUT, f"terrain_{name}_array.jpg")
    sheet.save(dst, quality=q)
    print(dst, os.path.getsize(dst) // 1024, "KB")
