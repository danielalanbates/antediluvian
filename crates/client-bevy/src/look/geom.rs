// Copyright (c) 2026 Daniel Bates / Bates LLC. All rights reserved.
//! Shapes behind a look: face morphs, marking masks, and the hair, beard and
//! mantle meshes grafted onto a rig's head and chest bones.
//!
//! Everything here is a pure function of the look, so the distinct-option
//! tests can count real geometry. Grafts follow `propgen`'s rules: built
//! from shared primitives, colour in vertex attributes, one merged mesh per
//! bone (hair and beard together), one shared material.
//!
//! Coordinates. Face morphs and markings work in the head MESH's space (the
//! rig's model space: feet at y 0, face toward +Z, eyes near y 1.6). Grafts
//! are built in BONE space: the head bone sits at the neck (model y 1.2425),
//! the chest bone at model y 0.973.

use antediluvia_protocol::look::Look;
use bevy::prelude::*;

use super::{Rgb, DYE, HAIR_COLOR};
use crate::propgen::Builder;

/// Model-space height of the head bone (sum of the rig's bind translations).
pub const HEAD_BONE_Y: f32 = 1.2425;

pub fn srgb_to_linear(c: f32) -> f32 {
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

fn lin(c: Rgb) -> [f32; 4] {
    [srgb_to_linear(c[0]), srgb_to_linear(c[1]), srgb_to_linear(c[2]), 1.0]
}

// ---------------------------------------------------------------------------
// Faces
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FaceShape {
    /// Width of the jaw and cheeks (1 = as modelled).
    pub jaw: f32,
    /// Extra chin length, downward (negative shortens).
    pub chin: f32,
    /// Nose size about its base.
    pub nose: f32,
    /// How far the brow ridge stands out.
    pub brow: f32,
}

/// 24 faces: 4 jaws x 3 noses x 2 brows.
pub fn face_shape(i: u8) -> FaceShape {
    let i = i as usize % 24;
    let (jaw, chin) = [(0.86, 0.045), (1.0, 0.0), (1.13, 0.0), (1.08, -0.05)][i % 4];
    let nose = [0.70, 1.0, 1.38][(i / 4) % 3];
    let brow = [0.0, 0.06][i / 12];
    FaceShape { jaw, chin, nose, brow }
}

fn smooth(a: f32, b: f32, x: f32) -> f32 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

const NOSE_BASE: Vec3 = Vec3::new(0.0, 1.475, 0.42);

/// Move one head-mesh vertex for a face shape. Only the front of the head
/// moves, so hair and the back of the skull keep their shape.
pub fn morph_face(f: FaceShape, p: Vec3) -> Vec3 {
    let mut q = p;
    if p.z > -0.05 {
        let front = smooth(-0.05, 0.15, p.z);
        // Jaw and cheeks: widest effect at the jawline, none above the eyes.
        let w = smooth(1.56, 1.30, p.y) * front;
        q.x = p.x * (1.0 + (f.jaw - 1.0) * w);
        // Chin.
        q.y = p.y - f.chin * smooth(1.44, 1.24, p.y) * front;
    }
    // Nose: the part that stands proud of the face.
    if p.z > 0.45 && p.x.abs() < 0.25 && p.y > 1.40 && p.y < 1.58 {
        q = NOSE_BASE + (q - NOSE_BASE) * f.nose;
    }
    // Brow ridge.
    if p.z > 0.30 && p.x.abs() < 0.42 && p.y > 1.68 && p.y < 1.80 {
        q.z += f.brow * (1.0 - ((p.y - 1.74).abs() / 0.06)).max(0.0);
    }
    q
}

/// Reference points on a face, for counting distinct face shapes.
fn face_probe() -> Vec<Vec3> {
    let mut v = Vec::new();
    for yi in 0..14 {
        for xi in -5..=5 {
            let (x, y) = (xi as f32 * 0.08, 1.24 + yi as f32 * 0.045);
            v.push(Vec3::new(x, y, 0.42 - x * x * 1.4));
        }
    }
    // The nose.
    for (x, y, z) in [(0.0, 1.50, 0.56), (0.08, 1.46, 0.50), (-0.08, 1.46, 0.50), (0.0, 1.44, 0.52)] {
        v.push(Vec3::new(x, y, z));
    }
    v
}

pub fn face_fingerprint(i: u8) -> u64 {
    let f = face_shape(i);
    let mut h = 0xCBF2_9CE4_8422_2325u64;
    for p in face_probe() {
        let q = morph_face(f, p);
        for c in [q.x, q.y, q.z] {
            h = (h ^ ((c * 1000.0).round() as i64 as u64)).wrapping_mul(0x100_0000_01B3);
        }
    }
    h
}

// ---------------------------------------------------------------------------
// Markings
// ---------------------------------------------------------------------------

/// Scars take the skin's own colour, lightened; paint takes the marking colour.
pub fn is_scar(mark: u8) -> bool {
    (14..=18).contains(&mark)
}

/// How strongly marking `mark` covers a point on the face (model space).
pub fn marking_weight(mark: u8, p: Vec3) -> f32 {
    if p.z < 0.22 || p.y < 1.22 || p.y > 1.95 {
        return 0.0; // only the front of the face
    }
    let (x, y) = (p.x, p.y);
    let hit = match mark {
        1 => y > 1.72,
        2 => y > 1.50 && y < 1.72,
        3 => y < 1.40,
        4 => y < 1.50,
        5 => x < -0.01,
        6 => x > 0.01,
        7 => x.abs() < 0.09,
        8 => x.abs() > 0.28 && y > 1.36 && y < 1.56,
        9 => x.abs() < 0.09 || (y > 1.50 && y < 1.62),
        10 => (((x + y * 0.8) * 9.0).floor() as i32).rem_euclid(2) == 0,
        11 => (x.abs() - 0.30).powi(2) + (y - 1.45).powi(2) < 0.0081,
        12 => x.abs() < 0.13 && y > 1.74,
        13 => x.abs() > 0.13 && x.abs() < 0.25 && y > 1.32 && y < 1.52,
        14 => (x - (-0.20 + (y - 1.6) * 0.35)).abs() < 0.07 && y > 1.40 && y < 1.82,
        15 => (y - (1.42 + (x - 0.25) * 0.5)).abs() < 0.06 && x > 0.08,
        16 => (y - 1.50 - x * 0.15).abs() < 0.055 && x.abs() < 0.32,
        17 => (x - 0.22).abs() < 0.07 && y > 1.66,
        18 => x < -0.08 && y < 1.62,
        19 => true,
        _ => false,
    };
    if hit {
        1.0
    } else {
        0.0
    }
}

pub fn marking_fingerprint(mark: u8) -> u64 {
    let mut h = 0xCBF2_9CE4_8422_2325u64 ^ is_scar(mark) as u64;
    for yi in 0..36 {
        for xi in -25..=25 {
            let p = Vec3::new(xi as f32 * 0.02, 1.24 + yi as f32 * 0.02, 0.4);
            h = (h ^ (marking_weight(mark, p) > 0.5) as u64).wrapping_mul(0x100_0000_01B3);
        }
    }
    h
}

// ---------------------------------------------------------------------------
// Hair, beards, mantles
// ---------------------------------------------------------------------------

const KNOT: u32 = 1;
const LOW_BUN: u32 = 1 << 1;
const HIGH_BUN: u32 = 1 << 2;
const MOHAWK: u32 = 1 << 3;
const CREST: u32 = 1 << 4;
const SPIKES: u32 = 1 << 5;
const POMP: u32 = 1 << 6;
const PONY: u32 = 1 << 7;
const FALL: u32 = 1 << 8;
const BRAID: u32 = 1 << 9;
const TWIN_TAILS: u32 = 1 << 10;
const FRONT_BRAIDS: u32 = 1 << 11;
const SIDE_BUNS: u32 = 1 << 12;
const LOCKS: u32 = 1 << 13;
const MANE: u32 = 1 << 14;
const FLAT: u32 = 1 << 15;
const CURLS: u32 = 1 << 16;
const HORNS: u32 = 1 << 17;
const RING: u32 = 1 << 18;
const QUEUE: u32 = 1 << 19;

/// The pieces each of the 32 hair styles is made of (same order as `HAIR`).
pub const HAIR_PIECES: [u32; 32] = [
    0,
    KNOT,
    LOW_BUN,
    HIGH_BUN,
    MOHAWK,
    CREST,
    SPIKES,
    POMP,
    PONY,
    KNOT | PONY,
    FALL,
    BRAID,
    TWIN_TAILS,
    FRONT_BRAIDS,
    SIDE_BUNS,
    LOCKS,
    MANE,
    FLAT,
    CURLS,
    HORNS,
    MOHAWK | BRAID,
    KNOT | LOCKS,
    LOW_BUN | FRONT_BRAIDS,
    SIDE_BUNS | FALL,
    CURLS | PONY,
    CREST | TWIN_TAILS,
    POMP | PONY,
    SPIKES | MANE,
    FLAT | LOCKS,
    RING,
    RING | FALL,
    QUEUE,
];

/// Beads from `a` to `z`, bulging toward `mid` (ponytails, braids, locks).
#[allow(clippy::too_many_arguments)]
fn strand(b: &mut Builder, a: Vec3, mid: Vec3, z: Vec3, r0: f32, r1: f32, n: u32, seed: u64, c: [f32; 4]) {
    for k in 0..=n {
        let t = k as f32 / n as f32;
        let p = a * (1.0 - t) * (1.0 - t) + mid * 2.0 * t * (1.0 - t) + z * t * t;
        let r = r0 + (r1 - r0) * t;
        b.blob(p, Vec3::splat(r), 3, 6, 0.06, seed ^ (k as u64 * 37 + 9), c);
    }
}

/// Height of the skull surface above the head bone at depth `z` on the midline.
fn crown_y(z: f32) -> f32 {
    0.50 + 0.47 * (1.0 - (z / 0.52).powi(2)).max(0.0).sqrt()
}

fn add_hair(b: &mut Builder, style: u8, c: [f32; 4]) {
    let m = HAIR_PIECES[style as usize % 32];
    let s = style as u64 * 1000;
    let v = Vec3::new;
    if m & KNOT != 0 {
        b.prism(v(0.0, 0.90, -0.05), 6, 0.10, 0.08, 0.14, 0.0, Vec3::ZERO, 0.04, s + 1, c);
        b.blob(v(0.0, 1.14, -0.05), v(0.17, 0.16, 0.17), 4, 7, 0.08, s + 2, c);
    }
    if m & LOW_BUN != 0 {
        b.blob(v(0.0, 0.52, -0.58), v(0.21, 0.20, 0.18), 4, 7, 0.08, s + 3, c);
    }
    if m & HIGH_BUN != 0 {
        b.blob(v(0.0, 1.06, -0.22), v(0.22, 0.20, 0.22), 4, 7, 0.08, s + 4, c);
    }
    if m & MOHAWK != 0 {
        for i in 0..6u64 {
            let z = 0.32 - i as f32 * 0.15;
            let h = 0.36 - (i as f32 - 2.0).abs() * 0.03;
            b.prism(v(0.0, crown_y(z) - 0.06, z), 4, 0.075, 0.015, h, 0.78, v(0.0, 0.0, -0.06), 0.05, s + 10 + i, c);
        }
    }
    if m & CREST != 0 {
        for i in 0..5u64 {
            let z = 0.30 - i as f32 * 0.16;
            let h = 0.20 + (1.0 - (i as f32 - 2.0).abs() / 2.5) * 0.18;
            b.boxy(v(0.0, crown_y(z) - 0.05, z), 0.07, h, 0.17, 0.0, s + 20 + i, c);
        }
    }
    if m & SPIKES != 0 {
        for i in 0..8u64 {
            let a = std::f32::consts::TAU * i as f32 / 8.0;
            let (x, z) = (a.cos() * 0.30, a.sin() * 0.30);
            b.prism(v(x, 0.80, z), 5, 0.085, 0.01, 0.34, a, v(x, 0.0, z) * 0.7, 0.06, s + 30 + i, c);
        }
        b.prism(v(0.0, 0.92, 0.0), 5, 0.09, 0.01, 0.36, 0.0, Vec3::ZERO, 0.06, s + 39, c);
    }
    if m & POMP != 0 {
        b.blob(v(0.0, 0.97, 0.30), v(0.30, 0.19, 0.24), 4, 8, 0.08, s + 40, c);
        b.blob(v(0.0, 1.02, 0.08), v(0.26, 0.15, 0.22), 4, 8, 0.08, s + 41, c);
    }
    if m & PONY != 0 {
        strand(b, v(0.0, 0.82, -0.48), v(0.0, 0.75, -0.95), v(0.0, 0.10, -0.80), 0.12, 0.05, 8, s + 50, c);
    }
    if m & FALL != 0 {
        b.blob(v(0.0, 0.28, -0.52), v(0.44, 0.50, 0.15), 5, 8, 0.07, s + 60, c);
        b.blob(v(0.0, -0.12, -0.52), v(0.36, 0.30, 0.12), 4, 8, 0.07, s + 61, c);
    }
    if m & BRAID != 0 {
        for i in 0..10u64 {
            let t = i as f32 / 9.0;
            let x = if i % 2 == 0 { 0.025 } else { -0.025 };
            b.blob(v(x, 0.62 - t * 1.05, -0.54 - t * 0.04), v(0.085, 0.075, 0.075), 3, 6, 0.05, s + 70 + i, c);
        }
    }
    if m & TWIN_TAILS != 0 {
        for side in [-1.0f32, 1.0] {
            strand(b, v(side * 0.40, 0.78, -0.28), v(side * 0.80, 0.70, -0.40), v(side * 0.58, 0.0, -0.36), 0.11, 0.04, 7, s + 80 + (side > 0.0) as u64 * 40, c);
        }
    }
    if m & FRONT_BRAIDS != 0 {
        for side in [-1.0f32, 1.0] {
            strand(b, v(side * 0.50, 0.55, 0.12), v(side * 0.52, 0.10, 0.22), v(side * 0.40, -0.38, 0.32), 0.075, 0.05, 8, s + 90 + (side > 0.0) as u64 * 40, c);
        }
    }
    if m & SIDE_BUNS != 0 {
        for side in [-1.0f32, 1.0] {
            b.blob(v(side * 0.58, 0.74, -0.05), v(0.17, 0.18, 0.18), 4, 7, 0.08, s + 100 + (side > 0.0) as u64, c);
        }
    }
    if m & LOCKS != 0 {
        for side in [-1.0f32, 1.0] {
            strand(b, v(side * 0.50, 0.52, 0.26), v(side * 0.56, 0.25, 0.32), v(side * 0.46, -0.02, 0.34), 0.065, 0.03, 6, s + 110 + (side > 0.0) as u64 * 40, c);
        }
    }
    if m & MANE != 0 {
        b.blob(v(0.0, 0.50, -0.50), v(0.58, 0.52, 0.26), 5, 9, 0.12, s + 120, c);
        for side in [-1.0f32, 1.0] {
            b.blob(v(side * 0.44, 0.22, -0.38), v(0.24, 0.30, 0.22), 4, 7, 0.12, s + 121 + (side > 0.0) as u64, c);
        }
    }
    if m & FLAT != 0 {
        b.boxy(v(0.0, 0.90, 0.0), 0.72, 0.22, 0.72, 0.0, s + 130, c);
    }
    if m & CURLS != 0 {
        for i in 0..9u64 {
            let (x, z) = if i == 8 {
                (0.0, 0.0)
            } else {
                let a = std::f32::consts::TAU * i as f32 / 8.0;
                (a.cos() * 0.33, a.sin() * 0.33)
            };
            let y = if i == 8 { 1.02 } else { 0.86 };
            b.blob(v(x, y, z), Vec3::splat(0.16), 3, 6, 0.1, s + 140 + i, c);
        }
    }
    if m & HORNS != 0 {
        for side in [-1.0f32, 1.0] {
            b.prism(v(side * 0.30, 0.84, 0.0), 5, 0.11, 0.02, 0.44, 0.0, v(side * 0.26, 0.0, 0.0), 0.05, s + 150 + (side > 0.0) as u64, c);
        }
    }
    if m & RING != 0 {
        for i in 0..16u64 {
            let a = std::f32::consts::TAU * i as f32 / 16.0;
            b.blob(v(a.cos() * 0.50, 0.80, a.sin() * 0.50), v(0.075, 0.065, 0.075), 3, 6, 0.05, s + 160 + i, c);
        }
    }
    if m & QUEUE != 0 {
        b.prism(v(0.0, 0.92, -0.16), 6, 0.07, 0.06, 0.10, 0.0, Vec3::ZERO, 0.03, s + 180, c);
        strand(b, v(0.0, 1.02, -0.18), v(0.0, 0.95, -0.85), v(0.0, -0.30, -0.62), 0.055, 0.035, 12, s + 181, c);
    }
}

fn add_beard(b: &mut Builder, beard: u8, c: [f32; 4]) {
    let s = 9000 + beard as u64 * 100;
    let v = Vec3::new;
    let goatee = |b: &mut Builder| b.blob(v(0.0, -0.02, 0.42), v(0.11, 0.14, 0.09), 3, 7, 0.08, s + 1, c);
    let short = |b: &mut Builder| b.blob(v(0.0, 0.02, 0.34), v(0.36, 0.14, 0.20), 4, 8, 0.08, s + 2, c);
    let full = |b: &mut Builder| b.blob(v(0.0, -0.04, 0.30), v(0.44, 0.24, 0.26), 4, 9, 0.08, s + 3, c);
    let stache = |b: &mut Builder| {
        for side in [-1.0f32, 1.0] {
            b.blob(v(side * 0.13, 0.16, 0.50), v(0.13, 0.05, 0.06), 3, 6, 0.06, s + 4 + (side > 0.0) as u64, c);
        }
    };
    match beard % 14 {
        1 => goatee(b),
        2 => short(b),
        3 => full(b),
        4 => {
            full(b);
            b.prism(v(0.0, -0.62, 0.36), 6, 0.05, 0.30, 0.56, 0.0, v(0.0, 0.0, -0.04), 0.06, s + 10, c);
        }
        5 => {
            short(b);
            for side in [-1.0f32, 1.0] {
                b.prism(v(side * 0.15, -0.52, 0.38), 5, 0.03, 0.14, 0.48, 0.0, v(-side * 0.04, 0.0, -0.03), 0.06, s + 11 + (side > 0.0) as u64, c);
            }
        }
        6 => {
            goatee(b);
            strand(b, v(0.0, -0.12, 0.43), v(0.0, -0.35, 0.47), v(0.0, -0.62, 0.44), 0.075, 0.045, 7, s + 13, c);
        }
        7 => stache(b),
        8 => {
            stache(b);
            for side in [-1.0f32, 1.0] {
                b.prism(v(side * 0.27, 0.14, 0.48), 4, 0.035, 0.008, 0.16, 0.0, v(side * 0.07, 0.0, 0.0), 0.04, s + 14 + (side > 0.0) as u64, c);
            }
        }
        9 => {
            for i in 0..11u64 {
                let a = -1.35 + 2.7 * i as f32 / 10.0;
                b.blob(v(a.sin() * 0.47, 0.03 + (1.0 - a.cos()) * 0.26, 0.42 * a.cos() + 0.02), v(0.07, 0.065, 0.07), 3, 6, 0.05, s + 20 + i, c);
            }
        }
        10 => {
            for side in [-1.0f32, 1.0] {
                b.blob(v(side * 0.46, 0.20, 0.22), v(0.10, 0.22, 0.17), 4, 7, 0.08, s + 32 + (side > 0.0) as u64, c);
            }
        }
        11 => {
            b.blob(v(0.0, 0.07, 0.47), v(0.05, 0.055, 0.04), 3, 6, 0.05, s + 34, c);
        }
        12 => {
            short(b);
            for side in [-1.0f32, 1.0] {
                strand(b, v(side * 0.14, -0.08, 0.42), v(side * 0.17, -0.34, 0.46), v(side * 0.16, -0.62, 0.44), 0.06, 0.04, 7, s + 40 + (side > 0.0) as u64 * 20, c);
            }
        }
        13 => {
            full(b);
            b.prism(v(0.0, -0.95, 0.34), 7, 0.04, 0.36, 0.88, 0.0, v(0.0, 0.0, -0.04), 0.07, s + 60, c);
        }
        _ => {}
    }
}

/// Hair and beard, merged into one mesh in head-bone space. `None` when the
/// look has neither (style "Natural", no beard): the rig's own hair shows.
pub fn head_graft(look: Look) -> Option<Mesh> {
    let look = look.clamped();
    if HAIR_PIECES[look.hair as usize] == 0 && look.beard == 0 {
        return None;
    }
    let c = lin(HAIR_COLOR[look.hair_color as usize].1);
    let mut b = Builder::default();
    add_hair(&mut b, look.hair, c);
    add_beard(&mut b, look.beard, c);
    Some(b.finish())
}

/// Colour of a grafted mantle: the dye, or natural hide when undyed.
pub fn mantle_color(look: Look) -> Rgb {
    DYE[look.dye as usize % DYE.len()].1
}

/// The mantle, in chest-bone space. `None` for no mantle and for the cape
/// (the cape is the rig's own mesh, shown or hidden).
pub fn mantle_graft(look: Look) -> Option<Mesh> {
    let look = look.clamped();
    let c = lin(mantle_color(look));
    let trim = [c[0] * 0.6, c[1] * 0.6, c[2] * 0.6, 1.0];
    let v = Vec3::new;
    let mut b = Builder::default();
    match look.mantle {
        2 => {
            for side in [-1.0f32, 1.0] {
                b.blob(v(side * 0.40, 0.22, 0.0), v(0.21, 0.13, 0.27), 4, 7, 0.16, 500 + (side > 0.0) as u64, c);
            }
            b.blob(v(0.0, 0.14, -0.36), v(0.38, 0.22, 0.13), 4, 8, 0.16, 502, c);
            b.blob(v(0.0, 0.16, 0.34), v(0.20, 0.10, 0.08), 3, 6, 0.12, 503, trim);
        }
        3 => {
            for i in 0..16u64 {
                let a = std::f32::consts::TAU * i as f32 / 16.0;
                // High on one shoulder, low on the opposite hip.
                b.blob(v(a.cos() * 0.45, 0.02 + a.cos() * 0.24, a.sin() * 0.42), v(0.085, 0.07, 0.085), 3, 6, 0.05, 510 + i, if i % 4 == 0 { trim } else { c });
            }
        }
        4 => {
            for i in 0..12u64 {
                let a = std::f32::consts::TAU * i as f32 / 12.0;
                b.blob(v(a.cos() * 0.34, 0.25, a.sin() * 0.31), v(0.10, 0.085, 0.10), 3, 6, 0.06, 530 + i, c);
            }
            strand(&mut b, v(0.16, 0.22, 0.32), v(0.20, 0.0, 0.44), v(0.18, -0.28, 0.42), 0.085, 0.07, 6, 545, trim);
        }
        5 => {
            b.prism(v(0.0, -0.04, 0.0), 9, 0.58, 0.30, 0.32, 0.0, Vec3::ZERO, 0.07, 560, c);
            b.prism(v(0.0, 0.26, 0.0), 9, 0.33, 0.30, 0.05, 0.0, Vec3::ZERO, 0.03, 561, trim);
        }
        _ => return None,
    }
    Some(b.finish())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::formations::{geom_hash, silhouette};
    use std::collections::HashSet;

    fn bounds(m: &Mesh) -> (Vec3, Vec3) {
        let p = m.attribute(Mesh::ATTRIBUTE_POSITION).and_then(|a| a.as_float3()).unwrap();
        let (mut lo, mut hi) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
        for v in p {
            assert!(v.iter().all(|c| c.is_finite()));
            lo = lo.min(Vec3::from(*v));
            hi = hi.max(Vec3::from(*v));
        }
        (lo, hi)
    }

    #[test]
    fn thirty_two_hair_styles_are_thirty_two_shapes() {
        let pieces: HashSet<u32> = HAIR_PIECES.iter().copied().collect();
        assert_eq!(pieces.len(), 32, "two styles share the same pieces");
        let mut geoms = HashSet::new();
        let mut shapes = HashSet::new();
        for s in 1..32u8 {
            let m = head_graft(Look { hair: s, ..Look::default() }).expect("styled hair has a mesh");
            let (lo, hi) = bounds(&m);
            // Hair stays on the head: within reach of the skull, above the waist.
            assert!(lo.y > -0.6 && hi.y < 1.5 && lo.x > -0.9 && hi.x < 0.9 && lo.z > -1.1 && hi.z < 0.7, "style {s}: {lo:?} {hi:?}");
            geoms.insert(geom_hash(&m));
            shapes.insert(silhouette(&m));
        }
        assert_eq!(geoms.len(), 31);
        assert_eq!(shapes.len(), 31, "two hair styles share a silhouette");
        assert!(head_graft(Look::default()).is_none(), "natural hair adds nothing");
    }

    #[test]
    fn fourteen_beards_are_distinct_and_sit_on_the_chin() {
        let mut geoms = HashSet::new();
        for bd in 1..14u8 {
            let m = head_graft(Look { beard: bd, ..Look::default() }).expect("beard has a mesh");
            let (lo, hi) = bounds(&m);
            assert!(hi.z > 0.3 && hi.z < 0.7, "beard {bd} is not on the face: {hi:?}");
            assert!(hi.y < 0.5 && lo.y > -1.0, "beard {bd}: {lo:?} {hi:?}");
            geoms.insert(geom_hash(&m));
        }
        assert_eq!(geoms.len(), 13);
    }

    #[test]
    fn mantles_are_distinct_and_dyed() {
        let mut geoms = HashSet::new();
        for mt in 2..6u8 {
            let m = mantle_graft(Look { mantle: mt, ..Look::default() }).expect("mantle mesh");
            let (lo, hi) = bounds(&m);
            assert!(lo.y > -0.4 && hi.y < 0.5 && hi.x < 0.75, "mantle {mt}: {lo:?} {hi:?}");
            geoms.insert(geom_hash(&m));
        }
        assert_eq!(geoms.len(), 4);
        assert!(mantle_graft(Look::default()).is_none());
        assert!(mantle_graft(Look { mantle: 1, ..Look::default() }).is_none(), "the cape is the rig's own");
        assert_ne!(mantle_color(Look { dye: 3, ..Look::default() }), mantle_color(Look { dye: 4, ..Look::default() }));
    }

    #[test]
    fn twenty_four_faces_are_twenty_four_shapes() {
        let prints: HashSet<u64> = (0..24).map(face_fingerprint).collect();
        assert_eq!(prints.len(), 24);
        // The back of the head never moves, whatever the face.
        for i in 0..24 {
            let p = Vec3::new(0.3, 1.6, -0.3);
            assert_eq!(morph_face(face_shape(i), p), p);
        }
        // Index 5 (oval, straight nose, soft brow) is the head as modelled.
        for p in super::face_probe() {
            assert!((morph_face(face_shape(5), p) - p).length() < 1e-6);
        }
    }

    #[test]
    fn twenty_markings_are_twenty_masks() {
        let prints: HashSet<u64> = (0..20).map(marking_fingerprint).collect();
        assert_eq!(prints.len(), 20);
        // Nothing paints the back of the head.
        for m in 0..20 {
            assert_eq!(marking_weight(m, Vec3::new(0.0, 1.6, -0.3)), 0.0);
        }
        assert_eq!(marking_weight(0, Vec3::new(0.0, 1.6, 0.4)), 0.0);
    }
}
