# ARCHITECT_TODO — Antediluvia
_Head Architect review 2026-07-02. Full context: `Code/_ARCHITECT_REVIEW_2026-07-02/`._
_Worker rules: NEVER delete source code (only venv/node_modules/target/.build/__pycache__/dist and compiled artifacts). Move whole projects to `Code/Archived_Projects/<name>/` with WHY_ARCHIVED.md. Before moving a folder, run: grep -rl "Antediluvia" ~/Library/LaunchAgents/ "$HOME/Library/Application Support/BatesAI/" — if anything matches, STOP and report. Secrets (*.p8, *.p12, .env, keys) go to ~/Library/Application Support/BatesAI/keys/ chmod 600. Do ONE step at a time; verify; report._

**Verdict:** INVESTIGATE — original game, needs a map
**What this is:** Substantial original game: Rust source + server/ + an archived TypeScript/Phaser prototype. Direction unclear from outside.

## Steps
1. Delete `target/`, `node_modules/`, `dist/` from this iCloud copy.
2. Read Cargo.toml + top of src/; write `PROJECT.md`: what the game is, engine, how to run. Report a 5-line summary to Daniel.

## Resolution (2026-07-09) — DONE
Both steps complete, and the game was carried well past "map it":
1. ✅ Build artifacts cleared; project moved off iCloud to `~/Documents/Antediluvia`
   (git repo → `github.com/danielalanbates/antediluvian`). TypeScript detour
   archived to `archive_typescript_2026-07-09/`.
2. ✅ `PROJECT.md` written (what/engine/layout/how-to-run).

Direction (chosen by Daniel): **full Rust MMORPG**. Delivered a working,
verified vertical slice — authoritative headless server + networked Bevy client
+ area-of-interest snapshots + resource harvesting + tests. See `PROJECT.md`
"Status" for done-vs-deferred.

## GOAL (set by Daniel 2026-07-09) — WoW-Classic-class MMORPG in Rust
Antediluvia is to become a Rust-based competitor to World of Warcraft, with
**full WoW-Classic-style graphics and systems**: 3D world + character rendering,
combat, class skills/spells, PvP, talent trees, guilds, professions, auction
houses, inns/rest, and the rest of the classic feature set. The current 2D Bevy
client is a stepping stone — the client must move to Bevy 3D. Server stays
headless/authoritative. Track the roadmap in `PROJECT.md`.
