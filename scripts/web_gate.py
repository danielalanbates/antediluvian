#!/usr/bin/env python3
"""Browser gate for the wasm client (docs/verification/2026-09-13).

Headless Chromium (SwiftShader) loads the bundle, gets a fresh character into
the world, stays logged in for --secs, and fails on any wasm panic, page
error, or a bad status notice from the client ("[beta] status: NewVersion",
"NoServer", "Disconnected", "Rejected").

Local (the default): the bundle served on 127.0.0.1:<http>, pointed at a local
server with ?server= (honoured on localhost pages only). Never the live shard.

  scripts/web_gate.py --http 8099 --ws ws://127.0.0.1:8797 --builder \\
      --server-log server.log --secs 200 --out shot.png --log console.log

Remote: --url gates a deployed page with its own server. --key-file is only
for the private /dev site (opens /dev/?key=... for the cookie; the key is
never printed). The public page needs no key. --server-log is optional: the
client's own "[beta] in world as <name>" console line marks entry. A remote
run on the public page creates a real character on that page's shard.

  scripts/web_gate.py --url https://batesai.org/play/ --builder --secs 200 \\
      --out live.png --log live_console.log

--builder takes the real new-player path: no ?name=, the client opens the
character builder, the gate types the name, saves --builder-shot, presses
Enter. Without it the page gets ?name= (local only; the public client ignores
it), which skips the builder.

--expect-status NAME passes as soon as the client shows that notice (e.g.
NewVersion against a server that speaks another protocol) instead of logging
in. Every run also checks that each pkg/ request carries the page's ?v=<stamp>.
"""
import argparse, random, re, sys, time
from urllib.parse import urlsplit
from playwright.sync_api import sync_playwright

ap = argparse.ArgumentParser()
ap.add_argument("--http", type=int, default=8099)
ap.add_argument("--ws", default="ws://127.0.0.1:8797")
ap.add_argument("--secs", type=int, default=200)
ap.add_argument("--out", required=True)
ap.add_argument("--log", required=True)
ap.add_argument("--url", help="gate a deployed page instead of the local bundle, e.g. https://batesai.org/play/ (its own ws)")
ap.add_argument("--key-file", help="with --url on the private /dev site: file holding the key (never printed)")
ap.add_argument("--server-log", help="server log; its login line marks entry into the world (optional: the client console line also does)")
ap.add_argument("--builder", action="store_true", help="create the character through the builder (the real new-player path)")
ap.add_argument("--builder-shot", help="with --builder: screenshot of the builder before Enter (default: <out>_builder.png)")
ap.add_argument("--expect-status", help="pass when the client shows this status notice (NewVersion, NoServer, ...)")
a = ap.parse_args()
if not a.url and ("159.54.191.177" in a.ws or ":8787" in a.ws or "batesai.org" in a.ws):
    sys.exit("refusing: live shard (local mode is for local servers only)")
if a.key_file and not (a.url and "/dev/" in a.url):
    sys.exit("refusing: --key-file is only for the private /dev site")
if a.url and not a.builder:
    print("note: remote pages ignore ?name=; using the builder", flush=True)
    a.builder = True

# A name the builder accepts as typed: letters only, a vowel, no letter three
# times in a row; stored with only the first letter capitalised.
tail = ""
while len(tail) < 5:
    c = random.choice("bcdfghjklmnprstvz" if len(tail) % 2 == 0 else "aeiou")
    tail += c
name = "Gate" + tail
if a.url:
    url = a.url
else:
    url = f"http://127.0.0.1:{a.http}/?server={a.ws}"
    if not a.builder:
        url += f"&name={name}"
shot_builder = a.builder_shot or re.sub(r"(\.\w+)?$", r"_builder\1", a.out, count=1)

lines, errors, pkg_urls = [], [], []
ANSI = re.compile(r"\x1b\[[0-9;]*m")
PANIC = re.compile(r"panicked|RuntimeError|unreachable", re.I)
BAD_STATUS = re.compile(r"\[beta\] status: (NewVersion|NoServer|Disconnected|Rejected)")

def in_world_line(l):
    return re.search(rf"\[beta\] in world as {name}\b", l, re.I)

with sync_playwright() as p:
    b = p.chromium.launch(headless=True, args=["--use-angle=swiftshader", "--enable-unsafe-swiftshader", "--ignore-gpu-blocklist"])
    pg = b.new_page(viewport={"width": 1280, "height": 720})
    pg.on("console", lambda m: lines.append(f"[{time.strftime('%H:%M:%S')}] {m.type}: {m.text}"))
    pg.on("request", lambda r: pkg_urls.append(r.url) if "/pkg/" in r.url else None)

    def perr(e):
        s = str(e)
        lines.append(f"PAGEERROR: {s}")
        # winit throws a control-flow exception by design on the web.
        if "Using exceptions for control flow" not in s:
            errors.append(s)
    pg.on("pageerror", perr)
    if a.key_file:
        u = urlsplit(a.url)
        key = open(a.key_file).read().strip()
        pg.goto(f"{u.scheme}://{u.netloc}/dev/?key={key}", wait_until="load", timeout=60000)
        del key
    print("open", url, "as", name, flush=True)
    pg.goto(url, wait_until="load", timeout=120000)
    stamp_m = re.search(r"BUILD_STAMP = '([0-9a-f]{12})'", pg.content())
    stamp = stamp_m.group(1) if stamp_m else None
    t0 = time.time()
    entered = None
    typed_at = None
    builder_seen = False
    shot_taken = False
    last_enter = 0.0
    expected = None
    while time.time() - t0 < 900:
        # Builder flow: wait for the client to open it, type, shoot, submit.
        if a.builder and entered is None:
            if not builder_seen and any("[beta] character builder open" in l for l in lines):
                builder_seen = True
                print("builder open at", round(time.time() - t0), "s", flush=True)
                try:
                    pg.mouse.click(900, 360)  # focus the canvas, right of the panel
                    pg.keyboard.type(name, delay=150)
                except Exception as e:
                    lines.append(f"input err {e}")
                typed_at = time.time()
            if typed_at and time.time() - typed_at > 15 and time.time() - last_enter > 25:
                # Frames are ~1 s apart under SwiftShader: give the panel time
                # to show the name, then press Enter (again if nothing came of
                # it; in the world Enter would open chat, so not too often).
                if not shot_taken:
                    pg.screenshot(path=shot_builder)
                    shot_taken = True
                    print("builder shot", shot_builder, flush=True)
                try:
                    pg.keyboard.press("Enter")
                except Exception as e:
                    lines.append(f"input err {e}")
                last_enter = time.time()
        elif entered is None and time.time() - t0 > 20:
            try:
                pg.mouse.click(640, 360)
                pg.keyboard.press("Enter")
            except Exception as e:
                lines.append(f"input err {e}")
        slog = ""
        if a.server_log:
            try:
                slog = ANSI.sub("", open(a.server_log, errors="replace").read())
            except OSError:
                pass
        # tracing output prints the message ("login (...)") before the fields,
        # so match the login line and our name in either order.
        if entered is None and (
            any("login" in l and re.search(rf"name={name}\b", l, re.I) for l in slog.splitlines())
            or any(in_world_line(l) for l in lines)
        ):
            entered = time.time()
            print("in world at", round(entered - t0), "s", flush=True)
        if a.expect_status and any(f"[beta] status: {a.expect_status}" in l for l in lines):
            expected = time.time()
            print("saw expected status", a.expect_status, "at", round(expected - t0), "s", flush=True)
            time.sleep(8)  # let the notice render before the shot
            break
        if entered and time.time() - entered >= a.secs:
            break
        if any(PANIC.search(l) for l in lines) or errors:
            break
        if not a.expect_status and any(BAD_STATUS.search(l) for l in lines):
            break
        time.sleep(5)
    pg.screenshot(path=a.out)
    b.close()

open(a.log, "w").write("\n".join(lines))
panics = [l for l in lines if PANIC.search(l)]
bad = [l for l in lines if BAD_STATUS.search(l)] if not a.expect_status else []
unbusted = [u for u in pkg_urls if not stamp or f"v={stamp}" not in u]
logged = round(time.time() - entered) if entered else 0
print(f"name={name} stamp={stamp} logged_in_secs={logged} console_lines={len(lines)} panics={len(panics)} "
      f"page_errors={len(errors)} bad_status={len(bad)} pkg_requests={len(pkg_urls)} pkg_without_stamp={len(unbusted)}")
for l in (panics + errors + bad)[:10]:
    print("  ", l[:300])
for u in unbusted[:5]:
    print("   unstamped:", u)
if a.expect_status:
    ok = bool(expected) and not panics and not errors and not unbusted
else:
    ok = bool(entered) and logged >= a.secs and not panics and not errors and not bad and not unbusted and bool(pkg_urls)
    if a.builder and not builder_seen:
        ok = False
        print("   builder never opened")
print("GATE", "PASS" if ok else "FAIL")
sys.exit(0 if ok else 1)
