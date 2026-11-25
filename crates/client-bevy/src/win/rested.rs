// Copyright (c) 2026 Daniel Bates / Bates LLC. All rights reserved.
//! Rested-XP indicator on the XP bar: a pale blue stretch ahead of the
//! purple fill showing how far the banked rested XP will carry, as on the
//! classic bar. Hover the bar for the numbers.

use bevy::prelude::*;

use super::Tip;
use crate::ui::XpFill;
use crate::Session;

/// (start, width) of the rested stretch as fractions of the bar. It begins
/// where the XP fill ends and never runs past the end of the level.
pub fn rested_span(xp: u32, max_xp: u32, rested: u32) -> (f32, f32) {
    if max_xp == 0 || rested == 0 {
        return (0.0, 0.0);
    }
    let start = (xp as f32 / max_xp as f32).clamp(0.0, 1.0);
    let width = (rested as f32 / max_xp as f32).min(1.0 - start).max(0.0);
    (start, width)
}

#[derive(Component)]
pub(crate) struct RestedFill;

pub(crate) fn update(
    mut commands: Commands,
    session: Res<Session>,
    bar: Query<&Parent, With<XpFill>>,
    mut fill: Query<(&mut Node, &mut Tip), With<RestedFill>>,
) {
    let Ok((mut node, mut tip)) = fill.get_single_mut() else {
        // First frame after the HUD exists: hang the stretch on the XP bar.
        if let Ok(parent) = bar.get_single() {
            let child = commands
                .spawn((
                    Node {
                        position_type: PositionType::Absolute,
                        left: Val::Percent(0.0),
                        top: Val::Px(0.0),
                        width: Val::Percent(0.0),
                        height: Val::Percent(100.0),
                        ..default()
                    },
                    BackgroundColor(Color::srgba(0.45, 0.72, 1.0, 0.85)),
                    Interaction::default(),
                    Tip(String::new()),
                    RestedFill,
                ))
                .id();
            commands.entity(parent.get()).add_child(child);
        }
        return;
    };
    let Some(s) = session.sheet.as_ref() else { return };
    let (start, width) = rested_span(s.xp, s.max_xp, s.rested_xp);
    let (l, w) = (Val::Percent(start * 100.0), Val::Percent(width * 100.0));
    if node.left != l || node.width != w {
        node.left = l;
        node.width = w;
    }
    let text = format!("XP {} / {}\nRested bonus banked: {}", s.xp, s.max_xp, s.rested_xp);
    if tip.0 != text {
        tip.0 = text;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rested_stretch_starts_at_the_fill_and_stops_at_the_level() {
        assert_eq!(rested_span(0, 1000, 0), (0.0, 0.0));
        assert_eq!(rested_span(250, 1000, 500), (0.25, 0.5));
        let (start, width) = rested_span(800, 1000, 5000);
        assert!((start - 0.8).abs() < 1e-6 && (width - 0.2).abs() < 1e-6, "capped at the end of the bar");
        assert_eq!(rested_span(10, 0, 50), (0.0, 0.0), "no divide by zero");
        assert_eq!(rested_span(1000, 1000, 50).1, 0.0);
    }
}
