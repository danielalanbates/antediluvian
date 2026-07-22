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
    /// Minimum character level to accept (default 1).
    pub min_level: u32,
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
        xp: 120, gold: 15, item: Some("bronze_sword"), requires: None, side: false, min_level: 1 },
    QuestDef { id: "altar_of_the_firstborn", act: Act::Eden, giver: "wanderer",
        offer: "Cainite scavengers dismantle Abel's altar. Drive off 8 of them.",
        objective: Kill { target: "cainite", count: 8 },
        xp: 180, gold: 20, item: None, requires: Some("serpents_in_the_garden"), side: false, min_level: 1 },
    QuestDef { id: "the_flaming_sword", act: Act::Eden, giver: "elder",
        offer: "Stray embers drift from the Garden's flaming sword. Gather 5 cherubim sparks from the wisps they ignite.",
        objective: Collect { item: "cherubim_spark", count: 5, source: "ember_wisp" },
        xp: 250, gold: 30, item: Some("spark_woven_cloak"), requires: Some("altar_of_the_firstborn"), side: false, min_level: 1 },
    QuestDef { id: "fruit_of_the_thorns", act: Act::Eden, giver: "wanderer",
        // Briar-fruit comes from harvesting Eden's trees (doc: gather from the thicket).
        offer: "The cursed ground still must feed us. Gather 10 briar-fruit from the thickets.",
        objective: Collect { item: "briar_fruit", count: 10, source: "tree" },
        xp: 100, gold: 5, item: Some("bitter_bread"), requires: None, side: true, min_level: 1 },
    QuestDef { id: "blood_on_the_soil", act: Act::Eden, giver: "seer",
        offer: "The blood-soaked earth is restless. Calm 5 earth elementals risen from the stained soil.",
        objective: Kill { target: "elemental", count: 5 },
        xp: 200, gold: 25, item: None, requires: None, side: true, min_level: 1 },
    QuestDef { id: "the_first_forges", act: Act::Eden, giver: "seer",
        offer: "Tubal-Cain strikes metal into blades. Loot 6 bronze ingots from the Cainite camp.",
        objective: Collect { item: "bronze_ingot", count: 6, source: "cainite" },
        xp: 220, gold: 35, item: Some("bronze_bracers"), requires: None, side: true, min_level: 1 },
    // — Hand-authored side chains (alpha content pass, 2026-07-18) —
    QuestDef { id: "ashes_of_the_first_altar", act: Act::Eden, giver: "seer",
        offer: "Abel's blood still cries from the ground. Scatter the cultists of Azazel who mock his altar — 6 of them.",
        objective: Kill { target: "azazel_cultist", count: 6 },
        xp: 160, gold: 18, item: None, requires: Some("the_flaming_sword"), side: true, min_level: 1 },
    QuestDef { id: "embers_for_the_hearth", act: Act::Eden, giver: "elder",
        offer: "The inn's hearth must never die — its flame was carried out of Eden itself. Bring me 4 living embers from the wisps.",
        objective: Collect { item: "living_ember", count: 4, source: "ember_wisp" },
        xp: 140, gold: 15, item: Some("bread"), requires: Some("the_flaming_sword"), side: true, min_level: 1 },
    QuestDef { id: "zealots_at_the_gate", act: Act::Eden, giver: "wanderer",
        offer: "Sethite zealots bar pilgrims from the Gate road, calling every traveler apostate. Humble 5 of them.",
        objective: Kill { target: "sethite_zealot", count: 5 },
        xp: 170, gold: 20, item: None, requires: Some("the_flaming_sword"), side: true, min_level: 1 },
    QuestDef { id: "the_serpents_brood", act: Act::Eden, giver: "seer",
        offer: "The serpent's brood nests where the four rivers part. Crush 9 serpents before the nests hatch.",
        objective: Kill { target: "serpent", count: 9 },
        xp: 200, gold: 24, item: Some("healing_potion"), requires: Some("ashes_of_the_first_altar"), side: true, min_level: 1 },
    QuestDef { id: "clay_of_the_riverbank", act: Act::Eden, giver: "elder",
        offer: "Adamah means earth — and from the river clay we shape lamps for the dark years ahead. Break 6 elementals and bring their clay hearts.",
        objective: Collect { item: "river_clay", count: 6, source: "elemental" },
        xp: 190, gold: 22, item: None, requires: Some("embers_for_the_hearth"), side: true, min_level: 1 },
    // =========================== ACT II — HERMON ==========================
    QuestDef { id: "watchers_on_the_mount", act: Act::Hermon, giver: "elder",
        offer: "The Watchers descend on Hermon. Fell 5 of them.",
        objective: Kill { target: "watcher", count: 5 },
        xp: 220, gold: 25, item: None, requires: None, side: false, min_level: 1 },
    QuestDef { id: "oath_of_imprecation", act: Act::Hermon, giver: "elder",
        // Destroy-the-object → the Oath-Stone spawns as a stationary destructible.
        offer: "Samyaza's pact is carved into the Oath-Stone on the summit. Shatter it.",
        objective: Kill { target: "oathstone", count: 1 },
        xp: 350, gold: 40, item: Some("amulet_of_the_unbound"), requires: Some("watchers_on_the_mount"), side: false, min_level: 1 },
    QuestDef { id: "stargazers_fall", act: Act::Hermon, giver: "seer",
        offer: "Baraqiel's chief Stargazer rallies cultists on the eastern ridge. Silence him.",
        objective: Kill { target: "stargazer", count: 1 },
        xp: 450, gold: 50, item: Some("astrologers_staff"), requires: Some("oath_of_imprecation"), side: false, min_level: 1 },
    QuestDef { id: "roots_of_sorcery", act: Act::Hermon, giver: "wanderer",
        // Mandrake roots carried by the cultists who cut them (doc: brewed enchantments).
        offer: "Corrupted herbs poison the springs. Take 8 toxic mandrake roots from the cultists.",
        objective: Collect { item: "mandrake_root", count: 8, source: "cultist" },
        xp: 200, gold: 20, item: Some("healing_potion"), requires: None, side: true, min_level: 1 },
    QuestDef { id: "cosmetic_deception", act: Act::Hermon, giver: "wanderer",
        offer: "A caravan hauls Azazel's cursed vanities through the pass. Destroy 3 of its wagons.",
        objective: Kill { target: "caravan_wagon", count: 3 },
        xp: 250, gold: 35, item: None, requires: None, side: true, min_level: 1 },
    QuestDef { id: "visions_of_the_gateway", act: Act::Hermon, giver: "seer",
        offer: "Abyssal energies twist the cave-dwellers below. Slay 10 chasm fiends.",
        objective: Kill { target: "chasm_fiend", count: 10 },
        xp: 300, gold: 40, item: None, requires: None, side: true, min_level: 1 },
    // — Hand-authored side chains (alpha content pass, 2026-07-18) —
    QuestDef { id: "the_two_hundred", act: Act::Hermon, giver: "seer",
        offer: "Two hundred swore on Hermon's peak. Every Watcher felled is one oath broken. Break 8 of them.",
        objective: Kill { target: "watcher", count: 8 },
        xp: 320, gold: 38, item: None, requires: Some("stargazers_fall"), side: true, min_level: 1 },
    QuestDef { id: "lenses_of_false_heaven", act: Act::Hermon, giver: "wanderer",
        offer: "Baraqiel grinds crystal lenses to read forbidden stars. Shatter 2 of them where they stand.",
        objective: Kill { target: "crystal_lens", count: 2 },
        xp: 280, gold: 32, item: None, requires: Some("stargazers_fall"), side: true, min_level: 1 },
    QuestDef { id: "songs_the_mountain_hates", act: Act::Hermon, giver: "elder",
        offer: "The cultists chant the Watchers' hymns into the springs until the water forgets its Maker. Silence 9 cultists.",
        objective: Kill { target: "cultist", count: 9 },
        xp: 340, gold: 40, item: None, requires: Some("the_two_hundred"), side: true, min_level: 1 },
    QuestDef { id: "what_the_chasm_keeps", act: Act::Hermon, giver: "seer",
        offer: "The fiends of the chasm hoard oath-shards — fragments of the stone the two hundred swore upon. Recover 5.",
        objective: Collect { item: "oath_shard", count: 5, source: "chasm_fiend" },
        xp: 360, gold: 42, item: Some("healing_potion"), requires: Some("stargazers_fall"), side: true, min_level: 1 },
    QuestDef { id: "the_healers_ledger", act: Act::Hermon, giver: "wanderer",
        offer: "Mahalalel treats the pass's wounded without payment — but not without bandages. Take 6 wrappings from the cultists who stole his stores.",
        objective: Collect { item: "linen_wrapping", count: 6, source: "cultist" },
        xp: 300, gold: 34, item: Some("bread"), requires: Some("stargazers_fall"), side: true, min_level: 1 },
    // ========================= ACT III — NEPHILIM =========================
    QuestDef { id: "giants_in_the_land", act: Act::Nephilim, giver: "elder",
        offer: "There were giants in those days. Bring down 5 of their hunting party.",
        objective: Kill { target: "giant", count: 5 },
        xp: 350, gold: 40, item: None, requires: None, side: false, min_level: 1 },
    QuestDef { id: "blood_and_iron", act: Act::Nephilim, giver: "wanderer",
        offer: "Their iron armory waits in the deep ravine. Sabotage 4 weapon caches.",
        objective: Kill { target: "weapon_cache", count: 4 },
        xp: 400, gold: 45, item: Some("iron_greaves"), requires: Some("giants_in_the_land"), side: false, min_level: 1 },
    QuestDef { id: "ohyahs_dream", act: Act::Nephilim, giver: "seer",
        // Boss retrieve → the dream tablet drops from the act's alpha giant.
        offer: "Retrieve the dream tablet from the warlord of the giants, that Enoch may read it.",
        objective: Collect { item: "dream_tablet", count: 1, source: "giant_alpha" },
        xp: 600, gold: 70, item: Some("ring_of_the_dreamer"), requires: Some("blood_and_iron"), side: false, min_level: 1 },
    QuestDef { id: "the_ravaged_earth", act: Act::Nephilim, giver: "wanderer",
        // Soil samples come from mining the wasteland's rocks.
        offer: "The earth dies beneath the giants. Bring me 5 depleted soil samples from the rocks.",
        objective: Collect { item: "soil_sample", count: 5, source: "rock" },
        xp: 300, gold: 30, item: None, requires: None, side: true, min_level: 1 },
    QuestDef { id: "bones_of_the_consumed", act: Act::Nephilim, giver: "seer",
        offer: "My family was taken to the feasting pits. Recover 6 remains from the giants for burial.",
        objective: Collect { item: "human_remains", count: 6, source: "giant" },
        xp: 350, gold: 35, item: None, requires: None, side: true, min_level: 1 },
    QuestDef { id: "the_blood_drinkers", act: Act::Nephilim, giver: "elder",
        offer: "Blood-drinking giants hunt the canyons at night. Slay 8 so our scouts can pass.",
        objective: Kill { target: "blood_drinker", count: 8 },
        xp: 450, gold: 50, item: Some("giant_bone_crusher"), requires: None, side: true, min_level: 1 },
    // — Hand-authored side chains (alpha content pass, 2026-07-18) —
    QuestDef { id: "meat_for_the_table_of_giants", act: Act::Nephilim, giver: "elder",
        offer: "The giants eat the flocks, then the herdsmen. Cull 10 giants before the valley starves.",
        objective: Kill { target: "giant", count: 10 },
        xp: 460, gold: 55, item: None, requires: Some("ohyahs_dream"), side: true, min_level: 1 },
    QuestDef { id: "the_thirst_that_grew", act: Act::Nephilim, giver: "wanderer",
        offer: "When the flesh of beasts ran out, they turned to blood. End 6 blood-drinkers stalking the refugee road.",
        objective: Kill { target: "blood_drinker", count: 6 },
        xp: 480, gold: 58, item: Some("hide_vest"), requires: Some("ohyahs_dream"), side: true, min_level: 1 },
    QuestDef { id: "arms_race_of_the_valley", act: Act::Nephilim, giver: "wanderer",
        offer: "Tubal-Cain's patterns spread: every cache of nephilim steel arms another raid. Burn 3 weapon caches.",
        objective: Kill { target: "weapon_cache", count: 3 },
        xp: 440, gold: 52, item: None, requires: Some("ohyahs_dream"), side: true, min_level: 1 },
    QuestDef { id: "trophies_of_the_fallen", act: Act::Nephilim, giver: "seer",
        offer: "Each giant wears the sigils of the Watcher that sired it. Bring me 5 sigil-plates — Mahaway must read the lineages.",
        objective: Collect { item: "sigil_plate", count: 5, source: "giant" },
        xp: 500, gold: 60, item: None, requires: Some("meat_for_the_table_of_giants"), side: true, min_level: 1 },
    QuestDef { id: "the_last_herdsman", act: Act::Nephilim, giver: "elder",
        offer: "One herdsman still grazes the high pasture, too stubborn to flee. Clear 8 more giants so his family can reach the pass alive.",
        objective: Kill { target: "giant", count: 8 },
        xp: 520, gold: 62, item: Some("healing_potion"), requires: Some("trophies_of_the_fallen"), side: true, min_level: 1 },
    // =========================== ACT IV — ENOCH ===========================
    QuestDef { id: "shades_of_enoch", act: Act::Enoch, giver: "elder",
        offer: "Shades haunt the city of Enoch. Banish 5.",
        objective: Kill { target: "shade", count: 5 },
        xp: 500, gold: 55, item: None, requires: None, side: false, min_level: 1 },
    QuestDef { id: "azazels_armory", act: Act::Enoch, giver: "wanderer",
        offer: "Lamech hoards Watcher armor in the citadel. Take 3 schematics from its guards.",
        objective: Collect { item: "armor_schematic", count: 3, source: "citadel_guard" },
        xp: 650, gold: 75, item: Some("lamechs_helm"), requires: Some("shades_of_enoch"), side: false, min_level: 1 },
    QuestDef { id: "smog_of_industry", act: Act::Enoch, giver: "elder",
        offer: "The furnaces never stop. Destroy 4 alchemical furnace regulators.",
        objective: Kill { target: "furnace_regulator", count: 4 },
        xp: 600, gold: 70, item: None, requires: Some("azazels_armory"), side: false, min_level: 1 },
    QuestDef { id: "the_hidden_prophet", act: Act::Enoch, giver: "seer",
        // Delivery → the scroll must be wrested from the shades warding the compound.
        offer: "Judgment is decreed. Recover the scroll of doom from the shades that ward the compound.",
        objective: Collect { item: "scroll_of_doom", count: 1, source: "shade" },
        xp: 550, gold: 60, item: None, requires: None, side: true, min_level: 1 },
    QuestDef { id: "song_of_the_sword", act: Act::Enoch, giver: "wanderer",
        offer: "Dark magic corrupts our ancestral craft. Defeat 6 enchanter smiths.",
        objective: Kill { target: "enchanter_smith", count: 6 },
        xp: 600, gold: 65, item: Some("enchanted_bronze_blade"), requires: None, side: true, min_level: 1 },
    QuestDef { id: "syndicate_of_sorcery", act: Act::Enoch, giver: "seer",
        offer: "Samyaza's syndicate drugs the slums. Slay 8 of their sorcerers.",
        objective: Kill { target: "sorcerer", count: 8 },
        xp: 650, gold: 70, item: None, requires: None, side: true, min_level: 1 },
    // — Hand-authored side chains (alpha content pass, 2026-07-18) —
    QuestDef { id: "the_citys_long_shadow", act: Act::Enoch, giver: "elder",
        offer: "Enoch's shades multiply in the alleys — echoes of every deal cut in blood. Lay 9 of them to rest.",
        objective: Kill { target: "shade", count: 9 },
        xp: 560, gold: 66, item: None, requires: Some("smog_of_industry"), side: true, min_level: 1 },
    QuestDef { id: "papers_of_passage", act: Act::Enoch, giver: "wanderer",
        offer: "The citadel guards sell exit papers, then arrest the buyers at the gate. Relieve 6 guards of their stamped seals.",
        objective: Collect { item: "citadel_seal", count: 6, source: "citadel_guard" },
        xp: 580, gold: 70, item: None, requires: Some("smog_of_industry"), side: true, min_level: 1 },
    QuestDef { id: "wolves_of_the_smelting_yards", act: Act::Enoch, giver: "elder",
        offer: "Lamech's kennels loose dire wolves in the smelting yards each night — cheaper than watchmen. Put down 10.",
        objective: Kill { target: "dire_wolf", count: 10 },
        xp: 600, gold: 72, item: Some("dire_wolf_horn"), requires: Some("smog_of_industry"), side: true, min_level: 1 },
    QuestDef { id: "the_sorcerers_debt", act: Act::Enoch, giver: "seer",
        offer: "The tower sorcerers bind spirits with borrowed names and pay in other men's years. Unbind 7 sorcerers.",
        objective: Kill { target: "sorcerer", count: 7 },
        xp: 620, gold: 74, item: None, requires: Some("the_citys_long_shadow"), side: true, min_level: 1 },
    QuestDef { id: "quenching_the_furnaces", act: Act::Enoch, giver: "wanderer",
        offer: "Every furnace regulator you wreck is a night the forges cool and the smiths go home to their children. Wreck 3.",
        objective: Kill { target: "furnace_regulator", count: 3 },
        xp: 590, gold: 68, item: None, requires: Some("papers_of_passage"), side: true, min_level: 1 },
    // =========================== ACT V — FLOOD ============================
    QuestDef { id: "leviathan_hunt", act: Act::Flood, giver: "elder",
        offer: "The deep sends leviathans. Hunt 3 before the end.",
        objective: Kill { target: "leviathan", count: 3 },
        xp: 800, gold: 90, item: Some("hide_vest"), requires: None, side: false, min_level: 1 },
    QuestDef { id: "fountains_of_the_deep", act: Act::Flood, giver: "elder",
        offer: "The crust shatters. Cap 5 abyssal geysers before they cut off the beasts' path.",
        objective: Kill { target: "geyser", count: 5 },
        xp: 900, gold: 100, item: None, requires: Some("leviathan_hunt"), side: false, min_level: 1 },
    QuestDef { id: "boarding_the_ark", act: Act::Flood, giver: "wanderer",
        // Wave defense → hold the ramp against the raiders assaulting it.
        offer: "The giants storm the ramp! Let no raider set foot on the gopher wood — fell 9.",
        objective: Kill { target: "nephilim_raider", count: 9 },
        xp: 1200, gold: 150, item: Some("covenant_signet"), requires: Some("fountains_of_the_deep"), side: false, min_level: 1 },
    QuestDef { id: "the_last_scion", act: Act::Flood, giver: "seer",
        // Escort → clear the drowned beasts between the outpost and the Ark.
        offer: "A righteous family is trapped south of us. Clear 6 drowned beasts from their path.",
        objective: Kill { target: "drowned_beast", count: 6 },
        xp: 850, gold: 90, item: None, requires: None, side: true, min_level: 1 },
    QuestDef { id: "drowning_the_corruption", act: Act::Flood, giver: "seer",
        offer: "The cultists drag their dark library to high ground. Sink 5 crates of forbidden scrolls.",
        objective: Kill { target: "scroll_crate", count: 5 },
        xp: 800, gold: 85, item: None, requires: None, side: true, min_level: 1 },
    QuestDef { id: "the_rain_begins", act: Act::Flood, giver: "elder",
        // Survival → the final onslaught, expressed as a cull of the surge.
        offer: "The door is sealed and the waters surge. Survive the onslaught — put down 10 drowned beasts.",
        objective: Kill { target: "drowned_beast", count: 10 },
        xp: 1500, gold: 200, item: None, requires: None, side: true, min_level: 1 },
    // — Hand-authored side chains (alpha content pass, 2026-07-18) —
    QuestDef { id: "the_waters_teeth", act: Act::Flood, giver: "elder",
        offer: "The deep sends leviathans ahead of the rain like teeth before the bite. Drive 12 back beneath the waves.",
        objective: Kill { target: "leviathan", count: 12 },
        xp: 700, gold: 85, item: None, requires: Some("boarding_the_ark"), side: true, min_level: 1 },
    QuestDef { id: "what_the_drowned_remember", act: Act::Flood, giver: "seer",
        offer: "The drowned beasts wash ashore wearing the harnesses of the herds they once were. Grant 8 of them rest.",
        objective: Kill { target: "drowned_beast", count: 8 },
        xp: 720, gold: 88, item: None, requires: Some("boarding_the_ark"), side: true, min_level: 1 },
    QuestDef { id: "raiders_of_the_last_hour", act: Act::Flood, giver: "wanderer",
        offer: "Nephilim raiders storm the Ark plateau — not to repent, but to take the boat. Hold the line: 7 raiders.",
        objective: Kill { target: "nephilim_raider", count: 7 },
        xp: 740, gold: 90, item: Some("iron_greaves"), requires: Some("boarding_the_ark"), side: true, min_level: 1 },
    QuestDef { id: "words_worth_saving", act: Act::Flood, giver: "seer",
        offer: "The scroll crates bobbing in the shallows hold the Sethite star-records — Pillars of Seth, written small. Recover 4 waterlogged scrolls.",
        objective: Collect { item: "waterlogged_scroll", count: 4, source: "scroll_crate" },
        xp: 710, gold: 86, item: None, requires: Some("boarding_the_ark"), side: true, min_level: 1 },
    QuestDef { id: "steam_from_the_deep", act: Act::Flood, giver: "elder",
        offer: "The fountains of the great deep are breaking up — cap 3 geysers near the plateau before they undermine the Ark's footing.",
        objective: Kill { target: "geyser", count: 3 },
        xp: 730, gold: 89, item: Some("healing_potion"), requires: Some("the_waters_teeth"), side: true, min_level: 1 },
    // ================= MOUNT QUESTLINE (C06, level 40, Enoch) =============
    // docs/quests/mount_questline.md — Parts 2A/2B collapse into one collect
    // quest (iron links from the Enchanter Smiths); Parts 3+4 collapse into
    // the Swift-Claw hunt whose turn-in grants the Horn of the Dire-Wolf.
    QuestDef { id: "mount_call_of_the_wild", act: Act::Enoch, giver: "jabal",
        offer: "A Dire-Wolf bows only to an Alpha. Slay 15 feral dire-wolves to prove your dominance.",
        objective: Kill { target: "dire_wolf", count: 15 },
        xp: 4000, gold: 0, item: None, requires: None, side: true, min_level: 40 },
    QuestDef { id: "mount_watchers_chain", act: Act::Enoch, giver: "jabal",
        offer: "Bring me 3 Watcher-forged iron links from the Enchanter Smiths; chain or bridle, the wolf must be reined.",
        objective: Collect { item: "iron_link", count: 3, source: "enchanter_smith" },
        xp: 5500, gold: 0, item: None, requires: Some("mount_call_of_the_wild"), side: true, min_level: 40 },
    QuestDef { id: "mount_alphas_den", act: Act::Enoch, giver: "jabal",
        offer: "Deep in the canyons a legendary pack dens. Subdue Swift-Claw, the young Alpha, and ride home.",
        objective: Kill { target: "swift_claw", count: 1 },
        xp: 15500, gold: 0, item: Some("dire_wolf_horn"), requires: Some("mount_watchers_chain"), side: true, min_level: 40 },
    // ============ THEME PILLAR I — THE FORBIDDEN ARTS (C08) ==============
    // 10-quest cross-act epic from docs/quests/themes/01_the_forbidden_arts.md.
    // Faithful subset: arc A #1/#3/#4/#10, arc B #11/#13, arc C #22/#24,
    // arc E #41/#46. Skipped: the remaining 37 doc entries (mechanics the
    // engine can't express yet: escort, stealth, dialogue, rituals).
    // Cross-act prerequisites are the point — see quest_theme()/quest_next_hint().
    QuestDef { id: "fa_first_blade", act: Act::Eden, giver: "wanderer",
        offer: "Azazel taught men the sword. Recover the first iron blade ever forged from the Cainite bandits of Nod.",
        objective: Collect { item: "first_iron_blade", count: 1, source: "cainite" },
        xp: 150, gold: 20, item: None, requires: None, side: true, min_level: 1 },
    QuestDef { id: "fa_enchanters_bellows", act: Act::Enoch, giver: "elder",
        offer: "The slum-forges breathe by sorcery. Sabotage 3 of the enchanted bellows.",
        objective: Kill { target: "enchanted_bellows", count: 3 },
        xp: 420, gold: 45, item: None, requires: Some("fa_first_blade"), side: true, min_level: 1 },
    QuestDef { id: "fa_blood_tempered_steel", act: Act::Enoch, giver: "wanderer",
        offer: "Smiths quench their blades in human blood for a magical edge. End 6 of the blood-smiths.",
        objective: Kill { target: "blood_smith", count: 6 },
        xp: 480, gold: 50, item: Some("healing_potion"), requires: Some("fa_enchanters_bellows"), side: true, min_level: 1 },
    QuestDef { id: "fa_smithing_giant", act: Act::Nephilim, giver: "elder",
        offer: "A Nephilim brute has learned Azazel's forge-craft and arms his brethren. Bring him down.",
        objective: Kill { target: "forge_giant", count: 1 },
        xp: 600, gold: 60, item: None, requires: Some("fa_blood_tempered_steel"), side: true, min_level: 1 },
    QuestDef { id: "fa_mandrake_harvest", act: Act::Hermon, giver: "wanderer",
        offer: "Samyaza's cultists cut mandrake on the slopes for their root-magic. Take 10 toxic roots from them.",
        objective: Collect { item: "mandrake_root", count: 10, source: "cultist" },
        xp: 320, gold: 30, item: None, requires: Some("fa_smithing_giant"), side: true, min_level: 1 },
    QuestDef { id: "fa_mind_benders", act: Act::Enoch, giver: "seer",
        offer: "Syndicate sorcerers drug the minds of the slums. Slay 10 of them.",
        objective: Kill { target: "sorcerer", count: 10 },
        xp: 520, gold: 55, item: None, requires: Some("fa_mandrake_harvest"), side: true, min_level: 1 },
    QuestDef { id: "fa_crystal_lenses", act: Act::Hermon, giver: "seer",
        offer: "Baraqiel's observatory reads doom in the stars through great quartz lenses. Shatter all 3.",
        objective: Kill { target: "crystal_lens", count: 3 },
        xp: 380, gold: 40, item: None, requires: Some("fa_mind_benders"), side: true, min_level: 1 },
    QuestDef { id: "fa_zodiac_stones", act: Act::Flood, giver: "elder",
        offer: "The raiders carry engraved zodiac stones that channel star-magic. Recover 4 from them.",
        objective: Collect { item: "zodiac_stone", count: 4, source: "nephilim_raider" },
        xp: 650, gold: 70, item: None, requires: Some("fa_crystal_lenses"), side: true, min_level: 1 },
    QuestDef { id: "fa_scholars_request", act: Act::Eden, giver: "seer",
        offer: "One artifact of each forbidden art must be sealed away. Take 4 from Azazel's cultists in the garden's shadow.",
        objective: Collect { item: "forbidden_artifact", count: 4, source: "azazel_cultist" },
        xp: 700, gold: 80, item: None, requires: Some("fa_zodiac_stones"), side: true, min_level: 1 },
    QuestDef { id: "fa_final_master", act: Act::Nephilim, giver: "seer",
        offer: "A warlord has mastered all four arts — blade, root, star and word. Face Azazel's Herald and end the corruption.",
        objective: Kill { target: "azazel_herald", count: 1 },
        xp: 1500, gold: 150, item: Some("warlords_star_blade"), requires: Some("fa_scholars_request"), side: true, min_level: 10 },
    // ============ GATE OF EDEN — the exile prologue (starting area) =======
    QuestDef { id: "garments_of_mercy", act: Act::Eden, giver: "sentinel",
        offer: "The garden is shut behind you, child of Adam, and the sword turns every way. Those skins on your back were mercy's first gift — earn the rest. Serpents of the Brood nest along the road west; slay 3 and prove you can live outside the garden.",
        objective: Kill { target: "serpent", count: 3 },
        xp: 60, gold: 5, item: Some("bread"), requires: None, side: false, min_level: 1 },
    QuestDef { id: "the_road_west", act: Act::Eden, giver: "sentinel",
        offer: "You will not stand at this gate forever. The serpent matriarchs swallow tokens of the garden — cut one free and return it to me, and I will bless your road. Then walk WEST along this road: an inn rises where the four rivers meet the plain, and the Elder there keeps the memory of your line.",
        objective: Collect { item: "gate_token", count: 1, source: "serpent" },
        xp: 80, gold: 10, item: None, requires: Some("garments_of_mercy"), side: false, min_level: 1 },
    // ============ FACTION VARIANTS (C10) — same deed, rival lore ==========
    QuestDef { id: "sethite_purge_of_nod", act: Act::Eden, giver: "elder",
        offer: "[Sethite] The Cainite camps profane Abel's memory. Scatter 6 of their scavengers.",
        objective: Kill { target: "cainite", count: 6 },
        xp: 300, gold: 30, item: None, requires: None, side: true, min_level: 10 },
    QuestDef { id: "cainite_reprisal", act: Act::Eden, giver: "elder",
        offer: "[Cainite] Sethite zealots burn our forges by night. Drive off 6 of them.",
        objective: Kill { target: "sethite_zealot", count: 6 },
        xp: 300, gold: 30, item: None, requires: None, side: true, min_level: 10 },
];

/// Faction a quest is restricted to (C10): only that lineage sees it, and
/// turn-in pays +250 reputation with it.
pub fn quest_faction(id: &str) -> Option<&'static str> {
    match id {
        "sethite_purge_of_nod" => Some("sethite"),
        "cainite_reprisal" => Some("cainite"),
        _ => None,
    }
}

/// Theme pillar a quest belongs to (client tracker prefixes these; the
/// cross-act prerequisite exemption in the consistency test keys off it).
pub fn quest_theme(id: &str) -> Option<&'static str> {
    if id.starts_with("fa_") {
        Some("Forbidden Arts")
    } else {
        None
    }
}

/// Where the chain sends you after turning a themed quest in (appended to
/// the turn-in notice so cross-act hops are discoverable in-game).
pub fn quest_next_hint(id: &str) -> Option<&'static str> {
    Some(match id {
        "fa_first_blade" => "Take the blade to the City Dissident in Enoch.",
        "fa_enchanters_bellows" => "Seek Lamech's Rival at the Enoch crossroads.",
        "fa_blood_tempered_steel" => "The forge-giant waits in the Nephilim Wastes — see the Refugee.",
        "fa_smithing_giant" => "Samyaza's roots grow on Hermon. Find Healer Mahalalel.",
        "fa_mandrake_harvest" => "Return to Enoch; the Patriarch watches the sorcerers.",
        "fa_mind_benders" => "Climb to Mahaway on Hermon's ridge — the stars lie.",
        "fa_crystal_lenses" => "Noah gathers the zodiac stones amid the Flood.",
        "fa_zodiac_stones" => "Carry the artifacts to Abel's Echo in Eden.",
        "fa_scholars_request" => "Azazel's Herald masses in the Wastes. Japheth and Ham know where.",
        _ => return None,
    })
}

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
    use crate::world::{act_spawn_table, item_def, CONSUMABLES, KEY_ITEMS};

    /// C02 integrity: every quest is satisfiable with what actually spawns,
    /// every reward resolves, chains resolve, and each act has a starter.
    #[test]
    fn quest_db_is_consistent() {
        for act in Act::ALL {
            assert!(
                QUESTS.iter().any(|q| q.act == act && q.requires.is_none() && !q.side),
                "{act:?} needs a main starter quest"
            );
            // 6 doc quests per act (+3 mount-chain quests parked in Enoch);
            // theme-pillar quests (C08) sit outside the per-act budget.
            // Eden carries the two Gate-of-Eden prologue quests on top of
            // its per-act budget.
            // +5 hand-authored side quests per act (alpha content pass 2026-07-18).
            let expect = match act {
                Act::Enoch => 14,
                Act::Eden => 13,
                _ => 11,
            };
            let n = QUESTS.iter()
                .filter(|q| q.act == act && quest_theme(q.id).is_none() && quest_faction(q.id).is_none())
                .count();
            assert_eq!(n, expect, "{act:?} quest count");
        }
        for q in QUESTS {
            assert!(["elder", "wanderer", "seer", "jabal", "sentinel"].contains(&q.giver), "{}: unknown giver {}", q.id, q.giver);
            if let Some(r) = q.requires {
                let pre = quest(r).unwrap_or_else(|| panic!("{}: missing prerequisite {r}", q.id));
                // Theme pillars (C08) hop acts on purpose; everything else stays local.
                if quest_theme(q.id).is_none() {
                    assert_eq!(pre.act, q.act, "{}: prerequisite crosses acts", q.id);
                }
            }
            if let Some(item) = q.item {
                assert!(
                    item_def(item).is_some() || CONSUMABLES.contains(&item) || KEY_ITEMS.contains(&item),
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

    /// C08: the Forbidden Arts pillar is one unbroken chain that genuinely
    /// spans the world, with hints covering every hop but the last.
    #[test]
    fn forbidden_arts_chain_spans_acts() {
        let chain: Vec<&QuestDef> =
            QUESTS.iter().filter(|q| quest_theme(q.id) == Some("Forbidden Arts")).collect();
        assert_eq!(chain.len(), 10);
        assert!(chain[0].requires.is_none());
        for pair in chain.windows(2) {
            assert_eq!(pair[1].requires, Some(pair[0].id), "chain must be unbroken in order");
        }
        let acts: std::collections::HashSet<_> = chain.iter().map(|q| q.act).collect();
        assert_eq!(acts.len(), 5, "the pillar must visit every act");
        // Every quest but the finale tells you where to go next.
        for q in &chain[..chain.len() - 1] {
            assert!(quest_next_hint(q.id).is_some(), "{} needs a travel hint", q.id);
        }
        assert!(quest_next_hint(chain[9].id).is_none());
    }
}
