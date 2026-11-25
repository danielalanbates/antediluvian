// Copyright (c) 2026 Daniel Bates / Bates LLC. All rights reserved.
//! Spirit-release prompt (B08). While the character is dead a centred panel
//! offers "Release spirit" (rise now at the nearest graveyard at half
//! health) and says what happens otherwise (the automatic return to the
//! home inn a few seconds later).

use antediluvia_protocol::{Act, ClientMsg};
use antediluvia_sim::travel;
use bevy::prelude::*;
use bevy::ui::FocusPolicy;

use super::{button, text, Action, Blocks, WinState, BAD, BORDER, GOLD, PANEL, TEXT, TEXT_DIM};
use crate::Session;

/// Text of the prompt for a character lying dead at (`x`, `y`).
pub fn prompt(act: Act, x: f32, y: f32) -> (String, String) {
    let (name, pos) = travel::nearest_graveyard(act, glam::Vec2::new(x, y));
    let dist = pos.distance(glam::Vec2::new(x, y));
    (
        format!("Release spirit - rise at {name}"),
        format!("About {:.0} paces away, at half health. Or wait a moment to wake at your home inn, whole.", dist / 10.0),
    )
}

#[derive(Component)]
pub(crate) struct DeathRoot;

pub(crate) fn update(
    mut commands: Commands,
    session: Res<Session>,
    st: Res<WinState>,
    existing: Query<Entity, With<DeathRoot>>,
) {
    let dead = session.sheet.is_some() && session.builder.is_none() && st.me.is_some() && st.me_health <= 0;
    if !dead {
        for e in &existing {
            commands.entity(e).despawn_recursive();
        }
        return;
    }
    if !existing.is_empty() {
        return;
    }
    let (x, y, _) = st.me.unwrap_or_default();
    let (label, detail) = prompt(session.act, x, y);
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                top: Val::Percent(30.0),
                left: Val::Px(0.0),
                width: Val::Percent(100.0),
                justify_content: JustifyContent::Center,
                ..default()
            },
            GlobalZIndex(20),
            DeathRoot,
        ))
        .with_children(|outer| {
            outer
                .spawn((
                    Node {
                        flex_direction: FlexDirection::Column,
                        align_items: AlignItems::Center,
                        row_gap: Val::Px(8.0),
                        padding: UiRect::all(Val::Px(18.0)),
                        border: UiRect::all(Val::Px(1.0)),
                        max_width: Val::Px(460.0),
                        ..default()
                    },
                    BackgroundColor(PANEL),
                    BorderColor(BORDER),
                    Interaction::default(),
                    FocusPolicy::Block,
                    Blocks,
                ))
                .with_children(|p| {
                    text(p, "You have died.", 26.0, BAD);
                    button(p, label, Action::Send(vec![ClientMsg::ReleaseSpirit]), GOLD);
                    text(p, detail, 12.0, TEXT);
                    text(p, "Dying wears your gear: repair at an inn.", 11.0, TEXT_DIM);
                });
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prompt_names_the_nearest_graveyard() {
        let (label, detail) = prompt(Act::Eden, 1650.0, -1690.0);
        assert!(label.contains("the eastern cairns"), "{label}");
        assert!(detail.contains("half health"));
        // At the inn, the shrine is nearest.
        assert!(prompt(Act::Hermon, 10.0, 10.0).0.contains("Inn of Hermon"));
    }
}
