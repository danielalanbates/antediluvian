//! Antediluvia — authoritative, headless MMORPG server.
//!
//! One tokio process runs: a TCP/WebSocket acceptor, one async task per
//! connection (see `net`), and a single **game loop** that owns the whole
//! `World` and steps every zone at a fixed rate. Clients talk to the game loop
//! only through channels, so the simulation stays lock-free and deterministic.

mod db;
mod net;
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
            conns.insert(id, Conn { out, name: None, entity: None, act: Act::Eden, logged_in: false });
        }
        GameCmd::Disconnect { id } => {
            if let Some(c) = conns.remove(&id) {
                if let (Some(ent), true) = (c.entity, c.logged_in) {
                    if let Some(sheet) = world.player_sheet(c.act, ent) {
                        if let Err(e) = db.save(&sheet) {
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
        ClientMsg::Login { proto, name } => {
            let name = name.trim().to_string();
            let key = name.to_lowercase();
            let reject = |conns: &HashMap<u64, Conn>, reason: &str| {
                if let Some(c) = conns.get(&id) {
                    c.send(ServerMsg::LoginRejected { reason: reason.into() });
                }
            };
            if proto != PROTOCOL_VERSION {
                return reject(conns, "protocol version mismatch");
            }
            if name.is_empty() || name.len() > 24 {
                return reject(conns, "name must be 1-24 characters");
            }
            if conns.get(&id).map(|c| c.logged_in).unwrap_or(false) {
                return reject(conns, "already logged in");
            }
            if active_names.contains(&key) {
                return reject(conns, "that name is already online");
            }
            // Load or create the character.
            let sheet = match db.load(&name) {
                Ok(Some(s)) => s,
                Ok(None) => {
                    let s = new_character(&name);
                    let _ = db.save(&s);
                    s
                }
                Err(e) => {
                    tracing::error!("db load: {e}");
                    return reject(conns, "server error loading character");
                }
            };
            let act = sheet.act;
            let entity_id = world.spawn_player(id, sheet.clone());
            active_names.insert(key.clone());
            if let Some(c) = conns.get_mut(&id) {
                c.name = Some(name.clone());
                c.entity = Some(entity_id);
                c.act = act;
                c.logged_in = true;
                c.send(ServerMsg::Welcome { entity_id, character: sheet });
            }
            tracing::info!(conn = id, %name, act = act.as_str(), "login");
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
        ClientMsg::Ping => {
            if let Some(c) = conns.get(&id) {
                c.send(ServerMsg::Pong);
            }
        }
    }
}

/// Push per-tick simulation events (level-ups, deaths, loot) to their owners,
/// along with a refreshed stat block.
fn dispatch_events(world: &mut World, conns: &HashMap<u64, Conn>, events: Vec<SimEvent>) {
    for ev in events {
        let owner = match &ev {
            SimEvent::LevelUp { owner, .. } => *owner,
            SimEvent::Died { owner } => *owner,
            SimEvent::Loot { owner, .. } => *owner,
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
    for c in conns.values() {
        if !c.logged_in {
            continue;
        }
        let Some(ent) = c.entity else { continue };
        let (tick, entities) = match world.player_pos(c.act, ent) {
            Some(pos) => world.zone_snapshot_around(c.act, pos, AOI_RADIUS),
            None => world.zone_snapshot(c.act),
        };
        c.send(ServerMsg::Snapshot { act: c.act, tick, entities });
    }
}

fn save_all(world: &World, conns: &HashMap<u64, Conn>, db: &Db) {
    for c in conns.values() {
        if let (true, Some(ent)) = (c.logged_in, c.entity) {
            if let Some(sheet) = world.player_sheet(c.act, ent) {
                if let Err(e) = db.save(&sheet) {
                    tracing::error!("periodic save: {e}");
                }
            }
        }
    }
}
