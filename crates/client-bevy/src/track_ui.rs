// Copyright (c) 2026 Daniel Bates / Bates LLC. All rights reserved.
//! Text-level client binding for the beta server track (B05-B08): the new
//! server messages print into the chat log (and the zone-in banner), and a
//! set of slash commands sends the new client messages. Windows (board,
//! raid frames, flight map, friends panel) are a later job; this makes every
//! feature reachable and visible in the meantime.

use antediluvia_protocol::{ClientMsg, ServerMsg};

use crate::net::NetTx;
use crate::Session;

fn log(session: &mut Session, line: String) {
    if session.chat_log.len() >= crate::CHAT_KEEP {
        session.chat_log.pop_front();
    }
    session.chat_log.push_back(line);
}

/// Show one of the track's server messages. Unknown messages are ignored.
pub(crate) fn on_server_msg(msg: ServerMsg, session: &mut Session) {
    match msg {
        ServerMsg::ZoneBanner { name, subtitle, level_min, level_max, pvp, .. } => {
            let mut line = name;
            if !subtitle.is_empty() {
                line.push_str(&format!(" - {subtitle}"));
            }
            if level_max > 0 {
                line.push_str(&format!("  (levels {level_min}-{level_max})"));
            }
            if pvp {
                line.push_str("  [PvP]");
            }
            session.banner = Some((line, 5.0));
        }
        ServerMsg::ChannelMsg { channel, from, text } => log(session, format!("[{channel}] {from}: {text}")),
        ServerMsg::Whisper { from, to, text } => {
            let line = if from == session.name { format!("To {to}: {text}") } else { format!("{from} whispers: {text}") };
            log(session, line);
        }
        ServerMsg::Emote { text, .. } => log(session, text),
        ServerMsg::RaidInfo { leader, members } => {
            let line = if members.is_empty() {
                "You are not in a raid.".to_string()
            } else {
                format!("Raid ({}), leader {leader}: {}", members.len(), members.join(", "))
            };
            log(session, line);
        }
        ServerMsg::Channels { joined, available } => {
            log(session, format!("Channels joined: {}. Available: {}", joined.join(", "), available.join(", ")));
        }
        ServerMsg::Friends { friends, ignored } => {
            let f: Vec<String> = friends
                .iter()
                .map(|f| if f.online { format!("{} (lv {}, online)", f.name, f.level) } else { format!("{} (offline)", f.name) })
                .collect();
            log(session, format!("Friends: {}", if f.is_empty() { "none".into() } else { f.join(", ") }));
            if !ignored.is_empty() {
                log(session, format!("Ignoring: {}", ignored.join(", ")));
            }
        }
        ServerMsg::WhoList { players, total } => {
            log(session, format!("{total} online:"));
            for p in players.iter().take(8) {
                log(session, format!("  {} - lv {} {} ({:?})", p.name, p.level, p.class.as_deref().unwrap_or("classless"), p.act));
            }
        }
        ServerMsg::QuestBoard { place, quests } => {
            log(session, format!("Notice board - {place} ({} quests; /accept <id>, /turnin <id>):", quests.len()));
            for q in quests.iter().take(8) {
                log(session, format!("  {} [{}] {} (lv {}, {} xp)", q.id, q.status, q.title, q.min_level, q.xp));
            }
        }
        ServerMsg::FlightPaths { here, nodes } => {
            log(session, format!("Flight master{} (/fly <id>):", here.map(|h| format!(" at {h}")).unwrap_or_default()));
            for n in nodes.iter().filter(|n| n.known).take(8) {
                log(session, format!("  {} - {} ({} gold)", n.id, n.name, n.cost));
            }
        }
        ServerMsg::Gear { items, repair_cost } => {
            for i in items.iter().filter(|i| i.max_durability > 0).take(8) {
                log(session, format!("  {} [{}] {}/{}", i.item, i.rarity.as_str(), i.durability, i.max_durability));
            }
            log(session, format!("Repair cost: {repair_cost} gold (/repair at an inn)"));
        }
        _ => {}
    }
}

/// Slash commands for the track. Returns true when `text` was one of them.
pub(crate) fn slash(text: &str, tx: &NetTx) -> bool {
    let (cmd, rest) = match text.split_once(' ') {
        Some((c, r)) => (c, r.trim()),
        None => (text, ""),
    };
    let arg = || rest.to_string();
    let msg = match (cmd, rest.is_empty()) {
        ("/board", true) => ClientMsg::QuestBoard,
        ("/accept", false) => ClientMsg::QuestAccept { id: arg() },
        ("/turnin", false) => ClientMsg::QuestTurnIn { id: arg() },
        ("/abandon", false) => ClientMsg::QuestAbandon { id: arg() },
        ("/raid", true) => ClientMsg::RaidCreate,
        ("/rinvite", false) => ClientMsg::RaidInvite { player: arg() },
        ("/raccept", true) => ClientMsg::RaidAccept,
        ("/rleave", true) => ClientMsg::RaidLeave,
        ("/rkick", false) => ClientMsg::RaidKick { player: arg() },
        ("/ra", false) => ClientMsg::RaidChat { text: arg() },
        ("/join", false) => ClientMsg::ChannelJoin { channel: arg() },
        ("/leave", false) => ClientMsg::ChannelLeave { channel: arg() },
        ("/channels", true) => ClientMsg::ChannelList,
        ("/general" | "/trade" | "/lfg" | "/zone", false) => {
            ClientMsg::ChannelSay { channel: cmd[1..].to_string(), text: arg() }
        }
        ("/w" | "/whisper", false) => {
            let Some((to, body)) = rest.split_once(' ') else { return true };
            ClientMsg::Whisper { to: to.to_string(), text: body.trim().to_string() }
        }
        ("/friend", false) => ClientMsg::FriendAdd { player: arg() },
        ("/unfriend", false) => ClientMsg::FriendRemove { player: arg() },
        ("/ignore", false) => ClientMsg::IgnoreAdd { player: arg() },
        ("/unignore", false) => ClientMsg::IgnoreRemove { player: arg() },
        ("/friends", true) => ClientMsg::FriendList,
        ("/e" | "/emote", false) => {
            let (emote, target) = match rest.split_once(' ') {
                Some((e, t)) => (e.to_string(), Some(t.trim().to_string())),
                None => (arg(), None),
            };
            ClientMsg::Emote { emote, target }
        }
        ("/who", _) => ClientMsg::Who { filter: (!rest.is_empty()).then(arg) },
        ("/flights", true) => ClientMsg::FlightList,
        ("/fly", false) => ClientMsg::FlightTake { node: arg() },
        ("/release", true) => ClientMsg::ReleaseSpirit,
        ("/repair", true) => ClientMsg::Repair,
        ("/gear", true) => ClientMsg::GearInfo,
        _ => return false,
    };
    tx.send(msg);
    true
}
