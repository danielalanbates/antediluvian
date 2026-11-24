/**
 * Dev WebSocket proxy: wss://batesai.org/dev/ws -> the DEV shard on the Oracle VM.
 *
 * DRAFT — NOT APPLIED. Part of the private dev site (docs/beta/DEV_SITE.md):
 * pre-release builds are tested at batesai.org/dev/play before /antediluvia gets them.
 *
 * To use: copy to the site repo as `functions/dev/ws.js`, alongside the
 * cookie-gate middleware (`functions/dev/_middleware.js`, see DEV_SITE.md),
 * and redeploy the site.
 *
 * Dev shard: `antediluvia-dev.service` on the Oracle VM, :8788, with an
 * iptables NAT 8080 -> 8788 and the OCI security list opened for 8080.
 * Port 8080 (unlike 8787) is on Cloudflare's allowed outbound-port list.
 * Same shard.batesai.org DNS prerequisite as the production proxy —
 * see ws.js in this directory.
 */

const DEV_SHARD_ORIGIN = 'http://shard.batesai.org:8080';

export async function onRequest(context) {
  const { request } = context;

  if ((request.headers.get('Upgrade') || '').toLowerCase() !== 'websocket') {
    return new Response(
      'This endpoint serves the Antediluvia DEV client over WebSocket.\n' +
        'It is private; the production game is at https://batesai.org/antediluvia.\n',
      { status: 426, headers: { 'Content-Type': 'text/plain' } }
    );
  }

  const incoming = new URL(request.url);
  const target = new URL(DEV_SHARD_ORIGIN);
  target.search = incoming.search;

  return fetch(new Request(target.toString(), request));
}
