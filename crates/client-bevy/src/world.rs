use bevy::prelude::*;
use rand::Rng;

#[derive(Component)]
pub struct Ground;

pub fn spawn_world(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
) {
    // Ground plane - large green rectangle
    commands.spawn((
        Mesh2d(meshes.add(Rectangle::new(4000.0, 4000.0))),
        MeshMaterial2d(materials.add(Color::srgb(0.2, 0.35, 0.15))),
        Transform::from_xyz(0.0, 0.0, -1.0),
        Ground,
    ));

    // Trees scattered around
    let mut rng = rand::thread_rng();
    for _ in 0..50 {
        let x = rng.gen_range(-1800.0..1800.0);
        let y = rng.gen_range(-1800.0..1800.0);
        spawn_tree(&mut commands, &mut meshes, &mut materials, x, y);
    }

    // Rocks
    for _ in 0..30 {
        let x = rng.gen_range(-1800.0..1800.0);
        let y = rng.gen_range(-1800.0..1800.0);
        spawn_rock(&mut commands, &mut meshes, &mut materials, x, y);
    }
}

fn spawn_tree(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<ColorMaterial>,
    x: f32,
    y: f32,
) {
    // Trunk
    commands.spawn((
        Mesh2d(meshes.add(Rectangle::new(30.0, 120.0))),
        MeshMaterial2d(materials.add(Color::srgb(0.35, 0.25, 0.15))),
        Transform::from_xyz(x, y - 30.0, 0.0),
    ));
    // Canopy
    commands.spawn((
        Mesh2d(meshes.add(Circle::new(60.0))),
        MeshMaterial2d(materials.add(Color::srgb(0.1, 0.45, 0.1))),
        Transform::from_xyz(x, y + 40.0, 0.5),
    ));
}

fn spawn_rock(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<ColorMaterial>,
    x: f32,
    y: f32,
) {
    let size = rand::thread_rng().gen_range(20.0..60.0);
    commands.spawn((
        Mesh2d(meshes.add(Circle::new(size))),
        MeshMaterial2d(materials.add(Color::srgb(0.4, 0.4, 0.42))),
        Transform::from_xyz(x, y, 0.0),
    ));
}
