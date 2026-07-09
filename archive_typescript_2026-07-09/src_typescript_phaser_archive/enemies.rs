use bevy::prelude::*;
use rand::Rng;
use crate::player::Player;

#[derive(Component)]
pub struct Enemy {
    pub health: f32,
    pub speed: f32,
    pub chase_range: f32,
    pub patrol_center: Vec2,
    pub patrol_range: f32,
    pub state: EnemyState,
}

#[derive(Clone, Copy, PartialEq)]
pub enum EnemyState {
    Patrolling,
    Chasing,
}

#[derive(Component)]
pub struct EnemySprite;

pub fn spawn_enemies(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
) {
    let mut rng = rand::thread_rng();

    // Spawn aggressive mobs from the books
    for _ in 0..15 {
        let x = rng.gen_range(-1500.0..1500.0);
        let y = rng.gen_range(-1500.0..1500.0);

        // Skip if too close to center
        if Vec2::new(x, y).length() < 300.0 {
            continue;
        }

        commands.spawn((
            Mesh2d(meshes.add(Rectangle::new(80.0, 100.0))),
            MeshMaterial2d(materials.add(Color::srgb(0.6, 0.1, 0.1))),
            Transform::from_xyz(x, y, 0.5),
            Enemy {
                health: 100.0,
                speed: 80.0,
                chase_range: 400.0,
                patrol_center: Vec2::new(x, y),
                patrol_range: 200.0,
                state: EnemyState::Patrolling,
            },
        ));
    }
}

pub fn enemy_ai(
    mut query: Query<(&mut Transform, &mut Enemy), Without<Player>>,
    player: Query<&Transform, With<Player>>,
    time: Res<Time>,
) {
    let Ok(player_transform) = player.get_single() else { return };

    for (mut transform, mut enemy) in query.iter_mut() {
        let pos = transform.translation.truncate();
        let to_player = player_transform.translation.truncate() - pos;
        let dist = to_player.length();

        if dist < enemy.chase_range {
            enemy.state = EnemyState::Chasing;
            let dir = to_player.normalize();
            transform.translation.x += dir.x * enemy.speed * time.delta_secs();
            transform.translation.y += dir.y * enemy.speed * time.delta_secs();
        } else {
            enemy.state = EnemyState::Patrolling;
            // Simple patrol: move in a circle around patrol center
            let angle = time.elapsed_secs() * 0.5;
            let target_x = enemy.patrol_center.x + angle.cos() * enemy.patrol_range;
            let target_y = enemy.patrol_center.y + angle.sin() * enemy.patrol_range;
            let to_target = Vec2::new(target_x, target_y) - pos;
            if to_target.length() > 5.0 {
                let dir = to_target.normalize();
                transform.translation.x += dir.x * enemy.speed * 0.3 * time.delta_secs();
                transform.translation.y += dir.y * enemy.speed * 0.3 * time.delta_secs();
            }
        }
    }
}
