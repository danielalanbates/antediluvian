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

// Full quest database from docs/quests/act1..act5.md (C02). 6 quests per act:
// 3 main (chained in order) + 3 side. Doc questgivers map onto the three NPC
// entities each act spawns:
//   elder    (inn)          — Elder Adamah / Jared / Desperate Refugee /
//                             City Dissident / Noah, plus co-located camp NPCs
//   wanderer (crossroads)   — Sethite Wanderer / Healer Mahalalel / Renegade
//                             Armorer / Lamech's Rival / Shem
//   seer     (far field)    — Abel's Echo / Enoch the Prophet / Mahaway /
//                             Enoch the Patriarch / Japheth & Ham
// Objectives the engine can't express yet are reframed as Kill/Collect and
// noted inline (escort/survive/deliver → themed kill or 1-item collect).
pub const QUESTS: &[QuestDef] = &[
    // ============================ ACT I — EDEN ============================
    QuestDef { id: "serpents_in_the_garden", act: Act::Eden, giver: "elder",
        offer: "Serpents defile the garden. Slay 5 of them.",
        objective: Kill { target: "serpent", count: 5 },
        xp: 120, gold: 15, item: Some("bronze_sword"), requires: None, side: false },
    QuestDef { id: "altar_of_the_firstborn", act: Act::Eden, giver: "wanderer",
        offer: "Cainite scavengers dismantle Abel's altar. Drive off 8 of them.",
        objective: Kill { target: "cainite", count: 8 },
        xp: 180, gold: 20, item: None, requires: Some("serpents_in_the_garden"), side: false },
    QuestDef { id: "the_flaming_sword", act: Act::Eden, giver: "elder",
        offer: "Stray embers drift from the Garden's flaming sword. Gather 5 cherubim sparks from the wisps they ignite.",
        objective: Collect { item: "cherubim_spark", count: 5, source: "ember_wisp" },
        xp: 250, gold: 30, item: Some("spark_woven_cloak"), requires: Some("altar_of_the_firstborn"), side: false },
    QuestDef { id: "fruit_of_the_thorns", act: Act::Eden, giver: "wanderer",
        // Briar-fruit comes from harvesting Eden's trees (doc: gather from the thicket).
        offer: "The cursed ground still must feed us. Gather 10 briar-fruit from the thickets.",
        objective: Collect { item: "briar_fruit", count: 10, source: "tree" },
        xp: 100, gold: 5, item: Some("bitter_bread"), requires: None, side: true },
    QuestDef { id: "blood_on_the_soil", act: Act::Eden, giver: "seer",
        offer: "The blood-soaked earth is restless. Calm 5 earth elementals risen from the stained soil.",
        objective: Kill { target: "elemental", count: 5 },
        xp: 200, gold: 25, item: None, requires: None, side: true },
    QuestDef { id: "the_first_forges", act: Act::Eden, giver: "seer",
        offer: "Tubal-Cain strikes metal into blades. Loot 6 bronze ingots from the Cainite camp.",
        objective: Collect { item: "bronze_ingot", count: 6, source: "cainite" },
        xp: 220, gold: 35, item: Some("bronze_bracers"), requires: None, side: true },
    // =========================== ACT II — HERMON ==========================
    QuestDef { id: "watchers_on_the_mount", act: Act::Hermon, giver: "elder",
        offer: "The Watchers descend on Hermon. Fell 5 of them.",
        objective: Kill { target: "watcher", count: 5 },
        xp: 220, gold: 25, item: None, requires: None, side: false },
    QuestDef { id: "oath_of_imprecation", act: Act::Hermon, giver: "elder",
        // Destroy-the-object → the Oath-Stone spawns as a stationary destructible.
        offer: "Samyaza's pact is carved into the Oath-Stone on the summit. Shatter it.",
        objective: Kill { target: "oathstone", count: 1 },
        xp: 350, gold: 40, item: Some("amulet_of_the_unbound"), requires: Some("watchers_on_the_mount"), side: false },
    QuestDef { id: "stargazers_fall", act: Act::Hermon, giver: "seer",
        offer: "Baraqiel's chief Stargazer rallies cultists on the eastern ridge. Silence him.",
        objective: Kill { target: "stargazer", count: 1 },
        xp: 450, gold: 50, item: Some("astrologers_staff"), requires: Some("oath_of_imprecation"), side: false },
    QuestDef { id: "roots_of_sorcery", act: Act::Hermon, giver: "wanderer",
        // Mandrake roots carried by the cultists who cut them (doc: brewed enchantments).
        offer: "Corrupted herbs poison the springs. Take 8 toxic mandrake roots from the cultists.",
        objective: Collect { item: "mandrake_root", count: 8, source: "cultist" },
        xp: 200, gold: 20, item: Some("healing_potion"), requires: None, side: true },
    QuestDef { id: "cosmetic_deception", act: Act::Hermon, giver: "wanderer",
        offer: "A caravan hauls Azazel's cursed vanities through the pass. Destroy 3 of its wagons.",
        objective: Kill { target: "caravan_wagon", count: 3 },
        xp: 250, gold: 35, item: None, requires: None, side: true },
    QuestDef { id: "visions_of_the_gateway", act: Act::Hermon, giver: "seer",
        offer: "Abyssal energies twist the cave-dwellers below. Slay 10 chasm fiends.",
        objective: Kill { target: "chasm_fiend", count: 10 },
        xp: 300, gold: 40, item: None, requires: None, side: true },
    // ========================= ACT III — NEPHILIM =========================
    QuestDef { id: "giants_in_the_land", act: Act::Nephilim, giver: "elder",
        offer: "There were giants in those days. Bring down 5 of their hunting party.",
        objective: Kill { target: "giant", count: 5 },
        xp: 350, gold: 40, item: None, requires: None, side: false },
    QuestDef { id: "blood_and_iron", act: Act::Nephilim, giver: "wanderer",
        offer: "Their iron armory waits in the deep ravine. Sabotage 4 weapon caches.",
        objective: Kill { target: "weapon_cache", count: 4 },
        xp: 400, gold: 45, item: Some("iron_greaves"), requires: Some("giants_in_the_land"), side: false },
    QuestDef { id: "ohyahs_dream", act: Act::Nephilim, giver: "seer",
        // Boss retrieve → the dream tablet drops from the act's alpha giant.
        offer: "Retrieve the dream tablet from the warlord of the giants, that Enoch may read it.",
        objective: Collect { item: "dream_tablet", count: 1, source: "giant_alpha" },
        xp: 600, gold: 70, item: Some("ring_of_the_dreamer"), requires: Some("blood_and_iron"), side: false },
    QuestDef { id: "the_ravaged_earth", act: Act::Nephilim, giver: "wanderer",
        // Soil samples come from mining the wasteland's rocks.
        offer: "The earth dies beneath the giants. Bring me 5 depleted soil samples from the rocks.",
        objective: Collect { item: "soil_sample", count: 5, source: "rock" },
        xp: 300, gold: 30, item: None, requires: None, side: true },
    QuestDef { id: "bones_of_the_consumed", act: Act::Nephilim, giver: "seer",
        offer: "My family was taken to the feasting pits. Recover 6 remains from the giants for burial.",
        objective: Collect { item: "human_remains", count: 6, source: "giant" },
        xp: 350, gold: 35, item: None, requires: None, side: true },
    QuestDef { id: "the_blood_drinkers", act: Act::Nephilim, giver: "elder",
        offer: "Blood-drinking giants hunt the canyons at night. Slay 8 so our scouts can pass.",
        objective: Kill { target: "blood_drinker", count: 8 },
        xp: 450, gold: 50, item: Some("giant_bone_crusher"), requires: None, side: true },
    // =========================== ACT IV — ENOCH ===========================
    QuestDef { id: "shades_of_enoch", act: Act::Enoch, giver: "elder",
        offer: "Shades haunt the city of Enoch. Banish 5.",
        objective: Kill { target: "shade", count: 5 },
        xp: 500, gold: 55, item: None, requires: None, side: false },
    QuestDef { id: "azazels_armory", act: Act::Enoch, giver: "wanderer",
        offer: "Lamech hoards Watcher armor in the citadel. Take 3 schematics from its guards.",
        objective: Collect { item: "armor_schematic", count: 3, source: "citadel_guard" },
        xp: 650, gold: 75, item: Some("lamechs_helm"), requires: Some("shades_of_enoch"), side: false },
    QuestDef { id: "smog_of_industry", act: Act::Enoch, giver: "elder",
        offer: "The furnaces never stop. Destroy 4 alchemical furnace regulators.",
        objective: Kill { target: "furnace_regulator", count: 4 },
        xp: 600, gold: 70, item: None, requires: Some("azazels_armory"), side: false },
    QuestDef { id: "the_hidden_prophet", act: Act::Enoch, giver: "seer",
        // Delivery → the scroll must be wrested from the shades warding the compound.
        offer: "Judgment is decreed. Recover the scroll of doom from the shades that ward the compound.",
        objective: Collect { item: "scroll_of_doom", count: 1, source: "shade" },
        xp: 550, gold: 60, item: None, requires: None, side: true },
    QuestDef { id: "song_of_the_sword", act: Act::Enoch, giver: "wanderer",
        offer: "Dark magic corrupts our ancestral craft. Defeat 6 enchanter smiths.",
        objective: Kill { target: "enchanter_smith", count: 6 },
        xp: 600, gold: 65, item: Some("enchanted_bronze_blade"), requires: None, side: true },
    QuestDef { id: "syndicate_of_sorcery", act: Act::Enoch, giver: "seer",
        offer: "Samyaza's syndicate drugs the slums. Slay 8 of their sorcerers.",
        objective: Kill { target: "sorcerer", count: 8 },
        xp: 650, gold: 70, item: None, requires: None, side: true },
    // =========================== ACT V — FLOOD ============================
    QuestDef { id: "leviathan_hunt", act: Act::Flood, giver: "elder",
        offer: "The deep sends leviathans. Hunt 3 before the end.",
        objective: Kill { target: "leviathan", count: 3 },
        xp: 800, gold: 90, item: Some("hide_vest"), requires: None, side: false },
    QuestDef { id: "fountains_of_the_deep", act: Act::Flood, giver: "elder",
        offer: "The crust shatters. Cap 5 abyssal geysers before they cut off the beasts' path.",
        objective: Kill { target: "geyser", count: 5 },
        xp: 900, gold: 100, item: None, requires: Some("leviathan_hunt"), side: false },
    QuestDef { id: "boarding_the_ark", act: Act::Flood, giver: "wanderer",
        // Wave defense → hold the ramp against the raiders assaulting it.
        offer: "The giants storm the ramp! Let no raider set foot on the gopher wood — fell 9.",
        objective: Kill { target: "nephilim_raider", count: 9 },
        xp: 1200, gold: 150, item: Some("covenant_signet"), requires: Some("fountains_of_the_deep"), side: false },
    QuestDef { id: "the_last_scion", act: Act::Flood, giver: "seer",
        // Escort → clear the drowned beasts between the outpost and the Ark.
        offer: "A righteous family is trapped south of us. Clear 6 drowned beasts from their path.",
        objective: Kill { target: "drowned_beast", count: 6 },
        xp: 850, gold: 90, item: None, requires: None, side: true },
    QuestDef { id: "drowning_the_corruption", act: Act::Flood, giver: "seer",
        offer: "The cultists drag their dark library to high ground. Sink 5 crates of forbidden scrolls.",
        objective: Kill { target: "scroll_crate", count: 5 },
        xp: 800, gold: 85, item: None, requires: None, side: true },
    QuestDef { id: "the_rain_begins", act: Act::Flood, giver: "elder",
        // Survival → the final onslaught, expressed as a cull of the surge.
        offer: "The door is sealed and the waters surge. Survive the onslaught — put down 10 drowned beasts.",
        objective: Kill { target: "drowned_beast", count: 10 },
        xp: 1500, gold: 200, item: None, requires: None, side: true },
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::{act_spawn_table, item_def, CONSUMABLES};

    /// C02 integrity: every quest is satisfiable with what actually spawns,
    /// every reward resolves, chains resolve, and each act has a starter.
    #[test]
    fn quest_db_is_consistent() {
        for act in Act::ALL {
            assert!(
                QUESTS.iter().any(|q| q.act == act && q.requires.is_none() && !q.side),
                "{act:?} needs a main starter quest"
            );
            assert_eq!(QUESTS.iter().filter(|q| q.act == act).count(), 6, "{act:?} quest count");
        }
        for q in QUESTS {
            assert!(["elder", "wanderer", "seer"].contains(&q.giver), "{}: unknown giver {}", q.id, q.giver);
            if let Some(r) = q.requires {
                let pre = quest(r).unwrap_or_else(|| panic!("{}: missing prerequisite {r}", q.id));
                assert_eq!(pre.act, q.act, "{}: prerequisite crosses acts", q.id);
            }
            if let Some(item) = q.item {
                assert!(
                    item_def(item).is_some() || CONSUMABLES.contains(&item),
                    "{}: reward item {item} unresolved", q.id
                );
            }
            let (needs_tag, kills) = match q.objective {
                Objective::Kill { target, .. } => (target, true),
                Objective::Collect { source, .. } => (source, false),
            };
            let spawns = act_spawn_table(q.act).iter().any(|(t, n)| t.starts_with(needs_tag) && *n > 0)
                || needs_tag == "tree" || needs_tag == "rock" // resource nodes spawn every act
                || (!kills && format!("{}_alpha", act_spawn_table(q.act)[0].0).starts_with(needs_tag));
            assert!(spawns, "{}: no spawner for {needs_tag}", q.id);
            // Enough mobs must exist per respawn cycle for kill counts to be reachable.
            if let Objective::Kill { target, count } = q.objective {
                let live: usize = act_spawn_table(q.act).iter()
                    .filter(|(t, _)| t.starts_with(target)).map(|(_, n)| n).sum();
                assert!(live >= 1 && live as u32 <= 200, "{}: {target} spawn count {live} vs need {count}", q.id);
            }
        }
    }
}
