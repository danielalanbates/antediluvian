// Copyright (c) 2026 Daniel Bates / Bates LLC. All rights reserved.
//! Landform families and the world's scatter sites.
//!
//! `variety::formation_mesh` gives every scatter site its own deformed rock,
//! but all eight of its families are one warped dome: none has a hole, a gap
//! or more than one body. This module adds twelve landforms that do (cliff
//! walls, real arches, mesas with talus, spire clusters, boulder fields,
//! ruined walls, root tangles, reefs, balanced rocks, stone rings, basalt
//! columns, fallen colossi), and it owns the list of scatter sites so the
//! "how many distinct models does the world hold" test walks exactly the
//! sites the game spawns.
//!
//! Frame-rate rules (same as `propgen`): one merged mesh and one draw per
//! site, colour in vertex attributes so every landform shares the prop
//! material, and the caller's existing visibility range. No asset files.

use bevy::prelude::*;

use crate::propgen::{h01, rng, Builder, Rgba};

pub const LAND_FAMILIES: u32 = 12;

pub fn land_family_name(f: u32) -> &'static str {
    match f % LAND_FAMILIES {
        0 => "cliff_wall",
        1 => "natural_arch",
        2 => "mesa",
        3 => "spire_cluster",
        4 => "boulder_field",
        5 => "ruined_wall",
        6 => "root_tangle",
        7 => "reef",
        8 => "balanced_rock",
        9 => "stone_ring",
        10 => "basalt_columns",
        11 => "fallen_colossus",
        _ => "unknown",
    }
}

/// Landforms each act favours (index = position in `Act::ALL`): Eden is
/// rooted and gentle, Hermon is cliffs and spires, the Nephilim lands and
/// Enoch are full of broken works, the Flood coast is reef and sea arch.
const ACT_FAVOURS: [[u32; 4]; 5] = [
    [6, 4, 1, 9],
    [0, 3, 10, 2],
    [11, 5, 9, 8],
    [5, 11, 10, 0],
    [7, 1, 4, 0],
];

/// Which landform a site grows: mostly the act's own, sometimes any.
pub fn land_family_for(act_idx: u64, seed: u64) -> u32 {
    if h01(seed ^ 0x7E44) < 0.6 {
        ACT_FAVOURS[(act_idx % 5) as usize][(h01(seed ^ 0x7E45) * 4.0) as usize % 4]
    } else {
        (h01(seed ^ 0x7E46) * LAND_FAMILIES as f32) as u32 % LAND_FAMILIES
    }
}

fn rock(seed: u64, act_idx: u64) -> Rgba {
    let g = rng(seed, 201, 0.30, 0.52);
    // A faint mineral cast per act: green-grey, blue-grey, rust, ash, sand.
    let cast: [f32; 3] = match act_idx % 5 {
        0 => [0.96, 1.02, 0.92],
        1 => [0.94, 0.98, 1.06],
        2 => [1.08, 0.95, 0.88],
        3 => [1.00, 0.98, 0.96],
        _ => [1.06, 1.02, 0.90],
    };
    [g * cast[0], g * cast[1], g * cast[2], 1.0]
}

fn bark(seed: u64) -> Rgba {
    let v = rng(seed, 202, 0.16, 0.28);
    [v * 1.04, v * 0.88, v * 0.72, 1.0]
}

fn coral(seed: u64) -> Rgba {
    match (h01(seed ^ 0xC02A) * 5.0) as u32 {
        0 => [0.72, 0.36, 0.34, 1.0],
        1 => [0.78, 0.58, 0.40, 1.0],
        2 => [0.52, 0.34, 0.56, 1.0],
        3 => [0.70, 0.66, 0.56, 1.0],
        _ => [0.38, 0.56, 0.54, 1.0],
    }
}

/// A chain of blobs from `a` to `b` bulging toward `mid` (roots, coral limbs).
#[allow(clippy::too_many_arguments)]
fn bead_arc(b: &mut Builder, a: Vec3, mid: Vec3, z: Vec3, r0: f32, r1: f32, n: u32, seed: u64, c: Rgba) {
    for k in 0..=n {
        let t = k as f32 / n as f32;
        // Quadratic Bezier.
        let p = a * (1.0 - t) * (1.0 - t) + mid * 2.0 * t * (1.0 - t) + z * t * t;
        let r = r0 + (r1 - r0) * t;
        b.blob(p, Vec3::splat(r), 3, 6, 0.12, seed ^ (k as u64 * 37 + 9), c);
    }
}

/// One landform, authored about one to three units across so the caller's
/// formation scale (14 to 60) applies unchanged. Stands on y = 0.
pub fn land_mesh(seed: u64, family: u32, act_idx: u64) -> Mesh {
    let mut b = Builder::default();
    let s = rock(seed, act_idx);
    let dark = [s[0] * 0.78, s[1] * 0.78, s[2] * 0.78, 1.0];
    match family % LAND_FAMILIES {
        // Cliff wall: a run of tall irregular buttresses, shoulder to shoulder.
        0 => {
            let n = 4 + (h01(seed ^ 1) * 5.0) as u32;
            let run = rng(seed, 2, 2.2, 3.8);
            let curve = rng(seed, 3, -0.6, 0.6);
            for i in 0..n {
                let t = i as f32 / (n - 1) as f32 - 0.5;
                let h = rng(seed, 10 + i as u64, 1.1, 2.6);
                let w = run / n as f32 * rng(seed, 30 + i as u64, 0.55, 0.8);
                b.prism(
                    Vec3::new(t * run, 0.0, curve * t * t * 4.0),
                    5 + (h01(seed ^ (50 + i as u64)) * 3.0) as u32,
                    w,
                    w * rng(seed, 70 + i as u64, 0.55, 0.9),
                    h,
                    rng(seed, 90 + i as u64, 0.0, 6.28),
                    Vec3::new(0.0, 0.0, rng(seed, 110 + i as u64, -0.2, 0.2)),
                    0.16,
                    seed ^ (i as u64 * 131),
                    if i % 2 == 0 { s } else { dark },
                );
            }
        }
        // Natural arch: two legs and a span of wedged blocks, open underneath.
        1 => {
            let half = rng(seed, 1, 0.95, 1.5);
            let h = rng(seed, 2, 1.2, 2.2);
            let thick = rng(seed, 3, 0.28, 0.46);
            for side in [-1.0f32, 1.0] {
                b.prism(
                    Vec3::new(side * half, 0.0, 0.0),
                    6,
                    thick * 1.1,
                    thick,
                    h,
                    rng(seed, 4, 0.0, 6.28),
                    Vec3::new(-side * half * 0.18, 0.0, 0.0),
                    0.16,
                    seed ^ if side < 0.0 { 11 } else { 12 },
                    s,
                );
            }
            let blocks = 5 + (h01(seed ^ 5) * 4.0) as u32;
            for i in 0..blocks {
                let t = i as f32 / (blocks - 1) as f32;
                let ang = std::f32::consts::PI * t;
                let x = -ang.cos() * half * 0.86;
                let y = h + ang.sin() * half * rng(seed, 6, 0.25, 0.55);
                b.blob(
                    Vec3::new(x, y, 0.0),
                    Vec3::new(half * 0.34, thick * 0.9, thick * 1.1),
                    3,
                    7,
                    0.14,
                    seed ^ (i as u64 * 17 + 40),
                    if i % 2 == 0 { dark } else { s },
                );
            }
        }
        // Mesa: stepped flat-topped table with talus at its foot.
        2 => {
            let r = rng(seed, 1, 1.0, 1.7);
            let tiers = 1 + (h01(seed ^ 2) * 3.0) as u32;
            let mut y = 0.0;
            for t in 0..tiers {
                let k = 1.0 - t as f32 * 0.24;
                let h = rng(seed, 5 + t as u64, 0.35, 0.8);
                b.prism(
                    Vec3::new(rng(seed, 20 + t as u64, -0.15, 0.15), y, rng(seed, 25 + t as u64, -0.15, 0.15)),
                    7 + (h01(seed ^ (8 + t as u64)) * 4.0) as u32,
                    r * k,
                    r * k * 0.86,
                    h,
                    rng(seed, 12 + t as u64, 0.0, 6.28),
                    Vec3::ZERO,
                    0.14,
                    seed ^ (t as u64 * 91 + 3),
                    if t % 2 == 0 { s } else { dark },
                );
                y += h;
            }
            let talus = 3 + (h01(seed ^ 3) * 5.0) as u32;
            for i in 0..talus {
                let a = rng(seed, 40 + i as u64, 0.0, 6.28);
                let rr = rng(seed, 60 + i as u64, 0.12, 0.3);
                b.blob(Vec3::new(a.cos() * r * 1.05, rr * 0.6, a.sin() * r * 1.05), Vec3::new(rr, rr * 0.8, rr), 3, 6, 0.2, seed ^ (i as u64 * 7 + 80), dark);
            }
        }
        // Spire cluster: needles of different heights leaning apart.
        3 => {
            let n = 3 + (h01(seed ^ 1) * 5.0) as u32;
            for i in 0..n {
                let a = rng(seed, 10 + i as u64, 0.0, 6.28);
                let d = if i == 0 { 0.0 } else { rng(seed, 20 + i as u64, 0.3, 0.9) };
                let h = rng(seed, 30 + i as u64, 1.2, 3.2) * if i == 0 { 1.2 } else { 0.8 };
                let r = rng(seed, 40 + i as u64, 0.16, 0.34);
                b.prism(
                    Vec3::new(a.cos() * d, 0.0, a.sin() * d),
                    5,
                    r,
                    r * 0.12,
                    h,
                    a,
                    Vec3::new(a.cos(), 0.0, a.sin()) * d * 0.45,
                    0.14,
                    seed ^ (i as u64 * 53 + 1),
                    if i % 2 == 0 { s } else { dark },
                );
            }
        }
        // Boulder field: many separate stones, a few big, most small.
        4 => {
            let n = 5 + (h01(seed ^ 1) * 8.0) as u32;
            let spread = rng(seed, 2, 1.0, 1.9);
            for i in 0..n {
                let a = rng(seed, 10 + i as u64, 0.0, 6.28);
                let d = rng(seed, 30 + i as u64, 0.0, 1.0).sqrt() * spread;
                let r = rng(seed, 50 + i as u64, 0.14, 0.34) * if i < 2 { 1.9 } else { 1.0 };
                b.blob(
                    Vec3::new(a.cos() * d, r * 0.55, a.sin() * d),
                    Vec3::new(r * rng(seed, 70 + i as u64, 0.8, 1.4), r * rng(seed, 90 + i as u64, 0.6, 1.0), r),
                    4,
                    7,
                    0.2,
                    seed ^ (i as u64 * 29 + 5),
                    if i % 3 == 0 { dark } else { s },
                );
            }
        }
        // Ruined wall: courses broken to different heights, a gap, fallen blocks.
        5 => {
            let n = 4 + (h01(seed ^ 1) * 5.0) as u32;
            let w = rng(seed, 2, 0.42, 0.62);
            let gap = 1 + (h01(seed ^ 3) * (n - 2) as f32) as u32;
            let bend = rng(seed, 4, -0.5, 0.5);
            for i in 0..n {
                if i == gap {
                    continue; // the doorway
                }
                let x = (i as f32 - (n - 1) as f32 * 0.5) * w;
                let h = rng(seed, 10 + i as u64, 0.5, 2.0);
                b.boxy(Vec3::new(x, 0.0, bend * (x * x) * 0.2), w * 0.98, h, 0.34, bend * x * 0.2, seed ^ (i as u64 * 19 + 2), if i % 2 == 0 { s } else { dark });
            }
            let fallen = 2 + (h01(seed ^ 5) * 4.0) as u32;
            for i in 0..fallen {
                b.boxy(
                    Vec3::new(rng(seed, 40 + i as u64, -1.4, 1.4), 0.0, rng(seed, 50 + i as u64, 0.35, 1.1)),
                    rng(seed, 60 + i as u64, 0.25, 0.5),
                    rng(seed, 70 + i as u64, 0.14, 0.28),
                    rng(seed, 80 + i as u64, 0.2, 0.4),
                    rng(seed, 90 + i as u64, 0.0, 3.14),
                    seed ^ (i as u64 * 23 + 60),
                    dark,
                );
            }
        }
        // Root tangle: great roots arching out of the ground and back in.
        6 => {
            let c = bark(seed);
            let n = 3 + (h01(seed ^ 1) * 5.0) as u32;
            b.blob(Vec3::new(0.0, 0.25, 0.0), Vec3::new(0.5, 0.42, 0.5), 4, 7, 0.2, seed ^ 2, c);
            for i in 0..n {
                let a = std::f32::consts::TAU * i as f32 / n as f32 + rng(seed, 10 + i as u64, -0.3, 0.3);
                let reach = rng(seed, 20 + i as u64, 1.0, 2.0);
                let lift = rng(seed, 30 + i as u64, 0.5, 1.5);
                let dir = Vec3::new(a.cos(), 0.0, a.sin());
                bead_arc(
                    &mut b,
                    Vec3::new(0.0, 0.3, 0.0),
                    dir * reach * 0.5 + Vec3::Y * lift,
                    dir * reach - Vec3::Y * 0.08,
                    rng(seed, 40 + i as u64, 0.14, 0.24),
                    0.05,
                    9,
                    seed ^ (i as u64 * 61 + 7),
                    c,
                );
            }
        }
        // Reef: a rock base with branching coral and a few fans.
        7 => {
            let c = coral(seed);
            let c2 = coral(seed ^ 0x99);
            b.blob(Vec3::new(0.0, 0.2, 0.0), Vec3::new(rng(seed, 1, 0.8, 1.4), 0.32, rng(seed, 2, 0.7, 1.2)), 4, 8, 0.2, seed ^ 3, dark);
            let n = 4 + (h01(seed ^ 4) * 6.0) as u32;
            for i in 0..n {
                let a = rng(seed, 10 + i as u64, 0.0, 6.28);
                let d = rng(seed, 20 + i as u64, 0.0, 0.8);
                let base = Vec3::new(a.cos() * d, 0.3, a.sin() * d);
                let h = rng(seed, 30 + i as u64, 0.5, 1.5);
                let col = if i % 2 == 0 { c } else { c2 };
                b.prism(base, 5, 0.09, 0.05, h, a, Vec3::new(a.cos(), 0.0, a.sin()) * 0.25, 0.12, seed ^ (i as u64 * 43 + 1), col);
                // Two branches off each trunk.
                for k in 0..2u64 {
                    let ba = a + 1.2 + k as f32 * 2.4;
                    let from = base + Vec3::Y * h * (0.45 + k as f32 * 0.2) + Vec3::new(a.cos(), 0.0, a.sin()) * 0.12;
                    b.prism(from, 4, 0.05, 0.02, h * 0.5, ba, Vec3::new(ba.cos(), 0.0, ba.sin()) * h * 0.3, 0.1, seed ^ (i as u64 * 47 + k + 300), col);
                }
            }
        }
        // Balanced rock: a stack that should not stand, on a narrow neck.
        8 => {
            let n = 2 + (h01(seed ^ 1) * 3.0) as u32;
            let mut y = 0.0;
            let mut off = Vec3::ZERO;
            for i in 0..n {
                let top = i == n - 1;
                let r = if top { rng(seed, 5, 0.6, 1.0) } else { rng(seed, 10 + i as u64, 0.28, 0.5) };
                let h = if top { r * 0.75 } else { rng(seed, 20 + i as u64, 0.4, 0.8) };
                b.blob(off + Vec3::new(0.0, y + h, 0.0), Vec3::new(r, h, r * rng(seed, 30 + i as u64, 0.8, 1.2)), 4, 8, 0.16, seed ^ (i as u64 * 71 + 4), if i % 2 == 0 { s } else { dark });
                y += h * 1.7;
                off += Vec3::new(rng(seed, 40 + i as u64, -0.14, 0.14), 0.0, rng(seed, 50 + i as u64, -0.14, 0.14));
            }
        }
        // Stone ring: standing stones in a circle, some capped with lintels.
        9 => {
            let n = 5 + (h01(seed ^ 1) * 6.0) as u32;
            let r = rng(seed, 2, 1.1, 1.8);
            let lintels = h01(seed ^ 3) > 0.45;
            let missing = (h01(seed ^ 4) * n as f32) as u32;
            for i in 0..n {
                if i == missing {
                    continue;
                }
                let a = std::f32::consts::TAU * i as f32 / n as f32;
                let h = rng(seed, 10 + i as u64, 0.8, 1.5);
                let p = Vec3::new(a.cos() * r, 0.0, a.sin() * r);
                b.boxy(p, 0.34, h, 0.2, -a + 1.57, seed ^ (i as u64 * 13 + 6), if i % 2 == 0 { s } else { dark });
                if lintels && i % 2 == 0 && i + 1 != missing && i + 1 < n {
                    let a2 = std::f32::consts::TAU * (i as f32 + 0.5) / n as f32;
                    let chord = 2.0 * r * (std::f32::consts::PI / n as f32).sin();
                    b.boxy(Vec3::new(a2.cos() * r, h.min(1.0), a2.sin() * r), chord * 1.15, 0.16, 0.24, -a2 + 1.57, seed ^ (i as u64 * 13 + 400), dark);
                }
            }
            if h01(seed ^ 5) > 0.5 {
                b.boxy(Vec3::ZERO, 0.6, 0.22, 0.4, rng(seed, 6, 0.0, 3.14), seed ^ 500, s);
            }
        }
        // Basalt columns: packed hexagonal pillars stepping up to one side.
        10 => {
            let n = 7 + (h01(seed ^ 1) * 10.0) as u32;
            let r = rng(seed, 2, 0.2, 0.3);
            let slope = rng(seed, 3, 0.0, 6.28);
            for i in 0..n {
                // Hex spiral packing.
                let ring = ((i as f32).sqrt() * 0.9) as u32;
                let a = i as f32 * 2.4 + rng(seed, 4, 0.0, 1.0);
                let d = ring as f32 * r * 1.75;
                let p = Vec3::new(a.cos() * d, 0.0, a.sin() * d);
                let rise = (p.x * slope.cos() + p.z * slope.sin()) * 0.6;
                let h = (rng(seed, 10 + i as u64, 0.5, 1.2) + rise + 0.6).max(0.2);
                b.prism(p, 6, r, r * 0.97, h, 0.52, Vec3::ZERO, 0.03, seed ^ (i as u64 * 11 + 8), if i % 3 == 0 { dark } else { s });
            }
        }
        // Fallen colossus: drums of a toppled column and its broken plinth.
        _ => {
            let drums = 3 + (h01(seed ^ 1) * 4.0) as u32;
            let r = rng(seed, 2, 0.3, 0.46);
            let yaw = rng(seed, 3, 0.0, 6.28);
            let dir = Vec3::new(yaw.cos(), 0.0, yaw.sin());
            b.boxy(Vec3::ZERO, r * 3.0, 0.3, r * 3.0, yaw, seed ^ 4, dark);
            b.prism(Vec3::new(0.0, 0.3, 0.0), 8, r, r * 0.95, rng(seed, 5, 0.3, 0.9), 0.0, Vec3::ZERO, 0.05, seed ^ 5, s);
            for i in 0..drums {
                // Drums lie end to end, a little apart where they rolled.
                let along = 0.75 + i as f32 * rng(seed, 6, 0.46, 0.6);
                let side = rng(seed, 20 + i as u64, -0.3, 0.3);
                let p = dir * along + Vec3::new(-dir.z, 0.0, dir.x) * side + Vec3::Y * r * 0.9;
                // A lying drum: an ellipsoid squashed along the fall line.
                let len = rng(seed, 30 + i as u64, 0.2, 0.34);
                b.blob(p, Vec3::new(r * dir.z.abs() + len * dir.x.abs(), r, r * dir.x.abs() + len * dir.z.abs()), 3, 8, 0.06, seed ^ (i as u64 * 59 + 9), if i % 2 == 0 { s } else { dark });
            }
            if h01(seed ^ 7) > 0.4 {
                // The head, or what the weather left of it.
                let p = dir * (1.0 + drums as f32 * 0.6);
                b.blob(p + Vec3::Y * r, Vec3::splat(r * 1.25), 4, 8, 0.12, seed ^ 600, s);
            }
        }
    }
    b.finish()
}

// ---------------------------------------------------------------------------
// Scatter sites
// ---------------------------------------------------------------------------

/// Same hash as `main.rs::hash01`; site positions must not move.
fn hash01(seed: u64) -> f32 {
    let h = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15).wrapping_add(0x2545_F491_4F6C_DD1D);
    ((h >> 40) & 0xFF_FFFF) as f32 / 16_777_216.0
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SiteKind {
    /// `variety::formation_mesh` (the deformed-dome rocks).
    Rock,
    /// A landform family from this module.
    Land(u32),
    /// `propgen::prop_mesh`.
    Prop,
}

#[derive(Debug, Clone, Copy)]
pub struct Site {
    pub seed: u64,
    pub x: f32,
    pub z: f32,
    pub size: f32,
    pub yaw: f32,
    pub kind: SiteKind,
    pub act_idx: u64,
}

pub const FORMATION_SITES: u64 = 1200;
pub const PROP_SITES: u64 = 600;
/// Share of formation sites that grow a landform instead of a dome rock.
const LAND_SHARE: f32 = 0.45;

/// Every procedural scatter site of one act, exactly as the game spawns
/// them: 1,200 formation sites and 600 prop sites, minus those that fall in
/// the inn clearing or within `clearance` of a point of interest.
pub fn act_sites(act_idx: u64, pois: &[(f32, f32)], bounds: f32, clearance: f32) -> Vec<Site> {
    let mut out = Vec::new();
    let blocked = |x: f32, z: f32| {
        (x * x + z * z).sqrt() < 320.0
            || pois.iter().any(|(px, pz)| ((x - px).powi(2) + (z - pz).powi(2)).sqrt() < clearance)
    };
    for i in 0..FORMATION_SITES {
        let seed = act_idx * 1_000_003 + i * 7919;
        let (x, z) = if i % 3 == 0 && !pois.is_empty() {
            let (px, pz) = pois[(seed / 3 % pois.len() as u64) as usize];
            (px + (hash01(seed * 4 + 11) - 0.5) * 900.0, pz + (hash01(seed * 4 + 12) - 0.5) * 900.0)
        } else {
            ((hash01(seed * 4 + 11) - 0.5) * bounds * 2.0, (hash01(seed * 4 + 12) - 0.5) * bounds * 2.0)
        };
        if blocked(x, z) {
            continue;
        }
        let kind = if h01(seed ^ 0x1A4D) < LAND_SHARE {
            SiteKind::Land(land_family_for(act_idx, seed))
        } else {
            SiteKind::Rock
        };
        out.push(Site {
            seed,
            x,
            z,
            size: 14.0 + hash01(seed * 4 + 13) * 46.0,
            yaw: hash01(seed * 4 + 14) * 6.283,
            kind,
            act_idx,
        });
    }
    for i in 0..PROP_SITES {
        let seed = act_idx * 7_700_017 + i * 6421 + 31;
        let (x, z) = if i % 3 != 0 && !pois.is_empty() {
            let (px, pz) = pois[(seed / 7 % pois.len() as u64) as usize];
            (px + (hash01(seed * 4 + 21) - 0.5) * 1100.0, pz + (hash01(seed * 4 + 22) - 0.5) * 1100.0)
        } else {
            ((hash01(seed * 4 + 21) - 0.5) * bounds * 2.0, (hash01(seed * 4 + 22) - 0.5) * bounds * 2.0)
        };
        if blocked(x, z) {
            continue;
        }
        out.push(Site {
            seed,
            x,
            z,
            size: 1.6 + hash01(seed * 4 + 23) * 2.2,
            yaw: hash01(seed * 4 + 24) * 6.283,
            kind: SiteKind::Prop,
            act_idx,
        });
    }
    out
}

/// The mesh a site spawns.
pub fn site_mesh(s: &Site) -> Mesh {
    match s.kind {
        SiteKind::Rock => crate::variety::formation_mesh(s.seed),
        SiteKind::Land(f) => land_mesh(s.seed, f, s.act_idx),
        SiteKind::Prop => crate::propgen::prop_mesh(s.seed),
    }
}

/// Geometry fingerprint: every vertex position, quantised to a thousandth of
/// a unit, folded in order. Two meshes share it only if they are the same
/// shape vertex for vertex.
pub fn geom_hash(m: &Mesh) -> u64 {
    let mut h = 0xCBF2_9CE4_8422_2325u64;
    if let Some(p) = m.attribute(Mesh::ATTRIBUTE_POSITION).and_then(|a| a.as_float3()) {
        for v in p {
            for c in v {
                h = (h ^ ((c * 1000.0).round() as i64 as u64)).wrapping_mul(0x100_0000_01B3);
            }
        }
        h ^= p.len() as u64;
    }
    h
}

/// Coarse silhouette: bounding-box proportions in a few bins plus vertex
/// count. Two meshes with different keys differ at a glance; the reverse is
/// not claimed.
pub fn silhouette(m: &Mesh) -> (u32, [i32; 3]) {
    let Some(p) = m.attribute(Mesh::ATTRIBUTE_POSITION).and_then(|a| a.as_float3()) else {
        return (0, [0; 3]);
    };
    let mut lo = Vec3::splat(f32::MAX);
    let mut hi = Vec3::splat(f32::MIN);
    for v in p {
        lo = lo.min(Vec3::from(*v));
        hi = hi.max(Vec3::from(*v));
    }
    let d = (hi - lo).max(Vec3::splat(1e-4));
    let m = d.max_element();
    (p.len() as u32, [(d.x / m * 6.0) as i32, (d.y / m * 6.0) as i32, (d.z / m * 6.0) as i32])
}

/// Art-review sheet: one row per landform family, columns are seeds.
pub fn spawn_land_sheet(commands: &mut Commands, meshes: &mut Assets<Mesh>, materials: &mut Assets<StandardMaterial>) {
    let mat = materials.add(StandardMaterial { base_color: Color::WHITE, perceptual_roughness: 0.9, ..default() });
    let cols: u32 = std::env::var("ANTEDILUVIA_LANDSHEET_COLS").ok().and_then(|v| v.parse().ok()).unwrap_or(8);
    const SPACING: f32 = 5.2;
    info!(
        "landsheet: rows far->near: {}",
        (0..LAND_FAMILIES).map(|f| format!("{f}={}", land_family_name(f))).collect::<Vec<_>>().join(" ")
    );
    for f in 0..LAND_FAMILIES {
        for v in 0..cols {
            let seed = (f as u64 + 1).wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ (v as u64 + 1).wrapping_mul(0x2545_F491);
            commands.spawn((
                Mesh3d(meshes.add(land_mesh(seed, f, v as u64 % 5))),
                MeshMaterial3d(mat.clone()),
                Transform::from_xyz(
                    (v as f32 - (cols - 1) as f32 * 0.5) * SPACING,
                    0.0,
                    (f as f32 - (LAND_FAMILIES - 1) as f32 * 0.5) * SPACING,
                ),
            ));
        }
    }
    let ground = meshes.add(Plane3d::default().mesh().size(400.0, 400.0));
    let gmat = materials.add(StandardMaterial { base_color: Color::srgb(0.30, 0.33, 0.26), perceptual_roughness: 1.0, ..default() });
    commands.spawn((Mesh3d(ground), MeshMaterial3d(gmat)));
    commands.spawn((
        DirectionalLight { illuminance: 14_000.0, shadows_enabled: true, ..default() },
        Transform::from_xyz(30.0, 60.0, 20.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
    let envf = |k: &str, d: f32| -> f32 { std::env::var(k).ok().and_then(|v| v.parse().ok()).unwrap_or(d) };
    commands.spawn((
        Camera3d::default(),
        Transform::from_xyz(0.0, envf("ANTEDILUVIA_LANDSHEET_CAMH", 58.0), envf("ANTEDILUVIA_LANDSHEET_CAMD", 54.0))
            .looking_at(Vec3::new(0.0, 0.0, 3.0), Vec3::Y),
    ));
}

#[cfg(test)]
mod tests {
    use super::*;
    use antediluvia_protocol::Act;
    use std::collections::{HashMap, HashSet};

    fn world_sites() -> Vec<Site> {
        let mut all = Vec::new();
        for (i, act) in Act::ALL.iter().enumerate() {
            let pois: Vec<(f32, f32)> = antediluvia_sim::pois::pois_for_act(*act).map(|p| (p.x, p.y)).collect();
            all.extend(act_sites(i as u64, &pois, antediluvia_protocol::WORLD_BOUNDS, 260.0));
        }
        all
    }

    /// The headline claim: walking the sites the game really spawns, across
    /// all five acts, yields at least 2,000 terrain and prop meshes with
    /// different geometry (counted by geometry hash, not by seed).
    #[test]
    fn five_acts_hold_over_2000_distinct_terrain_and_prop_meshes() {
        let sites = world_sites();
        let mut geoms = HashSet::new();
        let mut shapes = HashSet::new();
        let mut per_kind: HashMap<String, usize> = HashMap::new();
        for s in &sites {
            let m = site_mesh(s);
            let n = m.attribute(Mesh::ATTRIBUTE_POSITION).and_then(|a| a.as_float3()).map(|p| p.len()).unwrap_or(0);
            assert!(n >= 24, "site {:?} seed {} is degenerate", s.kind, s.seed);
            geoms.insert(geom_hash(&m));
            let (label, fam) = match s.kind {
                SiteKind::Rock => ("rock".to_string(), crate::variety::formation_family(s.seed)),
                SiteKind::Land(f) => (land_family_name(f).to_string(), 100 + f),
                SiteKind::Prop => ("prop".to_string(), 200 + crate::propgen::prop_family(s.seed)),
            };
            shapes.insert((fam, silhouette(&m)));
            *per_kind.entry(label).or_default() += 1;
        }
        println!(
            "sites {}  distinct geometry {}  distinct (family, silhouette) {}  by kind {:?}",
            sites.len(),
            geoms.len(),
            shapes.len(),
            per_kind
        );
        assert!(geoms.len() >= 2000, "only {} distinct meshes in {} sites", geoms.len(), sites.len());
        // Geometry hashes differ on any jitter; the coarse key is the
        // stricter "looks different at a glance" count.
        assert!(shapes.len() >= 1000, "only {} distinct silhouettes", shapes.len());
        // All 40 families (8 rock + 12 land + 20 prop) occur somewhere.
        let fams: HashSet<u32> = shapes.iter().map(|(f, _)| *f).collect();
        assert_eq!(fams.len(), 40, "families present: {}", fams.len());
    }

    #[test]
    fn every_act_gets_every_site_kind_and_its_own_landforms() {
        for (i, act) in Act::ALL.iter().enumerate() {
            let pois: Vec<(f32, f32)> = antediluvia_sim::pois::pois_for_act(*act).map(|p| (p.x, p.y)).collect();
            let sites = act_sites(i as u64, &pois, antediluvia_protocol::WORLD_BOUNDS, 260.0);
            let land: Vec<u32> = sites.iter().filter_map(|s| if let SiteKind::Land(f) = s.kind { Some(f) } else { None }).collect();
            assert!(land.len() >= 300, "{act:?}: only {} landforms", land.len());
            assert!(sites.iter().any(|s| s.kind == SiteKind::Rock));
            assert!(sites.iter().filter(|s| s.kind == SiteKind::Prop).count() >= 300);
            // The act's favoured families outnumber any single other family.
            let count = |f: u32| land.iter().filter(|x| **x == f).count();
            let fav_min = ACT_FAVOURS[i].iter().map(|f| count(*f)).min().unwrap();
            let other_max = (0..LAND_FAMILIES).filter(|f| !ACT_FAVOURS[i].contains(f)).map(count).max().unwrap();
            assert!(fav_min > other_max, "{act:?}: favoured {fav_min} vs other {other_max}");
            // No site in the inn clearing or on a point of interest.
            for s in &sites {
                assert!((s.x * s.x + s.z * s.z).sqrt() >= 320.0);
                assert!(pois.iter().all(|(px, pz)| ((s.x - px).powi(2) + (s.z - pz).powi(2)).sqrt() >= 260.0));
            }
        }
    }

    #[test]
    fn landforms_are_valid_bounded_grounded_and_deterministic() {
        let mut names = HashSet::new();
        for f in 0..LAND_FAMILIES {
            assert!(names.insert(land_family_name(f)), "duplicate family name");
            for k in 0..24u64 {
                let seed = k * 7919 + f as u64 * 31 + 5;
                let m = land_mesh(seed, f, k % 5);
                let p = m.attribute(Mesh::ATTRIBUTE_POSITION).and_then(|a| a.as_float3()).unwrap();
                assert!(m.attribute(Mesh::ATTRIBUTE_COLOR).is_some() && m.attribute(Mesh::ATTRIBUTE_NORMAL).is_some());
                let (mut lo, mut hi) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
                for v in p {
                    assert!(v.iter().all(|c| c.is_finite()));
                    lo = lo.min(Vec3::from(*v));
                    hi = hi.max(Vec3::from(*v));
                }
                let d = hi - lo;
                assert!(d.max_element() < 9.0, "{}: {d:?} too big", land_family_name(f));
                assert!(d.min_element() > 0.15, "{}: {d:?} flat", land_family_name(f));
                assert!(lo.y < 0.2 && lo.y > -0.6, "{}: base at {}", land_family_name(f), lo.y);
                // One draw each: a landform must stay a modest mesh.
                assert!(p.len() < 9000, "{}: {} vertices", land_family_name(f), p.len());
                assert_eq!(geom_hash(&m), geom_hash(&land_mesh(seed, f, k % 5)));
            }
        }
    }

    /// Arches must be open underneath: nothing solid on the centre line below
    /// the span, or it is just another lump.
    #[test]
    fn arches_have_a_hole() {
        for k in 0..20u64 {
            let m = land_mesh(k * 131 + 3, 1, 0);
            let p = m.attribute(Mesh::ATTRIBUTE_POSITION).and_then(|a| a.as_float3()).unwrap();
            let blocked = p.iter().any(|v| v[0].abs() < 0.2 && v[1] < 0.7);
            assert!(!blocked, "arch {k} is filled in");
        }
    }
}
