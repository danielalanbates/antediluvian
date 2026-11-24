// Copyright (c) 2026 Daniel Bates / Bates LLC. All rights reserved.
//! Notice-board window (B05): the generated quests posted where the
//! character stands, with Accept / Turn in / Abandon. Opens when the server
//! answers `QuestBoard` (F at a board, or `/board`); every action re-reads
//! the board so the statuses stay current.

use antediluvia_protocol::{BoardQuest, ClientMsg};
use bevy::prelude::*;

use super::{button, card, pretty, row, text, Action, Ctx, BAD, GOLD, GOOD, TEXT, TEXT_DIM};

/// Most quests listed at once (the window clips beyond that).
pub const SHOWN: usize = 7;

/// Display order: hand-ins first, then in progress, on offer, locked, done.
pub fn order(status: &str) -> u8 {
    match status {
        "ready" => 0,
        "active" => 1,
        "available" => 2,
        "locked" => 3,
        _ => 4,
    }
}

/// Status line and its colour class: 0 good, 1 normal, 2 dim, 3 bad.
pub fn status_line(q: &BoardQuest) -> (String, u8) {
    match q.status.as_str() {
        "ready" => ("Complete - turn it in".to_string(), 0),
        "active" => (format!("In progress {}/{}", q.progress, q.goal), 1),
        "available" => (format!("Available - level {}+", q.min_level), 1),
        "locked" => (q.locked_reason.clone().unwrap_or_else(|| "Locked".into()), 3),
        _ => ("Done".to_string(), 2),
    }
}

fn then_refresh(msg: ClientMsg) -> Action {
    Action::Send(vec![msg, ClientMsg::QuestBoard])
}

pub(crate) fn build(p: &mut ChildBuilder, ctx: &Ctx) {
    let Some((place, quests)) = ctx.st.board.as_ref() else {
        text(p, "No board read yet. Stand at a point of interest or a cave mouth and press F.", 12.0, TEXT);
        return;
    };
    text(p, format!("{place} - {} notices", quests.len()), 12.0, TEXT_DIM);
    let mut sorted: Vec<&BoardQuest> = quests.iter().collect();
    sorted.sort_by_key(|q| order(&q.status));
    for q in sorted.iter().take(SHOWN) {
        let (line, class) = status_line(q);
        let color = [GOOD, TEXT, TEXT_DIM, BAD][class as usize];
        card(p, |c| {
            row(c, |r| {
                text(r, format!("{} ({})", q.title, q.kind), 13.0, if q.status == "done" { TEXT_DIM } else { GOLD });
                let id = q.id.clone();
                match q.status.as_str() {
                    "available" => button(r, "Accept", then_refresh(ClientMsg::QuestAccept { id }), GOLD),
                    "ready" => button(r, "Turn in", then_refresh(ClientMsg::QuestTurnIn { id }), GOOD),
                    "active" => button(r, "Abandon", then_refresh(ClientMsg::QuestAbandon { id }), TEXT_DIM),
                    _ => {}
                }
            });
            text(c, line, 11.0, color);
            let mut reward = format!("{} xp, {} gold", q.xp, q.gold);
            if let Some(i) = &q.item {
                reward.push_str(&format!(", {}", pretty(i)));
            }
            text(c, format!("Level {}   Reward: {reward}", q.level), 11.0, TEXT_DIM);
        });
    }
    if sorted.len() > SHOWN {
        text(p, format!("...and {} more notices (finished ones sink to the bottom)", sorted.len() - SHOWN), 11.0, TEXT_DIM);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn q(status: &str) -> BoardQuest {
        BoardQuest {
            id: "gq_x".into(),
            title: "T".into(),
            text: String::new(),
            kind: "kill".into(),
            level: 5,
            min_level: 3,
            xp: 10,
            gold: 1,
            item: None,
            status: status.into(),
            progress: 2,
            goal: 6,
            locked_reason: Some("Requires level 9.".into()),
        }
    }

    #[test]
    fn hand_ins_sort_first_and_done_last() {
        let mut v = ["done", "locked", "available", "active", "ready"].map(q).to_vec();
        v.sort_by_key(|q| order(&q.status));
        let got: Vec<&str> = v.iter().map(|q| q.status.as_str()).collect();
        assert_eq!(got, ["ready", "active", "available", "locked", "done"]);
    }

    #[test]
    fn status_lines() {
        assert_eq!(status_line(&q("active")), ("In progress 2/6".into(), 1));
        assert_eq!(status_line(&q("locked")), ("Requires level 9.".into(), 3));
        assert_eq!(status_line(&q("ready")).1, 0);
        assert_eq!(status_line(&q("done")), ("Done".into(), 2));
    }
}
