use bevy::prelude::*;

mod player;
mod camera;
mod world;
mod enemies;
mod wildlife;
mod combat;
mod ui;

use player::*;
use camera::*;
use world::*;
use enemies::*;
use wildlife::*;
use combat::*;
use ui::*;

fn main() {
    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "Bates MMORPG".into(),
                resolution: (1920.0, 1080.0).into(),
                resizable: true,
                ..default()
            }),
            ..default()
        }))
        .insert_resource(ClearColor(Color::srgb(0.15, 0.2, 0.1)))
        .add_systems(Startup, (
            setup_camera,
            spawn_world,
            spawn_player,
            spawn_wildlife,
            spawn_enemies,
            setup_ui,
        ))
        .add_systems(Update, (
            player_movement,
            player_direction,
            enemy_ai,
            wildlife_ai,
            combat_system,
            update_health_bar,
            camera_follow_player,
        ))
        .run();
}
