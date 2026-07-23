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

## After deploy
- Service: `systemctl status antediluvia` (auto-restarts, starts on boot).
- Logs: `journalctl -u antediluvia -f`.
- The browser client connects to `wss://play.batesai.org`.
- DB persists at `/var/lib/antediluvia/antediluvia.sqlite`.

Alternative to Caddy: put Cloudflare's proxy in front (orange-cloud the DNS
record) — it terminates TLS and proxies WebSockets to port 443 on the VM.
Then Caddy can serve plain and Cloudflare handles the cert. deploy.sh uses
Caddy by default since it's self-contained.
