// Copyright (c) 2026 Daniel Bates / Bates LLC. All rights reserved.
//! Compact storage for completed generated (board) quests.
//!
//! A generated quest id is ~30 bytes of text ("gq_eden_p_...-kill"). A
//! character who finishes every board quest would carry tens of KB of ids.
//! On save, generated ids are replaced by a 32-bit FNV-1a hash written as six
//! base64url characters, concatenated. Authored ids stay as text. On load the
//! hashes are mapped back through the generated table; a hash that no longer
//! names a quest (the quest was removed) is dropped.

use std::collections::HashMap;
use std::sync::OnceLock;

const ALPHA: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

/// FNV-1a, 32 bit. Stable across builds and platforms.
pub fn id_hash(id: &str) -> u32 {
    let mut h: u32 = 0x811c_9dc5;
    for b in id.bytes() {
        h ^= b as u32;
        h = h.wrapping_mul(0x0100_0193);
    }
    h
}

fn enc(h: u32, out: &mut String) {
    for i in (0..6).rev() {
        out.push(ALPHA[((h as u64 >> (i * 6)) & 63) as usize] as char);
    }
}

fn dec(chunk: &[u8]) -> Option<u32> {
    let mut v: u64 = 0;
    for &c in chunk {
        let d = ALPHA.iter().position(|a| *a == c)? as u64;
        v = (v << 6) | d;
    }
    u32::try_from(v).ok()
}

fn by_hash() -> &'static HashMap<u32, &'static str> {
    static T: OnceLock<HashMap<u32, &'static str>> = OnceLock::new();
    T.get_or_init(|| crate::questgen::generated().iter().map(|g| (id_hash(g.def.id), g.def.id)).collect())
}

/// Split a completed list into (ids kept as text, packed generated hashes).
pub fn pack(done: &[String]) -> (Vec<String>, String) {
    let mut keep = Vec::new();
    let mut hashes: Vec<u32> = Vec::new();
    for d in done {
        if crate::questgen::gen_quest(d).is_some() {
            hashes.push(id_hash(d));
        } else {
            keep.push(d.clone());
        }
    }
    hashes.sort_unstable();
    hashes.dedup();
    let mut s = String::with_capacity(hashes.len() * 6);
    for h in hashes {
        enc(h, &mut s);
    }
    (keep, s)
}

/// Rebuild the full completed list from what `pack` wrote.
pub fn unpack(mut keep: Vec<String>, packed: &str) -> Vec<String> {
    for chunk in packed.as_bytes().chunks(6) {
        if chunk.len() != 6 {
            break;
        }
        if let Some(id) = dec(chunk).and_then(|h| by_hash().get(&h)) {
            if !keep.iter().any(|k| k == id) {
                keep.push((*id).to_string());
            }
        }
    }
    keep
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::questgen::generated;

    #[test]
    fn hashes_are_unique_over_every_quest() {
        let mut seen = std::collections::HashSet::new();
        for g in generated() {
            assert!(seen.insert(id_hash(g.def.id)), "hash collision at {}", g.def.id);
        }
        for q in crate::quests::QUESTS {
            assert!(!by_hash().contains_key(&id_hash(q.id)) || q.id.starts_with("gq_"), "authored {} collides", q.id);
        }
    }

    #[test]
    fn all_board_quests_done_round_trips_and_stays_small() {
        let mut done: Vec<String> = vec!["serpents_in_the_garden".into(), "the_flaming_sword".into()];
        done.extend(generated().iter().map(|g| g.def.id.to_string()));
        let text_bytes: usize = done.iter().map(|d| d.len() + 3).sum();
        let (keep, packed) = pack(&done);
        assert_eq!(keep, vec!["serpents_in_the_garden".to_string(), "the_flaming_sword".to_string()]);
        assert_eq!(packed.len(), generated().len() * 6);
        let back = unpack(keep, &packed);
        let mut a = done.clone();
        let mut b = back.clone();
        a.sort();
        b.sort();
        assert_eq!(a, b);
        // At least 4x smaller than the text list.
        assert!(packed.len() * 4 < text_bytes, "{} vs {}", packed.len(), text_bytes);
        println!("all {} board quests: {} B packed vs {} B as text", generated().len(), packed.len(), text_bytes);
    }

    #[test]
    fn old_saves_and_junk_load() {
        // Old save: ids as text, nothing packed.
        let old = vec!["first_steps".to_string(), generated()[0].def.id.to_string()];
        assert_eq!(unpack(old.clone(), ""), old);
        // Unknown hash and a cut chunk are dropped, never panic.
        assert_eq!(unpack(vec![], "AAAAAAxyz"), Vec::<String>::new());
        assert_eq!(unpack(vec![], "!!!!!!"), Vec::<String>::new());
        // Duplicates collapse.
        let g = generated()[3].def.id.to_string();
        let (k, p) = pack(&[g.clone(), g.clone()]);
        assert_eq!(unpack(k, &p), vec![g]);
    }
}
