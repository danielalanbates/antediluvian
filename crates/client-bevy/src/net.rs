//! Networking bridge between Bevy (sync, main thread) and the async WebSocket
//! connection to the Antediluvia server.
//!
//! A dedicated OS thread runs a single-threaded tokio runtime that owns the
//! socket. Bevy talks to it through two tokio mpsc channels — both ends are
//! usable without a runtime: `UnboundedSender::send` and
//! `UnboundedReceiver::try_recv` are plain sync calls. The outbound sender is a
//! normal `Resource`; the inbound receiver is a `NonSend` resource (it isn't
//! `Sync`) drained on the main thread each frame.

use antediluvia_protocol::{ClientMsg, ServerMsg, PROTOCOL_VERSION};
use bevy::prelude::*;
use futures_util::{SinkExt, StreamExt};
use std::thread;
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};
use tokio_tungstenite::tungstenite::Message;

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

/// Spawn the network thread and return the Bevy-side channel ends.
pub fn start_network(url: String, apple_id: String, character_name: Option<String>) -> (NetTx, NetRx) {
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
                let login = ClientMsg::Login {
                    proto: PROTOCOL_VERSION,
                    apple_id,
                    character_name,
                    create: None,
                };
                if let Ok(txt) = serde_json::to_string(&login) {
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
