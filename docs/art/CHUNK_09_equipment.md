# CHUNK 09 — Visible equipment

**Status: DONE 2026-07-10** — implemented as visibility toggling: the KayKit
rigs already ship every held-item mesh on their hand slots, so `equipment.rs`
mirrors `EntityState.weapon/chest` (proto v6) onto a `Loadout` component and
shows only the equipped weapon's node (bare hands when none — classless
characters no longer show the baked-in sword). Chest v1 = torso material
override (flat hide tint; subtle at night). Verified with two live clients +
SQL-equipped sword; NPC/enemy rigs untouched.

## Goal
What a character wears shows on the model, for *everyone*: the equipped
weapon appears in the right hand (replacing the baked-in model weapon look),
and chest armor tints the rig. Classic MMO dress-up feedback.

## Design
1. **Wire**: add `weapon: Option<String>` and `chest: Option<String>` to
   `EntityState` (skip-if-none like `tag`); server fills them from
   `sheet.equipment` for player entities. Bump `PROTOCOL_VERSION`.
2. **Weapon meshes**: KayKit Adventurers GLBs ship separate weapon models —
   check `assets/models/` for weapon GLBs or grab from the KayKit pack
   (sword/axe/staff). Map item ids: bronze_sword→sword, stone_axe→axe,
   oak_staff→staff; unknown/None→nothing.
3. **Socketing**: find the hand bone by walking the spawned scene's named
   nodes (`Name` component contains "hand" / "Hand_R" — print the node tree
   once to learn the names) and parent the weapon mesh to it with a baked
   offset/rotation. Re-socket when the entity's `weapon` field changes.
4. **Chest tint**: hide_vest → darken torso material or overlay a simple
   vest mesh scaled to the torso bone; a material tint is acceptable v1.
5. Keep it working with animations (the socket inherits bone transforms —
   test during a swing).

## Verify
Two windowed clients: player equips bronze_sword via test-client/`Equip`,
BOTH windows show the sword in hand; screenshot during a swing (weapon
follows the hand). Unequipped/classless characters show no weapon.
