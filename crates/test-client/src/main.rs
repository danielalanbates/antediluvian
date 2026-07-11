//! Thin headless test client for the Antediluvia server.
//!
//! Connects, logs in, then walks toward the nearest enemy and attacks it,
//! printing server notices and periodic world summaries. Enough to prove the
//! authoritative loop end-to-end without a graphical client.
//!
//! Usage: antediluvia-client [name] [ws-url]
//!   defaults: name="Adam", url="ws://127.0.0.1:8787"

use antediluvia_protocol::{ClientMsg, EntityKind, ServerMsg, PROTOCOL_VERSION};
use futures_util::{SinkExt, StreamExt};
use tokio_tungstenite::tungstenite::Message;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let name = args.next().unwrap_or_else(|| "Adam".into());
    let url = args.next().unwrap_or_else(|| "ws://127.0.0.1:8787".into());

    let (ws, _) = tokio_tungstenite::connect_async(&url).await?;
    println!("connected to {url} as {name}");
    let (mut write, mut read) = ws.split();

    let send = |m: &ClientMsg| serde_json::to_string(m).unwrap();
    write
        .send(Message::Text(send(&ClientMsg::Login {
            proto: PROTOCOL_VERSION,
            apple_id: format!("test_{name}"),
            character_name: Some(name.clone()),
        }).into()))
        .await?;

    let mut my_id: Option<u64> = None;
    let mut my_pos = (0.0f32, 0.0f32);
    let mut frames = 0u64;

    // Quest E2E mode (ANTEDILUVIA_QUEST=1): walk the Eden main quest chain
    // end-to-end — (giver position, mob tag to hunt, "n/n" done marker).
    // Prints QUEST-E2E-OK after every chain step turn-in.
    let quest_mode = std::env::var("ANTEDILUVIA_QUEST").is_ok();
    const CHAIN: &[((f32, f32), &str, &str)] = &[
        ((90.0, 0.0), "serpent", "5/5"),      // serpents_in_the_garden (elder)
        ((-70.0, 110.0), "cainite", "8/8"),   // altar_of_the_firstborn (wanderer)
        ((90.0, 0.0), "ember_wisp", "5/5"),   // the_flaming_sword (elder, collect)
    ];
    let mut chain_i = 0usize;
    let mut quest_ready = false;
    let mut talked = false;
    let mut last_talk_frame = 0u64;

    while let Some(frame) = read.next().await {
        let Message::Text(txt) = frame? else { continue };
        let msg: ServerMsg = match serde_json::from_str(&txt) {
            Ok(m) => m,
            Err(_) => continue,
        };
        match msg {
            ServerMsg::Welcome { entity_id, character } => {
                my_id = Some(entity_id);
                my_pos = (character.x, character.y);
                println!(
                    "WELCOME {} — lvl {} in {} at ({:.0},{:.0}) hp {}/{}",
                    character.name, character.level, character.act.as_str(),
                    character.x, character.y, character.health, character.max_health
                );
            }
            ServerMsg::LoginRejected { reason } => {
                println!("LOGIN REJECTED: {reason}");
                break;
            }
            ServerMsg::Notice { text } => {
                println!("[notice] {text}");
                if quest_mode && chain_i < CHAIN.len() {
                    if text.contains("(0/") {
                        talked = true; // quest accepted
                    }
                    if text.contains(CHAIN[chain_i].2) {
                        quest_ready = true;
                    }
                    if text.contains("It is done") {
                        println!("QUEST-E2E-OK step {}", chain_i + 1);
                        chain_i += 1;
                        talked = false;
                        quest_ready = false;
                        if chain_i == CHAIN.len() {
                            println!("CHAIN-E2E-OK");
                            break;
                        }
                    }
                }
            }
            ServerMsg::Event { .. } => {}
            ServerMsg::Chat { from, text } => println!("[chat] {from}: {text}"),
            ServerMsg::Stats { character } => {
                println!(
                    "[stats] lvl {} xp {}/{} hp {}/{} inv {}",
                    character.level, character.xp, character.max_xp,
                    character.health, character.max_health, character.inventory.len()
                );
            }
            ServerMsg::Snapshot { tick, entities, .. } => {
                frames += 1;
                // Locate self and nearest enemy.
                let me = entities.iter().find(|e| Some(e.id) == my_id);
                if let Some(me) = me {
                    my_pos = (me.x, me.y);
                }
                let nearest_enemy = entities
                    .iter()
                    .filter(|e| e.kind == EntityKind::Enemy && e.health > 0)
                    .min_by(|a, b| {
                        let da = (a.x - my_pos.0).hypot(a.y - my_pos.1);
                        let db = (b.x - my_pos.0).hypot(b.y - my_pos.1);
                        da.partial_cmp(&db).unwrap()
                    });

                if frames % 20 == 0 {
                    let counts = (
                        entities.iter().filter(|e| e.kind == EntityKind::Player).count(),
                        entities.iter().filter(|e| e.kind == EntityKind::Enemy).count(),
                        entities.iter().filter(|e| e.kind == EntityKind::Wildlife).count(),
                    );
                    println!(
                        "tick {tick}: {} entities (players {}, enemies {}, wildlife {}) — me@({:.0},{:.0})",
                        entities.len(), counts.0, counts.1, counts.2, my_pos.0, my_pos.1
                    );
                }

                // Quest mode: first walk to the giver and accept; once the
                // objective is complete, walk back and turn in.
                if quest_mode && (!talked || quest_ready) {
                    let giver = CHAIN[chain_i.min(CHAIN.len() - 1)].0;
                    let (dx, dy) = (giver.0 - my_pos.0, giver.1 - my_pos.1);
                    if dx.hypot(dy) > 80.0 {
                        write.send(Message::Text(send(&ClientMsg::Move { dx, dy }).into())).await?;
                    } else if frames.saturating_sub(last_talk_frame) >= 20 {
                        write.send(Message::Text(send(&ClientMsg::Move { dx: 0.0, dy: 0.0 }).into())).await?;
                        write.send(Message::Text(send(&ClientMsg::Talk).into())).await?;
                        last_talk_frame = frames;
                    }
                    if frames >= 12000 {
                        println!("QUEST-E2E-TIMEOUT");
                        break;
                    }
                    continue;
                }

                // Drive toward + attack the nearest enemy (in quest mode,
                // only mobs matching the current objective).
                let nearest_enemy = if quest_mode {
                    let want = CHAIN[chain_i.min(CHAIN.len() - 1)].1;
                    entities
                        .iter()
                        .filter(|e| e.kind == EntityKind::Enemy && e.health > 0)
                        .filter(|e| e.tag.as_deref().map(|t| t.starts_with(want)).unwrap_or(false))
                        .min_by(|a, b| {
                            let da = (a.x - my_pos.0).hypot(a.y - my_pos.1);
                            let db = (b.x - my_pos.0).hypot(b.y - my_pos.1);
                            da.partial_cmp(&db).unwrap()
                        })
                } else {
                    nearest_enemy
                };
                if let Some(en) = nearest_enemy {
                    let (dx, dy) = (en.x - my_pos.0, en.y - my_pos.1);
                    let dist = dx.hypot(dy);
                    if dist > 60.0 {
                        write
                            .send(Message::Text(send(&ClientMsg::Move { dx, dy }).into()))
                            .await?;
                    } else {
                        write.send(Message::Text(send(&ClientMsg::Move { dx: 0.0, dy: 0.0 }).into())).await?;
                        write.send(Message::Text(send(&ClientMsg::Attack).into())).await?;
                    }
                }

                // Bot runs for a bounded number of frames then leaves cleanly.
                if quest_mode && frames >= 12000 {
                    println!("QUEST-E2E-TIMEOUT");
                    break;
                }
                let frame_cap: u64 = std::env::var("ANTEDILUVIA_FRAMES")
                    .ok().and_then(|v| v.parse().ok()).unwrap_or(400);
                if !quest_mode && frames >= frame_cap {
                    println!("test run complete ({frames} frames); disconnecting");
                    break;
                }
            }
            ServerMsg::GuildInfo { name, members } => {
                println!("[guild] <{name}> members: {}", members.join(", "));
            }
            ServerMsg::Auctions { listings } => {
                for l in listings {
                    println!("[ah] #{} {} — {}g (seller {})", l.id, l.item, l.price, l.seller);
                }
            }
            ServerMsg::Pong => {}
        }
    }
    let _ = write.close().await;
    Ok(())
}
