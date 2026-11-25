// Copyright (c) 2026 Daniel Bates / Bates LLC. All rights reserved.
//! Buff / debuff icons beside the unit frame.
//!
//! The wire protocol carries no aura list, so the row shows what the client
//! can know for certain: the timed self-buffs of abilities it just saw fire
//! (`AbilityUsed`), and standing conditions read off the character sheet
//! (rested, PvP flag, broken gear, exhaustion). Effects enemies put on the
//! character are not shown; that needs a protocol addition.

use antediluvia_protocol::CharacterSheet;
use antediluvia_sim::abilities::{ability, Fx};
use bevy::prelude::*;

use super::{charsheet, Buff, Tip, WinState};
use crate::Session;

const ICONS: usize = 10;

/// The timed friendly effects an ability leaves on its caster.
pub fn buffs_of(ability_id: &str, now: f32) -> Vec<Buff> {
    let Some(a) = ability(ability_id) else { return Vec::new() };
    a.fx
        .iter()
        .filter_map(|fx| {
            let (label, what, secs) = match *fx {
                Fx::Hot { heal, secs } => ("Mend", format!("heals {heal} each second"), secs),
                Fx::Might { pct, secs } => ("Might", format!("deals {pct}% more damage"), secs),
                Fx::Guard { pct, secs } => ("Guard", format!("takes {pct}% less damage"), secs),
                Fx::Haste { pct, secs } => ("Haste", format!("moves {pct}% faster"), secs),
                Fx::Shield { amount, secs } => ("Shield", format!("absorbs up to {amount} damage"), secs),
                _ => return None,
            };
            Some(Buff { label: label.to_string(), tip: format!("{} ({}): {what}", label, a.name), until: now + secs })
        })
        .collect()
}

/// One icon: (label, tooltip, is a debuff).
pub type Status = (String, String, bool);

/// Standing conditions read from the sheet.
pub fn sheet_statuses(s: &CharacterSheet) -> Vec<Status> {
    let mut out = Vec::new();
    if s.rested_xp > 0 {
        out.push(("Rested".to_string(), format!("Rested: +{} bonus XP banked for kills", s.rested_xp), false));
    }
    if s.pvp {
        out.push(("PvP".to_string(), "Flagged for world PvP".to_string(), true));
    }
    let (_, _, broken) = charsheet::wear_summary(s);
    if broken > 0 {
        out.push(("Broken".to_string(), format!("{broken} broken piece(s) give no bonus - repair at an inn"), true));
    }
    if s.wakefulness < 5.0 {
        out.push(("Weary".to_string(), "Exhausted: rest at an inn or eat".to_string(), true));
    }
    out
}

/// Everything to draw, timed buffs first: (text, tooltip, debuff).
pub fn icons(buffs: &[Buff], sheet: &CharacterSheet, now: f32) -> Vec<Status> {
    let mut out: Vec<Status> = buffs
        .iter()
        .filter(|b| b.until > now)
        .map(|b| (format!("{}\n{:.0}s", b.label, (b.until - now).ceil()), b.tip.clone(), false))
        .collect();
    out.extend(sheet_statuses(sheet));
    out.truncate(ICONS);
    out
}

#[derive(Component)]
pub(crate) struct BuffIcon {
    i: usize,
    label: Entity,
}

pub(crate) fn spawn(mut commands: Commands) {
    commands
        .spawn(Node {
            position_type: PositionType::Absolute,
            top: Val::Px(14.0),
            left: Val::Px(262.0),
            flex_direction: FlexDirection::Row,
            column_gap: Val::Px(4.0),
            ..default()
        })
        .with_children(|row| {
            for i in 0..ICONS {
                let mut label = Entity::PLACEHOLDER;
                let mut icon = row.spawn((
                    Node {
                        width: Val::Px(46.0),
                        height: Val::Px(34.0),
                        border: UiRect::all(Val::Px(1.0)),
                        justify_content: JustifyContent::Center,
                        align_items: AlignItems::Center,
                        overflow: Overflow::clip(),
                        ..default()
                    },
                    BackgroundColor(Color::srgba(0.08, 0.20, 0.10, 0.90)),
                    BorderColor(Color::srgb(0.35, 0.75, 0.40)),
                    Interaction::default(),
                    Tip(String::new()),
                    Visibility::Hidden,
                ));
                icon.with_children(|b| {
                    label = b
                        .spawn((
                            Text::new(""),
                            TextFont { font_size: 10.0, ..default() },
                            TextColor(super::TEXT),
                            TextLayout::new_with_justify(JustifyText::Center),
                        ))
                        .id();
                });
                icon.insert(BuffIcon { i, label });
            }
        });
}

pub(crate) fn update(
    time: Res<Time>,
    session: Res<Session>,
    mut st: ResMut<WinState>,
    mut q: Query<(&BuffIcon, &mut Visibility, &mut BackgroundColor, &mut BorderColor, &mut Tip)>,
    mut texts: Query<&mut Text>,
) {
    let now = time.elapsed_secs();
    if st.buffs.iter().any(|b| b.until <= now) {
        st.buffs.retain(|b| b.until > now);
    }
    let shown = match session.sheet.as_ref().filter(|_| session.builder.is_none()) {
        Some(s) => icons(&st.buffs, s, now),
        None => Vec::new(),
    };
    for (icon, mut vis, mut bg, mut border, mut tip) in &mut q {
        let Some((text, tooltip, debuff)) = shown.get(icon.i) else {
            if *vis != Visibility::Hidden {
                *vis = Visibility::Hidden;
            }
            continue;
        };
        if *vis != Visibility::Inherited {
            *vis = Visibility::Inherited;
        }
        let (fill, edge) = if *debuff {
            (Color::srgba(0.26, 0.08, 0.08, 0.90), Color::srgb(0.85, 0.30, 0.25))
        } else {
            (Color::srgba(0.08, 0.20, 0.10, 0.90), Color::srgb(0.35, 0.75, 0.40))
        };
        if bg.0 != fill {
            bg.0 = fill;
            border.0 = edge;
        }
        if tip.0 != *tooltip {
            tip.0 = tooltip.clone();
        }
        if let Ok(mut t) = texts.get_mut(icon.label) {
            if **t != *text {
                **t = text.clone();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use antediluvia_sim::abilities::ABILITIES;

    #[test]
    fn friendly_timed_effects_become_buffs() {
        let a = ABILITIES
            .iter()
            .find(|a| a.fx.iter().any(|f| matches!(f, Fx::Guard { .. } | Fx::Might { .. } | Fx::Shield { .. } | Fx::Haste { .. } | Fx::Hot { .. })))
            .expect("some ability buffs its caster");
        let b = buffs_of(a.id, 10.0);
        assert!(!b.is_empty());
        assert!(b.iter().all(|b| b.until > 10.0 && b.tip.contains(a.name)));
        assert!(buffs_of("no_such_ability", 0.0).is_empty());
    }

    #[test]
    fn pure_damage_abilities_leave_no_buff() {
        let a = ABILITIES.iter().find(|a| a.fx.iter().all(|f| f.hostile())).unwrap();
        assert!(buffs_of(a.id, 0.0).is_empty());
    }

    #[test]
    fn sheet_conditions_and_expiry() {
        let mut s = antediluvia_sim::world::new_character("Tester");
        s.rested_xp = 0;
        s.pvp = false;
        s.durability.clear();
        assert!(sheet_statuses(&s).is_empty());
        s.rested_xp = 300;
        s.pvp = true;
        let st = sheet_statuses(&s);
        assert_eq!(st.len(), 2);
        assert!(!st[0].2 && st[1].2, "rested is a buff, the PvP flag a debuff");
        let buffs = vec![
            Buff { label: "Guard".into(), tip: "t".into(), until: 12.4 },
            Buff { label: "Old".into(), tip: "t".into(), until: 5.0 },
        ];
        let shown = icons(&buffs, &s, 10.0);
        assert_eq!(shown.len(), 3, "the expired buff is gone");
        assert_eq!(shown[0].0, "Guard\n3s");
    }
}
