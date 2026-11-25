// Copyright (c) 2026 Daniel Bates / Bates LLC. All rights reserved.
//! Developer panel inside the Escape menu (dev-allowlisted accounts only):
//! one button per dev-console command, grouped, so a tester never has to
//! remember the console syntax. The server re-checks every command against
//! its allowlist; these buttons only ask.

use antediluvia_protocol::{Act, DevCmd};
use bevy::prelude::*;

use crate::menu::{menu_button, MenuCmd};

const ACCENT: Color = Color::srgb(0.95, 0.55, 0.30);
const DIM: Color = Color::srgb(0.62, 0.60, 0.55);

/// The panel's contents: (group heading, [(button label, command)]).
/// Covers every `DevCmd` variant except free-form quest completion, which
/// needs an id and stays on the console (` key: `questdone ID`).
pub fn groups() -> Vec<(&'static str, Vec<(String, MenuCmd)>)> {
    let dev = |label: &str, cmd: DevCmd| (label.to_string(), MenuCmd::Dev(cmd));
    let give = |item: &str, n: u32| {
        (
            if n > 1 { format!("{} x{n}", crate::win::pretty(item)) } else { crate::win::pretty(item) },
            MenuCmd::Dev(DevCmd::GiveItem { item: item.to_string(), n }),
        )
    };
    let tp = |label: &str, x: f32, y: f32| dev(label, DevCmd::Teleport { x, y });
    vec![
        (
            "Character",
            vec![
                dev("God mode", DevCmd::Godmode),
                dev("Heal", DevCmd::Heal),
                dev("Slay target", DevCmd::KillTarget),
                dev("+1,000 XP", DevCmd::GiveXp { amount: 1000 }),
                dev("+10,000 XP", DevCmd::GiveXp { amount: 10_000 }),
            ],
        ),
        ("Level", [1u32, 10, 20, 30, 40, 50, 60].iter().map(|l| dev(&l.to_string(), DevCmd::SetLevel { level: *l })).collect()),
        (
            "Give",
            vec![
                give("bread", 5),
                give("healing_potion", 5),
                give("taming_lasso", 1),
                give("bronze_sword", 1),
                give("hide_vest", 1),
                give("iron_greaves", 1),
                give("lamechs_helm", 1),
                give("warlords_star_blade", 1),
            ],
        ),
        (
            "Spawn",
            vec![
                dev("Dire wolf", DevCmd::SpawnMob { tag: "dire_wolf".into() }),
                dev("Serpent", DevCmd::SpawnMob { tag: "serpent".into() }),
            ],
        ),
        (
            "Time",
            vec![
                dev("Midnight", DevCmd::TimeOfDay { t: 0.0 }),
                dev("Dawn", DevCmd::TimeOfDay { t: 0.15 }),
                dev("Noon", DevCmd::TimeOfDay { t: 0.3 }),
                dev("Dusk", DevCmd::TimeOfDay { t: 0.5 }),
            ],
        ),
        (
            "Teleport",
            vec![
                tp("Inn", 0.0, 0.0),
                tp("East cairns", 1700.0, -1700.0),
                tp("West cairns", -1700.0, 1700.0),
                tp("Gate of Eden", 2950.0, 0.0),
            ],
        ),
        ("Travel", Act::ALL.iter().map(|a| (crate::win::pretty(a.as_str()), MenuCmd::Travel(*a))).collect()),
    ]
}

pub(crate) fn build(panel: &mut ChildBuilder) {
    panel.spawn((Text::new("- DEVELOPER -"), TextFont { font_size: 14.0, ..default() }, TextColor(ACCENT)));
    for (heading, buttons) in groups() {
        panel
            .spawn(Node {
                flex_direction: FlexDirection::Row,
                flex_wrap: FlexWrap::Wrap,
                align_items: AlignItems::Center,
                column_gap: Val::Px(5.0),
                row_gap: Val::Px(4.0),
                ..default()
            })
            .with_children(|row| {
                row.spawn((
                    Text::new(heading),
                    TextFont { font_size: 12.0, ..default() },
                    TextColor(DIM),
                    Node { width: Val::Px(70.0), ..default() },
                ));
                for (label, cmd) in buttons {
                    menu_button(row, cmd, &label, ACCENT, true);
                }
            });
    }
    panel.spawn((
        Text::new("Console (` key) for the rest: tp X Y | give ITEM N | spawn TAG | questdone ID | time T"),
        TextFont { font_size: 11.0, ..default() },
        TextColor(DIM),
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn panel_covers_the_dev_commands() {
        let all: Vec<MenuCmd> = groups().into_iter().flat_map(|(_, b)| b.into_iter().map(|(_, c)| c)).collect();
        let has = |f: fn(&DevCmd) -> bool| all.iter().any(|c| matches!(c, MenuCmd::Dev(d) if f(d)));
        assert!(has(|d| matches!(d, DevCmd::Godmode)));
        assert!(has(|d| matches!(d, DevCmd::Heal)));
        assert!(has(|d| matches!(d, DevCmd::KillTarget)));
        assert!(has(|d| matches!(d, DevCmd::GiveXp { .. })));
        assert!(has(|d| matches!(d, DevCmd::SetLevel { level: 60 })));
        assert!(has(|d| matches!(d, DevCmd::GiveItem { .. })));
        assert!(has(|d| matches!(d, DevCmd::SpawnMob { .. })));
        assert!(has(|d| matches!(d, DevCmd::TimeOfDay { .. })));
        assert!(has(|d| matches!(d, DevCmd::Teleport { .. })));
        assert_eq!(all.iter().filter(|c| matches!(c, MenuCmd::Travel(_))).count(), Act::ALL.len());
    }

    #[test]
    fn given_items_exist() {
        for (_, buttons) in groups() {
            for (_, cmd) in buttons {
                if let MenuCmd::Dev(DevCmd::GiveItem { item, .. }) = cmd {
                    let known = antediluvia_sim::world::item_def(&item).is_some()
                        || antediluvia_sim::world::CONSUMABLES.contains(&item.as_str())
                        || antediluvia_sim::economy::vendor_price(&item).is_some();
                    assert!(known, "dev panel gives an unknown item: {item}");
                }
            }
        }
    }
}
