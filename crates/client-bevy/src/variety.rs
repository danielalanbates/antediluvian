//! Visual variety at scale (alpha directive 2026-07-12):
//! - thousands of distinct terrain models: every scattered rock/crystal/spire
//!   is its own procedurally deformed mesh (seeded by position, deterministic
//!   across clients);
//! - hundreds of distinct mob models: each bestiary species gets its own
//!   GEOMETRY — seeded body-plan stretch + unique grafted adornment meshes
//!   (see "Procedural creature models") — plus a stable hue/scale tint;
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

/// Character-creation palettes (Alpha-2: 16 skins, 12 hair colors, and each
/// hair color index also selects a grafted hairstyle GEOMETRY — see
/// `attach_hair_style`). 4 bodies x 16 skins x 12 hairs = 768 rendered combos.
pub const SKIN_CHOICES: u32 = 16;
pub const HAIR_CHOICES: u32 = 12;

pub fn skin_hue(idx: u32) -> (f32, f32) {
    // Warm earth tones through cool ash: hue shift + lightness.
    let table = [
        (0.0, 1.0), (12.0, 1.08), (25.0, 0.95), (40.0, 1.15), (330.0, 0.9),
        (300.0, 1.05), (200.0, 0.85), (160.0, 1.1), (80.0, 0.9), (55.0, 1.2),
        (18.0, 0.72), (32.0, 0.6), (8.0, 1.25), (210.0, 1.15), (275.0, 0.75),
        (120.0, 0.68),
    ];
    table[idx as usize % SKIN_CHOICES as usize]
}
pub fn hair_hue(idx: u32) -> f32 {
    [0.0, 30.0, 60.0, 100.0, 140.0, 180.0, 220.0, 260.0, 300.0, 340.0, 15.0, 45.0]
        [idx as usize % HAIR_CHOICES as usize]
}

// ─── Player hairstyles (Alpha-2 A3) ─────────────────────────────────────────
//
// The hair index doesn't just tint — it grafts one of 12 procedural hairstyle
// meshes onto the rig's head bone so choices read at a distance.

/// Ask for this player rig to grow its hairstyle once the scene has spawned.
#[derive(Component)]
pub struct HairStyle {
    pub style: u32,
    pub hue: f32,
}
#[derive(Component)]
pub struct HairApplied;

pub fn attach_hair_style(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    roots: Query<(Entity, &HairStyle), Without<HairApplied>>,
    children_q: Query<&Children>,
    names: Query<&Name>,
) {
    for (root, hs) in &roots {
        let mut head = None;
        let mut stack = vec![root];
        while let Some(ent) = stack.pop() {
            if let Ok(kids) = children_q.get(ent) {
                stack.extend(kids.iter().copied());
            }
            if let Ok(n) = names.get(ent) {
                let n = n.as_str().to_ascii_lowercase();
                if n.contains("head") || n.contains("neck") {
                    head = Some(ent);
                    break;
                }
            }
        }
        let Some(head) = head else { continue }; // scene not loaded yet
        let style = hs.style % HAIR_CHOICES;
        let mat = materials.add(StandardMaterial {
            base_color: Color::from(Hsla::new(hs.hue, 0.55, 0.32, 1.0)),
            perceptual_roughness: 0.9,
            ..default()
        });
        let seed = 0x4A1Fu64 ^ ((style as u64) << 8);
        // Each style is a distinct arrangement of scaled spike/crest meshes.
        let spawn = |commands: &mut Commands, m: Handle<Mesh>, t: Transform| {
            let mat = mat.clone();
            commands.entity(head).with_children(|p| {
                p.spawn((Mesh3d(m), MeshMaterial3d(mat), t));
            });
        };
        match style {
            0 => {} // shaved — tint only
            1 => {
                // Mohawk: 4 spikes down the crown midline.
                for i in 0..4u64 {
                    let m = meshes.add(spike_mesh(seed ^ i, 0.16, 0.05));
                    spawn(&mut commands, m, Transform::from_xyz(0.0, 0.16, 0.06 - i as f32 * 0.05));
                }
            }
            2 => {
                // Topknot: single fat spike up.
                let m = meshes.add(spike_mesh(seed, 0.2, 0.09));
                spawn(&mut commands, m, Transform::from_xyz(0.0, 0.17, 0.0));
            }
            3 => {
                // Long fall: flattened spike down the back.
                let m = meshes.add(spike_mesh(seed, 0.42, 0.11));
                spawn(&mut commands, m, Transform::from_xyz(0.0, 0.12, -0.07)
                    .with_rotation(Quat::from_rotation_x(2.6))
                    .with_scale(Vec3::new(1.3, 1.0, 0.5)));
            }
            4 => {
                // Twin tails.
                for side in [-1.0f32, 1.0] {
                    let m = meshes.add(spike_mesh(seed ^ (side as i64 as u64), 0.3, 0.07));
                    spawn(&mut commands, m, Transform::from_xyz(side * 0.09, 0.1, -0.05)
                        .with_rotation(Quat::from_rotation_x(2.4) * Quat::from_rotation_z(side * 0.3)));
                }
            }
            5 => {
                // Crown crest, front-to-back fin.
                let m = meshes.add(spike_mesh(seed, 0.24, 0.1));
                spawn(&mut commands, m, Transform::from_xyz(0.0, 0.15, 0.0)
                    .with_scale(Vec3::new(0.35, 1.0, 1.6)));
            }
            6 => {
                // Side sweep.
                let m = meshes.add(spike_mesh(seed, 0.26, 0.1));
                spawn(&mut commands, m, Transform::from_xyz(0.07, 0.14, 0.0)
                    .with_rotation(Quat::from_rotation_z(-0.9))
                    .with_scale(Vec3::new(1.0, 1.0, 1.4)));
            }
            7 => {
                // Bun + fringe.
                let m = meshes.add(spike_mesh(seed, 0.12, 0.08));
                spawn(&mut commands, m, Transform::from_xyz(0.0, 0.14, -0.08));
                let f = meshes.add(spike_mesh(seed ^ 9, 0.1, 0.09));
                spawn(&mut commands, f, Transform::from_xyz(0.0, 0.12, 0.09)
                    .with_rotation(Quat::from_rotation_x(-2.2)).with_scale(Vec3::new(1.5, 1.0, 0.6)));
            }
            8 => {
                // Wild mane: ring of 6 short spikes.
                for i in 0..6u64 {
                    let th = std::f32::consts::TAU * i as f32 / 6.0;
                    let m = meshes.add(spike_mesh(seed ^ i, 0.13, 0.05));
                    spawn(&mut commands, m, Transform::from_xyz(th.cos() * 0.08, 0.13, th.sin() * 0.08)
                        .with_rotation(Quat::from_rotation_z(-th.cos()) * Quat::from_rotation_x(th.sin())));
                }
            }
            9 => {
                // Braided tail: 3 segments down the back.
                for i in 0..3u64 {
                    let m = meshes.add(spike_mesh(seed ^ i, 0.16, 0.06 - i as f32 * 0.012));
                    spawn(&mut commands, m, Transform::from_xyz(0.0, 0.1 - i as f32 * 0.1, -0.08 - i as f32 * 0.05)
                        .with_rotation(Quat::from_rotation_x(2.7)));
                }
            }
            10 => {
                // Horned circlet: hair styled around two small nubs.
                for side in [-1.0f32, 1.0] {
                    let m = meshes.add(spike_mesh(seed, 0.09, 0.05));
                    spawn(&mut commands, m, Transform::from_xyz(side * 0.09, 0.15, 0.03)
                        .with_rotation(Quat::from_rotation_z(-side * 0.5)));
                }
                let c = meshes.add(spike_mesh(seed ^ 3, 0.14, 0.09));
                spawn(&mut commands, c, Transform::from_xyz(0.0, 0.15, -0.03));
            }
            _ => {
                // Pompadour: forward-leaning fat crest.
                let m = meshes.add(spike_mesh(seed, 0.2, 0.11));
                spawn(&mut commands, m, Transform::from_xyz(0.0, 0.15, 0.05)
                    .with_rotation(Quat::from_rotation_x(-0.5))
                    .with_scale(Vec3::new(0.9, 1.0, 1.2)));
            }
        }
        commands.entity(root).insert(HairApplied);
    }
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

// ─── Procedural creature models ──────────────────────────────────────────────
//
// Species-unique GEOMETRY, not just tint: every bestiary tag gets a seeded
// body-plan stretch plus 1–3 one-of-a-kind adornment meshes (horns, back
// ridge, crest, shoulder plates) grafted onto the rig's bones, so each
// species is a genuinely different model that still animates.

/// Body-plan families (Alpha-2 A2): each species hashes into one of 8 plans
/// with its own per-axis stretch band, so silhouettes differ at a glance —
/// serpentine stretch reads nothing like a brute or a stilt-walker.
/// Within a family the stretch is still continuous per species.
pub const BODY_PLANS: u32 = 8;

pub fn species_plan(tag: &str) -> u32 {
    (h01(hash_str(tag) ^ 0x9A9A) * BODY_PLANS as f32) as u32 % BODY_PLANS
}

/// Per-axis body-plan stretch for a species. Family sets the band; the
/// species hash picks the point inside it.
pub fn species_stretch(tag: &str) -> Vec3 {
    let h = hash_str(tag);
    let (bx, by, bz, sx, sy, sz) = match species_plan(tag) {
        0 => (0.85, 0.82, 0.85, 0.35, 0.42, 0.35), // baseline
        1 => (0.65, 0.55, 1.55, 0.2, 0.15, 0.55),  // serpentine: long + low
        2 => (0.7, 1.3, 0.7, 0.15, 0.35, 0.15),    // stilt: tall + slim
        3 => (1.3, 0.85, 1.2, 0.3, 0.15, 0.25),    // brute: broad
        4 => (1.15, 0.6, 1.15, 0.2, 0.12, 0.2),    // squat: wide + low
        5 => (0.6, 1.05, 0.9, 0.12, 0.25, 0.2),    // avian: narrow, upright
        6 => (0.95, 0.75, 1.35, 0.2, 0.15, 0.3),   // stalker: stretched torso
        _ => (1.05, 1.25, 1.05, 0.25, 0.35, 0.25), // giant: everything up
    };
    Vec3::new(
        bx + h01(h ^ 0x51) * sx,
        by + h01(h ^ 0x52) * sy,
        bz + h01(h ^ 0x53) * sz,
    )
}

/// Silhouette fingerprint: (plan, quantized stretch, part-selector bits).
/// Two species with the same key would look alike at a glance — the A2 test
/// asserts hundreds of distinct keys across the bestiary namespace.
pub fn silhouette_key(tag: &str) -> (u32, [i32; 3], u32) {
    let s = species_stretch(tag);
    let seed = species_parts_seed(tag);
    let bits = ((h01(seed ^ 0xA1) > 0.25) as u32)
        | ((h01(seed ^ 0xA6) > 0.5) as u32) << 1
        | ((h01(seed ^ 0xA9) > 0.4) as u32) << 2
        | ((h01(seed ^ 0xB7) > 0.45) as u32) << 3
        | ((h01(seed ^ 0xB1) > 0.6) as u32) << 4
        | ((h01(seed ^ 0xD5) > 0.55) as u32) << 5;
    (
        species_plan(tag),
        [(s.x * 20.0) as i32, (s.y * 20.0) as i32, (s.z * 20.0) as i32],
        bits,
    )
}

/// Ask for this rig to grow its species' adornment geometry once the glTF
/// scene has spawned (bones exist).
#[derive(Component)]
pub struct SpeciesParts {
    pub seed: u64,
}
#[derive(Component)]
pub struct PartsApplied;

pub fn species_parts_seed(tag: &str) -> u64 {
    hash_str(tag)
}

/// A unique jittered horn/spike cone. `len`/`girth` in bone-local units.
fn spike_mesh(seed: u64, len: f32, girth: f32) -> Mesh {
    let segs = 6;
    let rings = 3;
    let mut positions: Vec<[f32; 3]> = Vec::new();
    let mut indices: Vec<u32> = Vec::new();
    let bend = (h01(seed ^ 0x77) - 0.5) * 0.6;
    for r in 0..=rings {
        let t = r as f32 / rings as f32;
        let rad = girth * (1.0 - t * 0.92);
        for s in 0..segs {
            let th = std::f32::consts::TAU * s as f32 / segs as f32;
            let j = 0.8 + h01(seed ^ (r as u64 * 17 + s as u64 + 3)) * 0.4;
            positions.push([
                th.cos() * rad * j + bend * t * t * len,
                t * len * j.max(0.9),
                th.sin() * rad * j,
            ]);
        }
    }
    for r in 0..rings {
        for s in 0..segs {
            let a = (r * segs + s) as u32;
            let b = (r * segs + (s + 1) % segs) as u32;
            let (c, d) = (a + segs as u32, b + segs as u32);
            indices.extend_from_slice(&[a, b, c, b, d, c]);
        }
    }
    let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_indices(Indices::U32(indices));
    mesh.compute_smooth_normals();
    mesh
}

/// Graft the species' unique parts onto freshly-spawned rigs. Head nodes get
/// horns/crests, spine/torso nodes get a back ridge or shoulder plates.
/// Runs until bones exist, then marks the root done (same pattern as tints).
pub fn attach_species_parts(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    roots: Query<(Entity, &SpeciesParts), Without<PartsApplied>>,
    children_q: Query<&Children>,
    names: Query<&Name>,
) {
    for (root, parts) in &roots {
        let seed = parts.seed;
        let mut head = None;
        let mut spine = None;
        let mut tail = None;
        let mut stack = vec![root];
        while let Some(ent) = stack.pop() {
            if let Ok(kids) = children_q.get(ent) {
                stack.extend(kids.iter().copied());
            }
            if let Ok(n) = names.get(ent) {
                let n = n.as_str().to_ascii_lowercase();
                if head.is_none() && (n.contains("head") || n.contains("neck")) {
                    head = Some(ent);
                } else if spine.is_none()
                    && (n.contains("spine") || n.contains("torso") || n.contains("body") || n.contains("chest"))
                {
                    spine = Some(ent);
                } else if tail.is_none() && n.contains("tail") {
                    tail = Some(ent);
                }
            }
        }
        if head.is_none() && spine.is_none() {
            continue; // scene not loaded yet — retry next frame
        }
        let mat = materials.add(StandardMaterial {
            base_color: formation_color(seed ^ 0xD00D),
            perceptual_roughness: 0.85,
            ..default()
        });
        // 1–3 part groups chosen by the seed: horns, crest, back ridge, plates.
        if let Some(head) = head {
            if h01(seed ^ 0xA1) > 0.25 {
                let horns = 1 + (h01(seed ^ 0xA2) * 2.0) as u32; // 1–2 pairs
                for i in 0..horns {
                    let m = meshes.add(spike_mesh(seed ^ (0xB00 + i as u64), 0.28 + h01(seed ^ (0xA3 + i as u64)) * 0.5, 0.05 + h01(seed ^ (0xA4 + i as u64)) * 0.06));
                    let x = 0.09 + i as f32 * 0.07;
                    let tilt = (h01(seed ^ (0xA5 + i as u64)) - 0.3) * 1.2;
                    for side in [-1.0f32, 1.0] {
                        commands.entity(head).with_children(|p| {
                            p.spawn((
                                Mesh3d(m.clone()),
                                MeshMaterial3d(mat.clone()),
                                Transform::from_xyz(side * x, 0.12, 0.0)
                                    .with_rotation(Quat::from_rotation_z(-side * tilt)),
                            ));
                        });
                    }
                }
            } else {
                // Crest fin: a single flattened spike on the crown.
                let m = meshes.add(spike_mesh(seed ^ 0xC0FE, 0.45, 0.1));
                commands.entity(head).with_children(|p| {
                    p.spawn((
                        Mesh3d(m),
                        MeshMaterial3d(mat.clone()),
                        Transform::from_xyz(0.0, 0.14, 0.0).with_scale(Vec3::new(0.35, 1.0, 1.3)),
                    ));
                });
            }
            // Tusks (A2): down-curved pair jutting forward from the jaw.
            if h01(seed ^ 0xB1) > 0.6 {
                let m = meshes.add(spike_mesh(seed ^ 0x705C, 0.22 + h01(seed ^ 0xB2) * 0.2, 0.05));
                for side in [-1.0f32, 1.0] {
                    commands.entity(head).with_children(|p| {
                        p.spawn((
                            Mesh3d(m.clone()),
                            MeshMaterial3d(mat.clone()),
                            Transform::from_xyz(side * 0.07, -0.04, 0.12)
                                .with_rotation(Quat::from_rotation_x(-1.9) * Quat::from_rotation_z(side * 0.35)),
                        ));
                    });
                }
            }
        }
        if let Some(spine) = spine {
            if h01(seed ^ 0xA6) > 0.5 {
                // Back ridge: 3–5 unique spikes down the spine.
                let n = 3 + (h01(seed ^ 0xA7) * 3.0) as u32;
                for i in 0..n {
                    let m = meshes.add(spike_mesh(seed ^ (0xE00 + i as u64), 0.16 + h01(seed ^ (0xA8 + i as u64)) * 0.22, 0.05));
                    commands.entity(spine).with_children(|p| {
                        p.spawn((
                            Mesh3d(m),
                            MeshMaterial3d(mat.clone()),
                            Transform::from_xyz(0.0, 0.08 + i as f32 * 0.09, -0.09),
                        ));
                    });
                }
            } else if h01(seed ^ 0xD5) > 0.55 {
                // Dorsal sail (A2): tall thin fin along the spine.
                let m = meshes.add(spike_mesh(seed ^ 0x5A11, 0.35 + h01(seed ^ 0xD6) * 0.3, 0.14));
                commands.entity(spine).with_children(|p| {
                    p.spawn((
                        Mesh3d(m),
                        MeshMaterial3d(mat.clone()),
                        Transform::from_xyz(0.0, 0.14, -0.06).with_scale(Vec3::new(0.2, 1.0, 2.1)),
                    ));
                });
            } else if h01(seed ^ 0xA9) > 0.4 {
                // Shoulder plates.
                let m = meshes.add(spike_mesh(seed ^ 0xF1A7, 0.2, 0.12));
                for side in [-1.0f32, 1.0] {
                    commands.entity(spine).with_children(|p| {
                        p.spawn((
                            Mesh3d(m.clone()),
                            MeshMaterial3d(mat.clone()),
                            Transform::from_xyz(side * 0.16, 0.2, 0.0)
                                .with_rotation(Quat::from_rotation_z(-side * 1.1))
                                .with_scale(Vec3::new(1.0, 0.7, 1.6)),
                        ));
                    });
                }
            }
        }
        // Tail spikes (A2): 2–3 barbs on rigs that expose a tail bone.
        if let Some(tail) = tail {
            if h01(seed ^ 0xB7) > 0.45 {
                let n = 2 + (h01(seed ^ 0xB8) * 2.0) as u32;
                for i in 0..n {
                    let m = meshes.add(spike_mesh(seed ^ (0x7A11 + i as u64), 0.12 + h01(seed ^ (0xB9 + i as u64)) * 0.14, 0.04));
                    commands.entity(tail).with_children(|p| {
                        p.spawn((
                            Mesh3d(m),
                            MeshMaterial3d(mat.clone()),
                            Transform::from_xyz(0.0, 0.05, -0.06 - i as f32 * 0.08)
                                .with_rotation(Quat::from_rotation_x(0.7)),
                        ));
                    });
                }
            }
        }
        commands.entity(root).insert(PartsApplied);
    }
}

// ─── Procedural terrain models ───────────────────────────────────────────────

/// One-of-a-kind low-poly formation, seeded so every scatter site in the
/// world gets its own mesh. Alpha-2 A1: EIGHT families x continuous per-vertex
/// deformation — boulder, spire, slab, mesa, hoodoo stack, arch, shard
/// cluster, terrace — thousands of distinct models across the acts.
pub const FORMATION_FAMILIES: u32 = 8;

pub fn formation_family(seed: u64) -> u32 {
    (h01(seed ^ 0xF0F0) * FORMATION_FAMILIES as f32) as u32 % FORMATION_FAMILIES
}

pub fn formation_mesh(seed: u64) -> Mesh {
    let family = formation_family(seed);
    // Base shape: rings x segments dome, warped per family.
    let rings = 5;
    let segs = 8;
    let mut positions: Vec<[f32; 3]> = Vec::new();
    let mut indices: Vec<u32> = Vec::new();
    let (rx, ry, rz) = match family {
        0 => (1.0 + h01(seed ^ 1) * 0.8, 0.7 + h01(seed ^ 2) * 0.5, 1.0 + h01(seed ^ 3) * 0.8), // boulder
        1 => (0.45 + h01(seed ^ 1) * 0.3, 1.6 + h01(seed ^ 2) * 1.4, 0.45 + h01(seed ^ 3) * 0.3), // spire
        2 => (1.4 + h01(seed ^ 1) * 1.0, 0.35 + h01(seed ^ 2) * 0.3, 1.0 + h01(seed ^ 3) * 0.7), // slab
        3 => (1.2 + h01(seed ^ 1) * 0.7, 0.9 + h01(seed ^ 2) * 0.4, 1.2 + h01(seed ^ 3) * 0.7), // mesa
        4 => (0.6 + h01(seed ^ 1) * 0.25, 1.4 + h01(seed ^ 2) * 1.0, 0.6 + h01(seed ^ 3) * 0.25), // hoodoo
        5 => (1.3 + h01(seed ^ 1) * 0.6, 1.1 + h01(seed ^ 2) * 0.6, 0.5 + h01(seed ^ 3) * 0.2), // arch
        6 => (0.8 + h01(seed ^ 1) * 0.5, 1.2 + h01(seed ^ 2) * 0.9, 0.8 + h01(seed ^ 3) * 0.5), // shards
        _ => (1.3 + h01(seed ^ 1) * 0.8, 0.7 + h01(seed ^ 2) * 0.35, 1.1 + h01(seed ^ 3) * 0.6), // terrace
    };
    for r in 0..=rings {
        let phi = std::f32::consts::PI * 0.5 * r as f32 / rings as f32;
        let t = r as f32 / rings as f32;
        // Family-specific vertical profile warps.
        let (prof, twist) = match family {
            3 => (if t < 0.35 { 1.0 } else { 0.55 }, 0.0),               // mesa: flat cap, pinched base
            4 => (0.65 + 0.5 * ((t * 9.0).sin().abs()), 0.0),           // hoodoo: bulge stack
            5 => (1.0, 0.0),                                             // arch handled per-vertex below
            6 => (1.0 - t * 0.35, h01(seed ^ 0xF1) * 1.4),               // shards: twisted taper
            7 => (1.0 - (t * 4.0).floor() * 0.18, 0.0),                  // terrace: stepped shelves
            _ => (1.0, 0.0),
        };
        for s in 0..segs {
            let theta = std::f32::consts::TAU * s as f32 / segs as f32 + twist * t;
            // Per-vertex jitter is what makes each mesh unique.
            let j = 0.75 + h01(seed ^ (r as u64 * 31 + s as u64 + 7)) * 0.5;
            let mut x = rx * phi.sin() * theta.cos() * j * prof;
            let y = ry * phi.cos() * j;
            let z = rz * phi.sin() * theta.sin() * j * prof;
            if family == 5 {
                // Arch: push the crown outward into two legs by hollowing
                // the middle — vertices near the axis get shoved sideways.
                if x.abs() < rx * 0.45 && t > 0.3 {
                    x += x.signum().max(1.0) * rx * 0.5 * (1.0 - t);
                }
            }
            positions.push([x, y, z]);
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

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::render::mesh::VertexAttributeValues;

    fn positions(m: &Mesh) -> Vec<[f32; 3]> {
        match m.attribute(Mesh::ATTRIBUTE_POSITION).unwrap() {
            VertexAttributeValues::Float32x3(v) => v.clone(),
            _ => panic!("positions"),
        }
    }

    /// Every seed yields valid, geometry-distinct meshes — the "hundreds of
    /// different mob models" claim rests on this.
    #[test]
    fn species_geometry_is_unique_per_seed() {
        let tags = ["cainite_raider", "nephilim_goliath", "abyssal_aurochs", "eden_wolf"];
        let mut all: Vec<Vec<[f32; 3]>> = Vec::new();
        for t in tags {
            let s = species_parts_seed(t);
            let m = spike_mesh(s, 0.3, 0.06);
            let p = positions(&m);
            assert!(p.len() >= 12, "{t}: degenerate mesh");
            assert!(all.iter().all(|q| *q != p), "{t}: duplicate geometry");
            all.push(p);
            let st = species_stretch(t);
            // Alpha-2 body plans widen the band deliberately (serpentine z
            // reaches ~2.1); keep a sanity ceiling so shear stays invisible.
            assert!(st.min_element() > 0.4 && st.max_element() < 2.2, "{t}: stretch out of band");
        }
        // Stretch must differ across species too.
        assert_ne!(species_stretch("eden_wolf"), species_stretch("abyssal_aurochs"));
    }

    #[test]
    fn formation_meshes_unique_per_seed() {
        let a = positions(&formation_mesh(1));
        let b = positions(&formation_mesh(2));
        assert_ne!(a, b);
    }

    /// A1: all eight formation families actually occur, and a world-sized
    /// sample of seeds yields all-distinct geometry (thousands of models).
    #[test]
    fn formation_families_all_reachable_and_distinct() {
        let mut fams = std::collections::HashSet::new();
        let mut geoms = std::collections::HashSet::new();
        for i in 0..2000u64 {
            let seed = i * 7919 + 13;
            fams.insert(formation_family(seed));
            let p = positions(&formation_mesh(seed));
            assert!(p.len() >= 24, "seed {seed}: degenerate mesh");
            let key: Vec<[i64; 3]> = p.iter()
                .map(|v| [(v[0] * 1e4) as i64, (v[1] * 1e4) as i64, (v[2] * 1e4) as i64])
                .collect();
            assert!(geoms.insert(key), "seed {seed}: duplicate formation geometry");
        }
        assert_eq!(fams.len(), FORMATION_FAMILIES as usize, "unreachable formation family");
    }

    /// A2: across a bestiary-sized namespace of species tags, silhouettes
    /// (body plan + stretch + part loadout) stay distinct — the "hundreds of
    /// different mob models" claim rests on this.
    #[test]
    fn species_silhouettes_hundreds_distinct() {
        let bases = [
            "cainite", "nephilim", "abyssal", "eden", "marsh", "dune", "vale",
            "ridge", "hollow", "grove", "reef", "peak", "shade", "storm",
            "ember", "frost", "iron", "gilded", "wild", "elder",
        ];
        let kinds = [
            "raider", "goliath", "aurochs", "wolf", "serpent", "stalker",
            "brute", "harrier", "lurker", "matron", "whelp", "tyrant",
            "warden", "husk", "render", "sentinel", "hound", "drake",
            "gorger", "shaman",
        ];
        let mut plans = std::collections::HashSet::new();
        let mut keys = std::collections::HashSet::new();
        let mut tags = 0u32;
        for b in bases {
            for k in kinds {
                let tag = format!("{b}_{k}");
                plans.insert(species_plan(&tag));
                keys.insert(silhouette_key(&tag));
                tags += 1;
            }
        }
        assert_eq!(plans.len(), BODY_PLANS as usize, "unreachable body plan");
        assert!(
            keys.len() >= 300,
            "only {} distinct silhouettes across {} tags",
            keys.len(),
            tags
        );
    }

    /// A3: the rendered character-creation space is hundreds of combos wide,
    /// and every palette index is distinct.
    #[test]
    fn character_creation_hundreds_of_choices() {
        let combos = 4 * SKIN_CHOICES * HAIR_CHOICES; // bodies x skins x hairs
        assert!(combos >= 700, "only {combos} combos");
        let skins: std::collections::HashSet<_> =
            (0..SKIN_CHOICES).map(|i| { let (h, l) = skin_hue(i); ((h * 10.0) as i32, (l * 100.0) as i32) }).collect();
        assert_eq!(skins.len(), SKIN_CHOICES as usize, "duplicate skin tones");
        let hairs: std::collections::HashSet<_> =
            (0..HAIR_CHOICES).map(|i| (hair_hue(i) * 10.0) as i32).collect();
        assert_eq!(hairs.len(), HAIR_CHOICES as usize, "duplicate hair colors");
    }
}
