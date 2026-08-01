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

/// The `Login` every connection opens with, pre-serialized.
fn login_json(apple_id: String, character_name: Option<String>) -> Option<String> {
    serde_json::to_string(&ClientMsg::Login {
        proto: PROTOCOL_VERSION,
        apple_id,
        character_name,
        create: None,
    })
    .ok()
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
        apple_id: String,
        character_name: Option<String>,
    ) -> (NetTx, NetRx) {
        install_crypto_provider();
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
                            return;
                        }
                    };
                    println!("[net] connected to {url}");
                    let (mut sink, mut stream) = ws.split();

                    // Authenticate immediately.
                    if let Some(txt) = login_json(apple_id, character_name) {
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
                                    if let Ok(msg) = serde_json::from_str::<ServerMsg>(&txt) {
                                        if tx_server.send(msg).is_err() {
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
        apple_id: String,
        character_name: Option<String>,
    ) -> (NetTx, NetRx) {
        let (tx_client, mut rx_client) = unbounded_channel::<ClientMsg>();
        let (tx_server, rx_server) = unbounded_channel::<ServerMsg>();

        let ws = match WebSocket::new(&url) {
            Ok(ws) => ws,
            Err(e) => {
                // Nothing to connect to: hand back live-but-silent channels so
                // the app still boots and can show the failure in its UI.
                error!("[net] could not open {url}: {e:?}");
                return (NetTx(tx_client), NetRx(rx_server));
            }
        };
        // The protocol is JSON text; never hand us ArrayBuffers.
        ws.set_binary_type(web_sys::BinaryType::Arraybuffer);

        let pending: Pending = Rc::new(RefCell::new(Vec::new()));
        // Authenticate as soon as the socket is writable.
        if let Some(txt) = login_json(apple_id, character_name) {
            pending.borrow_mut().push(txt);
        }

        // --- inbound: browser callback -> Bevy channel ---
        {
            let tx_server = tx_server.clone();
            let on_message = Closure::<dyn FnMut(MessageEvent)>::new(move |e: MessageEvent| {
                if let Some(txt) = e.data().as_string() {
                    match serde_json::from_str::<ServerMsg>(&txt) {
                        Ok(msg) => {
                            let _ = tx_server.send(msg);
                        }
                        Err(err) => warn!("[net] undecodable frame: {err}"),
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
