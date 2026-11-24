// Copyright (c) 2026 Daniel Bates / Bates LLC. All rights reserved.
//! Two-connection smoke test for the beta server track (B05-B08).
//!
//! Logs two fresh characters in and exercises the new messages end to end
//! over a real WebSocket: zone banner, channels, whisper, friends, who,
//! emote, raid, flight list, gear report and the quest-board refusal.
//! Prints `TRACK-E2E-OK` when every expected reply arrived.
//!
//! Usage: antediluvia-track [ws-url] [name-suffix]
//!   default url: ws://127.0.0.1:8797

use antediluvia_protocol::{ClientMsg, ServerMsg, PROTOCOL_VERSION};
use futures_util::{SinkExt, StreamExt};
use std::time::Duration;
use tokio::net::TcpStream;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};

type Ws = WebSocketStream<MaybeTlsStream<TcpStream>>;

async fn send(ws: &mut Ws, m: ClientMsg) -> Result<(), Box<dyn std::error::Error>> {
    ws.send(Message::Text(serde_json::to_string(&m)?.into())).await?;
    Ok(())
}

/// Read until `want` matches a message (snapshots skipped) or 5 s pass.
async fn expect(ws: &mut Ws, what: &str, want: impl Fn(&ServerMsg) -> bool) -> Result<ServerMsg, Box<dyn std::error::Error>> {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    loop {
        let frame = tokio::time::timeout_at(deadline, ws.next()).await.map_err(|_| format!("timed out waiting for {what}"))?;
        let Some(frame) = frame else { return Err(format!("socket closed waiting for {what}").into()) };
        let Message::Text(txt) = frame? else { continue };
        let Ok(msg) = serde_json::from_str::<ServerMsg>(&txt) else { continue };
        if matches!(msg, ServerMsg::Snapshot { .. } | ServerMsg::Event { .. }) {
            continue;
        }
        if want(&msg) {
            println!("ok   {what}: {}", serde_json::to_string(&msg)?.chars().take(160).collect::<String>());
            return Ok(msg);
        }
    }
}

async fn login(url: &str, name: &str) -> Result<Ws, Box<dyn std::error::Error>> {
    let (mut ws, _) = tokio_tungstenite::connect_async(url).await?;
    send(&mut ws, ClientMsg::Login {
        proto: PROTOCOL_VERSION,
        apple_id: format!("track_{name}"),
        character_name: Some(name.to_string()),
        create: None,
    }).await?;
    expect(&mut ws, "welcome", |m| matches!(m, ServerMsg::Welcome { .. })).await?;
    expect(&mut ws, "zone banner", |m| matches!(m, ServerMsg::ZoneBanner { .. })).await?;
    Ok(ws)
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let mut args = std::env::args().skip(1);
    let url = args.next().unwrap_or_else(|| "ws://127.0.0.1:8797".into());
    let sfx = args.next().unwrap_or_else(|| "A".into());
    let (an, bn) = (format!("Tubal{sfx}"), format!("Naamah{sfx}"));
    let mut a = login(&url, &an).await?;
    let mut b = login(&url, &bn).await?;

    send(&mut a, ClientMsg::ChannelList).await?;
    expect(&mut a, "channels", |m| matches!(m, ServerMsg::Channels { joined, .. } if joined.len() == 3)).await?;
    send(&mut a, ClientMsg::ChannelSay { channel: "general".into(), text: "hail".into() }).await?;
    expect(&mut b, "general chat", |m| matches!(m, ServerMsg::ChannelMsg { channel, text, .. } if channel == "general" && text == "hail")).await?;
    send(&mut a, ClientMsg::Whisper { to: bn.clone(), text: "psst".into() }).await?;
    expect(&mut b, "whisper", |m| matches!(m, ServerMsg::Whisper { text, .. } if text == "psst")).await?;
    send(&mut a, ClientMsg::FriendAdd { player: bn.clone() }).await?;
    expect(&mut a, "friends", |m| matches!(m, ServerMsg::Friends { friends, .. } if friends.len() == 1 && friends[0].online)).await?;
    send(&mut a, ClientMsg::Who { filter: None }).await?;
    expect(&mut a, "who", |m| matches!(m, ServerMsg::WhoList { total, .. } if *total >= 2)).await?;
    send(&mut a, ClientMsg::Emote { emote: "wave".into(), target: Some(bn.clone()) }).await?;
    expect(&mut b, "emote", |m| matches!(m, ServerMsg::Emote { emote, .. } if emote == "wave")).await?;
    // Ignore: the next whisper must be refused.
    send(&mut b, ClientMsg::IgnoreAdd { player: an.clone() }).await?;
    expect(&mut b, "ignore list", |m| matches!(m, ServerMsg::Friends { ignored, .. } if ignored.len() == 1)).await?;
    send(&mut a, ClientMsg::Whisper { to: bn.clone(), text: "again".into() }).await?;
    expect(&mut a, "ignored notice", |m| matches!(m, ServerMsg::Notice { text } if text.contains("ignoring you"))).await?;
    send(&mut b, ClientMsg::IgnoreRemove { player: an.clone() }).await?;
    // Raid of two.
    send(&mut a, ClientMsg::RaidCreate).await?;
    send(&mut a, ClientMsg::RaidInvite { player: bn.clone() }).await?;
    expect(&mut b, "raid invite", |m| matches!(m, ServerMsg::Notice { text } if text.contains("invites you to a raid"))).await?;
    send(&mut b, ClientMsg::RaidAccept).await?;
    expect(&mut b, "raid roster", |m| matches!(m, ServerMsg::RaidInfo { members, .. } if members.len() == 2)).await?;
    // Travel, items, boards.
    send(&mut a, ClientMsg::FlightList).await?;
    expect(&mut a, "flight paths", |m| matches!(m, ServerMsg::FlightPaths { nodes, .. } if nodes.len() == 25)).await?;
    send(&mut a, ClientMsg::GearInfo).await?;
    expect(&mut a, "gear", |m| matches!(m, ServerMsg::Gear { items, .. } if items.iter().any(|i| i.item == "animal_skins"))).await?;
    send(&mut a, ClientMsg::QuestBoard).await?;
    expect(&mut a, "no board at the gate", |m| matches!(m, ServerMsg::Notice { text } if text.contains("no notice board"))).await?;
    send(&mut a, ClientMsg::ReleaseSpirit).await?;
    expect(&mut a, "release refused alive", |m| matches!(m, ServerMsg::Notice { text } if text.contains("not dead"))).await?;
    send(&mut a, ClientMsg::Repair).await?;
    expect(&mut a, "repair needs the inn", |m| matches!(m, ServerMsg::Notice { text } if text.contains("only at the inn"))).await?;
    println!("TRACK-E2E-OK");
    let _ = a.close(None).await;
    let _ = b.close(None).await;
    Ok(())
}
