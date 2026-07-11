#!/usr/bin/env python3
"""Parse docs/mobs/Bestiary_Vol_*.md (2,500 entries) into assets/data/mobs.json.

Output entry: {id, name, tag, level_min, level_max, habitat, act, temperament,
tameable, mount, drops[]}. `tag` is the snake_cased name (quest/spawn key);
`act` maps the habitat string onto the five zones. Rerunnable; commit output.
"""
import glob
import json
import os
import re
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))

ACT_MAP = [
    ("edenic basin", "eden"), ("plains of nod", "eden"), ("eden", "eden"),
    ("hermon", "hermon"),
    ("nephilim wastes", "nephilim"), ("nephilim", "nephilim"),
    ("city of enoch", "enoch"), ("enoch", "enoch"),
    ("abyssal basin", "flood"), ("flood", "flood"),
]

def snake(s):
    return re.sub(r"[^a-z0-9]+", "_", s.lower()).strip("_")

def act_for(habitat):
    h = habitat.lower()
    for key, act in ACT_MAP:
        if key in h:
            return act
    return None

entry_re = re.compile(
    r"### ID-(\d+): (.+?)\n"
    r"\*\*Level Range:\*\* (\d+)-(\d+) \| \*\*Habitat:\*\* (.+?)\n"
    r"\*\*Temperament:\*\* (.+?) \| \*\*Tameable:\*\* (.+?)\n"
    r"\*\*Common Drops:\*\* (.+?)\n",
    re.M,
)

mobs = []
for path in sorted(glob.glob(os.path.join(ROOT, "docs/mobs/Bestiary_Vol_*.md"))):
    text = open(path).read()
    for m in entry_re.finditer(text):
        eid, name, lo, hi, habitat, temper, tame, drops = m.groups()
        tame = tame.strip()
        mobs.append({
            "id": int(eid),
            "name": name.strip(),
            "tag": snake(name),
            "level_min": int(lo),
            "level_max": int(hi),
            "habitat": habitat.strip(),
            "act": act_for(habitat),
            "temperament": temper.strip().lower(),
            "tameable": tame.lower().startswith("yes"),
            "mount": (re.search(r"\((.+)\)", tame).group(1) if tame.lower().startswith("yes") and "(" in tame else None),
            "drops": [snake(d) for d in drops.split(",")],
        })

unmapped = [m for m in mobs if m["act"] is None]
out = os.path.join(ROOT, "assets/data/mobs.json")
os.makedirs(os.path.dirname(out), exist_ok=True)
json.dump(mobs, open(out, "w"), indent=0)
print(f"{len(mobs)} entries -> {out}")
print("acts:", {a: sum(1 for m in mobs if m['act'] == a) for a in ['eden','hermon','nephilim','enoch','flood', None]})
if unmapped:
    print(f"WARNING: {len(unmapped)} unmapped habitats:", sorted({m['habitat'] for m in unmapped})[:10], file=sys.stderr)
