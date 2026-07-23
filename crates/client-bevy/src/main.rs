//! Antediluvia — networked Bevy client (3D, WoW-Classic-style presentation).
//!
//! This is a *thin* client: it holds no game logic. It connects to the
//! authoritative server, sends input intents (`Move`/`Attack`/`Cast`), and
//! renders whatever entities the server reports in its per-tick snapshots.
//! All movement, AI, combat and progression happen server-side.
//!
//! Presentation: third-person orbit camera (right-drag to rotate, scroll to
//! zoom), rigged & animated glTF characters (KayKit CC0 packs — adventurers
//! for players/NPCs, skeletons for enemies) with an Idle/Run/Attack animation
//! state machine, low-poly environment, floating health bars, an inn ring at
//! the zone entry, and a class action bar (keys 1/2) once a class is chosen.
//!
//! The server world is top-down 2D; it maps into 3D as (x, height, y).
//!
//! Usage: antediluvia-client-bevy [name] [ws-url]
//!   defaults: name="Adam", url="ws://127.0.0.1:8787"

mod atmosphere;
mod audio;
mod equipment;
mod grass;
mod local;

/// Local player's visual jump arc (v0.5.0): height offset over time.
#[derive(Resource, Default)]
struct PlayerJump { start: Option<f32> }
mod net;
mod perf;
mod terrain;
mod ui;
mod variety;
mod vfx;

use atmosphere::{act_mood, spawn_sky, update_atmosphere, Sun};
use audio::{ambient_system, init_audio_assets, one_shot, AudioAssets, Pool};
use equipment::{apply_loadouts, init_equip_assets, Loadout};
use antediluvia_protocol::{
    Act, CharacterSheet, Class, ClientMsg, DevCmd, EntityKind, EntityState, EventKind, ServerMsg,
};
use terrain::{build_terrain_mesh, terrain_height};
use ui::{spawn_ui, update_banner, update_target_frame, update_ui_frames, update_ui_panels, Cooldowns};
use variety::{
    apply_tints, attach_hair_style, attach_species_parts, formation_color, formation_mesh,
    hair_hue, skin_hue, species_parts_seed, species_stretch, species_variation, HairStyle,
    SpeciesParts, TintCache, TintRig, HAIR_CHOICES, SKIN_CHOICES,
};
use vfx::{init_vfx, pulse_inn_ring, spawn_burst, update_vfx, InnRing, VfxAssets};
use bevy::gltf::GltfAssetLabel;
use bevy::input::keyboard::KeyboardInput;
use bevy::input::mouse::{MouseMotion, MouseWheel};
use bevy::prelude::*;
use net::{start_network, NetRx, NetTx};
use std::collections::{HashMap, HashSet, VecDeque};
use std::f32::consts::FRAC_PI_2;
use std::time::Duration;

fn main() {
    let mut args = std::env::args().skip(1);
    let apple_id = args.next().unwrap_or_else(|| "apple_user_1".into());
    let url_or_name = args.next().unwrap_or_else(|| "ws://127.0.0.1:8787".into());
    let (character_name, url) = if url_or_name.starts_with("ws://") || url_or_name.starts_with("wss://") {
        (None, url_or_name)
    } else {
        (Some(url_or_name), args.next().unwrap_or_else(|| "ws://127.0.0.1:8787".into()))
    };

    // Start the network thread before the app so login is already in flight.
    let display_name = character_name.clone().unwrap_or_else(|| apple_id.clone());
    let apple_id_for_session = apple_id.clone();
    let use_local = url == "local" || std::env::var("ANTEDILUVIA_LOCAL").is_ok();
    let (tx, rx) = if use_local {
        // Embedded single-player: run the World in-process (browser build path).
        // A default character is used unless one is created in the builder.
        local::start_local(display_name.clone(), None)
    } else {
        start_network(url, apple_id, character_name)
    };

    // Asset root: ANTEDILUVIA_ASSETS env override (app bundle), else the
    // workspace-level assets/ dir, independent of cwd.
    let assets_dir = std::env::var("ANTEDILUVIA_ASSETS")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets"))
        .canonicalize()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|_| "assets".into());

    App::new()
        .add_plugins(
            DefaultPlugins
                .set(AssetPlugin { file_path: assets_dir, ..default() })
                .set(WindowPlugin {
                    primary_window: Some(Window {
                        title: "Antediluvia".into(),
                        resolution: (1600.0, 900.0).into(),
                        resizable: true,
                        ..default()
                    }),
                    ..default()
                }),
        )
        .add_plugins(perf::PerfPlugin)
        // Sky.
        .insert_resource(ClearColor(Color::srgb(0.45, 0.62, 0.82)))
        .insert_resource(AmbientLight { color: Color::WHITE, brightness: 300.0 })
        // Higher-resolution shadow atlas for crisp, modern-looking shadows.
        .insert_resource(bevy::pbr::DirectionalLightShadowMap { size: 2048 })
        .insert_resource(tx)
        .insert_non_send_resource(rx)
        .insert_resource(EntityMap::default())
        .insert_resource(Orbit::default())
        .insert_resource(Cooldowns::default())
        .insert_resource(TintCache::default())
        .insert_resource(PlayerJump::default())
        .insert_resource(Session {
            name: display_name,
            apple_id: apple_id_for_session,
            ..default()
        })
        .add_event::<CombatEvt>()
        .add_systems(Startup, (setup, init_vfx, init_equip_assets, init_audio_assets, grass::init_grass))
        .add_systems(
            Update,
            (
                receive_from_server,
                send_input,
                chat_input,
                orbit_camera,
                face_billboards,
                attach_rigs,
                animate_movement,
                trigger_attack_anim,
                builder_screen,
                dev_console,
            ),
        )
        .add_systems(
            Update,
            (
                apply_combat_events,
                update_vfx,
                pulse_inn_ring,
                update_ui_frames,
                update_target_frame,
                update_banner,
                update_ui_panels,
                update_atmosphere,
                grass::update_grass,
                grass::sway_grass,
                animate_water,
                player_jump,
                ambient_system,
                apply_loadouts,
                apply_tints,
                attach_species_parts,
                attach_hair_style,
                smooth_motion,
            ),
        )
        .run();
}

// ─── Components / resources ──────────────────────────────────────────────────

/// A server-owned entity mirrored into the Bevy world. Holds the server id.
#[derive(Component)]
struct ServerEnt(u64);

/// The local player's entity (the one whose server id matches our own).
#[derive(Component)]
struct PlayerTag;

#[derive(Component)]
struct MainCamera;

// (HudText / ActionBarText markers replaced by ui.rs components.)

/// A node that should always face the camera (health-bar holders).
#[derive(Component)]
struct Billboard;

/// Where the server last put this entity (10 Hz); the render transform eases
/// toward it every frame so movement stays smooth between snapshots.
#[derive(Component)]
struct NetTarget {
    pos: Vec3,
    rot: f32,
}

/// On the SceneRoot entity of a character: the animation clips this rig uses.
/// `attach_rigs` finds this by walking up from the scene's `AnimationPlayer`.
#[derive(Component, Clone)]
struct RigClips {
    idle: Handle<AnimationClip>,
    run: Handle<AnimationClip>,
    attack: Handle<AnimationClip>,
    death: Handle<AnimationClip>,
}

/// A `ServerMsg::Event` forwarded out of the network drain for the animation
/// systems (remote swings, casts, deaths).
#[derive(Event)]
struct CombatEvt {
    kind: EventKind,
    src: u64,
    dst: Option<u64>,
}

/// Added to the SceneRoot entity once its `AnimationPlayer` is wired up:
/// graph node indices plus the entity that owns the `AnimationPlayer`.
#[derive(Component)]
struct RigAnim {
    player: Entity,
    idle: AnimationNodeIndex,
    run: AnimationNodeIndex,
    attack: AnimationNodeIndex,
    death: AnimationNodeIndex,
}

/// On a character's root: movement-derived animation state. `rig` points at
/// the SceneRoot entity (which carries `RigClips`/`RigAnim`).
#[derive(Component)]
struct Mover {
    rig: Entity,
    last: Vec3,
    moving: bool,
    /// While `time.elapsed_secs()` is below this, an attack one-shot owns the rig.
    attack_until: f32,
    was_attacking: bool,
}

/// Per-server-entity bookkeeping: scene root (translation only), the rotating
/// model node, and the health-bar fill node.
struct Mirrored {
    root: Entity,
    model: Option<Entity>,
    bar_fill: Option<Entity>,
    /// Spawned wolf scene while the player is mounted (C06).
    mount_model: Option<Entity>,
    /// While `time.elapsed_secs()` is below this, the entity is playing its
    /// death animation — keep the corpse visible even if it left the snapshot.
    dying_until: f32,
}

#[derive(Resource, Default)]
struct EntityMap(HashMap<u64, Mirrored>);

#[derive(Resource)]
pub struct Session {
    pub name: String,
    pub my_id: Option<u64>,
    pub class: Option<Class>,
    /// Act whose terrain is currently built (server world is flat; this only
    /// drives presentation).
    pub act: Act,
    /// Full character sheet from the server (replaces old `hud` string).
    pub sheet: Option<CharacterSheet>,
    /// Rolling chat / notice log (last ~24 lines kept).
    pub chat_log: VecDeque<String>,
    /// Current text being typed (Enter-to-chat).
    pub chat_input: String,
    /// True while the chat input bar is focused.
    pub chat_active: bool,
    /// 0.0 to 1.0 time of day (driven by server).
    pub time_of_day: f32,
    /// Nearest hostile within engage range: (display name, hp, max hp).
    pub target: Option<(String, i32, i32)>,
    /// Entity id of the current target (for targeted actions like Tame).
    pub target_id: Option<u64>,
    /// Big centered announcement (text, seconds remaining).
    pub banner: Option<(String, f32)>,
    /// Apple account id this session logs in with (C13 re-login from builder).
    pub apple_id: String,
    /// Character-builder state; `Some` while the create screen is up (C13).
    pub builder: Option<Builder>,
    /// Account has dev rights (server-advised, C14).
    pub is_dev: bool,
    /// Dev console: `Some(input)` while open (backquote toggles).
    pub dev_input: Option<String>,
    /// Current party roster (P1); empty = not in a party.
    pub party: Vec<String>,
}

/// Character-builder working state (C13).
pub struct Builder {
    pub name: String,
    pub class: Class,
    pub faction: Option<String>,
    pub appearance: [u32; 3],
    pub submitted: bool,
    pub error: Option<String>,
}

impl Default for Builder {
    fn default() -> Self {
        Self {
            name: String::new(),
            class: Class::Warrior,
            faction: None,
            appearance: [0, 0, 0],
            submitted: false,
            error: None,
        }
    }
}

impl Default for Session {
    fn default() -> Self {
        Self {
            name: String::new(),
            my_id: None,
            class: None,
            act: Act::Eden,
            sheet: None,
            chat_log: VecDeque::with_capacity(24),
            chat_input: String::new(),
            chat_active: false,
            time_of_day: 0.5,
            target: None,
            target_id: None,
            apple_id: String::new(),
            is_dev: false,
            dev_input: None,
            banner: None,
            builder: None,
            party: Vec::new(),
        }
    }
}

/// Marker on the act's terrain mesh entity (rebuilt on zone travel).
#[derive(Component)]
struct Terrain;

/// Water surface — animated with a gentle vertical swell for life.
#[derive(Component)]
struct WaterPlane { level: f32 }

/// Bob the water surface so it reads as a living body, not a static pane.
fn animate_water(time: Res<Time>, mut q: Query<(&WaterPlane, &mut Transform)>) {
    let t = time.elapsed_secs();
    for (w, mut tf) in &mut q {
        tf.translation.y = w.level + (t * 0.6).sin() * 1.2;
    }
}

/// Third-person orbit camera state (WoW-style).
#[derive(Resource)]
struct Orbit {
    yaw: f32,
    pitch: f32,
    dist: f32,
}

impl Default for Orbit {
    fn default() -> Self {
        Self { yaw: 0.0, pitch: 0.55, dist: 420.0 }
    }
}

/// Cached meshes + materials for the non-character environment pieces.
#[derive(Resource)]
struct RenderAssets {
    bar: Handle<Mesh>,
    m_bar_bg: Handle<StandardMaterial>,
    m_bar_hp: Handle<StandardMaterial>,
    /// C15 crowd LOD: shared capsule impostor for far players.
    lod_capsule: Handle<Mesh>,
    m_lod: Handle<StandardMaterial>,
}

/// Players farther than this from the local player spawn as capsule
/// impostors with no rig, animation, or nameplate (C15). Chosen at spawn;
/// AoI churn re-evaluates naturally as entities come and go.
const RIG_LOD_RADIUS: f32 = 350.0;

/// Height of a character's health bar above the ground.
const BAR_HEIGHT: f32 = 64.0;
const BAR_WIDTH: f32 = 34.0;

/// World-units scale for the ~1.8-unit-tall KayKit rigs.
const CHAR_SCALE: f32 = 30.0;
/// Boss ("alpha") enemies render half again as large.
const ALPHA_SCALE: f32 = 45.0;

/// Which glTF file + animation indices + scale a snapshot entity renders with.
///
/// Animation indices are stable per pack (verified against the shipped GLBs):
/// adventurers: Idle=36 Running_A=48 1H_slice=1 2H_chop=8 Spellcast_Shoot=62;
/// skeletons:   Idle=40 Running_A=54 1H_slice=2 2H_chop=9 Spellcast_Shoot=77.
fn rig_for(e: &EntityState) -> (&'static str, [usize; 4], f32) {
    const ADV: [usize; 3] = [36, 48, 23]; // idle, run, death
    const SKEL: [usize; 3] = [40, 54, 24];
    match e.kind {
        EntityKind::Player => {
            let (class_file, attack) = match e.tag.as_deref() {
                Some("warrior") => ("models/characters/Barbarian.glb", 8),
                Some("hunter") => ("models/characters/Rogue.glb", 1),
                Some("priest") => ("models/characters/Knight.glb", 62),
                Some("mage") => ("models/characters/Mage.glb", 62),
                _ => ("models/characters/Knight.glb", 1),
            };
            // Builder appearance (C13): body index overrides the rig model.
            let file = match e.appearance.map(|a| a[0]) {
                Some(1) => "models/characters/Barbarian.glb",
                Some(2) => "models/characters/Rogue.glb",
                Some(3) => "models/characters/Mage.glb",
                Some(0) => "models/characters/Knight.glb",
                _ => class_file,
            };
            (file, [ADV[0], ADV[1], attack, ADV[2]], CHAR_SCALE)
        }
        EntityKind::Npc => {
            ("models/characters/Rogue_Hooded.glb", [ADV[0], ADV[1], 1, ADV[2]], CHAR_SCALE)
        }
        EntityKind::Wildlife => {
            // Quaternius Animated Animals. Two clip orderings:
            // herbivores (Alpaca/Bull/Deer): Attack_Headbutt=0 Death=2 Gallop=4 Idle=6
            // predators (Fox/ShibaInu/Wolf): Attack=0 Death=1 Gallop=3 Idle=5
            const HERB: [usize; 4] = [6, 4, 0, 2]; // idle, run, attack, death
            const PRED: [usize; 4] = [5, 3, 0, 1];
            let tag = e.tag.as_deref().unwrap_or("");
            match tag {
                "goat" => ("models/wildlife/Alpaca.gltf", HERB, 22.0),
                "boar" => ("models/wildlife/Bull.gltf", HERB, 26.0),
                "dog" => ("models/wildlife/ShibaInu.gltf", PRED, 20.0),
                "fox" => ("models/wildlife/Fox.gltf", PRED, 20.0),
                "deer" => ("models/wildlife/Deer.gltf", HERB, 24.0),
                // Bestiary species (C03): crude keyword → model mapping.
                t if ["wolf", "hound", "jackal"].iter().any(|k| t.contains(k)) =>
                    ("models/wildlife/Wolf.gltf", PRED, 22.0),
                t if ["cat", "smilodon", "panther", "lion"].iter().any(|k| t.contains(k)) =>
                    ("models/wildlife/Fox.gltf", PRED, 24.0),
                t if ["bear", "mammoth", "mastodon", "behemoth", "bison", "auroch", "bull"].iter().any(|k| t.contains(k)) =>
                    ("models/wildlife/Bull.gltf", HERB, 30.0),
                t if ["goat", "ibex", "alpaca", "camel"].iter().any(|k| t.contains(k)) =>
                    ("models/wildlife/Alpaca.gltf", HERB, 22.0),
                _ => ("models/wildlife/Deer.gltf", HERB, 24.0),
            }
        }
        _ => {
            let tag = e.tag.as_deref().unwrap_or("");
            if tag.ends_with("_alpha") {
                return (
                    "models/enemies/Skeleton_Warrior.glb",
                    [SKEL[0], SKEL[1], 9, SKEL[2]],
                    ALPHA_SCALE,
                );
            }
            // Deterministic variety: hash the species tag onto the minion set.
            let h = tag.bytes().fold(0u32, |a, b| a.wrapping_mul(31).wrapping_add(b as u32));
            let (file, attack) = match h % 3 {
                0 => ("models/enemies/Skeleton_Minion.glb", 2),
                1 => ("models/enemies/Skeleton_Rogue.glb", 2),
                _ => ("models/enemies/Skeleton_Mage.glb", 77),
            };
            (file, [SKEL[0], SKEL[1], attack, SKEL[2]], CHAR_SCALE)
        }
    }
}

/// The two ability keys per class (action-bar slots 1 and 2).
pub fn class_abilities(class: Class) -> [&'static str; 2] {
    match class {
        Class::Warrior => ["heroic_strike", "whirlwind"],
        Class::Hunter => ["aimed_shot", "multi_shot"],
        Class::Priest => ["smite", "heal"],
        Class::Mage => ["firebolt", "frost_nova"],
    }
}

// ─── Props / scenery ─────────────────────────────────────────────────────────

/// Cheap deterministic hash → [0, 1). Used for prop variety so every client
/// renders the same world.
fn hash01(seed: u64) -> f32 {
    let h = seed
        .wrapping_mul(0x9E37_79B9_7F4A_7C15)
        .wrapping_add(0x2545_F491_4F6C_DD1D);
    ((h >> 40) & 0xFF_FFFF) as f32 / 16_777_216.0
}

/// Harvestable tree models per act (lush → pines → dead → parkland → coast).
fn tree_set(act: Act) -> [&'static str; 3] {
    match act {
        Act::Eden => [
            "models/props/nature/tree_single_A.gltf",
            "models/props/nature/tree_single_B.gltf",
            "models/props/nature/trees_A_medium.gltf",
        ],
        Act::Hermon => [
            "models/props/nature/tree_single_B.gltf",
            "models/props/nature/trees_B_small.gltf",
            "models/props/nature/tree_single_A.gltf",
        ],
        Act::Nephilim => [
            "models/props/halloween/tree_dead_large.gltf",
            "models/props/halloween/tree_dead_medium.gltf",
            "models/props/halloween/tree_dead_small.gltf",
        ],
        Act::Enoch => [
            "models/props/nature/tree_single_A.gltf",
            "models/props/nature/tree_single_B.gltf",
            "models/props/nature/trees_B_small.gltf",
        ],
        Act::Flood => [
            "models/props/halloween/tree_dead_small.gltf",
            "models/props/halloween/tree_dead_medium.gltf",
            "models/props/nature/rock_single_C.gltf",
        ],
    }
}

const ROCKS: [&str; 3] = [
    "models/props/nature/rock_single_A.gltf",
    "models/props/nature/rock_single_B.gltf",
    "models/props/nature/rock_single_C.gltf",
];

/// KayKit hexagon props are ~1–2 units across; world characters are ~55u.
const TREE_SCALE: f32 = 34.0;
const ROCK_SCALE: f32 = 26.0;

/// CHUNK_10: scatter decor / formations only render within this band; the
/// fade end sits deep enough in the fog that the pop is invisible, and the
/// far field drops ~hundreds of unique-mesh draw calls per frame.
const DECOR_RANGE: bevy::render::view::VisibilityRange = bevy::render::view::VisibilityRange {
    start_margin: 0.0..0.0,
    end_margin: 2200.0..2600.0,
    use_aabb: false,
};

/// Spawn one static prop scene (no server entity) and return it.
fn spawn_prop(
    commands: &mut Commands,
    asset_server: &AssetServer,
    path: &str,
    pos: Vec3,
    scale: f32,
    yaw: f32,
) -> Entity {
    commands
        .spawn((
            SceneRoot(asset_server.load(GltfAssetLabel::Scene(0).from_asset(path.to_string()))),
            Transform::from_translation(pos)
                .with_scale(Vec3::splat(scale))
                .with_rotation(Quat::from_rotation_y(yaw)),
        ))
        .id()
}

/// Tiling ground detail texture handle (generated once at startup).
#[derive(Resource)]
struct GroundDetail(Handle<Image>);

/// Procedural turf/soil speckle: value noise multiplied over the terrain's
/// vertex-color palette. sRGB, tileable by construction (hash per texel).
fn ground_detail_texture() -> Image {
    use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
    const N: usize = 256;
    let mut data = vec![0u8; N * N * 4];
    for y in 0..N {
        for x in 0..N {
            let h = {
                let s = (x as u64) << 32 | y as u64;
                let mut v = s.wrapping_mul(0x9E37_79B9_7F4A_7C15);
                v ^= v >> 33;
                v = v.wrapping_mul(0xC2B2_AE3D_27D4_EB4F);
                ((v >> 40) & 0xFF) as f32 / 255.0
            };
            // Mostly bright with sparse dark blades/pebbles.
            let v = if h > 0.92 { 0.62 } else { 0.86 + h * 0.14 };
            let g = (v * 255.0) as u8;
            let i = (y * N + x) * 4;
            data[i] = g;
            data[i + 1] = g;
            data[i + 2] = g;
            data[i + 3] = 255;
        }
    }
    let mut img = Image::new(
        Extent3d { width: N as u32, height: N as u32, depth_or_array_layers: 1 },
        TextureDimension::D2,
        data,
        TextureFormat::Rgba8UnormSrgb,
        bevy::render::render_asset::RenderAssetUsages::default(),
    );
    img.sampler = bevy::image::ImageSampler::Descriptor(bevy::image::ImageSamplerDescriptor {
        address_mode_u: bevy::image::ImageAddressMode::Repeat,
        address_mode_v: bevy::image::ImageAddressMode::Repeat,
        ..default()
    });
    img
}

/// Everything act-scoped and purely visual: the terrain mesh, the inn set at
/// the entry, and a deterministic decor scatter. All tagged `Terrain` so zone
/// travel despawns and rebuilds the lot.
fn spawn_act_scenery(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    ground_detail: Handle<Image>,
    materials: &mut Assets<StandardMaterial>,
    asset_server: &AssetServer,
    act: Act,
) {
    // Fidelity (v0.5.0): a tiling procedural detail texture multiplies the
    // vertex-color palette so the ground reads as turf/soil instead of flat
    // polygons.
    commands.spawn((
        Mesh3d(meshes.add(build_terrain_mesh(act))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::WHITE,
            base_color_texture: Some(ground_detail),
            uv_transform: bevy::math::Affine2::from_scale(Vec2::splat(700.0)),
            perceptual_roughness: 1.0,
            ..default()
        })),
        Transform::default(),
        Terrain,
    ));

    // Water plane (doc-driven): Eden's rivers, the Abyssal Basins' floodwater.
    if let Some(level) = terrain::water_level(act) {
        commands.spawn((
            Mesh3d(meshes.add(Plane3d::default().mesh().size(
                antediluvia_protocol::WORLD_BOUNDS * 2.0 + 600.0,
                antediluvia_protocol::WORLD_BOUNDS * 2.0 + 600.0,
            ))),
            MeshMaterial3d(materials.add(StandardMaterial {
                base_color: Color::srgba(0.10, 0.30, 0.44, 0.72),
                alpha_mode: AlphaMode::Blend,
                perceptual_roughness: 0.04,
                metallic: 0.25,
                reflectance: 0.5,
                ..default()
            })),
            Transform::from_xyz(0.0, level, 0.0),
            WaterPlane { level },
            Terrain,
        ));
    }

    // Inn set at the zone entry (flat by construction). Enoch is the city act.
    let inn: &[(&str, Vec3, f32, f32)] = if act == Act::Enoch {
        &[
            ("models/props/city/building_A.gltf", Vec3::new(-130.0, 0.0, -110.0), 40.0, 0.6),
            ("models/props/city/streetlight.gltf", Vec3::new(60.0, 0.0, -50.0), 30.0, 0.0),
            ("models/props/city/bush.gltf", Vec3::new(-40.0, 0.0, 90.0), 70.0, 1.9),
        ]
    } else {
        &[
            ("models/props/village/building_tavern_red.gltf", Vec3::new(-130.0, 0.0, -110.0), 55.0, 0.6),
            ("models/props/village/building_well_red.gltf", Vec3::new(90.0, -6.0, -70.0), 28.0, 0.0),
            ("models/props/city/bush.gltf", Vec3::new(-30.0, 0.0, 140.0), 64.0, 0.8),
            ("models/props/city/bush.gltf", Vec3::new(55.0, 0.0, 135.0), 56.0, 2.4),
        ]
    };
    for (path, pos, scale, yaw) in inn {
        let e = spawn_prop(commands, asset_server, path, *pos, *scale, *yaw);
        commands.entity(e).insert(Terrain);
    }

    // Non-gameplay decor scatter, deterministic per act.
    let act_idx = Act::ALL.iter().position(|a| *a == act).unwrap_or(0) as u64;
    let trees = tree_set(act);
    for i in 0..300u64 { // C05: 4x map area
        let s = act_idx * 100_000 + i;
        let x = (hash01(s * 4 + 1) - 0.5) * antediluvia_protocol::WORLD_BOUNDS * 2.0;
        let z = (hash01(s * 4 + 2) - 0.5) * antediluvia_protocol::WORLD_BOUNDS * 2.0;
        if (x * x + z * z).sqrt() < 300.0 {
            continue; // keep the inn clearing open
        }
        let (path, scale) = match (hash01(s * 4 + 3) * 3.0) as u32 {
            0 => ("models/props/city/bush.gltf", 42.0 + hash01(s * 4) * 24.0),
            1 => (ROCKS[(s % 3) as usize], 10.0 + hash01(s * 4) * 12.0),
            _ => (trees[(s % 3) as usize], TREE_SCALE * (0.55 + hash01(s * 4) * 0.35)),
        };
        let pos = Vec3::new(x, terrain_height(act, x, z), z);
        let e = spawn_prop(commands, asset_server, path, pos, scale, hash01(s * 4 + 5) * 6.283);
        // CHUNK_10: decor fades out into the fog instead of drawing world-wide.
        commands.entity(e).insert((Terrain, DECOR_RANGE));
    }

    // Procedural formations (Alpha-2 A1): 1,200 per act across eight
    // families, every one a unique deformed mesh — thousands of distinct
    // terrain models world-wide. A4 finding: every third site clusters near
    // a POI so the variety is visible where players actually walk.
    let poi_sites: Vec<(f32, f32)> = pois_for_act(act).map(|p| (p.x, p.y)).collect();
    for i in 0..1200u64 {
        let seed = act_idx * 1_000_003 + i * 7919;
        let (x, z) = if i % 3 == 0 && !poi_sites.is_empty() {
            let (px, pz) = poi_sites[(seed / 3 % poi_sites.len() as u64) as usize];
            (
                px + (hash01(seed * 4 + 11) - 0.5) * 900.0,
                pz + (hash01(seed * 4 + 12) - 0.5) * 900.0,
            )
        } else {
            (
                (hash01(seed * 4 + 11) - 0.5) * antediluvia_protocol::WORLD_BOUNDS * 2.0,
                (hash01(seed * 4 + 12) - 0.5) * antediluvia_protocol::WORLD_BOUNDS * 2.0,
            )
        };
        if (x * x + z * z).sqrt() < 320.0 {
            continue; // keep the inn clearing open
        }
        let size = 14.0 + hash01(seed * 4 + 13) * 46.0;
        let y = terrain_height(act, x, z);
        commands.spawn((
            Mesh3d(meshes.add(formation_mesh(seed))),
            MeshMaterial3d(materials.add(StandardMaterial {
                base_color: formation_color(seed),
                perceptual_roughness: 0.95,
                ..default()
            })),
            Transform::from_xyz(x, y, z)
                .with_scale(Vec3::splat(size))
                .with_rotation(Quat::from_rotation_y(hash01(seed * 4 + 14) * 6.283)),
            Terrain,
            DECOR_RANGE,
        ));
    }

    // Cave mouths (C09): two big flanking rocks and a leaning capstone make
    // a readable entrance arch at each cave center.
    for (i, cave) in caves_for_act(act).enumerate() {
        let h = terrain_height(act, cave.x, cave.y);
        let base = Vec3::new(cave.x, h, cave.y);
        let s0 = i as u64 * 13 + 5;
        for (j, (dx, dz, sc, tilt)) in [
            (-55.0f32, 0.0f32, 46.0f32, 0.0f32),
            (55.0, 0.0, 42.0, 0.0),
            (0.0, -8.0, 50.0, 1.35),
        ].iter().enumerate() {
            let e = spawn_prop(
                commands,
                asset_server,
                ROCKS[(i + j) % 3],
                base + Vec3::new(*dx, if *tilt > 0.0 { 34.0 } else { 0.0 }, *dz),
                *sc,
                hash01(s0 + j as u64) * 6.283,
            );
            if *tilt > 0.0 {
                commands.entity(e).insert(Terrain);
                // capstone leans across the gap
                commands.entity(e).entry::<Transform>().and_modify(move |mut t| {
                    t.rotation = Quat::from_rotation_z(*tilt * 0.35) * t.rotation;
                });
            } else {
                commands.entity(e).insert(Terrain);
            }
        }
    }

    // POI cairns (C04): a small stone stack marks each discoverable site.
    for (i, poi) in pois_for_act(act).enumerate() {
        let pos = Vec3::new(poi.x, terrain_height(act, poi.x, poi.y), poi.y);
        let e = spawn_prop(
            commands,
            asset_server,
            ROCKS[i % 3],
            pos,
            14.0,
            hash01(i as u64 * 7 + 3) * 6.283,
        );
        commands.entity(e).insert(Terrain);
    }
}

// ─── POIs (C04): cairn markers + data ────────────────────────────────────────

#[derive(serde::Deserialize)]
struct PoiDef {
    name: String,
    act: String,
    x: f32,
    y: f32,
}

#[derive(serde::Deserialize)]
pub struct CaveDef {
    pub act: String,
    pub x: f32,
    pub y: f32,
}

/// Cave pockets (C09): entrance props here, interior darkening in atmosphere.
pub fn caves_for_act(act: Act) -> impl Iterator<Item = &'static CaveDef> {
    static CAVES: std::sync::OnceLock<Vec<CaveDef>> = std::sync::OnceLock::new();
    let all = CAVES.get_or_init(|| {
        serde_json::from_str(include_str!("../../../assets/data/caves.json"))
            .expect("caves.json parses")
    });
    let key = act.as_str();
    all.iter().filter(move |c| c.act == key)
}

fn pois_for_act(act: Act) -> impl Iterator<Item = &'static PoiDef> {
    static POIS: std::sync::OnceLock<Vec<PoiDef>> = std::sync::OnceLock::new();
    let all = POIS.get_or_init(|| {
        serde_json::from_str(include_str!("../../../assets/data/pois.json"))
            .expect("pois.json parses")
    });
    let key = act.as_str();
    all.iter().filter(move |p| p.act == key)
}

// ─── Systems ─────────────────────────────────────────────────────────────────

fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
    asset_server: Res<AssetServer>,
) {
    let initial_mood = act_mood(Act::Eden);

    // Fidelity pass (v0.5.0): HDR + filmic tonemap + bloom + FXAA + SSAO —
    // modern-MMO image quality on top of the stylized art. MSAA stays Off
    // (retina cost, and SSAO requires it); FXAA covers the edges.
    commands.spawn((
        Camera3d::default(),
        Camera { hdr: true, ..default() },
        bevy::core_pipeline::tonemapping::Tonemapping::TonyMcMapface,
        bevy::core_pipeline::bloom::Bloom::NATURAL,
        bevy::core_pipeline::fxaa::Fxaa::default(),
        bevy::pbr::ShadowFilteringMethod::Gaussian,
        {
            // Filmic grade: gentle contrast S-curve + a touch more saturation
            // so the stylized palette reads with modern punch, not flat.
            let mut cg = bevy::render::view::ColorGrading::default();
            cg.global.exposure = 0.35;          // lift the murk
            cg.global.post_saturation = 1.32;    // vivid, modern palette
            cg.shadows.lift = 0.02;              // open up the blacks a touch
            cg.shadows.gamma = 1.05;
            cg.highlights.gain = 1.06;
            cg.midtones.contrast = 1.12;
            cg
        },
        Transform::from_xyz(0.0, 300.0, 420.0).looking_at(Vec3::ZERO, Vec3::Y),
        DistanceFog {
            color: initial_mood.fog_color,
            falloff: FogFalloff::Exponential { density: initial_mood.fog_density },
            ..default()
        },
        Msaa::Off,
        MainCamera,
    ));

    // Sun.
    commands.spawn((
        DirectionalLight {
            illuminance: 16_000.0,
            shadows_enabled: true,
            shadow_depth_bias: 0.02,
            shadow_normal_bias: 1.8,
            ..default()
        },
        // Crisp near shadows, soft falloff; 4 cascades over ~900 m.
        bevy::pbr::CascadeShadowConfigBuilder {
            num_cascades: 4,
            maximum_distance: 900.0,
            first_cascade_far_bound: 140.0,
            ..default()
        }
        .build(),
        Transform::from_rotation(Quat::from_euler(EulerRot::XYZ, -0.9, 0.6, 0.0)),
        Sun,
    ));

    // Sky.
    spawn_sky(&mut commands, &mut meshes, &mut materials, &mut images, &initial_mood);

    // Terrain + inn + decor (rebuilt on zone travel).
    let ground_detail = images.add(ground_detail_texture());
    commands.insert_resource(GroundDetail(ground_detail.clone()));
    spawn_act_scenery(&mut commands, &mut meshes, ground_detail, &mut materials, &asset_server, Act::Eden);

    // Inn ring at the zone entry (the rest / auction-house area). Pulses.
    let ring_mat = materials.add(StandardMaterial {
        base_color: Color::srgba(0.95, 0.82, 0.30, 0.35),
        alpha_mode: AlphaMode::Blend,
        unlit: true,
        ..default()
    });
    commands.spawn((
        Mesh3d(meshes.add(Cylinder::new(220.0, 0.6))),
        MeshMaterial3d(ring_mat.clone()),
        Transform::from_xyz(0.0, 0.4, 0.0),
        InnRing(ring_mat),
    ));

    let assets = RenderAssets {
        bar: meshes.add(Rectangle::new(1.0, 4.0)),
        m_bar_bg: materials.add(StandardMaterial {
            base_color: Color::srgb(0.10, 0.10, 0.10),
            unlit: true,
            ..default()
        }),
        m_bar_hp: materials.add(StandardMaterial {
            base_color: Color::srgb(0.15, 0.85, 0.20),
            unlit: true,
            ..default()
        }),
        lod_capsule: meshes.add(Capsule3d::new(9.0, 30.0)),
        m_lod: materials.add(StandardMaterial {
            base_color: Color::srgb(0.55, 0.5, 0.42),
            perceptual_roughness: 0.9,
            ..default()
        }),
    };
    commands.insert_resource(assets);

    // WoW-style HUD panels (unit frame, action bar, quest tracker, chat).
    spawn_ui(&mut commands);
}

/// Spawn the 3D rig for a snapshot entity. Returns (root, health-bar fill).
fn spawn_visual(
    commands: &mut Commands,
    assets: &RenderAssets,
    asset_server: &AssetServer,
    e: &EntityState,
    is_me: bool,
    act: Act,
    cheap: bool,
) -> Mirrored {
    let pos = Vec3::new(e.x, terrain_height(act, e.x, e.y), e.y);
    let rot = Quat::from_rotation_y(-e.rot);
    let mut bar_fill = None;
    let mut model = None;

    // Root carries translation only; the model child carries facing, so the
    // health bar (also a root child) can billboard independently.
    let root = commands
        .spawn((Transform::from_translation(pos), Visibility::default(), ServerEnt(e.id)))
        .id();
    match e.kind {
        // C15 crowd LOD: distant players are one shared capsule, no rig,
        // no animation graph, no nameplate.
        EntityKind::Player if cheap => {
            let mut m = Entity::PLACEHOLDER;
            commands.entity(root).with_children(|p| {
                m = p
                    .spawn((
                        Mesh3d(assets.lod_capsule.clone()),
                        MeshMaterial3d(assets.m_lod.clone()),
                        Transform::from_xyz(0.0, 30.0, 0.0).with_rotation(rot),
                    ))
                    .id();
            });
            model = Some(m);
        }
        EntityKind::Player | EntityKind::Enemy | EntityKind::Npc | EntityKind::Wildlife => {
            let (file, [i_idle, i_run, i_attack, i_death], mut scale) = rig_for(e);
            // Visual variety: species-stable tint+scale for mobs, rendered
            // skin/hair choices for players (hundreds of combos each).
            let mut stretch = Vec3::ONE;
            let mut parts = None;
            let tint = match e.kind {
                EntityKind::Enemy | EntityKind::Wildlife => {
                    let tag = e.tag.as_deref().unwrap_or("");
                    let (hue, light, k) = species_variation(tag);
                    scale *= k;
                    // Species-unique geometry: body-plan stretch + grafted
                    // adornment meshes (silhouette, not just tint).
                    stretch = species_stretch(tag);
                    parts = Some(SpeciesParts { seed: species_parts_seed(tag) });
                    Some(TintRig { hue, light, hair_hue: None })
                }
                EntityKind::Player => e.appearance.map(|a| {
                    let (hue, light) = skin_hue(a[1]);
                    TintRig { hue, light, hair_hue: Some(hair_hue(a[2])) }
                }),
                _ => None,
            };
            let clips = RigClips {
                idle: asset_server.load(GltfAssetLabel::Animation(i_idle).from_asset(file)),
                run: asset_server.load(GltfAssetLabel::Animation(i_run).from_asset(file)),
                attack: asset_server.load(GltfAssetLabel::Animation(i_attack).from_asset(file)),
                death: asset_server.load(GltfAssetLabel::Animation(i_death).from_asset(file)),
            };
            let scene = asset_server.load(GltfAssetLabel::Scene(0).from_asset(file));
            let mut m = Entity::PLACEHOLDER;
            let mut rig = Entity::PLACEHOLDER;
            commands.entity(root).with_children(|p| {
                m = p
                    .spawn((Transform::default().with_rotation(rot), Visibility::default()))
                    .with_children(|yaw| {
                        // glTF rigs face +Z; the server's facing convention is
                        // +X, hence the baked quarter-turn.
                        rig = yaw
                            .spawn((
                                SceneRoot(scene),
                                Transform::from_scale(Vec3::splat(scale) * stretch)
                                    .with_rotation(Quat::from_rotation_y(FRAC_PI_2)),
                                clips,
                            ))
                            .id();
                    })
                    .id();
            });
            if let Some(t) = tint {
                commands.entity(rig).insert(t);
            }
            if let Some(p) = parts {
                commands.entity(rig).insert(p);
            }
            // A3: players also grow their chosen hairstyle geometry.
            if e.kind == EntityKind::Player {
                if let Some(a) = e.appearance {
                    commands.entity(rig).insert(HairStyle { style: a[2], hue: hair_hue(a[2]) });
                }
            }
            commands.entity(root).insert(Mover {
                rig,
                last: pos,
                moving: false,
                attack_until: 0.0,
                was_attacking: false,
            });
            model = Some(m);
        }
        EntityKind::Resource => {
            let (path, scale) = if e.tag.as_deref() == Some("rock") {
                (ROCKS[(e.id % 3) as usize], ROCK_SCALE * (0.9 + hash01(e.id) * 0.4))
            } else {
                (tree_set(act)[(e.id % 3) as usize], TREE_SCALE * (0.9 + hash01(e.id) * 0.4))
            };
            let yaw = hash01(e.id * 7 + 1) * 6.283;
            commands.entity(root).with_children(|p| {
                p.spawn((
                    SceneRoot(
                        asset_server.load(GltfAssetLabel::Scene(0).from_asset(path.to_string())),
                    ),
                    Transform::from_scale(Vec3::splat(scale))
                        .with_rotation(Quat::from_rotation_y(yaw)),
                ));
            });
        }
    };

    // Health-bar nameplate for anything that fights or lives.
    if matches!(e.kind, EntityKind::Player | EntityKind::Enemy | EntityKind::Wildlife) {
        let mut fill = Entity::PLACEHOLDER;
        commands.entity(root).with_children(|p| {
            p.spawn((
                Transform::from_xyz(0.0, BAR_HEIGHT, 0.0),
                Visibility::default(),
                Billboard,
            ))
            .with_children(|holder| {
                holder.spawn((
                    Mesh3d(assets.bar.clone()),
                    MeshMaterial3d(assets.m_bar_bg.clone()),
                    Transform::default().with_scale(Vec3::new(BAR_WIDTH + 2.0, 1.3, 1.0)),
                ));
                fill = holder
                    .spawn((
                        Mesh3d(assets.bar.clone()),
                        MeshMaterial3d(assets.m_bar_hp.clone()),
                        Transform::from_xyz(0.0, 0.0, 0.5).with_scale(Vec3::new(BAR_WIDTH, 1.0, 1.0)),
                    ))
                    .id();
            });
        });
        bar_fill = Some(fill);
    }

    if is_me {
        commands.entity(root).insert(PlayerTag);
    }
    Mirrored { root, model, bar_fill, mount_model: None, dying_until: 0.0 }
}

/// "chasm_fiend" → "Chasm Fiend" for the target frame.
fn prettify_tag(tag: &str) -> String {
    tag.split('_')
        .map(|w| {
            let mut c = w.chars();
            match c.next() {
                Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Push a line into the rolling chat log, keeping at most 24 entries.
fn push_chat(session: &mut Session, line: String) {
    if session.chat_log.len() >= 24 {
        session.chat_log.pop_front();
    }
    session.chat_log.push_back(line);
}

/// Drain server messages, reconcile the entity set, update the HUD.
fn receive_from_server(
    mut commands: Commands,
    mut rx: NonSendMut<NetRx>,
    mut map: ResMut<EntityMap>,
    mut session: ResMut<Session>,
    assets: Res<RenderAssets>,
    asset_server: Res<AssetServer>,
    mut transforms: Query<&mut Transform>,
    mut combat: EventWriter<CombatEvt>,
    time: Res<Time>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    terrain_q: Query<Entity, With<Terrain>>,
    loadouts: Query<&Loadout>,
    ground: Res<GroundDetail>,
) {
    // Rebuild the terrain when the character's act changes (login or travel).
    let set_act = |commands: &mut Commands,
                       session: &mut Session,
                       meshes: &mut Assets<Mesh>,
                       materials: &mut Assets<StandardMaterial>,
                       act: Act| {
        if session.act == act {
            return;
        }
        session.act = act;
        for t in terrain_q.iter() {
            commands.entity(t).despawn_recursive();
        }
        spawn_act_scenery(commands, meshes, ground.0.clone(), materials, &asset_server, act);
    };
    let mut latest: Option<Vec<EntityState>> = None;
    while let Ok(msg) = rx.0.try_recv() {
        match msg {
            ServerMsg::Welcome { entity_id, character, is_dev } => {
                session.is_dev = is_dev;
                session.my_id = Some(entity_id);
                session.class = character.class;
                session.builder = None;
                set_act(&mut commands, &mut session, &mut meshes, &mut materials, character.act);
                push_chat(&mut session, format!("Welcome to {}, {}!", character.act.as_str(), character.name));
                session.sheet = Some(character);
            }
            ServerMsg::Stats { character } => {
                // A class choice changes our model: force a respawn of our rig.
                if session.class != character.class {
                    if let Some(m) = session.my_id.and_then(|id| map.0.remove(&id)) {
                        commands.entity(m.root).despawn_recursive();
                    }
                }
                session.class = character.class;
                set_act(&mut commands, &mut session, &mut meshes, &mut materials, character.act);
                session.sheet = Some(character);
            }
            ServerMsg::LoginRejected { reason } => {
                if reason.contains("no character on this account") {
                    // First launch on this Apple account: open the builder (C13).
                    if session.builder.is_none() {
                        session.builder = Some(Builder::default());
                    }
                } else if let Some(b) = session.builder.as_mut() {
                    b.submitted = false;
                    b.error = Some(reason.clone());
                }
                push_chat(&mut session, format!("Login rejected: {reason}"));
            }
            ServerMsg::Notice { text } => {
                if text.starts_with("Discovered:") || text.contains("flagged for PvP") {
                    session.banner = Some((text.clone(), 5.0));
                }
                push_chat(&mut session, text);
            }
            ServerMsg::Chat { from, text } => push_chat(&mut session, format!("{from}: {text}")),
            ServerMsg::Snapshot { time_of_day, entities, .. } => {
                session.time_of_day = time_of_day;
                latest = Some(entities);
            }
            ServerMsg::GuildInfo { name, members } => {
                push_chat(&mut session, format!("<{name}>: {}", members.join(", ")));
            }
            ServerMsg::PartyInfo { members } => {
                session.party = members.clone();
                if members.is_empty() {
                    push_chat(&mut session, "Party disbanded.".into());
                } else {
                    push_chat(&mut session, format!("Party: {}", members.join(", ")));
                }
            }
            ServerMsg::Auctions { listings } => {
                push_chat(&mut session, format!("{} auction lots", listings.len()));
            }
            ServerMsg::Event { kind, src, dst, .. } => {
                combat.send(CombatEvt { kind, src, dst });
            }
            ServerMsg::Pong => {}
        }
    }

    if let Some(entities) = latest {
        let my_id = session.my_id;
        // Target frame: nearest living enemy within engage range.
        let me_pos = entities.iter().find(|e| Some(e.id) == my_id).map(|e| (e.x, e.y));
        let nearest = me_pos.and_then(|(mx, my)| {
            entities
                .iter()
                .filter(|e| e.kind == EntityKind::Enemy && e.health > 0)
                .map(|e| {
                    let d = (e.x - mx).hypot(e.y - my);
                    (d, e)
                })
                .filter(|(d, _)| *d <= 420.0)
                .min_by(|a, b| a.0.total_cmp(&b.0))
                .map(|(_, e)| e)
        });
        session.target_id = nearest.map(|e| e.id);
        session.target = nearest.map(|e| {
            let label = e.name.clone().unwrap_or_else(|| {
                prettify_tag(e.tag.as_deref().unwrap_or("Creature"))
            });
            (label, e.health, e.max_health)
        });
        let mut seen: HashSet<u64> = HashSet::with_capacity(entities.len());
        let mut map_updates: Vec<(u64, Option<Entity>)> = Vec::new();
        for e in &entities {
            seen.insert(e.id);
            let is_me = Some(e.id) == my_id;
            match map.0.get(&e.id) {
                Some(m) => {
                    // Mirror the equipped weapon/chest onto the rig (players only).
                    if e.kind == EntityKind::Player {
                        let lo = Loadout {
                            weapon: e.weapon.clone(),
                            chest: e.chest.clone(),
                            back: e.back.clone(),
                            faction: e.faction.clone(),
                        };
                        if loadouts.get(m.root).map_or(true, |cur| *cur != lo) {
                            commands.entity(m.root).insert(lo);
                        }
                    }
                    // Movement smoothing: record the authoritative target;
                    // `smooth_motion` eases the transform toward it per frame.
                    commands.entity(m.root).insert(NetTarget {
                        pos: Vec3::new(e.x, terrain_height(session.act, e.x, e.y), e.y),
                        rot: e.rot,
                    });
                    if let Some(model) = m.model {
                        if let Ok(mut t) = transforms.get_mut(model) {
                            // Rider sits on the wolf's back while mounted.
                            t.translation.y = if e.mounted { 16.0 } else { 0.0 };
                        }
                    }
                    // Mount model appears/disappears with the flag (C06).
                    match (e.mounted, m.mount_model) {
                        (true, None) => {
                            // Species-appropriate model (C07 keyword map).
                            let sp = e.mount_species.as_deref().unwrap_or("wolf");
                            let (path, scale) = if ["bear", "mammoth", "mastodon", "behemoth", "ox", "auroch", "bull", "bison"]
                                .iter().any(|k| sp.contains(k))
                            {
                                ("models/wildlife/Bull.gltf", 32.0)
                            } else if ["cat", "smilodon", "panther", "lion", "fox"].iter().any(|k| sp.contains(k)) {
                                ("models/wildlife/Fox.gltf", 30.0)
                            } else {
                                ("models/wildlife/Wolf.gltf", 26.0)
                            };
                            let wolf = commands
                                .spawn((
                                    SceneRoot(asset_server.load(
                                        GltfAssetLabel::Scene(0).from_asset(path.to_string()),
                                    )),
                                    Transform::from_scale(Vec3::splat(scale))
                                        .with_rotation(Quat::from_rotation_y(-e.rot + std::f32::consts::FRAC_PI_2)),
                                ))
                                .id();
                            commands.entity(m.root).add_child(wolf);
                            map_updates.push((e.id, Some(wolf)));
                        }
                        (false, Some(wolf)) => {
                            commands.entity(wolf).despawn_recursive();
                            map_updates.push((e.id, None));
                        }
                        _ => {}
                    }
                    if let (Some(fill), true) = (m.bar_fill, e.max_health > 0) {
                        if let Ok(mut t) = transforms.get_mut(fill) {
                            let frac = (e.health.max(0) as f32 / e.max_health as f32).clamp(0.0, 1.0);
                            t.scale.x = BAR_WIDTH * frac;
                            t.translation.x = -(BAR_WIDTH * (1.0 - frac)) * 0.5;
                        }
                    }
                }
                None => {
                    let cheap = e.kind == EntityKind::Player
                        && !is_me
                        && me_pos.map_or(false, |(px, py)| {
                            Vec2::new(e.x - px, e.y - py).length() > RIG_LOD_RADIUS
                        });
                    let m = spawn_visual(&mut commands, &assets, &asset_server, e, is_me, session.act, cheap);
                    if e.kind == EntityKind::Player {
                        commands
                            .entity(m.root)
                            .insert(Loadout {
                                weapon: e.weapon.clone(),
                                chest: e.chest.clone(),
                                back: e.back.clone(),
                                faction: e.faction.clone(),
                            });
                    }
                    map.0.insert(e.id, m);
                }
            }
        }
        for (id, v) in map_updates {
            if let Some(m) = map.0.get_mut(&id) {
                m.mount_model = v;
            }
        }
        // Despawn entities that left the AoI / zone / died — but let anything
        // mid-death-animation linger as a corpse until its timer runs out.
        let now = time.elapsed_secs();
        let gone: Vec<u64> = map
            .0
            .iter()
            .filter(|(id, m)| !seen.contains(id) && now >= m.dying_until)
            .map(|(id, _)| *id)
            .collect();
        for id in gone {
            if let Some(m) = map.0.remove(&id) {
                commands.entity(m.root).despawn_recursive();
            }
        }
    }
}

/// Read the keyboard and send movement intent + attacks + casts. Movement is
/// camera-relative (WoW-style) and only sent when the direction changes.
/// Chat mode (Enter-to-chat) steals all keys while active.
fn send_input(
    keys: Res<ButtonInput<KeyCode>>,
    mut jump: ResMut<PlayerJump>,
    tx: Res<NetTx>,
    orbit: Res<Orbit>,
    session: Res<Session>,
    time: Res<Time>,
    mut cooldowns: ResMut<Cooldowns>,
    mut last_dir: Local<(i8, i8)>,
    mut last_yaw: Local<f32>,
) {
    // While chat is active, game keys are disabled.
    if session.chat_active {
        // Still send zero movement if we were moving.
        if *last_dir != (0, 0) {
            *last_dir = (0, 0);
            tx.send(ClientMsg::Move { dx: 0.0, dy: 0.0 });
        }
        return;
    }

    let now = time.elapsed_secs();

    let mut f = 0i8; // forward/back
    let mut s = 0i8; // strafe
    if keys.pressed(KeyCode::KeyW) || keys.pressed(KeyCode::ArrowUp) {
        f += 1;
    }
    if keys.pressed(KeyCode::KeyS) || keys.pressed(KeyCode::ArrowDown) {
        f -= 1;
    }
    if keys.pressed(KeyCode::KeyA) || keys.pressed(KeyCode::ArrowLeft) {
        s -= 1;
    }
    if keys.pressed(KeyCode::KeyD) || keys.pressed(KeyCode::ArrowRight) {
        s += 1;
    }

    // Re-send when the keys OR the camera yaw changed meaningfully while moving.
    let yaw_moved = (f, s) != (0, 0) && (orbit.yaw - *last_yaw).abs() > 0.03;
    if (f, s) != *last_dir || yaw_moved {
        *last_dir = (f, s);
        *last_yaw = orbit.yaw;
        // Camera forward projected onto the ground, in server coords (x, y=z).
        let fwd = Vec2::new(-orbit.yaw.sin(), -orbit.yaw.cos());
        let right = Vec2::new(-fwd.y, fwd.x);
        let dir = fwd * f as f32 + right * s as f32;
        tx.send(ClientMsg::Move { dx: dir.x, dy: dir.y });
    }
    if keys.just_pressed(KeyCode::Space) {
        tx.send(ClientMsg::Attack);
        cooldowns.trigger(0, now);
        jump.start.get_or_insert(now); // visual hop (ignored mid-air)
    }
    if keys.just_pressed(KeyCode::KeyE) {
        tx.send(ClientMsg::Talk);
    }
    if keys.just_pressed(KeyCode::KeyM) {
        tx.send(ClientMsg::Mount);
    }
    // Tame the current target (C07): needs a lasso and a weakened tameable
    // beast; the server enforces all the gates and replies with a notice.
    if keys.just_pressed(KeyCode::KeyT) {
        if let Some(target) = session.target_id {
            tx.send(ClientMsg::Tame { target });
        }
    }
    if let Some(class) = session.class {
        let [a, b] = class_abilities(class);
        if keys.just_pressed(KeyCode::Digit1) {
            tx.send(ClientMsg::Cast { ability: a.into() });
            cooldowns.trigger(1, now);
        }
        if keys.just_pressed(KeyCode::Digit2) {
            tx.send(ClientMsg::Cast { ability: b.into() });
            cooldowns.trigger(2, now);
        }
    }
    // (Classless in-world F1–F4 pick removed — class is chosen in the
    // character builder now, C13.)
}

/// Marker for builder UI root and rotating preview rig (C13).
#[derive(Component)]
struct BuilderUi;
#[derive(Component)]
struct BuilderPreview {
    look: [u32; 3],
}

/// Character-builder screen (C13): name typing, F1–F4 class, F5 lineage,
/// Left/Right body model, Up/Down skin, F6 hair, Enter to create. Keeps a
/// rotating rig preview in the empty pre-login world.
#[allow(clippy::too_many_arguments)]
fn builder_screen(
    mut commands: Commands,
    keys: Res<ButtonInput<KeyCode>>,
    mut kb_events: EventReader<KeyboardInput>,
    mut session: ResMut<Session>,
    tx: Res<NetTx>,
    time: Res<Time>,
    asset_server: Res<AssetServer>,
    ui_root: Query<Entity, With<BuilderUi>>,
    mut ui_text: Query<&mut Text, With<BuilderUi>>,
    mut preview: Query<(Entity, &BuilderPreview, &mut Transform)>,
) {
    let apple_id = session.apple_id.clone();
    let Some(b) = session.builder.as_mut() else {
        // Builder closed: clear any leftovers (screen + preview).
        for e in &ui_root {
            commands.entity(e).despawn_recursive();
        }
        for (e, _, _) in &preview {
            commands.entity(e).despawn_recursive();
        }
        kb_events.clear();
        return;
    };

    // ── Input ────────────────────────────────────────────────────────────
    for ev in kb_events.read() {
        if !ev.state.is_pressed() {
            continue;
        }
        if let bevy::input::keyboard::Key::Character(ref c) = ev.logical_key {
            if b.name.len() < 24 && c.chars().all(|ch| ch.is_alphanumeric()) {
                b.name.push_str(c);
            }
        } else if ev.logical_key == bevy::input::keyboard::Key::Backspace {
            b.name.pop();
        }
    }
    for (key, class) in [
        (KeyCode::F1, Class::Warrior),
        (KeyCode::F2, Class::Hunter),
        (KeyCode::F3, Class::Priest),
        (KeyCode::F4, Class::Mage),
    ] {
        if keys.just_pressed(key) {
            b.class = class;
        }
    }
    if keys.just_pressed(KeyCode::F5) {
        b.faction = match b.faction.as_deref() {
            None => Some("sethite".into()),
            Some("sethite") => Some("cainite".into()),
            _ => None,
        };
    }
    if keys.just_pressed(KeyCode::ArrowRight) {
        b.appearance[0] = (b.appearance[0] + 1) % 4;
    }
    if keys.just_pressed(KeyCode::ArrowLeft) {
        b.appearance[0] = (b.appearance[0] + 3) % 4;
    }
    if keys.just_pressed(KeyCode::ArrowUp) {
        b.appearance[1] = (b.appearance[1] + 1) % SKIN_CHOICES;
    }
    if keys.just_pressed(KeyCode::ArrowDown) {
        b.appearance[1] = (b.appearance[1] + SKIN_CHOICES - 1) % SKIN_CHOICES;
    }
    if keys.just_pressed(KeyCode::F6) {
        b.appearance[2] = (b.appearance[2] + 1) % HAIR_CHOICES;
    }
    if keys.just_pressed(KeyCode::Enter) && !b.submitted && !b.name.trim().is_empty() {
        b.submitted = true;
        b.error = None;
        tx.send(ClientMsg::Login {
            proto: antediluvia_protocol::PROTOCOL_VERSION,
            apple_id,
            character_name: None,
            create: Some(antediluvia_protocol::CharacterCreate {
                name: b.name.trim().to_string(),
                class: b.class,
                faction: b.faction.clone(),
                appearance: b.appearance,
            }),
        });
    }

    // ── Screen text ──────────────────────────────────────────────────────
    let body_names = ["Knight", "Barbarian", "Rogue", "Mage"];
    let text = format!(
        "CREATE YOUR CHARACTER

         Name: {}_
         Class [F1-F4]: {}   ({})
         Lineage [F5]: {}
         Body [Left/Right]: {}   Skin [Up/Down]: {}   Hair [F6]: {}

         {}
         Press ENTER to walk the earth",
        b.name,
        b.class.as_str(),
        match b.class {
            Class::Warrior => "Savage Strike / War Cry",
            Class::Hunter => "Piercing Shot / Trap",
            Class::Priest => "Mend / Smite",
            Class::Mage => "Firebolt / Frost Nova",
        },
        b.faction.as_deref().unwrap_or("undecided (choose at level 10)"),
        body_names[b.appearance[0] as usize % 4],
        b.appearance[1],
        b.appearance[2],
        match (&b.error, b.submitted) {
            (Some(e), _) => format!("!! {e}"),
            (None, true) => "Creating...".into(),
            _ => String::new(),
        },
    );
    if let Ok(mut t) = ui_text.get_single_mut() {
        **t = text;
    } else if ui_root.is_empty() {
        commands.spawn((
            Text::new(text),
            TextFont { font_size: 22.0, ..default() },
            TextColor(Color::srgb(0.95, 0.9, 0.75)),
            Node {
                position_type: PositionType::Absolute,
                left: Val::Percent(8.0),
                top: Val::Percent(18.0),
                ..default()
            },
            BuilderUi,
        ));
    }

    // ── Rotating preview rig ─────────────────────────────────────────────
    let want = b.appearance;
    let mut have = false;
    for (e, p, mut t) in &mut preview {
        if p.look != want {
            commands.entity(e).despawn_recursive();
        } else {
            have = true;
            t.rotation = Quat::from_rotation_y(time.elapsed_secs() * 0.8);
        }
    }
    if !have {
        let file = [
            "models/characters/Knight.glb",
            "models/characters/Barbarian.glb",
            "models/characters/Rogue.glb",
            "models/characters/Mage.glb",
        ][want[0] as usize % 4];
        let (hue, light) = skin_hue(want[1]);
        commands.spawn((
            SceneRoot(asset_server.load(GltfAssetLabel::Scene(0).from_asset(file))),
            Transform::from_translation(Vec3::new(0.0, 0.0, 0.0))
                .with_scale(Vec3::splat(CHAR_SCALE)),
            BuilderPreview { look: want },
            TintRig { hue, light, hair_hue: Some(hair_hue(want[2])) },
            HairStyle { style: want[2], hue: hair_hue(want[2]) },
        ));
    }
}

/// Dev console (C14): backquote toggles a one-line command input on dev
/// accounts. Commands: tp X Y | give ITEM [N] | level N | heal | spawn TAG |
/// kill | god | time T | travel ACT. Server re-checks authorization.
fn dev_console(
    mut kb_events: EventReader<KeyboardInput>,
    keys: Res<ButtonInput<KeyCode>>,
    mut session: ResMut<Session>,
    tx: Res<NetTx>,
) {
    if !session.is_dev {
        return;
    }
    if keys.just_pressed(KeyCode::Backquote) {
        session.dev_input = match session.dev_input {
            None => Some(String::new()),
            Some(_) => None,
        };
        kb_events.clear();
        let msg = if session.dev_input.is_some() {
            "[dev] console open — tp X Y | give ITEM N | level N | heal | spawn TAG | kill | god | time T | travel ACT"
        } else {
            "[dev] console closed"
        };
        push_chat(&mut session, msg.into());
        return;
    }
    if session.dev_input.is_none() {
        return;
    }
    let mut submit = false;
    for ev in kb_events.read() {
        if !ev.state.is_pressed() {
            continue;
        }
        let inp = session.dev_input.as_mut().unwrap();
        match &ev.logical_key {
            bevy::input::keyboard::Key::Character(c) if c != "`" => inp.push_str(c),
            bevy::input::keyboard::Key::Space => inp.push(' '),
            bevy::input::keyboard::Key::Backspace => { inp.pop(); }
            bevy::input::keyboard::Key::Enter => submit = true,
            _ => {}
        }
    }
    if !submit {
        return;
    }
    let line = session.dev_input.take().unwrap_or_default();
    let parts: Vec<&str> = line.split_whitespace().collect();
    let cmd = match parts.as_slice() {
        ["tp", x, y] => x.parse().ok().zip(y.parse().ok()).map(|(x, y)| DevCmd::Teleport { x, y }),
        ["give", item] => Some(DevCmd::GiveItem { item: item.to_string(), n: 1 }),
        ["give", item, n] => n.parse().ok().map(|n| DevCmd::GiveItem { item: item.to_string(), n }),
        ["level", n] => n.parse().ok().map(|level| DevCmd::SetLevel { level }),
        ["heal"] => Some(DevCmd::Heal),
        ["spawn", tag] => Some(DevCmd::SpawnMob { tag: tag.to_string() }),
        ["kill"] => Some(DevCmd::KillTarget),
        ["god"] => Some(DevCmd::Godmode),
        ["time", t] => t.parse().ok().map(|t| DevCmd::TimeOfDay { t }),
        ["travel", a] => {
            let act = match *a {
                "eden" => Some(Act::Eden),
                "hermon" => Some(Act::Hermon),
                "nephilim" => Some(Act::Nephilim),
                "enoch" => Some(Act::Enoch),
                "flood" => Some(Act::Flood),
                _ => None,
            };
            if let Some(act) = act {
                tx.send(ClientMsg::Travel { act });
            }
            None
        }
        _ => None,
    };
    match cmd {
        Some(c) => tx.send(ClientMsg::Dev { cmd: c }),
        None if !line.trim().is_empty() && !line.starts_with("travel") => {
            push_chat(&mut session, format!("[dev] bad command: {line}"));
        }
        None => {}
    }
}

/// Enter-to-chat: toggle chat mode, receive character input, send on Enter.
fn chat_input(
    keys: Res<ButtonInput<KeyCode>>,
    mut kb_events: EventReader<KeyboardInput>,
    mut session: ResMut<Session>,
    tx: Res<NetTx>,
) {
    if session.builder.is_some() || session.dev_input.is_some() {
        return; // builder (C13) or dev console (C14) owns the keyboard
    }
    if keys.just_pressed(KeyCode::Enter) {
        if session.chat_active {
            // Send the message if non-empty, then close chat.
            let text = session.chat_input.trim().to_string();
            if !text.is_empty() {
                // Slash commands (P1): party management from the chat box.
                if let Some(name) = text.strip_prefix("/party ") {
                    tx.send(ClientMsg::PartyInvite { player: name.trim().to_string() });
                } else if text == "/paccept" {
                    tx.send(ClientMsg::PartyAccept);
                } else if text == "/pleave" {
                    tx.send(ClientMsg::PartyLeave);
                } else if let Some(it) = text.strip_prefix("/bank ") {
                    tx.send(ClientMsg::BankDeposit { item: it.trim().to_string() });
                } else if let Some(it) = text.strip_prefix("/unbank ") {
                    tx.send(ClientMsg::BankWithdraw { item: it.trim().to_string() });
                } else if let Some(n) = text.strip_prefix("/bankgold ") {
                    if let Ok(a) = n.trim().parse::<i64>() {
                        tx.send(ClientMsg::BankGold { amount: a });
                    }
                } else if let Some(n) = text.strip_prefix("/unbankgold ") {
                    if let Ok(a) = n.trim().parse::<i64>() {
                        tx.send(ClientMsg::BankGold { amount: -a });
                    }
                } else if let Some(rest) = text.strip_prefix("/give ") {
                    // /give <player> <item>
                    let mut parts = rest.trim().splitn(2, ' ');
                    if let (Some(p2), Some(it)) = (parts.next(), parts.next()) {
                        tx.send(ClientMsg::TradeGive { player: p2.to_string(), item: it.trim().to_string() });
                    }
                } else if let Some(rest) = text.strip_prefix("/givegold ") {
                    let mut parts = rest.trim().splitn(2, ' ');
                    if let (Some(p2), Some(Ok(a))) = (parts.next(), parts.next().map(|n| n.trim().parse::<u32>())) {
                        tx.send(ClientMsg::TradeGold { player: p2.to_string(), amount: a });
                    }
                } else if let Some(rest) = text.strip_prefix("/mail ") {
                    // /mail <player> <item>  |  /mailgold <player> <n>
                    let mut parts = rest.trim().splitn(2, ' ');
                    if let (Some(p2), Some(it)) = (parts.next(), parts.next()) {
                        tx.send(ClientMsg::MailSend { to: p2.to_string(), item: Some(it.trim().to_string()), gold: 0 });
                    }
                } else if let Some(rest) = text.strip_prefix("/mailgold ") {
                    let mut parts = rest.trim().splitn(2, ' ');
                    if let (Some(p2), Some(Ok(a))) = (parts.next(), parts.next().map(|n| n.trim().parse::<u32>())) {
                        tx.send(ClientMsg::MailSend { to: p2.to_string(), item: None, gold: a });
                    }
                } else if text == "/mailcheck" {
                    tx.send(ClientMsg::MailCheck);
                } else if text == "/sethome" {
                    tx.send(ClientMsg::SetHome);
                } else {
                    tx.send(ClientMsg::Chat { text });
                }
            }
            session.chat_input.clear();
            session.chat_active = false;
        } else {
            session.chat_active = true;
        }
        return;
    }
    if keys.just_pressed(KeyCode::Escape) && session.chat_active {
        session.chat_input.clear();
        session.chat_active = false;
        return;
    }
    if !session.chat_active {
        // Drain so they don't pile up.
        kb_events.clear();
        return;
    }
    // Backspace.
    if keys.just_pressed(KeyCode::Backspace) {
        session.chat_input.pop();
    }
    // Character input via KeyboardInput logical_key.
    for ev in kb_events.read() {
        if !ev.state.is_pressed() {
            continue;
        }
        if let bevy::input::keyboard::Key::Character(ref s) = ev.logical_key {
            for ch in s.chars() {
                if !ch.is_control() {
                    session.chat_input.push(ch);
                }
            }
        } else if ev.logical_key == bevy::input::keyboard::Key::Space {
            session.chat_input.push(' ');
        }
    }
}

/// WoW-style third-person camera: right-drag orbits, wheel zooms, always
/// looking at the player.
fn orbit_camera(
    buttons: Res<ButtonInput<MouseButton>>,
    mut motion: EventReader<MouseMotion>,
    mut wheel: EventReader<MouseWheel>,
    mut orbit: ResMut<Orbit>,
    mut cam: Query<&mut Transform, (With<MainCamera>, Without<PlayerTag>)>,
    player: Query<&Transform, With<PlayerTag>>,
) {
    // Right-button (or two-finger click-hold) drag orbits both axes.
    if buttons.pressed(MouseButton::Right) || buttons.pressed(MouseButton::Middle) {
        for m in motion.read() {
            orbit.yaw -= m.delta.x * 0.005;
            orbit.pitch = (orbit.pitch + m.delta.y * 0.004).clamp(0.08, 1.35);
        }
    } else {
        motion.clear();
    }
    // Trackpad: two-finger swipe. Horizontal turns the camera (standard),
    // vertical zooms. Pixel-unit scrolls (trackpad) are small, so scale up.
    for w in wheel.read() {
        let k = match w.unit {
            bevy::input::mouse::MouseScrollUnit::Line => (1.0, 30.0),
            bevy::input::mouse::MouseScrollUnit::Pixel => (0.02, 1.2),
        };
        orbit.yaw -= w.x * k.0;
        orbit.dist = (orbit.dist - w.y * k.1).clamp(140.0, 900.0);
    }

    let Ok(mut cam_t) = cam.get_single_mut() else { return };
    let target = match player.get_single() {
        Ok(t) => t.translation + Vec3::Y * 26.0,
        Err(_) => Vec3::ZERO,
    };
    let offset = Vec3::new(
        orbit.dist * orbit.pitch.cos() * orbit.yaw.sin(),
        orbit.dist * orbit.pitch.sin(),
        orbit.dist * orbit.pitch.cos() * orbit.yaw.cos(),
    );
    *cam_t = Transform::from_translation(target + offset).looking_at(target, Vec3::Y);
}

/// When a spawned glTF scene's `AnimationPlayer` appears, walk up its ancestry
/// to the `RigClips` scene root, build a three-node animation graph
/// (idle/run/attack), start Idle looping, and record the node indices so the
/// movement/attack systems can drive the rig.
fn attach_rigs(
    mut commands: Commands,
    mut added: Query<(Entity, &mut AnimationPlayer), Added<AnimationPlayer>>,
    parents: Query<&Parent>,
    clips: Query<&RigClips>,
    mut graphs: ResMut<Assets<AnimationGraph>>,
) {
    for (ent, mut player) in added.iter_mut() {
        // Ascend to the SceneRoot entity carrying this rig's clip handles.
        let mut cur = ent;
        let rig_ent = loop {
            if clips.get(cur).is_ok() {
                break Some(cur);
            }
            match parents.get(cur) {
                Ok(p) => cur = p.get(),
                Err(_) => break None,
            }
        };
        let Some(rig_ent) = rig_ent else { continue };
        let Ok(rc) = clips.get(rig_ent) else { continue };

        let (graph, nodes) = AnimationGraph::from_clips([
            rc.idle.clone(),
            rc.run.clone(),
            rc.attack.clone(),
            rc.death.clone(),
        ]);
        let mut transitions = AnimationTransitions::new();
        transitions.play(&mut player, nodes[0], Duration::ZERO).repeat();
        // The scene can be despawned (AoI cull / zone travel) in the same frame
        // its AnimationPlayer appears — try_insert instead of panicking (B0003).
        commands
            .entity(ent)
            .try_insert((AnimationGraphHandle(graphs.add(graph)), transitions));
        commands.entity(rig_ent).try_insert(RigAnim {
            player: ent,
            idle: nodes[0],
            run: nodes[1],
            attack: nodes[2],
            death: nodes[3],
        });
    }
}

/// Crossfade each character between Idle and Running based on how far its
/// root actually moved (server-authoritative positions), unless an attack
/// one-shot currently owns the rig.
fn animate_movement(
    time: Res<Time>,
    mut movers: Query<(&Transform, &mut Mover), With<ServerEnt>>,
    rigs: Query<&RigAnim>,
    mut players: Query<(&mut AnimationPlayer, &mut AnimationTransitions)>,
) {
    let now = time.elapsed_secs();
    for (t, mut mv) in movers.iter_mut() {
        let moved = (t.translation - mv.last).length();
        mv.last = t.translation;
        let Ok(rig) = rigs.get(mv.rig) else { continue };
        let Ok((mut player, mut trans)) = players.get_mut(rig.player) else { continue };

        if now < mv.attack_until {
            mv.was_attacking = true;
            continue;
        }
        let want_run = moved > 0.05;
        if want_run != mv.moving || mv.was_attacking {
            mv.moving = want_run;
            mv.was_attacking = false;
            let node = if want_run { rig.run } else { rig.idle };
            trans
                .play(&mut player, node, Duration::from_millis(150))
                .repeat();
        }
    }
}

/// Play the local player's attack animation as a one-shot when an attack or
/// cast key is pressed. (Remote attacks aren't evented by the server yet —
/// documented as art-chunk follow-up work.)
fn trigger_attack_anim(
    keys: Res<ButtonInput<KeyCode>>,
    time: Res<Time>,
    mut me: Query<&mut Mover, With<PlayerTag>>,
    rigs: Query<&RigAnim>,
    mut players: Query<(&mut AnimationPlayer, &mut AnimationTransitions)>,
    mut commands: Commands,
    mut audio: ResMut<AudioAssets>,
) {
    let swung = keys.just_pressed(KeyCode::Space)
        || keys.just_pressed(KeyCode::Digit1)
        || keys.just_pressed(KeyCode::Digit2);
    if !swung {
        return;
    }
    // UI click for the bar press itself (C12).
    one_shot(&mut commands, &mut audio, Pool::Click, 0.0);
    let Ok(mut mv) = me.get_single_mut() else { return };
    let Ok(rig) = rigs.get(mv.rig) else { return };
    let Ok((mut player, mut trans)) = players.get_mut(rig.player) else { return };
    trans.play(&mut player, rig.attack, Duration::from_millis(40));
    mv.attack_until = time.elapsed_secs() + 0.7;
}

/// Apply the local player's visual jump arc to their model child (v0.5.0).
/// A ~0.5s parabola up to ~26 units; resets when it lands.
fn player_jump(
    time: Res<Time>,
    mut jump: ResMut<PlayerJump>,
    q_root: Query<&Children, With<PlayerTag>>,
    mut q_tf: Query<&mut Transform>,
) {
    let Ok(children) = q_root.get_single() else { return };
    let Some(&model) = children.iter().next() else { return };
    let Ok(mut t) = q_tf.get_mut(model) else { return };
    const DUR: f32 = 0.5;
    const PEAK: f32 = 26.0;
    match jump.start {
        Some(t0) => {
            let e = time.elapsed_secs() - t0;
            if e >= DUR {
                jump.start = None;
                t.translation.y = 0.0;
            } else {
                let p = e / DUR; // 0..1
                t.translation.y = PEAK * (4.0 * p * (1.0 - p)); // parabola
            }
        }
        None => {
            if t.translation.y != 0.0 {
                t.translation.y = 0.0;
            }
        }
    }
}

/// Animate remote entities from server combat events: swings/casts play the
/// attack one-shot, deaths play the death one-shot and pin the corpse in place
/// for a moment before the despawn logic reclaims it. The local player's own
/// Attack/Cast events are skipped — `trigger_attack_anim` already played them
/// instantly on key-press.
fn apply_combat_events(
    time: Res<Time>,
    mut commands: Commands,
    mut evs: EventReader<CombatEvt>,
    session: Res<Session>,
    mut map: ResMut<EntityMap>,
    mut movers: Query<&mut Mover>,
    rigs: Query<&RigAnim>,
    mut players: Query<(&mut AnimationPlayer, &mut AnimationTransitions)>,
    transforms: Query<&Transform>,
    vfx: Res<VfxAssets>,
    mut audio: ResMut<AudioAssets>,
    cam: Query<&Transform, (With<MainCamera>, Without<Billboard>)>,
) {
    let now = time.elapsed_secs();
    let cam_pos = cam.get_single().map(|t| t.translation).unwrap_or(Vec3::ZERO);
    // World position of a mirrored entity's root (chest height for bursts).
    let pos_of = |map: &EntityMap, id: u64, transforms: &Query<&Transform>| {
        map.0
            .get(&id)
            .and_then(|m| transforms.get(m.root).ok())
            .map(|t| t.translation + Vec3::Y * 30.0)
    };
    for ev in evs.read() {
        // One-shot audio riding the same events (C12).
        {
            let at = pos_of(&map, ev.src, &transforms)
                .or_else(|| ev.dst.and_then(|d| pos_of(&map, d, &transforms)));
            let dist = at.map(|p| p.distance(cam_pos)).unwrap_or(600.0);
            let pool = match ev.kind {
                EventKind::Attack => Pool::Attack,
                EventKind::Cast => Pool::Cast,
                EventKind::Hit => Pool::Hit,
                EventKind::Die => Pool::Die,
                EventKind::LevelUp => Pool::LevelUp,
            };
            one_shot(&mut commands, &mut audio, pool, dist);
        }
        // Purely visual bursts (also for our own events — they read well).
        match ev.kind {
            EventKind::Cast => {
                if let Some(p) = pos_of(&map, ev.src, &transforms) {
                    spawn_burst(&mut commands, &vfx, vfx.cast.clone(), p, 22, 120.0, 0.65, 0.3);
                }
            }
            EventKind::Hit => {
                if let Some(id) = ev.dst {
                    if let Some(p) = pos_of(&map, id, &transforms) {
                        spawn_burst(&mut commands, &vfx, vfx.hit.clone(), p, 14, 100.0, 0.45, 0.2);
                    }
                }
            }
            EventKind::Die => {
                if let Some(p) = pos_of(&map, ev.src, &transforms) {
                    spawn_burst(&mut commands, &vfx, vfx.die.clone(), p, 18, 70.0, 0.7, 0.5);
                }
            }
            EventKind::LevelUp => {
                if let Some(p) = pos_of(&map, ev.src, &transforms) {
                    // Gold column: strong upward bias, slow fade.
                    spawn_burst(&mut commands, &vfx, vfx.levelup.clone(), p, 26, 150.0, 1.0, 0.85);
                }
                continue; // no rig change
            }
            EventKind::Attack => {}
        }
        if session.my_id == Some(ev.src)
            && matches!(ev.kind, EventKind::Attack | EventKind::Cast)
        {
            continue;
        }
        let Some(m) = map.0.get_mut(&ev.src) else { continue };
        if ev.kind == EventKind::Die {
            m.dying_until = now + 1.5;
        }
        let Ok(mut mv) = movers.get_mut(m.root) else { continue };
        let Ok(rig) = rigs.get(mv.rig) else { continue };
        let Ok((mut player, mut trans)) = players.get_mut(rig.player) else { continue };
        match ev.kind {
            EventKind::Attack | EventKind::Cast => {
                trans.play(&mut player, rig.attack, Duration::from_millis(100));
                mv.attack_until = now + 0.9;
            }
            EventKind::Die => {
                trans.play(&mut player, rig.death, Duration::from_millis(80));
                // Longer than the corpse-linger so movement never re-takes the rig.
                mv.attack_until = now + 2.5;
            }
            EventKind::Hit | EventKind::LevelUp => {}
        }
    }
}

/// Ease every networked transform toward its latest server target (C15's
/// 10 Hz snapshots would otherwise stutter at snapshot rate). Exponential
/// smoothing ~14/s ≈ dead-on within a frame or two without rubber-banding;
/// rotation slerps on the model child.
fn smooth_motion(
    time: Res<Time>,
    mut roots: Query<(&NetTarget, &mut Transform, &ServerEnt)>,
    map: Res<EntityMap>,
    mut models: Query<&mut Transform, Without<NetTarget>>,
) {
    let k = 1.0 - (-14.0 * time.delta_secs()).exp();
    for (target, mut t, ent) in &mut roots {
        let d = target.pos - t.translation;
        if d.length_squared() > 200.0 * 200.0 {
            t.translation = target.pos; // teleport/travel: snap, don't glide
        } else {
            t.translation += d * k;
        }
        if let Some(m) = map.0.get(&ent.0).and_then(|m| m.model) {
            if let Ok(mut mt) = models.get_mut(m) {
                let want = Quat::from_rotation_y(-target.rot);
                mt.rotation = mt.rotation.slerp(want, k);
            }
        }
    }
}

/// Keep health bars facing the camera. Bars are children of translation-only
/// roots, so a plain camera-yaw rotation is exact.
fn face_billboards(orbit: Res<Orbit>, mut plates: Query<&mut Transform, With<Billboard>>) {
    let want = Quat::from_rotation_y(orbit.yaw);
    for mut t in plates.iter_mut() {
        t.rotation = want;
    }
}
