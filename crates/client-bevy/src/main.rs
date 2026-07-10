//! Antediluvia — networked Bevy client (3D, WoW-Classic-style presentation).
//!
//! This is a *thin* client: it holds no game logic. It connects to the
//! authoritative server, sends input intents (`Move`/`Attack`/`Cast`), and
//! renders whatever entities the server reports in its per-tick snapshots.
//! All movement, AI, combat and progression happen server-side.
//!
//! Presentation: third-person orbit camera (right-drag to rotate, scroll to
//! zoom), low-poly 3D world (capsule characters, cone-canopy trees, boulder
//! rocks), floating health bars over every living thing, an inn ring at the
//! zone entry, and a class action bar (keys 1/2) once a class is chosen.
//!
//! The server world is top-down 2D; it maps into 3D as (x, height, y).
//!
//! Usage: antediluvia-client-bevy [name] [ws-url]
//!   defaults: name="Adam", url="ws://127.0.0.1:8787"

mod net;

use antediluvia_protocol::{Class, ClientMsg, EntityKind, EntityState, ServerMsg};
use bevy::input::mouse::{MouseMotion, MouseWheel};
use bevy::prelude::*;
use net::{start_network, NetRx, NetTx};
use std::collections::{HashMap, HashSet};

fn main() {
    let mut args = std::env::args().skip(1);
    let name = args.next().unwrap_or_else(|| "Adam".into());
    let url = args.next().unwrap_or_else(|| "ws://127.0.0.1:8787".into());

    // Start the network thread before the app so login is already in flight.
    let (tx, rx) = start_network(url, name.clone());

    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "Antediluvia".into(),
                resolution: (1600.0, 900.0).into(),
                resizable: true,
                ..default()
            }),
            ..default()
        }))
        // Sky.
        .insert_resource(ClearColor(Color::srgb(0.45, 0.62, 0.82)))
        .insert_resource(AmbientLight { color: Color::WHITE, brightness: 300.0 })
        .insert_resource(tx)
        .insert_non_send_resource(rx)
        .insert_resource(EntityMap::default())
        .insert_resource(Orbit::default())
        .insert_resource(Session { name, ..default() })
        .add_systems(Startup, setup)
        .add_systems(
            Update,
            (
                receive_from_server,
                send_input,
                orbit_camera,
                face_billboards,
                animate_walk,
            ),
        )
        .run();
}

// ─── Components / resources ──────────────────────────────────────────────────

/// A server-owned entity mirrored into the Bevy world. Holds the server id.
#[derive(Component)]
struct ServerEnt(#[allow(dead_code)] u64);

/// The local player's entity (the one whose server id matches our own).
#[derive(Component)]
struct PlayerTag;

#[derive(Component)]
struct MainCamera;

#[derive(Component)]
struct HudText;

#[derive(Component)]
struct ActionBarText;

/// A node that should always face the camera (health-bar holders).
#[derive(Component)]
struct Billboard;

/// Articulated limb holders for a humanoid rig (pivot at shoulder/hip).
#[derive(Component)]
struct Limbs {
    arm_l: Entity,
    arm_r: Entity,
    leg_l: Entity,
    leg_r: Entity,
}

/// Walk-cycle state; phase advances with distance travelled.
#[derive(Component, Default)]
struct Walker {
    phase: f32,
    last: Vec3,
}

/// Per-server-entity bookkeeping: scene root (translation only), the rotating
/// model node, and the health-bar fill node.
struct Mirrored {
    root: Entity,
    model: Option<Entity>,
    bar_fill: Option<Entity>,
}

#[derive(Resource, Default)]
struct EntityMap(HashMap<u64, Mirrored>);

#[derive(Resource, Default)]
struct Session {
    name: String,
    my_id: Option<u64>,
    class: Option<Class>,
    hud: String,
    notice: String,
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

/// Cached meshes + materials so we don't allocate per entity.
#[derive(Resource)]
struct RenderAssets {
    torso: Handle<Mesh>,
    head: Handle<Mesh>,
    arm: Handle<Mesh>,
    leg: Handle<Mesh>,
    beast: Handle<Mesh>,
    trunk: Handle<Mesh>,
    canopy: Handle<Mesh>,
    rock: Handle<Mesh>,
    nose: Handle<Mesh>,
    bar: Handle<Mesh>,
    m_me: Handle<StandardMaterial>,
    m_player: Handle<StandardMaterial>,
    m_enemy: Handle<StandardMaterial>,
    m_wildlife: Handle<StandardMaterial>,
    m_trunk: Handle<StandardMaterial>,
    m_canopy: Handle<StandardMaterial>,
    m_rock: Handle<StandardMaterial>,
    m_npc: Handle<StandardMaterial>,
    m_bar_bg: Handle<StandardMaterial>,
    m_bar_hp: Handle<StandardMaterial>,
}

/// Height of a character's health bar above the ground.
const BAR_HEIGHT: f32 = 52.0;
const BAR_WIDTH: f32 = 34.0;

/// The two ability keys per class (action-bar slots 1 and 2).
fn class_abilities(class: Class) -> [&'static str; 2] {
    match class {
        Class::Warrior => ["heroic_strike", "whirlwind"],
        Class::Hunter => ["aimed_shot", "multi_shot"],
        Class::Priest => ["smite", "heal"],
        Class::Mage => ["firebolt", "frost_nova"],
    }
}

// ─── Systems ─────────────────────────────────────────────────────────────────

fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    commands.spawn((
        Camera3d::default(),
        Transform::from_xyz(0.0, 300.0, 420.0).looking_at(Vec3::ZERO, Vec3::Y),
        MainCamera,
    ));

    // Sun.
    commands.spawn((
        DirectionalLight { illuminance: 12_000.0, shadows_enabled: false, ..default() },
        Transform::from_rotation(Quat::from_euler(EulerRot::XYZ, -0.9, 0.6, 0.0)),
    ));

    // Ground plane.
    commands.spawn((
        Mesh3d(meshes.add(Plane3d::default().mesh().size(4200.0, 4200.0))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgb(0.24, 0.42, 0.20),
            perceptual_roughness: 1.0,
            ..default()
        })),
        Transform::from_xyz(0.0, 0.0, 0.0),
    ));

    // Inn ring at the zone entry (the rest / auction-house area).
    commands.spawn((
        Mesh3d(meshes.add(Cylinder::new(220.0, 0.6))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgba(0.95, 0.82, 0.30, 0.35),
            alpha_mode: AlphaMode::Blend,
            unlit: true,
            ..default()
        })),
        Transform::from_xyz(0.0, 0.4, 0.0),
    ));

    let assets = RenderAssets {
        torso: meshes.add(Cuboid::new(16.0, 20.0, 9.0)),
        head: meshes.add(Sphere::new(7.0)),
        arm: meshes.add(Cuboid::new(5.0, 18.0, 5.0)),
        leg: meshes.add(Cuboid::new(6.0, 20.0, 6.0)),
        beast: meshes.add(Sphere::new(9.0)),
        trunk: meshes.add(Cylinder::new(4.5, 26.0)),
        canopy: meshes.add(Cone { radius: 17.0, height: 34.0 }),
        rock: meshes.add(Sphere::new(11.0)),
        nose: meshes.add(Cuboid::new(10.0, 4.0, 4.0)),
        bar: meshes.add(Rectangle::new(1.0, 4.0)),
        m_me: materials.add(Color::srgb(0.25, 0.55, 0.95)),
        m_player: materials.add(Color::srgb(0.35, 0.80, 0.45)),
        m_enemy: materials.add(Color::srgb(0.80, 0.20, 0.18)),
        m_wildlife: materials.add(Color::srgb(0.72, 0.60, 0.38)),
        m_trunk: materials.add(Color::srgb(0.38, 0.26, 0.14)),
        m_canopy: materials.add(Color::srgb(0.12, 0.42, 0.16)),
        m_rock: materials.add(Color::srgb(0.52, 0.52, 0.55)),
        m_npc: materials.add(Color::srgb(0.90, 0.82, 0.30)),
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
    };
    commands.insert_resource(assets);

    // HUD line, top-left.
    commands.spawn((
        Text::new("Connecting…"),
        TextFont { font_size: 18.0, ..default() },
        TextColor(Color::srgb(0.95, 0.95, 0.90)),
        Node {
            position_type: PositionType::Absolute,
            top: Val::Px(12.0),
            left: Val::Px(14.0),
            ..default()
        },
        HudText,
    ));

    // Action bar, bottom-center.
    commands.spawn((
        Text::new(""),
        TextFont { font_size: 17.0, ..default() },
        TextColor(Color::srgb(0.98, 0.90, 0.55)),
        Node {
            position_type: PositionType::Absolute,
            bottom: Val::Px(16.0),
            left: Val::Px(14.0),
            ..default()
        },
        ActionBarText,
    ));
}

/// Spawn the 3D rig for a snapshot entity. Returns (root, health-bar fill).
fn spawn_visual(
    commands: &mut Commands,
    assets: &RenderAssets,
    e: &EntityState,
    is_me: bool,
) -> Mirrored {
    let pos = Vec3::new(e.x, 0.0, e.y);
    let rot = Quat::from_rotation_y(-e.rot);
    let mut bar_fill = None;
    let mut model = None;

    // Root carries translation only; the model child carries facing, so the
    // health bar (also a root child) can billboard independently.
    let root = commands
        .spawn((Transform::from_translation(pos), Visibility::default(), ServerEnt(e.id)))
        .id();
    match e.kind {
        EntityKind::Player | EntityKind::Enemy | EntityKind::Npc => {
            let mat = match e.kind {
                EntityKind::Player if is_me => assets.m_me.clone(),
                EntityKind::Player => assets.m_player.clone(),
                EntityKind::Enemy => assets.m_enemy.clone(),
                _ => assets.m_npc.clone(),
            };
            // Articulated humanoid: torso + head + four limbs pivoted at
            // shoulder/hip so a walk cycle can swing them.
            let mut m = Entity::PLACEHOLDER;
            let (mut al, mut ar, mut ll, mut lr) =
                (Entity::PLACEHOLDER, Entity::PLACEHOLDER, Entity::PLACEHOLDER, Entity::PLACEHOLDER);
            commands.entity(root).with_children(|p| {
                m = p
                    .spawn((Transform::default().with_rotation(rot), Visibility::default()))
                    .with_children(|body| {
                        body.spawn((Mesh3d(assets.torso.clone()), MeshMaterial3d(mat.clone()), Transform::from_xyz(0.0, 34.0, 0.0)));
                        body.spawn((Mesh3d(assets.head.clone()), MeshMaterial3d(mat.clone()), Transform::from_xyz(0.0, 51.0, 0.0)));
                        // A small nose block so facing is visible.
                        body.spawn((Mesh3d(assets.nose.clone()), MeshMaterial3d(mat.clone()), Transform::from_xyz(7.0, 51.0, 0.0)));
                        // Pivot at the top of each limb; z is sideways in
                        // model space (x is the facing axis).
                        let mut limb = |y: f32, z: f32, mesh: &Handle<Mesh>| {
                            body.spawn((Transform::from_xyz(0.0, y, z), Visibility::default()))
                                .with_children(|holder| {
                                    holder.spawn((
                                        Mesh3d(mesh.clone()),
                                        MeshMaterial3d(mat.clone()),
                                        Transform::from_xyz(0.0, -8.0, 0.0),
                                    ));
                                })
                                .id()
                        };
                        al = limb(42.0, -10.5, &assets.arm);
                        ar = limb(42.0, 10.5, &assets.arm);
                        ll = limb(22.0, -4.5, &assets.leg);
                        lr = limb(22.0, 4.5, &assets.leg);
                    })
                    .id();
            });
            commands
                .entity(root)
                .insert((Limbs { arm_l: al, arm_r: ar, leg_l: ll, leg_r: lr }, Walker::default()));
            model = Some(m);
        }
        EntityKind::Wildlife => {
            let mut m = Entity::PLACEHOLDER;
            commands.entity(root).with_children(|p| {
                m = p
                    .spawn((Transform::default().with_rotation(rot), Visibility::default()))
                    .with_children(|body| {
                        body.spawn((Mesh3d(assets.beast.clone()), MeshMaterial3d(assets.m_wildlife.clone()), Transform::from_xyz(0.0, 9.0, 0.0)));
                    })
                    .id();
            });
            model = Some(m);
        }
        EntityKind::Resource => {
            if e.tag.as_deref() == Some("rock") {
                commands.entity(root).with_children(|p| {
                    p.spawn((
                        Mesh3d(assets.rock.clone()),
                        MeshMaterial3d(assets.m_rock.clone()),
                        Transform::from_xyz(0.0, 6.0, 0.0).with_scale(Vec3::new(1.4, 0.8, 1.1)),
                    ));
                });
            } else {
                commands.entity(root).with_children(|p| {
                    p.spawn((Mesh3d(assets.trunk.clone()), MeshMaterial3d(assets.m_trunk.clone()), Transform::from_xyz(0.0, 13.0, 0.0)));
                    p.spawn((Mesh3d(assets.canopy.clone()), MeshMaterial3d(assets.m_canopy.clone()), Transform::from_xyz(0.0, 43.0, 0.0)));
                });
            }
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
    Mirrored { root, model, bar_fill }
}

/// Drain server messages, reconcile the entity set, update the HUD.
fn receive_from_server(
    mut commands: Commands,
    mut rx: NonSendMut<NetRx>,
    mut map: ResMut<EntityMap>,
    mut session: ResMut<Session>,
    assets: Res<RenderAssets>,
    mut transforms: Query<&mut Transform>,
    mut hud: Query<&mut Text, (With<HudText>, Without<ActionBarText>)>,
    mut bar: Query<&mut Text, (With<ActionBarText>, Without<HudText>)>,
) {
    let mut latest: Option<Vec<EntityState>> = None;
    while let Ok(msg) = rx.0.try_recv() {
        match msg {
            ServerMsg::Welcome { entity_id, character } => {
                session.my_id = Some(entity_id);
                session.class = character.class;
                session.hud = format!(
                    "{} — Lv {}  HP {}/{}  MP {}/{}  {}g  in {}",
                    character.name, character.level, character.health, character.max_health,
                    character.mana, character.max_mana, character.gold, character.act.as_str()
                );
            }
            ServerMsg::Stats { character } => {
                session.class = character.class;
                session.hud = format!(
                    "{} — Lv {}  HP {}/{}  MP {}/{}  XP {}/{}  {}g{}  in {}",
                    character.name, character.level, character.health, character.max_health,
                    character.mana, character.max_mana, character.xp, character.max_xp,
                    character.gold,
                    if character.pvp { "  [PvP]" } else { "" },
                    character.act.as_str()
                );
            }
            ServerMsg::LoginRejected { reason } => {
                session.notice = format!("login rejected: {reason}");
            }
            ServerMsg::Notice { text } => session.notice = text,
            ServerMsg::Chat { from, text } => session.notice = format!("{from}: {text}"),
            ServerMsg::Snapshot { entities, .. } => latest = Some(entities),
            ServerMsg::GuildInfo { name, members } => {
                session.notice = format!("<{name}>: {}", members.join(", "));
            }
            ServerMsg::Auctions { listings } => {
                session.notice = format!("{} auction lots", listings.len());
            }
            ServerMsg::Pong => {}
        }
    }

    if let Some(entities) = latest {
        let my_id = session.my_id;
        let mut seen: HashSet<u64> = HashSet::with_capacity(entities.len());
        for e in &entities {
            seen.insert(e.id);
            let is_me = Some(e.id) == my_id;
            match map.0.get(&e.id) {
                Some(m) => {
                    if let Ok(mut t) = transforms.get_mut(m.root) {
                        t.translation.x = e.x;
                        t.translation.z = e.y;
                    }
                    if let Some(model) = m.model {
                        if let Ok(mut t) = transforms.get_mut(model) {
                            t.rotation = Quat::from_rotation_y(-e.rot);
                        }
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
                    let m = spawn_visual(&mut commands, &assets, e, is_me);
                    map.0.insert(e.id, m);
                }
            }
        }
        // Despawn entities that left the AoI / zone / died.
        let gone: Vec<u64> = map.0.keys().copied().filter(|id| !seen.contains(id)).collect();
        for id in gone {
            if let Some(m) = map.0.remove(&id) {
                commands.entity(m.root).despawn_recursive();
            }
        }
    }

    if let Ok(mut text) = hud.get_single_mut() {
        let base = if session.hud.is_empty() {
            format!("{} — connecting…", session.name)
        } else {
            session.hud.clone()
        };
        **text = if session.notice.is_empty() {
            base
        } else {
            format!("{base}\n{}", session.notice)
        };
    }
    if let Ok(mut text) = bar.get_single_mut() {
        **text = match session.class {
            Some(c) => {
                let [a, b] = class_abilities(c);
                format!("[Space] attack   [1] {a}   [2] {b}   [E] talk   |  {}  |  right-drag orbit · scroll zoom", c.as_str())
            }
            None => "No class — press F1 warrior · F2 hunter · F3 priest · F4 mage".into(),
        };
    }
}

/// Read the keyboard and send movement intent + attacks + casts. Movement is
/// camera-relative (WoW-style) and only sent when the direction changes.
fn send_input(
    keys: Res<ButtonInput<KeyCode>>,
    tx: Res<NetTx>,
    orbit: Res<Orbit>,
    session: Res<Session>,
    mut last_dir: Local<(i8, i8)>,
    mut last_yaw: Local<f32>,
) {
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
    }
    if keys.just_pressed(KeyCode::KeyE) {
        tx.send(ClientMsg::Talk);
    }
    if let Some(class) = session.class {
        let [a, b] = class_abilities(class);
        if keys.just_pressed(KeyCode::Digit1) {
            tx.send(ClientMsg::Cast { ability: a.into() });
        }
        if keys.just_pressed(KeyCode::Digit2) {
            tx.send(ClientMsg::Cast { ability: b.into() });
        }
    } else {
        for (key, class) in [
            (KeyCode::F1, Class::Warrior),
            (KeyCode::F2, Class::Hunter),
            (KeyCode::F3, Class::Priest),
            (KeyCode::F4, Class::Mage),
        ] {
            if keys.just_pressed(key) {
                tx.send(ClientMsg::SelectClass { class });
            }
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
    if buttons.pressed(MouseButton::Right) {
        for m in motion.read() {
            orbit.yaw -= m.delta.x * 0.005;
            orbit.pitch = (orbit.pitch + m.delta.y * 0.004).clamp(0.08, 1.35);
        }
    } else {
        motion.clear();
    }
    for w in wheel.read() {
        orbit.dist = (orbit.dist - w.y * 30.0).clamp(140.0, 900.0);
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

/// Swing arms and legs while an entity moves: the walk phase advances with
/// distance travelled, so speed naturally sets the stride rate, and eases back
/// to rest when standing.
fn animate_walk(
    mut walkers: Query<(&Transform, &mut Walker, &Limbs), With<ServerEnt>>,
    mut limbs: Query<&mut Transform, Without<ServerEnt>>,
) {
    for (t, mut w, l) in walkers.iter_mut() {
        let moved = (t.translation - w.last).length();
        w.last = t.translation;
        let swing = if moved > 0.01 {
            w.phase += moved * 0.045;
            (w.phase.sin()) * 0.75
        } else {
            w.phase = 0.0;
            0.0
        };
        for (ent, dir) in [(l.arm_l, 1.0), (l.arm_r, -1.0), (l.leg_l, -1.0), (l.leg_r, 1.0)] {
            if let Ok(mut lt) = limbs.get_mut(ent) {
                // Limbs swing around the sideways (z) axis in model space.
                lt.rotation = Quat::from_rotation_z(swing * dir);
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
