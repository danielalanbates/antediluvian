// Copyright (c) 2026 Daniel Bates / Bates LLC. All rights reserved.
//! Payload types for the beta server track (B05 quest boards, B06 item
//! rarity/durability, B07 social, B08 travel). Kept out of `lib.rs` so the
//! message enums there only gain appended variants.

use crate::Act;
use serde::{Deserialize, Serialize};

/// Item quality tier (drives the name colour in bags, loot and tooltips).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Rarity {
    Poor,
    Common,
    Uncommon,
    Rare,
    Epic,
    Legendary,
}

impl Rarity {
    pub const ALL: [Rarity; 6] = [
        Rarity::Poor,
        Rarity::Common,
        Rarity::Uncommon,
        Rarity::Rare,
        Rarity::Epic,
        Rarity::Legendary,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Rarity::Poor => "poor",
            Rarity::Common => "common",
            Rarity::Uncommon => "uncommon",
            Rarity::Rare => "rare",
            Rarity::Epic => "epic",
            Rarity::Legendary => "legendary",
        }
    }

    /// Classic quality colours as `#rrggbb`.
    pub fn color(self) -> &'static str {
        match self {
            Rarity::Poor => "#9d9d9d",
            Rarity::Common => "#ffffff",
            Rarity::Uncommon => "#1eff00",
            Rarity::Rare => "#0070dd",
            Rarity::Epic => "#a335ee",
            Rarity::Legendary => "#ff8000",
        }
    }
}

/// One carried or worn item with its quality and wear.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ItemInfo {
    pub item: String,
    pub rarity: Rarity,
    /// `#rrggbb` for `rarity` (so thin clients need no table).
    pub color: String,
    /// Equipment slot if currently worn.
    #[serde(default)]
    pub slot: Option<String>,
    /// Durability points left; 0 with `max_durability` 0 = item never wears.
    #[serde(default)]
    pub durability: u32,
    #[serde(default)]
    pub max_durability: u32,
}

/// A generated quest as shown on a POI/cave board.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BoardQuest {
    pub id: String,
    pub title: String,
    pub text: String,
    /// "kill" | "collect" | "explore" | "deliver".
    pub kind: String,
    /// Recommended level (the act's band).
    pub level: u32,
    pub min_level: u32,
    pub xp: u32,
    pub gold: u32,
    #[serde(default)]
    pub item: Option<String>,
    /// "available" | "locked" | "active" | "ready" | "done".
    pub status: String,
    #[serde(default)]
    pub progress: u32,
    pub goal: u32,
    /// Why a quest is locked (unmet prerequisite / level), if it is.
    #[serde(default)]
    pub locked_reason: Option<String>,
}

/// A friend with live presence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FriendEntry {
    pub name: String,
    pub online: bool,
    #[serde(default)]
    pub act: Option<Act>,
    #[serde(default)]
    pub level: u32,
}

/// One `/who` row.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WhoEntry {
    pub name: String,
    pub level: u32,
    #[serde(default)]
    pub class: Option<String>,
    pub act: Act,
    #[serde(default)]
    pub guild: Option<String>,
}

/// One flight-master node as seen by a particular character.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FlightNode {
    pub id: String,
    pub name: String,
    pub act: Act,
    pub x: f32,
    pub y: f32,
    /// Discovered by this character (only known nodes can be flown to).
    pub known: bool,
    /// Gold fare from where the character stands (0 if not at a node).
    pub cost: u32,
}
