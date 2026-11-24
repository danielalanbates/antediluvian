#!/usr/bin/env python3
"""Local browser gate for the wasm client (docs/verification/2026-09-13).

Headless Chromium (SwiftShader) loads the bundle served on 127.0.0.1:<http>,
pointed at a local dev-mode server, creates a fresh character (Enter on the
builder), stays logged in for --secs, and fails on any wasm panic or
non-winit page error. Never point this at the live shard.

  scripts/web_gate.py --http 8099 --ws ws://127.0.0.1:8797 --secs 200 --out shot.png
"""
import argparse, random, re, sys, time
from playwright.sync_api import sync_playwright

ap = argparse.ArgumentParser()
ap.add_argument("--http", type=int, default=8099)
ap.add_argument("--ws", default="ws://127.0.0.1:8797")
ap.add_argument("--secs", type=int, default=200)
ap.add_argument("--out", required=True)
ap.add_argument("--log", required=True)
ap.add_argument("--server-log", required=True, help="local server log; its login line marks entry into the world")
a = ap.parse_args()
if "159.54.191.177" in a.ws or ":8787" in a.ws:
    sys.exit("refusing: live shard")

name = "Gate" + "".join(random.choice("abcdefghijklmnop") for _ in range(5)).capitalize()
url = f"http://127.0.0.1:{a.http}/?server={a.ws}&name={name}"
lines, errors = [], []
PANIC = re.compile(r"panicked|RuntimeError|unreachable", re.I)

with sync_playwright() as p:
    b = p.chromium.launch(headless=True, args=["--use-angle=swiftshader", "--enable-unsafe-swiftshader", "--ignore-gpu-blocklist"])
    pg = b.new_page(viewport={"width": 1280, "height": 720})
    pg.on("console", lambda m: lines.append(f"[{time.strftime('%H:%M:%S')}] {m.type}: {m.text}"))
    def perr(e):
        s = str(e)
        lines.append(f"PAGEERROR: {s}")
        # winit throws a control-flow exception by design on the web.
        if "Using exceptions for control flow" not in s:
            errors.append(s)
    pg.on("pageerror", perr)
    print("open", url, flush=True)
    pg.goto(url, wait_until="load", timeout=120000)
    t0 = time.time()
    entered = None
    while time.time() - t0 < 600:
        if entered is None and time.time() - t0 > 20:
            # Builder: click the canvas for focus, Enter submits the character.
            try:
                pg.mouse.click(640, 360)
                pg.keyboard.press("Enter")
            except Exception as e:
                lines.append(f"input err {e}")
        try:
            slog = open(a.server_log, errors="replace").read()
        except OSError:
            slog = ""
        if entered is None and re.search(rf"name={name}\b.*login", slog):
            entered = time.time()
            print("in world at", round(entered - t0), "s", flush=True)
        if entered and time.time() - entered >= a.secs:
            break
        if any(PANIC.search(l) for l in lines) or errors:
            break
        time.sleep(5)
    pg.screenshot(path=a.out)
    b.close()

open(a.log, "w").write("\n".join(lines))
panics = [l for l in lines if PANIC.search(l)]
logged = round(time.time() - entered) if entered else 0
print(f"name={name} logged_in_secs={logged} console_lines={len(lines)} panics={len(panics)} page_errors={len(errors)}")
for l in (panics + errors)[:10]:
    print("  ", l[:300])
ok = entered and logged >= a.secs and not panics and not errors
print("GATE", "PASS" if ok else "FAIL")
sys.exit(0 if ok else 1)
