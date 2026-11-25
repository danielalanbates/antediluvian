//! SQLite persistence for accounts and characters.
//!
//! One row per account (keyed by lowercased name) and one embedded character
//! sheet stored alongside it. The DB is touched only on login and on periodic
//! saves / disconnect — the hot simulation path never blocks on it.

use anyhow::Result;
use antediluvia_protocol::{Act, AuctionListing, CharacterSheet, Class};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

pub struct Db {
    conn: Connection,
}

/// The WoW-systems extension of a character sheet, stored as one JSON column so
/// adding fields never needs a schema migration.
#[derive(Debug, Serialize, Deserialize)]
struct SheetExt {
    #[serde(default)]
    class: Option<Class>,
    #[serde(default)]
    gold: u32,
    #[serde(default)]
    talent_points: u32,
    #[serde(default)]
    talents: std::collections::BTreeMap<String, u32>,
    #[serde(default)]
    professions: std::collections::BTreeMap<String, u32>,
    #[serde(default)]
    guild: Option<String>,
    #[serde(default)]
    rested_xp: u32,
    #[serde(default)]
    pvp: bool,
    #[serde(default)]
    quests: std::collections::BTreeMap<String, u32>,
    #[serde(default)]
    quests_done: Vec<String>,
    /// Completed board quests, packed (sim::questbits). Absent in old saves.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    quests_gq: String,
    #[serde(default)]
    equipment: std::collections::BTreeMap<String, String>,
    #[serde(default = "default_ext_wakefulness")]
    wakefulness: f32,
    #[serde(default)]
    last_logout: Option<u64>,
    #[serde(default)]
    discovered: Vec<String>,
    #[serde(default)]
    home_act: Option<Act>,
    #[serde(default)]
    bank: Vec<String>,
    #[serde(default)]
    bank_gold: u32,
    #[serde(default)]
    stable: Vec<String>,
    #[serde(default)]
    faction: Option<String>,
    #[serde(default)]
    reputation: std::collections::BTreeMap<String, i32>,
    #[serde(default)]
    appearance: [u32; 3],
    #[serde(default)]
    look: u64,
    #[serde(default)]
    stamina: i32,
    #[serde(default)]
    max_stamina: i32,
    // beta-server-track (B06)
    #[serde(default)]
    durability: std::collections::BTreeMap<String, u32>,
}

fn default_ext_wakefulness() -> f32 {
    100.0
}

impl Default for SheetExt {
    fn default() -> Self {
        serde_json::from_str("{}").expect("empty SheetExt deserializes")
    }
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
            CREATE TABLE IF NOT EXISTS guilds (
                name_key TEXT PRIMARY KEY,
                name     TEXT NOT NULL,
                leader   TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS guild_members (
                guild_key  TEXT NOT NULL,
                member_key TEXT NOT NULL,
                member     TEXT NOT NULL,
                PRIMARY KEY (guild_key, member_key)
            );
            CREATE TABLE IF NOT EXISTS mail (
                id        INTEGER PRIMARY KEY AUTOINCREMENT,
                to_key    TEXT NOT NULL,
                from_name TEXT NOT NULL,
                item      TEXT,
                gold      INTEGER NOT NULL DEFAULT 0,
                sent_at   TEXT NOT NULL DEFAULT (datetime('now'))
            );
            CREATE TABLE IF NOT EXISTS auctions (
                id     INTEGER PRIMARY KEY AUTOINCREMENT,
                seller TEXT NOT NULL,
                item   TEXT NOT NULL,
                price  INTEGER NOT NULL
            );
            "#,
        )?;
        // v2 migration: add the JSON ext column to pre-existing v1 databases.
        let has_ext: i64 = conn.query_row(
            "SELECT COUNT(*) FROM pragma_table_info('accounts') WHERE name = 'ext'",
            [],
            |r| r.get(0),
        )?;
        if has_ext == 0 {
            conn.execute("ALTER TABLE accounts ADD COLUMN ext TEXT NOT NULL DEFAULT '{}'", [])?;
        }
        
        let has_apple: i64 = conn.query_row(
            "SELECT COUNT(*) FROM pragma_table_info('accounts') WHERE name = 'apple_id'",
            [],
            |r| r.get(0),
        )?;
        if has_apple == 0 {
            conn.execute("ALTER TABLE accounts ADD COLUMN apple_id TEXT", [])?;
            conn.execute("CREATE UNIQUE INDEX idx_accounts_apple_id ON accounts(apple_id)", [])?;
        }
        Ok(Self { conn })
    }

    /// Load a character by name, or `None` if this is a brand-new account.
    pub fn load(&self, name: &str) -> Result<Option<CharacterSheet>> {
        let key = name.trim().to_lowercase();
        let row = self
            .conn
            .query_row(
                "SELECT name, act, x, y, level, xp, max_xp, health, max_health, mana, max_mana, inventory, ext
                 FROM accounts WHERE name_key = ?1",
                params![key],
                |r| {
                    let act_str: String = r.get(1)?;
                    let inv_str: String = r.get(11)?;
                    let ext_str: String = r.get(12)?;
                    let ext: SheetExt = serde_json::from_str(&ext_str).unwrap_or_default();
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
                        class: ext.class,
                        gold: ext.gold,
                        talent_points: ext.talent_points,
                        talents: ext.talents,
                        professions: ext.professions,
                        guild: ext.guild,
                        rested_xp: ext.rested_xp,
                        pvp: ext.pvp,
                        quests: ext.quests,
                        quests_done: antediluvia_sim::questbits::unpack(ext.quests_done, &ext.quests_gq),
                        equipment: ext.equipment,
                        wakefulness: ext.wakefulness,
                        last_logout: ext.last_logout,
                        discovered: ext.discovered,
                        home_act: ext.home_act,
                        bank: ext.bank,
                        bank_gold: ext.bank_gold,
                        stable: ext.stable,
                        faction: ext.faction,
                        reputation: ext.reputation,
                        appearance: ext.appearance,
                        look: ext.look,
                        stamina: ext.stamina,
                        max_stamina: ext.max_stamina,
                        durability: ext.durability, // beta-server-track (B06)
                    })
                },
            )
            .optional()?;
        Ok(row)
    }

    /// Load a character by account id (stored in the historical `apple_id`
    /// column), or `None` if this account has not created a character.
    pub fn load_by_apple_id(&self, apple_id: &str) -> Result<Option<CharacterSheet>> {
        let row = self
            .conn
            .query_row(
                "SELECT name_key FROM accounts WHERE apple_id = ?1",
                params![apple_id],
                |r| r.get::<_, String>(0),
            )
            .optional()?;
            
        if let Some(name_key) = row {
            self.load(&name_key)
        } else {
            Ok(None)
        }
    }

    /// Insert or update a character sheet.
    pub fn save(&self, c: &CharacterSheet, apple_id: Option<&str>) -> Result<()> {
        let key = c.name.trim().to_lowercase();
        let inv = c.inventory.join("\u{1f}");
        let (done_text, done_packed) = antediluvia_sim::questbits::pack(&c.quests_done);
        let ext = serde_json::to_string(&SheetExt {
            class: c.class,
            gold: c.gold,
            talent_points: c.talent_points,
            talents: c.talents.clone(),
            professions: c.professions.clone(),
            guild: c.guild.clone(),
            rested_xp: c.rested_xp,
            pvp: c.pvp,
            quests: c.quests.clone(),
            quests_done: done_text,
            quests_gq: done_packed,
            equipment: c.equipment.clone(),
            wakefulness: c.wakefulness,
            last_logout: c.last_logout,
            discovered: c.discovered.clone(),
            home_act: c.home_act,
            bank: c.bank.clone(),
            bank_gold: c.bank_gold,
            stable: c.stable.clone(),
            faction: c.faction.clone(),
            reputation: c.reputation.clone(),
            appearance: c.appearance,
            look: c.look,
            stamina: c.stamina,
            max_stamina: c.max_stamina,
            durability: c.durability.clone(), // beta-server-track (B06)
        })?;
        self.conn.execute(
            "INSERT INTO accounts
                (name_key, name, act, x, y, level, xp, max_xp, health, max_health, mana, max_mana, inventory, ext, updated_at, apple_id)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14, datetime('now'), ?15)
             ON CONFLICT(name_key) DO UPDATE SET
                act=excluded.act, x=excluded.x, y=excluded.y, level=excluded.level,
                xp=excluded.xp, max_xp=excluded.max_xp, health=excluded.health,
                max_health=excluded.max_health, mana=excluded.mana, max_mana=excluded.max_mana,
                inventory=excluded.inventory, ext=excluded.ext, updated_at=datetime('now')",
            params![
                key, c.name, c.act.as_str(), c.x, c.y,
                c.level as i64, c.xp as i64, c.max_xp as i64,
                c.health, c.max_health, c.mana, c.max_mana, inv, ext, apple_id
            ],
        )?;
        Ok(())
    }

    // ── Guilds ───────────────────────────────────────────────────────────────

    /// Create a guild with `leader` as its first member. Errors if the name is taken.
    pub fn guild_create(&self, name: &str, leader: &str) -> Result<bool> {
        let key = name.trim().to_lowercase();
        let n = self.conn.execute(
            "INSERT OR IGNORE INTO guilds (name_key, name, leader) VALUES (?1, ?2, ?3)",
            params![key, name.trim(), leader],
        )?;
        if n == 0 {
            return Ok(false);
        }
        self.guild_add_member(name, leader)?;
        Ok(true)
    }

    pub fn guild_add_member(&self, guild: &str, member: &str) -> Result<()> {
        self.conn.execute(
            "INSERT OR IGNORE INTO guild_members (guild_key, member_key, member) VALUES (?1, ?2, ?3)",
            params![guild.trim().to_lowercase(), member.trim().to_lowercase(), member.trim()],
        )?;
        Ok(())
    }

    pub fn guild_remove_member(&self, guild: &str, member: &str) -> Result<()> {
        self.conn.execute(
            "DELETE FROM guild_members WHERE guild_key = ?1 AND member_key = ?2",
            params![guild.trim().to_lowercase(), member.trim().to_lowercase()],
        )?;
        Ok(())
    }

    pub fn guild_members(&self, guild: &str) -> Result<Vec<String>> {
        let mut stmt = self
            .conn
            .prepare("SELECT member FROM guild_members WHERE guild_key = ?1 ORDER BY member")?;
        let rows = stmt.query_map(params![guild.trim().to_lowercase()], |r| r.get(0))?;
        Ok(rows.collect::<std::result::Result<Vec<String>, _>>()?)
    }

    // ── Mail (P3) ────────────────────────────────────────────────────────────

    /// Character name exists (has a saved sheet)?
    pub fn character_exists(&self, name: &str) -> Result<bool> {
        let n: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM accounts WHERE name_key = ?1",
            params![name.trim().to_lowercase()],
            |r| r.get(0),
        )?;
        Ok(n > 0)
    }

    pub fn mail_send(&self, to: &str, from: &str, item: Option<&str>, gold: u32) -> Result<()> {
        self.conn.execute(
            "INSERT INTO mail (to_key, from_name, item, gold) VALUES (?1, ?2, ?3, ?4)",
            params![to.trim().to_lowercase(), from, item, gold as i64],
        )?;
        Ok(())
    }

    /// Pop up to `limit` pending mails for a character: (from, item, gold).
    pub fn mail_take(&self, to: &str, limit: usize) -> Result<Vec<(String, Option<String>, u32)>> {
        let key = to.trim().to_lowercase();
        let mut stmt = self.conn.prepare(
            "SELECT id, from_name, item, gold FROM mail WHERE to_key = ?1 ORDER BY id LIMIT ?2",
        )?;
        let rows: Vec<(i64, String, Option<String>, i64)> = stmt
            .query_map(params![key, limit as i64], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
            })?
            .collect::<std::result::Result<_, _>>()?;
        let mut out = Vec::new();
        for (id, from, item, gold) in rows {
            self.conn.execute("DELETE FROM mail WHERE id = ?1", params![id])?;
            out.push((from, item, gold as u32));
        }
        Ok(out)
    }

    // ── Auction house ────────────────────────────────────────────────────────

    pub fn auction_insert(&self, seller: &str, item: &str, price: u32) -> Result<i64> {
        self.conn.execute(
            "INSERT INTO auctions (seller, item, price) VALUES (?1, ?2, ?3)",
            params![seller, item, price as i64],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn auction_list(&self) -> Result<Vec<AuctionListing>> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, seller, item, price FROM auctions ORDER BY id LIMIT 100")?;
        let rows = stmt.query_map([], |r| {
            Ok(AuctionListing {
                id: r.get(0)?,
                seller: r.get(1)?,
                item: r.get(2)?,
                price: r.get::<_, i64>(3)? as u32,
            })
        })?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    /// Atomically remove and return a listing (the buy path).
    pub fn auction_take(&self, id: i64) -> Result<Option<AuctionListing>> {
        let listing = self
            .conn
            .query_row(
                "SELECT id, seller, item, price FROM auctions WHERE id = ?1",
                params![id],
                |r| {
                    Ok(AuctionListing {
                        id: r.get(0)?,
                        seller: r.get(1)?,
                        item: r.get(2)?,
                        price: r.get::<_, i64>(3)? as u32,
                    })
                },
            )
            .optional()?;
        if listing.is_some() {
            self.conn.execute("DELETE FROM auctions WHERE id = ?1", params![id])?;
        }
        Ok(listing)
    }

    /// Credit gold to an offline character's saved sheet (auction proceeds).
    pub fn credit_gold(&self, name: &str, amount: u32) -> Result<()> {
        if let Some(mut sheet) = self.load(name)? {
            sheet.gold += amount;
            self.save(&sheet, None)?;
        }
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

/// What `antediluvia-server --check-db` found. Counts only: the tool runs on
/// copies of the production DB and its output may be pasted into notes.
#[derive(Debug, Default)]
pub struct DbAudit {
    pub accounts: usize,
    /// Rows with an account id (a player can log in to them).
    pub with_account_id: usize,
    pub loaded: usize,
    pub load_errors: usize,
    /// `ext` JSON that does not parse. `load` would silently reset class,
    /// gold, quests and the rest to defaults, so this must be 0.
    pub ext_unreadable: usize,
    /// `act` strings `parse_act` does not know (they load as Eden).
    pub act_unknown: usize,
    /// Sheets the XP-curve migration will change on their next login.
    pub xp_migrations: usize,
    pub levels: std::collections::BTreeMap<u32, usize>,
    pub guilds: i64,
    pub mail: i64,
    pub auctions: i64,
}

impl Db {
    /// Load every account the way a login does and count what would go wrong.
    pub fn audit(&self) -> Result<DbAudit> {
        let mut a = DbAudit::default();
        let rows: Vec<(String, bool, String, String)> = {
            let mut st = self.conn.prepare("SELECT name_key, apple_id IS NOT NULL, act, ext FROM accounts")?;
            let it = st.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?;
            it.collect::<rusqlite::Result<_>>()?
        };
        for (key, has_id, act, ext) in rows {
            a.accounts += 1;
            a.with_account_id += has_id as usize;
            if serde_json::from_str::<SheetExt>(&ext).is_err() {
                a.ext_unreadable += 1;
            }
            if !["eden", "hermon", "nephilim", "enoch", "flood"].contains(&act.as_str()) {
                a.act_unknown += 1;
            }
            match self.load(&key) {
                Ok(Some(mut s)) => {
                    a.loaded += 1;
                    if antediluvia_sim::progression::migrate_sheet(&mut s) {
                        a.xp_migrations += 1;
                    }
                    *a.levels.entry(s.level).or_default() += 1;
                }
                _ => a.load_errors += 1,
            }
        }
        let count = |t: &str| -> Result<i64> { Ok(self.conn.query_row(&format!("SELECT COUNT(*) FROM {t}"), [], |r| r.get(0))?) };
        a.guilds = count("guilds")?;
        a.mail = count("mail")?;
        a.auctions = count("auctions")?;
        Ok(a)
    }
}

// ── beta-server-track: raw connection for the social tables (social_db.rs) ──
impl Db {
    pub(crate) fn raw(&self) -> &Connection {
        &self.conn
    }
}

#[cfg(test)]
mod look_tests {
    use super::*;

    /// A row written before the beta fields existed (no look, stamina or
    /// durability in the ext JSON) must still load, with defaults.
    #[test]
    fn pre_beta_save_without_new_fields_still_loads() {
        let db = Db::open(":memory:").unwrap();
        db.raw()
            .execute(
                "INSERT INTO accounts (name_key, name, act, x, y, level, xp, max_xp, health, max_health, mana, max_mana, inventory, ext, apple_id)
                 VALUES ('oldtimer', 'Oldtimer', 'eden', 1.0, 2.0, 7, 10, 700, 80, 100, 40, 50, 'herb', ?1, 'apple-old')",
                [r#"{"class":"warrior","gold":12,"appearance":[1,5,3],"quests_done":["first_steps"]}"#],
            )
            .unwrap();
        let s = db.load_by_apple_id("apple-old").unwrap().expect("old row loads");
        assert_eq!((s.name.as_str(), s.level, s.gold), ("Oldtimer", 7, 12));
        assert_eq!(s.class, Some(antediluvia_protocol::Class::Warrior));
        assert_eq!(s.appearance, [1, 5, 3]);
        assert_eq!((s.look, s.stamina, s.max_stamina), (0, 0, 0));
        assert!(s.durability.is_empty());
        assert_eq!(s.inventory, vec!["herb".to_string()]);
        // The legacy skin and hair picks carry into a look, as on login.
        let look = antediluvia_protocol::Look::unpack(antediluvia_protocol::Look::from_legacy(s.appearance).pack());
        assert_eq!(look.skin, 5);
        // And the sim tops the empty stamina pool up when the character spawns.
        let mut w = antediluvia_sim::world::World::new(1);
        let id = w.spawn_player(9, s);
        let live = w.player_sheet(antediluvia_protocol::Act::Eden, id).unwrap();
        assert!(live.max_stamina > 0 && live.stamina == live.max_stamina, "{}/{}", live.stamina, live.max_stamina);
        // Saving it back and loading again keeps everything.
        db.save(&live, None).unwrap();
        let again = db.load("oldtimer").unwrap().unwrap();
        assert_eq!((again.gold, again.max_stamina), (12, live.max_stamina));
    }

    /// Three-place rule for the look: protocol field, SheetExt, load + save.
    #[test]
    fn look_survives_save_and_load() {
        let db = Db::open(":memory:").unwrap();
        let mut s = antediluvia_sim::world::new_character("Lookkeeper");
        s.look = antediluvia_protocol::Look::random(77).pack();
        s.appearance = [2, 0, 0];
        assert_ne!(s.look, 0);
        db.save(&s, Some("apple-look-test")).unwrap();
        let back = db.load("lookkeeper").unwrap().expect("saved character loads");
        assert_eq!(back.look, s.look);
        assert_eq!(back.appearance, [2, 0, 0]);
        let by_id = db.load_by_apple_id("apple-look-test").unwrap().unwrap();
        assert_eq!(by_id.look, s.look);
    }

    /// B10 chunk: completed board quests are stored packed; an old row with
    /// text ids loads, and its next save is small. Old-curve XP migrates.
    #[test]
    fn board_quests_store_packed_and_old_curve_migrates() {
        let db = Db::open(":memory:").unwrap();
        let gen = antediluvia_sim::questgen::generated();
        let mut done: Vec<String> = vec!["serpents_in_the_garden".into()];
        done.extend(gen.iter().map(|g| g.def.id.to_string()));
        let ext = serde_json::json!({"quests_done": done}).to_string();
        db.raw()
            .execute(
                "INSERT INTO accounts (name_key, name, act, x, y, level, xp, max_xp, health, max_health, mana, max_mana, inventory, ext, apple_id)
                 VALUES ('veteran', 'Veteran', 'eden', 1.0, 2.0, 20, 5000, 30000, 80, 100, 40, 50, '', ?1, NULL)",
                [ext.as_str()],
            )
            .unwrap();
        let old_len = ext.len();
        let s = db.load("veteran").unwrap().unwrap();
        assert_eq!(s.quests_done.len(), done.len());
        let mut w = antediluvia_sim::world::World::new(1);
        let id = w.spawn_player(9, s);
        let live = w.player_sheet(antediluvia_protocol::Act::Eden, id).unwrap();
        assert_eq!(live.max_xp, antediluvia_sim::progression::xp_to_next(20));
        assert_eq!(live.xp, (5000u64 * live.max_xp as u64 / 30000) as u32);
        db.save(&live, None).unwrap();
        let new_len: usize = db.raw().query_row("SELECT length(ext) FROM accounts WHERE name_key='veteran'", [], |r| r.get::<_, i64>(0)).unwrap() as usize;
        assert!(new_len * 3 < old_len, "ext {new_len} B vs old {old_len} B");
        let back = db.load("veteran").unwrap().unwrap();
        let (mut a, mut b) = (back.quests_done.clone(), done.clone());
        a.sort();
        b.sort();
        assert_eq!(a, b);
        println!("{} board quests done: ext {old_len} B as text -> {new_len} B packed", gen.len());
    }
}
