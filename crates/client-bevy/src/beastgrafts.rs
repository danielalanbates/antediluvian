// Copyright (c) 2026 Daniel Bates / Bates LLC. All rights reserved.
//! Grafts for generated creature bodies: horns, back spikes, sails, tusks,
//! neck ruffs, flank plates, head frills and tail blades.
//!
//! `creaturegen` varies a body plan's proportions continuously, which makes
//! every species a different mesh but leaves two wolves-of-a-plan reading as
//! the same animal a little longer or shorter. Grafts are discrete: a species
//! either has a sail or does not, so the silhouette changes at a glance.
//! Which grafts a species grows is a pure function of its seed and plan
//! ([`graft_key`]), and the parts ride the same rig nodes as the rest of the
//! body, so they bake into the one creature mesh (still one draw).

use bevy::prelude::*;

use crate::creaturegen::{Attach, Part};
use crate::propgen::{h01, rng, Builder, Rgba};

pub const GRAFT_KINDS: u32 = 8;
pub const HORNS: u32 = 1 << 0;
pub const BACK_SPIKES: u32 = 1 << 1;
pub const SAIL: u32 = 1 << 2;
pub const TUSKS: u32 = 1 << 3;
pub const RUFF: u32 = 1 << 4;
pub const PLATES: u32 = 1 << 5;
pub const FRILL: u32 = 1 << 6;
pub const TAIL_BLADES: u32 = 1 << 7;

pub fn graft_name(bit: u32) -> &'static str {
    match bit {
        HORNS => "horns",
        BACK_SPIKES => "back_spikes",
        SAIL => "sail",
        TUSKS => "tusks",
        RUFF => "ruff",
        PLATES => "plates",
        FRILL => "frill",
        TAIL_BLADES => "tail_blades",
        _ => "unknown",
    }
}

/// Grafts that make anatomical sense on each body plan.
fn allowed(plan: u32) -> u32 {
    match plan % crate::creaturegen::CREATURE_PLANS {
        0 => HORNS | BACK_SPIKES | TUSKS | RUFF | PLATES | FRILL | TAIL_BLADES,
        1 => HORNS | BACK_SPIKES | SAIL | RUFF | TAIL_BLADES,
        2 => HORNS | BACK_SPIKES | SAIL | FRILL | TUSKS,
        3 => FRILL | RUFF | TAIL_BLADES | HORNS,
        4 => HORNS | BACK_SPIKES | PLATES | TAIL_BLADES,
        5 => BACK_SPIKES | HORNS | TAIL_BLADES | PLATES,
        6 => SAIL | BACK_SPIKES | FRILL | HORNS | TAIL_BLADES,
        7 => HORNS | TUSKS | RUFF | PLATES | BACK_SPIKES,
        8 => BACK_SPIKES | HORNS | PLATES | TAIL_BLADES,
        _ => HORNS | RUFF | FRILL | TAIL_BLADES,
    }
}

/// Which grafts a creature grows (bit mask) and, in bits 8.., the shape
/// variant of its horns (0-2) and spikes (0-2). A sail and back spikes share
/// the spine, so a sail wins.
pub fn graft_key(seed: u64, plan: u32) -> u32 {
    let ok = allowed(plan);
    let mut mask = 0;
    for g in 0..GRAFT_KINDS {
        let bit = 1 << g;
        if ok & bit != 0 && h01(seed ^ (0x6AF7_0000 + g as u64 * 977)) < 0.42 {
            mask |= bit;
        }
    }
    if mask & SAIL != 0 {
        mask &= !BACK_SPIKES;
    }
    let horn_var = (h01(seed ^ 0x6AF7_1001) * 3.0) as u32 % 3;
    let spike_var = (h01(seed ^ 0x6AF7_1002) * 3.0) as u32 % 3;
    let mut key = mask;
    if mask & HORNS != 0 {
        key |= horn_var << 8;
    }
    if mask & (BACK_SPIKES | SAIL) != 0 {
        key |= spike_var << 10;
    }
    key
}

/// Body measurements the grafts hang from (all from `creature_parts`).
pub struct Dims {
    pub body_len: f32,
    pub body_r: f32,
    pub hip_y: f32,
    pub head_at: Vec3,
    pub head_r: f32,
}

pub fn graft_parts(seed: u64, plan: u32, d: &Dims, hide: Rgba) -> Vec<Part> {
    let key = graft_key(seed, plan);
    let mut parts = Vec::new();
    let horn: Rgba = {
        let v = rng(seed, 901, 0.62, 0.86);
        [v, v * 0.95, v * 0.80, 1.0]
    };
    let dark: Rgba = [hide[0] * 0.62, hide[1] * 0.62, hide[2] * 0.62, 1.0];
    let skin: Rgba = [
        (hide[0] * 1.25 + 0.06).min(0.9),
        (hide[1] * 1.05).min(0.9),
        (hide[2] * 0.95).min(0.9),
        1.0,
    ];
    let (hr, br, len) = (d.head_r, d.body_r, d.body_len);

    // Head grafts share one mesh.
    if key & (HORNS | TUSKS | FRILL) != 0 {
        let mut b = Builder::default();
        if key & HORNS != 0 {
            let var = (key >> 8) & 3;
            let pairs = 1 + (h01(seed ^ 0x6AF7_2001) * 1.6) as u32;
            for p in 0..pairs {
                let l = hr * rng(seed, 910 + p as u64, 1.1, 2.3) * if p == 0 { 1.0 } else { 0.6 };
                for side in [-1.0f32, 1.0] {
                    let lean = match var {
                        0 => Vec3::new(side * 0.2, 0.0, 0.1) * l,
                        1 => Vec3::new(side * 0.15, 0.0, 0.9) * l,
                        _ => Vec3::new(side * 1.0, 0.0, -0.1) * l,
                    };
                    b.prism(
                        Vec3::new(side * hr * (0.42 + p as f32 * 0.2), hr * 0.55, hr * (0.1 + p as f32 * 0.35)),
                        5,
                        hr * 0.2,
                        hr * 0.02,
                        l * if var == 2 { 0.45 } else { 1.0 },
                        0.0,
                        lean,
                        0.08,
                        seed ^ (0x920 + p as u64 * 2 + (side > 0.0) as u64),
                        horn,
                    );
                }
            }
        }
        if key & TUSKS != 0 {
            let l = hr * rng(seed, 930, 0.7, 1.5);
            for side in [-1.0f32, 1.0] {
                b.prism(
                    Vec3::new(side * hr * 0.42, -hr * 0.55, -hr * 0.8),
                    5,
                    hr * 0.14,
                    hr * 0.015,
                    l * 0.7,
                    0.0,
                    Vec3::new(side * 0.15, 0.0, -0.75) * l,
                    0.06,
                    seed ^ (0x931 + (side > 0.0) as u64),
                    horn,
                );
            }
        }
        if key & FRILL != 0 {
            let n = 5 + (h01(seed ^ 0x6AF7_2002) * 4.0) as u32;
            let l = hr * rng(seed, 940, 1.0, 1.9);
            for i in 0..n {
                let a = std::f32::consts::PI * (i as f32 / (n - 1) as f32) - std::f32::consts::FRAC_PI_2;
                b.prism(
                    Vec3::new(a.sin() * hr * 0.6, hr * 0.25 + a.cos() * hr * 0.3, hr * 0.75),
                    4,
                    hr * 0.22,
                    hr * 0.05,
                    l * a.cos().max(0.35),
                    0.78,
                    Vec3::new(a.sin() * l * 0.8, 0.0, l * 0.35),
                    0.05,
                    seed ^ (0x941 + i as u64),
                    if i % 2 == 0 { skin } else { dark },
                );
            }
        }
        parts.push(Part { attach: Attach::Head, xf: Transform::from_translation(d.head_at), mesh: b.finish() });
    }

    // Spine grafts share one mesh.
    if key & (BACK_SPIKES | SAIL | RUFF | PLATES) != 0 {
        let mut b = Builder::default();
        let var = (key >> 10) & 3;
        if key & BACK_SPIKES != 0 {
            let n = 4 + (h01(seed ^ 0x6AF7_2003) * 5.0) as u32;
            for i in 0..n {
                let t = i as f32 / (n - 1) as f32;
                let z = -len * 0.4 + t * len * 0.85;
                let h = br * rng(seed, 950 + i as u64, 0.55, 1.15) * (1.0 - (t - 0.4).abs() * 0.7)
                    * if var == 1 { 1.5 } else { 1.0 };
                let r = br * if var == 2 { 0.3 } else { 0.17 };
                b.prism(
                    Vec3::new(0.0, br * 0.8, z),
                    if var == 2 { 4 } else { 5 },
                    r,
                    r * 0.06,
                    h,
                    0.78,
                    Vec3::new(0.0, 0.0, h * 0.45),
                    0.08,
                    seed ^ (0x951 + i as u64),
                    horn,
                );
            }
        }
        if key & SAIL != 0 {
            let n = 7u32;
            let peak = br * rng(seed, 960, 1.3, 2.4);
            let skew = match var {
                0 => 0.5,
                1 => 0.25,
                _ => 0.75,
            };
            for i in 0..n {
                let t = (i as f32 + 0.5) / n as f32;
                let z = -len * 0.42 + t * len * 0.84;
                // Asymmetric hump: the peak sits at `skew` along the back.
                let u = if t < skew { t / skew } else { (1.0 - t) / (1.0 - skew) };
                let h = peak * (0.25 + 0.75 * (u * std::f32::consts::FRAC_PI_2).sin());
                b.boxy(Vec3::new(0.0, br * 0.7, z), br * 0.09, h, len * 0.84 / n as f32 * 1.02, 0.0, seed ^ (0x961 + i as u64), if i % 2 == 0 { skin } else { dark });
            }
        }
        if key & RUFF != 0 {
            let n = 9u32;
            let rr = br * rng(seed, 970, 0.26, 0.42);
            for i in 0..n {
                // Upper three quarters of a ring around the neck.
                let a = -0.8 + (std::f32::consts::PI + 1.6) * i as f32 / (n - 1) as f32;
                b.blob(
                    Vec3::new(a.cos() * br * 1.0, a.sin() * br * 0.95, -len * 0.42),
                    Vec3::new(rr, rr * 1.25, rr * 1.1),
                    3,
                    6,
                    0.18,
                    seed ^ (0x971 + i as u64),
                    if i % 2 == 0 { dark } else { hide },
                );
            }
        }
        if key & PLATES != 0 {
            let n = 3u32;
            for i in 0..n {
                let z = -len * 0.28 + i as f32 * len * 0.28;
                for side in [-1.0f32, 1.0] {
                    b.blob(
                        Vec3::new(side * br * 0.98, br * 0.15, z),
                        Vec3::new(br * 0.14, br * 0.46, len * 0.15),
                        3,
                        6,
                        0.1,
                        seed ^ (0x981 + i as u64 * 2 + (side > 0.0) as u64),
                        dark,
                    );
                }
            }
        }
        parts.push(Part { attach: Attach::Spine, xf: Transform::from_xyz(0.0, d.hip_y, 0.0), mesh: b.finish() });
    }

    if key & TAIL_BLADES != 0 {
        let mut b = Builder::default();
        let n = 2 + (h01(seed ^ 0x6AF7_2004) * 2.5) as u32;
        for i in 0..n {
            let h = br * rng(seed, 990 + i as u64, 0.5, 1.0);
            for side in [-1.0f32, 1.0] {
                b.prism(
                    Vec3::new(side * br * 0.12, br * 0.2, len * (0.04 + i as f32 * 0.07)),
                    4,
                    br * 0.16,
                    br * 0.01,
                    h,
                    0.78,
                    Vec3::new(side * h * 0.7, 0.0, h * 0.4),
                    0.06,
                    seed ^ (0x991 + i as u64 * 2 + (side > 0.0) as u64),
                    horn,
                );
            }
        }
        parts.push(Part { attach: Attach::Tail, xf: Transform::from_xyz(0.0, d.hip_y, d.body_len * 0.5), mesh: b.finish() });
    }
    parts
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::creaturegen::CREATURE_PLANS;

    #[test]
    fn every_graft_is_reachable_and_named() {
        let mut seen = 0u32;
        let mut keys = std::collections::HashSet::new();
        for plan in 0..CREATURE_PLANS {
            for k in 0..400u64 {
                let key = graft_key(k * 7919 + 1, plan);
                assert_eq!(key & 0xFF & !allowed(plan), 0, "plan {plan} grew a graft it may not have");
                assert!(key & SAIL == 0 || key & BACK_SPIKES == 0, "sail and spikes together");
                seen |= key & 0xFF;
                keys.insert((plan, key));
            }
        }
        assert_eq!(seen, 0xFF, "unreachable grafts: {seen:#b}");
        let names: std::collections::HashSet<_> = (0..GRAFT_KINDS).map(|g| graft_name(1 << g)).collect();
        assert_eq!(names.len(), GRAFT_KINDS as usize);
        assert!(keys.len() >= 300, "only {} (plan, graft) loadouts", keys.len());
    }

    #[test]
    fn grafts_add_geometry_only_when_selected() {
        let d = Dims { body_len: 1.0, body_r: 0.3, hip_y: 0.7, head_at: Vec3::new(0.0, 0.9, -0.7), head_r: 0.2 };
        for plan in 0..CREATURE_PLANS {
            for k in 0..60u64 {
                let seed = k * 131 + 9;
                let parts = graft_parts(seed, plan, &d, [0.4, 0.3, 0.2, 1.0]);
                let key = graft_key(seed, plan) & 0xFF;
                assert_eq!(parts.is_empty(), key == 0, "plan {plan} seed {seed}");
                for p in &parts {
                    let v = p.mesh.attribute(Mesh::ATTRIBUTE_POSITION).and_then(|a| a.as_float3()).unwrap();
                    assert!(!v.is_empty() && v.iter().all(|x| x.iter().all(|c| c.is_finite())));
                }
            }
        }
    }
}
