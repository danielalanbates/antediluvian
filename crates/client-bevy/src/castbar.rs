// Copyright (c) 2026 Daniel Bates / Bates LLC. All rights reserved.
//! Cast bar (B03): a progress bar above the action bar while a cast-time
//! ability is winding up. Driven by `Session.cast`, which `ServerMsg::CastStart`
//! sets and `AbilityUsed` (or an interrupt, `secs == 0`) clears.

use bevy::prelude::*;

use crate::Session;

const WIDTH: f32 = 260.0;
const PANEL: Color = Color::srgba(0.08, 0.08, 0.12, 0.85);
const FILL: Color = Color::srgb(0.95, 0.72, 0.20);
const TEXT: Color = Color::srgb(0.92, 0.90, 0.85);

#[derive(Component)]
pub(crate) struct CastBarRoot;
#[derive(Component)]
pub(crate) struct CastBarFill;
#[derive(Component)]
pub(crate) struct CastBarText;

pub(crate) fn spawn_cast_bar(mut commands: Commands) {
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                bottom: Val::Px(112.0),
                left: Val::Percent(50.0),
                margin: UiRect::left(Val::Px(-WIDTH / 2.0)),
                width: Val::Px(WIDTH),
                height: Val::Px(20.0),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                border: UiRect::all(Val::Px(1.0)),
                ..default()
            },
            BackgroundColor(PANEL),
            BorderColor(Color::srgb(0.45, 0.40, 0.28)),
            Visibility::Hidden,
            CastBarRoot,
        ))
        .with_children(|bar| {
            bar.spawn((
                Node {
                    position_type: PositionType::Absolute,
                    left: Val::Px(0.0),
                    top: Val::Px(0.0),
                    width: Val::Percent(0.0),
                    height: Val::Percent(100.0),
                    ..default()
                },
                BackgroundColor(FILL),
                CastBarFill,
            ));
            bar.spawn((
                Text::new(""),
                TextFont { font_size: 13.0, ..default() },
                TextColor(TEXT),
                TextLayout::new_with_no_wrap(),
                CastBarText,
            ));
        });
}

pub(crate) fn update_cast_bar(
    time: Res<Time>,
    mut session: ResMut<Session>,
    mut q_root: Query<&mut Visibility, With<CastBarRoot>>,
    mut q_fill: Query<&mut Node, With<CastBarFill>>,
    mut q_text: Query<&mut Text, With<CastBarText>>,
) {
    let Ok(mut vis) = q_root.get_single_mut() else { return };
    let now = time.elapsed_secs();
    // A cast whose `AbilityUsed` never arrived (lost, or refused at the end)
    // must not leave the bar stuck.
    if session.cast.as_ref().is_some_and(|(_, _, end)| now > *end + 0.5) {
        session.cast = None;
    }
    let Some((id, start, end)) = session.cast.as_ref() else {
        if *vis != Visibility::Hidden {
            *vis = Visibility::Hidden;
        }
        return;
    };
    *vis = Visibility::Inherited;
    let frac = if end > start { ((now - start) / (end - start)).clamp(0.0, 1.0) } else { 1.0 };
    if let Ok(mut n) = q_fill.get_single_mut() {
        n.width = Val::Percent(frac * 100.0);
    }
    if let Ok(mut t) = q_text.get_single_mut() {
        let name = antediluvia_sim::abilities::ability(id).map_or(id.as_str(), |a| a.name);
        let line = format!("{name}  {:.1}s", (end - now).max(0.0));
        if **t != line {
            **t = line;
        }
    }
}
