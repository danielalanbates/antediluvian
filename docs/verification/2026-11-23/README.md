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

---

# 2026-11-23 (later) - classic windows in the Bevy client

Code: `crates/client-bevy/src/win/` (one module per window), hooks in
`main.rs`, `menu.rs`, `ui.rs`, `local.rs`. Commits `a4e890c`, `a9779bb` and
the docs commit after them. Nothing pushed, deployed or installed.

## Keys
B bags, C character, L quest log, P spellbook, M world map, O social,
F notice board / flight master where you stand, Esc closes the top window
(only with none open does it reach the game menu). **Mount moved from M to
H** because M is the map. Vendor opens when the Innkeeper or Quartermaster
answers E. Loot pops on a drop. Windows never read movement keys.

## Scripted opening (windowless)
`ANTEDILUVIA_AUTOCMD="ui bags;ui only map;ui social:who;ui chat:trade;ui menu;ui close"`.
Also `ui talk` (E), `ui interact` (F), `ui accept` (first Accept on the open
board).

## Tests
| Suite | Result |
|---|---|
| `antediluvia-client-bevy` | 98 passed (40 before + 58 new: layout, map projection, minimap, rarity colours, quest lines, markers, vendor, chat tabs, buffs, rested span, banner fade, dev panel, autocmd) |
| `antediluvia-sim` (`--test-threads=1`) | 127 passed |
| `antediluvia-server` | 14 passed |
| `cargo build --release -p antediluvia-client-bevy -p antediluvia-test-client` | builds |

## Windowless runs (4 of 4 used; own server on 127.0.0.1:8797 for A, C, D; B is solo)
| Shot | Run | What it shows |
|---|---|---|
| `win_bags_character_minimap.jpg` | A, networked | Bags with rarity borders and wear %, character sheet (slots, repair state, stats), minimap, quest tracker with real objective, "!" over givers, chat tabs, **stamina bar on a Warrior** (was unverified). Taken before the unit-frame width fix. |
| `win_vendor_bags.jpg` | A | Vendor opened by talking to the Innkeeper: buy list with gold / lineage locks, sell list, repair line; bags switch to "right-click to sell" |
| `win_social_who.jpg` | A | Social window, Who tab, filled from the server's `WhoList` |
| `win_flight.jpg` | A | Flight master window at the inn (no routes known yet). The empty list box seen here was removed afterwards. |
| `win_board.jpg` | A | Notice board: one quest accepted through the window ("In progress 0/6"), three on offer, tracker updated |
| `win_zone_banner_solo.jpg` | B, solo | Zone-in banner "Inn of Eden" in the solo driver; full-width HP / stamina bars after the fix |
| `win_questlog_spellbook_solo.jpg` | B | Quest log and spellbook side by side (all 8 abilities, unlock levels), XP bar with the pale-blue rested stretch, "Rested" icon |
| `win_death_prompt_solo.jpg` | B | Spirit-release prompt after dying to dire wolves in solo play |
| `win_map_pin.jpg` | C, networked | World map: terrain picture, inn, capitals, graveyards, flight masters, player arrow, quest "!" pin lifted above the spot |
| `win_buff_icon.jpg` | C | "Might 11s" buff icon after War Cry; "!" over the board |
| `win_menu_dev_panel_chat_tab.jpg` | C | Escape menu with PvP toggle and the developer panel; chat on the General tab showing only the `[general]` line that went through the server |
| `win_loot_empty_bags.jpg` | D | Loot window in its empty state; chat log now keeps its newest line |

## Found and fixed while looking at the shots
- Unit-frame bars had no definite width: HP / mana shrank to their text and
  the XP bar was zero wide (it is missing from every earlier shot). Fixed in
  `ui.rs`; the rested stretch needs it.
- The Escape menu used an em dash and a middle dot the font does not have
  (drawn as boxes) and its help text overflowed the panel.
- The chat log clipped its newest lines when older lines wrapped.
- Quest log and spellbook opened on top of each other; panels were
  translucent enough for the lower one to show through.

## Not verified
- **No mouse or key was used** (not allowed): clicks, right-click use /
  equip / sell, hover tooltips, the hotkeys themselves and Escape-closes-top
  are covered by unit tests of the logic only. The button paths that ran for
  real are the scripted ones: talk, interact, accept, open / close.
- **Loot window with real drops**: dev `kill` gives no loot, so only the
  empty state was seen. The notice parser is unit-tested.
- **Death prompt against the server**: the character had 18 health left when
  run D ended. Seen in solo only. The Release button was never pressed.
- Buy, sell, repair, fly, turn in, abandon, friend / raid / channel buttons:
  never pressed. "?" (ready) markers and map pins: unit-tested, not seen.
- Other acts' maps, the Ignore / Raid / Channels / Friends tabs with data,
  flight list with known routes, Quartermaster wares unlocked.
- The marker z-order change (markers under the HUD) was made after the last
  run and has not been seen.
- Windowed play and the wasm build were not run.

## Known limits (by design or still open)
- Buff row shows own timed self-buffs (from `AbilityUsed`) and sheet
  conditions (rested, PvP, broken gear, weary). Debuffs enemies apply are not
  shown: the protocol has no aura list.
- Loot is a record of drops, not a pick list: the server auto-loots.
- Names for friend / ignore / raid invite are typed in chat; the windows have
  no text fields.
- Windows have fixed places and cannot be dragged.
- Solo play: unit frame health lags at the moment of death (sheet is pushed
  on respawn); raids, channels, whispers, friends answer "not available".
- Chat tabs classify lines by their text; a notice shaped like "Word: text"
  lands on Say.
- Action-bar names longer than a tile are still clipped ("Challenging Roar").
- Name labels still draw over the unit frame (older code).

## Variety chunk (later still): character looks, mob models, terrain and prop models

Three targets, each counted by a unit test over DISTINCT results, not seeds.

| Target | Test | Measured |
|---|---|---|
| Hundreds of creation options | `look::tests::every_option_of_every_field_renders_differently`, `look::tests::thousands_of_random_looks_are_all_distinct`, protocol `look::tests::hundreds_of_options` | 207 look options (12 fields + 4 outfits), each drawing differently from every other value of its field; 3000 of 3000 random looks distinct; 4.8e14 combinations |
| 300+ mob models from `mobs.json` | `mobmodels::tests::mobs_json_reaches_over_300_distinct_models` | 1768 species: 1473 distinct silhouette keys (708 of them generated bodies), 1768 distinct geometries |
| 2,000+ terrain and prop meshes over 5 acts | `formations::tests::five_acts_hold_over_2000_distinct_terrain_and_prop_meshes` | 6956 spawned sites, 6956 distinct geometries, 1325 distinct (family, silhouette) keys, all 40 families present |

What the keys mean. Geometry hash = every vertex, quantised to 1/1000 unit.
Silhouette key = coarse: bounding-box proportions in 6 bins per axis plus
vertex count (terrain), or body plan + graft loadout + proportions (mobs).
The geometry counts say "no two are the same mesh"; the silhouette counts are
the stricter "differ at a glance" number. For terrain that stricter number is
1325, not 2,000: the 2,000 claim holds for distinct meshes, and about
two thirds of that for clearly different shapes.

### Character looks
- `antediluvia_protocol::look::Look`: face 24, hair 32, hair colour 24, facial
  hair 14, skin 20, build 8, height 9, marking 20 (paint, tattoo-style bands,
  5 scars), marking colour 8, eyes 14, mantle 6, dye 24; outfit 4 stays in
  `appearance[0]`. Packed into one `u64` (51 bits).
- Three places: `CharacterSheet.look`, `SheetExt.look`, load + save
  (`db::look_tests::look_survives_save_and_load`). Broadcast as
  `EntityState.look`. Clamped by the server with `Look::sanitize`; the solo
  driver does the same. `PROTOCOL_VERSION` stays 16: the field is optional
  both ways.
- Rendering (`look/apply.rs`): the rigs keep skin, hair, eyes and clothes in
  one texture, so each body-part mesh is copied per character and given vertex
  colours that multiply the texture. The head copy is reshaped for the face.
  Hair, beard and mantle are grafted meshes (one per bone, one shared
  material). Build and height scale the rig. Distant LOD figures take skin
  tone, build and height (20 shared materials, one shared mesh).
- Builder (`look/builder.rs`): 15 lines with arrows and colour swatches, typed
  name with live validation, Randomise, live preview with idle animation.
- Names (`antediluvia_sim::names::validate_name`): 3 to 16 letters, one
  apostrophe or hyphen inside, no triple letters, needs a vowel, reserved
  names refused, stored capitalised. Applies to the builder path only; the
  legacy name path used by bots and tooling keeps its old 1 to 24 rule.
- Characters made without the builder get a stable random look from their
  name, so bot crowds are not clones. Old characters keep their skin and hair
  picks (`Look::from_legacy`).
- Apple login flow unchanged; owner steps in `docs/beta/APPLE_SIGN_IN.md`.

### Shots (all read)
| File | Shows |
|---|---|
| `var_looks_sheet.jpg` | 16 random looks, off-screen sheet: skin tones, hair pieces and colours, beards, paint, mantles, dyes, heights |
| `var_builder_plate.jpg`, `var_builder_cape.jpg`, `var_builder_furs.jpg` | the builder on my own server: panel, swatches, highlighted line, name status, three different rolled looks |
| `var_world_other_player.jpg` | after Create: own character in the world wearing the rolled look, a bot beside it with its own look |
| `var_world_lod_figures.jpg` | distant players as LOD figures in different skin tones and sizes |
| `var_species_sheet.jpg` | 84 real `mobs.json` species with generated bodies and grafts |
| `var_beast_plans_sheet.jpg` | the ten body plans (existing sheet, now with grafts) |
| `var_landform_sheet.jpg` | the twelve new landform families, eight seeds each |
| `var_prop_sheet.jpg` | the twenty prop families (existing sheet, now renderable off-screen) |

New off-screen sheet switches: `ANTEDILUVIA_PROPSHEET_OUT`,
`ANTEDILUVIA_LANDSHEET_OUT`, `ANTEDILUVIA_SPECIESSHEET_OUT` (with
`BEVY_ASSET_ROOT=<repo>`), beside the existing `_CHARSHEET_OUT` and
`_BEASTSHEET_OUT`. The windowed `ANTEDILUVIA_PROPSHEET=1` still opens a window.

Launches: 7 off-screen sheet renders and 2 windowless game runs (my own server
on 127.0.0.1:8797, temp DB removed, 10 and 30 swarm bots, all three processes
stopped, port free). Port 8787 untouched.

### Seen in the shots, not fixed
- Eyes are solid beads on these rigs, so an eye colour is a coloured bead.
- Face paint follows the head's few vertices: soft blotches, not crisp lines.
  Thin scars may land on no vertex at all and not show; only the wide ones
  were seen.
- "Furs and hide" has a beard and a bald head baked into its head mesh:
  "Facial hair: None" still shows that beard, in the chosen hair colour.
- A dye recolours the whole outfit, metal and leather included.
- Hair styles are pieces added on top of each outfit's own hair, not
  replacements.
- The fallen colossus reads as a line of boulders.
- The prop and landform sheets are small in frame and washed out (no
  tonemapping off-screen).

### Not verified
- No mouse or keyboard was used: arrow buttons, line clicks, typing, F5 and
  Enter are covered by unit tests of the same functions the scripted
  `look ...` commands call. Create itself ran for real, twice.
- A refused name coming back from the server (taken, reserved) was not seen.
- After the last shots, large-area colour multipliers were capped at 12 to
  stop bright blow-outs when the texture is sampled from a blurred mip at
  range. Not re-rendered: very dark painted hair (the Robes outfit) may now
  come out darker than a pale hair colour asks for; grafted hair pieces are
  unaffected.
- Hide vest and lineage mantle over a dyed outfit (the flat-material path in
  `equipment.rs`) was not seen.
- Frame rate was not measured. By construction the scatter has the same
  number of sites, meshes and draws as before (landforms replace 45% of the
  dome rocks and share the prop material); each nearby player costs six small
  mesh copies and up to two grafts.
- Landforms have no collision (the dome rocks they replace had none either).
- Solo play, the wasm build and windowed play were not run.
- No profanity filter on names.
