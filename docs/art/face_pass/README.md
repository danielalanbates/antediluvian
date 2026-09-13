# Face + mob colour pass (v0.5.7, 2026-09-12) — Daniel playtest feedback

| Feedback | Cause | Fix |
|---|---|---|
| "Larger rounded nose like the rogues" | KayKit faces are geometry; noses were small | `face.rs` grows nose vertices ×1.55 from the nose base (all bodies except the Barbarian, whose nose is already big and overlaps his moustache) |
| "No mouths" | Mouth = V-fold of duplicated vertices under the nose (down/up-facing normals → dark line) | Mouth-box vertices pulled onto a fitted cheek profile with forward normals. Barbarian: normals only (beard volume) — a faint crease can remain |
| "Giant bulls totally grey" | Quaternius Bull materials are pure grey; species tint was a hue shift, which is a no-op at zero saturation | Grey mob body colours → earth tone (hue 32°, sat 0.30) with a ±18° species nudge; all mob materials limited to the nudge (a full-wheel shift gave pink/green bears — see `bulls_fullwheel_rejected.png`) |
| "Skeletons weirdly stretched, skinny" | Per-species body-plan stretch (e.g. stilt plan 0.7×1.3) applied to skeleton rigs | Stretch skipped for Skeleton rigs (grafted parts + uniform scale kept) |

Verification: 32/32 client tests (incl. `face::tests`); off-screen renders in
this folder via `ANTEDILUVIA_CHARSHEET_OUT=<png>` (+ `_RAW`, `_MOBS`,
`_SPACING`, `_CAMH`, `_CAMD`, `_LOOKY`; run with `BEVY_ASSET_ROOT=<repo>`).
Not yet checked in-world at gameplay camera distance.

Tooling: `NO_INSTALL=1 scripts/make_app.sh` builds `dist/Antediluvia.app`
without touching a running install; a normal run now archives the old app to
`~/Downloads/Antediluvia-archived/` instead of deleting it.
