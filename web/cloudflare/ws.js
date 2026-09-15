/**
 * WebSocket proxy: wss://batesai.org/ws -> the Antediluvia game server.
 *
 * DRAFT — NOT APPLIED. This file lives in the antediluvian repo so the proxy
 * config is versioned next to the client it serves. To use it, copy it over
 * the site repo's `functions/ws.js` and redeploy the site.
 *
 * What changed vs the live `functions/ws.js` (2026-09-15):
 *   - SHARD_ORIGIN points at `shard.batesai.org` (a real DNS A record) instead
 *     of `159-54-191-177.sslip.io`. Same port 80, same :80 -> :8787 NAT on the
 *     VM, same Cloudflare TLS termination — the only difference is the name
 *     resolves via Daniel's own DNS instead of a third-party wildcard service.
 *
 * PREREQUISITE (Daniel's approval required): create the DNS record FIRST —
 *   Type: A | Name: shard | Content: 159.54.191.177 | Proxy: OFF (grey cloud)
 * Grey cloud matters: if it were proxied (orange), the Function's subrequest
 * would hairpin back through Cloudflare's edge instead of reaching the VM.
 * Then copy this file to the site repo and `wrangler pages deploy`.
 *
 * Rollback: Pages keeps deployment history — roll back to the previous
 * deployment, or restore the old SHARD_ORIGIN line (kept below as a comment).
 */

// const SHARD_ORIGIN_OLD = 'http://159-54-191-177.sslip.io:80'; // pre-DNS, 2026-09-15 and earlier
const SHARD_ORIGIN = 'http://shard.batesai.org:80';

export async function onRequest(context) {
  const { request } = context;

  // Anything that isn't an upgrade is almost certainly a human opening the URL
  // in a tab; 426 says exactly what's wrong instead of hanging.
  if ((request.headers.get('Upgrade') || '').toLowerCase() !== 'websocket') {
    return new Response(
      'This endpoint serves the Antediluvia game client over WebSocket.\n' +
        'Open https://batesai.org/antediluvia to play.\n',
      { status: 426, headers: { 'Content-Type': 'text/plain' } }
    );
  }

  const incoming = new URL(request.url);
  const target = new URL(SHARD_ORIGIN);
  target.search = incoming.search;

  // Forwarding the original Request preserves the Sec-WebSocket-* handshake
  // headers; the runtime pairs the resulting 101 with its socket for us.
  return fetch(new Request(target.toString(), request));
}
