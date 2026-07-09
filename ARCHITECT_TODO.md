# ARCHITECT_TODO — Antediluvia
_Head Architect review 2026-07-02. Full context: `Code/_ARCHITECT_REVIEW_2026-07-02/`._
_Worker rules: NEVER delete source code (only venv/node_modules/target/.build/__pycache__/dist and compiled artifacts). Move whole projects to `Code/Archived_Projects/<name>/` with WHY_ARCHIVED.md. Before moving a folder, run: grep -rl "Antediluvia" ~/Library/LaunchAgents/ "$HOME/Library/Application Support/BatesAI/" — if anything matches, STOP and report. Secrets (*.p8, *.p12, .env, keys) go to ~/Library/Application Support/BatesAI/keys/ chmod 600. Do ONE step at a time; verify; report._

**Verdict:** INVESTIGATE — original game, needs a map
**What this is:** Substantial original game: Rust source + server/ + an archived TypeScript/Phaser prototype. Direction unclear from outside.

## Steps
1. Delete `target/`, `node_modules/`, `dist/` from this iCloud copy.
2. Read Cargo.toml + top of src/; write `PROJECT.md`: what the game is, engine, how to run. Report a 5-line summary to Daniel.
