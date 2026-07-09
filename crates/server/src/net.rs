//! WebSocket connection handling. Each accepted socket becomes one async task
//! that (a) forwards decoded `ClientMsg`s into the game loop and (b) drains an
//! mpsc of `ServerMsg`s back out to the socket. The game loop never touches a
//! socket directly — it only sees channels.

use crate::GameCmd;
use antediluvia_protocol::{ClientMsg, ServerMsg};
use futures_util::{SinkExt, StreamExt};
use tokio::net::TcpStream;
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message;

pub async fn handle_connection(id: u64, stream: TcpStream, game: mpsc::UnboundedSender<GameCmd>) {
    let peer = stream.peer_addr().ok();
    let ws = match tokio_tungstenite::accept_async(stream).await {
        Ok(ws) => ws,
        Err(e) => {
            tracing::warn!(?peer, "websocket handshake failed: {e}");
            return;
        }
    };
    tracing::info!(conn = id, ?peer, "client connected");
    let (mut write, mut read) = ws.split();

    // Channel the game loop uses to push messages to this client.
    let (out_tx, mut out_rx) = mpsc::unbounded_channel::<ServerMsg>();
    if game.send(GameCmd::Connect { id, out: out_tx }).is_err() {
        return;
    }

    // Writer task: serialize ServerMsgs to the socket.
    let writer = tokio::spawn(async move {
        while let Some(msg) = out_rx.recv().await {
            let txt = match serde_json::to_string(&msg) {
                Ok(t) => t,
                Err(e) => {
                    tracing::error!("serialize ServerMsg: {e}");
                    continue;
                }
            };
            if write.send(Message::Text(txt.into())).await.is_err() {
                break;
            }
        }
        let _ = write.close().await;
    });

    // Reader loop: decode ClientMsgs and forward to the game loop.
    while let Some(frame) = read.next().await {
        match frame {
            Ok(Message::Text(txt)) => match serde_json::from_str::<ClientMsg>(&txt) {
                Ok(msg) => {
                    if game.send(GameCmd::Client { id, msg }).is_err() {
                        break;
                    }
                }
                Err(e) => tracing::debug!(conn = id, "bad client frame: {e}"),
            },
            Ok(Message::Close(_)) => break,
            Ok(Message::Ping(_)) | Ok(Message::Pong(_)) => {}
            Ok(_) => {}
            Err(e) => {
                tracing::debug!(conn = id, "read error: {e}");
                break;
            }
        }
    }

    let _ = game.send(GameCmd::Disconnect { id });
    writer.abort();
    tracing::info!(conn = id, "client disconnected");
}
