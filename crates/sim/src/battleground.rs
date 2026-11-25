// Copyright (c) 2026 Daniel Bates / Bates LLC. All rights reserved.
//! B10 battleground: "The Relic of Seth", a capture-the-relic match.
//!
//! Pure match logic, no networking: the server feeds positions in and acts
//! on the events that come out. One relic rests on an altar in the middle of
//! the field; whoever carries it to their own team's banner scores. A carrier
//! who dies drops it; a dropped relic can be picked up by anyone and returns
//! to the altar after `RELIC_RETURN_SECS`. The dead rise together at their
//! banner every `WAVE_SECS`. First to `SCORE_TO_WIN`, or the higher score
//! when the timer runs out, wins.
//!
//! Teams follow lineage (Sethite = team 0, Cainite = team 1); the unaligned
//! and any overflow fill the smaller side. 10 a side at most. The match
//! starts as soon as both sides reach `min_per_team` (1 in beta) and the
//! oldest entry has waited `QUEUE_FILL_SECS` for more, or at once when 20
//! are waiting.
//!
//! Honor is kept in the existing `reputation` map ("honor" = spendable,
//! "honor_earned" = lifetime, which sets the rank): no new sheet fields.

use std::collections::BTreeMap;

use antediluvia_protocol::{Act, CharacterSheet, EntityId};
use glam::Vec2;

use crate::world::World;

pub const TEAM_MAX: usize = 10;
pub const MATCH_SECS: f32 = 480.0;
/// A match must run this long for its win / loss / kill honor to be paid in
/// full; a shorter one (an alt that queues and walks out at once) pays in
/// proportion. Captures are always paid: each is a real relic run.
pub const FULL_HONOR_SECS: f32 = 180.0;
pub const SCORE_TO_WIN: u32 = 3;
pub const WAVE_SECS: f32 = 12.0;
pub const RELIC_RETURN_SECS: f32 = 20.0;
pub const QUEUE_FILL_SECS: f32 = 15.0;
/// Seconds the scoreboard stays up after the end, before everyone leaves.
pub const END_LINGER_SECS: f32 = 12.0;
/// Reach for picking up the relic or scoring at a banner.
pub const TOUCH: f32 = 45.0;
/// The arenas lie in the far south-west of this act, away from its quests.
pub const ARENA_ACT: Act = Act::Nephilim;
pub const TEAM_NAMES: [&str; 2] = ["Sethite", "Cainite"];

/// Arena slot `slot`'s altar (the field centre).
pub fn altar(slot: u32) -> Vec2 {
    Vec2::new(-3150.0 + (slot % 8) as f32 * 800.0, -3250.0 + (slot / 8) as f32 * 700.0)
}

/// Team banner (spawn and capture point).
pub fn banner(slot: u32, team: u8) -> Vec2 {
    altar(slot) + Vec2::new(if team == 0 { -320.0 } else { 320.0 }, 0.0)
}

// ── queue ───────────────────────────────────────────────────────────────

#[derive(Clone, Debug)]
pub struct QueueEntry {
    pub owner: u64,
    pub name: String,
    pub lineage: Option<String>,
    pub waited: f32,
}

#[derive(Debug)]
pub struct Queue {
    pub entries: Vec<QueueEntry>,
    pub min_per_team: usize,
}

impl Default for Queue {
    fn default() -> Self {
        Self { entries: Vec::new(), min_per_team: 1 }
    }
}

impl Queue {
    pub fn join(&mut self, owner: u64, name: &str, lineage: Option<String>) -> Result<usize, String> {
        if self.entries.iter().any(|e| e.owner == owner) {
            return Err("You are already queued for the Relic of Seth.".into());
        }
        self.entries.push(QueueEntry { owner, name: name.to_string(), lineage, waited: 0.0 });
        Ok(self.entries.len())
    }

    pub fn leave(&mut self, owner: u64) -> bool {
        let n = self.entries.len();
        self.entries.retain(|e| e.owner != owner);
        n != self.entries.len()
    }

    pub fn contains(&self, owner: u64) -> bool {
        self.entries.iter().any(|e| e.owner == owner)
    }

    /// Advance waiting time; when a match can start, take its players out of
    /// the queue and return them with their teams.
    pub fn tick(&mut self, dt: f32) -> Option<Vec<(u64, String, u8)>> {
        for e in &mut self.entries {
            e.waited += dt;
        }
        let take = self.entries.len().min(TEAM_MAX * 2);
        if take < self.min_per_team * 2 {
            return None;
        }
        let full = take == TEAM_MAX * 2;
        let waited = self.entries.first().map_or(0.0, |e| e.waited) >= QUEUE_FILL_SECS;
        if !full && !waited {
            return None;
        }
        let teams = assign_teams(&self.entries[..take]);
        let (a, b) = (teams.iter().filter(|t| **t == 0).count(), teams.iter().filter(|t| **t == 1).count());
        if a < self.min_per_team || b < self.min_per_team {
            return None;
        }
        let picked: Vec<QueueEntry> = self.entries.drain(..take).collect();
        Some(picked.into_iter().zip(teams).map(|(e, t)| (e.owner, e.name, t)).collect())
    }
}

/// Lineage first (Sethite 0, Cainite 1), then the unaligned and overflow
/// fill the smaller side; the sides never differ by more than one.
pub fn assign_teams(entries: &[QueueEntry]) -> Vec<u8> {
    let mut team: Vec<Option<u8>> = entries
        .iter()
        .map(|e| match e.lineage.as_deref() {
            Some("sethite") => Some(0),
            Some("cainite") => Some(1),
            _ => None,
        })
        .collect();
    let count = |team: &[Option<u8>], t: u8| team.iter().filter(|x| **x == Some(t)).count();
    // Fill the unaligned into the smaller side, in queue order.
    for i in 0..team.len() {
        if team[i].is_none() {
            team[i] = Some(if count(&team, 0) <= count(&team, 1) { 0 } else { 1 });
        }
    }
    // Overflow: move the latest joiners of the bigger side across.
    loop {
        let (a, b) = (count(&team, 0), count(&team, 1));
        let (big, small) = if a > b + 1 { (0u8, a - b) } else if b > a + 1 { (1u8, b - a) } else { break };
        let _ = small;
        let i = team.iter().rposition(|x| *x == Some(big)).expect("bigger side has someone");
        team[i] = Some(1 - big);
    }
    team.into_iter().map(|t| t.unwrap_or(0)).collect()
}

// ── match ───────────────────────────────────────────────────────────────

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Relic {
    Altar,
    Carried(u64),
    Dropped { pos: Vec2, secs: f32 },
}

#[derive(Clone, Debug)]
pub struct BgPlayer {
    pub owner: u64,
    pub name: String,
    pub team: u8,
    pub caps: u32,
    pub kills: u32,
    pub deaths: u32,
    pub dead: bool,
    pub last_pos: Vec2,
}

/// Where a player is this tick, as the server sees it.
#[derive(Clone, Copy, Debug)]
pub struct View {
    pub pos: Vec2,
    pub alive: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub enum BgEvent {
    Pickup { owner: u64 },
    Dropped { owner: u64, pos: Vec2 },
    Returned,
    Captured { owner: u64, team: u8 },
    Respawn { owner: u64, pos: Vec2 },
    Ended { winner: Option<u8> },
}

#[derive(Debug)]
pub struct Match {
    pub id: u32,
    pub slot: u32,
    pub players: BTreeMap<u64, BgPlayer>,
    pub score: [u32; 2],
    pub relic: Relic,
    pub secs_left: f32,
    pub wave_in: f32,
    /// `Some(winner)` once over (`Some(None)` is a draw).
    pub result: Option<Option<u8>>,
    pub linger: f32,
}

impl Match {
    pub fn new(id: u32, slot: u32, roster: &[(u64, String, u8)]) -> Self {
        let players = roster
            .iter()
            .map(|(o, n, t)| (*o, BgPlayer { owner: *o, name: n.clone(), team: *t, caps: 0, kills: 0, deaths: 0, dead: false, last_pos: banner(slot, *t) }))
            .collect();
        Self { id, slot, players, score: [0, 0], relic: Relic::Altar, secs_left: MATCH_SECS, wave_in: WAVE_SECS, result: None, linger: END_LINGER_SECS }
    }

    pub fn team_size(&self, t: u8) -> usize {
        self.players.values().filter(|p| p.team == t).count()
    }

    /// The server credits PvP kills inside the arena here.
    pub fn note_kill(&mut self, killer: u64) {
        if let Some(p) = self.players.get_mut(&killer) {
            p.kills += 1;
        }
    }

    /// A player left (or disconnected). A carried relic drops where they were.
    pub fn leave(&mut self, owner: u64) -> Vec<BgEvent> {
        let mut ev = Vec::new();
        if let Some(p) = self.players.remove(&owner) {
            if self.relic == Relic::Carried(owner) {
                self.relic = Relic::Dropped { pos: p.last_pos, secs: 0.0 };
                ev.push(BgEvent::Dropped { owner, pos: p.last_pos });
            }
        }
        ev
    }

    fn end(&mut self, ev: &mut Vec<BgEvent>) {
        let w = match self.score[0].cmp(&self.score[1]) {
            std::cmp::Ordering::Greater => Some(0),
            std::cmp::Ordering::Less => Some(1),
            std::cmp::Ordering::Equal => {
                // A side that walked out forfeits.
                match (self.team_size(0), self.team_size(1)) {
                    (0, b) if b > 0 => Some(1),
                    (a, 0) if a > 0 => Some(0),
                    _ => None,
                }
            }
        };
        self.result = Some(w);
        ev.push(BgEvent::Ended { winner: w });
    }

    pub fn tick(&mut self, dt: f32, views: &BTreeMap<u64, View>) -> Vec<BgEvent> {
        let mut ev = Vec::new();
        if self.result.is_some() {
            self.linger -= dt;
            return ev;
        }
        self.secs_left -= dt;
        for p in self.players.values_mut() {
            if let Some(v) = views.get(&p.owner) {
                p.last_pos = v.pos;
                if !v.alive && !p.dead {
                    p.dead = true;
                    p.deaths += 1;
                } else if v.alive && p.dead {
                    p.dead = false; // revived some other way
                }
            }
        }
        // Relic.
        match self.relic {
            Relic::Carried(c) => {
                let carrier = self.players.get(&c).cloned();
                match carrier {
                    Some(p) if !p.dead => {
                        if p.last_pos.distance(banner(self.slot, p.team)) <= TOUCH {
                            self.score[p.team as usize] += 1;
                            if let Some(pm) = self.players.get_mut(&c) {
                                pm.caps += 1;
                            }
                            self.relic = Relic::Altar;
                            ev.push(BgEvent::Captured { owner: c, team: p.team });
                        }
                    }
                    Some(p) => {
                        self.relic = Relic::Dropped { pos: p.last_pos, secs: 0.0 };
                        ev.push(BgEvent::Dropped { owner: c, pos: p.last_pos });
                    }
                    None => self.relic = Relic::Altar,
                }
            }
            Relic::Dropped { pos, secs } => {
                let secs = secs + dt;
                if secs >= RELIC_RETURN_SECS {
                    self.relic = Relic::Altar;
                    ev.push(BgEvent::Returned);
                } else {
                    self.relic = Relic::Dropped { pos, secs };
                    if let Some(o) = self.nearest_alive(pos) {
                        self.relic = Relic::Carried(o);
                        ev.push(BgEvent::Pickup { owner: o });
                    }
                }
            }
            Relic::Altar => {
                if let Some(o) = self.nearest_alive(altar(self.slot)) {
                    self.relic = Relic::Carried(o);
                    ev.push(BgEvent::Pickup { owner: o });
                }
            }
        }
        // Respawn wave.
        self.wave_in -= dt;
        if self.wave_in <= 0.0 {
            self.wave_in += WAVE_SECS;
            for p in self.players.values_mut().filter(|p| p.dead) {
                p.dead = false;
                let pos = banner(self.slot, p.team);
                p.last_pos = pos;
                ev.push(BgEvent::Respawn { owner: p.owner, pos });
            }
        }
        let empty_side = self.team_size(0) == 0 || self.team_size(1) == 0;
        if self.score.iter().any(|s| *s >= SCORE_TO_WIN) || self.secs_left <= 0.0 || empty_side {
            self.end(&mut ev);
        }
        ev
    }

    fn nearest_alive(&self, at: Vec2) -> Option<u64> {
        self.players
            .values()
            .filter(|p| !p.dead && p.last_pos.distance(at) <= TOUCH)
            .min_by(|a, b| a.last_pos.distance(at).total_cmp(&b.last_pos.distance(at)))
            .map(|p| p.owner)
    }

    /// Honor each player earns when the match ends.
    pub fn honor_rewards(&self) -> Vec<(u64, i32)> {
        let w = self.result.flatten();
        self.players
            .values()
            .map(|p| {
                let base = match w {
                    Some(t) if t == p.team => 120,
                    Some(_) => 40,
                    None => 70,
                };
                // Win/loss/kill honor scales with how long the match ran,
                // so a forfeit farmed with an alt pays next to nothing.
                let played = (MATCH_SECS - self.secs_left).max(0.0);
                let scale = (played / FULL_HONOR_SECS).min(1.0);
                let earned = ((base + 5 * p.kills.min(20) as i32) as f32 * scale) as i32;
                (p.owner, earned + 25 * p.caps as i32)
            })
            .collect()
    }
}

// ── honor, ranks, vendor ────────────────────────────────────────────────

pub const RANKS: &[(i32, &str)] = &[
    (0, "Recruit"),
    (300, "Shieldbearer"),
    (900, "Spearman"),
    (2000, "Banner Captain"),
    (4500, "Warlord"),
];

/// Rank index and title for lifetime honor.
pub fn rank_of(earned: i32) -> (usize, &'static str) {
    let i = RANKS.iter().rposition(|(t, _)| earned >= *t).unwrap_or(0);
    (i, RANKS[i].1)
}

/// (item, honor cost, rank index needed).
pub const VENDOR: &[(&str, i32, usize)] = &[
    ("healing_potion", 40, 0),
    ("relic_bearers_band", 450, 1),
    ("gladiators_tabard", 900, 2),
    ("warbanner_blade", 1600, 3),
];

pub fn honor(s: &CharacterSheet) -> (i32, i32) {
    (*s.reputation.get("honor").unwrap_or(&0), *s.reputation.get("honor_earned").unwrap_or(&0))
}

pub fn grant_honor(s: &mut CharacterSheet, n: i32) {
    *s.reputation.entry("honor".into()).or_insert(0) += n;
    *s.reputation.entry("honor_earned".into()).or_insert(0) += n;
}

pub fn vendor_buy(s: &mut CharacterSheet, item: &str) -> Result<String, String> {
    let (_, cost, need) = *VENDOR.iter().find(|(i, _, _)| *i == item).ok_or("The battlemaster does not sell that.")?;
    let (have, earned) = honor(s);
    let (rank, _) = rank_of(earned);
    if rank < need {
        return Err(format!("Requires the rank of {}.", RANKS[need].1));
    }
    if have < cost {
        return Err(format!("Costs {cost} honor; you have {have}."));
    }
    *s.reputation.entry("honor".into()).or_insert(0) -= cost;
    s.inventory.push(item.to_string());
    Ok(format!("You receive {item} for {cost} honor."))
}

// ── world helpers ───────────────────────────────────────────────────────

impl World {
    /// Move a live player (alive or not) within its zone, refiling the grid.
    pub fn bg_place(&mut self, act: Act, id: EntityId, pos: Vec2, revive: bool) -> bool {
        let Some(e) = self.zones.get_mut(&act).and_then(|z| z.entities.get_mut(&id)) else { return false };
        e.pos = pos;
        e.origin = pos;
        e.intent = Vec2::ZERO;
        if revive && e.health <= 0 {
            e.health = e.max_health;
            e.dead_timer = 0.0;
            e.combat_timer = 0.0;
            e.attack_queued = false;
            e.cast_queued = None;
            if let Some(s) = e.sheet.as_mut() {
                s.health = e.health;
            }
        }
        if let Some(z) = self.zones.get_mut(&act) {
            z.refile_entity(id);
        }
        true
    }

    /// Arena dead wait for the respawn wave instead of the 6 s inn respawn.
    pub fn bg_hold_dead(&mut self, act: Act, id: EntityId) {
        if let Some(e) = self.zones.get_mut(&act).and_then(|z| z.entities.get_mut(&id)) {
            if e.health <= 0 {
                e.dead_timer = e.dead_timer.max(60.0);
            }
        }
    }

    /// Position and liveness of a player, for `Match::tick`.
    pub fn bg_view(&self, act: Act, id: EntityId) -> Option<View> {
        let e = self.zones.get(&act)?.entities.get(&id)?;
        Some(View { pos: e.pos, alive: e.health > 0 })
    }

    pub fn set_pvp_flag(&mut self, act: Act, id: EntityId, on: bool) {
        if let Some(s) = self.zones.get_mut(&act).and_then(|z| z.entities.get_mut(&id)).and_then(|e| e.sheet.as_mut()) {
            s.pvp = on;
        }
    }

    /// Mutable sheet access for the battleground's honor and vendor.
    pub fn bg_sheet_mut(&mut self, act: Act, id: EntityId) -> Option<&mut CharacterSheet> {
        self.zones.get_mut(&act)?.entities.get_mut(&id)?.sheet.as_mut()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn q(owner: u64, lin: Option<&str>) -> QueueEntry {
        QueueEntry { owner, name: format!("P{owner}"), lineage: lin.map(String::from), waited: 0.0 }
    }

    #[test]
    fn teams_follow_lineage_then_fill() {
        let e = vec![q(1, Some("sethite")), q(2, Some("sethite")), q(3, Some("sethite")), q(4, None), q(5, Some("cainite"))];
        let t = assign_teams(&e);
        assert_eq!(&t[..3], &[0, 0, 0]);
        assert_eq!(t[4], 1);
        assert_eq!(t[3], 1, "the unaligned fill the smaller side");
        // All one lineage: the latest joiners cross over so sides differ by <= 1.
        let e: Vec<QueueEntry> = (1..=7).map(|i| q(i, Some("cainite"))).collect();
        let t = assign_teams(&e);
        let a = t.iter().filter(|x| **x == 0).count();
        assert!(a == 3 || a == 4, "{t:?}");
        assert_eq!(t[0], 1, "the first in keeps their lineage");
    }

    #[test]
    fn queue_starts_at_the_beta_minimum_after_the_fill_wait_and_caps_at_ten() {
        let mut qu = Queue::default();
        qu.join(1, "A", None).unwrap();
        assert!(qu.join(1, "A", None).is_err());
        assert!(qu.tick(60.0).is_none(), "one player never starts");
        qu.join(2, "B", Some("cainite".into())).unwrap();
        assert!(qu.tick(1.0).is_some(), "1v1 starts once the first has waited");
        // 25 waiting: exactly 20 go at once, 5 stay.
        let mut qu = Queue::default();
        for i in 0..25 {
            qu.join(i, "x", None).unwrap();
        }
        let r = qu.tick(0.1).expect("full match starts at once");
        assert_eq!(r.len(), 20);
        assert_eq!(r.iter().filter(|p| p.2 == 0).count(), 10);
        assert_eq!(qu.entries.len(), 5);
        // A stricter minimum holds a 1v1 back.
        let mut qu = Queue { min_per_team: 3, ..Default::default() };
        qu.join(1, "A", None).unwrap();
        qu.join(2, "B", None).unwrap();
        assert!(qu.tick(100.0).is_none());
        assert!(qu.leave(2) && !qu.leave(2));
    }

    fn views(m: &Match, at: &[(u64, Vec2, bool)]) -> BTreeMap<u64, View> {
        let mut v: BTreeMap<u64, View> = m.players.values().map(|p| (p.owner, View { pos: p.last_pos, alive: !p.dead })).collect();
        for (o, p, a) in at {
            v.insert(*o, View { pos: *p, alive: *a });
        }
        v
    }

    #[test]
    fn carry_capture_drop_return_and_win() {
        let mut m = Match::new(1, 0, &[(1, "Seth".into(), 0), (2, "Cain".into(), 1)]);
        let a = altar(0);
        // Seth reaches the altar and takes the relic.
        let ev = m.tick(0.1, &views(&m, &[(1, a, true)]));
        assert!(ev.contains(&BgEvent::Pickup { owner: 1 }));
        // Walking to the enemy banner does not score.
        assert!(m.tick(0.1, &views(&m, &[(1, banner(0, 1), true)])).is_empty());
        // Own banner scores and the relic goes home.
        let ev = m.tick(0.1, &views(&m, &[(1, banner(0, 0), true)]));
        assert!(ev.contains(&BgEvent::Captured { owner: 1, team: 0 }));
        assert_eq!((m.score, m.relic), ([1, 0], Relic::Altar));
        // Seth takes it again and dies halfway: it drops there.
        m.tick(0.1, &views(&m, &[(1, a, true)]));
        let mid = a + Vec2::new(-150.0, 0.0);
        let ev = m.tick(0.1, &views(&m, &[(1, mid, false)]));
        assert!(ev.contains(&BgEvent::Dropped { owner: 1, pos: mid }));
        assert_eq!(m.players[&1].deaths, 1);
        // Cain picks it up from the ground.
        let ev = m.tick(0.1, &views(&m, &[(1, mid, false), (2, mid + Vec2::new(10.0, 0.0), true)]));
        assert!(ev.contains(&BgEvent::Pickup { owner: 2 }));
        // Cain dies too; nobody touches it; it returns after the timer.
        m.tick(0.1, &views(&m, &[(1, mid, false), (2, mid, false)]));
        let mut returned = false;
        for _ in 0..((RELIC_RETURN_SECS / 1.0) as usize + 1) {
            let mut far = views(&m, &[]);
            for v in far.values_mut() {
                v.pos = Vec2::new(0.0, 0.0);
            }
            returned |= m.tick(1.0, &far).contains(&BgEvent::Returned);
        }
        assert!(returned && m.relic == Relic::Altar);
        // Respawn waves brought both back at their banners.
        assert!(!m.players[&1].dead && !m.players[&2].dead);
        // Two more Sethite captures win it.
        for _ in 0..2 {
            m.tick(0.1, &views(&m, &[(1, a, true)]));
            m.tick(0.1, &views(&m, &[(1, banner(0, 0), true)]));
        }
        assert_eq!(m.result, Some(Some(0)));
        m.secs_left = 0.0; // pay as for a full-length match
        let h: BTreeMap<u64, i32> = m.honor_rewards().into_iter().collect();
        assert_eq!(h[&1], 120 + 25 * 3);
        assert_eq!(h[&2], 40);
    }

    #[test]
    fn respawn_wave_timer_and_timeout_and_forfeit() {
        let mut m = Match::new(1, 3, &[(1, "A".into(), 0), (2, "B".into(), 1)]);
        let ev = m.tick(0.5, &views(&m, &[(2, Vec2::ZERO, false)]));
        assert!(ev.is_empty());
        let mut risen = None;
        for _ in 0..30 {
            for e in m.tick(0.5, &views(&m, &[])) {
                if let BgEvent::Respawn { owner, pos } = e {
                    risen = Some((owner, pos));
                }
            }
        }
        assert_eq!(risen, Some((2, banner(3, 1))));
        // Timer runs out level: a draw.
        let ev = m.tick(MATCH_SECS, &views(&m, &[]));
        assert!(ev.contains(&BgEvent::Ended { winner: None }));
        // A side that leaves forfeits.
        let mut m = Match::new(2, 1, &[(1, "A".into(), 0), (2, "B".into(), 1)]);
        m.tick(0.1, &views(&m, &[(2, altar(1), true)]));
        let ev = m.leave(2);
        assert!(matches!(ev[0], BgEvent::Dropped { owner: 2, .. }));
        let ev = m.tick(0.1, &views(&m, &[]));
        assert!(ev.contains(&BgEvent::Ended { winner: Some(0) }));
    }

    #[test]
    fn honor_ranks_and_vendor() {
        let mut s = crate::world::new_character("Honor");
        assert_eq!(rank_of(0).1, "Recruit");
        assert!(vendor_buy(&mut s, "relic_bearers_band").unwrap_err().contains("Shieldbearer"));
        grant_honor(&mut s, 350);
        assert_eq!(rank_of(honor(&s).1).1, "Shieldbearer");
        assert!(vendor_buy(&mut s, "relic_bearers_band").unwrap_err().contains("Costs 450"));
        grant_honor(&mut s, 200);
        vendor_buy(&mut s, "relic_bearers_band").unwrap();
        assert!(s.inventory.iter().any(|i| i == "relic_bearers_band"));
        // Spending keeps the rank: rank follows lifetime honor.
        assert_eq!(honor(&s), (100, 550));
        assert_eq!(rank_of(550).1, "Shieldbearer");
        assert!(vendor_buy(&mut s, "nope").is_err());
        // Every vendor item is a real item or a known consumable.
        for (item, _, _) in VENDOR {
            assert!(crate::world::ITEMS.iter().any(|d| d.id == *item) || *item == "healing_potion", "{item}");
        }
    }

    #[test]
    fn arenas_fit_inside_the_world_and_do_not_overlap() {
        let b = antediluvia_protocol::WORLD_BOUNDS - 10.0;
        for slot in 0..16 {
            for t in 0..2 {
                let p = banner(slot, t);
                assert!(p.x.abs() < b && p.y.abs() < b, "slot {slot} {p:?}");
            }
            if slot > 0 {
                assert!(altar(slot).distance(altar(slot - 1)) >= 700.0);
            }
        }
    }

    #[test]
    fn an_instant_forfeit_pays_almost_no_honor() {
        let roster = vec![(1u64, "Main".to_string(), 0u8), (2u64, "Alt".to_string(), 1u8)];
        let mut m = Match::new(1, 0, &roster);
        let views = BTreeMap::new();
        m.tick(1.0, &views);
        m.leave(2);
        m.tick(0.1, &views);
        assert_eq!(m.result, Some(Some(0)));
        let h: BTreeMap<u64, i32> = m.honor_rewards().into_iter().collect();
        assert!(h[&1] <= 1, "farmed forfeit paid {}", h[&1]);
        // A match that ran its full course pays the whole win.
        let mut m = Match::new(2, 0, &roster);
        m.secs_left = MATCH_SECS - FULL_HONOR_SECS - 1.0;
        m.leave(2);
        m.tick(0.1, &views);
        let h: BTreeMap<u64, i32> = m.honor_rewards().into_iter().collect();
        assert_eq!(h[&1], 120);
    }

}
