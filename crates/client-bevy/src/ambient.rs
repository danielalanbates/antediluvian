//! Ambient life pipeline (milestone 1) — client-side critters and particles.
//!
//! The world reads empty because nothing in it *moves* on its own: every mob
//! comes from the server at 10 Hz and the decor never stirs. This module adds
//! the cheap half of "alive" — butterflies, birds, rabbits, fireflies, dust,
//! leaves and embers — as pure presentation the server never hears about.
//!
//! Design (see `life-pipeline-design.md`):
//! - Deterministic per `(act, cell, slot)` seed: every client renders the
//!   same butterfly, and a screenshot run reproduces it exactly.
//! - Fixed pools re-seated on cell crossing, like `grass.rs`'s tuft ring —
//!   zero per-frame allocation, O(pool) transform updates.
//! - One procedural mesh + one shared material per critter kind; vertex
//!   colours carry the hue. A butterfly is 3 quads; 96 critters cost less
//!   than one character.
//! - Animation is a pure function of `(seed, anchor, time)` — no stored
//!   state, so critters never desync and never need saving.

use crate::propgen::h01;
use crate::terrain::{road_dist, terrain_height, water_level};
use antediluvia_protocol::Act;
use bevy::prelude::*;
use bevy::render::mesh::{Indices, PrimitiveTopology};
use bevy::render::render_asset::RenderAssetUsages;

// ─── Data: what lives in each act ────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CritterKind {
    Butterfly,
    Bird,
    Rabbit,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ParticleKind {
    /// Night-only drifting blink (Eden, Enoch).
    Firefly,
    /// Daytime bright-air motes, everywhere.
    Dust,
    /// Falling, wind-blown (Eden, Hermon).
    Leaf,
    /// Rising cinders (Nephilim, Flood).
    Ember,
}

/// (kind, weight) — a new act is a row here, not a code change.
pub fn act_critters(act: Act) -> &'static [(CritterKind, f32)] {
    match act {
        Act::Eden => &[
            (CritterKind::Butterfly, 0.55),
            (CritterKind::Bird, 0.30),
            (CritterKind::Rabbit, 0.15),
        ],
        Act::Hermon => &[
            (CritterKind::Bird, 0.55),
            (CritterKind::Rabbit, 0.30),
            (CritterKind::Butterfly, 0.15),
        ],
        Act::Nephilim => &[(CritterKind::Bird, 0.80), (CritterKind::Rabbit, 0.20)],
        Act::Enoch => &[(CritterKind::Bird, 0.60), (CritterKind::Butterfly, 0.40)],
        Act::Flood => &[(CritterKind::Bird, 1.0)],
    }
}

pub fn act_particles(act: Act) -> &'static [(ParticleKind, f32)] {
    match act {
        Act::Eden => &[
            (ParticleKind::Firefly, 0.35),
            (ParticleKind::Dust, 0.35),
            (ParticleKind::Leaf, 0.30),
        ],
        Act::Hermon => &[(ParticleKind::Dust, 0.45), (ParticleKind::Leaf, 0.55)],
        Act::Nephilim => &[(ParticleKind::Ember, 0.70), (ParticleKind::Dust, 0.30)],
        Act::Enoch => &[(ParticleKind::Firefly, 0.40), (ParticleKind::Dust, 0.60)],
        Act::Flood => &[(ParticleKind::Ember, 0.40), (ParticleKind::Dust, 0.60)],
    }
}

// ─── Procedural critter meshes ───────────────────────────────────────────────
// Vertex colours under one shared material; faceted flat normals like
// propgen. Sizes are world units (a character is ~55 tall).

fn push_quad(
    pos: &mut Vec<[f32; 3]>,
    nor: &mut Vec<[f32; 3]>,
    col: &mut Vec<[f32; 4]>,
    idx: &mut Vec<u32>,
    a: [f32; 3],
    b: [f32; 3],
    c: [f32; 3],
    d: [f32; 3],
    n: [f32; 3],
    color: [f32; 4],
) {
    let base = pos.len() as u32;
    pos.extend_from_slice(&[a, b, c, d]);
    nor.extend_from_slice(&[n, n, n, n]);
    col.extend_from_slice(&[color, color, color, color]);
    idx.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
}

fn push_box(
    pos: &mut Vec<[f32; 3]>,
    nor: &mut Vec<[f32; 3]>,
    col: &mut Vec<[f32; 4]>,
    idx: &mut Vec<u32>,
    c: [f32; 3],
    h: [f32; 3],
    color: [f32; 4],
) {
    // 6 flat-shaded quads; `h` is half-extents, scaled per axis.
    let ax = |v: [f32; 3]| [v[0] * h[0], v[1] * h[1], v[2] * h[2]];
    const FACES: [([f32; 3], [f32; 3], [f32; 3]); 6] = [
        ([0., 0., 1.], [1., 0., 0.], [0., 1., 0.]),
        ([0., 0., -1.], [-1., 0., 0.], [0., 1., 0.]),
        ([1., 0., 0.], [0., 0., -1.], [0., 1., 0.]),
        ([-1., 0., 0.], [0., 0., 1.], [0., 1., 0.]),
        ([0., 1., 0.], [1., 0., 0.], [0., 0., -1.]),
        ([0., -1., 0.], [1., 0., 0.], [0., 0., 1.]),
    ];
    for (n, u, v) in FACES {
        let (dn, du, dv) = (ax(n), ax(u), ax(v));
        let q = |su: f32, sv: f32| {
            [
                c[0] + dn[0] + du[0] * su + dv[0] * sv,
                c[1] + dn[1] + du[1] * su + dv[1] * sv,
                c[2] + dn[2] + du[2] * su + dv[2] * sv,
            ]
        };
        push_quad(
            pos,
            nor,
            col,
            idx,
            q(-1., -1.),
            q(1., -1.),
            q(1., 1.),
            q(-1., 1.),
            n,
            color,
        );
    }
}

fn finish_mesh(pos: Vec<[f32; 3]>, nor: Vec<[f32; 3]>, col: Vec<[f32; 4]>, idx: Vec<u32>) -> Mesh {
    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, pos);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, nor);
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, col);
    mesh.insert_indices(Indices::U32(idx));
    mesh
}

/// Two wing quads in a shallow V + a body sliver. Flap is faked in the
/// animate system by pulsing scale.x — at 60+ m the wings are 2 px anyway.
pub fn butterfly_mesh() -> Mesh {
    let (mut pos, mut nor, mut col, mut idx) = (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    let wing = [0.95, 0.72, 0.30, 1.0]; // warm amber
    let wing2 = [0.98, 0.88, 0.62, 1.0]; // pale tip
    let up = [0.0, 1.0, 0.0];
    // Left wing: body (x=0) out to x=-4, slight dihedral.
    push_quad(
        &mut pos,
        &mut nor,
        &mut col,
        &mut idx,
        [0.0, 0.6, -1.6],
        [-4.2, 1.8, -2.2],
        [-4.2, 1.8, 2.2],
        [0.0, 0.6, 1.6],
        up,
        wing,
    );
    // Right wing mirrored.
    push_quad(
        &mut pos,
        &mut nor,
        &mut col,
        &mut idx,
        [0.0, 0.6, 1.6],
        [4.2, 1.8, 2.2],
        [4.2, 1.8, -2.2],
        [0.0, 0.6, -1.6],
        up,
        wing2,
    );
    // Body sliver.
    push_box(
        &mut pos,
        &mut nor,
        &mut col,
        &mut idx,
        [0.0, 0.0, 0.0],
        [0.5, 0.5, 2.2],
        [0.25, 0.18, 0.12, 1.0],
    );
    finish_mesh(pos, nor, col, idx)
}

/// Gull silhouette: body sliver + two long wings in a shallow V + tail fan.
pub fn bird_mesh() -> Mesh {
    let (mut pos, mut nor, mut col, mut idx) = (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    let dark = [0.16, 0.15, 0.17, 1.0];
    let up = [0.0, 1.0, 0.0];
    push_quad(
        &mut pos,
        &mut nor,
        &mut col,
        &mut idx,
        [0.0, 0.8, -1.2],
        [-7.5, 2.6, -2.6],
        [-7.5, 2.6, 0.6],
        [0.0, 0.8, 1.0],
        up,
        dark,
    );
    push_quad(
        &mut pos,
        &mut nor,
        &mut col,
        &mut idx,
        [0.0, 0.8, 1.0],
        [7.5, 2.6, 0.6],
        [7.5, 2.6, -2.6],
        [0.0, 0.8, -1.2],
        up,
        dark,
    );
    push_box(
        &mut pos,
        &mut nor,
        &mut col,
        &mut idx,
        [0.0, 0.0, 0.0],
        [0.9, 0.9, 3.4],
        [0.22, 0.20, 0.22, 1.0],
    );
    // Tail fan.
    push_quad(
        &mut pos,
        &mut nor,
        &mut col,
        &mut idx,
        [-1.4, 0.2, -3.2],
        [-2.6, 0.4, -5.4],
        [2.6, 0.4, -5.4],
        [1.4, 0.2, -3.2],
        up,
        dark,
    );
    finish_mesh(pos, nor, col, idx)
}

/// Boxes like the crowd-LOD humanoid: body, head, two ears, haunch.
pub fn rabbit_mesh() -> Mesh {
    let (mut pos, mut nor, mut col, mut idx) = (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    let fur = [0.52, 0.44, 0.34, 1.0];
    let dark = [0.40, 0.33, 0.25, 1.0];
    push_box(
        &mut pos,
        &mut nor,
        &mut col,
        &mut idx,
        [0.0, 4.5, 0.0],
        [3.2, 3.4, 5.2],
        fur,
    );
    push_box(
        &mut pos,
        &mut nor,
        &mut col,
        &mut idx,
        [0.0, 8.6, 4.6],
        [2.2, 2.2, 2.4],
        fur,
    );
    push_box(
        &mut pos,
        &mut nor,
        &mut col,
        &mut idx,
        [-1.1, 12.0, 4.2],
        [0.7, 2.6, 0.9],
        dark,
    );
    push_box(
        &mut pos,
        &mut nor,
        &mut col,
        &mut idx,
        [1.1, 12.0, 4.2],
        [0.7, 2.6, 0.9],
        dark,
    );
    push_box(
        &mut pos,
        &mut nor,
        &mut col,
        &mut idx,
        [0.0, 3.0, -4.6],
        [2.6, 2.4, 2.2],
        dark,
    );
    finish_mesh(pos, nor, col, idx)
}

fn critter_mesh(kind: CritterKind) -> Mesh {
    match kind {
        CritterKind::Butterfly => butterfly_mesh(),
        CritterKind::Bird => bird_mesh(),
        CritterKind::Rabbit => rabbit_mesh(),
    }
}

/// Shared critter material: vertex colours carry the hue, like grass and
/// the propgen set, so every critter kind batches under one material.
fn critter_material() -> StandardMaterial {
    StandardMaterial {
        base_color: Color::WHITE,
        perceptual_roughness: 0.9,
        cull_mode: None,
        double_sided: true,
        ..default()
    }
}

fn particle_material(kind: ParticleKind) -> StandardMaterial {
    let (c, blend) = match kind {
        ParticleKind::Firefly => (Color::srgba(1.0, 0.82, 0.30, 0.95), AlphaMode::Add),
        ParticleKind::Ember => (Color::srgba(1.0, 0.42, 0.12, 0.9), AlphaMode::Add),
        ParticleKind::Dust => (Color::srgba(1.0, 1.0, 0.95, 0.28), AlphaMode::Blend),
        ParticleKind::Leaf => (Color::srgba(0.45, 0.62, 0.25, 0.9), AlphaMode::Blend),
    };
    StandardMaterial {
        base_color: c,
        unlit: true,
        alpha_mode: blend,
        cull_mode: None,
        double_sided: true,
        ..default()
    }
}

// ─── Pools ───────────────────────────────────────────────────────────────────

/// Cell edge for re-seating; critter ring radius around the player.
const CELL: f32 = 64.0;
const CRITTER_RADIUS: f32 = 280.0;
const CRITTER_COUNT: usize = 96;
const PARTICLE_COUNT: usize = 256;

#[derive(Component)]
pub struct AmbientCritter {
    slot: usize,
}

#[derive(Component)]
pub struct AmbientParticle {
    slot: usize,
}

/// Pure placement data for one pool slot — computed from the seed alone so
/// tests can assert determinism without a Bevy world.
#[derive(Debug, PartialEq)]
struct CritterSlot {
    kind: CritterKind,
    anchor: Vec3,
    phase: f32,
    hidden: bool,
}

#[derive(Debug, PartialEq)]
struct ParticleSlot {
    kind: ParticleKind,
    anchor: Vec3,
    phase: f32,
}

fn pick_weighted<T: Copy>(table: &[(T, f32)], r: f32) -> T {
    let total: f32 = table.iter().map(|(_, w)| w).sum();
    let mut acc = r * total;
    for (k, w) in table {
        acc -= w;
        if acc <= 0.0 {
            return *k;
        }
    }
    table.last().unwrap().0
}

/// Deterministic slot layout: same (act, cell, slot) → same critter.
fn critter_slot(slot: usize, act: Act, pcx: f32, pcz: f32) -> CritterSlot {
    let seed = (slot as u64)
        .wrapping_mul(0x9E37_79B9_7F4A_7C15)
        .wrapping_add((act as u64).wrapping_mul(0xC2B2_AE3D_27D4_EB4F));
    let kind = pick_weighted(act_critters(act), h01(seed ^ 0xC211));
    // Anchor in a ring around the player; butterflies stay close (they're
    // tiny), birds range wide.
    let reach = match kind {
        CritterKind::Butterfly => 0.45,
        CritterKind::Rabbit => 0.7,
        CritterKind::Bird => 1.0,
    } * CRITTER_RADIUS;
    let a = h01(seed ^ 0xB12D) * std::f32::consts::TAU;
    let r = (0.25 + 0.75 * h01(seed ^ 0xB22D)) * reach;
    let x = pcx + a.cos() * r;
    let z = pcz + a.sin() * r;
    let y = terrain_height(act, x, z);
    let on_road = road_dist(x, z) <= 40.0;
    let under_water = water_level(act).map(|w| y < w + 1.0).unwrap_or(false);
    // Birds don't care about roads or water; ground critters do.
    let hidden = on_road || (under_water && kind != CritterKind::Bird);
    let lift = match kind {
        CritterKind::Butterfly => 6.0 + h01(seed ^ 0xB32D) * 10.0,
        CritterKind::Bird => 45.0 + h01(seed ^ 0xB32D) * 60.0,
        CritterKind::Rabbit => 0.0,
    };
    CritterSlot {
        kind,
        anchor: Vec3::new(x, y + lift, z),
        phase: h01(seed ^ 0xB42D) * std::f32::consts::TAU,
        hidden,
    }
}

fn particle_slot(slot: usize, act: Act, pcx: f32, pcz: f32) -> ParticleSlot {
    let seed = (slot as u64)
        .wrapping_mul(0x51ED_7B8D_9C3A_1F05)
        .wrapping_add((act as u64).wrapping_mul(0xA1B2_C3D4));
    let kind = pick_weighted(act_particles(act), h01(seed ^ 0x9A17));
    let a = h01(seed ^ 0xD12D) * std::f32::consts::TAU;
    let r = (0.15 + 0.85 * h01(seed ^ 0xD22D)) * CRITTER_RADIUS;
    let x = pcx + a.cos() * r;
    let z = pcz + a.sin() * r;
    let y = terrain_height(act, x, z);
    let lift = match kind {
        ParticleKind::Firefly => 3.0 + h01(seed ^ 0xD32D) * 8.0,
        ParticleKind::Dust => 4.0 + h01(seed ^ 0xD32D) * 25.0,
        ParticleKind::Leaf => 10.0 + h01(seed ^ 0xD32D) * 20.0,
        ParticleKind::Ember => 2.0 + h01(seed ^ 0xD32D) * 6.0,
    };
    ParticleSlot {
        kind,
        anchor: Vec3::new(x, y + lift, z),
        phase: h01(seed ^ 0xD42D) * std::f32::consts::TAU,
    }
}

#[derive(Resource)]
pub struct AmbientState {
    cell: (i64, i64),
    act: Act,
    /// Particle pool tracks its own seating epoch: the two update systems
    /// run in ambiguous order, so they must not share one flag.
    pcell: (i64, i64),
    pact: Act,
}

#[derive(Resource)]
pub struct AmbientMeshes {
    critters: [Handle<Mesh>; 3],
    particle_mats: [Handle<StandardMaterial>; 4],
}

fn kind_index(k: CritterKind) -> usize {
    match k {
        CritterKind::Butterfly => 0,
        CritterKind::Bird => 1,
        CritterKind::Rabbit => 2,
    }
}

fn particle_index(k: ParticleKind) -> usize {
    match k {
        ParticleKind::Firefly => 0,
        ParticleKind::Dust => 1,
        ParticleKind::Leaf => 2,
        ParticleKind::Ember => 3,
    }
}

pub fn init_ambient(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let critter_meshes = [
        meshes.add(critter_mesh(CritterKind::Butterfly)),
        meshes.add(critter_mesh(CritterKind::Bird)),
        meshes.add(critter_mesh(CritterKind::Rabbit)),
    ];
    let particle_mesh = meshes.add(Sphere::new(1.6).mesh().uv(8, 6));
    let critter_mat = materials.add(critter_material());
    let particle_mats = [
        materials.add(particle_material(ParticleKind::Firefly)),
        materials.add(particle_material(ParticleKind::Dust)),
        materials.add(particle_material(ParticleKind::Leaf)),
        materials.add(particle_material(ParticleKind::Ember)),
    ];
    // Spawn the pools hidden below the world; `update_*` seats them.
    // Mesh/material are assigned on seating (kinds change per act).
    for slot in 0..CRITTER_COUNT {
        commands.spawn((
            Mesh3d(critter_meshes[0].clone()),
            MeshMaterial3d(critter_mat.clone()),
            Transform::from_xyz(0.0, -10_000.0, 0.0),
            AmbientCritter { slot },
        ));
    }
    for slot in 0..PARTICLE_COUNT {
        commands.spawn((
            Mesh3d(particle_mesh.clone()),
            MeshMaterial3d(particle_mats[1].clone()),
            Transform::from_xyz(0.0, -10_000.0, 0.0),
            AmbientParticle { slot },
        ));
    }
    commands.insert_resource(AmbientMeshes {
        critters: critter_meshes,
        particle_mats,
    });
    commands.insert_resource(AmbientState {
        cell: (i64::MIN, i64::MIN),
        act: Act::Eden,
        pcell: (i64::MIN, i64::MIN),
        pact: Act::Eden,
    });
}

/// Re-seat the critter pool when the player crosses a cell or changes act.
pub fn update_ambient_critters(
    mut commands: Commands,
    session: Res<crate::Session>,
    q_player: Query<&Transform, (With<crate::PlayerTag>, Without<AmbientCritter>)>,
    mut state: ResMut<AmbientState>,
    mut critters: Query<
        (Entity, &AmbientCritter, &mut Transform, &mut Mesh3d),
        Without<crate::PlayerTag>,
    >,
    assets: Res<AmbientMeshes>,
) {
    let Ok(pt) = q_player.get_single() else {
        return;
    };
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
    let (pcx, pcz) = (pt.translation.x, pt.translation.z);
    for (e, c, mut t, mut m) in &mut critters {
        let s = critter_slot(c.slot, act, pcx, pcz);
        m.0 = assets.critters[kind_index(s.kind)].clone();
        t.translation = s.anchor;
        t.scale = Vec3::splat(if s.hidden { 0.0 } else { 1.0 });
        // The animator recomputes the rest from (slot, act, time).
        commands.entity(e).insert(SeatedCritter {
            kind: s.kind,
            anchor: s.anchor,
            phase: s.phase,
            hidden: s.hidden,
        });
    }
}

/// Seating data the animator reads; inserted by `update_ambient_critters`.
#[derive(Component)]
pub struct SeatedCritter {
    kind: CritterKind,
    anchor: Vec3,
    phase: f32,
    hidden: bool,
}

/// Per-frame critter animation — a pure function of (seed, time).
pub fn animate_ambient_critters(
    time: Res<Time>,
    q_player: Query<&Transform, (With<crate::PlayerTag>, Without<AmbientCritter>)>,
    mut critters: Query<
        (&AmbientCritter, &SeatedCritter, &mut Transform),
        Without<crate::PlayerTag>,
    >,
) {
    let t = time.elapsed_secs();
    let player = q_player.get_single().ok().map(|p| p.translation);
    for (c, s, mut tf) in &mut critters {
        if s.hidden {
            tf.scale = Vec3::ZERO;
            continue;
        }
        let seed = (c.slot as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15);
        let p = s.phase;
        match s.kind {
            CritterKind::Butterfly => {
                // Lissajous wander around the anchor + bob; scale.x pulses
                // at ~9 Hz to fake the wingbeat.
                let r = 10.0 + h01(seed ^ 0xF1) * 14.0;
                let w1 = 0.35 + h01(seed ^ 0xF2) * 0.3;
                let w2 = 0.47 + h01(seed ^ 0xF3) * 0.3;
                let x = s.anchor.x + (t * w1 + p).cos() * r;
                let z = s.anchor.z + (t * w2 + p * 1.7).sin() * r;
                let y = s.anchor.y + (t * 2.1 + p).sin() * 2.0;
                // Face along the velocity.
                let vx = -(t * w1 + p).sin() * r * w1;
                let vz = (t * w2 + p * 1.7).cos() * r * w2;
                let yaw = vz.atan2(vx);
                let flap = 1.0 + 0.45 * (t * 19.0 + p * 7.0).sin();
                tf.translation = Vec3::new(x, y, z);
                tf.rotation = Quat::from_rotation_y(-yaw + std::f32::consts::FRAC_PI_2);
                tf.scale = Vec3::new(flap, 1.0, 1.0);
            }
            CritterKind::Bird => {
                // Slow circling thermal; bank into the turn.
                let r = 30.0 + h01(seed ^ 0xF1) * 40.0;
                let w = (0.25 + h01(seed ^ 0xF2) * 0.2)
                    * if h01(seed ^ 0xF5) < 0.5 { 1.0 } else { -1.0 };
                let a = t * w + p;
                let x = s.anchor.x + a.cos() * r;
                let z = s.anchor.z + a.sin() * r;
                let y = s.anchor.y + (t * 0.6 + p).sin() * 5.0;
                let yaw = (-a.sin() * w.signum()).atan2(a.cos() * w.signum());
                let flap = 1.0 + 0.28 * (t * 8.0 + p * 5.0).sin();
                tf.translation = Vec3::new(x, y, z);
                tf.rotation =
                    Quat::from_rotation_y(-yaw) * Quat::from_rotation_z(0.35 * w.signum());
                tf.scale = Vec3::new(1.0, flap, 1.0);
            }
            CritterKind::Rabbit => {
                // Drift around the anchor on a slow loop, hop-bob on top.
                // Flee: hop faster when the player is close.
                let flee = player
                    .map(|pp| {
                        let d = Vec2::new(s.anchor.x - pp.x, s.anchor.z - pp.z);
                        (d.length() < 90.0, d.normalize_or_zero())
                    })
                    .unwrap_or((false, Vec2::ZERO));
                let speed = if flee.0 { 2.2 } else { 1.0 };
                let r = 14.0 + h01(seed ^ 0xF1) * 18.0;
                let w = 0.12 * speed;
                let a = t * w + p;
                let mut x = s.anchor.x + a.cos() * r;
                let mut z = s.anchor.z + a.sin() * r;
                if flee.0 {
                    x += flee.1.x * 40.0;
                    z += flee.1.y * 40.0;
                }
                // Hop: ballistic arcs at ~2.5 Hz, pausing between bursts.
                let hp = t * 2.5 * speed + p * 3.0;
                let burst = (t * 0.45 * speed + p).sin() > 0.1;
                let hop = if burst {
                    let ph = hp.fract();
                    4.0 * ph * (1.0 - ph) * 7.0
                } else {
                    0.0
                };
                let yaw = (-(t * w + p).sin()).atan2((t * w + p).cos());
                tf.translation = Vec3::new(x, s.anchor.y + hop, z);
                tf.rotation = Quat::from_rotation_y(-yaw + std::f32::consts::FRAC_PI_2);
                // Squash-and-stretch on the hop.
                let sq = if burst {
                    1.0 + 0.18 * (hp * std::f32::consts::TAU).sin()
                } else {
                    1.0
                };
                tf.scale = Vec3::new(2.0 - sq, sq, 2.0 - sq);
            }
        }
    }
}

#[derive(Component)]
pub struct SeatedParticle {
    kind: ParticleKind,
    anchor: Vec3,
    phase: f32,
}

/// Re-seat the particle pool on cell/act change. Tracks its own epoch in
/// `AmbientState` — the critter and particle update systems run in ambiguous
/// order, so they must not share one "changed" flag.
pub fn update_ambient_particles(
    mut commands: Commands,
    session: Res<crate::Session>,
    q_player: Query<&Transform, (With<crate::PlayerTag>, Without<AmbientParticle>)>,
    mut state: ResMut<AmbientState>,
    mut particles: Query<
        (
            Entity,
            &AmbientParticle,
            &mut Transform,
            &mut MeshMaterial3d<StandardMaterial>,
        ),
        Without<crate::PlayerTag>,
    >,
    assets: Res<AmbientMeshes>,
) {
    let Ok(pt) = q_player.get_single() else {
        return;
    };
    let act = session.act;
    let cell = (
        (pt.translation.x / CELL).floor() as i64,
        (pt.translation.z / CELL).floor() as i64,
    );
    if cell == state.pcell && act == state.pact {
        return;
    }
    state.pcell = cell;
    state.pact = act;
    let (pcx, pcz) = (pt.translation.x, pt.translation.z);
    for (e, p, mut t, mut m) in &mut particles {
        let s = particle_slot(p.slot, act, pcx, pcz);
        m.0 = assets.particle_mats[particle_index(s.kind)].clone();
        t.translation = s.anchor;
        commands.entity(e).insert(SeatedParticle {
            kind: s.kind,
            anchor: s.anchor,
            phase: s.phase,
        });
    }
}

/// Day factor 0 (midnight) → 1 (noon), matching `lighting::apply_env_map`.
fn day_factor(time_of_day: f32) -> f32 {
    ((time_of_day - 0.25) * std::f32::consts::TAU)
        .sin()
        .max(0.0)
}

/// Per-frame particle drift — pure function of (seed, time).
pub fn animate_ambient_particles(
    time: Res<Time>,
    session: Res<crate::Session>,
    mut particles: Query<
        (&AmbientParticle, &SeatedParticle, &mut Transform),
        Without<crate::PlayerTag>,
    >,
) {
    let t = time.elapsed_secs();
    let day = day_factor(session.time_of_day);
    for (p, s, mut tf) in &mut particles {
        let seed = (p.slot as u64).wrapping_mul(0x51ED_7B8D_9C3A_1F05);
        let ph = s.phase;
        match s.kind {
            ParticleKind::Firefly => {
                // Night-only; hidden (scale 0) by day.
                let night = (0.30 - day).clamp(0.0, 1.0) / 0.30;
                if night <= 0.0 {
                    tf.scale = Vec3::ZERO;
                    continue;
                }
                let r = 6.0 + h01(seed ^ 0xE1) * 6.0;
                let x = s.anchor.x + (t * 0.5 + ph).cos() * r;
                let z = s.anchor.z + (t * 0.7 + ph * 1.3).sin() * r;
                let y = s.anchor.y + (t * 1.1 + ph).sin() * 2.5;
                let blink = ((t * 2.2 + ph * 4.0).sin() * 0.5 + 0.5).powf(2.0);
                tf.translation = Vec3::new(x, y, z);
                tf.scale = Vec3::splat((0.4 + 1.6 * blink) * night);
            }
            ParticleKind::Dust => {
                let r = 12.0 + h01(seed ^ 0xE1) * 16.0;
                let x = s.anchor.x + (t * 0.22 + ph).cos() * r;
                let z = s.anchor.z + (t * 0.17 + ph * 2.1).sin() * r;
                let y = s.anchor.y + (t * 0.35 + ph).sin() * 4.0;
                // Brightest at midday, fading at night.
                let b = 0.25 + 0.75 * day;
                tf.translation = Vec3::new(x, y, z);
                tf.scale = Vec3::splat(b);
            }
            ParticleKind::Leaf => {
                // Falls, sways, wraps back to the top of its column.
                let fall = 6.0 + h01(seed ^ 0xE1) * 4.0;
                let span = 24.0;
                let k = ((t * fall + ph * span) % span) / span;
                let x = s.anchor.x + (t * 1.3 + ph * 6.0).sin() * 6.0 + k * 18.0;
                let z = s.anchor.z + (t * 0.9 + ph * 3.0).cos() * 6.0;
                let y = s.anchor.y + span * 0.5 - k * span;
                tf.translation = Vec3::new(x, y, z);
                tf.rotation = Quat::from_rotation_y(t * 2.0 + ph * 9.0)
                    * Quat::from_rotation_x(t * 1.4 + ph * 5.0);
                tf.scale = Vec3::splat(1.4);
            }
            ParticleKind::Ember => {
                // Rises from the anchor, flickers, wraps.
                let rise = 9.0 + h01(seed ^ 0xE1) * 7.0;
                let span = 30.0;
                let k = ((t * rise + ph * span) % span) / span;
                let x = s.anchor.x + (t * 1.8 + ph * 8.0).sin() * (2.0 + k * 8.0);
                let z = s.anchor.z + (t * 1.2 + ph * 5.0).cos() * (2.0 + k * 8.0);
                let y = s.anchor.y + k * span;
                let flick = 0.7 + 0.5 * (t * 11.0 + ph * 20.0).sin();
                tf.translation = Vec3::new(x, y, z);
                tf.scale = Vec3::splat(flick * (1.0 - k * 0.6));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_act_has_critters_and_particles() {
        for act in Act::ALL {
            assert!(!act_critters(act).is_empty(), "{act:?} has no critters");
            assert!(!act_particles(act).is_empty(), "{act:?} has no particles");
            let cw: f32 = act_critters(act).iter().map(|(_, w)| w).sum();
            let pw: f32 = act_particles(act).iter().map(|(_, w)| w).sum();
            assert!(cw > 0.0 && pw > 0.0);
        }
    }

    #[test]
    fn slot_layout_is_deterministic() {
        for act in Act::ALL {
            for slot in [0usize, 1, 17, 95] {
                let a = critter_slot(slot, act, 1234.0, -567.0);
                let b = critter_slot(slot, act, 1234.0, -567.0);
                assert_eq!(a, b, "critter slot {slot} in {act:?} not deterministic");
                let a = particle_slot(slot, act, 1234.0, -567.0);
                let b = particle_slot(slot, act, 1234.0, -567.0);
                assert_eq!(a, b, "particle slot {slot} in {act:?} not deterministic");
            }
        }
    }

    #[test]
    fn critter_meshes_are_valid() {
        for m in [butterfly_mesh(), bird_mesh(), rabbit_mesh()] {
            assert!(m.count_vertices() > 0);
            assert!(m.indices().is_some());
            assert!(m.attribute(Mesh::ATTRIBUTE_COLOR).is_some());
        }
    }

    #[test]
    fn weighted_pick_covers_every_kind() {
        // With 2000 draws every kind in every act's table must appear —
        // guards against a weight typo zeroing a species out.
        for act in Act::ALL {
            let mut seen_c = std::collections::HashSet::new();
            let mut seen_p = std::collections::HashSet::new();
            for i in 0..2000 {
                seen_c.insert(critter_slot(i, act, 0.0, 0.0).kind as u8);
                seen_p.insert(particle_slot(i, act, 0.0, 0.0).kind as u8);
            }
            assert_eq!(
                seen_c.len(),
                act_critters(act).len(),
                "{act:?} critter kind never placed"
            );
            assert_eq!(
                seen_p.len(),
                act_particles(act).len(),
                "{act:?} particle kind never placed"
            );
        }
    }
}
