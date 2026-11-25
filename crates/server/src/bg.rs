// Copyright (c) 2026 Daniel Bates / Bates LLC. All rights reserved.
//! B10 battleground on the server: queue, arena placement, the per-tick
//! match drive, honor payout and the battlemaster's wares.
//!
//! The arena is a walled-off stretch of `battleground::ARENA_ACT` (one slot
//! per running match), not a separate zone instance: the sim's zones are
//! keyed by act. Everyone inside is PvP-flagged; leaving, the end of the
//! match, or a disconnect puts the character back where they queued.

use std::collections::{BTreeMap, HashMap};
use std::sync::{Mutex, MutexGuard, OnceLock};

use antediluvia_protocol::{Act, BgInfo, BgRow, ClientMsg, ServerMsg};
use antediluvia_sim::battleground::{self as bgs, BgEvent, Match, Queue, Relic, ARENA_ACT, TEAM_NAMES};
use antediluvia_sim::world::{World, TICK_HZ};
use glam::Vec2;

use crate::Conn;

/// Where to put a player back, and their PvP flag before the match.
#[derive(Clone, Copy)]
struct Return {
    act: Act,
    x: f32,
    y: f32,
    pvp: bool,
}

#[derive(Default)]
pub(crate) struct Bg {
    queue: Queue,
    matches: Vec<Match>,
    returns: HashMap<u64, Return>,
    next_id: u32,
    ticks: u64,
}

fn bg() -> MutexGuard<'static, Bg> {
    static B: OnceLock<Mutex<Bg>> = OnceLock::new();
    B.get_or_init(|| {
        let mut b = Bg::default();
        // Beta: a 1v1 starts. ANTEDILUVIA_BG_MIN raises the per-side minimum.
        if let Some(n) = std::env::var("ANTEDILUVIA_BG_MIN").ok().and_then(|v| v.parse::<usize>().ok()) {
            b.queue.min_per_team = n.clamp(1, bgs::TEAM_MAX);
        }
        Mutex::new(b)
    })
    .lock()
    .unwrap_or_else(|p| p.into_inner())
}

/// Move a logged-in player to `act` at `pos`, as `Travel` does (new entity
/// id, re-welcome; the snapshot hub sees the act change and sends a
/// keyframe). Within the same act it is a plain placement.
fn relocate(world: &mut World, conns: &mut HashMap<u64, Conn>, id: u64, act: Act, pos: Vec2) -> bool {
    let Some((cur, ent)) = crate::conn_entity(conns, id) else { return false };
    if cur == act {
        return world.bg_place(act, ent, pos, true);
    }
    let Some(mut sheet) = world.player_sheet(cur, ent) else { return false };
    world.remove_player(cur, ent);
    sheet.act = act;
    sheet.x = pos.x;
    sheet.y = pos.y;
    if sheet.health <= 0 {
        sheet.health = sheet.max_health;
    }
    let new_ent = world.spawn_player(id, sheet.clone());
    if let Some(c) = conns.get_mut(&id) {
        c.act = act;
        c.entity = Some(new_ent);
        c.send(ServerMsg::Welcome { entity_id: new_ent, character: sheet, is_dev: c.is_dev });
    }
    true
}

fn send_info(conns: &HashMap<u64, Conn>, id: u64, info: BgInfo) {
    if let Some(c) = conns.get(&id) {
        c.send(ServerMsg::BgState { info });
    }
}

fn honor_of(world: &World, conns: &HashMap<u64, Conn>, id: u64) -> (i32, i32, String) {
    let s = crate::conn_entity(conns, id).and_then(|(a, e)| world.player_sheet(a, e));
    let (h, e) = s.as_ref().map(bgs::honor).unwrap_or((0, 0));
    (h, e, bgs::rank_of(e).1.to_string())
}

fn idle_info(world: &World, conns: &HashMap<u64, Conn>, id: u64, phase: &str, queued: u32) -> BgInfo {
    let (honor, honor_earned, rank) = honor_of(world, conns, id);
    BgInfo {
        phase: phase.into(),
        queued,
        score: [0, 0],
        my_team: 0,
        relic: String::new(),
        carrier: None,
        relic_x: 0.0,
        relic_y: 0.0,
        secs_left: 0,
        winner: None,
        rows: Vec::new(),
        honor,
        honor_earned,
        rank,
    }
}

fn match_info(m: &Match, world: &World, conns: &HashMap<u64, Conn>, id: u64) -> BgInfo {
    let mut info = idle_info(world, conns, id, if m.result.is_some() { "ended" } else { "live" }, 0);
    info.score = m.score;
    info.my_team = m.players.get(&id).map_or(0, |p| p.team);
    let (relic, carrier, pos) = match m.relic {
        Relic::Altar => ("altar", None, bgs::altar(m.slot)),
        Relic::Carried(c) => ("carried", m.players.get(&c).map(|p| p.name.clone()), m.players.get(&c).map_or(bgs::altar(m.slot), |p| p.last_pos)),
        Relic::Dropped { pos, .. } => ("dropped", None, pos),
    };
    info.relic = relic.into();
    info.carrier = carrier;
    info.relic_x = pos.x;
    info.relic_y = pos.y;
    info.secs_left = m.secs_left.max(0.0).ceil() as u32;
    info.winner = m.result.flatten();
    info.rows = m.players.values().map(|p| BgRow { name: p.name.clone(), team: p.team, caps: p.caps, kills: p.kills, deaths: p.deaths }).collect();
    info
}

/// Put `id` back where they queued and drop the arena PvP flag.
fn send_home(b: &mut Bg, world: &mut World, conns: &mut HashMap<u64, Conn>, id: u64) {
    let Some(r) = b.returns.remove(&id) else { return };
    relocate(world, conns, id, r.act, Vec2::new(r.x, r.y));
    if let Some((act, ent)) = crate::conn_entity(conns, id) {
        world.set_pvp_flag(act, ent, r.pvp);
    }
}

/// Client messages: true when handled here.
pub(crate) fn handle(id: u64, msg: &ClientMsg, world: &mut World, conns: &mut HashMap<u64, Conn>) -> bool {
    let Some((act, ent)) = crate::conn_entity(conns, id) else { return matches!(msg, ClientMsg::BgQueue | ClientMsg::BgLeave | ClientMsg::BgBuy { .. }) };
    let mut b = bg();
    match msg {
        ClientMsg::BgQueue => {
            if b.matches.iter().any(|m| m.players.contains_key(&id)) {
                crate::notice(conns, id, "You are already on the field.".into());
                return true;
            }
            let Some(sheet) = world.player_sheet(act, ent) else { return true };
            if sheet.health <= 0 {
                crate::notice(conns, id, "The dead cannot queue.".into());
                return true;
            }
            match b.queue.join(id, &sheet.name, sheet.faction.clone()) {
                Ok(n) => {
                    crate::notice(conns, id, format!("You join the queue for the Relic of Seth ({n} waiting)."));
                    let info = idle_info(world, conns, id, "queued", n as u32);
                    send_info(conns, id, info);
                }
                Err(e) => crate::notice(conns, id, e),
            }
            true
        }
        ClientMsg::BgLeave => {
            if b.queue.leave(id) {
                crate::notice(conns, id, "You leave the battleground queue.".into());
                let info = idle_info(world, conns, id, "idle", 0);
                send_info(conns, id, info);
                return true;
            }
            let b = &mut *b;
            if let Some(m) = b.matches.iter_mut().find(|m| m.players.contains_key(&id)) {
                let _ = m.leave(id);
                send_home(b, world, conns, id);
                crate::notice(conns, id, "You leave the field and return to where you were.".into());
                let info = idle_info(world, conns, id, "idle", 0);
                send_info(conns, id, info);
            } else {
                crate::notice(conns, id, "You are not queued.".into());
            }
            true
        }
        ClientMsg::BgBuy { item } => {
            if !world.at_inn(act, ent) {
                crate::notice(conns, id, "The battlemaster keeps his wares at the inn.".into());
                return true;
            }
            let r = match world.bg_sheet_mut(act, ent) {
                Some(s) => bgs::vendor_buy(s, item),
                None => return true,
            };
            match r {
                Ok(t) | Err(t) => crate::notice(conns, id, t),
            }
            crate::send_stats(world, conns, id);
            true
        }
        _ => false,
    }
}

/// A connection is going away: leave queue or match and restore the
/// character's place before main.rs saves the sheet.
pub(crate) fn before_disconnect(id: u64, world: &mut World, conns: &mut HashMap<u64, Conn>) {
    let mut g = bg();
    let b = &mut *g;
    b.queue.leave(id);
    if let Some(m) = b.matches.iter_mut().find(|m| m.players.contains_key(&id)) {
        let _ = m.leave(id);
    }
    send_home(b, world, conns, id);
}

/// Every sim tick: form matches, drive them, pay out and tidy up.
pub(crate) fn tick(world: &mut World, conns: &mut HashMap<u64, Conn>) {
    let dt = 1.0 / TICK_HZ as f32;
    let mut g = bg();
    let b = &mut *g;
    b.ticks += 1;
    let second = b.ticks % TICK_HZ == 0;

    // Queued players who logged off drop out.
    b.queue.entries.retain(|e| conns.get(&e.owner).is_some_and(|c| c.logged_in));
    if let Some(roster) = b.queue.tick(dt) {
        let used: Vec<u32> = b.matches.iter().map(|m| m.slot).collect();
        let slot = (0..16).find(|s| !used.contains(s));
        match slot {
            None => {
                // All arenas busy: put them back at the front of the queue.
                for (o, n, _) in roster.into_iter().rev() {
                    b.queue.entries.insert(0, bgs::QueueEntry { owner: o, name: n, lineage: None, waited: bgs::QUEUE_FILL_SECS });
                }
            }
            Some(slot) => {
                b.next_id += 1;
                let m = Match::new(b.next_id, slot, &roster);
                for (o, _, team) in &roster {
                    if let Some((act, ent)) = crate::conn_entity(conns, *o) {
                        if let Some(s) = world.player_sheet(act, ent) {
                            b.returns.insert(*o, Return { act, x: s.x, y: s.y, pvp: s.pvp });
                        }
                    }
                    relocate(world, conns, *o, ARENA_ACT, bgs::banner(slot, *team));
                    if let Some((act, ent)) = crate::conn_entity(conns, *o) {
                        world.set_pvp_flag(act, ent, true);
                    }
                    crate::notice(conns, *o, format!(
                        "The Relic of Seth begins: you fight for the {}s. Carry the relic from the altar to your banner. First to {} wins.",
                        TEAM_NAMES[*team as usize], bgs::SCORE_TO_WIN
                    ));
                }
                tracing::info!("battleground {} started in slot {} with {} players", m.id, slot, roster.len());
                b.matches.push(m);
            }
        }
    }
    if second {
        let n = b.queue.entries.len() as u32;
        let ids: Vec<u64> = b.queue.entries.iter().map(|e| e.owner).collect();
        for o in ids {
            let info = idle_info(world, conns, o, "queued", n);
            send_info(conns, o, info);
        }
    }

    let mut finished = Vec::new();
    for (mi, m) in b.matches.iter_mut().enumerate() {
        // Players who vanished (disconnect handled elsewhere) leave.
        let gone: Vec<u64> = m.players.keys().copied().filter(|o| !conns.get(o).is_some_and(|c| c.logged_in)).collect();
        for o in gone {
            let _ = m.leave(o);
        }
        // Players who travelled off the field (zone travel while on it) have
        // left too: otherwise a carrier could take the relic out of the arena
        // and lock it, or idle in town and still be paid. They stay where they
        // went; only their own PvP flag comes back.
        if m.result.is_none() {
            let walked: Vec<u64> = m
                .players
                .keys()
                .copied()
                .filter(|o| crate::conn_entity(conns, *o).is_some_and(|(act, _)| act != ARENA_ACT))
                .collect();
            for o in walked {
                let _ = m.leave(o);
                if let Some(r) = b.returns.remove(&o) {
                    if let Some((act, ent)) = crate::conn_entity(conns, o) {
                        world.set_pvp_flag(act, ent, r.pvp);
                    }
                }
                crate::notice(conns, o, "You left the field; the battleground goes on without you.".into());
                let info = idle_info(world, conns, o, "idle", 0);
                send_info(conns, o, info);
            }
        }
        let mut views = BTreeMap::new();
        for o in m.players.keys() {
            if let Some((act, ent)) = crate::conn_entity(conns, *o) {
                if act == ARENA_ACT {
                    world.bg_hold_dead(act, ent);
                    world.set_pvp_flag(act, ent, true);
                    if let Some(v) = world.bg_view(act, ent) {
                        views.insert(*o, v);
                    }
                }
            }
        }
        let was_over = m.result.is_some();
        let events = m.tick(dt, &views);
        for ev in &events {
            let line = match ev {
                BgEvent::Pickup { owner } => Some(format!("{} has taken the relic!", m.players.get(owner).map_or("Someone", |p| p.name.as_str()))),
                BgEvent::Dropped { owner, .. } => Some(format!("{} dropped the relic!", m.players.get(owner).map_or("The carrier".to_string(), |p| p.name.clone()))),
                BgEvent::Returned => Some("The relic returns to its altar.".to_string()),
                BgEvent::Captured { owner, team } => Some(format!(
                    "{} brings the relic home for the {}s! ({} - {})",
                    m.players.get(owner).map_or("Someone", |p| p.name.as_str()), TEAM_NAMES[*team as usize], m.score[0], m.score[1]
                )),
                BgEvent::Respawn { owner, pos } => {
                    if let Some((act, ent)) = crate::conn_entity(conns, *owner) {
                        world.bg_place(act, ent, *pos, true);
                    }
                    None
                }
                BgEvent::Ended { winner } => Some(match winner {
                    Some(t) => format!("The {}s win the Relic of Seth, {} - {}.", TEAM_NAMES[*t as usize], m.score[0], m.score[1]),
                    None => format!("The Relic of Seth ends in a draw, {} - {}.", m.score[0], m.score[1]),
                }),
            };
            if let Some(line) = line {
                for o in m.players.keys() {
                    crate::notice(conns, *o, line.clone());
                }
            }
        }
        if !was_over && m.result.is_some() {
            for (o, h) in m.honor_rewards() {
                if let Some((act, ent)) = crate::conn_entity(conns, o) {
                    if let Some(s) = world.bg_sheet_mut(act, ent) {
                        bgs::grant_honor(s, h);
                        let earned = bgs::honor(s).1;
                        crate::notice(conns, o, format!("+{h} honor. Rank: {}.", bgs::rank_of(earned).1));
                    }
                }
            }
        }
        if second || !events.is_empty() {
            for o in m.players.keys().copied().collect::<Vec<_>>() {
                let info = match_info(m, world, conns, o);
                send_info(conns, o, info);
            }
        }
        if m.result.is_some() && m.linger <= 0.0 {
            finished.push(mi);
        }
    }
    for mi in finished.into_iter().rev() {
        let m = b.matches.remove(mi);
        for o in m.players.keys().copied() {
            send_home(b, world, conns, o);
            let info = idle_info(world, conns, o, "idle", 0);
            send_info(conns, o, info);
        }
        tracing::info!("battleground {} closed: {:?}", m.id, m.score);
    }
}

/// Is this connection on a battleground field (for the PvP kill credit hook)?
pub(crate) fn note_kill(owner: u64) {
    let mut b = bg();
    if let Some(m) = b.matches.iter_mut().find(|m| m.players.contains_key(&owner)) {
        m.note_kill(owner);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Out;
    use antediluvia_sim::world::new_character;
    use tokio::sync::mpsc::{self, UnboundedReceiver};

    struct Rig {
        world: World,
        conns: HashMap<u64, Conn>,
        rx: HashMap<u64, UnboundedReceiver<Out>>,
    }

    impl Rig {
        fn new() -> Self {
            *bg() = Bg::default();
            Rig { world: World::new(5), conns: HashMap::new(), rx: HashMap::new() }
        }
        fn login(&mut self, id: u64, name: &str, act: Act, pos: Vec2, lineage: Option<&str>) {
            let mut s = new_character(name);
            s.act = act;
            s.x = pos.x;
            s.y = pos.y;
            s.faction = lineage.map(String::from);
            let ent = self.world.spawn_player(id, s);
            let (tx, rx) = mpsc::unbounded_channel();
            self.conns.insert(id, Conn {
                out: tx, name: Some(name.to_string()), entity: Some(ent), act, logged_in: true, guild: None, is_dev: false,
                aoi: 1400.0, pending_duel: None, pending_guild_invite: None, party: None, pending_party_invite: None, wire: 1, resync: false,
            });
            self.rx.insert(id, rx);
        }
        fn send(&mut self, id: u64, msg: ClientMsg) -> bool {
            handle(id, &msg, &mut self.world, &mut self.conns)
        }
        fn ticks(&mut self, n: usize) {
            for _ in 0..n {
                self.world.step();
                tick(&mut self.world, &mut self.conns);
            }
        }
        fn drain(&mut self, id: u64) -> Vec<ServerMsg> {
            let mut out = Vec::new();
            while let Ok(m) = self.rx.get_mut(&id).unwrap().try_recv() {
                if let Out::Msg(m) = m {
                    out.push(m);
                }
            }
            out
        }
        fn last_info(&mut self, id: u64) -> Option<BgInfo> {
            self.drain(id).into_iter().filter_map(|m| if let ServerMsg::BgState { info } = m { Some(info) } else { None }).last()
        }
        fn at(&self, id: u64) -> (Act, Vec2, bool, i32) {
            let (act, ent) = crate::conn_entity(&self.conns, id).unwrap();
            let s = self.world.player_sheet(act, ent).unwrap();
            (act, self.world.bg_view(act, ent).unwrap().pos, s.pvp, s.health)
        }
        fn walk_to(&mut self, id: u64, p: Vec2) {
            let (act, ent) = crate::conn_entity(&self.conns, id).unwrap();
            self.world.bg_place(act, ent, p, false);
        }
    }

    /// One test (the queue and arenas are process-wide state): queue, 1v1
    /// start, arena placement with PvP on, a capture carried end to end, a
    /// leave that forfeits, honor paid and the player put back where they
    /// queued with their own PvP flag; then a full 10v10 with a 21st waiting.
    #[test]
    fn queue_to_field_to_score_and_home_again() {
        let mut r = Rig::new();
        let home_a = Vec2::new(120.0, 40.0);
        r.login(1, "Seth", Act::Eden, home_a, Some("sethite"));
        r.login(2, "Cain", Act::Hermon, Vec2::new(-30.0, 60.0), Some("cainite"));
        assert!(r.send(1, ClientMsg::BgQueue));
        assert_eq!(r.last_info(1).unwrap().phase, "queued");
        assert!(r.send(2, ClientMsg::BgQueue));
        // Not yet: the first in waits QUEUE_FILL_SECS for more players.
        r.ticks(5);
        assert_eq!(r.at(1).0, Act::Eden);
        r.ticks((bgs::QUEUE_FILL_SECS * TICK_HZ as f32) as usize + 2);
        let (act, pos, pvp, _) = r.at(1);
        assert_eq!(act, ARENA_ACT);
        assert!(pos.distance(bgs::banner(0, 0)) < 1.0, "Sethite at the Sethite banner: {pos:?}");
        assert!(pvp, "everyone on the field is PvP-flagged");
        assert!(r.at(2).1.distance(bgs::banner(0, 1)) < 1.0);
        // A re-welcome came with the move (new entity id, binary keyframe follows).
        assert!(r.drain(2).iter().any(|m| matches!(m, ServerMsg::Welcome { .. })));
        // Seth runs the relic home.
        r.walk_to(1, bgs::altar(0));
        r.ticks(2);
        r.walk_to(1, bgs::banner(0, 0));
        r.ticks(2);
        let info = r.last_info(1).unwrap();
        assert_eq!((info.phase.as_str(), info.score), ("live", [1, 0]));
        assert_eq!(info.rows.len(), 2);
        // Cain leaves: he goes home at once and the Sethites win by forfeit.
        assert!(r.send(2, ClientMsg::BgLeave));
        let (act, pos, pvp, _) = r.at(2);
        assert_eq!(act, Act::Hermon);
        assert!(pos.distance(Vec2::new(-30.0, 60.0)) < 1.0);
        assert!(!pvp, "own PvP flag restored");
        r.ticks(2);
        let info = r.last_info(1).unwrap();
        assert_eq!((info.phase.as_str(), info.winner), ("ended", Some(0)));
        // A forfeit a few seconds in pays the capture, and almost nothing for
        // the win (alt-farming guard).
        assert!((25..=30).contains(&info.honor), "honor {}", info.honor);
        // After the linger the winner is home too.
        r.ticks((bgs::END_LINGER_SECS * TICK_HZ as f32) as usize + 2);
        let (act, pos, pvp, _) = r.at(1);
        assert_eq!(act, Act::Eden);
        assert!(pos.distance(home_a) < 1.0);
        assert!(!pvp);
        assert!(bg().matches.is_empty() && bg().returns.is_empty());

        // 21 queue: 20 go at once (10 a side), one keeps waiting.
        let mut r = Rig::new();
        for i in 0..21u64 {
            r.login(100 + i, &format!("P{i}"), Act::Eden, Vec2::new(50.0, 0.0), None);
            r.send(100 + i, ClientMsg::BgQueue);
        }
        r.ticks(1);
        let on_field = (0..21u64).filter(|i| r.at(100 + i).0 == ARENA_ACT).count();
        assert_eq!(on_field, 20);
        assert_eq!(bg().queue.entries.len(), 1);
        assert_eq!(bg().matches[0].team_size(0), 10);
        // Death on the field waits for the wave, not the 6 s inn respawn.
        let (act, ent) = crate::conn_entity(&r.conns, 100).unwrap();
        {
            // As a sim kill leaves it: dead with the 6 s inn timer running.
            let e = r.world.zones.get_mut(&act).unwrap().entities.get_mut(&ent).unwrap();
            e.health = 0;
            e.dead_timer = 6.0;
        }
        r.ticks((bgs::WAVE_SECS * TICK_HZ as f32) as usize + 2);
        let (act, pos, _, hp) = r.at(100);
        assert_eq!(act, ARENA_ACT, "still on the field");
        assert!(hp > 0, "risen with the wave");
        let team = bg().matches[0].players[&100].team;
        // Ten bodies share the banner and the sim nudges them apart.
        assert!(pos.distance(bgs::banner(0, team)) < 60.0, "{pos:?}");
        // A disconnect mid-match restores the place before the save.
        before_disconnect(101, &mut r.world, &mut r.conns);
        assert_eq!(r.at(101).0, Act::Eden);

        // Fresh process-wide state for the travel case (same test: shared state).
        *bg() = Bg::default();
        travelling_off_the_field_leaves_the_match();
    }

    /// Travelling to another zone mid-match leaves the match: the relic is
    /// freed, the traveller is not paid, and the match is not left waiting.
    fn travelling_off_the_field_leaves_the_match() {
        let mut r = Rig::new();
        r.login(11, "Enos", Act::Eden, Vec2::new(10.0, 10.0), Some("sethite"));
        r.login(12, "Irad", Act::Eden, Vec2::new(20.0, 10.0), Some("cainite"));
        r.send(11, ClientMsg::BgQueue);
        r.send(12, ClientMsg::BgQueue);
        r.ticks((bgs::QUEUE_FILL_SECS * TICK_HZ as f32) as usize + 2);
        assert_eq!(r.at(11).0, ARENA_ACT);
        let slot = bg().matches.iter().find(|m| m.players.contains_key(&11)).unwrap().slot;
        r.walk_to(11, bgs::altar(slot));
        r.ticks(2);
        assert!(bg().matches.iter().any(|m| m.relic == Relic::Carried(11)));
        // Enos carries the relic and travels away (as ClientMsg::Travel does).
        relocate(&mut r.world, &mut r.conns, 11, Act::Hermon, Vec2::ZERO);
        r.ticks(2);
        assert_eq!(r.at(11).0, Act::Hermon, "stays where he travelled");
        assert!(!bg().returns.contains_key(&11));
        let g = bg();
        assert!(g.matches.iter().all(|m| !m.players.contains_key(&11)), "still on the roster");
        drop(g);
        // Irad alone: the match ends by forfeit and closes after the linger.
        r.ticks((bgs::END_LINGER_SECS * TICK_HZ as f32) as usize + 4);
        assert!(bg().matches.is_empty() && bg().returns.is_empty());
    }

}
