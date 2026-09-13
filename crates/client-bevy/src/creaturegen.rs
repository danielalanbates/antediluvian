//! Procedural creature bodies — hundreds of distinct beasts from code.
//!
//! The bestiary's weakest link is that every mob in the world is one of ~10
//! downloaded rigs. `variety::attach_species_parts` already grafts unique
//! horns, crests and ridges onto those rigs, and `species_stretch` squashes
//! them per-axis, but the BODY underneath is always the same handful of
//! silhouettes — and the CC0 sources that would fix it are all gated
//! (Quaternius behind itch.io, poly.pizza behind a key). So we do here what
//! `propgen` did for props: generate the body itself.
//!
//! Design notes:
//! - A creature is emitted as a list of [`Part`]s, each carrying the rig node
//!   it belongs to ([`Attach`]) and its transform relative to that node. That
//!   is what lets a generated body ride an EXISTING skeleton: parent each part
//!   to the matching bone and the source rig's walk cycle animates it for
//!   free. Generating an animated skeleton from scratch is the thing we are
//!   deliberately not doing.
//! - [`creature_mesh`] bakes every part into one mesh in the rest pose. That
//!   is what the contact sheet and the tests judge, and it is also what a
//!   static prop-like use (trophies, corpses, statues) would want.
//! - Geometry and colour follow `propgen`'s rules exactly: shared
//!   [`propgen::Builder`] primitives, colour in VERTEX attributes so the whole
//!   set batches under one material, faceted flat normals.

use bevy::prelude::*;
use bevy::render::mesh::{Mesh, VertexAttributeValues};
use bevy::render::render_asset::RenderAssetUsages;
use bevy::render::mesh::PrimitiveTopology;

use crate::propgen::{h01, rng, Builder, Rgba};

/// Number of distinct body plans. Bump this and extend `plan_name` +
/// `build_plan` together; the name test enforces the pairing.
pub const CREATURE_PLANS: u32 = 10;

/// Which rig node a generated part rides. The names mirror the bone search in
/// `variety::attach_species_parts`, which is the only naming convention the
/// downloaded GLBs reliably share.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Attach {
    /// Torso/thorax — the spine, chest or body node.
    Spine,
    /// Skull, jaw, antennae, beak.
    Head,
    /// Tail, abdomen, stinger.
    Tail,
    /// A limb. `front` picks shoulder vs hip; `left` mirrors it. Rigs that
    /// expose no limb bones fall back to hanging these off the spine, which
    /// still reads correctly because limbs are placed in spine-local space.
    Limb { front: bool, left: bool },
}

/// One piece of a generated creature, in the local space of its [`Attach`]
/// node. `xf` is the rest-pose placement relative to that node.
pub struct Part {
    pub attach: Attach,
    pub xf: Transform,
    pub mesh: Mesh,
}

/// Which plan a seed selects.
pub fn creature_plan(seed: u64) -> u32 {
    (h01(seed ^ 0xC0FF_EE00) * CREATURE_PLANS as f32) as u32 % CREATURE_PLANS
}

/// Human-readable plan name, used to label the contact sheet. Keep in step
/// with `build_plan`'s match arms — the index is what
/// `ANTEDILUVIA_BEASTSHEET_PLAN` takes, so a wrong name here sends you
/// inspecting the wrong plan.
pub fn plan_name(p: u32) -> &'static str {
    match p % CREATURE_PLANS {
        0 => "quadruped_heavy",
        1 => "quadruped_light",
        2 => "serpentine",
        3 => "avian",
        4 => "insectoid",
        5 => "arachnid",
        6 => "amphibian",
        7 => "biped_brute",
        8 => "crustacean",
        9 => "chiropteran",
        _ => "unknown",
    }
}

// ---------------------------------------------------------------------------
// Palettes
// ---------------------------------------------------------------------------
//
// Authored deliberately desaturated: the scene camera applies
// post_saturation 1.32 plus a warm bounce fill, so anything authored as a
// convincing "brown" grades to bright orange on screen. Same lesson propgen's
// `wood()` learned the hard way — judge these in-game, never in isolation.

fn hide(seed: u64) -> Rgba {
    let v = rng(seed, 31, 0.18, 0.38);
    [v * 1.04, v * 0.94, v * 0.82, 1.0]
}

fn chitin(seed: u64) -> Rgba {
    // Dark, faintly cool — beetles and crabs read wrong when warm.
    let v = rng(seed, 37, 0.10, 0.26);
    [v * 0.92, v * 0.98, v * 1.10, 1.0]
}

fn scale(seed: u64) -> Rgba {
    match (h01(seed ^ 0x5CA1) * 4.0) as u32 {
        0 => [0.22, 0.34, 0.26, 1.0], // swamp green
        1 => [0.26, 0.28, 0.36, 1.0], // slate
        2 => [0.34, 0.26, 0.24, 1.0], // oxblood
        _ => [0.30, 0.32, 0.22, 1.0], // olive
    }
}

fn feather(seed: u64) -> Rgba {
    let v = rng(seed, 41, 0.22, 0.44);
    match (h01(seed ^ 0xFEA7) * 3.0) as u32 {
        0 => [v * 1.05, v * 0.90, v * 0.72, 1.0], // tawny
        1 => [v * 0.80, v * 0.86, v * 0.96, 1.0], // ash
        _ => [v * 0.72, v * 0.80, v * 0.74, 1.0], // moss
    }
}

fn membrane(seed: u64) -> Rgba {
    // Thin skin over bone: pinker and lighter than the body it hangs from.
    let v = rng(seed, 43, 0.26, 0.42);
    [v * 1.18, v * 0.86, v * 0.88, 1.0]
}

/// The body palette a plan wears.
fn plan_palette(plan: u32, seed: u64) -> Rgba {
    match plan % CREATURE_PLANS {
        4 | 5 | 8 => chitin(seed),
        2 | 6 => scale(seed),
        3 => feather(seed),
        9 => membrane(seed),
        _ => hide(seed),
    }
}

// ---------------------------------------------------------------------------
// Limb / segment helpers
// ---------------------------------------------------------------------------

/// A jointed limb hanging DOWN from its origin: `segs` tapering sections, each
/// kinked by `splay` so knees and elbows read at a glance instead of the limb
/// being one straight pipe.
fn limb_mesh(seed: u64, len: f32, girth: f32, segs: u32, splay: f32, c: Rgba) -> Mesh {
    let mut b = Builder::default();
    let seg_len = len / segs.max(1) as f32;
    let mut at = Vec3::ZERO;
    for s in 0..segs.max(1) {
        let t = s as f32 / segs.max(1) as f32;
        let r0 = girth * (1.0 - t * 0.45);
        let r1 = girth * (1.0 - (t + 1.0 / segs.max(1) as f32) * 0.45);
        // Alternate the kink direction so the joint zig-zags like a real leg.
        let dir = if s % 2 == 0 { 1.0 } else { -1.0 };
        let lean = Vec3::new(0.0, 0.0, dir * splay * seg_len);
        // Build each segment as a downward prism: place it, then step down.
        b.prism(
            at - Vec3::Y * seg_len,
            5,
            r1.max(0.004),
            r0.max(0.004),
            seg_len,
            h01(seed ^ (s as u64 + 11)) * 6.28,
            lean,
            0.06,
            seed ^ (s as u64 * 7 + 3),
            c,
        );
        at += Vec3::new(lean.x, -seg_len, lean.z);
    }
    // Foot: a squat pad so the limb doesn't end in a needle point.
    b.blob(at, Vec3::new(girth * 1.5, girth * 0.7, girth * 2.1), 3, 6, 0.12, seed ^ 0x0F00, c);
    b.finish()
}

/// A tapering chain of body segments along -Z (serpent bodies, insect
/// abdomens, tails). Returns the mesh; the caller places it.
fn segment_chain(seed: u64, count: u32, len: f32, r0: f32, r1: f32, sag: f32, c: Rgba) -> Mesh {
    let mut b = Builder::default();
    let step = len / count.max(1) as f32;
    for s in 0..count.max(1) {
        let t = s as f32 / count.max(1) as f32;
        let r = r0 + (r1 - r0) * t;
        let z = -(s as f32) * step;
        // Sag droops the chain as it extends — a tail that stays perfectly
        // level reads as a broomstick.
        let y = -sag * t * t * len;
        b.blob(
            Vec3::new(0.0, y, z),
            Vec3::new(r, r * 0.92, step * 0.72),
            3,
            7,
            0.10,
            seed ^ (s as u64 * 29 + 5),
            c,
        );
    }
    b.finish()
}

/// A flattened membrane panel (bat wing, fin) spanning outward in +X.
/// `zsign` mirrors the sweep for a wing that is then yawed 180° onto the other
/// side (a yaw flips Z as well as X, and a negative scale would flip winding).
fn membrane_mesh(seed: u64, span: f32, chord: f32, zsign: f32, c: Rgba) -> Mesh {
    let mut b = Builder::default();
    // Finger struts, with skin stretched between them as thin boxes.
    let fingers = 3 + (h01(seed ^ 0x3313) * 2.0) as u32;
    for f in 0..fingers {
        let t = f as f32 / (fingers - 1).max(1) as f32;
        let ang = -0.35 + t * 1.3;
        let l = span * (0.72 + h01(seed ^ (f as u64 + 61)) * 0.36);
        // Prisms only extrude along +Y, so a strut built from one stands
        // straight up out of the back. Lay each strut as a tapering bead
        // chain along its real direction instead: outward, swept back.
        let dir = Vec3::new(ang.cos(), 0.08, zsign * ang.sin() * 0.6).normalize();
        const BEADS: u32 = 7;
        for k in 0..BEADS {
            let u = (k as f32 + 0.5) / BEADS as f32;
            let r = chord * (0.05 - u * 0.03);
            b.blob(
                dir * l * u,
                Vec3::new(l / BEADS as f32 * 0.62, r, r),
                2,
                5,
                0.0,
                seed ^ (f as u64 * 13 + 7 + k as u64),
                c,
            );
        }
    }
    // The skin itself: one thin slab, warped so it isn't a flat card.
    b.blob(
        Vec3::new(span * 0.42, 0.0, zsign * chord * 0.22),
        Vec3::new(span * 0.5, span * 0.035, chord * 0.55),
        3,
        7,
        0.16,
        seed ^ 0x5C1B,
        c,
    );
    b.finish()
}

// ---------------------------------------------------------------------------
// Plans
// ---------------------------------------------------------------------------

/// How many limbs a plan grows, and where they sit. `(front_pairs,
/// back_pairs)` — a pair is mirrored left/right.
fn limb_layout(plan: u32) -> (u32, u32) {
    match plan % CREATURE_PLANS {
        0 | 1 | 6 => (1, 1), // quadrupeds, amphibian
        2 => (0, 0),         // serpentine
        3 | 7 => (0, 1),     // avian, biped — arms/wings handled separately
        4 => (1, 2),         // insectoid: 6 legs
        5 => (2, 2),         // arachnid: 8 legs
        8 => (1, 2),         // crustacean
        9 => (0, 1),         // chiropteran
        _ => (1, 1),
    }
}

/// Build every part of one creature, in rest pose. The creature faces -Z and
/// stands on y = 0, matching the source rigs' convention.
#[allow(clippy::too_many_arguments)]
pub fn creature_parts(seed: u64, plan: u32) -> Vec<Part> {
    let plan = plan % CREATURE_PLANS;
    let c = plan_palette(plan, seed);
    let mut parts = Vec::new();

    // Overall proportions, jittered per seed so no two of a plan match.
    let scale_all = 0.8 + h01(seed ^ 0x11) * 0.6;
    let (body_len, body_r, leg_len) = match plan {
        0 => (1.05, 0.34, 0.46),  // heavy quadruped
        1 => (0.95, 0.22, 0.66),  // light quadruped
        2 => (1.9, 0.19, 0.0),    // serpent
        3 => (0.62, 0.26, 0.62),  // avian
        4 => (1.0, 0.20, 0.40),   // insectoid
        5 => (0.72, 0.30, 0.52),  // arachnid
        6 => (0.86, 0.32, 0.30),  // amphibian
        7 => (0.86, 0.38, 0.60),  // biped brute
        8 => (0.80, 0.34, 0.26),  // crustacean
        _ => (0.56, 0.20, 0.34),  // chiropteran
    };
    let body_len = body_len * scale_all * (0.85 + h01(seed ^ 0x12) * 0.35);
    let body_r = body_r * scale_all * (0.82 + h01(seed ^ 0x13) * 0.42);
    let leg_len = leg_len * scale_all * (0.82 + h01(seed ^ 0x14) * 0.42);
    // Standing height — everything hangs off this so legs reach the ground.
    let hip_y = leg_len + body_r * 0.9;

    // ── Torso ────────────────────────────────────────────────────────────
    {
        let mut b = Builder::default();
        let segs = match plan {
            2 => 9,          // serpent: a long chain IS the body
            4 | 5 | 8 => 3,  // segmented arthropods
            _ => 4,
        };
        let step = body_len / segs as f32;
        for s in 0..segs {
            let t = s as f32 / (segs - 1).max(1) as f32;
            // Torso profile: fattest at the chest for vertebrates, at the
            // rear for arthropods (abdomen-heavy).
            let bulge = match plan {
                4 | 5 | 8 => 0.55 + t * 0.75,
                2 => 1.0 - (t - 0.35).abs() * 0.5,
                _ => 1.05 - t * 0.35,
            };
            let r = body_r * bulge;
            b.blob(
                Vec3::new(0.0, 0.0, -(s as f32) * step + body_len * 0.5),
                Vec3::new(r * (0.9 + h01(seed ^ (s as u64 + 71)) * 0.3), r, step * 0.78),
                4,
                8,
                0.10,
                seed ^ (s as u64 * 31 + 17),
                c,
            );
        }
        // Carapace/shell for the armoured plans — a low dome over the back.
        if matches!(plan, 5 | 8) {
            b.blob(
                Vec3::new(0.0, body_r * 0.45, 0.0),
                Vec3::new(body_r * 1.45, body_r * 0.9, body_len * 0.52),
                4,
                9,
                0.09,
                seed ^ 0x5E11,
                c,
            );
        }
        parts.push(Part {
            attach: Attach::Spine,
            xf: Transform::from_xyz(0.0, hip_y, 0.0),
            mesh: b.finish(),
        });
    }

    // ── Head ─────────────────────────────────────────────────────────────
    {
        let mut b = Builder::default();
        let hr = body_r * (0.62 + h01(seed ^ 0x21) * 0.45);
        // Skull.
        b.blob(Vec3::ZERO, Vec3::new(hr * 0.86, hr, hr * 1.15), 4, 8, 0.10, seed ^ 0x22, c);
        match plan {
            3 => {
                // Beak: a forward cone.
                b.prism(
                    Vec3::new(0.0, -hr * 0.1, -hr * 0.9),
                    4,
                    hr * 0.42,
                    0.008,
                    hr * 1.5,
                    0.7,
                    Vec3::new(0.0, -hr * 0.25, -hr * 1.1),
                    0.03,
                    seed ^ 0x23,
                    c,
                );
            }
            4 | 5 => {
                // Mandibles + antennae.
                for side in [-1.0f32, 1.0] {
                    b.prism(
                        Vec3::new(side * hr * 0.35, -hr * 0.2, -hr * 0.7),
                        4,
                        hr * 0.16,
                        0.006,
                        hr * 0.9,
                        0.0,
                        Vec3::new(-side * hr * 0.3, 0.0, -hr * 0.5),
                        0.05,
                        seed ^ (0x24 + side.to_bits() as u64),
                        c,
                    );
                    b.prism(
                        Vec3::new(side * hr * 0.3, hr * 0.5, -hr * 0.35),
                        4,
                        hr * 0.06,
                        0.004,
                        hr * 1.6,
                        0.0,
                        Vec3::new(side * hr * 0.9, hr * 0.5, -hr * 0.6),
                        0.08,
                        seed ^ (0x25 + side.to_bits() as u64),
                        c,
                    );
                }
            }
            _ => {
                // Muzzle/snout of seeded length — the main head-shape lever.
                let snout = 0.5 + h01(seed ^ 0x26) * 1.3;
                b.blob(
                    Vec3::new(0.0, -hr * 0.22, -hr * (0.7 + snout * 0.5)),
                    Vec3::new(hr * 0.52, hr * 0.46, hr * snout * 0.8),
                    3,
                    7,
                    0.10,
                    seed ^ 0x27,
                    c,
                );
                // Lower jaw, slightly open, so the head reads as a face.
                b.blob(
                    Vec3::new(0.0, -hr * 0.5, -hr * (0.6 + snout * 0.4)),
                    Vec3::new(hr * 0.42, hr * 0.2, hr * snout * 0.7),
                    3,
                    6,
                    0.10,
                    seed ^ 0x28,
                    c,
                );
            }
        }
        // Neck length varies hugely between plans and is the other big
        // silhouette lever (think giraffe vs boar).
        let neck = match plan {
            2 => body_len * 0.10,
            3 => body_r * (1.6 + h01(seed ^ 0x29) * 2.4),
            _ => body_r * (0.5 + h01(seed ^ 0x29) * 1.7),
        };
        let (head_y, head_z) = match plan {
            // Brutes carry the head low on the shoulders, not on a stalk.
            7 => (hip_y + body_r * 0.75 + hr * 0.4, -body_len * 0.42),
            3 => (hip_y + neck * 0.9 + body_r * 0.5, -(body_len * 0.5 + neck * 0.5)),
            _ => (hip_y + neck * 0.35, -(body_len * 0.5 + neck * 0.5)),
        };
        parts.push(Part {
            attach: Attach::Head,
            xf: Transform::from_xyz(0.0, head_y, head_z),
            mesh: b.finish(),
        });
        // Neck: the head used to be placed a neck-length away with nothing
        // built in between, so long-necked seeds had a visibly floating head.
        // A tapering bead chain from the front of the torso to the skull.
        let from = Vec3::new(0.0, hip_y + body_r * 0.25, -body_len * 0.38);
        let to = Vec3::new(0.0, head_y - hr * 0.3, head_z + hr * 0.3);
        let gap = to - from;
        if gap.length() > hr * 0.4 {
            let mut nb = Builder::default();
            let n = ((gap.length() / (hr * 0.45)).ceil() as u32).clamp(2, 10);
            for k in 0..=n {
                let u = k as f32 / n as f32;
                let r = body_r * (0.55 - u * 0.2).max(0.2);
                nb.blob(gap * u, Vec3::splat(r), 3, 6, 0.08, seed ^ (0x2A0 + k as u64), c);
            }
            parts.push(Part {
                attach: Attach::Head,
                xf: Transform::from_translation(from),
                mesh: nb.finish(),
            });
        }
    }

    // ── Tail / abdomen ───────────────────────────────────────────────────
    let tail_len = match plan {
        2 => body_len * 1.15,
        1 => body_len * (0.5 + h01(seed ^ 0x31) * 0.7),
        5 | 8 => body_len * 0.25,
        3 | 9 => body_len * 0.25,
        _ => body_len * (0.25 + h01(seed ^ 0x31) * 0.6),
    };
    if tail_len > 0.06 {
        let count = 3 + (tail_len * 4.0) as u32;
        // Sag is a fraction of tail length, so a long tail on a low body (the
        // serpent especially, whose hips sit at ~0.17) droops straight through
        // the terrain. Cap the total drop at the height we hang from.
        let want = 0.35 + h01(seed ^ 0x32) * 0.5;
        let sag = want.min((hip_y * 0.8 / tail_len).max(0.0));
        parts.push(Part {
            attach: Attach::Tail,
            xf: Transform::from_xyz(0.0, hip_y, body_len * 0.5),
            mesh: segment_chain(
                seed ^ 0x7A11,
                count.min(10),
                tail_len,
                body_r * 0.5,
                body_r * 0.08,
                sag,
                c,
            ),
        });
    }

    // ── Limbs ────────────────────────────────────────────────────────────
    let (front_pairs, back_pairs) = limb_layout(plan);
    let girth = body_r * (0.20 + h01(seed ^ 0x41) * 0.16);
    let splay = match plan {
        6 | 5 | 4 | 8 => 0.55 + h01(seed ^ 0x42) * 0.5, // sprawling gait
        _ => 0.16 + h01(seed ^ 0x42) * 0.22,            // upright gait
    };
    let joints = match plan {
        4 | 5 | 8 => 3,
        _ => 2,
    };
    for (front, pairs) in [(true, front_pairs), (false, back_pairs)] {
        for i in 0..pairs {
            // Spread multiple pairs along the body so six/eight legs don't
            // stack on one hip.
            let spread = if pairs > 1 { i as f32 / (pairs - 1) as f32 - 0.5 } else { 0.0 };
            let z = if front {
                -body_len * (0.30 + spread * 0.22)
            } else {
                body_len * (0.30 + spread * 0.30)
            };
            // Front limbs ride a higher or lower shoulder than the hip (a
            // hunched back vs a level one), and the limb's LENGTH is derived
            // from that attach height so the foot still lands on y=0. Deriving
            // length independently is what made every creature float by
            // body_r*0.9 — the quadrupeds only passed the ground test by
            // squeaking under its tolerance.
            let attach_y = hip_y * if front { 0.88 + h01(seed ^ 0x43) * 0.3 } else { 1.0 };
            let l = (attach_y - girth * 0.6).max(0.05);
            for left in [true, false] {
                let side: f32 = if left { -1.0 } else { 1.0 };
                let m = limb_mesh(
                    seed ^ (0x100 + i as u64 * 7 + front as u64 * 3 + left as u64),
                    l,
                    girth,
                    joints,
                    splay * side,
                    c,
                );
                parts.push(Part {
                    attach: Attach::Limb { front, left },
                    xf: Transform::from_xyz(side * body_r * 0.85, attach_y, z),
                    mesh: m,
                });
            }
        }
    }

    // ── Wings / arms ─────────────────────────────────────────────────────
    if matches!(plan, 3 | 9) {
        let span = body_len * if plan == 9 { 2.3 } else { 1.4 } * (0.8 + h01(seed ^ 0x51) * 0.5);
        for left in [true, false] {
            let side = if left { -1.0 } else { 1.0 };
            let m = membrane_mesh(seed ^ (0x900 + left as u64), span, body_len * 0.7, if left { -1.0 } else { 1.0 }, c);
            parts.push(Part {
                attach: Attach::Limb { front: true, left },
                xf: Transform::from_xyz(side * body_r * 0.8, hip_y + body_r * 0.45, 0.0)
                    .with_rotation(Quat::from_rotation_y(if left { std::f32::consts::PI } else { 0.0 }))
                    .with_scale(Vec3::new(1.0, 1.0, 1.0)),
                mesh: m,
            });
        }
    } else if plan == 7 {
        // Brute arms: long, heavy, knuckle-low.
        for left in [true, false] {
            let side = if left { -1.0 } else { 1.0 };
            let m = limb_mesh(
                seed ^ (0xA00 + left as u64),
                leg_len * (0.9 + h01(seed ^ 0x52) * 0.4),
                girth * 1.25,
                2,
                0.2 * side,
                c,
            );
            parts.push(Part {
                attach: Attach::Limb { front: true, left },
                xf: Transform::from_xyz(side * body_r * 1.05, hip_y + body_r * 0.55, -body_len * 0.2),
                mesh: m,
            });
        }
    }

    parts
}

/// Every part baked into one mesh in the rest pose — what the contact sheet
/// and the tests judge.
pub fn creature_mesh(seed: u64) -> Mesh {
    creature_mesh_plan(seed, creature_plan(seed))
}

/// As [`creature_mesh`], for an explicitly chosen plan.
pub fn creature_mesh_plan(seed: u64, plan: u32) -> Mesh {
    let parts = creature_parts(seed, plan);
    let mut pos: Vec<[f32; 3]> = Vec::new();
    let mut col: Vec<[f32; 4]> = Vec::new();
    for p in &parts {
        let Some(vp) = p.mesh.attribute(Mesh::ATTRIBUTE_POSITION).and_then(|a| a.as_float3()) else {
            continue;
        };
        let vc = match p.mesh.attribute(Mesh::ATTRIBUTE_COLOR) {
            Some(VertexAttributeValues::Float32x4(v)) => Some(v),
            _ => None,
        };
        for (i, v) in vp.iter().enumerate() {
            let w = p.xf.transform_point(Vec3::from(*v));
            pos.push([w.x, w.y, w.z]);
            col.push(vc.and_then(|c| c.get(i).copied()).unwrap_or([1.0, 1.0, 1.0, 1.0]));
        }
    }
    let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, pos);
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, col);
    // Parts arrive already de-indexed (Builder::finish duplicates vertices),
    // so concatenating them yields a valid unindexed triangle soup.
    mesh.compute_flat_normals();
    mesh
}

// ---------------------------------------------------------------------------
// In-world use
// ---------------------------------------------------------------------------

/// Species keywords → body plan. Checked in order, so the more specific or
/// more visually defining word must come first ("leviathan" is a serpent even
/// when it is also "abyssal", a "crawler" is an insect even when "giant").
const PLAN_KEYWORDS: &[(&[&str], u32)] = &[
    (&["spider", "scorpion", "arachnid", "tarantula"], 5),
    (&["crab", "lobster", "crustacean", "tortoise", "turtle"], 8),
    (&["bat", "pterosaur", "pteranodon", "wyvern"], 9),
    (&["serpent", "snake", "wyrm", "eel", "leviathan", "worm", "basilisk", "viper"], 2),
    (&["eagle", "bird", "roc", "vulture", "owl", "hawk", "condor", "raven", "falcon"], 3),
    (&["crawler", "beetle", "mantis", "locust", "scarab", "centipede", "wasp", "ant"], 4),
    (&["crocodile", "salamander", "newt", "frog", "toad", "lizard", "gecko"], 6),
    (&["megatherium", "ape", "gorilla", "sloth", "troll", "ogre", "yeti"], 7),
    (&["chimera", "elasmotherium", "rhino", "manticore"], 0),
];

/// The body plan a species' own name demands, if any. These win over the
/// animal rigs: a "Pterosaur Behemoth-Rider Mount" is a pterosaur, even though
/// "behemoth" alone would pick the bull rig.
pub fn named_plan(tag: &str) -> Option<u32> {
    let t = tag.to_ascii_lowercase();
    // Whole-word match on `_`-separated tags: "ant" must not fire on "giant".
    let words: Vec<&str> = t.split(|c: char| !c.is_ascii_alphanumeric()).collect();
    for (keys, plan) in PLAN_KEYWORDS {
        if keys.iter().any(|k| words.iter().any(|w| w == k)) {
            // "Crawler" is ambiguous between six and eight legs; split the
            // species between them so both plans populate the world.
            if *plan == 4 && words.contains(&"crawler") && species_seed(&t) % 2 == 0 {
                return Some(5);
            }
            return Some(*plan);
        }
    }
    None
}

/// Body plan for a bestiary species: its name first, else a stable hash onto
/// the generic-monster plans (quadrupeds and brutes read best as "unknown").
pub fn plan_for_species(tag: &str) -> u32 {
    named_plan(tag).unwrap_or_else(|| [0u32, 1, 7][(species_seed(&tag.to_ascii_lowercase()) % 3) as usize])
}

/// Stable per-species seed (FNV-1a) — the same tag always builds the same body.
pub fn species_seed(tag: &str) -> u64 {
    tag.bytes().fold(0xCBF2_9CE4_8422_2325u64, |h, b| (h ^ b as u64).wrapping_mul(0x100_0000_01B3))
}

/// A body waiting for its mesh. `spawn_visual` has no mesh-asset access, so it
/// drops this marker and [`build_proc_bodies`] fills it in the same frame.
#[derive(Component)]
pub struct ProcBodyPending {
    pub seed: u64,
    pub plan: u32,
}

/// One cached mesh per (seed, plan): a zone full of the same species shares a
/// single GPU buffer, and a species re-entering AoI never regenerates.
#[derive(Resource, Default)]
pub struct ProcBodyCache {
    meshes: std::collections::HashMap<(u64, u32), Handle<Mesh>>,
    material: Option<Handle<StandardMaterial>>,
}

pub fn build_proc_bodies(
    mut commands: Commands,
    mut cache: ResMut<ProcBodyCache>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    pending: Query<(Entity, &ProcBodyPending)>,
) {
    for (ent, p) in &pending {
        let mat = cache
            .material
            .get_or_insert_with(|| {
                materials.add(StandardMaterial {
                    base_color: Color::WHITE,
                    perceptual_roughness: 0.85,
                    ..default()
                })
            })
            .clone();
        let mesh = cache
            .meshes
            .entry((p.seed, p.plan))
            .or_insert_with(|| meshes.add(creature_mesh_plan(p.seed, p.plan)))
            .clone();
        commands
            .entity(ent)
            .remove::<ProcBodyPending>()
            .insert((Mesh3d(mesh), MeshMaterial3d(mat)));
    }
}

/// Code-driven locomotion for generated bodies, which have no skeleton or
/// clips: a stride bob and a side-to-side roll while the root is moving, and a
/// topple onto the flank after death. `base` is the spawn transform the
/// offsets are applied on top of.
#[derive(Component)]
pub struct ProcGait {
    pub root: Entity,
    pub base: Transform,
    pub last: Vec3,
    pub phase: f32,
    pub speed: f32,
    /// World units of bob at full stride.
    pub bob: f32,
    pub died_at: Option<f32>,
}

impl ProcGait {
    pub fn new(root: Entity, base: Transform, start: Vec3) -> Self {
        Self { root, base, last: start, phase: 0.0, speed: 0.0, bob: base.scale.y * 0.06, died_at: None }
    }
}

/// Marks a gait body as dead; inserted from the combat-event handler.
#[derive(Component)]
pub struct ProcDied;

pub fn animate_proc_gait(
    time: Res<Time>,
    mut bodies: Query<(&mut ProcGait, &mut Transform, Option<&ProcDied>)>,
    roots: Query<&Transform, Without<ProcGait>>,
) {
    let dt = time.delta_secs().max(1e-4);
    let now = time.elapsed_secs();
    for (mut g, mut t, died) in &mut bodies {
        if died.is_some() && g.died_at.is_none() {
            g.died_at = Some(now);
        }
        if let Some(at) = g.died_at {
            let k = ((now - at) / 0.6).clamp(0.0, 1.0);
            let ease = 1.0 - (1.0 - k) * (1.0 - k);
            *t = g.base;
            t.rotation = g.base.rotation * Quat::from_rotation_z(ease * std::f32::consts::FRAC_PI_2);
            t.translation.y = g.base.translation.y + g.base.scale.y * 0.25 * ease;
            continue;
        }
        let Ok(root) = roots.get(g.root) else { continue };
        let moved = Vec2::new(root.translation.x - g.last.x, root.translation.z - g.last.z).length();
        g.last = root.translation;
        // Units/sec, smoothed so 10 Hz snapshot easing doesn't strobe the gait.
        let target = moved / dt;
        g.speed += (target - g.speed) * (dt * 8.0).min(1.0);
        let stride = (g.speed / 60.0).clamp(0.0, 1.0);
        g.phase += dt * (2.0 + g.speed * 0.12).min(14.0);
        *t = g.base;
        t.translation.y += g.phase.sin().abs() * g.bob * stride;
        t.rotation = g.base.rotation * Quat::from_rotation_z(g.phase.sin() * 0.07 * stride);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn species_keywords_pick_the_right_body() {
        assert_eq!(plan_for_species("giant_cave_spider_brute"), 5);
        assert_eq!(plan_for_species("abyssal_leviathan_calf"), 2);
        assert_eq!(plan_for_species("scaled_pterosaur_hunter"), 9);
        assert_eq!(plan_for_species("gilded_pterosaur_behemoth_rider_mount"), 9);
        assert!(matches!(plan_for_species("iron_crawler_goliath"), 4 | 5));
        assert_eq!(plan_for_species("slag_salamander_cub"), 6);
        assert_eq!(plan_for_species("feral_megatherium_grazer"), 7);
        // Whole-word guard: "giant" contains "ant" but is not an insect.
        assert_ne!(plan_for_species("giant_chimera_alpha"), 4);
        // Stable across calls.
        assert_eq!(plan_for_species("giant_chimera_alpha"), plan_for_species("giant_chimera_alpha"));
    }

    #[test]
    fn every_bestiary_species_gets_a_plan_and_plans_spread() {
        let mut per_plan = [0u32; CREATURE_PLANS as usize];
        for m in antediluvia_sim::mobs::all_mobs() {
            per_plan[plan_for_species(&m.tag) as usize] += 1;
        }
        // At least 8 of 10 body plans must actually appear in the bestiary,
        // or the keyword table has drifted away from the real species names.
        let used = per_plan.iter().filter(|&&n| n > 0).count();
        assert!(used >= 8, "only {used} plans used: {per_plan:?}");
    }

    fn positions(m: &Mesh) -> Vec<[f32; 3]> {
        m.attribute(Mesh::ATTRIBUTE_POSITION)
            .and_then(|a| a.as_float3())
            .map(|v| v.to_vec())
            .unwrap_or_default()
    }

    fn bounds(m: &Mesh) -> (Vec3, Vec3) {
        let p = positions(m);
        let mut lo = Vec3::splat(f32::MAX);
        let mut hi = Vec3::splat(f32::MIN);
        for v in &p {
            lo = lo.min(Vec3::from(*v));
            hi = hi.max(Vec3::from(*v));
        }
        (lo, hi)
    }

    /// Bumping CREATURE_PLANS without extending plan_name silently yields
    /// "unknown", which mislabels the contact sheet and sends you inspecting
    /// the wrong ANTEDILUVIA_BEASTSHEET_PLAN index.
    #[test]
    fn every_plan_has_a_distinct_name() {
        let names: Vec<&str> = (0..CREATURE_PLANS).map(plan_name).collect();
        for (p, n) in names.iter().enumerate() {
            assert_ne!(*n, "unknown", "plan {p} has no name");
        }
        let mut s = names.clone();
        s.sort_unstable();
        s.dedup();
        assert_eq!(s.len(), names.len(), "duplicate plan names: {names:?}");
    }

    /// Every plan must build real, finite, sanely-sized geometry. A NaN vertex
    /// or a 100-unit beast would sail past the eye but wreck culling.
    #[test]
    fn every_plan_is_valid_and_bounded() {
        for plan in 0..CREATURE_PLANS {
            for k in 0..12u64 {
                let m = creature_mesh_plan(k * 6151 + 7, plan);
                let p = positions(&m);
                assert!(!p.is_empty(), "{}: empty", plan_name(plan));
                for v in &p {
                    for c in v {
                        assert!(c.is_finite(), "{}: non-finite vertex", plan_name(plan));
                    }
                }
                let (lo, hi) = bounds(&m);
                let size = hi - lo;
                assert!(
                    size.max_element() < 12.0,
                    "{}: {size:?} is too big to be a creature",
                    plan_name(plan)
                );
                assert!(
                    size.min_element() > 0.02,
                    "{}: {size:?} is degenerate/flat",
                    plan_name(plan)
                );
            }
        }
    }

    /// A creature must stand ON the ground, not float above it or sink. The
    /// rigs place y=0 at the feet, so a generated body that ignores this walks
    /// through the terrain.
    #[test]
    fn feet_reach_the_ground() {
        for plan in 0..CREATURE_PLANS {
            for k in 0..8u64 {
                let m = creature_mesh_plan(k * 977 + 13, plan);
                let (lo, hi) = bounds(&m);
                assert!(
                    lo.y > -0.35 && lo.y < 0.35,
                    "{}: lowest point {} is not near the ground",
                    plan_name(plan),
                    lo.y
                );
                assert!(hi.y > 0.2, "{}: no height at all", plan_name(plan));
            }
        }
    }

    /// The whole point: no two creatures may share geometry. This is the same
    /// guarantee propgen's `hundreds_of_distinct_props` makes for props.
    #[test]
    fn hundreds_of_distinct_creatures() {
        let mut seen = std::collections::HashSet::new();
        for k in 0..400u64 {
            let m = creature_mesh(k * 2_654_435_761 + 11);
            let p = positions(&m);
            let key: Vec<i64> = p
                .iter()
                .take(64)
                .flat_map(|v| v.iter().map(|c| (c * 4096.0) as i64).collect::<Vec<_>>())
                .collect();
            assert!(seen.insert(key), "duplicate creature geometry at seed {k}");
        }
        assert_eq!(seen.len(), 400);
    }

    /// Every plan must be reachable from `creature_plan`, or a body plan we
    /// wrote never appears in the world.
    #[test]
    fn all_plans_are_reachable() {
        let mut hit = vec![false; CREATURE_PLANS as usize];
        for k in 0..4000u64 {
            hit[creature_plan(k * 7919 + 3) as usize] = true;
        }
        assert!(hit.iter().all(|h| *h), "unreachable plans: {hit:?}");
    }

    /// Same seed → same beast. Without this, a creature would morph as it
    /// streamed in and out of view range.
    #[test]
    fn generation_is_deterministic() {
        for k in 0..24u64 {
            let a = positions(&creature_mesh(k * 31 + 5));
            let b = positions(&creature_mesh(k * 31 + 5));
            assert_eq!(a, b, "seed {k} is not deterministic");
        }
    }

    /// Parts must claim the rig nodes they actually need, and limb counts must
    /// match the plan — an arachnid with four legs is a bug you cannot see in
    /// a screenshot of a static pose.
    #[test]
    fn limb_counts_match_the_plan() {
        for plan in 0..CREATURE_PLANS {
            let parts = creature_parts(1234567, plan);
            let limbs = parts
                .iter()
                .filter(|p| matches!(p.attach, Attach::Limb { .. }))
                .count();
            let (f, b) = limb_layout(plan);
            let expected = (f + b) as usize * 2 + if matches!(plan, 3 | 7 | 9) { 2 } else { 0 };
            assert_eq!(limbs, expected, "{} limb count", plan_name(plan));
            assert!(
                parts.iter().any(|p| p.attach == Attach::Head),
                "{} has no head",
                plan_name(plan)
            );
            assert!(
                parts.iter().any(|p| p.attach == Attach::Spine),
                "{} has no torso",
                plan_name(plan)
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Contact sheet
// ---------------------------------------------------------------------------

/// Art-review grid: rows are body plans, columns are seed variants, so a
/// regression in any single plan is visible at a glance. Same contract as
/// `propgen::spawn_contact_sheet` — see the note there about why this needs
/// its OWN minimal App rather than early-returning out of `setup`.
///
/// `ANTEDILUVIA_BEASTSHEET=1` spawns it; `_PLAN=n` inspects one plan up close,
/// `_COLS` / `_CAMH` / `_CAMD` tune the grid and camera.
pub fn spawn_beast_sheet(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
) {
    // One material for the whole sheet — vertex colours carry the variation.
    let mat = materials.add(StandardMaterial {
        base_color: Color::WHITE,
        perceptual_roughness: 0.85,
        ..default()
    });
    let variants: u32 = std::env::var("ANTEDILUVIA_BEASTSHEET_COLS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(6);
    // Creatures are authored around 1 unit tall, so the grid is tight
    // compared with the prop sheet's 55-unit spacing.
    const SPACING: f32 = 3.2;

    let only: Option<u32> = std::env::var("ANTEDILUVIA_BEASTSHEET_PLAN")
        .ok()
        .and_then(|v| v.parse().ok());
    let plans: Vec<u32> = match only {
        Some(p) => vec![p % CREATURE_PLANS],
        None => (0..CREATURE_PLANS).collect(),
    };

    // No in-world text, so print the legend. Rows march along +Z, meaning row
    // 0 is the FARTHEST from the camera in a screenshot.
    info!(
        "beastsheet: {} plans x {} variants, rows far->near: {}",
        plans.len(),
        variants,
        plans
            .iter()
            .enumerate()
            .map(|(row, &p)| format!("{row}={}", plan_name(p)))
            .collect::<Vec<_>>()
            .join(" "),
    );

    for (row, &p) in plans.iter().enumerate() {
        for v in 0..variants {
            let seed = (p as u64 + 1).wrapping_mul(0x9E37_79B9_7F4A_7C15)
                ^ (v as u64 + 1).wrapping_mul(0x2545_F491);
            commands.spawn((
                Mesh3d(meshes.add(creature_mesh_plan(seed, p))),
                MeshMaterial3d(mat.clone()),
                Transform::from_xyz(
                    (v as f32 - (variants - 1) as f32 * 0.5) * SPACING,
                    0.0,
                    (row as f32 - (plans.len() - 1) as f32 * 0.5) * SPACING,
                ),
            ));
        }
    }

    // Ground plane so the beasts read against something and cast shadows —
    // the ground test is meaningless if you cannot see where the ground is.
    let ground = meshes.add(Plane3d::default().mesh().size(200.0, 200.0));
    let gmat = materials.add(StandardMaterial {
        base_color: Color::srgb(0.30, 0.33, 0.26),
        perceptual_roughness: 1.0,
        ..default()
    });
    commands.spawn((Mesh3d(ground), MeshMaterial3d(gmat), Transform::from_xyz(0.0, 0.0, 0.0)));

    commands.spawn((
        DirectionalLight { illuminance: 14_000.0, shadows_enabled: true, ..default() },
        Transform::from_xyz(20.0, 40.0, 14.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
    let envf = |k: &str, d: f32| -> f32 {
        std::env::var(k).ok().and_then(|v| v.parse().ok()).unwrap_or(d)
    };
    let (cam_h, cam_d) = if only.is_some() {
        (envf("ANTEDILUVIA_BEASTSHEET_CAMH", 1.6), envf("ANTEDILUVIA_BEASTSHEET_CAMD", 7.0))
    } else {
        (envf("ANTEDILUVIA_BEASTSHEET_CAMH", 17.0), envf("ANTEDILUVIA_BEASTSHEET_CAMD", 22.0))
    };
    commands.spawn((
        Camera3d::default(),
        Transform::from_xyz(0.0, cam_h, cam_d).looking_at(Vec3::new(0.0, 0.5, 0.0), Vec3::Y),
    ));
}
