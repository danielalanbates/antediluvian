//! SQLite persistence for accounts and characters.
//!
//! One row per account (keyed by lowercased name) and one embedded character
//! sheet stored alongside it. The DB is touched only on login and on periodic
//! saves / disconnect — the hot simulation path never blocks on it.

use anyhow::Result;
use antediluvia_protocol::{Act, CharacterSheet};
use rusqlite::{params, Connection, OptionalExtension};

pub struct Db {
    conn: Connection,
}

impl Db {
    pub fn open(path: &str) -> Result<Self> {
        let conn = Connection::open(path)?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS accounts (
                name_key   TEXT PRIMARY KEY,
                name       TEXT NOT NULL,
                act        TEXT NOT NULL,
                x          REAL NOT NULL,
                y          REAL NOT NULL,
                level      INTEGER NOT NULL,
                xp         INTEGER NOT NULL,
                max_xp     INTEGER NOT NULL,
                health     INTEGER NOT NULL,
                max_health INTEGER NOT NULL,
                mana       INTEGER NOT NULL,
                max_mana   INTEGER NOT NULL,
                inventory  TEXT NOT NULL,
                created_at TEXT NOT NULL DEFAULT (datetime('now')),
                updated_at TEXT NOT NULL DEFAULT (datetime('now'))
            );
            "#,
        )?;
        Ok(Self { conn })
    }

    /// Load a character by name, or `None` if this is a brand-new account.
    pub fn load(&self, name: &str) -> Result<Option<CharacterSheet>> {
        let key = name.trim().to_lowercase();
        let row = self
            .conn
            .query_row(
                "SELECT name, act, x, y, level, xp, max_xp, health, max_health, mana, max_mana, inventory
                 FROM accounts WHERE name_key = ?1",
                params![key],
                |r| {
                    let act_str: String = r.get(1)?;
                    let inv_str: String = r.get(11)?;
                    Ok(CharacterSheet {
                        name: r.get(0)?,
                        act: parse_act(&act_str),
                        x: r.get(2)?,
                        y: r.get(3)?,
                        level: r.get::<_, i64>(4)? as u32,
                        xp: r.get::<_, i64>(5)? as u32,
                        max_xp: r.get::<_, i64>(6)? as u32,
                        health: r.get(7)?,
                        max_health: r.get(8)?,
                        mana: r.get(9)?,
                        max_mana: r.get(10)?,
                        inventory: inv_str.split('\u{1f}').filter(|s| !s.is_empty()).map(String::from).collect(),
                    })
                },
            )
            .optional()?;
        Ok(row)
    }

    /// Insert or update a character sheet.
    pub fn save(&self, c: &CharacterSheet) -> Result<()> {
        let key = c.name.trim().to_lowercase();
        let inv = c.inventory.join("\u{1f}");
        self.conn.execute(
            "INSERT INTO accounts
                (name_key, name, act, x, y, level, xp, max_xp, health, max_health, mana, max_mana, inventory, updated_at)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13, datetime('now'))
             ON CONFLICT(name_key) DO UPDATE SET
                act=excluded.act, x=excluded.x, y=excluded.y, level=excluded.level,
                xp=excluded.xp, max_xp=excluded.max_xp, health=excluded.health,
                max_health=excluded.max_health, mana=excluded.mana, max_mana=excluded.max_mana,
                inventory=excluded.inventory, updated_at=datetime('now')",
            params![
                key, c.name, c.act.as_str(), c.x, c.y,
                c.level as i64, c.xp as i64, c.max_xp as i64,
                c.health, c.max_health, c.mana, c.max_mana, inv
            ],
        )?;
        Ok(())
    }
}

fn parse_act(s: &str) -> Act {
    match s {
        "hermon" => Act::Hermon,
        "nephilim" => Act::Nephilim,
        "enoch" => Act::Enoch,
        "flood" => Act::Flood,
        _ => Act::Eden,
    }
}
