//! Antediluvia — authoritative, headless MMORPG server.
//!
//! One tokio process runs: a TCP/WebSocket acceptor, one async task per
//! connection (see `net`), and a single **game loop** that owns the whole
//! `World` and steps every zone at a fixed rate. Clients talk to the game loop
//! only through channels, so the simulation stays lock-free and deterministic.

mod bg;
mod db;
mod net;
mod snap;
// beta-server-track (B05-B08): new message handling lives in these modules.
mod social_db;
mod track;

use antediluvia_protocol::{DevCmd, Act, ClientMsg, ServerMsg, EntityId, PROTOCOL_VERSION};
use db::Db;
use glam::Vec2;
use std::collections::{HashMap, HashSet};
use std::time::Duration;
use tokio::net::TcpListener;
use tokio::sync::mpsc;
use antediluvia_sim::world::{new_character, World, SimEvent, TICK_HZ};

/// Commands funneled from connection tasks into the single game loop.
pub enum GameCmd {
    Connect { id: u64, out: mpsc::UnboundedSender<Out> },
    Client { id: u64, msg: ClientMsg },
    Disconnect { id: u64 },
}

/// Game-loop-side record of a connected client.
/// Message to a client's writer task: typed (serialized per-client) or a
/// pre-assembled JSON frame shared across clients (C15 snapshots).
pub enum Out {
    Msg(ServerMsg),
    Raw(String),
    /// A binary delta snapshot frame (B11, `snap`).
    Bin(Vec<u8>),
}

struct Conn {
    out: mpsc::UnboundedSender<Out>,
    name: Option<String>,
    entity: Option<EntityId>,
    act: Act,
    logged_in: bool,
    /// Live guild membership (mirrors the sheet, for chat routing).
    guild: Option<String>,
    /// Account is on the dev allowlist (C14).
    is_dev: bool,
    /// Snapshot area-of-interest radius (C15) — bots shrink theirs.
    aoi: f32,
    /// Conn id of a player who challenged us to a duel.
    pending_duel: Option<u64>,
    /// Guild we've been invited to.
    pending_guild_invite: Option<String>,
    /// Party membership (P1) — ephemeral shared-credit group id.
    party: Option<u64>,
    /// (party id or 0=new, inviter conn id) of the latest party invite.
    pending_party_invite: Option<u64>,
    /// Snapshot wire asked for at login (B11): `wire::WIRE_JSON` or
    /// `wire::WIRE_BINARY`.
    wire: u8,
    /// Binary wire: the client asked for a keyframe (`Resync`).
    resync: bool,
}

impl Conn {
    fn send(&self, msg: ServerMsg) {
        let _ = self.out.send(Out::Msg(msg));
    }
    fn send_raw(&self, frame: String) {
        let _ = self.out.send(Out::Raw(frame));
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
    let mut tick_accum = Duration::ZERO;
    let mut tick_max = Duration::ZERO;
    let mut seed_round: u64 = 0;
    seed_auctions(&db, &mut seed_round);
    let mut hub = snap::SnapHub::new();
    let mut listeners: (Vec<u64>, Vec<u64>) = (Vec::new(), Vec::new());
    // Telemetry window in seconds (60 by default; shorter for load tests).
    let telemetry_secs: u64 = std::env::var("ANTEDILUVIA_TELEMETRY_SECS")
        .ok()
        .and_then(|v| v.parse().ok())
        .filter(|v| *v > 0)
        .unwrap_or(60);
    let mut snap_accum = Duration::ZERO;
    let mut snap_max = Duration::ZERO;

    loop {
        tokio::select! {
            _ = tick.tick() => {
                let t0 = std::time::Instant::now();
                let events = world.step();
                dispatch_events(&mut world, &mut conns, events, &hub, &mut listeners);
                track::tick(ticks, &mut world, &mut conns, &db); // beta-server-track hook
                bg::tick(&mut world, &mut conns); // B10 battleground
                // Snapshots at 10 Hz (every other sim tick) — C15 budget.
                if ticks % 2 == 0 {
                    let s0 = std::time::Instant::now();
                    broadcast_snapshots(&world, &mut conns, &mut hub);
                    let s = s0.elapsed();
                    snap_accum += s;
                    snap_max = snap_max.max(s);
                }
                let spent = t0.elapsed();
                tick_accum += spent;
                tick_max = tick_max.max(spent);
                if spent.as_millis() > 45 {
                    tracing::warn!("slow tick: {:?} ({} conns)", spent, conns.len());
                }
                if ticks % (TICK_HZ * telemetry_secs) == 0 && ticks > 0 {
                    tracing::info!(
                        "tick telemetry: avg {:?} max {:?} over {}s ({} conns)",
                        tick_accum / (TICK_HZ * telemetry_secs) as u32, tick_max, telemetry_secs, conns.len()
                    );
                    tick_accum = Duration::ZERO;
                    tick_max = Duration::ZERO;
                    // Binary snapshot cost (B11): time per snapshot pass and
                    // bytes per client per second.
                    let st = hub.take_stats();
                    let bin = conns.values().filter(|c| c.logged_in && c.wire != 0).count() as u64;
                    if bin > 0 {
                        tracing::info!(
                            "snap telemetry: pass avg {:?} max {:?}; {} binary clients, {} B/s each, {} frames, {} keyframes, {} enters, {} updates, {} leaves",
                            snap_accum / (TICK_HZ * telemetry_secs / 2) as u32, snap_max, bin,
                            st.bytes / telemetry_secs / bin, st.frames, st.keyframes, st.enters, st.updates, st.leaves
                        );
                    }
                    snap_accum = Duration::ZERO;
                    snap_max = Duration::ZERO;
                }
                ticks += 1;
                // Persist every online character once every ~10s, a slice
                // of the connections each tick: saving 1,000 sheets in one
                // tick would blow the 50 ms budget by itself.
                save_slice(&world, &conns, &db, ticks);
                // Economy heartbeat (C11): hourly gold-flow log, daily AH re-seed.
                if ticks % (TICK_HZ * 3600) == 0 && ticks > 0 {
                    tracing::info!(
                        minted = world.gold_minted, sunk = world.gold_sunk,
                        "economy: gold flow since boot"
                    );
                }
                if ticks % (TICK_HZ * 86_400) == 0 && ticks > 0 {
                    seed_auctions(&db, &mut seed_round);
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

/// Dev-account allowlist (C14): apple_ids from ANTEDILUVIA_DEV_ACCOUNTS
/// (comma-separated) plus optional dev_accounts.txt (one per line, # comments).
fn dev_accounts() -> &'static std::collections::HashSet<String> {
    static SET: std::sync::OnceLock<std::collections::HashSet<String>> = std::sync::OnceLock::new();
    SET.get_or_init(|| {
        let mut set = std::collections::HashSet::new();
        if let Ok(v) = std::env::var("ANTEDILUVIA_DEV_ACCOUNTS") {
            set.extend(v.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()));
        }
        let path = std::env::var("ANTEDILUVIA_DEV_FILE").unwrap_or_else(|_| "dev_accounts.txt".into());
        if let Ok(txt) = std::fs::read_to_string(path) {
            set.extend(
                txt.lines().map(str::trim)
                    .filter(|l| !l.is_empty() && !l.starts_with('#'))
                    .map(str::to_string),
            );
        }
        set
    })
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
                is_dev: false,
                aoi: AOI_RADIUS,
                pending_duel: None,
                pending_guild_invite: None,
                party: None,
                pending_party_invite: None,
                wire: antediluvia_protocol::wire::WIRE_JSON,
                resync: false,
            });
        }
        GameCmd::Disconnect { id } => {
            // B10: off the field and back where they queued before the save.
            bg::before_disconnect(id, world, conns);
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
                if let Some(pid) = c.party {
                    world.set_party(id, None);
                    broadcast_party_info(conns, pid);
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
    // B10 chunk: the older social messages share the flood gate and honour
    // ignore lists; battleground messages are handled in bg.rs.
    if !matches!(msg, ClientMsg::Login { .. }) {
        if !track::legacy_gate(id, &msg, conns) {
            return;
        }
        let target = match &msg {
            ClientMsg::TradeGive { player, .. }
            | ClientMsg::TradeGold { player, .. }
            | ClientMsg::Duel { player }
            | ClientMsg::GuildInvite { player }
            | ClientMsg::PartyInvite { player } => Some(player.as_str()),
            ClientMsg::MailSend { to, .. } => Some(to.as_str()),
            _ => None,
        };
        if let Some(t) = target {
            let from = conns.get(&id).and_then(|c| c.name.clone()).unwrap_or_default();
            if track::ignored_by(conns, t, &from) {
                return notice(conns, id, format!("{t} is not accepting that from you."));
            }
        }
        if bg::handle(id, &msg, world, conns) {
            return;
        }
    }
    match msg {
        ClientMsg::Login { proto, apple_id, character_name, create, wire } => {
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
            let mut newborn = false;
            let mut sheet = match db.load_by_apple_id(&apple_id) {
                Ok(Some(mut s)) => {
                    // Characters saved before the look field keep their old
                    // skin and hair picks; a stored look is re-clamped in case
                    // the option tables shrank.
                    s.look = if s.look == 0 && (s.appearance[1] != 0 || s.appearance[2] != 0) {
                        antediluvia_protocol::Look::from_legacy(s.appearance).pack()
                    } else {
                        antediluvia_protocol::Look::sanitize(s.look)
                    };
                    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();
                    if let Some(last) = s.last_logout {
                        let diff = now.saturating_sub(last);
                        // 8 hours (28800s) of sleep = 100% recovery
                        let recovery = diff as f32 * 0.00347;
                        s.wakefulness = (s.wakefulness + recovery).clamp(0.0, 100.0);
                    }
                    track::offline_rest(&mut s, now); // beta-server-track hook (B08 rested XP)
                    s
                },
                Ok(None) => {
                    // No character on this account: the builder must create
                    // one (C13). The legacy character_name path still works
                    // for headless test tooling.
                    let (name, class, faction, appearance, look) = match (create, character_name) {
                        (Some(c), _) => {
                            if let Some(f) = &c.faction {
                                if !antediluvia_sim::world::FACTIONS.contains(&f.as_str()) {
                                    return reject(conns, "unknown lineage");
                                }
                            }
                            // Builder names follow the shared name rules and
                            // are stored in their normalised form.
                            let name = match antediluvia_sim::names::validate_name(&c.name) {
                                Ok(n) => n,
                                Err(why) => return reject(conns, why),
                            };
                            // The look is clamped here, whatever the client sent.
                            (name, Some(c.class), c.faction, [
                                c.appearance[0].min(3),
                                c.appearance[1].min(15), // 16 skins (Alpha-2 A3)
                                c.appearance[2].min(11), // 12 hair colors/styles
                            ], antediluvia_protocol::Look::sanitize(c.look))
                        }
                        // Characters made without the builder (bots, test
                        // tooling) get a stable look from their name, so a
                        // crowd of them is not a crowd of clones.
                        (None, Some(n)) => {
                            let seed = antediluvia_protocol::Look::seed_for_name(&n);
                            let look = antediluvia_protocol::Look::random(seed).pack();
                            (n, None, None, [(seed % 4) as u32, 0, 0], look)
                        }
                        (None, None) => {
                            return reject(conns, "no character on this account - run the character builder");
                        }
                    };
                    let name = name.trim().to_string();
                    if name.is_empty() || name.len() > 24 {
                        return reject(conns, "name must be 1-24 characters");
                    }
                    if let Ok(Some(_)) = db.load(&name) {
                        return reject(conns, "character name already taken");
                    }
                    newborn = true;
                    let mut s = antediluvia_sim::world::new_character_with(&name, class, faction, appearance);
                    s.look = look;
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
            let is_dev = dev_accounts().contains(&apple_id);
            let entity_id = world.spawn_player(id, sheet.clone());
            active_names.insert(key.clone());
            if let Some(c) = conns.get_mut(&id) {
                c.name = Some(name.clone());
                c.entity = Some(entity_id);
                c.act = act;
                c.logged_in = true;
                c.guild = guild;
                c.is_dev = is_dev;
                c.wire = wire.min(antediluvia_protocol::wire::WIRE_BINARY);
                c.resync = true;
                c.send(ServerMsg::Welcome { entity_id, character: sheet, is_dev });
                if newborn {
                    // Prologue (Gen 3): every mortal's story starts at the
                    // shut gate, clothed in skins.
                    c.send(ServerMsg::Notice { text: "The gate of the garden closes behind you, and the flaming sword turns every way. You are clothed in skins - mercy's first gift. A Sentinel waits by the gate; the road runs west.".into() });
                }
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
                    // Re-welcome: travel respawns the player under a NEW
                    // entity id — without this the client's my_id goes stale.
                    c.send(ServerMsg::Welcome { entity_id: new_ent, character: sheet, is_dev: c.is_dev });
                }
            }
        }
        ClientMsg::Chat { text } => {
            let (from, act) = match conns.get(&id) {
                Some(c) if c.logged_in => (c.name.clone().unwrap_or_default(), c.act),
                _ => return,
            };
            let text = text.chars().take(240).collect::<String>();
            for (cid, c) in conns.iter() {
                // beta-server-track hook (B07): ignore lists filter zone chat.
                if c.logged_in && c.act == act && track::hears(*cid, &from) {
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
                    notice(conns, id, "Duel started - fight!".into());
                    notice(conns, cid, "Your duel challenge was accepted - fight!".into());
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
                Ok(text) if text.starts_with("Battlemaster:") => {
                    notice(conns, id, text);
                    bg::handle(id, &ClientMsg::BgQueue, world, conns); // B10
                }
                Ok(text) | Err(text) => notice(conns, id, text),
            }
            send_stats(world, conns, id);
        }
        ClientMsg::Mount => {
            let Some((act, ent)) = conn_entity(conns, id) else { return };
            match world.toggle_mount(act, ent) {
                Ok(text) | Err(text) => notice(conns, id, text),
            }
        }
        ClientMsg::Tame { target } => {
            let Some((act, ent)) = conn_entity(conns, id) else { return };
            match world.tame(act, ent, target) {
                Ok(text) | Err(text) => notice(conns, id, text),
            }
            send_stats(world, conns, id);
        }
        ClientMsg::Stable { item } => {
            let Some((act, ent)) = conn_entity(conns, id) else { return };
            match world.stable_mount(act, ent, &item, true) {
                Ok(text) | Err(text) => notice(conns, id, text),
            }
            send_stats(world, conns, id);
        }
        ClientMsg::Unstable { item } => {
            let Some((act, ent)) = conn_entity(conns, id) else { return };
            match world.stable_mount(act, ent, &item, false) {
                Ok(text) | Err(text) => notice(conns, id, text),
            }
            send_stats(world, conns, id);
        }
        ClientMsg::ChooseFaction { faction } => {
            let Some((act, ent)) = conn_entity(conns, id) else { return };
            match world.choose_faction(act, ent, &faction) {
                Ok(text) | Err(text) => notice(conns, id, text),
            }
            send_stats(world, conns, id);
        }
        ClientMsg::Buy { item } => {
            let Some((act, ent)) = conn_entity(conns, id) else { return };
            match world.buy(act, ent, &item) {
                Ok(text) | Err(text) => notice(conns, id, text),
            }
            send_stats(world, conns, id);
        }
        ClientMsg::Sell { item } => {
            let Some((act, ent)) = conn_entity(conns, id) else { return };
            match world.sell(act, ent, &item) {
                Ok(text) | Err(text) => notice(conns, id, text),
            }
            send_stats(world, conns, id);
        }
        ClientMsg::Dev { cmd } => {
            let Some((act, ent)) = conn_entity(conns, id) else { return };
            let (who, is_dev) = conns
                .get(&id)
                .map(|c| (c.name.clone().unwrap_or_default(), c.is_dev))
                .unwrap_or_default();
            if !is_dev {
                tracing::warn!(conn = id, %who, ?cmd, "DEV AUDIT: unauthorized dev command");
                return notice(conns, id, "You are not a developer.".into());
            }
            tracing::info!(conn = id, %who, ?cmd, "DEV AUDIT");
            let text = match cmd {
                DevCmd::Teleport { x, y } => world.dev_teleport(act, ent, x, y),
                DevCmd::GiveItem { item, n } => world.dev_give(act, ent, &item, n.min(100)),
                DevCmd::SetLevel { level } => world.dev_set_level(act, ent, level),
                DevCmd::Heal => world.dev_heal(act, ent),
                DevCmd::SpawnMob { tag } => world.dev_spawn_mob(act, ent, &tag),
                DevCmd::KillTarget => world.dev_kill_target(act, ent),
                DevCmd::Godmode => world.dev_godmode(act, ent),
                DevCmd::GiveXp { amount } => world.dev_give_xp(act, ent, amount),
                DevCmd::QuestComplete { id } => world.dev_quest_complete(act, ent, &id),
                DevCmd::TimeOfDay { t } => {
                    world.time_override = Some(t.rem_euclid(1.0));
                    format!("[dev] time set to {:.2}", t.rem_euclid(1.0))
                }
            };
            notice(conns, id, text);
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
        ClientMsg::PartyInvite { player } => {
            let from = match conns.get(&id) {
                Some(c) if c.logged_in => c.name.clone().unwrap_or_default(),
                _ => return,
            };
            let target = conns.iter().find_map(|(cid, c)| {
                (*cid != id
                    && c.logged_in
                    && c.name.as_deref().map(|n| n.eq_ignore_ascii_case(&player)) == Some(true))
                .then_some(*cid)
            });
            match target {
                Some(tid) => {
                    if conns.get(&tid).map(|c| c.party.is_some()).unwrap_or(false) {
                        return notice(conns, id, format!("{player} is already in a party."));
                    }
                    if let Some(t) = conns.get_mut(&tid) {
                        t.pending_party_invite = Some(id);
                        t.send(ServerMsg::Notice {
                            text: format!("{from} invites you to a party. /paccept to join."),
                        });
                    }
                    notice(conns, id, format!("Invited {player} to your party."));
                }
                None => notice(conns, id, "No such player online.".into()),
            }
        }
        ClientMsg::PartyAccept => {
            let inviter = match conns.get_mut(&id) {
                Some(c) if c.logged_in => c.pending_party_invite.take(),
                _ => return,
            };
            let Some(inviter) = inviter else {
                return notice(conns, id, "No pending party invite.".into());
            };
            if conns.get(&id).map(|c| c.party.is_some()).unwrap_or(false) {
                return notice(conns, id, "You are already in a party.".into());
            }
            // B10 chunk: parties hold 5 (raids, 40, are the bigger group).
            let pid_now = conns.get(&inviter).and_then(|c| c.party).unwrap_or(inviter);
            let size = conns.values().filter(|c| c.logged_in && c.party == Some(pid_now)).count().max(1);
            if size >= PARTY_CAP {
                return notice(conns, id, format!("That party is full ({PARTY_CAP}). Form a raid for more."));
            }
            if !conns.get(&inviter).map(|c| c.logged_in).unwrap_or(false) {
                return notice(conns, id, "The inviter is no longer online.".into());
            }
            // Reuse the inviter's party or mint a new id (inviter conn id is
            // unique and stable for the session — use it as the party id).
            let pid = conns.get(&inviter).and_then(|c| c.party).unwrap_or(inviter);
            if let Some(c) = conns.get_mut(&inviter) {
                c.party = Some(pid);
            }
            world.set_party(inviter, Some(pid));
            if let Some(c) = conns.get_mut(&id) {
                c.party = Some(pid);
            }
            world.set_party(id, Some(pid));
            broadcast_party_info(conns, pid);
        }
        ClientMsg::PartyLeave => {
            let pid = match conns.get_mut(&id) {
                Some(c) if c.logged_in => c.party.take(),
                _ => return,
            };
            let Some(pid) = pid else {
                return notice(conns, id, "You are not in a party.".into());
            };
            world.set_party(id, None);
            notice(conns, id, "You left the party.".into());
            broadcast_party_info(conns, pid);
            // A party of one dissolves.
            let rest: Vec<u64> = conns
                .iter()
                .filter(|(_, c)| c.party == Some(pid))
                .map(|(cid, _)| *cid)
                .collect();
            if rest.len() == 1 {
                let last = rest[0];
                if let Some(c) = conns.get_mut(&last) {
                    c.party = None;
                }
                world.set_party(last, None);
                notice(conns, last, "Your party has disbanded.".into());
                if let Some(c) = conns.get(&last) {
                    c.send(ServerMsg::PartyInfo { members: Vec::new() });
                }
            }
        }
        ClientMsg::BankDeposit { item } => {
            let Some((act, ent)) = conn_entity(conns, id) else { return };
            match world.bank_item(act, ent, &item, true) {
                Ok(t) | Err(t) => notice(conns, id, t),
            }
            send_stats(world, conns, id);
        }
        ClientMsg::BankWithdraw { item } => {
            let Some((act, ent)) = conn_entity(conns, id) else { return };
            match world.bank_item(act, ent, &item, false) {
                Ok(t) | Err(t) => notice(conns, id, t),
            }
            send_stats(world, conns, id);
        }
        ClientMsg::SetHome => {
            let Some((act, ent)) = conn_entity(conns, id) else { return };
            match world.set_home(act, ent) {
                Ok(t) | Err(t) => notice(conns, id, t),
            }
        }
        ClientMsg::BankGold { amount } => {
            let Some((act, ent)) = conn_entity(conns, id) else { return };
            match world.bank_gold(act, ent, amount) {
                Ok(t) | Err(t) => notice(conns, id, t),
            }
            send_stats(world, conns, id);
        }
        ClientMsg::TradeGive { player, item } => {
            let Some((act, ent)) = conn_entity(conns, id) else { return };
            match world.trade_give(act, ent, &player, Some(&item), 0) {
                Ok((gm, rm, rowner)) => {
                    notice(conns, id, gm);
                    notice(conns, rowner, rm);
                    send_stats(world, conns, rowner);
                }
                Err(t) => notice(conns, id, t),
            }
            send_stats(world, conns, id);
        }
        ClientMsg::TradeGold { player, amount } => {
            let Some((act, ent)) = conn_entity(conns, id) else { return };
            if amount == 0 {
                return notice(conns, id, "Give how much gold?".into());
            }
            match world.trade_give(act, ent, &player, None, amount) {
                Ok((gm, rm, rowner)) => {
                    notice(conns, id, gm);
                    notice(conns, rowner, rm);
                    send_stats(world, conns, rowner);
                }
                Err(t) => notice(conns, id, t),
            }
            send_stats(world, conns, id);
        }
        ClientMsg::MailSend { to, item, gold } => {
            let Some((act, ent)) = conn_entity(conns, id) else { return };
            let from = conns.get(&id).and_then(|c| c.name.clone()).unwrap_or_default();
            if to.eq_ignore_ascii_case(&from) {
                return notice(conns, id, "You can't mail yourself.".into());
            }
            match db.character_exists(&to) {
                Ok(true) => {}
                Ok(false) => return notice(conns, id, "No character by that name.".into()),
                Err(e) => {
                    tracing::error!("mail lookup: {e}");
                    return notice(conns, id, "Server error.".into());
                }
            }
            // Take the attachment from the sender up front.
            if let Some(sheet) = world.sheet_of(act, ent) {
                if let Some(it) = &item {
                    if !sheet.inventory.iter().any(|i| i == it) {
                        return notice(conns, id, "You don't have that item.".into());
                    }
                }
                if sheet.gold < gold {
                    return notice(conns, id, "You don't have that much gold.".into());
                }
            } else {
                return;
            }
            world.mail_debit(act, ent, item.as_deref(), gold);
            if let Err(e) = db.mail_send(&to, &from, item.as_deref(), gold) {
                tracing::error!("mail_send: {e}");
                return notice(conns, id, "Server error sending mail.".into());
            }
            notice(conns, id, format!("Mail sent to {to}."));
            send_stats(world, conns, id);
            // Instant delivery ping if they're online.
            if let Some((tid, _)) = conns.iter().find(|(_, c)| {
                c.logged_in && c.name.as_deref().map(|n| n.eq_ignore_ascii_case(&to)) == Some(true)
            }) {
                notice(conns, *tid, format!("You have new mail from {from}. /mailcheck at an inn."));
            }
        }
        ClientMsg::MailCheck => {
            let Some((act, ent)) = conn_entity(conns, id) else { return };
            let me = conns.get(&id).and_then(|c| c.name.clone()).unwrap_or_default();
            if !world.at_inn(act, ent) {
                return notice(conns, id, "Visit an inn to collect your mail.".into());
            }
            match db.mail_take(&me, 10) {
                Ok(mails) if mails.is_empty() => notice(conns, id, "No mail.".into()),
                Ok(mails) => {
                    for (from, item, gold) in mails {
                        let ok = world.mail_credit(act, ent, item.as_deref(), gold);
                        if !ok {
                            // Bags full: return to sender's mailbox.
                            let _ = db.mail_send(&me, &from, item.as_deref(), gold);
                            notice(conns, id, "Bags full - remaining mail kept at the inn.".into());
                            break;
                        }
                        let what = match (&item, gold) {
                            (Some(it), 0) => it.clone(),
                            (Some(it), g) => format!("{it} and {g}g"),
                            (None, g) => format!("{g}g"),
                        };
                        notice(conns, id, format!("Mail from {from}: {what}."));
                    }
                    send_stats(world, conns, id);
                }
                Err(e) => {
                    tracing::error!("mail_take: {e}");
                    notice(conns, id, "Server error reading mail.".into());
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
                        // The house keeps 5% (C11 gold sink).
                        world.add_gold(sact, sent, antediluvia_sim::economy::ah_cut(listing.price));
                    }
                    notice(conns, scid, format!(
                        "Your {} sold for {}g ({}g after the house cut).",
                        listing.item, listing.price, antediluvia_sim::economy::ah_cut(listing.price)
                    ));
                    send_stats(world, conns, scid);
                }
                None => {
                    if let Err(e) = db.credit_gold(&listing.seller, antediluvia_sim::economy::ah_cut(listing.price)) {
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
        ClientMsg::SetAoi { radius } => {
            if let Some(c) = conns.get_mut(&id) {
                c.aoi = radius.clamp(100.0, AOI_RADIUS);
            }
        }
        ClientMsg::Ping => {
            if let Some(c) = conns.get(&id) {
                c.send(ServerMsg::Pong);
            }
        }
        ClientMsg::Resync => {
            // Answered with a keyframe by `snap` (rate-limited there).
            if let Some(c) = conns.get_mut(&id) {
                c.resync = true;
            }
        }
        // beta-server-track hook: every B05-B08 message is handled in track.rs.
        other => track::handle_client(id, other, world, conns, db),
    }
}

/// Seed the auction house with NPC listings when it runs thin (C11) so a
/// lone player still sees a market. Idempotent: only tops up below 5 lots.
fn seed_auctions(db: &Db, round: &mut u64) {
    let live = db.auction_list().map(|l| l.len()).unwrap_or(0);
    for _ in live..5 {
        let (seller, item, price) = antediluvia_sim::economy::seed_listing(*round);
        *round += 1;
        match db.auction_insert(seller, item, price) {
            Ok(_) => tracing::info!("AH seed: {seller} lists {item} at {price}g"),
            Err(e) => tracing::error!("AH seed: {e}"),
        }
    }
}

/// (act, entity) for a logged-in connection.
/// Send the current roster to every member of a party (P1).
fn broadcast_party_info(conns: &HashMap<u64, Conn>, pid: u64) {
    let members: Vec<String> = conns
        .values()
        .filter(|c| c.party == Some(pid))
        .filter_map(|c| c.name.clone())
        .collect();
    for c in conns.values() {
        if c.party == Some(pid) {
            c.send(ServerMsg::PartyInfo { members: members.clone() });
        }
    }
}

/// Party size cap (B10 chunk).
const PARTY_CAP: usize = 5;

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
fn dispatch_events(
    world: &mut World,
    conns: &mut HashMap<u64, Conn>,
    events: Vec<SimEvent>,
    hub: &snap::SnapHub,
    listeners: &mut (Vec<u64>, Vec<u64>),
) {
    for ev in events {
        // Combat events are cosmetic (they drive swing / hit / death
        // animations), and only entities drawn as full rigs animate. They
        // used to fan out to every client in the act, which is N listeners
        // for each of N fighters; now a client hears of an event when its
        // source is in that client's 10 Hz tier (B11), or, for JSON clients,
        // within the near radius.
        if let SimEvent::Combat { act, kind, src, dst } = ev {
            let (binary, plain) = (&mut listeners.0, &mut listeners.1);
            hub.event_listeners(world, act, src, binary, plain);
            // Whoever is hit always hears of it.
            let victim = dst
                .and_then(|d| world.zones.get(&act).and_then(|z| z.entities.get(&d)))
                .and_then(|e| e.owner);
            if let Some(v) = victim {
                if !binary.contains(&v) && !plain.contains(&v) {
                    binary.push(v);
                }
            }
            for id in binary.iter() {
                if let Some(c) = conns.get(id) {
                    if c.logged_in && c.act == act {
                        c.send(ServerMsg::Event { act, kind, src, dst });
                    }
                }
            }
            for id in plain.iter() {
                if let Some(c) = conns.get(id) {
                    if c.logged_in && c.act == act && c.wire == antediluvia_protocol::wire::WIRE_JSON {
                        c.send(ServerMsg::Event { act, kind, src, dst });
                    }
                }
            }
            continue;
        }
        // Death → home inn (v0.5.0). The player already respawned at the death
        // act's inn inside the sim; if their home is a different act, migrate
        // them there (same path as Travel) and re-Welcome under the new id.
        if let SimEvent::Died { owner, home } = &ev {
            let owner = *owner;
            let (cur_act, ent) = match conns.get(&owner) {
                Some(c) if c.logged_in => (c.act, c.entity),
                _ => continue,
            };
            let Some(ent) = ent else { continue };
            let home_msg = match home {
                Some(h) if *h != cur_act => {
                    if let Some(mut sheet) = world.player_sheet(cur_act, ent) {
                        world.remove_player(cur_act, ent);
                        sheet.act = *h;
                        sheet.x = 0.0;
                        sheet.y = 0.0;
                        let new_ent = world.spawn_player(owner, sheet.clone());
                        if let Some(c) = conns.get_mut(&owner) {
                            c.act = *h;
                            c.entity = Some(new_ent);
                            c.send(ServerMsg::Welcome {
                                entity_id: new_ent,
                                character: sheet,
                                is_dev: c.is_dev,
                            });
                        }
                    }
                    format!("You were slain. You return home to the {} inn.", h.as_str())
                }
                Some(h) => format!("You were slain. You return home to the {} inn.", h.as_str()),
                None => "You were slain and revived at the shrine. Use /sethome at an inn to set a home.".to_string(),
            };
            if let Some(c) = conns.get(&owner) {
                c.send(ServerMsg::Notice { text: home_msg });
            }
            send_stats(world, conns, owner);
            continue;
        }
        let owner = match &ev {
            SimEvent::LevelUp { owner, .. } => *owner,
            SimEvent::Died { owner, .. } => *owner,
            SimEvent::Loot { owner, .. } => *owner,
            SimEvent::Info { owner, .. } => *owner,
            SimEvent::CastStart { owner, .. } => *owner,
            SimEvent::AbilityUsed { owner, .. } => *owner,
            SimEvent::Vitals { owner } => *owner,
            SimEvent::Combat { .. } => unreachable!(),
        };
        let Some(c) = conns.get(&owner) else { continue };
        match ev {
            SimEvent::LevelUp { level, .. } => {
                c.send(ServerMsg::Notice { text: format!("You reached level {level}!") });
            }
            SimEvent::Died { .. } => {} // handled above (home migration)
            SimEvent::Loot { item, .. } => {
                c.send(ServerMsg::Notice { text: format!("You collected {item}.") });
            }
            SimEvent::Info { text, .. } => {
                if text == "Honorable kill!" {
                    bg::note_kill(owner); // B10 scoreboard
                }
                c.send(ServerMsg::Notice { text });
            }
            SimEvent::CastStart { ability, secs, .. } => {
                // Cast bar only; the sheet has not changed yet.
                c.send(ServerMsg::CastStart { ability: ability.to_string(), secs });
                continue;
            }
            SimEvent::AbilityUsed { ability, cooldown, .. } => {
                c.send(ServerMsg::AbilityUsed { ability: ability.to_string(), cooldown });
            }
            SimEvent::Vitals { .. } => {} // just the refreshed stat block below
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
/// Hard ceiling on entities in one JSON snapshot frame, nearest-first. This
/// is the interest-management backstop of the text wire: AOI alone does not
/// bound anything when a thousand players pile into the same square. The
/// binary wire (`snap`) has its own, much higher caps.
const MAX_ENTITIES_PER_FRAME: usize = 150;

/// Send each logged-in client a snapshot centered on its own player. Binary
/// clients get delta frames from `snap`; JSON clients (test tools) get their
/// nearest `MAX_ENTITIES_PER_FRAME` entities in full. Both walk the AoI grid:
/// no entity is read or serialized unless some client can see it.
fn broadcast_snapshots(world: &World, conns: &mut HashMap<u64, Conn>, hub: &mut snap::SnapHub) {
    let now_secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs();
    // 86400 seconds in a day. time_of_day = 0.0 at midnight UTC, 0.5 at noon UTC.
    let time_of_day = world
        .time_override
        .unwrap_or((now_secs % 86400) as f32 / 86400.0);

    // Binary delta clients.
    let mut reqs: Vec<snap::ClientReq> = Vec::new();
    for (id, c) in conns.iter_mut() {
        if !c.logged_in || c.wire == antediluvia_protocol::wire::WIRE_JSON {
            continue;
        }
        let Some(entity) = c.entity else { continue };
        reqs.push(snap::ClientReq { conn: *id, act: c.act, entity, aoi: c.aoi, resync: c.resync });
        c.resync = false;
    }
    hub.run(world, &reqs, time_of_day, |conn, frame| {
        if let Some(c) = conns.get(&conn) {
            let _ = c.out.send(Out::Bin(frame));
        }
    });

    // JSON clients. Each entity is serialized at most once per snapshot, on
    // first need, and the fragment is shared by every client that sees it.
    let mut frags: HashMap<EntityId, String> = HashMap::new();
    let mut near: Vec<(f32, EntityId)> = Vec::new();
    for c in conns.values() {
        if !c.logged_in || c.wire != antediluvia_protocol::wire::WIRE_JSON {
            continue;
        }
        let Some(ent) = c.entity else { continue };
        let Some(zone) = world.zones.get(&c.act) else { continue };
        let pos = world.player_pos(c.act, ent).unwrap_or_default();
        let r2 = c.aoi * c.aoi;
        near.clear();
        for cell in antediluvia_sim::aoi::cells_in_radius(pos, c.aoi) {
            for id in zone.grid.cell_entities(cell) {
                if let Some(e) = zone.entities.get(id) {
                    let d2 = e.pos.distance_squared(pos);
                    if d2 <= r2 {
                        near.push((d2, *id));
                    }
                }
            }
        }
        // Without this cap a crowded zone is O(N^2) on the text wire: every
        // one of N clients gets a frame containing all N entities in full.
        if near.len() > MAX_ENTITIES_PER_FRAME {
            near.select_nth_unstable_by(MAX_ENTITIES_PER_FRAME, |a, b| a.0.total_cmp(&b.0));
            near.truncate(MAX_ENTITIES_PER_FRAME);
        }
        let mut frame = format!(
            "{{\"t\":\"snapshot\",\"act\":\"{}\",\"tick\":{},\"time_of_day\":{},\"entities\":[",
            c.act.as_str(), zone.tick, time_of_day
        );
        let mut first = true;
        for (_, id) in &near {
            let Some(e) = zone.entities.get(id) else { continue };
            let json = frags
                .entry(*id)
                .or_insert_with(|| serde_json::to_string(&e.to_state()).unwrap_or_default());
            if !first {
                frame.push(',');
            }
            first = false;
            frame.push_str(json);
        }
        frame.push_str("]}");
        c.send_raw(frame);
    }
}

/// Ticks between two saves of the same character (10 s).
const SAVE_PERIOD: u64 = TICK_HZ * 10;

/// Save the characters whose turn it is this tick (connection id modulo the
/// save period), so the work is level instead of one spike every 10 s.
fn save_slice(world: &World, conns: &HashMap<u64, Conn>, db: &Db, ticks: u64) {
    for (id, c) in conns.iter() {
        if (id + ticks) % SAVE_PERIOD != 0 {
            continue;
        }
        if let (true, Some(ent)) = (c.logged_in, c.entity) {
            if let Some(sheet) = world.player_sheet(c.act, ent) {
                if let Err(e) = db.save(&sheet, None) {
                    tracing::error!("periodic save: {e}");
                }
            }
        }
    }
}
