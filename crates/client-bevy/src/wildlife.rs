use bevy::prelude::*;
use rand::Rng;
use crate::player::Player;

#[derive(Component)]
pub struct Wildlife {
    pub flee_range: f32,
    pub speed: f32,
    pub grazing: bool,
    pub graze_timer: f32,
}

pub fn spawn_wildlife(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
) {
    let mut rng = rand::thread_rng();

    // Passive wildlife - deer, rabbits, birds, etc.
    let wildlife_types = [
        (Color::srgb(0.6, 0.4, 0.2), 60.0, 80.0),  // Deer
        (Color::srgb(0.7, 0.6, 0.5), 40.0, 50.0),   // Rabbit
        (Color::srgb(0.3, 0.5, 0.7), 30.0, 30.0),   // Bird
        (Color::srgb(0.5, 0.5, 0.3), 50.0, 60.0),   // Fox
        (Color::srgb(0.4, 0.6, 0.4), 70.0, 90.0),   // Boar
        (Color::srgb(0.8, 0.7, 0.6), 35.0, 40.0),   // Sheep
    ];

    for _ in 0..25 {
        let x = rng.gen_range(-1600.0..1600.0);
        let y = rng.gen_range(-1600.0..1600.0);
        let (color, w, h) = wildlife_types[rng.gen_range(0..wildlife_types.len())];

        commands.spawn((
            Mesh2d(meshes.add(Rectangle::new(w, h))),
            MeshMaterial2d(materials.add(color)),
            Transform::from_xyz(x, y, 0.3),
            Wildlife {
                flee_range: 200.0,
                speed: 120.0,
                grazing: true,
                graze_timer: rng.gen_range(2.0..8.0),
            },
        ));
    }
}

pub fn wildlife_ai(
    mut query: Query<(&mut Transform, &mut Wildlife), Without<Player>>,
    player: Query<&Transform, With<Player>>,
    time: Res<Time>,
) {
    let Ok(player_transform) = player.get_single() else { return };

    for (mut transform, mut wildlife) in query.iter_mut() {
        let pos = transform.translation.truncate();
        let to_player = player_transform.translation.truncate() - pos;
        let dist = to_player.length();

        if dist < wildlife.flee_range {
            // Flee from player
            wildlife.grazing = false;
            let dir = -to_player.normalize();
            transform.translation.x += dir.x * wildlife.speed * time.delta_secs();
            transform.translation.y += dir.y * wildlife.speed * time.delta_secs();
        } else {
            // Grazing behavior - occasional small movements
            wildlife.graze_timer -= time.delta_secs();
            if wildlife.graze_timer <= 0.0 {
                wildlife.graze_timer = rand::thread_rng().gen_range(2.0..8.0);
                wildlife.grazing = !wildlife.grazing;
            }

            if wildlife.grazing {
                let angle = rand::thread_rng().gen_range(0.0..std::f32::consts::TAU);
                let move_dist = 20.0 * time.delta_secs();
                transform.translation.x += angle.cos() * move_dist;
                transform.translation.y += angle.sin() * move_dist;
            }
        }
    }
}
