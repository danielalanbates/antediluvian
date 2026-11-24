//! Embedded single-player driver — runs the authoritative `World` in-process
//! instead of over a WebSocket, so the game plays with no server (and, once
//! compiled to wasm, entirely in the browser).
//!
//! It returns the SAME `(NetTx, NetRx)` channel pair as `net::start_network`,
//! so the rest of the client is unchanged: a background thread owns a `World`,
//! spawns the player, drains `ClientMsg`s into `World`'s public verbs, steps
//! the sim at the fixed tick rate, and emits `Welcome`/`Snapshot`/`Stats`/
//! `Notice`/`Event` back to Bevy. Multiplayer-only messages (guild, auction,
//! mail, trade, party, duels, raids, channels, whispers, friends) reply with
//! a friendly notice in solo play; boards, flight masters, repair, spirit
//! release and zone banners run on the same sim calls the server uses. The
//! match in `apply` is exhaustive on purpose: a new `ClientMsg` does not
//! compile until solo play decides what to do with it.
//!
//! Persistence is per-session for now (localStorage save/load is a wasm
//! follow-up); the character comes from the builder payload each launch.

use crate::net::{NetRx, NetTx};
use antediluvia_protocol::{
    CharacterCreate, ClientMsg, DevCmd, EntityId, EventKind, ServerMsg, Vec2 as PVec2,
};
use antediluvia_sim::world::{new_character_with, SimEvent, World};
use glam::Vec2;
use std::time::{Duration, Instant};
use tokio::sync::mpsc::unbounded_channel;

/// Spawn the embedded sim thread; returns Bevy-side channel ends.
pub fn start_local(
    display_name: String,
    create: Option<CharacterCreate>,
) -> (NetTx, NetRx) {
    let (tx_client, mut rx_client) = unbounded_channel::<ClientMsg>();
    let (tx_server, rx_server) = unbounded_channel::<ServerMsg>();

    std::thread::Builder::new()
        .name("antediluvia-local".into())
        .spawn(move || {
            let mut world = World::new(0xA17E_D17Eu64);

            // Build the character from the builder payload (or a default so the
            // world is always playable), spawn it, and welcome the client.
            let (name, class, faction, appearance, look) = match create {
                Some(c) => (
                    antediluvia_sim::names::validate_name(&c.name).unwrap_or_else(|_| display_name.clone()),
                    Some(c.class),
                    c.faction,
                    [c.appearance[0].min(3), c.appearance[1].min(15), c.appearance[2].min(11)],
                    antediluvia_protocol::Look::sanitize(c.look),
                ),
                None => {
                    let seed = antediluvia_protocol::Look::seed_for_name(&display_name);
                    (display_name.clone(), None, None, [(seed % 4) as u32, 0, 0], antediluvia_protocol::Look::random(seed).pack())
                }
            };
            let mut sheet = new_character_with(&name, class, faction, appearance);
            sheet.look = look;
            let mut act = sheet.act;
            let mut me: EntityId = world.spawn_player(1, sheet.clone());

            let send = |m: ServerMsg| { let _ = tx_server.send(m); };
            send(ServerMsg::Welcome { entity_id: me, character: sheet, is_dev: true });
            if let Some(s) = world.player_sheet(act, me) {
                send(ServerMsg::Stats { character: s });
            }

            let tick_dt = Duration::from_millis(1000 / antediluvia_sim::world::TICK_HZ);
            let mut next = Instant::now();
            let start = Instant::now();
            let mut ticks: u64 = 0;
            let mut region: Option<(antediluvia_protocol::Act, String)> = None;

            loop {
                // Drain all pending client messages.
                let mut dirty = false; // stats changed → push a fresh sheet
                while let Ok(msg) = rx_client.try_recv() {
                    if apply(&mut world, &mut act, &mut me, msg, &send) {
                        dirty = true;
                    }
                }

                // Step the simulation and translate its events.
                for ev in world.step() {
                    match ev {
                        SimEvent::LevelUp { level, .. } => {
                            send(ServerMsg::Notice { text: format!("You reached level {level}!") });
                            dirty = true;
                        }
                        SimEvent::Died { home, .. } => {
                            let text = match home {
                                Some(h) => format!("You were slain. You return home to the {} inn.", h.as_str()),
                                None => "You were slain and revived at the shrine. Use /sethome at an inn to set a home.".into(),
                            };
                            send(ServerMsg::Notice { text });
                            dirty = true;
                        }
                        SimEvent::Loot { item, .. } => {
                            send(ServerMsg::Notice { text: format!("You collected {item}.") });
                            dirty = true;
                        }
                        SimEvent::Info { text, .. } => send(ServerMsg::Notice { text }),
                        SimEvent::CastStart { ability, secs, .. } => {
                            send(ServerMsg::CastStart { ability: ability.to_string(), secs });
                        }
                        SimEvent::AbilityUsed { ability, cooldown, .. } => {
                            send(ServerMsg::AbilityUsed { ability: ability.to_string(), cooldown });
                            dirty = true;
                        }
                        SimEvent::Vitals { .. } => dirty = true,
                        SimEvent::Combat { act: a, kind, src, dst } => {
                            send(ServerMsg::Event { act: a, kind, src, dst });
                        }
                    }
                }

                if dirty {
                    if let Some(s) = world.player_sheet(act, me) {
                        send(ServerMsg::Stats { character: s });
                    }
                }

                // Zone-in banners, checked four times a second like the server.
                if ticks % 5 == 0 {
                    if let Some(b) = zone_banner(&world, act, me, &mut region) {
                        send(b);
                    }
                }

                // Snapshot at ~10 Hz (every other tick), whole zone (solo = no AoI).
                if ticks % 2 == 0 {
                    let (tick, entities) = world.zone_snapshot(act);
                    let tod = (start.elapsed().as_secs_f32() / 120.0).fract(); // 2-min day
                    let time_of_day = world.time_override.unwrap_or(tod);
                    send(ServerMsg::Snapshot { act, tick, time_of_day, entities });
                }

                ticks += 1;
                next += tick_dt;
                let now = Instant::now();
                if next > now {
                    std::thread::sleep(next - now);
                } else {
                    next = now; // fell behind; don't spiral
                }
            }
        })
        .expect("spawn local sim thread");

    (NetTx(tx_client), NetRx(rx_server))
}

/// Apply one client message to the world. Returns true if the player's stats
/// changed (so the caller pushes a refreshed sheet).
fn apply(
    world: &mut World,
    act: &mut antediluvia_protocol::Act,
    me_ref: &mut EntityId,
    msg: ClientMsg,
    send: &impl Fn(ServerMsg),
) -> bool {
    let me = *me_ref;
    let a = *act;
    let notice = |t: String| send(ServerMsg::Notice { text: t });
    match msg {
        ClientMsg::Move { dx, dy } => {
            world.set_intent(a, me, Vec2::new(dx, dy));
            false
        }
        ClientMsg::Attack => { world.queue_attack(a, me); false }
        ClientMsg::Cast { ability } => { world.queue_cast(a, me, ability); false }
        ClientMsg::SelectClass { class } => {
            match world.select_class(a, me, class) { Ok(t) | Err(t) => notice(t) }; true
        }
        ClientMsg::LearnTalent { talent } => {
            match world.learn_talent(a, me, &talent) { Ok(t) | Err(t) => notice(t) }; true
        }
        ClientMsg::Talk => { match world.talk(a, me) { Ok(t) | Err(t) => notice(t) }; true }
        ClientMsg::UseItem { item } => { match world.use_item(a, me, &item) { Ok(t) | Err(t) => notice(t) }; true }
        ClientMsg::Equip { item } => { match world.equip(a, me, &item) { Ok(t) | Err(t) => notice(t) }; true }
        ClientMsg::Craft { recipe } => { match world.craft(a, me, &recipe) { Ok(t) | Err(t) => notice(t) }; true }
        ClientMsg::Buy { item } => { match world.buy(a, me, &item) { Ok(t) | Err(t) => notice(t) }; true }
        ClientMsg::Sell { item } => { match world.sell(a, me, &item) { Ok(t) | Err(t) => notice(t) }; true }
        ClientMsg::Mount => { match world.toggle_mount(a, me) { Ok(t) | Err(t) => notice(t) }; true }
        ClientMsg::Tame { target } => { match world.tame(a, me, target) { Ok(t) | Err(t) => notice(t) }; true }
        ClientMsg::ChooseFaction { faction } => { match world.choose_faction(a, me, &faction) { Ok(t) | Err(t) => notice(t) }; true }
        ClientMsg::SetHome => { match world.set_home(a, me) { Ok(t) | Err(t) => notice(t) }; false }
        ClientMsg::TogglePvp => {
            if let Some(on) = world.toggle_pvp(a, me) {
                notice(format!("PvP {}.", if on { "enabled" } else { "disabled" }));
            }
            true
        }
        ClientMsg::Travel { act: dest } => {
            if dest != a {
                if let Some(mut s) = world.player_sheet(a, me) {
                    world.remove_player(a, me);
                    s.act = dest;
                    s.x = 0.0; s.y = 0.0;
                    let new_ent = world.spawn_player(1, s.clone());
                    *act = dest;
                    *me_ref = new_ent; // dev cmds/actions must track the new entity
                    // Same entity-id convention as the server: re-Welcome.
                    send(ServerMsg::Welcome { entity_id: new_ent, character: s, is_dev: true });
                    notice(format!("You travel to {}.", dest.as_str()));
                }
            }
            true
        }
        ClientMsg::Dev { cmd } => {
            let text = match cmd {
                DevCmd::Teleport { x, y } => world.dev_teleport(a, me, x, y),
                DevCmd::GiveItem { item, n } => world.dev_give(a, me, &item, n.min(100)),
                DevCmd::SetLevel { level } => world.dev_set_level(a, me, level),
                DevCmd::Heal => world.dev_heal(a, me),
                DevCmd::SpawnMob { tag } => world.dev_spawn_mob(a, me, &tag),
                DevCmd::KillTarget => world.dev_kill_target(a, me),
                DevCmd::Godmode => world.dev_godmode(a, me),
                DevCmd::GiveXp { amount } => world.dev_give_xp(a, me, amount),
                DevCmd::QuestComplete { id } => world.dev_quest_complete(a, me, &id),
                DevCmd::TimeOfDay { t } => {
                    world.time_override = Some(t.rem_euclid(1.0));
                    format!("[dev] time set to {:.2}", t.rem_euclid(1.0))
                }
            };
            notice(text); true
        }
        ClientMsg::Chat { text } => { send(ServerMsg::Chat { from: "you".into(), text }); false }
        ClientMsg::Ping => { send(ServerMsg::Pong); false }
        // Multiplayer-only in solo play.
        ClientMsg::GuildCreate { .. } | ClientMsg::GuildInvite { .. } | ClientMsg::GuildAccept
        | ClientMsg::GuildLeave | ClientMsg::GuildChat { .. }
        | ClientMsg::PartyInvite { .. } | ClientMsg::PartyAccept | ClientMsg::PartyLeave
        | ClientMsg::Duel { .. } | ClientMsg::DuelAccept
        | ClientMsg::TradeGive { .. } | ClientMsg::TradeGold { .. }
        | ClientMsg::MailSend { .. } | ClientMsg::MailCheck
        | ClientMsg::AuctionList { .. } | ClientMsg::AuctionBuy { .. } | ClientMsg::AuctionBrowse => {
            notice("Not available in solo play.".into()); false
        }
        // Bank works solo (personal vault).
        ClientMsg::BankDeposit { item } => { match world.bank_item(a, me, &item, true) { Ok(t) | Err(t) => notice(t) }; true }
        ClientMsg::BankWithdraw { item } => { match world.bank_item(a, me, &item, false) { Ok(t) | Err(t) => notice(t) }; true }
        ClientMsg::BankGold { amount } => { match world.bank_gold(a, me, amount) { Ok(t) | Err(t) => notice(t) }; true }
        ClientMsg::Stable { item } => { match world.stable_mount(a, me, &item, true) { Ok(t) | Err(t) => notice(t) }; true }
        ClientMsg::Unstable { item } => { match world.stable_mount(a, me, &item, false) { Ok(t) | Err(t) => notice(t) }; true }
        ClientMsg::Login { .. } | ClientMsg::SetAoi { .. } => false,
        // ── beta-server-track (B05-B08), same sim calls as the server ──
        ClientMsg::QuestBoard => {
            match world.board(a, me) {
                Ok((place, quests)) => send(ServerMsg::QuestBoard { place, quests }),
                Err(t) => notice(t),
            }
            false
        }
        ClientMsg::QuestAccept { id } => { match world.board_accept(a, me, &id) { Ok(t) | Err(t) => notice(t) }; true }
        ClientMsg::QuestTurnIn { id } => { match world.board_turn_in(a, me, &id) { Ok(t) | Err(t) => notice(t) }; true }
        ClientMsg::QuestAbandon { id } => { match world.abandon_quest(a, me, &id) { Ok(t) | Err(t) => notice(t) }; true }
        ClientMsg::FlightList => {
            match world.flight_list(a, me) {
                Ok((here, nodes)) => send(ServerMsg::FlightPaths { here, nodes }),
                Err(t) => notice(t),
            }
            true // standing at a node discovers it
        }
        ClientMsg::FlightTake { node } => {
            match world.flight_take(a, me, &node) {
                Ok(f) => {
                    let moved = f.entity != me || f.act != a;
                    *act = f.act;
                    *me_ref = f.entity;
                    notice(f.text);
                    if moved {
                        // Same contract as Travel: a new entity id needs a
                        // fresh Welcome or the client's own id goes stale.
                        if let Some(sheet) = world.player_sheet(f.act, f.entity) {
                            send(ServerMsg::Welcome { entity_id: f.entity, character: sheet, is_dev: true });
                        }
                    }
                }
                Err(t) => notice(t),
            }
            true
        }
        ClientMsg::ReleaseSpirit => { match world.release_spirit(a, me) { Ok(t) | Err(t) => notice(t) }; true }
        ClientMsg::Repair => {
            match world.repair(a, me) { Ok(t) | Err(t) => notice(t) };
            if let Some((items, repair_cost)) = world.gear_info(a, me) {
                send(ServerMsg::Gear { items, repair_cost });
            }
            true
        }
        ClientMsg::GearInfo => {
            if let Some((items, repair_cost)) = world.gear_info(a, me) {
                send(ServerMsg::Gear { items, repair_cost });
            }
            false
        }
        // Alone in the world: you are the whole list.
        ClientMsg::Who { .. } => {
            if let Some(s) = world.player_sheet(a, me) {
                let entry = antediluvia_protocol::WhoEntry {
                    name: s.name,
                    level: s.level,
                    class: s.class.map(|c| c.as_str().to_string()),
                    act: a,
                    guild: s.guild,
                };
                send(ServerMsg::WhoList { players: vec![entry], total: 1 });
            }
            false
        }
        ClientMsg::Emote { emote, target } => {
            let name = world.player_sheet(a, me).map(|s| s.name).unwrap_or_else(|| "You".into());
            match antediluvia_sim::social::emote_text(&emote, &name, target.as_deref()) {
                Some(text) => send(ServerMsg::Emote { from: name, emote, text }),
                None => notice(format!("Unknown emote: {emote}")),
            }
            false
        }
        ClientMsg::FriendList => { send(ServerMsg::Friends { friends: Vec::new(), ignored: Vec::new() }); false }
        ClientMsg::ChannelList => { send(ServerMsg::Channels { joined: Vec::new(), available: Vec::new() }); false }
        // Other people are needed for these.
        ClientMsg::RaidCreate | ClientMsg::RaidInvite { .. } | ClientMsg::RaidAccept | ClientMsg::RaidLeave
        | ClientMsg::RaidKick { .. } | ClientMsg::RaidChat { .. }
        | ClientMsg::ChannelJoin { .. } | ClientMsg::ChannelLeave { .. } | ClientMsg::ChannelSay { .. }
        | ClientMsg::Whisper { .. }
        | ClientMsg::FriendAdd { .. } | ClientMsg::FriendRemove { .. }
        | ClientMsg::IgnoreAdd { .. } | ClientMsg::IgnoreRemove { .. } => {
            notice("Not available in solo play.".into()); false
        }
    }
}

/// Zone-in banner for solo play: the same region lookup and "quiet" rule as
/// the server (stepping back out of a place into open country is not an
/// arrival). `last` is the (act, region key) the banner was last drawn for.
fn zone_banner(
    world: &World,
    act: antediluvia_protocol::Act,
    me: EntityId,
    last: &mut Option<(antediluvia_protocol::Act, String)>,
) -> Option<ServerMsg> {
    let pos = world.player_pos(act, me)?;
    let faction = world.sheet_of(act, me).and_then(|s| s.faction.as_deref());
    let r = antediluvia_sim::travel::region_at(act, pos, faction);
    if last.as_ref().is_some_and(|(a, k)| *a == act && *k == r.key) {
        return None;
    }
    let quiet = r.key.starts_with("act:") && last.as_ref().is_some_and(|(a, _)| *a == act);
    *last = Some((act, r.key));
    (!quiet).then_some(ServerMsg::ZoneBanner {
        act,
        name: r.name,
        subtitle: r.subtitle,
        level_min: r.level_min,
        level_max: r.level_max,
        pvp: r.pvp,
    })
}

/// Silence unused-import warnings when compiled without the wasm entry.
#[allow(dead_code)]
fn _touch(_: PVec2, _: EventKind) {}
