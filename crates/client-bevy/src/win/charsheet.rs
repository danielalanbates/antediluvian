// Copyright (c) 2026 Daniel Bates / Bates LLC. All rights reserved.
//! Character sheet / paperdoll (C): what is worn in each slot (rarity colour,
//! durability), the character's numbers, and the repair state.

use antediluvia_protocol::{CharacterSheet, ClientMsg};
use antediluvia_sim::abilities::{class_resource, Resource};
use antediluvia_sim::{durability, world as simw};
use bevy::prelude::*;

use super::{bags, button, card, layout, pretty, row, text, Action, Blocks, Ctx, Tip, BAD, GOLD, GOOD, TEXT, TEXT_DIM};

/// Paperdoll slots, head to hand.
pub const SLOTS: [&str; 8] = ["head", "neck", "back", "chest", "wrist", "legs", "finger", "weapon"];

/// Totals granted by worn gear: (melee, spell, health).
pub fn gear_totals(sheet: &CharacterSheet) -> (i32, i32, i32) {
    sheet
        .equipment
        .values()
        .filter_map(|i| simw::item_def(i))
        // A broken piece gives nothing until repaired.
        .filter(|d| !durability::is_broken(sheet, d.id))
        .fold((0, 0, 0), |(m, s, h), d| (m + d.melee, s + d.spell, h + d.hp))
}

/// Standing name for a reputation total (same bands as the unit frame).
pub fn rank(rep: i32) -> &'static str {
    match rep {
        i32::MIN..=2999 => "neutral",
        3000..=8999 => "friendly",
        9000..=20999 => "honored",
        _ => "revered",
    }
}

/// (worn pieces that wear, of those damaged, of those broken).
pub fn wear_summary(sheet: &CharacterSheet) -> (u32, u32, u32) {
    let mut out = (0, 0, 0);
    for item in sheet.equipment.values() {
        let max = durability::max_durability(item);
        if max == 0 {
            continue;
        }
        let cur = durability::current(sheet, item);
        out.0 += 1;
        if cur < max {
            out.1 += 1;
        }
        if cur == 0 {
            out.2 += 1;
        }
    }
    out
}

pub(crate) fn build(p: &mut ChildBuilder, ctx: &Ctx) {
    let s = ctx.sheet;
    let class = s.class.map(|c| pretty(c.as_str())).unwrap_or_else(|| "Classless".into());
    text(p, format!("{} - level {} {}", s.name, s.level, class), 14.0, TEXT);

    // ── Paperdoll ──
    card(p, |c| {
        for slot in SLOTS {
            c.spawn((
                Node {
                    flex_direction: FlexDirection::Row,
                    justify_content: JustifyContent::SpaceBetween,
                    column_gap: Val::Px(6.0),
                    ..default()
                },
                Interaction::default(),
                Blocks,
                Tip(match s.equipment.get(slot) {
                    Some(item) => bags::item_tip(s, item, false).replace("\nRight-click to equip", ""),
                    None => format!("{}: empty", pretty(slot)),
                }),
            ))
            .with_children(|r| {
                text(r, pretty(slot), 12.0, TEXT_DIM);
                match s.equipment.get(slot) {
                    Some(item) => {
                        let max = durability::max_durability(item);
                        let cur = durability::current(s, item);
                        text(r, pretty(item), 12.0, layout::rarity_color(durability::rarity_of(item)));
                        let wear = if max == 0 { "-".to_string() } else { format!("{cur}/{max}") };
                        text(r, wear, 12.0, layout::wear_color(cur, max));
                    }
                    None => text(r, "empty", 12.0, Color::srgb(0.38, 0.38, 0.40)),
                }
            });
        }
    });

    // ── Repair state ──
    let (wearing, damaged, broken) = wear_summary(s);
    let cost = durability::repair_cost(s);
    row(p, |r| {
        let (line, color) = if wearing == 0 {
            ("Nothing worn that can wear out.".to_string(), TEXT_DIM)
        } else if broken > 0 {
            (format!("{broken} piece(s) BROKEN - repair: {cost} gold"), BAD)
        } else if damaged > 0 {
            (format!("{damaged} piece(s) worn - repair: {cost} gold"), GOLD)
        } else {
            ("All gear in good repair.".to_string(), GOOD)
        };
        text(r, line, 12.0, color);
        if damaged > 0 {
            button(r, "Repair (at an inn)", Action::Send(vec![ClientMsg::Repair]), GOLD);
        }
    });

    // ── Numbers ──
    let (melee, spell, hp) = gear_totals(s);
    let stamina = s.class.is_some_and(|c| class_resource(c) == Resource::Stamina);
    let pool = if stamina { format!("Stamina {}", s.max_stamina) } else { format!("Mana {}", s.max_mana) };
    card(p, |c| {
        let line = |c: &mut ChildBuilder, a: String, b: String| {
            row(c, |r| {
                text(r, a, 12.0, TEXT);
                text(r, b, 12.0, TEXT);
            });
        };
        line(c, format!("Health {}", s.max_health), pool);
        line(c, format!("Gear: +{melee} melee"), format!("+{spell} spell  +{hp} health"));
        line(c, format!("XP {} / {}", s.xp, s.max_xp), format!("Rested +{}", s.rested_xp));
        line(c, format!("Gold {}", s.gold), format!("Talent points {}", s.talent_points));
        let lineage = match s.faction.as_deref() {
            Some(f) => format!("{} ({})", pretty(f), rank(s.reputation.get(f).copied().unwrap_or(0))),
            None => "No lineage (choose at 10)".to_string(),
        };
        line(c, lineage, format!("World PvP {}", if s.pvp { "ON" } else { "off" }));
        line(
            c,
            format!("Guild: {}", s.guild.as_deref().unwrap_or("none")),
            format!("Home: {}", s.home_act.map(|a| pretty(a.as_str())).unwrap_or_else(|| "shrine".into())),
        );
        line(c, format!("Wakefulness {:.0}%", s.wakefulness), format!("Quests done {}", s.quests_done.len()));
    });
    if !s.professions.is_empty() {
        let profs: Vec<String> = s.professions.iter().map(|(k, v)| format!("{} {v}", pretty(k))).collect();
        text(p, format!("Professions: {}", profs.join(", ")), 11.0, TEXT_DIM);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gear_totals_skip_broken_pieces() {
        let mut s = antediluvia_sim::world::new_character("Tester");
        s.equipment.clear();
        s.equipment.insert("weapon".into(), "bronze_sword".into());
        s.equipment.insert("chest".into(), "hide_vest".into());
        assert_eq!(gear_totals(&s), (8, 0, 30));
        assert_eq!(wear_summary(&s), (2, 0, 0));
        s.durability.insert("bronze_sword".into(), 0);
        assert_eq!(gear_totals(&s), (0, 0, 30));
        assert_eq!(wear_summary(&s), (2, 1, 1));
    }

    #[test]
    fn reputation_ranks() {
        assert_eq!(rank(0), "neutral");
        assert_eq!(rank(3000), "friendly");
        assert_eq!(rank(9000), "honored");
        assert_eq!(rank(21000), "revered");
    }
}
