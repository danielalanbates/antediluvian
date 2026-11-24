//! Antediluvia shared wire protocol.
//!
//! Types shared between the authoritative server and any client. The transport
//! is a WebSocket carrying newline-free JSON frames (one `ClientMsg` /
//! `ServerMsg` per WebSocket text message). World snapshots alone may travel
//! as binary delta frames instead (`wire`, asked for in `Login`); the client
//! turns them back into `ServerMsg::Snapshot`, so these enums stay the
//! message contract.

use serde::{Deserialize, Serialize};

/// Half-extent of each act's square playable map, shared by server (spawns,
/// clamping) and client (terrain mesh, decor) so they cannot drift (C05).
pub const WORLD_BOUNDS: f32 = 3600.0;

/// Protocol version. Bump on any breaking change to the enums below; the server
/// rejects a `Login` whose `proto` does not match.
pub const PROTOCOL_VERSION: u32 = 17;

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
    /// Knife-fighter of the land of Nod (B09).
    Wanderer,
    /// Armored preacher of righteousness (B09).
    Herald,
    /// Keeper of the ground (B09).
    Warden,
    /// Student of the Watchers' forbidden lore (B09).
    Occultist,
}

impl Class {
    pub const ALL: [Class; 8] = [
        Class::Warrior,
        Class::Hunter,
        Class::Priest,
        Class::Mage,
        Class::Wanderer,
        Class::Herald,
        Class::Warden,
        Class::Occultist,
    ];
    pub fn as_str(self) -> &'static str {
        match self {
            Class::Warrior => "warrior",
            Class::Hunter => "hunter",
            Class::Priest => "priest",
            Class::Mage => "mage",
            Class::Wanderer => "wanderer",
            Class::Herald => "herald",
            Class::Warden => "warden",
            Class::Occultist => "occultist",
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
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
    /// Packed character look (players): see `look::Look`. One number, so a
    /// crowd of players costs little in every snapshot.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub look: Option<u64>,
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
    /// Stamina pool (B03): spent by warrior/hunter/wanderer abilities. Old
    /// saves load as 0/0 and are topped up to a full pool on spawn.
    #[serde(default)]
    pub stamina: i32,
    #[serde(default)]
    pub max_stamina: i32,
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
    /// Home inn act (v0.5.0): where the player respawns. Defaults to Eden.
    #[serde(default)]
    pub home_act: Option<Act>,
    /// Bank vault items (P2) — stored at any inn, shared across acts.
    #[serde(default)]
    pub bank: Vec<String>,
    /// Bank vault gold (P2).
    #[serde(default)]
    pub bank_gold: u32,
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
    /// Packed character look (`look::Look`): face, hair, skin, build, height,
    /// markings, eyes, mantle and dye. Clamped by the server on creation.
    #[serde(default)]
    pub look: u64,
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
    // ── beta-server-track (B06) ──
    /// Item wear: item id → durability points remaining. Absent = pristine.
    #[serde(default)]
    pub durability: std::collections::BTreeMap<String, u32>,
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
    /// Packed `look::Look`. The server clamps it; never trusted as sent.
    #[serde(default)]
    pub look: u64,
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
    /// Grant raw XP (beta testing: level at a controlled pace).
    GiveXp { amount: u32 },
    /// Mark a quest done and pay its xp/gold/item rewards (beta testing:
    /// skip quest chains to test later acts). Prereqs are not checked.
    QuestComplete { id: String },
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
        /// Snapshot wire this connection wants (B11): `wire::WIRE_JSON` (0,
        /// the default: text `Snapshot` messages, at most 150 entities) or
        /// `wire::WIRE_BINARY` (binary delta frames, see `wire`).
        #[serde(default)]
        wire: u8,
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
    /// friendly ones land on the caster (or the nearby party).
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
    /// Bank vault at any inn (P2): move an item in/out, or gold (positive
    /// deposits, negative withdraws).
    BankDeposit { item: String },
    BankWithdraw { item: String },
    BankGold { amount: i64 },
    /// Set this inn as your home/respawn point (v0.5.0). Must be at an inn.
    SetHome,
    /// Direct trade (P4): hand an item or gold to a nearby player.
    TradeGive { player: String, item: String },
    TradeGold { player: String, amount: u32 },
    /// Mail (P3): send an item or gold to any character (even offline);
    /// MailCheck at an inn lists and delivers pending mail.
    MailSend { to: String, item: Option<String>, gold: u32 },
    MailCheck,
    /// Auction house (usable near a zone's inn/entry).
    AuctionList { item: String, price: u32 },
    AuctionBuy { id: i64 },
    AuctionBrowse,
    /// Liveness; server replies `Pong`.
    Ping,
    /// Binary wire only (B11): the last frame did not decode or was out of
    /// sequence; send a keyframe. Rate-limited by the server.
    Resync,
    // ── beta-server-track: B05 quest boards ─────────────────────────────
    /// List the generated quests posted at the POI/cave you stand in.
    /// Reply: `ServerMsg::QuestBoard`.
    QuestBoard,
    /// Accept a board quest by id (must stand at its POI/cave).
    QuestAccept { id: String },
    /// Turn in a finished board quest (at its POI/cave, the act inn, or for
    /// deliveries the destination).
    QuestTurnIn { id: String },
    /// Drop any active quest (board or authored).
    QuestAbandon { id: String },
    // ── beta-server-track: B07 social ───────────────────────────────────
    /// Form a raid (you become leader). A raid holds up to 40.
    RaidCreate,
    RaidInvite { player: String },
    RaidAccept,
    RaidLeave,
    /// Leader only.
    RaidKick { player: String },
    RaidChat { text: String },
    /// Chat channels: "general", "trade", "lfg", "zone" (your current act).
    ChannelJoin { channel: String },
    ChannelLeave { channel: String },
    ChannelSay { channel: String, text: String },
    /// Reply: `ServerMsg::Channels`.
    ChannelList,
    /// Private message to an online character.
    Whisper { to: String, text: String },
    FriendAdd { player: String },
    FriendRemove { player: String },
    IgnoreAdd { player: String },
    IgnoreRemove { player: String },
    /// Reply: `ServerMsg::Friends`.
    FriendList,
    /// Play an emote ("wave", "bow", ...), optionally at a named player.
    Emote {
        emote: String,
        #[serde(default)]
        target: Option<String>,
    },
    /// Who is online; optional name/class/act/guild substring filter.
    Who {
        #[serde(default)]
        filter: Option<String>,
    },
    // ── beta-server-track: B08 travel ───────────────────────────────────
    /// Ask the flight master for known routes. Reply: `ServerMsg::FlightPaths`.
    FlightList,
    /// Fly to a discovered node id (must stand at a discovered node).
    FlightTake { node: String },
    /// While dead: respawn now at the nearest graveyard at half health.
    ReleaseSpirit,
    // ── beta-server-track: B06 items ────────────────────────────────────
    /// Repair all worn gear at the inn (gold cost).
    Repair,
    /// Rarity + durability for everything you carry or wear, plus the
    /// repair quote. Reply: `ServerMsg::Gear`.
    GearInfo,    // ── B10 battleground ────────────────────────────────────────────────
    /// Join the queue for the Relic of Seth (from anywhere; the Esc menu,
    /// the battlemaster or `/bg`). Leaving the match returns you to where
    /// you queued.
    BgQueue,
    /// Leave the queue, or leave the match you are in.
    BgLeave,
    /// Buy a battlemaster ware for honor (at an inn).
    BgBuy { item: String },
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
    /// The player began a cast-time ability (B03): show a cast bar for `secs`.
    /// `secs == 0.0` means the cast in progress was interrupted.
    CastStart { ability: String, secs: f32 },
    /// One of the player's abilities fired (B03): start its `cooldown` sweep
    /// (and the global cooldown) on the action bar.
    AbilityUsed { ability: String, cooldown: f32 },
    Pong,
    // ── beta-server-track (B05/B06/B07/B08) ─────────────────────────────
    /// Generated quests posted at the POI/cave the player stands in.
    QuestBoard { place: String, quests: Vec<BoardQuest> },
    /// Raid roster (empty members = you left / it disbanded).
    RaidInfo { leader: String, members: Vec<String> },
    /// A line on a chat channel ("general", "trade", "lfg", "zone", "raid").
    ChannelMsg { channel: String, from: String, text: String },
    /// Channel membership (reply to `ChannelList` and after join/leave).
    Channels { joined: Vec<String>, available: Vec<String> },
    /// A private message (echoed to the sender too).
    Whisper { from: String, to: String, text: String },
    /// Friends (with presence) and the ignore list.
    Friends { friends: Vec<FriendEntry>, ignored: Vec<String> },
    /// An emote performed nearby; `text` is the ready-to-print line.
    Emote { from: String, emote: String, text: String },
    /// Reply to `Who` (capped; `total` is the uncapped match count).
    WhoList { players: Vec<WhoEntry>, total: u32 },
    /// Flight master routes; `here` is the node you are standing at.
    FlightPaths { here: Option<String>, nodes: Vec<FlightNode> },
    /// Entering an act or a named sub-zone: draw the zone-in banner.
    ZoneBanner {
        act: Act,
        name: String,
        subtitle: String,
        level_min: u32,
        level_max: u32,
        /// True when this place force-flags the player for PvP.
        pvp: bool,
    },
    /// Rarity + durability of carried/worn items and the repair quote.
    Gear { items: Vec<ItemInfo>, repair_cost: u32 },
    /// Battleground queue / match / scoreboard state (B10).
    BgState { info: BgInfo },
}

// ── beta-server-track payload types ─────────────────────────────────────────
mod track;
pub use track::*;
pub mod look;
pub use look::Look;
pub mod wire;
