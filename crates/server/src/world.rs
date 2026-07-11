//! Authoritative world model and fixed-tick simulation.
//!
//! The world is the single source of truth. Clients send *intents*
//! (`Move`, `Attack`); the server integrates them here. One `World` owns every
//! zone; each zone owns its entities. The whole sim runs single-threaded on the
//! game loop, so there are no locks in the hot path.

use antediluvia_protocol::{Act, CharacterSheet, Class, EntityId, EntityKind, EntityState, EventKind};
use glam::Vec2;
use std::collections::HashMap;

pub const TICK_HZ: u64 = 20;
pub const DT: f32 = 1.0 / TICK_HZ as f32;
pub use antediluvia_protocol::WORLD_BOUNDS;

/// No hostile spawns within this distance of the inn (C05).
pub const SAFE_RING: f32 = 400.0;

/// Seconds after dealing/taking damage during which you count as in combat.
const COMBAT_LOCK_SECS: f32 = 5.0;

const PLAYER_SPEED: f32 = 180.0; // C05: bigger world, WoW-ish travel pacing
const ENEMY_SPEED: f32 = 120.0;
const WILDLIFE_SPEED: f32 = 190.0;
const MELEE_RANGE: f32 = 80.0;
const MELEE_ARC_DOT: f32 = 0.35; // cos of half-arc; target must be roughly in front
const ENEMY_MELEE_RANGE: f32 = 46.0;
const PLAYER_ATTACK_DMG: i32 = 12;
const ATTACK_COOLDOWN: f32 = 0.8;
const RESPAWN_SECS: f32 = 6.0;
const HEALTH_REGEN_PER_SEC: f32 = 2.0;
const MANA_REGEN_PER_SEC: f32 = 4.0;
/// Global cooldown between ability casts (WoW-style GCD).
const GCD_SECS: f32 = 1.0;
/// Radius around a zone's entry point that counts as the inn (rest area).
pub const INN_RADIUS: f32 = 220.0;
/// Rested XP gained per second while at an inn, and its cap.
const RESTED_PER_SEC: u32 = 20;
const RESTED_CAP: u32 = 2000;
const PROFESSION_CAP: u32 = 300;

// ─── Class abilities ─────────────────────────────────────────────────────────

pub enum AbilityEffect {
    /// Single-target damage to the nearest valid target in range & arc.
    Damage(i32),
    /// Damage to every valid target within the radius.
    Aoe(i32, f32),
    /// Heal self.
    Heal(i32),
}

pub struct Ability {
    pub id: &'static str,
    pub class: Class,
    pub min_level: u32,
    pub mana: i32,
    pub cooldown: f32,
    pub range: f32,
    pub effect: AbilityEffect,
}

pub const ABILITIES: &[Ability] = &[
    Ability { id: "heroic_strike", class: Class::Warrior, min_level: 1, mana: 5,  cooldown: 3.0,  range: 90.0,  effect: AbilityEffect::Damage(22) },
    Ability { id: "whirlwind",     class: Class::Warrior, min_level: 4, mana: 12, cooldown: 8.0,  range: 140.0, effect: AbilityEffect::Aoe(16, 140.0) },
    Ability { id: "aimed_shot",    class: Class::Hunter,  min_level: 1, mana: 6,  cooldown: 4.0,  range: 320.0, effect: AbilityEffect::Damage(20) },
    Ability { id: "multi_shot",    class: Class::Hunter,  min_level: 4, mana: 14, cooldown: 8.0,  range: 160.0, effect: AbilityEffect::Aoe(12, 160.0) },
    Ability { id: "smite",         class: Class::Priest,  min_level: 1, mana: 6,  cooldown: 3.0,  range: 260.0, effect: AbilityEffect::Damage(16) },
    Ability { id: "heal",          class: Class::Priest,  min_level: 2, mana: 12, cooldown: 5.0,  range: 0.0,   effect: AbilityEffect::Heal(35) },
    Ability { id: "firebolt",      class: Class::Mage,    min_level: 1, mana: 6,  cooldown: 2.5,  range: 280.0, effect: AbilityEffect::Damage(24) },
    Ability { id: "frost_nova",    class: Class::Mage,    min_level: 4, mana: 15, cooldown: 10.0, range: 150.0, effect: AbilityEffect::Aoe(12, 150.0) },
];

pub fn ability(id: &str) -> Option<&'static Ability> {
    ABILITIES.iter().find(|a| a.id == id)
}

/// Talent ids are `<class>_<branch>`; each has `TALENT_MAX_RANK` ranks.
/// power: +4% damage/rank · toughness: +12 max HP/rank · spirit: +6% healing/rank.
pub const TALENT_BRANCHES: [&str; 3] = ["power", "toughness", "spirit"];
pub const TALENT_MAX_RANK: u32 = 5;

fn damage_mult(sheet: &CharacterSheet) -> f32 {
    let rank = sheet
        .class
        .and_then(|c| sheet.talents.get(&format!("{}_power", c.as_str())))
        .copied()
        .unwrap_or(0);
    1.0 + 0.04 * rank as f32
}

fn heal_mult(sheet: &CharacterSheet) -> f32 {
    let rank = sheet
        .class
        .and_then(|c| sheet.talents.get(&format!("{}_spirit", c.as_str())))
        .copied()
        .unwrap_or(0);
    1.0 + 0.06 * rank as f32
}

/// Crafting recipes: (id, required (profession, skill), inputs, output).
pub struct Recipe {
    pub id: &'static str,
    pub needs: Option<(&'static str, u32)>,
    pub inputs: &'static [(&'static str, usize)],
    pub output: &'static str,
}

pub const RECIPES: &[Recipe] = &[
    Recipe { id: "bread",     needs: None,                    inputs: &[("wood", 2)],               output: "bread" },
    Recipe { id: "stone_axe", needs: Some(("mining", 5)),     inputs: &[("stone", 2), ("wood", 1)], output: "stone_axe" },
    Recipe { id: "oak_staff", needs: Some(("woodcutting", 5)), inputs: &[("wood", 3)],              output: "oak_staff" },
    Recipe { id: "hide_vest", needs: None,                    inputs: &[("thick_hide", 2)],         output: "hide_vest" },
];

/// Equippable gear. Bonuses apply only while equipped.
pub struct ItemDef {
    pub id: &'static str,
    pub slot: &'static str, // "weapon" | "chest"
    pub melee: i32,
    pub spell: i32,
    pub hp: i32,
}

pub const ITEMS: &[ItemDef] = &[
    ItemDef { id: "stone_axe",    slot: "weapon", melee: 5,  spell: 0, hp: 0 },
    ItemDef { id: "oak_staff",    slot: "weapon", melee: 0,  spell: 5, hp: 0 },
    ItemDef { id: "bronze_sword", slot: "weapon", melee: 8,  spell: 0, hp: 0 },
    ItemDef { id: "hide_vest",    slot: "chest",  melee: 0,  spell: 0, hp: 30 },
    // Quest rewards (C02), scaled against the act they come from.
    ItemDef { id: "spark_woven_cloak",     slot: "back",   melee: 0,  spell: 3,  hp: 10 },
    ItemDef { id: "bronze_bracers",        slot: "wrist",  melee: 2,  spell: 0,  hp: 5 },
    ItemDef { id: "amulet_of_the_unbound", slot: "neck",   melee: 0,  spell: 5,  hp: 0 },
    ItemDef { id: "astrologers_staff",     slot: "weapon", melee: 0,  spell: 12, hp: 0 },
    ItemDef { id: "iron_greaves",          slot: "legs",   melee: 0,  spell: 0,  hp: 20 },
    ItemDef { id: "ring_of_the_dreamer",   slot: "finger", melee: 3,  spell: 3,  hp: 0 },
    ItemDef { id: "giant_bone_crusher",    slot: "weapon", melee: 14, spell: 0,  hp: 0 },
    ItemDef { id: "lamechs_helm",          slot: "head",   melee: 0,  spell: 0,  hp: 25 },
    ItemDef { id: "enchanted_bronze_blade", slot: "weapon", melee: 11, spell: 4, hp: 0 },
    ItemDef { id: "covenant_signet",       slot: "finger", melee: 5,  spell: 5,  hp: 10 },
];

/// Key items: carried, never equipped or consumed.
pub const KEY_ITEMS: &[&str] = &["dire_wolf_horn"];

/// Consumable quest rewards handled by `use_item` (not equipment).
pub const CONSUMABLES: &[&str] = &[
    "bread", "fruit", "golden_apple", "energy_drink", "bitter_bread", "healing_potion",
];

pub fn item_def(id: &str) -> Option<&'static ItemDef> {
    ITEMS.iter().find(|i| i.id == id)
}

fn gear_bonus(sheet: &CharacterSheet) -> (i32, i32) {
    let mut melee = 0;
    let mut spell = 0;
    for item in sheet.equipment.values() {
        if let Some(d) = item_def(item) {
            melee += d.melee;
            spell += d.spell;
        }
    }
    (melee, spell)
}

use crate::quests::{quest, quests_for, Objective, QUEST_CAP};

/// Distance within which you can talk to an NPC.
const TALK_RANGE: f32 = 140.0;

/// A tiny xorshift RNG so the sim has no external rand dependency and stays
/// deterministic given a seed (useful for reproducible tests / replays).
pub struct Rng(u64);
impl Rng {
    pub fn new(seed: u64) -> Self {
        Self(seed | 1)
    }
    fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
    pub fn range(&mut self, lo: f32, hi: f32) -> f32 {
        let f = (self.next_u64() >> 11) as f32 / (1u64 << 53) as f32;
        lo + f * (hi - lo)
    }
    fn point(&mut self, bound: f32) -> Vec2 {
        Vec2::new(self.range(-bound, bound), self.range(-bound, bound))
    }
}

#[derive(Clone, Copy, PartialEq)]
pub enum AiState {
    Patrol,
    Chase,
    Return,
    Graze,
    Flee,
    Static,
}

pub struct Entity {
    pub id: EntityId,
    pub kind: EntityKind,
    pub pos: Vec2,
    pub rot: f32,
    pub health: i32,
    pub max_health: i32,
    pub tag: Option<String>,
    pub name: Option<String>,
    pub speed: f32,
    // AI / behavior
    pub origin: Vec2,
    pub aggro_range: f32,
    pub patrol_radius: f32,
    pub state: AiState,
    pub state_timer: f32,
    pub attack_cooldown: f32,
    pub damage: i32,
    pub xp_value: u32,
    pub wander_target: Vec2,
    // player-only
    pub intent: Vec2,
    pub attack_queued: bool,
    pub attack_timer: f32,
    pub dead_timer: f32,
    /// Ability id queued for this tick (players).
    pub cast_queued: Option<String>,
    /// Global cooldown remaining.
    pub gcd: f32,
    /// Per-ability cooldowns remaining.
    pub cooldowns: HashMap<String, f32>,
    /// Live duel partner, if dueling.
    pub duel_with: Option<EntityId>,
    /// Fractional-second accumulator for inn rested-XP accrual.
    pub rest_accum: f32,
    /// Riding the Dire-Wolf (players; C06). Cleared by any combat.
    pub mounted: bool,
    /// Seconds until the player counts as out of combat again.
    pub combat_timer: f32,
    /// For a player entity, the owning connection id.
    pub owner: Option<u64>,
    /// For a player, its persistent sheet (kept in sync so we can save it).
    pub sheet: Option<CharacterSheet>,
}

impl Entity {
    pub fn to_state(&self) -> EntityState {
        EntityState {
            id: self.id,
            kind: self.kind,
            x: self.pos.x,
            y: self.pos.y,
            rot: self.rot,
            health: self.health,
            max_health: self.max_health,
            tag: self.tag.clone(),
            name: self.name.clone(),
            weapon: self
                .sheet
                .as_ref()
                .and_then(|s| s.equipment.get("weapon").cloned()),
            chest: self
                .sheet
                .as_ref()
                .and_then(|s| s.equipment.get("chest").cloned()),
            mounted: self.mounted,
        }
    }
}

pub struct Zone {
    pub act: Act,
    pub entities: HashMap<EntityId, Entity>,
    pub tick: u64,
    /// Where players arriving in this zone spawn.
    pub entry: Vec2,
}

pub struct World {
    pub zones: HashMap<Act, Zone>,
    next_id: EntityId,
    rng: Rng,
}

/// Something that happened this tick and needs to be reported outside the sim
/// (e.g. pushed to a specific player's connection).
pub enum SimEvent {
    LevelUp { owner: u64, level: u32 },
    Died { owner: u64 },
    Loot { owner: u64, item: String },
    /// Generic per-player info line (cast failures, duel results, PvP kills).
    Info { owner: u64, text: String },
    /// Cosmetic combat event, broadcast to every client in the act (drives
    /// remote-entity swing/hit/death animations).
    Combat { act: Act, kind: EventKind, src: EntityId, dst: Option<EntityId> },
}

impl World {
    pub fn new(seed: u64) -> Self {
        let mut w = World {
            zones: HashMap::new(),
            next_id: 1,
            rng: Rng::new(seed),
        };
        for act in Act::ALL {
            let mut zone = Zone {
                act,
                entities: HashMap::new(),
                tick: 0,
                entry: Vec2::ZERO,
            };
            w.populate_zone(&mut zone);
            w.zones.insert(act, zone);
        }
        w
    }

    fn alloc_id(&mut self) -> EntityId {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    /// A random point in the playable area at least `min_dist` from `entry`.
    fn safe_point(&mut self, entry: Vec2, min_dist: f32) -> Vec2 {
        loop {
            let p = self.rng.point(WORLD_BOUNDS * 0.9);
            if p.distance(entry) > min_dist {
                return p;
            }
        }
    }

    /// Seed a zone with enemies, wildlife and resource nodes. Counts vary by act
    /// so the later, more dangerous acts feel different.
    fn populate_zone(&mut self, zone: &mut Zone) {
        let (enemy_tag, wildlife_tag) = match zone.act {
            Act::Eden => ("serpent", "deer"),
            Act::Hermon => ("watcher", "goat"),
            Act::Nephilim => ("giant", "boar"),
            Act::Enoch => ("shade", "dog"),
            Act::Flood => ("leviathan", "fox"),
        };
        let act = zone.act;
        // C05: spawns come in clusters ("camps") of up to 6 around a shared
        // center, never inside the safe ring around the inn.
        for (tag, n) in act_spawn_table(act) {
            let mut left = *n;
            while left > 0 {
                let center = self.safe_point(zone.entry, SAFE_RING);
                let group = left.min(6);
                for _ in 0..group {
                    let off = self.rng.point(150.0);
                    let id = self.alloc_id();
                    zone.entities.insert(id, make_enemy(id, center + off, tag, act));
                }
                left -= group;
            }
        }
        for _ in 0..2 {
            let center = self.safe_point(zone.entry, SAFE_RING);
            for _ in 0..4 {
                let off = self.rng.point(180.0);
                let id = self.alloc_id();
                zone.entities.insert(id, make_wildlife(id, center + off, wildlife_tag));
            }
        }
        // Bestiary species (C03): a dozen level-appropriate mobs from the
        // act's habitat. Hostile temperaments spawn as enemies, docile and
        // skittish ones as neutral wildlife.
        let species = crate::mobs::species_for_act(act);
        if !species.is_empty() {
            for _ in 0..12 {
                let def = species[(self.rng.range(0.0, species.len() as f32) as usize).min(species.len() - 1)];
                // Keep the (often higher-level) bestiary fauna out of the
                // starting inn area so fresh characters aren't ganked.
                let pos = loop {
                    let p = self.rng.point(WORLD_BOUNDS * 0.9);
                    if p.distance(zone.entry) > 700.0 {
                        break p;
                    }
                };
                let id = self.alloc_id();
                if def.hostile() {
                    zone.entities.insert(id, make_enemy(id, pos, &def.tag, act));
                } else {
                    let lvl = def.mid_level() as i32;
                    let mut e = make_wildlife(id, pos, &def.tag);
                    e.name = Some(def.name.clone());
                    e.max_health = 15 + lvl * 4;
                    e.health = e.max_health;
                    e.xp_value = (5 + lvl) as u32;
                    zone.entities.insert(id, e);
                }
            }
        }
        for _ in 0..5 {
            // Resource groves: a stand of trees or a rock outcrop.
            let center = self.safe_point(zone.entry, 350.0);
            let tag = if self.rng.range(0.0, 1.0) < 0.6 { "tree" } else { "rock" };
            for _ in 0..4 {
                let off = self.rng.point(140.0);
                let id = self.alloc_id();
                zone.entities.insert(id, make_resource(id, center + off, tag));
            }
        }
        // The act's quest givers: the Elder at the inn, a Wanderer at the
        // crossroads, and a Seer further afield (see quests.rs giver mapping).
        let id = self.alloc_id();
        zone.entities.insert(id, make_npc(id, zone.entry + Vec2::new(90.0, 0.0), "Elder"));
        let id = self.alloc_id();
        zone.entities.insert(id, make_npc(id, zone.entry + Vec2::new(-70.0, 110.0), "Wanderer"));
        let id = self.alloc_id();
        zone.entities.insert(id, make_npc(id, zone.entry + Vec2::new(260.0, -170.0), "Seer"));
        // Beast-Master Jabal offers the mount questline in Enoch (C06).
        if act == Act::Enoch {
            let id = self.alloc_id();
            zone.entities.insert(id, make_npc(id, zone.entry + Vec2::new(0.0, -220.0), "Jabal"));
        }
        // One elite "alpha" per act — the dungeon-boss placeholder. Guaranteed
        // rare drop (thick_hide) and big XP.
        let pos = self.rng.point(WORLD_BOUNDS * 0.7);
        let id = self.alloc_id();
        let mut boss = make_enemy(id, pos, enemy_tag, act);
        boss.tag = Some(format!("{enemy_tag}_alpha"));
        boss.max_health *= 4;
        boss.health = boss.max_health;
        boss.damage *= 2;
        boss.xp_value *= 5;
        boss.aggro_range = 300.0;
        zone.entities.insert(id, boss);
    }

    /// Spawn a player entity into its zone from a character sheet. Returns the
    /// new entity id.
    pub fn spawn_player(&mut self, owner: u64, sheet: CharacterSheet) -> EntityId {
        let id = self.alloc_id();
        let act = sheet.act;
        let pos = Vec2::new(sheet.x, sheet.y);
        let ent = Entity {
            id,
            kind: EntityKind::Player,
            pos,
            rot: 0.0,
            health: sheet.health,
            max_health: sheet.max_health,
            // Class rides the tag so clients can pick the right character model.
            tag: sheet.class.map(|c| c.as_str().to_string()),
            name: Some(sheet.name.clone()),
            speed: PLAYER_SPEED,
            origin: pos,
            aggro_range: 0.0,
            patrol_radius: 0.0,
            state: AiState::Static,
            state_timer: 0.0,
            attack_cooldown: 0.0,
            damage: PLAYER_ATTACK_DMG,
            xp_value: 0,
            wander_target: pos,
            intent: Vec2::ZERO,
            attack_queued: false,
            attack_timer: 0.0,
            dead_timer: 0.0,
            cast_queued: None,
            gcd: 0.0,
            cooldowns: HashMap::new(),
            duel_with: None,
            rest_accum: 0.0,
            mounted: false,
            combat_timer: 0.0,
            owner: Some(owner),
            sheet: Some(sheet),
        };
        self.zones.get_mut(&act).unwrap().entities.insert(id, ent);
        id
    }

    pub fn remove_player(&mut self, act: Act, id: EntityId) -> Option<CharacterSheet> {
        self.zones
            .get_mut(&act)
            .and_then(|z| z.entities.remove(&id))
            .and_then(|e| e.sheet)
    }

    /// Advance every zone one tick. Returns per-tick events keyed by owner
    /// connection so the caller can notify the right players.
    pub fn step(&mut self) -> Vec<SimEvent> {
        let mut events = Vec::new();
        let acts: Vec<Act> = self.zones.keys().copied().collect();
        for act in acts {
            self.step_zone(act, &mut events);
        }
        events
    }

    fn step_zone(&mut self, act: Act, events: &mut Vec<SimEvent>) {
        // The tick needs the zone, the rng, and the id counter simultaneously.
        // Move rng + counter into locals so the only borrow of `self` in the
        // hot loop is `self.zones`; restore them at the end.
        let mut rng = std::mem::replace(&mut self.rng, Rng::new(1));
        let mut next_id = self.next_id;
        let rng = &mut rng;

        let zone = self.zones.get_mut(&act).unwrap();
        zone.tick += 1;

        // Snapshots of live players and enemies for cross-entity AI/combat.
        // (id, pos, pvp-flagged, duel partner)
        let player_info: Vec<(EntityId, Vec2, bool, Option<EntityId>)> = zone
            .entities
            .values()
            .filter(|e| e.kind == EntityKind::Player && e.health > 0)
            .map(|e| {
                let pvp = e.sheet.as_ref().map(|s| s.pvp).unwrap_or(false);
                (e.id, e.pos, pvp, e.duel_with)
            })
            .collect();
        let players: Vec<(EntityId, Vec2)> =
            player_info.iter().map(|(id, p, _, _)| (*id, *p)).collect();
        let enemies: Vec<(EntityId, Vec2)> = zone
            .entities
            .values()
            .filter(|e| e.kind == EntityKind::Enemy && e.health > 0)
            .map(|e| (e.id, e.pos))
            .collect();
        // Harvestable resource nodes, for melee harvesting.
        let resources: Vec<(EntityId, Vec2)> = zone
            .entities
            .values()
            .filter(|e| e.kind == EntityKind::Resource)
            .map(|e| (e.id, e.pos))
            .collect();

        // (target_id, damage, attacker_owner_entity_if_player_source)
        let mut damage: Vec<(EntityId, i32, Option<EntityId>)> = Vec::new();
        let entry = zone.entry;

        for e in zone.entities.values_mut() {
            if e.attack_cooldown > 0.0 {
                e.attack_cooldown -= DT;
            }
            match e.kind {
                EntityKind::Player => {
                    if e.health <= 0 {
                        e.dead_timer -= DT;
                        if e.dead_timer <= 0.0 {
                            e.health = e.max_health;
                            e.pos = entry;
                            if let Some(o) = e.owner {
                                events.push(SimEvent::Died { owner: o });
                            }
                        }
                        continue;
                    }
                    let mut fatigue_penalty = 1.0;
                    if let Some(sheet) = &mut e.sheet {
                        // Drain wakefulness (100 -> 0 over 16 hours)
                        sheet.wakefulness = (sheet.wakefulness - (DT * 0.001736)).max(0.0);
                        if sheet.wakefulness < 5.0 {
                            fatigue_penalty = 0.5; // 50% movement speed when exhausted
                        }
                    }
                    if e.combat_timer > 0.0 {
                        e.combat_timer -= DT;
                    }
                    if e.intent.length_squared() > 0.0001 {
                        let dir = e.intent.normalize_or_zero();
                        let mount_mult = if e.mounted { 1.6 } else { 1.0 };
                        e.pos += dir * (e.speed * fatigue_penalty * mount_mult) * DT;
                        e.pos = e.pos.clamp(Vec2::splat(-WORLD_BOUNDS), Vec2::splat(WORLD_BOUNDS));
                        e.rot = dir.y.atan2(dir.x);
                    }
                    if e.gcd > 0.0 {
                        e.gcd -= DT;
                    }
                    for cd in e.cooldowns.values_mut() {
                        *cd -= DT;
                    }
                    e.cooldowns.retain(|_, cd| *cd > 0.0);
                    let mut poi_xp: u32 = 0;
                    if let Some(s) = e.sheet.as_mut() {
                        let regen = (HEALTH_REGEN_PER_SEC * DT).round() as i32;
                        if e.health < e.max_health {
                            e.health = (e.health + regen).min(e.max_health);
                        }
                        s.health = e.health;
                        if s.mana < s.max_mana {
                            s.mana = (s.mana + (MANA_REGEN_PER_SEC * DT).round() as i32).min(s.max_mana);
                        }
                        // Inn: resting near the zone entry banks rested XP.
                        if e.pos.distance(entry) <= INN_RADIUS && s.rested_xp < RESTED_CAP {
                            e.rest_accum += DT;
                            if e.rest_accum >= 1.0 {
                                e.rest_accum -= 1.0;
                                s.rested_xp = (s.rested_xp + RESTED_PER_SEC).min(RESTED_CAP);
                            }
                        }
                        // POI discovery (C04): first visit announces the
                        // place and grants discovery XP by act tier.
                        for poi in crate::pois::pois_for_act(act) {
                            if e.pos.distance(Vec2::new(poi.x, poi.y)) <= crate::pois::POI_RADIUS
                                && !s.discovered.iter().any(|d| d == &poi.name)
                            {
                                s.discovered.push(poi.name.clone());
                                let tier = Act::ALL.iter().position(|a| *a == act).unwrap_or(0) as u32;
                                let xp = 50 * (tier + 1);
                                if let Some(o) = e.owner {
                                    events.push(SimEvent::Info {
                                        owner: o,
                                        text: format!("Discovered: {} (+{} xp)", poi.name, xp),
                                    });
                                }
                                poi_xp += xp;
                                break; // one discovery per tick keeps notices readable
                            }
                        }
                    }
                    if poi_xp > 0 && award_xp(e, poi_xp) {
                        if let Some(o) = e.owner {
                            let lvl = e.sheet.as_ref().map(|s| s.level).unwrap_or(1);
                            events.push(SimEvent::LevelUp { owner: o, level: lvl });
                        }
                    }
                    // Talent/gear damage bonuses.
                    let (dmg_mult, melee_bonus, spell_bonus) = match e.sheet.as_ref() {
                        Some(s) => {
                            let (m, sp) = gear_bonus(s);
                            (damage_mult(s), m, sp)
                        }
                        None => (1.0, 0, 0),
                    };
                    // A player may be hit by another player only in a mutual
                    // duel or when both are PvP-flagged.
                    let my_pvp = e.sheet.as_ref().map(|s| s.pvp).unwrap_or(false);
                    let pvp_targets: Vec<(EntityId, Vec2)> = player_info
                        .iter()
                        .filter(|(pid, _, ppvp, pduel)| {
                            *pid != e.id
                                && (e.duel_with == Some(*pid) && *pduel == Some(e.id)
                                    || (my_pvp && *ppvp))
                        })
                        .map(|(pid, ppos, _, _)| (*pid, *ppos))
                        .collect();
                    if e.attack_queued && e.attack_cooldown <= 0.0 {
                        e.attack_cooldown = ATTACK_COOLDOWN;
                        events.push(SimEvent::Combat {
                            act,
                            kind: EventKind::Attack,
                            src: e.id,
                            dst: None,
                        });
                        let facing = Vec2::new(e.rot.cos(), e.rot.sin());
                        let melee_dmg = ((e.damage + melee_bonus) as f32 * dmg_mult).round() as i32;
                        for (eid, epos) in enemies.iter().chain(pvp_targets.iter()) {
                            let to = *epos - e.pos;
                            let d = to.length();
                            if d <= MELEE_RANGE && d > 0.01 && to.normalize().dot(facing) >= MELEE_ARC_DOT {
                                damage.push((*eid, melee_dmg, Some(e.id)));
                            }
                        }
                        // A swing also harvests a resource node in front of you
                        // (nodes have 1 HP, so one hit fells them).
                        for (rid, rpos) in &resources {
                            let to = *rpos - e.pos;
                            let d = to.length();
                            if d <= MELEE_RANGE && d > 0.01 && to.normalize().dot(facing) >= MELEE_ARC_DOT {
                                damage.push((*rid, 1, Some(e.id)));
                            }
                        }
                    }
                    e.attack_queued = false;
                    // Resolve a queued ability cast.
                    if let Some(ab_id) = e.cast_queued.take() {
                        let owner = e.owner.unwrap_or(0);
                        let fail = |events: &mut Vec<SimEvent>, text: &str| {
                            events.push(SimEvent::Info { owner, text: text.to_string() });
                        };
                        let Some(ab) = ability(&ab_id) else {
                            fail(events, "Unknown ability.");
                            continue;
                        };
                        let sheet_ok = e.sheet.as_ref().map(|s| {
                            (s.class == Some(ab.class), s.level >= ab.min_level, s.mana >= ab.mana)
                        });
                        match sheet_ok {
                            Some((true, true, true)) => {}
                            Some((false, _, _)) => { fail(events, "That ability is not of your class."); continue; }
                            Some((_, false, _)) => { fail(events, "You are not high enough level."); continue; }
                            Some((_, _, false)) => { fail(events, "Not enough mana."); continue; }
                            None => continue,
                        }
                        if e.gcd > 0.0 || e.cooldowns.get(ab.id).copied().unwrap_or(0.0) > 0.0 {
                            fail(events, "Ability not ready.");
                            continue;
                        }
                        e.gcd = GCD_SECS;
                        e.cooldowns.insert(ab.id.to_string(), ab.cooldown);
                        events.push(SimEvent::Combat {
                            act,
                            kind: EventKind::Cast,
                            src: e.id,
                            dst: None,
                        });
                        if let Some(s) = e.sheet.as_mut() {
                            s.mana -= ab.mana;
                        }
                        match ab.effect {
                            AbilityEffect::Damage(base) => {
                                let dmg = ((base + spell_bonus) as f32 * dmg_mult).round() as i32;
                                let facing = Vec2::new(e.rot.cos(), e.rot.sin());
                                // Nearest valid target in range, roughly in front.
                                let target = enemies
                                    .iter()
                                    .chain(pvp_targets.iter())
                                    .filter_map(|(tid, tpos)| {
                                        let to = *tpos - e.pos;
                                        let d = to.length();
                                        (d <= ab.range && d > 0.01 && to.normalize().dot(facing) >= 0.0)
                                            .then_some((*tid, d))
                                    })
                                    .min_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
                                match target {
                                    Some((tid, _)) => damage.push((tid, dmg, Some(e.id))),
                                    None => fail(events, "No target in range."),
                                }
                            }
                            AbilityEffect::Aoe(base, radius) => {
                                let dmg = ((base + spell_bonus) as f32 * dmg_mult).round() as i32;
                                for (tid, tpos) in enemies.iter().chain(pvp_targets.iter()) {
                                    if e.pos.distance(*tpos) <= radius {
                                        damage.push((*tid, dmg, Some(e.id)));
                                    }
                                }
                            }
                            AbilityEffect::Heal(base) => {
                                let amount = (base as f32
                                    * e.sheet.as_ref().map(heal_mult).unwrap_or(1.0))
                                .round() as i32;
                                e.health = (e.health + amount).min(e.max_health);
                                if let Some(s) = e.sheet.as_mut() {
                                    s.health = e.health;
                                }
                            }
                        }
                    }
                }
                EntityKind::Enemy => {
                    if e.health <= 0 {
                        continue;
                    }
                    let nearest = nearest_of(&players, e.pos);
                    match nearest {
                        Some((pid, ppos, dist)) if dist <= e.aggro_range => {
                            e.state = AiState::Chase;
                            let to = ppos - e.pos;
                            if dist > ENEMY_MELEE_RANGE {
                                let dir = to.normalize_or_zero();
                                e.pos += dir * e.speed * DT;
                                e.rot = dir.y.atan2(dir.x);
                            } else if e.attack_cooldown <= 0.0 {
                                e.attack_cooldown = ATTACK_COOLDOWN;
                                damage.push((pid, e.damage, None));
                                events.push(SimEvent::Combat {
                                    act,
                                    kind: EventKind::Attack,
                                    src: e.id,
                                    dst: Some(pid),
                                });
                            }
                        }
                        _ => {
                            e.state = AiState::Patrol;
                            e.state_timer -= DT;
                            if e.state_timer <= 0.0 || e.pos.distance(e.wander_target) < 20.0 {
                                e.state_timer = rng.range(1.5, 4.0);
                                let ang = rng.range(0.0, std::f32::consts::TAU);
                                let r = rng.range(0.0, e.patrol_radius);
                                e.wander_target = e.origin + Vec2::new(ang.cos(), ang.sin()) * r;
                            }
                            let dir = (e.wander_target - e.pos).normalize_or_zero();
                            e.pos += dir * (e.speed * 0.5) * DT;
                            if dir.length_squared() > 0.0 {
                                e.rot = dir.y.atan2(dir.x);
                            }
                        }
                    }
                }
                EntityKind::Wildlife => {
                    let nearest = nearest_of(&players, e.pos);
                    match nearest {
                        Some((_, ppos, dist)) if dist <= e.aggro_range => {
                            e.state = AiState::Flee;
                            let dir = (e.pos - ppos).normalize_or_zero();
                            e.pos += dir * e.speed * DT;
                            e.rot = dir.y.atan2(dir.x);
                        }
                        _ => {
                            e.state = AiState::Graze;
                            e.state_timer -= DT;
                            if e.state_timer <= 0.0 {
                                e.state_timer = rng.range(2.0, 5.0);
                                let ang = rng.range(0.0, std::f32::consts::TAU);
                                let r = rng.range(0.0, e.patrol_radius);
                                e.wander_target = e.origin + Vec2::new(ang.cos(), ang.sin()) * r;
                            }
                            let dir = (e.wander_target - e.pos).normalize_or_zero();
                            e.pos += dir * (e.speed * 0.35) * DT;
                        }
                    }
                    e.pos = e.pos.clamp(Vec2::splat(-WORLD_BOUNDS), Vec2::splat(WORLD_BOUNDS));
                }
                EntityKind::Resource | EntityKind::Npc => {}
            }
        }

        // Apply damage; collect kills. A dueling player never dies to duel
        // damage — at 0 HP the duel ends and they stay at 1 HP.
        let mut killed: Vec<(EntityId, Option<EntityId>)> = Vec::new();
        let mut duel_over: Vec<(EntityId, EntityId)> = Vec::new(); // (loser, winner)
        for (target, dmg, attacker) in damage {
            // Combat dismounts and marks both sides in-combat (C06).
            if let Some(att) = attacker {
                if let Some(a) = zone.entities.get_mut(&att) {
                    a.mounted = false;
                    a.combat_timer = COMBAT_LOCK_SECS;
                }
            }
            if let Some(t) = zone.entities.get_mut(&target) {
                if t.health <= 0 {
                    continue;
                }
                t.mounted = false;
                t.combat_timer = COMBAT_LOCK_SECS;
                t.health -= dmg;
                if t.health > 0 {
                    events.push(SimEvent::Combat {
                        act,
                        kind: EventKind::Hit,
                        src: attacker.unwrap_or(0),
                        dst: Some(target),
                    });
                }
                if t.health <= 0 {
                    if t.kind == EntityKind::Player {
                        if let (Some(partner), Some(att)) = (t.duel_with, attacker) {
                            if partner == att {
                                t.health = 1;
                                if let Some(s) = t.sheet.as_mut() {
                                    s.health = 1;
                                }
                                duel_over.push((target, partner));
                                continue;
                            }
                        }
                        t.dead_timer = RESPAWN_SECS;
                    }
                    events.push(SimEvent::Combat {
                        act,
                        kind: EventKind::Die,
                        src: target,
                        dst: attacker,
                    });
                    killed.push((target, attacker));
                }
            }
        }
        for (loser, winner) in duel_over {
            for (id, other) in [(loser, winner), (winner, loser)] {
                if let Some(p) = zone.entities.get_mut(&id) {
                    p.duel_with = None;
                    if let Some(o) = p.owner {
                        let text = if id == winner {
                            "You won the duel!".to_string()
                        } else {
                            "You lost the duel.".to_string()
                        };
                        events.push(SimEvent::Info { owner: o, text });
                    }
                    let _ = other;
                }
            }
        }

        // Resolve kills. Enemies grant xp + a trophy and respawn a replacement;
        // harvested resource nodes grant a material and respawn elsewhere.
        for (target_id, attacker_ent) in killed {
            let (kind, xp_value, etag) = match zone.entities.get(&target_id) {
                Some(t) if t.kind == EntityKind::Enemy => (EntityKind::Enemy, t.xp_value, t.tag.clone()),
                Some(t) if t.kind == EntityKind::Resource => (EntityKind::Resource, 0, t.tag.clone()),
                Some(t) if t.kind == EntityKind::Player => {
                    // World-PvP kill: victim respawns via dead_timer; killer
                    // gets a notice but no XP (no farming players for levels).
                    if let Some(att) = attacker_ent.and_then(|a| zone.entities.get(&a)) {
                        if let Some(o) = att.owner {
                            events.push(SimEvent::Info { owner: o, text: "Honorable kill!".into() });
                        }
                    }
                    continue;
                }
                _ => continue,
            };
            zone.entities.remove(&target_id);

            // Respawn a replacement elsewhere so the zone stays populated
            // (enemies never respawn inside the inn's safe ring).
            let pos = loop {
                let p = rng.point(WORLD_BOUNDS * 0.9);
                if kind != EntityKind::Enemy || p.length() > SAFE_RING {
                    break p;
                }
            };
            let nid = self_next_id(&mut next_id);
            let (reward_item, xp) = match kind {
                EntityKind::Enemy => {
                    let tag = etag.as_deref().unwrap_or("serpent");
                    // Bestiary species (incl. names that happen to end in
                    // "_alpha") respawn as themselves and drop bestiary loot.
                    if let Some(def) = crate::mobs::mob_by_tag(tag) {
                        zone.entities.insert(nid, make_enemy(nid, pos, tag, act));
                        let reward = if def.drops.is_empty() {
                            format!("{tag}_trophy")
                        } else {
                            let i = rng.range(0.0, def.drops.len() as f32) as usize;
                            def.drops[i.min(def.drops.len() - 1)].clone()
                        };
                        (reward, xp_value)
                    } else if let Some(base) = tag.strip_suffix("_alpha") {
                        // The act boss respawns as a boss and drops a rare.
                        let mut boss = make_enemy(nid, pos, base, act);
                        boss.tag = Some(tag.to_string());
                        boss.max_health *= 4;
                        boss.health = boss.max_health;
                        boss.damage *= 2;
                        boss.xp_value *= 5;
                        zone.entities.insert(nid, boss);
                        ("thick_hide".to_string(), xp_value)
                    } else {
                        zone.entities.insert(nid, make_enemy(nid, pos, tag, act));
                        (format!("{tag}_trophy"), xp_value)
                    }
                }
                EntityKind::Resource => {
                    let tag = etag.as_deref().unwrap_or("tree");
                    zone.entities.insert(nid, make_resource(nid, pos, tag));
                    let material = if tag == "rock" { "stone" } else { "wood" };
                    (material.to_string(), 0)
                }
                _ => continue,
            };

            // Award xp + loot to the killer/harvester.
            if let Some(owner_ent) = attacker_ent {
                if let Some(p) = zone.entities.get_mut(&owner_ent) {
                    if let Some(o) = p.owner {
                        if xp > 0 && award_xp(p, xp) {
                            let lvl = p.sheet.as_ref().map(|s| s.level).unwrap_or(1);
                            events.push(SimEvent::LevelUp { owner: o, level: lvl });
                            events.push(SimEvent::Combat {
                                act,
                                kind: EventKind::LevelUp,
                                src: owner_ent,
                                dst: None,
                            });
                        }
                        if let Some(s) = p.sheet.as_mut() {
                            match kind {
                                // Enemy kills also pay gold and advance quests.
                                EntityKind::Enemy => {
                                    let tier = Act::ALL.iter().position(|a| *a == act).unwrap_or(0) as u32;
                                    s.gold += 2 + tier * 2;
                                }
                                // Harvesting levels the matching profession.
                                EntityKind::Resource => {
                                    let prof = if reward_item == "stone" { "mining" } else { "woodcutting" };
                                    let skill = s.professions.entry(prof.to_string()).or_insert(0);
                                    if *skill < PROFESSION_CAP {
                                        *skill += 1;
                                    }
                                }
                                _ => {}
                            }
                            // Advance every active quest this kill/harvest
                            // satisfies: Kill objectives count enemy kills,
                            // Collect objectives loot the quest item (100%
                            // while active) from enemy OR resource sources.
                            let active: Vec<String> = s.quests.keys().cloned().collect();
                            for qid in active {
                                let Some(def) = quest(&qid) else { continue };
                                match def.objective {
                                    Objective::Kill { target, count } => {
                                        let hits = kind == EntityKind::Enemy
                                            && etag.as_deref().map(|t| t.starts_with(target)).unwrap_or(false);
                                        if !hits { continue }
                                        let prog = s.quests.get_mut(&qid).unwrap();
                                        if *prog < count {
                                            *prog += 1;
                                            events.push(SimEvent::Info {
                                                owner: o,
                                                text: format!("Quest: {} — {}/{}", qid, prog, count),
                                            });
                                        }
                                    }
                                    Objective::Collect { item, count, source } => {
                                        let hits = etag.as_deref().map(|t| t.starts_with(source)).unwrap_or(false);
                                        if !hits { continue }
                                        let have = s.inventory.iter().filter(|i| i.as_str() == item).count() as u32;
                                        if have < count && s.inventory.len() < 40 {
                                            s.inventory.push(item.to_string());
                                            s.quests.insert(qid.clone(), have + 1);
                                            events.push(SimEvent::Info {
                                                owner: o,
                                                text: format!("Quest: {} — {}/{} {}", qid, have + 1, count, item),
                                            });
                                        }
                                    }
                                }
                            }
                            if s.inventory.len() < 40 {
                                s.inventory.push(reward_item.clone());
                                events.push(SimEvent::Loot { owner: o, item: reward_item });
                            }
                        }
                    }
                }
            }
        }

        // Restore rng + counter into self.
        self.rng = std::mem::replace(rng, Rng::new(1));
        self.next_id = next_id;
    }

    pub fn set_intent(&mut self, act: Act, id: EntityId, dir: Vec2) {
        if let Some(z) = self.zones.get_mut(&act) {
            if let Some(e) = z.entities.get_mut(&id) {
                e.intent = dir;
            }
        }
    }

    pub fn queue_attack(&mut self, act: Act, id: EntityId) {
        if let Some(z) = self.zones.get_mut(&act) {
            if let Some(e) = z.entities.get_mut(&id) {
                e.attack_queued = true;
            }
        }
    }

    pub fn queue_cast(&mut self, act: Act, id: EntityId, ability: String) {
        if let Some(e) = self.entity_mut(act, id) {
            e.cast_queued = Some(ability);
        }
    }

    fn entity_mut(&mut self, act: Act, id: EntityId) -> Option<&mut Entity> {
        self.zones.get_mut(&act)?.entities.get_mut(&id)
    }

    fn sheet_mut(&mut self, act: Act, id: EntityId) -> Option<&mut CharacterSheet> {
        self.entity_mut(act, id)?.sheet.as_mut()
    }

    /// Choose a class (once). Applies the class's base-stat kit.
    pub fn select_class(&mut self, act: Act, id: EntityId, class: Class) -> Result<String, String> {
        let e = self.entity_mut(act, id).ok_or("no such player")?;
        let s = e.sheet.as_mut().ok_or("no sheet")?;
        if s.class.is_some() {
            return Err("You have already chosen a class.".into());
        }
        s.class = Some(class);
        let (hp, mana) = match class {
            Class::Warrior => (40, 0),
            Class::Hunter => (20, 15),
            Class::Priest => (10, 40),
            Class::Mage => (0, 50),
        };
        s.max_health += hp;
        s.health = s.max_health;
        s.max_mana += mana;
        s.mana = s.max_mana;
        e.max_health = s.max_health;
        e.health = s.health;
        e.tag = Some(class.as_str().to_string());
        Ok(format!("You are now a {}.", class.as_str()))
    }

    /// Spend one talent point on `talent` (id `<class>_<branch>`).
    pub fn learn_talent(&mut self, act: Act, id: EntityId, talent: &str) -> Result<String, String> {
        let e = self.entity_mut(act, id).ok_or("no such player")?;
        let s = e.sheet.as_mut().ok_or("no sheet")?;
        let class = s.class.ok_or("Choose a class first.")?;
        let valid = TALENT_BRANCHES
            .iter()
            .any(|b| talent == format!("{}_{}", class.as_str(), b));
        if !valid {
            return Err(format!("Unknown talent for your class: {talent}"));
        }
        if s.talent_points == 0 {
            return Err("No unspent talent points.".into());
        }
        let rank = s.talents.entry(talent.to_string()).or_insert(0);
        if *rank >= TALENT_MAX_RANK {
            return Err("That talent is already at max rank.".into());
        }
        *rank += 1;
        let rank = *rank;
        s.talent_points -= 1;
        if talent.ends_with("_toughness") {
            s.max_health += 12;
            s.health = s.max_health.min(s.health + 12);
            e.max_health = s.max_health;
            e.health = e.health.max(s.health);
        }
        Ok(format!("Learned {talent} (rank {rank})."))
    }

    /// Craft a recipe: checks profession skill, consumes inputs, adds output.
    pub fn craft(&mut self, act: Act, id: EntityId, recipe: &str) -> Result<String, String> {
        let r = RECIPES.iter().find(|r| r.id == recipe).ok_or("Unknown recipe.")?;
        let s = self.sheet_mut(act, id).ok_or("no sheet")?;
        if let Some((prof, min)) = r.needs {
            let skill = s.professions.get(prof).copied().unwrap_or(0);
            if skill < min {
                return Err(format!("Requires {prof} {min} (you have {skill})."));
            }
        }
        for (item, n) in r.inputs {
            let have = s.inventory.iter().filter(|i| i.as_str() == *item).count();
            if have < *n {
                return Err(format!("Requires {n}x {item} (you have {have})."));
            }
        }
        for (item, n) in r.inputs {
            for _ in 0..*n {
                let idx = s.inventory.iter().position(|i| i == item).unwrap();
                s.inventory.remove(idx);
            }
        }
        s.inventory.push(r.output.to_string());
        Ok(format!("You craft a {}.", r.output))
    }

    /// Use a consumable from the inventory.
    pub fn use_item(&mut self, act: Act, id: EntityId, item: &str) -> Result<String, String> {
        let e = self.entity_mut(act, id).ok_or("no such player")?;
        let s = e.sheet.as_mut().ok_or("no sheet")?;
        let idx = s.inventory.iter().position(|i| i == item).ok_or("You don't have that.")?;
        match item {
            "bread" => {
                s.inventory.remove(idx);
                e.health = (e.health + 40).min(e.max_health);
                s.health = e.health;
                Ok("You eat the bread and recover 40 health.".into())
            }
            "fruit" => {
                s.inventory.remove(idx);
                s.wakefulness = (s.wakefulness + 10.0).min(100.0);
                Ok("You eat the fruit and feel slightly more awake.".into())
            }
            "golden_apple" => {
                s.inventory.remove(idx);
                s.wakefulness = 100.0;
                Ok("You consume the Golden Apple. Your exhaustion completely vanishes!".into())
            }
            "energy_drink" => {
                s.inventory.remove(idx);
                s.wakefulness = (s.wakefulness + 50.0).min(100.0);
                Ok("You chug the premium energy drink and feel a massive surge of stamina.".into())
            }
            "bitter_bread" => {
                s.inventory.remove(idx);
                e.health = (e.health + 25).min(e.max_health);
                s.health = e.health;
                Ok("You choke down the bitter bread and recover 25 health.".into())
            }
            "healing_potion" => {
                s.inventory.remove(idx);
                e.health = (e.health + 60).min(e.max_health);
                s.health = e.health;
                Ok("You drink the healing potion and recover 60 health.".into())
            }
            _ => Err("You can't use that.".into()),
        }
    }

    /// Equip gear from the inventory into its slot, swapping out the old piece.
    pub fn equip(&mut self, act: Act, id: EntityId, item: &str) -> Result<String, String> {
        let def = item_def(item).ok_or("That can't be equipped.")?;
        let e = self.entity_mut(act, id).ok_or("no such player")?;
        let s = e.sheet.as_mut().ok_or("no sheet")?;
        let idx = s.inventory.iter().position(|i| i == item).ok_or("You don't have that.")?;
        s.inventory.remove(idx);
        if let Some(old) = s.equipment.insert(def.slot.to_string(), item.to_string()) {
            if let Some(old_def) = item_def(&old) {
                s.max_health -= old_def.hp;
            }
            s.inventory.push(old);
        }
        s.max_health += def.hp;
        s.health = s.health.min(s.max_health);
        e.max_health = s.max_health;
        e.health = s.health;
        Ok(format!("You equip the {item}."))
    }

    /// Talk to the nearest NPC. Priority: turn in a completed quest from this
    /// giver → accept the first available quest (prerequisites met, under the
    /// concurrent cap) → report progress on one in flight → pleasantries.
    pub fn talk(&mut self, act: Act, id: EntityId) -> Result<String, String> {
        let z = self.zones.get(&act).ok_or("no zone")?;
        let pos = z.entities.get(&id).map(|e| e.pos).ok_or("no such player")?;
        let npc = z
            .entities
            .values()
            .filter(|e| e.kind == EntityKind::Npc && e.pos.distance(pos) <= TALK_RANGE)
            .min_by(|a, b| a.pos.distance(pos).total_cmp(&b.pos.distance(pos)))
            .map(|e| e.name.clone().unwrap_or_else(|| "Elder".into()))
            .ok_or("There is no one nearby to talk to.")?;

        let e = self.entity_mut(act, id).unwrap();
        let s = e.sheet.as_mut().ok_or("no sheet")?;

        // 1) Turn-in: any of this giver's quests active and complete?
        let turn_in = quests_for(act, &npc).find(|q| {
            s.quests.get(q.id).is_some_and(|p| *p >= q.objective.count())
        });
        if let Some(q) = turn_in {
            if let Objective::Collect { item, count, .. } = q.objective {
                // Consume the collected items.
                for _ in 0..count {
                    if let Some(idx) = s.inventory.iter().position(|i| i == item) {
                        s.inventory.remove(idx);
                    }
                }
            }
            s.quests.remove(q.id);
            s.quests_done.push(q.id.to_string());
            s.gold += q.gold;
            let mut text = format!("{npc}: It is done! (+{} xp, +{}g", q.xp, q.gold);
            if let Some(item) = q.item {
                if s.inventory.len() < 40 {
                    s.inventory.push(item.to_string());
                    text.push_str(&format!(", {item}"));
                }
            }
            text.push(')');
            award_xp(e, q.xp);
            return Ok(text);
        }

        // 2) Accept: first not-yet-started quest whose prerequisite is done.
        let accept = quests_for(act, &npc).find(|q| {
            !s.quests.contains_key(q.id)
                && !s.quests_done.iter().any(|d| d == q.id)
                && s.level >= q.min_level
                && q.requires.map_or(true, |r| s.quests_done.iter().any(|d| d == r))
        });
        if let Some(q) = accept {
            if s.quests.len() >= QUEST_CAP {
                return Ok(format!("{npc}: Your hands are full. Finish what you carry first."));
            }
            s.quests.insert(q.id.to_string(), 0);
            return Ok(format!("{npc}: {} (0/{})", q.offer, q.objective.count()));
        }

        // 3) Progress report on one of this giver's quests in flight.
        if let Some(q) = quests_for(act, &npc).find(|q| s.quests.contains_key(q.id)) {
            let prog = s.quests[q.id];
            return Ok(format!("{npc}: Not done yet — {}/{}.", prog, q.objective.count()));
        }

        Ok(format!("{npc}: You have done all I asked. Go with peace."))
    }

    /// Toggle riding the Dire-Wolf (C06). Requires the horn item; refused
    /// while in combat. Any combat dismounts (see damage application).
    pub fn toggle_mount(&mut self, act: Act, id: EntityId) -> Result<String, String> {
        let e = self.entity_mut(act, id).ok_or("no such player")?;
        if e.mounted {
            e.mounted = false;
            return Ok("You dismount.".into());
        }
        let s = e.sheet.as_ref().ok_or("no sheet")?;
        if !s.inventory.iter().any(|i| i == "dire_wolf_horn") {
            return Err("You have no mount. Earn the Horn of the Dire-Wolf.".into());
        }
        if e.combat_timer > 0.0 {
            return Err("You can't mount in combat.".into());
        }
        e.mounted = true;
        Ok("You whistle, and the Dire-Wolf answers. (+60% speed)".into())
    }

    /// Toggle the PvP flag; returns the new state.
    pub fn toggle_pvp(&mut self, act: Act, id: EntityId) -> Option<bool> {
        let s = self.sheet_mut(act, id)?;
        s.pvp = !s.pvp;
        Some(s.pvp)
    }

    /// Pair two players in a duel (both must be live in the same zone).
    pub fn start_duel(&mut self, act: Act, a: EntityId, b: EntityId) -> bool {
        let z = match self.zones.get_mut(&act) {
            Some(z) => z,
            None => return false,
        };
        if !z.entities.contains_key(&a) || !z.entities.contains_key(&b) {
            return false;
        }
        z.entities.get_mut(&a).unwrap().duel_with = Some(b);
        z.entities.get_mut(&b).unwrap().duel_with = Some(a);
        true
    }

    /// Remove an item from a player's inventory (for auction listing). True on success.
    pub fn take_item(&mut self, act: Act, id: EntityId, item: &str) -> bool {
        match self.sheet_mut(act, id) {
            Some(s) => match s.inventory.iter().position(|i| i == item) {
                Some(idx) => {
                    s.inventory.remove(idx);
                    true
                }
                None => false,
            },
            None => false,
        }
    }

    pub fn give_item(&mut self, act: Act, id: EntityId, item: String) -> bool {
        match self.sheet_mut(act, id) {
            Some(s) if s.inventory.len() < 40 => {
                s.inventory.push(item);
                true
            }
            _ => false,
        }
    }

    pub fn set_guild(&mut self, act: Act, id: EntityId, guild: Option<String>) {
        if let Some(s) = self.sheet_mut(act, id) {
            s.guild = guild;
        }
    }

    pub fn add_gold(&mut self, act: Act, id: EntityId, amount: u32) {
        if let Some(s) = self.sheet_mut(act, id) {
            s.gold += amount;
        }
    }

    /// Deduct gold if the player can afford it. True on success.
    pub fn try_spend_gold(&mut self, act: Act, id: EntityId, amount: u32) -> bool {
        match self.sheet_mut(act, id) {
            Some(s) if s.gold >= amount => {
                s.gold -= amount;
                true
            }
            _ => false,
        }
    }

    /// Is the player inside the inn / rest area (required for auction house)?
    pub fn at_inn(&self, act: Act, id: EntityId) -> bool {
        match (self.zones.get(&act), self.player_pos(act, id)) {
            (Some(z), Some(pos)) => pos.distance(z.entry) <= INN_RADIUS,
            _ => false,
        }
    }

    /// Read back a player's current sheet (synced position + stats) for saving.
    pub fn player_sheet(&self, act: Act, id: EntityId) -> Option<CharacterSheet> {
        let e = self.zones.get(&act)?.entities.get(&id)?;
        let mut s = e.sheet.clone()?;
        s.x = e.pos.x;
        s.y = e.pos.y;
        s.act = act;
        s.health = e.health;
        Some(s)
    }

    pub fn zone_snapshot(&self, act: Act) -> (u64, Vec<EntityState>) {
        let z = &self.zones[&act];
        (z.tick, z.entities.values().map(|e| e.to_state()).collect())
    }

    /// A player's current position (for area-of-interest culling).
    pub fn player_pos(&self, act: Act, id: EntityId) -> Option<Vec2> {
        self.zones.get(&act)?.entities.get(&id).map(|e| e.pos)
    }

    /// Area-of-interest snapshot: only entities within `radius` of `center`.
    /// This is the bandwidth control for the MMO — a client never receives the
    /// whole zone, only what's near it. The player's own entity is always at the
    /// center, so it is always included.
    pub fn zone_snapshot_around(
        &self,
        act: Act,
        center: Vec2,
        radius: f32,
    ) -> (u64, Vec<EntityState>) {
        let z = &self.zones[&act];
        let r2 = radius * radius;
        let ents = z
            .entities
            .values()
            .filter(|e| e.pos.distance_squared(center) <= r2)
            .map(|e| e.to_state())
            .collect();
        (z.tick, ents)
    }
}

/// Bump and return the next entity id given a direct borrow of the counter
/// (used inside `step_zone` where `self` is partially borrowed).
fn self_next_id(counter: &mut EntityId) -> EntityId {
    let id = *counter;
    *counter += 1;
    id
}

/// Per-act enemy spawn table: (tag, count). The first entry is the act's
/// signature mob (its alpha variant is the zone boss); the rest are quest
/// targets from docs/quests (C02). Object tags spawn as stationary
/// destructibles via `OBJECT_TAGS`.
pub fn act_spawn_table(act: Act) -> &'static [(&'static str, usize)] {
    match act {
        Act::Eden => &[("serpent", 6), ("cainite", 8), ("elemental", 5), ("ember_wisp", 5)],
        Act::Hermon => &[("watcher", 10), ("cultist", 6), ("chasm_fiend", 8), ("caravan_wagon", 3), ("oathstone", 1), ("stargazer", 1)],
        Act::Nephilim => &[("giant", 14), ("blood_drinker", 8), ("weapon_cache", 4)],
        Act::Enoch => &[("shade", 12), ("citadel_guard", 6), ("furnace_regulator", 4), ("enchanter_smith", 6), ("sorcerer", 8), ("dire_wolf", 16), ("swift_claw", 1)],
        Act::Flood => &[("leviathan", 18), ("geyser", 5), ("nephilim_raider", 9), ("drowned_beast", 10), ("scroll_crate", 5)],
    }
}

/// Stationary destructible "enemies" (quest objects): they never move,
/// aggro, or strike back.
const OBJECT_TAGS: &[&str] = &[
    "oathstone", "caravan_wagon", "weapon_cache", "furnace_regulator", "geyser", "scroll_crate",
];

fn make_enemy(id: EntityId, pos: Vec2, tag: &str, act: Act) -> Entity {
    let tier = Act::ALL.iter().position(|a| *a == act).unwrap_or(0) as i32;
    // Bestiary species: stats scale with the entry's level, and the display
    // name comes from the bestiary for nameplates.
    if let Some(def) = crate::mobs::mob_by_tag(tag) {
        let lvl = def.mid_level() as i32;
        let mut e = make_enemy(id, pos, "__bestiary__", act);
        e.tag = Some(tag.to_string());
        e.name = Some(def.name.clone());
        e.max_health = 30 + lvl * 8;
        e.health = e.max_health;
        e.damage = 4 + lvl;
        e.xp_value = (10 + lvl * 3) as u32;
        return e;
    }
    if tag == "swift_claw" {
        // The mount questline's young Alpha: a level-40 elite (C06).
        let mut e = make_enemy(id, pos, "dire_wolf", act);
        e.tag = Some(tag.to_string());
        e.name = Some("Swift-Claw".into());
        e.max_health = 900;
        e.health = 900;
        e.damage = 40;
        e.xp_value = 800;
        e.aggro_range = 300.0;
        return e;
    }
    let is_object = OBJECT_TAGS.contains(&tag);
    if is_object {
        let mut e = make_enemy(id, pos, "__object__", act);
        e.tag = Some(tag.to_string());
        e.speed = 0.0;
        e.damage = 0;
        e.aggro_range = 0.0;
        e.patrol_radius = 0.0;
        e.state = AiState::Static;
        e.max_health = 30 + tier * 10;
        e.health = e.max_health;
        e.xp_value = (8 + tier * 4) as u32;
        return e;
    }
    let max_health = 40 + tier * 20;
    Entity {
        id,
        kind: EntityKind::Enemy,
        pos,
        rot: 0.0,
        health: max_health,
        max_health,
        tag: Some(tag.to_string()),
        name: None,
        speed: ENEMY_SPEED,
        origin: pos,
        aggro_range: 380.0,
        patrol_radius: 260.0,
        state: AiState::Patrol,
        state_timer: 0.0,
        attack_cooldown: 0.0,
        damage: 6 + tier * 3,
        xp_value: (15 + tier * 10) as u32,
        wander_target: pos,
        intent: Vec2::ZERO,
        attack_queued: false,
        attack_timer: 0.0,
        dead_timer: 0.0,
        cast_queued: None,
        gcd: 0.0,
        cooldowns: HashMap::new(),
        duel_with: None,
        rest_accum: 0.0,
        mounted: false,
        combat_timer: 0.0,
        owner: None,
        sheet: None,
    }
}

fn make_npc(id: EntityId, pos: Vec2, name: &str) -> Entity {
    Entity {
        id,
        kind: EntityKind::Npc,
        pos,
        rot: 0.0,
        health: 1000,
        max_health: 1000,
        tag: Some("questgiver".to_string()),
        name: Some(name.to_string()),
        speed: 0.0,
        origin: pos,
        aggro_range: 0.0,
        patrol_radius: 0.0,
        state: AiState::Static,
        state_timer: 0.0,
        attack_cooldown: 0.0,
        damage: 0,
        xp_value: 0,
        wander_target: pos,
        intent: Vec2::ZERO,
        attack_queued: false,
        attack_timer: 0.0,
        dead_timer: 0.0,
        cast_queued: None,
        gcd: 0.0,
        cooldowns: HashMap::new(),
        duel_with: None,
        rest_accum: 0.0,
        mounted: false,
        combat_timer: 0.0,
        owner: None,
        sheet: None,
    }
}

fn make_wildlife(id: EntityId, pos: Vec2, tag: &str) -> Entity {
    Entity {
        id,
        kind: EntityKind::Wildlife,
        pos,
        rot: 0.0,
        health: 20,
        max_health: 20,
        tag: Some(tag.to_string()),
        name: None,
        speed: WILDLIFE_SPEED,
        origin: pos,
        aggro_range: 240.0, // used as flee range
        patrol_radius: 200.0,
        state: AiState::Graze,
        state_timer: 0.0,
        attack_cooldown: 0.0,
        damage: 0,
        xp_value: 5,
        wander_target: pos,
        intent: Vec2::ZERO,
        attack_queued: false,
        attack_timer: 0.0,
        dead_timer: 0.0,
        cast_queued: None,
        gcd: 0.0,
        cooldowns: HashMap::new(),
        duel_with: None,
        rest_accum: 0.0,
        mounted: false,
        combat_timer: 0.0,
        owner: None,
        sheet: None,
    }
}

fn make_resource(id: EntityId, pos: Vec2, tag: &str) -> Entity {
    Entity {
        id,
        kind: EntityKind::Resource,
        pos,
        rot: 0.0,
        health: 1,
        max_health: 1,
        tag: Some(tag.to_string()),
        name: None,
        speed: 0.0,
        origin: pos,
        aggro_range: 0.0,
        patrol_radius: 0.0,
        state: AiState::Static,
        state_timer: 0.0,
        attack_cooldown: 0.0,
        damage: 0,
        xp_value: 0,
        wander_target: pos,
        intent: Vec2::ZERO,
        attack_queued: false,
        attack_timer: 0.0,
        dead_timer: 0.0,
        cast_queued: None,
        gcd: 0.0,
        cooldowns: HashMap::new(),
        duel_with: None,
        rest_accum: 0.0,
        mounted: false,
        combat_timer: 0.0,
        owner: None,
        sheet: None,
    }
}

fn nearest_of(players: &[(EntityId, Vec2)], from: Vec2) -> Option<(EntityId, Vec2, f32)> {
    players
        .iter()
        .map(|(id, p)| (*id, *p, from.distance(*p)))
        .min_by(|a, b| a.2.partial_cmp(&b.2).unwrap_or(std::cmp::Ordering::Equal))
}

/// Grant xp, applying level-ups. Rested XP (banked at inns) doubles kill XP
/// until the bank runs out, WoW-style. Returns true if the player leveled.
fn award_xp(p: &mut Entity, xp: u32) -> bool {
    let Some(s) = p.sheet.as_mut() else { return false };
    let rested_bonus = xp.min(s.rested_xp);
    s.rested_xp -= rested_bonus;
    s.xp += xp + rested_bonus;
    let mut leveled = false;
    while s.xp >= s.max_xp {
        s.xp -= s.max_xp;
        s.level += 1;
        s.max_xp = (s.max_xp as f32 * 1.35) as u32;
        s.max_health += 15;
        s.max_mana += 8;
        s.health = s.max_health;
        s.mana = s.max_mana;
        s.talent_points += 1;
        leveled = true;
    }
    if leveled {
        p.max_health = s.max_health;
        p.health = s.health;
    }
    leveled
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A player who swings with a resource node in front of them harvests it and
    /// gains a material (wood/stone), and the node is replaced.
    #[test]
    fn attacking_a_resource_harvests_a_material() {
        let mut w = World::new(42);
        // Find a resource node in Eden and its tag.
        let (res_id, res_pos, tag) = {
            let z = &w.zones[&Act::Eden];
            let e = z
                .entities
                .values()
                .find(|e| e.kind == EntityKind::Resource)
                .expect("eden has resource nodes");
            (e.id, e.pos, e.tag.clone().unwrap())
        };
        let expected = if tag == "rock" { "stone" } else { "wood" };

        // Spawn a player just to the -x side so the node is directly in front
        // (player faces +x by default).
        let mut sheet = new_character("Harvester");
        sheet.x = res_pos.x - 12.0;
        sheet.y = res_pos.y;
        let pid = w.spawn_player(7, sheet);

        w.queue_attack(Act::Eden, pid);
        w.step();

        let after = w.player_sheet(Act::Eden, pid).unwrap();
        assert!(
            after.inventory.iter().any(|i| i == expected),
            "expected a '{expected}' in inventory, got {:?}",
            after.inventory
        );
        // The original node id is gone (harvested + replaced).
        assert!(
            !w.zones[&Act::Eden].entities.contains_key(&res_id),
            "harvested node should be removed"
        );
    }

    /// Killing an enemy grants xp and a trophy.
    #[test]
    fn killing_an_enemy_grants_xp_and_trophy() {
        let mut w = World::new(7);
        let (eid, epos, etag) = {
            let z = &w.zones[&Act::Eden];
            let e = z
                .entities
                .values()
                .find(|e| e.kind == EntityKind::Enemy && e.tag.as_deref() == Some("serpent"))
                .unwrap();
            (e.id, e.pos, e.tag.clone().unwrap())
        };
        // Make the enemy killable in one hit by placing the player on top-ish.
        let mut sheet = new_character("Slayer");
        sheet.x = epos.x - 12.0;
        sheet.y = epos.y;
        let pid = w.spawn_player(9, sheet);
        // Enough swings to drop a 40 HP serpent at 12 dmg (respawns don't matter;
        // the *first* killed enemy is the original `eid`).
        let mut killed_original = false;
        for _ in 0..300 {
            w.queue_attack(Act::Eden, pid);
            w.set_intent(Act::Eden, pid, Vec2::ZERO);
            w.step();
            if !w.zones[&Act::Eden].entities.contains_key(&eid) {
                killed_original = true;
                break;
            }
        }
        assert!(killed_original, "should have killed the original enemy");
        let after = w.player_sheet(Act::Eden, pid).unwrap();
        assert!(after.xp > 0 || after.level > 1, "should have gained xp");
        assert!(
            after.inventory.iter().any(|i| i == &format!("{etag}_trophy")),
            "expected a {etag}_trophy, got {:?}",
            after.inventory
        );
    }

    /// Spawn a player at (x, y) in Eden with a chosen class, positioned so a
    /// test can drive it directly.
    fn spawn_at(w: &mut World, owner: u64, name: &str, x: f32, y: f32) -> EntityId {
        let mut sheet = new_character(name);
        sheet.x = x;
        sheet.y = y;
        w.spawn_player(owner, sheet)
    }

    #[test]
    fn class_select_and_cast_damages_target_and_costs_mana() {
        let mut w = World::new(3);
        let epos = {
            let e = w.zones[&Act::Eden]
                .entities
                .values()
                .find(|e| e.kind == EntityKind::Enemy)
                .unwrap();
            e.pos
        };
        let pid = spawn_at(&mut w, 1, "Magus", epos.x - 100.0, epos.y);
        // Clusters (C05): the cast hits the *nearest* enemy — resolve it.
        let me = w.zones[&Act::Eden].entities[&pid].pos;
        let eid = w.zones[&Act::Eden]
            .entities
            .values()
            .filter(|e| e.kind == EntityKind::Enemy)
            .min_by(|a, b| a.pos.distance(me).total_cmp(&b.pos.distance(me)))
            .unwrap()
            .id;
        w.select_class(Act::Eden, pid, Class::Mage).unwrap();
        // Second class pick is rejected.
        assert!(w.select_class(Act::Eden, pid, Class::Warrior).is_err());

        let before_hp = w.zones[&Act::Eden].entities[&eid].health;
        let mana_before = w.player_sheet(Act::Eden, pid).unwrap().mana;
        w.queue_cast(Act::Eden, pid, "firebolt".into());
        w.step();
        let after_hp = w.zones[&Act::Eden].entities[&eid].health;
        let s = w.player_sheet(Act::Eden, pid).unwrap();
        assert!(after_hp < before_hp, "firebolt should damage the enemy");
        assert!(s.mana < mana_before, "cast should cost mana");

        // Immediately recasting fails: GCD + cooldown.
        w.queue_cast(Act::Eden, pid, "firebolt".into());
        w.step();
        let hp2 = w.zones[&Act::Eden].entities[&eid].health;
        assert_eq!(hp2, after_hp, "cooldown should block the second cast");
    }

    #[test]
    fn wrong_class_ability_is_rejected() {
        let mut w = World::new(4);
        let pid = spawn_at(&mut w, 1, "Tank", 0.0, 0.0);
        w.select_class(Act::Eden, pid, Class::Warrior).unwrap();
        let mana_before = w.player_sheet(Act::Eden, pid).unwrap().mana;
        w.queue_cast(Act::Eden, pid, "firebolt".into());
        w.step();
        assert_eq!(
            w.player_sheet(Act::Eden, pid).unwrap().mana,
            mana_before,
            "cross-class cast must not consume mana"
        );
    }

    #[test]
    fn talents_require_points_and_boost_stats() {
        let mut w = World::new(5);
        let pid = spawn_at(&mut w, 1, "Vet", 0.0, 0.0);
        w.select_class(Act::Eden, pid, Class::Warrior).unwrap();
        // No points yet.
        assert!(w.learn_talent(Act::Eden, pid, "warrior_toughness").is_err());
        // Grant a point by hand (level-ups grant 1 each; tested via award path elsewhere).
        {
            let z = w.zones.get_mut(&Act::Eden).unwrap();
            z.entities.get_mut(&pid).unwrap().sheet.as_mut().unwrap().talent_points = 1;
        }
        let hp_before = w.player_sheet(Act::Eden, pid).unwrap().max_health;
        w.learn_talent(Act::Eden, pid, "warrior_toughness").unwrap();
        let s = w.player_sheet(Act::Eden, pid).unwrap();
        assert_eq!(s.max_health, hp_before + 12);
        assert_eq!(s.talent_points, 0);
        assert!(w.learn_talent(Act::Eden, pid, "mage_power").is_err(), "wrong class talent");
    }

    #[test]
    fn unflagged_players_cannot_hurt_each_other_but_duelists_can() {
        let mut w = World::new(6);
        // Far corner: no enemies wander at exactly the entry in tick 1; place
        // the pair away from everything.
        let a = spawn_at(&mut w, 1, "Cain", 500.0, 500.0);
        let b = spawn_at(&mut w, 2, "Abel", 540.0, 500.0);
        // Not flagged, not dueling: melee does nothing to the other player.
        w.queue_attack(Act::Eden, a);
        w.step();
        let b_hp = w.zones[&Act::Eden].entities[&b].health;
        assert_eq!(b_hp, 100, "unflagged players must be immune to PvP damage");
        // Duel on: damage lands, and the loser ends at 1 HP, never dead.
        assert!(w.start_duel(Act::Eden, a, b));
        let mut b_hp_last = b_hp;
        for _ in 0..600 {
            w.queue_attack(Act::Eden, a);
            w.step();
            b_hp_last = w.zones[&Act::Eden].entities[&b].health;
            if w.zones[&Act::Eden].entities[&b].duel_with.is_none() {
                break;
            }
        }
        assert_eq!(b_hp_last, 1, "duel loser should survive at 1 HP");
        assert!(w.zones[&Act::Eden].entities[&a].duel_with.is_none(), "duel should end");
    }

    #[test]
    fn crafting_consumes_materials_and_checks_skill() {
        let mut w = World::new(8);
        let pid = spawn_at(&mut w, 1, "Smith", 0.0, 0.0);
        // No materials.
        assert!(w.craft(Act::Eden, pid, "bread").is_err());
        {
            let z = w.zones.get_mut(&Act::Eden).unwrap();
            let s = z.entities.get_mut(&pid).unwrap().sheet.as_mut().unwrap();
            s.inventory = vec!["wood".into(), "wood".into(), "stone".into(), "stone".into(), "wood".into()];
        }
        // stone_axe needs mining 5.
        assert!(w.craft(Act::Eden, pid, "stone_axe").is_err());
        w.craft(Act::Eden, pid, "bread").unwrap();
        let s = w.player_sheet(Act::Eden, pid).unwrap();
        assert!(s.inventory.iter().any(|i| i == "bread"));
        assert_eq!(s.inventory.iter().filter(|i| i.as_str() == "wood").count(), 1);
        // Level mining and craft the axe.
        {
            let z = w.zones.get_mut(&Act::Eden).unwrap();
            let s = z.entities.get_mut(&pid).unwrap().sheet.as_mut().unwrap();
            s.professions.insert("mining".into(), 5);
        }
        w.craft(Act::Eden, pid, "stone_axe").unwrap();
        assert!(w.player_sheet(Act::Eden, pid).unwrap().inventory.iter().any(|i| i == "stone_axe"));
    }

    #[test]
    fn quest_accept_progress_and_turn_in() {
        let mut w = World::new(11);
        // Player at the inn: Elder stands at entry + (90, 0).
        let pid = spawn_at(&mut w, 1, "Pilgrim", 60.0, 0.0);
        // Accept.
        let offer = w.talk(Act::Eden, pid).unwrap();
        assert!(offer.contains("Serpents"), "{offer}");
        // Not done yet.
        assert!(w.talk(Act::Eden, pid).unwrap().contains("0/5"));
        // Simulate 5 serpent kills via the sheet hook (combat path covered by
        // other tests): count progress the way the sim does.
        {
            let z = w.zones.get_mut(&Act::Eden).unwrap();
            let s = z.entities.get_mut(&pid).unwrap().sheet.as_mut().unwrap();
            s.quests.insert("serpents_in_the_garden".into(), 5);
        }
        let done = w.talk(Act::Eden, pid).unwrap();
        assert!(done.contains("It is done"), "{done}");
        let s = w.player_sheet(Act::Eden, pid).unwrap();
        assert!(s.gold >= 15, "quest gold, got {}", s.gold);
        assert!(s.xp > 0 || s.level > 1, "quest xp");
        assert!(s.inventory.iter().any(|i| i == "bronze_sword"), "quest reward item");
        assert!(s.quests_done.iter().any(|q| q == "serpents_in_the_garden"));
        // Repeat talk: quest is done.
        assert!(w.talk(Act::Eden, pid).unwrap().contains("peace"));
    }

    #[test]
    fn kill_advances_quest_progress() {
        let mut w = World::new(7);
        let (eid, epos) = {
            let e = w.zones[&Act::Eden]
                .entities
                .values()
                .find(|e| e.kind == EntityKind::Enemy && e.tag.as_deref() == Some("serpent"))
                .unwrap();
            (e.id, e.pos)
        };
        let mut sheet = new_character("Hunter");
        sheet.x = epos.x - 12.0;
        sheet.y = epos.y;
        sheet.max_health = 100_000;
        sheet.health = 100_000;
        sheet.quests.insert("serpents_in_the_garden".into(), 0);
        let pid = w.spawn_player(9, sheet);
        for _ in 0..300 {
            w.queue_attack(Act::Eden, pid);
            w.step();
            if !w.zones[&Act::Eden].entities.contains_key(&eid) {
                break;
            }
        }
        let s = w.player_sheet(Act::Eden, pid).unwrap();
        // The swing cleaves the whole camp cluster, so 1+ kills may credit.
        assert!(s.quests.get("serpents_in_the_garden").copied().unwrap_or(0) >= 1);
    }

    /// Kill one Eden mob of the given tag with quest state pre-seeded.
    fn kill_one_tagged(w: &mut World, tag: &str, seed_quests: &[(&str, u32)]) -> CharacterSheet {
        let (eid, epos) = {
            let e = w.zones[&Act::Eden]
                .entities
                .values()
                .find(|e| e.kind == EntityKind::Enemy && e.tag.as_deref() == Some(tag))
                .unwrap();
            (e.id, e.pos)
        };
        let mut sheet = new_character("Hunter");
        sheet.x = epos.x - 12.0;
        sheet.y = epos.y;
        sheet.max_health = 100_000; // isolation: nearby spawns must not gank the test
        sheet.health = 100_000;
        for (q, p) in seed_quests {
            sheet.quests.insert((*q).into(), *p);
        }
        let pid = w.spawn_player(9, sheet);
        for _ in 0..300 {
            w.queue_attack(Act::Eden, pid);
            w.step();
            if !w.zones[&Act::Eden].entities.contains_key(&eid) {
                break;
            }
        }
        w.player_sheet(Act::Eden, pid).unwrap().clone()
    }

    #[test]
    fn collect_quest_loots_and_consumes_on_turn_in() {
        let mut w = World::new(21);
        // Source kill drops the quest item while the quest is active.
        let s = kill_one_tagged(&mut w, "cainite", &[("the_first_forges", 0)]);
        assert!(s.inventory.iter().any(|i| i == "bronze_ingot"), "quest item looted");
        assert_eq!(s.quests.get("the_first_forges").copied(), Some(1));

        // Turn-in at the Seer consumes the six ingots and pays out.
        let mut w = World::new(22);
        let pid = spawn_at(&mut w, 2, "Collector", 260.0, -170.0);
        {
            let z = w.zones.get_mut(&Act::Eden).unwrap();
            let s = z.entities.get_mut(&pid).unwrap().sheet.as_mut().unwrap();
            s.quests.insert("the_first_forges".into(), 6);
            s.inventory = vec!["bronze_ingot".into(); 6];
        }
        let done = w.talk(Act::Eden, pid).unwrap();
        assert!(done.contains("It is done"), "{done}");
        let s = w.player_sheet(Act::Eden, pid).unwrap();
        assert!(!s.inventory.iter().any(|i| i == "bronze_ingot"), "ingots consumed");
        assert!(s.inventory.iter().any(|i| i == "bronze_bracers"), "reward granted");
    }

    #[test]
    fn chained_quest_needs_prerequisite() {
        let mut w = World::new(23);
        // Stand by the Wanderer with its unchained quest already done.
        let pid = spawn_at(&mut w, 3, "Seeker", -70.0, 110.0);
        {
            let z = w.zones.get_mut(&Act::Eden).unwrap();
            let s = z.entities.get_mut(&pid).unwrap().sheet.as_mut().unwrap();
            // Finish the Wanderer's unchained quest so only the chain remains.
            s.quests_done.push("fruit_of_the_thorns".into());
        }
        // Prerequisite (elder main quest) not done: nothing to offer.
        let t = w.talk(Act::Eden, pid).unwrap();
        assert!(t.contains("peace"), "chained quest withheld, got: {t}");
        {
            let z = w.zones.get_mut(&Act::Eden).unwrap();
            let s = z.entities.get_mut(&pid).unwrap().sheet.as_mut().unwrap();
            s.quests_done.push("serpents_in_the_garden".into());
        }
        let t = w.talk(Act::Eden, pid).unwrap();
        assert!(t.contains("altar") || t.contains("Scavengers"), "chained quest now offered, got: {t}");
    }

    #[test]
    fn two_quests_progress_from_one_kill() {
        let mut w = World::new(24);
        // One cainite kill advances a Kill quest and a Collect quest together.
        let s = kill_one_tagged(&mut w, "cainite", &[("altar_of_the_firstborn", 0), ("the_first_forges", 0)]);
        // Cleave may take out 1+ cluster-mates; both quests must credit.
        assert!(s.quests.get("altar_of_the_firstborn").copied().unwrap_or(0) >= 1);
        assert!(s.quests.get("the_first_forges").copied().unwrap_or(0) >= 1);
        assert!(s.inventory.iter().any(|i| i == "bronze_ingot"));
    }

    #[test]
    fn bestiary_mobs_spawn_and_drop_their_loot() {
        let mut w = World::new(31);
        // Zones contain bestiary-tagged entities (hostile and/or docile).
        let z = &w.zones[&Act::Eden];
        let bestiary: Vec<_> = z
            .entities
            .values()
            .filter(|e| e.tag.as_deref().and_then(crate::mobs::mob_by_tag).is_some())
            .collect();
        assert!(bestiary.len() >= 6, "expected bestiary spawns, got {}", bestiary.len());
        assert!(bestiary.iter().all(|e| e.name.is_some()), "bestiary mobs carry names");

        // Kill a hostile bestiary mob: the loot comes from its drop list.
        let (eid, epos, tag) = {
            let e = w.zones[&Act::Eden]
                .entities
                .values()
                .find(|e| e.kind == EntityKind::Enemy
                    && e.tag.as_deref().and_then(crate::mobs::mob_by_tag).is_some())
                .expect("a hostile bestiary mob spawned in Eden");
            (e.id, e.pos, e.tag.clone().unwrap())
        };
        let def = crate::mobs::mob_by_tag(&tag).unwrap();
        let mut sheet = new_character("Tracker");
        sheet.x = epos.x - 12.0;
        sheet.y = epos.y;
        sheet.max_health = 100_000;
        sheet.health = 100_000;
        let pid = w.spawn_player(9, sheet);
        for _ in 0..2000 {
            w.queue_attack(Act::Eden, pid);
            w.step();
            if !w.zones[&Act::Eden].entities.contains_key(&eid) {
                break;
            }
        }
        assert!(
            !w.zones[&Act::Eden].entities.contains_key(&eid),
            "mob should die within the step budget"
        );
        let s = w.player_sheet(Act::Eden, pid).unwrap();
        assert!(
            s.inventory.iter().any(|i| def.drops.contains(i)),
            "inventory {:?} should contain one of {:?}", s.inventory, def.drops
        );
    }

    #[test]
    fn poi_discovery_grants_xp_once_and_persists() {
        let mut w = World::new(41);
        let poi = crate::pois::pois_for_act(Act::Eden).next().unwrap();
        let pid = spawn_at(&mut w, 5, "Explorer", poi.x, poi.y);
        w.step();
        let s = w.player_sheet(Act::Eden, pid).unwrap();
        assert!(s.discovered.iter().any(|d| d == &poi.name), "POI discovered");
        let xp_after = s.xp;
        assert!(xp_after >= 50, "discovery XP granted, got {xp_after}");
        // Standing there longer must not re-grant.
        for _ in 0..40 {
            w.step();
        }
        let s = w.player_sheet(Act::Eden, pid).unwrap();
        assert_eq!(s.discovered.iter().filter(|d| *d == &poi.name).count(), 1);
        assert_eq!(s.xp, xp_after, "no repeat XP");

        // Persists across a save/load round-trip.
        let db = crate::db::Db::open(":memory:").unwrap();
        db.save(&s, Some("apple_poi")).unwrap();
        let loaded = db.load(&s.name).unwrap().unwrap();
        assert!(loaded.discovered.iter().any(|d| d == &poi.name), "survives persistence");
    }

    #[test]
    fn mount_rules_horn_combat_speed_dismount() {
        let mut w = World::new(51);
        let pid = spawn_at(&mut w, 6, "Rider", 0.0, 0.0);
        // No horn: refused.
        assert!(w.toggle_mount(Act::Eden, pid).is_err());
        {
            let z = w.zones.get_mut(&Act::Eden).unwrap();
            let e = z.entities.get_mut(&pid).unwrap();
            e.sheet.as_mut().unwrap().inventory.push("dire_wolf_horn".into());
        }
        // In combat: refused.
        {
            let z = w.zones.get_mut(&Act::Eden).unwrap();
            z.entities.get_mut(&pid).unwrap().combat_timer = 3.0;
        }
        assert!(w.toggle_mount(Act::Eden, pid).is_err(), "no mounting in combat");
        {
            let z = w.zones.get_mut(&Act::Eden).unwrap();
            z.entities.get_mut(&pid).unwrap().combat_timer = 0.0;
        }
        w.toggle_mount(Act::Eden, pid).unwrap();
        assert!(w.zones[&Act::Eden].entities[&pid].mounted);
        assert!(w.zones[&Act::Eden].entities[&pid].to_state().mounted, "mounted rides the wire");

        // Mounted movement is 1.6x on-foot movement.
        let start = w.zones[&Act::Eden].entities[&pid].pos;
        w.set_intent(Act::Eden, pid, Vec2::new(1.0, 0.0));
        w.step();
        let mounted_dx = w.zones[&Act::Eden].entities[&pid].pos.x - start.x;
        {
            let z = w.zones.get_mut(&Act::Eden).unwrap();
            z.entities.get_mut(&pid).unwrap().mounted = false;
        }
        let start = w.zones[&Act::Eden].entities[&pid].pos;
        w.step();
        let foot_dx = w.zones[&Act::Eden].entities[&pid].pos.x - start.x;
        assert!(
            (mounted_dx / foot_dx - 1.6).abs() < 0.05,
            "expected 1.6x, got {}x", mounted_dx / foot_dx
        );

        // Taking damage dismounts.
        {
            let z = w.zones.get_mut(&Act::Eden).unwrap();
            z.entities.get_mut(&pid).unwrap().mounted = true;
            z.entities.get_mut(&pid).unwrap().intent = Vec2::ZERO;
        }
        let (eid, epos) = {
            let e = w.zones[&Act::Eden]
                .entities
                .values()
                .find(|e| e.kind == EntityKind::Enemy && e.tag.as_deref() == Some("serpent"))
                .unwrap();
            (e.id, e.pos)
        };
        {
            // Teleport next to a serpent and let it strike.
            let z = w.zones.get_mut(&Act::Eden).unwrap();
            let p = z.entities.get_mut(&pid).unwrap();
            p.pos = epos + Vec2::new(10.0, 0.0);
        }
        for _ in 0..200 {
            w.step();
            if !w.zones[&Act::Eden].entities[&pid].mounted {
                break;
            }
        }
        assert!(!w.zones[&Act::Eden].entities[&pid].mounted, "damage should dismount");
        let _ = eid;
    }

    #[test]
    fn mount_quests_gated_at_level_40() {
        let mut w = World::new(52);
        // Jabal stands at Enoch entry + (0,-220).
        let mut sheet = new_character("Tamer");
        sheet.act = Act::Enoch;
        sheet.x = 0.0;
        sheet.y = -220.0;
        let pid = w.spawn_player(8, sheet);
        // Level 1: Jabal has nothing for us.
        let t = w.talk(Act::Enoch, pid).unwrap();
        assert!(t.contains("peace"), "level gate should hide the chain, got: {t}");
        {
            let z = w.zones.get_mut(&Act::Enoch).unwrap();
            z.entities.get_mut(&pid).unwrap().sheet.as_mut().unwrap().level = 40;
        }
        let t = w.talk(Act::Enoch, pid).unwrap();
        assert!(t.contains("dire-wolves"), "chain offered at 40, got: {t}");
    }

    #[test]
    fn equipping_gear_boosts_damage_and_swaps() {
        let mut w = World::new(12);
        let pid = spawn_at(&mut w, 1, "Squire", 0.0, 0.0);
        {
            let z = w.zones.get_mut(&Act::Eden).unwrap();
            let s = z.entities.get_mut(&pid).unwrap().sheet.as_mut().unwrap();
            s.inventory = vec!["bronze_sword".into(), "stone_axe".into(), "hide_vest".into()];
        }
        assert!(w.equip(Act::Eden, pid, "serpent_trophy").is_err(), "non-gear rejected");
        w.equip(Act::Eden, pid, "bronze_sword").unwrap();
        let s = w.player_sheet(Act::Eden, pid).unwrap();
        assert_eq!(s.equipment.get("weapon").map(String::as_str), Some("bronze_sword"));
        // Swapping weapons returns the old one to the bags.
        w.equip(Act::Eden, pid, "stone_axe").unwrap();
        let s = w.player_sheet(Act::Eden, pid).unwrap();
        assert_eq!(s.equipment.get("weapon").map(String::as_str), Some("stone_axe"));
        assert!(s.inventory.iter().any(|i| i == "bronze_sword"));
        // Chest piece adds max health.
        let hp_before = s.max_health;
        w.equip(Act::Eden, pid, "hide_vest").unwrap();
        let s = w.player_sheet(Act::Eden, pid).unwrap();
        assert_eq!(s.max_health, hp_before + 30);
    }

    #[test]
    fn every_zone_has_an_elder_and_an_alpha() {
        let w = World::new(13);
        for act in Act::ALL {
            let z = &w.zones[&act];
            assert!(
                z.entities.values().any(|e| e.kind == EntityKind::Npc),
                "{} needs a questgiver",
                act.as_str()
            );
            let boss = z
                .entities
                .values()
                .find(|e| {
                    e.tag.as_deref().map(|t| t.ends_with("_alpha")) == Some(true)
                        && e.tag.as_deref().and_then(crate::mobs::mob_by_tag).is_none()
                })
                .expect("act boss");
            assert!(boss.max_health >= 160, "boss should be elite");
        }
    }

    #[test]
    fn resting_at_inn_banks_rested_xp() {
        let mut w = World::new(9);
        let pid = spawn_at(&mut w, 1, "Sleepy", 0.0, 0.0); // entry = inn
        for _ in 0..(TICK_HZ * 3) {
            w.step();
        }
        let s = w.player_sheet(Act::Eden, pid).unwrap();
        assert!(s.rested_xp > 0, "3s at the inn should bank rested XP, got {}", s.rested_xp);
    }

    #[test]
    fn harvesting_levels_the_profession() {
        let mut w = World::new(42);
        let (res_pos, tag) = {
            let e = w.zones[&Act::Eden]
                .entities
                .values()
                .find(|e| e.kind == EntityKind::Resource)
                .unwrap();
            (e.pos, e.tag.clone().unwrap())
        };
        let prof = if tag == "rock" { "mining" } else { "woodcutting" };
        let pid = spawn_at(&mut w, 1, "Gatherer", res_pos.x - 12.0, res_pos.y);
        w.queue_attack(Act::Eden, pid);
        w.step();
        let s = w.player_sheet(Act::Eden, pid).unwrap();
        // Groves cluster (C05): one swing can fell several nodes.
        assert!(s.professions.get(prof).copied().unwrap_or(0) >= 1);
    }
}

/// A fresh level-1 character at Eden's entry point.
pub fn new_character(name: &str) -> CharacterSheet {
    CharacterSheet {
        name: name.to_string(),
        act: Act::Eden,
        x: 0.0,
        y: 0.0,
        level: 1,
        xp: 0,
        max_xp: 100,
        health: 100,
        max_health: 100,
        mana: 50,
        max_mana: 50,
        inventory: Vec::new(),
        class: None,
        gold: 0,
        talent_points: 0,
        talents: Default::default(),
        professions: Default::default(),
        guild: None,
        rested_xp: 0,
        pvp: false,
        wakefulness: 100.0,
        last_logout: None,
        discovered: Vec::new(),
        quests: std::collections::BTreeMap::new(),
        quests_done: Vec::new(),
        equipment: Default::default(),
    }
}
