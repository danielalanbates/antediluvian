// Copyright (c) 2026 Daniel Bates / Bates LLC. All rights reserved.
//! Persistence for friends and ignore lists (beta server track, B07).
//!
//! One row per (owner, kind, target). Kept in its own table so the account
//! row and its JSON sheet are untouched; the table is created on first use,
//! which means existing databases need no migration step.

use crate::db::Db;
use anyhow::Result;
use rusqlite::params;

pub const KIND_FRIEND: &str = "friend";
pub const KIND_IGNORE: &str = "ignore";

impl Db {
    fn social_ensure(&self) -> Result<()> {
        self.raw().execute_batch(
            "CREATE TABLE IF NOT EXISTS social_list (
                owner_key  TEXT NOT NULL,
                kind       TEXT NOT NULL,
                target_key TEXT NOT NULL,
                target     TEXT NOT NULL,
                PRIMARY KEY (owner_key, kind, target_key)
            );",
        )?;
        Ok(())
    }

    /// (friends, ignored) display names for a character.
    pub fn social_load(&self, owner: &str) -> Result<(Vec<String>, Vec<String>)> {
        self.social_ensure()?;
        let mut stmt = self
            .raw()
            .prepare("SELECT kind, target FROM social_list WHERE owner_key = ?1 ORDER BY target_key")?;
        let rows = stmt.query_map(params![owner.trim().to_lowercase()], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
        })?;
        let (mut friends, mut ignored) = (Vec::new(), Vec::new());
        for row in rows {
            let (kind, target) = row?;
            if kind == KIND_FRIEND {
                friends.push(target);
            } else if kind == KIND_IGNORE {
                ignored.push(target);
            }
        }
        Ok((friends, ignored))
    }

    pub fn social_add(&self, owner: &str, kind: &str, target: &str) -> Result<()> {
        self.social_ensure()?;
        self.raw().execute(
            "INSERT OR REPLACE INTO social_list (owner_key, kind, target_key, target) VALUES (?1, ?2, ?3, ?4)",
            params![owner.trim().to_lowercase(), kind, target.trim().to_lowercase(), target.trim()],
        )?;
        Ok(())
    }

    pub fn social_remove(&self, owner: &str, kind: &str, target: &str) -> Result<()> {
        self.social_ensure()?;
        self.raw().execute(
            "DELETE FROM social_list WHERE owner_key = ?1 AND kind = ?2 AND target_key = ?3",
            params![owner.trim().to_lowercase(), kind, target.trim().to_lowercase()],
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn social_rows_round_trip_and_survive_reopen() {
        let dir = std::env::temp_dir().join(format!("antediluvia_social_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("social.sqlite");
        let _ = std::fs::remove_file(&path);
        let p = path.to_str().unwrap();
        {
            let db = Db::open(p).unwrap();
            assert_eq!(db.social_load("Cain").unwrap(), (vec![], vec![]));
            db.social_add("Cain", KIND_FRIEND, "Enoch").unwrap();
            db.social_add("cain", KIND_FRIEND, "Irad").unwrap();
            db.social_add("Cain", KIND_FRIEND, "enoch").unwrap(); // same key: replaces, no duplicate
            db.social_add("Cain", KIND_IGNORE, "Abel").unwrap();
            db.social_add("Seth", KIND_FRIEND, "Enosh").unwrap();
            let (f, i) = db.social_load("CAIN").unwrap();
            assert_eq!(f, vec!["enoch", "Irad"]);
            assert_eq!(i, vec!["Abel"]);
        }
        // A new process sees the same lists.
        let db = Db::open(p).unwrap();
        let (f, i) = db.social_load("cain").unwrap();
        assert_eq!((f.len(), i.len()), (2, 1));
        db.social_remove("Cain", KIND_FRIEND, "IRAD").unwrap();
        db.social_remove("Cain", KIND_IGNORE, "abel").unwrap();
        db.social_remove("Cain", KIND_IGNORE, "nobody").unwrap();
        assert_eq!(db.social_load("Cain").unwrap(), (vec!["enoch".to_string()], vec![]));
        assert_eq!(db.social_load("Seth").unwrap().0, vec!["Enosh"]);
        drop(db);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// B06 three-place rule: durability survives a save/load cycle, and a
    /// pre-durability row still loads.
    #[test]
    fn durability_persists_with_the_sheet() {
        let db = Db::open(":memory:").unwrap();
        let mut s = antediluvia_sim::world::new_character("Smith");
        db.save(&s, Some("apple-1")).unwrap();
        assert!(db.load("Smith").unwrap().unwrap().durability.is_empty());
        s.durability.insert("animal_skins".into(), 41);
        s.durability.insert("bronze_sword".into(), 0);
        db.save(&s, None).unwrap();
        let back = db.load("smith").unwrap().unwrap();
        assert_eq!(back.durability, s.durability);
        assert_eq!(back.equipment, s.equipment);
        // A row written before the field existed.
        db.raw().execute("UPDATE accounts SET ext = '{\"gold\": 7}' WHERE name_key = 'smith'", []).unwrap();
        let old = db.load("Smith").unwrap().unwrap();
        assert!(old.durability.is_empty());
        assert_eq!(old.gold, 7);
    }
}
