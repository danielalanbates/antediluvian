//! Antediluvia — authoritative, headless MMORPG server.
//!
//! One tokio process runs: a TCP/WebSocket acceptor, one async task per
//! connection (see `net`), and a single **game loop** that owns the whole
//! `World` and steps every zone at a fixed rate. Clients talk to the game loop
//! only through channels, so the simulation stays lock-free and deterministic.

mod db;
mod mobs;
mod net;
mod quests;
mod world;

use antediluvia_protocol::{Act, ClientMsg, ServerMsg, EntityId, PROTOCOL_VERSION};
use db::Db;
use glam::Vec2;
use std::collections::{HashMap, HashSet};
use std::time::Duration;
use tokio::net::TcpListener;
use tokio::sync::mpsc;
use world::{new_character, World, SimEvent, TICK_HZ};

/// Commands funneled from connection tasks into the single game loop.
pub enum GameCmd {
    Connect { id: u64, out: mpsc::UnboundedSender<ServerMsg> },
    Client { id: u64, msg: ClientMsg },
    Disconnect { id: u64 },
}

/// Game-loop-side record of a connected client.
struct Conn {
    out: mpsc::UnboundedSender<ServerMsg>,
    name: Option<String>,
    entity: Option<EntityId>,
    act: Act,
    logged_in: bool,
    /// Live guild membership (mirrors the sheet, for chat routing).
    guild: Option<String>,
    /// Conn id of a player who challenged us to a duel.
    pending_duel: Option<u64>,
    /// Guild we've been invited to.
    pending_guild_invite: Option<String>,
}

impl Conn {
    fn send(&self, msg: ServerMsg) {
        let _ = self.out.send(msg);
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "antediluvia_server=info".into()),
        )
        .init();

    let bind = std::env::var("ANTEDILUVIA_BIND").unwrap_or_else(|_| "127.0.0.1:8787".into());
    let db_path = std::env::var("ANTEDILUVIA_DB").unwrap_or_else(|_| "antediluvia.sqlite".into());

    let db = Db::open(&db_path)?;
    tracing::info!("persistence: {db_path}");

    let (tx, rx) = mpsc::unbounded_channel::<GameCmd>();

    // Game loop task.
    let _game = tokio::spawn(game_loop(rx, db));

    // Acceptor.
    let listener = TcpListener::bind(&bind).await?;
    tracing::info!("Antediluvia server listening on ws://{bind}  (proto v{PROTOCOL_VERSION})");
    let mut next_conn: u64 = 1;
    loop {
        let (stream, _addr) = match listener.accept().await {
            Ok(p) => p,
            Err(e) => {
                tracing::warn!("accept error: {e}");
                continue;
            }
        };
        let id = next_conn;
        next_conn += 1;
        let tx2 = tx.clone();
        tokio::spawn(net::handle_connection(id, stream, tx2));
    }

    // (loop above never returns; the game task runs for the process lifetime)
}

async fn game_loop(mut rx: mpsc::UnboundedReceiver<GameCmd>, db: Db) -> anyhow::Result<()> {
    let mut world = World::new(0xA17E_D17Eu64);
    let mut conns: HashMap<u64, Conn> = HashMap::new();
    let mut active_names: HashSet<String> = HashSet::new();

    let mut tick = tokio::time::interval(Duration::from_millis(1000 / TICK_HZ));
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    let mut ticks: u64 = 0;

    loop {
        tokio::select! {
            _ = tick.tick() => {
                let events = world.step();
                dispatch_events(&mut world, &conns, events);
                broadcast_snapshots(&world, &conns);
                ticks += 1;
                // Persist all online characters every ~10s.
                if ticks % (TICK_HZ * 10) == 0 {
                    save_all(&world, &conns, &db);
                }
            }
            maybe = rx.recv() => {
                let Some(cmd) = maybe else { break };
                handle_cmd(cmd, &mut world, &mut conns, &mut active_names, &db);
            }
        }
    }
    Ok(())
}

fn handle_cmd(
    cmd: GameCmd,
    world: &mut World,
    conns: &mut HashMap<u64, Conn>,
    active_names: &mut HashSet<String>,
    db: &Db,
) {
    match cmd {
        GameCmd::Connect { id, out } => {
            conns.insert(id, Conn {
                out,
                name: None,
                entity: None,
                act: Act::Eden,
                logged_in: false,
                guild: None,
                pending_duel: None,
                pending_guild_invite: None,
            });
        }
        GameCmd::Disconnect { id } => {
            if let Some(c) = conns.remove(&id) {
                if let (Some(ent), true) = (c.entity, c.logged_in) {
                    if let Some(mut sheet) = world.player_sheet(c.act, ent) {
                        sheet.last_logout = Some(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs());
                        if let Err(e) = db.save(&sheet, None) {
                            tracing::error!("save on disconnect: {e}");
                        }
                    }
                    world.remove_player(c.act, ent);
                }
                if let Some(n) = c.name {
                    active_names.remove(&n.to_lowercase());
                }
            }
        }
        GameCmd::Client { id, msg } => handle_client_msg(id, msg, world, conns, active_names, db),
    }
}

fn handle_client_msg(
    id: u64,
    msg: ClientMsg,
    world: &mut World,
    conns: &mut HashMap<u64, Conn>,
    active_names: &mut HashSet<String>,
    db: &Db,
) {
    match msg {
        ClientMsg::Login { proto, apple_id, character_name } => {
            let apple_id = apple_id.trim().to_string();
            let reject = |conns: &HashMap<u64, Conn>, reason: &str| {
                if let Some(c) = conns.get(&id) {
                    c.send(ServerMsg::LoginRejected { reason: reason.into() });
                }
            };
            if proto != PROTOCOL_VERSION {
                return reject(conns, "protocol version mismatch");
            }
            if conns.get(&id).map(|c| c.logged_in).unwrap_or(false) {
                return reject(conns, "already logged in");
            }

            // Load or create the character via apple_id
            let mut sheet = match db.load_by_apple_id(&apple_id) {
                Ok(Some(mut s)) => {
                    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();
                    if let Some(last) = s.last_logout {
                        let diff = now.saturating_sub(last);
                        // 8 hours (28800s) of sleep = 100% recovery
                        let recovery = diff as f32 * 0.00347;
                        s.wakefulness = (s.wakefulness + recovery).clamp(0.0, 100.0);
                    }
                    s
                },
                Ok(None) => {
                    // Account does not exist, need a character name
                    let Some(name) = character_name else {
                        return reject(conns, "New Apple account. Please provide a character name.");
                    };
                    let name = name.trim().to_string();
                    if name.is_empty() || name.len() > 24 {
                        return reject(conns, "name must be 1-24 characters");
                    }
                    if let Ok(Some(_)) = db.load(&name) {
                        return reject(conns, "character name already taken");
                    }
                    let s = new_character(&name);
                    if let Err(e) = db.save(&s, Some(&apple_id)) {
                        tracing::error!("db save new: {e}");
                        return reject(conns, "server error creating character");
                    }
                    s
                }
                Err(e) => {
                    tracing::error!("db load: {e}");
                    return reject(conns, "server error loading account");
                }
            };

            let name = sheet.name.clone();
            let key = name.to_lowercase();
            if active_names.contains(&key) {
                return reject(conns, "that character is already online");
            }

            let act = sheet.act;
            let guild = sheet.guild.clone();
            let entity_id = world.spawn_player(id, sheet.clone());
            active_names.insert(key.clone());
            if let Some(c) = conns.get_mut(&id) {
                c.name = Some(name.clone());
                c.entity = Some(entity_id);
                c.act = act;
                c.logged_in = true;
                c.guild = guild;
                c.send(ServerMsg::Welcome { entity_id, character: sheet });
            }
            tracing::info!(conn = id, %name, act = act.as_str(), "login (apple_id={apple_id})");
        }
        ClientMsg::Move { dx, dy } => {
            if let Some(c) = conns.get(&id) {
                if let Some(ent) = c.entity {
                    world.set_intent(c.act, ent, Vec2::new(dx, dy));
                }
            }
        }
        ClientMsg::Attack => {
            if let Some(c) = conns.get(&id) {
                if let Some(ent) = c.entity {
                    world.queue_attack(c.act, ent);
                }
            }
        }
        ClientMsg::Travel { act } => {
            // Move the player's persistent sheet to the new zone entry point.
            let (old_act, ent) = match conns.get(&id) {
                Some(c) if c.logged_in => (c.act, c.entity),
                _ => return,
            };
            let Some(ent) = ent else { return };
            if old_act == act {
                return;
            }
            if let Some(mut sheet) = world.player_sheet(old_act, ent) {
                world.remove_player(old_act, ent);
                sheet.act = act;
                sheet.x = 0.0;
                sheet.y = 0.0;
                let new_ent = world.spawn_player(id, sheet.clone());
                if let Some(c) = conns.get_mut(&id) {
                    c.act = act;
                    c.entity = Some(new_ent);
                    c.send(ServerMsg::Notice { text: format!("You travel to {}.", act.as_str()) });
                    c.send(ServerMsg::Stats { character: sheet });
                }
            }
        }
        ClientMsg::Chat { text } => {
            let (from, act) = match conns.get(&id) {
                Some(c) if c.logged_in => (c.name.clone().unwrap_or_default(), c.act),
                _ => return,
            };
            let text = text.chars().take(240).collect::<String>();
            for c in conns.values() {
                if c.logged_in && c.act == act {
                    c.send(ServerMsg::Chat { from: from.clone(), text: text.clone() });
                }
            }
        }
        ClientMsg::SelectClass { class } => {
            let Some((act, ent)) = conn_entity(conns, id) else { return };
            match world.select_class(act, ent, class) {
                Ok(text) | Err(text) => notice(conns, id, text),
            }
            send_stats(world, conns, id);
        }
        ClientMsg::Cast { ability } => {
            if let Some((act, ent)) = conn_entity(conns, id) {
                world.queue_cast(act, ent, ability);
            }
        }
        ClientMsg::LearnTalent { talent } => {
            let Some((act, ent)) = conn_entity(conns, id) else { return };
            match world.learn_talent(act, ent, &talent) {
                Ok(text) | Err(text) => notice(conns, id, text),
            }
            send_stats(world, conns, id);
        }
        ClientMsg::TogglePvp => {
            let Some((act, ent)) = conn_entity(conns, id) else { return };
            if let Some(on) = world.toggle_pvp(act, ent) {
                let text = if on {
                    "You are now flagged for PvP.".to_string()
                } else {
                    "PvP flag removed.".to_string()
                };
                notice(conns, id, text);
                send_stats(world, conns, id);
            }
        }
        ClientMsg::Duel { player } => {
            let Some((act, _)) = conn_entity(conns, id) else { return };
            let from = conns.get(&id).and_then(|c| c.name.clone()).unwrap_or_default();
            let target = conns.iter().find_map(|(cid, c)| {
                (*cid != id
                    && c.logged_in
                    && c.act == act
                    && c.name.as_deref().map(|n| n.eq_ignore_ascii_case(&player)) == Some(true))
                .then_some(*cid)
            });
            match target {
                Some(tid) => {
                    if let Some(t) = conns.get_mut(&tid) {
                        t.pending_duel = Some(id);
                        t.send(ServerMsg::Notice {
                            text: format!("{from} challenges you to a duel! (accept to fight)"),
                        });
                    }
                    notice(conns, id, format!("You challenge {player} to a duel."));
                }
                None => notice(conns, id, "No such player in this zone.".into()),
            }
        }
        ClientMsg::DuelAccept => {
            let challenger = match conns.get_mut(&id) {
                Some(c) if c.logged_in => c.pending_duel.take(),
                _ => return,
            };
            let Some(cid) = challenger else {
                return notice(conns, id, "No pending duel challenge.".into());
            };
            let (my_act, my_ent) = match conn_entity(conns, id) {
                Some(v) => v,
                None => return,
            };
            let their = match conns.get(&cid) {
                Some(c) if c.logged_in && c.act == my_act => c.entity,
                _ => None,
            };
            match their {
                Some(their_ent) if world.start_duel(my_act, my_ent, their_ent) => {
                    notice(conns, id, "Duel started — fight!".into());
                    notice(conns, cid, "Your duel challenge was accepted — fight!".into());
                }
                _ => notice(conns, id, "The challenger is no longer available.".into()),
            }
        }
        ClientMsg::UseItem { item } => {
            let Some((act, ent)) = conn_entity(conns, id) else { return };
            match world.use_item(act, ent, &item) {
                Ok(text) | Err(text) => notice(conns, id, text),
            }
            send_stats(world, conns, id);
        }
        ClientMsg::Equip { item } => {
            let Some((act, ent)) = conn_entity(conns, id) else { return };
            match world.equip(act, ent, &item) {
                Ok(text) | Err(text) => notice(conns, id, text),
            }
            send_stats(world, conns, id);
        }
        ClientMsg::Talk => {
            let Some((act, ent)) = conn_entity(conns, id) else { return };
            match world.talk(act, ent) {
                Ok(text) | Err(text) => notice(conns, id, text),
            }
            send_stats(world, conns, id);
        }
        ClientMsg::Craft { recipe } => {
            let Some((act, ent)) = conn_entity(conns, id) else { return };
            match world.craft(act, ent, &recipe) {
                Ok(text) | Err(text) => notice(conns, id, text),
            }
            send_stats(world, conns, id);
        }
        ClientMsg::GuildCreate { name } => {
            let gname = name.trim().to_string();
            let Some((act, ent)) = conn_entity(conns, id) else { return };
            if gname.is_empty() || gname.len() > 24 {
                return notice(conns, id, "Guild name must be 1-24 characters.".into());
            }
            if conns.get(&id).map(|c| c.guild.is_some()).unwrap_or(false) {
                return notice(conns, id, "You are already in a guild.".into());
            }
            let me = conns.get(&id).and_then(|c| c.name.clone()).unwrap_or_default();
            match db.guild_create(&gname, &me) {
                Ok(true) => {
                    world.set_guild(act, ent, Some(gname.clone()));
                    if let Some(c) = conns.get_mut(&id) {
                        c.guild = Some(gname.clone());
                    }
                    let members = db.guild_members(&gname).unwrap_or_default();
                    if let Some(c) = conns.get(&id) {
                        c.send(ServerMsg::GuildInfo { name: gname, members });
                    }
                    notice(conns, id, "Guild founded.".into());
                }
                Ok(false) => notice(conns, id, "That guild name is taken.".into()),
                Err(e) => {
                    tracing::error!("guild_create: {e}");
                    notice(conns, id, "Server error creating guild.".into());
                }
            }
        }
        ClientMsg::GuildInvite { player } => {
            let guild = match conns.get(&id) {
                Some(c) if c.logged_in => c.guild.clone(),
                _ => return,
            };
            let Some(guild) = guild else {
                return notice(conns, id, "You are not in a guild.".into());
            };
            let from = conns.get(&id).and_then(|c| c.name.clone()).unwrap_or_default();
            let target = conns.iter().find_map(|(cid, c)| {
                (*cid != id
                    && c.logged_in
                    && c.name.as_deref().map(|n| n.eq_ignore_ascii_case(&player)) == Some(true))
                .then_some(*cid)
            });
            match target {
                Some(tid) => {
                    if let Some(t) = conns.get_mut(&tid) {
                        t.pending_guild_invite = Some(guild.clone());
                        t.send(ServerMsg::Notice {
                            text: format!("{from} invites you to join <{guild}>."),
                        });
                    }
                    notice(conns, id, format!("Invited {player} to the guild."));
                }
                None => notice(conns, id, "No such player online.".into()),
            }
        }
        ClientMsg::GuildAccept => {
            let invite = match conns.get_mut(&id) {
                Some(c) if c.logged_in => c.pending_guild_invite.take(),
                _ => return,
            };
            let Some(guild) = invite else {
                return notice(conns, id, "No pending guild invite.".into());
            };
            if conns.get(&id).map(|c| c.guild.is_some()).unwrap_or(false) {
                return notice(conns, id, "You are already in a guild.".into());
            }
            let Some((act, ent)) = conn_entity(conns, id) else { return };
            let me = conns.get(&id).and_then(|c| c.name.clone()).unwrap_or_default();
            if let Err(e) = db.guild_add_member(&guild, &me) {
                tracing::error!("guild_add_member: {e}");
                return notice(conns, id, "Server error joining guild.".into());
            }
            world.set_guild(act, ent, Some(guild.clone()));
            if let Some(c) = conns.get_mut(&id) {
                c.guild = Some(guild.clone());
            }
            let members = db.guild_members(&guild).unwrap_or_default();
            if let Some(c) = conns.get(&id) {
                c.send(ServerMsg::GuildInfo { name: guild.clone(), members });
            }
            // Tell the guild.
            for c in conns.values() {
                if c.logged_in && c.guild.as_deref() == Some(guild.as_str()) {
                    c.send(ServerMsg::Notice { text: format!("{me} joined the guild.") });
                }
            }
        }
        ClientMsg::GuildLeave => {
            let (guild, me) = match conns.get(&id) {
                Some(c) if c.logged_in => (c.guild.clone(), c.name.clone().unwrap_or_default()),
                _ => return,
            };
            let Some(guild) = guild else {
                return notice(conns, id, "You are not in a guild.".into());
            };
            let _ = db.guild_remove_member(&guild, &me);
            let Some((act, ent)) = conn_entity(conns, id) else { return };
            world.set_guild(act, ent, None);
            if let Some(c) = conns.get_mut(&id) {
                c.guild = None;
            }
            notice(conns, id, format!("You left <{guild}>."));
        }
        ClientMsg::GuildChat { text } => {
            let (guild, from) = match conns.get(&id) {
                Some(c) if c.logged_in => (c.guild.clone(), c.name.clone().unwrap_or_default()),
                _ => return,
            };
            let Some(guild) = guild else {
                return notice(conns, id, "You are not in a guild.".into());
            };
            let text = text.chars().take(240).collect::<String>();
            for c in conns.values() {
                if c.logged_in && c.guild.as_deref() == Some(guild.as_str()) {
                    c.send(ServerMsg::Chat { from: format!("[G] {from}"), text: text.clone() });
                }
            }
        }
        ClientMsg::AuctionList { item, price } => {
            let Some((act, ent)) = conn_entity(conns, id) else { return };
            if !world.at_inn(act, ent) {
                return notice(conns, id, "You must be at an inn to use the auction house.".into());
            }
            if price == 0 || price > 1_000_000 {
                return notice(conns, id, "Price must be between 1 and 1,000,000 gold.".into());
            }
            if !world.take_item(act, ent, &item) {
                return notice(conns, id, "You don't have that item.".into());
            }
            let me = conns.get(&id).and_then(|c| c.name.clone()).unwrap_or_default();
            match db.auction_insert(&me, &item, price) {
                Ok(aid) => notice(conns, id, format!("Listed {item} for {price}g (lot #{aid}).")),
                Err(e) => {
                    tracing::error!("auction_insert: {e}");
                    world.give_item(act, ent, item);
                    notice(conns, id, "Server error listing item.".into());
                }
            }
            send_stats(world, conns, id);
        }
        ClientMsg::AuctionBuy { id: lot } => {
            let Some((act, ent)) = conn_entity(conns, id) else { return };
            if !world.at_inn(act, ent) {
                return notice(conns, id, "You must be at an inn to use the auction house.".into());
            }
            let listing = match db.auction_take(lot) {
                Ok(Some(l)) => l,
                Ok(None) => return notice(conns, id, "That lot is gone.".into()),
                Err(e) => {
                    tracing::error!("auction_take: {e}");
                    return notice(conns, id, "Server error.".into());
                }
            };
            if !world.try_spend_gold(act, ent, listing.price) {
                // Put the lot back.
                let _ = db.auction_insert(&listing.seller, &listing.item, listing.price);
                return notice(conns, id, "You can't afford that.".into());
            }
            if !world.give_item(act, ent, listing.item.clone()) {
                world.add_gold(act, ent, listing.price);
                let _ = db.auction_insert(&listing.seller, &listing.item, listing.price);
                return notice(conns, id, "Your bags are full.".into());
            }
            // Pay the seller — live sheet if online, saved sheet otherwise.
            let seller_conn = conns.iter().find_map(|(cid, c)| {
                (c.logged_in
                    && c.name.as_deref().map(|n| n.eq_ignore_ascii_case(&listing.seller))
                        == Some(true))
                .then_some(*cid)
            });
            match seller_conn {
                Some(scid) => {
                    if let Some((sact, sent)) = conn_entity(conns, scid) {
                        world.add_gold(sact, sent, listing.price);
                    }
                    notice(conns, scid, format!("Your {} sold for {}g.", listing.item, listing.price));
                    send_stats(world, conns, scid);
                }
                None => {
                    if let Err(e) = db.credit_gold(&listing.seller, listing.price) {
                        tracing::error!("credit_gold: {e}");
                    }
                }
            }
            notice(conns, id, format!("You bought {} for {}g.", listing.item, listing.price));
            send_stats(world, conns, id);
        }
        ClientMsg::AuctionBrowse => {
            if let Some(c) = conns.get(&id) {
                let listings = db.auction_list().unwrap_or_default();
                c.send(ServerMsg::Auctions { listings });
            }
        }
        ClientMsg::Ping => {
            if let Some(c) = conns.get(&id) {
                c.send(ServerMsg::Pong);
            }
        }
    }
}

/// (act, entity) for a logged-in connection.
fn conn_entity(conns: &HashMap<u64, Conn>, id: u64) -> Option<(Act, EntityId)> {
    let c = conns.get(&id)?;
    if !c.logged_in {
        return None;
    }
    Some((c.act, c.entity?))
}

fn notice(conns: &HashMap<u64, Conn>, id: u64, text: String) {
    if let Some(c) = conns.get(&id) {
        c.send(ServerMsg::Notice { text });
    }
}

fn send_stats(world: &World, conns: &HashMap<u64, Conn>, id: u64) {
    if let Some(c) = conns.get(&id) {
        if let Some(ent) = c.entity {
            if let Some(sheet) = world.player_sheet(c.act, ent) {
                c.send(ServerMsg::Stats { character: sheet });
            }
        }
    }
}

/// Push per-tick simulation events (level-ups, deaths, loot) to their owners,
/// along with a refreshed stat block.
fn dispatch_events(world: &mut World, conns: &HashMap<u64, Conn>, events: Vec<SimEvent>) {
    for ev in events {
        // Combat events are cosmetic and zone-wide: fan out to every client in
        // the act (same pattern as zone chat) rather than to one owner.
        if let SimEvent::Combat { act, kind, src, dst } = ev {
            for c in conns.values() {
                if c.logged_in && c.act == act {
                    c.send(ServerMsg::Event { act, kind, src, dst });
                }
            }
            continue;
        }
        let owner = match &ev {
            SimEvent::LevelUp { owner, .. } => *owner,
            SimEvent::Died { owner } => *owner,
            SimEvent::Loot { owner, .. } => *owner,
            SimEvent::Info { owner, .. } => *owner,
            SimEvent::Combat { .. } => unreachable!(),
        };
        let Some(c) = conns.get(&owner) else { continue };
        match ev {
            SimEvent::LevelUp { level, .. } => {
                c.send(ServerMsg::Notice { text: format!("You reached level {level}!") });
            }
            SimEvent::Died { .. } => {
                c.send(ServerMsg::Notice { text: "You were slain and revived at the shrine.".into() });
            }
            SimEvent::Loot { item, .. } => {
                c.send(ServerMsg::Notice { text: format!("You collected {item}.") });
            }
            SimEvent::Info { text, .. } => {
                c.send(ServerMsg::Notice { text });
            }
            SimEvent::Combat { .. } => unreachable!(),
        }
        if let Some(ent) = c.entity {
            if let Some(sheet) = world.player_sheet(c.act, ent) {
                c.send(ServerMsg::Stats { character: sheet });
            }
        }
    }
}

/// Radius (world units) a client is told about. Entities beyond this are culled
/// from that player's snapshot — the core MMO bandwidth control.
const AOI_RADIUS: f32 = 1400.0;

/// Send each logged-in client an area-of-interest snapshot centered on its own
/// player: only the entities near it, not the whole zone.
fn broadcast_snapshots(world: &World, conns: &HashMap<u64, Conn>) {
    let now_secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs();
    // 86400 seconds in a day. time_of_day = 0.0 at midnight UTC, 0.5 at noon UTC.
    let time_of_day = (now_secs % 86400) as f32 / 86400.0;

    for c in conns.values() {
        if !c.logged_in {
            continue;
        }
        let Some(ent) = c.entity else { continue };
        let (tick, entities) = match world.player_pos(c.act, ent) {
            Some(pos) => world.zone_snapshot_around(c.act, pos, AOI_RADIUS),
            None => world.zone_snapshot(c.act),
        };
        c.send(ServerMsg::Snapshot { act: c.act, tick, time_of_day, entities });
    }
}

fn save_all(world: &World, conns: &HashMap<u64, Conn>, db: &Db) {
    for c in conns.values() {
        if let (true, Some(ent)) = (c.logged_in, c.entity) {
            if let Some(sheet) = world.player_sheet(c.act, ent) {
                if let Err(e) = db.save(&sheet, None) {
                    tracing::error!("periodic save: {e}");
                }
            }
        }
    }
}
