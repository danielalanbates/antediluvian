# 2026-11-23 - integration of beta-b01-b09, origin/main and beta-server-track

Branch `beta-b01-b09` now contains: old main + Quaternius rigs + class kits
(B03/B09) + `origin/main` @ e192420 (AoI grid, escape menu, life pipeline,
web path) + `beta-server-track` (B05-B08). `PROTOCOL_VERSION` is 16.

## Merge resolutions
- `sim/lib.rs`: both `abilities` and `aoi` modules.
- `sim/world.rs` damage loop: durability wear hook first, then the
  Expose/Guard/Shield aura block. `party_shares_xp_and_quest_credit` takes
  main's relative-XP version.
- `server/db.rs`: `stamina`, `max_stamina` and `durability` in SheetExt, load
  and save.
- `pvp_tests::pvp_is_opt_in...`: the last assert compared health with `==`;
  the class-kit branch made health regenerate, so it is now `>=`.

## Client work
- `ui.rs` had an unbalanced brace (the only compile error in the never-built
  class-kit client code).
- `castbar.rs` (new): cast bar above the action bar.
- `track_ui.rs` (new): server-track messages print to chat / banner; slash
  commands `/board /accept /turnin /abandon /raid /rinvite /raccept /rleave
  /rkick /ra /join /leave /channels /general /trade /lfg /zone /w /friend
  /unfriend /ignore /unignore /friends /e /who /flights /fly /release
  /repair /gear`.
- Bar keys 1-0 are ignored while the escape menu, builder or dev console is up.
- Action-bar tiles are 50x60 so three-line ability names fit.
- **Startup crash found and fixed:** the first windowless run aborted 2.5 s in
  with `IO Task Pool ... has overflowed its stack`. The macOS crash report
  shows five nested `bevy_gltf::load_gltf` calls on one worker thread (the
  loader ticks the pool while it waits for textures). `main()` now sets
  `RUST_MIN_STACK` to 16 MB before any thread starts. Three runs after the
  fix did not crash. Whether old main had the same crash was not tested.

## Tests (release, this machine)
| Suite | Result |
|---|---|
| `antediluvia-sim` (`--test-threads=1`) | 127 passed |
| `antediluvia-server` | 14 passed |
| `antediluvia-client-bevy` | 40 passed |
| `cargo build --release -p antediluvia-client-bevy -p antediluvia-test-client` | builds |

## Windowless runs (own server on 127.0.0.1:8797, temp DB, stopped after)
| Shot | What it shows |
|---|---|
| `builder_8_classes.jpg` | Builder: "Warrior (1/8)", pitch line, all 8 ability names, 11-tile bar |
| `occultist_cast_bar_44s.jpg` | Level-14 Occultist: cast bar "Fires of Hermon 1.1s", slots 4-7 counting down, mana bar |
| `occultist_cooldowns_62s.jpg` | Cast bar "Umbral Bolt 0.7s", mana 170/199 after spending, serpent trophies looted |
| `occultist_relogin_taller_tiles.jpg` | Same character after re-login (persisted); three-line names fit; server refuses off-class casts |

## Not verified
- **Stamina bar and the icon tiles of the original four classes in play.** The
  fourth run was meant to be a Warrior but reused the dev account, which
  already held the Occultist, so it logged that character back in.
- Tab cycling in the builder (no keystrokes may be sent); only class 1/8 was seen.
- The cooldown sweep is a dark overlay on a dark tile: the countdown text is
  clear, the sweep itself is hard to see in the shots.
- The `track_ui.rs` slash commands and message printing compile but were not
  exercised against a server. `antediluvia-track` was not re-run after the merge.
- Windowed play, the solo (`local`) driver and the wasm build were not run.
- `rust-objcopy` aborts when stripping (missing libLLVM.dylib in the
  toolchain), so release binaries are left unstripped; harmless.

## Seen in the shots, not fixed
- The builder preview character stands behind the translucent text panel.
- The builder's ability-icon strip shows one icon only (art exists for 8 of 64 abilities).
- The scene is dark at the login hour; characters read as near-black.
