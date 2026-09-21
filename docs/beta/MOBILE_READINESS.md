# Mobile Readiness — audit & workstream notes

Date: 2026-09-21. Branch: `feature/mobile-touch`. Goal: playable on iOS Safari
and Android Chrome with zero regression to desktop mouse/keyboard.

## Gap list (audit, pre-implementation)

1. **No touch input at all.** `crates/client-bevy/src/local.rs:226` is only a
   stub (`fn _touch(_: PVec2, _: EventKind) {}`); nothing in the client reads
   `bevy::input::touch::TouchInput`. On a phone the game renders but cannot be
   controlled.
2. **Viewport / gesture CSS missing.** `web/index.html` has the correct
   `<meta name="viewport" content="width=device-width, initial-scale=1,
   viewport-fit=cover">`, but the canvas has no `touch-action: none`, so the
   browser fights the game for scroll/zoom gestures, and there is no
   `overscroll-behavior: none` to stop pull-to-refresh.
3. **HUD is fixed-pixel desktop-sized** (`crates/client-bevy/src/ui.rs`):
   chat log `min-width: 220px`, action bar 56 px slots centered with a
   `-120px` margin hack, 11–15 px fonts. On a 390 px-wide phone the action
   bar overflows and the chat log eats half the screen. Menu (`menu.rs`)
   buttons are desktop-sized too.
4. **Chat is keyboard-only.** `chat_input` (main.rs ~2675) opens on Enter and
   edits via `KeyboardInput`; mobile has no physical keyboard and Bevy 0.15
   provides no API to summon the OS keyboard. Needs a DOM `<input>` overlay
   bridged into the wasm.
5. **Character builder is keyboard-only** (main.rs ~2340 `builder_screen`):
   F1–F4 class, F5 lineage, F6 hair, arrows for body/skin, typing for the
   name, Enter to create. Completely unusable on touch.
6. **iOS Safari quirks, unhandled:**
   - `AudioContext` starts suspended until a user gesture; Bevy/rodio never
     resumes it → silent game on iOS.
   - WebGL2 requires iOS 15+ (older devices fail the boot check).
   - No Fullscreen API on iPhone Safari; `viewport-fit=cover` + safe-area
     insets must be honored by any DOM overlay.
   - Programmatic `input.focus()` outside a real user gesture does NOT summon
     the iOS keyboard — any chat overlay needs a visibly tappable field as a
     guaranteed fallback.
   - iOS zooms the page when focusing an `<input>` with `font-size < 16px`.
7. **Camera look is mouse-only** (`orbit_camera`, main.rs ~2839 reads
   `MouseMotion` gated on mouse buttons held). No touch-drag equivalent.
8. **Attack is click-only** (`mouse_click_attack`, main.rs ~3003); abilities
   are Digit1/Digit2; Interact=E, Mount=M, Tame=T. None reachable by touch.
9. **Escape menu has no touch path** (Esc key only). Out of scope for this
   pass; noted for follow-up.
10. **Boot hint is desktop-only** ("WASD to move, mouse to look, Enter to
    chat") — should adapt on coarse-pointer devices.

## What `feature/mobile-touch` implements

- `crates/client-bevy/src/touch.rs` (new): touch detection, virtual joystick
  (left half), camera drag (right half), touch-sized Bevy-UI buttons with
  custom hit-testing, DOM chat/name overlay bridge, touch character-builder
  controls. All wasm-unsafe DOM work is `#[cfg(target_arch = "wasm32")]`.
- Touch UI appears only after the first real `TouchInput` (or
  `pointer:coarse` at startup on web); desktop mouse/keyboard paths are
  untouched.
- Game-screen touch buttons hide whenever the builder, chat, dev console, or
  menu owns the screen/input; the builder sheet shows only while
  `session.builder` is active.
- `web/index.html`: `touch-action: none` on canvas, `overscroll-behavior:
  none`, AudioContext constructor wrapper that resumes suspended contexts
  on every touch/click (persistent — iOS can re-suspend on interruptions),
  hidden chat/name `<input>` overlay wired to wasm exports, adaptive boot
  hint.

## Control mapping (touch → existing system)

| Touch control        | Existing path it feeds                              |
|----------------------|-----------------------------------------------------|
| Left-half joystick   | `send_input` (main.rs ~2239): quantized to the same `(f,s)` i8 grid as WASD, combined with keyboard, same `ClientMsg::Move` change-detection |
| Right-half drag      | `orbit_camera` (main.rs ~2839): same `Orbit` resource, same yaw/pitch factors and pitch clamp |
| Attack button        | `ClientMsg::Attack` + `cooldowns.trigger(0, …)` — identical to `mouse_click_attack` |
| Jump button          | `jump.start.get_or_insert(now)` — identical to Space in `send_input` |
| Interact button      | `ClientMsg::Talk` — identical to KeyE |
| Ability 1 / 2        | `ClientMsg::Cast { ability }` + `cooldowns.trigger(1/2, …)` — identical to Digit1/Digit2 |
| Mount button         | `ClientMsg::Mount` — identical to KeyM |
| Chat button          | toggles `session.chat_active`; on touch, opens the DOM input overlay; submit drains through `dispatch_chat_line` → `ClientMsg::Chat` |
| Builder class/lineage/body/skin/hair buttons | mutate `session.builder` exactly like F1–F6/arrows in `builder_screen` |
| Builder name field   | DOM overlay in name mode → same alphanumeric ≤24 filter as keyboard entry |
| Builder Create       | extracted `builder_try_submit` — the same `ClientMsg::Login { create }` the Enter key sent |

## Verification (2026-09-21, worktree `~/workspace/antediluvian-mobile`)

- `cargo test -p antediluvia-client-bevy` — **46 passed, 0 failed**,
  including 7 new `touch::tests` (stick rest/deadzone/clamp/up-is-forward,
  hit-rect lookup, no button overlaps, builder sheet fits 390×844).
- `cargo check --target wasm32-unknown-unknown -p antediluvia-client-bevy`
  — **0 errors**; 18 warnings, all pre-existing or in vendored bevy code
  (none in `touch.rs` or the new touch wiring).
- No other workspace crate depends on `antediluvia-client-bevy`, so no
  further crate test runs were needed.
- `touch::touch_input` is explicitly ordered
  `.before(send_input).before(orbit_camera)` so joystick/camera consumers
  never read a stale frame and the TouchMove read/write ambiguity is
  resolved.
- Formatting: `touch.rs` is rustfmt-clean. `main.rs`/`ui.rs` were left
  unformatted on purpose — formatting them rewrote hundreds of unrelated
  lines in the pre-existing visual-pass churn on this branch.
- Real-device testing (iOS Safari / Android Chrome) has NOT been done and
  remains the gating item before calling this playable.

## Known limitations / follow-ups

- Real-device verification (iOS Safari / Android Chrome) still needed:
  keyboard summoning behavior, joystick feel, safe-area layout on notched
  phones. The chat overlay degrades gracefully: if iOS refuses the
  programmatic `focus()`, the input field is visible and tappable, which is a
  genuine user gesture and always summons the keyboard.
- Escape menu has no touch button yet.
- Tame (T key) has no touch button.
- HUD panels (chat log, unit frames, quest tracker) are still fixed-pixel;
  only the action bar is hidden in touch mode (replaced by touch buttons).
- No haptics; joystick is position-based with a 60 px radius and 0.3
  deadzone, tuned blind — needs device testing.
