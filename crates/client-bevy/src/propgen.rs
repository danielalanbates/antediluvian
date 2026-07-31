//! Procedural fantasy prop generator — hundreds of distinct meshes from code.
//!
//! The CC0 packs (Kenney, Quaternius, Poly Haven) give us good *environment*
//! art but a fixed, finite set of it: the same barrel and the same broken
//! column repeat across every act, which is the single loudest "this is asset-
//! pack filler" tell in the game. This module closes that gap the way
//! `variety::formation_mesh` closed it for rock formations — one parametric
//! builder, a family selector, and a seed, so every dressing site can ask for
//! a prop that exists nowhere else in the world.
//!
//! Design notes:
//! - Everything is built from tapered n-gon prisms and ellipsoids, merged into
//!   ONE mesh per prop, so a prop costs exactly one draw call like any GLB.
//! - Colour lives in vertex colours, not materials, so the whole procedural
//!   set shares a single `StandardMaterial` and still batches.
//! - Meshes are indexed while building, then de-indexed for flat normals:
//!   hard-surface stone/wood reads far better faceted than smooth-shaded.

use bevy::prelude::*;
use bevy::render::mesh::{Indices, PrimitiveTopology};
use bevy::render::render_asset::RenderAssetUsages;

/// Number of distinct prop families. Each family × seed jitter is what makes
/// the set effectively unbounded; bump this as new families are added.
pub const PROP_FAMILIES: u32 = 20;

fn h01(seed: u64) -> f32 {
    let mut x = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15);
    x ^= x >> 33;
    x = x.wrapping_mul(0xC2B2_AE3D_27D4_EB4F);
    ((x >> 40) & 0xFFFFFF) as f32 / 16_777_215.0
}

/// Salt for a mirrored pair (`-1.0` / `1.0`). Casting the float straight to
/// `u64` wraps -1 to `u64::MAX`, which then overflows on `+ 1` — so map the
/// sign to a plain 0/1 index instead.
fn sidx(side: f32) -> u64 {
    if side < 0.0 {
        0
    } else {
        1
    }
}

/// Seeded value in `lo..hi`.
fn rng(seed: u64, salt: u64, lo: f32, hi: f32) -> f32 {
    lo + h01(seed ^ salt.wrapping_mul(0x9E37_79B9)) * (hi - lo)
}

// ---------------------------------------------------------------------------
// Palettes
// ---------------------------------------------------------------------------

type Rgba = [f32; 4];

fn stone(seed: u64) -> Rgba {
    let g = rng(seed, 11, 0.34, 0.62);
    [g, g * rng(seed, 12, 0.94, 1.02), g * rng(seed, 13, 0.9, 1.0), 1.0]
}

fn wood(seed: u64) -> Rgba {
    // Weathered timber, not fresh-cut pine: the first pass multiplied red to
    // 1.35 and every fence in the world read as orange plastic on screen.
    // Kept near-neutral on purpose: the camera applies post_saturation 1.32
    // plus a warm bounce fill, so anything authored as "brown" comes out of
    // the grade as bright orange. Judge this palette in-game, never in
    // isolation.
    let v = rng(seed, 21, 0.20, 0.34);
    [v * 1.02, v * 0.90, v * 0.76, 1.0]
}

fn crystal(seed: u64) -> Rgba {
    // Bright enough for bloom to catch a facet, but kept under 1.0 — values
    // above that blew out to flat candy colour under the scene's tonemap.
    match (h01(seed ^ 0x5C) * 4.0) as u32 {
        0 => [0.38, 0.66, 0.88, 1.0],  // ice
        1 => [0.66, 0.42, 0.82, 1.0],  // amethyst
        2 => [0.40, 0.78, 0.52, 1.0],  // jade
        _ => [0.85, 0.62, 0.30, 1.0],  // amber
    }
}

fn cloth(seed: u64) -> Rgba {
    // Dyed but faded — banners are the one place saturation is welcome, so
    // these stay the strongest colours in the set without going neon.
    match (h01(seed ^ 0xC1) * 5.0) as u32 {
        0 => [0.46, 0.15, 0.16, 1.0],
        1 => [0.15, 0.24, 0.42, 1.0],
        2 => [0.42, 0.36, 0.16, 1.0],
        3 => [0.20, 0.32, 0.21, 1.0],
        _ => [0.36, 0.33, 0.30, 1.0],
    }
}

fn bone(seed: u64) -> Rgba {
    let v = rng(seed, 31, 0.72, 0.92);
    [v, v * 0.97, v * 0.86, 1.0]
}

fn foliage(seed: u64) -> Rgba {
    let v = rng(seed, 41, 0.20, 0.42);
    [v * 0.8, v * 1.5, v * 0.65, 1.0]
}

// ---------------------------------------------------------------------------
// Mesh builder
// ---------------------------------------------------------------------------

#[derive(Default)]
struct Builder {
    pos: Vec<[f32; 3]>,
    col: Vec<Rgba>,
    idx: Vec<u32>,
}

impl Builder {
    /// Tapered n-gon prism — the workhorse. `r0`/`r1` are the bottom/top radii,
    /// `lean` shifts the top cap sideways (broken columns, leaning posts),
    /// `warp` jitters each side's radius so no two calls match exactly.
    #[allow(clippy::too_many_arguments)]
    fn prism(
        &mut self,
        centre: Vec3,
        sides: u32,
        r0: f32,
        r1: f32,
        h: f32,
        yaw: f32,
        lean: Vec3,
        warp: f32,
        seed: u64,
        c: Rgba,
    ) {
        let base = self.pos.len() as u32;
        let n = sides.max(3);
        for ring in 0..2 {
            let (r, off) = if ring == 0 { (r0, Vec3::ZERO) } else { (r1, lean) };
            let y = if ring == 0 { 0.0 } else { h };
            for s in 0..n {
                let th = std::f32::consts::TAU * s as f32 / n as f32 + yaw;
                let j = 1.0 + (h01(seed ^ (ring * 97 + s as u64 + 3)) - 0.5) * 2.0 * warp;
                let p = centre + off + Vec3::new(r * j * th.cos(), y, r * j * th.sin());
                self.pos.push([p.x, p.y, p.z]);
                // Subtle per-vertex shade so flat facets still read as varied.
                let k = 0.88 + h01(seed ^ (s as u64 * 13 + ring * 7)) * 0.24;
                self.col.push([c[0] * k, c[1] * k, c[2] * k, c[3]]);
            }
        }
        for s in 0..n {
            let a = base + s;
            let b = base + (s + 1) % n;
            let cc = a + n;
            let d = b + n;
            self.idx.extend_from_slice(&[a, b, cc, b, d, cc]);
        }
        // Caps (fan from vertex 0 of each ring) — keeps props watertight so
        // they never show hollow interiors when the camera clips them.
        for ring in 0..2 {
            let o = base + ring * n;
            for s in 1..n - 1 {
                if ring == 0 {
                    self.idx.extend_from_slice(&[o, o + s + 1, o + s]);
                } else {
                    self.idx.extend_from_slice(&[o, o + s, o + s + 1]);
                }
            }
        }
    }

    /// Axis-aligned-ish box with yaw — a 4-gon prism, offset so flats face out.
    #[allow(clippy::too_many_arguments)]
    fn boxy(&mut self, centre: Vec3, w: f32, h: f32, d: f32, yaw: f32, seed: u64, c: Rgba) {
        // A 4-side prism inscribes the box; scale radius by sqrt(2)/2 and turn
        // it 45° so the faces (not the corners) point along w/d.
        let r = (w.max(d)) * 0.7071;
        let squash = if w > d { d / w } else { w / d };
        let before = self.pos.len();
        self.prism(centre, 4, r, r, h, yaw + std::f32::consts::FRAC_PI_4, Vec3::ZERO, 0.0, seed, c);
        // Squash the minor axis in local space around the centre.
        if squash < 0.999 {
            let (sy, cy) = (yaw.sin(), yaw.cos());
            for p in &mut self.pos[before..] {
                let (dx, dz) = (p[0] - centre.x, p[2] - centre.z);
                let (lx, lz) = (dx * cy + dz * sy, -dx * sy + dz * cy);
                let (lx, lz) = if w > d { (lx, lz * squash) } else { (lx * squash, lz) };
                p[0] = centre.x + lx * cy - lz * sy;
                p[2] = centre.z + lx * sy + lz * cy;
            }
        }
    }

    /// Low-poly ellipsoid for boulders, skulls, mushroom caps, foliage blobs.
    fn blob(&mut self, centre: Vec3, r: Vec3, rings: u32, segs: u32, warp: f32, seed: u64, c: Rgba) {
        let base = self.pos.len() as u32;
        for ri in 0..=rings {
            let phi = std::f32::consts::PI * ri as f32 / rings as f32;
            for s in 0..segs {
                let th = std::f32::consts::TAU * s as f32 / segs as f32;
                let j = 1.0 + (h01(seed ^ (ri as u64 * 41 + s as u64 + 5)) - 0.5) * 2.0 * warp;
                let p = centre
                    + Vec3::new(
                        r.x * phi.sin() * th.cos() * j,
                        r.y * phi.cos() * j,
                        r.z * phi.sin() * th.sin() * j,
                    );
                self.pos.push([p.x, p.y, p.z]);
                let k = 0.86 + h01(seed ^ (ri as u64 * 17 + s as u64)) * 0.28;
                self.col.push([c[0] * k, c[1] * k, c[2] * k, c[3]]);
            }
        }
        for ri in 0..rings {
            for s in 0..segs {
                let a = base + ri * segs + s;
                let b = base + ri * segs + (s + 1) % segs;
                let cc = a + segs;
                let d = b + segs;
                self.idx.extend_from_slice(&[a, b, cc, b, d, cc]);
            }
        }
    }

    fn finish(self) -> Mesh {
        let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
        mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, self.pos);
        mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, self.col);
        mesh.insert_indices(Indices::U32(self.idx));
        // Faceted hard-surface shading: de-index, then per-triangle normals.
        mesh.duplicate_vertices();
        mesh.compute_flat_normals();
        mesh
    }
}

// ---------------------------------------------------------------------------
// Families
// ---------------------------------------------------------------------------

/// Which family a seed selects. Exposed so dressing can bias sites toward
/// thematically appropriate props (graveyards → sarcophagi, camps → tents).
pub fn prop_family(seed: u64) -> u32 {
    (h01(seed ^ 0xB0B0_u64) * PROP_FAMILIES as f32) as u32 % PROP_FAMILIES
}

/// Human-readable family name — used by tests, the dev console and the
/// contact-sheet exporter.
pub fn family_name(f: u32) -> &'static str {
    match f % PROP_FAMILIES {
        0 => "standing_stone",
        1 => "ruined_column",
        2 => "ruined_arch",
        3 => "altar",
        4 => "statue",
        5 => "crystal_cluster",
        6 => "giant_mushroom",
        7 => "totem",
        8 => "campfire",
        9 => "tent",
        10 => "cart",
        11 => "well",
        12 => "fence",
        13 => "bone_pile",
        14 => "watchtower_ruin",
        15 => "cairn",
        16 => "banner",
        17 => "sarcophagus",
        18 => "brazier",
        19 => "dead_stump",
        _ => "unknown",
    }
}

/// Build a unique prop mesh for `seed`, using that seed's family.
pub fn prop_mesh(seed: u64) -> Mesh {
    prop_mesh_family(seed, prop_family(seed))
}

/// Build a prop of an explicit family, still uniquely jittered by `seed`.
pub fn prop_mesh_family(seed: u64, family: u32) -> Mesh {
    let mut b = Builder::default();
    let s = stone(seed);
    let w = wood(seed);
    match family % PROP_FAMILIES {
        // Monolith: one leaning slab, chipped top.
        0 => {
            let h = rng(seed, 1, 16.0, 34.0);
            let wdt = rng(seed, 2, 3.0, 7.0);
            b.prism(
                Vec3::ZERO,
                rng(seed, 3, 5.0, 8.0) as u32,
                wdt,
                wdt * rng(seed, 4, 0.6, 0.95),
                h,
                rng(seed, 5, 0.0, 6.28),
                Vec3::new(rng(seed, 6, -2.0, 2.0), 0.0, rng(seed, 7, -2.0, 2.0)),
                0.14,
                seed,
                s,
            );
        }
        // Broken fluted column on a square base.
        1 => {
            let h = rng(seed, 1, 10.0, 26.0);
            let r = rng(seed, 2, 2.2, 4.0);
            b.boxy(Vec3::ZERO, r * 3.0, 1.6, r * 3.0, 0.0, seed, s);
            b.prism(Vec3::Y * 1.6, 10, r, r * 0.86, h, 0.0, Vec3::ZERO, 0.05, seed, s);
            // Snapped-off cap sitting at an angle beside it, half the time.
            if h01(seed ^ 0xB0) > 0.5 {
                b.prism(
                    Vec3::new(r * 3.5, 0.0, rng(seed, 8, -4.0, 4.0)),
                    10,
                    r,
                    r * 0.9,
                    r * 1.5,
                    rng(seed, 9, 0.0, 3.14),
                    Vec3::new(r, 0.0, 0.0),
                    0.05,
                    seed ^ 7,
                    s,
                );
            }
        }
        // Free-standing gateway arch.
        2 => {
            let span = rng(seed, 1, 9.0, 18.0);
            let h = rng(seed, 2, 14.0, 24.0);
            let r = rng(seed, 3, 1.8, 3.2);
            for side in [-1.0f32, 1.0] {
                b.prism(
                    Vec3::new(span * side, 0.0, 0.0),
                    6,
                    r * 1.2,
                    r,
                    h,
                    0.0,
                    Vec3::ZERO,
                    0.08,
                    seed ^ sidx(side),
                    s,
                );
            }
            // Lintel: a flat prism laid across the top.
            let segs = 5;
            for i in 0..segs {
                let t = i as f32 / (segs - 1) as f32;
                let x = -span + 2.0 * span * t;
                let y = h + (1.0 - (t * 2.0 - 1.0).powi(2)) * rng(seed, 4, 1.0, 4.0);
                b.boxy(Vec3::new(x, y, 0.0), 2.0 * span / segs as f32 + 0.6, r * 1.4, r * 2.0, 0.0, seed ^ i as u64, s);
            }
        }
        // Stepped altar with an offering slab.
        3 => {
            let wdt = rng(seed, 1, 7.0, 12.0);
            let steps = rng(seed, 2, 2.0, 4.0) as u32;
            for i in 0..steps {
                let t = i as f32 / steps as f32;
                b.boxy(
                    Vec3::Y * (i as f32 * 1.8),
                    wdt * (1.0 - t * 0.35),
                    1.8,
                    wdt * (1.0 - t * 0.35) * rng(seed, 3, 0.7, 1.0),
                    0.0,
                    seed ^ i as u64,
                    s,
                );
            }
            b.boxy(Vec3::Y * (steps as f32 * 1.8 + 0.8), wdt * 0.7, 1.6, wdt * 0.5, rng(seed, 4, -0.2, 0.2), seed, s);
        }
        // Weathered statue: plinth, legs, torso, arms, head.
        4 => {
            let sc = rng(seed, 1, 0.8, 1.6);
            b.boxy(Vec3::ZERO, 7.0 * sc, 3.0 * sc, 6.0 * sc, 0.0, seed, s);
            let y = 3.0 * sc;
            for side in [-1.0f32, 1.0] {
                b.prism(Vec3::new(1.5 * sc * side, y, 0.0), 5, 1.1 * sc, 0.9 * sc, 6.0 * sc, 0.0, Vec3::ZERO, 0.05, seed ^ (sidx(side) + 1), s);
            }
            b.prism(Vec3::new(0.0, y + 6.0 * sc, 0.0), 6, 3.0 * sc, 2.4 * sc, 7.0 * sc, 0.0, Vec3::ZERO, 0.07, seed ^ 3, s);
            // Arms — one is broken off on roughly half of them.
            let broken = h01(seed ^ 0xA1) > 0.5;
            for (i, side) in [-1.0f32, 1.0].iter().enumerate() {
                if broken && i == 0 {
                    continue;
                }
                b.prism(
                    Vec3::new(3.2 * sc * side, y + 11.0 * sc, 0.0),
                    4,
                    0.8 * sc,
                    0.6 * sc,
                    -5.0 * sc,
                    0.0,
                    Vec3::new(rng(seed, 5, -1.0, 1.0) * sc, 0.0, 0.0),
                    0.05,
                    seed ^ (i as u64 + 9),
                    s,
                );
            }
            if h01(seed ^ 0xA2) > 0.25 {
                b.blob(Vec3::new(0.0, y + 14.5 * sc, 0.0), Vec3::splat(2.2 * sc), 4, 6, 0.12, seed, s);
            }
        }
        // Crystal cluster: shards of one hue fanning from a common root.
        5 => {
            let c = crystal(seed);
            let n = rng(seed, 1, 4.0, 9.0) as u32;
            for i in 0..n {
                let th = std::f32::consts::TAU * i as f32 / n as f32 + rng(seed, 2, 0.0, 1.0);
                let lean = rng(seed, 3 + i as u64, 1.5, 5.0);
                let h = rng(seed, 20 + i as u64, 5.0, 18.0);
                let r = rng(seed, 40 + i as u64, 0.8, 2.4);
                b.prism(
                    Vec3::new(th.cos() * r, 0.0, th.sin() * r),
                    6,
                    r,
                    r * 0.12,
                    h,
                    th,
                    Vec3::new(th.cos() * lean, 0.0, th.sin() * lean),
                    0.03,
                    seed ^ i as u64,
                    c,
                );
            }
        }
        // Giant mushroom: stalk plus a warped cap, sometimes a second one.
        6 => {
            let caps = rng(seed, 1, 1.0, 3.0) as u32;
            for i in 0..caps {
                let off = if i == 0 { Vec3::ZERO } else { Vec3::new(rng(seed, 2, 4.0, 9.0), 0.0, rng(seed, 3, -6.0, 6.0)) };
                let h = rng(seed, 10 + i as u64, 6.0, 16.0) * if i == 0 { 1.0 } else { 0.65 };
                let r = rng(seed, 20 + i as u64, 1.0, 2.2);
                b.prism(off, 7, r * 1.3, r * 0.8, h, 0.0, Vec3::new(rng(seed, 30 + i as u64, -1.5, 1.5), 0.0, 0.0), 0.06, seed ^ i as u64, [0.86, 0.82, 0.72, 1.0]);
                let cr = r * rng(seed, 40 + i as u64, 2.6, 4.4);
                b.blob(off + Vec3::Y * h, Vec3::new(cr, cr * rng(seed, 5, 0.4, 0.8), cr), 3, 8, 0.1, seed ^ (i as u64 + 3), crystal(seed ^ 0x77));
            }
        }
        // Totem: stacked carved blocks, alternating wood and paint.
        7 => {
            let n = rng(seed, 1, 3.0, 7.0) as u32;
            let mut y = 0.0;
            for i in 0..n {
                let hh = rng(seed, 10 + i as u64, 2.0, 4.5);
                let r = rng(seed, 20 + i as u64, 2.0, 3.6);
                let c = if i % 2 == 0 { w } else { cloth(seed ^ i as u64) };
                b.prism(Vec3::Y * y, rng(seed, 30 + i as u64, 4.0, 8.0) as u32, r, r * rng(seed, 40 + i as u64, 0.75, 1.05), hh, rng(seed, 50 + i as u64, 0.0, 6.28), Vec3::ZERO, 0.06, seed ^ i as u64, c);
                y += hh;
            }
            // Outstretched wings on the top block.
            if h01(seed ^ 0x7A) > 0.4 {
                for side in [-1.0f32, 1.0] {
                    b.boxy(Vec3::new(3.5 * side, y - 2.0, 0.0), 5.0, 0.8, 1.6, 0.0, seed ^ sidx(side), w);
                }
            }
        }
        // Campfire: log ring around an ember cone.
        8 => {
            let n = rng(seed, 1, 4.0, 8.0) as u32;
            let r = rng(seed, 2, 2.6, 4.4);
            for i in 0..n {
                let th = std::f32::consts::TAU * i as f32 / n as f32;
                b.prism(Vec3::new(th.cos() * r, 0.3, th.sin() * r), 5, 0.5, 0.42, rng(seed, 10 + i as u64, 3.5, 6.0), th, Vec3::new(-th.cos() * r * 0.7, 0.0, -th.sin() * r * 0.7), 0.05, seed ^ i as u64, w);
            }
            b.prism(Vec3::ZERO, 6, r * 0.6, 0.05, rng(seed, 3, 2.5, 5.0), 0.0, Vec3::ZERO, 0.2, seed, [1.4, 0.62, 0.18, 1.0]);
            // Stone ring.
            for i in 0..n + 2 {
                let th = std::f32::consts::TAU * i as f32 / (n + 2) as f32 + 0.3;
                b.blob(Vec3::new(th.cos() * r * 1.5, 0.0, th.sin() * r * 1.5), Vec3::splat(rng(seed, 60 + i as u64, 0.6, 1.2)), 2, 5, 0.25, seed ^ i as u64, s);
            }
        }
        // Tent: ridge pole with sloped cloth sides.
        9 => {
            let c = cloth(seed);
            let len = rng(seed, 1, 8.0, 14.0);
            let wd = rng(seed, 2, 5.0, 9.0);
            let h = rng(seed, 3, 5.0, 9.0);
            // Two sloped slabs meeting at the ridge.
            for side in [-1.0f32, 1.0] {
                let before = b.pos.len();
                b.boxy(Vec3::new(0.0, 0.0, wd * 0.5 * side), len, h, 0.5, 0.0, seed ^ sidx(side), c);
                // Shear the top toward the centre to make the slope.
                for p in &mut b.pos[before..] {
                    if p[1] > h * 0.5 {
                        p[2] -= wd * 0.5 * side;
                    }
                }
            }
            b.prism(Vec3::new(-len * 0.5, 0.0, 0.0), 5, 0.35, 0.3, h + 1.5, 0.0, Vec3::ZERO, 0.02, seed, w);
            b.prism(Vec3::new(len * 0.5, 0.0, 0.0), 5, 0.35, 0.3, h + 1.5, 0.0, Vec3::ZERO, 0.02, seed ^ 2, w);
        }
        // Hand cart: bed, two wheels, shafts.
        10 => {
            let len = rng(seed, 1, 8.0, 13.0);
            let wd = rng(seed, 2, 4.5, 7.0);
            b.boxy(Vec3::Y * 3.0, len, 1.2, wd, 0.0, seed, w);
            for side in [-1.0f32, 1.0] {
                b.boxy(Vec3::new(0.0, 4.0, wd * 0.5 * side), len, 2.2, 0.5, 0.0, seed ^ sidx(side), w);
                let before = b.pos.len();
                b.prism(Vec3::new(-len * 0.15, 3.0, wd * 0.55 * side), 9, 3.0, 3.0, 0.6, 0.0, Vec3::ZERO, 0.02, seed ^ 5, wood(seed ^ 9));
                // Stand the wheel upright (prisms build along +Y).
                for p in &mut b.pos[before..] {
                    let (y, z) = (p[1] - 3.0, p[2] - wd * 0.55 * side);
                    p[1] = 3.0 + z;
                    p[2] = wd * 0.55 * side + y;
                }
            }
            for side in [-1.0f32, 1.0] {
                b.boxy(Vec3::new(len * 0.62, 3.4, wd * 0.3 * side), len * 0.5, 0.5, 0.5, 0.0, seed ^ (sidx(side) + 3), w);
            }
        }
        // Well: stone drum, posts, roof.
        11 => {
            let r = rng(seed, 1, 3.4, 5.2);
            b.prism(Vec3::ZERO, 12, r, r * 0.96, rng(seed, 2, 3.0, 5.0), 0.0, Vec3::ZERO, 0.05, seed, s);
            let ph = rng(seed, 3, 8.0, 12.0);
            for side in [-1.0f32, 1.0] {
                b.prism(Vec3::new(r * 0.8 * side, 3.0, 0.0), 4, 0.5, 0.45, ph, 0.0, Vec3::ZERO, 0.03, seed ^ sidx(side), w);
            }
            b.prism(Vec3::new(0.0, 3.0 + ph, 0.0), 4, r * 1.5, 0.2, rng(seed, 4, 2.5, 4.5), std::f32::consts::FRAC_PI_4, Vec3::ZERO, 0.04, seed ^ 4, w);
        }
        // Fence run: posts plus two rails, some posts snapped.
        12 => {
            let n = rng(seed, 1, 3.0, 7.0) as u32;
            let gap = rng(seed, 2, 5.0, 8.0);
            for i in 0..n {
                let x = i as f32 * gap;
                let hh = if h01(seed ^ (i as u64 + 0x1F)) > 0.15 { rng(seed, 10 + i as u64, 5.0, 7.5) } else { rng(seed, 10 + i as u64, 1.5, 3.0) };
                b.prism(Vec3::new(x, 0.0, 0.0), 4, 0.5, 0.42, hh, rng(seed, 20 + i as u64, -0.15, 0.15), Vec3::new(rng(seed, 30 + i as u64, -0.5, 0.5), 0.0, 0.0), 0.05, seed ^ i as u64, w);
                if i + 1 < n {
                    for (k, ry) in [(0u64, 2.2f32), (1, 4.6)] {
                        b.boxy(Vec3::new(x + gap * 0.5, ry, 0.0), gap, 0.45, 0.35, 0.0, seed ^ (i as u64 * 3 + k), w);
                    }
                }
            }
        }
        // Bone pile: ribs arcing out of a skull-and-scatter heap.
        13 => {
            let c = bone(seed);
            let ribs = rng(seed, 1, 3.0, 7.0) as u32;
            for i in 0..ribs {
                let t = i as f32 / ribs as f32;
                let x = -6.0 + 12.0 * t;
                let hh = rng(seed, 10 + i as u64, 4.0, 9.0);
                b.prism(Vec3::new(x, 0.0, 0.0), 4, 0.45, 0.3, hh, 0.0, Vec3::new(rng(seed, 20 + i as u64, -3.0, 3.0), 0.0, rng(seed, 30 + i as u64, -2.0, 2.0)), 0.08, seed ^ i as u64, c);
            }
            b.blob(Vec3::new(rng(seed, 2, -7.0, -4.0), 1.4, 0.0), Vec3::new(2.4, 1.9, 2.0), 3, 6, 0.14, seed, c);
            for i in 0..4 {
                b.prism(Vec3::new(rng(seed, 40 + i, -6.0, 6.0), 0.2, rng(seed, 50 + i, -4.0, 4.0)), 4, 0.35, 0.3, rng(seed, 60 + i, 2.0, 5.0), rng(seed, 70 + i, 0.0, 6.28), Vec3::new(rng(seed, 80 + i, -2.0, 2.0), 0.0, 0.0), 0.06, seed ^ (i + 20), c);
            }
        }
        // Watchtower ruin: tapered drum, collapsed on one side.
        14 => {
            let r = rng(seed, 1, 5.0, 8.0);
            let h = rng(seed, 2, 18.0, 34.0);
            b.prism(Vec3::ZERO, 10, r * 1.2, r * 0.9, h, 0.0, Vec3::ZERO, 0.07, seed, s);
            // Crenellations, with gaps where the wall has fallen.
            let n = 10;
            for i in 0..n {
                if h01(seed ^ (i as u64 + 0x2C)) < 0.35 {
                    continue;
                }
                let th = std::f32::consts::TAU * i as f32 / n as f32;
                b.boxy(Vec3::new(th.cos() * r * 0.85, h, th.sin() * r * 0.85), 1.8, rng(seed, 10 + i as u64, 1.5, 3.5), 1.8, th, seed ^ i as u64, s);
            }
            // Rubble at the foot.
            for i in 0..5 {
                b.blob(Vec3::new(rng(seed, 20 + i, -r * 2.0, r * 2.0), 0.4, rng(seed, 30 + i, -r * 2.0, r * 2.0)), Vec3::splat(rng(seed, 40 + i, 0.8, 2.2)), 2, 5, 0.3, seed ^ (i + 5), s);
            }
        }
        // Cairn: stacked flat stones, narrowing upward.
        15 => {
            let n = rng(seed, 1, 4.0, 9.0) as u32;
            let mut y = 0.0;
            for i in 0..n {
                let t = i as f32 / n as f32;
                let r = rng(seed, 10 + i as u64, 1.4, 3.4) * (1.0 - t * 0.55);
                let hh = rng(seed, 20 + i as u64, 0.7, 1.8);
                b.blob(Vec3::new(rng(seed, 30 + i as u64, -0.6, 0.6), y + hh * 0.5, rng(seed, 40 + i as u64, -0.6, 0.6)), Vec3::new(r, hh, r * rng(seed, 50 + i as u64, 0.7, 1.1)), 2, 6, 0.2, seed ^ i as u64, s);
                y += hh * 1.7;
            }
        }
        // Banner: pole, crossbar, hanging cloth.
        16 => {
            let c = cloth(seed);
            let h = rng(seed, 1, 12.0, 22.0);
            b.prism(Vec3::ZERO, 6, 0.45, 0.35, h, 0.0, Vec3::new(rng(seed, 2, -0.8, 0.8), 0.0, 0.0), 0.03, seed, w);
            let bw = rng(seed, 3, 3.0, 6.0);
            b.boxy(Vec3::Y * (h - 1.0), bw * 2.0, 0.4, 0.4, 0.0, seed ^ 1, w);
            let before = b.pos.len();
            b.boxy(Vec3::new(0.0, h - rng(seed, 4, 7.0, 12.0), 0.0), bw * 1.8, rng(seed, 5, 6.0, 11.0), 0.25, 0.0, seed ^ 2, c);
            // Ragged lower hem.
            let lo = b.pos[before..].iter().map(|p| p[1]).fold(f32::MAX, f32::min);
            for (i, p) in b.pos[before..].iter_mut().enumerate() {
                if (p[1] - lo).abs() < 0.01 {
                    p[1] -= rng(seed, 60 + i as u64, 0.0, 2.5);
                }
            }
        }
        // Sarcophagus: lid slightly askew on a carved box.
        17 => {
            let len = rng(seed, 1, 9.0, 13.0);
            let wd = rng(seed, 2, 4.0, 6.0);
            b.boxy(Vec3::ZERO, len, rng(seed, 3, 3.0, 4.5), wd, 0.0, seed, s);
            let hh = rng(seed, 3, 3.0, 4.5);
            b.boxy(Vec3::new(rng(seed, 4, -1.5, 1.5), hh, rng(seed, 5, -0.8, 0.8)), len * 0.95, 1.4, wd * 0.95, rng(seed, 6, -0.25, 0.25), seed ^ 1, s);
            if h01(seed ^ 0x5A) > 0.5 {
                b.blob(Vec3::new(0.0, hh + 1.9, 0.0), Vec3::new(1.6, 1.2, 1.4), 3, 6, 0.12, seed, s);
            }
        }
        // Brazier: tripod legs, bowl, flame.
        18 => {
            let r = rng(seed, 1, 2.2, 3.6);
            let hh = rng(seed, 2, 4.0, 7.0);
            for i in 0..3 {
                let th = std::f32::consts::TAU * i as f32 / 3.0;
                b.prism(Vec3::new(th.cos() * r * 0.8, 0.0, th.sin() * r * 0.8), 4, 0.4, 0.3, hh, th, Vec3::new(-th.cos() * r * 0.5, 0.0, -th.sin() * r * 0.5), 0.04, seed ^ i, [0.22, 0.2, 0.19, 1.0]);
            }
            b.prism(Vec3::Y * hh, 9, r * 0.7, r, 1.8, 0.0, Vec3::ZERO, 0.05, seed, [0.26, 0.23, 0.2, 1.0]);
            b.prism(Vec3::Y * (hh + 1.6), 6, r * 0.7, 0.08, rng(seed, 3, 2.0, 4.5), 0.0, Vec3::ZERO, 0.25, seed ^ 3, [1.45, 0.6, 0.16, 1.0]);
        }
        // Dead stump with splayed roots.
        _ => {
            let r = rng(seed, 1, 2.4, 4.5);
            let hh = rng(seed, 2, 4.0, 11.0);
            b.prism(Vec3::ZERO, 8, r * 1.3, r * 0.7, hh, 0.0, Vec3::new(rng(seed, 3, -1.2, 1.2), 0.0, 0.0), 0.12, seed, w);
            let n = rng(seed, 4, 3.0, 7.0) as u32;
            for i in 0..n {
                let th = std::f32::consts::TAU * i as f32 / n as f32 + rng(seed, 5, 0.0, 1.0);
                b.prism(Vec3::new(th.cos() * r, 0.6, th.sin() * r), 4, 0.7, 0.25, rng(seed, 10 + i as u64, 2.0, 5.0), th, Vec3::new(th.cos() * r * 2.2, -1.2, th.sin() * r * 2.2), 0.1, seed ^ i as u64, w);
            }
            // Occasional shelf fungus.
            if h01(seed ^ 0xF3) > 0.5 {
                b.blob(Vec3::new(r * 0.8, hh * 0.6, 0.0), Vec3::new(1.6, 0.4, 1.6), 2, 6, 0.2, seed, foliage(seed));
            }
        }
    }
    b.finish()
}

/// Contact-sheet mode: lay every prop family out on a lit grid so the geometry
/// can be judged from a screenshot instead of by reading vertex maths.
///
/// `ANTEDILUVIA_PROPSHEET=1` spawns this instead of the world. Columns are
/// variants of one seed run, rows are families, so a regression in any single
/// family is visible at a glance.
pub fn spawn_contact_sheet(
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
    let variants: u32 = std::env::var("ANTEDILUVIA_PROPSHEET_COLS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(6);
    const SPACING: f32 = 55.0;

    // `ANTEDILUVIA_PROPSHEET_FAMILY=n` inspects one family up close (its
    // variants spread across a single row) instead of the full 20-row sheet.
    let only: Option<u32> = std::env::var("ANTEDILUVIA_PROPSHEET_FAMILY")
        .ok()
        .and_then(|v| v.parse().ok());
    let families: Vec<u32> = match only {
        Some(f) => vec![f % PROP_FAMILIES],
        None => (0..PROP_FAMILIES).collect(),
    };

    for (row, &f) in families.iter().enumerate() {
        for v in 0..variants {
            let seed = (f as u64 + 1).wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ (v as u64 + 1).wrapping_mul(0x2545_F491);
            let mesh = meshes.add(prop_mesh_family(seed, f));
            commands.spawn((
                Mesh3d(mesh),
                MeshMaterial3d(mat.clone()),
                Transform::from_xyz(
                    (v as f32 - (variants - 1) as f32 * 0.5) * SPACING,
                    0.0,
                    (row as f32 - (families.len() - 1) as f32 * 0.5) * SPACING,
                ),
            ));
        }
    }

    // Ground plane so props read against something and cast visible shadows.
    let ground = meshes.add(Plane3d::default().mesh().size(2200.0, 2200.0));
    let gmat = materials.add(StandardMaterial {
        base_color: Color::srgb(0.30, 0.33, 0.26),
        perceptual_roughness: 1.0,
        ..default()
    });
    commands.spawn((Mesh3d(ground), MeshMaterial3d(gmat), Transform::from_xyz(0.0, -0.05, 0.0)));

    commands.spawn((
        DirectionalLight { illuminance: 14_000.0, shadows_enabled: true, ..default() },
        Transform::from_xyz(300.0, 600.0, 200.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
    // Camera framing is env-tunable: the full sheet needs a high wide shot,
    // a single-family review needs a low near one that shows silhouettes.
    let envf = |k: &str, d: f32| -> f32 {
        std::env::var(k).ok().and_then(|v| v.parse().ok()).unwrap_or(d)
    };
    let (cam_h, cam_d) = if only.is_some() {
        (envf("ANTEDILUVIA_PROPSHEET_CAMH", 26.0), envf("ANTEDILUVIA_PROPSHEET_CAMD", 120.0))
    } else {
        (envf("ANTEDILUVIA_PROPSHEET_CAMH", 620.0), envf("ANTEDILUVIA_PROPSHEET_CAMD", 780.0))
    };
    commands.spawn((
        Camera3d::default(),
        Transform::from_xyz(0.0, cam_h, cam_d).looking_at(Vec3::new(0.0, 10.0, 0.0), Vec3::Y),
    ));
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

    /// The whole point of this module: hundreds of props, none identical.
    #[test]
    fn hundreds_of_distinct_props() {
        let mut seen: Vec<Vec<[f32; 3]>> = Vec::new();
        for seed in 0..400u64 {
            let m = prop_mesh(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ 0xBEEF);
            let p = positions(&m);
            assert!(p.len() >= 24, "seed {seed}: degenerate mesh ({} verts)", p.len());
            seen.push(p);
        }
        // Compare every pair — 400 props must be 400 distinct geometries.
        for i in 0..seen.len() {
            for j in i + 1..seen.len() {
                assert!(seen[i] != seen[j], "props {i} and {j} are identical");
            }
        }
    }

    /// Every family must build, be finite, and stay within a sane world size —
    /// a NaN or a 10km prop would sail past the eye but wreck culling.
    #[test]
    fn every_family_is_valid_and_bounded() {
        for f in 0..PROP_FAMILIES {
            for k in 0..12u64 {
                let m = prop_mesh_family(k * 7717 + 13, f);
                let p = positions(&m);
                assert!(!p.is_empty(), "{}: empty", family_name(f));
                for v in &p {
                    for c in v {
                        assert!(c.is_finite(), "{}: non-finite vertex", family_name(f));
                        assert!(c.abs() < 200.0, "{}: vertex {c} out of bounds", family_name(f));
                    }
                }
                let top = p.iter().map(|v| v[1]).fold(f32::MIN, f32::max);
                assert!(top > 0.5, "{}: flat prop (height {top})", family_name(f));
            }
        }
    }

    /// Flat shading requires normals to exist and be unit length.
    #[test]
    fn normals_and_colors_are_present() {
        let m = prop_mesh(42);
        let n = match m.attribute(Mesh::ATTRIBUTE_NORMAL).unwrap() {
            VertexAttributeValues::Float32x3(v) => v.clone(),
            _ => panic!("normals"),
        };
        assert_eq!(n.len(), positions(&m).len(), "normal/position count mismatch");
        for v in &n {
            let len = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
            assert!((len - 1.0).abs() < 0.05 || len < 1e-6, "non-unit normal {len}");
        }
        assert!(m.attribute(Mesh::ATTRIBUTE_COLOR).is_some(), "vertex colours missing");
    }

    /// Determinism: the same seed must rebuild byte-identically, or props will
    /// pop into different shapes as the player walks in and out of range.
    #[test]
    fn generation_is_deterministic() {
        for seed in [1u64, 9_999, 123_456_789] {
            assert_eq!(positions(&prop_mesh(seed)), positions(&prop_mesh(seed)));
        }
    }
}
