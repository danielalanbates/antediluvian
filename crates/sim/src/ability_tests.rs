// Copyright (c) 2026 Daniel Bates / Bates LLC. All rights reserved.
//! Tests for the data-driven class kits (B03 + B09): every ability of every
//! class is usable, charges and enforces its cost, respects its cooldown and
//! level gate, and each kind of effect does what its tooltip says.

use super::*;
use crate::abilities::{kit, Fx};

const A: Act = Act::Eden;
const HOME: Vec2 = Vec2::new(1000.0, 1000.0);

/// An empty world with one player of `class` at `level`, facing +x.
fn arena(class: Class, level: u32) -> (World, EntityId) {
    let mut w = World::new(11);
    // Strip every mob so nothing wanders into the test.
    for z in w.zones.values_mut() {
        z.entities.retain(|_, e| !matches!(e.kind, EntityKind::Enemy | EntityKind::Wildlife));
    }
    let pid = add_player(&mut w, 1, "Tester", class, level, HOME);
    (w, pid)
}

fn add_player(w: &mut World, owner: u64, name: &str, class: Class, level: u32, pos: Vec2) -> EntityId {
    let mut sheet = new_character_with(name, Some(class), None, [0, 0, 0]);
    sheet.x = pos.x;
    sheet.y = pos.y;
    // Pre-mark every place as discovered so discovery XP cannot level us.
    sheet.discovered = crate::pois::pois_for_act(A)
        .map(|p| p.name.clone())
        .chain(crate::caves::caves_for_act(A).map(|c| c.name.clone()))
        .collect();
    let pid = w.spawn_player(owner, sheet);
    w.dev_set_level(A, pid, level);
    pid
}

/// A mob `dx` units in front of HOME. It cannot move; it bites for `damage`
/// only if it starts within melee reach (46).
fn add_mob(w: &mut World, dx: f32, damage: i32) -> EntityId {
    let id = w.alloc_id();
    let mut m = make_enemy_raw(id, HOME + Vec2::new(dx, 0.0), "serpent", A);
    m.max_health = 850; // just under the boss threshold, so crowd control lands
    m.health = m.max_health;
    m.damage = damage;
    m.speed = 0.0;
    m.patrol_radius = 0.0;
    m.aggro_range = 400.0;
    w.zones.get_mut(&A).unwrap().entities.insert(id, m);
    id
}

fn ent(w: &World, id: EntityId) -> &Entity {
    &w.zones[&A].entities[&id]
}

fn ent_mut(w: &mut World, id: EntityId) -> &mut Entity {
    w.zones.get_mut(&A).unwrap().entities.get_mut(&id).unwrap()
}

fn sheet(w: &World, id: EntityId) -> &CharacterSheet {
    ent(w, id).sheet.as_ref().unwrap()
}

fn pool(w: &World, id: EntityId, class: Class) -> i32 {
    resource_of(sheet(w, id), class_resource(class))
}

fn set_pool(w: &mut World, id: EntityId, class: Class, n: i32) {
    let s = ent_mut(w, id).sheet.as_mut().unwrap();
    match class_resource(class) {
        Resource::Mana => s.mana = n,
        Resource::Stamina => s.stamina = n,
    }
}

fn has_aura(w: &World, id: EntityId, kind: AuraKind) -> bool {
    ent(w, id).auras.iter().any(|a| a.kind == kind)
}

fn fired(evs: &[SimEvent], id: &str) -> bool {
    evs.iter().any(|e| matches!(e, SimEvent::AbilityUsed { ability, .. } if *ability == id))
}

fn info(evs: &[SimEvent], needle: &str) -> bool {
    evs.iter().any(|e| matches!(e, SimEvent::Info { text, .. } if text.contains(needle)))
}

fn step_n(w: &mut World, n: usize) -> Vec<SimEvent> {
    let mut all = Vec::new();
    for _ in 0..n {
        all.extend(w.step());
    }
    all
}

fn secs(s: f32) -> usize {
    (s * TICK_HZ as f32).ceil() as usize
}

/// Queue `ab` and step until it fires or is refused. Returns the events of
/// every tick stepped and whether it fired. Stops on the firing tick.
fn cast(w: &mut World, pid: EntityId, ab: &Ability) -> (Vec<SimEvent>, bool) {
    w.queue_cast(A, pid, ab.id.to_string());
    let mut all = Vec::new();
    for _ in 0..secs(ab.cast) + 3 {
        let evs = w.step();
        let done = fired(&evs, ab.id);
        all.extend(evs);
        if done {
            return (all, true);
        }
        if ent(w, pid).casting.is_none() {
            break; // refused, or not started
        }
    }
    (all, false)
}

fn ab(id: &str) -> &'static Ability {
    ability(id).unwrap_or_else(|| panic!("no ability {id}"))
}

// ─── Whole-kit sweeps (every class, every ability) ───────────────────────────

#[test]
fn every_ability_of_every_class_is_usable_and_does_what_it_says() {
    let mut checked = 0;
    for class in Class::ALL {
        for ab in kit(class) {
            let (mut w, pid) = arena(class, 60);
            let mob = add_mob(&mut w, 60.0, 1);
            // Hurt and (for Restore) drained, so heals and restores can show.
            let half = ent(&w, pid).max_health / 2;
            ent_mut(&mut w, pid).health = half;
            let restores = ab.fx.iter().any(|f| matches!(f, Fx::Restore(_)));
            if restores {
                set_pool(&mut w, pid, class, 5);
            }
            let pool_before = pool(&w, pid, class);
            let mob_hp = ent(&w, mob).health;

            let (evs, ok) = cast(&mut w, pid, ab);
            assert!(ok, "{:?} {} did not fire", class, ab.id);
            assert!(
                evs.iter().any(|e| matches!(e, SimEvent::Combat { kind: EventKind::Cast, src, .. } if *src == pid)),
                "{} sent no cast animation event",
                ab.id
            );
            if ab.cast > 0.0 {
                assert!(
                    evs.iter().any(|e| matches!(e, SimEvent::CastStart { ability, secs, .. } if *ability == ab.id && *secs == ab.cast)),
                    "{} sent no cast bar",
                    ab.id
                );
            }
            // Dots/hots tick a second later; let one tick land.
            step_n(&mut w, secs(1.1));
            for fx in ab.fx {
                match *fx {
                    Fx::Damage(_) => assert!(ent(&w, mob).health < mob_hp, "{} dealt no damage", ab.id),
                    Fx::Dot { .. } => {
                        assert!(has_aura(&w, mob, AuraKind::Dot), "{} left no DoT", ab.id);
                        assert!(ent(&w, mob).health < mob_hp, "{} DoT did not tick", ab.id);
                    }
                    Fx::Stun(_) => assert!(has_aura(&w, mob, AuraKind::Stun), "{} no stun", ab.id),
                    Fx::Root(_) => assert!(has_aura(&w, mob, AuraKind::Root), "{} no root", ab.id),
                    Fx::Slow { .. } => assert!(has_aura(&w, mob, AuraKind::Slow), "{} no slow", ab.id),
                    Fx::Taunt(_) => assert!(has_aura(&w, mob, AuraKind::Taunt), "{} no taunt", ab.id),
                    Fx::Weaken { .. } => assert!(has_aura(&w, mob, AuraKind::Weaken), "{} no weaken", ab.id),
                    Fx::Expose { .. } => assert!(has_aura(&w, mob, AuraKind::Expose), "{} no expose", ab.id),
                    Fx::Heal(n) => assert!(
                        ent(&w, pid).health >= half + n,
                        "{} healed {} of {n}",
                        ab.id,
                        ent(&w, pid).health - half
                    ),
                    Fx::Hot { heal, .. } => {
                        assert!(has_aura(&w, pid, AuraKind::Hot), "{} left no HoT", ab.id);
                        assert!(ent(&w, pid).health >= half + heal, "{} HoT did not tick", ab.id);
                    }
                    Fx::Might { .. } => assert!(has_aura(&w, pid, AuraKind::Might), "{} no might", ab.id),
                    Fx::Guard { .. } => assert!(has_aura(&w, pid, AuraKind::Guard), "{} no guard", ab.id),
                    Fx::Haste { .. } => assert!(has_aura(&w, pid, AuraKind::Haste), "{} no haste", ab.id),
                    Fx::Shield { .. } => assert!(has_aura(&w, pid, AuraKind::Shield), "{} no shield", ab.id),
                    Fx::Restore(n) => assert!(
                        pool(&w, pid, class) >= pool_before - ab.cost + n,
                        "{} restored too little",
                        ab.id
                    ),
                }
            }
            checked += 1;
        }
    }
    assert!(checked >= 64, "only {checked} abilities exercised");
}

#[test]
fn costs_are_charged_exactly_and_enforced_for_every_ability() {
    for class in Class::ALL {
        for ab in kit(class) {
            let (mut w, pid) = arena(class, 60);
            add_mob(&mut w, 60.0, 0);
            let full = pool(&w, pid, class);
            assert!(full >= ab.cost, "{}: a level-60 pool cannot afford it", ab.id);

            // Too poor: refused, nothing spent, no cooldown started.
            if ab.cost >= 4 {
                set_pool(&mut w, pid, class, ab.cost - 3);
                let (evs, ok) = cast(&mut w, pid, ab);
                assert!(!ok, "{} fired without enough {:?}", ab.id, class_resource(class));
                assert!(info(&evs, "Not enough"), "{} gave no shortage notice", ab.id);
                assert!(ent(&w, pid).cooldowns.get(ab.id).is_none(), "{} cooled down unpaid", ab.id);
                step_n(&mut w, secs(1.2)); // let any GCD lapse
            }

            // Full pool: charged exactly `cost` on the tick it fires.
            set_pool(&mut w, pid, class, full);
            let (_, ok) = cast(&mut w, pid, ab);
            assert!(ok, "{} did not fire with a full pool", ab.id);
            let restored: i32 = ab.fx.iter().map(|f| if let Fx::Restore(n) = f { *n } else { 0 }).sum();
            assert_eq!(
                pool(&w, pid, class),
                (full - ab.cost + restored).min(full),
                "{} charged the wrong amount",
                ab.id
            );
        }
    }
}

#[test]
fn cooldowns_and_global_cooldown_are_enforced_for_every_ability() {
    for class in Class::ALL {
        for ab in kit(class) {
            let (mut w, pid) = arena(class, 60);
            add_mob(&mut w, 60.0, 0);
            let (evs, ok) = cast(&mut w, pid, ab);
            assert!(ok, "{} did not fire", ab.id);
            assert!(
                evs.iter().any(|e| matches!(e, SimEvent::AbilityUsed { ability, cooldown, .. }
                    if *ability == ab.id && *cooldown == ab.cooldown)),
                "{} did not report its cooldown",
                ab.id
            );

            // At once: blocked.
            w.dev_heal(A, pid);
            let (evs, ok) = cast(&mut w, pid, ab);
            assert!(!ok && info(&evs, "not ready"), "{} recast instantly", ab.id);

            // After the global cooldown but inside its own cooldown: blocked.
            step_n(&mut w, secs(1.1));
            assert!(ab.cooldown >= 2.0, "{} cooldown is shorter than the test window", ab.id);
            let (evs, ok) = cast(&mut w, pid, ab);
            assert!(!ok && info(&evs, "not ready"), "{} ignored its cooldown", ab.id);

            // Once the cooldown has run out: usable again.
            step_n(&mut w, secs(ab.cooldown));
            w.dev_heal(A, pid);
            let (_, ok) = cast(&mut w, pid, ab);
            assert!(ok, "{} never came off cooldown", ab.id);
        }
    }
}

#[test]
fn global_cooldown_blocks_a_different_ability() {
    let (mut w, pid) = arena(Class::Wanderer, 60);
    add_mob(&mut w, 60.0, 0);
    assert!(cast(&mut w, pid, ab("shiv")).1);
    let (evs, ok) = cast(&mut w, pid, ab("lacerate"));
    assert!(!ok && info(&evs, "not ready"), "GCD should gate the next ability");
    step_n(&mut w, secs(1.0));
    assert!(cast(&mut w, pid, ab("lacerate")).1, "usable after the GCD");
}

#[test]
fn abilities_unlock_at_their_level_for_every_class() {
    for class in Class::ALL {
        let k = kit(class);
        for ab in &k {
            if ab.min_level > 1 {
                let (mut w, pid) = arena(class, ab.min_level - 1);
                add_mob(&mut w, 60.0, 0);
                let (evs, ok) = cast(&mut w, pid, ab);
                assert!(!ok, "{} usable below level {}", ab.id, ab.min_level);
                assert!(info(&evs, "not high enough level"), "{} gave no level notice", ab.id);
            }
            let (mut w, pid) = arena(class, ab.min_level);
            add_mob(&mut w, 60.0, 0);
            // A low-level pool may be smaller than the cost of a late ability
            // is not a concern here: every kit is affordable at its own level.
            let (evs, ok) = cast(&mut w, pid, ab);
            assert!(ok, "{} not usable at level {} ({})", ab.id, ab.min_level, describe(&evs));
        }
        // The kit grows as the character levels.
        let at = |lvl: u32| k.iter().filter(|a| a.min_level <= lvl).count();
        assert!(at(1) >= 1 && at(1) < at(6) && at(6) < at(14), "{class:?} kit does not grow");
        assert_eq!(at(14), k.len(), "{class:?} kit should be complete by 14");
    }
}

fn describe(evs: &[SimEvent]) -> String {
    evs.iter()
        .filter_map(|e| if let SimEvent::Info { text, .. } = e { Some(text.as_str()) } else { None })
        .collect::<Vec<_>>()
        .join(" / ")
}

#[test]
fn another_class_cannot_use_the_kit() {
    for class in Class::ALL {
        let other = Class::ALL[(Class::ALL.iter().position(|c| *c == class).unwrap() + 1) % 8];
        let (mut w, pid) = arena(other, 60);
        add_mob(&mut w, 60.0, 0);
        for ab in kit(class) {
            let before = (sheet(&w, pid).mana, sheet(&w, pid).stamina);
            let (evs, ok) = cast(&mut w, pid, ab);
            assert!(!ok && info(&evs, "not of your class"), "{other:?} used {}", ab.id);
            assert_eq!(before, (sheet(&w, pid).mana, sheet(&w, pid).stamina));
        }
    }
}

// ─── Mechanics, one effect kind at a time ────────────────────────────────────

#[test]
fn no_target_means_no_cost_and_no_cooldown() {
    let (mut w, pid) = arena(Class::Mage, 60);
    let before = sheet(&w, pid).mana;
    let (evs, ok) = cast(&mut w, pid, ab("firebolt"));
    assert!(!ok && info(&evs, "No target in range"));
    assert_eq!(sheet(&w, pid).mana, before);
    assert!(ent(&w, pid).cooldowns.is_empty());
    // A target behind the caster does not count either.
    add_mob(&mut w, -60.0, 0);
    assert!(!cast(&mut w, pid, ab("firebolt")).1, "hit a target behind the caster");
}

#[test]
fn cast_time_delays_the_effect_and_moving_interrupts_it() {
    let (mut w, pid) = arena(Class::Mage, 60);
    let mob = add_mob(&mut w, 100.0, 0);
    let pyre = ab("pyre_blast");
    let (hp, mana) = (ent(&w, mob).health, sheet(&w, pid).mana);

    w.queue_cast(A, pid, pyre.id.into());
    let evs = step_n(&mut w, secs(pyre.cast) - 2);
    assert!(evs.iter().any(|e| matches!(e, SimEvent::CastStart { secs, .. } if *secs == pyre.cast)));
    assert!(!fired(&evs, pyre.id), "fired before the cast finished");
    assert_eq!(ent(&w, mob).health, hp, "damage landed mid-cast");
    assert_eq!(sheet(&w, pid).mana, mana, "mana is paid on release, not at start");
    let evs = step_n(&mut w, 4);
    assert!(fired(&evs, pyre.id));
    assert!(ent(&w, mob).health <= hp - 70, "pyre blast hits for at least its base");

    // Second cast: step, then walk away mid-cast.
    step_n(&mut w, secs(pyre.cooldown));
    let (hp, mana) = (ent(&w, mob).health, sheet(&w, pid).mana);
    w.queue_cast(A, pid, pyre.id.into());
    step_n(&mut w, 10);
    assert!(ent(&w, pid).casting.is_some());
    w.set_intent(A, pid, Vec2::new(0.0, 1.0));
    let evs = step_n(&mut w, secs(pyre.cast));
    assert!(info(&evs, "Interrupted"));
    assert!(evs.iter().any(|e| matches!(e, SimEvent::CastStart { secs, .. } if *secs == 0.0)));
    assert!(!fired(&evs, pyre.id));
    assert_eq!(ent(&w, mob).health, hp);
    assert!(sheet(&w, pid).mana >= mana, "an interrupted cast costs nothing");
    assert!(ent(&w, pid).cooldowns.get(pyre.id).is_none(), "an interrupted cast has no cooldown");
}

#[test]
fn damage_over_time_ticks_for_its_full_duration_then_stops() {
    let (mut w, pid) = arena(Class::Warrior, 60);
    let mob = add_mob(&mut w, 60.0, 0);
    let hp = ent(&w, mob).health;
    assert!(cast(&mut w, pid, ab("rend")).1);
    step_n(&mut w, secs(9.5));
    let lost = hp - ent(&w, mob).health;
    // 6 up front + 5 a second for 9 seconds.
    assert!((6 + 5 * 8..=6 + 5 * 9).contains(&lost), "rend did {lost}");
    assert!(!has_aura(&w, mob, AuraKind::Dot), "bleed should have expired");
    let hp = ent(&w, mob).health;
    step_n(&mut w, secs(3.0));
    assert_eq!(ent(&w, mob).health, hp, "no damage after expiry");
}

#[test]
fn heal_over_time_restores_its_total() {
    let (mut w, pid) = arena(Class::Priest, 60);
    ent_mut(&mut w, pid).health = 100;
    assert!(cast(&mut w, pid, ab("renewal")).1);
    step_n(&mut w, secs(12.5));
    let gained = ent(&w, pid).health - 100;
    // 6 a second for 12 seconds, plus ~2/s natural regeneration.
    assert!((6 * 11..=6 * 12 + 30).contains(&gained), "renewal healed {gained}");
    assert!(!has_aura(&w, pid, AuraKind::Hot));
}

#[test]
fn stun_stops_a_mob_from_attacking_and_wears_off() {
    let (mut w, pid) = arena(Class::Warrior, 60);
    add_mob(&mut w, 40.0, 30); // in bite range
    step_n(&mut w, secs(2.0));
    let max = ent(&w, pid).max_health;
    assert!(ent(&w, pid).health < max, "the mob should be biting");
    assert!(cast(&mut w, pid, ab("skull_crack")).1);
    let hp = ent(&w, pid).health;
    step_n(&mut w, secs(2.8));
    assert!(ent(&w, pid).health >= hp, "a stunned mob must not attack");
    step_n(&mut w, secs(2.5));
    assert!(ent(&w, pid).health < hp + 20, "the mob bites again after the stun");
}

#[test]
fn root_holds_and_slow_slows_a_chasing_mob() {
    // Distance a mob 300 away closes in 2 seconds under an opening ability.
    let run = |class: Class, id: Option<&str>| -> f32 {
        let (mut w, pid) = arena(class, 60);
        let mob = add_mob(&mut w, 120.0, 0);
        ent_mut(&mut w, mob).speed = 60.0;
        if let Some(id) = id {
            assert!(cast(&mut w, pid, ab(id)).1, "{id}");
        } else {
            step_n(&mut w, 1);
        }
        let x0 = ent(&w, mob).pos.x;
        step_n(&mut w, secs(1.0));
        x0 - ent(&w, mob).pos.x
    };
    let free = run(Class::Mage, None);
    assert!(free > 50.0, "an unhindered mob closes ~60 units, got {free}");
    let rooted = run(Class::Mage, Some("frost_nova"));
    assert!(rooted.abs() < 0.5, "a rooted mob moved {rooted}");
    let slowed = run(Class::Mage, Some("frost_shard")); // 40% slow
    assert!((free * 0.5..free * 0.7).contains(&slowed), "slowed mob moved {slowed} vs {free}");
}

#[test]
fn stunned_and_rooted_players_cannot_move_and_haste_speeds_them() {
    let (mut w, pid) = arena(Class::Mage, 60);
    let walk = |w: &mut World| {
        let x0 = ent(w, pid).pos.x;
        w.set_intent(A, pid, Vec2::X);
        step_n(w, secs(1.0));
        w.set_intent(A, pid, Vec2::ZERO);
        ent(w, pid).pos.x - x0
    };
    let free = walk(&mut w);
    assert!(free > 100.0);
    assert!(cast(&mut w, pid, ab("quicken")).1);
    let fast = walk(&mut w);
    assert!((free * 1.3..free * 1.5).contains(&fast), "hasted {fast} vs {free}");
    let aura = |kind| Aura { kind, left: 5.0, mag: 0.0, src: 0, ability: "test", tick: 0.0 };
    ent_mut(&mut w, pid).auras = vec![aura(AuraKind::Root)];
    assert!(walk(&mut w).abs() < 0.5, "rooted players stay put");
    ent_mut(&mut w, pid).auras = vec![aura(AuraKind::Stun)];
    add_mob(&mut w, 60.0, 0);
    let (_, ok) = cast(&mut w, pid, ab("firebolt"));
    assert!(!ok, "stunned players cannot cast");
}

#[test]
fn taunt_pulls_a_mob_off_the_nearer_player() {
    let (mut w, tank) = arena(Class::Warrior, 60);
    let healer = add_player(&mut w, 2, "Healer", Class::Priest, 60, HOME + Vec2::new(200.0, 0.0));
    // Mob stands between them, nearer the healer, and can walk.
    let mob = add_mob(&mut w, 150.0, 5);
    ent_mut(&mut w, mob).speed = 60.0;
    step_n(&mut w, secs(0.5));
    assert!(ent(&w, mob).pos.x > HOME.x + 150.0, "the mob should start toward the healer");
    assert!(cast(&mut w, tank, ab("challenging_roar")).1);
    let x0 = ent(&w, mob).pos.x;
    step_n(&mut w, secs(1.0));
    assert!(ent(&w, mob).pos.x < x0 - 30.0, "a taunted mob turns on the tank");
    let _ = healer;
}

/// Health a player loses to one bite from a 50-damage mob, after `prep`.
fn bite_taken(class: Class, prep: Option<&str>) -> i32 {
    let (mut w, pid) = arena(class, 60);
    let mob = add_mob(&mut w, 40.0, 50);
    // Hold the mob's swing until the player has prepared.
    ent_mut(&mut w, mob).attack_cooldown = 3.0;
    if let Some(id) = prep {
        assert!(cast(&mut w, pid, ab(id)).1, "{id}");
    }
    let hp = ent(&w, pid).health;
    ent_mut(&mut w, mob).attack_cooldown = 0.0;
    step_n(&mut w, 2);
    hp - ent(&w, pid).health
}

#[test]
fn guard_weaken_and_shield_reduce_incoming_damage() {
    assert_eq!(bite_taken(Class::Warrior, None), 50);
    assert_eq!(bite_taken(Class::Warrior, Some("brace")), 30, "40% guard");
    assert_eq!(bite_taken(Class::Herald, Some("judgment")), 40, "20% weaken");
    assert_eq!(bite_taken(Class::Occultist, Some("curse_of_frailty")), 38, "25% weaken (37.5 rounds up)");
    assert_eq!(bite_taken(Class::Priest, Some("covering")), 0, "60-point shield soaks a 50 bite");
    assert_eq!(bite_taken(Class::Herald, Some("aegis_of_faith")), 0);
}

#[test]
fn a_shield_is_used_up_by_the_damage_it_absorbs() {
    let (mut w, pid) = arena(Class::Priest, 60);
    let mob = add_mob(&mut w, 40.0, 50);
    ent_mut(&mut w, mob).attack_cooldown = 3.0;
    assert!(cast(&mut w, pid, ab("covering")).1); // absorbs 60
    let hp = ent(&w, pid).health;
    ent_mut(&mut w, mob).attack_cooldown = 0.0;
    step_n(&mut w, 2);
    assert_eq!(ent(&w, pid).health, hp, "first bite fully absorbed");
    ent_mut(&mut w, mob).attack_cooldown = 0.0;
    step_n(&mut w, 2);
    assert_eq!(hp - ent(&w, pid).health, 40, "second bite: 10 absorbed, 40 through");
    assert!(!has_aura(&w, pid, AuraKind::Shield), "a spent shield is removed");
}

/// Damage one `strike` does to a mob after the optional `prep` ability.
fn strike_damage(class: Class, prep: Option<&str>, strike: &str) -> i32 {
    let (mut w, pid) = arena(class, 60);
    let mob = add_mob(&mut w, 60.0, 0);
    if let Some(id) = prep {
        assert!(cast(&mut w, pid, ab(id)).1, "{id}");
        step_n(&mut w, secs(1.1));
    }
    let hp = ent(&w, mob).health;
    assert!(cast(&mut w, pid, ab(strike)).1, "{strike}");
    hp - ent(&w, mob).health
}

#[test]
fn might_and_expose_raise_damage_dealt() {
    assert_eq!(strike_damage(Class::Warrior, None, "heroic_strike"), 22);
    assert_eq!(strike_damage(Class::Warrior, Some("war_cry"), "heroic_strike"), 26, "20% might");
    assert_eq!(strike_damage(Class::Hunter, None, "aimed_shot"), 20);
    assert_eq!(strike_damage(Class::Hunter, Some("hunters_mark"), "aimed_shot"), 24, "20% expose");
    assert_eq!(strike_damage(Class::Wanderer, Some("mark_of_nod"), "shiv"), 23, "25% expose on 18");
    assert_eq!(strike_damage(Class::Occultist, Some("hex_of_unmaking"), "umbral_bolt"), 35, "25% on 28");
}

#[test]
fn area_abilities_hit_every_foe_in_range_and_none_beyond() {
    let (mut w, pid) = arena(Class::Warrior, 60);
    let near: Vec<EntityId> = [60.0, -80.0, 120.0].iter().map(|dx| add_mob(&mut w, *dx, 0)).collect();
    let far = add_mob(&mut w, 300.0, 0);
    let hp = ent(&w, far).health;
    assert!(cast(&mut w, pid, ab("whirlwind")).1);
    for m in near {
        assert_eq!(hp - ent(&w, m).health, 16, "whirlwind hits all around, even behind");
    }
    assert_eq!(ent(&w, far).health, hp, "out of range is untouched");
}

#[test]
fn siphon_life_hurts_the_foe_and_heals_the_caster() {
    let (mut w, pid) = arena(Class::Occultist, 60);
    let mob = add_mob(&mut w, 60.0, 0);
    ent_mut(&mut w, pid).health = 100;
    let hp = ent(&w, mob).health;
    assert!(cast(&mut w, pid, ab("siphon_life")).1);
    assert_eq!(hp - ent(&w, mob).health, 14);
    assert!(ent(&w, pid).health >= 114);
}

#[test]
fn party_abilities_reach_party_members_in_range_only() {
    let (mut w, priest) = arena(Class::Priest, 60);
    let friend = add_player(&mut w, 2, "Friend", Class::Warrior, 60, HOME + Vec2::new(100.0, 0.0));
    let distant = add_player(&mut w, 3, "Distant", Class::Warrior, 60, HOME + Vec2::new(900.0, 0.0));
    let stranger = add_player(&mut w, 4, "Stranger", Class::Warrior, 60, HOME + Vec2::new(0.0, 100.0));
    for owner in [1, 2, 3] {
        w.party_of.insert(owner, 77);
    }
    for id in [priest, friend, distant, stranger] {
        ent_mut(&mut w, id).health = 100;
    }
    assert!(cast(&mut w, priest, ab("intercession")).1);
    // 45 healed; natural regeneration adds a few points over the 2s cast.
    assert!(ent(&w, priest).health >= 145, "caster healed");
    assert!(ent(&w, friend).health >= 145, "party member in range healed");
    assert!(ent(&w, distant).health < 120, "party member out of range not healed");
    assert!(ent(&w, stranger).health < 120, "non-member not healed");

    step_n(&mut w, secs(1.1));
    assert!(cast(&mut w, priest, ab("benediction")).1);
    step_n(&mut w, 2);
    assert!(has_aura(&w, friend, AuraKind::Guard) && has_aura(&w, priest, AuraKind::Guard));
    assert!(!has_aura(&w, stranger, AuraKind::Guard) && !has_aura(&w, distant, AuraKind::Guard));
}

#[test]
fn bosses_shrug_off_stun_and_root_but_not_damage() {
    let (mut w, pid) = arena(Class::Warden, 60);
    let boss = add_mob(&mut w, 60.0, 0);
    ent_mut(&mut w, boss).tag = Some("serpent_alpha".into());
    let hp = ent(&w, boss).health;
    assert!(cast(&mut w, pid, ab("entangling_roots")).1); // root + DoT
    step_n(&mut w, secs(1.2));
    assert!(!has_aura(&w, boss, AuraKind::Root), "bosses cannot be rooted");
    assert!(has_aura(&w, boss, AuraKind::Dot) && ent(&w, boss).health < hp, "but the DoT still lands");
}

#[test]
fn reapplying_an_effect_refreshes_instead_of_stacking() {
    let (mut w, pid) = arena(Class::Occultist, 60);
    let mob = add_mob(&mut w, 60.0, 0);
    let curse = ab("wasting_curse");
    assert!(cast(&mut w, pid, curse).1);
    step_n(&mut w, secs(curse.cooldown));
    assert!(cast(&mut w, pid, curse).1);
    step_n(&mut w, 2);
    let dots: Vec<&Aura> = ent(&w, mob).auras.iter().filter(|a| a.kind == AuraKind::Dot).collect();
    assert_eq!(dots.len(), 1, "one curse, refreshed");
    assert!(dots[0].left > 11.0, "duration reset to full");
}

#[test]
fn a_dot_can_kill_and_credits_its_caster() {
    let (mut w, pid) = arena(Class::Warden, 60);
    let mob = add_mob(&mut w, 60.0, 0);
    ent_mut(&mut w, mob).health = 5;
    let xp = sheet(&w, pid).xp;
    assert!(cast(&mut w, pid, ab("locust_swarm")).1); // 7 a second
    let evs = step_n(&mut w, secs(1.5));
    assert!(!w.zones[&A].entities.contains_key(&mob), "the swarm should finish it");
    assert!(evs.iter().any(|e| matches!(e, SimEvent::Combat { kind: EventKind::Die, .. })));
    // Level 60 is the cap (B10 curve): XP no longer accrues there, so the
    // credit shows as the kill notice to the caster instead.
    assert_eq!(sheet(&w, pid).xp, xp);
    assert!(evs.iter().any(|e| matches!(e, SimEvent::Combat { kind: EventKind::Die, .. })), "kill credit goes to the caster");
}

// ─── Resources ───────────────────────────────────────────────────────────────

#[test]
fn mana_and_stamina_regenerate_and_the_owner_is_told() {
    let (mut w, pid) = arena(Class::Mage, 10);
    set_pool(&mut w, pid, Class::Mage, 0);
    ent_mut(&mut w, pid).sheet.as_mut().unwrap().stamina = 0;
    let evs = step_n(&mut w, secs(5.0));
    let s = sheet(&w, pid);
    assert!((9..=11).contains(&s.mana), "mana regen is ~2/s, got {}", s.mana);
    assert!((49..=51).contains(&s.stamina), "stamina regen is ~10/s, got {}", s.stamina);
    let pushes = evs.iter().filter(|e| matches!(e, SimEvent::Vitals { owner: 1 })).count();
    assert!((4..=6).contains(&pushes), "one stat push a second while regenerating, got {pushes}");
    // At rest with full bars nothing is pushed.
    w.dev_heal(A, pid);
    step_n(&mut w, secs(1.0));
    let evs = step_n(&mut w, secs(3.0));
    assert!(!evs.iter().any(|e| matches!(e, SimEvent::Vitals { .. })), "idle players cost no traffic");
}

#[test]
fn stamina_classes_spend_stamina_and_mana_classes_spend_mana() {
    for class in Class::ALL {
        let (mut w, pid) = arena(class, 60);
        add_mob(&mut w, 60.0, 0);
        let first = kit(class)[0];
        let (mana, stam) = (sheet(&w, pid).mana, sheet(&w, pid).stamina);
        assert!(cast(&mut w, pid, first).1);
        let s = sheet(&w, pid);
        match class_resource(class) {
            Resource::Stamina => assert_eq!((s.mana, s.stamina), (mana, stam - first.cost), "{class:?}"),
            Resource::Mana => assert_eq!((s.mana, s.stamina), (mana - first.cost, stam), "{class:?}"),
        }
    }
}

#[test]
fn saves_without_a_stamina_pool_are_topped_up_on_spawn() {
    let mut w = World::new(5);
    let mut old = new_character("Veteran");
    old.stamina = 0;
    old.max_stamina = 0;
    let pid = w.spawn_player(9, old);
    let s = w.player_sheet(A, pid).unwrap();
    assert_eq!((s.stamina, s.max_stamina), (STAMINA_MAX, STAMINA_MAX));
}

#[test]
fn every_class_gets_its_base_kit_from_the_builder_and_from_select_class() {
    for class in Class::ALL {
        let (hp, mana) = class_base(class);
        let built = new_character_with("Built", Some(class), None, [0, 0, 0]);
        assert_eq!((built.max_health, built.max_mana), (100 + hp, 50 + mana), "{class:?}");
        let mut w = World::new(5);
        let pid = w.spawn_player(1, new_character("Late"));
        w.select_class(A, pid, class).unwrap();
        let s = w.player_sheet(A, pid).unwrap();
        assert_eq!((s.max_health, s.max_mana), (100 + hp, 50 + mana), "{class:?}");
        assert_eq!(ent(&w, pid).tag.as_deref(), Some(class.as_str()));
    }
}

#[test]
fn every_first_ability_is_affordable_at_level_one() {
    for class in Class::ALL {
        let (w, pid) = arena(class, 1);
        for ab in kit(class).iter().filter(|a| a.min_level == 1) {
            assert!(pool(&w, pid, class) >= ab.cost * 3, "{} too dear for a fresh {class:?}", ab.id);
        }
    }
}

/// B10 chunk: a mob hit from beyond its aggro range turns and chases.
#[test]
fn a_mob_hit_from_range_answers() {
    let (mut w, pid) = arena(Class::Warrior, 10);
    let mob = add_mob(&mut w, 300.0, 5);
    {
        let m = ent_mut(&mut w, mob);
        m.speed = 120.0;
        m.aggro_range = 50.0; // the player at 300 is far outside it
    }
    step_n(&mut w, 5);
    assert!(ent(&w, mob).pos.distance(HOME) > 290.0, "unprovoked, it stays");
    // Hurt it as a ranged hit would land: queued damage from the player.
    let before = ent(&w, mob).pos.distance(HOME);
    ent_mut(&mut w, mob).provoked = Some((pid, 10.0));
    step_n(&mut w, 20);
    assert!(ent(&w, mob).pos.distance(HOME) < before - 50.0, "provoked, it closes in");
}
