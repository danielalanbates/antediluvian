# Antediluvia server deployment (Oracle Cloud Always Free)

Target: an **Ampere A1 (ARM64) Ubuntu 22.04** VM on Oracle Cloud Always Free.
The server is a single Rust process (measured 1000 players ≈ 6.3 ms/tick, ~78 MB
RSS), so even 1 OCPU / 4 GB is ample.

## One-time: provision the VM (you do this in the Oracle console)
1. Create an **Always Free** compute instance, shape **VM.Standard.A1.Flex**
   (ARM), image **Ubuntu 22.04**. Give it 1–2 OCPU and 4–6 GB (all free-tier).
2. Download the SSH private key it generates; note the **public IP**.
3. In the instance's VCN **security list**, add an ingress rule allowing
   TCP **443** (and 80 for the TLS cert challenge) from 0.0.0.0/0.
4. (Recommended) point a DNS name at the IP — e.g. a Cloudflare record
   `play.batesai.org` → the VM IP. Browser WSS needs a hostname + cert.

## Deploy (I run this once you give me the IP + key path)
```
./deploy.sh <ssh-key-path> ubuntu@<vm-ip> play.batesai.org
```
This rsyncs the source, builds release **on the VM** (native ARM — no
cross-compile headaches), installs a systemd service, and sets up **Caddy**
for automatic Let's Encrypt TLS terminating `wss://play.batesai.org` → the
local game server on 127.0.0.1:8787.

## Pointing the native client at the shard
The bundled `.app` defaults to spawning its own local server. To play on the
hosted shard instead, set either (env var wins):

```
ANTEDILUVIA_SERVER=wss://play.batesai.org open -a Antediluvia
# or, to make it stick across launches:
echo 'wss://play.batesai.org' > ~/Library/Application\ Support/Antediluvia/server_url
```

When a remote URL is set the launcher does **not** start a local server, and
kills any stray one so the next local launch isn't confused by it.

## After deploy
- Service: `systemctl status antediluvia` (auto-restarts, starts on boot).
- Logs: `journalctl -u antediluvia -f`.
- The browser client connects to `wss://play.batesai.org`.
- DB persists at `/var/lib/antediluvia/antediluvia.sqlite`.

## Pre-flight fixes applied 2026-07-29 (all four would have failed a first run)
1. **Client had no TLS at all.** `tokio-tungstenite` was declared with no TLS
   feature, so `connect_async("wss://…")` could never succeed. Now built with
   `rustls-tls-webpki-roots`.
2. **rustls 0.23 crypto provider.** With the feature on, rustls still panicked
   (`Could not automatically determine the process-level CryptoProvider`)
   because cargo feature unification enables more than one provider. Both
   clients now call `rustls::crypto::ring::default_provider().install_default()`
   before dialing. Verified live against `wss://echo.websocket.org`.
3. **deploy.sh synced 3 crates into a 5-member workspace** — cargo hard-errors
   on a missing member. Step 2 now rewrites `members`/`default-members` on the
   VM to protocol/sim/server.
4. **`crates/sim` `include_str!`s `assets/data/*.json` at compile time**
   (caves, mobs, pois, prices, pvp_zones) and those were never synced. Step 2
   now rsyncs `assets/data/`.

Also added: Oracle's Ubuntu images ship a default REJECT in the host `INPUT`
chain, so opening the VCN security list is *not* sufficient — step 1 now opens
80/443 in iptables and persists it. This is the usual "deploys clean, can't
connect" cause.

Verified locally before deploy: the exact 3-crate payload builds standalone,
the resulting binary serves, and two simultaneous clients each report
`players 2` (real shared world, not two solo sessions).

Alternative to Caddy: put Cloudflare's proxy in front (orange-cloud the DNS
record) — it terminates TLS and proxies WebSockets to port 443 on the VM.
Then Caddy can serve plain and Cloudflare handles the cert. deploy.sh uses
Caddy by default since it's self-contained.
