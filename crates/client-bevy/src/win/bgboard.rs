// Copyright (c) 2026 Daniel Bates / Bates LLC. All rights reserved.
//! Battleground scoreboard (B10): a strip at the top right while queued or
//! on the field, fed by `ServerMsg::BgState`. Score, timer and relic on the
//! first line, then one row per player (caps / kills / deaths), then the
//! honor line. Queue from the Esc menu or `/bg`; leave with `/bg leave`.

use antediluvia_protocol::BgInfo;
use bevy::prelude::*;

use super::{WinState, GOLD, TEXT};
use crate::ui::{set_text, set_vis};

const ROWS: usize = 24;
const TEAMS: [&str; 2] = ["Sethites", "Cainites"];

fn clock(secs: u32) -> String {
    format!("{}:{:02}", secs / 60, secs % 60)
}

/// The board as text lines (first is the header). Empty when idle.
pub fn lines(i: &BgInfo) -> Vec<String> {
    let mut out = Vec::new();
    match i.phase.as_str() {
        "queued" => {
            out.push(format!("Relic of Seth - queued ({} waiting)", i.queued));
            out.push("/bg leave to drop out".into());
        }
        "live" | "ended" => {
            let relic = match (i.relic.as_str(), &i.carrier) {
                ("carried", Some(c)) => format!("relic: {c}"),
                ("dropped", _) => "relic: on the ground".into(),
                _ => "relic: on the altar".into(),
            };
            let head = if i.phase == "ended" {
                match i.winner {
                    Some(t) => format!("{} win {} - {}", TEAMS[t as usize & 1], i.score[0], i.score[1]),
                    None => format!("Draw {} - {}", i.score[0], i.score[1]),
                }
            } else {
                format!("{} {} - {} {}   {}   {}", TEAMS[0], i.score[0], i.score[1], TEAMS[1], clock(i.secs_left), relic)
            };
            out.push(head);
            out.push(format!("You fight for the {}.  caps / kills / deaths", TEAMS[i.my_team as usize & 1]));
            let mut rows = i.rows.clone();
            rows.sort_by(|a, b| a.team.cmp(&b.team).then(b.caps.cmp(&a.caps)).then(b.kills.cmp(&a.kills)).then(a.name.cmp(&b.name)));
            for r in rows.iter().take(ROWS - 3) {
                out.push(format!("{} {:<14} {:>2} / {:>2} / {:>2}", if r.team == 0 { "S" } else { "C" }, r.name, r.caps, r.kills, r.deaths));
            }
        }
        _ => return out,
    }
    out.push(format!("Honor {}  (rank {}, {} earned)", i.honor, i.rank, i.honor_earned));
    out
}

#[derive(Component)]
pub(crate) struct BgPanel;

#[derive(Component)]
pub(crate) struct BgLine(usize);

pub(crate) fn spawn(mut commands: Commands) {
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                top: Val::Px(8.0),
                right: Val::Px(230.0),
                flex_direction: FlexDirection::Column,
                padding: UiRect::all(Val::Px(8.0)),
                row_gap: Val::Px(1.0),
                ..default()
            },
            BackgroundColor(Color::srgba(0.05, 0.05, 0.08, 0.80)),
            Visibility::Hidden,
            BgPanel,
        ))
        .with_children(|col| {
            for i in 0..ROWS {
                let (size, color) = if i == 0 { (16.0, GOLD) } else { (13.0, TEXT) };
                col.spawn((Text::new(""), TextFont { font_size: size, ..default() }, TextColor(color), BgLine(i)));
            }
        });
}

pub(crate) fn update(st: Res<WinState>, mut panel: Query<&mut Visibility, With<BgPanel>>, mut q: Query<(&BgLine, &mut Text)>) {
    let ls = st.bg.as_ref().map(lines).unwrap_or_default();
    if let Ok(mut v) = panel.get_single_mut() {
        set_vis(&mut v, !ls.is_empty());
    }
    for (line, mut t) in &mut q {
        set_text(&mut t, ls.get(line.0).map_or("", |s| s.as_str()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use antediluvia_protocol::BgRow;

    fn info(phase: &str) -> BgInfo {
        BgInfo {
            phase: phase.into(),
            queued: 3,
            score: [2, 1],
            my_team: 1,
            relic: "carried".into(),
            carrier: Some("Enosh".into()),
            relic_x: 0.0,
            relic_y: 0.0,
            secs_left: 125,
            winner: None,
            rows: vec![
                BgRow { name: "Cain".into(), team: 1, caps: 1, kills: 2, deaths: 0 },
                BgRow { name: "Enosh".into(), team: 0, caps: 2, kills: 0, deaths: 1 },
            ],
            honor: 40,
            honor_earned: 300,
            rank: "Shieldbearer".into(),
        }
    }

    #[test]
    fn board_reads_like_a_scoreboard() {
        assert!(lines(&info("idle")).is_empty());
        let q = lines(&info("queued"));
        assert!(q[0].contains("3 waiting"));
        let l = lines(&info("live"));
        assert_eq!(l[0], "Sethites 2 - 1 Cainites   2:05   relic: Enosh");
        assert!(l[1].contains("Cainites"));
        assert!(l[2].starts_with("S Enosh"), "Sethites listed first: {l:?}");
        assert!(l.last().unwrap().contains("Shieldbearer"));
        let mut e = info("ended");
        e.winner = Some(0);
        assert_eq!(lines(&e)[0], "Sethites win 2 - 1");
        e.winner = None;
        assert!(lines(&e)[0].starts_with("Draw"));
        assert!(lines(&info("live")).len() <= ROWS);
    }
}
