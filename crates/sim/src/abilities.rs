// Copyright (c) 2026 Daniel Bates / Bates LLC. All rights reserved.
//! Data-driven class ability kits (roadmap B03 + B09).
//!
//! Every ability is one row in [`ABILITIES`]: who may use it, from what level,
//! what it costs, how long it takes to cast and recharge, who it is aimed at
//! and a list of [`Fx`] it applies. The tick in `world.rs` interprets the rows;
//! nothing about a specific ability is hard-coded there. The client reads the
//! same table (it links this crate) to lay out the 10-slot action bar.
//!
//! This module is pure data + lookups (no threads, fs or clocks) so it stays
//! wasm32-compatible.

use antediluvia_protocol::Class;

/// Which pool a class spends on abilities.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Resource {
    /// Grows with level, regenerates slowly.
    Mana,
    /// Fixed pool of [`STAMINA_MAX`], regenerates quickly.
    Stamina,
}

impl Resource {
    pub fn as_str(self) -> &'static str {
        match self {
            Resource::Mana => "mana",
            Resource::Stamina => "stamina",
        }
    }
}

/// Size of every character's stamina pool.
pub const STAMINA_MAX: i32 = 100;
/// Action-bar width: keys 1-9 then 0.
pub const BAR_SLOTS: usize = 10;

/// Who an ability is aimed at.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Aim {
    /// Nearest hostile within `range`, in the half-circle in front.
    Enemy,
    /// Every hostile within `range` of the caster.
    Around,
    /// The caster only.
    Myself,
    /// The caster plus party members within `range`.
    Party,
}

/// Which gear bonus scales an ability's direct damage.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum School {
    Melee,
    Spell,
}

/// One effect of an ability. Hostile effects land on the aimed hostiles;
/// friendly effects land on the caster (or on the party for [`Aim::Party`]).
/// Percentages are whole numbers (25 = 25%).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Fx {
    // ── hostile ──
    /// Direct damage.
    Damage(i32),
    /// Damage every second for `secs`.
    Dot { dmg: i32, secs: f32 },
    /// Cannot move, attack or cast.
    Stun(f32),
    /// Cannot move.
    Root(f32),
    /// Moves `pct`% slower.
    Slow { pct: u32, secs: f32 },
    /// Forced to attack the caster.
    Taunt(f32),
    /// Deals `pct`% less damage.
    Weaken { pct: u32, secs: f32 },
    /// Takes `pct`% more damage.
    Expose { pct: u32, secs: f32 },
    // ── friendly ──
    /// Direct heal.
    Heal(i32),
    /// Heal every second for `secs`.
    Hot { heal: i32, secs: f32 },
    /// Deals `pct`% more damage.
    Might { pct: u32, secs: f32 },
    /// Takes `pct`% less damage.
    Guard { pct: u32, secs: f32 },
    /// Moves `pct`% faster.
    Haste { pct: u32, secs: f32 },
    /// Absorbs up to `amount` damage.
    Shield { amount: i32, secs: f32 },
    /// The caster regains this much of their class resource.
    Restore(i32),
}

impl Fx {
    /// True for effects that land on hostiles.
    pub fn hostile(&self) -> bool {
        matches!(
            self,
            Fx::Damage(_)
                | Fx::Dot { .. }
                | Fx::Stun(_)
                | Fx::Root(_)
                | Fx::Slow { .. }
                | Fx::Taunt(_)
                | Fx::Weaken { .. }
                | Fx::Expose { .. }
        )
    }
}

/// A lingering effect on an entity (the runtime form of the timed [`Fx`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuraKind {
    Dot,
    Hot,
    Stun,
    Root,
    Slow,
    Taunt,
    Weaken,
    Expose,
    Might,
    Guard,
    Haste,
    Shield,
}

pub struct Ability {
    pub id: &'static str,
    /// Display name.
    pub name: &'static str,
    pub class: Class,
    pub min_level: u32,
    /// Cost in the class resource (see [`class_resource`]).
    pub cost: i32,
    /// Seconds before this ability can be used again.
    pub cooldown: f32,
    /// Seconds of standing still before it fires (0 = instant).
    pub cast: f32,
    /// Reach in world units (aim radius for `Around` / `Party`).
    pub range: f32,
    pub aim: Aim,
    pub school: School,
    pub fx: &'static [Fx],
    /// One-line tooltip.
    pub desc: &'static str,
}

/// The pool a class spends.
pub fn class_resource(class: Class) -> Resource {
    match class {
        Class::Warrior | Class::Hunter | Class::Wanderer => Resource::Stamina,
        Class::Priest | Class::Mage | Class::Herald | Class::Warden | Class::Occultist => {
            Resource::Mana
        }
    }
}

/// Flat (max health, max mana) a class adds on top of the level-1 baseline.
pub fn class_base(class: Class) -> (i32, i32) {
    match class {
        Class::Warrior => (40, 0),
        Class::Hunter => (20, 15),
        Class::Priest => (10, 40),
        Class::Mage => (0, 50),
        Class::Wanderer => (15, 0),
        Class::Herald => (35, 20),
        Class::Warden => (10, 40),
        Class::Occultist => (0, 45),
    }
}

/// Display name and one-line pitch for the character builder.
pub fn class_blurb(class: Class) -> (&'static str, &'static str) {
    match class {
        Class::Warrior => ("Warrior", "Stamina. Front-line bruiser: bleeds, stuns, taunts."),
        Class::Hunter => ("Hunter", "Stamina. Bowman: stings, snares, marked prey."),
        Class::Priest => ("Priest", "Mana. Healer: mends, shields, rebukes."),
        Class::Mage => ("Mage", "Mana. Fire and frost: burst, roots, wards."),
        Class::Wanderer => ("Wanderer", "Stamina. Knife-fighter of Nod: bleeds, venom, ambush."),
        Class::Herald => ("Herald", "Mana. Armored preacher of righteousness: smites, shields, blesses."),
        Class::Warden => ("Warden", "Mana. Keeper of the ground: thorns, roots, healing rain."),
        Class::Occultist => ("Occultist", "Mana. Forbidden Watcher lore: curses, drains, dread."),
    }
}

const fn ab(
    id: &'static str,
    name: &'static str,
    class: Class,
    min_level: u32,
    cost: i32,
    cooldown: f32,
    cast: f32,
    range: f32,
    aim: Aim,
    school: School,
    fx: &'static [Fx],
    desc: &'static str,
) -> Ability {
    Ability { id, name, class, min_level, cost, cooldown, cast, range, aim, school, fx, desc }
}

use Aim::{Around, Enemy, Myself, Party};
use Class::{Herald, Hunter, Mage, Occultist, Priest, Wanderer, Warden, Warrior};
use School::{Melee, Spell};

/// Every ability in the game. Rows are grouped by class and ordered by unlock
/// level, which is also the action-bar order.
#[rustfmt::skip]
pub const ABILITIES: &[Ability] = &[
    // ── Warrior (stamina) ──
    ab("heroic_strike", "Heroic Strike", Warrior, 1, 20, 3.0, 0.0, 90.0, Enemy, Melee, &[Fx::Damage(22)], "A heavy blow."),
    ab("rend", "Rend", Warrior, 2, 15, 6.0, 0.0, 90.0, Enemy, Melee, &[Fx::Damage(6), Fx::Dot { dmg: 5, secs: 9.0 }], "Open a wound that bleeds for 9s."),
    ab("whirlwind", "Whirlwind", Warrior, 4, 30, 8.0, 0.0, 140.0, Around, Melee, &[Fx::Damage(16)], "Strike every foe around you."),
    ab("challenging_roar", "Challenging Roar", Warrior, 6, 10, 10.0, 0.0, 260.0, Around, Melee, &[Fx::Taunt(6.0)], "Nearby foes attack you for 6s."),
    ab("brace", "Brace", Warrior, 8, 15, 30.0, 0.0, 0.0, Myself, Melee, &[Fx::Guard { pct: 40, secs: 8.0 }], "Take 40% less damage for 8s."),
    ab("hamstring", "Hamstring", Warrior, 10, 15, 8.0, 0.0, 90.0, Enemy, Melee, &[Fx::Damage(8), Fx::Slow { pct: 50, secs: 8.0 }], "Cripple a foe: 50% slower for 8s."),
    ab("skull_crack", "Skull Crack", Warrior, 12, 20, 14.0, 0.0, 90.0, Enemy, Melee, &[Fx::Damage(14), Fx::Stun(3.0)], "Stun a foe for 3s."),
    ab("war_cry", "War Cry", Warrior, 14, 25, 45.0, 0.0, 400.0, Party, Melee, &[Fx::Might { pct: 20, secs: 15.0 }], "You and your party deal 20% more damage for 15s."),
    // ── Hunter (stamina) ──
    ab("aimed_shot", "Aimed Shot", Hunter, 1, 20, 4.0, 0.0, 320.0, Enemy, Melee, &[Fx::Damage(20)], "A careful long shot."),
    ab("viper_sting", "Viper Sting", Hunter, 2, 15, 6.0, 0.0, 320.0, Enemy, Melee, &[Fx::Dot { dmg: 5, secs: 10.0 }], "A venomed arrow: damage over 10s."),
    ab("multi_shot", "Multi-Shot", Hunter, 4, 30, 8.0, 0.0, 160.0, Around, Melee, &[Fx::Damage(12)], "Loose at every foe nearby."),
    ab("snare", "Snare", Hunter, 6, 15, 15.0, 0.0, 140.0, Around, Melee, &[Fx::Root(5.0)], "Root nearby foes for 5s."),
    ab("staggering_shot", "Staggering Shot", Hunter, 8, 20, 12.0, 0.0, 320.0, Enemy, Melee, &[Fx::Damage(8), Fx::Slow { pct: 50, secs: 6.0 }], "Slow a foe by 50% for 6s."),
    ab("field_dressing", "Field Dressing", Hunter, 10, 20, 20.0, 1.5, 0.0, Myself, Melee, &[Fx::Heal(40)], "Bind your wounds (1.5s cast)."),
    ab("hunters_mark", "Hunter's Mark", Hunter, 12, 10, 20.0, 0.0, 360.0, Enemy, Melee, &[Fx::Expose { pct: 20, secs: 15.0 }], "Marked prey takes 20% more damage for 15s."),
    ab("volley", "Volley", Hunter, 14, 40, 25.0, 2.0, 320.0, Enemy, Melee, &[Fx::Damage(60)], "A drawn-out killing shot (2s cast)."),
    // ── Priest (mana) ──
    ab("smite", "Smite", Priest, 1, 6, 3.0, 0.0, 260.0, Enemy, Spell, &[Fx::Damage(16)], "Strike with holy light."),
    ab("heal", "Heal", Priest, 2, 12, 5.0, 0.0, 0.0, Myself, Spell, &[Fx::Heal(35)], "Mend your wounds."),
    ab("word_of_reproof", "Word of Reproof", Priest, 4, 10, 8.0, 0.0, 260.0, Enemy, Spell, &[Fx::Damage(8), Fx::Dot { dmg: 4, secs: 8.0 }], "Searing words burn for 8s."),
    ab("renewal", "Renewal", Priest, 6, 14, 10.0, 0.0, 0.0, Myself, Spell, &[Fx::Hot { heal: 6, secs: 12.0 }], "Heal steadily for 12s."),
    ab("covering", "Covering", Priest, 8, 18, 20.0, 0.0, 0.0, Myself, Spell, &[Fx::Shield { amount: 60, secs: 15.0 }], "A shield that absorbs 60 damage."),
    ab("intercession", "Intercession", Priest, 10, 28, 15.0, 2.0, 400.0, Party, Spell, &[Fx::Heal(45)], "Heal yourself and your party (2s cast)."),
    ab("rebuke", "Rebuke", Priest, 12, 16, 20.0, 0.0, 200.0, Enemy, Spell, &[Fx::Stun(4.0)], "Silence a foe in awe: stunned 4s."),
    ab("benediction", "Benediction", Priest, 14, 30, 60.0, 0.0, 400.0, Party, Spell, &[Fx::Guard { pct: 25, secs: 12.0 }, Fx::Hot { heal: 5, secs: 12.0 }], "Party takes 25% less damage and mends for 12s."),
    // ── Mage (mana) ──
    ab("firebolt", "Firebolt", Mage, 1, 6, 2.5, 0.0, 280.0, Enemy, Spell, &[Fx::Damage(24)], "Hurl a bolt of fire."),
    ab("frost_shard", "Frost Shard", Mage, 2, 8, 4.0, 0.0, 280.0, Enemy, Spell, &[Fx::Damage(14), Fx::Slow { pct: 40, secs: 5.0 }], "Chill a foe: 40% slower for 5s."),
    ab("frost_nova", "Frost Nova", Mage, 4, 15, 10.0, 0.0, 150.0, Around, Spell, &[Fx::Damage(12), Fx::Root(4.0)], "Freeze nearby foes in place for 4s."),
    ab("ward_of_glyphs", "Ward of Glyphs", Mage, 6, 16, 25.0, 0.0, 0.0, Myself, Spell, &[Fx::Shield { amount: 50, secs: 20.0 }], "A ward that absorbs 50 damage."),
    ab("kindle", "Kindle", Mage, 8, 12, 8.0, 0.0, 280.0, Enemy, Spell, &[Fx::Dot { dmg: 8, secs: 8.0 }], "Set a foe alight for 8s."),
    ab("quicken", "Quicken", Mage, 10, 10, 20.0, 0.0, 0.0, Myself, Spell, &[Fx::Haste { pct: 40, secs: 6.0 }], "Move 40% faster for 6s."),
    ab("pyre_blast", "Pyre Blast", Mage, 12, 24, 12.0, 2.5, 300.0, Enemy, Spell, &[Fx::Damage(70)], "A devastating blast (2.5s cast)."),
    ab("rain_of_brimstone", "Rain of Brimstone", Mage, 14, 40, 30.0, 2.0, 220.0, Around, Spell, &[Fx::Damage(30), Fx::Dot { dmg: 6, secs: 6.0 }], "Fire falls on every foe nearby (2s cast)."),
    // ── Wanderer (stamina): knife-fighter of the land of Nod ──
    ab("shiv", "Shiv", Wanderer, 1, 20, 2.0, 0.0, 80.0, Enemy, Melee, &[Fx::Damage(18)], "A quick stab."),
    ab("lacerate", "Lacerate", Wanderer, 2, 20, 8.0, 0.0, 80.0, Enemy, Melee, &[Fx::Damage(6), Fx::Dot { dmg: 6, secs: 8.0 }], "A ragged cut that bleeds for 8s."),
    ab("blindside", "Blindside", Wanderer, 4, 25, 14.0, 0.0, 80.0, Enemy, Melee, &[Fx::Damage(10), Fx::Stun(3.0)], "Daze a foe for 3s."),
    ab("slip_away", "Slip Away", Wanderer, 6, 15, 30.0, 0.0, 0.0, Myself, Melee, &[Fx::Guard { pct: 50, secs: 5.0 }, Fx::Haste { pct: 30, secs: 5.0 }], "Dodge and dart: 50% less damage, 30% faster for 5s."),
    ab("venom_edge", "Venom Edge", Wanderer, 8, 20, 12.0, 0.0, 80.0, Enemy, Melee, &[Fx::Dot { dmg: 9, secs: 10.0 }, Fx::Weaken { pct: 15, secs: 10.0 }], "Poison that saps strength for 10s."),
    ab("fan_of_flint", "Fan of Flint", Wanderer, 10, 35, 10.0, 0.0, 130.0, Around, Melee, &[Fx::Damage(18)], "Fling blades at every foe nearby."),
    ab("mark_of_nod", "Mark of Nod", Wanderer, 12, 15, 20.0, 0.0, 80.0, Enemy, Melee, &[Fx::Expose { pct: 25, secs: 12.0 }], "The marked foe takes 25% more damage for 12s."),
    ab("kinslayers_strike", "Kinslayer's Strike", Wanderer, 14, 45, 20.0, 0.0, 80.0, Enemy, Melee, &[Fx::Damage(65)], "One ruinous blow."),
    // ── Herald (mana): armored preacher of righteousness ──
    ab("righteous_strike", "Righteous Strike", Herald, 1, 6, 3.0, 0.0, 90.0, Enemy, Melee, &[Fx::Damage(20)], "A blow struck in conviction."),
    ab("mending_touch", "Mending Touch", Herald, 2, 12, 6.0, 0.0, 0.0, Myself, Spell, &[Fx::Heal(30)], "Mend your wounds."),
    ab("judgment", "Judgment", Herald, 4, 10, 8.0, 0.0, 240.0, Enemy, Spell, &[Fx::Damage(18), Fx::Weaken { pct: 20, secs: 8.0 }], "The judged deal 20% less damage for 8s."),
    ab("summons", "Summons", Herald, 6, 8, 10.0, 0.0, 260.0, Around, Spell, &[Fx::Taunt(6.0)], "Call nearby foes to face you for 6s."),
    ab("hallowed_ground", "Hallowed Ground", Herald, 8, 18, 12.0, 0.0, 140.0, Around, Spell, &[Fx::Damage(10), Fx::Dot { dmg: 4, secs: 8.0 }], "The ground burns the wicked for 8s."),
    ab("aegis_of_faith", "Aegis of Faith", Herald, 10, 20, 40.0, 0.0, 0.0, Myself, Spell, &[Fx::Shield { amount: 80, secs: 10.0 }], "A shield that absorbs 80 damage."),
    ab("staff_of_reproof", "Staff of Reproof", Herald, 12, 14, 20.0, 0.0, 90.0, Enemy, Melee, &[Fx::Damage(10), Fx::Stun(4.0)], "Strike a foe senseless for 4s."),
    ab("blessing_of_seth", "Blessing of Seth", Herald, 14, 26, 45.0, 0.0, 400.0, Party, Spell, &[Fx::Might { pct: 15, secs: 20.0 }, Fx::Guard { pct: 15, secs: 20.0 }], "Party deals 15% more and takes 15% less for 20s."),
    // ── Warden (mana): keeper of the ground ──
    ab("thorn_lash", "Thorn Lash", Warden, 1, 6, 3.0, 0.0, 260.0, Enemy, Spell, &[Fx::Damage(12), Fx::Dot { dmg: 2, secs: 6.0 }], "Whip a foe with thorns."),
    ab("regrowth", "Regrowth", Warden, 2, 12, 6.0, 1.5, 0.0, Myself, Spell, &[Fx::Heal(25), Fx::Hot { heal: 4, secs: 9.0 }], "Heal now and over 9s (1.5s cast)."),
    ab("entangling_roots", "Entangling Roots", Warden, 4, 12, 12.0, 0.0, 260.0, Enemy, Spell, &[Fx::Root(6.0), Fx::Dot { dmg: 3, secs: 6.0 }], "Roots hold and tear a foe for 6s."),
    ab("locust_swarm", "Locust Swarm", Warden, 6, 14, 10.0, 0.0, 260.0, Enemy, Spell, &[Fx::Dot { dmg: 7, secs: 12.0 }], "A swarm devours a foe for 12s."),
    ab("barkskin", "Barkskin", Warden, 8, 14, 30.0, 0.0, 0.0, Myself, Spell, &[Fx::Guard { pct: 30, secs: 12.0 }], "Take 30% less damage for 12s."),
    ab("briar_ring", "Briar Ring", Warden, 10, 22, 14.0, 0.0, 150.0, Around, Spell, &[Fx::Damage(14), Fx::Slow { pct: 50, secs: 6.0 }], "Briars tear and slow nearby foes."),
    ab("gentle_rain", "Gentle Rain", Warden, 12, 30, 20.0, 0.0, 400.0, Party, Spell, &[Fx::Hot { heal: 8, secs: 10.0 }], "Rain mends you and your party for 10s."),
    ab("earthshaker", "Earthshaker", Warden, 14, 34, 25.0, 2.0, 200.0, Around, Spell, &[Fx::Damage(40), Fx::Stun(2.0)], "The ground heaves under nearby foes (2s cast)."),
    // ── Occultist (mana): forbidden lore of the Watchers ──
    ab("umbral_bolt", "Umbral Bolt", Occultist, 1, 7, 2.5, 1.5, 280.0, Enemy, Spell, &[Fx::Damage(28)], "A bolt of shadow (1.5s cast)."),
    ab("wasting_curse", "Wasting Curse", Occultist, 2, 10, 6.0, 0.0, 280.0, Enemy, Spell, &[Fx::Dot { dmg: 6, secs: 12.0 }], "The cursed waste away over 12s."),
    ab("siphon_life", "Siphon Life", Occultist, 4, 14, 8.0, 0.0, 240.0, Enemy, Spell, &[Fx::Damage(14), Fx::Heal(14)], "Drain a foe's life into your own."),
    ab("dread", "Dread", Occultist, 6, 14, 18.0, 0.0, 200.0, Enemy, Spell, &[Fx::Stun(4.0)], "A foe freezes in terror for 4s."),
    ab("curse_of_frailty", "Curse of Frailty", Occultist, 8, 10, 15.0, 0.0, 280.0, Enemy, Spell, &[Fx::Weaken { pct: 25, secs: 12.0 }], "The cursed deal 25% less damage for 12s."),
    ab("watchers_whisper", "Watcher's Whisper", Occultist, 10, 0, 30.0, 0.0, 0.0, Myself, Spell, &[Fx::Restore(40)], "Forbidden counsel restores 40 mana."),
    ab("hex_of_unmaking", "Hex of Unmaking", Occultist, 12, 12, 20.0, 0.0, 280.0, Enemy, Spell, &[Fx::Expose { pct: 25, secs: 12.0 }], "The hexed take 25% more damage for 12s."),
    ab("fires_of_hermon", "Fires of Hermon", Occultist, 14, 38, 25.0, 2.0, 180.0, Around, Spell, &[Fx::Damage(34), Fx::Dot { dmg: 8, secs: 6.0 }], "Unholy fire engulfs nearby foes (2s cast)."),
];

pub fn ability(id: &str) -> Option<&'static Ability> {
    ABILITIES.iter().find(|a| a.id == id)
}

/// A class's abilities in action-bar order (unlock level, then table order).
pub fn kit(class: Class) -> Vec<&'static Ability> {
    let mut v: Vec<&'static Ability> = ABILITIES.iter().filter(|a| a.class == class).collect();
    v.sort_by_key(|a| a.min_level);
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn eight_classes_each_with_a_full_kit() {
        assert_eq!(Class::ALL.len(), 8);
        for c in Class::ALL {
            let k = kit(c);
            assert!(k.len() >= 8, "{c:?} has only {} abilities", k.len());
            assert!(k.len() <= BAR_SLOTS, "{c:?} overflows the action bar");
            assert_eq!(k[0].min_level, 1, "{c:?} needs a level-1 ability");
            assert!(k.windows(2).all(|w| w[0].min_level <= w[1].min_level));
        }
    }

    #[test]
    fn table_rows_are_well_formed() {
        let mut seen = std::collections::HashSet::new();
        for a in ABILITIES {
            assert!(seen.insert(a.id), "duplicate ability id {}", a.id);
            assert!(!a.fx.is_empty(), "{} does nothing", a.id);
            assert!(!a.name.is_empty() && !a.desc.is_empty(), "{} lacks text", a.id);
            assert!(a.cost >= 0 && a.cooldown > 0.0 && a.cast >= 0.0, "{} bad numbers", a.id);
            let hostile = a.fx.iter().any(Fx::hostile);
            match a.aim {
                Aim::Enemy | Aim::Around => {
                    assert!(hostile, "{} aims at foes but has no hostile effect", a.id);
                    assert!(a.range > 0.0, "{} needs a range", a.id);
                }
                Aim::Myself => assert!(!hostile, "{} would harm its caster", a.id),
                Aim::Party => {
                    assert!(!hostile, "{} would harm the party", a.id);
                    assert!(a.range > 0.0, "{} needs a range", a.id);
                }
            }
            if class_resource(a.class) == Resource::Stamina {
                assert!(a.cost <= STAMINA_MAX, "{} costs more than a full pool", a.id);
            }
        }
    }

    /// Every kit offers the classic toolbox: damage plus at least one of a
    /// damage-over-time, a crowd-control and a defensive/supportive effect.
    #[test]
    fn every_kit_has_variety() {
        for c in Class::ALL {
            let fx: Vec<&Fx> = kit(c).iter().flat_map(|a| a.fx.iter()).collect();
            let has = |f: &dyn Fn(&Fx) -> bool| fx.iter().any(|x| f(x));
            assert!(has(&|x| matches!(x, Fx::Damage(_))), "{c:?} no damage");
            assert!(has(&|x| matches!(x, Fx::Dot { .. })), "{c:?} no DoT");
            assert!(
                has(&|x| matches!(x, Fx::Stun(_) | Fx::Root(_) | Fx::Slow { .. } | Fx::Taunt(_))),
                "{c:?} no crowd control"
            );
            assert!(has(&|x| !x.hostile()), "{c:?} no friendly effect");
        }
    }
}
