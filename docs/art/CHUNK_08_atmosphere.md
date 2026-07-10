# CHUNK 08 — Sky, lighting, atmosphere

**Status: todo** (best after CHUNK_03 terrain)

## Goal
Each act has a mood; the world has a sky and time-of-day instead of a flat
blue clear color.

## Steps
1. **Sky**: `bevy_atmosphere` (MIT) if it supports Bevy 0.15 (check first);
   fallback: a big inverted `Sphere` mesh with an unlit vertical-gradient
   `StandardMaterial` (custom shader not required — bake gradient into a tiny
   generated image) parented to the camera. Fallback is ~40 lines and always
   works; same rule as CHUNK_06 — don't burn the session on dependency fights.
2. **Day/night**: drive sun `DirectionalLight` rotation + illuminance +
   `AmbientLight` from a 10-min wall-clock cycle. Server should own game
   time so all players share it: add `time_of_day: f32` to `Snapshot`
   (cheap, one field; bump proto). Night stays readable (min ambient ~80).
3. **Fog**: `DistanceFog` component on the camera (built-in), per-act color:
   Eden soft green-gold, Hermon cool blue, Nephilim dusty red, Enoch smoggy
   gray, Flood storm-dark with rain (rain = CHUNK_06-style particle streaks
   parented to camera, only in Flood).
4. **Per-act light table**: sun color/intensity + fog + clear color chosen in
   one `fn act_mood(act) -> Mood` consumed on zone load/travel.

## Verify
Screenshots at two times of day (temporarily speed the cycle ×20) and in ≥3
acts — visibly distinct moods, shadows track the sun, night readable, HUD
legible against every sky. Two clients see the same time of day.
