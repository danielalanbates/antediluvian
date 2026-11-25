//! Networking bridge between Bevy (sync, main thread) and the WebSocket
//! connection to the Antediluvia server.
//!
//! Two transports, one API. Bevy always talks to the socket through two tokio
//! mpsc channels — both ends are usable without a runtime:
//! `UnboundedSender::send` and `UnboundedReceiver::try_recv` are plain sync
//! calls. The outbound sender is a normal `Resource`; the inbound receiver is a
//! `NonSend` resource (it isn't `Sync`) drained on the main thread each frame.
//!
//! * **Native** — a dedicated OS thread runs a single-threaded tokio runtime
//!   that owns a `tokio-tungstenite` socket.
//! * **Browser (wasm32)** — there are no OS threads and no tokio reactor, so
//!   the browser's own `WebSocket` is driven by callbacks on the JS event loop
//!   and pumped by a `spawn_local` task.
//!
//! `NetTx`/`NetRx` are the same types on both targets, so nothing downstream of
//! this module needs to know which transport it is talking to.
//!
//! World snapshots arrive as binary delta frames (B11,
//! `antediluvia_protocol::wire`). `Inbound` is the one decoder both transports
//! feed: it keeps the baseline and hands Bevy a whole `ServerMsg::Snapshot`
//! per frame, so the game code never sees the wire format. Everything else
//! stays JSON text.

use antediluvia_protocol::wire::{SnapshotDecoder, WIRE_BINARY, WIRE_JSON};
use antediluvia_protocol::{ClientMsg, ServerMsg, PROTOCOL_VERSION};
use bevy::prelude::*;
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};

/// Outbound channel to the server (clone-friendly, Send + Sync).
#[derive(Resource, Clone)]
pub struct NetTx(pub UnboundedSender<ClientMsg>);

impl NetTx {
    pub fn send(&self, msg: ClientMsg) {
        let _ = self.0.send(msg);
    }
}

/// Inbound channel from the server. Stored as a non-send resource.
pub struct NetRx(pub UnboundedReceiver<ServerMsg>);

/// State of the one socket this client opens, for the status notice
/// (`crate::beta`). A process-wide atomic rather than a resource: both
/// transports set it from their own callbacks or thread, and the native and
/// browser builds keep `start_network`'s signature unchanged.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Link {
    /// Dialling; no answer yet.
    Connecting,
    /// Socket open.
    Open,
    /// Was open, then closed (server restart, network drop).
    Closed,
    /// Never opened (bad URL, server down, proxy refused).
    Failed,
}

static LINK: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(0);

pub fn link() -> Link {
    match LINK.load(std::sync::atomic::Ordering::Relaxed) {
        1 => Link::Open,
        2 => Link::Closed,
        3 => Link::Failed,
        _ => Link::Connecting,
    }
}

pub fn set_link(l: Link) {
    let v = match l {
        Link::Connecting => 0,
        Link::Open => 1,
        Link::Closed => 2,
        Link::Failed => 3,
    };
    LINK.store(v, std::sync::atomic::Ordering::Relaxed);
}

/// The socket went away: `Closed` if it had opened, else `Failed`.
fn mark_down() {
    set_link(if link() == Link::Open || link() == Link::Closed { Link::Closed } else { Link::Failed });
}

/// The `Login` every connection opens with, pre-serialized. The account id
/// travels in the protocol's `apple_id` field (a historical name: it is any
/// opaque per-player id, see `crate::launch_config`).
fn login_json(account_id: String, character_name: Option<String>) -> Option<String> {
    serde_json::to_string(&ClientMsg::Login {
        proto: PROTOCOL_VERSION,
        apple_id: account_id,
        character_name,
        create: None,
        wire: wanted_wire(),
    })
    .ok()
}

/// Snapshot wire this client asks for at login: binary delta frames, unless
/// `ANTEDILUVIA_WIRE=json` (native only, for comparing the two paths).
pub fn wanted_wire() -> u8 {
    #[cfg(not(target_arch = "wasm32"))]
    if std::env::var("ANTEDILUVIA_WIRE").is_ok_and(|v| v.eq_ignore_ascii_case("json")) {
        return WIRE_JSON;
    }
    let _ = WIRE_JSON;
    WIRE_BINARY
}

/// Turns socket frames into `ServerMsg`s, for both transports.
#[derive(Default)]
pub(crate) struct Inbound {
    dec: SnapshotDecoder,
    /// A `Resync` is on its way; do not ask again until a keyframe lands.
    asked: bool,
}

impl Inbound {
    /// A text frame: one JSON `ServerMsg`.
    pub fn text(&mut self, txt: &str) -> Result<ServerMsg, serde_json::Error> {
        serde_json::from_str::<ServerMsg>(txt)
    }

    /// A binary frame: one delta snapshot. Returns the whole view for Bevy
    /// and, when the frame could not be applied, the `Resync` to send back.
    pub fn binary(&mut self, bytes: &[u8]) -> (Option<ServerMsg>, Option<ClientMsg>) {
        match self.dec.apply(bytes) {
            Ok(st) => {
                if st.key {
                    self.asked = false;
                }
                (Some(self.dec.snapshot()), None)
            }
            Err(_) if !self.asked => {
                self.asked = true;
                (None, Some(ClientMsg::Resync))
            }
            Err(_) => (None, None),
        }
    }
}

// ---------------------------------------------------------------------------
// Native transport
// ---------------------------------------------------------------------------

#[cfg(not(target_arch = "wasm32"))]
mod native {
    use super::*;
    use futures_util::{SinkExt, StreamExt};
    use std::thread;
    use tokio_tungstenite::tungstenite::Message;

    /// rustls 0.23 refuses to pick a crypto backend on its own once cargo
    /// feature unification enables more than one provider — it panics inside
    /// `connect_async` on the first `wss://` dial. Pin `ring` before
    /// connecting. Idempotent: a repeat call returns Err, which we ignore.
    fn install_crypto_provider() {
        let _ = rustls::crypto::ring::default_provider().install_default();
    }

    pub fn start_network(
        url: String,
        account_id: String,
        character_name: Option<String>,
    ) -> (NetTx, NetRx) {
        install_crypto_provider();
        set_link(Link::Connecting);
        let (tx_client, mut rx_client) = unbounded_channel::<ClientMsg>();
        let (tx_server, rx_server) = unbounded_channel::<ServerMsg>();

        thread::Builder::new()
            .name("antediluvia-net".into())
            .spawn(move || {
                let rt = match tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                {
                    Ok(rt) => rt,
                    Err(e) => {
                        eprintln!("[net] failed to build runtime: {e}");
                        return;
                    }
                };
                rt.block_on(async move {
                    let (ws, _) = match tokio_tungstenite::connect_async(&url).await {
                        Ok(x) => x,
                        Err(e) => {
                            eprintln!("[net] connect to {url} failed: {e}");
                            set_link(Link::Failed);
                            return;
                        }
                    };
                    println!("[net] connected to {url}");
                    set_link(Link::Open);
                    let (mut sink, mut stream) = ws.split();
                    let mut decode = Inbound::default();

                    // Authenticate immediately.
                    if let Some(txt) = login_json(account_id, character_name) {
                        let _ = sink.send(Message::Text(txt.into())).await;
                    }

                    loop {
                        tokio::select! {
                            outbound = rx_client.recv() => match outbound {
                                Some(msg) => {
                                    if let Ok(txt) = serde_json::to_string(&msg) {
                                        if sink.send(Message::Text(txt.into())).await.is_err() {
                                            break;
                                        }
                                    }
                                }
                                None => break, // Bevy side dropped the sender
                            },
                            inbound = stream.next() => match inbound {
                                Some(Ok(Message::Text(txt))) => {
                                    if let Ok(msg) = decode.text(&txt) {
                                        if tx_server.send(msg).is_err() {
                                            break;
                                        }
                                    }
                                }
                                Some(Ok(Message::Binary(bytes))) => {
                                    let (msg, ask) = decode.binary(&bytes);
                                    if let Some(msg) = msg {
                                        if tx_server.send(msg).is_err() {
                                            break;
                                        }
                                    }
                                    if let Some(txt) = ask.and_then(|m| serde_json::to_string(&m).ok()) {
                                        eprintln!("[net] snapshot frame did not apply; asking for a keyframe");
                                        if sink.send(Message::Text(txt.into())).await.is_err() {
                                            break;
                                        }
                                    }
                                }
                                Some(Ok(Message::Close(_))) | None => break,
                                Some(Err(e)) => {
                                    eprintln!("[net] read error: {e}");
                                    break;
                                }
                                _ => {}
                            },
                        }
                    }
                    println!("[net] disconnected");
                    mark_down();
                });
            })
            .expect("spawn network thread");

        (NetTx(tx_client), NetRx(rx_server))
    }
}

// ---------------------------------------------------------------------------
// Browser transport
// ---------------------------------------------------------------------------

#[cfg(target_arch = "wasm32")]
mod web {
    use super::*;
    use std::cell::RefCell;
    use std::rc::Rc;
    use wasm_bindgen::prelude::*;
    use wasm_bindgen::JsCast;
    use web_sys::{CloseEvent, ErrorEvent, MessageEvent, WebSocket};

    /// Messages produced before the socket finishes opening. The browser throws
    /// on `send()` in the CONNECTING state, so they queue here and are flushed
    /// by the `onopen` handler. Everything below runs on the single JS thread,
    /// so `Rc`/`RefCell` are sound (and `spawn_local` does not require `Send`).
    type Pending = Rc<RefCell<Vec<String>>>;

    fn flush(ws: &WebSocket, pending: &Pending) {
        if ws.ready_state() != WebSocket::OPEN {
            return;
        }
        for txt in pending.borrow_mut().drain(..) {
            if let Err(e) = ws.send_with_str(&txt) {
                error!("[net] send failed: {e:?}");
            }
        }
    }

    pub fn start_network(
        url: String,
        account_id: String,
        character_name: Option<String>,
    ) -> (NetTx, NetRx) {
        let (tx_client, mut rx_client) = unbounded_channel::<ClientMsg>();
        let (tx_server, rx_server) = unbounded_channel::<ServerMsg>();
        set_link(Link::Connecting);

        let ws = match WebSocket::new(&url) {
            Ok(ws) => ws,
            Err(e) => {
                // Nothing to connect to: hand back live-but-silent channels so
                // the app still boots and can show the failure in its UI.
                error!("[net] could not open {url}: {e:?}");
                set_link(Link::Failed);
                return (NetTx(tx_client), NetRx(rx_server));
            }
        };
        // Text frames are JSON messages; binary frames (delta snapshots)
        // must arrive as ArrayBuffers, not Blobs, to be read synchronously.
        ws.set_binary_type(web_sys::BinaryType::Arraybuffer);

        let pending: Pending = Rc::new(RefCell::new(Vec::new()));
        // Authenticate as soon as the socket is writable.
        if let Some(txt) = login_json(account_id, character_name) {
            pending.borrow_mut().push(txt);
        }

        // --- inbound: browser callback -> Bevy channel ---
        {
            let tx_server = tx_server.clone();
            let ws_in = ws.clone();
            let pending_in = pending.clone();
            let mut inbound = Inbound::default();
            let on_message = Closure::<dyn FnMut(MessageEvent)>::new(move |e: MessageEvent| {
                let data = e.data();
                if let Some(txt) = data.as_string() {
                    match inbound.text(&txt) {
                        Ok(msg) => {
                            let _ = tx_server.send(msg);
                        }
                        Err(err) => warn!("[net] undecodable frame: {err}"),
                    }
                } else if let Ok(buf) = data.dyn_into::<js_sys::ArrayBuffer>() {
                    // Binary delta snapshot (B11).
                    let bytes = js_sys::Uint8Array::new(&buf).to_vec();
                    let (msg, ask) = inbound.binary(&bytes);
                    if let Some(msg) = msg {
                        let _ = tx_server.send(msg);
                    }
                    if let Some(txt) = ask.and_then(|m| serde_json::to_string(&m).ok()) {
                        warn!("[net] snapshot frame did not apply; asking for a keyframe");
                        pending_in.borrow_mut().push(txt);
                        flush(&ws_in, &pending_in);
                    }
                }
            });
            ws.set_onmessage(Some(on_message.as_ref().unchecked_ref()));
            on_message.forget();
        }

        // --- lifecycle logging + login flush ---
        {
            let ws_open = ws.clone();
            let pending_open = pending.clone();
            let url_open = url.clone();
            let on_open = Closure::<dyn FnMut(JsValue)>::new(move |_| {
                info!("[net] connected to {url_open}");
                set_link(Link::Open);
                flush(&ws_open, &pending_open);
            });
            ws.set_onopen(Some(on_open.as_ref().unchecked_ref()));
            on_open.forget();

            let on_error = Closure::<dyn FnMut(ErrorEvent)>::new(move |e: ErrorEvent| {
                // Browsers deliberately withhold the reason (it would leak
                // cross-origin information); the close event carries more.
                error!("[net] socket error: {}", e.message());
            });
            ws.set_onerror(Some(on_error.as_ref().unchecked_ref()));
            on_error.forget();

            let on_close = Closure::<dyn FnMut(CloseEvent)>::new(move |e: CloseEvent| {
                info!("[net] disconnected (code {} {})", e.code(), e.reason());
                mark_down();
            });
            ws.set_onclose(Some(on_close.as_ref().unchecked_ref()));
            on_close.forget();
        }

        // --- outbound: Bevy channel -> browser socket ---
        {
            let ws = ws.clone();
            let pending = pending.clone();
            wasm_bindgen_futures::spawn_local(async move {
                // `recv()` is a plain future over an mpsc queue — no tokio
                // reactor involved, so it drives fine on the JS event loop.
                while let Some(msg) = rx_client.recv().await {
                    if let Ok(txt) = serde_json::to_string(&msg) {
                        pending.borrow_mut().push(txt);
                    }
                    flush(&ws, &pending);
                    if ws.ready_state() == WebSocket::CLOSED {
                        break;
                    }
                }
            });
        }

        (NetTx(tx_client), NetRx(rx_server))
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub use native::start_network;
#[cfg(target_arch = "wasm32")]
pub use web::start_network;

#[cfg(test)]
mod wire_tests {
    use super::*;
    use antediluvia_protocol::wire::FrameWriter;
    use antediluvia_protocol::{Act, EntityKind, EntityState};

    fn ent(id: u64, x: f32) -> EntityState {
        EntityState {
            id,
            kind: EntityKind::Player,
            x,
            y: 0.0,
            rot: 0.0,
            health: 10,
            max_health: 10,
            tag: None,
            name: Some(format!("P{id}")),
            weapon: None,
            chest: None,
            back: None,
            faction: None,
            appearance: Some([0, 0, 0]),
            look: Some(0),
            mounted: false,
            mount_species: None,
        }
    }

    fn key(seq: u32, ids: &[u64]) -> Vec<u8> {
        let mut w = FrameWriter::begin(true, Act::Eden, seq, 7, 0.5);
        for id in ids {
            w.enter_state(&ent(*id, 1.0));
        }
        w.finish()
    }

    /// Both transports feed `Inbound`: a binary frame becomes the same
    /// `ServerMsg::Snapshot` the JSON wire carries, and text still parses.
    #[test]
    fn binary_frames_become_whole_snapshots() {
        let mut inb = Inbound::default();
        let (msg, ask) = inb.binary(&key(1, &[4, 5]));
        assert!(ask.is_none());
        match msg {
            Some(ServerMsg::Snapshot { act, tick, entities, .. }) => {
                assert_eq!((act, tick, entities.len()), (Act::Eden, 7, 2));
                assert!(entities.iter().any(|e| e.name.as_deref() == Some("P5")));
            }
            other => panic!("{other:?}"),
        }
        // A delta that only moves entity 4 still yields the whole view.
        let mut w = FrameWriter::begin(false, Act::Eden, 2, 8, 0.5);
        w.update(4, antediluvia_protocol::wire::F_POS, antediluvia_protocol::wire::Dyn::new(50.0, 0.0, 0.0, 10, 10), &[]);
        let (msg, _) = inb.binary(&w.finish());
        let Some(ServerMsg::Snapshot { entities, .. }) = msg else { panic!() };
        assert_eq!(entities.len(), 2);
        assert_eq!(entities.iter().find(|e| e.id == 4).unwrap().x, 50.0);
        assert!(matches!(inb.text("{\"t\":\"pong\"}"), Ok(ServerMsg::Pong)));
    }

    /// A frame that does not apply asks for a keyframe once, yields nothing
    /// until it arrives, then carries on.
    #[test]
    fn a_bad_frame_asks_for_one_resync() {
        let mut inb = Inbound::default();
        assert!(inb.binary(&key(1, &[4])).0.is_some());
        // Sequence 2 is lost; 3 arrives.
        let delta = |seq| FrameWriter::begin(false, Act::Eden, seq, 9, 0.5).finish();
        let (msg, ask) = inb.binary(&delta(3));
        assert!(msg.is_none());
        assert!(matches!(ask, Some(ClientMsg::Resync)));
        let (msg, ask) = inb.binary(&delta(4));
        assert!(msg.is_none() && ask.is_none(), "one request, not one per frame");
        let (msg, ask) = inb.binary(&[0xFF, 0, 0]);
        assert!(msg.is_none() && ask.is_none());
        // The keyframe mends it; a later fault may ask again.
        assert!(inb.binary(&key(9, &[4, 6])).0.is_some());
        assert!(inb.binary(&delta(10)).0.is_some());
        assert!(matches!(inb.binary(&delta(12)).1, Some(ClientMsg::Resync)));
    }

    #[test]
    fn the_client_asks_for_the_binary_wire() {
        let json = login_json("acct".into(), None).unwrap();
        assert!(json.contains("\"wire\":1"), "{json}");
        assert!(json.contains(&format!("\"proto\":{PROTOCOL_VERSION}")));
    }
}
