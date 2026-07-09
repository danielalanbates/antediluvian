use bevy::prelude::*;
use crate::camera;

#[derive(Component)]
pub struct Player;

#[derive(Component)]
pub struct PlayerDirection {
    pub current: Direction,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Down,
    Up,
    Left,
    Right,
}

#[derive(Component)]
pub struct MovementSpeed(pub f32);

#[derive(Component)]
pub struct SpriteHandles {
    pub down: Handle<Image>,
    pub up: Handle<Image>,
    pub left: Handle<Image>,
    pub right: Handle<Image>,
}

pub fn spawn_player(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
) {
    let down = asset_server.load("sprites/player_down.png");
    let up = asset_server.load("sprites/player_up.png");
    let left = asset_server.load("sprites/player_left.png");
    let right = asset_server.load("sprites/player_right.png");

    commands.spawn((
        Sprite {
            image: down.clone(),
            custom_size: Some(Vec2::new(128.0, 160.0)),
            ..default()
        },
        Transform::from_xyz(0.0, 0.0, 1.0),
        Player,
        PlayerDirection { current: Direction::Down },
        MovementSpeed(200.0),
        SpriteHandles { down, up, left, right },
    ));
}

pub fn player_movement(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut query: Query<(&mut Transform, &MovementSpeed), (With<Player>, Without<camera::GameCamera>)>,
    time: Res<Time>,
) {
    let Ok((mut transform, speed)) = query.get_single_mut() else { return };

    let mut direction = Vec2::ZERO;

    if keyboard.pressed(KeyCode::KeyW) || keyboard.pressed(KeyCode::ArrowUp) {
        direction.y += 1.0;
    }
    if keyboard.pressed(KeyCode::KeyS) || keyboard.pressed(KeyCode::ArrowDown) {
        direction.y -= 1.0;
    }
    if keyboard.pressed(KeyCode::KeyA) || keyboard.pressed(KeyCode::ArrowLeft) {
        direction.x -= 1.0;
    }
    if keyboard.pressed(KeyCode::KeyD) || keyboard.pressed(KeyCode::ArrowRight) {
        direction.x += 1.0;
    }

    if direction.length() > 0.0 {
        direction = direction.normalize();
        transform.translation.x += direction.x * speed.0 * time.delta_secs();
        transform.translation.y += direction.y * speed.0 * time.delta_secs();
    }
}

pub fn player_direction(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut query: Query<(&mut Sprite, &mut PlayerDirection, &SpriteHandles), With<Player>>,
) {
    let Ok((mut sprite, mut dir, handles)) = query.get_single_mut() else { return };

    let new_dir = if keyboard.pressed(KeyCode::KeyW) || keyboard.pressed(KeyCode::ArrowUp) {
        Direction::Up
    } else if keyboard.pressed(KeyCode::KeyS) || keyboard.pressed(KeyCode::ArrowDown) {
        Direction::Down
    } else if keyboard.pressed(KeyCode::KeyA) || keyboard.pressed(KeyCode::ArrowLeft) {
        Direction::Left
    } else if keyboard.pressed(KeyCode::KeyD) || keyboard.pressed(KeyCode::ArrowRight) {
        Direction::Right
    } else {
        dir.current
    };

    if new_dir != dir.current {
        dir.current = new_dir;
        sprite.image = match new_dir {
            Direction::Down => handles.down.clone(),
            Direction::Up => handles.up.clone(),
            Direction::Left => handles.left.clone(),
            Direction::Right => handles.right.clone(),
        };
    }
}
