//! Visual variety at scale (alpha directive 2026-07-12):
//! - thousands of distinct terrain models: every scattered rock/crystal/spire
//!   is its own procedurally deformed mesh (seeded by position, deterministic
//!   across clients);
//! - hundreds of distinct mob models: each bestiary species gets a stable
//!   hue-shift + scale from its tag hash, layered over the base rigs;
//! - hundreds of character-creation choices: skin/hair indices tint the rig
//!   for real (4 bodies x 10 skins x 10 hairs = 400 combos on top of class).

use bevy::prelude::*;
use bevy::render::mesh::{Indices, PrimitiveTopology};
use bevy::render::render_asset::RenderAssetUsages;
use std::collections::HashMap;

/// Stable 0..1 hash (shared convention with main.rs `hash01`).
fn h01(seed: u64) -> f32 {
    let mut x = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15);
    x ^= x >> 33;
    x = x.wrapping_mul(0xC2B2_AE3D_27D4_EB4F);
    ((x >> 40) & 0xFFFFFF) as f32 / 16_777_215.0
}

fn hash_str(s: &str) -> u64 {
    s.bytes().fold(1469598103934665603u64, |h, b| (h ^ b as u64).wrapping_mul(1099511628211))
}

// ─── Rig tinting (mobs + player skin/hair) ───────────────────────────────────

/// Ask for this rig's meshes to be re-materialed with a hue shift once the
/// glTF scene has spawned. `hue` in degrees, `saturate`/`light` multipliers.
#[derive(Component)]
pub struct TintRig {
    pub hue: f32,
    pub light: f32,
    /// Extra hue for nodes that look like hair/head accents (players).
    pub hair_hue: Option<f32>,
}
#[derive(Component)]
pub struct TintApplied;

/// One tinted clone per (source material, quantized hue/light) — hundreds of
/// species share a small pool instead of allocating per entity.
#[derive(Resource, Default)]
pub struct TintCache(HashMap<(AssetId<StandardMaterial>, i32, i32), Handle<StandardMaterial>>);

/// Species tag → (hue shift, lightness, scale). Stable across clients and
/// sessions; ~24 hue bins x 5 light bins x continuous scale = hundreds of
/// distinct looks over the shared base rigs.
pub fn species_variation(tag: &str) -> (f32, f32, f32) {
    let h = hash_str(tag);
    let hue = (h01(h) * 360.0 / 15.0).floor() * 15.0;
    let light = 0.8 + h01(h ^ 0xBEEF) * 0.45;
    let scale = 0.8 + h01(h ^ 0xCAFE) * 0.55;
    (hue, light, scale)
}

/// Character-creation palettes (10 skins, 10 hairs — rendered, not just
/// stored). Index straight off the appearance ints.
pub fn skin_hue(idx: u32) -> (f32, f32) {
    // Warm earth tones through cool ash: hue shift + lightness.
    let table = [
        (0.0, 1.0), (12.0, 1.08), (25.0, 0.95), (40.0, 1.15), (330.0, 0.9),
        (300.0, 1.05), (200.0, 0.85), (160.0, 1.1), (80.0, 0.9), (55.0, 1.2),
    ];
    table[idx as usize % 10]
}
pub fn hair_hue(idx: u32) -> f32 {
    [0.0, 30.0, 60.0, 100.0, 140.0, 180.0, 220.0, 260.0, 300.0, 340.0][idx as usize % 10]
}

/// Walk freshly-loaded rigs and swap every mesh material for a hue-shifted
/// clone (cached). Runs until the scene exists, then marks the root done.
pub fn apply_tints(
    mut commands: Commands,
    mut cache: ResMut<TintCache>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    roots: Query<(Entity, &TintRig), Without<TintApplied>>,
    children_q: Query<&Children>,
    names: Query<&Name>,
    mats: Query<&MeshMaterial3d<StandardMaterial>>,
) {
    for (root, tint) in &roots {
        let mut touched = false;
        let mut stack = vec![(root, false)];
        while let Some((ent, hairish)) = stack.pop() {
            let hairish = hairish
                || names.get(ent).is_ok_and(|n| {
                    let n = n.as_str().to_ascii_lowercase();
                    n.contains("hair") || n.contains("head")
                });
            if let Ok(kids) = children_q.get(ent) {
                stack.extend(kids.iter().map(|k| (*k, hairish)));
            }
            let Ok(mat) = mats.get(ent) else { continue };
            let hue = match (hairish, tint.hair_hue) {
                (true, Some(hh)) => hh,
                _ => tint.hue,
            };
            let key = (mat.0.id(), hue as i32, (tint.light * 100.0) as i32);
            let handle = if let Some(h) = cache.0.get(&key) {
                h.clone()
            } else {
                let Some(src) = materials.get(mat.0.id()) else { continue };
                let mut m = src.clone();
                let hsla: Hsla = Hsla::from(m.base_color);
                m.base_color = Color::from(Hsla {
                    hue: (hsla.hue + hue).rem_euclid(360.0),
                    lightness: (hsla.lightness * tint.light).clamp(0.02, 0.98),
                    ..hsla
                });
                let h = materials.add(m);
                cache.0.insert(key, h.clone());
                h
            };
            commands.entity(ent).insert(MeshMaterial3d(handle));
            touched = true;
        }
        if touched {
            commands.entity(root).insert(TintApplied);
        }
    }
}

// ─── Procedural terrain models ───────────────────────────────────────────────

/// One-of-a-kind low-poly formation: a deformed sphere/spire/slab, seeded so
/// every scatter site in the world gets its own mesh. Three families x
/// continuous deformation = thousands of distinct models across the acts.
pub fn formation_mesh(seed: u64) -> Mesh {
    let family = (h01(seed ^ 0xF0F0) * 3.0) as u32;
    // Base shape: rings x segments dome.
    let rings = 5;
    let segs = 8;
    let mut positions: Vec<[f32; 3]> = Vec::new();
    let mut indices: Vec<u32> = Vec::new();
    let (rx, ry, rz) = match family {
        0 => (1.0 + h01(seed ^ 1) * 0.8, 0.7 + h01(seed ^ 2) * 0.5, 1.0 + h01(seed ^ 3) * 0.8), // boulder
        1 => (0.45 + h01(seed ^ 1) * 0.3, 1.6 + h01(seed ^ 2) * 1.4, 0.45 + h01(seed ^ 3) * 0.3), // spire
        _ => (1.4 + h01(seed ^ 1) * 1.0, 0.35 + h01(seed ^ 2) * 0.3, 1.0 + h01(seed ^ 3) * 0.7), // slab
    };
    for r in 0..=rings {
        let phi = std::f32::consts::PI * 0.5 * r as f32 / rings as f32;
        for s in 0..segs {
            let theta = std::f32::consts::TAU * s as f32 / segs as f32;
            // Per-vertex jitter is what makes each mesh unique.
            let j = 0.75 + h01(seed ^ (r as u64 * 31 + s as u64 + 7)) * 0.5;
            positions.push([
                rx * phi.sin() * theta.cos() * j,
                ry * phi.cos() * j,
                rz * phi.sin() * theta.sin() * j,
            ]);
        }
    }
    for r in 0..rings {
        for s in 0..segs {
            let a = (r * segs + s) as u32;
            let b = (r * segs + (s + 1) % segs) as u32;
            let c = a + segs as u32;
            let d = b + segs as u32;
            indices.extend_from_slice(&[a, b, c, b, d, c]);
        }
    }
    let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_indices(Indices::U32(indices));
    mesh.compute_smooth_normals();
    mesh
}

/// Formation tint per site: rock greys through act-flavored minerals.
pub fn formation_color(seed: u64) -> Color {
    let g = 0.28 + h01(seed ^ 0xAB) * 0.3;
    Color::srgb(
        g + h01(seed ^ 0xAC) * 0.18,
        g + h01(seed ^ 0xAD) * 0.12,
        g + h01(seed ^ 0xAE) * 0.16,
    )
}
