# CHUNK 07 — WoW-style UI

**Status: DONE 2026-07-10**

## Goal
Replace the two debug text lines with a real HUD: player unit frame
(portrait/HP/MP/XP bars), target frame, icon action bar with cooldown
sweeps, quest tracker, chat pane.

## Tools
Bevy 0.15 built-in UI (`Node`, `BackgroundColor`, `ImageNode`, `Text`) — no
extra crates needed. Icons: Kenney CC0 "Game Icons" / game-icons.net (CC-BY —
if used, credit in LICENSES.md; prefer Kenney to keep everything CC0). Put
them in `assets/ui/`.

## Layout (WoW Classic reference)
- Top-left: player frame — name, level, HP bar (green), MP bar (blue),
  thin XP bar (purple). Data source: the `CharacterSheet` from
  `ServerMsg::Stats`/`Welcome` (already stored in `Session` — extend Session
  to keep the whole sheet, not a preformatted string).
- Top-left below it: target frame. Requires target selection: click-to-
  target needs picking; cheaper v1 = "nearest enemy in front within 200u"
  auto-target shown when attacking. Mark with a subtle ring under the target.
- Bottom-center: action bar — slots for Attack/ability1/ability2 with key
  labels, grayed while `gcd`/cooldown active. Server doesn't send cooldowns
  today: approximate client-side (start a local cd when the key is pressed;
  lengths from a table) OR add cooldown info to `Stats`. Client-side approx
  is acceptable for this chunk.
- Right side: quest tracker — active quests from the sheet (`quests` map +
  quest names; the Elder's quest ids are `<act>_cull`, see world.rs) with
  n/target progress.
- Bottom-left: chat log — keep last ~8 `Chat`/`Notice` lines in a scrolling
  Text column instead of one overwriting line.

## Steps
1. Refactor `Session` to hold `sheet: Option<CharacterSheet>` + a
   `VecDeque<String>` chat log; delete the preformatted `hud` string.
2. Build the UI tree in `setup` (a `ui.rs` module; tag nodes with marker
   components), one `update_ui` system reads Session and writes
   widths/texts. Bars = nested `Node` with % width.
3. Keyboard focus: add Enter-to-chat (text input via `ReceivedCharacter`
   events → send `ClientMsg::Chat`) — WoW muscle memory.

## Verify
Screenshots: frames render with correct live values (take damage → HP bar
drops; kill → XP bar moves; accept quest → tracker shows 0/5 then ticks).
Chat: send a line from client B, see it in A's log. No text overlap at
1600×900 and after resizing the window smaller.
