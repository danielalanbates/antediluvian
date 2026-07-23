use antediluvia_protocol::Act;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};

/// Marker for the sun directional light.
#[derive(Component)]
pub struct Sun;

/// Marker for the sky sphere.
#[derive(Component)]
pub struct SkySphere;

/// Atmosphere colors per act.
pub struct Mood {
    pub fog_color: Color,
    pub fog_density: f32, // for start/end or density
    pub ambient_color: Color,
    pub sun_color: Color,
    pub sky_horizon: Color,
    pub sky_zenith: Color,
}

pub fn act_mood(act: Act) -> Mood {
    match act {
        Act::Eden => Mood {
            fog_color: Color::srgb(0.7, 0.8, 0.6), // soft green-gold
            fog_density: 0.00018,
            ambient_color: Color::srgb(0.9, 0.95, 0.8),
            sun_color: Color::srgb(1.0, 0.95, 0.8),
            sky_horizon: Color::srgb(0.6, 0.75, 0.6),
            sky_zenith: Color::srgb(0.3, 0.5, 0.8),
        },
        Act::Hermon => Mood {
            fog_color: Color::srgb(0.6, 0.7, 0.9), // cool blue
            fog_density: 0.0004,
            ambient_color: Color::srgb(0.8, 0.85, 1.0),
            sun_color: Color::srgb(0.9, 0.9, 1.0),
            sky_horizon: Color::srgb(0.7, 0.8, 0.95),
            sky_zenith: Color::srgb(0.2, 0.3, 0.6),
        },
        Act::Nephilim => Mood {
            fog_color: Color::srgb(0.8, 0.5, 0.4), // dusty red
            fog_density: 0.0005,
            ambient_color: Color::srgb(0.9, 0.7, 0.6),
            sun_color: Color::srgb(1.0, 0.7, 0.5),
            sky_horizon: Color::srgb(0.8, 0.6, 0.4),
            sky_zenith: Color::srgb(0.5, 0.3, 0.2),
        },
        Act::Enoch => Mood {
            fog_color: Color::srgb(0.5, 0.5, 0.55), // smoggy gray
            fog_density: 0.0006,
            ambient_color: Color::srgb(0.7, 0.7, 0.75),
            sun_color: Color::srgb(0.9, 0.9, 0.9),
            sky_horizon: Color::srgb(0.6, 0.6, 0.65),
            sky_zenith: Color::srgb(0.4, 0.4, 0.45),
        },
        Act::Flood => Mood {
            fog_color: Color::srgb(0.15, 0.2, 0.25), // storm-dark
            fog_density: 0.001,
            ambient_color: Color::srgb(0.4, 0.45, 0.5),
            sun_color: Color::srgb(0.6, 0.65, 0.7),
            sky_horizon: Color::srgb(0.25, 0.3, 0.35),
            sky_zenith: Color::srgb(0.1, 0.15, 0.2),
        },
    }
}

pub fn generate_sky_gradient(images: &mut Assets<Image>, zenith: Color, horizon: Color) -> Handle<Image> {
    let size = Extent3d { width: 1, height: 128, depth_or_array_layers: 1 };
    let mut data = Vec::with_capacity(128 * 4);
    for y in 0..128 {
        // y=0 is bottom (horizon), y=127 is top (zenith)
        let t = y as f32 / 127.0;
        // Simple linear interpolation
        let cz = zenith.to_srgba();
        let ch = horizon.to_srgba();
        let r = ch.red * (1.0 - t) + cz.red * t;
        let g = ch.green * (1.0 - t) + cz.green * t;
        let b = ch.blue * (1.0 - t) + cz.blue * t;
        data.push((r * 255.0) as u8);
        data.push((g * 255.0) as u8);
        data.push((b * 255.0) as u8);
        data.push(255);
    }
    images.add(Image::new(
        size,
        TextureDimension::D2,
        data,
        TextureFormat::Rgba8UnormSrgb,
        bevy::render::render_asset::RenderAssetUsages::RENDER_WORLD,
    ))
}

pub fn spawn_sky(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    images: &mut Assets<Image>,
    mood: &Mood,
) -> Entity {
    // We want the sphere to be visible from the inside, but Bevy's default sphere
    // has normals pointing out. We can cull front faces or disable culling.
    let texture = generate_sky_gradient(images, mood.sky_zenith, mood.sky_horizon);
    
    let material = materials.add(StandardMaterial {
        base_color_texture: Some(texture),
        unlit: true,
        cull_mode: None, // render inside
        ..default()
    });

    // Huge sphere that bounds the camera
    commands.spawn((
        Mesh3d(meshes.add(Sphere::new(4000.0).mesh().uv(32, 18))),
        MeshMaterial3d(material),
        Transform::default(),
        SkySphere,
    )).id()
}

/// System to update the sun, ambient light, fog, and sky position based on `time_of_day`.
pub fn update_atmosphere(
    session: Res<crate::Session>,
    time: Res<Time>,
    mut commands: Commands,
    mut meshes: Local<Option<Handle<Mesh>>>,
    mut materials: Local<Option<Handle<StandardMaterial>>>,
    mut q_sun: Query<(&mut Transform, &mut DirectionalLight), (With<Sun>, Without<crate::MainCamera>, Without<SkySphere>)>,
    mut q_sky: Query<&mut Transform, (With<SkySphere>, Without<crate::MainCamera>, Without<Sun>)>,
    mut q_cam: Query<(Entity, &Transform, &mut DistanceFog), (With<crate::MainCamera>, Without<Sun>, Without<SkySphere>)>,
    mut ambient: ResMut<AmbientLight>,
    mut global_meshes: ResMut<Assets<Mesh>>,
    mut global_materials: ResMut<Assets<StandardMaterial>>,
) {
    let t = session.time_of_day; // 0.0 to 1.0
    // Time of day logic: 
    // 0.25 = sunrise (sun at horizon)
    // 0.5 = noon (sun at zenith)
    // 0.75 = sunset (sun at horizon)
    // 0.0 / 1.0 = midnight (sun below horizon)
    
    // Convert t to an angle where 0.5 is -PI/2 (looking down), 0.25 is 0 (looking horizontal).
    // angle = (t - 0.25) * 2PI. At 0.25 -> 0. At 0.5 -> PI/2.
    let angle = (t - 0.25) * std::f32::consts::TAU;

    // Ambient light: minimum ~80 at night.
    let mood = act_mood(session.act);
    
    // Pitch around X axis.
    if let Ok((mut sun_tf, mut sun_light)) = q_sun.get_single_mut() {
        sun_tf.rotation = Quat::from_euler(EulerRot::XYZ, -angle, 0.6, 0.0);
        
        // Intensity peaks at noon, zero at night.
        let is_day = t > 0.2 && t < 0.8;
        let day_factor = if is_day { (angle.sin()).max(0.0) } else { 0.0 };
        sun_light.illuminance = 22_000.0 * day_factor;
        sun_light.color = mood.sun_color;
    }

    let day_factor = (angle.sin()).max(0.0);
    ambient.brightness = 260.0 + 520.0 * day_factor;
    ambient.color = mood.ambient_color;

    // Cave interiors (C09): darken ambient + tighten fog inside a pocket.
    let in_cave = q_cam.get_single().ok().map(|(_, cam_tf, _)| {
        let p = cam_tf.translation;
        crate::caves_for_act(session.act)
            .any(|c| (p.x - c.x).hypot(p.z - c.y) < 260.0)
    }).unwrap_or(false);
    if in_cave {
        ambient.brightness *= 0.25;
    }

    if let Ok((cam_ent, cam_tf, mut fog)) = q_cam.get_single_mut() {
        // Update fog color (darken slightly at night, but mostly retain mood)
        let night_dim = 0.3 + 0.7 * day_factor;
        let base_fog = mood.fog_color.to_srgba();
        let cave_dim = if in_cave { 0.35 } else { 1.0 };
        fog.color = Color::srgba(
            base_fog.red * night_dim * cave_dim,
            base_fog.green * night_dim * cave_dim,
            base_fog.blue * night_dim * cave_dim,
            1.0
        );
        fog.falloff = FogFalloff::Exponential {
            density: mood.fog_density * if in_cave { 4.0 } else { 1.0 },
        };

        // Keep sky centered on camera
        if let Ok(mut sky_tf) = q_sky.get_single_mut() {
            sky_tf.translation = cam_tf.translation;
        }

        // Spawn rain in Flood act
        if session.act == Act::Flood {
            let mesh = meshes.get_or_insert_with(|| global_meshes.add(Sphere::new(1.0))).clone();
            let mat = materials.get_or_insert_with(|| global_materials.add(StandardMaterial {
                base_color: Color::srgba(0.6, 0.7, 0.8, 0.4),
                unlit: true,
                alpha_mode: AlphaMode::Blend,
                ..default()
            })).clone();

            let spawn_count = 5; // Drops per frame
            for i in 0..spawn_count {
                let s = time.elapsed_secs_f64() * 1000.0 + i as f64;
                let x = (s.sin() * 41.0 % 1.0) as f32 * 400.0 - 200.0;
                let y = 150.0 + (s.cos() * 53.0 % 1.0) as f32 * 50.0;
                let z = (s.sin() * 79.0 % 1.0) as f32 * -300.0 - 50.0; // In front of camera
                
                let mut p = commands.spawn((
                    Mesh3d(mesh.clone()),
                    MeshMaterial3d(mat.clone()),
                    Transform::from_xyz(x, y, z).with_scale(Vec3::new(0.5, 12.0, 0.5)),
                    crate::vfx::Particle {
                        vel: Vec3::new(-10.0, -300.0, 0.0), // Falling fast, slightly diagonal
                        life: 1.0,
                        max_life: 1.0,
                        base_scale: 1.0,
                    },
                ));
                p.set_parent(cam_ent);
            }
        }
    }
}

