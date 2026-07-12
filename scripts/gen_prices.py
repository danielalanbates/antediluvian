#!/usr/bin/env python3
"""Generate assets/data/prices.json (CHUNK C11) from the bestiary.

Vendor price per drop item = act-tier base scaled by rarity: the fewer mobs
that drop an item, the pricier it is. Crafted/quest/vendor staples get
hand-tuned prices on top. Deterministic — rerunnable any time mobs.json
changes.
"""
import json, math, os
from collections import Counter, defaultdict

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
ACT_TIER = {"eden": 0, "hermon": 1, "nephilim": 2, "enoch": 3, "flood": 4}

mobs = json.load(open(f"{ROOT}/assets/data/mobs.json"))
count = Counter()
best_tier = defaultdict(int)
for m in mobs:
    t = ACT_TIER.get(m["act"], 0)
    for d in m["drops"]:
        count[d] += 1
        best_tier[d] = max(best_tier[d], t)

prices = {}
for item, n in count.items():
    tier = best_tier[item]
    base = 4 + 6 * tier                      # act-tier floor
    rarity = max(1.0, 40.0 / n)              # dropped by few mobs -> pricier
    prices[item] = int(round(base * math.sqrt(rarity)))

# Hand-tuned staples, craftables and quest rewards (override generated).
prices.update({
    "bread": 2, "fruit": 3, "healing_potion": 20, "taming_lasso": 35,
    "hide_vest": 45, "stone_axe": 12, "oak_staff": 14, "bronze_sword": 30,
    "thick_hide": 25, "orichalcum_ore": 40, "orichalcum_blade": 220,
    "luminous_charm": 180, "lineage_blade": 150, "lineage_mantle": 300,
    "cainite_trophy": 8, "dire_wolf_horn": 500,
})

out = f"{ROOT}/assets/data/prices.json"
json.dump(dict(sorted(prices.items())), open(out, "w"), indent=1)
print(f"wrote {out}: {len(prices)} items, "
      f"min {min(prices.values())} max {max(prices.values())}")
