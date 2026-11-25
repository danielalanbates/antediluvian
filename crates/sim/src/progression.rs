// Copyright (c) 2026 Daniel Bates / Bates LLC. All rights reserved.
//! Level curve (B10 chunk): a classic-shaped curve that reaches level 60,
//! the acceptance gates for generated quests, the opening-camp rule, and the
//! one-time migration of characters saved under the old 1.35x curve.

use antediluvia_protocol::{Act, CharacterSheet};

/// Level cap.
pub const MAX_LEVEL: u32 = 60;

/// XP needed to go from `level` to `level + 1`. Quadratic like the classic
/// curve: 100 at level 1, about 1,600 at 20, about 12,000 at 59. The whole
/// climb 1 -> 60 is 257,417 XP. 0 at the cap.
pub fn xp_to_next(level: u32) -> u32 {
    if level >= MAX_LEVEL {
        return 0;
    }
    let k = level.saturating_sub(1);
    100 + 30 * k + 3 * k * k
}

/// Total XP from level 1 to reach `level`.
pub fn xp_total_to(level: u32) -> u64 {
    (1..level.min(MAX_LEVEL)).map(|l| xp_to_next(l) as u64).sum()
}

/// Acceptance gate for a generated board quest of `act` at distance tier
/// 0/1/2: inside the act's real band (Eden 1/5/9 ... Flood 49/53/57).
pub fn quest_gate(act: Act, tier: u32) -> u32 {
    let (lo, _) = crate::mobs::act_band(act);
    lo + tier.min(2) * 4
}

/// Tags of the act's opening quest chain: their first camp spawns just
/// outside the safe ring instead of anywhere in the zone.
pub fn starter_tag(act: Act, tag: &str) -> bool {
    match act {
        Act::Eden => matches!(tag, "serpent" | "cainite" | "ember_wisp"),
        Act::Hermon => matches!(tag, "watcher"),
        Act::Nephilim => matches!(tag, "giant"),
        Act::Enoch => matches!(tag, "shade"),
        Act::Flood => matches!(tag, "leviathan"),
    }
}

/// Bring a saved sheet onto this curve. Keeps the fraction of the current
/// level already earned; clamps levels above the cap. Idempotent. Returns
/// true when anything changed.
pub fn migrate_sheet(s: &mut CharacterSheet) -> bool {
    let mut changed = false;
    if s.level == 0 {
        s.level = 1;
        changed = true;
    }
    if s.level > MAX_LEVEL {
        s.level = MAX_LEVEL;
        changed = true;
    }
    let want = xp_to_next(s.level);
    if s.max_xp != want {
        if want == 0 {
            s.xp = 0;
        } else if s.max_xp > 0 {
            let frac = (s.xp as u64 * want as u64) / s.max_xp as u64;
            s.xp = (frac as u32).min(want - 1);
        } else {
            s.xp = s.xp.min(want - 1);
        }
        s.max_xp = want;
        changed = true;
    } else if want == 0 && s.xp != 0 {
        s.xp = 0;
        changed = true;
    } else if want > 0 && s.xp >= want {
        s.xp = want - 1;
        changed = true;
    }
    changed
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sheet(level: u32, xp: u32, max_xp: u32) -> CharacterSheet {
        let mut s: CharacterSheet = serde_json::from_str(
            r#"{"name":"T","act":"eden","x":0,"y":0,"level":1,"xp":0,"max_xp":100,"health":100,"max_health":100,"mana":50,"max_mana":50,"inventory":[]}"#,
        ).unwrap();
        s.level = level;
        s.xp = xp;
        s.max_xp = max_xp;
        s
    }

    #[test]
    fn curve_is_classic_shaped_and_reaches_sixty() {
        assert_eq!(xp_to_next(1), 100);
        assert_eq!(xp_to_next(MAX_LEVEL), 0);
        for l in 1..MAX_LEVEL - 1 {
            assert!(xp_to_next(l + 1) > xp_to_next(l), "curve must rise at {l}");
        }
        assert_eq!(xp_total_to(MAX_LEVEL), 257_417);
        // Kills of a same-level camp mob (xp 10 + 3L) per level stay sane:
        // ~9 at level 1, never more than ~70 near the cap.
        for l in 1..MAX_LEVEL {
            let kills = xp_to_next(l) as f32 / (10 + 3 * l) as f32;
            assert!((5.0..=70.0).contains(&kills), "level {l}: {kills} kills");
        }
        // The old 1.35x curve overflowed u32 well before 60; this one is tiny.
        assert!(xp_to_next(59) < 20_000);
    }

    #[test]
    fn gates_sit_inside_each_band() {
        for act in Act::ALL {
            let (lo, hi) = crate::mobs::act_band(act);
            for t in 0..3 {
                assert!((lo..=hi).contains(&quest_gate(act, t)));
            }
        }
        assert_eq!(quest_gate(Act::Eden, 0), 1);
        assert_eq!(quest_gate(Act::Flood, 2), 57);
    }

    #[test]
    fn migration_keeps_progress_fraction_and_is_idempotent() {
        // Old curve at level 10: max_xp = 100 * 1.35^9 ~ 1488, half earned.
        let mut s = sheet(10, 744, 1488);
        assert!(migrate_sheet(&mut s));
        assert_eq!(s.max_xp, xp_to_next(10));
        assert_eq!(s.xp, (744u64 * xp_to_next(10) as u64 / 1488) as u32);
        let snap = (s.level, s.xp, s.max_xp);
        assert!(!migrate_sheet(&mut s));
        assert_eq!(snap, (s.level, s.xp, s.max_xp));
        // Overflowed old sheet above the cap clamps.
        let mut s = sheet(70, u32::MAX - 5, u32::MAX);
        migrate_sheet(&mut s);
        assert_eq!((s.level, s.xp, s.max_xp), (60, 0, 0));
        // Fresh sheet already on curve: untouched.
        let mut s = sheet(1, 40, 100);
        assert!(!migrate_sheet(&mut s));
        // Zero max_xp (corrupt) is repaired.
        let mut s = sheet(5, 9_999, 0);
        migrate_sheet(&mut s);
        assert_eq!(s.max_xp, xp_to_next(5));
        assert!(s.xp < s.max_xp);
    }
}
