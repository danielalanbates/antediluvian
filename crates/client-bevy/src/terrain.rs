//! Per-act visual terrain.
//!
//! The server's world is flat 2D — terrain here is presentation only. Heights
//! come from seeded fBm noise (seed = act index) so every client agrees, and
//! the land is lerped flat around the zone entry (the inn / shrine at the
//! origin) so gameplay landmarks stay readable. Character roots are offset by
//! `terrain_height` each snapshot so feet sit on the surface.

use antediluvia_protocol::Act;
use bevy::prelude::*;
use bevy::render::mesh::{Indices, PrimitiveTopology};
use bevy::render::render_asset::RenderAssetUsages;
use noise::{Fbm, MultiFractal, NoiseFn, Perlin};
use std::sync::OnceLock;

/// Grid resolution (quads per side) and world size of the terrain mesh.
const GRID: usize = 192;
/// Mesh extends a margin past the playable half-extent (C05: shared constant).
const SIZE: f32 = antediluvia_protocol::WORLD_BOUNDS * 2.0 + 600.0;

/// Dirt road from the inn (0,0) toward the far side of the map (+x).
const ROAD_END_X: f32 = antediluvia_protocol::WORLD_BOUNDS * 0.85;
const ROAD_HALF_WIDTH: f32 = 55.0;
const ROAD_BLEND: f32 = 130.0;

/// Distance from (x,z) to the road's center segment.
fn road_dist(x: f32, z: f32) -> f32 {
    let t = (x / ROAD_END_X).clamp(0.0, 1.0);
    let px = t * ROAD_END_X;
    ((x - px) * (x - px) + z * z).sqrt()
}
/// The inn stays perfectly flat inside this radius…
const FLAT_RADIUS: f32 = 240.0;
/// …and blends into the hills over this additional distance.
const BLEND_DIST: f32 = 320.0;

fn fbm(act: Act) -> &'static Fbm<Perlin> {
    static FBMS: OnceLock<Vec<Fbm<Perlin>>> = OnceLock::new();
    let all = FBMS.get_or_init(|| {
        (0..Act::ALL.len() as u32)
            .map(|i| Fbm::<Perlin>::new(i + 7).set_octaves(4).set_persistence(0.45))
            .collect()
    });
    &all[Act::ALL.iter().position(|a| *a == act).unwrap_or(0)]
}

/// Amplitude (world units) and wavelength per act — Eden gentle grassland,
/// Hermon foothills, Nephilim badlands, Enoch outskirts, Flood stormy coast.
fn act_shape(act: Act) -> (f32, f32) {
    match act {
        Act::Eden => (26.0, 700.0),
        Act::Hermon => (62.0, 540.0),
        Act::Nephilim => (44.0, 480.0),
        Act::Enoch => (20.0, 640.0),
        Act::Flood => (36.0, 460.0),
    }
}

/// Visual ground height at a server-world position (server y == render z).
pub fn terrain_height(act: Act, x: f32, z: f32) -> f32 {
    let (amp, wl) = act_shape(act);
    let n = fbm(act).get([(x / wl) as f64, (z / wl) as f64]) as f32;
    let mut h = n * amp;
    if h < 0.0 {
        h *= 0.35; // shallow basins, no pits to hide in
    }
    let d = (x * x + z * z).sqrt();
    let t = ((d - FLAT_RADIUS) / BLEND_DIST).clamp(0.0, 1.0);
    // The road cuts a flat strip through the hills.
    let r = ((road_dist(x, z) - ROAD_HALF_WIDTH) / ROAD_BLEND).clamp(0.0, 1.0);
    h * t * t * r * r
}

/// Height-banded vertex palette per act: low → mid → high.
fn act_palette(act: Act) -> ([f32; 3], [f32; 3], [f32; 3]) {
    match act {
        Act::Eden => ([0.20, 0.40, 0.16], [0.30, 0.47, 0.20], [0.46, 0.43, 0.30]),
        Act::Hermon => ([0.27, 0.40, 0.21], [0.44, 0.39, 0.27], [0.58, 0.58, 0.60]),
        Act::Nephilim => ([0.40, 0.30, 0.18], [0.52, 0.38, 0.20], [0.50, 0.45, 0.40]),
        Act::Enoch => ([0.30, 0.34, 0.24], [0.44, 0.42, 0.34], [0.54, 0.52, 0.48]),
        Act::Flood => ([0.22, 0.32, 0.27], [0.34, 0.38, 0.31], [0.44, 0.48, 0.53]),
    }
}

fn lerp3(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t]
}

fn color_for(act: Act, h: f32) -> [f32; 4] {
    let (low, mid, high) = act_palette(act);
    let (amp, _) = act_shape(act);
    let t1 = (h / (amp * 0.45)).clamp(0.0, 1.0);
    let t2 = ((h - amp * 0.45) / (amp * 0.55)).clamp(0.0, 1.0);
    let c = lerp3(lerp3(low, mid, t1), high, t2);
    // Palette values are sRGB; vertex colors feed the shader linear.
    let lin = Color::srgb(c[0], c[1], c[2]).to_linear();
    [lin.red, lin.green, lin.blue, 1.0]
}

/// Build the act's terrain mesh: GRID×GRID quads over SIZE×SIZE world units,
/// vertex-colored by height band, smooth normals.
pub fn build_terrain_mesh(act: Act) -> Mesh {
    let n = GRID;
    let step = SIZE / n as f32;
    let verts_per_side = n + 1;
    let mut positions = Vec::with_capacity(verts_per_side * verts_per_side);
    let mut colors = Vec::with_capacity(positions.capacity());
    let mut uvs = Vec::with_capacity(positions.capacity());
    for iz in 0..=n {
        for ix in 0..=n {
            let x = -SIZE / 2.0 + ix as f32 * step;
            let z = -SIZE / 2.0 + iz as f32 * step;
            let h = terrain_height(act, x, z);
            positions.push([x, h, z]);
            // Road strip renders packed dirt instead of the height palette.
            if road_dist(x, z) <= ROAD_HALF_WIDTH {
                let lin = Color::srgb(0.42, 0.33, 0.22).to_linear();
                colors.push([lin.red, lin.green, lin.blue, 1.0]);
            } else {
                colors.push(color_for(act, h));
            }
            uvs.push([ix as f32 / n as f32, iz as f32 / n as f32]);
        }
    }
    let mut indices = Vec::with_capacity(n * n * 6);
    for iz in 0..n {
        for ix in 0..n {
            let a = (iz * verts_per_side + ix) as u32;
            let b = a + 1;
            let c = a + verts_per_side as u32;
            let d = c + 1;
            // counter-clockwise from above (+Y normal)
            indices.extend_from_slice(&[a, c, b, b, c, d]);
        }
    }
    let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh.insert_indices(Indices::U32(indices));
    mesh.compute_smooth_normals();
    mesh
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn road_is_flat_and_bounded() {
        // On the road: flat all the way out.
        for x in [400.0, 1000.0, 2000.0, 3000.0] {
            let h = terrain_height(Act::Hermon, x, 0.0);
            assert!(h.abs() < 0.01, "road at x={x} should be flat, got {h}");
        }
        // Far off the road: the hills are real (Hermon is the mountain act).
        let mut any_hill = false;
        for x in [700.0, 1200.0, 1900.0] {
            if terrain_height(Act::Hermon, x, 900.0).abs() > 5.0 {
                any_hill = true;
            }
        }
        assert!(any_hill, "off-road terrain should have relief");
    }
}
