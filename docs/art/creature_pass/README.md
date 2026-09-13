# Creature pass — generated bodies in the world (v0.5.6, 2026-09-12)

## Problem found
- The bestiary has 2,500 species built from **21 animal nouns** (bear, eagle,
  crawler, mammoth, pterosaur, chimera, leviathan, serpent, megatherium,
  crocodile, behemoth, smilodon, tortoise, wolf, boar, mastodon,
  elasmotherium, lion, salamander, aurochs, + "rider" mounts).
- **Hostile** species (~1,480) all rendered as Skeleton Minion/Rogue/Mage:
  `rig_for` only mapped species keywords for `Wildlife`. A "Rabid Cave Bear
  Scavenger" was a skeleton.
- Wildlife species with no matching Quaternius animal (eagle, pterosaur,
  serpent, crawler, ...) all rendered as the **Deer**.
- `creaturegen.rs` (10 body plans, 400+ distinct seeded bodies) existed but
  only fed a dev contact sheet.

## What changed
| Area | Change |
|---|---|
| `main.rs` `beast_rig` | One keyword→animal-rig table shared by wildlife AND hostile bestiary beasts |
| `main.rs` `proc_body_plan` | Bestiary species whose name has a generated plan (or no rig) spawn a generated body |
| `creaturegen::named_plan` | Species noun → plan; wins over rig keywords ("Pterosaur Behemoth-Rider" is a pterosaur, not a bull). "crawler" splits 50/50 insectoid/arachnid |
| `creaturegen::build_proc_bodies` | Mesh cached per species (one GPU buffer per species) |
| `creaturegen::animate_proc_gait` | No skeleton: stride bob + roll scaled by root speed; topple on death (`ProcDied` inserted from combat events) |
| `is_bestiary` | Uses the full species list — `mobs::mob_by_tag` hides `_alpha` species (boss stats), which left "Dire Wolf Alpha" a skeleton |
| creaturegen geometry | Neck geometry added (heads floated on long-necked seeds); brute head seated on shoulders; wing struts laid along the wing instead of standing straight up; left wing sweep mirrored correctly |
| Dev tool | `ANTEDILUVIA_BEASTSHEET_OUT=<png>` renders the sheet **off-screen** (no window, no focus steal) — combine with `_PLAN`, `_COLS`, `_CAMH`, `_CAMD` |
| Test fix | `dressing::keyword_matching_covers_real_names` compared promoted-constant addresses (not guaranteed equal); now compares contents |

## Species → body
| Noun | Result |
|---|---|
| bear, mammoth, mastodon, behemoth, aurochs, boar | Bull rig |
| wolf / smilodon, lion | Wolf / Fox rig |
| serpent, leviathan | serpentine |
| eagle | avian |
| pterosaur | chiropteran (membrane wings) |
| crawler | insectoid or arachnid |
| crocodile, salamander | amphibian |
| tortoise | crustacean (shell plan) |
| megatherium | biped brute |
| chimera, elasmotherium | heavy quadruped |

## Verification (honest)
- 30/30 client tests pass, incl. `no_bestiary_species_renders_as_a_skeleton`
  (all 2,500 tags) and routing tests.
- Off-screen renders: `before_plans.png` (floating heads, vertical wing
  spikes) vs `after_plans.png` (fixed). Plans ordered 0,1 / 2,3 / ... in
  before; 0,3 / 5,7 / 9,1 in after.
- `/Applications/Antediluvia.app` 0.5.6 built, signed, bundled server listens
  on 8787, bundled client renders off-screen. 0.5.5 archived at
  `~/Downloads/Antediluvia-archived/Antediluvia-0.5.5.app`.
- **NOT verified in-world by screenshot**: the in-game window needs the
  screen (Daniel's rule: ask before GUI use). Unverified: `PROC_BODY_SCALE`
  (34) vs neighbouring rigs, facing direction (-FRAC_PI_2), gait feel,
  vertex-colour look under in-game grading.

## Next pathways
1. In-world screenshot pass (with permission): `ANTEDILUVIA_LOCAL=1
   ANTEDILUVIA_AUTOCMD="god;tp <x> <y>"` near a crawler/pterosaur habitat;
   tune scale + facing.
2. Off-screen render of the real game scene (camera → image target, like the
   sheet) so world verification never needs the screen.
3. Skinned generated bodies: `Part.attach` already records bone targets —
   parent parts to rig bones to borrow real walk cycles.
4. Art ceiling remains: these are low-poly faceted primitives. Real fidelity
   = authored/PBR creature models (Blender pipeline exists for icons).
