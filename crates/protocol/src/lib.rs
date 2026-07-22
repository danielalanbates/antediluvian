//! Antediluvia shared wire protocol.
//!
//! Types shared between the authoritative server and any client. The transport
//! is a WebSocket carrying newline-free JSON frames (one `ClientMsg` /
//! `ServerMsg` per WebSocket text message). Binary framing can be layered on
//! later without touching game logic — these enums stay the message contract.

use serde::{Deserialize, Serialize};

/// Half-extent of each act's square playable map, shared by server (spawns,
/// clamping) and client (terrain mesh, decor) so they cannot drift (C05).
pub const WORLD_BOUNDS: f32 = 3600.0;

/// Protocol version. Bump on any breaking change to the enums below; the server
/// rejects a `Login` whose `proto` does not match.
pub const PROTOCOL_VERSION: u32 = 14;

/// A broadcast combat event, for client-side animation of *remote* entities
/// (swings, casts, hits, deaths). Purely cosmetic — carries no game state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EventKind {
    Attack,
    Cast,
    Hit,
    Die,
    LevelUp,
}

/// Playable classes. Chosen once per character (level 1) via `SelectClass`;
/// gates which abilities and talents are available.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Class {
    Warrior,
    Hunter,
    Priest,
    Mage,
}

impl Class {
    pub const ALL: [Class; 4] = [Class::Warrior, Class::Hunter, Class::Priest, Class::Mage];
    pub fn as_str(self) -> &'static str {
        match self {
            Class::Warrior => "warrior",
            Class::Hunter => "hunter",
            Class::Priest => "priest",
            Class::Mage => "mage",
        }
    }
}

/// One auction-house listing as sent to browsing clients.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuctionListing {
    pub id: i64,
    pub seller: String,
    pub item: String,
    pub price: u32,
}

/// A world coordinate. The world is top-down 2D (matching the original game),
/// coordinates in "world units" (~pixels in the legacy client).
pub type Vec2 = glam::Vec2;

/// The five narrative acts of Antediluvia. Each act is an independent zone with
/// its own entity simulation. Players occupy exactly one zone at a time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Act {
    Eden,
    Hermon,
    Nephilim,
    Enoch,
    Flood,
}

impl Act {
    pub const ALL: [Act; 5] = [Act::Eden, Act::Hermon, Act::Nephilim, Act::Enoch, Act::Flood];

    pub fn as_str(self) -> &'static str {
        match self {
            Act::Eden => "eden",
            Act::Hermon => "hermon",
            Act::Nephilim => "nephilim",
            Act::Enoch => "enoch",
            Act::Flood => "flood",
        }
    }
}

/// Server-assigned, process-unique entity id. Stable for the lifetime of the
/// entity within a running server; not persisted.
pub type EntityId = u64;

/// What an entity *is*, for client rendering and combat rules.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EntityKind {
    Player,
    Enemy,
    Wildlife,
    Resource,
    Npc,
}

/// A single entity as broadcast to clients in a snapshot.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntityState {
    pub id: EntityId,
    pub kind: EntityKind,
    pub x: f32,
    pub y: f32,
    /// Facing angle (yaw) in radians.
    pub rot: f32,
    pub health: i32,
    pub max_health: i32,
    /// Sub-type tag: enemy species, wildlife species, resource kind, etc.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tag: Option<String>,
    /// Display name (players, NPCs).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Equipped weapon item id (players) — drives the visible held weapon.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub weapon: Option<String>,
    /// Equipped chest item id (players) — drives armor presentation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chest: Option<String>,
    /// Equipped back item id (players; C10 lineage_mantle cosmetic tint).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub back: Option<String>,
    /// Wearer's lineage (players; C10) — picks the mantle tint color.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub faction: Option<String>,
    /// Builder appearance [body, skin, hair] (players; C13).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub appearance: Option<[u32; 3]>,
    /// Riding a mount (players; C06).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub mounted: bool,
    /// Species tag of the ridden mount, e.g. "starving_smilodon" (C07).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mount_species: Option<String>,
}

/// The persistent, player-owned character sheet.
fn default_wakefulness() -> f32 { 100.0 }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CharacterSheet {
    pub name: String,
    pub act: Act,
    pub x: f32,
    pub y: f32,
    pub level: u32,
    pub xp: u32,
    pub max_xp: u32,
    pub health: i32,
    pub max_health: i32,
    pub mana: i32,
    pub max_mana: i32,
    pub inventory: Vec<String>,
    // ── WoW-style systems (all defaulted so v1 saves still load) ──
    /// Chosen class; `None` until the player picks one.
    #[serde(default)]
    pub class: Option<Class>,
    #[serde(default)]
    pub gold: u32,
    /// Unspent talent points (1 granted per level-up).
    #[serde(default)]
    pub talent_points: u32,
    /// Learned talents: talent id → rank.
    #[serde(default)]
    pub talents: std::collections::BTreeMap<String, u32>,
    /// Profession skill levels: profession id → skill (e.g. "woodcutting": 12).
    #[serde(default)]
    pub professions: std::collections::BTreeMap<String, u32>,
    /// Guild membership, if any.
    #[serde(default)]
    pub guild: Option<String>,
    /// Banked rested XP (accrued at inns; consumed as bonus XP on kills).
    #[serde(default)]
    pub rested_xp: u32,
    /// World-PvP opt-in flag.
    #[serde(default)]
    pub pvp: bool,
    /// Wakefulness (100.0 = fully awake, 0.0 = exhausted). Decreases while logged in, increases while logged out.
    #[serde(default = "default_wakefulness")]
    pub wakefulness: f32,
    /// POI names this character has discovered (C04).
    #[serde(default)]
    pub discovered: Vec<String>,
    /// Mount items parked at the inn stable (C07).
    #[serde(default)]
    pub stable: Vec<String>,
    /// Mortal lineage: "sethite" or "cainite" (C10). Chosen once at level 10;
    /// switching later zeroes all reputation.
    #[serde(default)]
    pub faction: Option<String>,
    /// Faction reputation totals, keyed by faction id (C10).
    #[serde(default)]
    pub reputation: std::collections::BTreeMap<String, i32>,
    /// Appearance indices from the character builder (C13): [body, skin, hair].
    #[serde(default)]
    pub appearance: [u32; 3],
    /// Unix timestamp of last logout (used to calculate sleep/rest).
    #[serde(default)]
    pub last_logout: Option<u64>,
    /// Active quests: quest id → kill progress.
    #[serde(default)]
    pub quests: std::collections::BTreeMap<String, u32>,
    /// Completed quest ids.
    #[serde(default)]
    pub quests_done: Vec<String>,
    /// Equipped gear: slot ("weapon"/"chest") → item id.
    #[serde(default)]
    pub equipment: std::collections::BTreeMap<String, String>,
}

/// First-time character creation payload (C13). Sent inside `Login`; the
/// server refuses world entry for an account with no character otherwise.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CharacterCreate {
    pub name: String,
    pub class: Class,
    /// Optional lineage pre-pick ("sethite"/"cainite"); also choosable at 10.
    #[serde(default)]
    pub faction: Option<String>,
    /// Appearance indices: [body rig, skin tint, hair tint].
    #[serde(default)]
    pub appearance: [u32; 3],
}

/// Developer commands (C14) — server-gated on a dev-account allowlist; the
/// client only requests, the server decides and audits.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "cmd", rename_all = "snake_case")]
pub enum DevCmd {
    Teleport { x: f32, y: f32 },
    GiveItem { item: String, n: u32 },
    SetLevel { level: u32 },
    Heal,
    SpawnMob { tag: String },
    KillTarget,
    Godmode,
    TimeOfDay { t: f32 },
}

// ─── Client → Server ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "t", rename_all = "snake_case")]
pub enum ClientMsg {
    /// First message on a connection. Authenticates an account via Apple ID.
    /// If creating a new account, a character name must be provided.
    Login {
        proto: u32,
        apple_id: String,
        /// Legacy pre-C13 creation path — ignored when `create` is present.
        character_name: Option<String>,
        /// First-time character builder payload (C13).
        #[serde(default)]
        create: Option<CharacterCreate>,
    },
    /// Desired movement direction this tick, as a unit-ish vector. The server
    /// clamps magnitude and applies speed — the client never sets position.
    Move { dx: f32, dy: f32 },
    /// Melee attack toward the current facing.
    Attack,
    /// Travel to another act/zone (spawns at that zone's entry point).
    Travel { act: Act },
    /// Zone-local chat.
    Chat { text: String },
    /// Choose a class (once, while still classless).
    SelectClass { class: Class },
    /// Cast a class ability by id (e.g. "heroic_strike"). Targeting is
    /// server-side: offensive abilities hit what's in front / in range,
    /// heals target self.
    Cast { ability: String },
    /// Spend one talent point on a talent id (e.g. "warrior_power").
    LearnTalent { talent: String },
    /// Toggle the world-PvP flag.
    TogglePvp,
    /// Challenge another player (by name, same zone) to a duel.
    Duel { player: String },
    /// Accept the most recent duel challenge.
    DuelAccept,
    /// Consume a usable inventory item (e.g. "bread").
    UseItem { item: String },
    /// Equip a piece of gear from the inventory (swaps out the slot's current
    /// item, if any).
    Equip { item: String },
    /// Talk to the nearest NPC (quest givers): offers, progress, or turn-in.
    Talk,
    /// Toggle riding the active mount (C06/C07).
    Mount,
    /// Attempt to tame a weakened tameable beast with a lasso (C07).
    Tame { target: EntityId },
    /// Move a mount item from inventory into the inn stable (C07).
    Stable { item: String },
    /// Take a mount item back out of the stable (C07).
    Unstable { item: String },
    /// Align with a mortal lineage: "sethite" | "cainite" (C10, level 10+).
    ChooseFaction { faction: String },
    /// Buy an item from the faction Quartermaster at the inn (C10),
    /// or an innkeeper staple (C11).
    Buy { item: String },
    /// Sell an item to the innkeeper at 80% of table price (C11).
    Sell { item: String },
    /// Developer command (C14) — ignored unless the account is on the
    /// server's dev allowlist.
    Dev { cmd: DevCmd },
    /// Ask for a smaller snapshot area-of-interest, in world units (C15).
    /// Headless bots use this to keep 1,000-connection swarms cheap; the
    /// server clamps to [100, 1400].
    SetAoi { radius: f32 },
    /// Craft a recipe by id (consumes materials; may need profession skill).
    Craft { recipe: String },
    /// Guild management.
    GuildCreate { name: String },
    GuildInvite { player: String },
    GuildAccept,
    GuildLeave,
    /// Guild-wide chat (crosses zones).
    GuildChat { text: String },
    /// Party (P1): invite a same-zone player by name; nearby party members
    /// share kill XP and quest credit. Accept takes the latest invite.
    PartyInvite { player: String },
    PartyAccept,
    PartyLeave,
    /// Auction house (usable near a zone's inn/entry).
    AuctionList { item: String, price: u32 },
    AuctionBuy { id: i64 },
    AuctionBrowse,
    /// Liveness; server replies `Pong`.
    Ping,
}

// ─── Server → Client ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "t", rename_all = "snake_case")]
pub enum ServerMsg {
    /// Login accepted. Carries the loaded character and the entity id the
    /// server assigned to this player.
    Welcome {
        entity_id: EntityId,
        character: CharacterSheet,
        /// Account is on the server's dev allowlist (C14) — client may show
        /// the dev panel. Purely advisory; every DevCmd is re-checked.
        #[serde(default)]
        is_dev: bool,
    },
    /// Login rejected (bad proto version, name taken by a live session, …).
    LoginRejected { reason: String },
    /// Full snapshot of every entity in the player's current zone. Sent each
    /// simulation tick. (Delta compression is a later optimization.)
    Snapshot { act: Act, tick: u64, time_of_day: f32, entities: Vec<EntityState> },
    /// The player's own live stats changed (xp gain, damage, level up).
    Stats { character: CharacterSheet },
    /// A chat line from another player (or the system) in the zone.
    Chat { from: String, text: String },
    /// Server-side notice (level up, death, zone change).
    Notice { text: String },
    /// Current auction-house listings (reply to `AuctionBrowse`).
    Auctions { listings: Vec<AuctionListing> },
    /// Guild roster (on join/create/query).
    GuildInfo { name: String, members: Vec<String> },
    /// Current party roster (empty = left/disbanded) (P1).
    PartyInfo { members: Vec<String> },
    /// A combat event in the player's zone: `src` swung/cast/was hit/died.
    /// Drives remote-entity animations; safe to ignore.
    Event { act: Act, kind: EventKind, src: EntityId, dst: Option<EntityId> },
    Pong,
}
