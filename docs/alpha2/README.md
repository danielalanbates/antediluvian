# Alpha-2 queue (set by Daniel 2026-07-22)

Goal restated: world-class alpha "millions would want to play." New standing asks
beyond the completed C01–C16 / art 01–10 queues:

- [x] **A0 Launcher + update channel** — DONE 2026-07-22. `/Applications/Antediluvia
  Launcher.app` (scripts/launcher/), manifest at batesai.org/antediluvia/manifest.json,
  payloads on GitHub Releases (danielalanbates/antediluvian). Landing page live at
  batesai.org/antediluvia. **Release procedure:** bump CFBundleShortVersionString in
  scripts/app/Info.plist → make_app.sh → ditto-zip /Applications/Antediluvia.app →
  shasum -a 256 → `gh release create vX.Y.Z <zip>` → update manifest.json version/url/
  sha256/notes in website dist+public → `npx wrangler pages deploy dist --project-name=website`.
  NOTE: website GitHub remote (danielalanbates/website) has DIVERGED from the live
  local repo (/Users/daniel/.git @ 0b51481); deploys must start from the local repo.
  Working copy used: ~/Library/Application Support/BatesAI/website-build2.
- [~] **A1 IMPLEMENTED 2026-07-22** (8 formation families incl. mesa/hoodoo/arch/
  shards/terrace; 1,200 sites/act = 6,000 unique meshes world-wide; family-
  reachability + 2,000-seed distinctness test). Perf re-check on retina pending.
  Original spec: **Terrain variety — thousands of models.** Extend formation_mesh
  (currently 500 procedural meshes/act) to ≥2,000 distinct terrain/landform meshes
  across acts: new formation families (mesas, arches, sinkholes, terraces, glacial
  erratics, reef rock), per-act material palettes, seeded erosion passes. Keep
  VisibilityRange + Msaa::Off perf budget (59 fps retina, RSS ≤ 300 MB).
- [~] **A2 IMPLEMENTED 2026-07-22** (8 body-plan families w/ family stretch bands
  — serpentine/stilt/brute/squat/avian/stalker/giant; new parts: tusks, dorsal
  sail, tail spikes on tail-boned rigs; silhouette_key test: ≥300 distinct
  silhouettes over 400-tag namespace, all plans reachable). Visual spot-check
  pending. Original spec: **Mob model variety — hundreds of base models.** species_stretch gives
  1,768 unique tags from few base rigs; add genuinely distinct base body plans
  (quadruped heavy/light, serpent, avian, insectoid, amphibian, giant) so
  silhouette variety reads at a glance. Target ≥200 visually distinct species
  silhouettes; unit tests assert silhouette-hash uniqueness.
- [~] **A3 IMPLEMENTED 2026-07-22** (16 skins x 12 hair colors x 4 bodies = 768
  rendered combos; hair index also grafts 1 of 12 procedural hairstyle
  GEOMETRIES onto the head bone — attach_hair_style in variety.rs; server
  clamp raised to 15/11; builder cycles use SKIN_CHOICES/HAIR_CHOICES).
  In-game visual check pending. Original spec: **Character creation —
  hundreds of choices.** Skin/hair ints exist in
  protocol but aren't rendered on player rigs. Render them (TintRig path), then
  expand: ≥8 faces, ≥20 hairstyles, ≥16 skin tones, ≥12 hair colors, body
  sliders, per-class starter garb variants (multiplies to hundreds of combos).
  Broadcast appearance already exists — verify remote players render choices.
- [ ] **A4 Skeptical alpha pass.** Play-audit vs "would millions play this":
  first-15-minutes flow, quest breadcrumbs, combat feel (GCD, feedback,
  floating damage), sound mix by ear, dev-menu polish, crash/logout resume.
  File concrete fix-list, then fix.

Rules unchanged: one cargo build/swarm at a time (8 GB), tests must pass,
gate new quests behind act finales, three-place rule for sheet fields.
