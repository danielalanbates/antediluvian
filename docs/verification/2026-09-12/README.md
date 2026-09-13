# Screenshot verification pass — 2026-09-12

All shots taken with the new **windowless screenshot mode** (no window, no
focus change, safe while someone uses the Mac):

```
ANTEDILUVIA_SHOTS=<dir> ANTEDILUVIA_SHOT_TIMES=45,62 \
ANTEDILUVIA_AUTOCMD="god;tp 0 0;/guild X;/ahsell bread 5" \
target/release/antediluvia-client-bevy <apple_id> <Name> <ws://host:port | local>
```
- Shots are LDR (HDR into an image target renders geometry see-through);
  set `ANTEDILUVIA_SHOTS_HDR=1` to override. Colours differ slightly from
  the windowed game (no tonemap/bloom).
- Server-only systems: run an isolated server,
  `ANTEDILUVIA_BIND=127.0.0.1:8797 ANTEDILUVIA_DB=/tmp/x.sqlite
  ANTEDILUVIA_DEV_ACCOUNTS=<ids> antediluvia-server` — never 8787 (a live
  game may be using it). Client args: `<apple_id> <Name> <url>`; omitting the
  name opens the character builder — do NOT pass "" (it shifts the URL arg).

## Verified by screenshot
| System | Shot | Evidence |
|---|---|---|
| PvP auto-flag in enemy capital | `pvp_capital_autoflag.jpg` | "You enter the Sethite capital of Eden - you are flagged for PvP!" |
| PvP flag linger + dungeon discovery | `dungeon_discovery.jpg` | "PvP flag drops in 20s", "Discovered: The Desolate Glimmering Hollow (+75 xp)" |
| Generated creature bodies in world | `pvp_capital_autoflag.jpg` | Slag-Salamander Stalker (amphibian plan) at sane scale |
| Character builder | `character_builder.jpg` | 4 bodies x 16 skins x 12 hair = 768 looks, class abilities + icons |
| Guild, guild chat | `guild_ah_bank_mail_pvp_toggle.jpg` | "[G] Valorin: hello guild" |
| Auction house | same | "Listed bread for 5g (lot #6)", "6 auction lots" |
| Hearth home, bank, mail | same | "Home set", "Vault: 1 items", "No mail." |
| Optional PvP toggle | same | "flagged for PvP" / "PvP flag removed" |

## Defects found and fixed this pass
| Defect | Fix |
|---|---|
| Inn marker was a filled 220u yellow disc over the hamlet (`inn_before_...jpg`) | Thin pulsing ring at INN_RADIUS |
| Every NPC the same hooded rogue | Role bodies: Elder/Seer mage, Innkeeper/Jabal barbarian, Quartermaster/Sentinel knight, Wanderer rogue |
| NPCs unnamed in world | Floating gold name labels (hidden past 900u) |
| Builder = bare text over the world (`character_builder_before.jpg`) | Framed panel with counts per option |
| Missing-glyph boxes in HUD/chat (— · • ⚑ ▌ unsupported by default font) | ASCII in all client/sim/server strings |
| 0.5.6 app shipped WITHOUT terrain PBR textures (gitignored, absent from this tree) | Restored `assets/textures/pbr` from the 0.5.5 bundle; rebuild 0.5.7 includes them |

## Still NOT verified / known gaps
- Took 36 damage with `god` sent first on the remote server (possibly damage
  before the dev reply) — investigate.
- No auction-house browse window or guild roster panel (chat-only UX).
- Apple login with the real entitlement (dev-portal provisioning).
- 1000-player crowd re-check under the new renderer; dungeon boss close-up.
- Faces at gameplay distance: players default to Knight (helmet hides face).
