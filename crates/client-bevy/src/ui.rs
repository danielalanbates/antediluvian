use bevy::prelude::*;
use crate::player::Player;
use crate::combat::Health;

#[derive(Component)]
pub struct HealthBar;

#[derive(Component)]
pub struct DialogueBox;

pub fn setup_ui(
    mut commands: Commands,
    player_query: Query<Entity, With<Player>>,
) {
    // Health bar background
    commands.spawn((
        Node {
            position_type: PositionType::Absolute,
            top: Val::Px(20.0),
            left: Val::Px(20.0),
            width: Val::Px(300.0),
            height: Val::Px(30.0),
            ..default()
        },
        BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.6)),
        BorderColor(Color::srgb(0.3, 0.3, 0.3)),
        BorderRadius::all(Val::Px(5.0)),
    )).with_children(|parent| {
        // Health bar fill
        parent.spawn((
            Node {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                ..default()
            },
            BackgroundColor(Color::srgb(0.8, 0.15, 0.15)),
            BorderRadius::all(Val::Px(5.0)),
            HealthBar,
        ));
    });

    // Dialogue box
    commands.spawn((
        Node {
            position_type: PositionType::Absolute,
            bottom: Val::Px(20.0),
            left: Val::Px(20.0),
            right: Val::Px(20.0),
            min_height: Val::Px(80.0),
            padding: UiRect::all(Val::Px(15.0)),
            ..default()
        },
        BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.75)),
        BorderColor(Color::srgb(0.5, 0.5, 0.5)),
        BorderRadius::all(Val::Px(8.0)),
        DialogueBox,
    )).with_children(|parent| {
        parent.spawn((
            Text::new("Welcome to Bates MMORPG - WASD to move, Space to attack"),
            TextFont {
                font_size: 20.0,
                ..default()
            },
            TextColor(Color::srgb(0.9, 0.9, 0.9)),
        ));
    });

    // Add Health component to player
    if let Ok(player_entity) = player_query.get_single() {
        commands.entity(player_entity).insert(Health {
            current: 100.0,
            max: 100.0,
        });
    }
}

pub fn update_health_bar(
    player_query: Query<&Health, With<Player>>,
    mut health_bar_query: Query<&mut Node, With<HealthBar>>,
) {
    let Ok(health) = player_query.get_single() else { return };
    let Ok(mut node) = health_bar_query.get_single_mut() else { return };

    let pct = (health.current / health.max * 100.0).max(0.0).min(100.0);
    node.width = Val::Percent(pct);
}
