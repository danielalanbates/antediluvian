# Staging verification checklist — web play on batesai.org

Run this gate **before every** `public/play/` update and **after every** site
deploy that touches `/play/` or `/ws`. It caught a real crash on 2026-09-13;
do not skip steps. Production shard (`:8787`) is never the test target.

## A. Build

- [ ] On branch `feature/web-play` (or the feature branch under test), working
      tree clean: `git status --short` empty.
- [ ] `scripts/build_web.sh` completes; wasm under 25 MB (the script enforces it).
- [ ] Record the build stamp printed by the script: `______________`.

## B. Local gate (staging)

- [ ] Fresh server: start a **test** shard on `:8797`
      (`ANTEDILUVIA_BIND=127.0.0.1:8797`, throwaway SQLite path — never the
      live DB, test characters must not land in production).
- [ ] Serve the bundle: `python3 -m http.server 8099` in `web/dist/`.
- [ ] Headless Chromium (Playwright, SwiftShader):
      `http://127.0.0.1:8099/?server=ws://127.0.0.1:8797&name=<fresh-name>`.
- [ ] Run **≥180 s LOGGED IN** (past the character builder — the builder screen
      streams no mobs and hides entity races).
- [ ] **FAIL** on: any wasm panic, any non-winit page error, login rejection,
      snapshot stall >5 s, or the client failing to reach the test shard.
- [ ] Known artifact (not a failure): headless SwiftShader renders characters
      semi-transparent at ~1 fps — seen in old and new builds alike.

## C. Site deploy (Daniel's checkout + credentials)

- [ ] Copy `web/dist/` → site checkout `public/play/`.
- [ ] Build the site; diff every page vs live with tags stripped —
      **must be identical except `/play/`**.
- [ ] `npx wrangler pages deploy dist --project-name=website --branch=main --commit-dirty=true`.
- [ ] Archive the previous bundle (`web-play-<old-stamp>`).

## D. Live verification

- [ ] Load `https://batesai.org/play/` fresh (cache-busted); loader reaches
      100%, canvas appears, no console errors.
- [ ] Log in and play ≥60 s: movement, one combat event, chat send/receive.
- [ ] Confirm `wss://batesai.org/ws` is the socket in devtools (same-origin,
      no mixed-content warnings).
- [ ] Spot-check the marketing pages (home, `/antediluvia`) render unchanged.

## E. Rollback (if anything above fails live)

- [ ] Cloudflare Pages dashboard → roll back to the previous deployment
      (restores both `/play/` and `/ws` in seconds).
- [ ] If the shard was restarted: restore `antediluvia-server.bak`,
      `sudo systemctl restart antediluvia`, confirm `systemctl is-active`.

## Sign-off

Build stamp: `____________` · Gate (local): PASS / FAIL · Live: PASS / FAIL ·
Deployed by: `____________` · Date: `____________`
