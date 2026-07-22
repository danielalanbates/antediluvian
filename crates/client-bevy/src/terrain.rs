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
pub fn road_dist(x: f32, z: f32) -> f32 {
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
        Act::Eden => (72.0, 700.0),
        Act::Hermon => (120.0, 540.0),
        Act::Nephilim => (95.0, 480.0),
        Act::Enoch => (42.0, 640.0),
        Act::Flood => (75.0, 460.0),
    }
}

// ─── Location-doc landforms (docs/locations/01–05) ───────────────────────────

const B: f32 = antediluvia_protocol::WORLD_BOUNDS;

/// Distance from p to segment ab.
fn seg_dist(p: (f32, f32), a: (f32, f32), b: (f32, f32)) -> f32 {
    let (px, pz) = p;
    let (ax, az) = a;
    let (bx, bz) = b;
    let (dx, dz) = (bx - ax, bz - az);
    let t = (((px - ax) * dx + (pz - az) * dz) / (dx * dx + dz * dz)).clamp(0.0, 1.0);
    ((px - ax - t * dx).powi(2) + (pz - az - t * dz).powi(2)).sqrt()
}

/// Smooth valley carved along a segment: depth at center, 0 beyond width.
fn channel(p: (f32, f32), a: (f32, f32), b: (f32, f32), width: f32, depth: f32) -> f32 {
    let d = seg_dist(p, a, b);
    if d >= width {
        0.0
    } else {
        let t = 1.0 - d / width;
        -depth * t * t
    }
}

/// Round hill/plateau: height at center, 0 beyond radius (cosine falloff).
fn dome(p: (f32, f32), c: (f32, f32), radius: f32, height: f32) -> f32 {
    let d = ((p.0 - c.0).powi(2) + (p.1 - c.1).powi(2)).sqrt();
    if d >= radius {
        0.0
    } else {
        height * (0.5 + 0.5 * (std::f32::consts::PI * d / radius).cos())
    }
}

/// The four rivers of Eden converge in the eastern basin (doc 01):
/// Pishon, Gihon, Tigris, Euphrates — carved channels meeting at (2300, 0).
const EDEN_CONFLUENCE: (f32, f32) = (2300.0, 0.0);
const EDEN_RIVERS: [(f32, f32); 4] =
    [(-3400.0, -2600.0), (-2200.0, 3400.0), (900.0, -3400.0), (1400.0, 3400.0)];

/// Doc-driven landform offset added onto the base fBm relief.
fn landforms(act: Act, x: f32, z: f32) -> f32 {
    let p = (x, z);
    match act {
        // 01 — Edenic Basin east (land dips toward the confluence), the four
        // rivers carved into it, Plains of Nod rising dry to the west, and
        // the impassable Flaming Boundary rampart on the far east rim.
        Act::Eden => {
            let mut h = -40.0 * ((x / B).clamp(-1.0, 1.0) * 0.5 + 0.5) // eastward dip
                + 30.0 * ((-x / B).clamp(0.0, 1.0)); // Nod rises west
            for src in EDEN_RIVERS {
                h += channel(p, src, EDEN_CONFLUENCE, 210.0, 55.0);
            }
            // Flaming Boundary: steep rampart past x = 3250.
            if x > 3250.0 {
                h += (x - 3250.0) * 0.9;
            }
            h
        }
        // 02 — City of Enoch: flattened urban shelf with the Ziggurat of
        // Lamech as a terraced plateau to the north-east.
        Act::Enoch => {
            let zig = dome(p, (2100.0, -2100.0), 1100.0, 130.0);
            (zig / 32.0).floor() * 32.0 // terraced megalith steps
        }
        // 03 — Hermon Range: continuous ascent to the Summit of the Oath in
        // the north-west; the summit itself is a flat oath-ground.
        Act::Hermon => {
            let toward = ((-(x + z)) / (2.0 * B)).clamp(-1.0, 1.0); // rises NW
            let ascent = 260.0 * (toward * 0.5 + 0.5).powi(2);
            let summit = dome(p, (-2700.0, -2700.0), 900.0, 160.0);
            ascent + summit.min(120.0) // cap = flat oath-ground
        }
        // 04 — Nephilim Wastes: strip-mined canyons, feeding pits, and the
        // rest quantized into red mesas.
        Act::Nephilim => {
            let mut h = 0.0;
            h += channel(p, (-3200.0, 800.0), (2600.0, 1900.0), 240.0, 55.0);
            h += channel(p, (-1400.0, -3200.0), (600.0, 3200.0), 200.0, 45.0);
            for pit in [(1800.0, -1600.0), (-2200.0, 2400.0), (2600.0, 2800.0)] {
                h -= dome(p, pit, 420.0, 50.0);
            }
            h
        }
        // 05 — Abyssal Basins: the whole land sinks below the coming sea,
        // torn by fissures of the great deep; the Ark plateau at the zone
        // entry stays high and dry.
        Act::Flood => {
            let mut h = -55.0;
            h += dome(p, (0.0, 0.0), 1000.0, 75.0); // Ark construction plateau
            h += channel(p, (-3400.0, -1200.0), (3400.0, 600.0), 190.0, 40.0);
            h += channel(p, (-800.0, 3400.0), (400.0, -3400.0), 170.0, 35.0);
            h
        }
    }
}

/// Water table per act (render height of the translucent water plane):
/// Eden's rivers fill their channels; the Abyssal Basins sit in floodwater.
pub fn water_level(act: Act) -> Option<f32> {
    match act {
        Act::Eden => Some(-16.0),
        Act::Flood => Some(-28.0),
        _ => None,
    }
}

/// New-character start: the Gate of Eden glade flattens like the inn does.
const GATE: (f32, f32) = (2950.0, 0.0);

/// Visual ground height at a server-world position (server y == render z).
pub fn terrain_height(act: Act, x: f32, z: f32) -> f32 {
    let (amp, wl) = act_shape(act);
    let n = fbm(act).get([(x / wl) as f64, (z / wl) as f64]) as f32;
    let mut h = n * amp;
    if h < 0.0 {
        h *= 0.35; // shallow basins, no pits to hide in
    }
    h += landforms(act, x, z);
    let d = (x * x + z * z).sqrt();
    let mut t = ((d - FLAT_RADIUS) / BLEND_DIST).clamp(0.0, 1.0);
    if act == Act::Eden {
        // The Gate of Eden glade (new-character start) is flat too.
        let dg = ((x - GATE.0).powi(2) + (z - GATE.1).powi(2)).sqrt();
        t = t.min(((dg - FLAT_RADIUS) / BLEND_DIST).clamp(0.0, 1.0));
    }
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
    // River/sea beds shade darker below the water table.
    if let Some(w) = water_level(act) {
        if h < w {
            let lin = Color::srgb(0.13, 0.20, 0.16).to_linear();
            return [lin.red, lin.green, lin.blue, 1.0];
        }
    }
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

    /// Doc-driven landforms (docs/locations 01–05) are present where the
    /// docs put them.
    #[test]
    fn location_landforms_exist() {
        // 01: a river channel near the Eden confluence dips below the water
        // table somewhere along each carved river.
        let mut wet = 0;
        for src in EDEN_RIVERS {
            for t in 1..40 {
                let f = t as f32 / 40.0;
                let x = src.0 + (EDEN_CONFLUENCE.0 - src.0) * f;
                let z = src.1 + (EDEN_CONFLUENCE.1 - src.1) * f;
                if terrain_height(Act::Eden, x, z) < water_level(Act::Eden).unwrap() {
                    wet += 1;
                    break;
                }
            }
        }
        assert!(wet >= 3, "at least 3 of Eden's four rivers hold water, got {wet}");
        // 01: the Flaming Boundary rampart rises steeply on the far east rim.
        assert!(
            terrain_height(Act::Eden, 3500.0, 0.0) > terrain_height(Act::Eden, 3000.0, 0.0) + 80.0,
            "flaming-boundary rampart"
        );
        // 02: the Ziggurat of Lamech plateau stands above the Enoch plain.
        assert!(
            terrain_height(Act::Enoch, 2100.0, -2100.0)
                > terrain_height(Act::Enoch, 0.0, 1500.0) + 60.0,
            "ziggurat plateau"
        );
        // 03: Hermon climbs continuously toward the north-west summit.
        let low = terrain_height(Act::Hermon, 2000.0, 2000.0);
        let high = terrain_height(Act::Hermon, -2600.0, -2600.0);
        assert!(high > low + 150.0, "Hermon ascent: {low} -> {high}");
        // 04: the Nephilim strip-mine canyon is carved well below its rim.
        assert!(
            terrain_height(Act::Nephilim, 0.0, 1450.0)
                < terrain_height(Act::Nephilim, 0.0, 2200.0) - 25.0,
            "strip-mine canyon"
        );
        // 05: the Abyssal Basins drown below the floodwater, but the Ark
        // plateau at the entry stays dry.
        let w = water_level(Act::Flood).unwrap();
        assert!(terrain_height(Act::Flood, 2500.0, 1500.0) < w, "basins flooded");
        assert!(terrain_height(Act::Flood, 0.0, 0.0) > w, "Ark plateau stays dry");
    }

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
