# A4 Pass 2 — 2026-07-22 (v0.3.0)

VERIFIED THIS PASS
- Party system (P1) live in v0.3.0: /party <name>, /paccept, /pleave; nearby
  members split kill XP + all get quest credit (server unit test); roster in
  the quest-tracker panel; leave/disconnect dissolves parties of one.
- Launcher auto-update FULL E2E: 0.2.0 installed + 0.3.0 manifest →
  "Update available" → Update → 40MB download from GitHub → sha256 verify →
  app swap → "Installed: v0.3.0", signature valid. The average-person flow
  (download launcher once, always current) is real and tested.
- Retina fps with 1,200 formations/act + POI clustering: 58.5–59.7.
- Source synced to github.com/danielalanbates/antediluvian (d0be562).
- Tests: 39 server + 7 client, all green. Protocol v14.

AUDIT VERDICT (v0.3.0)
Alpha-ready: YES, with confidence — stable, updatable, broad systems, now
with grouped play. The core WoW-Classic loop (quest → fight → loot → level →
talent → economy → PvP), 1000-player scale, and a real distribution channel
are all in place and verified. This is a game an alpha audience can genuinely
enjoy together.
The "millions" bar remains a roadmap, not a switch: trade/bank/mail,
dungeons/battlegrounds, a hosted public server, bespoke landmark art, and
onboarding tuning — sequenced in the post-alpha queue. No further honest
progress on that bar is possible without those systems and human playtests.
