// Copyright (c) 2026 Daniel Bates / Bates LLC. All rights reserved.
//! Open-beta safety and population tests (2026-11-24).
//!
//! Two defects seen on the dev shard and locally:
//! 1. The Eden chain bot stalled at 6/8 cainites, then found 0/8 on its
//!    second run: a killed mob came back at once at a random point anywhere
//!    in the 7,200-unit zone, so a camp cleared once was gone for good.
//! 2. An idle level 1 at the Gate of Eden (where new characters start) was
//!    killed in about 90 s by next-tier cave watchers patrolling 500 units
//!    away; nothing stopped a mob walking up to an idle player at spawn.

use super::*;
use crate::travel::graveyards;

/// The seed the server boots with (`crates/server/src/main.rs`).
const SERVER_SEED: u64 = 0xA17E_D17E;
const TICKS_PER_SEC: u64 = TICK_HZ;

fn step_secs(w: &mut World, secs: u64) -> Vec<SimEvent> {
    let mut out = Vec::new();
    for _ in 0..secs * TICKS_PER_SEC {
        out.extend(w.step());
    }
    out
}

/// Every live killable thing of `act` as a sorted census of what defines
/// it (kind, tag, home spot, stats, aggro, patrol). Players and NPCs are
/// left out.
fn census(w: &World, act: Act) -> Vec<String> {
    let mut v: Vec<String> = w.zones[&act]
        .entities
        .values()
        .filter(|e| matches!(e.kind, EntityKind::Enemy | EntityKind::Resource) && e.health > 0)
        .map(|e| {
            format!(
                "{:?} {} @({:.0},{:.0}) hp{} dmg{} xp{} aggro{:.0} patrol{:.0} speed{:.0}",
                e.kind,
                e.tag.as_deref().unwrap_or(""),
                e.origin.x,
                e.origin.y,
                e.max_health,
                e.damage,
                e.xp_value,
                e.aggro_range,
                e.patrol_radius,
                e.speed
            )
        })
        .collect();
    v.sort();
    v
}

fn live_tagged_near(w: &World, act: Act, tag: &str, at: Vec2, r: f32) -> Vec<(EntityId, Vec2)> {
    w.zones[&act]
        .entities
        .values()
        .filter(|e| e.kind == EntityKind::Enemy && e.health > 0 && e.state != AiState::Return)
        .filter(|e| e.tag.as_deref() == Some(tag) && e.pos.distance(at) <= r)
        .map(|e| (e.id, e.pos))
        .collect()
}

/// A godmode player at `pos` (mobs cannot end a population test early).
fn immortal(w: &mut World, act: Act, owner: u64, pos: Vec2) -> EntityId {
    let mut s = new_character("Warden");
    s.act = act;
    s.x = pos.x;
    s.y = pos.y;
    let id = w.spawn_player(owner, s);
    w.zones.get_mut(&act).unwrap().entities.get_mut(&id).unwrap().godmode = true;
    id
}

#[test]
fn every_kill_comes_back_at_its_home_spot_with_its_stats() {
    for act in Act::ALL {
        let mut w = World::new(SERVER_SEED);
        let before = census(&w, act);
        assert!(before.len() > 40, "{act:?}: census too small ({})", before.len());
        // A killer outside every sanctuary puts a lethal DoT on every mob
        // and node of the zone at once.
        let spot = Vec2::new(0.0, -3000.0);
        assert!(!in_sanctuary(act, spot));
        let killer = immortal(&mut w, act, 1, spot);
        for e in w.zones.get_mut(&act).unwrap().entities.values_mut() {
            if matches!(e.kind, EntityKind::Enemy | EntityKind::Resource) {
                e.health = 1;
                e.auras.push(Aura { kind: AuraKind::Dot, left: 1.5, mag: 5.0, src: killer, ability: "test", tick: 0.0 });
            }
        }
        step_secs(&mut w, 2);
        assert!(census(&w, act).is_empty(), "{act:?}: something survived or came back at once");
        assert_eq!(w.zones[&act].respawns.len(), before.len(), "{act:?}: every kill queued its replacement");
        // Nothing is back before the shortest delay ...
        step_secs(&mut w, RESPAWN_DELAY_SECS.0 as u64 - 3);
        assert!(census(&w, act).is_empty(), "{act:?}: replacement came back before its delay");
        // ... and everything is back, at home and with its own stats, after
        // the longest one.
        step_secs(&mut w, (RESPAWN_DELAY_SECS.1 - RESPAWN_DELAY_SECS.0) as u64 + 5);
        assert_eq!(census(&w, act), before, "{act:?}: population did not refill as it was");
        assert!(w.zones[&act].respawns.is_empty());
        w.zones[&act].check_grid();
    }
}

#[test]
fn the_eden_cainite_camp_serves_the_chain_quest_twice_in_a_row() {
    // What the quest bot does: hunt cainites only around the inn (its sweep
    // ring), 8 kills, then a second character does the same on the same
    // world. Before the fix the first run stalled at 6/8 (the camp held 6;
    // the other 2 cainites sit 2,800 units away) and the second found 0.
    let mut w = World::new(SERVER_SEED);
    let act = Act::Eden;
    let camp = live_tagged_near(&w, act, "cainite", Vec2::ZERO, 1000.0).len();
    assert!(camp >= 6, "the opening camp holds {camp} cainites");
    for (run, owner) in [(1, 11u64), (2, 12u64)] {
        let pid = immortal(&mut w, act, owner, Vec2::new(-70.0, 110.0));
        if let Some(s) = w.zones.get_mut(&act).unwrap().entities.get_mut(&pid).unwrap().sheet.as_mut() {
            s.quests_done.push("serpents_in_the_garden".into());
            s.quests.insert("altar_of_the_firstborn".into(), 0);
        }
        let progress = |w: &World| w.sheet_of(act, pid).and_then(|s| s.quests.get("altar_of_the_firstborn").copied()).unwrap_or(0);
        let mut ticks = 0u64;
        while progress(&w) < 8 {
            assert!(ticks < 4 * 60 * TICKS_PER_SEC, "run {run}: stalled at {}/8 after 4 minutes", progress(&w));
            let me = w.player_pos(act, pid).unwrap();
            let near = live_tagged_near(&w, act, "cainite", Vec2::ZERO, 1000.0);
            if let Some((_, epos)) = near.into_iter().min_by(|a, b| a.1.distance(me).total_cmp(&b.1.distance(me))) {
                let p = w.zones.get_mut(&act).unwrap().entities.get_mut(&pid).unwrap();
                p.pos = epos - Vec2::new(12.0, 0.0);
                p.rot = 0.0;
                w.zones.get_mut(&act).unwrap().refile_entity(pid);
                w.queue_attack(act, pid);
            }
            w.step();
            ticks += 1;
        }
        w.remove_player(act, pid);
    }
    // The zone still holds every cainite it booted with once things settle.
    step_secs(&mut w, RESPAWN_DELAY_SECS.1 as u64 + 2);
    let total = w.zones[&act].entities.values().filter(|e| e.tag.as_deref() == Some("cainite") && e.health > 0).count();
    let table = act_spawn_table(act).iter().find(|(t, _)| *t == "cainite").unwrap().1;
    assert_eq!(total, table, "cainite population drifted");
}

#[test]
fn idle_new_characters_are_safe_at_spawn_inns_and_graveyards() {
    for seed in [SERVER_SEED, 7, 31] {
        let mut w = World::new(seed);
        // A brand-new character exactly as the server makes one: at the
        // Gate of Eden. Plus one idle player at every graveyard (the first
        // is the inn shrine) of every act.
        let mut idle: Vec<(Act, EntityId, u64)> = vec![(Act::Eden, w.spawn_player(1, new_character("Newborn")), 1)];
        let mut owner = 2u64;
        for act in Act::ALL {
            for (_, p) in graveyards(act) {
                let mut s = new_character("Idler");
                s.act = act;
                s.x = p.x;
                s.y = p.y;
                idle.push((act, w.spawn_player(owner, s), owner));
                owner += 1;
            }
        }
        let ids: Vec<EntityId> = idle.iter().map(|(_, id, _)| *id).collect();
        // Ten idle minutes.
        for t in 0..10 * 60 * TICKS_PER_SEC {
            for ev in w.step() {
                match ev {
                    SimEvent::Died { owner, .. } => panic!("seed {seed}: idle player {owner} died at {:.1}s", t as f32 / 20.0),
                    SimEvent::Combat { kind: EventKind::Attack, src, dst: Some(d), act } if ids.contains(&d) => {
                        let m = &w.zones[&act].entities[&src];
                        panic!(
                            "seed {seed}: {:?} (home {:?}) attacked an idle player at {:?} at {:.1}s",
                            m.tag, m.origin, w.player_pos(act, d), t as f32 / 20.0
                        );
                    }
                    _ => {}
                }
            }
        }
        for (act, id, _) in &idle {
            let e = &w.zones[act].entities[id];
            assert_eq!(e.health, e.max_health, "seed {seed}: idle player hurt");
        }
    }
}

#[test]
fn no_hostile_spawn_or_patrol_home_sits_in_a_sanctuary() {
    for seed in [SERVER_SEED, 7, 31] {
        let w = World::new(seed);
        for act in Act::ALL {
            for e in w.zones[&act].entities.values() {
                if e.kind == EntityKind::Enemy && e.speed > 0.0 {
                    assert!(!in_sanctuary(act, e.origin), "seed {seed} {act:?}: {:?} homed at {:?} in a sanctuary", e.tag, e.origin);
                }
            }
        }
        // The new-character spawn point is one.
        assert!(in_sanctuary(Act::Eden, GATE_OF_EDEN));
        let s = new_character("X");
        assert!(in_sanctuary(Act::Eden, Vec2::new(s.x, s.y)));
    }
}

/// One fast mob of our own at `home`, every other mob stripped.
fn lone_mob(home: Vec2) -> (World, EntityId) {
    let mut w = World::new(SERVER_SEED);
    for z in w.zones.values_mut() {
        z.entities.retain(|_, e| !matches!(e.kind, EntityKind::Enemy | EntityKind::Wildlife));
        z.rebuild_grid();
    }
    let id = w.alloc_id();
    let mut m = make_enemy_raw(id, home, "cainite", Act::Eden);
    m.max_health = 400;
    m.health = 400;
    m.damage = 5;
    w.zones.get_mut(&Act::Eden).unwrap().insert_entity(id, m);
    (w, id)
}

fn mob(w: &World, id: EntityId) -> &Entity {
    &w.zones[&Act::Eden].entities[&id]
}

#[test]
fn a_mob_never_follows_a_player_into_a_sanctuary() {
    let home = Vec2::new(0.0, 640.0);
    let (mut w, m) = lone_mob(home);
    let pid = immortal(&mut w, Act::Eden, 1, Vec2::new(0.0, 520.0));
    step_secs(&mut w, 1);
    assert!(mob(&w, m).state == AiState::Chase, "the mob should notice a player outside the ring");
    // Run for the inn.
    w.set_intent(Act::Eden, pid, Vec2::new(0.0, -1.0));
    let mut attacked_inside = 0;
    for _ in 0..3 * TICKS_PER_SEC {
        for ev in w.step() {
            if let SimEvent::Combat { kind: EventKind::Attack, dst: Some(d), .. } = ev {
                if d == pid && in_sanctuary(Act::Eden, w.player_pos(Act::Eden, pid).unwrap()) {
                    attacked_inside += 1;
                }
            }
        }
        assert!(!in_sanctuary(Act::Eden, mob(&w, m).pos), "the mob followed into the inn ring");
    }
    w.set_intent(Act::Eden, pid, Vec2::ZERO);
    assert!(in_sanctuary(Act::Eden, w.player_pos(Act::Eden, pid).unwrap()));
    assert_eq!(attacked_inside, 0);
    // It gives up and walks home, then patrols there again.
    step_secs(&mut w, 8);
    assert!(mob(&w, m).state == AiState::Patrol);
    assert!(mob(&w, m).origin.distance(home) < 1.0);
    assert!(mob(&w, m).pos.distance(home) <= mob(&w, m).patrol_radius + 1.0);
    // Standing in the ring for another minute is safe.
    for ev in step_secs(&mut w, 60) {
        if let SimEvent::Combat { kind: EventKind::Attack, dst: Some(d), .. } = ev {
            assert_ne!(d, pid, "attacked while standing in the inn ring");
        }
    }
}

#[test]
fn a_mob_hit_from_a_sanctuary_evades_unhurt() {
    let home = Vec2::new(0.0, 455.0);
    let (mut w, m) = lone_mob(home);
    {
        let z = w.zones.get_mut(&Act::Eden).unwrap();
        let e = z.entities.get_mut(&m).unwrap();
        e.patrol_radius = 0.0; // hold still at the ring's edge
    }
    // Inside the ring, facing the mob, in melee reach.
    let pid = immortal(&mut w, Act::Eden, 1, Vec2::new(0.0, 390.0));
    w.zones.get_mut(&Act::Eden).unwrap().entities.get_mut(&pid).unwrap().rot = std::f32::consts::FRAC_PI_2;
    assert!(in_sanctuary(Act::Eden, w.player_pos(Act::Eden, pid).unwrap()));
    w.queue_attack(Act::Eden, pid);
    w.step();
    let e = mob(&w, m);
    assert_eq!(e.health, e.max_health, "a blow from inside the ring landed");
    assert!(e.state == AiState::Return, "the mob should evade");
}

#[test]
fn a_dragged_mob_leashes_home_and_resets() {
    let home = Vec2::new(0.0, 1400.0);
    let (mut w, m) = lone_mob(home);
    w.zones.get_mut(&Act::Eden).unwrap().entities.get_mut(&m).unwrap().health = 150;
    // Provoke it, then run away north; the provoke keeps it chasing past
    // its aggro range, until the leash snaps.
    let pid = immortal(&mut w, Act::Eden, 1, home + Vec2::new(0.0, 60.0));
    w.zones.get_mut(&Act::Eden).unwrap().entities.get_mut(&m).unwrap().provoked = Some((pid, 30.0));
    w.set_intent(Act::Eden, pid, Vec2::new(0.0, 1.0));
    let mut farthest = 0.0f32;
    let mut evaded = false;
    for _ in 0..20 * TICKS_PER_SEC {
        w.step();
        let e = mob(&w, m);
        farthest = farthest.max(e.pos.distance(home));
        evaded |= e.state == AiState::Return;
    }
    w.set_intent(Act::Eden, pid, Vec2::ZERO);
    assert!(evaded, "the mob never gave up the chase");
    assert!(farthest <= LEASH_RANGE + ENEMY_SPEED * DT + 1.0, "dragged {farthest:.0} units from home");
    step_secs(&mut w, 15);
    let e = mob(&w, m);
    assert!(e.state == AiState::Patrol, "still not home");
    assert_eq!(e.health, e.max_health, "evading home heals to full");
    assert!(e.pos.distance(home) <= e.patrol_radius + 1.0);
}

#[test]
fn a_tamed_beast_is_replaced_in_the_wild() {
    let mut w = World::new(SERVER_SEED);
    let (act, tid, ttag, home) = Act::ALL
        .iter()
        .find_map(|act| {
            w.zones[act]
                .entities
                .values()
                .filter(|e| e.tag.as_deref().and_then(crate::mobs::mob_by_tag).is_some_and(|d| d.tameable))
                .map(|e| (*act, e.id, e.tag.clone().unwrap(), e.origin))
                .min_by_key(|t| t.1)
        })
        .expect("a tameable beast somewhere");
    let count = |w: &World| w.zones[&act].entities.values().filter(|e| e.tag.as_deref() == Some(ttag.as_str()) && e.health > 0).count();
    let before = count(&w);
    let pid = immortal(&mut w, act, 1, home);
    let mut tamed = false;
    for _ in 0..40 {
        {
            let z = w.zones.get_mut(&act).unwrap();
            if let Some(t) = z.entities.get_mut(&tid) {
                t.health = 1;
                let tp = t.pos;
                z.entities.get_mut(&pid).unwrap().pos = tp;
            }
            let s = z.entities.get_mut(&pid).unwrap().sheet.as_mut().unwrap();
            s.inventory.retain(|i| i.starts_with("taming"));
            s.inventory.push("taming_lasso".into());
        }
        if w.tame(act, pid, tid).is_ok() {
            tamed = true;
            break;
        }
    }
    assert!(tamed, "taming never succeeded in 40 tries");
    assert_eq!(count(&w), before - 1);
    step_secs(&mut w, RESPAWN_DELAY_SECS.1 as u64 + 1);
    assert_eq!(count(&w), before, "the tamed beast's haunt stayed empty");
}
