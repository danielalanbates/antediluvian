// Copyright (c) 2026 Daniel Bates / Bates LLC. All rights reserved.
//! B06 (sim side): item rarity tiers, durability and vendor repair.
//!
//! * `rarity_of` classifies every item id into the six classic tiers.
//! * Durable gear (weapon / chest / legs / head / wrist) wears: a tenth of
//!   its maximum on each death, and a point now and then from swinging
//!   (weapon) or being struck (armour). At zero an item is *broken* and its
//!   melee/spell bonus stops counting until repaired.
//! * Wear is stored on the sheet as `durability: item id -> points left`
//!   (absent = pristine). It is keyed by item id, not by slot, so swapping,
//!   banking or re-equipping a piece never restores it - only `World::repair`
//!   at the inn does, for gold that leaves the economy (`gold_sunk`).
//!
//! Nothing here creates items or gold.

use crate::world::{item_def, mount_tier, Entity, World, CONSUMABLES, INN_RADIUS, KEY_ITEMS};
use antediluvia_protocol::{Act, CharacterSheet, EntityId, ItemInfo, Rarity};

/// Slots whose items wear. Jewellery, cloaks and mounts never do.
pub const DURABLE_SLOTS: &[&str] = &["weapon", "chest", "legs", "head", "wrist"];
/// Share of maximum durability lost by every worn piece on death.
pub const DEATH_LOSS_PCT: u32 = 10;
/// One swing in this many (on average) costs the weapon a point.
pub const WEAPON_WEAR_ODDS: u64 = 12;
/// One blow taken in this many (on average) costs an armour piece a point.
pub const ARMOR_WEAR_ODDS: u64 = 8;

fn tier(r: Rarity) -> u32 {
    Rarity::ALL.iter().position(|x| *x == r).unwrap_or(1) as u32
}

/// Quality tier of any item id.
pub fn rarity_of(item: &str) -> Rarity {
    // Hand-placed pieces first.
    match item {
        "warlords_star_blade" => return Rarity::Legendary,
        "lineage_mantle" | "covenant_signet" | "dire_wolf_horn" => return Rarity::Epic,
        "golden_apple" => return Rarity::Rare,
        "hearthstone" | "taming_lasso" => return Rarity::Uncommon,
        _ => {}
    }
    if let Some(species) = item.strip_prefix("mount:") {
        return if mount_tier(species).0 >= 1.7 { Rarity::Epic } else { Rarity::Rare };
    }
    if let Some(d) = item_def(item) {
        let budget = d.melee + d.spell + d.hp / 5;
        return match budget {
            i32::MIN..=6 => Rarity::Common,
            7..=11 => Rarity::Uncommon,
            12..=16 => Rarity::Rare,
            17..=25 => Rarity::Epic,
            _ => Rarity::Legendary,
        };
    }
    if item.ends_with("_trophy") {
        return Rarity::Poor;
    }
    if CONSUMABLES.contains(&item) || KEY_ITEMS.contains(&item) || item.starts_with("gq_") || item.ends_with("_proof") {
        return Rarity::Common;
    }
    match crate::economy::vendor_price(item) {
        Some(0..=2) => Rarity::Poor,
        Some(3..=24) | None => Rarity::Common,
        Some(25..=99) => Rarity::Uncommon,
        Some(100..=399) => Rarity::Rare,
        Some(_) => Rarity::Epic,
    }
}

/// Maximum durability of an item; 0 means the item never wears.
pub fn max_durability(item: &str) -> u32 {
    match item_def(item) {
        Some(d) if DURABLE_SLOTS.contains(&d.slot) => 40 + 20 * tier(rarity_of(item)),
        _ => 0,
    }
}

/// Durability points an item has left on this sheet.
pub fn current(sheet: &CharacterSheet, item: &str) -> u32 {
    let max = max_durability(item);
    sheet.durability.get(item).copied().unwrap_or(max).min(max)
}

/// A durable item worn down to nothing: its bonuses stop counting.
pub fn is_broken(sheet: &CharacterSheet, item: &str) -> bool {
    // Review: this runs for every worn item of every player on every sim
    // tick (via `gear_bonus`), so the map probe comes first - a pristine
    // item (no entry) never reaches the item-table scans below.
    sheet.durability.get(item).is_some_and(|d| *d == 0) && max_durability(item) > 0
}

/// Take `pts` off an item (never below zero). Returns the points actually lost.
pub fn lose(sheet: &mut CharacterSheet, item: &str, pts: u32) -> u32 {
    let max = max_durability(item);
    if max == 0 || pts == 0 {
        return 0;
    }
    let cur = current(sheet, item);
    let new = cur.saturating_sub(pts);
    sheet.durability.insert(item.to_string(), new);
    cur - new
}

fn worn_durables(sheet: &CharacterSheet) -> Vec<String> {
    sheet
        .equipment
        .iter()
        .filter(|(slot, item)| DURABLE_SLOTS.contains(&slot.as_str()) && max_durability(item) > 0)
        .map(|(_, item)| item.clone())
        .collect()
}

/// Death (hook in `step_zone`): every worn durable piece loses a tenth.
pub fn on_death(e: &mut Entity) {
    let Some(s) = e.sheet.as_mut() else { return };
    for item in worn_durables(s) {
        let max = max_durability(&item);
        lose(s, &item, (max * DEATH_LOSS_PCT).div_ceil(100));
    }
}

/// Cheap deterministic mixer (the sim has no RNG to spare at the hook sites).
fn mix(a: u64, b: u64) -> u64 {
    let mut z = a.wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ b.wrapping_mul(0xD1B5_4A32_D192_ED03);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// A player landed a blow (hook): the weapon may lose a point.
pub fn wear_weapon(e: &mut Entity, tick: u64) {
    let id = e.id;
    let Some(s) = e.sheet.as_mut() else { return };
    if mix(tick, id) % WEAPON_WEAR_ODDS != 0 {
        return;
    }
    if let Some(w) = s.equipment.get("weapon").cloned() {
        lose(s, &w, 1);
    }
}

/// A player was struck (hook): one armour piece may lose a point.
pub fn wear_armor(e: &mut Entity, tick: u64) {
    let id = e.id;
    let Some(s) = e.sheet.as_mut() else { return };
    let roll = mix(tick ^ 0x5EED, id);
    if roll % ARMOR_WEAR_ODDS != 0 {
        return;
    }
    let armor: Vec<String> = s
        .equipment
        .iter()
        .filter(|(slot, item)| slot.as_str() != "weapon" && DURABLE_SLOTS.contains(&slot.as_str()) && max_durability(item) > 0)
        .map(|(_, item)| item.clone())
        .collect();
    if armor.is_empty() {
        return;
    }
    let pick = &armor[((roll / ARMOR_WEAR_ODDS) % armor.len() as u64) as usize];
    lose(s, pick, 1);
}

fn point_price(r: Rarity) -> u32 {
    [1, 2, 3, 5, 8, 12][tier(r) as usize]
}

fn possesses(sheet: &CharacterSheet, item: &str) -> bool {
    sheet.equipment.values().any(|i| i == item)
        || sheet.inventory.iter().any(|i| i == item)
        || sheet.bank.iter().any(|i| i == item)
}

/// Gold to restore every worn item the character still owns.
pub fn repair_cost(sheet: &CharacterSheet) -> u32 {
    let weighted: u32 = sheet
        .durability
        .iter()
        .filter(|(item, _)| possesses(sheet, item))
        .map(|(item, cur)| max_durability(item).saturating_sub(*cur) * point_price(rarity_of(item)))
        .sum();
    weighted.div_ceil(10)
}

fn info(sheet: &CharacterSheet, item: &str, slot: Option<&str>) -> ItemInfo {
    let rarity = rarity_of(item);
    ItemInfo {
        item: item.to_string(),
        rarity,
        color: rarity.color().to_string(),
        slot: slot.map(str::to_string),
        durability: current(sheet, item),
        max_durability: max_durability(item),
    }
}

/// Rarity + wear for everything worn then carried, plus the repair quote.
pub fn gear_report(sheet: &CharacterSheet) -> (Vec<ItemInfo>, u32) {
    let mut out: Vec<ItemInfo> = sheet.equipment.iter().map(|(slot, item)| info(sheet, item, Some(slot))).collect();
    out.extend(sheet.inventory.iter().map(|item| info(sheet, item, None)));
    (out, repair_cost(sheet))
}

impl World {
    /// Rarity/durability report for a live player.
    pub fn gear_info(&self, act: Act, id: EntityId) -> Option<(Vec<ItemInfo>, u32)> {
        let e = self.zones.get(&act)?.entities.get(&id)?;
        e.sheet.as_ref().map(gear_report)
    }

    /// Repair all worn gear at the inn. The fee leaves the economy.
    pub fn repair(&mut self, act: Act, id: EntityId) -> Result<String, String> {
        let entry = self.zones.get(&act).ok_or("no zone")?.entry;
        let e = self.zones.get_mut(&act).and_then(|z| z.entities.get_mut(&id)).ok_or("no player")?;
        if e.health <= 0 {
            return Err("You are dead.".into());
        }
        if e.pos.distance(entry) > INN_RADIUS {
            return Err("The Innkeeper mends gear only at the inn.".into());
        }
        let s = e.sheet.as_mut().ok_or("no sheet")?;
        let cost = repair_cost(s);
        if cost == 0 {
            return Ok("Nothing you carry needs mending.".into());
        }
        if s.gold < cost {
            return Err(format!("Repairs cost {cost} gold; you have {}.", s.gold));
        }
        s.gold -= cost;
        let owned: Vec<String> = s.durability.keys().filter(|i| possesses(s, i)).cloned().collect();
        for item in owned {
            s.durability.remove(&item);
        }
        self.gold_sunk += cost as u64;
        Ok(format!("The Innkeeper mends your gear for {cost}g."))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::{new_character, ITEMS};
    use antediluvia_protocol::EntityKind;
    use glam::Vec2;

    fn geared() -> CharacterSheet {
        let mut s = new_character("Smith");
        s.equipment.insert("weapon".into(), "bronze_sword".into());
        s.equipment.insert("legs".into(), "iron_greaves".into());
        s.equipment.insert("neck".into(), "luminous_charm".into());
        s.x = 0.0;
        s.y = 0.0;
        s
    }

    fn totals(s: &CharacterSheet) -> (usize, usize, usize) {
        (s.inventory.len(), s.equipment.len(), s.bank.len())
    }

    #[test]
    fn rarity_tiers_cover_every_kind_of_item() {
        assert_eq!(rarity_of("serpent_trophy"), Rarity::Poor);
        assert_eq!(rarity_of("bread"), Rarity::Common);
        assert_eq!(rarity_of("animal_skins"), Rarity::Common);
        assert_eq!(rarity_of("bronze_sword"), Rarity::Uncommon);
        assert_eq!(rarity_of("astrologers_staff"), Rarity::Rare);
        assert_eq!(rarity_of("lineage_blade"), Rarity::Epic);
        assert_eq!(rarity_of("warlords_star_blade"), Rarity::Legendary);
        assert_eq!(rarity_of("mount:starving_smilodon"), Rarity::Epic);
        assert_eq!(rarity_of("mount:plains_auroch"), Rarity::Rare);
        // Every tier is reachable, colours are distinct, order is ascending.
        let mut seen = std::collections::BTreeSet::new();
        for i in ITEMS {
            seen.insert(rarity_of(i.id));
        }
        seen.insert(rarity_of("x_trophy"));
        assert_eq!(seen.len(), 6, "all six tiers in use: {seen:?}");
        let colors: std::collections::BTreeSet<_> = Rarity::ALL.iter().map(|r| r.color()).collect();
        assert_eq!(colors.len(), 6);
        assert!(Rarity::Poor < Rarity::Common && Rarity::Epic < Rarity::Legendary);
        // Better gear lasts longer; jewellery never wears.
        assert!(max_durability("warlords_star_blade") > max_durability("bronze_sword"));
        assert_eq!(max_durability("bronze_sword"), 80);
        assert_eq!(max_durability("animal_skins"), 60);
        assert_eq!(max_durability("luminous_charm"), 0);
        assert_eq!(max_durability("bread"), 0);
        // The wire form is lowercase.
        assert_eq!(serde_json::to_string(&Rarity::Epic).unwrap(), "\"epic\"");
    }

    #[test]
    fn death_takes_a_tenth_and_never_goes_negative() {
        let mut w = World::new(3);
        let id = w.spawn_player(1, geared());
        let before = w.player_sheet(Act::Eden, id).unwrap();
        for n in 1..=12u32 {
            on_death(w.zones.get_mut(&Act::Eden).unwrap().entities.get_mut(&id).unwrap());
            let s = w.player_sheet(Act::Eden, id).unwrap();
            assert_eq!(current(&s, "bronze_sword"), 80u32.saturating_sub(8 * n));
            assert_eq!(current(&s, "animal_skins"), 60u32.saturating_sub(6 * n));
            assert!(!s.durability.contains_key("luminous_charm"), "jewellery must not wear");
        }
        let s = w.player_sheet(Act::Eden, id).unwrap();
        assert!(is_broken(&s, "bronze_sword") && is_broken(&s, "animal_skins"));
        assert_eq!(totals(&s), totals(&before), "death destroys no items");
        assert_eq!(s.gold, before.gold);
    }

    #[test]
    fn dying_in_the_world_wears_gear() {
        let mut w = World::new(9);
        let foe = w.zones[&Act::Eden].entities.values().find(|e| e.kind == EntityKind::Enemy && e.damage > 0 && e.speed > 0.0).map(|e| e.pos).unwrap();
        let mut s = geared();
        s.x = foe.x - 10.0;
        s.y = foe.y;
        s.health = 1;
        let id = w.spawn_player(1, s);
        let mut died = false;
        for _ in 0..600 {
            w.step();
            if w.zones[&Act::Eden].entities[&id].health <= 0 {
                died = true;
                break;
            }
        }
        assert!(died, "a 1-HP exile beside a hostile should fall");
        let s = w.player_sheet(Act::Eden, id).unwrap();
        assert!(current(&s, "animal_skins") <= 54, "death hook ran: {:?}", s.durability);
        assert!(current(&s, "bronze_sword") <= 72);
    }

    #[test]
    fn combat_wears_weapon_and_armour_slowly() {
        let mut w = World::new(4);
        let id = w.spawn_player(1, geared());
        for tick in 0..2400 {
            let e = w.zones.get_mut(&Act::Eden).unwrap().entities.get_mut(&id).unwrap();
            wear_weapon(e, tick);
            wear_armor(e, tick);
        }
        let s = w.player_sheet(Act::Eden, id).unwrap();
        let weapon_lost = 80 - current(&s, "bronze_sword");
        let armour_lost = (60 - current(&s, "animal_skins")) + (max_durability("iron_greaves") - current(&s, "iron_greaves"));
        // 2400 blows: ~200 weapon points and ~300 armour points expected,
        // clamped by the items' maxima.
        assert_eq!(weapon_lost, 80, "weapon should be worn out after 2400 swings");
        assert!(armour_lost >= 100, "armour lost {armour_lost}");
        // A handful of blows barely scratches it.
        let mut w = World::new(4);
        let id = w.spawn_player(1, geared());
        for tick in 0..10 {
            let e = w.zones.get_mut(&Act::Eden).unwrap().entities.get_mut(&id).unwrap();
            wear_weapon(e, tick);
            wear_armor(e, tick);
        }
        let s = w.player_sheet(Act::Eden, id).unwrap();
        assert!(current(&s, "bronze_sword") >= 70 && current(&s, "animal_skins") >= 50);
        // Mobs (no sheet) are untouched by the hooks.
        let mob = w.zones[&Act::Eden].entities.values().find(|e| e.kind == EntityKind::Enemy).unwrap().id;
        let e = w.zones.get_mut(&Act::Eden).unwrap().entities.get_mut(&mob).unwrap();
        wear_weapon(e, 0);
        wear_armor(e, 0);
        on_death(e);
    }

    /// One melee swing against a dummy: the damage a weapon adds.
    fn swing_damage(broken: bool) -> i32 {
        let mut w = World::new(12);
        let (eid, epos) = {
            let e = w.zones[&Act::Eden].entities.values().find(|e| e.kind == EntityKind::Enemy).unwrap();
            (e.id, e.pos)
        };
        let mut s = geared();
        s.x = epos.x - 12.0;
        s.y = epos.y;
        s.max_health = 100_000;
        s.health = 100_000;
        if broken {
            s.durability.insert("bronze_sword".into(), 0);
        }
        let id = w.spawn_player(1, s);
        {
            let m = w.zones.get_mut(&Act::Eden).unwrap().entities.get_mut(&eid).unwrap();
            m.max_health = 1_000_000;
            m.health = 1_000_000;
        }
        for _ in 0..40 {
            let mpos = w.zones[&Act::Eden].entities[&eid].pos;
            let p = w.zones.get_mut(&Act::Eden).unwrap().entities.get_mut(&id).unwrap();
            p.pos = mpos - Vec2::new(12.0, 0.0);
            p.rot = 0.0;
            w.queue_attack(Act::Eden, id);
            w.step();
            let hp = w.zones[&Act::Eden].entities[&eid].health;
            if hp < 1_000_000 {
                return 1_000_000 - hp;
            }
        }
        panic!("the swing never landed");
    }

    #[test]
    fn a_broken_weapon_adds_no_damage() {
        let whole = swing_damage(false);
        let broken = swing_damage(true);
        assert_eq!(whole - broken, 8, "bronze_sword is +8 melee: whole {whole}, broken {broken}");
    }

    #[test]
    fn repair_charges_exactly_once_and_creates_nothing() {
        let mut w = World::new(5);
        let mut s = geared();
        s.gold = 100;
        s.inventory.push("stone_axe".into());
        s.bank.push("hide_vest".into());
        let id = w.spawn_player(1, s);
        for _ in 0..3 {
            on_death(w.zones.get_mut(&Act::Eden).unwrap().entities.get_mut(&id).unwrap());
        }
        let before = w.player_sheet(Act::Eden, id).unwrap();
        // sword 24 pts x3 (uncommon) + skins 18 x2 + greaves 18 x2 = 144 -> 15g.
        let quote = repair_cost(&before);
        assert_eq!(quote, 15);
        assert_eq!(w.gear_info(Act::Eden, id).unwrap().1, quote);
        let sunk0 = w.gold_sunk;
        let minted0 = w.gold_minted;
        // Away from the inn: refused, nothing changes.
        w.zones.get_mut(&Act::Eden).unwrap().entities.get_mut(&id).unwrap().pos = Vec2::new(900.0, 0.0);
        assert!(w.repair(Act::Eden, id).is_err());
        assert_eq!(w.player_sheet(Act::Eden, id).unwrap().gold, 100);
        w.zones.get_mut(&Act::Eden).unwrap().entities.get_mut(&id).unwrap().pos = Vec2::ZERO;
        w.repair(Act::Eden, id).unwrap();
        let after = w.player_sheet(Act::Eden, id).unwrap();
        assert_eq!(after.gold, 100 - quote);
        assert_eq!(w.gold_sunk, sunk0 + quote as u64, "the fee is a sink");
        assert_eq!(w.gold_minted, minted0, "repair mints nothing");
        assert!(after.durability.is_empty());
        assert_eq!(after.inventory, before.inventory);
        assert_eq!(after.equipment, before.equipment);
        assert_eq!(after.bank, before.bank);
        // Second repair is free and changes nothing.
        w.repair(Act::Eden, id).unwrap();
        assert_eq!(w.player_sheet(Act::Eden, id).unwrap().gold, 100 - quote);
        assert_eq!(w.gold_sunk, sunk0 + quote as u64);
    }

    #[test]
    fn repair_needs_the_gold_and_fails_cleanly() {
        let mut w = World::new(6);
        let mut s = geared();
        s.gold = 3;
        let id = w.spawn_player(1, s);
        for _ in 0..3 {
            on_death(w.zones.get_mut(&Act::Eden).unwrap().entities.get_mut(&id).unwrap());
        }
        let before = w.player_sheet(Act::Eden, id).unwrap();
        let err = w.repair(Act::Eden, id).unwrap_err();
        assert!(err.contains("cost"), "{err}");
        let after = w.player_sheet(Act::Eden, id).unwrap();
        assert_eq!(after.gold, 3);
        assert_eq!(after.durability, before.durability, "a failed repair mends nothing");
        assert_eq!(w.gold_sunk, 0);
    }

    /// Wear follows the item: no swap, bank trip or vendor round-trip mends
    /// it for free, and none of them duplicates an item or gold.
    #[test]
    fn swapping_banking_and_selling_never_mend_or_duplicate() {
        let mut w = World::new(7);
        let mut s = geared();
        s.gold = 0;
        s.inventory.push("stone_axe".into());
        let id = w.spawn_player(1, s);
        for _ in 0..4 {
            on_death(w.zones.get_mut(&Act::Eden).unwrap().entities.get_mut(&id).unwrap());
        }
        let worn = current(&w.player_sheet(Act::Eden, id).unwrap(), "bronze_sword");
        assert_eq!(worn, 80 - 32);
        let count = |w: &World, item: &str| {
            let s = w.player_sheet(Act::Eden, id).unwrap();
            s.inventory.iter().filter(|i| *i == item).count()
                + s.equipment.values().filter(|i| *i == item).count()
                + s.bank.iter().filter(|i| *i == item).count()
        };
        // Swap to the axe and back.
        w.equip(Act::Eden, id, "stone_axe").unwrap();
        w.equip(Act::Eden, id, "bronze_sword").unwrap();
        assert_eq!(current(&w.player_sheet(Act::Eden, id).unwrap(), "bronze_sword"), worn);
        assert_eq!((count(&w, "bronze_sword"), count(&w, "stone_axe")), (1, 1));
        // Bank round trip (unequipped).
        w.equip(Act::Eden, id, "stone_axe").unwrap();
        w.bank_item(Act::Eden, id, "bronze_sword", true).unwrap();
        assert!(repair_cost(&w.player_sheet(Act::Eden, id).unwrap()) > 0, "banked gear is still worn");
        w.bank_item(Act::Eden, id, "bronze_sword", false).unwrap();
        assert_eq!(current(&w.player_sheet(Act::Eden, id).unwrap(), "bronze_sword"), worn);
        assert_eq!(count(&w, "bronze_sword"), 1);
        // Sell it: the wear record stays, so buying one back is no repair,
        // and with it gone the quote no longer bills for it.
        let gold0 = w.player_sheet(Act::Eden, id).unwrap().gold;
        let quote_with = repair_cost(&w.player_sheet(Act::Eden, id).unwrap());
        if w.sell(Act::Eden, id, "bronze_sword").is_ok() {
            let s = w.player_sheet(Act::Eden, id).unwrap();
            assert_eq!(count(&w, "bronze_sword"), 0);
            assert_eq!(s.gold, gold0 + crate::economy::sell_price("bronze_sword").unwrap());
            assert!(repair_cost(&s) < quote_with);
            assert_eq!(s.durability.get("bronze_sword"), Some(&worn), "wear record survives the sale");
            // Repairing now must not clear the record of an item we no longer hold.
            w.zones.get_mut(&Act::Eden).unwrap().entities.get_mut(&id).unwrap().sheet.as_mut().unwrap().gold += 50;
            w.repair(Act::Eden, id).unwrap();
            let s = w.player_sheet(Act::Eden, id).unwrap();
            assert_eq!(s.durability.get("bronze_sword"), Some(&worn));
            let mut s2 = s.clone();
            s2.inventory.push("bronze_sword".into());
            assert_eq!(current(&s2, "bronze_sword"), worn, "a re-acquired copy is still worn");
        }
    }

    #[test]
    fn sheets_without_durability_still_load() {
        let s = new_character("Old");
        let mut v = serde_json::to_value(&s).unwrap();
        v.as_object_mut().unwrap().remove("durability");
        let back: CharacterSheet = serde_json::from_value(v).unwrap();
        assert!(back.durability.is_empty());
        assert_eq!(current(&back, "animal_skins"), 60);
    }
}
