//! Bot swarm load harness (CHUNK C15, reworked for B11).
//!
//! Connects N scripted players that gather around their spawn point (the
//! Eden gate for new characters) and keep walking out and back, so a single
//! 8 GB machine can host a 1,000-connection crowd test. Usage:
//!   antediluvia-swarm [N] [ws-url] [aoi] [json|bin] [gather-radius] [first] [total]
//! `first` and `total` let a crowd be grown in steps while one client
//! watches: `200 .. 0 1000`, then `300 .. 200 1000`, then `500 .. 500 1000`
//! fill one disc from the centre outwards with bots 0..1000.
//! `json` (default) is the old text wire with a small AoI; `bin` asks for
//! binary delta snapshots, the wire the game client uses: run it with the
//! full AoI (1400) to load the server the way real players would.
//! Every 5 s it prints the connected count and the downlink per bot. The
//! first `SWARM_DECODERS` bots (default 20) decode every frame and report
//! how many entities they hold; a frame that fails to decode is counted
//! and answered with `Resync`, as the game client does.

use antediluvia_protocol::wire::SnapshotDecoder;
use antediluvia_protocol::{ClientMsg, PROTOCOL_VERSION};
use futures_util::{SinkExt, StreamExt};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio_tungstenite::tungstenite::Message;

/// Player walking speed in world units per second (sim `PLAYER_SPEED`).
const SPEED: f32 = 180.0;

#[tokio::main(flavor = "multi_thread", worker_threads = 4)]
async fn main() {
    let mut args = std::env::args().skip(1);
    let n: usize = args.next().and_then(|a| a.parse().ok()).unwrap_or(100);
    let url = args.next().unwrap_or_else(|| "ws://127.0.0.1:8787".into());
    let aoi: f32 = args.next().and_then(|a| a.parse().ok()).unwrap_or(150.0);
    let binary = args.next().is_some_and(|a| a.eq_ignore_ascii_case("bin"));
    let gather: f32 = args.next().and_then(|a| a.parse().ok()).unwrap_or(500.0);
    let first: usize = args.next().and_then(|a| a.parse().ok()).unwrap_or(0);
    let total: usize = args.next().and_then(|a| a.parse().ok()).unwrap_or(first + n).max(1);
    let decoders: usize = std::env::var("SWARM_DECODERS").ok().and_then(|v| v.parse().ok()).unwrap_or(20);

    let connected = Arc::new(AtomicU64::new(0));
    let rx_bytes = Arc::new(AtomicU64::new(0));
    let rx_frames = Arc::new(AtomicU64::new(0));
    let held_sum = Arc::new(AtomicU64::new(0));
    let held_n = Arc::new(AtomicU64::new(0));
    let bad_frames = Arc::new(AtomicU64::new(0));

    for i in first..first + n {
        let url = url.clone();
        let connected = connected.clone();
        let rx_bytes = rx_bytes.clone();
        let rx_frames = rx_frames.clone();
        let held_sum = held_sum.clone();
        let held_n = held_n.clone();
        let bad_frames = bad_frames.clone();
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
                        wire: binary as u8,
                    })
                    .into(),
                ))
                .await;
            let _ = sink.send(Message::Text(send(&ClientMsg::SetAoi { radius: aoi }).into())).await;
            connected.fetch_add(1, Ordering::Relaxed);
            let mut dec = (binary && i - first < decoders).then(SnapshotDecoder::new);
            let mut asked = false;

            // Walk out to a place on an even disc around the spawn point
            // (golden-angle spiral), then pace: a fresh heading for one
            // second, the way back for the next. Bots stay gathered without
            // needing to know where they are.
            let out_angle = i as f32 * 2.399_963;
            let out_secs = gather * ((i as f32 + 0.5) / total as f32).sqrt() / SPEED;
            let _ = sink
                .send(Message::Text(send(&ClientMsg::Move { dx: out_angle.cos(), dy: out_angle.sin() }).into()))
                .await;
            let start = tokio::time::Instant::now() + std::time::Duration::from_secs_f32(out_secs.max(0.05));
            let mut tick = tokio::time::interval_at(start, std::time::Duration::from_millis(1000));
            let mut k: u32 = i as u32;
            let mut step: u32 = 0;
            let mut heading = 0.0f32;
            loop {
                tokio::select! {
                    _ = tick.tick() => {
                        if step % 2 == 0 {
                            k = k.wrapping_mul(1664525).wrapping_add(1013904223);
                            heading = (k >> 8) as f32 / (1u32 << 24) as f32 * std::f32::consts::TAU;
                        } else {
                            heading += std::f32::consts::PI;
                        }
                        step += 1;
                        let m = ClientMsg::Move { dx: heading.cos(), dy: heading.sin() };
                        if sink.send(Message::Text(send(&m).into())).await.is_err() { break; }
                        if k % 7 == 0 && step % 2 == 0 {
                            if sink.send(Message::Text(send(&ClientMsg::Attack).into())).await.is_err() { break; }
                        }
                        if let Some(d) = &dec {
                            held_sum.fetch_add(d.len() as u64, Ordering::Relaxed);
                            held_n.fetch_add(1, Ordering::Relaxed);
                        }
                    }
                    frame = stream.next() => match frame {
                        Some(Ok(Message::Text(t))) => {
                            rx_bytes.fetch_add(t.len() as u64, Ordering::Relaxed);
                            rx_frames.fetch_add(1, Ordering::Relaxed);
                        }
                        Some(Ok(Message::Binary(b))) => {
                            rx_bytes.fetch_add(b.len() as u64, Ordering::Relaxed);
                            rx_frames.fetch_add(1, Ordering::Relaxed);
                            if let Some(d) = dec.as_mut() {
                                match d.apply(&b) {
                                    Ok(st) => { if st.key { asked = false; } }
                                    Err(_) => {
                                        bad_frames.fetch_add(1, Ordering::Relaxed);
                                        if !asked {
                                            asked = true;
                                            if sink.send(Message::Text(send(&ClientMsg::Resync).into())).await.is_err() { break; }
                                        }
                                    }
                                }
                            }
                        }
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

    let mut last_bytes = 0u64;
    let mut last_frames = 0u64;
    loop {
        tokio::time::sleep(std::time::Duration::from_secs(5)).await;
        let c = connected.load(Ordering::Relaxed);
        let b = rx_bytes.load(Ordering::Relaxed);
        let f = rx_frames.load(Ordering::Relaxed);
        let hn = held_n.swap(0, Ordering::Relaxed);
        let hs = held_sum.swap(0, Ordering::Relaxed);
        println!(
            "swarm: {} connected ({}), rx total {:.1} MB, {} B/s per bot, {:.1} frames/s per bot, decoders hold {} entities avg, {} bad frames",
            c,
            if binary { "bin" } else { "json" },
            b as f64 / 1e6,
            (b - last_bytes) / 5 / c.max(1),
            (f - last_frames) as f64 / 5.0 / c.max(1) as f64,
            if hn > 0 { hs / hn } else { 0 },
            bad_frames.load(Ordering::Relaxed),
        );
        last_bytes = b;
        last_frames = f;
    }
}
