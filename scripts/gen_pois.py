#!/usr/bin/env python3
"""Parse docs/locations/points_of_interest/POI_Archive_*.md into
assets/data/pois.json, capped at 40 POIs per act (deterministic md5 pick).

Doc coordinates span ±8000; the playable world is ±1800 (WORLD_BOUNDS), so
positions scale by 1800/8000 = 0.225 and clamp to ±1600 to stay in bounds.
"""
import glob
import hashlib
import json
import os
import re

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
SCALE = 1800.0 / 8000.0
CLAMP = 1600.0
PER_ACT_CAP = 40

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

entry_re = re.compile(
    r"### POI-(\d+): (.+?)\n"
    r"\*\*Region:\*\* (.+?) \| \*\*Coordinates:\*\* \[(-?\d+), (-?\d+)\]",
    re.M,
)

pois = []
for path in sorted(glob.glob(os.path.join(ROOT, "docs/locations/points_of_interest/POI_Archive_*.md"))):
    for m in entry_re.finditer(open(path).read()):
        pid, name, region, x, y = m.groups()
        act = act_for(region)
        if act is None:
            continue
        clamp = lambda v: max(-CLAMP, min(CLAMP, float(v) * SCALE))
        pois.append({
            "id": int(pid),
            "name": name.strip(),
            "act": act,
            "x": round(clamp(x), 1),
            "y": round(clamp(y), 1),
        })

total = len(pois)
kept = []
for act in ["eden", "hermon", "nephilim", "enoch", "flood"]:
    in_act = [p for p in pois if p["act"] == act]
    in_act.sort(key=lambda p: hashlib.md5(str(p["id"]).encode()).hexdigest())
    kept.extend(sorted(in_act[:PER_ACT_CAP], key=lambda p: p["id"]))

out = os.path.join(ROOT, "assets/data/pois.json")
json.dump(kept, open(out, "w"), indent=0)
print(f"parsed {total}, kept {len(kept)} (cap {PER_ACT_CAP}/act, dropped {total - len(kept)}) -> {out}")
print("per act:", {a: sum(1 for p in kept if p['act'] == a) for a in ['eden','hermon','nephilim','enoch','flood']})
