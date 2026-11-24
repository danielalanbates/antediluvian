// Copyright (c) 2026 Daniel Bates / Bates LLC. All rights reserved.
//! B05 quest scale: POI- and cave-anchored generated quests.
//!
//! Every POI (200) posts four quests - kill / collect / explore / deliver -
//! and every cave (30) posts five (guard kill, ore collect, dweller kill,
//! explore, deliver). All 950 are built deterministically at first use from
//! the embedded `assets/data` tables (pois, caves, mobs); nothing is random
//! at runtime and nothing touches the filesystem.
//!
//! Design rules this module keeps:
//! * Generated quests are NOT offered through `talk()`. Their giver key is
//!   `"board"`, which no NPC carries, so they can never hijack an NPC's offer
//!   order. Players pick them up from the notice board at the place itself
//!   (`World::board` / `board_accept` / `board_turn_in`).
//! * Every generated quest additionally requires its act's main finale, so a
//!   fresh character sees none of them before the authored chain is done.
//! * Kill/collect targets are guaranteed to exist: each POI outside the
//!   starting areas owns a small camp of one bestiary species
//!   (`seed_camps`), and quests only ever target a camp species, a spawn
//!   table tag, or a cave tag that `populate_zone` creates.
//! * Anchors are addressed by key ("poi:<id>" / "cave:<id>") because POI and
//!   cave display names repeat.

use crate::quests::{quest_faction, quest_theme, Objective, QuestDef, QUESTS, QUEST_CAP};
use crate::world::{act_spawn_table, SimEvent, World, Zone, GATE_OF_EDEN, INN_RADIUS};
use antediluvia_protocol::{Act, BoardQuest, EntityId, EntityKind, WORLD_BOUNDS};
use glam::Vec2;
use std::collections::HashMap;
use std::sync::OnceLock;

/// You can read a POI's board within this distance of its centre.
pub const BOARD_RADIUS: f32 = 160.0;
/// Mobs per POI camp.
pub const CAMP_SIZE: usize = 2;
/// Camps stand this far off the POI centre so discovery is not an ambush.
const CAMP_OFFSET: f32 = 170.0;
/// Giver key of every generated quest (never an NPC name).
pub const BOARD_GIVER: &str = "board";

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum GenKind {
    Kill,
    Collect,
    Explore,
    Deliver,
}

impl GenKind {
    pub fn as_str(self) -> &'static str {
        match self {
            GenKind::Kill => "kill",
            GenKind::Collect => "collect",
            GenKind::Explore => "explore",
            GenKind::Deliver => "deliver",
        }
    }
}

/// A place that posts quests: a POI or a cave.
pub struct Anchor {
    /// "poi:<id>" or "cave:<id>".
    pub key: &'static str,
    pub name: &'static str,
    pub act: Act,
    pub pos: Vec2,
    /// Board / arrival radius.
    pub radius: f32,
    pub cave: bool,
    /// Bestiary species camped here (POIs away from the starting areas).
    pub camp: Option<&'static str>,
}

pub struct GenQuest {
    pub def: QuestDef,
    pub kind: GenKind,
    pub title: &'static str,
    /// Index into `anchors()` of the posting place.
    pub anchor: usize,
    /// Recommended level (inside the act's band).
    pub level: u32,
}

struct Table {
    anchors: Vec<Anchor>,
    quests: Vec<GenQuest>,
    by_id: HashMap<&'static str, usize>,
    by_anchor: Vec<Vec<usize>>,
    by_key: HashMap<&'static str, usize>,
}

fn leak(s: String) -> &'static str {
    Box::leak(s.into_boxed_str())
}

fn pretty(tag: &str) -> String {
    tag.split('_')
        .filter(|w| !w.is_empty())
        .map(|w| {
            let mut c = w.chars();
            match c.next() {
                Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn act_index(act: Act) -> usize {
    Act::ALL.iter().position(|a| *a == act).unwrap_or(0)
}

/// (people who post the notices, the menace of the age, a closing phrase).
fn flavour(act: Act) -> (&'static str, &'static str, &'static str) {
    match act {
        Act::Eden => ("The exiles of Adamah", "the curse upon the ground", "east of the garden"),
        Act::Hermon => ("The watchmen of the heights", "the oath of the two hundred", "under Hermon's snows"),
        Act::Nephilim => ("The refugees of the Wastes", "the hunger of the giants", "on the trampled plain"),
        Act::Enoch => ("The dissidents of Enoch", "the forges of Tubal-Cain", "beneath the city smoke"),
        Act::Flood => ("The builders of the Ark", "the fountains of the deep", "while the waters rise"),
    }
}

/// The last quest of each act's authored main chain: the gate every
/// generated quest in that act sits behind.
pub fn act_finale(act: Act) -> &'static str {
    static FINALES: OnceLock<[&'static str; 5]> = OnceLock::new();
    FINALES.get_or_init(|| {
        let mut out = [""; 5];
        for a in Act::ALL {
            let mains: Vec<&QuestDef> = QUESTS
                .iter()
                .filter(|q| q.act == a && !q.side && quest_theme(q.id).is_none() && quest_faction(q.id).is_none())
                .collect();
            let depth = |q: &QuestDef| {
                let mut d = 1;
                let mut cur = q.requires;
                while let Some(r) = cur {
                    d += 1;
                    cur = QUESTS.iter().find(|p| p.id == r).and_then(|p| p.requires);
                    if d > 64 {
                        break;
                    }
                }
                d
            };
            // Deepest main quest that no other main quest builds on.
            let mut best: Option<(&QuestDef, usize)> = None;
            for q in &mains {
                if mains.iter().any(|m| m.requires == Some(q.id)) {
                    continue;
                }
                let d = depth(*q);
                if best.map_or(true, |(_, bd)| d > bd) {
                    best = Some((*q, d));
                }
            }
            out[act_index(a)] = best.map(|(q, _)| q.id).unwrap_or("");
        }
        out
    })[act_index(act)]
}

/// Starting areas where no hostile camp may stand. Eden shelters the whole
/// "mercy" region `make_enemy` already softens (the inn country and the
/// Gate road), so a fresh exile's first hour meets no new hostiles.
fn sheltered(act: Act, pos: Vec2) -> bool {
    if act == Act::Eden {
        pos.length() < 1500.0 || pos.distance(GATE_OF_EDEN) < 1200.0
    } else {
        pos.length() < 700.0
    }
}

fn table() -> &'static Table {
    static TABLE: OnceLock<Table> = OnceLock::new();
    TABLE.get_or_init(build)
}

fn build() -> Table {
    let mut anchors: Vec<Anchor> = Vec::new();
    for act in Act::ALL {
        let species: Vec<_> = crate::mobs::species_for_act(act).into_iter().filter(|m| m.hostile()).collect();
        for p in crate::pois::pois_for_act(act) {
            let pos = Vec2::new(p.x, p.y);
            let camp = if sheltered(act, pos) || species.is_empty() {
                None
            } else {
                Some(species[(p.id as usize * 7 + 3) % species.len()].tag.as_str())
            };
            anchors.push(Anchor {
                key: leak(format!("poi:{}", p.id)),
                name: p.name.as_str(),
                act,
                pos,
                radius: BOARD_RADIUS,
                cave: false,
                camp,
            });
        }
        for c in crate::caves::caves_for_act(act) {
            anchors.push(Anchor {
                key: leak(format!("cave:{}", c.id)),
                name: c.name.as_str(),
                act,
                pos: Vec2::new(c.x, c.y),
                radius: crate::caves::CAVE_RADIUS,
                cave: true,
                camp: None,
            });
        }
    }

    let mut quests: Vec<GenQuest> = Vec::new();
    for act in Act::ALL {
        let ai = act_index(act);
        let (people, menace, closing) = flavour(act);
        let finale = act_finale(act);
        let (lo, hi) = crate::mobs::act_band(act);
        let idx: Vec<usize> = (0..anchors.len()).filter(|i| anchors[*i].act == act).collect();
        let camps: Vec<usize> = idx.iter().copied().filter(|i| anchors[*i].camp.is_some()).collect();
        let cave_idx: Vec<usize> = idx.iter().copied().filter(|i| anchors[*i].cave).collect();
        // Living (non-object) spawn-table tags with a healthy population:
        // the fallback targets for POIs too close to a starting area to camp.
        let fallback: Vec<&'static str> = act_spawn_table(act)
            .iter()
            .filter(|(_, n)| *n >= 5)
            .map(|(t, _)| *t)
            .filter(|t| !["geyser", "scroll_crate", "weapon_cache", "caravan_wagon"].contains(t))
            .collect();
        let next_act = Act::ALL[(ai + 1).min(Act::ALL.len() - 1)];
        let guard_tag = act_spawn_table(next_act)[0].0;

        for (n, &a) in idx.iter().enumerate() {
            let an = &anchors[a];
            let dist = an.pos.length();
            let frac = (dist / (WORLD_BOUNDS * 0.9)).clamp(0.0, 1.0);
            let level = (lo + (frac * (hi - lo) as f32).round() as u32 + u32::from(an.cave)).min(hi);
            // Acceptance gate follows the curve characters actually reach
            // (see docs/beta/SERVER_TRACK): three steps per act.
            let tier = if dist < 1500.0 { 0 } else if dist < 2500.0 { 1 } else { 2 };
            let min_level = 1 + ai as u32 * 3 + tier;
            let slug = leak(format!("gq_{}_{}{}", act.as_str(), if an.cave { "c" } else { "p" }, &an.key[an.key.find(':').unwrap() + 1..]));
            let reward = |mult: f32| -> (u32, u32) {
                let xp = ((40 + level * 10) as f32 * mult).round() as u32;
                let gold = ((4 + level / 2) as f32 * mult).round().max(1.0) as u32;
                (xp, gold)
            };
            let mut push = |suffix: &str, kind: GenKind, title: String, offer: String, objective: Objective, mult: f32, item: Option<&'static str>| {
                let (xp, gold) = reward(mult);
                quests.push(GenQuest {
                    def: QuestDef {
                        id: leak(format!("{slug}_{suffix}")),
                        act,
                        giver: BOARD_GIVER,
                        offer: leak(offer),
                        objective,
                        xp,
                        gold,
                        item,
                        requires: Some(finale),
                        min_level,
                        side: true,
                    },
                    kind,
                    title: leak(title),
                    anchor: a,
                    level,
                });
            };
            // Other places this board sends you to (same act, never itself).
            let other = |step: usize| -> usize {
                let j = idx[(n + step) % idx.len()];
                if j == a { idx[(n + step + 1) % idx.len()] } else { j }
            };
            let mark = |i: usize| -> String {
                let o = &anchors[i];
                let side = if o.pos.x >= an.pos.x { "east" } else { "west" };
                format!("{} (map mark {:.0}, {:.0}, {} of here)", o.name, o.pos.x, o.pos.y, side)
            };
            let variant = n % 3;

            if an.cave {
                let cave = crate::caves::caves_for_act(act)
                    .find(|c| format!("cave:{}", c.id) == an.key)
                    .expect("cave anchor resolves");
                let res: &'static str = cave.resources[0].as_str();
                let res_name = pretty(res);
                push(
                    "kill", GenKind::Kill,
                    format!("Deep Guard of {}", an.name),
                    format!(
                        "{people} warn that {} has crept up from the lower dark into {}. Drive back 3 of the {} that hold the galleries.",
                        menace, an.name, pretty(guard_tag)
                    ),
                    Objective::Kill { target: guard_tag, count: 3 },
                    1.3, None,
                );
                push(
                    "collect", GenKind::Collect,
                    format!("Ore for the Smiths: {res_name}"),
                    format!(
                        "The seams of {} still bear {res_name}. Break the ore nodes and bring 4 measures to this board {closing}.",
                        an.name
                    ),
                    // Review fix: the quest item is a quest-only sample, never
                    // the real ore. The sim's collect rule hands the item to
                    // EVERY group member in range on each node broken, so
                    // with the real (sellable) ore a 40-strong raid holding
                    // this quest minted 40 extra ore per node.
                    Objective::Collect { item: leak(format!("{slug}_ore")), count: 4, source: leak(format!("ore_{res}")) },
                    1.1, None,
                );
                push(
                    "boss", GenKind::Kill,
                    format!("The Dweller of {}", an.name),
                    format!(
                        "Something old guards the {res_name} in {}. Miners call it the Dweller. End it, and the seam is ours again.",
                        an.name
                    ),
                    Objective::Kill { target: leak(format!("{res}_dweller")), count: 1 },
                    2.0, Some("healing_potion"),
                );
                let pos_in_caves = cave_idx.iter().position(|c| *c == a).unwrap_or(0);
                let next_cave = cave_idx[(pos_in_caves + 1) % cave_idx.len()];
                push(
                    "explore", GenKind::Explore,
                    format!("Survey: {}", anchors[next_cave].name),
                    format!(
                        "The old tunnel maps disagree. Walk the mouth of {} and bring back a true account.",
                        mark(next_cave)
                    ),
                    Objective::Explore { place: anchors[next_cave].key },
                    0.6, None,
                );
                push(
                    "deliver", GenKind::Deliver,
                    format!("Samples for the Inn: {res_name}"),
                    format!(
                        "Carry this sealed {res_name} sample from {} to the inn at the heart of the land. The assayer there will pay.",
                        an.name
                    ),
                    Objective::Deliver { item: leak(format!("{slug}_sample")), to: "inn" },
                    0.7, None,
                );
                continue;
            }

            // ── POI boards ──
            let own = an.camp.unwrap_or_else(|| fallback[n % fallback.len()]);
            let own_name = crate::mobs::mob_by_tag(own).map(|m| m.name.clone()).unwrap_or_else(|| pretty(own));
            let kill_n = 5 + (n as u32 % 4);
            let kill_text = match variant {
                0 => format!("{people} can no longer pass {}. The {own_name} holds the ground. Slay {kill_n} of its kind.", an.name),
                1 => format!("Stirred by {menace}, the {own_name} gathers at {}. Thin them - {kill_n} must fall.", an.name),
                _ => format!("A bounty is scratched into the post at {}: the {own_name}, {kill_n} slain, paid {closing}.", an.name),
            };
            push(
                "kill", GenKind::Kill,
                format!("Bounty: {own_name}"),
                kill_text,
                Objective::Kill { target: own, count: kill_n },
                1.0,
                if n % 5 == 0 { Some("bread") } else { None },
            );

            // Collect proof from the species camped at a neighbouring POI.
            let (src, src_place) = if camps.is_empty() {
                (own, a)
            } else {
                let start = camps.iter().position(|c| *c > a).unwrap_or(0);
                let mut pick = camps[(start + n % 3) % camps.len()];
                if pick == a {
                    pick = camps[(start + 1) % camps.len()];
                }
                (anchors[pick].camp.unwrap_or(own), pick)
            };
            let src_name = crate::mobs::mob_by_tag(src).map(|m| m.name.clone()).unwrap_or_else(|| pretty(src));
            let collect_text = match variant {
                0 => format!("The elders want proof that the {src_name} can be beaten. Take 4 tokens from those that roam near {}.", mark(src_place)),
                1 => format!("{people} keep a tally against {menace}. Bring 4 proofs cut from the {src_name}, last seen near {}.", mark(src_place)),
                _ => format!("Four proofs of the {src_name} will buy this camp a season of peace. Hunt them near {}.", mark(src_place)),
            };
            push(
                "collect", GenKind::Collect,
                format!("Proof of the {src_name}"),
                collect_text,
                Objective::Collect { item: leak(format!("{src}_proof")), count: 4, source: src },
                1.1, None,
            );

            let ex = other(7);
            let explore_text = match variant {
                0 => format!("No one has come back from {}. Walk its ground and return with word.", mark(ex)),
                1 => format!("{people} need eyes on {}. Go, look, and report {closing}.", mark(ex)),
                _ => format!("The road to {} is unmapped since {menace} began. Scout it.", mark(ex)),
            };
            push(
                "explore", GenKind::Explore,
                format!("Survey: {}", anchors[ex].name),
                explore_text,
                Objective::Explore { place: anchors[ex].key },
                0.6, None,
            );

            let de = other(13);
            let deliver_text = match variant {
                0 => format!("This sealed satchel is owed to the keepers of {}. Carry it there unopened.", mark(de)),
                1 => format!("Word must reach {} before {menace} cuts the road. Take this satchel.", mark(de)),
                _ => format!("{people} send bread and a letter to {}. See that both arrive.", mark(de)),
            };
            push(
                "deliver", GenKind::Deliver,
                format!("Satchel for {}", anchors[de].name),
                deliver_text,
                Objective::Deliver { item: leak(format!("{slug}_satchel")), to: anchors[de].key },
                0.7, None,
            );
        }
    }

    let by_id = quests.iter().enumerate().map(|(i, q)| (q.def.id, i)).collect();
    let by_key = anchors.iter().enumerate().map(|(i, a)| (a.key, i)).collect();
    let mut by_anchor = vec![Vec::new(); anchors.len()];
    for (i, q) in quests.iter().enumerate() {
        by_anchor[q.anchor].push(i);
    }
    Table { anchors, quests, by_id, by_anchor, by_key }
}

pub fn anchors() -> &'static [Anchor] {
    &table().anchors
}

pub fn generated() -> &'static [GenQuest] {
    &table().quests
}

pub fn gen_quest(id: &str) -> Option<&'static GenQuest> {
    if !id.starts_with("gq_") {
        return None;
    }
    let t = table();
    t.by_id.get(id).map(|i| &t.quests[*i])
}

/// Lookup used by `quests::quest` so kill progress, the dev menu and every
/// other id-based path see generated quests too.
pub fn generated_quest(id: &str) -> Option<&'static QuestDef> {
    gen_quest(id).map(|g| &g.def)
}

pub fn anchor_by_key(key: &str) -> Option<&'static Anchor> {
    let t = table();
    t.by_key.get(key).map(|i| &t.anchors[*i])
}

/// Display title for any quest id (generated quests have real titles;
/// authored ones fall back to the prettified id).
pub fn title(id: &str) -> String {
    gen_quest(id).map(|g| g.title.to_string()).unwrap_or_else(|| pretty(id))
}

/// Total quest count: authored + generated.
pub fn total_quests() -> usize {
    QUESTS.len() + generated().len()
}

/// The board the position stands at: nearest anchor whose radius holds it.
pub fn anchor_at(act: Act, pos: Vec2) -> Option<usize> {
    let t = table();
    t.anchors
        .iter()
        .enumerate()
        .filter(|(_, a)| a.act == act && a.pos.distance(pos) <= a.radius)
        .min_by(|a, b| a.1.pos.distance(pos).total_cmp(&b.1.pos.distance(pos)))
        .map(|(i, _)| i)
}

/// Where a Deliver / Explore target is, and how close counts as "there".
fn target_spot(to: &str) -> Option<(Vec2, f32)> {
    if to == "inn" {
        return Some((Vec2::ZERO, INN_RADIUS));
    }
    anchor_by_key(to).map(|a| (a.pos, a.radius))
}

/// Seed each POI camp at zone boot (called once from `populate_zone`).
/// Positions are fixed offsets, not RNG rolls, so the zone's seeded layout
/// for everything else is unchanged.
pub fn seed_camps(zone: &mut Zone, next_id: &mut EntityId) {
    let act = zone.act;
    for a in anchors().iter().filter(|a| a.act == act) {
        let Some(tag) = a.camp else { continue };
        for k in 0..CAMP_SIZE {
            let ang = (a.pos.x * 0.013 + a.pos.y * 0.007) + k as f32 * std::f32::consts::PI;
            let lim = WORLD_BOUNDS - 60.0;
            let pos = (a.pos + Vec2::new(ang.cos(), ang.sin()) * CAMP_OFFSET).clamp(Vec2::splat(-lim), Vec2::splat(lim));
            let id = *next_id;
            *next_id += 1;
            let mut e = crate::world::st_make_enemy(id, pos, tag, act);
            // Camp guards hold their ground: short leash, short sight.
            e.aggro_range = e.aggro_range.min(240.0);
            e.patrol_radius = 110.0;
            zone.entities.insert(id, e);
        }
    }
}

/// Explore / deliver arrival check (called from `step_zone`; cheap: runs
/// twice a second and only looks at players with a board quest in flight).
pub fn tick_explore(zone: &mut Zone, events: &mut Vec<SimEvent>) {
    if zone.tick % 10 != 0 {
        return;
    }
    for e in zone.entities.values_mut() {
        if e.kind != EntityKind::Player || e.health <= 0 {
            continue;
        }
        let pos = e.pos;
        let Some(s) = e.sheet.as_mut() else { continue };
        if s.quests.is_empty() {
            continue;
        }
        let pending: Vec<String> = s.quests.iter().filter(|(k, p)| **p == 0 && k.starts_with("gq_")).map(|(k, _)| k.clone()).collect();
        for qid in pending {
            let Some(g) = gen_quest(&qid) else { continue };
            let arrived = match g.def.objective {
                Objective::Explore { place } => target_spot(place).is_some_and(|(p, r)| p.distance(pos) <= r),
                Objective::Deliver { item, to } => {
                    s.inventory.iter().any(|i| i == item) && target_spot(to).is_some_and(|(p, r)| p.distance(pos) <= r)
                }
                _ => false,
            };
            if arrived {
                s.quests.insert(qid.clone(), 1);
                if let Some(o) = e.owner {
                    let what = if g.kind == GenKind::Explore { "surveyed" } else { "arrived - hand it over (turn in here)" };
                    events.push(SimEvent::Info { owner: o, text: format!("Quest: {} - {what} (1/1)", g.title) });
                }
            }
        }
    }
}

/// Is the objective satisfied right now (turn-in still checks location)?
fn objective_met(s: &antediluvia_protocol::CharacterSheet, g: &GenQuest) -> bool {
    let prog = s.quests.get(g.def.id).copied().unwrap_or(0);
    match g.def.objective {
        Objective::Kill { count, .. } => prog >= count,
        Objective::Collect { item, count, .. } => s.inventory.iter().filter(|i| i.as_str() == item).count() as u32 >= count,
        Objective::Explore { .. } => prog >= 1,
        Objective::Deliver { item, .. } => s.inventory.iter().any(|i| i == item),
    }
}

/// Why a character cannot take a quest yet, if anything blocks it.
fn lock_reason(s: &antediluvia_protocol::CharacterSheet, g: &GenQuest) -> Option<String> {
    if let Some(r) = g.def.requires {
        if !s.quests_done.iter().any(|d| d == r) {
            return Some(format!("Finish this land's main story first ({}).", pretty(r)));
        }
    }
    if s.level < g.def.min_level {
        return Some(format!("Requires level {}.", g.def.min_level));
    }
    None
}

impl World {
    /// The notice board at the POI/cave the player stands in.
    pub fn board(&self, act: Act, id: EntityId) -> Result<(String, Vec<BoardQuest>), String> {
        let e = self.zones.get(&act).and_then(|z| z.entities.get(&id)).ok_or("no such player")?;
        let s = e.sheet.as_ref().ok_or("no sheet")?;
        let a = anchor_at(act, e.pos).ok_or("There is no notice board here. Boards stand at points of interest and cave mouths.")?;
        let t = table();
        let quests = t.by_anchor[a]
            .iter()
            .map(|i| {
                let g = &t.quests[*i];
                let goal = g.def.objective.count();
                let done = s.quests_done.iter().any(|d| d == g.def.id);
                let active = s.quests.contains_key(g.def.id);
                let lock = if done || active { None } else { lock_reason(s, g) };
                let progress = if active {
                    match g.def.objective {
                        Objective::Collect { item, count, .. } => {
                            (s.inventory.iter().filter(|i| i.as_str() == item).count() as u32).min(count)
                        }
                        _ => s.quests.get(g.def.id).copied().unwrap_or(0).min(goal),
                    }
                } else if done {
                    goal
                } else {
                    0
                };
                let status = if done {
                    "done"
                } else if active {
                    if objective_met(s, g) && !(matches!(g.def.objective, Objective::Deliver { .. }) && progress < 1) {
                        "ready"
                    } else {
                        "active"
                    }
                } else if lock.is_some() {
                    "locked"
                } else {
                    "available"
                };
                BoardQuest {
                    id: g.def.id.to_string(),
                    title: g.title.to_string(),
                    text: g.def.offer.to_string(),
                    kind: g.kind.as_str().to_string(),
                    level: g.level,
                    min_level: g.def.min_level,
                    xp: g.def.xp,
                    gold: g.def.gold,
                    item: g.def.item.map(str::to_string),
                    status: status.to_string(),
                    progress,
                    goal,
                    locked_reason: lock,
                }
            })
            .collect();
        Ok((t.anchors[a].name.to_string(), quests))
    }

    /// Accept a board quest. Must stand at its board; gated by the act
    /// finale, level, and the concurrent-quest cap.
    pub fn board_accept(&mut self, act: Act, id: EntityId, qid: &str) -> Result<String, String> {
        let g = gen_quest(qid).ok_or("That is not a board quest.")?;
        let an = &anchors()[g.anchor];
        let e = self.zones.get_mut(&act).and_then(|z| z.entities.get_mut(&id)).ok_or("no such player")?;
        if e.health <= 0 {
            return Err("You are dead.".into());
        }
        if g.def.act != act || e.pos.distance(an.pos) > an.radius {
            return Err(format!("That notice hangs at {}. Go there to take it.", an.name));
        }
        let s = e.sheet.as_mut().ok_or("no sheet")?;
        if s.quests_done.iter().any(|d| d == qid) {
            return Err("You have already finished that task.".into());
        }
        if s.quests.contains_key(qid) {
            return Err("You already carry that task.".into());
        }
        if let Some(why) = lock_reason(s, g) {
            return Err(why);
        }
        if s.quests.len() >= QUEST_CAP {
            return Err("Your hands are full. Finish what you carry first.".into());
        }
        if let Objective::Deliver { item, .. } = g.def.objective {
            if !s.inventory.iter().any(|i| i == item) {
                if s.inventory.len() >= crate::world::inv_cap(s) {
                    return Err("Your bags are full - you cannot carry the delivery.".into());
                }
                s.inventory.push(item.to_string());
            }
        }
        s.quests.insert(qid.to_string(), 0);
        Ok(format!("{}: {} (0/{})", g.title, g.def.offer, g.def.objective.count()))
    }

    /// Turn in a finished board quest: at its board or the act inn, or - for
    /// deliveries - at the destination. Pays through the same completion
    /// path as authored quests, exactly once.
    pub fn board_turn_in(&mut self, act: Act, id: EntityId, qid: &str) -> Result<String, String> {
        let g = gen_quest(qid).ok_or("That is not a board quest.")?;
        let an = &anchors()[g.anchor];
        let e = self.zones.get_mut(&act).and_then(|z| z.entities.get_mut(&id)).ok_or("no such player")?;
        if e.health <= 0 {
            return Err("You are dead.".into());
        }
        let pos = e.pos;
        let s = e.sheet.as_mut().ok_or("no sheet")?;
        if !s.quests.contains_key(qid) {
            return Err("You are not on that task.".into());
        }
        if g.def.act != act {
            return Err(format!("That task belongs to {}.", g.def.act.as_str()));
        }
        if !objective_met(s, g) {
            return Err(format!("Not done yet - {}/{}.", s.quests[qid].min(g.def.objective.count()), g.def.objective.count()));
        }
        match g.def.objective {
            Objective::Deliver { item, to } => {
                let (p, r) = target_spot(to).ok_or("The destination is lost to the maps.")?;
                if p.distance(pos) > r {
                    return Err("You must hand the delivery over at its destination.".into());
                }
                if let Some(i) = s.inventory.iter().position(|i| i == item) {
                    s.inventory.remove(i);
                }
            }
            other => {
                if pos.distance(an.pos) > an.radius && pos.length() > INN_RADIUS {
                    return Err(format!("Report at {} or at the inn.", an.name));
                }
                if let Objective::Collect { item, count, .. } = other {
                    for _ in 0..count {
                        if let Some(i) = s.inventory.iter().position(|i| i == item) {
                            s.inventory.remove(i);
                        }
                    }
                }
            }
        }
        let rewards = World::st_complete_quest(e, &g.def);
        Ok(format!("{}: done. {rewards}", g.title))
    }

    /// Abandon any active quest. A delivery's parcel is taken back.
    pub fn abandon_quest(&mut self, act: Act, id: EntityId, qid: &str) -> Result<String, String> {
        let e = self.zones.get_mut(&act).and_then(|z| z.entities.get_mut(&id)).ok_or("no such player")?;
        let s = e.sheet.as_mut().ok_or("no sheet")?;
        if s.quests.remove(qid).is_none() {
            return Err("You are not on that task.".into());
        }
        if let Some(Objective::Deliver { item, .. }) = gen_quest(qid).map(|g| g.def.objective) {
            s.inventory.retain(|i| i != item);
        }
        Ok(format!("Abandoned: {}.", title(qid)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::quests::{quest, quests_for};
    use crate::world::new_character;
    use std::collections::HashSet;

    fn spawn(w: &mut World, act: Act, pos: Vec2, done: &[&str], level: u32) -> EntityId {
        let mut s = new_character("Scribe");
        s.act = act;
        s.x = pos.x;
        s.y = pos.y;
        s.level = level;
        s.max_health = 1_000_000; // isolation: camp mobs must not end the test
        s.health = 1_000_000;
        for d in done {
            s.quests_done.push((*d).to_string());
        }
        w.spawn_player(7, s)
    }

    fn sheet(w: &World, act: Act, id: EntityId) -> antediluvia_protocol::CharacterSheet {
        w.player_sheet(act, id).unwrap()
    }

    #[test]
    fn counts_reach_a_thousand_and_cover_every_place() {
        let g = generated();
        assert_eq!(anchors().len(), 230, "200 POIs + 30 caves");
        assert_eq!(g.len(), 200 * 4 + 30 * 5);
        assert!(total_quests() >= 1000, "total quests {}", total_quests());
        for (i, a) in anchors().iter().enumerate() {
            let here: Vec<_> = g.iter().filter(|q| q.anchor == i).collect();
            assert_eq!(here.len(), if a.cave { 5 } else { 4 }, "{}", a.key);
            for k in [GenKind::Kill, GenKind::Collect, GenKind::Explore, GenKind::Deliver] {
                assert!(here.iter().any(|q| q.kind == k), "{} lacks a {k:?} quest", a.key);
            }
        }
        for act in Act::ALL {
            assert_eq!(g.iter().filter(|q| q.def.act == act).count(), 40 * 4 + 6 * 5, "{act:?}");
        }
    }

    #[test]
    fn ids_are_unique_and_resolve_through_the_quest_db() {
        let mut seen = HashSet::new();
        for q in QUESTS {
            assert!(seen.insert(q.id), "duplicate authored id {}", q.id);
            assert!(!q.id.starts_with("gq_"), "authored id {} collides with the generated prefix", q.id);
        }
        for g in generated() {
            assert!(seen.insert(g.def.id), "duplicate id {}", g.def.id);
            let q = quest(g.def.id).expect("generated quest resolves via quests::quest");
            assert_eq!(q.id, g.def.id);
            assert!(!g.title.is_empty() && g.def.offer.len() > 30, "{} needs readable text", g.def.id);
            assert!(!g.def.offer.contains('{') && !g.title.contains('_'), "{}: unrendered text", g.def.id);
        }
        let mut keys = HashSet::new();
        for a in anchors() {
            assert!(keys.insert(a.key), "duplicate anchor {}", a.key);
        }
        // Authored quests are untouched by the new lookup path.
        assert!(quest("serpents_in_the_garden").is_some());
        assert!(quest("gq_nope").is_none());
    }

    #[test]
    fn every_generated_quest_is_gated_and_level_banded() {
        assert_eq!(act_finale(Act::Eden), "the_flaming_sword");
        assert_eq!(act_finale(Act::Hermon), "stargazers_fall");
        let mut last_min = 0;
        for act in Act::ALL {
            let fin = act_finale(act);
            let fq = QUESTS.iter().find(|q| q.id == fin).unwrap_or_else(|| panic!("{act:?} finale {fin} missing"));
            assert!(fq.act == act && !fq.side && fq.requires.is_some(), "{fin} must end a main chain");
            assert!(!QUESTS.iter().any(|q| !q.side && q.act == act && q.requires == Some(fin)), "{fin} is not the finale");
            let (lo, hi) = crate::mobs::act_band(act);
            let mut act_min = u32::MAX;
            for g in generated().iter().filter(|g| g.def.act == act) {
                assert_eq!(g.def.requires, Some(fin), "{} must sit behind the act finale", g.def.id);
                assert_eq!(g.def.giver, BOARD_GIVER);
                assert!(g.def.side);
                assert!((lo..=hi).contains(&g.level), "{} level {} outside {lo}-{hi}", g.def.id, g.level);
                let ai = act_index(act) as u32;
                assert!((1 + ai * 3..=3 + ai * 3).contains(&g.def.min_level), "{} min_level {}", g.def.id, g.def.min_level);
                assert!(g.def.xp > 0 && g.def.gold > 0);
                act_min = act_min.min(g.def.min_level);
            }
            assert!(act_min > last_min, "acts must gate at rising levels");
            last_min = act_min;
        }
        // No NPC ever offers a generated quest: talk() order is untouched.
        for act in Act::ALL {
            for npc in ["Elder", "Wanderer", "Seer", "Sentinel", "Jabal", "Innkeeper", "Quartermaster", "board"] {
                assert!(quests_for(act, npc).all(|q| !q.id.starts_with("gq_")), "{npc} offers a generated quest");
            }
        }
    }

    #[test]
    fn fresh_characters_cannot_take_board_quests_and_talk_is_unchanged() {
        let mut w = World::new(5);
        let a = anchors().iter().position(|a| a.act == Act::Eden && !a.cave).unwrap();
        let pid = spawn(&mut w, Act::Eden, anchors()[a].pos, &[], 1);
        let (place, list) = w.board(Act::Eden, pid).unwrap();
        assert_eq!(place, anchors()[a].name);
        assert_eq!(list.len(), 4);
        assert!(list.iter().all(|q| q.status == "locked" && q.locked_reason.is_some()), "{list:?}");
        let qid = list[0].id.clone();
        let err = w.board_accept(Act::Eden, pid, &qid).unwrap_err();
        assert!(err.contains("main story"), "{err}");
        assert!(sheet(&w, Act::Eden, pid).quests.is_empty());
        // The Elder still opens with the authored starter, finale done or not.
        for done in [&[][..], &["the_flaming_sword"][..]] {
            let mut w = World::new(5);
            let p = spawn(&mut w, Act::Eden, Vec2::new(90.0, 0.0), done, 5);
            let line = w.talk(Act::Eden, p).unwrap();
            assert!(line.starts_with("Elder:"), "{line}");
            let s = sheet(&w, Act::Eden, p);
            assert!(s.quests.keys().all(|k| !k.starts_with("gq_")), "talk() offered a board quest: {:?}", s.quests);
            assert_eq!(s.quests.len(), 1);
        }
        // Level gate, once the story gate is open.
        let far = generated().iter().find(|g| g.def.act == Act::Hermon && g.def.min_level == 6).expect("a far Hermon quest");
        let mut w = World::new(5);
        let p = spawn(&mut w, Act::Hermon, anchors()[far.anchor].pos, &[act_finale(Act::Hermon)], 5);
        assert!(w.board_accept(Act::Hermon, p, far.def.id).unwrap_err().contains("level 6"));
        w.zones.get_mut(&Act::Hermon).unwrap().entities.get_mut(&p).unwrap().sheet.as_mut().unwrap().level = 6;
        assert!(w.board_accept(Act::Hermon, p, far.def.id).is_ok());
    }

    /// Every objective can actually be completed with what the world spawns.
    #[test]
    fn every_objective_is_completable_in_its_act() {
        let w = World::new(0xA17E_D17E);
        for act in Act::ALL {
            let z = &w.zones[&act];
            let enemy_tags: Vec<&str> = z.entities.values().filter(|e| e.kind == EntityKind::Enemy).filter_map(|e| e.tag.as_deref()).collect();
            let node_tags: Vec<&str> = z.entities.values().filter(|e| e.kind == EntityKind::Resource).filter_map(|e| e.tag.as_deref()).collect();
            for g in generated().iter().filter(|g| g.def.act == act) {
                match g.def.objective {
                    Objective::Kill { target, count } => {
                        let live = enemy_tags.iter().filter(|t| t.starts_with(target)).count();
                        assert!(live >= 1, "{}: nothing tagged {target} spawns in {act:?}", g.def.id);
                        assert!(count >= 1 && count <= 8, "{}: count {count}", g.def.id);
                    }
                    Objective::Collect { item, count, source } => {
                        let live = enemy_tags.iter().chain(node_tags.iter()).filter(|t| t.starts_with(source)).count();
                        assert!(live >= 1, "{}: no source {source} in {act:?}", g.def.id);
                        assert!(count >= 1 && !item.is_empty());
                        assert!(
                            crate::economy::vendor_price(item).is_none() && crate::world::item_def(item).is_none(),
                            "{}: quest item {item} must be quest-only (unsellable, unwearable)", g.def.id
                        );
                    }
                    Objective::Explore { place } => {
                        let t = anchor_by_key(place).unwrap_or_else(|| panic!("{}: unknown place {place}", g.def.id));
                        assert_eq!(t.act, act, "{}: explore target in another act", g.def.id);
                        assert_ne!(t.key, anchors()[g.anchor].key, "{}: explores itself", g.def.id);
                    }
                    Objective::Deliver { item, to } => {
                        assert!(crate::economy::vendor_price(item).is_none(), "{}: parcel {item} must be unsellable", g.def.id);
                        if to != "inn" {
                            let t = anchor_by_key(to).unwrap_or_else(|| panic!("{}: unknown destination {to}", g.def.id));
                            assert_eq!(t.act, act);
                            assert_ne!(t.key, anchors()[g.anchor].key, "{}: delivers to itself", g.def.id);
                        }
                    }
                }
                if let Some(item) = g.def.item {
                    assert!(crate::world::CONSUMABLES.contains(&item), "{}: reward {item} unresolved", g.def.id);
                }
            }
        }
    }

    #[test]
    fn camps_stand_at_their_pois_and_spare_the_starting_areas() {
        let w = World::new(11);
        let mut camped = 0;
        for a in anchors().iter().filter(|a| !a.cave) {
            let z = &w.zones[&a.act];
            match a.camp {
                Some(tag) => {
                    camped += 1;
                    let n = z.entities.values().filter(|e| {
                        e.kind == EntityKind::Enemy && e.tag.as_deref() == Some(tag) && e.pos.distance(a.pos) <= CAMP_OFFSET + 5.0
                    }).count();
                    assert!(n >= CAMP_SIZE, "{}: camp of {tag} has {n}", a.key);
                }
                None => assert!(sheltered(a.act, a.pos)),
            }
        }
        assert!(camped >= 170, "only {camped} POIs camped");
        // Nothing new in Eden's softened starting country or on the Gate road.
        for a in anchors().iter().filter(|a| a.act == Act::Eden && a.camp.is_some()) {
            assert!(a.pos.length() >= 1500.0 && a.pos.distance(GATE_OF_EDEN) >= 1200.0);
        }
        let eden_camps = anchors().iter().filter(|a| a.act == Act::Eden && a.camp.is_some()).count();
        assert!(eden_camps >= 20, "Eden still needs camps in its outer country ({eden_camps})");
    }

    /// Slay `n` mobs tagged `tag`, one at a time, standing beside each.
    fn slay(w: &mut World, act: Act, pid: EntityId, tag: &str, n: usize) {
        for _ in 0..n {
            let (eid, epos) = {
                let z = &w.zones[&act];
                let e = z.entities.values().filter(|e| e.kind == EntityKind::Enemy && e.health > 0)
                    .find(|e| e.tag.as_deref().is_some_and(|t| t.starts_with(tag)))
                    .unwrap_or_else(|| panic!("no live {tag} in {act:?}"));
                (e.id, e.pos)
            };
            {
                let z = w.zones.get_mut(&act).unwrap();
                z.entities.get_mut(&eid).unwrap().health = 1;
                let p = z.entities.get_mut(&pid).unwrap();
                p.pos = epos - Vec2::new(12.0, 0.0);
                p.rot = 0.0;
                p.attack_cooldown = 0.0;
            }
            for _ in 0..200 {
                if !w.zones[&act].entities.contains_key(&eid) {
                    break;
                }
                // Keep facing the target even if it shoved us around.
                let tpos = w.zones[&act].entities[&eid].pos;
                let p = w.zones.get_mut(&act).unwrap().entities.get_mut(&pid).unwrap();
                p.pos = tpos - Vec2::new(12.0, 0.0);
                p.rot = 0.0;
                w.queue_attack(act, pid);
                w.step();
            }
            assert!(!w.zones[&act].entities.contains_key(&eid), "{tag} would not die");
        }
    }

    fn teleport(w: &mut World, act: Act, pid: EntityId, pos: Vec2) {
        w.zones.get_mut(&act).unwrap().entities.get_mut(&pid).unwrap().pos = pos;
    }

    #[test]
    fn kill_quest_end_to_end_pays_once() {
        let mut w = World::new(31);
        let g = generated().iter().find(|g| g.def.act == Act::Eden && g.kind == GenKind::Kill && anchors()[g.anchor].camp.is_some()).unwrap();
        let an = &anchors()[g.anchor];
        let pid = spawn(&mut w, Act::Eden, an.pos, &["the_flaming_sword"], 3);
        let Objective::Kill { target, count } = g.def.objective else { panic!() };
        assert!(w.board_turn_in(Act::Eden, pid, g.def.id).is_err(), "cannot turn in before accepting");
        w.board_accept(Act::Eden, pid, g.def.id).unwrap();
        assert!(w.board_accept(Act::Eden, pid, g.def.id).is_err(), "no double accept");
        assert!(w.board_turn_in(Act::Eden, pid, g.def.id).unwrap_err().contains("Not done"));
        slay(&mut w, Act::Eden, pid, target, count as usize);
        assert_eq!(sheet(&w, Act::Eden, pid).quests[g.def.id], count);
        // Wrong place: neither the board nor the inn.
        teleport(&mut w, Act::Eden, pid, an.pos + Vec2::new(600.0, 0.0));
        assert!(w.board_turn_in(Act::Eden, pid, g.def.id).unwrap_err().contains("Report at"));
        teleport(&mut w, Act::Eden, pid, an.pos);
        let before = sheet(&w, Act::Eden, pid);
        let line = w.board_turn_in(Act::Eden, pid, g.def.id).unwrap();
        assert!(line.contains("done"), "{line}");
        let after = sheet(&w, Act::Eden, pid);
        assert_eq!(after.gold, before.gold + g.def.gold);
        assert!(after.quests_done.iter().any(|d| d == g.def.id));
        assert!(!after.quests.contains_key(g.def.id));
        // Paid exactly once: no second turn-in, no re-accept.
        assert!(w.board_turn_in(Act::Eden, pid, g.def.id).is_err());
        assert!(w.board_accept(Act::Eden, pid, g.def.id).is_err());
        assert_eq!(sheet(&w, Act::Eden, pid).gold, after.gold);
        let (_, list) = w.board(Act::Eden, pid).unwrap();
        assert_eq!(list.iter().find(|q| q.id == g.def.id).unwrap().status, "done");
    }

    #[test]
    fn collect_quest_end_to_end_consumes_the_proofs() {
        let mut w = World::new(32);
        let g = generated().iter().find(|g| g.def.act == Act::Hermon && g.kind == GenKind::Collect && !anchors()[g.anchor].cave).unwrap();
        let an = &anchors()[g.anchor];
        let pid = spawn(&mut w, Act::Hermon, an.pos, &[act_finale(Act::Hermon)], 9);
        let Objective::Collect { item, count, source } = g.def.objective else { panic!() };
        w.board_accept(Act::Hermon, pid, g.def.id).unwrap();
        slay(&mut w, Act::Hermon, pid, source, count as usize);
        let s = sheet(&w, Act::Hermon, pid);
        assert_eq!(s.inventory.iter().filter(|i| i.as_str() == item).count() as u32, count);
        // Turn in at the inn works for non-deliveries.
        teleport(&mut w, Act::Hermon, pid, Vec2::new(30.0, 30.0));
        w.board_turn_in(Act::Hermon, pid, g.def.id).unwrap();
        let s = sheet(&w, Act::Hermon, pid);
        assert_eq!(s.inventory.iter().filter(|i| i.as_str() == item).count(), 0, "proofs consumed");
        assert!(s.quests_done.iter().any(|d| d == g.def.id));
    }

    #[test]
    fn cave_ore_collect_and_dweller_quests_complete() {
        let mut w = World::new(33);
        let ore = generated().iter().find(|g| g.def.act == Act::Eden && anchors()[g.anchor].cave && g.kind == GenKind::Collect).unwrap();
        let an = &anchors()[ore.anchor];
        let pid = spawn(&mut w, Act::Eden, an.pos, &["the_flaming_sword"], 3);
        let Objective::Collect { item, count, source } = ore.def.objective else { panic!() };
        w.board_accept(Act::Eden, pid, ore.def.id).unwrap();
        // Harvest ore nodes of this resource until the quest is satisfied.
        for _ in 0..12 {
            if sheet(&w, Act::Eden, pid).inventory.iter().filter(|i| i.as_str() == item).count() as u32 >= count {
                break;
            }
            let (nid, npos) = {
                let z = &w.zones[&Act::Eden];
                // Nearest node to the cave (ties by id): the entity map is a
                // hash map, and taking "any" node made this test flaky.
                let n = z
                    .entities
                    .values()
                    .filter(|e| e.kind == EntityKind::Resource && e.tag.as_deref() == Some(source))
                    .min_by(|a, b| a.pos.distance(an.pos).total_cmp(&b.pos.distance(an.pos)).then(a.id.cmp(&b.id)))
                    .expect("ore node");
                (n.id, n.pos)
            };
            for _ in 0..60 {
                if !w.zones[&Act::Eden].entities.contains_key(&nid) {
                    break;
                }
                let p = w.zones.get_mut(&Act::Eden).unwrap().entities.get_mut(&pid).unwrap();
                p.pos = npos - Vec2::new(20.0, 0.0);
                p.rot = 0.0;
                w.queue_attack(Act::Eden, pid);
                w.step();
            }
        }
        // Review regression: the quest never mints the real, sellable ore.
        // Only the node's own yield (2 per node) is in the bags.
        let real = source.strip_prefix("ore_").unwrap();
        assert_ne!(item, real);
        let s = sheet(&w, Act::Eden, pid);
        let real_n = s.inventory.iter().filter(|i| i.as_str() == real).count() as u32;
        assert_eq!(real_n, 2 * count, "{count} nodes broken must yield exactly {} {real}", 2 * count);
        teleport(&mut w, Act::Eden, pid, Vec2::ZERO);
        assert!(w.sell(Act::Eden, pid, item).is_err(), "quest samples are worthless to the innkeeper");
        teleport(&mut w, Act::Eden, pid, an.pos);
        w.board_turn_in(Act::Eden, pid, ore.def.id).unwrap();
        assert_eq!(sheet(&w, Act::Eden, pid).inventory.iter().filter(|i| i.as_str() == item).count(), 0, "samples consumed");
        // The dweller bounty at the same cave.
        let boss = generated().iter().find(|g| g.anchor == ore.anchor && g.def.id.ends_with("_boss")).unwrap();
        let Objective::Kill { target, .. } = boss.def.objective else { panic!() };
        w.board_accept(Act::Eden, pid, boss.def.id).unwrap();
        slay(&mut w, Act::Eden, pid, target, 1);
        teleport(&mut w, Act::Eden, pid, an.pos);
        let line = w.board_turn_in(Act::Eden, pid, boss.def.id).unwrap();
        assert!(line.contains("healing_potion"), "{line}");
    }

    #[test]
    fn explore_quest_needs_a_visit_while_active() {
        let mut w = World::new(34);
        let g = generated().iter().find(|g| {
            g.def.act == Act::Nephilim && g.kind == GenKind::Explore
                && matches!(g.def.objective, Objective::Explore { place } if anchor_by_key(place).unwrap().pos.distance(anchors()[g.anchor].pos) > 600.0)
        }).unwrap();
        let an = &anchors()[g.anchor];
        let Objective::Explore { place } = g.def.objective else { panic!() };
        let target = anchor_by_key(place).unwrap();
        let pid = spawn(&mut w, Act::Nephilim, an.pos, &[act_finale(Act::Nephilim)], 12);
        w.board_accept(Act::Nephilim, pid, g.def.id).unwrap();
        for _ in 0..25 {
            w.step();
        }
        assert_eq!(sheet(&w, Act::Nephilim, pid).quests[g.def.id], 0, "standing at the giver is not exploring");
        assert!(w.board_turn_in(Act::Nephilim, pid, g.def.id).is_err());
        teleport(&mut w, Act::Nephilim, pid, target.pos);
        let mut told = false;
        for _ in 0..25 {
            for ev in w.step() {
                if let SimEvent::Info { text, .. } = ev {
                    told |= text.contains("surveyed");
                }
            }
        }
        assert!(told, "the player is told on arrival");
        assert_eq!(sheet(&w, Act::Nephilim, pid).quests[g.def.id], 1);
        teleport(&mut w, Act::Nephilim, pid, an.pos);
        w.board_turn_in(Act::Nephilim, pid, g.def.id).unwrap();
        assert!(sheet(&w, Act::Nephilim, pid).quests_done.iter().any(|d| d == g.def.id));
    }

    #[test]
    fn deliver_quest_hands_out_one_parcel_and_takes_it_back() {
        let mut w = World::new(35);
        let g = generated().iter().find(|g| {
            g.def.act == Act::Enoch && g.kind == GenKind::Deliver && !anchors()[g.anchor].cave
                && matches!(g.def.objective, Objective::Deliver { to, .. } if {
                    let d = anchor_by_key(to).unwrap().pos;
                    d.length() > 600.0 && d.distance(anchors()[g.anchor].pos) > 600.0
                })
        }).unwrap();
        let an = &anchors()[g.anchor];
        let Objective::Deliver { item, to } = g.def.objective else { panic!() };
        let dest = anchor_by_key(to).unwrap();
        let pid = spawn(&mut w, Act::Enoch, an.pos, &[act_finale(Act::Enoch)], 15);
        let count = |w: &World| sheet(w, Act::Enoch, pid).inventory.iter().filter(|i| i.as_str() == item).count();
        w.board_accept(Act::Enoch, pid, g.def.id).unwrap();
        assert_eq!(count(&w), 1);
        // Abandon takes the parcel back; re-accepting never yields two.
        w.abandon_quest(Act::Enoch, pid, g.def.id).unwrap();
        assert_eq!(count(&w), 0);
        w.board_accept(Act::Enoch, pid, g.def.id).unwrap();
        assert_eq!(count(&w), 1);
        // The parcel is worthless to the innkeeper.
        teleport(&mut w, Act::Enoch, pid, Vec2::ZERO);
        assert!(w.sell(Act::Enoch, pid, item).is_err());
        // It cannot be handed in at the giver or the inn.
        assert!(w.board_turn_in(Act::Enoch, pid, g.def.id).unwrap_err().contains("destination"));
        teleport(&mut w, Act::Enoch, pid, dest.pos);
        let gold0 = sheet(&w, Act::Enoch, pid).gold;
        w.board_turn_in(Act::Enoch, pid, g.def.id).unwrap();
        assert_eq!(count(&w), 0, "parcel consumed");
        assert_eq!(sheet(&w, Act::Enoch, pid).gold, gold0 + g.def.gold);
        // Without the parcel there is nothing to deliver.
        let g2 = generated().iter().find(|q| q.def.act == Act::Enoch && q.kind == GenKind::Deliver && q.def.id != g.def.id && !anchors()[q.anchor].cave).unwrap();
        let Objective::Deliver { item: item2, to: to2 } = g2.def.objective else { panic!() };
        teleport(&mut w, Act::Enoch, pid, anchors()[g2.anchor].pos);
        w.board_accept(Act::Enoch, pid, g2.def.id).unwrap();
        w.zones.get_mut(&Act::Enoch).unwrap().entities.get_mut(&pid).unwrap().sheet.as_mut().unwrap().inventory.retain(|i| i != item2);
        teleport(&mut w, Act::Enoch, pid, anchor_by_key(to2).unwrap().pos);
        assert!(w.board_turn_in(Act::Enoch, pid, g2.def.id).is_err());
    }

    #[test]
    fn quest_cap_and_wrong_place_are_enforced() {
        let mut w = World::new(36);
        let fin = act_finale(Act::Flood);
        let here: Vec<_> = generated().iter().filter(|g| g.def.act == Act::Flood).collect();
        let pid = spawn(&mut w, Act::Flood, anchors()[here[0].anchor].pos, &[fin], 15);
        // Not at that board.
        let elsewhere = here.iter().find(|g| anchors()[g.anchor].pos.distance(anchors()[here[0].anchor].pos) > 600.0).unwrap();
        assert!(w.board_accept(Act::Flood, pid, elsewhere.def.id).unwrap_err().contains("Go there"));
        let mut taken = 0;
        for g in &here {
            teleport(&mut w, Act::Flood, pid, anchors()[g.anchor].pos);
            match w.board_accept(Act::Flood, pid, g.def.id) {
                Ok(_) => taken += 1,
                Err(e) => {
                    assert!(e.contains("hands are full"), "{e}");
                    break;
                }
            }
        }
        assert_eq!(taken, QUEST_CAP);
        assert_eq!(sheet(&w, Act::Flood, pid).quests.len(), QUEST_CAP);
        // Another act's quest cannot be taken from here.
        let eden = generated().iter().find(|g| g.def.act == Act::Eden).unwrap();
        assert!(w.board_accept(Act::Flood, pid, eden.def.id).is_err());
        assert!(w.board_accept(Act::Flood, pid, "serpents_in_the_garden").unwrap_err().contains("not a board quest"));
    }
}
