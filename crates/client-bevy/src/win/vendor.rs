// Copyright (c) 2026 Daniel Bates / Bates LLC. All rights reserved.
//! Vendor window: buy the innkeeper's staples and the Quartermaster's
//! lineage wares, sell from the bags, repair worn gear. Opens when the
//! innkeeper or Quartermaster answers a Talk (E). The server (or the solo
//! driver) enforces standing at the inn, gold and reputation.

use antediluvia_protocol::{CharacterSheet, ClientMsg};
use antediluvia_sim::{durability, economy, world as simw};
use bevy::prelude::*;

use super::{bags, button, card, charsheet, layout, pretty, row, text, Action, Ctx, BAD, GOLD, TEXT, TEXT_DIM};

/// One line of the buy list.
#[derive(Debug, Clone, PartialEq)]
pub struct Ware {
    pub item: &'static str,
    pub price: u32,
    /// Why it cannot be bought right now, if anything.
    pub locked: Option<String>,
}

fn rank_index(r: &str) -> usize {
    ["neutral", "friendly", "honored", "revered"].iter().position(|x| *x == r).unwrap_or(0)
}

/// Everything the inn's two vendors stock, with affordability.
pub fn wares(sheet: &CharacterSheet) -> Vec<Ware> {
    let poor = |price: u32| (sheet.gold < price).then(|| "not enough gold".to_string());
    let mut out: Vec<Ware> = economy::STAPLES
        .iter()
        .filter_map(|i| economy::staple_price(i).map(|price| Ware { item: *i, price, locked: poor(price) }))
        .collect();
    let standing = sheet.faction.as_deref().map(|f| charsheet::rank(sheet.reputation.get(f).copied().unwrap_or(0)));
    for (item, price, need) in simw::QM_WARES {
        let locked = match standing {
            None => Some("choose a lineage first".to_string()),
            Some(have) if rank_index(have) < rank_index(need) => Some(format!("needs {need} standing")),
            Some(_) => poor(*price),
        };
        out.push(Ware { item: *item, price: *price, locked });
    }
    out
}

/// Sellable stacks: (item, count, gold each).
pub fn sellables(sheet: &CharacterSheet) -> Vec<(String, u32, u32)> {
    bags::stacks(&sheet.inventory)
        .into_iter()
        .filter_map(|(item, n)| economy::sell_price(&item).map(|p| (item, n, p)))
        .collect()
}

pub(crate) fn build(p: &mut ChildBuilder, ctx: &Ctx) {
    let s = ctx.sheet;
    row(p, |r| {
        text(r, "Innkeeper and Quartermaster", 12.0, TEXT_DIM);
        text(r, format!("{} gold", s.gold), 13.0, GOLD);
    });
    text(p, "Buy", 13.0, GOLD);
    card(p, |c| {
        for w in wares(s) {
            row(c, |r| {
                text(r, pretty(w.item), 12.0, layout::rarity_color(durability::rarity_of(w.item)));
                match &w.locked {
                    None => button(r, format!("Buy  {}g", w.price), Action::Send(vec![ClientMsg::Buy { item: w.item.to_string() }]), TEXT),
                    Some(why) => text(r, format!("{}g - {why}", w.price), 11.0, TEXT_DIM),
                }
            });
        }
    });
    text(p, "Sell (four-fifths of worth)", 13.0, GOLD);
    card(p, |c| {
        let list = sellables(s);
        if list.is_empty() {
            text(c, "Nothing in your bags the innkeeper wants.", 12.0, TEXT_DIM);
        }
        for (item, n, price) in list.iter().take(9) {
            row(c, |r| {
                let name = if *n > 1 { format!("{} x{n}", pretty(item)) } else { pretty(item) };
                text(r, name, 12.0, layout::rarity_color(durability::rarity_of(item)));
                button(r, format!("Sell 1  {price}g"), Action::Send(vec![ClientMsg::Sell { item: item.clone() }]), TEXT);
            });
        }
        if list.len() > 9 {
            text(c, format!("...and {} more (right-click items in your bags)", list.len() - 9), 11.0, TEXT_DIM);
        }
    });
    let cost = durability::repair_cost(s);
    row(p, |r| {
        if cost == 0 {
            text(r, "Your gear needs no repair.", 12.0, TEXT_DIM);
        } else {
            text(r, format!("Repair all worn gear: {cost} gold"), 12.0, if s.gold >= cost { TEXT } else { BAD });
            button(r, "Repair", Action::Send(vec![ClientMsg::Repair]), GOLD);
        }
    });
    text(p, "Trading works only while you stand at the inn.", 11.0, TEXT_DIM);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn staples_are_always_listed_and_gold_gates_them() {
        let mut s = antediluvia_sim::world::new_character("Tester");
        s.gold = 0;
        let w = wares(&s);
        assert!(w.len() >= economy::STAPLES.len());
        assert!(w.iter().all(|w| w.locked.is_some()), "a pauper can buy nothing");
        s.gold = 100_000;
        let w = wares(&s);
        assert!(w.iter().take(economy::STAPLES.len()).all(|w| w.locked.is_none()));
        // No lineage: Quartermaster wares stay locked however rich you are.
        assert!(w.iter().skip(economy::STAPLES.len()).all(|w| w.locked.as_deref() == Some("choose a lineage first")));
    }

    #[test]
    fn quartermaster_wares_unlock_with_standing() {
        let mut s = antediluvia_sim::world::new_character("Tester");
        s.gold = 100_000;
        s.faction = Some("sethite".into());
        s.reputation.insert("sethite".into(), 3000); // friendly
        let w = wares(&s);
        let qm = |item: &str| w.iter().find(|w| w.item == item).unwrap().locked.clone();
        assert_eq!(qm("thick_hide"), None);
        assert_eq!(qm("lineage_blade"), Some("needs honored standing".into()));
        assert_eq!(qm("lineage_mantle"), Some("needs revered standing".into()));
    }

    #[test]
    fn sell_list_prices_stacks() {
        let mut s = antediluvia_sim::world::new_character("Tester");
        s.inventory = vec!["bread".into(), "bread".into(), "bronze_sword".into()];
        let l = sellables(&s);
        assert_eq!(l[0], ("bread".to_string(), 2, economy::sell_price("bread").unwrap()));
        assert_eq!(l[1].0, "bronze_sword");
    }
}
