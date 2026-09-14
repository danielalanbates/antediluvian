//! Realism pass (2026-09-14): image-based lighting and water.
//!
//! Before this, everything was lit by one sun plus two fake directional
//! "rim" and "bounce" lights and a flat ambient term, so every surface facing
//! away from the sun was the same grey-green. Real outdoor light comes from
//! the whole sky: blue from above, warm bounce from the ground, bright near
//! the sun. This builds a small procedural sky cubemap per act (diffuse +
//! mipmapped specular) from the act's mood colours and hands it to Bevy as an
//! `EnvironmentMapLight`, which also gives water and wet/smooth surfaces real
//! sky reflections with Fresnel.
//!
//! Generated in code (no HDRI download, no KTX2 tooling), ~1.5 MB per act,
//! WebGL2-safe (Rgba8 sRGB cubemaps).

use antediluvia_protocol::Act;
use bevy::image::{ImageAddressMode, ImageFilterMode, ImageSampler, ImageSamplerDescriptor};
use bevy::pbr::environment_map::EnvironmentMapLight;
use bevy::prelude::*;
use bevy::render::render_asset::RenderAssetUsages;
use bevy::render::render_resource::{
    Extent3d, TextureDimension, TextureFormat, TextureViewDescriptor, TextureViewDimension,
};

use crate::atmosphere::act_mood;

const SPEC_SIZE: u32 = 128;
const DIFF_SIZE: u32 = 16;

/// Nominal IBL brightness at noon (cd/m²-ish scale, tuned by screenshot
/// against the 22 klx sun).
pub const ENV_INTENSITY: f32 = 6000.0;

/// Photographic balance: (camera EV100, IBL intensity, noon sun lux). The
/// camera used Bevy's indoor default (EV100 9.7) under a 22 klx sun, so the
/// HDR buffer was blown out and bloom laid a milky veil over everything.
/// `ANTEDILUVIA_LOOK="ev,ibl,sun"` overrides it for screenshot tuning.
pub fn look() -> (f32, f32, f32) {
    let def = (11.9, ENV_INTENSITY, 75_000.0);
    #[cfg(not(target_arch = "wasm32"))]
    if let Ok(v) = std::env::var("ANTEDILUVIA_LOOK") {
        let p: Vec<f32> = v.split(',').filter_map(|x| x.trim().parse().ok()).collect();
        if p.len() == 3 {
            return (p[0], p[1], p[2]);
        }
    }
    def
}

#[derive(Resource, Default)]
pub struct EnvMaps {
    by_act: std::collections::HashMap<Act, (Handle<Image>, Handle<Image>)>,
    applied: Option<Act>,
}

/// Direction for texel (u, v ∈ [-1, 1], v down) on cube face `f` in wgpu
/// order +X, -X, +Y, -Y, +Z, -Z.
fn face_dir(f: usize, u: f32, v: f32) -> Vec3 {
    let d = match f {
        0 => Vec3::new(1.0, -v, -u),
        1 => Vec3::new(-1.0, -v, u),
        2 => Vec3::new(u, 1.0, v),
        3 => Vec3::new(u, -1.0, -v),
        4 => Vec3::new(u, -v, 1.0),
        _ => Vec3::new(-u, -v, -1.0),
    };
    d.normalize()
}

struct Sky {
    zenith: Vec3,
    horizon: Vec3,
    ground: Vec3,
    sun: Vec3,
    sun_dir: Vec3,
}

fn lin(c: Color) -> Vec3 {
    let l = c.to_linear();
    Vec3::new(l.red, l.green, l.blue)
}

fn sky_for(act: Act) -> Sky {
    let m = act_mood(act);
    Sky {
        zenith: lin(m.sky_zenith),
        horizon: lin(m.sky_horizon),
        // Sunlit ground bounce: an olive/earth average of the terrain photos.
        ground: Vec3::new(0.09, 0.085, 0.05),
        sun: lin(m.sun_color),
        sun_dir: Vec3::new(0.35, 0.8, 0.45).normalize(),
    }
}

/// Sharp sky radiance (linear).
fn radiance(s: &Sky, d: Vec3) -> Vec3 {
    let base = if d.y >= 0.0 {
        s.horizon.lerp(s.zenith, d.y.powf(0.55))
    } else {
        s.horizon.lerp(s.ground, (-d.y * 4.0).min(1.0))
    };
    let sd = d.dot(s.sun_dir).max(0.0);
    base + s.sun * (sd.powf(180.0) * 3.0 + sd.powf(8.0) * 0.25)
}

/// Cosine-convolved sky (analytic approximation) — what a matte surface sees.
fn irradiance(s: &Sky, d: Vec3) -> Vec3 {
    let up = (d.y * 0.5 + 0.5).clamp(0.0, 1.0);
    let sky_avg = s.horizon.lerp(s.zenith, 0.45);
    let sun_term = s.sun * (d.dot(s.sun_dir) * 0.5 + 0.5).powf(2.0) * 0.18;
    s.ground.lerp(sky_avg, up.powf(0.8)) + sun_term
}

fn to_srgb8(c: Vec3) -> [u8; 4] {
    let enc = |x: f32| {
        let x = x.clamp(0.0, 1.0);
        let s = if x <= 0.0031308 { x * 12.92 } else { 1.055 * x.powf(1.0 / 2.4) - 0.055 };
        (s * 255.0 + 0.5) as u8
    };
    [enc(c.x), enc(c.y), enc(c.z), 255]
}

fn cube_image(size: u32, levels: u32, f: impl Fn(Vec3, f32) -> Vec3) -> Image {
    let mut data = Vec::new();
    for face in 0..6 {
        for level in 0..levels {
            let sz = (size >> level).max(1);
            let rough = if levels > 1 { level as f32 / (levels - 1) as f32 } else { 1.0 };
            for y in 0..sz {
                for x in 0..sz {
                    let u = (x as f32 + 0.5) / sz as f32 * 2.0 - 1.0;
                    let v = (y as f32 + 0.5) / sz as f32 * 2.0 - 1.0;
                    data.extend_from_slice(&to_srgb8(f(face_dir(face, u, v), rough)));
                }
            }
        }
    }
    let mut img = Image::new(
        Extent3d { width: size, height: size, depth_or_array_layers: 6 },
        TextureDimension::D2,
        data,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    );
    img.texture_descriptor.mip_level_count = levels;
    img.texture_view_descriptor = Some(TextureViewDescriptor {
        dimension: Some(TextureViewDimension::Cube),
        ..default()
    });
    img.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
        mag_filter: ImageFilterMode::Linear,
        min_filter: ImageFilterMode::Linear,
        mipmap_filter: ImageFilterMode::Linear,
        ..default()
    });
    img
}

pub fn build_env_maps(act: Act) -> (Image, Image) {
    let sky = sky_for(act);
    let levels = (SPEC_SIZE as f32).log2() as u32 + 1;
    let spec = cube_image(SPEC_SIZE, levels, |d, r| {
        // Rougher mips fade from the sharp sky toward the irradiance lobe.
        radiance(&sky, d).lerp(irradiance(&sky, d), r.powf(0.7))
    });
    let diff = cube_image(DIFF_SIZE, 1, |d, _| irradiance(&sky, d));
    (diff, spec)
}

/// Keep the camera's environment light matched to the current act and the
/// time of day.
pub fn apply_env_map(
    mut commands: Commands,
    session: Res<crate::Session>,
    mut maps: ResMut<EnvMaps>,
    mut images: ResMut<Assets<Image>>,
    mut cams: Query<(Entity, Option<&mut EnvironmentMapLight>), With<crate::MainCamera>>,
) {
    let act = session.act;
    let angle = (session.time_of_day - 0.25) * std::f32::consts::TAU;
    let day = angle.sin().max(0.0);
    let intensity = look().1 * (0.12 + 0.88 * day);
    for (e, env) in &mut cams {
        match env {
            Some(mut env) if maps.applied == Some(act) => {
                env.intensity = intensity;
            }
            _ => {
                let (diff, spec) = maps
                    .by_act
                    .entry(act)
                    .or_insert_with(|| {
                        let (d, s) = build_env_maps(act);
                        (images.add(d), images.add(s))
                    })
                    .clone();
                commands.entity(e).insert(EnvironmentMapLight {
                    diffuse_map: diff,
                    specular_map: spec,
                    intensity,
                    rotation: Quat::IDENTITY,
                });
                maps.applied = Some(act);
            }
        }
    }
}

// ─── Water ───────────────────────────────────────────────────────────────────

/// Tileable ripple normal map (GL convention), summed sine waves on integer
/// frequencies so it wraps seamlessly.
pub fn ripple_normal_texture() -> Image {
    const N: usize = 256;
    let waves: [(f32, f32, f32, f32); 6] = [
        (3.0, 1.0, 0.9, 0.0),
        (-2.0, 4.0, 0.6, 1.3),
        (5.0, -3.0, 0.4, 2.1),
        (7.0, 6.0, 0.25, 0.7),
        (-9.0, 4.0, 0.18, 4.0),
        (12.0, -11.0, 0.1, 5.2),
    ];
    let mut data = Vec::with_capacity(N * N * 4);
    let tau = std::f32::consts::TAU;
    for y in 0..N {
        for x in 0..N {
            let (u, v) = (x as f32 / N as f32, y as f32 / N as f32);
            let (mut dx, mut dy) = (0.0f32, 0.0f32);
            for (kx, ky, a, p) in waves {
                let c = (tau * (kx * u + ky * v) + p).cos() * a;
                dx += c * kx;
                dy += c * ky;
            }
            let n = Vec3::new(-dx * 0.012, -dy * 0.012, 1.0).normalize();
            data.extend_from_slice(&[
                ((n.x * 0.5 + 0.5) * 255.0) as u8,
                ((n.y * 0.5 + 0.5) * 255.0) as u8,
                ((n.z * 0.5 + 0.5) * 255.0) as u8,
                255,
            ]);
        }
    }
    let mut img = Image::new(
        Extent3d { width: N as u32, height: N as u32, depth_or_array_layers: 1 },
        TextureDimension::D2,
        data,
        TextureFormat::Rgba8Unorm,
        RenderAssetUsages::RENDER_WORLD,
    );
    crate::terrain_material::build_mips_layer_major(&mut img);
    img.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
        address_mode_u: ImageAddressMode::Repeat,
        address_mode_v: ImageAddressMode::Repeat,
        mag_filter: ImageFilterMode::Linear,
        min_filter: ImageFilterMode::Linear,
        mipmap_filter: ImageFilterMode::Linear,
        anisotropy_clamp: 8,
        ..default()
    });
    img
}

/// Marks a water material whose ripples drift over time.
#[derive(Component)]
pub struct RippleWater(pub Handle<StandardMaterial>);

pub fn drift_ripples(time: Res<Time>, q: Query<&RippleWater>, mut mats: ResMut<Assets<StandardMaterial>>) {
    let t = time.elapsed_secs();
    for w in &q {
        if let Some(m) = mats.get_mut(&w.0) {
            m.uv_transform = bevy::math::Affine2::from_scale_angle_translation(
                Vec2::splat(260.0),
                0.3,
                Vec2::new(t * 0.9, t * 0.55),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn env_maps_have_full_mip_chain_and_sky_above_ground() {
        let (diff, spec) = build_env_maps(Act::Eden);
        assert_eq!(spec.texture_descriptor.mip_level_count, 8);
        let expect: usize = (0..8).map(|l| ((128usize >> l).max(1)).pow(2) * 4).sum::<usize>() * 6;
        assert_eq!(spec.data.len(), expect);
        assert_eq!(diff.data.len(), 16 * 16 * 4 * 6);
        let s = sky_for(Act::Eden);
        assert!(irradiance(&s, Vec3::Y).length() > irradiance(&s, -Vec3::Y).length());
    }
}
