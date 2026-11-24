// Copyright (c) 2026 Daniel Bates / Bates LLC. All rights reserved.
//! Bags (B): the inventory as a slot grid. Border and name take the item's
//! rarity colour, worn gear shows its durability, hovering shows a tooltip
//! and a right-click uses or equips the item (or sells it while the vendor
//! window is open, as in the classics).

use antediluvia_protocol::{CharacterSheet, ClientMsg};
use antediluvia_sim::{durability, economy, world as simw};
use bevy::prelude::*;

use super::{layout, pretty, text, Action, Blocks, Ctx, RightAction, Tip, Win, GOLD, INSET, TEXT, TEXT_DIM};

const COLS: usize = 6;
const SLOT: f32 = 48.0;

/// Collapse the flat inventory into stacks, keeping first-seen order.
pub fn stacks(inv: &[String]) -> Vec<(String, u32)> {
    let mut out: Vec<(String, u32)> = Vec::new();
    for it in inv {
        match out.iter_mut().find(|(i, _)| i == it) {
            Some((_, n)) => *n += 1,
            None => out.push((it.clone(), 1)),
        }
    }
    out
}

/// What a right-click does with an item.
pub fn item_action(item: &str, vendor_open: bool) -> Option<(&'static str, ClientMsg)> {
    if vendor_open {
        return economy::sell_price(item).map(|_| ("sell", ClientMsg::Sell { item: item.to_string() }));
    }
    if simw::item_def(item).is_some() {
        Some(("equip", ClientMsg::Equip { item: item.to_string() }))
    } else if simw::CONSUMABLES.contains(&item) {
        Some(("use", ClientMsg::UseItem { item: item.to_string() }))
    } else {
        None
    }
}

/// Multi-line tooltip for an item the character carries or wears.
pub fn item_tip(sheet: &CharacterSheet, item: &str, vendor_open: bool) -> String {
    let rarity = durability::rarity_of(item);
    let mut lines = vec![pretty(item), pretty(rarity.as_str())];
    if let Some(d) = simw::item_def(item) {
        lines.push(format!("{} slot", pretty(d.slot)));
        let mut stats = Vec::new();
        if d.melee != 0 {
            stats.push(format!("+{} melee", d.melee));
        }
        if d.spell != 0 {
            stats.push(format!("+{} spell", d.spell));
        }
        if d.hp != 0 {
            stats.push(format!("+{} health", d.hp));
        }
        if !stats.is_empty() {
            lines.push(stats.join("  "));
        }
        let max = durability::max_durability(item);
        if max > 0 {
            let cur = durability::current(sheet, item);
            lines.push(if cur == 0 { format!("Durability 0 / {max} - BROKEN") } else { format!("Durability {cur} / {max}") });
        }
    } else if simw::CONSUMABLES.contains(&item) {
        lines.push("Consumable".into());
    } else if simw::KEY_ITEMS.contains(&item) {
        lines.push("Key item".into());
    }
    if let Some(p) = economy::sell_price(item) {
        lines.push(format!("Sells for {p} gold"));
    }
    match item_action(item, vendor_open) {
        Some((verb, _)) => lines.push(format!("Right-click to {verb}")),
        None if vendor_open => lines.push("The innkeeper will not buy this".into()),
        None => {}
    }
    lines.join("\n")
}

pub(crate) fn build(p: &mut ChildBuilder, ctx: &Ctx) {
    let sheet = ctx.sheet;
    let vendor = ctx.st.is_open(Win::Vendor);
    let stacks = stacks(&sheet.inventory);
    let slots = layout::bag_slots(stacks.len(), COLS, 4);
    p.spawn(Node {
        flex_direction: FlexDirection::Row,
        flex_wrap: FlexWrap::Wrap,
        column_gap: Val::Px(4.0),
        row_gap: Val::Px(4.0),
        ..default()
    })
    .with_children(|grid| {
        for i in 0..slots {
            let node = Node {
                width: Val::Px(SLOT),
                height: Val::Px(SLOT),
                border: UiRect::all(Val::Px(2.0)),
                padding: UiRect::all(Val::Px(1.0)),
                flex_direction: FlexDirection::Column,
                justify_content: JustifyContent::SpaceBetween,
                overflow: Overflow::clip(),
                ..default()
            };
            let Some((item, n)) = stacks.get(i) else {
                grid.spawn((node, BackgroundColor(INSET), BorderColor(Color::srgb(0.22, 0.22, 0.26))));
                continue;
            };
            let color = layout::rarity_color(durability::rarity_of(item));
            let max = durability::max_durability(item);
            let cur = durability::current(sheet, item);
            let mut slot = grid.spawn((
                Button,
                node,
                BackgroundColor(INSET),
                BorderColor(color),
                Blocks,
                Tip(item_tip(sheet, item, vendor)),
            ));
            if let Some((_, msg)) = item_action(item, vendor) {
                slot.insert(RightAction(Action::Send(vec![msg])));
            }
            slot.with_children(|s| {
                s.spawn((Text::new(pretty(item)), TextFont { font_size: 9.0, ..default() }, TextColor(color)));
                let foot = match (*n > 1, max > 0) {
                    (true, _) => format!("x{n}"),
                    (false, true) => format!("{}%", cur * 100 / max),
                    _ => String::new(),
                };
                let foot_color = if *n > 1 { TEXT } else { layout::wear_color(cur, max) };
                s.spawn((Text::new(foot), TextFont { font_size: 10.0, ..default() }, TextColor(foot_color)));
            });
        }
    });
    super::row(p, |r| {
        text(r, format!("{} gold", sheet.gold), 13.0, GOLD);
        text(r, format!("{} items in {} stacks", sheet.inventory.len(), stacks.len()), 12.0, TEXT_DIM);
    });
    text(
        p,
        if vendor { "Right-click an item to sell it." } else { "Right-click: use or equip. Hover for details." },
        11.0,
        TEXT_DIM,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stacks_keep_order_and_count() {
        let inv: Vec<String> = ["bread", "wolf_pelt", "bread", "bronze_sword", "bread"].iter().map(|s| s.to_string()).collect();
        assert_eq!(
            stacks(&inv),
            vec![("bread".to_string(), 3), ("wolf_pelt".to_string(), 1), ("bronze_sword".to_string(), 1)]
        );
        assert!(stacks(&[]).is_empty());
    }

    #[test]
    fn right_click_equips_gear_uses_food_and_sells_at_a_vendor() {
        assert!(matches!(item_action("bronze_sword", false), Some(("equip", ClientMsg::Equip { .. }))));
        assert!(matches!(item_action("bread", false), Some(("use", ClientMsg::UseItem { .. }))));
        assert!(item_action("gate_token", false).is_none());
        // With the vendor open, anything with a price sells instead.
        assert!(matches!(item_action("bread", true), Some(("sell", ClientMsg::Sell { .. }))));
    }

    #[test]
    fn tooltip_names_rarity_stats_and_wear() {
        let mut s = antediluvia_sim::world::new_character("Tester");
        s.inventory.push("bronze_sword".into());
        let tip = item_tip(&s, "bronze_sword", false);
        assert!(tip.starts_with("Bronze Sword\n"), "{tip}");
        assert!(tip.contains("Weapon slot") && tip.contains("+8 melee"), "{tip}");
        assert!(tip.contains("Durability") && tip.contains("Right-click to equip"), "{tip}");
        s.durability.insert("bronze_sword".into(), 0);
        assert!(item_tip(&s, "bronze_sword", false).contains("BROKEN"));
    }
}
