#!/usr/bin/env python3
"""Gather curated CC0 assets from Poly Haven (fully free, no attribution) into
the project. Models come as glTF + all texture/bin dependencies (from the API's
`include` map). HDRIs and PBR material sets download as flat files.

All Poly Haven content is CC0: free for any use, commercial included, no
attribution required. Run: python3 scripts/gather_assets.py
"""
import json, os, sys, urllib.request

ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", "assets"))
API = "https://api.polyhaven.com"

MODELS = [
    "fir_tree_01", "pine_tree_01", "island_tree_01", "island_tree_02",
    "dead_tree_trunk", "dead_tree_trunk_02", "boulder_01",
    "namaqualand_boulder_02", "coast_rocks_01", "rock_moss_set_01",
    "shrub_01", "fern_02", "moss_01", "root_cluster_01", "dry_branches_medium_01",
]
HDRIS = [
    "autumn_field_puresky", "qwantani_puresky", "syferfontein_18d_clear_puresky",
    "kloppenheim_06_puresky", "belfast_sunset_puresky",
]
TEXTURES = [
    "aerial_grass_rock", "brown_mud_leaves_01", "forrest_ground_01",
    "forest_leaves_02", "rocky_terrain_02", "dry_riverbed_rock",
    "mossy_cobblestone", "coast_sand_rocks_02",
]
MODEL_RES, HDRI_RES, TEX_RES = "1k", "2k", "1k"
TEX_MAPS = ("Diffuse", "nor_gl", "Rough", "arm", "AO", "Displacement")

UA = {"User-Agent": "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AntediluviaAssetGather/1.0"}

def get(url):
    req = urllib.request.Request(url, headers=UA)
    with urllib.request.urlopen(req, timeout=60) as r:
        return json.load(r)

def download(url, dest):
    if os.path.exists(dest) and os.path.getsize(dest) > 0:
        return os.path.getsize(dest)
    os.makedirs(os.path.dirname(dest), exist_ok=True)
    req = urllib.request.Request(url, headers=UA)
    with urllib.request.urlopen(req, timeout=120) as r:
        data = r.read()
    with open(dest, "wb") as f:
        f.write(data)
    return len(data)

total = 0

def gather_model(slug):
    global total
    try:
        files = get(f"{API}/files/{slug}")
        entry = files["gltf"][MODEL_RES]["gltf"]
    except Exception as e:
        print(f"  model {slug}: skip ({e})"); return
    base = os.path.join(ROOT, "models", "polyhaven", slug)
    total += download(entry["url"], os.path.join(base, os.path.basename(entry["url"])))
    for rel, meta in entry.get("include", {}).items():
        total += download(meta["url"], os.path.join(base, rel))
    print(f"  model {slug}: ok ({1+len(entry.get('include',{}))} files)")

def gather_hdri(slug):
    global total
    try:
        files = get(f"{API}/files/{slug}")
        hdr = files["hdri"][HDRI_RES]["hdr"]
    except Exception as e:
        print(f"  hdri {slug}: skip ({e})"); return
    dest = os.path.join(ROOT, "hdri", f"{slug}_{HDRI_RES}.hdr")
    total += download(hdr["url"], dest)
    print(f"  hdri {slug}: ok")

def gather_texture(slug):
    global total
    try:
        files = get(f"{API}/files/{slug}")
    except Exception as e:
        print(f"  tex {slug}: skip ({e})"); return
    base = os.path.join(ROOT, "textures", "pbr", slug)
    n = 0
    for m in TEX_MAPS:
        node = files.get(m)
        if not node or TEX_RES not in node:
            continue
        fmt = node[TEX_RES].get("jpg") or next(iter(node[TEX_RES].values()))
        total += download(fmt["url"], os.path.join(base, os.path.basename(fmt["url"])))
        n += 1
    print(f"  tex {slug}: ok ({n} maps)")

print("Gathering CC0 nature models...")
for s in MODELS: gather_model(s)
print("Gathering CC0 sky HDRIs...")
for s in HDRIS: gather_hdri(s)
print("Gathering CC0 PBR terrain textures...")
for s in TEXTURES: gather_texture(s)
print(f"\nDone. Total downloaded this run: {total/1e6:.1f} MB")
print("License: all Poly Haven assets are CC0 (public domain, commercial OK, no attribution).")
