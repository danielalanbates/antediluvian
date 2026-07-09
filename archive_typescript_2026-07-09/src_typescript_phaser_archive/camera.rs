use bevy::prelude::*;
use crate::player::Player;

#[derive(Component)]
pub struct GameCamera;

pub fn setup_camera(mut commands: Commands) {
    commands.spawn((
        Camera2d,
        GameCamera,
    ));
}

pub fn camera_follow_player(
    mut camera_query: Query<&mut Transform, (With<GameCamera>, Without<Player>)>,
    player: Query<&Transform, With<Player>>,
) {
    let Ok(mut cam_transform) = camera_query.get_single_mut() else { return };
    let Ok(player_transform) = player.get_single() else { return };

    cam_transform.translation.x = player_transform.translation.x;
    cam_transform.translation.y = player_transform.translation.y;
}
