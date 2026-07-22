//! Fidelity pass (v0.5.0): dense grass cover that follows the camera.
//!
//! One shared crossed-quad mesh + one material → the whole field renders as
//! a single instanced batch. Tufts sit on a deterministic grid keyed by cell
//! hash, repositioned only when the player crosses into a new cell, so the
//! ground near the camera always reads as vegetated without a world-sized
//! entity count.

use crate::terrain::{road_dist, terrain_height, water_level};
use bevy::prelude::*;
use bevy::render::mesh::{Indices, PrimitiveTopology};
use bevy::render::render_asset::RenderAssetUsages;

/// Grid cell edge in world units; tufts per cell.
const CELL: f32 = 16.0;
/// Grass ring radius around the player.
const RADIUS: f32 = 240.0;
const TUFTS_PER_CELL: usize = 2;

fn h01(seed: u64) -> f32 {
    let mut x = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15);
    x ^= x >> 33;
    x = x.wrapping_mul(0xC2B2_AE3D_27D4_EB4F);
    ((x >> 40) & 0xFFFFFF) as f32 / 16_777_215.0
}

#[derive(Component)]
pub struct GrassTuft(pub usize);

#[derive(Resource)]
pub struct GrassState {
    pub cell: (i64, i64),
    pub act: antediluvia_protocol::Act,
}

/// Two crossed quads leaning slightly outward — reads as a grass clump.
fn tuft_mesh() -> Mesh {
    let mut positions: Vec<[f32; 3]> = Vec::new();
    let mut uvs: Vec<[f32; 2]> = Vec::new();
    let mut indices: Vec<u32> = Vec::new();
    for (i, ang) in [0.0f32, std::f32::consts::FRAC_PI_2].iter().enumerate() {
        let (c, s) = (ang.cos(), ang.sin());
        let w = 1.4;
        let h = 4.6;
        let base = (i * 4) as u32;
        positions.extend_from_slice(&[
            [-w * c, 0.0, -w * s],
            [w * c, 0.0, w * s],
            [w * c, h, w * s],
            [-w * c, h, -w * s],
        ]);
        uvs.extend_from_slice(&[[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]]);
        indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
        // Back faces so the quad reads from every side.
        indices.extend_from_slice(&[base, base + 2, base + 1, base, base + 3, base + 2]);
    }
    // Upward normals so grass catches sky light without per-face computation
    // (compute_flat_normals panics on indexed meshes).
    let normals: Vec<[f32; 3]> = (0..positions.len()).map(|_| [0.0, 1.0, 0.0]).collect();
    let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh.insert_indices(Indices::U32(indices));
    mesh
}

pub fn init_grass(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let mesh = meshes.add(tuft_mesh());
    let mat = materials.add(StandardMaterial {
        base_color: Color::srgb(0.45, 0.66, 0.30),
        emissive: LinearRgba::rgb(0.04, 0.07, 0.02),
        perceptual_roughness: 1.0,
        reflectance: 0.05,
        cull_mode: None,
        double_sided: true,
        ..default()
    });
    let per_side = (RADIUS * 2.0 / CELL) as usize + 1;
    let count = per_side * per_side * TUFTS_PER_CELL;
    for i in 0..count {
        commands.spawn((
            Mesh3d(mesh.clone()),
            MeshMaterial3d(mat.clone()),
            Transform::from_xyz(0.0, -10_000.0, 0.0),
            GrassTuft(i),
        ));
    }
    commands.insert_resource(GrassState {
        cell: (i64::MIN, i64::MIN),
        act: antediluvia_protocol::Act::Eden,
    });
}

/// Re-seat the tuft field when the player crosses a cell boundary or zones.
pub fn update_grass(
    session: Res<crate::Session>,
    q_player: Query<&Transform, (With<crate::PlayerTag>, Without<GrassTuft>)>,
    mut state: ResMut<GrassState>,
    mut tufts: Query<(&GrassTuft, &mut Transform), Without<crate::PlayerTag>>,
) {
    let Ok(pt) = q_player.get_single() else { return };
    let act = session.act;
    let cell = (
        (pt.translation.x / CELL).floor() as i64,
        (pt.translation.z / CELL).floor() as i64,
    );
    if cell == state.cell && act == state.act {
        return;
    }
    state.cell = cell;
    state.act = act;
    let per_side = (RADIUS * 2.0 / CELL) as i64 + 1;
    let half = per_side / 2;
    for (tuft, mut t) in &mut tufts {
        let i = tuft.0 as i64;
        let slot = i / TUFTS_PER_CELL as i64;
        let k = i % TUFTS_PER_CELL as i64;
        let cx = cell.0 - half + slot % per_side;
        let cz = cell.1 - half + slot / per_side;
        let seed = (cx as u64)
            .wrapping_mul(0x9E37_79B9)
            .wrapping_add((cz as u64).wrapping_mul(0x85EB_CA6B))
            .wrapping_add(k as u64);
        let x = cx as f32 * CELL + h01(seed ^ 1) * CELL;
        let z = cz as f32 * CELL + h01(seed ^ 2) * CELL;
        // Skip roads, water, and steep rock — hide the tuft below ground.
        let y = terrain_height(act, x, z);
        let on_road = road_dist(x, z) <= 34.0;
        let under_water = water_level(act).map(|w| y < w + 1.0).unwrap_or(false);
        if on_road || under_water || h01(seed ^ 3) < 0.42 {
            t.translation = Vec3::new(x, -10_000.0, z);
            continue;
        }
        let scale = 0.7 + h01(seed ^ 4) * 0.9;
        t.translation = Vec3::new(x, y - 0.2, z);
        t.rotation = Quat::from_rotation_y(h01(seed ^ 5) * std::f32::consts::TAU);
        t.scale = Vec3::new(scale, scale * (0.8 + h01(seed ^ 6) * 0.6), scale);
    }
}
