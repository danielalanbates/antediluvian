#!/bin/bash
# Re-download the CC0 Quaternius (OpenGameArt) + KayKit (GitHub) packs into
# assets/models/quaternius (gitignored, lazy-loaded) and convert FBX/.blend -> GLB.
# Curated in-game rigs are committed separately under models/beasts + models/humanoids.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"; W="${TMPDIR:-/tmp}/qk-gather"; mkdir -p "$W/zip" "$W/x"
OGA="lowpoly-animated-monsters lowpoly-animated-farm-animal-pack animated-human-low-poly lowpoly-animated-knight
5-low-poly-animals lowpoly-rpg lowpoly-medieval-buildings lowpoly-modular-dungeon-pack lowpoly-survival-pack
animated-animales-low-poly animated-fish fantasy-low-poly-trees-and-bushes low-poly-nature-pack-1 lowpoly-crops-pack
lowpoly-medieval-village-pack lowpoly-nature-pack lowpoly-rpg-characters lowpoly-textured-trees stylized-nature-megakit
animated-characters-pack fantasy-props-megakit low-poly-rpg-pack lowpoly-buildings-pack lowpoly-house-interior-pack
medieval-village-megakit textured-cute-monster-pack furniture-low-poly"
for p in $OGA; do
  h=$(curl -sL -A Mozilla "https://opengameart.org/content/$p")
  grep -q CC0 <<<"$h" || { echo "skip $p (not CC0)"; continue; }
  u=$(grep -o 'https://opengameart.org/sites/default/files/[^"]*\.zip' <<<"$h" | head -1) || continue
  [ -f "$W/zip/$p.zip" ] || curl -sL -A Mozilla -o "$W/zip/$p.zip" "$u"
  ditto -x -k "$W/zip/$p.zip" "$W/x/$p"
done
find "$W/x" -name __MACOSX -prune -exec rm -rf {} +
find "$W/x" -name '.*' -type d | while read -r d; do mv "$d" "$(dirname "$d")/$(basename "$d" | sed 's/^\.//')"; done
blender -b --python "$ROOT/scripts/assets/conv2.py" -- "$W/x" "$ROOT/assets/models/quaternius"
blender -b --python "$ROOT/scripts/assets/blend.py" -- "$W/x" "$ROOT/assets/models/quaternius/animated"
for r in Dungeon-Remastered Medieval-Hexagon-Pack Halloween-Bits Furniture-Bits; do
  curl -sL -o "$W/zip/kk-$r.zip" "https://github.com/KayKit-Game-Assets/KayKit-$r-1.0/archive/refs/heads/main.zip"
  ditto -x -k "$W/zip/kk-$r.zip" "$W/x/kk-$r"
  mkdir -p "$ROOT/assets/models/kaykit/$r"
  find "$W/x/kk-$r" -path '*/gltf/*' \( -name '*.gltf' -o -name '*.glb' -o -name '*.bin' -o -name '*.png' \) -exec cp {} "$ROOT/assets/models/kaykit/$r/" \;
done
