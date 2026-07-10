//! Hand-rolled combat/spell particles — no GPU-particle dependency, just
//! small unlit spheres with velocity, gravity, shrink and a despawn timer.
//! At this art style ~15 particles per burst reads perfectly well.

use bevy::prelude::*;

#[derive(Component)]
pub struct Particle {
    vel: Vec3,
    life: f32,
    max_life: f32,
    base_scale: f32,
}

/// Cached particle mesh + palette so bursts never allocate new assets.
#[derive(Resource)]
pub struct VfxAssets {
    mesh: Handle<Mesh>,
    pub cast: Handle<StandardMaterial>,
    pub heal: Handle<StandardMaterial>,
    pub hit: Handle<StandardMaterial>,
    pub die: Handle<StandardMaterial>,
    pub levelup: Handle<StandardMaterial>,
}

pub fn init_vfx(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let mat = |c: Color, materials: &mut Assets<StandardMaterial>| {
        materials.add(StandardMaterial {
            base_color: c,
            unlit: true,
            alpha_mode: AlphaMode::Blend,
            ..default()
        })
    };
    let vfx = VfxAssets {
        mesh: meshes.add(Sphere::new(7.5)),
        cast: mat(Color::srgba(1.0, 0.55, 0.15, 0.95), &mut materials),
        heal: mat(Color::srgba(1.0, 0.95, 0.55, 0.95), &mut materials),
        hit: mat(Color::srgba(0.95, 0.15, 0.10, 0.95), &mut materials),
        die: mat(Color::srgba(0.55, 0.55, 0.58, 0.9), &mut materials),
        levelup: mat(Color::srgba(1.0, 0.85, 0.25, 0.95), &mut materials),
    };
    commands.insert_resource(vfx);
}

/// Deterministic-enough direction spread without an RNG dependency.
fn dir(i: usize, n: usize, up_bias: f32) -> Vec3 {
    let golden = 2.399_963; // golden-angle spiral
    let a = i as f32 * golden;
    let y = up_bias + (1.0 - up_bias) * (i as f32 / n.max(1) as f32);
    Vec3::new(a.cos(), y, a.sin()).normalize()
}

pub fn spawn_burst(
    commands: &mut Commands,
    vfx: &VfxAssets,
    material: Handle<StandardMaterial>,
    pos: Vec3,
    n: usize,
    speed: f32,
    life: f32,
    up_bias: f32,
) {
    for i in 0..n {
        let jitter = 0.6 + 0.8 * ((i * 2654435761) % 97) as f32 / 97.0;
        commands.spawn((
            Mesh3d(vfx.mesh.clone()),
            MeshMaterial3d(material.clone()),
            Transform::from_translation(pos),
            Particle {
                vel: dir(i, n, up_bias) * speed * jitter,
                life,
                max_life: life,
                base_scale: jitter,
            },
        ));
    }
}

pub fn update_vfx(
    time: Res<Time>,
    mut commands: Commands,
    mut parts: Query<(Entity, &mut Transform, &mut Particle)>,
) {
    let dt = time.delta_secs();
    for (ent, mut t, mut p) in parts.iter_mut() {
        p.life -= dt;
        if p.life <= 0.0 {
            commands.entity(ent).despawn();
            continue;
        }
        p.vel.y -= 90.0 * dt; // light gravity
        let vel = p.vel;
        t.translation += vel * dt;
        let frac = (p.life / p.max_life).clamp(0.0, 1.0);
        t.scale = Vec3::splat(p.base_scale * frac);
    }
}

/// Marker + pulse for the inn's rested-XP ring.
#[derive(Component)]
pub struct InnRing(pub Handle<StandardMaterial>);

pub fn pulse_inn_ring(
    time: Res<Time>,
    rings: Query<&InnRing>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let a = 0.28 + 0.12 * (time.elapsed_secs() * 1.4).sin();
    for InnRing(handle) in rings.iter() {
        if let Some(m) = materials.get_mut(handle) {
            m.base_color = m.base_color.with_alpha(a);
        }
    }
}
