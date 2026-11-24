//! Authoritative world model and fixed-tick simulation.
//!
//! The world is the single source of truth. Clients send *intents*
//! (`Move`, `Attack`); the server integrates them here. One `World` owns every
//! zone; each zone owns its entities. The whole sim runs single-threaded on the
//! game loop, so there are no locks in the hot path.

use antediluvia_protocol::{Act, CharacterSheet, Class, EntityId, EntityKind, EntityState, EventKind};
use glam::Vec2;
use std::collections::HashMap;

use crate::aoi::{self, AoiGrid};

pub const TICK_HZ: u64 = 20;
pub const DT: f32 = 1.0 / TICK_HZ as f32;
pub use antediluvia_protocol::WORLD_BOUNDS;

/// No hostile spawns within this distance of the inn (C05).
pub const SAFE_RING: f32 = 400.0;
/// Party shared-credit radius (P1): members must be near the kill.
const PARTY_SHARE_RANGE: f32 = 900.0;

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
/// Regeneration is applied in whole points on a half-second beat (a per-tick
/// fraction rounds to zero at 20 Hz, which is why nothing used to regenerate).
const REGEN_BEAT_SECS: f32 = 0.5;
const HEALTH_REGEN_PER_BEAT: i32 = 1; // 2 hp/s
const MANA_REGEN_PER_BEAT: i32 = 1; // 2 mana/s
/// Stamina returns one point every other tick (10/s).
const STAMINA_REGEN_TICKS: u64 = 2;
/// Global cooldown between ability casts (WoW-style GCD).
const GCD_SECS: f32 = 1.0;
/// Radius around a zone's entry point that counts as the inn (rest area).
pub const INN_RADIUS: f32 = 220.0;
/// Rested XP gained per second while at an inn, and its cap.
const RESTED_PER_SEC: u32 = 20;
const RESTED_CAP: u32 = 2000;
const PROFESSION_CAP: u32 = 300;

// ─── Class abilities ─────────────────────────────────────────────────────────
// The kits themselves are data in `crate::abilities`; this file only
// interprets them (see "Progress or resolve an ability cast" in `step_zone`).

pub use crate::abilities::{ability, Ability, ABILITIES};
use crate::abilities::{class_base, class_resource, Aim, AuraKind, Fx, Resource, School, STAMINA_MAX};

/// A timed effect on an entity (DoT, stun, shield, ...). Never persisted.
#[derive(Debug, Clone)]
pub struct Aura {
    pub kind: AuraKind,
    /// Seconds remaining.
    pub left: f32,
    /// Per-second amount (DoT/HoT), fraction (slow/weaken/...) or absorb pool.
    pub mag: f32,
    /// Who applied it (kill credit for DoTs, taunt target).
    pub src: EntityId,
    /// Source ability id; re-applying the same one refreshes, not stacks.
    pub ability: &'static str,
    /// Accumulator toward the next once-a-second DoT/HoT tick.
    pub tick: f32,
}

/// What an entity's auras add up to this tick.
struct AuraState {
    stunned: bool,
    rooted: bool,
    /// Movement-speed multiplier.
    speed: f32,
    /// Outgoing-damage multiplier.
    out: f32,
    taunt: Option<EntityId>,
}

/// Bosses shrug off hard crowd control (stun/root), as in classic MMOs.
fn cc_immune(e: &Entity) -> bool {
    e.kind == EntityKind::Enemy
        && (e.max_health >= 900
            || e.tag.as_deref().is_some_and(|t| t.ends_with("_alpha") || t.ends_with("_dweller")))
}

fn resource_of(s: &CharacterSheet, r: Resource) -> i32 {
    match r {
        Resource::Mana => s.mana,
        Resource::Stamina => s.stamina,
    }
}

fn add_resource(s: &mut CharacterSheet, r: Resource, n: i32) {
    match r {
        Resource::Mana => s.mana = (s.mana + n).clamp(0, s.max_mana),
        Resource::Stamina => s.stamina = (s.stamina + n).clamp(0, s.max_stamina),
    }
}

/// Can this player begin `ab_id` right now? (Class, level, resource, global
/// and per-ability cooldown. Target checks happen at resolution.)
fn cast_check(e: &Entity, ab_id: &str) -> Result<&'static Ability, &'static str> {
    let ab = ability(ab_id).ok_or("Unknown ability.")?;
    let s = e.sheet.as_ref().ok_or("Unknown ability.")?;
    if s.class != Some(ab.class) {
        return Err("That ability is not of your class.");
    }
    if s.level < ab.min_level {
        return Err("You are not high enough level.");
    }
    let res = class_resource(ab.class);
    if resource_of(s, res) < ab.cost {
        return Err(match res {
            Resource::Mana => "Not enough mana.",
            Resource::Stamina => "Not enough stamina.",
        });
    }
    if e.gcd > 0.0 || e.cooldowns.get(ab.id).copied().unwrap_or(0.0) > 0.0 {
        return Err("Ability not ready.");
    }
    Ok(ab)
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
    Recipe { id: "taming_lasso", needs: None,                  inputs: &[("thick_hide", 1), ("wood", 2)], output: "taming_lasso" },
    // Cave-ore upgrades (C09): each new ore has one craftable step up.
    Recipe { id: "orichalcum_blade", needs: Some(("mining", 10)), inputs: &[("orichalcum_ore", 3), ("wood", 1)], output: "orichalcum_blade" },
    Recipe { id: "luminous_charm",   needs: Some(("mining", 5)),  inputs: &[("luminous_quartz", 2)],             output: "luminous_charm" },
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
    // Forbidden Arts pillar finale (C08): the warlord's star-metal blade.
    ItemDef { id: "warlords_star_blade",   slot: "weapon", melee: 18, spell: 8,  hp: 0 },
    // Cave-ore crafts (C09).
    ItemDef { id: "orichalcum_blade",      slot: "weapon", melee: 15, spell: 0,  hp: 0 },
    ItemDef { id: "luminous_charm",        slot: "neck",   melee: 0,  spell: 8,  hp: 10 },
    // Starting garment (Gen 3:21): skins given at the gate of the garden.
    ItemDef { id: "animal_skins",          slot: "chest",  melee: 0,  spell: 0,  hp: 15 },
    // Faction Quartermaster wares (C10).
    ItemDef { id: "lineage_blade",         slot: "weapon", melee: 13, spell: 5,  hp: 0 },
    ItemDef { id: "lineage_mantle",        slot: "back",   melee: 0,  spell: 0,  hp: 40 },
];

/// Key items: carried, never equipped or consumed.
pub const KEY_ITEMS: &[&str] = &["dire_wolf_horn", "gate_token"];

/// Mount performance tiers by species keyword (C07, one place only):
/// swift predators run 1.7x; brutes haul slower but add 8 bag slots;
/// bears hold a rider through 3 hits before dismounting.
pub fn mount_tier(species: &str) -> (f32, usize, u32) {
    const SWIFT: &[&str] = &["wolf", "cat", "smilodon", "panther", "lion", "raptor", "jackal", "hound"];
    const BRUTE: &[&str] = &["ox", "auroch", "bull", "mastodon", "mammoth", "behemoth", "bison"];
    const BEAR: &[&str] = &["bear"];
    if BEAR.iter().any(|k| species.contains(k)) {
        (1.5, 0, 3)
    } else if BRUTE.iter().any(|k| species.contains(k)) {
        (1.35, 8, 0)
    } else if SWIFT.iter().any(|k| species.contains(k)) {
        (1.7, 0, 0)
    } else {
        (1.6, 0, 0) // dire-wolf horn and anything unclassified
    }
}

/// Active mount species from the equipment "mount" slot ("mount:<tag>").
fn active_mount(sheet: &CharacterSheet) -> Option<&str> {
    sheet.equipment.get("mount").and_then(|m| m.strip_prefix("mount:"))
}

/// Bag capacity: 40, +8 while a brute mount is the active mount.
pub fn inv_cap(sheet: &CharacterSheet) -> usize {
    40 + active_mount(sheet).map_or(0, |sp| mount_tier(sp).1)
}

// ─── World-PvP capital zones (C16) ───────────────────────────────────────────

/// A faction capital's territory: entering it as the ENEMY lineage
/// force-flags you for PvP until you leave (plus a cooldown).
#[derive(serde::Deserialize)]
pub struct PvpZone {
    pub act: String,
    pub name: String,
    pub faction: String,
    pub x: f32,
    pub y: f32,
    pub radius: f32,
}

pub fn pvp_zones(act: Act) -> impl Iterator<Item = &'static PvpZone> {
    static ZONES: std::sync::OnceLock<Vec<PvpZone>> = std::sync::OnceLock::new();
    ZONES
        .get_or_init(|| {
            serde_json::from_str(include_str!("../../../assets/data/pvp_zones.json"))
                .expect("pvp_zones.json parses")
        })
        .iter()
        .filter(move |z| z.act == act.as_str())
}

/// Seconds the forced flag lingers after leaving enemy territory.
pub const FORCED_PVP_LINGER: f32 = 20.0;

/// Where new mortals step out of the garden (east end of the Eden road).
pub const GATE_OF_EDEN: Vec2 = Vec2::new(2950.0, 0.0);

// ─── Factions & reputation (C10) ─────────────────────────────────────────────

pub const FACTIONS: [&str; 2] = ["sethite", "cainite"];

/// Reputation rungs, WoW-ish: Neutral / Friendly 3k / Honored 9k / Revered 21k.
pub fn rep_rank(rep: i32) -> &'static str {
    match rep {
        i32::MIN..=2999 => "neutral",
        3000..=8999 => "friendly",
        9000..=20999 => "honored",
        _ => "revered",
    }
}

fn rank_at_least(rep: i32, need: &str) -> bool {
    let order = ["neutral", "friendly", "honored", "revered"];
    let a = order.iter().position(|r| *r == rep_rank(rep)).unwrap_or(0);
    let b = order.iter().position(|r| *r == need).unwrap_or(0);
    a >= b
}

/// Mob tags that grant reputation to the OPPOSING lineage when killed.
/// (rival prefix, faction that earns the rep)
const RIVAL_KILL_REP: &[(&str, &str)] = &[
    ("cainite", "sethite"),
    ("azazel_cultist", "sethite"),
    ("sethite_zealot", "cainite"),
];

/// Quartermaster wares: (item, gold, minimum rank).
pub const QM_WARES: &[(&str, u32, &str)] = &[
    ("thick_hide", 25, "friendly"),
    ("lineage_blade", 150, "honored"),
    ("lineage_mantle", 300, "revered"),
];

/// Rep multiplier for test grinds (ANTEDILUVIA_REP_MULT, default 1).
fn rep_mult() -> i32 {
    std::env::var("ANTEDILUVIA_REP_MULT").ok().and_then(|v| v.parse().ok()).unwrap_or(1)
}

/// Consumable quest rewards handled by `use_item` (not equipment).
pub const CONSUMABLES: &[&str] = &[
    "bread", "fruit", "golden_apple", "energy_drink", "bitter_bread", "healing_potion",
    "hearthstone",
];

pub fn item_def(id: &str) -> Option<&'static ItemDef> {
    ITEMS.iter().find(|i| i.id == id)
}

fn gear_bonus(sheet: &CharacterSheet) -> (i32, i32) {
    let mut melee = 0;
    let mut spell = 0;
    for item in sheet.equipment.values() {
        if crate::durability::is_broken(sheet, item) { continue; } // beta-server-track hook (B06)
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
    /// Cast-time ability in progress and its seconds remaining (players).
    pub casting: Option<(&'static Ability, f32)>,
    /// Timed effects currently on this entity (B03).
    pub auras: Vec<Aura>,
    /// Accumulator toward the next regeneration beat (players).
    pub regen_accum: f32,
    /// (health, mana, stamina) last pushed to the owner, to throttle `Vitals`.
    pub vitals_sent: (i32, i32, i32),
    /// Live duel partner, if dueling.
    pub duel_with: Option<EntityId>,
    /// Fractional-second accumulator for inn rested-XP accrual.
    pub rest_accum: f32,
    /// Riding the Dire-Wolf (players; C06). Cleared by any combat.
    pub mounted: bool,
    /// Seconds until the player counts as out of combat again.
    pub combat_timer: f32,
    /// While positive this beast is enraged (failed tame): +50% damage.
    pub enrage_timer: f32,
    /// Hits a bear-tier mount still absorbs before the rider is dismounted.
    pub mount_hits_left: u32,
    /// Dev-menu godmode (C14): immune to all damage. Never persisted.
    pub godmode: bool,
    /// Force-flagged by an enemy capital zone (C16). Never persisted.
    pub forced_pvp: bool,
    /// Seconds of forced flag remaining after leaving the zone (C16).
    pub forced_pvp_timer: f32,
    /// For a player entity, the owning connection id.
    pub owner: Option<u64>,
    /// For a player, its persistent sheet (kept in sync so we can save it).
    pub sheet: Option<CharacterSheet>,
    /// AoI grid cell this entity is currently filed in (see `crate::aoi`).
    /// The sim keeps it in sync on spawn, despawn, and cell-boundary
    /// crossings; it is never read for gameplay, only for visibility queries.
    pub cell: u32,
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
            // Revered cosmetic (C10): back slot + wearer's lineage drive the tint.
            back: self
                .sheet
                .as_ref()
                .and_then(|s| s.equipment.get("back").cloned()),
            faction: self.sheet.as_ref().and_then(|s| s.faction.clone()),
            appearance: self.sheet.as_ref().map(|s| s.appearance),
            look: self.sheet.as_ref().map(|s| s.look),
            mounted: self.mounted,
            mount_species: if self.mounted {
                self.sheet.as_ref().and_then(|s| {
                    s.equipment
                        .get("mount")
                        .and_then(|m| m.strip_prefix("mount:"))
                        .map(str::to_string)
                })
            } else {
                None
            },
        }
    }
}

pub struct Zone {
    pub act: Act,
    pub entities: HashMap<EntityId, Entity>,
    pub tick: u64,
    /// Where players arriving in this zone spawn.
    pub entry: Vec2,
    /// Spatial AoI grid: every live entity is filed in exactly one cell.
    /// Maintained incrementally — spawns/despawns file through
    /// `insert_entity`/`remove_entity`, and cell-boundary crossings are
    /// refiled by the pass at the end of `step_zone`.
    pub grid: AoiGrid,
}

impl Zone {
    /// Insert an entity, filing it in the AoI grid.
    fn insert_entity(&mut self, id: EntityId, mut e: Entity) {
        e.cell = aoi::cell_of(e.pos.x, e.pos.y) as u32;
        self.grid.insert(id, e.cell as usize);
        self.entities.insert(id, e);
    }

    /// Remove an entity, unfiling it from the AoI grid.
    fn remove_entity(&mut self, id: &EntityId) -> Option<Entity> {
        let e = self.entities.remove(id)?;
        self.grid.remove(*id, e.cell as usize);
        Some(e)
    }

    /// Refile one entity after its position changed outside the sim tick
    /// (dev teleport, test setup). Ordinary sim movement is covered by the
    /// refile pass at the end of `step_zone`.
    pub(crate) fn refile_entity(&mut self, id: EntityId) {
        let (pos, old) = match self.entities.get(&id) {
            Some(e) => (e.pos, e.cell as usize),
            None => return,
        };
        let new = aoi::cell_of(pos.x, pos.y);
        if new != old {
            self.grid.move_entity(id, old, new);
            if let Some(e) = self.entities.get_mut(&id) {
                e.cell = new as u32;
            }
        }
    }

    /// Rebuild the whole grid from current entity positions (zone boot).
    fn rebuild_grid(&mut self) {
        self.grid = AoiGrid::new();
        for (id, e) in self.entities.iter_mut() {
            e.cell = aoi::cell_of(e.pos.x, e.pos.y) as u32;
            self.grid.insert(*id, e.cell as usize);
        }
    }

    /// Debug invariant: every entity filed exactly once, in the cell its
    /// position maps to.
    #[cfg(test)]
    fn check_grid(&self) {
        let mut filed = 0usize;
        for c in 0..aoi::GRID_CELLS {
            filed += self.grid.cell_entities(c).len();
        }
        assert_eq!(filed, self.entities.len(), "grid/entity count mismatch");
        for (id, e) in &self.entities {
            let c = e.cell as usize;
            assert_eq!(aoi::cell_of(e.pos.x, e.pos.y), c, "entity {id} cell stale");
            assert!(
                self.grid.cell_entities(c).contains(id),
                "entity {id} missing from its cell"
            );
        }
    }
}

pub struct World {
    /// C11 economy telemetry (since boot, not persisted).
    pub gold_minted: u64,
    pub gold_sunk: u64,
    /// Dev override of the day clock (C14); None = wall clock.
    pub time_override: Option<f32>,
    pub zones: HashMap<Act, Zone>,
    next_id: EntityId,
    rng: Rng,
    /// Party id by owner conn id (P1) — ephemeral shared-credit groups.
    party_of: HashMap<u64, u64>,
}

/// Something that happened this tick and needs to be reported outside the sim
/// (e.g. pushed to a specific player's connection).
pub enum SimEvent {
    LevelUp { owner: u64, level: u32 },
    Died { owner: u64, home: Option<Act> },
    Loot { owner: u64, item: String },
    /// Generic per-player info line (cast failures, duel results, PvP kills).
    Info { owner: u64, text: String },
    /// Cosmetic combat event, broadcast to every client in the act (drives
    /// remote-entity swing/hit/death animations).
    Combat { act: Act, kind: EventKind, src: EntityId, dst: Option<EntityId> },
    /// A cast-time ability began (`secs` > 0) or was interrupted (`secs` == 0).
    CastStart { owner: u64, ability: &'static str, secs: f32 },
    /// An ability fired: the owner's client starts its cooldown sweep.
    AbilityUsed { owner: u64, ability: &'static str, cooldown: f32 },
    /// Health/mana/stamina drifted (regeneration): push a fresh stat block.
    Vitals { owner: u64 },
}

impl World {
    pub fn new(seed: u64) -> Self {
        let mut w = World {
            zones: HashMap::new(),
            next_id: 1,
            rng: Rng::new(seed),
            party_of: HashMap::new(),
            gold_minted: 0,
            gold_sunk: 0,
            time_override: None,
        };
        for act in Act::ALL {
            let mut zone = Zone {
                act,
                entities: HashMap::new(),
                tick: 0,
                entry: Vec2::ZERO,
                grid: AoiGrid::new(),
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
        // Caves (C09): each cave pocket gets rich ore nodes, a few mobs from
        // one act tier up, and a <resource>_dweller mini-boss.
        let next_act = Act::ALL[(Act::ALL.iter().position(|a| *a == act).unwrap_or(0) + 1).min(Act::ALL.len() - 1)];
        let tough_tag = act_spawn_table(next_act)[0].0;
        for cave in crate::caves::caves_for_act(act) {
            let center = Vec2::new(cave.x, cave.y);
            for i in 0..4 {
                let off = self.rng.point(130.0);
                let id = self.alloc_id();
                let res = &cave.resources[i % cave.resources.len()];
                zone.entities.insert(id, make_resource(id, center + off, &format!("ore_{res}")));
            }
            for _ in 0..3 {
                let off = self.rng.point(170.0);
                let id = self.alloc_id();
                zone.entities.insert(id, make_enemy(id, center + off, tough_tag, next_act));
            }
            let id = self.alloc_id();
            let boss_tag = format!("{}_dweller", cave.resources[0]);
            zone.entities.insert(id, make_enemy(id, center + self.rng.point(60.0), &boss_tag, act));
        }
        // The act's quest givers: the Elder at the inn, a Wanderer at the
        // crossroads, and a Seer further afield (see quests.rs giver mapping).
        let id = self.alloc_id();
        zone.entities.insert(id, make_npc(id, zone.entry + Vec2::new(90.0, 0.0), "Elder"));
        let id = self.alloc_id();
        zone.entities.insert(id, make_npc(id, zone.entry + Vec2::new(-70.0, 110.0), "Wanderer"));
        let id = self.alloc_id();
        zone.entities.insert(id, make_npc(id, zone.entry + Vec2::new(260.0, -170.0), "Seer"));
        // Cherub Sentinel at the Gate of Eden: the new-character starting
        // questgiver (storyline: exile from the garden, clothed in skins).
        if act == Act::Eden {
            let id = self.alloc_id();
            zone.entities.insert(id, make_npc(id, GATE_OF_EDEN + Vec2::new(60.0, -40.0), "Sentinel"));
        }
        // Innkeeper (C11): buys anything priced, sells staples.
        let id = self.alloc_id();
        zone.entities.insert(id, make_npc(id, zone.entry + Vec2::new(40.0, -120.0), "Innkeeper"));
        // Faction Quartermaster (C10): rep-gated vendor at every inn.
        let id = self.alloc_id();
        zone.entities.insert(id, make_npc(id, zone.entry + Vec2::new(-160.0, -60.0), "Quartermaster"));
        // Beast-Master Jabal offers the mount questline in Enoch (C06).
        if act == Act::Enoch {
            let id = self.alloc_id();
            zone.entities.insert(id, make_npc(id, zone.entry + Vec2::new(0.0, -220.0), "Jabal"));
        }
        // One elite "alpha" per act — the dungeon-boss placeholder. Guaranteed
        // rare drop (thick_hide) and big XP.
        let pos = self.rng.point(WORLD_BOUNDS * 0.7);
        let id = self.alloc_id();
        // Raw stats: the boss is elite even if it rolls near the entry
        // (the starting-area mercy nerf must never touch it).
        let mut boss = make_enemy_raw(id, pos, enemy_tag, act);
        boss.tag = Some(format!("{enemy_tag}_alpha"));
        boss.max_health *= 4;
        boss.health = boss.max_health;
        boss.damage *= 2;
        boss.xp_value *= 5;
        boss.aggro_range = 300.0;
        zone.entities.insert(id, boss);
        crate::questgen::seed_camps(zone, &mut self.next_id); // beta-server-track hook (B05)
        // File every boot entity in the AoI grid now that the zone is populated.
        zone.rebuild_grid();
    }

    /// Spawn a player entity into its zone from a character sheet. Returns the
    /// new entity id.
    pub fn spawn_player(&mut self, owner: u64, mut sheet: CharacterSheet) -> EntityId {
        // Saves from before B03 have no stamina pool: start them full.
        if sheet.max_stamina <= 0 {
            sheet.max_stamina = STAMINA_MAX;
            sheet.stamina = STAMINA_MAX;
        }
        let id = self.alloc_id();
        let act = sheet.act;
        let pos = Vec2::new(sheet.x, sheet.y);
        let ent = Entity {
            id,
            kind: EntityKind::Player,
            pos,
            cell: aoi::cell_of(pos.x, pos.y) as u32,
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
            casting: None,
            auras: Vec::new(),
            regen_accum: 0.0,
            vitals_sent: (0, 0, 0),
            duel_with: None,
            rest_accum: 0.0,
            mounted: false,
            combat_timer: 0.0,
        godmode: false,
        forced_pvp: false,
        forced_pvp_timer: 0.0,
            enrage_timer: 0.0,
            mount_hits_left: 0,
            owner: Some(owner),
            sheet: Some(sheet),
        };
        self.zones.get_mut(&act).unwrap().insert_entity(id, ent);
        id
    }

    pub fn remove_player(&mut self, act: Act, id: EntityId) -> Option<CharacterSheet> {
        self.zones
            .get_mut(&act)
            .and_then(|z| z.remove_entity(&id))
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

        // P1: snapshot party membership before borrowing the zone mutably.
        let party_of = self.party_of.clone();
        let zone = self.zones.get_mut(&act).unwrap();
        zone.tick += 1;

        // Snapshots of live players and enemies for cross-entity AI/combat.
        // (id, pos, pvp-flagged, duel partner)
        let player_info: Vec<(EntityId, Vec2, bool, Option<EntityId>)> = zone
            .entities
            .values()
            .filter(|e| e.kind == EntityKind::Player && e.health > 0)
            .map(|e| {
                let pvp = e.forced_pvp
                    || e.forced_pvp_timer > 0.0
                    || e.sheet.as_ref().map(|s| s.pvp).unwrap_or(false);
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

        // Live players with their party id, for party-aimed abilities.
        let allies: Vec<(EntityId, Vec2, Option<u64>)> = zone
            .entities
            .values()
            .filter(|e| e.kind == EntityKind::Player && e.health > 0)
            .map(|e| (e.id, e.pos, e.owner.and_then(|o| party_of.get(&o).copied())))
            .collect();

        // (target_id, damage, attacker_owner_entity_if_player_source)
        let mut damage: Vec<(EntityId, i32, Option<EntityId>)> = Vec::new();
        // Ability effects on (possibly other) entities, applied after the loop.
        let mut aura_q: Vec<(EntityId, Aura)> = Vec::new();
        let mut heal_q: Vec<(EntityId, i32)> = Vec::new();
        let entry = zone.entry;
        let ztick = zone.tick;

        for e in zone.entities.values_mut() {
            if e.attack_cooldown > 0.0 {
                e.attack_cooldown -= DT;
            }
            // Tick this entity's auras and sum what they do to it.
            let mut st = AuraState { stunned: false, rooted: false, speed: 1.0, out: 1.0, taunt: None };
            if !e.auras.is_empty() && e.health > 0 {
                let eid = e.id;
                let mut hot = 0;
                for a in e.auras.iter_mut() {
                    a.left -= DT;
                    match a.kind {
                        AuraKind::Dot | AuraKind::Hot => {
                            a.tick += DT;
                            if a.tick >= 1.0 - 1e-3 {
                                a.tick -= 1.0;
                                if a.kind == AuraKind::Dot {
                                    damage.push((eid, a.mag as i32, Some(a.src)));
                                } else {
                                    hot += a.mag as i32;
                                }
                            }
                        }
                        AuraKind::Stun => st.stunned = true,
                        AuraKind::Root => st.rooted = true,
                        AuraKind::Slow => st.speed *= 1.0 - a.mag,
                        AuraKind::Haste => st.speed *= 1.0 + a.mag,
                        AuraKind::Taunt => st.taunt = Some(a.src),
                        AuraKind::Weaken => st.out *= 1.0 - a.mag,
                        AuraKind::Might => st.out *= 1.0 + a.mag,
                        AuraKind::Expose | AuraKind::Guard | AuraKind::Shield => {}
                    }
                }
                e.auras.retain(|a| a.left > 1e-3);
                if hot > 0 {
                    e.health = (e.health + hot).min(e.max_health);
                }
            }
            match e.kind {
                EntityKind::Player => {
                    if e.health <= 0 {
                        e.dead_timer -= DT;
                        if e.dead_timer <= 0.0 {
                            let home = e.sheet.as_ref().and_then(|s| s.home_act);
                            e.health = e.max_health;
                            e.pos = entry;
                            if let Some(o) = e.owner {
                                // If home is in another act, main.rs migrates
                                // the player there on Died; otherwise this inn.
                                events.push(SimEvent::Died { owner: o, home });
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
                    if e.enrage_timer > 0.0 {
                        e.enrage_timer -= DT;
                    }
                    if e.intent.length_squared() > 0.0001 && !st.stunned && !st.rooted {
                        let dir = e.intent.normalize_or_zero();
                        let mount_mult = if e.mounted {
                            e.sheet
                                .as_ref()
                                .and_then(|s| active_mount(s))
                                .map(|sp| mount_tier(sp).0)
                                .unwrap_or(1.6)
                        } else {
                            1.0
                        };
                        e.pos += dir * (e.speed * fatigue_penalty * mount_mult * st.speed) * DT;
                        e.pos = e.pos.clamp(Vec2::splat(-WORLD_BOUNDS), Vec2::splat(WORLD_BOUNDS));
                        e.rot = dir.y.atan2(dir.x);
                    }
                    // World-PvP capital zones (C16): standing in an ENEMY
                    // capital force-flags you; leaving starts a linger timer.
                    {
                        let my_faction = e.sheet.as_ref().and_then(|s| s.faction.clone());
                        // Lineage is optional until level 10, so `faction` is
                        // commonly None. Treating None as "no enemy capital
                        // exists" made undecided players permanently immune —
                        // they could walk any capital untouchable, which
                        // defeats the point of default-flagged zones. An
                        // unaligned outsider is hostile in EVERY capital.
                        let inside = pvp_zones(act).find(|z| {
                            my_faction.as_deref() != Some(z.faction.as_str())
                                && Vec2::new(z.x, z.y).distance(e.pos) <= z.radius
                        });
                        match (inside, e.forced_pvp) {
                            (Some(z), false) => {
                                e.forced_pvp = true;
                                if let Some(o) = e.owner {
                                    events.push(SimEvent::Info {
                                        owner: o,
                                        text: format!(
                                            "You enter the {} - you are flagged for PvP!",
                                            z.name
                                        ),
                                    });
                                }
                            }
                            (None, true) => {
                                e.forced_pvp = false;
                                e.forced_pvp_timer = FORCED_PVP_LINGER;
                                if let Some(o) = e.owner {
                                    events.push(SimEvent::Info {
                                        owner: o,
                                        text: format!(
                                            "You leave enemy territory. PvP flag drops in {FORCED_PVP_LINGER:.0}s."
                                        ),
                                    });
                                }
                            }
                            _ => {}
                        }
                        if !e.forced_pvp && e.forced_pvp_timer > 0.0 {
                            e.forced_pvp_timer -= DT;
                        }
                    }
                    if e.gcd > 0.0 {
                        e.gcd -= DT;
                    }
                    for cd in e.cooldowns.values_mut() {
                        *cd -= DT;
                    }
                    e.cooldowns.retain(|_, cd| *cd > 0.0);
                    let mut poi_xp: u32 = 0;
                    e.regen_accum += DT;
                    let beat = e.regen_accum >= REGEN_BEAT_SECS;
                    if beat {
                        e.regen_accum -= REGEN_BEAT_SECS;
                    }
                    if let Some(s) = e.sheet.as_mut() {
                        if beat && e.health < e.max_health {
                            e.health = (e.health + HEALTH_REGEN_PER_BEAT).min(e.max_health);
                        }
                        s.health = e.health;
                        if beat && s.mana < s.max_mana {
                            s.mana = (s.mana + MANA_REGEN_PER_BEAT).min(s.max_mana);
                        }
                        if ztick % STAMINA_REGEN_TICKS == 0 && s.stamina < s.max_stamina {
                            s.stamina += 1;
                        }
                        // Once a second, tell the owner if regeneration moved
                        // their bars (other changes already push stats).
                        if ztick % TICK_HZ == 0 {
                            let v = (e.health, s.mana, s.stamina);
                            if v != e.vitals_sent {
                                e.vitals_sent = v;
                                if let Some(o) = e.owner {
                                    events.push(SimEvent::Vitals { owner: o });
                                }
                            }
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
                        // Cave discovery (C09): caves count as POIs.
                        for cave in crate::caves::caves_for_act(act) {
                            if e.pos.distance(Vec2::new(cave.x, cave.y)) <= crate::caves::CAVE_RADIUS
                                && !s.discovered.iter().any(|d| d == &cave.name)
                            {
                                s.discovered.push(cave.name.clone());
                                let tier = Act::ALL.iter().position(|a| *a == act).unwrap_or(0) as u32;
                                let xp = 75 * (tier + 1);
                                if let Some(o) = e.owner {
                                    events.push(SimEvent::Info {
                                        owner: o,
                                        text: format!("Discovered: {} (+{} xp)", cave.name, xp),
                                    });
                                }
                                poi_xp += xp;
                                break;
                            }
                        }
                    }
                    if poi_xp > 0 && award_xp(e, poi_xp) {
                        if let Some(o) = e.owner {
                            let lvl = e.sheet.as_ref().map(|s| s.level).unwrap_or(1);
                            events.push(SimEvent::LevelUp { owner: o, level: lvl });
                        }
                    }
                    // Stunned: no swings, no casts; a cast in progress is lost.
                    if st.stunned {
                        e.attack_queued = false;
                        e.cast_queued = None;
                        if let (Some((ab, _)), Some(o)) = (e.casting.take(), e.owner) {
                            events.push(SimEvent::CastStart { owner: o, ability: ab.id, secs: 0.0 });
                        }
                        continue;
                    }
                    // Talent/gear damage bonuses, scaled by Might/Weaken auras.
                    let (dmg_mult, melee_bonus, spell_bonus) = match e.sheet.as_ref() {
                        Some(s) => {
                            let (m, sp) = gear_bonus(s);
                            (damage_mult(s) * st.out, m, sp)
                        }
                        None => (st.out, 0, 0),
                    };
                    // A player may be hit by another player only in a mutual
                    // duel or when both are PvP-flagged.
                    let my_pvp = e.forced_pvp
                        || e.forced_pvp_timer > 0.0
                        || e.sheet.as_ref().map(|s| s.pvp).unwrap_or(false);
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
                    // Progress or resolve an ability cast (data in
                    // `crate::abilities`).
                    let owner = e.owner.unwrap_or(0);
                    let mut fire: Option<&'static Ability> = None;
                    if let Some((ab, left)) = e.casting {
                        if e.intent.length_squared() > 0.0001 {
                            // Moving breaks concentration.
                            e.casting = None;
                            events.push(SimEvent::CastStart { owner, ability: ab.id, secs: 0.0 });
                            events.push(SimEvent::Info { owner, text: "Interrupted.".into() });
                        } else if left - DT <= 1e-3 {
                            e.casting = None;
                            fire = Some(ab);
                        } else {
                            e.casting = Some((ab, left - DT));
                        }
                    }
                    if let Some(ab_id) = e.cast_queued.take() {
                        if e.casting.is_some() {
                            events.push(SimEvent::Info { owner, text: "You are already casting.".into() });
                        } else if fire.is_none() {
                            match cast_check(e, &ab_id) {
                                Err(text) => events.push(SimEvent::Info { owner, text: text.into() }),
                                Ok(ab) if ab.cast > 0.0 => {
                                    if e.intent.length_squared() > 0.0001 {
                                        events.push(SimEvent::Info {
                                            owner,
                                            text: "You must stand still to cast that.".into(),
                                        });
                                    } else {
                                        e.casting = Some((ab, ab.cast));
                                        e.gcd = GCD_SECS;
                                        events.push(SimEvent::CastStart { owner, ability: ab.id, secs: ab.cast });
                                    }
                                }
                                Ok(ab) => fire = Some(ab),
                            }
                        }
                    }
                    if let Some(ab) = fire {
                        let res = class_resource(ab.class);
                        let facing = Vec2::new(e.rot.cos(), e.rot.sin());
                        let hostile: Vec<EntityId> = match ab.aim {
                            // Nearest valid target in range, roughly in front.
                            Aim::Enemy => enemies
                                .iter()
                                .chain(pvp_targets.iter())
                                .filter_map(|(tid, tpos)| {
                                    let to = *tpos - e.pos;
                                    let d = to.length();
                                    (d <= ab.range && d > 0.01 && to.normalize().dot(facing) >= 0.0)
                                        .then_some((*tid, d))
                                })
                                .min_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
                                .map(|(tid, _)| tid)
                                .into_iter()
                                .collect(),
                            Aim::Around => enemies
                                .iter()
                                .chain(pvp_targets.iter())
                                .filter(|(_, tpos)| e.pos.distance(*tpos) <= ab.range)
                                .map(|(tid, _)| *tid)
                                .collect(),
                            Aim::Myself | Aim::Party => Vec::new(),
                        };
                        let enough = e.sheet.as_ref().is_some_and(|s| resource_of(s, res) >= ab.cost);
                        if ab.aim == Aim::Enemy && hostile.is_empty() {
                            // Nothing to hit: the ability is not spent.
                            events.push(SimEvent::Info { owner, text: "No target in range.".into() });
                        } else if !enough {
                            events.push(SimEvent::Info {
                                owner,
                                text: format!("Not enough {}.", res.as_str()),
                            });
                        } else {
                            let friends: Vec<EntityId> = if ab.aim == Aim::Party {
                                let my_party = e.owner.and_then(|o| party_of.get(&o).copied());
                                allies
                                    .iter()
                                    .filter(|(aid, apos, party)| {
                                        *aid == e.id
                                            || (my_party.is_some()
                                                && *party == my_party
                                                && e.pos.distance(*apos) <= ab.range)
                                    })
                                    .map(|(aid, _, _)| *aid)
                                    .collect()
                            } else {
                                vec![e.id]
                            };
                            if ab.cast <= 0.0 {
                                e.gcd = GCD_SECS;
                            }
                            e.cooldowns.insert(ab.id.to_string(), ab.cooldown);
                            events.push(SimEvent::Combat {
                                act,
                                kind: EventKind::Cast,
                                src: e.id,
                                dst: hostile.first().copied(),
                            });
                            events.push(SimEvent::AbilityUsed { owner, ability: ab.id, cooldown: ab.cooldown });
                            let heal_m = e.sheet.as_ref().map(heal_mult).unwrap_or(1.0);
                            if let Some(s) = e.sheet.as_mut() {
                                add_resource(s, res, -ab.cost);
                            }
                            let bonus = match ab.school {
                                School::Melee => melee_bonus,
                                School::Spell => spell_bonus,
                            };
                            let src = e.id;
                            let mut aura = |to: &[EntityId], kind: AuraKind, secs: f32, mag: f32| {
                                for t in to {
                                    aura_q.push((*t, Aura { kind, left: secs, mag, src, ability: ab.id, tick: 0.0 }));
                                }
                            };
                            let pct = |p: u32| p as f32 / 100.0;
                            for fx in ab.fx {
                                match *fx {
                                    Fx::Damage(base) => {
                                        let dmg = ((base + bonus) as f32 * dmg_mult).round() as i32;
                                        for t in &hostile {
                                            damage.push((*t, dmg, Some(src)));
                                        }
                                    }
                                    Fx::Dot { dmg, secs } => {
                                        aura(&hostile, AuraKind::Dot, secs, (dmg as f32 * dmg_mult).round().max(1.0))
                                    }
                                    Fx::Stun(secs) => aura(&hostile, AuraKind::Stun, secs, 0.0),
                                    Fx::Root(secs) => aura(&hostile, AuraKind::Root, secs, 0.0),
                                    Fx::Slow { pct: p, secs } => aura(&hostile, AuraKind::Slow, secs, pct(p)),
                                    Fx::Taunt(secs) => aura(&hostile, AuraKind::Taunt, secs, 0.0),
                                    Fx::Weaken { pct: p, secs } => aura(&hostile, AuraKind::Weaken, secs, pct(p)),
                                    Fx::Expose { pct: p, secs } => aura(&hostile, AuraKind::Expose, secs, pct(p)),
                                    Fx::Heal(base) => {
                                        let amount = (base as f32 * heal_m).round() as i32;
                                        for t in &friends {
                                            heal_q.push((*t, amount));
                                        }
                                    }
                                    Fx::Hot { heal, secs } => {
                                        aura(&friends, AuraKind::Hot, secs, (heal as f32 * heal_m).round().max(1.0))
                                    }
                                    Fx::Might { pct: p, secs } => aura(&friends, AuraKind::Might, secs, pct(p)),
                                    Fx::Guard { pct: p, secs } => aura(&friends, AuraKind::Guard, secs, pct(p)),
                                    Fx::Haste { pct: p, secs } => aura(&friends, AuraKind::Haste, secs, pct(p)),
                                    Fx::Shield { amount, secs } => aura(&friends, AuraKind::Shield, secs, amount as f32),
                                    Fx::Restore(n) => {
                                        if let Some(s) = e.sheet.as_mut() {
                                            add_resource(s, res, n);
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                EntityKind::Enemy => {
                    if e.health <= 0 {
                        continue;
                    }
                    if e.enrage_timer > 0.0 {
                        e.enrage_timer -= DT;
                    }
                    if st.stunned {
                        continue;
                    }
                    let move_mult = if st.rooted { 0.0 } else { st.speed };
                    // A taunt overrides "nearest player" and aggro range.
                    let taunter = st.taunt.and_then(|t| {
                        players
                            .iter()
                            .find(|(pid, _)| *pid == t)
                            .map(|(pid, ppos)| (*pid, *ppos, e.pos.distance(*ppos)))
                    });
                    let taunted = taunter.is_some();
                    let nearest = taunter.or_else(|| nearest_of(&players, e.pos));
                    match nearest {
                        Some((pid, ppos, dist)) if taunted || dist <= e.aggro_range => {
                            e.state = AiState::Chase;
                            let to = ppos - e.pos;
                            if dist > ENEMY_MELEE_RANGE {
                                let dir = to.normalize_or_zero();
                                e.pos += dir * e.speed * move_mult * DT;
                                e.rot = dir.y.atan2(dir.x);
                            } else if e.attack_cooldown <= 0.0 {
                                e.attack_cooldown = ATTACK_COOLDOWN;
                                let dmg = if e.enrage_timer > 0.0 { e.damage * 3 / 2 } else { e.damage };
                                let dmg = (dmg as f32 * st.out).round() as i32;
                                damage.push((pid, dmg, None));
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
                            e.pos += dir * (e.speed * 0.5 * move_mult) * DT;
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
                            // Face where we graze-walk (fixes backwards-running animals).
                            if dir.length_squared() > 0.0 {
                                e.rot = dir.y.atan2(dir.x);
                            }
                        }
                    }
                    e.pos = e.pos.clamp(Vec2::splat(-WORLD_BOUNDS), Vec2::splat(WORLD_BOUNDS));
                }
                EntityKind::Resource | EntityKind::Npc => {}
            }
        }

        // Mob collision (v0.5.0, Daniel): players walk through each other,
        // but never through living mobs or NPCs — resolve overlap by pushing
        // the player out along the contact normal after movement.
        const MOB_RADIUS: f32 = 9.0;
        let solids: Vec<Vec2> = zone
            .entities
            .values()
            .filter(|e| {
                matches!(e.kind, EntityKind::Enemy | EntityKind::Wildlife | EntityKind::Npc)
                    && e.health > 0
            })
            .map(|e| e.pos)
            .collect();
        for e in zone.entities.values_mut() {
            if e.kind != EntityKind::Player || e.health <= 0 {
                continue;
            }
            for sp in &solids {
                let d = e.pos - *sp;
                let len = d.length();
                if len < MOB_RADIUS {
                    let push = if len > 0.001 { d / len } else { Vec2::X };
                    e.pos = *sp + push * MOB_RADIUS;
                }
            }
        }

        // Battleground v1 (P6): holding ground inside an enemy capital zone
        // while PvP-flagged earns honor every 60 seconds.
        if zone.tick % (TICK_HZ as u64 * 60) == 0 {
            for e in zone.entities.values_mut() {
                if e.kind != EntityKind::Player || e.health <= 0 || !e.forced_pvp {
                    continue;
                }
                let Some(s) = e.sheet.as_mut() else { continue };
                // Mirror the force-flag rule: an unaligned player is hostile in
                // EVERY capital. Requiring a faction here would leave undecided
                // players flagged and killable on enemy ground but unable to
                // earn anything for holding it.
                let my_faction = s.faction.clone();
                let inside = pvp_zones(act).any(|z| {
                    my_faction.as_deref() != Some(z.faction.as_str())
                        && Vec2::new(z.x, z.y).distance(e.pos) <= z.radius
                });
                if inside {
                    let h = s.reputation.entry("honor".to_string()).or_insert(0);
                    *h += 10;
                    if let Some(o) = e.owner {
                        events.push(SimEvent::Info {
                            owner: o,
                            text: format!("+10 honor for holding contested ground ({} total)", h),
                        });
                    }
                }
            }
        }

        // Land this tick's ability effects before damage, so a shield or heal
        // cast now counts against blows struck now.
        for (tid, aura) in aura_q {
            let Some(t) = zone.entities.get_mut(&tid) else { continue };
            if t.health <= 0 {
                continue;
            }
            if matches!(aura.kind, AuraKind::Stun | AuraKind::Root) && cc_immune(t) {
                continue;
            }
            // Destructible objects have no will to bend.
            if aura.kind == AuraKind::Taunt && t.damage == 0 {
                continue;
            }
            match t
                .auras
                .iter_mut()
                .find(|a| a.kind == aura.kind && a.ability == aura.ability && a.src == aura.src)
            {
                Some(old) => *old = aura,
                None => t.auras.push(aura),
            }
        }
        for (tid, amount) in heal_q {
            let Some(t) = zone.entities.get_mut(&tid) else { continue };
            if t.health <= 0 {
                continue;
            }
            t.health = (t.health + amount).min(t.max_health);
            if let Some(s) = t.sheet.as_mut() {
                s.health = t.health;
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
                    crate::durability::wear_weapon(a, zone.tick); // beta-server-track hook (B06)
                }
            }
            if let Some(t) = zone.entities.get_mut(&target) {
                if t.health <= 0 {
                    continue;
                }
                if t.mounted && t.mount_hits_left > 0 {
                    t.mount_hits_left -= 1; // bear-tier mounts soak hits
                } else {
                    t.mounted = false;
                }
                t.combat_timer = COMBAT_LOCK_SECS;
                if t.godmode {
                    continue; // dev godmode (C14)
                }
                // Expose/Guard scale the blow; shields soak what is left.
                let mut dmg = dmg;
                if !t.auras.is_empty() {
                    let mult = t.auras.iter().fold(1.0f32, |m, a| match a.kind {
                        AuraKind::Expose => m * (1.0 + a.mag),
                        AuraKind::Guard => m * (1.0 - a.mag),
                        _ => m,
                    });
                    dmg = (dmg as f32 * mult).round() as i32;
                    for a in t.auras.iter_mut().filter(|a| a.kind == AuraKind::Shield) {
                        let soak = dmg.min(a.mag as i32).max(0);
                        a.mag -= soak as f32;
                        dmg -= soak;
                    }
                    t.auras.retain(|a| a.kind != AuraKind::Shield || a.mag >= 1.0);
                }
                crate::durability::wear_armor(t, zone.tick); // beta-server-track hook (B06)
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
                    t.auras.clear();
                    t.casting = None;
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
                        crate::durability::on_death(t); // beta-server-track hook (B06)
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
                    // A finished duel leaves no lingering harm between the two.
                    p.auras.retain(|a| a.src != other);
                }
            }
        }

        // Resolve kills. Enemies grant xp + a trophy and respawn a replacement;
        // harvested resource nodes grant a material and respawn elsewhere.
        for (target_id, attacker_ent) in killed {
            let origin = zone.entities.get(&target_id).map(|t| t.origin);
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
            zone.remove_entity(&target_id);

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
                        zone.insert_entity(nid, make_enemy(nid, pos, tag, act));
                        let reward = if def.drops.is_empty() {
                            format!("{tag}_trophy")
                        } else {
                            let i = rng.range(0.0, def.drops.len() as f32) as usize;
                            def.drops[i.min(def.drops.len() - 1)].clone()
                        };
                        (reward, xp_value)
                    } else if let Some(res) = tag.strip_suffix("_dweller") {
                        // Cave mini-boss (C09): respawns near its cave (its
                        // origin), guaranteed crafting-rare drop.
                        let home = origin.unwrap_or(pos);
                        let mut boss = make_enemy(nid, home + rng.point(60.0), tag, act);
                        boss.tag = Some(tag.to_string());
                        zone.insert_entity(nid, boss);
                        (res.to_string(), xp_value)
                    } else if let Some(base) = tag.strip_suffix("_alpha") {
                        // The act boss respawns as a boss and drops a rare.
                        let mut boss = make_enemy(nid, pos, base, act);
                        boss.tag = Some(tag.to_string());
                        boss.max_health *= 4;
                        boss.health = boss.max_health;
                        boss.damage *= 2;
                        boss.xp_value *= 5;
                        zone.insert_entity(nid, boss);
                        ("thick_hide".to_string(), xp_value)
                    } else {
                        zone.insert_entity(nid, make_enemy(nid, pos, tag, act));
                        (format!("{tag}_trophy"), xp_value)
                    }
                }
                EntityKind::Resource => {
                    let tag = etag.as_deref().unwrap_or("tree");
                    if let Some(res) = tag.strip_prefix("ore_") {
                        // Cave ore (C09): respawns inside its cave pocket.
                        let home = origin.unwrap_or(pos);
                        zone.insert_entity(nid, make_resource(nid, home + rng.point(120.0), tag));
                        (res.to_string(), 0)
                    } else {
                        zone.insert_entity(nid, make_resource(nid, pos, tag));
                        let material = if tag == "rock" { "stone" } else { "wood" };
                        (material.to_string(), 0)
                    }
                }
                _ => continue,
            };

            // Award xp + loot. Party members (P1) within PARTY_SHARE_RANGE
            // of the killer split the XP evenly and all gain quest credit;
            // loot, gold, rep, and profession skill stay with the killer.
            let mut recipients: Vec<(EntityId, bool)> = Vec::new();
            if let Some(owner_ent) = attacker_ent {
                recipients.push((owner_ent, true));
                let kinfo = zone
                    .entities
                    .get(&owner_ent)
                    .and_then(|e| e.owner.map(|o| (o, e.pos)));
                if let Some((ko, kp)) = kinfo {
                    if let Some(pid) = party_of.get(&ko) {
                        for e in zone.entities.values() {
                            if e.id != owner_ent
                                && e.kind == EntityKind::Player
                                && e.health > 0
                                && e.owner.map(|o| party_of.get(&o) == Some(pid)).unwrap_or(false)
                                && e.pos.distance(kp) < PARTY_SHARE_RANGE
                            {
                                recipients.push((e.id, false));
                            }
                        }
                    }
                }
            }
            let share = if recipients.len() > 1 {
                (xp / recipients.len() as u32).max(1)
            } else {
                xp
            };
            for (owner_ent, is_killer) in recipients {
                if let Some(p) = zone.entities.get_mut(&owner_ent) {
                    if let Some(o) = p.owner {
                        if share > 0 && award_xp(p, share) {
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
                                // Enemy kills also pay gold and advance quests
                                // (gold/rep to the killer only).
                                EntityKind::Enemy if is_killer => {
                                    let tier = Act::ALL.iter().position(|a| *a == act).unwrap_or(0) as u32;
                                    s.gold += 2 + tier * 2;
                                    // Faction rep (C10): rival kills +5, act bosses +100.
                                    if let Some(f) = s.faction.clone() {
                                        let t = etag.as_deref().unwrap_or("");
                                        let mut gain = 0;
                                        if RIVAL_KILL_REP.iter().any(|(rt, ff)| t.starts_with(rt) && *ff == f) {
                                            gain += 5;
                                        }
                                        if t.ends_with("_alpha") {
                                            gain += 100;
                                        }
                                        if gain > 0 {
                                            let r = s.reputation.entry(f.clone()).or_insert(0);
                                            *r += gain * rep_mult();
                                            events.push(SimEvent::Info {
                                                owner: o,
                                                text: format!("+{} {} reputation ({})", gain * rep_mult(), f, rep_rank(*r)),
                                            });
                                        }
                                    }
                                }
                                // Harvesting levels the matching profession.
                                EntityKind::Resource if is_killer => {
                                    let is_ore = etag.as_deref().is_some_and(|t| t.starts_with("ore_"));
                                    let prof = if reward_item == "stone" || is_ore { "mining" } else { "woodcutting" };
                                    let skill = s.professions.entry(prof.to_string()).or_insert(0);
                                    if *skill < PROFESSION_CAP {
                                        *skill += 1;
                                    }
                                }
                                _ => {}
                            }
                            // Dungeon v1 (P5): cave bosses are dungeon
                            // encounters — every party member present gets
                            // the rare drop, not just the killer.
                            if !is_killer {
                                if let Some(res) =
                                    etag.as_deref().and_then(|t| t.strip_suffix("_dweller"))
                                {
                                    if s.inventory.len() < inv_cap(s) {
                                        s.inventory.push(res.to_string());
                                        events.push(SimEvent::Info {
                                            owner: o,
                                            text: format!("Dungeon loot: {res}!"),
                                        });
                                    }
                                }
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
                                                text: format!("Quest: {} - {}/{}", qid, prog, count),
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
                                                text: format!("Quest: {} - {}/{} {}", qid, have + 1, count, item),
                                            });
                                        }
                                    }
                                    _ => {} // beta-server-track (B05): explore/deliver advance in questgen
                                }
                            }
                            // Loot goes to the killer only.
                            if is_killer {
                                // Cave ore nodes yield double (C09).
                                let yield_n = if kind == EntityKind::Resource
                                    && etag.as_deref().is_some_and(|t| t.starts_with("ore_")) { 2 } else { 1 };
                                for _ in 0..yield_n {
                                    if s.inventory.len() < inv_cap(s) {
                                        s.inventory.push(reward_item.clone());
                                    }
                                }
                                events.push(SimEvent::Loot { owner: o, item: reward_item.clone() });
                            }
                        }
                    }
                }
            }
        }

        crate::questgen::tick_explore(zone, events); // beta-server-track hook (B05)
        // AoI grid maintenance: refile entities that crossed a cell boundary
        // this tick. This is a linear scan, but each entity costs only a
        // cell-index computation plus a branch when it stayed put — real work
        // happens solely for entities that actually moved cells, i.e.
        // O(moved entities), never O(players × entities). (Spawns/despawns
        // file through insert_entity/remove_entity; direct position sets
        // outside the tick refile via refile_entity.)
        for e in zone.entities.values_mut() {
            let c = aoi::cell_of(e.pos.x, e.pos.y);
            if c != e.cell as usize {
                zone.grid.move_entity(e.id, e.cell as usize, c);
                e.cell = c as u32;
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
        let (hp, mana) = class_base(class);
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
        // `moved` tracks cross-map teleports (hearthstone) so the AoI grid
        // can refile the entity immediately after the borrow ends.
        let (result, moved) = {
            let e = self.entity_mut(act, id).ok_or("no such player")?;
            let s = e.sheet.as_mut().ok_or("no sheet")?;
            let idx = s
                .inventory
                .iter()
                .position(|i| i == item)
                .ok_or("You don't have that.")?;
            let mut moved = false;
            let result = match item {
                "bread" => {
                    s.inventory.remove(idx);
                    e.health = (e.health + 40).min(e.max_health);
                    s.health = e.health;
                    Ok("You eat the bread and recover 40 health.".into())
                }
                // Hearthstone (C16 WoW parity): return to the zone inn, 30 min cd.
                "hearthstone" => {
                    let cd = e.cooldowns.get("hearthstone").copied().unwrap_or(0.0);
                    if cd > 0.0 {
                        return Err(format!("Hearthstone is on cooldown ({:.0}s).", cd));
                    }
                    e.cooldowns.insert("hearthstone".into(), 1800.0);
                    let entry = Vec2::ZERO; // every act's inn sits at the entry
                    e.pos = entry;
                    moved = true;
                    Ok("You focus on the inn's hearth and are carried home.".into())
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
            };
            (result, moved)
        };
        if moved {
            if let Some(z) = self.zones.get_mut(&act) {
                z.refile_entity(id);
            }
        }
        result
    }

    /// Equip gear from the inventory into its slot, swapping out the old piece.
    pub fn equip(&mut self, act: Act, id: EntityId, item: &str) -> Result<String, String> {
        // Mount items (C07) go to the "mount" slot with no stat changes.
        if item.starts_with("mount:") {
            let e = self.entity_mut(act, id).ok_or("no such player")?;
            let s = e.sheet.as_mut().ok_or("no sheet")?;
            let idx = s.inventory.iter().position(|i| i == item).ok_or("You don't have that.")?;
            s.inventory.remove(idx);
            if let Some(old) = s.equipment.insert("mount".into(), item.to_string()) {
                s.inventory.push(old);
            }
            e.mounted = false; // must re-mount on the new beast
            return Ok(format!("You saddle the {item}."));
        }
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

        // Innkeeper (C11): vendor — list staples and the buyback rate.
        if npc == "Innkeeper" {
            let wares = crate::economy::STAPLES
                .iter()
                .filter_map(|i| crate::economy::staple_price(i).map(|p| format!("{i} {p}g")))
                .collect::<Vec<_>>()
                .join(", ");
            return Ok(format!(
                "Innkeeper: I stock {wares}. I buy most anything at four-fifths its worth. Say Buy or Sell."
            ));
        }

        // Quartermaster (C10): vendor, not a questgiver — list rep-gated wares.
        if npc == "Quartermaster" {
            let Some(f) = s.faction.clone() else {
                return Ok("Quartermaster: I trade with the Two Lineages only. \
                           Choose sethite or cainite at level 10 (ChooseFaction)."
                    .into());
            };
            let rep = s.reputation.get(&f).copied().unwrap_or(0);
            let wares = QM_WARES
                .iter()
                .map(|(i, g, need)| {
                    let ok = if rank_at_least(rep, need) { "" } else { " [locked]" };
                    format!("{i} {g}g ({need}){ok}")
                })
                .collect::<Vec<_>>()
                .join(", ");
            return Ok(format!(
                "Quartermaster: {} standing - {}. Wares: {wares}. Say Buy.",
                rep_rank(rep),
                rep
            ));
        }

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
            let rewards = Self::grant_quest_completion(e, q);
            return Ok(format!("{npc}: It is done! {rewards}"));
        }

        // 2) Accept: first not-yet-started quest whose prerequisite is done.
        let accept = quests_for(act, &npc).find(|q| {
            !s.quests.contains_key(q.id)
                && !s.quests_done.iter().any(|d| d == q.id)
                && s.level >= q.min_level
                && q.requires.map_or(true, |r| s.quests_done.iter().any(|d| d == r))
                // Faction variants (C10) only show to their own lineage.
                && crate::quests::quest_faction(q.id)
                    .map_or(true, |f| s.faction.as_deref() == Some(f))
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
            return Ok(format!("{npc}: Not done yet - {}/{}.", prog, q.objective.count()));
        }

        Ok(format!("{npc}: You have done all I asked. Go with peace."))
    }

    /// Mark quest `q` done on the entity's sheet and pay its xp/gold/item
    /// rewards (plus faction rep and chain hints). Shared by the NPC turn-in
    /// path and the `QuestComplete` dev command.
    fn grant_quest_completion(e: &mut Entity, q: &crate::quests::QuestDef) -> String {
        let Some(s) = e.sheet.as_mut() else {
            return "(no character sheet)".into();
        };
        s.quests.remove(q.id);
        if !s.quests_done.iter().any(|d| d == q.id) {
            s.quests_done.push(q.id.to_string());
        }
        s.gold += q.gold;
        // Faction-variant quests pay reputation (C10).
        if let Some(f) = crate::quests::quest_faction(q.id) {
            let r = s.reputation.entry(f.to_string()).or_insert(0);
            *r += 250 * rep_mult();
        }
        let mut text = format!("(+{} xp, +{}g", q.xp, q.gold);
        if let Some(item) = q.item {
            if s.inventory.len() < 40 {
                s.inventory.push(item.to_string());
                text.push_str(&format!(", {item}"));
            }
        }
        text.push(')');
        // Theme chains hop acts (C08): tell the player where to go next.
        if let Some(hint) = crate::quests::quest_next_hint(q.id) {
            text.push_str(&format!(" {hint}"));
        }
        award_xp(e, q.xp);
        text
    }

    /// Grant raw XP (beta testing).
    pub fn dev_give_xp(&mut self, act: Act, id: EntityId, amount: u32) -> String {
        if let Some(e) = self.entity_mut(act, id) {
            award_xp(e, amount);
            format!("[dev] +{amount} xp")
        } else {
            "[dev] no player".into()
        }
    }

    /// Mark a quest done and pay its rewards (beta testing quest chains).
    /// Prereqs and progress are not checked — this is a testing shortcut.
    pub fn dev_quest_complete(&mut self, act: Act, id: EntityId, quest_id: &str) -> String {
        let Some(q) = crate::quests::quest(quest_id) else {
            return format!("[dev] unknown quest '{quest_id}'");
        };
        if let Some(e) = self.entity_mut(act, id) {
            let rewards = Self::grant_quest_completion(e, q);
            format!("[dev] quest '{quest_id}' completed {rewards}")
        } else {
            "[dev] no player".into()
        }
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
        let species = active_mount(s).map(str::to_string);
        let has_horn = s.inventory.iter().any(|i| i == "dire_wolf_horn");
        if species.is_none() && !has_horn {
            return Err("You have no mount. Earn the Horn of the Dire-Wolf or tame a beast.".into());
        }
        if e.combat_timer > 0.0 {
            return Err("You can't mount in combat.".into());
        }
        e.mounted = true;
        let (mult, _, hits) = species.as_deref().map(mount_tier).unwrap_or((1.6, 0, 0));
        e.mount_hits_left = hits;
        let label = species.unwrap_or_else(|| "dire_wolf".into());
        Ok(format!("You mount the {}. (+{:.0}% speed)", label, (mult - 1.0) * 100.0))
    }

    /// Attempt to tame a beast (C07): must be a `Tameable: Yes` bestiary
    /// species below 30% health, and consumes one taming lasso. Success
    /// odds are 40% +10% per act tier the player's zone sits above the
    /// mob's home act; failure enrages the beast.
    pub fn tame(&mut self, act: Act, id: EntityId, target: EntityId) -> Result<String, String> {
        let (ttag, tpos, thp, tmax) = {
            let z = self.zones.get(&act).ok_or("no zone")?;
            let t = z.entities.get(&target).ok_or("No such beast.")?;
            (t.tag.clone().unwrap_or_default(), t.pos, t.health, t.max_health)
        };
        let def = crate::mobs::mob_by_tag(&ttag).ok_or("That creature cannot be tamed.")?;
        if !def.tameable {
            return Err("That creature cannot be tamed.".into());
        }
        let (ppos, has_lasso) = {
            let e = self.zones[&act].entities.get(&id).ok_or("no player")?;
            let s = e.sheet.as_ref().ok_or("no sheet")?;
            (e.pos, s.inventory.iter().any(|i| i == "taming_lasso"))
        };
        if ppos.distance(tpos) > 160.0 {
            return Err("You are too far away to throw the lasso.".into());
        }
        if !has_lasso {
            return Err("You need a taming lasso.".into());
        }
        if thp * 10 > tmax * 3 {
            return Err("The beast is too strong - weaken it below 30% first.".into());
        }
        // Consume the lasso.
        {
            let e = self.entity_mut(act, id).unwrap();
            let s = e.sheet.as_mut().unwrap();
            let idx = s.inventory.iter().position(|i| i == "taming_lasso").unwrap();
            s.inventory.remove(idx);
        }
        let mob_tier = Act::ALL.iter().position(|a| a.as_str() == def.act).unwrap_or(0) as i32;
        let my_tier = Act::ALL.iter().position(|a| *a == act).unwrap_or(0) as i32;
        let chance = (0.4 + 0.1 * (my_tier - mob_tier).max(0) as f32).min(0.95);
        if self.rng.range(0.0, 1.0) <= chance {
            let z = self.zones.get_mut(&act).unwrap();
            z.remove_entity(&target);
            let e = z.entities.get_mut(&id).unwrap();
            let s = e.sheet.as_mut().unwrap();
            let item = format!("mount:{ttag}");
            if s.inventory.len() >= inv_cap(s) {
                return Err("Your bags are full - the beast escapes.".into());
            }
            s.inventory.push(item.clone());
            Ok(format!("The {} submits! {} added to your bags.", def.name, item))
        } else {
            let z = self.zones.get_mut(&act).unwrap();
            if let Some(t) = z.entities.get_mut(&target) {
                t.health = (t.health + t.max_health / 2).min(t.max_health);
                t.enrage_timer = 12.0;
            }
            Err(format!("The {} throws off the lasso and flies into a rage!", def.name))
        }
    }

    /// Park a mount item at the inn stable (must stand near the inn).
    pub fn stable_mount(&mut self, act: Act, id: EntityId, item: &str, put_in: bool) -> Result<String, String> {
        let entry = self.zones.get(&act).ok_or("no zone")?.entry;
        let e = self.entity_mut(act, id).ok_or("no player")?;
        if e.pos.distance(entry) > INN_RADIUS {
            return Err("You must be at the inn to use the stable.".into());
        }
        let s = e.sheet.as_mut().ok_or("no sheet")?;
        if !item.starts_with("mount:") {
            return Err("Only mounts can be stabled.".into());
        }
        if put_in {
            // The mount may be loose in the bags or saddled (equipped mounts
            // leave the inventory), so accept it from either place.
            if let Some(idx) = s.inventory.iter().position(|i| i == item) {
                s.inventory.remove(idx);
            } else if s.equipment.get("mount").map(String::as_str) == Some(item) {
                s.equipment.remove("mount");
                e.mounted = false;
            } else {
                return Err("You don't have that mount.".into());
            }
            s.stable.push(item.to_string());
            Ok(format!("{item} stabled."))
        } else {
            let idx = s.stable.iter().position(|i| i == item).ok_or("That mount is not in the stable.")?;
            if s.inventory.len() >= inv_cap(s) {
                return Err("Your bags are full.".into());
            }
            let it = s.stable.remove(idx);
            s.inventory.push(it);
            Ok(format!("{item} retrieved from the stable."))
        }
    }

    /// Set the player's home/respawn inn to their current act (v0.5.0).
    /// Must be standing in the inn.
    pub fn set_home(&mut self, act: Act, id: EntityId) -> Result<String, String> {
        if !self.at_inn(act, id) {
            return Err("You must be at an inn to make it your home.".into());
        }
        let s = self.sheet_mut(act, id).ok_or("no sheet")?;
        s.home_act = Some(act);
        Ok(format!("Home set. You will return to the {} inn when slain.", act.as_str()))
    }

    /// Read-only view of a player's sheet (P3 mail pre-checks).
    pub fn sheet_of(&self, act: Act, id: EntityId) -> Option<&CharacterSheet> {
        self.zones.get(&act)?.entities.get(&id)?.sheet.as_ref()
    }

    /// Remove a mail attachment from the sender (pre-validated) (P3).
    pub fn mail_debit(&mut self, act: Act, id: EntityId, item: Option<&str>, gold: u32) {
        if let Some(s) = self.sheet_mut(act, id) {
            if let Some(it) = item {
                if let Some(idx) = s.inventory.iter().position(|i| i == it) {
                    s.inventory.remove(idx);
                }
            }
            s.gold = s.gold.saturating_sub(gold);
        }
    }

    /// Deliver a mail attachment; false if the bags can't take the item (P3).
    pub fn mail_credit(&mut self, act: Act, id: EntityId, item: Option<&str>, gold: u32) -> bool {
        let Some(s) = self.sheet_mut(act, id) else { return false };
        if let Some(it) = item {
            if s.inventory.len() >= inv_cap(s) {
                return false;
            }
            s.inventory.push(it.to_string());
        }
        s.gold += gold;
        true
    }

    /// Bank vault (P2): move an item in/out at the inn. `put_in` deposits.
    pub fn bank_item(&mut self, act: Act, id: EntityId, item: &str, put_in: bool) -> Result<String, String> {
        let entry = self.zones.get(&act).ok_or("no zone")?.entry;
        let e = self.entity_mut(act, id).ok_or("no player")?;
        if e.pos.distance(entry) > INN_RADIUS {
            return Err("You must be at the inn to use your bank vault.".into());
        }
        let s = e.sheet.as_mut().ok_or("no sheet")?;
        if put_in {
            let idx = s.inventory.iter().position(|i| i == item).ok_or("You don't have that item.")?;
            if s.bank.len() >= 60 {
                return Err("Your bank vault is full.".into());
            }
            let it = s.inventory.remove(idx);
            s.bank.push(it);
            Ok(format!("{item} deposited. Vault: {} items.", s.bank.len()))
        } else {
            let idx = s.bank.iter().position(|i| i == item).ok_or("That item is not in your vault.")?;
            if s.inventory.len() >= inv_cap(s) {
                return Err("Your bags are full.".into());
            }
            let it = s.bank.remove(idx);
            s.inventory.push(it);
            Ok(format!("{item} withdrawn."))
        }
    }

    /// Bank gold (P2): positive deposits, negative withdraws.
    pub fn bank_gold(&mut self, act: Act, id: EntityId, amount: i64) -> Result<String, String> {
        let entry = self.zones.get(&act).ok_or("no zone")?.entry;
        let e = self.entity_mut(act, id).ok_or("no player")?;
        if e.pos.distance(entry) > INN_RADIUS {
            return Err("You must be at the inn to use your bank vault.".into());
        }
        let s = e.sheet.as_mut().ok_or("no sheet")?;
        if amount > 0 {
            let a = amount as u32;
            if s.gold < a {
                return Err("You don't have that much gold.".into());
            }
            s.gold -= a;
            s.bank_gold += a;
        } else {
            let a = (-amount) as u32;
            if s.bank_gold < a {
                return Err("Your vault doesn't hold that much gold.".into());
            }
            s.bank_gold -= a;
            s.gold += a;
        }
        Ok(format!("Vault: {}g. Bags: {}g.", s.bank_gold, s.gold))
    }

    /// Direct trade (P4): hand an item or gold to a nearby player. Returns
    /// (message-to-giver, message-to-receiver, receiver owner conn).
    pub fn trade_give(
        &mut self,
        act: Act,
        giver: EntityId,
        to_name: &str,
        item: Option<&str>,
        gold: u32,
    ) -> Result<(String, String, u64), String> {
        const TRADE_RANGE: f32 = 80.0;
        let z = self.zones.get(&act).ok_or("no zone")?;
        let gpos = z.entities.get(&giver).ok_or("no player")?.pos;
        let (rid, rowner) = z
            .entities
            .values()
            .find(|e| {
                e.id != giver
                    && e.kind == EntityKind::Player
                    && e.sheet.as_ref().map(|s| s.name.eq_ignore_ascii_case(to_name)) == Some(true)
            })
            .map(|e| (e.id, e.owner))
            .ok_or("No such player nearby.")?;
        let rowner = rowner.ok_or("No such player nearby.")?;
        let rpos = self.zones[&act].entities[&rid].pos;
        if gpos.distance(rpos) > TRADE_RANGE {
            return Err("They are too far away to trade.".into());
        }
        // Take from the giver first.
        let gname;
        {
            let gs = self
                .entity_mut(act, giver)
                .and_then(|e| e.sheet.as_mut())
                .ok_or("no sheet")?;
            gname = gs.name.clone();
            if let Some(it) = item {
                let idx = gs.inventory.iter().position(|i| i == it).ok_or("You don't have that item.")?;
                gs.inventory.remove(idx);
            }
            if gold > 0 {
                if gs.gold < gold {
                    return Err("You don't have that much gold.".into());
                }
                gs.gold -= gold;
            }
        }
        // Give to the receiver; on a full bag, refund the giver.
        let full = {
            let rs = self
                .entity_mut(act, rid)
                .and_then(|e| e.sheet.as_mut())
                .ok_or("no sheet")?;
            if let Some(it) = item {
                if rs.inventory.len() >= inv_cap(rs) {
                    true
                } else {
                    rs.inventory.push(it.to_string());
                    rs.gold += gold;
                    false
                }
            } else {
                rs.gold += gold;
                false
            }
        };
        if full {
            let gs = self.entity_mut(act, giver).and_then(|e| e.sheet.as_mut()).ok_or("no sheet")?;
            if let Some(it) = item {
                gs.inventory.push(it.to_string());
            }
            gs.gold += gold;
            return Err("Their bags are full.".into());
        }
        let what = match (item, gold) {
            (Some(it), 0) => it.to_string(),
            (Some(it), g) => format!("{it} and {g}g"),
            (None, g) => format!("{g}g"),
        };
        Ok((
            format!("You give {what} to {to_name}."),
            format!("{gname} gives you {what}."),
            rowner,
        ))
    }

    /// Align with a mortal lineage (C10). Level 10+; one-shot in spirit —
    /// re-choosing the other side is allowed but zeroes ALL reputation.
    pub fn choose_faction(&mut self, act: Act, id: EntityId, faction: &str) -> Result<String, String> {
        if !FACTIONS.contains(&faction) {
            return Err("Unknown lineage. Choose sethite or cainite.".into());
        }
        let s = self.sheet_mut(act, id).ok_or("no sheet")?;
        if s.level < 10 {
            return Err("The Two Lineages open at level 10.".into());
        }
        match s.faction.as_deref() {
            Some(f) if f == faction => Err(format!("You already walk the {faction} path.")),
            Some(_) => {
                s.faction = Some(faction.to_string());
                s.reputation.clear();
                Ok(format!("You forsake your line for the {faction}s. All reputation is lost."))
            }
            None => {
                s.faction = Some(faction.to_string());
                Ok(format!("You bind your lineage to the {faction}s."))
            }
        }
    }

    /// Sell an item to the innkeeper at 80% of table price (C11).
    pub fn sell(&mut self, act: Act, id: EntityId, item: &str) -> Result<String, String> {
        let entry = self.zones.get(&act).ok_or("no zone")?.entry;
        let e = self.entity_mut(act, id).ok_or("no player")?;
        if e.pos.distance(entry) > INN_RADIUS {
            return Err("The Innkeeper trades only at the inn.".into());
        }
        let s = e.sheet.as_mut().ok_or("no sheet")?;
        let Some(price) = crate::economy::sell_price(item) else {
            return Err("The Innkeeper has no use for that.".into());
        };
        let Some(idx) = s.inventory.iter().position(|i| i == item) else {
            return Err("You don't have that item.".into());
        };
        s.inventory.remove(idx);
        s.gold += price;
        self.gold_minted += price as u64;
        Ok(format!("Sold {item} for {price}g."))
    }

    /// Buy from the inn vendors: innkeeper staples (C11) need only gold;
    /// Quartermaster wares (C10) are rep- and lineage-gated.
    pub fn buy(&mut self, act: Act, id: EntityId, item: &str) -> Result<String, String> {
        let entry = self.zones.get(&act).ok_or("no zone")?.entry;
        let e = self.entity_mut(act, id).ok_or("no player")?;
        if e.pos.distance(entry) > INN_RADIUS {
            return Err("The Quartermaster trades only at the inn.".into());
        }
        let s = e.sheet.as_mut().ok_or("no sheet")?;
        // Innkeeper staples first — no faction required (C11).
        if let Some(price) = crate::economy::staple_price(item) {
            if s.gold < price {
                return Err(format!("You need {price} gold."));
            }
            if s.inventory.len() >= inv_cap(s) {
                return Err("Your bags are full.".into());
            }
            s.gold -= price;
            s.inventory.push(item.to_string());
            self.gold_sunk += price as u64;
            return Ok(format!("Purchased {item} for {price}g."));
        }
        let Some(faction) = s.faction.clone() else {
            return Err("Choose a lineage before trading with the Quartermaster.".into());
        };
        let Some((_, gold, need)) = QM_WARES.iter().find(|(i, _, _)| *i == item) else {
            return Err("The Quartermaster does not stock that.".into());
        };
        let rep = s.reputation.get(&faction).copied().unwrap_or(0);
        if !rank_at_least(rep, need) {
            return Err(format!("Requires {need} standing with the {faction}s."));
        }
        if s.gold < *gold {
            return Err(format!("You need {gold} gold."));
        }
        if s.inventory.len() >= inv_cap(s) {
            return Err("Your bags are full.".into());
        }
        s.gold -= *gold;
        s.inventory.push(item.to_string());
        self.gold_sunk += *gold as u64;
        Ok(format!("Purchased {item} for {gold}g."))
    }

    // ─── Developer commands (C14) — all callers are is_dev-gated ─────────

    pub fn dev_teleport(&mut self, act: Act, id: EntityId, x: f32, y: f32) -> String {
        let b = antediluvia_protocol::WORLD_BOUNDS - 10.0;
        let found = match self.entity_mut(act, id) {
            Some(e) => {
                e.pos = Vec2::new(x.clamp(-b, b), y.clamp(-b, b));
                true
            }
            None => false,
        };
        if found {
            // A teleport can jump many cells at once — refile immediately so
            // the AoI grid never lags the entity's real position.
            if let Some(z) = self.zones.get_mut(&act) {
                z.refile_entity(id);
            }
            format!("[dev] teleported to ({x:.0},{y:.0})")
        } else {
            "[dev] teleport failed: entity not in act".into()
        }
    }

    pub fn dev_give(&mut self, act: Act, id: EntityId, item: &str, n: u32) -> String {
        if let Some(s) = self.sheet_mut(act, id) {
            let cap = inv_cap(s);
            for _ in 0..n {
                if s.inventory.len() >= cap { break; }
                s.inventory.push(item.to_string());
            }
        }
        format!("[dev] gave {n}x {item}")
    }

    pub fn dev_set_level(&mut self, act: Act, id: EntityId, level: u32) -> String {
        if let Some(e) = self.entity_mut(act, id) {
            if let Some(s) = e.sheet.as_mut() {
                let level = level.clamp(1, 60);
                let (hp, mana) = s.class.map(class_base).unwrap_or((0, 0));
                s.level = level;
                s.xp = 0;
                s.max_xp = 100 * level;
                s.max_health = 100 + hp + (level as i32 - 1) * 15;
                s.health = s.max_health;
                s.max_mana = 50 + mana + (level as i32 - 1) * 8;
                s.mana = s.max_mana;
                s.stamina = s.max_stamina;
                // Keep the live entity in step with the sheet.
                e.max_health = s.max_health;
                e.health = s.max_health;
            }
        }
        format!("[dev] level set to {level}")
    }

    pub fn dev_heal(&mut self, act: Act, id: EntityId) -> String {
        if let Some(e) = self.entity_mut(act, id) {
            if let Some(s) = e.sheet.as_mut() {
                s.health = s.max_health;
                s.mana = s.max_mana;
                s.stamina = s.max_stamina;
            }
            e.health = e.max_health;
        }
        "[dev] healed".into()
    }

    pub fn dev_spawn_mob(&mut self, act: Act, id: EntityId, tag: &str) -> String {
        let Some(pos) = self.player_pos(act, id) else { return "[dev] no player".into() };
        let mid = self.alloc_id();
        if let Some(z) = self.zones.get_mut(&act) {
            z.insert_entity(mid, make_enemy(mid, pos + Vec2::new(80.0, 0.0), tag, act));
        }
        format!("[dev] spawned {tag}")
    }

    pub fn dev_kill_target(&mut self, act: Act, id: EntityId) -> String {
        let Some(pos) = self.player_pos(act, id) else { return "[dev] no player".into() };
        let target = self.zones[&act].entities.values()
            .filter(|e| e.kind == EntityKind::Enemy && e.health > 0)
            .min_by(|a, b| a.pos.distance(pos).total_cmp(&b.pos.distance(pos)))
            .map(|e| e.id);
        match target {
            Some(t) => {
                if let Some(e) = self.entity_mut(act, t) {
                    e.health = 0;
                }
                "[dev] target killed".into()
            }
            None => "[dev] nothing to kill".into(),
        }
    }

    pub fn dev_godmode(&mut self, act: Act, id: EntityId) -> String {
        if let Some(e) = self.entity_mut(act, id) {
            e.godmode = !e.godmode;
            format!("[dev] godmode {}", if e.godmode { "ON" } else { "off" })
        } else {
            "[dev] no player".into()
        }
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

    /// Party membership by OWNER conn id (P1) — ephemeral, not persisted.
    pub fn set_party(&mut self, owner: u64, party: Option<u64>) {
        match party {
            Some(pid) => { self.party_of.insert(owner, pid); }
            None => { self.party_of.remove(&owner); }
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
    /// All entities of a zone serialized once as (pos, JSON fragment) —
    /// the C15 shared-snapshot path.
    pub fn zone_serialized(&self, act: Act) -> (u64, Vec<(Vec2, String)>) {
        let z = &self.zones[&act];
        let frags = z
            .entities
            .values()
            .map(|e| (e.pos, serde_json::to_string(&e.to_state()).unwrap_or_default()))
            .collect();
        (z.tick, frags)
    }

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

    /// Per-client visibility set: ids of every entity within `radius` of
    /// `player` in `act`. This is the grid-accelerated replacement for the
    /// old per-client scan of all zone entities — only the cells intersecting
    /// the AoI circle are visited, then an exact distance test is applied.
    /// The player's own entity is always included (it sits at the circle's
    /// center). Unknown player → empty.
    ///
    /// The server will call this once per client per snapshot; the wire
    /// protocol and snapshot format are unchanged in this milestone.
    pub fn visible_entities(&self, act: Act, player: EntityId, radius: f32) -> Vec<EntityId> {
        let Some(z) = self.zones.get(&act) else {
            return Vec::new();
        };
        let Some(pe) = z.entities.get(&player) else {
            return Vec::new();
        };
        let center = pe.pos;
        let radius = radius.max(0.0);
        let r2 = radius * radius;
        let mut out = Vec::new();
        for c in aoi::cells_in_radius(center, radius) {
            for id in z.grid.cell_entities(c) {
                // Filed ids are always live, but look the entity up anyway:
                // the cell only guarantees the rectangle intersects the
                // circle, so the exact distance test still applies.
                if let Some(e) = z.entities.get(id) {
                    if e.pos.distance_squared(center) <= r2 {
                        out.push(*id);
                    }
                }
            }
        }
        out
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
        Act::Eden => &[("serpent", 6), ("cainite", 8), ("elemental", 5), ("ember_wisp", 5), ("azazel_cultist", 6), ("sethite_zealot", 6)],
        Act::Hermon => &[("watcher", 10), ("cultist", 6), ("chasm_fiend", 8), ("caravan_wagon", 3), ("oathstone", 1), ("stargazer", 1), ("crystal_lens", 3)],
        Act::Nephilim => &[("giant", 14), ("blood_drinker", 8), ("weapon_cache", 4), ("forge_giant", 1), ("azazel_herald", 1)],
        Act::Enoch => &[("shade", 12), ("citadel_guard", 6), ("furnace_regulator", 4), ("enchanter_smith", 6), ("sorcerer", 8), ("dire_wolf", 16), ("swift_claw", 1), ("enchanted_bellows", 3), ("blood_smith", 6)],
        Act::Flood => &[("leviathan", 18), ("geyser", 5), ("nephilim_raider", 9), ("drowned_beast", 10), ("scroll_crate", 5)],
    }
}

/// Stationary destructible "enemies" (quest objects): they never move,
/// aggro, or strike back.
const OBJECT_TAGS: &[&str] = &[
    "oathstone", "caravan_wagon", "weapon_cache", "furnace_regulator", "geyser", "scroll_crate",
    "enchanted_bellows", "crystal_lens",
];

fn make_enemy(id: EntityId, pos: Vec2, tag: &str, act: Act) -> Entity {
    let mut e = make_enemy_raw(id, pos, tag, act);
    // Starting-area mercy (Eden): anything near the inn road or the Gate of
    // Eden fights at half strength so fresh exiles can find their feet.
    if act == Act::Eden
        && (pos.distance(Vec2::ZERO) < 1500.0 || pos.distance(GATE_OF_EDEN) < 1200.0)
        && !tag.ends_with("_alpha")
    {
        e.max_health = (e.max_health / 2).max(20);
        e.health = e.max_health;
        e.damage = (e.damage / 2).max(2);
        e.aggro_range *= 0.7;
    }
    e
}

fn make_enemy_raw(id: EntityId, pos: Vec2, tag: &str, act: Act) -> Entity {
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
    // Forbidden Arts pillar elites (C08).
    if tag == "forge_giant" {
        let mut e = make_enemy(id, pos, "giant", act);
        e.tag = Some(tag.to_string());
        e.name = Some("The Smithing Giant".into());
        e.max_health = 1200;
        e.health = 1200;
        e.damage = 45;
        e.xp_value = 900;
        e.aggro_range = 300.0;
        return e;
    }
    if tag == "azazel_herald" {
        let mut e = make_enemy(id, pos, "giant", act);
        e.tag = Some(tag.to_string());
        e.name = Some("Azazel's Herald".into());
        e.max_health = 1600;
        e.health = 1600;
        e.damage = 55;
        e.xp_value = 1500;
        e.aggro_range = 320.0;
        return e;
    }
    // Cave mini-bosses (C09): "<resource>_dweller" — alpha-grade guardian
    // of a cave pocket; drops its resource as a guaranteed crafting rare.
    if let Some(res) = tag.strip_suffix("_dweller") {
        let base = act_spawn_table(act)[0].0;
        let mut e = make_enemy(id, pos, base, act);
        e.tag = Some(tag.to_string());
        let pretty = res.split('_').map(|w| {
            let mut c = w.chars();
            match c.next() { Some(f) => f.to_uppercase().collect::<String>() + c.as_str(), None => String::new() }
        }).collect::<Vec<_>>().join(" ");
        e.name = Some(format!("Dweller of the {pretty}"));
        e.max_health *= 4;
        e.health = e.max_health;
        e.damage *= 2;
        e.xp_value *= 5;
        e.patrol_radius = 120.0;
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
        cell: aoi::cell_of(pos.x, pos.y) as u32,
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
            casting: None,
            auras: Vec::new(),
            regen_accum: 0.0,
            vitals_sent: (0, 0, 0),
        duel_with: None,
        rest_accum: 0.0,
        mounted: false,
        combat_timer: 0.0,
        godmode: false,
        forced_pvp: false,
        forced_pvp_timer: 0.0,
        enrage_timer: 0.0,
        mount_hits_left: 0,
        owner: None,
        sheet: None,
    }
}

fn make_npc(id: EntityId, pos: Vec2, name: &str) -> Entity {
    Entity {
        id,
        kind: EntityKind::Npc,
        pos,
        cell: aoi::cell_of(pos.x, pos.y) as u32,
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
            casting: None,
            auras: Vec::new(),
            regen_accum: 0.0,
            vitals_sent: (0, 0, 0),
        duel_with: None,
        rest_accum: 0.0,
        mounted: false,
        combat_timer: 0.0,
        godmode: false,
        forced_pvp: false,
        forced_pvp_timer: 0.0,
        enrage_timer: 0.0,
        mount_hits_left: 0,
        owner: None,
        sheet: None,
    }
}

fn make_wildlife(id: EntityId, pos: Vec2, tag: &str) -> Entity {
    Entity {
        id,
        kind: EntityKind::Wildlife,
        pos,
        cell: aoi::cell_of(pos.x, pos.y) as u32,
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
            casting: None,
            auras: Vec::new(),
            regen_accum: 0.0,
            vitals_sent: (0, 0, 0),
        duel_with: None,
        rest_accum: 0.0,
        mounted: false,
        combat_timer: 0.0,
        godmode: false,
        forced_pvp: false,
        forced_pvp_timer: 0.0,
        enrage_timer: 0.0,
        mount_hits_left: 0,
        owner: None,
        sheet: None,
    }
}

fn make_resource(id: EntityId, pos: Vec2, tag: &str) -> Entity {
    Entity {
        id,
        kind: EntityKind::Resource,
        pos,
        cell: aoi::cell_of(pos.x, pos.y) as u32,
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
            casting: None,
            auras: Vec::new(),
            regen_accum: 0.0,
            vitals_sent: (0, 0, 0),
        duel_with: None,
        rest_accum: 0.0,
        mounted: false,
        combat_timer: 0.0,
        godmode: false,
        forced_pvp: false,
        forced_pvp_timer: 0.0,
        enrage_timer: 0.0,
        mount_hits_left: 0,
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
#[path = "ability_tests.rs"]
mod ability_tests;

#[cfg(test)]
mod tests {
    use super::*;

    /// v0.5.0: /sethome only works at an inn, and sets the home act.
    #[test]
    fn set_home_requires_inn() {
        let mut w = World::new(42);
        let entry = w.zones[&Act::Eden].entry;
        let mut a = new_character("Homebody");
        a.x = entry.x + 5000.0; // far from the inn
        a.y = entry.y;
        let pa = w.spawn_player(1, a);
        assert!(w.set_home(Act::Eden, pa).is_err());
        assert!(w.sheet_of(Act::Eden, pa).unwrap().home_act.is_none());
        // Move to the inn and retry.
        if let Some(e) = w.zones.get_mut(&Act::Eden).and_then(|z| z.entities.get_mut(&pa)) {
            e.pos = entry;
        }
        assert!(w.set_home(Act::Eden, pa).is_ok());
        assert_eq!(w.sheet_of(Act::Eden, pa).unwrap().home_act, Some(Act::Eden));
    }

    /// v0.5.0: players collide with mobs (no walking through), but still
    /// pass through other players.
    #[test]
    fn players_collide_with_mobs_not_players() {
        let mut w = World::new(42);
        let (foe_pos, _fid) = {
            let z = &w.zones[&Act::Eden];
            let e = z.entities.values().find(|e| e.kind == EntityKind::Enemy).unwrap();
            (e.pos, e.id)
        };
        let mut a = new_character("Runner");
        a.x = foe_pos.x - 30.0;
        a.y = foe_pos.y;
        let pa = w.spawn_player(1, a);
        // Run straight at/through the mob for 3 seconds.
        for _ in 0..(TICK_HZ * 3) {
            w.set_intent(Act::Eden, pa, Vec2::X);
            w.step();
        }
        let ppos = w.zones[&Act::Eden].entities[&pa].pos;
        // The mob may have moved (aggro), so check against its live position.
        let foe_now = w.zones[&Act::Eden]
            .entities
            .values()
            .filter(|e| e.kind == EntityKind::Enemy)
            .map(|e| e.pos.distance(ppos))
            .fold(f32::INFINITY, f32::min);
        assert!(foe_now >= 8.9, "player inside a mob: nearest {foe_now}");

        // Players still pass through each other — verify in mob-free space
        // (near the inn, offset away from its NPCs).
        let entry = w.zones[&Act::Eden].entry;
        let spot = entry + Vec2::new(90.0, 40.0);
        let mut c1 = new_character("GhostA");
        c1.x = spot.x;
        c1.y = spot.y;
        let mut c2 = new_character("GhostB");
        c2.x = spot.x;
        c2.y = spot.y;
        let p1 = w.spawn_player(3, c1);
        let p2 = w.spawn_player(4, c2);
        w.step();
        let d = w.zones[&Act::Eden].entities[&p1]
            .pos
            .distance(w.zones[&Act::Eden].entities[&p2].pos);
        assert!(d < 8.0, "players should overlap freely, got {d}");
    }

    /// P2: bank vault round-trips items and gold at the inn.
    #[test]
    fn bank_round_trip() {
        let mut w = World::new(42);
        let entry = w.zones[&Act::Eden].entry;
        let mut a = new_character("Banker");
        a.x = entry.x;
        a.y = entry.y;
        a.gold = 50;
        let pa = w.spawn_player(1, a);
        assert!(w.bank_item(Act::Eden, pa, "hearthstone", true).is_ok());
        assert!(w.sheet_of(Act::Eden, pa).unwrap().bank.contains(&"hearthstone".to_string()));
        assert!(w.bank_item(Act::Eden, pa, "hearthstone", false).is_ok());
        assert!(w.bank_gold(Act::Eden, pa, 30).is_ok());
        let s = w.sheet_of(Act::Eden, pa).unwrap();
        assert_eq!((s.gold, s.bank_gold), (20, 30));
        assert!(w.bank_gold(Act::Eden, pa, -30).is_ok());
        assert_eq!(w.sheet_of(Act::Eden, pa).unwrap().gold, 50);
        // Not at the inn → refused.
        let mut b = new_character("Roamer");
        b.x = entry.x + 5000.0;
        b.y = entry.y;
        let pb = w.spawn_player(2, b);
        assert!(w.bank_item(Act::Eden, pb, "hearthstone", true).is_err());
    }

    /// P4: direct trade hands items and gold to a nearby player only.
    #[test]
    fn trade_give_item_and_gold() {
        let mut w = World::new(42);
        let entry = w.zones[&Act::Eden].entry;
        let mut a = new_character("Giver");
        a.x = entry.x;
        a.y = entry.y;
        a.gold = 40;
        let mut b = new_character("Taker");
        b.x = entry.x + 10.0;
        b.y = entry.y;
        let pa = w.spawn_player(1, a);
        let _pb = w.spawn_player(2, b);
        let (gm, rm, rowner) = w
            .trade_give(Act::Eden, pa, "Taker", Some("hearthstone"), 15)
            .expect("trade should succeed");
        assert_eq!(rowner, 2);
        assert!(gm.contains("hearthstone") && rm.contains("Giver"));
        let sb = w
            .zones[&Act::Eden]
            .entities
            .values()
            .find(|e| e.sheet.as_ref().map(|s| s.name == "Taker") == Some(true))
            .and_then(|e| e.sheet.clone())
            .unwrap();
        assert_eq!(sb.gold, 15);
        assert_eq!(sb.inventory.iter().filter(|i| *i == "hearthstone").count(), 2);
        let sa = w.sheet_of(Act::Eden, pa).unwrap();
        assert_eq!(sa.gold, 25);
        assert!(!sa.inventory.contains(&"hearthstone".to_string()));
        // Too far → refused.
        assert!(w.trade_give(Act::Eden, pa, "Nobody", None, 5).is_err());
    }

    /// P1: party members near a kill split the XP and both get quest credit;
    /// loot goes only to the killer.
    #[test]
    fn party_shares_xp_and_quest_credit() {
        let mut w = World::new(42);
        // Find an enemy in Eden to anchor the fight.
        let foe_pos = {
            let z = &w.zones[&Act::Eden];
            let e = z
                .entities
                .values()
                // beta-server-track: stay clear of caves - a Dweller kill
                // hands its rare to the whole party by design (P5), which
                // is not what this test measures.
                .find(|e| e.kind == EntityKind::Enemy
                    && crate::caves::caves_for_act(Act::Eden).all(|c| Vec2::new(c.x, c.y).distance(e.pos) > 600.0))
                .expect("eden has enemies");
            e.pos
        };
        let mut a = new_character("Slayer");
        a.x = foe_pos.x - 12.0;
        a.y = foe_pos.y;
        // Collision (v0.5.0) keeps the fight at contact range, so the enemy
        // lands real hits now — give the attacker the durability to win.
        a.max_health = 5000;
        a.health = 5000;
        let mut b = new_character("Buddy");
        b.x = foe_pos.x - 60.0;
        b.y = foe_pos.y;
        // beta-server-track: this test was flaky (about 1 run in 7) for two
        // reasons, both fixed here. The buddy must survive the fight (a dead
        // member gets no share), and neither character may pick up POI/cave
        // discovery XP mid-fight, which is not kill XP and is not shared.
        b.max_health = 5000;
        b.health = 5000;
        let seen: Vec<String> = crate::pois::all_pois().iter().map(|p| p.name.clone())
            .chain(crate::caves::all_caves().iter().map(|c| c.name.clone())).collect();
        a.discovered = seen.clone();
        b.discovered = seen;
        let pa = w.spawn_player(1, a);
        let pb = w.spawn_player(2, b);
        w.set_party(1, Some(99));
        w.set_party(2, Some(99));

        // Hammer attacks until the first kill lands (bounded). The attacker
        // targets the nearest hostile, which is not necessarily the sampled
        // foe (HashMap order varies per run), so the assertions below are
        // relative to XP actually awarded — asserting against the sampled
        // foe's xp_value made this test flaky.
        let xa0 = w.player_sheet(Act::Eden, pa).unwrap().xp;
        let xb0 = w.player_sheet(Act::Eden, pb).unwrap().xp;
        for _ in 0..400 {
            w.queue_attack(Act::Eden, pa);
            w.step();
            if w.player_sheet(Act::Eden, pa).unwrap().xp > xa0 {
                break;
            }
        }
        let sa = w.player_sheet(Act::Eden, pa).unwrap();
        let sb = w.player_sheet(Act::Eden, pb).unwrap();
        assert!(sa.xp > xa0, "killer gained xp");
        assert!(sb.xp > xb0, "buddy gained xp");
        assert_eq!(sa.xp - xa0, sb.xp - xb0, "party split the kill XP evenly");
        // (No strict sum check — rested bonus can top up either share.)
        // Loot went only to the killer: buddy keeps just the starter kit.
        assert!(
            sb.inventory.iter().all(|i| i == "hearthstone"),
            "buddy looted: {:?}",
            sb.inventory
        );
    }

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
                .find(|e| e.kind == EntityKind::Resource
                    && matches!(e.tag.as_deref(), Some("tree") | Some("rock")))
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

    // ─── AoI grid (1k-player architecture, milestone 1) ─────────────────────

    /// Scatter `n` extra enemies uniformly over the map so perf tests have a
    /// crowded zone. Deterministic (LCG) — no extra dev-dependencies.
    fn crowd_zone(w: &mut World, act: Act, n: usize) {
        let mut s: u64 = 0x9E3779B97F4A7C15;
        let mut rnd = || {
            s = s
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            ((s >> 32) as f32) / (u32::MAX as f32) // [0, 1): high 32 bits
        };
        // Allocate ids before borrowing the zone mutably.
        let ids: Vec<EntityId> = (0..n).map(|_| w.alloc_id()).collect();
        let z = w.zones.get_mut(&act).unwrap();
        for id in ids {
            let pos = Vec2::new(
                (rnd() * 2.0 - 1.0) * (WORLD_BOUNDS - 20.0),
                (rnd() * 2.0 - 1.0) * (WORLD_BOUNDS - 20.0),
            );
            z.insert_entity(id, make_enemy(id, pos, "serpent", act));
        }
    }

    /// Park `n` stationary resource nodes in the far corner of the map
    /// (around (-3000, -3000)), far outside a 1400 AoI centered at the
    /// origin. Used to show query cost follows local density, not the zone
    /// total. Deterministic (LCG).
    fn crowd_far_corner(w: &mut World, act: Act, n: usize) {
        let mut s: u64 = 0x123456789ABCDEF;
        let mut rnd = || {
            s = s
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            ((s >> 32) as f32) / (u32::MAX as f32) // [0, 1)
        };
        let ids: Vec<EntityId> = (0..n).map(|_| w.alloc_id()).collect();
        let z = w.zones.get_mut(&act).unwrap();
        for id in ids {
            let pos = Vec2::new(-3000.0 + rnd() * 600.0, -3000.0 + rnd() * 600.0);
            z.insert_entity(id, make_resource(id, pos, "ore_copper"));
        }
    }

    /// Time one client's visibility query two ways: the AoI grid
    /// (`visible_entities`) and a fair brute-force baseline (scan every
    /// entity, exact distance test, collect ids — no serialization, which
    /// would only flatter the grid). Returns (grid_us, scan_us) per query.
    fn time_queries(w: &World, act: Act, p: EntityId, radius: f32) -> (f64, f64) {
        let center = w.zones[&act].entities[&p].pos;
        let r2 = radius * radius;
        let iters = 200;
        let t0 = std::time::Instant::now();
        for _ in 0..iters {
            let _ = w.visible_entities(act, p, radius);
        }
        let grid_us = t0.elapsed().as_micros() as f64 / iters as f64;
        let t0 = std::time::Instant::now();
        for _ in 0..iters {
            let z = &w.zones[&act];
            let mut ids = Vec::new();
            for e in z.entities.values() {
                if e.pos.distance_squared(center) <= r2 {
                    ids.push(e.id);
                }
            }
            let _ = ids;
        }
        let scan_us = t0.elapsed().as_micros() as f64 / iters as f64;
        (grid_us, scan_us)
    }

    #[test]
    fn aoi_boot_grid_is_consistent() {
        let w = World::new(42);
        for act in Act::ALL {
            w.zones[&act].check_grid();
        }
    }

    #[test]
    fn aoi_visibility_neighbor_cells() {
        let mut w = World::new(42);
        // Player at the center of cell (7,7).
        let p1 = spawn_at(&mut w, 1, "Watcher", 240.0, 240.0);
        // Neighbor cell (8,7), 400 units east — inside a 600 AoI.
        let p2 = spawn_at(&mut w, 2, "Neighbor", 640.0, 240.0);
        // Two cells east (cell 9), 1200 units away — outside the 600 AoI.
        let p3 = spawn_at(&mut w, 3, "FarAway", 1440.0, 240.0);
        // Three cells east (cell 10), 1536 units away — far outside.
        let p4 = spawn_at(&mut w, 4, "FarFarAway", 1776.0, 240.0);
        w.step(); // settle the grid
        w.zones[&Act::Eden].check_grid();

        let vis = w.visible_entities(Act::Eden, p1, 600.0);
        assert!(vis.contains(&p1), "player should always see itself");
        assert!(
            vis.contains(&p2),
            "neighbor-cell entity within radius must be visible"
        );
        assert!(
            !vis.contains(&p3),
            "entity 2 cells away must not be visible"
        );
        assert!(
            !vis.contains(&p4),
            "entity 3 cells away must not be visible"
        );

        // The grid query must agree exactly with the old brute-force scan.
        let center = w.zones[&Act::Eden].entities[&p1].pos;
        let (tick, brute) = w.zone_snapshot_around(Act::Eden, center, 600.0);
        let mut a: Vec<EntityId> = brute.iter().map(|e| e.id).collect();
        let mut b = vis.clone();
        a.sort_unstable();
        b.sort_unstable();
        assert_eq!(
            a, b,
            "grid visibility disagrees with brute force (tick {tick})"
        );

        // A 1400 AoI (the default client radius) sees the far entity too.
        let vis = w.visible_entities(Act::Eden, p1, 1400.0);
        assert!(vis.contains(&p3), "1400 AoI should reach 2 cells away");
        // And a 1600 AoI reaches the 3-cells-away entity: the grid does not
        // lose distant entities, it just culls them by radius.
        let vis = w.visible_entities(Act::Eden, p1, 1600.0);
        assert!(vis.contains(&p4), "1600 AoI should reach 3 cells away");
    }

    #[test]
    fn aoi_moving_entity_updates_cells() {
        let mut w = World::new(42);
        let p = spawn_at(&mut w, 1, "Walker", -60.0, 0.0);
        w.step();
        let before = w.zones[&Act::Eden].entities[&p].cell;
        assert_eq!(before as usize, crate::aoi::cell_of(-60.0, 0.0));

        // Drive the entity with a real move intent for 3 seconds; whatever
        // the sim does with it, the filed cell must match its live position.
        for _ in 0..(TICK_HZ * 3) {
            w.set_intent(Act::Eden, p, Vec2::X);
            w.step();
        }
        w.set_intent(Act::Eden, p, Vec2::ZERO);
        w.zones[&Act::Eden].check_grid();
        {
            let e = &w.zones[&Act::Eden].entities[&p];
            assert_eq!(
                e.cell as usize,
                crate::aoi::cell_of(e.pos.x, e.pos.y),
                "filed cell must match position after intent-driven movement"
            );
        }

        // A direct position write across the map (what dev tools and tests
        // do) is picked up by the next tick's refile pass.
        w.zones
            .get_mut(&Act::Eden)
            .unwrap()
            .entities
            .get_mut(&p)
            .unwrap()
            .pos = Vec2::new(-3000.0, 2000.0);
        w.step();
        let after = w.zones[&Act::Eden].entities[&p].cell;
        assert_ne!(before, after, "cross-map move must change the filed cell");
        assert_eq!(after as usize, crate::aoi::cell_of(-3000.0, 2000.0));
        w.zones[&Act::Eden].check_grid();
    }

    #[test]
    fn aoi_teleport_refiles_immediately() {
        let mut w = World::new(42);
        let p = spawn_at(&mut w, 1, "Hopper", -3000.0, -3000.0);
        w.step();
        let from = w.zones[&Act::Eden].entities[&p].cell;
        w.dev_teleport(Act::Eden, p, 3000.0, 3000.0);
        let to = w.zones[&Act::Eden].entities[&p].cell;
        assert_ne!(from, to, "teleport across the map must refile the cell");
        w.zones[&Act::Eden].check_grid();
        // No step() ran since the teleport: the grid is already correct.
        let vis = w.visible_entities(Act::Eden, p, 100.0);
        assert!(vis.contains(&p));
    }

    #[test]
    fn aoi_matches_brute_force_after_chaos() {
        let mut w = World::new(7);
        crowd_zone(&mut w, Act::Eden, 300);
        let p1 = spawn_at(&mut w, 1, "ChaosA", 0.0, 0.0);
        let p2 = spawn_at(&mut w, 2, "ChaosB", -2000.0, 1500.0);
        // Random walk + combat for 5 seconds; kills respawn replacements.
        let mut s: u64 = 12345;
        let mut rnd = || {
            s = s
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            ((s >> 32) as f32) / (u32::MAX as f32)
        };
        for _ in 0..(TICK_HZ * 5) {
            for (p, seed) in [(p1, 0.0), (p2, 2.1)] {
                let a = rnd() * std::f32::consts::TAU + seed;
                w.set_intent(Act::Eden, p, Vec2::new(a.cos(), a.sin()));
            }
            w.step();
        }
        w.zones[&Act::Eden].check_grid();
        for (p, radius) in [(p1, 1400.0), (p2, 700.0)] {
            let center = w.zones[&Act::Eden].entities[&p].pos;
            let mut a: Vec<EntityId> = w
                .zone_snapshot_around(Act::Eden, center, radius)
                .1
                .iter()
                .map(|e| e.id)
                .collect();
            let mut b = w.visible_entities(Act::Eden, p, radius);
            a.sort_unstable();
            b.sort_unstable();
            assert_eq!(
                a, b,
                "grid/brute-force mismatch for player {p} at radius {radius}"
            );
        }
        // Unknown player / unknown act → empty, no panic.
        assert!(w.visible_entities(Act::Eden, 999_999, 1400.0).is_empty());
    }

    #[test]
    fn aoi_perf_sanity() {
        // Part 1: the 20 Hz tick with ~1k AI-driven entities must stay well
        // under the 50 ms budget.
        let mut w = World::new(42);
        crowd_zone(&mut w, Act::Eden, 1000);
        let n = w.zones[&Act::Eden].entities.len();
        assert!(n >= 1000, "expected ~1k entities, got {n}");
        let _bench = spawn_at(&mut w, 1, "Bench", 0.0, 0.0);
        for _ in 0..10 {
            w.step();
        }
        let t0 = std::time::Instant::now();
        let ticks = 60;
        for _ in 0..ticks {
            w.step();
        }
        let tick_us = t0.elapsed().as_micros() as f64 / ticks as f64;
        eprintln!("aoi perf: {n} entities | tick {tick_us:.0}µs (20 Hz budget 50_000µs)");
        assert!(tick_us < 50_000.0, "tick budget blown: {tick_us:.0}µs");

        // Part 2: a visibility query's cost follows local density, not the
        // zone total. Fresh world (uniform scatter, no AI clustering yet).
        let mut w = World::new(7);
        crowd_zone(&mut w, Act::Eden, 1000);
        let p = spawn_at(&mut w, 2, "Bench", 0.0, 0.0);
        let (grid_1k, scan_1k) = time_queries(&w, Act::Eden, p, 1400.0);
        // Park 9000 more entities in the far corner: the player's local
        // density is unchanged, but the zone total grows ~10x.
        crowd_far_corner(&mut w, Act::Eden, 9000);
        let n2 = w.zones[&Act::Eden].entities.len();
        let (grid_10k, scan_10k) = time_queries(&w, Act::Eden, p, 1400.0);
        eprintln!(
            "aoi perf: query scaling | {n} entities: grid {grid_1k:.1}µs scan {scan_1k:.1}µs | \
             {n2} entities: grid {grid_10k:.1}µs scan {scan_10k:.1}µs"
        );
        assert!(
            grid_10k <= grid_1k * 2.0,
            "grid query should not follow the zone total ({grid_1k:.1}µs -> {grid_10k:.1}µs)"
        );
        assert!(
            scan_10k >= scan_1k * 3.0,
            "brute-force scan should follow the zone total ({scan_1k:.1}µs -> {scan_10k:.1}µs)"
        );
    }

    #[test]
    fn class_select_and_cast_damages_target_and_costs_mana() {
        let mut w = World::new(3);
        let epos = {
            // Lowest-id plain serpent, not whichever mob hash order yields.
            let e = w.zones[&Act::Eden]
                .entities
                .values()
                .filter(|e| e.kind == EntityKind::Enemy && e.tag.as_deref() == Some("serpent"))
                .min_by_key(|e| e.id)
                .unwrap();
            e.pos
        };
        let pid = spawn_at(&mut w, 1, "Magus", epos.x - 100.0, epos.y);
        // Clusters (C05) + wander: mobs move during step(), so which enemy is
        // nearest at resolution time isn't stable. Assert on per-enemy HP maps
        // instead of pinning one target.
        let enemy_hp = |w: &World| -> std::collections::HashMap<EntityId, i32> {
            w.zones[&Act::Eden]
                .entities
                .values()
                .filter(|e| e.kind == EntityKind::Enemy)
                .map(|e| (e.id, e.health))
                .collect()
        };
        w.select_class(Act::Eden, pid, Class::Mage).unwrap();
        // Second class pick is rejected.
        assert!(w.select_class(Act::Eden, pid, Class::Warrior).is_err());

        let before = enemy_hp(&w);
        let mana_before = w.player_sheet(Act::Eden, pid).unwrap().mana;
        w.queue_cast(Act::Eden, pid, "firebolt".into());
        w.step();
        let after = enemy_hp(&w);
        let s = w.player_sheet(Act::Eden, pid).unwrap();
        let hurt = before
            .iter()
            .any(|(id, hp)| after.get(id).map_or(true, |h| h < hp));
        assert!(hurt, "firebolt should damage an enemy");
        // A kill that levels the caster refills the pool (seen ~1 run in 80,
        // depending on which mob the hash order picks), so allow for it.
        assert!(
            s.mana < mana_before || s.level > 1,
            "cast should cost mana: {} -> {}",
            mana_before,
            s.mana
        );

        // Immediately recasting fails: GCD + cooldown.
        w.queue_cast(Act::Eden, pid, "firebolt".into());
        w.step();
        let after2 = enemy_hp(&w);
        let hurt2 = after
            .iter()
            .any(|(id, hp)| after2.get(id).map_or(true, |h| h < hp));
        assert!(!hurt2, "cooldown should block the second cast");
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
        // Find a spot with nothing else nearby so the swings can only hit the
        // duel partner (camps land anywhere as the spawn tables grow).
        let clear = {
            let z = &w.zones[&Act::Eden];
            let mut p = Vec2::new(500.0, 500.0);
            'search: for gx in -6..=6 {
                for gy in -6..=6 {
                    let c = Vec2::new(gx as f32 * 500.0, gy as f32 * 500.0);
                    if z.entities.values().all(|e| e.pos.distance(c) > 450.0) {
                        p = c;
                        break 'search;
                    }
                }
            }
            p
        };
        let a = spawn_at(&mut w, 1, "Cain", clear.x, clear.y);
        let b = spawn_at(&mut w, 2, "Abel", clear.x + 40.0, clear.y);
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
        // Cluster cleave may loot 1+ ingots in one fight.
        assert!(s.quests.get("the_first_forges").copied().unwrap_or(0) >= 1);

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
    fn dev_give_xp_levels_up() {
        let mut w = World::new(77);
        let pid = spawn_at(&mut w, 7, "Dev", 0.0, 0.0);
        let before = w.player_sheet(Act::Eden, pid).unwrap().level;
        let text = w.dev_give_xp(Act::Eden, pid, 5000);
        assert!(text.contains("+5000 xp"), "{text}");
        let after = w.player_sheet(Act::Eden, pid).unwrap();
        assert!(after.level > before, "xp grant leveled the player");
    }

    #[test]
    fn dev_quest_complete_pays_rewards_and_marks_done() {
        let mut w = World::new(78);
        let pid = spawn_at(&mut w, 8, "Dev", 0.0, 0.0);
        let text = w.dev_quest_complete(Act::Eden, pid, "serpents_in_the_garden");
        assert!(text.contains("completed"), "{text}");
        let s = w.player_sheet(Act::Eden, pid).unwrap();
        assert!(s.quests_done.iter().any(|q| q == "serpents_in_the_garden"));
        assert!(s.inventory.iter().any(|i| i == "bronze_sword"), "quest item reward");
        // Unknown quest ids are rejected cleanly, not panics.
        let bad = w.dev_quest_complete(Act::Eden, pid, "no_such_quest");
        assert!(bad.contains("unknown quest"), "{bad}");
    }

    #[test]
    fn chained_quest_needs_prerequisite() {
        let mut w = World::new(23);
        // Stand by the Wanderer with its unchained quest already done.
        let pid = spawn_at(&mut w, 3, "Seeker", -70.0, 110.0);
        {
            let z = w.zones.get_mut(&Act::Eden).unwrap();
            let s = z.entities.get_mut(&pid).unwrap().sheet.as_mut().unwrap();
            // Finish the Wanderer's unchained quests so only the chain remains.
            s.quests_done.push("fruit_of_the_thorns".into());
            s.quests_done.push("fa_first_blade".into()); // C08 pillar opener
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

    /// Spawn a tameable bestiary beast next to the player for taming tests.
    fn plant_tameable(w: &mut World, act: Act, pos: Vec2) -> (EntityId, &'static str) {
        let def = crate::mobs::all_mobs()
            .iter()
            .find(|m| m.tameable && crate::mobs::mob_by_tag(&m.tag).is_some())
            .expect("a tameable species exists");
        let id = w.alloc_id();
        let z = w.zones.get_mut(&act).unwrap();
        let mut e = make_enemy(id, pos, &def.tag, act);
        e.state = AiState::Static; // hold still for the test
        e.aggro_range = 0.0;
        e.speed = 0.0;
        z.insert_entity(id, e);
        (id, Box::leak(def.tag.clone().into_boxed_str()))
    }

    #[test]
    fn taming_gates_enrage_and_success() {
        let mut w = World::new(61);
        let pid = spawn_at(&mut w, 7, "Tamer", 0.0, 0.0);
        let (bid, _tag) = plant_tameable(&mut w, Act::Eden, Vec2::new(60.0, 0.0));

        // Gate 1: no lasso.
        {
            let z = w.zones.get_mut(&Act::Eden).unwrap();
            let b = z.entities.get_mut(&bid).unwrap();
            b.health = b.max_health / 5; // below 30%
        }
        assert!(w.tame(Act::Eden, pid, bid).unwrap_err().contains("lasso"));

        // Gate 2: too healthy.
        {
            let z = w.zones.get_mut(&Act::Eden).unwrap();
            z.entities.get_mut(&bid).unwrap().health = z.entities[&bid].max_health;
            let e = z.entities.get_mut(&pid).unwrap();
            e.sheet.as_mut().unwrap().inventory.push("taming_lasso".into());
        }
        assert!(w.tame(Act::Eden, pid, bid).unwrap_err().contains("weaken"));

        // Gate 3: non-tameable target (a legacy serpent).
        let sid = {
            let z = &w.zones[&Act::Eden];
            z.entities.values().find(|e| e.tag.as_deref() == Some("serpent")).unwrap().id
        };
        assert!(w.tame(Act::Eden, pid, sid).is_err());

        // Weakened: repeat attempts until success (each failure enrages and
        // must heal the beast by half its max).
        let mut tamed = false;
        for _ in 0..40 {
            {
                let z = w.zones.get_mut(&Act::Eden).unwrap();
                if let Some(b) = z.entities.get_mut(&bid) {
                    b.health = b.max_health / 5;
                } else {
                    break;
                }
                let e = z.entities.get_mut(&pid).unwrap();
                let s = e.sheet.as_mut().unwrap();
                if !s.inventory.iter().any(|i| i == "taming_lasso") {
                    s.inventory.push("taming_lasso".into());
                }
            }
            match w.tame(Act::Eden, pid, bid) {
                Ok(msg) => {
                    assert!(msg.contains("submits"), "{msg}");
                    tamed = true;
                    break;
                }
                Err(msg) => {
                    assert!(msg.contains("rage"), "{msg}");
                    let z = &w.zones[&Act::Eden];
                    let b = &z.entities[&bid];
                    assert!(b.enrage_timer > 0.0, "failure must enrage");
                    assert!(b.health > b.max_health / 5, "failure must heal the beast");
                }
            }
        }
        assert!(tamed, "40 attempts at 40%+ odds should succeed");
        let s = w.player_sheet(Act::Eden, pid).unwrap();
        let mount = s.inventory.iter().find(|i| i.starts_with("mount:")).unwrap().clone();
        assert!(!w.zones[&Act::Eden].entities.contains_key(&bid), "beast despawns");

        // Equip + ride the tamed beast.
        w.equip(Act::Eden, pid, &mount).unwrap();
        let msg = w.toggle_mount(Act::Eden, pid).unwrap();
        assert!(msg.contains("You mount"), "{msg}");
        assert!(w.zones[&Act::Eden].entities[&pid].to_state().mount_species.is_some());
    }


    #[test]
    fn mount_tiers_map_keywords() {
        assert_eq!(mount_tier("iron_scaled_dire_wolf_alpha").0, 1.7);
        assert_eq!(mount_tier("starving_smilodon").0, 1.7);
        let (speed, bags, _) = mount_tier("dire_mastodon_goliath");
        assert_eq!((speed, bags), (1.35, 8));
        let (speed, _, hits) = mount_tier("primordial_cave_bear");
        assert_eq!((speed, hits), (1.5, 3));
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
        // Chest piece adds max health (swapping out the starting
        // animal_skins, +15, for the hide_vest, +30).
        let hp_before = s.max_health;
        w.equip(Act::Eden, pid, "hide_vest").unwrap();
        let s = w.player_sheet(Act::Eden, pid).unwrap();
        assert_eq!(s.max_health, hp_before + 15);
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
        let prof = if tag == "rock" || tag.starts_with("ore_") { "mining" } else { "woodcutting" };
        let pid = spawn_at(&mut w, 1, "Gatherer", res_pos.x - 12.0, res_pos.y);
        w.queue_attack(Act::Eden, pid);
        w.step();
        let s = w.player_sheet(Act::Eden, pid).unwrap();
        // Groves cluster (C05): one swing can fell several nodes.
        assert!(s.professions.get(prof).copied().unwrap_or(0) >= 1);
    }

    /// C09: cave ore nodes yield the archive resource at double count and
    /// level mining; the cave's dweller mini-boss drops the crafting rare.
    #[test]
    fn cave_ores_and_dweller_drop() {
        let mut w = World::new(90);
        let cave = crate::caves::caves_for_act(Act::Eden).next().unwrap();
        let res = cave.resources[0].clone();
        let (node_pos, node_tag) = {
            let z = &w.zones[&Act::Eden];
            let e = z.entities.values()
                .find(|e| e.kind == EntityKind::Resource
                    && e.tag.as_deref().is_some_and(|t| t.starts_with("ore_")))
                .unwrap();
            (e.pos, e.tag.clone().unwrap())
        };
        let ore = node_tag.strip_prefix("ore_").unwrap().to_string();
        let pid = spawn_at(&mut w, 1, "Miner", node_pos.x - 12.0, node_pos.y);
        w.queue_attack(Act::Eden, pid);
        w.step();
        let s = w.player_sheet(Act::Eden, pid).unwrap();
        let got = s.inventory.iter().filter(|i| **i == ore).count();
        assert!(got >= 2, "double yield expected, got {got} of {ore} in {:?}", s.inventory);
        assert!(s.professions.get("mining").copied().unwrap_or(0) >= 1, "ore mining levels mining");

        // Dweller drop: kill the mini-boss directly.
        let boss_tag = format!("{}_dweller", res);
        let (bid, bpos) = {
            let z = &w.zones[&Act::Eden];
            let e = z.entities.values()
                .find(|e| e.tag.as_deref() == Some(boss_tag.as_str()))
                .unwrap();
            (e.id, e.pos)
        };
        let hunter = spawn_at(&mut w, 60, "Slayer", bpos.x - 12.0, bpos.y);
        {
            let z = w.zones.get_mut(&Act::Eden).unwrap();
            z.entities.get_mut(&bid).unwrap().health = 1;
        }
        for _ in 0..40 {
            w.queue_attack(Act::Eden, hunter);
            w.step();
            let s = w.player_sheet(Act::Eden, hunter).unwrap();
            if s.inventory.iter().any(|i| *i == res) {
                return;
            }
        }
        panic!("dweller never dropped {res}");
    }

    /// C09: crafting the orichalcum blade consumes ores and needs mining 10.
    #[test]
    fn cave_recipe_crafts() {
        let mut w = World::new(91);
        let pid = spawn_at(&mut w, 5, "Smith", 0.0, 0.0);
        {
            let z = w.zones.get_mut(&Act::Eden).unwrap();
            let s = z.entities.get_mut(&pid).unwrap().sheet.as_mut().unwrap();
            s.inventory.extend(["orichalcum_ore".to_string(), "orichalcum_ore".into(), "orichalcum_ore".into(), "wood".into()]);
            s.professions.insert("mining".into(), 10);
        }
        w.craft(Act::Eden, pid, "orichalcum_blade").unwrap();
        let s = w.player_sheet(Act::Eden, pid).unwrap();
        assert!(s.inventory.iter().any(|i| i == "orichalcum_blade"));
        assert!(!s.inventory.iter().any(|i| i == "orichalcum_ore"));
    }


    /// C11: innkeeper sells staples for gold alone and buys drops at 80%.
    #[test]
    fn innkeeper_sell_and_staples() {
        let mut w = World::new(71);
        let pid = spawn_at(&mut w, 1, "Naamah", 0.0, 0.0);
        {
            let z = w.zones.get_mut(&Act::Eden).unwrap();
            let s = z.entities.get_mut(&pid).unwrap().sheet.as_mut().unwrap();
            s.gold = 10;
            s.inventory.push("thick_hide".into()); // table 25 -> sells for 20
        }
        assert!(w.sell(Act::Eden, pid, "no_such").is_err());
        w.sell(Act::Eden, pid, "thick_hide").unwrap();
        let s = w.player_sheet(Act::Eden, pid).unwrap();
        assert_eq!(s.gold, 30);
        assert!(!s.inventory.iter().any(|i| i == "thick_hide"));
        // Staple purchase needs no faction: bread at 120% of 2 = 2 (floored, min 1).
        let bread = crate::economy::staple_price("bread").unwrap();
        w.buy(Act::Eden, pid, "bread").unwrap();
        let s = w.player_sheet(Act::Eden, pid).unwrap();
        assert_eq!(s.gold, 30 - bread);
        assert!(s.inventory.iter().any(|i| i == "bread"));
        assert_eq!(w.gold_minted, 20);
        assert_eq!(w.gold_sunk, bread as u64);
    }

    /// C16: entering an ENEMY capital force-flags PvP; own capital doesn't;
    /// the flag drops after the linger cooldown once outside.
    #[test]
    fn enemy_capital_zone_forces_pvp_flag() {
        let mut w = World::new(73);
        let pid = spawn_at(&mut w, 1, "Irad", 0.0, 0.0);
        {
            let z = w.zones.get_mut(&Act::Eden).unwrap();
            let s = z.entities.get_mut(&pid).unwrap().sheet.as_mut().unwrap();
            s.faction = Some("sethite".into());
        }
        let flagged = |w: &World, pid| {
            let e = &w.zones[&Act::Eden].entities[&pid];
            e.forced_pvp || e.forced_pvp_timer > 0.0
        };
        // Own capital (sethite, -2400,-2400): no flag.
        w.zones.get_mut(&Act::Eden).unwrap().entities.get_mut(&pid).unwrap().pos =
            Vec2::new(-2400.0, -2400.0);
        w.step();
        assert!(!flagged(&w, pid), "own capital must not flag");
        // Enemy capital (cainite, 2400,2400): flagged.
        w.zones.get_mut(&Act::Eden).unwrap().entities.get_mut(&pid).unwrap().pos =
            Vec2::new(2400.0, 2400.0);
        w.step();
        assert!(flagged(&w, pid), "enemy capital flags");
        // Leave: linger, then drop.
        w.zones.get_mut(&Act::Eden).unwrap().entities.get_mut(&pid).unwrap().pos = Vec2::ZERO;
        w.step();
        assert!(flagged(&w, pid), "flag lingers right after leaving");
        for _ in 0..(TICK_HZ as usize * 21) {
            w.step();
        }
        assert!(!flagged(&w, pid), "flag drops after linger");
    }

    /// Lineage is optional until level 10, so most early characters have
    /// `faction: None`. They must still flag inside a capital — otherwise an
    /// undecided player roams every enemy city permanently immune.
    #[test]
    fn undecided_lineage_still_flags_in_any_capital() {
        let mut w = World::new(74);
        let pid = spawn_at(&mut w, 1, "Nomad", 0.0, 0.0);
        {
            let z = w.zones.get_mut(&Act::Eden).unwrap();
            let s = z.entities.get_mut(&pid).unwrap().sheet.as_mut().unwrap();
            s.faction = None; // never picked a lineage
        }
        let flagged = |w: &World, pid| {
            let e = &w.zones[&Act::Eden].entities[&pid];
            e.forced_pvp || e.forced_pvp_timer > 0.0
        };
        // Neutral ground: no flag.
        w.step();
        assert!(!flagged(&w, pid), "neutral ground must not flag");
        // Both capitals are hostile to an unaligned outsider.
        for (x, y, what) in [(-2400.0, -2400.0, "sethite"), (2400.0, 2400.0, "cainite")] {
            w.zones.get_mut(&Act::Eden).unwrap().entities.get_mut(&pid).unwrap().pos =
                Vec2::new(x, y);
            w.step();
            assert!(flagged(&w, pid), "undecided player must flag in the {what} capital");
            // Clear the flag before testing the next capital.
            w.zones.get_mut(&Act::Eden).unwrap().entities.get_mut(&pid).unwrap().pos = Vec2::ZERO;
            for _ in 0..(TICK_HZ as usize * 21) {
                w.step();
            }
        }
    }

    /// C15: players never collide — two players can stand on the same spot
    /// and walking "through" another player causes no displacement.
    #[test]
    fn players_walk_through_each_other() {
        let mut w = World::new(72);
        let a = spawn_at(&mut w, 1, "PassA", 1000.0, 1000.0);
        let b = spawn_at(&mut w, 2, "PassB", 1040.0, 1000.0);
        // March A straight through B's position for 2 seconds.
        for _ in 0..(TICK_HZ * 2) {
            w.set_intent(Act::Eden, a, Vec2::new(1.0, 0.0));
            w.step();
        }
        let pa = w.player_pos(Act::Eden, a).unwrap();
        let pb = w.player_pos(Act::Eden, b).unwrap();
        assert!(pa.x > 1040.0, "A passed through B's spot (x={})", pa.x);
        assert_eq!(pb, Vec2::new(1040.0, 1000.0), "B was never displaced");
    }

    /// Kill one enemy whose tag starts with `tag` using an existing player.
    fn kill_one_tagged_with(w: &mut World, pid: EntityId, tag: &str) -> CharacterSheet {
        let (tid, tpos) = {
            let z = &w.zones[&Act::Eden];
            let e = z.entities.values()
                .find(|e| e.kind == EntityKind::Enemy
                    && e.tag.as_deref().is_some_and(|t| t.starts_with(tag)))
                .unwrap();
            (e.id, e.pos)
        };
        {
            let z = w.zones.get_mut(&Act::Eden).unwrap();
            z.entities.get_mut(&tid).unwrap().health = 1;
            let p = z.entities.get_mut(&pid).unwrap();
            p.pos = tpos + Vec2::new(-12.0, 0.0);
            if let Some(s) = p.sheet.as_mut() { s.health = 4000; s.max_health = 4000; }
            p.health = 4000; p.max_health = 4000;
        }
        for _ in 0..30 {
            w.queue_attack(Act::Eden, pid);
            w.step();
            let s = w.player_sheet(Act::Eden, pid).unwrap();
            if !s.reputation.is_empty() || s.faction.is_none() {
                return s;
            }
            // retarget in case the weakened mob wandered or another died first
        }
        w.player_sheet(Act::Eden, pid).unwrap()
    }
}

/// A fresh level-1 character at Eden's entry point.
pub fn new_character(name: &str) -> CharacterSheet {
    new_character_with(name, None, None, [0, 0, 0])
}

/// Builder-driven creation (C13): class/faction/appearance chosen up front.
pub fn new_character_with(
    name: &str,
    class: Option<antediluvia_protocol::Class>,
    faction: Option<String>,
    appearance: [u32; 3],
) -> CharacterSheet {
    let mut equipment = std::collections::BTreeMap::new();
    equipment.insert("chest".to_string(), "animal_skins".to_string());
    // A class picked in the builder gets the same base kit as `select_class`.
    let (hp, mana) = class.map(class_base).unwrap_or((0, 0));
    CharacterSheet {
        name: name.to_string(),
        class,
        faction,
        appearance,
        look: 0,
        inventory: vec!["hearthstone".to_string()],
        equipment,
        act: Act::Eden,
        // New mortals begin at the Gate of Eden (east, on the road), clothed
        // in skins, with the Flaming Boundary at their backs (Gen 3).
        x: GATE_OF_EDEN.x,
        y: GATE_OF_EDEN.y,
        level: 1,
        xp: 0,
        max_xp: 100,
        health: 100 + hp,
        max_health: 100 + hp,
        mana: 50 + mana,
        max_mana: 50 + mana,
        stamina: STAMINA_MAX,
        max_stamina: STAMINA_MAX,
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
        home_act: None,
        bank: Vec::new(),
        bank_gold: 0,
        stable: Vec::new(),
        reputation: std::collections::BTreeMap::new(),
        quests: std::collections::BTreeMap::new(),
        quests_done: Vec::new(),
        durability: Default::default(), // beta-server-track (B06)
    }
}

// ─── beta-server-track hooks (B05-B08) ──────────────────────────────────────
// Thin crate-visible doors onto the private helpers above, so the new
// modules (questgen / durability / travel / social) need no edits inside them.
impl World {
    pub(crate) fn st_complete_quest(e: &mut Entity, q: &crate::quests::QuestDef) -> String {
        Self::grant_quest_completion(e, q)
    }
}

pub(crate) fn st_make_enemy(id: EntityId, pos: Vec2, tag: &str, act: Act) -> Entity {
    make_enemy(id, pos, tag, act)
}
