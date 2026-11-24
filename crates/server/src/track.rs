// Copyright (c) 2026 Daniel Bates / Bates LLC. All rights reserved.
//! Beta server track (B05-B08) wiring: every new `ClientMsg` is handled
//! here, so `main.rs` only gains one catch-all match arm plus three one-line
//! hooks (`tick`, `offline_rest`, `hears`).
//!
//! The rules live in the sim (`questgen`, `social`, `travel`, `durability`);
//! this module owns the live server state they need - raids, channel
//! subscriptions, loaded friend/ignore lists, and the last zone banner sent
//! to each connection - and translates results into `ServerMsg`s.
//!
//! State is a process-wide mutex rather than a parameter so the existing
//! `handle_cmd` / `handle_client_msg` signatures stay untouched. Only the
//! single game-loop task ever locks it.

use crate::db::Db;
use crate::social_db::{KIND_FRIEND, KIND_IGNORE};
use crate::Conn;
use antediluvia_protocol::{Act, ClientMsg, FriendEntry, ServerMsg, WhoEntry};
use antediluvia_sim::social::{self, ChannelBook, Departure, Flood, FloodGate, RaidBook, SocialList};
use antediluvia_sim::travel;
use antediluvia_sim::world::World;
use std::collections::HashMap;
use std::sync::{Mutex, MutexGuard, OnceLock};

/// Emotes are seen within this distance (same act).
const EMOTE_RANGE: f32 = 1400.0;
/// Housekeeping cadence in sim ticks (4 times a second).
const TICK_EVERY: u64 = 5;
/// Longest name / id / channel field taken off the wire (review fix: these
/// used to be echoed back, lowercased and handed to SQLite at any length).
const FIELD_MAX: usize = 48;

fn clip(s: &mut String) {
    if let Some((i, _)) = s.char_indices().nth(FIELD_MAX) {
        s.truncate(i);
    }
}

/// Clip every free-text identifier in a track message. Chat bodies are
/// left alone: `social::clean_chat` caps those.
fn clip_msg(mut msg: ClientMsg) -> ClientMsg {
    match &mut msg {
        ClientMsg::QuestAccept { id } | ClientMsg::QuestTurnIn { id } | ClientMsg::QuestAbandon { id } => clip(id),
        ClientMsg::RaidInvite { player }
        | ClientMsg::RaidKick { player }
        | ClientMsg::FriendAdd { player }
        | ClientMsg::FriendRemove { player }
        | ClientMsg::IgnoreAdd { player }
        | ClientMsg::IgnoreRemove { player } => clip(player),
        ClientMsg::ChannelJoin { channel } | ClientMsg::ChannelLeave { channel } | ClientMsg::ChannelSay { channel, .. } => clip(channel),
        ClientMsg::Whisper { to, .. } => clip(to),
        ClientMsg::Emote { emote, target } => {
            clip(emote);
            if let Some(t) = target {
                clip(t);
            }
        }
        ClientMsg::Who { filter: Some(f) } => clip(f),
        ClientMsg::FlightTake { node } => clip(node),
        _ => {}
    }
    msg
}

/// Flood-budget cost of a message: anything that fans out to other
/// connections, scans every connection, or writes to the database is dear.
fn flood_cost(msg: &ClientMsg) -> u32 {
    match msg {
        ClientMsg::ChannelSay { .. }
        | ClientMsg::RaidChat { .. }
        | ClientMsg::Whisper { .. }
        | ClientMsg::Emote { .. }
        | ClientMsg::Who { .. }
        | ClientMsg::FriendAdd { .. }
        | ClientMsg::FriendRemove { .. }
        | ClientMsg::IgnoreAdd { .. }
        | ClientMsg::IgnoreRemove { .. } => social::FLOOD_COST_CHAT,
        ClientMsg::RaidInvite { .. } => 2,
        _ => 1,
    }
}

#[derive(Default)]
struct State {
    raids: RaidBook,
    channels: ChannelBook,
    lists: HashMap<u64, SocialList>,
    /// conn id -> character name, for every connection we have set up.
    seen: HashMap<u64, String>,
    /// conn id -> (act, region key, position) at the last banner check.
    region: HashMap<u64, (Act, String, (f32, f32))>,
    /// Per-connection message budget (review fix: chat fan-out and DB
    /// writes were unthrottled).
    flood: FloodGate,
    /// Latest sim tick seen by `tick` (the flood gate's clock).
    now: u64,
    /// lowercase character name -> rested XP banked at this login.
    pending_rest: HashMap<String, u32>,
}

fn state() -> MutexGuard<'static, State> {
    static STATE: OnceLock<Mutex<State>> = OnceLock::new();
    STATE.get_or_init(|| Mutex::new(State::default())).lock().unwrap_or_else(|e| e.into_inner())
}

fn name_of(conns: &HashMap<u64, Conn>, id: u64) -> String {
    conns.get(&id).and_then(|c| c.name.clone()).unwrap_or_default()
}

fn find_conn(conns: &HashMap<u64, Conn>, name: &str) -> Option<u64> {
    let name = name.trim();
    conns
        .iter()
        .find(|(_, c)| c.logged_in && c.name.as_deref().is_some_and(|n| n.eq_ignore_ascii_case(name)))
        .map(|(id, _)| *id)
}

impl State {
    /// First time we see a logged-in connection: load its lists, join the
    /// default channels, tell its friends, deliver the rested-XP notice.
    fn ensure(&mut self, id: u64, conns: &HashMap<u64, Conn>, db: &Db) {
        if self.seen.contains_key(&id) {
            return;
        }
        let Some(c) = conns.get(&id) else { return };
        if !c.logged_in {
            return;
        }
        let name = c.name.clone().unwrap_or_default();
        let list = match db.social_load(&name) {
            Ok((f, i)) => SocialList::from_rows(f, i),
            Err(e) => {
                tracing::error!("social load for {name}: {e}");
                SocialList::default()
            }
        };
        self.lists.insert(id, list);
        self.channels.join_defaults(id);
        self.seen.insert(id, name.clone());
        if let Some(gained) = self.pending_rest.remove(&name.to_lowercase()) {
            if gained > 0 {
                c.send(ServerMsg::Notice { text: format!("You feel rested (+{gained} rested XP while you were away).") });
            }
        }
        for (other, l) in &self.lists {
            if *other != id && l.is_friend(&name) {
                crate::notice(conns, *other, format!("Your friend {name} has come online."));
            }
        }
    }

    /// Does connection `to` accept messages from the character `from`?
    fn hears(&self, to: u64, from: &str) -> bool {
        !self.lists.get(&to).is_some_and(|l| l.ignores(from))
    }

    fn raid_info(&self, conns: &HashMap<u64, Conn>, group: u64) -> ServerMsg {
        match self.raids.raid(group) {
            Some(r) => ServerMsg::RaidInfo {
                leader: name_of(conns, r.leader),
                members: r.members.iter().map(|m| name_of(conns, *m)).collect(),
            },
            None => ServerMsg::RaidInfo { leader: String::new(), members: Vec::new() },
        }
    }

    fn broadcast_raid(&self, conns: &HashMap<u64, Conn>, group: u64) {
        let Some(r) = self.raids.raid(group) else { return };
        let msg = self.raid_info(conns, group);
        for m in &r.members {
            if let Some(c) = conns.get(m) {
                c.send(msg.clone());
            }
        }
    }

    /// Someone left a raid: update the rest (or dissolve it).
    fn after_departure(&self, dep: &Departure, world: &mut World, conns: &mut HashMap<u64, Conn>) {
        if let Some(last) = dep.disbanded {
            world.set_party(last, None);
            if let Some(c) = conns.get_mut(&last) {
                if c.party == Some(dep.group) {
                    c.party = None;
                }
                c.send(ServerMsg::Notice { text: "Your raid has disbanded.".into() });
                c.send(ServerMsg::RaidInfo { leader: String::new(), members: Vec::new() });
            }
            return;
        }
        if let Some(l) = dep.new_leader {
            let who = name_of(conns, l);
            for m in &dep.remaining {
                crate::notice(conns, *m, format!("{who} now leads the raid."));
            }
        }
        self.broadcast_raid(conns, dep.group);
    }

    /// Take `id` out of whatever raid it is in and clear its group links.
    fn drop_from_raid(&mut self, id: u64, world: &mut World, conns: &mut HashMap<u64, Conn>) -> bool {
        let Some(dep) = self.raids.leave(id) else { return false };
        world.set_party(id, None);
        if let Some(c) = conns.get_mut(&id) {
            if c.party == Some(dep.group) {
                c.party = None;
            }
            c.send(ServerMsg::RaidInfo { leader: String::new(), members: Vec::new() });
        }
        self.after_departure(&dep, world, conns);
        true
    }

    fn send_channels(&self, conns: &HashMap<u64, Conn>, id: u64) {
        if let Some(c) = conns.get(&id) {
            c.send(ServerMsg::Channels {
                joined: self.channels.joined(id),
                available: social::CHANNELS.iter().map(|s| s.to_string()).collect(),
            });
        }
    }

    fn send_friends(&self, world: &World, conns: &HashMap<u64, Conn>, id: u64) {
        let Some(c) = conns.get(&id) else { return };
        let Some(l) = self.lists.get(&id) else { return };
        let friends = l
            .friends()
            .into_iter()
            .map(|name| match find_conn(conns, &name).and_then(|fid| conns.get(&fid)) {
                Some(f) => FriendEntry {
                    name: f.name.clone().unwrap_or(name),
                    online: true,
                    act: Some(f.act),
                    level: f.entity.and_then(|e| world.sheet_of(f.act, e)).map_or(0, |s| s.level),
                },
                None => FriendEntry { name, online: false, act: None, level: 0 },
            })
            .collect();
        c.send(ServerMsg::Friends { friends, ignored: l.ignored() });
    }
}

/// main.rs hook (login): bank offline rested XP on the loaded sheet and
/// remember the amount for the welcome notice.
pub(crate) fn offline_rest(sheet: &mut antediluvia_protocol::CharacterSheet, now: u64) {
    let gained = travel::accrue_offline_rest(sheet, now);
    // Restart the clock: if the server dies before the next clean logout,
    // the next login must not bank this same absence again.
    sheet.last_logout = Some(now);
    state().pending_rest.insert(sheet.name.to_lowercase(), gained);
}

/// main.rs hook (zone chat): false when connection `to` ignores `from`.
pub(crate) fn hears(to: u64, from: &str) -> bool {
    state().hears(to, from)
}

/// main.rs hook (game loop, every sim tick): set up new logins, clean up
/// after departures, keep raids and the legacy party field in step, and
/// send zone-in banners.
pub(crate) fn tick(ticks: u64, world: &mut World, conns: &mut HashMap<u64, Conn>, db: &Db) {
    if ticks % TICK_EVERY != 0 {
        return;
    }
    let mut guard = state();
    let st = &mut *guard;
    st.now = ticks;

    // New logins.
    let fresh: Vec<u64> = conns.iter().filter(|(id, c)| c.logged_in && !st.seen.contains_key(id)).map(|(id, _)| *id).collect();
    for id in fresh {
        st.ensure(id, conns, db);
    }

    // Departures.
    let gone: Vec<u64> = st.seen.keys().filter(|id| !conns.get(id).is_some_and(|c| c.logged_in)).copied().collect();
    for id in gone {
        let name = st.seen.remove(&id).unwrap_or_default();
        st.lists.remove(&id);
        st.channels.drop_member(id);
        st.region.remove(&id);
        st.flood.forget(id);
        st.drop_from_raid(id, world, conns);
        for (other, l) in &st.lists {
            if l.is_friend(&name) {
                crate::notice(conns, *other, format!("Your friend {name} has gone offline."));
            }
        }
    }

    // Raids vs the legacy `Conn::party` field. A raid member whose party
    // field no longer names the raid used PartyLeave; a non-member whose
    // party field names a raid came in through PartyInvite/PartyAccept.
    for group in st.raids.groups() {
        let members = st.raids.raid(group).map(|r| r.members.clone()).unwrap_or_default();
        for m in members {
            if conns.get(&m).map(|c| c.party) != Some(Some(group)) {
                st.drop_from_raid(m, world, conns);
            }
        }
    }
    let strays: Vec<(u64, u64)> = conns
        .iter()
        .filter_map(|(id, c)| c.party.filter(|g| social::is_raid(*g)).map(|g| (*id, g)))
        .filter(|(id, g)| st.raids.raid_of(*id) != Some(*g))
        .collect();
    for (id, group) in strays {
        match st.raids.join(group, id) {
            Ok(_) => st.broadcast_raid(conns, group),
            Err(why) => {
                world.set_party(id, None);
                if let Some(c) = conns.get_mut(&id) {
                    c.party = None;
                    c.send(ServerMsg::PartyInfo { members: Vec::new() });
                    c.send(ServerMsg::Notice { text: why });
                }
            }
        }
    }

    // Zone-in banners.
    for (id, c) in conns.iter() {
        let (true, Some(ent)) = (c.logged_in, c.entity) else { continue };
        let Some(pos) = world.player_pos(c.act, ent) else { continue };
        let at = (pos.x, pos.y);
        // Review fix: a character who has not moved cannot have changed
        // region, so skip the lookup (230 anchors + three Strings per player,
        // four times a second) for everyone standing still.
        if st.region.get(id).is_some_and(|(a, _, p)| *a == c.act && *p == at) {
            continue;
        }
        let faction = world.sheet_of(c.act, ent).and_then(|s| s.faction.as_deref());
        let r = travel::region_at(c.act, pos, faction);
        if let Some((a, k, p)) = st.region.get_mut(id) {
            if *a == c.act && *k == r.key {
                *p = at;
                continue;
            }
        }
        let last = st.region.get(id);
        // Stepping back out of a POI into the same act's open country is
        // not an arrival; everything else is.
        let quiet = r.key.starts_with("act:") && last.is_some_and(|(a, _, _)| *a == c.act);
        st.region.insert(*id, (c.act, r.key.clone(), at));
        if !quiet {
            c.send(ServerMsg::ZoneBanner {
                act: c.act,
                name: r.name,
                subtitle: r.subtitle,
                level_min: r.level_min,
                level_max: r.level_max,
                pvp: r.pvp,
            });
        }
    }
}

/// Handle every beta-server-track `ClientMsg`. Called from the catch-all
/// arm of `handle_client_msg`.
pub(crate) fn handle_client(id: u64, msg: ClientMsg, world: &mut World, conns: &mut HashMap<u64, Conn>, db: &Db) {
    let Some((act, ent)) = crate::conn_entity(conns, id) else { return };
    let msg = clip_msg(msg);
    let mut guard = state();
    let st = &mut *guard;
    match st.flood.charge(id, st.now, flood_cost(&msg)) {
        Flood::Pass => {}
        Flood::Warn => return crate::notice(conns, id, "You are sending messages too quickly. Wait a few seconds.".into()),
        Flood::Drop => return,
    }
    st.ensure(id, conns, db);
    let me = name_of(conns, id);
    let say = |conns: &HashMap<u64, Conn>, r: Result<String, String>| match r {
        Ok(t) | Err(t) => crate::notice(conns, id, t),
    };

    match msg {
        // ── B05 quest boards ────────────────────────────────────────────
        ClientMsg::QuestBoard => match world.board(act, ent) {
            Ok((place, quests)) => {
                if let Some(c) = conns.get(&id) {
                    c.send(ServerMsg::QuestBoard { place, quests });
                }
            }
            Err(t) => crate::notice(conns, id, t),
        },
        ClientMsg::QuestAccept { id: qid } => {
            say(conns, world.board_accept(act, ent, &qid));
            crate::send_stats(world, conns, id);
        }
        ClientMsg::QuestTurnIn { id: qid } => {
            say(conns, world.board_turn_in(act, ent, &qid));
            crate::send_stats(world, conns, id);
        }
        ClientMsg::QuestAbandon { id: qid } => {
            say(conns, world.abandon_quest(act, ent, &qid));
            crate::send_stats(world, conns, id);
        }

        // ── B07 raids ───────────────────────────────────────────────────
        ClientMsg::RaidCreate => {
            if conns.get(&id).is_some_and(|c| c.party.is_some()) {
                return crate::notice(conns, id, "Leave your current group first.".into());
            }
            match st.raids.create(id) {
                Ok(group) => {
                    if let Some(c) = conns.get_mut(&id) {
                        c.party = Some(group);
                    }
                    world.set_party(id, Some(group));
                    crate::notice(conns, id, "You form a raid. Invite up to 39 others.".into());
                    st.broadcast_raid(conns, group);
                }
                Err(t) => crate::notice(conns, id, t),
            }
        }
        ClientMsg::RaidInvite { player } => {
            let Some(tid) = find_conn(conns, &player) else {
                return crate::notice(conns, id, "No such player online.".into());
            };
            if conns.get(&tid).is_some_and(|c| c.party.is_some()) {
                return crate::notice(conns, id, format!("{player} is already in a group."));
            }
            if !st.hears(tid, &me) {
                return crate::notice(conns, id, format!("{player} is ignoring you."));
            }
            match st.raids.invite(id, tid) {
                Ok(_) => {
                    crate::notice(conns, tid, format!("{me} invites you to a raid. Accept with RaidAccept."));
                    crate::notice(conns, id, format!("Invited {player} to the raid."));
                }
                Err(t) => crate::notice(conns, id, t),
            }
        }
        ClientMsg::RaidAccept => {
            if conns.get(&id).is_some_and(|c| c.party.is_some()) {
                return crate::notice(conns, id, "Leave your current group first.".into());
            }
            match st.raids.accept(id) {
                Ok(group) => {
                    if let Some(c) = conns.get_mut(&id) {
                        c.party = Some(group);
                    }
                    world.set_party(id, Some(group));
                    st.broadcast_raid(conns, group);
                }
                Err(t) => crate::notice(conns, id, t),
            }
        }
        ClientMsg::RaidLeave => {
            if st.drop_from_raid(id, world, conns) {
                crate::notice(conns, id, "You left the raid.".into());
            } else {
                crate::notice(conns, id, "You are not in a raid.".into());
            }
        }
        ClientMsg::RaidKick { player } => {
            let Some(tid) = find_conn(conns, &player) else {
                return crate::notice(conns, id, "No such player online.".into());
            };
            match st.raids.kick(id, tid) {
                Ok(dep) => {
                    world.set_party(tid, None);
                    if let Some(c) = conns.get_mut(&tid) {
                        c.party = None;
                        c.send(ServerMsg::Notice { text: "You were removed from the raid.".into() });
                        c.send(ServerMsg::RaidInfo { leader: String::new(), members: Vec::new() });
                    }
                    st.after_departure(&dep, world, conns);
                }
                Err(t) => crate::notice(conns, id, t),
            }
        }
        ClientMsg::RaidChat { text } => {
            let Some(group) = st.raids.raid_of(id) else {
                return crate::notice(conns, id, "You are not in a raid.".into());
            };
            let Some(text) = social::clean_chat(&text) else { return };
            let members = st.raids.raid(group).map(|r| r.members.clone()).unwrap_or_default();
            for m in members {
                if st.hears(m, &me) {
                    if let Some(c) = conns.get(&m) {
                        c.send(ServerMsg::ChannelMsg { channel: "raid".into(), from: me.clone(), text: text.clone() });
                    }
                }
            }
        }

        // ── B07 channels & whispers ─────────────────────────────────────
        ClientMsg::ChannelJoin { channel } => match social::canonical_channel(&channel) {
            Some(ch) => {
                st.channels.join(id, ch);
                st.send_channels(conns, id);
            }
            None => crate::notice(conns, id, "No such channel. Try general, trade, lfg or zone.".into()),
        },
        ClientMsg::ChannelLeave { channel } => match social::canonical_channel(&channel) {
            Some(ch) => {
                st.channels.leave(id, ch);
                st.send_channels(conns, id);
            }
            None => crate::notice(conns, id, "No such channel.".into()),
        },
        ClientMsg::ChannelList => st.send_channels(conns, id),
        ClientMsg::ChannelSay { channel, text } => {
            let Some(ch) = social::canonical_channel(&channel) else {
                return crate::notice(conns, id, "No such channel. Try general, trade, lfg or zone.".into());
            };
            if !st.channels.is_in(id, ch) {
                return crate::notice(conns, id, format!("Join the {ch} channel first."));
            }
            let Some(text) = social::clean_chat(&text) else { return };
            for to in st.channels.listeners(ch) {
                let Some(c) = conns.get(&to) else { continue };
                if !c.logged_in || (ch == "zone" && c.act != act) || !st.hears(to, &me) {
                    continue;
                }
                c.send(ServerMsg::ChannelMsg { channel: ch.to_string(), from: me.clone(), text: text.clone() });
            }
        }
        ClientMsg::Whisper { to, text } => {
            let Some(text) = social::clean_chat(&text) else { return };
            let Some(tid) = find_conn(conns, &to) else {
                return crate::notice(conns, id, format!("{} is not online.", to.trim()));
            };
            if tid == id {
                return crate::notice(conns, id, "You mutter to yourself.".into());
            }
            let their_name = name_of(conns, tid);
            if !st.hears(tid, &me) {
                return crate::notice(conns, id, format!("{their_name} is ignoring you."));
            }
            let msg = ServerMsg::Whisper { from: me.clone(), to: their_name, text };
            if let Some(c) = conns.get(&tid) {
                c.send(msg.clone());
            }
            if let Some(c) = conns.get(&id) {
                c.send(msg);
            }
        }

        // ── B07 friends & ignore ────────────────────────────────────────
        ClientMsg::FriendAdd { player } | ClientMsg::IgnoreAdd { player } if !db.character_exists(&player).unwrap_or(false) => {
            crate::notice(conns, id, format!("No character named {} exists.", player.trim()));
        }
        ClientMsg::FriendAdd { player } => {
            let r = st.lists.entry(id).or_default().add_friend(&me, &player);
            match r {
                Ok(name) => {
                    if let Err(e) = db.social_add(&me, KIND_FRIEND, &name) {
                        tracing::error!("friend add: {e}");
                    }
                    crate::notice(conns, id, format!("{name} added to your friends."));
                    st.send_friends(world, conns, id);
                }
                Err(t) => crate::notice(conns, id, t),
            }
        }
        ClientMsg::FriendRemove { player } => {
            if st.lists.entry(id).or_default().remove_friend(&player) {
                if let Err(e) = db.social_remove(&me, KIND_FRIEND, &player) {
                    tracing::error!("friend remove: {e}");
                }
                crate::notice(conns, id, format!("{} removed from your friends.", player.trim()));
                st.send_friends(world, conns, id);
            } else {
                crate::notice(conns, id, format!("{} is not on your friends list.", player.trim()));
            }
        }
        ClientMsg::IgnoreAdd { player } => {
            let r = st.lists.entry(id).or_default().add_ignore(&me, &player);
            match r {
                Ok(name) => {
                    // Ignoring also unfriends: keep the table in step.
                    if let Err(e) = db.social_remove(&me, KIND_FRIEND, &name).and_then(|_| db.social_add(&me, KIND_IGNORE, &name)) {
                        tracing::error!("ignore add: {e}");
                    }
                    crate::notice(conns, id, format!("You are now ignoring {name}."));
                    st.send_friends(world, conns, id);
                }
                Err(t) => crate::notice(conns, id, t),
            }
        }
        ClientMsg::IgnoreRemove { player } => {
            if st.lists.entry(id).or_default().remove_ignore(&player) {
                if let Err(e) = db.social_remove(&me, KIND_IGNORE, &player) {
                    tracing::error!("ignore remove: {e}");
                }
                crate::notice(conns, id, format!("You no longer ignore {}.", player.trim()));
                st.send_friends(world, conns, id);
            } else {
                crate::notice(conns, id, format!("You are not ignoring {}.", player.trim()));
            }
        }
        ClientMsg::FriendList => st.send_friends(world, conns, id),

        // ── B07 emotes & who ────────────────────────────────────────────
        ClientMsg::Emote { emote, target } => {
            // Review fix: only name-shaped targets (no control characters,
            // brackets or line breaks in text shown to other players).
            let target: Option<String> = target.as_deref().and_then(social::clean_name);
            let Some(text) = social::emote_text(&emote, &me, target.as_deref()) else {
                let known: Vec<&str> = social::EMOTES.iter().map(|(e, _, _)| *e).collect();
                return crate::notice(conns, id, format!("Unknown emote. Try: {}.", known.join(", ")));
            };
            let Some(pos) = world.player_pos(act, ent) else { return };
            let emote = emote.trim().trim_start_matches('/').to_lowercase();
            for (to, c) in conns.iter() {
                let (true, Some(e)) = (c.logged_in && c.act == act, c.entity) else { continue };
                let near = world.player_pos(act, e).is_some_and(|p| p.distance(pos) <= EMOTE_RANGE);
                if near && st.hears(*to, &me) {
                    c.send(ServerMsg::Emote { from: me.clone(), emote: emote.clone(), text: text.clone() });
                }
            }
        }
        ClientMsg::Who { filter } => {
            let all: Vec<WhoEntry> = conns
                .values()
                .filter(|c| c.logged_in)
                .filter_map(|c| {
                    let s = world.sheet_of(c.act, c.entity?)?;
                    Some(WhoEntry {
                        name: s.name.clone(),
                        level: s.level,
                        class: s.class.map(|k| k.as_str().to_string()),
                        act: c.act,
                        guild: s.guild.clone(),
                    })
                })
                .collect();
            let (players, total) = social::who_list(all, filter.as_deref().unwrap_or(""));
            if let Some(c) = conns.get(&id) {
                c.send(ServerMsg::WhoList { players, total });
            }
        }

        // ── B08 travel ──────────────────────────────────────────────────
        ClientMsg::FlightList => match world.flight_list(act, ent) {
            Ok((here, nodes)) => {
                if let Some(c) = conns.get(&id) {
                    c.send(ServerMsg::FlightPaths { here, nodes });
                }
                crate::send_stats(world, conns, id);
            }
            Err(t) => crate::notice(conns, id, t),
        },
        ClientMsg::FlightTake { node } => match world.flight_take(act, ent, &node) {
            Ok(f) => {
                if let Some(c) = conns.get_mut(&id) {
                    let moved = f.entity != ent || f.act != act;
                    c.act = f.act;
                    c.entity = Some(f.entity);
                    c.send(ServerMsg::Notice { text: f.text });
                    if moved {
                        // Same contract as Travel: a new entity id needs a
                        // fresh Welcome or the client's own id goes stale.
                        if let Some(sheet) = world.player_sheet(f.act, f.entity) {
                            c.send(ServerMsg::Welcome { entity_id: f.entity, character: sheet, is_dev: c.is_dev });
                        }
                    }
                }
                crate::send_stats(world, conns, id);
            }
            Err(t) => crate::notice(conns, id, t),
        },
        ClientMsg::ReleaseSpirit => {
            say(conns, world.release_spirit(act, ent));
            crate::send_stats(world, conns, id);
        }

        // ── B06 items ───────────────────────────────────────────────────
        ClientMsg::Repair => {
            say(conns, world.repair(act, ent));
            crate::send_stats(world, conns, id);
            if let (Some(c), Some((items, repair_cost))) = (conns.get(&id), world.gear_info(act, ent)) {
                c.send(ServerMsg::Gear { items, repair_cost });
            }
        }
        ClientMsg::GearInfo => {
            if let (Some(c), Some((items, repair_cost))) = (conns.get(&id), world.gear_info(act, ent)) {
                c.send(ServerMsg::Gear { items, repair_cost });
            }
        }

        // Everything older is handled in main.rs before this arm is reached.
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Out;
    use antediluvia_sim::world::new_character;
    use glam::Vec2;
    use tokio::sync::mpsc::{self, UnboundedReceiver};

    /// The tests share the process-wide state, so they run one at a time.
    fn serial() -> MutexGuard<'static, ()> {
        static LOCK: Mutex<()> = Mutex::new(());
        let g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
        *state() = State::default();
        g
    }

    struct Rig {
        world: World,
        conns: HashMap<u64, Conn>,
        db: Db,
        rx: HashMap<u64, UnboundedReceiver<Out>>,
        ticks: u64,
    }

    impl Rig {
        fn new() -> Self {
            Rig { world: World::new(77), conns: HashMap::new(), db: Db::open(":memory:").unwrap(), rx: HashMap::new(), ticks: 0 }
        }

        /// Log a character in the way main.rs does (saved sheet, spawned
        /// entity, logged-in Conn).
        fn login(&mut self, id: u64, name: &str, act: Act, pos: Vec2) {
            let mut s = new_character(name);
            s.act = act;
            s.x = pos.x;
            s.y = pos.y;
            s.max_health = 1_000_000;
            s.health = 1_000_000;
            self.db.save(&s, Some(&format!("apple-{name}"))).unwrap();
            let ent = self.world.spawn_player(id, s);
            let (tx, rx) = mpsc::unbounded_channel();
            self.conns.insert(id, Conn {
                out: tx,
                name: Some(name.to_string()),
                entity: Some(ent),
                act,
                logged_in: true,
                guild: None,
                is_dev: false,
                aoi: 1400.0,
                pending_duel: None,
                pending_guild_invite: None,
                party: None,
                pending_party_invite: None,
            });
            self.rx.insert(id, rx);
        }

        fn send(&mut self, id: u64, msg: ClientMsg) {
            handle_client(id, msg, &mut self.world, &mut self.conns, &self.db);
        }

        fn tick(&mut self) {
            tick(self.ticks, &mut self.world, &mut self.conns, &self.db);
            self.ticks += TICK_EVERY;
        }

        /// Let a whole flood window go by (refills every message budget).
        fn pass_window(&mut self) {
            self.ticks += social::FLOOD_WINDOW_TICKS;
            self.tick();
        }

        /// Everything queued for a connection since the last drain.
        fn drain(&mut self, id: u64) -> Vec<ServerMsg> {
            let mut out = Vec::new();
            if let Some(rx) = self.rx.get_mut(&id) {
                while let Ok(m) = rx.try_recv() {
                    if let Out::Msg(m) = m {
                        out.push(m);
                    }
                }
            }
            out
        }

        fn notices(&mut self, id: u64) -> Vec<String> {
            self.drain(id).into_iter().filter_map(|m| if let ServerMsg::Notice { text } = m { Some(text) } else { None }).collect()
        }

        fn logout(&mut self, id: u64) {
            if let Some(c) = self.conns.remove(&id) {
                if let Some(e) = c.entity {
                    self.world.remove_player(c.act, e);
                }
            }
            self.rx.remove(&id);
        }

        fn sheet(&self, id: u64) -> antediluvia_protocol::CharacterSheet {
            let c = &self.conns[&id];
            self.world.player_sheet(c.act, c.entity.unwrap()).unwrap()
        }
    }

    fn channel_lines(msgs: &[ServerMsg]) -> Vec<(String, String, String)> {
        msgs.iter()
            .filter_map(|m| match m {
                ServerMsg::ChannelMsg { channel, from, text } => Some((channel.clone(), from.clone(), text.clone())),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn friends_and_ignore_persist_across_sessions_and_show_presence() {
        let _g = serial();
        let mut r = Rig::new();
        r.login(1, "Cain", Act::Eden, Vec2::ZERO);
        r.login(2, "Abel", Act::Hermon, Vec2::ZERO);
        r.tick();
        r.drain(1);
        r.send(1, ClientMsg::FriendAdd { player: "abel".into() });
        r.send(1, ClientMsg::FriendAdd { player: "Nobody".into() });
        r.send(1, ClientMsg::IgnoreAdd { player: "Ghost".into() });
        let msgs = r.drain(1);
        let friends = msgs.iter().find_map(|m| if let ServerMsg::Friends { friends, .. } = m { Some(friends.clone()) } else { None }).unwrap();
        assert_eq!(friends.len(), 1);
        assert_eq!((friends[0].name.as_str(), friends[0].online, friends[0].act, friends[0].level), ("Abel", true, Some(Act::Hermon), 1));
        let notes: Vec<_> = msgs.iter().filter_map(|m| if let ServerMsg::Notice { text } = m { Some(text.as_str()) } else { None }).collect();
        assert!(notes.iter().any(|t| t.contains("No character named Nobody")), "{notes:?}");
        assert!(notes.iter().any(|t| t.contains("No character named Ghost")));
        // Offline presence, then a brand-new server process: the lists come back from the DB.
        r.logout(2);
        r.tick();
        assert!(r.notices(1).iter().any(|t| t.contains("Abel has gone offline")));
        r.login(3, "Lamech", Act::Eden, Vec2::ZERO);
        r.send(1, ClientMsg::IgnoreAdd { player: "Lamech".into() });
        r.drain(1);
        *state() = State::default();
        r.send(1, ClientMsg::FriendList);
        let msgs = r.drain(1);
        let (friends, ignored) = msgs.iter().find_map(|m| if let ServerMsg::Friends { friends, ignored } = m { Some((friends.clone(), ignored.clone())) } else { None }).unwrap();
        assert_eq!(friends.len(), 1);
        // Review fix: this used to read `!online && a || b`, which passed for
        // an online friend named "Abel".
        assert!(!friends[0].online, "Abel logged out, so he must show offline");
        assert!(friends[0].name.eq_ignore_ascii_case("abel"));
        assert_eq!(ignored, vec!["Lamech"]);
        // Friend logs back in: Cain is told.
        r.login(4, "Abel", Act::Flood, Vec2::ZERO);
        r.tick();
        assert!(r.notices(1).iter().any(|t| t.contains("Abel has come online")));
        // Remove both; the DB forgets them.
        r.send(1, ClientMsg::FriendRemove { player: "ABEL".into() });
        r.send(1, ClientMsg::IgnoreRemove { player: "lamech".into() });
        assert_eq!(r.db.social_load("Cain").unwrap(), (vec![], vec![]));
    }

    #[test]
    fn ignore_filters_whispers_channels_zone_chat_and_emotes() {
        let _g = serial();
        let mut r = Rig::new();
        r.login(1, "Cain", Act::Eden, Vec2::ZERO);
        r.login(2, "Abel", Act::Eden, Vec2::new(30.0, 0.0));
        r.login(3, "Seth", Act::Eden, Vec2::new(60.0, 0.0));
        r.tick();
        for id in [1, 2, 3] {
            r.drain(id);
        }
        // Before ignoring: whisper arrives and is echoed.
        r.send(1, ClientMsg::Whisper { to: "abel".into(), text: " hello ".into() });
        let got = r.drain(2);
        assert!(matches!(&got[..], [ServerMsg::Whisper { from, to, text }] if from == "Cain" && to == "Abel" && text == "hello"), "{got:?}");
        assert!(matches!(&r.drain(1)[..], [ServerMsg::Whisper { .. }]));
        assert!(hears(2, "Cain"));
        // Abel ignores Cain.
        r.send(2, ClientMsg::IgnoreAdd { player: "Cain".into() });
        r.drain(2);
        assert!(!hears(2, "CAIN"), "the zone-chat hook honours ignore");
        assert!(hears(3, "Cain"));
        r.send(1, ClientMsg::Whisper { to: "Abel".into(), text: "listen".into() });
        assert!(r.drain(2).is_empty(), "ignored whisper must not arrive");
        assert!(r.notices(1).iter().any(|t| t.contains("Abel is ignoring you")));
        // Channels: Seth hears Cain, Abel does not; Abel's own line reaches everyone.
        r.send(1, ClientMsg::ChannelSay { channel: "general".into(), text: "selling figs".into() });
        assert!(channel_lines(&r.drain(2)).is_empty());
        assert_eq!(channel_lines(&r.drain(3)), vec![("general".into(), "Cain".into(), "selling figs".into())]);
        assert_eq!(channel_lines(&r.drain(1)).len(), 1, "the speaker sees their own line");
        r.send(2, ClientMsg::ChannelSay { channel: "/1".into(), text: "peace".into() });
        assert_eq!(channel_lines(&r.drain(1)).len(), 1, "ignore is one-way");
        r.drain(3);
        r.drain(2);
        // Emotes and raid invites are filtered too.
        r.send(1, ClientMsg::Emote { emote: "wave".into(), target: Some("Abel".into()) });
        assert!(r.drain(2).is_empty());
        let seen = r.drain(3);
        assert!(matches!(&seen[..], [ServerMsg::Emote { from, emote, text }] if from == "Cain" && emote == "wave" && text == "Cain waves at Abel."), "{seen:?}");
        r.drain(1);
        r.send(1, ClientMsg::RaidCreate);
        r.send(1, ClientMsg::RaidInvite { player: "Abel".into() });
        assert!(r.drain(2).is_empty());
        // Lifting the ignore restores delivery.
        r.send(2, ClientMsg::IgnoreRemove { player: "Cain".into() });
        r.drain(2);
        r.send(1, ClientMsg::Whisper { to: "Abel".into(), text: "now?".into() });
        assert_eq!(r.drain(2).len(), 1);
        // Whisper edge cases.
        r.drain(1);
        r.send(1, ClientMsg::Whisper { to: "Nobody".into(), text: "hi".into() });
        r.send(1, ClientMsg::Whisper { to: "Cain".into(), text: "hi".into() });
        r.send(1, ClientMsg::Whisper { to: "Seth".into(), text: "   ".into() });
        let n = r.notices(1);
        assert!(n.iter().any(|t| t.contains("not online")) && n.iter().any(|t| t.contains("yourself")), "{n:?}");
        assert!(r.drain(3).is_empty(), "an empty whisper is dropped");
    }

    #[test]
    fn channels_join_leave_and_zone_scope() {
        let _g = serial();
        let mut r = Rig::new();
        r.login(1, "Cain", Act::Eden, Vec2::ZERO);
        r.login(2, "Abel", Act::Eden, Vec2::ZERO);
        r.login(3, "Seth", Act::Hermon, Vec2::ZERO);
        r.tick();
        for id in [1, 2, 3] {
            r.drain(id);
        }
        r.send(1, ClientMsg::ChannelList);
        let got = r.drain(1);
        assert!(matches!(&got[..], [ServerMsg::Channels { joined, available }] if joined == &["general", "trade", "zone"] && available.len() == 4), "{got:?}");
        // LFG is opt-in: speaking there needs a join, and only members hear it.
        r.send(1, ClientMsg::ChannelSay { channel: "lfg".into(), text: "tank lfg".into() });
        assert!(r.notices(1).iter().any(|t| t.contains("Join the lfg channel first")));
        r.send(1, ClientMsg::ChannelJoin { channel: "LFG".into() });
        r.send(3, ClientMsg::ChannelJoin { channel: "lfg".into() });
        r.drain(1);
        r.drain(3);
        r.send(1, ClientMsg::ChannelSay { channel: "lfg".into(), text: "tank lfg".into() });
        assert!(channel_lines(&r.drain(2)).is_empty());
        assert_eq!(channel_lines(&r.drain(3)).len(), 1, "LFG is world-wide");
        r.drain(1);
        // Zone: same act only.
        r.send(1, ClientMsg::ChannelSay { channel: "zone".into(), text: "wolves by the river".into() });
        assert_eq!(channel_lines(&r.drain(2)), vec![("zone".into(), "Cain".into(), "wolves by the river".into())]);
        assert!(channel_lines(&r.drain(3)).is_empty(), "another act must not hear zone chat");
        r.drain(1);
        // Trade is world-wide; leaving stops delivery and speech.
        r.send(3, ClientMsg::ChannelLeave { channel: "trade".into() });
        r.drain(3);
        r.send(1, ClientMsg::ChannelSay { channel: "trade".into(), text: "wts ore".into() });
        assert_eq!(channel_lines(&r.drain(2)).len(), 1);
        assert!(channel_lines(&r.drain(3)).is_empty());
        r.send(3, ClientMsg::ChannelSay { channel: "trade".into(), text: "x".into() });
        assert!(r.notices(3).iter().any(|t| t.contains("Join the trade channel first")));
        r.send(1, ClientMsg::ChannelJoin { channel: "guild".into() });
        assert!(r.notices(1).iter().any(|t| t.contains("No such channel")));
    }

    #[test]
    fn raid_lifecycle_links_the_sim_group_and_survives_legacy_party_leave() {
        let _g = serial();
        let mut r = Rig::new();
        for (id, name) in [(1, "Cain"), (2, "Abel"), (3, "Seth"), (4, "Enosh")] {
            r.login(id, name, Act::Eden, Vec2::new(id as f32 * 20.0, 0.0));
        }
        r.tick();
        for id in 1..=4 {
            r.drain(id);
        }
        r.send(2, ClientMsg::RaidInvite { player: "Seth".into() });
        assert!(r.notices(2).iter().any(|t| t.contains("not in a raid")));
        r.send(1, ClientMsg::RaidCreate);
        let group = r.conns[&1].party.expect("leader's party field names the raid");
        assert!(social::is_raid(group));
        r.send(1, ClientMsg::RaidInvite { player: "abel".into() });
        r.send(1, ClientMsg::RaidInvite { player: "Seth".into() });
        r.send(2, ClientMsg::RaidAccept);
        r.send(3, ClientMsg::RaidAccept);
        assert_eq!(r.conns[&2].party, Some(group));
        let info = r.drain(3).into_iter().rev().find_map(|m| if let ServerMsg::RaidInfo { leader, members } = m { Some((leader, members)) } else { None }).unwrap();
        assert_eq!(info, ("Cain".to_string(), vec!["Cain".to_string(), "Abel".to_string(), "Seth".to_string()]));
        // Only the leader invites or kicks.
        r.send(2, ClientMsg::RaidInvite { player: "Enosh".into() });
        assert!(r.notices(2).iter().any(|t| t.contains("Only the raid leader")));
        r.send(2, ClientMsg::RaidKick { player: "Seth".into() });
        assert!(r.notices(2).iter().any(|t| t.contains("Only the raid leader")));
        // Raid chat reaches members only.
        for id in 1..=4 {
            r.drain(id);
        }
        r.send(3, ClientMsg::RaidChat { text: "pull in 3".into() });
        for id in 1..=3 {
            assert_eq!(channel_lines(&r.drain(id)), vec![("raid".into(), "Seth".into(), "pull in 3".into())]);
        }
        assert!(r.drain(4).is_empty());
        // A member uses the old PartyLeave path (simulated: main.rs clears
        // the party field and the sim group): the next tick drops them.
        r.conns.get_mut(&3).unwrap().party = None;
        r.world.set_party(3, None);
        r.tick();
        let info = r.drain(1).into_iter().rev().find_map(|m| if let ServerMsg::RaidInfo { members, .. } = m { Some(members) } else { None }).unwrap();
        assert_eq!(info, vec!["Cain", "Abel"]);
        // Someone joins through the old PartyInvite path (party field set
        // to the raid's group id): the next tick seats them in the raid.
        r.conns.get_mut(&4).unwrap().party = Some(group);
        r.world.set_party(4, Some(group));
        r.tick();
        let info = r.drain(4).into_iter().rev().find_map(|m| if let ServerMsg::RaidInfo { members, .. } = m { Some(members) } else { None }).unwrap();
        assert_eq!(info, vec!["Cain", "Abel", "Enosh"]);
        // Kick, then the leader logs out: leadership passes, then the raid of one dissolves.
        r.send(1, ClientMsg::RaidKick { player: "Enosh".into() });
        assert_eq!(r.conns[&4].party, None);
        r.logout(1);
        r.tick();
        assert_eq!(r.conns[&2].party, None, "a raid of one dissolves and frees its last member");
        assert!(r.notices(2).iter().any(|t| t.contains("disbanded")));
        // Everyone is free to form a new one.
        r.send(2, ClientMsg::RaidCreate);
        assert!(r.conns[&2].party.is_some_and(social::is_raid));
        r.send(2, ClientMsg::RaidLeave);
        assert_eq!(r.conns[&2].party, None);
        r.send(2, ClientMsg::RaidLeave);
        assert!(r.notices(2).iter().any(|t| t.contains("not in a raid")));
    }

    #[test]
    fn a_full_raid_refuses_the_forty_first_by_either_path() {
        let _g = serial();
        let mut r = Rig::new();
        for id in 1..=42u64 {
            r.login(id, &format!("P{id}"), Act::Eden, Vec2::new(id as f32, 0.0));
        }
        r.tick();
        r.send(1, ClientMsg::RaidCreate);
        let group = r.conns[&1].party.unwrap();
        for id in 2..=40u64 {
            if id % 10 == 0 {
                r.pass_window(); // 39 invites are more than one flood window allows
            }
            r.send(1, ClientMsg::RaidInvite { player: format!("P{id}") });
            r.send(id, ClientMsg::RaidAccept);
            assert_eq!(r.conns[&id].party, Some(group), "P{id} seated");
        }
        r.pass_window();
        r.drain(1);
        r.send(1, ClientMsg::RaidInvite { player: "P41".into() });
        assert!(r.notices(1).iter().any(|t| t.contains("full")));
        // The legacy party path cannot squeeze a 41st in either.
        r.conns.get_mut(&42).unwrap().party = Some(group);
        r.world.set_party(42, Some(group));
        r.tick();
        assert_eq!(r.conns[&42].party, None);
        assert!(r.notices(42).iter().any(|t| t.contains("full")));
        let st = state();
        assert_eq!(st.raids.raid(group).unwrap().members.len(), 40);
    }

    #[test]
    fn who_and_emote_range() {
        let _g = serial();
        let mut r = Rig::new();
        r.login(1, "Cain", Act::Eden, Vec2::ZERO);
        r.login(2, "Abel", Act::Eden, Vec2::new(3000.0, 3000.0));
        r.login(3, "Seth", Act::Hermon, Vec2::ZERO);
        r.tick();
        for id in 1..=3 {
            r.drain(id);
        }
        r.send(1, ClientMsg::Who { filter: None });
        let got = r.drain(1);
        let ServerMsg::WhoList { players, total } = &got[0] else { panic!("{got:?}") };
        assert_eq!(*total, 3);
        assert_eq!(players.iter().map(|p| p.name.as_str()).collect::<Vec<_>>(), vec!["Abel", "Cain", "Seth"]);
        r.send(1, ClientMsg::Who { filter: Some("hermon".into()) });
        let got = r.drain(1);
        assert!(matches!(&got[..], [ServerMsg::WhoList { players, total: 1 }] if players[0].name == "Seth" && players[0].act == Act::Hermon));
        // Emotes: nearby only, and unknown emotes list the valid ones.
        r.send(1, ClientMsg::Emote { emote: "bow".into(), target: None });
        assert_eq!(r.drain(1).len(), 1);
        assert!(r.drain(2).is_empty(), "too far to see");
        assert!(r.drain(3).is_empty(), "another act");
        r.send(1, ClientMsg::Emote { emote: "backflip".into(), target: None });
        assert!(r.notices(1).iter().any(|t| t.contains("Unknown emote") && t.contains("wave")));
    }

    #[test]
    fn banners_fire_on_arrival_not_on_every_tick() {
        let _g = serial();
        let mut r = Rig::new();
        r.login(1, "Irad", Act::Eden, Vec2::new(900.0, -2600.0));
        let banners = |msgs: Vec<ServerMsg>| -> Vec<(String, bool)> {
            msgs.into_iter().filter_map(|m| if let ServerMsg::ZoneBanner { name, pvp, .. } = m { Some((name, pvp)) } else { None }).collect()
        };
        let here = travel::region_at(Act::Eden, Vec2::new(900.0, -2600.0), None);
        r.tick();
        assert_eq!(banners(r.drain(1)), vec![(here.name.clone(), false)], "login shows where you are");
        r.tick();
        r.tick();
        assert!(banners(r.drain(1)).is_empty(), "no repeats while standing still");
        // Into the Cainite capital, unaligned: a PvP banner.
        let ent = r.conns[&1].entity.unwrap();
        r.world.zones.get_mut(&Act::Eden).unwrap().entities.get_mut(&ent).unwrap().pos = Vec2::new(2400.0, 2400.0);
        r.tick();
        let b = banners(r.drain(1));
        assert_eq!(b.len(), 1);
        assert!(b[0].0.contains("Nod") && b[0].1);
        // Back to the inn: a sanctuary banner. Out into open country: silence.
        r.world.zones.get_mut(&Act::Eden).unwrap().entities.get_mut(&ent).unwrap().pos = Vec2::ZERO;
        r.tick();
        assert_eq!(banners(r.drain(1)), vec![("Inn of Eden".to_string(), false)]);
        let open = Vec2::new(3550.0, -3550.0);
        assert!(travel::region_at(Act::Eden, open, None).key.starts_with("act:"));
        r.world.zones.get_mut(&Act::Eden).unwrap().entities.get_mut(&ent).unwrap().pos = open;
        r.tick();
        assert!(banners(r.drain(1)).is_empty(), "leaving a place for open country is not an arrival");
    }

    #[test]
    fn flight_across_acts_rewelcomes_and_boards_repair_and_spirit_answer() {
        let _g = serial();
        let mut r = Rig::new();
        r.login(1, "Jabal", Act::Eden, Vec2::new(5.0, 5.0));
        r.tick();
        r.drain(1);
        // Flight master at the inn: routes, with the inn itself now known.
        r.send(1, ClientMsg::FlightList);
        let got = r.drain(1);
        let (here, nodes) = got.iter().find_map(|m| if let ServerMsg::FlightPaths { here, nodes } = m { Some((here.clone(), nodes.clone())) } else { None }).unwrap();
        assert_eq!(here.as_deref(), Some("inn_eden"));
        assert_eq!(nodes.len(), 25);
        assert_eq!(nodes.iter().filter(|n| n.known).count(), 1);
        r.send(1, ClientMsg::FlightTake { node: "inn_hermon".into() });
        assert!(r.notices(1).iter().any(|t| t.contains("not discovered")));
        // Learn Hermon's inn and carry the fare.
        let ent = r.conns[&1].entity.unwrap();
        {
            let s = r.world.zones.get_mut(&Act::Eden).unwrap().entities.get_mut(&ent).unwrap().sheet.as_mut().unwrap();
            s.discovered.push(travel::inn_name(Act::Hermon).to_string());
            s.gold = 60;
        }
        r.send(1, ClientMsg::FlightTake { node: "inn_hermon".into() });
        let got = r.drain(1);
        let welcome = got.iter().find_map(|m| if let ServerMsg::Welcome { entity_id, character, .. } = m { Some((*entity_id, character.clone())) } else { None }).expect("cross-act flight re-welcomes");
        assert_eq!(r.conns[&1].act, Act::Hermon);
        assert_eq!(r.conns[&1].entity, Some(welcome.0));
        assert_ne!(welcome.0, ent);
        assert_eq!((welcome.1.act, welcome.1.gold), (Act::Hermon, 35));
        assert_eq!(r.sheet(1).gold, 35);
        r.tick();
        assert!(r.drain(1).iter().any(|m| matches!(m, ServerMsg::ZoneBanner { act: Act::Hermon, name, .. } if name == "Inn of Hermon")));
        // Quest board: nothing at the inn; locked list at a POI.
        r.send(1, ClientMsg::QuestBoard);
        assert!(r.notices(1).iter().any(|t| t.contains("no notice board")));
        let all = antediluvia_sim::questgen::anchors();
        let a = all.iter().find(|a| {
            a.act == Act::Hermon && !a.cave && all.iter().all(|o| o.key == a.key || o.act != a.act || o.pos.distance(a.pos) > 500.0)
        }).unwrap();
        let ent = r.conns[&1].entity.unwrap();
        r.world.zones.get_mut(&Act::Hermon).unwrap().entities.get_mut(&ent).unwrap().pos = a.pos;
        r.send(1, ClientMsg::QuestBoard);
        let got = r.drain(1);
        let (place, quests) = got.iter().find_map(|m| if let ServerMsg::QuestBoard { place, quests } = m { Some((place.clone(), quests.clone())) } else { None }).unwrap();
        assert_eq!(place, a.name);
        assert_eq!(quests.len(), 4);
        assert!(quests.iter().all(|q| q.status == "locked"));
        r.send(1, ClientMsg::QuestAccept { id: quests[0].id.clone() });
        assert!(r.notices(1).iter().any(|t| t.contains("main story")));
        // With the story and level gates open, accept / abandon round-trips.
        {
            let s = r.world.zones.get_mut(&Act::Hermon).unwrap().entities.get_mut(&ent).unwrap().sheet.as_mut().unwrap();
            s.quests_done.push(antediluvia_sim::questgen::act_finale(Act::Hermon).to_string());
            s.level = 10;
        }
        r.send(1, ClientMsg::QuestAccept { id: quests[0].id.clone() });
        assert!(r.sheet(1).quests.contains_key(&quests[0].id));
        r.send(1, ClientMsg::QuestTurnIn { id: quests[0].id.clone() });
        r.send(1, ClientMsg::QuestAbandon { id: quests[0].id.clone() });
        assert!(r.sheet(1).quests.is_empty());
        assert!(r.notices(1).iter().any(|t| t.contains("Not done yet")));
        // Gear report, then repair at the inn after two deaths' wear.
        r.send(1, ClientMsg::GearInfo);
        let got = r.drain(1);
        assert!(matches!(&got[..], [ServerMsg::Gear { items, repair_cost: 0 }] if items.iter().any(|i| i.item == "animal_skins" && i.slot.as_deref() == Some("chest") && i.durability == 60 && i.max_durability == 60 && i.color == "#ffffff")), "{got:?}");
        {
            let e = r.world.zones.get_mut(&Act::Hermon).unwrap().entities.get_mut(&ent).unwrap();
            antediluvia_sim::durability::on_death(e);
            antediluvia_sim::durability::on_death(e);
            e.pos = Vec2::ZERO;
        }
        r.send(1, ClientMsg::Repair);
        let got = r.drain(1);
        assert!(got.iter().any(|m| matches!(m, ServerMsg::Notice { text } if text.contains("mends your gear for 3g"))), "{got:?}");
        assert!(got.iter().any(|m| matches!(m, ServerMsg::Gear { repair_cost: 0, .. })));
        assert_eq!(r.sheet(1).gold, 32);
        // Spirit release: refused alive, works dead.
        r.send(1, ClientMsg::ReleaseSpirit);
        assert!(r.notices(1).iter().any(|t| t.contains("not dead")));
        r.world.zones.get_mut(&Act::Hermon).unwrap().entities.get_mut(&ent).unwrap().health = 0;
        r.send(1, ClientMsg::ReleaseSpirit);
        assert!(r.notices(1).iter().any(|t| t.contains("release your spirit")));
        assert!(r.sheet(1).health > 0);
    }

    #[test]
    fn offline_rest_is_banked_once_and_announced() {
        let _g = serial();
        let mut r = Rig::new();
        let mut s = new_character("Sleeper");
        s.x = 0.0;
        s.y = 0.0;
        s.max_xp = 1000;
        s.last_logout = Some(1_000);
        offline_rest(&mut s, 1_000 + 8 * 3600);
        assert_eq!(s.rested_xp, 400);
        assert_eq!(s.last_logout, Some(1_000 + 8 * 3600), "the clock restarts at login");
        // A crash-and-relogin a minute later banks nothing more.
        let mut again = s.clone();
        offline_rest(&mut again, 1_000 + 8 * 3600 + 30);
        assert_eq!(again.rested_xp, 400);
        // The notice is delivered when the connection is first seen.
        offline_rest(&mut s.clone(), 1_000 + 16 * 3600);
        r.login(1, "Sleeper", Act::Eden, Vec2::ZERO);
        r.tick();
        assert!(r.notices(1).iter().any(|t| t.contains("You feel rested (+400")));
    }

    // ── Review regression tests ─────────────────────────────────────────

    #[test]
    fn chat_flood_is_throttled_per_connection_and_refills() {
        let _g = serial();
        let mut r = Rig::new();
        r.login(1, "Cain", Act::Eden, Vec2::ZERO);
        r.login(2, "Abel", Act::Eden, Vec2::new(30.0, 0.0));
        r.tick();
        r.drain(1);
        r.drain(2);
        let allowed = (social::FLOOD_BUDGET / social::FLOOD_COST_CHAT) as usize;
        for i in 0..200 {
            r.send(1, ClientMsg::ChannelSay { channel: "general".into(), text: format!("spam {i}") });
        }
        assert_eq!(channel_lines(&r.drain(2)).len(), allowed, "a spammer reaches listeners only up to the budget");
        let mine = r.drain(1);
        assert_eq!(channel_lines(&mine).len(), allowed);
        let warned = mine.iter().filter(|m| matches!(m, ServerMsg::Notice { text } if text.contains("too quickly"))).count();
        assert_eq!(warned, 1, "one warning per window, not one per dropped line");
        // Every other chat-shaped path shares the budget, and so do DB writes.
        r.send(1, ClientMsg::Whisper { to: "Abel".into(), text: "psst".into() });
        r.send(1, ClientMsg::Emote { emote: "wave".into(), target: None });
        r.send(1, ClientMsg::FriendAdd { player: "Abel".into() });
        assert!(r.drain(2).is_empty());
        assert_eq!(r.db.social_load("Cain").unwrap(), (vec![], vec![]), "a throttled friend add writes nothing");
        // The other connection is unaffected, and the window refills.
        r.send(2, ClientMsg::ChannelSay { channel: "general".into(), text: "quiet please".into() });
        assert_eq!(channel_lines(&r.drain(1)).len(), 1);
        r.drain(2);
        r.pass_window();
        r.send(1, ClientMsg::ChannelSay { channel: "general".into(), text: "sorry".into() });
        assert_eq!(channel_lines(&r.drain(2)).len(), 1);
    }

    #[test]
    fn oversized_and_hostile_fields_are_clipped_not_echoed() {
        let _g = serial();
        let mut r = Rig::new();
        r.login(1, "Cain", Act::Eden, Vec2::ZERO);
        r.login(2, "Abel", Act::Eden, Vec2::new(30.0, 0.0));
        r.tick();
        r.drain(1);
        r.drain(2);
        let big = "A".repeat(2_000_000);
        for m in [
            ClientMsg::FriendAdd { player: big.clone() },
            ClientMsg::IgnoreAdd { player: big.clone() },
            ClientMsg::RaidInvite { player: big.clone() },
            ClientMsg::Whisper { to: big.clone(), text: "hi".into() },
            ClientMsg::QuestAccept { id: big.clone() },
            ClientMsg::QuestAbandon { id: big.clone() },
            ClientMsg::FlightTake { node: big.clone() },
            ClientMsg::ChannelJoin { channel: big.clone() },
            ClientMsg::Who { filter: Some(big.clone()) },
        ] {
            r.send(1, m);
        }
        let out = r.drain(1);
        assert!(out.len() >= 8, "every request is answered: {}", out.len());
        for m in out.iter().filter(|m| !matches!(m, ServerMsg::Stats { .. })) {
            let n = serde_json::to_string(m).unwrap().len();
            assert!(n < 400, "a reply echoed {n} bytes of hostile input");
        }
        assert_eq!(r.db.social_load("Cain").unwrap(), (vec![], vec![]));
        // Chat bodies are capped by clean_chat; the channel name by clip.
        r.pass_window();
        r.send(1, ClientMsg::ChannelSay { channel: "general".into(), text: big.clone() });
        let got = channel_lines(&r.drain(2));
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].2.chars().count(), social::CHAT_MAX);
        // An emote target cannot inject a second line or a fake system tag.
        r.send(1, ClientMsg::Emote { emote: "wave".into(), target: Some("Abel.\n[System] Abel wins".into()) });
        let got = r.drain(2);
        assert!(matches!(&got[..], [ServerMsg::Emote { text, .. }] if !text.contains('\n') && !text.contains('[')), "{got:?}");
        r.drain(1);
        // Messages from a connection that is not logged in are ignored outright.
        r.conns.get_mut(&2).unwrap().logged_in = false;
        r.send(2, ClientMsg::ChannelSay { channel: "general".into(), text: "ghost".into() });
        r.send(2, ClientMsg::Repair);
        assert!(r.drain(1).is_empty() && r.drain(2).is_empty());
    }

    #[test]
    fn standing_still_costs_no_banner_work_and_moving_still_banners() {
        let _g = serial();
        let mut r = Rig::new();
        r.login(1, "Irad", Act::Eden, Vec2::ZERO);
        r.tick();
        assert_eq!(r.drain(1).iter().filter(|m| matches!(m, ServerMsg::ZoneBanner { .. })).count(), 1);
        assert_eq!(state().region.get(&1).map(|(_, k, p)| (k.clone(), *p)), Some(("inn:eden".to_string(), (0.0, 0.0))));
        // Moving inside the same region updates the cached position, silently.
        let ent = r.conns[&1].entity.unwrap();
        r.world.zones.get_mut(&Act::Eden).unwrap().entities.get_mut(&ent).unwrap().pos = Vec2::new(40.0, 0.0);
        r.tick();
        assert!(r.drain(1).is_empty());
        assert_eq!(state().region.get(&1).map(|(_, _, p)| *p), Some((40.0, 0.0)));
        // Leaving for a capital still banners.
        r.world.zones.get_mut(&Act::Eden).unwrap().entities.get_mut(&ent).unwrap().pos = Vec2::new(2400.0, 2400.0);
        r.tick();
        assert_eq!(r.drain(1).iter().filter(|m| matches!(m, ServerMsg::ZoneBanner { pvp: true, .. })).count(), 1);
    }
}
