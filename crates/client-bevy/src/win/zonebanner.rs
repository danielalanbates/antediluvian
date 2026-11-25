// Copyright (c) 2026 Daniel Bates / Bates LLC. All rights reserved.
//! Zone-in banner (B08): the place name in large letters with its subtitle
//! underneath, fading out after a few seconds. Red when the ground flags
//! the character for PvP. Fed by `ServerMsg::ZoneBanner`.

use bevy::prelude::*;

use super::{WinState, GOLD, TEXT};

/// Seconds the banner stays up.
pub const SHOW_SECS: f32 = 5.0;
const FADE_SECS: f32 = 1.0;

/// Opacity with `left` seconds to go: a short fade-in, then full, then out.
pub fn alpha(left: f32) -> f32 {
    let age = SHOW_SECS - left;
    (age / 0.4).clamp(0.0, 1.0).min((left / FADE_SECS).clamp(0.0, 1.0))
}

#[derive(Component)]
pub(crate) struct ZoneText(u8);

pub(crate) fn spawn(mut commands: Commands) {
    commands
        .spawn(Node {
            position_type: PositionType::Absolute,
            top: Val::Percent(8.0),
            left: Val::Px(0.0),
            width: Val::Percent(100.0),
            flex_direction: FlexDirection::Column,
            align_items: AlignItems::Center,
            row_gap: Val::Px(2.0),
            ..default()
        })
        .with_children(|col| {
            col.spawn((Text::new(""), TextFont { font_size: 40.0, ..default() }, TextColor(GOLD), ZoneText(0)));
            col.spawn((Text::new(""), TextFont { font_size: 17.0, ..default() }, TextColor(TEXT), ZoneText(1)));
        });
}

pub(crate) fn update(time: Res<Time>, mut st: ResMut<WinState>, mut q: Query<(&ZoneText, &mut Text, &mut TextColor)>) {
    let Some(card) = st.zone.as_ref() else {
        for (_, mut t, _) in &mut q {
            if !t.is_empty() {
                **t = String::new();
            }
        }
        return;
    };
    // Long loading frames must not eat the banner.
    let left = card.left - time.delta_secs().min(0.1);
    let a = alpha(left);
    let title = if card.pvp { Color::srgb(0.95, 0.30, 0.25) } else { GOLD };
    for (which, mut t, mut c) in &mut q {
        let (s, col) = if which.0 == 0 { (&card.name, title) } else { (&card.subtitle, TEXT) };
        if **t != **s {
            **t = s.clone();
        }
        c.0 = col.with_alpha(a);
    }
    if left <= 0.0 {
        st.zone = None;
    } else if let Some(z) = st.zone.as_mut() {
        z.left = left;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn banner_fades_in_holds_and_fades_out() {
        assert_eq!(alpha(SHOW_SECS), 0.0);
        assert_eq!(alpha(SHOW_SECS - 1.0), 1.0);
        assert_eq!(alpha(2.5), 1.0);
        assert!((alpha(0.5) - 0.5).abs() < 1e-6);
        assert_eq!(alpha(0.0), 0.0);
        assert_eq!(alpha(-1.0), 0.0);
    }
}
