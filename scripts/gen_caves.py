#!/usr/bin/env python3
"""Parse docs/locations/caves/Cave_Archive_*.md into assets/data/caves.json,
capped at 6 caves per act (deterministic md5 pick, same recipe as gen_pois).

Doc coordinates span ±8000 on [x, _, z]; the playable world is ±3600
(WORLD_BOUNDS), so positions scale by 0.45 and clamp to ±3200. Caves are
kept at least 700 from the zone entry (the inn) so interiors never overlap
the safe ring.
"""
import glob
import hashlib
import json
import os
import re

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
SCALE = 3600.0 / 8000.0
CLAMP = 3200.0
PER_ACT_CAP = 6
MIN_FROM_ENTRY = 700.0

ACT_MAP = [
    ("edenic basin", "eden"), ("plains of nod", "eden"), ("eden", "eden"),
    ("hermon", "hermon"),
    ("nephilim wastes", "nephilim"), ("nephilim", "nephilim"),
    ("city of enoch", "enoch"), ("enoch", "enoch"),
    ("abyssal basin", "flood"), ("flood", "flood"),
]

def act_for(region):
    r = region.lower()
    for key, act in ACT_MAP:
        if key in r:
            return act
    return None

def slug(name):
    return re.sub(r"[^a-z0-9]+", "_", name.lower()).strip("_")

ENTRY = re.compile(
    r"### CAVE-(\d+): (.+?)\n\*\*Region:\*\* (.+?) \| \*\*Coordinates:\*\* \[(-?\d+), (-?\d+), (-?\d+)\]"
    r".*?\*\*Primary Resources:\*\* (.+?)\n",
    re.S,
)

caves = []
for path in sorted(glob.glob(os.path.join(ROOT, "docs/locations/caves/Cave_Archive_*.md"))):
    for m in ENTRY.finditer(open(path).read()):
        cid, name, region, x, _, z, res = m.groups()
        act = act_for(region)
        if act is None:
            continue
        cx = max(-CLAMP, min(CLAMP, float(x) * SCALE))
        cy = max(-CLAMP, min(CLAMP, float(z) * SCALE))
        if (cx * cx + cy * cy) ** 0.5 < MIN_FROM_ENTRY:
            continue
        resources = sorted({slug(r) for r in res.split(",")})
        caves.append({"id": int(cid), "name": name.strip(), "act": act,
                      "x": round(cx, 1), "y": round(cy, 1), "resources": resources})

by_act = {}
for c in caves:
    by_act.setdefault(c["act"], []).append(c)

picked = []
for act, lst in by_act.items():
    lst.sort(key=lambda c: hashlib.md5(f"{c['id']}{c['name']}".encode()).hexdigest())
    picked.extend(sorted(lst[:PER_ACT_CAP], key=lambda c: c["id"]))

picked.sort(key=lambda c: (c["act"], c["id"]))
out = os.path.join(ROOT, "assets/data/caves.json")
json.dump(picked, open(out, "w"), indent=1)
print(f"{len(picked)} caves -> {out}")
for act, lst in sorted(by_act.items()):
    print(f"  {act}: {min(len(lst), PER_ACT_CAP)}")
