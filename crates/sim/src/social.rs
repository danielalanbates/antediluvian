// Copyright (c) 2026 Daniel Bates / Bates LLC. All rights reserved.
//! B07 social rules: raid groups, chat channels, friends / ignore lists,
//! emotes and `/who` filtering.
//!
//! Everything here is plain data + rules with no I/O, so the server keeps
//! the live state and the browser single-player build can reuse the same
//! logic. Members are identified by their connection id (`u64`), exactly as
//! the sim's party table does.
//!
//! **Raid kill credit.** A raid is one shared-credit group in the sim
//! (`World::set_party` with the raid's group id), so the existing rule
//! applies at raid size: members within 900 units of the killer split the
//! kill XP evenly and each gains quest credit; loot, gold and reputation
//! stay with the killer; cave Dwellers drop their rare for every member
//! present. Raid group ids carry `RAID_BIT` so they can never collide with
//! a party id (which is a connection id).

use antediluvia_protocol::WhoEntry;
use std::collections::{BTreeMap, BTreeSet, HashMap};

pub const RAID_CAP: usize = 40;
/// Set on every raid's shared-credit group id.
pub const RAID_BIT: u64 = 1 << 62;
pub const FRIEND_CAP: usize = 100;
pub const IGNORE_CAP: usize = 100;
pub const WHO_CAP: usize = 50;
pub const CHAT_MAX: usize = 240;
/// Longest `/who` filter that is looked at.
pub const WHO_FILTER_MAX: usize = 32;
/// Flood control: each connection may spend `FLOOD_BUDGET` points per
/// `FLOOD_WINDOW_TICKS` sim ticks (5 s at 20 Hz). A chat line costs
/// `FLOOD_COST_CHAT`, so that is 12 lines per window.
pub const FLOOD_WINDOW_TICKS: u64 = 100;
pub const FLOOD_BUDGET: u32 = 48;
pub const FLOOD_COST_CHAT: u32 = 4;

pub fn is_raid(group: u64) -> bool {
    group & RAID_BIT != 0
}

/// Trim, drop control characters, cap the length. `None` if nothing is left.
pub fn clean_chat(text: &str) -> Option<String> {
    let t: String = text.chars().filter(|c| !c.is_control()).take(CHAT_MAX).collect();
    let t = t.trim().to_string();
    (!t.is_empty()).then_some(t)
}

// ─── Raids ───────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Raid {
    pub leader: u64,
    /// Join order; the leader is always a member.
    pub members: Vec<u64>,
}

/// What happened when someone left (or was removed from) a raid.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Departure {
    pub group: u64,
    /// Members still in the raid (empty if it disbanded).
    pub remaining: Vec<u64>,
    /// Set when leadership passed on.
    pub new_leader: Option<u64>,
    /// The raid fell to one member and was dissolved; that member is here.
    pub disbanded: Option<u64>,
}

#[derive(Default)]
pub struct RaidBook {
    raids: BTreeMap<u64, Raid>,
    of: HashMap<u64, u64>,
    /// invitee -> raid group id of the latest invite.
    invites: HashMap<u64, u64>,
    next: u64,
}

impl RaidBook {
    pub fn raid_of(&self, who: u64) -> Option<u64> {
        self.of.get(&who).copied()
    }

    pub fn raid(&self, group: u64) -> Option<&Raid> {
        self.raids.get(&group)
    }

    pub fn groups(&self) -> Vec<u64> {
        self.raids.keys().copied().collect()
    }

    /// Start a raid with `leader` as its only member.
    pub fn create(&mut self, leader: u64) -> Result<u64, String> {
        if self.of.contains_key(&leader) {
            return Err("You are already in a raid.".into());
        }
        self.next += 1;
        let group = RAID_BIT | self.next;
        self.raids.insert(group, Raid { leader, members: vec![leader] });
        self.of.insert(leader, group);
        Ok(group)
    }

    /// Leader invites `to`. The seat is only taken on accept.
    pub fn invite(&mut self, from: u64, to: u64) -> Result<u64, String> {
        let group = self.raid_of(from).ok_or("You are not in a raid. Create one first.")?;
        let raid = self.raids.get(&group).ok_or("That raid has disbanded.")?;
        if raid.leader != from {
            return Err("Only the raid leader can invite.".into());
        }
        if from == to {
            return Err("You cannot invite yourself.".into());
        }
        if self.of.contains_key(&to) {
            return Err("They are already in a raid.".into());
        }
        if raid.members.len() >= RAID_CAP {
            return Err(format!("The raid is full ({RAID_CAP})."));
        }
        self.invites.insert(to, group);
        Ok(group)
    }

    /// Take the latest invite.
    pub fn accept(&mut self, who: u64) -> Result<u64, String> {
        let group = self.invites.remove(&who).ok_or("No pending raid invite.")?;
        self.join(group, who)
    }

    /// Seat `who` in `group` (also used when the server adopts a member who
    /// arrived through the legacy party invite path).
    pub fn join(&mut self, group: u64, who: u64) -> Result<u64, String> {
        if self.of.contains_key(&who) {
            return Err("You are already in a raid.".into());
        }
        let raid = self.raids.get_mut(&group).ok_or("That raid has disbanded.")?;
        if raid.members.len() >= RAID_CAP {
            return Err(format!("The raid is full ({RAID_CAP})."));
        }
        raid.members.push(who);
        self.of.insert(who, group);
        Ok(group)
    }

    /// Remove `who` from their raid: leadership passes to the longest-
    /// standing member, and a raid of one dissolves.
    pub fn leave(&mut self, who: u64) -> Option<Departure> {
        self.invites.remove(&who);
        let group = self.of.remove(&who)?;
        let raid = self.raids.get_mut(&group)?;
        raid.members.retain(|m| *m != who);
        let mut new_leader = None;
        if raid.leader == who {
            if let Some(first) = raid.members.first().copied() {
                raid.leader = first;
                new_leader = Some(first);
            }
        }
        if raid.members.len() <= 1 {
            let last = raid.members.first().copied();
            if let Some(l) = last {
                self.of.remove(&l);
            }
            self.raids.remove(&group);
            self.invites.retain(|_, g| *g != group);
            return Some(Departure { group, remaining: Vec::new(), new_leader: None, disbanded: last });
        }
        Some(Departure { group, remaining: raid.members.clone(), new_leader, disbanded: None })
    }

    /// Leader removes a member.
    pub fn kick(&mut self, leader: u64, who: u64) -> Result<Departure, String> {
        let group = self.raid_of(leader).ok_or("You are not in a raid.")?;
        if self.raids.get(&group).map(|r| r.leader) != Some(leader) {
            return Err("Only the raid leader can remove members.".into());
        }
        if leader == who {
            return Err("Leave the raid instead.".into());
        }
        if self.raid_of(who) != Some(group) {
            return Err("They are not in your raid.".into());
        }
        self.leave(who).ok_or_else(|| "They are not in your raid.".to_string())
    }
}

// ─── Chat channels ───────────────────────────────────────────────────────────

/// Joinable channels. "zone" always means the act the speaker is in.
pub const CHANNELS: [&str; 4] = ["general", "trade", "lfg", "zone"];
/// Joined automatically at login.
pub const DEFAULT_CHANNELS: [&str; 3] = ["general", "trade", "zone"];

/// Resolve what a player typed ("/2", "#LFG", "LookingForGroup") to a channel.
pub fn canonical_channel(name: &str) -> Option<&'static str> {
    let n = name.trim().trim_start_matches(['/', '#']).to_lowercase();
    Some(match n.as_str() {
        "general" | "1" | "gen" => "general",
        "trade" | "2" => "trade",
        "lfg" | "3" | "lookingforgroup" => "lfg",
        "zone" | "4" | "local" => "zone",
        _ => return None,
    })
}

#[derive(Default)]
pub struct ChannelBook {
    subs: HashMap<u64, BTreeSet<&'static str>>,
}

impl ChannelBook {
    /// Subscribe to the default channels (login).
    pub fn join_defaults(&mut self, who: u64) {
        let set = self.subs.entry(who).or_default();
        for c in DEFAULT_CHANNELS {
            set.insert(c);
        }
    }

    /// True if this was a new subscription.
    pub fn join(&mut self, who: u64, channel: &'static str) -> bool {
        self.subs.entry(who).or_default().insert(channel)
    }

    /// True if they were subscribed.
    pub fn leave(&mut self, who: u64, channel: &str) -> bool {
        self.subs.get_mut(&who).is_some_and(|s| s.remove(channel))
    }

    pub fn is_in(&self, who: u64, channel: &str) -> bool {
        self.subs.get(&who).is_some_and(|s| s.contains(channel))
    }

    pub fn joined(&self, who: u64) -> Vec<String> {
        self.subs.get(&who).map(|s| s.iter().map(|c| c.to_string()).collect()).unwrap_or_default()
    }

    /// Everyone subscribed to a channel (the caller narrows "zone" by act).
    pub fn listeners(&self, channel: &str) -> Vec<u64> {
        let mut v: Vec<u64> = self.subs.iter().filter(|(_, s)| s.contains(channel)).map(|(w, _)| *w).collect();
        v.sort_unstable();
        v
    }

    pub fn drop_member(&mut self, who: u64) {
        self.subs.remove(&who);
    }
}

// ─── Friends & ignore ────────────────────────────────────────────────────────

/// One character's friends and ignore lists (lowercase key -> display name).
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct SocialList {
    friends: BTreeMap<String, String>,
    ignored: BTreeMap<String, String>,
}

fn name_key(name: &str) -> Result<(String, String), String> {
    let n = name.trim();
    if n.is_empty() || n.chars().count() > 24 {
        return Err("Names are 1-24 characters.".into());
    }
    Ok((n.to_lowercase(), n.to_string()))
}

impl SocialList {
    pub fn from_rows(friends: Vec<String>, ignored: Vec<String>) -> Self {
        let mut s = SocialList::default();
        for f in friends {
            s.friends.insert(f.to_lowercase(), f);
        }
        for i in ignored {
            s.ignored.insert(i.to_lowercase(), i);
        }
        s
    }

    pub fn friends(&self) -> Vec<String> {
        self.friends.values().cloned().collect()
    }

    pub fn ignored(&self) -> Vec<String> {
        self.ignored.values().cloned().collect()
    }

    pub fn is_friend(&self, name: &str) -> bool {
        self.friends.contains_key(&name.trim().to_lowercase())
    }

    /// Does this character refuse to hear `name`?
    pub fn ignores(&self, name: &str) -> bool {
        self.ignored.contains_key(&name.trim().to_lowercase())
    }

    pub fn add_friend(&mut self, me: &str, name: &str) -> Result<String, String> {
        let (key, display) = name_key(name)?;
        if key == me.trim().to_lowercase() {
            return Err("You cannot befriend yourself.".into());
        }
        if self.ignored.contains_key(&key) {
            return Err(format!("You are ignoring {display}. Stop ignoring them first."));
        }
        if self.friends.contains_key(&key) {
            return Err(format!("{display} is already your friend."));
        }
        if self.friends.len() >= FRIEND_CAP {
            return Err(format!("Your friends list is full ({FRIEND_CAP})."));
        }
        self.friends.insert(key, display.clone());
        Ok(display)
    }

    pub fn remove_friend(&mut self, name: &str) -> bool {
        self.friends.remove(&name.trim().to_lowercase()).is_some()
    }

    /// Ignoring someone also drops them from your friends.
    pub fn add_ignore(&mut self, me: &str, name: &str) -> Result<String, String> {
        let (key, display) = name_key(name)?;
        if key == me.trim().to_lowercase() {
            return Err("You cannot ignore yourself.".into());
        }
        if self.ignored.contains_key(&key) {
            return Err(format!("You are already ignoring {display}."));
        }
        if self.ignored.len() >= IGNORE_CAP {
            return Err(format!("Your ignore list is full ({IGNORE_CAP})."));
        }
        self.friends.remove(&key);
        self.ignored.insert(key, display.clone());
        Ok(display)
    }

    pub fn remove_ignore(&mut self, name: &str) -> bool {
        self.ignored.remove(&name.trim().to_lowercase()).is_some()
    }
}

// ─── Emotes ──────────────────────────────────────────────────────────────────

/// (emote, line with no target, line at a target). `{a}` = actor, `{t}` = target.
pub const EMOTES: &[(&str, &str, &str)] = &[
    ("wave", "{a} waves.", "{a} waves at {t}."),
    ("bow", "{a} bows.", "{a} bows before {t}."),
    ("cheer", "{a} cheers!", "{a} cheers for {t}!"),
    ("dance", "{a} dances.", "{a} dances with {t}."),
    ("laugh", "{a} laughs.", "{a} laughs with {t}."),
    ("cry", "{a} weeps.", "{a} weeps on {t}'s shoulder."),
    ("salute", "{a} salutes.", "{a} salutes {t}."),
    ("kneel", "{a} kneels.", "{a} kneels before {t}."),
    ("point", "{a} points ahead.", "{a} points at {t}."),
    ("thank", "{a} gives thanks.", "{a} thanks {t}."),
    ("greet", "{a} greets everyone.", "{a} greets {t}."),
    ("pray", "{a} bows the head in prayer.", "{a} prays for {t}."),
    ("sit", "{a} sits down.", "{a} sits beside {t}."),
    ("shrug", "{a} shrugs.", "{a} shrugs at {t}."),
    ("roar", "{a} roars!", "{a} roars at {t}!"),
    ("mourn", "{a} mourns the fallen.", "{a} mourns with {t}."),
];

/// The printable line for an emote, or `None` if the emote is unknown.
pub fn emote_text(emote: &str, actor: &str, target: Option<&str>) -> Option<String> {
    let key = emote.trim().trim_start_matches('/').to_lowercase();
    let (_, solo, at) = EMOTES.iter().find(|(e, _, _)| *e == key)?;
    Some(match target.map(str::trim).filter(|t| !t.is_empty()) {
        Some(t) => at.replace("{a}", actor).replace("{t}", t),
        None => solo.replace("{a}", actor),
    })
}

// ─── /who ────────────────────────────────────────────────────────────────────

/// Substring match on name / class / act / guild, or a level range "10-20"
/// (or a single level "15"). An empty filter matches everyone.
pub fn who_matches(e: &WhoEntry, filter: &str) -> bool {
    let f = filter.trim().to_lowercase();
    if f.is_empty() {
        return true;
    }
    if let Some((a, b)) = f.split_once('-') {
        if let (Ok(lo), Ok(hi)) = (a.trim().parse::<u32>(), b.trim().parse::<u32>()) {
            return (lo..=hi).contains(&e.level);
        }
    }
    if let Ok(n) = f.parse::<u32>() {
        return e.level == n;
    }
    e.name.to_lowercase().contains(&f)
        || e.class.as_deref().is_some_and(|c| c.contains(&f))
        || e.act.as_str().contains(&f)
        || e.guild.as_deref().is_some_and(|g| g.to_lowercase().contains(&f))
}

/// Filter, sort by name, cap. Returns (rows, total matches before the cap).
/// The filter is clipped to `WHO_FILTER_MAX` characters first: it arrives
/// straight off the wire and is re-examined for every online character.
pub fn who_list(all: Vec<WhoEntry>, filter: &str) -> (Vec<WhoEntry>, u32) {
    let filter: String = filter.trim().chars().take(WHO_FILTER_MAX).collect();
    let filter = filter.as_str();
    let mut hits: Vec<WhoEntry> = all.into_iter().filter(|e| who_matches(e, filter)).collect();
    hits.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    let total = hits.len() as u32;
    hits.truncate(WHO_CAP);
    (hits, total)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::{new_character, World};
    use antediluvia_protocol::{Act, EntityKind};
    use glam::Vec2;

    #[test]
    fn raid_fills_to_forty_and_no_further() {
        let mut b = RaidBook::default();
        assert!(b.invite(1, 2).is_err(), "no raid yet");
        let g = b.create(1).unwrap();
        assert!(is_raid(g));
        assert!(b.create(1).is_err());
        for m in 2..=40u64 {
            assert_eq!(b.invite(1, m), Ok(g));
            assert_eq!(b.accept(m), Ok(g));
        }
        assert_eq!(b.raid(g).unwrap().members.len(), RAID_CAP);
        assert!(b.invite(1, 41).unwrap_err().contains("full"));
        assert!(b.join(g, 41).unwrap_err().contains("full"), "the adopt path honours the cap too");
        assert!(b.raid_of(41).is_none());
        // A seat frees up; the next invite works.
        b.leave(40).unwrap();
        assert_eq!(b.invite(1, 41), Ok(g));
        assert_eq!(b.accept(41), Ok(g));
        assert_eq!(b.raid(g).unwrap().members.len(), RAID_CAP);
        assert!(b.accept(41).is_err(), "an invite is single-use");
    }

    #[test]
    fn raid_invites_leadership_kick_and_disband() {
        let mut b = RaidBook::default();
        let g = b.create(10).unwrap();
        b.invite(10, 11).unwrap();
        b.accept(11).unwrap();
        b.invite(10, 12).unwrap();
        b.accept(12).unwrap();
        assert!(b.invite(11, 13).unwrap_err().contains("leader"));
        assert!(b.invite(10, 11).unwrap_err().contains("already"));
        assert!(b.invite(10, 10).is_err());
        assert!(b.accept(99).is_err());
        // A second raid cannot poach a seated member.
        let g2 = b.create(20).unwrap();
        assert_ne!(g, g2);
        assert!(b.invite(20, 11).is_err());
        // Kick: leader only, members only.
        assert!(b.kick(11, 12).is_err());
        assert!(b.kick(10, 20).is_err());
        assert!(b.kick(10, 10).is_err());
        let d = b.kick(10, 12).unwrap();
        assert_eq!(d.remaining, vec![10, 11]);
        assert!(b.raid_of(12).is_none());
        // Leader leaves a three-raid: the next in line leads.
        b.invite(10, 12).unwrap();
        b.accept(12).unwrap();
        let d = b.leave(10).unwrap();
        assert_eq!((d.new_leader, d.remaining.clone(), d.disbanded), (Some(11), vec![11, 12], None));
        assert_eq!(b.raid(g).unwrap().leader, 11);
        // Down to one: the raid dissolves and frees the last member.
        let d = b.leave(12).unwrap();
        assert_eq!((d.disbanded, d.remaining.len()), (Some(11), 0));
        assert!(b.raid(g).is_none() && b.raid_of(11).is_none());
        assert!(b.leave(11).is_none());
        // A pending invite to a dissolved raid is void.
        let g3 = b.create(30).unwrap();
        b.invite(30, 31).unwrap();
        b.invite(30, 32).unwrap();
        b.accept(31).unwrap();
        b.leave(31).unwrap();
        assert!(b.raid(g3).is_none());
        assert!(b.accept(32).is_err());
        assert_eq!(b.groups(), vec![g2]);
    }

    /// Sim-level rule: a raid is one shared-credit group. Forty members near
    /// the kill split its XP evenly and each gains quest credit; loot, gold
    /// stay with the killer; a member out of range and an outsider get nothing.
    #[test]
    fn raid_of_forty_shares_kill_credit_within_range() {
        let mut w = World::new(42);
        let (eid, epos) = {
            let e = w.zones[&Act::Eden].entities.values()
                .find(|e| e.kind == EntityKind::Enemy && e.tag.as_deref() == Some("serpent")).expect("eden has serpents");
            (e.id, e.pos)
        };
        w.zones.get_mut(&Act::Eden).unwrap().entities.get_mut(&eid).unwrap().health = 1;
        let mut book = RaidBook::default();
        let group = book.create(1).unwrap();
        let mut ids = Vec::new();
        for owner in 1..=42u64 {
            let mut s = new_character(&format!("R{owner}"));
            // 1 = killer; 2..=39 nearby; 40 = raid member far away; 41, 42 = outsiders nearby.
            let off = if owner == 1 { Vec2::new(-12.0, 0.0) } else if owner == 40 { Vec2::new(-1500.0, 0.0) } else { Vec2::new(-60.0 - owner as f32, 30.0) };
            s.x = epos.x + off.x;
            s.y = epos.y + off.y;
            s.max_health = 1_000_000;
            s.health = 1_000_000;
            s.quests.insert("serpents_in_the_garden".into(), 0);
            // Isolation: discovery XP from a nearby POI is not kill credit.
            s.discovered = crate::pois::pois_for_act(Act::Eden).map(|p| p.name.clone())
                .chain(crate::caves::caves_for_act(Act::Eden).map(|c| c.name.clone())).collect();
            ids.push(w.spawn_player(owner, s));
            if (2..=40).contains(&owner) {
                book.invite(1, owner).unwrap();
                book.accept(owner).unwrap();
            }
            if owner <= 40 {
                w.set_party(owner, book.raid_of(owner));
            }
        }
        assert_eq!(book.raid(group).unwrap().members.len(), 40);
        let before: Vec<_> = ids.iter().map(|id| w.player_sheet(Act::Eden, *id).unwrap()).collect();
        for _ in 0..300 {
            if !w.zones[&Act::Eden].entities.contains_key(&eid) {
                break;
            }
            let tpos = w.zones[&Act::Eden].entities[&eid].pos;
            let p = w.zones.get_mut(&Act::Eden).unwrap().entities.get_mut(&ids[0]).unwrap();
            p.pos = tpos - Vec2::new(12.0, 0.0);
            p.rot = 0.0;
            w.queue_attack(Act::Eden, ids[0]);
            w.step();
        }
        assert!(!w.zones[&Act::Eden].entities.contains_key(&eid), "the serpent died");
        let after: Vec<_> = ids.iter().map(|id| w.player_sheet(Act::Eden, *id).unwrap()).collect();
        let credit = |i: usize| after[i].quests["serpents_in_the_garden"] - before[i].quests["serpents_in_the_garden"];
        let xp = |i: usize| (after[i].xp + after[i].level * 1000) - (before[i].xp + before[i].level * 1000);
        // Killer + 38 nearby members: equal XP share and quest credit each.
        let share = xp(0);
        assert!(share >= 1);
        for i in 0..39 {
            assert_eq!(credit(i), 1, "member {} missed quest credit", i + 1);
            assert_eq!(xp(i), share, "member {} got a different XP share", i + 1);
        }
        // Out-of-range raid member and both outsiders: nothing.
        for i in [39, 40, 41] {
            assert_eq!(credit(i), 0, "player {} must not get credit", i + 1);
            assert_eq!(xp(i), 0);
        }
        // Loot and kill gold go to the killer alone.
        assert!(after[0].gold > before[0].gold && after[0].inventory.len() > before[0].inventory.len());
        for i in 1..42 {
            assert_eq!(after[i].gold, before[i].gold);
            assert_eq!(after[i].inventory, before[i].inventory);
        }
    }

    #[test]
    fn channels_join_leave_and_listeners() {
        let mut c = ChannelBook::default();
        assert_eq!(canonical_channel(" /2 "), Some("trade"));
        assert_eq!(canonical_channel("#LFG"), Some("lfg"));
        assert_eq!(canonical_channel("LookingForGroup"), Some("lfg"));
        assert_eq!(canonical_channel("General"), Some("general"));
        assert_eq!(canonical_channel("local"), Some("zone"));
        assert_eq!(canonical_channel("guild"), None);
        c.join_defaults(1);
        c.join_defaults(2);
        assert_eq!(c.joined(1), vec!["general", "trade", "zone"]);
        assert!(!c.is_in(1, "lfg"), "LFG is opt-in");
        assert!(c.join(1, "lfg"));
        assert!(!c.join(1, "lfg"), "joining twice is a no-op");
        assert_eq!(c.listeners("lfg"), vec![1]);
        assert_eq!(c.listeners("general"), vec![1, 2]);
        assert!(c.leave(2, "general"));
        assert!(!c.leave(2, "general"));
        assert_eq!(c.listeners("general"), vec![1]);
        c.drop_member(1);
        assert!(c.listeners("general").is_empty() && c.joined(1).is_empty());
        for ch in DEFAULT_CHANNELS {
            assert!(CHANNELS.contains(&ch));
        }
    }

    #[test]
    fn friends_and_ignore_rules() {
        let mut s = SocialList::default();
        assert_eq!(s.add_friend("Cain", " Abel "), Ok("Abel".into()));
        assert!(s.is_friend("ABEL"));
        assert!(s.add_friend("Cain", "abel").unwrap_err().contains("already"));
        assert!(s.add_friend("Cain", "cain").is_err());
        assert!(s.add_friend("Cain", "").is_err());
        assert!(s.add_friend("Cain", &"x".repeat(25)).is_err());
        // Ignoring a friend drops the friendship; an ignored name cannot be befriended.
        assert_eq!(s.add_ignore("Cain", "Abel"), Ok("Abel".into()));
        assert!(!s.is_friend("Abel") && s.ignores("abel"));
        assert!(s.add_friend("Cain", "Abel").unwrap_err().contains("ignoring"));
        assert!(s.add_ignore("Cain", "Abel").is_err());
        assert!(s.add_ignore("Cain", "Cain").is_err());
        assert!(s.remove_ignore("ABEL"));
        assert!(!s.remove_ignore("Abel"));
        assert!(s.add_friend("Cain", "Abel").is_ok());
        assert!(s.remove_friend("abel") && !s.remove_friend("abel"));
        // Caps.
        for i in 0..FRIEND_CAP {
            s.add_friend("Cain", &format!("f{i}")).unwrap();
        }
        assert!(s.add_friend("Cain", "one_more").unwrap_err().contains("full"));
        for i in 0..IGNORE_CAP {
            s.add_ignore("Cain", &format!("i{i}")).unwrap();
        }
        assert!(s.add_ignore("Cain", "one_more").unwrap_err().contains("full"));
        // Round-trips through the DB row form.
        let back = SocialList::from_rows(s.friends(), s.ignored());
        assert_eq!(back, s);
    }

    #[test]
    fn emotes_render_with_and_without_a_target() {
        assert_eq!(emote_text("wave", "Seth", None).unwrap(), "Seth waves.");
        assert_eq!(emote_text("/BOW", "Seth", Some("Enosh")).unwrap(), "Seth bows before Enosh.");
        assert_eq!(emote_text("cheer", "Seth", Some("  ")).unwrap(), "Seth cheers!");
        assert!(emote_text("backflip", "Seth", None).is_none());
        assert!(EMOTES.len() >= 12);
        let mut names = std::collections::HashSet::new();
        for (e, solo, at) in EMOTES {
            assert!(names.insert(*e));
            assert!(solo.contains("{a}") && !solo.contains("{t}"));
            assert!(at.contains("{a}") && at.contains("{t}"));
            let line = emote_text(e, "A", Some("B")).unwrap();
            assert!(!line.contains('{'));
        }
    }

    fn who(name: &str, level: u32, class: Option<&str>, act: Act, guild: Option<&str>) -> WhoEntry {
        WhoEntry { name: name.into(), level, class: class.map(str::to_string), act, guild: guild.map(str::to_string) }
    }

    #[test]
    fn who_filters_sorts_and_caps() {
        let all = vec![
            who("Zillah", 12, Some("mage"), Act::Hermon, Some("Sons of Seth")),
            who("adah", 4, Some("hunter"), Act::Eden, None),
            who("Jubal", 30, None, Act::Enoch, Some("Lyre")),
        ];
        let names = |f: &str| who_list(all.clone(), f).0.into_iter().map(|e| e.name).collect::<Vec<_>>();
        assert_eq!(names(""), vec!["adah", "Jubal", "Zillah"]);
        assert_eq!(names("MAGE"), vec!["Zillah"]);
        assert_eq!(names("eden"), vec!["adah"]);
        assert_eq!(names("seth"), vec!["Zillah"]);
        assert_eq!(names("ub"), vec!["Jubal"]);
        assert_eq!(names("10-30"), vec!["Jubal", "Zillah"]);
        assert_eq!(names("4"), vec!["adah"]);
        assert!(names("nobody").is_empty());
        let crowd: Vec<_> = (0..120).map(|i| who(&format!("p{i:03}"), 1, None, Act::Eden, None)).collect();
        let (rows, total) = who_list(crowd, "");
        assert_eq!((rows.len(), total), (WHO_CAP, 120));
        assert_eq!(rows[0].name, "p000");
    }

    #[test]
    fn chat_is_cleaned_and_capped() {
        assert_eq!(clean_chat("  hello\u{7} world \n").as_deref(), Some("hello world"));
        assert!(clean_chat(" \t\n").is_none());
        assert_eq!(clean_chat(&"x".repeat(1000)).unwrap().len(), CHAT_MAX);
    }
}

// ─── Review additions (beta-server-track review) ─────────────────────────────

/// A character-name-shaped string from untrusted input (emote targets):
/// letters, digits, space, apostrophe and hyphen only, at most 24 characters.
/// Anything else is dropped so a target can never smuggle control characters
/// or a fake system line into the text other players are shown.
pub fn clean_name(text: &str) -> Option<String> {
    let t: String = text.chars().filter(|c| c.is_alphanumeric() || matches!(c, ' ' | '\'' | '-')).take(24).collect();
    let t = t.trim().to_string();
    (!t.is_empty()).then_some(t)
}

/// Outcome of charging a message against a connection's flood budget.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Flood {
    /// Within budget: handle the message.
    Pass,
    /// Over budget, first time this window: drop it and tell the sender.
    Warn,
    /// Over budget and already warned: drop it silently.
    Drop,
}

/// Per-connection message budget. Without it one client could make the
/// server fan a chat line out to every listener as fast as it can type
/// frames (O(listeners) work per message), or queue a database write per
/// friend-list toggle.
#[derive(Default)]
pub struct FloodGate {
    /// who -> (window start tick, points spent, warned).
    spent: HashMap<u64, (u64, u32, bool)>,
}

impl FloodGate {
    pub fn charge(&mut self, who: u64, now_tick: u64, cost: u32) -> Flood {
        let e = self.spent.entry(who).or_insert((now_tick, 0, false));
        if now_tick < e.0 || now_tick - e.0 >= FLOOD_WINDOW_TICKS {
            *e = (now_tick, 0, false);
        }
        if e.1.saturating_add(cost) > FLOOD_BUDGET {
            let first = !e.2;
            e.2 = true;
            return if first { Flood::Warn } else { Flood::Drop };
        }
        e.1 += cost;
        Flood::Pass
    }

    pub fn forget(&mut self, who: u64) {
        self.spent.remove(&who);
    }
}

#[cfg(test)]
mod review_tests {
    use super::*;
    use antediluvia_protocol::Act;

    #[test]
    fn flood_gate_caps_a_window_and_refills() {
        let mut g = FloodGate::default();
        let lines = FLOOD_BUDGET / FLOOD_COST_CHAT;
        for _ in 0..lines {
            assert_eq!(g.charge(1, 10, FLOOD_COST_CHAT), Flood::Pass);
        }
        assert_eq!(g.charge(1, 10, FLOOD_COST_CHAT), Flood::Warn, "the first refusal is announced");
        assert_eq!(g.charge(1, 50, FLOOD_COST_CHAT), Flood::Drop, "later ones are silent");
        assert_eq!(g.charge(2, 50, FLOOD_COST_CHAT), Flood::Pass, "budgets are per connection");
        assert_eq!(g.charge(1, 10 + FLOOD_WINDOW_TICKS, FLOOD_COST_CHAT), Flood::Pass, "a new window refills");
        // A cost larger than the whole budget can never pass, and cannot overflow.
        assert_eq!(g.charge(3, 0, u32::MAX), Flood::Warn);
        g.forget(1);
        assert_eq!(g.charge(1, 10 + FLOOD_WINDOW_TICKS, FLOOD_BUDGET), Flood::Pass);
    }

    #[test]
    fn hostile_strings_are_clipped_and_cleaned() {
        // A megabyte-long filter is clipped before it is matched per player.
        let all: Vec<WhoEntry> = (0..200)
            .map(|i| WhoEntry { name: format!("P{i}"), level: 1, class: None, act: Act::Eden, guild: None })
            .collect();
        let huge = "p".repeat(1_000_000);
        let (rows, total) = who_list(all.clone(), &huge);
        assert!(rows.is_empty() && total == 0);
        let (rows, total) = who_list(all, &format!("  P19{}", " ".repeat(100_000)));
        assert_eq!((total, rows.len()), (11, 11), "P19 and P190..P199");
        // Emote targets cannot carry line breaks or bracketed fake notices.
        assert_eq!(clean_name("Abel\n[System] You win"), Some("AbelSystem You win".into()));
        assert_eq!(clean_name(" \u{7}\n "), None);
        assert_eq!(clean_name("Tubal-Cain's kin and many many more words").unwrap().chars().count(), 24);
        // A raid book whose index and table disagree refuses instead of panicking.
        let mut b = RaidBook::default();
        let g = b.create(1).unwrap();
        b.raids.remove(&g);
        assert!(b.invite(1, 2).is_err());
        assert!(b.kick(1, 2).is_err());
    }
}
