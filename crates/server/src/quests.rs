//! Data-driven quest database (CHUNK C01).
//!
//! Quests live here as `&'static` data; `world.rs` owns the runtime rules
//! (accept / progress / turn-in). C02 fills this file out from
//! `docs/quests/act*_*.md`; this chunk migrates the original five kill
//! quests and proves the engine with two more Eden quests (one collect,
//! one chained).

use antediluvia_protocol::Act;

/// What a quest asks the player to do.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Objective {
    /// Slay `count` enemies whose tag starts with `target`.
    Kill { target: &'static str, count: u32 },
    /// Hold `count` of `item`, looted 100% from kills of `source` while the
    /// quest is active. Turn-in consumes the items.
    Collect { item: &'static str, count: u32, source: &'static str },
}

impl Objective {
    pub fn count(&self) -> u32 {
        match self {
            Objective::Kill { count, .. } | Objective::Collect { count, .. } => *count,
        }
    }
}

pub struct QuestDef {
    pub id: &'static str,
    pub act: Act,
    /// NPC giver key — matched against the lowercase NPC `name`.
    pub giver: &'static str,
    pub offer: &'static str,
    pub objective: Objective,
    pub xp: u32,
    pub gold: u32,
    pub item: Option<&'static str>,
    /// Prerequisite quest id that must be in `quests_done`.
    pub requires: Option<&'static str>,
    #[allow(dead_code)] // C02+ content uses this to sort quest logs
    pub side: bool,
}

use Objective::{Collect, Kill};

pub const QUESTS: &[QuestDef] = &[
    // — original five main kill quests, one per act (giver: the act Elder) —
    QuestDef { id: "serpents_in_the_garden", act: Act::Eden, giver: "elder",
        offer: "Serpents defile the garden. Slay 5 of them.",
        objective: Kill { target: "serpent", count: 5 },
        xp: 120, gold: 15, item: Some("bronze_sword"), requires: None, side: false },
    QuestDef { id: "watchers_on_the_mount", act: Act::Hermon, giver: "elder",
        offer: "The Watchers descend on Hermon. Fell 5 of them.",
        objective: Kill { target: "watcher", count: 5 },
        xp: 220, gold: 25, item: None, requires: None, side: false },
    QuestDef { id: "giants_in_the_land", act: Act::Nephilim, giver: "elder",
        offer: "There were giants in those days. Bring down 5.",
        objective: Kill { target: "giant", count: 5 },
        xp: 350, gold: 40, item: None, requires: None, side: false },
    QuestDef { id: "shades_of_enoch", act: Act::Enoch, giver: "elder",
        offer: "Shades haunt the city of Enoch. Banish 5.",
        objective: Kill { target: "shade", count: 5 },
        xp: 500, gold: 55, item: None, requires: None, side: false },
    QuestDef { id: "leviathan_hunt", act: Act::Flood, giver: "elder",
        offer: "The deep sends leviathans. Hunt 3 before the end.",
        objective: Kill { target: "leviathan", count: 3 },
        xp: 800, gold: 90, item: Some("hide_vest"), requires: None, side: false },
    // — Eden engine-proof quests from docs/quests/act1_eden.md —
    // Collect-from-kill (doc #6, source mapped to Eden's live mob set).
    QuestDef { id: "the_first_forges", act: Act::Eden, giver: "wanderer",
        offer: "Tubal-Cain strikes metal into blades. Loot 6 bronze ingots so he cannot forge more.",
        objective: Collect { item: "bronze_ingot", count: 6, source: "serpent" },
        xp: 220, gold: 35, item: Some("bronze_bracers"), requires: None, side: true },
    // Chained on the Eden main quest (doc #2).
    QuestDef { id: "altar_of_the_firstborn", act: Act::Eden, giver: "wanderer",
        offer: "Scavengers dismantle Abel's altar. Drive off 8 of the beasts they loose.",
        objective: Kill { target: "serpent", count: 8 },
        xp: 180, gold: 20, item: None, requires: Some("serpents_in_the_garden"), side: true },
];

/// Max concurrent quests per player.
pub const QUEST_CAP: usize = 10;

pub fn quest(id: &str) -> Option<&'static QuestDef> {
    QUESTS.iter().find(|q| q.id == id)
}

/// Quests a given NPC (by lowercase giver key) offers in an act.
pub fn quests_for(act: Act, giver: &str) -> impl Iterator<Item = &'static QuestDef> + '_ {
    let giver = giver.to_lowercase();
    QUESTS.iter().filter(move |q| q.act == act && q.giver == giver)
}
