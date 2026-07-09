use bevy::prelude::*;
use crate::player::Player;
use crate::enemies::Enemy;
use crate::wildlife::Wildlife;

#[derive(Component)]
pub struct AttackHitbox;

#[derive(Component)]
pub struct AttackActive {
    pub timer: f32,
}

#[derive(Component)]
pub struct Health {
    pub current: f32,
    pub max: f32,
}

pub fn combat_system(
    mut commands: Commands,
    keyboard: Res<ButtonInput<KeyCode>>,
    mut player_query: Query<&mut Transform, (With<Player>, Without<Enemy>, Without<Wildlife>)>,
    mut enemy_query: Query<(Entity, &Transform, &mut Enemy), Without<Player>>,
    mut attack_query: Query<(Entity, &mut AttackActive)>,
    mut health_query: Query<&mut Health, With<Player>>,
    time: Res<Time>,
) {
    let Ok(mut player_transform) = player_query.get_single_mut() else { return };

    // Attack input
    if keyboard.just_pressed(KeyCode::Space) {
        if attack_query.iter().count() == 0 {
            commands.spawn((
                AttackActive { timer: 0.3 },
                AttackHitbox,
                Transform::from_translation(player_transform.translation),
            ));
        }
    }

    // Update attack timers
    for (entity, mut attack) in attack_query.iter_mut() {
        attack.timer -= time.delta_secs();
        if attack.timer <= 0.0 {
            commands.entity(entity).despawn();
        }
    }

    // Check hitbox collisions with enemies
    for (_, enemy_transform, mut enemy) in enemy_query.iter_mut() {
        let dist = player_transform.translation.distance(enemy_transform.translation);
        if dist < 120.0 {
            // Check if there's an active attack
            if attack_query.iter().count() > 0 {
                enemy.health -= 25.0;
                if enemy.health <= 0.0 {
                    // Will be despawned next frame
                }
            }
        }
    }

    // Enemy damage to player
    if let Ok(mut health) = health_query.get_single_mut() {
        for (_, enemy_transform, _) in enemy_query.iter_mut() {
            let dist = player_transform.translation.distance(enemy_transform.translation);
            if dist < 60.0 {
                health.current -= 10.0 * time.delta_secs();
                if health.current <= 0.0 {
                    health.current = health.max;
                    player_transform.translation = Vec3::new(0.0, 0.0, 1.0);
                }
            }
        }
    }
}
