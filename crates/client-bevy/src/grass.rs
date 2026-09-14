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
const RADIUS: f32 = 210.0;
const TUFTS_PER_CELL: usize = 4;

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

/// A clump of thin, curved, individually tinted blades (realism pass
/// 2026-09-14). The old tuft was three wide triangles in one saturated green,
/// which read as a carpet of spikes. Real grass is many narrow blades that
/// bend under their own weight, dark and dense at the root, varied in hue,
/// with a few dry straw-coloured tips. `detail` = blades per clump.
fn clump_mesh(blades: usize, segments: usize) -> Mesh {
    let mut positions: Vec<[f32; 3]> = Vec::new();
    let mut normals: Vec<[f32; 3]> = Vec::new();
    let mut uvs: Vec<[f32; 2]> = Vec::new();
    let mut colors: Vec<[f32; 4]> = Vec::new();
    let mut indices: Vec<u32> = Vec::new();
    for bi in 0..blades {
        let r = |k: u64| h01(bi as u64 * 7919 + k);
        let ang = r(1) * std::f32::consts::TAU;
        let (c, s) = (ang.cos(), ang.sin());
        // Root offset inside the clump, lean direction, height and width.
        let ox = (r(2) - 0.5) * 5.0;
        let oz = (r(3) - 0.5) * 5.0;
        let lean_ang = r(4) * std::f32::consts::TAU;
        let lean = Vec3::new(lean_ang.cos(), 0.0, lean_ang.sin()) * (0.6 + r(5) * 1.8);
        let h = 6.0 + r(6) * 7.0;
        let w = 0.35 + r(7) * 0.3;
        // Per-blade tint: mostly green, some olive, ~12% dry straw.
        let dry = r(8) < 0.12;
        let root = [0.16, 0.19, 0.08, 1.0];
        let tip = if dry {
            [0.62, 0.55, 0.30, 1.0]
        } else {
            let g = 0.50 + r(9) * 0.2;
            [0.30 + r(10) * 0.14, g, 0.14 + r(11) * 0.06, 1.0]
        };
        let side = Vec3::new(c, 0.0, s);
        let face = Vec3::new(-s, 0.0, c);
        let base = positions.len() as u32;
        for k in 0..=segments {
            let t = k as f32 / segments as f32;
            // Quadratic bend: tips droop outward along `lean`.
            let p = Vec3::new(ox, t * h, oz) + lean * 1.8 * t * t;
            let half = w * (1.0 - t * 0.92);
            let a = p - side * half;
            let b = p + side * half;
            positions.push(a.to_array());
            positions.push(b.to_array());
            // Normal: mostly up (grass is lit like the ground it covers) with
            // a little of the blade face for soft shading variation.
            let n = (Vec3::Y * 0.8 + face * 0.2 + lean.normalize_or_zero() * 0.15 * t).normalize();
            normals.push(n.to_array());
            normals.push(n.to_array());
            let col = [
                root[0] + (tip[0] - root[0]) * t.powf(0.6),
                root[1] + (tip[1] - root[1]) * t.powf(0.6),
                root[2] + (tip[2] - root[2]) * t.powf(0.6),
                1.0,
            ];
            colors.push(col);
            colors.push(col);
            uvs.push([0.0, 1.0 - t]);
            uvs.push([1.0, 1.0 - t]);
            if k < segments {
                let i = base + k as u32 * 2;
                indices.extend_from_slice(&[i, i + 1, i + 3, i, i + 3, i + 2]);
            }
        }
    }
    let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors);
    mesh.insert_indices(Indices::U32(indices));
    mesh
}

/// Near clump: 14 blades, 3 segments each (visible bend up close).
fn tuft_mesh() -> Mesh {
    clump_mesh(18, 3)
}

/// Far clump: fewer, straighter blades — the silhouette is all that reads.
fn far_tuft_mesh() -> Mesh {
    clump_mesh(8, 1)
}

/// Shared grass material: vertex colours carry the real hue, so the base is
/// white; slightly glossy blades catch the sky IBL.
fn grass_material() -> StandardMaterial {
    StandardMaterial {
        base_color: Color::srgb(1.0, 1.0, 1.0),
        perceptual_roughness: 0.78,
        reflectance: 0.25,
        diffuse_transmission: 0.0,
        cull_mode: None,
        double_sided: true,
        ..default()
    }
}


pub fn init_grass(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let mesh = meshes.add(tuft_mesh());
    let mat = materials.add(grass_material());
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
        if on_road || under_water || h01(seed ^ 3) < 0.22 {
            t.translation = Vec3::new(x, -10_000.0, z);
            continue;
        }
        let scale = 0.7 + h01(seed ^ 4) * 0.9;
        t.translation = Vec3::new(x, y - 0.2, z);
        t.rotation = Quat::from_rotation_y(h01(seed ^ 5) * std::f32::consts::TAU);
        t.scale = Vec3::new(scale, scale * (0.8 + h01(seed ^ 6) * 0.6), scale);
    }
}

/// Wind: sway every visible tuft with a travelling sine wave, so the meadow
/// ripples like a real field instead of standing rigid.
pub fn sway_grass(time: Res<Time>, mut tufts: Query<(&GrassTuft, &mut Transform)>) {
    let t = time.elapsed_secs();
    for (tuft, mut tf) in &mut tufts {
        if tf.translation.y < -1000.0 {
            continue; // hidden tuft
        }
        let phase = (tf.translation.x + tf.translation.z) * 0.05;
        // Two octaves: a slow roll plus a faster shimmer.
        let sway = (t * 1.3 + phase).sin() * 0.14 + (t * 3.1 + phase * 1.7).sin() * 0.05;
        let yaw = h01(tuft.0 as u64 ^ 0x5EED) * std::f32::consts::TAU;
        tf.rotation = Quat::from_rotation_y(yaw)
            * Quat::from_rotation_x(sway)
            * Quat::from_rotation_z(sway * 0.6);
    }
}

// ─── Far grass ───────────────────────────────────────────────────────────────
//
// The swaying tuft field above only reaches RADIUS (~3 character heights), so
// the meadow visibly ended just past the player. Beyond it, grass comes from
// static merged meshes: one per FAR_CHUNK square, sparser, no per-tuft
// entities or sway (invisible at range). A chunk's mesh depends only on its
// coordinates and act, so it is built once and reused as the player moves.

const FAR_CHUNK: f32 = 120.0;
const FAR_CELL: f32 = 20.0;
const FAR_RADIUS: f32 = 1000.0;

#[derive(Resource, Default)]
pub struct FarGrass {
    centre: Option<(i64, i64, antediluvia_protocol::Act)>,
    spawned: std::collections::HashMap<(i64, i64), Entity>,
    cache: std::collections::HashMap<(antediluvia_protocol::Act, i64, i64), Handle<Mesh>>,
    material: Option<Handle<StandardMaterial>>,
}

#[derive(Component)]
pub struct FarGrassChunk;

/// Bake every tuft of one chunk into a single mesh, in chunk-local space.
/// Placement rules (roads, water, gaps) match `update_grass`.
pub fn far_chunk_mesh(act: antediluvia_protocol::Act, cx: i64, cz: i64) -> Option<Mesh> {
    let tuft = far_tuft_mesh();
    let tp = tuft.attribute(Mesh::ATTRIBUTE_POSITION)?.as_float3()?.to_vec();
    let tc = match tuft.attribute(Mesh::ATTRIBUTE_COLOR)? {
        bevy::render::mesh::VertexAttributeValues::Float32x4(v) => v.clone(),
        _ => return None,
    };
    let ti: Vec<u32> = match tuft.indices()? {
        Indices::U32(v) => v.clone(),
        Indices::U16(v) => v.iter().map(|&i| i as u32).collect(),
    };
    let origin = Vec3::new(cx as f32 * FAR_CHUNK, 0.0, cz as f32 * FAR_CHUNK);
    let cells = (FAR_CHUNK / FAR_CELL) as i64;
    let (mut pos, mut col, mut idx) = (Vec::new(), Vec::new(), Vec::new());
    for gz in 0..cells {
        for gx in 0..cells {
            let seed = ((cx * cells + gx) as u64)
                .wrapping_mul(0x9E37_79B9)
                .wrapping_add(((cz * cells + gz) as u64).wrapping_mul(0x85EB_CA6B))
                ^ 0xFA12;
            let x = origin.x + (gx as f32 + h01(seed ^ 1)) * FAR_CELL;
            let z = origin.z + (gz as f32 + h01(seed ^ 2)) * FAR_CELL;
            let y = terrain_height(act, x, z);
            let on_road = road_dist(x, z) <= 34.0;
            let under_water = water_level(act).map(|w| y < w + 1.0).unwrap_or(false);
            if on_road || under_water || h01(seed ^ 3) < 0.18 {
                continue;
            }
            // Slightly larger than near tufts so the sparser field still reads.
            let s = 1.0 + h01(seed ^ 4) * 0.9;
            let xf = Transform::from_translation(Vec3::new(x, y - 0.2, z) - origin)
                .with_rotation(Quat::from_rotation_y(h01(seed ^ 5) * std::f32::consts::TAU))
                .with_scale(Vec3::new(s, s * (0.8 + h01(seed ^ 6) * 0.6), s));
            let base = pos.len() as u32;
            pos.extend(tp.iter().map(|p| xf.transform_point(Vec3::from(*p)).to_array()));
            col.extend(tc.iter().copied());
            idx.extend(ti.iter().map(|i| base + i));
        }
    }
    if pos.is_empty() {
        return None;
    }
    let normals = vec![[0.0, 1.0, 0.0]; pos.len()];
    let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, pos);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, col);
    mesh.insert_indices(Indices::U32(idx));
    Some(mesh)
}

pub fn update_far_grass(
    mut commands: Commands,
    session: Res<crate::Session>,
    q_player: Query<&Transform, (With<crate::PlayerTag>, Without<GrassTuft>)>,
    mut far: ResMut<FarGrass>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let Ok(pt) = q_player.get_single() else { return };
    let act = session.act;
    let pc = ((pt.translation.x / FAR_CHUNK).floor() as i64, (pt.translation.z / FAR_CHUNK).floor() as i64);
    if far.centre == Some((pc.0, pc.1, act)) {
        return;
    }
    let act_changed = far.centre.map_or(true, |c| c.2 != act);
    far.centre = Some((pc.0, pc.1, act));
    let mat = far
        .material
        .get_or_insert_with(|| {
            materials.add(grass_material())
        })
        .clone();
    let reach = (FAR_RADIUS / FAR_CHUNK).ceil() as i64;
    let mut want = std::collections::HashSet::new();
    for dz in -reach..=reach {
        for dx in -reach..=reach {
            if ((dx * dx + dz * dz) as f32).sqrt() * FAR_CHUNK <= FAR_RADIUS + FAR_CHUNK * 0.5 {
                want.insert((pc.0 + dx, pc.1 + dz));
            }
        }
    }
    let stale: Vec<(i64, i64)> = far
        .spawned
        .keys()
        .filter(|k| act_changed || !want.contains(k))
        .copied()
        .collect();
    for k in stale {
        if let Some(e) = far.spawned.remove(&k) {
            commands.entity(e).despawn();
        }
    }
    for (cx, cz) in want {
        if far.spawned.contains_key(&(cx, cz)) {
            continue;
        }
        let handle = match far.cache.get(&(act, cx, cz)) {
            Some(h) => h.clone(),
            None => {
                let Some(m) = far_chunk_mesh(act, cx, cz) else { continue };
                let h = meshes.add(m);
                far.cache.insert((act, cx, cz), h.clone());
                h
            }
        };
        let e = commands
            .spawn((
                Mesh3d(handle),
                MeshMaterial3d(mat.clone()),
                Transform::from_xyz(cx as f32 * FAR_CHUNK, 0.0, cz as f32 * FAR_CHUNK),
                FarGrassChunk,
            ))
            .id();
        far.spawned.insert((cx, cz), e);
    }
}

#[cfg(test)]
mod far_tests {
    use super::*;

    #[test]
    fn far_chunks_are_deterministic_and_nonempty_on_open_ground() {
        let act = antediluvia_protocol::Act::Eden;
        let mut any = 0;
        for (cx, cz) in [(3, 3), (-4, 2), (5, -6), (8, 8)] {
            let a = far_chunk_mesh(act, cx, cz);
            let b = far_chunk_mesh(act, cx, cz);
            assert_eq!(a.is_some(), b.is_some());
            if let (Some(a), Some(b)) = (a, b) {
                assert_eq!(a.count_vertices(), b.count_vertices());
                any += 1;
            }
        }
        assert!(any > 0, "no grass anywhere in open Eden meadow");
    }
}
