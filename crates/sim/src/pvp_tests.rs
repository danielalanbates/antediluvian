// Copyright (c) 2026 Daniel Bates / Bates LLC. All rights reserved.
//! PvP confirmation tests (beta server track, item 5): the opt-in toggle
//! really gates player-vs-player damage, and the ground around an ENEMY
//! capital force-flags by default - for every act's two capitals.
//!
//! These only use the sim's public surface, so they keep passing whatever
//! the combat internals do.

use crate::world::{new_character, pvp_zones, World, FORCED_PVP_LINGER, TICK_HZ};
use antediluvia_protocol::{Act, EntityId, EntityKind};
use glam::Vec2;

/// A spot with nothing alive within 450 units, away from every capital.
fn clear_spot(w: &World, act: Act) -> Vec2 {
    let z = &w.zones[&act];
    for gx in -6..=6 {
        for gy in -6..=6 {
            let c = Vec2::new(gx as f32 * 500.0 + 37.0, gy as f32 * 500.0 + 53.0);
            let in_capital = pvp_zones(act).any(|zn| Vec2::new(zn.x, zn.y).distance(c) <= zn.radius + 200.0);
            if !in_capital && z.entities.values().all(|e| e.pos.distance(c) > 450.0) {
                return c;
            }
        }
    }
    panic!("no clear spot in {act:?}");
}

/// Make every mob in the act harmless and rooted, so the only damage in a
/// test is player-versus-player.
fn pacify(w: &mut World, act: Act) {
    for e in w.zones.get_mut(&act).unwrap().entities.values_mut() {
        if e.kind != EntityKind::Player {
            e.damage = 0;
            e.aggro_range = 0.0;
            e.speed = 0.0;
        }
    }
}

fn spawn(w: &mut World, owner: u64, name: &str, act: Act, pos: Vec2, faction: Option<&str>) -> EntityId {
    let mut s = new_character(name);
    s.act = act;
    s.x = pos.x;
    s.y = pos.y;
    s.faction = faction.map(str::to_string);
    w.spawn_player(owner, s)
}

fn hp(w: &World, act: Act, id: EntityId) -> i32 {
    w.zones[&act].entities[&id].health
}

/// `a` (facing +x) swings at whatever stands in front for `ticks` ticks.
fn swing(w: &mut World, act: Act, a: EntityId, ticks: usize) {
    for _ in 0..ticks {
        w.queue_attack(act, a);
        w.step();
    }
}

fn flagged(w: &World, act: Act, id: EntityId) -> bool {
    let e = &w.zones[&act].entities[&id];
    e.forced_pvp || e.forced_pvp_timer > 0.0
}

#[test]
fn pvp_is_opt_in_and_needs_both_sides_flagged() {
    let mut w = World::new(6);
    let c = clear_spot(&w, Act::Eden);
    pacify(&mut w, Act::Eden);
    let a = spawn(&mut w, 1, "Cain", Act::Eden, c, None);
    let b = spawn(&mut w, 2, "Abel", Act::Eden, c + Vec2::new(40.0, 0.0), None);
    // Fresh characters are unflagged.
    assert!(!w.player_sheet(Act::Eden, a).unwrap().pvp);
    swing(&mut w, Act::Eden, a, 40);
    assert_eq!(hp(&w, Act::Eden, b), 100, "neither flagged: no damage");
    // Only the attacker opts in: still nothing.
    assert_eq!(w.toggle_pvp(Act::Eden, a), Some(true));
    assert!(w.player_sheet(Act::Eden, a).unwrap().pvp, "the flag is on the sheet (so it persists)");
    swing(&mut w, Act::Eden, a, 40);
    assert_eq!(hp(&w, Act::Eden, b), 100, "an unflagged player cannot be attacked");
    // Only the victim flagged: still nothing.
    assert_eq!(w.toggle_pvp(Act::Eden, a), Some(false));
    assert_eq!(w.toggle_pvp(Act::Eden, b), Some(true));
    swing(&mut w, Act::Eden, a, 40);
    assert_eq!(hp(&w, Act::Eden, b), 100, "an unflagged player cannot attack");
    // Both opted in: blows land.
    assert_eq!(w.toggle_pvp(Act::Eden, a), Some(true));
    swing(&mut w, Act::Eden, a, 40);
    let hurt = hp(&w, Act::Eden, b);
    assert!(hurt < 100, "both flagged: damage must land (hp {hurt})");
    // Opting back out stops it again.
    assert_eq!(w.toggle_pvp(Act::Eden, b), Some(false));
    swing(&mut w, Act::Eden, a, 40);
    // (>=, not ==: out-of-range health now regenerates between blows.)
    assert!(hp(&w, Act::Eden, b) >= hurt, "toggle off restores immunity");
    // Toggling a player that does not exist is a no-op.
    assert_eq!(w.toggle_pvp(Act::Eden, 999_999), None);
}

#[test]
fn every_enemy_capital_force_flags_and_own_capital_does_not() {
    let mut w = World::new(73);
    let mut owner = 0;
    for act in Act::ALL {
        let zones: Vec<_> = pvp_zones(act).collect();
        assert_eq!(zones.len(), 2, "{act:?} needs a capital for each lineage");
        let factions: std::collections::HashSet<_> = zones.iter().map(|z| z.faction.as_str()).collect();
        assert!(factions.contains("sethite") && factions.contains("cainite"));
        for z in &zones {
            let centre = Vec2::new(z.x, z.y);
            // Just inside the rim: the whole territory flags, not only the centre.
            let rim = centre + Vec2::new(-(z.radius - 20.0), 0.0);
            let outside = centre + Vec2::new(-(z.radius + 60.0), 0.0);
            let enemy = if z.faction == "sethite" { "cainite" } else { "sethite" };
            for (who, expect) in [(Some(enemy), true), (Some(z.faction.as_str()), false), (None, true)] {
                owner += 1;
                let id = spawn(&mut w, owner, &format!("P{owner}"), act, outside, who);
                w.step();
                assert!(!flagged(&w, act, id), "{act:?} {}: flagged outside the border as {who:?}", z.name);
                assert!(!w.player_sheet(act, id).unwrap().pvp, "never opted in");
                w.zones.get_mut(&act).unwrap().entities.get_mut(&id).unwrap().pos = rim;
                w.step();
                assert_eq!(flagged(&w, act, id), expect, "{act:?} {} as {who:?}", z.name);
                // The forced flag never touches the opt-in setting.
                assert!(!w.player_sheet(act, id).unwrap().pvp);
                w.remove_player(act, id);
            }
        }
    }
}

#[test]
fn forced_flag_makes_unopted_players_attackable_then_lingers_and_drops() {
    let mut w = World::new(74);
    let act = Act::Eden;
    let z = pvp_zones(act).find(|z| z.faction == "cainite").unwrap();
    // A quiet patch inside the Cainite capital.
    let spot = {
        let zone = &w.zones[&act];
        let mut found = None;
        'search: for dx in -8..=8 {
            for dy in -8..=8 {
                let c = Vec2::new(z.x + dx as f32 * 90.0, z.y + dy as f32 * 90.0);
                if c.distance(Vec2::new(z.x, z.y)) <= z.radius - 150.0
                    && c.x.abs() < 3400.0 && c.y.abs() < 3400.0
                    && zone.entities.values().all(|e| e.kind == EntityKind::Player || e.pos.distance(c) > 420.0)
                {
                    found = Some(c);
                    break 'search;
                }
            }
        }
        found.expect("a quiet patch in the capital")
    };
    pacify(&mut w, act);
    // Two Sethite intruders who never opted in to PvP.
    let a = spawn(&mut w, 1, "Irad", act, spot, Some("sethite"));
    let b = spawn(&mut w, 2, "Mehujael", act, spot + Vec2::new(40.0, 0.0), Some("sethite"));
    w.step();
    assert!(flagged(&w, act, a) && flagged(&w, act, b));
    swing(&mut w, act, a, 40);
    let hurt = hp(&w, act, b);
    assert!(hurt < 100, "force-flagged players can be attacked without opting in (hp {hurt})");
    // Leave: the flag lingers for FORCED_PVP_LINGER seconds, then drops.
    let out = Vec2::new(z.x - z.radius - 300.0, z.y);
    for id in [a, b] {
        w.zones.get_mut(&act).unwrap().entities.get_mut(&id).unwrap().pos = out;
    }
    w.step();
    assert!(flagged(&w, act, a), "the flag lingers right after leaving");
    for _ in 0..((FORCED_PVP_LINGER as usize + 1) * TICK_HZ as usize) {
        w.step();
    }
    assert!(!flagged(&w, act, a) && !flagged(&w, act, b), "the flag drops after the linger");
    assert!(!w.player_sheet(act, a).unwrap().pvp && !w.player_sheet(act, b).unwrap().pvp);
}
