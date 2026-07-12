//! Bot swarm load harness (CHUNK C15).
//!
//! Connects N scripted players that wander/fight near the inn with a tiny
//! snapshot area-of-interest, so a single 8 GB machine can host a
//! 1,000-connection load test. Usage:
//!   antediluvia-swarm [N] [ws-url] [aoi]
//! Prints a connected-count line every 5 s and per-bot RX byte totals at exit.

use antediluvia_protocol::{ClientMsg, PROTOCOL_VERSION};
use futures_util::{SinkExt, StreamExt};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio_tungstenite::tungstenite::Message;

#[tokio::main(flavor = "multi_thread", worker_threads = 4)]
async fn main() {
    let mut args = std::env::args().skip(1);
    let n: usize = args.next().and_then(|a| a.parse().ok()).unwrap_or(100);
    let url = args.next().unwrap_or_else(|| "ws://127.0.0.1:8787".into());
    let aoi: f32 = args.next().and_then(|a| a.parse().ok()).unwrap_or(150.0);

    let connected = Arc::new(AtomicU64::new(0));
    let rx_bytes = Arc::new(AtomicU64::new(0));

    for i in 0..n {
        let url = url.clone();
        let connected = connected.clone();
        let rx_bytes = rx_bytes.clone();
        tokio::spawn(async move {
            let Ok((ws, _)) = tokio_tungstenite::connect_async(&url).await else { return };
            let (mut sink, mut stream) = ws.split();
            let send = |m: &ClientMsg| serde_json::to_string(m).unwrap();
            let _ = sink
                .send(Message::Text(
                    send(&ClientMsg::Login {
                        proto: PROTOCOL_VERSION,
                        apple_id: format!("swarm_{i}"),
                        character_name: Some(format!("Bot{i:04}")),
                        create: None,
                    })
                    .into(),
                ))
                .await;
            let _ = sink.send(Message::Text(send(&ClientMsg::SetAoi { radius: aoi }).into())).await;
            connected.fetch_add(1, Ordering::Relaxed);

            // Wander: fresh heading every second; attack sometimes.
            let mut tick = tokio::time::interval(std::time::Duration::from_millis(1000));
            let mut k: u32 = i as u32;
            loop {
                tokio::select! {
                    _ = tick.tick() => {
                        k = k.wrapping_mul(1664525).wrapping_add(1013904223);
                        let ang = (k % 6283) as f32 / 1000.0;
                        let m = if k % 7 == 0 {
                            ClientMsg::Attack
                        } else {
                            ClientMsg::Move { dx: ang.cos(), dy: ang.sin() }
                        };
                        if sink.send(Message::Text(send(&m).into())).await.is_err() { break; }
                    }
                    frame = stream.next() => match frame {
                        Some(Ok(Message::Text(t))) => { rx_bytes.fetch_add(t.len() as u64, Ordering::Relaxed); }
                        Some(Ok(_)) => {}
                        _ => break,
                    }
                }
            }
            connected.fetch_sub(1, Ordering::Relaxed);
        });
        if i % 50 == 49 {
            tokio::time::sleep(std::time::Duration::from_millis(200)).await; // ramp gently
        }
    }

    loop {
        tokio::time::sleep(std::time::Duration::from_secs(5)).await;
        println!(
            "swarm: {} connected, rx total {:.1} MB",
            connected.load(Ordering::Relaxed),
            rx_bytes.load(Ordering::Relaxed) as f64 / 1e6
        );
    }
}
