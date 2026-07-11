# CHUNK C13 — Character builder + Apple login flow

**Status: todo** (Daniel directive 2026-07-11: alpha readiness)

## Goal
A real character-creation step between login and the world: the player signs
in with Apple, then (first time) walks through a builder — name, class,
faction, appearance — before spawning. Returning players go straight in.

## Design
1. Client boot flow becomes states: `SignIn → Create (first time) → InWorld`.
   Apple login already exists (apple_id on `Login`); keep it the account key.
2. `ClientMsg::Login` gains an optional `create: CharacterCreate { name,
   class, faction, appearance }` payload; server rejects world entry for an
   account with no character instead of auto-creating one.
3. Appearance: pick from the rigged character model variants already bundled
   (body/skin/hair tint indices are enough for alpha) — store as small ints
   on the sheet (SheetExt, three-place rule), send in `EntityState` so other
   clients render your look.
4. Builder UI in bevy: class cards (the 4 classes w/ ability blurbs), faction
   pick (ties into C10), name field, a rotating preview of the chosen model.
5. Class is chosen HERE from now on — remove the in-world F1–F4 classless
   pick once the builder lands.

## Verify
- Unit: login with no character + no create payload → rejected; create then
  relog → straight to world; appearance round-trips the DB.
- Live: create a character in the real client, see the chosen look on a
  second client, relog and skip the builder.
