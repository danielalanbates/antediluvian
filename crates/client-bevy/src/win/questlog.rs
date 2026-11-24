// Copyright (c) 2026 Daniel Bates / Bates LLC. All rights reserved.
//! Quest log (L): every active quest with its objective, progress, reward,
//! where to hand it in, and an Abandon button. The pure helpers here also
//! feed the HUD tracker, the map pins and the "!"/"?" markers.

use antediluvia_protocol::{CharacterSheet, ClientMsg};
use antediluvia_sim::questgen;
use antediluvia_sim::quests::{self, Objective, QuestDef, QUEST_CAP};
use bevy::prelude::*;

use super::{button, card, pretty, row, text, Action, Ctx, GOLD, GOOD, TEXT, TEXT_DIM};

/// (current, goal) for a quest on this sheet.
pub fn progress(def: &QuestDef, sheet: &CharacterSheet) -> (u32, u32) {
    let goal = def.objective.count();
    let have = match def.objective {
        Objective::Collect { item, .. } => sheet.inventory.iter().filter(|i| i.as_str() == item).count() as u32,
        Objective::Deliver { item, .. } => sheet.inventory.iter().any(|i| i == item) as u32,
        Objective::Kill { .. } | Objective::Explore { .. } => sheet.quests.get(def.id).copied().unwrap_or(0),
    };
    (have.min(goal), goal)
}

/// True when the objective is met and the quest can be handed in.
/// (A delivery is "ready" once carried; it completes at its destination.)
pub fn complete(def: &QuestDef, sheet: &CharacterSheet) -> bool {
    let (have, goal) = progress(def, sheet);
    have >= goal
}

fn place_name(key: &str) -> String {
    if key == "inn" {
        return "the inn".into();
    }
    questgen::anchor_by_key(key).map(|a| a.name.to_string()).unwrap_or_else(|| pretty(key))
}

/// One-line objective with progress, e.g. "Slay Dire Wolf: 3/8".
pub fn objective_line(def: &QuestDef, sheet: &CharacterSheet) -> String {
    let (have, goal) = progress(def, sheet);
    match def.objective {
        Objective::Kill { target, .. } => format!("Slay {}: {have}/{goal}", pretty(target)),
        Objective::Collect { item, source, .. } => format!("Collect {} from {}: {have}/{goal}", pretty(item), pretty(source)),
        Objective::Explore { place } => {
            format!("Explore {}: {}", place_name(place), if have >= 1 { "done" } else { "not yet found" })
        }
        Objective::Deliver { item, to } => format!("Carry {} to {}", pretty(item), place_name(to)),
    }
}

/// Where the quest is handed in.
pub fn turn_in(def: &QuestDef) -> String {
    match questgen::gen_quest(def.id) {
        Some(g) => match def.objective {
            Objective::Deliver { to, .. } => place_name(to),
            _ => format!("{} board (or the inn)", questgen::anchors()[g.anchor].name),
        },
        None => format!("{} in {}", pretty(def.giver), pretty(def.act.as_str())),
    }
}

/// Line for the HUD quest tracker.
pub fn tracker_line(id: &str, sheet: &CharacterSheet) -> String {
    match quests::quest(id) {
        Some(def) => {
            let mark = if complete(def, sheet) { " (ready)" } else { "" };
            format!("{}\n  {}{mark}", questgen::title(id), objective_line(def, sheet))
        }
        None => format!("{}: {}", pretty(id), sheet.quests.get(id).copied().unwrap_or(0)),
    }
}

fn reward(def: &QuestDef) -> String {
    let mut r = format!("{} xp", def.xp);
    if def.gold > 0 {
        r.push_str(&format!(", {} gold", def.gold));
    }
    if let Some(i) = def.item {
        r.push_str(&format!(", {}", pretty(i)));
    }
    r
}

/// First sentence or so of the offer text.
pub fn blurb(offer: &str, max: usize) -> String {
    if offer.chars().count() <= max {
        return offer.to_string();
    }
    let cut: String = offer.chars().take(max).collect();
    match cut.rfind(' ') {
        Some(i) => format!("{}...", &cut[..i]),
        None => format!("{cut}..."),
    }
}

pub(crate) fn build(p: &mut ChildBuilder, ctx: &Ctx) {
    let s = ctx.sheet;
    text(
        p,
        format!("{} of {} quests active   {} completed", s.quests.len(), QUEST_CAP, s.quests_done.len()),
        12.0,
        TEXT_DIM,
    );
    if s.quests.is_empty() {
        text(p, "No active quests. Talk (E) to someone with a gold \"!\" overhead, or read a notice board (F).", 12.0, TEXT);
        return;
    }
    // The window clips; with long logs show compact cards.
    let compact = s.quests.len() > 5;
    for id in s.quests.keys() {
        let Some(def) = quests::quest(id) else {
            card(p, |c| {
                row(c, |r| {
                    text(r, pretty(id), 13.0, GOLD);
                    button(r, "Abandon", Action::Send(vec![ClientMsg::QuestAbandon { id: id.clone() }]), TEXT_DIM);
                });
            });
            continue;
        };
        let done = complete(def, s);
        card(p, |c| {
            row(c, |r| {
                text(r, questgen::title(id), 13.0, GOLD);
                button(r, "Abandon", Action::Send(vec![ClientMsg::QuestAbandon { id: id.clone() }]), TEXT_DIM);
            });
            text(c, objective_line(def, s), 12.0, if done { GOOD } else { TEXT });
            if !compact {
                text(c, blurb(def.offer, 110), 11.0, TEXT_DIM);
            }
            let hand = if done { "Hand in at" } else { "From" };
            text(c, format!("{hand}: {}   Reward: {}", turn_in(def), reward(def)), 11.0, TEXT_DIM);
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use antediluvia_sim::quests::QUESTS;

    fn sheet() -> CharacterSheet {
        antediluvia_sim::world::new_character("Tester")
    }

    #[test]
    fn kill_progress_and_completion() {
        let def = QUESTS.iter().find(|q| matches!(q.objective, Objective::Kill { .. })).unwrap();
        let goal = def.objective.count();
        let mut s = sheet();
        s.quests.insert(def.id.into(), 0);
        assert_eq!(progress(def, &s), (0, goal));
        assert!(!complete(def, &s));
        assert!(objective_line(def, &s).ends_with(&format!("0/{goal}")));
        s.quests.insert(def.id.into(), goal + 5);
        assert_eq!(progress(def, &s), (goal, goal), "progress is capped at the goal");
        assert!(complete(def, &s));
        assert!(tracker_line(def.id, &s).ends_with("(ready)"));
    }

    #[test]
    fn collect_counts_the_bag() {
        let def = QUESTS.iter().find(|q| matches!(q.objective, Objective::Collect { .. })).unwrap();
        let Objective::Collect { item, count, .. } = def.objective else { unreachable!() };
        let mut s = sheet();
        s.quests.insert(def.id.into(), 0);
        s.inventory.retain(|i| i != item);
        assert_eq!(progress(def, &s).0, 0);
        for _ in 0..count {
            s.inventory.push(item.to_string());
        }
        assert!(complete(def, &s));
    }

    #[test]
    fn every_quest_has_a_readable_line_and_hand_in() {
        let s = sheet();
        for def in QUESTS.iter().chain(questgen::generated().iter().map(|g| &g.def)) {
            assert!(!objective_line(def, &s).is_empty());
            assert!(!turn_in(def).is_empty());
            assert!(!questgen::title(def.id).is_empty());
        }
    }

    #[test]
    fn blurb_cuts_on_a_word() {
        assert_eq!(blurb("short", 20), "short");
        assert_eq!(blurb("one two three four", 9), "one two...");
    }
}
