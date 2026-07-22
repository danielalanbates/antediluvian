# A4 Pass 1 results — 2026-07-22 (v0.2.0)
(Separate file: A4_AUDIT.md became iCloud-EPERM-locked mid-session.)

VERIFIED
- Hairstyle geometry renders on player rig in-game (screenshot).
- Dev console E2E on shipped v0.2.0: tp / spawn / god work; DEV AUDIT logged
  server-side; spawned mobs aggro, damage, kill, shrine-revive works.
- Server tick avg 175µs (1 conn) — headroom consistent with 6.3ms @ 1000 bots.
- Launcher reads live manifest; v0.2.0 release + notes shown; isNewer logic
  unit-tested incl. multi-digit versions.

PARTIAL / OPEN
- Mob silhouette close-up: spawns verified (distinct species + HP pools) but
  no close screenshot — remote keystrokes kept landing in chat. Eyeball live.
- Formations not sighted near shrine: FINDING — cluster some formations near
  roads/POIs so terrain variety shows in the first 15 minutes.
- Retina fps @1,200 formations/act unmeasured — add fps line to perf.rs.
- Audio by-ear + first-15-min human playtest: needs Daniel.

VERDICT (skeptical, honest)
Alpha-test ready: yes — stable build, full loop (build→release→manifest→
launcher), quests/factions/economy/PvP zones/char builder/dev tools all live.
"Millions would play": not yet — needs social systems (party/trade/bank/mail/
dungeons/battlegrounds), hosted server, hero-landmark art, onboarding polish.
That is the post-alpha queue, not alpha scope.
