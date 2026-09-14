//! Realism pass (2026-09-14): photoscanned splat terrain material.
//!
//! The ground used to be ONE tiled 1K texture multiplied by a flat
//! height-banded vertex palette — at gameplay distance it read as a uniform
//! green carpet. This swaps in an `ExtendedMaterial` (see `terrain.wgsl`) that
//! blends four CC0 Poly Haven layers (grass, dirt, cliff rock, sand) by slope,
//! vertex splat weights and texel height, with mipmaps + 16x anisotropic
//! filtering so the detail survives grazing camera angles instead of
//! shimmering.
//!
//! Textures ship as three vertically stacked JPEG atlases
//! (`assets/textures/terrain/terrain_{diffuse,nor_gl,arm}_array.jpg`, 4 x
//! 1024² layers, built by `scripts/build_terrain_arrays.py`). JPEG has no
//! mips, so they are generated here on load, once.

use bevy::image::{ImageAddressMode, ImageFilterMode, ImageLoaderSettings, ImageSampler, ImageSamplerDescriptor};
use bevy::pbr::{ExtendedMaterial, MaterialExtension};
use bevy::prelude::*;
use bevy::render::render_resource::{AsBindGroup, ShaderRef, ShaderType};

use antediluvia_protocol::Act;

pub type TerrainMaterial = ExtendedMaterial<StandardMaterial, TerrainExt>;

const LAYERS: u32 = 4;

#[derive(Clone, Copy, ShaderType, Debug, Default)]
pub struct TerrainParams {
    pub tint0: Vec4,
    pub tint1: Vec4,
    pub tint2: Vec4,
    pub tint3: Vec4,
    pub tiling: Vec4,
}

#[derive(Asset, AsBindGroup, Reflect, Debug, Clone)]
pub struct TerrainExt {
    #[uniform(100)]
    #[reflect(ignore)]
    pub params: TerrainParams,
    #[texture(101, dimension = "2d_array")]
    #[sampler(102)]
    pub albedo: Handle<Image>,
    #[texture(103, dimension = "2d_array")]
    #[sampler(104)]
    pub normal: Handle<Image>,
    #[texture(105, dimension = "2d_array")]
    #[sampler(106)]
    pub arm: Handle<Image>,
}

impl MaterialExtension for TerrainExt {
    fn fragment_shader() -> ShaderRef {
        "embedded://antediluvia_client_bevy/terrain.wgsl".into()
    }
}

/// Marks the terrain mesh entity whose material is upgraded once the arrays
/// are ready (it spawns with a plain StandardMaterial so the world is never
/// invisible while the atlases decode).
#[derive(Component)]
pub struct SplatGround(pub Act);

/// Rock formation awaiting the photoscanned rock material; the index picks
/// one of `ROCK_TINTS` so all formations batch into a handful of materials.
#[derive(Component)]
pub struct RockSkin(pub usize);

/// GLTF rock scene whose child meshes get the rock material once spawned.
#[derive(Component)]
pub struct RockScene;

pub const ROCK_TINTS: [[f32; 3]; 6] = [
    [0.95, 0.95, 0.95],
    [1.05, 0.98, 0.9],
    [0.85, 0.87, 0.9],
    [1.1, 0.95, 0.82],
    [0.75, 0.75, 0.72],
    [1.0, 1.0, 0.92],
];

#[derive(Resource)]
pub struct TerrainArrays {
    albedo: Handle<Image>,
    normal: Handle<Image>,
    arm: Handle<Image>,
    ready: bool,
    rocks: Vec<Handle<TerrainMaterial>>,
}

pub struct TerrainMaterialPlugin;

impl Plugin for TerrainMaterialPlugin {
    fn build(&self, app: &mut App) {
        bevy::asset::embedded_asset!(app, "terrain.wgsl");
        app.add_plugins(MaterialPlugin::<TerrainMaterial>::default())
            .add_systems(Startup, load_arrays)
            .add_systems(Update, (prepare_arrays, upgrade_ground, skin_rocks, skin_rock_scenes).chain());
    }
}

fn load_arrays(mut commands: Commands, asset_server: Res<AssetServer>) {
    let load = |path: &'static str, srgb: bool| {
        asset_server.load_with_settings(path, move |s: &mut ImageLoaderSettings| {
            s.is_srgb = srgb;
        })
    };
    commands.insert_resource(TerrainArrays {
        albedo: load("textures/terrain/terrain_diffuse_array.jpg", true),
        normal: load("textures/terrain/terrain_nor_gl_array.jpg", false),
        arm: load("textures/terrain/terrain_arm_array.jpg", false),
        ready: false,
        rocks: Vec::new(),
    });
}

/// Once all three atlases are decoded: reinterpret as 4-layer arrays, build a
/// full mip chain per layer, and give them a repeating anisotropic sampler.
fn prepare_arrays(mut arrays: ResMut<TerrainArrays>, mut images: ResMut<Assets<Image>>) {
    if arrays.ready {
        return;
    }
    let handles = [arrays.albedo.clone(), arrays.normal.clone(), arrays.arm.clone()];
    if handles.iter().any(|h| images.get(h).is_none()) {
        return;
    }
    for h in &handles {
        let img = images.get_mut(h).expect("checked above");
        if img.texture_descriptor.size.depth_or_array_layers == 1 {
            img.reinterpret_stacked_2d_as_array(LAYERS);
            build_mips_layer_major(img);
        }
        img.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
            address_mode_u: ImageAddressMode::Repeat,
            address_mode_v: ImageAddressMode::Repeat,
            mag_filter: ImageFilterMode::Linear,
            min_filter: ImageFilterMode::Linear,
            mipmap_filter: ImageFilterMode::Linear,
            anisotropy_clamp: 16,
            ..default()
        });
    }
    arrays.ready = true;
}

/// Box-filter mip chain for an RGBA8 texture array, laid out layer-major
/// (layer 0 mips 0..n, then layer 1 …), which is the order Bevy uploads in.
pub fn build_mips_layer_major(img: &mut Image) {
    let w0 = img.width() as usize;
    let h0 = img.height() as usize;
    let layers = img.texture_descriptor.size.depth_or_array_layers as usize;
    let levels = (w0.max(h0) as f32).log2().floor() as usize + 1;
    let base = std::mem::take(&mut img.data);
    let per = w0 * h0 * 4;
    let mut out = Vec::with_capacity(base.len() * 4 / 3 + 64);
    for l in 0..layers {
        let mut cur = base[l * per..(l + 1) * per].to_vec();
        let (mut w, mut h) = (w0, h0);
        out.extend_from_slice(&cur);
        for _ in 1..levels {
            let (nw, nh) = ((w / 2).max(1), (h / 2).max(1));
            let mut next = vec![0u8; nw * nh * 4];
            for y in 0..nh {
                for x in 0..nw {
                    for c in 0..4 {
                        let mut sum = 0u32;
                        for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                            let sx = (x * 2 + dx).min(w - 1);
                            let sy = (y * 2 + dy).min(h - 1);
                            sum += cur[(sy * w + sx) * 4 + c] as u32;
                        }
                        next[(y * nw + x) * 4 + c] = (sum / 4) as u8;
                    }
                }
            }
            out.extend_from_slice(&next);
            cur = next;
            w = nw;
            h = nh;
        }
    }
    img.data = out;
    img.texture_descriptor.mip_level_count = levels as u32;
}

/// Per-act layer tints. The photos are neutral; these keep Eden lush,
/// Nephilim sun-bleached and Flood cold without repainting textures.
pub fn act_params(act: Act) -> TerrainParams {
    let t = |r: f32, g: f32, b: f32| Vec4::new(r, g, b, 1.0);
    let (grass, dirt, rock, sand) = match act {
        Act::Eden => (t(0.95, 1.05, 0.85), t(1.0, 0.95, 0.9), t(0.95, 0.95, 0.92), t(1.0, 0.97, 0.9)),
        Act::Hermon => (t(0.88, 0.95, 0.85), t(0.95, 0.92, 0.9), t(0.92, 0.94, 0.98), t(0.95, 0.95, 0.95)),
        Act::Nephilim => (t(1.15, 0.98, 0.68), t(1.12, 0.9, 0.72), t(1.05, 0.88, 0.75), t(1.1, 0.95, 0.8)),
        Act::Enoch => (t(1.0, 0.98, 0.82), t(1.0, 0.94, 0.86), t(0.98, 0.95, 0.9), t(1.0, 0.96, 0.88)),
        Act::Flood => (t(0.8, 0.92, 0.88), t(0.85, 0.88, 0.9), t(0.85, 0.9, 0.95), t(0.9, 0.93, 0.95)),
    };
    TerrainParams {
        tint0: grass,
        tint1: dirt,
        tint2: rock,
        tint3: sand,
        // Grass tile ≈ 1.6 character heights, far tile 6x that; cliffs start
        // at ~30° and are full rock by ~48°.
        tiling: Vec4::new(90.0, 540.0, 0.13, 0.33),
    }
}

/// Rocks: the same shader with the cliff layer forced on everywhere
/// (negative slope thresholds) and a tighter triplanar tile.
fn rock_params(tint: [f32; 3]) -> TerrainParams {
    let t = Vec4::new(tint[0], tint[1], tint[2], 1.0);
    TerrainParams { tint0: t, tint1: t, tint2: t, tint3: t, tiling: Vec4::new(22.0, 120.0, -2.0, -1.5) }
}

fn skin_rocks(
    mut commands: Commands,
    mut arrays: ResMut<TerrainArrays>,
    mut mats: ResMut<Assets<TerrainMaterial>>,
    rocks: Query<(Entity, &RockSkin)>,
) {
    if !arrays.ready {
        return;
    }
    if arrays.rocks.is_empty() {
        let (a, n, m) = (arrays.albedo.clone(), arrays.normal.clone(), arrays.arm.clone());
        arrays.rocks = ROCK_TINTS
            .iter()
            .map(|t| {
                mats.add(ExtendedMaterial {
                    base: StandardMaterial { perceptual_roughness: 1.0, reflectance: 0.35, ..default() },
                    extension: TerrainExt { params: rock_params(*t), albedo: a.clone(), normal: n.clone(), arm: m.clone() },
                })
            })
            .collect();
    }
    for (e, r) in &rocks {
        commands
            .entity(e)
            .remove::<(RockSkin, MeshMaterial3d<StandardMaterial>)>()
            .insert(MeshMaterial3d(arrays.rocks[r.0 % arrays.rocks.len()].clone()));
    }
}

fn skin_rock_scenes(
    mut commands: Commands,
    arrays: Res<TerrainArrays>,
    scenes: Query<Entity, With<RockScene>>,
    children: Query<&Children>,
    meshes: Query<(), With<MeshMaterial3d<StandardMaterial>>>,
) {
    if arrays.rocks.is_empty() {
        return;
    }
    for root in &scenes {
        let mut found = false;
        for d in children.iter_descendants(root) {
            if meshes.contains(d) {
                found = true;
                commands.entity(d).insert(RockSkin((root.index() as usize) % ROCK_TINTS.len()));
            }
        }
        if found {
            commands.entity(root).remove::<RockScene>();
        }
    }
}

fn upgrade_ground(
    mut commands: Commands,
    arrays: Res<TerrainArrays>,
    mut mats: ResMut<Assets<TerrainMaterial>>,
    grounds: Query<(Entity, &SplatGround), With<MeshMaterial3d<StandardMaterial>>>,
) {
    if !arrays.ready {
        return;
    }
    for (e, g) in &grounds {
        let mat = mats.add(ExtendedMaterial {
            base: StandardMaterial { perceptual_roughness: 1.0, reflectance: 0.3, ..default() },
            extension: TerrainExt {
                params: act_params(g.0),
                albedo: arrays.albedo.clone(),
                normal: arrays.normal.clone(),
                arm: arrays.arm.clone(),
            },
        });
        commands
            .entity(e)
            .remove::<MeshMaterial3d<StandardMaterial>>()
            .insert(MeshMaterial3d(mat));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::render::render_asset::RenderAssetUsages;
    use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};

    #[test]
    fn mip_chain_is_layer_major_and_sized() {
        let (w, h, layers) = (8usize, 8usize, 2usize);
        let mut data = vec![0u8; w * h * 4 * layers];
        // Layer 1 all white so a layer-major chain keeps mips separate.
        for px in data[w * h * 4..].iter_mut() {
            *px = 255;
        }
        let mut img = Image::new(
            Extent3d { width: w as u32, height: h as u32, depth_or_array_layers: layers as u32 },
            TextureDimension::D2,
            data,
            TextureFormat::Rgba8Unorm,
            RenderAssetUsages::default(),
        );
        build_mips_layer_major(&mut img);
        assert_eq!(img.texture_descriptor.mip_level_count, 4);
        let per_layer = (64 + 16 + 4 + 1) * 4;
        assert_eq!(img.data.len(), per_layer * layers);
        assert!(img.data[..per_layer].iter().all(|&b| b == 0));
        assert!(img.data[per_layer..].iter().all(|&b| b == 255));
    }
}
