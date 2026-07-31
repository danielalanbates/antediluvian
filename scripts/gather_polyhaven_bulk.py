#!/usr/bin/env python3
"""Bulk-gather every fantasy-appropriate CC0 model from Poly Haven.

Supersedes the 15-model hand-typed list in gather_assets.py by walking the full
catalogue (521 models) and keeping anything whose categories fit a medieval /
fantasy world. All Poly Haven content is CC0: commercial OK, no attribution.

Run: python3 scripts/gather_polyhaven_bulk.py [--dry-run] [--budget-gb N]
"""
import json, os, sys, time, urllib.request, urllib.error

ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", "assets"))
API = "https://api.polyhaven.com"
RES = "1k"
UA = {"User-Agent": "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AntediluviaAssetGather/1.0"}

# Categories that fit a pre-flood / medieval fantasy world.
KEEP = {
    "nature", "plants", "trees", "rocks", "ground cover", "props", "containers",
    "food", "tools", "furniture", "seating", "table", "shelves", "structures",
    "decorative", "lighting", "fabric", "wood", "stone", "clutter", "kitchen",
}
# Anything modern / industrial / sci-fi disqualifies a model outright.
BAN = {
    "industrial", "electronics", "appliances", "vehicles", "cars", "sci-fi",
    "office", "medical", "sports", "military", "musical instruments", "signage",
    "bathroom", "gym", "construction",
}

DRY = "--dry-run" in sys.argv
BUDGET = 8.0
if "--budget-gb" in sys.argv:
    BUDGET = float(sys.argv[sys.argv.index("--budget-gb") + 1])
BUDGET_BYTES = BUDGET * 1e9


def get(url, tries=3):
    for i in range(tries):
        try:
            req = urllib.request.Request(url, headers=UA)
            with urllib.request.urlopen(req, timeout=60) as r:
                return json.load(r)
        except Exception as e:
            if i == tries - 1:
                raise
            time.sleep(2 * (i + 1))


def download(url, dest):
    if os.path.exists(dest) and os.path.getsize(dest) > 0:
        return 0
    os.makedirs(os.path.dirname(dest), exist_ok=True)
    req = urllib.request.Request(url, headers=UA)
    for i in range(3):
        try:
            with urllib.request.urlopen(req, timeout=180) as r:
                data = r.read()
            break
        except Exception:
            if i == 2:
                raise
            time.sleep(2 * (i + 1))
    tmp = dest + ".part"
    with open(tmp, "wb") as f:
        f.write(data)
    os.replace(tmp, dest)
    return len(data)


def wanted(meta):
    cats = {c.lower() for c in meta.get("categories", [])}
    tags = {t.lower() for t in meta.get("tags", [])}
    if (cats | tags) & BAN:
        return False
    return bool(cats & KEEP)


print("Fetching Poly Haven model catalogue...")
catalogue = get(f"{API}/assets?type=models")
picks = sorted(s for s, m in catalogue.items() if wanted(m))
print(f"{len(picks)}/{len(catalogue)} models match fantasy filter (budget {BUDGET} GB)")

if DRY:
    for s in picks:
        print(" ", s, catalogue[s].get("categories"))
    sys.exit(0)

total, ok, skipped, failed = 0, 0, 0, 0
for n, slug in enumerate(picks, 1):
    if total > BUDGET_BYTES:
        print(f"!! budget {BUDGET} GB reached at {n}/{len(picks)} — stopping")
        break
    base = os.path.join(ROOT, "models", "polyhaven", slug)
    marker = os.path.join(base, ".complete")
    if os.path.exists(marker):
        skipped += 1
        continue
    try:
        files = get(f"{API}/files/{slug}")
        entry = files["gltf"][RES]["gltf"]
    except Exception as e:
        print(f"[{n}/{len(picks)}] {slug}: NO gltf/{RES} ({e})")
        failed += 1
        continue
    try:
        got = download(entry["url"], os.path.join(base, os.path.basename(entry["url"])))
        for rel, m in entry.get("include", {}).items():
            got += download(m["url"], os.path.join(base, rel))
        open(marker, "w").close()
        total += got
        ok += 1
        print(f"[{n}/{len(picks)}] {slug}: ok {got/1e6:.1f} MB (run total {total/1e9:.2f} GB)")
    except Exception as e:
        print(f"[{n}/{len(picks)}] {slug}: FAILED {e}")
        failed += 1

print(f"\nDone. new={ok} already-had={skipped} failed={failed} downloaded={total/1e9:.2f} GB")
print("License: all Poly Haven assets are CC0 (public domain, commercial OK, no attribution).")
