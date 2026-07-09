//! Antediluvia shared wire protocol.
//!
//! Types shared between the authoritative server and any client. The transport
//! is a WebSocket carrying newline-free JSON frames (one `ClientMsg` /
//! `ServerMsg` per WebSocket text message). Binary framing can be layered on
//! later without touching game logic — these enums stay the message contract.

use serde::{Deserialize, Serialize};

/// Protocol version. Bump on any breaking change to the enums below; the server
/// rejects a `Login` whose `proto` does not match.
pub const PROTOCOL_VERSION: u32 = 1;

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
}

/// The persistent, player-owned character sheet.
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
}

// ─── Client → Server ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "t", rename_all = "snake_case")]
pub enum ClientMsg {
    /// First message on a connection. Authenticates (or auto-registers) an
    /// account by name and loads/creates its character.
    Login { proto: u32, name: String },
    /// Desired movement direction this tick, as a unit-ish vector. The server
    /// clamps magnitude and applies speed — the client never sets position.
    Move { dx: f32, dy: f32 },
    /// Melee attack toward the current facing.
    Attack,
    /// Travel to another act/zone (spawns at that zone's entry point).
    Travel { act: Act },
    /// Zone-local chat.
    Chat { text: String },
    /// Liveness; server replies `Pong`.
    Ping,
}

// ─── Server → Client ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "t", rename_all = "snake_case")]
pub enum ServerMsg {
    /// Login accepted. Carries the loaded character and the entity id the
    /// server assigned to this player.
    Welcome { entity_id: EntityId, character: CharacterSheet },
    /// Login rejected (bad proto version, name taken by a live session, …).
    LoginRejected { reason: String },
    /// Full snapshot of every entity in the player's current zone. Sent each
    /// simulation tick. (Delta compression is a later optimization.)
    Snapshot { act: Act, tick: u64, entities: Vec<EntityState> },
    /// The player's own live stats changed (xp gain, damage, level up).
    Stats { character: CharacterSheet },
    /// A chat line from another player (or the system) in the zone.
    Chat { from: String, text: String },
    /// Server-side notice (level up, death, zone change).
    Notice { text: String },
    Pong,
}
